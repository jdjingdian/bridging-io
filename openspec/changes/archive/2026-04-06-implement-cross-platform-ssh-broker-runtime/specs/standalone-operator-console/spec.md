## MODIFIED Requirements

### 需求:`menuconfig` 的 SSH target 管理页必须提供真实测试连接流程
对于 `kind = ssh` 的 target，`bridgingio-core menuconfig` 必须提供正式的 `Test Connection --->` 入口，用于对当前草稿连接配置执行一次真实 SSH 连通性探测。系统不得把该动作退化为字段完整性校验，也不得要求操作员先 `Apply Target --->` 或 `Save` 才能测试。对于 vault-managed SSH key，该流程必须只在 broker runtime 已真实就绪时才把 endpoint 下发给 SSH；对于 direct identity 文件，该流程必须显式走 identity-file 语义，而不是错误依赖 broker。

#### 场景:plain SSH 在 Connection Profile 中直接测试当前草稿
- **当** 操作员进入一个 plain SSH target 的 `Connection Profile`
- **那么** 系统必须提供 `Test Connection --->`
- **并且** 该动作必须基于当前草稿中的 host、port、username、`credential_ref` 与等价连接字段执行，而不是回退到磁盘上一次保存的基线值

#### 场景:sealed SSH 仅在 unlocked Sensitive Overlay 中允许测试
- **当** 操作员进入一个 sealed SSH target，且 vault 状态为 `unlocked`
- **那么** 系统必须在 `Sensitive Overlay` 中提供 `Test Connection --->`
- **并且** 该动作必须使用当前 overlay 草稿值执行真实 SSH 探测

#### 场景:sealed SSH 在 locked 状态下不得暴露测试入口
- **当** 操作员进入一个 sealed SSH target，且 vault 状态为 `locked`
- **那么** 系统必须继续只显示通用锁定提示与 `Unlock Vault --->`
- **并且** 系统不得显示 `Test Connection --->`、sensitive 连接字段或其他等价测试入口

#### 场景:测试连接采用 timeout -> waiting -> result 的 popup 流程
- **当** 操作员在 SSH target 页面触发 `Test Connection --->`
- **那么** 系统必须先弹出 timeout 输入弹窗，并以毫秒为单位提供默认值 `2000`
- **并且** 在确认后必须进入等待态弹窗
- **并且** 在探测结束后必须展示成功、失败、取消或超时结果弹窗，而不是只在状态栏瞬时输出一条文本

#### 场景:plain SSH 的 vault-backed credential 在 locked 状态下受控失败
- **当** plain SSH target 的当前草稿引用了 vault-managed `credential_ref`，且该 credential 在当前 lock state 下不可用于 SSH secret delivery
- **那么** 系统仍必须允许操作员触发 `Test Connection --->`
- **并且** 测试可以返回受控失败结果
- **并且** 系统不得因此隐式触发 `Unlock Vault` 或把该 plain target 改判为 sealed target

#### 场景:broker endpoint 未就绪时必须返回专属失败
- **当** 当前 SSH 测试连接依赖 broker delivery，且 broker runtime 未能把解析出的本地 endpoint 真正启动到 ready
- **那么** 系统必须返回 broker 专属的受控失败（例如 `broker-endpoint-unavailable`）
- **并且** 结果反馈必须明确提示失败点位于 broker endpoint 就绪性，而不是泛化为普通认证失败
- **并且** 系统不得通过 fallback 私钥路径掩盖该 broker 故障

#### 场景:agent 已返回身份时不得误报 broker 未就绪
- **当** SSH 测试日志已显示 `agent returned` / `Offering public key ... agent`，但认证最终被远端拒绝（例如 `Permission denied (publickey)`）
- **那么** 系统必须将结果归类为认证失败（如 `auth-publickey-rejected`）而非 `broker-endpoint-unready`
- **并且** 错误分类不得因为同一 stderr 中出现无关的 `no such identity` 文本而错误提升为 broker 异常

#### 场景:direct identity 文件测试必须显式旁路 broker
- **当** 当前 SSH 测试连接的 `credential_ref` 直接指向本地 identity 文件，而不是 canonical vault ref
- **那么** 系统必须通过显式 `-i <path>`、`IdentityFile=<path>` 或等价 direct identity 语义执行该次探测
- **并且** 系统不得创建 broker session 或展示 broker unavailable 结果，除非该次探测真实使用了 broker 路径
