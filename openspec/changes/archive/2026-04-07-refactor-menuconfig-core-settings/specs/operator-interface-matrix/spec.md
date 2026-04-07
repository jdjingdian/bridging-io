## 新增需求

### 需求:本地 operator interface matrix 必须记录 Core Settings 的正式菜单拓扑与字段归属
本地 operator interface matrix 必须明确记录 `menuconfig` 中 core 基础运行配置的正式菜单拓扑，包括唯一一级入口 `Core Settings / 核心设置`、其下的 `Cache Settings / 缓存设置` 与 `HTTP Interface Settings / HTTP 接口设置` 二级入口，以及各字段在这些页面中的 operator-facing 展示位置。矩阵不得继续把 `Storage` 与 `Model Plane` 记录为当前正式一级入口。

#### 场景:矩阵记录缓存设置入口与字段
- **当** 接口矩阵记录 `menuconfig` 中与 `storage.artifacts.*` 相关的 operator surface
- **那么** 矩阵必须明确这些字段通过 `Core Settings -> Cache Settings` 暴露
- **并且** 必须记录至少包含缓存存储方式与缓存最大占用空间两个正式 operator-facing 字段

#### 场景:矩阵记录 HTTP 设置入口与字段
- **当** 接口矩阵记录 `menuconfig` 中与 `model_plane.http.*` 相关的 operator surface
- **那么** 矩阵必须明确这些字段通过 `Core Settings -> HTTP Interface Settings` 暴露
- **并且** 必须记录至少包含 HTTP 监听地址、HTTP 监听端口与 non-local binding 控制项

### 需求:本地 operator interface matrix 必须记录 canonical 配置值与本地化显示值的分离合同
当 `menuconfig` 通过 choice popup 或字段值展示枚举配置时，接口矩阵必须记录“界面显示值”与“配置持久化值”分离的正式合同，明确 operator surface 输出的是本地化 display label，而真正写回配置的仍然是 canonical 枚举值。

#### 场景:矩阵记录缓存 backend 的显示与写回语义
- **当** 接口矩阵记录 `Cache Settings` 中的缓存存储方式字段
- **那么** 矩阵必须明确界面可显示 `Filesystem / Memory` 或 `文件系统 / 运行内存`
- **并且** 必须明确配置写回仍使用 `filesystem` / `memory` canonical 值

#### 场景:矩阵记录中文 locale 下的 operator-facing 命名
- **当** 接口矩阵记录 `zh-CN` locale 下的 `Core Settings`、`Cache Settings` 与 `HTTP Interface Settings`
- **那么** 矩阵必须明确这些页面与字段使用 `核心设置`、`缓存设置`、`HTTP 接口设置`、`缓存最大占用空间`、`HTTP 监听地址`、`HTTP 监听端口` 等正式 operator-facing 文案
- **并且** 不得继续把 `Artifact Backend`、`Artifact Max Bytes`、`HTTP Host`、`HTTP Port` 记为中文界面的正式显示名

## 修改需求

无。

## 移除需求

无。
