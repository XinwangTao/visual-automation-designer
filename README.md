# Visual Automation Designer

可视化屏幕自动化流程编辑器 — 通过拖拽节点和连线，像画流程图一样编排屏幕自动化任务。

## 技术栈

| 层级 | 技术 |
|------|------|
| 桌面框架 | Tauri 2 |
| 前端 UI | React 18 + ReactFlow 11 |
| 语言 | TypeScript + Rust |
| 构建工具 | Vite 5 |
| 测试 | Vitest + Rust unit tests |
| 代码质量 | ESLint 9 |

## 快速开始

### 前置条件

- **Node.js** 20+
- **Rust** 1.80+（通过 [rustup](https://rustup.rs) 安装）
- Windows 10+ / macOS 12+ / 主流 Linux 发行版

### 开发运行

```bash
npm install
npm run tauri dev
```

首次编译 Rust 后端可能需要几分钟下载依赖，后续增量编译会快很多。

### 其他命令

```bash
npm run dev          # 仅启动 Vite 前端（浏览器模式）
npm run build        # TypeScript 检查 + Vite 构建
npm run test         # 运行 Vitest 测试
npm run lint         # ESLint 检查
npm run tauri build  # 打包桌面应用
```

## AI 无头调试（vadctl / vad-mcp）

GUI 之外提供两个控制台程序，供 AI Agent 和脚本直接调用；它们复用同一套 Rust 执行核心：

```bash
cd src-tauri
cargo build --bins --release
# 产物：
#   target/release/vadctl.exe    命令行调试入口
#   target/release/vad-mcp.exe   MCP stdio 服务（转调 vadctl）
```

常用命令：

```bash
vadctl doctor                                  # 数据目录/显示器/输入/UIA 自检
vadctl validate --flow flow.json               # 结构化校验错误（含 blockId）
vadctl run --flow flow.json --dry-run          # 只解析目标，不点击
vadctl run --flow flow.json                    # 真跑并写证据
vadctl step --flow flow.json --index 3         # 从入口执行前 3 个块（游标不跨进程）
vadctl find --template ok.png --roi 0,0,800,600
vadctl uia --window "记事本" --name 编辑 --control-type Edit
vadctl click-target --uia --window "记事本" --name 确定 --dry-run

# Agent 闭环原语
vadctl windows                                   # 窗口标题/矩形/pid/前台
vadctl screenshot --window "记事本" --max-width 1280
vadctl type --window "记事本" --uia --automation-id 15 --text "hello" --press-enter
vadctl key --keys "ctrl+s" --window "记事本"
vadctl scroll --amount -3 --coords 1200,700
vadctl drag --from 100,100 --to 400,300 --duration-ms 500
vadctl wait --window "记事本" --timeout-ms 5000
vadctl wait --uia --window "记事本" --automation-id 15 --expect-value "已保存" --timeout-ms 5000
vadctl find --template ok.png --all --limit 5         # 多候选（含分数/矩形）
vadctl ocr --window "记事本" --lang chi_sim --expect 保存
vadctl ocr --window "记事本" --lang chi_sim --scale 2  # 小字号放大后识别
vadctl click-target --ocr --window "记事本" --text 保存 --lang chi_sim
vadctl diff --reference before.png --window "记事本" --threshold 30
vadctl activate --window "记事本"
```

`docs/AI_AGENT_PLAYBOOK.md` 给出了 Claude Code 用 MCP 完成任意 UI 任务的观察→执行→验证循环、选择器约定与安全规则。

`run`/`step` 的证据目录：`<data-dir>/runs/<timestamp>/`（默认 `%APPDATA%\com.visualautomation.designer`），包含每步截图与 `report.json`：

```json
{
  "schemaVersion": 1,
  "ok": false,
  "steps": [
    {
      "index": 0,
      "blockId": "...",
      "action": "click:uia",
      "ok": true,
      "attempts": 1,
      "score": 1.0,
      "hitRect": { "x": 120, "y": 340, "width": 80, "height": 24 },
      "elapsedMs": 42,
      "screenshot": "00_xxx.png",
      "target": "uia Button \"确定\""
    }
  ]
}
```

退出码：`0` 成功；`2` 用法/解析/校验失败；`3` 目标未找到或被遮挡；`4` 断言/预期失败；`5` I/O/COM/运行时错误。

UIA 点击默认先提窗、重读控件矩形并用 `WindowFromPoint` 校验遮挡；被其他窗口遮挡时拒绝点击（退出码 3），只有 `--force` 才跳过校验。

MCP 用法（Claude Code 等）：把 `vad-mcp.exe` 作为 stdio MCP server 注册即可，工具列表包含 `vad_run`、`vad_validate`、`vad_find`、`vad_uia_find`、`vad_click_target` 等。若 `vad-mcp` 不在 `vadctl` 同目录，设置 `VADCTL_PATH`。

## 核心功能

- **可视化流程编辑** — 基于 ReactFlow 的画布，拖拽节点、连线编排自动化步骤
- **屏幕截图比对** — 捕获屏幕区域，与预设图片进行像素级匹配，作为流程跳转条件
- **键盘鼠标模拟** — 支持点击、输入、快捷键、滚轮等完整鼠标/键盘操作
- **步骤调试** — 单步执行、暂停/继续、停止，实时查看每一步的执行状态
- **图片库管理** — 上传、管理用于图像匹配的参考图片
- **流程校验** — 执行前自动检查流程结构合法性，标记有问题的节点和连线
- **撤销/重做** — 完整的历史记录支持，跨编辑会话
- **双主题** — 亮色/暗色主题一键切换
- **DPI 自适应** — 自动检测显示器缩放比例，纠正坐标映射

## 项目结构

```
visual-automation-designer/
├── src/                          # 前端源代码
│   ├── main.tsx                  # 应用入口
│   ├── App.tsx                   # 根组件
│   ├── AppShell.tsx              # 应用壳层（布局框架）
│   ├── AppProviders.tsx          # Context 提供者组合
│   ├── components/
│   │   ├── App/                  # 应用级组件
│   │   ├── BlockToolbox/         # 步骤块工具箱（拖拽源）
│   │   ├── common/               # 通用 UI 组件
│   │   ├── ConfigPanel/          # 步骤配置面板（ActionBlockConfig / ControlBlockConfig）
│   │   ├── ExecutionStatus/      # 执行状态控制栏
│   │   ├── FlowEditor/           # 流程编辑器容器
│   │   │   └── FlowCanvas.tsx    # ReactFlow 画布
│   │   └── ImageLibrary/         # 图片库管理面板
│   ├── hooks/
│   │   ├── useFlow.ts            # 流程状态管理（编排层）
│   │   ├── useFlowNodes.ts       # 节点 CRUD 操作
│   │   ├── useFlowEdges.ts       # 连线 CRUD 操作
│   │   ├── useFlowHistory.ts     # 撤销/重做
│   │   ├── useFlowValidation.ts  # 流程校验状态
│   │   ├── useExecution.ts       # 执行控制
│   │   ├── useImageLibrary.ts    # 图片库管理
│   │   ├── useKeyboardShortcuts.ts # 键盘快捷键
│   │   └── useTheme.ts           # 主题切换
│   ├── types/                    # TypeScript 类型定义
│   ├── validation/               # 前端校验规则
│   └── styles/                   # 全局样式 & 设计令牌
├── src-tauri/                    # Rust 后端
│   ├── Cargo.toml                # Rust 依赖
│   ├── tauri.conf.json           # Tauri 配置
│   ├── icons/                    # 应用图标
│   ├── capabilities/             # 权限声明
│   └── src/
│       ├── main.rs               # 入口（windows_subsystem）
│       ├── lib.rs                # 应用初始化 & 命令注册
│       ├── error.rs              # 统一错误类型
│       ├── logging.rs            # 日志 & panic 处理
│       ├── commands/             # Tauri 命令层
│       │   ├── flow.rs           # 流程 CRUD / 块操作 / 连线操作 / 撤销重做
│       │   ├── execution.rs      # 执行控制 / 单步调试 / 状态查询
│       │   └── image_library.rs  # 图片库 CRUD
│       ├── core/                 # 核心业务逻辑
│       │   ├── flow/             # 流程管理器 / 校验器 / 序列化
│       │   ├── execution/        # 执行引擎 / 事件 / 图像匹配 / 输入模拟
│       │   ├── image_library/    # 图片库元数据管理
│       │   ├── blocks/           # 步骤块定义 & trait
│       │   └── history.rs        # 操作历史（撤销/重做）
│       ├── models/               # 数据模型
│       │   ├── block.rs          # BlockNode / BlockConfig / BlockType
│       │   ├── flow.rs           # Flow / Connection / FlowMetadata
│       │   └── image.rs          # ImageMetadata / ImageId
│       ├── platform/             # 平台抽象层
│       │   ├── screen.rs         # 屏幕捕获
│       │   └── input.rs          # 键盘鼠标模拟
│       └── matching/             # 图像匹配模块
├── docs/                         # 设计文档 & 规格说明
├── scripts/                      # 辅助脚本
├── package.json
├── vite.config.ts
├── tsconfig.json
└── vitest.config.ts
```

## License

MIT