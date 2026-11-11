//! Input simulation operations
//!
//! Mouse clicks (coordinate-based and image-based), keyboard text input,
//! with DPI scaling, stop/pause support, and panic safety.
//!
//! Validates: Requirements 8.4

use crate::error::{AppError, Result};
use crate::models::ClickMode;
use crate::core::BlockResult;
use crate::platform::InputController;

use super::events::{BlockEvidence, HitRect};
use super::runner::{safe_execute, Executor};

impl Executor {
    /// Execute ClickBlock
    ///
    /// Validates: Requirements 8.4 - Exception handling for input operations
    ///
    /// Coordinates are virtual-screen physical pixels in CLI, MCP and GUI.
    /// `dpi_scale` remains an explicit opt-in legacy correction (`vadctl
    /// --dpi-scale`); the GUI no longer sets it, so the same flow behaves the
    /// same everywhere.
    pub(super) async fn execute_click_block(
        &mut self,
        mode: &ClickMode,
        count: u32,
    ) -> Result<BlockResult> {
        // 鼠标按键在 Image 分支里可配置，其它模式保持左键。
        let mut click_button = crate::platform::MouseButton::Left;
        let (x, y) = match mode {
            ClickMode::Coordinates { x, y } => {
                // Apply DPI scaling: logical → physical pixels. Signed values
                // keep negative virtual-screen coordinates usable.
                if self.dpi_scale != 1.0 {
                    let sf = self.dpi_scale as f64;
                    (
                        (*x as f64 * sf).round() as i32,
                        (*y as f64 * sf).round() as i32,
                    )
                } else {
                    (*x, *y)
                }
            }
            ClickMode::Image { image_id, threshold, roi, fallback_click, button } => {
                click_button = match button.as_deref() {
                    Some("right") => crate::platform::MouseButton::Right,
                    Some("middle") => crate::platform::MouseButton::Middle,
                    _ => crate::platform::MouseButton::Left,
                };
                let image_id =
                    image_id.as_ref().ok_or_else(|| {
                        AppError::ExecutionFailed(
                            "Click block requires an image before execution"
                                .to_string(),
                        )
                    })?;
                // Check cache first
                let cached = if roi.is_some() {
                    // A cached point from a different/no ROI may be outside
                    // the configured search region; resolve again.
                    None
                } else {
                    let ctx = self.context.lock().await;
                    ctx.get_cached_image_match(image_id)
                };

                if let Some(pos) = cached {
                    let image_name = self.image_library
                        .get(image_id)
                        .map(|m| m.name.clone())
                        .unwrap_or_else(|| image_id.to_string());
                    self.last_evidence = Some(BlockEvidence {
                        score: None,
                        hit_rect: None,
                        target: Some(format!("cached image {image_name}")),
                    });
                    pos
                } else {
                    // Find image on screen
                    let (found, pos, confidence) =
                        self.find_image_on_screen(image_id, *threshold, *roi).await?;
                    if found {
                        pos
                    } else {
                        // 兜底：识别不到就按中心坐标点。中心优先取块的 ROI，
                        // 其次取图片自带的截取位置；两者都没有就没法兜底。
                        let centre = fallback_centre(
                            *roi,
                            self.image_library.get(image_id).and_then(|m| m.roi),
                        );

                        if fallback_click.unwrap_or(false) && centre.is_some() {
                            let (cx, cy) = centre.unwrap_or((0, 0));
                            crate::logging::log_error(
                                "Image match failed, falling back to centre click",
                                Some(&format!("image={image_id} centre=({cx},{cy})")),
                                None,
                            );
                            self.last_evidence = Some(BlockEvidence {
                                score: None,
                                hit_rect: None,
                                target: Some(format!(
                                    "fallback centre ({cx},{cy}) after miss {:.1}%",
                                    confidence * 100.0
                                )),
                            });
                            (cx, cy)
                        } else {
                            let image_name = self.image_library
                                .get(image_id)
                                .map(|m| m.name.as_str())
                                .unwrap_or("unknown");
                            let detail = if confidence > 0.0 {
                                format!(" (最佳匹配准确率 {:.1}%)", confidence * 100.0)
                            } else {
                                String::new()
                            };
                            return Ok(BlockResult::Error {
                                message: format!(
                                    "Image not found on screen: {} (name: {}){}",
                                    image_id, image_name, detail
                                ),
                            });
                        }
                    }
                }
            }
            ClickMode::Uia {
                window,
                name,
                automation_id,
                class_name,
                control_type,
                index,
                force,
            } => {
                #[cfg(windows)]
                {
                    let selector = crate::platform::uia::UiaSelector {
                        name: name.clone(),
                        automation_id: automation_id.clone(),
                        class_name: class_name.clone(),
                        control_type: control_type.clone(),
                        index: *index,
                    };
                    let node = if self.dry_run {
                        crate::platform::uia::inspect(
                            window,
                            &selector,
                            force.unwrap_or(false),
                        )
                        .map_err(AppError::ExecutionFailed)?
                    } else {
                        crate::platform::uia::prepare_click(
                            window,
                            &selector,
                            force.unwrap_or(false),
                        )
                        .map_err(AppError::ExecutionFailed)?
                    };
                    self.last_evidence = Some(BlockEvidence {
                        score: Some(1.0),
                        hit_rect: Some(HitRect::new(node.x, node.y, node.w as u32, node.h as u32)),
                        target: Some(format!(
                            "uia {} \"{}\"",
                            node.control_type_name, node.name
                        )),
                    });
                    (node.cx(), node.cy())
                }
                #[cfg(not(windows))]
                {
                    let _ = (window, name, automation_id, class_name, control_type, index, force);
                    return Err(AppError::ExecutionFailed(
                        "UIA targets are only supported on Windows".to_string(),
                    ));
                }
            }
        };

        // Dry-run: the target is resolved (and evidence recorded) but no
        // synthetic input is sent.
        if self.dry_run {
            return Ok(BlockResult::Continue);
        }

        // Check stop signal before starting clicks
        if *self.stop_receiver.borrow() {
            return Ok(BlockResult::Error {
                message: "Execution stopped".to_string(),
            });
        }

        self.wait_if_paused().await;

        // Perform clicks with panic handling in a single synchronous block
        // to avoid Enigo thread-safety (non-Send) issues across async awaits.
        let click_result = safe_execute(
            || {
                let mut input = InputController::new()?;
                for i in 0..count {
                    input.click_at(x, y, click_button)?;
                    if i < count - 1 {
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                }
                Ok(())
            },
            "Click operation",
        );

        match click_result {
            Ok(Ok(_)) => Ok(BlockResult::Continue),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(e),
        }
    }

    /// Execute InputTextBlock
    ///
    /// Validates: Requirements 8.4 - Exception handling for input operations
    pub(super) async fn execute_input_text_block(
        &mut self,
        text: &str,
        interval_ms: Option<u64>,
        window: Option<&str>,
    ) -> Result<BlockResult> {
        if self.dry_run {
            return Ok(BlockResult::Continue);
        }
        // 指定了目标窗口就先把它提到前台：运行时主窗口是最小化的，不指定
        // 目标就只能打到「碰巧在前台」的那个窗口上。
        if let Some(title) = window.map(str::trim).filter(|t| !t.is_empty()) {
            #[cfg(windows)]
            {
                crate::platform::uia::raise_window(title).map_err(AppError::ExecutionFailed)?;
                // 给窗口一点时间真正拿到焦点
                std::thread::sleep(std::time::Duration::from_millis(120));
            }
            #[cfg(not(windows))]
            {
                let _ = title;
            }
        }
        let interval = interval_ms.unwrap_or(10);

        let click_result = safe_execute(
            || {
                let mut input = InputController::new()?;
                input.type_text_with_interval(text, interval)
            },
            "Text input operation",
        );

        match click_result {
            Ok(Ok(_)) => Ok(BlockResult::Continue),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(e),
        }
    }
}

/// 兜底点击的中心：块自己的 ROI 优先，其次用图片自带的截取位置。
fn fallback_centre(
    block_roi: Option<crate::models::Region>,
    image_roi: Option<crate::models::Region>,
) -> Option<(i32, i32)> {
    block_roi.or(image_roi).map(|r| {
        (
            r.x + (r.width / 2) as i32,
            r.y + (r.height / 2) as i32,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(x: i32, y: i32, w: u32, h: u32) -> crate::models::Region {
        crate::models::Region { x, y, width: w, height: h }
    }

    #[test]
    fn fallback_centre_prefers_block_roi() {
        assert_eq!(
            fallback_centre(Some(region(100, 200, 40, 20)), Some(region(0, 0, 10, 10))),
            Some((120, 210))
        );
    }

    #[test]
    fn fallback_centre_falls_back_to_image_roi() {
        assert_eq!(
            fallback_centre(None, Some(region(1000, 600, 100, 50))),
            Some((1050, 625))
        );
    }

    #[test]
    fn fallback_centre_without_any_roi_is_none() {
        assert_eq!(fallback_centre(None, None), None);
    }
}
