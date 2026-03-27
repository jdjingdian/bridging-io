## 新增需求

### 需求:macOS 控制台必须在系统设置中管理全局工具来源默认值
macOS 控制台必须在正式设置界面中提供全局 `toolchains.<name>.path_override` 的查看与编辑入口，并将其作为系统级默认值与 model-plane host/port 同级展示，而不是混入 target 编辑页或工作台 target 详情页。

#### 场景:用户在设置界面修改全局 ADB 默认值
- **当** 用户在设置界面中保存新的全局 `toolchains.adb.path_override`
- **那么** 控制台必须通过 core-owned settings 接口提交该修改，并在刷新后让未配置 target override 的 ADB target 看到新的全局默认来源

#### 场景:工作台 target 详情页不再承担全局写入口
- **当** 用户在工作台主界面查看某个 target 的工具来源卡片
- **那么** 控制台必须将该区域定位为诊断展示，而不是继续把全局 override 的直接编辑入口放在 target 详情页中

## 修改需求

### 需求:macOS 控制台必须展示工具来源与连接诊断
macOS SwiftUI 控制台必须能够展示当前连接器或 provider 依赖工具的来源和诊断结果，并在职责清晰的界面边界内提供 target 级与全局级覆盖路径设置入口。target 编辑页只允许编辑当前 target 相关工具的局部 override；全局默认值必须在系统设置页编辑。来源至少必须覆盖“target 级用户覆盖”、“全局用户覆盖”、“系统 PATH”和“内置后备”四种状态。

#### 场景:编辑 SSH target 时只显示 SSH override
- **当** 用户打开一个 SSH target 的编辑界面
- **那么** 控制台必须只显示 `ssh` 相关的 target override 输入项，而不是同时显示 `adb` 或其他与当前 target 无关的工具项

#### 场景:编辑 ADB target 时只显示 ADB override
- **当** 用户打开一个 ADB target 的编辑界面
- **那么** 控制台必须只显示 `adb` 相关的 target override 输入项，而不是同时显示 `ssh` 或其他与当前 target 无关的工具项

#### 场景:target 诊断区分 target 与 global
- **当** 用户查看某个 target 的工具来源诊断，且该 target 同时存在全局默认值与 target 局部 override
- **那么** 控制台必须能够区分展示 target override、global override 与最终生效路径，而不是只显示一条无法判断来源层级的 effective path

## 移除需求

无。
