# 20-mib-minix-proc: MINIX_PROC 进程信息（ProcFS）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 10 进程信息（ProcFS 契约）
> **源码**: `proc.c:1177-1288` + `fs/procfs/tree.c`、`pid.c`
> **Rust 模块**: `proc/minix_proc.rs`
> **draft 素材**: 无（新建）

## 核心点

- `mib_minix_proc_list`（PROC_LIST）：全表 `minix_proc_list[NR_PROCS]`，IN_USE/ZOMBIE 标志 + pid/uid/gid；oldp==NULL 返回表大小
- `mib_minix_proc_data`（PROC_DATA）：单 PID；**负 PID=内核任务**（ProcFS 语义，与 KERN_LWP 的 -1 全列表不同）、PID 0 → ESRCH
- mpd_* 字段：endpoint/flags/blocked_on/priority/user_time/sys_time/cycles/kipc_cycles/kcall_cycles/nice/name
- MPDF_* 标志：SYSTEM/ZOMBIE/RUNNABLE/STOPPED
- A-5：`minix_proc_list`/`minix_proc_data` 布局 ABI（ProcFS 直接解释，`tree.c:87-92`/`pid.c:42-48,156,183`）

## 边界

- **前置依赖**: 16 + ProcFS
- **不覆盖（移交）**: 其他 PROC 接口（17~19）
