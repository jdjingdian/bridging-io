## 新增需求

### 需求:target 运行时索引必须保留 canonical 标识与完整可解析引用
系统必须为 MCP target 解析维护一份运行时可消费的 target descriptor/index。该索引必须至少保留 canonical target id、enabled、display name、全部 aliases、kind、notes 与连接摘要，而不是仅保留可执行所需的瘦身 profile 视图。

#### 场景:target 配置包含多个 aliases
- **当** 某个 target 在配置中声明了多个 aliases
- **那么** 系统必须在运行时索引中保留全部 aliases，使它们都可以参与解析与确认候选展示，而不是只保留第一条 alias

#### 场景:disabled target 不参与可执行解析
- **当** 某个 target 在配置中被标记为 `enabled = false`
- **那么** 该 target 必须不参与 MCP 的可执行 target 解析与候选确认结果，而不是继续像正常 target 一样被自动命中

### 需求:target 引用解析必须区分低风险自动命中与高风险相关候选
系统必须将 target 引用解析拆分为低风险自动命中与高风险候选发现两个阶段。低风险阶段必须支持大小写与 `-` / `_` / 空格差异等宽松归一化；高风险阶段必须处理 family-related 与 typo-related 候选，并禁止默认自动执行这些高风险推断。

#### 场景:低风险归一化唯一命中
- **当** 用户输入 `test_device`，系统中存在唯一 target `test-device`
- **那么** 系统必须将该输入视为低风险归一化命中并解析到 canonical target `test-device`

#### 场景:family-related 候选使用边界化前缀规则
- **当** 用户输入 `test`，系统中存在 `test`、`test-1`、`test_2`、`test 3`、`testlab` 与 `testcase`
- **那么** 系统必须只把 `test-1`、`test_2` 与 `test 3` 视为 `test` 的 family-related 候选，而禁止把 `testlab` 或 `testcase` 误判为同 family 设备

### 需求:MCP target resolution policy 必须可配置并默认对 family 歧义二次确认
core-owned settings 必须允许操作员配置 MCP target resolution policy，以决定“存在精确命中时是否仍需确认”。系统必须至少支持 `auto_execute`、`confirm_if_family`、`confirm_if_related` 与 `confirm_always` 四种策略，并且默认值必须为 `confirm_if_family`。

#### 场景:默认策略在 family 歧义时返回确认
- **当** 操作员未显式覆盖 target resolution policy，且用户输入 `test`，系统中同时存在 `test`、`test-1` 与 `test-2`
- **那么** 系统必须按默认策略 `confirm_if_family` 返回确认结果，而不是直接执行精确命中的 `test`

#### 场景:策略设为 auto_execute 时精确命中直接执行
- **当** 操作员将 MCP target resolution policy 配置为 `auto_execute`，且用户输入 `test`，系统中同时存在 `test`、`test-1` 与 `test-2`
- **那么** 系统必须直接将 `test` 解析为 canonical target 并继续执行，而不是仍然返回确认结果

## 修改需求

## 移除需求
