# standalone-operator-console 规范

## 目的
待定 - 由归档变更 add-ratatui-menuconfig-operator-console 创建。归档后请更新目的。
## 需求
### 需求:`bridgingio-core` 必须提供基于 `ratatui` 的 `menuconfig` 通用操作面
BridgingIO 必须提供 `bridgingio-core menuconfig` 命令，并通过 `ratatui` 提供一个正式的通用配置与诊断操作面。该操作面不绑定 standalone，而是 core 的通用配置模式；standalone 和 future UI 都可以复用同一配置文件与配置语义。该操作面必须支持树形浏览、帮助查看、搜索、保存与应用，而不是只依赖手工 TOML 编辑。

#### 场景:用户通过 `menuconfig` 进入配置界面
- **当** 操作员执行 `bridgingio-core menuconfig`
- **那么** 系统必须进入正式的 TUI 配置界面，而不是直接启动运行模式或要求用户手工编辑 TOML

#### 场景:用户通过树形导航浏览配置
- **当** 操作员进入 `bridgingio-core menuconfig`
- **那么** 系统必须提供可浏览的树形配置结构，使用户能够在不直接编辑 TOML 的情况下查看和修改配置项

#### 场景:用户搜索配置项
- **当** 操作员在 `menuconfig` 中搜索某个配置字段、目标项或诊断主题
- **那么** 系统必须返回可定位到对应节点的搜索结果，而不是要求用户手工滚动浏览全部树结构

### 需求:operator console 必须尽量对齐 Linux kernel `menuconfig` 的核心交互模式
BridgingIO 的 `menuconfig` 操作面必须尽量对齐 Linux kernel `menuconfig` 的核心交互模式，至少覆盖树形导航、帮助面板、搜索入口、dirty tracking 和保存/应用语义。

#### 场景:用户查看当前节点帮助
- **当** 操作员在 `menuconfig` 中选中某个配置节点
- **那么** 系统必须展示该节点的帮助、字段描述或等价说明，而不是只显示字段名和值

#### 场景:用户修改后准备退出
- **当** 操作员修改了一个或多个配置项并准备退出 `menuconfig`
- **那么** 系统必须明确显示存在未保存修改，并提供保存、应用或放弃修改的正式路径

#### 场景:底部按钮栏驱动主操作
- **当** 操作员位于主界面或子菜单界面
- **那么** 系统必须展示 `<Select> < Exit > < Help >` 操作栏，并允许通过左右键切换按钮焦点、通过 Enter 执行选中按钮

#### 场景:`Esc` 作为统一返回与退出键
- **当** 操作员位于子菜单并按下 `Esc`
- **那么** 系统必须返回上一级菜单，而不是使用 Backspace 作为主返回路径
- **并且当** 操作员位于主菜单并按下 `Esc`
- **那么** 系统必须触发退出流程；若存在未保存修改，必须弹出 `Yes/No/Cancel` 保存确认弹窗

### 需求:`menuconfig` 的列表上下导航必须支持首尾循环
`menuconfig` 在处理菜单列表的垂直焦点移动时，必须采用循环逻辑，而不是在边界停留。系统必须保证操作员在任意列表中连续按 `↑/↓` 时，焦点可以在首尾之间闭环移动。

#### 场景:第一项按上键跳转到最后一项
- **当** 操作员位于某菜单列表第一项并按下 `↑`
- **那么** 系统必须将焦点移动到该列表最后一项，而不是停留在第一项

#### 场景:最后一项按下键跳转到第一项
- **当** 操作员位于某菜单列表最后一项并按下 `↓`
- **那么** 系统必须将焦点移动到该列表第一项，而不是停留在最后一项

#### 场景:空列表或单项列表按上下键保持稳定
- **当** 当前列表为空，或列表仅有一个可聚焦项，且操作员按下 `↑` 或 `↓`
- **那么** 系统必须保持界面稳定且不得发生越界或异常

### 需求:可编辑项与菜单项视觉语义必须统一并可预测
`menuconfig` 的行内视觉语义必须一致，确保操作员可从一行内容直接判断“是否可编辑、是否可进入、是否只读、当前状态是什么”。

#### 场景:可弹窗编辑字段的行样式
- **当** 某字段通过弹窗编辑（文本或单选）且可在列表中被选中
- **那么** 系统必须展示为 `Label (value) --->` 形式（例如 `Instance Name (bridgingio-standalone) --->`、`Log Level (info) --->`）

#### 场景:开关与选择项标记语义
- **当** 配置项是布尔可切换字段
- **那么** 系统必须使用 `[ ]` / `[*]` 表示开关状态
- **并且当** 配置项是可进入子菜单且携带启用语义的入口
- **那么** 系统必须使用 `< >` / `<*>` 表示入口状态，并在右侧保留 `--->` 进入指引

#### 场景:只读项与说明项标记语义
- **当** 配置项不可直接编辑
- **那么** 系统必须使用 `---`（或等价只读标记）展示
- **并且当** 行内容用于强调说明/动作标题
- **那么** 系统必须使用 `*** ... ****`（或等价强调样式）展示

#### 场景:列表布局与对齐
- **当** 操作员浏览任一菜单列表
- **那么** 系统必须使选项区域整体居中，同时保持文本列左对齐、状态标记列左侧突出，以对齐 OpenWrt `menuconfig` 的阅读节奏

### 需求:安全与生命周期页面必须只展示 display-safe 投影
`menuconfig` 在展示 vault、token、runtime root、lifecycle 和诊断状态时，必须只使用 core 提供的 display-safe 投影。系统禁止在该 TUI 中直接展示 secret 明文、长期 token 明文或其他高敏内部字段。

#### 场景:用户查看 Vault 状态
- **当** 操作员在 `menuconfig` 中进入 vault 或安全状态页面
- **那么** 系统必须展示 lock state、protector readiness、策略摘要或 token summary，而不得回显 secret 或 token 明文

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

### 需求:`menuconfig` 的 vault 解锁必须只由显式操作触发
`bridgingio-core menuconfig` 在进入主界面、切换到 Vault 或 Security 页面、或刷新 display-safe 安全摘要时，必须保持对 vault 的被动观察。系统禁止因为加载 router、读取 lock state、统计 summary 或检查 protector readiness 而主动触发系统钥匙串、trusted verification 或其他本地解锁交互。只有在操作员显式触发解锁动作后，系统才可以开始正式 unlock flow。

#### 场景:macOS 操作员进入 menuconfig
- **当** 操作员在 macOS 上启动 `bridgingio-core menuconfig`，且 vault 当前处于 `locked`
- **那么** 系统必须先展示 `locked` 或等价 display-safe 摘要，而不得在进入界面时立即弹出系统钥匙串解锁提示

#### 场景:操作员浏览 Security 页面但未触发解锁
- **当** 操作员在 `menuconfig` 中进入 Vault 或 Security 页面，但尚未按下 `u` 或触发解锁入口
- **那么** 系统必须继续只显示 display-safe 状态投影，而不得因为页面切换或状态刷新启动正式 unlock

### 需求:`menuconfig` 必须为显式解锁提供可确认的反馈流程
`menuconfig` 在操作员显式触发 vault 解锁后，必须提供清晰的中间态与结果态反馈，包括等待系统完成解锁的弹窗、成功后的确认弹窗，以及返回菜单后可持续感知的已解锁提示。系统不得只在底部状态栏瞬时输出一条结果文本而让操作员自行猜测是否解锁成功。

#### 场景:操作员触发 os-native 解锁
- **当** 操作员在 macOS `menuconfig` 中按下 `u` 或触发 Security 页的解锁入口，且当前使用 `os-native` 作为首选解锁方式
- **那么** 系统必须先显示“等待解锁”或等价中间弹窗，再由操作系统弹出正式本地解锁验证界面

#### 场景:解锁成功后确认返回菜单
- **当** 操作员完成显式解锁且 vault 进入 `unlocked`
- **那么** 系统必须展示“解锁成功”或等价确认弹窗，并在操作员确认后返回菜单，同时移除原有 unlock 提示并显示“加密项管理已解锁”或等价已解锁状态

#### 场景:解锁失败或取消
- **当** 操作员显式触发解锁，但本地验证失败或主动取消
- **那么** 系统必须返回 `locked` 状态并给出明确失败反馈，而不得继续显示误导性的已解锁提示

#### 场景:等待态支持 Esc 取消
- **当** 操作员在 `WaitingForUnlock` 中间态按下 `Esc`
- **那么** `menuconfig` 必须立即退出等待弹窗并给出“已取消等待”反馈，且后续状态不得误报为已解锁

#### 场景:显式解锁入口不走隐藏回退路径
- **当** 操作员通过 `u` 或 Security 解锁入口触发本地解锁
- **那么** `menuconfig` 必须只执行 `os-native` 本地验证路径，不得在同一交互内自动回退到 passphrase 或其他隐藏输入流程

#### 场景:Security 菜单中的锁状态和解锁条目遵循强语义展示
- **当** 操作员进入 Security 菜单
- **那么** 系统必须将 `Vault 锁状态` 值按 `locked=红色`、`unlocked=绿色`（其他状态黄色）高亮显示
- **并且** 当 vault 为 `locked` 时显示 `解锁 Vault --->`
- **并且** 当 vault 为 `unlocked` 时显示 `-*- 加密项管理已解锁`，且不再显示解锁动作入口
