## 新增需求

<!-- 无 -->

## 修改需求

### 需求:core 必须具备跨平台契约测试基线
BridgingIO 的 Rust core 必须建立独立于 UI 的 cross-platform contract test 基线，用于验证宿主平台相关的关键能力。该契约至少必须覆盖 one-shot exec、interactive shell 生命周期、cwd/env 语义、local control-plane transport、runtime paths、toolchain fallback、vault backend、output decoding 与 self-test。`bridgingio-core --self-test` 作为 contract-critical smoke 入口时，必须额外覆盖默认 MCP model-plane 监听地址 `127.0.0.1:19718` 的绑定诊断，并在失败时输出可用于排障的原始错误信息。

#### 场景:验证 Windows 宿主平台契约
- **当** 团队在 Windows 宿主上运行 core 平台契约测试
- **那么** 测试必须能够验证 terminal provider、local transport、runtime path 与 artifact 文本采集等核心行为；若 `--self-test` 中默认 MCP 监听地址绑定失败，还必须输出具体绑定错误，而不是只给出笼统失败摘要

#### 场景:验证 Unix 宿主平台契约
- **当** 团队在 macOS 或 Linux 宿主上运行 core 平台契约测试
- **那么** 测试必须确认新的平台抽象没有破坏既有 Unix 行为，并记录该平台实际运行的 contract suite 与结果

#### 场景:执行 self-test smoke 入口
- **当** 团队执行 `bridgingio-core --self-test` 作为 core contract smoke
- **那么** 自检必须显式报告默认 MCP 监听地址 `127.0.0.1:19718` 的绑定检查结果，并在失败输出中保留底层错误文本

## 移除需求

<!-- 无 -->
