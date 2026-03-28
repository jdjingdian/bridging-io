## 为什么

当前 `bridgingio-core --self-test` 会验证执行链路与平台契约摘要，但不会显式验证默认 MCP model-plane 监听地址 `127.0.0.1:19718` 是否可绑定。尤其在 Windows standalone 场景下，一旦默认端口因端口占用、保留端口或权限类错误导致绑定失败，操作员很难通过现有自检快速拿到原始错误并区分根因。

## 变更内容

- 在 `bridgingio-core --self-test` 中新增默认 model-plane 监听地址 `127.0.0.1:19718` 的显式绑定检查。
- 当该绑定检查失败时，自检必须打印底层绑定错误细节，而不是只给出笼统的失败摘要。
- 将该检查纳入 `--self-test` 的通过/失败结果，确保默认 MCP 入口不可用时自检返回非零退出码。
- 更新相关规格与测试文档，使 self-test 覆盖范围明确包含默认 MCP 监听地址的可绑定性与错误可诊断性。

## 功能 (Capabilities)

### 新增功能

<!-- 无 -->

### 修改功能

- `target-session-management`: 扩展 `--self-test` 的规范要求，使其覆盖默认 model-plane 监听地址的绑定检查，并在失败时输出明确错误。
- `quality-and-test-automation`: 更新 core cross-platform contract/self-test 期望，明确默认 MCP 端口绑定诊断属于 contract-critical 的 smoke 覆盖范围。

## 影响

- `source/rust/bridgingio-mcp/src/bin/bridgingio-core.rs` 的 `--self-test` 流程与输出。
- `openspec/specs/target-session-management/spec.md` 与 `openspec/specs/quality-and-test-automation/spec.md` 的增量规范。
- `docs/testing/CORE_PLATFORM_CONTRACT.md` 等与 self-test 覆盖范围相关的文档。
