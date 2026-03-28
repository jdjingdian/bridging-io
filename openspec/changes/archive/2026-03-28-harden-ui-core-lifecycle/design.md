## 上下文

当前 bundled `ui-managed-ephemeral` 路径已经具备“UI 拉起 core、attach、读取 bootstrap”的基础能力，但生命周期治理仍停留在乐观假设上：

- macOS UI 当前主要依赖 `WorkspaceViewModel.deinit` 触发 `request_shutdown` 与本地 `terminate()`，缺少更上层的 app termination 协调。
- 现有 attach 是一次性握手，不是持久 owner 通道；core 无法从协议层感知“UI 已 crash/被强杀/已失去宿主”。
- 当前 macOS host 为 bundled 启动传入 `--control-plane-socket-override`，并把 control-plane endpoint 放到按 UI 启动随机生成的临时路径，这让“下次启动如何发现旧实例”天然变得脆弱。
- 新 core 启动时会尝试绑定 model-plane HTTP；若 orphan core 仍占用默认监听地址 `127.0.0.1:19718`，新实例会在 UI attach 之前就失败。
- runtime root 合约已经倾向于“由 core 在 runtime root 下维护稳定的 control-plane endpoint 和状态目录”，但当前 bundled host 默认路径与这条边界并未完全对齐。

因此，真正缺失的不是“如何再加一个 shutdown 调用”，而是一个明确的宿主生命周期模型：

```text
Host lifecycle problem

UI launch
  -> discover old instance?
  -> is it alive / stale / conflicting?
  -> recover or kill?
  -> spawn new core
  -> attach
  -> work
  -> shutdown request
  -> confirm core exit + port release
```

与此同时，团队已经明确两条长期方向：

- 产品默认仍是 UI 强绑定的 ephemeral 模式；
- 后续允许按宿主策略切到后台持续运行模式，但不希望因此推翻 control-plane / app API / MCP / 多平台 host adapter 边界。

这意味着本次设计既要解决当前 macOS orphan core 问题，又不能把未来 persistent 模式堵死。

## 目标 / 非目标

**目标：**

- 为 bundled host 定义正式的 lifecycle state machine，而不是把生命周期行为散落在视图层和局部进程调用中。
- 明确 `ui-managed-ephemeral` 的残留实例治理规则：发现旧实例、探测健康、回收 orphan、等待资源释放，再启动新实例。
- 为 local control-plane 增加启动前 probe / ownership 摘要语义，使宿主在 attach 之前就能判断“旧实例是谁、处于什么状态、是否应该回收”。
- 让 bundled 默认发现路径基于 runtime root 的稳定身份，而不是基于每次 UI 启动生成的临时 endpoint。
- 定义“请求关闭”与“确认 core 真正退出”之间的宿主责任边界，覆盖正常退出、受控重启与快速重启。
- 为未来 persistent 模式预留 attach ownership / reattach / takeover 协议边界，同时不要求本次交付完整后台服务化。
- 为 macOS UI 明确定义生命周期相关状态、冲突提示与恢复动作。

**非目标：**

- 本次不实现完整的 `system-service` / `SMAppService` / privileged helper 路径。
- 本次不要求实现所有 persistent 模式行为，只要求把协议与状态边界设计清楚。
- 本次不把 core 改造成多 UI 并发服务；任意时刻仍保持单个 UI owner 约束。
- 本次不要求通过周期性 heartbeat 解决所有 crash 绑定问题；优先解决宿主可治理的启动前 reconcile 与退出确认。
- 本次不尝试把所有生命周期状态都塞进 core 内部 `readiness_state`；宿主状态与 core readiness 必须分层。

## 决策

### 决策 1：将 Host Lifecycle State 与 Core Readiness State 分层

当前 core 已经有 `starting / waiting_for_ui_attach / ready / shutting_down` 这类 readiness 状态，但这只能描述 core 进程内部是否已就绪，不能表达宿主实际面对的问题，例如：

- 是否发现了旧实例
- 旧实例是否健康
- 是否正在回收 orphan
- 是否正在等待端口释放
- 是否遇到活动实例冲突

因此本次明确分层：

- `CoreReadinessState`：由 core 维护，描述 core 自身是否 ready。
- `HostLifecycleState`：由平台 host adapter 维护，描述 UI/宿主在启动、恢复、关闭时的治理流程。

推荐的 bundled host lifecycle：

```text
needs_runtime_root
  -> discovering_existing
  -> probing_existing
     -> reconciling_orphan
     -> waiting_existing_exit
     -> stale_artifact_cleanup
     -> ownership_conflict
  -> spawning_core
  -> waiting_control_plane
  -> attaching_ui
  -> connected
  -> shutdown_requested
  -> waiting_core_exit
  -> waiting_resource_release
  -> terminated
  -> failed
```

这样可以避免继续把宿主问题伪装成单一的 “attach failed”。

备选方案：

- **把所有状态继续塞进 core readiness**：会把宿主治理与 core 运行时职责混在一起，也不利于未来不同平台复用。
- **继续只用若干布尔值拼接**：短期简单，但无法覆盖 orphan、冲突、快速重启和等待释放这些正式产品状态。

### 决策 2：默认 bundled 模式继续是 `ui-managed-ephemeral`，但 persistent 模式作为独立 ownership mode 预留

本次不改变当前默认产品语义：

- bundled 默认仍是 `ui-managed-ephemeral`
- UI 正常退出后，core 不应继续在后台作为默认常驻实例存在

但设计上要明确另一条未来路径：

- `host-managed-persistent`

两者的核心差异不是“是否自动关闭”这么简单，而是所有权语义不同：

- `ui-managed-ephemeral`：UI 消失后 core 继续活着通常意味着异常残留，应优先回收。
- `host-managed-persistent`：UI 消失后 core 继续活着可能是正常状态，应允许重新发现并 reattach。

因此 persistent 不能作为给 ephemeral 模式追加的一个布尔开关，而应该是独立的 host ownership mode。

备选方案：

- **在 `ui-managed-ephemeral` 上增加 `keep_alive=true`**：会把回收语义、attach 语义与 UI 冲突语义搅在一起，长期上更难维护。
- **现在就实现完整 persistent 模式**：超出当前核心问题，容易拖慢 macOS orphan 故障的治理。

### 决策 3：bundled 默认发现路径必须回到 runtime root 的稳定身份，临时 socket override 仅保留为开发/诊断逃生口

要想在 UI 启动前发现旧实例，宿主必须拥有稳定的发现身份。当前 “按本次 UI 启动生成临时 socket path” 的做法会导致：

- 新 UI 无法稳定定位旧 core
- orphan reconcile 只能依赖 `ps` 或端口碰撞后的失败症状
- 生命周期治理与 runtime root 合约分裂

因此 bundled 默认路径应改为：

- control-plane endpoint 由 runtime root 派生的稳定 endpoint 提供
- core 在 runtime root 下暴露稳定的 instance metadata/lease 记录或等价发现线索

`--control-plane-socket-override` 仍可保留，但定位为：

- 开发调试
- 测试隔离
- 特殊运维恢复

而不是 bundled 正常产品路径的默认发现方式。

备选方案：

- **继续使用每次启动随机 endpoint**：无法为 orphan reconcile、reattach 和冲突检测提供稳定基础。
- **完全依赖 model-plane 端口探测**：信息太粗，只能知道“有东西占着端口”，无法判断 ownership、host mode 和 readiness。

### 决策 4：为启动前 reconcile 引入稳定 instance metadata + probe-first 策略

宿主在 `spawn_new_core` 之前，必须先做 reconcile。推荐流程：

```text
1. 基于 runtime root 读取稳定 instance metadata / endpoint
2. 若无线索，则直接启动新 core
3. 若存在线索，则先 probe 旧实例
4. 根据 probe 结果进入：
   - no_instance
   - responsive_orphan
   - stale_unresponsive
   - ownership_conflict
5. 只有在确认旧实例已退出且资源释放后，才允许启动新 core
```

推荐的 instance metadata 至少包含：

- `core_instance_id`
- `host_mode` / `ownership_mode`
- `pid`
- `started_at`
- `runtime_root`
- `control_plane_endpoint`
- `model_plane_host`
- `model_plane_port`

但这里的 metadata 只是 discovery hint，不是最终真相。**最终真相必须是 probe 结果**。  
也就是说：

- 不能因为看见 metadata 文件就直接认为实例还活着
- 也不能仅凭一个旧 pid 文件就直接杀进程

probe-first 的原则是：

- 先尝试通过本地 control-plane 请求一个轻量的 `describe_host_instance` / `probe_host_instance`
- probe 失败后，再结合 pid/start time/endpoint 残留情况决定 cleanup 或 force cleanup

备选方案：

- **只用 pid file 判断**：容易误杀 pid 复用后的无关进程。
- **只用 socket 文件是否存在判断**：进程 crash 后 socket 文件可能残留，误判概率高。

### 决策 5：attach ownership 协议必须区分 `host_id` 与 `ui_session_id`

当前 `ui_instance_id` 同时承担“谁在 attach”与“这个 UI 进程是不是新的”两层含义，这在 crash/relaunch 场景下会导致新 UI 只能被视为“另一个实例”，很难表达：

- 同一个 host 的新 UI 会话
- 老 owner 已失效后的恢复
- future persistent 模式下的 reattach

因此协议上建议拆开：

- `host_id`：稳定宿主标识。用于表示同一个宿主家族或同一 runtime root 下的合法接管者。
- `ui_session_id`：单次 UI 进程会话标识。用于诊断、日志和当前 attach 会话区分。

同时为 control-plane 预留以下动作语义：

- `probe_host_instance`
- `attach_ui`
- `detach_ui`
- `takeover_ui` 或等价显式接管动作

本次不要求全部实现，但要求协议和状态机不再建立在“每次进程启动都是完全陌生的新 owner”这一假设上。

备选方案：

- **继续复用单个 `ui_instance_id`**：在崩溃恢复和未来 persistent 模式下会持续放大冲突。

### 决策 6：正常退出与受控重启必须采用“请求 -> 确认退出 -> 确认资源释放 -> 必要时升级”的两阶段关闭模型

当前只发出 `request_shutdown` 不够。宿主必须把“core 真正结束”视为单独阶段。

推荐关闭序列：

```text
host requests shutdown
  -> core enters shutting_down
  -> host waits for process exit
  -> host waits for control-plane endpoint removal
  -> host waits for model-plane host:port release
  -> if timeout:
       escalate (terminate / force cleanup according to policy)
```

在这里：

- “等待进程退出”与“等待资源释放”是不同阶段
- 重启路径与应用退出路径共享同一套关闭状态机
- escalation 必须有明确的诊断和边界，避免静默留下 orphan

对于 `ui-managed-ephemeral`：

- 若宿主明确知道自己拥有该 child process，可先走温和终止
- 若只能看到稳定 metadata 与 probe 结果，则需要基于 pid/start time/runtime root 交叉验证后再做 force cleanup

备选方案：

- **只发送 shutdown 不等待**：无法解决快速重启和 orphan 端口占用问题。
- **直接 kill 不做优雅关闭**：虽然粗暴有效，但会丢失优雅退出、日志 flush 和一致性边界。

### 决策 7：macOS UI 必须把生命周期恢复作为正式 UX，而不是统一归类为 attach failed

macOS host 需要把以下状态正式产品化：

- 发现旧实例
- 正在探测旧实例
- 正在回收 orphan core
- 等待监听地址释放
- 发现活动实例冲突
- 关闭超时，正在升级回收

相应恢复动作至少应包括：

- 重试探测
- 查看日志/诊断
- 重新选择 runtime root
- 在满足安全条件时执行 force cleanup

这样可以避免用户只看到“attach failed”或“启动失败”，却完全不知道失败是由 orphan core 占用监听地址引起的。

备选方案：

- **继续用单一错误态承接所有失败**：实现简单，但会极大削弱可恢复性和诊断价值。

### 决策 8：多平台前端继续通过 Host Adapter 消费生命周期能力，而不是让每种前端直接实现底层恢复逻辑

无论是 SwiftUI、WebView、Flutter 还是 Tauri，本次都不建议让前端层直接处理：

- Unix socket / named pipe 细节
- pid/start time 校验
- orphan reconcile
- attach ownership 冲突

这些逻辑应留在平台 host adapter：

- 它负责 lifecycle state machine
- 它负责与本地 control-plane 交互
- 它负责向前端提供前端友好的状态与恢复动作

这样后续跨平台 UI 只需要消费抽象状态，不需要重复实现底层宿主治理。

## 风险 / 权衡

- **[引入稳定 instance metadata 可能带来 stale file 误判]** → 把 metadata 定位为 discovery hint，所有 destructive action 必须 probe-first，并用 pid/start time/runtime root 交叉校验。
- **[退出等待会拉长关闭和重启路径]** → 将其视为正式宿主阶段，并提供超时与升级策略；代价是换取不再把 orphan 问题留到下一次启动时爆炸。
- **[macOS UI 会新增更多状态和恢复动作]** → 用显式状态机降低复杂度，避免继续把所有异常收敛到单一 attach failed。
- **[bundled 默认改回稳定 endpoint 需要迁移当前临时 override 习惯]** → 保留 override 作为调试逃生口，但将产品默认路径迁回 runtime root 稳定身份。
- **[persistent 模式目前只设计不完整实现]** → 这是有意为之；先把 ownership 协议立住，避免再次在实现阶段硬塞例外。

## 迁移计划

1. 先通过本次变更补齐 proposal/design/specs，统一团队对 ownership mode、startup reconcile 和 shutdown confirmation 的理解。
2. 在 core/control-plane 层增加启动前 probe 所需的轻量实例摘要，并暴露稳定 instance metadata/lease。
3. 将 bundled 默认发现路径切换为 runtime-root-stable identity，保留 `--control-plane-socket-override` 仅用于调试和测试。
4. 在 macOS host 层实现 lifecycle state machine：启动前探测、回收 orphan、等待退出、等待资源释放、冲突恢复。
5. 在 macOS UI 中补齐相应状态与恢复动作，并将 app termination 协调从 `ViewModel.deinit` 上移到更可靠的 host/app 生命周期层。
6. 增加契约测试与宿主验证，覆盖 orphan core、快速重启、端口占用、控制平面残留和冲突场景。

## Open Questions

- `host_id` 的稳定范围应以 runtime root 为主、以应用安装为主，还是二者组合？
- startup reconcile 的升级策略是否应区分“当前进程已知 child pid”与“仅通过 metadata 发现的残留实例”？
- 对活动实例冲突，产品默认是自动拒绝、提示用户确认，还是允许同宿主家族自动 takeover？
- 是否需要在核心协议中显式引入 `detach_ui`，还是允许 persistent 模式用新的 attach 模式隐式覆盖？
