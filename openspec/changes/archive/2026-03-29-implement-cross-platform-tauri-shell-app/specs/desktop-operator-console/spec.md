## 新增需求

### 需求:桌面控制台必须由仓库内可运行的 Tauri 项目交付
跨平台桌面控制台必须由仓库内正式维护的 Tauri 宿主项目交付，而不是仅由静态 bundled webview 页面、空目录骨架或合同测试代表“已实现”。该宿主项目必须能够在受支持宿主平台上完成窗口创建、页面装载、runtime root 启动分流、bundled core sidecar 启动与本地 control-plane attach。

#### 场景:仓库包含正式宿主工程
- **当** 团队交付跨平台桌面控制台实现
- **那么** 仓库中必须存在可构建、可运行的 Tauri 宿主项目结构，包括宿主入口、窗口配置、权限配置与 sidecar 打包配置，而不是只保留 `src-tauri` 空目录或文档占位

#### 场景:宿主启动后按正式链路进入工作台
- **当** 操作员在受支持宿主平台上启动桌面控制台
- **那么** 宿主必须先解析 runtime root 启动状态，再按需进入 onboarding 或 recovery，并且仅在 managed core sidecar 启动、UI attach 与 bootstrap 完成后进入主工作台，而不是直接加载一个与真实 core 生命周期脱钩的静态页面

### 需求:bundled webview 资产必须存在单一真相源
跨平台桌面控制台的 bundled webview 资产必须以单一真相源维护。系统禁止长期同时维护两份人工编辑的 `index.html`、`onboarding.html` 或等价静态资产并依赖比对测试保持一致。任何宿主内嵌副本都必须由单一来源生成或同步，而不是独立演化。

#### 场景:更新 onboarding 或 workspace 页面
- **当** 团队修改 onboarding、workspace 或 settings-vault 相关 bundled 页面
- **那么** 修改必须落在单一来源并通过生成、复制或打包链路进入宿主发行物，而不是要求开发者同时手工维护 UI 目录和宿主资产目录中的双份 HTML

#### 场景:宿主读取 bundled 页面
- **当** Tauri 宿主启动并装载 bundled webview
- **那么** 宿主必须读取由单一真相源产出的页面资产，使页面路由、bridge command 和 contract snippet 与源码保持一致，而不是依赖可能已经漂移的历史拷贝

## 修改需求

## 移除需求
