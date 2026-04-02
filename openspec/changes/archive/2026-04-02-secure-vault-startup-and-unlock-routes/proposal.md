## 为什么

BridgingIO 的安全启动本质上是在正确的时机、通过受控载体，把 vault unlock material 交给 core 并立即丢弃。但当前仓库中的 secret 输入路线还处在过渡阶段：它们不是完全跨平台，也没有收束成一套最小安全路径。随着 standalone 将成为短期主形态，如果不先把启动解锁和后台解锁约束定住，就会不断引入新的泄露面。

现在需要定义一套尽可能通用、但载体最少的 secure startup / unlock contract，并把 standalone 前台与后台模式都纳入同一设计。尤其是后台 detached 场景，启动后不方便再次交互输入 credentials，因此必须忽略配置中的常规 `trigger_policy`，强制按 `on-core-start` 处理，以减少运行期额外交互和旁路泄露路径。

## 变更内容

- 建立统一的 secure vault startup / unlock contract，覆盖 standalone 前台、standalone 后台/daemon 和 future trusted host 路径。
- 收敛可接受的 unlock material 载体集合，优先使用隐藏终端输入、一次性本地进程载体或受信任本地验证触发，不鼓励继续增加新的明文传递方式。
- 明确 standalone 后台/脱离式模式属于 standalone 子模式，但其启动后不再允许依赖常规交互式 unlock；该模式必须覆盖配置中的 `trigger_policy`，默认按 `on-core-start` 处理。
- 规定 fail-closed 语义：当 required unlock material 未能通过允许的安全载体送达时，core 必须保持 locked / unavailable，而不是延迟到不受控时机再尝试解锁。
- **BREAKING**: standalone detached 模式的 unlock 策略将不再完全遵从配置文件中的常规 `trigger_policy`，而是采用受控 override 规则。

## 功能 (Capabilities)

### 新增功能

### 修改功能
- `credential-and-approval-control`: vault unlock material 载体、允许的启动解锁路径、fail-closed 语义和 detached 模式 override 规则发生规范级变更。
- `target-session-management`: standalone `run` / `-d` 的启动、locked/ready 状态投影、恢复语义与 unlock 触发时机发生规范级变更。

## 影响

- 受影响代码主要包括 `source/rust/bridgingio-mcp` 的启动与管理 CLI、`source/rust/bridgingio-secrets` 的 unlock policy/runtime、`source/rust/bridgingio-platform` 的本地输入/宿主能力边界，以及相关 operator 文档与安全测试。
- 需要同步更新 standalone 手册、vault operator guide、self-test 和 contract automation。
