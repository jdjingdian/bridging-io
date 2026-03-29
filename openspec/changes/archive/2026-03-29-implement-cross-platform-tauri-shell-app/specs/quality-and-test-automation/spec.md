## 新增需求

### 需求:桌面宿主项目必须具备可执行的启动 smoke 验证
跨平台桌面控制台的宿主项目必须提供可执行的启动 smoke 验证，用于证明仓库中的 Tauri 宿主、runtime root 分流、sidecar 启动配置与本地桥接链路能够协同工作。项目禁止仅依赖静态 HTML contract tests 或设计文档来宣称 bundled GUI 已可启动。

#### 场景:验证受支持平台上的桌面宿主启动
- **当** 团队在受支持宿主平台上执行桌面宿主 smoke 验证
- **那么** 验证流程必须至少确认 Tauri 宿主可启动、主窗口可装载 bundled 页面、runtime root 分流可决策，以及 managed core sidecar 启动参数与 attach/bootstrap 链路可被执行或被可信桩验证，而不是只检查 HTML 文件存在

#### 场景:宿主平台缺失本地 transport 时受控失败
- **当** 团队在尚未补齐平台原生 local transport 的宿主平台上执行桌面宿主 smoke 验证
- **那么** 验证必须返回明确的 unsupported、deferred 或等价受控诊断，指出阻塞点位于本地 control-plane transport，而不是把宿主启动失败误报为页面或配置层问题，更不能把该平台标记为“已通过跨平台启动验收”

## 修改需求

## 移除需求
