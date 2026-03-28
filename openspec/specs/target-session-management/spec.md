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
系统必须提供 `bridgingio-core --self-test` 启动入口，用于在不提供 `--config`、`--runtime-root` 的前提下执行内建自检。自检至少必须覆盖 one-shot 执行、interactive shell 生命周期和 runtime 执行链路，并通过进程退出码反映结果。

#### 场景:操作员运行 `--self-test`
- **当** 操作员执行 `bridgingio-core --self-test`
- **那么** 系统必须自动执行内建自检并输出清晰的通过/失败摘要；全部通过时返回退出码 `0`，任一检查失败时返回非零退出码

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

