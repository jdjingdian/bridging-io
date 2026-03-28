## 新增需求

### 需求:终端型 target 的会话与通道并发必须服从其并发策略
对于 terminal family 成员，系统必须依据 target 声明的并发策略管理 logical session、transport session 与 channel。系统禁止继续假定所有终端型目标都具有与 SSH 相同的多窗口并发特性。

#### 场景:SSH 或 ADB target 允许多窗口并发
- **当** 一个 `ssh` 或 `adb` target 被声明为 `multiplexed`，且同一访问作用域内已经存在一个活动逻辑会话
- **那么** 系统必须允许该逻辑会话下继续打开多个并发 channel 或窗口，并保持它们共享逻辑会话审计关系但彼此隔离 shell 状态

#### 场景:exclusive terminal target 阻止第二个活动访问
- **当** 一个 terminal target 被声明为 `exclusive`，且该 target 已存在活动 transport 或活动交互 channel
- **那么** 系统必须拒绝新的并发访问并返回明确的 lease/busy 语义，而不是静默创建第二个会话或第二个窗口

### 需求:独占型终端 target 必须以 target 级 lease 管理跨作用域竞争
对于 `exclusive` terminal target，系统必须把独占约束提升到 target 级别，而不是只在单个逻辑会话内生效。不同 agent、不同 run、不同 thread 或不同客户端会话访问同一独占 target 时，系统必须显式处理 lease 冲突。

#### 场景:另一个 agent 访问已占用的 serial target
- **当** agent A 已经持有某个 `exclusive` terminal target 的活动 transport，而 agent B 在不同访问作用域下尝试访问同一 target
- **那么** 系统必须返回明确的冲突或占用状态，并禁止复用或并行建立第二个活动 transport

#### 场景:独占 lease 释放后允许下一次访问
- **当** 持有独占 lease 的 transport 或交互 channel 已关闭
- **那么** 系统必须允许后续请求重新获取该 target 的访问权，而不是把该 target 永久标记为不可用

### 需求:future localshell target 必须沿用统一的 target/session/audit 语义
future `localshell` target 一旦进入 terminal family，系统必须按普通 target 处理其 profile、logical session、transport session、channel、artifact 与 approval 关联。系统禁止因为其底层执行依赖宿主 runtime，就把它降级为不进入 target/session 模型的本地快捷路径。

#### 场景:future localshell target 打开 interactive shell
- **当** 用户或 AI 为 future `localshell` target 打开 interactive shell
- **那么** 系统必须为其创建与 SSH / ADB 一致的 logical session、transport session 与 channel 记录，并将 transcript 与 artifact 关联到该 target，而不是只保留宿主 shell 进程状态

#### 场景:宿主诊断与 target 会话保持边界
- **当** 系统同时暴露宿主 `HostPlatformAdapter` 诊断与 future `localshell` target 会话信息
- **那么** 宿主平台诊断必须继续按 host scope 输出，而 `localshell` target 的命令、审批与 artifact 必须继续按 target/session scope 输出，二者不得混为同一条数据边界

## 修改需求

## 移除需求
