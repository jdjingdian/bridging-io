## ADDED Requirements

### 需求:Settings > Vault 必须展示真实 lock/protector 状态
桌面控制台的 `Settings > Vault` 页面在 canonical vault runtime 落地后，必须展示真实的 vault lock state、protector readiness 和 unlock policy 摘要，而不是只显示 backend 名称或抽象入口。

#### 场景:用户进入已初始化但锁定的 Vault 页面
- **当** 用户进入桌面控制台的 `Settings > Vault` 页面，且当前 vault 已初始化但尚未解锁
- **那么** 页面必须明确展示 `locked` 状态、允许的解锁方法或 protector 摘要，以及当前受限的 secret-backed 管理动作，而不是只显示一个静态 “Unlock Vault” 按钮

#### 场景:当前无可用 protector
- **当** 当前实例的 canonical vault 没有任何策略允许的可用 protector
- **那么** 页面必须展示 fail-closed 的不可用状态和受控诊断，而不是继续呈现仿佛可立即解锁的正常管理页面

### 需求:桌面高风险安全动作必须完成真实 intent/attestation 握手
桌面控制台发起 vault 解锁、长期 token 签发、secret reveal/export 或 scope 扩大时，必须先完成真实的 `Intent + Attestation` 握手，然后再调用正式 control-plane 命令。系统不得继续使用固定占位 attestation id 来冒充本地用户验证结果。

#### 场景:用户在桌面控制台中签发长期 token
- **当** 用户在 `Settings > Vault` 页面请求创建长期 token
- **那么** 前端必须先创建对应 intent、完成本地 trusted verification、取得匹配 attestation，并且 runtime 只在 attestation 校验通过后返回一次性 token 明文结果

#### 场景:用户尝试复用旧 attestation
- **当** 用户界面或脚本尝试复用旧的、已消费或不匹配的 attestation 来再次解锁 vault 或再次签发 token
- **那么** 桌面控制台必须收到 runtime 拒绝结果，并更新页面状态，而不是在本地静默假设验证仍然有效
