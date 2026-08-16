# 07: pm-fork

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 进程生命周期（fork 次主线）
> **源码**: minix3/minix/servers/pm/forkexit.c:do_fork(45)、minix3/minix/servers/pm/utility.c:get_free_pid(34)
> **Rust 模块**: fork.rs、mproc/fork.rs、mproc/pid_gen.rs、ipc/dispatcher.rs
> **draft 素材**: draft/do-fork-impl.md + draft/pid-generator.md + draft/pm-call-vm-fork.md（素材）

## 核心点

do_fork 全流程：procs_in_use/LAST_FEW 检查、next_child 槽位、vm_fork、*rmc=*rmp 复制 + sigact 重指、flags 继承（IN_USE|DELAY_CALL|TAINTED）、itimer/child 时间重置、get_free_pid、VFS_PM_FORK、tracer SIGSTOP、fork 次主线路径图

## 边界

- **前置依赖**: 03/04/05
- **不覆盖（移交）**: VM 侧地址空间复制（02-stage-vm/18-vm-fork.md）、VFS 侧 fd 复制（05-stage-vfs）、srv_fork（08）
