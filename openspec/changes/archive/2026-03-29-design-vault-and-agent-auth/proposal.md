## 为什么

BridgingIO 现在已经把 `vault abstraction`、`CredentialRef`、模型平面监听配置和审批策略这些边界搭出来了，但真正的安全真相层还没有落地。当前仓库里仍然存在几个会阻塞后续安全能力的问题：

- `os-native` vault 仍然是显式的 degraded memory shim，而不是真实平台安全存储
- MCP / model-plane 认证还停留在配置位，尚未形成真正的 agent 访问控制链路
- SSH connector 目前仍以普通 `ssh` 命令拼装和 host shell 字符串执行为主，不适合直接承载 vault 私钥注入
- loopback 监听并不能天然代表“只有被授权 agent 可访问”，当前本地端口访问仍可能直接触达已配置敏感 target
- standalone 未来必然需要支持内网 / headless 场景下的 vault 初始化、解锁、导入和 token 管理路线，但这些动作不能通过配置文件或命令行明文参数完成

随着后续要把用户 SSH 私钥、第三方认证 token，以及 model-plane 的 agent access token 都纳入统一安全模型，BridgingIO 需要先形成一套稳定的安全设计真相源，避免后续在 UI、core、connector、standalone CLI 各自补出互相冲突的局部方案。

## 变更内容

- 定义 canonical vault architecture：以内置加密 vault 作为敏感数据真相层，并允许 `os-native`、passphrase 或未来其他 protector 保护主密钥；继续保留稳定的 `CredentialRef` 抽象
- 统一凭据引用语义，约定 canonical `vault://...` 形式，并把历史 `vault:...` 风格纳入归一化兼容范围
- 定义 secret broker 模型：敏感值默认通过用途受限的 broker 使用，而不是面向 model-facing / provider-facing 路径暴露通用 `get() -> plaintext`
- 明确 SSH 私钥交付路径：优先使用临时 `ssh-agent` broker 或等价 agent-compatible delivery；仅在必要时允许显式 degraded fallback
- 定义 model-plane agent auth：用户显式签发、可撤销、可限权、可长期或时效的 agent token；认证后的 principal 必须从 token 派生，而不是信任请求体自报身份
- 把 passkey 纳入本地用户验证设计，用于 token 签发、vault 解锁、secret reveal、权限提升和高风险审批等管理动作
- 为 standalone 记录后续路线：配置文件只声明 vault / auth 策略，不保存明文 secret；未来通过独立 vault / auth 管理入口完成导入、解锁和 token 管理。本次变更只记录设计与规格，不要求立即实现 standalone 管理命令
- 为 `bridgingio-core --self-test` 补充 contract-critical 的 vault / auth smoke 范围，使 canonical `CredentialRef`、fail-closed、broker-only secret use、本地管理员验证、SSH secret delivery 与 non-loopback 安全默认值拥有统一自测入口

## 功能 (Capabilities)

### 修改功能

- `credential-and-approval-control`: 增加 canonical vault、secret broker、SSH 私钥安全交付、本地用户验证 / passkey 兼容语义
- `capability-aware-mcp`: 增加 model-plane bearer token 认证、agent principal / scope 授权，以及“请求身份必须从凭证派生”的要求
- `target-session-management`: 增加 standalone 模式下的 vault / unlock / token 管理路线约束，但 standalone 专用管理入口本次仅做规格记录
- `quality-and-test-automation`: 增加 vault / auth contract-critical smoke 必须进入 `--self-test` 的要求

### 新增功能

无。

## 影响

- Rust 核心将重点影响 `source/rust/bridgingio-secrets`、`source/rust/bridgingio-mcp`、`source/rust/bridgingio-connectors`、`source/rust/bridgingio-engine`、`source/rust/bridgingio-platform`
- 当前 `ssh` connector 与 host shell flatten 执行路径需要为后续 secret-aware structured execution 让路，否则无法安全承载 vault 私钥交付
- model-plane 的 HTTP 认证、token 生命周期和授权边界需要与 access scope、approval policy、audit event 统一建模
- 本地 UI 与未来 standalone CLI 都需要接入同一套 vault / auth 管理真相源，避免出现“UI 可以签发、CLI 不能撤销”这类分叉
- standalone 相关实现将是后续工作流，本次变更只先把路线、约束和禁止事项写进设计与 specs，避免未来再走明文参数或临时文件常驻等危险路径
- `bridgingio-core --self-test` 与 contract 文档需要同步吸收本次已落地的 vault / auth contract smoke，避免 proposal 中的关键安全边界只停留在单元测试或文档层面
