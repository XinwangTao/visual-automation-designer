//! Screen-capture helpers for the image picker.
//!
//! These commands implement the "freeze the desktop, drag a rectangle, keep
//! image + coordinates + ROI" workflow used by the GUI and by AI tooling.

use std::io::Cursor;

use base64::Engine as _;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size, State,
};
use std::time::Duration;

use crate::commands::ImageLibraryState;
use crate::error::{AppError, Result};
use crate::models::{ImageFormat as ModelImageFormat, ImageMetadata};
use crate::platform::ScreenCapture;

/// Frozen desktop kept between "open overlay" and "save/cancel".
///
/// The user picks a rectangle on a frozen image; cropping must use this same
/// image, otherwise the overlay itself would end up inside the capture.
#[derive(Default)]
pub struct ScreenCaptureState {
    /// Frozen desktop shared by the preview, the crop and the cancel path.
    session: Mutex<Option<FrozenDesktop>>,
    /// Main-window geometry to put back when the picker closes.
    window: Mutex<Option<MainWindowRestore>>,
}

impl ScreenCaptureState {
    /// True while the picker owns the desktop (used to keep the window alive).
    pub fn is_capturing(&self) -> bool {
        self.session
            .lock()
            .map(|session| session.is_some())
            .unwrap_or(false)
    }
}

struct FrozenDesktop {
    image: image::DynamicImage,
    origin_x: i32,
    origin_y: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenPreview {
    /// Base64 PNG of the (possibly scaled) virtual desktop.
    pub data: String,
    /// Native virtual-desktop pixel size.
    pub width: u32,
    pub height: u32,
    /// Size of `data` after scaling.
    pub preview_width: u32,
    pub preview_height: u32,
    /// Virtual-screen origin of the captured desktop.
    pub origin_x: i32,
    pub origin_y: i32,
    /// previewWidth / width (1.0 when not scaled).
    pub scale: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedImage {
    pub image: ImageMetadata,
    pub roi: ScreenRegion,
    pub center: ScreenPoint,
}

fn encode_png(image: &image::DynamicImage) -> Result<Vec<u8>> {
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| AppError::ImageError(format!("PNG encode failed: {e}")))?;
    Ok(bytes.into_inner())
}

/// Capture the virtual desktop and return a preview for the in-app picker.
/// Hide the app window while capturing so the frozen image shows the real
/// desktop, not the app's own dialog.
pub(crate) fn capture_without_app<T>(
    app: &AppHandle,
    capture: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let window = app.get_webview_window("main");
    if let Some(window) = window.as_ref() {
        let _ = window.hide();
    }
    std::thread::sleep(Duration::from_millis(200));
    let result = capture();
    if let Some(window) = window {
        let _ = window.show();
        let _ = window.set_focus();
    }
    result
}

#[tauri::command]
pub fn capture_screen_preview(app: AppHandle, max_width: Option<u32>) -> Result<ScreenPreview> {
    let (capture, max_width) = capture_without_app(&app, || {
        Ok((ScreenCapture::capture_virtual_desktop()?, max_width))
    })?;
    let width = capture.image.width();
    let height = capture.image.height();
    let max_width = max_width.unwrap_or(1800).max(320);

    let (preview, preview_width, preview_height, scale) = if width > max_width {
        let scale = max_width as f64 / width as f64;
        let preview_height =
            ((height as f64 * scale).round() as u32).max(1);
        (
            capture
                .image
                .resize_exact(max_width, preview_height, image::imageops::FilterType::Triangle),
            max_width,
            preview_height,
            scale,
        )
    } else {
        (capture.image.clone(), width, height, 1.0)
    };

    let data = base64::engine::general_purpose::STANDARD.encode(encode_png(&preview)?);
    Ok(ScreenPreview {
        data,
        width,
        height,
        preview_width,
        preview_height,
        origin_x: capture.origin_x,
        origin_y: capture.origin_y,
        scale,
    })
}

/// Crop a region from a fresh desktop capture, save it into the image library
/// and return the metadata plus the screen-space ROI/center.
#[tauri::command]
pub fn add_image_from_screen_region(
    app: AppHandle,
    image_library_state: State<'_, ImageLibraryState>,
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<CapturedImage> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::ValidationError("image name is empty".to_string()));
    }
    if width == 0 || height == 0 {
        return Err(AppError::ValidationError(
            "capture region must have a positive size".to_string(),
        ));
    }

    let capture = capture_without_app(&app, || ScreenCapture::capture_virtual_desktop())?;
    let local_x = x - capture.origin_x;
    let local_y = y - capture.origin_y;
    if local_x < 0
        || local_y < 0
        || local_x as u32 + width > capture.image.width()
        || local_y as u32 + height > capture.image.height()
    {
        return Err(AppError::ValidationError(format!(
            "capture region ({x},{y},{width},{height}) is outside the virtual desktop"
        )));
    }

    let cropped = capture
        .image
        .crop_imm(local_x as u32, local_y as u32, width, height);
    let bytes = encode_png(&cropped)?;
    let manager = image_library_state
        .manager
        .lock()
        .map_err(|e| AppError::InternalError(format!("Failed to lock image library: {e}")))?;
    let hash = crate::core::image_library::compute_content_hash(&bytes);
    let metadata = match manager.find_by_hash(&hash)? {
        Some(existing) => existing,
        None => manager.add_image_from_bytes(&bytes, name, ModelImageFormat::Png)?,
    };

    Ok(CapturedImage {
        image: metadata,
        roi: ScreenRegion {
            x,
            y,
            width,
            height,
        },
        center: ScreenPoint {
            x: x + (width / 2) as i32,
            y: y + (height / 2) as i32,
        },
    })
}

// ============================================================================
// Full-screen picker (drawn by the main window)
//
// The picker reuses the main window: it is temporarily made borderless,
// top-most and desktop-sized, and the frontend paints the frozen desktop over
// the whole app. The desktop is captured exactly once, with the main window
// hidden, so the picker itself can never end up inside the saved image.
// ============================================================================

/// Main-window geometry remembered while the picker is open.
struct MainWindowRestore {
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    decorations: bool,
    always_on_top: bool,
    maximized: bool,
    resizable: bool,
}

/// How many times the picker geometry is re-applied before giving up.
const GEOMETRY_ATTEMPTS: usize = 6;

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        // A window hidden while minimized comes back minimized, i.e. invisible.
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn restore_main_window(app: &AppHandle, state: &ScreenCaptureState) {
    let saved = state.window.lock().ok().and_then(|mut slot| slot.take());
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if let Some(saved) = saved {
        let _ = window.unminimize();
        let _ = window.set_always_on_top(saved.always_on_top);
        let _ = window.set_decorations(saved.decorations);
        let _ = window.set_resizable(saved.resizable);

        // A window that was minimized reports (-32000,-32000) and a 0x0 client
        // area, so those values must never be played back.
        let usable = saved.size.width >= 200 && saved.size.height >= 200;
        let on_screen = saved.position.x > -30_000 && saved.position.y > -30_000;
        if usable && on_screen {
            let _ = window.set_position(Position::Physical(saved.position));
            let _ = window.set_size(Size::Physical(saved.size));
            if saved.maximized {
                let _ = window.maximize();
            } else {
                let _ = window.unmaximize();
            }
        } else {
            let _ = window.unmaximize();
            let _ = window.set_size(Size::Physical(PhysicalSize::new(1280, 800)));
            let _ = window.center();
        }
    }
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// Make the main window's *client area* cover exactly the frozen desktop.
///
/// Two things reliably break a naive `set_size`:
/// - a minimized or maximized window silently ignores the resize;
/// - Windows keeps an invisible 8 px resize frame, so the client area ends up
///   inset from the screen origin and every picked coordinate drifts.
///
/// So the frame is disabled, the client rect is read back, and the outer rect
/// is corrected until it matches. `false` means the overlay would be wrong and
/// must not be shown.
fn fit_main_to_desktop(
    window: &tauri::WebviewWindow,
    origin: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) -> bool {
    let _ = window.unminimize();
    let _ = window.unmaximize();
    let _ = window.set_decorations(false);
    // Drops WS_THICKFRAME, which is what leaves the invisible frame behind.
    let _ = window.set_resizable(false);
    let _ = window.set_always_on_top(true);

    let mut outer_position = origin;
    let mut outer_size = size;
    for attempt in 0..GEOMETRY_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(40));
            let _ = window.unminimize();
            let _ = window.unmaximize();
        }
        let _ = window.set_position(Position::Physical(outer_position));
        let _ = window.set_size(Size::Physical(outer_size));
        let _ = window.show();
        let _ = window.set_focus();

        if window.is_minimized().unwrap_or(false) || window.is_maximized().unwrap_or(false) {
            continue;
        }

        let inner_position = match window.inner_position() {
            Ok(position) => position,
            Err(_) => continue,
        };
        let inner_size = match window.inner_size() {
            Ok(size) => size,
            Err(_) => continue,
        };

        let dx = origin.x - inner_position.x;
        let dy = origin.y - inner_position.y;
        let dw = size.width as i64 - inner_size.width as i64;
        let dh = size.height as i64 - inner_size.height as i64;
        if dx == 0 && dy == 0 && dw == 0 && dh == 0 {
            return true;
        }
        outer_position = PhysicalPosition::new(outer_position.x + dx, outer_position.y + dy);
        outer_size = PhysicalSize::new(
            (outer_size.width as i64 + dw).max(1) as u32,
            (outer_size.height as i64 + dh).max(1) as u32,
        );
    }
    false
}

/// Freeze the desktop, remember it, and turn the main window into the picker.
#[tauri::command]
pub fn open_screen_capture(app: AppHandle, state: State<'_, ScreenCaptureState>) -> Result<()> {
    if state
        .session
        .lock()
        .map(|session| session.is_some())
        .unwrap_or(false)
    {
        return Err(AppError::ValidationError(
            "screen capture is already open".to_string(),
        ));
    }

    let main = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::InternalError("main window is missing".to_string()))?;

    let saved = MainWindowRestore {
        position: main.outer_position().unwrap_or(PhysicalPosition::new(0, 0)),
        size: main.inner_size().unwrap_or(PhysicalSize::new(1280, 800)),
        decorations: main.is_decorated().unwrap_or(true),
        always_on_top: main.is_always_on_top().unwrap_or(false),
        maximized: main.is_maximized().unwrap_or(false),
        resizable: main.is_resizable().unwrap_or(true),
    };

    // The desktop must be frozen while the app is off screen.
    let _ = main.hide();
    std::thread::sleep(Duration::from_millis(220));

    let capture = match ScreenCapture::capture_virtual_desktop() {
        Ok(capture) => capture,
        Err(err) => {
            show_main_window(&app);
            return Err(err);
        }
    };
    let width = capture.image.width();
    let height = capture.image.height();
    if width == 0 || height == 0 {
        show_main_window(&app);
        return Err(AppError::InternalError(
            "virtual desktop is empty".to_string(),
        ));
    }

    {
        let mut session = state
            .session
            .lock()
            .map_err(|e| AppError::InternalError(format!("Failed to lock capture state: {e}")))?;
        *session = Some(FrozenDesktop {
            image: capture.image.clone(),
            origin_x: capture.origin_x,
            origin_y: capture.origin_y,
        });
    }
    if let Ok(mut slot) = state.window.lock() {
        *slot = Some(saved);
    }

    if !fit_main_to_desktop(
        &main,
        PhysicalPosition::new(capture.origin_x, capture.origin_y),
        PhysicalSize::new(width, height),
    ) {
        if let Ok(mut session) = state.session.lock() {
            *session = None;
        }
        restore_main_window(&app, &state);
        return Err(AppError::InternalError(
            "无法把主窗口铺满整个桌面，已退出屏幕截取".to_string(),
        ));
    }

    let _ = app.emit("screen-capture-opened", ());
    Ok(())
}

/// Kept for the picker page: the window is already visible, so this only
/// re-focuses it. (Previously it revealed a separate picker window.)
#[tauri::command]
pub fn show_screen_capture_window(app: AppHandle) -> Result<()> {
    show_main_window(&app);
    Ok(())
}

/// 虚拟桌面的边界（物理像素），供图片库画出 ROI 的相对位置。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopBounds {
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
}

#[tauri::command]
pub fn get_desktop_bounds() -> Result<DesktopBounds> {
    let monitors = ScreenCapture::list_monitors()?;
    let first = monitors
        .first()
        .ok_or_else(|| AppError::InternalError("no monitors reported".to_string()))?;
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x + first.width as i32;
    let mut max_y = first.y + first.height as i32;
    for monitor in &monitors[1..] {
        min_x = min_x.min(monitor.x);
        min_y = min_y.min(monitor.y);
        max_x = max_x.max(monitor.x + monitor.width as i32);
        max_y = max_y.max(monitor.y + monitor.height as i32);
    }
    Ok(DesktopBounds {
        origin_x: min_x,
        origin_y: min_y,
        width: (max_x - min_x).max(1) as u32,
        height: (max_y - min_y).max(1) as u32,
    })
}

/// Return a (optionally scaled) PNG of the frozen desktop for the picker.
#[tauri::command]
pub fn get_screen_capture_preview(
    state: State<'_, ScreenCaptureState>,
    max_width: Option<u32>,
) -> Result<ScreenPreview> {
    let session = state
        .session
        .lock()
        .map_err(|e| AppError::InternalError(format!("Failed to lock capture state: {e}")))?;
    let frozen = session
        .as_ref()
        .ok_or_else(|| AppError::InternalError("no frozen desktop capture".to_string()))?;

    let width = frozen.image.width();
    let height = frozen.image.height();
    let max_width = max_width.unwrap_or(1920).max(320);
    let (preview, preview_width, preview_height, scale) = if width > max_width {
        let scale = max_width as f64 / width as f64;
        let preview_height = ((height as f64 * scale).round() as u32).max(1);
        (
            frozen.image.resize_exact(
                max_width,
                preview_height,
                image::imageops::FilterType::Triangle,
            ),
            max_width,
            preview_height,
            scale,
        )
    } else {
        (frozen.image.clone(), width, height, 1.0)
    };

    let data = base64::engine::general_purpose::STANDARD.encode(encode_png(&preview)?);
    Ok(ScreenPreview {
        data,
        width,
        height,
        preview_width,
        preview_height,
        origin_x: frozen.origin_x,
        origin_y: frozen.origin_y,
        scale,
    })
}

/// Crop the frozen desktop (coordinates are relative to the frozen image),
/// save it into the image library, restore the main window and announce it.
#[tauri::command]
pub fn save_screen_capture(
    app: AppHandle,
    state: State<'_, ScreenCaptureState>,
    image_library_state: State<'_, ImageLibraryState>,
    name: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> Result<CapturedImage> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::ValidationError("image name is empty".to_string()));
    }
    if width == 0 || height == 0 {
        return Err(AppError::ValidationError(
            "capture region must have a positive size".to_string(),
        ));
    }

    // 先把活干完再取走会话：任一环节失败都要么能重试，要么能干净退出。
    // 之前的实现先 take() 再校验，越界/写盘失败会让窗口永远停在无边框全屏，
    // 而且 Alt+F4 会直接退出 app（is_capturing 已经为 false）。
    let result = (|| -> Result<CapturedImage> {
        let (image_width, image_height, origin_x, origin_y) = {
            let session = state
                .session
                .lock()
                .map_err(|e| AppError::InternalError(format!("Failed to lock capture state: {e}")))?;
            let frozen = session
                .as_ref()
                .ok_or_else(|| AppError::InternalError("no frozen desktop capture".to_string()))?;
            (
                frozen.image.width(),
                frozen.image.height(),
                frozen.origin_x,
                frozen.origin_y,
            )
        };

        let (ux, uy) = (x.max(0) as u32, y.max(0) as u32);
        if x < 0
            || y < 0
            || width > image_width.saturating_sub(ux)
            || height > image_height.saturating_sub(uy)
        {
            return Err(AppError::ValidationError(format!(
                "capture region ({x},{y},{width},{height}) is outside the frozen desktop"
            )));
        }

        let cropped = {
            let session = state
                .session
                .lock()
                .map_err(|e| AppError::InternalError(format!("Failed to lock capture state: {e}")))?;
            let frozen = session
                .as_ref()
                .ok_or_else(|| AppError::InternalError("no frozen desktop capture".to_string()))?;
            frozen.image.crop_imm(ux, uy, width, height)
        };
        let bytes = encode_png(&cropped)?;
        let manager = image_library_state
            .manager
            .lock()
            .map_err(|e| AppError::InternalError(format!("Failed to lock image library: {e}")))?;
        // 同一块屏幕区域重复截取很常见：库里已有同图（内容哈希一致）时直接复用，
        // 只更新 ROI。元数据还在但文件被外部删掉时，先清掉悬空条目再加。
        let hash = crate::core::image_library::compute_content_hash(&bytes);
        let mut metadata = match manager.find_by_hash(&hash)? {
            Some(existing) => match manager.image_bytes_match(&existing.id, &bytes)? {
                Some(true) => existing,
                Some(false) => manager.add_image_from_bytes_distinct(
                    &bytes,
                    name.clone(),
                    ModelImageFormat::Png,
                )?,
                None => {
                    let _ = manager.remove_image(&existing.id);
                    manager.add_image_from_bytes(&bytes, name.clone(), ModelImageFormat::Png)?
                }
            },
            None => manager.add_image_from_bytes(&bytes, name, ModelImageFormat::Png)?,
        };
        let screen_x = origin_x + x;
        let screen_y = origin_y + y;
        let region = crate::models::Region {
            x: screen_x,
            y: screen_y,
            width,
            height,
        };
        // 把这次的屏幕位置写进图片本身：图片库能可视化编辑，块没设 ROI 时也用它。
        manager.set_image_roi(&metadata.id, Some(region))?;
        metadata.set_roi(Some(region));
        drop(manager);

        Ok(CapturedImage {
            image: metadata,
            roi: ScreenRegion {
                x: screen_x,
                y: screen_y,
                width,
                height,
            },
            center: ScreenPoint {
                x: screen_x + (width / 2) as i32,
                y: screen_y + (height / 2) as i32,
            },
        })
    })();

    // 无论成功失败都收尾：清空会话、恢复窗口、通知前端。
    if let Ok(mut session) = state.session.lock() {
        *session = None;
    }
    restore_main_window(&app, &state);

    // 必须先发 saved 再发 closed：抽屉里的「重设 ROI」监听靠 closed 复位意图标记，
    // 先发 closed 会让随后到达的 saved 被图片选择器当成「换图」，把积木块绑到
    // 那张即将被删除的临时图上。
    let captured = result?;
    let _ = app.emit("screen-capture-saved", &captured);
    let _ = app.emit("screen-capture-closed", ());
    Ok(captured)
}

/// Close the picker without saving; the main window comes back.
#[tauri::command]
pub fn cancel_screen_capture(app: AppHandle, state: State<'_, ScreenCaptureState>) -> Result<()> {
    if let Ok(mut session) = state.session.lock() {
        *session = None;
    }
    restore_main_window(&app, &state);
    let _ = app.emit("screen-capture-closed", ());
    Ok(())
}
