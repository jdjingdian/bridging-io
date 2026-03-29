## 为什么

BridgingIO 已经完成了跨平台桌面控制台的产品设计、bundled webview 页面和一部分宿主合同测试，但仓库当前仍缺少一个真正可启动的 Tauri app 项目。现状使 `desktop-operator-console` 规格在“页面与合同存在”层面成立，却无法以桌面发行物形态验证单窗口、sidecar 启动、runtime root 分流和本地 control-plane attach 的真实链路。

现在需要把已有的设计资产和 `bridgingio-desktop-host` 合同层落成一个可运行、可验证、可长期演进的桌面工程骨架，并明确区分“首轮可启动的宿主实现”与“Windows named pipe 等跨平台 transport 补齐”这两个阶段，避免继续把设计完成误判为实现完成。

## 变更内容

- 在 `source/ui/tauri-console-web` 下补齐正式的 `src-tauri` 宿主项目结构，使仓库内存在可运行的 Tauri Shell app，而不再只有静态 webview 页面与空目录骨架。
- 将现有 `bridgingio-desktop-host` 继续定位为宿主行为与桥接合同库，并由 Tauri app 负责把窗口、托盘、目录选择、sidecar 启动、bridge command 和页面路由真正绑定起来。
- 固定桌面应用的启动链路：读取 runtime root 偏好、进入 onboarding/recovery/workspace 分流、按 `ui-managed-ephemeral` 模式启动 bundled `bridgingio-core`、完成 attach 与 bootstrap，再进入主工作台。
- 收敛 bundled webview 资产的单一真相源，避免 `source/ui/tauri-console-web` 与 `source/rust/bridgingio-desktop-host/assets` 长期双份漂移。
- 为桌面宿主补充可执行的 smoke 验证，至少覆盖 Tauri 壳可启动、sidecar 命令拼装、runtime root 分流和桥接握手；对于当前仍未补齐的平台原生 local transport，要求显式失败并给出受控诊断，而不是伪装为“跨平台已完成”。

## 功能 (Capabilities)

### 新增功能

### 修改功能

- `desktop-operator-console`: 增加“仓库内必须交付可运行的 Tauri 宿主项目、统一资产来源与正式启动链路”的要求。
- `quality-and-test-automation`: 增加桌面宿主项目的 smoke 验证要求，确保 bundled GUI 不是仅靠静态 HTML 与合同测试宣称完成。

## 影响

- 受影响的目录包括 `source/ui/tauri-console-web/src-tauri`、`source/rust/bridgingio-desktop-host`、以及 bundled asset 的生成或引用链路。
- 受影响的启动与桥接路径包括 runtime root 偏好持久化、`bridgingio-core ui-managed-ephemeral` sidecar 启动、UI attach、bootstrap 加载与错误恢复。
- 受影响的验证体系包括桌面宿主 smoke 测试、现有 host contract tests，以及后续针对非 Unix transport 缺口的受控失败诊断。
