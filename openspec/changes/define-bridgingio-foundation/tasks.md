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
- [x] 2.8 定义并实现 core-owned settings/config 边界，区分非敏感配置、`CredentialRef` 引用、运行期元数据以及 control plane / model plane 的 endpoint 配置，并产出 versioned standalone TOML schema、字段说明与最小/完整样例
- [x] 2.9 为保险库后端补充可替换抽象，兼容系统级安全存储与未来内置 vault 路线
- [x] 2.10 将 artifact 的重读取、重过滤和重分析提升为 target-agnostic 的一级核心能力，明确其不绑定 SSH/ADB/terminal provider，并补充处理归属模型（`source` / `bridgingio` / `auto`）
- [x] 2.11 将 artifact store 抽象为可替换 backend，至少支持 `memory` 与 `filesystem`，并让 raw / derived artifact 在 `filesystem` backend 下跨 core 重启继续可读
- [x] 2.12 为 artifact 引入稳定 hash-like object id 与独立 `content_digest`，替代进程内自增序号，并保留足够的 provenance 元数据以支撑基于 hash 的反查
- [x] 2.13 为 artifact 持久化补充容量治理，至少覆盖 artifact root、最大缓存限制、淘汰策略与活跃 artifact 保护语义

## 3. 连接器与 Provider MVP

- [x] 3.1 实现连接器执行路径解析，支持用户覆盖路径、系统 PATH 和内置后备三层来源
- [x] 3.2 实现 SSH 连接器 MVP，覆盖目标连接、会话建立、命令执行和环境探测
- [x] 3.3 实现 ADB 连接器 MVP，覆盖设备发现、会话建立、命令执行和环境探测
- [x] 3.4 实现 terminal provider，支持普通执行、流式输出、artifact 生成和续读
- [x] 3.5 实现 git provider，支持仓库状态、日志和 diff 的结构化查询
- [x] 3.6 让 SSH/ADB 连接器支持同一逻辑会话内的多通道连接与状态跟踪
- [x] 3.7 为 SSH/ADB 等连接器补充内置后备二进制的分发、定位与诊断策略，并明确其由 core 统一管理
- [x] 3.8 扩展 terminal provider 与 SSH/ADB 连接器，支持 `one-shot exec` 与 `interactive shell` 双模式，并明确 channel 级 `cwd`、环境变量、prompt、stdin、signal 与关闭语义

## 4. MCP 暴露面

- [x] 4.1 实现能力发现接口，允许客户端查询目标、会话和 provider 的结构化能力摘要
- [x] 4.2 实现 typed tools/resources，至少覆盖目标管理、会话管理、终端执行、artifact 读取/过滤和 Git 查询
- [x] 4.3 实现受控 raw command 兜底接口，并接入策略检查、审批流程和审计记录
- [x] 4.4 统一 MCP 返回模型，覆盖成功结果、artifact 引用、审批等待、权限拒绝和执行失败
- [x] 4.5 为 app API / MCP 输入补充 `agent_id`、`run_id`、`client_session_id` 和 `reuse_policy`
- [x] 4.6 扩展 app API 的 settings/profile 接口，覆盖目标创建/编辑、凭据引用选择、调用别名维护、工具来源诊断读取，以及本地 control-plane 所需的 session / approval / diagnostics 访问
- [x] 4.7 定义 standalone core 的配置装载入口，使前台进程、daemon 和平台 UI 都能复用同一套设置语义，并统一承载本地 IPC 与 MCP HTTP 的 endpoint 配置、schema version 校验与样例配置装载
- [x] 4.8 为受信任 UI/control plane 定义并实现本地 app API IPC 传输，复用现有 request / response / event 模型而不复用面向 AI 的 MCP HTTP 能力面
- [x] 4.9 为 AI/model plane 定义并实现 MCP HTTP 入口，默认监听 `127.0.0.1:19718`，支持 host/port 配置，并确保多个 MCP 客户端共享同一份 core 状态
- [x] 4.10 为 MCP HTTP 补充非 loopback 暴露的安全约束，至少覆盖显式启用、认证联动、告警与诊断回显
- [x] 4.11 扩展 MCP capability schema 与 typed tools，明确暴露终端型 target 的 `one-shot exec` / `interactive shell` 模式，并提供会话句柄的打开、写入、读取、中断和关闭接口
- [x] 4.12 将 artifact reanalysis 作为 MCP 一级能力显式暴露，使模型能够独立发现 `artifacts.read` / `artifacts.refine`，而不是仅从终端能力描述中被动得知
- [x] 4.13 为 artifact 重分析相关 typed tools 增加显式处理归属参数，至少覆盖 `source`、`bridgingio`、`auto` 三种语义
- [x] 4.14 为 MCP 能力暴露定义“英文简述 + 英文详细说明”双层描述契约，使 `tools/list` 提供 short description，并提供按 capability/tool 标识读取 detailed description 的入口
- [x] 4.15 为所有新增 MCP capability/tool 建立统一的能力描述登记机制，要求 short description 与 detailed description 从同一份 core-owned source of truth 派生
- [x] 4.16 为 MCP 补充 resources 兼容入口（`resources/list`、`resources/templates/list`、`resources/read`），确保先走 resources 探测的客户端也能稳定接入
- [x] 4.17 为 capability/tool 详情查询实现标识归一化与别名容错，支持下划线与点号等常见命名风格差异
- [x] 4.18 为 model-plane 增加可开关 MCP trace 诊断能力（默认关闭），用于定位客户端调用链路与错误回包问题
- [x] 4.19 固化 MCP JSON-RPC 入口与错误回包一致性：`/mcp` 仅接收 `POST` JSON-RPC，请求失败时错误响应必须保留原请求 `id`
- [x] 4.20 扩展 core-owned settings 与本地 control-plane app API，暴露 artifact cache backend、持久化根目录、最大缓存限制、淘汰策略与当前使用情况
- [x] 4.21 扩展 artifact 查询接口，使 UI 和 MCP 能基于稳定 artifact hash 反查 artifact 元数据、来源命令、所属会话/通道与父子派生关系

## 5. macOS 控制台设计与实现

- [x] 5.1 使用 `ui-ux-pro-max` 为 BridgingIO 生成 macOS 控制台设计系统建议，并补充 SwiftUI 栈约束、信息架构与可访问性要点
- [x] 5.2 与项目负责人确认设计方向，定稿目标列表、会话详情、命令时间线、artifact 详情和审批流程的界面结构
- [x] 5.3 将确认后的设计系统持久化到 `design/macos-console/design-system/MASTER.md`，并按页面需要创建 `design/macos-console/design-system/pages/` 下的可编辑覆盖文件
- [ ] 5.4 基于已确认的设计系统构建目标列表、会话详情和设置入口，展示连接状态、环境指纹、能力摘要与 profile 基本信息
- [ ] 5.5 构建目标 profile 创建/编辑流程，覆盖 SSH/ADB 连接参数、调用别名、用户备注、凭据引用和默认策略
- [ ] 5.6 构建连接器/Provider 诊断与工具来源界面，覆盖用户覆盖路径、系统 PATH 命中和内置后备来源回显
- [ ] 5.7 构建可折叠命令时间线，展示命令摘要、stdout/stderr 入口、exit status 和 artifact 关联
- [ ] 5.8 构建 artifact 详情与二次过滤交互，使用户能够重新查看或扩大过滤范围
- [ ] 5.9 构建审批界面和状态反馈，覆盖等待审批、批准、拒绝和失败结果
- [ ] 5.10 为 macOS 控制台补充 UI Test，覆盖目标选择、配置编辑、会话查看、命令时间线、artifact 详情和审批流程
- [ ] 5.11 为交互式 shell channel 设计并实现 transcript 视图，支持查看 prompt、发送输入、切换 channel，以及执行关闭/中断操作
- [ ] 5.12 构建 artifact cache 设置界面，覆盖 backend 选择、持久化根目录、最大缓存限制、淘汰策略、已使用空间与清理入口
- [ ] 5.13 在命令时间线与 artifact 详情中显示 artifact hash，并支持基于 hash 反查来源命令、所属会话/通道与派生关系

## 6. 验证与收敛

- [x] 6.1 为核心对象、artifact 管线、审批策略和能力发现补充自动化测试
- [x] 6.2 为 SSH、ADB 和 Git 的关键流程补充集成验证，确认会话、artifact 与审批协同工作
- [x] 6.3 定义跨平台 UI 的关键用户流测试清单，作为 Linux、Windows、鸿蒙 PC 等后续平台的 UI Test 契约
- [x] 6.4 定义 archive 完成后的交付输出模板，确保 commit message 与 PR 描述引用 `.gitmessage` 和本次变更上下文
- [x] 6.5 补充开发文档，说明项目结构、MVP 范围、配置方式、测试要求和 archive 后的交付要求
- [x] 6.6 为多 agent 隔离、逻辑会话恢复和多通道并发补充集成测试
- [x] 6.7 为本地 control-plane IPC 与 MCP HTTP model-plane 补充集成验证，确认它们共享同一份 core 状态真相且不会串用访问语义
- [x] 6.8 基于项目提供的 standalone TOML 样例配置补充 headless smoke / integration 验证，在 UI 开发前确认 core 能独立完成目标装载、监听初始化、工具解析与基础连接探测
- [x] 6.9 为单次执行隔离、交互式 shell 状态延续、多 shell channel 并发与断线恢复补充自动化测试
- [x] 6.10 为跨 target 的 artifact 重分析补充自动化验证，确认其不依赖 SSH/ADB 专有语义即可工作
- [x] 6.11 为 `tools/list` 的英文简述、能力详情读取入口以及处理归属参数补充自动化验证
- [x] 6.12 为新增 capability/tool 的描述登记约束补充自动化验证，确认对外暴露的 MCP 能力不会遗漏英文简述或详述
- [x] 6.13 为 resources 兼容入口补充自动化验证，覆盖 `resources/list`、`resources/templates/list` 与 `resources/read`
- [x] 6.14 为 capability/tool 详情查询的别名归一化和 canonical id 回包补充自动化验证
- [x] 6.15 为 MCP JSON-RPC 错误路径补充自动化验证，确认错误回包不会丢失请求 `id`，并覆盖 `/mcp` 非 POST 访问语义
- [x] 6.16 为 `memory` / `filesystem` artifact backend 补充自动化验证，确认持久化 artifact 在 core 重启后仍可 `read` / `refine`
- [x] 6.17 为 artifact hash identity、`content_digest`、缓存上限与淘汰策略补充自动化验证，并覆盖基于 hash 的反查流程
