# 10-vm-syscalls: VM 内存 syscall 封装与客户端库

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 5 — syscall 封装（次主线·按服务分组）
> **源码**: `minix3/minix/lib/libc/sys/mmap.c`、`brk.c`、`sbrk.c`、`libsys/vm_fork.c`/`vm_exit.c`/`vm_map_phys.c`/`vm_info.c`/`vm_procctl.c`/`vm_cache.c`/`vm_getrusage.c`、`minix/include/minix/vm.h`
> **Rust 模块**: `os/libs/minix-sys`（vm 模块）、`os/libs/minix-types`（ipc/vm.rs）
> **draft 素材**: 无（新建）

## 核心点

- VM 调用（com.h VM_RQ_BASE+0~48）：MMAP/MUNMAP/BRK/MAP_PHYS/UNMAP_PHYS/REMAP/SHM_UNMAP/GETPHYS/GETREF/INFO/PROCCTL/VFS_MMAP/GETRUSAGE
- 用户 mmap/munmap（mmap.c:70-96）+ minix_mmap_for（MAP_THIRDPARTY）/minix_vfs_mmap（MVM_WRITABLE）
- VM 客户端库：vm_remap/vm_remap_ro/vm_unmap/vm_getphys/vm_getrefcount（mmap.c）+ vm_fork/vm_exit/vm_willexit/vm_map_phys/vm_unmap_phys/vm_info_*/vm_procctl_*/vm_cache 族/vm_getrusage（libsys）
- brk/sbrk 消息封装（06 的接口约定消费面）
- RS 交互 vm_* 边界声明（vm_set_priv/vm_update/vm_memctl/vm_prepare → 集成 stage）

## 边界

- **前置依赖**: 05
- **不覆盖（移交）**: VM 服务端实现（02-stage-vm）、RS 交互 vm_*（边界→集成）、分配策略（06）
