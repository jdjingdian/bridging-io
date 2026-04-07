## 新增需求

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

## 修改需求

### 需求:`menuconfig` 必须维护正式的风格矩阵文档
BridgingIO 必须维护一份正式的 `menuconfig` 风格矩阵文档，作为 `bridgingio-core menuconfig` 及未来复用同一 menuconfig 语法的本地 UI/TUI host 的风格真相源。该矩阵必须记录布局区域、行语法、row template、group template、popup template、可聚焦性、键位语义、popup 样式、fallback 规则、最小视口与溢出裁剪，而不是继续让这些约定散落在代码与零散 spec 中。

#### 场景:风格矩阵升级为模板化真相源
- **当** 团队维护或审查 `menuconfig` 风格矩阵文档
- **那么** 该文档必须同时记录字符串语法与正式模板目录
- **并且** 不得继续把 row template / group template / popup template 留在实现细节或设计口头约定中

## 移除需求
