## 为什么

当前 SSH target 的产品语义仍然围绕单一 `credential_ref` 展开：vault-managed SSH key 已具备 broker 路径，direct identity 也已有显式 `-i` 语义，但 target 登录密码、本地未加密私钥的 secure delivery、plain / sealed 两类 target 的认证组合约束、以及 “SSH 安全访问” 这一面向 operator 的统一心智都还没有被正式建模。

这已经开始阻碍后续产品化。一方面，MCP 与 menuconfig 都需要“应用内部闭环”的认证交付，不能依赖运行时再向用户提示输入密码或 key passphrase；另一方面，plain target 又需要保留一条刻意放行的低门槛路径，允许偷懒用户在 `config.toml` 中保存明文 password 或本地 key 路径，但同时必须通过强提示与清晰合同告诉用户何时应该升级到 vault / sealed / broker 路径。现在需要一次成体系的 change，把 SSH auth matrix、创建交互、运行时交付与测试合同一起收敛。

## 变更内容

- 为 SSH target 正式定义认证矩阵，而不再继续只靠 `credential_ref` 隐式推断：至少覆盖 `none`、target 登录 `password`、本地未加密 SSH 私钥、vault-managed SSH 私钥，以及“本地加密 SSH 私钥必须先导入 vault”的约束。
- 为 operator surface 引入统一的产品语义 `SSH 安全访问`：对 plain SSH target，该开关默认开启但允许关闭；对 sealed SSH target，只要当前认证类型涉及 secret material，就必须强制启用且不可关闭。
- 明确 plain / sealed 两类 target 的允许组合：
  - plain：允许 `none`、明文 `password`、本地未加密私钥；其中 `SSH 安全访问` 默认开启但允许关闭，并继续保留强风险提示。
  - sealed：允许 `none`、password、本地未加密私钥、vault-managed 私钥；若私钥文件带 passphrase，则必须先导入 vault，sealed 路径强制 `SSH 安全访问`。
- 重构 SSH target 的创建流程，把认证选择前置为正式步骤：`storage mode -> target type -> SSH authentication setup -> detail editor`。在本地私钥路径录入时，系统必须立即检测私钥是否带 passphrase，并在不允许的组合上阻止继续创建。
- 扩展 runtime 交付模型，使 target 登录 password 也拥有正式的 secret-backed secure delivery 语义，而不是只在研发 preflight 脚本中存在 ad-hoc 方案。该能力必须保证 password 不通过 cmdline 暴露，并可被 MCP / menuconfig / future interactive runtime 复用。
- 继续保持 direct / secure-local / vault-broker 的错误分类与 display-safe 语义分流，避免 plain password、local key secure delivery、vault key broker 与 direct identity 的失败被混淆。

## 功能 (Capabilities)

### 新增功能

- 无

### 修改功能

- `credential-and-approval-control`: 扩展 SSH 认证材料与 secure delivery 的正式边界，覆盖 target 登录 password、plain 明文放行、sealed 强制安全访问、本地加密私钥必须先导入 vault，以及 `SSH 安全访问` 的产品语义。
- `standalone-operator-console`: 重构 SSH target 创建与编辑交互，新增 `SSH Authentication Setup` / `SSH 安全访问` 条件显示规则，并把 plain / sealed 约束与风险提示产品化。
- `target-session-management`: 为 target 登录 password 增加正式的 secret-backed delivery 语义，并统一 direct / secure-local / vault-broker 三类 SSH 交付计划与生命周期。
- `operator-interface-matrix`: 记录新的 SSH 创建流、plain / sealed 不同状态下的 `SSH 安全访问` 可见性、条件显示、强制语义与输入输出合同。
- `menuconfig-style-matrix`: 记录 `SSH 安全访问`、认证类型选择与强制只读状态在 menuconfig 中采用的条目语法与交互规则。
- `quality-and-test-automation`: 增加 SSH auth matrix 的自动化覆盖，验证 plain / sealed、password / local key / vault key、secure delivery / direct fallback、以及非法组合阻断行为。
- `core-error-and-status-contract`: 补齐 SSH 认证矩阵新增失败语义，例如“不允许的 auth/storage 组合”、“本地私钥带 passphrase 但未导入 vault”、“password secure delivery unavailable”等结构化错误分类。

## 影响

- 受影响代码将覆盖 `source/rust/bridgingio-engine` 的 target 配置模型、`source/rust/bridgingio-domain` 的 target profile 语义、`source/rust/bridgingio-operator-console` 的 SSH target 创建/编辑流、`source/rust/bridgingio-mcp` 的 structured SSH invocation 准备逻辑、`source/rust/bridgingio-secrets` 的 password broker / lease 扩展，以及 `source/rust/bridgingio-providers` 对 structured invocation carrier 的支持能力。
- 受影响文档包括对应 OpenSpec 增量规范，以及 `docs/matrix/LOCAL_OPERATOR_INTERFACE_MATRIX.md` 与 `docs/matrix/MENUCONFIG_STYLE_MATRIX.md`。
- 该变更会显式保留 plain target 的“低门槛但高风险”路径，因此不会强制所有用户立即迁移到 vault；但它会把这条放行路径收敛为受控、可提示、可验证的正式语义。
- 该变更不直接实现桌面端新 UI，但会为未来 desktop/operator host 复用同一 SSH auth matrix 与 secure delivery 合同打基础。
