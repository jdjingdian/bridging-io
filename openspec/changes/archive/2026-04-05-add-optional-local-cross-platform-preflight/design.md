## 上下文

当前仓库已经具备正式的 testing baseline、core platform contract script 与 `bridgingio-core --self-test` 入口，但它们主要服务于正式验证、回归和 release confidence，而不是服务于个人开发者的“高频跨端预检”内环。

对当前维护者而言，主要开发宿主是 macOS，而本轮讨论中卡住的现实问题是：Windows / Linux 真相通常只能在手动拷贝产物后才能确认。虽然未来 GitHub Actions 可以承担正式 gate，但那条链路更适合作为外环门控，而不是替代本地快速发现编译问题的内环工具。

这次设计要解决的是一个可选的 developer workflow capability，而不是新的产品运行时语义。它需要同时满足以下约束：

- 默认不强制，不影响没有远端宿主的贡献者
- 本地真实配置必须与仓库跟踪内容隔离
- 远端宿主只要求具备 SSH 与 Rust 基础环境；Windows 不要求 Bash
- 预检必须基于当前工作树快照，而不是只基于已提交代码
- 该能力不得篡改正式 GitHub Actions gate 或 `bridgingio-core --self-test` 的 contract

## 目标 / 非目标

**目标：**

- 提供一个仓库内可发现、但默认可选的本地 cross-platform preflight 入口
- 允许开发者使用 gitignored TOML 配置声明 Linux / Windows 远端宿主
- 让 preflight 基于当前工作树快照，将代码打包后通过 SSH / SCP 发送到远端执行验证
- 让默认验证模式优先暴露编译 / 链接问题，并可逐步扩展到更深的测试层
- 让 Windows 远端默认使用 PowerShell / cmd 跑通 compile-first 预检，不把 Git Bash 作为前置条件
- 在本地保留每次预检的日志、摘要与失败点，方便研发阶段快速定位

**非目标：**

- 不把该能力设计成正式 CI / CD gate，也不要求未来 GitHub Actions 复用完全相同的内部实现
- 不修改 `bridgingio-core --self-test` 的参数语义或把远端宿主配置塞进 self-test
- 不要求远端宿主维护仓库 clone、git 分支或与本地完全一致的开发工作流
- 不试图在首轮把所有平台依赖、安装器、镜像管理或自动 provision 一并做完

## 决策

### 决策 1：把本地跨端预检定义为独立的 developer preflight runner，而不是扩展 `bridgingio-core --self-test`

preflight 的入口应当位于 `scripts/testing` 或等价的开发者工具层，由它读取本地配置、打包工作树、分发到远端并执行命令；它不直接改变 `bridgingio-core --self-test` 的参数面。

这样做的原因：

- `--self-test` 当前已经有明确的 contract 边界，并且显式拒绝 `--config` / `--runtime-root` 等覆盖参数
- 本地 preflight 是开发者 workflow，不是产品运行时入口
- 把远端宿主、上传、日志回收这些问题放在外层 runner，更容易保持正式 contract 的纯度

考虑过的替代方案：

- **扩展 `bridgingio-core --self-test` 接受远端配置**：会把开发者本地工具需求混入正式 contract 入口，扩大 CLI 语义面。
- **完全仓库外自维护脚本**：侵入最低，但 discoverability 较差，也不利于团队后续复用同一套约定。

### 决策 2：使用“仓库内 example 文件 + gitignored 本地配置”的双层配置模式

仓库内只跟踪示例配置文件与文档，真实远端信息保存在 gitignored 路径，例如 `tmp/` 下的本地 TOML 文件。runner 默认读取约定路径，也允许显式传入配置文件。

这样做的原因：

- 保持真实 IP、端口、用户名等环境细节不进入版本库
- 同时给未来参与者一个可发现的最小配置样例，例如 `scripts/testing/local-preflight.example.toml`
- 与项目现有 TOML 配置习惯保持一致

考虑过的替代方案：

- **直接把真实配置写进仓库**：会泄露个人环境细节，也会制造无关 diff。
- **只靠环境变量配置**：短期可行，但可读性和可维护性较弱，不利于多宿主配置。

### 决策 3：runner 本体首选 Python，实现本地编排；远端只要求 SSH 和 Rust 基础环境

本地 runner 选择 Python 实现，优先使用标准库处理 TOML、打包、进程执行与日志组织。远端宿主不要求安装 Python，只接收打包后的工作树并执行 shell / PowerShell 命令。

这样做的原因：

- Python 在当前开发机上已现成可用，适合做跨平台 orchestration
- 相比纯 shell，更容易统一处理 Windows PowerShell / cmd 和 Unix shell 的转义与流程控制
- 相比 Rust `xtask`，首轮落地更轻，适合先把工具跑起来

考虑过的替代方案：

- **纯 shell runner**：在 Unix 上简单，但一旦混入 Windows 命令链和引号处理，复杂度会上升。
- **Rust `xtask`**：长期更统一，但首轮实现与迭代成本更高。

### 决策 4：远端验证基于当前工作树快照执行，不依赖远端 git checkout

preflight 必须打包本地当前工作树，并排除明显的临时目录（例如 `.git/`、`target/`、本地运行日志目录），然后把快照上传到远端执行。这样未提交改动也能进入验证。

推荐的数据流：

```text
local worktree
  -> package snapshot
  -> scp to remote host
  -> extract into remote scratch dir
  -> run compile/test command
  -> stream or collect logs
  -> store local run summary
```

这样做的原因：

- 研发阶段最需要验证的是“当前手头代码”，而不是只验证远端能否 checkout 某个 commit
- 可以避免要求远端机器维护仓库 clone 与认证
- 失败时能更准确对应本地未提交改动

考虑过的替代方案：

- **远端 `git pull` / clone**：会丢失未提交改动，也给远端增加仓库状态管理负担。
- **只拷贝二进制产物**：能做运行时 smoke，但无法及早发现 Windows / Linux 编译问题。

### 决策 5：打包与远端执行按宿主平台分层；Windows 默认 `zip + PowerShell`，Unix 使用 Unix 友好的解包路径

Windows 远端默认使用 ZIP 包上传，并通过 PowerShell 解压与执行命令。Unix 远端使用 Unix 友好的归档/解包路径，以降低对 Windows 兼容性的牵连。Windows 若检测到 Git Bash 且配置显式启用，可追加 bash 相关测试，但这不是参与 preflight 的前提。

这样做的原因：

- ZIP + PowerShell 在 Windows 上更通用、可预期
- Unix 保持更自然的归档语义，避免所有平台被迫围绕 Windows 传输格式折中
- “默认 PowerShell，Git Bash 可选”与当前团队约束一致

考虑过的替代方案：

- **所有平台统一只用 TAR**：Windows 支持与行为可预期性较差，容易给首轮体验增加障碍。
- **所有平台统一只用 ZIP**：可以做，但会牺牲 Unix 侧部分自然性与元数据保真。

### 决策 6：默认运行模式是 compile-first，并显式区分 `compile-only` 与更深模式

首轮默认模式应以“尽早发现跨平台编译 / 链接问题”为目标，因此默认命令选用 `cargo test --workspace --no-run` 或等价 compile-first 路径。后续可扩展到 `unit`、`extended` 等更深模式，但它们不应成为默认负担。

这样做的原因：

- 当前最痛的反馈点是“代码在 Windows / Linux 上到底能不能编译”
- `compile-only` 能覆盖测试代码的编译与链接，又比完整执行更适合开发内环
- 更深模式可以保留给阶段性检查，而不是每次都压在开发者身上

考虑过的替代方案：

- **默认完整 `cargo test`**：反馈更全，但会让内环变慢，降低使用频率。
- **默认只 `cargo check`**：更快，但对真实链接与测试二进制产出覆盖不足。

### 决策 7：本地结果必须落到忽略目录，并明确标记为开发者辅助记录

每次 preflight 的输出都应保留在本地忽略目录，包括运行摘要、远端命令、关键日志与 pass/fail/skip 状态。结果格式应服务于快速定位问题，而不是承担正式 gate record 的职责。

这样做的原因：

- 研发阶段最重要的是快速回看失败点
- 与正式 CI record 分离，可以避免“本地辅助工具”被误认成正式质量真相

### 决策 8：在 `docs/testing` 下提供独立说明文档，而不是把本地 preflight 混进正式 contract 章节

本地 cross-platform preflight 需要在 `docs/testing` 下拥有独立说明文档，明确它的目标、前置条件、配置模板、典型命令与结果目录语义。该文档可以从现有 testing / development 指南引用，但不应与正式 contract gate 描述混为一体。

这样做的原因：

- 本地 preflight 是开发者辅助能力，适合在 testing 文档下被发现
- 独立文档可以清晰区分“推荐的本地预检”与“正式 gate / contract run”
- 贡献者可以先看 example 配置和宿主前置条件，再决定是否启用

考虑过的替代方案：

- **只在 `docs/DEVELOPMENT.md` 中顺手提一句**：可发现性不足，也不利于后续扩展配置说明。
- **把本地 preflight 直接写进正式 contract 文档**：容易让读者误解它属于必须通过的门控项。

## 风险 / 权衡

- **远端工具链漂移** → 在文档中明确“远端至少需要 SSH + Rust + 对应平台构建依赖”，并在 preflight 输出中先打印宿主/toolchain 摘要
- **工作树打包体积较大** → 默认排除 `.git/`、`target/`、本地日志与临时目录，必要时后续再引入增量同步
- **Windows 构建链路可能还需要 MSVC / SDK 等依赖** → 在文档中把这类前置依赖列为宿主准备要求，不把失败误归类为 runner 逻辑问题
- **本地脚本与未来 GitHub Actions 不完全一致** → 明确其目标是研发预检而不是正式 gate，避免用同一把尺子要求两者完全一致

## Migration Plan

1. 在仓库中增加 tracked 的 local preflight runner、example 配置文件和 `docs/testing` 说明文档
2. 为 gitignored 本地配置与结果目录建立默认约定
3. 先落地 Linux / Windows compile-first 预检
4. 再按需要扩展更深模式与 Git Bash 附加测试

回滚策略：

- 如果该能力维护成本过高，可以只回滚 runner / 文档 / 示例配置，不影响正式 contract 流程与产品运行时

## Open Questions

- gitignored 配置的默认约定路径最终放在 `tmp/` 还是仓库内更专门的本地目录
- Unix 侧首轮是否直接使用现有 contract script，还是先统一走 compile-first 命令再逐步扩展
- 每次远端 preflight 完成后，远端 scratch 目录是立即清理还是保留最近一次失败现场以便排查
