import { describe, expect, it } from 'vitest';
import { fromTauriBlockConfig } from '../tauri/flow';
import { blockNodeToReactFlowNode, buildCanonicalFlow } from './flowHelpers';
import type { BlockNode as TauriBlockNode, Flow as TauriFlow } from '../tauri/flow';

const asRecord = (value: unknown) => value as Record<string, unknown>;

/**
 * 回归：后端 serde 用 camelCase 变体名（waitImage），前端用下划线（wait_image）。
 * 不做归一化时，从后端加载的流程再次发给 validate_flow 会因为 toTauriBlockConfig
 * 落空而丢掉整个 config 字段，后端直接报 missing field `config`。
 */
describe('fromTauriBlockConfig', () => {
  it('把后端的 camelCase 类型映射回前端命名', () => {
    expect(asRecord(fromTauriBlockConfig({ type: 'waitImage', timeoutMs: 15000 }))).toEqual({
      type: 'wait_image',
      timeoutMs: 15000,
    });
    expect(
      asRecord(fromTauriBlockConfig({ type: 'screenshotAssert', threshold: 0.9, strictMode: false })),
    ).toEqual({
      type: 'screenshot_assert',
      threshold: 0.9,
      strictMode: false,
    });
    expect(asRecord(fromTauriBlockConfig({ type: 'loopInfinite' }))).toEqual({ type: 'loop_infinite' });
  });

  it('本来就同名的类型保持原样', () => {
    expect(asRecord(fromTauriBlockConfig({ type: 'click', count: 1 }))).toEqual({ type: 'click', count: 1 });
    expect(asRecord(fromTauriBlockConfig({ type: 'condition', condition: 'image_exists' }))).toEqual({
      type: 'condition',
      condition: 'image_exists',
    });
  });

  it('未知类型与非法输入原样返回，不静默改写', () => {
    expect(asRecord(fromTauriBlockConfig({ type: 'somethingNew' }))).toEqual({ type: 'somethingNew' });
    expect(fromTauriBlockConfig(null)).toBeNull();
    expect(fromTauriBlockConfig(undefined)).toBeUndefined();
  });

  it('不修改传入对象', () => {
    const original = { type: 'waitImage', timeoutMs: 1 };
    fromTauriBlockConfig(original);
    expect(original.type).toBe('waitImage');
  });
});

describe('从后端块构造节点时的配置归一化', () => {
  it('节点数据里的 config 已经是前端命名，串行化后不会丢字段', () => {
    const block: TauriBlockNode = {
      id: 'block-1',
      blockType: { type: 'action', action: 'wait_image' } as TauriBlockNode['blockType'],
      position: { x: 10, y: 20 },
      config: { type: 'waitImage', timeoutMs: 15000 } as unknown as TauriBlockNode['config'],
      children: [],
    };

    const node = blockNodeToReactFlowNode(block);
    expect(asRecord(node.data.config).type).toBe('wait_image');

    const flow = {
      id: 'flow-1',
      name: '回归',
      blocks: {},
      connections: [],
      createdAt: '2026-01-01T00:00:00Z',
      updatedAt: '2026-01-01T00:00:00Z',
    } as unknown as TauriFlow;

    const canonical = buildCanonicalFlow(flow, [node], []);
    const built = canonical.blocks['block-1'];
    expect(built).toBeDefined();
    expect(asRecord(built.config).type).toBe('wait_image');
  });
});
