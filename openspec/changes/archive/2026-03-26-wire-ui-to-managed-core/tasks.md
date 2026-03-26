## 1. Core 宿主与生命周期

- [x] 1.1 为 `bridgingio-core` 引入清晰的宿主/运行模式语义，区分 bundled 的 `ui-managed-ephemeral`、standalone 前台 `run` 与 standalone 后台 `-d`
- [x] 1.2 实现 bundled 模式下的两阶段启动状态，使 core 先开放 control-plane，再等待 UI attach 后进入 ready
- [x] 1.3 在 bundled 模式中实现 UI 正常退出时的 core 关闭路径，并确保该路径不影响 standalone 模式

## 2. Control-plane 与 App API

- [x] 2.1 扩展 `bridgingio-app-api`，增加 UI attach / bootstrap 所需的命令、响应与错误语义
- [x] 2.2 为控制台补齐真实状态读取面，至少覆盖 targets、sessions、approvals、settings、diagnostics、timeline、artifacts 与 interactive shell transcript
- [x] 2.3 为 bundled 模式的 `/mcp` 与等价 model-plane 入口实现 attach 前 not-ready 或等价不可用语义

## 3. SwiftUI 控制台接线

- [x] 3.1 将 macOS SwiftUI app 的默认数据源切换为本地 control-plane，并补齐 loading、empty、attach failed 与 connected 状态
- [x] 3.2 移除产品运行时默认 seed 数据，把 fixture 迁移为测试与 Preview 专用注入路径
- [x] 3.3 将目标列表、会话摘要、时间线、artifact、审批和 transcript 视图改为消费 core 真实数据与增量刷新结果

## 4. 验证与文档

- [x] 4.1 为 Rust 侧补充自动化测试，验证 `run`/`-d` 语义、bundled attach gating 与 attach 前 MCP 不可用行为
- [x] 4.2 更新 SwiftUI 单元测试与 UI Test，使其覆盖真实空态、启动中、attach 失败和成功连接 core 的关键路径
- [x] 4.3 更新开发与运行文档，说明 bundled 默认生命周期、standalone 启动方式，以及未来系统服务宿主扩展位
