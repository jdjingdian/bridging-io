## 新增需求

### 需求:canonical vault runtime 必须把 uninitialized 作为正式状态并要求显式 init
系统必须把“当前实例不存在可用 vault metadata / 持久化真相”建模为正式的 `uninitialized` 状态，而不是继续将其视为 `locked` 的弱变体。处于 `uninitialized` 时，系统必须拒绝 unlock，并要求本地 operator 先执行显式 `vault init` 或等价初始化动作。

#### 场景:当前 runtime store 中不存在 vault metadata
- **当** 当前实例的 vault 持久化目录不存在 `metadata.db` 或等价初始化真相
- **那么** 系统必须返回 `uninitialized` 状态，并拒绝把该实例继续视为可直接 unlock 的 `locked` vault

#### 场景:删除 vault 后重新进入管理面
- **当** 操作员显式删除了当前实例的 vault 持久化数据
- **那么** 系统必须使该实例回到 `uninitialized`，并在再次解锁前要求先执行显式 init

### 需求:vault delete 必须只作用于当前 vault store 且不得隐式清除共享 os-native protector
系统必须允许本地 operator 显式删除当前实例的 vault 持久化数据，以便恢复测试或重建初始状态；但该删除语义必须限定在当前 vault store，不得隐式扩展为清除共享的全局 `os-native` keyring 条目或其他不与当前 runtime root 绑定的宿主级 protector 状态。

#### 场景:操作员删除当前实例的 vault
- **当** 本地 operator 对某个实例执行 `vault delete`
- **那么** 系统必须删除该实例当前 runtime store 下的 vault 数据，并使该实例返回 `uninitialized`

#### 场景:删除当前实例 vault 时存在共享 os-native protector
- **当** 当前 vault 使用 `os-native` 作为可用 protector，且宿主平台上的 keyring 条目并不与当前 runtime root 唯一绑定
- **那么** `vault delete` 不得把该共享 keyring 条目一并删除，而必须仅作用于当前实例的 vault store

### 需求:长期 agent token 的 delete 必须以 revoked 作为唯一前置条件
系统必须把长期 agent token 的删除建模为一个独立于 revoke 的终态动作，并明确 `revoked` 是唯一允许进入 delete 的前置条件。`expired` 只能表示 TTL 已到，不得被视为 delete 的等价授权条件。

#### 场景:active token 直接请求 delete
- **当** 本地 operator 尝试删除一个仍处于 `active` 的 token
- **那么** 系统必须拒绝该请求，并要求其先将 token 置为 `revoked`

#### 场景:expired token 请求 delete
- **当** 本地 operator 尝试删除一个处于 `expired` 的 token
- **那么** 系统必须拒绝直接 delete，并要求该 token 先显式进入 `revoked` 状态

#### 场景:revoked token 请求 delete
- **当** 本地 operator 删除一个已处于 `revoked` 的 token
- **那么** 系统必须允许该 token 进入 `deleted` 或等价终态，并使其不再出现在默认 token 管理摘要列表中

### 需求:token 删除后必须保留最小化审计 tombstone
系统在执行 token delete 后，必须保留该 token 的最小化生命周期终态信息，用于解释该 token 曾经存在、何时被 revoke、何时被 delete，而不是把它从持久化真相层中物理硬删到完全无迹可寻。

#### 场景:已删除 token 不再出现在普通列表中
- **当** 某个 token 已成功进入 `deleted` 终态
- **那么** 默认 token 摘要列表不得继续把它显示为可管理 token

#### 场景:需要解释历史 token 生命周期
- **当** 系统、审计面或本地受信任管理面需要解释某个 token 的历史状态
- **那么** 系统必须仍能返回该 token 已被 revoke / delete 的最小化审计信息，而不是因物理硬删而完全无法解释其历史

### 需求:token 的 operator-facing 备注必须可在不重签发的前提下更新
系统必须允许本地受信任管理面在不重新签发 token 的前提下更新 token 的 operator-facing 备注字段，并使该字段继续作为 display-safe 摘要的一部分被展示。

#### 场景:操作员更新 token 备注
- **当** 本地 operator 对一个尚未删除的 token 更新备注字段
- **那么** 系统必须在不改变 token 明文、token id 与 principal 绑定的前提下保存该备注更新

### 需求:expiring token 必须在依赖本机时钟的同时防止时钟回拨复活
系统必须允许本地 operator 创建“长期有效”或“在指定时间之前有效”的 token；但对于后者，系统必须明确其过期语义建立在宿主本机时钟之上，并通过持久化单向过期状态、时钟水位线或等价机制，确保已过期 token 不会因为系统时间后退而重新变为可用。

#### 场景:创建长期 token
- **当** 本地 operator 创建一个选择“长期有效”的 token
- **那么** 系统必须允许该 token 不设置过期时间，并继续仅通过 revoke/delete 管理其生命周期

#### 场景:创建指定时间失效 token
- **当** 本地 operator 创建一个选择“在指定时间之前有效”的 token
- **那么** 系统必须记录该 token 的失效时间，并明确该失效时间按当前实例的本机系统时间解释

#### 场景:token 已过期后系统时间回拨
- **当** 某个 token 已被系统判断为 `expired`，随后宿主系统时间回拨到该 token 的失效时间之前
- **那么** 系统不得因本机时间后退而重新把该 token 视为 `active`

#### 场景:系统检测到本机时钟回拨或明显异常
- **当** 当前实例检测到本机 wall clock 早于最近可信观测值、存在明显回拨，或处于等价的时间异常状态
- **那么** 系统必须返回明确诊断，并阻止或严格限制新的 expiring token 创建，直到操作员修复时钟或改用不依赖过期时间的 token 模式

## 修改需求

### 需求:高风险 vault 与 token 管理动作必须消费真实 attestation
对于 vault 解锁、vault delete、secret reveal/export、长期 token 签发、token 删除、token scope 扩大和敏感 secret rotation，系统必须校验并消费真实存在、未过期、与当前 intent 和 payload digest 匹配的 `LocalAdminAttestationRecord`。系统不得接受 UI 或 CLI 传入的占位 attestation id 作为等价授权。

#### 场景:前端传入 synthetic attestation id
- **当** 桌面 UI、TUI 或其他 trusted client 在未先创建 intent/attestation 的情况下，直接对 `unlock_vault`、`create_agent_token`、`delete_agent_token` 或 `delete_vault` 传入一个固定占位 attestation id
- **那么** runtime 必须拒绝该请求，并返回 attestation 缺失或不匹配的诊断，而不能把该字符串直接记入 record 后继续放行

#### 场景:匹配的 attestation 被单次消费
- **当** 本地管理员已为某个高风险动作创建匹配的 intent 并完成本地用户验证
- **那么** 系统必须只允许该 attestation 成功消费一次；后续任何复用同一 attestation 的请求都必须被拒绝

## 移除需求
