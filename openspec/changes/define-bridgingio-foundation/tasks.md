## 1. 仓库与模块基线

- [x] 1.1 创建项目目录骨架，拆分 `source/rust`、`source/ui/swiftui-macos`、`design` 与 `openspec` 的长期结构
- [x] 1.2 初始化 Rust workspace，定义 domain、engine、artifacts、policy、secrets、connectors、providers、mcp、app-api 等核心 crate
- [x] 1.3 定义 SwiftUI macOS 与 Rust 核心之间的 app API 边界，明确请求、响应、事件和错误模型
- [x] 1.4 建立 Rust 单元测试、集成测试和平台 UI Test 的基础目录与运行约定
- [x] 1.5 明确 archive 后的交付规则，约定从 `.gitmessage` 读取 commit message 格式并定义 PR 描述最小结构

## 2. 核心模型与本地存储

- [x] 2.1 实现 `Target`、`Session`、`Capability`、`Profile`、`CredentialRef`、`Artifact`、`ApprovalRequest`、`EnvironmentFingerprint` 等核心对象
- [x] 2.2 建立本地元数据存储，覆盖 profile、session、artifact index、environment fingerprint 和审计事件
- [x] 2.3 实现 artifact 存储策略，支持原始 artifact、派生 artifact、父子关系和分块内容索引
- [x] 2.4 实现凭据引用与保险库抽象，确保配置层只保存引用而不保存明文
- [x] 2.5 实现审批策略与高风险操作分类，覆盖写操作、删除操作、敏感读取和特权执行
- [x] 2.6 为核心模型补充 `AccessScope`、`LogicalSession`、`TransportSession` 和 `Channel`，明确默认隔离边界
- [x] 2.7 设计逻辑会话复用键与复用策略，覆盖 `always_new`、`reuse_if_alive`、`resume_or_create`
- [ ] 2.8 定义并实现 core-owned settings/config 边界，区分非敏感配置、`CredentialRef` 引用和运行期元数据
- [ ] 2.9 为保险库后端补充可替换抽象，兼容系统级安全存储与未来内置 vault 路线

## 3. 连接器与 Provider MVP

- [x] 3.1 实现连接器执行路径解析，支持用户覆盖路径、系统 PATH 和内置后备三层来源
- [x] 3.2 实现 SSH 连接器 MVP，覆盖目标连接、会话建立、命令执行和环境探测
- [x] 3.3 实现 ADB 连接器 MVP，覆盖设备发现、会话建立、命令执行和环境探测
- [x] 3.4 实现 terminal provider，支持普通执行、流式输出、artifact 生成和续读
- [x] 3.5 实现 git provider，支持仓库状态、日志和 diff 的结构化查询
- [x] 3.6 让 SSH/ADB 连接器支持同一逻辑会话内的多通道连接与状态跟踪
- [ ] 3.7 为 SSH/ADB 等连接器补充内置后备二进制的分发、定位与诊断策略，并明确其由 core 统一管理

## 4. MCP 暴露面

- [x] 4.1 实现能力发现接口，允许客户端查询目标、会话和 provider 的结构化能力摘要
- [x] 4.2 实现 typed tools/resources，至少覆盖目标管理、会话管理、终端执行、artifact 读取/过滤和 Git 查询
- [x] 4.3 实现受控 raw command 兜底接口，并接入策略检查、审批流程和审计记录
- [x] 4.4 统一 MCP 返回模型，覆盖成功结果、artifact 引用、审批等待、权限拒绝和执行失败
- [x] 4.5 为 app API / MCP 输入补充 `agent_id`、`run_id`、`client_session_id` 和 `reuse_policy`
- [ ] 4.6 扩展 app API 的 settings/profile 接口，覆盖目标创建/编辑、凭据引用选择、调用别名维护和工具来源诊断读取
- [ ] 4.7 定义 standalone core 的配置装载入口，使前台进程、daemon 和 macOS UI 都能复用同一套设置语义

## 5. macOS 控制台设计与实现

- [x] 5.1 使用 `ui-ux-pro-max` 为 BridgingIO 生成 macOS 控制台设计系统建议，并补充 SwiftUI 栈约束、信息架构与可访问性要点
- [ ] 5.2 与项目负责人确认设计方向，定稿目标列表、会话详情、命令时间线、artifact 详情和审批流程的界面结构
- [ ] 5.3 将确认后的设计系统持久化到 `design/macos-console/design-system/MASTER.md`，并按页面需要创建 `design/macos-console/design-system/pages/` 下的可编辑覆盖文件
- [ ] 5.4 基于已确认的设计系统构建目标列表、会话详情和设置入口，展示连接状态、环境指纹、能力摘要与 profile 基本信息
- [ ] 5.5 构建目标 profile 创建/编辑流程，覆盖 SSH/ADB 连接参数、调用别名、用户备注、凭据引用和默认策略
- [ ] 5.6 构建连接器/Provider 诊断与工具来源界面，覆盖用户覆盖路径、系统 PATH 命中和内置后备来源回显
- [ ] 5.7 构建可折叠命令时间线，展示命令摘要、stdout/stderr 入口、exit status 和 artifact 关联
- [ ] 5.8 构建 artifact 详情与二次过滤交互，使用户能够重新查看或扩大过滤范围
- [ ] 5.9 构建审批界面和状态反馈，覆盖等待审批、批准、拒绝和失败结果
- [ ] 5.10 为 macOS 控制台补充 UI Test，覆盖目标选择、配置编辑、会话查看、命令时间线、artifact 详情和审批流程

## 6. 验证与收敛

- [x] 6.1 为核心对象、artifact 管线、审批策略和能力发现补充自动化测试
- [x] 6.2 为 SSH、ADB 和 Git 的关键流程补充集成验证，确认会话、artifact 与审批协同工作
- [x] 6.3 定义跨平台 UI 的关键用户流测试清单，作为 Linux、Windows、鸿蒙 PC 等后续平台的 UI Test 契约
- [x] 6.4 定义 archive 完成后的交付输出模板，确保 commit message 与 PR 描述引用 `.gitmessage` 和本次变更上下文
- [x] 6.5 补充开发文档，说明项目结构、MVP 范围、配置方式、测试要求和 archive 后的交付要求
- [x] 6.6 为多 agent 隔离、逻辑会话恢复和多通道并发补充集成测试
