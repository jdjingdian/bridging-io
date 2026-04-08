## 1. `bridgingio-secrets` 的 `os-native` unlock 语义收敛

- [x] 1.1 为 `os-native` protector 访问引入可区分 `Found/NotFound/Cancelled/Denied/PlatformFailure` 的内部结果模型，并把读取路径与 provisioning/写入路径拆开
- [x] 1.2 收紧已有 vault 的 verified `os-native` unlock：除 init/bootstrap 或显式 provisioning 外，任何读取失败、取消或拒绝都必须 fail-closed，且不得生成或覆盖 protector KEK
- [x] 1.3 重构 legacy fallback -> verified `os-native` 的 rewrap 提交流程，确保取消、unwrap 失败或持久化失败不会改写既有 wrap blob、digest、`last_verified_at` 或 `status`
- [x] 1.4 修正 wrap verification metadata 的推进时机，避免普通 `persist_metadata_db()` 把未验证的 wrap 伪装为“刚验证过”
- [x] 1.5 为 `protector-mismatch` 或等价失配路径补充稳定的 display-safe 错误/恢复语义

## 2. `menuconfig` unlock 结果与恢复提示

- [x] 2.1 调整 unlock worker 与 UI 状态汇总，稳定区分 `cancelled`、`denied`、`verification-failed` 与 `protector-mismatch`
- [x] 2.2 更新 `menuconfig` 的 unlock 失败/取消提示文案，保证普通取消不会显示高风险恢复动作，而失配结果会给出受控恢复提示
- [x] 2.3 验证 repeated unlock 行为：平台取消后的下一次 `Unlock Vault` 必须重新进入正式 waiting + verified unlock 流程，而不是直接短路为永久 unwrap 失败
- [x] 2.4 对齐授权日志与 session breadcrumb 的结果分类，确保 display-safe 事件能够反映取消、拒绝与失配的差异

## 3. 回归测试与验证

- [x] 3.1 为 `bridgingio-secrets` 增加单元测试，覆盖已有 protector 时的取消/拒绝不会写入新 KEK，也不会改变 canonical wrap blob
- [x] 3.2 为 verified rewrap 增加成功/失败/提交失败测试，验证 metadata 与 blob 只在完整成功后推进
- [x] 3.3 为 `menuconfig` 增加自动化或 contract 测试，覆盖平台取消后的 repeated unlock 仍可重新发起正式验证
- [x] 3.4 为授权日志与 worker 汇总增加回归测试，验证 `cancelled`、`denied`、`protector-mismatch` 的 display-safe 分类稳定落盘
- [x] 3.5 在 macOS 路径通过后，复核 Windows 共享 `os-native` 抽象不会因新的错误分类和 fail-closed 语义引入回归
