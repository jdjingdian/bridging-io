## 为什么

当前 `menuconfig` 的 Security 页面已经把 vault 状态与 token 管理产品化，但 SSH 私钥这条能力链仍然停留在“底层已支持、operator surface 未成型”的状态：

- `bridgingio-secrets` 已经能够保存 canonical `vault://.../ssh-private-key/...` secret，并为运行时 SSH broker 提供交付能力
- standalone CLI 已经存在通用 `bridgingio-core vault import` 路径
- `menuconfig` 的 Security 页面仍然只有 vault 操作与 token 管理，没有正式的 SSH key import / list / delete-first 更新入口
- SSH target 目前仍主要依赖手填 `credential_ref` 或继续使用本地文件路径/外部 SSH 配置，缺少“把本地 key 导入 vault 并绑定到 target”的受控产品路径

这会让 SSH target 的产品故事始终缺一块。尤其在 `add-menuconfig-sensitive-target-management` 已经开始把 sensitive target 收敛为 vault-authoritative 对象之后，如果没有独立的 SSH key 管理提案，后续 target 管理仍会被迫停留在“手工拼 ref 或继续依赖宿主本地文件”的半完成状态。

现在需要新增一个独立 change，把 `ssh-private-key` 正式产品化为 menuconfig 可管理的安全对象：允许本地 operator 通过受控流程把 SSH 私钥导入 vault，查看 display-safe 摘要，并在 SSH target 流程中绑定已导入的 key，而不是继续要求用户记住 raw `vault://...` URI。

## 变更内容

- 为 `menuconfig` 的 Security 页面新增正式的 `Import SSH Key --->` 与 `SSH Key Management --->` 入口，并根据 vault 锁状态裁剪这些入口；`locked` 状态下最多只显示聚合 `SSH Key Count`，不得暴露任何单个 key 身份信息。
- 将 SSH key import 产品化为 metadata-first 的本地受控流程：操作员先填写 key name / label / source path，再由系统在本地读取 key 文件；若 key 自带 passphrase，则必须通过受控本地提示完成验证与导入。
- 为 `ssh-private-key` 提供正式的 display-safe 管理页：仅在 vault 已解锁时开放列表页和详情页；`locked` 状态下禁止展示 label、canonical ref、status、record id 等单个 key 摘要，只允许在 Security 页显示聚合数量。
- 为 SSH target 补齐“绑定已导入 vault key”的正式 menuconfig 子流程，避免 operator 继续只能手填原始 `credential_ref`。该绑定流程必须输出 canonical `vault://...` 引用，而不是把 key material 写回目标配置；对于 `sensitive + ssh`，该流程只在 unlocked 的 `Sensitive Overlay` 中开放，locked 状态不得泄露已绑定 key 身份。
- 明确 imported SSH key 与 target storage mode 解耦：plain SSH target 与 future sensitive SSH target 都可以引用同一 `ssh-private-key` secret；本 change 不重新定义 target storage class，只补齐 key 管理与绑定路径。

## 功能 (Capabilities)

### 新增功能

- 无

### 修改功能

- `standalone-operator-console`: Security 页面新增 SSH key import / management 入口，并为 SSH target 增加 vault key 绑定子流程。
- `credential-and-approval-control`: 补齐 `ssh-private-key` 的 operator-facing import / summary / delete-first 更新语义，以及 passphrase-protected key 的本地导入合同。
- `target-session-management`: SSH target 的 `credential_ref` 绑定流程从“手工输入优先”提升为“可选择已导入 vault key 的正式 operator 流程”。
- `operator-interface-matrix`: 本地 operator interface matrix 需要记录 `Import SSH Key`、`SSH Key Management`、`Delete SSH Key`（delete-first 更新）与 target key binding 的输入输出、状态门控与 display-safe 合同。

## 影响

- 受影响模块主要包括 `source/rust/bridgingio-operator-console` 的 Security / Target 交互流、`source/rust/bridgingio-secrets` 的 SSH key import / metadata summary 路径，以及 `source/rust/bridgingio-mcp` / `source/rust/bridgingio-engine` 的 target `credential_ref` 绑定语义。
- 受影响文档包括 `openspec/specs/*` 对应规范，以及 `docs/matrix/LOCAL_OPERATOR_INTERFACE_MATRIX.md` 与必要的 menuconfig 风格/接口矩阵文档。
- 对现有 CLI `vault import` 兼容路径无破坏性迁移要求；该 CLI 仍可作为 headless / automation 路径存在，但 `menuconfig` 将补齐 operator-facing 的正式 SSH key 管理面。
- 本变更与 `add-menuconfig-sensitive-target-management` 互补：该变更负责“SSH key 如何进入 vault 并被 target 绑定”，而不是重新定义 sensitive target 的 vault-authoritative 存储模型。
