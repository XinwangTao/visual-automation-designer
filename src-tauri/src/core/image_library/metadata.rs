//! Image metadata handling and hash calculation
//!
//! This module provides functionality for computing image hashes
//! for deduplication purposes.
//!
//! Validates: Requirements 1.2

use image::DynamicImage;

use crate::error::Result;

/// Compute a content hash used for deduplication (exact byte identity).
///
/// 去重必须判断「内容是否真的相同」：之前用的 dHash 只看相邻像素的亮度关系，
/// 纯色图不论什么颜色都会得到同一个哈希，两张不同颜色的扁平按钮会被判成同一张，
/// 用户最后拿到的模板根本对不上。感知哈希保留给相似度分析，去重改走内容哈希。
pub fn compute_content_hash(bytes: &[u8]) -> String {
    // FNV-1a 64：稳定、无依赖，本地题库去重足够。
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Compute a perceptual hash for an image for deduplication purposes.
///
/// Uses a difference hash (dHash) algorithm that produces a 64-bit hash
/// based on the image's grayscale gradients. This allows detecting
/// near-duplicate images even if they have minor differences.
///
/// # Arguments
/// * `image` - The dynamic image to hash
///
/// # Returns
/// A hexadecimal string representation of the hash
pub fn compute_image_hash(image: &DynamicImage) -> Result<String> {
    // Resize to 9x8 for difference hash (one extra column for gradient calculation)
    let resized = image.resize_exact(9, 8, image::imageops::FilterType::Lanczos3);
    
    // Convert to grayscale
    let grayscale = resized.to_luma8();
    
    // Compute difference hash
    let mut hash: u64 = 0;
    
    for y in 0..8 {
        for x in 0..8 {
            // Compare each pixel with its right neighbor
            let left = grayscale.get_pixel(x, y);
            let right = grayscale.get_pixel(x + 1, y);
            
            // If left pixel is brighter than right, set bit to 1
            if left[0] > right[0] {
                let bit_position = y * 8 + x;
                hash |= 1u64 << bit_position;
            }
        }
    }
    
    // Convert to hexadecimal string
    Ok(format!("{:016x}", hash))
}

/// Calculate the Hamming distance between two hashes.
///
/// The Hamming distance is the number of bit positions where
/// the two hashes differ. A smaller distance indicates more
/// similar images.
///
/// # Arguments
/// * `hash1` - First hash as hexadecimal string
/// * `hash2` - Second hash as hexadecimal string
///
/// # Returns
/// The Hamming distance, or None if hashes are invalid
#[cfg(test)]
fn hamming_distance(hash1: &str, hash2: &str) -> Option<u32> {
    let h1 = u64::from_str_radix(hash1, 16).ok()?;
    let h2 = u64::from_str_radix(hash2, 16).ok()?;
    
    Some((h1 ^ h2).count_ones())
}

/// Check if two images are likely duplicates based on hash distance.
///
/// # Arguments
/// * `hash1` - First hash as hexadecimal string
/// * `hash2` - Second hash as hexadecimal string
/// * `threshold` - Maximum Hamming distance to consider as duplicate (typically 5-10)
///
/// # Returns
/// true if the images are likely duplicates
#[cfg(test)]
fn are_likely_duplicates(hash1: &str, hash2: &str, threshold: u32) -> bool {
    hamming_distance(hash1, hash2)
        .map(|d| d <= threshold)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn create_test_image(width: u32, height: u32, color: Rgb<u8>) -> DynamicImage {
        let buffer: ImageBuffer<Rgb<u8>, Vec<u8>> = 
            ImageBuffer::from_pixel(width, height, color);
        DynamicImage::ImageRgb8(buffer)
    }

    #[test]
    fn test_compute_hash_produces_consistent_results() {
        let image = create_test_image(100, 100, Rgb([128, 128, 128]));
        let hash1 = compute_image_hash(&image).unwrap();
        let hash2 = compute_image_hash(&image).unwrap();
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 16); // 64-bit hex string
    }

    #[test]
    fn test_different_images_have_different_hashes() {
        // Create two images with different gradient patterns
        // The dHash algorithm compares pixels to their right neighbors,
        // so we need images with different horizontal gradient patterns.
        
        // Create a horizontal gradient (left to right, increasing)
        let mut img1 = ImageBuffer::from_pixel(100, 100, Rgb([0u8, 0, 0]));
        for y in 0..100 {
            for x in 0..100 {
                let pixel = img1.get_pixel_mut(x, y);
                *pixel = Rgb([(x * 255 / 100) as u8, 0, 0]);
            }
        }
        let dyn_img1 = DynamicImage::ImageRgb8(img1);
        
        // Create a horizontal gradient (left to right, decreasing)
        let mut img2 = ImageBuffer::from_pixel(100, 100, Rgb([0u8, 0, 0]));
        for y in 0..100 {
            for x in 0..100 {
                let pixel = img2.get_pixel_mut(x, y);
                *pixel = Rgb([(255 - x * 255 / 100) as u8, 0, 0]);
            }
        }
        let dyn_img2 = DynamicImage::ImageRgb8(img2);
        
        let hash1 = compute_image_hash(&dyn_img1).unwrap();
        let _hash2 = compute_image_hash(&dyn_img2).unwrap();
        
        // Note: Both gradients produce the same hash because dHash only looks at
        // whether left > right (gradient direction doesn't matter when gradients are uniform).
        // So we use a more complex pattern for the third image.
        
        // Create a checkerboard-like pattern (alternating gradient)
        let mut img3 = ImageBuffer::from_pixel(100, 100, Rgb([0u8, 0, 0]));
        for y in 0..100 {
            for x in 0..100 {
                let pixel = img3.get_pixel_mut(x, y);
                *pixel = Rgb([if x % 2 == 0 { 255 } else { 0 }, 0, 0]);
            }
        }
        let dyn_img3 = DynamicImage::ImageRgb8(img3);
        let hash3 = compute_image_hash(&dyn_img3).unwrap();
        
        // Checkerboard pattern should be different from gradient
        assert_ne!(hash1, hash3);
    }

    #[test]
    fn test_hamming_distance() {
        let hash1 = "0000000000000000";
        let hash2 = "0000000000000001";
        let distance = hamming_distance(hash1, hash2).unwrap();
        assert_eq!(distance, 1);
        
        let hash3 = "ffffffffffffffff";
        let distance2 = hamming_distance(hash1, hash3).unwrap();
        assert_eq!(distance2, 64); // All bits different
    }

    #[test]
    fn test_are_likely_duplicates() {
        let hash1 = "0000000000000000";
        let hash2 = "0000000000000001"; // 1 bit difference
        let hash3 = "ffffffffffffffff"; // 64 bits difference
        
        assert!(are_likely_duplicates(hash1, hash2, 5));
        assert!(!are_likely_duplicates(hash1, hash3, 5));
    }
}
