# 20: misc-queries

> **状态**: pending（最小骨架，待改写）
> **定位**: 阶段 10 时间与系统信息
> **源码**: minix3/minix/servers/pm/misc.c（72/108/149/169/199/291/401）、minix3/minix/servers/pm/profile.c:do_sprofile(22)、minix3/minix/servers/pm/mcontext.c（13/23）、minix3/minix/servers/pm/utility.c:find_param(57)
> **Rust 模块**: misc/query 模块（未实现）
> **draft 素材**: 无

## 核心点

do_sysuname（uts_tbl）、do_getsysinfo（SI_PROC_TAB/SI_CALL_STATS + 权限）、do_getprocnr/do_getepinfo（RS 查询）、do_reboot（SIGKILL 广播 + VFS_PM_REBOOT）、do_svrctl（monitor params + find_param + 本地覆盖）、do_getrusage（sys_times + vm_getrusage + set_rusage_times）、do_sprofile（#if SPROFILE）、do_getmcontext/do_setmcontext

## 边界

- **前置依赖**: 03/04
- **不覆盖（移交）**: calls_stats（cfg feature，文档化）、getrusage 的子时间累计（10）、readclock 通知细节（00-master-plan）
