## 为什么

当前 `bridgingio-core` 的 operator-facing 文案仍然分散在代码里：CLI help 由手写 `print_usage()` 输出，`menuconfig` 里存在大量英文硬编码，workspace 与各 crate 的版本号也处于重复声明状态。这会让中文/英文双语支持、版本维护和后续 UI/core 边界演进都变得脆弱。

现在需要把 core 自己的语言、版本和帮助输出统一成正式合同：core 仅支持 `zh-CN` 与 `en-US` 两种语言，所有展示文案走 catalog；core 版本只认主 `Cargo.toml` 的 `version`；CLI help 需要像 `xgit` 一样提供更清晰的分段、留白和加粗效果。

## 变更内容

- 为 `bridgingio-core` 建立共享的中英文 catalog，覆盖 CLI help / version / about 与 `menuconfig` 的 operator-facing 文案，禁止在这些显示路径继续硬编码展示文本。
- 在 core-owned 配置中新增独立的 `core.operator_locale`（命名待最终实现确认）字段，只用于表明 core 自己的语言；future UI 不得消费该字段作为自己的多语言真相。
- 将 core 版本治理收口到 workspace 主 `Cargo.toml` 的 `version`，格式要求为 `YYMM.DD.BuildNumber`，代码中禁止再维护独立硬编码版本。
- 优化 `bridgingio-core --help` / `help` 的输出结构，使其具备分组留白、标题强调和更稳定的命令说明布局，并与 locale catalog 联动。

## 功能 (Capabilities)

### 新增功能
- `core-cli-surface`: 定义 `bridgingio-core` 的本地化 CLI 文案、版本输出真相和格式化 help 合同。

### 修改功能
- `standalone-operator-console`: `menuconfig` 需要提供 core 语言配置入口，并将 operator-facing 文案改为通过 catalog 渲染。
- `target-session-management`: core-owned 配置模型需要新增独立的 core 语言字段，并明确其只影响 core-owned surfaces、不代表 future UI 的语言偏好。
- `quality-and-test-automation`: 需要新增 catalog 一致性、help/version 渲染和“禁止硬编码 operator-facing 文案”的自动化校验。

## 影响

- 受影响代码包括 `source/rust/Cargo.toml` 与 member crate 的版本声明方式、`source/rust/bridgingio-mcp/src/bin/bridgingio-core.rs` 的 CLI 入口、`source/rust/bridgingio-operator-console` 的 TUI 文案与帮助渲染、`source/rust/bridgingio-engine` 的配置模型/序列化，以及新增的 core i18n 资源与测试。
- 需要补充与 operator surface 相关的文档、接口矩阵和回归测试，确保后续新增文案与版本输出不再绕过统一入口。

## 实现备注

- 版本展示和版本校验仍遵循 `YYMM.DD.BuildNumber` 语义，但必须满足
  Cargo semver 解析规则：多位数字段不能带前导零。
- 因此 core 版本示例采用 `2604.2.1`（等价语义于 `2604.02.1`），以确保
  `Cargo.toml`、`--version` 与测试合同保持一致且可构建。
