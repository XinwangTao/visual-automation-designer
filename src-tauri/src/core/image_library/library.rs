//! Image Library Manager implementation
//!
//! This module provides the main ImageLibraryManager that handles
//! file system operations for the image library.
//!
//! Validates: Requirements 1.1, 1.3, 1.5

use std::path::{Path, PathBuf};
use std::fs;
use std::sync::{Arc, Mutex};

use image::io::Reader as ImageReader;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, Result};
use crate::models::image::{ImageFormat, ImageId, ImageMetadata, ImageLibrary};

/// Manager for the image library, handling file system operations
pub struct ImageLibraryManager {
    /// The in-memory image library
    library: Arc<Mutex<ImageLibrary>>,
    /// The directory where images are stored
    images_dir: PathBuf,
    /// Path to the library metadata file
    metadata_path: PathBuf,
}

impl ImageLibraryManager {
    /// Create a new ImageLibraryManager with the given images directory.
    ///
    /// This will create the images directory if it doesn't exist,
    /// and load any existing library metadata.
    ///
    /// # Arguments
    /// * `images_dir` - Directory to store image files
    ///
    /// # Returns
    /// A new ImageLibraryManager instance
    pub fn new<P: Into<PathBuf>>(images_dir: P) -> Result<Self> {
        let images_dir = images_dir.into();
        
        // Create images directory if it doesn't exist
        if !images_dir.exists() {
            fs::create_dir_all(&images_dir)
                .map_err(|e| AppError::InternalError(
                    format!("Failed to create images directory: {}", e)
                ))?;
        }
        
        let metadata_path = images_dir.join("library_metadata.json");

        // 原子保存会先删旧文件再 rename；若正好在这中间崩溃/断电，主文件不在了，
        // 但临时文件里是最新数据——直接恢复它，否则会被当成空库，下次保存就真丢了。
        let temp_path = metadata_path.with_extension("json.tmp");
        if !metadata_path.exists() && temp_path.exists() {
            let _ = fs::rename(&temp_path, &metadata_path);
        }

        // Load existing library or create new one
        let library = Self::load_library(&metadata_path)?;
        
        Ok(Self {
            library: Arc::new(Mutex::new(library)),
            images_dir,
            metadata_path,
        })
    }
    
    /// Create an ImageLibraryManager from a Tauri AppHandle.
    ///
    /// This uses the app's data directory to store images.
    ///
    /// # Arguments
    /// * `app_handle` - Tauri application handle
    ///
    /// # Returns
    /// A new ImageLibraryManager instance
    pub fn from_app_handle(app_handle: &AppHandle) -> Result<Self> {
        let data_dir = app_handle
            .path()
            .app_data_dir()
            .map_err(|e| AppError::InternalError(
                format!("Failed to get app data directory: {}", e)
            ))?;
        
        let images_dir = data_dir.join("images");
        Self::new(images_dir)
    }
    
    /// Load the library metadata from disk with corruption recovery.
    fn load_library(path: &Path) -> Result<ImageLibrary> {
        if path.exists() {
            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    crate::logging::log_error(
                        "Failed to read image library metadata file from disk",
                        Some(&e.to_string()),
                        None,
                    );
                    return Ok(ImageLibrary::new());
                }
            };

            match serde_json::from_str::<ImageLibrary>(&content) {
                Ok(library) => Ok(library),
                Err(e) => {
                    // Corruption detected!
                    crate::logging::log_error(
                        "Image library metadata file is corrupted, backing up and creating clean replacement",
                        Some(&e.to_string()),
                        None,
                    );
                    
                    // Attempt to backup the corrupted file
                    let mut backup_path = path.to_path_buf();
                    backup_path.set_extension("json.bak");
                    if let Err(backup_err) = fs::rename(path, &backup_path) {
                        crate::logging::log_error(
                            "Failed to backup corrupted image library metadata file",
                            Some(&backup_err.to_string()),
                            None,
                        );
                    } else {
                        crate::logging::log_error(
                            &format!("Corrupted metadata backed up to {:?}", backup_path),
                            None,
                            None,
                        );
                    }

                    // Fallback to a clean library to prevent app crash
                    Ok(ImageLibrary::new())
                }
            }
        } else {
            Ok(ImageLibrary::new())
        }
    }
    
    /// 从磁盘重新读取元数据。
    ///
    /// GUI 和 `vadctl` 是两个进程，各自只在启动时读一次；不重读的话，后写的
    /// 那个会拿自己的旧快照覆盖掉对方刚新增/删除的条目。
    fn refresh_from_disk(&self) {
        // 文件暂时不存在（另一次保存正在 rename，或刚被外部移走）时保留内存里的
        // 一份，绝不能当成「空库」——否则下一次保存会把整库写没。
        if !self.metadata_path.exists() {
            return;
        }
        if let Ok(fresh) = Self::load_library(&self.metadata_path) {
            if let Ok(mut library) = self.library.lock() {
                *library = fresh;
            }
        }
    }

    /// Save the library metadata to disk.
    ///
    /// 先写临时文件再 rename：GUI 和 `vadctl` 会同时读写同一个文件，
    /// 直接截断覆盖时另一个进程可能读到半个 JSON，被当成损坏而整库清空。
    fn save_library(&self) -> Result<()> {
        let content = {
            let library = self.library.lock()
                .map_err(|e| AppError::InternalError(
                    format!("Failed to lock library: {}", e)
                ))?;
            serde_json::to_string_pretty(&*library)?
        };

        // 临时名带 pid：GUI 和 vadctl 同时保存时不会互相截断同一个 tmp。
        let temp_path = self.metadata_path.with_extension(format!("json.{}.tmp", std::process::id()));
        fs::write(&temp_path, content)?;
        // Rust 的 rename 在 Windows 上是 MoveFileEx(MOVEFILE_REPLACE_EXISTING)，
        // 直接覆盖即可；先删目标会制造「主文件不存在」的崩溃窗口。
        if let Err(err) = fs::rename(&temp_path, &self.metadata_path) {
            let _ = fs::remove_file(&temp_path);
            return Err(err.into());
        }

        Ok(())
    }
    
    /// Add an image to the library from a file path.
    ///
    /// This will:
    /// 1. Validate the image format
    /// 2. Read the image dimensions
    /// 3. Compute a hash for deduplication
    /// 4. Copy the file to the images directory
    /// 5. Add metadata to the library
    ///
    /// # Arguments
    /// * `source_path` - Path to the source image file
    /// * `name` - Display name for the image
    ///
    /// # Returns
    /// The created ImageMetadata
    pub fn add_image<P: AsRef<Path>>(&self, source_path: P, name: String) -> Result<ImageMetadata> {
        self.refresh_from_disk();
        let source_path = source_path.as_ref();
        
        // Validate file exists
        if !source_path.exists() {
            return Err(AppError::ImageNotFound(
                format!("Image file not found: {}", source_path.display())
            ));
        }
        
        // Determine format from extension
        let extension = source_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        
        let format = ImageFormat::from_extension(extension)
            .ok_or_else(|| AppError::ImageError(
                format!("Unsupported image format: {}", extension)
            ))?;
        
        // Load image to get dimensions and compute hash
        let img = ImageReader::open(source_path)
            .map_err(|e| AppError::ImageError(e.to_string()))?
            .decode()
            .map_err(|e| AppError::ImageError(e.to_string()))?;
        
        let (width, height) = (img.width(), img.height());
        
        // Compute hash for deduplication
        let hash = super::metadata::compute_content_hash(&fs::read(source_path)?);
        
        // Check for duplicates
        {
            let library = self.library.lock()
                .map_err(|e| AppError::InternalError(
                    format!("Failed to lock library: {}", e)
                ))?;
            
            if library.has_duplicate(&hash) {
                return Err(AppError::ValidationError(
                    "An identical image already exists in the library".to_string()
                ));
            }
        }
        
        // Generate unique filename
        let id = ImageId::new();
        let filename = format!("{}.{}", id.0, format.extension());
        let dest_path = self.images_dir.join(&filename);
        
        // Copy file to images directory
        fs::copy(source_path, &dest_path)?;
        
        // Create metadata
        let metadata = ImageMetadata::new(
            name,
            filename,
            width,
            height,
            format,
            hash,
        );
        
        // Add to library
        {
            let mut library = self.library.lock()
                .map_err(|e| AppError::InternalError(
                    format!("Failed to lock library: {}", e)
                ))?;
            
            library.add_image(metadata.clone());
        }
        
        // Save library metadata
        self.save_library()?;
        
        Ok(metadata)
    }
    
    /// Add an image from raw bytes.
    ///
    /// # Arguments
    /// * `bytes` - Raw image data
    /// * `name` - Display name for the image
    /// * `format` - Image format
    ///
    /// # Returns
    /// The created ImageMetadata
    pub fn add_image_from_bytes(
        &self,
        bytes: &[u8],
        name: String,
        format: ImageFormat,
    ) -> Result<ImageMetadata> {
        self.add_image_from_bytes_inner(bytes, name, format, false)
    }

    /// 与 [`add_image_from_bytes`] 相同，但允许感知哈希撞车。
    ///
    /// 去重用的是 dHash（只看结构）：纯色图、结构相似的截图会得到同一个哈希。
    /// 调用方在逐字节确认「不是同一张」之后必须能把它加进库，否则用户会莫名
    /// 保存在失败，或者更糟——拿到另一张图的模板。
    pub fn add_image_from_bytes_distinct(
        &self,
        bytes: &[u8],
        name: String,
        format: ImageFormat,
    ) -> Result<ImageMetadata> {
        self.add_image_from_bytes_inner(bytes, name, format, true)
    }

    fn add_image_from_bytes_inner(
        &self,
        bytes: &[u8],
        name: String,
        format: ImageFormat,
        allow_duplicate: bool,
    ) -> Result<ImageMetadata> {
        self.refresh_from_disk();
        // Decode image from bytes
        let img = image::load_from_memory_with_format(
            bytes,
            match format {
                ImageFormat::Png => image::ImageFormat::Png,
                ImageFormat::Jpg => image::ImageFormat::Jpeg,
                ImageFormat::Bmp => image::ImageFormat::Bmp,
            },
        ).map_err(|e| AppError::ImageError(e.to_string()))?;
        
        let (width, height) = (img.width(), img.height());
        
        // Compute hash for deduplication
        let hash = super::metadata::compute_content_hash(bytes);

        // Check for duplicates
        {
            let library = self.library.lock()
                .map_err(|e| AppError::InternalError(
                    format!("Failed to lock library: {}", e)
                ))?;

            if !allow_duplicate && library.has_duplicate(&hash) {
                return Err(AppError::ValidationError(
                    "An identical image already exists in the library".to_string()
                ));
            }
        }
        
        // Generate unique filename
        let id = ImageId::new();
        let filename = format!("{}.{}", id.0, format.extension());
        let dest_path = self.images_dir.join(&filename);
        
        // Save image file
        img.save(&dest_path)
            .map_err(|e| AppError::ImageError(e.to_string()))?;
        
        // Create metadata
        let metadata = ImageMetadata::new(
            name,
            filename,
            width,
            height,
            format,
            hash,
        );
        
        // Add to library
        {
            let mut library = self.library.lock()
                .map_err(|e| AppError::InternalError(
                    format!("Failed to lock library: {}", e)
                ))?;
            
            library.add_image(metadata.clone());
        }
        
        // Save library metadata
        self.save_library()?;
        
        Ok(metadata)
    }
    
    /// Remove an image from the library.
    ///
    /// This will:
    /// 1. Remove the metadata from the library
    /// 2. Delete the image file
    /// 3. Save the updated library metadata
    ///
    /// # Arguments
    /// * `id` - ID of the image to remove
    ///
    /// # Returns
    /// true if the image was removed, false if not found
    pub fn remove_image(&self, id: &ImageId) -> Result<bool> {
        self.refresh_from_disk();
        let metadata = {
            let mut library = self.library.lock()
                .map_err(|e| AppError::InternalError(
                    format!("Failed to lock library: {}", e)
                ))?;
            
            let metadata = library.remove_image(id);
            metadata
        };
        
        if let Some(metadata) = metadata {
            // 先落盘再删文件：写盘失败时最多留个孤儿文件，绝不会出现
            // 「元数据已删、文件还在」或反过来的半截状态。
            self.save_library()?;

            let file_path = self.images_dir.join(&metadata.file_path);
            if file_path.exists() {
                let _ = fs::remove_file(file_path);
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// Get an image's metadata by ID.
    ///
    /// # Arguments
    /// * `id` - ID of the image
    ///
    /// # Returns
    /// The image metadata, or None if not found
    pub fn get_image(&self, id: &ImageId) -> Result<Option<ImageMetadata>> {
        self.refresh_from_disk();
        let library = self.library.lock()
            .map_err(|e| AppError::InternalError(
                format!("Failed to lock library: {}", e)
            ))?;
        
        Ok(library.get_image(id).cloned())
    }
    
    /// List all images in the library.
    ///
    /// # Returns
    /// A list of all image metadata
    pub fn list_images(&self) -> Result<Vec<ImageMetadata>> {
        self.refresh_from_disk();
        let library = self.library.lock()
            .map_err(|e| AppError::InternalError(
                format!("Failed to lock library: {}", e)
            ))?;
        
        Ok(library.list_images().to_vec())
    }
    
    /// Rename an image.
    ///
    /// # Arguments
    /// * `id` - ID of the image to rename
    /// * `new_name` - New name for the image
    ///
    /// # Returns
    /// true if the image was renamed, false if not found
    pub fn rename_image(&self, id: &ImageId, new_name: String) -> Result<bool> {
        self.refresh_from_disk();
        let success = {
            let mut library = self.library.lock()
                .map_err(|e| AppError::InternalError(
                    format!("Failed to lock library: {}", e)
                ))?;
            
            library.rename_image(id, new_name)
        };
        
        if success {
            self.save_library()?;
        }

        Ok(success)
    }

    /// Attach (or clear) the screen region an image was captured from.
    ///
    /// 用一块新截图**原地替换**已有图片的内容（保持 id / 名称），同时写入新的
    /// 尺寸、哈希和屏幕位置。用于图片库里「在屏幕上调整」：框选即重裁这张图。
    pub fn replace_image_content(
        &self,
        id: &ImageId,
        bytes: &[u8],
        source: Option<crate::models::block::Region>,
        roi: Option<crate::models::block::Region>,
    ) -> Result<ImageMetadata> {
        self.refresh_from_disk();

        let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map_err(|e| AppError::ImageError(e.to_string()))?;
        let (width, height) = (image.width(), image.height());
        let hash = super::metadata::compute_content_hash(bytes);

        let (file_path, metadata) = {
            let mut library = self.library.lock()
                .map_err(|e| AppError::InternalError(
                    format!("Failed to lock library: {}", e)
                ))?;

            let entry = library
                .get_image_mut(id)
                .ok_or_else(|| AppError::ImageNotFound(id.to_string()))?;
            entry.width = width;
            entry.height = height;
            entry.hash = hash;
            entry.set_source(source);
            if roi.is_some() {
                entry.set_roi(roi);
            }
            (entry.file_path.clone(), entry.clone())
        };

        fs::write(self.images_dir.join(file_path), bytes)?;
        self.save_library()?;
        Ok(metadata)
    }

    /// Blocks fall back to this region when their own ROI is empty, so editing
    /// it here changes where flows look for the image.
    pub fn set_image_roi(
        &self,
        id: &ImageId,
        roi: Option<crate::models::block::Region>,
    ) -> Result<bool> {
        self.refresh_from_disk();
        let success = {
            let mut library = self.library.lock()
                .map_err(|e| AppError::InternalError(
                    format!("Failed to lock library: {}", e)
                ))?;

            match library.get_image_mut(id) {
                Some(image) => {
                    image.set_roi(roi);
                    true
                }
                None => false,
            }
        };

        if success {
            self.save_library()?;
        }

        Ok(success)
    }

    /// Remove every image and reset the metadata file.
    ///
    /// 连没登记在元数据里的孤儿图片文件一起删——用户点「清空」期待的是目录变空，
    /// 不是「元数据清空、文件还在」。
    ///
    /// Destructive: the caller must confirm with the user first.
    pub fn clear_images(&self) -> Result<usize> {
        self.refresh_from_disk();
        let removed = {
            let mut library = self.library.lock()
                .map_err(|e| AppError::InternalError(
                    format!("Failed to lock library: {}", e)
                ))?;

            let removed = library.count();
            let files: Vec<String> = library
                .list_images()
                .iter()
                .map(|image| image.file_path.clone())
                .collect();
            library.clear();
            (removed, files)
        };

        // 先落盘再删文件（同上：宁可留孤儿文件，也不要元数据与磁盘互相矛盾）。
        self.save_library()?;

        let mut deleted = removed.1.clone();
        // 顺带清掉没登记在元数据里的孤儿图片文件。
        if let Ok(entries) = fs::read_dir(&self.images_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_image = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "bmp"))
                    .unwrap_or(false);
                if is_image {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        deleted.push(name.to_string());
                    }
                }
            }
        }
        for file in &deleted {
            let path = self.images_dir.join(file);
            if path.exists() {
                let _ = fs::remove_file(path);
            }
        }

        Ok(removed.0)
    }

    /// 库里的这张图与给定字节是否完全相同。
    ///
    /// 感知哈希只判断「结构像」，判重之后必须再比一次字节，否则两张不同颜色的
    /// 纯色按钮会被当成同一张，用户拿到的模板根本对不上。
    /// 返回 `None` 表示元数据还在但文件已经没了（外部删除）。
    pub fn image_bytes_match(&self, id: &ImageId, bytes: &[u8]) -> Result<Option<bool>> {
        let Some(path) = self.get_image_path(id)? else {
            return Ok(None);
        };
        match fs::read(&path) {
            Ok(existing) => Ok(Some(existing == bytes)),
            Err(_) => Ok(None),
        }
    }

    /// Get the full path to an image file.
    ///
    /// # Arguments
    /// * `id` - ID of the image
    ///
    /// # Returns
    /// The full path to the image file, or None if not found
    pub fn get_image_path(&self, id: &ImageId) -> Result<Option<PathBuf>> {
        self.refresh_from_disk();
        let library = self.library.lock()
            .map_err(|e| AppError::InternalError(
                format!("Failed to lock library: {}", e)
            ))?;
        
        Ok(library.get_image(id).map(|img| self.images_dir.join(&img.file_path)))
    }
    
    /// Get the count of images in the library.
    pub fn count(&self) -> Result<usize> {
        self.refresh_from_disk();
        let library = self.library.lock()
            .map_err(|e| AppError::InternalError(
                format!("Failed to lock library: {}", e)
            ))?;
        
        Ok(library.count())
    }
    
    /// Check if an image with the given hash exists (for deduplication).
    pub fn has_duplicate(&self, hash: &str) -> Result<bool> {
        let library = self.library.lock()
            .map_err(|e| AppError::InternalError(
                format!("Failed to lock library: {}", e)
            ))?;
        
        Ok(library.has_duplicate(hash))
    }

    /// Find the stored image with the given hash, if any.
    ///
    /// The library refuses to store two identical images, so callers that only
    /// need coverage (a second capture of the same button, say) can reuse the
    /// existing entry instead of failing.
    pub fn find_by_hash(&self, hash: &str) -> Result<Option<ImageMetadata>> {
        self.refresh_from_disk();
        let library = self.library.lock()
            .map_err(|e| AppError::InternalError(
                format!("Failed to lock library: {}", e)
            ))?;

        Ok(library.find_by_hash(hash).cloned())
    }

    /// Find images by name (partial match, case-insensitive).
    pub fn find_by_name(&self, query: &str) -> Result<Vec<ImageMetadata>> {
        let library = self.library.lock()
            .map_err(|e| AppError::InternalError(
                format!("Failed to lock library: {}", e)
            ))?;
        
        Ok(library.find_by_name(query).into_iter().cloned().collect())
    }
    
    /// Get a reference to the underlying ImageLibrary (for serialization).
    pub fn library(&self) -> Result<Arc<Mutex<ImageLibrary>>> {
        Ok(self.library.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use image::{ImageBuffer, Rgb};
    use std::io::Write;

    fn create_test_image_file(dir: &Path, name: &str, format: ImageFormat) -> PathBuf {
        create_test_image_file_with_color(dir, name, format, Rgb([128, 128, 128]))
    }
    
    fn create_test_image_file_with_color(dir: &Path, name: &str, format: ImageFormat, color: Rgb<u8>) -> PathBuf {
        let path = dir.join(name);
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> = 
            ImageBuffer::from_pixel(100, 100, color);
        
        let img_format = match format {
            ImageFormat::Png => image::ImageFormat::Png,
            ImageFormat::Jpg => image::ImageFormat::Jpeg,
            ImageFormat::Bmp => image::ImageFormat::Bmp,
        };
        
        img.save_with_format(&path, img_format).unwrap();
        path
    }

    #[test]
    fn test_image_library_manager_new() {
        let temp_dir = tempdir().unwrap();
        let images_dir = temp_dir.path().join("images");
        
        let manager = ImageLibraryManager::new(&images_dir).unwrap();
        assert!(images_dir.exists());
        assert_eq!(manager.count().unwrap(), 0);
    }

    #[test]
    fn test_add_image() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let images_dir = temp_dir.path().join("images");
        fs::create_dir_all(&source_dir).unwrap();
        
        let image_path = create_test_image_file(&source_dir, "test.png", ImageFormat::Png);
        
        let manager = ImageLibraryManager::new(&images_dir).unwrap();
        let metadata = manager.add_image(&image_path, "Test Image".to_string()).unwrap();
        
        assert_eq!(metadata.name, "Test Image");
        assert_eq!(metadata.width, 100);
        assert_eq!(metadata.height, 100);
        assert_eq!(metadata.format, ImageFormat::Png);
        assert_eq!(manager.count().unwrap(), 1);
    }

    #[test]
    fn test_add_duplicate_image_fails() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let images_dir = temp_dir.path().join("images");
        fs::create_dir_all(&source_dir).unwrap();
        
        let image_path = create_test_image_file(&source_dir, "test.png", ImageFormat::Png);
        
        let manager = ImageLibraryManager::new(&images_dir).unwrap();
        
        // First add should succeed
        manager.add_image(&image_path, "Image 1".to_string()).unwrap();
        
        // Second add with same content should fail
        let result = manager.add_image(&image_path, "Image 2".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::ValidationError(_)));
    }

    #[test]
    fn test_remove_image() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let images_dir = temp_dir.path().join("images");
        fs::create_dir_all(&source_dir).unwrap();
        
        let image_path = create_test_image_file(&source_dir, "test.png", ImageFormat::Png);
        
        let manager = ImageLibraryManager::new(&images_dir).unwrap();
        let metadata = manager.add_image(&image_path, "Test".to_string()).unwrap();
        
        assert!(manager.remove_image(&metadata.id).unwrap());
        assert_eq!(manager.count().unwrap(), 0);
        assert!(!manager.get_image(&metadata.id).unwrap().is_some());
    }

    #[test]
    fn test_rename_image() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let images_dir = temp_dir.path().join("images");
        fs::create_dir_all(&source_dir).unwrap();
        
        let image_path = create_test_image_file(&source_dir, "test.png", ImageFormat::Png);
        
        let manager = ImageLibraryManager::new(&images_dir).unwrap();
        let metadata = manager.add_image(&image_path, "Original".to_string()).unwrap();
        
        assert!(manager.rename_image(&metadata.id, "Renamed".to_string()).unwrap());
        
        let updated = manager.get_image(&metadata.id).unwrap().unwrap();
        assert_eq!(updated.name, "Renamed");
    }

    #[test]
    fn test_list_images() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let images_dir = temp_dir.path().join("images");
        fs::create_dir_all(&source_dir).unwrap();
        
        // Create two images with different horizontal gradient patterns
        // Horizontal gradient (increasing)
        let mut img1 = ImageBuffer::from_pixel(100, 100, Rgb([0u8, 0, 0]));
        for y in 0..100 {
            for x in 0..100 {
                let pixel = img1.get_pixel_mut(x, y);
                *pixel = Rgb([(x * 255 / 100) as u8, 0, 0]);
            }
        }
        let png_path = source_dir.join("test.png");
        img1.save_with_format(&png_path, image::ImageFormat::Png).unwrap();
        
        // Checkerboard pattern (alternating columns)
        let mut img2 = ImageBuffer::from_pixel(100, 100, Rgb([0u8, 0, 0]));
        for y in 0..100 {
            for x in 0..100 {
                let pixel = img2.get_pixel_mut(x, y);
                *pixel = Rgb([if x % 2 == 0 { 255 } else { 0 }, 0, 0]);
            }
        }
        let bmp_path = source_dir.join("test.bmp");
        img2.save_with_format(&bmp_path, image::ImageFormat::Bmp).unwrap();
        
        let manager = ImageLibraryManager::new(&images_dir).unwrap();
        manager.add_image(&png_path, "PNG Image".to_string()).unwrap();
        manager.add_image(&bmp_path, "BMP Image".to_string()).unwrap();
        
        let images = manager.list_images().unwrap();
        assert_eq!(images.len(), 2);
    }

    #[test]
    fn test_persistence() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let images_dir = temp_dir.path().join("images");
        fs::create_dir_all(&source_dir).unwrap();
        
        let image_path = create_test_image_file(&source_dir, "test.png", ImageFormat::Png);
        
        // Create manager and add image
        {
            let manager = ImageLibraryManager::new(&images_dir).unwrap();
            manager.add_image(&image_path, "Persistent Image".to_string()).unwrap();
        }
        
        // Create new manager and verify persistence
        {
            let manager = ImageLibraryManager::new(&images_dir).unwrap();
            assert_eq!(manager.count().unwrap(), 1);
            
            let images = manager.list_images().unwrap();
            assert_eq!(images[0].name, "Persistent Image");
        }
    }

    #[test]
    fn test_unsupported_format() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let images_dir = temp_dir.path().join("images");
        fs::create_dir_all(&source_dir).unwrap();
        
        // Create a non-image file
        let txt_path = source_dir.join("test.txt");
        let mut file = fs::File::create(&txt_path).unwrap();
        file.write_all(b"not an image").unwrap();
        
        let manager = ImageLibraryManager::new(&images_dir).unwrap();
        let result = manager.add_image(&txt_path, "Text File".to_string());
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::ImageError(_)));
    }

    #[test]
    fn test_set_image_roi_persists() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let images_dir = temp_dir.path().join("images");
        fs::create_dir_all(&source_dir).unwrap();

        let image_path = create_test_image_file(&source_dir, "roi.png", ImageFormat::Png);
        let manager = ImageLibraryManager::new(&images_dir).unwrap();
        let metadata = manager.add_image(&image_path, "roi".to_string()).unwrap();

        let region = crate::models::Region { x: 12, y: 34, width: 100, height: 100 };
        assert!(manager.set_image_roi(&metadata.id, Some(region)).unwrap());
        assert_eq!(manager.get_image(&metadata.id).unwrap().unwrap().roi, Some(region));

        // 重新从磁盘加载后仍然保留
        let reopened = ImageLibraryManager::new(&images_dir).unwrap();
        assert_eq!(reopened.get_image(&metadata.id).unwrap().unwrap().roi, Some(region));

        assert!(manager.set_image_roi(&metadata.id, None).unwrap());
        assert_eq!(manager.get_image(&metadata.id).unwrap().unwrap().roi, None);
    }

    #[test]
    fn test_clear_images_removes_files_and_metadata() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let images_dir = temp_dir.path().join("images");
        fs::create_dir_all(&source_dir).unwrap();

        // 纯色图会被感知哈希判成同一张（哈希只看结构不看色值），
        // 所以第二张用渐变图案。
        let a = create_test_image_file_with_color(
            &source_dir, "a.png", ImageFormat::Png, Rgb([10, 20, 30]));
        let b = source_dir.join("b.png");
        let mut pattern: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(100, 100);
        for y in 0..100u32 {
            for x in 0..100u32 {
                let mut v = x.wrapping_mul(2654435761) ^ y.wrapping_mul(40503);
                v ^= v >> 13;
                v = v.wrapping_mul(1274126177);
                v ^= v >> 16;
                pattern.put_pixel(x, y, Rgb([v as u8, (v >> 8) as u8, (v >> 16) as u8]));
            }
        }
        pattern.save_with_format(&b, image::ImageFormat::Png).unwrap();

        let manager = ImageLibraryManager::new(&images_dir).unwrap();
        manager.add_image(&a, "a".to_string()).unwrap();
        manager.add_image(&b, "b".to_string()).unwrap();
        assert_eq!(manager.count().unwrap(), 2);

        let count_png = || {
            fs::read_dir(&images_dir)
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().extension().map(|ext| ext == "png").unwrap_or(false))
                .count()
        };
        assert_eq!(count_png(), 2);

        assert_eq!(manager.clear_images().unwrap(), 2);
        assert_eq!(manager.count().unwrap(), 0);
        assert_eq!(count_png(), 0);

        // 元数据文件也清干净了
        let reopened = ImageLibraryManager::new(&images_dir).unwrap();
        assert_eq!(reopened.count().unwrap(), 0);
    }

    #[test]
    fn test_two_managers_do_not_clobber_each_other() {
        // GUI 和 vadctl 各自持有一份内存快照；不重读磁盘的话后写的会覆盖对方。
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let images_dir = temp_dir.path().join("images");
        fs::create_dir_all(&source_dir).unwrap();

        let a = create_test_image_file_with_color(
            &source_dir, "a.png", ImageFormat::Png, Rgb([10, 20, 30]));
        let b = source_dir.join("b.png");
        let mut pattern: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(100, 100);
        for y in 0..100u32 {
            for x in 0..100u32 {
                let mut v = x.wrapping_mul(2654435761) ^ y.wrapping_mul(40503);
                v ^= v >> 13;
                v = v.wrapping_mul(1274126177);
                v ^= v >> 16;
                pattern.put_pixel(x, y, Rgb([v as u8, (v >> 8) as u8, (v >> 16) as u8]));
            }
        }
        pattern.save_with_format(&b, image::ImageFormat::Png).unwrap();

        let first = ImageLibraryManager::new(&images_dir).unwrap();
        first.add_image(&a, "a".to_string()).unwrap();

        let second = ImageLibraryManager::new(&images_dir).unwrap();
        second.add_image(&b, "b".to_string()).unwrap();

        // first 还没重读过，但任何写操作/列举前都会重读，所以能看到 second 加的那条。
        assert_eq!(first.list_images().unwrap().len(), 2);

        let reopened = ImageLibraryManager::new(&images_dir).unwrap();
        assert_eq!(reopened.list_images().unwrap().len(), 2);
    }

    #[test]
    fn test_flat_images_with_different_colors_are_both_kept() {
        // dHash 只看结构，两张纯色图的哈希相同；去重不能因此把它们当成一张。
        let temp_dir = tempdir().unwrap();
        let images_dir = temp_dir.path().join("images");
        let manager = ImageLibraryManager::new(&images_dir).unwrap();

        let encode = |rgb: Rgb<u8>| {
            let img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_pixel(40, 20, rgb);
            let mut buf = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
                .unwrap();
            (img, buf)
        };

        let (red_img, red_bytes) = encode(Rgb([200, 30, 40]));
        let (blue_img, blue_bytes) = encode(Rgb([30, 60, 200]));
        assert_eq!(
            crate::core::image_library::compute_image_hash(&image::DynamicImage::ImageRgb8(red_img)).unwrap(),
            crate::core::image_library::compute_image_hash(&image::DynamicImage::ImageRgb8(blue_img)).unwrap(),
            "前提：两张纯色图的 dHash 相同"
        );

        manager
            .add_image_from_bytes(&red_bytes, "red".to_string(), ImageFormat::Png)
            .unwrap();
        manager
            .add_image_from_bytes(&blue_bytes, "blue".to_string(), ImageFormat::Png)
            .unwrap();
        assert_eq!(manager.count().unwrap(), 2);
    }

    #[test]
    fn test_clear_images_removes_orphan_files() {
        let temp_dir = tempdir().unwrap();
        let images_dir = temp_dir.path().join("images");
        fs::create_dir_all(&images_dir).unwrap();
        // 不在元数据里的孤儿文件
        let orphan = images_dir.join("orphan.png");
        fs::write(&orphan, b"not really a png").unwrap();

        let manager = ImageLibraryManager::new(&images_dir).unwrap();
        assert_eq!(manager.clear_images().unwrap(), 0);
        assert!(!orphan.exists(), "清空应把没登记的图片文件一起删掉");
    }
}
