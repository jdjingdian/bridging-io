## 为什么

在 `establish-cross-platform-core-adapters` 的 4.x 实现中，`terminal.exec` 已迁移到 structured invocation 真相源；`terminal.shell.open` 已统一走同一套 invocation 解析与诊断视图，但 interactive 启动链路仍保留“先打开 host baseline shell，再消费 structured invocation 诊断”的受控过渡语义。

这是为了在当前阶段保证交互式 shell 生命周期（write/read/interrupt/close）的稳定性与既有测试行为，避免把远端 transport 启动时序问题直接扩散到 host runtime 基线。

## 变更内容

- 为 interactive shell 增加“structured invocation 直接驱动 runtime launch”的增量实现
- 收束 `terminal.shell.open` 到与 one-shot exec 一致的最终执行真相，减少过渡分支
- 明确失败回退语义：当远端 interactive launch 不可用时，回退策略、诊断文本与状态暴露必须一致

## 影响

- 重点影响 `bridgingio-mcp`、`bridgingio-providers`、`bridgingio-platform` 的 interactive 启动与状态同步路径
- 需要补充针对 interrupt/readiness 的回归测试，避免回归到时间竞态断言
