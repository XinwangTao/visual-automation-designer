/**
 * Tauri Command 封装
 * 提供类型安全的 Tauri Command 调用接口
 * 
 * Validates: Requirements 8.4
 */

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import type { ImageMetadata, ScreenRect } from '../types/image';
import type { Flow, FlowMetadata } from '../types/flow';
import type { BlockId } from '../types/block';

// ============================================================================
// 图片格式验证
// ============================================================================

/** 支持的图片格式 */
export const SUPPORTED_IMAGE_FORMATS = ['png', 'jpg', 'jpeg', 'bmp'] as const;
export type SupportedImageFormat = typeof SUPPORTED_IMAGE_FORMATS[number];

/** 支持的图片 MIME 类型 */
export const SUPPORTED_IMAGE_MIME_TYPES = [
  'image/png',
  'image/jpeg',
  'image/bmp',
] as const;

/**
 * 验证图片格式是否受支持
 * @param filePath 文件路径
 * @returns 如果格式受支持返回 true
 */
export function validateImageFormat(filePath: string): boolean {
  const extension = filePath.split('.').pop()?.toLowerCase() || '';
  return SUPPORTED_IMAGE_FORMATS.includes(extension as SupportedImageFormat);
}

/**
 * 获取文件扩展名
 * @param filePath 文件路径
 * @returns 文件扩展名（小写），如果没有扩展名则返回空字符串
 */
export function getFileExtension(filePath: string): string {
  const lastDotIndex = filePath.lastIndexOf('.');
  
  // 如果没有点，或者点后面没有内容
  if (lastDotIndex === -1 || lastDotIndex === filePath.length - 1) {
    return '';
  }
  
  // 确保点不在路径分隔符之后（即点是文件名的一部分）
  const lastSeparator = Math.max(filePath.lastIndexOf('/'), filePath.lastIndexOf('\\'));
  if (lastSeparator > lastDotIndex) {
    return '';
  }
  
  // 检查点是否在文件名开头（隐藏文件如 .hidden）
  // 这种情况下，如果文件名只有点开头且没有其他点，则没有扩展名
  const fileName = filePath.substring(lastSeparator + 1);
  if (fileName.startsWith('.') && fileName.indexOf('.', 1) === -1) {
    return '';
  }
  
  return filePath.substring(lastDotIndex + 1).toLowerCase();
}

/**
 * 获取文件名（不含扩展名）
 * @param filePath 文件路径
 * @returns 文件名
 */
export function getFileName(filePath: string): string {
  const parts = filePath.split(/[\\/]/);
  const fileName = parts.pop() || '';
  const dotIndex = fileName.lastIndexOf('.');
  return dotIndex > 0 ? fileName.substring(0, dotIndex) : fileName;
}

// ============================================================================
// 图片库相关命令
// ============================================================================

/**
 * 选择图片文件（打开文件选择对话框）
 * @returns 选择的文件路径，如果取消则返回 null
 */
export async function selectImageFile(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    filters: [
      {
        name: 'Images',
        extensions: ['png', 'jpg', 'jpeg', 'bmp'],
      },
    ],
  });
  
  return selected;
}

/**
 * 选择多个图片文件（打开文件选择对话框）
 * @returns 选择的文件路径数组，如果取消则返回 null
 */
export async function selectMultipleImageFiles(): Promise<string[] | null> {
  const selected = await open({
    multiple: true,
    filters: [
      {
        name: 'Images',
        extensions: ['png', 'jpg', 'jpeg', 'bmp'],
      },
    ],
  });
  
  if (selected === null) {
    return null;
  }
  
  // 单选时返回的是 string，多选时返回的是 string[]
  if (typeof selected === 'string') {
    return [selected];
  }
  
  return selected;
}

/**
 * 添加图片
 */
export async function addImage(filePath: string, name: string): Promise<ImageMetadata> {
  // 验证格式
  if (!validateImageFormat(filePath)) {
    throw new Error(`不支持的图片格式。支持的格式: ${SUPPORTED_IMAGE_FORMATS.join(', ')}`);
  }
  
  return invoke<ImageMetadata>('add_image', { filePath, name });
}

/**
 * 删除图片
 */
export async function removeImage(id: ImageId): Promise<void> {
  return invoke('remove_image', { id });
}

/**
 * 重命名图片
 */
export async function renameImage(id: ImageId, newName: string): Promise<void> {
  return invoke('rename_image', { id, newName });
}

/**
 * 列出所有图片
 */
export async function listImages(): Promise<ImageMetadata[]> {
  return invoke<ImageMetadata[]>('list_images');
}

/**
 * 设置图片的屏幕 ROI（截取位置）；传 null 表示清除。
 */
export async function setImageRoi(id: string, roi: ScreenRect | null): Promise<boolean> {
  return invoke<boolean>('set_image_roi', { id, roi });
}

/**
 * 清空图片库（删除所有图片文件，不可恢复）。
 */
export async function clearImageLibrary(): Promise<number> {
  return invoke<number>('clear_image_library');
}

/** 虚拟桌面边界（物理像素），用于图片库里画出 ROI 的相对位置。 */
export interface DesktopBounds {
  originX: number;
  originY: number;
  width: number;
  height: number;
}

export async function getDesktopBounds(): Promise<DesktopBounds> {
  return invoke<DesktopBounds>('get_desktop_bounds');
}

/**
 * 取图片缩略图的 data URL。
 *
 * WebView 读不到图片库目录，`<img src="文件名">` 一定 404——这就是之前
 * 选择器里一片空白的根因。
 */
export async function getImageThumbnail(id: string, maxSize = 320): Promise<string> {
  return invoke<string>('get_image_thumbnail', { id, maxSize });
}

/**
 * 从 base64 数据添加图片（用于剪贴板粘贴）
 */
export async function addImageFromBase64(base64Data: string, name: string): Promise<ImageMetadata> {
  return invoke<ImageMetadata>('add_image_from_base64', { base64Data, name });
}

// ============================================================================
// 流程管理相关命令
// ============================================================================

/**
 * 创建流程
 */
export async function createFlow(name: string): Promise<Flow> {
  return invoke<Flow>('create_flow', { name });
}

/**
 * 保存流程
 */
export async function saveFlow(flow: Flow): Promise<void> {
  return invoke('save_flow', { flow });
}

/**
 * 加载流程
 */
export async function loadFlow(id: string): Promise<Flow> {
  return invoke<Flow>('load_flow', { id });
}

/**
 * 列出所有流程
 */
export async function listFlows(): Promise<FlowMetadata[]> {
  return invoke<FlowMetadata[]>('list_flows');
}

/**
 * 删除流程
 */
export async function deleteFlow(id: string): Promise<void> {
  return invoke('delete_flow', { id });
}

/**
 * 验证流程
 */
export async function validateFlow(flow: Flow): Promise<ValidationResult> {
  return invoke<ValidationResult>('validate_flow', { flow });
}

// ============================================================================
// 积木块操作相关命令
// ============================================================================

import type { BlockConfig, BlockNode, BlockPosition, BlockType } from '../types/block';

/**
 * 创建积木块
 */
export async function createBlock(
  flowId: string,
  blockType: BlockType,
  config: BlockConfig,
  position: BlockPosition
): Promise<BlockNode> {
  return invoke<BlockNode>('create_block', { flowId, blockType, config, position });
}

/**
 * 更新积木块位置
 */
export async function updateBlockPosition(
  flowId: string,
  blockId: BlockId,
  position: BlockPosition
): Promise<void> {
  return invoke('update_block_position', { flowId, blockId, position });
}

/**
 * 更新积木块配置
 */
export async function updateBlockConfig(
  flowId: string,
  blockId: BlockId,
  config: BlockConfig
): Promise<void> {
  return invoke('update_block_config', { flowId, blockId, config });
}

/**
 * 删除积木块
 */
export async function deleteBlock(flowId: string, blockId: BlockId): Promise<void> {
  return invoke('delete_block', { flowId, blockId });
}

/**
 * 设置入口积木块
 */
export async function setEntryBlock(
  flowId: string,
  blockId: BlockId | null
): Promise<void> {
  return invoke('set_entry_block', { flowId, blockId });
}

/**
 * 创建连接
 */
export async function createConnection(
  flowId: string,
  source: BlockId,
  target: BlockId,
  sourceHandle?: string
): Promise<Connection> {
  return invoke<Connection>('create_connection', { flowId, source, target, sourceHandle });
}

/**
 * 删除连接
 */
export async function deleteConnection(flowId: string, connectionId: string): Promise<void> {
  return invoke('delete_connection', { flowId, connectionId });
}

// ============================================================================
// 类型定义
// ============================================================================

import type { Connection } from '../types/flow';

type ImageId = string;

interface ValidationResult {
  isValid: boolean;
  errors: ValidationError[];
  warnings: ValidationWarning[];
}

interface ValidationError {
  code: string;
  message: string;
  blockId?: BlockId;
  connectionId?: string;
}

type ValidationWarning = ValidationError;

// ============================================================================
// 目标测试（只读解析，不点击）
// ============================================================================

export interface TargetPoint {
  x: number;
  y: number;
}

export interface TargetRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ImageTargetTest {
  found: boolean;
  score?: number;
  threshold: number;
  center?: TargetPoint;
  hitRect?: TargetRect;
  /** 实际搜索的屏幕区域（ROI 裁剪后）；为空表示搜索整个桌面。 */
  searchedRegion?: TargetRect;
  elapsedMs: number;
  target: string;
  error?: string;
}

export interface UiaNodeInfo {
  name: string;
  automationId: string;
  className: string;
  controlType: number;
  controlTypeName: string;
  x: number;
  y: number;
  w: number;
  h: number;
  enabled: boolean;
  offscreen: boolean;
  depth: number;
}

export interface UiaTargetTest {
  found: boolean;
  node?: UiaNodeInfo;
  occludedBy?: string;
  error?: string;
}

/** 只读解析图片目标：截取虚拟桌面并做模板匹配。 */
export async function testImageTarget(
  imageId: string,
  threshold?: number,
  roi?: { x: number; y: number; width: number; height: number },
): Promise<ImageTargetTest> {
  return invoke<ImageTargetTest>('test_image_target', { imageId, threshold, roi });
}

/** 只读解析 UIA 控件：不提升窗口、不点击。 */
export async function testUiaTarget(params: {
  window: string;
  name?: string;
  automationId?: string;
  className?: string;
  controlType?: string;
  index?: number;
}): Promise<UiaTargetTest> {
  return invoke<UiaTargetTest>('test_uia_target', params);
}

// ============================================================================
// 屏幕截取（图像选择器）
// ============================================================================

export interface ScreenPreview {
  data: string;
  width: number;
  height: number;
  previewWidth: number;
  previewHeight: number;
  originX: number;
  originY: number;
  scale: number;
}

export interface ScreenRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface ScreenPoint {
  x: number;
  y: number;
}

export interface CapturedImage {
  image: ImageMetadata;
  roi: ScreenRegion;
  center: ScreenPoint;
}

/** 截取虚拟桌面，返回可直接显示的 PNG（可缩放预览）。 */
export async function captureScreenPreview(maxWidth?: number): Promise<ScreenPreview> {
  return invoke<ScreenPreview>('capture_screen_preview', { maxWidth });
}

/** 从当前屏幕裁剪区域并加入图片库，返回图片元数据 + 屏幕 ROI/中心点。 */
export async function addImageFromScreenRegion(params: {
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
}): Promise<CapturedImage> {
  return invoke<CapturedImage>('add_image_from_screen_region', params);
}

// ----------------------------------------------------------------------------
// 全屏截取窗口
//
// open_screen_capture 冻结整个虚拟桌面并打开一个覆盖全屏、置顶的无边框窗口；
// 用户在窗口里直接框选，保存时从同一张冻结图上裁剪，避免把遮罩截进去。
// ----------------------------------------------------------------------------

/** 冻结桌面并打开全屏框选窗口；主窗口会被隐藏，取消/保存后自动恢复。 */
export async function openScreenCapture(): Promise<void> {
  return invoke('open_screen_capture');
}

/** 框选窗口在冻结图渲染完成后调用，避免闪白。 */
export async function showScreenCaptureWindow(): Promise<void> {
  return invoke('show_screen_capture_window');
}

/** 获取冻结桌面的预览（可缩放）；坐标尺寸为虚拟桌面物理像素。 */
export async function getScreenCapturePreview(maxWidth?: number): Promise<ScreenPreview> {
  return invoke<ScreenPreview>('get_screen_capture_preview', { maxWidth });
}

/** 从冻结桌面裁剪并保存到图片库；x/y/width/height 以冻结图左上角为原点。 */
export async function saveScreenCapture(params: {
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
}): Promise<CapturedImage> {
  return invoke<CapturedImage>('save_screen_capture', params);
}

/** 放弃本次截取，关闭框选窗口并恢复主窗口。 */
export async function cancelScreenCapture(): Promise<void> {
  return invoke('cancel_screen_capture');
}

/** 监听「截取已保存」事件（主窗口用它刷新图片库并选中新图）。 */
export async function onScreenCaptureSaved(
  callback: (captured: CapturedImage) => void,
): Promise<() => void> {
  const { listen } = await import('@tauri-apps/api/event');
  return listen<CapturedImage>('screen-capture-saved', (event) => {
    callback(event.payload);
  });
}

/** 主窗口已被切成全屏框选层：前端挂载 CaptureOverlay。 */
export async function onScreenCaptureOpened(callback: () => void): Promise<() => void> {
  const { listen } = await import('@tauri-apps/api/event');
  return listen('screen-capture-opened', () => {
    callback();
  });
}

/** 框选结束（保存或取消）：前端卸载 CaptureOverlay。 */
export async function onScreenCaptureClosed(callback: () => void): Promise<() => void> {
  const { listen } = await import('@tauri-apps/api/event');
  return listen('screen-capture-closed', () => {
    callback();
  });
}
