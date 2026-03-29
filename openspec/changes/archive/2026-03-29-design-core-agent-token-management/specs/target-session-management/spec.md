## ADDED Requirements

### 需求:本地 control-plane 必须提供 agent token 管理接口
受信任的本地 control-plane app API 必须为 bundled UI 与 future standalone 管理入口提供统一的 agent token 管理能力，至少覆盖 `create`、`list` 与 `revoke`。这些接口必须由 core 直接持有其生命周期与持久化真相，而不是要求 UI 自行生成、缓存或解释 token。

#### 场景:UI 通过 IPC 创建 token
- **当** 本地 UI 通过受信任 IPC 请求 core 创建一个 agent token，并提交备注名称与生命周期配置
- **那么** core 必须自行生成 opaque token、分配稳定的 `token_id` / `principal_id`、持久化 hash-only record，并返回可供 UI 展示的一次性签发结果

#### 场景:本地管理面列出现有 token
- **当** 本地 UI 或 future standalone 管理入口通过 control-plane 请求列出当前 token
- **那么** core 必须返回 token 的安全摘要列表，而不能要求前端自行拼装 token 状态或从其他接口侧推导生命周期

#### 场景:用户删除 token
- **当** 本地 UI 或 future standalone 管理入口请求删除一个 token
- **那么** core 必须将该动作建模为 revoke，并返回更新后的安全摘要或等价状态结果，而不是把该 token 作为物理记录直接静默删除

### 需求:本地 token 签发响应必须一次性返回明文，后续查询只返回安全摘要
通过本地受信任 control-plane 创建 token 时，系统必须只在签发成功的那一次响应中返回明文 token。后续的 `list`、`get`、诊断或等价管理查询都只能返回安全摘要，而不得再次展示明文 token、`token_hash` 或等价内部认证真相。

#### 场景:首次签发 token 后立即返回明文
- **当** 本地管理面成功创建一个新的 agent token
- **那么** 系统必须在该次 create 响应中一次性返回明文 token 与安全摘要，使调用方能够提示用户立刻保存，而不是要求后续再调用 reveal 接口取回明文

#### 场景:稍后再次查看同一个 token
- **当** 本地管理面在 token 创建完成后再次请求查看该 token 的状态
- **那么** 系统必须只返回诸如 `token_id`、label、status、scope 摘要、创建时间和过期时间这类 display-safe 字段，而不能再次返回明文 token 或内部 hash 字段

### 需求:本地 control-plane token 管理接口必须为后续 scope 更新保持兼容扩展位
即使首版只实现 `create`、`list` 与 `revoke`，本地 control-plane token 管理接口仍必须为后续 `update-scope` 或等价权限管理动作预留兼容扩展位，避免未来新增 scope 更新能力时破坏已发布的 IPC 契约。

#### 场景:后续版本增加 scope 更新命令
- **当** 后续版本为本地 token 管理面增加 `update-scope` 或等价权限更新能力
- **那么** 系统必须能够在不推翻既有 create/list/revoke 基本响应模型的前提下扩展该能力，而不是要求 bundled UI 与 future standalone 管理入口全部重做 token 管理协议
