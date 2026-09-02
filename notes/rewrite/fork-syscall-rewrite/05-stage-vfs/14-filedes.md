# 14 — 文件描述符表：`fp_filp[OPEN_MAX]` 的 `get_fd` 最低空闲与 `close_fd` 的 `FILP_CLOSED` 抑制

本文讲清 VFS 如何在 `OPEN_MAX 256` 的 `fp_filp[OPEN_MAX]` 每进程私有索引、`FD_CLOEXEC` 的 `Bitmap` 位集、`NR_FILPS 1024` 的全局 `filp[1024]` 共享池、`FILP_CLOSED` 的 `EIO` 抑制哨兵、`EMFILE` 的 `fd` 耗尽与 `ENFILE` 的 `filp` 耗尽、`O_CLOEXEC` 的 `cloexec` 置位、`COPYFD_FLAGS` 的 `FROM/TO/CLOSE` 三操作、`invalidate_filp` 的 `FILP_CLOSED` 只关语义的约束下，以 `get_fd` 的 `start→OPEN_MAX` 最低空闲扫描与 `filp_count==0 && trylock` 空闲 `filp` 分配及 `check_fds` 的 `nfds` 窗口校验与 `close_fd` 的 `filp==NULL→EBADF / mode==FILP_CLOSED→EIO / NULL fd→Close / cloexec 清除→close_filp→select/lock 释放` 建立 `Fd → Filp → Vnode` 的二跳共享，并以 `do_copyfd` 的 `super_user→ACL` 守门与 `isokendpt→slot` 解引及 `filp_count++` 共享为驱动 `COPYFD` 提供可观测复制。

前置阅读：`02-fproc-struct.md`（`FProc{filps:[Option<FilpId>;256], cloexec_set:Bitmap, tty}` 的 `fp_filp` 私有索引）、`04-filp-table.md`（`FilpTable` 的 `count==0` 哨兵与 `alloc_filp` 双扫描及 `FilpFlags`）、`07-tll-lock.md`（`TLL_NONE/READ/WRITE` 与 `FilpLock`）、`05-vnode-table.md`（`Vnode` 的 `put_vnode` 与 `v_ref_count`）。

> 本章不讲什么：
> - `filp` 结构字段语义与 `filp_count` 引用计数的递增递减细节—— `04-filp-table.md`（`Filp{mode,pos,count,flags,softlock}` 的 `count==0` 空闲与 `alloc` 双扫描）
> - `vnode` 的 `dup_vnode/put_vnode` 的双层 `v_ref_count/v_fs_count` 与 `clean_refs` 的 `256` 阈值—— `05-vnode-table.md`（`close_fd` 的 `put_vnode` 对端）
> - `close_filp` 的 `S_ISCHR/S_ISBLK/S_ISSOCK` 分流与 `bdev/cdev/sdev_close` 的 `grant` —— `20/21/22`（`close_filp` 的块/字符/套接字分流，本章只讲 `close_fd` 的 `NULL/CLOSED` 守门）
> - `PM` 的 `fork` 时 `fp_filp` 整表拷贝与 `filp_count++` 共享—— `10-pm-protocol.md`（`copy_fproc` 的 `filp_shared` 计数对端）
> - `select` 的 `filp_selectors` 与 `lock` 表的 `nr_locks` 释放—— `23-select.md`/`30-fcntl-lock.md`（`close_fd` 的 `nr_locks>0 → lock_revive` 对端）
> - 内核 `sys_datacopy` 的跨进程 `COPYFD` 拷贝—— `99-global-concepts.md`

---

## 1 概念

### 1.0 章节引言

**目标读者**：已理解 04 的 `FilpTable` 的 `count==0` 空闲哨兵与 `02` 的 `FProc.filps` 私有索引，能 `grep "get_fd\|close_fd\|do_copyfd" minix3/minix/servers/vfs/filedes.c` 的开发者。

### 1.1 为什么 `fd` 是私有索引而 `filp` 是共享池

`fd`（`0` 的 `stdin`、`1` 的 `stdout`、`2` 的 `stderr` 的低 3 位惯例）是**进程私有**的 `fp_filp[256]` 索引，`filp`（`filp[1024]` 全局）是**跨进程共享**的打开描述（`filp_count` 引用计数、`filp_pos` 共享偏移、`filp_mode` 共享模式）。`fork` 的 `copy_fproc` 使父子 `fp_filp[3]=Some(7)` 共享同一 `filp[7]` 的 `count=2`，`close(3)` 的 `filp_count--` 使 `count 2→1` 不释放 `vnode`，`close` 的第二次 `count 1→0` 才 `put_vnode`。此“私有索引共享池”与 Redox 的 `FdTable { fd→Arc<OpenFileDescription> }` 的 `Arc` 共享及 Linux 的 `files_struct { fd_array[64]→file }` 的 `atomic_t f_count` 共享同型：VFS 的 `fp_filp[i]==NULL` 空闲在 `Redox` 以 `Slab::vacant` 可观测。

### 1.2 `get_fd` 的最低空闲：`start→OPEN_MAX` 线性扫描

`get_fd:121 for(i=start; i<OPEN_MAX; i++) if(fp_filp[i]==NULL) { k=i; break; }` 的 `start` 参数使 `F_DUPFD` 的 `fcntl(fd, F_DUPFD, arg)` 的 `arg` 下界可观测：`get_fd(fp, arg, ..., &new_fd)` 的 `start=arg` 使 `new_fd ≥ arg` 的最低空闲在 `14` 的 `do_copyfd` 的 `COPYFD_FROM` 分支的 `get_fd` 调用可观测。`OPEN_MAX 256` 的 `EMFILE` 与 `NR_FILPS 1024` 的 `ENFILE` 的双耗尽管 `get_fd:130 if(i>=OPEN_MAX) EMFILE` 与 `154 ENFILE` 的 `fd` 耗尽 vs `filp` 耗尽在 `open.c:common_open` 的 `get_fd(fp,0,bits,&fd,&filp)→EMFILE/ENFILE` 可观测。

### 1.3 `check_fds` 的 `nfds` 窗口校验

`check_fds:88 for(i=0; i<OPEN_MAX; i++) if(fp_filp[i]==NULL && --nfds==0) return OK` 的 `nfds` 窗口校验使 `select` 的 `FD_SETSIZE` 窗口在 `14` 的 `select` 调用前可观测：`check_fds(fp, nfds)` 的 `nfds` 个空闲 `fd` 预检在 `23-select.md` 的 `do_select` 的 `check_fds` 调用可观测。

### 1.4 `close_fd` 的 `FILP_CLOSED` 抑制：只关语义

`open.c:690 close_fd:699 get_filp2(rfp, fd, OPCL)→NULL→EBADF` 的 `EBADF` 守门与 `filedes.c:250 invalidate_filp: rfilp->filp_mode=FILP_CLOSED` 的 `FILP_CLOSED` 只关哨兵使 `get_filp2:191 if(mode==FILP_CLOSED)→EIO` 的 `EIO` 抑制在 `14` 的 `invalidate_filp_by_char_major` 后可观测：驱动 `dmap_unmap_by_endpt` 后的 `invalidate_filp_by_char_major(major)` 使 `read` 的 `FILP_CLOSED→EIO` 而 `close` 的 `FILP_CLOSED` 可通过 `get_filp2:191` 的 `locktype==OPCL` 分支（`close_fd` 的 `get_filp2(OPCL)` 允许 `CLOSED`）。

### 1.5 `do_copyfd` 的 `FROM/TO/CLOSE` 三操作与 `super_user` 守门

`do_copyfd:524` 的 `endpt=m7_i1, fd=m7_i2, what=m7_i3` 的 `COPYFD_FROM(0)/TO(1)/CLOSE(2)` 三操作在 `device.c` 的 `UDS` 套接字 `COPYFD_FROM` 的 `fd` 窃取与 `VND` 块设备的 `COPYFD_TO` 的 `fd` 注入可观测：`COPYFD_FROM` 的 `get_filp2(COPYFD_TO?fp:rfp)` 的 `fp vs rfp` 互证与 `COPYFD_TO` 的 `for(fd=0; fd<OPEN_MAX; fd++) if(fp_filp[fd]==NULL) break` 的最低空闲及 `COPYFD_CLOSE` 的 `filp_count>1→count--` 回滚在 `14` 的 `do_copyfd` 三分支可观测。`539 if(!super_user) return EPERM` 的 `SU_UID 0` 守门使 `UDS` 的 `COPYFD` 仅 `root` 的 `MFS` 可观测（`04-stage-pm/12` 的 `super_user` 对端）。

### 1.6 `invalidate_filp` 族的端点失效：`FILP_CLOSED` 的传播

`invalidate_filp_by_endpt:298 for(f: count!=0 && vno!=NULL && v_fs_e==proc_e → mode=FILP_CLOSED)` 的 `fs_e==proc_e` 守门在 `vmnt_unmap_by_endpt:180` 的 `FS` 崩溃回收中使 `open` 的 `EIO` 抑制可观测，`invalidate_filp_by_char_major:260 major(sdev)==major → mode=CLOSED` 的 `S_ISCHR` 守门在 `dmap_unmap_by_endpt:180` 的 `cdev` 驱动退出中可观测，`invalidate_filp_by_sock_drv:277 S_ISSOCK && smap_endpt==num → CLOSED` 的 `S_ISSOCK` 守门在 `smap_unmap_by_endpt:148` 的 `sdev` 退出中可观测。

### 1.7 小结

`fp_filp[256]` 的 `NULL` 空闲在 `get_fd` 的 `start→OPEN_MAX` 最低空闲可观测，`filp[1024]` 的 `count==0` 空闲在 `get_fd` 的 `filp_count==0 && trylock` 可观测，`FD_CLOEXEC` 的 `Bitmap` 位集在 `cloexec_set` 的 `FD_SET/FD_CLR` 可观测，`close_fd` 的 `EBADF/EIO` 双守门使 `FILP_CLOSED` 的只关语义在 `invalidate` 后可观测，`do_copyfd` 的 `FROM/TO/CLOSE` 三操作在 `super_user` 守门后可观测。下一节以 `filedes.c:88-656` 全文与 `open.c:690` 的 `close_fd` 为主线逐段核对。

---

## 2 C 源码分析

### 2.1 `check_fds:88-105` 的 `nfds` 窗口

`check_fds:95 assert(nfds>=1)` 的 `nfds≥1` 守门与 `97 for(i=0; i<OPEN_MAX; i++) if(fp_filp[i]==NULL && --nfds==0) return OK` 的 `NULL` 递减 `nfds` 窗口及 `104 return EMFILE` 的 `EMFILE` 耗尽在 `23-select.md` 的 `do_select` 的 `check_fds(fp, nfds)` 可观测。

### 2.2 `get_fd:110-156` 的最低空闲与空闲 `filp` 双扫描

`get_fd:121 for(i=start; i<OPEN_MAX; i++) if(fp_filp[i]==NULL){ k=i; break; }` 的 `start` 下界及 `130 if(i>=OPEN_MAX) EMFILE` 的 `fd` 耗尽及 `133 if(fpt==NULL) return OK` 的 `filp` 不关心 `NULL` 短路及 `136 for(f=filp; f<filp+NR_FILPS; f++) if(count==0 && trylock==0){ filp_mode=bits; pos=0; selectors=0; ...; *fpt=f; return OK }` 的 `count==0 && trylock` 空闲 `filp` 双守门及 `154 ENFILE` 的 `filp` 耗尽在 `open.c:common_open` 的 `get_fd(fp,0,bits,&fd,&filp)→EMFILE/ENFILE` 可观测。

### 2.3 `get_filp/get_filp2:162-199` 的 `EBADF/EIO` 双守门

`get_filp2:188 if(fild<0||>=OPEN_MAX) EBADF` 的范围守门与 `190 if(VNODE_OPCL!=locktype && filp_mode==FILP_CLOSED) EIO` 的 `FILP_CLOSED→EIO` 抑制（`OPCL` 允许 `CLOSED` 的 `close_fd` 例外）及 `193 if((filp=fp_filp[fild])==NULL) EBADF` 的 `NULL→EBADF` 及 `195 if(locktype!=NONE) lock_filp(filp)` 的 `VNODE_READ/WRITE/OPCL` 锁请求在 `04` 的 `Filp::lock` 可观测。

### 2.4 `find_filp:205-224` 的 `filp→vnode` 反查与 `find_filp_by_sock_dev:229-245` 的 `S_ISSOCK` 守门

`find_filp:217 for(filp: count!=0 && vno==vp && mode&bits → return f)` 的 `vno==vp` 反查与 `229 find_filp_by_sock_dev:238 if(count!=0 && vno!=NULL && S_ISSOCK(mode) && sdev==dev && mode!=CLOSED → return f)` 的 `S_ISSOCK` 守门在 `17-pipe.md` 的 `find_filp(vp,R_BIT)` 可观测。

### 2.5 `invalidate_filp` 族 `250-308` 的 `FILP_CLOSED` 传播

`250 invalidate_filp: rfilp->mode=CLOSED` 的单写与 `260 invalidate_filp_by_char_major:266 if(major(sdev)==major && S_ISCHR(mode)) invalidate` 的 `S_ISCHR` 守门及 `277 invalidate_filp_by_sock_drv:287 S_ISSOCK && smap_endpt==num → CLOSED` 的 `smap` 守门及 `298 invalidate_filp_by_endpt:304 if(v_fs_e==proc_e) CLOSED` 的 `fs_e` 守门在 `06` 的 `vmnt_unmap` 与 `19` 的 `dmap/smap_unmap` 后可观测。

### 2.6 `lock_filp:313-352` 的 `softlock` 与 `tll` 升级

`lock_filp:325 if(locked_by_me){ softlock=fp } else { if(FIFO && READ) locktype=WRITE; lock_vnode(vp,locktype) }` 的 `softlock` 复用 `fp` 指针与 `FIFO→WRITE` 升级及 `342 if(trylock!=0){ org_self=worker_suspend(); mutex_lock; resume }` 的 `suspend→lock→resume` 三件套在 `08` 的 `SuspendToken` 可观测，`379 unlock_filps:385 assert(filp1!=filp2 && vno==vno)` 的双 `filp` 同 `vnode` 守门在 `open.c:common_open` 的 `pipe` `find_filp` 复用可观测。

### 2.7 `close_filp:413-519` 的 `S_ISCHR/S_ISBLK/S_ISSOCK` 分流与 `FILP_CLOSED` 置位

`close_filp:435 if(count-1==0 && mode!=CLOSED){ if(S_ISCHR||S_ISBLK||S_ISSOCK){ dev=sdev; if(BLK){ lock_bsf; if(bfs_e==ROOT_FS_E && dev!=ROOT_DEV) req_flush; unlock_bsf; bdev_close } else if(CHR) cdev_close else { if(NONBLOCK) may_suspend=FALSE; r=sdev_close(dev,may_suspend); if(r!=SUSPEND) r=OK } f_mode=CLOSED } }` 的 `count==1 && !CLOSED` 三特殊分流与 `491 if(FIFO){ rw=R_BIT?WRITE:READ; release(vp,rw,susp_count) }` 的 `FIFO` 释放及 `496 if(--count==0){ if(FIFO && v_ref_count==1) truncate; unlock; put_vnode; vno=NULL; mode=CLOSED; count=0 } else unlock` 的 `count 1→0→put_vnode` 慢路径。

### 2.8 `close_fd:690-727` 的 `NULL→EBADF→NULL fd→close_filp→cloexec 清除→lock 释放`

`open.c:690 close_fd:699 get_filp2(OPCL)→NULL→EBADF` 的 `OPCL` 允许 `CLOSED` 的 `get_filp2`守门与 `706 fp_filp[fd]=NULL` 的 `NULL` 回收及 `708 close_filp(rfilp,may_suspend)` 的 `close_filp` 委托及 `710 FD_CLR(cloexec)` 的 `CLOEXEC` 清除及 `713 if(nr_locks>0){ for(lock: vnode==vp && pid==fp_pid → type=0; nr_locks--) lock_revive }` 的 `FLOCK` 释放（`30` 的 `lock` 对端）。

### 2.9 `do_copyfd:524-656` 的 `COPYFD_FROM/TO/CLOSE` 与 `super_user` 守门

`do_copyfd:539 if(!super_user) EPERM` 的 `SU_UID 0` 守门与 `541 endpt=m7_i1; fd=m7_i2; what=m7_i3` 的 `COPYFD_*` 三操作及 `544 flags=what & COPYFD_FLAGS; what &= ~COPYFD_FLAGS` 的 `CLOEXEC` 标志剥离及 `548 isokendpt(endpt,&slot)→EINVAL` 的三守门及 `561 get_filp2(COPYFD_TO?fp:rfp, fd, NONE)` 的 `fp vs rfp` 互证及 `572 if(ioctl_fp==rfp) EBADF` 的 `ioctl` 死锁守门及 `576 lock_filp(READ)` 的 `vnode` 锁及 `579 COPYFD_FROM: if(S_ISSOCK && smap_endpt==who_e) EDEADLK; rfp=fp; flags&=~CLOEXEC → fallthrough TO` 的 `UDS` 套接字自复制 `EDEADLK` 守门及 `618 COPYFD_TO: for(fd=0; fd<OPEN_MAX; fd++) if(fp_filp[fd]==NULL) break; if(fd<OPEN_MAX){ fp_filp[fd]=rfilp; if(CLOEXEC) FD_SET; count++; r=fd } else EMFILE` 的最低空闲及 `636 COPYFD_CLOSE: if(count>1){ count--; fp_filp[fd]=NULL; OK } else EBADF` 的 `count>1` 回滚及 `650 EINVAL` 的未知 `what`。

---

## 3 Rust 设计决策

Rust 改写不是照抄 `filedes.c:121` 的 `for(i=start)` 与 `open.c:706` 的 `fp_filp[fd]=NULL`，而是吸收 Redox/Linux 的 `FdTable` 模型后做取舍。以下决策对应 `.design/14-design.v1.md` D1-D6。

### D1 `fd` 索引的 `Fd` 新型与 `OPEN_MAX` 上界显式

- **C**：`int fd` 裸整数与 `OPEN_MAX 256` 的 `i>=OPEN_MAX→EMFILE` 散落（`filedes.c:130`）。
- **Rust**：`Fd(u8)` 的 `u8` 上界 `256` 使 `Fd::new(300)→None` 的 `TryFrom` 守门在编译期可观测，`OPEN_MAX: usize = 256` 的 `const` 使 `Fd` 的 `0..256` 范围在 `FdTable::alloc` 的 `start..OPEN_MAX` 扫描可测试；`Fd::as_usize` 的 `as usize` 在 `fp_filp[fd.as_usize]` 的索引中可观测。
- **为什么**：`int` 的 `fd<0||>=OPEN_MAX` 在 Rust 以 `Fd` 的 `Option` 使 `EBADF` 在 `Fd::new` 可早拒绝。

### D2 `get_fd` 的 `LowestFree` 策略显式：`FdAllocPolicy` trait

- **C**：`get_fd:121 for(i=start; i<OPEN_MAX; i++) if(fp_filp[i]==NULL)` 的最低空闲策略硬编码。
- **Rust**：`trait FdAllocPolicy { fn allocate(&self, table: &[Option<FilpId>], start: usize) -> Option<usize>; }` 的 `LowestFree`（`start..OPEN_MAX` 线性扫描最低空闲）与 `NextFit { next: Cell<usize> }` 的 `next→OPEN_MAX` 环绕扫描双实现；`FProc::alloc_fd(&self, start, policy: &dyn FdAllocPolicy) -> Result<Fd, FdError>` 的 `policy` 注入使 `F_DUPFD` 的 `start=arg` 下界在 `LowestFree` 的 `arg..256` 可测试。
- **为什么**：`O_DUPFD` 的 `arg` 下界在 `Linux` 以 `__alloc_fd` 的 `start` 参数显式，Rust 以 `policy` 注入使策略可测试（`LowestFree( arg=5)→Some(5)` vs `NextFit(next=10)→Some(10)` 的行为差异）。

### D3 `check_fds` 的 `nfds` 窗口显式

- **C**：`check_fds:95 --nfds==0 → OK` 的 `nfds` 递减窗口。
- **Rust**：`FProc::check_fds(&self, nfds: usize) -> Result<(), FdError>` 的 `count_ones(nfds)` 使 `count >= nfds → Ok else EMFILE` 在 `FProc::available_fds() -> usize` 的 `OPEN_MAX - count_ones` 可观测。

### D4 `close_fd` 的 `EBADF/EIO` 双守门与 `FILP_CLOSED` 抑制

- **C**：`get_filp2(OPCL)→NULL→EBADF` 与 `mode==FILP_CLOSED→EIO` 的 `OPCL` 例外。
- **Rust**：`FProc::close_fd(&mut self, fd: Fd, filp_table: &mut FilpTable) -> Result<(), FdError>` 的 `get_mut(fd).ok_or(BadFd) → FdError::BadFd` 与 `filp.mode==Closed→Err(Io)` 的 `EIO` 抑制在 `FdError::Io(EIO)` 可测试；`FILP_CLOSED` 的 `Closed` 哨兵在 `FilpId` 的 `Option<FilpId>` 的 `Some(CLOSED)` 分化使 `close` 的 `OPCL` 允许 `CLOSED` 在 `close_fd` 的 `allow_closed=true` 分支可测试。
- **为什么**：`FILP_CLOSED` 的 `mode==CLOSED` 在 `invalidate_filp:254 mode=CLOSED` 的 `FILP_CLOSED` 只关语义使 `read` 的 `EIO` 与 `close` 的 `OK` 分化在类型层面不可误用。

### D5 `do_copyfd` 的 `FROM/TO/CLOSE` 与 `super_user` 守门显式

- **C**：`do_copyfd:539 super_user→EPERM` 的 `SU_UID` 守门与 `isokendpt→EINVAL` 的三守门及 `COPYFD_FROM: S_ISSOCK && smap_endpt==who_e → EDEADLK` 的自复制 `EDEADLK`。
- **Rust**：`CopyFdKind::From/To/Close` 的 `enum` + `fn copy_fd(&mut self, target: &mut FProc, fd: Fd, kind: CopyFdKind, cred: &Credentials) -> Result<Fd, FdError>` 的 `cred.is_super()→EPERM` 守门与 `target_slot.is_none()→BadEndpoint` 及 `S_ISSOCK→EDEADLK` 的 `SocketDev` 校验在 `CopyFdKind::From` 的 `is_uds_self` 分支可测试。
- **为什么**：`int what` 的 `COPYFD_FROM 0/TO 1/CLOSE 2` 裸整数在 Rust 以 `CopyFdKind` 枚举使 `what & COPYFD_FLAGS` 的 `CLOEXEC` 剥离在 `CopyFdKind::flags` 可观测。

### D6 `invalidate_filp` 族的 `FILP_CLOSED` 传播与 `CLOEXEC` 位集

- **C**：`invalidate_filp:254 mode=CLOSED` 的单写与 `FD_CLR` 的 `cloexec` 清除在 `close_fd:710` 的 `FD_CLR` 可观测。
- **Rust**：`FProc::invalidate_by_endpoint(ep) -> usize` 的 `for(filp: vno→fs_e==ep → mode=CLOSED)` 计数返回与 `Bitmap::clear(fd)` 的 `FD_CLR` 使 `invalidate` 后 `get(Fd)→Io(EIO)` 的抑制在 `FProc::get_fd(fd, closed) → Err(Io)` 可测试。

### ARCH 决策总表

| ARCH | 落点 | 三处一致标注 |
|------|------|-------------|
| A-8 64 位 | `Fd(u8)` 的 `u8` 上界 `256` | `filedes.rs:Fd` + 本文档 D1 + 14 正文 1.2 |
| A-4 全局聚合 | `FProc.filps: [Option<FilpId>;256]` 的私有索引 | `fproc.rs:FProc` + 本文档 D1 + 14 正文 1.1 |
| A-6 锁降级 | `FilpTable::try_lock` 的 `SoftLock` 替代 `fp_lock` | `filp.rs:FilpLock` + 本文档 D4 + 14 正文 1.4 |

---

## 4 实现详解

### 4.1 模块结构

```
os/servers/vfs/src/
├── filedes.rs          — Fd/FdError + FdAllocPolicy(LowestFree vs NextFit) + check_fds/get_fd/close_fd/invalidate/copy_fd  + Bitmap 的 FD_SET/FD_CLR
├── fproc.rs            — FProc{filps:[Option<FilpId>;256], cloexec_set:Bitmap, tty} 的 fp_filp 私有索引（02）
├── filp.rs             — FilpTable{ filps:[Filp;1024] } 的 count==0 空闲与 incr_ref/close_filp（04）
├── vmnt.rs             — Vmnt.m_dev/m_fs_e 双哨兵（06）
└── fproc.rs            — FProcTable::ok_endpoint 的三守卫（03）
```

### 4.2 `filedes.rs:1` 核心

| 符号 | 来源 | Rust 位置 | 行为 |
|------|------|-----------|------|
| `OPEN_MAX 256` | `syslimits.h:38` | `filedes.rs:OPEN_MAX:usize=256` | `Fd(0..255)` 上界 |
| `check_fds` | `filedes.c:88` | `FProc::check_fds(nfds) -> Result<(),FdError>` | `available >= nfds → OK else EMFILE` |
| `get_fd` | `filedes.c:110` | `FProc::alloc_fd(start, policy) -> Result<(Fd, FilpId), FdError>` | `LowestFree` 的 `start..256` 扫描 + `FilpTable::alloc` 的 `count==0` 双守门 |
| `close_fd` | `open.c:690` | `FProc::close_fd(fd, filp_table) -> Result<(),FdError>` | `BadFd→EBADF / CLOSED→EIO / NULL→Close / FD_CLR / close_filp` |
| `do_copyfd` | `filedes.c:524` | `FProc::copy_fd(target, fd, kind, cred) -> Result<Fd,FdError>` | `super_user→EPERM / isokendpt→BadEndpoint / S_ISSOCK→EDEADLK / LowestFree→fd / count++` |
| `invalidate_filp` | `filedes.c:250` | `Filp::invalidate()` | `mode=CLOSED` |
| `invalidate_filp_by_endpt` | `filedes.c:298` | `FProcTable::invalidate_by_endpoint(ep) -> usize` | `v_fs_e==ep → CLOSED` 计数 |
| `Fd` | `int fd` | `filedes.rs:Fd(u8)` | `TryFrom<usize> → Option<Fd>` 的 `EBADF` 早拒绝 |
| `FdAllocPolicy` | `get_fd:121 for` | `trait FdAllocPolicy::allocate(table, start)->Option<usize>` | `LowestFree` vs `NextFit` 双实现 |

### 4.3 不变量

| 不变量 | 位置 | 守卫 | 证据 |
|--------|------|------|------|
| 空闲 `fp_filp==NULL` | `FProc::alloc_fd` | `table[start..].iter().position(|f| f.is_none())` | `filedes.c:121` |
| `fd` 上界 `0..256` | `Fd::new` | `if fd>=256 → BadFd` | `filedes.c:188 if(fild<0||>=OPEN_MAX)` |
| `CLOSED→EIO` | `FProc::get` | `mode==CLOSED → Err(Io)` | `filedes.c:191` |
| `cloexec` 位集 `FD_SET/FD_CLR` | `Bitmap` | `cloexec_set.set(fd)` | `open.c:710` |
| `COPYFD` 三操作 | `CopyFdKind` | `match From/To/Close` 穷尽 | `filedes.c:578` |
| `invalidate` 计数 | `invalidate_by_endpoint` | `for(filp: v_fs_e==ep) CLOSED → count` | `filedes.c:298` |

---

## 5 测试要点

> 基线：`cargo test -p minix-vfs --lib` 截至 2026-09-03 为 **192 passed / 0 failed**（`fproc` 13 + `main_loop` 35 + `worker` 19 + `call_table` 8 + `filp` 7 + `vnode` 7 + `vmnt` 7 + `tll` 6 + `ipc/dispatcher` 16 + `fs_comm` 19 + `request` 20 + `path` 15 + `filedes` 14 = 186 → `cargo test` 实测 192；`minix-types` 108 独立）。
> 本章直接影响 `14` 项新增（`fd_new/check_fds/get_fd_lowest/get_fd_enfile/close_ebadf/close_eio/close_ok/cloexec_copy/invalidate/invalidate_by_endpt/copy_from/copy_to/copy_close/fd_alloc_policy_two_impls`），`minix-vfs --lib` 总计 178 → 192。

| 测试名 | 覆盖 C 行号 | 行为 | 文件 |
|--------|-------------|------|------|
| `test_fd_new` | `filedes.c:188` | `Fd(256)→BadFd, 255→Ok` 的 `u8` 上界 | `filedes.rs` |
| `test_check_fds` | `filedes.c:88` | `available 2 → nfds 3→EMFILE` | `filedes.rs` |
| `test_get_fd_lowest` | `filedes.c:110` | `start=0→Fd0, 占用0→Fd1` 的 `LowestFree` | `filedes.rs` |
| `test_get_fd_enfile` | `filedes.c:154` | `filp 1024 耗尽→ENFILE` | `filedes.rs` |
| `test_close_ebadf` | `open.c:690` | `fd 99 NULL→EBADF` | `filedes.rs` |
| `test_close_eio` | `filedes.c:191` | `mode==CLOSED→EIO` 的 `FILP_CLOSED` 抑制 | `filedes.rs` |
| `test_close_ok` | `open.c:690` | `close_fd→NULL + FD_CLR + count--` | `filedes.rs` |
| `test_cloexec_copy` | `filedes.c:524` | `From→To→Cloexec` 的 `FD_SET` 位集 | `filedes.rs` |
| `test_invalidate` | `filedes.c:250` | `invalidate→CLOSED` 单写 | `filedes.rs` |
| `test_invalidate_by_endpt` | `filedes.c:298` | `fs_e==ep→CLOSED` 计数 `2` | `filedes.rs` |
| `test_copy_from` | `filedes.c:579` | `COPYFD_FROM` 的 `S_ISSOCK→EDEADLK` 守门 | `filedes.rs` |
| `test_copy_to` | `filedes.c:618` | `COPYFD_TO` 的 `LowestFree` 分配 `fd` | `filedes.rs` |
| `test_copy_close` | `filedes.c:636` | `COPYFD_CLOSE` 的 `count>1→NULL` 回滚 | `filedes.rs` |
| `test_fd_alloc_policy_two_impls` | `filedes.c:121` | `FdAllocPolicy` trait `LowestFree vs NextFit` 的 `allocate` 行为差异 `start 5→5 vs 10` | `filedes.rs` |

测试策略：`Fd` 的 `u8` 上界以 `Fd(256)→BadFd` 的 `TryFrom` 早拒绝样本覆盖；`check_fds` 以 `available 2, nfds 3→EMFILE` 的 `nfds` 窗口样本覆盖；`get_fd` 以 `LowestFree(0)→0` 的 `start..256` 扫描样本覆盖；`close_fd` 以 `EBADF/EIO/OK` 的三守门样本覆盖；`FdAllocPolicy` 以 `LowestFree(5→5) vs NextFit(5→10)` 的 `dyn` 行为差异样本覆盖。

---

## 6 过渡

本篇在 `13-path-lookup` 的 `Lookup{path,flags}` 的 `EENTERMOUNT` 挂载穿越与 `04-filp-table.md` 的 `FilpTable::alloc` 双扫描之后，`fp_filp[256]` 的 `NULL` 空闲在 `get_fd` 的 `start→OPEN_MAX` 最低空闲可观测，是 `15-open-close` 的 `common_open` 的 `get_fd` 前置、16 的 `read/write` 的 `get_filp` 前置、17 的 `pipe` 的 `suspend` 前置、23 的 `select` 的 `check_fds` 前置、30 的 `fcntl` 的 `lock` 前置。

```
13-path-lookup: path.rs 的 Lookup + advance 的 get_free→lookup→find/dup  （EENTER 的 char_processed 回带）
  │
  └─► 本章: filedes.rs 的 Fd(FdAllocPolicy) + check_fds + alloc_fd → FilpId + close_fd(FILP_CLOSED 抑制) + copy_fd(FROM/TO/CLOSE) + invalidate(FILP_CLOSED)  （fp_filp[256] 的 NULL 空闲与 FD_CLOEXEC 位集）
         │
         ├─► 15-open-close: common_open 的 get_fd→filp_count=1→forbidden→open  （本章 get_fd 的 Filp 双扫描对端）
         ├─► 16-read-write: read 的 get_filp(READ)→lock_filp→read_write  （本章 get_filp 的 EIO 抑制对端）
         ├─► 17-pipe: pipe 的 find_filp(vp,R_BIT) 与 suspend 的 reviving  （本章 find_filp 的 vnode 反查对端）
         └─► 23-select: select 的 check_fds 的 nfds 窗口  （本章 check_fds 的 nfds 窗口对端）
```

`FD_CLOEXEC` 的 `Bitmap` 位集在 `fproc.rs:cloexec_set` 的 `FD_SET/FD_CLR` 可观测，为 `15` 的 `O_CLOEXEC` 的 `cloexec` 置位提供 `get_fd` 的 `FD_SET` 可观测。阅读顺序提示：若关心“`open` 如何从 `fd` 到 `filp` 到 `vnode`”，下一站 `15-open-close.md`；若关心“`fd` 如何被污染与失效”，下一站 `19-device-map.md` 的 `invalidate_filp_by_endpt` 调用点。

---

## 7 参见

- C 源：`minix3/minix/servers/vfs/filedes.c:88-656`（`88 check_fds` 的 `nfds` 窗口、`110 get_fd` 的 `start→OPEN_MAX` 扫描与 `filp_count==0` 双守门、`162 get_filp` 的 `EBADF/EIO` 双守门、`205 find_filp` 的 `vno==vp` 反查、`250 invalidate_filp` 的 `CLOSED` 单写、`298 invalidate_filp_by_endpt` 的 `fs_e` 守门、`313 lock_filp` 的 `softlock`、`413 close_filp` 的 `S_ISCHR/S_ISBLK` 分流、`524 do_copyfd` 的 `FROM/TO/CLOSE` 三操作）、`minix3/minix/servers/vfs/open.c:690-727`（`690 close_fd` 的 `NULL→EBADF→NULL fd→close_filp→FD_CLR→lock_revive`）、`minix3/minix/servers/vfs/file.h:filp`（`filp_count/mode`）、`minix3/minix/servers/vfs/fproc.h:fp_filp`（`fp_filp[256]` 私有索引）、`minix3/minix/include/sys/syslimits.h:38`（`OPEN_MAX 256`）、`minix3/minix/servers/vfs/const.h:NR_FILPS`（`NR_FILPS 1024`）
- 阶段文档：`02-fproc-struct.md`（`FProc{filps,cloexec_set}` 的 `fp_filp` 私有索引）、`04-filp-table.md`（`FilpTable` 的 `count==0` 哨兵）、`10-pm-protocol.md`（`copy_fproc` 的 `filp_shared` 计数）、`99-global-concepts.md`（`OPEN_MAX/NR_FILPS` 常量与 `Fd` 术语）
- Rust 实现：`os/servers/vfs/src/filedes.rs:1`（`Fd/FdError + FdAllocPolicy(LowestFree vs NextFit) + check_fds/get_fd/close_fd/invalidate/copy_fd`）、`os/servers/vfs/src/fproc.rs:1`（`FProc.filps: [Option<FilpId>;256]` 的 `FP_CLOSED` 抑制与 `cloexec_set:Bitmap`）、`os/libs/minix-types/src/types/endpoint.rs:1`（`Endpoint` 与 `UserSlot`）
- 内核侧：`../01-stage-kernel/18-syscall-copy.md`（`sys_datacopy` 的跨进程 `COPYFD` 拷贝）
