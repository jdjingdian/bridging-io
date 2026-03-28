## 上下文

当前 BridgingIO 已经有一套可运行的 Rust core，但跨平台行为主要是以零散补丁方式存在：

- `bridgingio-providers` 里同时处理 one-shot exec、interactive shell、PTY/pipe fallback、cwd/env 推断和平台差异
- `bridgingio-mcp` 里既负责 tool schema，也直接拼接 SSH / ADB shell 字符串
- `bridgingio-connectors` 已经具备结构化 `CommandInvocation` 能力，但运行时没有把它提升为唯一执行真相
- runtime 入口和默认配置仍然默认 Unix 路径、Unix socket 和 Unix shell
- 测试与 fixture 大量使用 `/tmp`、`/bin/sh`、`pwd`、`printf` 等 Unix 假设

Windows 部署暴露的问题，本质上说明当前 core 的通用逻辑与平台特化逻辑耦合过深。继续追加平台分支虽然能短期修复单点问题，但会让未来 Linux / Windows / 鸿蒙 PC 的实现越来越脆弱。

这次设计的目标不是“一次性支持所有平台细节”，而是先把跨平台所需的边界立住，让后续平台实现可以在 adapter 层演进，而不是继续扩散到 domain / engine / MCP 业务层。

```text
┌──────────────────────────────────────────────────────────┐
│                 Domain / Engine / Policies              │
│ logical session | channels | artifacts | approvals      │
└─────────────────────────────┬────────────────────────────┘
                              │
               ┌──────────────▼──────────────┐
               │      HostPlatformAdapter    │
               │  shell | ipc | paths | log  │
               │  toolchain | vault | codec  │
               └──────────────┬──────────────┘
                              │
          ┌───────────────────▼───────────────────┐
          │       TargetShellDialectAdapter       │
          │ ssh-posix | adb-android | future-*    │
          └───────────────────┬───────────────────┘
                              │
        ┌─────────────────────▼─────────────────────┐
        │ Connectors / Providers / MCP / App API    │
        │ use structured invocations and contracts   │
        └────────────────────────────────────────────┘
```

## 术语与组合矩阵（design/spec 统一）

### 术语

- `host platform`：BridgingIO core 进程实际运行的宿主操作系统语义（Unix / Windows / future host）
- `target shell dialect`：目标端执行命令时遵循的远端 shell 方言语义（`ssh-posix`、`adb-android-shell`、future `ssh-windows-cmd` / `ssh-powershell`）
- `local transport`：宿主侧 UI/Core 本地 control-plane 通道语义（Unix socket / named pipe / future platform-native IPC）
- `runtime paths`：runtime root 下由 core 维护的数据目录、metadata、artifact、logs、temp、control-plane endpoint 的平台化解析结果
- `structured invocation`：由 connectors / adapter 产出的结构化命令执行模型（`program + args + diagnostics`），是 one-shot、interactive、diagnostics 的唯一执行真相源

### 宿主平台 vs 目标方言组合矩阵

| host platform | target shell dialect | 组合定位 | 当前阶段要求 |
| --- | --- | --- | --- |
| `unix host` | `ssh-posix` | 现有主路径组合 | 维持兼容并迁移到 adapter + structured invocation |
| `windows host` | `adb-android-shell` | 本次必须验证的交叉组合 | 本地进程/IPC 遵循 Windows，目标命令语义遵循 Android shell |
| `unix host` | `ssh-windows-cmd` | future 组合（接口先行） | 本次仅保留 dialect 接口与诊断位，不承诺完整行为 |

## 目标 / 非目标

**目标：**

- 为本地运行时建立清晰的宿主平台适配边界
- 为目标端 shell 行为建立独立于宿主平台的 dialect 模型
- 收束 command execution、interactive shell、IPC、paths、toolchains、vault、logging、output decoding 的跨平台真相源
- 让 MCP、provider、runtime 和测试不再各自发明平台规则
- 为未来平台 UI 实现提供稳定基石

**非目标：**

- 本次不直接交付所有未来平台 UI
- 本次不要求一次性引入 PowerShell / WSL / 所有 shell 组合的完整支持
- 本次不追求把所有运行时逻辑都抽象成过度泛化的 mega trait
- 本次不重写 domain / capability / artifact 基础模型

## 决策

### 决策 1：把宿主平台差异提升为 `HostPlatformAdapter`

系统需要引入明确的宿主平台适配边界，用来集中承载以下能力：

- `LocalShellRuntime`
- `ControlPlaneTransport`
- `RuntimePaths`
- `ToolchainLocator`
- `NativeVaultBinding`
- `RuntimeLogger`
- `OutputDecoder`

这些能力都属于“core 当前运行在哪个操作系统上”的问题，不应该散落在 provider、MCP、runtime 入口和测试 fixture 里分别处理。

这样做的价值：

- 可以把 `cfg(unix)` / `cfg(windows)` 收敛到平台实现层
- 让业务逻辑围绕能力而不是围绕编译条件工作
- 让未来 Linux / Windows / OpenHarmony 的宿主差异变成增量实现，而不是全仓散点修改

### 决策 2：把 target shell 方言与宿主平台彻底分离

系统必须显式区分两个维度：

- host platform：BridgingIO core 自己运行在哪个操作系统上
- target shell dialect：SSH / ADB / future target 远端真正使用什么 shell 语义

典型例子：

- Windows 宿主运行 `adb shell` 时，本地进程启动与 IPC 是 Windows 语义，但目标端命令仍是 Android shell / POSIX 风格
- 将来通过 SSH 连接 Windows 目标时，本地宿主可能是 macOS，但目标端方言可能是 `cmd` 或 PowerShell

如果继续把这两个维度混在一起，平台修复只会不断制造新的组合回归。

### 决策 3：结构化 invocation 成为命令执行唯一真相源

当前 `bridgingio-connectors` 已经有 `CommandInvocation { program, args }`，但运行时和 MCP 仍然广泛使用 shell 字符串拼接。后续应采用以下原则：

- connectors 负责产出结构化 one-shot / interactive invocation
- providers 负责执行 invocation，而不是重新拼接 connector 命令
- MCP 负责透传和选择，不再自己决定 SSH / ADB 字符串引号细节

这样做可以显著降低：

- Windows 与 Unix 的路径引号分歧
- “diagnostics 看到的是一套路径，interactive shell 实际走另一套”的漂移
- target override / global override / built-in fallback 在不同调用链中的不一致

### 决策 4：interactive shell 状态必须由 adapter 和显式模型维护，而不是主要靠命令文本猜测

当前 `TerminalProvider` 中的 shell 状态大量依赖命令字符串推断，例如：

- `cd ...`
- `pwd`
- `export ...`
- `unset ...`

这在 POSIX 最小路径下勉强成立，但一旦进入 Windows、未来 PowerShell、或更复杂的 interactive transcript，就会变得非常脆弱。

后续需要把 interactive shell 的核心状态迁移到：

- adapter 能直接维护的本地 shell 状态
- 或至少显式的状态变更规则，而不是依赖少数指令文本匹配

同时，interrupt、PTY/pipe fallback、prompt 采集和 command completion marker 也应统一纳入 shell runtime adapter。

### 决策 5：本地 control-plane transport 必须提供平台原生实现

目前 control-plane IPC 语义基本等价于 Unix socket，这在 macOS / Linux 上成立，但对 Windows 并不构成完整实现。后续需要把 local transport 抽象成统一接口，并允许：

- Unix: Unix domain socket
- Windows: named pipe 或等价本地 IPC
- 其他平台: 平台原生本地 IPC 或显式降级语义

UI-managed / standalone / detached 模式都应依赖同一 transport 抽象，而不是在 runtime 入口里用 `cfg(unix)` 决定功能是否存在。

### 决策 6：runtime path、默认配置样例和生成配置必须 host-aware

当前配置与样例中仍然存在大量 Unix 默认值，例如：

- `~/.bridgingio`
- `/bin/sh`
- `control-plane.sock`

后续需要明确：

- 配置 schema 可以保留逻辑字段，但默认值必须由 `RuntimePathsAdapter` 在宿主平台上解析
- bundled / UI-managed 自动生成的配置必须使用宿主平台可工作的默认 endpoint 和目录布局
- 示例配置要么改为平台中性的占位语义，要么按平台提供独立样例

### 决策 7：统一 logger 与输出解码属于跨平台基线，而不是附属改进

`log_level` 已经是 core-owned setting，但当前日志仍分散在 `println!` / `eprintln!` / trace env 分支中。与此同时，artifact 文本捕获普遍默认 UTF-8 lossy 解码，这在 Windows 非 UTF-8 终端环境下不够稳。

因此需要：

- 建立统一 `RuntimeLogger`，让 `log_level`、MCP trace、startup/shutdown 日志来自同一套运行时模型
- 建立 `OutputDecoder`，保证不同宿主平台的文本输出、换行与编码被一致处理
- 对无法可靠解码的情况给出明确诊断，而不是静默“能跑但字坏了”

### 决策 8：测试体系采用 core cross-platform contract，而不是继续默许 Unix 为默认真相

需要新增一份 `core platform contract`，覆盖至少这些流程：

- one-shot exec
- interactive shell open/write/read/interrupt/close
- cwd/env 行为
- runtime paths
- control-plane local transport
- toolchain override / PATH / built-in fallback
- vault backend behavior
- output decoding / newline normalization
- self-test coverage

自动化上采用两条线：

- adapter / dialect 级单元测试
- 平台矩阵集成测试与自检

并且要显式去掉当前依赖固定 sleep 窗口的交互式 shell 断言，改为 marker / poll / readiness 驱动的测试方式。

### 决策 9：Windows local control-plane 的目标架构采用 named pipe，但当前阶段不立即实现

Windows 平台上的本地 control-plane transport 目标架构确定为 `named pipe`，而不是 loopback TCP。原因是：

- `named pipe` 更符合 `platform-ipc` 的本地语义
- 可以更清晰地区分本地 control-plane 与面向 AI 的 model-plane HTTP
- 长期上更适合作为 bundled UI 与 core 的受信任本地通道

但当前项目阶段只有 macOS UI 在推进，因此本次变更不要求立即交付 Windows named pipe 的完整实现。当前阶段只需要：

- 在设计与接口层为 Windows transport 预留 named pipe 语义
- 避免继续把 Unix socket 当成唯一的 local transport 模型
- 为 Windows 平台未实现 local transport 的情况提供明确的 `unsupported` / `deferred` diagnostics

### 决策 10：Windows host shell 通过 adapter 抽象管理，首版默认 `cmd`

Windows 宿主 shell 不应直接在业务逻辑中硬编码为单一路径，而应通过 host shell adapter 进行选择。首版默认支持 `cmd`，后续再增量引入 PowerShell / `pwsh`。

选择该策略的原因：

- `cmd` 与当前已经落地的 Windows bugfix 路线一致，迁移成本最低
- 通过 adapter 抽象预留 PowerShell 能力，比立即把 PowerShell 设为默认更稳
- 可以避免把未来 shell 扩展需求再次散落到 provider / MCP 逻辑中

因此本次变更的目标是：

- 把 Windows host shell 选择权提升到 adapter 层
- 首版 baseline 明确 `cmd` 为默认实现
- PowerShell 作为后续扩展能力进入后续提案或增量实现

### 决策 11：文本输出采用“平台默认编码 + 诊断”，并把 raw bytes 作为后续目标

本次跨平台基线阶段，artifact 与 transcript 的文本输出策略采用：

- 优先按宿主平台默认编码进行解码
- 对解码降级、替换字符或潜在损坏提供明确 diagnostics
- 保持换行归一化和文本可读性的一致处理

同时，将 “raw bytes 作为 artifact 真相源，再派生文本视图” 明确为后续增强目标，而不是本次基线重构的首轮交付范围。

选择该策略的原因：

- 它比“强制 UTF-8”更符合当前多平台现实
- 它能在不大幅扩大 artifact 模型改动面的前提下提高 Windows 可用性
- 它为后续需要更强保真度时升级到 raw-bytes canonical 模型保留空间

raw-bytes canonical 后续引入影响评估：

- `artifact` 存储层需要保留 canonical raw bytes，并把当前文本 chunk 视为派生视图
- `artifacts.read` / `artifacts.refine` 等接口需要增加 raw/text 读取模式及对应 metadata
- digest / dedup / 摘要生成需要明确“以 raw bytes 为真相，以文本为展示”的边界，避免跨编码摘要漂移

### 决策 12：structured invocation 是唯一执行真相源；若实现面过大，则拆出独立后续提案

命令执行的长期架构明确为：structured invocation 是 one-shot exec、interactive shell、diagnostics 与 inspect_basic 的唯一执行真相源。

这意味着：

- connector 负责产出结构化 invocation
- provider 负责执行 invocation
- MCP 不再维护独立的 SSH / ADB shell 字符串真相

但考虑到当前仓库中 `bridgingio-mcp`、`bridgingio-providers` 与 `bridgingio-connectors` 的现状，如果在落地时发现该改动面明显超出本次 change 的可控范围，则应拆出独立后续提案处理实现迁移，而不改变本设计决策本身。

也就是说：

- 设计方向已确定，不再把“继续混用字符串拼接”视为可接受长期方案
- 实现节奏可以视改动面决定是否拆分到独立 proposal / change

## 风险 / 权衡

- 这是一次“收束边界”的重构，短期内会涉及多个 crate，节奏上比局部 bugfix 慢
- 如果 adapter 设计得过于抽象，可能会带来实现负担；因此应优先围绕现有 seam 收敛，而不是从零设计万能框架
- control-plane transport 的平台实现差异可能会影响 bundled UI 启动链路，需要分阶段迁移
- 测试体系去 Unix 偏置后，可能会一次暴露出更多历史问题，但这是健康信号

## 工作流拆分（轻量闸门）

### 工作流 A：`host-platform-foundation`

目标是先建立宿主平台基础边界与 baseline，实现范围限定为：

- `HostPlatformAdapter` 顶层装配与能力暴露
- `ControlPlaneTransport` 抽象与 Unix baseline（Windows 语义在接口层预留）
- `RuntimePaths` 与生成配置默认值按宿主平台解析
- `RuntimeLogger` 统一日志入口与关键事件分类基础
- `OutputDecoder` 平台兼容解码、换行归一化与降级诊断基础

### 工作流 B：`terminal-runtime-and-dialects`

在工作流 A 的基础上收束终端执行主链路，实现范围限定为：

- `LocalShellRuntime`（one-shot + interactive）生命周期统一
- interactive shell 的 `cwd/env/prompt` 状态模型显式化
- structured invocation 作为 one-shot/interactive/diagnostics 的执行真相源
- `TargetShellDialect` 首版覆盖 `ssh-posix` 与 `adb-android-shell`
- MCP 终端执行链路改为消费 structured invocation，收束重复字符串拼接逻辑

### 工作流 C：`platform-contract-and-parity`

负责平台契约收口和一致性验证，实现范围限定为：

- `ToolchainLocator` 统一 target/global/PATH/built-in fallback 的定位与诊断
- `NativeVaultBinding` 从占位后端演进到真实绑定或显式 `unsupported/degraded`
- cross-platform contract、自检扩展、平台矩阵验证
- 开发/测试/handoff 文档收口

### 工作流依赖与并行规则

- A 必须先于 B：B 不得在 A 的 adapter 边界未稳定时继续扩散平台分支
- A 与 C 可部分并行：C 可先建设 contract 与测试框架，但最终验收依赖 A 的基础能力稳定
- B 的执行链路收口先于 C 的最终平台契约验收：C 只在 structured invocation 主链路一致后做最终平台对齐结论

### 各工作流完成标准

- A 完成标准：
  - `HostPlatformAdapter`、`ControlPlaneTransport`、`RuntimePaths`、`RuntimeLogger`、`OutputDecoder` 边界稳定
  - Unix baseline 保持兼容，Windows baseline 给出可消费诊断（允许 `degraded/unsupported`）
  - 无新的跨 crate 循环依赖
- B 完成标准：
  - `LocalShellRuntime` 收束 one-shot + interactive 生命周期
  - MCP/provider 不再维护独立 shell 字符串真相
  - `TargetShellDialect` 对 `ssh-posix` 与 `adb-android-shell` 行为可测且可诊断
- C 完成标准：
  - core platform contract 与自检覆盖关键路径
  - 平台矩阵结果可复现且有记录格式
  - 文档明确 core 保证面、UI 责任面和回滚边界

### 已固化关键决策（不回退）

- Windows local transport 目标架构固定为 named pipe（非 loopback TCP）
- Windows host shell 首版默认 `cmd`
- 文本输出首版采用“平台默认编码 + 解码诊断”
- structured invocation 是长期唯一执行真相源

## 代码入口盘点（任务 1.3）

| crate | 当前平台相关入口 | 盘点结论 |
| --- | --- | --- |
| `bridgingio-providers` | `source/rust/bridgingio-providers/src/lib.rs` | interactive runtime 同时承载 PTY/pipe、interrupt、marker、`cwd/env` 命令文本推断，是宿主平台差异与 shell 状态耦合最重入口 |
| `bridgingio-mcp` | `source/rust/bridgingio-mcp/src/lib.rs` | 仍有 SSH/ADB connector command 字符串拼接、平台 quoting 分支、本地 IPC Unix-only 实现入口 |
| `bridgingio-connectors` | `source/rust/bridgingio-connectors/src/lib.rs` | 已具备结构化 invocation 与 toolchain 解析能力，但仍混有 POSIX 取证命令假设（`uname`、`echo $SHELL` 等） |
| `bridgingio-engine` | `source/rust/bridgingio-engine/src/lib.rs` + `tests/fixtures/*.toml` | 示例配置直接绑定 Unix 风格路径/工具路径，target terminal shell 字段常以 Unix shell 书写 |
| `bridgingio-secrets` | `source/rust/bridgingio-secrets/src/lib.rs` | `os-native` 当前仍以内存后端占位实现，尚未接入真实平台安全存储绑定 |

## Unix-only 假设迁移清单（任务 1.4）

| 类别 | 位置 | 当前假设 | 迁移方向 |
| --- | --- | --- | --- |
| 样例配置 fixture | `source/rust/bridgingio-engine/tests/fixtures/standalone-minimal.toml` | `~/.bridgingio`、`/bin/sh` | 保留 schema，默认值改为 host-aware 解析或平台中性占位 |
| 样例配置 fixture | `source/rust/bridgingio-engine/tests/fixtures/standalone-complete.toml` | `/bin/bash`、`/opt/homebrew/bin/adb`、`/Applications/.../adb` | 将工具链与 shell 示例拆为平台中性写法或分平台样例 |
| 自生成配置 | `source/rust/bridgingio-mcp/src/bin/bridgingio-core.rs` | usage/tests 大量 `/tmp` 与 `.sock` 语义 | 生成逻辑保持 adapter 驱动，测试样例迁移到 `temp_dir()` / runtime path 抽象 |
| MCP 单元测试 | `source/rust/bridgingio-mcp/src/lib.rs` | `/tmp`、`control-plane.sock`、`#[cfg(unix)]` IPC 路径 | 抽离平台 fixture，Unix-only case 显式标注 |
| 集成测试 | `source/rust/bridgingio-mcp/tests/integration_workflows.rs` | `#!/bin/sh`、`/bin/sh -lc`、`cd /tmp`、`pwd`、`printf` | 把通用 contract 改为平台命令适配层驱动，Unix-only 场景单独保留 |
| provider 测试与状态模型 | `source/rust/bridgingio-providers/src/lib.rs` | 基于 `pwd/cd/export/unset` 文本匹配维护状态 | 迁移到 `LocalShellRuntime` 显式状态与 marker/poll 协议 |

## 本次重构完成标准（任务 1.5）

### 先立边界（本次必须完成）

- 术语、矩阵、职责边界与关键决策在 design/spec 可追踪
- A/B/C 工作流边界、依赖与各自验收标准明确
- 平台相关入口与 Unix-only 假设形成迁移清单
- structured invocation、host adapter、target dialect 的责任边界固定

### 后续平台增量实现（本次不阻塞）

- Windows named pipe server/client 的完整工程化落地
- `os-native` vault 真实平台绑定
- raw-bytes canonical artifact 模型
- future dialect（`ssh-windows-cmd` / `ssh-powershell`）完整行为

## Future Dialect 范围控制（任务 1.6）

本次不进入完整实现的 future dialect 范围：

- `ssh-windows-cmd`
- `ssh-powershell`
- 其他 future target shell 方言

本次仅要求：

- 在 `TargetShellDialect` 保留可扩展接口与默认语义
- 在 diagnostics 中明确 “接口已预留 / 行为未承诺” 状态
- 不将 future dialect 未完成阻塞 A/B/C 当前交付验收

## 规格层决策锚点与 open questions 收敛（任务 1.7）

- 已拍板决策已在本 design 与 specs 中同步锚定：named pipe、Windows 默认 `cmd`、平台默认编码+诊断、structured invocation 唯一真相源
- 当前仅保留 3 个 open questions（见下文），禁止新增无边界开放项；若超出范围，必须通过新提案承接

## Migration Plan

### 阶段 1：建立边界与契约

- 新增 OpenSpec 要求、术语和测试契约
- 定义 `HostPlatformAdapter`、`TargetShellDialect`、structured invocation 责任边界
- 盘点当前 Unix-only 假设并形成迁移清单

### 阶段 2：优先迁移最脆弱的运行时路径

- 先迁移 local shell runtime、interactive shell 生命周期、runtime path、control-plane transport
- 收束 MCP 中的 connector command 拼接逻辑
- 补齐 output decoding 与 logger

### 阶段 3：补平台实现与矩阵验证

- 实现 Unix baseline adapter
- 实现 Windows baseline adapter
- 跑通 self-test、integration 和 core platform contract

### 阶段 4：为未来平台 UI 提供稳定基线

- 更新文档、样例配置和 handoff 约束
- 为 Linux / Windows / OpenHarmony UI 团队提供可复用的 runtime 约定

## Open Questions

- Windows named pipe transport 的具体生命周期与 endpoint 命名约定应如何与 macOS / Linux 的 control-plane endpoint 语义对齐？
- PowerShell 作为后续 Windows host shell 适配器接入时，最小兼容语义是否需要与 `cmd` 完全等价，还是允许能力分层？
- raw bytes canonical artifact 模型在未来引入时，是否需要单独扩展 artifact metadata 与 read/refine 接口？
