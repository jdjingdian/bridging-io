## ADDED Requirements

### 需求:core 必须具备跨平台契约测试基线
BridgingIO 的 Rust core 必须建立独立于 UI 的 cross-platform contract test 基线，用于验证宿主平台相关的关键能力。该契约至少必须覆盖 one-shot exec、interactive shell 生命周期、cwd/env 语义、local control-plane transport、runtime paths、toolchain fallback、vault backend、output decoding 与 self-test。

#### 场景:验证 Windows 宿主平台契约
- **当** 团队在 Windows 宿主上运行 core 平台契约测试
- **那么** 测试必须能够验证 terminal provider、local transport、runtime path 与 artifact 文本采集等核心行为，而不是只依赖 `--self-test` 的少量 smoke 用例

#### 场景:验证 Unix 宿主平台契约
- **当** 团队在 macOS 或 Linux 宿主上运行 core 平台契约测试
- **那么** 测试必须确认新的平台抽象没有破坏既有 Unix 行为，并记录该平台实际运行的 contract suite 与结果

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

#### 场景:验证 shell 中的 `cd` 后续 `pwd`
- **当** 测试需要验证 interactive shell 在执行 `cd` 后的后续 `pwd` 结果
- **那么** 测试必须通过明确的完成标记或状态轮询确认命令执行完成，而不是依赖固定的 `sleep` 时长推测输出已经稳定

#### 场景:验证长时间运行命令的 interrupt
- **当** 测试需要验证 interactive shell 对长时间运行命令的 interrupt 行为
- **那么** 测试必须通过明确的 running / interrupted 状态与 transcript 事件进行断言，而不是依赖脆弱的时间竞态判断
