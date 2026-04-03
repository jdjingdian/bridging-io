## 新增需求

### 需求:Security 页面必须按 vault 状态裁剪动作
`bridgingio-core menuconfig` 的 Security 页面必须根据当前 vault 的真实状态裁剪可见动作，而不是在 `uninitialized`、`locked`、`unlocked` 等状态下长期展示同一组管理入口。系统禁止依赖“先把所有入口都列出来，再在点按后报错”的弱门控方式替代正式状态机。

#### 场景:vault 未初始化时进入 Security 页面
- **当** 操作员进入 Security 页面，且当前 vault 状态为 `uninitialized`
- **那么** 系统必须显示 `uninitialized` 或等价状态说明，并且只提供 `Init Vault --->` 或等价初始化入口，而不得显示 `Unlock Vault`、`Create Token` 或 `Token Management`

#### 场景:vault 已初始化但未解锁时进入 Security 页面
- **当** 操作员进入 Security 页面，且当前 vault 状态为 `locked`
- **那么** 系统必须显示 `locked` 或等价状态说明，并且只提供 `Unlock Vault --->` 或等价解锁入口，而不得继续显示 `Init Vault`、`Create Token` 或 `Token Management`

#### 场景:vault 已解锁时进入 Security 页面
- **当** 操作员进入 Security 页面，且当前 vault 状态为 `unlocked`
- **那么** 系统必须显示 `vault unlocked` 或等价已解锁提示，并展示 `Create Token --->` 与 `Token Management --->` 入口，而不得继续显示 `Init Vault` 或 `Unlock Vault`

### 需求:menuconfig 必须提供显式 vault init/delete 管理路径
`bridgingio-core menuconfig` 必须把 vault 的初始化与删除建模为正式管理动作，而不是要求操作者手工删除目录、重启程序或依赖隐式 bootstrap。系统必须确保 `vault delete` 后重新回到 `uninitialized`，并且所有高风险删除动作都必须具备二次确认。

#### 场景:操作员从未初始化状态执行 init
- **当** 操作员在 Security 页面触发 `Init Vault`，且当前 vault 为 `uninitialized`
- **那么** 系统必须完成当前 runtime store 的正式 vault 初始化，并在成功后把 Security 页面状态更新为 `locked` 或等价的“已初始化待解锁”状态

#### 场景:操作员删除当前 vault
- **当** 操作员在已初始化的 Security 页面中触发 `Delete Vault` 并完成二次确认
- **那么** 系统必须删除当前实例的 vault 持久化数据，并把 Security 页面重新切换为 `uninitialized` 状态，同时移除 token 管理入口

#### 场景:vault 缺失时不得走隐藏 unlock fallback
- **当** 当前 vault 持久化数据不存在，且操作员尚未执行 `Init Vault`
- **那么** `menuconfig` 不得通过默认内存 router、隐藏 bootstrap 或其他回退路径继续完成 unlock，而必须保持 `uninitialized`

### 需求:Token Management 子页必须支持备注编辑与带确认的 revoke/delete
当 Security 页面暴露 `Token Management` 后，`menuconfig` 必须提供一个独立的 token 管理子页，用于展示 token 安全摘要、编辑备注字段以及执行 revoke/delete。系统必须要求 revoke/delete 经过二次确认，并且 `delete` 只能对 `revoked` token 生效。

#### 场景:操作员编辑 token 备注
- **当** 操作员进入 `Token Management` 子页并修改某个 token 的备注
- **那么** 系统必须允许其更新该 token 的 operator-facing 备注字段，并在返回列表后继续以 display-safe 方式展示更新后的备注，而不得要求重新签发 token

#### 场景:操作员撤销 token
- **当** 操作员在 `Token Management` 子页中请求撤销一个尚未删除的 token
- **那么** 系统必须先显示 revoke 二次确认，并且只有在操作员确认后才将该 token 置为 `revoked`

#### 场景:expired token 不能直接 delete
- **当** 操作员在 `Token Management` 子页中选中一个 `expired` token 并尝试删除
- **那么** 系统必须拒绝直接 delete，并明确提示该 token 必须先进入 `revoked` 状态后才能删除

#### 场景:revoked token 删除必须二次确认
- **当** 操作员在 `Token Management` 子页中请求删除一个 `revoked` token
- **那么** 系统必须显示 delete 二次确认，并且只有在操作员确认后才允许该 token 进入 `deleted` 终态或等价的已删除管理状态

### 需求:流程入口动作必须使用一致的导航样式
`menuconfig` 中会进入下一级页面、触发多步流程或弹出确认/结果弹窗的动作，必须统一使用与导航一致的 `--->` 样式，避免与即时切换类动作的 `*** ... ****` 样式混用导致误解。

#### 场景:Security 页面展示流程入口动作
- **当** 操作员在 Security 页面看到 `Create Token`、`Token Management`、`Delete Vault` 或等价流程入口
- **那么** 这些动作必须使用 `--->` 导航样式，而不得渲染为 `*** ... ****` 的即时动作样式

#### 场景:Token Management 子页展示编辑/撤销/删除动作
- **当** 操作员在 Token Management 子页查看 `Edit Label`、`Revoke Token`、`Delete Token` 或等价动作
- **那么** 这些动作必须使用 `--->` 导航样式，以表明其会进入编辑流程或确认链路

### 需求:`Create Token` 必须采用引导式创建并明确本机时间边界
当操作员在已解锁的 Security 页面中触发 `Create Token` 时，`menuconfig` 必须按“label -> 有效期 -> 一次性结果”的顺序引导创建，而不是把高敏字段与结果展示混杂在同一个长期停留的列表页中。对于指定时间失效的 token，界面必须明确展示当前本机时间与时区，并在时钟异常时给出限制或强提示。

#### 场景:操作员先输入 token 别名
- **当** 操作员触发 `Create Token`
- **那么** 系统必须先弹出输入框要求填写 token 别名（`label`），并在别名为空时拒绝继续下一步

#### 场景:操作员选择长期 token
- **当** 操作员已确认 token 别名，并在有效期步骤中选择 `长期有效`
- **那么** 系统必须允许继续创建，并把该 token 视为无过期时间限制的长期 token

#### 场景:操作员选择指定时间失效
- **当** 操作员已确认 token 别名，并在有效期步骤中选择 `有效至指定时间`
- **那么** 系统必须展示当前本机时间与时区，要求操作员选择一个未来失效时间，并明确该时间按本机时钟解释

#### 场景:本机时钟异常时创建 expiring token
- **当** runtime 已检测到明显的时钟回拨、时钟健康异常或等价的不可信本机时间状态
- **那么** `Create Token` 流程必须阻止或强警告 `有效至指定时间` 选项，并要求操作员先修复本机时间或改为创建长期 token

#### 场景:token 创建成功后一次性展示明文
- **当** 操作员完成 `Create Token` 并创建成功
- **那么** 系统必须展示包含明文 token 的一次性结果弹窗，提示操作员自行保存；关闭该弹窗后，系统不得再从普通摘要页面重新显示该明文

#### 场景:Create Token 弹窗字段文案必须面向操作员
- **当** 操作员在 `Create Token` 流程中进入 label/有效期等编辑弹窗
- **那么** 系统必须展示可读的 operator-facing 字段标题（例如“创建 Token 别名”），而不得直接暴露内部字段标识（例如 `__token_create_label__`）

## 修改需求

### 需求:安全与生命周期页面必须只展示 display-safe 投影
`menuconfig` 在展示 vault、token、runtime root、lifecycle 和诊断状态时，默认必须只使用 core 提供的 display-safe 投影。系统禁止在普通状态页、普通列表、搜索结果或重复进入的管理页面中直接展示 secret 明文、长期 token 明文或其他高敏内部字段。唯一允许的例外是：当操作员在受信任本地管理面中显式执行 `Create Token` 且本次签发成功后，系统可以通过独立的一次性结果弹窗承接该次响应中的明文 token；关闭该弹窗后，系统不得再从普通摘要路径重新回显该明文 token。

#### 场景:用户查看 Vault 状态
- **当** 操作员在 `menuconfig` 中进入 vault、Security 或 Token Management 页面
- **那么** 系统必须展示 lock state、策略摘要、token 备注、token 状态或等价 display-safe 字段，而不得在普通列表中回显 secret 或长期 token 明文

#### 场景:操作员显式创建 token
- **当** 操作员在已解锁的 Security 页面中显式触发 `Create Token` 并成功创建一个长期 token
- **那么** 系统必须展示一个包含该次明文 token 的一次性结果弹窗，并在关闭或离开后恢复为只显示 display-safe token 摘要，而不得从普通列表再次回显该明文 token

## 移除需求
