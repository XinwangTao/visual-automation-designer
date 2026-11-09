# VAD AI 无头调试入口（vadctl）产品 brief

日期：2026-11-09
状态：已确认并实现（分支 `feat/ai-debug-cli`，对应图 `docs/archify/vadctl_ai_debug_workflow.html`）
参考实现：`F:\projects\mouse-recorder`（MIT，`README.md` 与 `src/cli*`）

## 用户与问题

用户是 AI Agent（Claude Code 等）和少量脚本。当前 `visual-automation-designer` 只能通过 GUI 手动执行流程，Agent 无法：

1. 在无人值守下跑一条流程并拿到机器可读结果；
2. 失败时知道「哪一步、差多少分、点到了哪里、被什么窗口挡住」；
3. 在不真正点击的情况下预演目标解析结果；
4. 用稳定的退出码决定重试、改流程还是重录参考图。

结果：AI 只能“写流程”，不能“自己调试流程”。

## In Scope（首期）

- 新增独立控制台程序 `vadctl`（Rust bin，复用现有 core），命令：
  - `doctor`：数据目录、显示器、输入权限、OCR 可用性自检，JSON 输出。
  - `validate --flow <path>`：调用现有 `FlowValidator`，输出 errors/warnings（code、blockId、message）。
  - `list` / `get`：列出/读取流程与图片库元数据。
  - `run --flow <path> [--dry-run] [--report-dir DIR] [--json]`：无头执行，逐步产出证据。
  - `step --flow <path> --index N`：只解析并执行第 N 步，便于 Agent 二分定位。
  - `find --template PATH [--roi x,y,w,h] [--threshold T]`：截图并做模板匹配，返回分数、命中矩形。
  - `uia --window TITLE [--name/--automation-id/--class/--control-type/--index]`：枚举/过滤 UIA 控件及屏幕矩形。
  - `click-target`：坐标 / 模板 / UIA 三选一，带 `--dry-run` 与遮挡防护。
- 执行证据目录 `runs/<timestamp>/`：
  - 每步截图（成功与失败都留）、`score`、命中矩形、`elapsed_ms`、`attempts`、`error`；
  - `report.json`（schema_version + 结构化步骤数组）；
  - stdout `--json` 输出同一份精简结果。
- 退出码：`0` 成功；`2` 用法/解析/校验失败；`3` 目标未找到或被遮挡；`4` 断言/预期失败；`5` I/O/COM/运行时错误。
- 定位增强（移植自 mouse-recorder）：
  - UIA 目标定位：窗口标题子串匹配 + name 子串、automation_id/class/control_type 全等（忽略大小写）+ index；
  - 点击安全门：先提窗（AttachThreadInput + SetForegroundWindow，失败用 SetWindowPos 兜底），再重读控件矩形，最后 `WindowFromPoint + GetAncestor(GA_ROOT)` 校验；被遮挡拒点，`--force` 才跳过；
  - 模板搜索硬化：ROI 优先、全屏 3 秒预算、大模板粗扫 + 局部细扫。
- 坐标模型修正（前置缺陷，不修则定位增强无意义）：
  - 点击坐标从 `u32` 改为 `i32`，匹配中心不再把负的显示器原点强转 `u32`；
  - 统一物理像素 + 虚拟桌面坐标系，副屏在左/上时不再回绕到 `(-1,-1)`。
- 执行核心解耦：`Executor` 的事件发送从 `AppHandle` 抽象为 `EventSink`，CLI/MCP/GUI 共用同一执行核心。
- 单步语义修正：`step` 命令执行后推进 `current_block`，避免同一块被重复执行（现有 `Executor::step()` 缺陷）。
- 有界重试：`run`/`step` 支持步骤级重试（默认 2 次），每次重试重新解析目标；UIA 被遮挡不重试、直接报 3。
- MCP 薄包装：`vad-mcp` stdio server，工具名 `vad_doctor` / `vad_validate` / `vad_run` / `vad_find` / `vad_uia_find` / `vad_click_target`，内部转调 `vadctl --json`，不重复实现业务逻辑。

## Out of Scope（首期不做）

- 浏览器 DOM 自动化、反作弊/内核级输入。
- 常驻调度服务、多机分发。
- 自适应阈值、多状态参考图、跨分辨率自动缩放（后续版本）。
- 自动修改流程结构或参考图；AI 只读证据并决定下一步。
- 替换或重写 GUI 现有编辑体验。
- OCR 目标定位与 OCR 链路修复：本期不把 OCR 作为点击目标。已知缺陷（`TextExtract` 丢弃结果、`TextCheck` 分支为空、OCR 行坐标恒为 0、语言标签与 Windows OCR 不匹配）记录在案，留到下一版。

## 与本期直接相关的既有缺陷（证据见只读分析）

| 缺陷 | 位置 | 本期处理 |
|------|------|----------|
| 窗口/UIA 完全缺失 | `platform/input.rs` 无窗口 API；`Cargo.toml` 无 Win32 UI feature | 新增 `platform/uia.rs` + crate feature |
| 负坐标回绕 | `models/block.rs:95`（u32）、`core/execution/image_match.rs:16-25`（`origin as u32`） | 改 `i32` 并修换算 |
| 模板匹配无 ROI/预算 | `matching/matcher.rs:340-371,435-494` | 新增有界搜索 API |
| 失败无证据落盘 | `core/execution/runner.rs:509-517` 仅日志 | 新增 `runs/<ts>/report.json` |
| 单步重复执行 | `core/execution/runner.rs:291-321` 不推进 `current_block` | 修正 `step` 语义 |
| 执行器耦合 AppHandle | `core/execution/runner.rs:84,197` | 抽象 `EventSink` |
| 图片库写入非原子 | `core/image_library/library.rs:136-146` | 本期不动（记录待办） |
| 流程无 schema_version | `models/flow.rs:101-123` | 本期不动（记录待办） |

## 成功标准（可验收）

1. `vadctl validate --flow <坏流程> --json` 退出 2，输出至少一条含 `blockId` 的结构化错误。
2. `vadctl run --flow <示例流程> --dry-run --json` 不产生任何真实点击/按键，但输出每步解析到的目标坐标/分数。
3. `vadctl run --flow <示例流程> --json` 成功时退出 0，`runs/<ts>/report.json` 含每步截图、`score`、`hit_rect`、`elapsed_ms`。
4. 目标被其他窗口遮挡时，`vadctl click-target --uia ...` 退出 3，错误信息包含遮挡窗口标题；加 `--force` 才会点击。
5. `vadctl uia --window "记事本" --json` 返回控件树（name/automationId/class/controlType/rect）。
6. MCP server 可被 Claude Code 配置后调用 `vad_run`，返回与 CLI 一致的 JSON。
7. 现有前端 `npm run test` 与后端 `cargo test` 全绿；GUI 行为无回归。

## 主路径场景

1. **AI 跑流程并定位失败**：validate → run（dry-run 预演）→ run 真跑 → 读 report 失败步骤 → 用 `find`/`uia` 单独复核目标 → 修改流程或重录图片 → 再跑。
2. **UIA 点击安全路径**：uia 查控件 → click-target 带提窗与遮挡校验 → 点击后截图留证。
3. **图片匹配调参**：find 带 ROI 与阈值 → 看 score 与命中矩形 → 调整阈值/ROI 后重跑。
4. **MCP 调用**：Agent 通过 MCP 工具触发上述命令，拿到结构化结果。

## 关键失败 / 人工兜底

- 流程 JSON 损坏或版本不支持 → 退出 2，不回写、不执行。
- 目标窗口不存在/最小化/控件零尺寸 → 退出 3，报告写明选择器。
- 点击点被遮挡 → 退出 3，报告遮挡窗口标题；`--force` 显式绕过。
- 断言失败 → 退出 4，保留期望与实际截图。
- UIA COM 异常或超时 → 退出 5，回退模板/OCR 建议写入报告。
- 任何一步失败都不静默继续：默认短路；`--continue-on-error` 才继续并在报告标记 `ok=false`。

## 关键约束

- Windows 10/11，PMv2 DPI 感知；坐标统一物理像素。
- 不新增外部服务依赖；MCP server 用 stdio + 手写 JSON-RPC，不引入 MCP SDK。
- `vadctl` 复用现有 `core::flow` / `core::execution` / `matching` / `platform`，不复制业务逻辑。
- 执行引擎需从 `AppHandle` 解耦为事件 sink，GUI 行为保持不变。
- 流程文件保持现有 JSON 结构（`Flow` serde），不新增私有格式；不破坏现有 `flows/<id>.json`。
- 证据目录独立于 `images/`，不污染图片库。
- 迁移借鉴 MIT 代码时在文件头注明来源与许可。

## 借鉴调研（1 回合）

- **mouse-recorder `--cli`**（同作者 Rust 项目）：采纳「单命令 + JSON + 退出码 + runs 证据目录」的 Agent 交互模型；采纳 UIA 选择器语义、提窗后重读矩形 + WindowFromPoint 校验、ROI 优先 + 3 秒预算 + 粗扫/细扫、有界重试。
- **pywinauto**（历史调研，见 mouse-recorder brief）：控件选择器（name/automation_id/class/type）已被 mouse-recorder 落地，直接复用其语义，不再引入 Python。
- **不采用**：mouse-recorder 的 SQLite 元素库与录制 GUI（VAD 已有图片库与画布）；自适应阈值（首期不做）。

## 对架构的直接影响

1. `Executor` 的事件发送从 `AppHandle` 抽象为 `EventSink`，CLI/MCP 与 GUI 共用同一执行核心。
2. 新增 `platform::uia` 模块与 `windows` crate 的 UIA/线程/窗口 feature。
3. 新增两个 bin（`vadctl`、`vad-mcp`），共享 lib。
4. 流程加载支持任意文件路径（AI 可直接读写工程内 JSON）。

## 待确认

1. 是否同意首期只覆盖上面 5 类定位/执行能力（坐标、模板、OCR、UIA、窗口守卫），自适应阈值与多状态留到下一版？
2. `vadctl` 与 `vad-mcp` 作为独立 exe 交付是否可接受（不塞进 Tauri GUI 二进制）？
3. MCP 服务器是否按「转调 vadctl」的薄包装实现（推荐，便于两边行为一致）？

## 实现记录（2026-11-09）

已确认：CLI + MCP 薄包装；首期做「调试闭环 + 定位增强」；两个独立 exe。

| 改动 | 位置 |
|------|------|
| `vadctl` 命令入口 | `src-tauri/src/cli/mod.rs`、`src-tauri/src/bin/vadctl.rs` |
| `vad-mcp` 薄包装 | `src-tauri/src/bin/vad_mcp.rs` |
| UIA 定位与点击守卫 | `src-tauri/src/platform/uia.rs`、`src-tauri/src/core/execution/input_sim.rs` |
| 执行事件解耦 | `src-tauri/src/core/execution/sink.rs`、`runner.rs`、`commands/execution.rs` |
| 证据报告 | `src-tauri/src/cli/mod.rs`（`CliSink` / `RunReport`） |
| 模板搜索预算/ROI | `src-tauri/src/matching/matcher.rs`（`find_image_bounded`） |
| 负坐标修正 | `src-tauri/src/models/block.rs`、`image_match.rs`、`platform/input.rs` |
| 单步推进 | `src-tauri/src/core/execution/runner.rs`（`step`） |
| GUI UIA 配置 | `src/types/block.ts`、`src/components/ConfigPanel/ActionBlockConfig.tsx` |

验证：`cargo test --lib` 337 passed；`cargo test --bins`（见提交记录）；`npm run build`、`npm run lint` 通过；`npm run test` 仅 2 个既有 `App.test.tsx` 前端异常提示用例失败（主干同样失败，非本次回归）。
