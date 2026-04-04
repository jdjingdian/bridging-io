## 上下文

当前 `menuconfig` 已经把交互风格收敛到单栏、逐级进入的 `menuconfig` 拓扑，但 Targets 页面仍停留在早期形态：列表页只展示现有 target 和少量“新增 SSH/ADB”入口，详情页也仍按“直接编辑 config 字段”的思路展开。与此同时，runtime 与配置模型已经引入了 `storage_class`、`access_class` 和 `sealed_profile_ref`，sealed target 也具备 locked/unlocked 下的 redacted/resolved 投影能力，但这些能力尚未被 menuconfig 正式组织成面向操作员的目标管理体验。

现状还存在一个更关键的真相边界问题。当前 sealed overlay 设计默认把 public descriptor 继续放在 `config.toml` 外层，vault 里的 overlay 只保存 digest 和敏感字段。这足以做“解锁后校验外层 descriptor 是否被篡改”，却不足以在操作者手工删除 config descriptor、partial rewrite 或 cache 漂移之后，从 vault 单独恢复一个完整的 sensitive target。对 menuconfig 来说，这会让“哪一层才是真相源”变得含糊。

本次设计覆盖四个相互耦合的模块：

- `bridgingio-operator-console`：Targets 页面、Add Target 流程、详情页和删除门控
- `bridgingio-engine`：standalone target 配置读写与 public cache 语义
- `bridgingio-mcp`：sealed target 的 catalog projection、unlock 后 reconcile 与 digest 校验
- `bridgingio-secrets`：vault 中 `target-profile` 对象的存储、摘要发现与受控读取

同时，本设计必须保留三个既有边界：

1. `menuconfig` 主流程继续保持单栏逐级进入，不引入 NetworkManager 式分栏主界面
2. `plain target` 的 loopback 兼容路径继续存在，避免把低敏配置一刀切地强制迁移进 vault
3. 普通列表与普通详情页继续使用 display-safe projection，不把 vault 内部对象直接暴露给 UI

另外，本设计需要与并行的 `add-menuconfig-vault-ssh-key-management` 保持边界清晰：

- 那个 change 负责把 `ssh-private-key` 做成独立的 vault-managed secret object，并提供 Security 页面和 picker
- 本 change 负责规定 sensitive target 如何引用这些对象，以及在 locked/unlocked 状态下对该引用暴露多少信息

## 目标 / 非目标

**目标：**

- 把 `Add Target` 固化为 `storage mode -> target type -> detail editor` 的正式流程，并对 `plain + ssh` 提供明确风险确认
- 将 `sensitive target` 正式定义为 vault-authoritative 对象，而不是“config 真相 + vault 补丁”
- 让 `config.toml` 对 sensitive target 只承担 public cache 角色，并允许在 unlock 后从 vault 自动修复缺失或过期缓存
- 要求 `vault locked` 时禁止创建、编辑、删除 sensitive target，避免出现无法保存 overlay 的半完成体验
- 收敛 sensitive target 的 digest 语义，使其仅覆盖 public descriptor 字段，而不把 sensitive overlay 混入“公开完整性”校验
- 为旧格式 sealed overlay 提供向 `target-profile` payload 的升级路径

**非目标：**

- 不在本次设计中引入新的主布局风格；Targets 页面不会切换为分栏或表单驱动主界面
- 不在本次设计中开放 `sealed-full` 给 menuconfig 操作员作为首发产品模式；首发只产品化 `plain` 与 `sensitive(sealed-overlay)`
- 不在本次设计中补齐所有 future target kind；menuconfig 首发只要求展示当前 core 正式支持的类型
- 不把 sensitive target 删除升级为 `vault delete` 同级的全局危险动作；本次仅处理 target 级对象

## 决策

### 决策 1：把 `sensitive target` 产品化为 `sealed-overlay + token-scoped`

`storage_class` / `access_class` 在底层仍保留通用语义，但 menuconfig 不直接把这两个底层枚举暴露给操作者。操作员在 Targets 中只看到两种模式：

- `Plain Target`
  - 映射到 `storage_class = plain`
  - 默认 `access_class = anonymous-local`
- `Sensitive Target`
  - 映射到 `storage_class = sealed-overlay`
  - 默认 `access_class = token-scoped`

选择这一路径，而不是在 UI 中直接暴露 `plain / sealed-overlay / sealed-full` 与 `anonymous-local / token-scoped`，是为了让目标管理保留 operator-facing 的可理解性，同时避免在第一版就把 `sealed-full` 的迁移和恢复复杂度强行暴露给用户。

### 决策 2：vault 中引入 authoritative `target-profile` 对象，替代“外层 descriptor + 内层 overlay”弱组合

sensitive target 在 vault 中存储为单个 `target-profile` secret，其 payload 统一包含：

- `public_descriptor`
  - `id`
  - `display_name`
  - `aliases`
  - `kind`
  - `enabled`
  - `storage_class`
  - `access_class`
  - `sealed_profile_ref`
- `sensitive_overlay`
  - connection 细节
  - `notes`
  - `credential_ref`
  - target policy
  - target-scoped toolchain override
- `public_descriptor_digest`
  - 仅覆盖 `public_descriptor`

这样设计，而不是继续让 vault 只保存 digest + overlay，有三个原因：

1. unlock 后才能从 vault 单独恢复 config public cache
2. 手工删掉 config cache 不会等价于删除 sensitive target 真相
3. digest 只需要回答“公开 descriptor 有没有漂移”，而不需要承担“恢复全部对象”的职责

对于 `kind = ssh` 的 sensitive target，这里的 `credential_ref` 可以指向独立的 imported `ssh-private-key` secret。但该引用仍然属于 `sensitive_overlay`，而不是 `public_descriptor`。这意味着：

- `config.toml` public cache 不得保存该 canonical ref
- locked 状态下 UI 不得展示所选 imported key 的 label / canonical ref
- unlocked 状态下 `Sensitive Overlay` 才可以通过正式 picker 管理它

### 决策 3：`config.toml` 对 sensitive target 只保留 public cache，但仍然允许自动修复

对 sensitive target，`config.toml` 继续持久化一个公开缓存，以便：

- vault 锁定时仍可显示最小 inventory
- search / list / diagnostics 在 locked 态下仍有稳定入口
- standalone 仍保有可理解的人类可编辑配置快照

但该缓存不再是 authoritative truth。正式语义改为：

- vault 中的 `target-profile` 才是 authoritative truth
- `config.toml` 中的 sensitive descriptor 只是可恢复 public cache
- 手工删除或损坏 public cache，不得被解释为“该 sensitive target 已被正式删除”

选择“保留 public cache + 允许自动修复”，而不是“完全不再写 config”，是为了兼顾 locked 态可见性与 operator 可诊断性；同时又避免继续把外层 config 当成唯一真相。

### 决策 4：`unlock vault` 成功后执行 `reconcile sensitive targets`

unlock 成功后的正式流程固定为：

1. 列出 vault 中 `kind = target-profile` 的 secret summary
2. 逐个读取 authoritative payload
3. 校验 `public_descriptor_digest`
4. 生成当前 sensitive target 的 authoritative public descriptor map
5. 对比并修复 `config.toml` 中的 sensitive public cache
6. 刷新内存中的 target catalog，使当前 Targets 页面从 `[cached]` 进入 `[resolved]`

这样定义，而不是把 reconcile 变成“仅刷新运行时显示、不回写 config”，是因为本设计明确把 config public cache 视为可恢复副本。若 unlock 后不修复磁盘缓存，用户一旦重启或回到 locked 态，就仍会看到一份被手工破坏的 inventory。

同时为了避免覆盖 menuconfig 当前未保存改动，实现上允许采用两阶段策略：

- 先刷新内存 catalog
- 若当前 session 无 dirty edits，则立即回写磁盘 config
- 若当前 session 有 dirty edits，则提示“Sensitive target cache repaired; save to persist”

### 决策 5：`vault locked` 时禁止创建、编辑、删除 sensitive target

这项设计不允许“先填 sensitive draft，最后保存时报错”。正式语义是：

- locked 时：
  - `Add Target` 只允许进入 `plain`
  - `Sensitive Target` 入口必须被禁用或改为 `Unlock Vault --->`
  - sensitive detail 只允许浏览 public cache
  - 对 `sensitive + ssh`，不得显示当前绑定 imported key 的 label、canonical ref 或等价 inventory；最多只能显示通用的“Sensitive Overlay Locked”提示
  - 不允许进入 overlay 编辑和删除流程
- unlocked 时：
  - 才允许完整 sensitive create/edit/delete

选择前置门控，而不是后置失败，是因为 sensitive target 的关键价值就在于 vault 保存 overlay；如果 vault 当前不可写，允许用户先填大量字段只会制造明显挫败感。

### 决策 5.1：`sensitive + ssh` 的 key 绑定属于 unlocked `Sensitive Overlay` 子流程

对于 `kind = ssh` 的 sensitive target，credential binding 的正式语义固定为：

- 绑定结果仍然是 canonical `credential_ref`
- 该字段存放在 vault authoritative `target-profile.sensitive_overlay`
- `Sensitive Overlay` 子页在 `unlocked` 时可以暴露 `Credential Source --->`
- `Credential Source --->` 可以复用 `add-menuconfig-vault-ssh-key-management` 中定义的 imported key picker / inline import 流程

而在 `locked` 状态下：

- 不开放 `Credential Source --->`
- 不显示已绑定 imported key 的具体身份
- 不把 imported key 的 existence / label 复制进 public cache

这样做，是为了让“target object 的 public cache”与“secret object 的 inventory”边界保持一致，避免一个 locked 的 sensitive target 反过来泄露 Security 页面本来已经隐藏的 key inventory。

### 决策 6：Targets 页面继续遵守 menuconfig 的单栏 topology，吸收而不复制表单式 TUI

Targets 信息架构固定为：

```text
Targets List
  -> Add Target
      -> Choose Storage Mode
      -> Choose Target Type
      -> Risk Confirm (plain + ssh only)
      -> Detail Editor
  -> Target Detail
      -> Public Descriptor
      -> Sensitive Overlay
          -> Credential Source (ssh only)
      -> Policy
      -> Delete Target
```

这保留了 `menuconfig` 的 row grammar、`--->` 导航语义和 popup 合同，同时在 detail 层吸收类似 NetworkManager 的“按段编辑”心智。之所以不直接转向 NetworkManager 风格，是因为当前仓库已经正式把 menuconfig 定义为单栏主流程；改成分栏或大表单会与现有 style matrix 冲突。

### 决策 7：`plain + ssh` 允许保留，但必须走显式风险确认

SSH 是最明显会让用户产生“是否该进 vault”疑虑的 target 类型，因此 `plain + ssh` 不能只是提示文案，必须是正式确认步骤。确认弹窗应明确告知：

- 该 target 的连接字段会留在 `config.toml`
- 若希望把连接细节交给 vault 管理，应改选 `Sensitive Target`

这样做，而不是直接禁止 `plain + ssh`，是为了保留“不想初始化 vault 的单机用户”路径；而这样做，而不是仅在帮助文本中轻描淡写，是因为 SSH 连接信息的风险边界足够明确，值得用户在创建时主动确认一次。

### 决策 8：旧格式 sealed overlay 采用“解锁后升级”为 authoritative `target-profile`

当前已存在的 sealed overlay payload 只包含 digest 和敏感字段，无法独立重建 `public_descriptor`。迁移策略定义为：

- 若发现旧格式 sealed overlay，且外层 config descriptor 仍存在
  - 在 unlocked 状态下读取旧 overlay
  - 用当前 config public descriptor + 旧 overlay 组装新的 `target-profile`
  - 成功写回 vault 后，后续均按新格式读取
- 若发现旧格式 sealed overlay，但外层 descriptor 已缺失
  - 系统不得猜测恢复
  - 必须返回明确的 `repair-needed` 或等价诊断，要求操作者手工补齐 descriptor 或从备份恢复

这样设计，而不是在缺 descriptor 时“尝试从 label/ref 猜测重建”，是为了避免把 sensitive target 恢复变成不稳定的隐式推断。

## 风险 / 权衡

- [vault authoritative sensitive target 需要新 payload 结构] → 通过旧格式 upgrade path 缓解；缺 descriptor 的旧对象明确进入 `repair-needed`
- [unlock 后自动修复 config cache 可能与当前 dirty edits 冲突] → 先刷新内存，再按 dirty state 决定是否立即落盘
- [保留 public cache 仍允许用户误删或手改 config] → 通过 authoritative reconcile 修复，并把“手改 cache 不等价删除”写入规范
- [Targets 仍走单栏，会比表单式 TUI 多一步导航] → 换来与当前 menuconfig style contract 的一致性，避免项目再维护第二套交互语言
- [plain + ssh 继续存在] → 通过显式风险确认降低误用，但不消除用户主动接受的风险

## 迁移计划

1. 在 vault 层引入 `target-profile` secret kind 与新 payload 结构。
2. 扩展 target digest 逻辑，只对 `public_descriptor` 做摘要，不再混入 sensitive 字段。
3. 在 runtime 中实现 unlock 后的 `reconcile sensitive targets`。
4. 为旧格式 sealed overlay 提供 unlocked upgrade 逻辑，并把无法自动升级的对象标记为 `repair-needed`。
5. 重构 menuconfig 的 Targets 页面：
   - mode-first Add Target
   - locked/unlocked state gating
   - public descriptor / sensitive overlay / policy / delete 四段式 detail
6. 更新 operator interface matrix 与测试矩阵，覆盖 create/edit/delete/reconcile/tamper/repair 场景。

## 开放问题

- `target-profile` secret 的 operator-facing `label` 是否固定等于 `display_name`，还是需要独立 label 字段作为未来的 vault summary 文案入口，当前仍可在实现时按现有 summary 体验再确认。
- 对于 unlock 后发现 dirty edits 的 menuconfig session，最终是“提示用户保存”还是“强制先处理 reconcile 再允许继续编辑”，当前可先按前者设计并在实现中验证可用性。
