# credential-and-approval-control 规范

## 目的
待定 - 由归档变更 define-bridgingio-foundation 创建。归档后请更新目的。
## 需求
### 需求:凭据必须通过保险库引用访问
系统必须将 SSH 密钥、ADB 密钥、仓库认证信息和其他敏感凭据存储在系统级保险库或等效安全存储中。Profile、Session 和 Artifact 元数据中禁止保存可直接被模型读取的明文凭据，只能保存 `CredentialRef` 一类的引用。

#### 场景:为目标配置 SSH 密钥
- **当** 用户为一个目标导入 SSH 私钥
- **那么** 系统必须将实际私钥保存到保险库，并且在目标 profile 中仅保存凭据引用而不是私钥内容

### 需求:保险库后端必须可替换且保持凭据引用稳定
系统必须通过可替换的保险库后端抽象访问敏感凭据，以兼容系统钥匙串、等效安全存储以及未来可能引入的内置 vault 实现。无论底层后端如何变化，target profile 和配置层引用的都必须是稳定的 `CredentialRef` 语义，而不是后端私有实现细节。

#### 场景:切换到未来的内置 vault 后端
- **当** 部署从系统密钥库切换到内置 vault 或其他安全后端
- **那么** 系统必须能够继续通过既有的凭据引用解析目标配置，而不要求 UI 或目标 profile 改写为后端专有格式

### 需求:配置文件中禁止保存敏感明文
系统必须禁止在 standalone 模式使用的配置文件中保存 SSH 私钥、口令、token 或其他可直接用于认证的明文。配置文件只能保存非敏感设置和 `CredentialRef` 一类的引用信息。

#### 场景:用户手写 standalone 配置文件
- **当** 用户为 standalone core 编写包含 SSH 目标的配置文件
- **那么** 配置文件中必须只出现凭据引用、标签或导入提示，而不能包含可被模型或普通文件读取直接获取的私钥明文

### 需求:高风险操作必须经过审批策略
系统必须在执行写操作、删除操作、特权操作或策略标记为敏感的读取操作之前评估审批策略。若策略要求人工确认，系统必须先创建审批请求，再决定是否继续执行。

#### 场景:执行受策略保护的删除命令
- **当** MCP 客户端请求执行一个被策略判定为高风险的命令
- **那么** 系统必须先生成审批请求，并在审批通过前禁止真正执行该命令

### 需求:特权执行必须隔离模型与凭据
系统必须将普通执行与特权执行区分开来。对于 `sudo` 或等效提权场景，系统必须优先探测无密码执行能力；如果需要交互式凭据，则必须通过本地 UI 完成审批和输入，并且禁止把明文口令返回给模型。

#### 场景:需要密码的 sudo 请求
- **当** 调用方请求执行一个需要交互式 sudo 凭据的命令
- **那么** 系统必须向本地 UI 发起审批与输入流程，并且只向调用方返回批准、拒绝或失败结果，而不返回实际口令

### 需求:`os-native` 保险库后端必须对应真实平台安全存储或显式降级语义
当配置选择 `os-native` 保险库后端时，系统必须将其映射到当前宿主平台的真实安全存储实现，或在当前平台暂不可用时返回明确的降级 / 不可用语义。系统不得把 `os-native` 静默退化为普通内存实现并继续对外宣称其为平台原生安全存储。

#### 场景:受支持平台上使用 `os-native`
- **当** 宿主平台具备受支持的原生安全存储能力，且配置选择 `os-native` backend
- **那么** 系统必须将凭据保存到对应平台的原生安全存储，并继续对外暴露稳定的 `CredentialRef`

#### 场景:当前平台暂不支持原生安全存储
- **当** 宿主平台暂未实现 `os-native` backend 所需的真实平台绑定
- **那么** 系统必须返回明确的诊断、依赖缺失或受控降级结果，而不是静默把敏感数据保存在普通内存后仍报告 backend 为 `os-native`

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

### 需求:桌面设置界面必须提供 token 安全摘要与 revoke 入口
跨平台桌面控制台必须在受信任的本地设置界面中提供 token 安全摘要列表与 revoke 入口，而不是要求用户先进入深层 vault 页面或通过其他管理工具间接处理 token。该列表必须继续遵守 display-safe projection 约束，禁止再次展示明文 token 或认证内部真相。

#### 场景:用户从设置页面查看 token 列表
- **当** 用户进入桌面控制台的设置与安全管理区域
- **那么** 系统必须返回 token 的安全摘要列表，并允许用户在同一受信任管理面中理解 token 的 label、状态与生命周期，而不是只提供一个抽象的“管理 token”入口

#### 场景:用户在设置页面 revoke token
- **当** 用户在桌面控制台的设置与安全管理区域请求撤销某个 token
- **那么** 系统必须在本地受信任管理面中完成 revoke 流程并返回更新后的安全摘要，而不是要求用户切换到其他工具或依赖未来的外部页面

### 需求:桌面安全管理动作必须通过本地用户验证完成
对于 vault 解锁、长期 token 签发以及等价高风险的本地安全管理动作，跨平台桌面控制台必须通过受信任宿主触发本地用户验证，而不能把“当前页面已打开”或“操作来自 bundled webview”视为足够的授权条件。

#### 场景:用户在桌面控制台中解锁 vault
- **当** 用户在桌面控制台中发起 vault 解锁
- **那么** 系统必须通过受信任本地宿主触发所需的本地用户验证或受控 fallback，而不是把 bundled webview 的普通交互直接视为已授权

#### 场景:用户在桌面控制台中创建长期 token
- **当** 用户在桌面控制台中请求创建一个长期 agent token
- **那么** 系统必须在签发前执行本地用户验证，并且只向 UI 返回一次性签发结果与安全摘要，而不是绕过本地验证直接创建

### 需求:canonical vault runtime 必须作为唯一的持久化真相层
实现 `design-vault-and-agent-auth` 时，系统必须把 `builtin-encrypted` 作为 vault 的唯一 canonical runtime/store 语义。legacy `os-native` backend 只能作为 protector 兼容输入读取，不得继续被实现为直接保存 secret 的长期真相层。

#### 场景:升级实例读取 legacy `os-native` backend
- **当** 已升级实例读取到 legacy `[vault] backend = "os-native"` 配置
- **那么** runtime 必须把它解释为 canonical `builtin-encrypted` vault 加 `os-native` primary protector，而不是继续把 `os-native` 当作直接保存 secret 的 store

#### 场景:持久化设置时回写 canonical 配置
- **当** 已升级实例完成设置持久化或 config rewrite
- **那么** 系统必须回写 canonical vault 配置结构，而不是继续输出 legacy `backend = "os-native"` 作为长期真相

### 需求:高风险 vault 与 token 管理动作必须消费真实 attestation
对于 vault 解锁、vault delete、secret reveal/export、长期 token 签发、token 删除、token scope 扩大和敏感 secret rotation，系统必须校验并消费真实存在、未过期、与当前 intent 和 payload digest 匹配的 `LocalAdminAttestationRecord`。系统不得接受 UI 或 CLI 传入的占位 attestation id 作为等价授权。

#### 场景:前端传入 synthetic attestation id
- **当** 桌面 UI、TUI 或其他 trusted client 在未先创建 intent/attestation 的情况下，直接对 `unlock_vault`、`create_agent_token`、`delete_agent_token` 或 `delete_vault` 传入一个固定占位 attestation id
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

### 需求:启动解锁载体必须收敛到最小安全集合
BridgingIO 的 vault 启动解锁路径必须收敛到最小安全集合，以降低额外泄露面。对于正式 startup/unlock contract，系统只允许使用隐藏输入的本地终端 prompt、父进程一次性本地 carrier，或受信任本地验证触发的正式 unlock 路径。系统禁止把明文 argv、普通环境变量或普通文件作为正式启动解锁载体。

#### 场景:standalone 前台需要解锁
- **当** standalone 前台模式在启动阶段或首次 secret access 阶段需要输入 unlock material
- **那么** 系统必须通过隐藏输入的本地终端 prompt 获取该材料，而不得要求操作员通过 argv、普通环境变量或普通文件提供明文口令

#### 场景:standalone 后台或服务需要解锁
- **当** standalone 后台/服务模式在启动阶段需要 unlock material
- **那么** 系统必须只接受父进程提供的一次性本地 carrier，而不得要求后台进程在启动后再等待普通交互式输入

### 需求:启动解锁失败必须保持 fail-closed
若 policy 要求在启动阶段完成解锁，但系统未能通过允许的正式 carrier 成功获取 unlock material，则 core 必须保持 `locked` 或 `unavailable`，并拒绝 secret-backed 功能。系统禁止在这种情况下隐式降级为“先运行，稍后再看”。

#### 场景:carrier 不可用或材料无效
- **当** core 在启动阶段检测到 unlock material 缺失、carrier 不可用或输入验证失败
- **那么** 系统必须保持 fail-closed 状态，并返回明确恢复提示，而不是把 vault 伪装为已准备就绪

### 需求:vault 与本地验证错误必须映射到共享错误与状态契约
BridgingIO 在 vault、token、intent、attestation、approval 与本地验证路径上返回的公共错误，必须映射到共享错误与状态契约，同时保留安全域专属子码。系统不得把 `locked`、`verification_required`、`attestation_mismatch`、`passphrase_rejected` 等专属语义统一压扁为普通 validation failure。

#### 场景:本地管理员验证失败
- **当** 用户或本地受信任调用面在执行高风险安全动作时遇到验证缺失、验证过期或 attestation 不匹配
- **那么** 系统必须返回共享错误封装，并保留安全域专属子码，以便调用方稳定区分“需要重新验证”和“真正内部失败”

### 需求:安全域公共错误必须默认保持 display-safe
BridgingIO 在 vault、token、secret broker 和 approval 路径中对外暴露的错误 message 与 details 必须默认保持 display-safe，不得把 secret 明文、token 明文、密文 locator 或等价高敏内部字段带入公共错误对象。

#### 场景:secret-backed 操作失败
- **当** 一个 secret-backed 操作因为 vault unavailable、broker delivery 失败或 policy 拒绝而失败
- **那么** 系统必须向公共调用面返回 display-safe 的错误摘要和恢复提示，而不能把内部 secret 材料或 locator 信息拼入错误消息

### 需求:display-safe vault 状态投影不得隐式触发本地解锁
系统在返回 display-safe 的 vault lock state、策略摘要、summary、protector diagnostics 或等价 readiness 投影时，必须将“读取状态”与“执行解锁”分离。状态投影路径禁止消费 unlock material、调用正式 unlock handler、访问会触发用户验证的 platform keyring 接口，或产生等价的本地解锁副作用。

#### 场景:受信任本地界面只请求 vault 摘要
- **当** `menuconfig`、桌面控制台或其他受信任本地界面只请求 lock state、allowed methods、token summary 或等价 display-safe 摘要
- **那么** runtime 必须返回被动状态投影，而不得因此触发系统钥匙串、passkey、passphrase prompt 或等价解锁交互

#### 场景:未显式声明 unlock intent
- **当** 当前请求未显式声明 unlock intent，也未调用 `unlock_vault` 或等价正式解锁路由
- **那么** 系统必须保持 vault 当前 lock state，而不得因为 readiness 检查、router 初始化或 protector 探测把状态推进到 `unlocking` 或 `unlocked`

#### 场景:平台只能通过用户验证判断 os-native readiness
- **当** 某个平台只有在触发用户验证后才能精确判断 `os-native` protector 是否可用
- **那么** 系统必须返回受控的 deferred/unknown readiness 或等价 display-safe 诊断，而不得为了获取更精确状态提前触发验证

#### 场景:router 初始化不得触发平台 keyring 验证
- **当** runtime 仅执行 router 默认初始化、持久化元数据加载或 display-safe 摘要读取
- **那么** 系统不得访问会触发系统弹窗的 keyring 验证路径，避免把一次显式解锁放大为多次系统认证请求

#### 场景:verified os-native 解锁需要兼容历史 fallback wrap
- **当** vault 历史 root wrap 由 fallback KEK 生成，而当前显式解锁走 verified `os-native` 路径
- **那么** 系统必须在验证成功后允许兼容解包，并自动重包裹迁移到当前 verified KEK，避免“系统验证成功但 root key 解包失败”

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

### 需求:长期 agent token 必须支持可逆的本地禁用状态
系统必须允许本地受信任管理面在不执行 `revoke` 的前提下，把长期 agent token 切换为临时不可访问的 `disabled` 管理状态。该状态必须与 `revoked / expired / deleted` 等终态区分，并通过独立的 enable flag 或等价受控语义持久化。系统必须拒绝 disabled token 的后续认证，但在 token 尚未 `expired`、`revoked` 或 `deleted` 时允许重新启用。

#### 场景:操作员禁用一个 active token
- **当** 本地受信任管理面把一个当前处于可访问状态的 token 切换为 `disabled`
- **那么** 系统必须持久化该状态，并在后续 bearer 认证时拒绝该 token

#### 场景:操作员重新启用一个未过期的 disabled token
- **当** 本地受信任管理面重新启用一个尚未过期、尚未撤销且尚未删除的 disabled token
- **那么** 系统必须允许该 token 恢复访问，而不要求重新签发新 token

#### 场景:disabled token 在终态后不得被重新激活
- **当** 某个 disabled token 在后续进入 `expired`、`revoked` 或 `deleted` 终态
- **那么** 系统不得再通过启用开关让其恢复访问，而必须继续按对应终态拒绝使用

### 需求:token 别名写入必须遵循统一字符约束
系统必须对 token 的 operator-facing 别名执行统一写入校验。新创建或更新的 token 别名必须先做 trim，长度必须保持在 1 到 64 个字符之间，并且只能包含 ASCII 字母、数字、`-`、`_`；系统禁止把空格、制表符或其他分隔符写入持久化别名。系统必须允许历史版本遗留的不合规别名继续被读取与展示，但不得允许新的不合规值被写回。

#### 场景:写入带连字符或下划线的别名
- **当** 本地受信任管理面创建或更新 token，并把别名设置为 `test_tok` 或 `test-tok2`
- **那么** 系统必须接受该值并将其持久化为 token 的 operator-facing 别名

#### 场景:写入包含空格的别名
- **当** 本地受信任管理面创建或更新 token，并把别名设置为 `test tok`
- **那么** 系统必须拒绝该请求，并返回明确的别名校验失败语义

#### 场景:读取历史遗留的不合规别名
- **当** 某个 token 在旧版本中保存了不符合新规则的别名
- **那么** 系统必须继续允许受信任管理面读取并展示该别名；并且只有在更新该字段时才强制要求新值符合当前规则

### 需求:os-native 解锁链路不得因诊断路径重复触发钥匙串授权
在 macOS/Windows 的 `os-native` 保护器模式下，系统必须把“能力诊断/可用性探测”与“显式解锁验证”分离：只读状态投影、readiness 诊断和 menuconfig 页面刷新不得触发钥匙串读写；仅在操作员明确执行 `unlock_vault`（或等价解锁动作）时才允许访问平台钥匙串完成受验证解锁。对于同一次解锁流程，系统不得因内部重复探测导致额外授权弹窗；若钥匙串访问控制（ACL）策略仍要求用户确认，单次确认属于平台安全行为，不应被视为业务回归。

#### 场景:浏览 menuconfig 安全摘要时不触发钥匙串授权
- **当** 操作员仅进入 Security/Token Management 页面查看摘要信息，未触发 `unlock_vault`
- **那么** 系统不得访问 `io.bridgingio.vault` 对应钥匙串项，也不得弹出钥匙串授权对话框

#### 场景:解锁流程只执行一次受验证钥匙串访问
- **当** 操作员在 menuconfig 中触发一次 `unlock_vault`
- **那么** 系统必须把钥匙串访问收敛为单次受验证读取语义，不得因为 readiness probe、后台诊断或重复 KEK 获取在同一流程内再次触发同类访问

#### 场景:钥匙串 ACL 要求额外确认时按平台行为处理
- **当** macOS 钥匙串访问控制未将当前 `bridgingio-core` 可执行体标记为“始终允许”
- **那么** 平台仍可能弹出一次“访问钥匙串中的密钥”确认；该提示属于平台 ACL 行为，系统应保持可预期且不重复触发

