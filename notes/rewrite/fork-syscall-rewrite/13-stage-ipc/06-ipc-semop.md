# 06-ipc-semop: semop 原子性与等待队列

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 信号量（call_vec → `IPC_SEMOP`；主循环 SUSPEND 后由 check_set / 进程事件恢复）
> **源码**: `minix3/minix/servers/ipc/sem.c:6-14,163-250,294-429,654-786,866-888`
> **Rust 模块**: `sem/op.rs`、`sem/waiter.rs`
> **draft 素材**: 无（新建）

## 核心点

- `iproc[NR_PROCS]` 表：slot=`_ENDPOINT_P(endpt)`、**单挂起不变量**（`assert(ip_sem == NULL)`，A-2 typestate）、ip_endpt/ip_pid/ip_sops/ip_nsops/ip_blkop
- `do_semop`：nsops==0→OK、>SEMOPM→E2BIG、malloc+`sys_datacopy` 拷贝 sops、权限掩码（全 0 op→IPC_R 否则 IPC_W）、sem_num 越界→EFBIG、SEM_UNDO→EINVAL（A-7）、`try_semop`、SUSPEND→入队 + `inc_susp_count`、OK→`check_set`
- `try_semop` **原子性**：数组顺序乐观执行 + 失败回滚（SEMVMX 溢出→ERANGE、负操作不足→NOWAIT?EAGAIN:SUSPEND、0 操作非零→同）、成功更新 sempid/otime
- `check_set`：FIFO 重试循环（woken_up 驱动）、blkop 变更时 inc/dec_susp_count 调整
- `inc/dec_susp_count`：semzcnt（op==0）/semncnt（op≠0）计数
- `complete_semop`：出队 + dec + free sops + `send_reply`（EDONTREPLY 不回复）
- `sem_process_event`：PM 事件取消（EXIT→EDONTREPLY、SIGNAL→EINTR）

## 边界

- **前置依赖**: 05 + 04
- **不覆盖（移交）**: 表管理（05）、事件订阅机制（09）
