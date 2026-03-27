## 为什么

当前仓库已经有一组针对 Windows 运行时兼容性的 bugfix 代码，但这些行为还没有被 OpenSpec 规格和任务清单正式描述。缺少规格约束会导致后续回归修复、测试补齐和跨平台演进缺乏统一真相源。

## 变更内容

- 补充 Windows 平台运行相关的规范增量，明确终端执行与交互式 shell 在不同操作系统上的行为契约。
- 补充 MCP 侧与 shell 参数处理相关的规范增量，避免继续隐含 POSIX-only 的引用与命令启动假设。
- 产出与该 bugfix 对应的设计说明与可执行任务清单，用于后续验证与收口。
- 本次变更仅新增和完善 OpenSpec 文档（proposal/design/specs/tasks），不引入新的产品功能代码改动。

## 功能 (Capabilities)

### 新增功能

### 修改功能
- `target-session-management`: 明确本地终端 provider 的 one-shot exec 与 interactive shell 必须按宿主平台选择可执行 shell，并保持 cwd/env 语义可用。
- `capability-aware-mcp`: 明确 MCP 对终端相关 typed tools 的跨平台可用性要求，禁止将 POSIX shell 引号规则硬编码为唯一行为。

## 影响

- OpenSpec 变更目录：新增 proposal/design/tasks 与 2 个 capability 的增量 spec
- Rust 核心实现关联面（仅作为规格映射，不在本提案内新增代码）：`source/rust/bridgingio-providers`、`source/rust/bridgingio-mcp`
- 测试与验收：需要补齐 Windows 平台下 one-shot exec、interactive shell、cwd/env 保持、引号处理的回归验证任务

## 后续演进（不在本次交付）

- 可选将 Windows 默认 shell 从 `cmd` 扩展为可配置策略（如 `cmd` / `powershell`）
- 可选补充 Windows interactive shell 的编码与 prompt 规范化策略
