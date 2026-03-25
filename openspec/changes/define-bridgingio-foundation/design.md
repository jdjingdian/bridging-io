## 上下文

BridgingIO 当前还是一个空白项目，但产品方向已经比较清晰：它不是单纯把若干命令行工具塞进 MCP，而是要为 AI 模型提供一层统一、安全、低 token 成本的开发环境桥接层。这个桥接层需要同时服务两类入口：

- MCP 客户端，用于结构化调用连接、执行、查询与缓存能力
- 本地桌面 UI，用于展示目标、会话、命令事件、日志、审批与凭据状态

该项目有几个天然约束：

- 首发仅做 macOS UI，后续还要考虑 Linux、Windows、鸿蒙 PC
- 核心能力需要尽量跨平台，因此不能把业务逻辑绑定在 SwiftUI 内
- 目标连接和命令执行会产生长文本、流式日志和敏感信息
- SSH、ADB、Git、后续 Gerrit/Serial/Docker/HTTP/Search 等能力需要统一抽象，但不能为了“通用”而把接口做成一个无法治理的万能 `run_command`

因此，这次设计的重点是先定义稳定的核心对象和边界，让首版 MVP 能围绕 SSH、ADB、Git 工作，同时保留后续扩展空间。

```text
┌──────────────────────────────────────────────────────┐
│                  SwiftUI macOS App                  │
│  Targets / Sessions / Timeline / Approvals / Vault  │
└──────────────────────────────┬───────────────────────┘
                               │ local app API
┌──────────────────────────────▼───────────────────────┐
│                    Rust Core Engine                  │
│  Session Manager | Capability Registry | Policy      │
│  Artifact Store  | Secret Abstraction | Providers    │
└───────────────┬───────────────────────────────┬──────┘
                │                               │
      ┌─────────▼─────────┐           ┌─────────▼─────────┐
      │    Connectors     │           │     Providers      │
      │ SSH / ADB / ...   │           │ terminal / git /   │
      │ transport access  │           │ review / http ...  │
      └─────────┬─────────┘           └─────────┬─────────┘
                │                               │
                └──────────────┬────────────────┘
                               ▼
                         MCP Facade
```

## 目标 / 非目标

**目标：**

- 用 Rust 定义可复用的核心引擎，避免平台 UI 持有业务真相
- 用统一的 `Target` / `Session` / `Capability` 模型覆盖 SSH、ADB，并为后续连接器保留扩展位
- 用内容寻址的 Artifact 机制处理长输出、流式日志和二次过滤
- 用结构化的 MCP tools/resources 暴露能力，优先 typed tools，保留受控兜底
- 用凭据句柄和审批流隔离敏感信息与高风险操作
- 在 macOS SwiftUI 中提供可观察、可审计、可审批的操作台体验

**非目标：**

- 第一阶段同时交付 Linux、Windows 或鸿蒙 PC UI
- 第一阶段支持所有候选工具与平台，包括 Serial、Docker、HTTP 调试、OpenGrok、Gerrit
- 第一阶段构建完整的插件市场或热插拔扩展系统
- 在应用内部自行实现新的密钥管理系统或长期保存明文密码
- 把 BridgingIO 做成一个完全替代原生终端、Git GUI 或设备管理工具的通用工作台

## 决策

### 决策 1：以 Rust 核心引擎为中心，UI 与 MCP 都作为适配层

BridgingIO 将以 Rust 作为核心实现语言，承载 domain、session manager、artifact store、policy、connectors 和 providers。SwiftUI macOS UI 与 MCP 暴露面都不直接实现业务逻辑，而是通过统一的 app API 使用 Rust 核心。

选择该方案的原因：

- Rust 便于复用跨平台核心逻辑，适合长生命周期会话、流式 I/O 与并发
- UI 与 MCP 共用同一套对象模型和审计/审批能力，避免双份业务逻辑
- 后续接入其他平台 UI 时，不需要重新实现连接与缓存核心

备选方案：

- 全部用 Swift 编写：macOS 开发效率高，但跨平台价值低
- UI 直接驱动系统 CLI：实现快，但难以统一治理缓存、审批和审计
- 每个平台各自实现核心逻辑：长期维护成本过高

### 决策 2：拆分 Connector 与 Provider，而不是按工具名堆模块

系统将连接能力拆为两层：

- `Connector` 负责“如何连通”，例如 SSH、ADB、Serial、Docker exec
- `Provider` 负责“连上之后能做什么”，例如 terminal、filesystem、git、review、http、search

这样 `git` 不会被硬绑定到 SSH，`review` 也不会被硬绑定到 Git。一个目标可以通过同一连接器挂载多个 provider，一个 provider 也可以在多个连接器上复用。

备选方案：

- 以工具名为中心：`ssh-tool`、`adb-tool`、`git-tool` 各自管理连接和能力，早期简单，但后续会造成能力重复和策略碎片化
- 单一命令路由器：所有请求最后都变成字符串命令，缺少 typed contract 与治理能力

### 决策 3：以能力为中心定义核心对象模型

核心对象将至少包括：

- `Target`：可访问目标及其 profile
- `Session`：一次活跃连接及其生命周期状态
- `Capability`：当前目标/会话可提供的结构化能力
- `Profile`：连接、仓库、默认策略、模板等配置
- `CredentialRef`：指向保险库的凭据引用
- `Artifact`：命令输出、日志流、diff、搜索结果及其派生视图
- `ApprovalRequest`：需要人工确认的高风险操作
- `EnvironmentFingerprint`：系统、工具、shell、权限等探测摘要

AI 客户端将围绕“能力”发现和调用接口，而不是围绕“这台机器装了哪些二进制”进行猜测。

备选方案：

- 只暴露 shell 能力：灵活但不利于安全控制与高成功率调用
- 只做少量固定高层接口：太僵硬，无法覆盖调试与运维场景

### 决策 4：使用元数据索引 + 内容寻址 Artifact 存储

Artifact 存储分为两部分：

- 元数据索引：保存 artifact id、来源、父子关系、过滤条件、时间戳、摘要和访问统计
- 原始与派生内容：保存长文本、流式 chunks、日志快照和二次过滤结果

推荐实现：

- SQLite 保存 profile、session、artifact metadata、environment fingerprint 和审计事件
- 文件系统分块保存较大的 artifact 内容和流式输出片段

这样既能避免把大块日志全部塞进数据库，也能让 artifact 具备稳定标识和后续重放/重过滤能力。

备选方案：

- 全量只存内存：实现简单，但无法复用缓存或恢复会话上下文
- 全量只存 SQLite：单文件部署方便，但对长日志和大对象扩展性较差

### 决策 4.5：会话采用 Access Scope + Logical Session + Transport Session + Channel 分层

为了支持多个 agent 并发访问同一目标、同一 agent 多次恢复到同一工作上下文、以及同一会话内打开多个终端或日志通道，BridgingIO 需要把“会话”从单层概念拆成四层：

- `AccessScope`：描述本次访问属于哪个 workspace、principal、agent、run、thread 或客户端会话
- `LogicalSession`：面向 AI 与用户的工作会话，承载 timeline、artifacts、approvals、notes 和会话级策略
- `TransportSession`：实际的 SSH/ADB 连接实例，可断开、重连，不等同于逻辑会话
- `Channel`：逻辑会话内部的具体交互通道，例如一个交互式终端、一个日志流、一个只读命令执行通道

默认隔离规则：

- 不同 `agent_id` 访问同一 target，必须创建不同的 `LogicalSession`
- 同一 `agent_id + target_id + client_session_id` 在复用策略允许时，可以恢复到同一个 `LogicalSession`
- `TransportSession` 默认不跨 agent 共享
- timeline、artifact、approval、命令上下文默认按 `LogicalSession` 隔离
- 只读缓存，例如目标 profile、工具路径解析、环境指纹摘要，可按 target 维度共享，但必须带 TTL 或重新探测策略

这样可以避免把“同一目标”错误地当成“同一会话”，也避免因为底层连接断开而丢失逻辑会话上下文。

同一逻辑会话内允许同时存在多个 `Channel`。例如：

- `channel-a`: 交互式 SSH 终端，用于执行命令
- `channel-b`: 同一主机上的日志 tail 通道，用于持续收集输出

它们共享同一个 `LogicalSession`，因此 AI 和用户能看到统一的会话时间线；但每个 channel 仍应有独立的通道标识、状态和执行上下文。

备选方案：

- 只保留单一 `Session`：实现简单，但无法可靠表达并发访问和多终端场景
- 仅按 target 复用会话：会导致不同 agent、不同运行之间的数据污染
- 把每个终端都建模成完全独立的会话：隔离过强，不利于在一次工作会话内统一管理 artifacts 和 approvals

### 决策 5：优先 typed tools，保留受控 raw command 兜底

MCP 暴露面将分两层：

- 高层 typed tools/resources，例如 `targets.list`、`sessions.open`、`terminal.exec`、`artifacts.read`、`artifacts.refine`、`git.status`、`git.diff`
- 低层受控兜底，例如 `terminal.exec_raw`

高层接口提供更稳定的参数模型、错误模型和策略校验。低层接口只在高层能力不足时使用，并且必须纳入审批、审计和输出缓存。

备选方案：

- 只提供万能 `run_command`：最灵活，但会让模型更容易犯错，也更难治理
- 只提供固定 typed tools：长期会被边缘场景卡住

### 决策 6：过滤分为 source filter 与 post filter，并统一产出 Artifact

对于长日志和超大命令输出，系统将支持两种过滤路径：

- `source filter`：在源头减少输出，例如远端执行时附加 grep、时间窗口或分页参数
- `post filter`：对已缓存 artifact 做本地关键字、正则、范围过滤

无论来源如何，系统都必须保留原始 artifact 与派生 artifact 的引用关系。这样模型可以先拿摘要，再用 artifact id 请求更宽或更窄的过滤结果，而不必重复触发远端命令。

备选方案：

- 只在远端过滤：节省带宽，但后续调整过滤条件时需要重跑命令
- 只在本地过滤：对首次大输出不友好，可能浪费网络和 token

### 决策 7：敏感信息和高风险操作走显式审批模型

凭据不直接暴露给模型。Profile 与 Session 只保存 `CredentialRef`，实际凭据由系统密钥库存储。对于写操作、删除操作、特权操作、敏感资源读取等行为，系统将由 policy engine 判定是否需要审批。

对于 `sudo` 类场景：

- 先探测是否支持 `sudo -n`
- 如果可无密码执行，则仍记录审批与审计结果
- 如果需要交互式凭据，则在 UI 中发起审批，由本地安全组件处理输入
- 模型只能获得“批准/拒绝/失败”结果，不能看到明文密码

备选方案：

- 让模型直接读写凭据：风险过高
- 完全禁止特权操作：会削弱远端调试和运维价值

### 决策 8：macOS UI 采用“目标列表 + 会话详情 + 命令时间线”的信息架构

首版 SwiftUI UI 不追求完整终端仿真器，而优先提供：

- 目标列表与 profile 管理
- 会话详情、环境指纹和能力摘要
- 可折叠命令卡片时间线
- stdout/stderr/exit status/approval 事件的结构化呈现
- Artifact 摘要、过滤历史与重读入口

这样用户既能看见 AI 做了什么，也不会被大量原始终端输出淹没；必要时仍可展开查看原始内容。

在开始 SwiftUI 编码前，必须先使用 `ui-ux-pro-max` 生成可评审的 macOS 控制台设计系统，并与项目负责人确认方向。确认后的设计源文件将持久化到 `design/macos-console/design-system/MASTER.md`，页面级差异放在 `design/macos-console/design-system/pages/` 中，作为后续可编辑的设计基线。当前基于技能的初步检索结果显示，`JetBrains Mono + IBM Plex Sans` 的开发者工具字体组合和深色中性底色配绿色强调较适合 BridgingIO，但面向官网的 `Feature-Rich Showcase` 或 `Enterprise Gateway` 页面模式不能直接照搬到操作台，后续设计规范需要将其收敛成更适合高信息密度的桌面控制台布局。

备选方案：

- 纯终端视图：真实但信息噪声大，审批和 artifact 关系不清晰
- 只展示高层摘要：透明度不足，难以诊断问题

### 决策 9：MVP 先聚焦 SSH、ADB、Git 与 macOS

第一阶段范围限定为：

- 连接器：SSH、ADB
- Provider：terminal、git
- 核心：session、artifact、policy、vault abstraction、environment fingerprint
- UI：SwiftUI macOS
- MCP：围绕目标、会话、终端执行、artifact 与 git 查询的接口

以下内容明确放在后续阶段：

- Serial、Docker、HTTP 调试、OpenGrok、Gerrit
- 复杂插件系统
- 多平台 UI 适配

这个范围可以先验证产品核心价值，再决定后续扩展优先级。

### 决策 10：功能交付必须附带自动化测试

BridgingIO 的代码交付标准不只是“功能可用”，还必须包含与实现层次匹配的自动化测试：

- Rust 核心、domain、policy、artifact、connectors、providers、MCP adapter 必须附带单元测试；对跨模块流程再补充集成测试
- macOS SwiftUI UI 必须附带覆盖关键用户流的 UI Test
- 后续 Linux、Windows、鸿蒙 PC 等平台 UI 接入时，也必须为各自平台提供等价的 UI Test，而不是只复用手工测试说明

关键用户流至少包括：

- 目标创建或选择
- 会话建立与状态展示
- 命令时间线浏览
- artifact 查看与重新过滤
- 审批请求展示与处理

这样做的原因是：

- BridgingIO 涉及远端连接、长输出缓存和审批策略，回归风险高
- UI 既承担透明度，也承担安全确认入口，不能只靠手工点测
- 后续多平台扩展时，先定义统一测试门槛，能避免某个平台长期无测试积压

备选方案：

- 仅依赖手工测试：前期看似快，但后续回归成本高
- 只测 Rust 核心不测 UI：会遗漏审批流、时间线和 artifact 交互这类关键可见行为

### 决策 11：每次变更 archive 后都必须输出标准化交付文案

每次 OpenSpec 变更完成并执行 archive 后，交付流程都必须额外产出两类文本：

- 一个参考仓库根目录 `.gitmessage` 模板生成的 git commit message
- 一个与该变更范围匹配的 pull request 描述

其中 commit message 必须满足 `.gitmessage` 中定义的 Conventional Commits 约束：

- 使用 `<type>(<scope>): <subject>` 结构
- 使用祈使句
- subject 首字母不大写，末尾不加句号
- subject 不超过 72 个字符

PR 描述至少应覆盖：

- 本次变更做了什么
- 为什么需要这项变更
- 关键实现或设计点
- 测试与验证情况
- 如有必要，列出风险、后续工作或 breaking change

这样可以让 archive 不只意味着“规范已收尾”，还意味着“已经准备好进入实际提交和评审流程”。

备选方案：

- 仅输出 commit message 不输出 PR 描述：仍然会把交接成本留给人工
- 每次由人工自由发挥：风格容易漂移，也难以持续符合 `.gitmessage` 约定

## 风险 / 权衡

- 本地 app API 与跨进程协作会增加集成复杂度 → 通过稳定的请求/响应协议和明确的 domain model 降低耦合
- 依赖系统 CLI 或外部工具时会遇到版本差异 → 提供可执行路径解析、健康检查和工具来源回显
- Artifact 缓存会持续增长 → 提供容量配额、TTL、手动清理和基于引用关系的回收策略
- 能力抽象可能掩盖底层工具细节 → 提供 capability 摘要与受控 raw command 兜底
- 首版只做 macOS UI，后续跨平台时可能暴露新的交互差异 → 把 UI 的共享状态机和 app API 提前稳定下来
- 多平台 UI Test 将带来测试栈差异 → 提前定义“关键用户流”的统一测试契约，再由各平台选择本地测试框架实现
- archive 后的交付文案如果缺少上下文容易流于模板化 → 生成时强制参考本次 change 的 proposal、design、tasks、测试结果和 `.gitmessage`
- 会话分层与多通道支持会增加状态机复杂度 → 先明确 `LogicalSession`、`TransportSession` 和 `Channel` 的职责边界，再决定默认复用策略

## Migration Plan

1. 初始化仓库结构，落定 Rust 核心模块、SwiftUI macOS 目录和 OpenSpec 文档基线
2. 实现核心 domain model、session manager、policy engine、artifact metadata 和本地存储层
3. 为 session manager 增加 access scope、logical session、transport session 和 channel 的建模与复用策略
4. 接入 SSH 与 ADB 连接器，以及 terminal / git provider 的 MVP 能力
5. 使用 `ui-ux-pro-max` 生成 BridgingIO macOS 控制台设计系统，与项目负责人确认后持久化到 `design/macos-console/design-system/`
6. 基于已确认的设计系统实现 macOS 控制台，覆盖目标管理、会话视图、命令时间线、artifact 浏览和审批反馈
7. 为 Rust 核心补齐单元测试和关键集成测试，并为 macOS 控制台补齐关键用户流 UI Test
8. 在每次 change archive 后，基于 `.gitmessage` 生成建议的 git commit message 与 pull request 描述
9. 增加 MCP adapter，让外部 AI 客户端可通过同一核心引擎访问能力
10. 在 MVP 稳定后，再引入 review、serial、docker、http、search 等 provider/connector

由于项目尚未发布，当前不需要复杂的线上迁移或回滚方案；如果某项能力不稳定，可通过 feature flag 或实验性入口限制暴露范围。

## Open Questions

- SwiftUI 与 Rust 核心的本地 app API 在 MVP 中采用哪种具体协议最合适，例如 Unix domain socket、gRPC 还是更轻量的 JSON-RPC
- Git provider 在 MVP 中是优先调用系统 `git`，还是引入库级实现以减少环境差异
- Review provider 何时接入，以及 Gerrit 的变更查询与 patch 视图应否独立成 capability
- Serial 和 Docker 是否只作为新 connector 接入，还是需要在 provider 层增加专有能力
- artifact 过滤语法在 v1 中是否只支持 Rust regex 语义，还是需要兼容更接近 PCRE 的表达方式
- 后续多平台 UI 是否需要先定义一份跨平台关键用户流测试清单，以便 macOS 之外的平台直接继承
- PR 描述是否需要在仓库中再落一份固定模板，还是保持由 archive 流程按变更内容动态生成
- `client_session_id`、`run_id` 和 `reuse_policy` 应由 MCP 客户端显式传入，还是允许桌面 UI 在本地自动生成并托管
