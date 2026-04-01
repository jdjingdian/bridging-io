## 1. Canonical Vault Runtime

- [x] 1.1 在 `bridgingio-secrets` 中引入真正持久化的 vault store，拆分 metadata db 与 ciphertext/wrap blob store
- [x] 1.2 落地 `VaultSecretRecord`、`VaultSecretVersionRecord`、`VaultKeyEnvelopeRecord`、`ProtectorWrapManifest` 的持久化 schema 与 format version
- [x] 1.3 用正式 AEAD envelope 取代当前的内存态 XOR 占位格式，固定 `VRK -> DEK -> ciphertext` 分层
- [x] 1.4 为 canonical vault 写入 format migration 与 legacy 初始化路径，避免未来 schema 升级无路可走

## 2. Config Canonicalization And Migration

- [x] 2.1 扩展 `bridgingio-engine` 的 `[vault]` 配置解析，支持 `[vault.unlock]`、`[vault.protectors.*]`、`[vault.ssh]`
- [x] 2.2 将 legacy `[vault] backend = "os-native"` 读取兼容为 canonical `builtin-encrypted + os-native primary protector`
- [x] 2.3 将 legacy `vault:*` 引用规范化为 canonical `vault://...`，并在设置持久化时统一回写 canonical 形式
- [x] 2.4 为 standalone 示例、fixture 与文档更新 canonical vault 配置样例

## 3. Protector And Unlock Policy

- [x] 3.1 实现 `passphrase` protector，使用 `Argon2id`、随机 salt 和版本化 KDF 参数
- [x] 3.2 为 vault 引入真实 `uninitialized/locked/unlocking/unlocked/unavailable` 状态机
- [x] 3.3 实现 `trigger_policy = on-core-start / on-first-secret-access / on-every-secret-access / manual-only`
- [x] 3.4 实现 unlock cache TTL、显式 lock/shutdown 清理和 fail-closed gate
- [x] 3.5 为 `os-native` 保留真实 protector 接口与 diagnostics，禁止继续把 memory shim 伪装成 ready

## 4. Trusted Local Admin Verification

- [x] 4.1 将 `LocalAdminActionIntent` 与 `LocalAdminAttestationRecord` 持久化到 canonical metadata store
- [x] 4.2 为 trusted control-plane 增加 `create_local_admin_intent` 与 `complete_local_admin_attestation` 命令
- [x] 4.3 在 `unlock_vault`、`create_agent_token`、`update_agent_token_scope`、`reveal/export secret` 路径中强制校验 attestation
- [x] 4.4 拒绝 synthetic / placeholder attestation id，并补充 payload digest、principal、freshness 和 single-use enforcement

## 5. Token Authority Productization

- [x] 5.1 将 `AgentTokenRecord` 与 `TokenScopeRecord` 持久化，并为 scope 变更建立 supersede 版本语义
- [x] 5.2 将长期 token 明文限制为“一次性显示”，所有后续查询只返回 summary
- [x] 5.3 将长期 token 签发与 scope 扩大接入真实本地验证；revoke 继续在 trusted control-plane 下执行
- [x] 5.4 为后续 delegated run token 预留 lineage schema 与 revoke 传播语义

## 6. Vault Management Control Plane

- [x] 6.1 在 `bridgingio-app-api` 与 `bridgingio-mcp` 中加入正式的 vault 管理命令与响应类型
- [x] 6.2 提供 display-safe vault state projection，至少覆盖 lock state、protector summary、unlock policy summary 和 secret summary 列表
- [x] 6.3 将 desktop 与 standalone 都接到同一套 vault/token 管理 handler，而不是各自维护独立逻辑

## 7. Desktop UI Integration

- [x] 7.1 将 Tauri `unlock_vault` 从 deferred 占位升级为正式 trusted command
- [x] 7.2 将 Settings > Vault 改为展示真实 lock/protector 状态，而不是只显示 backend 标签
- [x] 7.3 将长期 token 签发从固定 `attestation_id` 占位改为真实 intent/attestation 握手
- [x] 7.4 保留 one-time token reveal UI，但其数据源必须来自真实 runtime-enforced issuance

## 8. SSH Secret Delivery Hardening

- [x] 8.1 将 secret-backed SSH 路径接入 vault unlock gate，禁止在 locked/unavailable 状态下继续执行
- [x] 8.2 将现有 SSH broker session 与 persisted vault truth 绑定，明确 lifecycle、cleanup 和 diagnostics
- [x] 8.3 推进 connector 走 structured exec overlay，而不是继续依赖 host shell flatten 注入 broker 参数
- [x] 8.4 保留 identity-file fallback，但要求显式 degraded diagnostics、私有运行目录和连接级清理

## 9. Native Protector Rollout

- [x] 9.1 为 macOS 落地真实 `os-native` protector binding
- [x] 9.2 为 Windows 落地真实 `os-native` protector binding
- [x] 9.3 为 OpenHarmony PC 落地真实 `os-native` protector binding 或明确 unsupported/fallback 语义（跳过：当前暂无 OpenHarmony PC 设备）
- [x] 9.4 将 capability health 的 vault readiness 切换到真实实现结果

## 10. Standalone Management Surface

- [x] 10.1 为 `bridgingio-core` 设计并实现 `vault init/import/unlock` 与 `auth token create/revoke` 管理入口
- [x] 10.2 固化 `fd > stdin > file > tty-prompt` 的 secret 输入优先级与冲突处理
- [x] 10.3 为 standalone 管理动作补充最小审计语义，记录 source_kind、intent_id 与来源摘要而不泄露明文

## 11. Tests And Documentation

- [x] 11.1 为 canonical vault persistence、locked fail-closed、passphrase unlock 和 config canonicalization 补充单元测试与集成测试
- [x] 11.2 扩展 `bridgingio-core --self-test`，吸收 attestation enforcement、single-use、SSH broker lifecycle 和 canonical migration smoke
- [x] 11.3 为 desktop UI contract test 增加 locked state、unlock flow、token issue/revoke 和 one-time reveal 场景
- [x] 11.4 更新开发文档与 operator 手册，明确 canonical vault、protector、trusted control-plane 和 standalone 管理路线

## 12. Target Catalog Layering And Anonymous Compatibility

- [x] 12.1 将 target 数据模型拆分为 public descriptor 与 resolved profile，增加 `storage_class` / `access_class` 与 `sealed_profile_ref` 语义，并为 legacy target 默认映射 `plain + anonymous-local`
- [x] 12.2 在 `bridgingio-mcp` 中实现 loopback 匿名兼容 principal，允许无 token 请求执行 plain target，同时禁止 non-loopback 请求与 sealed target 回退到匿名路径
- [x] 12.3 为 locked/unlocked 两种 vault 状态补齐 target catalog 投影：未解锁时对 sealed target 返回 redacted descriptor，解锁后再解析 overlay 并校验 public descriptor digest
- [x] 12.4 补充兼容与安全测试，覆盖 legacy plain target 无 token 直连、显式无效 token 不回退匿名、sealed target 未解锁拒绝、descriptor tamper diagnostics
