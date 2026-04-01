## 为什么

`design-vault-and-agent-auth` 已经把 BridgingIO 的安全边界设计清楚了，但仓库当前仍停留在“语义骨架已建立、真实安全真相层未产品化”的阶段。当前存在几个直接阻塞点：

- `os-native` vault 仍然只是显式的 degraded memory shim，而不是真实平台 protector
- `builtin-encrypted` 还没有形成真正持久化的 canonical vault store；secret/version/wrap 语义主要停留在进程内模型
- 桌面端存在 `Unlock Vault` 入口，但 `unlock_vault` control-plane 命令仍是 deferred
- 长期 token 的本地验证还没有在 runtime 中被强制验证；前端仍可传入占位 attestation id
- standalone 配置还不能表达完整的 `vault.unlock` / protector 策略，legacy `os-native` 配置也没有被规范化为 canonical vault 语义
- SSH 私钥交付虽然已有 broker 骨架，但尚未与真实 unlock gate、persisted vault 和 structured exec 路线形成闭环

如果继续只补局部 UI、局部 token 接口或局部平台绑定，最终会得到多套互相冲突的安全真相源：UI 一套、standalone 一套、runtime 内存模型一套、未来 native vault 再一套。现在需要把归档设计落实为单一的实现提案，并明确分阶段交付路径。

## 变更内容

- 将 `builtin-encrypted` 落实为 canonical vault runtime/store，补齐 metadata store、ciphertext blob store、root key / per-version DEK envelope 和持久化对象模型
- 将 `passphrase` 作为第一优先级的正式 protector 实现，使用 `Argon2id` 和版本化 KDF 参数；`os-native` 作为 primary protector 集成位继续跟进
- 为 vault 引入真实的 locked/unlocked/unavailable 状态机，以及 `on-core-start`、`on-first-secret-access`、`on-every-secret-access` 等 unlock trigger policy
- 将本地高风险管理动作正式收敛到 `Intent + Attestation` 控制面，要求 `unlock_vault`、`create_agent_token`、`update_agent_token_scope`、`reveal/export secret` 等动作必须消费真实 attestation，而不是接受占位字符串
- 统一 desktop 与 standalone 的 vault / auth 管理真相源：UI、trusted control-plane 与未来 CLI 子命令调用同一套 runtime handler 与持久化对象
- 将 legacy `[vault] backend = "os-native"` 和 legacy `vault:...` 引用纳入兼容输入，但运行时与持久化回写都规范化为 canonical config 与 canonical `vault://...` 引用
- 将 target 配置拆分为 public inventory 与可选 sealed overlay：`config.toml` 继续承载 plain target 与 sealed target 的公开 descriptor，vault 负责 high-sensitivity target 的敏感连接配置与 credential 关联
- 为旧版本升级路径保留显式 loopback 匿名兼容 principal：无 token 请求在该兼容模式下仍可执行 plain target，但不得访问 sealed target，也不得把 non-loopback 或无效 token 请求降级为匿名访问
- 把 SSH secret delivery 与 canonical vault 产品化挂钩：secret-backed SSH 必须经过 unlock gate，默认通过 agent broker 路线，identity file 仅保留为显式 degraded fallback
- 为 `bridgingio-core --self-test`、control-plane contract test 和桌面 UI test 增加 vault persistence、locked fail-closed、attestation enforcement、canonical config migration 和 one-time token reveal 的验证

## 功能 (Capabilities)

### 修改功能

- `credential-and-approval-control`: 将 canonical vault、unlock policy、protector readiness、display-safe projection 和本地用户验证强制落地为 runtime 真相源
- `capability-aware-mcp`: 将长期 token 签发、scope 变更与 principal/scope enforcement 接入真实 attestation 与 shared token authority，并正式定义 loopback 匿名兼容 principal 对 plain target 的访问边界
- `target-session-management`: 将 standalone vault 配置、unlock 策略、target public descriptor / sealed overlay 分层、secret 输入路线和管理子命令统一到 canonical vault runtime
- `desktop-operator-console`: 将 Settings > Vault 从静态入口升级为真实的 lock/protector 管理界面，并要求长期 token 签发走真实本地验证握手
- `quality-and-test-automation`: 为 vault/auth 的 contract-critical 路径补齐 persistence、fail-closed、attestation single-use、config migration 和 SSH delivery smoke

### 新增功能

无。

## 影响

- 受影响的 Rust 核心模块包括 `source/rust/bridgingio-secrets`、`source/rust/bridgingio-mcp`、`source/rust/bridgingio-engine`、`source/rust/bridgingio-platform` 和 `source/rust/bridgingio-app-api`
- 受影响的桌面宿主与 UI 路径包括 `source/ui/tauri-console-web`、`source/ui/tauri-console-web/src-tauri` 和 `source/rust/bridgingio-desktop-host`
- 受影响的配置与迁移路径包括 standalone TOML、runtime settings persistence、legacy `vault:ssh-key:*` 引用和 legacy `backend = "os-native"` 读取兼容
- 受影响的安全与可观测性路径包括 token issuance/revoke、local admin verification、SSH secret delivery、runtime diagnostics、self-test 和 UI contract automation
