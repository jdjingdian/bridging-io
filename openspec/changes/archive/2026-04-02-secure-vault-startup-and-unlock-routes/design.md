## 上下文

当前仓库虽然已经有若干 secret 输入路线，但它们并不等于“已经具备正式安全启动合同”。尤其是 standalone 将成为短期主模式时，启动时如何安全提供 vault unlock material 会直接决定泄露面大小。

本次还有一个新增约束：standalone 的前台和后台模式都属于同一 standalone 家族，但后台模式在启动后不方便再交互式输入 credentials，因此必须忽略配置中的常规 `trigger_policy`，强制按 `on-core-start` 处理。

这意味着我们需要一套同时满足“尽可能通用”和“载体尽量少”的设计，而不是继续增加更多 secret 输入路径。

## 目标 / 非目标

**目标：**

- 定义一条共享的 secure startup / unlock 协议
- 把 standalone 前台与后台纳入同一模式家族，但明确后台的安全 override 规则
- 将启动解锁载体收敛到最小集合，减少旁路泄露面
- 保持 fail-closed：拿不到允许的 unlock material 就不进入 ready
- 让平台差异停留在 carrier adapter，而不是停留在安全模型本身

**非目标：**

- 本次不为所有 secret 管理动作增加更多输入路线
- 本次不把普通文件、argv 或普通环境变量纳入正式启动解锁路径
- 本次不要求一次性完成所有平台原生验证集成

## 决策

### 决策 1：安全启动围绕“共享 unlock 协议 + 最少 carrier”设计

共享 unlock 协议保持一致：

1. 读取 policy
2. 判断当前 host mode
3. 选择允许的 carrier
4. 在进程内完成 unlock
5. 立即丢弃输入材料
6. 后续 secret 使用只走 broker

正式 carrier 只保留三类：

- standalone 前台：隐藏输入的本地终端 prompt
- standalone 后台/服务：父进程一次性本地 carrier（pipe/handle/等价本地通道）
- trusted host：本地验证后触发正式 unlock 动作

### 决策 2：standalone detached 强制覆盖 `trigger_policy = on-core-start`

对于 standalone detached / daemon 子模式：

- 不再遵从配置中的常规 `trigger_policy`
- 启动时一律按 `on-core-start` 处理

原因：

- 后台模式启动后不方便再进行安全的交互式输入
- 若允许其保留 `on-first-secret-access` 或 `manual-only`，就会把 unlock 时机拖到更难控制的运行期
- 统一成启动即决策，最能降低旁路载体扩散风险

### 决策 3：foreground 继续遵从配置，但只能走隐藏输入 prompt

对于 standalone 前台 `run`：

- 继续尊重配置中的 `trigger_policy`
- 但如果需要本地输入 unlock material，正式路径只允许隐藏输入 prompt

这样既保留了前台模式的灵活性，也避免“为了通用性”把 startup carrier 扩散到更多危险路径。

### 决策 4：平台差异只落在 carrier adapter

逻辑上三类 carrier 保持统一，平台只特化具体实现：

- foreground hidden prompt：按平台做 no-echo 终端输入
- parent-provided one-shot carrier：按平台做 pipe/handle 继承
- trusted local verification：按平台接本地宿主能力

这样可以保证：

- 安全模型在各平台上仍是一套
- 平台差异不扩散到 vault policy、broker 或 token 模型

### 决策 5：启动解锁失败必须停留在 locked/unavailable，而不是延迟试错

只要 standalone 启动阶段要求解锁，但 unlock material 未通过允许 carrier 成功送达，系统就必须：

- 保持 `locked` 或 `unavailable`
- 返回明确恢复提示
- 禁止把 secret-backed 路径伪装成可后续自动补救

## 风险 / 权衡

- [风险] 收窄 carrier 会牺牲部分脚本灵活性
  → 缓解措施：把“可脚本化”收敛到父进程一次性 carrier，而不是再次开放明文 argv/file/env
- [风险] detached override 会让部分旧配置的行为改变
  → 缓解措施：明确把它定义为安全优先的 BREAKING 规则，并通过文档和启动诊断提示
- [风险] 跨平台 no-echo prompt 与 pipe carrier 的实现复杂度较高
  → 缓解措施：先稳定共享协议，再把实现落到平台 adapter

## 迁移计划

1. 冻结共享 unlock 协议与允许 carrier 集合。
2. 为 detached mode 引入 `on-core-start` override。
3. 将 foreground prompt、parent carrier 与 trusted host 接到同一 unlock handler。
4. 更新 operator 文档、lifecycle 状态和 self-test/contract automation。

## 未决问题

- foreground hidden prompt 在各平台上是否统一通过同一抽象实现，还是允许平台各自实现后再适配
- 后续是否需要为 system service 集成额外的宿主级 credential bridge，但不改变共享协议
