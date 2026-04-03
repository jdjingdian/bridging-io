## 新增需求

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

#### 场景:矩阵记录 uninitialized 状态下的 Security 页
- **当** `menuconfig` 的 Security 页面在 `uninitialized` 状态下只暴露 `Init Vault`
- **那么** 接口矩阵必须明确记载该状态下不显示 `Unlock Vault`、`Create Token`、`Token Management`

#### 场景:矩阵记录 unlocked 状态下的 Security 页
- **当** `menuconfig` 的 Security 页面在 `unlocked` 状态下暴露 `Create Token` 与 `Token Management`
- **那么** 接口矩阵必须明确记载这些入口的可见条件，以及普通列表继续遵守 display-safe projection 的限制

## 修改需求

### 需求:本地 operator surfaces 必须维护正式的接口矩阵文档
BridgingIO 必须维护一份正式的本地 operator interface matrix，用于记录暴露给 UI、TUI、standalone 管理面和受信任本地调用方的接口真相。该矩阵至少必须覆盖命令或接口名称、调用方范围、输入、输出、状态、错误码与 apply strategy。对于 vault 与 token 这类安全管理接口，矩阵还必须额外记录状态前置条件、是否需要二次确认、是否涉及一次性 reveal 结果，以及 destructive action 的恢复语义。

#### 场景:新增本地 control-plane 命令
- **当** 团队新增或修改一个面向本地 operator surface 的 command、事件或设置写入接口
- **那么** 系统必须同步更新接口矩阵文档，记录该接口的输入输出合同、适用调用方、状态/错误语义，以及必要的状态门控或确认要求，而不是只修改实现代码

#### 场景:接口行为涉及重启或受控未实现
- **当** 某个本地 operator 接口存在 `restart_required`、`not_ready`、`method_not_implemented`、一次性 reveal 或 destructive confirmation 等正式语义
- **那么** 接口矩阵必须明确记录这些状态与对应恢复动作，而不是仅靠 README 叙述或代码注释隐式表达

## 移除需求
