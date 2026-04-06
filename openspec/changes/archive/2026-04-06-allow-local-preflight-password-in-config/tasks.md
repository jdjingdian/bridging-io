## 1. Spec And Safety Boundary Alignment

- [x] 1.1 更新 `developer-local-preflight` 规格，明确本地 gitignored 配置允许可选 `password` 字段
- [x] 1.2 明确认证优先级（`identity_file` > `password` > 交互提示）与回退语义
- [x] 1.3 明确日志与 summary 的脱敏要求，禁止密码值进入可观测产物

## 2. Runner Implementation

- [x] 2.1 扩展 `run-local-cross-platform-preflight.py` 配置解析，支持读取 host 级 `password` 字段
- [x] 2.2 为 `ssh/scp` 调用增加 password 非交互认证路径，并保证不通过 argv 传递明文密码
- [x] 2.3 保持 key 优先逻辑：当 `identity_file` 存在时，password 字段不参与本次认证
- [x] 2.4 保留无 key/无 password 时的交互提示行为，确保向后兼容

## 3. Docs And Example Update

- [x] 3.1 更新 `scripts/testing/local-preflight.example.toml`，新增 `password` 字段注释模板与风险提示（不提供真实值）
- [x] 3.2 更新 `docs/testing/LOCAL_CROSS_PLATFORM_PREFLIGHT.md`，补充密码模式说明与推荐实践（优先 key）

## 4. Verification

- [x] 4.1 验证无 `identity_file` 且配置 `password` 时，Windows/Linux preflight 可非交互执行
- [x] 4.2 验证配置 `identity_file` + `password` 时，runner 走 key 认证且不使用 password
- [x] 4.3 验证无 key/无 password 时，runner 仍出现交互提示并保持既有行为
- [x] 4.4 验证 run log 与 summary 不包含密码明文

## 5. Windows Compile Hygiene Follow-up (2026-04-06)

- [x] 5.1 记录并修复 Windows 目标编译错误：`bridgingio-mcp` 测试模块对 `#[cfg(unix)]` 的 `ControlPlaneIpcClient/Server` 采用了无条件导入，导致 Windows `cargo test --workspace --no-run` 出现 `E0432 unresolved import`
- [x] 5.2 收紧跨平台 import 边界：对仅 Unix 使用的类型与 helper 统一补齐 `#[cfg(unix)]`（含测试与二进制入口导入），消除 Windows 编译路径下的误导入
- [x] 5.3 以本地 Windows preflight 复测上述修复：`python3 scripts/testing/run-local-cross-platform-preflight.py --target windows` 通过，并确认 compile-only 输出不再包含本轮定位的 unused import 警告
