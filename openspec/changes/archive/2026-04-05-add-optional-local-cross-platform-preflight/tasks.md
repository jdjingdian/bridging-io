## 1. Runner And Local Config Foundation

- [x] 1.1 在 `scripts/testing` 下新增本地 preflight 入口，并支持读取 gitignored TOML 配置中的 Linux / Windows 宿主定义
- [x] 1.2 增加 tracked 的 example 配置文件与默认本地配置约定，确保真实宿主信息继续留在 gitignored 路径而不是进入版本库
- [x] 1.3 为本地 preflight 入口增加“未配置时受控跳过”的结果语义，避免把未启用状态误报为失败

## 2. Workspace Packaging And Remote Transfer

- [x] 2.1 实现当前工作树快照打包逻辑，排除 `.git/`、`target/`、本地运行日志等明显临时目录
- [x] 2.2 实现通过 SSH / SCP 将工作树快照发送到远端宿主，并在远端 scratch 目录完成解包，而不要求远端预先 clone 仓库
- [x] 2.3 为 Windows 与 Unix 宿主分别补齐解包与远端工作目录准备逻辑，确保各自平台的默认工具链都可用

## 3. Remote Execution Modes

- [x] 3.1 实现默认 `compile-only` 模式，并在 Linux / Windows 宿主上使用 `cargo test --workspace --no-run` 或等价 compile-first 命令
- [x] 3.2 实现 Windows 默认 PowerShell / cmd 执行路径，确保未安装 Git Bash 的宿主也能参与 preflight
- [x] 3.3 为 Windows 增加 Git Bash 检测与显式启用后的附加检查流程，确保 bash 相关测试是可选附加而不是强制前提
- [x] 3.4 预留更深模式（如 `unit` / `extended`）的命令选择与调度结构，但不改变默认 compile-first 基线

## 4. Local Logs And Developer Feedback

- [x] 4.1 在 gitignored 的本地结果目录中持久化每次 preflight 的宿主、模式、命令与 pass / fail / skip 摘要
- [x] 4.2 为远端失败输出提供简洁的本地回看入口，帮助开发者快速定位编译、链接或环境依赖问题

## 5. Documentation And Boundary Setting

- [x] 5.1 在 `docs/testing` 下新增本地跨平台 preflight 说明文档，并补充用途、远端宿主前置条件、配置字段、example 文件位置与使用方式
- [x] 5.2 明确记录该能力与正式 GitHub Actions gate、`bridgingio-core --self-test`、contract matrix 的边界，说明其定位为推荐但不强制的研发预检工具
- [x] 5.3 验证“无配置跳过、Linux 远端 compile-only、Windows 远端 compile-only、Windows Git Bash 可选附加检查”这几条关键路径与提案/设计/spec 保持一致
- [x] 5.4 记录并验证 compile-only 语义：当 `cargo build --workspace` 通过但 `cargo test --workspace --no-run` 在测试目标编译阶段失败时，preflight 必须按失败处理并输出可回看摘要
