## 1. Auth Model And Compatibility Foundation

- [x] 1.1 在 `bridgingio-domain`、`bridgingio-engine` 与相关投影结构中引入 typed `SshAuthConfig`，覆盖 `kind`、`secure_access`、`password`、`private_key_source` 与 key locator / canonical ref 语义
- [x] 1.2 为 standalone 配置读写增加 legacy `credential_ref` 兼容读取与新 `ssh_auth` 结构的规范化写回
- [x] 1.3 实现 shared SSH auth validator，固化 plain / sealed 与 `none` / `password` / `private-key` 的允许矩阵
- [x] 1.4 为 SSH auth validator 增加“sealed secret-backed auth 强制 `SSH 安全访问`”“plain vault key 禁止”“plain encrypted local key 禁止”等稳定校验结果

## 2. Trusted Local Key Inspection And Import Gating

- [x] 2.1 在受信任本地路径检查逻辑中增加 SSH 私钥格式识别与“是否带 passphrase”检测能力
- [x] 2.2 为 plain SSH 的本地带 passphrase 私钥路径建立阻断结果，明确提示必须改用 sealed + vault import
- [x] 2.3 为 sealed SSH 的本地带 passphrase 私钥路径建立阻断结果，明确提示需要解锁 vault 并进入 `Import Local SSH Key Into Vault`
- [x] 2.4 把本地私钥检测结果接入创建流、编辑流与保存前校验，避免非法组合只在运行时才暴露

## 3. Structured Invocation And Delivery Plan Foundation

- [x] 3.1 为 `CommandInvocation`、`bridgingio-providers` 与相关执行路径增加 invocation-scoped env overlay 能力
- [x] 3.2 为 structured invocation 增加 helper / carrier cleanup contract，确保 askpass helper、临时 env 与等价资源在调用结束后被清理
- [x] 3.3 引入 shared `SshDeliveryPlan` 推导层，覆盖 `None`、`PasswordDirectAskpass`、`PasswordManagedAskpass`、`DirectIdentityFile`、`LocalBrokeredIdentity` 与 `VaultBrokeredIdentity`
- [x] 3.4 让 `bridgingio-engine`、`bridgingio-mcp`、menuconfig SSH probe 与 future interactive SSH 共享同一套 auth->delivery plan 解析逻辑

## 4. Password Delivery Implementation

- [x] 4.1 为 plain + `password` + `SSH 安全访问 = false` 实现 direct password carrier，保证 password 不进入 argv、日志与 artifact
- [x] 4.2 在 `bridgingio-secrets` 或等价 shared runtime 中扩展 target 登录 password 的 managed delivery/session 能力
- [x] 4.3 为 sealed + `password` 与 plain + `password` + `SSH 安全访问 = true` 接入 managed password delivery
- [x] 4.4 收紧 password delivery 失败语义，确保 carrier 建立失败时返回受控错误而不是运行时提示用户输入 password

## 5. Private Key Delivery Implementation

- [x] 5.1 保持 canonical vault SSH key 继续走现有 vault broker runtime，并把它迁移到新的 `SshDeliveryPlan` 框架下
- [x] 5.2 为 local unencrypted key + `SSH 安全访问 = true` 实现 local brokered identity 或等价 secure-local signer delivery
- [x] 5.3 为 local unencrypted key + `SSH 安全访问 = false` 保持显式 `-i <path>` / `IdentityFile=<path>` direct identity 语义
- [x] 5.4 为 secure-local 与 direct identity 两条本地 key 路径补齐 ambient agent / env 隔离与 display-safe diagnostics
- [x] 5.5 收紧 secure-local 失败行为，禁止在 `SSH 安全访问` 开启时静默回落到 direct identity

## 6. Menuconfig SSH Authentication Flow

- [x] 6.1 在 `Add Target` 中为 `kind = ssh` 引入 `SSH Authentication Setup` 步骤，并保持 `storage mode -> target type -> auth setup -> detail editor` 顺序
- [x] 6.2 为 plain SSH 的 password 路径增加正式风险确认，明确 password 可能保留在 `config.toml` 且仍建议升级到 vault / sealed
- [x] 6.3 在 plain `Connection Profile` 与 unlocked sealed `Sensitive Overlay` 中新增 `SSH Authentication --->` 入口
- [x] 6.4 在 `SSH Authentication` 流中实现 `none` / `password` / `private-key` 选择，以及 private-key 分支下的 `Use Local Key Path` / `Use Imported Vault Key`
- [x] 6.5 为 plain secret-backed auth 渲染默认开启、可关闭的 `SSH 安全访问` toggle
- [x] 6.6 为 sealed secret-backed auth 渲染只读的 `SSH 安全访问 = required` 状态，并禁止关闭
- [x] 6.7 把现有 `Credential Source --->`、imported key picker 与 inline import 流程重构为 private-key 分支下的子流程，而不是 SSH auth 的顶层入口
- [x] 6.8 为“本地私钥带 passphrase”补齐阻断弹窗、解锁引导与返回上一步的交互

## 7. Error Mapping, Logs And Operator Documentation

- [x] 7.1 为 SSH auth matrix 新增结构化错误子码，覆盖非法组合、本地带 passphrase 私钥必须导入 vault、password delivery unavailable / rejected 等场景
- [x] 7.2 收紧 menuconfig、MCP 与 shared runtime 的 display-safe 日志边界，确保 password、helper payload 与本地私钥内容不进入公共错误或普通日志
- [x] 7.3 更新 `docs/matrix/LOCAL_OPERATOR_INTERFACE_MATRIX.md`，记录 `SSH Authentication Setup`、`SSH 安全访问`、plain / sealed 约束与阻断路径
- [x] 7.4 更新 `docs/matrix/MENUCONFIG_STYLE_MATRIX.md`，记录 `SSH Authentication --->`、plain toggle、sealed 只读状态与阻断弹窗语法

## 8. Verification And Regression Coverage

- [x] 8.1 为 `bridgingio-engine` / `bridgingio-domain` 增加 auth model、legacy 配置迁移与 validator 的单元测试
- [x] 8.2 为 `bridgingio-providers` / structured invocation 增加 env overlay、helper cleanup 与 argv 泄露边界的回归测试
- [x] 8.3 为 `bridgingio-secrets` / shared runtime 增加 password managed delivery、secure-local key delivery 与失败清理的单元测试
- [x] 8.4 为 `bridgingio-mcp` 增加 SSH auth matrix 集成测试，覆盖 plain password direct/managed、本地 key direct/secure-local 与 vault key broker
- [x] 8.5 为 menuconfig 增加创建流与编辑流回归测试，覆盖 `SSH Authentication Setup`、强制 `SSH 安全访问`、以及本地加密私钥阻断弹窗
- [x] 8.6 复核 proposal/design/specs 与实现的一致性，并确认 `/opsx:apply` 阶段所需的文档、错误分类与测试矩阵已经完整对齐

## 9. Post-Implementation Hardening And UX Fixes

- [x] 9.1 修正 `SSH Authentication Setup` 交互语义：`Space` 负责单选切换，`Enter` 负责进入详情编辑，不再使用 `Enter` 直接切换认证类型
- [x] 9.2 为 `none` / `password` / `private-key` 增加互斥单选语义与继续门控：必须完成合法选择后才允许进入详情编辑；`password` / `private-key` 需完成必填输入并通过校验
- [x] 9.3 修复 plain / sealed `private-key` 来源菜单重复与越权显示：plain 路径不再展示 vault 来源，移除重复的本地 key 行
- [x] 9.4 为 `Test Connection` 的 debug/trace 输出补齐 `ssh_probe_context`，记录 delivery plan、env overlay 键、关键 `-o` 参数与 askpass/staged preflight 状态
- [x] 9.5 修复 password probe 参数冲突：消除 `BatchMode=yes/no`、`NumberOfPasswordPrompts=0/1` 并存，确保 password 路径稳定使用 `BatchMode=no` 与 `NumberOfPasswordPrompts=1`
