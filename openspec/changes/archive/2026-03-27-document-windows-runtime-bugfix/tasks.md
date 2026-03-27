## 1. 规格对齐与范围确认

- [x] 1.1 对照当前 `source/rust/bridgingio-providers` 与 `source/rust/bridgingio-mcp` 的 Windows bugfix 差异，确认其行为与本次新增规格场景逐条映射
- [x] 1.2 明确本次变更仅覆盖“已有修复的规格化与验证任务”，不引入额外功能实现范围

## 2. Windows 运行时行为验证

- [x] 2.1 在 Windows 环境验证 one-shot exec 能通过 `cmd` 路径执行，不再依赖 `/bin/sh`
- [x] 2.2 在 Windows 环境验证 interactive shell 的 open/write/read/interrupt/close 全链路可用
- [x] 2.3 在 Windows 环境验证带状态执行（cwd 与 env）语义可用，覆盖 `cd /d` 与 `set KEY=VALUE` 场景
- [x] 2.4 在 Windows 环境验证命令参数/路径包含空格时的引号处理行为，确保不再误用 POSIX 单引号规则

## 3. 非 Windows 回归与自动化测试补齐

- [x] 3.1 在 macOS/Linux 回归 one-shot exec 与 interactive shell 行为，确认跨平台修复未破坏既有 POSIX 路径
- [x] 3.2 为 provider 层补充（或更新）跨平台单元测试，覆盖 shell 选择、cwd/env 注入与参数转义关键分支
- [x] 3.3 为 MCP 层补充（或更新）Windows 相关回归测试，覆盖终端 typed tools 的可用性与错误语义

## 4. 验收与文档收口

- [x] 4.1 按新增 spec 场景形成验收记录（包含通过平台、命令样例、结果摘要）
- [x] 4.2 在变更说明中记录后续可选演进项（例如 PowerShell 可配置化），并明确不属于本次交付

## 5. 启动入口自检能力

- [x] 5.1 在 `bridgingio-core` 启动入口新增 `--self-test` 模式，支持无配置执行内建自检并返回明确退出码
- [x] 5.2 在规格与验收文档中记录 `--self-test` 的参数约束、覆盖范围与使用方式
