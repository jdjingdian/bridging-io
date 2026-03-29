## ADDED Requirements

### 需求:agent token scope 更新必须版本化并保留历史授权真相
当本地受信任管理面为一个已签发 token 调整 target、tool 或其他授权维度时，系统必须通过创建新的 `TokenScopeRecord` 版本并切换 active 指针来生效，而不是直接覆盖旧 scope 记录。旧 scope 版本必须保留为审计真相，但不得继续参与后续授权判定。

#### 场景:为现有 token 增加新的 target 访问范围
- **当** 本地管理员为一个已有 token 新增可访问的 target 集合
- **那么** 系统必须创建新的 scope version，切换该 token 的 active scope 指针，并将旧 scope 保留为 superseded 审计记录，而不能直接修改原 scope 使历史授权不可追溯

#### 场景:scope 更新后后续请求按新版本鉴权
- **当** 某个 token 的 active scope 已从旧版本切换到新版本
- **那么** 后续基于该 token 的 target / tool / risk 授权判定必须使用新的 active scope，而不能继续沿用旧 scope 的权限结果

### 需求:首版 token scope 即使只开放 `target_ids` 也必须保持多维默认拒绝结构
即使首版本地管理面只真正开放 `target_ids` 一类的权限配置，系统仍必须把 token scope 持久化为多维结构，并为未显式开放的维度保留正式字段与默认拒绝语义。系统不得把“当前 UI 没有配置该字段”误解释为该维度自动放行。

#### 场景:首版仅配置 target 范围创建 token
- **当** 本地管理员创建一个 token，并只提供 `target_ids` 而未显式配置 tool、interactive shell 或其他维度
- **那么** 系统必须持久化完整的 scope record，并让未配置维度按默认拒绝或 profile-default 语义收敛，而不是把这些能力隐式视为允许

#### 场景:后续版本扩展更多 scope 维度
- **当** 系统后续为 token 管理面增加 `tool_ids`、risk envelope 或 interactive shell 等配置项
- **那么** 已有 token record 与 scope record 必须能够在不破坏既有持久化语义和 IPC 基本形状的前提下吸收这些字段，而不是要求重新设计 token 真相模型
