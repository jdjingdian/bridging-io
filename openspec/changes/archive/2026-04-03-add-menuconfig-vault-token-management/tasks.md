## 1. Vault lifecycle runtime

- [x] 1.1 将缺失 vault metadata 的 runtime 状态收敛为正式 `uninitialized`，并移除 `menuconfig` 对缺失持久化 vault 的默认内存 fallback unlock 路径
- [x] 1.2 为当前实例的 vault store 增加显式 `init` / `delete` 管理语义与 attestation/确认约束，确保 `vault delete` 成功后返回 `uninitialized`
- [x] 1.3 明确 `vault delete` 只作用于当前 runtime store，不隐式清除共享 `os-native` protector，并补充相应错误与状态合同

## 2. Token lifecycle and metadata

- [x] 2.1 为长期 agent token 增加 `deleted` 终态或等价 tombstone 语义，并让默认 token 摘要列表过滤已删除 token
- [x] 2.2 实现 `revoked` 作为唯一 token delete 前置条件，确保 `expired` token 必须先 revoke 再 delete
- [x] 2.3 增加 token 备注更新能力，复用现有 `label` 作为 operator-facing 备注字段并保持 display-safe 摘要兼容
- [x] 2.4 为 expiring token 增加反回拨保护或等价单向过期语义，并在本机时钟异常时返回明确诊断

## 3. Menuconfig security surface

- [x] 3.1 按 `uninitialized / locked / unlocked` 重构 Security 页的状态展示与动作裁剪，仅显示当前状态允许的入口
- [x] 3.2 为 Security 页增加显式 `Init Vault` / `Delete Vault` / `Create Token` / `Token Management` 流程，并为 destructive action 增加二次确认弹窗
- [x] 3.3 实现 `Create Token` 的三步引导式交互（label、有效期、一次性结果弹窗），以及 `Token Management` 子页中的备注编辑、revoke 确认和 delete 确认交互

## 4. Contracts and verification

- [x] 4.1 更新 operator interface matrix 与相关本地管理接口文档，记录 vault/token 的状态门控、确认要求和 delete 前置条件
- [x] 4.2 补充自动化测试，覆盖 vault `uninitialized/locked/unlocked` 状态切换、`vault delete -> uninitialized`、`expired -> revoke -> delete`、expiring token 的回拨保护，以及 menuconfig 的 label/expiry/确认弹窗路径
