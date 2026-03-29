## 上下文

`design-core-agent-token-management` 已经把 token 的安全边界固定下来：明文 token 只在签发响应中返回一次，后续查询和列表只能返回 display-safe 摘要。`desktop-operator-console` 与 `credential-and-approval-control` 也已经要求桌面控制台通过受信任本地管理面完成长期 token 签发，并且不得把明文 token 混入后续列表。

当前实现的缺口不在 core，而在 bundled webview：

- `create_agent_token` 已经会把 `plaintext_token` 透回 UI
- `Settings > Vault` 当前只显示状态消息，没有正式的结果承接界面
- token 列表已经是安全摘要视图，但成功签发后的“只展示一次”交互没有闭环

因此这次设计聚焦 UI 收口，而不是重新讨论 token 真相模型或本地用户验证链路。

## 目标 / 非目标

**目标：**

- 为长期 token 签发成功路径提供明确的一次性结果展示界面，让用户能在首次响应中看到并复制明文 token。
- 保持 token 列表继续作为 display-safe 摘要视图，不把明文 token 混入列表或持久 UI 状态。
- 让 bundled webview 对“一次性结果”的生命周期有清晰规则，避免刷新、关闭或再次查询时错误地重新展示明文。
- 为这个交互补齐自动化 UI 合同，保证后续实现不会再次退化成“后端有返回、前端没显示”。

**非目标：**

- 本次不修改 `create_agent_token` 的 control-plane 契约，也不新增 token 真相字段。
- 本次不改变 token 的持久化、scope、revoke 或本地用户验证语义。
- 本次不把 token 列表升级为可 reveal 历史明文的管理界面。
- 本次不要求同时改造 macOS SwiftUI 控制台；范围限定在跨平台 bundled webview 基线。

## 决策

### 决策 1：签发成功结果采用 Vault 区域内的专用一次性结果面板，而不是写入 token 列表

token 明文属于“只在本次创建响应中可见”的高敏感结果，不应与长期存在的 token 摘要列表混在一起。首版建议在 `Settings > Vault` 内直接渲染一个专用结果面板，位置紧邻签发动作区域，与 token 列表形成明确分层：

```text
Vault status
Token issue form
Issue action
-------------------------
One-time token result panel
  label
  plaintext token
  copy action
  "closing or refreshing will hide this value" warning
-------------------------
Display-safe token summaries
```

选择结果面板而不是把明文注入列表，有三个原因：

- 它与“列表永远只显示安全摘要”的规则不冲突
- 它不需要把一次性结果塞进长期状态模型
- 相比模态弹窗，内联面板更容易与当前 bundled webview 结构、可访问性和自动化测试集成

考虑过的替代方案：

- **模态弹窗**：能更强烈强调一次性结果，但会引入额外的焦点管理、关闭时机和自动化复杂度；当前 bundled webview 首版不需要先走这条更重的路径。
- **直接写进状态栏**：实现最简单，但已经证明不可用，用户无法稳定复制或核对 token。
- **把明文插入 token 列表首项**：会破坏 display-safe 分层，并让“后续列表不得再次展示明文”变得含混。

### 决策 2：一次性结果只保存在前端内存态，并在显式关闭或状态重建时清除

前端应当把新签发 token 的明文只保存在当前页面内存态，例如 `pendingIssuedTokenResult` 之类的瞬时状态，而不是写入 local storage、URL、列表数据源或其他会穿越会话边界的地方。

清除规则建议为：

- 用户显式关闭结果面板时清除
- 页面重新加载、重新 bootstrap 或离开当前受信任管理会话时清除
- 重新从 `list_agent_tokens` 拉取列表不会重新构造该明文结果

这样可以保持“UI 能承接本次响应”和“系统不会把明文变成持久真相”两者同时成立。

考虑过的替代方案：

- **把一次性结果持久化到浏览器存储**：会把本应一次性展示的高敏感值延长暴露窗口，不符合现有安全边界。
- **每次刷新后根据 token_id 再次请求 reveal**：当前规范明确不支持这类历史明文回读，也会让列表和 reveal 真相混淆。

### 决策 3：Copy 是明确动作，但结果文本在面板存活期间必须保持可见且可手动复制

结果面板必须提供明确的 copy 按钮，但不能把可用性完全押注在 clipboard API 是否成功。首版交互应满足：

- 默认可见明文 token
- 提供明确的 `Copy token` 动作
- copy 成功或失败都给出可见反馈
- 即使 copy 失败，用户仍可以在面板存活期间手动选择并复制文本

这样可以避免 WebView / 平台差异导致“按钮存在但用户仍拿不到 token”。

考虑过的替代方案：

- **只显示 copy 按钮、不显示完整文本**：一旦 clipboard 权限或平台行为异常，用户会直接失去这次一次性结果。
- **把 token 只放在只读 input 中**：可用，但语义过弱，不如专用结果面板更能表达“一次性高敏感结果”的边界。

## 风险 / 权衡

- **[用户可能在未复制前误关结果面板]** → 在面板中明确提示“关闭或刷新后不会再次显示”，并避免自动消失。
- **[不同宿主平台的 clipboard 行为可能不一致]** → 设计上要求 token 文本始终可见且可手动复制，copy 按钮只是主路径而不是唯一出口。
- **[再次签发新 token 时可能覆盖上一次未处理的结果]** → 首版实现应把“同一时间只保留一个待处理结果”作为显式状态规则；若后续发现需要更强保护，可再补充二次确认或禁用连续签发策略。
- **[UI 增加结果面板后，可能让人误解为 token 可以在设置页长期查看]** → 在文案与测试里同时强调：只有创建响应后的临时结果面板显示明文；列表和后续刷新始终只显示安全摘要。

## 迁移计划

1. 为 `desktop-operator-console` 增加 token 一次性结果面板的规范要求。
2. 为 `quality-and-test-automation` 增加对应的 UI 自动化合同，覆盖一次性展示、copy 和关闭后不再复现。
3. 在 bundled webview 中引入一次性结果状态与渲染区域，复用现有 `create_agent_token` 响应，不改 bridge 契约。
4. 为结果面板接入 copy 反馈与关闭逻辑，并验证 token 列表继续只显示 display-safe 摘要。

## 开放问题

- 首版是否需要在结果面板关闭前阻止再次签发新 token，还是允许新结果直接覆盖旧结果？
- copy 优先使用浏览器标准 clipboard API，还是直接收口到 Tauri 宿主能力以降低平台差异？
- 后续 macOS SwiftUI 控制台是否要复用同一套“一次性结果面板”语义，还是只把这次变更视为 bundled webview 基线？
