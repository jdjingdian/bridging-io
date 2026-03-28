## 为什么

`document-windows-runtime-bugfix` 已经说明了一个更深层的问题：BridgingIO 当前的跨平台问题并不只是一两个 Windows 兼容缺陷，而是 core 中缺少稳定的宿主平台抽象边界。当前仓库里与平台相关的逻辑分散在 `bridgingio-providers`、`bridgingio-mcp`、`bridgingio-connectors`、runtime 入口和测试代码中，主要以 `cfg(...)`、硬编码路径、字符串拼接命令与 Unix 默认假设的方式存在。

这种结构会带来几个长期风险：

- 修复一个平台问题时，往往只能在局部追加分支，无法形成后续平台复用的统一基线
- 宿主平台差异与目标端 shell 方言差异混在一起，难以同时支持 “Windows 宿主 + ADB target” 与未来 “SSH 到 Windows target” 等组合
- control-plane IPC、runtime path、toolchain fallback、输出解码、日志与 trace 行为没有共享真相源，导致平台支持容易出现“单点修复，多处回归”
- 测试体系整体仍然偏向 Unix 真相，很多问题只能在 Windows 打包或真实部署后才暴露

这次变更需要把“跨平台补洞”升级为“跨平台核心基线重构”，为未来 Linux、Windows、鸿蒙 PC 等平台实现提供可持续演进的基础。

## 变更内容

- 引入 host platform adapter 边界，把本地 shell/runtime、control-plane IPC、runtime path、toolchain 定位、日志与输出解码等宿主平台行为从业务逻辑中抽离出来
- 明确区分宿主平台与 target shell dialect，避免继续把 Windows/Unix 宿主差异与 SSH/ADB/未来 Windows target 的远端 shell 语义混为一谈
- 让 connectors 暴露的结构化 `CommandInvocation` / interactive invocation 成为执行真相源，逐步替代 MCP 与 provider 中自行拼接 shell 字符串的路径
- 为 local control-plane 定义平台原生 transport 语义，使 UI-managed / standalone 模式不再只在 Unix socket 语义下完整
- 将 runtime root、默认配置样例、生成配置和内置工具分发路径调整为宿主平台可解析的模型，而不是继续默认 `/bin/sh`、`~/.bridgingio`、`.sock` 等 Unix 假设
- 将统一 logger 与平台输出解码纳入 core 运行时基线，使 `log_level`、trace 与 artifact 文本捕获在不同平台上都有一致的治理模型
- 建立 core cross-platform contract 与自动化矩阵测试，覆盖 one-shot exec、interactive shell、IPC、paths、toolchain fallback、vault、output decoding 等关键流程

## 功能 (Capabilities)

### 修改功能

- `target-session-management`: 增加 host platform adapter、target shell dialect、平台原生 IPC 与 runtime path 解析要求
- `capability-aware-mcp`: 要求终端相关 typed tools 与 diagnostics 依赖结构化 invocation 和平台适配层，而不是继续以内联 shell 字符串作为执行真相
- `artifact-management`: 要求本地执行文本输出采用宿主平台兼容的解码与换行归一化策略，避免跨平台 artifact 内容失真
- `credential-and-approval-control`: 要求 `os-native` vault backend 在不同宿主平台上具备真实平台实现或明确降级/不可用语义
- `quality-and-test-automation`: 增加 core cross-platform contract、平台矩阵验证与去 Unix 偏置的测试基线

### 新增功能

无。

## 影响

- Rust 核心实现将重点影响 `source/rust/bridgingio-providers`、`source/rust/bridgingio-mcp`、`source/rust/bridgingio-connectors`、`source/rust/bridgingio-engine`、`source/rust/bridgingio-secrets`
- standalone runtime、UI-managed runtime 和 control-plane / model-plane 的启动与配置生成逻辑需要统一接入新的平台抽象
- 现有的 shell 状态维护、命令拼接、interrupt、PTY/pipe fallback 与 artifact 输出采集逻辑需要迁移到更稳定的 adapter 边界
- 测试体系需要从当前偏 Unix 的 fixture 和路径假设迁移到平台矩阵与契约测试
- 后续平台 UI 团队将能够基于同一套 core 语义扩展 Linux / Windows / OpenHarmony PC，而不是再次从 UI 侧补平台差异
