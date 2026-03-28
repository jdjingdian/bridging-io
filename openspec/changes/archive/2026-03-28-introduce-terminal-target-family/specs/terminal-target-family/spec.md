## 新增需求

### 需求:终端型 target 必须形成显式的 family 抽象
系统必须把 `ssh`、`adb`、future `localshell`、future `serial` 归入显式的 terminal target family，并为该 family 提供统一的能力契约。系统禁止仅通过分散的 connector 特例或 MCP 分支隐式表达这些目标的共同终端语义。

#### 场景:当前 SSH 与 ADB 对齐到同一终端家族
- **当** 系统为 `ssh` 与 `adb` target 生成能力摘要、执行 one-shot invocation 或打开 interactive shell
- **那么** 两者必须通过同一 terminal family 契约表达共享能力，而不是在每条调用链上分别临时拼接“它们其实都算终端”这一语义

#### 场景:future terminal target 接入共享家族
- **当** 团队未来新增 `localshell` 或 `serial` target
- **那么** 新 target 必须作为 terminal family 成员接入统一契约，而不是复制一套独立的 session/channel/invocation 语义

### 需求:终端型 target 必须保留具体类型并同时声明共享能力
系统必须同时保留 terminal target 的具体 `TargetKind` 与 family 级共享能力。系统禁止把所有终端型目标折叠成单一的通用 `shell` 类型，以免丢失连接配置、工具链、探测计划与 transport 差异。

#### 场景:SSH 与 ADB 保持各自连接配置
- **当** 系统读取一个 `ssh` target 和一个 `adb` target 的 profile
- **那么** 系统必须继续保留各自的连接参数与工具链语义，同时把它们映射到同一个 terminal family，而不是只保留一个模糊的 shell 大类

#### 场景:future serial target 接入 family
- **当** 团队未来引入 `serial` target
- **那么** 系统必须允许该 target 保持串口设备、波特率等专属配置，并同时获得 terminal family 的共享 session/channel 契约

### 需求:终端型 target 必须显式声明并发策略
每个 terminal family 成员都必须显式声明并发策略。系统首轮至少必须支持 `multiplexed` 与 `exclusive` 两类策略，并使会话、transport 与 channel 管理遵循该策略。

#### 场景:多路复用型终端 target
- **当** 一个 terminal target 被声明为 `multiplexed`
- **那么** 系统必须允许该 target 在复用策略允许时创建多个逻辑会话，并允许同一逻辑会话中存在多个并发 channel 或窗口

#### 场景:独占型终端 target
- **当** 一个 terminal target 被声明为 `exclusive`
- **那么** 系统必须把该 target 视为需要独占 lease 的目标，并禁止在同一时刻建立第二个活动 transport 或第二个活动交互 channel

### 需求:宿主运行时与 target transport 必须分层
系统必须把宿主运行时能力与 terminal target transport 显式区分。`HostPlatformAdapter.local_shell_runtime` 必须继续代表宿主机上的执行承载能力；future `localshell` target 必须代表进入 target/session/channel 模型的一类目标 transport。系统禁止把两者视为同一对象或同一层职责。

#### 场景:future localshell target 进入统一 target 模型
- **当** 团队未来为工作空间或宿主机配置 `localshell` target
- **那么** 系统必须为其创建 target profile、logical session、transport session、channel 与 artifact 关联，而不是因为它运行在本机就绕过 target 模型

#### 场景:宿主运行时继续承担执行承载职责
- **当** terminal family 成员需要在宿主机上启动本地进程以建立 transport 或驱动交互 I/O
- **那么** 系统必须继续通过宿主 runtime 承载该执行能力，而不是把 target transport 自身误建模为宿主 runtime 设施

## 修改需求

## 移除需求
