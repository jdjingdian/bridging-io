## 新增需求

### 需求:bundled 桌面管理面必须独立于 model-plane HTTP 监听工作
在 bundled 桌面宿主形态下，本地管理面必须通过受信任 control-plane 与 core 交互，而不是依赖 model-plane HTTP 监听作为自己的管理通道。即使用户修改了 model-plane host/port 并触发重启，宿主也必须继续通过本地 control-plane 呈现生命周期状态与恢复路径。

#### 场景:用户保存新的 model-plane 端口
- **当** 用户在桌面管理面中保存新的 model-plane host/port，并且该变更需要重启 core
- **那么** 宿主必须继续通过本地 control-plane 展示 `saving / restarting / connected / failed` 等状态，而不是要求 UI 通过浏览器跳转或额外管理端口才能继续工作

#### 场景:model-plane 旧端口已失效但管理面仍可恢复
- **当** 旧 model-plane 监听地址已被关闭，且 replacement core 仍在启动或 attach
- **那么** 桌面管理面必须仍可展示重启进度、诊断与恢复入口，而不是因为旧端口失效就丢失主界面

### 需求:本地 control-plane 必须提供 timeline 来源分组摘要
受信任的本地 control-plane 在向桌面控制台返回 timeline 或等价审计数据时，必须同时提供适合 UI 做来源分组的安全摘要字段。该摘要至少必须支持“有 token label 的认证请求”和“无 token 的指纹 / user-agent 来源请求”两类分组主键。

#### 场景:桌面 UI 首次读取 timeline
- **当** 桌面 UI attach 成功后请求首屏 timeline 数据
- **那么** control-plane 必须返回每条活动所属的来源分组摘要，使 UI 能直接按来源分组渲染，而不是要求 UI 自己猜测如何把事件归并

#### 场景:后续增量时间线更新
- **当** 桌面 UI 继续读取 timeline 增量或事件流
- **那么** 系统必须继续为新增活动附带一致的来源分组摘要，而不是让同一来源在不同读取批次中失去可追踪的归属关系

## 修改需求

## 移除需求
