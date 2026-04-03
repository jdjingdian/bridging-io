## 1. Token Runtime 语义

- [x] 1.1 在 `bridgingio-secrets` 中为 agent token 增加可逆的 access gate 持久化字段与状态投影优先级，确保 `deleted > revoked > expired > disabled > active`
- [x] 1.2 为 token 创建与编辑路径实现统一的别名校验器，并兼容 legacy 不合规别名的只读显示
- [x] 1.3 为 token 安全摘要增加 display-safe 指纹/摘要字段，同时继续禁止回传明文 token 和原始 `token_hash`
- [x] 1.4 增加本地受信任管理面的 token enable/disable 写接口，并保留 `delete requires revoked` 的 runtime 硬性前置条件

## 2. MCP 与共享错误映射

- [x] 2.1 将 `authenticate_agent_token()` 从 `Option` 升级为结构化认证结果，明确区分 `invalid / disabled / revoked / expired`
- [x] 2.2 在 `bridgingio-mcp` 中把 bearer token 认证失败映射到稳定的 `domain/common_code/module_code`，不再返回泛化的 `invalid_or_expired_token`
- [x] 2.3 为公共调用面的 token 认证失败补充 display-safe message 与 recovery hint，覆盖 disabled、revoked、expired、invalid 四类场景

## 3. Menuconfig Token 管理页

- [x] 3.1 重构 `bridgingio-operator-console` 的 Token Management 页面为摘要列表，并显示稳定序号、别名、有效期摘要、状态与详情导航
- [x] 3.2 新增统一的单 token 详情页，承载 `别名 = <value> --->` 直编入口、`<*>/< > 开关状态 = [启用|禁用]` 单行开关、display-safe 指纹/摘要、有效期和状态感知动作
- [x] 3.3 在详情页实现 revoke/delete 的状态裁剪与二次确认，并对“未撤销不可删除”提供本地受控提示而不是运行时崩溃
- [x] 3.4 为详情页补充 `权限管理 --->` 单箭头入口（不重复箭头、不重复 token_id 文案），并在 scope 编辑器未落地时返回受控占位反馈
- [x] 3.5 更新 `menuconfig` 相关 i18n 文案、帮助文本与状态栏提示，使 alias 校验、disabled 状态和认证失败语义可读

## 4. 验证与矩阵文档

- [x] 4.1 为 `bridgingio-secrets` 增加 alias 校验、enable/disable、终态优先级与 legacy label 兼容测试
- [x] 4.2 为 `bridgingio-mcp` 增加 token invalid/disabled/revoked/expired 的认证失败与错误子码测试
- [x] 4.3 为 `bridgingio-operator-console` 增加 Token Management 列表/详情流、删除隐藏、权限入口占位反馈，以及“开关状态仅空格触发、Enter 不触发切换”的交互测试
- [x] 4.4 更新实际的 operator interface matrix / 错误契约相关文档或夹具，使 token 列表-详情拓扑与错误子码合同可被回归校验

## 5. macOS 钥匙串点检补充

- [x] 5.1 收敛 `menuconfig -> unlock_vault` 链路中的钥匙串访问次数，消除由 readiness/probe 路径导致的重复授权弹窗
- [x] 5.2 在 spec 中补充平台边界：剩余一次“访问钥匙串中的密钥”提示可由 macOS Keychain ACL 决定，属于平台安全行为而非业务回归
