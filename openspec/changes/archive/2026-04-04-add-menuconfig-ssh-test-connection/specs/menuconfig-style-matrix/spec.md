## 新增需求

### 需求:风格矩阵必须记录 SSH `Test Connection` 行与 popup 合同
当 `menuconfig` 为 SSH target 引入 `Test Connection --->` 动作时，风格矩阵必须明确记录该 action row 的位置、行语法、可聚焦性与 popup 链路，而不是只在实现中临时决定。

#### 场景:矩阵记录 SSH Test Connection 行语法
- **当** `menuconfig` 在 plain SSH 的 `Connection Profile` 或 unlocked sealed SSH 的 `Sensitive Overlay` 中暴露 `Test Connection --->`
- **那么** 风格矩阵必须明确记录该行使用与其他 action row 一致的 `--->` 语法
- **并且** 必须明确记录该行由 `Enter` 触发，而不是 `Space`

#### 场景:矩阵记录 timeout 输入弹窗
- **当** `menuconfig` 为 SSH 测试连接弹出 timeout 输入框
- **那么** 风格矩阵必须明确记录该弹窗属于 centered text-input popup
- **并且** 必须记录默认值为 `2000` 毫秒、`Enter` 确认开始测试、`Esc` 取消返回

#### 场景:矩阵记录 waiting 与 result 弹窗
- **当** `menuconfig` 进入 SSH 测试连接的等待态或结果态
- **那么** 风格矩阵必须明确记录 waiting popup 会拦截普通菜单导航
- **并且** 必须记录 waiting 态支持 `Esc` 取消
- **并且** 必须记录 result popup 只展示简洁状态，并由 `Enter` 或 `Esc` 关闭

## 修改需求

## 移除需求
