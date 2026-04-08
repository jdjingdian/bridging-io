## 上下文

当前 `bridgingio-secrets` 的 `os-native` unlock 链路把“如何访问平台 keychain / secure store”和“如何用拿到的 KEK 解包 canonical vault root key”耦合得过紧。现状里至少有三个危险点：

1. `os_native_protector_kek_blocking()` 只要 `get_password()` 不是 `Ok(...)`，就会继续生成新 KEK 并 `set_password(...)`，没有区分“item 不存在”和“用户取消 / ACL 拒绝 / 暂时性访问失败”。
2. verified `os-native` unlock 在拿到某个 `verified_kek` 后，会尝试把 legacy fallback wrap 直接迁移成 verified wrap；该迁移路径当前是 best-effort 写 blob + 更新内存 manifest，而不是显式的 cancel-safe / corruption-safe 持久化事务。
3. `menuconfig` unlock worker 当前把平台取消、普通验证失败和 protector 失配等不同路径压扁成相近的失败结果，导致 UI、日志和恢复动作都无法可靠区分“本次取消”和“当前 vault 真相层已经失配”。

这次设计还受到几个既有约束：

- `menuconfig` 的显式 unlock 仍然只能走 verified `os-native` 路径，不能在同一交互里隐式回退到 passphrase。
- display-safe 约束不变：日志、状态文本和错误对象不得泄漏 keychain item 内容、KEK、wrap blob、locator 或其他高敏内部材料。
- Windows 与 macOS 共享同一 `os-native` 抽象，因此设计必须优先落在共享 runtime 语义上；macOS 只是这次最先暴露问题的平台。
- 当前 vault 持久化格式不宜做大迁移；优先修复运行时行为和持久化不变式，而不是重新设计 metadata schema。

## 目标 / 非目标

**目标：**
- 让 `os-native` unlock 在“用户取消 / ACL 拒绝 / 平台访问失败 / item 缺失”之间拥有严格区分的内部结果语义。
- 保证 verified `os-native` unlock 是 fail-closed 且 cancel-safe：取消或失败不得生成新 KEK、覆盖既有 keychain 项、改写 canonical wrap blob，或把 metadata 伪装成已验证。
- 让 legacy fallback -> verified `os-native` 的迁移只在确认绑定正确且持久化可完整提交时发生。
- 让 `menuconfig` 和授权日志能稳定区分 `cancelled`、`denied`、`verification-failed`、`protector-mismatch` 等 display-safe 结果，并给出可恢复提示。
- 为这一链路补上回归测试，覆盖平台取消后的 repeated unlock、持久化原子性和 metadata 真值一致性。

**非目标：**
- 本次不重做 canonical vault 的整体加密模型，也不替换 `VRK -> DEK -> ciphertext` 的分层设计。
- 本次不把 `menuconfig` 扩展为 passphrase recovery UI，也不在显式 unlock 流程中加入新的本地输入路线。
- 本次不自动修复已经发生真实 keychain / wrap 失配的历史 vault；只要求系统能安全诊断、停止继续破坏，并给出受控恢复提示。
- 本次不引入新的 metadata schema version；若需要附加诊断字段，应优先利用现有错误映射和 display-safe 提示承载。

## 决策

### 决策 1：把 `os-native` keychain 访问改为显式结果模型，而不是 `Option<Vec<u8>>`

共享 runtime 必须把 `os-native` 平台访问的结果至少区分为：

- `Found(kek)`
- `NotFound`
- `Cancelled`
- `Denied`
- `PlatformFailure`

这样才能保证“用户取消 / 平台拒绝”不会再被当成“当前没有 keychain item”。

考虑过的替代方案：
- 继续返回 `Option<Vec<u8>>`，只在调用处通过字符串猜测失败原因。
  拒绝原因：会把平台语义继续压扁，无法建立可靠的 fail-closed 分支。
- 只在 macOS 上做条件分支，Windows 暂不收敛。
  拒绝原因：问题根源在共享抽象；平台分叉会让后续行为再次漂移。

### 决策 2：把 protector provisioning 与 verified unlock 严格分离

已有 vault 的 verified unlock 路径必须视为“只读验证路径”，它可以读取既有 keychain 项，但不得在 unlock 期间新建或覆盖 `io.bridgingio.vault` 对应项。只有以下路径才允许创建或写入 `os-native` protector material：

- vault 初始化 / bootstrap
- 明确的 protector provisioning / repair 管理动作（如果后续提供）

这意味着：
- `NotFound` 在已有 vault 的 verified unlock 中不再等价于“那就生成新的 KEK”；
- 对已有 canonical wrap 而言，`NotFound`、`Cancelled`、`Denied` 和 `PlatformFailure` 都必须 fail-closed。

考虑过的替代方案：
- 保留现有“没有读到就生成”的逻辑，再额外打日志。
  拒绝原因：这正是当前数据破坏的来源，日志不能替代安全边界。
- 在 unlock 期间允许写入，但只有当旧 wrap 仍能被 fallback 解开时才写。
  拒绝原因：一旦把错误类型判错，仍然会把无关 KEK 写进既有 vault 绑定中。

### 决策 3：verified rewrap 必须先证明绑定正确，再执行显式持久化提交

legacy fallback wrap 迁移到 verified `os-native` 仍然保留，但要加两层约束：

1. 只有在 verified 路径已经成功拿到可信 `Found(kek)`，并且当前 wrap 确实能被 legacy fallback 解开为一个有效 root key 时，才允许迁移。
2. 迁移必须以“先生成新 blob，再完整提交，再切换 manifest 视图”的方式进行。若任一步失败，旧 blob、旧 manifest 语义和旧 verification metadata 都必须保持不变。

这里的核心不是“绝对数据库事务”，而是保证系统不会在取消/失败时留下半迁移状态。至少需要做到：
- 失败不覆盖旧 wrap blob；
- `last_verified_at` 只在真实 verified success 后推进；
- `status=Ready` 只在迁移或 verified unwrap 真成功后出现；
- 普通 `persist_metadata_db()` 不得顺手把所有 wrap 都写成“刚验证过”。

考虑过的替代方案：
- 暂时完全禁用 legacy -> verified rewrap。
  拒绝原因：能止血，但会让长期处于 degraded fallback 的 vault 永远不能安全收敛到 verified 状态。
- 保留现有 best-effort write blob，再依赖下一次 persist 修正 metadata。
  拒绝原因：这会继续制造“blob 已变、metadata 未跟上”或“metadata 看似已验证、实际未验证”的歧义。

### 决策 4：`menuconfig` 结果语义必须把“取消”与“protector 失配”分开

`menuconfig` 需要把 runtime 返回的 display-safe unlock 结果至少映射成以下几类：

- `cancelled`: 用户取消本次平台验证或等待超时取消
- `denied`: 平台 ACL / 本地授权明确拒绝
- `verification-failed`: 本次验证失败，但尚无证据表明持久化真相层已失配
- `protector-mismatch`: 当前 `os-native` protector 与 canonical wrap 已失配，继续重试不会自动恢复

UI 和日志都必须保留这个区分：
- `cancelled` 不得配上“需要删除 vault”类恢复文案；
- `protector-mismatch` 不得被错误展示成普通取消；
- repeated unlock 在 `cancelled` 后必须重新进入新的 verified flow，而不是直接短路成内部 unwrap 错误。

考虑过的替代方案：
- 继续统一显示 `unlock-failed`，把细分语义留给 debug 日志。
  拒绝原因：用户和自动化都无法据此判断 vault 是否仍安全可用。
- 直接把底层内部错误字符串透传到 UI。
  拒绝原因：不满足 display-safe 和稳定契约要求，也会把实现细节暴露给操作者。

### 决策 5：恢复路径先做“受控诊断 + 显式提示”，不做隐式自愈

对于已经发生真实失配的 vault，本次设计只要求：
- 检测并稳定报告 `protector-mismatch` 或等价结果；
- 保持 vault `locked` / fail-closed；
- 给出 display-safe 恢复提示，例如“恢复原 keychain protector 后重试”或“执行正式 Delete Vault / Re-init”。

系统不得尝试：
- 在未证明旧绑定仍可信时自动重建 protector；
- 在普通 unlock 流程里悄悄删除旧 wrap / 旧 keychain 项；
- 把历史损坏态掩盖为本次普通验证失败。

考虑过的替代方案：
- 在检测到 mismatch 后自动清空 keychain 项并重建。
  拒绝原因：这是高风险破坏性动作，且会让误判代价更高。
- 在 mismatch 后自动切回 passphrase recovery。
  拒绝原因：`menuconfig` 当前没有这条正式交互路径，也违背显式单一路径合同。

### 决策 6：测试要同时覆盖 UI 合同、错误分类和持久化不变式

这次回归不能只靠 `menuconfig` UI 测试，还必须在 `bridgingio-secrets` 层增加持久化不变式测试。最关键的自动化点包括：

- keychain 已存在时，`Cancelled` / `Denied` 不会触发新的 protector 写入；
- verified rewrap 只在完整成功后才改写 wrap blob 与 verification metadata；
- repeated unlock 在 `cancelled` 后仍能重新进入 verified flow；
- `protector-mismatch` 会返回稳定 display-safe 分类，并且日志/worker 汇总保持一致。

## 风险 / 权衡

- [风险] `keyring` crate 可能无法稳定暴露足够细的取消/拒绝错误语义
  → 缓解措施：在共享层定义内部结果枚举；若通用错误不足，就在 macOS/Windows adapter 中补平台特化映射。
- [风险] 收紧 unlock 路径后，某些旧测试或 dev shim 会从“自动生成 KEK”变成 fail-closed
  → 缓解措施：把“创建 protector”显式收束到 init/bootstrap 路径，并同步更新测试夹具。
- [风险] rewrap 提交改为更严格后，会让部分历史 degraded wrap 更晚迁移到 verified 状态
  → 缓解措施：优先保证不损坏现有 vault；迁移速度让位于持久化正确性。
- [风险] UI 结果分类增多会带来文案和日志字段更新成本
  → 缓解措施：保持对外分类数量可控，只暴露 display-safe 的少数稳定结果枚举。
- [风险] 已经损坏的历史 vault 仍然需要人工恢复
  → 缓解措施：明确把这类实例区分成 `protector-mismatch`，避免后续进一步破坏，并给出正式恢复路径。

## Migration Plan

1. 在 `bridgingio-secrets` 中引入 typed `os-native` access outcome，并把“读取已有 protector”与“创建/写入 protector”拆成不同内部路径。
2. 重写 verified `os-native` unlock 的迁移提交流程，确保取消/拒绝/失败不会改写 keychain、wrap blob 或 verification metadata。
3. 调整 `menuconfig` unlock worker 的结果映射与 display-safe 文案，让 `cancelled`、`denied`、`verification-failed`、`protector-mismatch` 可被稳定区分。
4. 补充单元/contract 测试，覆盖平台取消后的 repeated unlock、持久化不变式和日志分类。
5. 如有需要，再追加 operator-facing 文档或恢复指引；本次不做 metadata schema migration。

回滚策略：本次主要是运行时逻辑收紧与结果分类修正，不引入新 schema。若实现出现兼容性问题，可回滚到旧逻辑版本；但已发生真实 `protector-mismatch` 的实例仍需按显式恢复路径处理，而不是依赖回滚自动修复。

## Open Questions

- `keyring` 当前在 macOS/Windows 上是否足以稳定区分 `Cancelled` 与 `Denied`，还是需要进一步下沉到平台原生 adapter？
- 本次是否只暴露 `protector-mismatch` 一个“需要恢复”的结果，还是要继续细分为 `keychain-item-missing`、`keychain-value-invalid`、`wrap-mismatch` 等更窄子类？
- 后续是否需要提供显式的“Repair os-native protector”管理动作，还是继续只保留 `Delete Vault` / 恢复原 keychain 项两条恢复路径？
