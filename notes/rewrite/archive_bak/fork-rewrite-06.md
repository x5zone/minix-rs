# 阶段 6：全链路集成与验证测试
> **状态**: ❌ 待实现
> **硬件依赖**: 全栈Mock环境
> **Mock说明**: 使用统一的Mock层连接四个服务，模拟完整的跨服务交互流程

---

## 一、任务清单
| # | 任务 | 依赖 | 目标文件 |
|---|------|------|---------|
| 6.1 | 定义 `ForkCoordinator` — 全局 fork 协调器 | 阶段 2-5 | `os/servers/pm/src/mproc/coordinator.rs` |
| 6.2 | 实现 PM → VM IPC 消息发送 | 6.1 | `os/servers/pm/src/mproc/coordinator.rs` |
| 6.3 | 实现 VM → Kernel sys_fork 调用 | 6.1 | `os/servers/pm/src/mproc/coordinator.rs` |
| 6.4 | 实现 VM pt_bind 后清除 RTS_VMINHIBIT | 6.3 | `os/servers/pm/src/mproc/coordinator.rs` |
| 6.5 | 实现 PM → VFS 异步通知 | 6.1 | `os/servers/pm/src/mproc/coordinator.rs` |
| 6.6 | 实现 VFS 回复后唤醒父进程 | 6.5 | `os/servers/pm/src/mproc/coordinator.rs` |
| 6.7 | 编写集成测试: 四表 endpoint 一致性 | 6.1-6.6 | 集成测试文件 |
| 6.8 | 编写集成测试: CoW 验证 | 6.7 | 集成测试文件 |
| 6.9 | 编写集成测试: filp 引用计数验证 | 6.7 | 集成测试文件 |
| 6.10 | 编写集成测试: vnode 引用计数验证 | 6.7 | 集成测试文件 |
| 6.11 | 编写集成测试: ret_reg = 0 验证 | 6.7 | 集成测试文件 |
| 6.12 | 编写集成测试: endpoint generation 验证 | 6.7 | 集成测试文件 |

---

## 二、必须通过的集成测试
```
test_full_fork_flow()
test_cow_write_triggers_copy()
test_fork_close_does_not_close_file()
test_endpoint_generation_increment()
test_ret_reg_zero()
test_shared_memory_not_cow()
test_privileged_process_demotion()
test_four_table_endpoint_consistency()
test_pid_pm_vfs_consistency()
test_fork_table_full()
test_pid_allocation_no_conflict()
```

---

## 三、检查清单
| # | 逻辑点 | 状态 |
|---|--------|------|
| M-02 | VM→Kernel SYS_FORK 消息协议 | ❌ |
| M-05 | VM pt_bind 清除 RTS_VMINHIBIT | ❌ |
| T-01 | 四份进程表 endpoint 一致性 | ❌ |
| T-02 | PID 在 PM 和 VFS 中一致性 | ❌ |
| T-03 | 子进程初始不可运行 | ❌ |
| T-04 | CoW 写触发页面复制 | ❌ |
| T-05 | CoW refcount 从2降为1 | ❌ |
| T-06 | filp 引用计数验证 | ❌ |
| T-07 | vnode 引用计数验证 | ❌ |
| T-08 | ret_reg=0 子进程返回0 | ❌ |
| T-09 | endpoint generation 递增 | ❌ |
| T-10 | 共享内存段不触发 CoW | ❌ |
| T-11 | 特权进程子进程降级 | ❌ |
| T-12 | 进程表满返回 EAGAIN | ❌ |
| T-13 | PID 冲突检测 | ❌ |