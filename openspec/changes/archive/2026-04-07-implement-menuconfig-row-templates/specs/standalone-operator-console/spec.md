## 新增需求

### 需求:当前 menuconfig 中可正式映射的行必须迁移到模板组件
当 `menuconfig` 的某一类现有行已经能够与 `MENUCONFIG_STYLE_MATRIX.md` 中的正式 row template catalog 一对一映射时，`bridgingio-core menuconfig` 必须通过共享模板组件构建该行，而不是继续只依赖 `field` 路径、`ActionKind` 分支或零散 helper 在渲染阶段猜测其语义。系统必须把模板组件落实到当前代码，而不只是保留为未来新增页面的建议。

#### 场景:现有基础行族切换到模板组件
- **当** 当前 menuconfig 页面渲染 `info-row`、`field-entry-row`、`action-row`、`boolean-toggle-row`、`multi-select-row`、`exclusive-choice-row`、`exclusive-choice-entry-row`、`blocked-action-row`、`required-readonly-row` 或 `fixed-enabled-row`
- **那么** 系统必须通过共享模板组件构建这些行
- **并且** screen 层不得继续仅通过 `nav_entry`、`edit_entry`、`action_entry`、`info_entry` 或等价基础 helper 间接表达这些模板语义

#### 场景:模板组件保留动态 payload 能力
- **当** 某个 row template 需要展示动态 label、value、selected 状态、blocked reason、required value 或运行时 description
- **那么** 系统必须允许 screen 通过模板组件注入这些动态 payload
- **并且** 不得要求 screen 回退到直接拼装最终显示字符串才能表达动态差异

#### 场景:未正式建模的复合行维持受控例外
- **当** 某个当前行无法与现有正式 row template catalog 一对一映射
- **那么** 系统可以暂时保留受控兼容路径
- **并且** 该例外必须被明确记录为“尚无正式模板”的复合行，而不得把本应已迁移的普通行继续混在例外范围里

### 需求:当前高频输入与选择弹窗必须迁移到 popup template 组件
当 `menuconfig` 的某一类现有弹窗已经能够与 `MENUCONFIG_STYLE_MATRIX.md` 中的正式 popup template catalog 一对一映射时，`bridgingio-core menuconfig` 必须通过共享 popup template 组件构建该弹窗，而不是继续让不同流程各自复制布局、cursor、候选选中态和键位语义。本次至少必须覆盖 `text-input modal` 与 `choice-list modal`。

#### 场景:现有 text-input modal 切换到模板组件
- **当** 当前 menuconfig 流程打开普通字段编辑输入、SSH timeout 输入或等价的单行文本输入弹窗
- **那么** 系统必须通过共享 `text-input modal` 组件构建该弹窗
- **并且** 其标题、正文、input prefix、cursor 与 `Enter/Esc` 语义必须由模板组件统一派生

#### 场景:现有 choice-list modal 切换到模板组件
- **当** 当前 menuconfig 流程打开 locale、backend、trigger policy 或其他枚举/候选选择弹窗
- **那么** 系统必须通过共享 `choice-list modal` 组件构建该弹窗
- **并且** 其候选列表、当前选中态与 `Up/Down/Enter/Space/Esc` 语义必须由模板组件统一派生

## 修改需求

### 需求:menuconfig 必须通过共享模板层构建行语义
`bridgingio-core menuconfig` 在构建菜单页面时，必须通过共享的 row template / group template / popup template 语义层来声明每一行、每一组候选和每一种 overlay 的交互角色，而不是继续由各个 screen 自由拼装前缀、后缀、箭头、按钮区和 `selected/unselected` 状态。模板层必须成为 screen 数据与最终渲染之间的正式边界，并在现有可迁移页面中真正落地为模板组件。

#### 场景:当前实现不再通过 field 或 action 分支猜模板
- **当** 某行的视觉与交互语义已经由正式 row template 覆盖
- **那么** renderer 与按键分发不得继续把 `field` 名称、`ActionKind` 分支或 label 特判作为模板真相源
- **并且** 必须由模板组件统一派生其前缀、`--->`、可聚焦性与按键语义

#### 场景:当前实现不再为 input / choice popup 复制布局合同
- **当** 某个弹窗的视觉与交互语义已经由 `text-input modal` 或 `choice-list modal` 覆盖
- **那么** 系统不得继续在具体流程里复制 `centered rect + clear + block + hint + input line/choice list` 的布局代码
- **并且** 必须由 popup template 组件统一派生其结构与键位语义

## 移除需求
