## 新增需求

### 需求:授权观测与单次触发合同必须具备自动化回归覆盖
针对 `menuconfig` 与本地受信任授权链路的自动化测试，必须覆盖授权日志落盘、display-safe 边界和 verified `os-native` 的 singleflight 行为，而不是只验证“按钮存在”或“最终状态变成 unlocked”。测试必须能够回答一次显式授权是否产生了稳定 flow、记录了哪些关键事件，以及底层平台验证是否只被调用一次。

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

## 修改需求

## 移除需求
