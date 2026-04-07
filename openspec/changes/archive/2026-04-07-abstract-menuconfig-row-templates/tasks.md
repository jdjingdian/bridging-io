## 1. 模板模型设计

- [x] 1.1 盘点当前 menuconfig 中已经出现的行模式，明确哪些地方存在“规范存在但实现容易漂移”的问题
- [x] 1.2 定义 row template vocabulary，至少覆盖纯开关、互斥单选、带详情的互斥单选、多选、字段入口、动作入口、阻断态与强制只读态
- [x] 1.3 定义 group template vocabulary，至少覆盖互斥单选组与 picker 型单选组
- [x] 1.4 盘点 popup/overlay 结构，定义 popup template 与控件原语边界
- [x] 1.5 记录当前设计缺漏与后续 review checklist

## 2. 规格更新

- [x] 2.1 为 `menuconfig-style-matrix` 增加 row / popup template catalog 与模板映射要求
- [x] 2.2 为 `standalone-operator-console` 增加“menuconfig 必须通过共享模板层构建行与弹框语义”的要求
- [x] 2.3 为 `quality-and-test-automation` 增加模板合同级回归覆盖要求
- [x] 2.4 按新的 row / popup template 设计更新 `docs/matrix/MENUCONFIG_STYLE_MATRIX.md`，补齐 template catalog、primitive mapping 与示例映射

## 3. 后续实现准备

- [x] 3.1 规划 `bridgingio-operator-console` 的模板语义层迁移路径，避免 screen 继续手拼前缀和后缀
- [x] 3.2 规划 popup primitive 与 overlay state 的迁移路径，避免新增弹框继续扩展分散状态字段
- [x] 3.3 选定第一批迁移对象：`Allow Non Loopback`、Token Access Switch、SSH Authentication、Credential Picker、阻断态 continue 行、required 只读态、confirm modal、text-input modal、waiting/result modal
- [x] 3.4 为后续实现准备模板级渲染测试与交互测试清单
