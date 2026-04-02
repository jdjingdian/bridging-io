## 新增需求

### 需求:共享错误封装必须同时表达通用分类与模块专属语义
BridgingIO 必须定义一个面向公共调用面的结构化错误封装，使调用方能够同时读取通用错误分类与模块专属子码，而不是只能收到一段不稳定的 message 文本。该封装至少必须包含 `domain`、`common_code`、`module_code`、`message`、`retriable` 与 `recovery_hint` 等字段。

#### 场景:公共调用面返回结构化错误
- **当** 本地 control-plane、MCP 或 standalone CLI 因 vault、config、toolchain 或 runtime 生命周期问题返回错误
- **那么** 返回结果必须同时包含稳定的通用错误分类和模块专属子码，而不是只返回一段字符串化诊断

### 需求:共享状态集合必须包含 `method_not_implemented` 的正式语义
BridgingIO 必须在共享状态集合中正式定义 `method_not_implemented` 或等价 canonical 语义，并要求所有暂未实现但已保留正式调用面的能力返回该状态。系统禁止把“方法未实现”混同为 `unsupported`、`internal` 或普通字符串说明。

#### 场景:调用保留但尚未实现的能力
- **当** 调用方访问一个已经对外保留正式入口、但当前版本尚未实现的能力或方法
- **那么** 系统必须返回 `method_not_implemented` 对应的共享状态与错误分类，而不是把它伪装成平台不支持或内部异常

### 需求:公共错误详情必须保持 display-safe
共享错误封装中对外暴露的 message、recovery hint 与 details 必须保持 display-safe。系统禁止在公共错误对象中暴露 secret 明文、token 明文、ciphertext locator、完整 fd/pipe locator 或其他高敏内部字段。

#### 场景:安全模块返回失败
- **当** vault、token、secret broker 或本地验证路径返回错误
- **那么** 公共错误封装必须只返回 display-safe 的错误摘要与恢复提示，而不能直接回传高敏内部上下文
