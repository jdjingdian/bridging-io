## 新增需求

### 需求:vault 与本地验证错误必须映射到共享错误与状态契约
BridgingIO 在 vault、token、intent、attestation、approval 与本地验证路径上返回的公共错误，必须映射到共享错误与状态契约，同时保留安全域专属子码。系统不得把 `locked`、`verification_required`、`attestation_mismatch`、`passphrase_rejected` 等专属语义统一压扁为普通 validation failure。

#### 场景:本地管理员验证失败
- **当** 用户或本地受信任调用面在执行高风险安全动作时遇到验证缺失、验证过期或 attestation 不匹配
- **那么** 系统必须返回共享错误封装，并保留安全域专属子码，以便调用方稳定区分“需要重新验证”和“真正内部失败”

### 需求:安全域公共错误必须默认保持 display-safe
BridgingIO 在 vault、token、secret broker 和 approval 路径中对外暴露的错误 message 与 details 必须默认保持 display-safe，不得把 secret 明文、token 明文、密文 locator 或等价高敏内部字段带入公共错误对象。

#### 场景:secret-backed 操作失败
- **当** 一个 secret-backed 操作因为 vault unavailable、broker delivery 失败或 policy 拒绝而失败
- **那么** 系统必须向公共调用面返回 display-safe 的错误摘要和恢复提示，而不能把内部 secret 材料或 locator 信息拼入错误消息
