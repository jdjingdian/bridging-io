## 新增需求

### 需求:model-plane 请求必须生成可供管理面展示的安全来源摘要
系统必须为每个进入 model-plane 的请求生成可供本地受信任管理面展示的安全来源摘要，而不是只保留不可读的内部认证记录。若请求由带 label 的 token 认证，则该摘要必须优先使用 token label；若请求未携带 token，则摘要必须回落到稳定请求指纹、user-agent 摘要或等价来源信息。该摘要禁止包含明文 token、token hash 或其他认证内部真相。

#### 场景:带标签 token 的请求进入 model-plane
- **当** 某个请求通过带有 label 的 agent token 完成认证
- **那么** 系统必须为该请求生成可供管理面展示的来源摘要，并使该摘要优先体现 token label，而不是回显明文 token 或仅暴露内部 token_id/hash

#### 场景:未携带 token 的请求进入 model-plane
- **当** 某个请求未携带 token，或当前模式下不存在 token 认证信息
- **那么** 系统必须为该请求生成稳定的非认证来源摘要，例如请求指纹或 user-agent 摘要，而不是把它完全折叠成不可区分的匿名请求

## 修改需求

## 移除需求
