## 为什么

当前主开发宿主是 macOS，而跨平台 Rust 改动往往要到手动拷贝产物、切换到 Windows 或 Linux 设备后才暴露编译或环境问题。这种验证方式过于依赖手工搬运，导致跨平台问题发现得偏晚，也让本地研发内环不愿意频繁做跨端确认。

项目需要一个可选的本地 cross-platform preflight：它能够复用开发者自己维护的 Linux / Windows 虚拟机或物理机，通过 SSH 把当前工作树快照送到远端做 compile-first 验证，同时保持这套能力是“推荐但不强制”的本地研发工具，而不是正式 CI gate。

## 变更内容

- 新增一个面向开发者的本地跨平台 preflight 入口，用于从本机触发 Linux / Windows 远端验证。
- 为该入口定义一套 gitignored 的本地配置约定，并提供仓库内可跟踪的示例配置文件；真实 IP、端口、用户名等机器信息不得进入版本库。
- 规定 preflight 必须基于“当前工作树快照”执行：本地打包并拷贝当前代码到远端，再在远端运行 compile-first 验证，而不是要求远端预先 clone 仓库或只验证已提交状态。
- 规定 Windows 远端默认使用 PowerShell / cmd 作为执行面，不得要求 Git Bash 才能参与；仅当检测到 Git Bash 且本地配置显式启用时，才追加 bash 相关测试。
- 规定默认模式优先检查跨平台编译问题，并允许后续扩展到更深的单元测试或扩展预检模式。
- 记录该能力与正式 GitHub Actions gate、`bridgingio-core --self-test` 的边界：它是开发者本地预检工具，不改变正式 contract gate。
- 在 `docs/testing` 下补充本地跨平台 preflight 的使用说明、远端宿主前置条件、配置约定与结果目录说明，帮助贡献者理解这条能力的定位和使用方式。

## 功能 (Capabilities)

### 新增功能
- `developer-local-preflight`: 提供一个可选的本地跨平台预检入口，使用 gitignored 配置驱动 Linux / Windows 远端 compile-first 验证，并把结果作为研发期辅助反馈而不是正式 gate。

### 修改功能

无。

## 影响

- 受影响代码：`scripts/testing` 下的开发者辅助脚本与示例配置文件
- 受影响文档：`docs/testing` 下的本地跨平台 preflight 说明，以及 development/testing 指南中的入口补充
- 受影响系统：开发者本地工作流、个人维护的 Linux / Windows 远端宿主、SSH / SCP 连接链路
- 明确保持不变：正式 GitHub Actions gate、`bridgingio-core --self-test` 语义、现有 contract matrix 验收边界
