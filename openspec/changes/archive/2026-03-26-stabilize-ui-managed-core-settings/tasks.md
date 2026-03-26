## 1. Bundled 启动与配置归属

- [x] 1.1 为 bundled `ui-managed-ephemeral` 模式引入 `runtime root` 启动契约，使 core 可以在该目录下 `load-or-create` 稳定配置，而不是继续依赖 UI 代写临时 TOML
- [x] 1.2 定义 bundled 默认配置初始化规则，至少保证 model-plane 默认监听 `127.0.0.1:19718`
- [x] 1.3 为 bundled runtime root 增加目录布局、日志位置与目录失效诊断语义

## 2. App API 与 Core-Owned 设置闭环

- [x] 2.1 扩展 `bridgingio-app-api`，增加完整 profile 读取/写入与设置更新接口，而不是只返回 bootstrap 摘要
- [x] 2.2 让 core 在 settings/profile 写入后执行校验与落盘，并在响应中返回 `live_applied` 或 `restart_required` 等生效策略
- [x] 2.3 为 bootstrap 或按需读取补齐 host/port、完整 profile 与相关诊断字段，确保 UI 不再依赖占位值编辑真实目标

## 3. SwiftUI 宿主与控制台体验

- [x] 3.1 为 macOS UI 增加首次启动目录选择流程，并在本地持久化 `runtime root`
- [x] 3.2 在设置页中增加 model-plane host/port 设置入口，并展示默认值与“保存后将重启 core”的反馈
- [x] 3.3 将目标创建、编辑、工具覆盖、缓存设置等写操作改为调用 core IPC，而不是只在 fixture 路径下生效
- [x] 3.4 为 UI-managed host 建立正式状态机，覆盖 `needs_runtime_root`、`starting_core`、`attaching_ui`、`connected`、`saving_changes`、`restart_required`、`restarting_core`、`restart_failed`、`runtime_root_unavailable`

## 4. 验证与文档

- [x] 4.1 为 Rust 侧补充自动化测试，覆盖 bundled `load-or-create`、settings 持久化、`restart_required` 响应与 host/port 默认值
- [x] 4.2 为 SwiftUI 侧补充测试，覆盖首次选目录、目录不可用恢复、目标保存、host/port 修改后受控重启与重启失败提示
- [x] 4.3 更新开发与运行文档，说明 bundled runtime root、core-owned settings 边界，以及“保存后重启 core”的产品语义
