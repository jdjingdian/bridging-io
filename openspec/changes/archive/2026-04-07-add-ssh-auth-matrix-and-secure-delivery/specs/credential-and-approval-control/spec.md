## 新增需求

### 需求:SSH target 登录 password 必须支持非 argv 的受控交付路径
当 SSH target 使用登录 password 认证时，系统必须提供不依赖运行时人工输入、且不通过 cmdline 暴露 password 的正式交付路径。系统禁止把 target 登录 password 直接拼接进 argv、shell flatten 命令字符串、普通日志或普通状态文本。

#### 场景:plain SSH password 关闭 SSH 安全访问
- **当** 一个 plain SSH target 选择 `password` 认证，且 `SSH 安全访问` 为关闭
- **那么** 系统必须仍然通过一次性 askpass carrier、受控 stdin wrapper 或等价非 argv 方案完成该次认证
- **并且** password 不得出现在 argv、artifact、普通 transcript 或 display-safe 错误对象中

#### 场景:plain 或 sealed SSH password 使用受控交付
- **当** 一个 plain SSH target 开启 `SSH 安全访问`，或一个 sealed SSH target 选择 `password` 认证
- **那么** 系统必须通过受控的 managed delivery session、等价 broker carrier 或受控 env overlay 交付 password
- **并且** invocation 结束后必须清理 helper、session 与任何临时 carrier 产物

#### 场景:password 交付失败不得退化为运行时输入
- **当** target 登录 password 的 direct 或 managed carrier 无法建立、被策略拒绝或运行失败
- **那么** 系统必须返回受控的结构化失败
- **并且** 不得退化为运行时提示用户重新输入 password

### 需求:本地带 passphrase 的 SSH 私钥必须先导入 vault
当操作员希望使用本地 SSH 私钥路径作为 target 认证材料时，系统必须在 trusted local inspection 阶段检测该私钥是否带 passphrase。若私钥带 passphrase，则系统必须禁止继续沿用“本地路径直接使用”语义，并要求改走 sealed + vault import 路径。

#### 场景:plain target 选择带 passphrase 的本地私钥
- **当** 操作员为 plain SSH target 选择本地私钥路径，且系统检测到该私钥带 passphrase
- **那么** 系统必须阻止继续创建或保存该配置
- **并且** 必须明确提示该组合只允许通过 sealed target + vault import 支持

#### 场景:sealed target 选择带 passphrase 的本地私钥但尚未导入 vault
- **当** 操作员为 sealed SSH target 选择本地私钥路径，且系统检测到该私钥带 passphrase
- **那么** 系统必须阻止继续把该路径作为 direct/local-path 认证材料保存
- **并且** 必须引导用户解锁 vault 并进入 `Import Local SSH Key Into Vault` 或等价受控导入流程

## 修改需求

### 需求:配置文件中禁止保存敏感明文
系统必须默认禁止在 standalone 模式使用的配置文件中保存 SSH 私钥、token 或其他可直接用于认证的高敏明文。除显式放行的 plain SSH password 例外外，配置文件只能保存非敏感设置和 `CredentialRef` 一类的引用信息。对于 `storage_class = plain` 且 `kind = ssh` 且认证类型明确为 `password` 的 target，系统可以在操作员完成正式风险确认后允许将 target 登录 password 明文写入 `config.toml`；除此之外，系统禁止把 password 或等价 secret 明文写入配置文件。

#### 场景:plain SSH target 在确认风险后保存 password
- **当** 操作员为 plain SSH target 选择 `password` 认证，并完成正式风险确认后保存配置
- **那么** 系统可以把该 target 的登录 password 作为 plain 配置字段写入 `config.toml`
- **并且** 该放行必须仅限该 explicit plain SSH password 语义，而不得扩展到 token、私钥或其他 secret 类型

#### 场景:非 plain-password 组合尝试把 secret 明文写入配置
- **当** 用户或系统尝试把 sealed target 的 password、任何 SSH 私钥内容、token 明文或其他等价 secret 明文写入 standalone 配置文件
- **那么** 系统必须拒绝该配置或在保存前阻止写回
- **并且** 必须要求改用 sensitive overlay、vault import、canonical ref 或等价受控 secret 路径

### 需求:SSH 私钥必须通过安全交付路径提供给 SSH 连接器
当 target 使用 SSH 私钥认证时，系统必须区分 vault-managed key、本地未加密私钥的 secure-local 路径，以及本地未加密私钥的 direct identity 路径。对于 canonical vault `credential_ref`，系统必须优先通过真实运行中的 agent-compatible 本地 broker endpoint 将 signer 能力提供给 OpenSSH 或等价 SSH 连接程序，而不是将私钥明文、普通环境变量、配置文件、纯 locator hint 或可长期残留的临时文件交给 SSH 客户端。对于本地未加密私钥，当 `SSH 安全访问` 为开启或被强制要求时，系统必须通过 local brokered identity、等价 signer endpoint 或受控 managed delivery 交付 signer 能力，而不得静默回落到 direct identity；只有在 `SSH 安全访问` 被明确关闭时，本地未加密私钥才允许走显式 identity-file 语义。

#### 场景:支持 agent-compatible delivery 的平台建立 vault-backed SSH 连接
- **当** 宿主平台具备可被 SSH 连接程序消费的 agent-compatible endpoint，且 target 配置了 vault-managed SSH 私钥引用
- **那么** 系统必须优先通过临时 agent broker 或等价 signer endpoint 交付私钥
- **并且** 只有在该 endpoint 已真实 bind/listen 且可接受请求后，系统才可以将其视为 `ready` 并交给 SSH 客户端
- **并且** 该路径必须继续保持 SSH publickey auth、host key verification 与常规 interactive / one-shot 行为兼容

#### 场景:本地未加密私钥在 SSH 安全访问开启时走 secure-local 路径
- **当** target 使用本地未加密私钥路径完成认证，且该 target 当前 `SSH 安全访问` 为开启或被强制要求
- **那么** 系统必须通过 local brokered identity、等价本地 signer endpoint 或受控 managed delivery 交付 signer 能力
- **并且** 系统不得把该调用静默降级为 `-i <path>` 的 direct identity 语义

#### 场景:secure-local 路径失败不得伪装为 direct identity 成功
- **当** 一个本地未加密私钥的 secure-local 认证路径因为 broker startup、carrier 建立或策略拒绝而失败
- **那么** 系统必须返回 secure-local 专属或通用受控失败
- **并且** 不得在未显式关闭 `SSH 安全访问` 的情况下静默回落到 direct identity

### 需求:direct identity SSH 引用必须与 vault broker 路径显式分流
当 SSH target 直接引用本地未加密 identity 文件路径，且该 target 的 `SSH 安全访问` 被明确关闭时，系统必须走显式 `IdentityFile` / `-i` 语义，而不得创建 vault broker session、local broker session、materialize vault secret 或把该路径伪装成 broker delivery。

#### 场景:direct identity 文件通过显式 identity 语义启动 SSH
- **当** SSH target 的当前认证配置指向本地未加密 identity 文件路径，且该引用不是 `vault:` / `vault://` canonical ref，且 `SSH 安全访问` 为关闭
- **那么** 系统必须通过结构化执行参数为该次 SSH 调用注入 `-i <path>`、`IdentityFile=<path>` 或等价显式 identity 语义
- **并且** 系统不得为该调用创建 vault broker session 或 local secure-delivery session

#### 场景:direct identity 失败不得误归类为 broker 故障
- **当** `SSH 安全访问` 为关闭的 direct identity 路径调用失败
- **那么** 系统不得返回 `broker-endpoint-unavailable`、`identity-agent-missing`、`password-delivery-unavailable` 或等价 managed-delivery 专属失败分类
- **并且** 该失败必须继续保留为 direct identity 或通用 SSH 失败语义

## 移除需求
