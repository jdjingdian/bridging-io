## 1. Bundled Webview Token Result Flow

- [x] 1.1 在 `Settings > Vault` 的 token 签发路径中新增一次性结果状态，并定义其在显式关闭、页面重载和 bootstrap 重建时的清除规则
- [x] 1.2 渲染正式的一次性结果面板，展示明文 token、签发摘要、copy 动作与“关闭或刷新后不会再次显示”的提示
- [x] 1.3 保持 token 列表继续只显示 display-safe 摘要，确保 `loadTokens`、刷新和重新进入设置页都不会再次回显明文 token
- [x] 1.4 为 copy 动作补充成功/失败反馈，并在 clipboard 不可用时保持 token 文本仍然可见且可手动复制

## 2. Verification

- [x] 2.1 为 bundled GUI 增加自动化 UI 覆盖，验证长期 token 签发成功后会出现正式的一次性结果面板与 copy 入口
- [x] 2.2 增加自动化验证，确认关闭结果面板、刷新 token 列表或重新进入设置页后，界面只显示安全摘要而不会再次回显明文 token
