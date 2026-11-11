/**
 * ImageLibraryDrawer — 图片库抽屉。
 *
 * 两种用途共用一套界面：
 * - mode="select"：积木块里挑图片，点一行即选中并关闭；
 * - mode="manage"：顶部「图片库」进来，看缩略图、改名、改 ROI、删除、清空。
 *
 * 做成抽屉而不是铺满的网格，是因为图片多起来以后需要筛选着找，而不是一屏
 * 摊开逐张处理。
 */

import { useCallback, useEffect, useMemo, useState, type CSSProperties } from 'react';
import {
  captureScreenPreview,
  clearImageLibrary,
  listImages,
  onScreenCaptureClosed,
  onScreenCaptureSaved,
  openScreenCapture,
  removeImage,
  renameImage,
  setImageRoi,
  testImageTarget,
} from '../../tauri/commands';
import type { ImageMetadata, ScreenRect } from '../../types/image';
import type { DesktopBounds, ScreenPreview } from '../../tauri/commands';
import { beginRoiPick, endRoiPick, setEditTarget } from '../../tauri/captureIntent';
import { toError } from '../../utils/error';
import { ConfirmDialog } from '../common/ConfirmDialog';
import { ImageThumb, invalidateImageThumb } from '../common/ImageThumb';
import styles from './ImageLibraryDrawer.module.css';

export interface ImageLibraryDrawerProps {
  open: boolean;
  mode?: 'select' | 'manage';
  /** 当前已选中的图片（select 模式高亮用） */
  selectedId?: string | null;
  onClose: () => void;
  /** select 模式：点一行就回调 */
  onSelect?: (id: string) => void;
  onChanged?: () => void;
}

interface RoiDraft {
  x: string;
  y: string;
  width: string;
  height: string;
}

function draftFromRoi(roi?: ScreenRect): RoiDraft {
  return {
    x: roi ? String(roi.x) : '',
    y: roi ? String(roi.y) : '',
    width: roi ? String(roi.width) : '',
    height: roi ? String(roi.height) : '',
  };
}

function parseDraft(draft: RoiDraft): ScreenRect | null {
  // 空字段不能当 0：留空表示「没填」，静默变成 0 会把搜索区挪到屏幕左上角。
  if (![draft.x, draft.y, draft.width, draft.height].every((value) => value.trim() !== '')) {
    return null;
  }
  const x = Number(draft.x);
  const y = Number(draft.y);
  const width = Number(draft.width);
  const height = Number(draft.height);
  if (![x, y, width, height].every((n) => Number.isFinite(n))) return null;
  if (width < 1 || height < 1) return null;
  return {
    x: Math.round(x),
    y: Math.round(y),
    width: Math.round(width),
    height: Math.round(height),
  };
}

/** ROI 在桌面缩略图上的百分比位置。 */
function roiStyle(roi: ScreenRect, bounds: DesktopBounds): CSSProperties {
  const left = ((roi.x - bounds.originX) / bounds.width) * 100;
  const top = ((roi.y - bounds.originY) / bounds.height) * 100;
  return {
    left: `${left}%`,
    top: `${top}%`,
    // 小目标在缩略图上会小到看不见，给个最小可点击尺寸。
    width: `${Math.max(2.5, (roi.width / bounds.width) * 100)}%`,
    height: `${Math.max(3.5, (roi.height / bounds.height) * 100)}%`,
  };
}

function centrestyle(roi: ScreenRect, bounds: DesktopBounds): CSSProperties {
  return {
    left: `${((roi.x + roi.width / 2 - bounds.originX) / bounds.width) * 100}%`,
    top: `${((roi.y + roi.height / 2 - bounds.originY) / bounds.height) * 100}%`,
  };
}

export function ImageLibraryDrawer({
  open,
  mode = 'manage',
  selectedId = null,
  onClose,
  onSelect,
  onChanged,
}: ImageLibraryDrawerProps) {
  const [images, setImages] = useState<ImageMetadata[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [activeId, setActiveId] = useState<string | null>(null);
  const [nameDraft, setNameDraft] = useState('');
  const [roiDraft, setRoiDraft] = useState<RoiDraft>(draftFromRoi());
  const [confirm, setConfirm] = useState<'delete' | 'clear' | null>(null);
  const [busy, setBusy] = useState(false);
  const [picking, setPicking] = useState(false);
  const [pickedFrom, setPickedFrom] = useState<Set<string> | null>(null);
  const [testResult, setTestResult] = useState<string | null>(null);
  /** 真实屏幕截图（缩放后），用来核对 ROI 框在哪。 */
  const [screenShot, setScreenShot] = useState<ScreenPreview | null>(null);
  const [shotLoading, setShotLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const list = await listImages();
      setImages(list);
      setError(null);
      return list;
    } catch (err) {
      setError(toError(err).message || '读取图片库失败');
      return [];
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (!open) {
      setActiveId(null);
      return;
    }
    setQuery('');
    setTestResult(null);
    void refresh();
  }, [open, refresh]);

  const refreshShot = useCallback(async () => {
    setShotLoading(true);
    try {
      setScreenShot(await captureScreenPreview(1600));
    } catch {
      setScreenShot(null);
    } finally {
      setShotLoading(false);
    }
  }, []);

  // 展开某一行时抓一张当前屏幕，把 ROI 画在真实画面上。
  useEffect(() => {
    if (!open || !activeId) {
      setScreenShot(null);
      return;
    }
    void refreshShot();
  }, [open, activeId, refreshShot]);

  const active = useMemo(
    () => images.find((image) => image.id === activeId) ?? null,
    [images, activeId],
  );

  useEffect(() => {
    if (!active) {
      setNameDraft('');
      setRoiDraft(draftFromRoi());
      return;
    }
    setNameDraft(active.name);
    setRoiDraft(draftFromRoi(active.roi));
    setTestResult(null);
  }, [active?.id, active?.name, active?.roi]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return images;
    return images.filter((image) => image.name.toLowerCase().includes(q));
  }, [images, query]);

  const notifyChanged = useCallback(() => onChanged?.(), [onChanged]);

  /** 框选完成后把 ROI 写回当前图片，并清掉本次临时入库的图片。 */
  useEffect(() => {
    if (!picking || !activeId) return;
    let disposed = false;
    const disposers: Array<() => void> = [];
    const keep = (dispose: () => void) => {
      if (disposed) dispose();
      else disposers.push(dispose);
    };
    const targetId = activeId;

    void onScreenCaptureSaved(async (captured) => {
      setPicking(false);
      endRoiPick();
      try {
        await setImageRoi(targetId, captured.roi);
        const isFresh = !pickedFrom?.has(captured.image.id);
        if (isFresh && captured.image.id !== targetId) {
          await removeImage(captured.image.id).catch(() => undefined);
          invalidateImageThumb(captured.image.id);
        }
        await refresh();
        notifyChanged();
      } catch (err) {
        setError(toError(err).message || '写入 ROI 失败');
      }
    }).then(keep, () => undefined);

    void onScreenCaptureClosed(() => {
      setPicking(false);
      endRoiPick();
    }).then(keep, () => undefined);

    return () => {
      disposed = true;
      disposers.forEach((dispose) => dispose());
      // 抽屉中途关掉/组件卸载时也要复位，否则后续截屏都会被当成「只是重设 ROI」。
      if (picking) endRoiPick();
    };
  }, [picking, activeId, pickedFrom, refresh, notifyChanged]);

  /** select 模式：点一行就选中并关闭；manage 模式：展开编辑。 */
  const handleRowClick = (image: ImageMetadata) => {
    if (mode === 'select') {
      onSelect?.(image.id);
      onClose();
      return;
    }
    setActiveId((current) => (current === image.id ? null : image.id));
  };

  const handleRename = async () => {
    if (!active) return;
    const name = nameDraft.trim();
    if (!name || name === active.name) return;
    setBusy(true);
    try {
      await renameImage(active.id, name);
      await refresh();
      notifyChanged();
    } catch (err) {
      setError(toError(err).message || '改名失败');
    } finally {
      setBusy(false);
    }
  };

  const handleApplyRoi = async () => {
    if (!active) return;
    const roi = parseDraft(roiDraft);
    if (!roi) {
      setError('ROI 需要填写完整的 x / y / 宽 / 高（宽高必须大于 0）');
      return;
    }
    setBusy(true);
    try {
      await setImageRoi(active.id, roi);
      await refresh();
      setError(null);
      notifyChanged();
    } catch (err) {
      setError(toError(err).message || '写入 ROI 失败');
    } finally {
      setBusy(false);
    }
  };

  const handleClearRoi = async () => {
    if (!active) return;
    setBusy(true);
    try {
      await setImageRoi(active.id, null);
      await refresh();
      notifyChanged();
    } catch (err) {
      setError(toError(err).message || '清除 ROI 失败');
    } finally {
      setBusy(false);
    }
  };

  const handleTest = async () => {
    if (!active) return;
    setBusy(true);
    setTestResult('正在匹配…');
    try {
      const result = await testImageTarget(active.id, 0.7, active.roi ?? undefined);
      setTestResult(
        result.found
          ? `已找到 · 匹配 ${((result.score ?? 0) * 100).toFixed(1)}% · 用时 ${result.elapsedMs}ms`
          : `未找到 · 最高 ${((result.score ?? 0) * 100).toFixed(1)}%（阈值 70%）· 用时 ${result.elapsedMs}ms`,
      );
    } catch (err) {
      setTestResult(toError(err).message || '测试失败');
    } finally {
      setBusy(false);
    }
  };

  /**
   * 在屏幕上全屏查看并调整：遮罩里把这边的 ROI 作为初始选区，拖动即改位置/大小，
   * 确认后**原地重裁这张图**（内容和 ROI 一起更新），不会新增图片。
   */
  const handleEditOnScreen = async () => {
    if (!active) return;
    setError(null);
    setPickedFrom(new Set(images.map((image) => image.id)));
    setEditTarget({ id: active.id, source: active.source ?? active.roi, roi: active.roi });
    beginRoiPick();
    setPicking(true);
    try {
      await openScreenCapture();
    } catch (err) {
      setPicking(false);
      endRoiPick();
      setEditTarget(null);
      setError(toError(err).message || '打开屏幕截取失败');
    }
  };

  const handleDelete = async () => {
    if (!active) return;
    setBusy(true);
    try {
      await removeImage(active.id);
      invalidateImageThumb(active.id);
      await refresh();
      setActiveId(null);
      notifyChanged();
    } catch (err) {
      setError(toError(err).message || '删除失败');
    } finally {
      setBusy(false);
      setConfirm(null);
    }
  };

  const handleClearAll = async () => {
    setBusy(true);
    try {
      await clearImageLibrary();
      images.forEach((image) => invalidateImageThumb(image.id));
      await refresh();
      setActiveId(null);
      notifyChanged();
      setError(null);
    } catch (err) {
      setError(toError(err).message || '清空失败');
    } finally {
      setBusy(false);
      setConfirm(null);
    }
  };

  if (!open) return null;

  return (
    <div
      className={`${styles.overlay} ${picking ? styles.overlayHidden : ''}`}
      onClick={onClose}
      role="presentation"
    >
      <aside
        className={styles.drawer}
        role="dialog"
        aria-modal="true"
        aria-label="图片库"
        onClick={(event) => event.stopPropagation()}
        data-testid="image-library-drawer"
      >
        <header className={styles.header}>
          <h2 className={styles.title}>
            图片库 <span className={styles.count}>{images.length}</span>
          </h2>
          <button className={styles.closeBtn} type="button" onClick={onClose} aria-label="关闭">
            ×
          </button>
        </header>

        <div className={styles.toolbar}>
          <input
            className={styles.search}
            type="search"
            value={query}
            placeholder="按名称筛选…"
            onChange={(event) => setQuery(event.target.value)}
            data-testid="image-library-search"
          />
          {mode === 'manage' && (
            <button
              className={styles.dangerBtn}
              type="button"
              onClick={() => setConfirm('clear')}
              disabled={busy || images.length === 0}
              data-testid="image-library-clear"
            >
              清空
            </button>
          )}
        </div>

        {error && <div className={styles.error}>{error}</div>}

        <div className={styles.list} data-testid="image-library-list">
          {loading && images.length === 0 && <p className={styles.empty}>读取中…</p>}
          {!loading && filtered.length === 0 && (
            <p className={styles.empty}>
              {images.length === 0
                ? '还没有图片。用「从屏幕截取」或「上传」添加。'
                : '没有匹配的图片。'}
            </p>
          )}
          {filtered.map((image) => (
            <div key={image.id} className={styles.rowWrap}>
              <button
                type="button"
                className={`${styles.row} ${
                  (mode === 'select' ? selectedId : activeId) === image.id ? styles.rowActive : ''
                }`}
                onClick={() => handleRowClick(image)}
                data-testid={`image-row-${image.id}`}
                title={image.name}
              >
                <span className={styles.thumb}>
                  <ImageThumb id={image.id} alt={image.name} maxSize={120} className={styles.thumbImg} />
                </span>
                <span className={styles.rowText}>
                  <span className={styles.rowName}>{image.name}</span>
                  <span className={styles.rowMeta}>
                    {image.width}×{image.height}
                    {image.roi ? ` · ${image.roi.x},${image.roi.y}` : ' · 未设 ROI'}
                  </span>
                </span>
                {mode === 'manage' && (
                  <span className={styles.rowHint}>{activeId === image.id ? '收起' : '编辑'}</span>
                )}
                {mode === 'select' && selectedId === image.id && (
                  <span className={styles.rowCheck}>✓</span>
                )}
              </button>

              {mode === 'manage' && activeId === image.id && (
                <div className={styles.editor}>
                  <div className={styles.preview}>
                    <ImageThumb
                      id={image.id}
                      alt={image.name}
                      maxSize={720}
                      className={styles.previewImg}
                    />
                  </div>

                  <div className={styles.screenBlock}>
                    <div className={styles.screenHead}>
                      <span className={styles.label}>当前屏幕</span>
                      <button
                        className={styles.btnGhost}
                        type="button"
                        onClick={() => void refreshShot()}
                        disabled={shotLoading}
                        data-testid="image-screen-refresh"
                      >
                        {shotLoading ? '截取中…' : '刷新截图'}
                      </button>
                    </div>
                    {screenShot ? (
                      <div className={styles.screenView} data-testid="image-screen-view">
                        <img
                          className={styles.screenImg}
                          src={`data:image/png;base64,${screenShot.data}`}
                          alt="当前屏幕"
                          draggable={false}
                        />
                        {image.roi && (
                          <span
                            className={styles.screenRoi}
                            style={roiStyle(image.roi, {
                              originX: screenShot.originX,
                              originY: screenShot.originY,
                              width: screenShot.width,
                              height: screenShot.height,
                            })}
                          />
                        )}
                        {image.roi && (
                          <span
                            className={styles.screenCentre}
                            style={centrestyle(image.roi, {
                              originX: screenShot.originX,
                              originY: screenShot.originY,
                              width: screenShot.width,
                              height: screenShot.height,
                            })}
                          />
                        )}
                      </div>
                    ) : (
                      <div className={styles.mapMeta}>
                        {shotLoading ? '正在截取当前屏幕…' : '暂时拿不到屏幕截图'}
                      </div>
                    )}
                  </div>

                  {image.roi ? (
                    <div className={styles.mapBlock}>
                      <div className={styles.mapMeta}>
                        屏幕位置 ({image.roi.x}, {image.roi.y}) {image.roi.width}×
                        {image.roi.height} · 中心 (
                        {image.roi.x + Math.floor(image.roi.width / 2)},{' '}
                        {image.roi.y + Math.floor(image.roi.height / 2)})
                      </div>
                    </div>
                  ) : (
                    <div className={styles.mapMeta}>这张图还没有屏幕位置，框选后会显示在这里。</div>
                  )}

                  <label className={styles.field}>
                    <span className={styles.label}>名称</span>
                    <div className={styles.inline}>
                      <input
                        className={styles.input}
                        value={nameDraft}
                        onChange={(event) => setNameDraft(event.target.value)}
                        data-testid="image-library-name"
                      />
                      <button
                        className={styles.btn}
                        type="button"
                        onClick={() => void handleRename()}
                        disabled={busy || !nameDraft.trim() || nameDraft === image.name}
                      >
                        保存
                      </button>
                    </div>
                  </label>

                  <div className={styles.field}>
                    <span className={styles.label}>
                      屏幕位置 / ROI
                      <em className={styles.hint}>块没设 ROI 时执行会回落到这里</em>
                    </span>
                    <div className={styles.roiRow}>
                      {(['x', 'y', 'width', 'height'] as const).map((key) => (
                        <label key={key} className={styles.roiCell}>
                          <span>
                            {key === 'width' ? '宽' : key === 'height' ? '高' : key.toUpperCase()}
                          </span>
                          <input
                            className={styles.input}
                            inputMode="numeric"
                            value={roiDraft[key]}
                            placeholder="—"
                            onChange={(event) =>
                              setRoiDraft((draft) => ({ ...draft, [key]: event.target.value }))
                            }
                          />
                        </label>
                      ))}
                    </div>
                    <div className={styles.btnRow}>
                      <button className={styles.btn} type="button" onClick={() => void handleApplyRoi()} disabled={busy}>
                        应用
                      </button>
                      <button
                        className={styles.btn}
                        type="button"
                        onClick={() => void handleEditOnScreen()}
                        disabled={busy || picking}
                        title="全屏打开当前屏幕，拖动框选即可修改这张图的大小/位置与 ROI"
                        data-testid="image-library-pick-roi"
                      >
                        {picking ? '请在屏幕上调整…' : '在屏幕上调整'}
                      </button>
                      <button className={styles.btnGhost} type="button" onClick={() => void handleClearRoi()} disabled={busy}>
                        清除 ROI
                      </button>
                    </div>
                  </div>

                  <div className={styles.btnRow}>
                    <button className={styles.btn} type="button" onClick={() => void handleTest()} disabled={busy}>
                      测试匹配
                    </button>
                    <button
                      className={styles.dangerBtn}
                      type="button"
                      onClick={() => setConfirm('delete')}
                      disabled={busy}
                    >
                      删除这张
                    </button>
                  </div>
                  {testResult && <div className={styles.result}>{testResult}</div>}
                  <p className={styles.footMeta}>
                    {image.width}×{image.height} · {image.format.toUpperCase()} ·{' '}
                    {new Date(image.createdAt).toLocaleString()}
                  </p>
                </div>
              )}
            </div>
          ))}
        </div>
      </aside>

      <ConfirmDialog
        isOpen={confirm === 'delete'}
        title="删除这张图片？"
        message={active ? `「${active.name}」会从图片库删除，引用它的积木块会失效。` : ''}
        confirmText="删除"
        onConfirm={() => void handleDelete()}
        onCancel={() => setConfirm(null)}
      />
      <ConfirmDialog
        isOpen={confirm === 'clear'}
        title="清空图片库？"
        message={`将删除全部 ${images.length} 张图片，无法恢复。引用这些图片的积木块会失效。`}
        confirmText="全部清空"
        onConfirm={() => void handleClearAll()}
        onCancel={() => setConfirm(null)}
      />
    </div>
  );
}

export default ImageLibraryDrawer;
