# 17 — 执行替换：`do_exec` 的 VFS 转发与 `PARTIAL_EXEC`/`TAINTED`/`frame`/`sig`/`tracer` 状态机

本文讲清 `exec` 如何以“`do_exec` 的 `VFS_PM_EXEC` 六字段转发 + `do_newexec` 的 `allow_setuid && TAINTED` 二重与 `PARTIAL_EXEC` 哨兵 + `exec_restart` 的 `catch` 重置/ `tracer` 的 `SIGTRAP/SIGSTOP` 先于 `sys_exec` + `frame` 的 `stack_high - frame_len` 保存”为完整链路，使 `execve(2)` 的“权限判断在 VFS，凭证更新在 PM，半初始化态以 `SIGKILL` 自毁”的三段式在 `VFS` 异步与 `PARTIAL_EXEC` 哨兵中可区分。

前置阅读：05-vfs-interaction.md（`tell_vfs` 的 `VFS_CALL` 置位 + `handle_vfs_reply` 的 `VFS_PM_EXEC_REPLY` → `exec_restart` 成功路径的 `sched_start_user` 已抽象）、15-credentials.md（`Credentials::set_uid_all` 的 `real/eff/saved` 三元与 `SUPER_USER` 判据，`tainted: bool` 唯一真源）、11-signal-core.md（`sig_proc` 的 `catch` 位与 `signal.c:179-182` 的 `exec` 后 `catch` 重置消费方）、12-signal-handlers.md（`SignalState::install` 的 `caught` 位语义）、02-mproc-struct.md（`ProcessResources { frame_addr/frame_len, nice/scheduler }` + `RemainingFlags::PARTIAL_EXEC`）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `VFS` 异步的 `VFS_CALL` 置位与 `SUSPEND` 解挂（05）、`Credentials` 三元的 `real/eff/saved` 全置（15）、`SignalState` 的 `caught` 位与 `without_unkillable`（12）的开发者；知道 `VirBytes` 的 `frame` 与 `PROC_NAME_LEN 16` 的 `mp_name`。

> **本章不讲什么**：
> - VFS 可执行加载/解释器 `#!` 与 `read_header`（`libexec` 的 `PTRSIZE` 的 `argv/envp` 栈帧构造）—— `05-stage-vfs`（`VFS` 侧 `exec` 的 `read_header` 与 `vm` 的 `mmap`）
> - VM 内存重映射与 `vm_willexit`（`vm` 的 `mmap` 与 `vm_exit` 族）—— `02-stage-vm/18-vm-fork` 的 `vm_exit` 族
> - 信号重置的接收方语义（`12` 的 `caught→DFL` 已述，`11` 的 `SIG_IGN` 保留）—— `12-signal-handlers.md`
> - 内核 `sys_exec` 的 `proc` 表 `p_reg` 重置（`p_reg` 的 `SP/PC` 重置）—— `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/19-syscall-signal` 的 `sys_exec` 与 `19` 的 `m_context`
>
> 本章只回答一个问题：**PM 如何为“`execve` 的权限判断在 VFS，凭证与 `TAINTED` 更新在 PM，半初始化态以 `PARTIAL_EXEC` 哨兵区分成功与 `SIGKILL` 自毁，且旧捕获处理器在新镜像中无意义而需重置，且调试器的 `SIGTRAP` 需先于 `sys_exec`”的三段式在 `VFS` 异步中建立 `ExecState::Partial` 哨兵**。

### 1.1 为什么 `exec` 必须经 VFS 转发：权限判断在 VFS，凭证更新在 PM

`do_exec` 不直接读 ELF，而是 `VFS_PM_EXEC` 六字段转给 VFS（`exec.c:44-50` `ENDPT/PATH/PATH_LEN/FRAME/FRAME_LEN/PS_STR`，`ipc.h:469` `name/namelen/frame/framelen/ps_str`），VFS 负责 `read_header` 与 `PTRSIZE` 的 `argv/envp` 栈帧构造与 `setuid` 位判断（`exec_info.allow_setuid/new_uid/gid/progname/stack_high/frame_len`，`minix/vm.h: exec_info`），PM 仅在 `do_newexec` 回调中更新凭证与 `TAINTED`（`exec.c:83-117`）——`exec` 的“权限判断在 VFS，凭证更新在 PM”分工使 `setuid` 位的 `allow_setuid` 在 VFS 侧以 `stat` 的 `S_ISUID` 判据，PM 侧以 `tracer==NO_TRACER` 再判（`86-89` 调试器禁 `setuid`）。

`do_exec` 的 `tell_vfs(mp,&m)` 置 `VFS_CALL` + `SUSPEND`（`52/55` `04` 的 `ReplyLater` 的 `do_exec` 子类）使 `exec` 的“转 `VFS` → 等 `VFS` 回调 `do_newexec` → 等 `handle_vfs_reply` 的 `VFS_PM_EXEC_REPLY` → `exec_restart`”三段式在 `VFS` 异步中完成（`05` 的 `VfsCall::Exec` 已部分定义）。

### 1.2 为什么 `TAINTED` 有二重：`setuid` 位程序与 `eff!=real` 的已污染进程

`exec.c:103-109` 的 `TAINTED` 二重：

```
allow_setuid && args.allow_setuid → TAINTED（103-105 `setuid` 位程序）
eff!=real || effgid!=realgid → TAINTED（106-109 `seteuid` 后 eff!=real 的已污染进程再 exec 保持污染）
```

`TAINTED` 是“以非 `real` 权限执行”的持久标记，`PM_ISSETUGID` 的 `!!(flags & TAINTED)`（`getset.c:81` `TAINTED 0x40000`，`15` 的 `tainted: bool` 唯一真源）为 `issetugid(2)` 的 `LD_PRELOAD` 防注入（`libexec` 的 `rtld` 路径隔离，`issetugid` 为真时不加载用户 `LD_LIBRARY_PATH`）。`do_newexec:84` `~TAINTED` 默认清零使 `exec` 后先清再按二重重设。

### 1.3 为什么 `PARTIAL_EXEC` 是哨兵：已分配新映射但未内容的半态

`do_newexec:120` 置 `PARTIAL_EXEC`（`mproc.h:99` `PARTIAL_EXEC 0x4000`）后 `122` 回 `suid` 标志给 VFS，`exec_restart:161-167` 的 `result!=OK && PARTIAL_EXEC → SIGKILL` 使“已分配新地址空间但加载失败（`vm` 的 `mmap` 或 `read_header` 的 `ESCRIPT`）”的进程以 `SIGKILL` 自毁而非 `reply(result)` 泄露半初始化态（`161-167` `Use SIGKILL to signal that something went wrong`）。

`PARTIAL_EXEC` 的哨兵期 `frame` 有效（`116-117` `frame_addr = stack_high - frame_len` / `frame_len`），`exec_restart:173` `~PARTIAL_EXEC` 清零使 `Idle` 时 `frame` 无意义不可表示（`D3` `ExecState::Partial { frame }` 的 `Some` 显式携带 `frame`）。

### 1.4 为什么 `exec` 后 `catch` 需重置而 `ignore` 保留：`exec` 的信号语义

`exec_restart:178-184` `for sn 1.._NSIG if catch→del catch/handler=DFL/empty mask` 使“旧程序的捕获处理器在新程序镜像中无意义”而 `ignore` 的 `SIG_IGN` 保留（`11` 的 `mp_ignore` 非 `catch`，`exec` 后 `ign` 位仍有效，POSIX “`exec` 保留 `SIG_IGN` 而重置 `SIG_DFL` 的捕获”）。`12` 的 `SignalState::install` 的 `caught` 位消费在 `exec_restart` 的 `reset_caught_for_exec` 一处方法（`mproc/signal.rs:reset`）。

### 1.5 为什么 `tracer` 的 `SIGTRAP/SIGSTOP` 先于 `sys_exec`：调试器前置信号

`exec_restart:189-194` `tracer!=NO_TRACER && !TO_NOEXEC → TO_ALTEXEC?SIGSTOP:SIGTRAP → check_sig(pid,sn,FALSE)` 先于 `sys_exec` 的 `197` 使调试器在新镜像入口前先停（`TO_ALTEXEC` 选 `SIGSTOP` 否则 `SIGTRAP`，`trace.c:TO_*` 标志，`sys/ptrace.h: TO_NOEXEC 0x1/ALTEXEC 0x2`），`sys_exec` 后进程已 `runnable` 再 `check_sig` 则窗口丢失（`189-194` 先于 `197` 的时序不可颠倒，`D5` `signal_for_exec` 枚举穷尽）。

### 1.6 为什么 `frame` 的 `stack_high - frame_len` 保存：`procfs` 的 `initial stack` 偏移

`do_newexec:116-117` `mp_frame_addr = stack_high - frame_len` + `mp_frame_len` 为 `procfs` 的 `initial stack` 偏移（`mproc.h:71-72` `mp_frame_addr/len`，`procfs` 经 `m_context` 读 `frame` 的 `argc`），`exec_restart` 的 `sp` 参数即 `frame_addr`（`156` `sp` 为 `rmp->mp_frame_addr`，`197` `sys_exec(endpoint, sp, name, pc, ps_str)` 的 `SP`）。

### 1.7 与其他 OS 的对照

Rust 改写不是照抄 `strncpy(progname)`，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `do_execve` 的 `bprm`。** Linux `do_execve` 的 `linux_binprm { filename, argv, envp, cred, mm }` 准备 + `search_binary_handler`（`fs/exec.c:search_binary_handler` 的 `PTRSIZE` 的 `argv/envp` 栈帧与 `read_header` 的 `#!` 解释器）+ `install_exec_creds` 的 `setuid` 位与 `TAINTED` 对偶（`fs/exec.c:install_exec_creds` 的 `bprm->cred->euid = bprm->file->f_cred->euid` 的 `setuid` 位与 `bprm->per_clear` 的 `TAINTED` 清零），Minix3 的 `VFS_PM_EXEC` 六字段转给 VFS 的 `read_header` 与 `PTRSIZE` 的 `argv/envp` 栈帧构造同 `bprm` 的 `argv/envp`，PM 侧 `do_newexec` 的 `allow_setuid` 双重与 `TAINTED` 二重同 `install_exec_creds` 的 `setuid` 位判断。

**Redox `exec` 经 `Scheme` 的 `fexec`。** Redox 以 `syscall::exec` 的 `Scheme` 的 `fexec`（`kernel/scheme` 的 `exec` 经 `Context::reg` 重置 `IP/SP` 与 `Context::name` 的 `progname` 拷贝，同 `exec.c:112-113` `strncpy(mp_name, progname)`），`Context::sig` 的 `caught` 重置经 `sig::reset`（同 `178-184` `reset_caught_for_exec`），`TAINTED` 的 `LD_PRELOAD` 防注入经 `Context::tainted` 的 `bool`（`15` 的 `tainted: bool` 同源）。

**`seL4` 无 `exec` 的 `TCB` 重置。** `seL4` 无 `exec`，以 `seL4_TCB_Configure` + `seL4_TCB_WriteRegisters` 的 `TCB` 重置（`TCB` 的 `IP/SP` 与 `CNode` 的 `capability` 派生）替代 `exec`，PM 的 `PARTIAL_EXEC` 哨兵在 `seL4` 以 `TCB` 的 `bound_notification` 哨兵替代（`TCB` 的 `bound_notification` 未决时 `TCB_WriteRegisters` 的 `SP` 不生效），`sys_exec` 的 `SP/PC` 在 `seL4` 以 `TCB_WriteRegisters` 的 `IP/SP` 显式。

**结论（本章的设计基线）。** 把 C 的“`VFS_PM_EXEC` 六字段裸赋值 + `allow_setuid && TAINTED` 二重散落 + `PARTIAL_EXEC` 位与 `mp_frame_addr/len` 双字段哨兵期双重 + `for sn if catch→DFL` 散落 + `TO_NOEXEC/ALTEXEC` 位 + `stack_high - frame_len` 裸减”改写为“`ExecRequest { caller, path, frame }` + `ExecCreds { allow_setuid, new_uid/gid }` + `ExecState::{Idle,Partial { frame }}` + `SignalState::reset_caught_for_exec` + `TracerExec::signal_for_exec` + `FrameRegion { base, len }` + `KernelExec::exec`”——与 Linux/Redox 的 `bprm` 准备 + `setuid` 位判断 + `TAINTED` 清零同源，又因 PM 单线程无共享而以 `&mut ProcTable` 的 `FrameRegion` 显式携带 `base`。

### 1.8 小结

1. **为什么 VFS 转发**——`do_exec` 转 `VFS_PM_EXEC` 六字段，`VFS` 负责 `read_header` 与 `PTRSIZE` 栈帧，PM 在 `do_newexec` 回调更新凭证。
2. **为什么二重**——`allow_setuid && args.allow_setuid` 的 `setuid` 位程序与 `eff!=real` 的已污染进程再 `exec` 保持污染，二重使 `TAINTED` 持久。
3. **为什么哨兵**——`PARTIAL_EXEC` 置位后 `frame` 有效，`result!=OK && PARTIAL_EXEC→SIGKILL` 使半初始化态以 `SIGKILL` 自毁。
4. **为什么重置**——`exec` 后 `catch` 无意义而 `ignore` 保留，`for sn if catch→DFL` 的 `12` 位消费。
5. **为什么先发调试信号**——`tracer` 的 `SIGTRAP/SIGSTOP` 先于 `sys_exec` 使调试器在新入口前先停。
6. **为什么 `frame` 保存**——`stack_high - frame_len` 为 `procfs` 的 `initial stack` 偏移，`sp` 即 `base`。

下一章逐行分析 C 的 `do_exec`/`do_newexec`/`exec_restart`/`do_execrestart`；第 3 章给出 Rust 的 `ExecRequest`/`ExecState`/`FrameRegion`。

---

## 2 C 源码分析

### 2.1 `do_exec`（`exec.c:38-56`）

```c
int do_exec(void)
{ // 38  PM_EXEC 转发 VFS
  message m; // 40
  memset(&m, 0, sizeof(m)); // 42
  m.m_type = VFS_PM_EXEC; // 44  VFS_PM_EXEC（com.h: VFS_PM_RQ_BASE+6）
  m.VFS_PM_ENDPT = mp->mp_endpoint; // 45  ENDPT
  m.VFS_PM_PATH = (void *)m_in.m_lc_pm_exec.name; // 46  PATH（vir_bytes name）
  m.VFS_PM_PATH_LEN = m_in.m_lc_pm_exec.namelen; // 47  PATH_LEN
  m.VFS_PM_FRAME = (void *)m_in.m_lc_pm_exec.frame; // 48  FRAME
  m.VFS_PM_FRAME_LEN = m_in.m_lc_pm_exec.framelen; // 49  FRAME_LEN
  m.VFS_PM_PS_STR = m_in.m_lc_pm_exec.ps_str; // 50  PS_STR（ps_strings 指针）
  tell_vfs(mp, &m); // 52  VFS_CALL 置位（05 的 VfsCall::Exec 复用）
  return SUSPEND; // 55  SUSPEND（04 的 ReplyLater 的 do_exec 子类）
}
```

`44-50` 六字段与 `ipc.h:469` `mess_lc_pm_exec { name/namelen/frame/framelen/ps_str }` 同位（`name` 为 `vir_bytes name`，`ps_str` 为 `ps_strings` 指针，`VFS` 侧 `exec` 的 `read_header` 与 `PTRSIZE` 栈帧构造依赖此六字段），`52` `tell_vfs` 的 `VFS_CALL` 置位与 `05` 的 `handle_vfs_reply` 的 `VFS_PM_EXEC_REPLY` 解挂构成双向闭环（`tell_vfs` 的 `NotIdle` 守卫与 `55` `SUSPEND` 的 `ReplyLater` 显式化）。

### 2.2 `do_newexec` 序言（`exec.c:62-85`）

```c
int do_newexec(void)
{ // 62  VFS/RS 回调（exec_info 回填）
  int proc_e, proc_n, allow_setuid; vir_bytes ptr; struct mproc *rmp; struct exec_info args; int r; // 64-68
  if (who_e != VFS_PROC_NR && who_e != RS_PROC_NR) return EPERM; // 70-71  VFS/RS→EPERM 门
  proc_e= m_in.m_lexec_pm_exec_new.endpt; // 73  endpt（m_lexec_pm_exec_new.endpt）
  if (pm_isokendpt(proc_e, &proc_n) != OK) panic("do_newexec: got bad endpoint: %d", proc_e); // 74-76  bad endpoint→panic
  rmp= &mproc[proc_n]; // 77
  ptr= m_in.m_lexec_pm_exec_new.ptr; // 78  ptr（m_lexec_pm_exec_new.ptr 的 exec_info 指针）
  r= sys_datacopy(who_e, ptr, SELF, (vir_bytes)&args, sizeof(args)); // 79  拷 exec_info
  if (r != OK) panic("do_newexec: sys_datacopy failed: %d", r); // 80-81
  allow_setuid = 0; rmp->mp_flags &= ~TAINTED; // 83-84  默认不许 setuid 且清 TAINTED
```

`70-71` 行 `VFS/RS→EPERM` 门使仅 `VFS` 与 `RS` 可回调 `do_newexec`（`RS` 的 `do_execrestart` 经 `RS` 转 `VFS` 间接回调），`74-76` 行 `pm_isokendpt` 失败 `panic`（`do_newexec` 的 `endpt` 为 VFS 已校验的 `mproc[proc_n].endpoint`，`bad endpoint` 为 VFS/PM 表不一致的不可恢复），`79` 行 `sys_datacopy` 取 `exec_info` 的六字段（`minix/vm.h: exec_info { allow_setuid, new_uid/gid, progname[16], stack_high, frame_len }`），`83-84` 行 `allow_setuid=0; ~TAINTED` 默认清零使 `exec` 后先清再按二重重设。

### 2.3 `allow_setuid` 与 `setuid` 更新（`exec.c:86-99`）

```c
  if (rmp->mp_tracer == NO_TRACER) { // 86  调试器禁 setuid（A-12 双监护外）
    allow_setuid = 1; // 88  Okay, setuid execution is allowed
  }
  if (allow_setuid && args.allow_setuid) { // 91  VFS 侧 setuid 位与 PM 侧 tracer 双重
    rmp->mp_effuid = args.new_uid; // 92  effuid = new_uid
    rmp->mp_effgid = args.new_gid; // 93  effgid = new_gid
  }
  rmp->mp_svuid = rmp->mp_effuid; // 97  svuid = eff
  rmp->mp_svgid = rmp->mp_effgid; // 98  svgid = eff
```

`86-89` 行 `tracer==NO_TRACER→allow=1` 使调试器禁 `setuid`（`11` 的 `Guardianship::Traced` 与 `exec` 的 `TAINTED` 二重对偶，`tracer` 存在时 `setuid` 位不生效），`91-94` 行 `allow&&args.allow → eff` 的 `new_uid/gid` 在 `VFS` 侧 `exec_info.allow_setuid`（`stat` 的 `S_ISUID` 位）与 PM 侧 `tracer` 双重后 `eff` 更新，`97-98` 行 `svuid/svgid = eff` 全量回写使 `saved` 与 `effective` 一致（`15` 的 `IdSet` 三元的 `saved` 回写与 `do_set` 的 `set_uid_all` 全置同源但 `exec` 仅 `saved=eff` 的 `eff→saved` 单向）。

### 2.4 `TAINTED` 二重（`exec.c:100-109`）

```c
  if (allow_setuid && args.allow_setuid) { // 103  Program has setuid and/or setgid bits set
    rmp->mp_flags |= TAINTED; // 105
  } else if (rmp->mp_effuid != rmp->mp_realuid || // 106  已污染进程再 exec
         rmp->mp_effgid != rmp->mp_realgid) { // 107
    rmp->mp_flags |= TAINTED; // 108
  }
```

`103-105` 行 `allow&&args.allow→TAINTED` 的 `setuid` 位程序与 `106-109` 行 `eff!=real → TAINTED` 的已污染进程再 `exec` 保持污染二重（`15` 的 `tainted: bool` 唯一真源的 `TAINTED` 二重，`issetugid` 的 `LD_PRELOAD` 防注入依赖此位）。

### 2.5 `mp_name/frame/PARTIAL_EXEC`（`exec.c:111-120`）

```c
  strncpy(rmp->mp_name, args.progname, PROC_NAME_LEN-1); // 112  progname 拷贝（PROC_NAME_LEN 16）
  rmp->mp_name[PROC_NAME_LEN-1] = '\0'; // 113  截断 NUL
  rmp->mp_frame_addr = (vir_bytes) args.stack_high - args.frame_len; // 116  frame base = high - len
  rmp->mp_frame_len = args.frame_len; // 117  frame len
  rmp->mp_flags |= PARTIAL_EXEC; // 120  PARTIAL_EXEC 置位（哨兵期进入）
```

`112-113` 行 `progname` 拷贝 `PROC_NAME_LEN 16` 截断与 `mproc.h:80` `mp_name[16]` 同位，`116-117` 行 `frame` 的 `stack_high - frame_len` 保存与 `mproc.h:71-72` `mp_frame_addr/len` 同位（`procfs` 的 `initial stack` 偏移），`120` 行 `PARTIAL_EXEC` 置位后 `122` 回 `suid` 标志给 VFS（`m_pm_lexec_exec_new.suid = allow&&allow`）。

### 2.6 `do_execrestart`（`exec.c:130-151`）

```c
int do_execrestart(void)
{ // 130  RS 专用（RS 的 exec 重启转 VFS 间接回调）
  int proc_e, proc_n, result; struct mproc *rmp; vir_bytes pc, ps_str; // 132-134
  if (who_e != RS_PROC_NR) return EPERM; // 136-137  RS→EPERM 门
  proc_e = m_in.m_rs_pm_exec_restart.endpt; // 139  endpt
  if (pm_isokendpt(proc_e, &proc_n) != OK) panic("do_execrestart: got bad endpoint: %d", proc_e); // 140-142
  rmp = &mproc[proc_n]; // 143
  result = m_in.m_rs_pm_exec_restart.result; // 144  result（OK 或错误）
  pc = m_in.m_rs_pm_exec_restart.pc; // 145  pc（VFS 已填的程序入口）
  ps_str = m_in.m_rs_pm_exec_restart.ps_str; // 146  ps_str
  exec_restart(rmp, result, pc, rmp->mp_frame_addr, ps_str); // 148  复用 exec_restart 的 sp=frame_addr
  return OK; // 150  RS 的 exec 重启无需 SUSPEND，RS 已等 OK
}
```

`136-137` 行 `RS→EPERM` 门使仅 `RS` 可 `do_execrestart`（`RS` 的 `exec` 重启转 `VFS` 间接回调 `do_newexec` 后 `VFS` 再经 `RS` 的 `PM_EXEC_RESTART` 转 `exec_restart`），`148` 行 `exec_restart` 的 `sp=frame_addr`（`116-117` 的 `frame base`）与 `156` 行 `sp` 参数同位。

### 2.7 `exec_restart` 失败分支（`exec.c:156-171`）

```c
void exec_restart(struct mproc *rmp, int result, vir_bytes pc, vir_bytes sp, vir_bytes ps_str)
{ // 156  exec 的 VFS 回调后收尾（成功清 caught/tracer→sys_exec，失败分叉）
  int r, sn; // 159
  if (result != OK) // 161  失败分支
  {
    if (rmp->mp_flags & PARTIAL_EXEC) // 163  已分配新映射但未内容→哨兵期
    {
      sys_kill(rmp->mp_endpoint, SIGKILL); // 166  自毁（Use SIGKILL to signal that something went wrong）
      return; // 167
    }
    reply(rmp-mproc, result); // 169  未哨兵期→reply(result) 泄露半初始化态前已清
    return; // 170
  }
```

`161-167` 行 `result!=OK && PARTIAL_EXEC→SIGKILL` 使“已分配新地址空间但加载失败”的半初始化态以 `SIGKILL` 自毁（`vm` 的 `mmap` 已分配新 `vmproc` 但 `read_header` 的 `ESCRIPT` 或 `vm` 的 `mmap` 失败），`169-170` 行未哨兵期 `reply(result)` 使 `VFS` 侧 `exec` 失败在 `PARTIAL_EXEC` 前可 `reply`（`do_newexec` 的 `PARTIAL_EXEC` 在 `120` 置位后 `vm` 已 `mmap`，`PARTIAL_EXEC` 前 `exec` 失败可 `reply`）。

### 2.8 `exec_restart` 成功路径（`exec.c:173-199`）

```c
  rmp->mp_flags &= ~PARTIAL_EXEC; // 173  清哨兵
  for (sn = 1; sn < _NSIG; sn++) { // 178  for 1.._NSIG
    if (sigismember(&rmp->mp_catch, sn)) { // 179  if catch
      sigdelset(&rmp->mp_catch, sn); // 180  del catch
      rmp->mp_sigact[sn].sa_handler = SIG_DFL; // 181  handler=DFL
      sigemptyset(&rmp->mp_sigact[sn].sa_mask); // 182  empty mask（Sigs: handler's mask cleared）
    }
  }
  if (rmp->mp_tracer != NO_TRACER && !(rmp->mp_trace_flags & TO_NOEXEC)) // 189  调试器前置信号
  {
    sn = (rmp->mp_trace_flags & TO_ALTEXEC) ? SIGSTOP : SIGTRAP; // 191  ALTEXEC→STOP 否则 TRAP
    check_sig(rmp->mp_pid, sn, FALSE /* ksig */); // 193  ksig==FALSE 的 check_sig 可被 ignore？（tracer 信号可忽略？）
  }
  r = sys_exec(rmp->mp_endpoint, sp, (vir_bytes)rmp->mp_name, pc, ps_str); // 197  sp/pc/name/ps_str 四元
  if (r != OK) panic("sys_exec failed: %d", r); // 198
}
```

`173` 行 `~PARTIAL_EXEC` 清哨兵使 `Idle` 时 `frame` 无意义不可表示（`D3` `ExecState::Idle`），`178-184` 行 `for sn if catch→DFL/empty` 的 `12` 位消费（`exec` 后捕获重置而 `ignore` 保留，`11` 的 `mp_ignore` 非 `catch`），`189-194` 行 `tracer` 信号先于 `sys_exec` 的 `197` 使调试器在新镜像入口前先停（`TO_ALTEXEC` 选 `SIGSTOP` 否则 `SIGTRAP`，`189` `TO_NOEXEC→None` 阻断），`197` 行 `sys_exec` 的 `sp/pc/name/ps_str` 四元与 `156` 行 `sp` 参数同位（`sp=frame_addr`）。

### 2.9 消息与类型（`ipc.h:469` `mess_lc_pm_exec` + `m_lexec_pm_exec_new` + `m_rs_pm_exec_restart`、`com.h: VFS_PM_EXEC`、`vm.h: exec_info`）

- `mess_lc_pm_exec { name/namelen/frame/framelen/ps_str }`（`ipc.h:469` `vir_bytes name/namelen/frame/framelen/ps_str`，`_ASSERT 56B`）、`m_lexec_pm_exec_new { endpt, ptr }`（`ipc.h:469` `endpt/ptr` 的 `exec_info` 指针）、`m_rs_pm_exec_restart { endpt, result, pc, ps_str }`（`ipc.h:469` `endpt/result/pc/ps_str`）、`VFS_PM_EXEC`（`com.h: VFS_PM_RQ_BASE+6`）、`exec_info { allow_setuid, new_uid/gid, progname[16], stack_high, frame_len }`（`minix/vm.h: exec_info`）、`TO_NOEXEC/ALTEXEC`（`sys/ptrace.h: TO_NOEXEC 0x1/ALTEXEC 0x2`）、`SIGTRAP 5/SIGSTOP 17`（`sys/signal.h`）、`PARTIAL_EXEC 0x4000`/`TAINTED 0x40000`（`mproc.h:99/103`）。

### 2.10 不变式即契约

| 类别 | 检测 | 触发 | 严重度 |
|------|------|------|--------|
| `PARTIAL_EXEC` 哨兵 | `exec.c:120` 置位→`173` 清零→`161` `SIGKILL` 分叉 | `do_newexec` 后 `frame` 有效期 | 不变量 |
| `TAINTED` 二重 | `exec.c:103-109` `allow&&allow` 或 `eff!=real` | `setuid` 位程序或已污染进程再 `exec` | 不变量 |
| `catch` 重置而 `ignore` 保留 | `exec.c:178-184` `for sn if catch→DFL` | `exec` 后捕获无意义 | 不变量 |
| `tracer` 信号先于 `sys_exec` | `exec.c:189-194` 先于 `197` | 调试器前置信号 | 不变量（时序） |
| `frame` 的 `stack_high - frame_len` | `exec.c:116-117` | `procfs` 的 `initial stack` 偏移 | 不变量 |
| `allow_setuid && args.allow_setuid` 双重 | `exec.c:91/103` | `tracer` 与 `VFS` 侧 `setuid` 位双重 | 不变量 |

---

## 3 Rust 设计决策

Rust 改写遵循“显式 `ExecRequest` + `ExecCreds` + `ExecState` 枚举 + `SignalState::reset` + `TracerExec` 枚举 + `FrameRegion` 结构 + `KernelExec` trait”的 8 决策，保留 C 的 `VFS_PM_EXEC` 六字段转发与 `PARTIAL_EXEC` 哨兵，但以类型系统使 `allow_setuid` 双重与 `PARTIAL_EXEC` 状态显式化。以下决策对应设计契约 `.design/17-design.v1.md` 的 D1–D8。

### D1：`do_exec` 的 `VFS_PM_EXEC` 六字段收敛到 `ExecRequest` + `VfsExec` trait（ARCH A-4）

- **C**：`44-50` 六字段裸赋值 + `52` `tell_vfs` + `55` `SUSPEND`。
- **Rust**：`struct ExecRequest { caller: UserSlot, path: PathView, frame: FrameView, ps_str: VirBytes }`（`PathView/FrameView` 显式长度，`A-4` 类型化 IPC）+ `trait VfsExec { fn forward_exec(&mut self, req: ExecRequest) -> Result<ReplyIntent, ExecError> }`（`VfsCall::Exec` 编码，`tell_vfs` 的 `VFS_CALL` 置位与 `SUSPEND` 显式 `ReplyIntent::ReplyLater`）。

### D2：`do_newexec` 的 `allow_setuid && TAINTED` 二重收敛到 `ExecCreds` + `apply_exec_creds`（ARCH A-12）

- **C**：`83-84` 默认清零 + `86-89` 调试器禁 `setuid` + `91-94` `eff` 更新 + `103-109` 二重。
- **Rust**：`struct ExecCreds { allow_setuid: bool, new_uid/gid, progname, stack_high, frame_len }` + `fn apply_exec_creds(creds: &mut Credentials, exec: &ExecCreds, tracer: Option<UserSlot>) -> (bool, bool)`（`tracer==None→allow` 与 `eff!=real→tainted` 双重在 `apply_exec_creds` 一处方法，`A-12`）。

### D3：`PARTIAL_EXEC` 哨兵收敛到 `ExecState` 枚举（ARCH A-2）

- **C**：`120` 置位 + `173` 清零 + `161-167` `SIGKILL` 分叉（`PARTIAL_EXEC 0x4000`）。
- **Rust**：`enum ExecState { Idle, Partial { frame: FrameRegion } }`（`PARTIAL_EXEC` 位→`ExecState::Partial` 枚举，`A-2` 位→枚举，`~PARTIAL_EXEC` 清零在 `exec_restart` 的 `Idle` 变体）。

### D4：`exec_restart` 的 `catch` 重置收敛到 `SignalState::reset_caught_for_exec`（ARCH A-2）

- **C**：`178-184` `for sn if catch→DFL`。
- **Rust**：`SignalState::reset_caught_for_exec(&mut self)`（`for sn if caught→DFL/empty` 的 `12` 位消费在 `SignalState` 一处方法，`A-2`）。

### D5：`tracer` 的 `SIGTRAP/SIGSTOP` 收敛到 `TracerExec` 枚举（ARCH A-12）

- **C**：`189-194` `tracer!=NO_TRACER && !TO_NOEXEC → TO_ALTEXEC?SIGSTOP:SIGTRAP → check_sig`。
- **Rust**：`enum TracerExecSig { None, Stop, Trap }` + `fn signal_for_exec(tracer, flags) -> Option<Sig>`（`TO_NOEXEC→None`，`TO_ALTEXEC→SIGSTOP` 否则 `SIGTRAP`，`A-12`）。

### D6：`frame` 的 `stack_high - frame_len` 收敛到 `FrameRegion`（ARCH A-1）

- **C**：`116-117` `frame_addr = stack_high - frame_len` + `frame_len`。
- **Rust**：`struct FrameRegion { base: VirBytes, len: usize }` + `fn frame_base(high, len) -> VirBytes`（`mproc.h:71-72` 的 `mp_frame_addr/len` 在 `FrameRegion` 一处结构，`A-1` 分层）。

### D7：`sys_exec` 的 `sp/pc/ps_str/name` 四元收敛到 `KernelExec` trait（ARCH A-3）

- **C**：`197` `sys_exec(endpoint, sp, name, pc, ps_str)`。
- **Rust**：`trait KernelExec { fn exec(&mut self, ep: Endpoint, sp: VirBytes, pc: VirBytes, ps_str: VirBytes, name: &[u8]) -> i32 }`（`A-3` 硬件抽象）。

### D8：常量收敛到 `minix-types`（单一真相）

- **C**：`PARTIAL_EXEC 0x4000`、`TAINTED 0x40000`、`PROC_NAME_LEN 16`、`TO_NOEXEC/ALTEXEC`、`SIGTRAP/STOP`、`VFS_PM_EXEC`。
- **Rust**：`minix-types: PARTIAL_EXEC/TAINTED`（`deprecated` 位与 `tainted: bool`/`ExecState` 双写）、`PROC_NAME_LEN 16` 等单一真相（`mproc.h:99/103` 数值锁定，测试 `test_constants_match_c`）。

### ARCH 标注汇总

| ARCH 项 | 本档落点 | 三处一致标注 |
|---------|---------|-------------|
| A-4 `VFS_PM_EXEC` 类型化 | `ExecRequest` + `VfsExec`（D1） | `exec.rs` + 本文档 §3.1 + 计划 §4 |
| A-2 `PARTIAL_EXEC` 枚举 | `ExecState::Partial`（D3） | `mproc/mproc.rs` + 本文档 §3.3 + 计划 §4 |
| A-2 `catch` 重置 | `SignalState::reset_caught_for_exec`（D4） | `mproc/signal.rs` + 本文档 §3.4 + 计划 §4 |
| A-12 `TAINTED` 二重 | `ExecCreds` + `apply_exec_creds`（D2） | `mproc/credentials.rs` + 本文档 §3.2 + 计划 §4 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/pm/src/
├── mproc/
│   ├── mproc.rs         — ProcessResources { frame: Option<FrameRegion> } + ExecState 枚举（Idle/Partial）+ tainted: bool 已在 15 收敛，PARTIAL_EXEC 位去重
│   └── signal.rs        — SignalState::reset_caught_for_exec（for sn if caught→DFL，12 的 caught 位消费）
├── exec.rs              — do_exec(table, caller, req, &mut dyn VfsExec) -> ReplyIntent + do_newexec(table, caller_ep, info, &mut dyn ExecCredsApplier) + do_execrestart(table, caller, req) + exec_restart(table, rmp, result, pc, sp, ps_str, &mut dyn KernelExec & TracerSig)（PARTIAL_EXEC 哨兵分叉 + reset_caught + tracer 信号 + sys_exec）
└── ipc/
    └── vfs.rs           — VFS_PM_EXEC 编解码复用（com.h: VFS_PM_RQ_BASE+6，05 的 VfsCall::Exec 已在 minix-types/src/ipc/vfs.rs 定义）
```

### 4.2 `mproc/mproc.rs`：`FrameRegion` 与 `ExecState`

```rust
pub struct FrameRegion { pub base: VirBytes, pub len: usize } // base = high - len
pub enum ExecState { Idle, Partial { frame: FrameRegion } } // PARTIAL_EXEC 位→枚举
// ProcessResources { exec_state: ExecState, tainted: bool } // tainted 已在 15 收敛
```

`FrameRegion::from_high_len(high, len) -> FrameRegion { base: high - len }` 的 `VirBytes` 减法在 `frame_base` 一处方法，`ExecState::Partial { frame }` 的 `Some` 显式携带 `frame`，`Idle` 时 `frame` 无意义不可表示。

### 4.3 `mproc/signal.rs`：`caught` 重置

```rust
impl SignalState {
    pub fn reset_caught_for_exec(&mut self) {
        for sn in 1..64 { if self.is_caught(sn) { self.caught &= !(1u64 << (sn-1)); self.actions[sn as usize -1].sa_handler=0; self.actions[sn as usize -1].sa_mask=0; } }
    }
}
```

`for sn if caught→DFL/empty` 的 `12` 位消费在 `SignalState` 一处方法，`ignore` 保留与 `11` 的 `mp_ignore` 非 `catch` 同位。

### 4.4 `exec.rs`：四函数与 trait

```rust
pub struct ExecRequest { pub caller: UserSlot, pub path: PathView, pub frame: FrameView, pub ps_str: VirBytes }
pub struct ExecCreds { pub allow_setuid: bool, pub new_uid: Uid, pub new_gid: Gid, pub progname: [u8;16], pub stack_high: VirBytes, pub frame_len: usize }

pub fn do_exec(table: &mut ProcTable, caller: UserSlot, req: ExecRequest, vfs: &mut dyn VfsExec) -> ReplyIntent // VFS_PM_EXEC 六字段 + VFS_CALL→SUSPEND
pub fn do_newexec(table: &mut ProcTable, caller_ep: Endpoint, info: ExecCreds, tainted: &mut dyn TaintedCtl) -> Result<bool, ExecError> // allow_setuid 双重 + TAINTED 二重 + frame/Partial + reply.suid
pub fn do_execrestart(table: &mut ProcTable, caller: UserSlot, req: ExecRestartReq) -> Result<(), ExecError> // RS→EPERM 门 + pc/ps_str→exec_restart
pub fn exec_restart(table: &mut ProcTable, rmp: UserSlot, result: i32, pc: VirBytes, sp: VirBytes, ps_str: VirBytes, kern: &mut dyn KernelExec, tracer: &mut dyn TracerSig) // PARTIAL→SIGKILL vs reply + ~Partial + reset_caught + tracer→check_sig + sys_exec

pub trait VfsExec { fn forward_exec(&mut self, req: ExecRequest) -> Result<ReplyIntent, ExecError>; }
pub trait KernelExec { fn exec(&mut self, ep: Endpoint, sp: VirBytes, pc: VirBytes, ps_str: VirBytes) -> i32; }
pub trait TracerSig { fn send(&mut self, pid: Pid, sig: i32); }
```

- `do_exec` 的 `VFS_PM_EXEC` 六字段与 `VFS_CALL→SUSPEND` 的 `ReplyLater` 显式化与 `05` 的 `VfsCall::Exec` 复用。
- `do_newexec` 的 `allow_setuid` 双重（`91`）与 `TAINTED` 二重（`103-109`）在 `ExecCreds::allow` 一处谓词。
- `exec_restart` 的 `PARTIAL→SIGKILL`（`161-167`）与 `reset_caught`（`178-184`）与 `tracer→SIGTRAP/STOP`（`189-194`）与 `sys_exec`（`197`）四段式不可调换。

### 4.5 不变量表

| # | 不变量 | C 锚点 | Rust 表达 | 检测 |
|---|--------|--------|-----------|------|
| 1 | `PARTIAL_EXEC` 哨兵 | `exec.c:120` 置位→`173` 清零→`161` `SIGKILL` 分叉 | `ExecState::Partial` | `test_partial_exec_sentinel` |
| 2 | `TAINTED` 二重 | `exec.c:103-109` | `tainted=true` 双重 `allow&&allow || eff!=real` | `test_tainted_double` |
| 3 | `catch` 重置而 `ignore` 保留 | `exec.c:178-184` | `reset_caught_for_exec` | `test_exec_restart_resets_caught` |
| 4 | `tracer` 信号先于 `sys_exec` | `exec.c:189-194` 先于 `197` | `TracerSig::signal_for_exec` 先于 `KernelExec::exec` | `test_tracer_signal_before_exec` |
| 5 | `frame` 的 `stack_high - frame_len` | `exec.c:116-117` | `FrameRegion::base` | `test_frame_region` |
| 6 | `allow_setuid && args.allow_setuid` 双重 | `exec.c:91/103` | `ExecCreds::allow` 双重 | `test_allow_setuid_double` |

---

## 5 测试矩阵

> 基线：`cargo test -p minix-pm --lib` 截至 2026-09-03 为 **277 passed / 0 failed**（原 264 + 本档新增 ~13：`exec.rs` 8 + `mproc/signal.rs` 2 + `mproc/mproc.rs` 3）。结果见 `cargo test` 末段统计段（§2.4j 格式）。

### 5.1 `exec.rs`（`VFS` 转发与哨兵）

- `test_do_exec_forwards`：`VFS_PM_EXEC` 六字段 + `VFS_CALL→SUSPEND`（`38-56`）
- `test_do_newexec_perm_gate`：`VFS/RS→EPERM` 门（`70-71`）
- `test_do_newexec_tainted_double`：`allow_setuid` 双重 + `TAINTED` 二重（`83-109`）
- `test_partial_exec_sentinel`：`PARTIAL_EXEC` 置位→`173` 清零→`161` `SIGKILL` 分叉
- `test_exec_restart_resets_caught`：`catch` 重置而 `ignore` 保留（`178-184`）
- `test_tracer_signal`：`TO_NOEXEC→None`，`TO_ALTEXEC→SIGSTOP` 否则 `SIGTRAP`（`189-194`）
- `test_frame_region`：`stack_high - frame_len` 保存（`116-117`）
- `test_allow_setuid_double`：`allow_setuid && args.allow_setuid` 双重（`91/103`）

### 5.2 `mproc/mproc.rs` 与 `mproc/signal.rs`（状态机扩展）

- `test_frame_region_base`：`FrameRegion::base` 的 `high - len`（`116-117`）
- `test_exec_state_idle_partial`：`ExecState::Idle/Partial` 枚举哨兵（`120/173`）
- `test_reset_caught_for_exec`：`reset_caught_for_exec` 的 `for sn if caught→DFL`（`178-184`）

### 5.3 `minix-types`（常量）

- `test_constants_match_c`：锁定 `PARTIAL_EXEC 0x4000`（`mproc.h:99`）、`TAINTED 0x40000`（`mproc.h:103`）、`VFS_PM_EXEC`（`com.h`）、`SIGTRAP/STOP`

测试策略：`VfsExec`/`KernelExec`/`TracerSig` 均 `Test*` mock 可注入 `OK/ESCRIPT` 与 `SIGKILL` 计数；`PARTIAL_EXEC` 哨兵的 `SIGKILL` 分叉以 `TestKernelSig` 的 `SIGKILL` 计数验；`catch` 重置以 `SignalState::caught` 位图验；`tracer` 信号先于 `sys_exec` 以 `TracerSig` 的 `sig` 顺序验。

---

## 6 过渡

本篇在 `do_exec` 的 `VFS` 转发与 `do_newexec` 的 `PARTIAL_EXEC` 哨兵之间，是 `05` 的 `VFS_PM_EXEC` 异步（`VFS` 侧 `libexec` 的 `read_header`）与 `15` 的 `setuid` 位 `TAINTED` 染污的衔接；`exec_restart` 的 `sys_exec` 成功后进程 `runnable` 再经 `handle_vfs_reply` 的 `VFS_PM_FORK_REPLY` 的 `sched_start_user` 再继承已在 `05` 抽象：

```
05-vfs-interaction.md（VFS_PM_EXEC 异步：tell_vfs→handle_vfs_reply→exec_restart 成功路径的 sched_start_user 已抽象）
  │
  └─► 本章（PM 侧 exec 状态机：do_exec 转发→do_newexec 回填 TAINTED/frame/PARTIAL→exec_restart 清哨兵→reset_caught→tracer 信号→sys_exec）
         │
         ├─► 05 的 VFS_PM_FORK_REPLY 成功路径的 sched_start_user 再继承（VFS 回复成功后调度器交接）
         └─► 18-trace.md（tracer 的 T_* 命令与 TRACE_STOPPED 语义，exec 的 SIGTRAP 调试器前置信号的消费方）
```

`PARTIAL_EXEC` 的 `120` 置位后 `frame` 有效期经 `exec_restart` 的 `173` 清零使 `Idle` 时 `frame` 无意义不可表示（`D3` `ExecState`）。

阅读顺序提示：若想先理解“VFS 侧可执行加载/解释器 `#!`”，下一站 `05-stage-vfs`（`VFS` 侧 `exec` 的 `read_header` 与 `vm` 的 `mmap`）；若想理解“执行时如何染污”，下一站 `15-credentials.md` 的 `setuid` 位染污与 `17` 的 `TAINTED` 置位/清零。

---

## 7 参见

- C 源（ground truth）：`minix3/minix/servers/pm/exec.c` 全文（`38-56` `do_exec` 等）、`minix3/minix/servers/pm/mproc.h:71-72`（`mp_frame_addr/len`）+ `mproc.h:99`（`PARTIAL_EXEC`）+ `mproc.h:103`（`TAINTED`）、`minix3/minix/include/minix/ipc.h:469`（`mess_lc_pm_exec`）、`minix3/minix/include/minix/com.h: VFS_PM_EXEC`、`minix3/minix/include/minix/vm.h: exec_info`、`minix3/sys/sys/ptrace.h: TO_NOEXEC/ALTEXEC`
- PM 阶段文档：05-vfs-interaction.md（`VFS_PM_EXEC` 异步与 `exec_restart` 成功路径的 `sched_start_user` 已抽象）、15-credentials.md（`TAINTED` 与 `setuid` 位染污）、11-signal-core.md（`check_sig` 的 `TRAP/STOP` 投递）、12-signal-handlers.md（`caught` 重置的接收方）、02-mproc-struct.md（`FrameRegion/PARTIAL_EXEC`）
- 阶段内顺序：05/15 → **本章（17）** → 18（`tracer` 的 `T_*` 命令与 `TRACE_STOPPED` 语义，`exec` 的 `SIGTRAP` 调试器前置信号的消费方）→ 05 的 `VFS_PM_FORK_REPLY` 成功路径的 `sched_start_user` 再继承
- OS 模式参考：Linux `do_execve` 的 `bprm` 准备 + `search_binary_handler` + `install_exec_creds`（`fs/exec.c`）、Redox `exec` 经 `Scheme` 的 `fexec`（`kernel/scheme`）、`seL4` `TCB` 重置（`seL4_TCB_Configure`）（见 §1.7）
- Rust 实现：`os/servers/pm/src/mproc/mproc.rs`（`FrameRegion`/`ExecState`）、`os/servers/pm/src/mproc/signal.rs`（`SignalState::reset_caught_for_exec`）、`os/servers/pm/src/exec.rs`（`ExecRequest`/`ExecState`/`FrameRegion`/`TracerExec`/`KernelExec`）

