## 新增需求

### 需求:桌面宿主的 onboarding 与 recovery 必须消费 core 共享生命周期状态机
跨平台桌面控制台的 onboarding、recovery 和 managed restart 流程必须消费 core 共享的 runtime/config 生命周期状态与恢复动作，而不是继续由宿主独立推断目录是否有效、配置是否可迁移或权限是否可恢复。

#### 场景:已保存 runtime root 失效
- **当** 桌面宿主启动时发现已保存的 runtime root 对应的 core 生命周期状态为需要恢复或需要重选目录
- **那么** 宿主必须进入恢复态引导流程，并展示 core 返回的恢复动作，而不是自行发明另一套目录失效判断逻辑

#### 场景:packaged host 遇到权限问题
- **当** packaged 桌面宿主在默认目录或已保存目录上遇到权限不足问题
- **那么** 宿主必须展示 core 返回的权限修复或重选目录语义，而不是静默切换到新的临时目录继续工作
