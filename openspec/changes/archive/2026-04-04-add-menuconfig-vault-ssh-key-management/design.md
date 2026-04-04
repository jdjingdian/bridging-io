## 上下文

当前仓库已经具备 SSH key vault 化所需的若干底层能力，但这些能力还没有被组织成操作员可用的完整产品路径：

- `bridgingio-secrets` 已支持 canonical `vault://bridgingio/ssh-private-key/<name>`、secret summary、version record，以及 SSH broker / degraded fallback 交付
- `bridgingio-core vault import` 已能通过 file / stdin / fd / tty prompt 等 route 导入 generic secret
- `bridgingio-operator-console` 的 Security 页当前只产品化了 vault 状态与 token 管理；SSH key 仍没有专门的信息架构
- `Targets` 页当前仍把 `credential_ref` 视为普通文本字段，operator 需要自己记住 `vault://...` URI 或继续依赖宿主现成 key 文件

这导致 SSH target 的核心价值没有被完整产品化。系统在运行时已经知道如何把 vault 中的 `ssh-private-key` 交给 SSH connector，但在“操作员如何把 key 放进 vault、如何确认导入成功、如何在 target 中复用它”这一段，仍然缺少正式管理面。

更重要的是，这个缺口不能简单塞进现有的 `add-menuconfig-sensitive-target-management`。后者的重点是 target 对象自身的真相边界，而不是 `ssh-private-key` 这种可被多个 target 复用的独立 secret 对象。若继续混在同一个 change 中，会把“target object 管理”和“secret object 管理”两套问题搅在一起。

本设计因此聚焦一个独立对象族：

- `ssh-private-key` secret 是 vault-managed object
- SSH target 只消费其 canonical `credential_ref`
- Security 页面承担导入与管理入口
- Targets 页面承担绑定与切换入口

## 目标 / 非目标

**目标：**

- 为 `menuconfig` 的 Security 页面新增正式的 SSH key import / management 路径，而不是继续只提供 vault/token 管理
- 把本地 SSH 私钥导入流程收敛为 display-safe metadata + 受控本地 secret capture 的组合，而不是把私钥内容放进普通字段编辑器
- 让 SSH key 在 menuconfig 中成为可浏览、可删除后再导入更新、可被 target 选择的正式对象
- 让 SSH target 绑定 vault key 时不再依赖手填 raw `vault://...` URI
- 保持 imported key 与 target storage mode 解耦，使 plain SSH target 与 future sensitive SSH target 都可复用同一 canonical key ref

**非目标：**

- 不在本次设计中引入通用“所有 vault secret 类型的统一浏览器”；本次只产品化 `ssh-private-key`
- 不在本次设计中实现 secret reveal / export，也不把私钥明文作为任何 menuconfig 结果页面的一部分
- 不在本次设计中引入 SSH key 批量删除、恢复站或跨实例 destroy 语义；首发只要求 import、display-safe browse 与单 key delete-first 更新
- 不在本次设计中重做整个 Targets 主布局；target 侧只增加可复用的 key binding 子流程
- 不在本次设计中替代 headless CLI `vault import`；CLI 仍是自动化和非交互部署路径

## 设计草图

### Security 页面拓扑

```text
Security
  -> Unlock Vault --->               (locked only)
  --- SSH Key Count = N              (locked only)
  -> Import SSH Key --->             (unlocked only)
  -> SSH Key Management --->         (unlocked only)
  -> Create Token --->               (unlocked only)
  -> Token Management --->           (unlocked only)
  -> Delete Vault --->               (locked/unlocked per existing contract)
```

### SSH key import 流程

```text
Import SSH Key
  -> Key Name (ops-prod) --->
  -> Key Label (Ops Prod) --->
  -> Source Path (~/.ssh/id_ed25519) --->
  -> Import Now --->
       -> local file read
       -> parse / canonicalize key
       -> if encrypted: hidden local passphrase prompt
       -> write vault://<ns>/ssh-private-key/<name>
  -> Import Result (display-safe only)
```

### SSH target 绑定流程

```text
SSH Target Detail
  -> Credential Source --->
       -> No Credential
       -> Use Imported Vault SSH Key --->
       -> Import Local SSH Key Into Vault --->
       -> Manual Reference --->

Sensitive SSH Target Detail (unlocked)
  -> Sensitive Overlay --->
       -> Credential Source --->
            -> Use Imported Vault SSH Key --->
            -> Import Local SSH Key Into Vault --->
            -> Manual Reference --->
```

## 决策

### 决策 1：把 `ssh-private-key` 产品化为 Security 页面的一等管理对象，而不是继续藏在 generic `vault import` 背后

虽然 CLI 已经存在 generic `vault import`，但 operator-facing 的心智并不是“我要导入某个抽象 secret”，而是“我要把 SSH key 放进 vault 并给 target 用”。因此 `menuconfig` 不应只暴露一个无类型的 generic import，而应在 Security 页中明确产品化：

- `Import SSH Key --->`
- `SSH Key Management --->`

这样做，而不是继续要求 operator 在外部 CLI 执行 `vault import` 再回到 TUI 手填 `credential_ref`，是为了让 SSH target 的关键依赖链路在本地管理面内闭环。

### 决策 1.1：vault locked 时只暴露聚合 `SSH Key Count`，不暴露任何单个 key inventory

虽然 `ssh-private-key` 的列表和详情都属于 display-safe 投影，但它们仍然会暴露单个 key 的 identity 信息，例如：

- `label`
- canonical `credential_ref`
- status
- record id
- last rotated / last used

这些字段在 vault `locked` 时仍然可能泄露环境命名、租户命名或凭据 inventory，因此本设计要求：

- `locked` 状态下，Security 页最多只显示静态聚合 `SSH Key Count = N`
- `locked` 状态下，不开放 `SSH Key Management --->` 列表浏览
- `locked` 状态下，不显示任何单个 imported key 的 label、canonical ref、status、record id 或 usage 摘要

这样做，而不是继续允许 operator 在 locked 状态下浏览 display-safe key inventory，是为了让 vault `locked` 的语义更接近“只知道有多少，不知道具体是谁”。

### 决策 2：SSH key 的 operator identity 分离为 `key name` 与 `label`

每个 imported key 都需要两个不同层次的标识：

- `key name`
  - 用于生成 canonical `vault://<namespace>/ssh-private-key/<name>`
  - 必须稳定、可归一化、适合作为引用真相
- `label`
  - operator-facing 展示文案
  - 允许与 `key name` 脱钩，避免把 URI slug 直接当成人类文案

若 operator 未填写 `label`，系统可以默认回落到 `key name` 或文件名派生值；但 target 与 runtime 持久化时必须只保存 canonical `credential_ref`，不得把 `label` 当成引用真相。

### 决策 3：menuconfig import 采用 metadata-first、file-path-first 的受控本地导入

`menuconfig` 不适合把 SSH 私钥内容放进普通单行编辑弹窗。首发流程定义为：

1. 在普通 popup 中采集 display-safe metadata：
   - `key name`
   - `label`
   - `source path`
2. operator 确认后，由本地管理面读取该路径对应的 SSH 私钥文件
3. 若 key 为 passphrase-protected，则通过隐藏输入的本地 prompt 完成验证
4. 导入成功后，只返回 display-safe 结果摘要

选择 file-path-first，而不是让 operator 直接在 TUI 中粘贴多行 PEM，有三个原因：

1. 当前 menuconfig 文本编辑合同是单行 popup，不适合承载多行 secret
2. 用户当前最自然的来源本来就是本地 `~/.ssh/...` 文件
3. 这样可以复用既有 CLI / core 的 secret import 逻辑，而不用为 TUI 发明一套新的明文输入语法

CLI generic `vault import` 仍可继续支持 stdin / fd / tty prompt；menuconfig 首发只需要把本地文件导入路径产品化。

### 决策 4：passphrase-protected SSH key 必须在导入时解决，而不是把 passphrase 问题留给运行时

仓库当前已经明确：运行时 SSH broker 不允许再次触发 key passphrase prompt。本设计据此固定产品语义：

- 若导入的是未加密 key
  - 直接 canonicalize 并写入 vault
- 若导入的是 passphrase-protected key
  - menuconfig / trusted local flow 必须在导入阶段完成本地 passphrase 输入与验证
  - runtime 后续只消费 vault 保护下的 signer material

这样做，而不是在连接时再要求 operator / agent 输入 SSH key passphrase，是为了避免把另一份高价值 secret 重新带回运行时数据面。

### 决策 5：`SSH Key Management` 采用列表页 -> 详情页拓扑，并且只在 unlocked 状态下开放

`SSH Key Management` 不是一个 reveal 页面，而是一个 display-safe 管理页；但该管理页只在 vault `unlocked` 时开放。其正式拓扑定义为：

```text
SSH Key Management
  -> (serial) label [status] ---> 
      -> Credential Ref = vault://...
      -> Label = ...
      -> Kind = ssh-private-key
      -> Record ID = ...
      -> Last Rotated = ...
      -> Last Used = ...
      -> Delete SSH Key --->        (unlocked only, requires confirm)
```

列表页与详情页都不得展示：

- 私钥明文
- 密文 locator
- unwrap material
- import passphrase

选择这种 display-safe detail，而不是把 key material 做成“一次性回显”，是因为 SSH key import 并不会产生一个用户此前不知道的新秘密；用户需要的是“确认对象已被导入并可被 target 绑定”，而不是再次查看私钥内容。同时，locked 状态下连这些 detail 也不应暴露，只保留聚合 count。

### 决策 6：menuconfig 采用 delete-first 更新策略，而不是在详情页做 `Import New Version`

`bridgingio-secrets` 已经具备 secret record / version record 结构，但 menuconfig 作为 operator 面板需要降低误操作成本并避免“已导入后仍停留在导入页重复执行”的交互陷阱。因此 menuconfig 的正式语义改为：

- 若 operator 尝试用同一 `key name` 重复导入
- 系统必须拒绝该导入并提示先删除旧 key
- operator 通过详情页 `Delete SSH Key` 完成删除并二次确认后，再执行重新导入

这样做，是为了把“更新 key”的运维动作显式化，并减少用户把 `record id` 误解为可手工管理版本号的认知负担。

### 决策 7：SSH target 绑定 vault key 必须采用 picker 子流程，而不是继续把 raw `credential_ref` 输入当作默认路径

当前 Targets 页面里的 `credential_ref` 只是一个文本字段，这不足以构成 operator-friendly 的 SSH key 绑定体验。本设计将 SSH target 的正式绑定流程定义为：

- 默认提供 `Credential Source --->`
- 若 vault 已解锁，可进入 `Use Imported Vault SSH Key --->`
- picker 展示已导入 `ssh-private-key` 的 display-safe 摘要
- 选中后只把 canonical `credential_ref` 写回 target

同时，考虑到当前仓库仍存在手工配置与兼容路径，manual reference fallback 可以继续保留，但不再作为使用 vault-managed SSH key 的首选 operator path。

### 决策 7.1：`sensitive + ssh` 只在 unlocked `Sensitive Overlay` 中暴露 picker，locked 状态不得泄露已绑定 key 身份

这个 change 与 `add-menuconfig-sensitive-target-management` 的衔接规则固定为：

- 对 plain SSH target，`Credential Source --->` 可以作为普通详情页中的正式子流程
- 对 sensitive SSH target，`Credential Source --->` 属于 unlocked 的 `Sensitive Overlay` 子页
- sensitive target 处于 `locked` 时，不得通过 target 详情页泄露当前绑定 imported key 的 label、canonical ref、status 或 record id

也就是说：

- Security 页面在 locked 状态下已经隐藏 key inventory
- sensitive target 的 locked detail 也不得把同一 inventory 从 target 侧重新泄露出来

这样定义，是为了保证 “secret object inventory” 与 “sensitive target public cache” 两侧的最小披露边界一致。

### 决策 8：SSH key 管理与 target storage mode 明确解耦

这个 change 不重新讨论 target 是 `plain` 还是 `sensitive`。无论 target 采用哪种 storage mode，只要它需要 SSH private key，都应消费同一个 canonical `ssh-private-key` secret family。

也就是说：

- `plain SSH target` 可以绑定 imported SSH key
- future `sensitive SSH target` 也可以绑定同一 imported SSH key
- imported key 自身不是 target profile 的一部分，也不跟随单个 target 复制存储

这样做，而不是把 SSH key 藏进某个 target-specific secret overlay，是为了让同一 key 能被多个 target 受控复用，并且让 Security 页面承担“secret object 管理”的职责。

## 风险 / 权衡

- [menuconfig 首发采用 file-path-first import] → 不能覆盖所有无文件来源的交互场景；通过继续保留 CLI generic `vault import` 作为 headless / automation 路径缓解
- [target 绑定增加 picker，会与现有 target 编辑路径交叉] → 通过把它定义为可复用子流程，而不是重做整个 Targets 主布局来降低冲突
- [locked 状态下只能看到 count，不能浏览 key inventory] → 排障时信息更少，但换来更严格的 inventory 最小披露边界
- [SSH key management 提供 delete] → destructive 动作风险上升；通过二次确认 + 删除后清理 target 绑定来降低误删后悬挂引用风险
- [引入 `key name` 与 `label` 两层标识] → UI 稍微更复杂，但换来稳定引用真相与更好的 operator-facing 文案
- [要求 encrypted key 在导入时解决 passphrase] → 导入实现复杂度上升，但能换来运行时不再暴露第二套 secret prompt

## 迁移计划

1. 在 vault / trusted local 管理层补齐 SSH key import 与 delete-first 更新的 operator-facing 合同，继续沿用 canonical `ssh-private-key` secret kind。
2. 在 `menuconfig` 的 Security 页面新增 `Import SSH Key` 与 `SSH Key Management` 拓扑，并在 `locked` 状态下只保留 `SSH Key Count` 聚合摘要，不开放单个 key 列表。
3. 在 SSH target 流程中新增 vault key picker / inline import 子流程，并让最终落盘结果继续只保存 canonical `credential_ref`。
4. 更新 operator interface matrix 与必要测试，覆盖导入、encrypted key import、duplicate key name 拒绝、delete-first 更新、target binding 与 locked/unlocked gate。

## 开放问题

- `key name` 的 operator-facing 标题最终使用 `Key Name`、`Credential Name` 还是 `SSH Key ID`，可以在实现阶段按整体现有术语统一，但语义上都指向 canonical ref 的 name 片段。
