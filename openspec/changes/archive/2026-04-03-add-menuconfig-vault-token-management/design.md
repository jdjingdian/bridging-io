## 上下文

当前 vault 与 token 管理同时存在“状态表达不完整”和“管理动作入口过于隐式”两个问题。

- `menuconfig` 启动时使用被动投影展示 Security 摘要，但缺失 vault metadata 时默认表现为 `locked`，没有把“未初始化”作为正式 operator 状态暴露。
- `menuconfig` 在缺失持久化 vault 目录时仍可退回 `SecretVaultRouter::default()`，从而让 unlock 流看起来像是对真实 vault 生效，实际上却绕开了显式 init 语义。
- token 当前只有 `active / revoked / expired`，没有正式 `delete` 终态，也没有“expired 仍需 revoke 才能 delete”的合同。
- `menuconfig` 当前只有 token 数量摘要，没有单独的 Token Management 子页，也没有备注编辑、revoke/delete 的确认链路。
- expiring token 当前直接基于 `SystemTime::now()` 与 `expires_at` 做比较，没有“时钟回拨后不得重新激活已过期 token”的正式约束，也没有对“本机时间未同步”场景的 operator 提示。

这次设计需要同时覆盖 `bridgingio-secrets`、`bridgingio-core` 管理命令、`bridgingio-operator-console` 的 Security/TUI 入口，以及 operator interface matrix 的合同表达，因此属于跨模块的状态机与管理面设计变更。

## 目标 / 非目标

**目标：**

- 将 vault 缺失状态正式建模为 `uninitialized`，并要求操作员通过显式 `init` 进入可解锁状态。
- 让 Security 页按 `uninitialized / locked / unlocked` 三种核心状态裁剪动作，而不是继续长期显示同一组入口。
- 为 token 引入正式的 delete 终态，并明确 `revoked` 是唯一 delete 前置条件；`expired` token 也必须先 revoke 再 delete。
- 提供独立的 Token Management 子页，用于编辑 token 备注、执行 revoke/delete，并在高风险动作前要求二次确认。
- 保持 token 明文默认不可见，但允许 `Create Token` 在受控的一次性结果弹窗中临时承接本次签发结果。
- 让 `Create Token` 具备清晰、最少出错的引导式创建体验，并把“有效期依赖本机时钟”的限制前移到交互中。
- 统一 Security / Token Management 中“流程入口动作”的视觉语义，避免把会进入子流程或确认链路的动作渲染成即时执行样式。
- 对 expiring token 增加最小可行的反回拨保护，使已过期 token 不会因为系统时间后退而恢复为可用。

**非目标：**

- 不在本次变更中扩展桌面 GUI 的信息架构；桌面设置页是否同步补充 token delete 入口可作为后续复用 runtime 语义的独立工作。
- 不在本次变更中设计 secret reveal/export、passphrase 输入 UI 或更广泛的 vault 深度管理页面。
- 不把 `vault delete` 扩展为“清除全局 os-native keyring 条目”的全局危险操作。

## 决策

### 决策 1：缺失 vault metadata 时必须进入 `uninitialized`，并移除 menuconfig 的隐藏 fallback unlock

Security 页的核心误导来自当前“缺失 metadata 仍按 locked 渲染”和“缺失持久化 vault 时仍退回默认内存 router”。本次变更要求：

- 缺失 `metadata.db` 或等价持久化真相时，vault 必须表现为 `uninitialized`。
- `menuconfig` 在 `uninitialized` 状态下不得提供 `Unlock Vault`，只能提供 `Init Vault`。
- `menuconfig` 不再对缺失持久化 vault 使用 `SecretVaultRouter::default()` 作为解锁 fallback。

选择该方案而不是继续沿用 `locked` 兼容语义，是因为测试与运维首先需要的是“状态真实”，而不是“尽量还能点下去”。继续保留 fallback 会让 operator 无法判断当前操作的是正式 vault、临时内存 vault，还是隐式 bootstrap 结果。

### 决策 2：Security 页只暴露与当前 vault 状态匹配的动作

Security 页采用严格状态裁剪：

- `uninitialized`：显示 `Vault 未初始化` 与 `Init Vault --->`
- `locked`：显示 `Vault Locked` 与 `Unlock Vault --->`
- `unlocked`：显示 `Vault Unlocked`，并展示 `Create Token --->` 与 `Token Management --->`

这样做而不是长期同时展示 `Init / Unlock / Token Management`，是为了让 TUI 的信息架构直接体现运行态真相，避免用户在错误状态下触发无意义动作，再由错误提示兜底。

### 决策 3：`vault delete` 是当前 runtime store 的受控重置，不触碰全局 os-native keyring

`vault delete` 的主要用途是把当前实例恢复到可重复测试的初始态，因此其默认语义定义为：

- 删除当前 `core.data_dir/vault` 下的持久化 vault 数据。
- 删除成功后 vault 状态立即回到 `uninitialized`。
- 该操作必须经过二次确认。
- 该操作不得顺手删除全局 `os-native` protector keyring 条目。

不删除全局 keyring 的原因是当前 os-native protector 使用固定的全局服务名/账户名，天然不与 runtime root 绑定。把它纳入普通 `vault delete` 会把“删除当前实例 vault”升级为“影响其他实例的共享本地 protector”，风险与用户预期都不匹配。

### 决策 4：token 的“备注”直接复用现有 `label` 字段

用户提出的“备注（如 Codex / Claude Code）”与现有 token `label` 语义高度重叠。本次设计不新增平行的 `note` 或 `display_name` 字段，而是：

- 继续把 token 的 operator-facing 备注建模为 `label`
- 允许在 Token Management 子页中编辑该字段
- 创建时与后续编辑时都沿用同一 display-safe 字段

选择复用 `label` 而不是新增第二个备注字段，是为了避免两个近义元数据长期漂移，同时减少运行时和接口矩阵的额外复杂度。

### 决策 5：token delete 使用“终态 tombstone”而不是物理硬删

token lifecycle 收敛为：

- `active`
- `expired`
- `revoked`
- `deleted`

其中：

- `expired` 由 TTL 自动得出，但不是 delete 前置条件
- `revoked` 是唯一允许进入 `deleted` 的前置条件
- `deleted` token 不再出现在默认管理列表中，但仍保留最小化审计 tombstone

选择 tombstone 而不是物理删除，是因为 token revoke/delete 都属于安全管理动作；若直接物理硬删，后续几乎无法解释“这个 token 是否存在过、何时被撤销、何时被删除”。保留终态 tombstone 可以同时满足“管理列表干净”和“安全审计不丢失”。

### 决策 6：`Create Token` 允许一次性 reveal，但只能存在于独立结果弹窗

`menuconfig` 仍然以 display-safe projection 为默认原则，但 `Create Token` 若不提供一次性明文结果，就无法成为可用的 operator 功能。因此本次定义一个受控例外：

- 只有在操作员显式触发 `Create Token` 且创建成功后，系统才可以展示一次性 token 明文结果。
- 该结果必须位于独立弹窗或结果面板中，并带有“关闭后不再显示”的提示。
- Token Management 列表、Security 摘要、重进页面或刷新后都不得再次回显明文 token。

这复用现有桌面控制台的一次性 reveal 设计，而不是把 token 明文重新引入普通 TUI 列表。

### 决策 7：`Create Token` 采用三步式引导，先 label，再有效期，再一次性 reveal

`Create Token` 的目标不是把全部字段丢进一个表单，而是把操作员最关心的三件事按顺序拆开：

1. 先通过弹窗要求输入 token 别名（复用 `label`）。
2. 别名确认后，再选择有效期模式：
   - `长期有效`
   - `有效至指定时间`
3. 创建成功后，单独展示一次性 token 明文结果，并明确提示“关闭后无法再次查看”。

当操作员选择 `有效至指定时间` 时，界面必须同时展示本机当前时间与时区，让用户知道这不是云端权威时间，而是“按本机时钟解释”的失效时间。该流程比把 label、expiry、plaintext 混在同一页更稳妥，也更符合本地受信任控制面的使用心智。

### 决策 8：expiring token 必须具备“时钟后退不复活”的约束，并在时钟异常时显式告警

单机本地 vault 无法凭空创造一个绝对可信的时间源，因此“指定时间之前有效”只能建立在宿主系统时间之上。但这并不意味着系统可以接受简单的时钟回拨重放。本次设计要求：

- token 过期判定必须是单向的：一旦某个 token 已被系统判断为 `expired`，后续即使系统时间回退，也不得重新恢复为 `active`。
- runtime 必须维护某种持久化的时钟健康保护，例如非递减 wall-clock watermark、sticky expiry 记录或等价机制，以防止重启后因系统时间回拨而重新接受已过期 token。
- 当系统检测到当前 wall clock 明显早于最近可信观测值时，必须把该状态暴露为本地 operator 可见的时钟异常诊断，并阻止或至少强警告新的“定时过期 token”创建流程。

这一定义承认了一个现实边界：如果没有外部可信时间源，我们无法把 expiring token 变成“强对抗宿主控制者”的密码学保证；但我们仍然应当阻断最直接的回拨复活路径，并把时间信任边界清晰呈现给 operator。

### 决策 9：流程入口动作统一采用导航样式，编辑弹窗字段名必须 operator-facing

为了让操作员在 TUI 中快速区分“即时动作”和“进入流程/确认链路的动作”，本次约束：

- Security 页面中的 `Create Token`、`Token Management`、`Delete Vault` 统一按 `--->` 样式渲染。
- Token Management 子页中的 `Edit Label`、`Revoke Token`、`Delete Token` 统一按 `--->` 样式渲染。
- `Create Token` 的编辑弹窗必须显示可读字段标题（例如“创建 Token 别名”），不得暴露内部字段键名（例如 `__token_create_label__`）。

该约束的核心目的是防止操作员把高风险动作误判为“单击即生效”或把内部实现细节误解为业务字段，降低本地受信任管理面的认知负担与误操作风险。

## 风险 / 权衡

- [从隐式 init/fallback 迁移到显式 `uninitialized`] → 现有依赖“删目录后还能直接 unlock”的测试脚本会失效；通过新增 `vault init` / `vault delete` 的正式路径和更明确的状态提示来替代。
- [`vault delete` 不删除全局 os-native keyring] → 某些测试人员可能误以为“删除 vault 后一切都被清空”；通过文案与接口矩阵明确其语义仅限当前 runtime store。
- [token delete 改为 tombstone 而非硬删] → 实现复杂度高于简单 `HashMap::remove`；但可以换来更稳定的审计与生命周期语义。
- [`Create Token` 允许一次性 reveal] → 对既有“默认只展示 display-safe projection”的规则引入例外；通过限定为显式创建后的单次结果弹窗，将风险控制在最小范围。
- [expiring token 引入反回拨保护] → 如果宿主时钟曾被错误地大幅拨快，再恢复正常后，某些 token 可能维持过期/不可恢复状态；这是“避免时钟回拨复活”与“容忍本机时间混乱”之间的现实权衡，需要通过诊断文案讲清楚。

## 迁移计划

1. 先调整 runtime 的 vault 状态语义：缺失 metadata 时返回 `uninitialized`，并禁止 menuconfig 通过默认 router 执行 unlock fallback。
2. 增加显式的 `vault delete` 与 `token delete / token label update` 管理语义，并更新 operator interface matrix。
3. 在 `menuconfig` 中重构 Security 页：按 vault 状态裁剪入口，并引入 Token Management 子页与二次确认弹窗。
4. 为 token create 的三步交互、一次性 reveal、时钟异常提示、expired->revoke->delete、vault delete -> uninitialized 路径补充自动化验证。

## 开放问题

- `menuconfig` 文案最终采用 `Init Vault` 还是 `Create Vault` 作为 operator-facing 标签，可以在实现阶段按整体术语风格统一，但语义上都指向同一显式初始化动作。
