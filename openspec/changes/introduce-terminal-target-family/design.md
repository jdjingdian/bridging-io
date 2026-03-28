## 上下文

BridgingIO 当前已经在行为上把 `ssh` 与 `adb` 当作终端型目标处理：两者共享 target/session/channel 审计模型、共享 one-shot 与 interactive shell 操作方式、共享结构化 invocation 与 toolchain 诊断链路。但在实现结构上，`ssh` 与 `adb` 仍主要表现为两个彼此平行的 connector，MCP、connector、provider 与 runtime 之间存在多处分发和重复判断。

与此同时，宿主平台侧已经建立了 `HostPlatformAdapter` 与 `local_shell_runtime` 边界，用于承载宿主 shell、路径、IPC、解码与日志等能力。后续如果要支持 `localshell` target 与 `serial` target，就必须避免把“宿主执行能力”与“目标传输能力”混成同一层。

这项设计面向三个直接利益相关方：

- core/runtime 维护者：需要一个稳定的扩展点，而不是继续堆叠 `match TargetKind`
- future connector 实现者：需要明确知道 `localshell` 与 `serial` 应该落在哪层
- future UI/control-plane 团队：需要稳定的 session/channel/concurrency 语义，而不是实现阶段临时决定

## 目标 / 非目标

**目标：**

- 显式引入 `terminal target family` 这一层抽象，统一终端型目标的能力契约
- 保留具体 `TargetKind`，不把 `ssh`、`adb`、`localshell`、`serial` 折叠成一个无差别的大类
- 把以下三个维度分层清楚：
  - 宿主运行时能力
  - 目标传输/连接器能力
  - 目标 shell 方言能力
- 为终端型目标定义并发策略，使 `multiplexed` 与 `exclusive` 的差异成为显式模型，而不是未来散落在实现特例中
- 为 future `localshell` 与 `serial` 接入建立统一契约，而不要求在本变更中完成完整实现

**非目标：**

- 不在本变更中直接实现 `localshell` 或 `serial`
- 不替换现有 `HostPlatformAdapter` 作为宿主平台边界
- 不把所有 target 都归类为 terminal target；非终端型目标仍可采用其他能力模型
- 不在本轮承诺 future dialect（如 `ssh-powershell`）具备完整命令等价行为

## 决策

### 决策 1：保留具体 `TargetKind`，新增 terminal family 作为共享抽象

系统继续保留 `ssh`、`adb`、future `localshell`、future `serial` 等具体 `TargetKind`，同时新增一个更高一层的 terminal family 抽象，用于承载终端型目标的共享能力与约束。

选择这个方案，是因为具体 kind 仍然承载了不可忽略的连接参数、工具链、探测命令与 transport 差异；但如果没有 family 层，新增 target 时又会持续复制现有 `ssh` / `adb` 的公共逻辑。

备选方案：

- 单一 `shell` 大类 + 子类继承
  - 放弃原因：`serial` 不一定是完整 shell；`localshell` 也容易和宿主 runtime 混层
- 维持现状，继续按 connector 单独扩展
  - 放弃原因：会让 future target 在 MCP、provider、runtime 中继续增加分叉点

### 决策 2：把并发差异建模为 terminal transport/concurrency policy，而不是 session 模型特例

终端型目标必须显式声明并发策略。首轮至少包含：

- `multiplexed`: 支持多个逻辑会话，且同一逻辑会话内允许多个并发 channel/window
- `exclusive`: 同一 target 在同一时刻只允许一个独占 transport 或一个活动交互 channel

选择这个方案，是因为 `serial` 的核心差异在于物理链路独占，而不是“不配拥有 session”。Session 模型本身仍然适用于独占 target，只是 transport/channel 的获取需要 target-wide lease。

备选方案：

- 规定 `serial` 永远只能有一个 session
  - 放弃原因：把物理独占错误地下沉成了领域模型的永久限制，不利于 future queue/reconnect/lease 设计
- 在 UI 或 MCP 层手工禁止第二个窗口
  - 放弃原因：约束不应只停留在前端，必须成为 core 规格与能力契约

### 决策 3：宿主 `local_shell_runtime` 与 future `localshell` target 必须保持分层

`HostPlatformAdapter.local_shell_runtime` 继续表示宿主机上的执行承载能力，用于本地启动进程、处理 I/O、解码与交互控制。future `localshell` target 则表示一种 terminal target transport，必须经过 target/session/channel/audit 统一链路进入系统。

选择这个方案，是为了避免“本地 shell”一词同时承担宿主能力和目标对象两种职责。宿主 runtime 是设施层；`localshell` target 是领域层。

备选方案：

- 将 `localshell` 直接等同于宿主 runtime
  - 放弃原因：会绕开 target profile、session、artifact 与 approval 模型，造成一条旁路

### 决策 4：终端型 connector 必须收敛到统一契约

terminal family 需要统一契约，至少覆盖：

- 构建 one-shot invocation
- 构建 interactive invocation
- 声明 target shell dialect
- 声明 concurrency policy
- 生成环境探测计划
- 对接 toolchain 解析与诊断

首轮并不要求立刻引入最终 trait 名称或注册机制，但设计和规格必须以这套统一契约为真相源，避免 future target 继续通过 MCP/connector/provider 多处分支拼装。

备选方案：

- 保持 connector 仅共享数据结构，不共享契约
  - 放弃原因：无法真正降低 future target 接入成本

### 决策 5：target shell dialect、host runtime 与 concurrency policy 是三个独立维度

系统必须避免把以下维度混用：

- host runtime：决定本地进程如何被启动和驱动
- target shell dialect：决定远端命令、prompt、cwd/env 语义
- concurrency policy：决定 target 是否允许多 transport / 多 channel 并发

例如：

- `windows host + adb-android-shell + multiplexed`
- `unix host + ssh-posix + multiplexed`
- future `unix host + serial-console + exclusive`

这三个维度分离后，future target 才能在不重写整条执行链的前提下接入。

## 风险 / 权衡

- [抽象层增加] → 需要额外的能力模型与命名约束；通过先固化规格、后渐进实现来降低一次性改造成本
- [过早泛化 `serial`] → 串口设备类型很多，行为不完全一致；通过首轮只定义“独占 transport / 活动 channel”最小语义来控制范围
- [`localshell` 容易重新混层] → 名称上与宿主 runtime 太接近；通过在规格中明确“target transport vs host runtime”来减少歧义
- [实现迁移期间会出现双轨逻辑] → 旧有 `ssh` / `adb` 代码与新抽象可能短期并存；通过要求结构化 invocation 与 shared contract 成为唯一真相源来收敛

## 迁移计划

1. 在规格层引入 terminal family、concurrency policy 与宿主/runtime 分层定义。
2. 在设计层明确 `ssh` / `adb` 先对齐到统一终端契约，保持现有行为不回退。
3. 在实现层分阶段收敛：
   - 先提炼共享接口与分发点
   - 再将 `ssh` / `adb` 适配到统一契约
   - 最后为 future `localshell` / `serial` 预留接入位
4. 若迁移中出现回退风险，允许回滚到旧实现细节，但不得回退规格中的三层分离原则与并发策略定义。

## 开放问题

- `exclusive terminal` 是否只允许单 writer，还是允许 future “单 writer + 多只读观察者”模式？
- future `localshell` target 的默认并发策略是否直接视为 `multiplexed`，还是允许按 profile 配置收紧？
- terminal family 的共享契约最终是落在 domain capability、connector trait、还是 runtime registry，需要在实现设计时再做一次命名收敛。
