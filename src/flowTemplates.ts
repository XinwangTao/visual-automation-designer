import type { Connection } from 'reactflow';

/**
 * 新建流程时可选的内置模板。
 *
 * 空白画布对第一次使用的人几乎是劝退的：不知道先拖哪个块、块之间要不要连、配置填什么。
 * 模板只做一件事——把「等待图片 → 点击 → 等待 → 断言」这条最常见的链路摆好，
 * 用户改成自己的图就能跑。模板里的节点都是普通节点，创建后可以随意增删改。
 */
export interface FlowTemplateNode {
  /** 与工具箱一致的具体类型，如 click / wait_image */
  type: string;
  category: 'action' | 'control';
  position: { x: number; y: number };
  /** 直接落库的配置，字段与 BlockConfig 对应 */
  config: Record<string, unknown>;
}

export interface FlowTemplateConnection {
  /** 节点在 nodes 数组中的下标，从 0 开始 */
  from: number;
  to: number;
}

export interface FlowTemplate {
  id: string;
  name: string;
  description: string;
  nodes: FlowTemplateNode[];
  connections: FlowTemplateConnection[];
}

export const FLOW_TEMPLATES: FlowTemplate[] = [
  {
    id: 'starter-click',
    name: '入门：等图 → 点击 → 等待',
    description: '最常见的三步链路：等到目标元素出现，点它，再给界面一点反应时间。把两张图换成自己的即可。',
    nodes: [
      {
        type: 'wait_image',
        category: 'action',
        position: { x: 80, y: 160 },
        config: { type: 'wait_image', timeoutMs: 15000 },
      },
      {
        type: 'click',
        category: 'action',
        position: { x: 360, y: 160 },
        config: { type: 'click', mode: { mode: 'coordinates', x: 640, y: 360 }, count: 1 },
      },
      {
        type: 'wait_time',
        category: 'action',
        position: { x: 640, y: 160 },
        config: { type: 'wait_time', durationMs: 1200 },
      },
    ],
    connections: [
      { from: 0, to: 1 },
      { from: 1, to: 2 },
    ],
  },
  {
    id: 'assert-result',
    name: '带校验：等图 → 点击 → 等待 → 截图断言',
    description: '在点击之后加一步截图断言，用来确认操作真的生效了。适合发版前跑一遍的回归流程。',
    nodes: [
      {
        type: 'wait_image',
        category: 'action',
        position: { x: 80, y: 160 },
        config: { type: 'wait_image', timeoutMs: 15000 },
      },
      {
        type: 'click',
        category: 'action',
        position: { x: 340, y: 160 },
        config: { type: 'click', mode: { mode: 'coordinates', x: 640, y: 360 }, count: 1 },
      },
      {
        type: 'wait_time',
        category: 'action',
        position: { x: 600, y: 160 },
        config: { type: 'wait_time', durationMs: 1500 },
      },
      {
        type: 'screenshot_assert',
        category: 'action',
        position: { x: 860, y: 160 },
        config: { type: 'screenshot_assert', threshold: 0.9, strictMode: false },
      },
    ],
    connections: [
      { from: 0, to: 1 },
      { from: 1, to: 2 },
      { from: 2, to: 3 },
    ],
  },
];

export type AddNodeFn = (
  type: string,
  category: string,
  position: { x: number; y: number },
  config?: Record<string, unknown>,
  flowIdOverride?: string,
) => Promise<string>;

export type AddConnectionFn = (connection: Connection, flowIdOverride?: string) => Promise<void>;

/**
 * 把模板落到刚创建的流程里：先按顺序建节点（拿到真实 ID），再按索引连线。
 *
 * 节点必须串行创建——连线要用上一步返回的 ID；并发建节点拿不到稳定顺序。
 */
export async function applyFlowTemplate(
  template: FlowTemplate,
  flowId: string,
  addNode: AddNodeFn,
  addConnection: AddConnectionFn,
): Promise<string[]> {
  const ids: string[] = [];
  for (const node of template.nodes) {
    const id = await addNode(node.type, node.category, node.position, node.config, flowId);
    ids.push(id);
  }

  for (const connection of template.connections) {
    const source = ids[connection.from];
    const target = ids[connection.to];
    if (!source || !target) {
      throw new Error(`模板连线指向了不存在的节点：${connection.from} → ${connection.to}`);
    }
    await addConnection({ source, target, sourceHandle: null, targetHandle: null }, flowId);
  }

  return ids;
}
