## 新增需求

### 需求:启动解锁载体必须收敛到最小安全集合
BridgingIO 的 vault 启动解锁路径必须收敛到最小安全集合，以降低额外泄露面。对于正式 startup/unlock contract，系统只允许使用隐藏输入的本地终端 prompt、父进程一次性本地 carrier，或受信任本地验证触发的正式 unlock 路径。系统禁止把明文 argv、普通环境变量或普通文件作为正式启动解锁载体。

#### 场景:standalone 前台需要解锁
- **当** standalone 前台模式在启动阶段或首次 secret access 阶段需要输入 unlock material
- **那么** 系统必须通过隐藏输入的本地终端 prompt 获取该材料，而不得要求操作员通过 argv、普通环境变量或普通文件提供明文口令

#### 场景:standalone 后台或服务需要解锁
- **当** standalone 后台/服务模式在启动阶段需要 unlock material
- **那么** 系统必须只接受父进程提供的一次性本地 carrier，而不得要求后台进程在启动后再等待普通交互式输入

### 需求:启动解锁失败必须保持 fail-closed
若 policy 要求在启动阶段完成解锁，但系统未能通过允许的正式 carrier 成功获取 unlock material，则 core 必须保持 `locked` 或 `unavailable`，并拒绝 secret-backed 功能。系统禁止在这种情况下隐式降级为“先运行，稍后再看”。

#### 场景:carrier 不可用或材料无效
- **当** core 在启动阶段检测到 unlock material 缺失、carrier 不可用或输入验证失败
- **那么** 系统必须保持 fail-closed 状态，并返回明确恢复提示，而不是把 vault 伪装为已准备就绪
