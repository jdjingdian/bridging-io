## 新增需求

### 需求:`menuconfig` 必须保存 display-safe 的授权与会话诊断日志
`bridgingio-core menuconfig` 必须把关键授权事件和有价值的会话诊断持久化到当前 runtime root 的 `logs/` 下，而不是只保留瞬时 stderr 或内存中的 `last_status`。系统必须把关键业务节点记为 `INFO` 级事件，把关键调用链 breadcrumb 记为 `DEBUG` 级事件；所有落盘内容都必须保持 display-safe，并允许通过稳定 `flow_id` 关联同一次显式操作。

#### 场景:显式授权动作写入 authorization 日志
- **当** 操作员在 `menuconfig` 中触发 `Unlock Vault`、`Delete Vault`、`Import SSH Key`、`Delete SSH Key`、`Create Token` 或 `Delete Token`
- **那么** 系统必须向 `logs/local-authorization.jsonl` 追加 display-safe 事件
- **并且** 每条事件必须至少包含 `flow_id`、`surface=menuconfig`、`screen`、`action`、`operation`、`phase` 与结果摘要

#### 场景:debug 级别写入 menuconfig breadcrumb
- **当** `menuconfig` 运行时 `core.log_level` 为 `debug` 或 `trace`
- **那么** 系统必须向 `logs/menuconfig-session.jsonl` 追加关键 breadcrumb，例如 screen 切换、授权 worker 开始/结束、去重命中、保存/取消等事件
- **并且** 这些 breadcrumb 必须可通过同一 `flow_id` 与授权日志关联

#### 场景:unlock worker breadcrumb 复用授权 flow_id
- **当** `menuconfig` 在 worker 线程内执行 `Unlock Vault` 验证并在主线程汇总结果
- **那么** `logs/menuconfig-session.jsonl` 中 `action=unlock.worker` 的 debug breadcrumb 必须复用本次 `vault.unlock` 的授权 `flow_id`
- **并且** 不得仅使用会话级 flow 导致同一次 unlock 在 authorization/session 两个日志流中无法直接关联

#### 场景:日志默认保持 display-safe
- **当** `menuconfig` 将授权或 session 事件写入 runtime `logs/`
- **那么** 系统不得把 passphrase、私钥明文、token 明文、自由文本敏感输入或等价 secret material 写入这些日志
- **并且** 若日志涉及 SSH key 或 token，只允许记录 canonical `credential_ref`、稳定 `token_id`、label、field path 或等价 display-safe 标识

## 修改需求

## 移除需求
