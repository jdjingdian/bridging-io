## 1. Tauri 宿主工程骨架

- [x] 1.1 在 `source/ui/tauri-console-web/src-tauri` 下创建正式的 Tauri app crate，补齐 `Cargo.toml`、`tauri.conf.json`、权限配置、入口源码与基础目录结构
- [x] 1.2 将宿主工程接入 `bridgingio-desktop-host`，建立 `app_state`、`commands`、`windowing`、`sidecar` 等模块，而不是在 `main.rs` 中重写宿主合同逻辑
- [x] 1.3 定义 `bridgingio-core` sidecar 的开发态与打包态 staging 方案，使 Tauri 宿主可以稳定定位并启动 `ui-managed-ephemeral` 可执行文件

## 2. 启动链路与受信任桥接

- [x] 2.1 将 runtime root 偏好读取、startup route 分流与 onboarding/recovery/workspace 切换接入 Tauri 宿主启动流程
- [x] 2.2 基于 `TauriShellHostSpec` 和 `TrustedControlPlaneBridge` 实现 sidecar 启动、UI attach、bootstrap 读取与受控重启链路
- [x] 2.3 把窗口打开 / 聚焦、系统通知、runtime root 选择与本地可信动作桥接成 Tauri commands，供 bundled webview 正式调用
- [x] 2.4 在 local transport 尚未补齐的平台上返回明确的 deferred/unsupported 诊断，禁止伪造 workspace 已连接状态

## 3. Bundled 资产单一真相源

- [x] 3.1 将 `source/ui/tauri-console-web` 设为 onboarding 与 workspace 页面单一真相源，移除或生成化当前 Rust crate 中的人工双份 HTML
- [x] 3.2 改造宿主资产装载与 contract tests，使其验证“由单一来源产出的 bundled 页面”而不是历史手工拷贝
- [x] 3.3 明确页面源码、生成产物和 sidecar 二进制在开发与打包流程中的目录约定，避免后续宿主与前端各自维护路径真相

## 4. 启动验证与自动化

- [x] 4.1 为 Tauri 宿主的 `app_state`、startup route、sidecar 命令拼装和桥接顺序补充单元测试或集成测试
- [x] 4.2 提供可执行的桌面宿主 smoke 验证，覆盖宿主启动、页面装载、runtime root 分流、attach/bootstrap 路径，以及 transport 缺失时的受控失败诊断
- [x] 4.3 在当前受支持宿主平台上完成一次真实启动验证，并把可复现的开发命令与预期结果固化到文档或脚本

## 5. 文档与后续平台衔接

- [x] 5.1 更新桌面宿主开发文档，说明 `src-tauri`、`bridgingio-desktop-host`、bundled 页面与 `bridgingio-core` sidecar 之间的依赖关系
- [x] 5.2 记录 non-Unix local transport 的后续衔接点，明确 Windows named pipe 或等价平台 IPC 仍是“真正跨平台可运行”的剩余前置项
