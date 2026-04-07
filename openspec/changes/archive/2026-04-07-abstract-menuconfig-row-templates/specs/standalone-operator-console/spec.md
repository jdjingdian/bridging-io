## 新增需求

### 需求:menuconfig 必须通过共享模板层构建行语义
`bridgingio-core menuconfig` 在构建菜单页面时，必须通过共享的 row template / group template / popup template 语义层来声明每一行、每一组候选和每一种 overlay 的交互角色，而不是继续由各个 screen 自由拼装前缀、后缀、箭头、按钮区和 `selected/unselected` 状态。模板层必须成为 screen 数据与最终渲染之间的正式边界。

#### 场景:新增页面行时声明模板语义
- **当** 团队为某个 menuconfig screen 新增一行
- **那么** 该 screen 必须先声明该行属于哪个正式模板
- **并且** 模板层必须统一派生其前缀、`--->`、可聚焦性与按键语义

#### 场景:screen 不得手工拼装模板语义
- **当** 某行的视觉或交互语义已经被 row template 覆盖
- **那么** screen 层不得再直接手工拼装 `<*>`、`[ ]`、`--->` 或等价字符串来表达核心交互语义
- **并且** 不得仅依赖字段路径或 action 分支猜测该行“看起来像哪个模板”

#### 场景:popup 不得各自手工复制通用布局
- **当** 某个弹框的结构已经被 popup template 和控件原语覆盖
- **那么** 系统不得继续为其单独复制 `centered rect + clear + block + hint + button row/input line` 等通用布局代码
- **并且** 必须优先复用共享 `modal shell`、`button row`、`input line`、`choice list` 或等价原语

### 需求:纯开关与互斥单选必须使用不同模板
`menuconfig` 必须将“单个布尔值的纯开关”和“多个候选中只允许一个被选中”的互斥单选明确建模为不同模板，而不是继续共享同一含混术语。系统必须确保二者在设计、实现和测试中都能够被独立识别。

#### 场景:纯开关使用 boolean toggle template
- **当** 某个配置项本身只有 `on/off` 语义，且不属于一组互斥候选
- **那么** 系统必须使用 `boolean toggle` 或等价模板
- **并且** 该模板不得要求组内互斥语义

#### 场景:互斥候选项使用 exclusive choice template
- **当** 某个 screen 暴露多个候选项，且任意时刻只允许一个被选中
- **那么** 系统必须使用 `exclusive choice` 或 `exclusive choice entry` 模板以及对应的 group template
- **并且** 必须保证组内恰好一个候选被选中

### 需求:阻断态与强制态必须使用正式模板
`menuconfig` 对于“目标动作存在但当前不可进入”的阻断场景，以及“状态被系统强制固定”的只读场景，必须使用正式模板表达，而不是继续退化成普通说明行或伪装成交互开关。

#### 场景:继续进入详情编辑的门控使用 blocked template
- **当** `SSH Authentication Setup` 或其他流程中的后续动作因为前置条件不满足而不可继续
- **那么** 系统必须把该行建模为 `blocked action` 或等价模板
- **并且** 文案必须直接说明阻断原因

#### 场景:强制安全状态使用 required readonly template
- **当** 某个目标状态被策略要求始终开启，例如 sealed secret-backed auth 的 `SSH 安全访问`
- **那么** 系统必须使用 `required readonly` 或等价模板
- **并且** 不得将其继续建模为可通过 `Space` 切换的 toggle

### 需求:menuconfig 必须通过统一 overlay model 管理弹框
`menuconfig` 对于确认、输入、等待、结果、帮助、一次性 reveal 和 blocking overlay 等弹框，不应继续通过多个互不相干的布尔值、`Option` 或流程状态字段分别管理。系统必须具备统一 overlay model 的正式设计方向，使新增弹框能够复用共享优先级、关闭规则和渲染入口。

#### 场景:新增 overlay 时不再扩展硬编码优先级链
- **当** 团队为 `menuconfig` 新增一种 popup 或 overlay
- **那么** 该设计必须能够映射到统一 overlay model
- **并且** 不得要求再往事件循环里继续硬编码追加一段新的“如果某字段存在就先处理它”的优先级分支

#### 场景:等待态和结果态属于正式 overlay 模型
- **当** `menuconfig` 进入 unlock waiting、SSH test waiting、unlock result、SSH test result 或等价中间态/结果态
- **那么** 系统必须把这些状态视为正式 overlay template 的实例
- **并且** 不得把它们继续视为与普通 popup 毫无关系的特殊流程分支

## 修改需求

### 需求:可编辑项与菜单项视觉语义必须统一并可预测
`menuconfig` 的行内视觉语义必须遵守统一的风格合同，使操作者能够从一行内容直接判断“当前是否可聚焦、是否可切换、是否可进入下一级、当前状态是什么”。系统必须把焦点标识、语义前缀、正文与导航后缀拆分为稳定的固定列，并通过正式的 row template / group template 语义层统一派生，而不得由各个页面自由拼装。

#### 场景:页面通过模板层派生统一行语义
- **当** 操作员进入任一 menuconfig 页面并浏览其中的可编辑项、动作项或状态项
- **那么** 系统必须通过共享模板层派生这些行的视觉与交互语义
- **并且** 不得因为页面作者自行拼装字符串而让同类行出现不同的前缀、箭头或键位行为

## 移除需求
