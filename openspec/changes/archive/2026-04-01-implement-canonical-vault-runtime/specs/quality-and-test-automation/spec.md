## ADDED Requirements

### 需求:canonical vault productization 必须进入 self-test 与集成验证
在 canonical vault runtime 落地后，`bridgingio-core --self-test`、control-plane contract test 与相关集成测试必须覆盖 vault persistence、locked fail-closed、attestation enforcement、canonical config migration 与 secret-backed SSH lifecycle，而不能继续只验证内存态 shim 或静态 settings projection。

#### 场景:执行包含 canonical vault 的 self-test
- **当** 操作员执行 `bridgingio-core --self-test`
- **那么** 自检必须验证至少以下路径：canonical `vault://...` 归一化、legacy vault backend 配置迁移诊断、initialized-but-locked fail-closed、passphrase unlock、attestation single-use、以及 SSH broker basic lifecycle

#### 场景:验证 synthetic attestation 被拒绝
- **当** 测试向长期 token 签发或 vault unlock 路径传入 synthetic 或过期 attestation
- **那么** 自动化测试必须验证 runtime 明确拒绝该请求，而不是允许 trusted client 自报验证通过

### 需求:桌面与 control-plane 自动化必须验证真实 vault 管理流
桌面 UI automation 与 control-plane contract automation 必须验证真实的 vault 管理流，包括 lock state 投影、unlock flow、token 签发 one-time reveal、token revoke 和 settings-vault 受信任动作，而不是只验证按钮存在或占位命令返回 deferred。

#### 场景:桌面 UI 验证 vault unlock
- **当** 自动化 UI test 运行 `Settings > Vault` 解锁流程
- **那么** 测试必须验证页面先展示真实 `locked/unavailable/uninitialized` 等状态，并在完成 trusted local verification 后触发正式 `unlock_vault` control-plane command

#### 场景:control-plane 验证 token 一次性显示
- **当** 自动化 contract test 通过 trusted control-plane 创建长期 token
- **那么** 测试必须验证创建响应包含一次性明文结果，而后续 list/query 仅返回 summary
