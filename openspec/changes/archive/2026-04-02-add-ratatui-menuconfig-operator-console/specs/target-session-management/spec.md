## 新增需求

### 需求:core 必须允许通过 `menuconfig` 管理同一套设置真相
BridgingIO 除了允许通过配置文件管理设置外，还必须允许通过 `bridgingio-core menuconfig` 管理同一套设置真相。该交互式配置面必须与配置文件、control-plane 和 future UI 共享同一套校验与持久化语义，而不是只服务 standalone。

#### 场景:用户通过 `menuconfig` 修改设置
- **当** 操作员在 `bridgingio-core menuconfig` 中修改某个 profile、toolchain、model-plane 或 storage 配置
- **那么** 系统必须对该修改应用与配置文件、control-plane 相同的校验与持久化规则，而不是由 TUI 维护一套私有逻辑

#### 场景:修改需要重启才能生效
- **当** 操作员通过 `menuconfig` 保存了一个需要重启或重新初始化才能生效的配置项
- **那么** 系统必须返回正式的 apply 结果或等价状态，而不是让 TUI 自行猜测修改是否立即生效

#### 场景:Target 列表与编辑入口遵循一致的 menuconfig 视觉语义
- **当** 操作员在 Targets 菜单浏览或进入某个 target
- **那么** 系统必须在 target 入口行使用可识别的状态标记（如 `< >` / `<*>`）与 `--->` 导航提示
- **并且当** 操作员编辑 target 字段
- **那么** 系统必须沿用统一弹窗字段样式 `Label (value) --->` 与统一开关标记 `[ ]` / `[*]`
