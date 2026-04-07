## 新增需求

### 需求:SSH target 必须提供前置的认证设置流程
对于 `kind = ssh` 的 target，`menuconfig` 必须在字段详情编辑前提供正式的 `SSH Authentication Setup`，并在编辑现有 target 时提供 `SSH Authentication --->` 入口。该流程必须先确定认证类型与允许组合，再进入 host、port、username 等连接细节编辑，而不得继续把认证约束留到 `Create/Apply` 时才失败。

#### 场景:创建 SSH target 时先进入认证设置
- **当** 操作员在 `Add Target` 流程中选择 `SSH`
- **那么** 系统必须在进入 SSH detail editor 前先进入 `SSH Authentication Setup`
- **并且** 该步骤必须允许选择 `none`、`password` 或 `private-key`

#### 场景:编辑 plain 或 sealed SSH target 时存在认证入口
- **当** 操作员编辑一个已存在的 SSH target
- **那么** plain SSH 必须在 `Connection Profile` 中提供 `SSH Authentication --->`
- **并且** unlocked sealed SSH 必须在 `Sensitive Overlay` 中提供 `SSH Authentication --->`

#### 场景:认证类型选择必须使用互斥单选交互
- **当** 操作员在 `SSH Authentication Setup` 中调整 `none` / `password` / `private-key`
- **那么** 系统必须保证三者互斥且必须有且仅有一个被选中
- **并且** `Space` 必须用于切换选择，`Enter` 必须用于进入详情编辑，不得把 `Enter` 作为切换触发键

#### 场景:继续进入详情编辑必须受认证前置条件门控
- **当** 当前认证类型为 `password` 或 `private-key` 且必填字段未完成或校验未通过
- **那么** `继续进入详情编辑` 必须保持禁用
- **并且** 禁用提示必须明确说明“需要填写并通过校验”的阻断原因

#### 场景:plain secret-backed 认证显示可切换的 SSH 安全访问
- **当** plain SSH target 选择 `password` 或本地未加密 `private-key`
- **那么** 系统必须显示 `SSH 安全访问` 的单选 toggle
- **并且** 该 toggle 必须默认开启，但允许明确关闭

#### 场景:sealed secret-backed 认证强制 SSH 安全访问
- **当** sealed SSH target 选择 `password`、本地未加密 `private-key` 或 imported vault key
- **那么** 系统必须把 `SSH 安全访问` 显示为只读的必选状态
- **并且** 不得允许操作员将其关闭

#### 场景:本地私钥路径带 passphrase 时阻断并提示 sealed + vault import
- **当** 操作员在 `SSH Authentication Setup` 中输入本地私钥路径，且系统检测到该私钥带 passphrase
- **那么** 系统必须立即弹出阻断性错误提示
- **并且** 该提示必须明确说明该组合只允许通过 sealed target + vault import 支持

#### 场景:plain private-key 认证不得展示 vault key 来源
- **当** plain SSH target 在 `SSH Authentication Setup` 选择 `private-key`
- **那么** 系统不得展示 `Use Imported Vault Key` 或等价 vault 来源入口
- **并且** 同屏不得出现语义重复的本地 key 来源行

## 修改需求

### 需求:Add Target 必须先选择 storage mode 再选择 target type
`menuconfig` 的 `Add Target` 流程必须先让操作员在 `plain` 与 `sensitive` 之间做出明确选择，再进入受支持 target 类型的选择与后续详情编辑。对于 `kind = ssh` 的 target，系统必须在 target type 选择之后、detail editor 之前插入正式的 `SSH Authentication Setup` 步骤。

#### 场景:操作员开始创建 target
- **当** 操作员在 Targets 页面触发 `Add Target --->`
- **那么** 系统必须先进入 `Choose Storage Mode` 或等价步骤，并提供 `Plain Target --->` 与 `Sensitive Target --->` 两个入口

#### 场景:vault locked 时创建 target
- **当** 操作员在 vault 为 `locked` 的状态下进入 `Choose Storage Mode`
- **那么** 系统必须只允许继续进入 `Plain Target --->`
- **并且** `Sensitive Target` 必须显示为禁用或引导到 `Unlock Vault --->`

#### 场景:操作员选择 plain ssh target
- **当** 操作员选择 `Plain Target` 并继续选择 `SSH`
- **那么** 系统必须在进入字段编辑前显示正式风险确认
- **并且** 该确认必须明确说明相关连接描述字段与 plain password 可能保留在 `config.toml`
- **并且** 在确认后系统必须先进入 `SSH Authentication Setup`，而不是直接进入字段编辑

#### 场景:操作员选择 sensitive ssh target
- **当** 操作员在 unlocked 状态下选择 `Sensitive Target` 并继续选择 `SSH`
- **那么** 系统必须先进入 `SSH Authentication Setup`
- **并且** 只有在认证组合校验通过后，系统才可以进入 SSH detail editor

## 移除需求
