## 1. 拆分被动状态投影与主动解锁

- [x] 1.1 在 `bridgingio-secrets` 中补齐被动 vault 状态投影接口，使 lock state、策略摘要与 summary 读取不触发正式 unlock
- [x] 1.2 调整 `os-native` readiness / keyring 探测边界，确保 router 初始化与 display-safe 读取不会访问会触发用户验证的平台接口
- [x] 1.3 重构 `bridgingio-operator-console` 的启动摘要加载路径，使进入 `menuconfig` 与浏览 Security/Vault 页面时保持 locked 被动观察

## 2. 实现 menuconfig 显式解锁反馈流

- [x] 2.1 为 `menuconfig` 引入统一的 unlock flow 状态机，并让 `u` 快捷键与 Security 页面解锁入口复用同一条流程
- [x] 2.2 增加“等待解锁”“解锁成功确认”“解锁失败/取消”弹窗，以及返回菜单后的“加密项管理已解锁”状态提示
- [x] 2.3 调整 Security 页面条目呈现方式，使 vault 已解锁后不再继续显示误导性的 unlock action
- [x] 2.4 补齐 `menuconfig` 新增 unlock flow 的中英文 catalog 文案

## 3. 补齐回归验证

- [x] 3.1 为 `menuconfig` 增加回归测试，验证进入界面和切换到 Security/Vault 页面不会触发主动解锁
- [x] 3.2 增加显式 unlock 成功/失败路径测试，覆盖等待态、确认态与 locked/unlocked 状态切换
- [x] 3.3 更新质量验证说明或测试矩阵，记录 macOS `menuconfig`“启动不弹钥匙串、显式触发才解锁”的验收要求

## 4. 实现后问题回补与规范同步

- [x] 4.1 修复显式解锁状态一致性缺陷：避免“系统验证未完成即显示成功”，并处理“验证完成但等待态不退出”的卡滞问题
- [x] 4.2 修复 verified `os-native` 对历史 fallback wrap 的兼容性，消除“系统验证成功但 canonical root key 解包失败”回归
- [x] 4.3 收敛 `menuconfig` 显式解锁触发路径，避免一次 `u` 触发多次系统认证，并移除隐藏 passphrase 回退阻塞
- [x] 4.4 为等待态补充 `Esc` 取消交互与状态回收文案，确保用户可中断本地验证等待过程
- [x] 4.5 强化 Security 菜单关键状态语义：`Vault 锁状态` 红/绿高亮，`解锁 Vault --->` 与 `-*- 加密项管理已解锁` 互斥呈现
- [x] 4.6 将上述回补内容同步到 proposal/design/specs，确保本次 change 的规范、实现与测试保持一致
