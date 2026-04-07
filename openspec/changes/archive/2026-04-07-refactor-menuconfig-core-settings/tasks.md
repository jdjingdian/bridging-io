## 1. 菜单拓扑与字段归属重构

- [x] 1.1 重构 `bridgingio-operator-console` 的 `Screen`、root menu entry 与标题映射，将 `Core`、`Storage`、`Model Plane` 收口为单一 `Core Settings` 一级入口。
- [x] 1.2 在 `Core Settings` 页面中增加 `Cache Settings --->` 与 `HTTP Interface Settings --->` 二级入口，并保留现有 core 通用字段。
- [x] 1.3 将 `storage.artifacts.backend`、`storage.artifacts.max_bytes` 迁移到 `Cache Settings` 子菜单，并将 `model_plane.http.host`、`model_plane.http.port`、`model_plane.http.allow_non_loopback` 迁移到 `HTTP Interface Settings` 子菜单。
- [x] 1.4 更新 `screen_for_field`、搜索结果跳转与相关导航路径，确保缓存/HTTP 字段会落到新的子菜单 screen。

## 2. Catalog 文案与值显示本地化

- [x] 2.1 更新 `en-US` 与 `zh-CN` catalog 中与 `Core Settings`、`Cache Settings`、`HTTP Interface Settings`、缓存字段、HTTP 字段相关的标题、标签与说明文案。
- [x] 2.2 为 `memory`、`filesystem` 及后续同类枚举值建立 canonical value -> localized display label 的共享映射，确保字段行内值与 choice popup 统一显示 operator-facing 文案。
- [x] 2.3 调整 `menuconfig` 的字段值渲染与 choice popup 渲染路径，使其显示本地化 display label，但保存时继续写回 canonical 配置值。
- [x] 2.4 确认 `allow_non_loopback` 在 `HTTP Interface Settings` 中继续作为正式 operator-facing 安全项展示，而不是被隐藏或回退为运行时报错提示。

## 3. 文档矩阵与回归验证

- [x] 3.1 更新 `openspec/specs/standalone-operator-console/spec.md`、`menuconfig-style-matrix/spec.md` 与 `operator-interface-matrix/spec.md` 的 root spec，使其与新菜单拓扑和本地化显示合同一致。
- [x] 3.2 更新 `docs/matrix/MENUCONFIG_STYLE_MATRIX.md` 与 `docs/matrix/LOCAL_OPERATOR_INTERFACE_MATRIX.md`，记录 `Core Settings`、二级子菜单入口与值显示/写回分离语义。
- [x] 3.3 为 `bridgingio-operator-console` 补齐或更新回归测试，覆盖主菜单拓扑、字段跳转、新子菜单导航与 choice popup 的本地化值显示。
- [x] 3.4 运行相关测试与手工回归，确认菜单重构后搜索、保存回写、中文 locale 展示与 non-loopback 安全护栏行为没有回归。
