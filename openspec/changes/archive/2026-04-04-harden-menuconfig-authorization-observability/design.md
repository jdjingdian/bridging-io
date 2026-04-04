## 上下文

当前仓库已经把 `menuconfig` 的 vault 解锁从“启动即触发”收敛为显式触发，也为 `Waiting/Success/Failed` 提供了基础状态机。但这条链路在可观测性上仍然很弱：

- `bridgingio-operator-console` 主要依赖 `last_status` 和局部 worker 状态表达结果，没有正式的授权事件模型，也没有落盘的 session 诊断。
- `bridgingio-platform` 已经定义了 `RuntimeLogLevel` / `RuntimeLogCategory`，但默认 logger 仍然是 stderr-only；`menuconfig` 本身没有接入这套能力。
- standalone CLI 已经有一套 ad-hoc `emit_management_audit(...)` JSON 输出，但它与 `menuconfig` 没有统一 schema，也没有统一的 flow / action 分类。
- `bridgingio-secrets` 的 `os_native_protector_kek_verified()` 只有“成功后缓存”，没有“调用中的合并等待”；因此如果多个调用链几乎同时落到 verified `os-native` 路径，理论上仍可能重复触发平台认证。

这次变更因此不是单独修一个 UI bug，而是要把三层真相收束到一起：

1. operator surface 如何声明“这是一次正式授权动作”；
2. runtime 如何记录可归因的 display-safe 授权 / session 日志；
3. secrets 层如何保证同一进程内的 verified `os-native` 授权不会被并发调用放大成多次系统认证。

本设计覆盖四个模块：

- `bridgingio-operator-console`：`menuconfig` 的 action 归类、session breadcrumb、落盘调用点
- `bridgingio-secrets`：verified `os-native` singleflight / dedupe 边界
- `bridgingio-platform`：共享日志级别语义与 append-only JSONL event sink
- `bridgingio-mcp`：standalone 管理命令与已有 audit 输出向统一 schema 对齐

同时必须保持三个既有边界：

1. 所有新增日志仍然必须 display-safe，禁止把 passphrase、私钥、token 明文、ciphertext locator 或等价敏感材料写入 UI 文本、stderr、JSONL 或 matrix 文档。
2. 这次变更不把仓库切换到通用 telemetry / tracing 平台；目标是本地受信任授权与 `menuconfig` 诊断，而不是全局埋点系统。
3. singleflight 只要求覆盖同一进程内的重复触发；不把多进程互斥或全局 OS 级锁带入本轮范围。

## 目标 / 非目标

**目标：**

- 为本地受信任授权动作定义稳定的分类与 flow 语义，使 `menuconfig`、standalone CLI 与底层授权链路能围绕同一套事件模型记录日志。
- 将关键授权日志以 append-only JSONL 持久化到 runtime root `logs/` 下，而不是仅保留 stderr 或 TUI 内存状态。
- 将 `INFO` 级别用于关键授权事件，将 `DEBUG` 级别用于关键调用链 breadcrumb，并让两者都具备稳定 flow id 以便排查偶发重复弹窗。
- 在 `bridgingio-secrets` 中为 verified `os-native` 解锁建立 singleflight / dedupe 约束，确保同一进程内的并发调用不会重复访问平台验证路径。
- 为 `menuconfig` 增加正式的 session 诊断落盘，覆盖会话开始/结束、关键 screen/action、unlock worker 生命周期、保存/取消/失败等高价值节点。
- 为新日志与 singleflight 行为补齐自动化回归验证，并同步更新 operator interface matrix。

**非目标：**

- 不引入通用远程上报、metrics 后端、集中式日志平台或全仓库统一 `tracing` 改造。
- 不解决多进程同时启动多个 BridgingIO 实例时的跨进程钥匙串提示合并；本轮只约束单进程内的授权放大问题。
- 不改变现有 vault / token / SSH key 的安全策略本身，也不改变哪些动作需要 local verification。
- 不把 `menuconfig` 的所有普通交互都持久化为高粒度审计；重点仅在授权动作与高价值会话诊断。

## 决策

### 决策 1：建立统一的本地受信任授权动作模型

这次变更引入一组稳定的 operator-facing 授权动作分类，至少覆盖：

- `vault.unlock`
- `vault.delete`
- `ssh_key.import`
- `ssh_key.delete`
- `auth.token.create`
- `auth.token.delete`

每一次正式授权动作都必须在其 surface 入口处生成一个稳定的 `flow_id`，并在后续事件中携带：

- `surface`：`menuconfig` / `standalone-cli`
- `screen` 或等价入口位置
- `action`：UI action / CLI command
- `operation`
- `phase`：`requested` / `started` / `joined` / `succeeded` / `failed` / `cancelled`
- `result` / `error_code`
- `dedupe_state`：`leader` / `joined` / `not-applicable`

这样设计，而不是继续让 `menuconfig last_status`、standalone CLI audit JSON 和 secrets 内部错误各自为政，是为了让一次显式授权在不同模块中都能被同一个 flow 串起来。

备选方案：

- 继续保留 surface 各自的状态字符串和零散 audit 输出  
  拒绝原因：无法稳定归因“究竟哪条链触发了第二次授权窗口”。
- 直接引入通用 tracing / span 系统  
  拒绝原因：这会把变更范围扩大到全仓库 observability 重构，超出本轮问题边界。

### 决策 2：日志持久化采用两个 append-only JSONL 流，而不是单一 stderr 或大型通用日志文件

本设计将 runtime root `logs/` 下新增两类本地日志流：

- `local-authorization.jsonl`
  - 记录授权动作的关键事件
  - 默认持久化 `INFO` 级别事件
  - 所有事件都必须 display-safe
- `menuconfig-session.jsonl`
  - 记录 `menuconfig` 的高价值 session 诊断
  - `INFO` 持久化会话开始/结束、save、unlock 结果、关键失败
  - `DEBUG` 持久化 screen/action/worker breadcrumb

两类日志都采用 append-only JSONL，保持“可以 tail / grep / 按 flow 过滤”的排障属性，而不是试图在本轮引入复杂的结构化存储。

这里还明确一个重要边界：

- `INFO` / `DEBUG` 是事件级别语义，不等价于“是否允许持久化”
- 关键授权事件必须被持久化，即使 surface 不直接把它们镜像到 stderr
- `DEBUG` breadcrumb 受 `core.log_level >= debug` 门控，避免默认 info 模式下写入过量细节

备选方案：

- 只写 stderr  
  拒绝原因：`menuconfig` 过程中的关键线索无法稳定留存，线程间时序也难以还原。
- 只写一个大而全的 runtime log 文件  
  拒绝原因：授权事件与 session breadcrumb 混在一起后，排查重复弹窗时噪声太大。
- 只为 `menuconfig` 写日志，不统一 standalone CLI  
  拒绝原因：会继续保留“相似动作不同 schema”的问题，后续难以共用 matrix 和测试。

### 决策 3：复用现有日志级别 vocabulary，但新增一个轻量共享 JSONL sink

`bridgingio-platform` 已经拥有 `RuntimeLogLevel` / `RuntimeLogCategory` 这些稳定 vocabulary，因此本设计不另造一套 level 枚举。新增能力应当是一个轻量的 append-only JSONL sink / recorder，供：

- `bridgingio-operator-console`
- `bridgingio-mcp` 的 standalone 管理命令
- 需要落底层 breadcrumb 的 `bridgingio-secrets`

共同调用。

这样设计，而不是强行要求 `menuconfig` 在 TUI 启动时构造完整 `HostPlatformAdapter`，是为了把变更控制在“共享 recorder + 共享 schema”层，而不是把 operator console 深度耦合进 runtime host adapter 生命周期。

### 决策 4：singleflight 必须落在 `bridgingio-secrets` 的 verified `os-native` 路径，而不是只做 UI 层防抖

当前 `menuconfig` 已经通过 `unlock_worker` 避免同一页面重复触发，但这只能挡住“同一 surface 的重复点击”。真正会访问平台 keyring 的路径在 `bridgingio-secrets::os_native_protector_kek_verified()`，因此 singleflight 必须放在这里：

- 当同一进程内已有一个 verified `os-native` 请求在执行时，后续调用必须加入同一次 in-flight flow，而不是再次启动平台访问。
- `leader` 负责执行真正的 keyring / 平台验证调用。
- `joined` 调用方等待同一个结果，并记录 `dedupe_state=joined`。
- 只有成功结果可以进入 verified KEK cache。
- 失败 / 超时 / 取消结果不得作为“成功缓存”留下。

这样设计，而不是只在 `menuconfig` 层做布尔锁，是为了覆盖 future UI、standalone 管理命令或其他同进程调用链对同一个 verified 路径的竞争。

备选方案：

- 只在 `menuconfig` 中加更强的 worker gate  
  拒绝原因：不能约束其他 surface 或 future caller。
- 做跨进程锁  
  拒绝原因：复杂度高，而且本轮问题主要来自同一进程内的重复触发与缺少归因。

### 决策 5：取消语义保持“取消等待者”，而不是尝试取消平台验证本身

`menuconfig` 当前的 `Esc` 语义是取消等待态，而不是强制关闭系统弹窗。本设计保留这一点，并把它扩展到 singleflight 模型：

- 取消仅表示当前等待者不再继续等待结果
- 若该等待者是 `joined`，不得因为它退出等待而重启新的 provider 调用
- 若平台验证已由 `leader` 发起，系统允许其在后台完成；取消者只记录 `cancelled` 事件并忽略后续结果

这样做，是因为平台级钥匙串 / 本地验证弹窗通常不支持由应用方可靠终止；把“取消等待 UI”误当成“取消底层验证”只会引入更多状态竞争。

### 决策 6：`menuconfig` session 日志只记录高价值 breadcrumb，不记录自由文本或敏感字段值

`menuconfig-session.jsonl` 不记录普通输入弹窗的自由文本值，也不记录 passphrase、私钥内容、token 明文或类似载荷。允许进入日志的内容仅限：

- `screen`
- `action`
- `field_path`
- `flow_id`
- `result`
- `error_code`
- `target_index` / `token_id` / canonical `credential_ref` 等稳定 display-safe 标识

换句话说，日志记录的是“走了哪条路径、在哪一步失败或去重”，而不是“用户输入了什么秘密”。

### 决策 7：standalone CLI 的现有管理审计要向同一 schema 对齐，而不是继续做特例

`bridgingio-core vault ...` / `auth ...` 目前已经通过 `emit_management_audit(...)` 输出 JSON，但字段集合和 `menuconfig` 并不统一。本设计要求：

- standalone CLI 继续可以向 stderr 镜像 display-safe audit
- 同时必须写入与 `menuconfig` 同类的 `local-authorization.jsonl`
- 动作名、flow 关联与 display-safe 字段保持一致

这样 matrix 才能对 `menuconfig` 与 CLI 使用同一套本地授权合同描述，而不是分别维护两种近似概念。

## 风险 / 权衡

- [风险] runtime `logs/` 中新增 JSONL 流会带来文件增长问题  
  → 缓解措施：本轮只定义 append-only 合同与 display-safe 内容，rotation / retention 作为后续独立议题；实现上可保持单文件追加且不阻塞主流程。

- [风险] `DEBUG` breadcrumb 若设计不严谨，仍可能泄露过多操作上下文  
  → 缓解措施：schema 只允许 screen/action/field_path/稳定 display-safe 标识进入日志，禁止自由文本和 secret material。

- [风险] singleflight 只解决同进程重复触发，不能解决多进程竞争  
  → 缓解措施：在 spec 中明确本轮边界；若后续仍出现多进程重叠，需要单独 change 处理。

- [风险] 为 `menuconfig` 和 standalone CLI 引入共享 recorder 会增加少量跨模块耦合  
  → 缓解措施：共享点只保留在 level/category vocabulary 与 JSONL sink，不把 TUI 生命周期绑定到完整 runtime host adapter。

- [风险] 日志写入失败可能干扰授权主流程  
  → 缓解措施：日志 sink 必须 best-effort；写入失败最多回退到 stderr / display-safe 警告，不得导致 vault unlock、token delete 或 SSH key import 失败。

## Migration Plan

1. 在共享层补齐本地 operator 事件 schema 与 JSONL sink，固定 `flow_id`、`operation`、`phase`、`dedupe_state` 等字段。
2. 让 `menuconfig` 在授权动作入口与关键 session 节点调用 recorder，并将事件写入 runtime root `logs/`。
3. 将 standalone CLI 的现有管理审计迁移到同一 schema，并保持必要的 stderr 兼容镜像。
4. 在 `bridgingio-secrets` 的 verified `os-native` 路径加入 singleflight / joined-waiter 语义。
5. 补充自动化测试与 operator interface matrix，覆盖 display-safe 日志、`INFO/DEBUG` 语义和单次 provider 调用约束。

本次无需配置迁移；新增日志文件为 additive 行为，旧实例可直接开始写入。

## Open Questions

- `local-authorization.jsonl` 与 `menuconfig-session.jsonl` 的 rotation / retention 是在实现时做最小阈值切分，还是留到后续 change 单独定义？
- `flow_id` 是否需要在 `menuconfig` 失败提示里以短码形式展示，还是先只保留在日志中用于排障？
- standalone CLI 是否继续保留当前 stderr JSON 格式完全兼容，还是在本轮直接切换到新 schema 后再由文档说明更新？
