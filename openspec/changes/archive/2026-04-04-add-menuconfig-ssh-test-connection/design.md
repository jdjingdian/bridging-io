## 上下文

当前 `menuconfig` 已经把 SSH target 的编辑路径组织为单栏、逐级进入的拓扑：plain target 在 `Target Editor -> Connection Profile` 中编辑连接字段，sealed target 在 `Target Editor -> Sensitive Overlay` 中编辑敏感连接字段；对于 sealed target，vault `locked` 时页面会被正式裁剪为锁定提示与 `Unlock Vault --->`。这一拓扑已经天然提供了 plain / sealed 两类 SSH target 的差异化入口位置。

与此同时，现有实现还缺少一个正式的“连接探测”流程。操作员可以在 `menuconfig` 中修改主机、端口、用户名和 `credential_ref`，但在触发 `Apply Target` 或 `Save` 之前，没有办法确认当前草稿配置是否真的能建立 SSH 连接。对于 imported vault SSH key、target 级 toolchain override 或 sealed overlay 来说，这会让错误发现明显滞后。

实现上还存在一个重要约束：`bridgingio-operator-console` 当前并不直接依赖 `bridgingio-mcp` 的 target 执行路径，但仓库里真正完整的 SSH one-shot 执行、toolchain 解析和 secret-backed SSH delivery 逻辑已经存在于 `bridgingio-mcp`、`bridgingio-connectors` 与 `bridgingio-secrets` 一侧。本设计既要避免在 operator-console 中再手写一套容易漂移的 SSH 命令拼装，也要保持 menuconfig 的 display-safe 日志边界。

最新的现场日志还确认了一个关键偏差：vault key import 与 canonical `credential_ref` 绑定都已成功，但测试连接时 `ssh -vvv` 命中了 broker `IdentityAgent` 路径后报 `No such file or directory`，随后 OpenSSH 回退到本地默认 key 并最终 `Permission denied (publickey)`。这说明当前失败点不是“导入未生效”，而是“broker endpoint 元数据已准备但实际 socket 端点未就绪”。如果 UI 只显示泛化失败，操作员会被误导去反复排查 key/用户名，而不是优先修复 broker 生命周期。

## 目标 / 非目标

**目标：**

- 为 plain SSH 与 sealed SSH 提供正式的 `Test Connection --->` 流程，并保持与现有 `menuconfig` 单栏导航和 popup 语法一致。
- 让测试连接基于当前草稿值执行，而不是强迫操作员先 `Apply Target` 或 `Save`。
- 对 sealed SSH 维持正式的 unlock 门控：只有 vault `unlocked` 时才允许测试 sensitive overlay。
- 用真实 SSH 连接探测判断成功或失败，而不是只做字段完整性校验。
- 把详细测试过程写入 display-safe 的 menuconfig 日志，同时让界面只显示简洁的成功/失败/取消反馈。
- 尽量复用现有 unlock worker / popup 模式与既有的 SSH invocation / secret delivery 逻辑，降低未来语义漂移。

**非目标：**

- 不在本次设计中为 ADB 或其他 target kind 增加测试连接入口。
- 不在界面中展示实时 SSH stderr、逐行调试输出或 secret material。
- 不把测试连接与 `Apply Target`、`Save` 绑定成一个隐式提交动作。
- 不绕过 vault 或 secret delivery 约束去“伪造成功”的 SSH 测试。
- 不在本次设计里重做整个 target 执行 runtime；本次只抽取 menuconfig 需要的最小共享能力。

## 决策

### 决策 1：`Test Connection --->` 作为 SSH 专属 action row，而不是持久字段

入口位置固定为：

- plain SSH：`Target Editor -> Connection Profile -> Test Connection --->`
- sealed SSH：`Target Editor -> Sensitive Overlay -> Test Connection --->`

对于 vault `locked` 的 sealed target，`Target Editor` 与 `Sensitive Overlay` 继续沿用既有裁剪行为，只显示通用锁定提示与 `Unlock Vault --->`，不暴露测试入口。

之所以把测试连接放在 SSH 配置分段里，而不是顶层 `Target Editor` 或持久化字段，是因为它本质上是一个即时动作，依赖当前 SSH 连接草稿，而不是 target 的长期配置值。这样也能让 plain / sealed 两条路径自然继承现有页面边界，而无需额外发明第三套状态门控。

### 决策 2：测试连接始终基于当前草稿快照执行

`Test Connection` 必须读取当前 `menuconfig` 内存中的 draft target，而不是磁盘上一次保存的基线值。也就是说：

- 在创建会话里，未 `Create Target` 的新 SSH target 也可以测试。
- 在管理会话里，已修改但未 `Apply Target` 的 host / port / username / `credential_ref` 也可以立即测试。
- 测试动作本身不得把当前草稿视为已提交。

这样做能保证测试行为和操作员眼前正在编辑的值一致，避免“页面上看见的是新值，测试跑的却是旧配置”的错觉。

### 决策 3：采用三段式 popup 流程，并复用现有 worker 模式

测试连接采用与现有 unlock flow 相同的 modal / worker 模式，但状态机独立：

1. `Timeout` 输入弹窗：默认填入 `2000` 毫秒，`Enter` 开始测试，`Esc` 取消。
2. `Waiting` 弹窗：显示“检测中”或等价文案，等待 worker 完成；`Esc` 请求取消。
3. `Result` 弹窗：只显示 `Connection succeeded` / `Connection failed` / `Connection cancelled` / `Connection timed out` 等简洁结果，`Enter` 或 `Esc` 关闭。

之所以不直接复用普通字段编辑流程，是因为测试连接需要把“输入 timeout”“等待执行”“展示结果”组织成一条连续动作链，而不是一个普通字段提交。复用 unlock flow 的总体结构则可以减少新的键位和焦点语义。

### 决策 4：真实探测使用 SSH one-shot `exit 0`，并与现有执行路径对齐

测试连接必须执行真实 SSH 探测，而不是只校验字段完整性。探测语义固定为：

- 使用当前草稿 target 解析出的有效 SSH executable、端口、用户名、主机与 secret delivery 参数。
- 执行最小 one-shot 远程命令，例如 `exit 0`。
- 只要 transport 建立、认证成功并在 timeout 内完成该命令，即判定为成功。

实现上不应在 `bridgingio-operator-console` 中手写一套新的 SSH 命令拼装。推荐路径是把当前位于 `bridgingio-mcp` 的“target -> resolved SSH invocation + secret delivery”子能力抽成更低层、可复用的共享 helper，然后让 menuconfig 和既有 MCP/terminal 执行路径共同调用。这样可以保证：

- target 级 / 全局 toolchain override 的解析顺序一致；
- imported vault SSH key 的 broker / fallback 语义一致；
- menuconfig 的测试结果与后续真实执行结果不会因为命令拼装漂移而分叉。

### 决策 5：plain target 永远可见测试入口，但不绕过 vault-backed credential 的前置条件

plain SSH 的 UI 规则很简单：`Test Connection --->` 始终可见、始终可触发，不因为 vault 锁状态而在页面级被禁用。

但如果当前 plain 草稿绑定的是 vault-managed `credential_ref`，而该 credential 在当前 lock state 下无法被 materialize，则测试可以在 preflight 或执行初期快速失败，并返回简洁失败结果；系统不得为此隐式触发 `Unlock Vault`，也不得把 plain target 重新解释为 sealed target。

这样既满足“plain 类型允许直接测试”的产品意图，又不会破坏现有 vault/secret delivery 边界。

### 决策 6：详细测试日志写入 `menuconfig-session.jsonl`，而不是新增专用日志文件

本次设计优先复用现有 `logs/menuconfig-session.jsonl`，并为 `Test Connection` 增加带 `flow_id` 的 display-safe session 事件：

- `INFO`：记录用户可审计的结果摘要，例如 started / succeeded / failed / cancelled / timed-out。
- `DEBUG`：记录更详细但仍 display-safe 的 breadcrumb，例如 `target_id`、`target_index`、`storage_class`、`timeout_ms`、`elapsed_ms`、resolved toolchain source/path summary、sanitized error category、exit status。

日志不得包含：

- 私钥明文、passphrase、token 明文；
- 原样 SSH stderr；
- 可能泄露 secret locator 或敏感自由文本的未净化输出。

之所以不在本次直接新增 `menuconfig-ssh-test.jsonl`，是为了控制改动面并优先复用现有 menuconfig breadcrumb 语义；如果后续证明确实需要专用流，再单独演进。

### 决策 7：timeout 使用应用层毫秒语义，取消/超时由 worker 负责兜底

用户配置的是应用层 timeout，单位固定为毫秒，默认值为 `2000`。worker 必须在应用层跟踪该超时，而不能只依赖 SSH 自身的秒级 `ConnectTimeout`：

- timeout 到期后，界面结果必须明确显示为 `timed out`；
- `Esc` 取消后，界面结果必须明确显示为 `cancelled`；
- worker 必须忽略取消后的迟到成功结果；
- 若底层子进程仍在运行，实现必须尝试结束该进程，避免在 UI 已退出等待态后继续悬挂。

这样可以保证 UI 语义与用户输入的 timeout 一致，而不是把底层工具的粗粒度超时当成唯一时钟。

### 决策 8：broker-backed SSH probe 必须显式校验 endpoint 就绪性

对于使用 vault imported SSH key 的测试连接，系统必须把“broker endpoint 是否真正可用”视为正式前置条件，而不是隐式依赖 OpenSSH 的后续报错推断。

- preflight 至少要校验 `IdentityAgent` 参数已解析且 endpoint socket 已存在/可访问；
- 若 broker endpoint 未就绪，必须返回稳定的 broker 专属失败分类（例如 `broker-endpoint-unavailable`），并在结果弹窗给出可定位提示；
- 不得把该路径误归类为普通认证失败，也不得通过 fallback 私钥路径掩盖 broker 不可用问题；
- `menuconfig-session.jsonl` 必须记录 display-safe 的 broker 诊断 breadcrumb（如 endpoint 缺失、preflight 失败分类、flow_id 关联），用于后续定位。

## 风险 / 权衡

- [抽取共享 SSH invocation helper 会同时触及 `bridgingio-mcp`、`bridgingio-connectors` 与 `bridgingio-operator-console`] → 通过把共享边界收窄到“draft target -> resolved invocation/probe result”来减小重构面，避免把整套 runtime 都拖进 menuconfig。
- [取消/超时需要跨平台管理子进程生命周期] → 通过保持 worker 只负责 one-shot 探测、为 timeout/cancel 加自动化覆盖，并优先复用已有本地 shell/runtime 能力来降低平台差异风险。
- [plain target 可见但因 vault-backed credential 快速失败，可能让用户觉得‘不是说可以直接测吗’] → 在结果文案中明确说明失败是因为当前 credential 无法在 locked vault 下使用，并把详细原因写入日志。
- [日志想要足够详细，又容易越界写入敏感输出] → 采用白名单字段和错误分类，不记录 raw stderr，只记录 sanitized summary。

## 迁移计划

1. 先补齐本 change 的 proposal / design / specs / tasks，并同步需要更新的矩阵类文档合同。
2. 抽取或新增共享的 SSH probe helper，确保 menuconfig 可基于 draft target 获得与正式执行一致的 invocation / secret delivery 结果。
3. 在 `bridgingio-operator-console` 中加入 `Test Connection` action、timeout/wait/result popup 与 worker 状态机。
4. 为 `menuconfig-session.jsonl` 增加 flow-scoped 的 SSH test breadcrumb / result 记录，并确保保持 display-safe。
5. 补齐回归测试与矩阵文档，覆盖成功、失败、超时、取消、sealed locked/unlocked、plain + vault-backed credential preflight 失败等路径。

## 开放问题

- 暂无阻塞性开放问题；若实现阶段发现现有 session 事件结构无法优雅承载 `timeout_ms` / `elapsed_ms` / `exit_status` 等字段，再在实现 change 中细化事件 schema。
