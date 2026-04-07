## 为什么

当前 `menuconfig` 已经有正式的风格矩阵和统一渲染入口，但还缺少一层可复用的“模板语义层”。

现状更接近：
- 文档层定义了行语法与交互规范
- 实现层提供了 `MenuEntry`、`MenuEntryKind` 和统一渲染函数
- 各个 screen 仍然需要自己决定某一行到底是 toggle、单选项、带后续编辑的单选项，还是只读阻断提示

这会带来两个问题：
- “纯开关”与“互斥单选组中的一个选项”都被归入笼统的 `Single-choice`，语义边界不够清楚
- 新增 menuconfig 页面或流程时，开发者容易直接拼装字符串、前缀和 `ActionKind` 分支，导致实现偶尔偏离矩阵

尤其是下面几类行，目前已经显示出需要模板化抽象：
- 纯单选开关：仅表达 `on/off`，没有组选项语义
- 互斥单选项：多个选项里任意时刻只允许一个被选中
- 带后续编辑的互斥单选项：`Space` 负责切换，`Enter` 负责进入详情
- 阻断态动作：视觉上仍与目标动作有关，但当前不可进入，且需要直接显示原因
- 强制只读态：例如 required/forced-on 的安全状态，不应再伪装成交互控件

继续对代码做细看后，还能看到 popup / overlay 这一层也有同样的抽象缺口：
- 退出确认、删除确认、风险确认本质上都属于 confirm modal
- 普通文本编辑弹框和 SSH timeout 输入弹框本质上都属于 text-input modal
- enum/choice 选择器本质上属于 choice-list modal
- unlock waiting / success / failed、SSH test waiting / result、SSH auth block、token reveal 都属于 message / result / acknowledge modal 家族
- 当前虽已有少量共享函数，但还没有正式的 popup template catalog 和 overlay state 设计

因此，这次变更先不实现 Rust 代码，而是先把 menuconfig 的模板模型设计清楚，形成后续实现和 review 的真正规范抓手。

## 变更内容

- 为 `menuconfig` 设计正式的 row template / group template / popup template 语义层。
- 将当前笼统的 `Single-choice` 语义拆分为更清晰的模板类别，至少区分：
  - 纯开关型单选
  - 互斥选择型单选
  - 带后续编辑的互斥选择项
- 为“阻断但需解释原因”的场景补充正式模板，而不是继续退化成普通 `info row`。
- 为“强制开启且只读”的场景补充正式模板，避免视觉和交互语义混淆。
- 为输入弹框、确认弹框、等待弹框、结果弹框、帮助弹框和一次性 reveal 弹框补充正式 popup template。
- 明确 `modal shell`、`button row`、`choice list`、`input line` 等可复用控件原语。
- 明确未来新增 menuconfig 行时，必须先选模板，再填数据，而不是直接手拼前缀和后缀。
- 明确未来新增 menuconfig popup 时，必须先选 popup template，再填标题、文案、按钮、输入或候选内容。
- 补充模板级测试与回归要求，避免后续实现再次偏离规范。

本次变更只产出 proposal、design、tasks 和 spec deltas，不直接修改 menuconfig 代码实现。

## 功能 (Capabilities)

### 新增功能
- 无

### 修改功能
- `menuconfig-style-matrix`: 增加 row / popup template catalog 与语义分层要求
- `standalone-operator-console`: 增加 menuconfig 需通过共享模板层构建行与弹框语义的要求
- `quality-and-test-automation`: 增加 menuconfig 模板合同级回归覆盖要求

## 影响

- 受影响模块：`bridgingio-operator-console` 的菜单行建模、popup/overlay 建模、渲染入口、交互分发与测试结构
- 受影响文档：`docs/matrix/MENUCONFIG_STYLE_MATRIX.md` 以及相关 OpenSpec 能力规格
- 受影响流程：后续新增 menuconfig 页面时的设计评审、实现路径和验收方式
- 受影响风险：可降低“样式矩阵存在，但实现仍时不时跑偏”的漂移风险
