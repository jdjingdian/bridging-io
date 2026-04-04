## 新增需求

### 需求:vault 必须支持 `target-profile` 作为 sensitive target 的 authoritative secret kind
BridgingIO 的 canonical vault 必须允许把 sensitive target 保存为专用的 `target-profile` secret kind，而不是继续只保存零散的 overlay 片段或把 sensitive target 真相留在外层 `config.toml`。该 secret 必须至少能够承载 `public_descriptor`、`sensitive_overlay` 与 `public_descriptor_digest`。

#### 场景:保存 sensitive target
- **当** 本地受信任管理面创建或更新一个 sensitive target
- **那么** 系统必须把该 target 的 authoritative `target-profile` 写入 vault
- **并且** 该写入对象必须同时包含 public descriptor 与 sensitive overlay，而不得只保存 digest 或零散敏感字段

#### 场景:升级旧格式 sealed overlay
- **当** 系统在 unlocked 状态下读取到旧格式 sealed overlay，且外层 public descriptor 仍可用
- **那么** 系统必须允许把该对象升级为新的 `target-profile` secret 格式，以便后续 sensitive target 由 vault authoritative truth 驱动

### 需求:受信任本地管理面必须能够以 display-safe 方式发现 `target-profile`
vault 的 secret summary 与本地受信任管理面必须允许以 display-safe 方式发现 `target-profile` 对象，以支持 unlock 后的 sensitive target reconcile。系统禁止把 `target-profile` 的 sensitive overlay 明文直接暴露给普通摘要列表、普通 MCP tool 或非受信任读取路径。

#### 场景:本地管理面列出 target-profile 摘要
- **当** 本地受信任管理面请求列出 vault 中的 secret summary
- **那么** 系统必须允许其中包含 `target-profile` 的 reference、kind、label、status 与等价 display-safe 字段
- **但是** 不得在该摘要列表中返回 connection、notes、credential_ref 或其他 sensitive overlay 内容

#### 场景:普通读取路径请求 target-profile 内部内容
- **当** 普通 MCP tool、普通摘要页或非受信任路径尝试读取某个 `target-profile` 的内部 payload
- **那么** 系统必须拒绝该请求，或仅返回 display-safe 摘要，而不得直接暴露 `public_descriptor` 之外的 sensitive overlay

## 修改需求

### 需求:配置文件中禁止保存敏感明文
系统必须禁止在 standalone 模式使用的配置文件中保存 SSH 私钥、口令、token 或其他可直接用于认证的明文。对于 sensitive target，配置文件还必须禁止持久化 host、username、selector、notes、policy、credential 引用或其他属于 sensitive overlay 的字段；`config.toml` 只允许保存该 target 的 public descriptor cache。

#### 场景:用户手写 standalone 配置文件
- **当** 用户为 standalone core 编写包含 sensitive target 的配置文件
- **那么** 配置文件中必须只出现该 sensitive target 的 public descriptor cache
- **并且** 不得继续把其 host、selector、notes、policy 或 `credential_ref` 作为外层持久化字段写入 config

#### 场景:用户创建 plain target
- **当** 用户创建一个 plain target
- **那么** 系统可以继续把该 target 的连接描述字段保存在 `config.toml`
- **但是** 该 target 仍不得在配置文件中保存私钥、口令、token 或其他直接可用于认证的明文

## 移除需求
