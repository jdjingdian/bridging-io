## 新增需求

### 需求:SSH target 运行时必须把认证配置解析为结构化 delivery plan
当 runtime 为 SSH target 准备 one-shot、interactive shell 或 test connection 时，系统必须根据 typed SSH auth 配置解析出稳定的结构化 delivery plan，而不是继续只依赖 `credential_ref` 或零散布尔值推断。该 plan 至少必须区分 `None`、`PasswordDirectAskpass`、`PasswordManagedAskpass`、`DirectIdentityFile`、`LocalBrokeredIdentity` 与 `VaultBrokeredIdentity` 等正式语义。

#### 场景:password 认证根据 SSH 安全访问切换 delivery plan
- **当** SSH target 使用 `password` 认证
- **那么** plain + `SSH 安全访问 = false` 必须解析为 direct password carrier plan
- **并且** plain + `SSH 安全访问 = true` 或 sealed + `password` 必须解析为 managed password delivery plan

#### 场景:本地未加密私钥根据 SSH 安全访问切换 delivery plan
- **当** SSH target 使用本地未加密私钥路径完成认证
- **那么** `SSH 安全访问 = false` 必须解析为 `DirectIdentityFile`
- **并且** `SSH 安全访问 = true` 或 sealed 强制安全访问必须解析为 `LocalBrokeredIdentity`

#### 场景:vault-managed 私钥固定走 vault broker
- **当** SSH target 使用 canonical vault `credential_ref` 进行私钥认证
- **那么** 系统必须解析为 `VaultBrokeredIdentity`
- **并且** 不得为该组合再提供关闭 `SSH 安全访问` 的 direct path

### 需求:structured SSH 执行必须支持受控 env overlay 与 carrier 清理
当 SSH 认证依赖 password askpass、broker endpoint、helper 路径或等价 invocation-scoped carrier 时，structured invocation 必须支持受控 env overlay、helper 生命周期清理与 display-safe 诊断。系统不得为了 password secure delivery 回退到 shell flatten、全局环境污染或长期驻留 helper。

#### 场景:password carrier 仅存在于当前 invocation 的 env overlay
- **当** runtime 为某次 SSH 调用建立 askpass helper、password env 变量或等价 carrier
- **那么** 系统必须把这些值限制在当前结构化执行上下文中
- **并且** 不得要求宿主 shell、全局环境或后续无关调用继续持有该 carrier

#### 场景:password carrier 与 helper 在调用结束后清理
- **当** 一次依赖 password carrier 的 SSH 调用成功、失败、取消或超时结束
- **那么** 系统必须清理 invocation-scoped helper、env carrier 与等价临时资源
- **并且** 不得让后续调用复用上一轮 password helper 或临时文件

#### 场景:password delivery 覆盖参数不得与默认 probe 参数冲突
- **当** runtime 为 password 路径注入 `BatchMode`、`NumberOfPasswordPrompts`、`PreferredAuthentications`、`PubkeyAuthentication` 等 SSH 选项
- **那么** 系统必须先移除同 key 的默认选项或历史残留选项，再注入 password delivery 选项
- **并且** 不得在同一次调用中并存 `BatchMode=yes/no`、`NumberOfPasswordPrompts=0/1` 等互斥配置

## 修改需求

### 需求:secret-backed SSH 启动必须优先使用结构化执行覆盖层
当 SSH 连接依赖 vault broker、managed password delivery、`SSH_AUTH_SOCK`、`IdentityAgent`、`SSH_ASKPASS` 或等价本地认证 carrier 时，系统必须优先通过结构化的 `program + args + env overlay` 或等价 direct exec 机制把这些信息传递给 SSH 进程，而不是通过 host shell 字符串拼接、全局环境污染或 shell rc 注入来完成。对于 managed 路径，系统必须确保 SSH 进程只看见当前 invocation 对应的 carrier，而不是宿主原有 agent / askpass 环境。

#### 场景:通过结构化 overlay 注入当前 broker endpoint
- **当** 某个平台使用 `SSH_AUTH_SOCK`、`IdentityAgent` 或等价环境变量把临时 broker endpoint 交给 SSH
- **那么** 系统必须把该信息限制在本次 SSH 子进程或等价结构化执行上下文中
- **并且** 不得要求用户 shell 或通用宿主环境长期持有该变量或 locator

#### 场景:通过结构化 overlay 注入 password askpass carrier
- **当** 某次 SSH 调用依赖 askpass helper、`SSH_ASKPASS`、`SSH_ASKPASS_REQUIRE` 或等价 password carrier
- **那么** 系统必须通过结构化 env overlay 把这些信息限制在当前 invocation
- **并且** 不得把 password 或 helper payload 直接拼进 host shell 命令字符串

#### 场景:debug/trace SSH probe 必须产出可比较的上下文摘要
- **当** menuconfig `Test Connection` 在 debug 或 trace 级别执行 SSH probe
- **那么** 系统必须在 verbose 日志中记录 display-safe 的 probe 上下文摘要（至少包括 delivery plan、env overlay 键名、关键 `-o` 覆盖项与 pre-spawn carrier 状态）
- **并且** 该摘要不得包含 password 明文、私钥明文或 helper payload 内容

#### 场景:当前 runtime 仅支持 shell flatten
- **当** 某个 connector 的当前 runtime 仍只能把调用 flatten 为 host shell 字符串
- **那么** 系统必须为 secret-backed SSH 选择 direct exec wrapper 或受控失败
- **并且** 不得把 broker / askpass 注入信息直接拼进可被日志、transcript 或 shell history 捕获的命令字符串

### 需求:direct identity SSH 启动必须显式旁路 broker 并隔离 ambient agent
当 SSH target 使用本地未加密 identity 文件路径，且当前 `SSH 安全访问` 被明确关闭时，系统必须显式绕过 vault broker、local broker 与 managed password delivery，并隔离宿主环境中已有的 `SSH_AUTH_SOCK` 或等价 ambient agent 影响，避免错误命中宿主默认 agent 或把 direct identity 路径误报为 broker 故障。

#### 场景:direct identity 启动不创建 broker session
- **当** 某次 one-shot SSH 调用使用本地未加密 identity 文件路径完成认证，且 `SSH 安全访问` 为关闭
- **那么** 系统必须直接为该次调用注入 `-i <path>`、`IdentityFile=<path>` 或等价 direct identity 参数
- **并且** 系统不得创建、attach、detach 或 close 任何 vault broker session 或 local secure-delivery session

#### 场景:direct identity 启动隔离 ambient agent
- **当** 宿主环境中已经存在 `SSH_AUTH_SOCK` 或等价 ambient agent 配置，且当前 SSH 调用走 `SSH 安全访问 = false` 的 direct identity 路径
- **那么** 系统必须在本次结构化执行上下文中清理、覆盖或等价隔离该环境输入
- **并且** 系统不得让 direct identity 结果受到宿主默认 agent 的隐式影响

## 移除需求
