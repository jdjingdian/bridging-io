## 1. 共享语言与配置真相

- [x] 1.1 为 core-owned operator surfaces 建立共享 catalog 层，并新增 `zh-CN` / `en-US` 资源文件与键命名规范
- [x] 1.2 在 `bridgingio-engine` 的配置模型、示例配置和序列化逻辑中新增独立的 core operator locale 字段，并限制合法值为 `zh-CN` / `en-US`
- [x] 1.3 在 `menuconfig` 中新增 core 语言配置入口，并确保保存后稳定写回配置文件

## 2. 版本与 CLI help 收口

- [x] 2.1 将 workspace 主 `Cargo.toml` 的 `version` 设为 core 版本唯一真相，并让 member crate 统一继承该版本
- [x] 2.2 让 `bridgingio-core --version` 与其他对外 version 元数据统一读取 Cargo 版本真相，并校验格式为 `YYMM.DD.BuildNumber`
- [x] 2.3 将 `bridgingio-core` help 渲染改为统一命令元数据驱动，支持 locale catalog、分组留白和标题强调样式

## 3. menuconfig 文案迁移

- [x] 3.1 将 `bridgingio-operator-console` 的菜单标题、字段标签、字段说明、状态栏、帮助面板、弹窗和按钮文案迁移到 catalog
- [x] 3.2 保持 `menuconfig` 搜索同时支持本地化显示标签与 canonical config path，避免翻译后条目不可发现
- [x] 3.3 为缺失翻译场景提供 `en-US` 回退，确保 operator surface 在 catalog 不完整时仍可用

## 4. 验证与文档

- [x] 4.1 补充自动化测试，覆盖 catalog 键集合一致性、locale 切换、`--help`/`--version` 输出和版本来源一致性
- [x] 4.2 增加针对 core operator surface 的硬编码显示文本检查，阻断 CLI 和 `menuconfig` 文案回流到代码字面量
- [x] 4.3 更新 operator interface matrix、core 使用文档与变更说明，明确 core locale 字段边界和版本维护规则
