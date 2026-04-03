## 新增需求

### 需求:display-safe vault 状态投影不得隐式触发本地解锁
系统在返回 display-safe 的 vault lock state、策略摘要、summary、protector diagnostics 或等价 readiness 投影时，必须将“读取状态”与“执行解锁”分离。状态投影路径禁止消费 unlock material、调用正式 unlock handler、访问会触发用户验证的 platform keyring 接口，或产生等价的本地解锁副作用。

#### 场景:受信任本地界面只请求 vault 摘要
- **当** `menuconfig`、桌面控制台或其他受信任本地界面只请求 lock state、allowed methods、token summary 或等价 display-safe 摘要
- **那么** runtime 必须返回被动状态投影，而不得因此触发系统钥匙串、passkey、passphrase prompt 或等价解锁交互

#### 场景:未显式声明 unlock intent
- **当** 当前请求未显式声明 unlock intent，也未调用 `unlock_vault` 或等价正式解锁路由
- **那么** 系统必须保持 vault 当前 lock state，而不得因为 readiness 检查、router 初始化或 protector 探测把状态推进到 `unlocking` 或 `unlocked`

#### 场景:平台只能通过用户验证判断 os-native readiness
- **当** 某个平台只有在触发用户验证后才能精确判断 `os-native` protector 是否可用
- **那么** 系统必须返回受控的 deferred/unknown readiness 或等价 display-safe 诊断，而不得为了获取更精确状态提前触发验证

#### 场景:router 初始化不得触发平台 keyring 验证
- **当** runtime 仅执行 router 默认初始化、持久化元数据加载或 display-safe 摘要读取
- **那么** 系统不得访问会触发系统弹窗的 keyring 验证路径，避免把一次显式解锁放大为多次系统认证请求

#### 场景:verified os-native 解锁需要兼容历史 fallback wrap
- **当** vault 历史 root wrap 由 fallback KEK 生成，而当前显式解锁走 verified `os-native` 路径
- **那么** 系统必须在验证成功后允许兼容解包，并自动重包裹迁移到当前 verified KEK，避免“系统验证成功但 root key 解包失败”

## 修改需求

## 移除需求
