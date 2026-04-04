## 新增需求

### 需求:本地 operator interface matrix 必须记录 Targets 页的 storage mode 流程与风险确认
当 `menuconfig` 的 Targets 页面提供 `Add Target` 流程时，接口矩阵必须明确记录该流程先选择 `plain/sensitive` 模式、再选择 target type 的顺序，以及 `plain + ssh` 需要显式风险确认的合同，而不是只写一个抽象的“新增 target”入口。

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

#### 场景:矩阵记录 sensitive ssh target 的 credential binding
- **当** `menuconfig` 为一个 `kind = ssh` 的 sensitive target 暴露 `Sensitive Overlay --->`
- **那么** 接口矩阵必须明确记录 `Credential Source --->` 只在 vault `unlocked` 时可见，并且最终写回的是 sensitive overlay 中的 canonical `credential_ref`

#### 场景:矩阵记录 locked 态 sensitive ssh target 的信息裁剪
- **当** vault 仍为 `locked`，且操作员查看一个 `kind = ssh` 的 sensitive target
- **那么** 接口矩阵必须明确记录该页面不得显示 imported key 的 label、canonical ref、status 或 version，只能显示 generic lock/protected 诊断

## 修改需求

### 需求:本地 operator surfaces 必须维护正式的接口矩阵文档
BridgingIO 必须维护一份正式的本地 operator interface matrix，用于记录暴露给 UI、TUI、standalone 管理面和受信任本地调用方的接口真相。该矩阵至少必须覆盖命令或接口名称、调用方范围、输入、输出、状态、错误码与 apply strategy。对于 vault、token 与 sensitive target 这类安全管理接口，矩阵还必须额外记录状态前置条件、是否需要二次确认、是否涉及一次性 reveal、是否存在 unlock 后 reconcile，以及 destructive action 的恢复语义。

#### 场景:新增或修改 sensitive target 管理流程
- **当** 团队为本地 operator surface 新增或修改 sensitive target 的 create/edit/delete/reconcile 路径
- **那么** 系统必须同步更新接口矩阵文档，记录该路径的输入输出合同、状态门控与恢复语义，而不是只修改实现代码

#### 场景:接口行为涉及 unlock 后修复
- **当** 某个本地 operator 接口在 vault unlock 后会触发 sensitive target reconcile、public cache 修复或等价恢复动作
- **那么** 接口矩阵必须明确记录该行为，而不是把这类恢复语义留作未文档化的内部实现细节

## 移除需求
