## ADDED Requirements

### 需求:MCP 终端执行必须消费结构化 invocation 而不是 shell 字符串真相
BridgingIO 的终端相关 MCP typed tools 在执行 SSH、ADB 或其他终端型 target 时，必须消费由 connector / adapter 产出的结构化 invocation 作为执行真相源，而不是在 MCP 层重新拼接一份带平台引号规则的 shell 字符串。

#### 场景:one-shot exec 使用结构化 SSH invocation
- **当** MCP 客户端请求对某个 SSH target 执行一次性命令
- **那么** 系统必须使用该 target 当前解析出的结构化程序路径与参数执行请求，而不是在 MCP 层再次手工拼接 `ssh ... 'command'` 字符串作为唯一执行路径

#### 场景:interactive shell 使用与 diagnostics 一致的 invocation
- **当** MCP 客户端为某个 ADB 或 SSH target 打开 interactive shell
- **那么** 系统必须使用与 toolchain diagnostics、one-shot exec 一致的结构化 interactive invocation，而不是另外走一套独立的字符串启动逻辑

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
