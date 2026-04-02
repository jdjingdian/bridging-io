## 1. Matrix 文档骨架

- [x] 1.1 创建 self-test / contract case matrix 文档，覆盖平台、架构、宿主模式、build profile 与 case id
- [x] 1.2 创建本地 operator interface matrix 文档，覆盖命令、调用方、输入输出、状态、错误码与 apply strategy

## 2. 规则与自动化

- [x] 2.1 将 `--self-test` 的 debug-only 规则和 release 拒绝语义写入测试矩阵并补充验证路径
- [x] 2.2 为非 Linux 宿主补充同架构 Linux `cross` contract 验证要求，并更新脚本或运行手册

## 3. 文档收口

- [x] 3.1 更新 `docs/testing`、`bridgingio-app-api` 边界文档和开发文档，引用新的 matrix 真相源
- [x] 3.2 更新验收/归档清单，要求后续核心变更同步维护接口矩阵与测试矩阵
