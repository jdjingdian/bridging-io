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
BridgingIO 的 `menuconfig` 操作面必须尽量对齐 Linux kernel `menuconfig` 的核心交互模式，至少覆盖树形导航、帮助面板、搜索入口、dirty tracking、保存/应用语义，以及单栏逐级进入的主界面组织方式。

#### 场景:主流程保持单栏逐级进入
- **当** 操作员从主菜单进入任一子页面、详情页或管理流程
- **那么** 系统必须通过 `--->` 与 `Enter` 进入下一级单栏页面或 centered popup，而不是切换到分栏主界面或并排工作区

#### 场景:popup 优先拦截 `Esc`
- **当** 操作员当前位于确认弹窗、帮助弹窗、文本输入弹窗或单选弹窗中并按下 `Esc`
- **那么** 系统必须先关闭当前 popup 或取消当前流程，而不是把 `Esc` 直接穿透成上一层菜单返回

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
`menuconfig` 的行内视觉语义必须遵守统一的风格合同，使操作者能够从一行内容直接判断“当前是否可聚焦、是否可切换、是否可进入下一级、当前状态是什么”。系统必须把焦点标识、语义前缀、正文与导航后缀拆分为稳定的固定列，并通过正式的 row template / group template 语义层统一派生，而不得由各个页面自由拼装。

#### 场景:页面通过模板层派生统一行语义
- **当** 操作员进入任一 menuconfig 页面并浏览其中的可编辑项、动作项或状态项
- **那么** 系统必须通过共享模板层派生这些行的视觉与交互语义
- **并且** 不得因为页面作者自行拼装字符串而让同类行出现不同的前缀、箭头或键位行为

### 需求:安全与生命周期页面必须只展示 display-safe 投影
`menuconfig` 在展示 vault、token、runtime root、lifecycle、诊断与 SSH key 摘要状态时，默认必须只使用 core 提供的 display-safe 投影。系统禁止在 Security 页面、SSH Key Management 列表页、普通详情页或 target 绑定 picker 中直接展示 secret 明文、长期 token 明文或其他高敏内部字段。

#### 场景:查看 SSH key 管理页
- **当** 操作员在 menuconfig 中浏览 `SSH Key Management` 列表页或详情页
- **那么** 系统只能展示 canonical ref、label、status、record-id 和等价 display-safe 摘要
- **并且** 不得把私钥内容、passphrase、ciphertext locator 或等价秘密材料带入 UI 文本

#### 场景:vault locked 时查看 Security 摘要
- **当** 操作员在 vault `locked` 状态下进入 Security 页面
- **那么** SSH key 相关摘要最多只能显示聚合数量
- **并且** 不得把单个 imported key 的 identity 信息作为“display-safe 摘要”带入 locked 态页面

#### 场景:SSH target 选择 imported vault key
- **当** 操作员在 SSH target 流程中选择一个已导入的 vault SSH key
- **那么** picker 或选择页必须只展示 display-safe key 摘要
- **并且** 不得把 underlying secret material 暴露给 target 编辑界面

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

### 需求:`menuconfig` 必须为所有可操作焦点体提供一致的选中态语义
`menuconfig` 中所有“当前可操作且可获得焦点”的元素必须共享一致的选中态反馈策略。该范围至少包括主菜单行、底部按钮栏、确认弹窗按钮和单选弹窗当前项。支持反色的终端必须使用统一反色策略；不支持反色的终端必须对这些焦点体同时启用同一套可见 fallback 样式，而不是只让部分区域降级。

#### 场景:确认弹窗按钮共享反色语义
- **当** 操作员在 `Yes/No/Cancel` 或 `Confirm/Cancel` 等确认弹窗中左右切换当前按钮
- **那么** 当前按钮必须使用与主菜单选中项等价的反色或 fallback 高亮，而不是只通过尖括号文本提示焦点

#### 场景:单选弹窗列表共享反色语义
- **当** 操作员在单选弹窗中通过 `↑/↓` 切换当前选项
- **那么** 当前选项必须使用与主菜单选中项等价的反色或 fallback 高亮，而不是仅显示一个普通的 `>` 光标

#### 场景:终端反色不可用时 popup 与主菜单同步降级
- **当** 终端能力探测结果为不支持反色
- **那么** 主菜单、底部按钮、确认弹窗和单选弹窗中的当前焦点项都必须同时切换到约定 fallback 样式，而不是出现“主菜单已降级、popup 仍无选中反馈”的不一致状态

### 需求:`menuconfig` 必须提供单栏布局、最小视口保护与稳定的溢出裁剪
`menuconfig` 必须保持单栏、逐级进入的主布局，不得在主流程中引入分栏主界面。系统必须使菜单区域整体居中、文本列左对齐、前缀列固定突出，并为最小终端尺寸与长行溢出提供正式保护，防止视觉语义被挤坏。

#### 场景:终端尺寸低于最小支持值
- **当** 终端视口小于 `80x24` 或等价的正式最小尺寸
- **那么** 系统必须显示受控的“请放大终端”提示或等价保护弹窗，而不是继续渲染错位、截断后不可判读的 menu 布局

#### 场景:菜单行文案过长
- **当** 菜单行文本长度超出当前可用宽度
- **那么** 系统必须优先保留左侧焦点标识、语义前缀与右侧 `--->`，中间文本可以使用受控截断，而不得让前缀或 `--->` 被挤掉或折成多行

#### 场景:帮助或状态文本较长
- **当** 状态栏、帮助弹窗或说明文案内容较长
- **那么** 系统可以在这些区域进行受控换行，但主菜单列表行本身不得折成多行

### 需求:搜索结果与特殊状态页必须复用同一行语法与焦点合同
`menuconfig` 的搜索结果、空状态、锁定提示、缺失提示和其他特殊页面不得自行发明第二套语法。凡是出现在菜单列表中的行，都必须复用主菜单的前缀、后缀、焦点和跳过规则。

#### 场景:搜索结果包含可进入项
- **当** 操作员在搜索结果列表中看到一个可进入的字段或页面入口
- **那么** 该行必须继续使用与主菜单一致的 `--->` 和当前选中态语义，而不是使用另一套搜索专用样式

#### 场景:搜索结果或特殊页面包含只读提示
- **当** 搜索结果为空、页面被锁定、对象缺失或系统需要展示只读提示
- **那么** 该提示行必须使用 `---`、`- -`、`-*-` 或等价的 non-focusable 语法，并在导航中被自动跳过

### 需求:Targets 页面必须按 vault 锁状态裁剪 sensitive target 管理动作
`bridgingio-core menuconfig` 的 Targets 页面必须根据当前 vault 锁状态裁剪 sensitive target 的创建、编辑和删除入口。系统不得使用“先进入流程，最后保存时报错”的弱门控替代正式状态裁剪。

#### 场景:vault locked 时进入 Targets 页面
- **当** 操作员进入 Targets 页面，且当前 vault 状态为 `locked`
- **那么** 系统必须继续显示 plain target 与 sensitive target 的 public cache 摘要
- **并且** 系统必须允许继续创建 plain target
- **并且** 系统不得允许创建、编辑或删除 sensitive target

#### 场景:vault unlocked 时进入 sensitive target 详情页
- **当** 操作员进入一个**已存在**的 sensitive target 的详情页，且当前 vault 状态为 `unlocked`
- **那么** 系统必须提供 `Public Descriptor --->`、`Sensitive Overlay --->`、`Policy --->`、`Apply Target --->` 与 `Delete Target --->` 等正式管理入口
- **并且** 若该 target 的 kind 为 `ssh`，`Sensitive Overlay` 中必须允许进入 `Credential Source --->` 绑定流程

#### 场景:vault locked 时进入 sensitive target 详情页
- **当** 操作员进入一个 sensitive target 的详情页，且当前 vault 状态为 `locked`
- **那么** 系统必须只展示 public cache 与锁定提示
- **并且** 系统必须提供 `Unlock Vault --->` 或等价入口
- **并且** 系统不得显示 sensitive overlay 编辑入口或删除入口
- **并且** 若该 target 的 kind 为 `ssh`，系统不得显示当前绑定 imported key 的 label、canonical `credential_ref` 或等价 inventory 信息

### 需求:Add Target 必须先选择 storage mode 再选择 target type
`menuconfig` 的 `Add Target` 流程必须先让操作员在 `plain` 与 `sensitive` 之间做出明确选择，再进入受支持 target 类型的选择与后续详情编辑。对于 `kind = ssh` 的 target，系统必须在 target type 选择之后、detail editor 之前插入正式的 `SSH Authentication Setup` 步骤。

#### 场景:操作员开始创建 target
- **当** 操作员在 Targets 页面触发 `Add Target --->`
- **那么** 系统必须先进入 `Choose Storage Mode` 或等价步骤，并提供 `Plain Target --->` 与 `Sensitive Target --->` 两个入口

#### 场景:vault locked 时创建 target
- **当** 操作员在 vault 为 `locked` 的状态下进入 `Choose Storage Mode`
- **那么** 系统必须只允许继续进入 `Plain Target --->`
- **并且** `Sensitive Target` 必须显示为禁用或引导到 `Unlock Vault --->`

#### 场景:操作员选择 plain ssh target
- **当** 操作员选择 `Plain Target` 并继续选择 `SSH`
- **那么** 系统必须在进入字段编辑前显示正式风险确认
- **并且** 该确认必须明确说明相关连接描述字段与 plain password 可能保留在 `config.toml`
- **并且** 在确认后系统必须先进入 `SSH Authentication Setup`，而不是直接进入字段编辑

#### 场景:操作员选择 sensitive ssh target
- **当** 操作员在 unlocked 状态下选择 `Sensitive Target` 并继续选择 `SSH`
- **那么** 系统必须先进入 `SSH Authentication Setup`
- **并且** 只有在认证组合校验通过后，系统才可以进入 SSH detail editor

### 需求:Target 创建与管理必须区分会话动作并采用显式 `Create/Apply`
`menuconfig` 的 target 详情页必须区分“创建会话”和“管理会话”。系统不得在用户仅选择 target 类型后就把该 target 视为已创建完成；必须通过显式 `Create Target --->` 或 `Apply Target --->` 动作确认当前会话结果。离开会话时若存在未提交内容，必须先确认是否丢弃。

#### 场景:创建会话进入 target 详情页
- **当** 操作员从 `Add Target` 流程进入某个新 target 详情页
- **那么** 系统必须提供 `Create Target --->` 作为会话提交动作
- **并且** 系统不得在该创建会话里显示 `Delete Target --->`

#### 场景:管理会话进入 target 详情页
- **当** 操作员从现有 target 列表进入某个已存在 target 详情页
- **那么** 系统必须提供 `Apply Target --->` 与 `Delete Target --->`
- **并且** 仅当操作者触发 `Apply Target --->` 时，当前会话编辑结果才视为已提交

#### 场景:创建会话未提交即离开
- **当** 操作员在创建会话中修改了 target 字段，但未触发 `Create Target --->` 就按 `Esc` 或触发 `Exit`
- **那么** 系统必须先弹出“未创建 target 草稿是否丢弃”的确认
- **并且** 若确认丢弃，系统必须删除该草稿 target 并返回上一级

#### 场景:管理会话未提交即离开
- **当** 操作员在管理会话中修改了 target 字段，但未触发 `Apply Target --->` 就按 `Esc` 或触发 `Exit`
- **那么** 系统必须先弹出“未应用变更是否丢弃”的确认
- **并且** 若确认丢弃，系统必须回滚该 target 到进入详情页前的基线状态

### 需求:Security 页面必须产品化 SSH key import 与管理入口
`menuconfig` 的 Security 页面在 vault `unlocked` 时必须提供 `Import SSH Key --->` 与 `SSH Key Management --->`；在 `locked` 时不得开放这两个入口，仅允许显示聚合 `SSH Key Count`。

#### 场景:vault unlocked 时进入 Security 页面
- **当** 操作员进入 Security 页面且 vault 为 `unlocked`
- **那么** 系统必须显示 `Import SSH Key --->` 与 `SSH Key Management --->`
- **并且** 导入/管理流程必须只返回 display-safe 字段

#### 场景:SSH key 详情页展示 record-id 与删除动作
- **当** 操作员进入某个已导入 SSH key 的详情页
- **那么** 系统必须以 `record id` 作为当前记录标识字段进行展示
- **并且** 详情页必须提供 `Delete SSH Key --->` 二次确认动作

#### 场景:menuconfig 重复 key name 导入走 delete-first
- **当** 操作员在 menuconfig 中使用同一 `key name` 重复导入 SSH key
- **那么** 系统必须拒绝重复导入并提示先删除旧 key
- **并且** 不得在详情页暴露就地 `Import New Version` 入口

#### 场景:vault locked 时进入 Security 页面
- **当** 操作员进入 Security 页面且 vault 为 `locked`
- **那么** 系统不得开放 SSH key 导入或列表详情浏览
- **并且** 最多只显示聚合 `SSH Key Count`

### 需求:SSH target 的 credential source 必须支持 picker 与 inline import
对于 `kind = ssh` target，`menuconfig` 必须提供 `Credential Source --->` 子流程，支持选择已导入 vault SSH key、在流程内联导入本地 key、以及手工 reference 回退。敏感 SSH target 的该流程仅允许在 unlocked `Sensitive Overlay` 中访问。

#### 场景:操作员为 SSH target 选择 imported key
- **当** 操作员在 SSH target 的 `Credential Source` 里选择 `Use Imported Vault SSH Key`
- **那么** 系统必须展示 display-safe picker，并在选择后把 canonical `credential_ref` 回填到当前 target
- **并且** picker 列表必须使用 `Single-choice Toggle` 行语义（`< >` / `<*>`）
- **并且** 对 picker 当前行必须仅允许 `Space` 触发绑定，`Enter` 不得触发绑定

#### 场景:操作员在 target 流程内联导入 key
- **当** 操作员在 `Credential Source` 里选择 `Import Local SSH Key Into Vault`
- **那么** 系统必须在导入成功后自动把新 canonical `credential_ref` 回填到当前 target
- **并且** 不得要求用户返回后手工粘贴 raw URI

### 需求:Security 页面必须提供正式的 SSH key import 与管理入口
`bridgingio-core menuconfig` 的 Security 页面在 vault 已解锁时，必须提供 `Import SSH Key --->` 与 `SSH Key Management --->` 等正式入口，而不是继续只产品化 vault 状态和 token 管理。系统不得继续要求 operator 退出到外部 CLI 完成 SSH key import，再回到 menuconfig 手填 raw `credential_ref`。

#### 场景:vault unlocked 时进入 Security 页面
- **当** 操作员进入 Security 页面，且当前 vault 状态为 `unlocked`
- **那么** 系统必须展示 `Import SSH Key --->` 与 `SSH Key Management --->`
- **并且** 这些入口必须与 `Create Token --->`、`Token Management --->` 共同构成正式安全管理面，而不是仅作为帮助文本或未实现占位

#### 场景:vault locked 时进入 Security 页面
- **当** 操作员进入 Security 页面，且当前 vault 状态为 `locked`
- **那么** 系统不得允许进入 SSH key import 流程
- **并且** 系统最多只能展示聚合 `SSH Key Count` 或等价静态数量摘要
- **并且** 系统不得展示任何单个 imported SSH key 的 label、canonical `credential_ref`、status 或 record-id 摘要
- **并且** 系统不得把 `SSH Key Management` 伪装为可执行的完整管理入口后再在保存阶段报错

### 需求:menuconfig SSH key import 流程必须使用 display-safe metadata 与受控本地 secret capture
`menuconfig` 的 SSH key import 流程必须把普通可见 metadata 输入与实际私钥材料读取分离。系统必须允许 operator 通过 `key name`、`label`、`source path` 或等价 display-safe 字段组织导入流程，但不得把私钥内容或 passphrase 放进普通单行编辑弹窗、状态栏或常规列表行中。

#### 场景:操作员导入本地 SSH key 文件
- **当** 操作员在 menuconfig 中触发 `Import SSH Key --->`
- **那么** 系统必须先采集 display-safe metadata，再在受控本地流程中读取对应 SSH key 文件
- **并且** 导入成功后的结果页面只能展示 canonical `credential_ref`、label、status、record-id 等安全摘要，而不得再次展示私钥内容
- **并且** 若 canonical ref 已存在，系统必须拒绝重复导入并要求先删除旧 key

#### 场景:导入 encrypted SSH key
- **当** 操作员导入一个 passphrase-protected 的 SSH key 文件
- **那么** 系统必须通过受控本地隐藏输入处理该 passphrase
- **并且** 不得把该 passphrase 写入普通 popup 文本、日志、搜索结果或状态提示

### 需求:SSH Key Management 必须采用列表页 -> 详情页拓扑
`menuconfig` 必须为已导入的 `ssh-private-key` 提供正式的 `SSH Key Management` 列表页与详情页，而不是继续把这类对象隐藏在 generic secret count 或抽象的 vault summary 背后。

#### 场景:查看已导入 SSH key 列表
- **当** 操作员进入 `SSH Key Management`
- **那么** 系统必须以摘要列表展示每个已导入 key 的 label、canonical ref 或等价稳定标识、status 和 record-id 摘要
- **并且** 列表项必须使用 `--->` 导航语义进入详情页

#### 场景:查看 SSH key 详情页
- **当** 操作员进入某个已导入 SSH key 的详情页
- **那么** 系统必须至少展示 canonical `credential_ref`、label、kind、status、record-id 与 rotation / usage 摘要
- **并且** 详情页必须提供 `Delete SSH Key --->` 并要求二次确认
- **并且** 删除成功后必须清理所有绑定该 key 的 target `credential_ref`
- **并且** 详情页不得展示私钥明文、ciphertext locator、unwrap material 或其他非 display-safe 内部字段

#### 场景:vault locked 时不得浏览 SSH key 列表
- **当** 操作员处于 vault `locked` 状态
- **那么** 系统不得开放 `SSH Key Management` 的列表页或详情页浏览
- **并且** 不得通过 locked 态列表泄露任何单个 key inventory 信息

### 需求:`menuconfig` 必须保存 display-safe 的授权与会话诊断日志
`bridgingio-core menuconfig` 必须把关键授权事件和有价值的会话诊断持久化到当前 runtime root 的 `logs/` 下，而不是只保留瞬时 stderr 或内存中的 `last_status`。系统必须把关键业务节点记为 `INFO` 级事件，把关键调用链 breadcrumb 记为 `DEBUG` 级事件；所有落盘内容都必须保持 display-safe，并允许通过稳定 `flow_id` 关联同一次显式操作。

#### 场景:显式授权动作写入 authorization 日志
- **当** 操作员在 `menuconfig` 中触发 `Unlock Vault`、`Delete Vault`、`Import SSH Key`、`Delete SSH Key`、`Create Token` 或 `Delete Token`
- **那么** 系统必须向 `logs/local-authorization.jsonl` 追加 display-safe 事件
- **并且** 每条事件必须至少包含 `flow_id`、`surface=menuconfig`、`screen`、`action`、`operation`、`phase` 与结果摘要

#### 场景:debug 级别写入 menuconfig breadcrumb
- **当** `menuconfig` 运行时 `core.log_level` 为 `debug` 或 `trace`
- **那么** 系统必须向 `logs/menuconfig-session.jsonl` 追加关键 breadcrumb，例如 screen 切换、授权 worker 开始/结束、去重命中、保存/取消等事件
- **并且** 这些 breadcrumb 必须可通过同一 `flow_id` 与授权日志关联

#### 场景:unlock worker breadcrumb 复用授权 flow_id
- **当** `menuconfig` 在 worker 线程内执行 `Unlock Vault` 验证并在主线程汇总结果
- **那么** `logs/menuconfig-session.jsonl` 中 `action=unlock.worker` 的 debug breadcrumb 必须复用本次 `vault.unlock` 的授权 `flow_id`
- **并且** 不得仅使用会话级 flow 导致同一次 unlock 在 authorization/session 两个日志流中无法直接关联

#### 场景:日志默认保持 display-safe
- **当** `menuconfig` 将授权或 session 事件写入 runtime `logs/`
- **那么** 系统不得把 passphrase、私钥明文、token 明文、自由文本敏感输入或等价 secret material 写入这些日志
- **并且** 若日志涉及 SSH key 或 token，只允许记录 canonical `credential_ref`、稳定 `token_id`、label、field path 或等价 display-safe 标识

### 需求:`menuconfig` 的 SSH target 管理页必须提供真实测试连接流程
对于 `kind = ssh` 的 target，`bridgingio-core menuconfig` 必须提供正式的 `Test Connection --->` 入口，用于对当前草稿连接配置执行一次真实 SSH 连通性探测。系统不得把该动作退化为字段完整性校验，也不得要求操作员先 `Apply Target --->` 或 `Save` 才能测试。对于 vault-managed SSH key，该流程必须只在 broker runtime 已真实就绪时才把 endpoint 下发给 SSH；对于 direct identity 文件，该流程必须显式走 identity-file 语义，而不是错误依赖 broker。

#### 场景:plain SSH 在 Connection Profile 中直接测试当前草稿
- **当** 操作员进入一个 plain SSH target 的 `Connection Profile`
- **那么** 系统必须提供 `Test Connection --->`
- **并且** 该动作必须基于当前草稿中的 host、port、username、`credential_ref` 与等价连接字段执行，而不是回退到磁盘上一次保存的基线值

#### 场景:sealed SSH 仅在 unlocked Sensitive Overlay 中允许测试
- **当** 操作员进入一个 sealed SSH target，且 vault 状态为 `unlocked`
- **那么** 系统必须在 `Sensitive Overlay` 中提供 `Test Connection --->`
- **并且** 该动作必须使用当前 overlay 草稿值执行真实 SSH 探测

#### 场景:sealed SSH 在 locked 状态下不得暴露测试入口
- **当** 操作员进入一个 sealed SSH target，且 vault 状态为 `locked`
- **那么** 系统必须继续只显示通用锁定提示与 `Unlock Vault --->`
- **并且** 系统不得显示 `Test Connection --->`、sensitive 连接字段或其他等价测试入口

#### 场景:测试连接采用 timeout -> waiting -> result 的 popup 流程
- **当** 操作员在 SSH target 页面触发 `Test Connection --->`
- **那么** 系统必须先弹出 timeout 输入弹窗，并以毫秒为单位提供默认值 `2000`
- **并且** 在确认后必须进入等待态弹窗
- **并且** 在探测结束后必须展示成功、失败、取消或超时结果弹窗，而不是只在状态栏瞬时输出一条文本

#### 场景:plain SSH 的 vault-backed credential 在 locked 状态下受控失败
- **当** plain SSH target 的当前草稿引用了 vault-managed `credential_ref`，且该 credential 在当前 lock state 下不可用于 SSH secret delivery
- **那么** 系统仍必须允许操作员触发 `Test Connection --->`
- **并且** 测试可以返回受控失败结果
- **并且** 系统不得因此隐式触发 `Unlock Vault` 或把该 plain target 改判为 sealed target

#### 场景:broker endpoint 未就绪时必须返回专属失败
- **当** 当前 SSH 测试连接依赖 broker delivery，且 broker runtime 未能把解析出的本地 endpoint 真正启动到 ready
- **那么** 系统必须返回 broker 专属的受控失败（例如 `broker-endpoint-unavailable`）
- **并且** 结果反馈必须明确提示失败点位于 broker endpoint 就绪性，而不是泛化为普通认证失败
- **并且** 系统不得通过 fallback 私钥路径掩盖该 broker 故障

#### 场景:agent 已返回身份时不得误报 broker 未就绪
- **当** SSH 测试日志已显示 `agent returned` / `Offering public key ... agent`，但认证最终被远端拒绝（例如 `Permission denied (publickey)`）
- **那么** 系统必须将结果归类为认证失败（如 `auth-publickey-rejected`）而非 `broker-endpoint-unready`
- **并且** 错误分类不得因为同一 stderr 中出现无关的 `no such identity` 文本而错误提升为 broker 异常

#### 场景:direct identity 文件测试必须显式旁路 broker
- **当** 当前 SSH 测试连接的 `credential_ref` 直接指向本地 identity 文件，而不是 canonical vault ref
- **那么** 系统必须通过显式 `-i <path>`、`IdentityFile=<path>` 或等价 direct identity 语义执行该次探测
- **并且** 系统不得创建 broker session 或展示 broker unavailable 结果，除非该次探测真实使用了 broker 路径

### 需求:`menuconfig` 的 SSH 测试连接必须只显示简洁结果并保留 display-safe 日志
`menuconfig` 的 SSH 测试连接流程在界面上必须只显示简洁结果摘要，而更详细的执行诊断必须以 display-safe 方式写入 menuconfig 日志。系统不得在结果弹窗、状态栏或普通列表行中回显 raw SSH 错误输出或 secret material。

#### 场景:结果弹窗只显示成功或失败状态
- **当** SSH 测试连接完成
- **那么** 系统必须在结果弹窗中只显示 `Connection succeeded`、`Connection failed`、`Connection cancelled`、`Connection timed out` 或等价简洁状态
- **并且** 当失败分类为 broker endpoint 未就绪时，系统可以显示 `Connection failed (broker unavailable)` 或等价简洁提示
- **并且** 不得在该弹窗中展开 raw stderr、命令全文或 secret 相关内容

#### 场景:测试连接把详细过程写入 display-safe session 日志
- **当** 操作员在 `menuconfig` 中触发一次 SSH 测试连接
- **那么** 系统必须向当前 runtime root 的 menuconfig 日志写入带稳定 `flow_id` 的 display-safe 事件
- **并且** 这些事件至少必须覆盖 started / finished / failed / cancelled / timed-out 等关键节点

#### 场景:测试连接日志不得泄露 secret material
- **当** 系统为 SSH 测试连接写入日志
- **那么** 系统不得记录私钥明文、passphrase、token 明文、raw SSH stderr 或等价 secret material
- **并且** 若日志涉及 credential 或 toolchain，只允许记录 canonical `credential_ref`、工具来源摘要、错误分类或其他 display-safe 标识

#### 场景:broker endpoint 失败必须产出可定位 breadcrumb
- **当** SSH 测试连接因 broker endpoint 未就绪而失败
- **那么** `menuconfig-session` 日志必须记录可关联 `flow_id` 的 display-safe 诊断节点
- **并且** 该节点至少包含 broker 失败分类与 endpoint 就绪性检查结果摘要

### 需求:SSH target 必须提供前置的认证设置流程
对于 `kind = ssh` 的 target，`menuconfig` 必须在字段详情编辑前提供正式的 `SSH Authentication Setup`，并在编辑现有 target 时提供 `SSH Authentication --->` 入口。该流程必须先确定认证类型与允许组合，再进入 host、port、username 等连接细节编辑，而不得继续把认证约束留到 `Create/Apply` 时才失败。

#### 场景:创建 SSH target 时先进入认证设置
- **当** 操作员在 `Add Target` 流程中选择 `SSH`
- **那么** 系统必须在进入 SSH detail editor 前先进入 `SSH Authentication Setup`
- **并且** 该步骤必须允许选择 `none`、`password` 或 `private-key`

#### 场景:编辑 plain 或 sealed SSH target 时存在认证入口
- **当** 操作员编辑一个已存在的 SSH target
- **那么** plain SSH 必须在 `Connection Profile` 中提供 `SSH Authentication --->`
- **并且** unlocked sealed SSH 必须在 `Sensitive Overlay` 中提供 `SSH Authentication --->`

#### 场景:认证类型选择必须使用互斥单选交互
- **当** 操作员在 `SSH Authentication Setup` 中调整 `none` / `password` / `private-key`
- **那么** 系统必须保证三者互斥且必须有且仅有一个被选中
- **并且** `Space` 必须用于切换选择，`Enter` 必须用于进入详情编辑，不得把 `Enter` 作为切换触发键

#### 场景:继续进入详情编辑必须受认证前置条件门控
- **当** 当前认证类型为 `password` 或 `private-key` 且必填字段未完成或校验未通过
- **那么** `继续进入详情编辑` 必须保持禁用
- **并且** 禁用提示必须明确说明“需要填写并通过校验”的阻断原因

#### 场景:plain secret-backed 认证显示可切换的 SSH 安全访问
- **当** plain SSH target 选择 `password` 或本地未加密 `private-key`
- **那么** 系统必须显示 `SSH 安全访问` 的单选 toggle
- **并且** 该 toggle 必须默认开启，但允许明确关闭

#### 场景:sealed secret-backed 认证强制 SSH 安全访问
- **当** sealed SSH target 选择 `password`、本地未加密 `private-key` 或 imported vault key
- **那么** 系统必须把 `SSH 安全访问` 显示为只读的必选状态
- **并且** 不得允许操作员将其关闭

#### 场景:本地私钥路径带 passphrase 时阻断并提示 sealed + vault import
- **当** 操作员在 `SSH Authentication Setup` 中输入本地私钥路径，且系统检测到该私钥带 passphrase
- **那么** 系统必须立即弹出阻断性错误提示
- **并且** 该提示必须明确说明该组合只允许通过 sealed target + vault import 支持

#### 场景:plain private-key 认证不得展示 vault key 来源
- **当** plain SSH target 在 `SSH Authentication Setup` 选择 `private-key`
- **那么** 系统不得展示 `Use Imported Vault Key` 或等价 vault 来源入口
- **并且** 同屏不得出现语义重复的本地 key 来源行

### 需求:menuconfig 必须通过共享模板层构建行语义
`bridgingio-core menuconfig` 在构建菜单页面时，必须通过共享的 row template / group template / popup template 语义层来声明每一行、每一组候选和每一种 overlay 的交互角色，而不是继续由各个 screen 自由拼装前缀、后缀、箭头、按钮区和 `selected/unselected` 状态。模板层必须成为 screen 数据与最终渲染之间的正式边界。

#### 场景:新增页面行时声明模板语义
- **当** 团队为某个 menuconfig screen 新增一行
- **那么** 该 screen 必须先声明该行属于哪个正式模板
- **并且** 模板层必须统一派生其前缀、`--->`、可聚焦性与按键语义

#### 场景:screen 不得手工拼装模板语义
- **当** 某行的视觉或交互语义已经被 row template 覆盖
- **那么** screen 层不得再直接手工拼装 `<*>`、`[ ]`、`--->` 或等价字符串来表达核心交互语义
- **并且** 不得仅依赖字段路径或 action 分支猜测该行“看起来像哪个模板”

#### 场景:popup 不得各自手工复制通用布局
- **当** 某个弹框的结构已经被 popup template 和控件原语覆盖
- **那么** 系统不得继续为其单独复制 `centered rect + clear + block + hint + button row/input line` 等通用布局代码
- **并且** 必须优先复用共享 `modal shell`、`button row`、`input line`、`choice list` 或等价原语

### 需求:纯开关与互斥单选必须使用不同模板
`menuconfig` 必须将“单个布尔值的纯开关”和“多个候选中只允许一个被选中”的互斥单选明确建模为不同模板，而不是继续共享同一含混术语。系统必须确保二者在设计、实现和测试中都能够被独立识别。

#### 场景:纯开关使用 boolean toggle template
- **当** 某个配置项本身只有 `on/off` 语义，且不属于一组互斥候选
- **那么** 系统必须使用 `boolean toggle` 或等价模板
- **并且** 该模板不得要求组内互斥语义

#### 场景:互斥候选项使用 exclusive choice template
- **当** 某个 screen 暴露多个候选项，且任意时刻只允许一个被选中
- **那么** 系统必须使用 `exclusive choice` 或 `exclusive choice entry` 模板以及对应的 group template
- **并且** 必须保证组内恰好一个候选被选中

### 需求:阻断态与强制态必须使用正式模板
`menuconfig` 对于“目标动作存在但当前不可进入”的阻断场景，以及“状态被系统强制固定”的只读场景，必须使用正式模板表达，而不是继续退化成普通说明行或伪装成交互开关。

#### 场景:继续进入详情编辑的门控使用 blocked template
- **当** `SSH Authentication Setup` 或其他流程中的后续动作因为前置条件不满足而不可继续
- **那么** 系统必须把该行建模为 `blocked action` 或等价模板
- **并且** 文案必须直接说明阻断原因

#### 场景:强制安全状态使用 required readonly template
- **当** 某个目标状态被策略要求始终开启，例如 sealed secret-backed auth 的 `SSH 安全访问`
- **那么** 系统必须使用 `required readonly` 或等价模板
- **并且** 不得将其继续建模为可通过 `Space` 切换的 toggle

### 需求:menuconfig 必须通过统一 overlay model 管理弹框
`menuconfig` 对于确认、输入、等待、结果、帮助、一次性 reveal 和 blocking overlay 等弹框，不应继续通过多个互不相干的布尔值、`Option` 或流程状态字段分别管理。系统必须具备统一 overlay model 的正式设计方向，使新增弹框能够复用共享优先级、关闭规则和渲染入口。

#### 场景:新增 overlay 时不再扩展硬编码优先级链
- **当** 团队为 `menuconfig` 新增一种 popup 或 overlay
- **那么** 该设计必须能够映射到统一 overlay model
- **并且** 不得要求再往事件循环里继续硬编码追加一段新的“如果某字段存在就先处理它”的优先级分支

#### 场景:等待态和结果态属于正式 overlay 模型
- **当** `menuconfig` 进入 unlock waiting、SSH test waiting、unlock result、SSH test result 或等价中间态/结果态
- **那么** 系统必须把这些状态视为正式 overlay template 的实例
- **并且** 不得把它们继续视为与普通 popup 毫无关系的特殊流程分支

