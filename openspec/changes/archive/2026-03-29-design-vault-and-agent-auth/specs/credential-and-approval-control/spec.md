## ADDED Requirements

### 需求:保险库必须提供规范化引用与加密真相层
BridgingIO 必须通过规范化的加密保险库真相层管理敏感凭据，而不是继续依赖普通内存或明文文件作为长期存储语义。外部 profile、settings 和审计系统必须继续通过稳定的 `CredentialRef` 引用 secret，并且该引用必须采用 canonical `vault://<namespace>/<kind>/<name>` 语义。系统可以接受 legacy alias 作为输入，但在保存、回写和审计中必须输出 canonical 引用。

#### 场景:迁移或轮换 secret 后保持引用稳定
- **当** 操作员将某个 SSH 私钥从旧后端迁移到新的加密 vault，或对其执行 rotation
- **那么** target profile 中保存的 `CredentialRef` 必须保持稳定，系统只能更新内部 active version / backend mapping，而不能要求 profile 改写为新的后端私有标识

#### 场景:导入 legacy 风格引用
- **当** 用户或旧配置输入 `vault:ssh-key:ops-prod` 这类 legacy 风格引用
- **那么** 系统可以接受并归一化该输入，但最终持久化与回写时必须使用 canonical `vault://...` 形式

### 需求:保险库必须维护稳定 secret record 与 version record
系统必须把“稳定 secret 引用”和“某个具体 secret 加密版本”建模为分离对象，而不是把一个 `CredentialRef` 直接等同于一段可变明文。至少必须存在 secret 级 metadata record 与 version 级 metadata record，以支持 active version 切换、rotation、revoke 与审计。

#### 场景:同一 secret 导入新版本
- **当** 操作员为一个已有 `CredentialRef` 的 secret 导入新的内容版本
- **那么** 系统必须创建新的 version record，并更新 secret record 的 active version 指向，而不是直接覆盖旧版本导致审计与回滚能力丢失

#### 场景:旧版本进入 superseded 状态
- **当** 某个新的 secret 版本被提升为 active
- **那么** 先前的 active 版本必须转为 superseded、revoked 或等价的非活动状态，并保留必要的审计与销毁语义

### 需求:保险库对象必须分离元数据、密文材料与展示投影
系统必须把 secret 的 metadata、加密材料与对本地管理面的展示投影视为不同层次，而不是把内部 record 直接暴露给 UI、CLI、model-plane 或普通 MCP tool。`metadata store` 只能保存引用、状态、策略、时间戳与审计关联字段；secret 明文、unwrap material、原始 ciphertext locator、受保护导入包与等价高敏内容必须位于加密或受保护存储中。所有本地管理界面的读取默认必须返回 display-safe projection，而不是内部原始 record。

#### 场景:本地管理员列出 vault 中的 secret
- **当** 本地受信任 control-plane 请求列出当前 vault 中的 secret
- **那么** 系统只能返回诸如 canonical `CredentialRef`、label、kind、status、rotation 时间等 display-safe 字段，而不得返回 secret 明文、`ciphertext_locator`、unwrap material 或等价内部细节

#### 场景:普通 MCP tool 请求读取 secret metadata
- **当** 模型侧或普通 MCP tool 请求读取某个 secret 的信息
- **那么** 系统必须拒绝该请求，或仅返回经过策略允许的极小化安全摘要，而不能把内部 vault record 直接序列化返回

### 需求:保险库加密层必须采用 root key 与每版本 DEK 的分层封装
系统必须把 vault 的长期解锁真相与各 secret version 的内容加密分离。至少必须存在一个由 protector 包裹的 vault root key，以及每个 secret version 独立生成的内容级 DEK。系统不得把所有 secret 直接用同一个 protector 输出或单一固定密钥明文加密后持久化。

#### 场景:轮换 protector 时不重加密全部 secret 内容
- **当** 操作员为现有 vault 增加、替换或移除某个 protector
- **那么** 系统应能够通过重包裹 vault root key 完成该变更，而不必为所有 secret version 重新生成内容密文

#### 场景:导入新 secret version
- **当** 操作员向某个现有 `CredentialRef` 导入新的 secret version
- **那么** 系统必须为该版本生成新的内容级 DEK，并将其与 vault root key 的包裹关系一并持久化，而不能复用未分层的全局明文密钥状态

### 需求:protector 可用性必须以 fail-closed 方式影响 secret 使用
系统必须把 protector readiness 视为实际安全边界，而不是纯诊断信息。若 vault 已初始化但当前不存在策略允许的可用 protector，则系统必须保持 locked / unavailable，并拒绝 secret broker、SSH key delivery、token 签发与等价高风险 secret 使用流程。

#### 场景:已初始化 vault 但没有可用 protector
- **当** 实例启动时检测到 vault 已存在，但 `primary` 与允许的 `recovery` protector 均不可用
- **那么** 系统必须 fail closed，并明确诊断当前实例无法安全解锁 vault，而不能回退到明文配置、普通内存副本或静默跳过解锁

#### 场景:仅存在显式测试 shim
- **当** 当前宿主只具备显式 dev/test 的 insecure shim，而未启用任何正式 protector
- **那么** 系统最多只能进入受控测试模式或拒绝启动，不得把该 shim 当作正式长期 secret store

### 需求:passphrase protector 必须作为可恢复保护层并允许备份迁移
系统必须允许把 `passphrase` protector 建模为 vault 的恢复或迁移保护层，而不是只能依赖当前宿主的 `os-native` 绑定。只要攻击者未获得正确 passphrase，单独拷走 vault 数据文件不得直接恢复其中的 token 或私钥明文。

#### 场景:把 vault 迁移到新设备
- **当** 操作员将加密的 vault 数据复制到新的宿主设备，并通过受信任入口提供正确 passphrase
- **那么** 系统必须允许其恢复同一个 vault root key 与已有 secret 内容，而不要求原设备的 `os-native` protector 仍然在场

#### 场景:仅拿到 vault 文件的攻击者尝试离线恢复
- **当** 攻击者只获得了 vault 数据文件、metadata 与 KDF 参数，但未获得正确 passphrase 或其他可用 protector
- **那么** 系统设计必须保证攻击者至多面对离线口令猜解，而不能直接读取 vault 中保存的 token 或私钥明文

### 需求:passphrase protector 必须使用 memory-hard KDF 且推荐 Argon2id
系统必须确保 `passphrase` protector 使用 memory-hard KDF，而不是普通哈希或仅 CPU-bound 的弱派生方式。默认推荐 `Argon2id` 作为首选方案。每个 passphrase wrap set 必须使用随机 salt，并记录可升级的 KDF 参数与格式版本，以支持后续提高离线攻击成本。

#### 场景:创建 passphrase protector
- **当** 操作员为 vault 配置新的 `passphrase` protector
- **那么** 系统必须为其生成随机 salt，使用 memory-hard KDF 派生 KEK，并仅用该 KEK 包裹 vault root key，而不是直接用 passphrase 或固定派生值加密全部 secret 内容

#### 场景:提升 KDF 成本参数
- **当** 安全策略提高了 `passphrase` protector 的最低 KDF 成本，且本地管理员随后成功解锁 vault
- **那么** 系统必须允许对 vault root key 执行重新包裹或等价升级流程，以提高后续离线攻击成本，而不必重加密全部 secret version

### 需求:passphrase protector 的 KDF 参数必须达到最低安全基线
系统必须为 `passphrase` protector 维护最低 KDF 参数基线，例如最小内存成本、轮数与版本要求。若当前设备无法满足最低基线，系统必须给出明确 diagnostics，并禁止静默降级到过弱参数。

#### 场景:设备资源不足以满足最低内存成本
- **当** 当前宿主设备尝试配置或使用 `passphrase` protector，但实际可用资源不足以满足策略要求的最小 memory-hard KDF 参数
- **那么** 系统必须返回显式 diagnostics，并要求管理员调整策略、换设备或接受受控降级，而不能静默退回到弱参数

### 需求:vault 解锁必须分离触发策略与本地解锁方法策略
系统必须把“何时要求解锁 vault”与“这次解锁允许使用哪些本地方法”建模为分离策略，而不是只保留单一 unlock mode。至少必须能够表达 `on-core-start`、`on-first-secret-access`、`on-every-secret-access` 与等价策略，并允许把 `passkey`、`os-native`、`passphrase` 或受控 fallback 作为可选本地解锁方法。

#### 场景:配置为 core 启动即要求解锁
- **当** 实例的 vault policy 配置为 `on-core-start`
- **那么** 系统必须在 startup 后保持 vault 为 locked，并在完成允许的本地解锁方法前拒绝 secret-backed 操作

#### 场景:配置为首次访问 secret 时才解锁
- **当** 实例的 vault policy 配置为 `on-first-secret-access`
- **那么** 系统可以先启动 core，但在第一次真正访问 vault / secret broker 时必须阻塞到本地解锁完成，而不能在启动时静默预解锁

### 需求:passphrase 必须可作为常规本地解锁因子而不只限恢复迁移
系统必须允许把 `passphrase` 配置为 vault 的常规本地解锁因子，而不只是在恢复、迁移或 break-glass 场景下使用。若策略要求，系统必须支持在 core 启动时、首次 secret access 时或每次 secret access 时要求输入 passphrase 或等价本地证明。

#### 场景:高安全策略要求每次启动输入 passphrase
- **当** 本地管理员把 vault policy 配置为 `trigger=on-core-start` 且允许方法为 `passphrase`
- **那么** 每次 core 启动后系统都必须要求通过 passphrase 完成 vault 解锁，之后才允许 secret-backed 功能继续执行

#### 场景:高安全策略要求每次访问 secret 都重新解锁
- **当** 本地管理员把 vault policy 配置为 `trigger=on-every-secret-access`
- **那么** 系统必须在每次 secret use 前重新满足本地解锁条件，或把缓存窗口限制到仅覆盖本次操作

### 需求:passkey 可作为优先本地解锁方法但不得替代数据面 agent 认证
若本地 UI / control-plane 已配置 passkey 或等价 platform authenticator，系统必须允许把它作为 vault 解锁的优先本地方法。与此同时，系统不得把这种本地解锁结果误当成远端 agent 的 bearer 身份或 model-plane 数据面认证结果。

#### 场景:桌面 UI 配置为 passkey 优先、passphrase 兜底
- **当** 本地管理员为 vault 配置 `preferred_method = passkey`，并允许 `passphrase` 作为 fallback
- **那么** 系统必须优先发起 passkey 或等价平台用户验证；若该方法不可用且策略允许，再回退到 passphrase 解锁

#### 场景:远端 agent 已通过 bearer 认证但 vault 仍锁定
- **当** 一个远端 agent 已通过 model-plane 的 bearer 认证，但当前 vault policy 仍要求本地解锁
- **那么** 系统必须继续把 vault 保持 locked，并等待允许的本地解锁方法完成，而不能因为 bearer 认证通过就直接放行 secret-backed 操作

### 需求:敏感值默认必须通过用途受限的 broker 使用
系统必须把 secret 的日常使用收敛到用途受限的 broker 入口，而不是向 model-facing、provider-facing 或 connector-facing 路径暴露通用 plaintext `get()`。对于 SSH 连接、HTTP 认证、签名或 token 签发等场景，调用方默认只能请求“代为使用”或“代为注入”，而不能直接获得 secret 明文。

#### 场景:provider 需要使用 HTTP token
- **当** 某个 provider 需要对外发起携带认证头的 HTTP 请求
- **那么** 系统必须允许 provider 通过 broker 请求注入认证信息，并且向 provider / 模型返回的结构化结果中不得包含 token 明文

#### 场景:模型请求查看 vault 中的 secret
- **当** 模型或普通 MCP tool 请求直接读取某个凭据的明文内容
- **那么** 系统必须默认拒绝该请求，或仅返回需要本地管理员验证的 reveal / export 入口，而不能把 secret 明文直接返回给模型

### 需求:SSH 私钥必须通过安全交付路径提供给 SSH 连接器
当 target 使用 SSH 私钥认证时，系统必须优先通过 agent-compatible 的本地安全交付路径将私钥提供给 OpenSSH 或等价 SSH 连接程序，而不是将私钥明文写入命令行参数、普通环境变量、配置文件或可长期残留的临时文件。若当前平台暂不具备 agent-compatible 路径，系统只能进入带显式诊断的受控 degraded fallback。

#### 场景:支持 agent-compatible delivery 的平台建立 SSH 连接
- **当** 宿主平台具备可被 SSH 连接程序消费的 agent-compatible endpoint，且 target 配置了 SSH 私钥引用
- **那么** 系统必须优先通过临时 agent broker 或等价 signer endpoint 交付私钥，并保持 SSH publickey auth、host key verification 与常规 interactive / one-shot 行为兼容

#### 场景:仅能使用受控 degraded fallback
- **当** 当前平台尚未具备可用的 agent-compatible delivery，且系统为了兼容 OpenSSH 必须临时使用 identity file fallback
- **那么** 该 fallback 必须使用私有运行目录、严格文件权限、连接级生命周期清理与显式 degraded diagnostics，并且不得把私钥暴露给模型、Artifact 或常规日志

### 需求:受保护的 SSH key import 禁止把 key passphrase 重新带入运行时数据面
系统必须确保：若系统允许导入 passphrase-protected 的 SSH 私钥，则该 passphrase 只能在本地受信任的导入或管理流程中使用。运行时的 SSH broker 或 connector 不得再要求远端 agent、普通 MCP tool 或常规运行链路提供该 key passphrase，且不得把其转写到 argv、prompt transcript、artifact 或日志中。

#### 场景:导入受 passphrase 保护的 OpenSSH 私钥
- **当** 本地管理员导入一个带 passphrase 的 OpenSSH 私钥
- **那么** 系统必须在本地受控流程中完成验证与 canonical 化，并使后续运行时连接能够通过 vault 保护下的 signer material 建立，而不是每次连接都再次暴露该 passphrase

### 需求:高风险保险库管理动作必须支持本地用户验证并兼容 passkey
对于 vault 解锁、secret reveal / export、长期 agent token 签发、权限提升、敏感 secret rotation 或等价的高风险管理动作，系统必须要求本地用户验证。该验证机制在支持的平台上必须能够兼容 passkey / platform authenticator，而不是只依赖“当前 UI 已打开”或“调用来自本机”这类弱判断。

#### 场景:用户签发长期 agent token
- **当** 本地用户请求创建一个可长期使用的 model-plane agent token
- **那么** 系统必须在签发前执行本地用户验证，并允许在支持的平台上通过 passkey 或等价 platform authenticator 完成验证

#### 场景:headless 或暂不支持 passkey 的宿主
- **当** 当前宿主环境暂不具备可用的 passkey / platform authenticator
- **那么** 系统必须要求等价的本地管理员验证或受控 fallback，而不能在无用户验证的情况下直接放行高风险 vault 管理动作

### 需求:本地用户验证结果必须绑定具体管理意图且具有短时效
系统在完成 passkey 或等价本地用户验证后，不得把结果视为一个可长期复用的全局登录态。验证结果必须绑定具体管理动作或有限动作集合，具有短时效，并且只能在本地受信任控制面中使用。

#### 场景:为“创建长期 token”完成本地用户验证
- **当** 本地用户为创建长期 agent token 完成了一次 passkey 或等价本地用户验证
- **那么** 系统只能把这次验证结果用于该次受控签发流程或其短时效内允许复用的有限管理动作，而不能把它无限期复用到后续 secret reveal、权限扩大或其他无关管理动作

#### 场景:远端 agent 尝试复用本地用户验证结果
- **当** 一个远端 bearer agent 请求执行本应依赖本地用户验证的高风险管理动作
- **那么** 系统不得允许该 agent 直接复用先前的本地用户验证结果，除非该结果明确绑定了当前意图、目标对象和短期有效窗口

### 需求:本地用户验证必须通过 intent 与 attestation 两层对象建模
系统必须把“待执行的管理动作意图”和“围绕该意图完成的本地用户验证结果”建模为分离对象。验证结果必须引用具体 intent，并绑定关键管理参数摘要，以防止验证结果被脱离原始动作上下文重放。

#### 场景:为扩大 token scope 创建验证意图
- **当** 本地管理员请求扩大某个 agent token 的 scope
- **那么** 系统必须先创建该次管理动作的 intent，并把目标 token、scope 变化摘要与请求来源绑定到该 intent 上，再允许后续本地用户验证围绕该 intent 执行

#### 场景:验证结果脱离原始 intent
- **当** 某个本地用户验证结果不再能匹配原始 intent、目标对象或参数摘要
- **那么** 系统必须拒绝将其用于执行高风险管理动作，而不能把它当作通用管理员通行证

### 需求:intent 与 attestation 的生命周期必须单次消费且终态一致
系统必须把 `LocalAdminActionIntent` 与 `LocalAdminAttestationRecord` 设计为单向收敛的生命周期对象。attestation 只能被与其匹配的 intent 消费一次；一旦 intent 或 attestation 进入 `consumed`、`expired`、`cancelled`、`revoked` 或等价终态，就不得再被后续管理动作复用。

#### 场景:验证后的 intent 被执行并消费
- **当** 某个已验证的高风险管理动作被系统接纳进入实际执行链路
- **那么** 系统必须消费与之绑定的 intent / attestation，并阻止后续请求再次复用同一组验证对象

#### 场景:intent 过期导致 attestation 失效
- **当** 某个 intent 在短时窗口内未被消费而进入 expired 或 cancelled 状态
- **那么** 与其绑定的 attestation 必须同时变为不可用于后续管理动作，而不能继续作为独立管理员凭证存在

### 需求:本地用户验证必须支持 freshness window 与动作分级
系统必须允许策略为不同管理动作定义不同的 fresh user verification 要求，而不是对所有动作一刀切。至少应区分“必须 fresh 验证”的动作、“可在短 freshness window 内复用”的动作，以及“无需 fresh 验证但仍需受信任本地入口”的动作。

#### 场景:连续执行多个低范围管理动作
- **当** 本地管理员刚完成一次新鲜用户验证，并在短时间内连续执行多个低范围、同类且策略允许复用的管理动作
- **那么** 系统可以在 freshness window 内复用该验证结果，而不必为每个动作都强制重新发起 passkey challenge

#### 场景:高风险动作要求 fresh 验证
- **当** 本地管理员尝试 reveal secret、扩大 token scope、注册 passkey 或降低 model-plane 认证级别
- **那么** 系统必须要求 fresh user verification，而不能只依赖过期的本地验证窗口

### 需求:passkey credential metadata 必须支持 rotation 与 revoke 生命周期
系统必须为本地用户验证层维护稳定的 `PasskeyCredentialRecord`（或等价对象），至少覆盖 `credential_id`、principal 绑定、`rp_id`、公钥、状态、创建时间、最近使用时间、轮换时间、撤销时间与撤销原因。系统不得把“当前 passkey 是否可用”简化为无结构布尔状态。

#### 场景:注册新 passkey 仅持久化安全元数据
- **当** 本地管理员注册一个新的 passkey / platform authenticator 凭据
- **那么** 系统必须仅持久化该凭据的 metadata 与公钥材料，而不能把 passkey 私钥或原始 challenge/response 持久化到普通 metadata store

#### 场景:轮换 passkey 凭据
- **当** 本地管理员为同一 principal 轮换 passkey 凭据
- **那么** 系统必须把新凭据置为 `active`，并将旧凭据置为 `superseded` 或等价非活动状态，同时保留审计链路

#### 场景:撤销 passkey 后强制失效
- **当** 某个 passkey 凭据被管理员撤销或被标记为设备遗失
- **那么** 系统必须将其置为 `revoked` 并拒绝其后续用于 fresh user verification，且不得把撤销凭据重新激活
