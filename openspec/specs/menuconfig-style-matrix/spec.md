# menuconfig-style-matrix 规范

## 目的
待定 - 由归档变更 standardize-menuconfig-style-contract 创建。归档后请更新目的。
## 需求
### 需求:`menuconfig` 必须维护正式的风格矩阵文档
BridgingIO 必须维护一份正式的 `menuconfig` 风格矩阵文档，作为 `bridgingio-core menuconfig` 及未来复用同一 menuconfig 语法的本地 UI/TUI host 的风格真相源。该矩阵必须记录布局区域、行语法、row template、group template、popup template、可聚焦性、键位语义、popup 样式、fallback 规则、最小视口与溢出裁剪，而不是继续让这些约定散落在代码与零散 spec 中。

#### 场景:风格矩阵升级为模板化真相源
- **当** 团队维护或审查 `menuconfig` 风格矩阵文档
- **那么** 该文档必须同时记录字符串语法与正式模板目录
- **并且** 不得继续把 row template / group template / popup template 留在实现细节或设计口头约定中

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

### 需求:风格矩阵必须记录 dirty 标记与退出确认的真实差异语义
当 `menuconfig` 在标题、状态栏或退出确认中展示 dirty tracking 时，风格矩阵必须明确记录其真相是“当前草稿相对会话基线仍存在真实差异”。矩阵不得继续把 dirty 标记描述成“本次会话中编辑过字段”的历史痕迹。

#### 场景:矩阵记录标题 dirty 后缀的出现条件
- **当** 风格矩阵记录主标题中的 dirty 后缀或等价视觉提示
- **那么** 必须明确该提示只在当前配置相对会话基线仍存在真实差异时显示
- **并且** 不得把“字段曾被编辑过”记录为标题 dirty 后缀的触发条件

#### 场景:矩阵记录字段恢复基线后的消脏语义
- **当** 某个字段先被修改、随后又恢复到会话基线值
- **并且** 当前配置相对会话基线已不存在真实差异
- **那么** 风格矩阵必须明确记录该状态不再显示 dirty 标记
- **并且** 退出时不得进入未保存修改确认链路

#### 场景:矩阵记录真实差异仍存在时的退出确认
- **当** 当前配置相对会话基线仍有至少一处真实差异
- **那么** 风格矩阵必须明确记录根页面 `Esc` / `Exit` 会触发未保存修改确认弹窗
- **并且** 该合同必须与标题 dirty 标记的出现条件保持一致

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

### 需求:风格矩阵必须维护正式的 row template catalog
BridgingIO 的 `menuconfig` 风格矩阵除了记录行语法外，还必须维护一套正式的 row template catalog，用于说明每种行的语义角色、固定列结构、可聚焦性、键位触发规则、`--->` 使用条件和选中态表现。系统不得继续只依赖“看起来像某种字符串语法”的弱约定来指导后续页面设计。

#### 场景:新增 menuconfig 行时先选择模板
- **当** 团队为 `menuconfig` 新增一类行、入口或状态展示
- **那么** 风格矩阵必须先说明该行属于哪个正式 row template
- **并且** 必须记录该模板的前缀、后缀、焦点性、键位语义和降级表现

#### 场景:模板目录记录当前可复用的最小集合
- **当** 风格矩阵维护 row template catalog
- **那么** 目录中至少必须区分 `info`、`boolean toggle`、`exclusive choice`、`exclusive choice entry`、`multi-select`、`field entry`、`action`、`blocked action` 与 `required readonly` 等模板
- **并且** 不得继续把纯布尔开关与互斥单选组中的选项合并为一个含混的模板概念

#### 场景:矩阵为模板提供示例映射
- **当** 风格矩阵记录某个 row template
- **那么** 必须至少提供一个当前 menuconfig 中的正式示例或字段映射
- **并且** 使开发者能够从模板目录直接找到“什么场景该复用这个模板”

### 需求:风格矩阵必须维护 group template 合同
对于互斥单选、多候选 picker 或其他需要跨多行共同表达一个交互语义的场景，`menuconfig` 风格矩阵必须维护正式的 group template 合同，而不是只记录单行长相。该 group template 必须定义组选中约束、组内行模板组合、切换键位与进入详情的边界。

#### 场景:互斥单选组必须有正式组语义
- **当** `menuconfig` 渲染 `none / password / private-key` 或等价的互斥候选组
- **那么** 风格矩阵必须明确记录该组属于 `exclusive choice group` 或等价 group template
- **并且** 必须记录任意时刻有且仅有一个选项被选中

#### 场景:带详情的互斥选项组必须记录 Space 与 Enter 分工
- **当** 某个 group template 中的选项允许在选中后进入详情编辑
- **那么** 风格矩阵必须明确记录 `Space` 负责切换组内选择
- **并且** 必须明确记录 `Enter` 仅用于进入当前已选项的详情，而不得把 `Enter` 解释为切换选择

### 需求:风格矩阵必须维护正式的 popup template catalog
BridgingIO 的 `menuconfig` 风格矩阵除了记录 popup matrix 外，还必须维护一套正式的 popup template catalog，用于说明各类弹框和 overlay 的固定结构、焦点体、按钮布局、输入语义、等待/结果语义和关闭规则。系统不得继续仅依赖“这个弹框看起来和另一个差不多”的实现习惯来扩展 popup。

#### 场景:新增弹框时先选择 popup template
- **当** 团队为 `menuconfig` 新增一个输入弹框、确认弹框、结果弹框、帮助弹框或其他 overlay
- **那么** 风格矩阵必须先说明该弹框属于哪个正式 popup template
- **并且** 必须记录其标题、正文、按钮区、输入区或候选区分别由哪些控件原语组成

#### 场景:popup template 至少覆盖当前常见弹框家族
- **当** 风格矩阵维护 popup template catalog
- **那么** 目录中至少必须区分 `message modal`、`confirm modal`、`text-input modal`、`choice-list modal`、`waiting modal`、`result modal`、`one-time reveal modal` 与 `blocking overlay`
- **并且** 不得把等待态、结果态和一次性 reveal 都继续视为同一类未命名弹框

#### 场景:popup template 为底层控件原语提供映射
- **当** 风格矩阵记录某个 popup template
- **那么** 必须明确它是否使用共享 `modal shell`、`button row`、`input line`、`choice list` 等控件原语
- **并且** 使后续实现能够在不复制布局代码的前提下复用这些控件

### 需求:风格矩阵必须正式建模阻断态与强制只读态模板
当 `menuconfig` 某个动作因前置条件不满足而暂时不可进入，或某个状态被系统/策略强制锁定时，风格矩阵必须使用正式模板来表达该语义，而不是继续把这些场景笼统退化成普通 `info row`。阻断态与强制态必须在视觉和交互上可区分于普通说明行与可操作行。

#### 场景:阻断态动作使用正式 blocked template
- **当** 某个动作存在正式目标，但当前因必填输入未完成、校验未通过或状态不满足而不可进入
- **那么** 风格矩阵必须记录该行使用 `blocked action row` 或等价模板
- **并且** 必须记录该模板直接展示阻断原因
- **并且** 不得把它误记录成普通 action row

#### 场景:强制开启状态使用正式 required-readonly template
- **当** 某个状态被系统或安全策略强制固定为开启
- **那么** 风格矩阵必须记录该行使用 `required readonly row`、`fixed enabled row` 或等价只读模板
- **并且** 不得把该状态渲染成仍可切换的 toggle

### 需求:风格矩阵必须记录 Core Settings 主入口与 Cache/HTTP 子菜单映射
风格矩阵必须把基础配置入口记录为 `Core Settings`，并明确 `Cache Settings` 与 `HTTP Interface Settings` 是其下的二级导航语义。矩阵不得继续把 `Storage` 与 `Model Plane` 记录为并列一级入口真相。

#### 场景:矩阵记录主菜单入口变化
- **当** 风格矩阵更新主菜单映射
- **那么** 基础配置入口必须记录为单一 `Core Settings`
- **并且** `Storage` 与 `Model Plane` 不得继续作为一级导航入口出现

#### 场景:矩阵记录 Core 页面子菜单动作行
- **当** 风格矩阵记录 `Core Settings` 页面的导航行
- **那么** `Cache Settings --->` 与 `HTTP Interface Settings --->` 必须记录为 `action-row` 或等价模板
- **并且** 缓存字段与 HTTP 字段必须映射到各自子菜单页面

### 需求:风格矩阵必须记录 canonical 值与 localized display 值分离
对于 `menuconfig` 中采用枚举候选值的字段，风格矩阵必须明确记录“显示值可本地化、持久化值保持 canonical”的合同，并要求字段行和 choice popup 使用同一套 display 映射。

#### 场景:矩阵记录缓存 backend 的 display/persist 合同
- **当** 风格矩阵记录 `storage.artifacts.backend`
- **那么** 必须明确 `memory` / `filesystem` 是配置持久化值
- **并且** 必须明确 `Memory` / `Filesystem`（及其本地化等价）是 operator-facing 显示值

#### 场景:矩阵要求字段行与 choice popup 显示一致
- **当** 某枚举字段支持 choice popup
- **那么** 当前值行内显示和 popup 候选显示必须使用一致的本地化映射
- **并且** 不得出现“行内已本地化、popup 仍显示 canonical 枚举”的不一致状态
