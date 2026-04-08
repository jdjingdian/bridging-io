## 新增需求

### 需求:`os-native` protector 的取消安全与持久化不变式必须具备自动化回归覆盖
针对 canonical vault 的 `os-native` verified unlock，自动化测试必须覆盖 keychain 取消/拒绝后的安全不变式，而不是只验证 UI 仍显示 locked。测试必须证明：取消或失败不会生成新的 protector material、不会改写现有 canonical wrap、不会伪造 verification metadata，并且 protector 失配会产生稳定的 display-safe 诊断。

#### 场景:已有 protector 时取消验证不会改写 key material
- **当** 自动化准备一个已有 `os-native` protector 的 vault，并在一次显式 verified unlock 中模拟用户取消或 ACL 拒绝
- **那么** 测试必须验证现有 protector material 未被覆盖或重建
- **并且** 必须验证 canonical wrap blob 与其 digest 保持不变

#### 场景:verified rewrap 仅在完整成功后推进元数据
- **当** 自动化覆盖 legacy fallback wrap 迁移到 verified `os-native` 的路径，并分别模拟成功、unwrap 失败与持久化提交失败
- **那么** 测试必须验证只有完整成功的路径才会推进 `wrapped_key_digest`、`last_verified_at` 与 `status`
- **并且** 失败路径不得留下半迁移状态

#### 场景:protector 失配返回稳定 display-safe 诊断
- **当** 自动化构造“当前 keychain protector 与 canonical wrap 已失配”的 vault 状态并触发显式 unlock
- **那么** 测试必须验证系统返回稳定的 `protector-mismatch` 或等价 display-safe 结果
- **并且** 不得退化为裸内部错误字符串或普通取消结果

## 修改需求

### 需求:`menuconfig` 的显式解锁时机与反馈流程必须有回归覆盖
针对 `bridgingio-core menuconfig` 的自动化或 contract 测试必须覆盖显式解锁时机与结果反馈，特别是 macOS `os-native` 路径。测试禁止只验证解锁入口存在，而必须验证进入界面时不触发系统验证、显式 unlock 后才进入等待态，以及成功或失败后 UI 状态完成切换。对于平台取消路径，测试还必须验证 repeated unlock 不会把健康 vault 推入永久损坏态。

#### 场景:macOS 进入 menuconfig 不触发系统验证
- **当** 自动化在 macOS 上启动 `bridgingio-core menuconfig`，且 vault 当前处于 `locked`
- **那么** 测试必须验证界面先显示 locked 摘要，且在未按下 `u` 或未触发解锁入口前不会触发系统钥匙串验证

#### 场景:显式解锁后完成成功状态切换
- **当** 自动化在 macOS `menuconfig` 中显式触发 vault 解锁，并模拟本地验证成功
- **那么** 测试必须验证界面依次经历等待态、成功确认态，并在确认返回后移除 unlock 提示并显示已解锁状态

#### 场景:显式解锁失败后保持 locked
- **当** 自动化在 `menuconfig` 中显式触发解锁，但本地验证失败或被取消
- **那么** 测试必须验证界面回到 locked 状态并显示明确失败反馈，而不是误报为已解锁

#### 场景:单次显式解锁不会触发重复认证链路
- **当** 自动化在 `menuconfig` 中触发一次 `u` 解锁
- **那么** 测试必须验证初始化和显式解锁路径不会叠加触发多次系统认证流程，且不会进入隐藏 passphrase 回退阻塞

#### 场景:等待态支持 Esc 取消且不会误报成功
- **当** 自动化在等待本地验证时发送 `Esc`
- **那么** 测试必须验证 UI 立即退出等待态并进入取消反馈，且最终状态保持或恢复为 locked，不得出现“取消后仍报解锁成功”

#### 场景:平台取消后 repeated unlock 仍可重新发起正式验证
- **当** 自动化在已有健康 vault 的前提下，先模拟一次平台钥匙串取消，再次触发 `Unlock Vault`
- **那么** 测试必须验证第二次仍重新进入 waiting + verified unlock 流程
- **并且** 不得因为第一次取消而稳定复现 canonical wrap unwrap 失败

#### 场景:Security 菜单关键状态语义有样式回归覆盖
- **当** 自动化渲染 Security 菜单
- **那么** 测试必须验证 `Vault 锁状态` 的值高亮遵循 `locked=红色`、`unlocked=绿色`，并验证 `解锁 Vault --->` 与 `-*- 加密项管理已解锁` 的互斥呈现规则

### 需求:授权观测与单次触发合同必须具备自动化回归覆盖
针对 `menuconfig` 与本地受信任授权链路的自动化测试，必须覆盖授权日志落盘、display-safe 边界和 verified `os-native` 的 singleflight 行为，而不是只验证“按钮存在”或“最终状态变成 unlocked”。测试必须能够回答一次显式授权是否产生了稳定 flow、记录了哪些关键事件，以及底层平台验证是否只被调用一次。对于取消、拒绝与 protector 失配等结果，日志也必须保持稳定的 display-safe 分类。

#### 场景:显式授权动作产生持久化日志
- **当** 自动化在 `menuconfig` 中触发一次显式授权动作，例如 `Unlock Vault` 或 `Delete Token`
- **那么** 测试必须验证 `logs/local-authorization.jsonl` 中存在对应的 display-safe 事件
- **并且** 该事件必须包含稳定 `flow_id`、`operation`、`phase` 与结果字段

#### 场景:debug 级别产生 session breadcrumb
- **当** 自动化以 `core.log_level=debug` 或更高等级运行 `menuconfig`
- **那么** 测试必须验证 `logs/menuconfig-session.jsonl` 中出现与授权 flow 可关联的 breadcrumb 事件
- **并且** 至少覆盖 screen/action、worker 生命周期或去重命中等高价值节点

#### 场景:授权日志保持 display-safe
- **当** 自动化执行涉及 passphrase、SSH 私钥或 token 一次性结果的本地授权流程
- **那么** 测试必须验证新落盘日志不包含私钥明文、passphrase、token 明文或等价 secret material

#### 场景:并发 verified os-native 解锁只访问一次 provider
- **当** 自动化在同一进程内并发触发两个 verified `os-native` 解锁请求
- **那么** 测试必须验证平台 provider / keyring 访问只发生一次
- **并且** 必须验证结果中同时存在 `leader` 与 `joined` 或等价去重语义，而不是两个互不相关的验证流程

#### 场景:unlock worker 跨线程汇总保持 flow 与 dedupe 一致
- **当** 自动化通过 `menuconfig` unlock worker 路径触发一次显式 `Unlock Vault`
- **那么** 测试必须验证 `logs/local-authorization.jsonl` 的 unlock 成功事件含有非 `not-applicable` 的 dedupe 语义（例如 `leader` 或 `joined`）
- **并且** 必须验证 `logs/menuconfig-session.jsonl` 中 `action=unlock.worker` 事件复用同一个授权 `flow_id`

#### 场景:取消与失配结果落盘为稳定分类
- **当** 自动化分别模拟平台取消、ACL 拒绝和 protector 失配三种显式 unlock 结果
- **那么** 测试必须验证授权日志与 worker 汇总事件能够稳定区分这些 display-safe 分类
- **并且** 不得全部退化为同一个模糊 `unlock-failed`
