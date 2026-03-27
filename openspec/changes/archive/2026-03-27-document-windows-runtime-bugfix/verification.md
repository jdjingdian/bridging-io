## 代码到规格映射（任务 1.1）

### target-session-management 增量规格

- 需求:终端执行必须按宿主平台选择可用 shell
  - 代码位置: `source/rust/bridgingio-providers/src/lib.rs`
  - 对应实现:
    - `run_shell_command`：Windows 使用 `cmd /C`，非 Windows 使用 `/bin/sh -lc`
    - `exec_local` 与 `execute_with_shell_state` 统一复用 `run_shell_command`
- 需求:交互式 shell 启动必须具备跨平台兼容默认值
  - 代码位置: `source/rust/bridgingio-providers/src/lib.rs`
  - 对应实现:
    - `platform_interactive_shell_command(None)`：Windows 为 `cmd /Q /K`，非 Windows 为 `/bin/sh -s`
    - `spawn_interactive_pipe_process` 统一调用 `platform_interactive_shell_command`
- 需求:带 shell 状态执行必须保持 cwd 与 env 的平台等价语义
  - 代码位置: `source/rust/bridgingio-providers/src/lib.rs`
  - 对应实现:
    - `execute_with_shell_state` 中 Windows 使用 `cd /d` 与 `set KEY=VALUE`
    - 非 Windows 使用 `cd` 与 `export`

### capability-aware-mcp 增量规格

- 需求:MCP 终端相关 typed tools 必须支持 Windows 宿主运行
  - 代码位置: `source/rust/bridgingio-mcp/src/lib.rs`、`source/rust/bridgingio-providers/src/lib.rs`
  - 对应实现:
    - MCP 侧连接命令构造使用平台分流的 `shell_single_quote`
    - Provider 侧执行与交互 shell 启动不再硬编码 `/bin/sh`
- 需求:MCP 命令参数处理必须避免将 POSIX 单引号规则误用于 Windows
  - 代码位置: `source/rust/bridgingio-mcp/src/lib.rs`
  - 对应实现:
    - `shell_single_quote` 在 Windows 下返回原值（不注入 POSIX 单引号）

## 范围确认（任务 1.2）

- 本次仅完成“已有修复代码的规格化、测试补强与验收记录”。
- 未新增产品功能，未引入新的 target 类型，未扩展 PowerShell/WSL 适配。

## 验收记录（任务 4.1）

### 已执行验证（macOS）

- 平台: macOS
- 命令:
  - `cargo test -p bridgingio-providers -p bridgingio-mcp`
- 结果摘要:
  - provider 与 mcp 新增/更新测试通过
  - 非 Windows 路径（`/bin/sh`、POSIX 引号）回归通过

### Windows 验证进展

- 平台: Windows
- 执行命令:
  - `./bridgingio-core.exe --self-test`
- 用户回传输出摘要:
  - `self-test [ok] terminal provider one-shot execution`
  - `self-test [ok] interactive shell lifecycle`
  - `self-test [ok] interactive cwd/env semantics`
  - `self-test [ok] space-containing path/argument handling`
  - `self-test [ok] standalone runtime execute path`
  - `self-test passed`
- 结果判定:
  - 已覆盖并通过: one-shot exec、interactive shell 生命周期链路、`cd /d` 与 `set KEY=VALUE` 语义、含空格路径/参数引号场景（对应任务 2.1、2.2、2.3、2.4）

## Windows 快速自检入口

- 启动命令:
  - `bridgingio-core --self-test`
  - 若通过 Cargo 运行: `cargo run -p bridgingio-mcp --bin bridgingio-core -- --self-test`
- 预期输出摘要:
  - `self-test [ok] terminal provider one-shot execution`
  - `self-test [ok] interactive shell lifecycle`
  - `self-test [ok] interactive cwd/env semantics`
  - `self-test [ok] space-containing path/argument handling`
  - `self-test [ok] standalone runtime execute path`
  - `self-test passed`
- 预期退出码: `0`（任一检查失败将返回非零退出码并输出失败原因）
