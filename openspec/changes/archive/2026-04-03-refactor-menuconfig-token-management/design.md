## 上下文

当前仓库已经为 `menuconfig` 补齐了 Security 页、Token Management 入口、token create/revoke/delete 与一次性 reveal，但现有实现仍然停留在“平铺动作列表”阶段：

- `bridgingio-operator-console` 当前用 `TokenManagementRow { token_id, label, status, expires_at }` 驱动页面，同一 token 的“编辑备注 / 撤销 / 删除”动作都直接堆在列表层，缺少统一的单 token 详情页。
- `bridgingio-secrets` 目前只校验 token `label` 非空，尚未定义别名字符集、长度与 legacy label 的兼容策略。
- token 当前只有 `active / revoked / expired / deleted` 终态语义，缺少一个可逆的“临时关闭访问”管理态。
- `authenticate_agent_token()` 目前返回 `Option<AuthenticatedAgentToken>`，导致 `bridgingio-mcp` 只能把各种认证失败压成 `invalid_or_expired_token`，无法向 MCP 调用方区分 `revoked`、`disabled`、`expired`。
- `menuconfig` 的删除流程依赖 runtime 最后一道 `revoked` 前置校验；当 UI 状态陈旧或入口门控不够严格时，错误会以运行时失败的方式冒泡，而不是在本地管理面被提前拦住。

这次变更同时触及 `bridgingio-operator-console`、`bridgingio-secrets`、`bridgingio-mcp`、i18n 文案、接口矩阵与共享错误契约，因此需要一份跨模块设计来把 UI 心智模型、token 生命周期投影和认证失败语义统一起来。

## 目标 / 非目标

**目标：**

- 把 Token Management 重构成“摘要列表 -> 单 token 详情页”的直觉式导航结构。
- 让 token 详情页稳定承载别名、启停开关、display-safe 指纹摘要、有效期、权限管理入口与状态感知动作。
- 为 token 引入可逆的本地禁用语义，同时保持 `revoked / expired / deleted` 的单向终态边界。
- 让创建和编辑 token 别名复用同一套校验合同，并兼容历史遗留的不合规别名。
- 把 bearer token 认证失败从“统一无效”改成可区分的稳定原因，以支撑 MCP 报错和自动化断言。
- 在 UI 层提前拦住“未撤销直接删除”的误操作，同时保留 runtime 末端的硬性前置条件。

**非目标：**

- 不在本次变更中交付完整的 token 权限编辑器；`权限管理 --->` 可以先落为稳定入口与受控占位反馈。
- 不改动桌面 GUI 的 token 管理信息架构；本次仅覆盖 `menuconfig` 与共享 runtime/token 语义。
- 不放宽现有的一次性 reveal、delete tombstone、TTL 过期或本地 attestation 安全边界。
- 不把 token 别名扩展为自由 Unicode 文本；如果后续需要支持中文或更宽字符集，应由独立变更重新定义合同。

## 决策

### 决策 1：Token Management 采用“两级导航”，列表只做摘要，详情页承接动作

Security 页进入 `Token Management` 后，不再直接平铺某个 token 的所有动作，而是先展示一个未删除 token 的摘要列表。每一行固定包含：

- 稳定序号：优先从 `token-000001` 这类 `token_id` 的数字后缀投影为 `(000001)`；若未来出现非标准 `token_id`，则退回稳定的 display-safe 序号生成策略。
- token 别名：直接显示 operator-facing label。
- 有效期摘要：长期 token 显示 `[长期]`；有截止时间的 token 在列表中显示本地短日期摘要，例如 `[26-04-03]`。
- 生命周期状态：按 `有效 / 过期 / 撤销 / 禁用` 投影。
- 导航指引：统一以 `--->` 进入详情页。

选中某行后进入统一的单 token 详情页，由该页面承接全部管理动作与状态说明。详情页固定包含：

- `别名 = <value> --->` 直编入口（不再额外提供独立“编辑备注”动作）
- 显式启用/禁用开关，采用 `<*>/< > 开关状态 = [启用|禁用]` 的单行投影
- display-safe 的 token 指纹/摘要，而不是原始 `token_hash`
- 有效期说明
- `权限管理 --->` 入口（仅单个箭头，不重复箭头）
- 根据状态裁剪的 `撤销 Token --->` / `删除 --->`（详情页首行已给出 Token ID，动作文案不重复附带 token_id）

这样做的原因是：列表承担“比较和浏览”，详情承担“理解和操作”，符合 `menuconfig` 里“先选对象，再管理对象”的心智模型。相比继续把编辑/撤销/删除平铺在列表层，这种结构更容易支持后续的权限管理、状态说明和确认链路。

### 决策 2：`disabled` 作为可逆访问门控单独建模，状态投影按优先级收敛

为了支持“临时关闭访问，但不进入 revoke 终态”，本次不把 `disabled` 简单塞进现有单一终态枚举，而是把它设计成独立的 operator-controlled access gate，例如 `access_enabled` 或等价布尔/元数据字段。最终对外投影状态按以下优先级收敛：

`deleted > revoked > expired > disabled > active`

这样有三个直接好处：

- token 被禁用时可以再启用，而不必重签发。
- token 在禁用期间到期后，重新启用也不会“复活”为 active。
- UI 与 MCP 可以共享同一套状态推导逻辑，而不必在 `Active+Disabled+Expired` 这类组合状态间分裂。

相比直接把 `Disabled` 加进 `AgentTokenStatus` 平铺枚举，这种做法更适合同时表达“可逆开关”和“不可逆终态”，也避免了 expired/disabled 的歧义。

### 决策 3：token 别名采用统一 ASCII 合同，并对 legacy label 只读兼容

创建与编辑 token 别名必须共享同一套输入验证规则：

- 先做 trim
- 新写入值长度限定在 1 到 64 个字符之间
- 仅允许 ASCII 字母、数字、`-`、`_`
- 禁止空格、制表符以及其他连接符号

推荐的 canonical 形状可等价表达为：

`^[A-Za-z0-9]+(?:[-_][A-Za-z0-9]+)*$`

同时，为了不破坏已存在数据：

- 旧版本留下来的不合规 label 仍然允许在列表页和详情页原样显示
- 一旦用户重新编辑或保存，就必须改成合规值

相比“只禁止空字符串”或“允许任意非空文本”，这种方案更适合 TUI 即时校验、CLI/API 一致性和后续自动化脚本命名约定。

### 决策 4：认证入口返回结构化拒绝原因，不再用 `Option` 折叠所有失败

`authenticate_agent_token()` 需要从“成功返回 token，失败返回 `None`”升级为结构化结果，例如：

- `Authenticated`
- `Rejected(Invalid)`
- `Rejected(Disabled)`
- `Rejected(Revoked)`
- `Rejected(Expired)`

`bridgingio-mcp` 在 bearer 鉴权阶段只负责消费这类结构化结果，再把它映射到共享错误契约中的稳定 `domain/common_code/module_code`。scope 拒绝继续保留在 authz 阶段，不与 authn 原因混淆。

相比“认证失败后再去 list/query 补查 token 状态”，直接在认证入口返回结构化原因可以避免：

- 未知 token 与已存在但失效 token 被错误混同
- 重复查询造成的竞态
- MCP 错误 message 继续依赖 ad hoc 字符串

### 决策 5：详情页始终保留 `权限管理 --->` 入口，但未实现时必须受控占位

用户已经把“权限管理”视为 token 详情页的一部分，因此这次重构不应再把它留在信息架构之外。详情页必须始终显示 `权限管理 --->`，以便未来把 token 可访问 profile/scope 的配置自然接入同一页面。

如果本批实现尚未交付完整的 scope 编辑器，则该入口必须进入受控占位反馈，例如：

- 显式的 `method_not_implemented` 提示
- 只读的 scope 摘要页，并提示后续版本开放编辑

系统不得因为该入口尚未完成而直接隐藏它、空白返回或崩溃。相比等未来 scope UI 准备好后再补入口，这种做法可以避免第二次信息架构迁移。

### 决策 7：访问开关交互采用“空格独占触发”，Enter 不触发切换

为了保持 `menuconfig` 中“Space 表示行内切换”的一致交互，token 详情页的访问开关必须仅允许 `Space` 触发。`Enter` 在该行仅用于提示用户使用空格，而不能改变开关状态。

这样可以避免把“确认按钮语义（Enter）”和“即时切换语义（Space）”混在同一控件上，降低误操作。

### 决策 6：删除动作由 UI 可见性前置保护，runtime 保留最终拒绝

本次不会放宽“只有 `revoked` 才能 delete”的 runtime 规则。相反，处理方式改为双层门控：

- UI 层：只有当 token 已投影为 `revoked` 时，详情页才显示 `删除 --->`
- Runtime 层：继续保留 `delete requires revoked` 的最后一道硬性校验

如果出现并发修改、陈旧缓存或其他边缘情况导致 UI 发起了非法删除请求，控制面必须把该失败翻译成受控的“前置条件未满足”反馈，而不是冒泡成未处理的启动/操作失败。

这比单独依赖 UI 隐藏，或单独依赖 runtime 报错都更稳妥。

## 风险 / 权衡

- [更严格的别名规则可能拒绝部分现有命名习惯] → 通过“legacy label 继续显示、仅在重写时收紧”的策略降低破坏性。
- [引入 `disabled` 的投影优先级会增加状态推导复杂度] → 通过单一的 summary/auth projection 函数和覆盖 `active/disabled/expired/revoked/deleted` 组合的测试矩阵缓解。
- [`权限管理 --->` 先占位可能让页面看起来像是半成品] → 通过稳定入口和明确的 `method_not_implemented`/只读提示，避免误导为“已经可配置”。
- [稳定错误子码会触及 MCP、CLI、TUI 多个调用面] → 通过共享错误契约统一命名，避免每个 surface 再各自加工一套 token 失败文案。

## 迁移计划

1. 先在 `bridgingio-secrets` 中补齐 token access gate、别名校验器、display-safe 指纹投影与结构化认证拒绝原因。
2. 在 `bridgingio-mcp` 中把 bearer 鉴权从 `invalid_or_expired_token` 改为基于共享错误契约的稳定错误映射。
3. 重构 `bridgingio-operator-console` 的 Token Management 页面：落地列表页、详情页、启停开关、删除可见性门控与权限管理占位入口。
4. 更新 operator interface matrix、错误契约文档与 i18n 文案。
5. 为 token 别名校验、禁用/启用投影、MCP 错误分类和 `menuconfig` 交互补齐测试。

## 开放问题

- 暂无。开关视觉与交互已收敛为 `<*>/< >` 且仅空格触发。
