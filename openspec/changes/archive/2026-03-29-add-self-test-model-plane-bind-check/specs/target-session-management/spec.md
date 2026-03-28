## 新增需求

<!-- 无 -->

## 修改需求

### 需求:core 启动入口必须支持无配置的 `--self-test` 自检模式
系统必须提供 `bridgingio-core --self-test` 启动入口，用于在不提供 `--config`、`--runtime-root` 的前提下执行内建自检。自检至少必须覆盖 one-shot 执行、interactive shell 生命周期、runtime 执行链路，以及默认 MCP model-plane 监听地址 `127.0.0.1:19718` 的绑定检查，并通过进程退出码反映结果。

#### 场景:操作员运行 `--self-test`
- **当** 操作员执行 `bridgingio-core --self-test`
- **那么** 系统必须自动执行内建自检并输出清晰的通过/失败摘要；全部通过时返回退出码 `0`，任一检查失败时返回非零退出码

#### 场景:默认 MCP 监听地址可绑定
- **当** 操作员执行 `bridgingio-core --self-test`，且默认 model-plane 地址 `127.0.0.1:19718` 可成功绑定
- **那么** 自检必须将该项视为通过，并继续后续检查，而不是跳过该诊断

#### 场景:默认 MCP 监听地址绑定失败
- **当** 操作员执行 `bridgingio-core --self-test`，且默认 model-plane 地址 `127.0.0.1:19718` 无法绑定
- **那么** 自检必须打印包含目标 host/port 与底层绑定错误文本的失败信息，并以非零退出码结束

#### 场景:`--self-test` 与配置参数混用
- **当** 操作员同时传入 `--self-test` 与 `--config`、`--runtime-root` 或 `--control-plane-socket-override`
- **那么** 系统必须拒绝该参数组合并返回明确参数错误，而不是在含糊模式下继续启动

## 移除需求

<!-- 无 -->
