## 为什么

BridgingIO 当前的错误与状态语义分散在 `bridgingio-app-api`、`bridgingio-mcp`、`bridgingio-engine`、`bridgingio-secrets` 和 `bridgingio-platform` 中，各层既有粗粒度错误码，也有模块私有错误枚举，还存在大量字符串化诊断。随着短期内产品重心切到 core-first 和 standalone-first，这种碎片化表达会直接扩大未来 menuconfig、CLI、MCP 与后续 UI 的问题边界，导致“能力未实现”“平台受控降级”“真正运行时失败”被混为一类。

现在需要先建立一个 core 级共享错误与状态契约，把通用状态、模块专属错误和稳定回传格式收束到同一套真相源中，并明确 `method_not_implemented` 这类必须长期存在的受控语义。

## 变更内容

- 定义一个 core 共享的错误与状态契约模块，用于统一表达通用状态、错误分类、恢复提示、是否可重试、模块来源和模块专属子码。
- 引入稳定的通用状态集合，至少覆盖 `ready`、`degraded`、`unsupported`、`locked`、`not_ready`、`method_not_implemented` 等核心语义，避免不同调用面各自发明近义词。
- 要求本地 control-plane、standalone CLI、MCP typed tools 与 runtime 生命周期诊断统一使用结构化错误封装，而不是继续依赖 ad hoc 字符串或只保留 4-5 个过粗的公共错误码。
- **BREAKING**: 对外暴露给受信任本地调用面和后续前端的错误/状态投影将切换到新的结构化契约；现有只含 `message` 或过粗 `ApiErrorCode` 的路径将需要迁移。

## 功能 (Capabilities)

### 新增功能
- `core-error-and-status-contract`: 定义 core 共享的通用状态、结构化错误封装、`method_not_implemented` 受控语义，以及模块专属子码扩展规则。

### 修改功能
- `capability-aware-mcp`: MCP 返回的 runtime/tool/readiness 错误与诊断必须映射到共享错误与状态契约，而不是继续依赖 ad hoc 字符串。
- `target-session-management`: standalone 启动、control-plane 生命周期、运行模式切换和配置装载失败必须使用共享错误与状态契约。
- `credential-and-approval-control`: vault、token、local verification 和 approval 路径必须在保留模块专属错误语义的同时，对外回传共享错误封装。

## 影响

- 受影响代码主要包括 `source/rust/bridgingio-app-api`、`source/rust/bridgingio-mcp`、`source/rust/bridgingio-engine`、`source/rust/bridgingio-secrets`、`source/rust/bridgingio-platform`。
- 需要同步更新本地控制面、MCP 和后续前端的错误处理边界，以及相关文档和测试断言。
