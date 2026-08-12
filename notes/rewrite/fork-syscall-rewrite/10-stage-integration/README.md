# Stage 6: 集成与协调

## 1. 阶段目标

将 PM → VM → Kernel → VFS → SCHED 五个服务的 fork 路径端到端连通，并搭建用户态 lib 层接口，使 fork 从用户态 `fork()` 调用到内核态完整闭环。

## 2. 文档索引

| 文档 | 说明 |
|------|------|
| [lib-syscall-iface.md](lib-syscall-iface.md) | 用户态 fork 系统调用接口（libc/libsys 层） |
| [cross-service-msg.md](cross-service-msg.md) | 跨服务消息协议与流转 |
| [state-machine.md](state-machine.md) | 状态机实现 |
| [error-rollback.md](error-rollback.md) | 错误回滚机制 |
| [suspend-wakeup.md](suspend-wakeup.md) | SUSPEND 与唤醒机制 |
| [integration-tests.md](integration-tests.md) | 集成测试 |

## 3. 跨服务协调

## 4. 状态机实现

## 5. 错误回滚

## 6. 集成测试

## 7. 性能验证
