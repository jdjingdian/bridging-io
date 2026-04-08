## 新增需求

### 需求:`menuconfig` 必须为 `os-native` protector 失配提供受控恢复提示
当 `menuconfig` 通过显式 unlock 流程得知当前 `os-native` protector 与 canonical vault root wrap 已失配时，系统必须把该状态作为独立的受控失败类型展示，而不是继续把它混同为普通取消、普通验证失败或内部原始报错字符串。恢复提示必须保持 display-safe，并明确要求操作员走正式恢复路径。

#### 场景:显式 unlock 返回 protector 失配
- **当** `menuconfig` 的 unlock worker 收到 `protector-mismatch` 或等价受控结果
- **那么** UI 必须保持 `locked`
- **并且** 必须展示与“本次取消”不同的恢复提示，例如恢复原有 keychain protector 后重试或执行正式 `Delete Vault` / `Init Vault` 路径

#### 场景:普通取消不得误显示为失配恢复提示
- **当** 操作员只是取消了一次平台验证，而 runtime 未报告 `protector-mismatch`
- **那么** `menuconfig` 必须仅反馈本次验证已取消、拒绝或失败
- **并且** 不得过早提示删除 vault、重建 protector 或等价高风险恢复动作

## 修改需求

### 需求:`menuconfig` 必须为显式解锁提供可确认的反馈流程
`menuconfig` 在操作员显式触发 vault 解锁后，必须提供清晰的中间态与结果态反馈，包括等待系统完成解锁的弹窗、成功后的确认弹窗，以及返回菜单后可持续感知的已解锁提示。系统不得只在底部状态栏瞬时输出一条结果文本而让操作员自行猜测是否解锁成功。对于失败结果，`menuconfig` 必须稳定区分本次取消/拒绝/验证失败与当前 protector 已失配两类语义；取消一次平台验证不得把原本仍健康的 vault 变成后续 repeated unlock 的永久失败状态。

#### 场景:操作员触发 os-native 解锁
- **当** 操作员在 macOS `menuconfig` 中按下 `u` 或触发 Security 页的解锁入口，且当前使用 `os-native` 作为首选解锁方式
- **那么** 系统必须先显示“等待解锁”或等价中间弹窗，再由操作系统弹出正式本地解锁验证界面

#### 场景:解锁成功后确认返回菜单
- **当** 操作员完成显式解锁且 vault 进入 `unlocked`
- **那么** 系统必须展示“解锁成功”或等价确认弹窗，并在操作员确认后返回菜单，同时移除原有 unlock 提示并显示“加密项管理已解锁”或等价已解锁状态

#### 场景:解锁失败或取消
- **当** 操作员显式触发解锁，但本地验证失败、被平台拒绝或主动取消
- **那么** 系统必须返回 `locked` 状态并给出明确失败反馈，而不得继续显示误导性的已解锁提示

#### 场景:等待态支持 Esc 取消
- **当** 操作员在 `WaitingForUnlock` 中间态按下 `Esc`
- **那么** `menuconfig` 必须立即退出等待弹窗并给出“已取消等待”反馈，且后续状态不得误报为已解锁

#### 场景:显式解锁入口不走隐藏回退路径
- **当** 操作员通过 `u` 或 Security 解锁入口触发本地解锁
- **那么** `menuconfig` 必须只执行 `os-native` 本地验证路径，不得在同一交互内自动回退到 passphrase 或其他隐藏输入流程

#### 场景:Security 菜单中的锁状态和解锁条目遵循强语义展示
- **当** 操作员进入 Security 菜单
- **那么** 系统必须将 `Vault 锁状态` 值按 `locked=红色`、`unlocked=绿色`（其他状态黄色）高亮显示
- **并且** 当 vault 为 `locked` 时显示 `解锁 Vault --->`
- **并且** 当 vault 为 `unlocked` 时显示 `-*- 加密项管理已解锁`，且不再显示解锁动作入口

#### 场景:平台取消后再次解锁仍走正式验证流程
- **当** 操作员在一次平台钥匙串弹窗中取消验证，且当前 vault 实际上仍保持健康的 `locked` 状态
- **那么** 之后再次触发 `Unlock Vault` 时，`menuconfig` 必须重新进入正式 waiting + verified unlock 流程
- **并且** 不得因为上一次取消而直接短路成永久 `UnlockFailed("os-native protector could not unwrap the canonical vault root key")` 或等价损坏态
