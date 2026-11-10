# AI Agent Playbook — 用 vad MCP 完成任意 UI 自动化

面向 Claude Code 等 Agent。目标：**观察 → 决策 → 执行 → 验证**，全程只用结构化工具，不靠猜坐标。

## 0. 工具清单（MCP 名 `vad`）

| 阶段 | 工具 | 用途 |
|------|------|------|
| 观察 | `vad_windows` | 可见窗口的标题、pid、屏幕矩形、是否前台/最小化 |
| 观察 | `vad_uia_find` | 枚举控件及矩形；编辑框/下拉框同时返回当前 `value` |
| 观察 | `vad_screenshot` | 截全屏/区域/窗口，返回 PNG 图像 + 文件路径（默认缩到 1280 宽） |
| 观察 | `vad_capture` | 与 GUI「从屏幕截取」同一条管线：冻结桌面→裁剪→存进图片库，返回 `image` + `roi` + `center`；做图片点击/等待图片块时优先用它取模板 |
| 观察 | `vad_find` | 模板匹配当前屏幕，返回分数与命中矩形（可带 ROI；`all` 返回候选列表） |
| 观察 | `vad_ocr` | Windows OCR 读屏幕/区域/窗口文字，返回每行屏幕矩形 |
| 观察 | `vad_diff` | 与参考图做像素差异，返回差异率与热力图 |
| 执行 | `vad_click_target` | 点坐标 / 模板 / UIA 控件；支持左右键、双击、次数、`force` |
| 执行 | `vad_type` | 输入文本；可先点击 UIA 控件聚焦；可回车 |
| 执行 | `vad_key` | 按键/组合键，如 `enter`、`ctrl+s`、`alt+f4` |
| 执行 | `vad_scroll` | 滚轮，可先移动到坐标 |
| 执行 | `vad_drag` | 从一个点拖到另一个点 |
| 执行 | `vad_activate` | 只提窗/还原，不点击 |
| 等待 | `vad_wait` | 等窗口/控件出现、等控件 `value` 包含指定文本，或纯 sleep |
| 流程 | `vad_validate` | 校验流程 JSON，返回 blockId 级错误 |
| 流程 | `vad_run` / `vad_step` | 真跑 / 单步，证据写 `runs/<ts>/report.json` |
| 辅助 | `vad_doctor` / `vad_list` | 环境自检、流程/图片列表 |

## 1. 标准循环

1. `vad_windows` 找到目标窗口标题与矩形。
2. `vad_screenshot {window}` 看一眼真实界面；或 `vad_uia_find {window}` 拿控件树。
3. 优先用 UIA 控件；标准控件无 UIA 或无名字时用 OCR（`vad_ocr` / `vad_click_target {ocr:true}`）；再退到模板/坐标。
4. 动作：`vad_click_target` / `vad_type` / `vad_key` / `vad_scroll` / `vad_drag`。
5. 验证：`vad_wait {expectValue}` 等控件值变化，再截一次图，或 `vad_uia_find` 读控件 `value`，或读取应用产生的文件/窗口标题。
6. 失败时读错误 JSON：目标未找到/被遮挡/窗口未激活（3）、断言或 OCR 预期不符（4）、运行时（5）。

## 2. 选择器约定

- 窗口标题：**大小写不敏感子串**。
- `name`：**子串**；`automationId` / `class` / `controlType`：**全等、忽略大小写**。
- `index`：过滤后从 0 开始取第 N 个。
- UIA 点击会先提窗、再重读控件矩形、再用 `WindowFromPoint` 校验遮挡；被遮挡默认拒点（退出码 3），只有明确需要时才传 `force: true`。
- 不要用空选择器点击（会拒绝），也不要盲点屏幕中心。

## 3. 模板 / 视觉任务

1. `vad_screenshot {window, region}`：`path` 是**原始分辨率**参考图，`previewPath` 是给视觉看的缩放图。
2. 后续用 `vad_find {template: path, roi?}` 定位；分数低于阈值会返回 `found:false` 与最佳分数。
3. 需要点击时用 `vad_click_target {template: path, threshold?}`。
4. 截图会带 `originX/originY`；ROI 与坐标都是**虚拟屏幕物理像素**。

## 4. 流程化任务（可重复）

1. 把流程 JSON 写到磁盘（`Flow` 结构，`blocks` / `connections` / `entryBlock`）。
2. `vad_validate {flow}`，有错误按 blockId 修。
3. `vad_run {flow, dryRun: true}` 预演目标解析，不产生点击。
4. `vad_run {flow}` 真跑；读 `runs/<timestamp>/report.json` 的每步截图、分数、矩形、耗时、错误。
5. 失败优先读 `report.json` 定位；`vad_step` 每次调用都从入口执行 N 块，不要对已产生副作用的步骤反复 step。

## 5. 安全与可靠性

- 输入前先确认前台窗口；`vad_type` 带 `uia` 目标时会先点击聚焦。
- 对破坏性按钮（删除/发送/支付）先截图 + 读取控件名，再点击。
- 点击后留 100–300ms 再验证；UI 异步写入文件时用重试读取，不要一次读失败就判定失败。
- 不要用 `force` 绕过遮挡，除非明确知道覆盖窗口也是目标。
- 不要把未经验证的坐标写进流程；优先 UIA 或模板。

## 6. 最小示例（记事本输入并保存）

```text
vad_windows                                  → 找到 "记事本" 窗口
vad_uia_find {window:"记事本", controlType:"Edit"} → 拿到编辑框 automationId
vad_type {text:"hello", window:"记事本", uia:true, automationId:"15"}
vad_key {keys:"ctrl+s", window:"记事本"}
vad_screenshot {window:"记事本"}              → 目视确认
```


## 7. 已知边界

- MCP 调 vadctl 默认 120s 超时（`VAD_MCP_TIMEOUT_SECS` 可调）；超时会 kill 子进程并返回 `exitCode:5`。
- `vad_run` 的 retry 只作用于动作块；循环/条件块不会整棵重放。
- `vad_step --index N` 每次调用都从流程入口执行 N 个块，游标不跨进程保存；控制块会执行其子树。二分定位请优先读 `report.json`，避免重复点击/输入。
- 证据截图目前在动作完成后截取，且默认主屏；跨屏任务请用 `vad_screenshot {window/region}` 自行留证。
- `vad_screenshot` 默认缩到 1280 宽，适合视觉判断；需要像素级模板时用 `maxWidth` 放大或 `vad_find` 的 ROI。
