## 上下文

当前仓库没有正式的 core 级交互式配置操作面。short-term UI 又明确暂停推进，这意味着如果不尽快补上一个 core-first 的 operator console，`bridgingio-core` 将继续依赖 TOML 手工编辑和零散命令，后续也很难区分“core 问题”和“前端问题”的边界。

`ratatui` 是合适的方向，因为它能提供跨平台、终端内的正式交互层，同时允许我们把交互风格尽量对齐 Linux kernel `menuconfig`，让 `bridgingio-core menuconfig` 成为真正的第一类通用配置入口。standalone 只是当前最直接的消费方，而不是该入口的唯一归属。

## 目标 / 非目标

**目标：**

- 为 `bridgingio-core` 提供正式的 `menuconfig` 通用配置操作面
- 尽量对齐 Linux kernel `menuconfig` 的树导航、搜索、帮助与保存体验
- 直接复用 core-owned settings/profile schema、字段说明和 apply 语义
- 将 vault/token/runtime root 等安全与生命周期状态以 display-safe 方式投影到 TUI

**非目标：**

- 本次不重建现有 GUI 设计系统
- 本次不让 TUI 直接管理 secret 明文导入与 reveal
- 本次不要求 TUI 在首轮就覆盖所有 future UI 细节

## 决策

### 决策 1：menuconfig 是 `bridgingio-core` 的正式通用配置入口，而不是 standalone 专属工具

该 TUI 必须被设计为 `bridgingio-core menuconfig` 的正式入口，而不是“临时配置器”或 standalone 专属前置工具。它承担的职责包括：

- 配置浏览与编辑
- 搜索和帮助
- 生命周期与诊断查看
- 保存与应用动作

### 决策 2：TUI 直接消费 core-owned schema 与配置写回语义

TUI 不维护私有配置 schema。它必须直接依赖：

- core 字段描述
- 配置模型与校验语义
- 保存后的 apply 语义或等价重启提示

这能保证：

- 与 future UI 共用同一套配置真相
- 手工编辑配置与 TUI 编辑的结果一致
- 错误边界停留在 core，而不是在 TUI 层复制一套逻辑

### 决策 3：交互模型尽量对齐 Linux kernel `menuconfig`

首轮交互原则：

- 左侧树 / 层级导航
- 当前项帮助/说明
- `/` 搜索
- dirty tracking
- 保存/应用/放弃修改

但并不机械复制 kernel 的全部细节。只保留那些有助于配置浏览和状态切换的核心交互。

补充约束（已落地）：

- 主操作区使用固定底部按钮栏 `<Select> < Exit > < Help >`
- 左右键切换按钮焦点，Enter 执行焦点按钮
- `Esc` 作为统一返回键：子菜单返回上级；主菜单触发退出流程
- 主菜单存在 dirty 时，`Esc` 退出必须先弹出保存确认（`Yes/No/Cancel`）

### 决策 4：安全相关页面只展示 display-safe 投影

TUI 必须能看到：

- vault lock state
- protector readiness
- token summary
- runtime root / config lifecycle 状态

但不得直接成为 secret 明文或 token 明文展示通道。高风险操作保留在受控管理路径中。

### 决策 5：现有 `vault/auth` CLI 子命令本轮保留兼容，不立即删除

虽然长期方向可以把更多管理动作收敛到 `menuconfig`，但本轮不立即删除：

- `vault ...`
- `auth ...`

原因：

- 当前安全启动与管理流刚刚完成收口
- 立即移除 CLI 会增加回归风险
- 可以先让 `menuconfig` 成为主入口，再在后续单独变更中评估删减

### 决策 6：字段显示语义与标记语义统一，优先可读性和可预测性

为了让操作员一眼判断“能不能改、能不能进、当前状态是什么”，行级语义固定如下：

- 弹窗可编辑项使用 `Label (value) --->`
- 布尔开关使用 `[ ]` / `[*]`
- 子菜单状态入口使用 `< >` / `<*>`，并保留 `--->`
- 只读项使用 `---`（或等价只读标记）
- 强调说明/动作标题使用 `*** ... ****`

这套语义在 core/general 页与 targets 页保持一致，不因页面不同而改变。

### 决策 7：布局与视觉重心参考 OpenWrt `menuconfig`

为了降低学习成本，列表布局采用：

- 选项整体居中
- 文本列左对齐
- 开关/状态标记列在左侧突出
- 底部按钮置于 Main Menu 框底部，并使用反色高亮表示当前焦点

## 风险 / 权衡

- [风险] TUI 首轮范围过大，可能吞掉 core 迭代节奏
  → 缓解措施：先聚焦配置、搜索、帮助、保存/应用和 display-safe 安全状态
- [风险] 如果 TUI 维护私有 schema，会再次制造边界漂移
  → 缓解措施：强制 TUI 只消费 core-owned 字段描述与 apply 语义
- [风险] menuconfig 风格不一定适合所有新用户
  → 缓解措施：把交互风格对齐为“可预期且高密度”，同时保留帮助和搜索路径

## 迁移计划

1. 定义 TUI 能消费的 schema/字段描述接口。
2. 建立 `bridgingio-core menuconfig` 命令与基础导航/搜索。
3. 接入配置保存/应用与 lifecycle 状态查看。
4. 再逐步扩展安全状态页面，并在后续决定是否吸纳更多管理动作。

## 未决问题

- TUI 是否独立成新 crate，还是先放入现有 standalone/core 二进制内
- 搜索结果页与树形导航之间的最小信息架构应该如何组织
