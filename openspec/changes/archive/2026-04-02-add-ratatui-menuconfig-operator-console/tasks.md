## 1. TUI 基础骨架

- [x] 1.1 创建 `ratatui` operator console 模块或 crate，并建立 `bridgingio-core menuconfig` 命令入口
- [x] 1.2 实现 menuconfig 风格的树形导航、帮助面板、搜索入口与 dirty tracking 基础交互

## 2. Core 真相接入

- [x] 2.1 将 TUI 接到 core-owned 字段描述、设置模型与校验语义，而不是维护私有 schema
- [x] 2.2 接入配置保存/应用结果，并展示 `restart_required`、诊断与 display-safe 的安全状态投影

## 3. 验证与文档

- [x] 3.1 为 operator console 补充交互 smoke 或 contract 测试，覆盖搜索、保存/应用和 display-safe 安全页
- [x] 3.2 更新 core/standalone 操作手册，说明 `menuconfig` 使用方式、与配置文件的关系，以及现有 `vault/auth` CLI 仍保留兼容

## 4. menuconfig 交互体验对齐（OpenWrt 风格）

- [x] 4.1 将可弹窗编辑字段统一为 `Label (value) --->` 形式，包含 `Instance Name`、`Log Level` 及其他同类可编辑项
- [x] 4.2 `Log Level` 改为单选弹窗；`Instance Name` 改为文本输入弹窗，并支持可见光标、左右移动与光标位置插入/删除
- [x] 4.3 在主流程中启用空格语义：布尔字段空格切换，枚举字段空格进入/确认选择
- [x] 4.4 采用 `<Select> < Exit > < Help >` 底部操作栏，支持左右键切换焦点，Enter 执行选中按钮
- [x] 4.5 统一返回/退出语义：`Esc` 作为返回上级快捷键；主菜单 `Esc` 触发退出；存在未保存修改时弹出 `Yes/No/Cancel` 保存确认
- [x] 4.6 列表项视觉语义对齐：可切换项使用 `[ ]/[*]`，可进入子菜单项使用 `--->`，子菜单开关入口使用 `< >/<*>`，只读项使用 `---`，说明/强调项使用 `*** ... ****`
- [x] 4.7 优化布局对齐：选项列表整体居中，文本列左对齐，开关/选择标记列在左侧突出显示，按钮行位于 Main Menu 框底部
- [x] 4.8 补充回归测试覆盖：返回后恢复上次入口位置、底部按钮行为、脏退出保存确认、弹窗字段行样式
