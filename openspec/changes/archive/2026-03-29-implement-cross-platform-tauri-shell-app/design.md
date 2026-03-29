## 上下文

`design-cross-platform-tauri-console` 已经把跨平台桌面控制台的产品形态、页面信息架构和宿主职责定义清楚，并且仓库里已经有三类可复用资产：

- `source/ui/tauri-console-web` 中的 bundled webview 页面
- `source/rust/bridgingio-desktop-host` 中的宿主合同、启动分流、bridge 与资产 contract tests
- `source/rust/bridgingio-mcp/src/bin/bridgingio-core.rs` 中的 `ui-managed-ephemeral` core sidecar 入口

但当前实现仍停在“合同层可测试、页面可预览、宿主目录存在”而不是“桌面应用可启动”：

- `source/ui/tauri-console-web/src-tauri` 目前为空骨架，没有可运行的 Tauri 入口
- `bridgingio-desktop-host` 只是 library crate，没有 bin target，无法作为桌面应用直接运行
- bundled 页面在 `source/ui/tauri-console-web` 与 `source/rust/bridgingio-desktop-host/assets` 双份存在，长期容易漂移
- `ui-managed-ephemeral` 在非 Unix 宿主上仍受本地 control-plane transport 缺失约束，目前只能受控报 deferred，而不能假装“已跨平台完成”

这次设计的目标不是重新做一遍桌面产品设计，而是把“已有产品设计 + 现有合同层”落成一个长期可维护的工程结构，并为后续 Windows named pipe / 其他宿主 transport 实现保留清晰扩展点。

## 目标 / 非目标

**目标：**

- 在 `source/ui/tauri-console-web/src-tauri` 中建立正式的 Tauri 宿主项目结构，使仓库内存在可构建、可运行的桌面宿主工程。
- 保持 `bridgingio-desktop-host` 作为宿主行为、启动分流与 trusted bridge 的复用库，而不是把所有逻辑重新塞进 Tauri 入口文件。
- 固定桌面启动链路：runtime root 偏好读取、onboarding/recovery/workspace 分流、sidecar 启动、UI attach、bootstrap、主工作台展示。
- 收敛 bundled webview 资产为单一真相源，避免 HTML 双份维护。
- 为桌面宿主补充可执行的 smoke 验证，并把“受支持平台可启动”和“transport 尚未补齐的平台必须显式失败”都变成可验证结果。

**非目标：**

- 本次不重新设计 `Targets`、`Timeline`、`Settings` 页面，也不重写 bundled webview 的视觉结构。
- 本次不把 `bridgingio-core` 改成 in-process 嵌入式库；sidecar 形态保持不变。
- 本次不在同一变更内完成 Windows named pipe 或其他非 Unix 本地 transport 的完整实现；该缺口继续由既有跨平台 adapter 路线承接。
- 本次不替换或删除现有 macOS SwiftUI 控制台。

## 决策

### 决策 1：正式桌面应用采用“`src-tauri` app crate + `bridgingio-desktop-host` contract library”两层结构

新的桌面实现分成两层：

```text
source/ui/tauri-console-web/src-tauri
  -> Tauri app crate
     -> app_state / commands / windowing / sidecar wiring
        -> bridgingio-desktop-host
           -> startup route / host contract / trusted bridge / storage
```

职责划分如下：

- `src-tauri` 负责 Tauri Builder、窗口、托盘、权限、目录选择、sidecar spawn、前端命令注册与宿主生命周期
- `bridgingio-desktop-host` 负责宿主领域模型、runtime root 偏好、启动路由、bridge 协议与合同测试

这样做的原因：

- 现有 library 已经包含真实可复用的行为，不应被一次性抛弃
- Tauri 层天然包含大量平台与框架胶水代码，直接污染 library 会降低可测性
- 后续如果 Linux / Windows / OpenHarmony 需要不同壳层，仍可复用同一 host contract

考虑过的替代方案：

- **直接把 `bridgingio-desktop-host` 改成桌面 bin**：会把 Tauri 框架绑定、窗口管理与业务合同混在一起，复用性和测试边界都更差
- **在 `source/rust` 新建单独 app crate，不放在 `src-tauri`**：会弱化 Tauri 工程的标准目录形态，增加后续开发与打包心智负担

### 决策 2：`source/ui/tauri-console-web` 是 bundled 页面单一真相源

bundled webview 页面继续以 `source/ui/tauri-console-web` 为源码真相源。宿主内嵌副本如果仍然需要存在，必须改为构建期生成或同步产物，而不是与源码目录并列维护。

建议结构：

```text
source/ui/tauri-console-web/
  index.html
  onboarding.html
  src-tauri/
    Cargo.toml
    tauri.conf.json
    capabilities/
    src/
```

配套原则：

- 页面 contract test 读取源码真相源
- 宿主如果需要 `include_str!` 内嵌页面，则由构建步骤从真相源复制到生成目录
- 禁止继续手工维护两份 HTML 并依靠字符串相等测试兜底

考虑过的替代方案：

- **继续保留双份 HTML 并用测试比对**：短期简单，但长期一定会出现“改了一份、漏了另一份”的漂移
- **把页面真相源迁回 Rust crate assets**：会让 UI 开发与页面设计回到 Rust 目录，不利于桌面前端长期演进

### 决策 3：桌面启动流程由宿主状态机驱动，而不是由静态 hash 页面自我决定

正式启动链路定义为：

```text
App start
  -> load runtime-root preference
  -> resolve startup route
     -> onboarding / recovery
     -> or launch managed core sidecar
  -> attach UI over trusted control-plane
  -> load bootstrap snapshot
  -> enter workspace
```

这里的关键点是：

- hash route 仍可用于页面内部导航，但“是否能进入 workspace”由宿主状态决定
- onboarding/recovery/workspace 的切换必须复用 `bridgingio-desktop-host` 的启动路由逻辑
- workspace 不得在 sidecar 未 ready、attach 未完成时乐观展示伪连接态

Tauri commands 应保持薄层，只暴露：

- runtime root 选择与完成引导
- 读取初始宿主状态
- 触发 attach / bootstrap / refresh
- 受控重启、通知与本地可信动作桥接

考虑过的替代方案：

- **让前端页面靠本地 storage 或 hash 自己决定是否进入 workspace**：会使启动状态脱离真实 core 生命周期
- **在前端直接与 model-plane HTTP 通信**：违背现有安全边界，也会让端口变更与恢复流程变脆

### 决策 4：`bridgingio-core` 继续作为 sidecar，并通过显式打包链路进入 Tauri 项目

桌面应用仍然采用 `bridgingio-core ui-managed-ephemeral` sidecar 形态。Tauri 项目负责引用、打包和启动该 sidecar，而不是把 core 链接进同一进程。

这样做的原因：

- 与既有 `Tauri Shell + bundled webview + Rust core sidecar` 架构保持一致
- 可以继续复用现有 `bridgingio-core` 启动模式与 control-plane 语义
- 宿主崩溃、重启和 sidecar 生命周期可以保持清晰边界

建议打包策略：

- 以 Tauri sidecar / external binary 机制分发 `bridgingio-core`
- 用显式脚本或构建步骤把目标平台的 `bridgingio-core` staged 到 `src-tauri` 期望的位置
- sidecar 命令行由 `TauriShellHostSpec` 负责生成，避免 Tauri 层自己拼接 argv

考虑过的替代方案：

- **把 core 直接嵌进 Tauri 进程**：实现面过大，也会打散现有 sidecar 语义与故障隔离边界
- **运行时随意寻找任意本地 `bridgingio-core` 可执行文件**：会让开发、打包和诊断都失去稳定性

### 决策 5：平台支持按 local transport 能力分阶段兑现，缺口必须 fail closed

本次实现阶段把平台支持拆成两层：

- **第一阶段**：在当前已有 local transport 的宿主平台上，完成真实 Tauri 启动链路
- **第二阶段**：在 Windows 等宿主上补齐 named pipe 或等价 transport 后，再把“跨平台已完成”升级为真正成立

这意味着：

- Tauri 项目结构必须从一开始按跨平台目录和能力边界设计
- 但运行时不得在 transport 缺失的平台上伪造成功启动
- smoke 验证必须把“受控 deferred/unsupported”视为当前已知限制，而不是误判为页面层故障

考虑过的替代方案：

- **等所有平台 transport 都补齐后再落 Tauri 项目**：会让仓库继续停留在“只有设计没有宿主工程”的状态
- **先用 HTTP 或临时绕路冒充本地 control-plane**：会破坏既有架构边界，并把技术债固化进首版桌面宿主

### 决策 6：宿主 smoke 验证采用“同一路径的轻量模式”，而不是单独造一套演示代码

为了满足“项目真的可启动”的验证要求，宿主需要提供一条与正式启动共享大部分代码路径的 smoke 验证模式。验证重点包括：

- Tauri app 配置可编译并可启动
- 页面资产可被宿主装载
- runtime root 启动分流可执行
- sidecar 命令拼装与 attach/bootstrap 顺序可被真实执行或可信桩验证
- transport 缺失平台会返回明确诊断

这里不建议再造一套“只为测试存在”的假应用。更稳妥的方式是：

- 把大部分逻辑收敛到 `app_state` / `commands` / `sidecar` 模块
- smoke 模式复用这些模块，只在窗口展示或外部依赖上做最小替身

考虑过的替代方案：

- **只保留 library unit tests**：证明不了 Tauri 宿主工程真的存在且能启动
- **只做人工打开应用验证**：不够稳定，也不满足长期自动化要求

## 风险 / 权衡

- **[Tauri sidecar 打包路径容易随平台或构建模式出错]** → 通过显式 staging 规则和统一命令入口收敛，不让多个模块各自猜测 sidecar 位置
- **[单一真相源改造会触碰现有 `include_str!` 资产测试]** → 先保留兼容层，但把其改为生成产物，随后再逐步删掉人工双份资产
- **[Windows transport 尚未补齐会让“跨平台”字面上仍不完整]** → 在 proposal、design、tasks 和 smoke 输出中明确区分“宿主结构跨平台化”与“所有宿主均可运行”两个阶段
- **[Tauri 宿主状态机可能与现有前端 hash 路由发生职责重叠]** → 由宿主掌握 route authority，前端只负责展示与工作台内导航

## 迁移计划

1. 建立 `src-tauri` 的正式 Tauri app crate，并把其接入现有仓库结构。
2. 抽出 `app_state`、`commands`、`sidecar`、`windowing` 等宿主模块，复用 `bridgingio-desktop-host` 现有逻辑。
3. 把 bundled 页面收敛到 `source/ui/tauri-console-web` 单一真相源，并改造宿主资产引用链路。
4. 为宿主增加 smoke 验证与开发启动文档，确保受支持平台可以真实启动。
5. 将 non-Unix local transport 继续作为后续实现阶段推进，并在当前宿主中保留明确的 deferred/unsupported 诊断。

## 开放问题

- sidecar staging 最终采用仓库脚本、Cargo 辅助命令，还是 Tauri 构建钩子，哪种在当前仓库里最容易维护？
- `bridgingio-desktop-host` 是否需要保留生成后的内嵌资产副本，还是在完成 Tauri 迁移后彻底移除 `assets/` 下的人工页面拷贝？
- 首轮“受支持宿主平台”是否限定为当前 Unix 宿主，还是在 Linux 上同步提供一条同等级别的启动验证路径？
