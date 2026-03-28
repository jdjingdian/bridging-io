## 上下文

当前 BridgingIO MCP runtime 对 `target` 参数的解析依赖一张启动时构建的 `target_refs` 精确映射表：target id 与 aliases 会被注册到 map 中，但解析时只做 `trim + lowercase` 后的直接查找。这意味着：

- `test_device` 无法命中 `test-device`
- `test-devics` 这类轻微手误只能直接报错
- `test` 在同时存在 `test-1`、`test-2`、`test-3` 时仍会直接执行精确项，无法表达“也许用户本来想选 family 内其他设备”

与此同时，当前 runtime 暴露给 MCP 的 `TargetProfile` 视图并不保留完整的 target 描述：配置层允许多个 aliases、`display_name` 和 `enabled`，但运行时执行视图只保留了第一个 alias，且没有把 `enabled` 作为解析约束稳定保留下来。这会让“用于执行的 canonical target”与“用于确认展示的 target 摘要”混杂在一起。

这项设计主要服务三类利益相关方：

- MCP/AI 客户端：需要稳定、可恢复的 target 引用体验，而不是“命中则执行、否则报错”的二元路径
- core/runtime 维护者：需要一套统一的 target resolver，而不是在每个 tool 里各自处理 target 参数
- 操作员：需要在同前缀设备较多时，通过确认结果看见足够上下文并避免误打到错误设备

## 目标 / 非目标

**目标：**

- 为所有接收 `target` 参数的 MCP typed tools 建立统一的 target 引用解析流程
- 将 target 解析结果显式建模为“已解析 / 需要确认 / 未找到”，而不是继续折叠为简单的 `Option<TargetProfile>`
- 支持低风险的宽松归一化命中，例如大小写与 `-` / `_` / 空格差异
- 支持 family-related 候选发现，并允许在存在精确项时仍基于策略返回确认
- 将“精准命中是否仍需确认”做成 core-owned 配置，默认策略为 `confirm_if_family`
- 在确认结果中返回足够的 target 摘要信息，但禁止为了确认而主动探测远端 target

**非目标：**

- 不在本变更中为 `artifact_id`、`session_id`、`shell_id`、`approval_id` 等机器标识引入模糊解析
- 不在本变更中为 repo id、tool name、capability id 统一引入同级别的歧义确认流程
- 不在本变更中定义 UI 层如何呈现确认结果；本设计只定义 MCP 返回语义
- 不在本变更中通过 `inspect_basic` 或其他探测命令为候选列表主动补充远端动态信息

## 决策

### 决策 1：引入统一的 `TargetResolutionResult`，替代当前的“查到 profile 就执行”

所有接收 `target` 参数的 MCP tools 都必须先经过统一 resolver，resolver 至少返回三类结果：

- `resolved`: 已得到唯一 canonical target，可继续执行
- `confirmation_required`: 存在可疑或歧义候选，禁止继续执行，必须把候选返回给调用方
- `not_found`: 没有可接受候选，按现有错误语义返回

选择这个方案，是因为“输入解析”和“执行动作”需要从控制流上解耦。只有先把 target 解析结果显式建模，才能稳定支撑 typo 容错、family 歧义与策略切换。

备选方案：

- 保持当前 `Option<TargetProfile>` 接口，在每个 tool 内单独补 fuzzy 逻辑
  - 放弃原因：会让 `terminal.exec`、`shell.open`、`inspect_basic` 的行为继续漂移
- 把所有模糊命中都直接转成 JSON-RPC error
  - 放弃原因：歧义候选不是失败，而是“需要用户做下一步选择”的正常分支

### 决策 2：把 target 解析拆成“低风险自动命中”与“高风险候选发现”两层

resolver 必须分两阶段工作：

1. 低风险自动命中：
   - 精确 id / alias / display name 命中
   - 宽松归一化后的唯一命中，例如大小写差异、`-` / `_` / 空格折叠
2. 高风险候选发现：
   - family-related 候选
   - 轻微 typo 候选
   - 其他相似但不能安全自动执行的候选

选择这个方案，是为了避免把“`test_device` 命中 `test-device`”和“`test-devics` 可能想表达 `test-device`”混成同一种风险等级。前者是低风险归一化，后者是需要确认的推断。

备选方案：

- 所有 fuzzy/normalized 命中都直接自动执行
  - 放弃原因：会让 typo 解析在设备较多时变成误操作放大器
- 所有非精确命中都必须确认
  - 放弃原因：会损失 `-/_/空格` 这种高频、低风险输入差异的可用性

### 决策 3：将 `confirm_if_family` 作为默认策略，并把“精确项也要确认”显式做成配置

系统必须把“存在精确命中时是否仍需确认”做成显式策略，而不是硬编码为“精确命中永远执行”。首轮定义以下枚举值：

- `auto_execute`: 有精确命中时直接执行
- `confirm_if_family`: 若存在精确命中且还存在同 family 候选，则返回确认
- `confirm_if_related`: 若存在精确命中且还存在 family 或 typo 候选，则返回确认
- `confirm_always`: 只要不是唯一明显候选，就返回确认

默认值选择 `confirm_if_family`，因为这是最符合当前用户目标的折中：`test` 与 `test-1/2/3` 这类高风险命名家族会触发确认，但 `lab` 与一个不相关的远距离 typo 候选不会无谓打断执行。

备选方案：

- 默认 `auto_execute`
  - 放弃原因：无法覆盖“`test` 其实可能是 `test-2`”的真实风险
- 只提供布尔配置（例如 `confirm_on_exact_conflict = true/false`）
  - 放弃原因：很快就无法表达 family-only 与 family+fuzzy 的不同风险偏好

### 决策 4：`family-related` 必须使用边界化前缀规则，而不是任意字符串前缀

首轮 family 关系必须采用“带边界的前缀族”定义。若用户输入为 `test`，则以下候选视为 related：

- `test-1`
- `test_1`
- `test 1`

以下不视为同 family：

- `testlab`
- `testcase`
- `mytest`

选择这个方案，是为了让“同系列设备”与“只是刚好有同样字母前缀”的字符串区分开来，避免 family 确认泛滥。

备选方案：

- 任意 `starts_with("test")` 都视为 family
  - 放弃原因：误报过多，确认列表会快速失去可信度
- 完全依赖编辑距离，不单独定义 family
  - 放弃原因：`test` 与 `test-1` 的关系是命名结构关系，而不只是字符串相似度

### 决策 5：确认结果必须是“无副作用的正常 tool 返回”，且候选摘要只使用静态或已缓存信息

当 resolver 返回 `confirmation_required` 时，MCP tool 必须返回结构化确认结果，且：

- `isError` 必须为 `false`
- 不得创建新的执行 artifact、transport、channel 或 timeline 成功记录
- 候选摘要只能来自配置、运行时已缓存 session/diagnostic/fingerprint 信息
- 禁止为了补充候选信息而主动在 target 上执行 `uname`、`whoami` 等探测命令

选择这个方案，是因为确认是“下一步选择提示”，不是错误；同时确认阶段一旦引入远端执行，就会把一个本应无副作用的解析动作升级成真实操作。

备选方案：

- 使用 JSON-RPC error 承载歧义候选
  - 放弃原因：会把可恢复分支伪装成失败，增加客户端分支复杂度
- 为了展示更多信息，对每个候选都临时执行 `inspect_basic`
  - 放弃原因：副作用、性能和审计成本都不可接受

### 决策 6：为 MCP 单独建立 richer target descriptor/index，而不是继续复用瘦身后的执行 profile 视图

resolver 与确认结果需要消费一份 richer target descriptor，至少保留：

- canonical target id
- enabled
- display name
- 全部 aliases
- kind
- notes
- 连接摘要
- 已缓存的 session / fingerprint / toolchain 诊断摘要

选择这个方案，是因为当前执行 profile 视图只保留了第一条 alias，且没有稳定暴露 `enabled` 等确认关键字段。继续复用它会让解析规则和展示语义长期受限。

备选方案：

- 继续仅使用 `TargetProfile`
  - 放弃原因：无法完整支撑多 alias、disabled filtering 和 richer candidate summary

## 风险 / 权衡

- [确认分支会改变一部分客户端的 happy path] → 通过把 `confirmation_required` 设计成结构化成功返回，并保留 canonical target id 与 policy reason，降低客户端接入成本
- [默认 `confirm_if_family` 可能增加一次交互] → 通过配置允许操作员切回 `auto_execute`，同时把默认确认范围限制在边界化 family 关系内
- [richer target descriptor 会增加运行时索引复杂度] → 通过把它限制为 MCP resolver/summary 的内部视图，不扩大到所有 domain 模型
- [宽松归一化可能引入新的冲突集合] → 通过把归一化冲突降级为确认候选，而不是在配置装载阶段直接报全局冲突

## 迁移计划

1. 在 specs 中定义 target resolution、family-related 规则、确认返回语义与策略配置。
2. 在 core-owned settings 中加入 MCP target resolution policy，并将默认值设为 `confirm_if_family`。
3. 在 runtime 中构建 richer target descriptor/index，保留全部 aliases、display name 与 enabled。
4. 将 `bridgingio.terminal.exec`、`bridgingio.terminal.shell.*`、`bridgingio.target.inspect_basic` 统一接入 resolver。
5. 为 resolved / confirmation_required / not_found 三类分支补齐集成测试与回归测试。
6. 若发布后发现确认策略过于保守，可先通过配置切回 `auto_execute` 作为临时缓解，而不必回滚整个解析模型。

## 开放问题

- 当前设计不再保留必须阻塞本提案的开放问题；repo id、tool name 等其他标识的模糊确认是否沿用同一 resolver 模式，留待后续独立变更决定。
