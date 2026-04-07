## 为什么

当前 `menuconfig` 把 `Core`、`Storage` 和 `Model Plane` 分散为三个一级菜单，但对操作员来说，这几组能力本质上都属于 core 的基础运行配置。现有结构既增加了主菜单理解成本，也让中文界面继续暴露 `Artifact`、`Model Plane`、`HTTP Host` 这类偏内部实现的词汇，不利于把 `menuconfig` 打磨成稳定、清晰的 operator-facing 产品表面。

现在需要对这部分菜单做一轮信息架构重构与语言产品化定义：把基础配置收口到单一 `Core Settings / 核心设置` 入口，在其下用更贴近用户心智的子菜单组织缓存与 HTTP 接口设置，并补齐中文环境下字段名与候选值的正式本地化显示，减少中英夹杂和概念跳跃。

## 变更内容

- 将 `menuconfig` 主菜单中的 `Core`、`Storage`、`Model Plane` 三个一级入口重组为单一一级入口 `Core Settings`；中文 locale 下显示为 `核心设置`。
- 在 `Core Settings / 核心设置` 下保留现有 core 基础字段，并新增两个子菜单入口：`Cache Settings / 缓存设置` 与 `HTTP Interface Settings / HTTP 接口设置`。
- 将当前 `storage.artifacts.backend` 与 `storage.artifacts.max_bytes` 收口到 `Cache Settings` 子菜单中，并将 operator-facing 文案从 `Artifact` 术语改为缓存导向表达。
- 将当前 `model_plane.http.host`、`model_plane.http.port` 与 `model_plane.http.allow_non_loopback` 收口到 `HTTP Interface Settings` 子菜单中，并统一为“监听/接口暴露”语义下的产品文案。
- 为 `menuconfig` 的 choice popup 与行内值展示补齐缓存 backend 等候选值的 locale catalog 渲染，确保中文 locale 下 `filesystem` / `memory` 能分别显示为 `文件系统` / `运行内存`，而不是继续裸露内部枚举值。
- 本次变更不调整底层配置 schema，不新增 artifact cache 的 `root` / `eviction_policy` 菜单入口；后续若要产品化缓存淘汰策略，将作为独立能力继续设计。

## 功能 (Capabilities)

### 新增功能

- 无

### 修改功能

- `standalone-operator-console`: 调整 `menuconfig` 中 core 基础配置的信息架构、菜单层级与 operator-facing 文案，要求基础运行设置收口到 `Core Settings / 核心设置` 并支持缓存/HTTP 子菜单。
- `menuconfig-style-matrix`: 更新主菜单与相关子菜单的 row grammar、导航入口与字段映射，反映 `Core Settings`、`Cache Settings`、`HTTP Interface Settings` 的正式结构与文案。
- `operator-interface-matrix`: 更新 `menuconfig` 对 `storage.artifacts.*` 与 `model_plane.http.*` 字段的 operator-facing 展示、命名和入口映射，确保接口矩阵与新的菜单组织保持一致。

## 影响

- 受影响代码主要包括 `source/rust/bridgingio-operator-console` 的菜单 screen 定义、根菜单/子菜单 entry 组织、搜索字段归属、choice popup 显示与值本地化路径。
- 受影响文案主要包括 `source/rust/bridgingio-engine/resources/i18n/en-US.toml` 与 `zh-CN.toml` 中与 `Core`、`Storage`、`Model Plane`、缓存字段、HTTP 字段及候选值相关的 catalog 条目。
- 受影响规范与文档包括 `openspec/specs/standalone-operator-console/spec.md`、`openspec/specs/menuconfig-style-matrix/spec.md`、`openspec/specs/operator-interface-matrix/spec.md` 以及后续对应的 `docs/matrix/*` 真相文档。
