## 为什么

当前 `bridgingio-core menuconfig` 的 vault 解锁虽然已经从“启动即触发”收敛为显式触发，但在 macOS 上仍会偶发出现一次操作对应多次系统授权窗口的情况。与此同时，现有实现缺少统一的授权动作入口与持久化可观测性，导致团队很难快速判断究竟是哪个入口、哪条调用链或哪次状态刷新再次触发了本地验证。

现在需要把这类问题从“交互瑕疵”提升为正式合同：把本地授权动作统一分类、为关键授权路径补齐 `INFO/DEBUG` 分级日志、将 `menuconfig` 有价值的会话日志落到 runtime `logs/` 下，并明确 verified `os-native` 路径在一次显式解锁流程中不得重复触发系统认证。

## 变更内容

- 为 `menuconfig` 与相关本地受信任管理路径建立统一的授权动作分类，至少覆盖 `vault.unlock`、`vault.delete`、`ssh_key.import`、`ssh_key.delete`、`auth.token.create` 与 `auth.token.delete` 这类会触发本地验证、确认或高风险授权的动作。
- 要求上述授权动作通过统一的 operator-facing 授权入口记录关键事件，而不是继续由各个 UI action、worker 和底层 router 各自零散触发、各自输出状态文本。
- 为授权链路补齐分级日志合同：
  - `INFO` 记录关键业务事件，例如动作开始、成功、失败、取消、超时、去重命中与最终状态；
  - `DEBUG` 记录关键调用链 breadcrumb，例如 screen/action/worker/router/provider 的触发路径与关联 flow id。
- 为 `menuconfig` 增加正式的 session / authorization 日志落盘要求，将 display-safe 的关键诊断写入 runtime root 下的 `logs/`，而不是仅保留 stderr 输出或内存中的 `last_status`。
- 收紧 `verified os-native` 授权触发语义：一次显式授权流程必须具备单次触发 / singleflight 约束，避免多个近同时调用链叠加触发多次系统认证窗口。
- 明确日志与审计必须保持 display-safe：禁止把 passphrase、私钥、token 明文或其他敏感材料写入 `menuconfig` 状态栏、runtime logs 或 session 日志。
- 增加针对 `menuconfig` 授权观测与单次触发行为的自动化回归验证，覆盖“同一显式 unlock 不会触发重复系统认证”“日志按级别落盘且可关联 flow”“取消/失败后状态仍保持一致”等路径。
- 更新本地 operator interface matrix，使 `menuconfig` 与 standalone 管理命令的授权日志、状态门控、去重与落盘语义有正式记录，而不是只留在实现细节中。

## 功能 (Capabilities)

### 新增功能

- 无

### 修改功能

- `standalone-operator-console`: 为 `menuconfig` 补齐统一授权动作入口、session / authorization 日志落盘与单次显式授权触发约束。
- `credential-and-approval-control`: 明确本地受信任授权动作的分类、display-safe 审计边界，以及 verified `os-native` 授权路径的去重 / singleflight 合同。
- `quality-and-test-automation`: 增加针对 `menuconfig` 授权日志、调用链观测与单次系统认证触发的回归验证要求。
- `operator-interface-matrix`: 补充 `menuconfig` 与 standalone 管理路径的授权动作、日志级别、落盘位置、状态门控与去重语义。

## 影响

- 受影响模块：`source/rust/bridgingio-operator-console`、`source/rust/bridgingio-secrets`、`source/rust/bridgingio-platform`、`source/rust/bridgingio-mcp`。
- 受影响运行时路径：runtime root `logs/`、`menuconfig` unlock / SSH key / token 管理动作、standalone 管理命令的审计输出。
- 受影响验证：需要新增 `menuconfig` 授权 flow 与日志合同测试，并补充 verified `os-native` 去重 / 单次触发相关回归覆盖。
