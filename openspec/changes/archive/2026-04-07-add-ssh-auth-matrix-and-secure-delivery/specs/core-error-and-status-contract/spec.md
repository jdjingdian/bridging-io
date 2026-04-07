## 新增需求

### 需求:SSH 认证矩阵失败必须暴露稳定的公共错误子码
当公共调用面、menuconfig 或 standalone runtime 因 SSH auth matrix 校验、delivery plan 建立或 carrier 生命周期问题而失败时，系统必须返回共享结构化错误封装，并暴露稳定的 SSH 模块子码。系统不得要求调用方通过 message 文本猜测“是组合非法、需要导入 vault，还是 secure delivery 不可用”。

#### 场景:非法的 auth 与 storage 组合
- **当** 系统遇到 plain + local encrypted key、plain + vault-managed key、sealed + secret-backed auth 关闭 `SSH 安全访问`，或等价不允许的 SSH 认证组合
- **那么** 系统必须返回共享错误封装
- **并且** 必须暴露稳定的 SSH 模块子码以指示该失败属于 `auth-combination-disallowed` 或等价 canonical 语义

#### 场景:本地私钥带 passphrase 但未导入 vault
- **当** 系统检测到操作员尝试以本地路径直接使用一个带 passphrase 的 SSH 私钥
- **那么** 系统必须返回共享错误封装
- **并且** 必须暴露稳定的 SSH 模块子码以指示 `local-key-passphrase-requires-vault-import` 或等价 canonical 语义

#### 场景:password secure delivery 无法建立
- **当** target 登录 password 的 managed carrier、helper、env overlay 或等价 secure delivery 机制无法建立或被策略拒绝
- **那么** 系统必须返回共享错误封装
- **并且** 必须暴露稳定的 SSH 模块子码以指示 `password-delivery-unavailable`、`password-delivery-rejected` 或等价 canonical 语义

#### 场景:SSH 认证矩阵错误保持 display-safe
- **当** SSH auth matrix 相关失败对外返回错误对象
- **那么** 公共错误中的 message、recovery hint 与 details 必须保持 display-safe
- **并且** 不得包含 password 明文、私钥明文、本地私钥真实内容、raw helper payload 或等价敏感字段

## 修改需求

## 移除需求
