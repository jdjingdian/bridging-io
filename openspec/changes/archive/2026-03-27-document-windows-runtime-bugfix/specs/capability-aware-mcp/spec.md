## ADDED Requirements

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
