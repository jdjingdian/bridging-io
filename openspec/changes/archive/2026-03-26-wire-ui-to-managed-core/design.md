## 上下文

当前仓库已经具备三块基础能力，但它们还没有组合成真实产品运行态：

- SwiftUI macOS UI 已经完成了主要页面骨架，但运行时数据仍来自 `WorkspaceViewModel` 内部 seed，而不是真实 core 状态。
- Rust core 已经具备 standalone 运行、control-plane IPC 与 model-plane HTTP/MCP 的基础边界。
- `bridgingio-app-api` 已经定义了 UI 与 core 之间的 request/response 与 event 模型方向，但控制台实际所需的数据面、启动握手与事件节流还未落地。

这带来几个直接问题：

- UI 展示的 target、timeline、artifact、approval、transcript 并不可信，用户无法把它当成真实操作台。
- bundled 发行物中，core 与 UI 的生命周期、启动顺序和 readiness 语义还没有被产品化定义。
- 现有 core 会在启动时立刻同时开放 control-plane 与 model-plane，这与“bundled 模式需先连上 UI，再开放 MCP”这一决策不一致。
- 未来如果要让用户选择后台持续运行，就必须把“core 业务运行时”和“core 宿主/启动方式”提前拆开，否则后续接入系统服务托管会与当前 UI 托管模式互相缠绕。

本次设计需要同时满足两类约束：

- **当前 MVP 约束**：UI 与 core 强绑定，UI 退出时 core 跟随退出；不要求周期性 heartbeat；产品默认不是后台常驻服务。
- **未来扩展约束**：后续允许切换到系统服务型宿主，例如 SwiftUI 通过 `SMAppService` 或等价机制让 core 在后台持续运行，但不应重做 control-plane、MCP 或数据模型边界。
- **多平台前端约束**：后续前端可能来自 SwiftUI、Web UI、Flutter、Tauri 或其他原生技术栈；同一个 core 运行实例在任意时刻最多只服务一个 UI 前端 attach。

未来的多平台形态推荐理解为：

- `core` 继续持有运行时真相；
- 每个平台发行物提供一个本地 `Host Adapter` 负责拉起/关闭 core、完成 attach，并把本地 control-plane 转成该前端技术栈易于消费的桥接接口；
- 共享 Web UI 可以作为多平台快速落地的表现层，但不要求在每个阶段都与 SwiftUI 或其他原生 UI 保持完整功能对等。

推荐的当前拓扑如下：

```text
┌──────────────────────────────────────────────┐
│              BridgingIO.app                  │
│  SwiftUI shell + CoreHost(client side)       │
└──────────────────────┬───────────────────────┘
                       │ spawn / attach
                       ▼
┌──────────────────────────────────────────────┐
│          bridgingio-core (managed)           │
│                                              │
│  Phase 1: control-plane ready                │
│  Phase 2: UI attach succeeds                 │
│  Phase 3: model-plane MCP becomes available  │
└───────────────┬──────────────────────────────┘
                │
                ▼
      external MCP clients (after ready)
```

## 目标 / 非目标

**目标：**
- 将 bundled 发行物中的默认形态定义为“UI 托管的独立 core 进程”，而不是 UI 内嵌 Rust 逻辑。
- 让 macOS 控制台从运行时 seed 迁移到 core 真实状态，并为无数据、启动中、连接失败等状态提供正式语义。
- 把 core 启动过程拆成“control-plane 可用”与“model-plane 可用”两个阶段，使 UI attach 成为 bundled 模式下的 ready 条件。
- 在不引入周期性 heartbeat 的前提下，明确 UI 正常退出时的 core 终止语义。
- 为 app API 补足控制台启动、快照读取、交互式 shell 与增量刷新所需的最小契约。
- 在架构层预留未来系统服务型宿主模式，使后台持续运行成为宿主切换问题，而不是 runtime 重构问题。
- 在架构层预留共享 Web 前端与平台宿主适配器路径，使 Linux、Windows、鸿蒙等后续平台可以复用前端层而不推翻本地 control-plane 设计。

**非目标：**
- 本次不实现完整的后台持续运行设置 UI，也不直接接入 `SMAppService`、privileged helper 或其他系统服务注册流程。
- 本次不承诺 UI 崩溃、被强杀或系统异常退出时的绝对 orphan-free 回收语义；当前优先覆盖“产品默认路径上的正常启动与正常退出”。
- 本次不把 Rust core 改造成 Swift 进程内库，也不重写现有 MCP HTTP 总体能力模型。
- 本次不试图一次性完成所有控制台所需 API 的终态设计；只覆盖真实连接 core 所需的最小集合。
- 本次不实现共享 Web UI，也不要求未来 Web UI 与 SwiftUI 在每个阶段都保持完整功能等价。

## 决策

### 决策 1：当前产品默认采用“UI 托管的独立 core 进程”

Bundled 模式下，SwiftUI app 必须以宿主身份拉起 `bridgingio-core` 独立进程，并通过本地 control-plane IPC 与其通信。UI 不直接内嵌 Rust 核心逻辑，也不通过 MCP HTTP 回环访问 core。

选择该方案的原因：

- 与现有 `bridgingio-core`、control-plane IPC 和 model-plane HTTP 的代码形态一致。
- 可以保持 core 作为单独分发物继续支持 standalone 模式。
- 让 UI crash、MCP 访问与 core 运行时隔离开，避免把所有生命周期和线程模型塞进同一进程。
- 即使未来切换到系统服务托管，也只需要替换宿主层，不必重做 core 协议边界。

备选方案：

- **Rust 以 lib 形式嵌入 SwiftUI App**：FFI、线程、崩溃隔离和后续 service 化成本都更高。
- **一开始就做系统服务常驻**：会把当前 MVP 的安装、权限和运维复杂度提前引入。

### 决策 2：把“core 宿主方式”抽象成 Launch Backend，而不是写死为 UI 子进程

设计上将宿主/启动方式从 core runtime 中拆开。当前至少保留以下宿主模式概念：

- `ui-managed-ephemeral`：当前默认模式，由 UI 拉起并托管，短生命周期。
- `standalone-run`：前台运行，日志输出到终端。
- `standalone-detached`：后台/脱离式运行，对应 `-d`。
- `system-service`：预留未来由系统服务框架托管的模式。

core runtime 本身只关心配置、目标、session、artifact、approval、control-plane 与 model-plane，不关心是由谁把它拉起来。

选择该方案的原因：

- 允许当前 MVP 用最简单的 UI 子进程托管方式交付。
- 未来引入 `SMAppService` 或等价系统服务时，不需要修改 app API、MCP、artifact 或 session 语义。
- 宿主层可以独立决定日志策略、pid 管理、重启策略和后台持续运行设置。

备选方案：

- **把 UI 托管逻辑写死到 core CLI**：短期简单，但之后切换宿主会牵连 CLI、配置与 ready 语义。
- **把 service 模式现在就做出来**：超出本次范围，会拖慢“先连上真实 core”的主线。

### 决策 2.5：为多平台前端引入 Platform Host Adapter，而不是让所有前端直接消费底层 IPC 细节

未来如果引入共享 Web UI、Flutter、Tauri 或其他跨平台前端，不要求这些前端直接处理 Unix domain socket、Windows named pipe 或当前 line codec。推荐由每个平台发行物提供一个 `Platform Host Adapter`：

- 它负责拉起/关闭 core；
- 它负责 enforce “单 core 仅允许一个 UI attach”；
- 它负责与本地 control-plane 通信；
- 它负责把 control-plane 语义桥接为前端技术栈更适合的接口，例如原生命令桥、桌面壳事件、WebView bridge 或等价机制。

这样保留了“control-plane 默认是本地 IPC”的部署和信任边界，同时避免把某个具体 IPC transport 或 wire format 直接暴露给每一种 UI 技术栈。

选择该方案的原因：

- 让 SwiftUI、原生桌面 UI 与后续共享 Web UI 都能复用同一个 core，而不强迫所有前端直接理解底层 socket 细节。
- 让 `control-plane` 保持 transport-agnostic，后续若确有需要再增加浏览器友好桥接或其他 transport adapter，也属于增量扩展。
- 更符合“单 UI attach、强生命周期绑定”的产品约束，因为 attach 所有权由平台宿主控制，而不是下放给浏览器前端。

备选方案：

- **让所有前端直接连接底层 IPC**：对原生前端可行，但会让 Web UI、Flutter、Tauri 等技术栈的接入成本和平台差异上升。
- **把 control-plane 直接改成 gRPC 作为唯一方案**：在当前单 UI、同机、本地受信任的约束下收益有限，却会引入额外 RPC 栈与部署复杂度。

### 决策 3：core 启动采用“两阶段 ready”，bundled 模式必须先 attach UI 再开放 MCP

bundled 模式下，core 启动顺序定义为：

1. 读取配置并初始化 runtime。
2. 开放本地 control-plane。
3. 等待 UI 完成 `AttachUi` 握手。
4. attach 成功后，才允许 `/mcp` 和等价 model-plane 工具入口对外提供服务。

这里的“提供服务”指真正接受 MCP 调用，而不是单纯 bind 端口。实现上可以是：

- 提前绑定 HTTP listener，但在 attach 前对 `/mcp` 返回明确 `not_ready` / `ui_not_attached` 语义。
- 或在 attach 成功后再真正开放 `/mcp`。

本次设计要求对外行为等价：**在 attach 前，外部 MCP 客户端不得把 bundled core 当成 ready 的模型平面使用。**

推荐保留一个显式 readiness 状态，例如：

- `starting`
- `waiting_for_ui_attach`
- `ready`
- `shutting_down`

选择该方案的原因：

- 与“UI 是当前 bundled 形态下的主控面”这一产品决策一致。
- 避免外部 MCP 客户端在 UI 尚未接手前就开始写入或读取半初始化状态。
- 为以后切换到后台持久服务留下清晰的模式分叉：system-service 模式可直接进入 `ready`，bundled 模式必须先 attach。

备选方案：

- **core 启动后立即开放 MCP**：实现最简单，但与当前决策冲突。
- **必须绑定一个长期 heartbeat 才允许 ready**：能更强地检测 UI 存活，但超出当前需要。

### 决策 4：当前阶段不引入周期性 heartbeat，正常退出通过宿主显式关停 core

当前 MVP 不要求 app-layer 周期性 heartbeat。UI 与 core 的强绑定语义按以下方式实现：

- UI 启动时拉起 core，并在 attach 成功后进入正常控制台工作流。
- UI 正常退出时，宿主必须显式向 core 发出 shutdown，或直接终止其托管的 core 进程。
- attach 只作为启动阶段的握手条件，不额外要求周期性保活消息。

这意味着当前阶段优先保证：

- 正常启动路径正确
- 正常退出路径正确
- MCP 在 attach 前不可用

对于 UI 崩溃、被强杀或系统异常退出导致的 orphan 进程问题，本次只要求记录为设计风险并预留未来增强位，不把 heartbeat 作为当前 MVP 的前置条件。

选择该方案的原因：

- 满足当前用户决策，避免把 liveness 协议复杂度过早引入。
- 让实现聚焦在“接通真实 core”与“定义 ready 顺序”。
- 后续如果真的需要更强的 crash 绑定，可在宿主层补充 parent-pid 监控、持久 attach channel 或系统服务策略，而不必改动控制台数据模型。

备选方案：

- **control-plane 持久 attach 流 + EOF 退出**：更强，但需要额外的长连接管理。
- **周期性 heartbeat**：能解决 crash 检测，但当前被明确排除。

### 决策 5：UI 运行时数据改为“快照 + 事件 + 节流”，seed 仅保留给测试与 Preview

macOS 控制台必须以 core 为运行时真相源。产品运行时不再默认构造伪 target、伪 timeline、伪 artifact、伪 approval 或伪 transcript。

控制台数据流建议拆成三类：

- **启动快照**：targets、sessions、approvals、settings、diagnostics、最近 timeline、当前 channel/transcript 摘要。
- **按需读取**：artifact 详情、timeline 分页、channel transcript 分页、hash lookup。
- **事件通知**：session 状态变化、approval 状态变化、timeline 追加、transcript 有新增输出、settings 变更完成。

高频输出场景下，UI 刷新采用“事件通知 + 增量拉取 + 节流/合批”：

- approval / session state：立即更新。
- timeline 追加：短时间窗口内合并。
- transcript 输出：按行数、字节数或 50-100ms 窗口批量刷新。

测试与 Preview 需要的 seed/fixture 继续保留，但必须通过显式注入的 fixture provider 使用，禁止作为产品默认启动路径。

选择该方案的原因：

- 当前 SwiftUI 模型和 Rust domain shape 并不完全一致，需要一层映射和状态适配。
- 控制台需要真实状态，但不适合对每次输出都做全量对象重建。
- 可以把“真实运行时”和“测试/设计态数据”彻底分开。

备选方案：

- **纯轮询全量快照**：实现简单，但高频输出下性能和 UI 稳定性较差。
- **继续把 seed 作为默认数据源，后续再替换**：会继续模糊产品真实状态边界。

### 决策 6：app API 需要补足控制台最小可用面，而不是只停留在通用 request/response 骨架

现有 `bridgingio-app-api` 已定义基础命令和事件方向，但要支撑真实控制台，还需要最小补齐以下语义：

- 启动握手：`AttachUi` / `GetBootstrapState` 或等价组合
- 控制台快照：targets、sessions、approvals、settings、diagnostics
- timeline 访问：列表、分页或按 target/session 过滤
- artifact 访问：按 id/hash 读取、派生关系、片段读取
- interactive shell：open / write / read / interrupt / close
- 事件订阅：至少覆盖 session、approval、timeline、transcript、settings 应用结果

这不要求一次性把 app API 设计成最终形态，但必须让 UI 不再依赖虚构状态即可运行。

选择该方案的原因：

- 现有命令集不足以表达控制台全部真实视图。
- 与其继续在 ViewModel 里堆本地伪状态，不如尽快让 control-plane 成为唯一可信入口。
- 只要 app API 语义稳定，SwiftUI 与未来共享 Web UI 就可以基于各自宿主适配器以不同桥接方式消费同一套真实状态。

备选方案：

- **UI 直接读 runtime 内部状态文件或 HTTP debug 端点**：会绕过正式契约，后续难维护。

### 决策 7：共享 Web UI 以“更快覆盖核心能力”为目标，不要求与原生 UI 阶段性全量对齐

未来如果引入共享 Web UI，其首要目标是帮助 Linux、Windows、鸿蒙等平台更快落地核心能力，而不是在首个阶段就复制 SwiftUI 的完整体验。原生 UI 仍可继续承担更高质量或更具平台特性的交互体验。

这意味着未来的前端策略允许分层：

- `shared web frontend` 优先覆盖 targets、sessions、timeline、artifacts、approvals、settings 等通用能力；
- `native frontend` 可以继续提供更强的平台融合、动画、菜单栏/托盘、系统服务配置与其他平台特性；
- 两者只需共享 core 真相源与 app API 语义，不要求页面结构和交互细节完全一致。

选择该方案的原因：

- 能更现实地平衡“多平台快速落地”和“原生体验质量”这两个目标。
- 避免未来把 Web UI 变成另一条必须追赶 SwiftUI 全量功能的高压并行路线。

备选方案：

- **要求 Web UI 与原生 UI 功能严格同步**：会显著增加跨平台落地成本，并拖慢快速交付。
- **完全放弃原生 UI，只保留 Web UI**：不符合保留平台差异化体验的产品方向。

## 风险 / 权衡

- **[不引入 heartbeat 可能导致异常退出后 orphan core]** → 当前只保证正常退出路径；在 design 中预留未来由宿主层补充 parent-pid 监控、持久 attach channel 或系统服务模式。
- **[UI 与 Rust domain 模型不完全同构]** → 在控制台层引入明确的 mapping/adapter，而不是让 SwiftUI 直接依赖 Rust 原始对象形状。
- **[bundled 模式的 MCP readiness gating 会改变当前“启动即监听”的行为]** → 用显式 readiness 语义和测试覆盖说明该变化，避免客户端误判回归。
- **[移除默认 seed 会让 UI 在早期更频繁落入空态/启动态]** → 把 loading、empty、attach failed 视为正式产品状态，并纳入 UI Test，而不是继续依赖假数据掩盖空状态。
- **[预留 system-service 模式会引入少量宿主抽象成本]** → 只抽象启动后端和 ownership mode，不提前实现完整后台常驻功能，保持当前实现路径简单。

## 迁移计划

1. 在 core 宿主层引入运行模式与 ownership mode 语义，明确 `run`、`-d` 与 `ui-managed-ephemeral` 的行为边界。
2. 为 control-plane 增加 UI attach / bootstrap / console snapshot 所需的最小 app API。
3. 在 bundled 模式下为 `/mcp` 引入 attach 后 ready 的 gating 语义。
4. 将 SwiftUI app 的默认数据源切换为 control-plane，补充 loading/empty/error 状态。
5. 将现有运行时 seed 移出产品默认路径，迁移为测试与 Preview 专用 fixture。
6. 更新单元测试、UI Test 和集成测试，使其覆盖 UI 托管启动、attach 成功、attach 前 MCP 不可用、正常退出 core 跟随退出等路径。

## 开放问题

- bundled 模式下，`/mcp` 在 attach 前是“端口已监听但返回 not-ready”，还是“attach 前不绑定 model-plane 入口”，需要在实现阶段基于宿主复杂度二选一。
- 当前阶段是否需要限制单个 bundled core 只允许一个 UI 实例 attach，还是允许同一用户会话内的重连/替换 attach，需要在实现前明确。
- 未来如果引入后台持续运行设置，UI 应把它表达为“宿主模式切换”还是“保持 core 运行”的更高层产品语言，需要在服务化设计阶段再确定。
