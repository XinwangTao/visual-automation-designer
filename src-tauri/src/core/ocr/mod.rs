//! OCR (Optical Character Recognition) engine
//!
//! On Windows 10+, uses Windows.Media.Ocr API for text extraction.
//! On other platforms, returns a "not supported" error.
//!
//! Validates: Phase B — OCR Node

use serde::{Deserialize, Serialize};
use crate::error::{AppError, Result};

/// A single line of OCR-recognized text with position info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrLine {
    /// The recognized text
    pub text: String,
    /// Bounding box (x, y, width, height) in pixels
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// OCR recognition result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrResult {
    /// All recognized text concatenated
    pub text: String,
    /// Individual text lines with positions
    pub lines: Vec<OcrLine>,
}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use image::RgbaImage;

    use windows::Graphics::Imaging::{
        BitmapPixelFormat, SoftwareBitmap,
    };
    use windows::Media::Ocr::OcrEngine;

    /// Internal helper: create a SoftwareBitmap from RGBA pixel data
    /// Uses a memory stream + BitmapEncoder round-trip, which avoids direct
    /// pointer manipulation of the SoftwareBitmap buffer.
    fn rgba_to_software_bitmap(rgba: &[u8], width: i32, height: i32) -> Result<SoftwareBitmap> {
        use windows::Storage::Streams::DataWriter;

        // Copy the RGBA bytes into an IBuffer without a stream round-trip.
        // The previous PNG/stream path closed the stream before Seek and made
        // OCR fail with 0x80000013 ("object closed").
        let writer = DataWriter::new().map_err(|e| {
            AppError::ExecutionFailed(format!("Failed to create DataWriter: {e}"))
        })?;
        writer.WriteBytes(rgba).map_err(|e| {
            AppError::ExecutionFailed(format!("Failed to write OCR pixels: {e}"))
        })?;
        let buffer = writer.DetachBuffer().map_err(|e| {
            AppError::ExecutionFailed(format!("Failed to detach OCR buffer: {e}"))
        })?;

        SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Rgba8, width, height)
            .map_err(|e| {
                AppError::ExecutionFailed(format!("Failed to create OCR SoftwareBitmap: {e}"))
            })
    }

    /// Map Editor language tags to BCP-47 tags understood by Windows OCR.
    fn normalize_language(language: &str) -> String {
        match language.trim().to_ascii_lowercase().as_str() {
            "chi_sim" | "chi_sim+eng" | "zh" | "zh_cn" => "zh-Hans-CN".to_string(),
            "chi_tra" | "zh_tw" => "zh-Hant-TW".to_string(),
            "eng" | "en" => "en-US".to_string(),
            "jpn" | "ja" => "ja-JP".to_string(),
            "kor" | "ko" => "ko-KR".to_string(),
            other => other.to_string(),
        }
    }

    /// Extract lines with bounding boxes from the real OCR result.
    fn extract_lines(ocr_result: &windows::Media::Ocr::OcrResult) -> Result<Vec<OcrLine>> {
        let lines = ocr_result
            .Lines()
            .map_err(|e| AppError::ExecutionFailed(format!("OCR lines unavailable: {e}")))?;
        let count = lines
            .Size()
            .map_err(|e| AppError::ExecutionFailed(format!("OCR line count failed: {e}")))?;
        let mut out = Vec::new();
        for index in 0..count {
            let line = lines
                .GetAt(index)
                .map_err(|e| AppError::ExecutionFailed(format!("OCR line {index}: {e}")))?;
            let text = line
                .Text()
                .map_err(|e| AppError::ExecutionFailed(format!("OCR line text: {e}")))?
                .to_string();
            let words = line
                .Words()
                .map_err(|e| AppError::ExecutionFailed(format!("OCR words: {e}")))?;
            let word_count = words
                .Size()
                .map_err(|e| AppError::ExecutionFailed(format!("OCR word count: {e}")))?;
            let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
            let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);
            for word_index in 0..word_count {
                let word = words.GetAt(word_index).map_err(|e| {
                    AppError::ExecutionFailed(format!("OCR word {word_index}: {e}"))
                })?;
                let rect = word
                    .BoundingRect()
                    .map_err(|e| AppError::ExecutionFailed(format!("OCR word rect: {e}")))?;
                min_x = min_x.min(rect.X);
                min_y = min_y.min(rect.Y);
                max_x = max_x.max(rect.X + rect.Width);
                max_y = max_y.max(rect.Y + rect.Height);
            }
            let (x, y, width, height) = if word_count == 0 {
                (0, 0, 0, 0)
            } else {
                (
                    min_x.max(0.0) as u32,
                    min_y.max(0.0) as u32,
                    (max_x - min_x).max(0.0) as u32,
                    (max_y - min_y).max(0.0) as u32,
                )
            };
            out.push(OcrLine { text, x, y, width, height });
        }
        Ok(out)
    }

    /// Run OCR using Windows.Media.Ocr
    pub(super) fn recognize_text_impl(
        image: &RgbaImage,
        language: Option<&str>,
    ) -> Result<OcrResult> {
        let width = image.width() as i32;
        let height = image.height() as i32;
        let rgba = image.as_raw();

        // Create SoftwareBitmap from RGBA pixel data via memory stream
        let software_bitmap = rgba_to_software_bitmap(rgba, width, height)?;

        // Create OCR engine
        let engine = if let Some(lang) = language {
            // Convert &str to HSTRING for the Windows API
            let normalized = normalize_language(lang);
            let lang_hstring = windows::core::HSTRING::from(normalized.as_str());
            let language_obj = windows::Globalization::Language::CreateLanguage(&lang_hstring)
                .map_err(|e| {
                    AppError::ExecutionFailed(format!(
                        "Failed to create language '{}': {}",
                        lang, e
                    ))
                })?;

            // Check if language is available
            let available = OcrEngine::IsLanguageSupported(&language_obj)
                .map_err(|e| {
                    AppError::ExecutionFailed(format!(
                        "Failed to check OCR language support: {}",
                        e
                    ))
                })?;

            if !available {
                OcrEngine::TryCreateFromUserProfileLanguages().map_err(|e| {
                    AppError::ExecutionFailed(format!(
                        "OCR language '{}' is not available and the user profile fallback failed: {}",
                        normalized, e
                    ))
                })?
            } else {
                OcrEngine::TryCreateFromLanguage(&language_obj).map_err(|e| {
                    AppError::ExecutionFailed(format!(
                        "Failed to create OCR engine for '{}': {}",
                        normalized, e
                    ))
                })?
            }
        } else {
            // Use user profile languages
            OcrEngine::TryCreateFromUserProfileLanguages().map_err(|e| {
                AppError::ExecutionFailed(format!("Failed to create OCR engine: {}", e))
            })?
        };

        // Recognize
        let ocr_result = engine
            .RecognizeAsync(&software_bitmap)
            .map_err(|e| AppError::ExecutionFailed(format!("OCR recognition failed: {}", e)))?
            .get()
            .map_err(|e| AppError::ExecutionFailed(format!("OCR async failed: {}", e)))?;

        // Extract the recognized text using the OcrResult.Text property
        let recognized_text = ocr_result.Text().map_err(|e| {
            AppError::ExecutionFailed(format!("Failed to get OCR result text: {}", e))
        })?;

        let text = recognized_text.to_string();
        let lines = extract_lines(&ocr_result)?;

        Ok(OcrResult { text, lines })
    }
}

/// Run OCR on an image
pub fn recognize_text(
    image: &image::RgbaImage,
    language: Option<&str>,
) -> Result<OcrResult> {
    #[cfg(windows)]
    {
        windows_impl::recognize_text_impl(image, language)
    }

    #[cfg(not(windows))]
    {
        let _ = image; // suppress unused warning
        Err(AppError::ExecutionFailed(
            "OCR is only supported on Windows 10+ (Windows.Media.Ocr)".to_string(),
        ))
    }
}

/// Check if OCR is available on the current platform
pub fn is_ocr_available() -> bool {
    #[cfg(windows)]
    {
        true
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Check if keyword exists in OCR result (case-insensitive partial match)
pub fn contains_keyword(text: &str, keyword: &str) -> bool {
    text.to_lowercase().contains(&keyword.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_keyword_found() {
        assert!(contains_keyword("Hello World", "world"));
        assert!(contains_keyword("Hello World", "HELLO"));
        assert!(contains_keyword("你好世界", "世界"));
    }

    #[test]
    fn test_contains_keyword_not_found() {
        assert!(!contains_keyword("Hello World", "xyz"));
        assert!(!contains_keyword("你好世界", "再见"));
    }

    #[test]
    fn test_contains_keyword_case_insensitive() {
        assert!(contains_keyword("Hello World", "hello"));
        assert!(contains_keyword("Hello World", "WORLD"));
        assert!(contains_keyword("HELLO", "hello"));
    }

    #[test]
    fn test_contains_keyword_empty() {
        assert!(contains_keyword("anything", ""), "Empty keyword should always match");
        assert!(contains_keyword("", ""), "Empty string with empty keyword should match");
    }

    #[test]
    fn test_contains_keyword_partial_word() {
        assert!(contains_keyword("automation", "auto"));
        assert!(contains_keyword("automation", "tion"));
    }

    #[test]
    fn test_is_ocr_available() {
        // This function returns true on Windows, false elsewhere
        // Just verify it doesn't panic
        let _available = is_ocr_available();
    }
}
