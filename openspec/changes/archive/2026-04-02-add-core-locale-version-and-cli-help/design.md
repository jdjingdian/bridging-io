## 上下文

当前 `bridgingio-core` 的 CLI 与 `menuconfig` 已经是 core-first operator surface，但它们还没有共享的展示层基础设施：

- `bridgingio-core` 通过手写 `print_usage()` 输出 help，命令说明、留白和强调样式都很有限。
- `bridgingio-operator-console` 中存在大量英文硬编码文本，包括菜单标题、字段标签、状态栏、帮助面板和弹窗文案。
- workspace `source/rust/Cargo.toml` 与各 member crate 仍重复写 `version = "0.1.0"`，导致“core 版本唯一真相”尚未建立。
- 配置模型尚未提供一个明确声明“core 自己用什么语言”的字段；future UI 若继续扩展自己的多语言能力，不能直接拿 core 的 operator locale 当通用 UI locale。

这次变更横跨 `bridgingio-engine`、`bridgingio-mcp`、`bridgingio-operator-console` 和 workspace Cargo 元数据，属于明显的跨模块行为合同调整，适合先通过设计收口关键决策。

## 目标 / 非目标

**目标：**

- 为 core-owned operator surfaces 建立共享的双语 catalog，首批仅支持 `zh-CN` 与 `en-US`。
- 为 core-owned 配置增加独立的 operator locale 字段，并让 `menuconfig` 可查看与修改该字段。
- 让 `bridgingio-core --version`、CLI help、以及任何对外暴露的 core 版本元数据都只读取 workspace 主 `Cargo.toml` 的 `version`。
- 将 CLI help 升级到类似 `xgit` 的分段、留白、标题强调体验，同时保持既有命令语义稳定。
- 用自动化校验阻断 catalog 漏翻译、operator-facing 硬编码文本和版本来源漂移。

**非目标：**

- 本次不为 future GUI 定义通用 locale 真相，也不要求 UI 消费 core 的 operator locale。
- 本次不引入第三种语言、远端翻译平台或动态下载语言包。
- 本次不改变 core 现有 `run` / `-d` / `menuconfig` / `vault` / `auth` 的业务语义，只改进其展示、配置与版本治理方式。
- 本次不要求把所有结构化错误码都本地化；稳定 machine-readable code 继续保持不翻译。

## 决策

### 决策 1：新增共享的 core i18n catalog 层，资源文件独立于代码保存，但编译进 core

实现上引入一个共享 catalog 层，资源文件采用与 `xgit` 相同的思路，使用独立的 `en-US.toml` / `zh-CN.toml` 键值文件维护 operator-facing 文案。CLI 与 `menuconfig` 都通过 key 查表，而不是在渲染路径直接写展示文本。

为避免 bundled / standalone 场景再额外依赖运行时资源定位，catalog 在构建时直接编译进 core；代码中保留的是稳定 key，而不是展示文案本身。

备选方案：
- 运行时从磁盘加载资源：更接近 `xgit`，但会引入 bundled 发行物中的路径管理复杂度。
- 继续用 Rust 常量字符串：实现快，但不满足“展示层禁硬编码”和双语扩展目标。

### 决策 2：配置模型新增独立的 `core.operator_locale` 字段，只影响 core-owned surfaces

在 `CoreSettings.core` 下新增独立 locale 字段，暂定键名为 `operator_locale`，合法值仅允许 `zh-CN` 与 `en-US`。该字段属于 core-owned settings：

- `bridgingio-core` CLI help / version / about 使用它。
- `menuconfig` 的标题、字段说明、状态栏、帮助面板和弹窗使用它。
- future UI 不得把该字段当成自己的 locale 真相；UI 可以忽略它，或仅把它当“core 当前语言”的只读辅助信息。

默认值保持 `en-US`，这样在未显式配置时不改变当前英文行为，并降低已有脚本与截图文档的回归风险。

备选方案：
- 将字段命名为通用 `lang` / `locale`：太容易被 future UI 误当成跨前端通用设置。
- 按系统 locale 自动切换：会让 core 行为失去可预测性，也不利于测试快照稳定。

### 决策 3：workspace 主 `Cargo.toml` 的 `version` 是 core 版本唯一真相，member crate 必须继承它

版本治理统一以 `source/rust/Cargo.toml` 的 `[workspace.package].version` 为唯一真相，并约束其格式为 `YYMM.DD.BuildNumber`。所有 member crate 改为继承 workspace version，而不是各自维护独立 `version = "..."` 文本。

对外行为上：

- `bridgingio-core --version` 必须输出当前 workspace version。
- 任何通过 MCP / control-plane / self-test 暴露的 core version 元数据必须与之完全一致。
- 发布时只允许修改主 `Cargo.toml` 的 version；代码不得保留第二份硬编码版本常量。

备选方案：
- 保留各 crate 独立版本号：容易出现 core CLI、库 crate 和对外元数据不一致。
- 额外维护一个 `VERSION` 文件：会形成第二个真相源，与 Cargo 元数据脱节。

### 决策 4：CLI help 改为由命令元数据驱动渲染，并支持像 `xgit` 一样的分组留白与标题强调

`bridgingio-core` 的 CLI 前端需要从“手写 usage 文本”升级为“命令元数据 + 统一渲染模板”。推荐引入 `clap` 这类成熟 CLI 元数据层，原因是它天然支持：

- `--help` / `help` / `--version` 的正式入口
- 分组命令说明、空行与标题样式
- TTY 下的 ANSI 加粗/强调，以及非 TTY 下的纯文本回退
- 将 locale catalog 注入 command/about/help 文案

解析行为需要保持与现有命令兼容：`menuconfig`、`run`、`-d`、`ui-managed-ephemeral`、`vault ...`、`auth ...` 继续保留，只是 help 生成机制改为统一元数据驱动。

备选方案：
- 继续维护手写 `print_usage()`：低改动，但 help、version 和 i18n 三者会继续分叉。
- 自己实现 ANSI 模板渲染器：可控，但重复造轮子且更难维护子命令层级。

### 决策 5：`menuconfig` 全量切换到 catalog key，搜索仍保留 canonical path 可发现性

`menuconfig` 中的 operator-facing 字段名、描述、状态、帮助、弹窗、按钮标签都迁移到 catalog key。与此同时，配置字段的 canonical path 仍保持英文稳定路径（例如 `core.instance_name`、`core.operator_locale`），以保证：

- 配置写回与测试断言仍然稳定
- 搜索既可以命中本地化标签，也可以命中 canonical path
- future UI 若复用字段描述，不会被翻译后的显示文本反向绑死

这意味着显示文本本地化，但配置真相与内部标识不本地化。

备选方案：
- 只本地化标题，不本地化字段描述与状态：会产生体验割裂。
- 连 canonical path 也本地化：会破坏配置与测试的稳定性。

### 决策 6：自动化校验同时覆盖 catalog 完整性、help/version 合同和硬编码回归

新增三类自动化保障：

- catalog 键集合一致性检查：`zh-CN` 与 `en-US` 不允许缺 key。
- help/version contract tests：验证 `--help` / `--version` 在两种 locale 下的关键输出结构、版本值和 ANSI/plain-text 回退行为。
- operator-facing 文案回归检查：针对 `bridgingio-mcp` CLI 与 `bridgingio-operator-console` 的显示层路径，阻断新增硬编码展示文本。

备选方案：
- 只依赖 code review：很难长期阻止硬编码回流。
- 只做 snapshot，不做 key parity：会把缺翻译问题推迟到运行期才暴露。

## 风险 / 权衡

- [风险] CLI 从手写 parser/help 迁移到命令元数据层时，可能出现参数兼容性回归  
  → 缓解措施：为现有命令形态补齐解析测试与 help snapshot，对关键路径做 before/after contract 对比。

- [风险] catalog key 漏配会导致 `menuconfig` 或 help 出现空文案  
  → 缓解措施：增加 key parity 检查，并在运行时对缺 key 回退到 `en-US` 与 canonical key 诊断。

- [风险] ANSI 加粗在某些终端或管道场景中不可用  
  → 缓解措施：仅在支持样式的终端启用强调；非 TTY 输出保持无转义序列的纯文本结构。

- [风险] “core locale” 与 “future UI locale” 的边界如果命名不清，后续容易被误用  
  → 缓解措施：用明确字段名和规范文本强调该字段只影响 core-owned surfaces，并同步更新接口矩阵与文档。

## 迁移计划

1. 在 `bridgingio-engine` 中新增并序列化 `core.operator_locale`，默认值为 `en-US`，更新 fixture 与示例配置。
2. 新增共享 catalog 层与 `zh-CN` / `en-US` 资源文件，让 CLI 和 `menuconfig` 都能按同一 locale 取文案。
3. 收口 workspace 版本治理：member crate 继承 workspace version，`bridgingio-core --version` 与相关元数据统一改为读取 Cargo 版本真相。
4. 用统一命令元数据替换当前手写 help 渲染，接入 locale catalog 和更好的输出样式。
5. 批量迁移 `menuconfig` 硬编码文案到 catalog，并把语言切换入口放到 core/general 设置页。
6. 增加 contract tests、lint/脚本校验与文档更新，确保后续新增 operator-facing 文案和版本输出不再绕过统一入口。

## Open Questions

- 是否需要在本次同时把部分面向人工阅读的结构化错误 message 也切换到 catalog，还是先只覆盖 help、status、menuconfig 和 about 文案。
