## 0. 本次设计草案已记录

- [x] 0.1 固化 canonical vault architecture：内置加密真相层 + protector 分层，而不是继续依赖 memory shim 或把平台后端直接当作全部产品语义
- [x] 0.2 固化 canonical `CredentialRef` 形式与 legacy alias 归一化路线
- [x] 0.3 固化 secret broker 模型，明确后续不能继续以通用 plaintext `get()` 作为主接口
- [x] 0.4 固化 SSH 私钥交付优先级：临时 ssh-agent broker 优先，临时 identity file 仅作受控 degraded fallback
- [x] 0.5 固化 model-plane agent auth 路线：用户签发的 opaque bearer token、hash-only 存储、principal 从 token 派生
- [x] 0.6 固化 passkey 的定位：本地高风险管理动作的用户验证层，而不是 headless agent 的默认数据面认证
- [x] 0.7 记录 standalone future route：配置声明 + 独立 vault/auth 管理入口；本次不实现 standalone 管理命令
- [x] 0.8 补充对象级存储边界、显示安全投影与核心状态机，收紧后续实现边界
- [x] 0.9 补充 vault root key / per-version DEK / protector rewrap 设计，以及 SSH broker endpoint / cleanup / structured exec 前提
- [x] 0.10 补充 passphrase protector、Argon2id 与 backup/migration 的安全边界
- [x] 0.11 补充 vault unlock trigger policy 与 method policy，明确启动锁定和 passkey/passphrase 角色边界

## 1. Vault 与引用模型

- [x] 1.1 统一 `CredentialRef` canonical URI 规范，并定义 legacy `vault:...` 到 canonical `vault://...` 的归一化策略
- [x] 1.2 设计 vault secret metadata 模型，至少覆盖 kind、label、version、status、created_by、rotation 与 audit 维度
- [x] 1.3 设计内置加密 vault 的 master key、DEK、ciphertext blob 与 metadata 分层
- [x] 1.4 定义 `os-native`、passphrase 与未来其他 protector 的装配方式与 fail-closed 语义
- [x] 1.5 为 vault backend / protector 暴露 readiness、degraded、unsupported 诊断
- [x] 1.6 明确 `VaultSecretRecord` 与 `VaultSecretVersionRecord` 的字段、状态机与 rotation 切换规则

## 2. Secret Broker 与防泄露治理

- [x] 2.1 用用途受限的 broker API 替换通用 plaintext `get()` 主路径
- [x] 2.2 设计 reveal / export 这类本地管理员动作的专用审批与 user verification 语义
- [x] 2.3 设计 runtime redaction registry，使日志、artifact、stderr 捕获共享同一套 secret 脱敏真相源
- [x] 2.4 收敛当前会记录原始命令字符串的日志路径，避免 secret-aware execution 落地后仍从 logger 重新泄露
- [x] 2.5 定义 secret 在内存中的专用容器、零化和禁止 `Debug` / `Clone` / `Serialize` 规则

## 3. SSH 私钥交付链路

- [x] 3.1 设计 `ssh-agent broker` 抽象，明确 session 生命周期、endpoint 类型与清理规则
- [x] 3.2 验证 macOS / Windows / OpenHarmony PC 上 agent-compatible delivery 与 OpenSSH 的兼容语义
- [x] 3.3 为无 agent-compatible 路径的平台定义 `ephemeral identity file` degraded fallback 规则与诊断输出
- [x] 3.4 将 SSH 私钥交付与 structured execution 改造挂钩，避免继续依赖 host shell flatten 命令字符串
- [x] 3.5 定义 SSH key passphrase、host key policy 与 broker 之间的交互语义

## 4. Model-Plane Agent Auth

- [x] 4.1 为 model-plane 定义 `auth_mode` 扩展语义，至少覆盖 `none`、`bearer` 与未来扩展位
- [x] 4.2 设计 opaque bearer token 的生成、hash 存储、scope、revocation、idle timeout 与 optional expiry
- [x] 4.3 把 authenticated principal 纳入 access scope、session isolation 与 audit event，而不是信任请求体自报的 `agent_id`
- [x] 4.4 为 loopback 与 non-loopback 暴露统一 auth enforcement 语义，避免把 loopback 本身当作可信身份边界
- [x] 4.5 定义长期 token 与短期 run token 的关系，以及是否允许受控 delegation
- [x] 4.6 固化多维 scope 矩阵，至少覆盖 target、tool、risk_envelope、interactive shell、artifact 与 delegation 维度
- [x] 4.7 定义内置 scope profile 或等价推荐模板，例如 `read-only`、`interactive-read`、`operator` 与 `admin`
- [x] 4.8 定义 “AuthN -> AuthZ(scope) -> Policy -> Approval -> Execution” 的统一判定顺序，并要求错误归因可审计
- [x] 4.9 为现有 MCP tool catalog 建立最小 scope profile 映射表，并把 interactive shell / artifact 的资源归属约束纳入授权真相源
- [x] 4.10 为终端类 tool 定义命令风险分类与 token risk_envelope 的结合规则，避免 tool 级权限被误当成无限制 shell 权限
- [x] 4.11 明确 `AgentTokenRecord` 与 `TokenScopeRecord` 的字段、hash 方案、delegation lineage 与 revoke 语义

## 5. Passkey 与本地用户验证

- [x] 5.1 定义 passkey / platform authenticator 在 token 签发、vault 解锁、secret reveal、审批等场景中的使用边界
- [x] 5.2 设计本地用户验证抽象，允许 passkey、平台用户验证与 headless fallback 共存
- [x] 5.3 明确哪些动作必须强制 user verification，哪些动作允许仅凭现有登录态或本地管理员会话完成
- [x] 5.4 设计 passkey credential metadata 存储结构与 rotation / revoke 语义
- [x] 5.5 设计 local admin attestation / intent 绑定模型，确保用户验证结果短时效、不可被远端 agent 重放
- [x] 5.6 为 user verification 定义 freshness window 与动作分级矩阵，并明确哪些动作必须 fresh UV
- [x] 5.7 明确 `LocalAdminActionIntent` 与 `LocalAdminAttestationRecord` 的字段、状态流转与一次性消费规则

## 6. Standalone Follow-Up（本次不实现）

- [x] 6.1 细化 standalone 配置中的 `[vault]`、`[vault.unlock]`、`[vault.ssh]` 与 `model_plane.http.auth` 语义
- [x] 6.2 设计 standalone 的 `vault init/import/unlock` 与 `auth token create/revoke` 管理入口
- [x] 6.3 明确 `stdin`、`file`、`fd`、`tty prompt` 几类 secret 输入路线的优先级与审计语义
- [x] 6.4 明确哪些 `run` 级参数允许补充，哪些明文 secret argv 形式必须永久禁止
- [x] 6.5 为内网 / headless 部署整理操作手册与安全默认值建议

## 7. Self-Test 与 Contract Smoke

- [x] 7.1 补充 `quality-and-test-automation` 与 `target-session-management` 规格，明确 `bridgingio-core --self-test` 必须吸收本次提案中 contract-critical 的 vault / auth smoke 范围
- [x] 7.2 扩展 `bridgingio-core --self-test`，覆盖 canonical `CredentialRef`、fail-closed、broker-only secret use、本地管理员验证、SSH secret delivery 与 non-loopback 安全默认值
- [x] 7.3 更新 core contract 文档，使 `--self-test` 的 vault / auth 覆盖范围与步骤说明保持一致
