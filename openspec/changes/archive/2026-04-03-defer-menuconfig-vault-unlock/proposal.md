## 为什么

当前 `bridgingio-core menuconfig` 在 macOS 上进入界面时就可能触发系统钥匙串访问提示，但现有交互文案和产品意图都要求 vault 解锁是按需触发，而不是在启动 `menuconfig` 时被动触发。与此同时，现有 TUI 只在底部状态栏输出一条解锁结果，缺少“等待解锁”“解锁成功确认”“已解锁后移除提示”的完整反馈流，导致操作员很难判断系统当前处于哪个解锁阶段。

在实现过程中还暴露了三类高风险体验问题，需要纳入同一变更闭环：
- 系统验证完成前可能提前显示成功，或验证完成后仍停留在等待态，造成 UI 与真实状态不一致；
- 一次 `u` 解锁可能触发多次系统弹窗，且等待态缺少可中断路径；
- Security 菜单中 `Vault 锁状态` 与 `解锁/已解锁` 条目视觉提示不够强，关键状态不够醒目。

## 变更内容

- 调整 `menuconfig` 的 vault 状态加载路径，确保进入 TUI、浏览 Security/Vault 页面或刷新 display-safe 摘要时，不会主动触发 `os-native` 钥匙串访问、trusted verification 或等价本地解锁动作。
- 为 `menuconfig` 增加显式 unlock flow：仅当操作员按下 `u` 快捷键或触发 Security 页面中的解锁入口时，系统才开始正式解锁流程。
- 为显式 unlock flow 补齐交互反馈，包括“等待系统解锁”的中间弹窗、解锁成功后的确认弹窗，以及返回菜单后显示“加密项管理已解锁”并移除原有 unlock 提示。
- 修复 `os-native` 显式解锁状态一致性问题：禁止提前成功；对历史 fallback wrap 进行兼容解包与自动重包裹迁移，避免“系统验证成功但 vault 解包失败”。
- 收敛显式解锁触发行为：`menuconfig` 入口仅走 `os-native` 本地验证，不做隐藏 passphrase 回退；通过去除初始化阶段隐式 keyring 访问减少重复系统弹窗。
- 为等待态提供 `Esc` 可取消能力（取消 menuconfig 等待，不强制关闭系统弹窗），避免 TUI 长时间卡在等待页。
- 强化 Security 菜单关键状态可见性：`Vault 锁状态` 值同步使用红/绿高亮；未解锁时显示 `解锁 Vault --->`，已解锁时显示 `-*- 加密项管理已解锁`。
- 增加针对 macOS `menuconfig` 的回归验证，覆盖“进入界面不触发钥匙串”“显式解锁才触发系统弹框”“成功后 UI 状态切换完成”这三类关键路径。

## 功能 (Capabilities)

### 新增功能

- 无

### 修改功能

- `standalone-operator-console`: 收紧 `menuconfig` 的 vault 交互时机，并为显式解锁补齐等待态、成功确认和已解锁反馈。
- `credential-and-approval-control`: 明确 display-safe vault 状态投影与 readiness 检查不得隐式触发本地解锁、trusted verification 或平台 keyring 访问。
- `quality-and-test-automation`: 增加 `menuconfig` 显式解锁时机与 macOS 解锁反馈流程的回归验证要求。

## 影响

- 受影响模块：`source/rust/bridgingio-operator-console`、`source/rust/bridgingio-secrets`、相关 i18n 资源与测试。
- 受影响行为：`menuconfig` 启动时的 vault/router 初始化、副作用边界、Security 页面 action 呈现方式、解锁弹窗状态机。
- 受影响验证：需要补充 `menuconfig` 单元/集成测试，并为 macOS `os-native` 路径增加不提前触发钥匙串的回归覆盖。
