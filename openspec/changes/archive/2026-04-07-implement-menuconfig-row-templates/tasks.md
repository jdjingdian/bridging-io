## 1. 抽象模板组件

- [x] 1.1 盘点当前 `menuconfig` 行族，列出哪些已经能一对一映射到 `MENUCONFIG_STYLE_MATRIX.md` 中的正式 row template，哪些仍属于受控例外
- [x] 1.2 在 `bridgingio-operator-console` 中引入正式的 row template 数据模型与 builder API，使 screen 能先声明模板，再填充动态 payload 和交互目标
- [x] 1.3 在 `bridgingio-operator-console` 中引入正式的 popup template 数据模型与 builder API，至少覆盖 `text-input modal` 与 `choice-list modal`
- [x] 1.4 让 renderer、focusability 和按键分发以模板语义为主驱动，不再把 `field` 名称和 `ActionKind` 分支作为模板真相源，并让 input / choice popup 的布局与键位由 popup template 驱动
- [x] 1.5 保留对未正式模板化复合行和未纳入本次范围的 overlay 的最小兼容路径，并把例外范围记录清楚，避免后续继续扩大

## 2. 迁移当前 menuconfig 代码

- [x] 2.1 将 core / storage / model plane / vault / security / token 页面中所有可直接映射到 `info-row`、`field-entry-row`、`action-row`、`boolean-toggle-row`、`fixed-enabled-row` 的行切换到模板组件方式
- [x] 2.2 将 target 相关页面中所有可直接映射到 `multi-select-row`、`exclusive-choice-row`、`exclusive-choice-entry-row`、`blocked-action-row`、`required-readonly-row` 的行切换到模板组件方式
- [x] 2.3 将当前普通字段编辑、SSH timeout 输入以及枚举选择弹窗中所有可直接映射到 `text-input modal`、`choice-list modal` 的流程切换到 popup template 组件方式
- [x] 2.4 收口当前依赖 `is_boolean_toggle_field`、`is_single_choice_toggle_field`、`ActionKind::*` 特判来推断模板语义的渲染/格式化分支，并收口输入 / 选择弹窗的重复布局逻辑
- [x] 2.5 为模板合同和迁移完整性补充自动化测试，确保“所有可迁移项已切到模板组件，只有已登记例外仍走兼容路径”
