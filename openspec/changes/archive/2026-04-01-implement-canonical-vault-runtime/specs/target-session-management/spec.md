## ADDED Requirements

### 需求:target 配置必须区分 public descriptor 与 sealed overlay
实现 canonical vault runtime 后，target 配置必须支持把公开 inventory 与敏感连接配置拆成两层：`config.toml` 继续承载 plain target 与 sealed target 的 public descriptor，vault 负责 sealed overlay、credential 关联和其他高敏字段。系统不得继续把“是否在 vault 中存储”与“是否需要 token 才能访问”混成同一个开关。

#### 场景:读取 legacy plain target 配置
- **当** standalone core 读取到未声明安全分层的 legacy target 配置
- **那么** 系统必须将其兼容映射为 `storage_class = plain` 且 `access_class = anonymous-local`，以保持旧版本 loopback 直连能力

#### 场景:高敏 target 使用 sealed overlay
- **当** 操作员将某个 target 标记为高敏或创建需要 vault credential 的 target
- **那么** 系统必须允许仅把最小 public descriptor 保留在 `config.toml`，并把 host、selector、notes、policy、`credential_ref` 等敏感字段保存到 vault overlay

### 需求:sealed target 在未解锁时必须返回受限描述而不是完整连接摘要
target 运行时索引在支持 sealed target 后，必须允许对未解锁 target 返回 redacted descriptor。系统不得要求所有 target 在任意时刻都暴露完整连接摘要；对于依赖 vault overlay 的 target，未解锁时只需返回足以识别 target 的最小 public 信息。

#### 场景:未解锁时列出 sealed target
- **当** vault 尚未解锁，且某个 target 的连接配置依赖 sealed overlay
- **那么** 系统必须至少返回该 target 的 canonical id、display name、aliases、kind、enabled 和安全状态摘要，但不得暴露完整 host、username、selector 或等价敏感连接信息

#### 场景:解锁后校验 public descriptor
- **当** vault 从 locked 进入 unlocked，且某个 sealed target 的 public descriptor 位于 vault 外层
- **那么** 系统必须允许使用 vault 内绑定的 digest 或 manifest 校验该 descriptor 的完整性，并在发现不一致时返回明确的 tamper diagnostics

### 需求:standalone 配置必须规范化到 canonical vault 结构
standalone 的 core-owned 配置在实现 canonical vault runtime 后，必须能够表达 `builtin-encrypted`、`vault.unlock`、`vault.protectors` 和 `vault.ssh` 语义。legacy `[vault] backend = "os-native"` 只能作为兼容输入读取，不得继续作为规范化回写格式。

#### 场景:读取 legacy standalone 配置
- **当** standalone core 读取到 legacy `[vault] backend = "os-native"` 配置
- **那么** 系统必须在运行时将其归一化为 canonical vault + `os-native` protector 语义，并在后续持久化时回写 canonical 结构

#### 场景:standalone 配置声明 unlock policy
- **当** 操作员为 standalone core 配置 `trigger_policy`、`allowed_methods`、`preferred_method` 与 `cache_ttl_sec`
- **那么** 系统必须按这些字段驱动真实 vault lock/unlock 行为，而不是把它们当作未生效的文档字段

### 需求:standalone 的 vault 与 auth 管理入口必须复用 shared runtime truth
standalone 模式下的 `vault init/import/unlock` 与 `auth token create/revoke` 管理入口必须调用与桌面控制台相同的 canonical vault runtime 和 token authority，而不是维护独立的 CLI 私有逻辑。

#### 场景:standalone 解锁 vault
- **当** 操作员通过 standalone 管理入口执行 `vault unlock`
- **那么** 系统必须更新与桌面控制台共享的 vault lock state、protector 状态与审计真相，而不是只在 CLI 进程内临时记录一个解锁标记

#### 场景:standalone 创建长期 token
- **当** 操作员通过 standalone 管理入口执行 `auth token create`
- **那么** 系统必须复用 shared token authority、attestation enforcement 与 one-time reveal 语义，而不是实现另一套独立 token 存储或签发流程
