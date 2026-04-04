## 新增需求

### 需求:sensitive target 的 public descriptor digest 必须只覆盖公开字段
当系统为 sensitive target 计算 public descriptor digest 时，摘要必须仅覆盖 `id`、`display_name`、`aliases`、`kind`、`enabled`、`storage_class`、`access_class` 与 `sealed_profile_ref` 等公开字段。系统禁止把 `notes`、连接参数、credential 引用或等价 sensitive overlay 字段混入 public descriptor digest。

#### 场景:仅修改 sensitive overlay 字段
- **当** 操作员仅修改某个 sensitive target 的 `notes`、连接参数、policy 或 `credential_ref`
- **那么** 系统不得把该修改视为 public descriptor 漂移
- **并且** public descriptor digest 必须保持稳定

#### 场景:修改 public descriptor 字段
- **当** 操作员修改某个 sensitive target 的 `display_name`、`aliases` 或 `enabled`
- **那么** 系统必须更新该 target 的 public descriptor digest，并使后续 public cache 与 vault authoritative descriptor 保持一致

### 需求:vault unlock 后必须执行 sensitive target reconcile
当 vault 从 `locked` 进入 `unlocked` 时，系统必须执行一次正式的 sensitive target reconcile，而不是仅临时把 overlay 叠加到当前内存对象。该 reconcile 必须重新读取 vault authoritative target-profile、校验 public descriptor，并修复或刷新 `config.toml` 中的 sensitive target public cache。

#### 场景:unlock 后修复缺失的 public cache
- **当** vault 解锁成功，且某个 sensitive target 在 `config.toml` 中的 public cache 缺失
- **那么** 系统必须从 vault authoritative target-profile 重新生成该 public cache，并使该 target 重新出现在正式 catalog 中

#### 场景:unlock 后修复过期的 public cache
- **当** vault 解锁成功，且某个 sensitive target 在 `config.toml` 中的 public cache 与 vault authoritative public descriptor 不一致
- **那么** 系统必须以 vault authoritative public descriptor 为准刷新该 public cache，而不得继续信任陈旧外层配置

## 修改需求

### 需求:target 配置必须区分 public descriptor 与 sealed overlay
BridgingIO 必须继续将 target 配置区分为公开层与敏感层，但对 sensitive target，系统必须把 vault 中的 `target-profile` 对象定义为唯一真相源，而不再把 `config.toml` 外层 descriptor 视为 authoritative truth。`config.toml` 中的 sensitive target 只允许作为 public cache 存在；plain target 继续以 `config.toml` 为真相源。

#### 场景:读取 legacy plain target 配置
- **当** standalone core 读取到未声明安全分层的 legacy target 配置
- **那么** 系统必须将其兼容映射为 `storage_class = plain` 且 `access_class = anonymous-local`
- **并且** 该 plain target 继续以 `config.toml` 为唯一真相源

#### 场景:创建新的 sensitive target
- **当** 操作员创建一个新的 sensitive target
- **那么** 系统必须把该 target 的 `public_descriptor + sensitive_overlay` 一并写入 vault authoritative `target-profile`
- **并且** `config.toml` 只能持久化该 target 的 public cache，而不得再持久化其 sensitive overlay

#### 场景:创建 sensitive ssh target 并绑定 imported key
- **当** 操作员创建或更新一个 `kind = ssh` 的 sensitive target，并为其选择某个 imported vault SSH key
- **那么** 系统必须把该 key 的 canonical `credential_ref` 写入 `target-profile.sensitive_overlay`
- **并且** 不得把该 `credential_ref` 复制到 `public_descriptor` 或 `config.toml` public cache

#### 场景:手工删除 sensitive target 的 config cache
- **当** 操作员手工删除某个 sensitive target 在 `config.toml` 中的 public cache，但 vault 中的 authoritative `target-profile` 仍然存在
- **那么** 系统不得把该操作视为正式删除该 target
- **并且** 该 target 必须在后续 unlock/reconcile 流程中可被恢复

### 需求:sealed target 在未解锁时必须返回受限描述而不是完整连接摘要
target 运行时索引在支持 sensitive target 后，必须允许对未解锁 target 返回受限 public cache，而不是要求任意时刻都拥有完整 resolved profile。对于依赖 vault authoritative `target-profile` 的 sensitive target，locked 状态下只需返回足以识别 target 的最小 public 信息；unlock 后必须完成 resolved projection 与完整性校验。

#### 场景:未解锁时列出 sensitive target
- **当** vault 尚未解锁，且某个 target 的真相位于 vault authoritative `target-profile`
- **那么** 系统必须至少返回该 target 的 canonical id、display name、aliases、kind、enabled 与安全状态摘要
- **并且** 不得暴露完整 host、username、selector、notes 或等价 sensitive overlay 字段

#### 场景:未解锁时查看 sensitive ssh target 的凭据状态
- **当** vault 尚未解锁，且某个 `kind = ssh` 的 sensitive target 绑定了 imported vault SSH key
- **那么** 系统不得暴露该 imported key 的 label、canonical `credential_ref`、version 或等价身份字段
- **并且** 最多只能返回 generic locked / protected 诊断，而不能把 secret inventory 复制到 target 投影中

#### 场景:解锁后解析 sensitive target
- **当** vault 从 `locked` 进入 `unlocked`，且某个 sensitive target 的 vault authoritative `target-profile` 可用
- **那么** 系统必须解析该 target 的 sensitive overlay，生成 resolved profile，并将 catalog projection 更新为已解析状态

#### 场景:解锁后发现 public descriptor digest 不匹配
- **当** vault 从 `locked` 进入 `unlocked`，且某个 sensitive target 的 public descriptor digest 与 authoritative descriptor 不匹配
- **那么** 系统必须返回明确的 tamper 或 repair-needed diagnostics，而不得继续把该 target 视为正常 resolved target

## 移除需求
