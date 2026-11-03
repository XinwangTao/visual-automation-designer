# ONBOARDING

## 1. 这个项目是什么

`visual-automation-designer` 是一个 **React + TypeScript + Tauri + Rust** 的桌面自动化流程编辑器。它的核心目标不是“展示流程图”，而是让画布上的流程结构和桌面运行时行为保持一致：用户在前端拖拽节点、配置动作和控制流，最终由 Rust 执行引擎驱动真实的屏幕自动化。

如果你第一次接手它，应该先把它看成一个 **双运行面系统**：
- **前端面**：负责编辑体验、状态可视化、配置和校验反馈
- **后端面**：负责执行语义、系统调用、输入模拟、图像匹配与平台抽象

这也是为什么这个仓库比普通前端项目更难：真正的正确性来自 **UI → hooks → Tauri commands → Rust core → platform** 整条链路是否一致。

---

## 2. 最快上手路线

### 路线 A：只想先理解整体架构
按这个顺序读：
1. `README.md` — 目标、功能、技术栈
2. `ARCHITECTURE.md` — 前后端分层与数据流
3. `PROJECT_PLAN.md` — 当前阶段、优先级、维护规则
4. `API.md` — 前端能调用哪些 Tauri command

### 路线 B：想理解前端编辑器怎么工作
按这个顺序读：
1. `src/main.tsx`
2. `src/App.tsx`
3. `src/AppShell.tsx`
4. `src/hooks/useFlow.ts`
5. `src/hooks/useExecution.ts`
6. `src/components/FlowEditor/` 与 `src/components/ConfigPanel/`

### 路线 C：想理解桌面执行引擎怎么工作
按这个顺序读：
1. `src-tauri/src/lib.rs`
2. `src-tauri/src/commands/execution.rs`
3. `src-tauri/src/core/execution/`
4. `src-tauri/src/platform/`
5. `src-tauri/src/matching/`

---

## 3. 你需要先建立的三个心智模型

### 模型一：画布不是真相，执行链路才是真相
用户在画布上的连接、入口块、条件分支和循环语义，最终都必须被后端解释成稳定执行路径。很多 bug 都不是单个函数坏了，而是“前端以为 A，后端实际执行 B”。

### 模型二：hooks 层是前端的编排中枢
UI 组件不是重点，`useFlow` / `useFlowNodes` / `useFlowEdges` / `useFlowHistory` / `useFlowValidation` / `useExecution` 才是把交互、持久化、校验和执行控制粘在一起的地方。

### 模型三：Rust 端不是纯 API 层，而是业务主引擎
`commands/*.rs` 只是入口。真正关键的语义在 `core/flow`、`core/execution`、`blocks/`、`platform/`、`matching/`。

---

## 4. 关键目录怎么理解

### 前端
- `src/components/`：可视化编辑器、配置面板、执行状态和图片库 UI
- `src/hooks/`：前端状态编排、校验、历史记录、执行控制
- `src/types/`：前端类型契约
- `src/validation/`：执行前结构规则和错误提示逻辑

### 后端
- `src-tauri/src/commands/`：前端 invoke 能直接打到的命令层
- `src-tauri/src/core/`：流程管理、执行引擎、块定义、图片库、历史
- `src-tauri/src/models/`：前后端共享的数据模型
- `src-tauri/src/platform/`：屏幕与输入的 OS 抽象
- `src-tauri/src/matching/`：图像匹配逻辑

### 工程与运维
- `scripts/`：校验、构建或辅助脚本
- `docs/`：补充设计和规格说明
- `.understand-anything/knowledge-graph.json`：给 AI/图谱工具的结构化项目图

---

## 5. 新维护者最容易踩的坑

1. **只看 React 组件，不看 hooks 和 Rust 命令链路**
   - 会误判问题归属，修完 UI 但运行时仍错。
2. **把浏览器模式当成桌面真相**
   - 这个项目以桌面运行正确性为最高优先级。
3. **只修前端数据结构，不验证序列化与命令参数形状**
   - Tauri payload 和 Rust 模型一旦偏差，就会出现“看起来能配，运行就错”。
4. **忽略入口块、条件分支、循环 children 的派生逻辑**
   - 这些属于历史高风险区。
5. **只跑单个测试，不跑完整验证链**
   - 常规代码变更至少要跑：`npm run test`、`npm run lint`、`npm run build`。

---

## 6. 推荐的最小工作方式

如果你准备修改代码，先做这组动作：
1. 看 `PROJECT_PLAN.md` 里的当前焦点
2. 找到你改动对应的前端 hook / 后端 command / core 模块
3. 明确这是“编辑期问题”还是“运行期问题”
4. 改完后至少执行：
   - `npm run test`
   - `npm run lint`
   - `npm run build`
5. 如果碰到桌面运行或发布相关变更，再考虑 `npm run tauri build`

---

## 7. 最适合问 AI 的问题

这个仓库已经适合用结构化提问，不必只问“这个文件是干嘛的”。更高价值的问题是：
- “用户在画布上新增一个 block 后，状态是如何从前端传到 Rust 的？”
- “条件块分支语义在哪些层被派生和校验？”
- “执行引擎如何把 flow 模型转换成一步步的桌面动作？”
- “如果运行结果和画布不一致，最可能先看哪些文件？”

---

## 8. 下一步建议

看完本文件后，继续阅读：
- `docs/PROJECT_MAP.md` — 更偏结构地图
- `docs/TASK_GUIDE.md` — 更偏任务入口
- `.understand-anything/knowledge-graph.json` — 给 AI/图谱工具使用的结构化底座
