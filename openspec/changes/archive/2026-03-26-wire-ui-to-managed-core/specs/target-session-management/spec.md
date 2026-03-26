## 新增需求

### 需求:bundled 发行物中的 core 必须由 UI 托管并与其生命周期强绑定
在 bundled 发行物中，BridgingIO 必须把 Rust core 作为受 UI 托管的本地进程启动。默认模式下，UI 必须在启动时拉起 core、通过本地 control-plane 完成 attach，并在 UI 正常退出时终止同一 core，而不是默认把 core 作为独立后台常驻服务留在系统中。

#### 场景:UI 启动 bundled core 并完成 attach
- **当** 平台 UI 以 bundled 形态启动并准备进入控制台工作流
- **那么** 系统必须先拉起同一发行物内受托管的 core、开放本地 control-plane，并在 UI attach 成功后才将该运行实例视为已就绪

#### 场景:UI 正常退出 bundled 应用
- **当** 用户正常关闭 bundled UI
- **那么** 由该 UI 托管的 core 必须随之退出，而不是在后台继续保持默认运行

### 需求:standalone core 运行模式必须区分前台 `run` 与后台 `-d`
BridgingIO core 必须提供明确的 standalone 运行模式语义。`run` 必须表示前台运行并占据当前终端；`-d` 必须表示后台或脱离式运行。两种模式必须复用同一套 core-owned 配置、control-plane 和 model-plane 语义，而不是形成两套分叉实现。

#### 场景:操作员以前台模式启动 core
- **当** 操作员使用 `run` 启动 standalone core
- **那么** 系统必须以前台进程方式运行 core，并使其立即按照该配置开放相应的 control-plane 与 model-plane 能力

#### 场景:操作员以脱离式模式启动 core
- **当** 操作员使用 `-d` 启动 standalone core
- **那么** 系统必须以后台或脱离式方式运行同一套 core 语义，而不是要求 UI 托管该实例或切换到另一套实现路径

### 需求:本地 control-plane 必须提供 UI attach 与控制台 bootstrap 语义
受信任的本地 control-plane 必须允许平台 UI 在启动时完成 attach，并读取构建真实控制台所需的最小快照。该最小快照至少必须覆盖 targets、sessions、approvals、settings、diagnostics，以及时间线或 transcript 的初始读取入口。

#### 场景:UI 首次连接 core 并读取控制台快照
- **当** 平台 UI 连接到本地 core 并完成 attach
- **那么** control-plane 必须允许 UI 读取构建控制台首屏所需的最小真实状态，而不是要求 UI 先用本地 seed 占位

#### 场景:core 尚无目标或会话数据
- **当** UI attach 成功，但 core 当前尚未装载任何 target、session 或 artifact
- **那么** control-plane 必须返回真实的空结果，使 UI 能展示正式空态，而不是伪造示例目标或示例时间线

## 修改需求

无。

## 移除需求

无。
