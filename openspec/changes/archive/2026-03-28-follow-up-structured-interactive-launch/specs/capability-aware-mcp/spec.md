## MODIFIED Requirements

### 需求:MCP 终端执行必须消费结构化 invocation 而不是 shell 字符串真相
BridgingIO 的终端相关 MCP typed tools 在执行 SSH、ADB 或其他终端型 target 时，必须消费由 connector / adapter 产出的结构化 invocation 作为执行真相源，而不是在 MCP 层重新拼接 shell 字符串。

#### 场景:interactive shell open 直接消费 structured interactive invocation
- **当** MCP 客户端调用 `terminal.shell.open` 为某个 SSH 或 ADB target 打开 interactive shell
- **那么** 系统必须优先使用 structured interactive invocation 直接驱动 runtime launch，而不是固定先走 host baseline shell

#### 场景:structured interactive launch 失败时回退并暴露诊断
- **当** `terminal.shell.open` 的 structured interactive launch 在 runtime 启动阶段失败
- **那么** 系统必须回退到 host baseline 路径，并在返回结果中暴露 `launch_strategy`、`launch_fallback_applied` 与 `launch_diagnostics`，使调用方能够区分“structured 直通成功”和“fallback 生效”
