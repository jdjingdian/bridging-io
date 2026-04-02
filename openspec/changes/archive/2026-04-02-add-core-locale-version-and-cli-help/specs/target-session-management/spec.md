## 新增需求

### 需求:core-owned 配置必须持久化独立的 operator locale 字段
BridgingIO 的 core-owned 配置模型必须持久化一个独立的 operator locale 字段，用于声明 core-owned CLI/TUI 使用的语言。该字段必须与 future UI 的语言偏好解耦，并且当前版本只允许 `zh-CN` 与 `en-US` 两个合法值。

#### 场景:配置文件显式声明 core locale
- **当** 操作员在配置文件中写入 `core.operator_locale = "zh-CN"`（或等价正式键名）
- **那么** core 必须在装载该配置后将中文作为 CLI/TUI 的 operator-facing 语言，而不是忽略该字段或要求 UI 代为翻译

#### 场景:配置文件未声明 core locale
- **当** 配置文件未显式设置 core operator locale
- **那么** core 必须按正式默认值装载该字段，并在后续序列化或示例配置中保持可预测的一致行为，而不是依赖宿主系统 locale 隐式漂移

### 需求:core-owned locale 字段必须执行严格校验与稳定写回
core operator locale 字段必须执行严格枚举校验，并且在配置装载、menuconfig 保存、示例配置生成与 round-trip 序列化时保持稳定写回。系统禁止接受当前版本不支持的 locale 值并静默降级。

#### 场景:保存不受支持的 locale 值
- **当** 操作员或工具尝试把 core operator locale 写为 `ja-JP` 等当前版本不支持的值
- **那么** core 必须拒绝该配置并返回明确错误，而不是静默接受后再回退成其他语言

#### 场景:menuconfig 保存 core locale
- **当** 操作员在 `menuconfig` 中修改 core operator locale 并保存
- **那么** 配置写回后的 TOML 必须稳定保留该字段和值，而不是只在内存中生效或在下一次序列化时丢失

## 修改需求

无。

## 移除需求

无。
