## 上下文

当前仓库已经有 testing contract、self-test 文档和 app-api README，但它们主要是说明性文本，不足以形成正式的 matrix 真相。问题主要体现在：

- core contract 文档缺少架构维度
- `--self-test` 还没有明确 debug/release 支持矩阵
- 暴露给本地 UI/TUI/CLI 的接口缺少统一矩阵
- 当前支持平台与长期规划平台容易在文档中被混写

如果不把这些 matrix 文档前置下来，后续 core-first 变更很容易只改实现、不更新验收真相。

## 目标 / 非目标

**目标：**

- 为 self-test / contract automation 建立正式 case matrix
- 为本地 operator surfaces 建立正式 interface matrix
- 明确平台、架构、宿主模式和 build profile 的支持关系
- 把 matrix 更新纳入功能交付验收要求

**非目标：**

- 本次不引入新的远端 CI 编排系统
- 本次不设计全部未来平台 UI 的实现细节
- 本次不替代 OpenSpec specs 本身

## 决策

### 决策 1：将测试矩阵与接口矩阵拆成两份正式文档

建议新增两类真相文档：

- `SELF_TEST_CASE_MATRIX`
  - 关注 case id、平台、架构、host mode、build profile、命令和预期结果
- `LOCAL_OPERATOR_INTERFACE_MATRIX`
  - 关注 command/interface、调用方、输入、输出、状态、错误码、apply strategy

这样能把“如何验收”与“暴露了什么接口”分开管理。

### 决策 2：matrix 必须区分“当前支持”与“长期规划”

文档必须显式区分：

- 当前支持：macOS、Linux x86_64、Linux aarch64、Windows
- 长期规划：OpenHarmony

这可以避免把长期占位误写成“已完成支持”。

### 决策 3：`--self-test` 作为 debug-only 运行时测试框架管理

`--self-test` 必须在 matrix 中明确标注为：

- debug 模式支持
- release 模式拒绝

这样可以把它定位为正式的运行时测试框架，而不是生产发行物的默认操作路径。

### 决策 4：非 Linux 宿主必须补齐同架构 Linux contract run

对于非 Linux 宿主的编译/验证流程，matrix 必须要求补齐同架构 Linux contract run，例如：

- Apple Silicon macOS → Linux aarch64
- x86_64 macOS/Windows → Linux x86_64

这样可以把“构建通过”和“核心 Linux contract 也通过”同时纳入验收。

## 风险 / 权衡

- [风险] 维护两份 matrix 文档会增加文档负担
  → 缓解措施：把它们纳入任务和验收清单，而不是作为可选文档
- [风险] debug-only self-test 可能与现有文档冲突
  → 缓解措施：在 matrix 中明确迁移规则和 release 拒绝语义
- [风险] cross 验证要求会提高本地验证成本
  → 缓解措施：先明确最小必跑矩阵，再逐步自动化

## 迁移计划

1. 建立测试矩阵与接口矩阵文档骨架。
2. 更新现有 testing docs 与 app-api boundary 文档引用。
3. 补充 build profile、平台、架构与 host mode 维度。
4. 再将 matrix 更新要求纳入后续变更模板和验收流程。

## 未决问题

- self-test release 拒绝是否还需要一个更轻量的生产健康检查替代入口
- interface matrix 是否直接放在 `docs/` 下，还是同步镜像到 README/skill 文档
