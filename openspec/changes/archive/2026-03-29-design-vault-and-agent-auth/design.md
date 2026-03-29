## 上下文

BridgingIO 讨论的 `vault` 不只是“找个地方把 secret 存起来”。这次要解决的是三条必须同时成立的链路：

1. 用户自己的高价值秘密如何保存：SSH 私钥、第三方 API token、未来签名密钥
2. agent 如何被授权访问 model-plane：谁可以连到 MCP HTTP、权限边界是什么、如何撤销
3. secret 在实际使用时如何不从配置、日志、artifact、命令行参数、host shell 或模型输出中重新泄露

如果只做存储而不设计“如何使用”和“谁能访问”，vault 很容易退化成一个把秘密从配置文件搬到另一个位置的薄封装。

```text
                    ┌────────────────────────────┐
                    │   Local UI / CLI Admin     │
                    │ issue token / unlock vault │
                    │ reveal / approve           │
                    └─────────────┬──────────────┘
                                  │ user verification
                                  ▼
                    ┌────────────────────────────┐
                    │  Auth + Vault Control      │
                    │  passkey / os-native /     │
                    │  passphrase protector      │
                    └─────────────┬──────────────┘
                                  │
             ┌────────────────────┴────────────────────┐
             ▼                                         ▼
┌────────────────────────────┐            ┌────────────────────────────┐
│ Built-in Encrypted Vault   │            │ Model Plane Agent Auth     │
│ canonical secret store     │            │ token hash / scopes /      │
│ stable refs / versions     │            │ principal / revocation     │
└─────────────┬──────────────┘            └─────────────┬──────────────┘
              │                                         │
              ▼                                         ▼
┌────────────────────────────┐            ┌────────────────────────────┐
│ Secret Broker              │            │ MCP / Typed Tools          │
│ ssh / http / sign / reveal │            │ access scope / approvals   │
│ redaction / audit          │            │ runtime isolation          │
└─────────────┬──────────────┘            └────────────────────────────┘
              │
              ▼
┌────────────────────────────┐
│ SSH Agent Broker /         │
│ HTTP Auth Injection /      │
│ future signer adapters     │
└────────────────────────────┘
```

## 目标 / 非目标

**目标：**

- 形成一套不会把 secret 明文重新泄露到配置、日志、artifact、shell 命令字符串或模型输出的统一安全架构
- 为 macOS、Windows、OpenHarmony PC 保留 passkey / 平台用户验证接入位，同时不把 headless standalone 场景直接堵死
- 让 `CredentialRef` 成为稳定引用，而不是后端实现细节
- 让 model-plane 具备真正的 agent 认证与授权能力，并允许用户显式签发长期或时效 token
- 保持与 OpenSSH 的正常功能兼容，而不是用一个功能残缺的“自研私钥注入技巧”破坏现有 SSH 行为
- 明确 standalone 的 future route，但不在本次变更里要求立即实现

**非目标：**

- 本次直接实现完整 vault、token、passkey 或 standalone CLI
- 在完全被 root / malware 控制的宿主机上承诺“绝对不泄露”
- 用自研 SSH 协议栈替换 OpenSSH
- 强迫所有日常 agent 请求都经过 passkey challenge

## 决策

### 决策 1：以内置加密 vault 作为 canonical secret store，protector 负责解锁而不直接充当全部真相层

BridgingIO 后续的 secret 真相层应当是 `builtin-encrypted vault`，而不是把每个平台自己的钥匙串直接当成唯一存储模型。原因如下：

- 平台原生安全存储在 macOS、Windows、Linux、OpenHarmony PC 上的对象模型和 headless 语义并不一致
- 未来需要统一支持版本化、rotation、metadata、audit 和跨后端迁移，仅靠平台钥匙串很难形成稳定产品语义
- standalone 内网 / 服务器场景需要 passphrase 或其他 unlock protector 路线，不能假设永远有 GUI keychain

推荐分层：

- `builtin-encrypted`：加密保存 secret blob、元数据、版本与状态，作为 canonical store
- `os-native`：保护 vault 主密钥或 unwrap material
- `passphrase`：headless / standalone 的 fallback protector
- 未来可扩展其他 protector，但都不改变外部 `CredentialRef` 语义

这意味着：

- `os-native` 不能再是 today 的 memory shim
- 如果当前平台没有可用 protector，系统必须 fail closed 或进入明确的受控降级，而不是继续伪装成安全存储

### 决策 1.1：vault 需要稳定的 secret record / version record，而不是“一个引用对应一段无结构明文”

为了支持稳定引用、rotation、审计和后续迁移，vault 至少需要两层持久化对象：

- `VaultSecretRecord`
  - 描述一个稳定 secret 引用及其管理元数据
- `VaultSecretVersionRecord`
  - 描述该 secret 的某个具体加密版本

推荐对象形状如下：

```text
VaultSecretRecord
- reference: String                // canonical vault://...
- namespace: String
- kind: String                     // ssh-private-key / opaque-token / signing-key / ...
- name: String
- label: String
- status: active | disabled | scheduled_delete | deleted
- active_version_id: Option<String>
- usage_policy: String             // ssh-auth / http-auth / model-plane-auth / signing
- export_policy: String            // disallow / local-admin-only / temporary-export
- created_by: String
- created_at: SystemTime
- last_used_at: Option<SystemTime>
- last_rotated_at: Option<SystemTime>
- notes: Option<String>
```

```text
VaultSecretVersionRecord
- version_id: String
- reference: String
- version_seq: u32
- state: pending | active | superseded | revoked | destroyed
- protector_binding: String        // os-native / passphrase / future protector id
- ciphertext_locator: String       // blob path / content address / row id
- ciphertext_digest: String
- content_format: String           // openssh-key / opaque-token / pem / signer-material
- import_method: String            // file / stdin / generated / migrated
- created_by: String
- created_at: SystemTime
- activated_at: Option<SystemTime>
- superseded_at: Option<SystemTime>
- revoked_at: Option<SystemTime>
- destroy_after: Option<SystemTime>
```

有意不把“secret 明文摘要”作为对外元数据暴露，以避免后续有人把可比对的 plaintext hash 当成旁路索引乱用。需要完整性时，优先记录 ciphertext digest 或包裹后的 manifest digest。

### 决策 1.2：rotation 必须是“稳定引用 + 版本切换”，而不是重命名或覆盖旧值

secret rotation 后续应遵循固定状态机：

```text
new version imported -> pending
pending version verified/promoted -> active
previous active -> superseded
superseded -> revoked or destroyed (possibly after grace period)
```

关键约束：

- `reference` 不变
- 同一时刻最多一个 `active_version_id`
- 审计必须能看见是谁导入了新版本、何时切换、何时撤销旧版本
- `ssh-private-key`、`opaque-token`、`signing-key` 可以有不同的 grace policy，但都不能靠“直接覆盖旧值”实现 rotation

这样才能兼容：

- target profile 不改 ref
- token 签发和 connector 仍用稳定 reference 找到当前 active version
- 出现回滚或兼容问题时还能定位和切回上一版本

### 决策 1.3：vault 加密层采用“vault root key + per-version DEK + versioned envelope”两层封装

为了同时支持跨平台 protector、secret version rotation 与后续的 protector rewrap，推荐把 `builtin-encrypted vault` 固定为两层密钥模型：

```text
protector
  -> unwrap vault root key (VRK)
      -> unwrap per-version DEK
          -> decrypt secret ciphertext blob
```

推荐语义：

- 初始化 vault 时生成随机 `vault root key`
- 每个 `VaultSecretVersionRecord` 导入时都生成新的随机 `DEK`
- secret 明文只由该版本自己的 `DEK` 加密
- `DEK` 再由 `vault root key` 包裹
- `vault root key` 只以“被 protector 包裹后的形式”持久化

这样带来的好处：

- **轮换 secret version**：只影响该版本自己的 `DEK + ciphertext`
- **轮换 protector**：通常只需重包裹 `vault root key`，不必重加密全部 secret
- **迁移 protector 实现**：不改变 `CredentialRef`
- **后续引入多 protector**：可以为同一个 vault root key 维护多个 wrap manifest，而不改变 secret record 结构

推荐补一个内部对象：

```text
VaultKeyEnvelopeRecord
- vault_key_id: String
- state: active | rewrapping | retired | destroyed
- active_wrap_set_id: String
- created_at: SystemTime
- rotated_at: Option<SystemTime>
- last_unlocked_at: Option<SystemTime>
```

```text
ProtectorWrapManifest
- wrap_id: String
- vault_key_id: String
- protector_binding: String
- wrap_format: String
- wrapped_key_locator: String
- wrapped_key_digest: String
- created_at: SystemTime
- last_verified_at: Option<SystemTime>
- status: ready | fallback | degraded | unavailable | retired
```

加密信封还应满足：

- 使用版本化 envelope format，方便后续算法升级或参数迁移
- AEAD 的 associated data 至少绑定 `reference`、`version_id`、`content_format`
- `VaultSecretVersionRecord` 持有的是 `ciphertext` 与被 root key 包裹后的 `DEK` 引用，而不是未包裹材料

### 决策 1.4：protector 负责解锁和重包裹，不负责替代 vault 的产品语义

protector 的职责应严格限定为：

- 帮助 unwrap `vault root key`
- 参与 protector rotation / rewrap
- 提供本地用户验证或设备在场语义
- 输出 readiness / degraded / unavailable 诊断

而不应负责：

- 直接充当 secret record 的真相层
- 替代 version / rotation / audit 语义
- 决定 `CredentialRef` 形状

推荐把 protector 装配语义分成三类：

- `primary protector`
  - 当前策略首选的 unlock 路线，例如 `os-native`
- `recovery protector`
  - 用于 headless / break-glass / standalone 的备用 unlock 路线，例如 `passphrase`
- `retired protector`
  - 仅保留审计与迁移语义，不再参与正常解锁

主密钥解锁缓存建议：

- 解锁成功后，仅在当前进程内缓存 `vault root key`
- 缓存应绑定短 TTL、受控内存容器与显式 lock / shutdown 清理
- root key 缓存过期后，后续 secret 使用必须重新走 unlock
- 解锁缓存不应被序列化到磁盘、普通共享内存或通用日志

### 决策 1.4.1：`passphrase` 应作为恢复 / 迁移 protector，并使用 memory-hard KDF

对于允许备份恢复与跨设备迁移的场景，`passphrase` 不应被视为“低配备用解锁方式”，而应被明确建模为：

- `recovery protector`
  - 当 `os-native` 不可用时用于恢复
- `migration protector`
  - 用于把 vault 数据迁移到新宿主
- 可选的双 protector 之一
  - 与 `os-native` 并存，而不是互斥

推荐链路：

```text
passphrase
  + random salt
  + Argon2id(parameters)
    -> KEK
      -> unwrap vault root key
        -> unwrap per-version DEK
          -> decrypt secret ciphertext
```

关键语义：

- `passphrase` 不直接加密全部 secret blob
- `passphrase` 只用于导出一个 `KEK`
- `KEK` 只负责包裹或解包 `vault root key`
- 更换 passphrase 时，通常只需重包裹 `vault root key`，不必重加密全部 secret version

这样可以同时满足：

- vault 数据允许备份
- 文件可迁移到另一台机器
- 单独拷走 vault 文件时，攻击者仍需面对离线口令猜解成本

### 决策 1.4.2：`Argon2id` 作为 passphrase protector 的首选 KDF，参数必须版本化并可升级

对于 `passphrase` protector，推荐首选 `Argon2id`，而不是继续采用普通哈希或仅 CPU-bound 的旧式 KDF。原因：

- `Argon2id` 兼顾抗 GPU / ASIC 并行爆破与实现成熟度
- 更适合“文件已被拷走后的离线攻击”这一威胁模型
- 支持通过参数升级逐步提高防护强度，而不破坏 vault 外部引用语义

推荐记录一类内部参数对象：

```text
PassphraseProtectorParams
- kdf_scheme: String              // argon2id
- kdf_version: String
- memory_cost_kib: u32
- time_cost: u32
- parallelism: u32
- salt_locator: String
- salt_digest: String
- created_at: SystemTime
- upgraded_at: Option<SystemTime>
```

实现约束：

- 每个 `passphrase` wrap set 必须使用随机 salt
- salt、KDF 参数、格式版本可以明文存放于 metadata store
- 真正敏感的是 passphrase 本身、派生后的 `KEK`、解锁后的 `vault root key`
- 参数必须支持升级；当策略提高成本时，应允许在下次本地成功解锁后执行 rewrap

默认策略建议：

- 初始参数目标应校准到“合法用户单次解锁约数百毫秒到约 1 秒”
- 具体值按平台能力调整，而不是把某一组数字写死为永远不变的常量
- 若当前设备资源过弱导致无法满足最低 memory cost，系统应明确进入 degraded/fallback diagnostics，而不是静默降低到过弱参数

### 决策 1.4.3：对“文件被拷走”的风险边界，系统应当明确承诺到“抗离线解密”，而不是“绝对不可恢复”

在支持备份恢复与迁移的前提下，BridgingIO 对 passphrase vault 的安全承诺应明确为：

- 攻击者**仅拿到 vault 数据文件**
  - 不应直接恢复 token / 私钥明文
- 攻击者拿到 vault 文件 + metadata + KDF 参数
  - 仍应只能进行离线口令猜测
- 攻击者若还拿到正确 passphrase，或已控制运行中的宿主与进程内存
  - 则不再属于 vault-at-rest 可单独解决的问题

也就是说，系统目标是：

- **保护 secret 明文**
- **允许备份恢复 / 迁移**
- **接受 passphrase 模式天然存在离线猜测风险，并通过 memory-hard KDF + 强口令策略把攻击成本抬高**

而不是声称：

- “只要文件被拷走也永远不可能恢复”

### 决策 1.4.4：`os-native` 与 `passphrase` 可以并存，分别服务便利性与可恢复性

推荐后续实现支持同一 vault 同时配置：

- `os-native primary protector`
  - 日常本机解锁体验更好
- `passphrase recovery protector`
  - 用于备份恢复、设备迁移、headless break-glass

在这种组合下：

- 日常本机使用优先走 `os-native`
- 迁移或恢复时允许显式选择 `passphrase`
- 移除某个 protector 时，不应影响 `CredentialRef` 与 secret version 结构
- 审计必须能看见是谁添加、轮换或移除了 `passphrase` protector

### 决策 1.4.5：vault 解锁必须区分“何时触发”与“允许哪种本地方法”

为了支持你提到的“每次 core 启动都要求解锁”与“每次访问 secret 才解锁”的差异，推荐把 vault 解锁策略拆成两层，而不是只保留一个 `unlock mode`：

- `unlock trigger policy`
  - 何时要求本地解锁
- `unlock method policy`
  - 本次解锁允许使用哪些本地方法，以及优先顺序

推荐对象：

```text
VaultUnlockPolicy
- trigger_policy: String          // on-core-start / on-first-secret-access / on-every-secret-access / manual-only
- allowed_methods: Vec<String>    // os-native / passkey / passphrase / controlled-fallback
- preferred_method: Option<String>
- cache_ttl_sec: Option<u64>
- start_locked: bool
- require_fresh_user_verification: bool
```

推荐触发语义：

| trigger_policy | 语义 |
| --- | --- |
| `on-core-start` | core 启动后进入 locked 状态，secret-backed 能力在成功解锁前不得执行 |
| `on-first-secret-access` | core 可先启动，但第一次真正访问 vault / secret broker 时阻塞到本地解锁完成 |
| `on-every-secret-access` | 每次 secret 使用都要求重新满足解锁条件或零缓存窗口 |
| `manual-only` | 仅在本地管理员显式执行 unlock 后允许 secret use |

推荐方法语义：

- `passphrase-only`
  - 适合 headless 或严格口令门槛场景
- `passkey-preferred`
  - UI / desktop 有本地 passkey 时优先使用
  - 若策略允许，也可回退到 `passphrase`
- `os-native-or-passphrase`
  - 本机便利性与迁移恢复并存
- `passkey-or-passphrase`
  - 本地用户在场优先，但仍保留可迁移恢复口令

关键边界：

- `trigger policy` 决定“什么时候必须解锁”
- `method policy` 决定“这次解锁可接受什么本地证明”
- 两者都不改变 model-plane 的 bearer token 身份语义
- 也就是说，vault 解锁是本地 secret use gate，不是远端 agent 的数据面身份来源

### 决策 1.4.6：`passphrase` 可以作为常规本地解锁因子，而不仅是恢复口令

前面的设计把 `passphrase` 先定位为 recovery / migration protector，但这并不意味着它只能在灾难恢复时出现。后续实现应允许它成为**常规本地解锁因子**，例如：

- 每次 core 启动都要求输入 passphrase
- 第一次访问 secret 时要求输入 passphrase
- 高安全模式下每次 secret access 都要求重新输入 passphrase

同时要保持两个澄清：

- `passphrase` 证明的是“本地有人掌握这项解锁因子”
  - 它更像 unlock factor，而不是高保真的个人身份标识
- `passkey` / platform authenticator
  - 更适合作为本地用户在场验证和便捷解锁方法
  - 但它仍然不是远端 agent 的 bearer 身份

推荐默认策略：

- desktop / UI 默认
  - `trigger_policy = on-first-secret-access`
  - `preferred_method = passkey` 或 `os-native`
  - 保留 `passphrase` 作为 recovery / fallback
- 高安全桌面模式
  - `trigger_policy = on-core-start` 或 `on-every-secret-access`
  - `allowed_methods` 包含 `passphrase`
- headless / standalone 模式
  - `allowed_methods = [passphrase]` 或 `passphrase` 优先

### 决策 1.4.7：当策略要求启动即解锁时，core 应当以“有限 locked 模式”启动

如果配置了 `trigger_policy = on-core-start`，推荐的运行语义不是“进程完全起不来”，而是：

- core 可以以 `locked` 状态启动
- 本地受信任 control-plane / UI / CLI 仍可访问最小管理面
- 所有 secret-backed 能力必须在 unlock 完成前拒绝执行
- diagnostics 必须清楚区分：
  - `process is running`
  - `vault is locked`
  - `secret-backed operations are unavailable until local unlock`

这样可以兼顾：

- 启动时需要本地用户在场
- 不把“未解锁”误判成“core 启动失败”
- UI 可在启动后立刻发起 passkey / passphrase 解锁流程

### 决策 1.5：vault / protector 必须有 readiness 矩阵与 fail-closed 语义

后续不能再把“平台 keychain 还没做完”的状态伪装成正常安全后端。推荐固定以下矩阵：

| 场景 | 结果 |
| --- | --- |
| 已初始化 vault，且存在可用 `primary` 或允许的 `recovery protector` | 允许 unlock |
| 已初始化 vault，但没有任何允许的可用 protector | fail closed；禁止 secret use |
| 未初始化 vault，但策略允许创建时使用当前 protector | 允许 init |
| 仅存在显式 dev/test 的 insecure shim | 只能进入受控测试模式，不得视为生产可用 secret store |

推荐把 capability diagnostics 统一到以下状态：

- `ready`
  - 当前 protector 可满足策略要求
- `fallback`
  - 主 protector 不可用，但存在策略允许的 recovery 路线
- `degraded`
  - 仅适用于显式受控的测试或兼容模式，不能被当成正式安全后端
- `unsupported`
  - 当前宿主没有可用实现

需要特别收紧的规则：

- current 的 memory shim 只能在显式 dev/test 下保留为诊断或探针路径，不得继续承担“长期持久化 secret”的正式语义
- 如果 vault 处于 `locked + no usable protector`，系统必须拒绝 broker 使用、SSH 连接和 token 签发，而不是静默回退到明文配置或临时内存 copy
- rewrap / protector rotation 必须是显式管理动作，并进入本地用户验证与审计链路

对 `passphrase` 路线还应补充两条：

- 如果策略要求 `passphrase` 作为 recovery protector，则系统必须检查其 KDF 参数是否达到当前最小安全基线
- 当 vault 数据被拷走但宿主 protector 不可用时，系统不应把这种情况误判为 secret 已泄露；正确诊断应是“仍受 passphrase / protector 保护，但存在离线攻击面”

### 决策 2：凭据引用采用 canonical URI，并支持 legacy alias 归一化

为了避免当前仓库里 `vault://...` 与 `vault:...` 混用的问题，后续 canonical `CredentialRef` 应统一为：

```text
vault://<namespace>/<kind>/<name>
```

示例：

- `vault://bridgingio/ssh-private-key/lab`
- `vault://bridgingio/opaque-token/grok-default`
- `vault://bridgingio/model-plane-token/codex-lab`

同时允许输入层接受 legacy alias，例如：

- `vault:ssh-key:ops-prod`
- `vault:grok-token:default`

但系统在保存、审计、诊断和配置回写时必须输出 canonical URI。

secret rotation 不应改变 reference 本身，而应通过内部 version / active pointer 管理：

- reference 稳定
- active version 可切换
- audit 能看到历史

### 决策 3：默认走 secret broker，而不是暴露通用 plaintext get

后续 vault API 不应该继续围绕“`get(secret) -> String`”展开。对 model-facing、provider-facing、connector-facing 路径，推荐语义应当是：

- `use_for_ssh(reference, target_scope)`
- `use_for_http_auth(reference, request_scope)`
- `sign(reference, payload)`
- `mint_agent_token(policy)`
- `reveal_for_local_admin(reference)`

这样做的原因：

- 可以把 secret 使用点收敛到少数受控入口
- redaction、audit、approval、user verification 可以围绕 broker 做统一治理
- provider / connector 不需要到处拿一段明文再自己决定怎么注入

broker 必须配套这些规则：

- 禁止把 secret 明文写入 Artifact
- 禁止把 secret 明文返回给模型
- 默认禁止在 runtime logger 中记录可能含 secret 的原始参数
- 解锁到内存的 secret 只允许进入 `SecretString` / `zeroize` 等专用容器
- 对已知 active secret 建立 redaction registry，在日志、stderr、artifact 捕获阶段自动脱敏

### 决策 4：SSH 私钥交付优先使用临时 ssh-agent broker，避免 argv / env / 配置文件泄露

对于 SSH target，私钥的主要交付路径不应是：

- `ssh -i /tmp/key`
- `IDENTITY=... ssh`
- 在 shell 命令字符串里拼 PEM

这些方式都容易出现在：

- `ps`
- shell history
- debug log
- 临时文件残留
- artifact 捕获

推荐主路径：

```text
Vault -> Secret Broker -> Ephemeral SSH Agent Broker -> OpenSSH
```

运行机制：

- 连接建立前，BridgingIO 从 vault 解锁对应 SSH 私钥到受控内存
- 仅为当前 transport session / interactive shell 创建临时 agent broker
- `ssh` 通过 `SSH_AUTH_SOCK`、`IdentityAgent` 或平台等价机制向 agent 请求签名
- session 结束后销毁 agent、清理 socket / pipe、擦除内存材料

这样有几个好处：

- 私钥不需要落盘
- `ssh` 仍然沿用 OpenSSH 正常 publickey auth 语义
- host key verification、ProxyJump、常规一跳 / 多跳、interactive / one-shot 流程通常不受影响

受控 fallback：

- 如果某个平台暂时没有 agent-compatible delivery，可允许 `ephemeral identity file` 作为 degraded fallback
- fallback 必须：
  - 使用 0600 私有文件权限
  - 放在私有运行目录
  - 生命周期严格绑定到本次连接
  - 在 diagnostics 中明确标记为 degraded

需要额外记录的前置改造：

- 当前 connector 仍会把 structured invocation flatten 成 host shell 字符串，这对 secret-aware execution 不够安全
- 后续实现必须优先推动 `program + args + env overlay` 级别的 structured execution，减少 `sh -lc '...'` 这一层在 secret 交付链路中的参与

### 决策 4.1：SSH agent broker 必须是连接级本地 signer endpoint，而不是全局共享代理

后续建议引入 `SshAgentBrokerSession` 作为显式对象，而不是把 agent broker 当成某种进程级全局状态：

```text
SshAgentBrokerSession
- broker_session_id: String
- target_id: String
- credential_ref: String
- state: preparing | ready | attached | draining | closed | failed
- endpoint_kind: String            // unix-socket / named-pipe / platform-local-endpoint / identity-file-fallback
- degraded: bool
- created_at: SystemTime
- expires_at: Option<SystemTime>
- attached_channel_count: u32
- cleanup_deadline: Option<SystemTime>
```

设计约束：

- 一个 broker session 默认只服务于一个逻辑 target session 或一个 interactive shell 生命周期
- 不同 principal、不同 target、不同 key ref 默认不得复用同一 broker session
- 对于一条逻辑连接链中的多次 `ssh` 调用，例如 `ProxyJump` 或同一 interactive shell 下的多次连接，可在同一父会话内受控复用同一 broker session
- broker endpoint 必须是 local-only，且创建在私有运行目录或平台等价的私有 IPC 命名空间中

推荐 endpoint 抽象：

- Unix / macOS / OpenHarmony / Linux 路线
  - 优先 `unix domain socket`
- Windows 路线
  - 优先 OpenSSH 可消费的本地 IPC endpoint，例如 named pipe 或平台等价 endpoint
- 无 agent-compatible endpoint 的平台
  - 仅允许 `identity-file-fallback`

### 决策 4.2：SSH broker 注入必须通过结构化执行覆盖层，而不是 host shell 字符串拼接

即使 `SSH_AUTH_SOCK` 或 `IdentityAgent` 指向的只是临时 endpoint，这条链路也不应再通过：

- `export SSH_AUTH_SOCK=... && ssh ...`
- `cmd /c set SSH_AUTH_SOCK=... && ssh ...`
- host shell rc file 注入

推荐做法：

- 使用 `program + args + env overlay` 直接启动 `ssh`
- 若使用 `IdentityAgent`，应把 endpoint 作为结构化参数而不是 shell 片段
- env overlay 仅作用于本次 `ssh` 子进程，而不污染用户全局 shell 环境

如果当前 runtime 仍只支持 shell flatten，有两种允许路线：

- 提供 connector-owned direct exec wrapper，绕过 host shell
- 或者显式阻止使用 secret-backed SSH broker，退回受控 degraded fallback

不允许的路线是：

- 把 broker 注入信息拼进 shell 字符串后再依赖通用日志或 transcript 捕获

### 决策 4.3：SSH key passphrase 与 host key policy 必须和 broker 分层治理

SSH 私钥自己的 passphrase 语义，不能在运行时再次变成“要求 agent 或模型临时知道一串 passphrase”。推荐后续实现采用如下治理：

- 若导入的是 passphrase-protected OpenSSH key
  - 本地管理员在导入时完成一次受控解密 / 验证
  - vault 保存的是可供 broker 使用的 canonical signer material，并继续由 vault protector 保护
- runtime 建立连接时
  - broker 只负责提供签名能力
  - 不再要求远端 agent、普通 MCP tool 或连接进程再额外知道 SSH key passphrase

这样做的原因：

- 运行时再次提示 key passphrase 会把高价值秘密重新引入 argv、prompt transcript、artifact 或模型上下文风险面
- vault protector 已经承担“谁能解锁 signer material”的职责，没必要再让 SSH key 自身 passphrase 成为第二套不透明数据面口令

同时要明确：

- host key verification 仍由 SSH / target policy 负责，不由 broker 决定
- `known_hosts`、`StrictHostKeyChecking`、`ProxyJump` 等兼容性语义必须保持不变
- broker 只解决 client auth key delivery，不改变目标主机认证策略

### 决策 5：model-plane agent auth 使用用户签发的 opaque bearer token，principal 从 token 派生

这里的 `agent token` 不是第三方 API secret，而是 BridgingIO 本地 model-plane 的访问凭证。因此它应被建模为：

- 用户显式签发
- 可长期存在或设置 TTL
- 可撤销
- 可限权
- 与 target / tool / risk scope 绑定

推荐实现形态：

- token 采用高熵随机 opaque bearer token，而不是一开始就依赖 JWT
- 服务端只保存 `token_hash + metadata`，不保存明文 token
- token metadata 至少包含：
  - `token_id`
  - `principal_id`
  - `label`
  - `scopes`
  - `created_by`
  - `created_at`
  - `last_used_at`
  - `expires_at`
  - `idle_timeout`
  - `revoked_at`
  - `status`

scope 推荐至少覆盖：

- 可访问的 target 集合
- 可调用的 tool / capability 集合
- 是否允许 interactive shell
- 是否允许写操作
- 是否允许触发需审批的动作
- 是否允许 token 再派生短期 run token

最关键的一条：**请求体中的 `agent_id`、`run_id`、`client_session_id` 不再是可信身份，只能是调用标签。**

认证通过后，系统必须：

- 从 token 派生 authenticated principal
- 用 principal 参与 access scope / session isolation / audit
- 拒绝依赖请求体自报字段进行权限提升

也就是说，“谁在访问”来自 token；“这次 run / thread 想怎么标记”才来自请求体。

### 决策 5.1：token scope 采用多维默认拒绝模型，而不是单一的“能否访问 model-plane”

agent token 不应该只有“可访问 / 不可访问 model-plane”这一层粗粒度开关。为了让长期 token 在安全上可接受，scope 必须至少拆成以下维度：

- `targets`
  - 允许访问哪些 canonical target id / alias 归一化后的 target
- `tools`
  - 允许调用哪些 typed tools / capability id
- `risk_envelope`
  - 允许哪些风险等级，例如 `read-only`、`interactive-read`、`write-with-approval`、`admin`
- `session_controls`
  - 是否允许创建 interactive shell、是否允许复用逻辑会话、是否允许中断 / 关闭已有 channel
- `artifact_controls`
  - 是否允许读取 artifact、是否允许 refine、是否允许读取其他 principal 创建的 artifact
- `delegation`
  - 是否允许从长期 token 再派生短期 run token
- `network_binding`
  - 可选的来源约束，例如 loopback-only、指定 host、后续 mTLS 绑定或本地发行物绑定

推荐把 token scope 视为“默认拒绝”的交集模型：

```text
allow(request) =
  authn_success
  AND target_in_scope
  AND tool_in_scope
  AND requested_risk <= risk_envelope
  AND session_controls_allow
  AND artifact_controls_allow
  AND policy_engine_result
```

这意味着：

- token scope 允许，不代表自动跳过审批
- policy engine 允许，也不代表能绕过 token scope
- 任意一层拒绝，都必须在执行前停止

推荐的内置 scope profile 可以在后续实现阶段提供，但本设计先固定四种语义层次：

- `read-only`
  - 允许 target inspect、typed read tools、artifact read / refine
  - 不允许 interactive shell
  - 不允许写操作
- `interactive-read`
  - 允许 one-shot exec 与 interactive shell
  - 仅允许只读类命令与低风险终端行为
- `operator`
  - 允许受 scope 限制的常规操作
  - 写操作仍受审批与 policy 保护
- `admin`
  - 仅用于本地受信任管理入口
  - 不推荐作为常驻 agent token profile

### 决策 5.2：区分长期 token 与派生 run token，并限制 delegation

虽然用户可以签发长期 token，但长期 token 不应天然成为“永久超级令牌”。推荐后续实现时区分两类凭证：

- `persistent access token`
  - 用户显式创建
  - 可长期存在
  - 默认带较窄 scope
  - 可选择是否允许 delegation
- `delegated run token`
  - 由 persistent token 或本地受信任入口派生
  - TTL 很短
  - scope 只能收窄，不能扩大
  - 适用于单次自动化、单次 run、临时 thread

delegation 必须满足：

- 父 token 显式允许 delegation
- 子 token scope 只能是父 token scope 的子集
- 子 token 生命周期必须短于父 token
- revoke 父 token 时，所有子 token 必须一并失效或不可再续签

这样可以兼顾：

- 用户希望“长期授权某个 agent”
- 系统又不必把长期 token 直接暴露给每次临时 run

### 决策 5.3：`auth_mode` 必须有清晰矩阵，`none` 只作为显式受限模式保留

`model_plane.http.auth_mode` 不能只作为“有没有打开认证”的字符串开关，后续必须有稳定的矩阵语义。推荐至少保留以下模式：

| auth_mode | 目标场景 | 数据面认证 | 本地用户验证 | 备注 |
| --- | --- | --- | --- | --- |
| `none` | 仅开发 / 测试 | 无 | 无 | 必须显式开启，不应作为常规发行默认值 |
| `bearer` | 常规 agent 访问 | Opaque bearer token | 管理动作按需验证 | 推荐默认模式 |
| `mtls` | 未来受管环境 | 客户端证书 | 管理动作按需验证 | 预留扩展位 |
| `bearer+mtls` | 未来高安全部署 | 双因子传输层认证 | 管理动作按需验证 | 预留扩展位 |

其中需要明确几条硬规则：

- `none` 只能用于显式 dev/test 或受控诊断场景
- `none` 不应再因为 loopback 就被默认视为安全生产模式
- 一旦启用 `bearer` 或更强模式，loopback 与 non-loopback 都必须统一执行认证
- passkey 不作为 `auth_mode` 本身；它属于本地管理员动作的 user verification 层

推荐的默认策略是：

- bundled / standalone 的常规发行模式：`bearer`
- 非 loopback 暴露：至少 `bearer`
- `none`：仅在操作员显式声明 dev/test intent 后可用，并应在 diagnostics 中高亮

### 决策 5.4：授权与审批必须是串联关系，而不是二选一

为了避免后续实现把“拿到 token”误当成“拿到了所有操作许可”，授权链路必须固定为：

```text
AuthN -> AuthZ(scope) -> Policy -> Approval(if needed) -> Execution
```

具体语义：

- `AuthN`
  - 校验 token 是否存在、未撤销、未过期、未超出 idle timeout
- `AuthZ(scope)`
  - 判断 principal 是否允许访问 target / tool / risk envelope
- `Policy`
  - 按 profile、operation kind、系统策略继续判断是否需要审批
- `Approval`
  - 若需要，则通过本地受信任管理面发起审批
- `Execution`
  - 只有前面全部通过后才执行

为了让拒绝原因具备可审计性，任一阶段拒绝都应记录统一归因字段，例如：

- `decision_stage`
  - `authn` / `authz` / `policy` / `approval`
- `decision_reason_code`
  - 机器可读的拒绝原因代码
- `authenticated_principal`
  - 失败发生时解析出的 principal（若有）
- `audit_event_id`
  - 对应审计事件引用

这样可以避免两种常见误用：

- “有 token，所以不需要审批”
- “策略允许写操作，所以任何 token 都能写”

### 决策 5.5：现有 MCP tools 需要映射到最小 scope profile，而不是统一挂在一个“terminal access”大权限下

为了避免后续把 token scope 做成难以理解的黑箱，现有 MCP tool catalog 应先约定一份最小权限映射。它不是最终 UI 文案，但可以作为实现和审计的真相源。

推荐映射如下：

| Tool / Capability | 最小 scope profile | 额外限制 |
| --- | --- | --- |
| `bridgingio.capability.describe` / `capability.documentation` | `read-only` | 不需要 target scope |
| `bridgingio.target.inspect_basic` / `target.inspect` | `read-only` | 需要 target scope |
| `bridgingio.artifacts.read` / `artifact.reanalysis` | `read-only` | 默认只允许读取同 principal 创建或显式共享的 artifact |
| `bridgingio.artifacts.refine` / `artifact.reanalysis` | `read-only` | 默认只允许基于同 principal 可见 artifact 派生 |
| `bridgingio.terminal.exec` / `terminal.exec` | `interactive-read` 或更高 | 真正是否可执行还取决于命令风险分类与 policy |
| `bridgingio.terminal.shell.open` / `terminal.interactive_shell` | `interactive-read` 或更高 | 需要 `session_controls.open_shell=true` |
| `bridgingio.terminal.shell.write` | `interactive-read` 或更高 | 后续每次 write 仍需按命令风险重新判定 |
| `bridgingio.terminal.shell.read` | `interactive-read` 或更高 | 默认只允许读取本 principal 持有的 shell transcript |
| `bridgingio.terminal.shell.interrupt` | `interactive-read` 或更高 | 默认只允许中断本 principal 持有的 shell |
| `bridgingio.terminal.shell.close` | `interactive-read` 或更高 | 默认只允许关闭本 principal 持有的 shell |

这里要特别强调两个边界：

- `terminal.exec` 不等于自动允许写操作；它只是“允许尝试终端类调用”
- `artifact.read` / `shell.read` 这类读操作也不能天然跨 principal 共享，因为 transcript / artifact 里可能包含敏感上下文

因此，除了 target/tool/risk 维度之外，后续实现还必须补一个**资源归属约束**：

- 默认只能访问自己 principal 创建的 interactive shell、artifact、approval 上下文
- 若未来需要跨 principal 协作，应通过显式共享语义完成，而不是默认放开

另外，tool catalog 到最小 profile 的映射应成为授权真相源的一部分，并与 `TokenScopeRecord` 字段直接对齐：

- tool 映射决定最低 `scope_profile` 门槛
- `TokenScopeRecord.tool_ids`、`max_risk_envelope`、`allow_open_shell`、`allow_write_shell_input`、`allow_artifact_cross_principal` 共同决定最终是否可执行
- 任一字段不满足时，必须在 `AuthZ(scope)` 阶段拒绝，而不是延迟到执行后失败

### 决策 5.6：终端命令风险分类必须独立于 tool 名称存在

`bridgingio.terminal.exec` 与 `bridgingio.terminal.shell.write` 都只是调用入口，真正的风险来自“这次输入到底是什么操作”。因此后续实现不能仅凭 tool 名称做 token 授权，还需要结合命令风险分类。

推荐最小风险分类沿用并扩展当前 policy 的概念：

- `read`
  - 只读型探测、状态查询、环境检查
- `sensitive_read`
  - 可能读取敏感文件、敏感配置或敏感系统信息
- `write`
  - 会修改文件、环境或目标状态
- `delete`
  - 会删除文件、对象或配置
- `privileged`
  - 提权、管理级变更或等价高风险操作

执行判断应当是：

- tool scope 允许调用该 terminal tool
- risk envelope 允许该次命令风险等级
- 若当前 policy 仍要求审批，则进入审批

这能避免出现“给了 interactive shell 权限，结果等于给了无限制 root shell”的危险放大。

### 决策 5.7：agent token 需要稳定的 metadata record，且服务端只保存 hash 真相

为了让长期 token、派生 token、revocation 和审计都可治理，model-plane auth 至少需要一类稳定对象：`AgentTokenRecord`。

推荐对象形状如下：

```text
AgentTokenRecord
- token_id: String
- principal_id: String
- label: String
- status: active | revoked | expired
- token_hash: String
- hash_scheme: String              // argon2id / scrypt / other approved scheme
- scope_profile: String            // read-only / interactive-read / operator / admin
- scope_payload: TokenScopeRecord
- parent_token_id: Option<String>  // delegation lineage
- issued_via_attestation_id: Option<String>
- network_binding: Option<String>
- created_by: String
- created_at: SystemTime
- last_used_at: Option<SystemTime>
- expires_at: Option<SystemTime>
- idle_timeout_sec: Option<u64>
- revoked_at: Option<SystemTime>
- revoke_reason: Option<String>
```

其中 `TokenScopeRecord` 推荐内联或独立存储，但语义上至少覆盖：

```text
TokenScopeRecord
- target_ids: Vec<String>
- tool_ids: Vec<String>
- max_risk_envelope: String
- allow_open_shell: bool
- allow_write_shell_input: bool
- allow_artifact_cross_principal: bool
- allow_delegation: bool
- allow_admin_actions: bool
```

设计约束：

- 明文 token 只在签发瞬间显示一次
- 服务端仅使用 hash 真相做校验
- `parent_token_id` 用于 delegation lineage 和批量 revoke
- `issued_via_attestation_id` 把高风险签发动作与本地用户验证审计串起来

### 决策 6.3：本地用户验证需要 `Intent` 与 `Attestation` 两层对象，确保不可重放

如果只记录“某个用户刚刚通过了 passkey 验证”，后续很容易被误复用。为了把验证结果绑定到具体管理动作，推荐拆成两层：

- `LocalAdminActionIntent`
  - 描述“想做什么”
- `LocalAdminAttestationRecord`
  - 描述“本地用户确实为这个意图完成了验证”

推荐对象形状如下：

```text
LocalAdminActionIntent
- intent_id: String
- action_kind: String              // create-token / expand-scope / reveal-secret / unlock-vault
- target_object_ref: String
- requested_payload_digest: String // 绑定 scope 变更、目标 token、目标 secret 等
- requested_by_principal: String
- requested_via: String            // ui / local-cli / trusted-control-plane
- created_at: SystemTime
- expires_at: SystemTime
- status: pending | verified | consumed | expired | cancelled
```

```text
LocalAdminAttestationRecord
- attestation_id: String
- intent_id: String
- verified_principal: String
- verification_method: String      // passkey / platform-biometric / controlled-fallback
- assurance_level: String          // uv-required / platform-user-verified / fallback-local-admin
- issued_at: SystemTime
- expires_at: SystemTime
- consumed_at: Option<SystemTime>
- status: active | consumed | expired | revoked
- local_session_binding: Option<String>
```

关键约束：

- attestation 必须绑定 intent，不能脱离 intent 单独复用
- `requested_payload_digest` 必须覆盖关键管理参数，避免“为 A 验证后拿去执行 B”
- attestation 默认一次性消费；若策略允许短窗口复用，也必须限定 action family 和 target object 范围

### 决策 6.4：这些对象应进入同一安全审计真相源，并与现有 `ApprovalRequestRecord` / `AuditEvent` 风格保持一致

考虑到当前 domain 已经使用 `ApprovalRequestRecord`、`AuditEvent`、`SessionRecord` 这种稳定命名与时间字段风格，后续安全相关对象也应保持一致：

- `VaultSecretRecord`
- `VaultSecretVersionRecord`
- `AgentTokenRecord`
- `LocalAdminActionIntent`
- `LocalAdminAttestationRecord`

这样有几个好处：

- 与现有 `status + created_at + ..._at` 风格一致，降低实现心智负担
- 审计事件可以自然引用这些 record id
- UI / control-plane 后续展示不需要再引入一套完全不同的对象命名体系

### 决策 6：passkey 作为本地用户验证层，而不是默认的日常 agent 数据面认证

passkey 很适合进入本次架构，但它最合适的位置是“高风险管理动作的用户验证”，而不是强行替代每次 agent 请求的 bearer auth。

推荐由 passkey / 平台用户验证覆盖的动作：

- 签发长期 agent token
- 提升 token scope
- 撤销受保护 token
- 解锁 vault
- reveal / export secret
- 批准 privileged / sensitive action
- 轮换 vault master protector 或 token signing material

原因：

- macOS、Windows、OpenHarmony PC 都具备 passkey / platform authenticator 路线
- 这些场景都存在明确的“本地用户在场”语义
- headless agent 的日常请求并不适合每次都发起 passkey challenge

因此推荐分层：

- `bearer token`：agent 的日常数据面访问凭证
- `passkey / platform user verification`：本地管理员动作的根认证

补充澄清：

- 当 vault `unlock trigger policy` 要求在 core 启动或 secret 访问前进行本地解锁时，`passkey` 也可以成为首选的**本地解锁方法**
- 这仍然属于“本地用户在场验证 + 本地 secret use gate”
- 并不意味着 passkey 变成远端 agent 的日常数据面认证

需要记录的兼容边界：

- passkey 私钥不进入 BridgingIO vault，仍由平台 authenticator 托管
- BridgingIO 只保存 credential id、公钥和策略元数据
- 在暂不具备 passkey 的宿主场景下，允许等价的本地管理员验证方式，但不得回退到“无验证直接签发长期 token”

### 决策 6.1：本地用户验证需要动作矩阵与 freshness window，而不是每次管理动作都强制重新 challenge

为了兼顾安全性与可用性，本地用户验证应设计成“动作分级 + freshness window”模型，而不是简单的“所有管理动作都重验一次”。

推荐动作矩阵如下：

| 动作 | 是否需要 fresh user verification | 说明 |
| --- | --- | --- |
| 创建长期 token | 必须 | 高风险授权起点 |
| 扩大 token scope | 必须 | 权限提升 |
| 创建短期 delegated run token | 取决于 freshness | 若父 token 已在新鲜管理员会话内签发，可复用短窗口 |
| revoke token | 建议，但可放宽到近期已验证会话 | 撤销是收权动作，风险低于签发 |
| reveal / export secret | 必须 | 直接接触明文 |
| 解锁 vault | 必须 | 打开高价值 secret 使用面 |
| 批准 privileged / sensitive_read | 必须 | 高风险操作放行 |
| 批准普通 write | 取决于策略与 freshness | 可由策略要求 fresh UV 或复用短窗口 |
| 注册 / 移除 passkey | 必须 | 根认证面变更 |
| 降级 `auth_mode` 到 `none` | 必须 | 安全边界下调 |

推荐 freshness 语义：

- 一次成功的本地 user verification 只产生短期 `admin attestation`
- attestation 只在本地受信任控制面有效
- 默认 TTL 很短，例如 1 到 5 分钟
- attestation 仅覆盖一组明确可复用的动作，不应变成长期后台登录态

这能避免两种坏结果：

- 过度频繁 challenge，导致管理员干脆关闭安全机制
- 一次 challenge 后长期无限期复用，导致安全边界被静默抹平

### 决策 6.2：passkey / user verification 结果必须绑定具体 intent，且不可被远端 agent 重放

passkey challenge 的结果不应被简单当成“当前机器上有人验证过”这种全局布尔值，而应绑定到具体 intent：

- 谁发起
- 发起自哪个本地受信任入口
- 要执行什么管理动作
- 作用于哪个 token / secret / policy 目标
- 何时失效

推荐 ceremony：

```text
Local admin action request
  -> create intent descriptor
  -> local user verification (passkey / platform authenticator / controlled fallback)
  -> mint short-lived local admin attestation
  -> perform exactly one authorized management action
  -> write audit event
```

attestation 至少应绑定：

- `intent_id`
- `action_kind`
- `target_object`
- `verified_principal`
- `issued_at`
- `expires_at`
- `verification_method`

这样可以保证：

- 远端 bearer agent 不能拿到 user verification 结果后随意重放
- 一个为“创建 token”通过的验证，不能直接复用去“reveal secret”
- 审计里能明确看到“是谁、以什么方式、为哪件事做了本地确认”

### 决策 6.5：PasskeyCredentialRecord 需要稳定元数据与 rotation/revoke 生命周期

为了支持多设备注册、凭据替换与安全回收，本地用户验证层应为每个 passkey 凭据维护稳定 metadata record，而不是只存一个“当前可用 passkey”布尔状态。推荐对象：

```text
PasskeyCredentialRecord
- credential_id: String
- principal_id: String
- rp_id: String
- public_key: String
- transports: Vec<String>         // internal / usb / ble / hybrid / ...
- user_verification_policy: String
- assurance_level: String
- status: active | superseded | revoked | disabled
- created_at: SystemTime
- last_used_at: Option<SystemTime>
- rotated_at: Option<SystemTime>
- revoked_at: Option<SystemTime>
- revoke_reason: Option<String>
- superseded_by: Option<String>
```

生命周期语义：

- 新注册 passkey 后进入 `active`
- 发生 rotation 时，新凭据进入 `active`，旧凭据进入 `superseded`
- 被撤销或设备遗失时进入 `revoked`，不得再参与任何 fresh user verification
- `revoked` 与 `disabled` 不得重新激活；后续只能重新注册新凭据

安全边界：

- BridgingIO 仅持久化 credential metadata 与公钥材料
- passkey 私钥继续由平台 authenticator 托管
- passkey 查询默认仅返回 display-safe summary（label、状态、创建时间、最近使用时间），不返回内部 challenge/assertion 材料

### 决策 7：standalone 先记录配置与管理路线，不在本次变更中实现

standalone 未来需要支持内网 / headless 部署，但这不意味着要在本次设计里把 CLI 细节全部实现完。当前先把路线定住：

配置文件只声明策略，不保存明文 secret。建议后续至少支持：

```toml
[vault]
backend = "builtin-encrypted"
namespace = "io.bridgingio"

[vault.unlock]
trigger_policy = "on-first-secret-access"
allowed_methods = ["passkey", "os-native", "passphrase"]
preferred_method = "passkey"
cache_ttl_sec = 600

[vault.ssh]
delivery_mode = "ssh-agent-broker"
fallback_delivery_mode = "ephemeral-identity-file"

[model_plane.http.auth]
mode = "bearer"
required_when_non_loopback = true
```

CLI 路线推荐采用独立管理入口，而不是把 secret 直接揉进 `run`：

- `bridgingio-core vault init`
- `bridgingio-core vault import`
- `bridgingio-core vault unlock`
- `bridgingio-core auth token create`
- `bridgingio-core auth token revoke`

secret 输入只允许这些路线：

- `--from-file`
- `--from-stdin`
- `--from-fd`
- `--from-tty-prompt`

不推荐也不应后续实现的路线：

- `bridgingio-core run --import-secret <plaintext>`
- `bridgingio-core --token <plaintext>`
- `bridgingio-core --private-key <plaintext>`

如果 `run` 未来需要补充参数，优先只补 unlock source 一类参数，例如：

- `--vault-unlock-from-env`
- `--vault-unlock-from-fd`

这样可以兼顾 headless 部署，同时不把明文秘密重新带回 argv 泄露面。

### 决策 7.1：standalone secret 输入必须有确定优先级与审计语义

为了避免运行脚本中出现“同一命令从多个输入源抢占 secret”的歧义，推荐对 standalone 管理入口固定输入优先级与冲突规则：

- 优先级：`fd > stdin > file > tty-prompt`
- 若用户显式同时声明多个输入源（例如 `--from-fd` 与 `--from-file`），必须 fail closed 并返回可诊断错误
- 未显式指定输入源时，系统可按上述优先级自动选择，但必须把选择结果写入审计事件

审计字段至少应包含：

- `source_kind`（fd / stdin / file / tty-prompt）
- `intent_id`
- `operator_principal`
- `byte_length`
- `source_locator_digest`（而不是原始路径、fd 值或明文）

禁止事项：

- 不得把 secret 明文写入审计日志、shell transcript 或 artifact
- 不得把完整输入路径、fd 原值与 prompt 原文作为默认可见日志字段

### 决策 7.2：内网 / headless 部署需要最小操作手册与安全默认值

为了降低误配置概率，standalone future route 应同时提供“最小可运行手册”和“安全默认值建议”。推荐最小操作手册：

1. 初始化 vault（`builtin-encrypted` + `passphrase protector` + Argon2id 基线参数）
2. 通过 `stdin/file/fd/tty-prompt` 导入 SSH 私钥或 token，禁止 argv 明文导入
3. 以本地受信任入口完成 unlock，再启动或放行 secret-backed 操作
4. 创建最小权限 token（默认短期 TTL、受限 scope、可撤销）
5. 周期性执行 token revoke/rotation 与 KDF 参数基线检查

推荐安全默认值：

- `model_plane.http.auth.mode = bearer`
- `required_when_non_loopback = true`
- `vault.unlock.trigger_policy = on-first-secret-access`（高安全场景可切到 `on-core-start`）
- `vault.unlock.allowed_methods` 至少包含 `passphrase`
- `vault.ssh.delivery_mode = ssh-agent-broker`
- 明文 secret 不进入 TOML、argv、普通 env snapshot 与常规日志

## 存储边界与显示安全

### 总体分层规则

为了让后续实现不把内部 record 直接“顺手返回给 UI / MCP”，推荐把安全对象统一分成四层：

1. `metadata store`
   - 保存状态、策略、引用、时间戳、lineage、审计关联 id
   - 适合放在结构化本地数据库中，例如 SQLite 或等价嵌入式 store
2. `encrypted material / protected storage`
   - 只保存被加密后的 secret material、受保护的原始断言或等价高敏材料
   - 不承担面向 UI 的对象读取语义
3. `display-safe projection`
   - 本地受信任 control-plane 只读取这一层，而不是直接读取内部 record
   - projection 应只保留本地管理员做决策必需的字段
4. `internal-only`
   - 仅供 vault / auth / policy 内部使用
   - 不应直接暴露给普通 MCP、模型输出、artifact、常规诊断或通用日志

硬约束：

- secret 明文、token 明文、vault unwrap material、passphrase、原始 authenticator challenge/response 不得进入 `metadata store`
- 普通 model-plane / MCP 不得返回内部 record 原型；即使本地受信任 control-plane 也应返回 projection，而不是直接序列化内部对象
- diagnostics 可以引用 record id、状态和 canonical ref，但不得输出 `ciphertext_locator`、`token_hash`、`requested_payload_digest`、`local_session_binding` 等内部字段

### Vault 对象的存储边界

| 对象 | metadata store | encrypted / protected storage | display-safe projection | internal-only |
| --- | --- | --- | --- | --- |
| `VaultSecretRecord` | `reference`、`namespace`、`kind`、`name`、`label`、`status`、`active_version_id`、`usage_policy`、`export_policy`、`created_by`、`created_at`、`last_used_at`、`last_rotated_at` | 无；该 record 本身不保存 secret bytes | `reference`、`label`、`kind`、`status`、`usage_policy`、`export_policy`、`active_version_seq`、`last_used_at`、`last_rotated_at` | 自由文本 `notes`、后台 locator、protector 装配细节、任何本地迁移备注 |
| `VaultSecretVersionRecord` | `version_id`、`reference`、`version_seq`、`state`、`protector_binding` 的粗粒度类别、`content_format`、`import_method`、`created_by`、`created_at`、`activated_at`、`superseded_at`、`revoked_at`、`destroy_after` | secret 密文本体、wrapped DEK、可选受保护导入包或平台 assertion 副本 | `version_id`、`reference`、`version_seq`、`state`、`content_format`、`created_at`、`activated_at`、`destroy_after` | `ciphertext_locator`、`ciphertext_digest`、具体 protector handle、unwrap cache、任何内存中的明文缓冲 |

### Vault 主密钥对象的存储边界

| 对象 | metadata store | encrypted / protected storage | display-safe projection | internal-only |
| --- | --- | --- | --- | --- |
| `VaultKeyEnvelopeRecord` | `vault_key_id`、`state`、`active_wrap_set_id`、`created_at`、`rotated_at`、`last_unlocked_at` | 无；root key 本体不应以明文存在于 metadata store | `vault_key_id`、`state`、`protector_summary`、`last_unlocked_at` | 当前活跃 root key、解锁缓存、secure memory handle |
| `ProtectorWrapManifest` | `wrap_id`、`vault_key_id`、`protector_binding` 的粗粒度类别、`wrap_format`、`created_at`、`last_verified_at`、`status` | 被 protector 包裹后的 root key blob、平台受保护 wrapping material | `wrap_id`、`protector_category`、`status`、`last_verified_at` | `wrapped_key_locator`、`wrapped_key_digest`、平台 key handle、任何临时 unwrap token |
| `PassphraseProtectorParams` | `kdf_scheme`、`kdf_version`、`memory_cost_kib`、`time_cost`、`parallelism`、`created_at`、`upgraded_at` | 随机 salt、可选受保护 rewrap transcript | `kdf_scheme`、`kdf_version`、`cost_summary`、`upgraded_at` | `salt_locator`、`salt_digest`、派生中的 `KEK`、任何 passphrase 缓冲 |

额外建议：

- `protector_binding` 可以在 projection 中只显示类别，例如 `os-native` 或 `passphrase`，而不显示具体 keychain item id 或句柄
- 若允许管理员填写 `notes`，该字段也应视为“可能含敏感上下文”的 admin-only 内容，不应进入普通模型可见面
- `ciphertext_digest` 只用于完整性与内部比对，不应变成 UI 或模型可见的“可枚举索引”
- `salt` 与 KDF 参数可以不视为 secret，但它们的展示也应以调试/管理需要为限，不必默认暴露给普通 UI

### Agent Token 对象的存储边界

| 对象 | metadata store | encrypted / protected storage | display-safe projection | internal-only |
| --- | --- | --- | --- | --- |
| `AgentTokenRecord` | `token_id`、`principal_id`、`label`、`status`、`scope_profile`、`scope_payload`、`parent_token_id`、`issued_via_attestation_id`、`network_binding` 的策略摘要、`created_by`、`created_at`、`last_used_at`、`expires_at`、`idle_timeout_sec`、`revoked_at`、`revoke_reason` | 不保存 token 明文；签发瞬间的明文 token 只存在于一次性显示缓冲与受控内存 | `token_id`、`label`、`principal_summary`、`status`、`scope_profile`、`scope_summary`、`created_at`、`last_used_at`、`expires_at`、`idle_timeout_sec`、`parent_token_id` | `token_hash`、`hash_scheme`、精确 network matcher、内部 principal routing、任何一次性显示缓冲 |
| `TokenScopeRecord` | `target_ids`、`tool_ids`、`max_risk_envelope`、`allow_open_shell`、`allow_write_shell_input`、`allow_artifact_cross_principal`、`allow_delegation`、`allow_admin_actions` | 无 | 对本地管理员展示经归类的 scope summary，例如 target 列表、tool 分类、风险上限 | 实现期的内部 matcher cache、归一化索引、策略编译缓存 |

额外建议：

- `scope_payload` 可以完整保存在 metadata store，但对本地 UI 默认展示 `scope_summary`，避免把内部 capability id、matcher 细节和未来扩展字段直接泄露到产品表层
- token 明文只在签发完成的那一刻显示一次；后续所有查询都只能返回 `AgentTokenSummary`
- 若未来支持导出一次性 bootstrap bundle，该 bundle 也不应成为新的长期真相层

### 本地用户验证对象的存储边界

| 对象 | metadata store | encrypted / protected storage | display-safe projection | internal-only |
| --- | --- | --- | --- | --- |
| `LocalAdminActionIntent` | `intent_id`、`action_kind`、`target_object_ref`、`requested_by_principal`、`requested_via`、`created_at`、`expires_at`、`status` | 无 | `intent_id`、`action_kind`、`target_object_summary`、`requested_by_principal`、`status`、`created_at`、`expires_at` | `requested_payload_digest`、原始参数差异、challenge nonce 关联 |
| `LocalAdminAttestationRecord` | `attestation_id`、`intent_id`、`verified_principal`、`verification_method`、`assurance_level`、`issued_at`、`expires_at`、`consumed_at`、`status` | 若保留原始 authenticator assertion、平台证明或受控 fallback transcript，必须进入受保护存储而不是普通 metadata | `attestation_id`、`intent_id`、`verification_method`、`assurance_level`、`status`、`issued_at`、`expires_at`、`consumed_at` | `local_session_binding`、原始 assertion bytes、挑战材料、设备绑定细节 |

### SSH broker 对象的存储边界

| 对象 | metadata store | encrypted / protected storage | display-safe projection | internal-only |
| --- | --- | --- | --- | --- |
| `SshAgentBrokerSession` | `broker_session_id`、`target_id`、`credential_ref`、`state`、`endpoint_kind`、`degraded`、`created_at`、`expires_at`、`attached_channel_count`、`cleanup_deadline` | 无；broker session 不应把私钥材料持久化 | `broker_session_id`、`target_summary`、`credential_ref`、`state`、`endpoint_kind`、`degraded`、`created_at` | socket path、pipe name、env overlay、in-memory signer handle、fallback identity file path |

额外建议：

- `requested_payload_digest` 的目的只是把 attestation 绑定到具体管理动作；它不应被 UI 当作可读字段展示
- `local_session_binding` 只用于防止本地控制面重放，不应成为跨进程公开接口的一部分
- `socket path`、`pipe name`、env overlay 与 fallback identity file path 都不应进入 display-safe projection；它们属于 cleanup 与 runtime attach 的内部细节

### Passkey 的未来存储边界

未来若引入 `PasskeyCredentialRecord`，BridgingIO 推荐只持久化：

- `credential_id`
- `public_key`
- `rp_id`
- `user_handle` 或本地 principal 绑定
- `created_at`
- `last_used_at`
- `revoked_at`
- `status`

必须明确：

- passkey 私钥继续由平台 authenticator 托管，不进入 BridgingIO vault
- BridgingIO 不应把 WebAuthn / passkey 的原始 challenge response 长期保存在普通 metadata store
- 对本地 UI 展示时，只需要显示 credential label、创建时间、最近使用时间、状态与支持的平台类型

### 推荐的显示安全投影模型

为了避免 future API 直接把内部 record 暴露出去，推荐从一开始就约定 projection 类型：

```text
VaultSecretSummary
- reference: String
- label: String
- kind: String
- status: String
- usage_policy: String
- export_policy: String
- active_version_seq: Option<u32>
- last_used_at: Option<SystemTime>
- last_rotated_at: Option<SystemTime>
```

```text
AgentTokenSummary
- token_id: String
- label: String
- principal_summary: String
- status: String
- scope_profile: String
- scope_summary: String
- created_at: SystemTime
- last_used_at: Option<SystemTime>
- expires_at: Option<SystemTime>
- idle_timeout_sec: Option<u64>
```

```text
LocalAdminActionSummary
- intent_id: String
- action_kind: String
- target_object_summary: String
- status: String
- verification_method: Option<String>
- created_at: SystemTime
- expires_at: SystemTime
```

```text
VaultProtectorSummary
- vault_key_id: String
- state: String
- protector_summary: String
- last_unlocked_at: Option<SystemTime>
```

```text
PassphraseProtectorSummary
- kdf_scheme: String
- kdf_version: String
- cost_summary: String
- upgraded_at: Option<SystemTime>
```

```text
SshBrokerSessionSummary
- broker_session_id: String
- target_summary: String
- credential_ref: String
- state: String
- endpoint_kind: String
- degraded: bool
- created_at: SystemTime
```

这些 projection 主要服务于：

- desktop UI
- standalone 本地 CLI / 管理面
- diagnostics 中的“安全可显示”部分

而不应服务于：

- 普通模型响应
- 默认 artifact 内容
- 未授权的 MCP introspection

## 生命周期与状态机

### `VaultSecretVersionRecord`

```text
             import
  (not persisted) ───────▶ pending
                            │
                promote     │ discard / failed verification
                            ▼
                          active ───────────────▶ revoked
                            │                        │
          new version       │                        │ purge ciphertext
            promoted        ▼                        ▼
                        superseded ─────────────▶ destroyed
                            │
             rollback       │ retire after grace
                            └──────────────▶ revoked
```

约束：

- 同一 `reference` 在任意时刻最多只有一个 `active` version
- `destroyed` 是终态，进入后必须完成 ciphertext purge、locator tombstone 与审计记录
- `superseded` 版本仅在尚未 `revoked` / `destroyed` 且策略允许时才可重新提升为 `active`
- 紧急停用时可以从 `active` 直接进入 `revoked`，而不必等待出现新版本

### `AgentTokenRecord`

```text
valid intent + attestation + successful mint
  (not persisted) ─────────────────────────▶ active
                                               │  │
                                ttl / idle     │  │ manual revoke /
                                timeout        │  │ parent revoke /
                                               │  │ policy breach
                                               ▼  ▼
                                           expired revoked
```

等价地理解为：

- record 只在真正签发成功后 materialize 为 `active`
- `active -> expired` 由绝对过期时间或 idle timeout 驱动
- `active -> revoked` 由显式撤销、父 token 级联撤销、策略封禁或安全事件触发
- `expired` 与 `revoked` 都是终态；重新授权只能创建新 token record，而不是恢复旧 token

补充约束：

- 若父 token 被 revoke，所有仍为 `active` 的子 token 必须级联转为 `revoked`
- 若 token 已进入 `expired`，后续手工 revoke 只写审计，不应把旧 token 重新带回活动态或重置时效

### `LocalAdminActionIntent`

```text
             create
  (not persisted) ───────▶ pending
                            │  │  │
                attestation │  │  │ explicit cancel
                matches     │  │  ▼
                            │  │ cancelled
                            │  ▼
                            │ expired
                            ▼
                         verified
                            │  │  │
             execute        │  │  │ explicit cancel
             admitted       │  │  ▼
                            │  │ cancelled
                            │  ▼
                            │ expired
                            ▼
                         consumed
```

推荐按下列单向语义实现：

- `pending -> verified`
  - 仅当存在匹配的有效 attestation，且 `requested_payload_digest`、目标对象与动作类型一致
- `pending -> expired`
  - 超过短时 intent TTL
- `pending -> cancelled`
  - 本地管理员主动取消，或上游对象已失效
- `verified -> consumed`
  - 当绑定动作被接纳进入执行链路时立即消费，避免重复重放
- `verified -> expired`
  - 验证窗口耗尽但动作未执行

实现建议：

- 默认按“一次 intent 对应一次执行尝试”建模；若执行失败需要重试，建议重新创建 intent，而不是复用旧 intent
- `consumed`、`expired`、`cancelled` 都是终态

### `LocalAdminAttestationRecord`

```text
successful local user verification
  (not persisted) ─────────────────▶ active
                                       │  │  │
                           bound action│  │  │ explicit revoke /
                           uses it     │  │  │ session mismatch /
                                       │  │  │ credential removed
                                       ▼  ▼  ▼
                                   consumed expired revoked
```

推荐约束：

- `active` attestation 只在本地受信任 control-plane 内有效
- `active -> consumed` 发生在绑定 intent 被真正消费时
- `active -> revoked` 发生在 intent 被取消、`local_session_binding` 不匹配、passkey 被移除或策略要求强制失效时
- `consumed`、`expired`、`revoked` 都不得重新激活

### `SshAgentBrokerSession`

```text
broker requested
  (not persisted) ─────────────────────────▶ preparing
                                               │  │
                                 endpoint up   │  │ setup failure
                                               ▼  ▼
                                            ready failed
                                              │   │
                         channel bind         │   │ cleanup
                         explicit close       │   │
                         cleanup timeout      │   │
                                           attached │
                                              │     │
                           last channel       │     │
                           detached           ▼     ▼
                                            draining
                                               │
                                               │ cleanup done
                                               ▼
                                             closed
```

推荐约束：

- `preparing -> ready` 只有在 endpoint、signer material 与 cleanup hook 都成功就绪后才成立
- `ready -> attached` 发生在真正有 SSH transport / channel 绑定时
- `attached -> draining` 发生在最后一个绑定 channel 结束，或显式关闭
- `failed` 与 `closed` 都必须触发 cleanup；若 cleanup 失败，仍需把残留标记进 diagnostics 并在下次启动时清扫
- `identity-file-fallback` 也必须复用这一状态机，只是 `endpoint_kind` 与 `degraded=true` 不同

### 状态机之间的联动约束

- `LocalAdminActionIntent.status = expired | cancelled | consumed` 后，关联的 `active` attestation 必须变为不可用
- `AgentTokenRecord.issued_via_attestation_id` 只能引用当时真实存在且匹配 intent 的 attestation
- `VaultSecretRecord.active_version_id` 的切换必须与 `VaultSecretVersionRecord` 的 `active / superseded` 迁移一起原子写入
- `VaultSecretVersionRecord.destroyed` 后，任何 projection 都不应再显示该版本可重新激活
- `VaultKeyEnvelopeRecord.state = rewrapping` 时，不得丢失现有可用 wrap set；只有新 wrap set 校验通过后才能 retire 旧 wrap
- `SshAgentBrokerSession.state = attached | ready` 时，对应 signer material 只能存在于受控内存；进入 `draining | failed | closed` 后必须启动零化与 endpoint 清理

## 审计关联建议

为了让后续审计能回答“是谁、通过什么验证、操作了哪个 secret / token、最后是否执行成功”，推荐每个相关 `AuditEvent` 至少允许挂接以下引用：

- `authenticated_principal`
- `target_id`
- `secret_reference`
- `secret_version_id`
- `token_id`
- `parent_token_id`
- `intent_id`
- `attestation_id`
- `session_id`
- `artifact_id`
- `decision_stage`，例如 `authn` / `authz` / `policy` / `approval` / `execution`
- `result`，例如 `allowed` / `denied` / `expired` / `revoked` / `degraded`

这样可以把以下链路串起来：

```text
local verification
  -> attestation
  -> token issued
  -> token used
  -> target / tool requested
  -> policy / approval decision
  -> execution result
```

也可以把以下 secret 生命周期串起来：

```text
secret imported
  -> version promoted
  -> connector / broker used secret
  -> version superseded
  -> version revoked / destroyed
```

## 风险与待确认问题

- `bridgingio-core --self-test` 只能承载 contract-critical smoke，不能替代完整的 token lifecycle、passkey ceremony 或跨进程 authn/integration test
- Windows 与 OpenHarmony PC 上的 agent-compatible SSH key delivery 具体采用何种本地 endpoint 语义，还需要平台实现 Spike
- Linux 侧 passkey / platform authenticator 路线可以后续补齐，但本次不阻塞 macOS / Windows / OpenHarmony PC 架构定稿
- model-plane `auth_mode=none` 的兼容保留窗口需要单独决定；长期目标应是不再把 loopback 本身视为可信身份边界
- 当前 runtime logger 仍会记录原始命令字符串；secret-aware execution 落地前必须同步收敛日志与 artifact redaction

## self-test 补充决策

### 决策 7：`--self-test` 只吸收本次提案中已经产品化且必须持续守住的 contract-critical smoke

`design-vault-and-agent-auth` 的范围里既有“已经具备代码承载的安全边界”，也有“仍然停留在设计与后续路线”的部分。`bridgingio-core --self-test` 应只吸收前者，避免把还未形成稳定产品对象或跨进程协议的设计稿伪装成已自动化验收的事实。

应进入 `--self-test` 的最小 contract-critical 集合：

- canonical `CredentialRef` 归一化仍然输出稳定 `vault://...`
- degraded / shim vault backend 在未显式放宽前必须 fail-closed
- secret 的日常使用必须走 broker，而不是恢复通用 plaintext `get()`
- 本地管理员 `intent + attestation` 验证必须是短时、单次消费
- secret-backed SSH delivery 必须验证 broker lifecycle 与受控 fallback 语义
- model-plane 非 loopback 暴露仍必须要求显式 enable 与认证保护

明确不要求进入当前 `--self-test` 的内容：

- 长期 token / 派生 token / revoke / idle timeout 的完整生命周期
- passkey ceremony、authenticator registration 与多设备管理
- 真正的 loopback / non-loopback bearer request authn 流程

这些能力仍需要独立的单元测试、集成测试或未来专门的 auth contract suite；`--self-test` 在这里承担的是“关键安全边界没有被回归打穿”的 smoke 角色，而不是完整替代测试矩阵。

## 本次边界总结

本次 change 的交付目标是：

- 定义安全真相层
- 定义 token / principal / scope 模型
- 定义 SSH 私钥交付路线
- 定义 passkey 在管理动作中的位置
- 记录 standalone future route

本次 **不** 要求：

- 立即实现 standalone vault 管理命令
- 立即实现 passkey ceremony
- 立即把全部 connector / runtime 改造完毕

但这些后续实现必须遵守本设计中确定下来的安全边界，不能再回到“明文配置 / 明文 argv / memory shim 假装安全”的路线。
