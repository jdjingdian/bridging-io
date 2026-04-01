## ADDED Requirements

### 需求:loopback 匿名兼容 principal 只允许访问 plain target
为兼容旧版本升级路径，model-plane 必须把“无 token 仍可访问 plain target”的能力建模为显式启用、仅限 loopback 的匿名兼容 principal。该 principal 不是 `auth_mode=none` 的隐式别名，也不得访问 sealed target 或扩大到 non-loopback 监听。

#### 场景:loopback 缺失 token 时执行 plain target
- **当** model-plane 监听在 loopback，操作员显式启用了匿名兼容模式，且某个请求未携带 token
- **那么** 系统可以将该请求映射为 `anonymous-local` principal，并仅允许其访问 `access_class = anonymous-local` 的 plain target 与最小 public catalog

#### 场景:loopback 缺失 token 时访问 sealed target
- **当** model-plane 监听在 loopback，某个未携带 token 的请求尝试访问 sealed target 或依赖 vault overlay 的 target profile
- **那么** 系统必须拒绝该请求，而不能因为当前实例启用了匿名兼容模式就放宽到 sealed target

#### 场景:non-loopback 缺失 token
- **当** model-plane 监听在非 loopback 地址，且某个请求未携带 token
- **那么** 系统必须拒绝该请求，而不能将 non-loopback 请求降级为匿名兼容 principal

### 需求:显式携带无效 token 的请求不得回退为匿名 principal
一旦请求显式携带 bearer token，系统必须先按 token principal 路径完成认证。若该 token 无效、过期、被撤销或 scope 不允许，系统必须直接拒绝，而不能把该请求视为“等价于没带 token”并回退到匿名兼容 principal。

#### 场景:显式携带无效 token
- **当** 某个 loopback 请求显式携带了无效、过期或已撤销的 bearer token
- **那么** 系统必须返回认证失败或基于 scope 的拒绝结果，而不能改按匿名 principal 继续执行 plain target

### 需求:长期 token 签发与 scope 扩大必须由本地 attestation 强制保护
model-plane 的长期 agent token 签发与 scope 扩大必须在 runtime 中消费真实 `LocalAdminAttestationRecord`，而不是只依赖 trusted client 自报“已经完成本地验证”。该保护必须在 `AuthN/AuthZ` 之前作为 token authority 的管理动作入口条件执行。

#### 场景:缺少 attestation 时签发长期 token
- **当** trusted client 请求创建长期 agent token，但未提供匹配当前 intent 的有效 attestation
- **那么** 系统必须拒绝签发，并且不得生成任何可用的 token 明文或 metadata record

#### 场景:扩大 token scope 时 attestation 与 payload 不匹配
- **当** trusted client 请求扩大某个 token 的 scope，但 attestation 对应的是其他 token、其他 scope 变化或其他 action kind
- **那么** 系统必须拒绝该次 scope 变更，而不能把 attestation 当作通用管理员凭证

### 需求:长期 token 明文必须只允许一次性显示
长期 token 在 runtime 成功签发后，必须只在签发响应中以一次性结果形式返回。后续 list/query/revoke/scope update 等控制面读取只能返回安全摘要，不得再次 reveal 同一 token 明文。

#### 场景:首次签发长期 token
- **当** 本地管理员通过受信任控制面成功创建一个长期 token
- **那么** 系统必须在该次签发响应中返回一次性 token 明文和对应 summary，并在后续查询中仅返回 summary

#### 场景:后续列出 token
- **当** trusted control-plane 或桌面 UI 后续列出现有 token
- **那么** 系统必须只返回 display-safe 的 token summary，而不能再次 reveal 已签发 token 的明文值
