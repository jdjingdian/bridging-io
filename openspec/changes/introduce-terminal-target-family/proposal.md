## 为什么

当前实现已经把 `ssh` 与 `adb` 视为终端型目标，并共享会话、通道、结构化调用与交互式 shell 执行链路，但代码结构上仍以独立 connector 和若干分散的 `match` 分发为主。随着后续 `localshell` 与 `serial` 接入，这种“局部共享、整体分叉”的形态会让扩展点、并发语义与 target 方言边界越来越模糊。

现在提出这项变更，是为了在仍保留具体 target 类型的前提下，显式建立“terminal target family”这一层抽象，把宿主运行时、目标传输与目标 shell 方言分层清楚，并为 future target 的接入预留统一契约。

## 变更内容

- 引入 `terminal target family` 概念，明确 `ssh`、`adb`、future `localshell`、future `serial` 都属于终端型目标，但继续保留各自具体 `TargetKind` 与连接配置。
- 为终端型目标定义统一的能力契约，包括 one-shot exec、interactive shell、环境探测、结构化 invocation、toolchain 解析与 target shell dialect 声明。
- 为终端型目标引入并发策略建模，区分：
  - `multiplexed terminal`: 同一 target 可复用或新建多个逻辑会话，且单个逻辑会话内允许多个并发 channel/window。
  - `exclusive terminal`: 同一 target 在同一时刻只允许一个独占 transport 或一个活动交互 channel，用于 `serial` 一类共享物理链路目标。
- 明确 future `localshell` target 与宿主 `HostPlatformAdapter.local_shell_runtime` 不是同一层概念：
  - `localshell` 是一种 target transport
  - `local_shell_runtime` 是宿主执行与 I/O 承载能力
- 收敛当前 `ssh` / `adb` 的公共终端行为，减少新增 terminal target 时在 MCP、connector、provider、runtime 多处分发和重复建模的需要。

## 功能 (Capabilities)

### 新增功能
- `terminal-target-family`: 定义终端型 target 的共享能力模型、并发策略、宿主运行时与目标 transport 分层，以及 future `localshell` / `serial` 接入契约。

### 修改功能
- `target-session-management`: 将现有 SSH / ADB 的会话、通道、interactive shell 与 target shell dialect 规则对齐到 terminal family 抽象，并增加对独占型终端 target 并发限制的规格要求。

## 影响

- 受影响代码主要集中在 `source/rust/bridgingio-domain`、`source/rust/bridgingio-connectors`、`source/rust/bridgingio-providers`、`source/rust/bridgingio-mcp` 与 `source/rust/bridgingio-platform`。
- 受影响设计文档包括终端执行链、target 方言与并发模型相关说明。
- 该提案不要求立即实现 `localshell` 或 `serial`，但会为两者建立明确的规格与架构落点。
