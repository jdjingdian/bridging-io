## 上下文

`wire-ui-to-managed-core` 解决了 bundled UI 与 managed core 的“能启动、能 attach、能读快照”问题，但后验收暴露出新的结构性缺口：

- UI 的大量写操作仍然只在 fixture data source 下有效，真实 managed core 路径没有把用户动作送到 control-plane。
- bootstrap 只返回 target 摘要，UI 端编辑器所需的连接参数、别名、凭据引用等字段仍由本地占位值填充，导致“编辑真实目标”无法成立。
- bundled 启动仍由 UI 临时生成 TOML，默认 `model_plane.http.port = 0`，同时 UI 中没有 host/port 设置界面，因此用户既看不到当前监听地址，也无法通过产品路径修改它。
- core 当前虽然有 `settings_view` 一类只读接口，但还没有一条可稳定复用的“读取完整设置 -> 校验写入 -> 落盘 -> 告知是否需重启”的闭环。
- bundled 异常路径尚未产品化。例如首次启动没有 runtime 目录、目录失效、保存后重启失败、attach 失败、core 启动后 bootstrap 失败等，都缺少正式状态与恢复入口。

本次设计的目标不是继续把 UI 变成“代写配置文件的薄壳”，而是把 bundled 模式的宿主边界理顺：

- UI 只负责启动前必须知道的宿主信息；
- core 继续拥有所有业务设置与配置真相；
- 设置变更的生效路径采用“保存后重启 core”，而不是进程内 reload；
- 异常路径通过一个明确的宿主状态机管理，而不是靠散落在视图层的条件分支。

## 目标 / 非目标

**目标：**

- 让 bundled UI-managed 模式不再依赖 UI 代写用户可编辑 TOML，而是改为 UI 传递 `runtime root`、core 自己装载或创建配置。
- 让 targets、tool override、artifact cache、model-plane host/port 等设置都通过 core-owned IPC 完成读取、校验与落盘。
- 为 bundled UI 增加首启目录选择与本地持久化能力，使用户能明确决定 runtime 数据保存位置。
- 为 macOS 控制台补齐 model-plane host/port 设置入口，默认值使用 `127.0.0.1:19718`。
- 定义一套 UI-managed host state machine，覆盖启动、attach、保存、重启与异常恢复。
- 把“需要重启才能生效”的语义显式纳入 app API，而不是让 UI 猜测。

**非目标：**

- 本次不实现进程内 reload，也不要求 core 在监听地址变化时做无缝热切换。
- 本次不把 runtime root 目录选择变成 core-owned 业务设置；该选择发生在 core 启动之前，属于 UI host adapter 的启动偏好。
- 本次不直接实现后台服务模式、自动重启守护或 orphan 清理增强。
- 本次不引入跨进程事务回滚系统；优先保证“写入先校验，重启失败可恢复”。

## 决策

### 决策 1：UI 只拥有 `runtime root` 启动偏好，业务配置仍归 core 所有

bundled UI-managed 模式下，UI 必须在 core 启动前知道“去哪一个目录找或创建 runtime”，因此 `runtime root` 的首次选择与本地持久化应由 UI host adapter 持有。这是一个启动偏好，而不是 target/profile/model-plane/artifact cache 一类业务设置。

UI 需要持久化的最小信息：

- 用户选择的 `runtime root` 路径；
- 访问该路径所需的本地平台凭据或 bookmark（若平台需要）；
- 最近一次选择是否有效的轻量诊断缓存。

这些信息不得替代 core 的业务配置真相。core 启动后，所有业务设置读取与修改都必须走 control-plane。

选择该方案的原因：

- 在 core 尚未运行之前，只有 UI 能发起目录选择与本地平台权限交互。
- 可以把“启动前偏好”与“启动后业务设置”清晰分层，避免 UI 同时保存两套配置真相。
- 后续若宿主从 SwiftUI 换成其他平台壳，依旧只需要替换 host adapter 的启动偏好层。

### 决策 2：bundled core 改为从 `runtime root` 执行 `load-or-create`，不再要求 UI 代写 TOML

bundled 模式下推荐新的启动契约：

```text
UI host adapter
  -> resolve runtime root
  -> start bridgingio-core ui-managed-ephemeral --runtime-root <dir>
  -> core loads existing config or creates default config in <dir>
  -> UI attach / bootstrap
```

推荐的目录布局：

```text
<runtime-root>/
├── config/
│   └── managed-core.toml
├── state/
│   └── metadata.sqlite3
├── artifacts/
└── logs/
```

其中：

- `config/managed-core.toml` 由 core 创建、更新与持久化；
- `state/`、`artifacts/`、`logs/` 也由 core 相对该 root 管理；
- UI 不再负责根据表单把 TOML 模板拼出来。

首次创建时，bundled 默认配置必须采用：

- `model_plane.http.host = "127.0.0.1"`
- `model_plane.http.port = 19718`

而不是随机端口 `0`。这样用户在默认路径下既能从 UI 看到地址，也能从外部 MCP 客户端按稳定默认值接入。

选择该方案的原因：

- 避免 UI 与 core 分别维护 TOML 模板而产生漂移。
- 让 bundled 与 standalone 最终共享同一份 core-owned 配置模型，只是启动入口不同。
- 为后续“设置保存 -> core 落盘 -> 下次启动继续生效”建立稳定文件位置。

### 决策 3：引入 core-owned settings/profile 写接口，并让响应显式返回生效策略

本次需要把“只读 bootstrap/settings”扩展为“完整读写契约”。最小接口建议包括：

- `GetBootstrapState`: 继续提供首屏摘要，但补充 setting/profile 读取入口引用。
- `GetProfile` / `ListProfilesDetailed`: 返回可用于编辑器的完整 profile，而不是摘要。
- `UpsertProfile`: 接收完整 profile，完成校验与持久化。
- `GetSettings`: 返回完整 settings 视图。
- `UpdateSettings`: 接收结构化设置变更，完成校验与持久化。
- `ClearArtifactCache` 或等价操作接口。

所有写接口都必须返回明确的 `apply_strategy`，例如：

- `live_applied`
- `restart_required`

对当前 MVP，建议采用以下策略：

- target/profile 的新增、编辑、删除：优先 `live_applied`
- model-plane host/port 与其他监听相关设置：`restart_required`
- artifact cache backend/root/max/eviction：允许由 core 决定是否 `restart_required`
- tool override：允许由 core 决定是否 `live_applied` 或 `restart_required`

UI 不得自行推断某项设置是否需要重启，必须以 core 回包为准。

选择该方案的原因：

- 把“校验规则”和“是否需重启”留在 core 这一真相源中，避免 UI 写死条件分支。
- 后续即使某类设置从“需重启”演进为“可热生效”，也只需更新 core 判定逻辑。

### 决策 4：target 编辑必须基于真实完整 profile，而不是 bootstrap 摘要

bootstrap 继续承担“首屏尽快可见”的职责，但 target 编辑器必须在打开时读取完整 profile。UI 不得再用摘要字段拼出一个带占位连接参数的“伪真实对象”。

推荐交互：

1. 列表页使用 bootstrap 摘要展示 target rows。
2. 用户点击编辑时，UI 调用 `GetProfile(target_id)`。
3. core 返回完整 profile。
4. UI 以该 profile 初始化编辑器。
5. 保存时通过 `UpsertProfile` 回写 core。

新建 target 时：

1. UI 创建空 draft；
2. 保存时通过 `UpsertProfile` 发送完整结构化 profile；
3. core 负责生成或确认 id、校验字段并持久化。

选择该方案的原因：

- 编辑器的字段集合比列表摘要丰富得多，二者不应共用同一份轻量载荷。
- 避免 UI 因占位值而覆盖真实配置。

### 决策 5：bundled 设置变更统一采用“保存后由 UI 托管重启 core”，不做 reload

对于需要重新绑定 listener、重建 store 或重建 runtime 的设置，本次不实现进程内 reload。标准路径是：

1. UI 把设置变更发送给当前 core。
2. core 完成校验并写入稳定配置。
3. core 在响应中返回 `restart_required`。
4. UI-managed host 进入重启状态机，优雅关闭当前 core。
5. UI 用同一个 `runtime root` 拉起新 core、重新 attach、重新 bootstrap。

之所以不做 reload：

- 当前 HTTP listener 与 runtime 初始化路径都偏向启动时一次性构造。
- reload 会引入半生效、中间态回滚、跨组件重建顺序等额外复杂度。
- 对 bundled MVP 来说，受控重启更容易验证和恢复。

### 决策 6：为 bundled host 定义显式状态机，而不是散落的布尔值

推荐状态机：

```text
needs_runtime_root
    │ choose / restore
    ▼
starting_core
    │ core spawned
    ▼
attaching_ui
    │ attach ok
    ▼
bootstrapping
    │ snapshot ok
    ▼
connected
    │ save settings/profile
    ├──────────────▶ saving_changes
    │                   │ live_applied
    │                   └────────────▶ connected
    │
    └──────────────▶ restart_required
                            │ confirm / auto-continue
                            ▼
                         restarting_core
                            │ success
                            ├────────────▶ attaching_ui
                            │
                            └────────────▶ restart_failed
```

异常状态与恢复要求：

- `runtime_root_unavailable`
  - 已保存目录不存在、不可写或权限失效时进入
  - UI 必须提供“重新选择目录”入口
- `start_failed`
  - core 进程无法启动或 socket 未就绪
  - UI 必须提供“重试启动”“查看日志”“重新选择目录”入口
- `attach_failed`
  - core 已启动但 attach/bootstrapping 失败
  - UI 必须允许重试 attach，必要时执行完整重启
- `restart_failed`
  - 设置已通过校验并落盘，但新 core 未能成功 attach
  - UI 必须保留失败原因、日志入口与重试按钮

行为约束：

- `saving_changes`、`restart_required`、`restarting_core` 期间，新的写操作必须禁用或排队，避免并发写入不同版本配置。
- 如果设置校验失败，UI 必须留在 `connected`，显示来自 core 的字段错误，不进入重启流程。
- 如果重启失败，UI 不得悄悄回退到 fixture 或随机临时目录。

### 决策 7：settings 页必须补齐 model-plane host/port，并展示默认值与生效方式

macOS 设置页必须至少新增：

- model-plane host 输入框
- model-plane port 输入框
- 默认值说明：`127.0.0.1:19718`
- 生效提示：保存后由 UI 托管重启 core

若未来还需要 expose `allow_non_loopback`、`auth_mode` 等高级项，可继续扩展；但本次至少要补齐 host/port，避免当前 bundled 版本出现“监听地址不可见、不可改、默认又是随机端口”的不可用状态。

## 数据流草图

```text
首次启动
────────
UI -> ask runtime root
UI -> persist runtime root locally
UI -> spawn core with runtime root
core -> load/create config under runtime root
UI -> attach / bootstrap

保存 target
───────────
UI editor -> GetProfile / draft
UI -> UpsertProfile
core -> validate + persist
core -> apply_strategy=live_applied
UI -> refresh snapshot

保存 host/port
──────────────
UI settings -> UpdateSettings
core -> validate + persist
core -> apply_strategy=restart_required
UI host -> graceful shutdown old core
UI host -> start new core with same runtime root
UI -> attach / bootstrap
```

## 风险与缓解

- **风险：重启前写入成功，重启后 attach 失败**
  - 缓解：写入前做完整校验；失败时提供明确 `restart_failed` 状态、日志查看与重试入口。
- **风险：用户已保存的 runtime root 被移动或权限失效**
  - 缓解：在启动前做目录可用性检查，失败时进入 `runtime_root_unavailable`，要求重新选择。
- **风险：UI 仍然只拿到摘要导致编辑覆盖真实值**
  - 缓解：将“编辑前必须读取完整 profile”写成正式需求和任务。

## 迁移说明

- 若用户此前已经在默认应用支持目录下运行 bundled 版本，可以将该默认目录视作一次隐式初始 `runtime root`，避免升级后强制迁移。
- 旧的“UI 临时写 TOML + 随机端口”启动路径在本次完成后应视为 deprecated，不再作为 bundled 默认实现保留。
