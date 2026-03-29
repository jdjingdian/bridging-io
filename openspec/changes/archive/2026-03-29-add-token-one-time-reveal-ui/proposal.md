## 为什么

当前桌面控制台已经能够通过受信任本地 control-plane 成功签发长期 agent token，并且 core 也会在创建响应中一次性返回明文 token。但 bundled webview 目前只显示一条状态提示，没有正式的结果面板或弹窗承接这次一次性响应，导致用户无法直接复制或验证新 token 是否可用。

这个缺口现在已经影响 token 管理流的可用性：系统在安全模型上要求“明文只返回一次、后续列表只显示安全摘要”，但 UI 还没有把这条交互闭环补齐，结果既没有再次展示明文，也没有在首次响应时提供可操作的 reveal / copy 入口。

## 变更内容

- 为桌面控制台的 token 签发成功路径增加正式的一次性结果展示界面，可以是面板、内联结果区或模态弹窗，但必须明确承接本次签发响应中的明文 token。
- 在一次性结果展示界面中提供明确的 copy 动作，并向用户说明该明文 token 不会在后续列表或刷新后再次展示。
- 保持现有 token 列表为 display-safe 摘要视图，只展示 label、状态、scope 摘要、时间戳与 revoke 等安全信息，而不把新签发的明文 token 混入列表。
- 为签发成功后的 UI 状态切换建立清晰语义，包括成功反馈、关闭或离开结果面板后的行为、以及在重新加载列表时继续只显示安全摘要。
- 为 bundled webview 的 token 签发结果流补充 UI 合同与自动化验证，覆盖“一次性展示”“可 copy”“关闭后不再重复显示”“列表保持安全摘要”这些关键行为。

## 功能 (Capabilities)

### 新增功能

无。

### 修改功能

- `desktop-operator-console`: 增加 token 签发成功后的一次性结果展示与 copy 交互要求，并明确该结果与 token 安全摘要列表之间的分层关系。
- `quality-and-test-automation`: 增加桌面控制台 token 签发结果流的自动化 UI 覆盖，验证一次性展示、copy 入口和结果关闭后的 display-safe 行为。

## 影响

- 前端主要影响 `source/ui/tauri-console-web/index.html` 的 `Settings > Vault` token 签发交互，以及可能新增的结果面板 / 弹窗状态管理。
- 桌面宿主与 bridge 契约原则上无需新增 token 真相字段；本次复用现有 `create_agent_token` 的一次性 `plaintext_token` 响应。
- 相关 UI 合同与自动化验证需要补充，至少覆盖签发成功后结果可见、copy 可用、关闭后列表仍只显示安全摘要。
- 本次不改变 token 的持久化、scope、revoke 或本地用户验证语义；这些现有安全边界继续保持不变。
