## 1. 本地化基础设施

- [x] 1.1 在 Xcode 工程中启用 `en` 与 `zh-Hans` 本地化，并新增 `Localizable` 资源（String Catalog 或等价文件）
- [x] 1.2 建立本地化键命名规范并新增统一访问入口（如 `L10n`），覆盖静态文案与带参数文案
- [x] 1.3 新增品牌键并配置双语值，确保产品名统一为 `Bridging IO`

## 2. 界面文案迁移

- [x] 2.1 迁移 `WorkspaceConsoleView` 与其子组件中的标题、按钮、占位符、空状态和提示文案到 `Localizable`
- [x] 2.2 迁移 `SettingsSheetView`、`TargetProfileSheetView`、`ConsoleControls` 等设置/编辑路径文案到 `Localizable`
- [x] 2.3 迁移 `Domain` 与 `ViewModel` 中的 `title`、状态文本、校验报错与默认提示到本地化键
- [x] 2.4 清理显示路径中的硬编码展示字符，确保 `Text/Label/Button/TextField` 等 UI 文案均通过本地化入口渲染

## 3. 约束与验证

- [x] 3.1 增加硬编码文案检测规则（lint 或脚本），阻断新增显示层字面量文本
- [x] 3.2 增加 `en` 与 `zh-Hans` 键一致性检查，确保缺失翻译可回退英文且不会显示键名
- [x] 3.3 补充或更新测试，覆盖语言切换下的关键页面文案与产品名显示一致性
- [x] 3.4 更新 SwiftUI macOS 相关文档，说明本地化约束、键规范和新增文案流程
