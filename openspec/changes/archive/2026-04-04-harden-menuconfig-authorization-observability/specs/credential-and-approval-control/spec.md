## 新增需求

### 需求:本地受信任授权动作必须使用统一分类与 flow 语义
对于 `menuconfig`、standalone CLI 与其他受信任本地管理面触发的高风险授权动作，系统必须使用稳定的操作分类与 flow 语义，而不是让不同 surface 各自发明状态文本或 audit 字段。至少 `vault.unlock`、`vault.delete`、`ssh_key.import`、`ssh_key.delete`、`auth.token.create` 与 `auth.token.delete` 必须映射到统一的 `operation` 分类，并产生可关联的 display-safe 审计事件。

#### 场景:menuconfig 触发高风险授权动作
- **当** 操作员在 `menuconfig` 中显式触发一个受信任本地授权动作
- **那么** 系统必须为该动作生成稳定 `flow_id`
- **并且** 必须把该动作归类到正式 `operation`，而不是只输出不稳定的自由文本状态提示

#### 场景:standalone CLI 触发等价管理动作
- **当** 操作员通过 standalone CLI 触发 `vault unlock`、`vault delete`、`token delete` 或等价高风险动作
- **那么** 系统必须沿用与 `menuconfig` 相同的 `operation` 分类和 display-safe 审计字段
- **并且** 不得为 CLI 与 TUI 维护两套彼此不兼容的授权事件语义

### 需求:verified `os-native` 解锁必须对同进程并发请求执行 singleflight
当同一进程内存在多个几乎同时到达的 verified `os-native` 解锁请求时，系统必须把它们合并到同一次 in-flight 验证流程，而不是为每个调用链重复访问平台 keyring 或重复触发系统认证。只有真正执行平台验证的请求可以被标记为 `leader`；加入中的请求必须被标记为 `joined` 并等待同一个结果。

#### 场景:两个调用链同时请求 verified os-native 解锁
- **当** 同一进程内两个调用链在 verified KEK 尚未缓存时几乎同时请求 verified `os-native` 解锁
- **那么** 系统必须只执行一次平台验证 / keyring 访问
- **并且** 后续调用链必须加入同一次 in-flight flow，而不得触发第二次系统认证窗口

#### 场景:共享验证流程中的等待者取消
- **当** 一个等待中的调用者在共享 verified `os-native` 验证尚未完成时主动取消等待，而其他调用者仍在等待
- **那么** 系统必须只将该调用者标记为 `cancelled`
- **并且** 不得因此重新启动新的平台验证流程，也不得把同一次显式验证拆成两次系统认证

#### 场景:共享验证失败不会形成成功缓存
- **当** 作为 `leader` 的 verified `os-native` 验证失败、超时或被平台拒绝
- **那么** 所有 `joined` 调用者必须接收同一次失败结果
- **并且** 系统不得把该结果写入成功缓存后继续将后续请求视为已验证

#### 场景:跨线程 surface 汇总必须保留 dedupe 语义
- **当** verified `os-native` 验证在 worker 线程完成，而授权事件在另一个线程汇总并落盘
- **那么** 授权事件中的 `dedupe_state` 必须保留 `leader` / `joined` / `cancelled` / `failed` 的真实结果语义
- **并且** 不得因线程边界丢失验证结果而退化为 `not-applicable`

## 修改需求

## 移除需求
