## 新增需求

### 需求:Security 页面必须提供正式的 SSH key import 与管理入口
`bridgingio-core menuconfig` 的 Security 页面在 vault 已解锁时，必须提供 `Import SSH Key --->` 与 `SSH Key Management --->` 等正式入口，而不是继续只产品化 vault 状态和 token 管理。系统不得继续要求 operator 退出到外部 CLI 完成 SSH key import，再回到 menuconfig 手填 raw `credential_ref`。

#### 场景:vault unlocked 时进入 Security 页面
- **当** 操作员进入 Security 页面，且当前 vault 状态为 `unlocked`
- **那么** 系统必须展示 `Import SSH Key --->` 与 `SSH Key Management --->`
- **并且** 这些入口必须与 `Create Token --->`、`Token Management --->` 共同构成正式安全管理面，而不是仅作为帮助文本或未实现占位

#### 场景:vault locked 时进入 Security 页面
- **当** 操作员进入 Security 页面，且当前 vault 状态为 `locked`
- **那么** 系统不得允许进入 SSH key import 流程
- **并且** 系统最多只能展示聚合 `SSH Key Count` 或等价静态数量摘要
- **并且** 系统不得展示任何单个 imported SSH key 的 label、canonical `credential_ref`、status 或 record-id 摘要
- **并且** 系统不得把 `SSH Key Management` 伪装为可执行的完整管理入口后再在保存阶段报错

### 需求:menuconfig SSH key import 流程必须使用 display-safe metadata 与受控本地 secret capture
`menuconfig` 的 SSH key import 流程必须把普通可见 metadata 输入与实际私钥材料读取分离。系统必须允许 operator 通过 `key name`、`label`、`source path` 或等价 display-safe 字段组织导入流程，但不得把私钥内容或 passphrase 放进普通单行编辑弹窗、状态栏或常规列表行中。

#### 场景:操作员导入本地 SSH key 文件
- **当** 操作员在 menuconfig 中触发 `Import SSH Key --->`
- **那么** 系统必须先采集 display-safe metadata，再在受控本地流程中读取对应 SSH key 文件
- **并且** 导入成功后的结果页面只能展示 canonical `credential_ref`、label、status、record-id 等安全摘要，而不得再次展示私钥内容
- **并且** 若 canonical ref 已存在，系统必须拒绝重复导入并要求先删除旧 key

#### 场景:导入 encrypted SSH key
- **当** 操作员导入一个 passphrase-protected 的 SSH key 文件
- **那么** 系统必须通过受控本地隐藏输入处理该 passphrase
- **并且** 不得把该 passphrase 写入普通 popup 文本、日志、搜索结果或状态提示

### 需求:SSH Key Management 必须采用列表页 -> 详情页拓扑
`menuconfig` 必须为已导入的 `ssh-private-key` 提供正式的 `SSH Key Management` 列表页与详情页，而不是继续把这类对象隐藏在 generic secret count 或抽象的 vault summary 背后。

#### 场景:查看已导入 SSH key 列表
- **当** 操作员进入 `SSH Key Management`
- **那么** 系统必须以摘要列表展示每个已导入 key 的 label、canonical ref 或等价稳定标识、status 和 record-id 摘要
- **并且** 列表项必须使用 `--->` 导航语义进入详情页

#### 场景:查看 SSH key 详情页
- **当** 操作员进入某个已导入 SSH key 的详情页
- **那么** 系统必须至少展示 canonical `credential_ref`、label、kind、status、record-id 与 rotation / usage 摘要
- **并且** 详情页必须提供 `Delete SSH Key --->` 并要求二次确认
- **并且** 删除成功后必须清理所有绑定该 key 的 target `credential_ref`
- **并且** 详情页不得展示私钥明文、ciphertext locator、unwrap material 或其他非 display-safe 内部字段

#### 场景:vault locked 时不得浏览 SSH key 列表
- **当** 操作员处于 vault `locked` 状态
- **那么** 系统不得开放 `SSH Key Management` 的列表页或详情页浏览
- **并且** 不得通过 locked 态列表泄露任何单个 key inventory 信息

## 修改需求

### 需求:安全与生命周期页面必须只展示 display-safe 投影
`menuconfig` 在展示 vault、token、runtime root、lifecycle、诊断与 SSH key 摘要状态时，默认必须只使用 core 提供的 display-safe 投影。系统禁止在 Security 页面、SSH Key Management 列表页、普通详情页或 target 绑定 picker 中直接展示 secret 明文、长期 token 明文或其他高敏内部字段。

#### 场景:查看 SSH key 管理页
- **当** 操作员在 menuconfig 中浏览 `SSH Key Management` 列表页或详情页
- **那么** 系统只能展示 canonical ref、label、status、record-id 和等价 display-safe 摘要
- **并且** 不得把私钥内容、passphrase、ciphertext locator 或等价秘密材料带入 UI 文本

#### 场景:vault locked 时查看 Security 摘要
- **当** 操作员在 vault `locked` 状态下进入 Security 页面
- **那么** SSH key 相关摘要最多只能显示聚合数量
- **并且** 不得把单个 imported key 的 identity 信息作为“display-safe 摘要”带入 locked 态页面

#### 场景:SSH target 选择 imported vault key
- **当** 操作员在 SSH target 流程中选择一个已导入的 vault SSH key
- **那么** picker 或选择页必须只展示 display-safe key 摘要
- **并且** 不得把 underlying secret material 暴露给 target 编辑界面

## 移除需求
