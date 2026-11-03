# TASK GUIDE

这个文件不是按目录介绍，而是按“你现在要做什么”来给出切入点。

---

## 1. 我想修“画布显示”和“实际执行”不一致

先看：
1. `src/hooks/useFlow.ts`
2. `src/hooks/useFlowValidation.ts`
3. `src-tauri/src/commands/flow.rs`
4. `src-tauri/src/commands/execution.rs`
5. `src-tauri/src/core/flow/`
6. `src-tauri/src/core/execution/`

重点问题：
- 入口块是谁决定的
- 条件分支和循环 children 在哪里派生
- 执行前是否保存了当前 flow
- 前端状态和后端序列化对象是否一致

---

## 2. 我想新增一种 block / 动作能力

先看：
1. `src-tauri/src/core/blocks/`
2. `src-tauri/src/models/block.rs`
3. `src/components/BlockToolbox/`
4. `src/components/ConfigPanel/`
5. `src/types/`
6. `src/hooks/useFlowNodes.ts`

通常会跨越的层：
- block 类型定义
- 前端 toolbox 展示
- 配置面板表单
- 持久化模型
- 执行引擎中的运行逻辑

---

## 3. 我想改执行控制（运行 / 暂停 / 继续 / 单步）

先看：
1. `src/hooks/useExecution.ts`
2. `src/components/ExecutionStatus/`
3. `src-tauri/src/commands/execution.rs`
4. `src-tauri/src/core/execution/`

重点关注：
- 前端状态如何监听 `execution-event`
- 后端 stop/pause/resume 标志位如何维护
- 单步执行是否和全量执行共享语义

---

## 4. 我想改 flow 校验 / 错误提示

先看：
1. `src/validation/`
2. `src/hooks/useFlowValidation.ts`
3. `src/components/` 中展示 validation 的地方
4. `src-tauri/src/core/flow/`
5. `API.md` 中 `validate_flow`

重点关注：
- 哪些规则应在前端提示，哪些规则必须后端兜底
- error 与 warning 的边界
- 是否能把结构问题定位到节点级别

---

## 5. 我想改图片库 / 图像匹配

先看：
1. `src/components/ImageLibrary/`
2. `src/hooks/useImageLibrary.ts`
3. `src-tauri/src/commands/image_library.rs`
4. `src-tauri/src/core/image_library/`
5. `src-tauri/src/matching/`

重点关注：
- 图片元数据和文件存储如何组织
- 匹配失败如何反馈给执行层和 UI
- 截图 / 匹配 / 点击三步是否共享坐标语义

---

## 6. 我想排查桌面平台问题（权限、输入、屏幕、DPI）

先看：
1. `src-tauri/src/platform/screen.rs`
2. `src-tauri/src/platform/input.rs`
3. `src-tauri/src/commands/execution.rs`
4. `PROJECT_PLAN.md` 当前焦点

重点关注：
- 多显示器与 DPI 缩放
- 坐标映射
- 操作系统级权限失败
- 环境自检 `runtime_self_check`

---

## 7. 我想做发布 / 版本 / 安装包相关工作

先看：
1. `package.json`
2. `src-tauri/Cargo.toml`
3. `src-tauri/tauri.conf.json`
4. `scripts/verify-versions.cjs`
5. `PROJECT_PLAN.md` 的 Release Checklist

最重要的事：
- 三处版本必须一致
- 常规验证链不能跳：`test` / `lint` / `build`
- 涉及桌面运行时或发布行为时，要额外谨慎

---

## 8. 我只想快速建立 AI 上下文，最小输入怎么给

建议把下面几样一起给 AI：
1. `docs/ONBOARDING.md`
2. `docs/PROJECT_MAP.md`
3. 你当前要做的任务说明
4. 必要时再补 `.understand-anything/knowledge-graph.json`

如果问题偏运行时，再补：
- `ARCHITECTURE.md`
- `API.md`
- 相关 hook / command / core 文件路径
