## 为什么

当前 `bridgingio-core` 的 vault 与 token 管理语义仍然存在两处不清晰：一是 `menuconfig` 的 Security 页面只提供被动摘要与解锁入口，未把 `uninitialized`、`locked`、`unlocked` 三种核心状态分离成明确的管理路径；二是 token 目前只有 create/revoke 语义，没有定义“expired 后如何进入 delete”的正式生命周期，也没有把 token 创建时的别名输入、有效期选择和一次性明文回显整理成稳定交互。这使测试、运维和 UI 设计都容易围绕隐式行为做出错误假设。

现在需要把 vault 与 token 管理面收敛成正式、可测试的状态机，避免 `menuconfig` 在 vault 缺失时仍走隐藏 unlock 路径，也避免不同 operator surface 对 token delete 前置条件、token 创建引导以及“本机系统时间不可靠时如何处理 expiring token”产生分歧。

## 变更内容

- 明确 `menuconfig` Security 页必须根据 vault 的真实状态裁剪动作，而不是在任意状态下展示同一组入口。
- 为 vault 增加正式的 `uninitialized/init/delete` 管理语义，使“删除 vault 以回到测试初始态”成为受控操作，而不是依赖手工删目录和隐式 bootstrap。
- 明确 token 生命周期必须收敛为 `active -> expired? -> revoked -> deleted`，其中 `revoked` 是唯一的 delete 前置条件；`expired` token 也必须先 revoke 才能 delete。
- 在 Security 页中收敛 token 入口为 `Create Token` 与 `Token Management`；在 `Token Management` 子页中允许查看 display-safe token 摘要、编辑备注、执行 revoke 和 delete。
- 把 `Create Token` 定义为显式三步交互：先输入 token 别名（`label`），再选择有效期（长期或本机本地时间下的指定失效时间），最后在一次性结果弹窗中展示明文 token。
- 明确 Security 与 Token Management 中会进入流程/确认链路的动作统一使用 `--->` 导航样式，避免 `*** ... ****` 的即时动作样式造成误解。
- 明确 `Create Token` 的编辑弹窗字段标题必须使用 operator-facing 文案（例如“创建 Token 别名”），不得暴露内部字段键名。
- 明确 expiring token 的有效期依赖本地时钟，但运行时必须具备反回拨保护或等价的“不可因时钟后退而重新激活已过期 token”的语义，并在时钟异常时给出明确提示。
- 明确 token 的 revoke/delete 以及 vault delete 都必须具备二次确认流程，并继续遵守 display-safe projection 约束。
- 同步更新本地 operator interface matrix，使 CLI / TUI / trusted host 对 `vault init`、`vault delete`、`token create`、`token revoke`、`token delete` 的输入输出与状态合同保持一致。

## 功能 (Capabilities)

### 新增功能
- 无

### 修改功能
- `standalone-operator-console`: 调整 menuconfig 的 Security / Token Management 信息架构、状态显示、确认交互与 vault 状态门控。
- `credential-and-approval-control`: 明确 vault 初始化/删除语义、token 生命周期与 delete 前置条件，以及高风险管理动作的确认约束。
- `operator-interface-matrix`: 补充本地 operator surface 的 vault/token 管理接口矩阵，覆盖新增的 delete 语义与状态合同。

## 影响

- 受影响模块：`bridgingio-operator-console` 的 Security/TUI 流程，`bridgingio-secrets` 的 vault 与 token 生命周期，`bridgingio-core` 的本地管理命令面与错误语义。
- 受影响体验：操作员在 `menuconfig` 中将看到更严格的 vault 状态门控、三步式 `Create Token` 流程，以及单独的 Token Management 子页与二次确认流程。
- 受影响测试：需要覆盖 vault `uninitialized/locked/unlocked` 状态切换、vault delete 回到初始态、expiring token 的时钟展示与回拨保护、expired token revoke 后 delete，以及确认弹窗取消/确认路径。
