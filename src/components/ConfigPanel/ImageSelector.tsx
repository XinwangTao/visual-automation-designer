/**
 * ImageSelector - 积木块里的图片选择器。
 *
 * 不再把整库图片摊在配置面板里（图一多就只能一张张删），改为：
 * 只展示当前选中的那张，点「选择图片」从右侧抽屉里筛选、挑选、管理。
 */

import { useCallback, useEffect, useState } from 'react';
import { useImageLibrary } from '../../hooks/useImageLibrary';
import {
  openScreenCapture,
  onScreenCaptureSaved,
  type ScreenPoint,
  type ScreenRegion,
} from '../../tauri/commands';
import { toError } from '../../utils/error';
import { isRoiPick } from '../../tauri/captureIntent';
import { ImageLibraryDrawer } from '../ImageLibrary/ImageLibraryDrawer';
import { ImageThumb } from '../common/ImageThumb';

export interface ImageSelectorProps {
  /** Currently selected image ID */
  selectedId?: string;
  /** Callback when an image is selected */
  onSelect?: (imageId: string) => void;
  /** Placeholder text for search input（保留兼容，抽屉内自带筛选） */
  searchPlaceholder?: string;
  /** Whether to show upload button when empty */
  showUploadButton?: boolean;
  /** 兼容旧参数：删除统一放到抽屉里 */
  showDeleteButton?: boolean;
  /** Whether to show paste button */
  showPasteButton?: boolean;
  /** Custom empty state message */
  emptyMessage?: string;
  /** Custom class name */
  className?: string;
  /** Called after the built-in screen capture adds an image (returns ROI/center) */
  onRegionSelect?: (imageId: string, roi: ScreenRegion, center: ScreenPoint) => void;
}

export function ImageSelector({
  selectedId,
  onSelect,
  showUploadButton = true,
  showPasteButton = true,
  emptyMessage = '还没有选择图片',
  className = '',
  onRegionSelect,
}: ImageSelectorProps) {
  const {
    images,
    refreshImages,
    selectAndUploadImage,
    pasteImageFromClipboard,
  } = useImageLibrary();
  const [isUploading, setIsUploading] = useState(false);
  const [isPasting, setIsPasting] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);

  const selected = images.find((image) => image.id === selectedId) ?? null;

  // 全屏框选窗口保存后会广播事件，这里刷新图片库并自动选中新图。
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    onScreenCaptureSaved((captured) => {
      void refreshImages();
      // 图片库只是在给已有图片重设 ROI：那张临时图会被删掉，不能写进块配置。
      if (isRoiPick()) return;
      onSelect?.(captured.image.id);
      onRegionSelect?.(captured.image.id, captured.roi, captured.center);
    })
      .then((dispose) => {
        if (disposed) dispose();
        else unlisten = dispose;
      })
      .catch(() => {
        // 浏览器模式没有 Tauri 事件，忽略。
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refreshImages, onSelect, onRegionSelect]);

  // 抽屉里改名/改 ROI/删除后，这里要跟着刷新，否则显示的还是旧信息。
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const onChange = () => void refreshImages();
    window.addEventListener('vad:image-library-changed', onChange);
    return () => window.removeEventListener('vad:image-library-changed', onChange);
  }, [refreshImages]);

  const handleUpload = useCallback(async () => {
    setIsUploading(true);
    try {
      const metadata = await selectAndUploadImage();
      if (metadata) onSelect?.(metadata.id);
    } catch (err) {
      console.error('Failed to upload image:', err);
    } finally {
      setIsUploading(false);
    }
  }, [selectAndUploadImage, onSelect]);

  const handlePaste = useCallback(async () => {
    setIsPasting(true);
    try {
      const metadata = await pasteImageFromClipboard();
      if (metadata) onSelect?.(metadata.id);
    } catch {
      console.error('Failed to paste image');
    } finally {
      setIsPasting(false);
    }
  }, [pasteImageFromClipboard, onSelect]);

  const handleCapture = useCallback(() => {
    void openScreenCapture().catch((err) => {
      console.error(toError(err).message || '打开屏幕截取失败');
    });
  }, []);

  return (
    <div className={`image-selector ${className}`} data-testid="image-selector">
      {selected ? (
        <button
          type="button"
          className="image-selector__current"
          onClick={() => setDrawerOpen(true)}
          data-testid="image-selector-current"
          title="点击更换图片"
        >
          <span className="image-selector__thumb">
            <ImageThumb id={selected.id} alt={selected.name} maxSize={120} className="image-selector__thumb-img" />
          </span>
          <span className="image-selector__text">
            <span className="image-selector__name">{selected.name}</span>
            <span className="image-selector__meta">
              {selected.width}×{selected.height}
              {selected.roi ? ` · ${selected.roi.x},${selected.roi.y}` : ''}
            </span>
          </span>
          <span className="image-selector__change">更换</span>
        </button>
      ) : (
        <div className="image-selector__empty">
          <span>{emptyMessage}</span>
        </div>
      )}

      <div className="image-selector__actions">
        <button
          type="button"
          className="image-selector-capture-btn image-selector__btn image-selector__btn--primary"
          onClick={handleCapture}
          data-testid="btn-capture-screen"
          title="冻结屏幕并拖拽框选目标，自动记录坐标与 ROI"
        >
          从屏幕截取
        </button>
        <button
          type="button"
          className="image-selector__btn"
          onClick={() => setDrawerOpen(true)}
          data-testid="btn-open-image-drawer"
        >
          选择图片
        </button>
        {showUploadButton && (
          <button
            type="button"
            className="image-selector__btn"
            onClick={() => void handleUpload()}
            disabled={isUploading}
            data-testid="btn-upload-image"
          >
            {isUploading ? '上传中…' : '上传'}
          </button>
        )}
        {showPasteButton && (
          <button
            type="button"
            className="image-selector__btn"
            onClick={() => void handlePaste()}
            disabled={isPasting}
            data-testid="btn-paste-image"
            title="从剪贴板粘贴图片 (Ctrl+V)"
          >
            {isPasting ? '粘贴中…' : '粘贴'}
          </button>
        )}
      </div>

      <ImageLibraryDrawer
        open={drawerOpen}
        mode="select"
        selectedId={selectedId ?? null}
        onClose={() => setDrawerOpen(false)}
        onSelect={(id) => onSelect?.(id)}
        onChanged={() => void refreshImages()}
      />
    </div>
  );
}

export default ImageSelector;
