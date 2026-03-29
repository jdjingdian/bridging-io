## 为什么

BridgingIO 现有的 UI 设计与规范已经围绕 `macOS SwiftUI` 和本地 control-plane 建立了清晰边界，但这条路径还没有形成一个可复用的跨平台桌面产品形态。继续围绕当前 macOS 版本局部修补，会把未来的 Windows / Linux / OpenHarmony 桌面宿主、以及后续 macOS 重构，一起绑在一套用户体验并不理想的旧界面结构上。

与此同时，团队已经明确希望采用 `Tauri Shell + bundled webview + Rust core` 的跨平台桌面方案，并让管理 UI 不再依赖额外本地 HTTP 端口。这意味着现在需要先把“桌面壳、bundled webview、首次启动引导、target/timeline/settings/vault 信息架构、以及本地可信安全动作”的产品与规格边界固定下来，避免后面 host、UI、vault/token 管理和 timeline 审计视图各自发散。

## 变更内容

- 新增一个平台中立的跨平台桌面控制台能力，定义 `Tauri Shell + bundled webview` 下的首启引导、主工作台和设置 / vault 管理面，而不是继续沿用当前 macOS UI 的页面划分和视觉结构。
- 将 bundled 桌面管理面明确为本地 GUI 宿主形态：桌面壳负责托盘 / 菜单栏、首次启动目录选择、窗口打开与恢复、系统通知、本地可信动作桥接，以及 core 生命周期管理；管理 UI 本身不再依赖额外本地 HTTP 管理端口。
- 为首次启动引导定义正式产品路径。桌面应用首次启动时必须引导用户选择 runtime 数据根目录，并在后续启动时基于该选择启动或恢复 managed core；该引导页必须与主工作台分离，并允许用户理解目录用途、权限与恢复语义。
- 为新的主工作台定义清晰的信息架构，至少覆盖三个一级入口：`Targets`、`Timeline`、`Settings`。其中 `Timeline` 需要支持按 token label 分组；若请求未配置 token，则必须回落到按 HTTP 请求指纹 / user-agent 等稳定来源摘要分组，以展示 agent 执行了哪些命令。
- 为设置界面定义正式的管理入口，至少覆盖 core 监听端口、日志大小、缓存大小、vault 管理与 token 管理。进入 vault 管理前，用户必须能够先查看已有 token 的安全摘要，并对 token 执行 revoke，而不是要求未来 UI 再自行拼装这些链路。
- 为跨平台桌面控制台建立一套新的简约设计系统与页面边界，使其既能指导 Tauri bundled webview 实现，也能为未来 macOS 重构提供参考；本次不要求视觉上继续跟随当前 macOS SwiftUI 版本。

## 功能 (Capabilities)

### 新增功能

- `desktop-operator-console`: 定义平台中立的跨平台桌面控制台能力，覆盖 Tauri Shell 宿主、bundled webview 管理面、首次启动引导、主工作台信息架构、设置 / vault / token 管理入口，以及新的桌面 UI 设计语言。

### 修改功能

- `target-session-management`: 增加 bundled 桌面宿主在无本地 HTTP 管理端口前提下的生命周期责任，以及 timeline 对请求来源分组摘要的读取要求。
- `capability-aware-mcp`: 增加 timeline / 审计视图所需的请求来源归因语义，使 token label、principal 摘要、未认证请求指纹 / user-agent 摘要能够以安全投影形式进入本地受信任管理面。
- `credential-and-approval-control`: 增加跨平台桌面管理面中的 vault 解锁、token 列表与 revoke、以及长期 token 签发前本地用户验证的产品入口要求。
- `quality-and-test-automation`: 增加跨平台桌面控制台在引导页、主工作台、timeline 分组、设置 / vault / token 管理等核心路径上的 UI 合同与自动化覆盖要求。

## 影响

- 受影响的宿主与前端边界包括未来的 Tauri Shell、bundled webview 页面结构、托盘 / 通知 / 窗口恢复、首次启动流程，以及 UI 与本地 control-plane 的桥接方式。
- 受影响的 Rust 代码包括 `bridgingio-app-api`、`bridgingio-core` 的本地管理接口、timeline / request attribution 的安全投影、以及 vault / token 管理相关 control-plane 命令。
- 受影响的设计资产包括新的跨平台桌面控制台设计系统；该设计系统需要成为后续 macOS 重构的参考，而不是继续以当前 `macos-operator-console` 视觉为唯一来源。
- 需要补充跨平台桌面 UI 测试与设计验收，覆盖首次选择目录、主工作台导航、timeline 分组、设置保存、vault 解锁、token 列表与 revoke 等关键路径。
