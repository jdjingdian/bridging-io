## 1. 终端家族建模

- [x] 1.1 在 domain 与配置模型中引入 terminal family / concurrency policy 元数据，同时保留现有具体 `TargetKind`
- [x] 1.2 将 terminal family、target shell dialect 与 concurrency policy 暴露到 MCP / app-api / diagnostics 的 target 视图中
- [x] 1.3 为 future `localshell` 与 `serial` 预留配置与能力声明扩展点，但不在本任务中实现其具体 transport

## 2. 统一终端 connector 契约

- [x] 2.1 提炼共享的 terminal connector 契约，统一 one-shot、interactive、probe、toolchain 与 dialect 声明入口
- [x] 2.2 将现有 `ssh` connector 适配到统一 terminal connector 契约，并保持现有行为不回退
- [x] 2.3 将现有 `adb` connector 适配到统一 terminal connector 契约，并保持现有行为不回退
- [x] 2.4 收敛 MCP 中针对 `ssh` / `adb` 的终端分发逻辑，使结构化 invocation 与共享契约成为唯一执行真相源

## 3. 并发策略与独占 lease

- [x] 3.1 明确并实现 `multiplexed` terminal target 的默认行为，确保 SSH / ADB 继续支持同一逻辑会话内多 channel / 多窗口
- [x] 3.2 在 metadata/runtime 层实现 `exclusive` terminal target 的 target 级 lease 语义
- [x] 3.3 为独占 lease 冲突补齐明确的 busy / conflict 返回语义，避免静默创建第二个活动 transport 或交互 channel
- [x] 3.4 确保 future `localshell` target 走统一 target/session/channel/audit 流程，而不是旁路到宿主 runtime 快捷路径

## 4. 验证与文档

- [x] 4.1 为 terminal family、multiplexed / exclusive 并发策略与 target-wide lease 语义补充单元测试和集成测试
- [x] 4.2 为 host runtime 与 future `localshell` target 的分层边界补充契约测试或诊断验证
- [x] 4.3 更新开发文档与 handoff 文档，说明 terminal family、并发策略以及 future `serial` / `localshell` 的接入边界
