## 为什么

BridgingIO 旨在为 AI 模型提供一层统一、安全、低 token 成本的开发环境桥接能力，使其能够访问远端主机、调试设备、仓库和评审系统，而不局限于本地终端。现在定义基础架构，可以在 macOS 上先交付可用 MVP，同时避免未来扩展到 Linux、Windows、鸿蒙 PC 时被 UI 或单一工具实现绑死。

## 变更内容

- 引入 Rust 核心与平台 UI 解耦的整体架构，明确 `Target`、`Session`、`Profile`、`Capability`、`Artifact`、`ApprovalRequest` 等核心对象。
- 定义统一的连接与能力注册模型，支持以能力而非工具名对外暴露 SSH、ADB、后续 serial/docker 等连接器。
- 引入内容寻址的 Artifact 缓存机制，用于保存命令输出、日志流及其二次过滤结果，降低重复执行和 token 消耗。
- 将 Artifact 的后续重读取、重过滤和重分析提升为独立于 SSH/ADB 的一级核心能力，使其可复用于未来所有能产出文本内容的 target，而不是仅作为终端执行的附属能力。
- 为 artifact cache 增加可配置后端，至少支持 `memory` 与 `filesystem` 两种模式；在 `filesystem` 模式下，artifact 必须在 core 重启后继续支持读取与重分析。
- 为 artifact cache 增加容量治理能力，允许通过配置与 UI 设置 artifact 持久化根目录、最大缓存限制与淘汰策略，而不是任由缓存无限增长。
- 将 artifact 标识升级为稳定的 hash-like object id，并额外保留内容摘要指纹，使 UI 与 MCP 可以基于 artifact hash 反查来源命令、所属会话、父子派生关系与执行上下文。
- 定义面向 MCP 的 typed tools 与 resources 暴露方式，并保留受控的 raw command 兜底能力。
- 为 MCP 能力暴露补充“简述 + 详细说明”双层描述契约，要求 `tools/list` 返回可供模型快速理解的英文简述，并允许客户端按 capability/tool 标识继续获取更详细的英文使用说明。
- 约束后续所有新暴露的 MCP capability/tool 在进入规范与实现时，都必须同步提供英文 short description 与 detailed description，而不是在实现阶段临时补文案。
- 为 MCP 兼容性补充最小 resources 能力面（`resources/list`、`resources/templates/list`、`resources/read`），避免仅支持 tools 的实现在部分客户端中出现能力探测失败。
- 为 capability/tool 详情读取补充标识归一化规则，允许客户端使用下划线、点号或等价别名查询，避免因命名风格差异导致误判“能力不存在”。
- 为 model-plane 增加可开关的 MCP 请求/响应 trace 诊断能力（默认关闭），用于排查客户端兼容、协议调用顺序和错误回包问题。
- 明确 MCP JSON-RPC 入口的传输约束：`/mcp` 只接收 `POST` JSON-RPC 调用；服务端在错误回包时也必须回显原请求 `id`，避免客户端在解析错误时丢失请求关联。
- 定义 core 的双访问平面：本地 UI/control plane 通过受信任的 app API 本地 IPC 访问 core，AI/model plane 通过共享的 MCP HTTP 入口访问同一份 core 状态。
- 规划凭据保险库与高风险操作审批模型，覆盖 SSH/ADB 密钥、仓库认证以及 `sudo` 等特权操作，并为未来内置 vault 后端保留兼容抽象。
- 定义多 agent 访问时的会话隔离规则，并区分逻辑会话与底层传输连接，支持同一逻辑会话内打开多个并发终端或连接通道。
- 定义 SwiftUI macOS 控制台的首版交互方向，包括连接管理、目标设置、会话视图、可折叠命令时间线与审批反馈。
- 定义 standalone core 的运行方向，使其既可由 macOS UI 驱动，也可在前台进程或 daemon 模式下读取配置文件独立运行。
- 为 standalone core 定义版本化配置文件结构与最小可运行样例，使 UI 开发前即可基于 core 独立验证目标装载、监听配置和基础连接能力。
- 定义连接器工具依赖的分发策略，优先以 core 管理的内置后备二进制补足 SSH/ADB 等外部依赖，而不是把依赖绑定在 UI 进程内。
- 为 Rust 核心与平台 UI 建立自动化测试要求，明确核心功能必须附带单元测试，平台 UI 必须附带 UI Test。
- 为变更归档后的交付收尾建立规范，要求每次 archive 后都基于 `.gitmessage` 输出合适的 git commit message 和 pull request 描述。
- 明确 MVP 范围：优先支持 SSH、ADB、Git，以及围绕它们的终端、环境探测、Artifact 查看和审批流；将 serial、docker、HTTP 调试、OpenGrok 和更多 code review 平台放在后续阶段。

## 功能 (Capabilities)

### 新增功能
- `target-session-management`: 管理目标连接、会话生命周期、环境指纹探测、standalone 配置装载、工具来源解析与能力发现，为 SSH、ADB 和后续连接器提供统一抽象。
- `artifact-management`: 缓存命令输出和日志流，支持基于稳定 artifact 标识的持久化、重新过滤、重分析、摘要、续读与反查。
- `credential-and-approval-control`: 管理凭据引用、保险库存储、未来 vault 后端兼容与高风险操作审批，防止模型直接接触敏感信息。
- `capability-aware-mcp`: 以结构化能力、typed tools 和 resources 向 MCP 客户端暴露可发现、可审计的桥接接口。
- `macos-operator-console`: 提供 macOS SwiftUI 控制台，用于展示和编辑目标配置、查看会话、命令事件、日志摘要和审批状态。
- `quality-and-test-automation`: 为 Rust 核心和各平台 UI 定义自动化测试基线，确保功能实现伴随单元测试与 UI Test。
- `change-archive-handoff`: 为每次变更归档后的 git 提交与 PR 交接提供标准化输出，确保符合仓库提交约定。

### 修改功能

无。

## 影响

- 新建 Rust 核心模块边界，包括 domain、engine、connectors、providers、artifacts、policy、secrets、mcp adapter 等。
- 新建 SwiftUI macOS 应用层与 Rust 核心之间的本地 API/IPC 边界。
- 需要定义本地 control plane 与 MCP model plane 的职责边界，避免 UI 管理接口与模型能力接口混用。
- 需要引入本地状态与缓存存储机制，用于 profile、session metadata、artifact index 和 environment fingerprint。
- 需要定义 core-owned settings/config 边界，使 UI、CLI 与 standalone mode 都复用同一套目标配置与工具解析模型。
- 需要引入访问作用域和会话复用策略，用于区分多 agent、多轮运行以及同一逻辑会话下的多个连接通道。
- 需要与系统密钥库集成，而不是在应用内部直接保存敏感明文。
- 需要为配置文件中的非敏感设置、保险库中的敏感凭据和运行期元数据存储定义清晰边界。
- 需要为 standalone core 提供一份规范化、可版本迁移的配置 schema 和参考样例，作为 UI 之前的独立验证基线。
- 需要把 artifact cache backend、持久化根目录、最大缓存限制与淘汰策略纳入 core-owned settings/config 与 UI 设置模型。
- 需要为 MCP 工具设计统一的输入输出模型、错误模型、审批回调与资源寻址规则。
- 需要为 artifact 定义稳定的 hash-like 标识与 `content_digest` 模型，替代进程内自增序号，并支撑基于 hash 的跨重启查询与追溯。
- 需要为 MCP 的 capability / tool 元数据设计统一的简述与详细说明模型，并明确它们的英文文案要求与读取方式。
- 需要为新增 MCP capability/tool 建立统一的英文文案交付约束与登记源，确保后续扩展能力时不会遗漏 short/detail 两层说明。
- 需要为 resources 与 tools 的双通道能力发现定义兼容基线，确保客户端先探测 resources 再调用 tools 时也能稳定工作。
- 需要为 capability/tool 标识查找定义归一化策略，容忍常见别名写法并保持回包语义一致。
- 需要为 MCP 运行期问题提供轻量可开关的请求日志与错误定位信息，同时避免默认输出过多噪声或敏感原文。
- 需要定义 MCP JSON-RPC 入口与错误回包的一致性约束，至少覆盖“`/mcp` 只接收 `POST`”和“错误响应必须保留请求 `id`”。
- 需要把“由源侧自行过滤”与“由 BridgingIO 代理处理 artifact 文本”的选择显式纳入 typed tool 输入，而不是完全依赖提示词约定。
- 需要为 MCP HTTP 入口定义默认监听配置与安全约束，至少覆盖 `127.0.0.1:19718`、host/port 可配置，以及非 loopback 监听的显式启用与认证要求。
- 需要为目标设置、凭据选择、连接器工具覆盖和诊断结果补充 app API / UI 侧接口。
- 需要为 Rust 核心、连接器、provider 和平台 UI 建立自动化测试框架与关键流程测试样例。
- 需要在变更归档流程中纳入 `.gitmessage` 约定，统一 commit message 和 PR 描述的生成方式。
