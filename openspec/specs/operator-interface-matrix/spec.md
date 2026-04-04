# operator-interface-matrix 规范

## 目的
待定 - 由归档变更 expand-core-test-matrix-and-interface-matrix-docs 创建。归档后请更新目的。
## 需求
### 需求:本地 operator surfaces 必须维护正式的接口矩阵文档
BridgingIO 必须维护一份正式的本地 operator interface matrix，用于记录暴露给 UI、TUI、standalone 管理面和受信任本地调用方的接口真相。该矩阵至少必须覆盖命令或接口名称、调用方范围、输入、输出、状态、错误码与 apply strategy。对于 vault 与 token 这类安全管理接口，矩阵还必须额外记录状态前置条件、是否需要二次确认、是否涉及一次性 reveal 结果，以及 destructive action 的恢复语义。

#### 场景:新增本地 control-plane 命令
- **当** 团队新增或修改一个面向本地 operator surface 的 command、事件或设置写入接口
- **那么** 系统必须同步更新接口矩阵文档，记录该接口的输入输出合同、适用调用方、状态/错误语义，以及必要的状态门控或确认要求，而不是只修改实现代码

#### 场景:接口行为涉及重启或受控未实现
- **当** 某个本地 operator 接口存在 `restart_required`、`not_ready`、`method_not_implemented`、一次性 reveal 或 destructive confirmation 等正式语义
- **那么** 接口矩阵必须明确记录这些状态与对应恢复动作，而不是仅靠 README 叙述或代码注释隐式表达

### 需求:本地安全管理接口矩阵必须记录 vault/token 的状态门控与删除前置条件
当本地 operator surface 提供 vault 与 token 管理动作时，接口矩阵必须明确记录这些动作各自的状态前置条件、确认要求与输出合同，而不是只列出命令名称。至少必须覆盖 `vault init`、`vault delete`、`token create`、`token revoke`、`token delete` 以及它们与 `uninitialized / locked / unlocked / revoked / expired / deleted` 等状态的关系。

#### 场景:矩阵记录 token delete 的正式前置条件
- **当** 团队为本地 operator surface 新增 `token delete`
- **那么** 接口矩阵必须明确记录“只有 `revoked` token 才允许 delete，`expired` 仍需先 revoke”的合同，而不是只写一个抽象的删除接口

#### 场景:矩阵记录 vault delete 的确认与恢复语义
- **当** 团队为本地 operator surface 新增 `vault delete`
- **那么** 接口矩阵必须记录该动作需要二次确认、成功后返回 `uninitialized`、且不隐式清除共享 os-native protector 的合同

#### 场景:矩阵记录 token create 的有效期输入与一次性 reveal
- **当** 团队为本地 operator surface 暴露 `token create`
- **那么** 接口矩阵必须明确记录该动作至少包含 `label`、有效期模式（长期或指定失效时间）、一次性明文结果回显，以及该失效时间按本机时钟解释的合同

#### 场景:矩阵记录 expiring token 的时间异常约束
- **当** 本地 operator surface 支持创建具有过期时间的 token
- **那么** 接口矩阵必须记录本机时钟异常、时钟回拨防护或等价时间健康诊断对该接口的影响，而不是把 expiring token 当作与时间无关的普通 create 操作

### 需求:本地安全管理接口矩阵必须记录 menuconfig Security 页的状态裁剪行为
对于 `menuconfig` 这类本地 TUI surface，接口矩阵必须记录不同 vault 状态下可见动作的裁剪行为，而不是只记录“Security 页面存在 vault 管理入口”的笼统描述。

#### 场景:矩阵记录 unlocked 状态下的 Security 页
- **当** `menuconfig` 的 Security 页面在 `unlocked` 状态下暴露 `Import SSH Key`、`SSH Key Management`、`Create Token` 与 `Token Management`
- **那么** 接口矩阵必须明确记载这些入口各自的可见条件、导入/管理语义与 display-safe 限制

#### 场景:矩阵记录 locked 状态下的 Security 页
- **当** `menuconfig` 的 Security 页面在 `locked` 状态下显示 SSH key 相关信息
- **那么** 接口矩阵必须明确记载该状态只允许显示聚合 `SSH Key Count`
- **并且** 不得暴露任何单个 imported key 的可识别摘要或管理入口

#### 场景:矩阵记录 SSH target key binding 输出
- **当** 本地 operator surface 允许 SSH target 通过 picker 选择某个 imported vault key
- **那么** 接口矩阵必须明确记录该流程的最终输出是 canonical `credential_ref`，而不是 secret 明文或非正式 display label

### 需求:token 管理接口矩阵必须记录列表页与详情页拓扑
当本地 operator surface 为 token 管理提供“列表页 -> 详情页”的两级结构时，接口矩阵必须明确记录这两个页面/接口各自展示的字段、可触发动作、状态可见性和受控未实现语义，而不是继续只写一个抽象的 `Token Management` 入口。

#### 场景:矩阵记录 token 列表页摘要字段
- **当** `menuconfig` 的 Token Management 采用摘要列表展示 token
- **那么** 接口矩阵必须明确记录每一行至少包含稳定序号、别名、有效期摘要、状态标签和 `--->` 导航入口

#### 场景:矩阵记录 token 详情页动作与字段
- **当** `menuconfig` 为单个 token 提供统一详情页
- **那么** 接口矩阵必须明确记录该页面至少包含 `Token ID`、`别名 = <value> --->` 直编入口、`<*>/< > 开关状态 = [启用|禁用]`（仅空格触发切换）、display-safe 指纹/摘要、有效期、`权限管理 --->`（单箭头）与 `撤销 --->` / 受状态裁剪的 `删除 --->`

#### 场景:矩阵记录权限管理入口的受控未实现语义
- **当** 当前版本尚未开放 token 对 profile/scope 的完整编辑能力，但详情页仍暴露 `权限管理 --->`
- **那么** 接口矩阵必须明确记录该入口返回 `method_not_implemented` 或等价占位反馈的合同，而不是留空为未定义行为

#### 场景:矩阵记录详情页动作文案去重
- **当** token 详情页首行已经给出 `Token ID`
- **那么** 接口矩阵必须明确记录详情动作（如 `权限管理 --->`、`撤销 Token --->`）不再重复附带 `(token-id)` 文案

### 需求:认证拒绝矩阵必须记录 token 状态专属语义
当 model-plane 或 MCP surface 因 bearer token 被拒绝时，接口矩阵必须明确记录不同 token 状态对应的认证拒绝语义，而不是继续以一个泛化的“无效 token”条目覆盖所有失败原因。

#### 场景:矩阵记录 disabled token 的认证拒绝
- **当** bearer token 因本地管理面已将其禁用而被拒绝
- **那么** 接口矩阵必须明确记录该请求在 `authn` 阶段失败，并具有 `token_disabled` 或等价稳定语义

#### 场景:矩阵记录 revoked 与 expired token 的认证拒绝
- **当** bearer token 因 `revoked` 或 `expired` 被拒绝
- **那么** 接口矩阵必须分别记录其稳定拒绝语义，而不得继续合并成同一个错误描述

### 需求:本地 operator interface matrix 必须记录 Targets 页的 storage mode 流程与风险确认
当 `menuconfig` 的 Targets 页面提供 `Add Target` 流程时，接口矩阵必须明确记录该流程先选择 `plain/sensitive` 模式、再选择 target type 的顺序，以及 `plain + ssh` 需要显式风险确认的合同。

#### 场景:矩阵记录 vault locked 时的 Add Target
- **当** `menuconfig` 的 vault 为 `locked`，且 Targets 页面暴露 `Add Target`
- **那么** 接口矩阵必须明确记录此时只允许继续创建 plain target，sensitive target 需要先 unlock vault

#### 场景:矩阵记录 plain ssh 风险确认
- **当** `menuconfig` 允许在 plain 模式下创建 SSH target
- **那么** 接口矩阵必须明确记录该流程在进入字段编辑前要求正式风险确认，而不是把该风险只留在零散文案中

### 需求:本地 operator interface matrix 必须记录 sensitive target 的 reconcile 与删除门控
当 `menuconfig` 允许管理 sensitive target 时，接口矩阵必须明确记录 unlock 后 reconcile、locked 态删除裁剪以及 unlocked 态删除语义，而不是继续把 sensitive target 当作普通 config 行编辑入口。

#### 场景:矩阵记录 unlock 后 reconcile
- **当** `menuconfig` 的 vault 从 `locked` 进入 `unlocked`
- **那么** 接口矩阵必须明确记录系统会重新读取 vault authoritative sensitive target，并修复或刷新 `config.toml` 中的 public cache

#### 场景:矩阵记录 sensitive target 的删除前置条件
- **当** `menuconfig` 暴露 `Delete Target --->` 用于 sensitive target
- **那么** 接口矩阵必须明确记录该入口只在 vault `unlocked` 时可见，并且删除同时作用于 vault authoritative `target-profile` 与 `config.toml` public cache

### 需求:本地 operator interface matrix 必须记录 SSH key import/management 与 target binding 合同
当 `menuconfig` 产品化 SSH key 管理面时，接口矩阵必须明确记录 `Import SSH Key`、`SSH Key Management`、`Delete SSH Key` 与 `Credential Source` 流程的输入输出、状态门控与 display-safe 约束。

#### 场景:矩阵记录 Security 页 SSH key 状态裁剪
- **当** Security 页在 vault `locked` 与 `unlocked` 两种状态下展示 SSH key 入口
- **那么** 接口矩阵必须明确记录 `locked` 仅暴露聚合 `SSH Key Count`，`unlocked` 才开放导入与管理入口

#### 场景:矩阵记录 target credential source 输出
- **当** SSH target 通过 picker 或 inline import 绑定 imported vault key
- **那么** 接口矩阵必须明确记录流程输出只允许 canonical `credential_ref`
- **并且** 不得把私钥明文、passphrase 或非 canonical label 作为持久化输出

### 需求:本地安全管理接口矩阵必须记录 SSH key import / management 的正式合同
当本地 operator surface 为 `ssh-private-key` 提供 import、列表、详情与删除管理流时，接口矩阵必须明确记录这些接口的输入、输出、状态前置条件与 display-safe 约束，而不是继续只记载一个抽象的 `vault import` 或 generic secret count。

#### 场景:矩阵记录 SSH key import 的输入输出
- **当** 团队为本地 operator surface 新增 `Import SSH Key`
- **那么** 接口矩阵必须明确记录该动作至少包含 `key name`、`label`、source path 或等价本地 secret route、成功后的 canonical `credential_ref` / record-id 摘要，以及“不得回显私钥内容”的合同
- **并且** 必须明确记录重复 `key name` 会被拒绝，操作员需要先删除旧 key 再导入新 key

#### 场景:矩阵记录 SSH key delete-first 轮换流程
- **当** 团队在 SSH key 详情页提供 `Delete SSH Key`
- **那么** 接口矩阵必须明确记录该动作会删除当前 canonical ref，并清理所有绑定该 ref 的 target `credential_ref`
- **并且** 更新 key 的正式路径是删除后重新导入，而不是在详情页执行就地覆盖更新

### 需求:本地安全管理接口矩阵必须记录 SSH Key Management 的列表页与详情页拓扑
当 `menuconfig` 或其他 trusted local surface 为 `ssh-private-key` 提供管理页时，接口矩阵必须明确记录其列表页与详情页各自展示的字段、导航关系、锁状态门控与 display-safe 语义，而不是继续把其视为未定义的 future UI。

#### 场景:矩阵记录 SSH key 列表页摘要字段
- **当** `menuconfig` 的 `SSH Key Management` 采用摘要列表展示已导入 key
- **那么** 接口矩阵必须明确记录每一行至少包含 label、canonical ref 或等价稳定标识、status、record-id 摘要与 `--->` 导航入口

#### 场景:矩阵记录 SSH key 详情页字段
- **当** `menuconfig` 为单个 imported SSH key 提供统一详情页
- **那么** 接口矩阵必须明确记录该页面至少包含 canonical `credential_ref`、label、kind、status、record-id、last rotated / last used，以及 `Delete SSH Key --->` 等动作
- **并且** 不得把私钥明文或密文 locator 作为详情字段写入矩阵

#### 场景:矩阵记录 locked 状态下的 SSH key 聚合摘要
- **当** `menuconfig` 的 vault 状态为 `locked`
- **那么** 接口矩阵必须明确记录 SSH key 相关摘要最多只暴露聚合 `count`
- **并且** 不得在该状态下列出 label、canonical ref、status、record-id 或其他单个 key inventory 字段

### 需求:本地 operator interface matrix 必须记录授权日志与 flow 关联合同
当本地 operator surface 提供受信任授权动作时，接口矩阵必须明确记录这些动作的正式 `operation` 分类、日志落盘位置、display-safe 输出边界与 flow 关联语义，而不是只记录“某个页面有按钮”或“某个 CLI 命令会执行成功”。矩阵至少必须覆盖 `menuconfig` 与 standalone CLI 的高风险授权动作。

#### 场景:矩阵记录 menuconfig 授权日志输出
- **当** `menuconfig` 的 Security、SSH key 或 token 管理流会触发本地受信任授权动作
- **那么** 接口矩阵必须明确记录这些动作对应的 `operation` 分类、`logs/local-authorization.jsonl` 与 `logs/menuconfig-session.jsonl` 输出位置，以及事件只允许包含 display-safe 字段的合同

#### 场景:矩阵记录 standalone CLI 的等价授权语义
- **当** standalone CLI 暴露 `vault unlock`、`vault delete`、`token delete` 或等价高风险动作
- **那么** 接口矩阵必须明确记录这些命令与 `menuconfig` 共享同一套授权分类与 flow 语义
- **并且** 不得把 CLI 审计输出描述为与 TUI 完全独立的另一套合同

### 需求:本地 operator interface matrix 必须记录显式解锁的去重语义
当 `menuconfig` 或其他本地受信任 surface 触发 verified `os-native` 解锁时，接口矩阵必须明确记录“一次显式授权流程对应一次平台验证”的去重语义，以及 `leader/joined` 或等价 joined-flow 行为，而不是只记录“会触发系统验证”。

#### 场景:矩阵记录 menuconfig Unlock Vault 的单次触发语义
- **当** 接口矩阵记录 `menuconfig -> Security -> Unlock Vault`
- **那么** 矩阵必须明确记载同一进程内的近同时调用链会合并到同一次 verified `os-native` 验证
- **并且** 必须记录 joined 调用者不会额外触发第二次系统认证窗口

