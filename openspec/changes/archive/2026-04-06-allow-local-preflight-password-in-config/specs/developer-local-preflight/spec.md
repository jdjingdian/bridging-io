## MODIFIED Requirements

### 需求:本地 preflight 配置必须与仓库跟踪内容隔离
本地 preflight 的真实宿主信息必须保存在 gitignored 配置中，仓库内仅允许跟踪示例配置文件与说明文档。示例配置禁止包含真实 IP、端口、用户名、私钥路径、密码或其他个人环境细节，且示例文件本身不得被自动视为有效运行配置。

#### 场景:仓库提供示例配置
- **当** 项目为本地 preflight 提供示例配置文件
- **那么** 该示例文件必须只展示字段结构与默认语义，而禁止包含可直接连接真实宿主的个人环境数据或真实密码
- **并且** 该示例文件必须作为仓库内可发现的 example 模板提供给开发者复制与本地化

#### 场景:示例配置不会意外触发远端运行
- **当** 开发者尚未创建自己的 gitignored 本地配置，只存在仓库跟踪的示例配置
- **那么** 本地 preflight 入口必须不把示例文件当作激活配置，而是继续返回 not-configured 或要求显式复制/指定本地配置

## ADDED Requirements

### 需求:本地 preflight 必须支持可选的非交互密码认证
本地 preflight runner 必须支持在 gitignored 本地配置中声明可选 `password` 字段，用于 SSH/SCP 非交互认证。该能力必须保持“key 优先、密码次之、交互兜底”的行为语义，并且不得把密码明文写入日志、摘要或命令参数回显。

#### 场景:配置 password 时可非交互执行
- **当** 开发者为某个启用宿主配置了 `password`，且未配置 `identity_file`
- **那么** runner 必须能够在不提示终端输入密码的前提下完成 SCP 传输与 SSH 远端执行
- **并且** 失败时必须返回受控失败信息，而不是无限等待交互输入

#### 场景:identity_file 与 password 同时存在时 key 优先
- **当** 开发者同时配置 `identity_file` 与 `password`
- **那么** runner 必须优先使用 `identity_file` 路径
- **并且** 不得在该次运行中将 password 作为隐式回退输入

#### 场景:未配置 key 与 password 时保持既有交互行为
- **当** 启用宿主未配置 `identity_file` 且未配置 `password`
- **那么** runner 必须保持现有交互式 SSH/SCP 密码提示行为
- **并且** 不得把该情况误判为配置错误

#### 场景:日志与摘要不得泄露密码
- **当** runner 使用 password 模式执行 preflight
- **那么** 本地 run log、summary 与终端辅助输出必须不包含密码明文
- **并且** 任何认证相关日志必须仅输出脱敏状态信息（例如是否启用 password 模式）

### 需求:Windows compile-only preflight 必须守住平台条件编译边界
当本地 preflight 在 Windows 宿主执行 compile-only（`cargo test --workspace --no-run`）时，仓库实现必须满足平台条件编译边界，不得因为 Unix-only 类型或导入泄漏到 Windows 编译路径而失败。该合同既覆盖正常源码，也覆盖测试模块与二进制入口的导入边界。

#### 场景:Unix-only IPC 类型不得在 Windows 测试导入阶段触发失败
- **当** 某个测试模块依赖仅在 `#[cfg(unix)]` 下存在的类型（例如控制平面 Unix socket IPC client/server）
- **那么** 该模块必须通过 `#[cfg(unix)]` 或等价机制限制导入与引用范围
- **并且** Windows compile-only preflight 不得因导入阶段 `E0432 unresolved import` 失败

#### 场景:平台专属导入清理属于 preflight 通过前置
- **当** Windows compile-only preflight 暴露 platform-specific 的 unused import 或误导入回归
- **那么** 团队必须在本轮变更内清理并复测到 Windows preflight 通过
- **并且** 相关修复事项必须记录到任务清单，避免后续规范丢失
