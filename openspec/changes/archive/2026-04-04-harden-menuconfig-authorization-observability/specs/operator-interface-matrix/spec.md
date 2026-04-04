## 新增需求

### 需求:本地 operator interface matrix 必须记录授权日志与 flow 关联合同
当本地 operator surface 提供受信任授权动作时，接口矩阵必须明确记录这些动作的正式 `operation` 分类、日志落盘位置、display-safe 输出边界与 flow 关联语义，而不是只记录“某个页面有按钮”或“某个 CLI 命令会执行成功”。矩阵至少必须覆盖 `menuconfig` 与 standalone CLI 的高风险授权动作。

#### 场景:矩阵记录 menuconfig 授权日志输出
- **当** `menuconfig` 的 Security、SSH key 或 token 管理流会触发本地受信任授权动作
- **那么** 接口矩阵必须明确记录这些动作对应的 `operation` 分类、`logs/local-authorization.jsonl` 与 `logs/menuconfig-session.jsonl` 输出位置，以及事件只允许包含 display-safe 字段的合同

#### 场景:矩阵记录 standalone CLI 的等价授权语义
- **当** standalone CLI 暴露 `vault unlock`、`vault delete`、`token delete` 或等价高风险动作
- **那么** 接口矩阵必须明确记录这些命令与 `menuconfig` 共享同一套授权分类与 flow 语义
- **并且** 不得把 CLI 审计输出描述为与 TUI 完全独立的另一套合同

### 需求:本地 operator interface matrix 必须记录显式解锁的去重语义
当 `menuconfig` 或其他本地受信任 surface 触发 verified `os-native` 解锁时，接口矩阵必须明确记录“一次显式授权流程对应一次平台验证”的去重语义，以及 `leader/joined` 或等价 joined-flow 行为，而不是只记录“会触发系统验证”。

#### 场景:矩阵记录 menuconfig Unlock Vault 的单次触发语义
- **当** 接口矩阵记录 `menuconfig -> Security -> Unlock Vault`
- **那么** 矩阵必须明确记载同一进程内的近同时调用链会合并到同一次 verified `os-native` 验证
- **并且** 必须记录 joined 调用者不会额外触发第二次系统认证窗口

## 修改需求

## 移除需求
