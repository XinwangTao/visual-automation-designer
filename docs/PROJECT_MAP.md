# PROJECT MAP

## 项目定位

`visual-automation-designer` 的本质是一个 **桌面自动化编辑器 + 执行引擎** 组合体。它不是单纯前端应用，也不是单纯 Rust 工具，而是一个前后端语义必须一致的桌面系统。

---

## 1. 分层地图

### Documentation
用途：帮助维护者快速建立背景、标准和当前优先级。

建议先看：
- `README.md`
- `ARCHITECTURE.md`
- `API.md`
- `PROJECT_PLAN.md`
- `docs/specs/`

### Frontend
用途：实现画布编辑、块配置、状态展示、主题与交互。

关键入口：
- `src/main.tsx` — React 启动入口
- `src/App.tsx` — 根级应用组合
- `src/AppShell.tsx` — 页面骨架
- `src/components/FlowEditor/` — 画布层
- `src/components/ConfigPanel/` — 配置面板
- `src/components/ExecutionStatus/` — 执行状态反馈

### Hooks / State Orchestration
用途：把 UI 事件转成可保存、可校验、可执行的流程状态。

关键文件：
- `src/hooks/useFlow.ts`
- `src/hooks/useFlowNodes.ts`
- `src/hooks/useFlowEdges.ts`
- `src/hooks/useFlowHistory.ts`
- `src/hooks/useFlowValidation.ts`
- `src/hooks/useExecution.ts`

### Backend
用途：桌面运行时逻辑、流程执行语义、命令暴露。

关键入口：
- `src-tauri/src/lib.rs` — Tauri 初始化与 command 注册
- `src-tauri/src/commands/flow.rs`
- `src-tauri/src/commands/execution.rs`
- `src-tauri/src/commands/image_library.rs`

### Core Runtime
用途：真正定义流程如何管理、校验和执行。

关键目录：
- `src-tauri/src/core/flow/`
- `src-tauri/src/core/execution/`
- `src-tauri/src/core/blocks/`
- `src-tauri/src/core/image_library/`
- `src-tauri/src/history.rs`

### Platform & Matching
用途：接触真实桌面环境。

关键目录：
- `src-tauri/src/platform/`
- `src-tauri/src/matching/`

### Configuration / Infrastructure / Tooling
用途：构建、打包、版本、脚本与维护流程。

关键文件：
- `package.json`
- `tsconfig.json`
- `vite.config.ts`
- `vitest.config.ts`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`
- `scripts/`

---

## 2. 系统主链路

### 编辑链路
用户操作画布
→ React 组件事件
→ hooks 更新前端状态
→ invoke Tauri commands
→ Rust 端持久化/校验
→ 返回结果给前端重渲染

### 执行链路
用户点击执行
→ 前端调用 `execute_flow` / `step_execution`
→ `commands/execution.rs`
→ `core/execution` 执行引擎
→ `platform/` 调系统能力
→ `matching/` 做图像判断
→ 事件回流前端状态栏

### 校验链路
用户改 flow
→ `useFlowValidation`
→ 后端 `validate_flow`
→ 返回 error / warning
→ UI 高亮问题节点与状态栏反馈

---

## 3. 关键文件索引

### 如果你只读 8 个文件
1. `README.md`
2. `ARCHITECTURE.md`
3. `src/main.tsx`
4. `src/App.tsx`
5. `src/hooks/useFlow.ts`
6. `src/hooks/useExecution.ts`
7. `src-tauri/src/lib.rs`
8. `src-tauri/src/commands/execution.rs`

### 如果你要理解执行语义
1. `src-tauri/src/commands/execution.rs`
2. `src-tauri/src/core/execution/`
3. `src-tauri/src/core/blocks/`
4. `src-tauri/src/models/`
5. `src-tauri/src/platform/`

### 如果你要理解配置和校验体验
1. `src/components/ConfigPanel/`
2. `src/hooks/useFlowValidation.ts`
3. `src/validation/`
4. `API.md` 中的 `validate_flow`

---

## 4. 高复杂度热点

最值得谨慎阅读的区域：
- `src/hooks/`：因为它们承载前端真实语义编排
- `src-tauri/src/commands/execution.rs`：前后端执行控制桥
- `src-tauri/src/core/execution/`：执行引擎核心
- `src-tauri/src/core/blocks/`：块类型与控制流语义

这些地方既影响用户看见的行为，也影响实际执行结果。

---

## 5. 使用方式建议

### 给人看
- 先读本文件，再读 `ONBOARDING.md`
- 然后按你关心的任务去看 `TASK_GUIDE.md`

### 给 AI 用
- 优先喂 `.understand-anything/knowledge-graph.json`
- 再补这三类上下文：
  - 当前要改什么任务
  - 当前怀疑哪条链路有问题
  - 哪些文件已经读过
