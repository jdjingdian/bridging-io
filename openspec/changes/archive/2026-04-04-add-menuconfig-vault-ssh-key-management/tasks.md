## 1. Vault SSH key import contract

- [x] 1.1 为 `ssh-private-key` 补齐受信任本地 import 语义，明确 `key name -> canonical credential_ref`、`label`、record id 与 display-safe 返回结构；menuconfig 默认执行 delete-first 更新流程
- [x] 1.2 为 passphrase-protected OpenSSH key 补齐本地导入验证逻辑，确保 passphrase 仅在导入流程中使用，运行时 SSH broker 不再重新请求该 passphrase
- [x] 1.3 补齐 vault 侧回归测试，覆盖 canonical ref 生成、底层接口重复导入同一 ref 创建新 version、非法 key 文件拒绝、以及 display-safe summary / detail 输出

## 2. Menuconfig security surface

- [x] 2.1 在 `bridgingio-operator-console` 的 Security 页面新增 `Import SSH Key --->` 与 `SSH Key Management --->`，并按 vault 锁状态裁剪入口；`locked` 状态下只显示聚合 `SSH Key Count`
- [x] 2.2 实现 SSH key import 向导：至少覆盖 `key name`、`label`、`source path` 输入，以及导入确认后的 display-safe 结果反馈
- [x] 2.3 为 encrypted SSH key 导入补齐受控本地 passphrase 输入链路，禁止把 passphrase 显示在普通 popup、日志或状态文本中
- [x] 2.4 实现 `SSH Key Management` 列表页与详情页，展示 canonical ref、label、status、record id、last rotated / last used 等 display-safe 字段，并提供 `Delete SSH Key --->` + 二次确认入口；同时确保这些页面仅在 `unlocked` 状态下开放

## 3. Target binding integration

- [x] 3.1 为 SSH target 补齐 `Credential Source --->` 或等价子流程，使 operator 可以选择已导入 vault SSH key，而不是继续主要依赖手工输入 raw `credential_ref`；对 `sensitive + ssh`，该流程只在 unlocked `Sensitive Overlay` 中开放
- [x] 3.2 支持在 target 流程内联触发 `Import Local SSH Key Into Vault --->`，并在导入成功后把 canonical `credential_ref` 回填到当前 target
- [x] 3.3 保持 SSH target 落盘语义为“只保存 canonical `credential_ref`”，不得把私钥明文、passphrase 或其他 secret material 写回配置文件；并确保 locked 的 sensitive ssh target 不泄露已绑定 key 身份

## 4. Contracts and docs

- [x] 4.1 更新增量 specs 与 root specs 对应实现，确保 `standalone-operator-console`、`credential-and-approval-control`、`target-session-management` 与 `operator-interface-matrix` 行为一致
- [x] 4.2 更新 `LOCAL_OPERATOR_INTERFACE_MATRIX` 与必要的 menuconfig 相关文档，记录 SSH key import / management / delete-first 更新 / target binding 的输入输出、状态门控和 display-safe 约束
- [x] 4.3 补充验收与验证说明，覆盖 unlocked import、encrypted key import、duplicate key name delete-first、以及 target 成功绑定 imported key 的完整路径
