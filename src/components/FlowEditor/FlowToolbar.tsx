/**
 * FlowToolbar - 工具栏组件
 * 提供保存、执行、撤销等操作按钮
 *
 * Validates: Requirements 2.6, 5.1, 5.5, 5.6, 7.1, 7.2
 */

import type { ThemeMode } from '../../hooks';
import { memo, useMemo } from 'react';
import styles from './FlowEditor.module.css';

export interface FlowToolbarProps {
  canUndo?: boolean;
  canRedo?: boolean;
  isExecuting?: boolean;
  isPaused?: boolean;
  hasFlow?: boolean;
  flowName?: string;
  themeMode?: ThemeMode;
  onSave?: () => void;
  onLoad?: () => void;
  onUndo?: () => void;
  onRedo?: () => void;
  onExecute?: () => void;
  onPause?: () => void;
  onStop?: () => void;
  onStep?: () => void;
  onDryRun?: () => void;
  onNew?: () => void;
  onToggleTheme?: () => void;
  onHelp?: () => void;
}


type ToolbarIconName =
  | 'plus' | 'save' | 'folder' | 'undo' | 'redo'
  | 'play' | 'pause' | 'stop' | 'step' | 'dry'
  | 'sun' | 'moon' | 'auto' | 'help';

const TOOLBAR_ICON_PATHS: Record<ToolbarIconName, string> = {
  plus: 'M8 3v10M3 8h10',
  save: 'M3 2.5h8l2 2V13.5H3zM5 2.5v4h6v-4M5 13.5v-4.2h6v4.2',
  folder: 'M2 4.5h4l1.2 2H14v7H2z',
  undo: 'M6 3.5 2.5 7 6 10.5M2.5 7h7a4 4 0 0 1 0 8H7',
  redo: 'M10 3.5 13.5 7 10 10.5M13.5 7h-7a4 4 0 0 0 0 8H9',
  play: 'M5.5 3.5 12.5 8l-7 4.5z',
  pause: 'M5.5 3.5v9M10.5 3.5v9',
  stop: 'M4.5 4.5h7v7h-7z',
  step: 'M4.5 3.5 10 8l-5.5 4.5zM12 3.5v9',
  dry: 'M6 2h4M7 2v3.6L3.6 12a1 1 0 0 0 .9 1.5h7a1 1 0 0 0 .9-1.5L9 5.6V2',
  sun: 'M8 5.2a2.8 2.8 0 1 0 0 5.6 2.8 2.8 0 0 0 0-5.6zM8 1.5v1.6M8 12.9v1.6M1.5 8h1.6M12.9 8h1.6M3.4 3.4l1.1 1.1M11.5 11.5l1.1 1.1M12.6 3.4l-1.1 1.1M4.5 11.5l-1.1 1.1',
  moon: 'M13 9.6A5.6 5.6 0 0 1 6.4 3a5.6 5.6 0 1 0 6.6 6.6z',
  auto: 'M3 8a5 5 0 0 1 8.6-3.5M13 8a5 5 0 0 1-8.6 3.5M11.4 2.6v2.2H9.2M4.6 13.4v-2.2h2.2',
  help: 'M8 14A6 6 0 1 0 8 2a6 6 0 0 0 0 12zM6.2 6.4A1.9 1.9 0 0 1 8 5.2c1 0 1.8.8 1.8 1.8 0 1.4-1.8 1.5-1.8 2.8M8 11.6v.2',
};

const FILLED_ICONS = new Set<ToolbarIconName>(['play', 'stop']);

function ToolbarIcon({ name }: { name: ToolbarIconName }) {
  const filled = FILLED_ICONS.has(name);
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 16 16"
      aria-hidden="true"
      focusable="false"
      fill={filled ? 'currentColor' : 'none'}
      stroke={filled ? 'none' : 'currentColor'}
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d={TOOLBAR_ICON_PATHS[name]} />
    </svg>
  );
}

/**
 * FlowToolbar 组件 - 流程编辑工具栏
 * 提供流程控制、执行和文件操作按钮
 */
export const FlowToolbar = memo(function FlowToolbar({
  canUndo = false,
  canRedo = false,
  isExecuting = false,
  isPaused = false,
  hasFlow = false,
  flowName,
  themeMode = 'auto',
  onSave,
  onLoad,
  onUndo,
  onRedo,
  onExecute,
  onPause,
  onStop,
  onStep,
  onDryRun,
  onNew,
  onToggleTheme,
  onHelp,
}: FlowToolbarProps) {
  const currentModeLabel = useMemo(
    () => isExecuting ? (isPaused ? '已暂停，可继续或单步' : '正在执行自动化流程') : '准备编辑并运行流程',
    [isExecuting, isPaused],
  );

  const themeInfo = useMemo(() => {
    switch (themeMode) {
      case 'light': return { icon: 'sun' as const, title: '当前: 亮色主题 (点击切换)' };
      case 'dark': return { icon: 'moon' as const, title: '当前: 暗色主题 (点击切换)' };
      case 'auto': return { icon: 'auto' as const, title: '当前: 跟随系统 (点击切换)' };
    }
  }, [themeMode]);

  return (
    <div className={styles.flowToolbar} data-testid="flow-toolbar" role="toolbar" aria-label="流程编辑工具栏">
      {/* Flow Name Display - UX优化1: 显示当前流程名称 */}
      {flowName && (
        <div className={styles.flowToolbarFlowName} title={flowName}>
          <span className={styles.flowToolbarFlowNameLabel}>当前流程</span>
          <span className={styles.flowToolbarFlowNameValue}>{flowName}</span>
        </div>
      )}
      
      {/* File Operations Group */}
      <div className={`${styles.flowToolbarGroup} ${styles.flowToolbarGroupFile}`} role="group" aria-label="文件操作">
        {/* UX优化2: 添加新建按钮 */}
        <button
          className={styles.flowToolbarBtn}
          onClick={onNew}
          title="新建流程 (Ctrl+N)"
          data-testid="btn-new"
          type="button"
          aria-label="新建流程"
        >
          <ToolbarIcon name="plus" /> 新建
        </button>
        <button
          className={styles.flowToolbarBtn}
          onClick={onSave}
          disabled={!hasFlow}
          title="保存 (Ctrl+S)"
          data-testid="btn-save"
          type="button"
          aria-label="保存流程"
        >
          <ToolbarIcon name="save" /> 保存
        </button>
        <button
          className={styles.flowToolbarBtn}
          onClick={onLoad}
          title="打开 (Ctrl+O)"
          data-testid="btn-load"
          type="button"
          aria-label="打开流程"
        >
          <ToolbarIcon name="folder" /> 打开
        </button>
      </div>

      {/* Edit Operations Group */}
      <div className={`${styles.flowToolbarGroup} ${styles.flowToolbarGroupEdit}`} role="group" aria-label="编辑操作">
        <button
          className={styles.flowToolbarBtn}
          onClick={onUndo}
          disabled={!canUndo}
          title="撤销 (Ctrl+Z)"
          data-testid="btn-undo"
          type="button"
          aria-label="撤销"
          aria-disabled={!canUndo}
        >
          <ToolbarIcon name="undo" /> 撤销
        </button>
        <button
          className={styles.flowToolbarBtn}
          onClick={onRedo}
          disabled={!canRedo}
          title="重做 (Ctrl+Y)"
          data-testid="btn-redo"
          type="button"
          aria-label="重做"
          aria-disabled={!canRedo}
        >
          <ToolbarIcon name="redo" /> 重做
        </button>
      </div>

      {/* Execution Control Group */}
      <div className="flow-toolbar__group flow-toolbar__group--execute" role="group" aria-label="执行控制">
        <div className={styles.flowToolbarExecuteMeta} aria-live="polite">
          <span className={styles.flowToolbarExecuteMetaLabel}>运行状态</span>
          <span className={styles.flowToolbarExecuteMetaValue}>{currentModeLabel}</span>
        </div>
        {!isExecuting ? (
          // Execute button (shown when not executing)
          <button
            className={`${styles.flowToolbarBtn} ${styles.flowToolbarBtnPrimary}`}
            onClick={onExecute}
            disabled={!hasFlow}
            title={hasFlow ? "执行 (F5)" : "请先创建或加载流程"}
            data-testid="btn-execute"
            type="button"
            aria-label="执行流程"
          >
            <ToolbarIcon name="play" /> 执行
          </button>
        ) : (
          // Pause/Resume and Stop buttons (shown when executing)
          <>
            <button
              className={`${styles.flowToolbarBtn} ${isPaused ? styles.flowToolbarBtnPrimary : ''}`}
              onClick={onPause}
              title={isPaused ? '继续 (F5)' : '暂停'}
              data-testid="btn-pause"
              type="button"
              aria-label={isPaused ? '继续执行' : '暂停执行'}
              aria-pressed={isPaused}
            >
              {isPaused ? <><ToolbarIcon name="play" /> 继续</> : <><ToolbarIcon name="pause" /> 暂停</>}
            </button>
            <button
              className={`${styles.flowToolbarBtn} ${styles.flowToolbarBtnDanger}`}
              onClick={onStop}
              title="停止 (Shift+F5)"
              data-testid="btn-stop"
              type="button"
              aria-label="停止执行"
            >
              <ToolbarIcon name="stop" /> 停止
            </button>
          </>
        )}

        <button
          className={styles.flowToolbarBtn}
          onClick={onDryRun}
          disabled={!hasFlow || isExecuting}
          title="试运行：只解析目标，不点击、不输入"
          data-testid="btn-dry-run"
          type="button"
          aria-label="试运行"
        >
          <ToolbarIcon name="dry" /> 试跑
        </button>

        {/* Step button - always visible, disabled during running (not paused) execution */}
        <button
          className={styles.flowToolbarBtn}
          onClick={onStep}
          disabled={!hasFlow || (isExecuting && !isPaused)}
          title="单步执行 (F10)"
          data-testid="btn-step"
          type="button"
          aria-label="单步执行"
          aria-disabled={!hasFlow || (isExecuting && !isPaused)}
        >
          <ToolbarIcon name="step" /> 单步
        </button>
      </div>

      {/* Help Group - UX优化41: 增强快捷键帮助 */}
      <div className={`${styles.flowToolbarGroup} ${styles.flowToolbarGroupHelp}`} role="group" aria-label="帮助">
        {/* UX优化103: 主题切换按钮 */}
        <button
          className={`${styles.flowToolbarBtn} ${styles.flowToolbarBtnTheme}`}
          onClick={onToggleTheme}
          title={themeInfo.title}
          data-testid="btn-theme"
          type="button"
          aria-label="切换主题"
        >
          <ToolbarIcon name={themeInfo.icon} />
        </button>
        <button
          className={`${styles.flowToolbarBtn} ${styles.flowToolbarBtnHelp}`}
          onClick={onHelp}
          title="查看快捷键"
          data-testid="btn-help"
          type="button"
          aria-label="帮助"
        >
          <ToolbarIcon name="help" />
        </button>
      </div>
    </div>
  );
});

export default FlowToolbar;
