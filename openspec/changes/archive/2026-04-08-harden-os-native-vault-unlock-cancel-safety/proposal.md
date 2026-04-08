## 为什么

当前 macOS `menuconfig` 的 `os-native` vault 解锁存在一个必现恶性问题：操作员在系统钥匙串弹窗中按 `Esc` 取消后，后续再次解锁会稳定进入 `UnlockFailed("os-native protector could not unwrap the canonical vault root key")`，并且当前实例通常只能通过删除 vault 重建才能恢复。这说明现有 `os-native` 解锁链路没有把“用户取消/ACL 拒绝/临时访问失败”和“keychain 项不存在”严格区分，导致受验证解锁流程可能改写错误的 KEK 或持久化坏状态，直接破坏已有 vault 的可恢复性。

现在必须把这条链路收敛为 fail-closed、cancel-safe、不会损坏既有 vault 的正式合同。否则 macOS 上一次正常的用户取消就可能把本地 secret store 变成不可恢复状态，这既违反当前 vault/approval 设计，也会让 `menuconfig` 的显式解锁入口失去可信度。

## 变更内容

- 收紧 `os-native` protector 的 keychain/readiness/verified unlock 语义：系统必须区分“item 不存在”与“用户取消、ACL 拒绝、平台访问失败”等错误，后者只能 fail-closed 返回，不得生成新 KEK、覆盖既有 keychain 项或触发隐式重建。
- 规定 canonical vault root key 的 unwrap/rewrap 迁移必须是 cancel-safe 和 corruption-safe：只有在确认当前 verified KEK 与既有 wrap 属于同一可信绑定后，系统才允许执行 rewrap；取消、拒绝、超时或 unwrap 失败不得改写已有 wrap blob、manifest 或其他持久化真相层。
- 明确 `menuconfig` 的显式 unlock 失败反馈与恢复语义：当操作员取消或拒绝平台验证时，UI 必须保持 `locked`，返回清晰的“已取消/未授权/验证失败”反馈，并避免把“vault 已损坏”与“用户取消本次验证”混成同一模糊报错。
- 为已发生损坏或绑定不一致的 vault 定义受控诊断与恢复路径：系统必须能够把“本次验证取消”和“现有 os-native protector 与 canonical wrap 已失配”区分为不同结果，并给出 display-safe 的恢复提示，而不是把所有路径都压成同一个通用 `unlock-failed`。
- 补充回归测试，覆盖 keychain 已存在时的取消/拒绝/失败路径、verified rewrap 的持久化原子性、以及 `menuconfig` repeated unlock 的状态稳定性。

## 功能 (Capabilities)

### 新增功能

### 修改功能
- `credential-and-approval-control`: `os-native` protector 的 verified unlock、keychain 访问错误分类、KEK/VRK rewrap 条件、cancel-safe/fail-closed 语义与恢复诊断发生规范级变更。
- `standalone-operator-console`: `menuconfig` 的 `Unlock Vault` 失败/取消反馈、损坏态区分、重复 unlock 后的状态保持与恢复提示发生规范级变更。
- `quality-and-test-automation`: vault unlock 的自动化回归范围需要新增 keychain 取消/拒绝后不损坏既有 vault、重复 unlock 稳定性和持久化不变式验证。

## 影响

- 受影响代码主要包括 [bridgingio-secrets](/Users/magicdian/Documents/personal_project/bridging-io/source/rust/bridgingio-secrets/src/lib.rs) 中的 `os-native` KEK 获取、verified unlock、legacy fallback rewrap 与 metadata/blob persistence；以及 [bridgingio-operator-console](/Users/magicdian/Documents/personal_project/bridging-io/source/rust/bridgingio-operator-console/src/lib.rs) 中的 unlock worker 状态汇总与用户提示。
- 需要同步调整本地授权日志与错误分类，确保 `cancelled`、`denied`、`corrupted-or-mismatched-protector` 等 display-safe 结果可以被 UI 和自动化区分。
- 需要新增 macOS 导向的 contract/unit 测试，并复核 Windows 共享 `os-native` 路径不会因为同类错误分类改动引入回归。
