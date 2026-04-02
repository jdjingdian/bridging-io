## 上下文

BridgingIO 目前已经有多层错误与状态表达：

- `bridgingio-app-api` 暴露的是少量高层 `ApiErrorCode`
- `bridgingio-secrets`、`bridgingio-engine`、`bridgingio-platform` 各自维护模块错误
- `bridgingio-mcp` 和 CLI 仍存在大量字符串化诊断与临时映射

这在过去 UI-first 阶段还能勉强工作，但在当前的 core-first 和 standalone-first 方向下，会直接导致几个问题：

- menuconfig、CLI、MCP、future UI 无法共享同一套错误边界
- `degraded`、`unsupported`、`locked`、`not ready`、`not implemented` 等状态被混用
- 模块专属错误细节要么丢失，要么直接泄露内部实现语义
- 问题定位容易退化成“前端显示不对”而不是“core 合同没收敛”

因此本变更的重点不是简单新增一个 enum，而是建立一个可以横跨 runtime、control-plane、MCP 和安全管理面的统一错误与状态合同。

## 目标 / 非目标

**目标：**

- 定义共享的通用状态集合，并把它作为公共投影真相源
- 定义统一错误封装，允许同时携带通用分类和模块专属子码
- 为 `method_not_implemented` 建立长期稳定的正式语义
- 收敛 app-api、MCP、standalone CLI 和 runtime 生命周期的错误回传模型
- 确保用户可见错误不会泄露 secret、路径、locator 或内部实现细节

**非目标：**

- 本次不要求一次性改造所有内部私有错误类型
- 本次不把所有错误都统一成单一 enum 并移除模块边界
- 本次不设计国际化文案体系
- 本次不替代日志与审计系统

## 决策

### 决策 1：采用“共享状态 + 共享封装 + 模块子码”的三层模型

公共调用面统一暴露三层信息：

1. `status`
   - 描述当前能力或对象处于什么状态
2. `common_code`
   - 描述跨模块可复用的错误类型
3. `module_code`
   - 描述模块专属语义

这样可以同时满足：

- 前端和自动化先用通用状态/错误分类做稳定分支
- 诊断和测试仍能识别 vault、config、toolchain 等专属原因
- 不必强迫所有模块共享一个超大枚举

### 决策 2：`method_not_implemented` 是正式通用语义，而不是临时字符串

对当前明确无法立即实现、但希望保留稳定调用面的能力，系统必须返回正式的：

- `status = not_implemented` 或等价状态投影
- `common_code = method_not_implemented`
- 允许的 `module_code`

它必须与以下状态严格区分：

- `unsupported`: 当前平台或当前对象不支持
- `degraded`: 当前能力可运行但降级
- `not_ready`: 当前能力尚未就绪但未来可就绪

### 决策 3：共享错误封装必须包含恢复信息，但 details 默认最小化

公共错误封装至少应支持：

- `domain`
- `common_code`
- `module_code`
- `message`
- `retriable`
- `recovery_hint`
- `details`

其中：

- `message` 面向普通调用方，必须可安全显示
- `details` 只承载 display-safe 的结构化附加信息
- secret、locator、明文 token、明文路径摘要等高敏字段不得进入公共 details

### 决策 4：不同调用面共享 canonical 标识，但允许投影深度不同

MCP、app-api、standalone CLI 和 runtime 诊断必须共享同一套 canonical `status/common_code/module_code` 标识，但可以投影成不同深度：

- MCP / app-api：完整结构化对象
- CLI：结构化对象 + 人类可读摘要
- 日志：结构化字段 + 运行时上下文

这样既能统一自动化断言，也不会强迫每个调用面拥有相同的展示层。

### 决策 5：迁移按“外围先统一、内部渐进收口”推进

迁移顺序采用：

1. 先定义共享合同模块与映射规则
2. 先改公共调用面投影
3. 再逐步把各模块内部错误类型补上更清晰的映射

这样可以先稳定外部边界，再在实现阶段逐步收口内部结构，避免一次性重写所有错误类型。

## 风险 / 权衡

- [风险] 新旧错误格式并存一段时间会增加迁移复杂度
  → 缓解措施：先冻结公共合同，再通过映射层兜底旧格式
- [风险] 通用错误分类过粗会再次丢失模块语义
  → 缓解措施：保留 `module_code`，禁止只返回 `message`
- [风险] 公共 details 设计过深可能重新泄露内部信息
  → 缓解措施：要求公共 details 仅允许 display-safe 字段，并把高敏数据继续留在日志/审计私有面

## 迁移计划

1. 定义共享错误与状态合同模块及 canonical 标识表。
2. 更新 app-api、MCP、standalone CLI 与 runtime 生命周期投影。
3. 为 vault/config/toolchain/lifecycle 等主要模块补齐映射与测试。
4. 清理历史 ad hoc 文本路径，把它们降为辅助日志而非协议真相。

## 未决问题

- `status = not_implemented` 是否与 `common_code = method_not_implemented` 同时保留，还是仅保留一个主语义字段
- CLI 默认输出是否同时提供 JSON 与人类可读格式
