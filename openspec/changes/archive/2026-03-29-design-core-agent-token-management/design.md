## 上下文

本次讨论聚焦 core 侧的 token generate，不涉及 UI 改版，但 UI 的调用流程已经很明确：

```text
用户点击生成
  -> UI 先收集 token 备注名称
  -> UI 通过本地 IPC 调用 core
  -> core 生成 token、持久化 metadata/scope
  -> core 只在这次响应里返回明文 token
  -> UI 提示用户自行保存
```

当前仓库里已经有 vault、secret broker、本地管理员 intent/attestation 等安全积木，但 token 管理仍缺少一条可直接用于 IPC 的 core 真相链路：

- `bridgingio-app-api` 还没有 token create/list/revoke 类命令
- `bridgingio-secrets` 已经有 secret record 和 broker，但还没有 `AgentTokenRecord` / `TokenScopeRecord`
- `bridgingio-mcp` 当前仍主要消费请求体中的 `agent_id` 标签，尚未真正把 authenticated principal 从 token 派生

如果这一步直接以“生成一串 token 文本并写入 vault”落地，后续会出现两个明显问题：

- 明文 token 会被错误地建模成可被 reveal 的普通 secret
- target 级权限、后续 scope 更新、authn/authz 衔接都会被压成临时补丁

## 目标 / 非目标

**目标：**

- 为本地受信任 control-plane 固化稳定的 token admin 接口
- 让 token 明文只在签发瞬间返回一次，长期真相只保留 hash 与 metadata
- 支持长期 token 与时效 token
- 把用户看到的备注名称与真正的 authenticated principal 分离
- 为 `target_ids` 级别的授权预留正式 scope 结构，并允许后续更新
- 让未来 standalone 管理入口复用同一套 service 语义

**非目标：**

- 本次实现 standalone CLI
- 本次改 UI 交互细节或展示页面
- 本次把全部 model-plane bearer auth enforcement 一次性补完
- 本次实现完整 passkey ceremony；只预留 attestation 接口位

## 决策

### 决策 1：token 管理必须通过本地受信任 control-plane 暴露，而不是直接复用 vault secret API

token generate 的入口应当是专门的 token admin service，而不是让 UI 直接调用“put 一个 secret”这类泛化 vault 接口。

推荐最小命令面：

```text
CreateAgentToken
- label
- lifetime
- scope
- attestation_id?           // 预留

ListAgentTokens

RevokeAgentToken
- token_id
- reason?

UpdateAgentTokenScope       // 本次预留，不要求实现
- token_id
- patch / replacement
- reason?
- attestation_id?           // 预留
```

这样设计有三个好处：

- UI、future standalone CLI、受信任本地自动化都可以复用同一套 core service
- token 生命周期可以与普通 vault secret 生命周期分层治理
- 后续如果要把 scope 更新纳入本地管理员验证，不需要再推翻 IPC 形状

### 决策 1.1：UI 只提供 label，`token_id` 与 `principal_id` 必须由 core 分配

用户输入的“备注名称”应直接落到 token 的 `label` 字段，但它不应承担认证真相。

推荐语义：

- `label`
  - 用户可见备注，例如 “codex-lab” 或 “nightly-runner”
- `token_id`
  - core 分配的稳定 token 标识，用于 revoke、list、audit
- `principal_id`
  - core 分配的稳定认证身份，用于后续 authn/authz 与资源归属

这意味着：

- UI 首版只需要收集 `label`
- token 的真实身份不依赖用户命名
- 后续即使允许改 label，也不会改变 authenticated principal

### 决策 2：明文 token 只在签发响应中出现一次，持久化只保留 hash 真相

agent access token 与普通 secret 的关键差异是：它的明文值在签发后通常不需要再被系统回读。真正需要长期保存的是“如何校验它”和“它拥有什么权限”。

因此推荐：

- 生成高熵随机 opaque bearer token
- 返回给调用方一次性明文
- core 长期只保存 `token_hash + metadata + active scope`
- 后续 `list/get` 只能返回安全摘要，不再返回明文 token

推荐返回对象：

```text
CreateAgentTokenResult
- plaintext_token: String          // 仅本次响应可见
- summary: AgentTokenSummary
```

推荐摘要对象：

```text
AgentTokenSummary
- token_id: String
- label: String
- principal_summary: String
- status: String
- scope_profile: String
- target_scope_summary: String
- created_at: SystemTime
- last_used_at: Option<SystemTime>
- expires_at: Option<SystemTime>
```

### 决策 2.1：token 不写入 vault 明文真相层；“保存到 vault”只适用于 record/metadata 所在的受保护持久层

你最初提到“core 完成 token 后，将 token 与备注写入 vault”。这里建议收紧一下术语：

- **不建议** 把 access token 明文本身作为 `VaultSecretRecord` 长期保存
- **建议** 把 token 的 metadata record 存放在 core 的受保护持久层中，必要时与 vault / protector 策略协同

也就是说，真正进入长期真相层的是：

- `token_hash`
- `token_id`
- `principal_id`
- `label`
- `status`
- `scope`
- `expires_at / revoked_at / last_used_at`

而不是 bearer token 明文。

### 决策 3：用户视角的“删除 token”在 core 中统一建模为 revoke

为了保证审计、级联收权和后续 authn 行为一致，本次不建议提供“物理删除 token record”的管理语义。

推荐状态机：

```text
issued -> active -> revoked
                -> expired
```

用户在 UI 或本地管理面触发“删除 token”时，core 内部执行：

- 将 token 标记为 `revoked`
- 记录 `revoked_at` 与 `revoke_reason`
- 后续认证一律拒绝

物理清理可以保留为后续 GC / archive 行为，不进入首版用户动作语义。

### 决策 4：scope 必须单独建模，并使用 version 切换而不是原地覆盖

用户已经明确表示 token 后续需要能限制“可访问哪些 target”，而且权限配置未来可能在运行中更新。为了避免以后因为 scope 更新而破坏审计真相，推荐从一开始就把 scope 单独建模为版本化对象。

推荐对象：

```text
AgentTokenRecord
- token_id: String
- principal_id: String
- label: String
- status: String                   // active / revoked / expired
- token_hash: String
- hash_scheme: String
- scope_profile: String
- active_scope_version: u32
- created_by: String
- created_at: SystemTime
- last_used_at: Option<SystemTime>
- expires_at: Option<SystemTime>
- idle_timeout_sec: Option<u64>
- revoked_at: Option<SystemTime>
- revoke_reason: Option<String>
- parent_token_id: Option<String>
- issued_via_attestation_id: Option<String>
```

```text
TokenScopeRecord
- token_id: String
- version: u32
- status: String                   // active / superseded
- scope_profile: String
- target_ids: Vec<String>
- tool_ids: Vec<String>
- max_risk_envelope: String
- allow_open_shell: bool
- allow_write_shell_input: bool
- allow_artifact_cross_principal: bool
- allow_delegation: bool
- allow_admin_actions: bool
- created_by: String
- created_at: SystemTime
- superseded_at: Option<SystemTime>
- change_reason: Option<String>
```

scope 更新语义：

- 创建 token 时生成 `version = 1`
- 更新 scope 时生成新版本并切换 `active_scope_version`
- 旧版本转为 `superseded`
- 不允许通过“修改旧记录”抹掉历史 scope 真相

### 决策 4.1：首版只要求开放 `target_ids` 配置，但其他维度必须正式保留且默认拒绝

本次用户最关心的是 token 能访问哪些 target，因此首版 UI / IPC 可以只真正开放：

- `label`
- `lifetime`（长期或时效）
- `target_ids`
- `scope_profile`（可选，若首版不开放则使用 core 默认值）

但 record 模型不能只剩 `target_ids`。后续必然会演进到：

- `tool_ids`
- `max_risk_envelope`
- `allow_open_shell`
- `allow_write_shell_input`
- `allow_artifact_cross_principal`
- `allow_delegation`
- `allow_admin_actions`

本次建议：

- 字段现在就进入 `TokenScopeRecord`
- 首版未配置维度采用“默认拒绝”或 profile-default 映射
- 这样后续扩展能力时不需要破坏 IPC / 存储模型

### 决策 4.2：scope 更新对后续授权立即生效，但旧 scope 不被回写覆盖

当 token 的 target 范围或其他能力被更新后，后续授权应使用新的 `active_scope_version`。

推荐运行语义：

- 新请求总是按当前 active scope 鉴权
- 已存在的 interactive shell / artifact 后续操作若继续走鉴权，也必须按新 scope 判定
- 旧 scope 版本仅保留审计与回溯用途，不参与新请求授权

这避免了“旧权限被默默延续”和“更新后看不出曾经授过什么权限”这两类问题。

### 决策 5：scope 扩权属于高风险管理动作，本次预留 attestation 接口位

虽然这次不要求完整实现 passkey / local admin verification，但 token create 与 scope expand 最终都应能挂到已有的 intent/attestation 模型上。

因此建议现在就在数据模型和 IPC 上预留：

- `issued_via_attestation_id`
- `attestation_id?`

本次可以允许：

- 本地受信任 UI / control-plane 在默认受信任边界内先完成 create/revoke

后续收紧时不需要重做数据模型。

### 决策 6：future standalone 管理入口必须复用同一套 service，而不是另一套明文参数协议

本次不实现 standalone，但需要固定 future route：

- future `bridgingio-core auth token create/list/revoke/update-scope`
- 与 IPC 复用同一组 service / record / lifecycle 语义
- 不允许 `--token <plaintext>` 或等价 argv 明文输入

这样可以避免：

- UI 能生成 token，但 standalone 不能 revoke
- standalone 自己再发明另一套 scope 形状

### 决策 7：与 model-plane authn/authz 的衔接必须从一开始保持一致

虽然本次用户聚焦 token generate，但 record 模型必须直接服务于后续 bearer auth：

```text
Authorization: Bearer <token>
  -> hash match
  -> AgentTokenRecord
  -> principal_id
  -> active TokenScopeRecord
  -> AuthZ(scope)
```

也就是说：

- `agent_id`、`run_id`、`client_session_id` 仍可保留在请求体中
- 但它们只能作为调用标签
- 真正的 principal 必须来自 `AgentTokenRecord.principal_id`

## 分阶段落地建议

### 阶段 1：token admin service 与 IPC 管理面

优先实现：

- `CreateAgentToken`
- `ListAgentTokens`
- `RevokeAgentToken`
- `AgentTokenRecord` / `TokenScopeRecord`
- 一次性明文返回与后续安全摘要

### 阶段 2：scope 更新与 model-plane bearer enforcement 收口

后续实现：

- `UpdateAgentTokenScope`
- scope version 切换
- HTTP bearer 校验
- principal 从 token 派生
- request context 字段彻底降级为标签
