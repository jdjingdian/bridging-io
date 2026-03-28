## MODIFIED Requirements

### 需求:interactive shell 自动化验证必须采用确定性同步机制
对于 interactive shell 的自动化测试，系统必须优先使用 marker、poll、readiness 或等价的确定性同步机制，而不是依赖固定睡眠时间窗口推测命令是否完成。

#### 场景:structured launch fallback 路径的状态一致性验证
- **当** 测试触发 `terminal.shell.open` 的 structured interactive launch 失败并进入 fallback 路径
- **那么** 测试必须断言 read/interrupt/close 的 `running`、`interrupted`、`closed` 状态语义保持确定性，并验证 fallback 诊断可见
