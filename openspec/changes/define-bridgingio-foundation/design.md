## 上下文

BridgingIO 当前还是一个空白项目，但产品方向已经比较清晰：它不是单纯把若干命令行工具塞进 MCP，而是要为 AI 模型提供一层统一、安全、低 token 成本的开发环境桥接层。这个桥接层需要同时服务两类入口：

- MCP 客户端，用于结构化调用连接、执行、查询与缓存能力
- 本地桌面 UI，用于展示和配置目标、会话、命令事件、日志、审批与凭据状态

该项目有几个天然约束：

- 首发仅做 macOS UI，后续还要考虑 Linux、Windows、鸿蒙 PC
- 核心能力需要尽量跨平台，因此不能把业务逻辑绑定在 SwiftUI 内
- 目标连接和命令执行会产生长文本、流式日志和敏感信息
- SSH、ADB、Git、后续 Gerrit/Serial/Docker/HTTP/Search 等能力需要统一抽象，但不能为了“通用”而把接口做成一个无法治理的万能 `run_command`
- 首发产品会以“一个平台发行物 + 一个本地 UI + 一个 core”的完整应用形态交付，而不是让多个不同 UI 同时共享同一个前端壳

因此，这次设计的重点是先定义稳定的核心对象和边界，让首版 MVP 能围绕 SSH、ADB、Git 工作，同时保留后续扩展空间。

```text
┌──────────────────────────────────────────────────────┐
│                  SwiftUI macOS App                  │
│ Targets / Sessions / Timeline / Approvals / Vault / │
│                    Settings                          │
└──────────────────────────────┬───────────────────────┘
                               │ local control plane
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
                  MCP HTTP Model Plane Facade
```

## 目标 / 非目标

**目标：**

- 用 Rust 定义可复用的核心引擎，避免平台 UI 持有业务真相
- 用统一的 `Target` / `Session` / `Capability` 模型覆盖 SSH、ADB，并为后续连接器保留扩展位
- 用内容寻址的 Artifact 机制处理长输出、流式日志和二次过滤
- 用结构化的 MCP tools/resources 暴露能力，优先 typed tools，保留受控兜底
- 用凭据句柄和审批流隔离敏感信息与高风险操作
- 在 macOS SwiftUI 中提供可观察、可审计、可审批、可配置的操作台体验

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
- `Profile`：连接、凭据引用、别名/备注、工具覆盖、默认策略、模板等配置
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

- 元数据索引：保存 artifact id、content digest、来源命令、父子关系、过滤条件、时间戳、摘要和访问统计
- 原始与派生内容：保存长文本、流式 chunks、日志快照和二次过滤结果

推荐实现：

- SQLite 保存 profile、session、artifact metadata、environment fingerprint 和审计事件
- 文件系统分块保存较大的 artifact 内容和流式输出片段
- 在 core 内部定义 `ArtifactStore` 抽象，至少提供 `memory` 与 `filesystem` 两种 backend

这样既能避免把大块日志全部塞进数据库，也能让 artifact 具备稳定标识和后续重放/重过滤能力。

备选方案：

- 全量只存内存：实现简单，但无法复用缓存或恢复会话上下文
- 全量只存 SQLite：单文件部署方便，但对长日志和大对象扩展性较差

### 决策 4.1：Artifact 标识采用稳定 hash-like object id，而不是进程内自增序号

一旦 artifact 支持跨重启持久化，进程内自增序号就不再可靠。因此，BridgingIO 不应继续使用类似 `artifact-000001` 这种启动后重置的本地计数，而应采用类似 git object / commit 的稳定 hash-like 标识。

推荐模型如下：

- `artifact_id`：基于规范化 artifact manifest 计算出的稳定对象标识，作为对外主引用
- `content_digest`：基于 artifact 原始文本内容或 chunk 集合计算出的内容摘要，用于完整性校验与潜在去重

`artifact_id` 推荐纳入以下信息后再哈希：

- artifact kind
- logical session / channel / target 等 provenance 信息
- parent artifact 引用
- source command 或 source descriptor
- filter / processing mode 等派生条件
- `content_digest`

这样做的原因：

- 可以在 core 重启后稳定引用同一个 artifact
- UI 与 MCP 可以基于 artifact hash 反查来源命令、会话、派生链路和执行上下文
- 可以避免“相同内容但不同来源”的 artifact 被错误折叠成同一对象

显示层可以借鉴 git 的使用体验：内部保存 full hash，UI 可以显示短 hash，但任何短 hash 都必须在当前存储中唯一后才能被接受为查询前缀。

### 决策 4.2：Artifact cache 由可配置 backend 驱动，并受容量上限治理

Artifact cache 不应只有“是否缓存”的粗粒度开关，而应被视为 core 的一项正式存储能力。MVP 至少支持：

- `memory` backend：仅进程内缓存，适合轻量临时使用，core 重启后 artifact 丢失
- `filesystem` backend：将 artifact 元数据和文本内容持久化到本地存储，支持 core 重启后的继续读取与重分析

对于 `filesystem` backend，必须同时支持容量治理：

- 配置 artifact root
- 配置最大缓存限制，例如 `max_bytes`
- 配置淘汰策略，MVP 推荐 `lru`

当缓存接近上限时，系统应优先淘汰冷数据，而不是静默写爆磁盘。正在被活跃 channel 持有、仍在写入、或刚被读取/重分析的 artifact，应优先视为受保护对象，避免立刻被回收。

这意味着 artifact persistence 不等于 session persistence。换句话说：

- 持久化 artifact 在 core 重启后仍应可读、可 refine
- 活跃连接、前台 shell 状态和 transport session 是否恢复，是另一套独立语义，不能因为 artifact 持久化而被隐式承诺

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

### 决策 4.6：standalone 模式由 Core 持有配置真相，并同时服务 UI 与配置文件

考虑到短期内产品只推进 macOS UI，但核心能力需要支持前台二进制和 daemon 两种 standalone 运行形态，BridgingIO 必须把“设置”也纳入 Rust Core，而不是由 UI 私有维护一份配置模型。

推荐边界如下：

- Core 维护统一的 settings/profile 模型，并负责校验、持久化和对外暴露 typed settings API
- standalone 模式允许 core 在启动时读取操作员提供的配置文件，以完成 headless 启动和默认目标装载
- UI、CLI、后续其他平台前端通过 app API 调用同一套 settings/profile 接口，而不是各自定义配置语义
- 非敏感设置可来自配置文件或 core 管理的持久层；敏感凭据始终通过 `CredentialRef` 指向 vault backend
- MVP 为 standalone core 定义版本化的 TOML 配置文件结构与参考样例，作为 UI 开发前验证 core 基本功能的规范入口

MVP 配置文件的推荐顶层结构如下：

- `schema_version`：配置 schema 版本号，用于兼容后续迁移
- `[core]`：实例名、数据目录、日志级别等全局运行设置
- `[storage]`：元数据数据库等 core 持久化位置
- `[storage.artifacts]`：artifact cache backend、artifact root、最大缓存限制与淘汰策略
- `[vault]`：保险库后端类型与命名空间，只保存后端配置，不保存敏感明文
- `[control_plane]`：本地受信任控制面的启停和 IPC 端点策略，MVP 语义上固定为本地 IPC
- `[model_plane.http]`：MCP HTTP 的监听 host、port 与非 loopback 暴露控制
- `[model_plane.http.auth]`：模型平面的认证约束与未来 token / mTLS 等扩展位
- `[toolchains.<name>]`：外部工具路径覆盖、是否优先使用内置后备等连接器依赖策略
- `[policies.defaults]`：默认复用策略、审批模式、环境探测开关等策略默认值
- `[[targets]]`：目标 profile 列表，包括 SSH/ADB 连接参数、凭据引用、别名、备注与 provider 配置

配置文件只承载“操作员声明式配置”，不承载运行时真相。也就是说，session 时间线、artifact 索引、审批状态、环境探测缓存等运行期数据必须进入 core 的状态存储，而不是回写进 standalone 配置文件。

推荐参考样例如下：

```toml
schema_version = 1

[core]
instance_name = "bridgingio-local"
data_dir = "~/.bridgingio"
log_level = "info"

[storage]
metadata_backend = "sqlite"
metadata_path = "~/.bridgingio/state/metadata.sqlite3"

[storage.artifacts]
backend = "filesystem"
root = "~/.bridgingio/artifacts"
max_bytes = 1073741824
eviction_policy = "lru"

[vault]
backend = "os-native"
namespace = "io.bridgingio"

[control_plane]
enabled = true
transport = "platform-ipc"
endpoint = "auto"

[model_plane.http]
enabled = true
host = "127.0.0.1"
port = 19718
allow_non_loopback = false

[model_plane.http.auth]
mode = "none"
required_when_non_loopback = true

[toolchains.ssh]
path_override = ""
prefer_builtin_fallback = false

[toolchains.adb]
path_override = "/opt/homebrew/bin/adb"
prefer_builtin_fallback = true

[policies.defaults]
reuse_policy = "resume_or_create"
approval_mode = "on-risk"
capture_env_fingerprint = true

[[targets]]
id = "lab-ssh-01"
display_name = "Lab Ubuntu"
kind = "ssh"
enabled = true
aliases = ["lab", "ubuntu-main"]
credential_ref = "vault://bridgingio/ssh/lab"
notes = "Primary lab server"

[targets.connection]
host = "10.0.0.18"
port = 22
username = "debug"
known_hosts_policy = "system-default"

[targets.providers.terminal]
enabled = true
shell = "/bin/bash"

[[targets.providers.git.repositories]]
id = "bridgingio"
path = "/srv/repos/bridgingio"
remote_name = "origin"
web_url = "https://gerrit.example.com/plugins/gitiles/bridgingio"

[targets.providers.git.repositories.review]
kind = "gerrit"
base_url = "https://gerrit.example.com"
project = "bridgingio"

[[targets]]
id = "android-emulator"
display_name = "Android Emulator"
kind = "adb"
enabled = true
aliases = ["emu", "pixel-test"]
credential_ref = "vault://bridgingio/adb/default"

[targets.connection]
selector_kind = "serial"
selector_value = "emulator-5554"

[targets.providers.terminal]
enabled = true
shell = "/system/bin/sh"
```

这样可以同时满足：

- 没有 UI 时，用户仍然可以靠配置文件启动 core
- 有 UI 时，设置修改仍然经过 core 的统一模型和校验链路
- 未来切换 IPC 方案或扩展到其他桌面平台时，不需要重定义设置语义
- 后续在 UI 开发前，可以直接拿参考样例验证 standalone core 的目标装载、监听配置和基础连接能力
- artifact cache 的持久化 backend、容量限制和落盘位置可以在 standalone 模式下独立验证，而不依赖 UI 先实现

不推荐的方案：

- 仅让 UI 保存设置，再把结果“喂给” core：会导致 headless 模式缺失能力，也会让其他前端重复实现设置逻辑
- 仅让 core 读取配置文件，但不提供设置 API：会把 UI 降级成只读审计台，无法承载目标管理和凭据选择

### 决策 4.7：外部工具优先以内置后备二进制接入，并由 Core 统一解析来源

对于 SSH、ADB 等连接器依赖，BridgingIO 优先集成“可执行二进制后备”，而不是把第三方库直接静态或动态链接进 UI 进程。

推荐策略：

- 逻辑所有权属于 core 的 connector/tool resolver
- 运行时按“用户覆盖路径 → 系统 PATH → 内置后备二进制”顺序决策
- macOS UI 分发包可以物理携带这些后备二进制，但它们在语义上属于 core 运行时资源，而不是 UI 专属资源
- 诊断接口必须能回显当前选中的工具路径与来源，方便 UI 展示和问题排查

选择该方案的原因：

- 当前 connector 抽象天然围绕可执行文件解析，而不是围绕库级 API
- 可执行后备更利于不同分发形态复用，例如 `.app`、独立 daemon 包和未来其他平台发行物
- 能保留用户覆盖系统工具路径的能力，避免把产品锁死在单一内置版本

备选方案：

- 把依赖库直接打进 UI：会让 standalone core 与其他平台前端失去复用价值
- 只依赖系统安装：部署更轻，但会降低首次成功率和 AI 可用性

### 决策 4.8：拆分本地控制平面与模型平面，UI 走本地 IPC，AI 走 MCP HTTP

BridgingIO Core 在 MVP 中采用双访问平面，而不是让 UI 与 AI 共享同一条传输协议：

- `Control Plane`：面向受信任的本地操作员前端，例如 macOS SwiftUI、未来 Linux/Windows/鸿蒙 PC 前端，以及本地 CLI。该平面复用 `bridgingio-app-api` 语义，但默认通过本地 IPC 接入 core，而不是通过对外 HTTP 监听。
- `Model Plane`：面向 AI/MCP 客户端。该平面通过共享的 MCP HTTP 入口暴露能力，确保多个 AI 客户端访问的是同一份 core 状态真相，而不是各自拉起独立进程维护隔离状态。

推荐默认值如下：

- MCP HTTP 默认监听 `127.0.0.1:19718`
- 允许操作员显式配置 host 和 port
- 允许切换到非 loopback 地址，例如 `0.0.0.0`，但必须作为显式高级选项开启
- 非 loopback 监听必须联动认证与安全告警，而不是静默暴露高权限桥接能力

选择该方案的原因：

- 当前产品发行形态是“一个平台 UI + 一个本地 core”，UI 与 core 更像本地控制面，不需要为了统一而强行走网络协议
- AI 客户端需要共享唯一的 core 状态真相，因此使用单一 HTTP 模型平面比把 stdio 作为主入口更容易避免多进程状态分叉
- `bridgingio-app-api` 已经保持 transport-agnostic，因此本地 IPC 不会破坏未来在其上增加 gRPC facade 的可能性
- 将 UI/control plane 与 AI/model plane 分开后，更容易在权限、安全审计和接口治理上形成清晰边界

MVP 不选择 gRPC 作为默认 control-plane 传输，原因是当前并不存在“一个 core 同时服务多个不同本地 UI”的需求。对于按平台整包发布的形态，本地 IPC 的实现复杂度更低，更贴合受信任单前端对接的实际场景。

MVP 也不把 stdio 作为 core 的主入口。如果未来需要兼容只支持 stdio 的宿主，stdio 只能作为无状态适配层，将请求转发到同一个 core，而不能成为另一份状态真相。

### 决策 5：优先 typed tools，保留受控 raw command 兜底

MCP 暴露面将分两层：

- 高层 typed tools/resources，例如 `targets.list`、`sessions.open`、`terminal.exec`、`artifacts.read`、`artifacts.refine`、`git.status`、`git.diff`
- 低层受控兜底，例如 `terminal.exec_raw`

高层接口提供更稳定的参数模型、错误模型和策略校验。低层接口只在高层能力不足时使用，并且必须纳入审批、审计和输出缓存。

备选方案：

- 只提供万能 `run_command`：最灵活，但会让模型更容易犯错，也更难治理
- 只提供固定 typed tools：长期会被边缘场景卡住

### 决策 5.5：终端型 Target 同时暴露单次执行与持久交互两种模式

对于 SSH、ADB 这类“终端型 target”，BridgingIO 不应只暴露单一的 `terminal.exec` 语义，而是必须显式支持两种交互模式：

- `one-shot exec`：类似 `ssh host whoami` 或 `adb shell whoami` 的单次命令执行
- `interactive shell`：类似先进入 `adb shell` 或 `ssh` 终端，再在同一 shell 中连续执行多条命令

两种模式各自适合不同问题：

- `one-shot exec` 适合短平快、上下文隔离的查询或操作，易审计、易缓存、易控制 token 成本
- `interactive shell` 适合依赖 shell 状态延续的任务，例如 `cd`、`export`、定义函数、观察 prompt、逐步排障、保持前台程序或按步骤查看中间结果

这两种模式都应被建模为 `LogicalSession` 内部的 `Channel`，但它们的状态语义必须不同：

- `one-shot exec` 默认不保留 shell 进程上下文，也不继承其他 channel 的当前目录、环境变量或前台程序状态
- `interactive shell` 必须保留 channel 级的 shell 状态，包括当前工作目录、环境变量、prompt、前台/后台任务和输入输出流
- 同一 `LogicalSession` 下如果存在多个 `interactive shell` channel，它们彼此默认隔离；一个 channel 中的 `export` 或 `cd` 不应隐式污染另一个 channel
- `interactive shell` 在运行期应优先绑定 PTY，以支持 `top`、`stty` 等依赖 TTY 的命令；若平台或运行环境无法分配 PTY，必须降级到 pipe 并在 transcript 中回显降级提示

推荐的交互关系如下：

```text
LogicalSession
├─ channel-a (one-shot exec)
│  ├─ whoami
│  └─ uname -r
├─ channel-b (interactive shell)
│  ├─ export BUILD_MODE=debug
│  ├─ cd /data/local/tmp
│  └─ ./run-tests.sh
└─ channel-c (interactive shell)
   └─ top
```

MCP 能力发现必须把这两种模式都明确暴露给模型，而不是假设模型只能反复调用单次命令。模型应该能根据任务自行选择：

- 如果只是读取用户名、内核版本、仓库状态，优先选择 `one-shot exec`
- 如果任务需要保持上下文或逐步试探，优先打开 `interactive shell`

这也意味着终端 provider 不能只返回“执行结果”，还要为交互式 shell 提供会话句柄、流式 transcript、prompt 边界、stdin 写入、中断和关闭等控制面语义。

### 决策 6：过滤分为 source filter 与 post filter，并统一产出 Artifact

对于长日志和超大命令输出，系统将支持两种过滤路径：

- `source filter`：在源头减少输出，例如远端执行时附加 grep、时间窗口或分页参数
- `post filter`：对已缓存 artifact 做本地关键字、正则、范围过滤

无论来源如何，系统都必须保留原始 artifact 与派生 artifact 的引用关系。这样模型可以先拿摘要，再用 artifact id 请求更宽或更窄的过滤结果，而不必重复触发远端命令。

备选方案：

- 只在远端过滤：节省带宽，但后续调整过滤条件时需要重跑命令
- 只在本地过滤：对首次大输出不友好，可能浪费网络和 token

### 决策 6.5：Artifact 重分析是跨 Target 的一级能力，而不是终端附属能力

虽然当前 MVP 最先通过 SSH、ADB 和 terminal provider 暴露 artifact，但其本质并不是“终端日志过滤”，而是“对已缓存文本内容进行可重放、可派生、可多轮复用的再分析”。因此这项能力必须在模型和实现层都保持 target-agnostic：

- 只要某个 target 或 provider 产出了文本 artifact，就应默认能够复用同一套读取、过滤、重分析与派生 artifact 语义
- 终端只是首批 artifact 来源之一，而不是 artifact 重分析能力的宿主边界
- 后续接入 HTTP 调试、OpenGrok、Gerrit 或其他文本型 provider 时，不应重新发明一套“搜索结果过滤”或“响应体再分析”接口，而应直接复用 artifact 能力面

这意味着能力发现和文案表达也要跟着调整：模型不应只在 `terminal.exec` 的描述里“顺带”得知可以做 artifact refine，而应能够直接发现“artifact reanalysis”本身。

### 决策 6.6：文本处理归属使用显式模式参数，而不是布尔开关

用户提出“由 BridgingIO 代理处理数据”这个想法是对的，但从长期演进看，用布尔开关表达会过早锁死语义。因此更推荐把它建模为显式模式参数，例如：

- `source`：优先源侧过滤，尽量在 target/provider 端减少输出
- `bridgingio`：优先保留较完整原始文本，再由 BridgingIO 基于 artifact 执行重分析
- `auto`：由系统结合能力、成本和策略自动选择

选择这种建模方式的原因：

- 比布尔值更清楚，后续更容易扩展到 `hybrid`、`preview_then_bridgingio` 一类策略
- 对模型更友好，避免其把“是否代理处理”误解成单纯的 true/false 细节
- 便于在 artifact 元数据和审计记录里表达本次文本处理究竟由哪一侧完成

### 决策 6.7：能力描述分为简述与详述两层

当前 `tools/list` 已能返回 `name`、`title`、`description` 和 `inputSchema`，这足以承载第一层发现信息，但对复杂能力仍然不够。BridgingIO 应把能力说明拆成两层：

- `short description`：在 `tools/list` 中返回的英文简述，强调“做什么”“什么时候该用”
- `detailed description`：通过独立的 capability/tool 详情读取入口返回的英文详述，覆盖推荐模式、参数解释、常见例子、与其他能力的关系

推荐的关系如下：

```text
tools/list
├─ terminal.exec
│  └─ short description
├─ artifacts.refine
│  └─ short description
└─ capability/detail (by id or tool name)
   ├─ when to use
   ├─ parameter semantics
   ├─ examples
   └─ related capabilities
```

这样做的价值在于：

- 模型首轮决策时不会被超长文档淹没
- 对复杂能力，例如 artifact reanalysis、interactive shell、多种处理归属策略，模型仍能在需要时继续追问更详细说明
- 后续新增能力时，spec 可以强制要求同时产出 short 和 detailed 两套英文文案，而不是把描述质量留给实现者临时发挥

进一步说，这不应该只是“写文档时顺手补一下”，而应该成为能力注册契约的一部分：任何新暴露的 MCP capability/tool 都必须在 core 侧登记其英文 short description 与 detailed description，`tools/list` 与详情读取入口都应从同一份 source of truth 派生，避免名称、参数语义与示例在多个地方漂移。

### 决策 6.8：能力发现同时兼容 tools 与 resources 探测路径

虽然 BridgingIO 的核心执行面主要通过 typed tools 暴露，但在实际客户端生态中，很多 MCP 客户端会先调用 `resources/list` 或 `resources/templates/list` 再进入工具调用流程。为了避免客户端把“method not found”误判成服务异常，MVP 也应提供最小可用的 resources 能力面：

- `resources/list`：返回可发现的 capability/tool 文档资源入口
- `resources/templates/list`：返回可参数化模板，便于客户端按约定构造资源 URI
- `resources/read`：按资源 URI 读取详细说明，作为 `tools/list` 之外的第二条文档链路

这样做并不是把 resources 变成主执行接口，而是提供协议级兼容层，确保“先 resources 后 tools”的客户端也能稳定接入。

### 决策 6.9：能力详情查询必须支持标识归一化与别名容错

客户端在查询 capability/tool 详情时，常见输入并不总是标准点号形式（例如会传 `bridgingio_terminal_exec` 而不是 `bridgingio.terminal.exec`）。如果服务端只做严格字符串匹配，会导致模型误判“能力不存在”。

因此，BridgingIO 应在详情查询入口中支持标识归一化与别名容错，至少覆盖：

- 下划线与点号互转
- `bridgingio_` 与 `bridgingio.` 前缀互转
- 常见分隔符差异（例如 `/` 与 `.`）

同时返回的 canonical id 必须保持稳定，确保审计、日志和客户端缓存都指向统一标识。

### 决策 6.10：提供可开关 MCP trace 诊断，默认关闭

针对“本地 curl 正常但模型端失败”这类问题，单看客户端错误往往不足以定位。BridgingIO 需要提供轻量可开关的 model-plane trace：

- 默认关闭，不影响日常运行
- 显式开启后输出请求/响应摘要（method、tool、id、状态码、body 大小）
- 避免默认记录敏感正文，优先输出最小定位信息

建议通过环境变量（如 `BRIDGINGIO_MCP_TRACE`）控制，便于临时排障并与现有启动方式兼容。

### 决策 6.11：MCP JSON-RPC 入口与错误回包必须保持协议一致性

MVP 的 MCP HTTP 模型平面必须明确采用“`POST /mcp` + JSON-RPC”约束，避免客户端把健康检查或普通 GET 探测误认为可执行入口：

- `/mcp` 仅作为 JSON-RPC POST 入口
- `GET /mcp` 返回 `404 not found` 属于预期行为，不应被解释为服务不可用
- 健康检查与可执行入口分离（例如 `GET /health` 与 `POST /mcp`）

同时，错误回包必须保持请求关联一致性：

- 若请求携带了非空 `id`，服务端错误响应必须回显相同 `id`
- 不允许在错误路径中把请求 `id` 丢失为 `null`，否则客户端难以把错误绑定回原调用

这样可直接改善“本地 curl 正常、模型端解码失败”类问题的可定位性，并降低不同 MCP 客户端对错误回包解析差异带来的兼容风险。

### 决策 7：敏感信息和高风险操作走显式审批模型

凭据不直接暴露给模型。Profile 与 Session 只保存 `CredentialRef`，实际凭据由系统密钥库存储。对于写操作、删除操作、特权操作、敏感资源读取等行为，系统将由 policy engine 判定是否需要审批。

对于 `sudo` 类场景：

- 先探测是否支持 `sudo -n`
- 如果可无密码执行，则仍记录审批与审计结果
- 如果需要交互式凭据，则在 UI 中发起审批，由本地安全组件处理输入
- 模型只能获得“批准/拒绝/失败”结果，不能看到明文密码

为了兼容未来 roadmap 中的内置 vault，保险库访问必须继续通过可替换 backend abstraction 暴露。也就是说，profile、session 和配置文件应只认 `CredentialRef`，而不与具体的 macOS Keychain、文件型 vault 或未来自研 vault 实现耦合。

备选方案：

- 让模型直接读写凭据：风险过高
- 完全禁止特权操作：会削弱远端调试和运维价值

### 决策 8：macOS UI 采用“目标列表 + 会话详情 + 命令时间线”的信息架构

首版 SwiftUI UI 不追求完整终端仿真器，而优先提供：

- 目标列表与 profile 管理，包括 SSH/ADB 目标创建、编辑、别名/备注维护和凭据引用选择
- 会话详情、环境指纹和能力摘要
- 连接器/Provider 诊断与工具来源回显，包括用户覆盖路径、系统 PATH 命中和内置后备来源
- 可折叠命令卡片时间线
- 交互式 shell channel 的 transcript 视图与基本控制，例如输入、关闭和中断
- stdout/stderr/exit status/approval 事件的结构化呈现
- Artifact 摘要、过滤历史与重读入口
- Artifact cache 设置入口，包括 backend 选择、最大缓存限制、已使用空间、清理入口与“是否需要重启 core 生效”的反馈
- 在 artifact 详情或命令时间线中显示 artifact hash，并允许用户基于 hash 反查来源命令、派生关系与所属执行上下文

这样用户既能看见 AI 做了什么，也不会被大量原始终端输出淹没；必要时仍可展开查看原始内容。

换句话说，首版 UI 不应该被定义成“纯审计屏”。它首先是一个以可观测性为核心的操作台，但同时必须承担受控设置入口，至少覆盖目标连接参数、凭据引用、可读名称/别名和工具来源诊断。

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
- standalone 配置文件、UI 设置和运行时状态如果边界不清容易产生多份真相 → 由 core 持有统一 settings/profile 模型，并让配置文件与 UI 都接入同一套校验语义
- Artifact 缓存会持续增长 → 提供容量配额、TTL、手动清理和基于引用关系的回收策略
- 能力抽象可能掩盖底层工具细节 → 提供 capability 摘要与受控 raw command 兜底
- 首版只做 macOS UI，后续跨平台时可能暴露新的交互差异 → 把 UI 的共享状态机和 app API 提前稳定下来
- 多平台 UI Test 将带来测试栈差异 → 提前定义“关键用户流”的统一测试契约，再由各平台选择本地测试框架实现
- archive 后的交付文案如果缺少上下文容易流于模板化 → 生成时强制参考本次 change 的 proposal、design、tasks、测试结果和 `.gitmessage`
- 会话分层与多通道支持会增加状态机复杂度 → 先明确 `LogicalSession`、`TransportSession` 和 `Channel` 的职责边界，再决定默认复用策略

## Migration Plan

1. 初始化仓库结构，落定 Rust 核心模块、SwiftUI macOS 目录和 OpenSpec 文档基线
2. 实现核心 domain model、settings/profile 模型、policy engine、artifact metadata 和本地存储层
3. 为 standalone core 增加配置文件装载、vault backend abstraction 和 settings API 边界
4. 为 session manager 增加 access scope、logical session、transport session 和 channel 的建模与复用策略
5. 接入 SSH 与 ADB 连接器，以及 terminal / git provider 的 MVP 能力，同时落实用户覆盖路径、系统 PATH 和内置后备二进制的解析链路
6. 使用 `ui-ux-pro-max` 生成 BridgingIO macOS 控制台设计系统，与项目负责人确认后持久化到 `design/macos-console/design-system/`
7. 基于已确认的设计系统实现 macOS 控制台，覆盖目标管理、设置入口、会话视图、命令时间线、artifact 浏览和审批反馈
8. 为 Rust 核心补齐单元测试和关键集成测试，并为 macOS 控制台补齐关键用户流 UI Test
9. 在每次 change archive 后，基于 `.gitmessage` 生成建议的 git commit message 与 pull request 描述
10. 增加 MCP adapter，让外部 AI 客户端可通过同一核心引擎访问能力
11. 在 MVP 稳定后，再引入 review、serial、docker、http、search 等 provider/connector

由于项目尚未发布，当前不需要复杂的线上迁移或回滚方案；如果某项能力不稳定，可通过 feature flag 或实验性入口限制暴露范围。

## Open Questions

- control plane 的本地 IPC 在各平台的具体承载形式是否统一抽象为“平台本地 socket”，还是需要分别落到 Unix domain socket、named pipe 等实现细节
- 如果未来出现多个受信任本地前端同时对接一个 core，是否需要在现有 app API 语义之上补充 gRPC facade
- Git provider 在 MVP 中是优先调用系统 `git`，还是引入库级实现以减少环境差异
- Review provider 何时接入，以及 Gerrit 的变更查询与 patch 视图应否独立成 capability
- Serial 和 Docker 是否只作为新 connector 接入，还是需要在 provider 层增加专有能力
- artifact 过滤语法在 v1 中是否只支持 Rust regex 语义，还是需要兼容更接近 PCRE 的表达方式
- 后续多平台 UI 是否需要先定义一份跨平台关键用户流测试清单，以便 macOS 之外的平台直接继承
- PR 描述是否需要在仓库中再落一份固定模板，还是保持由 archive 流程按变更内容动态生成
- `client_session_id`、`run_id` 和 `reuse_policy` 应由 MCP 客户端显式传入，还是允许桌面 UI 在本地 control plane 中自动生成并托管
