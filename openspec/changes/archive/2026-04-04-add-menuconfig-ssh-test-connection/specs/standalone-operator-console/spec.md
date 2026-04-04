## 新增需求

### 需求:`menuconfig` 的 SSH target 管理页必须提供真实测试连接流程
对于 `kind = ssh` 的 target，`bridgingio-core menuconfig` 必须提供正式的 `Test Connection --->` 入口，用于对当前草稿连接配置执行一次真实 SSH 连通性探测。系统不得把该动作退化为字段完整性校验，也不得要求操作员先 `Apply Target --->` 或 `Save` 才能测试。

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
- **当** 当前 SSH 测试连接依赖 broker delivery，且解析出的 `IdentityAgent` endpoint 未配置、缺失或不可访问
- **那么** 系统必须返回 broker 专属的受控失败（例如 `broker-endpoint-unavailable`）
- **并且** 结果反馈必须明确提示失败点位于 broker endpoint 就绪性，而不是泛化为普通认证失败
- **并且** 系统不得通过 fallback 私钥路径掩盖该 broker 故障

### 需求:`menuconfig` 的 SSH 测试连接必须只显示简洁结果并保留 display-safe 日志
`menuconfig` 的 SSH 测试连接流程在界面上必须只显示简洁结果摘要，而更详细的执行诊断必须以 display-safe 方式写入 menuconfig 日志。系统不得在结果弹窗、状态栏或普通列表行中回显 raw SSH 错误输出或 secret material。

#### 场景:结果弹窗只显示成功或失败状态
- **当** SSH 测试连接完成
- **那么** 系统必须在结果弹窗中只显示 `Connection succeeded`、`Connection failed`、`Connection cancelled`、`Connection timed out` 或等价简洁状态
- **并且** 当失败分类为 broker endpoint 未就绪时，系统可以显示 `Connection failed (broker unavailable)` 或等价简洁提示
- **并且** 不得在该弹窗中展开 raw stderr、命令全文或 secret 相关内容

#### 场景:测试连接把详细过程写入 display-safe session 日志
- **当** 操作员在 `menuconfig` 中触发一次 SSH 测试连接
- **那么** 系统必须向当前 runtime root 的 menuconfig 日志写入带稳定 `flow_id` 的 display-safe 事件
- **并且** 这些事件至少必须覆盖 started / finished / failed / cancelled / timed-out 等关键节点

#### 场景:测试连接日志不得泄露 secret material
- **当** 系统为 SSH 测试连接写入日志
- **那么** 系统不得记录私钥明文、passphrase、token 明文、raw SSH stderr 或等价 secret material
- **并且** 若日志涉及 credential 或 toolchain，只允许记录 canonical `credential_ref`、工具来源摘要、错误分类或其他 display-safe 标识

#### 场景:broker endpoint 失败必须产出可定位 breadcrumb
- **当** SSH 测试连接因 broker endpoint 未就绪而失败
- **那么** `menuconfig-session` 日志必须记录可关联 `flow_id` 的 display-safe 诊断节点
- **并且** 该节点至少包含 broker 失败分类与 endpoint 就绪性检查结果摘要

## 修改需求

## 移除需求
