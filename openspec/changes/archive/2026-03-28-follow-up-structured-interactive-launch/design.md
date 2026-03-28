## 背景

`follow-up-structured-interactive-launch` 是在 `establish-cross-platform-core-adapters` 基础上的收口变更：将 `terminal.shell.open` 从“host baseline 先启动 + structured invocation 仅用于诊断”的过渡语义，推进为“优先由 structured interactive invocation 直接驱动 runtime launch”。

## 设计目标

- 将 interactive open 与 one-shot exec 的执行真相统一到 structured invocation
- 在 structured interactive launch 失败时提供统一回退：回退到 host baseline，同时暴露明确诊断与可观测状态
- 保持 interrupt/readiness/close 的确定性状态语义，避免回归到时间竞态断言

## 关键决策

1. `terminal.shell.open` 在 runtime 层优先执行 structured interactive launch command；仅在该路径失败时执行 host baseline fallback。
2. fallback 不应静默发生，必须通过可观测字段暴露，包括 `launch_strategy`、`launch_fallback_applied` 和 `launch_diagnostics`。
3. 自动化测试必须覆盖两类路径：
   - structured launch 直通成功路径
   - structured launch 失败后 fallback 路径，并验证 read/interrupt/close 状态仍满足确定性语义

## 验证策略

- provider 层：验证 fallback 发生时诊断文本与 transcript 可见
- mcp/runtime 层：验证 `shell.open` 返回结构化 launch 状态，并在 fallback 路径下保持生命周期状态一致
- integration 层：验证 interactive read/interrupt/close 的稳定断言
