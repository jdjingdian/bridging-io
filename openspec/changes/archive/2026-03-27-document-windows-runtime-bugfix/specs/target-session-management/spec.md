## ADDED Requirements

### 需求:终端执行必须按宿主平台选择可用 shell
对于本地终端 provider，系统必须按宿主平台选择可用 shell 启动命令执行流程，禁止在 Windows 上硬编码依赖 `/bin/sh`。当平台为 Windows 时，系统必须使用 `cmd` 语义；当平台为非 Windows 时，系统必须保持现有 POSIX shell 语义。

#### 场景:Windows 平台执行 one-shot 命令
- **当** 运行环境为 Windows，且用户或 MCP 客户端触发 one-shot exec
- **那么** 系统必须通过 `cmd` 路径执行命令，而不是尝试启动 `/bin/sh`

#### 场景:非 Windows 平台执行 one-shot 命令
- **当** 运行环境为 Linux 或 macOS，且用户或 MCP 客户端触发 one-shot exec
- **那么** 系统必须继续使用 POSIX shell 路径执行命令，并保持与既有行为兼容

### 需求:交互式 shell 启动必须具备跨平台兼容默认值
系统必须为 interactive shell 提供按平台兼容的默认启动方式。Windows 平台必须使用可持续交互的 `cmd` 启动参数；非 Windows 平台必须保持 `/bin/sh` 的交互行为。

#### 场景:Windows 平台打开交互式 shell
- **当** 运行环境为 Windows，且客户端请求打开 interactive shell 且未提供 launch command
- **那么** 系统必须创建可持续接收后续输入的 `cmd` 会话，并允许后续 write/read/interrupt/close 调用正常工作

#### 场景:非 Windows 平台打开交互式 shell
- **当** 运行环境为非 Windows，且客户端请求打开 interactive shell
- **那么** 系统必须保持现有 `/bin/sh` 交互模式，不得因为跨平台适配而破坏既有 shell 会话行为

### 需求:带 shell 状态执行必须保持 cwd 与 env 的平台等价语义
系统在执行依赖 shell 状态的命令时，必须在不同平台保持“可切换工作目录并注入环境变量”的等价语义。Windows 必须使用 `cd /d` 与 `set KEY=VALUE` 等等价机制；非 Windows 必须继续使用 `cd` 与 `export` 机制。

#### 场景:Windows 平台执行依赖 cwd/env 的命令
- **当** 运行环境为 Windows，且命令执行依赖已设置的工作目录或环境变量
- **那么** 系统必须在命令执行前正确应用 cwd 与 env，使后续命令可在同一执行上下文中读取这些状态

#### 场景:跨平台回归验证 cwd/env 语义
- **当** 团队在 Windows 与非 Windows 平台分别执行同一组依赖 cwd/env 的回归用例
- **那么** 两端必须都满足“目录切换成功且环境变量生效”的行为验收标准，而不是只在 POSIX 平台通过

### 需求:core 启动入口必须支持无配置的 `--self-test` 自检模式
系统必须提供 `bridgingio-core --self-test` 启动入口，用于在不提供 `--config`、`--runtime-root` 的前提下执行内建自检。自检至少必须覆盖 one-shot 执行、interactive shell 生命周期和 runtime 执行链路，并通过进程退出码反映结果。

#### 场景:操作员运行 `--self-test`
- **当** 操作员执行 `bridgingio-core --self-test`
- **那么** 系统必须自动执行内建自检并输出清晰的通过/失败摘要；全部通过时返回退出码 `0`，任一检查失败时返回非零退出码

#### 场景:`--self-test` 与配置参数混用
- **当** 操作员同时传入 `--self-test` 与 `--config`、`--runtime-root` 或 `--control-plane-socket-override`
- **那么** 系统必须拒绝该参数组合并返回明确参数错误，而不是在含糊模式下继续启动
