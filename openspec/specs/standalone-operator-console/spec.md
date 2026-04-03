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
`menuconfig` 在展示 vault、token、runtime root、lifecycle 和诊断状态时，默认必须只使用 core 提供的 display-safe 投影。系统禁止在普通状态页、普通列表、搜索结果或重复进入的管理页面中直接展示 secret 明文、长期 token 明文或其他高敏内部字段。唯一允许的例外是：当操作员在受信任本地管理面中显式执行 `Create Token` 且本次签发成功后，系统可以通过独立的一次性结果弹窗承接该次响应中的明文 token；关闭该弹窗后，系统不得再从普通摘要路径重新回显该明文 token。

#### 场景:用户查看 Vault 状态
- **当** 操作员在 `menuconfig` 中进入 vault、Security 或 Token Management 页面
- **那么** 系统必须展示 lock state、策略摘要、token 备注、token 状态或等价 display-safe 字段，而不得在普通列表中回显 secret 或长期 token 明文

#### 场景:操作员显式创建 token
- **当** 操作员在已解锁的 Security 页面中显式触发 `Create Token` 并成功创建一个长期 token
- **那么** 系统必须展示一个包含该次明文 token 的一次性结果弹窗，并在关闭或离开后恢复为只显示 display-safe token 摘要，而不得从普通列表再次回显该明文 token

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

### 需求:Security 页面必须按 vault 状态裁剪动作
`bridgingio-core menuconfig` 的 Security 页面必须根据当前 vault 的真实状态裁剪可见动作，而不是在 `uninitialized`、`locked`、`unlocked` 等状态下长期展示同一组管理入口。系统禁止依赖“先把所有入口都列出来，再在点按后报错”的弱门控方式替代正式状态机。

#### 场景:vault 未初始化时进入 Security 页面
- **当** 操作员进入 Security 页面，且当前 vault 状态为 `uninitialized`
- **那么** 系统必须显示 `uninitialized` 或等价状态说明，并且只提供 `Init Vault --->` 或等价初始化入口，而不得显示 `Unlock Vault`、`Create Token` 或 `Token Management`

#### 场景:vault 已初始化但未解锁时进入 Security 页面
- **当** 操作员进入 Security 页面，且当前 vault 状态为 `locked`
- **那么** 系统必须显示 `locked` 或等价状态说明，并且只提供 `Unlock Vault --->` 或等价解锁入口，而不得继续显示 `Init Vault`、`Create Token` 或 `Token Management`

#### 场景:vault 已解锁时进入 Security 页面
- **当** 操作员进入 Security 页面，且当前 vault 状态为 `unlocked`
- **那么** 系统必须显示 `vault unlocked` 或等价已解锁提示，并展示 `Create Token --->` 与 `Token Management --->` 入口，而不得继续显示 `Init Vault` 或 `Unlock Vault`

### 需求:menuconfig 必须提供显式 vault init/delete 管理路径
`bridgingio-core menuconfig` 必须把 vault 的初始化与删除建模为正式管理动作，而不是要求操作者手工删除目录、重启程序或依赖隐式 bootstrap。系统必须确保 `vault delete` 后重新回到 `uninitialized`，并且所有高风险删除动作都必须具备二次确认。

#### 场景:操作员从未初始化状态执行 init
- **当** 操作员在 Security 页面触发 `Init Vault`，且当前 vault 为 `uninitialized`
- **那么** 系统必须完成当前 runtime store 的正式 vault 初始化，并在成功后把 Security 页面状态更新为 `locked` 或等价的“已初始化待解锁”状态

#### 场景:操作员删除当前 vault
- **当** 操作员在已初始化的 Security 页面中触发 `Delete Vault` 并完成二次确认
- **那么** 系统必须删除当前实例的 vault 持久化数据，并把 Security 页面重新切换为 `uninitialized` 状态，同时移除 token 管理入口

#### 场景:vault 缺失时不得走隐藏 unlock fallback
- **当** 当前 vault 持久化数据不存在，且操作员尚未执行 `Init Vault`
- **那么** `menuconfig` 不得通过默认内存 router、隐藏 bootstrap 或其他回退路径继续完成 unlock，而必须保持 `uninitialized`

### 需求:Token Management 子页必须支持备注编辑与带确认的 revoke/delete
当 Security 页面暴露 `Token Management` 后，`menuconfig` 必须先提供一个 token 摘要列表，再允许操作员进入统一的单 token 详情页执行管理动作。列表页必须为每个未删除 token 展示稳定序号、别名、有效期摘要与当前状态，并以 `--->` 进入详情页。详情页必须展示 `Token ID`、`别名 = <value> --->` 直编入口、display-safe 的 token 指纹/摘要、有效期、`<*>/< > 开关状态 = [启用|禁用]` 单行访问开关、`权限管理 --->` 单箭头入口，以及根据状态裁剪的 revoke/delete 动作。系统必须继续要求 revoke/delete 经过二次确认，并且 `delete` 只能对 `revoked` token 生效。

#### 场景:Token Management 列表展示摘要
- **当** 操作员在已解锁的 Security 页面进入 `Token Management`
- **那么** 系统必须先展示未删除 token 的摘要列表，并为每个 token 以 `(000001) 别名 [长期|YY-MM-DD] [有效|过期|撤销|禁用] --->` 或等价格式展示，而不是直接把编辑、撤销、删除动作平铺在列表层

#### 场景:操作员进入统一 token 详情页
- **当** 操作员在 Token Management 列表中选中某个 token 并进入详情页
- **那么** 系统必须展示统一的单 token 管理页面，其中包含 `Token ID` 首行、`别名 = <value> --->`、`<*>/< > 开关状态 = [启用|禁用]`、display-safe 的 token 指纹/摘要、有效期、`权限管理 --->` 单箭头入口和与当前状态匹配的管理动作；并且不得显示明文 token 或原始 `token_hash`

#### 场景:操作员切换 token 启用状态
- **当** 操作员在 token 详情页切换一个当前处于 `有效` 或 `禁用` 投影状态的 token 的访问开关
- **那么** 系统必须仅允许使用 `Space` 触发访问开关切换（`Enter` 不能触发切换），并在不重签发 token 的前提下更新该 token 的可访问状态；返回列表后状态更新为 `有效` 或 `禁用`

#### 场景:操作员撤销 token
- **当** 操作员在 token 详情页中请求撤销一个尚未 `deleted` 且尚未 `revoked` 的 token
- **那么** 系统必须先显示 revoke 二次确认，并且只有在操作员确认后才将该 token 置为 `revoked`

#### 场景:revoked token 删除必须二次确认
- **当** 操作员在 token 详情页中请求删除一个已处于 `revoked` 的 token
- **那么** 系统必须显示 delete 二次确认，并且只有在操作员确认后才允许该 token 进入 `deleted` 终态或等价的已删除管理状态

#### 场景:未撤销 token 不显示删除入口
- **当** 操作员查看一个处于 `有效`、`禁用` 或 `过期` 状态的 token 详情页
- **那么** 系统不得显示 `删除 --->` 入口，并且必须通过说明文案或状态提示明确该 token 需先撤销后才可删除

#### 场景:权限管理入口暂未实现时给出受控反馈
- **当** 操作员在 token 详情页触发 `权限管理 --->`，但当前版本尚未完成按 profile 配置 token scope 的编辑能力
- **那么** 系统必须进入受控的占位反馈，例如 `method_not_implemented` 或等价提示，而不是空白返回、崩溃或泄漏内部错误

#### 场景:详情页动作文案不重复展示 token_id
- **当** 操作员已经进入某个 token 的详情页（页面首行已显示 `Token ID`）
- **那么** `权限管理 --->` 与 `撤销 Token --->` 等动作文案不得再次附带括号形式的 token_id

### 需求:流程入口动作必须使用一致的导航样式
`menuconfig` 中会进入下一级页面、触发多步流程或弹出确认/结果弹窗的动作，必须统一使用与导航一致的 `--->` 样式，避免与即时切换类动作的 `*** ... ****` 样式混用导致误解。

#### 场景:Security 页面展示流程入口动作
- **当** 操作员在 Security 页面看到 `Create Token`、`Token Management`、`Delete Vault` 或等价流程入口
- **那么** 这些动作必须使用 `--->` 导航样式，而不得渲染为 `*** ... ****` 的即时动作样式

#### 场景:Token Management 子页展示编辑/撤销/删除动作
- **当** 操作员在 Token Management 子页查看 `Edit Label`、`Revoke Token`、`Delete Token` 或等价动作
- **那么** 这些动作必须使用 `--->` 导航样式，以表明其会进入编辑流程或确认链路

### 需求:`Create Token` 必须采用引导式创建并明确本机时间边界
当操作员在已解锁的 Security 页面中触发 `Create Token` 时，`menuconfig` 必须按“label -> 有效期 -> 一次性结果”的顺序引导创建，而不是把高敏字段与结果展示混杂在同一个长期停留的列表页中。对于指定时间失效的 token，界面必须明确展示当前本机时间与时区，并在时钟异常时给出限制或强提示。

#### 场景:操作员先输入 token 别名
- **当** 操作员触发 `Create Token`
- **那么** 系统必须先弹出输入框要求填写 token 别名（`label`），并在别名为空时拒绝继续下一步

#### 场景:操作员选择长期 token
- **当** 操作员已确认 token 别名，并在有效期步骤中选择 `长期有效`
- **那么** 系统必须允许继续创建，并把该 token 视为无过期时间限制的长期 token

#### 场景:操作员选择指定时间失效
- **当** 操作员已确认 token 别名，并在有效期步骤中选择 `有效至指定时间`
- **那么** 系统必须展示当前本机时间与时区，要求操作员选择一个未来失效时间，并明确该时间按本机时钟解释

#### 场景:本机时钟异常时创建 expiring token
- **当** runtime 已检测到明显的时钟回拨、时钟健康异常或等价的不可信本机时间状态
- **那么** `Create Token` 流程必须阻止或强警告 `有效至指定时间` 选项，并要求操作员先修复本机时间或改为创建长期 token

#### 场景:token 创建成功后一次性展示明文
- **当** 操作员完成 `Create Token` 并创建成功
- **那么** 系统必须展示包含明文 token 的一次性结果弹窗，提示操作员自行保存；关闭该弹窗后，系统不得再从普通摘要页面重新显示该明文

#### 场景:Create Token 弹窗字段文案必须面向操作员
- **当** 操作员在 `Create Token` 流程中进入 label/有效期等编辑弹窗
- **那么** 系统必须展示可读的 operator-facing 字段标题（例如“创建 Token 别名”），而不得直接暴露内部字段标识（例如 `__token_create_label__`）

### 需求:Token 别名输入必须遵循统一校验合同
在 `menuconfig` 的 `Create Token` 与单 token 详情编辑流程中，系统必须对 operator-facing token 别名执行统一校验。新写入的别名必须先做 trim，长度必须保持在 1 到 64 个字符之间，并且只能包含 ASCII 字母、数字、`-`、`_`；系统禁止空格、制表符和其他连接符号进入持久化值。对历史版本遗留的不合规别名，系统必须继续允许列表/详情页只读展示，但在用户重新保存时必须要求其改为合规值。

#### 场景:创建 token 时输入带连接符的别名
- **当** 操作员在 `Create Token` 流程中输入 `test_tok` 或 `test-tok2`
- **那么** 系统必须接受该别名，并允许继续进入有效期选择步骤

#### 场景:编辑别名时输入包含空格的值
- **当** 操作员在 token 详情页尝试把别名改为 `test tok`
- **那么** 系统必须拒绝保存，并明确提示别名不能包含空格且只允许字母、数字、`-`、`_`

#### 场景:存在历史遗留的不合规别名
- **当** 某个旧 token 已保存了包含空格或其他旧格式字符的别名
- **那么** Token Management 列表与详情页必须继续显示该旧值；并且当操作员进入编辑流程后，系统必须要求其改为合规别名后才允许保存

