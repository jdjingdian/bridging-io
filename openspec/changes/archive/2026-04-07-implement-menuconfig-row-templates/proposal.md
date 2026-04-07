## 为什么

`docs/matrix/MENUCONFIG_STYLE_MATRIX.md` 已经把 `menuconfig` 的 row template catalog
定义清楚，归档变更 `abstract-menuconfig-row-templates` 也已经把模板语义、测试口径和迁移方向补进了
OpenSpec；但当前 Rust 实现还没有真正落到“模板组件层”。

现在的实现仍然主要依赖以下方式拼出行语义：

- `MenuEntryKind` 只区分 `Navigate / EditField / FocusField / Action / Info`
- `format_menu_entry_line` 继续根据 `field` 名称和 `ActionKind` 分支推断
  `boolean toggle`、`exclusive choice`、`blocked action`、`required readonly`
- `nav_entry`、`edit_entry`、`action_entry`、`info_entry` 这类 helper 只能表达“做什么”，
不能直接表达“这一行属于哪个正式模板”
- 若某一类行需要特殊样式，当前仍靠 label/field/action 的特殊分支修补
- `render_edit_popup` 与 choice 编辑弹窗也仍然是“局部共享 + 局部手写”
  的混合状态，缺少正式的 input / choice popup 模板边界

这会让风格矩阵虽然已经存在，但实现层仍然缺少一个强约束边界：

- 后续页面很容易继续通过字符串拼装或 action 分支“长得像模板”，而不是“正式使用模板”
- 当前能明显映射到模板目录的行，仍然没有形成稳定的可复用构件
- review 很难直接回答“这行是否通过模板组件构建”，只能继续读渲染分支猜语义

尤其是下面这些家族，已经具备非常明确的模板化收益：

- `boolean-toggle-row`
  - `Allow Non Loopback`
  - Token detail `Access Switch`
  - plain SSH `SSH Secure Access`
- `multi-select-row`
  - `targets[*].enabled`
- `exclusive-choice-row` / `exclusive-choice-entry-row`
  - SSH Authentication `none / password / private-key`
  - imported vault key picker
- `blocked-action-row`
  - `Continue To Detail Editor (blocked: ...)`
- `required-readonly-row`
  - sealed SSH `SSH Secure Access = required`
- `fixed-enabled-row`
  - Security `Credential Management Unlocked`
- `field-entry-row` / `action-row` / `info-row`
  - 绝大多数 core/storage/model/vault/target/security/token 页面
- `text-input-modal`
  - 普通字段编辑弹窗
  - SSH timeout 输入弹窗
  - Token 创建 label / expiry 输入
- `choice-list-modal`
  - enum / choice 字段编辑弹窗
  - trigger policy / locale / backend 等候选弹窗

因此，这次需要开启一个新的实现型 change，把“模板目录”真正变成
`bridgingio-operator-console` 中可复用、可迁移、可测试的模板组件层，并且把最常用的输入弹窗与选择弹窗一起纳入模板化范围。

## 变更内容

- 为 `menuconfig` 引入正式的 row template 组件层，使 screen 先声明模板语义，再填充动态数据。
- 为 `menuconfig` 引入正式的 popup template 组件层，至少覆盖 `text-input modal`
  与 `choice-list modal` 两类高频弹窗。
- 在模板组件中保留必要的动态能力，至少允许注入：
  - label / value / description
  - 当前选中态或开关态
  - blocked reason / required value / badge
  - `Enter` / `Space` 对应的交互目标
  - 组内唯一选中所需的 group 语义
- 对 popup template 保留必要的动态能力，至少允许注入：
  - title / message / hint
  - input value / cursor / input prefix
  - choice options / selected index
  - confirm or cancel semantics
- 将当前 `menuconfig` 代码里所有能一对一映射到现有 row template catalog 的行，
  统一切换到模板组件方式，而不是继续通过 `field` / `ActionKind` 推断语义。
- 将当前 `menuconfig` 代码里所有能一对一映射到 `text-input modal` 或
  `choice-list modal` 的弹窗，统一切换到 popup template 组件方式，而不是继续散落在各个流程里分别布局。
- 为暂时还没有正式 row template 的复合行保留受控兼容路径，并明确记录例外范围，
  避免“部分迁移 + 大量漏网”的灰色状态。
- 为暂时还未纳入本次范围的 waiting / result / message / reveal / confirm overlay
  保留兼容路径，但新方案不得阻塞它们后续接入 popup template。
- 补充模板层级的渲染、交互和迁移覆盖测试，确保后续新增页面不会绕过模板组件。

本次 change 以 row template 组件化、`text-input modal` / `choice-list modal`
模板化和现有页面迁移为主，不把 popup / overlay 全量重构纳入必做范围。

## 功能 (Capabilities)

### 新增功能
- 无

### 修改功能
- `standalone-operator-console`
- `quality-and-test-automation`

## 影响

- 受影响模块：
  - `source/rust/bridgingio-operator-console/src/lib.rs` 的 menu row 数据模型
  - screen entry builders
  - 行渲染与按键分发
  - 输入弹窗与选择弹窗渲染、状态承载与测试
  - menuconfig 相关测试
- 受影响流程：
  - 后续新增 menuconfig 行时，必须优先复用模板组件
  - review 需要回答“是否已经切到模板组件”而不只是“视觉上像模板”
- 主要收益：
  - 让风格矩阵从文档真相源，升级为实现层的正式边界
  - 降低新增页面继续手拼前缀/箭头/选中态的漂移风险
  - 避免输入弹窗和选择弹窗继续在不同流程里各写一套布局与键位逻辑
  - 为后续继续抽 confirm / waiting / result / message / reveal 模板提供更稳定的前置层
