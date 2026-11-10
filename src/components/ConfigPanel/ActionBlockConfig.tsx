/**
 * ActionBlockConfig — 动作类积木配置组件
 * 从 BlockConfig.tsx 提取：click / wait_image / wait_time / input_text / screenshot_assert / text_extract 的配置 UI。
 */

import { useState } from 'react';
import type { ClickMode } from '../../types/block';
import { testImageTarget, testUiaTarget } from '../../tauri/commands';
import { ImageSelector } from './ImageSelector';
import styles from './ConfigPanel.module.css';

interface ConfigUIProps {
  config: Record<string, unknown>;
  onChange: (config: Record<string, unknown>) => void;
}

type TestTone = 'ok' | 'warn' | 'error';
interface TestResult {
  tone: TestTone;
  title: string;
  detail: string;
}

function thresholdFromInput(value: string): number {
  if (value.trim() === '') return 0.7;
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return 0.7;
  return Math.min(1, Math.max(0, parsed));
}

function describeScore(score: number | undefined, threshold: number): string {
  if (score === undefined) return '';
  return `匹配 ${(score * 100).toFixed(1)}%（阈值 ${(threshold * 100).toFixed(0)}%）`;
}

// ── Click ──────────────────────────────────────────────────────────

export function ClickConfigUI({ config, onChange }: ConfigUIProps) {
  const mode = (config.mode as ClickMode) || { mode: 'coordinates', x: 0, y: 0 };
  const count = (config.count as number) ?? 1;

  const handleModeChange = (newMode: 'coordinates' | 'image' | 'uia') => {
    if (newMode === 'coordinates') {
      onChange({ ...config, mode: { mode: 'coordinates', x: 0, y: 0 } });
    } else if (newMode === 'image') {
      onChange({ ...config, mode: { mode: 'image', imageId: '' } });
    } else {
      onChange({ ...config, mode: { mode: 'uia', window: '', force: false } });
    }
  };

  const handleCoordinateChange = (field: 'x' | 'y', value: number) => {
    const currentMode = mode as { mode: 'coordinates'; x: number; y: number };
    if (currentMode.mode === 'coordinates') {
      onChange({ ...config, mode: { ...currentMode, [field]: value } });
    }
  };

  const handleImageSelect = (imageId: string) => {
    const currentMode = mode as { mode: 'image'; imageId: string; roi?: unknown };
    if (currentMode.mode === 'image') {
      // A different reference image invalidates the old ROI.
      onChange({ ...config, mode: { ...currentMode, imageId, roi: undefined } });
    }
  };

  const handleRegionSelect = (imageId: string, roi: { x: number; y: number; width: number; height: number }) => {
    const currentMode = mode as { mode: 'image'; imageId?: string; threshold?: number };
    if (currentMode.mode === 'image') {
      onChange({
        ...config,
        mode: { ...currentMode, imageId, roi, threshold: currentMode.threshold ?? 0.7 },
      });
    }
  };

  const uiaMode =
    mode.mode === 'uia'
      ? (mode as Extract<ClickMode, { mode: 'uia' }>)
      : null;

  const updateUia = (patch: Partial<NonNullable<typeof uiaMode>>) => {
    if (!uiaMode) return;
    onChange({ ...config, mode: { ...uiaMode, ...patch } });
  };

  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<TestResult | null>(null);

  const handleTestTarget = async () => {
    setTestResult(null);
    if (mode.mode === 'coordinates') {
      const point = mode as { mode: 'coordinates'; x: number; y: number };
      setTestResult({
        tone: 'ok',
        title: '坐标已解析',
        detail: `将点击虚拟屏幕坐标 (${point.x}, ${point.y})`,
      });
      return;
    }
    if (mode.mode === 'image') {
      const image = mode as { mode: 'image'; imageId?: string; threshold?: number; roi?: { x: number; y: number; width: number; height: number } };
      if (!image.imageId) {
        setTestResult({ tone: 'warn', title: '缺少图片', detail: '请先选择一张参考图片' });
        return;
      }
      setTesting(true);
      try {
        const result = await testImageTarget(image.imageId, image.threshold ?? 0.7, image.roi);
        if (result.found) {
          const rect = result.hitRect;
          setTestResult({
            tone: 'ok',
            title: '图片目标已找到',
            detail: [
              describeScore(result.score, result.threshold),
              rect ? `矩形 (${rect.x}, ${rect.y}) ${rect.width}×${rect.height}` : '',
              `用时 ${result.elapsedMs}ms`,
            ].filter(Boolean).join(' · '),
          });
        } else {
          setTestResult({
            tone: 'error',
            title: '未找到图片目标',
            detail: `${describeScore(result.score, result.threshold)} · 用时 ${result.elapsedMs}ms`,
          });
        }
      } catch (error) {
        setTestResult({
          tone: 'error',
          title: '测试失败',
          detail: error instanceof Error ? error.message : '仅桌面应用可用',
        });
      } finally {
        setTesting(false);
      }
      return;
    }
    if (uiaMode) {
      if (!uiaMode.window.trim()) {
        setTestResult({ tone: 'warn', title: '缺少窗口', detail: '请先填写窗口标题' });
        return;
      }
      if (!uiaMode.name?.trim() && !uiaMode.automationId?.trim() && !uiaMode.className?.trim() && !uiaMode.controlType?.trim()) {
        setTestResult({ tone: 'warn', title: '缺少选择条件', detail: '至少填写名称 / AutomationId / 类名 / 控件类型之一' });
        return;
      }
      setTesting(true);
      try {
        const result = await testUiaTarget({
          window: uiaMode.window,
          name: uiaMode.name || undefined,
          automationId: uiaMode.automationId || undefined,
          className: uiaMode.className || undefined,
          controlType: uiaMode.controlType || undefined,
          index: uiaMode.index,
        });
        if (result.found && result.node) {
          const node = result.node;
          setTestResult({
            tone: result.occludedBy ? 'warn' : 'ok',
            title: result.occludedBy ? '控件被遮挡' : 'UIA 控件已找到',
            detail: [
              `${node.controlTypeName}${node.name ? `「${node.name}」` : ''}`,
              `(${node.x}, ${node.y}) ${node.w}×${node.h}`,
              result.occludedBy ? `被「${result.occludedBy}」覆盖` : '',
            ].filter(Boolean).join(' · '),
          });
        } else {
          setTestResult({
            tone: 'error',
            title: '未找到 UIA 控件',
            detail: result.error ?? '请检查窗口标题与选择条件',
          });
        }
      } catch (error) {
        setTestResult({
          tone: 'error',
          title: '测试失败',
          detail: error instanceof Error ? error.message : '仅桌面应用可用',
        });
      } finally {
        setTesting(false);
      }
    }
  };

  return (
    <div className={styles.blockConfigSection} data-testid="click-config">
      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel}>点击模式</label>
        <div className={styles.blockConfigModeSelector}>
          <button
            className={`${styles.blockConfigModeBtn} ${mode.mode === 'coordinates' ? styles.active : ''}`}
            onClick={() => handleModeChange('coordinates')}
            data-testid="mode-coordinates"
          >
            坐标模式
          </button>
          <button
            className={`${styles.blockConfigModeBtn} ${mode.mode === 'image' ? styles.active : ''}`}
            onClick={() => handleModeChange('image')}
            data-testid="mode-image"
          >
            图片模式
          </button>
          <button
            className={`${styles.blockConfigModeBtn} ${mode.mode === 'uia' ? styles.active : ''}`}
            onClick={() => handleModeChange('uia')}
            data-testid="mode-uia"
          >
            UIA 控件
          </button>
        </div>
      </div>

      {mode.mode === 'coordinates' && (
        <div className={styles.blockConfigFieldGroup}>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel} htmlFor="click-x">X 坐标</label>
            <input
              id="click-x"
              type="number"
              className={styles.blockConfigInput}
              value={(mode as { mode: 'coordinates'; x: number; y: number }).x}
              onChange={(e) => handleCoordinateChange('x', Number(e.target.value))}
              data-testid="input-x"
              aria-required="true"
            />
            <span className={styles.blockConfigHint}>虚拟屏幕物理像素，可为负值</span>
          </div>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel} htmlFor="click-y">Y 坐标</label>
            <input
              id="click-y"
              type="number"
              className={styles.blockConfigInput}
              value={(mode as { mode: 'coordinates'; x: number; y: number }).y}
              onChange={(e) => handleCoordinateChange('y', Number(e.target.value))}
              data-testid="input-y"
              aria-required="true"
            />
            <span className={styles.blockConfigHint}>虚拟屏幕物理像素，可为负值</span>
          </div>
        </div>
      )}

      {mode.mode === 'image' && (
        <>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel}>选择图片</label>
            <ImageSelector
              selectedId={(mode as { mode: 'image'; imageId: string }).imageId}
              onSelect={handleImageSelect}
              onRegionSelect={handleRegionSelect}
              showUploadButton={true}
              emptyMessage="请选择、上传或从屏幕截取图片"
            />
          </div>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel} htmlFor="click-image-threshold">匹配阈值</label>
            <input
              id="click-image-threshold"
              type="number"
              className={styles.blockConfigInput}
              value={(mode as { mode: 'image'; threshold?: number }).threshold ?? 0.7}
              onChange={(e) =>
                onChange({
                  ...config,
                  mode: { ...(mode as { mode: 'image'; imageId?: string }), threshold: thresholdFromInput(e.target.value) },
                })
              }
              min={0}
              max={1}
              step={0.05}
              data-testid="input-click-threshold"
            />
            <span className={styles.blockConfigHint}>0.7 为默认值；调低更宽松，调高更严格</span>
          </div>
          {(mode as { roi?: { x: number; y: number; width: number; height: number } }).roi && (
            <div className={styles.blockConfigField}>
              <label className={styles.blockConfigLabel}>搜索区域（ROI）</label>
              <div className="roi-summary">
                <span>
                  {(() => {
                    const roi = (mode as { roi: { x: number; y: number; width: number; height: number } }).roi;
                    return `${roi.x}, ${roi.y} · ${roi.width}×${roi.height}`;
                  })()}
                </span>
                <button
                  type="button"
                  onClick={() => onChange({ ...config, mode: { ...(mode as object), roi: undefined } })}
                >
                  清除
                </button>
              </div>
              <span className={styles.blockConfigHint}>执行时只在该虚拟屏幕区域内匹配图片</span>
            </div>
          )}
        </>
      )}

      {uiaMode && (
        <div className={styles.blockConfigFieldGroup}>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel} htmlFor="click-uia-window">窗口标题（包含即可）</label>
            <input
              id="click-uia-window"
              type="text"
              className={styles.blockConfigInput}
              value={uiaMode.window}
              onChange={(e) => updateUia({ window: e.target.value })}
              placeholder="例如：记事本"
              data-testid="input-uia-window"
            />
          </div>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel} htmlFor="click-uia-name">控件名称（包含即可）</label>
            <input
              id="click-uia-name"
              type="text"
              className={styles.blockConfigInput}
              value={uiaMode.name ?? ''}
              onChange={(e) => updateUia({ name: e.target.value })}
              placeholder="例如：确定"
              data-testid="input-uia-name"
            />
          </div>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel} htmlFor="click-uia-automation-id">AutomationId（精确）</label>
            <input
              id="click-uia-automation-id"
              type="text"
              className={styles.blockConfigInput}
              value={uiaMode.automationId ?? ''}
              onChange={(e) => updateUia({ automationId: e.target.value })}
              data-testid="input-uia-automation-id"
            />
          </div>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel} htmlFor="click-uia-class">类名（精确）</label>
            <input
              id="click-uia-class"
              type="text"
              className={styles.blockConfigInput}
              value={uiaMode.className ?? ''}
              onChange={(e) => updateUia({ className: e.target.value })}
              data-testid="input-uia-class"
            />
          </div>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel} htmlFor="click-uia-type">控件类型（精确）</label>
            <input
              id="click-uia-type"
              type="text"
              className={styles.blockConfigInput}
              value={uiaMode.controlType ?? ''}
              onChange={(e) => updateUia({ controlType: e.target.value })}
              placeholder="Button / Edit / Text ..."
              data-testid="input-uia-control-type"
            />
          </div>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel} htmlFor="click-uia-index">同名控件序号（从 0 开始）</label>
            <input
              id="click-uia-index"
              type="number"
              className={styles.blockConfigInput}
              value={uiaMode.index ?? ''}
              onChange={(e) =>
                updateUia({
                  index: e.target.value === '' ? undefined : Number(e.target.value),
                })
              }
              min={0}
              data-testid="input-uia-index"
            />
          </div>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel}>
              <input
                type="checkbox"
                checked={uiaMode.force ?? false}
                onChange={(e) => updateUia({ force: e.target.checked })}
                data-testid="checkbox-uia-force"
              />
              {' '}跳过遮挡校验
            </label>
            <span className={styles.blockConfigHint}>
              默认会先提窗并用 WindowFromPoint 校验；被其他窗口遮挡时拒绝点击
            </span>
          </div>
        </div>
      )}

      <div className={styles.blockConfigField}>
        <button
          className={styles.blockConfigTestBtn}
          type="button"
          onClick={() => void handleTestTarget()}
          disabled={testing}
          data-testid="btn-test-target"
        >
          {testing ? '正在解析目标…' : '测试目标'}
        </button>
        <span className={styles.blockConfigHint}>
          只读解析当前屏幕，不会点击、不会提升窗口
        </span>
      </div>

      {testResult && (
        <div
          className={`${styles.targetTestResult} ${styles[`targetTestResult${testResult.tone.charAt(0).toUpperCase() + testResult.tone.slice(1)}`]}`}
          role="status"
          data-testid="target-test-result"
        >
          <strong>{testResult.title}</strong>
          <span>{testResult.detail}</span>
        </div>
      )}

      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel} htmlFor="click-count">点击次数</label>
        <input
          id="click-count"
          type="number"
          className={styles.blockConfigInput}
          value={count}
          onChange={(e) => onChange({ ...config, count: Number(e.target.value) })}
          min={1}
          max={3}
          data-testid="input-count"
          aria-required="true"
        />
      </div>
    </div>
  );
}

// ── Wait Image ─────────────────────────────────────────────────────

export function WaitImageConfigUI({ config, onChange }: ConfigUIProps) {
  const imageId = (config.imageId as string) || '';
  const timeoutMs = (config.timeoutMs as number) ?? 5000;
  const threshold = (config.threshold as number) ?? 0.7;
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<TestResult | null>(null);

  const handleTest = async () => {
    if (!imageId) {
      setTestResult({ tone: 'warn', title: '缺少图片', detail: '请先选择一张参考图片' });
      return;
    }
    setTestResult(null);
    setTesting(true);
    try {
      const result = await testImageTarget(imageId, threshold, config.roi as { x: number; y: number; width: number; height: number } | undefined);
      if (result.found) {
        const rect = result.hitRect;
        setTestResult({
          tone: 'ok',
          title: '当前屏幕已出现目标',
          detail: [
            describeScore(result.score, result.threshold),
            rect ? `矩形 (${rect.x}, ${rect.y}) ${rect.width}×${rect.height}` : '',
            `用时 ${result.elapsedMs}ms`,
          ].filter(Boolean).join(' · '),
        });
      } else {
        setTestResult({
          tone: 'warn',
          title: '当前屏幕未出现目标',
          detail: `${describeScore(result.score, result.threshold)} · 执行时会按超时继续等待`,
        });
      }
    } catch (error) {
      setTestResult({
        tone: 'error',
        title: '测试失败',
        detail: error instanceof Error ? error.message : '仅桌面应用可用',
      });
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className={styles.blockConfigSection} data-testid="wait-image-config">
      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel}>等待图片</label>
        <ImageSelector
          selectedId={imageId}
          onSelect={(newImageId: string) => onChange({ ...config, imageId: newImageId, roi: undefined })}
          onRegionSelect={(newImageId, roi) => onChange({ ...config, imageId: newImageId, roi })}
          showUploadButton={true}
          emptyMessage="请选择、上传或从屏幕截取要等待的图片"
        />
      </div>
      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel} htmlFor="waitimage-timeout">超时时间 (毫秒)</label>
        <input
          id="waitimage-timeout"
          type="number"
          className={styles.blockConfigInput}
          value={timeoutMs}
          onChange={(e) => onChange({ ...config, timeoutMs: Number(e.target.value) })}
          min={100}
          step={100}
          data-testid="input-timeout"
        />
        <span className={styles.blockConfigHint}>默认 5000 毫秒</span>
      </div>
      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel} htmlFor="waitimage-threshold">匹配阈值</label>
        <input
          id="waitimage-threshold"
          type="number"
          className={styles.blockConfigInput}
          value={threshold}
          onChange={(e) => onChange({ ...config, threshold: thresholdFromInput(e.target.value) })}
          min={0}
          max={1}
          step={0.05}
          data-testid="input-wait-threshold"
        />
        <span className={styles.blockConfigHint}>0.7 为默认值；低于阈值的匹配会被视为未出现</span>
      </div>
      {(config.roi as { x: number; y: number; width: number; height: number } | undefined) && (
        <div className={styles.blockConfigField}>
          <label className={styles.blockConfigLabel}>搜索区域（ROI）</label>
          <div className="roi-summary">
            <span>
              {(() => {
                const roi = config.roi as { x: number; y: number; width: number; height: number };
                return `${roi.x}, ${roi.y} · ${roi.width}×${roi.height}`;
              })()}
            </span>
            <button type="button" onClick={() => onChange({ ...config, roi: undefined })}>
              清除
            </button>
          </div>
          <span className={styles.blockConfigHint}>只在该虚拟屏幕区域内等待图片</span>
        </div>
      )}
      <div className={styles.blockConfigField}>
        <button
          className={styles.blockConfigTestBtn}
          type="button"
          onClick={() => void handleTest()}
          disabled={testing}
          data-testid="btn-test-wait-image"
        >
          {testing ? '正在匹配…' : '测试当前屏幕'}
        </button>
        <span className={styles.blockConfigHint}>只读匹配，不会等待、不会点击</span>
      </div>
      {testResult && (
        <div
          className={`${styles.targetTestResult} ${styles[`targetTestResult${testResult.tone.charAt(0).toUpperCase() + testResult.tone.slice(1)}`]}`}
          role="status"
          data-testid="wait-image-test-result"
        >
          <strong>{testResult.title}</strong>
          <span>{testResult.detail}</span>
        </div>
      )}
    </div>
  );
}

// ── Wait Time ──────────────────────────────────────────────────────

export function WaitTimeConfigUI({ config, onChange }: ConfigUIProps) {
  const durationMs = (config.durationMs as number) ?? 1000;

  return (
    <div className={styles.blockConfigSection} data-testid="wait-time-config">
      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel} htmlFor="waittime-duration">等待时间 (毫秒)</label>
        <input
          id="waittime-duration"
          type="number"
          className={styles.blockConfigInput}
          value={durationMs}
          onChange={(e) => onChange({ ...config, durationMs: Number(e.target.value) })}
          min={0}
          step={100}
          data-testid="input-duration"
        />
        <span className={styles.blockConfigHint}>1000 毫秒 = 1 秒</span>
      </div>
    </div>
  );
}

// ── Input Text ─────────────────────────────────────────────────────

export function InputTextConfigUI({ config, onChange }: ConfigUIProps) {
  const text = (config.text as string) || '';
  const intervalMs = (config.intervalMs as number) ?? 50;

  return (
    <div className={styles.blockConfigSection} data-testid="input-text-config">
      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel} htmlFor="inputtext-text">输入文本</label>
        <textarea
          id="inputtext-text"
          className={styles.blockConfigTextarea}
          value={text}
          onChange={(e) => onChange({ ...config, text: e.target.value })}
          placeholder="输入要模拟输入的文本内容"
          rows={3}
          data-testid="input-text"
          aria-required="true"
        />
      </div>
      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel} htmlFor="inputtext-interval">输入间隔 (毫秒)</label>
        <input
          id="inputtext-interval"
          type="number"
          className={styles.blockConfigInput}
          value={intervalMs}
          onChange={(e) => onChange({ ...config, intervalMs: Number(e.target.value) })}
          min={0}
          max={1000}
          step={10}
          data-testid="input-interval"
        />
        <span className={styles.blockConfigHint}>字符之间的输入间隔，默认 50 毫秒</span>
      </div>
    </div>
  );
}

// ── Screenshot Assert ──────────────────────────────────────────────

export function ScreenshotAssertConfigUI({ config, onChange }: ConfigUIProps) {
  const imageId = (config.imageId as string) || '';
  const threshold = (config.threshold as number) ?? 0.0;
  const strictMode = (config.strictMode as boolean) ?? false;
  const region = config.region as { x: number; y: number; width: number; height: number } | undefined;

  const handleRegionChange = (field: string, value: number) => {
    const currentRegion = region ?? { x: 0, y: 0, width: 0, height: 0 };
    onChange({ ...config, region: { ...currentRegion, [field]: value } });
  };

  return (
    <div className={styles.blockConfigSection} data-testid="screenshot-assert-config">
      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel}>参考图片</label>
        <ImageSelector
          selectedId={imageId}
          onSelect={(newImageId: string) => onChange({ ...config, imageId: newImageId })}
          showUploadButton={true}
          emptyMessage="请选择或上传参考图片"
        />
      </div>
      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel} htmlFor="screenshot-threshold">比对阈值</label>
        <input
          id="screenshot-threshold"
          type="number"
          className={styles.blockConfigInput}
          value={threshold}
          onChange={(e) => onChange({ ...config, threshold: thresholdFromInput(e.target.value) })}
          min={0}
          max={1}
          step={0.05}
          data-testid="input-threshold"
        />
        <span className={styles.blockConfigHint}>0.0 = 完全一致, 1.0 = 完全忽略差异</span>
      </div>
      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel}>
          <input
            type="checkbox"
            checked={strictMode}
            onChange={(e) => onChange({ ...config, strictMode: e.target.checked })}
            data-testid="checkbox-strict"
          />
          {' '}严格模式
        </label>
        <span className={styles.blockConfigHint}>开启后任何像素差异都将视为失败</span>
      </div>
      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel}>裁剪区域（可选）</label>
        <div className={styles.blockConfigFieldGroup}>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel} htmlFor="screenshot-region-x">X</label>
            <input
              id="screenshot-region-x"
              type="number"
              className={styles.blockConfigInput}
              value={region?.x ?? 0}
              onChange={(e) => handleRegionChange('x', Number(e.target.value))}
              min={0}
              data-testid="input-region-x"
            />
          </div>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel} htmlFor="screenshot-region-y">Y</label>
            <input
              id="screenshot-region-y"
              type="number"
              className={styles.blockConfigInput}
              value={region?.y ?? 0}
              onChange={(e) => handleRegionChange('y', Number(e.target.value))}
              min={0}
              data-testid="input-region-y"
            />
          </div>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel} htmlFor="screenshot-region-w">宽度</label>
            <input
              id="screenshot-region-w"
              type="number"
              className={styles.blockConfigInput}
              value={region?.width ?? 0}
              onChange={(e) => handleRegionChange('width', Number(e.target.value))}
              min={0}
              data-testid="input-region-w"
            />
          </div>
          <div className={styles.blockConfigField}>
            <label className={styles.blockConfigLabel} htmlFor="screenshot-region-h">高度</label>
            <input
              id="screenshot-region-h"
              type="number"
              className={styles.blockConfigInput}
              value={region?.height ?? 0}
              onChange={(e) => handleRegionChange('height', Number(e.target.value))}
              min={0}
              data-testid="input-region-h"
            />
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Text Extract ───────────────────────────────────────────────────

export function TextExtractConfigUI({ config, onChange }: ConfigUIProps) {
  const imageId = (config.imageId as string) || '';
  const language = (config.language as string) || 'chi_sim';

  return (
    <div className={styles.blockConfigSection} data-testid="text-extract-config">
      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel}>提取区域图片</label>
        <ImageSelector
          selectedId={imageId}
          onSelect={(newImageId: string) => onChange({ ...config, imageId: newImageId })}
          showUploadButton={true}
          emptyMessage="选择要提取文字的图片区域（可选，留空则全屏提取）"
        />
      </div>
      <div className={styles.blockConfigField}>
        <label className={styles.blockConfigLabel} htmlFor="text-extract-lang">识别语言</label>
        <select
          id="text-extract-lang"
          className={styles.blockConfigSelect}
          value={language}
          onChange={(e) => onChange({ ...config, language: e.target.value })}
          data-testid="select-language"
        >
          <option value="chi_sim">简体中文</option>
          <option value="chi_tra">繁体中文</option>
          <option value="eng">英语</option>
          <option value="jpn">日语</option>
          <option value="kor">韩语</option>
        </select>
        <span className={styles.blockConfigHint}>选择 OCR 识别语言</span>
      </div>
    </div>
  );
}
