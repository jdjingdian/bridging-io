## 新增需求

### 需求:standalone 生命周期与 control-plane 错误必须使用共享错误与状态契约
BridgingIO 在 standalone 启动、runtime root bootstrap、配置装载、会话恢复、control-plane attach 和 host mode 切换过程中，必须通过共享错误与状态契约返回 lifecycle 状态与失败结果，而不是继续依赖散落的字符串消息。

#### 场景:standalone 启动遇到配置问题
- **当** standalone core 在启动阶段遇到配置缺失、配置不兼容、运行目录不可写或迁移失败
- **那么** 系统必须返回共享状态、共享错误分类、模块子码和恢复提示，而不是只输出临时错误文本

### 需求:会话与运行模式状态投影必须使用 canonical 状态词汇
BridgingIO 对外暴露的会话、运行模式和 lifecycle 状态投影必须使用共享状态词汇，至少能够稳定区分 `ready`、`degraded`、`locked`、`not_ready`、`unsupported` 和 `method_not_implemented` 等语义。

#### 场景:control-plane 读取当前运行状态
- **当** 本地 control-plane、TUI 或 future UI 读取当前 core 的启动、attach、恢复或 shutdown 状态
- **那么** 返回结果必须使用共享状态词汇，而不能由不同调用面各自发明近义状态标签
