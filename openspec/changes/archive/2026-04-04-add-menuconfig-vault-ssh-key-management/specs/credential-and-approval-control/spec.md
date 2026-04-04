## 新增需求

### 需求:受信任本地管理面必须把 SSH 私钥导入建模为正式的 `ssh-private-key` import 流程
BridgingIO 的受信任本地管理面必须允许把 SSH 私钥作为正式的 `ssh-private-key` secret family 导入 canonical vault，而不是继续只把它当作 generic text blob 或要求 target 侧手工拼接引用。该 import 流程必须输出稳定 canonical `vault://<namespace>/ssh-private-key/<name>` 引用；底层 secret 模型保留 version 记录能力，但 menuconfig 的默认运维流程应使用 delete-first 再导入更新 key。

#### 场景:导入新的 SSH key
- **当** 本地受信任管理面导入一个新的 SSH 私钥，并提供 `key name` 与等价 operator-facing label
- **那么** 系统必须把该对象保存为 canonical `ssh-private-key` secret，并返回 canonical `credential_ref`
- **并且** 后续 target 配置只能引用该 canonical ref，而不得保存私钥明文

#### 场景:底层接口为同一 canonical ref 导入新版本
- **当** 本地受信任管理接口为一个已存在的 `vault://.../ssh-private-key/<name>` 再次导入新的 SSH 私钥内容
- **那么** 系统必须在同一 secret record 下创建新的 version，并更新 active version
- **并且** 不得要求所有引用该 ref 的 target 改写为新的 URI

#### 场景:menuconfig 默认执行 delete-first 更新
- **当** 操作员在 menuconfig 中尝试用同一 `key name` 重复导入 SSH key
- **那么** 系统必须拒绝重复导入并提示先删除旧 key
- **并且** 详情页应提供 `Delete SSH Key` 二次确认流程，供操作员删除后重新导入

### 需求:SSH key import 的 display-safe metadata 与 secret material 必须分层处理
SSH key import 必须把 operator-facing metadata 与实际 secret material 分层处理。`key name`、`label`、`source path`、kind、status、record-id 与 rotation 摘要可以进入 trusted local 管理面的 display-safe 交互；私钥材料、passphrase、ciphertext locator 与 unwrap material 则不得进入普通 UI 字段、普通 CLI 输出、日志或搜索结果。

#### 场景:本地管理面展示导入结果
- **当** 受信任本地管理面完成一次 SSH key import
- **那么** 结果输出只能包含 canonical `credential_ref`、label、status、record-id 或等价 display-safe 摘要
- **并且** 不得把 SSH key 内容或 passphrase 作为“导入成功结果”的一部分返回

#### 场景:列出已导入的 SSH key
- **当** 受信任本地管理面列出当前 vault 中已导入的 SSH key
- **那么** 系统只能返回 canonical `credential_ref`、label、kind、status、record-id 与等价 display-safe 字段
- **并且** 不得把密文定位信息、key 明文或 passphrase 相关材料暴露给普通管理列表

## 修改需求

### 需求:受保护的 SSH key import 禁止把 key passphrase 重新带入运行时数据面
系统必须确保：若系统允许导入 passphrase-protected 的 SSH 私钥，则该 passphrase 只能在本地受信任的导入或管理流程中使用。运行时的 SSH broker、connector、target 绑定 picker 或普通 MCP/tool 路径不得再要求该 passphrase，且不得把它转写到 argv、prompt transcript、artifact、menuconfig 状态文本或日志中。

#### 场景:menuconfig 导入受 passphrase 保护的 OpenSSH 私钥
- **当** 本地管理员通过 menuconfig 导入一个带 passphrase 的 OpenSSH 私钥
- **那么** 系统必须在本地受控流程中完成该 passphrase 的输入与校验，并把导入结果写入 vault canonical secret
- **并且** 后续运行时连接不得再次要求 target、agent 或普通工具知道该 passphrase

#### 场景:SSH target 使用已导入的 encrypted key
- **当** 某个 SSH target 绑定了一个曾在导入时处理过 passphrase 的 `ssh-private-key`
- **那么** 运行时 SSH broker 只能消费 vault 保护下的 signer material
- **并且** 不得退化为再次提示 key passphrase 或把其暴露给运行时数据面

## 移除需求
