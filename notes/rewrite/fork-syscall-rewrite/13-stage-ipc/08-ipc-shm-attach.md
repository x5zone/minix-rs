# 08-ipc-shm-attach: 挂接与引用计数

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 4 共享内存（call_vec → `IPC_SHMAT`/`IPC_SHMDT`/`IPC_SHMCTL`；主循环每循环收尾）
> **源码**: `minix3/minix/servers/ipc/shm.c:130-247,248-260,261-378,379-447,465-469`
> **Rust 模块**: `shm/attach.rs`、`shm/refcount.rs`
> **draft 素材**: 无（新建）

## 核心点

- `do_shmat`：addr 页对齐（`SHM_RND` 向下取整否则 EINVAL）、`shm_find_id`、权限（SHM_RDONLY→IPC_R 否则 IPC_R|IPC_W）、**`vm_remap(m_source, sef_self(), addr, page, segsz)`**、retaddr、atime/lpid（nattch 惰性）
- `update_refcount_and_destroy`：**全表轮询 `vm_getrefcount`** → `nattch = rc-1`、`SHM_DEST` 且 nattch==0 → `munmap` + 清 SHM_ALLOC、收缩 shm_list_nr（**A-3 决策点**：显式计数 vs lazy 轮询）
- `do_shmdt`：`vm_getphys(m_source, addr)` 定位、vm_id 匹配、`vm_unmap(m_source, addr)`、update_refcount_and_destroy
- `do_shmctl`：IPC_STAT/SHM_STAT 先 update_refcount；IPC_SET/IPC_RMID（owner EPERM，RMID → `SHM_DEST` 置位 + update_refcount_and_destroy 延迟销毁）；IPC_INFO（fill_shminfo）；SHM_INFO（shm_info：used_ids/shm_tot/shm_rss/shm_swp/swap_*）
- `fill_shminfo`：shmmax=-1/shmmin=1/shmmni=SHMMNI/shmseg=-1/shmall=-1
- `get_shm_mib_info`：sysvipc_info 输出契约
- `is_shm_nil`；VM 特权面（A-9：REMAP/SHM_UNMAP/GETPHYS/GETREF）

## 边界

- **前置依赖**: 07 + 04
- **不覆盖（移交）**: 段创建（07）、权限模型（04）
