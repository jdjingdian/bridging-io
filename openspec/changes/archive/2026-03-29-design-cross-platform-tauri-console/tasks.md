## 1. Tauri 宿主与本地桥接基础

- [x] 1.1 搭建跨平台 Tauri Shell 宿主，打包 bundled webview 与 `bridgingio-core` sidecar，并建立单窗口 + 托盘 / 菜单栏生命周期
- [x] 1.2 实现 runtime root 启动偏好的本地持久化，以及“无目录时进入硬门槛引导页 / 目录失效时进入恢复态”的启动分流
- [x] 1.3 实现 bundled webview 到本地 control-plane 的受信任桥接，确保桌面管理面不依赖额外本地 HTTP 管理端口
- [x] 1.4 实现宿主层的窗口打开 / 聚焦、系统通知、managed restart 状态展示与错误恢复入口

## 2. Core / App API 与 timeline 归因

- [x] 2.1 为 `bridgingio-app-api` 与本地 control-plane 增加 timeline 来源分组摘要字段，并在 bootstrap / timeline 读取路径中返回
- [x] 2.2 为 model-plane 请求实现安全来源归因投影，优先使用 token label，缺失时回落到稳定请求指纹 / user-agent 摘要
- [x] 2.3 收敛 bundled 桌面管理面对 model-plane 端口变更的语义，确保 host/port 保存后的受控重启不要求 UI 自己跳转管理地址
- [x] 2.4 为设置页补齐 core 监听参数、日志大小、缓存大小、token 摘要与 vault 状态所需的 control-plane 读取 / 更新接口

## 3. Bundled Webview 页面结构

- [x] 3.1 实现首次启动引导页，覆盖目录用途说明、目录选择、可写性校验与恢复路径，并确保未完成配置前不存在进入主工作台的取消路径
- [x] 3.2 实现新的主工作台外壳，提供统一状态栏和 `Targets`、`Timeline`、`Settings` 三个一级入口
- [x] 3.3 实现独立的 `Targets` 页面，覆盖 target 列表、详情与编辑流，而不是继续与 timeline 或 settings 混排
- [x] 3.4 实现 `Timeline` 页面中的“被审计使用方”选择侧栏，按 token label 或 HTTP 指纹 / user-agent 摘要组织来源
- [x] 3.5 实现选中使用方后的执行简述卡片列表，保持列表态摘要化并展示审批状态、target 与结果
- [x] 3.6 实现执行卡片点击后的独立详情界面，展示审批上下文、artifact 关系与按需 transcript 读取

## 4. Settings / Vault / Token 管理

- [x] 4.1 实现 `Settings` 页面中的 `Core Runtime`、`Storage & Cache`、`Tokens`、`Vault` 分节布局
- [x] 4.2 实现 token 安全摘要列表与 revoke 流，并在设置页中先于 vault 深层管理入口展示
- [x] 4.3 将 vault 解锁与长期 token 签发接入受信任宿主触发的本地用户验证流程
- [x] 4.4 按新的跨平台设计系统收敛视觉与交互，落实简约、可信、轻动效的设计原则并避免沿用当前 macOS 旧版结构

## 5. 验证、文档与迁移参考

- [x] 5.1 为 timeline 来源归因、model-plane 端口变更后的管理面连续性、token revoke 与 vault 状态读取补充 core / control-plane contract tests
- [x] 5.2 为跨平台 bundled GUI 补充自动化 UI Test，覆盖引导页、主工作台导航、timeline 分组、token revoke 与 vault 解锁入口
- [x] 5.3 更新跨平台 UI 测试合同、宿主 handoff 文档和设计资产索引，明确这套桌面控制台设计将作为后续 macOS 重构参考
