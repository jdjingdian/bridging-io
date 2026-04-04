## 新增需求

### 需求:SSH target 的凭据绑定流程必须支持选择已导入的 vault SSH key
当本地 operator 在受信任管理面中编辑 SSH target 时，系统必须允许其通过正式选择流程绑定已导入的 `ssh-private-key`，而不是继续把“手工输入 raw `credential_ref`”作为使用 vault-managed SSH key 的主要路径。该流程必须最终把 canonical `vault://...` 引用写入 target 配置，而不是把 key material 写回 profile。

#### 场景:操作员为 SSH target 选择 imported key
- **当** 操作员在 menuconfig 中编辑一个 SSH target，并选择使用某个已导入的 vault SSH key
- **那么** 系统必须提供 display-safe 的 key picker 或等价子流程
- **并且** target 落盘结果必须只保存被选中的 canonical `credential_ref`

#### 场景:操作员为 sensitive ssh target 选择 imported key
- **当** 操作员在 menuconfig 中编辑一个 `kind = ssh` 的 sensitive target，且当前 vault 已 `unlocked`
- **那么** 系统必须在 `Sensitive Overlay` 中提供等价的 imported key picker / 绑定子流程
- **并且** 最终写回结果必须进入 sensitive overlay，而不得复制进 public cache

#### 场景:操作员在 target 流程内联导入本地 key
- **当** 操作员在 SSH target 的凭据绑定流程中发现当前没有合适的 imported key
- **那么** 系统必须允许其进入受控的 `Import Local SSH Key Into Vault` 或等价子流程
- **并且** 导入成功后必须能够把新生成的 canonical `credential_ref` 回填到当前 target，而不是要求用户返回后手工输入 URI

#### 场景:locked 的 sensitive ssh target 不得暴露已绑定 key 身份
- **当** 某个 `kind = ssh` 的 sensitive target 当前仍依赖 vault authoritative overlay，且 vault 状态为 `locked`
- **那么** 系统不得在 target 列表、target detail 或任何等价投影中暴露所绑定 imported key 的 label、canonical `credential_ref`、status 或 record-id
- **并且** 不得在该状态下开放 imported key picker

## 修改需求

### 需求:standalone 的 vault / auth 管理入口必须与 `run` 分离并避免明文 argv
standalone 模式下，系统必须为 vault 初始化、SSH key 导入、vault 解锁、agent token 创建和 revoke 等管理动作提供显式的本地管理入口，例如独立 CLI 子命令、menuconfig 安全管理页或等价的受信任本地 control-plane 动作。该管理入口必须支持通过受控本地文件读取、prompt 或等价 secret route 传递 SSH key 材料，并且不得要求操作员通过命令行参数直接传入 secret 明文。

#### 场景:操作员在 menuconfig 中导入 SSH 私钥
- **当** 操作员在 standalone 环境的 menuconfig 中为某个 target 准备导入 SSH 私钥
- **那么** 系统必须允许其通过受控本地导入流程完成该操作，而不能要求其切换到“把私钥内容粘到普通文本字段”或 `--private-key <plaintext>` 之类的 argv 明文方式

#### 场景:target 绑定已导入 key 后保存配置
- **当** 操作员为 SSH target 选中了某个已导入 vault key 并保存配置
- **那么** standalone 配置文件中必须只保存 canonical `credential_ref`
- **并且** 不得因为 target 绑定流程而把私钥明文、passphrase 或其他 secret material 写入配置文件

## 移除需求
