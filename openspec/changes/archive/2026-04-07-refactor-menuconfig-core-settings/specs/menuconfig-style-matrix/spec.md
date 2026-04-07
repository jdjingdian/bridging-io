## 新增需求

### 需求:风格矩阵必须记录 Core Settings 主入口与二级子菜单拓扑
`MENUCONFIG_STYLE_MATRIX` 必须正式记录 `Core Settings` 作为基础运行配置的唯一一级入口，以及其下 `Cache Settings`、`HTTP Interface Settings` 二级子菜单的行语法、导航关系与字段映射。风格矩阵不得继续把 `Storage` 与 `Model Plane` 当作长期并列的一级菜单真相。

#### 场景:矩阵记录主菜单导航入口
- **当** 风格矩阵记录 `menuconfig` 主菜单的导航项
- **那么** 矩阵必须把基础运行配置入口记录为单一的 `Core Settings` 或等价本地化标题
- **并且** 不得继续把 `Core`、`Storage` 与 `Model Plane` 三项并列记录为当前正式一级入口

#### 场景:矩阵记录 Core Settings 子菜单映射
- **当** 风格矩阵记录 `Core Settings` 页面中的导航项与字段项
- **那么** 矩阵必须明确 `Cache Settings --->` 与 `HTTP Interface Settings --->` 属于 `action-row` 或等价导航模板
- **并且** 必须明确缓存与 HTTP 字段分别映射到对应二级子菜单，而不是继续记在独立一级页面下

### 需求:风格矩阵必须区分 canonical value 与 localized display value
当 `menuconfig` 某个字段使用 choice popup 或行内值显示时，风格矩阵必须明确记录“持久化值”和“显示值”可以不同，并为这类字段记录 operator-facing display label 的合同，而不是默认界面直接展示 canonical 枚举。

#### 场景:矩阵记录缓存 backend 的显示合同
- **当** 风格矩阵记录 `storage.artifacts.backend` 的行语法与 choice popup
- **那么** 矩阵必须明确该字段持久化值仍为 `memory` / `filesystem`
- **并且** 必须明确 operator-facing 显示值可以为 `Memory` / `Filesystem` 或其本地化等价文案

#### 场景:矩阵记录字段行与 choice popup 的一致显示
- **当** 风格矩阵记录一个带 choice popup 的枚举字段
- **那么** 矩阵必须要求字段行中的当前值显示与 choice popup 中的候选显示名保持同一套本地化规则
- **并且** 不得允许出现“字段行翻译了、popup 仍显示原始枚举值”的不一致状态

## 修改需求

无。

## 移除需求

无。
