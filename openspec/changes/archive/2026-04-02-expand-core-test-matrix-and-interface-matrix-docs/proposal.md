## 为什么

当前测试与接口文档已经有 cross-platform contract 和 app-api 说明，但它们还不够形成可维护的 matrix：self-test 没有明确的平台/架构/模式维度，暴露给 UI 或 future operator console 的本地接口也缺少一份可审查、可同步更新的接口矩阵文档。随着项目进入 core-first 与 standalone-first 阶段，这种“只有叙述没有矩阵”的状态会让后续开发难以判断变更影响面。

现在需要把“自测测试用例 matrix”和“暴露给 UI/TUI 的接口 matrix”都正式纳入 OpenSpec 约束，让每次核心变更都能同步更新这些真相文档。

## 变更内容

- 为 core cross-platform contract 和 `--self-test` 建立正式的 matrix 文档，覆盖平台、架构、宿主模式、case id、前置条件与预期结果。
- 为本地 control-plane / operator surfaces 建立正式的接口 matrix 文档，覆盖命令、输入、输出、状态、错误码、apply strategy 与调用方范围。
- 将 matrix 文档更新纳入质量基线，要求新增或修改核心行为时同步更新相应 matrix，而不是只改实现和零散 README。
- 明确平台支持矩阵以 macOS、Linux x86_64、Linux aarch64、Windows 为当前目标，并为 OpenHarmony 保留长期规划占位而不误报为当前已完整实现。

## 功能 (Capabilities)

### 新增功能
- `operator-interface-matrix`: 维护暴露给 UI、TUI、standalone 管理面和受信任本地调用方的接口矩阵文档。

### 修改功能
- `quality-and-test-automation`: core contract/self-test 文档必须扩展到平台、架构、宿主模式与 case matrix 维度，并把 matrix 更新纳入验收要求。

## 影响

- 受影响内容主要包括 `docs/testing`、`source/rust/bridgingio-app-api` 边界文档、`scripts/testing`、OpenSpec 规格与归档验收流程。
- 需要同步调整测试脚本、self-test 文档和后续变更的验收清单。
