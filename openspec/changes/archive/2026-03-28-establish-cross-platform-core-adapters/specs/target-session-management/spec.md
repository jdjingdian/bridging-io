## ADDED Requirements

术语对齐（与 design 同步）：

- `host platform`：core 实际运行宿主的本地操作系统语义
- `target shell dialect`：目标端命令执行语义（如 `ssh-posix`、`adb-android-shell`、future `ssh-windows-cmd`）
- `local transport`：宿主侧 UI/Core 本地 control-plane 传输语义
- `runtime paths`：由宿主平台路径适配器解析出的 data/state/metadata/artifacts/logs/temp/endpoint
- `structured invocation`：`program + args + diagnostics` 的结构化执行真相源

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
