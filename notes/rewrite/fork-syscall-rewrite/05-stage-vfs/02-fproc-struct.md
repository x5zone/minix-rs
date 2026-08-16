# 02-fproc-struct: fproc 结构与标志

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 1 — 进程模型：每进程文件系统上下文
> **源码**: `fproc.h`（struct fproc 全字段）
> **Rust 模块**: `os/servers/vfs/src/fproc.rs`（FProc/FpFlags/BlockedOn）
> **draft 素材**: `draft/01-fproc-struct.md` + `draft/02-fproc-flags.md` + `draft/03-fproc-cred.md` + `draft/fproc-design.md`（素材）

## 核心点

- 结构全字段：fp_flags/fp_pid/fp_endpoint/fp_wd/fp_rd/fp_filp[]/fp_cloexec_set/fp_tty/fp_blocked_on/fp_u/凭证/umask/fp_lock/fp_worker/fp_func/fp_msg/fp_pm_msg/fp_name
- fp_flags 6 位：SRV_PROC/REVIVED/SESLDR/PENDING/EXITING/PM_WORK
- fp_blocked_on + fp_u 五类阻塞 union：u_pipe/u_popen/u_flock/u_cdev/u_sdev
- 凭证字段：realuid/effuid/realgid/effgid/ngroups/sgroups/umask
- 槽位锁属于槽而非进程（fork 复制时保留 fp_lock）——ARCH A-6
- BlockedOn 类型化枚举（fproc.rs:49）——ARCH A-3

## 边界

- fproc 表管理/endpoint 验证不覆盖（03）
- 阻塞状态机的通用回复路径不覆盖（09/17/23）
