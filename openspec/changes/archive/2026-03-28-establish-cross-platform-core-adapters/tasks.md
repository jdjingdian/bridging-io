## 0. 建议工作流拆分

- [x] 0.1 将本 change 拆分为工作流 A：`host-platform-foundation`，负责 `HostPlatformAdapter`、`ControlPlaneTransport`、`RuntimePaths`、`RuntimeLogger`、`OutputDecoder` 的边界与 baseline 实现
- [x] 0.2 将本 change 拆分为工作流 B：`terminal-runtime-and-dialects`，负责 `LocalShellRuntime`、interactive shell 状态模型、structured invocation、`TargetShellDialect` 与 MCP 执行链路收束
- [x] 0.3 将本 change 拆分为工作流 C：`platform-contract-and-parity`，负责 `ToolchainLocator`、`NativeVaultBinding`、cross-platform contract、自检扩展、平台矩阵测试与文档收口
- [x] 0.4 明确工作流依赖：A 先于 B；A 与 C 可部分并行；B 的执行链路收口先于 C 的最终平台契约验收
- [x] 0.5 为每个工作流定义单独完成标准，避免实现阶段继续以“大草案整体完成”作为唯一验收口径
- [x] 0.6 固化关键决策：Windows local transport 目标架构为 named pipe、Windows host shell 首版默认 `cmd`、文本输出首版采用平台默认编码加诊断、structured invocation 为长期唯一真相源

## 1. 规格与边界定义

- [x] 1.1 在 design / specs 中统一术语，明确 `host platform`、`target shell dialect`、`local transport`、`runtime paths`、`structured invocation` 的精确定义
- [x] 1.2 建立 “宿主平台 vs 目标方言” 的组合矩阵，至少覆盖 `unix host + ssh-posix`、`windows host + adb-android-shell`、未来 `unix host + ssh-windows-cmd`
- [x] 1.3 盘点当前代码中与宿主平台相关的实现入口，覆盖 `bridgingio-providers`、`bridgingio-mcp`、`bridgingio-connectors`、`bridgingio-engine`、`bridgingio-secrets`
- [x] 1.4 盘点当前样例配置、自生成配置与测试 fixture 中的 Unix-only 假设，形成迁移清单
- [x] 1.5 定义本次重构的完成标准，明确哪些问题属于“先立边界”，哪些问题属于“后续平台增量实现”
- [x] 1.6 明确本次不进入实现的 future dialect 范围，例如 `ssh-windows-cmd`、`ssh-powershell` 仅保留接口，不在首轮交付中承诺完整行为
- [x] 1.7 在规格层显式记录已拍板的架构决策，并将尚未确定的内容收敛为有限 open questions

## 2. HostPlatformAdapter 顶层骨架

- [x] 2.1 设计 `HostPlatformAdapter` 顶层接口，明确它负责 `LocalShellRuntime`、`ControlPlaneTransport`、`RuntimePaths`、`ToolchainLocator`、`NativeVaultBinding`、`RuntimeLogger`、`OutputDecoder`
- [x] 2.2 定义 adapter 的装配方式，明确 core 在启动时如何按宿主平台选择具体实现
- [x] 2.3 设计统一 diagnostics 接口，使 adapter 能报告 `unsupported`、`degraded`、`fallback` 等状态
- [x] 2.4 提供 Unix baseline adapter 骨架，先把当前 macOS / Linux 行为收束到 adapter 内
- [x] 2.5 提供 Windows baseline adapter 骨架，先定义 named pipe / `cmd` / platform-default-decoding 的能力边界与占位实现点
- [x] 2.6 定义 adapter 与 domain / engine / MCP 之间允许暴露的数据形状，避免把平台特有类型泄漏到核心模型
- [x] 2.7 为 adapter 顶层接口定义 crate 放置位置与依赖方向，避免 `engine -> mcp -> providers` 形成新的循环依赖
- [x] 2.8 定义 adapter 初始化时的 capability discovery / diagnostics 暴露方式，确保 UI 与 MCP 都能读取平台层状态

## 3. Local Shell Runtime 重构

- [x] 3.1 将 one-shot shell 启动逻辑从 `bridgingio-providers` 收束到 `LocalShellRuntime`
- [x] 3.2 将 interactive shell 启动、PTY/pipe fallback 和 stdin/stdout/stderr 管理迁移到 `LocalShellRuntime`
- [x] 3.3 将 interrupt、close、completion marker 与长任务 running 状态判定统一到 `LocalShellRuntime`
- [x] 3.4 重构 interactive shell 的 `cwd` / `env` / prompt 状态维护，减少对 `pwd`、`cd`、`export`、`unset` 等命令文本匹配的依赖
- [x] 3.5 定义 Windows 与 Unix 在 interactive shell 上的最小等价语义，包括 `cwd`、`env`、interrupt、close、prompt、degraded mode，并明确 Windows baseline 首版以 `cmd` 为默认宿主 shell
- [x] 3.6 为 shell runtime 补充确定性的完成信号与状态查询接口，支撑后续测试去掉固定 sleep 断言
- [x] 3.7 将 one-shot exec artifact 捕获与 interactive artifact 捕获接入统一输出解码路径，避免 shell runtime 与 artifact 层各自处理文本
- [x] 3.8 定义 interactive shell 在 pipe backend 下的降级 transcript 语义，明确哪些状态必须可见、哪些能力允许退化
- [x] 3.9 为 local shell runtime 明确 API 分层：启动、写入、读取、状态查询、中断、关闭、诊断

## 4. Connector Invocation 与 Target Shell Dialect 重构

- [x] 4.1 盘点当前 `bridgingio-connectors` 已有的 `CommandInvocation` 与 interactive invocation 能力，确定缺失字段
- [x] 4.2 扩展 structured invocation 模型，使其能完整表达 one-shot、interactive、target override、全局 override 与 built-in fallback 的最终结果
- [x] 4.3 让 `bridgingio-mcp` 的 `terminal.exec` 改为消费 structured invocation，而不是再次拼接 SSH / ADB shell 字符串
- [x] 4.4 让 `bridgingio-mcp` 的 `terminal.shell.open` 改为消费 structured interactive invocation，而不是走独立字符串启动路径
- [x] 4.5 删除或收束 MCP 与 provider 中重复的 shell quoting / command build 逻辑，避免多套真相源并存
- [x] 4.6 引入 `TargetShellDialect` 模型，先覆盖 `ssh-posix` 与 `adb-android-shell`
- [x] 4.7 为未来 `ssh-windows-cmd` / `ssh-powershell` 等远端方言预留扩展接口与默认语义
- [x] 4.8 让 `targets.providers.terminal.shell` 或等价 target 级配置真正参与 target dialect / runtime 选择，而不再只是可回写字段
- [x] 4.9 将 `inspect_basic`、diagnostics、one-shot exec、interactive shell 的连接器解析统一到同一套 invocation pipeline
- [x] 4.10 为 invocation 模型定义可序列化诊断视图，方便 control-plane / MCP 回显最终生效的程序、参数与来源层级
- [x] 4.11 明确哪些 quoting / escaping 属于 host shell runtime，哪些属于 target dialect，避免责任边界再次模糊
- [x] 4.12 如果 structured invocation 的实现迁移面超出本次 change 的可控范围，则单独创建后续提案承接执行链路改造，但保持本 change 中的设计决策不变

## 5. Control Plane Transport 与 Runtime Paths

- [x] 5.1 定义 `ControlPlaneTransport` 抽象，明确 attach、request/response、lifecycle 和 endpoint 语义
- [x] 5.2 把当前 Unix socket 实现收束到 Unix transport adapter
- [x] 5.3 为 Windows 设计 named pipe 语义、endpoint 命名规则与 diagnostics 暴露方式
- [x] 5.4 将 Windows local transport 的完整实现标记为后续平台 UI 推进时的增量任务，而不是当前 macOS-only 阶段的阻塞项
- [x] 5.5 为 transport 不可用、权限不足或当前平台暂未实现场景补充明确错误与 diagnostics，而不是只打印运行日志
- [x] 5.6 定义 `RuntimePathsAdapter`，统一处理 `data_dir`、`metadata_path`、`artifact_root`、`temp_dir`、control-plane endpoint、logs 目录
- [x] 5.7 重构 `expand_tilde_path` 与相关路径生成逻辑，使其不再只依赖 `HOME`
- [x] 5.8 改造 bundled / UI-managed 自动生成配置，使 endpoint、metadata path、artifact root 默认值按宿主平台可工作
- [x] 5.9 改造 standalone 样例配置，使默认值改为平台中性占位或按平台可工作的样例表达
- [x] 5.10 为 runtime bootstrap 增加平台路径合法性与可写性检查，明确失败时的恢复路径
- [x] 5.11 定义 runtime root 目录布局契约，使各平台 UI 团队理解哪些目录由 core 保留、哪些目录可由 UI 消费

## 6. Toolchain Locator、Vault、Logging、Output Decoding

- [x] 6.1 把当前 toolchain 定位逻辑收束到 `ToolchainLocator`，统一处理 target override、global override、PATH、built-in fallback
- [x] 6.2 补齐 built-in toolchain 在不同宿主平台上的扩展名、资源路径与分发形态处理
- [x] 6.3 确保 diagnostics、one-shot exec、interactive shell 与 inspect_basic 共享同一份 toolchain 解析结果
- [x] 6.4 将 `os-native` vault backend 从占位实现升级为真实平台绑定，或补充明确的 `unsupported` / `degraded` 语义
- [x] 6.5 定义统一 `RuntimeLogger`，接管 `core.log_level`、startup/shutdown 日志、MCP trace 和关键运行诊断
- [x] 6.6 迁移散落在 runtime 与 MCP 中的 `println!` / `eprintln!` / env trace 逻辑到统一 logger
- [x] 6.7 定义统一 `OutputDecoder`，覆盖平台默认编码、UTF-8 fallback、换行归一化与 artifact 文本采集策略
- [x] 6.8 为 decode 失败或降级场景补充可观测 diagnostics，避免“输出可跑但文本损坏”
- [x] 6.9 为 toolchain / vault / logger / decoder 明确对外暴露的 health 状态，便于 UI 和 MCP 在启动时快速展示平台 readiness
- [x] 6.10 定义 logger 的最小事件分类，区分 startup、transport、terminal、toolchain、vault、decode 与 policy 诊断
- [x] 6.11 将 raw bytes canonical artifact 记录为后续增强目标，并评估未来引入时对 artifact 模型和接口的影响

## 7. 测试与自动化

- [x] 7.1 新建 core cross-platform contract 文档，覆盖 shell、IPC、paths、toolchain、vault、logger、decoder、自检
- [x] 7.2 为 `HostPlatformAdapter` 及其子能力补充单元测试，覆盖 Unix / Windows 分支选择与 degraded mode
- [x] 7.3 为 `TargetShellDialect` 补充单元测试，覆盖 SSH / ADB 当前方言与未来扩展位
- [x] 7.4 将现有 `integration_workflows` 中的 Unix-only fixture 与路径假设抽离成平台 fixture 或显式平台限定测试
- [x] 7.5 为 one-shot exec、interactive shell、control-plane transport、runtime path、toolchain fallback 建立平台矩阵集成验证
- [x] 7.6 将 interactive shell 测试从固定 sleep 断言改为 marker / poll / readiness 驱动
- [x] 7.7 扩展 `bridgingio-core --self-test`，使其覆盖新的 core platform contract 关键场景
- [x] 7.8 为 CI 与手工验证流程增加平台执行记录格式，明确 macOS、Windows、Linux 各自运行的 contract suite 和结果
- [x] 7.9 增加 flaky-test 观察项，明确 interactive shell 与 transport 相关测试在矩阵里需要记录重试 / 抖动信息
- [x] 7.10 为平台契约测试定义最小本地手工验证脚本，确保 CI 之外也能快速复现平台行为

## 8. 文档、交付与后续平台对接

- [x] 8.1 更新 `docs/testing/TESTING.md`，补充 core 平台契约测试与运行约定
- [x] 8.2 更新开发文档，说明新的 adapter 边界、structured invocation 真相源与 target dialect 模型
- [x] 8.3 为未来 Linux / Windows / OpenHarmony UI 团队编写 runtime handoff，明确哪些平台能力由 core 保障，哪些由 UI 负责
- [x] 8.4 定义本次 change 的 archive 验收标准，要求 adapter 骨架、关键迁移、platform contract、自检与样例配置全部收口
- [x] 8.5 在进入实现前，将工作流 A/B/C 映射到具体代码负责面和评审边界，明确哪些文件集可并行修改、哪些必须串行合入
- [x] 8.6 为每个工作流定义独立 handoff 清单，包括设计前提、受影响模块、测试要求与回滚边界
