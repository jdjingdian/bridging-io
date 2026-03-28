# capability-aware-mcp 规范

## 目的
待定 - 由归档变更 define-bridgingio-foundation 创建。归档后请更新目的。
## 需求
### 需求:MCP 必须暴露结构化能力发现
MCP 接口必须允许客户端查询目标、会话和 provider 的结构化能力摘要。能力摘要必须描述可执行的操作类型、是否支持流式输出、是否支持文件传输、是否需要审批以及是否存在受控 raw command 兜底能力。

#### 场景:客户端查询目标能力
- **当** MCP 客户端请求查看某个目标或会话的能力
- **那么** 系统必须返回结构化能力列表，而不是只返回“支持 SSH”或“支持 ADB”这类过于粗糙的文本说明

### 需求:MCP 必须优先提供 typed tools 与 resources
系统必须为目标、会话、终端执行、artifact 读取与过滤、以及 Git 查询暴露结构化 tools 或 resources。调用方必须能够在不手工拼接 shell 字符串的情况下完成常见查询和执行流程。

#### 场景:客户端查询仓库 diff
- **当** MCP 客户端希望查看一个已配置仓库的变更差异
- **那么** 系统必须提供结构化的 Git 查询入口，并返回可继续阅读或缓存的结果，而不是要求客户端只能调用通用 shell 执行 `git diff`

### 需求:MCP 必须把 Artifact 重分析作为一级可发现能力暴露
MCP 能力模型必须把 artifact 的重读取、重过滤和重分析作为一级能力暴露，而不是仅把它们埋在终端执行能力的附属说明里。客户端必须能够在不依赖 SSH/ADB 语义的前提下发现这项能力，并理解其适用于所有会产出文本 artifact 的 target。

#### 场景:客户端查询文本分析相关能力
- **当** MCP 客户端希望判断 BridgingIO 是否支持“先采集原始文本，再基于 artifact 做二次过滤或重分析”
- **那么** 系统必须能够把该能力作为独立能力项返回，并说明它不局限于终端型 target

### 需求:MCP 能力发现必须暴露终端交互模式
对于 SSH、ADB 等终端型 target，MCP 能力发现必须不仅告诉客户端“可以执行终端命令”，还必须明确暴露该目标支持哪些终端交互模式，例如 `one-shot exec`、`interactive shell`，以及它们各自适合的使用场景。

#### 场景:客户端查询终端型 target 的能力
- **当** MCP 客户端请求查看某个 SSH 或 ADB target 的终端能力
- **那么** 系统必须返回该 target 支持的终端模式摘要，并提示哪些模式适合短命令查询，哪些模式适合需要保持 shell 上下文的多步任务

### 需求:MCP 必须允许客户端按模式选择单次执行或交互式会话
对于终端型 target，MCP 必须提供结构化的 typed tools，使客户端能够显式选择：

- 直接执行一次命令
- 打开一个交互式 shell 会话句柄
- 向该句柄写入输入、读取流式输出、请求中断或关闭会话

#### 场景:客户端需要保持 shell 上下文
- **当** MCP 客户端需要在同一个远端 shell 中连续执行 `cd`、`export`、再执行后续命令
- **那么** 系统必须允许其打开交互式 shell 句柄并在后续调用中持续引用该句柄，而不是要求客户端把所有步骤都压扁成一次性的 shell 字符串

#### 场景:客户端只需要执行一次读取型命令
- **当** MCP 客户端只需要读取用户名、内核版本或其他一次性信息
- **那么** 系统必须允许其通过单次执行模式完成任务，而不要求为此额外建立交互式 shell 会话

### 需求:`tools/list` 必须返回英文简述
系统通过 MCP `tools/list` 返回 tool 定义时，必须为每个对外暴露的 tool 或 capability 提供稳定的英文简述，至少覆盖它做什么、适合何种场景，以及关键输入语义。该简述必须足够短，使模型可以在首轮发现阶段快速决定是否调用。

#### 场景:模型首次浏览可用工具
- **当** 模型通过 `tools/list` 查看 BridgingIO 暴露的工具
- **那么** 返回结果必须包含可直接消费的英文简述，而不是只返回工具名称和参数 schema

### 需求:系统必须提供能力或工具的详细说明读取方式
除了 `tools/list` 中的简述，系统还必须提供按 capability id 或 tool name 读取更详细英文说明的机制。该详细说明至少应覆盖使用时机、参数语义、推荐模式、常见示例以及与其他能力的关系。

#### 场景:模型需要进一步理解某个能力
- **当** 模型已经从 `tools/list` 看到某个能力的简述，但仍需要更详细的使用建议
- **那么** 系统必须允许其通过约定的 tool 或 resource 读取该能力的详细英文说明，而不要求依赖外部文档或提示词注入

### 需求:MCP 必须兼容 resources 能力探测入口
系统必须支持最小可用的 resources 协议入口，至少覆盖 `resources/list`、`resources/templates/list` 和 `resources/read`。这些入口可以作为文档或元数据通道，但不得影响 typed tools 作为主要执行面。

#### 场景:客户端先调用 resources/list 再决定是否调用 tools
- **当** 某个 MCP 客户端在会话初期先探测 `resources/list` 或 `resources/templates/list`
- **那么** 系统必须返回结构化结果（可以为空数组），而不是直接返回 `method not found`

#### 场景:客户端通过资源 URI 读取能力详情
- **当** 客户端拿到 `bridgingio://capability/<id>` 或 `bridgingio://tool/<name>` 这类资源 URI
- **那么** 系统必须允许其通过 `resources/read` 获取对应详细说明，并与 `tools/list` 的简述语义保持一致

### 需求:能力详情查询必须支持标识归一化
系统在按 capability id 或 tool name 查询详细说明时，必须支持常见命名风格差异的归一化与别名容错，例如点号与下划线、`bridgingio.` 与 `bridgingio_` 前缀差异。系统不得因格式差异直接误判为“能力不存在”。

#### 场景:客户端使用下划线风格查询工具
- **当** 客户端请求 `bridgingio.capability.describe`，参数中传入 `bridgingio_terminal_exec`
- **那么** 系统必须能将其归一化并命中 canonical 工具标识 `bridgingio.terminal.exec`

#### 场景:服务端返回 canonical 标识
- **当** 系统成功命中某个 capability/tool 的别名查询
- **那么** 返回结果必须包含稳定的 canonical id，确保后续日志、缓存与审计引用一致

### 需求:MCP 诊断必须提供可开关请求追踪
系统必须提供可显式启用的 MCP 请求追踪能力，用于排查客户端兼容、协议调用顺序与错误回包问题。该能力默认关闭，并且默认输出应以摘要为主，避免直接暴露大段敏感正文。

#### 场景:操作员启用 MCP trace 调试
- **当** 操作员通过配置或环境变量显式启用 MCP trace
- **那么** 系统必须输出每次请求/响应的关键摘要信息（至少包含 method、请求标识、状态和响应大小）

#### 场景:操作员未启用 MCP trace
- **当** 操作员未启用该诊断开关
- **那么** 系统必须保持默认静默或低噪声日志行为，不得无条件输出高频请求明细

### 需求:新增 MCP 能力必须同步交付英文简述与详述
任何后续新增、修改或拆分出的 MCP capability/tool，只要它会被对外暴露给模型，就必须同步提供稳定的英文 short description 与 detailed description。系统不得允许只注册工具名和参数 schema 而缺失描述层。

#### 场景:后续新增一个新的 MCP 能力
- **当** 团队为 BridgingIO 增加新的 tool 或 capability，例如新的 artifact 分析能力、HTTP 调试能力或代码搜索能力
- **那么** 对应变更规范与最终实现都必须同时提供该能力的英文 short description、英文 detailed description，以及可被客户端稳定读取的 capability/tool 标识

#### 场景:能力文案与参数语义来自同一真相源
- **当** `tools/list` 返回某个能力的英文简述，同时客户端继续读取该能力的详细说明
- **那么** 两者必须来自同一份 capability/tool 元数据真相源，避免工具名称、参数语义或推荐用法在不同入口出现漂移

### 需求:Artifact 重分析工具必须显式暴露处理归属参数
Artifact 重分析相关 typed tools 必须允许客户端显式声明文本处理归属，例如“源侧过滤”、“BridgingIO 代理处理”或“自动选择”。该输入应当成为正式参数模型的一部分，而不是只体现在自然语言描述中。

#### 场景:客户端要求 BridgingIO 代理处理文本
- **当** MCP 客户端调用 artifact 重分析能力并明确要求由 BridgingIO 对已缓存文本进行处理
- **那么** 相关 tool 的参数 schema 必须能够表达这一偏好，并让服务端据此记录和执行相应策略

### 需求:MCP 必须通过共享的 HTTP 模型平面对外暴露
系统必须允许外部 AI/MCP 客户端通过同一个 MCP HTTP 入口访问共享的 core 状态，而不是要求每个客户端都通过独立 stdio 进程维护各自的状态真相。该 HTTP 入口在默认配置下必须监听 `127.0.0.1:19718`，并允许操作员显式修改 host 与 port。对于 bundled 的 UI-managed 模式，系统必须在受信任本地 UI 完成 attach 之前将 `/mcp` 视为未就绪；在 attach 完成之前，服务端必须返回明确的 not-ready 或等价 readiness 语义，或保持等价的不可用状态，而不得把该模型平面伪装成已可执行的正常入口。该 attach gating 不适用于显式 standalone 模式。

#### 场景:两个 MCP 客户端访问同一份 core 状态
- **当** 两个独立的 MCP 客户端连接到同一个 BridgingIO Core 的 MCP HTTP 入口
- **那么** 它们必须访问到同一份目标/profile 与 session 管理真相，并通过访问作用域与复用策略完成逻辑隔离，而不是因为每个客户端各自拉起独立进程而形成分叉状态

#### 场景:bundled 模式下 UI attach 前访问 `/mcp`
- **当** 外部 MCP 客户端在 bundled 的 UI-managed core 尚未完成本地 UI attach 时访问 `/mcp`
- **那么** 系统必须返回明确的未就绪语义或保持等价的不可用状态，而不是把该请求当成正常可执行的 MCP 调用受理

#### 场景:standalone 模式下 MCP 可直接提供服务
- **当** 操作员以显式 standalone 模式启动 core，且未要求 bundled UI attach gating
- **那么** 系统必须允许外部 MCP 客户端在该实例完成自身启动后直接访问共享的 model-plane HTTP 入口

### 需求:非 loopback 的 MCP 监听必须显式启用并受保护
如果操作员将 MCP HTTP 入口从 loopback 地址改为非 loopback 地址，系统必须要求显式启用该模式，并联动认证或等价安全约束与清晰告警，避免把高权限桥接能力静默暴露到更广的网络范围。

#### 场景:操作员将 MCP HTTP 暴露到 `0.0.0.0`
- **当** 操作员尝试把 MCP HTTP 监听地址改为 `0.0.0.0` 或其他非 loopback 地址
- **那么** 系统必须将其视为高级高风险模式，并要求同时满足显式启用与认证保护要求后才允许生效

### 需求:MCP JSON-RPC 入口必须采用 POST 且错误回包保留请求 id
系统必须把 `/mcp` 约束为 JSON-RPC POST 入口。对该入口的非 POST 请求可以返回非 2xx（例如 404），但不得伪装成成功执行。对于 JSON-RPC 错误响应，系统必须保留并回显请求中的 `id`，确保客户端能将错误与原调用关联。

#### 场景:客户端用 GET 访问 `/mcp`
- **当** 客户端直接对 `/mcp` 发起 GET 请求
- **那么** 系统必须明确返回“非 MCP JSON-RPC 执行入口”的响应，而不是返回一个看似可执行但无语义保障的成功结果

#### 场景:工具调用失败时返回 JSON-RPC 错误
- **当** 客户端发起带 `id` 的 JSON-RPC 请求，且调用过程出现参数错误、能力不存在或运行时失败
- **那么** 系统必须在错误回包中回显同一个 `id`，避免客户端因 `id` 丢失而无法正确解码或关联错误

### 需求:Raw command 兜底必须受策略约束并可审计
如果系统暴露 raw command 兜底能力，则该能力必须接受策略校验、审批流程和输出缓存约束。每次 raw command 调用必须产生可审计的事件记录，并将输出纳入 artifact 体系。

#### 场景:客户端使用 raw command 作为兜底
- **当** typed tools 无法覆盖某个边缘调试场景而客户端改为请求 raw command
- **那么** 系统必须对该请求执行策略检查，并在执行后记录事件与 artifact 标识

### 需求:MCP 终端相关 typed tools 必须支持 Windows 宿主运行
BridgingIO 的终端相关 MCP typed tools（包含 one-shot exec、interactive shell 及其读写控制链路）必须在 Windows 宿主环境下可用，禁止依赖仅在 POSIX shell 成立的启动假设。

#### 场景:Windows 上调用 one-shot exec
- **当** BridgingIO core 运行在 Windows，且 MCP 客户端调用终端 one-shot exec 工具
- **那么** 系统必须返回可执行结果或标准化错误，不得因硬编码 `/bin/sh` 导致工具不可用

#### 场景:Windows 上调用 interactive shell 链路
- **当** BridgingIO core 运行在 Windows，且 MCP 客户端依次调用 open/write/read/interrupt/close 等 shell 相关工具
- **那么** 系统必须维持一致的句柄语义与状态流转，而不是在启动阶段因平台不兼容失败

### 需求:MCP 命令参数处理必须避免将 POSIX 单引号规则误用于 Windows
系统在构造终端命令参数时，必须按平台使用兼容的引用策略。Windows 运行时禁止把 POSIX 单引号转义规则作为默认策略，以避免命令参数被错误包裹而执行失败。

#### 场景:Windows 路径参数包含空格
- **当** MCP 工具调用需要传递包含空格的路径或参数，且 BridgingIO core 运行在 Windows
- **那么** 系统必须使用 Windows 兼容的参数传递方式，禁止输出依赖 POSIX 单引号语义的参数文本

#### 场景:非 Windows 保持既有引号行为
- **当** BridgingIO core 运行在非 Windows 平台，且 MCP 工具调用涉及 shell 引号处理
- **那么** 系统必须保持既有 POSIX 引号语义，避免因为 Windows 兼容修复而引入非 Windows 回归

### 需求:MCP 终端执行必须消费结构化 invocation 而不是 shell 字符串真相
BridgingIO 的终端相关 MCP typed tools 在执行 SSH、ADB 或其他终端型 target 时，必须消费由 connector / adapter 产出的结构化 invocation 作为执行真相源，而不是在 MCP 层重新拼接 shell 字符串。

#### 场景:interactive shell open 直接消费 structured interactive invocation
- **当** MCP 客户端调用 `terminal.shell.open` 为某个 SSH 或 ADB target 打开 interactive shell
- **那么** 系统必须优先使用 structured interactive invocation 直接驱动 runtime launch，而不是固定先走 host baseline shell

#### 场景:structured interactive launch 失败时回退并暴露诊断
- **当** `terminal.shell.open` 的 structured interactive launch 在 runtime 启动阶段失败
- **那么** 系统必须回退到 host baseline 路径，并在返回结果中暴露 `launch_strategy`、`launch_fallback_applied` 与 `launch_diagnostics`，使调用方能够区分“structured 直通成功”和“fallback 生效”

### 需求:MCP 终端相关 typed tools 必须保留宿主平台与 target 方言的双重语义
MCP 终端相关 typed tools 必须同时尊重宿主平台适配层与 target shell 方言。系统不得把本地平台引号、路径或 shell 语义错误传播到远端 target 的命令方言中，也不得把远端方言反向当成本地 runtime 规则。

#### 场景:Windows 宿主构造 ADB terminal 调用
- **当** BridgingIO core 运行在 Windows，且 MCP 客户端调用 ADB target 的 one-shot exec 或 interactive shell
- **那么** 系统必须使用 Windows 兼容的本地进程调用方式，同时保持目标端 ADB shell 的命令方言与参数语义，不得因本地平台切换而错误改变远端命令含义

#### 场景:未来非 POSIX 远端 shell
- **当** 某个 target 明确声明远端 shell 方言不是 POSIX，例如 Windows `cmd` 或 PowerShell
- **那么** MCP 终端工具必须允许该 target 走自己的 dialect 适配路径，而不是继续默认使用 POSIX 单引号与 POSIX 状态推断规则

### 需求:MCP 诊断与详细说明必须暴露关键平台解析结果
当系统向 MCP 客户端暴露终端能力、诊断信息或能力详述时，必须能够说明当前请求涉及的关键平台解析结果，至少包括 toolchain 来源、宿主平台相关的本地 transport / runtime 语义，以及 target shell 方言选择。这样调用方才能理解为什么某个目标在不同宿主平台上表现不同。

#### 场景:客户端读取 toolchain 与终端能力诊断
- **当** MCP 客户端或本地控制面读取某个 target 的 toolchain / terminal 诊断
- **那么** 返回结果必须能够区分“本地宿主平台如何启动进程”和“远端 target 使用何种 shell 方言”，而不是只返回一个模糊的命令字符串

