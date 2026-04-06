## ADDED Requirements

### 需求:direct identity SSH 启动必须显式旁路 broker 并隔离 ambient agent
当 SSH target 使用本地 identity 文件或等价 direct identity 引用时，系统必须显式绕过 vault broker，并隔离宿主环境中已有的 `SSH_AUTH_SOCK` 或等价 ambient agent 影响，避免错误命中宿主默认 agent 或把 direct identity 路径误报为 broker 故障。

#### 场景:direct identity 启动不创建 broker session
- **当** 某次 one-shot SSH 调用使用本地 identity 文件路径完成认证
- **那么** 系统必须直接为该次调用注入 `-i <path>`、`IdentityFile=<path>` 或等价 direct identity 参数
- **并且** 系统不得创建、attach、detach 或 close 任何 vault broker session

#### 场景:direct identity 启动隔离 ambient agent
- **当** 宿主环境中已经存在 `SSH_AUTH_SOCK` 或等价 ambient agent 配置，且当前 SSH 调用走 direct identity 路径
- **那么** 系统必须在本次结构化执行上下文中清理、覆盖或等价隔离该环境输入
- **并且** 系统不得让 direct identity 结果受到宿主默认 agent 的隐式影响

## MODIFIED Requirements

### 需求:SSH broker endpoint 必须是连接级本地资源并具备自动清理语义
当 target session 使用 vault 管理的 SSH 私钥时，系统必须把对应的 broker endpoint 建模为连接级或逻辑会话级的本地资源，而不是长驻全局代理。该 endpoint 必须具备显式状态、引用计数或等价的附着关系，以及在 session 结束、失败或超时后的自动清理语义。系统不得把“仅分配 endpoint locator 但尚未真实 bind/listen”的状态视为 broker 已 ready。

#### 场景:one-shot SSH 调用创建真实临时 broker
- **当** 某次 one-shot SSH 调用需要使用 vault 中的私钥
- **那么** 系统必须只为该次调用创建临时 broker endpoint
- **并且** 只有在该 endpoint 已真实 bind/listen 并可接受请求后，系统才可以把其下发给 SSH 客户端
- **并且** 在调用结束后系统必须及时清理 endpoint、内存 signer material 与任何 fallback 文件

#### 场景:interactive shell 结束后清理 broker
- **当** 某个 interactive shell 或逻辑 target session 结束，且不再有活动 channel 依赖该 SSH broker
- **那么** 系统必须将该 broker 进入清理流程
- **并且** 必须关闭其本地 endpoint 与相关 runtime handle
- **并且** 禁止后续未重新授权的会话继续复用它

#### 场景:broker bind 失败不得伪装为 ready
- **当** 某个 vault-backed SSH broker session 在平台 endpoint bind、listen 或 adapter startup 阶段失败
- **那么** 系统必须返回受控的 broker startup failure 或等价结构化失败
- **并且** 不得把一个尚未实际就绪的 endpoint locator 提前交给 SSH 客户端

### 需求:secret-backed SSH 启动必须优先使用结构化执行覆盖层
当 SSH 连接依赖 vault broker、`SSH_AUTH_SOCK`、`IdentityAgent` 或等价本地 signer endpoint 时，系统必须优先通过结构化的 `program + args + env overlay` 或等价 direct exec 机制把这些信息传递给 SSH 进程，而不是通过 host shell 字符串拼接、全局环境污染或 shell rc 注入来完成。对于 broker 路径，系统必须确保 SSH 进程只看见当前 invocation 对应的 broker endpoint，而不是宿主原有 agent 环境。

#### 场景:通过结构化 overlay 注入当前 broker endpoint
- **当** 某个平台使用 `SSH_AUTH_SOCK`、`IdentityAgent` 或等价环境变量把临时 broker endpoint 交给 SSH
- **那么** 系统必须把该信息限制在本次 SSH 子进程或等价结构化执行上下文中
- **并且** 不得要求用户 shell 或通用宿主环境长期持有该变量或 locator

#### 场景:broker 启动覆盖层必须屏蔽宿主环境干扰
- **当** 当前 SSH 调用依赖 vault broker 交付私钥，且宿主环境已存在其他 `SSH_AUTH_SOCK` 或等价 agent 配置
- **那么** 系统必须在本次 invocation 中显式覆盖为当前 broker endpoint 或隔离宿主原有配置
- **并且** 不得让宿主默认 agent 抢先参与认证

#### 场景:当前 runtime 仅支持 shell flatten
- **当** 某个 connector 的当前 runtime 仍只能把调用 flatten 为 host shell 字符串
- **那么** 系统必须为 secret-backed SSH 选择 direct exec wrapper 或受控 degraded fallback
- **并且** 不得把 broker 注入信息直接拼进可被日志、transcript 或 shell history 捕获的命令字符串
