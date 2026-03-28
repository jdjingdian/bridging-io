## 1. Structured Interactive Launch 收口

- [x] 1.1 将 `terminal.shell.open` 的 runtime 启动从 host baseline 过渡语义迁移为直接消费 structured interactive invocation
- [x] 1.2 定义并实现 interactive launch 失败时的统一回退与诊断行为（含可观测状态）
- [x] 1.3 为 interrupt/readiness/close 场景补充回归测试，确保迁移后仍满足确定性状态语义
