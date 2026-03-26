## 新增需求

无。

## 修改需求

### 需求:MCP 必须通过共享的 HTTP 模型平面对外暴露
系统必须允许外部 AI/MCP 客户端通过同一个 MCP HTTP 入口访问共享的 core 状态，而不是要求每个客户端都通过独立 stdio 进程维护各自的状态真相。该 HTTP 入口在默认配置下必须监听 `127.0.0.1:19718`，并允许操作员显式修改 host 与 port。对于 bundled 的 UI-managed 模式，系统必须在受信任本地 UI 完成 attach 之前将 `/mcp` 视为未就绪；在 attach 完成之前，服务端必须返回明确的 not-ready 或等价 readiness 语义，或保持等价的不可用状态，而不得把该模型平面伪装成已可执行的正常入口。该 attach gating 不适用于显式 standalone 模式。若 bundled 操作员通过 UI 修改 host 或 port，core 必须先完成校验与持久化，再通过受控重启让新的监听地址生效。

#### 场景:两个 MCP 客户端访问同一份 core 状态
- **当** 两个独立的 MCP 客户端连接到同一个 BridgingIO Core 的 MCP HTTP 入口
- **那么** 它们必须访问到同一份目标/profile 与 session 管理真相，并通过访问作用域与复用策略完成逻辑隔离，而不是因为每个客户端各自拉起独立进程而形成分叉状态

#### 场景:bundled 模式下 UI attach 前访问 `/mcp`
- **当** 外部 MCP 客户端在 bundled 的 UI-managed core 尚未完成本地 UI attach 时访问 `/mcp`
- **那么** 系统必须返回明确的未就绪语义或保持等价的不可用状态，而不是把该请求当成正常可执行的 MCP 调用受理

#### 场景:bundled 模式下修改监听地址后生效新端口
- **当** bundled 操作员在 UI 中保存新的 model-plane host 或 port，且 core 判定该修改需要重启
- **那么** 系统必须先持久化该配置，再由 UI 托管重启 core，并让新的 MCP HTTP 入口按新地址生效，而不是继续保留旧监听地址却对外宣称已更新

#### 场景:保存无效监听配置
- **当** bundled 操作员在 UI 中提交无效的 host、port 或与安全约束冲突的监听配置
- **那么** core 必须拒绝该写入并返回明确错误，当前已运行的 MCP 监听不得被半生效修改

#### 场景:standalone 模式下 MCP 可直接提供服务
- **当** 操作员以显式 standalone 模式启动 core，且未要求 bundled UI attach gating
- **那么** 系统必须允许外部 MCP 客户端在该实例完成自身启动后直接访问共享的 model-plane HTTP 入口

## 移除需求

无。
