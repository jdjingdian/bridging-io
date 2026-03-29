## 上下文

BridgingIO 当前已经有一套围绕本地 control-plane、managed core 生命周期和 macOS SwiftUI 控制台建立起来的产品边界，但这条线还没有演化出一个可复用的跨平台桌面宿主形态。现状主要有四个约束：

- 未来桌面产品需要覆盖 Windows / Linux / OpenHarmony，并继续保留本地可信宿主、托盘入口、目录选择、通知和本地用户验证这些 GUI 能力。
- 团队已经决定以 `Tauri Shell + bundled webview + Rust core` 作为跨平台默认方向，而不是继续把管理面建立在外部浏览器和额外本地 HTTP 管理端口之上。
- 用户明确希望新的跨平台管理面不要被当前 macOS 版本的页面划分和视觉语言绑定；新界面需要成为后续 macOS 重构的参考，而不是继续跟随旧版本。
- 新管理面不仅要覆盖 target、timeline 和 settings，还要承接 vault 解锁、token 列表 / revoke、长期 token 签发前本地验证等高信任动作，因此它不能只是一个“弱能力配置页”。

与此同时，用户已经给出了明确的信息架构输入：

1. 首次启动引导界面：选择 runtime 数据根目录。
2. 主界面：独立的 `Targets`、`Timeline`、`Settings` 页面。
3. 设置界面：配置 core 监听端口、日志大小、缓存大小，并进入 vault/token 管理。

为了让这次变更既能指导技术实现，又能为未来平台迁移提供设计基线，本次设计同时吸收了 `ui-ux-pro-max` 生成的设计系统结果，并对其进行了人工收敛。保留的方向是“简约、可信、轻量动效、平台中立”；明确排除的方向包括紫色液态玻璃、赛博终端氛围、重装饰霓虹和强平台拟态。

相关设计系统文件已保存到：

- `openspec/changes/design-cross-platform-tauri-console/design-system/bridgingio-desktop-console/MASTER.md`
- `openspec/changes/design-cross-platform-tauri-console/design-system/bridgingio-desktop-console/pages/onboarding.md`
- `openspec/changes/design-cross-platform-tauri-console/design-system/bridgingio-desktop-console/pages/workspace.md`
- `openspec/changes/design-cross-platform-tauri-console/design-system/bridgingio-desktop-console/pages/settings-vault.md`

## 目标 / 非目标

**目标：**

- 定义跨平台桌面控制台的默认产品形态：`Tauri Shell` 作为本地 GUI 宿主，`bundled webview` 作为主管理面。
- 明确管理 UI 与 model-plane HTTP 端口的职责分离，使 UI 不依赖额外本地 HTTP 管理端口。
- 为首次启动引导、主工作台和设置 / vault / token 管理建立清晰的信息架构。
- 为 timeline 建立正式的“请求来源分组”语义，优先使用 token label，缺失时回落到请求指纹 / user-agent 摘要。
- 把 vault 解锁、长期 token 签发、token revoke 等高风险管理动作纳入本地可信桌面壳与本地用户验证边界。
- 给新的跨平台桌面控制台沉淀一套平台中立、简约但有辨识度的设计系统，供后续 macOS 重构复用。
- 明确 webview 方案下的性能约束与缓解策略，避免因为“agent 请求频率高”而把 UI 先验降级成弱能力界面。

**非目标：**

- 本次不实现桌面应用代码，也不直接替换当前 macOS SwiftUI 应用。
- 本次不要求保留“外部浏览器作为主入口”的产品路径；若未来需要浏览器模式，视为后续增量扩展。
- 本次不要求在首轮设计中覆盖完整 IDE 级别的终端工作台、复杂多窗格编排或远程浏览器管理模式。
- 本次不定义前端框架细节（React/Vue/Svelte）；Tauri 宿主与 control-plane 边界优先，前端实现栈后续决定。
- 本次不把 vault/token 的内部数据模型重新设计一遍；仅定义桌面管理面的宿主边界与交互要求。

## 决策

### 决策 1：跨平台默认宿主采用 Tauri Shell，本地管理面采用 bundled webview

桌面宿主明确建模为一个真正的 GUI 应用，而不是 TUI 或“托盘 + 外部浏览器重定向器”。Tauri Shell 负责：

- 托盘 / 菜单栏图标与窗口打开、聚焦、恢复
- 本地目录选择器、系统通知和未来的本地用户验证桥接
- 拉起 / attach / 重启 / 关闭 bundled core
- 与本地 control-plane 的平台原生 IPC 通信
- 将 control-plane 语义桥接给 bundled webview

bundled webview 负责：

- 首次启动引导页面
- 主工作台页面
- 设置 / vault / token 管理页面
- timeline、targets、diagnostics 等主要可视界面

之所以不选择“托盘壳 + 外部浏览器”作为默认形态，是因为一旦加入 vault 解锁、token 管理和本地用户验证，浏览器方案就会迫使系统再额外维护一层“浏览器可访问的本地可信管理服务”，安全边界和生命周期都更复杂。

考虑过的替代方案：

- **外部浏览器作为主入口**：开发上看似更快，但会引入本地管理 HTTP 服务、端口切换恢复、浏览器信任边界和 token / vault 高风险动作桥接复杂度。
- **继续以原生平台 UI 分别实现**：长期体验可能更强，但会拖慢 Linux / Windows 的交付，也不利于把控制台数据模型收敛到一个平台中立设计。
- **Electron 作为默认壳**：可行，但当前项目以 Rust core 为中心，Tauri 在宿主语言、sidecar 协调和本地 IPC 桥接上阻抗更小。

### 决策 2：管理 UI 与 model-plane HTTP 端口彻底解耦

跨平台桌面控制台不再依赖额外的本地 HTTP 管理端口。新的核心边界是：

```text
tray/menu/window
    -> Tauri Shell
        -> local control-plane bridge
            -> bridgingio-core

bundled webview
    -> talks to shell bridge
    -> does not talk to model-plane HTTP directly
```

这意味着：

- 用户在设置中修改 core 监听 host/port 时，不需要让“当前管理界面自己跳转到新端口”。
- UI 可以在 core 重启期间维持自己的宿主状态和恢复页面，而不被旧端口失效拖垮。
- model-plane HTTP 继续只承担 agent / MCP 数据面角色；本地管理面通过受信任 control-plane 工作。

考虑过的替代方案：

- **继续让管理 UI 跑在 localhost 页面上**：会使 UI 自己依赖正在修改的端口，导致重启、切换端口、恢复和安全隔离都更脆弱。
- **让 bundled webview 直接调用 model-plane HTTP**：会混淆本地受信任 control-plane 和外部 agent 数据面的边界，与现有规格方向冲突。

### 决策 3：信息架构采用“引导页 + 单窗口三大一级页面”，而不是旧 macOS 工作台形态

新的跨平台桌面控制台采用以下结构：

```text
Route A: Onboarding
  - 解释 runtime root 用途
  - 选择目录
  - 校验与恢复

Route B: Workspace
  - Targets
  - Timeline
  - Settings
```

主窗口结构建议为：

- 顶部窄状态栏：显示当前 runtime root / core 状态 / 重启状态 / 顶层操作。
- 左侧一级导航：`Targets`、`Timeline`、`Settings`。
- 中央主内容区：展示当前页面的主视图。
- 右侧或抽屉式次级检查器：展示 target 详情、timeline 事件详情、token 详情、diagnostics 等上下文内容。

页面设计决定如下：

- **Onboarding**
  - 单列居中，不和主工作台混在一起
  - 主动作只有“选择数据目录”
  - 同时展示目录用途、可写性预期、恢复语义
  - 在未配置有效目录前不允许进入主工作台；该页是 setup gate，而不是可跳过的欢迎页
- **Targets**
  - 左侧为 target 列表与筛选
  - 右侧为 target 详情 / 编辑器
  - 不与 timeline 混排，避免“目标管理”和“审计查看”相互干扰
- **Timeline**
  - 第一列先选择“当前被审计的使用方”，即 token label 或 HTTP 请求指纹 / user-agent 摘要
  - 第二列展示该使用方对应的执行简述卡片列表，而不是直接展开完整 transcript
  - 点击执行卡片进入独立详情界面，展示命令摘要、target、审批上下文、artifact 关系与按需 transcript 片段
- **Settings**
  - 采用分节而不是独立漂浮弹窗
  - 至少分为 `Core Runtime`、`Storage & Cache`、`Tokens`、`Vault`
  - `Tokens` 在 `Vault` 之前可见，便于先看已有 token 和 revoke

考虑过的替代方案：

- **继续沿用当前 macOS 页面对照关系**：会把跨平台产品继续绑在旧 UI 结构上，用户已明确不满意。
- **把 target / timeline / settings 混在单屏 dashboard**：信息密度会过高，也不利于新用户理解“配置”和“审计”的边界。

### 决策 4：新的设计语言走“简约、可信、轻动效”，而不是拟态 macOS 或重装饰 AI 风格

本次沿用设计系统中有价值的部分，并做人工收敛：

- **保留**
  - Inter 为主字体
  - 以蓝、白、青、绿色为主的可信安全色系
  - 大留白、低噪声、轻量动效
  - 清晰的卡片层级、状态 chips、分段设置区
- **明确排除**
  - 紫色主色
  - 液态玻璃和大面积透明模糊
  - “赛博终端”、“扫描线”、“霓虹 HUD”
  - 为了“AI 感”而堆叠流光、粒子和炫光

推荐的视觉基线：

- Light-first，默认浅色界面
- 主背景以偏冷白 / 浅蓝灰为主，避免纯平惨白
- 主要强调色为可信蓝，成功 / 受保护动作用绿色，风险提示用琥珀或红
- 只对页面进入、状态切换和 timeline 增量更新做轻量动画
- 必须遵守 `prefers-reduced-motion`

这套设计语言的目的不是模仿任一平台原生控件，而是建立一个“可在任意桌面壳中成立”的平台中立控制台样式，为后续 macOS 重构提供参考。

考虑过的替代方案：

- **继续贴近现有 macOS 原生视觉**：不利于跨平台，也不能满足“重新设计”目标。
- **走开发者终端感 / 霓虹 AI 风格**：辨识度强，但会牺牲长期可读性、可用性和安全产品的可信感。

### 决策 5：Timeline 采用“先选被审计使用方，再看执行卡片，再进详情”的三步模型

Timeline 页不是简单的时间倒序日志，而是三步结构：

1. **被审计使用方（Audited Actors）**
   - 优先按 token label 形成 bucket
   - 若请求没有 token，则按稳定请求指纹 / user-agent 摘要形成 bucket
   - 这一步回答的是“当前需要审计的使用方是谁”
2. **执行简述卡片（Execution Cards）**
   - 只展示该使用方执行过什么
   - 每张卡片至少包含命令摘要、target、时间、结果、审批状态、artifact / session 关联
   - 卡片视图必须保持紧凑，禁止在列表态直接把完整 transcript 平铺出来
3. **执行详情（Execution Detail）**
   - 用户点击卡片后进入独立详情界面
   - 详情界面展示审批上下文、artifact 关系、来源身份摘要，以及按需 transcript 片段

这里有两个关键边界：

- 分组依据来自 core 提供的安全投影，不能把明文 token、原始认证内部字段或可导致重放的细节展示到 UI。
- Timeline 用于管理与审计，不承担“高频终端输出逐字滚动”的唯一展示职责；列表态保持摘要化，transcript / 大输出只在详情页按需懒加载和分页。

考虑过的替代方案：

- **只按 target 或 session 分组**：无法满足“按 token / 请求来源观察 agent 行为”的目标。
- **完全平铺全局时间线**：当 token 和客户端增多后，可读性会迅速下降。
- **在列表中直接展开长输出**：会破坏“先看谁、再看做了什么”的审计路径，也会显著恶化 UI 性能。

### 决策 6：vault 解锁、长期 token 签发和 revoke 走 trusted desktop flow，而不是纯前端行为

bundled webview 可以承载这些管理入口，但真正的高风险动作必须通过宿主桥接执行：

- vault 解锁
- 长期 token 签发
- token revoke
- 未来的 secret reveal / export 或扩大 token scope

交互语义建议为：

- 用户在 Settings 中先看到 token 摘要列表和 revoke 动作
- 进入 Vault 区域时先看到 vault 状态、锁定状态和可执行动作
- 对需要本地用户验证的动作，由宿主拉起本地验证流程，然后将结果回传到 UI
- UI 永远只接收 display-safe summary 或一次性签发结果，不持有长期认证真相

这条决策使得“完整功能 webview”成为可行路径：界面足够完整，但安全边界仍由本地宿主和 core 保持。

考虑过的替代方案：

- **把这些能力留给未来原生页面处理**：会把“跨平台桌面控制台”降级成弱能力外壳，不符合当前目标。
- **全部做成普通前端表单提交**：会削弱本地可信管理动作的边界，也不便于接入未来 passkey / platform authenticator。

### 决策 7：性能问题通过数据流设计解决，而不是通过砍掉 webview 能力解决

用户担心“agent 调用频率高，web 版本会不会有性能问题”，这里需要把性能瓶颈分层：

- 高请求频率主要发生在 model-plane 与 core 之间
- UI 真正承受的是管理面级别的快照、事件和按需详情读取

因此管理面采用以下策略：

- `bootstrap snapshot + event stream + lazy detail fetch`
- Timeline 列表和 transcript 视图都采用虚拟化 / 分页
- 高频事件按短窗口合批，避免逐条重建整个页面
- 大日志 / 大 artifact 不做首屏全量加载
- 设置和管理页不与高频流式输出共用粗粒度全局刷新

结论是：  
**在正确的数据流设计下，bundled webview 足以承担完整的配置管理、timeline 审计和安全管理面。**  
真正需要谨慎控制的是 transcript / 超长输出 / 超高频事件刷新，而不是把 webview 先验降级为“只能做弱配置页”。

考虑过的替代方案：

- **因为担心性能而默认只做基础配置页**：会错失统一跨平台桌面管理面的价值，也不利于后续 macOS 重构。
- **让所有时间线和 transcript 都按浏览器页面实时逐行刷新**：这才是真正会导致性能问题的做法。

## 风险 / 权衡

- **[跨平台 webview 体验不如完全原生统一]** → 用平台中立设计系统换取更快的多平台落地，并把 macOS 的进一步原生优化留到后续重构。
- **[Tauri 的系统 WebView 在不同平台表现不完全一致]** → 视觉系统避免依赖重度滤镜、复杂透明和平台特定交互；以布局、信息架构和状态反馈为主。
- **[Timeline 分组需要核心层提供新的来源归因摘要]** → 在 `capability-aware-mcp` 和 `target-session-management` 中补充规格，明确安全投影和 control-plane 暴露边界。
- **[webview 做完整安全管理面可能被误解为“前端持有安全真相”]** → 明确高风险动作由宿主桥接执行，前端只拿 display-safe summary 和一次性响应。
- **[新的跨平台设计与现有 macOS UI 并存一段时间]** → 把这次设计定位为未来参考基线，而不是立即回写到当前 macOS 视觉实现。

## 迁移计划

1. 先新增 `desktop-operator-console` 能力，并补齐相关增量 specs，固定跨平台桌面控制台的产品边界。
2. 基于本次设计，在未来实现阶段搭建 Tauri Shell 原型，打通托盘、窗口、runtime root 选择和本地 control-plane 桥接。
3. 以新的信息架构实现 bundled webview 的 `Onboarding`、`Targets`、`Timeline`、`Settings` 页面。
4. 补齐 timeline 来源归因、token 摘要 / revoke、vault 解锁与本地用户验证等 control-plane 接口。
5. 为跨平台桌面控制台建立 UI 自动化合同，覆盖引导页、timeline 分组、settings/vault/token 管理流。
6. 在跨平台版本稳定后，再由 macOS 重构工作决定如何吸收这套设计系统与页面结构。

## 开放问题

- bundled webview 首版采用哪一种前端栈更合适：React、Vue 还是 Svelte？本次仅固定宿主和信息架构，不固定框架。
- Timeline 分组中的“请求指纹”是否只基于 user-agent + remote addr 摘要，还是需要加入更稳定的客户端标识策略？本次先固定“安全投影 + 稳定分组”原则。
- Tokens 和 Vault 是保留在同一个 `Settings` 页面中以分节呈现，还是后续拆为 `Settings > Security` 二级导航？本次偏向同页分节，以减少用户迷失。
