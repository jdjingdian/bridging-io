## 1. Dirty State Model

- [x] 1.1 为 `MenuConfigApp` 增加全局会话基线配置快照，并将根页面 `is_dirty()` 切换为基于当前 `settings` 与基线配置的结构化比较
- [x] 1.2 调整普通字段编辑、choice 提交与布尔切换路径，使字段恢复到基线值后自动清除对应 dirty path，而不是继续保留编辑历史痕迹
- [x] 1.3 调整 target 删除、discard、apply 与 save 路径，明确区分 target 局部 baseline 与整体保存基线，并保证 target 相关 dirty path 不残留过期索引

## 2. Exit And Regression Coverage

- [x] 2.1 为普通字段补充回归测试，覆盖 `info -> debug -> info`、布尔字段 `false -> true -> false` 等“改回基线后不再提示保存”的场景
- [x] 2.2 为 target 编辑流补充回归测试，覆盖 target 字段改回基线、discard 回滚、`Apply Target --->` 后仍需根页面 `Save` 的场景
- [x] 2.3 为根页面退出流程补充回归测试，确认 dirty 标题、`Esc` / `Exit` 保存确认与 save 后基线刷新都只反映真实未保存差异

## 3. Spec And Matrix Follow-Through

- [x] 3.1 更新 `openspec/specs/standalone-operator-console/spec.md`，正式记录 dirty tracking 与退出确认基于真实配置差异的合同
- [x] 3.2 更新 `openspec/specs/menuconfig-style-matrix/spec.md` 与 `docs/matrix/MENUCONFIG_STYLE_MATRIX.md`，记录 dirty 标记与退出确认的显示条件
- [x] 3.3 更新 `openspec/specs/operator-interface-matrix/spec.md` 与 `docs/matrix/LOCAL_OPERATOR_INTERFACE_MATRIX.md`，记录 `menuconfig` 保存/退出行为与 `Apply Target --->` / `Save` 的边界
