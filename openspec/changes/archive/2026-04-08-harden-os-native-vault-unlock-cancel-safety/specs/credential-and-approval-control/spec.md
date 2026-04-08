## 新增需求

### 需求:verified `os-native` 迁移必须保持 canonical root wrap 的 cancel-safe 持久化
当系统把 legacy fallback wrap 迁移为 verified `os-native` wrap 时，必须先证明当前 verified KEK 与现有 canonical vault root key 绑定正确，再执行显式持久化提交。取消、拒绝、unwrap 失败、平台访问失败或任意提交失败都不得改写既有 wrap blob、verification metadata 或 manifest 状态。

#### 场景:legacy fallback wrap 成功迁移到 verified os-native
- **当** 当前 canonical root wrap 仍使用 legacy fallback KEK，且一次 verified `os-native` 解锁成功证明了当前 keychain protector 可用
- **那么** 系统必须允许把同一个 root key 重新包裹为 verified `os-native` wrap
- **并且** 迁移完成后才能把对应 manifest 更新为已验证或等价 `Ready` 状态

#### 场景:验证取消或失败时不改写既有 wrap
- **当** verified `os-native` 解锁在迁移前或迁移中遭遇用户取消、ACL 拒绝、平台访问失败、unwrap 失败或持久化提交失败
- **那么** 系统必须保持原有 canonical wrap blob、`wrapped_key_digest`、`last_verified_at` 与 `status` 不变
- **并且** 不得留下“blob 已替换但 metadata 未跟上”或“metadata 看似已验证但实际未验证”的半迁移状态

#### 场景:当前 protector 与 canonical wrap 已失配
- **当** 系统确认当前 `os-native` protector 无法解开现有 canonical wrap，且该 wrap 也不再符合允许的 legacy fallback 迁移前提
- **那么** 系统必须返回受控的 `protector-mismatch` 或等价 display-safe 诊断
- **并且** 必须保持 vault 为 `locked` 或 `unavailable`，而不得隐式重建 protector 或覆盖现有 wrap

## 修改需求

### 需求:os-native 解锁链路不得因诊断路径重复触发钥匙串授权
在 macOS/Windows 的 `os-native` 保护器模式下，系统必须把“能力诊断/可用性探测”“protector provisioning”和“显式解锁验证”严格分离：只读状态投影、readiness 诊断和 menuconfig 页面刷新不得触发钥匙串读写；仅在操作员明确执行 `unlock_vault`（或等价解锁动作）时才允许访问平台钥匙串完成受验证解锁。对于同一次解锁流程，系统不得因内部重复探测导致额外授权弹窗；若钥匙串访问控制（ACL）策略仍要求用户确认，单次确认属于平台安全行为，不应被视为业务回归。对于已存在的 canonical vault，显式 verified unlock 只能读取既有 protector；除正式 init/provisioning 路径外，系统不得因为读取失败、用户取消、ACL 拒绝或临时平台错误而生成新 KEK、覆盖既有 `io.bridgingio.vault` 项或触发隐式重建。

#### 场景:浏览 menuconfig 安全摘要时不触发钥匙串授权
- **当** 操作员仅进入 Security/Token Management 页面查看摘要信息，未触发 `unlock_vault`
- **那么** 系统不得访问 `io.bridgingio.vault` 对应钥匙串项，也不得弹出钥匙串授权对话框

#### 场景:解锁流程只执行一次受验证钥匙串访问
- **当** 操作员在 menuconfig 中触发一次 `unlock_vault`
- **那么** 系统必须把钥匙串访问收敛为单次受验证读取语义，不得因为 readiness probe、后台诊断或重复 KEK 获取在同一流程内再次触发同类访问

#### 场景:钥匙串 ACL 要求额外确认时按平台行为处理
- **当** macOS 钥匙串访问控制未将当前 `bridgingio-core` 可执行体标记为“始终允许” 
- **那么** 平台仍可能弹出一次“访问钥匙串中的密钥”确认；该提示属于平台 ACL 行为，系统应保持可预期且不重复触发

#### 场景:已存在钥匙串项但用户取消本次访问
- **当** 当前实例已存在 `io.bridgingio.vault` 对应的 `os-native` protector 项，而操作员在一次显式 verified unlock 中取消平台授权弹窗
- **那么** 系统必须把该结果视为 `cancelled` 或等价受控失败
- **并且** 不得把这次取消解释为“item 不存在”并生成新的 KEK 或覆盖既有 keychain 项

#### 场景:平台拒绝或暂时失败不得触发隐式重建
- **当** verified `os-native` unlock 因 ACL 拒绝、平台访问失败、keychain 值无效或等价读取错误而未能拿到既有 protector material
- **那么** 系统必须 fail-closed 返回受控结果
- **并且** 只有在正式 init/provisioning 路径中才允许创建新的 protector material，而不得在普通 unlock 中隐式写入
