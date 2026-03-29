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

