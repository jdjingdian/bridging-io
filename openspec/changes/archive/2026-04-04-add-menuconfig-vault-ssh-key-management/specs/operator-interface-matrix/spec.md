## 新增需求

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

## 修改需求

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

## 移除需求
