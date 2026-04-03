## 上下文

当前 `menuconfig` 的 vault 路径把“显示安全状态”和“尝试接触平台 protector”耦合在一起。`MenuConfigApp::load()` 会在进入 TUI 前构造 `SecretVaultRouter` 并立即生成 `security_summary`；而 `SecretVaultRouter::default()` 当前又会执行一次 `os-native` bootstrap wrap/unlock。对 macOS 来说，这意味着操作员只是进入 `menuconfig`，就可能先看到系统钥匙串提示，而不是先看到 `locked` 状态。

同时，`menuconfig` 现有的显式解锁反馈只有底部状态栏文案，缺少独立的“等待解锁”“解锁成功确认”“失败/取消回到 locked”的状态机。对于带系统弹框的 `os-native` 路径，这种反馈过于弱化，操作员难以判断当前究竟是还在等待系统验证，还是已经完成解锁并刷新了 menuconfig 状态。

在落地中还出现了几个 follow-up 缺陷，需要并入本次设计约束：
- 状态一致性缺陷：系统密码尚未完成时提前显示成功，或已完成验证后 UI 仍卡在等待态；
- 触发次数缺陷：一次 `u` 触发了多次系统密码弹窗，来源是初始化阶段与隐式 fallback 路径的重复触发；
- 可操作性缺陷：等待态无法取消，操作员只能被动卡住；
- 可感知性缺陷：Security 菜单中的 lock state 与解锁条目缺少强视觉语义。

这次变更同时涉及：

- `bridgingio-operator-console` 的 TUI 生命周期与 modal 交互
- `bridgingio-secrets` 的被动状态投影与主动解锁边界
- `menuconfig` 文案与测试矩阵

因此需要独立设计文档来约束交互边界和模块分工。

## 目标 / 非目标

**目标：**

- 保证进入 `bridgingio-core menuconfig` 不会主动触发 macOS 钥匙串或其他本地解锁交互
- 将 display-safe vault 状态投影与正式 unlock 流程完全分离
- 为 `menuconfig` 增加显式 unlock 状态机，覆盖等待态、成功确认态和失败反馈
- 让 `u` 快捷键和 Security 页面解锁入口复用同一条显式 unlock flow
- 为 macOS `os-native` 路径补齐回归验证，防止再次回到“启动即弹框”

**非目标：**

- 不重做 `menuconfig` 的整体信息架构、底部按钮栏或搜索交互
- 不改变 vault policy 本身的 trigger policy 语义
- 不把 `menuconfig` 改造成桌面 UI 那套 `Intent + Attestation` 工作流
- 不在本次变更中重构所有 vault backend 或更换 canonical vault 数据模型

## 决策

### 决策 1：把“被动状态投影”与“主动解锁动作”拆成两条路径

`menuconfig` 启动、页面切换、Security/Vault 摘要刷新都只能走被动状态投影路径。这条路径只允许读取已持久化的 lock state、策略摘要、secret/token summary 与受控 diagnostics，不允许：

- 调用正式 unlock handler
- 消费 unlock material
- 访问会触发用户验证的 platform keyring 接口
- 让 router 状态从 `locked` 推进到 `unlocking/unlocked`

备选方案：

- 继续复用当前 router 初始化逻辑，再通过 UI 层“忽略”弹框结果
  拒绝原因：副作用已经发生，无法满足“进入 menuconfig 不弹框”的目标。
- 通过环境变量或平台开关临时禁用 keyring
  拒绝原因：这只是绕过症状，没有建立正式的 display-safe 与 unlock 边界。

### 决策 2：将 `os-native` protector 探测延后到显式 unlock 时刻

对于 macOS `os-native` 路径，只有在操作员显式触发 unlock action 时，系统才允许接触真正可能触发钥匙串 UI 的平台接口。进入 `menuconfig` 时如果无法在不触发用户验证的前提下确定 protector readiness，系统必须返回受控的被动摘要或 deferred/unknown 诊断，而不是为了追求更精确的 readiness 在启动阶段抢先探测。

备选方案：

- 在进入 Security 页面时预探测一次 `os-native` readiness
  拒绝原因：仍然会把“看状态”变成“触发解锁副作用”。
- 完全不展示任何 readiness，只显示 locked/unlocked
  拒绝原因：会丢失现有 display-safe 状态页的诊断价值。

### 决策 3：为 `menuconfig` 引入显式 unlock modal 状态机

`menuconfig` 需要新增独立于底部状态栏的 unlock 交互状态机，至少覆盖：

- `Idle`: 默认浏览态
- `WaitingForUnlock`: 已显式发起解锁，正在等待本地验证或系统钥匙串完成
- `UnlockSucceeded`: 解锁成功，等待操作员确认返回
- `UnlockFailed`: 解锁失败或取消，给出明确结果并返回 locked

`u` 快捷键和 Security 页面中的解锁入口都进入同一条状态机，避免两个入口出现不一致结果。成功后 Security 页面不再继续强调解锁 action，而是切换成已解锁提示，例如“加密项管理已解锁”。

备选方案：

- 保持现有底部状态栏提示，不增加 modal
  拒绝原因：对于系统弹框型流程，底部一行文本不足以表达等待态和成功确认态。
- 解锁成功后自动关闭提示，不要求确认
  拒绝原因：用户需求明确希望看到成功确认，并在确认后退出提示。

### 决策 4：结果反馈按方法适配，但保持统一的用户语义

不同 unlock method 可以有不同的底层采集方式：

- `os-native`: 先进入 `WaitingForUnlock`，再由系统弹出正式本地验证界面
- `passphrase`: 继续通过受控输入方式采集口令，再进入统一的成功/失败结果态

但对操作员而言，`menuconfig` 暴露的语义保持一致：显式发起、等待完成、确认结果、刷新为已解锁。

### 决策 5：把回归重点放在“时机”而不只是“入口存在”

这次最容易回归的问题不是按钮缺失，而是时机边界被重新打破。因此测试必须验证：

- 启动 `menuconfig` 不触发解锁
- 进入 Security/Vault 页面不触发解锁
- 只有显式 action 才进入等待态
- 成功后 UI 真的切成已解锁
- 失败/取消后仍保持 locked

### 决策 6：`menuconfig` 显式解锁入口只执行单一 `os-native` 本地验证路径

`menuconfig` 的 `u` 和 Security 解锁入口只允许触发 `os-native` verified 流程，不在同一交互中自动回退到 passphrase。这样可以避免 TUI 内隐藏的口令读取阻塞与多路径状态竞争，让“按下 u -> 系统验证 -> 成功/失败”保持可预测的一条链路。

同时，`SecretVaultRouter` 默认初始化必须避免隐式 keyring 访问，防止在一次解锁会话里出现多次系统弹窗。

### 决策 7：等待态支持显式取消，取消语义限定为“取消 menuconfig 等待”

解锁执行放到后台 worker，TUI 主循环持续可响应按键。操作员在 `WaitingForUnlock` 状态按 `Esc` 时，系统立即退出等待弹窗并标记取消请求。若系统弹窗仍在，操作员可手动关闭；当 worker 返回后系统必须确保 vault 状态保持或回到 locked，并给出“已取消等待”的明确反馈。

### 决策 8：Security 菜单中的锁状态与解锁条目采用强语义视觉规范

- `--- Vault 锁状态 = <value>` 中的 `<value>` 必须颜色高亮：`locked` 红色，`unlocked` 绿色（其他状态黄色兜底）；
- 未解锁时的动作条目使用导航语义：`解锁 Vault --->`；
- 已解锁时使用状态提示语义：`-*- 加密项管理已解锁`；
- Security 菜单与 footer 必须共享同一 lock-state 颜色语义，避免同一状态在不同页面出现冲突表达。

## 风险 / 权衡

- [风险] 被动状态投影在未显式 unlock 前可能拿不到最精确的 protector readiness
  → 缓解措施：允许返回受控的 deferred/unknown 摘要，但禁止为了精确度抢先触发用户验证。
- [风险] `menuconfig` 事件循环引入 modal 状态机会增加状态分支
  → 缓解措施：用单一 unlock flow 状态枚举统一处理 `u` 快捷键和 Security action。
- [风险] 现有 `passphrase` 路径与 `os-native` 路径的交互差异可能导致体验不一致
  → 缓解措施：`menuconfig` 入口固定单一路径（`os-native` verified），其他方法由非 menuconfig 通道处理。
- [风险] 调整 router 初始化边界可能影响已有 vault self-test 或摘要读取逻辑
  → 缓解措施：把“被动摘要 API”与“主动 unlock API”明确分层，并补针对 locked 状态的回归测试。
- [风险] `Esc` 取消无法强制关闭系统级验证弹窗，可能造成“已取消等待但系统弹窗仍在”
  → 缓解措施：明确取消语义是“取消 menuconfig 等待态”；文案提示需要手动关闭系统弹窗，worker 回收后强制恢复一致状态。

## Migration Plan

1. 在 `bridgingio-secrets` 中引入或整理被动 vault 状态投影路径，保证初始化和摘要读取不触发平台解锁副作用。
2. 在 `bridgingio-operator-console` 中重构 Security/Vault 解锁入口，加入 unlock modal 状态机和文案刷新逻辑。
3. 更新 `menuconfig` i18n 文案，补齐等待态、成功确认态、失败反馈和已解锁提示。
4. 增加 `menuconfig` 单元/集成测试，重点覆盖 macOS `os-native` 路径的触发时机、单次触发、取消路径和状态切换。

本次无需配置迁移；行为迁移主要体现在 `menuconfig` 的解锁时机和反馈方式。

## Open Questions

- 被动摘要中的 protector readiness 在未显式 unlock 前应显示为明确的 `deferred`/`unknown`，还是仅保留 lock state + allowed methods 即可？
