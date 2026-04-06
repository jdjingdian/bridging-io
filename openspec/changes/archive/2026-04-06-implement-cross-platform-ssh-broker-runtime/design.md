## 上下文

当前仓库已经具备 vault-managed SSH key 的若干前置能力：canonical `credential_ref`、`ssh-private-key` secret kind、session metadata、`prepare_ssh_agent_broker_session()`、menuconfig 的 broker preflight，以及 MCP / operator-console 的 SSH probe wiring。但现状仍停留在“为 broker session 生成一个 endpoint locator 并把 `IdentityAgent` / fallback 参数塞给 SSH”这一层，而没有真正把本地 agent-compatible signer 服务拉起来。

这使得系统在语义上已经承诺了 broker-only secret use，在实现上却还停留在 hint 阶段：

- vault-managed SSH key 的主路径会返回一个看似 ready 的 Unix socket / named pipe endpoint
- menuconfig 与 runtime 会把该 endpoint 传给 OpenSSH 或等价客户端
- 但 secrets 层并没有实际 bind / listen / serve 对应 endpoint
- 于是 UI 只能看到 `IdentityAgent` 缺失、不可访问或误分类成通用认证失败

与此同时，当前 `credential_ref` 对 direct identity 也没有形成完整运行时语义。非 `vault:` 的 `credential_ref` 可以被保存，但 probe / invocation 并不会自动转换为 `-i <path>` 或等价 `IdentityFile`。这让“手工引用本地 key 文件”的兼容路径在产品心智上存在、在执行层面却不完整，还会被 ambient `SSH_AUTH_SOCK` 或错误分类进一步干扰。

这次设计要解决的不是“再补几个状态码”，而是把 SSH broker 提升为真正的跨平台运行时能力，并继续遵守此前已经确定的安全边界：

- vault 中的私钥只能通过 broker 或受控 degraded fallback 使用，不返回明文给模型、普通工具或普通日志
- broker endpoint 必须是连接级本地资源，而不是长驻全局代理
- SSH 启动必须通过结构化执行覆盖层下发 endpoint，而不是通过 shell flatten 或全局环境污染注入
- 最小化 agent 功能并不意味着最小化安全要求；只要该最小子集还不能支撑正常 SSH publickey auth，就不能把它计为 broker 完成态

## 目标 / 非目标

**目标：**

- 为 vault-managed SSH key 实现真实可运行的跨平台 broker runtime，而不是只生成 endpoint 路径或 pipe 名称。
- 在 Unix 上通过真实 Unix domain socket 提供 agent-compatible signer 服务；在 Windows 上通过 named pipe 或受控 platform-local adapter 提供等价能力。
- 让 broker session 的 `prepare / attach / detach / close / cleanup` 直接驱动 endpoint 生命周期、in-memory signer 生命周期与清理行为。
- 为 SSH broker 定义“最小但完整可用”的协议子集，使其足以完成常规 SSH publickey auth；若不能满足该条件，只能返回 unsupported / degraded，而不能伪装成 ready。
- 规范 direct identity 路径：当 target 直接引用本地 identity 文件时，运行时必须走显式 `-i` / `IdentityFile` 语义，不创建 broker session，也不将该路径误报为 broker 故障。
- 消除 ambient `SSH_AUTH_SOCK`、shell flatten、普通 stderr 误分类与日志越界对 SSH 连接安全边界的干扰。
- 为 self-test、menuconfig、MCP 与 contract automation 建立 Unix / Windows 一致的 broker readiness 与 cleanup 验证合同。
- 明确首轮交付后的扩展方向，说明后续还需要补齐哪些能力，但不把这些扩展错误地混入首轮完成标准。

**非目标：**

- 不用自研 SSH transport 替换 OpenSSH，也不在本次重新设计整个 connector / terminal runtime。
- 不在本次把 SSH broker 做成通用 secret agent；本轮只覆盖 vault-managed SSH private key 的受控使用。
- 不把 direct identity 路径重新包装为 vault secret；它是兼容路径，不承担 broker-only secret use 合同。
- 不要求首轮就支持 ssh-agent 全量管理能力，例如 add/remove identity、agent lock/unlock、agent forwarding 管理面或动态 rekey 管理。
- 不承诺在宿主已被 root / malware 完全控制时提供绝对保密；本设计聚焦于防止正常产品路径中的 secret 再泄露。

## 决策

### 决策 1：引入 `SshBrokerRuntime` 分层，把“session 真相”与“平台 endpoint 服务”拆开

`SecretVaultRouter` 继续持有 broker session 的 authoritative metadata、vault lease、cleanup policy 与 display-safe diagnostics，但不再把“生成 endpoint locator”视为 broker 已 ready。真正的 endpoint bind / serve 由独立的 broker runtime 层负责。

推荐分层：

```text
Vault secret version
  -> SecretVaultRouter session prepare
  -> SshBrokerRuntime start(session, signer material)
  -> platform endpoint bind/listen
  -> agent-compatible request loop
  -> SSH process consumes endpoint
  -> detach / close / timeout
  -> endpoint + signer cleanup
```

推荐内部接口：

```text
SshBrokerRuntime
- start(prepared_session, signer_material) -> RunningBrokerHandle
- readiness() -> binding | ready | failed
- endpoint_locator() -> platform-scoped locator
- stop(reason)

RunningBrokerHandle
- session_id
- endpoint_kind
- endpoint_locator
- join / shutdown channel
- cleanup paths or platform handles
```

这样做的原因：

- `SecretVaultRouter` 仍然是 secret use policy 和 session contract 的真相层
- 平台差异收束在 runtime adapter，而不是扩散到 menuconfig / engine / MCP
- `prepare` 能否成功不再由“能否拼出一个路径”决定，而由“能否真正启动 endpoint 服务”决定

考虑过的替代方案：

- **继续把 runtime 逻辑塞进 `SecretVaultRouter`**：会把 vault policy、platform bind/listen、协议处理与线程生命周期搅在一起，增加跨平台复杂度。
- **在 caller 侧自行启动 broker 子线程**：会让 MCP、menuconfig、future desktop host 各自持有一套 broker startup 逻辑，破坏统一语义。

### 决策 2：把 broker readiness 定义为“endpoint 已真实可服务”，而不是“locator 已分配”

系统必须收紧 broker session 的状态机。

推荐状态机：

```text
Preparing
  -> BindingEndpoint
  -> Ready
  -> Attached
  -> Draining
  -> Closed / Failed
```

语义固定为：

- `Preparing`: 已完成 vault lease 与 session metadata 创建，但 runtime 尚未尝试 bind
- `BindingEndpoint`: 正在创建 listener / pipe server / platform-local adapter
- `Ready`: endpoint 已可接受 agent-compatible 请求，SSH 可以安全启动
- `Attached`: 至少一个 channel / invocation 正在使用该 broker
- `Draining`: 已无活动引用，等待短暂清理窗口或确认子进程退出
- `Closed`: 资源已释放，endpoint 不再可用
- `Failed`: endpoint 无法启动、协议协商失败或运行时崩溃

这意味着：

- `prepare_ssh_agent_broker_session()` 只有在 runtime 报告 endpoint 已成功 bind/listen 后才能返回 `Ready`
- 若 bind 失败，则必须返回稳定的 broker failure，而不是把一个将来一定失败的 locator 提前下发
- menuconfig 的 endpoint preflight 仍可保留，但它从“发现设计缺口”变成“防御性校验”

考虑过的替代方案：

- **保留当前 ready 语义，再靠 UI preflight 兜底**：这会让 runtime 自己持续对外撒谎，违反 contract-critical 的安全与生命周期合同。
- **把 bind 延后到 SSH 子进程已经 spawn 之后**：会制造竞态，并让 SSH 客户端更容易先看到 endpoint 缺失。

### 决策 3：定义“最小但完整可用”的 agent 协议子集；不足以完成 publickey auth 就不能算完成

首轮 broker 不做“全量 ssh-agent”，但最小子集必须足以支撑常规 SSH publickey auth。推荐首轮协议面：

- request identities
- sign request
- failure / unsupported replies
- orderly close / cleanup

首轮不做：

- add identity
- remove identity
- list extensible constraints
- agent lock / unlock
- forwarding 管理策略面

这里的关键点不是“少做功能”，而是“少做不影响核心目标的功能”。只要 OpenSSH 与等价客户端仍能基于该子集完成 vault key 的 publickey authentication，并保持 host key verification 与常规 one-shot / interactive 行为，首轮就满足完成标准；如果还做不到，就必须停留在 unsupported / degraded，而不能把缺失隐藏成 ready。

考虑过的替代方案：

- **直接做全量 agent 协议**：首轮复杂度过高，容易把核心安全路径拖慢。
- **只做 identities，不做 sign**：无法完成真正认证，只能算一个不可用 stub。

### 决策 4：Unix 与 Windows 采用统一 session contract、不同 platform transport adapter

跨平台差异必须被约束在 transport adapter 内部。

#### Unix

- endpoint kind: `unix-socket`
- endpoint path: runtime root 下的私有 session 目录，而不是裸 `/tmp/*.sock` 作为最终产品语义
- runtime root 权限必须保持私有；socket 所在目录应与 fallback identity runtime dir 一样受到权限控制
- SSH 注入方式：`-o IdentityAgent=<absolute socket path>`
- cleanup：关闭 listener、unlink socket、删除 session runtime dir

#### Windows

- endpoint kind: `named-pipe` 为正式主语义
- 若当前 SSH 客户端无法直接消费 pipe locator，则 broker runtime 必须自行提供 platform-local adapter，并仍把该差异封装在 runtime 内，而不是要求 caller 降级为明文 identity file
- cleanup：关闭 pipe server / adapter handle，并释放 session 级运行时资源
- display-safe summary 只暴露 endpoint kind，不暴露 pipe name、adapter internals 或其他内部 locator

这样做的原因：

- 保持 `target-session-management` 中“宿主平台语义”和“目标方言语义”分离
- Unix / Windows 上层调用者只看见统一的 broker session contract
- 避免把 Windows 差异扩散到 menuconfig / engine / MCP 的条件分支中

考虑过的替代方案：

- **把 Windows 直接定为长期 fallback-only**：不满足用户现在对 broker 作为核心连接能力的要求，也与现有跨平台规范方向不一致。
- **继续用 `PlatformLocalEndpoint` 作为模糊占位**：会推迟最关键的运行时合同，无法给实现与测试明确目标。

### 决策 5：direct identity 路径必须显式旁路 broker，并清理 ambient agent 干扰

当 `credential_ref` 不是 canonical vault ref，而是本地 identity 文件路径或等价 direct identity 输入时，系统必须明确走 direct identity 语义：

- probe / invocation 直接注入 `-i <path>` 或等价 `IdentityFile=<path>`
- 同时加 `IdentitiesOnly=yes`，避免 ambient agent 或默认 key 顺序污染认证结果
- 不创建 broker session
- 不把 direct identity 的错误归类为 broker failure
- secret-backed SSH 与 direct identity 两条路径都必须使用结构化 exec 覆盖层，不通过 shell flatten 注入

此外，运行时必须显式处理 ambient `SSH_AUTH_SOCK`：

- secret-backed broker 路径：只允许本次 invocation 看见当前 broker 提供的 endpoint
- direct identity 路径：应清理或覆盖 ambient `SSH_AUTH_SOCK`，避免错误命中宿主环境里的 agent

考虑过的替代方案：

- **继续让 direct identity 借用 ambient agent**：行为不可预测，也会让 operator 把 broker 故障误解为目标配置问题。
- **让 direct identity 也统一包进临时 broker**：会把兼容路径错误提升为 vault 路径，增加不必要复杂度。

### 决策 6：secret material 只驻留受控内存，broker runtime 不得把 signer material 持久化到 display-safe 或普通运行目录

对 vault-managed SSH key，broker runtime 必须只持有受控内存中的 signer material 或其等价受保护表示。以下内容禁止进入普通持久层或 display-safe diagnostics：

- private key plaintext
- decrypted signer blob
- raw sign request payload
- socket / pipe 的内部控制数据
- runtime adapter 内部握手细节

允许持有的 display-safe 元数据仅限：

- `broker_session_id`
- `credential_ref`
- `endpoint_kind`
- `state`
- `degraded`
- `created_at` / `expires_at` / `cleanup_deadline`
- display-safe failure category

受控 degraded fallback 仍然允许存在，但必须满足：

- 只在 platform-compatible broker path 当前不可用时使用
- 使用私有 runtime dir 与严格权限
- 生命周期严格绑定到当前连接或逻辑 session
- diagnostics 明确标记 `degraded`
- 不得把 degraded fallback 误记为正常 broker success

### 决策 7：MCP、menuconfig、self-test 共用同一 broker startup / direct identity 解析能力

`menuconfig` 的 SSH test connection、MCP one-shot SSH 与 future interactive SSH 都必须复用同一套准备逻辑：

- 解析 credential source 类型
- 决定是 vault broker 还是 direct identity
- 注入结构化 exec args / env overlay
- 维护 broker session lifecycle
- 产出一致的 failure category 与 diagnostics

这样做的原因：

- 避免 menuconfig 修好了、正式执行路径还停留在旧逻辑
- 让 self-test 能验证真正的生产路径，而不是一个平行测试 helper
- 让“broker readiness”成为 shared contract，而不是某个 UI 的本地补丁

### 决策 8：首轮交付后必须预留扩展面，但不把扩展目标混入完成标准

为了回应“后续还需要扩展什么”，本设计把扩展分两层记录：

**首轮完成标准内：**

- Unix socket broker 真正运行
- Windows named pipe / platform-local adapter 真正运行
- 最小协议子集可完成 publickey auth
- direct identity `-i` bypass 落地
- secret-backed / direct identity 的环境隔离与错误分类落地
- self-test 与自动化覆盖上述合同

**后续扩展项：**

- 更完整的 ssh-agent protocol coverage
- RSA / Ed25519 / future key algorithm compatibility matrix 扩展
- interactive shell 长生命周期 broker multiplexing 优化
- agent forwarding 管理策略
- future provider / desktop host 复用 broker runtime
- richer diagnostics，例如签名请求计数、attach graph、更细的 platform adapter metrics

这些扩展项必须在设计与任务中标明为“后续演进”，避免团队把首轮 broker runtime 验收拖成一个无限扩张的范围。

## 风险 / 权衡

- [Windows 客户端对 named pipe / platform-local endpoint 的兼容细节比 Unix 复杂] → 通过 transport adapter 封装平台差异，并要求自动化 contract test 明确验证当前宿主下的真实可用路径，而不是把复杂度扩散到上层调用者。
- [最小 agent 协议子集若理解不准，可能出现“设计上完成、真实 SSH 仍失败”] → 以“必须能完成正常 publickey auth”为完成标准，并在 self-test / integration workflow 中验证真实签名交互，而不是只验证 endpoint 存在。
- [broker runtime 会引入线程、listener 与 session 清理复杂度] → 通过显式状态机、统一 runtime handle 和 attach/detach/timeout cleanup 规则降低泄露与悬挂风险。
- [继续保留 degraded fallback 会让实现看上去像双路径系统] → 保持 fallback 明确标记为 degraded，并要求 ready path 优先使用真实 broker；只有 broker 不可用时才允许 fallback。
- [direct identity 兼容路径可能被误用为绕过 vault 的默认路径] → 在 operator surface 与矩阵规范中明确该路径属于兼容 / 手工引用场景，不是 vault-managed secret 的主产品路径。
- [环境隔离若处理不彻底，ambient `SSH_AUTH_SOCK` 仍可能污染结果] → 强制通过结构化 exec overlay 注入或清理相关环境变量，并把这项要求写入自动化测试合同。

## 迁移计划

1. 先在 `bridgingio-secrets` 中引入 broker runtime 抽象、session state 收紧与 platform adapter 接口，保持现有 session metadata 结构尽量兼容。
2. 落地 Unix broker runtime，并把 `prepare_ssh_agent_broker_session()` 改成真实 startup + readiness gating。
3. 落地 Windows broker runtime（named pipe 或 platform-local adapter），补齐与 Unix 对齐的 readiness / cleanup 语义。
4. 在 `bridgingio-engine` 中补 direct identity `-i` / `IdentityFile` 旁路，并把 ambient agent 隔离逻辑纳入 shared SSH invocation preparation。
5. 让 `bridgingio-mcp`、`bridgingio-operator-console`、`bridgingio-core --self-test` 全部切换到共享路径，移除“endpoint hint 即 ready”的旧假设。
6. 增加自动化覆盖后再启用新的 ready 语义作为默认合同；若某个平台适配未完成，则该平台必须返回受控 degraded / unsupported，而不是伪装为 ready。
7. 若回滚需要，允许临时恢复到显式 degraded fallback-only 模式，但必须保留正确 diagnostics，不得恢复“假 ready”行为。

## 开放问题

- Windows 当前计划采用的 SSH 客户端兼容面具体需要 `named-pipe` 直连还是 `platform-local adapter` 过渡层，仍需在实现阶段以真实 contract test 确认；但无论哪种形式，都必须由 broker runtime 自身吸收复杂度。
- 首轮最小协议子集对各类已支持 SSH key algorithm 的覆盖边界，需要在实现时与现有导入能力共同核对，确保“可导入”与“可 broker 使用”不发生语义分裂。
- future desktop host 是否直接复用本轮 broker runtime handle，还是通过更高层 trusted bridge 间接访问，可在 broker runtime 稳定后再单独设计。
