## 新增需求

### 需求:`menuconfig` 必须将 core 基础运行配置收口到单一 `Core Settings` 入口
`bridgingio-core menuconfig` 必须将面向操作员的 core 基础运行配置收口到单一一级入口 `Core Settings`（中文 locale 为 `核心设置`），而不得继续把 `Core`、`Storage` 与 `Model Plane` 作为三个独立一级菜单长期并列暴露。该入口必须在单栏逐级进入拓扑下继续承载 core 通用字段，并通过正式子菜单暴露缓存与 HTTP 接口相关设置。

#### 场景:操作员浏览主菜单中的基础配置入口
- **当** 操作员进入 `bridgingio-core menuconfig` 主菜单
- **那么** 系统必须显示单一的 `Core Settings` 或 `核心设置` 入口
- **并且** 不得继续同时显示独立的 `Core`、`Storage` 与 `Model Plane` 三个一级入口

#### 场景:操作员进入 Core Settings 页面
- **当** 操作员从主菜单进入 `Core Settings` 或 `核心设置`
- **那么** 系统必须继续展示 core 通用字段，并提供 `Cache Settings --->` 与 `HTTP Interface Settings --->`（中文 locale 下分别为 `缓存设置 --->` 与 `HTTP 接口设置 --->`）两个正式子菜单入口

#### 场景:操作员进入缓存与 HTTP 子菜单
- **当** 操作员从 `Core Settings` 页面进入缓存或 HTTP 子菜单
- **那么** 系统必须在 `Cache Settings` 子菜单中展示当前正式支持的缓存字段
- **并且** 必须在 `HTTP Interface Settings` 子菜单中展示 HTTP 地址、端口与 non-local binding 控制项
- **并且** 不得改为分栏式配置页或脱离既有 `--->` 单栏导航拓扑

### 需求:`menuconfig` 必须对字段值与候选值使用本地化产品文案渲染
`menuconfig` 的 operator-facing 文案通过 catalog 渲染时，必须同时覆盖字段标签、字段说明、字段当前值与 choice popup 候选值，而不是只翻译字段名后继续裸露内部枚举。系统必须允许显示值与持久化值分离：界面显示 operator-facing 本地化文案，配置文件继续保持 canonical 值。

#### 场景:中文 locale 查看缓存 backend 当前值
- **当** core locale 为 `zh-CN`，且操作员在 `Cache Settings` 中查看缓存存储方式字段
- **那么** 当前值必须以 `文件系统` 或 `运行内存` 等中文 operator-facing 文案显示
- **并且** 不得继续直接显示 `filesystem` 或 `memory` 原始枚举值

#### 场景:中文 locale 打开缓存 backend 选择弹窗
- **当** core locale 为 `zh-CN`，且操作员在 `Cache Settings` 中打开缓存存储方式的 choice popup
- **那么** 系统必须把候选值显示为 `文件系统` 与 `运行内存`
- **并且** 在用户确认后继续把 canonical 值写回配置，而不是把中文显示名持久化到 TOML

#### 场景:HTTP 字段使用产品化命名
- **当** 操作员在 `HTTP Interface Settings` 页面浏览 host 与 port 配置
- **那么** 系统必须使用 `HTTP Listening Address` / `HTTP 监听地址` 与 `HTTP Listening Port` / `HTTP 监听端口` 等 operator-facing 文案
- **并且** 不得继续在界面中直接使用 `HTTP Host` 与 `HTTP Port` 作为正式产品命名

## 修改需求

无。

## 移除需求

无。
