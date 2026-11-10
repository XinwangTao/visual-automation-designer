# Changelog

所有重要变更记录。版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

---

## [Unreleased]

### Added
- **GUI 目标测试**：点击/等待图片配置面板新增「测试目标」按钮，只读解析当前屏幕（图片分数、命中矩形、UIA 控件与遮挡窗口），不会点击或提窗。
- **图片匹配阈值可配**：点击（图片模式）与等待图片支持 `threshold`，GUI 与流程 JSON 双向保存。
- **UIA 控件配置 UI**：窗口标题、名称、AutomationId、类名、控件类型、序号、跳过遮挡校验。
- **UI Refresh**：新增 `styles/ui-refresh.css` 最终视觉层——紧凑工具栏与 SVG 图标、工具箱列表化、节点卡片缩小到 236px、配置面板统一输入/按钮、底部面板折叠清理、弹窗与流程列表重排、亮/暗主题统一。
- **Inspira UI 风格交互（React/CSS 复刻，MIT）**：Aurora 极光背景、Card Spotlight 鼠标聚光、Shimmer 主按钮流光、选中/运行节点的 Border Beam、主按钮 Ripple 按压反馈。
- **液态玻璃背景（重做）**：低饱和漂移光场 + 细颗粒；工具栏/侧栏/画布/日志/配置/弹窗全部浮动磨砂玻璃，带渐变边框、内高光与柔和阴影；积木图标统一为单色线性 SVG，移除可见 emoji；支持 `prefers-reduced-motion`。
- **液态玻璃背景**：缓慢漂移的彩色光场 + 磨砂工具栏/侧栏/日志/弹窗（backdrop-filter），并提供 `prefers-reduced-motion` 降级。
- **GUI 试跑**：工具栏新增「试跑」按钮与 `dry_run_flow` 命令，按真实控制流解析目标但不点击、不输入、不提窗、不等待，返回每步证据。
- **OCR 验证与文字点击**：修复 Windows OCR（IBuffer 直接建位图、真实行坐标、语言标签映射、`--scale` 放大）；新增 `vadctl ocr` / MCP `vad_ocr`，`click-target --ocr --text` 可按屏幕文字点击。
- **视觉断言**：`vadctl diff` / MCP `vad_diff` 对比参考图，返回差异率与热力图；`find --all` 返回多候选与分数。
- **窗口激活**：`vadctl activate` / MCP `vad_activate` 只提窗不点击；窗口截图/OCR 前自动提窗，避免截到覆盖者。
- **安全约束**：`type`/`key` 在目标未真正前台时拒绝输入；drag 失败也会释放鼠标键，MCP 超时 best-effort mouse-up；截图默认拒绝覆盖、原图与预览分离；`--help` 不再触发真实动作；`wait --expect-value` 必须配 `--uia`。
- **UIA 值读取**：`UIA` 节点新增 `value`（ValuePattern），`vad_wait` 支持 `--expect-value` / MCP `expectValue`，可等待编辑框/下拉框出现指定文本。
- **Agent 闭环原语**：`vadctl windows`（窗口+矩形+pid）、`screenshot`（全屏/区域/窗口，可缩放）、`type`、`key`、`scroll`、`drag`、`wait`（窗口/控件/纯等待）；点击支持左右键/双击/次数。
- **MCP 14 工具**：新增 `vad_windows` / `vad_screenshot` / `vad_type` / `vad_key` / `vad_scroll` / `vad_drag` / `vad_wait`；`vad_screenshot` 直接返回 image content，Claude 可在对话里直接看图。
- **`docs/AI_AGENT_PLAYBOOK.md`**：Claude Code 完成任意 UI 自动化的观察→执行→验证循环、选择器语义与安全规则。

- **`vadctl` 无头调试入口**（`src-tauri/src/bin/vadctl.rs` + `src-tauri/src/cli/`）：
  `doctor` / `validate` / `list` / `get` / `find` / `uia` / `click-target` / `run` / `step`，统一 JSON 输出与退出码 `0/2/3/4/5`。
- **执行证据**：`run`/`step` 写入 `runs/<timestamp>/report.json`，每步包含截图、匹配分数、命中矩形、耗时、尝试次数与错误。
- **UIA 目标定位**（`platform/uia.rs`，移植自 MIT 许可的 mouse-recorder）：
  窗口标题子串 + 控件 name 子串 / automationId / class / controlType / index。
- **点击安全门**：UIA 点击前先提窗（AttachThreadInput + SetForegroundWindow），再重读控件矩形，最后用 `WindowFromPoint + GetAncestor(GA_ROOT)` 校验遮挡；被遮挡默认拒点，`--force` 才绕过。
- **`vad-mcp`**：MCP stdio 薄包装，转调 `vadctl --json`，暴露 `vad_run` / `vad_validate` / `vad_find` / `vad_uia_find` / `vad_click_target` 等工具。
- **模板搜索预算**：`find_image_bounded` 支持 ROI、3 秒时间预算、大模板粗扫 + 局部细扫；CLI `find` 默认使用。
- **Dry-run**：解析坐标/模板/UIA 目标但不点击、不提窗、不等待、不写差异热力图。

### Fixed
- **负坐标回绕**：点击坐标从 `u32` 改为 `i32`，`match_center` 不再把负的显示器原点强转 `u32`；副屏在主屏左侧时不再点到 `(-1,-1)`。
- **单步重复执行**：`Executor::step()` 执行后推进 `current_block`，下一次 step 不再重复执行同一块；流程结束后也不从入口重跑。
- **阈值状态泄漏**：`find_image_on_screen` 的临时阈值只在单次调用内生效，不再污染后续 `None` 调用。
- **UIA 遮挡拒点**：覆盖窗口时返回目标错误（退出码 3）而不是把点击落到错误应用。

### Changed
- 执行引擎从 `tauri::AppHandle` 解耦为 `EventSink`，GUI、CLI、MCP 共用同一执行核心。
- 点击坐标语义统一为虚拟桌面物理像素（可为负），GUI 配置面板新增 UIA 点击模式。

### Verified
- 实机：`doctor`、`validate`、`run --dry-run`（图片 30.2% 匹配 → 退出码 3 + 截图证据）。
- 实机：受控 WinForms 窗口 UIA 按钮点击成功（按钮回调写入标记文件）。
- 实机：置顶窗口覆盖同一控件时 `click-target --uia` 退出码 3，拒绝点击，错误信息指出覆盖窗口。
- MCP：`initialize → tools/list → tools/call(vad_doctor)` 全链路返回正常。

---

## [1.0.1] - 2026-07-11

### Changed
- 提取 `isInputElement` 到 `src/utils/dom.ts`，消除 `AppShell.tsx` 与 `useKeyboardShortcuts.ts` 中的重复代码

---

## [1.0.0] - 2026-06-09

### 🎉 V1 正式发布 — 功能完备，生产就绪

### Added
- **Phase A: 基础积木块体系** — Path + Mouse Input 节点、Condition + Loop 控制流节点、完善 Port 校验系统
- **Phase B: OCR / 文字检测** — Tesseract OCR 引擎集成、TextExtract 块（文字提取）、TextCheck 块（文字条件跳转）
- **Phase C: ScreenshotAssert 截图断言** — 像素级截图比对块，支持 Region of Interest 裁剪、可配置阈值、严格模式、差异热力图生成
- **Phase D: 执行引擎加固** — 8 个差异比对单元测试、60 秒超时保护、可选下采样加速（diff_images_scaled）

### Changed
- 全版本号统一提升至 **1.0.0**，语义化版本正式发布

### Technical
- 新增 `DiffResult` / `diff_images()` / `diff_images_scaled()` 差异比对引擎
- 新增 `execute_screenshot_assert_block()` 截图断言执行器
- 新增 `ExecutionContext.variables` 运行时变量存储
- 新增 `Rect` + `parse_region()` 区域裁剪工具
- 新增 `SCREENSHOT_ASSERT_TIMEOUT_SECS` 超时保护常量
- TypeScript 端补齐 `toTauriBlockConfig` 中 text_extract / screenshot_assert / text_check 三个缺失分支，修复 TS 编译阻断

---

## [0.5.23] - 2026-06-09

### Added
- ScreenshotAssert 块类型：数据模型（`ActionType::ScreenshotAssert`, `BlockConfig::ScreenshotAssert`）、端口定义、前端类型、BlockNode UI
- `ImageMatcher::diff_images()`：逐像素灰度 absdiff 差异比对，支持热力图覆盖、尺寸自适应
- 图片差异比对单元测试 8 个（覆盖完全一致、完全差异、单像素差异、热力图、尺寸缩放、零阈值、高阈值、空图）
- `diff_images_scaled()`：可选下采样加速方法，scale_factor 参数 [0.01, 1.0]，6 个额外测试
- 执行引擎超时保护：`image::open` + `ScreenCapture::capture_screen` 60 秒超时，嵌套 `safe_execute` + `tokio::time::timeout`
- `ExecutionContext.variables`：通用运行时变量 HashMap，支持 `set_variable` / `get_variable` / `clear_variables`

### Fixed
- `toTauriBlockConfig()` 补齐 text_extract / screenshot_assert / text_check 三个 switch 分支，消除 TS 编译阻塞

### Technical
- `Rect` + `parse_region()` 工具函数，支持可选 ROI 裁剪
- 前端 BlockNode 映射：ScreenshotAssert → 颜色 #f44336 / 图标 📸

---

## [0.5.22] - 2026-06-08

### Performance
- **大幅加速 NCC 图像匹配**：引入积分图（Summed-Area Table）和 GrayBuffer 灰度缓存，将 NCC 计算复杂度从 O(w×h) 降低到 O(1)，单帧匹配速度提升约 5-10 倍
- GrayBuffer 预计算：一次性缓存模板和搜索窗口的灰度数据，消除冗余像素读取和灰度转换
- SAT (Summed-Area Table) 加速：通过积分图在常量时间内计算任意矩形区域的像素和与平方和，彻底重构方差计算路径

### Fixed
- 修复边缘情况下的浮点除零问题：当检测到零方差时使用 VARIANCE_EPS 兜底，防止 NCC 计算溢出

### Technical
- 新增 `GrayBuffer` 结构体封装灰度数据缓存，支持灰度值、均值、方差预计算
- 新增 `IntegralImage` 结构体实现积分图（SAT），支持快速矩形区域查询
- 内联文档覆盖所有公共方法，消除所有 dead code 警告

---

## [0.5.0] - 2026-05

### Added
- 无障碍 (a11y) 合规和所有组件的微交互
- 设计令牌系统，支持亮色/暗色双主题
- 多平台发布 CI 工作流 (Windows / macOS Intel+Apple Silicon / Linux)
- 执行流失败事件与失败状态持久化

### Changed
- **重构**：拆分超大 Rust 后端文件（executor.rs → 6 个子模块，validator.rs → 3 个子模块）
- **重构**：App 拆分为 AppShell 和 AppProviders
- **重构**：BlockConfig 拆分为 ActionBlockConfig 和 ControlBlockConfig
- **重构**：useFlow.ts 拆分为 4 个子 hook（nodes, edges, history, validation）
- **重构**：从 FlowCanvas 提取 useDragDrop 和 useCanvasShortcuts

### Fixed
- 修复 useFlow 和 FlowCanvas 中 4 个 P0 重渲染问题
- 修复 CI 构建中缺失的 diagnostics/logging 模块
- 修复 BlockConfig 和 ClickMode 枚举的 serde 命名字段

### Security
- 添加自动化 API 密钥和密钥扫描器到 git-sync

---

## [0.4.11] - 2026-05

### Fixed
- 修复图片依赖块在无图片时可创建的问题
- 运行时保护图片依赖执行的守卫逻辑

---

## [0.4.10] - 2026-05

### Added
- 覆盖所有块类型的快速添加创建测试
- 块配置兼容可选图片 ID 的测试覆盖

### Fixed
- Rust 块模型中接受可选 image_ids 字段

---

## [0.4.8] - 2026-04

### Added
- 执行控制台状态持久化
- 覆盖已存在节点的精准放置测试

### Fixed
- 收紧执行等待节奏
- 执行控制台交互改进

---

## [0.4.7] - 2026-04

### Added
- 控制流结构校验集成
- 不支持的控制流连线引导提示
- 运行时自检能力
- 执行失败状态处理
- 节点上高亮显示校验问题

### Fixed
- 编辑后刷新校验状态
- 执行前校验流程
- 输入动作可被暂停感知
- 等待块遵守暂停状态

---

## [0.4.6] - 2026-04

### Added
- 桌面应用图标资源更新
- 画布上设置入口节点
- 新手引导和浏览器模式编辑回退

### Fixed
- 保存前派生稳定入口块
- 执行前规范化流程状态
- Tauri 命令的块配置序列化
- 流程编辑后刷新撤销状态

---

## [0.4.0] - 2026-03

### Added
- 流程编辑状态与执行历史能力的完善
- Redo 语义修正及历史回归测试补充
- 多轮 UX 优化 (1-8 轮，共 160 项改进)
  - 日志筛选搜索
  - 键盘提示
  - 颜色图例
  - 执行时间线
  - 智能建议
  - 无障碍优化
  - 暗色主题增强
  - 快捷键帮助面板
  - 状态栏增强
  - 拖拽预览
  - Toast 通知等

### Fixed
- 拖拽放置和剪贴板粘贴图片功能改进
- 图片选择器上传和删除功能

---

## [0.1.0] - 2025-12

### Added
- 初始版本：Visual Automation Designer 基础框架
- Tauri 2 + React 18 + ReactFlow 11 项目骨架
- 多平台 GitHub Actions 发布工作流