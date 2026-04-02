## 新增需求

### 需求:standalone `run` 与 `-d` 必须属于同一模式家族但允许不同解锁策略
BridgingIO 必须把 standalone 前台 `run` 与后台 `-d` 视为同一 standalone 模式家族下的两个子模式。两者必须共享相同的 runtime/config 真相与基本生命周期模型，但在启动解锁策略上允许受控差异。

#### 场景:前台模式遵从配置中的 trigger policy
- **当** 操作员使用 standalone `run` 启动 core
- **那么** 系统必须继续按配置中的 trigger policy 决定何时要求完成 vault 解锁，而不是无条件改写为启动即解锁

### 需求:standalone 后台模式必须覆盖 trigger policy 为 `on-core-start`
当 standalone 以后台/脱离式子模式启动时，系统必须忽略配置中的常规 trigger policy，并以安全优先的方式强制使用 `on-core-start`。该 override 必须被视为正式 lifecycle 规则，而不是临时实现细节。

#### 场景:配置声明 `on-first-secret-access` 但操作员使用 `-d`
- **当** 操作员以 `-d` 启动 standalone core，且配置中的 vault trigger policy 为 `on-first-secret-access`、`manual-only` 或其他非启动即解锁策略
- **那么** 系统必须覆盖为 `on-core-start`，并在完成解锁前保持该后台实例未就绪或 fail-closed

#### 场景:后台模式未能在启动阶段完成解锁
- **当** standalone `-d` 模式在启动阶段未能通过允许的安全 carrier 成功解锁 vault
- **那么** 系统必须保持 locked/unavailable 状态并返回明确启动失败或受控未就绪语义，而不能在后台默默等待未来某次 secret access 时再尝试解锁
