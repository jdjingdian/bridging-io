## 上下文

当前 `menuconfig` 已经把整体交互收敛为单栏、逐级进入的 `menuconfig` 拓扑，但 `Core`、`Storage` 与 `Model Plane` 仍以三个一级菜单暴露给操作员。这种切分更接近底层配置模型的技术分层，而不是操作员对“基础运行设置”的理解方式：用户往往只是在做 core 的基础配置，却需要先理解为什么缓存和 HTTP 接口不属于 `Core`。

与此同时，当前 operator-facing 文案仍然直接暴露了一批偏内部的实现术语，例如 `Artifact Backend`、`Artifact Max Bytes`、`HTTP Host`、`Model Plane`。这在中文 locale 下尤其明显：主菜单与字段说明已经部分中文化，但缓存 backend 的候选值和部分字段名仍然保留英文原词，形成“菜单是中文、选项值是内部枚举”的割裂体验。

这次变更主要横跨三个层面：

- `bridgingio-operator-console` 的 screen 拓扑、根菜单组织、字段归属与 choice/value 显示。
- `bridgingio-engine` i18n catalog 中的主菜单、子菜单、字段标签、字段说明与候选值文案。
- `menuconfig` 的规范和矩阵真相，确保后续实现与文档对同一套产品语言和信息架构负责。

另一个约束是：底层配置模型当前仍然保留 `storage.artifacts.root` 与 `storage.artifacts.eviction_policy`，但 menuconfig 尚未把它们产品化。本次不应借机把缓存策略、缓存根目录等更宽范围一起塞入，以免把“菜单重构与文案收口”扩成“缓存配置全面重做”。

## 目标 / 非目标

**目标：**

- 将 `Core`、`Storage`、`Model Plane` 三个一级菜单收口为一个正式的 `Core Settings / 核心设置` 一级入口。
- 在 `Core Settings` 下为缓存与 HTTP 相关配置建立正式的二级子菜单，而不是继续把这些能力平铺为独立一级菜单。
- 把 `Artifact`、`HTTP Host` 等偏内部术语替换为更贴近 operator 心智的产品文案。
- 让 choice popup、字段行内值和中文 locale 下的候选值都通过 catalog 渲染，避免直接暴露 `memory` / `filesystem` 这类内部枚举。
- 保持底层配置字段路径、校验与序列化真相不变，确保这次只调整 operator-facing 结构和表达，不引入 schema 迁移。

**非目标：**

- 不在本次变更中新增 `storage.artifacts.root` 或 `storage.artifacts.eviction_policy` 的 menuconfig 暴露入口。
- 不重命名底层 TOML 字段路径，也不改变 `storage.artifacts.backend = memory|filesystem` 与 `model_plane.http.*` 的持久化值。
- 不改变 `allow_non_loopback` 的安全语义；该开关继续作为 non-loopback 暴露的正式护栏存在。
- 不把这次菜单重构扩展为桌面 GUI、CLI help 或其他 non-menuconfig surface 的统一术语重写。

## 决策

### 决策 1：主菜单收口为单一 `Core Settings`，缓存与 HTTP 进入二级子菜单

主菜单中原有的 `Core`、`Storage`、`Model Plane` 三个一级入口收口为单一 `Core Settings`。`Core Settings` 页面保留当前真正属于 core 通用属性的字段：

- `Instance Name`
- `Data Dir`
- `Log Level`
- `Core Language`
- `Cache Settings --->`
- `HTTP Interface Settings --->`

其中 `Cache Settings` 页面先只承载：

- `storage.artifacts.backend`
- `storage.artifacts.max_bytes`

`HTTP Interface Settings` 页面承载：

- `model_plane.http.host`
- `model_plane.http.port`
- `model_plane.http.allow_non_loopback`

这样做的原因是：

- 对操作员而言，这三组能力本质上都属于 core 基础设置，而不是三个平级产品域。
- 通过二级子菜单承接缓存与 HTTP，可以保持 `menuconfig` 现有单栏逐级进入拓扑，而不需要引入分组标题、折叠组或分栏布局。
- 相比继续让 `Storage` 与 `Model Plane` 独立存在，这种结构更容易支持后续在 `Core Settings` 下继续扩展基础运行配置。

替代方案：

- 保持三个一级菜单不变，只改文案。问题是用户心智负担没有真正降低，`Core` 与基础配置之间仍然存在概念断层。
- 把缓存和 HTTP 字段直接平铺回 `Core Settings`。问题是随着字段继续增长，页面会重新变长，后续新增缓存策略等内容时会再次失控。

### 决策 2：operator-facing 文案以“用户能理解什么”为准，而不是以底层模型命名为准

本次将采用以下正式 operator-facing 命名方向：

- `Core Settings` / `核心设置`
- `Cache Settings` / `缓存设置`
- `HTTP Interface Settings` / `HTTP 接口设置`
- `Cache Storage Mode` / `缓存存储方式`
- `Max Cache Usage` / `缓存最大占用空间`
- `HTTP Listening Address` / `HTTP 监听地址`
- `HTTP Listening Port` / `HTTP 监听端口`
- `Allow Non-Local Binding` / `允许绑定非本机地址`
- `Filesystem` / `文件系统`
- `Memory` / `运行内存`

这里刻意避免继续把 `Artifact`、`Model Plane`、`HTTP Host` 直接暴露给操作员。底层配置和代码结构仍然可以保留原命名，但 catalog 与菜单信息架构必须优先服务 operator 的理解成本。

替代方案：

- 使用更贴近实现的术语，例如 `Artifact Cache Backend`、`HTTP Bind Host`。优点是技术精确，缺点是 operator-facing 可读性差。
- 将 `memory` 在中文中翻成 `内存`。这也可接受，但 `运行内存` 更能避免与“保存到某个抽象缓存层”的理解混淆，因此本提案优先采用后者。

### 决策 3：显示值与持久化值分离，choice popup 与字段行都渲染本地化显示名

底层持久化值继续保持 canonical 枚举，例如：

- `storage.artifacts.backend = "memory" | "filesystem"`
- `model_plane.http.allow_non_loopback = true | false`

但 `menuconfig` 中所有面向操作员的显示必须通过 locale catalog 投影：

- 字段行中的当前值不再直接显示原始枚举，而显示本地化标签。
- choice popup 的候选项也不再直接列出原始值，而显示与当前语言一致的 operator-facing 候选名。
- 保存与编辑提交时仍然回写 canonical 值，而不是把本地化显示名写回配置。

这样做的原因是：

- 只翻译字段标签、不翻译值，会让中文 locale 下继续出现明显中英混杂。
- 把显示名直接写回持久化配置会破坏现有校验、序列化和兼容性边界。
- “显示值”和“真实值”分离后，未来可以继续为其他枚举字段补充产品化文案，而不需要修改 schema。

替代方案：

- 继续直接显示 canonical 枚举值。问题是中文 locale 的产品完成度明显不够。
- 把本地化值作为新的持久化值写回。问题是会引入 schema 迁移，并破坏现有英文 canonical 合同。

### 决策 4：`allow_non_loopback` 继续属于 HTTP 子菜单，而不是被隐藏

尽管本次产品化的重点是 `HTTP Host` / `HTTP Port` 改名，但 `allow_non_loopback` 不能因为“看起来更技术”就被移出或隐藏。它当前是 non-loopback 暴露的正式安全护栏，必须继续留在 `HTTP Interface Settings` 子菜单中，以 operator-facing 文案承载其安全意义。

这样做的原因是：

- `host` 与 `allow_non_loopback` 共同决定“监听在哪里”和“是否允许暴露到非本机地址”，它们属于同一产品域。
- 如果只保留地址和端口、隐藏暴露开关，用户会更难理解为什么某些 host 值被拒绝。
- 与其把它单独留在不存在的 `Model Plane` 菜单，不如把它纳入 `HTTP Interface Settings` 的高级项。

替代方案：

- 隐藏该字段，仅在验证失败时报错提示。问题是会把正式配置前置条件退化为运行时惊喜。
- 把它继续留在一级菜单或其他杂项页面。问题是会破坏 HTTP 配置的完整性。

### 决策 5：本次缓存子菜单保持 MVP 范围，不顺带产品化 `root` 与 `eviction_policy`

虽然底层模型已经具备 `storage.artifacts.root` 与 `storage.artifacts.eviction_policy`，但本次 `Cache Settings` 先只覆盖当前已在 menuconfig 正式暴露的两项：backend 和 max bytes。`eviction_policy` 的产品语义仍待单独设计，例如“按最早创建删除”还是“按最早使用删除”，这应在后续独立 change 中定义。

这样做的原因是：

- 本次任务的确定性部分是菜单重构与命名产品化，而不是缓存策略语义设计。
- 如果现在顺手把 `eviction_policy` 拉进来，会迫使我们在提案阶段就对 FIFO/LRU 等合同作出仓促承诺。
- 保持 MVP 范围，有利于先把信息架构和语言表面稳定下来。

## 风险 / 权衡

- [二级子菜单比直接平铺多一次进入] → 换来主菜单更清晰的心智模型，以及后续扩展缓存/HTTP 设置时更稳定的容器结构。
- [显示值与真实值分离会增加一层映射逻辑] → 通过集中式 value-display helper 和 catalog key 约定降低重复实现风险。
- [中文产品文案更友好，但可能偏离部分底层术语] → 通过保留 canonical config path 搜索与持久化值，确保实现层和 operator-facing 层解耦。
- [`eviction_policy` 继续不暴露会让缓存设置暂时不完整] → 明确将其视为后续独立设计议题，避免本次 scope 漂移。

## 迁移计划

1. 在 `openspec` 中更新 `standalone-operator-console`、`menuconfig-style-matrix` 与 `operator-interface-matrix` 的规范，确定新的菜单拓扑与文案合同。
2. 在 `bridgingio-operator-console` 中重构 `Screen`、root entry、`Core Settings` 子菜单入口与字段归属，并保持搜索/定位逻辑指向新的 screen。
3. 在 `bridgingio-engine` i18n catalog 中更新主菜单、子菜单、字段标签、字段说明与候选值显示名。
4. 为 `menuconfig` 的 choice popup 与字段行增加 canonical value -> localized display label 的共享映射路径。
5. 更新实际的 matrix 文档与回归测试，确认导航、搜索、值显示与保存回写行为保持一致。

## 开放问题

- `Max Cache Usage` 英文是否最终保留为当前提案，还是改为更工程化的 `Maximum Cache Size`，可以在实现前最后确认；两者都比 `Artifact Max Bytes` 更适合作为 operator-facing 文案。
- `running memory` 与 `memory` 的中文映射当前优先定为 `运行内存`；若后续真实用户反馈更偏好 `内存缓存`，可作为 catalog 微调单独处理，不影响本次结构决策。
