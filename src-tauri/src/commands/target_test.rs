//! Read-only target resolution commands for the configuration panel.
//!
//! These commands answer "what would be clicked right now?" without raising a
//! window or sending input. They are the GUI counterpart of `vadctl find` /
//! `vadctl uia`, so the visual editor and the headless tool agree.

use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::commands::ImageLibraryState;
use crate::error::{AppError, Result};
use crate::matching::{ImageMatcher, MatchConfig, SearchRegion};
use crate::models::ImageId;
use crate::platform::ScreenCapture;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageTargetTest {
    pub found: bool,
    pub score: Option<f64>,
    pub threshold: f64,
    pub center: Option<TargetPoint>,
    pub hit_rect: Option<TargetRect>,
    pub elapsed_ms: u64,
    pub target: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiaTargetTest {
    pub found: bool,
    pub node: Option<serde_json::Value>,
    /// Title of a window covering the control, if any. Resolution is read-only,
    /// so the window is not raised first.
    pub occluded_by: Option<String>,
    pub error: Option<String>,
}

/// Resolve an image target on the current virtual desktop.
#[tauri::command]
pub fn test_image_target(
    image_library_state: State<'_, ImageLibraryState>,
    image_id: String,
    threshold: Option<f64>,
    roi: Option<crate::models::Region>,
) -> Result<ImageTargetTest> {
    let id = ImageId(
        Uuid::parse_str(&image_id)
            .map_err(|_| AppError::ValidationError(format!("invalid image id: {image_id}")))?,
    );
    let (path, name) = {
        let manager = image_library_state
            .manager
            .lock()
            .map_err(|e| AppError::InternalError(format!("Failed to lock image library: {e}")))?;
        let metadata = manager
            .get_image(&id)?
            .ok_or_else(|| AppError::ImageNotFound(image_id.clone()))?;
        let path = manager
            .get_image_path(&id)?
            .ok_or_else(|| AppError::ImageNotFound(image_id.clone()))?;
        (path, metadata.name)
    };

    let threshold = threshold.unwrap_or(0.7).clamp(0.0, 1.0);
    let capture = ScreenCapture::capture_virtual_desktop()?;
    let template = image::open(&path)?;

    let region = roi.and_then(|roi| {
        let x = (roi.x - capture.origin_x).max(0) as u32;
        let y = (roi.y - capture.origin_y).max(0) as u32;
        if x < capture.width && y < capture.height {
            Some(SearchRegion::new(
                x,
                y,
                roi.width.min(capture.width.saturating_sub(x)),
                roi.height.min(capture.height.saturating_sub(y)),
            ))
        } else {
            None
        }
    });
    let matcher = ImageMatcher::with_config(MatchConfig::with_threshold(threshold));
    let started = Instant::now();
    let result = matcher.find_image_bounded(
        &capture.image,
        &template,
        region,
        Duration::from_millis(3000),
    );
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let center = if result.found {
        Some(TargetPoint {
            x: result.center_x.unwrap_or(0) as i32 + capture.origin_x,
            y: result.center_y.unwrap_or(0) as i32 + capture.origin_y,
        })
    } else {
        None
    };
    let hit_rect = center.map(|point| {
        let width = result.width.unwrap_or(0);
        let height = result.height.unwrap_or(0);
        TargetRect {
            x: point.x - (width / 2) as i32,
            y: point.y - (height / 2) as i32,
            width,
            height,
        }
    });

    Ok(ImageTargetTest {
        found: result.found,
        score: result.confidence,
        threshold,
        center,
        hit_rect,
        elapsed_ms,
        target: name,
        error: None,
    })
}

/// Resolve a Windows UIA control without raising or clicking it.
#[tauri::command]
pub fn test_uia_target(
    window: String,
    name: Option<String>,
    automation_id: Option<String>,
    class_name: Option<String>,
    control_type: Option<String>,
    index: Option<usize>,
) -> Result<UiaTargetTest> {
    #[cfg(windows)]
    {
        use crate::platform::uia::{self, UiaSelector};

        let selector = UiaSelector {
            name,
            automation_id,
            class_name,
            control_type,
            index,
        };
        if selector.is_empty() {
            return Ok(UiaTargetTest {
                found: false,
                node: None,
                occluded_by: None,
                error: Some("至少需要一个选择条件（名称 / AutomationId / 类名 / 控件类型）".into()),
            });
        }
        match uia::find(&window, &selector) {
            Ok(Some(node)) => {
                let occluded_by = uia::occluder_at(&window, node.cx(), node.cy()).unwrap_or(None);
                Ok(UiaTargetTest {
                    found: true,
                    node: serde_json::to_value(&node).ok(),
                    occluded_by,
                    error: None,
                })
            }
            Ok(None) => Ok(UiaTargetTest {
                found: false,
                node: None,
                occluded_by: None,
                error: Some("未找到匹配的 UIA 控件".into()),
            }),
            Err(error) => Ok(UiaTargetTest {
                found: false,
                node: None,
                occluded_by: None,
                error: Some(error),
            }),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (window, name, automation_id, class_name, control_type, index);
        Ok(UiaTargetTest {
            found: false,
            node: None,
            occluded_by: None,
            error: Some("UIA 仅在 Windows 上可用".into()),
        })
    }
}
