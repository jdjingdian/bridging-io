## 为什么

当前 macOS UI 仍以运行时 seed 数据驱动，未连接到 Rust core，因此目标、时间线、artifact、审批和 transcript 都不是同一份真实状态。随着 standalone core、MCP HTTP 和本地 control-plane 已经形成基础边界，现在需要把 UI 与 core 接成一个真实可运行的组合形态，并明确它们的生命周期与启动顺序。

## 变更内容

- 移除 macOS UI 在产品运行时中的预置业务数据，默认改为连接 core 后展示真实状态；测试和 Preview 所需的 fixture 必须转为测试专用注入，而不是继续作为产品默认数据源。
- 将 macOS UI 与 Rust core 的当前产品形态定义为“强绑定生命周期”：UI 启动时托管并拉起 core，UI 正常退出时 core 必须跟随退出。
- 明确 bundled 模式下的启动顺序：core 先开放本地 control-plane，待 UI 完成 attach/握手后，才允许对外提供 MCP model-plane 能力。
- 为 core 增加清晰的运行模式语义，至少覆盖前台 `run` 与后台/脱离式 `-d`，以支持 standalone 分发；但在当前 bundled 模式下，UI 默认托管前台 core 进程，而不是依赖后台常驻模式。
- 为 UI 与 core 之间的 app API 增补面向真实控制台的数据契约，覆盖 targets、sessions、timeline、artifacts、approvals、settings、interactive shell 以及启动握手所需的最小状态面。
- 为高频 AI/终端活动下的 UI 刷新定义“事件通知 + 增量读取 + 节流”的方向，避免每次输出都触发全量 UI 刷新。
- 在架构层显式预留未来服务化宿主模式，使后续 SwiftUI 接入 `SMAppService` 或等价系统服务托管时，可以切换 core 的启动/归属方式，而不需要重做 runtime、control-plane 或 MCP 协议边界。
- 在架构层显式预留未来共享 Web 前端的发展路径：允许后续以 Web UI 作为多平台快速落地的共享表现层，由各平台宿主适配器负责连接本地 core、托管生命周期并向前端暴露浏览器友好的桥接接口。
- 明确共享 Web UI 不要求与 SwiftUI 或其他原生 UI 在每个阶段保持完整功能对等；Web UI 的主要目标是让多平台能力更快落地，而原生 UI 可以继续承担更高质量或更具平台特性的体验。

## 功能 (Capabilities)

### 新增功能

无。

### 修改功能
- `target-session-management`: 增加 UI 托管 core 生命周期、bundled/standalone 启动模式、attach 握手与 `run`/`-d` 运行语义。
- `capability-aware-mcp`: 增加 bundled 模式下 model-plane 的 attach 后可用约束，以及 readiness/错误语义。
- `macos-operator-console`: 要求控制台消费 core 真实数据而非运行时 seed，并展示 core 启动、连接和空态行为。

## 影响

- 受影响的 UI 代码包括 SwiftUI app 启动入口、`WorkspaceViewModel` 的数据来源、空态与测试 fixture 注入方式。
- 受影响的 Rust 代码包括 `bridgingio-app-api` 契约、`bridgingio-core` CLI/宿主逻辑、control-plane IPC、model-plane readiness gating 与事件/增量读取接口。
- 需要把 control-plane 继续保持为 transport-agnostic 边界，使 Unix socket、named pipe 及后续浏览器友好桥接方式都能作为宿主适配器层的可替换实现。
- 需要补充 bundled 模式和 standalone 模式的启动状态、退出行为和 MCP 可用性验证。
- 需要更新现有 UI 单元测试与 UI Test，使其不再依赖产品运行时 seed 数据。
