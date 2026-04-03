## 1. 风格合同与矩阵真相

- [x] 1.1 为 `standalone-operator-console` 补充正式的 menuconfig 风格合同 spec delta，明确单栏布局、行语法、焦点规则、选中态和 popup contract
- [x] 1.2 新增 `menuconfig-style-matrix` capability/spec delta，要求后续 menuconfig 功能变更必须维护 canonical style matrix
- [x] 1.3 创建 `docs/matrix/MENUCONFIG_STYLE_MATRIX.md`，覆盖布局区域、行类型、键位语义、popup 样式、fallback、最小视口与溢出裁剪
- [x] 1.4 在 matrix 中登记当前已知风格偏差，并明确这些偏差在本 change 归档前必须清零

## 2. 当前风格错误项修复范围（代码实现后续补齐）

- [x] 2.1 修复不可更改项仍可获得焦点的问题：`---`、`- -`、`-*-` 必须不可聚焦，初始进入与 `↑/↓` 导航都要自动跳过
- [x] 2.2 修复 popup 选中态不一致的问题：确认弹窗按钮、单选弹窗列表必须与主菜单/底部按钮共享同一反色与 fallback 高亮语义
- [x] 2.3 统一所有现有 menuconfig 页面行语法，覆盖主菜单、搜索结果、空状态、Security/Token/Vault 等特殊状态页
- [x] 2.4 实现最小视口保护与菜单行溢出裁剪，保证小终端和长文案不会破坏前缀/后缀语义
- [x] 2.5 统一文本输入弹窗行为，确保所有文本字段都遵守可见光标、左右移动、退格删除、`Enter` 提交、`Esc` 取消

## 3. 回归与文档对齐

- [x] 3.1 为焦点跳过、popup 高亮一致性、最小视口保护和溢出裁剪补充自动化测试
- [x] 3.2 更新相关帮助文案、开发文档或引用文档，明确 `MENUCONFIG_STYLE_MATRIX` 是后续 menuconfig 开发的风格真相源
- [x] 3.3 在实现完成后复核 matrix 的 `Known Current Deviations` 段落，确认已清空再归档本 change
