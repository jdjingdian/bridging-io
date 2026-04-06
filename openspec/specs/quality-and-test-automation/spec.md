# quality-and-test-automation 规范

## 目的
待定 - 由归档变更 define-bridgingio-foundation 创建。归档后请更新目的。
## 需求
### 需求:核心实现必须附带单元测试
BridgingIO 的 Rust 核心实现必须为新增或修改的 domain、artifact、policy、connector、provider 和 MCP 适配层提供自动化单元测试。每个功能交付禁止只提交实现代码而不提交对应的核心测试。

#### 场景:新增核心能力时交付测试
- **当** 项目新增一个 Rust 核心模块或对现有核心模块进行行为修改
- **那么** 该变更必须同时包含可自动运行的单元测试，用于验证关键行为、错误分支或边界条件

### 需求:跨模块关键流程必须可自动验证
对于目标连接、会话生命周期、artifact 派生、审批策略和 Git 查询等跨模块流程，系统必须提供自动化集成验证，以确保多个模块协同时的行为符合规范。

#### 场景:验证命令执行与 artifact 流程
- **当** 系统实现一次完整的目标连接、命令执行和 artifact 生成流程
- **那么** 自动化测试必须能够验证该流程中的会话状态、输出缓存和后续读取结果

### 需求:每个平台 UI 必须附带 UI Test
BridgingIO 的每个平台 UI 实现都必须提供平台原生或等价能力的 UI Test，并且这些 UI Test 必须覆盖关键用户流。关键用户流至少必须包括目标选择或创建、会话状态查看、命令时间线浏览、artifact 查看或过滤，以及审批请求处理。

#### 场景:macOS 控制台交付前验证关键界面流程
- **当** macOS SwiftUI 控制台新增或修改目标管理、命令时间线、artifact 详情或审批相关界面
- **那么** 项目必须同时提供能够自动验证这些关键界面流程的 UI Test

#### 场景:后续新增其他平台 UI
- **当** 项目未来新增 Linux、Windows 或鸿蒙 PC 的 UI 实现
- **那么** 该平台实现必须附带覆盖相同关键用户流的 UI Test，而不能只依赖 macOS UI Test 或手工验证

### 需求:core 必须具备跨平台契约测试基线
BridgingIO 的 Rust core 必须建立独立于 UI 的 cross-platform contract test 基线，用于验证宿主平台相关的关键能力。对于 `design-vault-and-agent-auth` 已经产品化且属于 contract-critical 的安全边界，`bridgingio-core --self-test` 必须额外覆盖 canonical `CredentialRef` 归一化、degraded vault fail-closed、broker-only secret use、本地管理员 `intent + attestation` 单次消费、secret-backed SSH delivery lifecycle，以及 non-loopback model-plane 安全默认值。

#### 场景:执行包含 vault / auth smoke 的 self-test
- **当** 团队执行 `bridgingio-core --self-test` 作为 core contract smoke
- **那么** 自检必须验证 canonical `vault://...` 归一化、degraded backend fail-closed、本地管理员验证单次消费，以及 secret-backed SSH delivery 的 broker / fallback 语义，而不能只验证 shell 与 platform contract

#### 场景:验证 model-plane 安全默认值
- **当** 团队执行 `bridgingio-core --self-test`
- **那么** 自检必须验证 non-loopback model-plane 暴露仍要求显式 enable 与认证保护，而不能把 `auth_mode=none` + 非 loopback 暴露当作可接受默认值

### 需求:跨平台自动化测试不得默认依赖 Unix 专有假设
Rust 核心的集成测试与 fixture 在未显式声明仅限 Unix 的情况下，不得默认依赖 `/tmp`、`/bin/sh`、`pwd`、`printf`、Unix socket 路径或其他 Unix 专有假设。平台差异必须通过 adapter、fixture 或显式平台分层表达，而不是隐式埋在测试脚本中。

#### 场景:新增跨平台集成测试
- **当** 团队为 terminal、IPC、runtime 或 toolchain 增加新的跨平台集成测试
- **那么** 该测试必须通过平台 fixture 或适配层选择路径、命令和 transport，而不是直接硬编码 Unix 默认值

#### 场景:保留仅 Unix 的测试
- **当** 某个测试确实只验证 Unix 专有能力，例如 PTY 行为
- **那么** 该测试必须显式标注平台范围，并由独立的跨平台 contract 测试补齐通用行为，而不是让 Unix-only 测试隐式代表全部平台真相

### 需求:interactive shell 自动化验证必须采用确定性同步机制
对于 interactive shell 的自动化测试，系统必须优先使用 marker、poll、readiness 或等价的确定性同步机制，而不是依赖固定睡眠时间窗口推测命令是否完成。

#### 场景:structured launch fallback 路径的状态一致性验证
- **当** 测试触发 `terminal.shell.open` 的 structured interactive launch 失败并进入 fallback 路径
- **那么** 测试必须断言 read/interrupt/close 的 `running`、`interrupted`、`closed` 状态语义保持确定性，并验证 fallback 诊断可见

### 需求:跨平台桌面控制台关键流必须具备自动化 UI 覆盖
跨平台桌面控制台的 bundled GUI 实现必须提供自动化 UI Test，覆盖首启引导、主工作台导航、timeline 来源分组、以及 settings / token / vault 等关键管理流。项目不得仅依赖现有 macOS UI Test 或手工验证来代表跨平台桌面控制台的验收。

#### 场景:验证首次启动目录选择流
- **当** 团队为跨平台桌面控制台交付首轮 bundled GUI 版本
- **那么** 自动化 UI Test 必须覆盖首次启动进入引导页、选择 runtime 数据目录、以及目录失效后的恢复路径，而不是只验证已连接主界面

#### 场景:验证 timeline 来源分组
- **当** 团队交付桌面控制台的 Timeline 页面
- **那么** 自动化 UI Test 必须验证“按 token label 分组”和“按请求指纹 / user-agent 摘要分组”这两类来源归因路径，而不是只检查时间线列表是否能渲染

#### 场景:验证 token revoke 与 vault 解锁入口
- **当** 团队交付桌面控制台的 Settings / Security 页面
- **那么** 自动化 UI Test 必须验证 token 摘要列表可见、revoke 可执行，以及 vault 解锁入口会进入受控本地验证流程，而不是只验证设置页静态表单存在

### 需求:桌面宿主项目必须具备可执行的启动 smoke 验证
跨平台桌面控制台的宿主项目必须提供可执行的启动 smoke 验证，用于证明仓库中的 Tauri 宿主、runtime root 分流、sidecar 启动配置与本地桥接链路能够协同工作。项目禁止仅依赖静态 HTML contract tests 或设计文档来宣称 bundled GUI 已可启动。

#### 场景:验证受支持平台上的桌面宿主启动
- **当** 团队在受支持宿主平台上执行桌面宿主 smoke 验证
- **那么** 验证流程必须至少确认 Tauri 宿主可启动、主窗口可装载 bundled 页面、runtime root 分流可决策，以及 managed core sidecar 启动参数与 attach/bootstrap 链路可被执行或被可信桩验证，而不是只检查 HTML 文件存在

#### 场景:宿主平台缺失本地 transport 时受控失败
- **当** 团队在尚未补齐平台原生 local transport 的宿主平台上执行桌面宿主 smoke 验证
- **那么** 验证必须返回明确的 unsupported、deferred 或等价受控诊断，指出阻塞点位于本地 control-plane transport，而不是把宿主启动失败误报为页面或配置层问题，更不能把该平台标记为“已通过跨平台启动验收”

### 需求:桌面 token 一次性签发结果必须具备自动化 UI 覆盖
跨平台桌面控制台的 bundled GUI 实现必须提供自动化 UI Test，验证长期 token 签发成功后的一次性结果展示、copy 入口以及结果关闭后的 display-safe 行为。项目不得仅通过手工点击确认“创建成功”文案来代表该交互已完成验收。

#### 场景:验证签发成功后展示一次性结果面板
- **当** 团队交付桌面控制台中的长期 token 签发交互
- **那么** 自动化 UI Test 必须验证签发成功后可见正式的一次性结果面板，并且该面板包含明文 token 展示与 copy 入口，而不是只验证状态栏出现成功提示

#### 场景:验证关闭结果后列表仍保持安全摘要
- **当** 自动化 UI Test 在签发成功后关闭一次性结果面板、刷新 token 摘要列表或重新进入设置与安全管理区域
- **那么** 测试必须验证 token 列表继续只显示 display-safe 摘要，且不会再次回显先前签发响应中的明文 token

### 需求:canonical vault productization 必须进入 self-test 与集成验证
在 canonical vault runtime 落地后，`bridgingio-core --self-test`、control-plane contract test 与相关集成测试必须覆盖 vault persistence、locked fail-closed、attestation enforcement、canonical config migration 与 secret-backed SSH lifecycle，而不能继续只验证内存态 shim 或静态 settings projection。

#### 场景:执行包含 canonical vault 的 self-test
- **当** 操作员执行 `bridgingio-core --self-test`
- **那么** 自检必须验证至少以下路径：canonical `vault://...` 归一化、legacy vault backend 配置迁移诊断、initialized-but-locked fail-closed、passphrase unlock、attestation single-use、以及 SSH broker basic lifecycle

#### 场景:验证 synthetic attestation 被拒绝
- **当** 测试向长期 token 签发或 vault unlock 路径传入 synthetic 或过期 attestation
- **那么** 自动化测试必须验证 runtime 明确拒绝该请求，而不是允许 trusted client 自报验证通过

### 需求:桌面与 control-plane 自动化必须验证真实 vault 管理流
桌面 UI automation 与 control-plane contract automation 必须验证真实的 vault 管理流，包括 lock state 投影、unlock flow、token 签发 one-time reveal、token revoke 和 settings-vault 受信任动作，而不是只验证按钮存在或占位命令返回 deferred。

#### 场景:桌面 UI 验证 vault unlock
- **当** 自动化 UI test 运行 `Settings > Vault` 解锁流程
- **那么** 测试必须验证页面先展示真实 `locked/unavailable/uninitialized` 等状态，并在完成 trusted local verification 后触发正式 `unlock_vault` control-plane command

#### 场景:control-plane 验证 token 一次性显示
- **当** 自动化 contract test 通过 trusted control-plane 创建长期 token
- **那么** 测试必须验证创建响应包含一次性明文结果，而后续 list/query 仅返回 summary

### 需求:core contract 与 self-test 必须维护平台/架构/宿主模式矩阵
BridgingIO 的 core contract automation 与 `--self-test` 文档必须维护正式 matrix，至少覆盖平台、架构、宿主模式、build profile、case id、命令与预期结果。该矩阵必须明确区分当前支持的平台集合与长期规划平台集合。

#### 场景:记录当前支持与长期规划
- **当** 团队更新 core contract 或 self-test matrix
- **那么** 文档必须明确将 macOS、Linux x86_64、Linux aarch64 和 Windows 标为当前目标，将 OpenHarmony 标为长期规划占位，而不能混写为同一支持等级

### 需求:`--self-test` 必须作为 debug-only 运行时测试框架管理
`bridgingio-core --self-test` 必须在质量矩阵中被明确建模为 debug-only 的运行时测试框架。debug 构建必须支持该入口；release 构建必须拒绝该入口，而不是继续把它当作生产模式的常规运行面。

#### 场景:debug 构建执行 self-test
- **当** 团队使用 debug 构建执行 `bridgingio-core --self-test`
- **那么** 系统必须运行完整的运行时测试框架并按正式 case matrix 返回结果

#### 场景:release 构建执行 self-test
- **当** 操作员或自动化在 release 构建上执行 `bridgingio-core --self-test`
- **那么** 系统必须明确拒绝该入口并返回清晰诊断，而不是继续执行完整运行时测试

### 需求:非 Linux 宿主验证必须补齐同架构 Linux contract run
当团队在非 Linux 宿主上编译或验证 BridgingIO core 时，必须通过 `cross` 或等价机制补齐同架构 Linux contract run，而不是只以本机宿主测试通过作为验收标准。

#### 场景:Apple Silicon macOS 验证 core
- **当** 团队在 Apple Silicon macOS 上编译并验证 BridgingIO core
- **那么** 自动化或手工验收必须补齐 Linux aarch64 contract run，而不是只运行 macOS 本机测试

#### 场景:x86_64 非 Linux 宿主验证 core
- **当** 团队在 x86_64 的 macOS 或 Windows 宿主上编译并验证 BridgingIO core
- **那么** 自动化或手工验收必须补齐 Linux x86_64 contract run，而不是只以本机宿主结果作为最终结论

### 需求:core operator surface 的本地化与版本合同必须具备自动化校验
对于 `bridgingio-core` 的 locale catalog、help/version 输出与 operator-facing 显示路径，系统必须提供自动化校验，确保中英文键集合一致、版本真相唯一且展示层不回流硬编码文本。项目禁止只靠人工 review 保证这些合同。

#### 场景:验证中英文 catalog 键集合一致
- **当** 团队新增或修改 core operator surface 文案键
- **那么** 自动化校验必须验证 `zh-CN` 与 `en-US` 的键集合保持一致，而不是允许某个语言缺 key 后在运行时才暴露问题

#### 场景:验证 help 和 version 合同
- **当** 团队执行与 `bridgingio-core` CLI 相关的自动化测试
- **那么** 测试必须验证 `--help` / `help` / `--version` 的关键输出结构、locale 切换行为与版本值来源，而不是只验证命令执行没有崩溃

### 需求:operator-facing 硬编码显示文本必须被自动阻断
对于 `bridgingio-mcp` CLI 与 `bridgingio-operator-console` 的 operator-facing 显示路径，项目必须提供自动化检查以阻断新增硬编码展示文本。允许保留稳定 machine-readable key、字段路径和错误码，但禁止把用户可见文案直接写回渲染路径。

#### 场景:新增 menuconfig 状态提示
- **当** 团队在 `menuconfig` 中新增状态栏提示、帮助说明或弹窗文案
- **那么** 自动化检查必须能够发现这些显示文本是否绕过 catalog 直接硬编码，而不是等到人工体验时才发现语言漂移

#### 场景:新增 CLI 子命令说明
- **当** 团队为 `bridgingio-core` 新增命令说明、参数帮助或 about 文案
- **那么** 自动化检查必须验证这些 operator-facing 文案走统一 catalog / 命令元数据入口，而不是直接拼接字面量字符串

### 需求:`menuconfig` 的显式解锁时机与反馈流程必须有回归覆盖
针对 `bridgingio-core menuconfig` 的自动化或 contract 测试必须覆盖显式解锁时机与结果反馈，特别是 macOS `os-native` 路径。测试禁止只验证解锁入口存在，而必须验证进入界面时不触发系统验证、显式 unlock 后才进入等待态，以及成功或失败后 UI 状态完成切换。

#### 场景:macOS 进入 menuconfig 不触发系统验证
- **当** 自动化在 macOS 上启动 `bridgingio-core menuconfig`，且 vault 当前处于 `locked`
- **那么** 测试必须验证界面先显示 locked 摘要，且在未按下 `u` 或未触发解锁入口前不会触发系统钥匙串验证

#### 场景:显式解锁后完成成功状态切换
- **当** 自动化在 macOS `menuconfig` 中显式触发 vault 解锁，并模拟本地验证成功
- **那么** 测试必须验证界面依次经历等待态、成功确认态，并在确认返回后移除 unlock 提示并显示已解锁状态

#### 场景:显式解锁失败后保持 locked
- **当** 自动化在 `menuconfig` 中显式触发解锁，但本地验证失败或被取消
- **那么** 测试必须验证界面回到 locked 状态并显示明确失败反馈，而不是误报为已解锁

#### 场景:单次显式解锁不会触发重复认证链路
- **当** 自动化在 `menuconfig` 中触发一次 `u` 解锁
- **那么** 测试必须验证初始化和显式解锁路径不会叠加触发多次系统认证流程，且不会进入隐藏 passphrase 回退阻塞

#### 场景:等待态支持 Esc 取消且不会误报成功
- **当** 自动化在等待本地验证时发送 `Esc`
- **那么** 测试必须验证 UI 立即退出等待态并进入取消反馈，且最终状态保持或恢复为 locked，不得出现“取消后仍报解锁成功”

#### 场景:Security 菜单关键状态语义有样式回归覆盖
- **当** 自动化渲染 Security 菜单
- **那么** 测试必须验证 `Vault 锁状态` 的值高亮遵循 `locked=红色`、`unlocked=绿色`，并验证 `解锁 Vault --->` 与 `-*- 加密项管理已解锁` 的互斥呈现规则

### 需求:授权观测与单次触发合同必须具备自动化回归覆盖
针对 `menuconfig` 与本地受信任授权链路的自动化测试，必须覆盖授权日志落盘、display-safe 边界和 verified `os-native` 的 singleflight 行为，而不是只验证“按钮存在”或“最终状态变成 unlocked”。测试必须能够回答一次显式授权是否产生了稳定 flow、记录了哪些关键事件，以及底层平台验证是否只被调用一次。

#### 场景:显式授权动作产生持久化日志
- **当** 自动化在 `menuconfig` 中触发一次显式授权动作，例如 `Unlock Vault` 或 `Delete Token`
- **那么** 测试必须验证 `logs/local-authorization.jsonl` 中存在对应的 display-safe 事件
- **并且** 该事件必须包含稳定 `flow_id`、`operation`、`phase` 与结果字段

#### 场景:debug 级别产生 session breadcrumb
- **当** 自动化以 `core.log_level=debug` 或更高等级运行 `menuconfig`
- **那么** 测试必须验证 `logs/menuconfig-session.jsonl` 中出现与授权 flow 可关联的 breadcrumb 事件
- **并且** 至少覆盖 screen/action、worker 生命周期或去重命中等高价值节点

#### 场景:授权日志保持 display-safe
- **当** 自动化执行涉及 passphrase、SSH 私钥或 token 一次性结果的本地授权流程
- **那么** 测试必须验证新落盘日志不包含私钥明文、passphrase、token 明文或等价 secret material

#### 场景:并发 verified os-native 解锁只访问一次 provider
- **当** 自动化在同一进程内并发触发两个 verified `os-native` 解锁请求
- **那么** 测试必须验证平台 provider / keyring 访问只发生一次
- **并且** 必须验证结果中同时存在 `leader` 与 `joined` 或等价去重语义，而不是两个互不相关的验证流程

#### 场景:unlock worker 跨线程汇总保持 flow 与 dedupe 一致
- **当** 自动化通过 `menuconfig` unlock worker 路径触发一次显式 `Unlock Vault`
- **那么** 测试必须验证 `logs/local-authorization.jsonl` 的 unlock 成功事件含有非 `not-applicable` 的 dedupe 语义（例如 `leader` 或 `joined`）
- **并且** 必须验证 `logs/menuconfig-session.jsonl` 中 `action=unlock.worker` 事件复用同一个授权 `flow_id`

### 需求:`menuconfig` 的 SSH 测试连接流程必须具备自动化回归覆盖
针对 `bridgingio-core menuconfig` 的自动化或 contract 测试必须覆盖 SSH `Test Connection` 的关键交互和结果语义。测试不得只验证按钮存在，而必须验证其真实探测、状态门控、取消/超时行为与日志边界。

#### 场景:plain SSH 可以在未 Apply/Save 前测试成功
- **当** 自动化在 `menuconfig` 中修改一个 plain SSH target 的 host、port、username 或 `credential_ref` 草稿，但尚未触发 `Apply Target --->` 或 `Save`
- **那么** 测试必须验证 `Test Connection --->` 仍可对当前草稿执行真实 SSH 探测
- **并且** 成功结果不得依赖磁盘上旧配置

#### 场景:测试连接覆盖失败、超时与取消路径
- **当** 自动化触发一次 SSH 测试连接，并分别模拟认证失败、超时与等待态 `Esc` 取消
- **那么** 测试必须验证界面分别返回明确的失败、超时与取消结果
- **并且** 不得出现取消后仍误报成功的情况

#### 场景:sealed SSH 的测试入口受 unlock 状态门控
- **当** 自动化分别在 vault `locked` 与 `unlocked` 状态下进入 sealed SSH target
- **那么** 测试必须验证 `locked` 状态下不显示 `Test Connection --->`
- **并且** 必须验证 `unlocked` 状态下可在 `Sensitive Overlay` 中触发该动作

#### 场景:plain SSH 的 vault-backed credential 不触发隐式 unlock
- **当** 自动化对一个绑定 vault-managed credential 的 plain SSH target 触发测试连接，且 vault 当前为 `locked`
- **那么** 测试必须验证该动作返回受控失败结果
- **并且** 不得隐式触发 vault unlock 流程

#### 场景:broker endpoint 缺失时返回专属失败并可定位
- **当** 自动化对依赖 broker delivery 的 SSH target 触发测试连接，且 `IdentityAgent` endpoint 缺失或不可访问
- **那么** 测试必须验证结果被归类为 broker 专属失败（例如 `broker-endpoint-unavailable`），而不是普通认证失败
- **并且** 必须验证结果反馈包含“broker endpoint 未就绪”或等价可定位提示
- **并且** 不得通过 fallback 本地私钥路径把该用例误判为成功

#### 场景:测试连接日志保持 display-safe
- **当** 自动化执行一次成功或失败的 SSH 测试连接
- **那么** 测试必须验证 `logs/menuconfig-session.jsonl` 中存在带稳定 `flow_id` 的相关事件
- **并且** 必须验证这些事件不包含私钥明文、passphrase、token 明文或 raw SSH stderr

### 需求:跨平台 SSH broker runtime 必须具备真实 readiness 与 bypass 自动化覆盖
对于 vault-managed SSH key 的 broker runtime、degraded fallback 与 direct identity bypass，系统必须提供跨平台自动化验证，证明 readiness 对应真实可用 endpoint、cleanup 语义成立、direct identity 不依赖 broker，且相关日志与错误分类保持 display-safe。

#### 场景:Unix 与 Windows contract automation 验证 broker 真正 ready
- **当** 团队在受支持宿主平台上执行 SSH broker contract automation 或等价 self-test
- **那么** 自动化必须验证 broker runtime 已真实 bind/listen 平台本地 endpoint 后才进入 `ready`
- **并且** 必须验证该 endpoint 可以完成最小但完整可用的 SSH publickey auth 交互，而不是只验证 locator 字符串存在

#### 场景:broker startup 失败不得伪装为 ready
- **当** 自动化测试模拟 broker runtime 在 bind、listen、adapter startup 或清理阶段失败
- **那么** 测试必须验证系统返回受控的 broker startup failure、degraded 或 unsupported 结果
- **并且** 不得允许一个未真实就绪的 endpoint 被记录为 ready 并下发给 SSH 客户端

#### 场景:direct identity bypass 保持独立合同
- **当** 自动化测试使用本地 identity 文件而不是 canonical vault ref 发起 SSH 探测或执行
- **那么** 测试必须验证运行时走显式 `-i <path>`、`IdentityFile=<path>` 或等价 direct identity 路径
- **并且** 必须验证该调用不创建 broker session，且失败时不会误报为 broker unavailable

#### 场景:broker cleanup 与日志边界具备自动化断言
- **当** 自动化测试完成一次 vault-backed SSH one-shot、interactive attach/detach 或 timeout/cancel 流程
- **那么** 测试必须验证 broker endpoint、runtime handle、内存 signer material 与任何 degraded fallback 文件都进入清理流程
- **并且** 必须验证相关日志只包含 display-safe `flow_id`、`credential_ref`、状态与错误分类，而不包含私钥明文、passphrase、token 明文或 raw sign payload

#### 场景:local cross-platform preflight 在 Windows 受控模式执行 named-pipe runtime contract
- **当** 开发者执行 `python3 scripts/testing/run-local-cross-platform-preflight.py --target windows`，且当前 mode 命中 `[windows.contracts].modes`（默认包含 `compile-only` 与 `extended`）
- **那么** preflight 必须在 Windows host 上执行 named-pipe runtime contract probe 命令，覆盖实际 runtime 启动路径
- **并且** 该 probe 至少应包含可验证 `bridgingio-core --self-test` 中 broker ready/cleanup 合同的步骤
- **并且** probe 失败必须使该 Windows host preflight 结果失败，而不是仅记录 warning

#### 场景:Windows extended preflight 的功能测试必须具备跨平台可移植断言
- **当** 团队在 Windows host 执行 `--mode extended` 并进入 workspace 级功能测试（含 `integration_workflows`、providers、secrets 回归）
- **那么** 测试基线必须使用可在 Windows 真实解析的 mock 可执行命名与调用约定（例如 `ssh.bat` / `adb.bat`），避免因宿主可执行解析差异导致 `resolved_target_id`、`resolution_state` 等结构化断言失真
- **并且** 对宿主环境工具缺失（如 `git`）的断言应采用显式 skip/guard，而不是让与 broker runtime 无关的外部依赖缺失污染合同结论
- **并且** 涉及文件路径与交互 shell cwd 的断言必须使用分隔符归一化与 backend 能力分层：非 degraded backend 维持严格断言，degraded backend 允许受控放宽但必须输出可诊断标记

