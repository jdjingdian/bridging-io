## ADDED Requirements

### 需求:model-plane HTTP 必须支持 agent 认证并将其应用于 loopback 与非 loopback 监听
BridgingIO 的 model-plane HTTP 必须支持面向 agent 的正式认证机制，例如 bearer token，并且一旦启用该认证模式，系统必须在 loopback 与非 loopback 监听上一致执行认证校验。系统不得因为监听地址是 `127.0.0.1` 就绕过已配置的认证约束。

#### 场景:loopback 模式下缺失 token
- **当** model-plane 监听在 `127.0.0.1`，且当前实例启用了 bearer 或等价 agent 认证
- **那么** 未携带有效凭证的客户端请求必须被拒绝，而不能因为它来自 loopback 就被视为自动可信

#### 场景:non-loopback 模式下启用认证
- **当** 操作员把 model-plane HTTP 暴露到非 loopback 地址，并启用了 agent 认证
- **那么** 系统必须对所有入站请求执行相同的凭证校验与授权流程，而不是区分“本地请求”和“远端请求”走两套身份判断

### 需求:agent access token 必须由用户显式签发并具备撤销、限权和可选时效
BridgingIO 的 agent access token 必须被建模为用户显式签发的访问凭证，而不是静态硬编码口令。系统必须支持长期 token 与时效 token 两种形式，并且至少提供 revoke、scope 限制、last-used 追踪与可选 idle timeout 语义。服务端不得依赖明文 token 持久化作为唯一真相。

#### 场景:用户创建长期 token
- **当** 用户为某个受信任 agent 创建长期访问 token
- **那么** 系统必须允许该 token 不设置绝对过期时间，但仍必须支持 revoke、scope 限制、最近使用时间追踪与等价的风险控制

#### 场景:用户创建带时效 token
- **当** 用户为某次 run 或特定自动化任务创建带 TTL 的访问 token
- **那么** 系统必须在 token 到期后自动拒绝该凭证，并允许操作员在到期前主动 revoke

### 需求:agent token 必须以稳定 metadata record 持久化，且仅保存 hash 真相
系统必须为每个已签发的 agent token 维护稳定的 metadata record，至少覆盖 token 标识、principal、label、status、scope、创建者、创建时间、最近使用时间、过期时间、idle timeout、撤销信息以及可选的 delegation lineage。服务端不得把明文 token 作为长期持久化真相。该 record 必须可映射到 `AgentTokenRecord` 与 `TokenScopeRecord` 两层对象：`AgentTokenRecord` 负责 token 生命周期与 hash-only 真相，`TokenScopeRecord` 至少覆盖 `target_ids`、`tool_ids`、`max_risk_envelope`、`allow_open_shell`、`allow_write_shell_input`、`allow_artifact_cross_principal`、`allow_delegation` 与 `allow_admin_actions`。

#### 场景:签发长期 token 后查询元数据
- **当** 用户已创建一个长期 agent token，并通过本地受信任入口查看其状态
- **那么** 系统必须能够返回该 token 的 metadata record，而无需重新展示明文 token

#### 场景:token 被 revoke 后拒绝访问
- **当** 某个 token 已被 revoke，但客户端仍继续携带旧 token 发起请求
- **那么** 系统必须根据持久化的 token metadata record 判定其已失效，并拒绝访问

#### 场景:签发 token 后保留 scope 真相字段
- **当** 系统签发一个可访问部分 target 且允许 interactive shell 的 token
- **那么** 系统必须把对应 scope 字段持久化到 `TokenScopeRecord`，并在后续授权时据此判定，而不能退化为仅按 token 存在与否放行

### 需求:token 查询与管理返回必须使用安全投影而不是内部认证真相
系统必须把 token 的内部认证真相与对本地管理面的展示结果分离。即使在本地受信任 control-plane 中，token 查询默认也只能返回安全投影，例如 `token_id`、label、scope 摘要、状态与时间戳，而不能返回明文 token、`token_hash`、内部 matcher cache、精确 network binding 或等价认证内部字段。普通 model-plane / MCP 请求不得把认证内部 record 作为调试信息回显给模型。

#### 场景:本地管理员查看 token 列表
- **当** 本地受信任 control-plane 请求查看当前 token 列表或某个 token 的状态
- **那么** 系统必须返回 display-safe summary，而不能再次展示明文 token，也不能输出 `token_hash` 等内部校验字段

#### 场景:已认证 agent 请求获取自身认证细节
- **当** 某个已认证 agent 试图通过 model-plane 或普通 MCP tool 查看自身或其他 token 的内部认证记录
- **那么** 系统必须拒绝该请求，或仅返回最小化 principal / scope 摘要，而不能回显认证内部真相

### 需求:agent token scope 必须采用多维默认拒绝模型
系统必须把 agent token scope 设计为多维授权模型，而不是单一的“是否能访问 MCP 入口”。至少必须能表达 target、tool / capability、风险边界，以及 interactive shell / artifact 等关键控制位。只要任一维度不允许，本次请求就必须在真正执行前被拒绝。

#### 场景:read-only token 访问 interactive shell
- **当** 某个 token 仅具备只读 scope，但客户端尝试打开 interactive shell 或执行需要 shell 持续上下文的高风险终端操作
- **那么** 系统必须在真正建立 channel 前拒绝该请求，而不能因为该 token 已通过认证就默认允许 interactive 行为

#### 场景:token 仅允许部分 target
- **当** 某个 token 的 scope 只允许访问 `lab-ssh-01`，但客户端尝试调用另一个 target 的 typed tool
- **那么** 系统必须返回基于 scope 的拒绝结果，而不是把该情况伪装成普通 target not found

### 需求:系统必须提供内置 scope profile 语义模板并可映射到多维 scope
系统必须提供可复用的内置 scope profile 或等价推荐模板，至少覆盖 `read-only`、`interactive-read`、`operator` 与 `admin`。这些 profile 必须可确定性地映射到多维 `TokenScopeRecord`，并保持“默认拒绝、显式放行”的授权语义。

#### 场景:按 profile 创建 read-only token
- **当** 本地管理员按 `read-only` profile 创建 token
- **那么** 该 token 的 `TokenScopeRecord` 必须禁止 interactive shell 打开与写入类风险操作，并仅允许 profile 规定的读能力

#### 场景:按 profile 创建 operator token
- **当** 本地管理员按 `operator` profile 创建 token
- **那么** 系统必须把该 profile 映射为受 target/tool/risk 约束的常规操作权限，而不是等价为无限制 admin 权限

### 需求:现有 MCP tool catalog 必须维护最小 scope profile 映射真相
系统必须为现有 MCP typed tools 维护一份最小 scope profile 映射真相源，并把 interactive shell / artifact 的资源归属约束纳入授权判定。该映射必须可审计、可演进，且不能把 terminal tool 权限误当成“任意 shell 都可执行”。

#### 场景:tool 请求触发 profile 下限校验
- **当** 已认证 token 请求调用某个 MCP typed tool
- **那么** 系统必须先根据 tool catalog 映射判定该 tool 的最小 profile，并验证 token profile 是否满足，再继续后续授权流程

#### 场景:跨 principal 资源访问缺失共享授权
- **当** token 在 target 与 tool 维度满足条件，但试图读取或操作其他 principal 的 interactive shell 或 artifact
- **那么** 系统必须因资源归属约束拒绝该请求，除非存在显式共享或更高管理权限

### 需求:interactive shell 与 artifact 访问必须默认受 principal 资源归属约束
对于 interactive shell、artifact、approval context 等运行时资源，系统必须默认把“资源归属”纳入授权判断。即使某个 token 在 target / tool 维度上具备权限，也不得自动读取、关闭、中断或派生其他 principal 创建的运行时资源，除非存在显式共享或更高管理权限。

#### 场景:读取其他 principal 的 shell transcript
- **当** 某个已认证 token 具备 `terminal.shell.read` 之类的能力，但尝试读取另一个 principal 创建的 interactive shell transcript
- **那么** 系统必须默认拒绝该请求，而不是因为它拥有相同 target scope 就自动放行

#### 场景:基于其他 principal 的 artifact 继续 refine
- **当** 某个已认证 token 尝试对另一个 principal 创建的 artifact 执行 refine
- **那么** 系统必须默认拒绝，除非该 artifact 已被显式共享或该 principal 具备更高管理权限

### 需求:终端类 tool 的授权必须结合命令风险分类
对于 `bridgingio.terminal.exec` 与 `bridgingio.terminal.shell.write` 等终端类 typed tools，系统必须在 tool scope 之外继续结合命令风险分类执行授权判断。系统不得因为 token 具备 terminal tool 访问权限，就默认允许所有 shell 命令。

#### 场景:interactive-read token 尝试执行写操作
- **当** 某个 token 具备 interactive shell 能力，但其风险边界仅允许 read-only 或 interactive-read
- **那么** 若客户端通过 `terminal.exec` 或 `terminal.shell.write` 提交写操作、删除操作或 privileged 操作，系统必须在真正执行前拒绝或进入更高一层授权 / 审批流程

#### 场景:tool 允许但风险边界不足
- **当** 某个 token 的 tool scope 允许 `bridgingio.terminal.exec`，但其风险边界不允许 `privileged`
- **那么** 系统必须拒绝该次 privileged terminal 请求，而不是把 tool 访问权限等同于无限制终端权限

### 需求:长期 token 与派生 run token 必须保持 scope 只能收窄
如果系统支持从长期 token 派生短期 run token 或等价 delegation 凭证，则子凭证的 scope 与生命周期都必须是父凭证的严格子集。系统不得允许子凭证扩大 target、tool 或风险边界，也不得允许其寿命超过父凭证。

#### 场景:长期 token 派生短期 run token
- **当** 某个具备 delegation 权限的长期 token 为一次临时 run 派生短期 token
- **那么** 新 token 的 target 范围、tool 范围、风险边界和有效期都必须不超过父 token，且 revoke 父 token 后子 token 也必须失效

### 需求:agent token 生命周期必须单向收敛并级联收权
系统必须把 agent token 设计成单向收敛的生命周期对象。token 只能从“尚未签发”进入 `active`，随后进入 `revoked` 或 `expired` 等终态；系统不得重新激活已失效 token。若 token 之间存在 delegation lineage，则父 token 失效时必须对仍处于活动态的子 token 一致施加级联收权。

#### 场景:token 到期后不得恢复使用
- **当** 某个 token 因绝对 TTL 或 idle timeout 进入 expired 状态
- **那么** 系统必须拒绝继续使用该 token，并要求重新签发新 token，而不能把旧 token 重新激活

#### 场景:父 token 收权后子 token 级联失效
- **当** 某个允许 delegation 的父 token 被 revoke 或因策略进入失效状态
- **那么** 仍处于活动态的子 token 必须级联失效，并在后续访问中被一致拒绝

### 需求:认证后的 principal 必须从凭证派生，而不是信任请求体自报身份
对 model-plane 或 MCP typed tools 的每次调用，系统都必须把 authenticated principal 视为真正的身份来源。请求体中的 `agent_id`、`run_id`、`client_session_id` 等字段只能作为调用标签或子上下文，不能单独决定权限，也不能覆盖 token 所绑定的 principal / scope。

#### 场景:请求体自报其他 agent 身份
- **当** 一个客户端携带有效 token 发起请求，但在请求体中自报不同的 `agent_id`
- **那么** 系统必须继续以 token 对应的 authenticated principal 作为权限与审计真相源，而不能因为请求体字段变化就提升或转移其身份

#### 场景:token scope 不允许访问目标
- **当** 已认证客户端尝试调用一个超出 token scope 的 target 或 tool
- **那么** 系统必须在真正执行前拒绝该请求，并将拒绝原因归因到 authenticated principal 与其 scope，而不是把该错误表述成普通 target not found

### 需求:`auth_mode=none` 必须被视为显式受限模式
`model_plane.http.auth_mode = none` 只能用于操作员显式声明的开发、测试或受控诊断场景，而不得继续被视为常规安全默认值。只要系统启用了 bearer 或更强认证模式，就必须在 loopback 与 non-loopback 监听上一致执行认证。

#### 场景:发行模式下使用默认配置启动 model-plane
- **当** 系统以常规发行模式启动 model-plane
- **那么** 推荐默认认证模式必须是 bearer 或等价正式认证模式，而不是继续默认把 `none` 视为常规工作配置

#### 场景:操作员显式启用 `auth_mode=none`
- **当** 操作员在开发或诊断场景下显式把 `auth_mode` 设置为 `none`
- **那么** 系统必须把当前实例标记为受限模式，并通过 diagnostics 或启动信息清晰提示该实例未启用正式 agent 认证

### 需求:认证、scope 授权、策略与审批必须串联生效
对于每次会触发 target、session、artifact 或 typed tool 执行的请求，系统必须按“认证 -> scope 授权 -> policy -> 审批 -> 执行”的顺序串联判断。任何上游阶段拒绝时，系统都不得继续进入后续执行阶段。拒绝结果必须带有可审计的阶段归因（例如 `authn` / `authz` / `policy` / `approval`），避免把安全拒绝伪装为一般业务错误。

#### 场景:token 允许但策略要求审批
- **当** 某个已认证 token 的 scope 允许访问某个 target 和 tool，但该操作按策略仍需人工审批
- **那么** 系统必须进入审批流程，并在审批完成前禁止实际执行，而不能因为 token 已授权就绕过审批

#### 场景:策略允许但 token scope 不允许
- **当** 某个操作在 profile / policy 层面本来可执行，但调用方 token 的 scope 不允许
- **那么** 系统必须优先因 scope 不允许而拒绝请求，而不是继续进入执行或审批阶段

#### 场景:拒绝结果的阶段归因可审计
- **当** 某个请求在授权链路中被拒绝
- **那么** 系统必须记录并返回可审计的拒绝归因阶段，以便审计事件能够区分该请求是失败在认证、scope、策略还是审批阶段
