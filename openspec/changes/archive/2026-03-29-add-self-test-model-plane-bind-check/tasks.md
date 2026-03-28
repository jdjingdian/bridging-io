## 1. Self-test 绑定探针

- [x] 1.1 在 `bridgingio-core --self-test` 流程中加入默认 model-plane 地址 `127.0.0.1:19718` 的显式绑定检查
- [x] 1.2 让该检查复用正式 model-plane 绑定路径或等价校验逻辑，并在成功后立即释放监听句柄

## 2. 失败诊断输出

- [x] 2.1 当默认地址绑定失败时，输出包含 host、port 和底层错误文本的 self-test 失败信息
- [x] 2.2 将默认地址绑定失败纳入 `--self-test` 的非零退出码路径，并保持现有通过/失败摘要风格一致

## 3. 验证与文档

- [x] 3.1 为 self-test 补充自动化测试，覆盖默认 model-plane 地址可绑定与绑定失败时的输出行为
- [x] 3.2 更新 contract/self-test 相关文档，明确 `127.0.0.1:19718` 绑定诊断已属于 self-test 覆盖范围
