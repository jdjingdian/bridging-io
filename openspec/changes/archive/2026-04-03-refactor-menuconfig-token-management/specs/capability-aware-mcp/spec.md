## 新增需求

### 需求:bearer token 认证失败必须返回可区分的 token 拒绝原因
当 model-plane 或 MCP typed tools 因 bearer token 认证失败而拒绝请求时，系统必须返回可区分的 token 拒绝原因，而不能继续把 `invalid`、`disabled`、`revoked`、`expired` 等不同情况压成单一的“无效 token”语义。该拒绝原因必须能够被 MCP 回包、审计记录和本地管理面一致消费，并与后续的 scope / policy 拒绝区分。

#### 场景:disabled token 发起 MCP 请求
- **当** 客户端显式携带一个处于 `disabled` 管理状态的 bearer token 调用 model-plane 或 MCP typed tool
- **那么** 系统必须在 `authn` 阶段直接拒绝该请求，并返回等价于 `token_disabled` 的稳定拒绝原因

#### 场景:revoked token 发起 MCP 请求
- **当** 客户端显式携带一个已被 `revoked` 的 bearer token 调用 model-plane 或 MCP typed tool
- **那么** 系统必须在 `authn` 阶段直接拒绝该请求，并返回等价于 `token_revoked` 的稳定拒绝原因

#### 场景:expired token 发起 MCP 请求
- **当** 客户端显式携带一个已过期的 bearer token 调用 model-plane 或 MCP typed tool
- **那么** 系统必须在 `authn` 阶段直接拒绝该请求，并返回等价于 `token_expired` 的稳定拒绝原因

#### 场景:未知 token 发起 MCP 请求
- **当** 客户端显式携带一个不存在或无法匹配的 bearer token 调用 model-plane 或 MCP typed tool
- **那么** 系统必须在 `authn` 阶段直接拒绝该请求，并返回等价于 `token_invalid` 的稳定拒绝原因

## 修改需求

### 需求:agent token 生命周期必须单向收敛并级联收权
系统必须把 agent token 设计成“单向终态 + 可逆启停”的组合生命周期对象。token 在签发后进入可访问态，并且在未 `revoked`、`expired` 或 `deleted` 时允许本地受信任管理面临时切换为 `disabled` 或等价的不可访问状态；`revoked`、`expired` 与 `deleted` 仍属于不可重新激活的收敛终态。系统不得通过 enable/disable 开关让已过期或已撤销 token 恢复可用。若 token 之间存在 delegation lineage，则父 token 进入失效终态时必须对仍处于可访问态或 `disabled` 管理态的子 token 一致施加级联收权。

#### 场景:token 被禁用后不得继续使用
- **当** 某个 token 被本地受信任管理面切换为 `disabled`
- **那么** 系统必须拒绝继续使用该 token，直到其被重新启用或进入其他终态

#### 场景:禁用 token 到期后不得因重新启用而复活
- **当** 某个 token 在 `disabled` 状态下到达 TTL 或 idle timeout 并进入 `expired`
- **那么** 系统不得因本地管理面后来重新启用该 token 而把它恢复为可访问状态

#### 场景:父 token 收权后子 token 级联失效
- **当** 某个允许 delegation 的父 token 被 revoke 或因策略进入失效终态
- **那么** 仍处于可访问态或 `disabled` 管理态的子 token 必须级联失效，并在后续访问中被一致拒绝

### 需求:token 查询与管理返回必须使用安全投影而不是内部认证真相
系统必须把 token 的内部认证真相与对本地管理面的展示结果分离。即使在本地受信任 control-plane 中，token 查询默认也只能返回安全投影，例如 `token_id`、label、display-safe 的 token 指纹/摘要、scope 摘要、状态与时间戳，而不能返回明文 token、原始 `token_hash`、内部 matcher cache、精确 network binding 或等价认证内部字段。普通 model-plane / MCP 请求不得把认证内部 record 作为调试信息回显给模型。

#### 场景:本地管理员查看 token 列表或详情
- **当** 本地受信任 control-plane 请求查看当前 token 列表或某个 token 的详情
- **那么** 系统必须返回 display-safe summary，并且若需要辅助人工识别 token，只能返回 display-safe 的指纹/摘要字段，而不能输出明文 token 或原始 `token_hash`

#### 场景:已认证 agent 请求获取自身认证细节
- **当** 某个已认证 agent 试图通过 model-plane 或普通 MCP tool 查看自身或其他 token 的内部认证记录
- **那么** 系统必须拒绝该请求，或仅返回最小化 principal / scope 摘要，而不能回显认证内部真相

## 移除需求
