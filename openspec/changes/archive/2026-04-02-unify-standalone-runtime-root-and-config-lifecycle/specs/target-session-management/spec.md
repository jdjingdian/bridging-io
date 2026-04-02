## 新增需求

### 需求:standalone 默认 runtime root 与配置路径必须统一到用户目录 `.bridgingio`
BridgingIO 在 standalone 模式下，未显式提供 `--config` 时，必须把用户目录下的 `.bridgingio` 作为 canonical 默认 runtime root，并从该目录解析或创建默认配置文件。系统不得继续要求 standalone 只能通过显式 `--config` 启动。

#### 场景:未提供 `--config` 启动 standalone
- **当** 操作员以 standalone `run` 或 `-d` 模式启动 core，且未传入 `--config`
- **那么** 系统必须从用户目录下的 `.bridgingio` 解析 runtime root、保留目录和默认配置，而不是直接报缺少 `--config`

#### 场景:显式提供 `--config`
- **当** 操作员为 standalone 启动显式传入 `--config <path>`
- **那么** 系统必须使用该配置路径作为覆盖入口，并优先采用该配置而不是默认 `.bridgingio` 路径

### 需求:runtime/config bootstrap 必须提供共享生命周期状态与恢复动作
BridgingIO 必须把 runtime root 解析、可写性校验、配置 load-or-create、迁移、修复与受控失败纳入共享生命周期状态机，并向本地 control-plane、TUI 与 future UI 返回明确状态和恢复动作。

#### 场景:默认 runtime root 不可写
- **当** core 在解析 standalone 默认 `.bridgingio` runtime root 后发现该目录不存在、不可写或保留子目录校验失败
- **那么** 系统必须进入明确的恢复状态，并返回诸如重新选择目录、修复权限或重试等恢复动作，而不是静默退回到随机临时目录

#### 场景:配置需要迁移
- **当** core 在启动阶段检测到默认配置或显式配置需要迁移、修复或版本升级
- **那么** 系统必须返回明确的 lifecycle 状态和迁移提示，而不是在未告知调用方的情况下继续使用不兼容配置
