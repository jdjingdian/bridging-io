## 新增需求

### 需求:目标 profile 必须支持 target 级 toolchain override
系统必须允许 target profile 在 `targets.toolchains.<name>.path_override` 中声明当前 target 的局部工具覆盖。该字段是可选的，并且在语义上只影响当前 target 的连接器或 provider 工具解析，不得隐式修改全局 `toolchains.<name>.path_override`。

#### 场景:target override 覆盖全局默认值
- **当** 用户为某个 ADB target 配置 `targets.toolchains.adb.path_override`，同时系统全局存在 `toolchains.adb.path_override`
- **那么** 当前 target 必须优先使用自己的 `adb` 路径，而其他未配置 target override 的 ADB target 继续使用全局默认值

#### 场景:清空 target override 后回退全局默认值
- **当** 用户清空某个 target 的 `targets.toolchains.adb.path_override`，且系统全局仍保留 `toolchains.adb.path_override`
- **那么** 系统必须移除该 target 的局部 override，并让当前 target 回退使用全局默认值，而不是继续保留陈旧的 target 级生效路径

## 修改需求

### 需求:连接器执行路径解析
系统必须为每个连接器或 provider 解析可执行工具来源，并且必须按“target 级用户覆盖路径、全局用户覆盖路径、系统 PATH、内置后备”这一顺序选择有效实现。该解析结果必须同时驱动诊断回显、one-shot exec、结构化基础信息探测与 interactive shell 启动，而不是只停留在诊断层。系统必须能够向 UI 和诊断接口返回当前 target 的 target override、global override 以及最终生效来源。

#### 场景:target 未配置 override 时命中全局默认值
- **当** 某个 target 未配置 `targets.toolchains.adb.path_override`，但系统全局配置了 `toolchains.adb.path_override` 且该路径可用
- **那么** 系统必须使用全局默认值建立该 target 的 ADB 能力，并在诊断信息中明确表明当前生效来源来自全局 override，而不是 target override

#### 场景:全局与 target 都未配置时继续回退
- **当** 某个 target 既没有 target 级 override，也没有全局 override
- **那么** 系统必须继续按顺序尝试系统 PATH 与内置后备，而不是因为缺失显式 override 就直接失败

#### 场景:诊断回显分层来源
- **当** UI 读取某个 target 的工具来源诊断
- **那么** 系统必须能够返回该 target 的 target override、global override 和最终生效来源，使 UI 可以区分“继承全局默认值”和“被 target override 覆盖”这两种状态

#### 场景:one-shot exec 使用 target 级生效路径
- **当** 某个 SSH 或 ADB target 配置了可用的 target 级 `path_override`，且用户或 AI 通过 `terminal.exec` 或 `inspect_basic` 触发一次性命令执行
- **那么** 系统必须使用该 target 当前解析出的有效工具路径拼接并执行连接命令，而不是回退到裸命令名或忽略 override

#### 场景:interactive shell 使用与诊断一致的连接器路径
- **当** 用户或 AI 为某个 SSH 或 ADB target 打开 interactive shell
- **那么** 系统必须使用与 diagnostics 和 one-shot exec 一致的有效工具路径建立该 shell，而不是另外走一套未应用 override 的启动路径

#### 场景:ADB serial 选择器优先于 transport 快捷标志
- **当** 某个 ADB target 同时提供了明确的 serial 值和 transport 提示
- **那么** 系统必须优先使用 serial 选择器建立命令或 shell 连接，而不是退化为 `-d`、`-e` 等无法唯一定位设备的快捷标志

## 移除需求

无。
