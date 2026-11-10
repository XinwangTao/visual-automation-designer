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
fn capture_without_app<T>(
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
    let metadata = manager.add_image_from_bytes(&bytes, name, ModelImageFormat::Png)?;

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
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
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
        let _ = window.set_always_on_top(saved.always_on_top);
        let _ = window.set_decorations(saved.decorations);
        let _ = window.set_position(Position::Physical(saved.position));
        let _ = window.set_size(Size::Physical(saved.size));
    }
    let _ = window.show();
    let _ = window.set_focus();
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

    let _ = main.unmaximize();
    let _ = main.set_decorations(false);
    let _ = main.set_always_on_top(true);
    let _ = main.set_position(Position::Physical(PhysicalPosition::new(
        capture.origin_x,
        capture.origin_y,
    )));
    let _ = main.set_size(Size::Physical(PhysicalSize::new(width, height)));
    let _ = main.show();
    let _ = main.set_focus();

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

    let frozen = {
        let mut session = state
            .session
            .lock()
            .map_err(|e| AppError::InternalError(format!("Failed to lock capture state: {e}")))?;
        session
            .take()
            .ok_or_else(|| AppError::InternalError("no frozen desktop capture".to_string()))?
    };

    if x < 0
        || y < 0
        || x as u32 + width > frozen.image.width()
        || y as u32 + height > frozen.image.height()
    {
        return Err(AppError::ValidationError(format!(
            "capture region ({x},{y},{width},{height}) is outside the frozen desktop"
        )));
    }

    let cropped = frozen.image.crop_imm(x as u32, y as u32, width, height);
    let bytes = encode_png(&cropped)?;
    let manager = image_library_state
        .manager
        .lock()
        .map_err(|e| AppError::InternalError(format!("Failed to lock image library: {e}")))?;
    let metadata = manager.add_image_from_bytes(&bytes, name, ModelImageFormat::Png)?;
    drop(manager);

    restore_main_window(&app, &state);

    let screen_x = frozen.origin_x + x;
    let screen_y = frozen.origin_y + y;
    let captured = CapturedImage {
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
    };
    let _ = app.emit("screen-capture-closed", ());
    let _ = app.emit("screen-capture-saved", &captured);
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
