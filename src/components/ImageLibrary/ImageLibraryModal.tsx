/**
 * ImageLibraryModal — 图片库面板。
 *
 * 顶部工具栏「图片库」打开：网格看缩略图，右侧改名称、改屏幕 ROI（可回屏幕
 * 重新框选）、试匹配、删除单张或清空全部。
 */

import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  clearImageLibrary,
  onScreenCaptureClosed,
  onScreenCaptureSaved,
  openScreenCapture,
  removeImage,
  renameImage,
  setImageRoi,
  testImageTarget,
  listImages,
} from '../../tauri/commands';
import type { ImageMetadata, ScreenRect } from '../../types/image';
import { toError } from '../../utils/error';
import { ConfirmDialog } from '../common/ConfirmDialog';
import { ImageThumb, invalidateImageThumb } from '../common/ImageThumb';
import styles from './ImageLibraryModal.module.css';

export interface ImageLibraryModalProps {
  open: boolean;
  onClose: () => void;
  /** 图片增删后通知外层刷新角标 */
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
  const x = Number(draft.x);
  const y = Number(draft.y);
  const width = Number(draft.width);
  const height = Number(draft.height);
  if (![x, y, width, height].every((n) => Number.isFinite(n))) return null;
  if (width < 1 || height < 1) return null;
  return { x: Math.round(x), y: Math.round(y), width: Math.round(width), height: Math.round(height) };
}

export function ImageLibraryModal({ open, onClose, onChanged }: ImageLibraryModalProps) {
  const [images, setImages] = useState<ImageMetadata[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [nameDraft, setNameDraft] = useState('');
  const [roiDraft, setRoiDraft] = useState<RoiDraft>(draftFromRoi());
  const [confirm, setConfirm] = useState<'delete' | 'clear' | null>(null);
  const [busy, setBusy] = useState(false);
  const [picking, setPicking] = useState(false);
  /** 开始框选时库里已有的 id：只删本次新建的临时图片。 */
  const [pickedFrom, setPickedFrom] = useState<Set<string> | null>(null);
  const [testResult, setTestResult] = useState<string | null>(null);

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
    if (!open) return;
    void refresh().then((list) => {
      setSelectedId((current) =>
        current && list.some((image) => image.id === current)
          ? current
          : list[list.length - 1]?.id ?? null,
      );
    });
  }, [open, refresh]);

  useEffect(() => {
    if (!open) {
      setQuery('');
      setTestResult(null);
      setPicking(false);
    }
  }, [open]);

  const selected = useMemo(
    () => images.find((image) => image.id === selectedId) ?? null,
    [images, selectedId],
  );

  useEffect(() => {
    if (!selected) return;
    setNameDraft(selected.name);
    setRoiDraft(draftFromRoi(selected.roi));
    setTestResult(null);
  }, [selected?.id, selected?.name, selected?.roi?.x, selected?.roi?.y]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return images;
    return images.filter((image) => image.name.toLowerCase().includes(q));
  }, [images, query]);

  const notifyChanged = useCallback(() => {
    onChanged?.();
  }, [onChanged]);

  /** 截屏结束后把框选结果写回当前图片，并删掉这次临时生成的图片。 */
  useEffect(() => {
    if (!picking || !selectedId) return;
    let disposed = false;
    const disposers: Array<() => void> = [];
    const keep = (dispose: () => void) => {
      if (disposed) dispose();
      else disposers.push(dispose);
    };
    const targetId = selectedId;

    void onScreenCaptureSaved(async (captured) => {
      setPicking(false);
      try {
        await setImageRoi(targetId, captured.roi);
        // 截图会顺带入库一张图：只有它是这次新建的才删，去重命中的已有图不能动。
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

    void onScreenCaptureClosed(() => setPicking(false)).then(keep, () => undefined);

    return () => {
      disposed = true;
      disposers.forEach((dispose) => dispose());
    };
  }, [picking, selectedId, pickedFrom, refresh, notifyChanged]);

  const handleRename = async () => {
    if (!selected) return;
    const name = nameDraft.trim();
    if (!name || name === selected.name) return;
    setBusy(true);
    try {
      await renameImage(selected.id, name);
      await refresh();
      notifyChanged();
    } catch (err) {
      setError(toError(err).message || '改名失败');
    } finally {
      setBusy(false);
    }
  };

  const handleApplyRoi = async () => {
    if (!selected) return;
    const roi = parseDraft(roiDraft);
    if (!roi) {
      setError('ROI 需要填写完整的 x / y / 宽 / 高（宽高必须大于 0）');
      return;
    }
    setBusy(true);
    try {
      await setImageRoi(selected.id, roi);
      await refresh();
      setError(null);
    } catch (err) {
      setError(toError(err).message || '写入 ROI 失败');
    } finally {
      setBusy(false);
    }
  };

  const handleClearRoi = async () => {
    if (!selected) return;
    setBusy(true);
    try {
      await setImageRoi(selected.id, null);
      await refresh();
    } catch (err) {
      setError(toError(err).message || '清除 ROI 失败');
    } finally {
      setBusy(false);
    }
  };

  const handleTest = async () => {
    if (!selected) return;
    setBusy(true);
    setTestResult('正在匹配…');
    try {
      const result = await testImageTarget(selected.id, 0.7, selected.roi ?? undefined);
      setTestResult(
        result.found
          ? `已找到 · 匹配 ${((result.score ?? 0) * 100).toFixed(1)}% · 用时 ${result.elapsedMs}ms`
          : `未找到 · 最高匹配 ${((result.score ?? 0) * 100).toFixed(1)}%（阈值 70%）· 用时 ${result.elapsedMs}ms`,
      );
    } catch (err) {
      setTestResult(toError(err).message || '测试失败');
    } finally {
      setBusy(false);
    }
  };

  const handlePickOnScreen = async () => {
    if (!selected) return;
    setError(null);
    setPickedFrom(new Set(images.map((image) => image.id)));
    setPicking(true);
    try {
      await openScreenCapture();
    } catch (err) {
      setPicking(false);
      setError(toError(err).message || '打开屏幕截取失败');
    }
  };

  const handleDelete = async () => {
    if (!selected) return;
    setBusy(true);
    try {
      await removeImage(selected.id);
      invalidateImageThumb(selected.id);
      const list = await refresh();
      setSelectedId(list[list.length - 1]?.id ?? null);
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
      setSelectedId(null);
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
    <div className={styles.overlay} onClick={onClose} role="presentation">
      <div
        className={styles.modal}
        role="dialog"
        aria-modal="true"
        aria-label="图片库"
        onClick={(event) => event.stopPropagation()}
      >
        <header className={styles.header}>
          <div className={styles.titleGroup}>
            <h2 className={styles.title}>图片库</h2>
            <span className={styles.count}>{images.length} 张</span>
          </div>
          <input
            className={styles.search}
            type="search"
            value={query}
            placeholder="搜索图片名称…"
            onChange={(event) => setQuery(event.target.value)}
            data-testid="image-library-search"
          />
          <button
            className={styles.dangerBtn}
            type="button"
            onClick={() => setConfirm('clear')}
            disabled={busy || images.length === 0}
            data-testid="image-library-clear"
          >
            清空全部
          </button>
          <button className={styles.closeBtn} type="button" onClick={onClose} aria-label="关闭">
            ×
          </button>
        </header>

        {error && <div className={styles.error}>{error}</div>}

        <div className={styles.body}>
          <div className={styles.grid} data-testid="image-library-grid">
            {loading && images.length === 0 && <p className={styles.empty}>读取中…</p>}
            {!loading && filtered.length === 0 && (
              <p className={styles.empty}>
                还没有图片。在积木块配置里点「从屏幕截取」就会存到这里。
              </p>
            )}
            {filtered.map((image) => (
              <button
                key={image.id}
                type="button"
                className={`${styles.card} ${image.id === selectedId ? styles.cardActive : ''}`}
                onClick={() => setSelectedId(image.id)}
                data-testid={`image-card-${image.id}`}
                title={image.name}
              >
                <div className={styles.thumb}>
                  <ImageThumb id={image.id} alt={image.name} maxSize={200} className={styles.thumbImg} />
                </div>
                <span className={styles.cardName}>{image.name}</span>
                <span className={styles.cardMeta}>
                  {image.width}×{image.height}
                  {image.roi ? ` · ${image.roi.x},${image.roi.y}` : ' · 未设 ROI'}
                </span>
              </button>
            ))}
          </div>

          <aside className={styles.detail}>
            {!selected && <p className={styles.empty}>选择一张图片查看和编辑</p>}
            {selected && (
              <>
                <div className={styles.preview}>
                  <ImageThumb
                    id={selected.id}
                    alt={selected.name}
                    maxSize={720}
                    className={styles.previewImg}
                  />
                </div>

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
                      disabled={busy || !nameDraft.trim() || nameDraft === selected.name}
                    >
                      保存
                    </button>
                  </div>
                </label>

                <div className={styles.field}>
                  <span className={styles.label}>
                    屏幕位置 / ROI
                    <em className={styles.hint}>块本身没设 ROI 时，执行会回落到这里</em>
                  </span>
                  <div className={styles.roiRow}>
                    {(['x', 'y', 'width', 'height'] as const).map((key) => (
                      <label key={key} className={styles.roiCell}>
                        <span>{key === 'width' ? '宽' : key === 'height' ? '高' : key.toUpperCase()}</span>
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
                      onClick={() => void handlePickOnScreen()}
                      disabled={busy || picking}
                      data-testid="image-library-pick-roi"
                    >
                      {picking ? '请在屏幕上框选…' : '在屏幕上框选'}
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
                  {selected.width}×{selected.height} · {selected.format.toUpperCase()} ·{' '}
                  {new Date(selected.createdAt).toLocaleString()}
                </p>
              </>
            )}
          </aside>
        </div>
      </div>

      <ConfirmDialog
        isOpen={confirm === 'delete'}
        title="删除这张图片？"
        message={selected ? `「${selected.name}」会从图片库删除，引用它的积木块会失效。` : ''}
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

export default ImageLibraryModal;
