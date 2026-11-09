/**
 * 积木块相关类型定义
 * 对应后端 src-tauri/src/models/block.rs
 * 
 * Validates: Requirements 2.2, 2.3
 */

/**
 * 积木块 ID
 */
export type BlockId = string;

/**
 * 动作积木块类型
 */
export type ActionType = 'click' | 'wait_image' | 'wait_time' | 'input_text' | 'text_extract' | 'screenshot_assert';

/**
 * 控制积木块类型
 */
export type ControlType = 'loop' | 'loop_infinite' | 'condition' | 'text_check';

/**
 * 积木块类型
 */
export type BlockType =
  | { type: 'action'; action: ActionType }
  | { type: 'control'; control: ControlType };

/**
 * 积木块位置（画布坐标）
 */
export interface BlockPosition {
  x: number;
  y: number;
}

/**
 * 点击模式
 *
 * coordinates 使用虚拟桌面物理像素，允许负坐标（副屏在主屏左侧/上方）。
 * uia 通过 Windows UI Automation 定位控件；执行前会提窗、重读矩形并校验遮挡。
 */
export type ClickMode =
  | { mode: 'coordinates'; x: number; y: number }
  | { mode: 'image'; imageId?: string; threshold?: number }
  | {
      mode: 'uia';
      window: string;
      name?: string;
      automationId?: string;
      className?: string;
      controlType?: string;
      index?: number;
      force?: boolean;
    };

/**
 * 条件操作
 */
export type ConditionOp = 'image_exists' | 'image_not_exists';

/**
 * 积木块配置
 */
export type BlockConfig =
  | {
      type: 'click';
      mode: ClickMode;
      count: number;
    }
  | {
      type: 'wait_image';
      imageId?: string;
      timeoutMs?: number;
      threshold?: number;
    }
  | {
      type: 'wait_time';
      durationMs: number;
    }
  | {
      type: 'input_text';
      text: string;
      intervalMs?: number;
    }
  | {
      type: 'loop';
      count: number;
    }
  | {
      type: 'loop_infinite';
    }
  | {
      type: 'condition';
      imageId?: string;
      condition: ConditionOp;
      trueBranch: BlockId[];
      falseBranch: BlockId[];
    }
  | {
      type: 'text_extract';
      imageId?: string;
      language?: string;
    }
  | {
      type: 'screenshot_assert';
      imageId?: string;
      threshold?: number;
      strictMode: boolean;
      region?: { x: number; y: number; width: number; height: number };
    }
  | {
      type: 'text_check';
      imageId?: string;
      keyword: string;
      trueBranch: BlockId[];
      falseBranch: BlockId[];
    };

/**
 * 积木块节点
 */
export interface BlockNode {
  /** 积木块 ID */
  id: BlockId;
  /** 积木块类型 */
  blockType: BlockType;
  /** 位置 */
  position: BlockPosition;
  /** 配置 */
  config: BlockConfig;
  /** 子积木块（用于控制积木块） */
  children: BlockId[];
}
