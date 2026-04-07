## 上下文

当前仓库已经有 menuconfig 的正式风格矩阵，也有统一的 `MenuEntry` / `MenuEntryKind` 和共享渲染函数；但这些抽象主要解决了“有没有统一出口”，还没有解决“每一行的语义是否有正式模板”，也没有解决“popup / overlay 是否有正式控件模板”。

现在的主要缺口有四个：

1. 行语义分层还不够清楚
- `Single-choice` 同时覆盖了纯开关和互斥单选组中的单个选项
- 一些行是“切换 + 进入详情”的复合语义，但目前仍靠具体 action 分支处理

2. 组语义没有正式建模
- 例如 SSH auth 的 `none / password / private-key`
- 例如 vault key picker 的“多个候选里只能选一个”
- 这些都属于同一类“exclusive choice group”，但当前没有组级模板

3. 阻断态和强制态还缺正式模板
- `Continue To Detail Editor` 这类“当前不可进入，但必须说明原因”的行，目前更像临时降级成 info row
- `SSH Secure Access = required` 这类“强制开启且只读”的行，虽然在矩阵里有要求，但缺少明确模板命名和复用方式

4. popup / overlay 仍停留在“局部复用函数 + 分散状态字段”
- 当前已经有 `centered_rect`、`render_popup_button_line`、`render_choice_option_line` 这类局部原语
- 但弹框状态分散在 `edit_mode`、`exit_confirm_mode`、`confirm_action`、`ssh_auth_block_popup`、`token_reveal`、`unlock_flow_state`、`ssh_test_flow_state`
- 事件分发依赖显式优先级链，而不是统一 overlay model
- 多个 popup 在结构上高度相似，但仍各自手写布局与按键处理

这导致现在的规范落点主要在文档，而不是体现在“开发者必须先选模板”的结构上。

## 目标 / 非目标

**目标：**
- 为 menuconfig 定义一套正式的 row template vocabulary，而不再只停留在字符串语法层。
- 为 menuconfig 定义 popup template vocabulary 和可复用控件原语。
- 清晰区分纯开关、互斥单选、带后续编辑的互斥单选项、多选项、字段入口、动作入口、阻断态和只读强制态。
- 清晰区分 message modal、confirm modal、text-input modal、choice-list modal、waiting modal、result modal、one-time reveal modal 与 blocking overlay。
- 为互斥单选引入组级模板概念，减少各页面各自实现互斥逻辑和样式。
- 为弹框引入统一 overlay state 的设计方向，减少事件优先级链和重复布局代码。
- 让 future menuconfig feature work 在 review 时可以回答“用了哪个模板”，而不是只回答“看起来像矩阵里的某一行”。
- 为后续实现提供明确的迁移路径和测试边界。

**非目标：**
- 本次不直接重构 `bridgingio-operator-console` 的 Rust 实现。
- 本次不重新设计 menuconfig 的信息架构、页面层级或按键体系。
- 本次不改变现有文案内容或本地化 catalog 结构。
- 本次不把 popup 系统直接重构为另一套组件框架，但会把值得模板化的边界和方向写清楚。

## 决策

### 决策 1：从“行语法”升级到“行模板”

后续 menuconfig 设计不应只描述某一行看起来像什么，而应先声明它属于哪个模板，再由模板派生：
- 前缀
- 是否带 `--->`
- 是否可聚焦
- `Space` / `Enter` / `Esc` 的语义
- 选中态与降级态
- 是否需要组内互斥

建议最小模板集合如下：

| 模板 | 用途 |
| --- | --- |
| `info-row` | 普通只读说明或状态投影 |
| `fixed-disabled-row` | 固定关闭、不可交互 |
| `fixed-enabled-row` | 固定开启、不可交互 |
| `boolean-toggle-row` | 纯开关，只有 `on/off`，无后续页面 |
| `exclusive-choice-row` | 多个候选项中的一个，组内互斥，无后续页面 |
| `exclusive-choice-entry-row` | 组内互斥，且选中后允许 `Enter` 进入详情编辑 |
| `multi-select-row` | `[ ] / [*]` 多选项 |
| `field-entry-row` | `Label (value) --->` 的字段编辑入口 |
| `action-row` | `Label --->` 的流程入口或动作入口 |
| `blocked-action-row` | 当前不可进入，但必须直接说明阻断原因 |
| `required-readonly-row` | 强制状态、只读且不可交互 |

这套模板比当前“Single-choice / Multi-select / Action / Info”更适合约束未来开发。

### 决策 2：正式引入 group template，而不是只定义单行模板

仅有单行模板还不够，因为互斥单选的真正语义属于“组”，而不是“某一行”。

建议至少定义：
- `exclusive-choice-group`
  - 任意时刻必须且只能有一个选项被选中
  - 组内选项必须共享同一套 marker 语义
  - 若某些选项支持 `Enter` 进入详情，则必须通过 `exclusive-choice-entry-row` 明确表达
- `selection-picker-group`
  - 与 `exclusive-choice-group` 同属互斥选择，但强调其结果是绑定某个引用对象，而不是切换配置模式

这样可以把以下场景统一到一类模板：
- SSH auth kind 选择
- imported vault key picker
- 后续可能出现的单 profile / 单 transport / 单 backend 选择器

### 决策 3：把“纯开关”和“互斥单选项”从术语上拆开

这是本次最重要的语义修正。

建议后续文档与实现统一使用：
- `Boolean Toggle`
  - 一个配置本身只有 `true/false`
  - 例如 `Allow Non Loopback`
  - 例如 `Token Access Switch`
- `Exclusive Choice`
  - 多个候选中只允许一个被选中
  - 例如 `none / password / private-key`
  - 例如 “从多个已导入 key 里选一个”

这样可以避免后续再把“单个布尔值”和“一组互斥候选项”都塞回 `Single-choice`。

### 决策 4：为阻断态和强制态建立单独模板，而不是继续借道 `info-row`

当前缺的不是“能不能显示一行文字”，而是“这行文字在流程中的角色是否清楚”。

建议：
- `blocked-action-row`
  - 用于“目标动作存在，但当前由于前置条件不满足而不可进入”
  - 文案必须直接包含阻断原因
  - 不可聚焦，防止误导为弱可点态
- `required-readonly-row`
  - 用于“状态被系统或安全策略强制锁定”
  - 显示为只读结果，不伪装为 toggle

这会直接覆盖两个目前容易漂移的点：
- `Continue To Detail Editor` 的阻断行
- `SSH Secure Access = required` 的强制只读行

### 决策 5：popup 也应从“矩阵类型”升级到“模板目录”

当前代码已经暴露出若干可复用 popup 家族：

| popup template | 当前对应场景 |
| --- | --- |
| `message-modal` | help、ssh auth block、token reveal、resize guard |
| `confirm-modal` | exit confirm、delete/revoke/风险确认 |
| `text-input-modal` | 普通编辑弹框、SSH timeout 输入 |
| `choice-list-modal` | enum/choice 编辑弹框 |
| `waiting-modal` | unlock waiting、SSH test waiting |
| `result-modal` | unlock success/failed、SSH test result |
| `blocking-overlay` | resize guard、等待态 |

并且可以抽出更底层的控件原语：
- `modal-shell`
  - centered rect
  - `Clear`
  - bordered block
  - title + body
- `button-row`
  - 水平按钮组
  - 当前已有共享 `render_popup_button_line`
- `choice-list`
  - 纵向候选列表
  - 当前已有共享 `render_choice_option_line`
- `input-line`
  - prefix + value + cursor
  - 当前在 edit popup 和 SSH timeout popup 中各写一份

因此建议后续把 popup contract 收敛成两层：
- popup template
  - 负责“这个弹框是什么类型”
- popup primitive
  - 负责“按钮行 / 输入行 / 选择列表 / modal shell 怎么统一渲染”

### 决策 6：overlay state 应有统一模型，而不是继续用分散字段串优先级

当前事件循环里，popup/overlay 优先级是硬编码链式分发。

建议后续实现阶段考虑统一为：
- `OverlayState::None`
- `OverlayState::Help(...)`
- `OverlayState::Confirm(...)`
- `OverlayState::TextInput(...)`
- `OverlayState::ChoiceList(...)`
- `OverlayState::Waiting(...)`
- `OverlayState::Result(...)`
- `OverlayState::Reveal(...)`
- `OverlayState::Blocking(...)`

本次先把它记为设计方向，不要求立即改实现。

### 决策 7：模板应拥有自己的 review checklist 和映射表

仅定义模板名称还不够，必须把“怎么用”写成 review checklist。

建议每次新增 menuconfig 行时，评审至少回答：
- 这行属于哪个 `row template`
- 是否属于某个 `group template`
- 它的键位语义来自模板还是页面自定义
- 它是可交互态、阻断态还是强制只读态
- 是否已经能在矩阵中找到对应模板映射

建议在风格矩阵中增加一张 “Template Catalog / Template Mapping” 表，而不只保留 “Row Grammar Matrix”。

### 决策 8：模板合同必须配套模板级测试，而不是只做页面级 smoke test

如果只测页面存在与否，模板层仍然容易被绕过。

后续实现阶段建议至少覆盖：
- 每种 row template 的渲染合同
- `Boolean Toggle` 不得出现 `--->`
- `Exclusive Choice Entry` 允许 `Space` 切换、`Enter` 进入详情
- `blocked-action-row` 不可聚焦且必须显示阻断原因
- `required-readonly-row` 不得响应切换按键
- `exclusive-choice-group` 必须维持“恰好一个选中”
- 每种 popup template 的布局与键位合同
- `text-input-modal` 的 cursor 行为
- `confirm-modal` 的按钮焦点与确认/取消语义
- `waiting-modal` 的阻塞性和可取消性
- `result-modal` / `reveal-modal` 的 acknowledge 行为

## 风险 / 权衡

- [风险] 模板类别定义过多，初期看起来比现在更重。
  -> 缓解：先收敛最小模板集，只覆盖已经在实现中出现的模式。

- [风险] 若只更新 spec 不跟进实现，短期内仍可能继续漂移。
  -> 缓解：把模板级测试要求一并写进 spec，后续实现时优先补护栏。

- [权衡] 从“字符串语法”升级到“模板语义”会提高前期设计成本。
  -> 但这正是为了降低后续每新增一个页面都重新判断样式的成本。

- [权衡] 一些现有复合行可能需要重新归类。
  -> 例如 target 列表中的 `<*> Label --->` 更像“状态 + 导航复合模板”，后续实现时可根据迁移结果决定是否把它提升为单独模板。

## 迁移计划

1. 先在 OpenSpec 中引入 row template、group template、popup template 和测试要求。
2. 再在 `docs/matrix/MENUCONFIG_STYLE_MATRIX.md` 中补齐模板表与模板映射。
3. 后续实现阶段在 `bridgingio-operator-console` 中增加模板语义层，让 screen 先产出模板化数据，再统一渲染。
4. 迁移现有高风险场景作为第一批验证对象：
   - `Allow Non Loopback`
   - Token Access Switch
   - SSH Authentication Setup
   - Credential Picker
   - `Continue To Detail Editor`
   - `SSH Secure Access = required`
   - 普通编辑弹框
   - SSH timeout 输入弹框
   - delete/revoke/风险确认弹框
   - unlock waiting / success / failed
   - SSH test waiting / result

## 开放问题

- target 列表中的 `enabled + navigate` 复合行是否应抽成独立模板，例如 `status-entry-row`。
- 阻断态是否需要保留“可聚焦但不可执行”的次级模板，用于强调原因读取；当前建议先保持不可聚焦。
- popup 是否也要在下一阶段采用同样的 template catalog 方式建模，以减少等待态 / 结果态 / confirm popup 的实现偏移。
  当前建议答案是“要”，而且优先级不低。

## 设计建议与当前可能缺漏

除了这次明确提到的单选项模板，我认为还有几个设计缺漏值得一并记录：

1. 缺少“组级语义”真相源
- 现在矩阵更强调单行，没有正式描述“哪些行必须一起看待”

2. 缺少“阻断态”作为正式 UI 类型
- 现在很多阻断原因容易退化成普通 info，导致视觉与流程角色变弱

3. 缺少“模板 ID -> 示例 -> 适用场景”的映射表
- 规范有语法，但开发者仍缺“我该选哪个模板”的入口

4. 缺少“禁止手拼样式”的工程护栏
- 未来实现阶段最好让 screen 只能产出模板化结构，而不是自由拼 `<*>`、`--->` 和 `selected/unselected`

5. 缺少模板级回归测试口径
- 这会让规范仍停留在文档，而不是落到 CI 护栏

6. 缺少 popup primitive 的统一边界
- 当前已经有共享按钮行和 choice 行，但 `modal shell`、`input line`、`acknowledge modal` 仍未正式抽象

7. 缺少统一 overlay state
- 现在新增一个弹框，往往意味着再加一个字段、一个渲染函数和一段事件优先级判断
