## 新增需求

### 需求:本地 operator interface matrix 必须记录 SSH 认证设置与 SSH 安全访问合同
当 `menuconfig` 为 SSH target 引入前置认证设置、`SSH Authentication --->` 入口与 `SSH 安全访问` 语义时，本地 operator interface matrix 必须正式记录这些动作的入口位置、输入、输出、状态门控与 display-safe 约束，而不是把它们继续留作实现细节。

#### 场景:矩阵记录 SSH 创建流程中的认证设置步骤
- **当** 操作员在 `Add Target` 中创建一个 SSH target
- **那么** 接口矩阵必须明确记录流程顺序至少包括 `Choose Storage Mode`、`Choose Target Type`、`SSH Authentication Setup` 与 detail editor
- **并且** 必须明确记录 SSH 认证类型选择发生在连接字段编辑之前

#### 场景:矩阵记录 SSH 认证类型切换与进入键位
- **当** 操作员在 `SSH Authentication Setup` 中操作 `none` / `password` / `private-key`
- **那么** 接口矩阵必须明确记录 `Space` 用于切换单选项，`Enter` 用于进入详情编辑
- **并且** 必须明确记录 `继续进入详情编辑` 在认证前置条件未满足时呈现禁用态

#### 场景:矩阵记录 plain SSH 的 SSH 安全访问 toggle
- **当** plain SSH target 选择 `password` 或本地未加密私钥
- **那么** 接口矩阵必须明确记录 `SSH 安全访问` 默认开启且允许关闭
- **并且** 必须明确记录关闭后仍禁止通过 cmdline 传递 password

#### 场景:矩阵记录 sealed SSH 的强制安全访问
- **当** sealed SSH target 选择任何 secret-backed 认证方式
- **那么** 接口矩阵必须明确记录 `SSH 安全访问` 为强制开启且不可关闭
- **并且** 必须明确记录 locked 状态下相关设置入口受 vault 状态门控

#### 场景:矩阵记录 plain private-key 来源约束
- **当** plain SSH target 选择 `private-key` 认证
- **那么** 接口矩阵必须明确记录来源仅允许本地 key 路径
- **并且** 不得把 vault key 来源入口展示给 plain 路径

#### 场景:矩阵记录本地带 passphrase 私钥的阻断语义
- **当** 操作员在 SSH 认证设置中输入一个带 passphrase 的本地私钥路径
- **那么** 接口矩阵必须明确记录系统返回阻断性错误提示
- **并且** 必须明确记录该提示会引导用户改走 sealed + vault import 路径

## 修改需求

## 移除需求
