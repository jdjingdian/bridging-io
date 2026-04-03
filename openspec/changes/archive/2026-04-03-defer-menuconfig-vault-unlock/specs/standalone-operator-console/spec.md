## 新增需求

### 需求:`menuconfig` 的 vault 解锁必须只由显式操作触发
`bridgingio-core menuconfig` 在进入主界面、切换到 Vault 或 Security 页面、或刷新 display-safe 安全摘要时，必须保持对 vault 的被动观察。系统禁止因为加载 router、读取 lock state、统计 summary 或检查 protector readiness 而主动触发系统钥匙串、trusted verification 或其他本地解锁交互。只有在操作员显式触发解锁动作后，系统才可以开始正式 unlock flow。

#### 场景:macOS 操作员进入 menuconfig
- **当** 操作员在 macOS 上启动 `bridgingio-core menuconfig`，且 vault 当前处于 `locked`
- **那么** 系统必须先展示 `locked` 或等价 display-safe 摘要，而不得在进入界面时立即弹出系统钥匙串解锁提示

#### 场景:操作员浏览 Security 页面但未触发解锁
- **当** 操作员在 `menuconfig` 中进入 Vault 或 Security 页面，但尚未按下 `u` 或触发解锁入口
- **那么** 系统必须继续只显示 display-safe 状态投影，而不得因为页面切换或状态刷新启动正式 unlock

### 需求:`menuconfig` 必须为显式解锁提供可确认的反馈流程
`menuconfig` 在操作员显式触发 vault 解锁后，必须提供清晰的中间态与结果态反馈，包括等待系统完成解锁的弹窗、成功后的确认弹窗，以及返回菜单后可持续感知的已解锁提示。系统不得只在底部状态栏瞬时输出一条结果文本而让操作员自行猜测是否解锁成功。

#### 场景:操作员触发 os-native 解锁
- **当** 操作员在 macOS `menuconfig` 中按下 `u` 或触发 Security 页的解锁入口，且当前使用 `os-native` 作为首选解锁方式
- **那么** 系统必须先显示“等待解锁”或等价中间弹窗，再由操作系统弹出正式本地解锁验证界面

#### 场景:解锁成功后确认返回菜单
- **当** 操作员完成显式解锁且 vault 进入 `unlocked`
- **那么** 系统必须展示“解锁成功”或等价确认弹窗，并在操作员确认后返回菜单，同时移除原有 unlock 提示并显示“加密项管理已解锁”或等价已解锁状态

#### 场景:解锁失败或取消
- **当** 操作员显式触发解锁，但本地验证失败或主动取消
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

## 修改需求

## 移除需求
