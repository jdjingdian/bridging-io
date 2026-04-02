## 新增需求

### 需求:`menuconfig` 必须提供 core-owned 语言配置入口
`bridgingio-core menuconfig` 必须在 core/general 等通用设置区域中提供独立的 core 语言配置入口，用于声明 core-owned operator surfaces 使用的语言。该配置项必须只允许选择 `zh-CN` 与 `en-US`，并且不得被 future UI 解释为其自身的通用多语言设置。

#### 场景:操作员在 `menuconfig` 中切换 core 语言
- **当** 操作员在 `menuconfig` 中将 core 语言从 `en-US` 切换为 `zh-CN` 并保存配置
- **那么** 系统必须把该值持久化到 core-owned 配置文件中，并在后续 core-owned CLI/TUI 展示中使用中文文案

#### 场景:future UI 忽略 core 语言字段
- **当** future UI 读取同一份 core-owned 配置文件
- **那么** UI 必须能够区分“core 当前语言”与“UI 自己的语言设置”，而不是把该字段直接当成 UI 多语言真相使用

### 需求:`menuconfig` 的 operator-facing 文案必须通过 catalog 渲染
`menuconfig` 的菜单标题、字段标签、字段说明、状态栏、帮助面板、弹窗文案和按钮标签必须通过 locale catalog 渲染，而不是继续在代码中硬编码显示文本。若当前语言存在缺失翻译，系统必须至少回退到 `en-US`，以保持操作面可用。

#### 场景:中文 locale 打开 `menuconfig`
- **当** core locale 为 `zh-CN`，且操作员进入 `bridgingio-core menuconfig`
- **那么** 主菜单标题、字段描述、状态栏提示、帮助面板和退出确认弹窗必须以中文展示，而不是夹杂英文硬编码

#### 场景:本地化搜索与 canonical path 并存
- **当** 操作员在 `menuconfig` 中搜索字段、菜单或目标项
- **那么** 系统必须至少能够通过本地化显示标签或 canonical config path 命中对应条目，而不是因为字段被翻译后失去可发现性

## 修改需求

无。

## 移除需求

无。
