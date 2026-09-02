# 05-ipc-sem-table: 信号量集合表与生命周期

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 3 信号量（call_vec → `IPC_SEMGET`/`IPC_SEMCTL`）
> **源码**: `minix3/minix/servers/ipc/sem.c:39-46,53-162,251-293,430-468,469-653,787-865`
> **Rust 模块**: `sem/table.rs`、`sem/ctl.rs`
> **draft 素材**: 无（新建）

## 核心点

- `sem_list[SEMMNI]`（SEMMNI=10）+ `sem_list_nr`（最高在用槽+1）、`SEM_ALLOC` 位、`_seq` 递增（`(seq+1) & 0x7fff`）
- `sem_find_key`（排除 `IPC_PRIVATE`）/ `sem_find_id`（`IPCID_TO_IX/SEQ` 校验）
- `do_semget`：已存在（CREAT+EXCL→EEXIST、check_perm、nsems 上限校验）vs 新建（无 CREAT→ENOENT、nsems 0..SEMMSL、空槽 ENOSPC、uid/gid/mode/ctime、**首集 → `update_sem_sub(TRUE)`（09）**）、retid=`IXSEQ_TO_IPCID`
- `do_semctl` 全 cmd：IPC_INFO/SEM_INFO（fill_seminfo + ret=最高槽）、SEM_STAT（槽索引）、IPC_STAT（`sys_datacopy`）、IPC_SET、IPC_RMID（remove_set）、GETVAL/GETPID/GETNCNT/GETZCNT（num 越界 EINVAL）、GETALL/SETALL（valbuf、SETALL SEMVMX 检查 ERANGE + check_set）、SETVAL（0..SEMVMX + check_set）
- `remove_set`：EIDRM 唤醒全部等待者（→06 `complete_semop`）、清 SEM_ALLOC、收缩 sem_list_nr、末集 → `update_sem_sub(FALSE)`
- `fill_seminfo`：IPC_INFO vs SEM_INFO 差异（semusz=sem_list_nr / semaem=总 nsems）
- `get_sem_mib_info`：sysvipc_info 输出契约（seminfo + semids 数组，ipcs(1) 依赖 semmni 全量数组）
- `is_sem_nil`；SEM_UNDO 契约（A-7，EINVAL）

## 边界

- **前置依赖**: 02/04 + 03（info 面）
- **不覆盖（移交）**: semop 原子性与等待队列（06）、订阅生命周期（09）
