## 上下文

当前工具来源链路存在三个同时发生的问题：

- `TargetProfileSheetView` 提供了 `overridePath` 输入框，但 `UpsertProfile` 没有传输该字段，target 保存时自然不会落盘。
- 唯一真实可写的 override 入口是 `UpdateSettings(tool_override_command, tool_override_path)`，这条链路只会写入全局 `toolchains.<name>.path_override`。
- UI 没有把 target 级配置和系统级配置分层，导致目标编辑页、工作台主界面与系统设置页之间的边界都不清晰。

用户已经明确了期望语义：

```text
target override
-> global toolchain default
-> system PATH
-> built-in fallback
```

同时，配置写法也已经确定：

```toml
[toolchains.adb]
path_override = "/opt/homebrew/bin/adb"

[[targets]]
id = "android-emulator"
display_name = "Android Emulator"
kind = "adb"

[targets.connection]
selector_kind = "serial"
selector_value = "emulator-5554"

[targets.toolchains.adb]
path_override = "/Applications/AndroidStudio.app/.../adb"
```

本次设计的目标不是“再补一个输入框”，而是把配置分层、运行时解析、诊断回显和 UI 责任一起理顺。

## 目标 / 非目标

**目标：**

- 保留现有全局 `toolchains.<name>.path_override`，同时让 target profile 支持 `targets.toolchains.<name>.path_override`。
- 让 target 和 global 两种写法都能通过 core-owned 接口完成读取、保存、落盘和真实生效。
- 让工具来源诊断明确区分 target override、global override、system PATH 和 built-in fallback。
- 让 macOS UI 的工具来源入口职责清晰：target 页改 target，settings 页改 global，工作台详情页看诊断。
- 补齐自动化测试，覆盖分层回退与 UI 可见性。

**非目标：**

- 本次不重新设计 built-in fallback 打包策略，也不改变系统 PATH 的搜索规则。
- 本次不要求已打开的 transport/channel 在运行中热切换到底层新二进制；优先保证新建诊断与后续新会话使用最新配置。
- 本次不为尚未定义工具依赖映射的 target 类型强行增加 override 入口。

## 决策

### 决策 1：使用对称 schema，同名 `path_override` 按作用域区分

全局层保持：

```toml
[toolchains.<name>]
path_override = "/path/to/tool"
```

target 层新增：

```toml
[targets.toolchains.<name>]
path_override = "/path/to/tool"
```

不采用 `target_path_override` 这类字段名，因为 section 路径本身已经表达了作用域；保持同名字段可以减少序列化、解析、测试和文档的认知负担。

推荐数据模型方向：

- `CoreSettings.toolchains: HashMap<String, ToolchainSection>` 继续承载全局默认值
- `StandaloneTargetProfile.toolchains: HashMap<String, ToolchainSection>` 新增承载 target 局部值
- `TargetProfile` 新增等价字段，用于 `GetProfile` / `UpsertProfile` round-trip

### 决策 2：工具解析顺序固定为 target -> global -> PATH -> built-in

运行时必须使用以下顺序解析：

```text
targets.toolchains.<name>.path_override
-> toolchains.<name>.path_override
-> system PATH
-> built-in fallback
```

行为要求：

- target 级 override 只影响当前 target，不得隐式修改全局 `toolchains.<name>`。
- 当 target 清空该字段时，系统必须回退到全局默认值；若全局也为空，再继续回退到 PATH 和 built-in。
- 后续新建 session、打开 interactive shell、执行 one-shot exec，以及 `inspect_basic` 一类结构化探测时，必须使用最新解析结果。
- 已经建立的 transport/channel 可以继续使用旧路径直到重建，但 diagnostics 和后续新会话不得继续回显旧值。
- 对 ADB 目标，当配置里同时存在明确 serial 和 transport 提示时，命令拼接必须优先 serial 选择器，避免把可唯一定位的目标退化成 `-d` / `-e` 这类模糊选择。

### 决策 3：app API 与 diagnostics 必须同时回显两层配置和最终命中结果

为避免 UI 再次依赖猜测，相关契约必须显式返回：

- profile payload 中的 target 级 `toolchains`
- settings payload 中的全局 `toolchains`
- diagnostics 中的：
  - `target_override_path`
  - `global_override_path`
  - `effective_path`
  - `effective_source`
  - `effective_scope`

其中 `effective_scope` 建议至少覆盖：

- `target_override`
- `global_override`
- `system_path`
- `builtin_fallback`

这能让 UI 清楚表达：

- 当前 target 是否配置了自己的覆盖路径
- 若没有 target override，是否正在继承全局默认值
- 最终实际命中的是哪一层

### 决策 4：target 编辑页只展示当前 target 相关工具

目标编辑页中的工具来源区块必须只展示与当前 target 类型直接相关的工具：

- SSH target: 只显示 `ssh`
- ADB target: 只显示 `adb`

对于当前尚未定义稳定工具依赖映射的其他 target 类型，本次不强行渲染 `ssh`/`adb` 两行占位项。若某类型没有可配置工具来源，本次允许隐藏整个工具来源编辑区块，或仅展示只读诊断说明。

这样做的原因：

- 避免用户把“页面里出现的每一项”误解为当前 target 都会用到。
- 减少 `ssh target` 页面里出现 `adb` 输入框的认知噪声。
- 让每个 target 页面中的 override 都能直指本 target 的执行链路。

### 决策 5：全局 override 只在系统设置页编辑，工作台详情页回归诊断展示

全局 toolchain 默认值属于系统级设置，应与 model-plane host/port 一起放在 `SettingsSheetView` 中，而不是混在 target 页面或工作台详情页。

推荐 UI 边界：

- `TargetProfileSheetView`
  - 编辑当前 target 的局部 toolchain override
- `SettingsSheetView`
  - 编辑全局 `toolchains.<name>.path_override`
  - 与 model-plane host/port 同级呈现
- `WorkspaceConsoleView`
  - 展示选中 target 的有效工具来源与诊断摘要
  - 不再承载全局 override 的直接写入口

这样可以把“当前 target 的特例”和“整个系统的默认值”从视觉和交互上彻底分开。

### 决策 6：toolchain override 写入以 `live_applied` 为目标，但仍允许明确返回重启语义

本次实现目标应是：

- target override 写入后，diagnostics 与后续新会话能够直接使用最新值
- global override 写入后，diagnostics 与后续新会话能够直接使用最新值

也就是说，理想回包是 `live_applied`。

但若某条实现路径当前无法在进程内完成刷新，core 仍必须显式返回 `restart_required`，UI 按既有 host state machine 执行受控重启，而不是静默半生效。

### 决策 7：测试必须覆盖两层写法和回退链路

至少需要覆盖以下自动化矩阵：

```text
1. 仅配置 global override -> 命中 global
2. 同时配置 target + global -> 命中 target
3. 清空 target override -> 回退 global
4. global 为空 -> 回退 system PATH
5. PATH 缺失 -> 回退 built-in fallback
6. GetProfile / UpsertProfile round-trip target toolchains
7. GetSettings / UpdateSettings round-trip global toolchains
8. SSH target 页只出现 ssh；ADB target 页只出现 adb
9. Settings 页提供全局 override 编辑入口
10. one-shot exec / inspect_basic 使用 target 当前解析出的工具路径
11. interactive shell 使用与 diagnostics 一致的工具路径
12. ADB serial 与 transport 并存时优先 serial selector
```

## 数据流草图

```text
编辑 target 局部 override
────────────────────────
UI target sheet
  -> GetProfile(target_id)
  -> edit target.toolchains.<name>.path_override
  -> UpsertProfile
  -> core validate + persist
  -> refresh diagnostics

编辑系统全局默认值
──────────────────
UI settings sheet
  -> GetSettings
  -> edit toolchains.<name>.path_override
  -> UpdateSettings
  -> core validate + persist
  -> refresh diagnostics

运行时解析
──────────
resolve(target, tool)
  -> target.toolchains[tool].path_override?
  -> settings.toolchains[tool].path_override?
  -> PATH lookup
  -> built-in fallback
```
