# menuconfig-style-matrix 规范

## 目的
待定 - 由归档变更 standardize-menuconfig-style-contract 创建。归档后请更新目的。
## 需求
### 需求:`menuconfig` 必须维护正式的风格矩阵文档
BridgingIO 必须维护一份正式的 `menuconfig` 风格矩阵文档，作为 `bridgingio-core menuconfig` 及未来复用同一 menuconfig 语法的本地 UI/TUI host 的风格真相源。该矩阵必须记录布局区域、行语法、可聚焦性、键位语义、popup 样式、fallback 规则、最小视口与溢出裁剪，而不是继续让这些约定散落在代码与零散 spec 中。

#### 场景:新增 menuconfig 行类型
- **当** 团队新增一种 menuconfig 行类型、前缀语法或新的可操作入口
- **那么** 系统必须同步更新风格矩阵，明确该行的前缀、后缀、是否可聚焦、由 `Space` 还是 `Enter` 触发，以及其 fallback 语义

#### 场景:新增 menuconfig popup 或流程
- **当** 团队为 menuconfig 新增确认链路、选择弹窗、文本编辑器或其他 popup 流程
- **那么** 系统必须同步更新风格矩阵，明确该 popup 的布局、焦点体、按键语义、退出规则与选中态表现，而不是只在实现中临时决定

#### 场景:未来功能变更触及 menuconfig 风格
- **当** 某个后续功能变更修改了 `menuconfig` 的行语法、键位语义、popup 行为、终端降级样式或最小视口规则
- **那么** 该变更必须同步更新风格矩阵和对应 spec，而不是只提交代码实现

### 需求:风格矩阵必须记录当前偏差并阻止带病归档
当当前实现与正式 menuconfig 风格合同存在偏差时，风格矩阵必须明确记录这些偏差、受影响语义和修复要求。只要这些偏差仍属于当前 change 的定义范围，本 change 就不得归档为完成状态。

#### 场景:当前实现仍允许不可更改项获得焦点
- **当** 当前实现仍让 `---`、`- -`、`-*-` 这类行获得焦点或显示可执行的选中态
- **那么** 风格矩阵必须把该问题记录为当前偏差，并要求在本 change 归档前修复

#### 场景:popup 选中态与主菜单不一致
- **当** 当前实现的确认弹窗或单选弹窗没有使用与主菜单一致的反色或 fallback 高亮
- **那么** 风格矩阵必须把该问题记录为当前偏差，并要求在本 change 归档前修复

#### 场景:本 change 准备归档但仍有已知偏差
- **当** 团队准备归档本 change，但风格矩阵中的当前偏差仍未清空
- **那么** 本 change 不得被视为已完成，除非这些偏差已被修复并从矩阵中移除

### 需求:风格矩阵必须记录 SSH `Test Connection` 行与 popup 合同
当 `menuconfig` 为 SSH target 引入 `Test Connection --->` 动作时，风格矩阵必须明确记录该 action row 的位置、行语法、可聚焦性与 popup 链路，而不是只在实现中临时决定。

#### 场景:矩阵记录 SSH Test Connection 行语法
- **当** `menuconfig` 在 plain SSH 的 `Connection Profile` 或 unlocked sealed SSH 的 `Sensitive Overlay` 中暴露 `Test Connection --->`
- **那么** 风格矩阵必须明确记录该行使用与其他 action row 一致的 `--->` 语法
- **并且** 必须明确记录该行由 `Enter` 触发，而不是 `Space`

#### 场景:矩阵记录 timeout 输入弹窗
- **当** `menuconfig` 为 SSH 测试连接弹出 timeout 输入框
- **那么** 风格矩阵必须明确记录该弹窗属于 centered text-input popup
- **并且** 必须记录默认值为 `2000` 毫秒、`Enter` 确认开始测试、`Esc` 取消返回

#### 场景:矩阵记录 waiting 与 result 弹窗
- **当** `menuconfig` 进入 SSH 测试连接的等待态或结果态
- **那么** 风格矩阵必须明确记录 waiting popup 会拦截普通菜单导航
- **并且** 必须记录 waiting 态支持 `Esc` 取消
- **并且** 必须记录 result popup 只展示简洁状态，并由 `Enter` 或 `Esc` 关闭

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

