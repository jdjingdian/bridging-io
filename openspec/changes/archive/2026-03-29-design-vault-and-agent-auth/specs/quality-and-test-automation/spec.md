## 修改需求

### 需求:core 必须具备跨平台契约测试基线
BridgingIO 的 Rust core 必须建立独立于 UI 的 cross-platform contract test 基线，用于验证宿主平台相关的关键能力。对于 `design-vault-and-agent-auth` 已经产品化且属于 contract-critical 的安全边界，`bridgingio-core --self-test` 必须额外覆盖 canonical `CredentialRef` 归一化、degraded vault fail-closed、broker-only secret use、本地管理员 `intent + attestation` 单次消费、secret-backed SSH delivery lifecycle，以及 non-loopback model-plane 安全默认值。

#### 场景:执行包含 vault / auth smoke 的 self-test
- **当** 团队执行 `bridgingio-core --self-test` 作为 core contract smoke
- **那么** 自检必须验证 canonical `vault://...` 归一化、degraded backend fail-closed、本地管理员验证单次消费，以及 secret-backed SSH delivery 的 broker / fallback 语义，而不能只验证 shell 与 platform contract

#### 场景:验证 model-plane 安全默认值
- **当** 团队执行 `bridgingio-core --self-test`
- **那么** 自检必须验证 non-loopback model-plane 暴露仍要求显式 enable 与认证保护，而不能把 `auth_mode=none` + 非 loopback 暴露当作可接受默认值
