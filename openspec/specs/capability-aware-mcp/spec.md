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

### 需求:接收 `target` 参数的 MCP tools 必须使用统一的 target 引用解析
所有接收 `target` 参数的 MCP typed tools 必须在执行前通过统一的 target resolver 解析用户输入，而不是继续在各个 tool 内分别做精确字符串查找。该 resolver 必须同时支持 canonical target id、alias 与 display name 的统一查询语义。

#### 场景:宽松归一化唯一命中 target
- **当** MCP 客户端调用接收 `target` 参数的 tool，传入 `test_device`，而系统中存在唯一可执行 target `test-device`
- **那么** 系统必须将其解析为 canonical target `test-device` 并继续执行，而不是因为 `-` 与 `_` 的差异直接报错

#### 场景:多个接收 `target` 参数的 tools 行为保持一致
- **当** MCP 客户端分别调用 `bridgingio.terminal.exec`、`bridgingio.terminal.shell.open` 与 `bridgingio.target.inspect_basic`，并传入同一个 target 引用
- **那么** 这些 tools 必须共享同一套 target 解析结果与确认语义，而不是出现某个 tool 可以解析、另一个 tool 直接报错的漂移行为

### 需求:存在潜在歧义但有可选候选时 MCP 必须返回结构化确认结果
当接收 `target` 参数的 MCP tool 无法安全自动选择唯一 target，但又存在可信候选时，系统必须返回结构化确认结果，而不是直接执行或直接把该情况视为错误。该确认结果必须是正常 tool 返回，并且必须包含请求输入、策略原因、精确命中项（如果存在）与候选列表。

#### 场景:target 输入存在 typo 候选
- **当** MCP 客户端传入 `test-devics`，系统未找到精确命中，但存在可信候选 `test-device`
- **那么** 系统必须返回 `confirmation_required` 类结果，提示调用方确认实际 target，而不是直接执行 `test-device` 或直接返回“target not found”

#### 场景:精确命中但存在同 family 候选且策略要求确认
- **当** MCP 客户端传入 `test`，系统中存在可执行 target `test`、`test-1`、`test-2` 与 `test-3`，且当前策略为 `confirm_if_family`
- **那么** 系统必须返回结构化确认结果，并同时把精确命中的 `test` 与其 family-related 候选一并返回，而不是直接执行 `test`

### 需求:确认结果必须无副作用且仅使用静态或已缓存摘要
当 MCP tool 返回 target 确认结果时，系统必须保持该次调用为无副作用的解析阶段：禁止为了确认而建立新的 transport、channel、artifact 或远端探测动作。确认候选中使用的上下文信息必须来自配置或已缓存的运行时摘要。

#### 场景:确认结果不触发远端命令执行
- **当** MCP 客户端调用 `bridgingio.target.inspect_basic`，但 target 解析进入 `confirmation_required`
- **那么** 系统必须只返回候选确认信息，而禁止在任何候选 target 上主动执行 `uname`、`whoami` 或其他探测命令

#### 场景:确认候选返回可帮助选择的摘要
- **当** MCP tool 返回 target 确认结果
- **那么** 每个候选项必须至少包含 canonical target id、显示名称、kind，以及可用时的 aliases、notes、连接摘要、诊断摘要或已缓存会话摘要，以帮助调用方做出选择

### 需求:model-plane HTTP 必须支持 agent 认证并将其应用于 loopback 与非 loopback 监听
BridgingIO 的 model-plane HTTP 必须支持面向 agent 的正式认证机制，例如 bearer token，并且一旦启用该认证模式，系统必须在 loopback 与非 loopback 监听上一致执行认证校验。系统不得因为监听地址是 `127.0.0.1` 就绕过已配置的认证约束。

#### 场景:loopback 模式下缺失 token
- **当** model-plane 监听在 `127.0.0.1`，且当前实例启用了 bearer 或等价 agent 认证
- **那么** 未携带有效凭证的客户端请求必须被拒绝，而不能因为它来自 loopback 就被视为自动可信

#### 场景:non-loopback 模式下启用认证
- **当** 操作员把 model-plane HTTP 暴露到非 loopback 地址，并启用了 agent 认证
- **那么** 系统必须对所有入站请求执行相同的凭证校验与授权流程，而不是区分“本地请求”和“远端请求”走两套身份判断

### 需求:agent access token 必须由用户显式签发并具备撤销、限权和可选时效
BridgingIO 的 agent access token 必须被建模为用户显式签发的访问凭证，而不是静态硬编码口令。系统必须支持长期 token 与时效 token 两种形式，并且至少提供 revoke、scope 限制、last-used 追踪与可选 idle timeout 语义。服务端不得依赖明文 token 持久化作为唯一真相。

#### 场景:用户创建长期 token
- **当** 用户为某个受信任 agent 创建长期访问 token
- **那么** 系统必须允许该 token 不设置绝对过期时间，但仍必须支持 revoke、scope 限制、最近使用时间追踪与等价的风险控制

#### 场景:用户创建带时效 token
- **当** 用户为某次 run 或特定自动化任务创建带 TTL 的访问 token
- **那么** 系统必须在 token 到期后自动拒绝该凭证，并允许操作员在到期前主动 revoke

### 需求:agent token 必须以稳定 metadata record 持久化，且仅保存 hash 真相
系统必须为每个已签发的 agent token 维护稳定的 metadata record，至少覆盖 token 标识、principal、label、status、scope、创建者、创建时间、最近使用时间、过期时间、idle timeout、撤销信息以及可选的 delegation lineage。服务端不得把明文 token 作为长期持久化真相。该 record 必须可映射到 `AgentTokenRecord` 与 `TokenScopeRecord` 两层对象：`AgentTokenRecord` 负责 token 生命周期与 hash-only 真相，`TokenScopeRecord` 至少覆盖 `target_ids`、`tool_ids`、`max_risk_envelope`、`allow_open_shell`、`allow_write_shell_input`、`allow_artifact_cross_principal`、`allow_delegation` 与 `allow_admin_actions`。

#### 场景:签发长期 token 后查询元数据
- **当** 用户已创建一个长期 agent token，并通过本地受信任入口查看其状态
- **那么** 系统必须能够返回该 token 的 metadata record，而无需重新展示明文 token

#### 场景:token 被 revoke 后拒绝访问
- **当** 某个 token 已被 revoke，但客户端仍继续携带旧 token 发起请求
- **那么** 系统必须根据持久化的 token metadata record 判定其已失效，并拒绝访问

#### 场景:签发 token 后保留 scope 真相字段
- **当** 系统签发一个可访问部分 target 且允许 interactive shell 的 token
- **那么** 系统必须把对应 scope 字段持久化到 `TokenScopeRecord`，并在后续授权时据此判定，而不能退化为仅按 token 存在与否放行

### 需求:token 查询与管理返回必须使用安全投影而不是内部认证真相
系统必须把 token 的内部认证真相与对本地管理面的展示结果分离。即使在本地受信任 control-plane 中，token 查询默认也只能返回安全投影，例如 `token_id`、label、display-safe 的 token 指纹/摘要、scope 摘要、状态与时间戳，而不能返回明文 token、原始 `token_hash`、内部 matcher cache、精确 network binding 或等价认证内部字段。普通 model-plane / MCP 请求不得把认证内部 record 作为调试信息回显给模型。

#### 场景:本地管理员查看 token 列表或详情
- **当** 本地受信任 control-plane 请求查看当前 token 列表或某个 token 的详情
- **那么** 系统必须返回 display-safe summary，并且若需要辅助人工识别 token，只能返回 display-safe 的指纹/摘要字段，而不能输出明文 token 或原始 `token_hash`

#### 场景:已认证 agent 请求获取自身认证细节
- **当** 某个已认证 agent 试图通过 model-plane 或普通 MCP tool 查看自身或其他 token 的内部认证记录
- **那么** 系统必须拒绝该请求，或仅返回最小化 principal / scope 摘要，而不能回显认证内部真相

### 需求:agent token scope 必须采用多维默认拒绝模型
系统必须把 agent token scope 设计为多维授权模型，而不是单一的“是否能访问 MCP 入口”。至少必须能表达 target、tool / capability、风险边界，以及 interactive shell / artifact 等关键控制位。只要任一维度不允许，本次请求就必须在真正执行前被拒绝。

#### 场景:read-only token 访问 interactive shell
- **当** 某个 token 仅具备只读 scope，但客户端尝试打开 interactive shell 或执行需要 shell 持续上下文的高风险终端操作
- **那么** 系统必须在真正建立 channel 前拒绝该请求，而不能因为该 token 已通过认证就默认允许 interactive 行为

#### 场景:token 仅允许部分 target
- **当** 某个 token 的 scope 只允许访问 `lab-ssh-01`，但客户端尝试调用另一个 target 的 typed tool
- **那么** 系统必须返回基于 scope 的拒绝结果，而不是把该情况伪装成普通 target not found

### 需求:系统必须提供内置 scope profile 语义模板并可映射到多维 scope
系统必须提供可复用的内置 scope profile 或等价推荐模板，至少覆盖 `read-only`、`interactive-read`、`operator` 与 `admin`。这些 profile 必须可确定性地映射到多维 `TokenScopeRecord`，并保持“默认拒绝、显式放行”的授权语义。

#### 场景:按 profile 创建 read-only token
- **当** 本地管理员按 `read-only` profile 创建 token
- **那么** 该 token 的 `TokenScopeRecord` 必须禁止 interactive shell 打开与写入类风险操作，并仅允许 profile 规定的读能力

#### 场景:按 profile 创建 operator token
- **当** 本地管理员按 `operator` profile 创建 token
- **那么** 系统必须把该 profile 映射为受 target/tool/risk 约束的常规操作权限，而不是等价为无限制 admin 权限

### 需求:现有 MCP tool catalog 必须维护最小 scope profile 映射真相
系统必须为现有 MCP typed tools 维护一份最小 scope profile 映射真相源，并把 interactive shell / artifact 的资源归属约束纳入授权判定。该映射必须可审计、可演进，且不能把 terminal tool 权限误当成“任意 shell 都可执行”。

#### 场景:tool 请求触发 profile 下限校验
- **当** 已认证 token 请求调用某个 MCP typed tool
- **那么** 系统必须先根据 tool catalog 映射判定该 tool 的最小 profile，并验证 token profile 是否满足，再继续后续授权流程

#### 场景:跨 principal 资源访问缺失共享授权
- **当** token 在 target 与 tool 维度满足条件，但试图读取或操作其他 principal 的 interactive shell 或 artifact
- **那么** 系统必须因资源归属约束拒绝该请求，除非存在显式共享或更高管理权限

### 需求:interactive shell 与 artifact 访问必须默认受 principal 资源归属约束
对于 interactive shell、artifact、approval context 等运行时资源，系统必须默认把“资源归属”纳入授权判断。即使某个 token 在 target / tool 维度上具备权限，也不得自动读取、关闭、中断或派生其他 principal 创建的运行时资源，除非存在显式共享或更高管理权限。

#### 场景:读取其他 principal 的 shell transcript
- **当** 某个已认证 token 具备 `terminal.shell.read` 之类的能力，但尝试读取另一个 principal 创建的 interactive shell transcript
- **那么** 系统必须默认拒绝该请求，而不是因为它拥有相同 target scope 就自动放行

#### 场景:基于其他 principal 的 artifact 继续 refine
- **当** 某个已认证 token 尝试对另一个 principal 创建的 artifact 执行 refine
- **那么** 系统必须默认拒绝，除非该 artifact 已被显式共享或该 principal 具备更高管理权限

### 需求:终端类 tool 的授权必须结合命令风险分类
对于 `bridgingio.terminal.exec` 与 `bridgingio.terminal.shell.write` 等终端类 typed tools，系统必须在 tool scope 之外继续结合命令风险分类执行授权判断。系统不得因为 token 具备 terminal tool 访问权限，就默认允许所有 shell 命令。

#### 场景:interactive-read token 尝试执行写操作
- **当** 某个 token 具备 interactive shell 能力，但其风险边界仅允许 read-only 或 interactive-read
- **那么** 若客户端通过 `terminal.exec` 或 `terminal.shell.write` 提交写操作、删除操作或 privileged 操作，系统必须在真正执行前拒绝或进入更高一层授权 / 审批流程

#### 场景:tool 允许但风险边界不足
- **当** 某个 token 的 tool scope 允许 `bridgingio.terminal.exec`，但其风险边界不允许 `privileged`
- **那么** 系统必须拒绝该次 privileged terminal 请求，而不是把 tool 访问权限等同于无限制终端权限

### 需求:长期 token 与派生 run token 必须保持 scope 只能收窄
如果系统支持从长期 token 派生短期 run token 或等价 delegation 凭证，则子凭证的 scope 与生命周期都必须是父凭证的严格子集。系统不得允许子凭证扩大 target、tool 或风险边界，也不得允许其寿命超过父凭证。

#### 场景:长期 token 派生短期 run token
- **当** 某个具备 delegation 权限的长期 token 为一次临时 run 派生短期 token
- **那么** 新 token 的 target 范围、tool 范围、风险边界和有效期都必须不超过父 token，且 revoke 父 token 后子 token 也必须失效

### 需求:agent token 生命周期必须单向收敛并级联收权
系统必须把 agent token 设计成“单向终态 + 可逆启停”的组合生命周期对象。token 在签发后进入可访问态，并且在未 `revoked`、`expired` 或 `deleted` 时允许本地受信任管理面临时切换为 `disabled` 或等价的不可访问状态；`revoked`、`expired` 与 `deleted` 仍属于不可重新激活的收敛终态。系统不得通过 enable/disable 开关让已过期或已撤销 token 恢复可用。若 token 之间存在 delegation lineage，则父 token 进入失效终态时必须对仍处于可访问态或 `disabled` 管理态的子 token 一致施加级联收权。

#### 场景:token 被禁用后不得继续使用
- **当** 某个 token 被本地受信任管理面切换为 `disabled`
- **那么** 系统必须拒绝继续使用该 token，直到其被重新启用或进入其他终态

#### 场景:禁用 token 到期后不得因重新启用而复活
- **当** 某个 token 在 `disabled` 状态下到达 TTL 或 idle timeout 并进入 `expired`
- **那么** 系统不得因本地管理面后来重新启用该 token 而把它恢复为可访问状态

#### 场景:父 token 收权后子 token 级联失效
- **当** 某个允许 delegation 的父 token 被 revoke 或因策略进入失效终态
- **那么** 仍处于可访问态或 `disabled` 管理态的子 token 必须级联失效，并在后续访问中被一致拒绝

### 需求:认证后的 principal 必须从凭证派生，而不是信任请求体自报身份
对 model-plane 或 MCP typed tools 的每次调用，系统都必须把 authenticated principal 视为真正的身份来源。请求体中的 `agent_id`、`run_id`、`client_session_id` 等字段只能作为调用标签或子上下文，不能单独决定权限，也不能覆盖 token 所绑定的 principal / scope。

#### 场景:请求体自报其他 agent 身份
- **当** 一个客户端携带有效 token 发起请求，但在请求体中自报不同的 `agent_id`
- **那么** 系统必须继续以 token 对应的 authenticated principal 作为权限与审计真相源，而不能因为请求体字段变化就提升或转移其身份

#### 场景:token scope 不允许访问目标
- **当** 已认证客户端尝试调用一个超出 token scope 的 target 或 tool
- **那么** 系统必须在真正执行前拒绝该请求，并将拒绝原因归因到 authenticated principal 与其 scope，而不是把该错误表述成普通 target not found

### 需求:`auth_mode=none` 必须被视为显式受限模式
`model_plane.http.auth_mode = none` 只能用于操作员显式声明的开发、测试或受控诊断场景，而不得继续被视为常规安全默认值。只要系统启用了 bearer 或更强认证模式，就必须在 loopback 与 non-loopback 监听上一致执行认证。

#### 场景:发行模式下使用默认配置启动 model-plane
- **当** 系统以常规发行模式启动 model-plane
- **那么** 推荐默认认证模式必须是 bearer 或等价正式认证模式，而不是继续默认把 `none` 视为常规工作配置

#### 场景:操作员显式启用 `auth_mode=none`
- **当** 操作员在开发或诊断场景下显式把 `auth_mode` 设置为 `none`
- **那么** 系统必须把当前实例标记为受限模式，并通过 diagnostics 或启动信息清晰提示该实例未启用正式 agent 认证

### 需求:认证、scope 授权、策略与审批必须串联生效
对于每次会触发 target、session、artifact 或 typed tool 执行的请求，系统必须按“认证 -> scope 授权 -> policy -> 审批 -> 执行”的顺序串联判断。任何上游阶段拒绝时，系统都不得继续进入后续执行阶段。拒绝结果必须带有可审计的阶段归因（例如 `authn` / `authz` / `policy` / `approval`），避免把安全拒绝伪装为一般业务错误。

#### 场景:token 允许但策略要求审批
- **当** 某个已认证 token 的 scope 允许访问某个 target 和 tool，但该操作按策略仍需人工审批
- **那么** 系统必须进入审批流程，并在审批完成前禁止实际执行，而不能因为 token 已授权就绕过审批

#### 场景:策略允许但 token scope 不允许
- **当** 某个操作在 profile / policy 层面本来可执行，但调用方 token 的 scope 不允许
- **那么** 系统必须优先因 scope 不允许而拒绝请求，而不是继续进入执行或审批阶段

#### 场景:拒绝结果的阶段归因可审计
- **当** 某个请求在授权链路中被拒绝
- **那么** 系统必须记录并返回可审计的拒绝归因阶段，以便审计事件能够区分该请求是失败在认证、scope、策略还是审批阶段

### 需求:agent token scope 更新必须版本化并保留历史授权真相
当本地受信任管理面为一个已签发 token 调整 target、tool 或其他授权维度时，系统必须通过创建新的 `TokenScopeRecord` 版本并切换 active 指针来生效，而不是直接覆盖旧 scope 记录。旧 scope 版本必须保留为审计真相，但不得继续参与后续授权判定。

#### 场景:为现有 token 增加新的 target 访问范围
- **当** 本地管理员为一个已有 token 新增可访问的 target 集合
- **那么** 系统必须创建新的 scope version，切换该 token 的 active scope 指针，并将旧 scope 保留为 superseded 审计记录，而不能直接修改原 scope 使历史授权不可追溯

#### 场景:scope 更新后后续请求按新版本鉴权
- **当** 某个 token 的 active scope 已从旧版本切换到新版本
- **那么** 后续基于该 token 的 target / tool / risk 授权判定必须使用新的 active scope，而不能继续沿用旧 scope 的权限结果

### 需求:首版 token scope 即使只开放 `target_ids` 也必须保持多维默认拒绝结构
即使首版本地管理面只真正开放 `target_ids` 一类的权限配置，系统仍必须把 token scope 持久化为多维结构，并为未显式开放的维度保留正式字段与默认拒绝语义。系统不得把“当前 UI 没有配置该字段”误解释为该维度自动放行。

#### 场景:首版仅配置 target 范围创建 token
- **当** 本地管理员创建一个 token，并只提供 `target_ids` 而未显式配置 tool、interactive shell 或其他维度
- **那么** 系统必须持久化完整的 scope record，并让未配置维度按默认拒绝或 profile-default 语义收敛，而不是把这些能力隐式视为允许

#### 场景:后续版本扩展更多 scope 维度
- **当** 系统后续为 token 管理面增加 `tool_ids`、risk envelope 或 interactive shell 等配置项
- **那么** 已有 token record 与 scope record 必须能够在不破坏既有持久化语义和 IPC 基本形状的前提下吸收这些字段，而不是要求重新设计 token 真相模型

### 需求:model-plane 请求必须生成可供管理面展示的安全来源摘要
系统必须为每个进入 model-plane 的请求生成可供本地受信任管理面展示的安全来源摘要，而不是只保留不可读的内部认证记录。若请求由带 label 的 token 认证，则该摘要必须优先使用 token label；若请求未携带 token，则摘要必须回落到稳定请求指纹、user-agent 摘要或等价来源信息。该摘要禁止包含明文 token、token hash 或其他认证内部真相。

#### 场景:带标签 token 的请求进入 model-plane
- **当** 某个请求通过带有 label 的 agent token 完成认证
- **那么** 系统必须为该请求生成可供管理面展示的来源摘要，并使该摘要优先体现 token label，而不是回显明文 token 或仅暴露内部 token_id/hash

#### 场景:未携带 token 的请求进入 model-plane
- **当** 某个请求未携带 token，或当前模式下不存在 token 认证信息
- **那么** 系统必须为该请求生成稳定的非认证来源摘要，例如请求指纹或 user-agent 摘要，而不是把它完全折叠成不可区分的匿名请求

### 需求:loopback 匿名兼容 principal 只允许访问 plain target
为兼容旧版本升级路径，model-plane 必须把“无 token 仍可访问 plain target”的能力建模为显式启用、仅限 loopback 的匿名兼容 principal。该 principal 不是 `auth_mode=none` 的隐式别名，也不得访问 sealed target 或扩大到 non-loopback 监听。

#### 场景:loopback 缺失 token 时执行 plain target
- **当** model-plane 监听在 loopback，操作员显式启用了匿名兼容模式，且某个请求未携带 token
- **那么** 系统可以将该请求映射为 `anonymous-local` principal，并仅允许其访问 `access_class = anonymous-local` 的 plain target 与最小 public catalog

#### 场景:loopback 缺失 token 时访问 sealed target
- **当** model-plane 监听在 loopback，某个未携带 token 的请求尝试访问 sealed target 或依赖 vault overlay 的 target profile
- **那么** 系统必须拒绝该请求，而不能因为当前实例启用了匿名兼容模式就放宽到 sealed target

#### 场景:non-loopback 缺失 token
- **当** model-plane 监听在非 loopback 地址，且某个请求未携带 token
- **那么** 系统必须拒绝该请求，而不能将 non-loopback 请求降级为匿名兼容 principal

### 需求:显式携带无效 token 的请求不得回退为匿名 principal
一旦请求显式携带 bearer token，系统必须先按 token principal 路径完成认证。若该 token 无效、过期、被撤销或 scope 不允许，系统必须直接拒绝，而不能把该请求视为“等价于没带 token”并回退到匿名兼容 principal。

#### 场景:显式携带无效 token
- **当** 某个 loopback 请求显式携带了无效、过期或已撤销的 bearer token
- **那么** 系统必须返回认证失败或基于 scope 的拒绝结果，而不能改按匿名 principal 继续执行 plain target

### 需求:长期 token 签发与 scope 扩大必须由本地 attestation 强制保护
model-plane 的长期 agent token 签发与 scope 扩大必须在 runtime 中消费真实 `LocalAdminAttestationRecord`，而不是只依赖 trusted client 自报“已经完成本地验证”。该保护必须在 `AuthN/AuthZ` 之前作为 token authority 的管理动作入口条件执行。

#### 场景:缺少 attestation 时签发长期 token
- **当** trusted client 请求创建长期 agent token，但未提供匹配当前 intent 的有效 attestation
- **那么** 系统必须拒绝签发，并且不得生成任何可用的 token 明文或 metadata record

#### 场景:扩大 token scope 时 attestation 与 payload 不匹配
- **当** trusted client 请求扩大某个 token 的 scope，但 attestation 对应的是其他 token、其他 scope 变化或其他 action kind
- **那么** 系统必须拒绝该次 scope 变更，而不能把 attestation 当作通用管理员凭证

### 需求:长期 token 明文必须只允许一次性显示
长期 token 在 runtime 成功签发后，必须只在签发响应中以一次性结果形式返回。后续 list/query/revoke/scope update 等控制面读取只能返回安全摘要，不得再次 reveal 同一 token 明文。

#### 场景:首次签发长期 token
- **当** 本地管理员通过受信任控制面成功创建一个长期 token
- **那么** 系统必须在该次签发响应中返回一次性 token 明文和对应 summary，并在后续查询中仅返回 summary

#### 场景:后续列出 token
- **当** trusted control-plane 或桌面 UI 后续列出现有 token
- **那么** 系统必须只返回 display-safe 的 token summary，而不能再次 reveal 已签发 token 的明文值

### 需求:MCP 错误回包必须映射到共享错误与状态契约
BridgingIO 的 MCP 回包在出现参数错误、能力未就绪、方法未实现、受控降级或运行时失败时，必须映射到共享错误与状态契约，而不是继续依赖 ad hoc 字符串或过粗错误类别。

#### 场景:tool 尚未实现
- **当** MCP 客户端调用一个当前版本尚未实现但已保留入口的 tool 或 capability
- **那么** JSON-RPC 错误或等价回包必须包含 `method_not_implemented` 对应的共享错误/状态语义，并保留原始请求 `id`

#### 场景:能力处于受控降级
- **当** MCP 客户端调用的能力当前处于 `degraded` 或 `not_ready` 状态
- **那么** 系统必须在回包中明确返回对应共享状态与恢复提示，而不是把该情况统一压扁为内部错误

### 需求:bearer token 认证失败必须返回可区分的 token 拒绝原因
当 model-plane 或 MCP typed tools 因 bearer token 认证失败而拒绝请求时，系统必须返回可区分的 token 拒绝原因，而不能继续把 `invalid`、`disabled`、`revoked`、`expired` 等不同情况压成单一的“无效 token”语义。该拒绝原因必须能够被 MCP 回包、审计记录和本地管理面一致消费，并与后续的 scope / policy 拒绝区分。

#### 场景:disabled token 发起 MCP 请求
- **当** 客户端显式携带一个处于 `disabled` 管理状态的 bearer token 调用 model-plane 或 MCP typed tool
- **那么** 系统必须在 `authn` 阶段直接拒绝该请求，并返回等价于 `token_disabled` 的稳定拒绝原因

#### 场景:revoked token 发起 MCP 请求
- **当** 客户端显式携带一个已被 `revoked` 的 bearer token 调用 model-plane 或 MCP typed tool
- **那么** 系统必须在 `authn` 阶段直接拒绝该请求，并返回等价于 `token_revoked` 的稳定拒绝原因

#### 场景:expired token 发起 MCP 请求
- **当** 客户端显式携带一个已过期的 bearer token 调用 model-plane 或 MCP typed tool
- **那么** 系统必须在 `authn` 阶段直接拒绝该请求，并返回等价于 `token_expired` 的稳定拒绝原因

#### 场景:未知 token 发起 MCP 请求
- **当** 客户端显式携带一个不存在或无法匹配的 bearer token 调用 model-plane 或 MCP typed tool
- **那么** 系统必须在 `authn` 阶段直接拒绝该请求，并返回等价于 `token_invalid` 的稳定拒绝原因

