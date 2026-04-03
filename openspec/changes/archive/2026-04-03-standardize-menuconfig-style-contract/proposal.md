## 为什么

当前 `menuconfig` 的风格约束分散在 `standalone-operator-console`、`menuconfig-selection-contrast`、局部实现细节和零散变更里，虽然已经具备 Linux kernel `menuconfig` 的一部分交互外形，但还没有形成一份可审查、可维护、可回归的完整风格合同。结果是：

- 行前缀、焦点语义、弹窗样式、文本输入、最小终端尺寸与溢出裁剪都还不够明确；
- 当前实现已经出现风格偏差，例如不可更改项仍可能停留焦点、弹窗选择按钮未与主菜单共享一致的反色高亮；
- 后续功能如果继续在没有统一 style matrix 的情况下扩展，会让 `menuconfig` 逐步偏离统一体验。

现在需要把 `menuconfig` 的“单栏布局、行语法、焦点规则、键位语义、弹窗合同、降级样式和维护规则”正式定义下来，并建立一份 canonical 的风格矩阵文档，要求后续 `menuconfig` 功能开发必须匹配这套标准。

## 变更内容

- 为 `menuconfig` 建立一份完整的风格合同，明确单栏、整体居中、文本左对齐、前缀突出、逐级进入子菜单、不分栏的基础形态。
- 把行语法明确写入 spec，包括 `---`、`- -`、`-*-`、`< >`、`<*>`、`[ ]`、`[*]`、`--->` 和当前选中项左侧 `>` 的正式语义。
- 明确规定不可更改项必须不可聚焦，并在上下移动时自动跳过；这类当前风格错误必须在本 change 的后续实现阶段修复。
- 明确规定菜单、底部按钮、弹窗按钮、单选弹窗列表都必须共享一致的反色高亮与 fallback 语义，而不是只让主菜单高亮。
- 新增一份 `MENUCONFIG_STYLE_MATRIX`，作为后续 `menuconfig` 开发的风格真相源；凡是改变行语法、键位语义、弹窗样式或终端布局的功能，都必须同步更新该矩阵。
- 把当前已知偏差记录为本 change 的 must-fix 项，在修复前不得归档本 change。

## 功能 (Capabilities)

### 新增功能

- `menuconfig-style-matrix`: 维护 `menuconfig` 的正式风格矩阵与维护规则，作为后续功能开发与验收的风格真相源。

### 修改功能

- `standalone-operator-console`: 补全 `menuconfig` 的完整风格合同，覆盖行语法、焦点语义、弹窗样式、文本输入、最小视口与溢出裁剪。

## 影响

- 当前阶段主要新增和更新 OpenSpec 产物与 matrix 文档，不涉及应用代码实现。
- 后续实现阶段将影响 `source/rust/bridgingio-operator-console`、相关 i18n 文案、交互测试以及所有新增 `menuconfig` 功能的验收基线。
- 本 change 引入后，`menuconfig` 不再允许“只改实现、不更新风格真相”的交付方式；风格合同与 style matrix 必须与实现同步演进。
