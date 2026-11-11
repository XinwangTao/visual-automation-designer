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
  replaceImageFromScreen,
  saveScreenCapture,
  setImageRoi,
  showScreenCaptureWindow,
  type ScreenPreview,
} from '../../tauri/commands';
import { getEditTarget, setEditTarget } from '../../tauri/captureIntent';
import { toError } from '../../utils/error';
import styles from './CaptureOverlay.module.css';

type Handle = 'n' | 's' | 'e' | 'w' | 'nw' | 'ne' | 'sw' | 'se';

/** 命中某个框的 8 个手柄之一（留 10px 容差）。 */
function hitHandle(box: Selection, x: number, y: number): Handle | null {
  const TOL = 10;
  if (x < box.x - TOL || x > box.x + box.w + TOL || y < box.y - TOL || y > box.y + box.h + TOL) {
    return null;
  }
  const nearL = Math.abs(x - box.x) <= TOL;
  const nearR = Math.abs(x - (box.x + box.w)) <= TOL;
  const nearT = Math.abs(y - box.y) <= TOL;
  const nearB = Math.abs(y - (box.y + box.h)) <= TOL;
  const vertical = nearT ? 'n' : nearB ? 's' : '';
  const horizontal = nearL ? 'w' : nearR ? 'e' : '';
  if (vertical && horizontal) return `${vertical}${horizontal}` as Handle;
  if (vertical) return vertical as Handle;
  if (horizontal) return horizontal as Handle;
  return null;
}

/** 按手柄方向拉伸，对边固定；最小 12px。 */
function resizeBox(
  origin: Selection,
  handle: Handle,
  dx: number,
  dy: number,
  bounds: { w: number; h: number },
): Selection {
  let left = origin.x;
  let top = origin.y;
  let right = origin.x + origin.w;
  let bottom = origin.y + origin.h;
  const MIN = 12;
  if (handle.includes('w')) left = Math.max(0, Math.min(origin.x + dx, right - MIN));
  if (handle.includes('e')) right = Math.min(bounds.w, Math.max(origin.x + origin.w + dx, left + MIN));
  if (handle.includes('n')) top = Math.max(0, Math.min(origin.y + dy, bottom - MIN));
  if (handle.includes('s')) bottom = Math.min(bounds.h, Math.max(origin.y + origin.h + dy, top + MIN));
  return { x: left, y: top, w: right - left, h: bottom - top };
}

interface Selection {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** 手柄在框内的绝对位置（百分比定位，随框缩放）。 */
function handleStyle(handle: Handle): React.CSSProperties {
  const pos: Record<Handle, React.CSSProperties> = {
    nw: { left: 0, top: 0 },
    n: { left: '50%', top: 0 },
    ne: { left: '100%', top: 0 },
    e: { left: '100%', top: '50%' },
    se: { left: '100%', top: '100%' },
    s: { left: '50%', top: '100%' },
    sw: { left: 0, top: '100%' },
    w: { left: 0, top: '50%' },
  };
  return { ...pos[handle], transform: 'translate(-50%, -50%)' };
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
  /** 光标位置（CSS 像素），用来画十字辅助线和坐标提示。 */
  const [pointer, setPointer] = useState<{ x: number; y: number } | null>(null);
  const [saving, setSaving] = useState(false);
  const imgRef = useRef<HTMLImageElement>(null);
  const dragRef = useRef<{ startX: number; startY: number; dragging: boolean } | null>(null);
  /** 编辑模式：图片框与 ROI 框各自独立，拖动谁就动谁。 */
  const [editBoxes, setEditBoxes] = useState<{ image: Selection; roi: Selection } | null>(null);
  const dragBoxRef = useRef<{
    kind: 'image' | 'roi';
    handle: Handle | null;
    startX: number;
    startY: number;
    origin: Selection;
  } | null>(null);
  /** 刚打开编辑时的两个框，Ctrl+Z 用来复位。 */
  const initialBoxesRef = useRef<{ image: Selection; roi: Selection } | null>(null);

  const editTarget = useRef(getEditTarget());
  const isEdit = editTarget.current !== null;

  useEffect(() => {
    let cancelled = false;
    // 按屏幕物理像素请求，避免 4K 屏上整屏放大发糊。窗口刚被拉成全屏时
    // innerWidth 可能还是旧值，所以同时参考 screen 尺寸。
    // 1:1 截图 + JPEG 编码：后端缩放（resize_exact）反而比编码更贵，
    // 显示缩放交给浏览器做。坐标按 preview.width 与渲染宽度换算，不受影响。
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

      if (editBoxes) {
        // 编辑模式：落在哪个框里就拖哪个，ROI 在上层优先命中。
        const inside = (box: Selection) =>
          point.x >= box.x && point.x <= box.x + box.w && point.y >= box.y && point.y <= box.y + box.h;

        // 手柄优先（ROI 在上层），其次是框内拖动。
        for (const kind of ['roi', 'image'] as const) {
          const box = editBoxes[kind];
          const handle = hitHandle(box, point.x, point.y);
          if (handle) {
            event.currentTarget.setPointerCapture(event.pointerId);
            dragBoxRef.current = { kind, handle, startX: point.x, startY: point.y, origin: { ...box } };
            return;
          }
        }
        const kind = inside(editBoxes.roi) ? 'roi' : inside(editBoxes.image) ? 'image' : null;
        if (!kind) return;
        event.currentTarget.setPointerCapture(event.pointerId);
        dragBoxRef.current = {
          kind,
          handle: null,
          startX: point.x,
          startY: point.y,
          origin: { ...editBoxes[kind] },
        };
        return;
      }

      event.currentTarget.setPointerCapture(event.pointerId);
      dragRef.current = { startX: point.x, startY: point.y, dragging: true };
      setSelection({ x: point.x, y: point.y, w: 0, h: 0 });
    },
    [editBoxes, pointFromEvent, preview, saving],
  );

  const handlePointerMove = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      const point = pointFromEvent(event);
      setPointer({ x: point.x, y: point.y });

      const boxDrag = dragBoxRef.current;
      if (boxDrag) {
        const { rect } = point;
        const dx = point.x - boxDrag.startX;
        const dy = point.y - boxDrag.startY;
        const next = boxDrag.handle
          ? resizeBox(boxDrag.origin, boxDrag.handle, dx, dy, { w: rect.width, h: rect.height })
          : {
              ...boxDrag.origin,
              x: Math.min(
                Math.max(0, boxDrag.origin.x + dx),
                Math.max(0, rect.width - boxDrag.origin.w),
              ),
              y: Math.min(
                Math.max(0, boxDrag.origin.y + dy),
                Math.max(0, rect.height - boxDrag.origin.h),
              ),
            };
        setEditBoxes((prev) => (prev ? { ...prev, [boxDrag.kind]: next } : prev));
        return;
      }

      const drag = dragRef.current;
      if (!drag?.dragging) return;
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
    dragBoxRef.current = null;
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

  /** 光标位置的屏幕绝对坐标（用于实时提示）。 */
  const pointerScreen = (() => {
    if (!preview || !pointer || !imgRef.current) return null;
    const rect = imgRef.current.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return null;
    return {
      x: preview.originX + Math.round(pointer.x * (preview.width / rect.width)),
      y: preview.originY + Math.round(pointer.y * (preview.height / rect.height)),
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
    if (saving) return;
    const target = editTarget.current;

    if (target) {
      // 编辑模式：两个框各自的屏幕坐标。图片框动过就重裁内容，
      // 只动 ROI 就只更新搜索区域。
      if (!editBoxes || !imgRef.current || !preview) return;
      const rect = imgRef.current.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) return;
      const scaleX = preview.width / rect.width;
      const scaleY = preview.height / rect.height;
      const toScreen = (box: Selection) => ({
        x: Math.round(box.x * scaleX) + preview.originX,
        y: Math.round(box.y * scaleY) + preview.originY,
        width: Math.max(1, Math.round(box.w * scaleX)),
        height: Math.max(1, Math.round(box.h * scaleY)),
      });
      const imageRect = toScreen(editBoxes.image);
      const roiRect = toScreen(editBoxes.roi);
      const originSource = target.source ?? target.roi;
      const moved =
        !originSource ||
        Math.abs(originSource.x - imageRect.x) > 1 ||
        Math.abs(originSource.y - imageRect.y) > 1 ||
        Math.abs(originSource.width - imageRect.width) > 1 ||
        Math.abs(originSource.height - imageRect.height) > 1;

      setSaving(true);
      setError(null);
      try {
        if (moved) {
          await replaceImageFromScreen({ id: target.id, ...imageRect, roi: roiRect });
        } else {
          await setImageRoi(target.id, roiRect);
          await cancelScreenCapture();
        }
        setEditTarget(null);
      } catch (err) {
        setError(toError(err).message || '应用调整失败');
        setSaving(false);
      }
      return;
    }

    if (!region) return;
    setSaving(true);
    setError(null);
    try {
      await saveScreenCapture({ name: defaultName(), ...region });
      setEditTarget(null);
      // 后端保存后会关闭本窗口；这里不再 setState。
    } catch (err) {
      setError(toError(err).message || '保存截图失败');
      setSaving(false);
    }
  }, [editBoxes, preview, region, saving]);

  const handleCancel = useCallback(async () => {
    try {
      setEditTarget(null);
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
      } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'z') {
        // 编辑模式：Ctrl+Z 把两个框恢复到刚打开时的位置
        if (initialBoxesRef.current) {
          event.preventDefault();
          setEditBoxes(initialBoxesRef.current);
        }
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
          src={`data:${preview.mime ?? 'image/png'};base64,${preview.data}`}
          alt="桌面截图"
          draggable={false}
          onLoad={() => {
            void showScreenCaptureWindow();
            // 编辑模式：把「图片来源框」和「搜索 ROI」各自铺出来，直接拖动。
            const target = editTarget.current;
            const node = imgRef.current;
            if (target && node) {
              const rect = node.getBoundingClientRect();
              if (rect.width > 0 && rect.height > 0) {
                const scaleX = rect.width / preview.width;
                const scaleY = rect.height / preview.height;
                const toBox = (r: { x: number; y: number; width: number; height: number }): Selection => ({
                  x: (r.x - preview.originX) * scaleX,
                  y: (r.y - preview.originY) * scaleY,
                  w: r.width * scaleX,
                  h: r.height * scaleY,
                });
                const source = target.source ?? target.roi;
                const roi = target.roi ?? target.source;
                if (source && roi) {
                  const initial = { image: toBox(source), roi: toBox(roi) };
                  initialBoxesRef.current = initial;
                  setEditBoxes(initial);
                }
              }
            }
          }}
        />
      )}
      {preview && pointer && !saving && (
        <>
          <div className={styles.crosshairV} style={{ left: pointer.x }} />
          <div className={styles.crosshairH} style={{ top: pointer.y }} />
          {pointerScreen && (
            <div
              className={styles.cursorChip}
              style={{
                left: Math.min(pointer.x + 16, window.innerWidth - 120),
                top: Math.min(pointer.y + 16, window.innerHeight - 36),
              }}
            >
              {pointerScreen.x}, {pointerScreen.y}
            </div>
          )}
        </>
      )}
      {isEdit && editBoxes && (
        <>
          <div
            className={styles.imageBox}
            style={(() => {
              // 两个框完全重合时，把图片框外扩几像素，否则红框会把蓝框完全盖住。
              const a = editBoxes.image;
              const b = editBoxes.roi;
              const same =
                Math.abs(a.x - b.x) < 2 &&
                Math.abs(a.y - b.y) < 2 &&
                Math.abs(a.w - b.w) < 2 &&
                Math.abs(a.h - b.h) < 2;
              const pad = same ? 3 : 0;
              return {
                left: a.x - pad,
                top: a.y - pad,
                width: a.w + pad * 2,
                height: a.h + pad * 2,
              };
            })()}
            data-testid="edit-image-box"
          >
            <span className={styles.boxLabel}>图片</span>
            {(['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'] as Handle[]).map((handle) => (
              <span
                key={handle}
                className={`${styles.editHandle} ${styles.editHandleImage}`}
                data-handle={handle}
                style={handleStyle(handle)}
              />
            ))}
          </div>
          <div
            className={styles.roiBox}
            style={{
              left: editBoxes.roi.x,
              top: editBoxes.roi.y,
              width: editBoxes.roi.w,
              height: editBoxes.roi.h,
            }}
            data-testid="edit-roi-box"
          >
            <span className={`${styles.boxLabel} ${styles.boxLabelRoi}`}>ROI</span>
            {(['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'] as Handle[]).map((handle) => (
              <span
                key={handle}
                className={`${styles.editHandle} ${styles.editHandleRoi}`}
                data-handle={handle}
                style={handleStyle(handle)}
              />
            ))}
          </div>
        </>
      )}
      {!isEdit && selection && (
        <div className={styles.selection} style={selectionStyle}>
          <span className={`${styles.handle} ${styles.handleNw}`} />
          <span className={`${styles.handle} ${styles.handleNe}`} />
          <span className={`${styles.handle} ${styles.handleSw}`} />
          <span className={`${styles.handle} ${styles.handleSe}`} />
        </div>
      )}
      {!isEdit && selection && screenRegion && (
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
          {isEdit
            ? '拖动「图片」框改内容位置，拖动红框改搜索 ROI · Enter 应用 · Esc 取消'
            : region
              ? `${region.width} × ${region.height} px · 屏幕 ${screenRegion?.x ?? 0}, ${screenRegion?.y ?? 0} · Enter 保存 · Esc 取消`
              : '拖拽框选区域 · Esc 取消'}
          {error ? ` · ${error}` : ''}
        </div>
      )}
    </div>
  );
}

export default CaptureOverlay;
