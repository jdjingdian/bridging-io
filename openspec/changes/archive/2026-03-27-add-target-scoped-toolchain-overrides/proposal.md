## 为什么

当前工具来源能力同时存在产品语义和实现语义的错位：

- 目标编辑页里的工具来源输入框会出现在 SSH 和 ADB 两种行上，但保存 target 时并不会把这些字段写进 profile，导致用户在目标页中填写 override 后，配置文件里没有对应落盘结果。
- 现有可落盘的工具覆盖只存在于全局 `toolchains.<name>.path_override`，无法表达“同一台机器上两个 target 使用不同 adb/ssh 版本”的真实需求。
- 当前 UI 把 target 级编辑和全局级编辑混在一起：目标编辑页对所有 target 都显示 `ssh` 和 `adb`，工作台主界面也直接承载了全局 override 的写入口，容易让用户误以为自己正在修改当前 target。
- 现有诊断只回显最终生效路径，不足以区分“target 覆盖了全局默认值”还是“只是命中了全局默认值”，后续排查会越来越困难。

如果不把配置分层、UI 入口和诊断回显同时理顺，后续即使补一个“能保存”的字段，也会继续在 target/global 语义上制造混乱。

## 变更内容

- 在保留全局 `toolchains.<name>.path_override` 的前提下，为 target profile 新增 `targets.toolchains.<name>.path_override`，作为当前 target 的局部覆盖层。
- 明确定义工具解析优先级为：`targets.toolchains.<name>.path_override -> toolchains.<name>.path_override -> system PATH -> built-in fallback`。
- 扩展 core-owned profile/settings/app API/diagnostics 契约，使 target 级与全局级 override 都能被读取、保存、落盘和诊断回显。
- 调整 macOS UI 分层：
  - 目标编辑页只展示当前 target 相关的工具 override；
  - 系统设置页承担全局 toolchain 默认值编辑；
  - 工作台 target 详情中的工具来源卡片回归诊断展示，不再承载全局 override 写入口。
- 补齐自动化验证，覆盖 target override、global override、回退链路、UI 可见性以及保存后真实生效路径。

## 功能 (Capabilities)

### 新增功能

无。

### 修改功能

- `target-session-management`: 增加 target 级 toolchain override 配置层、分层解析顺序、profile/settings/diagnostics round-trip，以及全局与 target 写入口的统一契约。
- `macos-operator-console`: 调整工具来源 UI 分层，目标编辑页只展示当前 target 相关工具，系统设置页承载全局默认值编辑，工作台详情页回归诊断展示。

## 影响

- 受影响的 Rust 代码包括 `bridgingio-engine` 配置模型与 TOML 序列化、`bridgingio-domain` profile 数据结构、`bridgingio-app-api` profile/settings 契约、`bridgingio-mcp` 的 profile/settings 持久化与 diagnostics 组装。
- 受影响的 SwiftUI 代码包括 `TargetProfileSheetView`、`SettingsSheetView`、`WorkspaceConsoleView`、`WorkspaceViewModel` 以及 profile/settings 读取与保存流程。
- 需要新增或更新自动化测试，覆盖两层 override 的读写、分层回退、诊断回显与 UI 入口可见性。
