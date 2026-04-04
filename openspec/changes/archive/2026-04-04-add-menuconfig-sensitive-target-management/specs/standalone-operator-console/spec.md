## 新增需求

### 需求:Targets 页面必须按 vault 锁状态裁剪 sensitive target 管理动作
`bridgingio-core menuconfig` 的 Targets 页面必须根据当前 vault 的真实锁状态裁剪 sensitive target 的创建、编辑和删除入口，而不是允许操作者先进入 sensitive 流程、最后在保存时才发现 vault 当前不可用。系统禁止用“点进去再报错”的弱门控替代正式状态裁剪。

#### 场景:vault locked 时进入 Targets 页面
- **当** 操作员进入 Targets 页面，且当前 vault 状态为 `locked`
- **那么** 系统必须继续显示 plain target 与 sensitive target 的 public cache 摘要
- **并且** 系统必须允许继续创建 plain target
- **并且** 系统不得允许创建、编辑或删除 sensitive target

#### 场景:vault unlocked 时进入 sensitive target 详情页
- **当** 操作员进入一个**已存在**的 sensitive target 的详情页，且当前 vault 状态为 `unlocked`
- **那么** 系统必须提供 `Public Descriptor --->`、`Sensitive Overlay --->`、`Policy --->`、`Apply Target --->` 与 `Delete Target --->` 等正式管理入口，而不是继续停留在只读摘要状态
- **并且** 若该 target 的 kind 为 `ssh`，`Sensitive Overlay` 中必须允许进入正式的 `Credential Source --->` 绑定流程

#### 场景:vault locked 时进入 sensitive target 详情页
- **当** 操作员进入一个 sensitive target 的详情页，且当前 vault 状态为 `locked`
- **那么** 系统必须只展示 public cache 与锁定提示
- **并且** 系统必须提供 `Unlock Vault --->` 或等价入口
- **并且** 系统不得显示 sensitive overlay 编辑入口或删除入口
- **并且** 若该 target 的 kind 为 `ssh`，系统不得显示当前绑定 imported key 的 label、canonical `credential_ref` 或等价 inventory 信息

### 需求:Add Target 必须先选择 storage mode 再选择 target type
`menuconfig` 的 `Add Target` 流程必须先让操作员在 `plain` 与 `sensitive` 之间做出明确选择，再进入受支持 target 类型的选择与后续详情编辑，而不是在同一层把 storage mode、target type 和具体字段编辑混在一起。

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
- **并且** 该确认必须明确说明相关连接描述字段将保留在 `config.toml`

### 需求:Target 创建与管理必须区分会话动作并采用显式 `Create/Apply`
`menuconfig` 的 target 详情页必须区分“创建会话”和“管理会话”。系统不得在用户仅选择 target 类型后就把该 target 视为已创建完成；必须通过显式 `Create Target --->` 或 `Apply Target --->` 动作确认当前会话结果。离开会话时若存在未提交内容，必须先确认是否丢弃。

#### 场景:创建会话进入 target 详情页
- **当** 操作员从 `Add Target` 流程进入某个新 target 详情页
- **那么** 系统必须提供 `Create Target --->` 作为会话提交动作
- **并且** 系统不得在该创建会话里显示 `Delete Target --->`

#### 场景:管理会话进入 target 详情页
- **当** 操作员从现有 target 列表进入某个已存在 target 详情页
- **那么** 系统必须提供 `Apply Target --->` 与 `Delete Target --->`
- **并且** 仅当操作者触发 `Apply Target --->` 时，当前会话编辑结果才视为已提交

#### 场景:创建会话未提交即离开
- **当** 操作员在创建会话中修改了 target 字段，但未触发 `Create Target --->` 就按 `Esc` 或触发 `Exit`
- **那么** 系统必须先弹出“未创建 target 草稿是否丢弃”的确认
- **并且** 若确认丢弃，系统必须删除该草稿 target 并返回上一级

#### 场景:管理会话未提交即离开
- **当** 操作员在管理会话中修改了 target 字段，但未触发 `Apply Target --->` 就按 `Esc` 或触发 `Exit`
- **那么** 系统必须先弹出“未应用变更是否丢弃”的确认
- **并且** 若确认丢弃，系统必须回滚该 target 到进入详情页前的基线状态

### 需求:Targets 详情页必须按 public descriptor 与 sensitive overlay 分段组织
`menuconfig` 的 Targets 详情页必须继续遵守单栏逐级进入的 `menuconfig` 拓扑，但对于 target 字段组织，系统必须把 `public descriptor`、`sensitive overlay`、`policy` 和 `delete` 区分为独立子页或独立管理段，而不是把所有字段长期堆叠在一个平面列表中。

#### 场景:查看 plain target 详情页
- **当** 操作员进入一个 plain target 的详情页
- **那么** 系统必须至少提供 `Public Descriptor --->`、`Connection Profile --->` 与 `Policy --->` 等分段入口，而不是把所有字段混为一个无分段的长列表

#### 场景:查看 sensitive target 详情页
- **当** 操作员进入一个 sensitive target 的详情页
- **那么** 系统必须把 public descriptor 与 sensitive overlay 视为不同层次的对象
- **并且** 系统不得在同一普通列表层把两类字段无差别平铺

#### 场景:查看 unlocked 的 sensitive ssh target
- **当** 操作员进入一个 `kind = ssh` 的 sensitive target，且当前 vault 已 `unlocked`
- **那么** `Sensitive Overlay` 子页必须允许通过 `Credential Source --->` 选择 imported vault SSH key 或等价正式绑定流程
- **并且** imported key picker 列表必须使用 `Single-choice Toggle` 行语义（`< >` / `<*>`）
- **并且** 对 picker 当前行必须仅允许 `Space` 触发绑定，`Enter` 不得触发绑定
- **并且** 最终写回结果必须仍然属于 sensitive overlay，而不是 public descriptor

## 修改需求

### 需求:安全与生命周期页面必须只展示 display-safe 投影
`menuconfig` 在展示 vault、token、runtime root、lifecycle、诊断与 sensitive target 摘要状态时，默认必须只使用 core 提供的 display-safe 投影。系统禁止在 Targets 列表、locked 状态的 sensitive detail、普通搜索结果或重复进入的管理页面中直接展示 secret 明文、长期 token 明文或其他高敏内部字段。唯一允许的例外仍然是显式创建 token 后的一次性结果弹窗。

#### 场景:用户在 vault locked 时查看 sensitive target
- **当** 操作员在 `menuconfig` 中查看一个当前依赖 vault 的 sensitive target，且 vault 仍为 `locked`
- **那么** 系统必须只展示 public cache、锁定状态与等价的 display-safe 诊断
- **并且** 不得显示 host、username、selector、notes、credential_ref 或等价 sensitive overlay 字段
- **并且** 对 `kind = ssh` 的 sensitive target，系统不得借由任何“已绑定凭据”文案泄露 imported key 的 label、canonical ref 或等价 inventory

#### 场景:用户在 vault unlocked 时查看 sensitive target
- **当** 操作员在 `menuconfig` 中查看一个 sensitive target，且 vault 已 `unlocked`
- **那么** 系统可以在受控的 detail 子页中展示该 target 的 resolved sensitive overlay
- **但是** 普通列表与普通摘要仍必须保持 display-safe，不得把 vault 内部 payload 原样暴露

## 移除需求
