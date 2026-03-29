## 为什么

BridgingIO 已经在主规范中明确了 model-plane bearer token 的长期方向，但当前仓库还没有把“本地如何签发 / 列表 / 撤销 token”这条管理链路落成一个稳定的 core 真相源。现在直接开始做 token generate，如果没有先把管理面和 record 模型收敛好，会马上遇到几个问题：

- 本地受信任 control-plane 目前没有 token 管理命令，UI 无法通过 IPC 调用稳定的 core 接口完成签发
- 当前讨论中的“生成后写入 vault”会把本来只需一次性显示的 access token 重新建模成可 reveal 的 secret，不符合 hash-only token 真相
- token 需要支持长期或时效两种生命周期，但“删除 token”在安全语义上更接近 revoke，而不是立即物理删除
- 用户已经明确希望 token 后续可以限制可访问的 target，这意味着 scope 不能再被设计成一次性布尔开关，而要预留可演进、可更新的结构
- 当前运行时仍大量把 `agent_id` 视作调用身份标签；如果 token record 不先把 principal 和 label 分开，后续 authn/authz 接入会很难收敛

这次变更的目标不是改 UI，而是先把 core 侧 token generate 的真实边界、IPC 形状、scope 模型与后续 standalone 路线固定下来，避免后面 UI、core、HTTP auth 和 standalone 再各自发散。

## 变更内容

- 为本地受信任 control-plane 定义 agent token 管理接口，至少覆盖 `create`、`list`、`revoke`，并预留 `update-scope`
- 固化 token 持久化模型：明文 token 只在签发响应中返回一次，core 长期只保存 `hash + metadata + scope`
- 固化 token 生命周期：支持长期 token 与时效 token；用户侧“删除”动作在 core 中统一建模为 `revoke`
- 固化 token identity 语义：UI 只提供用户可见 label，core 自行分配稳定的 `token_id` 与 `principal_id`
- 固化 scope 模型：首版至少允许配置 `target_ids`，同时保留 `tool_ids`、risk envelope、interactive shell 等多维字段，未显式配置维度必须默认拒绝
- 固化 scope 更新语义：后续允许通过本地受信任管理面更新 scope，但更新必须以 scope version 切换而不是覆盖旧真相
- 为 standalone 记录 future route：未来 CLI 或 headless 管理入口复用同一套 token admin service，本次只预留接口和约束，不实现 standalone 管理命令

## 功能 (Capabilities)

### 修改功能

- `capability-aware-mcp`: 增加 token scope version、target-scoped 初始授权、以及 label 与 authenticated principal 分离的管理语义
- `target-session-management`: 增加本地 control-plane token 管理接口与“一次性明文返回 + 后续安全摘要”语义

### 新增功能

无。

## 影响

- Rust 核心主要影响 `source/rust/bridgingio-app-api`、`source/rust/bridgingio-mcp`、`source/rust/bridgingio-secrets`
- 本次不包含 UI 侧变更，但 UI 后续可以直接基于本次 IPC 契约接入“输入备注名称 -> 生成 token -> 只显示一次”的流程
- 本次不实现 standalone CLI，但后续 standalone 需要复用同一套 token admin service，而不是重新设计另一套明文参数路线
- token 生成与 scope 记录落地后，后续 model-plane bearer auth 才能稳定接入“principal 从 token 派生，request context 只做标签”这条链路
