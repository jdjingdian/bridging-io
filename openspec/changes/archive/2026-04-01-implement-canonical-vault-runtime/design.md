## 上下文

本变更不是重新设计 vault / token / passkey / SSH 安全模型，而是把归档变更 `design-vault-and-agent-auth` 产品化。实现提案必须保留归档设计中的四条核心边界：

1. `builtin-encrypted` 才是 canonical secret store，protector 只负责解锁和重包裹
2. 高风险本地管理动作必须走 `Intent + Attestation`
3. secret 日常使用必须默认走 broker，而不是回退到通用 plaintext `get()`
4. desktop、standalone、trusted control-plane 和 runtime 必须共享一套安全真相源

当前实现已经有部分对象骨架，但仍是内存态与占位态：

- `SecretVaultRouter` 的 secret/version/wrap/intention/token 主要存在于进程内内存
- `os-native` readiness 是显式 `degraded`
- `unlock_vault` UI 入口已存在，但 control-plane command 尚未成形
- 长期 token 签发可写入 `issued_via_attestation_id`，但 runtime 还不会校验该 attestation

因此本提案的重点不是“再补一层 API”，而是把安全边界真正落到持久化、状态机和 control-plane enforcement。

```text
                   ┌──────────────────────────────┐
                   │  Trusted Local UI / CLI      │
                   │  create intent / verify UV   │
                   └──────────────┬───────────────┘
                                  │
                                  v
                   ┌──────────────────────────────┐
                   │  Vault/Auth Control Plane    │
                   │  intent + attestation gate   │
                   └───────┬──────────────┬───────┘
                           │              │
                           v              v
              ┌──────────────────┐   ┌──────────────────┐
              │ Protector Manager │   │ Token Authority  │
              │ os-native/passph. │   │ hash + scopes    │
              └─────────┬────────┘   └─────────┬────────┘
                        │                      │
                        v                      v
              ┌─────────────────────────────────────────┐
              │   Builtin Encrypted Vault Runtime       │
              │ secret records / version records / VRK  │
              └──────────────────┬──────────────────────┘
                                 │
                                 v
                    ┌────────────────────────────┐
                    │ Secret Broker + SSH Broker │
                    └────────────────────────────┘
```

## 目标 / 非目标

**目标：**

- 将 `builtin-encrypted` 落地为真正持久化的 canonical vault runtime
- 先用 `passphrase` 把跨平台、headless、standalone 的最小真实安全边界闭合，再把 `os-native` 接成 primary protector
- 将 `vault.unlock` 策略、locked mode 和 local admin attestation 变成 runtime 强制逻辑
- 统一 desktop 与 standalone 的受信任管理路径，避免 UI 可以做、CLI 不可做或反之的分叉
- 为 token issuance、scope change、secret reveal/export 和 SSH secret delivery 建立共享的 unlock / verification / audit 真相
- 明确 rollout 阶段、迁移路径、测试要求与平台绑定后续点

**非目标：**

- 在同一阶段内完成所有平台的 `os-native` 真正实现
- 在本次实现中引入 JWT、mTLS 或完整远端 delegation 生态作为前置条件
- 改写 OpenSSH 协议栈或引入自研 SSH transport
- 承诺在宿主已被 root / malware 控制时 vault-at-rest 仍然绝对安全

## 决策

### 决策 1：采用 `passphrase-first, canonical-vault-first` 的实施顺序

归档设计允许 `os-native` 与 `passphrase` 并存，但实现顺序不能反过来。建议：

- **第一阶段**：先实现 `builtin-encrypted + passphrase protector + locked mode + attestation enforcement`
- **第二阶段**：再把 `os-native` 接入为 primary protector

原因：

- `passphrase` 可以在 desktop、headless 和 standalone 上统一闭合最小真实安全边界
- 只做 `os-native` 无法解决 canonical store、迁移恢复和 headless 场景
- 一旦 canonical vault runtime 存在，`os-native` 的职责就被正确限制为 protector，而不是又变回“平台私有 store”

### 决策 2：配置与运行时真相统一为 canonical vault model

后续配置真相应从当前的：

```toml
[vault]
backend = "os-native"
namespace = "io.bridgingio"
```

升级为：

```toml
[vault]
backend = "builtin-encrypted"
namespace = "io.bridgingio"

[vault.unlock]
trigger_policy = "on-first-secret-access"
allowed_methods = ["os-native", "passphrase"]
preferred_method = "os-native"
cache_ttl_sec = 600
require_fresh_user_verification = true

[vault.protectors.primary]
kind = "os-native"

[[vault.protectors.recovery]]
kind = "passphrase"
kdf = "argon2id"
profile = "interactive-default"

[vault.ssh]
delivery_mode = "ssh-agent-broker"
fallback_delivery_mode = "ephemeral-identity-file"
```

兼容策略：

- 读取 legacy `[vault] backend = "os-native"` 时，runtime 应解释为：
  - `backend = "builtin-encrypted"`
  - `primary protector = os-native`
- 读取 legacy `vault:ssh-key:ops` 时，应归一化为 canonical `vault://...`
- 配置一旦被重新持久化，必须回写 canonical 结构，而不是继续输出 legacy 语义

### 决策 3：target 配置采用 `public descriptor + sealed overlay` 分层，并将存储方式与访问权限解耦

target 配置不能只按“是否放进 vault”建模，否则会把兼容性和访问控制绑死在同一个开关上。本变更将 target 配置拆成两个正交维度：

- `storage_class`
  - `plain`
  - `sealed-overlay`
  - `sealed-full`
- `access_class`
  - `anonymous-local`
  - `token-scoped`

语义：

- `plain`
  - 全量 target 配置保留在 `config.toml`
  - 适合低敏或旧版本兼容 target
- `sealed-overlay`
  - `config.toml` 仅保留 public descriptor 与 `sealed_profile_ref`
  - host、username、selector、notes、policy、`credential_ref` 等敏感字段进入 vault
- `sealed-full`
  - `config.toml` 只保留最小索引项
  - 其余配置全部依赖 vault

public descriptor 至少包括：

- `target_id`
- `display_name`
- `aliases`
- `kind`
- `enabled`
- `storage_class`
- `access_class`
- 可选 `sealed_profile_ref`

敏感 overlay 可包括：

- host / port / username
- selector kind / selector value
- notes
- target-scoped policy
- toolchain override
- `credential_ref`

兼容策略：

- legacy target 默认映射为 `storage_class = plain`
- legacy target 默认映射为 `access_class = anonymous-local`
- 该默认值只服务于本地 loopback 兼容，不意味着 non-loopback 或发布模式自动开放匿名访问

显示与完整性规则：

- model-plane / UI 返回的是 runtime public projection，而不是原始 `config.toml`
- 对 `sealed-*` target，未解锁时允许只返回最小 descriptor 或 redacted summary
- 若 public descriptor 位于 vault 外层，其完整性默认视为“解锁前未验证”
- 解锁后可用 vault 内绑定的 digest / manifest 校验 descriptor 是否被篡改

### 决策 4：loopback 匿名兼容 principal 是显式兼容模式，而不是 `auth_mode=none` 的别名

为兼容旧版本升级路径，系统需要允许“无 token 仍可执行 plain target”的产品路径，但该能力必须作为显式、受限、仅限 loopback 的 principal 语义存在，而不是把认证模型重新退回到“默认全匿名”。

规则：

- 请求带有 `Authorization: Bearer ...`
  - 必须先完成 token 校验
  - 若 token 无效、过期或 scope 不允许，必须直接拒绝
  - 不得回退到匿名 principal
- 请求未带 token
  - 仅在 loopback 且操作员显式启用匿名兼容模式时，可映射为 `anonymous-local` principal
  - 该 principal 只允许访问 `access_class = anonymous-local` 的 plain target 与其公开 catalog
- non-loopback 请求未带 token
  - 一律拒绝

这样可以同时满足：

- 旧版本升级后，不配置 token 仍能继续访问原有 plain target
- 新增的 sealed target / vault target 不会因为缺 token 被匿名执行
- future capability-aware-mcp 的 token scope 仍保持正式真相语义

### 决策 5：canonical vault store 采用 `metadata db + blob store` 双层持久化

建议 vault 的磁盘布局固定为：

```text
<data_dir>/vault/
  metadata.sqlite
  blobs/
    secret/
    wrap/
    salt/
```

其中：

- `metadata.sqlite`
  - 保存 `VaultSecretRecord`
  - 保存 `VaultSecretVersionRecord`
  - 保存 `VaultKeyEnvelopeRecord`
  - 保存 `ProtectorWrapManifest`
  - 保存 `AgentTokenRecord`
  - 保存 `TokenScopeRecord`
  - 保存 `LocalAdminActionIntent`
  - 保存 `LocalAdminAttestationRecord`
  - 保存后续 `PasskeyCredentialRecord`
- `blobs/`
  - 保存 secret ciphertext
  - 保存 wrapped DEK / wrapped VRK material
  - 保存 passphrase salt 与版本化 envelope blob

显示安全规则：

- control-plane / UI 默认只读取 display-safe projection
- `ciphertext_locator`、wrapped key locator、salt locator、digest、local session binding 都属于 internal-only

### 决策 6：加密与 protector 分层固定为 `VRK -> DEK -> ciphertext`

实现模型：

- 初始化 vault 时生成随机 `VRK`
- 每个 secret version 导入时生成随机 `DEK`
- secret plaintext 由 `DEK` 使用 AEAD 加密
- `DEK` 再由 `VRK` 使用 versioned envelope 包裹
- `VRK` 只以 protector-wrapped 形式持久化

推荐算法：

- secret ciphertext AEAD：`XChaCha20-Poly1305`
- DEK/VRK envelope：同样使用版本化 AEAD envelope
- passphrase KDF：`Argon2id`

关联数据至少绑定：

- `reference`
- `version_id`
- `content_format`
- envelope format version

这使得：

- rotation 不必重写历史 reference
- rewrap protector 不必重加密全部 secret
- future native protector integration 不会改变 `CredentialRef`

### 决策 7：unlock policy 必须变成真实 runtime 状态机

需要显式的 vault runtime state：

```text
uninitialized -> locked -> unlocking -> unlocked
                    |             |
                    v             v
               unavailable <------+
```

语义：

- `uninitialized`
  - canonical vault 尚未建立
- `locked`
  - vault 已存在，但当前未解锁
- `unlocking`
  - 正在等待 passphrase、passkey 或 native protector
- `unlocked`
  - VRK 已进入受控内存缓存
- `unavailable`
  - 当前策略要求的 protector 不可满足，必须 fail closed

所有 secret-backed 操作在进入 broker / token high-risk admin / SSH key delivery 前，都必须先经过：

```text
check vault state
-> if unlocked: continue
-> if locked and trigger allows interactive unlock: start trusted unlock flow
-> if locked and trigger disallows auto-unlock: reject with diagnostic
-> if unavailable: fail closed
```

### 决策 8：trusted local admin control plane 采用双阶段握手

本地高风险管理动作必须按统一 ceremony 执行：

```text
request action
  -> create intent
  -> perform local verification
  -> mint attestation
  -> consume attestation for exactly one action
  -> write audit event
```

建议正式 control-plane command 至少包括：

- `create_local_admin_intent`
- `complete_local_admin_attestation`
- `unlock_vault`
- `list_vault_secrets`
- `import_vault_secret`
- `rotate_vault_secret`
- `reveal_vault_secret`
- `export_vault_secret`
- `create_agent_token`
- `update_agent_token_scope`
- `revoke_agent_token`

强制规则：

- `unlock_vault` 不再接受“只要来自 trusted host 就算通过”的弱语义
- `create_agent_token` / `update_agent_token_scope` 不再接受前端伪造的固定 attestation id
- attestation 必须匹配：
  - intent id
  - action kind
  - target object
  - payload digest
  - principal
  - freshness window

### 决策 9：desktop 与 standalone 必须共享同一套 handler

不允许出现：

- desktop 用 UI 专用逻辑签发 token
- standalone 用 CLI 专用逻辑解锁 vault
- runtime 内部再维护第三套 token/vault 内存态

建议结构：

- `bridgingio-secrets`
  - canonical objects、crypto、protector、unlock gate、token authority
- `bridgingio-mcp`
  - App API / control-plane command dispatch
- `bridgingio-engine`
  - config parsing、canonical rewrite、runtime path ownership
- desktop / standalone
  - 只负责 trusted input ceremony 与 command transport

这样 UI 和 CLI 的差异只体现在“如何获得本地用户输入”，而不是“如何操作 vault/token 真相”。

### 决策 10：长期 token authority 必须与 attestation 和 scope versioning 一起落地

当前 token 逻辑已经有 `AgentTokenRecord`、`TokenScopeRecord` 和 hash-only 存储骨架，但还没把高风险签发强制绑定到 attestation。实现时应固定以下规则：

- 创建长期 token
  - 必须要求 fresh attestation
- 扩大 token scope
  - 必须要求 fresh attestation
- revoke token
  - 仍在 trusted control-plane 中完成；是否 fresh UV 由策略决定
- token 明文
  - 只在签发瞬间显示一次
- token 查询
  - 只能返回 summary

哈希策略建议：

- bearer token 本身为高熵随机值
- 服务端只保存 approved digest，例如 `sha256`
- 不把 password KDF 用于 token digest；高熵 token 没有必要承受额外计算成本

### 决策 11：SSH secret delivery 与 canonical vault 联动发布

SSH 路线不能继续独立于 vault 产品化。建议分两层推进：

- **第 1 层**
  - secret-backed SSH 调用必须要求 vault 已解锁
  - 当前 agent broker/session 语义继续保留
  - identity-file fallback 仅在显式 degraded 模式下允许
- **第 2 层**
  - 将 SSH secret delivery 固化为真正的 ephemeral ssh-agent endpoint
  - connector 优先走 structured exec，而不是 host shell flatten

禁止事项：

- 不允许为了“先跑通”而把私钥重新带回 argv、普通 env、artifact 或通用日志
- 不允许把 runtime key passphrase prompt 引回普通数据面

### 决策 12：`os-native` readiness 只在真实 binding 完成后才能报告 `ready`

当前 `os-native` 是显式 degraded memory shim，这个诊断是正确的。实现阶段要防止另一个坏结果：把 capability health 文案提前改成 `ready`，而真实 binding 仍未完成。

因此：

- `os-native = ready`
  - 只有在真实平台 protector 可完成 unwrap/rewrap 时成立
- `os-native = fallback`
  - 主 protector 不可用，但 `passphrase` recovery 满足策略
- `os-native = degraded`
  - 仅限 dev/test shim
- `os-native = unsupported`
  - 当前宿主没有实现

## 分阶段实施

### Phase 1：Canonical Vault Core

- 持久化 metadata/db/blob store
- `VRK -> DEK -> ciphertext` productization
- canonical config and legacy migration
- `passphrase` protector + `Argon2id`
- real vault state machine and unlock gate

交付后应达到：

- 重启后 secret/version/wrap/token 仍存在
- locked vault 时 secret-backed 能力 fail closed
- standalone 与 desktop 至少都能走 `passphrase` 路线

### Phase 2：Trusted Local Admin Control Plane

- `Intent + Attestation` command surface
- `unlock_vault` productization
- token issuance/scope/reveal/export 强制消费 attestation
- desktop UI 接真实 unlock/token 流

交付后应达到：

- 没有真实 attestation 就不能签发长期 token
- `unlock_vault` 不再 deferred

### Phase 3：Token Authority Hardening

- `AgentTokenRecord` / `TokenScopeRecord` 完整版本化
- one-time reveal 与 summary-only list/query
- parent lineage 与 delegated run token 预留

### Phase 4：SSH Secret Delivery Productization

- unlock-gated SSH broker
- structured exec overlay
- degraded fallback diagnostics

### Phase 5：Native Protector Rollout

- macOS / Windows / OpenHarmony `os-native` binding
- readiness matrix 切换到真实实现状态

### Phase 6：Standalone Management Surface

- `vault init/import/unlock`
- `auth token create/revoke`
- secret input source priority and audit semantics

## 数据迁移

### 配置迁移

- 旧配置：
  - `[vault] backend = "os-native"`
- 新配置：
  - `[vault] backend = "builtin-encrypted"`
  - `[vault.unlock]`
  - `[vault.protectors.*]`
  - `[vault.ssh]`

迁移策略：

- 首次读取时做内存归一化
- 成功持久化设置时回写 canonical 形式
- 若当前实例尚未初始化 canonical vault，则允许以 legacy 配置启动迁移向导

### 数据迁移

- 现有内存态 router 不承诺生产级持久化迁移
- dev/test shim 数据只作为开发数据，不保证自动迁移
- canonical vault 一旦初始化，应写入 format version，后续走版本化迁移步骤

## 测试与验收

### self-test

必须新增或扩展以下 smoke：

- canonical `vault://...` normalization
- legacy `backend = os-native` canonical rewrite diagnostics
- initialized vault locked -> fail closed
- passphrase unlock success / wrong passphrase / KDF diagnostics
- real attestation required for long-lived token issuance
- attestation single-use enforcement
- SSH broker basic lifecycle on unlocked vault

### integration / contract

- control-plane settings must expose:
  - `vault.state`
  - `configured_backend`
  - protector summary
  - unlock policy summary
- desktop UI test must cover:
  - locked state projection
  - unlock flow
  - token issue with one-time reveal
  - reject synthetic attestation

## 风险与待确认问题

- `os-native` 的具体平台 binding 方案仍需分别落到 macOS、Windows、OpenHarmony 适配层
- canonical vault 选择 SQLite + blob store 时，需要确认并发策略与 crash recovery 细节
- `PasskeyCredentialRecord` 可以先落 metadata schema 和 policy surface，真实平台接入可分阶段完成
- delegated run token 可以先保留 schema 与 lineage 字段，实际发行入口按后续风险评估再打开

## 边界总结

本提案要求项目先把 vault 变成真正存在的产品能力，再继续扩展 UI、native binding 或更多 token/SSH 细节。实现顺序的核心原则是：

- 先 canonical runtime
- 再 trusted local enforcement
- 再平台便利性

如果跳过前两步，后续所有“安全功能”都会继续建立在 degraded shim 之上。
