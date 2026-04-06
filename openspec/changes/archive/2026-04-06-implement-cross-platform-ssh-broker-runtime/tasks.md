## 1. Broker Runtime Foundation

- [x] 1.1 在 `bridgingio-secrets` 中引入 `SshBrokerRuntime` 抽象、运行中 handle 与 broker session state 收紧模型
- [x] 1.2 改造 `prepare_ssh_agent_broker_session()`，使其只有在 endpoint 真正 bind/listen 成功后才返回 `ready`
- [x] 1.3 为 broker session 增加 startup failure、draining、close 与 cleanup 的统一状态迁移和 display-safe diagnostics
- [x] 1.4 收紧 broker session 内部存储边界，确保 signer material 与 runtime locator internals 不进入 display-safe projection 或普通持久层

## 2. Platform Endpoint Adapters

- [x] 2.1 实现 Unix broker runtime，使用私有 runtime 目录下的 Unix domain socket 提供真实 agent-compatible endpoint
- [x] 2.2 为 Unix broker runtime 补齐 endpoint bind/listen、request loop、shutdown 与 socket/runtime-dir cleanup
- [x] 2.3 实现 Windows broker runtime 的 named pipe 或 platform-local adapter 路径，并把平台差异封装在 adapter 内部
- [x] 2.4 为 Windows broker runtime 补齐 startup/readiness、shutdown 与 cleanup 语义，并产出与 Unix 对齐的 session contract
- [x] 2.5 以真实 contract probe 验证 Windows 当前采用的 endpoint 注入形式，确保 caller 不需要为平台差异自行降级到明文 identity file

## 3. SSH Delivery And Invocation Wiring

- [x] 3.1 在 shared SSH preparation 路径中区分 vault broker 与 direct identity 两类 credential source
- [x] 3.2 为 vault-backed SSH 接入真实 broker startup、attach/detach/close 生命周期，并移除“endpoint hint 即 ready”的旧假设
- [x] 3.3 为 direct identity 路径实现显式 `-i <path>` / `IdentityFile=<path>` 注入，并同时加上 `IdentitiesOnly=yes` 或等价限制
- [x] 3.4 为 secret-backed 与 direct identity 两条路径补齐 ambient `SSH_AUTH_SOCK` / 等价 agent 环境的隔离逻辑
- [x] 3.5 让 `bridgingio-mcp`、`bridgingio-engine` 与 shared SSH probe/invocation helper 共用同一套 broker / bypass 决策逻辑

## 4. Operator Surface And Diagnostics

- [x] 4.1 改造 `menuconfig` SSH test connection，使 vault-backed 测试只在 broker runtime 真正 ready 时下发 endpoint
- [x] 4.2 为 `menuconfig` direct identity 测试接入显式 bypass 语义，避免创建 broker session 或误报 broker unavailable
- [x] 4.3 收紧 SSH 失败分类与提示，把 broker startup/readiness 失败、direct identity 失败与通用认证失败稳定区分
- [x] 4.4 更新 self-test 与 display-safe diagnostics，确保 broker lifecycle、degraded fallback 与 direct identity bypass 都走正式共享路径
- [x] 4.5 记录首轮完成标准与后续扩展项边界，明确哪些协议/功能增强属于后续演进而不是本轮验收

## 5. Verification And Contract Coverage

- [x] 5.1 为 `bridgingio-secrets` 增加 broker runtime 单元测试，覆盖 startup、ready gating、cleanup 与 failure paths
- [x] 5.2 为 `bridgingio-mcp` / shared runtime 增加集成测试，覆盖 vault-backed broker success、bind failure、degraded fallback 与 direct identity bypass
- [x] 5.3 为 `bridgingio-core --self-test` 增加或更新 contract smoke，验证 Unix / Windows broker readiness、minimal publickey auth compatibility 与 cleanup 语义
- [x] 5.4 为 `menuconfig` SSH test connection 增加回归测试，覆盖 broker unavailable、direct identity bypass、locked preflight 与 display-safe logging
- [x] 5.5 复核变更中的 proposal/design/specs 与矩阵文档，确保实现完成后文档、错误分类与测试矩阵保持一致

## 6. Debug Follow-up (2026-04-05)

- [x] 6.1 记录并修复 menuconfig 错误分类误判：`stderr` 同时出现 `ssh_get_authentication_socket_path` 与 `no such identity` 时不得误报 `broker-endpoint-unready`；仅在真实 agent 不可用签名（`ssh_get_authentication_socket: No such file` / `communication with agent failed`）命中时归类为 broker 未就绪
- [x] 6.2 记录并修复 vault broker 参数策略：broker 模式显式使用 `IdentityAgent=<endpoint>` + `IdentitiesOnly=no`，避免 agent key 被宿主配置 `IdentitiesOnly=yes` 过滤；identity-file/direct 路径继续使用 `IdentitiesOnly=yes` + `IdentityAgent=none`
- [x] 6.3 为上述两类修复补充回归：覆盖“agent returned key + local default identity 不存在”时应归类 `auth-publickey-rejected`，并在 self-test 中区分 broker 与 fallback 的 `IdentitiesOnly` contract

## 7. Local Preflight Follow-up (2026-04-06)

- [x] 7.1 在 `run-local-cross-platform-preflight.py` 中为 Windows 增加 named-pipe runtime contract probe 入口，确保命中 contract mode（默认 `compile-only`/`extended`）时可执行实际 runtime 验证步骤
- [x] 7.2 为 local preflight Windows 配置增加 contract probe 开关/命令/模式映射，并把执行状态写入 summary/log
- [x] 7.3 更新 `LOCAL_CROSS_PLATFORM_PREFLIGHT.md` 与示例配置，明确如何通过 preflight 覆盖 Windows named-pipe runtime 合同
- [x] 7.4 更新 quality-and-test-automation spec，记录 Windows preflight 在命中 contract mode（默认 `compile-only`/`extended`）时必须执行 named-pipe runtime contract probe

## 8. Windows Extended Stabilization Follow-up (2026-04-06)

- [x] 8.1 修复 `bridgingio-mcp/tests/integration_workflows.rs` 的 Windows mock toolchain 可执行名，确保 `ssh`/`adb` mock 在 Windows 解析路径下使用 `.bat` 形式并被 structured invocation 稳定命中
- [x] 8.2 为 `integration_workflows` 中 git 依赖段落增加“宿主缺失 git 时的显式跳过”保护，避免环境差异导致与 broker runtime 无关的假阴性失败
- [x] 8.3 收敛 `bridgingio-providers` 交互 shell 用例为跨平台断言：Windows/Unix 分别使用对应 env/cwd 命令，并在 degraded backend 下允许非严格 cwd 结果但要求显式 diagnostics
- [x] 8.4 收敛 `bridgingio-secrets` broker 回归用例的跨平台断言：`host_platform` 按当前平台选择，identity fallback 路径断言统一做分隔符归一化
- [x] 8.5 复跑 `python3 scripts/testing/run-local-cross-platform-preflight.py --target windows --mode extended`，确认 `integration_workflows`、`bridgingio-providers`、`bridgingio-secrets` 在 Windows extended 路径全部通过
