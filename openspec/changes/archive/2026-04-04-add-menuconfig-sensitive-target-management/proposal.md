## 为什么

当前 `menuconfig` 的 Targets 页面仍然停留在“直接编辑 config 中的 target 字段”阶段，尚未把 `plain` 与 `sensitive` target 的存储边界、锁状态差异和删除语义正式产品化。随着 canonical vault runtime 已经落地，继续让敏感 target 依赖脆弱的 config 外层 descriptor，会让用户在手工修改配置、vault 锁定和 target 恢复场景下难以判断哪一层才是真相源。

现在需要把 Targets 管理流程补齐为正式的 menuconfig 体验：允许普通 target 继续走 config-first 兼容路径，同时把 sensitive target 明确定义为 vault-authoritative 对象，并在 unlock 后执行受控 reconcile，确保公开 config 只扮演可恢复的 public cache，而不是敏感 target 的唯一真相。

## 变更内容

- 为 `menuconfig` 的 Targets 页面补齐正式的添加、详情、删除与状态裁剪流程，继续沿用单栏逐级进入的 `menuconfig` 交互拓扑，而不是切换为分栏表单式 TUI。
- 将 `Add Target` 流程正式定义为先选择 `plain` / `sensitive` 模式，再选择受支持的 target 类型；其中 `plain + ssh` 允许继续创建，但必须先经过明确的风险确认。
- 将 `sensitive target` 的真相源切换为 vault 中的 `target-profile` 对象：vault 保存完整 `public_descriptor + sensitive_overlay`，`config.toml` 只保留可恢复的 public cache。
- 要求 `vault locked` 时禁止创建、编辑或删除 `sensitive target`，避免操作员在无法保存 overlay 时进入半完成状态。
- 对 `sensitive + ssh`，要求 imported vault SSH key 的绑定继续留在 `sensitive_overlay` 中；`locked` 状态下不得泄露所选 key 的 label 或 canonical ref，`unlocked` 状态下再通过 `Sensitive Overlay` 中的正式 picker / 绑定流程管理该字段。
- 要求 `unlock vault` 成功后执行 `reconcile sensitive targets`：从 vault authoritative target-profile 重新读取 public descriptor，修复或刷新 `config.toml` 中缺失、过期或被手工删除的 sensitive target public cache。
- 细化 `sealed` target 的摘要、恢复和校验语义：public descriptor digest 仅覆盖 public descriptor 字段，不再混入 notes、connection summary 等 sensitive 内容。

## 功能 (Capabilities)

### 新增功能

无。

### 修改功能

- `standalone-operator-console`: Targets 页面新增 `plain/sensitive` 添加流程、locked/unlocked 状态裁剪、sensitive target 的详情与删除门控，以及 unlock 后的 sensitive target reconcile 行为。
- `target-session-management`: sensitive target 的真相模型从“config 外层 descriptor + vault overlay”收紧为“vault authoritative target-profile + config public cache”，并补充 public descriptor、sensitive overlay、digest、SSH credential binding 与 reconcile 语义。
- `credential-and-approval-control`: vault 需要支持 `target-profile` secret kind 的 display-safe 发现、受控读取与删除语义，使 sensitive target 可以作为 vault-managed 对象参与本地管理面。
- `operator-interface-matrix`: 本地 operator interface matrix 需要记录 Targets 页面中新引入的 state gating、sensitive target reconcile、以及 create/delete 的 unlock 前置条件。

## 影响

- 受影响代码主要包括 `source/rust/bridgingio-operator-console` 的 Targets 页面与流程状态机、`source/rust/bridgingio-engine` 的 standalone target 配置读写语义、`source/rust/bridgingio-mcp` 的 sealed target catalog projection / reconcile 逻辑，以及 `source/rust/bridgingio-secrets` 的 vault secret kind 与 summary 路径。
- 受影响文档包括 `openspec/specs/*` 相关规范、`docs/matrix/LOCAL_OPERATOR_INTERFACE_MATRIX.md`、`docs/matrix/MENUCONFIG_STYLE_MATRIX.md` 与未来的测试矩阵。
- 对现有 plain target 兼容路径无破坏性迁移要求；legacy plain target 仍保持 `config.toml` 为真相源，但 sensitive target 的恢复与删除必须改为通过 vault-managed 路径完成。
