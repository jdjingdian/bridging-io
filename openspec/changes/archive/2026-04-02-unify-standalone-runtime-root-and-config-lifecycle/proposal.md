## 为什么

当前 runtime root 与 config 生命周期在 standalone 与 `ui-managed-ephemeral` 之间是分裂的：standalone 仍要求显式 `--config`，而 UI-managed 已经具备 load-or-create 和 runtime root 校验路径。与此同时，默认数据目录、权限失败恢复、配置迁移与生命周期状态机也还没有形成统一的 core 真相，这会直接阻碍 standalone 成为短期主操作面。

现在需要把 standalone 的 runtime root、默认配置路径、`--config` 覆盖语义和生命周期状态机收束成统一模型，让 core 在无 UI 的情况下也能稳定完成初始化、恢复和诊断。

## 变更内容

- 将 standalone 的默认 runtime root 统一到用户目录下的 `.bridgingio`，在未显式传入 `--config` 时使用 canonical 默认路径与 load-or-create 语义。
- 明确 `--config` 的覆盖规则：带 `--config` 时使用指定配置路径；不带时由 core 解析默认 runtime root 和默认配置文件位置。
- 建立 core 共享的 runtime/config 生命周期状态机，覆盖 bootstrap、路径解析、可写性校验、配置 load-or-create、迁移、修复、受控失败和恢复提示。
- 将 runtime root 权限不足、路径失效、配置不兼容和需要迁移等情况，统一映射到生命周期状态与恢复动作，而不是让不同宿主模式各自处理。
- **BREAKING**: standalone 的启动契约从“必须显式 `--config`”切换为“默认路径优先，`--config` 仅作覆盖”；相关文档、脚本和测试需要同步调整。

## 功能 (Capabilities)

### 新增功能

### 修改功能
- `target-session-management`: standalone runtime root 解析、默认配置行为、`--config` 覆盖语义与启动生命周期状态机发生规范级变更。
- `desktop-operator-console`: runtime-root onboarding、恢复路径与 core 生命周期诊断必须对齐新的共享状态机，而不是继续维护独立语义。

## 影响

- 受影响代码主要包括 `source/rust/bridgingio-mcp` 启动入口、`source/rust/bridgingio-engine` 配置模型、`source/rust/bridgingio-platform` 运行路径解析、`source/rust/bridgingio-desktop-host` 的 onboarding/startup 合约。
- 需要同步更新开发文档、operator 手册、启动 smoke 与 lifecycle 相关测试。
