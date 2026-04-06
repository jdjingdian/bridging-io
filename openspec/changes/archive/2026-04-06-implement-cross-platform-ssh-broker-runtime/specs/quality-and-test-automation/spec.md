## ADDED Requirements

### 需求:跨平台 SSH broker runtime 必须具备真实 readiness 与 bypass 自动化覆盖
对于 vault-managed SSH key 的 broker runtime、degraded fallback 与 direct identity bypass，系统必须提供跨平台自动化验证，证明 readiness 对应真实可用 endpoint、cleanup 语义成立、direct identity 不依赖 broker，且相关日志与错误分类保持 display-safe。

#### 场景:Unix 与 Windows contract automation 验证 broker 真正 ready
- **当** 团队在受支持宿主平台上执行 SSH broker contract automation 或等价 self-test
- **那么** 自动化必须验证 broker runtime 已真实 bind/listen 平台本地 endpoint 后才进入 `ready`
- **并且** 必须验证该 endpoint 可以完成最小但完整可用的 SSH publickey auth 交互，而不是只验证 locator 字符串存在

#### 场景:broker startup 失败不得伪装为 ready
- **当** 自动化测试模拟 broker runtime 在 bind、listen、adapter startup 或清理阶段失败
- **那么** 测试必须验证系统返回受控的 broker startup failure、degraded 或 unsupported 结果
- **并且** 不得允许一个未真实就绪的 endpoint 被记录为 ready 并下发给 SSH 客户端

#### 场景:direct identity bypass 保持独立合同
- **当** 自动化测试使用本地 identity 文件而不是 canonical vault ref 发起 SSH 探测或执行
- **那么** 测试必须验证运行时走显式 `-i <path>`、`IdentityFile=<path>` 或等价 direct identity 路径
- **并且** 必须验证该调用不创建 broker session，且失败时不会误报为 broker unavailable

#### 场景:broker cleanup 与日志边界具备自动化断言
- **当** 自动化测试完成一次 vault-backed SSH one-shot、interactive attach/detach 或 timeout/cancel 流程
- **那么** 测试必须验证 broker endpoint、runtime handle、内存 signer material 与任何 degraded fallback 文件都进入清理流程
- **并且** 必须验证相关日志只包含 display-safe `flow_id`、`credential_ref`、状态与错误分类，而不包含私钥明文、passphrase、token 明文或 raw sign payload

#### 场景:local cross-platform preflight 在 Windows 受控模式执行 named-pipe runtime contract
- **当** 开发者执行 `python3 scripts/testing/run-local-cross-platform-preflight.py --target windows`，且当前 mode 命中 `[windows.contracts].modes`（默认包含 `compile-only` 与 `extended`）
- **那么** preflight 必须在 Windows host 上执行 named-pipe runtime contract probe 命令，覆盖实际 runtime 启动路径
- **并且** 该 probe 至少应包含可验证 `bridgingio-core --self-test` 中 broker ready/cleanup 合同的步骤
- **并且** probe 失败必须使该 Windows host preflight 结果失败，而不是仅记录 warning

#### 场景:Windows extended preflight 的功能测试必须具备跨平台可移植断言
- **当** 团队在 Windows host 执行 `--mode extended` 并进入 workspace 级功能测试（含 `integration_workflows`、providers、secrets 回归）
- **那么** 测试基线必须使用可在 Windows 真实解析的 mock 可执行命名与调用约定（例如 `ssh.bat` / `adb.bat`），避免因宿主可执行解析差异导致 `resolved_target_id`、`resolution_state` 等结构化断言失真
- **并且** 对宿主环境工具缺失（如 `git`）的断言应采用显式 skip/guard，而不是让与 broker runtime 无关的外部依赖缺失污染合同结论
- **并且** 涉及文件路径与交互 shell cwd 的断言必须使用分隔符归一化与 backend 能力分层：非 degraded backend 维持严格断言，degraded backend 允许受控放宽但必须输出可诊断标记
