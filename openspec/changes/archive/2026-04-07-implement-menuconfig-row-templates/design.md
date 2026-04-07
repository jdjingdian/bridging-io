## 上下文

归档变更 `abstract-menuconfig-row-templates` 已经完成了两件关键事情：

1. 在 OpenSpec 和 `MENUCONFIG_STYLE_MATRIX.md` 中定义了正式的 row template / popup template catalog
2. 明确后续实现必须通过共享模板层构建行语义

但当前实现还停留在“统一渲染出口”而非“统一模板语义出口”。

从 `source/rust/bridgingio-operator-console/src/lib.rs` 可以看到几个明显现状：

- `MenuEntryKind` 过粗
  - 只能表达 `Navigate / EditField / FocusField / Action / Info`
  - 不能直接表达 `boolean-toggle-row`、`blocked-action-row`、
    `required-readonly-row` 等模板语义
- `format_menu_entry_line` 仍在做模板猜测
  - 通过 `is_boolean_toggle_field(field)` 推断 toggle
  - 通过 `ActionKind::ToggleTokenAccess` / `BindTargetCredentialRef` /
    `SelectTargetSshAuth*` 推断互斥选择或开关
  - 通过 `Info` + 特定 label/value 推断某些特殊只读样式
- screen builders 仍在直接生产基础 kind
  - `nav_entry`
  - `edit_entry`
  - `action_entry`
  - `info_entry`
- `render_edit_popup` 与 SSH timeout popup 已经显示出同一类 `text-input modal`
  结构，但仍分别承载输入值、cursor 和 layout
- `EditModeKind::Choice` 与 choice 编辑弹窗已经显示出 `choice-list modal`
  结构，但 screen 侧还没有正式的 popup template 组件入口
- disabled / non-focusable 语义仍过于粗糙
  - `menu_entry_is_disabled` 现在只把 `Info` 视为 disabled
  - 这意味着 `blocked action`、`required readonly`、`fixed enabled`
    等模板没有自己的正式交互语义边界

换句话说，现在是：

```text
screen builder
  -> MenuEntryKind
  -> format/render 时再猜它像哪种模板
```

而目标应该变成：

```text
screen builder
  -> 明确选择 row template
  -> 填充动态 payload / interaction
  -> render / key handling 按模板合同工作

popup builder
  -> 明确选择 popup template
  -> 填充动态 payload / interaction
  -> render / key handling 按模板合同工作
```

## 目标 / 非目标

**目标：**

- 在实现层引入正式的 row template 组件边界。
- 在实现层引入正式的 popup template 组件边界，至少覆盖输入和选择两类高频弹窗。
- 保留模板的动态能力，而不是把模板做成只适用于静态文案的死结构。
- 将当前所有能一对一映射到 style matrix row template catalog 的行迁移到模板组件。
- 将当前所有能一对一映射到 `text-input modal` / `choice-list modal` 的弹窗迁移到模板组件。
- 让 renderer、focusability 和 key 语义由模板驱动，而不是由 `field` / `action`
  分支二次推断。
- 让 input / choice popup 的布局、cursor、selected row 和提交/取消语义由 popup template 驱动，而不是由各流程分别组织。
- 为未正式建模的复合行保留受控兼容路径，并显式列出例外。
- 增加模板级和迁移级测试护栏。

**非目标：**

- 本次不重做 `menuconfig` 的页面结构、信息架构或键位体系。
- 本次不要求把 popup / overlay 全量迁移到 popup template 组件；confirm / waiting / result / message / reveal 可后续推进。
- 本次不强行把“当前矩阵里尚未正式命名的复合行”塞进一个不稳定的新模板。
- 本次不拆分 `lib.rs` 文件结构作为单独目标，除非为了模板组件落地有必要做最小重组。

## 当前可迁移对象盘点

当前代码里，下面这些行已经可以直接映射到正式模板目录，适合作为本 change 的完整迁移范围：

| 模板 | 当前行族 |
| --- | --- |
| `info-row` | 各类 summary / locked hint / empty state / missing state |
| `field-entry-row` | core / storage / model / vault / target / token 的普通编辑字段 |
| `action-row` | 导航、创建、删除、导入、测试连接、进入 picker 等动作 |
| `boolean-toggle-row` | `Allow Non Loopback`、Token `Access Switch`、plain `SSH Secure Access` |
| `multi-select-row` | `targets[*].enabled` |
| `exclusive-choice-row` | SSH auth `none`、vault credential picker item |
| `exclusive-choice-entry-row` | SSH auth `password` / `private-key` |
| `blocked-action-row` | `Continue To Detail Editor` 被阻断时 |
| `required-readonly-row` | sealed `SSH Secure Access = required` |
| `fixed-enabled-row` | Security `Credential Management Unlocked` |

当前代码里，下面这些弹窗也已经可以直接映射到正式 popup template，适合作为本 change 的完整迁移范围：

| 模板 | 当前弹窗族 |
| --- | --- |
| `text-input-modal` | 普通字段编辑弹窗、SSH timeout 输入、token create / edit 输入 |
| `choice-list-modal` | 枚举型字段编辑弹窗、locale / backend / trigger policy 等选择弹窗 |

## Popup 迁移清单

为了避免后续实现阶段还要重新盘点一次，本 change 先把当前 input / choice popup
的正式迁移对象写成清单。

### `text-input-modal` 迁移对象

| 当前来源 | 当前入口 | 说明 |
| --- | --- | --- |
| 普通字段编辑 | `begin_edit(field)` 且 `field_options(field) == None` | 通用单行文本/数字输入 |
| SSH Import `key name` | `SSH_IMPORT_KEY_NAME_FIELD` | 导入 SSH key 时的名称输入 |
| SSH Import `label` | `SSH_IMPORT_LABEL_FIELD` | 导入 SSH key 时的显示标签输入 |
| SSH Import `source path` | `SSH_IMPORT_SOURCE_PATH_FIELD` | 导入 SSH key 时的源路径输入 |
| SSH Import `passphrase` | `SSH_IMPORT_PASSPHRASE_FIELD` | 需要 masking 的单行输入 |
| Token Create `label` | `TOKEN_CREATE_LABEL_FIELD` | 创建 token 的首步输入 |
| Token Create `expires-at` | `TOKEN_CREATE_EXPIRY_AT_FIELD` | 指定失效时间输入 |
| Token Label Edit | `__token_label_edit__:{token_id}` | token 详情页备注编辑 |
| SSH timeout 输入 | `SshTestFlowState::TimeoutInput` | `Test Connection` 前置 timeout 输入 |
| SSH auth 详情编辑中的文本字段 | 例如 `targets[*].ssh_auth.password`、`targets[*].ssh_auth.key_locator` | 仍应复用同一 input modal 合同 |

### `choice-list-modal` 迁移对象

| 当前来源 | 当前入口 | 说明 |
| --- | --- | --- |
| Token Create expiry mode | `TOKEN_CREATE_EXPIRY_MODE_FIELD` | `long-lived / expires-at-time` 二选一 |
| 通用 enum/choice 字段 | `begin_edit(field)` 且 `field_options(field) != None` | 统一走候选列表模板 |
| `core.log_level` | `field_options("core.log_level")` | log level 选择 |
| `core.operator_locale` | `field_options("core.operator_locale")` | locale 选择 |
| `storage.artifacts.backend` | `field_options("storage.artifacts.backend")` | backend 选择 |
| `vault.unlock.trigger_policy` | `field_options("vault.unlock.trigger_policy")` | trigger policy 选择 |

### 不纳入本次 popup 迁移清单的对象

| 当前弹窗 | 原因 |
| --- | --- |
| exit confirm / delete confirm / revoke confirm / risk confirm | 属于 `confirm-modal`，应后续单独推进 |
| unlock waiting / unlock success / unlock failed | 属于 `waiting-modal` / `result-modal`，本次不一起收 |
| SSH test waiting / SSH test result | 同上，后续可按 overlay model 继续迁移 |
| help / auth block / token reveal / resize guard | 属于 `message-modal` / `one-time-reveal-modal` / `blocking-overlay` 家族，本次先不扩大范围 |

当前仍不宜强行纳入本次 change 的对象：

| 当前行 | 原因 |
| --- | --- |
| target 列表 / target editor 中的 `<*> Label --->` 复合导航行 | 当前 style matrix 还没有正式的 `status-entry-row` 或等价模板 |
| lock-state 这类“info-row + 局部高亮 value” | 仍可通过 `info-row` + style hook 保持，不需要先发明新模板 |

当前仍不纳入本次必做范围的弹窗：

| 当前弹窗 | 原因 |
| --- | --- |
| confirm / waiting / result / message / reveal / blocking overlay | 可以复用同一方向，但本次先收敛到 input 与 choice 两类高频模板 |

本次 change 的原则是：

- 能一对一映射到现有 catalog 的，必须迁移
- 还没有正式模板命名的复合行，不做假迁移，而是记录为例外

## 决策

### 决策 1：模板组件必须同时覆盖 row 和高频 popup

这次不能只做 row template。

如果 `text-input modal` 和 `choice-list modal` 继续散落在具体流程里手写，
那模板化只完成了一半，后续新增字段编辑和选择器时仍会继续漂移。

因此本次的最小实现集合应为：

- row template 组件
- `text-input modal` 组件
- `choice-list modal` 组件

### 决策 2：模板组件必须拆分“模板语义”和“交互目标”

模板组件不能只保存一段最终字符串，否则只是换了个地方继续拼字面量。

建议实现层至少拆成两部分：

- `RowTemplate`
  - 定义该行属于哪种正式模板
  - 决定前缀、是否带 `--->`、可聚焦性、`Space` / `Enter` 语义边界
- `RowInteraction` 或等价结构
  - 表达 `Enter` / `Space` 最终会触发什么
  - 例如 navigate、open editor、toggle、select choice、run action、none

这样可以把“模板是什么”和“点了之后做什么”从当前的混合分支里拆开。

### 决策 3：动态能力通过 payload 注入，而不是通过开放字符串逃逸

为了让模板可复用，同时不丢掉灵活性，模板组件需要支持动态 payload。

建议模板层允许注入的数据至少包括：

- `label`
- `value`
- `description`
- `selected` / `checked`
- `required_value`
- `blocked_reason`
- `group_id` 或等价组语义
- dirty tracking 或 style hook 所需元数据

但模板层不应把“最终渲染出来的完整一行字符串”作为输入，否则会重新回到自由拼装。

popup template 也应遵守同样原则。

建议 popup 层允许注入的数据至少包括：

- `title`
- `message`
- `hint`
- `input_prefix`
- `input_value`
- `cursor`
- `options`
- `selected_index`
- `submit/cancel` 交互目标

但 popup 层不应把整块 paragraph 文本或最终渲染布局作为自由输入。

### 决策 4：screen builders 要从“基础 kind helper”迁移到“模板 helper”

当前 `nav_entry` / `edit_entry` / `action_entry` / `info_entry`
适合快速建数据，但不适合做模板真相源。

本次建议把 screen builder 的主入口改成模板化 helper，例如：

- `row_info(...)`
- `row_field_entry(...)`
- `row_action(...)`
- `row_boolean_toggle(...)`
- `row_multi_select(...)`
- `row_exclusive_choice(...)`
- `row_exclusive_choice_entry(...)`
- `row_blocked_action(...)`
- `row_required_readonly(...)`
- `row_fixed_enabled(...)`

具体命名可以调整，但要满足一个关键约束：

- screen 在产出行数据时，必须显式声明模板

### 决策 5：popup builders 也要从流程手写迁移到模板 helper

当前输入和选择弹窗已经具有明显共性：

- centered rect
- clear
- block(title)
- message / hint
- input line 或 choice list
- cursor 或 selected row

因此本次建议增加 popup helper，例如：

- `popup_text_input(...)`
- `popup_choice_list(...)`

具体命名可以调整，但要满足两个约束：

- 流程层必须显式声明自己在打开哪种 popup template
- 公共 layout 和交互语义必须由 popup template 统一派生

### 决策 6：renderer 不再从 field/action 猜模板

`format_menu_entry_line` 当前最核心的问题不是“代码长”，而是“它在猜”。

本次应把渲染层改为：

- 先根据模板决定固定列结构
- 再根据 payload 填充 label/value/reason
- 最后由 interaction 决定 `--->` 是否出现以及哪些按键有效

迁移完成后，下列分支应不再是模板真相源：

- `is_boolean_toggle_field`
- `is_single_choice_toggle_field`
- `matches!(action, ActionKind::ToggleTokenAccess(_))`
- `matches!(action, ActionKind::BindTargetCredentialRef { .. })`
- `matches!(action, ActionKind::SelectTargetSshAuth*(_))`

它们最多只能作为临时兼容桥，不应继续决定模板语义。

### 决策 7：input / choice popup 不再各自维护独立布局合同

迁移完成后，下面这些结构不应继续各自定义一套独立合同：

- 普通文本编辑 popup
- SSH timeout 输入 popup
- choice 编辑 popup

它们应统一落到：

- `text-input-modal`
- `choice-list-modal`

并共享：

- 标题、正文、hint 结构
- input prefix / caret 合同
- option selected feedback 合同
- `Enter` / `Esc` / `Space` 的职责边界

### 决策 8：focusable / disabled 语义必须跟模板走

现在 `menu_entry_is_disabled` 只把 `Info` 看成 disabled，这会让模板合同和交互边界脱节。

迁移后应至少满足：

- `info-row` 非聚焦
- `blocked-action-row` 非聚焦
- `required-readonly-row` 非聚焦
- `fixed-enabled-row` 非聚焦
- `field-entry-row`、`action-row`、`boolean-toggle-row`、`multi-select-row`、
  `exclusive-choice-row`、`exclusive-choice-entry-row` 可聚焦

这样选中态、循环导航和按键分发才能真正按模板合同工作。

### 决策 9：对“未正式模板化的复合行”保留例外清单

本次不能为了追求“100% 行都换模板”而把还没定义好的复合语义塞进错误模板。

建议保留一个最小例外集合：

- target 列表类 `status + navigate` 复合行
- 其他无法一对一映射到当前 catalog 的行

并要求：

- 例外必须在 design / tasks / 测试注释中明确列出
- 新增例外必须先补 matrix/template，再扩张例外范围

### 决策 10：测试要覆盖“模板合同”与“迁移完整性”两层

本次不是简单换 helper 名称，所以测试也不能只看页面 smoke test。

至少需要两层护栏：

1. 模板合同测试
- 各模板的前缀、`--->`、可聚焦性、选中态、`Space` / `Enter` 语义
- input / choice popup 的 layout、cursor、selected row、提交/取消语义

2. 迁移完整性测试
- 当前已知可迁移的 screen/field/action family 都应通过模板组件产出
- 当前已知可迁移的 input / choice popup family 都应通过 popup template 组件产出
- 只有已记录例外允许继续走兼容路径

## 迁移策略

### 阶段 1：建立模板组件层

- 为行数据增加正式模板字段
- 定义模板 helper / builder API
- 让 renderer / focus / key handling 能以模板作为主驱动
- 为 input / choice popup 增加正式 template 字段或等价 popup model
- 定义 popup helper / builder API
- 保留旧分支作为短期桥接，直到批量迁移完成

### 阶段 2：按模板族迁移现有 screen

优先顺序建议如下：

1. `field-entry-row` / `action-row` / `info-row`
2. `boolean-toggle-row` / `multi-select-row`
3. `exclusive-choice-row` / `exclusive-choice-entry-row`
4. `blocked-action-row` / `required-readonly-row` / `fixed-enabled-row`
5. `text-input-modal` / `choice-list-modal`

这能先把通用量大的基础行迁完，再处理更依赖交互语义的模板。

### 阶段 3：收口兼容分支并补齐测试

- 去掉渲染层对 `field` / `ActionKind` 的模板猜测
- 去掉输入 / 选择弹窗在具体流程里的重复 layout 逻辑
- 把迁移过程中保留的桥接逻辑压缩到最小
- 增加例外清单检查和模板合同测试

## 风险 / 权衡

- [风险] 模板层如果做得太薄，只是把现有 helper 改名。
  - 缓解：要求 renderer / focus / key handling 都以模板为主驱动，而不是只在 builder 层包一层。

- [风险] 为了追求全迁移，可能把复合行硬塞进错误模板。
  - 缓解：明确“一对一映射才迁移”，其余行进入受控例外清单。

- [风险] 若只迁 row builder，不改渲染分支，后续仍可能继续漂移。
  - 缓解：把移除 `field/action` 推断写进任务与验收条件。

- [权衡] 本次只把 `text-input modal` 与 `choice-list modal` 纳入 popup template，而不一次性覆盖全部 overlay。
  - 这样能把最常复用的输入/选择结构先稳定下来，同时避免 waiting/result/confirm 一次性扩张范围。

## 开放问题

- target 列表类 `status + navigate` 复合行是否应该在后续单独抽成 `status-entry-row`。
- lock-state 这类“模板不变，但 value 需要局部高亮”的需求，是否需要统一 style hook 机制。
- 模板 helper 最终是保留在 `MenuEntry` 附近，还是顺手拆成独立模块；当前建议先以最小重组为主。
- popup template 是直接共用现有 `edit_mode` 状态，还是提前抽出更统一的 popup model；当前建议先抽最小 popup model，避免未来再返工。
