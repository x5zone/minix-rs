# 10-pm-protocol: PM 协议（fork 次主线）

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 4 — PM 协议：VFS 的非 syscall 入口
> **源码**: `main.c:764-920`（service_pm）、`main.c:668-763`（service_pm_postponed）、`misc.c:577-1010`（pm_*/free_proc）、`com.h:513-544`（VFS_PM_*）
> **Rust 模块**: `os/servers/vfs/src/main_loop.rs`（PmMessageType）、`os/servers/vfs/src/ipc/dispatcher.rs`
> **draft 素材**: `draft/11-service-pm.md` + `draft/12~16-pm-fork-*.md` + `draft/19-pm-exit.md` + `draft/pm-fork-impl.md`（素材）

## 核心点

- service_pm 全 12 个 VFS_PM 请求：INIT/SETUID/SETGID/SETSID/EXIT/DUMPCORE/EXEC/FORK/SRV_FORK/UNPAUSE/REBOOT/SETGROUPS
- fork 次主线路径图：fproc 复制 → filp_count++ → dup_vnode → flags 重置 → VFS_PM_FORK_REPLY（plan §1.3）
- pm_fork（misc.c:577）：槽位锁保留、FD 共享、目录引用、SRV_FORK 附 uid/gid
- pm_exit/free_proc（misc.c:713/639）：close_fd 全关、put_vnode、unsuspend/dmap_unmap/smap_unmap/vmnt_unmap、SESLDR tty revoke
- pm_setuid/pm_setgid/pm_setgroups/pm_setsid（凭证注入）、pm_exec 转发、pm_dumpcore（调用点归 26）、pm_reboot
- service_pm_postponed：PM_WORK 延迟执行与 worker_start 关联

## 边界

- syscall 服务面不覆盖（14~31）
- FS 通信协议不覆盖（11/12）
- VFS_PM_* 发送方状态机在 PM 侧（`../04-stage-pm/05-vfs-interaction.md`）
