## ADDED Requirements

### 需求:direct identity SSH 引用必须与 vault broker 路径显式分流
当 SSH target 直接引用本地 identity 文件或等价 direct identity 输入，而不是 canonical vault `credential_ref` 时，系统必须走显式 `IdentityFile` / `-i` 语义，而不得创建 vault broker session、materialize vault secret 或把该路径伪装成 broker delivery。

#### 场景:direct identity 文件通过显式 identity 语义启动 SSH
- **当** SSH target 的当前 credential source 指向本地 identity 文件路径，且该引用不是 `vault:` / `vault://` canonical ref
- **那么** 系统必须通过结构化执行参数为该次 SSH 调用注入 `-i <path>`、`IdentityFile=<path>` 或等价显式 identity 语义
- **并且** 系统不得为该调用创建 vault broker session

#### 场景:direct identity 失败不得误归类为 broker 故障
- **当** direct identity 路径的 SSH 调用失败
- **那么** 系统不得返回 `broker-endpoint-unavailable`、`identity-agent-missing` 或等价 broker 专属失败分类
- **并且** 该失败必须继续保留为 direct identity 或通用 SSH 失败语义

## MODIFIED Requirements

### 需求:SSH 私钥必须通过安全交付路径提供给 SSH 连接器
当 target 使用 SSH 私钥认证时，系统必须区分 vault-managed secret 与 direct identity 两类路径。对于 canonical vault `credential_ref`，系统必须优先通过真实运行中的 agent-compatible 本地 broker endpoint 将 signer 能力提供给 OpenSSH 或等价 SSH 连接程序，而不是将私钥明文、普通环境变量、配置文件、纯 locator hint 或可长期残留的临时文件交给 SSH 客户端。若当前平台暂不具备可用的 broker 路径，系统只能进入带显式诊断的受控 degraded fallback。对于 direct identity 文件路径，系统必须走显式 identity-file 语义，而不得冒充 vault broker。

#### 场景:支持 agent-compatible delivery 的平台建立 vault-backed SSH 连接
- **当** 宿主平台具备可被 SSH 连接程序消费的 agent-compatible endpoint，且 target 配置了 vault-managed SSH 私钥引用
- **那么** 系统必须优先通过临时 agent broker 或等价 signer endpoint 交付私钥
- **并且** 只有在该 endpoint 已真实 bind/listen 且可接受请求后，系统才可以将其视为 `ready` 并交给 SSH 客户端
- **并且** 该路径必须继续保持 SSH publickey auth、host key verification 与常规 interactive / one-shot 行为兼容

#### 场景:broker path 不可用时进入受控 degraded fallback
- **当** 当前平台尚未具备可用的 agent-compatible broker path，且系统为了兼容 OpenSSH 必须临时使用 identity file fallback
- **那么** 该 fallback 必须使用私有运行目录、严格文件权限、连接级生命周期清理与显式 degraded diagnostics
- **并且** 系统不得把 degraded fallback 报告为正常 broker ready

#### 场景:vault broker 路径不得过滤 broker agent 身份
- **当** canonical vault `credential_ref` 通过 broker endpoint 启动 SSH，且宿主 OpenSSH 配置可能存在 `IdentitiesOnly=yes`
- **那么** 系统必须显式采用允许 broker agent 身份参与认证的参数（例如 `IdentityAgent=<endpoint>` 且 `IdentitiesOnly=no` 或等价语义）
- **并且** 系统不得在 broker 路径强制 `IdentitiesOnly=yes` 导致 agent key 被跳过
- **并且** direct identity / identity-file fallback 路径仍必须保持 `IdentitiesOnly=yes` + `IdentityAgent=none` 或等价隔离语义

#### 场景:direct identity 路径不得触发 vault secret delivery
- **当** SSH target 通过本地 identity 文件完成认证，而不是通过 vault-managed secret
- **那么** 系统不得触发 vault secret materialization、broker session startup 或等价 secret-backed delivery
- **并且** 系统必须继续保持该路径不向模型、Artifact 或普通日志暴露私钥内容
