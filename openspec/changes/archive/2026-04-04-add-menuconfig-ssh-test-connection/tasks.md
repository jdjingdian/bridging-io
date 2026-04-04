## 1. Shared SSH probe plumbing

- [x] 1.1 提取或新增可复用的 SSH probe helper，使 `menuconfig` 能基于当前 draft SSH target 解析有效 toolchain、命令调用与 secret delivery 参数
- [x] 1.2 定义 SSH 测试连接的受控结果模型与错误分类，至少覆盖 success、failed、timed-out、cancelled、vault-locked-preflight 与 toolchain-unavailable
- [x] 1.3 为真实 SSH one-shot `exit 0` 探测补齐 timeout / cancel 的 worker 生命周期与子进程清理语义

## 2. Menuconfig UX and logging

- [x] 2.1 在 plain SSH 的 `Connection Profile` 和 unlocked sealed SSH 的 `Sensitive Overlay` 中新增 `Test Connection --->`，并保持 sealed locked 状态下不暴露该入口
- [x] 2.2 在 `bridgingio-operator-console` 中实现 timeout 输入弹窗、waiting 弹窗和 result 弹窗，确保测试始终基于当前草稿而不隐式提交
- [x] 2.3 为 SSH 测试连接写入带稳定 `flow_id` 的 display-safe `menuconfig-session` 日志，覆盖 started / finished / failed / cancelled / timed-out 等关键节点

## 3. Regression coverage and docs

- [x] 3.1 补齐自动化回归测试，覆盖 plain draft success、failure、timeout、cancel、sealed locked/unlocked 门控，以及 plain + vault-backed credential 的受控失败路径
- [x] 3.2 更新 `standalone-operator-console`、`menuconfig-style-matrix`、`operator-interface-matrix` 与 `quality-and-test-automation` 的 root specs，使其与实现合同一致
- [x] 3.3 更新 `LOCAL_OPERATOR_INTERFACE_MATRIX.md`、`MENUCONFIG_STYLE_MATRIX.md` 与相关测试矩阵文档，记录 SSH `Test Connection` 的入口、popup 与日志语义

## 4. Broker readiness follow-up

- [x] 4.1 在 secret-backed SSH probe 路径建立 broker endpoint 就绪性合同，确保 `IdentityAgent` 被下发前 endpoint socket 已可访问；若未就绪，返回稳定分类而非泛化失败
- [x] 4.2 在 `menuconfig` 测试连接结果中增加 broker 专属失败提示（endpoint 未就绪/不可访问），避免误导操作员排查 key 导入或用户名
- [x] 4.3 补齐回归测试与日志断言，覆盖 broker endpoint 缺失/未配置/不可访问路径，并验证 `menuconfig-session` 中有 flow-scoped 的 display-safe broker 诊断 breadcrumb
