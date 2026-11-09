# 贡献指南

感谢你愿意改进 Visual Automation Designer。这份文档只讲「怎么让改动顺利合进来」，架构细节请看 [docs/ONBOARDING.md](docs/ONBOARDING.md)。

## 开发环境

- Node.js 20+
- Rust 1.80+（rustup 安装）
- 各平台 Tauri 系统依赖：Windows 需要 WebView2（Win10/11 自带），Linux 需要 `libwebkit2gtk-4.1-dev`、`libgtk-3-dev` 等

```bash
npm install
npm run tauri dev
```

## 提交前必须通过

CI 会跑这三条，本地先跑省一轮往返：

```bash
npm run lint     # ESLint，0 warning 才算过
npm test         # Vitest，全部用例通过
npm run build    # tsc 类型检查 + vite 构建
```

改到 Rust 侧（`src-tauri/`）时，另外跑：

```bash
cd src-tauri && cargo test && cargo clippy
```

## 代码约定

- 前端状态逻辑放在 `src/hooks/`，组件只负责渲染与事件接线；能被纯函数表达的判断优先抽成纯函数并配单测。
- Tauri command 的入参与返回值必须与 `src/types/` 里的类型保持一致；改名要同时改前端调用点、`src/tauri/` 封装和 `API.md`。
- 新增行为请补一条对应的单测；修复 bug 请先写一条能复现的失败用例。
- 注释写「为什么」，不写「做了什么」。

## 提交与 PR

- 提交信息用 `type(scope): 描述`，type 取 `feat` / `fix` / `refactor` / `perf` / `docs` / `chore`。
- 一个 PR 只做一件事；顺手重构请在描述里单独说明。
- PR 描述写清：解决什么问题、怎么验证的（命令或截图）、有无行为变化。
- 界面变更请附截图，Rust 侧变更请附 `cargo test` 输出。

## 报 Issue

请附：操作系统与版本、应用版本（设置或关于里能看到）、复现步骤、期望结果与实际结果、日志面板里相关的几行（可以截图）。缺这些信息的问题很难定位，通常会被追问一轮。
