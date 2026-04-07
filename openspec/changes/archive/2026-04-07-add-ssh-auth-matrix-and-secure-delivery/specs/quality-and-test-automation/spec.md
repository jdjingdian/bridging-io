## 新增需求

### 需求:SSH auth matrix 与 SSH 安全访问必须具备自动化回归覆盖
针对 SSH target 的自动化、contract test 与 self-test 必须覆盖新的认证矩阵与 `SSH 安全访问` 语义，而不是继续只验证 vault key broker 与 direct identity 两端。测试必须证明 plain / sealed、password / local key / vault key、direct / managed delivery、以及非法组合阻断都符合正式合同。

#### 场景:plain password 在 direct 与 managed delivery 间切换
- **当** 自动化分别执行 plain SSH target 的 `password + SSH 安全访问 = false` 与 `password + SSH 安全访问 = true`
- **那么** 测试必须验证两者都不通过 cmdline 暴露 password
- **并且** 必须验证前者走 direct carrier、后者走 managed delivery 语义

#### 场景:password probe 参数覆盖不允许互斥并存
- **当** 自动化执行 password `Test Connection` 并抓取实际 SSH argv 或 verbose context
- **那么** 测试必须验证最终参数中仅保留 `BatchMode=no` 与 `NumberOfPasswordPrompts=1`
- **并且** 必须验证同一次调用中不会出现 `BatchMode=yes`、`NumberOfPasswordPrompts=0` 等冲突组合

#### 场景:本地未加密私钥在 direct 与 secure-local 间切换
- **当** 自动化分别执行本地未加密私钥的 `SSH 安全访问 = false` 与 `SSH 安全访问 = true`
- **那么** 测试必须验证前者走显式 `-i` / `IdentityFile` 路径
- **并且** 必须验证后者走 local brokered identity 或等价 secure-local delivery，而不是 direct identity

#### 场景:sealed secret-backed SSH 强制安全访问
- **当** 自动化尝试为 sealed SSH target 配置 `password`、本地未加密私钥或 vault-managed key，并同时关闭 `SSH 安全访问`
- **那么** 测试必须验证系统拒绝该组合或自动回正为强制开启
- **并且** 不得允许 sealed secret-backed auth 以 direct path 通过验证

#### 场景:本地带 passphrase 的私钥在创建流中被阻断
- **当** 自动化在 `SSH Authentication Setup` 中输入一个带 passphrase 的本地私钥路径
- **那么** 测试必须验证系统立即阻断后续创建或保存
- **并且** 必须验证界面反馈明确指向 sealed + vault import 路径

#### 场景:legacy SSH 配置迁移到新 auth 模型
- **当** 自动化加载 legacy `credential_ref` 风格的 SSH target 并执行一次保存或应用
- **那么** 测试必须验证系统继续按旧语义运行
- **并且** 必须验证写回结果被规范化到新 auth 模型而不改变既有 direct / vault 行为

#### 场景:menuconfig verbose SSH 日志包含 display-safe probe context
- **当** 自动化在 debug/trace 下执行 menuconfig `Test Connection`
- **那么** 测试必须验证 verbose 日志包含 probe context 区块（delivery plan、env overlay 键名、关键参数覆盖、carrier preflight 状态）
- **并且** 必须验证该区块不包含 password 明文或私钥明文

## 修改需求

## 移除需求
