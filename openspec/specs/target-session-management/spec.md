# target-session-management 规范

## 目的
待定 - 由归档变更 define-bridgingio-foundation 创建。归档后请更新目的。
## 需求
### 需求:统一目标配置
BridgingIO 必须允许用户使用统一的数据模型保存目标 profile，并且首个版本必须至少支持 SSH 与 ADB 两种目标类型。每个 profile 必须至少包含目标类型、连接参数、凭据引用、默认策略、用户可见名称，以及可选的用户备注、调用别名或模型提示信息。

#### 场景:保存 SSH 目标 profile
- **当** 用户创建一个新的 SSH 目标并填写主机地址、端口、用户名、调用别名和凭据引用
- **那么** 系统保存的 profile 必须包含标准化的目标类型、连接参数、别名提示和策略字段，而不是仅保存一段未结构化的命令文本

### 需求:目标 profile 必须支持 target 级 toolchain override
系统必须允许 target profile 在 `targets.toolchains.<name>.path_override` 中声明当前 target 的局部工具覆盖。该字段是可选的，并且在语义上只影响当前 target 的连接器或 provider 工具解析，不得隐式修改全局 `toolchains.<name>.path_override`。

#### 场景:target override 覆盖全局默认值
- **当** 用户为某个 ADB target 配置 `targets.toolchains.adb.path_override`，同时系统全局存在 `toolchains.adb.path_override`
- **那么** 当前 target 必须优先使用自己的 `adb` 路径，而其他未配置 target override 的 ADB target 继续使用全局默认值

#### 场景:清空 target override 后回退全局默认值
- **当** 用户清空某个 target 的 `targets.toolchains.adb.path_override`，且系统全局仍保留 `toolchains.adb.path_override`
- **那么** 系统必须移除该 target 的局部 override，并让当前 target 回退使用全局默认值，而不是继续保留陈旧的 target 级生效路径

### 需求:连接器执行路径解析
系统必须为每个连接器或 provider 解析可执行工具来源，并且必须按“target 级用户覆盖路径、全局用户覆盖路径、系统 PATH、内置后备”这一顺序选择有效实现。该解析结果必须同时驱动诊断回显、one-shot exec、结构化基础信息探测与 interactive shell 启动，而不是只停留在诊断层。系统必须能够向 UI 和诊断接口返回当前 target 的 target override、global override 和最终生效来源。

#### 场景:target 未配置 override 时命中全局默认值
- **当** 某个 target 未配置 `targets.toolchains.adb.path_override`，但系统全局配置了 `toolchains.adb.path_override` 且该路径可用
- **那么** 系统必须使用全局默认值建立该 target 的 ADB 能力，并在诊断信息中明确表明当前生效来源来自全局 override，而不是 target override

#### 场景:全局与 target 都未配置时继续回退
- **当** 某个 target 既没有 target 级 override，也没有全局 override
- **那么** 系统必须继续按顺序尝试系统 PATH 与内置后备，而不是因为缺失显式 override 就直接失败

#### 场景:系统安装缺失但存在内置后备二进制
- **当** 用户系统未安装所需的 ADB 或 SSH 工具，但产品分发包中存在可用的内置后备二进制
- **那么** 系统必须使用该内置后备建立相关能力，并且在目标或诊断信息中显示当前来源为内置后备

#### 场景:诊断回显分层来源
- **当** UI 读取某个 target 的工具来源诊断
- **那么** 系统必须能够返回该 target 的 target override、global override 和最终生效来源，使 UI 可以区分“继承全局默认值”和“被 target override 覆盖”这两种状态

#### 场景:one-shot exec 使用 target 级生效路径
- **当** 某个 SSH 或 ADB target 配置了可用的 target 级 `path_override`，且用户或 AI 通过 `terminal.exec` 或 `inspect_basic` 触发一次性命令执行
- **那么** 系统必须使用该 target 当前解析出的有效工具路径拼接并执行连接命令，而不是回退到裸命令名或忽略 override

#### 场景:interactive shell 使用与诊断一致的连接器路径
- **当** 用户或 AI 为某个 SSH 或 ADB target 打开 interactive shell
- **那么** 系统必须使用与 diagnostics 和 one-shot exec 一致的有效工具路径建立该 shell，而不是另外走一套未应用 override 的启动路径

#### 场景:ADB serial 选择器优先于 transport 快捷标志
- **当** 某个 ADB target 同时提供了明确的 serial 值和 transport 提示
- **那么** 系统必须优先使用 serial 选择器建立命令或 shell 连接，而不是退化为 `-d`、`-e` 等无法唯一定位设备的快捷标志

### 需求:standalone core 必须统一管理配置装载、设置入口与控制平面
系统必须允许 Rust core 以前台进程或 daemon 形式独立运行，并且在启动时读取操作员提供的配置文件。该配置文件与 UI、CLI、后续其他前端看到的目标/profile 设置，必须映射到同一套 core-owned 数据模型与校验语义，而不是由 UI 自行维护一份独立 schema。受信任的本地 UI/control plane 默认必须通过本地 app API IPC 接入 core，而不是复用面向 AI 的 MCP HTTP 能力面。

#### 场景:standalone core 启动并装载目标配置
- **当** 用户以 standalone 模式启动 core，并提供包含 SSH/ADB 目标、默认策略或工具偏好的配置文件
- **那么** 系统必须在 core 内完成配置装载和标准化，并使这些目标可立即被 UI 或 MCP 客户端读取和使用

#### 场景:UI 修改目标设置
- **当** 用户在 UI 中修改某个 SSH 目标的主机地址、用户名、调用别名或凭据引用
- **那么** 该修改必须通过 core 的设置接口完成校验和持久化，而不是只停留在 UI 的本地状态中

#### 场景:UI 通过本地控制平面访问 core
- **当** 平台 UI 作为完整发行物的一部分启动并连接本地 core
- **那么** UI 必须通过受信任的本地 control-plane app API 访问 session、settings、approval 和诊断能力，而不是依赖面向 AI 的 MCP HTTP 入口

### 需求:core 设置必须包含模型平面监听配置
系统必须将模型平面的 MCP HTTP 监听配置纳入 core-owned settings/config 边界。该配置至少必须覆盖监听 host、监听 port、是否允许非 loopback 暴露，以及与之配套的认证或安全约束开关。

#### 场景:操作员修改 MCP HTTP 监听配置
- **当** 操作员在配置文件或 UI 中修改 MCP HTTP 的监听地址或端口
- **那么** core 必须对该配置执行标准化和校验，并将结果持久化到统一的 settings/config 模型中

### 需求:core 设置必须包含 artifact cache 配置
系统必须将 artifact cache backend 与容量治理配置纳入 core-owned settings/config 边界。该配置在 MVP 中至少必须覆盖 backend 类型、持久化根目录、最大缓存限制与淘汰策略。

#### 场景:操作员在配置文件中切换 artifact cache backend
- **当** 操作员在 standalone 配置中把 artifact cache 从 `memory` 切换到 `filesystem`
- **那么** core 必须校验并装载该配置，使后续 artifact 采集与读取遵循新的 backend 语义

#### 场景:操作员在 UI 中修改持久化缓存限制
- **当** 操作员在 UI 或本地 control plane 中修改 artifact cache 的最大缓存限制或淘汰策略
- **那么** core 必须通过统一 settings/config 模型完成校验、持久化与诊断回显，而不是由 UI 私自保存一份本地偏好

### 需求:standalone 配置文件必须具备版本化结构与参考样例
MVP 必须为 standalone core 定义一个规范化、可版本迁移的人类可编辑配置结构，并提供至少一份可直接用于启动验证的参考样例。该配置在 MVP 中必须以 TOML 作为规范格式，并且至少覆盖 `schema_version`、`core`、`storage`、`storage.artifacts`、`vault`、`control_plane`、`model_plane.http`、`toolchains`、`policies.defaults` 和 `targets` 等顶层语义。

配置文件只允许承载操作员声明式配置和 `CredentialRef` 一类的非敏感引用；session、artifact、approval、environment fingerprint cache 等运行时状态必须进入 core 的运行期存储，而不是写入该配置文件。

#### 场景:操作员在 UI 之前使用样例配置启动 standalone core
- **当** 操作员在尚未完成平台 UI 的阶段，使用项目提供的 standalone TOML 样例文件启动 core
- **那么** core 必须能够完成配置装载、target 标准化、模型平面监听参数初始化以及本地控制平面初始化，从而让团队先验证 core 的基本能力

#### 场景:配置文件 schema 版本不受支持
- **当** core 读取到一个 `schema_version` 不受当前版本支持的 standalone 配置文件
- **那么** 系统必须返回明确的版本不兼容或迁移提示，而不是静默忽略未知结构继续运行

### 需求:会话生命周期管理
系统必须为每次连接创建稳定的 session 标识，并且必须记录其状态、开始时间、最后活动时间和关闭原因。会话状态至少必须覆盖“连接中、已连接、降级、失败、已关闭”。

#### 场景:成功建立目标会话
- **当** 用户或 MCP 客户端对一个可用目标发起连接
- **那么** 系统必须创建新的 session 标识，将状态转为“已连接”，并记录该会话的目标引用和时间戳

### 需求:会话必须带访问作用域并默认隔离不同 agent
系统必须使用访问作用域区分不同 workspace、principal、agent、run、thread 或客户端会话。对于同一 target 的访问，只要访问作用域不同，系统就禁止默认复用同一个逻辑会话、时间线、artifact 或审批数据。

#### 场景:两个 agent 访问同一目标
- **当** 两个不同的 agent 在相近时间访问同一个 SSH 主机或 ADB 设备
- **那么** 系统必须为它们创建不同的逻辑会话，并且禁止将一个 agent 的命令历史、artifact 和审批结果暴露给另一个 agent

### 需求:系统必须区分逻辑会话与底层传输连接
系统必须区分面向用户和 AI 的逻辑会话与底层的 SSH/ADB 传输连接。底层传输连接断开、重连或替换时，只要访问作用域和复用策略满足条件，系统就必须能够恢复到同一个逻辑会话，而不是强制创建新的工作上下文。

#### 场景:同一运行中断后恢复访问
- **当** 同一个 agent 在同一轮运行内携带相同的客户端会话标识再次访问同一目标
- **那么** 系统必须能够在复用策略允许时恢复到同一个逻辑会话，并保留原有时间线、artifact 引用和审批上下文

### 需求:同一逻辑会话必须支持多个并发通道
系统必须允许在同一个逻辑会话内打开多个并发终端或连接通道。每个通道必须具有独立的通道标识、状态和执行上下文，并且系统必须能够将命令事件和 artifact 同时关联到逻辑会话与具体通道。

#### 场景:一个通道执行命令，另一个通道采集日志
- **当** 用户或 AI 在同一个逻辑会话内同时打开两个针对同一服务器的 SSH 通道，其中一个用于执行命令，另一个用于持续获取日志
- **那么** 系统必须保留它们属于同一个逻辑会话的关联关系，同时区分两个通道各自的命令事件、输出和状态

### 需求:终端型 Target 必须同时支持单次执行与交互式通道
对于 SSH、ADB 等终端型 target，系统必须同时支持 `one-shot exec` 与 `interactive shell` 两种操作方式。两种方式必须共享同一套逻辑会话与审计体系，但必须保持不同的状态语义。

`one-shot exec` 用于单次命令访问，默认不保留 shell 上下文；`interactive shell` 用于在同一个 shell 进程中连续执行多步任务，并保留工作目录、环境变量、prompt 与进程状态直到 channel 关闭。

#### 场景:单次命令访问 ADB 或 SSH 目标
- **当** 用户或 AI 只需要执行一次类似 `whoami`、`uname -r` 的短命令
- **那么** 系统必须允许通过单次执行模式完成调用，而不要求先创建持久 shell 句柄

#### 场景:在同一交互式 shell 中连续执行依赖上下文的命令
- **当** 用户或 AI 在一个终端型 target 上先执行 `export BUILD_MODE=debug`，随后继续执行依赖该环境变量的命令
- **那么** 系统必须允许这些命令在同一个交互式 channel 中执行，并保留该 shell 的上下文状态

#### 场景:同一逻辑会话内两个交互式 shell 默认彼此隔离
- **当** 用户或 AI 在同一个逻辑会话内同时打开两个 SSH 或 ADB 交互式 shell，其中一个 channel 执行了 `cd /tmp` 或 `export X=1`
- **那么** 这些 shell 级状态必须只影响当前 channel，而不能自动泄漏到另一个 channel

#### 场景:交互式命令依赖 TTY 能力
- **当** 用户或 AI 在交互式 shell channel 中执行 `top`、`stty -a` 等依赖 TTY 的命令
- **那么** 系统必须优先以 PTY 后端承载该 channel；若当前平台无法分配 PTY，系统必须回退到 pipe 并在 transcript 中写入可观测的降级提示

### 需求:环境指纹与能力发现
系统必须在会话建立后探测并缓存标准化的环境指纹与能力摘要。该摘要必须至少包含操作系统类型、架构、内核版本或设备版本、默认 shell、检测到的关键工具和支持的结构化能力。

#### 场景:首次连接目标后生成能力摘要
- **当** 一个新的目标会话首次连接成功
- **那么** 系统必须生成该会话的环境指纹与能力摘要，并允许 UI 或 MCP 客户端读取该摘要而无需重复进行完整探测

### 需求:bundled 发行物中的 core 必须由 UI 托管并与其生命周期强绑定
在 bundled 发行物中，BridgingIO 默认必须把 Rust core 作为由本地 host adapter 托管的 `ui-managed-ephemeral` 进程启动。默认模式下，UI 必须在启动时拉起或接管同一发行物内的 managed core、通过本地 control-plane 完成 attach，并在 UI 正常退出或受控重启时确保该 core 最终退出并释放其本地 transport 与 model-plane 监听资源，而不是仅发出 shutdown 请求后放任其在后台残留。

#### 场景:UI 启动 bundled core 并完成 attach
- **当** 平台 UI 以 bundled 默认形态启动并准备进入控制台工作流
- **那么** 系统必须先完成旧实例协调、再拉起或接管同一发行物内受托管的 core、开放本地 control-plane，并在 UI attach 成功后才将该运行实例视为已就绪

#### 场景:UI 正常退出 bundled 应用
- **当** 用户正常关闭 bundled UI
- **那么** 宿主必须请求当前 managed core 关闭，并等待该实例真正退出且释放本地 endpoint 与默认监听资源后，才将本次关闭视为完成，而不是仅发送一次 shutdown 后立即结束生命周期管理

#### 场景:bundled host 因设置变更执行受控重启
- **当** bundled host 因设置变更或恢复操作需要重启当前 managed core
- **那么** 宿主必须先关闭旧 core、确认旧实例退出并释放关键资源，再启动 replacement core，而不是允许新旧两个实例在同一 runtime root 或监听地址上竞争

### 需求:standalone core 运行模式必须区分前台 `run` 与后台 `-d`
BridgingIO core 必须提供明确的 standalone 运行模式语义。`run` 必须表示前台运行并占据当前终端；`-d` 必须表示后台或脱离式运行。两种模式必须复用同一套 core-owned 配置、control-plane 和 model-plane 语义，而不是形成两套分叉实现。

#### 场景:操作员以前台模式启动 core
- **当** 操作员使用 `run` 启动 standalone core
- **那么** 系统必须以前台进程方式运行 core，并使其立即按照该配置开放相应的 control-plane 与 model-plane 能力

#### 场景:操作员以脱离式模式启动 core
- **当** 操作员使用 `-d` 启动 standalone core
- **那么** 系统必须以后台或脱离式方式运行同一套 core 语义，而不是要求 UI 托管该实例或切换到另一套实现路径

### 需求:本地 control-plane 必须提供 UI attach 与控制台 bootstrap 语义
受信任的本地 control-plane 必须允许平台 UI 在启动时完成 attach，并读取构建真实控制台所需的最小快照。该最小快照至少必须覆盖 targets、sessions、approvals、settings、diagnostics，以及时间线或 transcript 的初始读取入口。

#### 场景:UI 首次连接 core 并读取控制台快照
- **当** 平台 UI 连接到本地 core 并完成 attach
- **那么** control-plane 必须允许 UI 读取构建控制台首屏所需的最小真实状态，而不是要求 UI 先用本地 seed 占位

#### 场景:core 尚无目标或会话数据
- **当** UI attach 成功，但 core 当前尚未装载任何 target、session 或 artifact
- **那么** control-plane 必须返回真实的空结果，使 UI 能展示正式空态，而不是伪造示例目标或示例时间线

### 需求:终端执行必须按宿主平台选择可用 shell
对于本地终端 provider，系统必须按宿主平台选择可用 shell 启动命令执行流程，禁止在 Windows 上硬编码依赖 `/bin/sh`。当平台为 Windows 时，系统必须使用 `cmd` 语义；当平台为非 Windows 时，系统必须保持现有 POSIX shell 语义。

#### 场景:Windows 平台执行 one-shot 命令
- **当** 运行环境为 Windows，且用户或 MCP 客户端触发 one-shot exec
- **那么** 系统必须通过 `cmd` 路径执行命令，而不是尝试启动 `/bin/sh`

#### 场景:非 Windows 平台执行 one-shot 命令
- **当** 运行环境为 Linux 或 macOS，且用户或 MCP 客户端触发 one-shot exec
- **那么** 系统必须继续使用 POSIX shell 路径执行命令，并保持与既有行为兼容

### 需求:交互式 shell 启动必须具备跨平台兼容默认值
系统必须为 interactive shell 提供按平台兼容的默认启动方式。Windows 平台必须使用可持续交互的 `cmd` 启动参数；非 Windows 平台必须保持 `/bin/sh` 的交互行为。

#### 场景:Windows 平台打开交互式 shell
- **当** 运行环境为 Windows，且客户端请求打开 interactive shell 且未提供 launch command
- **那么** 系统必须创建可持续接收后续输入的 `cmd` 会话，并允许后续 write/read/interrupt/close 调用正常工作

#### 场景:非 Windows 平台打开交互式 shell
- **当** 运行环境为非 Windows，且客户端请求打开 interactive shell
- **那么** 系统必须保持现有 `/bin/sh` 交互模式，不得因为跨平台适配而破坏既有 shell 会话行为

### 需求:带 shell 状态执行必须保持 cwd 与 env 的平台等价语义
系统在执行依赖 shell 状态的命令时，必须在不同平台保持“可切换工作目录并注入环境变量”的等价语义。Windows 必须使用 `cd /d` 与 `set KEY=VALUE` 等等价机制；非 Windows 必须继续使用 `cd` 与 `export` 机制。

#### 场景:Windows 平台执行依赖 cwd/env 的命令
- **当** 运行环境为 Windows，且命令执行依赖已设置的工作目录或环境变量
- **那么** 系统必须在命令执行前正确应用 cwd 与 env，使后续命令可在同一执行上下文中读取这些状态

#### 场景:跨平台回归验证 cwd/env 语义
- **当** 团队在 Windows 与非 Windows 平台分别执行同一组依赖 cwd/env 的回归用例
- **那么** 两端必须都满足“目录切换成功且环境变量生效”的行为验收标准，而不是只在 POSIX 平台通过

### 需求:core 启动入口必须支持无配置的 `--self-test` 自检模式
系统必须提供 `bridgingio-core --self-test` 启动入口，用于在不提供 `--config`、`--runtime-root` 的前提下执行内建自检。自检至少必须覆盖 one-shot 执行、interactive shell 生命周期、runtime 执行链路，以及默认 MCP model-plane 监听地址 `127.0.0.1:19718` 的绑定检查，并通过进程退出码反映结果。

#### 场景:操作员运行 `--self-test`
- **当** 操作员执行 `bridgingio-core --self-test`
- **那么** 系统必须自动执行内建自检并输出清晰的通过/失败摘要；全部通过时返回退出码 `0`，任一检查失败时返回非零退出码

#### 场景:默认 MCP 监听地址可绑定
- **当** 操作员执行 `bridgingio-core --self-test`，且默认 model-plane 地址 `127.0.0.1:19718` 可成功绑定
- **那么** 自检必须将该项视为通过，并继续后续检查，而不是跳过该诊断

#### 场景:默认 MCP 监听地址绑定失败
- **当** 操作员执行 `bridgingio-core --self-test`，且默认 model-plane 地址 `127.0.0.1:19718` 无法绑定
- **那么** 自检必须打印包含目标 host/port 与底层绑定错误文本的失败信息，并以非零退出码结束

#### 场景:`--self-test` 与配置参数混用
- **当** 操作员同时传入 `--self-test` 与 `--config`、`--runtime-root` 或 `--control-plane-socket-override`
- **那么** 系统必须拒绝该参数组合并返回明确参数错误，而不是在含糊模式下继续启动

### 需求:core 必须通过宿主平台适配层管理本地运行时差异
BridgingIO core 必须通过明确的宿主平台适配层管理本地 shell/runtime、control-plane IPC、runtime path、toolchain 定位与输出采集等宿主平台差异。系统不得继续把这些差异零散散布在 provider、MCP、runtime 入口与测试中分别实现。

#### 场景:Windows 宿主连接 ADB target
- **当** BridgingIO core 运行在 Windows，且用户或 MCP 客户端访问一个 ADB target
- **那么** 系统必须使用 Windows 宿主平台适配层处理本地进程启动、路径、IPC 与输出采集，同时保持 ADB target 的远端 shell 语义，而不是把两者混为同一种平台判断

#### 场景:Unix 宿主连接 SSH target
- **当** BridgingIO core 运行在 macOS 或 Linux，且用户或 MCP 客户端访问一个 SSH target
- **那么** 系统必须通过 Unix 宿主平台适配层提供本地 shell/runtime 能力，并保持与既有 Unix 行为兼容，而不是要求业务层直接感知 `cfg(unix)` 细节

### 需求:target shell 方言必须独立于宿主平台建模
系统必须把 target shell 方言与宿主平台显式区分。target profile、connector 或 provider 必须能够表达远端使用的 shell 方言，而不能默认把宿主平台 shell 语义套用到所有 target。

#### 场景:Windows 宿主上的 ADB interactive shell
- **当** BridgingIO core 运行在 Windows，且客户端为 ADB target 打开 interactive shell
- **那么** 系统必须以 Windows 宿主适配层负责本地启动与 I/O，但在命令、prompt、cwd/env 等语义上遵循 ADB/Android shell 方言，而不是错误切换到本地 `cmd` 语义

#### 场景:未来通过 SSH 连接 Windows target
- **当** 未来某个 SSH target 被配置为 Windows 目标机，并声明远端 shell 为 `cmd` 或 PowerShell
- **那么** 系统必须允许该 target 使用自己的 shell 方言，而不是仅依据宿主平台决定远端命令拼接与状态语义

### 需求:本地 control-plane 必须提供平台原生 transport
受信任的本地 control-plane 必须通过平台原生 transport 访问 core。Unix 系统必须支持 Unix domain socket 或等价本地 IPC；Windows 必须支持 named pipe 或等价的本地 IPC。系统不得把“没有 Unix socket”视为 Windows 上 control-plane 的默认完成态。

#### 场景:Unix 平台启动 UI-managed core
- **当** bundled UI 在 Unix 宿主上拉起本地 core 并执行 attach
- **那么** 系统必须通过平台原生本地 IPC 完成 attach，而不是退化为与 model-plane 混用的公共 HTTP 入口

#### 场景:Windows 平台启动 UI-managed core
- **当** bundled UI 在 Windows 宿主上拉起本地 core 并执行 attach
- **那么** 系统必须提供可工作的本地 IPC transport，使 settings、sessions、approvals 与 diagnostics 能通过本地 control-plane 访问，而不是仅打印“当前平台无 Unix socket 支持”后继续运行

### 需求:runtime path 与生成配置默认值必须按宿主平台解析
系统必须按宿主平台解析 runtime root、data dir、artifact root、metadata path、control-plane endpoint 与相关默认值。自动生成配置、样例配置与 runtime bootstrap 禁止继续隐式依赖只在 Unix 上成立的默认路径或后缀。

#### 场景:UI-managed 模式生成运行配置
- **当** UI-managed 模式在不同宿主平台上首次为 runtime root 生成 core 配置
- **那么** 系统必须生成对当前宿主平台有效的 metadata path、artifact root 与 control-plane endpoint，而不是统一写入 Unix 风格路径或 `.sock` 端点

#### 场景:standalone 示例配置跨平台阅读
- **当** 操作员阅读或复制项目提供的 standalone 样例配置
- **那么** 示例中的默认值必须是平台中性占位语义或按宿主平台可工作的默认值，而不是继续默认 `/bin/sh`、`~/.bridgingio` 等只对部分平台天然成立的写法

### 需求:规格必须覆盖宿主平台与目标方言的最小组合矩阵
规格必须显式覆盖至少以下组合：`unix host + ssh-posix`、`windows host + adb-android-shell`、future `unix host + ssh-windows-cmd`。实现与测试计划不得只验证单维平台或单维方言。

#### 场景:当前主路径组合
- **当** 团队定义首轮可回归组合
- **那么** 规格必须至少包含 `unix host + ssh-posix` 与 `windows host + adb-android-shell`，并区分“宿主语义”与“目标方言语义”

#### 场景:future 组合保留
- **当** 团队记录 future `unix host + ssh-windows-cmd`
- **那么** 规格必须将其标记为“接口与诊断先行、行为后续补齐”，而不是默认为本轮完整交付

### 需求:future dialect 必须明确为接口保留范围而非首轮完整交付
对 `ssh-windows-cmd`、`ssh-powershell` 等 future dialect，首轮必须保留扩展接口与诊断语义，但不承诺完整命令、状态与 quoting 行为等价。

#### 场景:调用方选择 future dialect
- **当** target 明确声明 future dialect（例如 `ssh-powershell`）
- **那么** 系统必须返回受控的“未完整实现”语义（如 `unsupported/degraded/deferred`）并保留扩展接口，而不是伪装为已完全支持

### 需求:规格层必须固化关键架构决策并将开放问题收敛为有限集合
规格必须显式固化以下已拍板决策：Windows local transport 目标架构为 named pipe、Windows host shell 首版默认 `cmd`、文本输出首版采用平台默认编码并暴露解码诊断、structured invocation 为长期唯一执行真相源。开放问题必须收敛为有限集合，超出时通过新提案承接。

#### 场景:评审关键决策一致性
- **当** 评审 design、spec 与实现任务拆分的一致性
- **那么** 上述四项决策必须在规格层可直接追踪，且不得被实现细节回退为“继续多套真相源并存”

#### 场景:新增未决问题
- **当** 实现过程出现新的跨平台重大不确定项
- **那么** 团队必须将其并入有限 open questions 或拆分新提案，避免在当前 change 中无限扩张未决范围

### 需求:终端型 target 的会话与通道并发必须服从其并发策略
对于 terminal family 成员，系统必须依据 target 声明的并发策略管理 logical session、transport session 与 channel。系统禁止继续假定所有终端型目标都具有与 SSH 相同的多窗口并发特性。

#### 场景:SSH 或 ADB target 允许多窗口并发
- **当** 一个 `ssh` 或 `adb` target 被声明为 `multiplexed`，且同一访问作用域内已经存在一个活动逻辑会话
- **那么** 系统必须允许该逻辑会话下继续打开多个并发 channel 或窗口，并保持它们共享逻辑会话审计关系但彼此隔离 shell 状态

#### 场景:exclusive terminal target 阻止第二个活动访问
- **当** 一个 terminal target 被声明为 `exclusive`，且该 target 已存在活动 transport 或活动交互 channel
- **那么** 系统必须拒绝新的并发访问并返回明确的 lease/busy 语义，而不是静默创建第二个会话或第二个窗口

### 需求:独占型终端 target 必须以 target 级 lease 管理跨作用域竞争
对于 `exclusive` terminal target，系统必须把独占约束提升到 target 级别，而不是只在单个逻辑会话内生效。不同 agent、不同 run、不同 thread 或不同客户端会话访问同一独占 target 时，系统必须显式处理 lease 冲突。

#### 场景:另一个 agent 访问已占用的 serial target
- **当** agent A 已经持有某个 `exclusive` terminal target 的活动 transport，而 agent B 在不同访问作用域下尝试访问同一 target
- **那么** 系统必须返回明确的冲突或占用状态，并禁止复用或并行建立第二个活动 transport

#### 场景:独占 lease 释放后允许下一次访问
- **当** 持有独占 lease 的 transport 或交互 channel 已关闭
- **那么** 系统必须允许后续请求重新获取该 target 的访问权，而不是把该 target 永久标记为不可用

### 需求:future localshell target 必须沿用统一的 target/session/audit 语义
future `localshell` target 一旦进入 terminal family，系统必须按普通 target 处理其 profile、logical session、transport session、channel、artifact 与 approval 关联。系统禁止因为其底层执行依赖宿主 runtime，就把它降级为不进入 target/session 模型的本地快捷路径。

#### 场景:future localshell target 打开 interactive shell
- **当** 用户或 AI 为 future `localshell` target 打开 interactive shell
- **那么** 系统必须为其创建与 SSH / ADB 一致的 logical session、transport session 与 channel 记录，并将 transcript 与 artifact 关联到该 target，而不是只保留宿主 shell 进程状态

#### 场景:宿主诊断与 target 会话保持边界
- **当** 系统同时暴露宿主 `HostPlatformAdapter` 诊断与 future `localshell` target 会话信息
- **那么** 宿主平台诊断必须继续按 host scope 输出，而 `localshell` target 的命令、审批与 artifact 必须继续按 target/session scope 输出，二者不得混为同一条数据边界

### 需求:target 运行时索引必须保留 canonical 标识与完整可解析引用
系统必须为 MCP target 解析维护一份运行时可消费的 target descriptor/index。该索引必须至少保留 canonical target id、enabled、display name、全部 aliases、kind、notes 与连接摘要，而不是仅保留可执行所需的瘦身 profile 视图。

#### 场景:target 配置包含多个 aliases
- **当** 某个 target 在配置中声明了多个 aliases
- **那么** 系统必须在运行时索引中保留全部 aliases，使它们都可以参与解析与确认候选展示，而不是只保留第一条 alias

#### 场景:disabled target 不参与可执行解析
- **当** 某个 target 在配置中被标记为 `enabled = false`
- **那么** 该 target 必须不参与 MCP 的可执行 target 解析与候选确认结果，而不是继续像正常 target 一样被自动命中

### 需求:target 引用解析必须区分低风险自动命中与高风险相关候选
系统必须将 target 引用解析拆分为低风险自动命中与高风险候选发现两个阶段。低风险阶段必须支持大小写与 `-` / `_` / 空格差异等宽松归一化；高风险阶段必须处理 family-related 与 typo-related 候选，并禁止默认自动执行这些高风险推断。

#### 场景:低风险归一化唯一命中
- **当** 用户输入 `test_device`，系统中存在唯一 target `test-device`
- **那么** 系统必须将该输入视为低风险归一化命中并解析到 canonical target `test-device`

#### 场景:family-related 候选使用边界化前缀规则
- **当** 用户输入 `test`，系统中存在 `test`、`test-1`、`test_2`、`test 3`、`testlab` 与 `testcase`
- **那么** 系统必须只把 `test-1`、`test_2` 与 `test 3` 视为 `test` 的 family-related 候选，而禁止把 `testlab` 或 `testcase` 误判为同 family 设备

### 需求:MCP target resolution policy 必须可配置并默认对 family 歧义二次确认
core-owned settings 必须允许操作员配置 MCP target resolution policy，以决定“存在精确命中时是否仍需确认”。系统必须至少支持 `auto_execute`、`confirm_if_family`、`confirm_if_related` 与 `confirm_always` 四种策略，并且默认值必须为 `confirm_if_family`。

#### 场景:默认策略在 family 歧义时返回确认
- **当** 操作员未显式覆盖 target resolution policy，且用户输入 `test`，系统中同时存在 `test`、`test-1` 与 `test-2`
- **那么** 系统必须按默认策略 `confirm_if_family` 返回确认结果，而不是直接执行精确命中的 `test`

#### 场景:策略设为 auto_execute 时精确命中直接执行
- **当** 操作员将 MCP target resolution policy 配置为 `auto_execute`，且用户输入 `test`，系统中同时存在 `test`、`test-1` 与 `test-2`
- **那么** 系统必须直接将 `test` 解析为 canonical target 并继续执行，而不是仍然返回确认结果

### 需求:bundled host 的生命周期发现必须基于 runtime root 稳定身份
在 bundled 默认产品路径下，宿主必须使用由 runtime root 推导出的稳定 control-plane endpoint、instance metadata 或等价稳定身份来发现现有 managed core。系统禁止把仅随单次 UI 进程变化的临时 endpoint 作为正常产品路径下唯一的实例发现依据。

#### 场景:同一 runtime root 上重启 bundled UI
- **当** 用户在同一 runtime root 上重新启动 bundled UI
- **那么** 宿主必须仍能发现上一次遗留或仍在运行的 managed core，而不是因为本次 UI 会话生成了新的临时 endpoint 就把旧实例视为不可见

#### 场景:存在残留实例但当前 UI 会话标识已变化
- **当** 上一次 UI 会话异常退出，新的 UI 会话在同一 runtime root 上重新启动
- **那么** 系统必须仍能基于稳定身份发现旧 core，并进入后续的 probe / reconcile 流程，而不是直接把残留实例留到新的 bind 冲突阶段才暴露

### 需求:bundled host 在启动新的 ephemeral core 之前必须协调残留实例
在 `ui-managed-ephemeral` 默认模式下，宿主在拉起新的 managed core 之前，必须先协调同一 runtime root 下可能残留的旧实例。只要旧实例仍响应或仍占用本地 transport/model-plane 资源，宿主就必须先完成回收或进入显式冲突状态，而不是直接继续启动另一个 core。

#### 场景:启动前发现可响应的 orphan core
- **当** bundled host 在同一 runtime root 下发现一个仍可通过本地 control-plane probe 的 `ui-managed-ephemeral` core，但当前 UI 会话并不是原始 owner
- **那么** 宿主必须先请求该实例关闭、等待其退出并确认本地 endpoint 与 model-plane 监听资源释放后，才能启动新的 core

#### 场景:启动前发现不可响应的残留发现线索
- **当** 宿主在 runtime root 下发现残留的 endpoint、instance metadata 或等价发现线索，但 probe 无法连通且不存在可确认的活动 core
- **那么** 宿主必须把这些线索视为 stale artifact 并先完成清理，再进入新的启动流程，而不是直接把后续 bind 失败暴露为通用启动错误

#### 场景:启动前发现不可自动回收的活动实例
- **当** 宿主在同一 runtime root 下发现一个活动实例，但该实例不满足自动回收策略或 ownership 策略要求人工介入
- **那么** 系统必须进入显式冲突状态并返回恢复指引，而不是静默杀掉旧实例或继续启动第二个 core

### 需求:本地 control-plane 必须提供启动前实例探测与 ownership 摘要
受信任的本地 control-plane 除了 attach 与 bootstrap 语义外，还必须允许宿主在 attach 之前探测已有实例，并读取最小 ownership 摘要。该摘要至少必须覆盖 `core_instance_id`、host/ownership mode、readiness state、runtime root 标识，以及当前 attached owner 的摘要或空值。

#### 场景:宿主在 attach 前探测已有实例
- **当** bundled host 在启动新 core 之前对一个已发现的本地实例执行 probe
- **那么** control-plane 必须返回足以支持 reconcile 决策的实例摘要，而不是要求宿主先完成 attach 才能知道该实例是谁、处于什么状态

#### 场景:attach 遇到 owner 冲突
- **当** 一个新的 UI 会话尝试 attach 到已被其他 owner 占据的本地 core
- **那么** 系统必须返回显式的 ownership conflict 或等价结构化语义，并包含足够的诊断信息，而不是只返回无法区分原因的通用校验失败

### 需求:SSH broker endpoint 必须是连接级本地资源并具备自动清理语义
当 target session 使用 vault 管理的 SSH 私钥时，系统必须把对应的 broker endpoint 建模为连接级或逻辑会话级的本地资源，而不是长驻全局代理。该 endpoint 必须具备显式状态、引用计数或等价的附着关系，以及在 session 结束、失败或超时后的自动清理语义。系统不得把“仅分配 endpoint locator 但尚未真实 bind/listen”的状态视为 broker 已 ready。

#### 场景:one-shot SSH 调用创建真实临时 broker
- **当** 某次 one-shot SSH 调用需要使用 vault 中的私钥
- **那么** 系统必须只为该次调用创建临时 broker endpoint
- **并且** 只有在该 endpoint 已真实 bind/listen 并可接受请求后，系统才可以把其下发给 SSH 客户端
- **并且** 在调用结束后系统必须及时清理 endpoint、内存 signer material 与任何 fallback 文件

#### 场景:interactive shell 结束后清理 broker
- **当** 某个 interactive shell 或逻辑 target session 结束，且不再有活动 channel 依赖该 SSH broker
- **那么** 系统必须将该 broker 进入清理流程
- **并且** 必须关闭其本地 endpoint 与相关 runtime handle
- **并且** 禁止后续未重新授权的会话继续复用它

#### 场景:broker bind 失败不得伪装为 ready
- **当** 某个 vault-backed SSH broker session 在平台 endpoint bind、listen 或 adapter startup 阶段失败
- **那么** 系统必须返回受控的 broker startup failure 或等价结构化失败
- **并且** 不得把一个尚未实际就绪的 endpoint locator 提前交给 SSH 客户端

### 需求:secret-backed SSH 启动必须优先使用结构化执行覆盖层
当 SSH 连接依赖 vault broker、managed password delivery、`SSH_AUTH_SOCK`、`IdentityAgent`、`SSH_ASKPASS` 或等价本地认证 carrier 时，系统必须优先通过结构化的 `program + args + env overlay` 或等价 direct exec 机制把这些信息传递给 SSH 进程，而不是通过 host shell 字符串拼接、全局环境污染或 shell rc 注入来完成。对于 managed 路径，系统必须确保 SSH 进程只看见当前 invocation 对应的 carrier，而不是宿主原有 agent / askpass 环境。

#### 场景:通过结构化 overlay 注入当前 broker endpoint
- **当** 某个平台使用 `SSH_AUTH_SOCK`、`IdentityAgent` 或等价环境变量把临时 broker endpoint 交给 SSH
- **那么** 系统必须把该信息限制在本次 SSH 子进程或等价结构化执行上下文中
- **并且** 不得要求用户 shell 或通用宿主环境长期持有该变量或 locator

#### 场景:通过结构化 overlay 注入 password askpass carrier
- **当** 某次 SSH 调用依赖 askpass helper、`SSH_ASKPASS`、`SSH_ASKPASS_REQUIRE` 或等价 password carrier
- **那么** 系统必须通过结构化 env overlay 把这些信息限制在当前 invocation
- **并且** 不得把 password 或 helper payload 直接拼进 host shell 命令字符串

#### 场景:debug/trace SSH probe 必须产出可比较的上下文摘要
- **当** menuconfig `Test Connection` 在 debug 或 trace 级别执行 SSH probe
- **那么** 系统必须在 verbose 日志中记录 display-safe 的 probe 上下文摘要（至少包括 delivery plan、env overlay 键名、关键 `-o` 覆盖项与 pre-spawn carrier 状态）
- **并且** 该摘要不得包含 password 明文、私钥明文或 helper payload 内容

#### 场景:当前 runtime 仅支持 shell flatten
- **当** 某个 connector 的当前 runtime 仍只能把调用 flatten 为 host shell 字符串
- **那么** 系统必须为 secret-backed SSH 选择 direct exec wrapper 或受控失败
- **并且** 不得把 broker / askpass 注入信息直接拼进可被日志、transcript 或 shell history 捕获的命令字符串

### 需求:standalone 配置必须声明 vault / unlock 策略而不保存敏感明文
对于 standalone 模式，core-owned 配置必须允许操作员声明 vault backend、namespace、unlock policy、以及 SSH 私钥交付相关策略，但不得在 TOML 或等价配置文件中保存 secret 明文。standalone 配置只允许保存非敏感策略字段、路径、模式、`CredentialRef`，以及诸如 `trigger_policy`、`allowed_methods`、`preferred_method`、`cache_ttl_sec` 这类 unlock 策略参数。

#### 场景:内网 standalone 部署声明 vault 策略
- **当** 操作员在内网服务器上为 standalone core 配置 vault backend、unlock mode 与 model-plane auth 策略
- **那么** 配置文件必须能够表达这些策略，但不得要求操作员把 vault passphrase、SSH 私钥或 agent token 明文写入配置文件

#### 场景:standalone 配置声明 SSH key delivery 策略
- **当** 操作员为 standalone core 配置 SSH target，且需要声明默认的 SSH 私钥交付模式
- **那么** 配置文件必须能够表达诸如 `ssh-agent broker` 或受控 fallback 这类策略选择，但不得直接包含私钥内容

#### 场景:standalone 配置声明启动即锁定
- **当** 操作员为 standalone core 配置 `trigger_policy = on-core-start`，并声明 `passphrase` 或其他允许的本地解锁方法
- **那么** 配置文件必须能够表达这一 unlock 策略，但不得要求把实际 passphrase 或其他明文解锁材料写入配置

### 需求:standalone 的 vault / auth 管理入口必须与 `run` 分离并避免明文 argv
standalone 模式下，系统必须为 vault 初始化、SSH key 导入、vault 解锁、agent token 创建和 revoke 等管理动作提供显式的本地管理入口，例如独立 CLI 子命令、menuconfig 安全管理页或等价的受信任本地 control-plane 动作。该管理入口必须支持通过受控本地文件读取、prompt 或等价 secret route 传递 SSH key 材料，并且不得要求操作员通过命令行参数直接传入 secret 明文。

#### 场景:操作员在 menuconfig 中导入 SSH 私钥
- **当** 操作员在 standalone 环境的 menuconfig 中为某个 target 准备导入 SSH 私钥
- **那么** 系统必须允许其通过受控本地导入流程完成该操作，而不能要求其切换到“把私钥内容粘到普通文本字段”或 `--private-key <plaintext>` 之类的 argv 明文方式

#### 场景:target 绑定已导入 key 后保存配置
- **当** 操作员为 SSH target 选中了某个已导入 vault key 并保存配置
- **那么** standalone 配置文件中必须只保存 canonical `credential_ref`
- **并且** 不得因为 target 绑定流程而把私钥明文、passphrase 或其他 secret material 写入配置文件

### 需求:standalone secret 输入路线必须具备确定优先级与审计语义
系统必须为 `stdin`、`file`、`fd`、`tty prompt` 四类 secret 输入路线定义确定优先级与冲突处理语义，避免多输入源并存时的歧义。推荐优先级为 `fd > stdin > file > tty prompt`。若一次管理动作显式声明了多个输入源，系统必须 fail closed 并返回可诊断错误。

#### 场景:多输入源冲突时拒绝执行
- **当** 操作员在同一次 secret 导入或 vault 解锁动作中同时显式提供多个输入源（例如 `--from-fd` 与 `--from-file`）
- **那么** 系统必须拒绝该请求并提示冲突输入源，而不能静默猜测使用哪个来源

#### 场景:记录输入源审计而不泄露 secret
- **当** 系统接收并处理一次 standalone secret 输入
- **那么** 审计事件必须记录 `source_kind`、`intent_id`、operator、字节长度与来源摘要等最小字段，同时不得记录 secret 明文、完整输入路径、完整 fd 值或 prompt 原文

### 需求:standalone 后续必须允许声明 passphrase protector 的策略参数而不暴露口令本身
对于支持备份恢复与迁移的 standalone 部署，后续配置与管理入口必须允许声明 `passphrase` protector 的策略，例如 KDF 方案、成本等级、是否启用 recovery protector，但不得在配置文件、argv、日志或常规脚本输出中暴露 passphrase 明文。

#### 场景:standalone 初始化可恢复 vault
- **当** 操作员在 standalone 场景下初始化一个支持备份恢复的 vault，并选择启用 `passphrase` protector
- **那么** 系统必须允许其声明 `Argon2id` 或等价 memory-hard KDF 的策略参数，但不得要求把实际 passphrase 写入 TOML、普通环境变量快照或命令行字面量参数

### 需求:standalone 后续必须允许声明常规解锁触发点与方法偏好
对于 standalone 的 future route，系统必须允许操作员声明 vault 是否在 core 启动时、首次 secret access 时或每次 secret access 时要求本地解锁，并允许为这些触发点声明方法偏好，例如 `passphrase-only`、`passkey-preferred` 或等价策略。

#### 场景:standalone 声明首次访问 secret 时再解锁
- **当** 操作员为 standalone core 配置 `trigger_policy = on-first-secret-access`
- **那么** 系统必须允许 core 在 locked 状态下先启动，并在第一次 secret-backed 操作到来时再要求完成允许的本地解锁

### 需求:standalone 的本地管理面后续必须只暴露规范化引用与安全投影
即使在 standalone 模式下，后续本地管理入口也必须把 vault / token 的内部 record 与展示结果分离。`vault list`、`auth token list`、诊断页或等价管理界面只能展示 canonical `CredentialRef`、token summary、状态与策略摘要，而不能把 secret 明文、token 明文、`token_hash`、`ciphertext_locator` 或等价内部字段暴露给操作员界面、日志或脚本输出。

#### 场景:standalone 列出已导入的 secret
- **当** 操作员通过未来的 standalone 管理入口列出当前 vault 中的 secret
- **那么** 系统只能返回 canonical `CredentialRef`、label、kind、status 与 rotation 摘要，而不能把密文定位信息或 secret 内容输出到命令结果

#### 场景:standalone 创建 token 后再次查询
- **当** 操作员通过未来的 standalone 管理入口创建了一个 agent token，并在稍后再次查看该 token
- **那么** 首次签发后系统最多只允许一次性显示明文 token；此后所有查询都只能返回安全投影摘要

### 需求:内网 / headless standalone 必须提供最小操作手册与安全默认值建议
系统必须为内网 / headless 部署提供最小可执行的操作手册与安全默认值建议，覆盖 vault 初始化、secret 导入、解锁、token 签发与轮换撤销的顺序化流程，并明确禁止把 secret 明文写入 TOML、argv、常规日志或脚本输出。

#### 场景:按推荐默认值初始化 headless 部署
- **当** 操作员首次部署 headless standalone core 并采用推荐默认值
- **那么** 系统文档必须至少推荐启用 `model_plane.http.auth.mode = bearer`、非 loopback 强制认证、`ssh-agent broker` 优先交付与最小权限 token 签发策略

#### 场景:按操作手册执行周期安全维护
- **当** 操作员按系统提供的内网 / headless 操作手册执行日常维护
- **那么** 手册必须包含 token 轮换 / 撤销、vault protector 状态检查与 KDF 参数基线复核步骤，且默认流程不得要求明文 secret 出现在 argv

### 需求:`--self-test` 必须覆盖本次变更已落地的 vault / auth contract smoke
对于 `design-vault-and-agent-auth` 已在 core 中落地的能力，`bridgingio-core --self-test` 必须提供无配置 smoke 入口，验证 canonical `CredentialRef`、vault fail-closed、broker-only secret use、本地管理员验证单次消费、secret-backed SSH delivery，以及 non-loopback model-plane 安全默认值。该 smoke 必须继续通过进程退出码反映结果。

#### 场景:操作员运行包含 vault / auth smoke 的 `--self-test`
- **当** 操作员执行 `bridgingio-core --self-test`
- **那么** 系统必须在原有 shell / runtime smoke 之外，继续执行本次变更的 vault / auth contract smoke；任一子项失败时返回非零退出码

#### 场景:当前宿主验证 secret-backed SSH delivery
- **当** 操作员执行 `bridgingio-core --self-test`，且 self-test 进入 secret-backed SSH delivery smoke
- **那么** 系统必须根据当前宿主平台验证 broker endpoint lifecycle 或受控 fallback 诊断，并确认 session cleanup 语义仍成立

### 需求:本地 control-plane 必须提供 agent token 管理接口
受信任的本地 control-plane app API 必须为 bundled UI 与 future standalone 管理入口提供统一的 agent token 管理能力，至少覆盖 `create`、`list` 与 `revoke`。这些接口必须由 core 直接持有其生命周期与持久化真相，而不是要求 UI 自行生成、缓存或解释 token。

#### 场景:UI 通过 IPC 创建 token
- **当** 本地 UI 通过受信任 IPC 请求 core 创建一个 agent token，并提交备注名称与生命周期配置
- **那么** core 必须自行生成 opaque token、分配稳定的 `token_id` / `principal_id`、持久化 hash-only record，并返回可供 UI 展示的一次性签发结果

#### 场景:本地管理面列出现有 token
- **当** 本地 UI 或 future standalone 管理入口通过 control-plane 请求列出当前 token
- **那么** core 必须返回 token 的安全摘要列表，而不能要求前端自行拼装 token 状态或从其他接口侧推导生命周期

#### 场景:用户删除 token
- **当** 本地 UI 或 future standalone 管理入口请求删除一个 token
- **那么** core 必须将该动作建模为 revoke，并返回更新后的安全摘要或等价状态结果，而不是把该 token 作为物理记录直接静默删除

### 需求:本地 token 签发响应必须一次性返回明文，后续查询只返回安全摘要
通过本地受信任 control-plane 创建 token 时，系统必须只在签发成功的那一次响应中返回明文 token。后续的 `list`、`get`、诊断或等价管理查询都只能返回安全摘要，而不得再次展示明文 token、`token_hash` 或等价内部认证真相。

#### 场景:首次签发 token 后立即返回明文
- **当** 本地管理面成功创建一个新的 agent token
- **那么** 系统必须在该次 create 响应中一次性返回明文 token 与安全摘要，使调用方能够提示用户立刻保存，而不是要求后续再调用 reveal 接口取回明文

#### 场景:稍后再次查看同一个 token
- **当** 本地管理面在 token 创建完成后再次请求查看该 token 的状态
- **那么** 系统必须只返回诸如 `token_id`、label、status、scope 摘要、创建时间和过期时间这类 display-safe 字段，而不能再次返回明文 token 或内部 hash 字段

### 需求:本地 control-plane token 管理接口必须为后续 scope 更新保持兼容扩展位
即使首版只实现 `create`、`list` 与 `revoke`，本地 control-plane token 管理接口仍必须为后续 `update-scope` 或等价权限管理动作预留兼容扩展位，避免未来新增 scope 更新能力时破坏已发布的 IPC 契约。

#### 场景:后续版本增加 scope 更新命令
- **当** 后续版本为本地 token 管理面增加 `update-scope` 或等价权限更新能力
- **那么** 系统必须能够在不推翻既有 create/list/revoke 基本响应模型的前提下扩展该能力，而不是要求 bundled UI 与 future standalone 管理入口全部重做 token 管理协议

### 需求:bundled 桌面管理面必须独立于 model-plane HTTP 监听工作
在 bundled 桌面宿主形态下，本地管理面必须通过受信任 control-plane 与 core 交互，而不是依赖 model-plane HTTP 监听作为自己的管理通道。即使用户修改了 model-plane host/port 并触发重启，宿主也必须继续通过本地 control-plane 呈现生命周期状态与恢复路径。

#### 场景:用户保存新的 model-plane 端口
- **当** 用户在桌面管理面中保存新的 model-plane host/port，并且该变更需要重启 core
- **那么** 宿主必须继续通过本地 control-plane 展示 `saving / restarting / connected / failed` 等状态，而不是要求 UI 通过浏览器跳转或额外管理端口才能继续工作

#### 场景:model-plane 旧端口已失效但管理面仍可恢复
- **当** 旧 model-plane 监听地址已被关闭，且 replacement core 仍在启动或 attach
- **那么** 桌面管理面必须仍可展示重启进度、诊断与恢复入口，而不是因为旧端口失效就丢失主界面

### 需求:本地 control-plane 必须提供 timeline 来源分组摘要
受信任的本地 control-plane 在向桌面控制台返回 timeline 或等价审计数据时，必须同时提供适合 UI 做来源分组的安全摘要字段。该摘要至少必须支持“有 token label 的认证请求”和“无 token 的指纹 / user-agent 来源请求”两类分组主键。

#### 场景:桌面 UI 首次读取 timeline
- **当** 桌面 UI attach 成功后请求首屏 timeline 数据
- **那么** control-plane 必须返回每条活动所属的来源分组摘要，使 UI 能直接按来源分组渲染，而不是要求 UI 自己猜测如何把事件归并

#### 场景:后续增量时间线更新
- **当** 桌面 UI 继续读取 timeline 增量或事件流
- **那么** 系统必须继续为新增活动附带一致的来源分组摘要，而不是让同一来源在不同读取批次中失去可追踪的归属关系

### 需求:target 配置必须区分 public descriptor 与 sealed overlay
实现 canonical vault runtime 后，target 配置必须支持把公开 inventory 与敏感连接配置拆成两层：`config.toml` 继续承载 plain target 与 sealed target 的 public descriptor，vault 负责 sealed overlay、credential 关联和其他高敏字段。系统不得继续把“是否在 vault 中存储”与“是否需要 token 才能访问”混成同一个开关。

#### 场景:读取 legacy plain target 配置
- **当** standalone core 读取到未声明安全分层的 legacy target 配置
- **那么** 系统必须将其兼容映射为 `storage_class = plain` 且 `access_class = anonymous-local`，以保持旧版本 loopback 直连能力

#### 场景:高敏 target 使用 sealed overlay
- **当** 操作员将某个 target 标记为高敏或创建需要 vault credential 的 target
- **那么** 系统必须允许仅把最小 public descriptor 保留在 `config.toml`，并把 host、selector、notes、policy、`credential_ref` 等敏感字段保存到 vault overlay

### 需求:sealed target 在未解锁时必须返回受限描述而不是完整连接摘要
target 运行时索引在支持 sealed target 后，必须允许对未解锁 target 返回 redacted descriptor。系统不得要求所有 target 在任意时刻都暴露完整连接摘要；对于依赖 vault overlay 的 target，未解锁时只需返回足以识别 target 的最小 public 信息。

#### 场景:未解锁时列出 sealed target
- **当** vault 尚未解锁，且某个 target 的连接配置依赖 sealed overlay
- **那么** 系统必须至少返回该 target 的 canonical id、display name、aliases、kind、enabled 和安全状态摘要，但不得暴露完整 host、username、selector 或等价敏感连接信息

#### 场景:解锁后校验 public descriptor
- **当** vault 从 locked 进入 unlocked，且某个 sealed target 的 public descriptor 位于 vault 外层
- **那么** 系统必须允许使用 vault 内绑定的 digest 或 manifest 校验该 descriptor 的完整性，并在发现不一致时返回明确的 tamper diagnostics

### 需求:sensitive target 的 public descriptor digest 必须只覆盖公开字段
当系统为 sensitive target 计算 public descriptor digest 时，摘要必须仅覆盖 `id`、`display_name`、`aliases`、`kind`、`enabled`、`storage_class`、`access_class` 与 `sealed_profile_ref` 等公开字段。系统禁止把 `notes`、连接参数、credential 引用或等价 sensitive overlay 字段混入 public descriptor digest。

#### 场景:仅修改 sensitive overlay 字段
- **当** 操作员仅修改某个 sensitive target 的 `notes`、连接参数、policy 或 `credential_ref`
- **那么** 系统不得把该修改视为 public descriptor 漂移
- **并且** public descriptor digest 必须保持稳定

#### 场景:修改 public descriptor 字段
- **当** 操作员修改某个 sensitive target 的 `display_name`、`aliases` 或 `enabled`
- **那么** 系统必须更新该 target 的 public descriptor digest，并使后续 public cache 与 vault authoritative descriptor 保持一致

### 需求:vault unlock 后必须执行 sensitive target reconcile
当 vault 从 `locked` 进入 `unlocked` 时，系统必须执行一次正式的 sensitive target reconcile。该 reconcile 必须重新读取 vault authoritative target-profile、校验 public descriptor，并修复或刷新 `config.toml` 中的 sensitive target public cache。

#### 场景:unlock 后修复缺失的 public cache
- **当** vault 解锁成功，且某个 sensitive target 在 `config.toml` 中的 public cache 缺失
- **那么** 系统必须从 vault authoritative target-profile 重新生成该 public cache，并使该 target 重新出现在正式 catalog 中

#### 场景:unlock 后修复过期的 public cache
- **当** vault 解锁成功，且某个 sensitive target 在 `config.toml` 中的 public cache 与 vault authoritative public descriptor 不一致
- **那么** 系统必须以 vault authoritative public descriptor 为准刷新该 public cache，而不得继续信任陈旧外层配置

### 需求:sensitive target projection 必须提供 locked/resolved/repair-needed/tamper 正式语义
target catalog 在 sensitive target 路径必须提供正式 projection state 语义：`locked`、`resolved`、`repair-needed` 与 `tamper`。其中 locked 状态下只允许返回 public cache。

#### 场景:未解锁时列出 sensitive target
- **当** vault 尚未解锁，且某个 target 的真相位于 vault authoritative `target-profile`
- **那么** 系统必须至少返回该 target 的 canonical id、display name、aliases、kind、enabled 与安全状态摘要
- **并且** 不得暴露完整 host、username、selector、notes 或等价 sensitive overlay 字段

#### 场景:解锁后发现 public descriptor digest 不匹配
- **当** vault 从 `locked` 进入 `unlocked`，且某个 sensitive target 的 public descriptor digest 与 authoritative descriptor 不匹配
- **那么** 系统必须返回明确的 `tamper` 或 `repair-needed` diagnostics，而不得继续把该 target 视为正常 resolved target

### 需求:standalone 配置必须规范化到 canonical vault 结构
standalone 的 core-owned 配置在实现 canonical vault runtime 后，必须能够表达 `builtin-encrypted`、`vault.unlock`、`vault.protectors` 和 `vault.ssh` 语义。legacy `[vault] backend = "os-native"` 只能作为兼容输入读取，不得继续作为规范化回写格式。

#### 场景:读取 legacy standalone 配置
- **当** standalone core 读取到 legacy `[vault] backend = "os-native"` 配置
- **那么** 系统必须在运行时将其归一化为 canonical vault + `os-native` protector 语义，并在后续持久化时回写 canonical 结构

#### 场景:standalone 配置声明 unlock policy
- **当** 操作员为 standalone core 配置 `trigger_policy`、`allowed_methods`、`preferred_method` 与 `cache_ttl_sec`
- **那么** 系统必须按这些字段驱动真实 vault lock/unlock 行为，而不是把它们当作未生效的文档字段

### 需求:standalone 的 vault 与 auth 管理入口必须复用 shared runtime truth
standalone 模式下的 `vault init/import/unlock` 与 `auth token create/revoke` 管理入口必须调用与桌面控制台相同的 canonical vault runtime 和 token authority，而不是维护独立的 CLI 私有逻辑。

#### 场景:standalone 解锁 vault
- **当** 操作员通过 standalone 管理入口执行 `vault unlock`
- **那么** 系统必须更新与桌面控制台共享的 vault lock state、protector 状态与审计真相，而不是只在 CLI 进程内临时记录一个解锁标记

#### 场景:standalone 创建长期 token
- **当** 操作员通过 standalone 管理入口执行 `auth token create`
- **那么** 系统必须复用 shared token authority、attestation enforcement 与 one-time reveal 语义，而不是实现另一套独立 token 存储或签发流程

### 需求:core 必须允许通过 `menuconfig` 管理同一套设置真相
BridgingIO 除了允许通过配置文件管理设置外，还必须允许通过 `bridgingio-core menuconfig` 管理同一套设置真相。该交互式配置面必须与配置文件、control-plane 和 future UI 共享同一套校验与持久化语义，而不是只服务 standalone。

#### 场景:用户通过 `menuconfig` 修改设置
- **当** 操作员在 `bridgingio-core menuconfig` 中修改某个 profile、toolchain、model-plane 或 storage 配置
- **那么** 系统必须对该修改应用与配置文件、control-plane 相同的校验与持久化规则，而不是由 TUI 维护一套私有逻辑

#### 场景:修改需要重启才能生效
- **当** 操作员通过 `menuconfig` 保存了一个需要重启或重新初始化才能生效的配置项
- **那么** 系统必须返回正式的 apply 结果或等价状态，而不是让 TUI 自行猜测修改是否立即生效

#### 场景:Target 列表与编辑入口遵循一致的 menuconfig 视觉语义
- **当** 操作员在 Targets 菜单浏览或进入某个 target
- **那么** 系统必须在 target 入口行使用可识别的状态标记（如 `< >` / `<*>`）与 `--->` 导航提示
- **并且当** 操作员编辑 target 字段
- **那么** 系统必须沿用统一弹窗字段样式 `Label (value) --->` 与统一开关标记 `[ ]` / `[*]`

### 需求:standalone `run` 与 `-d` 必须属于同一模式家族但允许不同解锁策略
BridgingIO 必须把 standalone 前台 `run` 与后台 `-d` 视为同一 standalone 模式家族下的两个子模式。两者必须共享相同的 runtime/config 真相与基本生命周期模型，但在启动解锁策略上允许受控差异。

#### 场景:前台模式遵从配置中的 trigger policy
- **当** 操作员使用 standalone `run` 启动 core
- **那么** 系统必须继续按配置中的 trigger policy 决定何时要求完成 vault 解锁，而不是无条件改写为启动即解锁

### 需求:standalone 后台模式必须覆盖 trigger policy 为 `on-core-start`
当 standalone 以后台/脱离式子模式启动时，系统必须忽略配置中的常规 trigger policy，并以安全优先的方式强制使用 `on-core-start`。该 override 必须被视为正式 lifecycle 规则，而不是临时实现细节。

#### 场景:配置声明 `on-first-secret-access` 但操作员使用 `-d`
- **当** 操作员以 `-d` 启动 standalone core，且配置中的 vault trigger policy 为 `on-first-secret-access`、`manual-only` 或其他非启动即解锁策略
- **那么** 系统必须覆盖为 `on-core-start`，并在完成解锁前保持该后台实例未就绪或 fail-closed

#### 场景:后台模式未能在启动阶段完成解锁
- **当** standalone `-d` 模式在启动阶段未能通过允许的安全 carrier 成功解锁 vault
- **那么** 系统必须保持 locked/unavailable 状态并返回明确启动失败或受控未就绪语义，而不能在后台默默等待未来某次 secret access 时再尝试解锁

### 需求:standalone 默认 runtime root 与配置路径必须统一到用户目录 `.bridgingio`
BridgingIO 在 standalone 模式下，未显式提供 `--config` 时，必须把用户目录下的 `.bridgingio` 作为 canonical 默认 runtime root，并从该目录解析或创建默认配置文件。系统不得继续要求 standalone 只能通过显式 `--config` 启动。

#### 场景:未提供 `--config` 启动 standalone
- **当** 操作员以 standalone `run` 或 `-d` 模式启动 core，且未传入 `--config`
- **那么** 系统必须从用户目录下的 `.bridgingio` 解析 runtime root、保留目录和默认配置，而不是直接报缺少 `--config`

#### 场景:显式提供 `--config`
- **当** 操作员为 standalone 启动显式传入 `--config <path>`
- **那么** 系统必须使用该配置路径作为覆盖入口，并优先采用该配置而不是默认 `.bridgingio` 路径

### 需求:runtime/config bootstrap 必须提供共享生命周期状态与恢复动作
BridgingIO 必须把 runtime root 解析、可写性校验、配置 load-or-create、迁移、修复与受控失败纳入共享生命周期状态机，并向本地 control-plane、TUI 与 future UI 返回明确状态和恢复动作。

#### 场景:默认 runtime root 不可写
- **当** core 在解析 standalone 默认 `.bridgingio` runtime root 后发现该目录不存在、不可写或保留子目录校验失败
- **那么** 系统必须进入明确的恢复状态，并返回诸如重新选择目录、修复权限或重试等恢复动作，而不是静默退回到随机临时目录

#### 场景:配置需要迁移
- **当** core 在启动阶段检测到默认配置或显式配置需要迁移、修复或版本升级
- **那么** 系统必须返回明确的 lifecycle 状态和迁移提示，而不是在未告知调用方的情况下继续使用不兼容配置

### 需求:standalone 生命周期与 control-plane 错误必须使用共享错误与状态契约
BridgingIO 在 standalone 启动、runtime root bootstrap、配置装载、会话恢复、control-plane attach 和 host mode 切换过程中，必须通过共享错误与状态契约返回 lifecycle 状态与失败结果，而不是继续依赖散落的字符串消息。

#### 场景:standalone 启动遇到配置问题
- **当** standalone core 在启动阶段遇到配置缺失、配置不兼容、运行目录不可写或迁移失败
- **那么** 系统必须返回共享状态、共享错误分类、模块子码和恢复提示，而不是只输出临时错误文本

### 需求:会话与运行模式状态投影必须使用 canonical 状态词汇
BridgingIO 对外暴露的会话、运行模式和 lifecycle 状态投影必须使用共享状态词汇，至少能够稳定区分 `ready`、`degraded`、`locked`、`not_ready`、`unsupported` 和 `method_not_implemented` 等语义。

#### 场景:control-plane 读取当前运行状态
- **当** 本地 control-plane、TUI 或 future UI 读取当前 core 的启动、attach、恢复或 shutdown 状态
- **那么** 返回结果必须使用共享状态词汇，而不能由不同调用面各自发明近义状态标签

### 需求:core-owned 配置必须持久化独立的 operator locale 字段
BridgingIO 的 core-owned 配置模型必须持久化一个独立的 operator locale 字段，用于声明 core-owned CLI/TUI 使用的语言。该字段必须与 future UI 的语言偏好解耦，并且当前版本只允许 `zh-CN` 与 `en-US` 两个合法值。

#### 场景:配置文件显式声明 core locale
- **当** 操作员在配置文件中写入 `core.operator_locale = "zh-CN"`（或等价正式键名）
- **那么** core 必须在装载该配置后将中文作为 CLI/TUI 的 operator-facing 语言，而不是忽略该字段或要求 UI 代为翻译

#### 场景:配置文件未声明 core locale
- **当** 配置文件未显式设置 core operator locale
- **那么** core 必须按正式默认值装载该字段，并在后续序列化或示例配置中保持可预测的一致行为，而不是依赖宿主系统 locale 隐式漂移

### 需求:core-owned locale 字段必须执行严格校验与稳定写回
core operator locale 字段必须执行严格枚举校验，并且在配置装载、menuconfig 保存、示例配置生成与 round-trip 序列化时保持稳定写回。系统禁止接受当前版本不支持的 locale 值并静默降级。

#### 场景:保存不受支持的 locale 值
- **当** 操作员或工具尝试把 core operator locale 写为 `ja-JP` 等当前版本不支持的值
- **那么** core 必须拒绝该配置并返回明确错误，而不是静默接受后再回退成其他语言

#### 场景:menuconfig 保存 core locale
- **当** 操作员在 `menuconfig` 中修改 core operator locale 并保存
- **那么** 配置写回后的 TOML 必须稳定保留该字段和值，而不是只在内存中生效或在下一次序列化时丢失

### 需求:SSH target 绑定 imported vault key 时必须只持久化 canonical `credential_ref`
当操作员在 target 编辑流程中通过 picker 或 inline import 绑定 SSH key 时，系统必须只把 canonical `vault://.../ssh-private-key/<name>` 写回 target 配置。系统禁止把私钥明文、passphrase 或其他 secret material 写回 `config.toml`。

#### 场景:picker 绑定 imported SSH key 后保存 target
- **当** 操作员通过 `Credential Source` picker 选中 imported vault SSH key 并保存 target
- **那么** target 配置中必须只保存 canonical `credential_ref`
- **并且** 不得写入 key 内容、passphrase 或密文 locator

#### 场景:locked sensitive SSH target
- **当** sensitive SSH target 处于 vault `locked` 状态
- **那么** target 列表/详情不得泄露已绑定 key 的 label、canonical ref、status 或 version 摘要
- **并且** imported key picker 必须在该状态下不可用

### 需求:SSH target 的凭据绑定流程必须支持选择已导入的 vault SSH key
当本地 operator 在受信任管理面中编辑 SSH target 时，系统必须允许其通过正式选择流程绑定已导入的 `ssh-private-key`，而不是继续把“手工输入 raw `credential_ref`”作为使用 vault-managed SSH key 的主要路径。该流程必须最终把 canonical `vault://...` 引用写入 target 配置，而不是把 key material 写回 profile。

#### 场景:操作员为 SSH target 选择 imported key
- **当** 操作员在 menuconfig 中编辑一个 SSH target，并选择使用某个已导入的 vault SSH key
- **那么** 系统必须提供 display-safe 的 key picker 或等价子流程
- **并且** target 落盘结果必须只保存被选中的 canonical `credential_ref`

#### 场景:操作员为 sensitive ssh target 选择 imported key
- **当** 操作员在 menuconfig 中编辑一个 `kind = ssh` 的 sensitive target，且当前 vault 已 `unlocked`
- **那么** 系统必须在 `Sensitive Overlay` 中提供等价的 imported key picker / 绑定子流程
- **并且** 最终写回结果必须进入 sensitive overlay，而不得复制进 public cache

#### 场景:操作员在 target 流程内联导入本地 key
- **当** 操作员在 SSH target 的凭据绑定流程中发现当前没有合适的 imported key
- **那么** 系统必须允许其进入受控的 `Import Local SSH Key Into Vault` 或等价子流程
- **并且** 导入成功后必须能够把新生成的 canonical `credential_ref` 回填到当前 target，而不是要求用户返回后手工输入 URI

#### 场景:locked 的 sensitive ssh target 不得暴露已绑定 key 身份
- **当** 某个 `kind = ssh` 的 sensitive target 当前仍依赖 vault authoritative overlay，且 vault 状态为 `locked`
- **那么** 系统不得在 target 列表、target detail 或任何等价投影中暴露所绑定 imported key 的 label、canonical `credential_ref`、status 或 record-id
- **并且** 不得在该状态下开放 imported key picker

### 需求:direct identity SSH 启动必须显式旁路 broker 并隔离 ambient agent
当 SSH target 使用本地未加密 identity 文件路径，且当前 `SSH 安全访问` 被明确关闭时，系统必须显式绕过 vault broker、local broker 与 managed password delivery，并隔离宿主环境中已有的 `SSH_AUTH_SOCK` 或等价 ambient agent 影响，避免错误命中宿主默认 agent 或把 direct identity 路径误报为 broker 故障。

#### 场景:direct identity 启动不创建 broker session
- **当** 某次 one-shot SSH 调用使用本地未加密 identity 文件路径完成认证，且 `SSH 安全访问` 为关闭
- **那么** 系统必须直接为该次调用注入 `-i <path>`、`IdentityFile=<path>` 或等价 direct identity 参数
- **并且** 系统不得创建、attach、detach 或 close 任何 vault broker session 或 local secure-delivery session

#### 场景:direct identity 启动隔离 ambient agent
- **当** 宿主环境中已经存在 `SSH_AUTH_SOCK` 或等价 ambient agent 配置，且当前 SSH 调用走 `SSH 安全访问 = false` 的 direct identity 路径
- **那么** 系统必须在本次结构化执行上下文中清理、覆盖或等价隔离该环境输入
- **并且** 系统不得让 direct identity 结果受到宿主默认 agent 的隐式影响

### 需求:SSH target 运行时必须把认证配置解析为结构化 delivery plan
当 runtime 为 SSH target 准备 one-shot、interactive shell 或 test connection 时，系统必须根据 typed SSH auth 配置解析出稳定的结构化 delivery plan，而不是继续只依赖 `credential_ref` 或零散布尔值推断。该 plan 至少必须区分 `None`、`PasswordDirectAskpass`、`PasswordManagedAskpass`、`DirectIdentityFile`、`LocalBrokeredIdentity` 与 `VaultBrokeredIdentity` 等正式语义。

#### 场景:password 认证根据 SSH 安全访问切换 delivery plan
- **当** SSH target 使用 `password` 认证
- **那么** plain + `SSH 安全访问 = false` 必须解析为 direct password carrier plan
- **并且** plain + `SSH 安全访问 = true` 或 sealed + `password` 必须解析为 managed password delivery plan

#### 场景:本地未加密私钥根据 SSH 安全访问切换 delivery plan
- **当** SSH target 使用本地未加密私钥路径完成认证
- **那么** `SSH 安全访问 = false` 必须解析为 `DirectIdentityFile`
- **并且** `SSH 安全访问 = true` 或 sealed 强制安全访问必须解析为 `LocalBrokeredIdentity`

#### 场景:vault-managed 私钥固定走 vault broker
- **当** SSH target 使用 canonical vault `credential_ref` 进行私钥认证
- **那么** 系统必须解析为 `VaultBrokeredIdentity`
- **并且** 不得为该组合再提供关闭 `SSH 安全访问` 的 direct path

### 需求:structured SSH 执行必须支持受控 env overlay 与 carrier 清理
当 SSH 认证依赖 password askpass、broker endpoint、helper 路径或等价 invocation-scoped carrier 时，structured invocation 必须支持受控 env overlay、helper 生命周期清理与 display-safe 诊断。系统不得为了 password secure delivery 回退到 shell flatten、全局环境污染或长期驻留 helper。

#### 场景:password carrier 仅存在于当前 invocation 的 env overlay
- **当** runtime 为某次 SSH 调用建立 askpass helper、password env 变量或等价 carrier
- **那么** 系统必须把这些值限制在当前结构化执行上下文中
- **并且** 不得要求宿主 shell、全局环境或后续无关调用继续持有该 carrier

#### 场景:password carrier 与 helper 在调用结束后清理
- **当** 一次依赖 password carrier 的 SSH 调用成功、失败、取消或超时结束
- **那么** 系统必须清理 invocation-scoped helper、env carrier 与等价临时资源
- **并且** 不得让后续调用复用上一轮 password helper 或临时文件

#### 场景:password delivery 覆盖参数不得与默认 probe 参数冲突
- **当** runtime 为 password 路径注入 `BatchMode`、`NumberOfPasswordPrompts`、`PreferredAuthentications`、`PubkeyAuthentication` 等 SSH 选项
- **那么** 系统必须先移除同 key 的默认选项或历史残留选项，再注入 password delivery 选项
- **并且** 不得在同一次调用中并存 `BatchMode=yes/no`、`NumberOfPasswordPrompts=0/1` 等互斥配置

