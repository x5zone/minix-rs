# 07-ipc-shm-segment: 共享内存段创建

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 4 共享内存（call_vec → `IPC_SHMGET`）
> **源码**: `minix3/minix/servers/ipc/shm.c:6-12,15-49,51-129`
> **Rust 模块**: `shm/segment.rs`
> **draft 素材**: 无（新建）

## 核心点

- `shm_list[SHMMNI]`（SHMMNI=1024）+ `shm_list_nr` + `SHM_ALLOC=0x0800` 私有标志 + `_seq` 递增
- `shm_find_key`（`IPC_PRIVATE` → NULL）/ `shm_find_id`
- `do_shmget`：已存在（check_perm、CREAT+EXCL→EEXIST、size>segsz→EINVAL）vs 新建（无 CREAT→ENOENT、size<=0→EINVAL、`roundup(size, PAGE_SIZE)`、空槽 ENOSPC、`mmap(MAP_ANON)+memset(0)`、uid/gid/mode/ctime/cpid、**`vm_getphys(sef_self(), page)` 存 vm_id**、retid）
- `shm_segsz` 存原始 size（未 roundup）
- mmap 内存面替换（A-4）+ VM 服务契约（A-9：VM_MMAP/VM_GETPHYS 对齐 02-stage-vm）

## 边界

- **前置依赖**: 02/04
- **不覆盖（移交）**: 挂接与引用计数（08）、权限掩码细节（04）
