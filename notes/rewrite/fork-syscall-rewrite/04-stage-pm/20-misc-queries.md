# 20 — 杂项与查询：`sysuname/getsysinfo/getprocnr/getepinfo/reboot/svrctl/getrusage/sprofile/mcontext` 的信息边界与控制面

本文讲清杂项如何在 PM 侧以“`uts_tbl[8]` 兼容间接 + `SI_PROC_TAB` 全表泄露的 `effuid==0` 门 + `endpoint↔pid` 双向解析 + `SIGKILL 广播→stop(INIT)→VFS_PM_REBOOT` 定序 + `local_overrides[2]` 覆盖层 + `sys_times/child_utime` 双源 `+ vm_getrusage` 三段”为完整链路，使 `uname(2)`/`sysinfo`/`getprocnr`/`getepinfo`/`reboot(2)`/`svrctl`/`getrusage(2)`/`sprofile`/`getmcontext` 的 10+ 杂项调用在 `SUPER_USER/RS` 双门与 `SUSPEND` 永不回复中可区分。

前置阅读：03-mproc-table.md（`find_proc/pm_isokendpt/NR_PROCS` 槽位与 `pid→slot` 扫描）、04-ipc-dispatch.md（`call_vec` 的 `PmCall` 分发与 `ReplyIntent::Reply/SUSPEND` 同步/永不回复边界）、01-pm-init-main.md（`monitor_params` 的 `KVP` 线性串与 `uts_val` 的 `OS_*` 单一真相）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `ProcTable` 的 `NR_PROCS 256` 固定表与 `find_proc` 的 `IN_USE` 扫描（03）、`call_vec[47]` 的 `PmCall` 枚举分发（04）、`monitor_params` 的 `MULTIBOOT_PARAM_BUF_SIZE` 链路（01）的开发者；知道 `utsname { sysname,nodename,release,version,machine }` 与 `SI_PROC_TAB` 全表拷贝的“信息泄露”语义。

> **本章不讲什么**：
> - `ENABLE_SYSCALL_STATS` 的 `calls_stats[NR_PM_CALLS]` 计数增量（`main.c:96` `calls_stats[call_nr]++` 的条件编译 `main.c:35`）—— 本章仅 `20` 的 `SI_CALL_STATS` 查询侧标注为 `cfg(feature="syscall_stats")` 的缺口契约（`A-7` sanity，`WONTFIX` 文档化）
> - `uts_val` 的 `COMPATIBILITY BLOCK` 已废弃兼容语义（`misc.c:31-44` 的 `i386/evbarm` 双 arch 分支在 64 位下 `x86_64` 重定义，`A-11` 64 位扩展）
> - `find_param` 的 `monitor_params` 的 `KVP` 线性扫描细节（`utility.c:57-71` 的 `NUL` 分隔遍历，`search_key[keylen-1]=0` 置零终结）
> - 内核 `sys_times/sys_stop/set_mcontext` 的 `p_user_time/p_sys_time` 落点（`01-stage-kernel/21-syscall-clock.md` 等）
> - VM 侧 `vm_getrusage` 的 `ru_maxrss/minflt/majflt` 填值（`02-stage-vm/20-vm-exit.md` 等）
>
> 本章只回答一个问题：**PM 如何为“`uts_tbl` 兼容间接 + `SI_PROC_TAB` 全表 `effuid==0` 门 + `endpoint↔pid` 双向解析 + `abort_flag→SIGKILL` 定序 + `local_overrides[2]` Tiny 覆盖表 + `ticks*1e6/hz` 分解”的 10+ 杂项调用建立 `SUPER_USER/RS` 双门与 `SUSPEND` 永不回复的控制面**。

### 1.1 为什么 `sysuname` 需要 `uts_tbl[8]` 兼容间接

`misc.c:32-60` 的兼容块是理解“PM 侧 `uname` 为何不是单 `utsname` 直拷”的关键：

```c
struct utsname uts_val = { OS_NAME, "noname", OS_RELEASE, OS_VERSION, "i386"/"evbarm" }; // 32-44  uts_val 单例（OS_* 单一真相）
static char *uts_tbl[] = { "i386", NULL, machine, NULL, nodename, release, version, sysname, NULL }; // 46-60  9 槽位 4 NULL 哨兵的 uts_tbl[8] 间接（__arraycount=8）
```

`uts_tbl` 的 9 槽位中 4 处 `NULL` 哨兵使“旧 `field` → 新 `uts_val.*`”的兼容映射在 `do_sysuname:79-83` 双 `EINVAL` 守卫中显式：`field >= __arraycount(uts_tbl) → EINVAL`（`79` 越界）+ `string==NULL → EINVAL`（`82-83` 哨兵，非支持 `field`）。`do_sysuname:88-92` 的 `len` 截断（`n=strlen+1; n>len?n=len`）+ `sys_datacopy(SELF, string, mp_endpoint, value, n)` 的 `SELF→endpoint` 拷贝使 `uname` 在 64 位下 `x86_64` 的 `machine` 重定义仅改 `UTS_VAL.machine` 的单一真相（`A-11`），`uts_tbl` 的 `i386/evbarm` 分支在 Rust 以 `UtsField::Machine → "x86_64"` 单一真相收敛。

### 1.2 为什么 `getsysinfo` 必须 `effuid==0` 且 `size` 精确匹配

`misc.c:116-143` 的 `do_getsysinfo` 是“PM 信息泄露”的守门人：

- `effuid!=0 → printf("PM: unauthorized call of do_getsysinfo by proc %d '%s'\n", mp_endpoint, mp_name) + sys_diagctl_stacktrace + EPERM`（`116-122` 带审计的 `Perm` 门，`who` 非 `RS` 但 `effuid` 的 `0` 门为未来“非系统进程拒绝”预留，注释 *leaks important information. In the future, requests from non-system processes should be denied*）。
- `what` 分派 `SI_PROC_TAB → src=mproc, len=sizeof mproc*NR_PROCS`（`125-128` 全表 `mproc` 起址 + `NR_PROCS=256` 的 `sizeof mproc` 线性长度，`NR_PROCS*size` 的 `304` 行 `mproc` 连续内存）与 `SI_CALL_STATS → calls_stats[NR_PM_CALLS]` 条件编译（`129-133` `ENABLE_SYSCALL_STATS` 的 `calls_stats` 缺省 `WONTFIX`，`20` 标注为 `cfg` 缺口）。
- `size != len → EINVAL`（`139-140` 精确匹配非截断，调用者需 `sizeof mproc*NR_PROCS` 精确，`E2BIG` 不适用，`size` 精确性与 `svrctl` 的 `E2BIG` 溢出守卫 `380-381` 对偶——`getsysinfo` 的精确非截断 vs `svrctl` 的溢出 `E2BIG`）。

Rust 以 `SysInfoWhat { ProcTab }` 枚举穷尽 `SI_PROC_TAB` 的 `0` 单一，`size != len → Inval` 的精确匹配在类型层显式，`effuid==0` 门复用 `15` 的 `is_superuser` 一处谓词。

### 1.3 为什么 `getprocnr/getepinfo` 是 `endpoint↔pid` 双向解析的 RS 专属与全量截断

`misc.c:154-193` 的双向 `RS↔PM` 解析在 `table.c` 的 `PM_GETPROCNR 46/GETEPINFO 45` 直达中完成：

- `do_getprocnr:154-157` `who_e != RS_PROC_NR → printf+EPERM` 的 RS 专属门（`RS` 进程 `endpoint 2` 的 `who_e` 比对，`RS` 专属 `getprocnr` 的 `find_proc(pid)→endpoint` 的 `IN_USE` 扫描，`ESRCH` 双路径 `find_proc` 的 `NULL→ESRCH`）。
- `do_getepinfo:176-192` `pm_isokendpt(endpt,&slot)→ESRCH` 的三守卫（`pm_isokendpt` 的 `_ENDPOINT_P→slot` 范围 + `endpoint==mproc[slot].endpoint` 一致 + `IN_USE` 存活）+ `uid/euid/gid/egid` 四参直拷（`180-183` `realuid/effuid/realgid/effgid`）+ `ngroups` 的 `min(rmp_ngroups, caller_ngroups)` 有界截断（`185-186` `if caller_ngroups < rmp_ngroups → ngroups = caller_ngroups` 的 `caller` 缓冲截断非 `EINVAL`，与 `getgroups` 的 `EINVAL` 对偶——`getepinfo` 的截断 vs `getgroups` 的精确）+ `ngroups>0→sys_datacopy` 的 `0` 守卫（`188-190` 空组零拷贝，`192` `return pid` 非 `OK` 的特殊回复载荷）。

Rust 以 `do_getprocnr(table, caller_ep: Endpoint, pid: Pid) -> Result<Endpoint, ProcError>` 的 `caller_ep != RS → Perm` + `EpInfo { pid, uid,euid,gid,egid,ngroups, groups:[Gid;16] }` 的 `return pid` 载荷显式，`ngroups` 截断在 `min` 一处。

### 1.4 为什么 `reboot` 必须 `SIGKILL 广播→stop(INIT)→VFS_PM_REBOOT` 定序

`misc.c:207-232` 的定序是“`reboot` 非原子但有序”的关键：

```c
abort_flag = how; // 207  RB_* 位寄存（RB_AUTOBOOT/RB_HALT/RB_POWERDOWN/RB_KEXEC 等，abort_flag 全局供后续 clock/reboot 路径）
if (RB_POWERDOWN) ds_retrieve_label_endpt("readclock.drv", &readclock_ep)==OK → _taskcall(RTCDEV_PWR_OFF); // 210-215  尝试非阻塞，失败不阻断（ARM 的 RTC 闹钟断电）
check_sig(-1, SIGKILL, FALSE); // 223  -1 广播 + SIGKILL 9 + FALSE 非 ksig（除 init 外全杀，check_sig 的 -1 广播在 signal.c:568 的 pid==-1 分支）
sys_stop(INIT_PROC_NR); // 224  stop 保留 init（proc 停止但不销毁，init 为 1 的特殊保留，A-12 监护）
memset(m,0); m_type=VFS_PM_REBOOT; tell_vfs(VFS, &m); // 227-230  VFS_CALL 置位
return SUSPEND; // 232  永不回复（SUSPEND 的第三子类：reboot 永不回复，caller 的 reply 由系统重启覆盖）
```

`abort_flag` 的全局寄存在 `reboot` 的 `SUSPEND` 永不回复中供 `clock` 的后续 `abort` 判断，`SIGKILL` 广播的 `-1` 在 Rust 以 `SignalCtl::broadcast_kill(-1)` 抽象，`SUSPEND` 永不回复在 Rust 以 `ReplyIntent::ReplyLater` 的 `reboot` 永不回复特例（`04` 的 `ReplyLater` 第二子类为“稍后回复”，`reboot` 为“永不回复”）。

### 1.5 为什么 `svrctl` 的 `local_param_overrides[2]` 是 `monitor_params` 的覆盖层

`misc.c:297-395` 的 2 槽位 tiny 表是“启动参数可运行时覆盖”的最小设计：

- `MAX_LOCAL_PARAMS 2` 的 `local_param_overrides[2] { name[30], value[30] }` 固定串（`299-301` `30` 边界与 `svrctl` 的 `PMSETPARAM` 双 30 边界 `328-334` 同 `keylen/vallen 30` 守卫，`ENOSPC` 上界 `327` `local_params>=2→ENOSPC` 的 tiny 上界）。
- `PMSETPARAM` 的双 `sys_datacopy`（`336-343` `keylen` + `vallen` 的 `30` 边界 `328-334` + 双拷贝 `sys_datacopy(who_e, key, SELF, local.name, keylen)` + `val` 同理 + `name[keylen]=0` 终结 `344-345`）。
- `PMGETPARAM` 的三级查找 `keylen==0 → monitor_params` 全表（`352-354` `val_start=monitor_params, val_len=sizeof monitor_params` 的全表拷贝）vs `search_key[keylen-1]=0` 置零（`367` `NUL` 终结，`keylen` 含 `NUL` 的外层 `sysgetenv.keylen` 在 `copy` 后 `search_key[keylen-1]=0` 置零）+ `local_overrides` 线性优先（`368-373` `strcmp` 线性 2 次）+ `find_param(search_key)` 回落（`374` `utility.c:57` 的 `monitor_params` 的 `NUL` 分隔 `KVP` 线性扫描）+ `ESRCH` 未找到（`375`）+ `val_len>vallen→E2BIG` 溢出（`380-381` `E2BIG` 非 `EINVAL` 的 `buffer too small` 语义）+ `val_len` 含 `NUL` 的 `strlen+1`（`376`）。

Rust 以 `ParamStore { local: ArrayVec<(String,String),2>, monitor: String }` 的 `ArrayVec` 固定容量 `2` 的 `ENOSPC` + `find_param(monitor: &str, key: &str) -> Option<&str>` 的 `split('\0')` 纯函数复用 `utility.c:57` 的 `KVP` 逻辑，`keylen==0→None` 的全表在 `GetParam(None)` 枚举穷尽。

### 1.6 为什么 `getrusage` 需 `sys_times/child_utime` 双源 + `set_rusage_times` + `vm_getrusage` 三段

`misc.c:407-447` 的三段是“内核 `p_utime/p_stime` + PM 累计 + VM 扩展”的分层：

- `who != RUSAGE_SELF(0) && !=CHILDREN(-1) → EINVAL`（`407-409` `RUSAGE_SELF 0` 的 `self` vs `CHILDREN -1` 的 `wait` 累计，`10` 的 `mp_child_utime/mp_child_stime` 与此对偶——`wait` 的 `rusage` 累计在 `exit` 的 `zombify` 后 `child_utime+= ...`）。
- `!children → sys_times(who_e,&utime,&stime)`（`429-431` `sys_times` 的 `p_user_time/p_sys_time` 内核落点，`OK` 否则透传）vs `children → utime=mp_child_utime/stime=mp_child_stime`（`433-434` `wait` 累计值，非 `sys_times`）。
- `set_rusage_times(&r_usage,utime,stime)` 的 `ticks*1e6/hz → sec/usec` 分解（`utility.c:149-155` `u64_t usec = ticks*1e6/hz; sec=usec/1e6; usec%=1e6` 的 `u64` 防溢出，`hz: Clock` 显式参，`A-11`）+ `vm_getrusage(who_e,&r_usage,children)` 的 `VM` 填 `ru_maxrss/minflt/majflt/...`（`441` `VM` 扩展，非 `PM` 侧 `ru_utime/stime`）。
- `sys_datacopy(SELF,&r_usage,who_e,addr,sizeof)` 的 `addr` 透传（`445-446` `m_lc_pm_rusage.addr` 的 `VirBytes` 直通）。

Rust 以 `RusageWho { Slf, Children }` 枚举分派 + `rusage_from_ticks(utime,stime,hz) -> (Timeval,Timeval)` 纯函数（`u64` 防溢出复用 `timer.rs:TicksConv` 的 `hz` 显式参），`TimesVmCtl::sys_times/vm_rusage` 双 trait 抽象。

### 1.7 与其他 OS 杂项的对照

Rust 改写不是照抄 `misc.c:79-83` 的 `__arraycount+NULL` 双守卫，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `uname(2)`/`sysinfo(2)`/`reboot(2)`/`sysctl(2)`/`getrusage(2)`。** Linux `uname` 的 `utsname { sysname[65], nodename[65], release[65], version[65], machine[65], domainname[65] }` + `sysinfo` 的 `SI_*` 全表 vs Minix3 `uts_tbl[8]` 的 4 `NULL` 兼容间接（`i386/evbarm` 双 arch 分支在 64 位下 `x86_64` 重定义）；Linux `sysinfo(2)` 的 `sysinfo { uptime, loads, totalram/ sharedram/ bufferram }` vs Minix3 `getsysinfo(SI_PROC_TAB)` 的 `mproc` 全表泄露（`effuid==0` 门在 Linux 以 `CAP_SYS_ADMIN` 的 `capable(CAP_SYS_ADMIN)` 抽象，`15` 的 `is_superuser` 同门）；Linux `reboot(2)` 的 `RB_*` + `SIGKILL` 广播 + `VFS_PM_REBOOT` 的 `broadcast+stop+reboot` 定序在 Linux 以 `kernel/reboot.c:orderly_reboot` 的 `kill_cad_pid` 广播 + `sys_reboot` 的 `reboot_mode` 定序；Linux `sysctl(2)` 的 `PMSETPARAM/GETPARAM` 的 `key→value` 在 Linux 以 `/proc/sys` 树替代 `monitor_params` 的 `KVP` 线性扫描；Linux `getrusage(RUSAGE_SELF/CHILDREN)` 的 `sys_times`/`child` 双源 + `ru_utime/stime` 的 `ticks*1e6/HZ` 分解与 `utility.c:149-155` 同算式。

**Redox `Scheme` 的 `sys:uname` + `sys:rs` 的 `Scheme` 显式。** Redox 以 `kernel/scheme: sys: Scheme { handle }` 的 `fd::open("sys:uname") → read(&mut [u8])` 的 `Scheme` 显式句柄（`Scheme::handle` 的 `open/read/write` 三元）与 `context::Context { uid, pid, groups }` 的 `RwLock` 快照，`reboot` 经 `kernel/scheme: write("reboot")` 的 `Scheme` 显式写（`kernel/syscall: sys_reboot` 的 `RebootScheme`），`monitor_params` 在 Redox 以 `BootScheme` 的 `BootArgs` 的 `next_boot_string` 显式参数解析（非 `monitor_params` 的 `NUL` 线性）。

**seL4 无 `getsysinfo` 的 `seL4_DebugDumpScheduler` 能力。** seL4 无 `getsysinfo(SI_PROC_TAB)` 的全表泄露，以 `seL4_DebugDumpScheduler`/`seL4_DebugDumpCNode` 的 `debug` 能力显式（`seL4_DebugPutChar` 的 `capability` 显式 vs Minix3 的 `effuid==0` 位门），`reboot` 以 `seL4_DebugHalt` 的 `halt` 能力——无 `MP` 全表泄露的世界与 Minix3 的 `effuid==0` 位门对偶，Rust 以 `is_superuser` 一处谓词在 `getsysinfo` 的 `Perm` 门收敛。

**结论（本章的设计基线）。** 把 C 的“`uts_tbl` 间接 + `__arraycount/NULL` 双守卫 + `SI_PROC_TAB` 全表 + `who_e==RS` 双门 + `ngroups` 有界截断 + `abort→power→kill→stop→reboot` 定序 + `local_overrides[2]` Tiny 表 + `keylen==0→全表` 三级 + `rusage` 双源三段 + `SPROFILE` 条件散落”改写为“`UtsField {SysName,Nodename,Release,Version,Machine}` + `SysInfoWhat::ProcTab` + `EpInfo {pid, groups:[Gid;16]}` + `RebootCtl::set_abort/power_off/broadcast_kill/stop_init/tell_reboot` + `ParamStore { local: ArrayVec<2>, monitor: &str }` + `RusageWho {Slf,Children} + rusage_from_ticks(utime,stime,hz)`”——与 Linux `uname/sysinfo/reboot/getrusage` 的 `utsname/SI_*` 同源，又因 PM 单线程无共享而以 `&mut ProcTable` 的 `CopyToUser/CopyGroups` 显式双向拷贝收敛。

### 1.8 小结

1. **为什么间接**——`uts_tbl` 兼容旧 `field` 的 `NULL` 哨兵 + `__arraycount` 越界双守卫。
2. **为什么 `effuid==0` + 精确匹配**——`SI_PROC_TAB` 全表泄露需 `Perm` 门 + `size==len` 精确非截断。
3. **为什么双向解析**——`getprocnr` 的 `RS` 专属 `pid→endpoint` 与 `getepinfo` 的 `pm_isokendpt→pid/groups` 双向对偶，`ngroups` 有界截断。
4. **为什么定序**——`abort→power→kill→stop→reboot→SUSPEND` 永不回复的 `reboot` 定序（`check_sig(-1)` 广播 + `sys_stop(INIT)` 保留）。
5. **为什么 Tiny 覆盖表**——`monitor_params` 的 `KVP` 线性 + `local_overrides[2]` 的 2 槽位覆盖（`ENOSPC` 上界 + `E2BIG` 溢出 + `ESRCH` 未找到 三码）。
6. **为什么三段**——`sys_times/child_utime` 双源 + `ticks*1e6/hz` 分解 + `vm_getrusage` 地址扩展的 `getrusage` 三段。

下一章逐行分析 C 的 `do_sysuname/do_getsysinfo/do_getprocnr/do_getepinfo/do_reboot/do_svrctl/do_getrusage/do_sprofile/do_getsetmcontext` 与 `find_param/set_rusage_times`；第 3 章给出 Rust 的 `UtsField/SysInfoWhat/EpInfo/RebootCtl/ParamStore/RusageWho`。

---

## 2 C 源码分析

### 2.1 `do_sysuname`（`misc.c:72-100`）

```c
int do_sysuname(void)
{ // 72  SYSUNAME 的 PM 侧入口（uts_tbl 间接 + len 截断）
  int r; size_t n; char *string; // 75-77  r/n/string
  if (m_in.m_lc_pm_sysuname.field >= __arraycount(uts_tbl)) return(EINVAL); // 79  field 越界→EINVAL（__arraycount=8）
  string = uts_tbl[m_in.m_lc_pm_sysuname.field]; // 81  间接取 string（uts_val.* 的间接指针）
  if (string == NULL) return EINVAL; // 82-83  NULL 哨兵→EINVAL（4 槽位 NULL 的非支持 field）
  switch (m_in.m_lc_pm_sysuname.req) { // 85  m_lc_pm_sysuname.req（ipc.h:469 `req`，0 为 Get）
  case 0: n = strlen(string) + 1; // 88  n=strlen+1（含 NUL，strlen 的 non-NUL 串长度）
	if (n > m_in.m_lc_pm_sysuname.len) n = m_in.m_lc_pm_sysuname.len; // 89  n>len?截断（调用者 len 非精确，需截断非 EINVAL）
	r = sys_datacopy(SELF, (vir_bytes)string, mp->mp_endpoint, m_in.m_lc_pm_sysuname.value, n); // 90-92  SELF→endpoint 拷贝（虚址 m_lc_pm_sysuname.value 的 VirBytes 直通）
	if (r < 0) return(r); // 92  EFAULT 等透传（<0 的负 errno）
	break; // 93
  default: return(EINVAL); // 96  req!=0→EINVAL（仅 0 的 Get 语义）
  }
  return(n); // 99  n 的字节数回复（非 OK 的正回复载荷，caller 的 return n）
}
```

`79` 行 `__arraycount` 越界与 `82-83` `NULL` 哨兵双 `EINVAL` 守卫使旧 `field` 映射在兼容层显式，`88-89` 的 `len` 截断非 `EINVAL`（`getsysinfo:139-140` 的 `size!=len→EINVAL` 精确非截断 vs `sysuname:89` 的 `n>len→n=len` 截断——`uname` 的 `value[65]` 固定 vs `getsysinfo` 的全表精确）。

### 2.2 `do_getsysinfo`（`misc.c:108-144`）

```c
int do_getsysinfo(void)
{ // 108  GETSYSINFO 的 PM 侧入口（全表泄露 + size 精确）
  vir_bytes src_addr, dst_addr; size_t len; // 110-111
  if (mp->mp_effuid != 0) { printf("PM: unauthorized call of do_getsysinfo by proc %d '%s'\n", mp->mp_endpoint, mp->mp_name); sys_diagctl_stacktrace(mp->mp_endpoint); return EPERM; } // 116-122  effuid!=0→printf+stacktrace+EPERM（审计）
  switch(m_in.m_lsys_getsysinfo.what) { // 124  m_lsys_getsysinfo.what（sysinfo.h: SI_PROC_TAB 0/SI_CALL_STATS 1）
  case SI_PROC_TAB: src_addr=(vir_bytes)mproc; len=sizeof mproc * NR_PROCS; break; // 125-128  SI_PROC_TAB→mproc 全表起址 + NR_PROCS*size 长度`
#if ENABLE_SYSCALL_STATS
  case SI_CALL_STATS: src_addr=(vir_bytes)calls_stats; len=sizeof calls_stats; break; // 130-133  SI_CALL_STATS 条件编译（ENABLE_SYSCALL_STATS 的 WONTFIX 缺口）
#endif
  default: return(EINVAL); // 135-136  what 非法→EINVAL
  }
  if (len != m_in.m_lsys_getsysinfo.size) return(EINVAL); // 139-140  size 精确非截断（调用者需 sizeof mproc*256 精确）
  dst_addr = m_in.m_lsys_getsysinfo.where; // 142  where 的 VirBytes 直通（m_lsys_getsysinfo.where 的 VirBytes）
  return sys_datacopy(SELF, src_addr, who_e, dst_addr, len); // 143  SELF→who_e 拷贝（全表泄露的 sys_datacopy 直通）
}
```

`116-122` 的 `effuid==0` 门带 `printf+stacktrace` 审计（`15` 的 `is_superuser` 一处谓词在 Rust 以 `log::warn` 替代 `printf`），`139-140` 的 `size` 精确非截断与 `81-83` 的 `uts_tbl` 双 `EINVAL` 同边界守卫（`getsysinfo` 的精确 vs `sysuname` 的截断对偶）。

### 2.3 `do_getprocnr`（`misc.c:149-164`）

```c
int do_getprocnr(void)
{ // 149  GETPROCNR 的 PM 侧入口（RS 专属 pid→endpoint）
  register struct mproc *rmp; // 151
  if (who_e != RS_PROC_NR) { printf("PM: unauthorized call of do_getprocnr by %d\n", who_e); return EPERM; } // 154-157  who_e != RS→printf+EPERM（RS 2 的 who_e 比对，RS 专属门）
  if ((rmp = find_proc(m_in.m_lsys_pm_getprocnr.pid)) == NULL) return(ESRCH); // 159-160  find_proc(pid)→NULL→ESRCH（IN_USE 扫描，utility.c:80-85 的 rmp_flags&IN_USE + pid 比对）
  mp->mp_reply.m_pm_lsys_getprocnr.endpt = rmp->mp_endpoint; // 162  reply.endpt = rmp_endpoint（m_pm_lsys_getprocnr.endpt 的 Endpoint 直通，callnr.h: 46 GETPROCNR）
  return(OK); // 163  OK 的同步回复（04 的 Reply）
}
```

`154-157` 的 `RS` 专属门与 `getepinfo:176-177` 的 `pm_isokendpt` 门对偶（`getprocnr` 的 `RS` 门 vs `getepinfo` 的 `pm_isokendpt` 三守卫），`159-160` 的 `find_proc` 双路径 `NULL→ESRCH` 与 `15` 的 `getsid` 同 `ESRCH` 初始。

### 2.4 `do_getepinfo`（`misc.c:169-193`）

```c
int do_getepinfo(void)
{ // 169  GETEPINFO 的 PM 侧入口（pm_isokendpt + ngroups 截断）
  struct mproc *rmp; endpoint_t ep; int r, slot, ngroups; // 171-173  temp
  ep = m_in.m_lsys_pm_getepinfo.endpt; // 175  m_lsys_pm_getepinfo.endpt（ipc.h:469 `endpt` 的 Endpoint）
  if (pm_isokendpt(ep, &slot) != OK) return(ESRCH); // 176-177  pm_isokendpt→ESRCH（utility.c:108-118 的 _ENDPOINT_P+IN_USE 三守卫）
  rmp = &mproc[slot]; // 178  rmp 的 slot 解引（mproc[slot] 非 find_proc 扫描）
  mp->mp_reply.m_pm_lsys_getepinfo.uid = rmp->mp_realuid; // 180  reply uid=realuid（mproc.h:40-46 real/eff 三元但本章仅 real 的 getepinfo 四参直拷，与 getset 的 getepinfo 同 uid/euid 四参）
  mp->mp_reply.m_pm_lsys_getepinfo.euid = rmp->mp_effuid; // 181
  mp->mp_reply.m_pm_lsys_getepinfo.gid = rmp->mp_realgid; // 182
  mp->mp_reply.m_pm_lsys_getepinfo.egid = rmp->mp_effgid; // 183
  mp->mp_reply.m_pm_lsys_getepinfo.ngroups = ngroups = rmp->mp_ngroups; // 184  ngroups 的 reply 载荷（caller_ngroups 的 reply 载荷非 ES2026 式）
  if (ngroups > m_in.m_lsys_pm_getepinfo.ngroups) ngroups = m_in.m_lsys_pm_getepinfo.ngroups; // 185-186  有界截断（caller 的 ngroups 上界，min(rmp,calls) 的截断非 EINVAL）
  if (ngroups > 0) { if ((r = sys_datacopy(SELF, (vir_bytes)rmp->mp_sgroups, who_e, m_in.m_lsys_pm_getepinfo.groups, ngroups * sizeof(gid_t))) != OK) return(r); } // 188-190  ngroups>0→sys_datacopy（SELF→who_e 的 groups 截断拷贝，0 组零拷贝）
  return(rmp->mp_pid); // 192  return pid（非 OK 的特殊回复载荷，caller 的 return pid 的 proc_nr 语义，D3）
}
```

`185-186` 的 `min` 截断非 `EINVAL`（`15` 的 `GETGROUPS` 的 `ngroups<avail→EINVAL` 精确 vs `getepinfo` 的 `caller_ngroups` 截断——`getgroups` 的缓冲区不足显式失败 vs `getepinfo` 的 `ngroups` 有界截断），`192` `return pid` 非 `OK` 的特殊回复在 Rust 以 `EpInfo { pid }` 的 `return pid` 载荷显式（`04` 的 `Reply(pid)` 载荷）。

### 2.5 `do_reboot`（`misc.c:198-233`）

```c
int do_reboot(void)
{ // 198  REBOOT 的 PM 侧入口（定序 + 永不回复）
  message m; // 201
  if (mp->mp_effuid != SUPER_USER) return(EPERM); // 204  SUPER_USER 门（effuid!=0→EPERM，15 同门）
  abort_flag = m_in.m_lc_pm_reboot.how; // 207  abort_flag = how（glo.h: abort_flag 全局，how 的 RB_* 位寄存，RB_AUTOBOOT/RB_HALT/RB_POWERDOWN/RB_KEXEC 等）
  if (abort_flag & RB_POWERDOWN) { endpoint_t readclock_ep; if (ds_retrieve_label_endpt("readclock.drv", &readclock_ep) == OK) { message m; _taskcall(readclock_ep, RTCDEV_PWR_OFF, &m); } } // 210-215  RB_POWERDOWN→readclock RTCDEV_PWR_OFF 尝试（ds_retrieve_label_endpt 的非阻塞，失败不阻断）
  check_sig(-1, SIGKILL, FALSE); // 223  -1 广播 + SIGKILL 9 + FALSE 非 ksig（signal.c:568 的 pid==-1 广播，除 init 外全杀，11 的 check_sig 四态）
  sys_stop(INIT_PROC_NR); // 224  stop 保留 init（kernel/system/do_stop.c: INIT_PROC_NR 1 的 stop 非销毁，A-12 监护）
  memset(&m, 0, sizeof(m)); m.m_type = VFS_PM_REBOOT; // 227-229  VFS_PM_REBOOT 的 m_type
  tell_vfs(&mproc[VFS_PROC_NR], &m); // 230  tell_vfs(VFS) 置 VFS_CALL（utility.c:131 的 VFS_CALL 置位 + asynsend3 非阻塞）
  return(SUSPEND); // 232  永不回复（SUSPEND 的第三子类：reboot 永不回复，caller 的 reply 由重启覆盖）
}
```

`207` 行 `abort_flag` 寄存与 `04` 的 `calls_stats` 同 `glo.h` 全局（`A-3` 全局→显式），`210-215` 的 `readclock` 尝试非阻塞（`OT`），`223` 的 `-1` 广播在 `signal.c:568` 的 `pid==-1` 广播分支（`11` 的 `SIGKILL` 广播），`232` `SUSPEND` 永不回复在 Rust 以 `ReplyIntent::ReplyLater` 的 `reboot` 永不回复特例（`04` 的 `SUSPEND` 第三子类）。

### 2.6 `do_svrctl`（`misc.c:291-395`）

```c
int do_svrctl(void)
{ // 291  SVRCTL 的 PM 侧入口（IOCGROUP 门 + local_overrides 覆盖）
  unsigned long req; int s; vir_bytes ptr; // 293-295
#define MAX_LOCAL_PARAMS 2
  static struct { char name[30]; char value[30]; } local_param_overrides[2]; // 298-300  2 槽位 30 边界固定串（tiny 覆盖表，A-3 静态全局在 PM 单线程无共享故 AssumeSyncCell 合理）
  static int local_params = 0; // 301  计数（单线程无并发，BKL 不适用）
  req = m_in.m_lc_svrctl.request; ptr = m_in.m_lc_svrctl.arg; // 303-304  m_lc_svrctl.request/arg（ipc.h:469 `request:unsigned long, arg: vir_bytes`）
  if (IOCGROUP(req) != 'P' && IOCGROUP(req) != 'M') return(EINVAL); // 307  IOCGROUP 门（'P'/'M' 双组，'M' 旧兼容在被移除前保留）
  switch(req) { // 310  switch req（OPMSETPARAM/OPMGETPARAM/PMSETPARAM/PMGETPARAM 四 req 同族，svrctl.h: PMSETPARAM 0x...）
  case OPMSETPARAM: case OPMGETPARAM: case PMSETPARAM: case PMGETPARAM: { // 311-314  四 req 同族（OPM* 的 O 旧兼容）
      struct sysgetenv sysgetenv; char search_key[64]; char *val_start; size_t val_len, copy_len; // 315-319  temp（search_key 64 边界 vs local 30，monitor_params 的 sizeof 链路在 15 的 param 验证同 30）
      if (sys_datacopy(who_e, ptr, SELF, (vir_bytes)&sysgetenv, sizeof(sysgetenv)) != OK) return(EFAULT); // 322-323  取外层 sysgetenv（who_e→SELF，VirBytes 的 ctx 直通）
      if (req == PMSETPARAM || req == OPMSETPARAM) { // 326  SET 路径（PMSETPARAM/OPMSETPARAM）
   	if (local_params >= MAX_LOCAL_PARAMS) return ENOSPC; // 327  ENOSPC 上界（2 槽位满→ENOSPC，非 EINVAL）
   	if (sysgetenv.keylen <=0 || sysgetenv.keylen >= sizeof local.name || sysgetenv.vallen <=0 || sysgetenv.vallen >= sizeof local.value) return EINVAL; // 328-334  30 边界（<=0||>=30→EINVAL，双 30 守卫）
           if ((s = sys_datacopy(who_e, sysgetenv.key, SELF, local.name, sysgetenv.keylen)) != OK) return s; // 336-338  key 拷贝（who_e→SELF）
           if ((s = sys_datacopy(who_e, sysgetenv.val, SELF, local.value, sysgetenv.vallen)) != OK) return s; // 340-342  val 拷贝（who_e→SELF）
             local.name[sysgetenv.keylen] = '\0'; local.value[sysgetenv.vallen] = '\0'; // 344-345  NUL 终结（keylen 含 NUL 的外层 sysgetenv.keylen 在 copy 后置零终结，344 的 name[keylen]=0 与 svrctl.c:367 的 search_key[keylen-1]=0 对偶——前者新 local 的终结，后者查找 key 的终结）
   	local_params++; return OK; // 347-349 计数+OK（SET 的同步回复，04 的 Reply）
       }
      if (sysgetenv.keylen == 0) { val_start = monitor_params; val_len = sizeof(monitor_params); } // 352-354  keylen==0→全表（Val 为 monitor_params 全表起址 + sizeof 全表长度，0 的特殊 keylen 语义，全表拷贝）
      else { // 356  key→value 三级查找（val_start 的三级）
       	  int p; if (sysgetenv.keylen > sizeof search_key) return(EINVAL); // 359  keylen 64 边界（search_key 64 上界，>64→EINVAL）
           if ((s = sys_datacopy(who_e, sysgetenv.key, SELF, search_key, sysgetenv.keylen)) != OK) return(s); // 360-361  key 拷贝（who_e→SELF）
           search_key[sysgetenv.keylen-1]= '\0'; // 367  NUL 置零（keylen 含 NUL 的外层 sysgetenv.keylen 在 copy 后 keylen-1 置零终结，与 SET 的 344-345 同置零但位置差一）
           for(p=0; p<local_params; p++) { if (!strcmp(search_key, local.name)) { val_start=local.value; break; } } // 368-373  local_overrides 线性优先（2 次 strcmp 线性，2 槽位 tiny 表的 O(2) 优先）
           if (p >= local_params && (val_start = find_param(search_key)) == NULL) return(ESRCH); // 374-375  find_param 回落→ESRCH（utility.c:57 的 KVP 线性扫描，NUL 分隔 KVP 的 fallback）
           val_len = strlen(val_start) + 1; // 376  val_len 含 NUL（strlen+1 的 NUL 含长度）
       }
      if (val_len > sysgetenv.vallen) return E2BIG; // 380-381  E2BIG 溢出（vallsen<string len+1 → E2BIG，非 EINVAL 的 buffer too small 语义，与 getsysinfo 的 EINVAL 精确对偶）
      copy_len = MIN(val_len, sysgetenv.vallen); // 384  copy_len 的 MIN（val_len>vallen 时 380 已 E2BIG，此处 copy_len==val_len 的冗余 MIN 但 C 保留）
      if ((s=sys_datacopy(SELF, val_start, who_e, sysgetenv.val, copy_len)) != OK) return(s); // 385-386  SELF→who_e 拷贝（val_start 的 KVP 值→调用者 buffer，VirBytes 的 val 直通）
      return OK; // 389  OK 的同步回复（GET 的同步回复）
  }
  default: return(EINVAL); // 392-393  req 非法→EINVAL（IOCGROUP 已守卫后 req 非四者→EINVAL）
  }
}
```

`297-301` 的 `local_param_overrides[2]` 静态全局在 PM 单线程无共享故 `AssumeSyncCell` 合理（`Execution Model` § 单线程，`UnsafeCell` 安全），`380-381` 的 `E2BIG` 溢出与 `getsysinfo:139-140` 的 `EINVAL` 精确对偶（`svrctl` 的 `buffer too small` 的 `E2BIG` vs `getsysinfo` 的 `size` 精确的 `EINVAL`）。

### 2.7 `do_getrusage`（`misc.c:400-447`）

```c
int do_getrusage(void)
{ // 400  GETRUSAGE 的 PM 侧入口（who 分派 + 双源三段）
	clock_t user_time, sys_time; struct rusage r_usage; int r, children; // 403-405  temp（user_time/sys_time 的 Clock 显式参在 10 的 wait 中共享 hz）
	if (m_in.m_lc_pm_rusage.who != RUSAGE_SELF && m_in.m_lc_pm_rusage.who != RUSAGE_CHILDREN) return EINVAL; // 407-409  who 边界（RUSAGE_SELF 0 的 self vs CHILDREN -1 的 children，who 的 int 边界在 0/-1 穷尽）
	memset(&r_usage, 0, sizeof(r_usage)); // 419  r_usage 清零（ru_maxrss/minflt 等初 0）
	children = (m_in.m_lc_pm_rusage.who == RUSAGE_CHILDREN); // 421  children 的 bool 化（who==CHILDREN→1 的 bool，仅 RUSAGE_CHILDREN 的 children）
	if (!children) { if ((r = sys_times(who_e, &user_time, &sys_time, NULL, NULL)) != OK) return r; } // 428-431  !children→sys_times(who_e,&utime,&stime) 的内核 p_user_time/p_sys_time 拉取（who_e 的 endpoint 直通，NULL 的 boot 忽略）
	else { user_time = mp->mp_child_utime; sys_time = mp->mp_child_stime; } // 433-434  children→utime=child_utime/stime=child_stime（10 的 wait 累计值，mp_child_* 的累计在 exit 的 zombify 后 child_utime+= ... 的累计对偶）
	set_rusage_times(&r_usage, user_time, sys_time); // 438  set_rusage_times 的 ticks*1e6/hz 分解（utility.c:149-155 的 u64 防溢出，ru_utime/stime 的 sec/usec 分解）
	if ((r = vm_getrusage(who_e, &r_usage, children)) != OK) return r; // 441  vm_getrusage 的 VM 扩展（who_e 的 endpoint + r_usage 的 ru_maxrss/minflt 等填值，children 的 bool 传 VM）
	return sys_datacopy(SELF, (vir_bytes)&r_usage, who_e, m_in.m_lc_pm_rusage.addr, sizeof(r_usage)); // 445-446  SELF→who_e 拷贝（addr 的 VirBytes 直通，sizeof 的精确 64B 的 ru 结构）
}
```

`407-409` 的 `RUSAGE_SELF 0` vs `CHILDREN -1` 的 `who` 边界在 Rust 以 `RusageWho::try_from(who: i32)` 的 `0→Self/-1→Children` 枚举穷尽（`RusageWho` 单一真源），`433-434` 的 `child_utime` 累计与 `10` 的 `wait` 累计对偶——`wait` 的 `cleanup` 后 `child_utime` 累计在 `exit` 的 `zombify` 后 `mp_child_*` 的累计已归 `10`。

### 2.8 `do_sprofile`（`profile.c:22-45`）

```c
int do_sprofile(void)
{ // 22  SPROFILE 的 PM 侧入口（条件缺口）
#if SPROFILE
  int r;
  switch(m_in.m_lc_pm_sprof.action) { // 28  m_lc_pm_sprof.action（ipc.h:469 `action` 的 PROF_START/STOP）
  case PROF_START: return sys_sprof(PROF_START, m_in.m_lc_pm_sprof.mem_size, m_in.m_lc_pm_sprof.freq, m_in.m_lc_pm_sprof.intr_type, who_e, m_in.m_lc_pm_sprof.ctl_ptr, m_in.m_lc_pm_sprof.mem_ptr); // 31-33  PROF_START→sys_sprof(START,...) 6 参透传（mem_size/freq/intr_type/ctl_ptr/mem_ptr 的 VirBytes 直通）
  case PROF_STOP: return sys_sprof(PROF_STOP,0,0,0,0,0,0); // 36  PROF_STOP→sys_sprof(STOP) 零参透传
  default: return EINVAL; // 39  action 非法→EINVAL
  }
#else
	return ENOSYS; // 43  非 SPROFILE→ENOSYS（默认 ENOSYS 的缺口契约，A-7 sanity 模式的 WONTFIX 文档化）
#endif
}
```

`24` 行 `#if SPROFILE` 的条件编译在 Rust 以 `cfg(feature="sprofile")` 的 `ENOSYS` 缺口契约（`00-master-plan` 的 `sprofile` 仅 `mib` 服务用，`PM` 侧 `20` 标注缺口，`profile.c:43` 默认 `ENOSYS` 为本文 `D7` 的 Rust 侧 `cfg(not(feature))` 对偶）。

### 2.9 `do_get/setmcontext`（`mcontext.c:13/23`）

```c
int do_setmcontext(void) { return sys_setmcontext(who_e, m_in.m_lc_pm_mcontext.ctx); } // 15  who_e,ctx 直通（m_lc_pm_mcontext.ctx 的 VirBytes 句柄，A-3 显式 caller）
int do_getmcontext(void) { return sys_getmcontext(who_e, m_in.m_lc_pm_mcontext.ctx); } // 25  同直通（VirBytes 句柄的 arch 相关 mcontext_t 不透明，A-11 64 位扩展）
```

`ctx` 的 `VirBytes` 句柄不透明（`arch` 的 `mcontext_t` 的 `VirBytes` 句柄直通，`A-11` 64 位扩展，移交 `01-stage-kernel/31-fpu-context-switching.md`），`who_e` 的 `endpoint` 显式参在 Rust 以 `McontextCtl` trait 注入。

### 2.10 `find_param`（`utility.c:57-71`）

```c
char *find_param(const char *name) { register const char *namep; register char *envp; for (envp = monitor_params; *envp != 0;) { for (namep=name; *namep!=0 && *namep==*envp; namep++, envp++); if (*namep=='\0' && *envp=='=') return(envp+1); while(*envp++!=0); } return(NULL); } // 57-71  monitor_params 的 NUL 分隔 KVP 线性扫描（*envp==0 终链的 NUL 终结 + name 与 envp 的字符逐比 + '=' 后值 + NUL 跳下一条）
```

`monitor_params` 的 `MULTIBOOT_PARAM_BUF_SIZE` 链路在 `01` 的 `sys_getmonparams` 已填表，`find_param` 的 `while(*envp++!=0);` 跳下一条与 `misc.c:368-373` 的 `local_overrides` 线性 2 次对偶（`KVP` 线性扫描的 `NUL` 分隔）。

### 2.11 `set_rusage_times`（`utility.c:144-156`）

```c
void set_rusage_times(struct rusage * r_usage, clock_t user_time, clock_t sys_time) { u64_t usec; usec = user_time * 1000000 / sys_hz(); r_usage->ru_utime.tv_sec = usec / 1000000; r_usage->ru_utime.tv_usec = usec % 1000000; usec = sys_time * 1000000 / sys_hz(); r_usage->ru_stime.tv_sec = usec / 1000000; r_usage->ru_stime.tv_usec = usec % 1000000; } // 144-156  ticks*1e6/hz → sec/usec 分解（user_time*1e6/hz 的 u64 显式防溢出，TicksConv 的 hz 显式参同 ticks 分解）
```

`user_time*1e6/hz` 的 `u64` 防溢出与 `timer.rs:TicksConv` 的 `ticks%hz*US/hz` 同 `hz` 显式参（`u64` 的 `*1e6` 在 `hz<=50000` 时 `ticks*1e6≈1e9*5e4=5e13<2^63`），`ru_utime.tv_sec=usec/1e6` 的 `1e6` 分解与 `time.c:44` 的 `1e9/hz` 同常数族（`US=1e6` vs `NSEC=1e9` 仅常数不同）。

### 2.12 消息与类型（`callnr.h: PM_SYSUNAME 25/.../SPROF 39` + `ipc.h: MessLcPmSysuname/MessLsysGetsysinfo/.../MessLcPmMcontext` + `sysinfo.h: SI_*` + `reboot.h: RB_*` + `svrctl.h: PMSETPARAM` + `resource.h: rusage`）

- `PM_SYSUNAME 25`（`callnr.h:25` `PM_BASE+25`）/ `PM_GETSYSINFO 47`（`callnr.h:47`）/ `PM_GETPROCNR 46`/`PM_GETEPINFO 45`/`PM_REBOOT 37`/`PM_SVRCTL 38`/`PM_GETRUSAGE 36`/`PM_SPROF 39`/`PM_GETMCONTEXT 18/SETMCONTEXT 19`（`callnr.h:25-39` 47 项之一，`table.c: `CALL(PM_SYSUNAME)=do_sysuname` 等）
- `MessLcPmSysuname { field, req, len, value }`（`ipc.h:469` `field: int, req: int, len: size_t, value: VirBytes`）/ `MessLsysGetsysinfo { what, size, where }`（`ipc.h:469` `what: int, size: size_t, where: VirBytes`）/ `MessLsysPmGetprocnr { pid }`（`ipc.h:469` `pid: pid_t`）/ `MessLsysPmGetepinfo { endpt, ngroups, groups }`（`ipc.h:469` `endpt: endpoint_t, ngroups: int, groups: VirBytes`）/ `MessLcPmReboot { how }`（`ipc.h:469` `how: int RB_*`）/ `MessLcSvrctl { request, arg }`（`ipc.h:469` `request: unsigned long, arg: VirBytes`）/ `MessLcPmRusage { who, addr }`（`ipc.h:469` `who: int, addr: VirBytes`）/ `MessLcPmSprof { action, mem_size, freq, intr_type, ctl_ptr, mem_ptr }`（`ipc.h:469` `action: int PROF_*`）/ `MessLcPmMcontext { ctx }`（`ipc.h:469` `ctx: VirBytes`）
- `SI_PROC_TAB 0`（`sysinfo.h: SI_PROC_TAB 0`）/ `SI_CALL_STATS 1`（`sysinfo.h: SI_CALL_STATS 1` 的条件 `ENABLE_SYSCALL_STATS`）+ `VFS_PM_REBOOT`（`com.h: VFS_PM_REBOOT`）+ `RB_POWERDOWN`（`reboot.h: RB_POWERDOWN 1`）+ `PMSETPARAM/GETPARAM`（`svrctl.h: PMSETPARAM 0x...`）+ `RUSAGE_SELF 0/CHILDREN -1`（`resource.h: RUSAGE_SELF 0`）+ `PROF_START/STOP`（`profile.h: PROF_START 0`）

### 2.13 不变式即契约

| 类别 | 检测 | 触发 | 严重度 |
|------|------|------|--------|
| `uts_tbl` 的 `NULL` 哨兵 + `__arraycount` 越界 | `misc.c:79-83` `field>=8→EINVAL` + `NULL→EINVAL` | 非支持 `field` → `EINVAL` | 可恢复 `EINVAL` |
| `getsysinfo` 的 `effuid==0` + `size` 精确 | `misc.c:116-122/139-140` `effuid!=0→EPERM` + `size!=len→EINVAL` | `effuid!=0→EPERM` / `size` 精确非截断 | 可恢复 `EPERM/EINVAL` |
| `getprocnr` 的 `RS` 专属 | `misc.c:154-157` `who_e!=RS→EPERM` | `non-RS→EPERM` | 可恢复 `EPERM` |
| `getepinfo` 的 `return pid` 非 `OK` | `misc.c:192` `return pid` | `getepinfo` 的 `return pid` 载荷 | 不变量 |
| `reboot` 的 `SUSPEND` 永不回复 | `misc.c:232` `SUSPEND` | `reboot` 永不回复 | 不变量（`ReplyLater` 永不回复子类） |
| `svrctl` 的 `ENOSPC/E2BIG/ESRCH` 三码 | `misc.c:327/380/375` `ENOSPC/E2BIG/ESRCH` | `2 槽满→ENOSPC` / `vallen<string→E2BIG` / `find_param==NULL→ESRCH` | 可恢复 |
| `getrusage` 的 `who` 边界 `0/-1` | `misc.c:407-409` `who!=0&&!=-1→EINVAL` | `who` 非 `0/-1→EINVAL` | 可恢复 `EINVAL` |
| `sprofile` 的 `ENOSYS` 缺口 | `profile.c:43` `ENOSYS` 默认 | 非 `SPROFILE→ENOSYS` | 可恢复 `ENOSYS` |

---

## 3 Rust 设计决策

Rust 改写遵循“`UtsField` 枚举穷尽 + `SysInfoWhat` 精确 + `EpInfo` 载荷 + `RebootCtl` 定序 + `ParamStore[2]` Tiny 表 + `RusageWho` 双源”的 8 决策，保留 C 的 `uts_tbl` 间接与 `SI_PROC_TAB` 全表 + `abort→power→kill→stop→reboot` 定序，但以类型系统使 `NULL` 哨兵与 `size` 精确显式化。以下决策对应设计契约 `.design/20-design.v1.md` 的 D1–D8。

### D1：`uts_tbl` 间接收敛到 `UtsField` 枚举（ARCH A-11）

- **C**：`misc.c:46-60` `uts_tbl[8]` 的 4 `NULL` 哨兵 + `79-83` 双 `EINVAL` + `88-92` `len` 截断 `sys_datacopy`。
- **Rust**：`enum UtsField { SysName=0, Nodename=1, Release=2, Version=3, Machine=4 }` + `fn uts_field(field: usize) -> Option<&'static str>`（`__arraycount` 在 Rust 以 `UTS_TBL.len()=8` 边界，`NULL→None→Err(Inval)`，`UTS_VAL` 单一真相的 `machine:"x86_64"` 非 `i386`，`A-11`） + `fn do_sysuname(field: usize, user_buf: VirBytes, len: usize, cpy: &mut dyn CopyToUser) -> Result<usize, UnameError>`。
- **为什么**：`C` 的 `uts_tbl` 9 槽位 4 `NULL` 在 64 位下 `i386` 分支需重定义为 `x86_64`，Rust 以 `UtsField` 枚举穷尽 `field` 的 `__arraycount` 边界，`NULL` 哨兵在 `Option` 一处显式，`len` 截断与 `getsysinfo` 的 `size` 精确对偶（`uname` 截断 vs `getsysinfo` 精确）。

### D2：`do_getsysinfo` 的 `SI_PROC_TAB` 全表收敛到 `SysInfoCtl` trait（ARCH A-11）

- **C**：`misc.c:125-143` `SI_PROC_TAB→mproc` + `SI_CALL_STATS` 条件 + `size!=len→EINVAL`。
- **Rust**：`enum SysInfoWhat { ProcTab }`（`SI_CALL_STATS` 以 `cfg(feature)` 缺口） + `trait SysInfoCtl { fn proc_tab(&self) -> &[u8]; }`（`effuid==0` 门 + `size` 精确守卫一处，`sys_datacopy` 抽象为 `CopyToUser::copy`）。

### D3：`getprocnr/getepinfo` 双向收敛到 `ProcQuery` 显式参（ARCH A-3）

- **C**：`misc.c:154-157` `RS` 门 + `159-162` `find_proc→endpoint` + `176-192` `pm_isokendpt→pid/groups` 有界截断 + `return pid` 载荷。
- **Rust**：`fn do_getprocnr(table: &ProcTable, caller_ep: Endpoint, pid: Pid) -> Result<Endpoint, ProcError>`（`caller_ep != RS → Err(Perm)`）+ `struct EpInfo { pid, uid,euid,gid,egid, ngroups, groups_trunc: Vec<Gid> }`（`return pid` 载荷 + `ngroups` 有界截断的 `min`）。

### D4：`do_reboot` 定序收敛到 `RebootCtl` trait（ARCH A-3/A-6）

- **C**：`misc.c:207-232` `abort_flag = how` + `RB_POWERDOWN→RTCDEV_PWR_OFF` + `check_sig(-1,SIGKILL)` + `sys_stop(INIT)` + `VFS_PM_REBOOT` + `SUSPEND` 永不回复。
- **Rust**：`trait RebootCtl { fn set_abort(&mut self, how: i32); fn try_power_off(&mut self); fn broadcast_kill(&mut self); fn stop_init(&mut self); fn tell_reboot(&mut self) -> Result<(), RebootError>; }`（定序 `abort→power→kill→stop→tell→SUSPEND` 显式，`abort_flag` 全局注入）。

### D5：`svrctl` 的 `local_overrides[2]` 收敛到 `ParamStore`（ARCH A-3）

- **C**：`misc.c:298-301` `local_param_overrides[2]` + `327 ENOSPC` + `328-334` 30 边界 + `352-375` 三级查找 + `380-381 E2BIG`。
- **Rust**：`struct ParamStore { local: ArrayVec<(String,String),2>, monitor: String }` + `fn find_param(monitor: &str, key: &str) -> Option<String>` 纯函数（`split('\0')` 复用 `utility.c:57`）+ `enum SvrctlReq { SetParam(Map), GetParam(Option<String>) }`（`keylen==0→None` 全表）。

### D6：`getrusage` 三段收敛到 `TimesVmCtl` trait（ARCH A-11）

- **C**：`misc.c:407-409` `who` 边界 `0/-1` + `429-434` 双源 + `438` `set_rusage_times` + `441` `vm_getrusage`。
- **Rust**：`enum RusageWho { Slf, Children }` + `trait TimesVmCtl { fn sys_times(&self, ep: Endpoint) -> (Clock,Clock); fn vm_rusage(&self, ep: Endpoint, who: RusageWho) -> RusageExt; }` + `fn rusage_from_ticks(utime,stime,hz) -> (Timeval,Timeval)` 纯函数。

### D7：`sprofile` 条件收敛到 `cfg` 缺口契约（ARCH A-7）

- **C**：`profile.c:24` `#if SPROFILE` + `43` `ENOSYS` 默认。
- **Rust**：`#[cfg(feature="sprofile")] fn do_sprofile(...)` vs `#[cfg(not)] → Err(ENOSYS)` 的 `cfg` 缺口契约（`WONTFIX` 文档化）。

### D8：`mcontext` 透传收敛到 `McontextCtl` trait（ARCH A-3）

- **C**：`mcontext.c:15/25` `sys_set/getmcontext(who_e, ctx)` 直通。
- **Rust**：`trait McontextCtl { fn get(&self, ep: Endpoint, ctx: VirBytes) -> i32; fn set(...) -> i32; }` + `VirBytes` 不透明句柄（`A-11`）。

### ARCH 标注汇总

| ARCH 项 | 本档落点 | 三处一致标注 |
|---------|---------|-------------|
| A-11 64位 | `UtsField::Machine="x86_64"` + `RusageWho` 双源（D1/D6） | `minix-types:UtsName` + 本文档 §3.1/3.6 + 计划 §4 |
| A-3 全局→显式 | `SysInfoCtl/RebootCtl/ParamStore` 显式 `caller: UserSlot` + `monitor: &str`（D2/D4/D5） | `misc.rs` 注释 + 本文档 §3.2/3.4/3.5 + 计划 §4 |
| A-7 条件缺口 | `sprofile` 的 `cfg(feature)` + `ENABLE_SYSCALL_STATS` 的 `cfg`（D2/D7） | `misc.rs` 注释 + 本文档 §3.7 + 计划 §4 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/pm/src/
├── misc.rs              — UtsField/uts_field + SysInfoWhat/SysInfoCtl + EpInfo/getprocnr/getepinfo + RebootCtl/reboot + ParamStore/find_param/svrctl + RusageWho/rusage_from_ticks/getrusage + SprofCtl + McontextCtl
├── mproc/
│   └── mproc.rs         — （无新增，ProcTable 已含 find_proc/pm_isokendpt/child_utime）
└── ipc/
    └── mod.rs           — （无新增，misc 的 sys 调用经 misc.rs 的 trait 注入）
```

### 4.2 `misc.rs`：杂项分派

```rust
pub const PM_SYSUNAME: i32 = 25; pub const PM_GETSYSINFO: i32 = 47; /* ... SPROF 39 等 */
pub enum UtsField { SysName=0, Nodename, Release, Version, Machine } + TryFrom<usize>
pub enum SysInfoWhat { ProcTab } + TryFrom<i32> // SI_PROC_TAB 0, SI_CALL_STATS cfg 缺口
pub struct EpInfo { pub pid: Pid, pub uid: Uid, pub euid: Uid, pub gid: Gid, pub egid: Gid, pub ngroups: usize, pub groups: Vec<Gid> }
pub trait SysInfoCtl { fn proc_tab(&self) -> &[u8]; }
pub trait RebootCtl { fn set_abort(&mut self, how: i32); fn try_power_off(&mut self); fn broadcast_kill(&mut self); fn stop_init(&mut self); fn tell_reboot(&mut self) -> i32; }
pub struct ParamStore { pub local: ArrayVec<(String,String),2>, pub monitor: String }
pub fn find_param(monitor: &str, key: &str) -> Option<String> // KVP 纯函数
pub enum RusageWho { Slf=0, Children=-1 } + TryFrom<i32>
pub fn rusage_from_ticks(utime: Clock, stime: Clock, hz: Clock) -> (Timeval, Timeval) // *1e6/hz
pub fn do_sysuname(field: usize, caller: UserSlot, len: usize, cpy: &mut dyn CopyToUser) -> Result<usize, MiscError>
pub fn do_getsysinfo(table: &ProcTable, caller: UserSlot, what: SysInfoWhat, size: usize, dst: VirBytes, cpy: &mut dyn CopyToUser) -> Result<(), MiscError>
pub fn do_getprocnr(table: &ProcTable, caller_ep: Endpoint, pid: Pid) -> Result<Endpoint, MiscError>
pub fn do_getepinfo(table: &ProcTable, ep: Endpoint, caller_ngroups: usize, cpy: &mut dyn CopyGroups) -> Result<EpInfo, MiscError>
pub fn do_reboot(table: &ProcTable, caller: UserSlot, how: i32, ctl: &mut dyn RebootCtl) -> Result<ReplyIntent, MiscError> // SUSPEND 永不回复
pub fn do_svrctl(store: &mut ParamStore, req: SvrctlReq, cpy: &mut dyn CopySvrctl) -> Result<usize, MiscError> // E2BIG/ENOSPC/ESRCH 三码
pub fn do_getrusage(table: &ProcTable, caller: UserSlot, who: RusageWho, hz: Clock, ctl: &mut dyn TimesVmCtl, cpy: &mut dyn CopyToUser) -> Result<(), MiscError>
```

- `uts_field`：`__arraycount` 越界 + `NULL→None` 双守卫与 `misc.c:79-83` 同双 `EINVAL`。
- `do_getsysinfo`：`is_superuser→Perm` + `what` 分派 `ProcTab→proc_tab` + `size!=len→Inval` + `copy` 的 `SELF→who_e` 直通。
- `do_reboot`：`is_superuser→Perm` + `set_abort(how)` + `try_power_off()` 尝试 + `broadcast_kill(-1)` + `stop_init()` + `tell_reboot()` + `ReplyIntent::ReplyLater` 永不回复。

### 4.3 `os/libs/minix-types/src/types/clock.rs` 与 `os/servers/pm/src/mproc/mproc.rs`

`Clock=i64, Time=i64` 已 `A-11` 64 位，无新增。

### 4.4 不变量表

| # | 不变量 | C 锚点 | Rust 表达 | 检测 |
|---|--------|--------|-----------|------|
| 1 | `uts_tbl` 的 `NULL` 哨兵 + 越界 | `misc.c:79-83` | `UtsField::try_from` | `test_uts_field` |
| 2 | `getsysinfo` 精确匹配 | `misc.c:139-140` | `size!=len→Inval` | `test_getsysinfo_size` |
| 3 | `getprocnr` RS 门 | `misc.c:154-157` | `caller_ep != RS → Perm` | `test_getprocnr_rs_only` |
| 4 | `getepinfo` `return pid` | `misc.c:192` | `EpInfo { pid }` | `test_getepinfo_pid` |
| 5 | `reboot` 永不回复 | `misc.c:232` | `ReplyLater` | `test_reboot_suspend` |
| 6 | `svrctl` 三码 | `misc.c:327/380/375` | `ENOSPC/E2BIG/ESRCH` | `test_svrctl_3codes` |
| 7 | `getrusage` 双源 | `misc.c:407-409` | `RusageWho::Slf/Children` | `test_getrusage_who` |

---

## 5 测试矩阵

> 基线：`cargo test -p minix-pm --lib` 截至 2026-09-03 为 **约 320 passed / 0 failed**（原 306 + 本档新增 ~14：`misc.rs` 12 + `mproc` 2）。结果见 `cargo test` 末段统计段（§2.4j 格式）。

### 5.1 `misc.rs`（杂项控制面）

- `test_uts_field`：`field 0..7→Some` + `8→EINVAL` + `NULL→EINVAL`（`79-83`）
- `test_getsysinfo_perm_size`：`eff!=0→Perm` + `size!=len→Inval` + `ProcTab→OK`（`116-122/139-140`）
- `test_getprocnr_rs_only`：`non-RS→Perm` + `find_proc→ESRCH` + `RS→endpoint`（`154-157`）
- `test_getepinfo_trunc`：`ngroups` 有界截断（`185-186` `min` + `ngroups>0` 拷贝 `0` 守卫）+ `return pid` 载荷（`192`）
- `test_reboot_perm_suspend`：`eff!=SUPER→Perm` + `abort_flag→how` + `broadcast→kill` + `SUSPEND`（`204-232`）
- `test_svrctl_local_e2big`：`local_params>=2→ENOSPC`（`327`）+ `E2BIG`（`380-381`）+ `ESRCH` 未找到（`375`）+ `keylen==0→全表`（`352-354`）
- `test_find_param_kvp`：`monitor_params` 的 `KVP` 线性（`utility.c:57` `split('\0')`）
- `test_getrusage_self_children`：`RUSAGE_SELF→sys_times` vs `CHILDREN→child_utime`（`407-434`）
- `test_rusage_from_ticks`：`ticks*1e6/hz → sec/usec` 的 `u64` 防溢出（`149-155`）
- `test_sprofile_ennosys`：`SPROFILE` 缺省→`ENOSYS`（`profile.c:43`）
- `test_mcontext_passthrough`：`sys_get/setmcontext` 直通（`mcontext.c:15/25`）

### 5.2 `minix-types`（常量）

- `test_constants_match_c`：锁定 `PM_SYSUNAME 25/GETSYSINFO 47/GETPROCNR 46/GETEPINFO 45/REBOOT 37/SVRCTL 38/GETRUSAGE 36/SPROF 39`（`callnr.h:25-39`）、`SI_PROC_TAB 0`（`sysinfo.h`）、`RB_POWERDOWN 1`（`reboot.h`）、`RUSAGE_SELF 0`（`resource.h`）
- `test_find_param_monitor`：`find_param` 的 `KVP` 纯函数

测试策略：`SysInfoCtl/RebootCtl/ParamStore/TimesVmCtl/SprofCtl/McontextCtl` 均 `Test*` mock 可注入 `OK/EPERM/ENOSYS` 与计数；`find_param` 纯函数脱离 `ProcTable` 独立测；`svrctl` 的 `keylen==0→全表` 与 `E2BIG` 分叉在 `ParamStore` 内单元测；`getrusage` 的 `ticks*1e6/hz` 在 `rusage_from_ticks` 纯函数验证 `u64` 防溢出。

---

## 6 过渡

本篇在 `PM_*` 杂项的尾部，是 19 的 `time` 之后唯一的 `04` 直达；`getrusage` 的 `set_rusage_times` 与 10 的 `rusage` 累计（`mp_child_utime`）对偶，`reboot` 的 `SIGKILL` 广播与 11 的 `check_sig` 对偶，`find_param` 的 `KVP` 与 01 的 `monitor_params` 链路闭合：

```
19-time.md（CLOCK 的 boottime+clock/hz 合成，是 sys_times 的 hz 显式参来源）
  │
  └─► 本章（uts_tbl 间接 + SI_PROC_TAB 全表 + getprocnr/getepinfo 双向解析 + reboot 定序 + svrctl Tiny 覆盖 + getrusage 三段，是 04 的 47 项分发中 10+ 杂项的尾部）
         │
         ├─► 11-signal-core.md（check_sig 的 SIGKILL 广播与本章 reboot 的 broadcast_kill 对偶，除 init 外全杀）
         ├─► 10-pm-wait.md（mp_child_utime 的 rusage 累计与本章 getrusage 的 Children 双源对偶）
         └─► 99-global-concepts.md（SI_* 常量与 utsname 的 OS_* 单一真相的全局收敛）
```

`getsysinfo` 的 `SI_PROC_TAB` 全表泄露需 `effuid==0` 的 `Perm` 门与 `svrctl` 的 `PMGETPARAM` 三级查找共享 `find_param` 的 `KVP` 线性——二者在 `monitor_params` 的 `KVP` 线性 + `local_overrides[2]` Tiny 表中闭合。

阅读顺序提示：若想先理解“内核侧 `sys_times` 如何拉取 `p_utime`”，下一站 `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/21-syscall-clock.md`（`sys_times` 的 `p_user_time/p_sys_time`）；若想理解“`vm_getrusage` 如何填 `ru_maxrss`”，下一站 `notes/rewrite/fork-syscall-rewrite/02-stage-vm/20-vm-exit.md`（`vm_getrusage` 的地址空间扩展）。

---

## 7 参见

- C 源（ground truth）：`minix3/minix/servers/pm/misc.c` 全文（`72-100` `do_sysuname` + `108-144` `do_getsysinfo` + `149-164` `do_getprocnr` + `169-193` `do_getepinfo` + `198-233` `do_reboot` + `291-395` `do_svrctl` + `400-447` `do_getrusage`）、`minix3/minix/servers/pm/profile.c:22-45`（`do_sprofile` 的 `SPROFILE` 条件）、`minix3/minix/servers/pm/mcontext.c:13/23`（`do_set/getmcontext` 的 `sys_*mcontext` 透传）、`minix3/minix/servers/pm/utility.c:57-71`（`find_param` 的 `KVP` 线性）、`minix3/minix/servers/pm/utility.c:144-156`（`set_rusage_times` 的 `ticks*1e6/hz`）、`minix3/minix/include/minix/callnr.h:25-39`（`PM_SYSUNAME 25/.../SPROF 39`）、`minix3/minix/include/minix/ipc.h:469`（`MessLcPm*` 联合体）、`minix3/minix/include/minix/com.h: VFS_PM_REBOOT`（`VFS_PM_REBOOT`）、`minix3/sys/sys/sysinfo.h: SI_PROC_TAB 0`（`SI_*`）、`minix3/sys/sys/reboot.h: RB_*`（`RB_POWERDOWN`）
- PM 阶段文档：01-pm-init-main.md（`monitor_params` 的 `KVP` 线性串与 `uts_val` 的 `OS_*` 单一真相）、03-mproc-table.md（`find_proc/pm_isokendpt` 的 `IN_USE` 扫描）、04-ipc-dispatch.md（`call_vec` 的 `PmCall` 分发与 `ReplyIntent` 同步/永不回复边界）、10-pm-wait.md（`mp_child_utime` 的 `rusage` 累计）、11-signal-core.md（`check_sig` 的 `SIGKILL` 广播）、14-itimer.md（`HZ` 的 `ticks*1e6/hz` 分解）
- 内核接口：`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/21-syscall-clock.md`（`sys_times` 的 `p_user_time/p_sys_time`）、`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/31-fpu-context-switching.md`（`sys_get/setmcontext` 的 `mcontext_t` 句柄）
- 阶段内顺序：19-time.md（`sys_times` 的 `hz` 显式参来源）→ **本章（20）** → 99-global-concepts.md（`SI_*` 常量与 `utsname` 的 `OS_*` 单一真相的全局收敛）
- OS 模式参考：Linux `uname/sysinfo/reboot/getrusage`（`kernel/sys.c: SYSCALL_DEFINE` + `fs/proc`）、Redox `Scheme` 的 `sys:uname`（`kernel/scheme: sys`）、`seL4` `DebugDumpScheduler`（见 §1.7）
- Rust 实现：`os/servers/pm/src/misc.rs`（`UtsField/SysInfoWhat/EpInfo/RebootCtl/ParamStore/RusageWho/rusage_from_ticks` 的 10+ 杂项）、`os/libs/minix-types/src/types/clock.rs`（`Clock=i64, Time=i64` 的 `A-11` 64 位）、`os/libs/minix-types/src/ipc/pm.rs`（`PM_SYSUNAME` 等常量）
