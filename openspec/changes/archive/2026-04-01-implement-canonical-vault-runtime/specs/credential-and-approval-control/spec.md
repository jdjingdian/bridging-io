## ADDED Requirements

### 需求:canonical vault runtime 必须作为唯一的持久化真相层
实现 `design-vault-and-agent-auth` 时，系统必须把 `builtin-encrypted` 作为 vault 的唯一 canonical runtime/store 语义。legacy `os-native` backend 只能作为 protector 兼容输入读取，不得继续被实现为直接保存 secret 的长期真相层。

#### 场景:升级实例读取 legacy `os-native` backend
- **当** 已升级实例读取到 legacy `[vault] backend = "os-native"` 配置
- **那么** runtime 必须把它解释为 canonical `builtin-encrypted` vault 加 `os-native` primary protector，而不是继续把 `os-native` 当作直接保存 secret 的 store

#### 场景:持久化设置时回写 canonical 配置
- **当** 已升级实例完成设置持久化或 config rewrite
- **那么** 系统必须回写 canonical vault 配置结构，而不是继续输出 legacy `backend = "os-native"` 作为长期真相

### 需求:高风险 vault 与 token 管理动作必须消费真实 attestation
对于 vault 解锁、secret reveal/export、长期 token 签发、token scope 扩大和敏感 secret rotation，系统必须校验并消费真实存在、未过期、与当前 intent 和 payload digest 匹配的 `LocalAdminAttestationRecord`。系统不得接受 UI 或 CLI 传入的占位 attestation id 作为等价授权。

#### 场景:前端传入 synthetic attestation id
- **当** 桌面 UI 或其他 trusted client 在未先创建 intent/attestation 的情况下，直接对 `unlock_vault` 或 `create_agent_token` 传入一个固定占位 attestation id
- **那么** runtime 必须拒绝该请求，并返回 attestation 缺失或不匹配的诊断，而不能把该字符串直接记入 record 后继续放行

#### 场景:匹配的 attestation 被单次消费
- **当** 本地管理员已为某个高风险动作创建匹配的 intent 并完成本地用户验证
- **那么** 系统必须只允许该 attestation 成功消费一次；后续任何复用同一 attestation 的请求都必须被拒绝

### 需求:vault 状态投影必须区分 lock state 与 protector readiness
系统向 trusted control-plane、桌面 UI 或 standalone 管理入口返回 vault 状态时，必须同时区分 canonical vault 的 lock state 与 protector readiness，而不是只返回一个抽象的 backend 标签。

#### 场景:已初始化但尚未解锁的 vault
- **当** 实例已存在 canonical vault 数据，且当前尚未完成本地解锁
- **那么** 状态投影必须明确返回 `locked` 及相应 protector summary，而不是仅显示 `configured_backend=builtin-encrypted`

#### 场景:当前无可用 protector
- **当** 已初始化 vault 但没有任何策略允许的可用 protector
- **那么** 状态投影必须明确返回 `unavailable` 或等价 fail-closed 结果，并阻止 secret-backed 管理动作继续执行
