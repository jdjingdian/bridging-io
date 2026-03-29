## 0. 设计产出已记录

- [x] 0.1 固化 token generate 必须通过本地受信任 control-plane 管理，而不是直接复用泛化 vault `put/get`
- [x] 0.2 固化“UI 只输入备注名称，core 分配 `token_id` 与 `principal_id`”的身份分层
- [x] 0.3 固化“明文 token 只返回一次，长期只保存 hash-only record”的真相边界
- [x] 0.4 固化用户侧“删除 token”在 core 中映射为 revoke，而不是物理删除
- [x] 0.5 固化 `TokenScopeRecord` 必须支持 version 切换，并为 `target_ids` 先行开放、其他维度预留默认拒绝
- [x] 0.6 记录 standalone future route：复用同一 token admin service，本次不实现 standalone 管理命令

## 1. App API / 本地 control-plane

- [x] 1.1 为 `bridgingio-app-api` 增加 `CreateAgentToken`、`ListAgentTokens`、`RevokeAgentToken` 命令
- [x] 1.2 预留 `UpdateAgentTokenScope` 命令与兼容的 line codec 形状
- [x] 1.3 为 token create 响应定义“一次性明文 + 安全摘要”返回对象
- [x] 1.4 为 token list / revoke 定义仅返回安全摘要的响应对象

## 2. Token Record 与生命周期

- [x] 2.1 在 core 安全域中新增 `AgentTokenRecord`、`TokenScopeRecord` 与 `AgentTokenSummary`
- [x] 2.2 实现 opaque bearer token 生成与 hash-only 持久化
- [x] 2.3 实现长期 token 与时效 token 的生命周期字段
- [x] 2.4 实现 revoke 状态流转，并将用户侧 delete 语义映射到 revoke
- [x] 2.5 实现 `active_scope_version` 与 scope superseded 状态切换

## 3. Scope 模型

- [x] 3.1 首版支持 `target_ids` 输入与 canonical target id 持久化
- [x] 3.2 为 `tool_ids`、risk envelope、interactive shell、artifact、delegation、admin controls 预留默认拒绝字段
- [x] 3.3 明确未显式配置维度的默认拒绝或 profile-default 映射
- [x] 3.4 预留 `UpdateAgentTokenScope` 的 record / service 钩子，即使首版暂不开放 UI

## 4. Runtime 接入

- [x] 4.1 在 `bridgingio-mcp` 的本地 app request handler 中接入 token create/list/revoke
- [x] 4.2 确保本地 control-plane 查询默认只返回安全投影
- [x] 4.3 将 token 明文只限制在 create 响应中出现一次

## 5. Authn/Authz 收口

- [x] 5.1 在后续 model-plane bearer auth 中使用 token record 派生 `principal_id`
- [x] 5.2 将请求体中的 `agent_id`、`run_id`、`client_session_id` 收敛为标签字段而非身份真相
- [x] 5.3 让 target scope 与后续多维 scope 在 AuthZ 阶段参与判定

## 6. Standalone Follow-Up

- [x] 6.1 规划 future standalone `auth token create/list/revoke/update-scope` 管理入口
- [x] 6.2 确保 standalone 管理入口与 IPC 复用同一 service / record 语义
- [x] 6.3 明确禁止通过 argv 明文传递 token 或其他 secret

## 7. Verification

- [x] 7.1 补充 create/list/revoke 的 control-plane contract tests
- [x] 7.2 补充“明文只在首次返回，后续查询不可再得”的测试
- [x] 7.3 补充 scope version 切换与 superseded 保留的测试
- [x] 7.4 补充长期 / 时效 token 生命周期测试
