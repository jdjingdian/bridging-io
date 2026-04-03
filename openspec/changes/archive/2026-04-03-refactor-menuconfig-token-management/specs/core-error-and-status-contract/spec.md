## 新增需求

### 需求:token 认证拒绝必须暴露稳定的公共错误子码
当 MCP、model-plane 或其他公共调用面因 bearer token 认证失败而拒绝请求时，系统必须返回共享结构化错误封装，并为 token 认证拒绝暴露稳定的 canonical 标识。对于这类错误，系统必须使用 `domain = authn`、`common_code = credential_rejected`，并通过 `module_code` 区分 `agent_token_invalid`、`agent_token_disabled`、`agent_token_revoked` 与 `agent_token_expired`。系统不得再要求调用方通过 message 文本猜测 token 失败原因。

#### 场景:disabled token 的公共错误
- **当** 公共调用面收到一个已被本地管理面禁用的 bearer token
- **那么** 系统必须返回 `domain = authn`、`common_code = credential_rejected`、`module_code = agent_token_disabled`，并提供 display-safe 的恢复提示

#### 场景:revoked token 的公共错误
- **当** 公共调用面收到一个已被撤销的 bearer token
- **那么** 系统必须返回 `domain = authn`、`common_code = credential_rejected`、`module_code = agent_token_revoked`

#### 场景:expired token 的公共错误
- **当** 公共调用面收到一个已经过期的 bearer token
- **那么** 系统必须返回 `domain = authn`、`common_code = credential_rejected`、`module_code = agent_token_expired`

#### 场景:未知 token 的公共错误
- **当** 公共调用面收到一个不存在或无法匹配的 bearer token
- **那么** 系统必须返回 `domain = authn`、`common_code = credential_rejected`、`module_code = agent_token_invalid`

## 修改需求
<!-- 无 -->

## 移除需求
