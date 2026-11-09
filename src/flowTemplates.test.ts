import { describe, expect, it, vi } from 'vitest';
import { FLOW_TEMPLATES, applyFlowTemplate, type FlowTemplate } from './flowTemplates';

const KNOWN_TYPES = new Set([
  'click',
  'wait_image',
  'wait_time',
  'input_text',
  'text_extract',
  'screenshot_assert',
  'loop',
  'loop_infinite',
  'condition',
  'text_check',
]);

describe('内置流程模板', () => {
  it('模板 id 唯一且必备字段齐全', () => {
    const ids = FLOW_TEMPLATES.map((template) => template.id);
    expect(new Set(ids).size).toBe(ids.length);

    for (const template of FLOW_TEMPLATES) {
      expect(template.name.trim()).not.toBe('');
      expect(template.description.trim()).not.toBe('');
      expect(template.nodes.length).toBeGreaterThan(0);
    }
  });

  it('节点类型都是工具箱里真实存在的类型，配置里的 type 与节点 type 一致', () => {
    for (const template of FLOW_TEMPLATES) {
      for (const node of template.nodes) {
        expect(KNOWN_TYPES.has(node.type)).toBe(true);
        expect(node.config.type).toBe(node.type);
        expect(['action', 'control']).toContain(node.category);
      }
    }
  });

  it('连线只指向存在的节点，且不存在重复连线', () => {    for (const template of FLOW_TEMPLATES) {
      const seen = new Set<string>();
      for (const connection of template.connections) {
        expect(connection.from).toBeGreaterThanOrEqual(0);
        expect(connection.from).toBeLessThan(template.nodes.length);
        expect(connection.to).toBeGreaterThanOrEqual(0);
        expect(connection.to).toBeLessThan(template.nodes.length);
        const key = `${connection.from}->${connection.to}`;
        expect(seen.has(key)).toBe(false);
        seen.add(key);
      }
    }
  });

  it('多输出端口的块流出时带 sourceHandle（否则后端报端口歧义）', () => {
    // 「等待图片」有 found / match_position 两个输出端口
    for (const template of FLOW_TEMPLATES) {
      for (const connection of template.connections) {
        const sourceNode = template.nodes[connection.from];
        if (sourceNode.type === 'wait_image') {
          expect(connection.sourceHandle).toBeTruthy();
        }
      }
    }
  });
});

describe('applyFlowTemplate', () => {
  const template: FlowTemplate = {
    id: 't',
    name: '测试模板',
    description: '两个节点一条线',
    nodes: [
      { type: 'wait_time', category: 'action', position: { x: 0, y: 0 }, config: { type: 'wait_time', durationMs: 1 } },
      { type: 'wait_time', category: 'action', position: { x: 100, y: 0 }, config: { type: 'wait_time', durationMs: 1 } },
    ],
    connections: [{ from: 0, to: 1 }],
  };

  it('按顺序创建节点，并用真实 id 连线', async () => {
    const addNode = vi.fn()
      .mockResolvedValueOnce('node-a')
      .mockResolvedValueOnce('node-b');
    const addConnection = vi.fn().mockResolvedValue(undefined);

    const ids = await applyFlowTemplate(template, 'flow-1', addNode, addConnection);

    expect(ids).toEqual(['node-a', 'node-b']);
    expect(addNode).toHaveBeenNthCalledWith(
      1,
      'wait_time',
      'action',
      { x: 0, y: 0 },
      { type: 'wait_time', durationMs: 1 },
      'flow-1',
    );
    expect(addNode).toHaveBeenNthCalledWith(
      2,
      'wait_time',
      'action',
      { x: 100, y: 0 },
      { type: 'wait_time', durationMs: 1 },
      'flow-1',
    );
    expect(addConnection).toHaveBeenCalledWith(
      { source: 'node-a', target: 'node-b', sourceHandle: null, targetHandle: null },
      'flow-1',
    );
  });

  it('节点创建失败时不继续连线，把错误抛给调用方', async () => {
    const addNode = vi.fn()
      .mockResolvedValueOnce('node-a')
      .mockRejectedValueOnce(new Error('后端拒绝'));
    const addConnection = vi.fn();

    await expect(applyFlowTemplate(template, 'flow-1', addNode, addConnection)).rejects.toThrow('后端拒绝');
    expect(addConnection).not.toHaveBeenCalled();
  });

  it('连线索引越界时抛错而不是静默跳过', async () => {
    const broken: FlowTemplate = {
      ...template,
      connections: [{ from: 0, to: 9 }],
    };
    const addNode = vi.fn().mockResolvedValue('node-a');
    const addConnection = vi.fn();

    await expect(applyFlowTemplate(broken, 'flow-1', addNode, addConnection)).rejects.toThrow('不存在的节点');
    expect(addConnection).not.toHaveBeenCalled();
  });
});
