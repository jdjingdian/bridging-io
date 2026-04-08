## 为什么

当前 `menuconfig` 的 dirty tracking 更接近“本次会话里编辑过哪些字段”，而不是“当前内存草稿与已加载/已保存配置是否真的不同”。这会导致操作员把某个字段临时改成别的值、又改回原值后，根页面仍然显示 dirty，`Esc` 退出时也继续提示保存，容易让人误以为配置仍有未保存变更。

现在需要把 `menuconfig` 的保存提示语义收敛为“仅在当前草稿相对基线配置存在真实差异时才提示保存”。这样可以减少误导性的退出确认，让 dirty tracking、字段级修改提示与 target 编辑流的基线比较语义保持一致，也更符合用户对 Linux kernel `menuconfig` 类交互的直觉。

## 变更内容

- 将 `menuconfig` 的全局 dirty 判定从“是否记录过 dirty path”收敛为“当前配置是否与本次会话基线配置存在真实差异”。
- 要求普通配置字段在值被改回基线后自动清除对应 dirty 状态，而不是继续把该字段视为未保存修改。
- 要求根页面 `Esc` / `Exit` 的保存确认弹窗仅在当前配置相对基线仍有真实未保存差异时出现；若所有变更都已回退到基线，则直接退出。
- 要求 target 编辑流与全局配置 dirty tracking 继续共享一致的“基线比较”语义，避免出现 target 子流程与根配置退出语义不一致。
- 本次变更不调整配置文件 schema，不引入新的保存/应用策略，也不改变现有 `Yes / No / Cancel` 退出确认流程本身，只修正其触发条件与 dirty 状态真相。

## 功能 (Capabilities)

### 新增功能

- 无

### 修改功能

- `standalone-operator-console`: 调整 `menuconfig` 的 dirty tracking 与退出保存提示语义，要求其仅反映当前草稿相对基线配置的真实差异，而不是单纯记录编辑历史。
- `menuconfig-style-matrix`: 更新风格矩阵中与 dirty tracking、标题 dirty 标记和退出确认相关的合同，明确“恢复到基线后不再显示 dirty”的正式语义。
- `operator-interface-matrix`: 更新本地 operator interface matrix 对 `bridgingio-core menuconfig` 保存/退出行为的描述，明确保存确认只在真实配置差异仍存在时触发。

## 影响

- 受影响代码主要包括 `source/rust/bridgingio-operator-console` 中 `MenuConfigApp` 的 dirty 状态模型、普通字段编辑提交流程、target 编辑会话与根页面退出判定逻辑。
- 受影响回归测试主要包括字段编辑、choice popup、布尔切换、target 编辑回退、根页面退出确认与保存后基线刷新等场景。
- 受影响规范与文档包括 `openspec/specs/standalone-operator-console/spec.md`、`openspec/specs/menuconfig-style-matrix/spec.md`、`openspec/specs/operator-interface-matrix/spec.md` 以及后续对应的 matrix 真相文档。
