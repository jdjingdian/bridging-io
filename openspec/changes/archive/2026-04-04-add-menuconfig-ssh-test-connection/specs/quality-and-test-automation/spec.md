## 新增需求

### 需求:`menuconfig` 的 SSH 测试连接流程必须具备自动化回归覆盖
针对 `bridgingio-core menuconfig` 的自动化或 contract 测试必须覆盖 SSH `Test Connection` 的关键交互和结果语义。测试不得只验证按钮存在，而必须验证其真实探测、状态门控、取消/超时行为与日志边界。

#### 场景:plain SSH 可以在未 Apply/Save 前测试成功
- **当** 自动化在 `menuconfig` 中修改一个 plain SSH target 的 host、port、username 或 `credential_ref` 草稿，但尚未触发 `Apply Target --->` 或 `Save`
- **那么** 测试必须验证 `Test Connection --->` 仍可对当前草稿执行真实 SSH 探测
- **并且** 成功结果不得依赖磁盘上旧配置

#### 场景:测试连接覆盖失败、超时与取消路径
- **当** 自动化触发一次 SSH 测试连接，并分别模拟认证失败、超时与等待态 `Esc` 取消
- **那么** 测试必须验证界面分别返回明确的失败、超时与取消结果
- **并且** 不得出现取消后仍误报成功的情况

#### 场景:sealed SSH 的测试入口受 unlock 状态门控
- **当** 自动化分别在 vault `locked` 与 `unlocked` 状态下进入 sealed SSH target
- **那么** 测试必须验证 `locked` 状态下不显示 `Test Connection --->`
- **并且** 必须验证 `unlocked` 状态下可在 `Sensitive Overlay` 中触发该动作

#### 场景:plain SSH 的 vault-backed credential 不触发隐式 unlock
- **当** 自动化对一个绑定 vault-managed credential 的 plain SSH target 触发测试连接，且 vault 当前为 `locked`
- **那么** 测试必须验证该动作返回受控失败结果
- **并且** 不得隐式触发 vault unlock 流程

#### 场景:broker endpoint 缺失时返回专属失败并可定位
- **当** 自动化对依赖 broker delivery 的 SSH target 触发测试连接，且 `IdentityAgent` endpoint 缺失或不可访问
- **那么** 测试必须验证结果被归类为 broker 专属失败（例如 `broker-endpoint-unavailable`），而不是普通认证失败
- **并且** 必须验证结果反馈包含“broker endpoint 未就绪”或等价可定位提示
- **并且** 不得通过 fallback 本地私钥路径把该用例误判为成功

#### 场景:测试连接日志保持 display-safe
- **当** 自动化执行一次成功或失败的 SSH 测试连接
- **那么** 测试必须验证 `logs/menuconfig-session.jsonl` 中存在带稳定 `flow_id` 的相关事件
- **并且** 必须验证这些事件不包含私钥明文、passphrase、token 明文或 raw SSH stderr

## 修改需求

## 移除需求
