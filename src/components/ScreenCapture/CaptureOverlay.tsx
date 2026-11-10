/**
 * CaptureOverlay — 全屏框选页（独立 Tauri 窗口，覆盖整个虚拟桌面）。
 *
 * 流程：后端冻结桌面 → 本页拉取预览并渲染 → 显示窗口 → 用户拖拽框选 →
 * Enter/双击保存（后端从同一张冻结图裁剪）或 Esc 取消。
 * 坐标以冻结图左上角为原点，按图片实际像素换算后交给后端。
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import {
  cancelScreenCapture,
  getScreenCapturePreview,
  saveScreenCapture,
  showScreenCaptureWindow,
  type ScreenPreview,
} from '../../tauri/commands';
import { toError } from '../../utils/error';
import styles from './CaptureOverlay.module.css';

interface Selection {
  x: number;
  y: number;
  w: number;
  h: number;
}

function defaultName(): string {
  const stamp = new Date()
    .toISOString()
    .slice(0, 19)
    .replace(/[:T]/g, '-');
  return `capture_${stamp}`;
}

export function CaptureOverlay() {
  const [preview, setPreview] = useState<ScreenPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const [saving, setSaving] = useState(false);
  const imgRef = useRef<HTMLImageElement>(null);
  const dragRef = useRef<{ startX: number; startY: number; dragging: boolean } | null>(null);

  useEffect(() => {
    let cancelled = false;
    // 按屏幕物理像素请求，避免 4K 屏上整屏放大发糊。窗口刚被拉成全屏时
    // innerWidth 可能还是旧值，所以同时参考 screen 尺寸。
    const dpr = window.devicePixelRatio || 1;
    const maxWidth = Math.min(
      7680,
      Math.round(Math.max(window.innerWidth, window.screen?.width ?? 0) * dpr),
    );
    getScreenCapturePreview(maxWidth)
      .then((next) => {
        if (!cancelled) setPreview(next);
      })
      .catch((err) => {
        if (!cancelled) setError(toError(err).message || '截取屏幕失败');
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const pointFromEvent = useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return {
      x: Math.max(0, Math.min(rect.width, event.clientX - rect.left)),
      y: Math.max(0, Math.min(rect.height, event.clientY - rect.top)),
      rect,
    };
  }, []);

  const handlePointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (saving || !preview) return;
      const point = pointFromEvent(event);
      event.currentTarget.setPointerCapture(event.pointerId);
      dragRef.current = { startX: point.x, startY: point.y, dragging: true };
      setSelection({ x: point.x, y: point.y, w: 0, h: 0 });
    },
    [pointFromEvent, preview, saving],
  );

  const handlePointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const drag = dragRef.current;
      if (!drag?.dragging) return;
      const point = pointFromEvent(event);
      setSelection({
        x: Math.min(drag.startX, point.x),
        y: Math.min(drag.startY, point.y),
        w: Math.abs(point.x - drag.startX),
        h: Math.abs(point.y - drag.startY),
      });
    },
    [pointFromEvent],
  );

  const handlePointerUp = useCallback(() => {
    if (dragRef.current) dragRef.current.dragging = false;
    setSelection((current) =>
      current && current.w >= 6 && current.h >= 6 ? current : null,
    );
  }, []);

  /** 屏幕（CSS 像素）选区 → 冻结图物理像素。 */
  const region = (() => {
    if (!preview || !selection || !imgRef.current) return null;
    const rect = imgRef.current.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return null;
    const scaleX = preview.width / rect.width;
    const scaleY = preview.height / rect.height;
    return {
      x: Math.max(0, Math.round(selection.x * scaleX)),
      y: Math.max(0, Math.round(selection.y * scaleY)),
      width: Math.max(1, Math.round(selection.w * scaleX)),
      height: Math.max(1, Math.round(selection.h * scaleY)),
    };
  })();

  /** 冻结图坐标 → 虚拟桌面绝对坐标，和配置面板里的 ROI 一一对应。 */
  const screenRegion = region && preview
    ? {
        x: region.x + preview.originX,
        y: region.y + preview.originY,
        width: region.width,
        height: region.height,
      }
    : null;

  const handleSave = useCallback(async () => {
    if (!region || saving) return;
    setSaving(true);
    setError(null);
    try {
      await saveScreenCapture({ name: defaultName(), ...region });
      // 后端保存后会关闭本窗口；这里不再 setState。
    } catch (err) {
      setError(toError(err).message || '保存截图失败');
      setSaving(false);
    }
  }, [region, saving]);

  const handleCancel = useCallback(async () => {
    try {
      await cancelScreenCapture();
    } catch {
      // 后端不可用时用户仍可用 Alt+F4 关闭；不在这里直接操作窗口，避免依赖额外权限。
    }
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        void handleCancel();
      } else if (event.key === 'Enter') {
        event.preventDefault();
        void handleSave();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [handleCancel, handleSave]);

  const selectionStyle = selection
    ? {
        left: selection.x,
        top: selection.y,
        width: selection.w,
        height: selection.h,
      }
    : undefined;

  return (
    <div
      className={styles.overlay}
      data-testid="capture-overlay"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerLeave={handlePointerUp}
    >
      {!preview && (
        <div className={styles.status} role="status">
          {error ?? '正在冻结屏幕…'}
        </div>
      )}
      {preview && (
        <img
          ref={imgRef}
          className={styles.image}
          src={`data:image/png;base64,${preview.data}`}
          alt="桌面截图"
          draggable={false}
          onLoad={() => {
            void showScreenCaptureWindow();
          }}
        />
      )}
      {selection && <div className={styles.selection} style={selectionStyle} />}
      {selection && screenRegion && (
        <div
          className={styles.badge}
          style={{
            left: selection.x,
            top:
              selection.y + selection.h + 40 > window.innerHeight
                ? Math.max(0, selection.y - 30)
                : selection.y + selection.h + 8,
          }}
        >
          {screenRegion.width} × {screenRegion.height} · ({screenRegion.x}, {screenRegion.y})
        </div>
      )}
      {preview && (
        <div className={styles.hint} role="status">
          {region
            ? `${region.width} × ${region.height} px · 屏幕 ${screenRegion?.x ?? 0}, ${screenRegion?.y ?? 0} · Enter 保存 · Esc 取消`
            : '拖拽框选区域 · Esc 取消'}
          {error ? ` · ${error}` : ''}
        </div>
      )}
    </div>
  );
}

export default CaptureOverlay;
