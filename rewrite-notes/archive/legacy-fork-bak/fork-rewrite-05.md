# 阶段 5：PM — do_fork完整状态机与跨服务协调实现
> **状态**: ❌ 待实现
> **硬件依赖**: IPC通信Mock
> **Mock说明**: 所有跨服务IPC通信使用Mock实现，仅实现状态机流转、失败回滚、异步通知逻辑
> **对应源码**: `minix/servers/pm/forkexit.c`, `minix/servers/pm/main.c`

---

## 一、任务清单
| # | 任务 | 依赖 | 目标文件 |
|---|------|------|---------|
| 5.1 | 实现 `vm_fork()` IPC 调用 (模拟) | 阶段 2 | `os/servers/pm/src/mproc/fork.rs` |
| 5.2 | 实现 vm_fork 失败时的回滚 | 5.1 | `os/servers/pm/src/mproc/fork.rs` |
| 5.3 | 实现 `tell_vfs()` 异步通知 (模拟) | 阶段 4 | `os/servers/pm/src/mproc/fork.rs` |
| 5.4 | 实现 `VFS_CALL` 标志管理 | 5.3 | `os/servers/pm/src/mproc/fork.rs` |
| 5.5 | 实现 `do_fork()` 完整状态机 | 5.1-5.4 | `os/servers/pm/src/mproc/fork.rs` |
| 5.6 | 实现 SUSPEND 返回机制 | 5.5 | `os/servers/pm/src/mproc/fork.rs` |
| 5.7 | 实现 `do_fork_reply()` VFS 回复处理 | 5.5 | `os/servers/pm/src/mproc/fork.rs` |
| 5.8 | 实现调度失败回滚 (exit_proc) | 5.7 | `os/servers/pm/src/mproc/fork.rs` |
| 5.9 | 编写 PM 状态机单元测试 | 5.5 | `os/servers/pm/src/mproc/fork.rs` |

---

## 二、核心实现模板
### 2.1 `do_fork()` 完整状态机
```rust
impl<'a> PmContext<'a> {
    pub fn do_fork(&mut self) -> Result<i32, ForkError> {
        // 前置检查与槽位分配（已完成）
        let prepare = self.do_fork_prepare()?;

        // 调用VM fork，失败时直接释放槽位回滚
        let vm_result = self.call_vm_fork(
            self.current_proc().identity.endpoint,
            prepare.child_index,
        ).map_err(|e| {
            self.table.release_slot(prepare.child_index);
            e
        })?;

        // VM fork成功后不可失败，必须完成后续流程
        self.fork_child_from_parent(
            prepare.child_index,
            prepare.child_pid,
            vm_result.child_endpoint,
        );

        // 异步通知VFS
        self.tell_vfs(VfsPmForkRequest {
            child_endpoint: vm_result.child_endpoint,
            parent_endpoint: self.current_proc().identity.endpoint,
            child_pid: prepare.child_pid,
            real_uid: -1,
            real_gid: -1,
        });

        // 标记VFS_CALL标志，等待回复
        self.current_proc_mut().resources.flags |= ProcessFlags::VFS_CALL;

        // 有追踪器时发送SIGSTOP
        if self.current_proc().state.trace.is_traced() {
            sig_proc(&self.table[prepare.child_index], Signal::SIGSTOP, true, false);
        }

        // 返回SUSPEND，父进程挂起等待VFS回复
        Ok(SYS_SUSPEND)
    }
}
```

### 2.2 `do_fork_reply()` VFS回复处理
```rust
impl<'a> PmContext<'a> {
    pub fn do_fork_reply(&mut self, child_endpoint: Endpoint) -> Result<(), ForkError> {
        let child_slot = endpoint_slot(child_endpoint);
        let child = &mut self.table[child_slot];

        // 清除VFS_CALL标志
        child.resources.flags &= !ProcessFlags::VFS_CALL;

        // 尝试调度子进程
        match sched_start_user(child) {
            Ok(_) => {
                // 调度成功：回复子进程OK，回复父进程子PID
                reply(child_endpoint, SYS_OK);
                reply(child.state.guardianship.parent, child.identity.id.pid as i32);
            }
            Err(_) => {
                // 调度失败：销毁子进程，回复父进程错误
                exit_proc(child, ExitCode::from(-1));
                reply(child.state.guardianship.parent, -1);
            }
        }

        Ok(())
    }
}
```

---

## 三、必须通过的单元测试
```
test_do_fork_full_flow()
test_do_fork_vm_fork_failure_rollback()
test_do_fork_returns_suspend()
test_do_fork_reply_wakes_parent()
test_do_fork_scheduling_failure_rollback()
```

---

## 四、检查清单
| # | 逻辑点 | 状态 |
|---|--------|------|
| P-16 | 容量检查 `procs_in_use == NR_PROCS → EAGAIN` | ❌ |
| P-19 | `vm_fork()` IPC 调用实现 | ❌ |
| P-20 | vm_fork失败无状态无需回滚 | ❌ |
| P-21 | vm_fork成功后不可失败约束 | ❌ |
| P-26 | Tracer处理 TO_TRACEFORK 检查 | ❌ |
| P-30 | `tell_vfs()` 异步通知实现 | ❌ |
| P-31 | `VFS_CALL` 标志管理 | ❌ |
| P-32 | Tracer SIGSTOP 通知 | ❌ |
| P-33 | SUSPEND 返回机制 | ❌ |
| P-34 | `do_fork_reply()` VFS回复处理 | ❌ |
| P-35 | 调度失败回滚 exit_proc | ❌ |
| P-36 | NEW_PARENT 竞态处理 | ❌ |
| M-01 | PM→VM VM_FORK 消息协议 | ❌ |
| M-03 | PM→VFS VFS_PM_FORK 消息协议 | ❌ |
| M-04 | VFS→PM VFS_PM_FORK_REPLY 协议 | ❌ |
| M-06 | asynsend3 异步发送实现 | ❌ |
| M-07 | SUSPEND 返回内核实现 | ❌ |