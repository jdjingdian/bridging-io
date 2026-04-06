## 为什么

当前 vault-managed SSH key 的“broker ready”只停留在 endpoint 路径分配与状态标记层，并没有真正拉起可被 OpenSSH 消费的本地 agent-compatible 服务。这导致 menuconfig `Test Connection`、MCP one-shot SSH 与后续 interactive target session 在使用 `vault://.../ssh-private-key/...` 时会命中 `IdentityAgent` 未就绪、错误回退或环境误分类，破坏了既有规范要求的“通过 broker 使用 secret、避免泄露到 argv / 日志 / artifact / 配置”的安全边界。

现在需要把 SSH broker 落成一个正式的跨平台运行时能力：在 Unix 与 Windows 上都以真实的连接级本地 endpoint 承载 signer 服务，确保 vault 中的 SSH 私钥能够被安全使用而不被重新暴露；同时补齐 direct identity 路径，使手工引用本地 key 文件的 SSH target 不再错误依赖 vault broker。

## 变更内容

- 实现跨平台 SSH broker runtime，使 vault-managed SSH key 在 Unix / Windows 上都通过真实的连接级本地 endpoint 提供给 OpenSSH 或等价 SSH 客户端，而不是只生成 endpoint hint。
- 把 broker session 的 `prepare / attach / detach / close / cleanup` 从元数据状态流升级为真实的 endpoint 生命周期与 signer 资源生命周期。
- 为 broker 定义最小可用协议面与 readiness 合同，确保只有在本地 endpoint 已实际 bind/listen 并可接受请求后才可进入 `ready`。
- 规范 direct identity 路径：当 SSH target 直接引用本地 identity 文件而不是 vault credential 时，系统必须走显式 `-i` / `IdentityFile` 语义，不创建 broker session，也不把该路径误归类为 broker 故障。
- 收紧 secret-backed SSH 的环境与日志边界，避免 ambient `SSH_AUTH_SOCK`、shell flatten、fallback 误用或普通错误分类掩盖真正的 broker / secret delivery 状态。
- 增加跨平台自测、集成测试与 menuconfig / runtime 诊断合同，覆盖 broker readiness、cleanup、安全退化与 direct identity bypass。

## 功能 (Capabilities)

### 新增功能

- 无

### 修改功能

- `credential-and-approval-control`: 细化 vault-managed SSH key 的运行时交付合同，要求 broker readiness 对应真实 agent-compatible endpoint，并明确 direct identity 与 broker-only secret use 的边界。
- `target-session-management`: 细化连接级 SSH broker 的状态机、跨平台 endpoint 语义、结构化注入方式与自动清理行为。
- `standalone-operator-console`: 调整 menuconfig / standalone surface 对 SSH 测试连接和 direct identity 的行为合同，确保 direct identity 不依赖 broker，broker 失败分类与提示与真实运行时一致。
- `operator-interface-matrix`: 更新矩阵中 SSH credential source、broker endpoint readiness、direct identity bypass 与失败分类的操作员可见合同。
- `quality-and-test-automation`: 为 Unix / Windows broker lifecycle、degraded fallback、direct identity bypass 与安全边界增加必须覆盖的回归测试合同。

## 影响

- 受影响代码：`source/rust/bridgingio-secrets`、`source/rust/bridgingio-mcp`、`source/rust/bridgingio-engine`、`source/rust/bridgingio-operator-console`
- 受影响运行时语义：vault-managed SSH key delivery、broker session lifecycle、SSH test connection、interactive / one-shot SSH invocation、direct identity handling
- 受影响平台：Unix（Unix domain socket）与 Windows（named pipe 或等价平台本地 endpoint）
- 受影响测试：core self-test、broker lifecycle 单元/集成测试、menuconfig SSH test regression、跨平台 contract tests

## 首轮完成边界

- 本轮验收范围：Unix broker endpoint 真实 bind/listen 与 cleanup、vault/direct identity 分流、`-i`/`IdentityFile` + `IdentitiesOnly` 注入、menuconfig broker/readiness 与 direct identity 分类、display-safe 回归与 self-test 冒烟。
- 本轮不作为验收阻塞项：Windows named pipe 完整运行时（含 adapter 细节）、更完整 ssh-agent protocol 覆盖、publickey algorithm compatibility matrix 扩展、forwarding 与更细粒度 runtime metrics。
