## ADDED Requirements

### 需求:SSH broker endpoint 必须是连接级本地资源并具备自动清理语义
当 target session 使用 vault 管理的 SSH 私钥时，系统必须把对应的 broker endpoint 建模为连接级或逻辑会话级的本地资源，而不是长驻全局代理。该 endpoint 必须具备显式状态、引用计数或等价的附着关系，以及在 session 结束、失败或超时后的自动清理语义。

#### 场景:one-shot SSH 调用创建临时 broker
- **当** 某次 one-shot SSH 调用需要使用 vault 中的私钥
- **那么** 系统必须只为该次调用创建临时 broker endpoint，并在调用结束后及时清理 endpoint、内存 signer material 与任何 fallback 文件

#### 场景:interactive shell 结束后清理 broker
- **当** 某个 interactive shell 或逻辑 target session 结束，且不再有活动 channel 依赖该 SSH broker
- **那么** 系统必须将该 broker 进入清理流程，并禁止后续未重新授权的会话继续复用它

### 需求:secret-backed SSH 启动必须优先使用结构化执行覆盖层
当 SSH 连接依赖 vault broker、`SSH_AUTH_SOCK`、`IdentityAgent` 或等价本地 signer endpoint 时，系统必须优先通过结构化的 `program + args + env overlay` 或等价 direct exec 机制把这些信息传递给 SSH 进程，而不是通过 host shell 字符串拼接、全局环境污染或 shell rc 注入来完成。

#### 场景:通过 env overlay 注入临时 agent endpoint
- **当** 某个平台使用 `SSH_AUTH_SOCK` 或等价环境变量把临时 agent endpoint 交给 SSH
- **那么** 系统必须把该变量限制在本次 SSH 子进程或等价结构化执行上下文中，而不能要求用户 shell 或通用宿主环境长期持有该变量

#### 场景:当前 runtime 仅支持 shell flatten
- **当** 某个 connector 的当前 runtime 仍只能把调用 flatten 为 host shell 字符串
- **那么** 系统必须为 secret-backed SSH 选择 direct exec wrapper 或受控 degraded fallback，而不能把 broker 注入信息直接拼进可被日志、transcript 或 shell history 捕获的命令字符串

### 需求:standalone 配置必须声明 vault / unlock 策略而不保存敏感明文
对于 standalone 模式，core-owned 配置必须允许操作员声明 vault backend、namespace、unlock policy、以及 SSH 私钥交付相关策略，但不得在 TOML 或等价配置文件中保存 secret 明文。standalone 配置只允许保存非敏感策略字段、路径、模式、`CredentialRef`，以及诸如 `trigger_policy`、`allowed_methods`、`preferred_method`、`cache_ttl_sec` 这类 unlock 策略参数。

#### 场景:内网 standalone 部署声明 vault 策略
- **当** 操作员在内网服务器上为 standalone core 配置 vault backend、unlock mode 与 model-plane auth 策略
- **那么** 配置文件必须能够表达这些策略，但不得要求操作员把 vault passphrase、SSH 私钥或 agent token 明文写入配置文件

#### 场景:standalone 配置声明 SSH key delivery 策略
- **当** 操作员为 standalone core 配置 SSH target，且需要声明默认的 SSH 私钥交付模式
- **那么** 配置文件必须能够表达诸如 `ssh-agent broker` 或受控 fallback 这类策略选择，但不得直接包含私钥内容

#### 场景:standalone 配置声明启动即锁定
- **当** 操作员为 standalone core 配置 `trigger_policy = on-core-start`，并声明 `passphrase` 或其他允许的本地解锁方法
- **那么** 配置文件必须能够表达这一 unlock 策略，但不得要求把实际 passphrase 或其他明文解锁材料写入配置

### 需求:standalone 的 vault / auth 管理入口必须与 `run` 分离并避免明文 argv
standalone 模式下，系统必须为 vault 初始化、secret 导入、vault 解锁、agent token 创建和 revoke 等管理动作提供显式的本地管理入口，例如独立 CLI 子命令或等价的受信任本地 control-plane 动作。该管理入口必须支持通过 stdin、file、fd 或受控本地 prompt 传递 secret 材料，并且不得要求操作员通过命令行参数直接传入 secret 明文。

#### 场景:操作员导入 SSH 私钥
- **当** 操作员在 standalone 环境中为某个 target 导入 SSH 私钥
- **那么** 系统必须允许其通过 `stdin`、文件、fd 或受控本地 prompt 导入该私钥，而不能要求使用 `--private-key <plaintext>` 一类的 argv 明文方式

#### 场景:standalone 运行时提供 unlock 材料
- **当** 操作员需要在 standalone `run` 流程中向 core 提供 vault 解锁材料
- **那么** 系统可以允许声明 unlock source，例如 env / fd / prompt，但不得把 unlock secret 本身设计成命令行字面量参数

### 需求:standalone secret 输入路线必须具备确定优先级与审计语义
系统必须为 `stdin`、`file`、`fd`、`tty prompt` 四类 secret 输入路线定义确定优先级与冲突处理语义，避免多输入源并存时的歧义。推荐优先级为 `fd > stdin > file > tty prompt`。若一次管理动作显式声明了多个输入源，系统必须 fail closed 并返回可诊断错误。

#### 场景:多输入源冲突时拒绝执行
- **当** 操作员在同一次 secret 导入或 vault 解锁动作中同时显式提供多个输入源（例如 `--from-fd` 与 `--from-file`）
- **那么** 系统必须拒绝该请求并提示冲突输入源，而不能静默猜测使用哪个来源

#### 场景:记录输入源审计而不泄露 secret
- **当** 系统接收并处理一次 standalone secret 输入
- **那么** 审计事件必须记录 `source_kind`、`intent_id`、operator、字节长度与来源摘要等最小字段，同时不得记录 secret 明文、完整输入路径、完整 fd 值或 prompt 原文

### 需求:standalone 后续必须允许声明 passphrase protector 的策略参数而不暴露口令本身
对于支持备份恢复与迁移的 standalone 部署，后续配置与管理入口必须允许声明 `passphrase` protector 的策略，例如 KDF 方案、成本等级、是否启用 recovery protector，但不得在配置文件、argv、日志或常规脚本输出中暴露 passphrase 明文。

#### 场景:standalone 初始化可恢复 vault
- **当** 操作员在 standalone 场景下初始化一个支持备份恢复的 vault，并选择启用 `passphrase` protector
- **那么** 系统必须允许其声明 `Argon2id` 或等价 memory-hard KDF 的策略参数，但不得要求把实际 passphrase 写入 TOML、普通环境变量快照或命令行字面量参数

### 需求:standalone 后续必须允许声明常规解锁触发点与方法偏好
对于 standalone 的 future route，系统必须允许操作员声明 vault 是否在 core 启动时、首次 secret access 时或每次 secret access 时要求本地解锁，并允许为这些触发点声明方法偏好，例如 `passphrase-only`、`passkey-preferred` 或等价策略。

#### 场景:standalone 声明首次访问 secret 时再解锁
- **当** 操作员为 standalone core 配置 `trigger_policy = on-first-secret-access`
- **那么** 系统必须允许 core 在 locked 状态下先启动，并在第一次 secret-backed 操作到来时再要求完成允许的本地解锁

### 需求:standalone 的本地管理面后续必须只暴露规范化引用与安全投影
即使在 standalone 模式下，后续本地管理入口也必须把 vault / token 的内部 record 与展示结果分离。`vault list`、`auth token list`、诊断页或等价管理界面只能展示 canonical `CredentialRef`、token summary、状态与策略摘要，而不能把 secret 明文、token 明文、`token_hash`、`ciphertext_locator` 或等价内部字段暴露给操作员界面、日志或脚本输出。

#### 场景:standalone 列出已导入的 secret
- **当** 操作员通过未来的 standalone 管理入口列出当前 vault 中的 secret
- **那么** 系统只能返回 canonical `CredentialRef`、label、kind、status 与 rotation 摘要，而不能把密文定位信息或 secret 内容输出到命令结果

#### 场景:standalone 创建 token 后再次查询
- **当** 操作员通过未来的 standalone 管理入口创建了一个 agent token，并在稍后再次查看该 token
- **那么** 首次签发后系统最多只允许一次性显示明文 token；此后所有查询都只能返回安全投影摘要

### 需求:内网 / headless standalone 必须提供最小操作手册与安全默认值建议
系统必须为内网 / headless 部署提供最小可执行的操作手册与安全默认值建议，覆盖 vault 初始化、secret 导入、解锁、token 签发与轮换撤销的顺序化流程，并明确禁止把 secret 明文写入 TOML、argv、常规日志或脚本输出。

#### 场景:按推荐默认值初始化 headless 部署
- **当** 操作员首次部署 headless standalone core 并采用推荐默认值
- **那么** 系统文档必须至少推荐启用 `model_plane.http.auth.mode = bearer`、非 loopback 强制认证、`ssh-agent broker` 优先交付与最小权限 token 签发策略

#### 场景:按操作手册执行周期安全维护
- **当** 操作员按系统提供的内网 / headless 操作手册执行日常维护
- **那么** 手册必须包含 token 轮换 / 撤销、vault protector 状态检查与 KDF 参数基线复核步骤，且默认流程不得要求明文 secret 出现在 argv

### 需求:`--self-test` 必须覆盖本次变更已落地的 vault / auth contract smoke
对于 `design-vault-and-agent-auth` 已在 core 中落地的能力，`bridgingio-core --self-test` 必须提供无配置 smoke 入口，验证 canonical `CredentialRef`、vault fail-closed、broker-only secret use、本地管理员验证单次消费、secret-backed SSH delivery，以及 non-loopback model-plane 安全默认值。该 smoke 必须继续通过进程退出码反映结果。

#### 场景:操作员运行包含 vault / auth smoke 的 `--self-test`
- **当** 操作员执行 `bridgingio-core --self-test`
- **那么** 系统必须在原有 shell / runtime smoke 之外，继续执行本次变更的 vault / auth contract smoke；任一子项失败时返回非零退出码

#### 场景:当前宿主验证 secret-backed SSH delivery
- **当** 操作员执行 `bridgingio-core --self-test`，且 self-test 进入 secret-backed SSH delivery smoke
- **那么** 系统必须根据当前宿主平台验证 broker endpoint lifecycle 或受控 fallback 诊断，并确认 session cleanup 语义仍成立
