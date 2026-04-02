## 1. 共享合同模型

- [x] 1.1 定义 core 共享错误与状态合同模块，固定通用状态、通用错误码与模块子码扩展规则
- [x] 1.2 为 config、vault、platform、runtime lifecycle 等主要模块建立到共享合同的映射表

## 2. 公共调用面迁移

- [x] 2.1 更新 `bridgingio-app-api` 与本地 control-plane，使其返回共享错误封装而不是仅有粗粒度错误码
- [x] 2.2 更新 `bridgingio-mcp` 与 standalone CLI 的错误/状态投影，补齐 `method_not_implemented`、`degraded`、`unsupported`、`not_ready` 等正式语义

## 3. 验证与文档

- [x] 3.1 为共享错误合同补充单元测试与集成测试，覆盖模块子码保留和 display-safe 错误投影
- [x] 3.2 更新错误处理文档与接口说明，明确公共错误字段、恢复提示和兼容迁移规则
