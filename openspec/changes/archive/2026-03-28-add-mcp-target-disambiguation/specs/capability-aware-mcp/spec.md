## 新增需求

### 需求:接收 `target` 参数的 MCP tools 必须使用统一的 target 引用解析
所有接收 `target` 参数的 MCP typed tools 必须在执行前通过统一的 target resolver 解析用户输入，而不是继续在各个 tool 内分别做精确字符串查找。该 resolver 必须同时支持 canonical target id、alias 与 display name 的统一查询语义。

#### 场景:宽松归一化唯一命中 target
- **当** MCP 客户端调用接收 `target` 参数的 tool，传入 `test_device`，而系统中存在唯一可执行 target `test-device`
- **那么** 系统必须将其解析为 canonical target `test-device` 并继续执行，而不是因为 `-` 与 `_` 的差异直接报错

#### 场景:多个接收 `target` 参数的 tools 行为保持一致
- **当** MCP 客户端分别调用 `bridgingio.terminal.exec`、`bridgingio.terminal.shell.open` 与 `bridgingio.target.inspect_basic`，并传入同一个 target 引用
- **那么** 这些 tools 必须共享同一套 target 解析结果与确认语义，而不是出现某个 tool 可以解析、另一个 tool 直接报错的漂移行为

### 需求:存在潜在歧义但有可选候选时 MCP 必须返回结构化确认结果
当接收 `target` 参数的 MCP tool 无法安全自动选择唯一 target，但又存在可信候选时，系统必须返回结构化确认结果，而不是直接执行或直接把该情况视为错误。该确认结果必须是正常 tool 返回，并且必须包含请求输入、策略原因、精确命中项（如果存在）与候选列表。

#### 场景:target 输入存在 typo 候选
- **当** MCP 客户端传入 `test-devics`，系统未找到精确命中，但存在可信候选 `test-device`
- **那么** 系统必须返回 `confirmation_required` 类结果，提示调用方确认实际 target，而不是直接执行 `test-device` 或直接返回“target not found”

#### 场景:精确命中但存在同 family 候选且策略要求确认
- **当** MCP 客户端传入 `test`，系统中存在可执行 target `test`、`test-1`、`test-2` 与 `test-3`，且当前策略为 `confirm_if_family`
- **那么** 系统必须返回结构化确认结果，并同时把精确命中的 `test` 与其 family-related 候选一并返回，而不是直接执行 `test`

### 需求:确认结果必须无副作用且仅使用静态或已缓存摘要
当 MCP tool 返回 target 确认结果时，系统必须保持该次调用为无副作用的解析阶段：禁止为了确认而建立新的 transport、channel、artifact 或远端探测动作。确认候选中使用的上下文信息必须来自配置或已缓存的运行时摘要。

#### 场景:确认结果不触发远端命令执行
- **当** MCP 客户端调用 `bridgingio.target.inspect_basic`，但 target 解析进入 `confirmation_required`
- **那么** 系统必须只返回候选确认信息，而禁止在任何候选 target 上主动执行 `uname`、`whoami` 或其他探测命令

#### 场景:确认候选返回可帮助选择的摘要
- **当** MCP tool 返回 target 确认结果
- **那么** 每个候选项必须至少包含 canonical target id、显示名称、kind，以及可用时的 aliases、notes、连接摘要、诊断摘要或已缓存会话摘要，以帮助调用方做出选择

## 修改需求

## 移除需求
