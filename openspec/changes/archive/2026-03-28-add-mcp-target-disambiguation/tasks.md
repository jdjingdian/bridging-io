## 1. 配置与 target 描述基础

- [x] 1.1 为 core-owned settings 增加 MCP target resolution policy，并实现默认值 `confirm_if_family` 的解析、校验与序列化
- [x] 1.2 在 runtime 中建立 richer target descriptor/index，保留 canonical target id、enabled、display name、全部 aliases、notes 与连接摘要
- [x] 1.3 让 disabled target 从 MCP 可执行 target 解析集合中排除，并确保其不会进入确认候选列表

## 2. 统一 resolver 与确认模型

- [x] 2.1 实现共享的 target resolver 结果模型，明确区分 `resolved`、`confirmation_required` 与 `not_found`
- [x] 2.2 实现低风险自动命中规则，覆盖大小写与 `-` / `_` / 空格差异等宽松归一化唯一命中
- [x] 2.3 实现高风险候选发现规则，覆盖 family-related 边界匹配与 typo-related 候选分类
- [x] 2.4 实现基于 policy 的决策逻辑，覆盖 `auto_execute`、`confirm_if_family`、`confirm_if_related` 与 `confirm_always`
- [x] 2.5 实现结构化确认 payload builder，返回 policy reason、exact match、related candidates 与静态/缓存摘要，并禁止主动远端探测

## 3. MCP tools 接入

- [x] 3.1 将 `bridgingio.terminal.exec` 接入统一 target resolver，并在需要确认时返回无副作用的结构化确认结果
- [x] 3.2 将 `bridgingio.terminal.shell.open` 接入统一 target resolver，并在需要确认时禁止创建 transport、channel 或 shell 句柄
- [x] 3.3 将 `bridgingio.target.inspect_basic` 接入统一 target resolver，并在需要确认时禁止执行任何远端探测命令
- [x] 3.4 对所有接收 `target` 参数的 MCP tool 对齐返回字段，确保解析与确认语义一致

## 4. 验证与回归

- [x] 4.1 为 resolver 增加单元测试，覆盖多 alias、disabled filtering、宽松归一化、family 边界与策略分支
- [x] 4.2 为 MCP 增加集成测试，覆盖 typo 候选确认、family 歧义确认、`auto_execute` 覆盖行为与无副作用确认语义
- [x] 4.3 复核现有 target 相关 MCP 回归用例，确保未引入对已解析 canonical target 的执行行为回退
