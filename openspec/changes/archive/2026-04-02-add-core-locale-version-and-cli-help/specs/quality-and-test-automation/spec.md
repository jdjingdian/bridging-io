## 新增需求

### 需求:core operator surface 的本地化与版本合同必须具备自动化校验
对于 `bridgingio-core` 的 locale catalog、help/version 输出与 operator-facing 显示路径，系统必须提供自动化校验，确保中英文键集合一致、版本真相唯一且展示层不回流硬编码文本。项目禁止只靠人工 review 保证这些合同。

#### 场景:验证中英文 catalog 键集合一致
- **当** 团队新增或修改 core operator surface 文案键
- **那么** 自动化校验必须验证 `zh-CN` 与 `en-US` 的键集合保持一致，而不是允许某个语言缺 key 后在运行时才暴露问题

#### 场景:验证 help 和 version 合同
- **当** 团队执行与 `bridgingio-core` CLI 相关的自动化测试
- **那么** 测试必须验证 `--help` / `help` / `--version` 的关键输出结构、locale 切换行为与版本值来源，而不是只验证命令执行没有崩溃

### 需求:operator-facing 硬编码显示文本必须被自动阻断
对于 `bridgingio-mcp` CLI 与 `bridgingio-operator-console` 的 operator-facing 显示路径，项目必须提供自动化检查以阻断新增硬编码展示文本。允许保留稳定 machine-readable key、字段路径和错误码，但禁止把用户可见文案直接写回渲染路径。

#### 场景:新增 menuconfig 状态提示
- **当** 团队在 `menuconfig` 中新增状态栏提示、帮助说明或弹窗文案
- **那么** 自动化检查必须能够发现这些显示文本是否绕过 catalog 直接硬编码，而不是等到人工体验时才发现语言漂移

#### 场景:新增 CLI 子命令说明
- **当** 团队为 `bridgingio-core` 新增命令说明、参数帮助或 about 文案
- **那么** 自动化检查必须验证这些 operator-facing 文案走统一 catalog / 命令元数据入口，而不是直接拼接字面量字符串

## 修改需求

无。

## 移除需求

无。
