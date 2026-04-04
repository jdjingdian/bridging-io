## 1. Vault authoritative target-profile

- [x] 1.1 在 `bridgingio-secrets` 中引入 `target-profile` secret kind 与对应 payload 结构，确保同一对象可同时保存 `public_descriptor`、`sensitive_overlay` 与 `public_descriptor_digest`
- [x] 1.2 为现有旧格式 sealed overlay 提供升级路径：当外层 public descriptor 仍可用时，在 unlocked 状态下升级写回新的 `target-profile` payload
- [x] 1.3 收敛 sensitive target 的 digest 语义，只对 `public_descriptor` 字段计算摘要，不再混入 notes、connection summary 或其他 sensitive overlay 内容

## 2. Runtime reconcile and config cache

- [x] 2.1 在 `bridgingio-mcp` / runtime target catalog 中把 sensitive target 真相改为 vault authoritative `target-profile`，plain target 继续保持 `config.toml` authoritative
- [x] 2.2 实现 `unlock vault -> reconcile sensitive targets` 流程：列出 `target-profile` 摘要、读取 authoritative payload、校验 digest，并修复或刷新 `config.toml` 中的 sensitive target public cache
- [x] 2.3 为 sensitive target 增加 locked/resolved/repair-needed/tamper 等正式 projection 语义，并确保 locked 状态下只返回 public cache
- [x] 2.4 补齐 migration / runtime 回归测试，覆盖旧格式升级、手工删除 public cache 后 unlock 恢复、digest mismatch/tamper 与 repair-needed 诊断

## 3. Menuconfig targets surface

- [x] 3.1 重构 `bridgingio-operator-console` 的 Targets 列表页与详情页，使其按 `menuconfig` 单栏拓扑提供 `Public Descriptor`、`Sensitive Overlay`、`Policy` 与 `Delete` 分段入口
- [x] 3.2 实现 `Add Target` 的 mode-first 流程：先选 `plain/sensitive`，再选 target type，并在 vault locked 时禁用 sensitive create 路径
- [x] 3.3 为 `plain + ssh` 增加强风险确认弹窗，并为 sensitive target 的 create/edit/delete 增加 locked/unlocked 状态裁剪与 `Unlock Vault --->` 引导
- [x] 3.4 为 `sensitive + ssh` 补齐 unlocked `Sensitive Overlay -> Credential Source --->` 绑定流程，并确保 locked 状态下不暴露 imported key 的 label / canonical ref
- [x] 3.5 补齐 Targets 页面相关的中英文文案、交互 smoke 或 contract 测试，覆盖 plain/sensitive 创建、locked 态 sensitive detail、unlocked 态删除、`sensitive + ssh` key binding 与 unlock 后 reconcile 刷新
- [x] 3.6 收敛 Target 编辑会话语义：创建流仅显示 `Create Target --->`、管理流显示 `Apply Target --->` + `Delete Target --->`，并在 `Esc/Exit` 离开未提交会话时强制二次确认丢弃（含 create 草稿删除与 manage 回滚）

## 4. Contracts and docs

- [x] 4.1 更新增量 specs 与 root specs 对应实现，确保 `standalone-operator-console`、`target-session-management`、`credential-and-approval-control`、`operator-interface-matrix` 与最终行为一致
- [x] 4.2 更新 `LOCAL_OPERATOR_INTERFACE_MATRIX`、必要的 menuconfig style / interface 文档与测试矩阵，记录 Targets 页面新增的 state gating、risk confirmation、reconcile 与删除语义
- [x] 4.3 补充端到端验证说明，确保后续实现阶段可以明确验收“vault authoritative sensitive target + config public cache + unlock reconcile”整条产品路径
