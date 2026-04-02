## 上下文

当前 BridgingIO 的 runtime root 与配置生命周期存在明显分裂：

- `ui-managed-ephemeral` 已具备 runtime root 校验与 load-or-create
- standalone `run/-d` 仍要求显式 `--config`
- 默认数据目录在不同宿主上并不一致
- 权限失败、目录失效、配置迁移和恢复动作尚未被统一建模

这与当前要把 standalone 作为短期主要操作面的方向相冲突。为了让后续 menuconfig、CLI 和 future UI 都能共享同一条启动真相，runtime root 与 config lifecycle 必须先统一。

## 目标 / 非目标

**目标：**

- 统一 standalone 与 UI-managed 的 runtime root / config 真相模型
- 将默认 standalone 根目录固定到用户目录下的 `.bridgingio`
- 建立共享的 bootstrap/lifecycle 状态机与恢复动作
- 明确 `--config` 只作为覆盖入口，而不是唯一启动路径
- 避免权限失败时静默退回随机临时目录

**非目标：**

- 本次不解决多实例编排与实例目录命名策略的全部问题
- 本次不设计新的 GUI 目录选择体验
- 本次不把所有 host-specific packaging 细节都塞进 core

## 决策

### 决策 1：standalone 默认 runtime root 统一为用户目录下的 `.bridgingio`

在未显式提供 `--config` 时，standalone 一律从用户目录下的 `.bridgingio` 解析 runtime root 与默认配置。

这样做的原因：

- 与用户要求的“默认在用户目录创建 `.bridgingio`”保持一致
- 减少不同平台/宿主模式间的目录漂移
- 为 menuconfig、CLI 和 future UI 提供统一的默认真相

### 决策 2：`--config` 是覆盖入口，不再是 standalone 唯一入口

standalone 启动优先级固定为：

1. 显式 `--config`
2. 默认 runtime root 下的 canonical config

这意味着：

- `run/-d` 不再强制要求显式 `--config`
- 手动指定配置仍然是合法高级入口
- 文档和脚本需要改为“默认路径优先、显式覆盖可选”

### 决策 3：runtime/config bootstrap 使用共享状态机

共享状态机建议至少覆盖：

```text
bootstrap
  -> resolve_runtime_root
  -> validate_runtime_root
  -> load_or_create_config
  -> migrate_or_repair_if_needed
  -> locked_or_ready
  -> running

error branches:
  -> needs_relocate
  -> needs_permission_fix
  -> needs_migration
  -> failed
```

该状态机同时服务：

- standalone 前台/后台模式
- UI-managed onboarding/recovery
- future menuconfig 启动提示

### 决策 4：core 负责状态与恢复语义，宿主负责目录选择与展示

职责边界保持：

- core：状态机、校验、配置 load-or-create、恢复诊断
- host/TUI/UI：展示状态、选择目录、触发重试/迁移/修复

这样可以避免后续 menuconfig 或 GUI 再各自维护一份生命周期判断逻辑。

### 决策 5：权限失败必须 fail closed，并返回明确恢复动作

在 packaged host（例如 macOS app bundle）或普通 standalone 环境中，只要默认目录不可写、路径失效或保留子目录不满足要求，系统都必须进入受控失败或恢复态，而不是静默切换到临时目录继续运行。

## 风险 / 权衡

- [风险] 改变 standalone 默认启动语义会影响现有脚本
  → 缓解措施：保留 `--config` 覆盖入口，并在迁移期更新文档与测试
- [风险] 固定 `.bridgingio` 可能弱化未来多实例灵活性
  → 缓解措施：先把默认路径统一；多实例留给后续显式设计
- [风险] 将 lifecycle 收束到 core 会提高核心启动逻辑复杂度
  → 缓解措施：明确 host 只负责展示与触发动作，避免双向漂移

## 迁移计划

1. 统一 runtime root 默认值解析。
2. 为 standalone 引入默认 config load-or-create。
3. 将 lifecycle 状态与恢复动作投影到 control-plane 与 host 合同。
4. 更新 onboarding/startup 流程、文档与测试脚本。

## 未决问题

- 默认 standalone config 的文件名是否沿用现有命名，还是显式切到新的 canonical 名称
- 默认 `.bridgingio` 目录是否需要预留子目录用于 future multi-instance
