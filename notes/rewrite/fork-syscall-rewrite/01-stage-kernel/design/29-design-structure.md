# 29-kernel-debug: 设计结构（Design Structure）

> **文档**: `29-kernel-debug.md`
> **状态**: v1 快照（2026-08-12）

---

## 知识点全集

### 1. 调试基础设施四类功能
1. **调度队列 sanity check**: `runqueues_ok_cpu` / `runqueues_ok_all` / `runqueues_ok`
2. **进程信息打印**: `print_proc` / `print_proc_depends` / `print_proc_recursive` / `printproc` / `printparam` / `namematch`
3. **IPC 统计**: `printstats` / `sortstats` / `statmsg`
4. **IPC hooks**: `hook_ipc_msgkcall` / `hook_ipc_msgkresult` / `hook_ipc_msgrecv` / `hook_ipc_msgsend` / `hook_ipc_clear`

### 2. 条件编译模型
- `CONFIG_SMP`: SMP 调度队列检查（`runqueues_ok_all`）
- `DEBUG_DUMPIPC` / `DEBUG_DUMPIPCF`: IPC 消息打印（`mtypename` / `printmsg`）
- `DEBUG_IPCSTATS`: IPC 统计（`messages[][]` 矩阵 + `winners[]` 排序）
- `DEBUG_IPC_HOOK`: IPC hook 回调（5 个 hook 函数）

### 3. 宏定义
- `MAX_LOOP` (debug.c:14): `NR_PROCS + NR_TASKS`，进程表遍历上限
- `IPCPROCS` (debug.c:428): `NR_PROCS+1`，IPC 统计矩阵维度
- `KERNELIPC` (debug.c:429): `NR_PROCS`，内核调用槽位
- `PRINTSLOTS` (debug.c:432): 20，统计排行榜大小

### 4. 64-bit 重写决策
- 全部 WONTFIX——调试专用功能
- 替代: `log` crate + `tracing` crate + `debug_assert!` + `Debug`/`Display` trait

---

## 设计决策预览

| ID | 决策 | 理由 |
|----|------|------|
| D1 | 调度队列 sanity check 不实现 | Rust 类型系统防止大部分错误；`debug_assert!` 替代 |
| D2 | 进程信息打印不实现 | `Debug`/`Display` trait 替代 |
| D3 | IPC 消息跟踪不实现 | `tracing` crate 替代 |
| D4 | IPC 统计不实现 | 外部工具（perf/flamegraph）替代 |
