## 新增需求

### 需求:风格矩阵必须记录 SSH Authentication 与 SSH 安全访问行合同
当 `menuconfig` 为 SSH target 引入 `SSH Authentication --->`、前置的 `SSH Authentication Setup` 流程以及条件显示的 `SSH 安全访问` 控件时，风格矩阵必须明确记录这些行的语法、可聚焦性、触发键位与只读状态表现。

#### 场景:矩阵记录 SSH Authentication 入口行语法
- **当** plain SSH 的 `Connection Profile` 或 unlocked sealed SSH 的 `Sensitive Overlay` 暴露 `SSH Authentication --->`
- **那么** 风格矩阵必须明确记录该行使用与其他 action row 一致的 `--->` 语法
- **并且** 必须记录该行由 `Enter` 进入，而不是由 `Space` 切换

#### 场景:矩阵记录认证类型行的单选语义与键位
- **当** `SSH Authentication Setup` 渲染 `none` / `password` / `private-key` 认证类型
- **那么** 风格矩阵必须明确记录三者是互斥单选，且任意时刻只能选中一个
- **并且** 必须记录 `Space` 用于切换当前焦点项，`Enter` 用于进入 `password` / `private-key` 的详情编辑，不得把 `Enter` 解释为切换操作

#### 场景:矩阵记录继续编辑门控状态
- **当** 当前认证草稿尚未满足进入详情编辑的前置条件（例如未完成 `password` 或 `private-key` 必填输入）
- **那么** 风格矩阵必须明确记录 `继续进入详情编辑` 使用禁用态呈现
- **并且** 必须记录禁用态文案需直接说明阻断原因，而不是仅提供弱提示

#### 场景:矩阵记录 plain SSH 安全访问 toggle 语法
- **当** plain SSH target 选择 `password` 或本地未加密私钥，并显示 `SSH 安全访问`
- **那么** 风格矩阵必须明确记录该行使用 `Single-choice toggle` 语义（`< >` / `<*>`）
- **并且** 必须明确记录该控件仅允许 `Space` 切换，`Enter` 不得触发切换

#### 场景:矩阵记录 sealed SSH 强制安全访问的只读样式
- **当** sealed SSH target 需要显示强制开启的 `SSH 安全访问`
- **那么** 风格矩阵必须明确记录该状态使用只读 info row、placeholder row 或等价 non-focusable 语法
- **并且** 不得把该强制状态渲染成可交互 toggle

#### 场景:矩阵记录 plain private-key 来源选项去重
- **当** plain SSH target 选择 `private-key`
- **那么** 风格矩阵必须明确记录来源选项仅包含本地路径语义，不得展示 vault key 来源
- **并且** 同一屏幕不得同时出现重复含义的本地 key 路径条目

#### 场景:矩阵记录本地加密私钥阻断弹窗
- **当** 操作员在 `SSH Authentication Setup` 中输入本地私钥路径且系统检测到该私钥带 passphrase
- **那么** 风格矩阵必须明确记录该反馈使用 centered error/result popup
- **并且** 必须记录该弹窗只展示阻断性说明与后续引导，而不继续停留在可提交状态

## 修改需求

## 移除需求
