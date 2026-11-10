//! Windows UI Automation target discovery and click safety.
//!
//! Adapted from `mouse-recorder` (MIT License, 2026 mouse-recorder
//! contributors), `src/cli/uia.rs`. The selector semantics, bounded tree walk,
//! raise sequence and `WindowFromPoint` guard are kept identical so the two
//! tools behave the same on the same desktop.
//!
//! Windows-only: `platform::uia` is compiled only on Windows.

use serde::Serialize;
use windows::core::Result as WResult;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
    IUIAutomationValuePattern, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetAncestor, GetWindowTextLengthW, GetWindowTextW, GetWindowRect, IsIconic,
    IsWindowVisible, WindowFromPoint, GA_ROOT,
};

/// A UIA control with its screen-space bounding rectangle.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiaNode {
    pub name: String,
    /// Current ValuePattern text (edits, combo boxes); `None` for other controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    pub automation_id: String,
    pub class_name: String,
    pub control_type: i32,
    pub control_type_name: &'static str,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub enabled: bool,
    pub offscreen: bool,
    pub depth: usize,
}

impl UiaNode {
    pub fn cx(&self) -> i32 {
        self.x + self.w / 2
    }

    pub fn cy(&self) -> i32 {
        self.y + self.h / 2
    }

    pub fn rect(&self) -> (i32, i32, u32, u32) {
        (self.x, self.y, self.w.max(0) as u32, self.h.max(0) as u32)
    }
}

/// Selector semantics:
/// - `name`: case-insensitive substring
/// - `automation_id` / `class_name` / `control_type`: exact, case-insensitive
/// - `index`: zero-based pick among matches, applied after filtering
#[derive(Debug, Clone, Default)]
pub struct UiaSelector {
    pub name: Option<String>,
    pub automation_id: Option<String>,
    pub class_name: Option<String>,
    pub control_type: Option<String>,
    pub index: Option<usize>,
}

impl UiaSelector {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.automation_id.is_none()
            && self.class_name.is_none()
            && self.control_type.is_none()
    }

    fn matches(&self, n: &UiaNode) -> bool {
        let name_ok = self
            .name
            .as_ref()
            .map(|q| n.name.to_lowercase().contains(&q.to_lowercase()))
            .unwrap_or(true);
        let id_ok = self
            .automation_id
            .as_ref()
            .map(|q| n.automation_id.eq_ignore_ascii_case(q))
            .unwrap_or(true);
        let class_ok = self
            .class_name
            .as_ref()
            .map(|q| n.class_name.eq_ignore_ascii_case(q))
            .unwrap_or(true);
        let type_ok = self
            .control_type
            .as_ref()
            .map(|q| n.control_type_name.eq_ignore_ascii_case(q))
            .unwrap_or(true);
        name_ok && id_ok && class_ok && type_ok
    }
}

/// Enable per-monitor-v2 DPI awareness so UIA/GDI rectangles are physical
/// pixels and match the coordinates used by screen capture and SendInput.
pub fn enable_dpi_awareness() {
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn control_type_name(id: i32) -> &'static str {
    match id {
        50000 => "Button",
        50001 => "Calendar",
        50002 => "CheckBox",
        50003 => "ComboBox",
        50004 => "Edit",
        50005 => "Hyperlink",
        50006 => "Image",
        50007 => "ListItem",
        50008 => "List",
        50009 => "Menu",
        50010 => "MenuBar",
        50011 => "MenuItem",
        50012 => "ProgressBar",
        50013 => "RadioButton",
        50014 => "ScrollBar",
        50015 => "Slider",
        50016 => "Spinner",
        50017 => "StatusBar",
        50018 => "Tab",
        50019 => "TabItem",
        50020 => "Text",
        50021 => "ToolBar",
        50022 => "ToolTip",
        50023 => "Tree",
        50024 => "TreeItem",
        50025 => "Custom",
        50026 => "Group",
        50027 => "Thumb",
        50028 => "DataGrid",
        50029 => "DataItem",
        50030 => "Document",
        50031 => "SplitButton",
        50032 => "Window",
        50033 => "Pane",
        50034 => "Header",
        50035 => "HeaderItem",
        50036 => "Table",
        50037 => "TitleBar",
        50038 => "Separator",
        50039 => "SemanticZoom",
        50040 => "AppBar",
        _ => "Unknown",
    }
}

unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let out = &mut *(lparam.0 as *mut Vec<HWND>);
    if IsWindowVisible(hwnd).as_bool() {
        out.push(hwnd);
    }
    BOOL(1)
}

fn window_title(hwnd: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let mut buf = vec![0u16; (len + 1) as usize];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn visible_windows() -> Vec<HWND> {
    let mut windows: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(enum_window), LPARAM(&mut windows as *mut _ as isize));
    }
    windows
}

/// Find the first visible, non-minimized top-level window whose title contains
/// `title` (case-insensitive substring).
pub fn find_window(title: &str) -> Result<HWND, String> {
    let q = title.trim().to_lowercase();
    if q.is_empty() {
        return Err("window title is empty".into());
    }
    for hwnd in visible_windows() {
        let t = window_title(hwnd);
        if !t.is_empty() && t.to_lowercase().contains(&q) {
            return Ok(hwnd);
        }
    }
    Err(format!("未找到窗口: {title}"))
}

/// A visible top-level window with its screen rectangle.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub title: String,
    pub pid: u32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub minimized: bool,
    pub foreground: bool,
}

/// Enumerate visible top-level windows with process id and screen rect.
pub fn list_windows() -> Vec<WindowInfo> {
    let mut out = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_window_info),
            LPARAM(&mut out as *mut Vec<WindowInfo> as isize),
        );
    }

    out
}

unsafe extern "system" fn enum_window_info(hwnd: HWND, lparam: LPARAM) -> BOOL {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let title = window_title(hwnd);
    if title.is_empty() {
        return BOOL(1);
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let mut rect = RECT::default();
    let has_rect = unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok();
    let fg = unsafe { GetForegroundWindow() };
    let out = &mut *(lparam.0 as *mut Vec<WindowInfo>);
    out.push(WindowInfo {
        title,
        pid,
        x: if has_rect { rect.left } else { 0 },
        y: if has_rect { rect.top } else { 0 },
        w: if has_rect { rect.right - rect.left } else { 0 },
        h: if has_rect { rect.bottom - rect.top } else { 0 },
        minimized: unsafe { IsIconic(hwnd) }.as_bool(),
        foreground: fg == hwnd,
    });
    BOOL(1)
}

/// List visible top-level window titles (for `vadctl doctor` / diagnostics).
pub fn list_window_titles() -> Vec<String> {
    visible_windows()
        .into_iter()
        .map(window_title)
        .filter(|t| !t.is_empty())
        .collect()
}

/// Whether the matching window is minimized.
pub fn window_is_minimized(title: &str) -> bool {
    match find_window(title) {
        Ok(hwnd) => unsafe { IsIconic(hwnd) }.as_bool(),
        Err(_) => false,
    }
}

/// Whether `title` currently owns the foreground window.
pub fn is_foreground_window(title: &str) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let Ok(hwnd) = find_window(title) else {
        return false;
    };
    unsafe { GetForegroundWindow() == hwnd }
}

/// Bounding rectangle of a window in virtual-screen physical pixels.
pub fn window_rect(title: &str) -> Result<(i32, i32, i32, i32), String> {
    let hwnd = find_window(title)?;
    let mut r = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut r) }.map_err(|e| format!("GetWindowRect: {e}"))?;
    let w = r.right - r.left;
    let h = r.bottom - r.top;
    if w <= 0 || h <= 0 {
        return Err(format!("窗口矩形无效: {title}"));
    }
    Ok((r.left, r.top, w, h))
}

fn read_node(el: &IUIAutomationElement, depth: usize) -> UiaNode {
    let name = unsafe { el.CurrentName() }
        .map(|b| b.to_string())
        .unwrap_or_default();
    let automation_id = unsafe { el.CurrentAutomationId() }
        .map(|b| b.to_string())
        .unwrap_or_default();
    let class_name = unsafe { el.CurrentClassName() }
        .map(|b| b.to_string())
        .unwrap_or_default();
    let control_type = unsafe { el.CurrentControlType() }
        .map(|c| c.0)
        .unwrap_or(0);
    let (x, y, w, h) = match unsafe { el.CurrentBoundingRectangle() } {
        Ok(r) => (r.left, r.top, r.right - r.left, r.bottom - r.top),
        Err(_) => (0, 0, 0, 0),
    };
    // ValuePattern is a cross-process COM call; only ask controls that
    // actually expose a value (Edit / ComboBox / Document).
    let value = if matches!(control_type, 50003 | 50004 | 50030) {
        unsafe {
            el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .ok()
                .and_then(|pattern| pattern.CurrentValue().ok())
                .map(|text| text.to_string())
                .filter(|text| !text.is_empty())
        }
    } else {
        None
    };
    let enabled = unsafe { el.CurrentIsEnabled() }
        .map(|b| b.as_bool())
        .unwrap_or(true);
    let offscreen = unsafe { el.CurrentIsOffscreen() }
        .map(|b| b.as_bool())
        .unwrap_or(false);
    UiaNode {
        name,
        value,
        automation_id,
        class_name,
        control_type,
        control_type_name: control_type_name(control_type),
        x,
        y,
        w,
        h,
        enabled,
        offscreen,
        depth,
    }
}

fn walk(
    walker: &IUIAutomationTreeWalker,
    parent: &IUIAutomationElement,
    level: usize,
    max_depth: usize,
    max_nodes: usize,
    selector: &Option<UiaSelector>,
    stop_after: Option<usize>,
    out: &mut Vec<UiaNode>,
) {
    if level > max_depth || out.len() >= max_nodes {
        return;
    }
    if stop_after.map(|limit| out.len() >= limit).unwrap_or(false) {
        return;
    }
    let mut child: WResult<IUIAutomationElement> =
        unsafe { walker.GetFirstChildElement(parent) };
    while let Ok(el) = child {
        if out.len() >= max_nodes
            || stop_after.map(|limit| out.len() >= limit).unwrap_or(false)
        {
            break;
        }
        let node = read_node(&el, level);
        let matched = selector
            .as_ref()
            .map(|s| s.matches(&node))
            .unwrap_or(true);
        if matched && !node.offscreen && node.w > 0 && node.h > 0 {
            out.push(node);
            if stop_after.map(|limit| out.len() >= limit).unwrap_or(false) {
                return;
            }
        }
        // Recurse even when this node is filtered out: the selector may target
        // a descendant of an unmatched container.
        walk(
            walker,
            &el,
            level + 1,
            max_depth,
            max_nodes,
            selector,
            stop_after,
            out,
        );
        child = unsafe { walker.GetNextSiblingElement(&el) };
    }
}

fn automation() -> Result<IUIAutomation, String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }
        .map_err(|e| format!("CoCreateInstance(UIAutomation): {e}"))
}

/// Walk the UIA control tree of `window`, filtered by `selector`.
///
/// Bounded by `max_nodes` and `max_depth` so a pathological tree cannot hang
/// the caller.
pub fn dump(
    window: &str,
    max_nodes: usize,
    max_depth: usize,
    selector: Option<UiaSelector>,
) -> Result<Vec<UiaNode>, String> {
    dump_internal(window, max_nodes, max_depth, selector, None)
}

fn dump_internal(
    window: &str,
    max_nodes: usize,
    max_depth: usize,
    selector: Option<UiaSelector>,
    stop_after: Option<usize>,
) -> Result<Vec<UiaNode>, String> {
    let hwnd = find_window(window)?;
    let automation = automation()?;
    let root = unsafe { automation.ElementFromHandle(hwnd) }
        .map_err(|e| format!("ElementFromHandle: {e}"))?;
    let walker = unsafe { automation.ControlViewWalker() }
        .map_err(|e| format!("ControlViewWalker: {e}"))?;
    let mut out = Vec::new();
    walk(
        &walker,
        &root,
        0,
        max_depth,
        max_nodes,
        &selector,
        stop_after,
        &mut out,
    );
    if let Some(idx) = selector.as_ref().and_then(|s| s.index) {
        if idx >= out.len() {
            return Ok(Vec::new());
        }
        out = vec![out[idx].clone()];
    }
    Ok(out)
}

/// Find the first control matching `selector`.
pub fn find(window: &str, selector: &UiaSelector) -> Result<Option<UiaNode>, String> {
    if selector.is_empty() {
        return Err(
            "UIA selector is empty: provide name / automation-id / class / control-type".into(),
        );
    }
    let stop_after = Some(selector.index.map(|index| index + 1).unwrap_or(1));
    let nodes = dump_internal(window, 400, 12, Some(selector.clone()), stop_after)?;
    Ok(nodes.into_iter().next())
}

pub fn window_exists(title: &str) -> bool {
    find_window(title).is_ok()
}

/// Raise `title` to the foreground so synthetic clicks land on it.
///
/// UIA can read a covered window; SendInput cannot reach it. Windows only lets
/// the foreground process steal focus, so attach to the current foreground
/// thread first, then fall back to a no-activate z-order bump.
pub fn raise_window(title: &str) -> Result<(), String> {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
        SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SWP_SHOWWINDOW, SW_RESTORE,
    };
    let hwnd = find_window(title)?;
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if GetForegroundWindow() == hwnd {
            return Ok(());
        }
        let fg = GetForegroundWindow();
        let fg_tid = GetWindowThreadProcessId(fg, None);
        let me = GetCurrentThreadId();
        let attached =
            fg_tid != 0 && fg_tid != me && AttachThreadInput(me, fg_tid, BOOL(1)).as_bool();
        let _ = BringWindowToTop(hwnd);
        let activated = SetForegroundWindow(hwnd).as_bool();
        if attached {
            let _ = AttachThreadInput(me, fg_tid, BOOL(0));
        }
        if !activated || GetForegroundWindow() != hwnd {
            // Best effort: at least put it on top so a click reaches it.
            let _ = SetWindowPos(
                hwnd,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
    // Give the compositor a moment to finish the z-order change; otherwise
    // WindowFromPoint still reports the old top window.
    std::thread::sleep(std::time::Duration::from_millis(150));
    Ok(())
}

/// `Ok(None)` when the point really hits `title`'s window, otherwise the title
/// of the window that would receive the click.
pub fn occluder_at(title: &str, x: i32, y: i32) -> Result<Option<String>, String> {
    let hwnd = find_window(title)?;
    let hit = unsafe { WindowFromPoint(POINT { x, y }) };
    if hit.0.is_null() {
        return Ok(Some("<无窗口>".to_string()));
    }
    let root = unsafe { GetAncestor(hit, GA_ROOT) };
    if hit == hwnd || root == hwnd {
        return Ok(None);
    }
    let name = window_title(root);
    Ok(Some(if name.is_empty() {
        "<无标题窗口>".to_string()
    } else {
        name
    }))
}

/// Read-only inspection used by dry-run: resolve the control, reject disabled
/// controls, and report an occluder without raising the window.
pub fn inspect(window: &str, selector: &UiaSelector, force: bool) -> Result<UiaNode, String> {
    if selector.is_empty() {
        return Err("UIA selector is empty: provide name / automation-id / class / control-type".into());
    }
    let node = find(window, selector)?
        .ok_or_else(|| format!("未找到 UIA 控件: window={window:?} selector={selector:?}"))?;
    if !node.enabled {
        return Err(format!(
            "UIA 控件已禁用: {} \"{}\"",
            node.control_type_name, node.name
        ));
    }
    if !force {
        if let Some(occluder) = occluder_at(window, node.cx(), node.cy())? {
            return Err(format!(
                "UIA 控件被遮挡: {} \"{}\" 被窗口 {:?} 覆盖",
                node.control_type_name, node.name, occluder
            ));
        }
    }
    Ok(node)
}

/// Prepare a guarded UIA click.
///
/// Order matters: raise the window first, then re-read the control rect (the
/// window may move while activating), then verify the point is not covered.
/// Returns the refreshed node so the caller clicks fresh coordinates.
pub fn prepare_click(
    window: &str,
    selector: &UiaSelector,
    force: bool,
) -> Result<UiaNode, String> {
    if selector.is_empty() {
        return Err("UIA click requires at least one selector (name/automation-id/class/control-type)".into());
    }
    raise_window(window)?;
    let node = find(window, selector)?
        .ok_or_else(|| format!("未找到 UIA 控件: window={window:?} selector={selector:?}"))?;
    if !node.enabled {
        return Err(format!(
            "UIA 控件已禁用: {} \"{}\"",
            node.control_type_name, node.name
        ));
    }
    let (cx, cy) = (node.cx(), node.cy());
    if !force {
        if let Some(occluder) = occluder_at(window, cx, cy)? {
            return Err(format!(
                "UIA 控件被遮挡: {} \"{}\" @({cx},{cy}) 被窗口 {:?} 覆盖。移动该窗口，或传 --force 跳过校验。",
                node.control_type_name, node.name, occluder
            ));
        }
    }
    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_matches_case_insensitive_substring_for_name() {
        let sel = UiaSelector {
            name: Some("确定".into()),
            ..Default::default()
        };
        let mut node = UiaNode {
            name: "确定按钮".into(),
            value: None,
            automation_id: String::new(),
            class_name: String::new(),
            control_type: 50000,
            control_type_name: "Button",
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            enabled: true,
            offscreen: false,
            depth: 1,
        };
        assert!(sel.matches(&node));
        node.name = "取消".into();
        assert!(!sel.matches(&node));
    }

    #[test]
    fn selector_requires_exact_id_and_type() {
        let sel = UiaSelector {
            automation_id: Some("submitBtn".into()),
            control_type: Some("button".into()),
            ..Default::default()
        };
        let node = UiaNode {
            name: String::new(),
            value: None,
            automation_id: "SUBMITBTN".into(),
            class_name: String::new(),
            control_type: 50000,
            control_type_name: "Button",
            x: 0,
            y: 0,
            w: 10,
            h: 10,
            enabled: true,
            offscreen: false,
            depth: 1,
        };
        assert!(sel.matches(&node));
    }

    #[test]
    fn empty_selector_is_rejected_for_inspection() {
        let err = inspect("whatever", &UiaSelector::default(), false).unwrap_err();
        assert!(err.contains("selector"));
    }

    #[test]
    fn empty_selector_is_rejected_for_clicks() {
        let err = prepare_click("whatever", &UiaSelector::default(), false).unwrap_err();
        assert!(err.contains("selector"));
    }
}
