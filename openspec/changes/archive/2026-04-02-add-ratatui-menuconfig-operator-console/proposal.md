## 为什么

短期内 BridgingIO 不再继续推进和修补现有 UI 侧功能，而是全力开发 core 能力。但如果 core 仍只有配置文件与零散 CLI 路线，后续无论继续做 GUI 还是排查问题，都很难清晰区分“core 配置/状态问题”和“前端交互问题”的边界。

现在需要一个 core-first 的正式操作面，让 `bridgingio-core menuconfig` 通过 `ratatui` 提供类似 Linux kernel `menuconfig` 的配置与诊断体验，并且直接复用 core 的 schema、字段说明和校验语义，而不是再演化出一套脱节的 TUI 私有配置层。这个入口是通用配置模式，不绑定 standalone；未来 UI 也可以复用同一套配置文件和配置语义。

## 变更内容

- 新增基于 `ratatui` 的 `bridgingio-core menuconfig` 操作面，作为 core 的通用配置入口和诊断入口。
- 将 menuconfig 风格的树形导航、搜索、帮助、dirty tracking、保存/应用语义纳入正式产品范围，并尽可能对齐 Linux kernel `menuconfig` 的交互模式。
- 要求该 TUI 直接消费 core-owned 的字段描述、状态投影与校验真相，而不是自行维护平行 schema。
- 要求 TUI 中涉及 vault、token、runtime root 与目标配置的展示都使用 display-safe 投影，不暴露 secret 明文。
- `vault ...` / `auth ...` 现有 CLI 管理子命令本轮继续保留为兼容入口，不在本变更中删除。

## 功能 (Capabilities)

### 新增功能
- `standalone-operator-console`: 基于 `ratatui` 的 `bridgingio-core menuconfig` 通用操作面，覆盖配置浏览、搜索、帮助、保存/应用、诊断与状态投影。

### 修改功能
- `target-session-management`: standalone 模式新增 core-owned 的交互式配置入口，并定义其与配置落盘、应用策略和生命周期状态的关系。

## 影响

- 受影响代码主要包括新的 Rust TUI 模块或 crate、`source/rust/bridgingio-engine` 的字段描述与配置写回能力、`source/rust/bridgingio-mcp` 的 `menuconfig` 命令入口与 apply 语义，以及相关文档与测试。
- 后续前端实现需要把该 TUI 视作 core-first 通用配置入口，而不是 standalone 专属临时工具。
