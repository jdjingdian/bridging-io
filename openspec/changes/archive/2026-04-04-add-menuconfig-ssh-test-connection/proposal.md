## 为什么

当前 `menuconfig` 的 SSH target 新增/管理流程只允许操作员填写主机、端口、用户名与密钥引用，但在 `Apply Target` 或离开页面之前，没有正式入口验证这些配置是否真的可以建立 SSH 连接。这样会让操作员在保存后才发现配置错误，尤其是在 imported key、用户名、超时或 toolchain 解析存在偏差时，排障成本较高。

现在需要把“测试连接”补成正式的 menuconfig 交互：操作员在 SSH 详情页里就可以对当前草稿配置执行一次真实连接探测，界面只显示成功或失败，详细过程落入 display-safe 日志；同时对 sealed target 保持与现有 vault 解锁边界一致，避免在 locked 状态下绕过敏感配置门控。

## 变更内容

- 为 `menuconfig` 的 SSH target 管理页新增 `Test Connection --->` 入口，并继续沿用单栏逐级进入与 centered popup 的既有交互风格。
- 将 plain SSH 的测试入口放在 `Connection Profile` 中，允许操作员直接对当前草稿配置发起测试，而不要求先 `Apply Target` 或 `Save`。
- 将 sealed SSH 的测试入口放在 `Sensitive Overlay` 中，并要求该入口只在 vault `unlocked` 时可见；`locked` 状态继续只显示通用锁定提示与 `Unlock Vault --->`。
- 为测试连接增加 timeout 配置弹窗、等待态弹窗以及成功/失败结果弹窗；等待态支持 `Esc` 取消，最终界面只显示简洁状态，不回显详细连接日志。
- 要求测试连接执行真实 SSH 连通性探测，而不是仅做字段校验；详细过程必须写入 display-safe 的 menuconfig 日志，包含稳定 `flow_id`、结果摘要与受控错误分类。
- 补充 menuconfig 风格矩阵、operator interface matrix 和自动化回归合同，覆盖成功、失败、超时、取消、sealed locked/unlocked 门控以及日志 display-safe 边界。

## 功能 (Capabilities)

### 新增功能

无。

### 修改功能

- `standalone-operator-console`: SSH target 详情页新增测试连接入口、popup 流程、draft-aware 测试语义，以及 plain/sealed 的状态门控。
- `menuconfig-style-matrix`: 风格矩阵需要记录 `Test Connection --->` 行语法、timeout/edit/wait/result popup 的布局与按键合同。
- `operator-interface-matrix`: 本地 operator interface matrix 需要记录 SSH 测试连接的入口位置、输入输出、状态前置条件、日志产物与 display-safe 约束。
- `quality-and-test-automation`: 自动化测试需要覆盖 menuconfig SSH 测试连接的成功、失败、超时、取消与日志落盘行为。

## 影响

- 受影响代码主要包括 `source/rust/bridgingio-operator-console` 的 SSH target 页面、popup/worker 状态机与 menuconfig 日志记录路径。
- 受影响文档包括 `openspec/specs/standalone-operator-console/spec.md`、`openspec/specs/menuconfig-style-matrix/spec.md`、`openspec/specs/operator-interface-matrix/spec.md`、`openspec/specs/quality-and-test-automation/spec.md`，以及对应的 `docs/matrix/*` 文档。
- 实现阶段大概率还会触及 SSH 连接探测所依赖的 toolchain / secret-delivery 复用边界，但本提案不扩大到 target 数据模型或 vault truth model 的新迁移。
