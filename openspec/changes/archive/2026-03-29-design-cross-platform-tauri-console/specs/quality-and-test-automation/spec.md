## 新增需求

### 需求:跨平台桌面控制台关键流必须具备自动化 UI 覆盖
跨平台桌面控制台的 bundled GUI 实现必须提供自动化 UI Test，覆盖首启引导、主工作台导航、timeline 来源分组、以及 settings / token / vault 等关键管理流。项目不得仅依赖现有 macOS UI Test 或手工验证来代表跨平台桌面控制台的验收。

#### 场景:验证首次启动目录选择流
- **当** 团队为跨平台桌面控制台交付首轮 bundled GUI 版本
- **那么** 自动化 UI Test 必须覆盖首次启动进入引导页、选择 runtime 数据目录、以及目录失效后的恢复路径，而不是只验证已连接主界面

#### 场景:验证 timeline 来源分组
- **当** 团队交付桌面控制台的 Timeline 页面
- **那么** 自动化 UI Test 必须验证“按 token label 分组”和“按请求指纹 / user-agent 摘要分组”这两类来源归因路径，而不是只检查时间线列表是否能渲染

#### 场景:验证 token revoke 与 vault 解锁入口
- **当** 团队交付桌面控制台的 Settings / Security 页面
- **那么** 自动化 UI Test 必须验证 token 摘要列表可见、revoke 可执行，以及 vault 解锁入口会进入受控本地验证流程，而不是只验证设置页静态表单存在

## 修改需求

## 移除需求
