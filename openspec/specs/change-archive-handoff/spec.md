# change-archive-handoff 规范

## 目的
待定 - 由归档变更 define-bridgingio-foundation 创建。归档后请更新目的。
## 需求
### 需求:每次归档后必须生成符合 `.gitmessage` 的 commit message
每次 OpenSpec 变更在 archive 完成后，系统或执行该流程的助手必须参考仓库根目录的 `.gitmessage` 模板输出至少一个建议的 git commit message。该 commit message 必须符合 Conventional Commits 的 `<type>(<scope>): <subject>` 结构，并且必须遵守模板中关于语气、长度和格式的约束。

#### 场景:归档后生成提交标题
- **当** 一个变更已经完成实现、验证并执行 archive
- **那么** 系统必须基于该变更内容和 `.gitmessage` 约定输出一个合适的 git commit message，而不是只返回笼统的“ready to commit”提示

### 需求:每次归档后必须生成与变更匹配的 PR 描述
每次 OpenSpec 变更在 archive 完成后，系统或执行该流程的助手必须输出适合该变更范围的 pull request 描述。PR 描述必须至少包含变更摘要、变更原因、关键实现或设计点，以及测试或验证情况。

#### 场景:归档后生成 PR 描述
- **当** 一个变更完成 archive 并准备进入代码评审流程
- **那么** 系统必须输出一份与该变更内容一致的 PR 描述，帮助用户直接用于后续评审提交

### 需求:交付文案必须参考变更上下文
生成 commit message 和 PR 描述时，系统必须参考本次变更的 proposal、design、tasks、相关规格和验证结果，而不是仅根据最后一次对话或单个文件名生成通用模板。

#### 场景:变更包含设计和测试要求
- **当** 一个归档变更同时涉及架构设计、测试基线和 UI 约束
- **那么** 生成的 commit message 与 PR 描述必须反映这些关键变更点，而不能遗漏主要能力或测试范围

