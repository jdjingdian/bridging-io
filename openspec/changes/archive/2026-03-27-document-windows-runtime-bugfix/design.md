## 上下文

当前 Windows 运行时 bugfix 已在 Rust 代码中实现，核心变化集中在两处：

- `bridgingio-providers` 不再假定 `/bin/sh` 始终可用，而是按平台选择 shell（Windows 使用 `cmd`，非 Windows 继续使用 `/bin/sh`）。
- `bridgingio-mcp` 的命令引号辅助逻辑不再把 POSIX 单引号规则无条件套用到 Windows。

问题在于这些行为尚未被规格化，导致团队虽然有实现，但缺少可复用的验收标准、测试范围和后续维护基线。

## 目标 / 非目标

**目标：**

- 将已有 Windows 兼容 bugfix 对应的行为契约写入 OpenSpec。
- 明确 one-shot exec、interactive shell、cwd/env 注入和引号处理的跨平台语义。
- 输出与规格一致的任务清单，指导后续验证、回归与文档同步。

**非目标：**

- 本次不新增任何功能代码，也不重构现有 provider/mcp 架构。
- 本次不引入 PowerShell、WSL 或第三方 shell 适配策略。
- 本次不扩展到新的 target 类型，仅覆盖当前终端相关能力在 Windows 的可用性。

## 决策

### 决策 1：按宿主平台选择 shell 启动路径

- 选择：
  - Windows：one-shot 使用 `cmd /C`；interactive 使用 `cmd /Q /K`。
  - 非 Windows：保持 `/bin/sh -lc` 与 `/bin/sh -s` 语义。
- 理由：当前 bugfix 已基于该策略落地，实现成本低、行为稳定，且无需引入额外运行时依赖。
- 备选方案：
  - 统一要求 Windows 安装 POSIX shell（如 Git Bash/MSYS）后继续走 `/bin/sh`。
  - 拒绝原因：增加环境前置条件，且与“开箱可用”目标冲突。

### 决策 2：cwd/env 语义保持平台等价，而非命令文本完全一致

- 选择：
  - Windows 脚本语义使用 `cd /d` 与 `set KEY=VALUE`。
  - 非 Windows 保持 `cd` + `export`。
- 理由：平台 shell 语法天然不同，但调用方需要的是“能切目录、能设置环境变量并执行命令”的等价能力。
- 备选方案：
  - 用进程级 `current_dir` 和 `env` API 完全替代脚本注入。
  - 拒绝原因：对 interactive/历史行为兼容影响较大，且不利于与现有 transcript 语义保持一致。

### 决策 3：引号策略按平台分流，避免 POSIX 规则误用到 Windows

- 选择：
  - `bridgingio-mcp` 在 Windows 下不使用 POSIX 单引号拼接策略。
  - `bridgingio-providers` 在 Windows 路径场景使用双引号转义辅助。
- 理由：Windows `cmd` 不支持 POSIX 单引号语义，强行复用会造成命令失败或参数错位。
- 备选方案：
  - 引入统一跨 shell quoting 抽象层并一次性替换全部调用点。
  - 拒绝原因：范围过大，不符合本次“已有 bugfix 补规格”的目标。

### 决策 4：本次只补规格与任务，不追加代码提交

- 选择：将已有代码视为既有实现事实，本次仅补 proposal/design/specs/tasks 以形成正式契约。
- 理由：用户已明确本次提案目的为“补充规格和任务说明”，不重复实现。
- 备选方案：
  - 在同一变更中追加代码整理与测试补齐。
  - 拒绝原因：会扩大本次提案范围，不利于先完成规格归档。

## 风险 / 权衡

- [Windows `cmd` 与 POSIX shell 行为差异仍可能在边缘字符集上表现不一致] → 通过规格强制覆盖关键转义与路径场景测试。
- [仅补文档不补测试代码，短期内仍有回归风险] → 在 tasks 中明确把 Windows 回归测试列为后续必做项。
- [未来引入 PowerShell/WSL 时可能与当前契约冲突] → 将现有契约限定为“当前默认 shell 策略”，为后续扩展留出增量空间。

## Migration Plan

1. 合入本次 OpenSpec 文档变更，建立跨平台 shell 行为的规格基线。
2. 按 tasks 清单补充自动化与手工验证，确保已存在 bugfix 满足规格要求。
3. 若验证发现偏差，后续通过独立实现变更修正代码并回填测试。
4. 若需回滚，仅需回滚本次文档变更，不影响当前运行代码路径。

## Open Questions

- 后续是否需要把 Windows 默认 shell 从 `cmd` 升级为可配置（`cmd`/`powershell`）？
- interactive shell 在 Windows 下是否需要补充更细粒度的 prompt/编码规范？
