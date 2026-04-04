## 1. 共享授权事件与日志落盘基础

- [x] 1.1 在共享层定义本地受信任授权事件 schema，固定 `flow_id`、`surface`、`screen`、`action`、`operation`、`phase`、`result` 与 `dedupe_state` 等 display-safe 字段
- [x] 1.2 在共享层实现 append-only JSONL sink，支持写入 runtime root `logs/local-authorization.jsonl` 与 `logs/menuconfig-session.jsonl`
- [x] 1.3 复用现有 `RuntimeLogLevel` 语义，明确 `INFO` 关键事件与 `DEBUG` breadcrumb 的持久化边界，并为写入失败提供不阻塞主流程的 fallback 诊断

## 2. Secrets 层 verified os-native singleflight

- [x] 2.1 在 `bridgingio-secrets` 的 verified `os-native` 路径引入同进程 singleflight / joined-waiter 语义，避免并发请求重复触发平台验证
- [x] 2.2 为 shared verification 结果补齐 `leader` / `joined` / `cancelled` / `failed` 等 display-safe 事件与缓存边界，确保失败不会形成成功缓存
- [x] 2.3 补齐 `bridgingio-secrets` 单元测试，覆盖并发 joined、等待者取消与单次 provider 调用约束

## 3. Surface 集成

- [x] 3.1 在 `bridgingio-operator-console` 中为 `vault.unlock`、`vault.delete`、`ssh_key.import`、`ssh_key.delete`、`auth.token.create` 与 `auth.token.delete` 接入统一授权事件入口
- [x] 3.2 为 `menuconfig` 增加 session 日志落盘，记录会话开始/结束、关键 screen/action、unlock worker 生命周期、保存/取消/失败等高价值 breadcrumb
- [x] 3.3 将 standalone CLI 现有 `emit_management_audit(...)` 迁移到同一授权事件 schema，并把等价事件写入 runtime `logs/`

## 4. 合同验证与文档

- [x] 4.1 为 `menuconfig` 与本地授权链路补齐自动化测试，覆盖 authorization JSONL 落盘、display-safe 边界与 debug breadcrumb
- [x] 4.2 为 verified `os-native` singleflight 增加回归验证，证明同一进程并发显式解锁只触发一次平台验证
- [x] 4.3 更新 `LOCAL_OPERATOR_INTERFACE_MATRIX`、相关 specs 与验证说明，记录授权动作分类、日志文件位置、flow 关联与去重语义

## 5. 修正规范对齐（补充）

- [x] 5.1 修正 `menuconfig` unlock worker 的跨线程事件透传：确保 `dedupe_state` 从 verified `os-native` worker 结果回传到 authorization 事件，而不是在主线程丢失后退化为 `not-applicable`
- [x] 5.2 修正 unlock 相关 session breadcrumb 的 `flow_id` 关联语义：`unlock.worker` 的 debug 事件必须复用同一次授权动作 `flow_id`，避免与会话级 flow 分离导致排障歧义
- [x] 5.3 补充回归测试覆盖上述修正（dedupe 透传 + flow 关联），并将修正语义固化到 specs，避免后续实现偏离
