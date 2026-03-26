## 新增需求

### 需求:macOS 控制台必须以 core 真实状态作为运行时真相源
macOS SwiftUI 控制台在产品运行时必须通过受信任的本地 control-plane 读取 core 真实状态，而不是默认展示预置 target、预置 timeline、预置 artifact、预置 approval 或其他示例性业务数据。若 core 当前没有真实数据，控制台必须展示正式空态、加载态或错误态。

#### 场景:首次启动时 core 没有任何目标数据
- **当** 用户启动控制台并成功连接 core，但 core 当前没有任何 target、session 或 artifact
- **那么** 控制台必须展示正式空态，而不是自动渲染示例目标、示例命令历史或示例日志输出

#### 场景:core 返回真实目标与会话数据
- **当** 控制台从 core 读取到真实的 target、session、timeline 或 approval 数据
- **那么** 控制台必须基于这些真实数据渲染界面，而不是继续保留与真实状态无关的预置展示内容

### 需求:macOS 控制台必须展示 bundled core 的启动与连接状态
在 bundled 发行物中，macOS 控制台必须能够反映其托管 core 的启动、attach、连接失败与恢复状态，而不是假定 core 永远已经就绪。至少必须覆盖“正在启动 core”、“等待 attach 完成”、“attach 失败”与“已连接到真实 core”这些状态。

#### 场景:控制台等待 bundled core 完成 attach
- **当** 用户启动 bundled 控制台，且受托管 core 尚在启动或等待 attach 完成
- **那么** 控制台必须展示明确的启动中或连接中状态，而不是立即进入看似已有真实数据的主界面

#### 场景:控制台连接 bundled core 失败
- **当** bundled 控制台启动 core 或 attach core 失败
- **那么** 控制台必须展示可理解的失败状态与后续恢复入口，而不是继续显示过期 seed 内容或空白的伪成功界面

## 修改需求

无。

## 移除需求

无。
