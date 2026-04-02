## 新增需求

### 需求:core contract 与 self-test 必须维护平台/架构/宿主模式矩阵
BridgingIO 的 core contract automation 与 `--self-test` 文档必须维护正式 matrix，至少覆盖平台、架构、宿主模式、build profile、case id、命令与预期结果。该矩阵必须明确区分当前支持的平台集合与长期规划平台集合。

#### 场景:记录当前支持与长期规划
- **当** 团队更新 core contract 或 self-test matrix
- **那么** 文档必须明确将 macOS、Linux x86_64、Linux aarch64 和 Windows 标为当前目标，将 OpenHarmony 标为长期规划占位，而不能混写为同一支持等级

### 需求:`--self-test` 必须作为 debug-only 运行时测试框架管理
`bridgingio-core --self-test` 必须在质量矩阵中被明确建模为 debug-only 的运行时测试框架。debug 构建必须支持该入口；release 构建必须拒绝该入口，而不是继续把它当作生产模式的常规运行面。

#### 场景:debug 构建执行 self-test
- **当** 团队使用 debug 构建执行 `bridgingio-core --self-test`
- **那么** 系统必须运行完整的运行时测试框架并按正式 case matrix 返回结果

#### 场景:release 构建执行 self-test
- **当** 操作员或自动化在 release 构建上执行 `bridgingio-core --self-test`
- **那么** 系统必须明确拒绝该入口并返回清晰诊断，而不是继续执行完整运行时测试

### 需求:非 Linux 宿主验证必须补齐同架构 Linux contract run
当团队在非 Linux 宿主上编译或验证 BridgingIO core 时，必须通过 `cross` 或等价机制补齐同架构 Linux contract run，而不是只以本机宿主测试通过作为验收标准。

#### 场景:Apple Silicon macOS 验证 core
- **当** 团队在 Apple Silicon macOS 上编译并验证 BridgingIO core
- **那么** 自动化或手工验收必须补齐 Linux aarch64 contract run，而不是只运行 macOS 本机测试

#### 场景:x86_64 非 Linux 宿主验证 core
- **当** 团队在 x86_64 的 macOS 或 Windows 宿主上编译并验证 BridgingIO core
- **那么** 自动化或手工验收必须补齐 Linux x86_64 contract run，而不是只以本机宿主结果作为最终结论
