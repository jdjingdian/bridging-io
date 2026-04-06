## MODIFIED Requirements

### 需求:本地 operator interface matrix 必须记录 SSH 测试连接合同
当 `menuconfig` 为 SSH target 提供 `Test Connection` 动作时，本地 operator interface matrix 必须明确记录其入口位置、输入、输出、状态前置条件、日志产物与 display-safe 约束，而不是继续把该能力留作未文档化的实现细节。矩阵还必须区分 vault broker 路径与 direct identity 路径，避免把兼容路径误记录为 broker 依赖。

#### 场景:矩阵记录 plain 与 sealed SSH 的入口位置
- **当** `menuconfig` 为 SSH target 暴露 `Test Connection --->`
- **那么** 接口矩阵必须明确记录 plain SSH 的入口位于 `Connection Profile`
- **并且** 必须明确记录 sealed SSH 的入口位于 `Sensitive Overlay`
- **并且** sealed SSH 仅在 vault `unlocked` 时允许测试

#### 场景:矩阵记录测试连接的输入输出
- **当** 操作员在 `menuconfig` 中执行一次 SSH 测试连接
- **那么** 接口矩阵必须明确记录该动作的显式输入至少包括当前 SSH 草稿配置与 `timeout_ms`
- **并且** 必须明确记录界面输出只允许成功、失败、取消或超时等简洁结果，而不返回 raw SSH 日志

#### 场景:矩阵记录 plain SSH 的受控失败边界
- **当** plain SSH 的当前草稿引用 vault-managed credential，且当前 lock state 无法完成 secret delivery
- **那么** 接口矩阵必须明确记录该动作仍可被触发
- **并且** 必须明确记录结果会以受控失败返回，而不是隐式触发 unlock 或隐藏该入口

#### 场景:矩阵记录 broker endpoint 未就绪语义
- **当** SSH 测试连接依赖 broker delivery，且 broker runtime 未能把本地 endpoint 真正启动到 ready
- **那么** 接口矩阵必须明确记录该动作返回 broker 专属失败分类（例如 `broker-endpoint-unavailable`）
- **并且** 必须明确记录该失败优先级高于通用认证失败归类
- **并且** 不得把 fallback 本地私钥探测作为“成功绕过 broker”合同

#### 场景:矩阵记录 direct identity bypass 语义
- **当** SSH target 通过手工 reference 或等价输入直接引用本地 identity 文件
- **那么** 接口矩阵必须明确记录该路径使用显式 identity-file 语义
- **并且** 必须明确记录该路径不依赖 vault broker readiness
- **并且** 不得把 direct identity 失败记录为 broker unavailable

#### 场景:矩阵记录测试连接的日志产物
- **当** `menuconfig` 记录一次 SSH 测试连接的详细过程
- **那么** 接口矩阵必须明确记录相关 display-safe 事件写入 `logs/menuconfig-session.jsonl`
- **并且** 这些事件必须包含稳定 `flow_id`、结果摘要、timeout / elapsed 等受控诊断字段或等价信息
