## 为什么

`wire-ui-to-managed-core` 已经打通了 bundled UI 与 managed core 的启动、attach 与首屏读取，但当前 bundled 形态仍然存在三个阻塞产品可用性的缺口：

- UI 在真实运行时下仍然缺少可写控制平面闭环。目标创建、编辑、审批、交互式 shell、工具覆盖与缓存设置等入口仍停留在 fixture 路径，导致用户点击后没有实际效果。
- bundled 启动仍由 UI 临时生成 TOML 并通过 `--config` 拉起 core，配置默认把 model-plane 端口写成 `0`，而 UI 侧又没有 host/port 设置与展示，导致外部 MCP 入口难以发现和使用。
- 当前缺少一个对 bundled 异常路径友好的宿主状态机。无论是首次启动尚未选择 runtime 目录、已保存目录不可用、设置写入后需要重启，还是重启过程中 attach 失败，都还没有被产品化定义。

如果不补上这些边界，UI 虽然已经“连上了真实 core”，但仍然不能可靠承担目标管理与本地控制台的职责。

## 变更内容

- 将 bundled UI-managed 模式的启动语义调整为：UI 只负责选择并持久化 `runtime root`，core 负责在该目录下装载或创建自己的配置、状态与 artifacts；UI 不再负责拼接用户可编辑 TOML。
- 为 bundled 发行物定义首次启动目录选择流程。首次启动时 UI 必须提示用户选择运行数据目录，并在 UI 本地持久化该选择；后续启动时 UI 使用该目录拉起 core。若目录失效、不可访问或被移除，UI 必须进入明确的恢复路径，而不是静默回退到随机临时目录。
- 将 targets、tool override、artifact cache、model-plane host/port 等设置统一收敛到 core-owned settings/profile 接口；UI 的读取与保存都必须通过本地 control-plane IPC，由 core 完成校验、持久化与诊断回显。
- 为 macOS 控制台补充 bundled 设置入口，至少覆盖 model-plane host/port，默认值为 `127.0.0.1:19718`；若当前 UI 中缺失该设置入口，则本次必须补齐。
- 将 bundled 设置生效策略明确为“保存后由 UI 托管重启 core”，而不是在进程内做 reload。core 负责返回该次写入的生效策略与诊断，UI 负责驱动受控重启并呈现状态。
- 为 target/profile 编辑补足真实数据闭环：UI 必须从 core 获取完整 profile 进行编辑，而不是依赖 bootstrap 摘要字段与本地占位值推断。
- 定义 UI-managed host state machine，覆盖 `需要选择目录`、`启动中`、`attach 中`、`已连接`、`保存中`、`等待重启`、`重启中`、`重启失败`、`目录不可用` 等状态，以及相应的恢复入口。

## 功能 (Capabilities)

### 新增功能

无。

### 修改功能

- `target-session-management`: 增加 bundled runtime root 选择与持久化边界、core-owned settings/profile 写入语义，以及“保存后重启”生效策略。
- `macos-operator-console`: 增加 runtime 目录选择、model-plane host/port 设置入口、真实 profile 编辑闭环与 managed restart 状态展示。
- `capability-aware-mcp`: 明确 bundled 模式下 host/port 保存后的重启生效语义，以及默认 `127.0.0.1:19718` 的产品路径。

## 影响

- 受影响的 UI 代码包括 `WorkspaceViewModel`、启动入口、目录选择与持久化、设置页、目标编辑页，以及 managed core 的状态呈现与恢复入口。
- 受影响的 Rust 代码包括 `bridgingio-core` 启动参数、core 配置装载/创建路径、`bridgingio-app-api` 的设置与 profile 写入契约、control-plane handler，以及 settings 持久化与生效策略回包。
- 需要补充 bundled 场景下的自动化验证，覆盖首次选目录、目录失效恢复、目标保存、host/port 修改、重启成功与失败路径。
