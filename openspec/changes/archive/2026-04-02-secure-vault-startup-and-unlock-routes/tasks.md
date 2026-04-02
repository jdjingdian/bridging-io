## 1. 解锁合同与策略

- [x] 1.1 定义共享的 startup/unlock carrier 抽象，并冻结正式允许的启动解锁载体集合
- [x] 1.2 为 standalone detached 模式实现 `trigger_policy -> on-core-start` 的安全 override 规则

## 2. 载体实现与接线

- [x] 2.1 实现 standalone 前台隐藏输入 prompt 与后台父进程一次性 carrier 的接入路径
- [x] 2.2 将 trusted host/local verification 与 standalone unlock 统一接入同一 unlock handler，并保持 fail-closed

## 3. 验证与手册

- [x] 3.1 补充前台/后台 startup unlock、carrier 缺失和 fail-closed 相关测试
- [x] 3.2 更新 vault operator guide、standalone 手册与安全启动说明，明确后台模式 override 语义
