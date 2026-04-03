## 为什么

当前 `menuconfig` 的 Token 管理页把“查看 token”“编辑备注”“撤销”“删除”堆叠在同一层列表里，操作员进入页面后很难快速判断每个 token 的生命周期、可执行动作和下一步入口，整体交互不符合列表浏览再进入详情管理的直觉。与此同时，token 别名缺少明确的输入约束、token 缺少可临时关闭的 `disabled` 管理状态、未撤销 token 仍暴露删除路径，已经导致 `delete token failed: AgentTokenRejected("token delete requires token status revoked")` 这类运行时失败直接泄漏到管理流程中。

现在需要把 Token 管理页收敛成“列表页 + 统一详情页”的稳定心智模型，并把 token 的启停、撤销、删除前置条件和 MCP 认证失败语义一起补全，否则后续 scope 管理、profile 授权接入和本地运维都会继续建立在容易误操作的页面结构上。

## 变更内容

- 将 `menuconfig` 的 Token 管理入口重构为列表页：每一行必须展示稳定序号、token 别名、有效期摘要（长期或有效日期）、生命周期状态（有效 / 过期 / 撤销 / 禁用）以及进入详情页的导航入口。
- 将单个 token 的管理动作收敛到统一详情页：展示别名、启用开关、display-safe 的 token 指纹/摘要、有效期、权限管理入口，以及根据状态裁剪的 `撤销` / `删除` 动作。
- 为 token 引入可逆的 `disabled` 状态，用于在不进入 `revoked` 终态的前提下临时关闭访问；MCP / model-plane 认证失败必须能区分 `revoked`、`disabled`、`expired` 等原因，而不是继续压成单一的无效 token 错误。
- 为 token 别名补充正式校验合同：禁止空白或仅空格输入，并明确允许的连接符语义（至少覆盖 `-`、`_`），以便 TUI 与 core 侧保持一致验证。
- 在 `menuconfig` 中把删除动作改为显式前置保护：未撤销 token 不显示 `删除` 入口；即使状态陈旧或外部并发变更，也必须在本地管理面先给出可理解的阻止反馈，而不是让运行时错误直接冒泡成启动/操作失败。

## 功能 (Capabilities)

### 新增功能

- 无

### 修改功能

- `standalone-operator-console`: 重构 `menuconfig` 的 Token 管理信息架构、列表展示、统一详情页、权限管理入口、别名输入校验提示与删除前置保护。
- `credential-and-approval-control`: 扩展长期 token 生命周期，加入 `disabled` 的本地管理语义、alias 校验约束，以及删除前本地门控要求。
- `capability-aware-mcp`: 细化 bearer token 认证拒绝原因，要求对 `disabled`、`revoked`、`expired` 等状态提供可区分的认证结果。
- `operator-interface-matrix`: 更新 Token 管理相关接口矩阵，记录列表页/详情页动作、启用开关、删除可见条件与认证错误语义。
- `core-error-and-status-contract`: 补充 token 认证失败在公共错误封装中的稳定子码要求，避免 UI/MCP 只能依赖 message 文本分辨 `disabled` 与 `revoked`。

## 影响

- 受影响代码主要位于 `source/rust/bridgingio-operator-console`、`source/rust/bridgingio-secrets`、`source/rust/bridgingio-mcp` 与对应 i18n 文案。
- 需要补充 token 生命周期、menuconfig 交互和 MCP 认证失败分类的自动化测试。
- 会修改现有 OpenSpec 能力文档与 operator interface matrix，但不引入新的外部依赖。
