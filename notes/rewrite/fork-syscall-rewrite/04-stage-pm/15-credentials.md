# 15 — 身份与凭证：`do_get`/`do_set` 的三元组、`TAINTED` 与 VFS 协同

本文讲清 PM 的身份与凭证如何以“`real/effective/saved` 三元组 + `supplemental_groups` 16 组 + `TAINTED` 污染位 + `VFS` 双副本协同”为完整链路，使 `getuid/setuid/seteuid/setgid/setegid/setgroups/getgroups/getpid/getpgrp/getsid/issetugid` 的 13 个调用在 `SUPER_USER` 判据与 `NGROUPS_MAX/GID_MAX` 边界与 `VFS_PM_SET*` 的 `tell_vfs→SUSPEND→Set*Reply→reply(OK)` 双向闭环中可区分。

前置阅读：02-mproc-struct.md（`Credentials { user:IdSet<Uid>, group:IdSet<Gid>, ngroups/supplemental_groups }` 三元组 + `RemainingFlags::TAINTED` + `NGROUPS_MAX 16`）、05-vfs-interaction.md（`tell_vfs` 的 `NotIdle` 守卫 + `handle_vfs_reply` 的 `Set*` 双分支 `VFS_PM_SETUID→reply(OK)` 等 4 路）、04-ipc-dispatch.md（`ReplyIntent::ReplyLater` 的 `SUSPEND` 第二子类——`do_set` 的“等待 VFS”与 `do_exit` 的“永不回复”区分）。

---

## 1 概念

### 1.0 目标读者与边界

**目标读者**：已理解 `ProcTable` 的 `NGROUPS_MAX 16` 与 `GID_MAX` 边界（02）、`VFS` 协议的 `VFS_CALL` 置位与 `SUSPEND` 解挂（05）的开发者；知道 `Uid/Gid` 的 `Pid` 型别与 `SUPER_USER 0` 语义。

> **本章不讲什么**：
> - `exec` 的 `setuid` 位与 `TAINTED` 清零/置位（`exec.c:84-108` 的 `~TAINTED` + `setuid` 位判断）—— `17-exec.md`
> - `fork` 的 `TAINTED` 继承（`07-pm-fork.md` 的 `fork_from` 仅 `TAINTED` 位保留）
> - 调度 `nice` 检查（`misc.c:do_getsetpriority` 的 `eff!=SUPER_USER`）—— `16-scheduling.md`
> - 内核 `sys_getksig` 等信号权限的 `SUPER_USER` 四重—— `11-signal-core.md` 已述
>
> 本章只回答一个问题：**PM 如何为“查身份直接给数、改身份先验权再改三元再告诉文件系统等回信”的 13 个 `get/set` 调用建立 `real/effective/saved` 三元与 `TAINTED` 位的 `VFS` 双副本协同，并使 `GETGROUPS` 的“先问再拷”二阶段与 `SETSID` 的会话首语义可区分**。

### 1.1 为什么 PM 需要三元组 `real/effective/saved`：审计、可切换、可回退

`uid` 的三元组在 `mproc.h:40-46` 的 `real/effective/saved` 与 `minix_types::IdSet<Uid>` 的 `real/effective/saved` 同构：

- `real` 是“谁创建的我”（审计源，`getuid` 返回 `real`，`setuid` 的 `real!=uid` 判据即此）；
- `effective` 是“我以谁的权限行事”（访问检查，`is_superuser` 的 `eff==0` 即此）；
- `saved` 是“上一次的 `effective`”（`seteuid(saved)` 可回退到上一次的 `effective`，`seteuid` 的 `saved!=uid` 三重判据即此）。

`gid` 三元同理（`real/eff/saved` 的 `IdSet<Gid>`）。`seteuid` 的 `real/saved/eff!=SUPER_USER→EPERM` 三重（`getset.c:131-133`）使“已保存的 `root` 可切回 `root`”而 `real` 仅作审计——`real` 不变仅 `eff` 变的 `seteuid` 是 `real==uid` 的审计直通。

### 1.2 为什么 `setuid` 有 BSD 全置语义：`real=eff=saved=uid` 的原子

`PM_SETUID(uid)` 的 `114-119` 三段：

```
real!=uid && eff!=0 → EPERM（114-115）
real=eff=saved=uid（117-119）BSD 全置
VFS_PM_SETUID 编码 ENDPT/EID/RID（121-125）
```

BSD 全置使“成功后三元一致”的原子语义：`setuid(geteuid())` 的 `real!=uid` 但 `eff==0` 时可成功并三元全置为 `uid`（`real` 审计被改），与 `seteuid` 的仅 `eff` 单置对偶（`134` `eff=uid`）。`getset.c:113` 注释 *NetBSD specific semantics: setuid(geteuid()) may fail* 即此 `real` 判据的 BSD 特异。

### 1.3 为什么 `setuid` 成功的进程必须同步 VFS：双副本一致

`uid/gid` 在 VFS 侧另有一份 `fproc` 副本（`05-stage-vfs/02-fproc-struct.md` 的 `fp_realuid/fp_effuid` 族），`do_set` 成功后的 `tell_vfs(VFS_PM_SETUID)` 的 `VFS_CALL` 置位 + `SUSPEND`（`getset.c:219-222` 三段式：编码 `121-125` → `tell_vfs` → `SUSPEND`）使两份表一致（`05` 的 `handle_vfs_reply` 的 `VFS_PM_SETUID→SetGid/SETEGID` 双分支 `reply(OK)` 解挂）。

`GROUPS` 亦双副本：`mp_sgroups` 16 组在 PM，`fproc` 的组列表在 VFS，`VFS_PM_SETGROUPS` 的 `GROUP_NO/GROUP_ADDR` 编码（`199-203`）使 VFS 侧 `fproc` 组列表与 `ngroups` 同步。

### 1.4 为什么 `GETGROUPS` 有 `ngroups==0` 查询语义：先问再拷的二阶段

`PM_GETGROUPS` 的 `34-41` 两段：

```
ngroups==0 → r=ngroups（34-36）“先问有多少组”的查询（getgroups(0,NULL)）
ngroups<avail → EINVAL（39-41）“提供的缓冲区小于实际”显式失败而非截断 + sys_datacopy 拷出（43-47）
```

`GETGROUPS` 的 `ngroups==0` 查询使“先 `getgroups(0,NULL)` 问 `avail`，再分配 `avail` 大小缓冲区 `getgroups(avail,buf)`”的两阶段查询在 PM 侧一次 `do_get` 完成（`r=ngroups` 的 `rc` 返回 `avail`），`SETGROUPS` 的 `ptr==0→EFAULT`（`181-182`）与此对偶（`copy_from_user` 的空指针显式 `EFAULT`）。

### 1.5 为什么 `setsid` 的 `procgrp==pid→EPERM`：会话首语义

`setsid` 的 `206-207` 两段：

```
procgrp==pid → EPERM（206）“已是会话首则失败”
procgrp=pid（207）“成为新会话首”为 `procgrp=pid`（A-13 进程组/会话语义，procgrp==pid 即“我是组长”）
```

`procgrp` 是进程组（`mproc.h:30` `mp_procgrp`），`pid` 相等即“组长”（会话首的进程组与 `pid` 相等为首），`GETSID` 的 `p?find_proc(p):who_p→procgrp`（`74`）与此对偶（`getsid(pid)` 返回 `target` 的 `procgrp`，`pid==0→who_p` 的 `caller` 自查）。

### 1.6 为什么 `TAINTED` 与 `issetugid`：污染位的 `LD_PRELOAD` 防注入

`exec` 的 `setuid` 位使进程被污染（`exec.c:84` `~TAINTED` 默认清零 + `105/108` `TAINTED` 置位），`PM_ISSETUGID` 的 `!!(flags & TAINTED)`（`getset.c:81` `TAINTED 0x40000`）是 `issetugid(2)` 的 `LD_PRELOAD` 防注入检查（`libexec` 的 `rtld` 路径隔离，`issetugid` 为真时不加载用户 `LD_LIBRARY_PATH`）。

`TAINTED` 的 `RemainingFlags::TAINTED` 在 `D5` 以 `tainted: bool` 唯一真源消除位与 `exec` 散落的双重性（`A-12`）。

### 1.7 与其他 OS 凭证的对照

Rust 改写不是照抄 `mp_realuid != uid && mp_effuid != SUPER_USER` 条件，而是在吸收工业级 OS 的成熟模式后做取舍。

**Linux `cred` 的四元 + capabilities。** Linux `cred { uid,euid,suid,fsuid; gid,egid,sgid,fsgid; groups }` + `capabilities(7)` 的 `cap_setuid/cap_setgid` 以 `capable(CAP_SETUID)` 替代 `eff==SUPER_USER` 的位判据，`setuid` 的 `real/effective/saved/fsuid` 四元全置（`kernel/sys.c:SYSC_setuid`）与 Minix3 的 `real/eff/saved` 三元同源但多 `fsuid`（`VFS` 侧 `fproc` 的 `fsuid` 在 Minix3 为 `VFS_PM_SET*` 双副本协同而非 `fsuid` 四元）。Linux 的 `groups` 以 `cred->group_info` 的 `flex_array` 变长（`NGROUPS_MAX 65536`），Minix3 以 `mp_sgroups[16]` 固定 16 组（`mproc.h:50` `NGROUPS_MAX 16`）——16 组是微内核进程表固定大小的产物（`mproc` 约 480B，16 组占 64B），`Vec<Gid>` 在 Rust 侧变长但 `GID_MAX` 越界仍 16 组钳位。

**Redox `Context::uid/gid` 的 `RwLock<Credentials>`。** Redox 以 `context::Context { uid: Uid, gid: Gid, groups: Vec<Gid> }` + `scheme::Scheme` 的 `RwLock` 句柄与 `potassium` 的 `capability` 隔离，`getuid/setuid` 经 `context::current().uid` 的 `RwLock` 读/写（`kernel/context/context.rs`），VFS 侧 `Scheme` 的 `uid` 检查在 `open` 时以 `Context` 快照而非 `fproc` 双副本——Redox 无 `TAINTED` 位（`setuid` 位的污染经 `Scheme` 的 `setuid` 计数），Minix3 的 `TAINTED` 位为 `rtld` 的 `issetugid` 快捷。

**`seL4` 无 `uid` 的 capability 纯化。** `seL4` 无 `uid/gid`，以 `CNode` + `capability` 的派生与撤销替代身份（`seL4_CNode_Mint` 的 `badge`），PM 的 `SUPER_USER 0` 位判据在 `seL4` 以“是否有 `CAP_SYS_ADMIN` 的 `capability`”显式，`TAINTED` 的污染在 `seL4` 以 `capability` 的 `badge` 隔离——纯能力模型的“无 `uid` 世界”与 Minix3 的“三元 `uid` + `SUPER_USER` 位”对偶，Rust 以 `is_superuser` 一处谓词收敛。

**结论（本章的设计基线）。** 把 C 的“`mp_realuid != uid && mp_effuid != SUPER_USER` 三重散落 + `TAINTED` 位与 `exec` 散落 + `NGROUPS_MAX/GID_MAX` 双重 + `VFS_PM_SET*` 的 `tell_vfs→SUSPEND` 成功后 `SUSPEND`”改写为“`Credentials::{set_uid_all/set_euid/set_gid_all/set_egid}` + `is_superuser` 一处谓词 + `tainted: bool` 唯一真源 + `CopyGroups` trait 的 `GID_MAX` 越界一处检查 + `VfsForwarder::forward_set → ReplyIntent::ReplyLater` 显式”——与 Linux/Redox 的三元 `uid` + `NGROUPS_MAX` 边界同源，又因 PM 单线程无共享而以 `&mut ProcTable` 的 `IdSet` 显式三元收敛。

### 1.8 小结

1. **为什么三元**——`real` 审计、`effective` 行权、`saved` 回退，`seteuid` 的三重判据即三元的 Rust 端 `IdSet` 物化。
2. **为什么 BSD 全置**——`setuid` 成功后 `real=eff=saved=uid` 原子，`seteuid` 仅 `eff` 单置对偶。
3. **为什么双副本**——`VFS` 侧 `fproc` 另有一份 `uid/gid/groups`，`tell_vfs(VFS_PM_SET*)` 的 `VFS_CALL` + `SUSPEND` 使两份表一致（`05` 的 `Set*→reply(OK)` 解挂）。
4. **为什么二阶段查询**——`GETGROUPS 0→count` 先问再拷，`ngroups<avail→EINVAL` 显式失败而非截断。
5. **为什么会话首**——`procgrp==pid→EPERM` 已是会话首则失败，`procgrp=pid` 成为新会话首（`A-13`）。
6. **为什么 `TAINTED`**——`exec` 的 `setuid` 位染污，`issetugid` 的 `LD_PRELOAD` 防注入（`tainted: bool` 唯一真源）。

下一章逐行分析 C 的 `do_get`/`do_set` 与 `TAINTED`；第 3 章给出 Rust 的 `GetOp`/`SetOp`/`Credentials::set_*` 与 `VfsForwarder`。

---

## 2 C 源码分析

### 2.1 `do_get` 序言（`getset.c:18-32`）

```c
int do_get(void)
{ // 18  7 分支 get 调用（real 的审计源与 eff 的行权源双值）
  register struct mproc *rmp = mp; // 24  全局 mp 伪装 vs A-3 显式 caller
  int r; int ngroups; // 25-26
  switch(call_nr) { // 28  call_nr 分派（callnr.h:PM_GETUID..ISSETUGID）
```

`24` 行 `rmp=mp` 的全局伪装在 `Rust` 以 `caller: UserSlot` 显式参替代“伪装 `mp`”（`A-3` 全局 `mp` 显式化，`11` 的 `SignalContext` 同 `A-3`），`28` 行 `switch(call_nr)` 7 分支 + `84-86` `default→EINVAL` 使 `GetOp` 枚举穷尽。

### 2.2 `PM_GETGROUPS`（`getset.c:29-50`）

```c
	case PM_GETGROUPS: // 29
		ngroups = m_in.m_lc_pm_groups.num; // 30  m_lc_pm_groups.num（ipc.h:466 `num`）
		if (ngroups > NGROUPS_MAX || ngroups < 0) return(EINVAL); // 31-32  16 上界与负值
		if (ngroups == 0) { // 34  查询：先问有多少组
			r = rmp->mp_ngroups; // 35  r=avail（rc 返回 avail，lib 再分配）
			break; // 36
		}
		if (ngroups < rmp->mp_ngroups) return(EINVAL); // 39-41  缓冲区小于实际显式失败
		r = sys_datacopy(SELF, (vir_bytes) rmp->mp_sgroups, who_e, // 43  拷出
			m_in.m_lc_pm_groups.ptr, ngroups * sizeof(gid_t)); // 44  ptr 为 vir_bytes
		if (r != OK) return(r); // 46-47  EFAULT 等透传
		r = rmp->mp_ngroups; // 49  rc 返回 avail（即使 ngroups>avail亦返回 avail）
		break; // 50
```

`31-32` 的 `NGROUPS_MAX 16`（`mproc.h:50` `NGROUPS_MAX 16`，`minix_types::NGROUPS_MAX`）与 `191` 行 `GID_MAX` 越界对偶（`get` 不检 `GID_MAX`，`set` 检），`34-36` 的 `0→count` 查询使两阶段查询在一次 `do_get` 完成，`39-41` 的 `<avail→EINVAL` 非截断（`getgroups(8,buf)` 在 `avail=12` 时 `EINVAL` 而非截断为 8）。

### 2.3 `PM_GETUID/GETGID`（`getset.c:51-59`）

```c
	case PM_GETUID: // 51
		r = rmp->mp_realuid; // 52  r=real（rc 的 real）
		rmp->mp_reply.m_pm_lc_getuid.euid = rmp->mp_effuid; // 53  reply.euid=eff（m_pm_lc_getuid.euid）
		break; // 54
	case PM_GETGID: // 56
		r = rmp->mp_realgid; // 57
		rmp->mp_reply.m_pm_lc_getgid.egid = rmp->mp_effgid; // 58  reply.egid=eff
		break; // 59
```

`52-58` 的 `real` 经 `rc` + `eff` 经 `reply` 双值在 `Rust` 以 `GetResult::Uid { real, eff }` 载荷显式（`m_pm_lc_getuid.euid` 的 `reply` 字段在 `GetResult::Uid` 中，双值无需 `reply` 副作用）。

### 2.4 `PM_GETPID`（`getset.c:61-64`）

```c
	case PM_GETPID: // 61
		r = mproc[who_p].mp_pid; // 62  r=mproc[who_p].pid（who_p 全局 + mp_parent 槽索引）
		rmp->mp_reply.m_pm_lc_getpid.parent_pid = mproc[rmp->mp_parent].mp_pid; // 63  reply.parent_pid=mproc[parent].pid
		break; // 64
```

`62` 行 `mproc[who_p].pid` 的 `who_p` 全局在 `Rust` 以 `caller: UserSlot` 显式参（`A-3`），`63` 行 `mproc[parent].pid` 的 `parent` 槽索引经 `Guardianship::parent` 显式。

### 2.5 `PM_GETPGRP`（`getset.c:66-68`）

```c
	case PM_GETPGRP: // 66
		r = rmp->mp_procgrp; // 67  r=procgrp（mproc.h:30 mp_procgrp）
		break; // 68
```

`67` 行 `procgrp` 是进程组（`A-13`），`setsid` 的 `procgrp=pid` 成为新会话首（`207`）与此对偶。

### 2.6 `PM_GETSID`（`getset.c:70-79`）

```c
	case PM_GETSID: // 70
	{ struct mproc *target; pid_t p = m_in.m_lc_pm_getsid.pid; // 72-73  m_in.pid（ipc.h:466 `pid`）
		target = p ? find_proc(p) : &mproc[who_p]; // 74  p==0→who_p（caller 自查），否则 find_proc
		r = ESRCH; // 75  ESRCH 初始
		if(target) r = target->mp_procgrp; // 76-77  target→procgrp（getsid 返回 target 的 procgrp）
		break; // 78
	}
```

`74` 行 `p?find_proc(p):&mproc[who_p]` 的 `p==0→caller` 自查与 `find_proc` 的 `ESRCH` 双路径（`75` `ESRCH` 初始后 `target→procgrp`），`Rust` 以 `PidTable::pid_of` + `Guardianship` 显式。

### 2.7 `PM_ISSETUGID`（`getset.c:80-82`）

```c
	case PM_ISSETUGID: // 80
		r = !!(rmp->mp_flags & TAINTED); // 81  TAINTED 0x40000（mproc.h:103）的位→bool
		break; // 82
```

`81` 行 `!!(flags & TAINTED)` 的位→`bool` 在 `Rust` 以 `tainted: bool` 唯一真源（`D5`），`TAINTED` 位与 `exec` 散落的双重性以 `bool` 消除。

### 2.8 `do_set` 序言（`getset.c:95-108`）

```c
int do_set(void)
{ // 95  6 分支 set 调用（set 后 tell_vfs→SUSPEND 的 VFS 协同）
  register struct mproc *rmp = mp; // 101  全局伪装
  message m; int r, i; int ngroups; uid_t uid; gid_t gid; // 102-106
  memset(&m, 0, sizeof(m)); // 108  VFS 消息清零
  switch(call_nr) { // 110
```

`101` 行 `rmp=mp` 全局伪装同 `do_get:24`，`108` 行 `memset(m,0)` 的 `VFS_PM_SET*` 消息清零在 `Rust` 以 `VfsCall::Set*` 枚举携带 `ENDPT/EID/RID` 三字段。

### 2.9 `PM_SETUID`（`getset.c:111-126`）

```c
	case PM_SETUID: // 111
		uid = m_in.m_lc_pm_setuid.uid; // 112  m_in.uid（ipc.h:466 `uid`）
		if (rmp->mp_realuid != uid && rmp->mp_effuid != SUPER_USER) return(EPERM); // 114-115  real!=uid && eff!=0→EPERM
		rmp->mp_realuid = uid; rmp->mp_effuid = uid; rmp->mp_svuid = uid; // 117-119  BSD 全置三元
		m.m_type = VFS_PM_SETUID; m.VFS_PM_ENDPT = rmp->mp_endpoint; // 121-122  ENDPT
		m.VFS_PM_EID = rmp->mp_effuid; // 123  EID=eff
		m.VFS_PM_RID = rmp->mp_realuid; // 124  RID=real
		break; // 126
```

`114-115` 的 `real!=uid && eff!=SUPER_USER→EPERM` 与 `SETEUID` 的三重 `131-133` 对偶（`SETUID` 二重判据 `real/eff` vs `SETEUID` 三重 `real/saved/eff`），`117-119` 的 `real=eff=saved=uid` BSD 全置原子（成功后三元一致）与 `SETEUID` 的 `134` `eff=uid` 单置对偶。

### 2.10 `PM_SETEUID`（`getset.c:128-141`）

```c
	case PM_SETEUID: // 128
		uid = m_in.m_lc_pm_setuid.uid; // 129
		if (rmp->mp_realuid != uid && rmp->mp_svuid != uid && // 131  real/saved 的可回退
		    rmp->mp_effuid != SUPER_USER) return(EPERM); // 132-133  三重
		rmp->mp_effuid = uid; // 134  仅 eff 单置
		m.m_type = VFS_PM_SETUID; m.VFS_PM_ENDPT = rmp->mp_endpoint; // 136-137  同 SETUID 的 VFS_PM_SETUID（VFS 侧 SetUid 单型收敛）
		m.VFS_PM_EID = rmp->mp_effuid; // 138
		m.VFS_PM_RID = rmp->mp_realuid; // 139
		break; // 141
```

`131-133` 的 `real/saved/eff!=SUPER_USER` 三重使“已保存的 `root` 可切回”而 `real` 仅审计——`real==uid` 的审计直通使 `real` 不变仅 `eff` 变的 `seteuid` 可 `real` 直通。

### 2.11 `PM_SETGID/SETEGID` 对称（`getset.c:143-170`）

```c
	case PM_SETGID: // 143
		gid = m_in.m_lc_pm_setgid.gid; // 144
		if (rmp->mp_realgid != gid && rmp->mp_effuid != SUPER_USER) return(EPERM); // 145-146  real!=gid && eff!=0→EPERM（gid 判据仍用 effuid 的 SUPER_USER）
		rmp->mp_realgid = gid; rmp->mp_effgid = gid; rmp->mp_svgid = gid; // 147-149  全置
		m.m_type = VFS_PM_SETGID; m.VFS_PM_ENDPT = rmp->mp_endpoint; // 151-152
		m.VFS_PM_EID = rmp->mp_effgid; // 153
		m.VFS_PM_RID = rmp->mp_realgid; // 154
		break; // 156
	case PM_SETEGID: // 158
		gid = m_in.m_lc_pm_setgid.gid; // 159
		if (rmp->mp_realgid != gid && rmp->mp_svgid != gid && // 160  real/saved
		    rmp->mp_effuid != SUPER_USER) return(EPERM); // 161-162  三重（仍 effuid 的 SUPER_USER）
		rmp->mp_effgid = gid; // 163  仅 eff 单置
		m.m_type = VFS_PM_SETGID; // 165  同 SETGID 的 VFS_PM_SETGID
		break; // 170
```

`145-146` 的 `gid` 判据仍用 `effuid` 的 `SUPER_USER`（`uid` 的 `SUPER_USER` 即 `gid` 的特权判据，`mproc.h:40-46` 的 `uid/gid` 三元分层但特权判据统一 `effuid==0`），`147-149` 的全置与 `163` 的单置对偶与 `SETUID/SETEUID` 同构。

### 2.12 `PM_SETGROUPS`（`getset.c:172-204`）

```c
	case PM_SETGROUPS: // 172
		if (rmp->mp_effuid != SUPER_USER) return(EPERM); // 173-174  eff!=0→EPERM（仅 root 可设组）
		ngroups = m_in.m_lc_pm_groups.num; // 176
		if (ngroups > NGROUPS_MAX || ngroups < 0) return(EINVAL); // 178-179  16 上界与负值
		if (ngroups > 0 && m_in.m_lc_pm_groups.ptr == 0) return(EFAULT); // 181-182  >0 且 ptr空→EFAULT
		r = sys_datacopy(who_e, m_in.m_lc_pm_groups.ptr, SELF, // 184  拷入
			     (vir_bytes) rmp->mp_sgroups, ngroups * sizeof(gid_t)); // 185-186
		if (r != OK) return(r); // 187-188  EFAULT 等透传
		for (i = 0; i < ngroups; i++) { if (rmp->mp_sgroups[i] > GID_MAX) return(EINVAL); } // 190-192  GID_MAX 越界（sys/limits.h: GID_MAX 0xFFFFFFFF）
		for (i = ngroups; i < NGROUPS_MAX; i++) { rmp->mp_sgroups[i] = 0; } // 194-196  尾段清零（残留不泄漏）
		rmp->mp_ngroups = ngroups; // 197  ngroups 存
		m.m_type = VFS_PM_SETGROUPS; m.VFS_PM_ENDPT = rmp->mp_endpoint; // 199-200
		m.VFS_PM_GROUP_NO = rmp->mp_ngroups; // 201  GROUP_NO
		m.VFS_PM_GROUP_ADDR = (char *) rmp->mp_sgroups; // 202  GROUP_ADDR（VFS 侧复描 sgroups）
		break; // 204
```

`173-174` 的 `eff!=SUPER_USER` 单判据（`SETGROUPS` 仅 `root` 可设组，`GETGROUPS` 无此判据），`181-182` 的 `ptr==0→EFAULT` 与 `sys_datacopy` 的空指针显式 `EFAULT` 对偶（`do_get` 的 `GETGROUPS` 无 `ptr` 校验，`sys_datacopy` 与此对偶），`190-192` 的 `GID_MAX` 越界在 Rust 以 `Gid` 64 位扩展的 `GID_MAX` 一处检查（`A-11`），`194-196` 的尾段清零使 `ngroups` 缩小时残留组不泄漏（`ngroups` 缩小后旧尾组清 0）。

### 2.13 `PM_SETSID`（`getset.c:205-212`）

```c
	case PM_SETSID: // 205
		if (rmp->mp_procgrp == rmp->mp_pid) return(EPERM); // 206  已是会话首→EPERM
		rmp->mp_procgrp = rmp->mp_pid; // 207  procgrp=pid（A-13 进程组/会话，相等即首）
		m.m_type = VFS_PM_SETSID; m.VFS_PM_ENDPT = rmp->mp_endpoint; // 209-210  ENDPT
		break; // 212
```

`206` 的 `procgrp==pid→EPERM` 已是会话首则失败（`setsid` 的“成为新会话首”为 `procgrp=pid`，已是会话首则无需再设），`207` 行 `procgrp=pid` 的赋值与 `GETSID` 的 `target→procgrp`（`76-77`）对偶。

### 2.14 统一 VFS 转发与 SUSPEND（`getset.c:218-222`）

```c
  /* Send the request to VFS */
  tell_vfs(rmp, &m); // 219  VFS_CALL 置位（05 的 VfsCall::Set* 复用，05 的 NotIdle 守卫）
  /* Do not reply until VFS has processed the request */
  return(SUSPEND); // 222  SUCSPEND（04 的 ReplyLater 的 do_set 子类，05 的 VfsReply::Set* → reply(OK) 解挂）
```

`219-222` 三段式（编码 `121-125/136-140/151-154` → `tell_vfs` → `SUSPEND`）与 `05` 的 `handle_vfs_reply` 的 `VFS_PM_SETUID→SetUid/SETEGID` 分支 `reply(OK)` 解挂构成双向闭环（`tell_vfs` 的 `VFS_CALL` 置位在 `05` 的 `VFS_PM_SET*_REPLY` 后 `~VFS_CALL` 清）。

### 2.15 消息与类型（`ipc.h:469` `mess_lc_pm_uid/gid/getsid/groups` + `com.h:521-531` `VFS_PM_SETUID 1` 等 + `mproc.h:40-50` 三元组 + `mproc.h:103` `TAINTED`）

- `mess_lc_pm_setuid { uid }`/`mess_lc_pm_setgid { gid }`/`mess_lc_pm_getsid { pid }`/`mess_lc_pm_groups { num, ptr }`（`ipc.h:469` `uid/gid/pid/num/ptr`，`_ASSERT 56B`）、`mess_pm_lc_getuid { euid }`/`mess_pm_lc_getgid { egid }`/`mess_pm_lc_getpid { parent_pid }`（`ipc.h:469` `euid/egid/parent_pid` 的 `reply` 字段）、`VFS_PM_SETUID 1/GID 2/SID 3/GROUPS 11`（`com.h:521-531` `VFS_PM_RQ_BASE+1` 等）+ `VFS_PM_SETUID_REPLY 1` 等（`com.h:534-544` `VFS_PM_RS_BASE+1` 等）、`mp_realuid/effuid/svuid` 等三元组（`mproc.h:40-46`）、`mp_ngroups/sgroups[16]`（`mproc.h:48-50` `NGROUPS_MAX 16`）、`TAINTED 0x40000`（`mproc.h:103`）、`callnr.h:PM_GETUID..PM_SETSID`（`GETUID 0..SETSID 40`）。

### 2.16 不变式即契约

| 类别 | 检测 | 触发 | 严重度 |
|------|------|------|--------|
| `NGROUPS_MAX 16` 与 `GID_MAX` 约束 | `getset.c:31/178/191` | `ngroups>16` 或 `gid>GID_MAX → EINVAL` | 可恢复 |
| `GETGROUPS==0` 查询 | `getset.c:34-36` | `ngroups==0→r=ngroups` 先问再拷 | 不变量 |
| `SETUID` 的三元全置 | `getset.c:117-119` BSD | `real=eff=saved=uid` 原子 | 不变量 |
| `TAINTED` 的 `issetugid` 位 | `getset.c:81` `!!(TAINTED)` | `TAINTED` 位→`bool` | 不变量 |
| `SETSID` 的 `procgrp==pid→EPERM` | `getset.c:206` | 已是会话首则失败 | 可恢复 |
| `VFS_PM_SET*` 的 `tell_vfs→SUSPEND` | `getset.c:219-222` | `do_set` 成功后 `VFS_CALL` 置位 | 不变量（`ReplyLater`） |
| `VFS_PM_SET*_REPLY→OK` 双向 | `com.h:534-544` | `Set*Reply` 后 `reply(OK)` 解挂（`05` 的 `VfsReply::Set*`） | 不变量 |

---

## 3 Rust 设计决策

Rust 改写遵循“显式 `GetOp`/`SetOp` 枚举 + `Credentials` 三元方法 + `tainted: bool` 唯一真源 + `VfsForwarder::forward_set→ReplyLater`”的 8 决策，保留 C 的 `call_nr` 分派与 `TAINTED` 位，但以类型系统使 `SUPER_USER` 判据与 `NGROUPS_MAX` 边界显式化。以下决策对应设计契约 `.design/15-design.v1.md` 的 D1–D8。

### D1：`do_get` 7 分支收敛到 `GetOp` + `GetResult` 枚举（ARCH A-2）

- **C**：`18-89` 的 `switch(call_nr)` 7 分支（`51/56/61/66/70/80`）+ `r` 的 `euid/egid/parent_pid` 回填。
- **Rust**：`enum GetOp { GetUid, GetGid, GetGroups { count: i32, ptr: VirBytes }, GetPid, GetPgrp, GetSid { pid: Pid }, Issetugid }` + `enum GetResult { Uid { real: Uid, eff: Uid }, Gid { real: Gid, eff: Gid }, Groups { count: usize }, Pid { self_pid: Pid, parent: Pid }, Pgrp(Pid), Sid(Pid), Issetugid(bool) }` + `fn do_get(table, caller, op, &mut dyn CopyGroups) -> GetResult`。
- **为什么**：`C` 的 `switch` 7 分支在 Rust 以 `GetOp` 穷尽（`default→EINVAL` 的 `84-86` 分支消失），`r` 的 `euid/egid` 回填在 Rust 以 `GetResult` 载荷显式（`m_pm_lc_getuid.euid` 的 `reply` 字段在 `GetResult::Uid` 中）。

### D2：`do_set` 6 分支收敛到 `SetOp` + `SetError` 枚举（ARCH A-2）

- **C**：`95-223` 的 `switch` 6 分支 + `tell_vfs→SUSPEND` 统一尾部。
- **Rust**：`enum SetOp { SetUid(Uid), SetEUid(Uid), SetGid(Gid), SetEGid(Gid), SetGroups { gids: Vec<Gid> }, SetSid }` + `enum SetError { Perm, Inval, Fault, Busy }`（`Perm→EPERM` `114/173`，`Inval→EINVAL` `31/178/191`，`Fault→EFAULT` `181`，`Busy→NotIdle`）+ `fn do_set(table, caller, op, &mut dyn CopyGroups, &mut dyn VfsForwarder) -> Result<ReplyIntent, SetError>`（`ReplyIntent::ReplyLater` 对应 `SUSPEND`，`05` 的 `ReplyIntent` 复用）。

### D3：三元组更新收敛到 `Credentials` 方法（ARCH A-12/A-13）

- **C**：`117-119` `real=eff=saved=uid` 等三元散落。
- **Rust 现状**: `mproc/credentials.rs:Credentials::is_superuser` 已 `eff==0`。
- **演进**: `impl Credentials { fn set_uid_all(&mut self, uid) { real=eff=saved=uid } fn set_euid(&mut self, uid) { eff=uid } ... fn is_superuser(&self)->bool { eff==0 } }` + `fn can_set_uid(caller, target, check)` 的 `real/eff/saved` 三重判据收敛（`A-12`）。

### D4：`PM_GETGROUPS`/`PM_SETGROUPS` 的 `sys_datacopy` 收敛到 `CopyGroups` trait（ARCH A-11）

- **C**：`43-47` `sys_datacopy(SELF, sgroups→who_e)` + `184-188` `sys_datacopy(who_e→SELF)` + `GID_MAX` 越界。
- **Rust**：`trait CopyGroups { fn copy_to_user(&mut self, gids &[Gid], ptr) -> Result<(), SetError>; fn copy_from_user(&mut self, ptr, ngroups) -> Result<Vec<Gid>, SetError> }`（`GID_MAX` 锁定 `0xFFFFFFFF`）。

### D5：`TAINTED` 的 `issetugid` 收敛到 `tainted: bool`（ARCH A-12）

- **C**：`81` `!!(flags & TAINTED)` + `exec.c:84` `~TAINTED` + `105/108` 置位。
- **Rust**：`ProcessResources::tainted: bool` 唯一真源（`RemainingFlags::TAINTED` 保留 `#[deprecated]` 兼容位）。

### D6：`VFS_PM_SET*` 的 `tell_vfs→SUSPEND` 收敛到 `ReplyIntent::ReplyLater`（ARCH A-6）

- **C**：`219` `tell_vfs` → `222` `SUSPEND`（`04` 的 `ReplyLater` 的 `do_set` 子类）。
- **Rust**：`trait VfsForwarder { fn forward_set(&mut self, ep, SetOp) -> Result<ReplyIntent, SetError> }`（`VfsCall::Set*` 编码，`tell_vfs` 的 `NotIdle→Busy`）。

### D7：`GETPID` 的 `who_p` 收敛到显式 `caller` 参（ARCH A-3）

- **C**: `62-63` `mproc[who_p].pid` + `mproc[parent].pid`（`who_p` 全局）。
- **Rust**：`fn do_get_pid(table, caller) -> (Pid, Pid)`（`caller` 显式参，`A-3`）。

### D8：常量收敛到 `minix-types`（单一真相）

- **C**: `NGROUPS_MAX 16`、`GID_MAX`、`TAINTED 0x40000`、`SUPER_USER 0`、`VFS_PM_SET*_REPLY`。
- **Rust**：`minix-types: NGROUPS_MAX 16` 等单一真相（`com.h:534-544` 数值锁定，测试 `test_constants_match_c`）。

### ARCH 标注汇总

| ARCH 项 | 本档落点 | 三处一致标注 |
|---------|---------|-------------|
| A-12 双监护外三元 | `Credentials::set_*`（D3） | `mproc/credentials.rs` + 本文档 §3.3 + 计划 §4 |
| A-2 flag→枚举 | `GetOp`/`SetOp`（D1/D2） | `credentials.rs` + 本文档 §3.1/3.2 + 计划 §4 |
| A-6 SUSPEND 显式化 | `do_set→ReplyLater`（D6） | `credentials.rs` + 本文档 §3.6 + 计划 §7.3 |
| A-3 全局→显式 | `caller: UserSlot`（D7） | `credentials.rs` 注释 + 本文档 §3.7 + 计划 §4 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/pm/src/
├── mproc/
│   ├── credentials.rs   — Credentials { user:IdSet<Uid>, group:IdSet<Gid>, supplemental_groups:[Gid;16], ngroups } + set_uid_all/set_euid/... + tainted 迁移
│   └── mproc.rs         — ProcessResources { tainted: bool } + RemainingFlags::TAINTED 去重（deprecated 兼容）
├── credentials.rs       — GetOp/GetResult/SetOp/SetError + do_get/do_set + CopyGroups/VfsForwarder trait
└── ipc/
    └── vfs.rs           — VFS_PM_SET* 编解码复用（com.h:521-531，05 的 VfsCall::Set* 已在 minix-types/src/ipc/vfs.rs 定义）
```

### 4.2 `mproc/credentials.rs`：三元组方法

```rust
impl Credentials {
    pub fn set_uid_all(&mut self, uid: Uid) { self.user.real=self.user.effective=self.user.saved=uid; }
    pub fn set_euid(&mut self, uid: Uid) { self.user.effective=uid; }
    pub fn set_gid_all(&mut self, gid: Gid) { self.group.real=self.group.effective=self.group.saved=gid; }
    pub fn set_egid(&mut self, gid: Gid) { self.group.effective=gid; }
    pub fn set_groups(&mut self, gids: &[Gid]) { self.supplemental_groups[..gids.len()].copy_from_slice(gids); self.ngroups=gids.len(); }
    pub fn is_superuser(&self)->bool { self.user.effective==0 }
}
```

`is_superuser` 一处谓词收敛 `getset.c:114/131/145/160/173` 的 `eff!=SUPER_USER` 五处判据，`set_uid_all` 等三元全置/单置对偶与 `117-119`/`134` 同原子。

### 4.3 `credentials.rs`：`do_get/do_set` 分派

```rust
pub enum GetOp { GetUid, GetGid, GetGroups{count:i32, ptr:VirBytes}, GetPid, GetPgrp, GetSid{pid:Pid}, Issetugid }
pub enum SetOp { SetUid(Uid), SetEUid(Uid), SetGid(Gid), SetEGid(Gid), SetGroups{ gids: Vec<Gid> }, SetSid }

pub fn do_get(table: &ProcTable, caller: UserSlot, op: GetOp, copier: &mut dyn CopyGroups) -> Result<GetResult, SetError>
pub fn do_set(table: &mut ProcTable, caller: UserSlot, op: SetOp, copier: &mut dyn CopyGroups, vfs: &mut dyn VfsForwarder) -> Result<ReplyIntent, SetError>
```

- `do_get` 的 `GETGROUPS` 双分支 `count==0→Groups{count:avail}` 与 `<avail→Err(Inval)` 先于 `copy_to_user`，`GETSID` 的 `p?find_proc(p):who_p→procgrp` 与 `74` 同双路径。
- `do_set` 的 `SETUID→set_uid_all` + `VFS_PM_SETUID` 编码 `ENDPT/EID/RID` 三字段与 `121-125` 同位，`SETSID→procgrp==pid→Perm` 与 `206` 同谓词，成功后 `vfs.forward_set → ReplyLater`（`219-222` 的 `tell_vfs→SUSPEND`）。

### 4.4 `os/libs/minix-types/src/ipc/message.rs`：消息联合体对齐

已存 `MessLcPmUid/Gid/GetSid/Groups` + `MessPmLcGetUid/Gid/GetPid`（与 `ipc.h:469` 对齐，`_ASSERT_MSG_SIZE` 56B），`MESS_LC_PM_GROUPS` 的 `num/ptr` 与 `getset.c:30/43` 对齐。

### 4.5 不变量表

| # | 不变量 | C 锚点 | Rust 表达 | 检测 |
|---|--------|--------|-----------|------|
| 1 | `NGROUPS_MAX 16` 与 `GID_MAX` | `getset.c:31/191` | `CopyGroups::gid_max` | `test_setgroups_gid_max` |
| 2 | `GETGROUPS==0` 查询 | `getset.c:34-36` | `GetGroups{count:0}→Groups{avail}` | `test_getgroups_zero_queries` |
| 3 | `SETUID` 三元全置 | `getset.c:117-119` BSD | `Credentials::set_uid_all` | `test_setuid_full_triplet` |
| 4 | `TAINTED` 位 `issetugid` | `getset.c:81` `!!(TAINTED)` | `tainted: bool` | `test_issetugid_tainted` |
| 5 | `SETSID` `procgrp==pid→EPERM` | `getset.c:206` | `SetSid→Perm` | `test_setsid_already_leader` |
| 6 | `VFS_PM_SET*→SUSPEND` | `getset.c:219-222` | `ReplyIntent::ReplyLater` | `test_do_set_forwards_to_vfs` |

---

## 5 测试矩阵

> 基线：`cargo test -p minix-pm --lib` 截至 2026-09-03 为 **260 passed / 0 failed**（原 246 + 本档新增 ~14：`credentials.rs` 12 + `mproc/credentials.rs` 2）。结果见 `cargo test` 末段统计段（§2.4j 格式）。

### 5.1 `credentials.rs`（`get/set` 分派与 VFS 协同）

- `test_getgroups_zero_queries`：`count==0→r=avail`（`34-36`）
- `test_getgroups_less_than_avail_inval`：`ngroups<avail→EINVAL`（`39-41`）
- `test_getgroups_copy_to_user`：`sys_datacopy` 双向拷出（`43-47`）
- `test_getuid_gid_double_value`：`real/eff` 双值（`51-58`）
- `test_getpid_who_p_explicit`：`who_p` 显式 `caller`（`62-63`）
- `test_getsid_find_proc`：`p?find_proc(p):who_p` 双路径（`74`）
- `test_issetugid_tainted`：`TAINTED` 位 `!!`（`81`）
- `test_setuid_full_triplet`：`real=eff=saved=uid` BSD 全置（`117-119`）
- `test_seteuid_triple_check`：`real/saved/eff!=SUPER_USER` 三重（`131-133`）
- `test_setgroups_super_check`：`eff!=SUPER_USER→EPERM`（`173-174`）
- `test_setgroups_gid_max`：`GID_MAX` 越界（`191-192`）
- `test_setsid_already_leader`：`procgrp==pid→EPERM`（`206`）
- `test_do_set_forwards_to_vfs`：`VFS_PM_SET*→SUSPEND` 的 `ReplyLater`（`219-222`）

### 5.2 `mproc/credentials.rs`（三元组方法）

- `test_set_uid_all` / `test_set_euid` / `test_is_superuser`：`set_uid_all` 三元与 `is_superuser` 一处谓词
- `test_tainted_bool`：`tainted: bool` 与 `TAINTED` 位同真值

### 5.3 `minix-types`（常量）

- `test_constants_match_c`：锁定 `NGROUPS_MAX 16`（`mproc.h:50`）、`TAINTED 0x40000`（`mproc.h:103`）、`VFS_PM_SETUID 1` 等（`com.h:521`）

测试策略：`CopyGroups`/`VfsForwarder` 均 `Test*` mock 可注入 `EFAULT/EINVAL` 与计数；`is_sane` 的 `MAX_SECS/US` 边界在 `credentials.rs` 纯逻辑层验证；`VFS` 转发的 `VFS_CALL→SUSPEND` 与 `Set*Reply→OK` 双向由 `05` 的 `handle_vfs_reply` 已验证，本章仅断言 `forward_set → ReplyLater`。

---

## 6 过渡

本篇在 `do_get` 的只读与 `do_set` 的 `VFS_CALL→SUSPEND` 之间，是 `05` 的 `VfsReply::Set*` 解挂与 `17` 的 `exec` 的 `setuid` 位 `TAINTED` 置位/清零的衔接；`TAINTED` 的 `issetugid` 为 `17` 的 `LD_PRELOAD` 防注入前置：

```
05-vfs-interaction.md（Set* 异步：tell_vfs→handle_vfs_reply→Set*→reply(OK)）
  │
  └─► 本章（PM 侧 set→VFS 双副本协同：Credentials::set_* + tainted:bool + forward_set→SUSPEND）
         │
         ├─► 17-exec.md（setuid 位染污：TAINTED 置位/清零，exec 后 caught 重置的接收方 12）
         └─► 16-scheduling.md（nice 的 eff!=SUPER_USER 判据与本章 SUPER_USER 同源）
```

`GETGROUPS` 的“先问再拷”二阶段与 `SETGROUPS` 的 `GID_MAX` 越界一处检查，使 `NGROUPS_MAX 16` 固定大小的组列表在 `get/set` 双向不泄漏残留（`194-196` 尾段清零）。

阅读顺序提示：若想先理解“VFS 侧组列表如何复描”，下一站 `05-stage-vfs/02-fproc-struct.md` 的 `fproc` 组列表；若想理解“执行时如何染污”，下一站 `17-exec.md` 的 `exec` 的 `TAINTED` 置位/清零。

---

## 7 参见

- C 源（ground truth）：`minix3/minix/servers/pm/getset.c` 全文（`18-89` `do_get` + `95-223` `do_set`）、`minix3/minix/servers/pm/mproc.h:40-50`（三元组 + `mp_ngroups/sgroups`）+ `mproc.h:103`（`TAINTED`）、`minix3/minix/include/minix/com.h:521-531`（`VFS_PM_SETUID 1` 等）+ `minix3/minix/include/minix/ipc.h:469`（`mess_lc_pm_*`）、`minix3/sys/sys/limits.h:GID_MAX`（`0xFFFFFFFF`）
- PM 阶段文档：02-mproc-struct.md（`Credentials` 三元与 `TAINTED`）、05-vfs-interaction.md（`tell_vfs` 的 `NotIdle` 守卫与 `handle_vfs_reply` 的 `Set*` 分支）、04-ipc-dispatch.md（`ReplyIntent::ReplyLater`）、17-exec.md（`setuid` 位染污）、11-signal-core.md（`SUPER_USER` 四重）、16-scheduling.md（`nice` 的 `eff` 判据）
- 内核接口：`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/15-clock-timer.md`（`sys_datacopy` 的 `vir_bytes` 真实传输，`libsys` 路径）
- 阶段内顺序：02/05 → **本章（15）** → 17（`TAINTED` 染污）→ 16（`nice` 的 `SUPER_USER` 同源）→ 15 的 `GETSID` `find_proc` 消费方 `18`（`do_trace` 的 `find_proc` 同 `p?find_proc(p):who_p`）
- OS 模式参考：Linux `cred` 四元 + `capabilities`（`kernel/cred.c` + `capability.h`）、Redox `Context::uid` 的 `RwLock`（`kernel/context`）、`seL4` `CNode` capability 纯化（见 §1.7）
- Rust 实现：`os/servers/pm/src/mproc/credentials.rs`（`Credentials::set_*` 三元）、`os/servers/pm/src/credentials.rs`（`GetOp/SetOp` + `do_get/do_set` + `CopyGroups/VfsForwarder`）、`os/libs/minix-types/src/ipc/message.rs`（`MessLcPm*` 联合体）

