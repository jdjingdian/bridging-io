## 新增需求

### 需求:bundled UI-managed core 必须从用户选定的 runtime root 装载或创建配置
在 bundled UI-managed 模式下，UI 必须在首次启动时提示用户选择一个 runtime root 目录，并在本地持久化该选择。后续启动时，UI 必须使用该目录拉起 core；core 必须在该目录下装载或创建自己的配置、状态与 artifact 数据，而不是继续依赖 UI 代写临时配置文件。

#### 场景:首次启动 bundled UI 时尚未选择 runtime root
- **当** 用户首次启动 bundled UI，且当前尚未存在已保存的 runtime root
- **那么** UI 必须先提示用户选择 runtime root，保存该选择后再启动 core，而不是静默回退到随机临时目录

#### 场景:已保存的 runtime root 不可访问
- **当** UI 启动时发现已保存的 runtime root 不存在、不可写或权限已失效
- **那么** UI 必须进入明确的恢复状态并要求用户重新选择目录，而不是在后台悄悄切换到其他路径继续运行

### 需求:core-owned 设置写入必须返回生效策略
受信任的本地 control-plane 在处理 settings/profile 写入时，core 必须完成校验与持久化，并明确返回该次变更的生效策略，例如 `live_applied` 或 `restart_required`。UI 不得自行猜测某项设置是否需要重启。

#### 场景:UI 修改 model-plane host 或 port
- **当** 用户在 UI 中修改 model-plane 的监听 host 或 port
- **那么** core 必须对该配置执行校验与持久化，并返回该变更是否需要重启当前 bundled core 才能生效

#### 场景:UI 修改 target profile
- **当** 用户在 UI 中创建或编辑某个 target profile
- **那么** 该修改必须通过 core 的 profile 写接口完成校验与持久化，并返回明确结果，而不是只停留在 UI 本地状态中

## 修改需求

### 需求:standalone core 必须统一管理配置装载、设置入口与控制平面
系统必须允许 Rust core 以前台进程、daemon 形式或 bundled UI-managed 形态运行，并且在启动时读取或创建由 core 自己拥有的配置文件。该配置文件与 UI、CLI、后续其他前端看到的目标/profile 设置，必须映射到同一套 core-owned 数据模型与校验语义，而不是由 UI 自行维护一份独立 schema。受信任的本地 UI/control plane 默认必须通过本地 app API IPC 接入 core，而不是复用面向 AI 的 MCP HTTP 能力面。

#### 场景:bundled UI-managed 模式从 runtime root 启动 core
- **当** bundled UI 使用已保存的 runtime root 拉起 core
- **那么** core 必须在该目录下装载或创建统一配置，并使后续的 target、settings 与 diagnostics 都基于同一份 core-owned 配置真相运行

#### 场景:UI 修改目标设置
- **当** 用户在 UI 中修改某个 SSH 目标的主机地址、用户名、调用别名或凭据引用
- **那么** 该修改必须通过 core 的设置接口完成校验和持久化，而不是只停留在 UI 的本地状态中

## 移除需求

无。
