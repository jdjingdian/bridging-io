# operator-interface-matrix 规范

## 目的
待定 - 由归档变更 expand-core-test-matrix-and-interface-matrix-docs 创建。归档后请更新目的。
## 需求
### 需求:本地 operator surfaces 必须维护正式的接口矩阵文档
BridgingIO 必须维护一份正式的本地 operator interface matrix，用于记录暴露给 UI、TUI、standalone 管理面和受信任本地调用方的接口真相。该矩阵至少必须覆盖命令或接口名称、调用方范围、输入、输出、状态、错误码与 apply strategy。

#### 场景:新增本地 control-plane 命令
- **当** 团队新增或修改一个面向本地 operator surface 的 command、事件或设置写入接口
- **那么** 系统必须同步更新接口矩阵文档，记录该接口的输入输出合同、适用调用方和状态/错误语义，而不是只修改实现代码

#### 场景:接口行为涉及重启或受控未实现
- **当** 某个本地 operator 接口存在 `restart_required`、`not_ready` 或 `method_not_implemented` 等正式语义
- **那么** 接口矩阵必须明确记录这些状态与对应恢复动作，而不是仅靠 README 叙述或代码注释隐式表达

