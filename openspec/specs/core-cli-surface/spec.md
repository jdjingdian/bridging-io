# core-cli-surface 规范

## 目的
待定 - 由归档变更 add-core-locale-version-and-cli-help 创建。归档后请更新目的。
## 需求
### 需求:`bridgingio-core` 必须提供可本地化的 core-owned CLI 文案面
`bridgingio-core` 必须为 help、about、命令说明、参数说明和其他 operator-facing CLI 展示文案提供正式的本地化能力。首批支持语言必须至少包含 `zh-CN` 与 `en-US`，并且这些展示文案禁止继续在渲染路径中硬编码。

#### 场景:core locale 为中文时查看 help
- **当** 操作员将 core locale 配置为 `zh-CN`，并执行 `bridgingio-core --help`
- **那么** CLI 必须以中文展示标题、命令说明和参数帮助，而不是继续输出英文硬编码文案

#### 场景:core locale 为英文时查看 help
- **当** 操作员将 core locale 配置为 `en-US`，并执行 `bridgingio-core help` 或 `bridgingio-core --help`
- **那么** CLI 必须以英文展示同一组命令与参数说明，并与中文共用同一套键集合，而不是维护一份漂移的独立帮助文本

### 需求:core 版本输出必须只来源于主 `Cargo.toml` 的版本真相
`bridgingio-core` 对外暴露的版本字符串必须只来源于 workspace 主 `Cargo.toml` 的 `version`，并且该版本必须符合 `YYMM.DD.BuildNumber` 格式。系统禁止在代码、资源文件或脚本中再维护第二份 core 版本常量。

#### 场景:操作员查看 CLI 版本
- **当** 操作员执行 `bridgingio-core --version`
- **那么** CLI 必须返回当前 workspace 主 `Cargo.toml` 中的版本字符串，而不是返回独立硬编码的版本号

#### 场景:系统暴露 core 元数据版本
- **当** 本地 control-plane、MCP 或自检输出需要展示 core version 元数据
- **那么** 这些对外版本值必须与 `bridgingio-core --version` 完全一致，而不是出现 CLI、运行时元数据和构建信息彼此不一致的情况

### 需求:CLI help 必须提供正式的分组留白与强调样式
`bridgingio-core` 的 help 输出必须提供清晰的章节分组、空行留白和标题强调效果，以对齐 `xgit` 风格的可读性。系统在支持样式的终端中必须展示等价于加粗的强调效果；在不支持样式的输出环境中，必须回退为不含乱码转义序列的纯文本布局。

#### 场景:TTY 环境中查看 help
- **当** 操作员在支持 ANSI 样式的终端中执行 `bridgingio-core --help`
- **那么** 输出必须以正式分段展示 `Usage`、`Commands`、`Options` 或其本地化等价标题，并对标题应用可见强调，而不是输出一段无层次的纯文本

#### 场景:非 TTY 环境中查看 help
- **当** 操作员将 `bridgingio-core --help` 的输出重定向到文件或管道
- **那么** 系统必须输出结构清晰但不包含失真的 ANSI 控制序列的纯文本帮助内容

