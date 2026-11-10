//! Image matching operations
//!
//! Screenshot capture, template matching with caching, and performance
//! metrics for image-based automation blocks.
//!
//! Validates: Requirements 8.4

use std::time::{Duration, Instant};

use crate::error::{AppError, Result};
use crate::matching::SearchRegion;
use crate::models::{ImageId, Region};

use super::events::{BlockEvidence, HitRect};
use super::runner::{safe_execute, Executor};

/// Compute the screen-global center coordinates from a capture offset and match result.
///
/// Screen coordinates are signed: monitors left of / above the primary monitor
/// have negative origins. Casting the origin to `u32` (the previous behaviour)
/// wrapped negative values and made clicks land at `(-1, -1)`.
fn match_center(
    origin_x: &i32,
    origin_y: &i32,
    result: &crate::matching::MatchResult,
) -> (i32, i32) {
    (
        origin_x.saturating_add(result.center_x.unwrap_or(0) as i32),
        origin_y.saturating_add(result.center_y.unwrap_or(0) as i32),
    )
}

/// Build screen-space evidence for a match result.
fn evidence_for(
    origin_x: &i32,
    origin_y: &i32,
    result: &crate::matching::MatchResult,
    target: String,
) -> BlockEvidence {
    let (cx, cy) = match_center(origin_x, origin_y, result);
    let width = result.width.unwrap_or(0);
    let height = result.height.unwrap_or(0);
    BlockEvidence {
        score: result.confidence,
        hit_rect: if width > 0 && height > 0 {
            Some(HitRect::new(
                cx - (width / 2) as i32,
                cy - (height / 2) as i32,
                width,
                height,
            ))
        } else {
            None
        },
        target: Some(target),
    }
}


/// Clip a virtual-screen ROI to one monitor capture and convert to image coords.
/// Returns `None` when the ROI does not intersect this monitor.
fn search_region_for_capture(
    roi: Region,
    origin_x: i32,
    origin_y: i32,
    width: u32,
    height: u32,
) -> Option<SearchRegion> {
    let roi_right = roi.x.saturating_add(roi.width as i32);
    let roi_bottom = roi.y.saturating_add(roi.height as i32);
    let mon_right = origin_x.saturating_add(width as i32);
    let mon_bottom = origin_y.saturating_add(height as i32);
    let left = roi.x.max(origin_x);
    let top = roi.y.max(origin_y);
    let right = roi_right.min(mon_right);
    let bottom = roi_bottom.min(mon_bottom);
    if right <= left || bottom <= top {
        return None;
    }
    Some(SearchRegion::new(
        (left - origin_x) as u32,
        (top - origin_y) as u32,
        (right - left) as u32,
        (bottom - top) as u32,
    ))
}

impl Executor {
    /// Find an image on screen with caching and performance tracking.
    ///
    /// `threshold` overrides the matcher's default threshold for this call.
    /// Returns `(found, center_coords, best_confidence)` — the confidence is
    /// the highest NCC score seen even when `found` is false, so callers can
    /// give specific feedback like "匹配准确率 15% 低于阈值 70%".
    ///
    /// The screen coordinates are signed and already include the monitor
    /// origin, so negative-origin monitors work on both sides of the primary.
    ///
    /// Validates: Requirements 8.4 - Exception handling for image operations
    pub(super) async fn find_image_on_screen(
        &mut self,
        image_id: &ImageId,
        threshold: Option<f64>,
        roi: Option<Region>,
    ) -> Result<(bool, (i32, i32), f64)> {
        let start = Instant::now();

        // Get image metadata
        let metadata = self
            .image_library
            .get(image_id)
            .ok_or_else(|| AppError::ImageNotFound(image_id.to_string()))?
            .clone();

        // 块本身没设 ROI 时，回落到图片自带的截取区域（图片库里可编辑）。
        let roi = roi.or(metadata.roi);

        // Load the template image with panic handling
        let image_path = self.images_dir.join(&metadata.file_path);
        let template: image::DynamicImage = safe_execute(
            || image::open(&image_path).map_err(|e| AppError::ImageError(e.to_string())),
            "Image loading",
        )??;

        // Track best confidence across all monitors
        let mut best_confidence: f64 = 0.0;

        // 1. Try primary monitor first (fast path)
        let capture_result = safe_execute(
            || crate::platform::ScreenCapture::new().capture_screen(),
            "Screen capture primary",
        );

        if let Ok(Ok(capture)) = capture_result {
            let search_region = match roi {
                Some(region) => search_region_for_capture(
                    region,
                    capture.origin_x,
                    capture.origin_y,
                    capture.width,
                    capture.height,
                ),
                None => None,
            };
            if roi.is_some() && search_region.is_none() {
                // ROI is not on the primary monitor; fall through to others.
            } else {
            let result = {
                let mut matcher = self.matcher.lock().await;
                // Apply an optional threshold for this call only; previously
                // this mutated the matcher permanently and leaked into later
                // blocks that passed `None`.
                let previous = matcher.threshold();
                if let Some(t) = threshold {
                    matcher.set_threshold(t);
                }
                let result = matcher.find_image_bounded(
                    &capture.image,
                    &template,
                    search_region,
                    Duration::from_millis(3000),
                );
                if threshold.is_some() {
                    matcher.set_threshold(previous);
                }
                result
            };

            // Track best confidence even on failure
            if let Some(c) = result.confidence {
                best_confidence = best_confidence.max(c);
            }

            if result.found {
                let duration = start.elapsed();
                log::debug!(
                    "Image matching found on primary screen in {}ms for image_id: {}",
                    duration.as_millis(),
                    image_id
                );
                self.last_evidence = Some(evidence_for(
                    &capture.origin_x,
                    &capture.origin_y,
                    &result,
                    metadata.name.clone(),
                ));
                return Ok((true, match_center(&capture.origin_x, &capture.origin_y, &result), best_confidence));
            }
            }
        }

        // 2. If not found, try all other monitors (multi-monitor fallback path)
        if let Ok(count) = crate::platform::ScreenCapture::screen_count() {
            for i in 1..count {
                let capture_fallback_result = safe_execute(
                    || crate::platform::ScreenCapture::with_screen(i).capture_screen(),
                    "Screen capture fallback",
                );

                if let Ok(Ok(capture)) = capture_fallback_result {
                    let search_region = match roi {
                        Some(region) => search_region_for_capture(
                            region,
                            capture.origin_x,
                            capture.origin_y,
                            capture.width,
                            capture.height,
                        ),
                        None => None,
                    };
                    if roi.is_some() && search_region.is_none() {
                        continue;
                    }
                    let result = {
                        let mut matcher = self.matcher.lock().await;
                        let previous = matcher.threshold();
                        if let Some(t) = threshold {
                            matcher.set_threshold(t);
                        }
                        let result = matcher.find_image_bounded(
                            &capture.image,
                            &template,
                            search_region,
                            Duration::from_millis(3000),
                        );
                        if threshold.is_some() {
                            matcher.set_threshold(previous);
                        }
                        result
                    };

                    if let Some(c) = result.confidence {
                        best_confidence = best_confidence.max(c);
                    }

                    if result.found {
                        let duration = start.elapsed();
                        log::info!(
                            "Image matching found on secondary screen {} in {}ms for image_id: {}",
                            i,
                            duration.as_millis(),
                            image_id
                        );
                        self.last_evidence = Some(evidence_for(
                            &capture.origin_x,
                            &capture.origin_y,
                            &result,
                            metadata.name.clone(),
                        ));
                        return Ok((true, match_center(&capture.origin_x, &capture.origin_y, &result), best_confidence));
                    }
                }
            }
        }

        let duration = start.elapsed();
        let current_threshold = threshold.unwrap_or(0.7);
        if best_confidence > 0.0 && best_confidence < current_threshold {
            log::warn!(
                "Image matching failed: {} — 最佳匹配准确率 {:.1}% 低于阈值 {:.0}% (用时 {}ms)",
                image_id,
                best_confidence * 100.0,
                current_threshold * 100.0,
                duration.as_millis(),
            );
        } else {
            log::warn!(
                "Image matching failed across all screens in {}ms for image_id: {}",
                duration.as_millis(),
                image_id
            );
        }

        self.last_evidence = Some(BlockEvidence {
            score: if best_confidence > 0.0 {
                Some(best_confidence)
            } else {
                None
            },
            hit_rect: None,
            target: Some(metadata.name.clone()),
        });

        Ok((false, (0, 0), best_confidence))
    }

    /// Get matcher performance metrics
    pub async fn matcher_metrics(&self) -> crate::matching::MatchMetrics {
        let matcher = self.matcher.lock().await;
        matcher.metrics().clone()
    }

    /// Clear matcher cache
    pub async fn clear_matcher_cache(&self) {
        let mut matcher = self.matcher.lock().await;
        matcher.clear_cache();
    }

    /// Get matcher cache statistics
    pub async fn cache_stats(&self) -> crate::matching::CacheStats {
        let matcher = self.matcher.lock().await;
        matcher.cache_stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::MatchResult;

    #[test]
    fn negative_origin_does_not_wrap_coordinates() {
        let result = MatchResult::found(10, 20, 0.9, 4, 6);
        // A monitor to the left of the primary has a negative origin. The old
        // implementation cast it to u32 and wrapped to ~4.29e9.
        assert_eq!(match_center(&-1920, &-40, &result), (-1910, -20));
    }

    #[test]
    fn positive_origin_is_added() {
        let result = MatchResult::found(10, 20, 0.9, 4, 6);
        assert_eq!(match_center(&100, &200, &result), (110, 220));
    }

    #[test]
    fn evidence_rect_is_screen_space() {
        // Match center is image-relative; the origin is added once.
        let result = MatchResult::found(10, 20, 0.9, 20, 10);
        let evidence = evidence_for(&100, &200, &result, "button".to_string());
        assert_eq!(evidence.score, Some(0.9));
        let rect = evidence.hit_rect.expect("rect");
        assert_eq!(rect.x, 100);
        assert_eq!(rect.y, 215);
        assert_eq!(rect.width, 20);
        assert_eq!(rect.height, 10);
        assert_eq!(evidence.target.as_deref(), Some("button"));
    }
}
