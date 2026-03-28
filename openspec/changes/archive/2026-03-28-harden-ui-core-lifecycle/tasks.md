## 1. 实例发现与 ownership 协议基础

- [x] 1.1 将 bundled 默认发现路径切换为基于 runtime root 的稳定 control-plane identity，并把 `--control-plane-socket-override` 收敛为调试/测试逃生口
- [x] 1.2 为 managed core 增加稳定的 instance metadata / lease 记录，至少暴露 `core_instance_id`、host mode、pid、started_at、runtime root 与本地 endpoint 信息
- [x] 1.3 扩展本地 control-plane / app API，增加启动前 probe 能力与结构化 ownership conflict 响应
- [x] 1.4 将 attach 标识从单一 `ui_instance_id` 演进为稳定 `host_id` 与瞬时 `ui_session_id` 或等价协议形状，为后续 reattach/takeover 预留边界

## 2. `ui-managed-ephemeral` 生命周期协调

- [x] 2.1 在 bundled host 启动路径中实现“发现旧实例 -> probe -> reconcile -> spawn”的启动前协调流程，而不是直接拉起新 core
- [x] 2.2 为 `ui-managed-ephemeral` 实现 orphan core 回收策略，覆盖可响应旧实例的优雅关闭、不可响应 stale artifact 的清理，以及不可自动回收实例的冲突分支
- [x] 2.3 将正常退出与受控重启统一为“request shutdown -> wait process exit -> wait endpoint removal -> wait model-plane release”的两阶段关闭流程
- [x] 2.4 为关闭超时或 probe 失败路径增加受控升级策略与诊断记录，避免静默残留 orphan core

## 3. macOS Host 与控制台状态机

- [x] 3.1 将 managed core 的启动/关闭协调从 `WorkspaceViewModel.deinit` 上移到更可靠的 macOS app/host 生命周期层
- [x] 3.2 扩展 macOS host lifecycle state machine，覆盖 `discovering_existing`、`probing_existing`、`reconciling_orphan`、`waiting_resource_release`、`ownership_conflict` 等正式状态
- [x] 3.3 更新 macOS 控制台连接状态与恢复 UI，提供 orphan 回收、冲突恢复、日志查看与重试入口
- [x] 3.4 更新 macOS bundled 启动逻辑，使其消费稳定 discovery identity 与新的 probe/ownership 摘要，而不是继续依赖按 UI 会话生成的临时 endpoint

## 4. 验证与文档

- [x] 4.1 为 Rust/core 侧补充验证，覆盖 orphan 实例探测、stale artifact 清理、正常退出等待、监听地址释放与快速重启
- [x] 4.2 为 macOS host/UI 补充验证，覆盖启动前回收 orphan core、发现活动实例冲突、重启等待释放与恢复动作展示
- [x] 4.3 更新 `docs/DEVELOPMENT.md`、runtime handoff 与相关生命周期文档，说明 bundled 默认 ownership mode、reconcile 流程与 persistent follow-up 边界
