## 新增需求

### 需求:bundled host 的生命周期发现必须基于 runtime root 稳定身份
在 bundled 默认产品路径下，宿主必须使用由 runtime root 推导出的稳定 control-plane endpoint、instance metadata 或等价稳定身份来发现现有 managed core。系统禁止把仅随单次 UI 进程变化的临时 endpoint 作为正常产品路径下唯一的实例发现依据。

#### 场景:同一 runtime root 上重启 bundled UI
- **当** 用户在同一 runtime root 上重新启动 bundled UI
- **那么** 宿主必须仍能发现上一次遗留或仍在运行的 managed core，而不是因为本次 UI 会话生成了新的临时 endpoint 就把旧实例视为不可见

#### 场景:存在残留实例但当前 UI 会话标识已变化
- **当** 上一次 UI 会话异常退出，新的 UI 会话在同一 runtime root 上重新启动
- **那么** 系统必须仍能基于稳定身份发现旧 core，并进入后续的 probe / reconcile 流程，而不是直接把残留实例留到新的 bind 冲突阶段才暴露

### 需求:bundled host 在启动新的 ephemeral core 之前必须协调残留实例
在 `ui-managed-ephemeral` 默认模式下，宿主在拉起新的 managed core 之前，必须先协调同一 runtime root 下可能残留的旧实例。只要旧实例仍响应或仍占用本地 transport/model-plane 资源，宿主就必须先完成回收或进入显式冲突状态，而不是直接继续启动另一个 core。

#### 场景:启动前发现可响应的 orphan core
- **当** bundled host 在同一 runtime root 下发现一个仍可通过本地 control-plane probe 的 `ui-managed-ephemeral` core，但当前 UI 会话并不是原始 owner
- **那么** 宿主必须先请求该实例关闭、等待其退出并确认本地 endpoint 与 model-plane 监听资源释放后，才能启动新的 core

#### 场景:启动前发现不可响应的残留发现线索
- **当** 宿主在 runtime root 下发现残留的 endpoint、instance metadata 或等价发现线索，但 probe 无法连通且不存在可确认的活动 core
- **那么** 宿主必须把这些线索视为 stale artifact 并先完成清理，再进入新的启动流程，而不是直接把后续 bind 失败暴露为通用启动错误

#### 场景:启动前发现不可自动回收的活动实例
- **当** 宿主在同一 runtime root 下发现一个活动实例，但该实例不满足自动回收策略或 ownership 策略要求人工介入
- **那么** 系统必须进入显式冲突状态并返回恢复指引，而不是静默杀掉旧实例或继续启动第二个 core

### 需求:本地 control-plane 必须提供启动前实例探测与 ownership 摘要
受信任的本地 control-plane 除了 attach 与 bootstrap 语义外，还必须允许宿主在 attach 之前探测已有实例，并读取最小 ownership 摘要。该摘要至少必须覆盖 `core_instance_id`、host/ownership mode、readiness state、runtime root 标识，以及当前 attached owner 的摘要或空值。

#### 场景:宿主在 attach 前探测已有实例
- **当** bundled host 在启动新 core 之前对一个已发现的本地实例执行 probe
- **那么** control-plane 必须返回足以支持 reconcile 决策的实例摘要，而不是要求宿主先完成 attach 才能知道该实例是谁、处于什么状态

#### 场景:attach 遇到 owner 冲突
- **当** 一个新的 UI 会话尝试 attach 到已被其他 owner 占据的本地 core
- **那么** 系统必须返回显式的 ownership conflict 或等价结构化语义，并包含足够的诊断信息，而不是只返回无法区分原因的通用校验失败

## 修改需求

### 需求:bundled 发行物中的 core 必须由 UI 托管并与其生命周期强绑定
在 bundled 发行物中，BridgingIO 默认必须把 Rust core 作为由本地 host adapter 托管的 `ui-managed-ephemeral` 进程启动。默认模式下，UI 必须在启动时拉起或接管同一发行物内的 managed core、通过本地 control-plane 完成 attach，并在 UI 正常退出或受控重启时确保该 core 最终退出并释放其本地 transport 与 model-plane 监听资源，而不是仅发出 shutdown 请求后放任其在后台残留。

#### 场景:UI 启动 bundled core 并完成 attach
- **当** 平台 UI 以 bundled 默认形态启动并准备进入控制台工作流
- **那么** 系统必须先完成旧实例协调、再拉起或接管同一发行物内受托管的 core、开放本地 control-plane，并在 UI attach 成功后才将该运行实例视为已就绪

#### 场景:UI 正常退出 bundled 应用
- **当** 用户正常关闭 bundled UI
- **那么** 宿主必须请求当前 managed core 关闭，并等待该实例真正退出且释放本地 endpoint 与默认监听资源后，才将本次关闭视为完成，而不是仅发送一次 shutdown 后立即结束生命周期管理

#### 场景:bundled host 因设置变更执行受控重启
- **当** bundled host 因设置变更或恢复操作需要重启当前 managed core
- **那么** 宿主必须先关闭旧 core、确认旧实例退出并释放关键资源，再启动 replacement core，而不是允许新旧两个实例在同一 runtime root 或监听地址上竞争

## 移除需求

无。
