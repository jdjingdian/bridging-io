## 为什么

BridgingIO 旨在为 AI 模型提供一层统一、安全、低 token 成本的开发环境桥接能力，使其能够访问远端主机、调试设备、仓库和评审系统，而不局限于本地终端。现在定义基础架构，可以在 macOS 上先交付可用 MVP，同时避免未来扩展到 Linux、Windows、鸿蒙 PC 时被 UI 或单一工具实现绑死。

## 变更内容

- 引入 Rust 核心与平台 UI 解耦的整体架构，明确 `Target`、`Session`、`Profile`、`Capability`、`Artifact`、`ApprovalRequest` 等核心对象。
- 定义统一的连接与能力注册模型，支持以能力而非工具名对外暴露 SSH、ADB、后续 serial/docker 等连接器。
- 引入内容寻址的 Artifact 缓存机制，用于保存命令输出、日志流及其二次过滤结果，降低重复执行和 token 消耗。
- 定义面向 MCP 的 typed tools 与 resources 暴露方式，并保留受控的 raw command 兜底能力。
- 规划凭据保险库与高风险操作审批模型，覆盖 SSH/ADB 密钥、仓库认证以及 `sudo` 等特权操作。
- 定义多 agent 访问时的会话隔离规则，并区分逻辑会话与底层传输连接，支持同一逻辑会话内打开多个并发终端或连接通道。
- 定义 SwiftUI macOS 控制台的首版交互方向，包括连接管理、会话视图、可折叠命令时间线与审批反馈。
- 为 Rust 核心与平台 UI 建立自动化测试要求，明确核心功能必须附带单元测试，平台 UI 必须附带 UI Test。
- 为变更归档后的交付收尾建立规范，要求每次 archive 后都基于 `.gitmessage` 输出合适的 git commit message 和 pull request 描述。
- 明确 MVP 范围：优先支持 SSH、ADB、Git，以及围绕它们的终端、环境探测、Artifact 查看和审批流；将 serial、docker、HTTP 调试、OpenGrok 和更多 code review 平台放在后续阶段。

## 功能 (Capabilities)

### 新增功能
- `target-session-management`: 管理目标连接、会话生命周期、环境指纹探测与能力发现，为 SSH、ADB 和后续连接器提供统一抽象。
- `artifact-management`: 缓存命令输出和日志流，支持基于 artifact 标识的重新过滤、摘要和续读。
- `credential-and-approval-control`: 管理凭据引用、保险库存储与高风险操作审批，防止模型直接接触敏感信息。
- `capability-aware-mcp`: 以结构化能力、typed tools 和 resources 向 MCP 客户端暴露可发现、可审计的桥接接口。
- `macos-operator-console`: 提供 macOS SwiftUI 控制台，用于展示目标、会话、命令事件、日志摘要和审批状态。
- `quality-and-test-automation`: 为 Rust 核心和各平台 UI 定义自动化测试基线，确保功能实现伴随单元测试与 UI Test。
- `change-archive-handoff`: 为每次变更归档后的 git 提交与 PR 交接提供标准化输出，确保符合仓库提交约定。

### 修改功能

无。

## 影响

- 新建 Rust 核心模块边界，包括 domain、engine、connectors、providers、artifacts、policy、secrets、mcp adapter 等。
- 新建 SwiftUI macOS 应用层与 Rust 核心之间的本地 API/IPC 边界。
- 需要引入本地状态与缓存存储机制，用于 profile、session metadata、artifact index 和 environment fingerprint。
- 需要引入访问作用域和会话复用策略，用于区分多 agent、多轮运行以及同一逻辑会话下的多个连接通道。
- 需要与系统密钥库集成，而不是在应用内部直接保存敏感明文。
- 需要为 MCP 工具设计统一的输入输出模型、错误模型、审批回调与资源寻址规则。
- 需要为 Rust 核心、连接器、provider 和平台 UI 建立自动化测试框架与关键流程测试样例。
- 需要在变更归档流程中纳入 `.gitmessage` 约定，统一 commit message 和 PR 描述的生成方式。
