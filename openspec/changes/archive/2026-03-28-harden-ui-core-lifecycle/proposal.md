## 为什么

当前 bundled `ui-managed-ephemeral` 模式把 UI 与 core 定义为强绑定生命周期，但在 macOS 真实运行中，UI 主动退出、crash 退出或快速重启后仍可能遗留 orphan core 进程。残留实例会继续占用 model-plane 监听地址或本地 control-plane 资源，导致下一次 UI 启动时无法可靠拉起新的 core，也无法判断应该回收旧实例还是接管它。

如果不把这部分生命周期治理显式产品化，后续无论是继续维持默认的 ephemeral 模式，还是按配置引入后台持续运行模式，都会把“发现旧实例、判断所有权、回收残留进程、恢复连接”这些复杂度扩散到每个平台 UI 中，破坏跨平台前端的可复用边界。

## 变更内容

- 为 bundled UI/core 关系补充显式的 host lifecycle state machine，覆盖启动前发现旧实例、探测实例健康、回收 orphan core、等待端口与 endpoint 释放、attach 成功、正常退出、异常恢复与冲突处理。
- 将“UI 绑定型 ephemeral 模式”与“后台持续运行型 persistent 模式”在设计上明确分层：前者默认要求 UI 退出后 core 不应继续存活，残留实例应被视为异常并优先回收；后者预留为未来可配置宿主模式，由稳定宿主负责持续运行与重连。
- 明确 `ui-managed-ephemeral` 模式下的启动前 reconcile 语义：UI 在拉起新 core 之前必须先发现并探测同一 runtime root 对应的已有实例；若实例是 stale/orphan，则必须先回收并确认监听资源释放，再进入新的 spawn 流程。
- 明确正常退出路径不应停留在“发出 shutdown 请求”这一层，而必须具备等待 core 真正退出、确认 endpoint 消失、确认 model-plane 监听地址释放的宿主治理语义。
- 为未来 persistent 模式预留 attach ownership 语义，包括稳定实例发现、owner 身份、detach、re-attach 与 takeover 的协议方向，但本次不要求实现完整后台服务化。
- 为 macOS 控制台补充与生命周期治理对应的正式状态与恢复入口，例如“发现残留实例”“正在回收旧 core”“等待监听地址释放”“发现活动实例冲突”“重试/强制回收”等，而不是只展示泛化的 attach failed。

## 功能 (Capabilities)

### 新增功能

无。

### 修改功能

- `target-session-management`: 增加 bundled host lifecycle state machine、orphan reconcile、启动前实例探测、退出确认、ephemeral/persistent 宿主模式分层，以及未来 attach ownership 的协议边界。
- `macos-operator-console`: 增加与生命周期治理一致的连接状态、残留实例恢复入口、冲突提示与回收过程可视化要求。

## 影响

- 受影响的 Rust 边界包括 `bridgingio-core` 启动/退出宿主语义、app API 的实例探测与 ownership 协议、runtime metadata 暴露，以及本地 control-plane/model-plane 的生命周期约束。
- 受影响的 macOS UI 边界包括 app termination 协调、core host 启动与关闭流程、错误恢复 UX、状态机呈现和日志/诊断入口。
- 需要补充针对 orphan core、端口占用、快速重启、正常退出等待与冲突恢复的契约测试或宿主级验证。
- 该变更会为未来 Linux/Windows/WebView/Flutter/Tauri 等平台宿主适配器提供统一生命周期边界，避免让每个前端直接处理底层 IPC 与残留实例治理。
