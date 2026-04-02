## 1. 默认路径与配置解析

- [x] 1.1 统一 standalone 默认 runtime root 到用户目录 `.bridgingio`，并实现默认配置路径解析
- [x] 1.2 调整 `run/-d` 启动语义，使 `--config` 成为覆盖入口而非 standalone 唯一入口

## 2. 生命周期状态机

- [x] 2.1 在 core 中实现 runtime/config bootstrap 生命周期状态与恢复动作投影
- [x] 2.2 将 lifecycle 状态接入 control-plane、desktop host onboarding/recovery 和启动诊断

## 3. 收口与验证

- [x] 3.1 更新开发文档、runtime root 合同和 packaged host 目录恢复说明
- [x] 3.2 补充 standalone/default-path、权限失败、迁移/修复和 desktop onboarding 相关测试
