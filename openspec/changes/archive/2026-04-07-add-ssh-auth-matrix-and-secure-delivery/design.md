## 上下文

当前仓库围绕 SSH target 已经形成了几条稳定但彼此割裂的能力链：

- vault-managed SSH 私钥已经具备 canonical `credential_ref`、broker runtime、display-safe 诊断与 menuconfig 绑定流程；
- direct identity 已经有显式 `-i` / `IdentityFile` 语义；
- plain / sensitive target 已经有正式的 `Connection Profile` / `Sensitive Overlay` 分层；
- 本地 preflight 另有一条只服务研发自测的 password 非交互路径，但它不是 target runtime 的正式产品能力。

这带来三个结构性问题。

第一，当前 SSH target 的产品心智仍然是“有没有 `credential_ref`”，而不是“当前到底是 `none` / `password` / `private-key` 哪一种认证方式”。一旦需要把 target 登录 password、本地未加密私钥、vault-managed 私钥和本地加密私钥前置区分，现有模型就不够表达。

第二，`SSH 安全访问` 作为 operator-facing 产品名并没有对应的统一运行时语义。当前系统只有 vault key broker 和 direct identity 两端，没有 password secure delivery，也没有 local unencrypted key 的 secure-local 模式。

第三，当前 structured invocation 执行模型仍然偏向 `program + args`。这对 `IdentityAgent` 足够，但对 target 登录 password 这类必须通过 `SSH_ASKPASS`、临时 helper、环境覆盖层或等价 carrier 传递的认证材料来说还不够正式。

这次设计需要同时服务几个 stakeholder：

- 本地 operator：需要一个清晰可预测的 SSH 创建与编辑流程；
- MCP / menuconfig / future interactive runtime：需要“不再依赖运行时提示用户输入秘密值”的闭环交付；
- 偷懒用户：plain target 需要保留一条刻意放行的低门槛路径；
- 安全边界：sealed / vault / broker 路径必须继续保持 display-safe 与 fail-closed。

## 目标 / 非目标

**目标：**

- 为 SSH target 引入正式的认证矩阵，覆盖 `none`、target 登录 `password`、本地未加密 SSH 私钥、vault-managed SSH 私钥，并把“本地加密 SSH 私钥必须先导入 vault”固化为正式规则。
- 为 `SSH 安全访问` 定义清晰的产品语义：它表示认证材料是否通过受控 delivery plan 交付给 SSH，而不是认证类型本身。
- 允许 plain SSH target 显式放行 password 明文存储在 `config.toml`，但必须有强风险确认、明确的条件边界和后续升级路径。
- 对 sealed SSH target 强制 `SSH 安全访问`，并让其支持 password、本地未加密私钥和 vault-managed 私钥。
- 将 SSH target 的创建流重构为 `storage mode -> target type -> SSH Authentication Setup -> detail editor`，并在用户输入本地私钥路径时立即检测“是否带 passphrase”。
- 扩展 runtime，使 target 登录 password 也可以在应用内部闭环交付，不通过 cmdline 泄露，并可被 MCP、menuconfig 和 future interactive runtime 复用。
- 让 plain / sealed、direct / secure-local / vault-broker、password / key 这些组合拥有稳定的错误分类与自动化测试合同。

**非目标：**

- 不在本次把所有 plain target 都强制迁移到 vault；plain 明文 password 放行是故意保留的兼容路径。
- 不在本次支持“本地带 passphrase 的私钥在运行时直接输入 passphrase 并继续给 MCP 使用”；这类私钥必须先导入 vault。
- 不以 `sshpass`、`expect` 或 shell flatten 脚本作为新的正式主架构；如需兼容旧工具，只能是受控 fallback，而不是产品主路径。
- 不在本次新增独立的通用 `ssh-password` secret family 管理界面；sealed password 先作为 target-scoped secret 处理即可。
- 不在本次重新设计整个 target 列表、desktop UI 或 control-plane 体系；本次只收敛 SSH auth 与 secure delivery。

## 决策

### 决策 1：引入 typed `SshAuthConfig`，把“认证类型”从 `credential_ref` 推断中解耦

SSH target 的 authoritative 认证真相不再只靠 `credential_ref` 隐式推断，而是引入一层显式的 typed auth model：

```text
SshAuthConfig
- kind = none | password | private-key
- secure_access = true | false
- password = <optional plaintext, plain-only>
- private_key_source = local-path | vault-ref
- credential_ref = <local path or canonical vault ref, key-only>
```

语义约束：

- `kind = none` 时，不存在 password / key material；
- `kind = password` 时，`credential_ref` 为空，认证材料来自 `password` 或 future secret carrier；
- `kind = private-key` 时，`credential_ref` 承担 locator 角色：
  - `private_key_source = local-path` 时是本地路径；
  - `private_key_source = vault-ref` 时是 canonical `vault://.../ssh-private-key/...`。

为了降低迁移风险，legacy 配置仍允许读取旧的 top-level `credential_ref`，但新的内存真相、校验逻辑和最终写回必须基于 `SshAuthConfig`。旧配置在保存时会被规范化重写到新模型。

考虑过的替代方案：

- **继续只用 `credential_ref` + 若干 metadata 布尔值**：会让 password、none、vault key、本地 key 的组合继续模糊，后续再扩展时仍然会漂移。
- **把 password 和 key 都抽象为通用 vault secret 引用**：长期可能可行，但会把本次 plain 放行路径、本地未加密 key 支持和 sealed password 一起推高复杂度。

### 决策 2：`SSH 安全访问` 表示 delivery mode，而不是 auth kind 或存储等级

`SSH 安全访问` 的正式含义固定为：当前认证材料是否通过受控的 managed delivery plan 注入给 SSH，而不是通过 direct path 使用。

它不是“是否使用 vault”，也不是“是否使用密码/私钥”，而是这几类认证材料上方的一层交付策略：

```text
auth kind                    secure_access = false            secure_access = true
---------------------------------------------------------------------------------
none                         N/A                              N/A
password                     direct password carrier          managed password delivery
local unencrypted key        direct identity (-i)             local brokered identity
vault-managed key            N/A                              vault brokered identity
```

显示规则固定为：

- `none`：不显示 toggle；
- plain + `password` / plain + local unencrypted key：显示 toggle，默认开启，可关闭；
- sealed + 任何 secret-backed auth：不显示可切换 toggle，只显示只读状态 `required`；
- vault-managed key：视为天然 secure，不提供“关闭”路径；
- local encrypted key：不显示 toggle，因为该组合本身不允许成立。

这样定义的原因：

- operator 看见的是一个统一产品名；
- runtime 实际上仍然可以根据 auth kind 推导不同 delivery plan；
- 避免把“是否安全存储”与“是否安全交付”混为一个布尔值。

考虑过的替代方案：

- **让 `SSH 安全访问` 成为全局总开关**：会把 `none`、vault key、encrypted key 等无意义或强制场景也挤进同一控件，用户心智会混乱。
- **直接把它做成 auth kind 的别名**：会让 password 与 private key 路径无法共用同一面向 operator 的语言。

### 决策 3：plain 与 sealed 的组合约束由 auth validator 正式裁决，而不是在 UI 末尾兜底

系统必须把以下矩阵固化为正式 validator，而不是仅靠菜单提示：

```text
plain:
- none                                 allowed
- password                             allowed, strong warning, default secure_access=true
- local unencrypted key                allowed, default secure_access=true
- local encrypted key path             forbidden
- vault-managed key                    forbidden

sealed:
- none                                 allowed
- password                             allowed, secure_access required
- local unencrypted key                allowed, secure_access required
- vault-managed key                    allowed, secure_access required
- local encrypted key path             forbidden unless imported into vault first
```

选择这条路的原因是：如果只在“保存时失败”，operator 会在创建会话里来回输入 host/port/username 后才发现认证组合本身无效，体验和实现都很差。validator 必须尽量前置到 `SSH Authentication Setup` 阶段。

考虑过的替代方案：

- **只在 `Create/Apply` 时做最终校验**：会让错误发现太晚，也会让 menuconfig 流程难以解释“为什么已经填完却不能建”。
- **完全依赖 UI 控件裁剪，不做 shared validator**：MCP、配置文件读写与 future desktop host 仍然可能产生非法组合。

### 决策 4：SSH 创建与编辑流引入独立的 `SSH Authentication Setup` / `SSH Authentication --->`

在 `Add Target` 流中：

```text
Choose Storage Mode
  -> Choose Target Type = SSH
  -> (plain only) risk confirmation
  -> SSH Authentication Setup
  -> SSH Detail Editor
  -> Create Target
```

在编辑现有 target 时：

- plain SSH 在 `Connection Profile` 中显示 `SSH Authentication --->`；
- sealed SSH 在 unlocked `Sensitive Overlay` 中显示 `SSH Authentication --->`；
- `Credential Source --->` 不再是 SSH auth 的顶层入口，而是 private-key 分支里的子步骤。

`SSH Authentication Setup` 的职责：

- 选择 `none` / `password` / `private-key`
- 选择 `private-key` 时，再选择 `Use Local Key Path` / `Use Imported Vault Key`
- 录入本地 key path 时立即进行 trusted local inspection
- 若检测到 key 带 passphrase，立即中止当前分支并弹出强提示：
  - plain：必须改用 sealed，并将 key 导入 vault
  - sealed：必须先解锁 vault，并通过 `Import Local SSH Key Into Vault` 流程导入

之所以不把这些逻辑继续堆在 detail editor 里，是为了让创建流首先确认“这个 target 到底如何认证”，而不是让连接字段先于认证约束。

考虑过的替代方案：

- **维持当前 `Credential Source --->` 顶层入口，仅逐步拼补 password/none**：会让 password 与 none 成为 credential-source 的特例，模型不自然。
- **把所有 SSH auth 字段直接平铺在 detail editor**：会让 plain/sealed 条件裁剪与非法组合阻断更难表达。

### 决策 5：target 登录 password 的正式交付路径采用 app-generated askpass carrier，而不是 `sshpass` / `expect`

对于 target 登录 password，本次设计明确拒绝把 `sshpass` 或 `expect` 作为正式主架构。正式方案是：

- direct path（plain + secure_access=false）：
  - runtime 生成一次性 askpass helper
  - 通过结构化 env overlay 传递 `SSH_ASKPASS`、`SSH_ASKPASS_REQUIRE` 和受控 password carrier
  - helper 生命周期绑定当前 invocation
- managed path（plain + secure_access=true 或 sealed + password）：
  - runtime 创建受控的 password delivery session
  - 只把 display-safe session 信息与 helper/env overlay 交给子进程
  - 对 session cleanup、日志与错误分类使用和 broker 类似的 contract

这里有一个重要原则：**即使 `SSH 安全访问` 关闭，也不得把 password 暴露到 cmdline**。关闭开关只表示“绕过 managed delivery/session lifecycle”，不表示“允许 argv 泄露密码”。

选择 askpass carrier 的原因：

- OpenSSH 已有稳定接口，跨平台语义相对清晰；
- 可以通过 env overlay + helper 文件满足 MCP 的非交互闭环；
- 比 `expect` 更容易约束 display-safe 边界；
- 比 `sshpass` 更适合作为应用自带能力，而不是宿主外部依赖。

考虑过的替代方案：

- **`sshpass`**：虽然能避免明文直接出现在 cmdline，但依赖外部工具、平台兼容性和行为差异更大，不适合作为正式主路径。
- **`expect` 脚本**：对 prompt 文本、区域语言和交互时序依赖过强，极易脆弱。
- **运行时再次提示用户输入 password**：不能满足 MCP 与 unattended runtime 闭环要求。

### 决策 6：扩展 runtime 为统一的 `SshDeliveryPlan`，并为 structured invocation 增加 env overlay

为了让 menuconfig probe、MCP one-shot、interactive shell 和 future desktop host 复用同一条逻辑，runtime 需要统一产出一个结构化 delivery plan：

```text
SshDeliveryPlan
- None
- PasswordDirectAskpass
- PasswordManagedAskpass
- DirectIdentityFile
- LocalBrokeredIdentity
- VaultBrokeredIdentity
```

这要求 `CommandInvocation` / terminal provider 不再只支持 `program + args`，还必须支持 scoped env overlay 与受控 cleanup artifact。例如：

- `IdentityAgent=<endpoint>` 继续走 args；
- password askpass 走 env overlay；
- future helper path / temp dir 走 invocation-scoped cleanup contract。

这是本次设计的关键基础设施前置。没有 env overlay，password secure delivery 只能退回 shell flatten、外部脚本或 argv，都会破坏安全边界。

考虑过的替代方案：

- **把 env 继续藏在 terminal provider 的隐式宿主状态里**：会让 structured invocation 与 diagnostics 失真，也不利于 future desktop host 复用。
- **为 password 单独写一条与 structured invocation 平行的执行路径**：会重新制造 SSH 执行分叉。

### 决策 7：sealed password 先作为 target-scoped secret 处理，不额外引入独立 password secret family

本次设计中：

- plain + password：password 可以显式保存在 `config.toml` 中；
- sealed + password：password 存在 target 的 `Sensitive Overlay` 中，由 vault-authoritative `target-profile` 保护；
- 本次不新增独立的 `ssh-password` 或通用 password inventory 管理面。

这样做的原因：

- 用户当前最核心的诉求是“SSH target 能闭环使用 password”，而不是“密码也能像 SSH key 一样做独立库存管理”；
- sealed target 已经有 vault-authoritative 的 secret container，不需要为了 v1 再引入另一套 password secret family；
- 可以避免本次 change 再叠加新的 Security 页面 inventory 设计。

考虑过的替代方案：

- **把 sealed password 也做成独立 vault secret family**：长期可扩展，但会让本次范围从“SSH auth matrix”膨胀到“通用 password inventory productization”。
- **完全不允许 sealed password**：不符合用户要求，也会让许多遗留 SSH target 无法迁移到安全路径。

### 决策 8：legacy 配置采用“兼容读取、规范化写回”的渐进迁移

读取旧配置时：

- 无 auth 字段且无 `credential_ref`：视为 `kind=none`
- 有 canonical vault `credential_ref`：视为 `kind=private-key`、`private_key_source=vault-ref`、`secure_access=true`
- 有非 vault `credential_ref`：视为 `kind=private-key`、`private_key_source=local-path`、`secure_access=false`

写回新配置时：

- 统一写入 `ssh_auth` 结构
- 不再把 password 伪装成其他字段
- 旧的 top-level `credential_ref` 只作为兼容输入，不再作为未来的 authoritative 输出格式

这样做的原因：

- 可以让现有 direct identity 与 vault key 配置继续无损工作；
- 不要求用户先手动迁移全部 target；
- 新旧模型并存一段时间，但真相层只有一个。

考虑过的替代方案：

- **一次性破坏式迁移**：对已有配置过于激进。
- **永久维持双真相（`credential_ref` + `ssh_auth` 同时写回）**：后续维护成本太高，也容易漂移。

## 风险 / 权衡

- **plain SSH password 明文放行削弱了“配置文件中禁止保存敏感明文”的原始强约束** → 通过显式 risk confirmation、默认开启 `SSH 安全访问`、明确升级路径（sealed / vault）和文档矩阵把例外约束在 SSH plain target 这一小块范围内。
- **增加 `ssh_auth` 新模型与 legacy `credential_ref` 迁移会扩大 engine / domain / mcp / operator-console 的联动改动面** → 通过 shared validator 和统一 `SshDeliveryPlan` 避免多处各写一套分支。
- **password secure delivery 要求 structured invocation 支持 env overlay，属于基础设施变更** → 将 env overlay 作为单独的先行实现切片，并在 tasks 中明确其优先级高于 UI 重排。
- **本地 key inspection 会引入 trusted local file read 与格式判断复杂度** → 只做“是否带 passphrase / 是否是受支持 SSH key”的受控检查，不在此阶段扩展成完整 key inventory。
- **sealed password 先放进 target-scoped secret 而不是独立 inventory，未来可能需要再抽象** → 在本次先把它收敛为 non-blocking follow-up，而不是把 v1 范围膨胀。

## 迁移计划

1. 先落 spec 与 shared validator，固定合法/非法 auth 组合和 `SSH 安全访问` 规则。
2. 在 engine / domain 中引入 `SshAuthConfig` 与 legacy `credential_ref` 兼容读取。
3. 为 structured invocation 与 terminal provider 增加 env overlay / cleanup 支持。
4. 在 runtime 中实现 `SshDeliveryPlan` 推导，先打通 password askpass carrier 与 local/vault key 计划分流。
5. 重构 menuconfig 创建与编辑流，加入 `SSH Authentication Setup` / `SSH Authentication --->`。
6. 实现配置写回规范化，把旧配置保存为新 `ssh_auth` 结构。
7. 补齐 automated tests、matrix 文档与错误分类断言。

回滚策略：

- 兼容读取逻辑必须在整个迁移期保留；
- 若新 writer 或 UI 流出现问题，仍可回退到 legacy reader 解释现有配置；
- 新增的 `ssh_auth` 字段必须采用可忽略的向后兼容解析方式，避免让旧实例直接无法启动。

## 开放问题

- 后续是否需要把 sealed password 从 target-scoped secret 提升为可复用的独立 vault secret family？本次设计暂不阻塞实现，但需要保留扩展位。
