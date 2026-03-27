## 1. 配置模型与持久化

- [x] 1.1 扩展 `bridgingio-engine` 配置模型与 TOML 读写，支持 `targets.toolchains.<name>.path_override`，并保持现有全局 `toolchains.<name>.path_override` 不回归
- [x] 1.2 扩展 `bridgingio-domain` / `bridgingio-app-api` / `bridgingio-mcp` 的 profile/settings 数据结构与编解码，确保 target 级和全局级 toolchain override 都能 round-trip
- [x] 1.3 更新 `GetProfile`、`UpsertProfile`、`GetSettings`、`UpdateSettings` 和 bootstrap/diagnostics 载荷，使 UI 不再依赖默认假值推断 tool override

## 2. 运行时解析与诊断

- [x] 2.1 实现分层解析顺序 `target -> global -> system PATH -> built-in fallback`
- [x] 2.2 扩展 diagnostics 回包，至少包含 `target_override_path`、`global_override_path`、`effective_path`、`effective_source` 与 `effective_scope`
- [x] 2.3 确保 target/global override 写入后，diagnostics 与后续新会话可以真实命中最新配置；若某路径暂时无法热生效，必须显式返回 `restart_required`
- [x] 2.4 确保 `terminal.exec`、`inspect_basic` 与 `interactive shell` 在 SSH/ADB target 下都使用同一套有效工具解析结果；当 ADB 同时携带 serial 与 transport 提示时必须优先 serial 选择器

## 3. macOS UI 分层调整

- [x] 3.1 调整目标编辑页的工具来源区块，只显示当前 target 相关工具，并将输入绑定到 target 级 override
- [x] 3.2 在系统设置页新增全局 Tool Sources 区块，与 model-plane host/port 同级，用于编辑全局 `toolchains.<name>.path_override`
- [x] 3.3 将工作台 target 详情中的工具来源卡片调整为诊断展示，不再承担全局 override 的直接写入口

## 4. 自动化验证

- [x] 4.1 为 Rust 侧补充测试，覆盖 target/global toolchain override 的 TOML round-trip、profile/settings round-trip 与分层回退顺序
- [x] 4.2 为 `bridgingio-mcp` 补充运行时测试，覆盖 target override 覆盖 global、target 清空后回退 global、global 清空后回退 PATH/built-in
- [x] 4.3 为 SwiftUI 侧补充测试，覆盖 SSH/ADB target 编辑页的可见性规则、系统设置页的全局 override 入口，以及保存后新的诊断回显
- [x] 4.4 为 MCP 的 one-shot exec / interactive shell 补充测试，覆盖 SSH/ADB override 命中真实执行路径，以及 ADB serial 优先于 `-d`/`-e` 的命令拼接
