## 为什么

当前 MCP 工具对 `target` 参数只支持精确匹配：用户一旦把 target 名称或 alias 输错，或面对同一前缀下的多个设备时，系统只能直接报错或直接命中单个目标，既不利于容错，也缺少“有风险但可恢复”的确认语义。随着 target 数量增加、命名趋同以及 AI/MCP 客户端更多依赖自然输入引用目标，这个问题已经开始影响可用性与安全性。

## 变更内容

- 为 MCP 中接收 `target` 参数的 typed tools 引入统一的 target 引用解析流程，支持精确匹配、宽松归一化匹配和受控的相关候选发现。
- 在 target 名、alias 与 display name 之间建立统一解析语义，支持大小写、`-` / `_` / 空格差异等低风险归一化。
- 为“可能是手误”或“存在同 family 设备”的场景引入结构化确认返回，而不是直接执行或直接报错。
- 引入 target 解析策略配置，允许系统在“精确命中即可执行”和“精确命中但存在相关候选时仍需确认”之间切换；默认策略采用 `confirm_if_family`。
- 为确认候选返回可帮助决策的上下文摘要，包括 canonical target id、显示名称、kind、aliases、notes、连接摘要、诊断/会话摘要等静态或已缓存信息。
- 明确本次变更仅覆盖 `target` 引用解析与确认语义；其他标识（如 repo id、artifact id、session id）的模糊支持不纳入本次实现承诺。

## 功能 (Capabilities)

### 新增功能

无

### 修改功能

- `capability-aware-mcp`: MCP tools 在处理 `target` 参数时需要支持统一的宽松解析、歧义检测与结构化确认返回语义。
- `target-session-management`: target profile 与运行时 target 索引需要支持 canonical target id、多个 alias、display name 与 family-related 候选的统一解析规则和策略配置。

## 影响

- Rust core / MCP runtime 的 target 引用解析路径与 tool response 结构
- `bridgingio.terminal.exec`、`bridgingio.terminal.shell.*`、`bridgingio.target.inspect_basic` 等接收 `target` 的 MCP tools
- standalone/core-owned settings 中与 target resolution policy 相关的配置模型
- target/profile 索引与可供确认候选使用的 target 摘要视图
- MCP 相关集成测试、target resolution 回归测试与歧义确认场景测试
