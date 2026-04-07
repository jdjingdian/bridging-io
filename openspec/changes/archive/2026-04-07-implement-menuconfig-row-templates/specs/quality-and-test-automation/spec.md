## 新增需求

### 需求:menuconfig 模板迁移必须具备完整性测试护栏
当团队把现有 `menuconfig` 行切换到共享模板组件时，自动化测试除了验证模板合同本身，还必须验证“当前所有可正式映射的行都已经迁移完成”。测试不得只覆盖一两个样例页面，而让其余仍可迁移的行继续停留在旧路径中。

#### 场景:模板迁移覆盖当前可迁移行族
- **当** 自动化测试验证 menuconfig row template migration
- **那么** 测试必须覆盖当前 catalog 中已被项目正式采用的各类行族
- **并且** 至少包含 `boolean toggle`、`multi-select`、`exclusive choice`、`exclusive choice entry`、`blocked action`、`required readonly`、`fixed enabled`、`field entry`、`action` 与 `info` 的代表性实例

#### 场景:只有已登记例外允许留在旧路径
- **当** 自动化测试检查当前 menuconfig screen builders 的迁移状态
- **那么** 测试必须能够识别哪些行仍属于“尚无正式模板的复合例外”
- **并且** 除这些已登记例外外，其余可正式映射的行不得继续停留在旧的拼装路径

#### 场景:高频 input / choice popup 迁移也受完整性约束
- **当** 自动化测试检查当前 menuconfig popup migration
- **那么** 测试必须覆盖所有可正式映射到 `text-input modal` 与 `choice-list modal` 的现有弹窗
- **并且** 不得只迁移其中一个流程样例而让其他输入/选择弹窗继续停留在手写布局路径

## 修改需求

### 需求:menuconfig 的模板合同必须具备自动化回归覆盖
针对 `menuconfig` 的自动化或 contract 测试，系统必须覆盖 row template、group template 与 popup template 的核心合同，而不是只验证某个具体页面“看起来差不多”。测试必须能够回答：某类模板是否稳定渲染、是否使用了正确键位语义、是否保持了组内约束，以及当实现偏离模板合同后能否被及时发现；对于现有行迁移，还必须能够验证模板组件落地的完整性。

#### 场景:模板迁移后渲染不再依赖隐式语义推断
- **当** 某个已迁移 row family 经过自动化测试渲染
- **那么** 测试必须能够验证其模板语义来自正式模板组件
- **并且** 不得只通过最终字符串“看起来正确”就放过仍依赖隐式推断的实现

#### 场景:input / choice popup 共享模板合同可回归验证
- **当** 自动化测试覆盖 `text-input modal` 或 `choice-list modal`
- **那么** 测试必须验证这些弹窗共享统一的标题/正文结构、cursor 或 selected-row 行为以及提交/取消语义
- **并且** 必须能够在新增同类输入或选择弹窗时复用同一类测试口径

## 移除需求
