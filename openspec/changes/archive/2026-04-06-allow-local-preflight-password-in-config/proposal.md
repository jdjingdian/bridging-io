## 为什么

`add-optional-local-cross-platform-preflight` 落地后，本地 preflight 在未配置 `identity_file` 时会走 SSH/SCP 交互密码提示。该行为在人工单次运行时可接受，但在开发者希望“半自动化高频自测”时会导致流程卡在密码输入，降低本地 preflight 的可用性与使用频率。

当前能力定位是开发者本地辅助工具，而不是生产运行时或正式 gate。只要密码仍然限制在 gitignored 本地配置中，并且不进入仓库跟踪内容、日志与运行摘要，就可以接受“本地落盘密码”这一权衡，以换取稳定的无人值守预检体验。

## 变更内容

- 在本地 preflight 配置模型中引入可选密码字段（仅限 gitignored 本地配置），用于 SSH/SCP 非交互认证。
- 明确认证优先级与兼容语义：优先 `identity_file`，未配置 key 时可使用 `password`，两者都未配置时保持现有交互提示。
- 明确安全边界：密码值不得进入 example 配置、仓库跟踪文件、终端回显、运行日志或 summary。
- 在 `docs/testing/LOCAL_CROSS_PLATFORM_PREFLIGHT.md` 增补密码字段说明、风险提示与推荐实践（优先 key，密码仅用于本地开发便利）。
- 为 preflight runner 增补覆盖：无密码交互、密码非交互、key 优先路径以及日志脱敏回归。

## 功能 (Capabilities)

### 新增功能

- 无

### 修改功能

- `developer-local-preflight`: 扩展本地 preflight 认证模型，允许在 gitignored 本地配置中声明可选密码，实现 SSH/SCP 非交互运行，同时保持配置隔离与日志安全边界。

## 影响

- 受影响代码：`scripts/testing/run-local-cross-platform-preflight.py`、`scripts/testing/local-preflight.example.toml`
- 受影响文档：`docs/testing/LOCAL_CROSS_PLATFORM_PREFLIGHT.md`
- 受影响配置：`tmp/local-preflight.toml`（本地 gitignored）
- 明确保持不变：正式 CI gate、`bridgingio-core --self-test`、contract matrix 语义
