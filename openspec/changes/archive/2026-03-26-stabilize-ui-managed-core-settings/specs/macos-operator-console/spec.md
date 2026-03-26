## 新增需求

### 需求:macOS 控制台必须在首次 bundled 启动时处理 runtime root 选择
在 bundled UI-managed 模式下，macOS 控制台必须在首次启动时提示用户选择 runtime root 目录，并在后续启动中复用该选择。若已保存目录失效，控制台必须提供重新选择与恢复入口。

#### 场景:首次启动时尚未配置 runtime root
- **当** 用户首次打开 bundled macOS 控制台且尚未保存 runtime root
- **那么** 控制台必须优先展示目录选择流程，而不是直接尝试启动 core 并落到不透明失败状态

#### 场景:已保存目录失效
- **当** 控制台启动时发现已保存的 runtime root 已不可访问
- **那么** 控制台必须展示明确的目录失效状态，并提供重新选择目录的操作入口

### 需求:macOS 控制台必须提供 model-plane host 与 port 设置入口
macOS 控制台必须在正式设置界面中提供 model-plane host 与 port 的查看与编辑入口，并明确展示 bundled 默认值 `127.0.0.1:19718` 以及“保存后将重启 core”的生效方式。

#### 场景:用户查看 bundled 默认监听地址
- **当** 用户打开设置界面查看 model-plane 配置
- **那么** 控制台必须能够展示当前生效的 host 与 port，并在默认路径下显示 `127.0.0.1:19718`

#### 场景:用户修改 model-plane host 或 port
- **当** 用户在设置界面中保存新的 model-plane host 或 port
- **那么** 控制台必须通过 core-owned settings 接口提交修改，并在需要时进入受控重启流程，而不是继续保持旧值且无任何反馈

### 需求:macOS 控制台的 target 编辑必须基于 core 返回的真实完整 profile
macOS 控制台在打开 target 创建或编辑界面时，必须使用 core 返回的真实完整 profile 初始化表单，而不是使用 bootstrap 摘要与本地占位值拼接出一个伪 profile。

#### 场景:用户编辑已存在目标
- **当** 用户打开某个已保存 target 的编辑界面
- **那么** 控制台必须先从 core 读取该 target 的完整 profile，再展示可编辑字段，而不是用默认端口、空连接信息或伪凭据引用填充界面

## 修改需求

### 需求:macOS 控制台必须展示 bundled core 的启动与连接状态
在 bundled 发行物中，macOS 控制台必须能够反映其托管 core 的启动、attach、连接失败、目录失效、重启中、重启失败与恢复状态，而不是假定 core 永远已经就绪。至少必须覆盖“需要选择 runtime root”、“正在启动 core”、“等待 attach 完成”、“保存中”、“等待重启”、“重启中”、“重启失败”与“已连接到真实 core”这些状态。

#### 场景:控制台等待 bundled core 完成 attach
- **当** 用户启动 bundled 控制台，且受托管 core 尚在启动或等待 attach 完成
- **那么** 控制台必须展示明确的启动中或连接中状态，而不是立即进入看似已有真实数据的主界面

#### 场景:控制台因设置变更进入重启流程
- **当** 用户保存了一项由 core 判定为 `restart_required` 的设置
- **那么** 控制台必须展示受控重启状态，并在新 core attach 成功前阻止界面伪装成已经恢复连接

#### 场景:控制台重启 bundled core 失败
- **当** bundled 控制台在保存设置后重启 core 失败
- **那么** 控制台必须展示可理解的失败状态、日志或诊断入口与重试动作，而不是静默回退到过期 seed 内容或空白界面

## 移除需求

无。
