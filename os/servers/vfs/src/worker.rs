//! Worker slot pool — the single-threaded state-machine analogue of Minix3's `mthread` workers.
//!
//! Minix3 VFS is the only server that uses real threads (`worker.c` + `threads.h`).
//! Nine `mthread` threads are created at boot (`NR_WTHREADS = 9`), each bound to at
//! most one `fproc` via the bidirectional `w_fp ↔ fp_worker` link.  A blocking
//! file-system call (`pipe`, `select`, `cdev`/`sdev`, `FS sendrec`, `F_SETLKW`)
//! executes `worker_suspend`/`worker_wait` (`cond_wait` on `w_event`) so that the
//! main thread can keep dispatching through `get_work`'s `reviving` fast-path.
//!
//! # Why the rewrite eliminates threads
//!
//! `ARCH A-1` (plan.md): the 9 threads exist only to isolate blocking side-effects.
//! In a single-threaded event loop that isolation is obtained by turning "blocked"
//! into an explicit reply intent (`ReplyIntent::ReplyLater` in `main_loop.rs`) and
//! by turning "worker bound to fproc" into a **request-slot state machine**.  The
//! slot keeps `Idle → Busy → WaitingForFs / Suspended → Idle` instead of
//! `mthread_create` + `cond_signal`.  `pending` / `busy` / `allow` remain as the
//! observable scheduling invariants; `TH_STACKSIZE` disappears.
//!
//! # Relation to Redox / Linux
//!
//! * **Linux** `workqueue` — `pending` is `work_struct.pending` linked on
//!   `pool.worklist`, `busy` is `pool.nr_running`, `spare` is `rescuer_thread`.
//!   `block_all` is `freeze_workqueues_begin`.
//! * **Redox** async `Scheme` — each `Scheme::handle` is a `Future`; `w_fp`
//!   binding is `SchemeId → TaskId`; `suspend` is `Future::poll(Pending)` with
//!   the `Waker` stored in the slot; `block_all` is `executor.block_on(root_mount)`.
//! * **seL4** passive server — no threads; `pending` is the kernel `endpoint`
//!   queue, `spare` is the `notification` badge used for deadlock callbacks.
//!
//! Common constraint: *a blocking call must not poison the server*.
//! Minix3 chooses 9 fixed kernel-visible threads; the rewrite chooses 9
//! user-visible request slots with the same `may_do_pending` spare invariant.

extern crate alloc;

use alloc::boxed::Box;
use core::cell::Cell;

use minix_types::{Endpoint, Message, UserSlot};

use crate::fproc::{FProc, FpFlags};

/// Number of worker slots — `const.h:9` `NR_WTHREADS 9`.
pub const NR_WTHREADS: usize = 9;

/// Handler executed in a worker slot.
///
/// In C `worker_start` accepts `void (*func)(void)`; `func == NULL` means
/// "PM postponed work" (`FP_PM_WORK`, `fproc.h:98`), otherwise it is the
/// normal `do_work` / `do_pending_pipe` / `ds_event` / `pm_reboot` entry.
/// The Rust enum makes that discriminator exhaustive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerFunc {
    /// Normal syscall path (`do_work`, `table.c:call_vec`).
    DoWork,
    /// Deferred pipe resume (`do_pending_pipe`, `main.c:216`).
    DoPendingPipe,
    /// DS driver event (`ds_event`, `main.c:105`).
    DsEvent,
    /// Reboot helper (`pm_reboot`, `main.c:901`).
    PmReboot,
    /// PM postponed work (`service_pm_postponed`, `main.c:668`).
    ///
    /// This is the `func == NULL` track — the job is stored as
    /// `FP_PM_WORK | fp_pm_msg` in C and consumed in `worker_main:270`.
    PmPostponed,
}

impl WorkerFunc {
    /// `true` iff this function is the PM-postponed track (`func == NULL` in C).
    pub fn is_pm_work(self) -> bool {
        matches!(self, Self::PmPostponed | Self::PmReboot)
    }
}

/// 续接标识：臂挂起前登记"FS 回复到了之后要做什么"。
///
/// C 的"回复后半段"活在 worker 线程的栈上（`fs_sendrec` 里 `worker_wait()`
/// 让出，回复到达后线程从该点继续，调用方接着解回复、拷出、回用户），
/// 单线程事件循环没有可恢复的栈，故把这一半显式成一个标记 + 少量参数。
/// **参数尽量从槽上已有的 `input`（原始请求）与 `sendrec`（回复）重算**，
/// 只存重算不出来的东西（如已发出的 grant id）。
/// `socketpair` 的成对编排状态（两半各带一份，见 `WorkerCont::SockFd`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PairState {
    /// 另一半的设备号（**第一半**用它起第二半；第二半不用）。
    pub other_dev: u64,
    /// 已建好的第一个 fd（**第二半**用它拼 `fdpair` 回复）。
    pub fd0: u32,
    /// 这份状态属于第二半吗（第一半为 `false`）。
    pub second: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerCont {
    /// 字符设备 open 的对话半（`CDEV_OPEN`）：回复是
    /// `mess_lchardriver_vfs_reply { status, id }`（状态在首字）；`status >= 0`
    /// 时低位带 `CDEV_CLONED`/`CDEV_CTTY` 两个效果位——克隆要 PFS（未接）、
    /// 控制终端要写回 `fp_tty` 与该 dmap 行的 `dmap_seen_tty`。
    /// C `cdev_opcl`（cdev.c:236-249）。
    CdevOpen {
        /// 已认领的 fd（成功时作为返回值）。
        fd: u32,
        /// 已认领的 filp（失败时要放开）。
        filp: usize,
        /// 目标设备号（`CDEV_CTTY` 效果要把它写进 `fp_tty`）。
        dev: u64,
    },
    /// 块设备 ioctl 的对话半（`BDEV_IOCTL`）：回复是
    /// `mess_lblockdriver_lbdev_reply { int status; int id; }`（状态在首字）；
    /// 续接体撤销 grant、清 `filp_ioctl_fp` 守卫并把状态回给用户。
    /// C `bdev_ioctl`（bdev.c:144-186）。
    BdevIoctl {
        /// 已发给驱动的 magic grant（收尾时撤销）。
        grant: i32,
        /// 被 ioctl 占着的 filp（收尾时清 `ioctl_holder`）。
        filp: usize,
    },
    /// 字符设备 ioctl 的对话半（`CDEV_IOCTL`）：回复是
    /// `mess_lchardriver_vfs_reply { int status; uint32_t id; }`，续接体撤销
    /// ioctl 的 magic grant 并把状态回给用户。C `cdev_io`（cdev.c:277-340）。
    CdevIoctl {
        /// 已发给驱动的 magic grant（收尾时撤销）。
        grant: i32,
    },
    /// `Fstat`（`do_fstat` → `REQ_STAT`）：回复只有状态字，续接就是把
    /// 用户的 grant 撤掉再把状态回给用户。
    Fstat {
        /// 已发给 FS 的 magic grant（续接里 revoke，C request.c:1109）。
        grant: i32,
    },
    /// `chown` 的对话半（`REQ_CHOWN`）：回复的 `mode` 是新的模式
    /// （C `do_chown:159-163`），uid/gid 由本续接体自己写进 vnode。
    Chown {
        /// 被改归属的 vnode 下标。
        vnode: usize,
        /// 折算后的新属主（`-1` 已经用现有值替代）。
        uid: u32,
        /// 折算后的新属组。
        gid: u32,
    },
    /// 套接字驱动对话的对话半（`SDEV_SOCKET`/`SDEV_SOCKETPAIR`）：回复带
    /// 新套接字的设备号（单个或一对），续接体解析后转 `make_sock_fd`
    /// （`WorkerCont::SockFd`）。C `sdev_socket`（sdev.c:124-170）。
    SdevSocket {
        /// `true` 是 `SDEV_SOCKETPAIR`（要两个设备号）。
        pair: bool,
        /// 驱动回复里带的打开标志（`socket::sock_flags` 已翻成 `O_*`）。
        flags: u32,
        /// 该域的 smap 行号（`make_smap_dev` 的高 32 位）。
        smap_num: u32,
    },
    /// 套接字驱动 getset 族的对话半（`SDEV_SETSOCKOPT`/`SDEV_GETSOCKOPT`/
    /// `SDEV_GETSOCKNAME`/`SDEV_GETPEERNAME`）：回复号必须是 `SDEV_REPLY`，
    /// **状态在回复载荷里**；`get` 方向（状态 ≥ 0）时那个状态就是**新长度**，
    /// 由续接体放进回复载荷（`m_vfs_lc_socklen { len }`）。C `sdev_getset`
    /// （sdev.c:450-555）。
    SdevGetSet {
        /// 已发给驱动的 magic grant（收尾时撤销）。
        grant: i32,
        /// `true` = `get` 方向（状态是新长度，要放进回复载荷）。
        write_dir: bool,
    },
    /// 套接字驱动"简单请求"的对话半（`SDEV_LISTEN`/`SDEV_SHUTDOWN`/
    /// `SDEV_CLOSE`）：回复号必须是 `SDEV_REPLY`，**状态在回复载荷里**
    /// （`mess_lsockdriver_vfs_reply.status`）。C `sdev_simple`（sdev.c:245-276）。
    SdevSimple,
    /// `make_sock_fd` 的对话半（`REQ_NEWNODE` 到 PFS）：回复带新节点的
    /// `node_details`，续接体填 vnode 与 filp 并把 fd 作为**返回值**回给用户。
    /// C `make_sock_fd`（socket.c:86-176）。
    SockFd {
        /// 已认领的 filp。
        filp: usize,
        /// 已认领的 fd 号。
        fd: u32,
        /// 打开标志（`O_CLOEXEC` 等）。
        flags: u32,
        /// 预留的 vnode 下标。
        vnode: usize,
        /// 套接字设备号（写进 vnode 的 `v_sdev`）。
        dev: u64,
        /// `accept` 的收尾还要把**对端地址长度**放进回复载荷
        /// （`m_vfs_lc_socklen { len }`，C `resume_accept` 的末段）；其余调用
        /// 为 `None`。
        addr_len_out: Option<u32>,
        /// `socketpair` 的成对编排状态（C `do_socketpair` socket.c:239-266）：
        /// 第一半带着"另一半的设备号"、第二半带着"第一个 fd"（最后要拼
        /// `m_vfs_lc_fdpair { fd0, fd1 }` 回复）。
        pair: Option<PairState>,
    },
    /// `accept` 失败但**驱动已建了新套接字**时（C 的 case #2 与 case #1 的
    /// `make_sock_fd` 失败）：先给驱动发 `SDEV_CLOSE` 把那个套接字关掉，再回
    /// **原来的错误**（C 的 `(void)sdev_close(dev, ...)` 不看它的状态）。
    SdevCloseThenReply {
        /// 要回给用户的原始状态。
        status: i32,
    },
    /// `pipe2` 的对话半（`REQ_NEWNODE`）：回复带新节点的 `node_details`，续接体
    /// 要用它填 vnode 与两个 filp，并把 `m_vfs_lc_fdpair { fd0, fd1 }` 作为
    /// **回复载荷**发回（用户拿到的就是这两个 fd）。C `create_pipe`
    /// （pipe.c:58-135）的后半。
    Pipe2 {
        /// 读端 filp（已认领，等 vnode 落位）。
        filp0: usize,
        /// 写端 filp。
        filp1: usize,
        /// 读端 fd 号。
        fd0: u32,
        /// 写端 fd 号。
        fd1: u32,
        /// `flags | oflags`（CLOEXEC 位要用）。
        flags: i32,
        /// 预留的 vnode 下标。
        vnode: usize,
    },
    /// `statvfs` 的对话半（`REQ_STATVFS`）：回复只有状态，FS 已经把统计量写
    /// 进 VFS 侧的 `struct statvfs` 缓冲了；续接体要撤销 grant、把 FS 那 17 个
    /// 字段存进挂载行的缓存（C `update_statvfs`）、补本地字段，再整块拷给用户
    /// （C `fill_statvfs` 尾部的 `sys_datacopy_wrapper`）。
    Statvfs {
        /// 已发给 FS 的 direct grant（收尾时撤销）。
        grant: i32,
        /// 用户缓冲地址（拷给它的目的地；`getvfsstat` 时随序号推进）。
        user_buf: u64,
        /// 被查询的挂载行下标（缓存写回处）。
        vmnt: usize,
        /// `getvfsstat` 的多挂载序列：前 `seq_count` 个有效（`0` = 单挂载的
        /// `statvfs1`/`fstatvfs1`）。
        seq: [usize; crate::vmnt::NR_MNTS],
        /// 序列长度。
        seq_count: u8,
        /// 当前是第几个（回复到达后推进）。
        seq_at: u8,
    },
    /// `chmod` 的对话半（`REQ_CHMOD`）：回复的 `mode` 是实际生效的模式
    /// （C request.c:127），成功时回写 vnode 缓存（C `vp->v_mode =
    /// result_mode`，protect.c:127-128）。
    Chmod {
        /// 被改模式的 vnode 下标（走完时并入缓存的那个）。
        vnode: usize,
    },
    /// `sync`/`fsync` 的**多挂载序列**（C `do_sync` misc.c:276-296 与
    /// `do_fsync` misc.c:229-267 的 `for (vmp = &vmnt[0]; ...)` 循环）：
    /// 每个匹配的挂载各发一条 `REQ_SYNC`，逐条等回复（单线程模型里就是逐段
    /// 挂起）。C 把 `req_sync` 的返回值**丢掉**，只保留加锁失败的错误——所以
    /// 序列跑完报的是 `first_err`（通常 0）。
    ///
    /// 用定长数组而不是 `Vec`：`WorkerCont` 必须是 `Copy`（驱动层
    /// `let (Some(cont), Some(reply)) = (wp.cont, wp.sendrec)` 直接按值取），
    /// 而 `NR_MNTS` 只有 16，定长数组够用且保住了 `Copy`。
    SyncMounts {
        /// 待发的挂载 FS 端点（前 `count` 个有效）。
        targets: [Endpoint; crate::vmnt::NR_MNTS],
        /// 有效元素个数。
        count: u8,
        /// 下一个要发的下标。
        at: u8,
        /// 要报给用户的错误（C 里只有加锁失败会写它）。
        first_err: i32,
    },
    /// `readlink` 的对话半（`REQ_RDLINK`）：C `req_rdlink_actual`
    /// （request.c:741-747）——回复的 `m_type` 只是 `OK`，**字节数在载荷里**
    /// （`mess_fs_vfs_rdlink { size_t nbytes; }`），所以不能复用 `Status`。
    Rdlink {
        /// 已发给 FS 的 magic grant（收尾时撤销）。
        grant: i32,
    },
    /// `getdents` 的对话半（`REQ_GETDENTS`）：C `do_getdents`（read.c:282-317）
    /// 的回复处理与 read/write 不同——**只有 `nbytes > 0` 才推进 filp 位置**
    /// （C `if (r > 0) rfilp->filp_pos = new_pos;`），也不动 vnode 大小，
    /// 所以不能复用 `Transfer`。
    Getdents {
        /// 已发给 FS 的 magic grant（收尾时撤销）。
        grant: i32,
        /// 目标 filp 下标（位置写回处）。
        filp: usize,
    },
    /// `open` 的 `O_TRUNC` 分支（`REQ_FTRUNC`）：C `common_open:150-157` 对
    /// 常规文件先过 W 位门、再 `truncate_vnode(vp, 0)`，**截断结果被忽略**
    /// （C 没接返回值），随后照常装配 fd/filp。续接里按此继续本地半
    /// （`O_TRUNC` 位要清掉，否则 `dispatch_open` 会再判一次 NeedTruncate）。
    OpenTrunc {
        /// 走完的节点（本地半要用它的 mode/ino 做类型分派与并表）。
        node: crate::path::NodeDetails,
        /// 原始 `oflags`（含 `O_TRUNC`）。
        oflags: u32,
    },
    /// `creat` 阶段 3（`REQ_CREATE`）：回复是新建节点的 `node_details`，
    /// 续接里并进 vnode 表、再做 `common_open` 的本地半（fd/filp 装配）。
    Create {
        /// 调用方端点。
        user: Endpoint,
        /// 原始 `oflags`。
        oflags: u32,
    },
    /// 纯状态续接（`REQ_MKDIR`/`REQ_UNLINK` 一类"回了状态就完事"的请求）：
    /// 把回复的状态原样回给用户，无载荷、无副作用。
    Status,
    /// `Lseek` 的"抑制预读"请求（`REQ_INHIBREAD`）：位置已改，回复到达后
    /// 把新位置写进回复载荷（C `do_lseek` 的 `job_m_out.m_vfs_lc_lseek`）。
    InhibRead {
        /// 新的文件位置（回给用户）。
        offset: i64,
    },
    /// `Ftruncate`（`REQ_FTRUNC`）：回复只有状态，但成功时要按 C
    /// `truncate_vnode:382` 把 vnode 大小改成新长度。
    Ftrunc {
        /// 目标 vnode 下标（成功时更新大小）。
        vnode: usize,
        /// 新的文件长度（C 的 `newsize`）。
        newsize: i64,
    },
    /// 路径遍历（`REQ_LOOKUP` 一趟或多趟）：现场在 `WorkerSlot.path`
    /// （`PathPending`）——续接体把它 `take()` 出来、`resume()` 后决定
    /// "再发一条 lookup"（放回现场，继续挂起）还是"做相位 2"。
    Path,
    /// `Read`/`Write`（`REQ_READ`/`REQ_WRITE`）：回复的
    /// `seek_pos`/`nbytes` 要写回 filp 位置，状态是实际传输的字节数
    /// （C read.c 的 `cum_io`）；写方向还要按 C read.c:255-259 更新
    /// vnode 大小（位置越过旧大小即抬高）。
    Transfer {
        /// 已发给 FS 的 magic grant。
        grant: i32,
        /// 目标 filp 下标（位置推进要写回它）。
        filp: usize,
        /// 目标 vnode 下标（写方向的大小更新要回写它）。
        vnode: usize,
        /// 请求时的位置（C 的 `position`，回复里给的是新位置）。
        orig_pos: i64,
        /// 方向：`true` = 写（要更新 vnode 大小），`false` = 读。
        write: bool,
    },
}

/// 路径遍历的挂起现场（C 那份"活在 worker 线程栈上"的局部量）。
#[derive(Debug)]
pub struct PathPending {
    /// 遍历状态机（起点三元组 + symloop + 路径游标）。
    pub walk: crate::path::LookupWalk,
    /// 当前在途的路径 grant（回复到达后 revoke）。
    pub grant: i32,
    /// 走完之后做什么（臂的"相位 2"）。
    pub follow: PathFollow,
}

/// 路径走完之后的动作（C 里是 `eat_path` 返回后臂自己接着写的那段代码）。
///
/// 非 `Copy`：`Mkdir` 要带上"最后组件名"（`String`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathFollow {
    /// `rename(name1, name2)` 阶段 1：走**name1 的父目录**（`last_dir`）。
    /// 走通后先过粘滞位（开着就转 `RenameOldSticky` 子遍历），再转阶段 2
    /// （走 name2 的父目录）。C `do_rename`（link.c:166-280）。
    RenameOld {
        /// name1 的最后组件（**要保存的旧名**）。
        entry: alloc::string::String,
        /// name2（阶段 2 要切出它的父目录与组件名）。
        new_path: alloc::string::String,
    },
    /// `rename` 阶段 1 的粘滞位子遍历（只在旧父目录带粘滞位时走）：从旧父
    /// 目录起走 name1 的最后组件（`PATH_RET_SYMLINK`），取受害者属主。
    RenameOldSticky {
        /// 旧名（阶段 2 要用）。
        entry: alloc::string::String,
        /// name2。
        new_path: alloc::string::String,
    },
    /// `rename` 阶段 2：走 name2 的**父目录**，走通后过跨设备门与两个
    /// `W|X` 门，再发 `REQ_RENAME`。
    RenameNew {
        /// 旧父目录所在 FS 端点（跨设备门用它比）。
        old_fs_e: Endpoint,
        /// 旧父目录节点号（`REQ_RENAME` 的 `dir_old`）。
        old_ino: u64,
        /// 保存下来的旧名。
        old_name: alloc::string::String,
        /// name2 的最后组件（新名；阶段 2 起走时切好带进来）。
        new_entry: alloc::string::String,
    },
    /// `link(name1, name2)` 阶段 1：走**整条 name1**（要链接的源文件，
    /// C `do_link:188-189` 的 `eat_path`）。走通后转阶段 2（走 name2 的
    /// 父目录）。
    LinkSrc {
        /// name2（新链接的路径）——阶段 2 要重新切出父目录与组件名。
        dst_path: alloc::string::String,
    },
    /// `link` 阶段 2：走 name2 的**父目录**（`last_dir`），走通后过跨设备门与
    /// `W|X` 门再发 `REQ_LINK`。C `do_link:191-214`。
    LinkDst {
        /// 源文件所在 FS 端点（跨设备门用它比）。
        src_fs_e: Endpoint,
        /// 源文件的节点号（`REQ_LINK` 的 `inode` 域）。
        src_ino: u64,
        /// name2 的最后组件名。
        entry: alloc::string::String,
    },
    /// `symlink(target, linkpath)`：走完**父目录**（`last_dir`）过门后发
    /// `REQ_SLINK`——两个 grant：名字（VFS 内存，direct）+ 目标串（**用户
    /// 内存**，magic）。C `do_slink`（link.c:386-424）。
    Slink {
        /// 最后组件名（新链接的名字）。
        entry: alloc::string::String,
        /// 用户内存里目标串的地址。
        target_addr: u64,
        /// 目标串长度（**不含**结尾 NUL，C 传 `vname1_length - 1`）。
        target_len: u64,
    },
    /// `statvfs1(path, buf, flags)`：走完拿到 vnode 后进 `fill_statvfs`
    /// （挂载行 → 缓存或 FS 往返 → 本地字段 → 拷给用户）。C `do_statvfs`
    /// （stadir.c:294-326）。
    Statvfs {
        /// 用户 `struct statvfs` 缓冲地址。
        user_buf: u64,
        /// `ST_NOWAIT` 等标志。
        flags: i32,
    },
    /// `chdir(path)` / `chroot(path)`：走完改**本进程**的当前目录/根目录
    /// （`change_into`，stadir.c:120-140）——没有 FS 往返，走完即判即改。
    Chdir {
        /// `true` 改根目录（`fp_rd`），`false` 改当前目录（`fp_wd`）。
        into_root: bool,
    },
    /// `utimens(path, times, flags)`：走完过门后发 `REQ_UTIME`；时间里的
    /// `UTIME_NOW`/`UTIME_OMIT` 哨兵在走完之后才折算（要先知道节点的属主）。
    /// C `do_utimens`（time.c:44-160）。
    Utimens {
        /// 用户给的 atime（秒, 纳秒）。
        atime: (i64, i64),
        /// 用户给的 mtime（秒, 纳秒）。
        mtime: (i64, i64),
        /// `AT_SYMLINK_NOFOLLOW` 等标志（遍历标志在入口就用了，这里只做
        /// "未知标志即 EINVAL"的判定）。
        flags: u32,
    },
    /// `mknod(path, mode, dev)`：走完**父目录**（`last_dir`）过门后发
    /// `REQ_MKNOD`。C `do_mknod`（open.c:514-556）。
    Mknod {
        /// 最后组件名。
        entry: alloc::string::String,
        /// 已按 umask 收窄的完整模式（含类型位）。
        mode_bits: u32,
        /// 设备号（FIFO/常规文件为 0）。
        dev: u64,
    },
    /// `truncate(path, length)`：走完过写位门后发 `REQ_FTRUNC`（与
    /// `Ftruncate` 共用发送半）。C `do_truncate`（link.c:277-326）的路径半。
    Truncate {
        /// 新长度（负值已在入口挡掉）。
        length: i64,
    },
    /// `chown(path, uid, gid)`：走完过门后发 `REQ_CHOWN`；回复带新的模式，
    /// uid/gid 由续接体写回 vnode 缓存。C `do_chown`（protect.c:24-110）的
    /// 路径半。
    Chown {
        /// 用户给的属主（`u32::MAX` = 不改，C 的 `-1`）。
        uid: u32,
        /// 用户给的属组（同上）。
        gid: u32,
    },
    /// `unlink(path)` / `rmdir(path)` 阶段 1：走**父目录**（`last_dir` 的
    /// 目录前缀）。走通后过三道门（父目录类型 → `X|W` 权限 → 粘滞位），
    /// 粘滞位开着就转阶段 2（子遍历取受害者属主），否则直接发
    /// `REQ_UNLINK`/`REQ_RMDIR`。C `do_unlink`（link.c:94-163）。
    Unlink {
        /// 最后组件名（发给 FS 的名字）。
        entry: alloc::string::String,
        /// `true` 走 `REQ_RMDIR`，`false` 走 `REQ_UNLINK`（C 按 `job_call_nr`
        /// 分流，link.c:156-159）。
        rmdir: bool,
    },
    /// `unlink`/`rmdir` 阶段 2（**只在粘滞位目录上走**）：从父目录起走
    /// 最后组件本身（`PATH_RET_SYMLINK`，不跟进），拿到受害者属主后过
    /// 粘滞位门，再发请求。C `do_unlink:132-152` 的 `advance(dirp, ...)`。
    UnlinkSticky {
        /// 最后组件名。
        entry: alloc::string::String,
        /// 是否 rmdir。
        rmdir: bool,
        /// 父目录所在 FS 端点（发请求的目标，阶段 2 的回复要用）。
        dir_fs_e: Endpoint,
        /// 父目录节点号（`REQ_UNLINK` 的 `inode` 域）。
        dir_ino: u64,
    },
    /// `chmod(path, mode)`：走完过权限门后发 `REQ_CHMOD`，回复带**实际生效
    /// 的模式**（FS 可能收窄），由续接体回写 vnode 缓存。C `do_chmod`
    /// （protect.c:62-133）的路径半。
    Chmod {
        /// 调用方端点（回复目的地）。
        user: Endpoint,
        /// 用户给的模式位（setgid 位可能被清，见续接前的本地半）。
        mode: u32,
    },
    /// `readlink(path, buf, bufsize)`：走完（**不跟进末组件符号链接**，
    /// `PATH_RET_SYMLINK`）后发 `REQ_RDLINK`，链接文本由 FS 经 magic grant
    /// 写进用户缓冲。C `do_rdlink`（link.c:473-507）。
    Rdlink {
        /// 调用方端点（magic grant 的 `who_from` 与回复目的地）。
        user: Endpoint,
        /// 用户缓冲地址。
        buf: u64,
        /// 用户给的窗口大小。
        buf_size: u64,
    },
    /// `access(path, mode)`：走完就结束——权限判断全在本地
    /// （C `do_access`，protect.c:199-233：`eat_path` 之后只剩一次
    /// `forbidden`），**没有 FS 往返**，所以这个 follow 的续接体直接回状态。
    Access {
        /// 调用方端点（回复目的地）。
        user: Endpoint,
        /// 用户给的 `mode`（`R_OK`/`W_OK`/`X_OK`/`F_OK` 的位组合）。
        access: u32,
    },
    /// `stat(path, buf)`：按走完的 ino 发 `REQ_STAT`（C `do_stat` →
    /// `req_stat`），随后由 `WorkerCont::Fstat` 续接收尾。
    Stat {
        /// 调用方端点（stat 缓冲的属主，magic grant 的 `who_from`）。
        user: Endpoint,
        /// 用户 `struct stat` 缓冲地址。
        buf: u64,
    },
    /// `open(path, flags)`：走完后做 `common_open` 的**本地半**（类型分派 +
    /// fd/filp 装配 + 回 fd），C `open.c:118-274` 里不碰 FS 的那些步。
    Open {
        /// 调用方端点（回复目的地）。
        user: Endpoint,
        /// 原始 `oflags`（C 的 `open_flags`）。
        oflags: u32,
    },
    /// `creat(path, flags, mode)` 阶段 1：**走整条路径**。走通＝文件已存在
    /// （`O_EXCL` 时是 EEXIST）；ENOENT 则转阶段 2（走父目录）。
    /// C `common_open` 的 O_CREAT 支（open.c:100-135）。
    Creat {
        /// 调用方端点。
        user: Endpoint,
        /// 原始 `oflags`（含 `O_CREAT`）。
        oflags: u32,
        /// 新建节点的模式位（已按 umask 收窄）。
        mode: u32,
        /// 原路径（阶段 2 要重新 split 出父目录与组件名）。
        path: alloc::string::String,
    },
    /// `creat` 阶段 2：走父目录（`last_dir_split` 的 `dir_path`），走通后发
    /// `REQ_CREATE`。C `new_node` 的 `last_dir` 那一步（open.c:322）。
    CreatInDir {
        /// 调用方端点。
        user: Endpoint,
        /// 原始 `oflags`。
        oflags: u32,
        /// 新建节点的模式位。
        mode: u32,
        /// 最后组件名（新节点的名字）。
        entry: alloc::string::String,
    },
    /// `mkdir(path, mode)`：**走的是父目录**（`last_dir_split` 的
    /// `dir_path`），走完后发 `REQ_MKDIR`（C `do_mkdir` 的
    /// `last_dir` + `req_mkdir`）。
    Mkdir {
        /// 调用方端点。
        user: Endpoint,
        /// 最后组件名（新目录的名字）。
        entry: alloc::string::String,
        /// 建目录的权限位（已含 `I_DIRECTORY` 与 umask）。
        mode: u32,
    },
}

/// Observable state of a single worker slot.
///
/// Collapses `threads.h:w_fp` + `tll` wait state + `w_task` / `w_sendrec` into
/// a type-state that can be matched in the event loop without `cond_wait`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerState {
    /// `w_fp == NULL` — slot free (`worker.c:45`).
    Idle,
    /// Bound to an `fproc` and executing (`worker_assign:139` `busy++`).
    Busy,
    /// `fs_sendrec` / `drv_sendrec` in flight (`w_task != NONE`, `worker_stop:539`).
    WaitingForFs,
    /// `worker_suspend` saved the coroutine context (`worker.c:485` `w_err_code`).
    Suspended,
}

/// One slot in the fixed pool.
///
/// Mirrors `struct worker_thread` (`threads.h:23`) field-by-field, but with
/// `Option` instead of nullable pointers and without `w_tid` / `w_event_mutex`
/// / `w_event` / `w_next` — those are `mthread` artefacts.  `w_next` (the
/// `tll_append` queue link, `tll.c:35`) is modelled by `WorkerPool.pending_q`
/// (`VecDeque` per `tll` lock) at the pool level.
#[derive(Debug)]
pub struct WorkerSlot {
    /// `w_fp` — the bound `fproc` slot, if any.
    pub fp_slot: Option<UserSlot>,
    /// `w_task` — FS/DRV endpoint we are waiting for.
    pub task: Option<Endpoint>,
    /// `w_sendrec` / `w_drv_sendrec` — at most one is `Some` while `WaitingForFs`.
    pub sendrec: Option<Message>,
    /// `w_m_in` — input message for the current job (`worker_main:261`).
    pub input: Option<Message>,
    /// `w_err_code` — saved `err_code` across `suspend` (`worker.c:485/504`).
    pub saved_err: Option<i32>,
    /// Current state.
    pub state: WorkerState,
    /// Which handler will run (`fp_func` in C, per-slot here — `ARCH A-6`).
    pub func: Option<WorkerFunc>,
    /// Slot index (`self` in `worker_main:243` `ASSERTW(self)`).
    pub self_index: usize,
    /// 续接标识（`Some` = 该槽的作业在等 FS 回复，回复到了要跑续接体；
    /// C 的"后半段在栈上"在单线程模型里的显式对应物）。
    pub cont: Option<WorkerCont>,
    /// 路径遍历现场（`WorkerCont::Path` 时必有）：跨多次 `REQ_LOOKUP`
    /// 存活，直到走完或出错。
    pub path: Option<PathPending>,
    /// 路径 grant 的源缓冲：grant 要 NUL 结尾的字节区，而 `String` 没有
    /// 结尾 NUL，故每次 lookup 把当前路径拷进来 + 补 NUL 再授权；放在槽上
    /// 是因为它必须活过"挂起 → 回复"这段（每槽一块，池子建一次）。
    pub path_scratch: Box<[u8; crate::path::PATH_MAX]>,
}

impl WorkerSlot {
    /// Idle slot (`w_fp = NULL`, `w_task = NONE`).
    pub fn new(index: usize) -> Self {
        Self {
            fp_slot: None,
            task: None,
            sendrec: None,
            input: None,
            saved_err: None,
            state: WorkerState::Idle,
            func: None,
            self_index: index,
            cont: None,
            path: None,
            path_scratch: Box::new([0u8; crate::path::PATH_MAX]),
        }
    }

    /// `w_fp == NULL` — the C `NULL` check inlined.
    pub fn is_idle(&self) -> bool {
        self.state == WorkerState::Idle
    }

    /// Bind the slot (`worker_assign` + `worker_start` fast path).
    pub fn bind(&mut self, slot: UserSlot, func: WorkerFunc, msg: &Message) {
        self.fp_slot = Some(slot);
        self.func = Some(func);
        self.input = Some(*msg);
        self.state = WorkerState::Busy;
    }

    /// Release the slot (`worker_main:283` `w_fp = NULL, busy--`).
    pub fn release(&mut self) {
        self.fp_slot = None;
        self.task = None;
        self.sendrec = None;
        self.input = None;
        self.saved_err = None;
        self.state = WorkerState::Idle;
        self.func = None;
        self.cont = None;
        self.path = None;
    }

    /// `WaitingForFs` — `fs_sendrec` in flight (`worker.c:539` `w_task != NONE`).
    pub fn set_waiting(&mut self, task: Endpoint, sendrec: Message) {
        self.task = Some(task);
        self.sendrec = Some(sendrec);
        self.state = WorkerState::WaitingForFs;
    }

    /// `Suspended` — `worker_suspend:485`.
    pub fn suspend(&mut self, err: i32) -> SuspendToken {
        debug_assert!(self.state == WorkerState::Busy);
        self.saved_err = Some(err);
        self.state = WorkerState::Suspended;
        SuspendToken {
            slot_index: self.self_index,
            slot: self.fp_slot.expect("suspend on unbound slot"),
            saved_err: err,
        }
    }

    /// `worker_resume:493` — consumes the token.
    pub fn resume(&mut self, tok: SuspendToken) {
        debug_assert_eq!(self.self_index, tok.slot_index);
        debug_assert_eq!(self.fp_slot, Some(tok.slot));
        debug_assert_eq!(self.state, WorkerState::Suspended);
        self.saved_err = None;
        self.state = WorkerState::Busy;
    }
}

/// Opaque coroutine token from `suspend` — must be consumed by `resume`.
///
/// Models `struct worker_thread *org_self` (`worker_suspend:474`) with
/// `w_err_code` piggy-backed, but as an owned value rather than a raw
/// pointer, so `resume` cannot be called twice and cannot resume the wrong
/// slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuspendToken {
    slot_index: usize,
    slot: UserSlot,
    saved_err: i32,
}

/// Outcome of `try_activate` (`worker.c:331`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivateOutcome {
    /// `worker_assign` succeeded — `busy++` and slot is now `Busy`.
    Assigned(usize),
    /// No spare slot or `block_all` gated — `FP_PENDING` and `pending++`.
    Queued,
}

/// Error from `WorkerPool::start` — the `panic("…")` branches in
/// `worker_start:382-408` turned into typed errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerError {
    /// `is_pending && is_active` — `panic("work cannot be both pending and active")`.
    BothPendingAndActive,
    /// Normal work already present (`fp_func != NULL`) for this `fproc`.
    AlreadyHasNormal,
    /// PM work already present (`FP_PM_WORK`) for this `fproc`.
    AlreadyHasPm,
    /// Free-slot bookkeeping mismatch (should be unreachable).
    NoFreeSlot,
    /// `steal_context` target not idle.
    TargetNotIdle,
    /// No such slot.
    NoSuchSlot,
    /// Wrong suspend state.
    NotSuspended,
}

impl minix_types::ToErrno for WorkerError {
    fn to_errno(&self) -> minix_types::Errno {
        minix_types::Errno::from_i32((*self).to_errno())
    }
}

impl WorkerError {
    /// Minix errno mapping — all map to `EINVAL` family except `EDEADLK`
    /// which is preserved for the `vmnt` self-lock path; worker errors are
    /// internal and surfaced as `EINVAL` to the caller (see `05-stage-vfs`
    /// plan §5.4 exclusion of `LOCK_DEBUG`).
    pub fn to_errno(self) -> i32 {
        match self {
            Self::BothPendingAndActive => minix_types::EINVAL,
            Self::AlreadyHasNormal => minix_types::EBUSY,
            Self::AlreadyHasPm => minix_types::EBUSY,
            Self::NoFreeSlot => minix_types::ENOSPC,
            Self::TargetNotIdle => minix_types::EBUSY,
            Self::NoSuchSlot => minix_types::EINVAL,
            Self::NotSuspended => minix_types::EINVAL,
        }
    }
}

// ---------------------------------------------------------------------------
// Scheduling policy trait — Gate D "trait has ≥2 impls" + future policy swap.
// ---------------------------------------------------------------------------

/// Pluggable idle-slot selection — separates "which slot to use" from "whether
/// to use one at all" (`may_do_pending` / `block_all` below).
///
/// The default in Minix3 is first-fit linear scan (`worker_assign:128`
/// `for (i=0..NR) if (w_fp==NULL) break`).  Redox and Linux both allow
/// alternative affinities; the trait keeps that door open without changing
/// `WorkerPool` invariants.
pub trait SlotSelector {
    /// Return the index of an `Idle` slot to bind, or `None` if the policy
    /// declines to bind even though one is free.
    fn select_idle(&self, pool: &WorkerPool) -> Option<usize>;
}

/// C-faithful first-fit — `worker_assign:128` linear scan.
#[derive(Debug, Default, Clone, Copy)]
pub struct FirstFitSelector;

impl SlotSelector for FirstFitSelector {
    fn select_idle(&self, pool: &WorkerPool) -> Option<usize> {
        pool.slots.iter().position(|s| s.is_idle())
    }
}

/// Round-robin — starts scanning from `next` to spread `w_fp` churn.
///
/// Behaviourally different from `FirstFitSelector` (different slot chosen
/// when both are idle), so Gate D "≥2 behaviourally different impls" is met.
#[derive(Debug)]
pub struct RoundRobinSelector {
    next: Cell<usize>,
}

impl RoundRobinSelector {
    /// New selector starting at slot 0.
    pub fn new() -> Self {
        Self { next: Cell::new(0) }
    }
}

impl Default for RoundRobinSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl SlotSelector for RoundRobinSelector {
    fn select_idle(&self, pool: &WorkerPool) -> Option<usize> {
        let n = pool.slots.len();
        let start = self.next.get() % n;
        for off in 0..n {
            let i = (start + off) % n;
            if pool.slots[i].is_idle() {
                self.next.set((i + 1) % n);
                return Some(i);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Pool
// ---------------------------------------------------------------------------

/// Fixed pool of `NR_WTHREADS` request slots.
///
/// Owns the `pending` / `busy` / `allow` counters (`worker.c:10-12` `static
/// pending/busy/block_all`) and the 9 `WorkerSlot`s (`glo.h:37`
/// `workers[NR_WTHREADS]`).  The pool is `!Send`/`!Sync` — single-threaded
/// event loop (`ARCH A-1`), so `Arc`/`Mutex` are not used.
///
/// Invariants (checked in `debug_assert!`):
/// * `busy == slots.iter().filter(|s| !is_idle).count()`
/// * `pending == fproc.iter().filter(|fp| flags.contains(PENDING)).count()`
///   — the latter is maintained by `start`/`drain_pending` in concert with
///   `FProcTable`; `pending` here is the pool's view of that count.
/// * `available() == NR_WTHREADS - busy`
#[derive(Debug)]
pub struct WorkerPool {
    slots: Vec<WorkerSlot>,
    /// `w_fp != NULL` count (`worker.c:11` `busy`).
    busy: usize,
    /// `FP_PENDING` count (`worker.c:10` `pending`).
    pending: usize,
    /// `!block_all` — `true` means "allow immediate assignment" (`worker.c:12`).
    allow: bool,
    /// Pending jobs queued while `may_do_pending` was false or spare was needed.
    /// Models the `fproc[NR_PROCS]` scan for `FP_PENDING` in `worker_allow:176`.
    pending_q: Vec<(UserSlot, WorkerFunc, Message)>,
}

impl WorkerPool {
    /// `worker_init:27` — `pending=0, busy=0, allow=true, slots=[Idle;9]`.
    pub fn new() -> Self {
        let slots = (0..NR_WTHREADS).map(WorkerSlot::new).collect();
        Self {
            slots,
            busy: 0,
            pending: 0,
            allow: true,
            pending_q: Vec::new(),
        }
    }

    /// `worker_idle:113` — `pending==0 && busy==0`.
    pub fn is_idle(&self) -> bool {
        self.pending == 0 && self.busy == 0
    }

    /// `worker_available:233` — `NR_WTHREADS - busy`.
    pub fn available(&self) -> usize {
        NR_WTHREADS - self.busy
    }

    /// Alias kept for `main_loop.rs` compatibility.
    pub fn available_count(&self) -> usize {
        self.available()
    }

    /// Whether at least one slot is `Idle`.
    pub fn has_available(&self) -> bool {
        self.available() > 0
    }

    /// Whether all slots are `Idle` — `main_loop` fast-path.
    pub fn all_idle(&self) -> bool {
        self.slots.iter().all(|s| s.is_idle())
    }

    /// `worker_may_do_pending:156` — `pending>0 && available>1 && allow`.
    ///
    /// The `>1` leaves one spare thread for `use_spare` callbacks
    /// (`worker.c:150` comment).  `allow == !block_all`.
    pub fn may_do_pending(&self) -> bool {
        self.pending > 0 && self.available() > 1 && self.allow
    }

    /// `worker_allow:162` — gate switch.
    ///
    /// Closing (`allow=false`) only flips the flag (no active workers are
    /// stopped, `worker.c:165` comment).  Opening (`allow=true`) is expected
    /// to be followed by `drain_pending` to bind queued jobs; this method
    /// itself does not touch `FProc` so it remains testable without a table.
    pub fn set_allow(&mut self, allow: bool) {
        self.allow = allow;
    }

    /// Drain at most `available()-1` pending jobs using `selector`.
    ///
    /// Models `worker_allow:176-185` `for (rfp: PENDING) assign; if (!may) return`.
    /// Returns the number of jobs actually bound.
    pub fn drain_pending<S: SlotSelector>(&mut self, selector: &S) -> usize {
        let mut bound = 0;
        while self.may_do_pending() {
            let Some(job_idx) = self.pending_q.iter().position(|_| true) else {
                break;
            };
            let Some(slot_idx) = selector.select_idle(self) else {
                break;
            };
            let (fslot, func, msg) = self.pending_q.remove(job_idx);
            self.slots[slot_idx].bind(fslot, func, &msg);
            self.busy += 1;
            debug_assert!(self.pending > 0);
            self.pending -= 1;
            bound += 1;
        }
        bound
    }

    /// Whether the pool has any `Idle` slot.
    pub fn has_idle(&self) -> bool {
        self.slots.iter().any(|s| s.is_idle())
    }

    /// `worker_assign:119` — bind `slot` to an idle worker via `selector`.
    ///
    /// O(9) scan through `selector`; increments `busy`; wakes the logical slot.
    /// Returns the slot index on success.
    pub fn assign<S: SlotSelector>(
        &mut self,
        fslot: UserSlot,
        func: WorkerFunc,
        msg: &Message,
        selector: &S,
    ) -> Option<usize> {
        let idx = selector.select_idle(self)?;
        self.slots[idx].bind(fslot, func, msg);
        self.busy += 1;
        Some(idx)
    }

    /// Fast assign with the default `FirstFitSelector` — C-faithful.
    pub fn assign_first_fit(
        &mut self,
        fslot: UserSlot,
        func: WorkerFunc,
        msg: &Message,
    ) -> Option<usize> {
        self.assign(fslot, func, msg, &FirstFitSelector)
    }

    /// Release a slot (`worker_main:284` `busy--`).
    pub fn release(&mut self, idx: usize) {
        assert!(idx < self.slots.len());
        assert!(!self.slots[idx].is_idle());
        self.slots[idx].release();
        assert!(self.busy > 0);
        self.busy -= 1;
    }

    /// Whether `fslot` is currently bound to any `Busy`/`WaitingForFs`/`Suspended` slot.
    pub fn is_active_for(&self, fslot: UserSlot) -> bool {
        self.slots
            .iter()
            .any(|s| s.fp_slot == Some(fslot) && !s.is_idle())
    }

    /// Whether `fslot` has a pending or active normal (`!is_pm_work`) job.
    fn has_normal_for(&self, fslot: UserSlot) -> bool {
        self.pending_q
            .iter()
            .any(|(s, f, _)| *s == fslot && !f.is_pm_work())
            || self
                .slots
                .iter()
                .any(|s| s.fp_slot == Some(fslot) && s.func.is_some_and(|f| !f.is_pm_work()))
    }

    /// Whether `fslot` has a PM job (`FP_PM_WORK` or pending PM func).
    fn has_pm_for(&self, fslot: UserSlot, fproc: &FProc) -> bool {
        fproc.flags.contains(FpFlags::PM_WORK)
            || self
                .pending_q
                .iter()
                .any(|(s, f, _)| *s == fslot && f.is_pm_work())
            || self
                .slots
                .iter()
                .any(|s| s.fp_slot == Some(fslot) && s.func.is_some_and(|f| f.is_pm_work()))
    }

    /// `worker_can_start:295` — whether normal work may be added for `fproc`.
    ///
    /// `!pending && !active → true` (no work at all);
    /// `has_normal → false` (one normal job per fproc);
    /// `is_pending → true` (PM pending but no normal — can add normal);
    /// `active (PM) → false` (worker already running PM).
    pub fn can_start(&self, fslot: UserSlot, fproc: &FProc) -> bool {
        let is_pending = fproc.flags.contains(FpFlags::PENDING);
        let is_active = self.is_active_for(fslot);
        let has_normal = self.has_normal_for(fslot);

        if !is_pending && !is_active {
            return true;
        }
        if has_normal {
            return false;
        }
        if is_pending {
            return true;
        }
        false
    }

    /// `worker_try_activate:331` — spare-aware activation or `PENDING` queue.
    pub fn try_activate<S: SlotSelector>(
        &mut self,
        fslot: UserSlot,
        func: WorkerFunc,
        msg: &Message,
        use_spare: bool,
        fproc: &mut FProc,
        selector: &S,
    ) -> ActivateOutcome {
        let needed: usize = if use_spare { 1 } else { 2 };
        if needed <= self.available() && (self.allow || use_spare) {
            let idx = self
                .assign(fslot, func, msg, selector)
                .expect("available assured");
            // Clear a stale PENDING if we had queued earlier (defensive).
            if fproc.flags.contains(FpFlags::PENDING) {
                fproc.flags.remove(FpFlags::PENDING);
                assert!(self.pending > 0);
                self.pending -= 1;
                // Also drop from pending_q if it was queued.
                if let Some(pos) = self.pending_q.iter().position(|(s, _, _)| *s == fslot) {
                    self.pending_q.remove(pos);
                }
            }
            ActivateOutcome::Assigned(idx)
        } else {
            // Queue — `rfp->fp_flags |= FP_PENDING; pending++` (`worker.c:352`).
            if !fproc.flags.contains(FpFlags::PENDING) {
                fproc.flags.insert(FpFlags::PENDING);
                self.pending += 1;
            }
            // Keep the job so drain can re-bind with its func.
            if !self.pending_q.iter().any(|(s, _, _)| *s == fslot) {
                self.pending_q.push((fslot, func, *msg));
            }
            ActivateOutcome::Queued
        }
    }

    /// `worker_start:360` — four-guard validation + dual storage + activation.
    ///
    /// Mirrors the C `panic("…")` branches as `Err` variants; the caller
    /// decides whether that is fatal (e.g. `main.c:901` `pm_reboot` would
    /// `panic` while `handle_work:165` would `EAGAIN`).
    pub fn start<S: SlotSelector>(
        &mut self,
        fslot: UserSlot,
        fproc: &mut FProc,
        func: WorkerFunc,
        msg: &Message,
        use_spare: bool,
        selector: &S,
    ) -> Result<ActivateOutcome, WorkerError> {
        let is_pm_work = func.is_pm_work();
        let is_pending = fproc.flags.contains(FpFlags::PENDING);
        let is_active = self.is_active_for(fslot);
        let has_normal = self.has_normal_for(fslot);
        let has_pm = self.has_pm_for(fslot, fproc);

        if is_pending || is_active {
            if is_pending && is_active {
                return Err(WorkerError::BothPendingAndActive);
            }
            if !is_pm_work && has_normal {
                return Err(WorkerError::AlreadyHasNormal);
            }
            if is_pm_work && has_pm {
                return Err(WorkerError::AlreadyHasPm);
            }
        } else if has_normal || has_pm {
            return Err(WorkerError::BothPendingAndActive);
        }

        // Persist PM flag (`worker.c:417` `flags |= FP_PM_WORK`) for the
        // postponed track; normal track's `fp_func` is modelled by the pending
        // queue entry / slot `func`.
        if is_pm_work {
            fproc.flags.insert(FpFlags::PM_WORK);
        }

        // Only schedule a new binding if we are not already pending/active on
        // an existing binding (`worker.c:424` `if (!pending && !active) try_activate`).
        // If we are already pending/active we have just appended work to the
        // existing job (the C fall-through at `worker.c:422` comment).
        if !is_pending && !is_active {
            Ok(self.try_activate(fslot, func, msg, use_spare, fproc, selector))
        } else {
            // Already have a binding — the additional PM/normal work is now
            // coalesced (the `worker.c:422` "already PM pending" case).
            // For the state machine we keep it as Queued; the active slot
            // will consume it in `worker_main:270` order.
            if is_pm_work && !fproc.flags.contains(FpFlags::PM_WORK) {
                fproc.flags.insert(FpFlags::PM_WORK);
            }
            Ok(ActivateOutcome::Queued)
        }
    }

    /// Convenience: `start` with `FirstFitSelector` (C-faithful).
    pub fn start_first_fit(
        &mut self,
        fslot: UserSlot,
        fproc: &mut FProc,
        func: WorkerFunc,
        msg: &Message,
        use_spare: bool,
    ) -> Result<ActivateOutcome, WorkerError> {
        self.start(fslot, fproc, func, msg, use_spare, &FirstFitSelector)
    }

    // -----------------------------------------------------------------------
    // Suspend / resume / wait / signal — coroutine + condvar decomposition.
    // -----------------------------------------------------------------------

    /// `worker_yield:431` — single-threaded `noop` (`mthread_yield_all` has no
    /// meaning without threads; `self` TLS hand-off is modelled by the event
    /// loop's `current_slot` in `main_loop.rs`).
    pub fn yield_now(&mut self) {}

    /// `worker_suspend:474` — save `err` and return an owning token.
    pub fn suspend(&mut self, slot_idx: usize, err: i32) -> Result<SuspendToken, WorkerError> {
        let slot = self
            .slots
            .get_mut(slot_idx)
            .ok_or(WorkerError::NoSuchSlot)?;
        if slot.state != WorkerState::Busy {
            return Err(WorkerError::NotSuspended);
        }
        Ok(slot.suspend(err))
    }

    /// `worker_resume:493` — consume the token and restore `err_code`.
    pub fn resume(&mut self, tok: SuspendToken) -> Result<(), WorkerError> {
        let slot = self
            .slots
            .get_mut(tok.slot_index)
            .ok_or(WorkerError::NoSuchSlot)?;
        if slot.state != WorkerState::Suspended {
            return Err(WorkerError::NotSuspended);
        }
        slot.resume(tok);
        Ok(())
    }

    /// `worker_wait:510` — `suspend` + logical sleep (no `cond_wait`).
    ///
    /// In the state machine the "sleep" is the `Suspended` state itself; the
    /// driver/FS reply path calls `signal` to wake it.  The `suspend` token is
    /// returned so the caller can later `resume` or `signal`.
    pub fn wait(&mut self, slot_idx: usize, err: i32) -> Result<SuspendToken, WorkerError> {
        self.suspend(slot_idx, err)
    }

    /// `worker_signal:526` — wake a `Suspended` / `WaitingForFs` slot.
    pub fn signal(&mut self, slot_idx: usize) -> Result<(), WorkerError> {
        let slot = self
            .slots
            .get_mut(slot_idx)
            .ok_or(WorkerError::NoSuchSlot)?;
        match slot.state {
            WorkerState::Suspended => {
                slot.state = WorkerState::Busy;
                slot.saved_err = None;
                Ok(())
            }
            WorkerState::WaitingForFs => {
                // `do_reply:210` `worker_signal` — waiting for FS, now reply arrived.
                slot.state = WorkerState::Busy;
                slot.task = None;
                slot.sendrec = None;
                Ok(())
            }
            _ => Err(WorkerError::NotSuspended),
        }
    }

    /// `worker_wait` + `signal` in one step — test helper for §5.
    pub fn wait_then_signal(&mut self, slot_idx: usize, err: i32) -> Result<(), WorkerError> {
        let tok = self.wait(slot_idx, err)?;
        // Immediate wake models a zero-delay `tll` unlock.
        let slot = &mut self.slots[tok.slot_index];
        // `wait` moved it to Suspended; signal back to Busy via token.
        slot.state = WorkerState::Busy;
        slot.saved_err = None;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Stop / cleanup
    // -----------------------------------------------------------------------

    /// `worker_stop:535` — inject `EIO` into the waiting sendrec.
    pub fn stop(&mut self, slot_idx: usize) {
        let slot = &mut self.slots[slot_idx];
        if slot.sendrec.is_some() {
            // `w_sendrec->m_type = EIO` or `w_drv_sendrec->m_type = EIO`.
            slot.sendrec = None;
            slot.task = None;
            // Wake so the slot can observe the `EIO`.
            if slot.state == WorkerState::WaitingForFs || slot.state == WorkerState::Suspended {
                slot.state = WorkerState::Busy;
            }
        } else if !slot.is_idle() {
            // `panic("reply storage consistency error")` in C — in the state
            // machine there may be no sendrec yet (still queued), so we just
            // mark it for wake.
            if slot.state == WorkerState::Suspended {
                slot.state = WorkerState::Busy;
            }
        }
    }

    /// `worker_stop_by_endpt:555` — `for (w: w_fp && w_task==ep) stop`.
    pub fn stop_by_endpoint(&mut self, ep: Endpoint) -> usize {
        if ep == Endpoint::NONE {
            return 0;
        }
        let mut stopped = 0;
        for idx in 0..self.slots.len() {
            let should = self.slots[idx].task == Some(ep) && self.slots[idx].fp_slot.is_some();
            if should {
                self.stop(idx);
                stopped += 1;
            }
        }
        stopped
    }

    /// `worker_get:572` — by `UserSlot` instead of `thread_t` (`ARCH A-8`).
    pub fn find_by_slot(&self, fslot: UserSlot) -> Option<&WorkerSlot> {
        self.slots.iter().find(|s| s.fp_slot == Some(fslot))
    }

    /// Mutable variant.
    pub fn find_by_slot_mut(&mut self, fslot: UserSlot) -> Option<&mut WorkerSlot> {
        self.slots.iter_mut().find(|s| s.fp_slot == Some(fslot))
    }

    /// `worker_set_proc:586` — *incredibly ugly* `reboot` context steal.
    ///
    /// Moves the binding from `from` to `to`, asserts `to` is idle, and
    /// preserves the `WorkerState`.  Only `pm_reboot` uses this.
    pub fn steal_context(&mut self, from: UserSlot, to: UserSlot) -> Result<(), WorkerError> {
        if from == to {
            return Ok(());
        }
        let from_idx = self
            .slots
            .iter()
            .position(|s| s.fp_slot == Some(from))
            .ok_or(WorkerError::NoSuchSlot)?;
        if self.is_active_for(to) {
            return Err(WorkerError::TargetNotIdle);
        }
        let func = self.slots[from_idx].func;
        let state = self.slots[from_idx].state;
        let task = self.slots[from_idx].task;
        let sendrec = self.slots[from_idx].sendrec;
        let input = self.slots[from_idx].input;
        self.slots[from_idx].release();
        // busy stays the same — one release, one re-bind.
        let new_idx = self
            .slots
            .iter()
            .position(|s| s.is_idle())
            .ok_or(WorkerError::NoFreeSlot)?;
        self.slots[new_idx].fp_slot = Some(to);
        self.slots[new_idx].func = func;
        self.slots[new_idx].state = state;
        self.slots[new_idx].task = task;
        self.slots[new_idx].sendrec = sendrec;
        self.slots[new_idx].input = input;
        // busy unchanged (release + bind cancel)
        Ok(())
    }

    /// Total slots (`NR_WTHREADS`).
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Pool empty — always `false` for the fixed pool, but kept for
    /// `main_loop` compatibility.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Get slot by index.
    pub fn get(&self, index: usize) -> Option<&WorkerSlot> {
        self.slots.get(index)
    }

    /// Get slot by index (mutable).
    pub fn get_mut(&mut self, index: usize) -> Option<&mut WorkerSlot> {
        self.slots.get_mut(index)
    }

    /// First `Idle` slot (immutable) — `main_loop` compatibility.
    pub fn get_idle(&self) -> Option<&WorkerSlot> {
        self.slots.iter().find(|s| s.is_idle())
    }

    /// First `Idle` slot (mutable).
    pub fn get_idle_mut(&mut self) -> Option<&mut WorkerSlot> {
        self.slots.iter_mut().find(|s| s.is_idle())
    }

    /// `worker_cleanup:63` — requires `is_idle`.
    pub fn cleanup(&mut self) -> Result<(), WorkerError> {
        if !self.is_idle() {
            return Err(WorkerError::BothPendingAndActive);
        }
        self.pending_q.clear();
        // Drop any stray bindings (defensive; C `memset(workers,0)`).
        for s in &mut self.slots {
            s.release();
        }
        self.busy = 0;
        self.pending = 0;
        self.allow = true;
        Ok(())
    }

    // Introspection for §4 invariants.
    /// `busy` counter value.
    pub fn busy_count(&self) -> usize {
        self.busy
    }
    /// `pending` counter value.
    pub fn pending_count(&self) -> usize {
        self.pending
    }
    /// `allow` flag (`!block_all`).
    pub fn is_allowing(&self) -> bool {
        self.allow
    }
    /// Pending queue length.
    pub fn pending_queue_len(&self) -> usize {
        self.pending_q.len()
    }
}

impl Default for WorkerPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fproc::FProc;
    use minix_types::{Endpoint, UserSlot};

    fn slot(n: usize) -> UserSlot {
        UserSlot::new(n)
    }

    #[test]
    fn test_worker_pool_new_is_idle() {
        let pool = WorkerPool::new();
        assert_eq!(pool.len(), NR_WTHREADS);
        assert!(pool.is_idle());
        assert!(pool.all_idle());
        assert_eq!(pool.available_count(), NR_WTHREADS);
        assert_eq!(pool.busy_count(), 0);
        assert_eq!(pool.pending_count(), 0);
        assert!(pool.is_allowing());
        assert!(pool.get_idle().is_some());
    }

    #[test]
    fn test_worker_available() {
        let mut pool = WorkerPool::new();
        let mut fp = FProc::new_unused();
        let msg = Message::default();
        // Bind one slot
        let out = pool
            .start_first_fit(slot(5), &mut fp, WorkerFunc::DoWork, &msg, false)
            .unwrap();
        assert!(matches!(out, ActivateOutcome::Assigned(_)));
        assert_eq!(pool.available(), NR_WTHREADS - 1);
        assert_eq!(pool.available_count(), NR_WTHREADS - 1);
        assert!(pool.has_available());
        // Fill rest
        for i in 0..NR_WTHREADS - 1 {
            let mut fp2 = FProc::new_unused();
            let s = slot(i + 10);
            // Use spare to force assignment even when available==1
            let _ = pool.start_first_fit(s, &mut fp2, WorkerFunc::DoWork, &msg, true);
        }
        assert_eq!(pool.available(), 0);
        assert!(!pool.has_available());
        assert!(pool.get_idle().is_none());
        assert!(pool.get_idle_mut().is_none());
    }

    #[test]
    fn test_worker_may_do_pending_spare() {
        let mut pool = WorkerPool::new();
        let msg = Message::default();
        // No pending → false
        assert!(!pool.may_do_pending());

        // Create pending by blocking allow
        pool.set_allow(false);
        let mut fp = FProc::new_unused();
        let out = pool
            .start_first_fit(slot(3), &mut fp, WorkerFunc::DoWork, &msg, false)
            .unwrap();
        assert_eq!(out, ActivateOutcome::Queued);
        assert_eq!(pool.pending_count(), 1);
        assert!(fp.flags.contains(FpFlags::PENDING));
        // Even with pending, !allow → false
        assert!(!pool.may_do_pending());

        // Allow but available() >1 needed: busy=0, pending=1, allow=true → avail=9>1 → true
        pool.set_allow(true);
        assert!(pool.may_do_pending());

        // Now occupy 8 slots (busy=8, avail=1) → may_do_pending false (spare reserved)
        for i in 0..8 {
            let mut f = FProc::new_unused();
            let _ = pool.start_first_fit(slot(i + 20), &mut f, WorkerFunc::DoWork, &msg, true);
        }
        // busy now 8, pending still 1, avail=1 → need >1 so false
        assert_eq!(pool.available(), 1);
        assert!(!pool.may_do_pending());

        // Free one → avail=2 → true again
        pool.release(0);
        assert!(pool.may_do_pending());
    }

    #[test]
    fn test_worker_allow_drains() {
        let mut pool = WorkerPool::new();
        let msg = Message::default();
        pool.set_allow(false);
        let mut fps: Vec<FProc> = (0..3).map(|_| FProc::new_unused()).collect();
        let slots = [slot(1), slot(2), slot(3)];
        for (fp, s) in fps.iter_mut().zip(slots.iter()) {
            let out = pool
                .start_first_fit(*s, fp, WorkerFunc::DoWork, &msg, false)
                .unwrap();
            assert_eq!(out, ActivateOutcome::Queued);
        }
        assert_eq!(pool.pending_count(), 3);
        assert_eq!(pool.pending_queue_len(), 3);
        assert_eq!(pool.busy_count(), 0);

        // Open gate and drain with FirstFit
        pool.set_allow(true);
        let drained = pool.drain_pending(&FirstFitSelector);
        assert_eq!(drained, 3);
        assert_eq!(pool.pending_count(), 0);
        assert_eq!(pool.busy_count(), 3);
        assert!(!pool.may_do_pending()); // no pending left
    }

    #[test]
    fn test_worker_assign_busy() {
        let mut pool = WorkerPool::new();
        let m = Message::default();
        let idx = pool
            .assign_first_fit(slot(7), WorkerFunc::DsEvent, &m)
            .unwrap();
        assert_eq!(pool.busy_count(), 1);
        assert!(!pool.slots[idx].is_idle());
        assert_eq!(pool.slots[idx].fp_slot, Some(slot(7)));
        assert_eq!(pool.slots[idx].func, Some(WorkerFunc::DsEvent));
        assert_eq!(pool.slots[idx].state, WorkerState::Busy);

        pool.release(idx);
        assert!(pool.slots[idx].is_idle());
        assert_eq!(pool.busy_count(), 0);
        assert!(pool.slots[idx].fp_slot.is_none());
    }

    #[test]
    fn test_worker_can_start_pending() {
        let mut pool = WorkerPool::new();
        let msg = Message::default();
        let s = slot(4);
        let mut fp = FProc::new_unused();

        // No work → can start
        assert!(pool.can_start(s, &fp));

        // Start normal work — now has_normal → cannot start second normal
        let _ = pool
            .start_first_fit(s, &mut fp, WorkerFunc::DoWork, &msg, false)
            .unwrap();
        // fp now active; can_start should reflect has_normal
        assert!(!pool.can_start(s, &fp));

        // Fresh slot with pending flag set but no normal — can add normal
        let mut fp2 = FProc::new_unused();
        fp2.flags.insert(FpFlags::PENDING);
        // Simulate pending without slot binding (set pool pending counter)
        pool.pending = 1;
        pool.pending_q.push((slot(9), WorkerFunc::PmPostponed, msg));
        // has_normal false, is_pending true → true
        assert!(pool.can_start(slot(9), &fp2));
        // But if we also mark has_normal, then false
        pool.pending_q.clear();
        pool.pending_q.push((slot(9), WorkerFunc::DoWork, msg));
        assert!(!pool.can_start(slot(9), &fp2));
    }

    #[test]
    fn test_worker_start_pm_vs_normal() {
        let mut pool = WorkerPool::new();
        let msg = Message::default();
        let s = slot(6);
        let mut fp = FProc::new_unused();

        // First normal start succeeds
        let out = pool
            .start_first_fit(s, &mut fp, WorkerFunc::DoWork, &msg, false)
            .unwrap();
        assert!(matches!(out, ActivateOutcome::Assigned(_)));
        assert!(!fp.flags.contains(FpFlags::PM_WORK));

        // Second normal for same slot → AlreadyHasNormal (C panic "process has two calls")
        let mut fp2 = fp.clone();
        // fp2 still active in pool, but clone lost flag; re-insert active tracking via pool
        // pool already has active for slot 6, so start should fail
        let err = pool
            .start_first_fit(s, &mut fp2, WorkerFunc::DoWork, &msg, false)
            .unwrap_err();
        assert_eq!(err, WorkerError::AlreadyHasNormal);

        // PM work for fresh slot succeeds and sets PM_WORK
        let mut fp3 = FProc::new_unused();
        let out = pool
            .start_first_fit(slot(8), &mut fp3, WorkerFunc::PmPostponed, &msg, false)
            .unwrap();
        assert!(matches!(out, ActivateOutcome::Assigned(_)));
        assert!(fp3.flags.contains(FpFlags::PM_WORK));

        // Second PM for same slot → AlreadyHasPm
        let err = pool
            .start_first_fit(slot(8), &mut fp3, WorkerFunc::PmPostponed, &msg, false)
            .unwrap_err();
        assert_eq!(err, WorkerError::AlreadyHasPm);
    }

    #[test]
    fn test_worker_start_both_pending_and_active() {
        let mut pool = WorkerPool::new();
        let msg = Message::default();
        let s = slot(11);
        let mut fp = FProc::new_unused();
        // Make fproc look both pending and active (corrupted state)
        fp.flags.insert(FpFlags::PENDING);
        // Make pool think it's active
        let _ = pool.assign_first_fit(s, WorkerFunc::DoWork, &msg).unwrap();
        // Now start should detect BothPendingAndActive
        let err = pool
            .start_first_fit(s, &mut fp, WorkerFunc::DoWork, &msg, false)
            .unwrap_err();
        assert_eq!(err, WorkerError::BothPendingAndActive);
    }

    #[test]
    fn test_worker_suspend_resume_token() {
        let mut pool = WorkerPool::new();
        let m = Message::default();
        let idx = pool
            .assign_first_fit(slot(2), WorkerFunc::DoWork, &m)
            .unwrap();
        let tok = pool.suspend(idx, 42).unwrap();
        assert_eq!(pool.slots[idx].state, WorkerState::Suspended);
        assert_eq!(pool.slots[idx].saved_err, Some(42));
        pool.resume(tok).unwrap();
        assert_eq!(pool.slots[idx].state, WorkerState::Busy);
        assert_eq!(pool.slots[idx].saved_err, None);
    }

    #[test]
    fn test_worker_wait_is_suspend_plus_sleep() {
        let mut pool = WorkerPool::new();
        let m = Message::default();
        let idx = pool
            .assign_first_fit(slot(3), WorkerFunc::DoWork, &m)
            .unwrap();
        // wait is suspend
        let tok = pool.wait(idx, -11).unwrap();
        assert_eq!(pool.slots[idx].state, WorkerState::Suspended);
        // signal back to Busy (no token consumption for simple signal)
        pool.signal(idx).unwrap();
        assert_eq!(pool.slots[idx].state, WorkerState::Busy);
        // Also test wait_then_signal helper
        let idx2 = pool
            .assign_first_fit(slot(4), WorkerFunc::DoWork, &m)
            .unwrap();
        pool.wait_then_signal(idx2, 99).unwrap();
        assert_eq!(pool.slots[idx2].state, WorkerState::Busy);
        let _ = tok; // original token is now stale; signal path cleared it
    }

    #[test]
    fn test_worker_stop_injects_eio() {
        let mut pool = WorkerPool::new();
        let m = Message::default();
        let idx = pool
            .assign_first_fit(slot(5), WorkerFunc::DoWork, &m)
            .unwrap();
        // Simulate WaitingForFs with sendrec
        pool.slots[idx].set_waiting(Endpoint::MFS, m);
        assert_eq!(pool.slots[idx].state, WorkerState::WaitingForFs);
        assert!(pool.slots[idx].sendrec.is_some());
        pool.stop(idx);
        assert!(pool.slots[idx].sendrec.is_none());
        assert!(pool.slots[idx].task.is_none());
        assert_eq!(pool.slots[idx].state, WorkerState::Busy);
    }

    #[test]
    fn test_worker_stop_by_endpoint() {
        let mut pool = WorkerPool::new();
        let m = Message::default();
        let idx0 = pool
            .assign_first_fit(slot(10), WorkerFunc::DoWork, &m)
            .unwrap();
        let idx1 = pool
            .assign_first_fit(slot(11), WorkerFunc::DoWork, &m)
            .unwrap();
        pool.slots[idx0].set_waiting(Endpoint::MFS, m);
        pool.slots[idx1].set_waiting(Endpoint::VM, m);
        let n = pool.stop_by_endpoint(Endpoint::MFS);
        assert_eq!(n, 1);
        assert!(pool.slots[idx0].sendrec.is_none());
        assert!(pool.slots[idx1].sendrec.is_some());
        // NONE is no-op
        assert_eq!(pool.stop_by_endpoint(Endpoint::NONE), 0);
    }

    #[test]
    fn test_worker_steal_context() {
        let mut pool = WorkerPool::new();
        let m = Message::default();
        let from = slot(20);
        let to = slot(21);
        let idx = pool.assign_first_fit(from, WorkerFunc::DoWork, &m).unwrap();
        pool.slots[idx].set_waiting(Endpoint::MFS, m);
        pool.steal_context(from, to).unwrap();
        assert!(pool.find_by_slot(from).is_none());
        assert!(pool.find_by_slot(to).is_some());
        assert_eq!(pool.find_by_slot(to).unwrap().task, Some(Endpoint::MFS));
        // Target already active → error
        let _ = pool.assign_first_fit(from, WorkerFunc::DoWork, &m).unwrap();
        let err = pool.steal_context(from, to).unwrap_err();
        assert_eq!(err, WorkerError::TargetNotIdle);
        // Self-steal is no-op
        assert!(pool.steal_context(to, to).is_ok());
    }

    #[test]
    fn test_slot_selector_two_impls() {
        let mut pool = WorkerPool::new();
        let m = Message::default();
        let ff = FirstFitSelector;
        let rr = RoundRobinSelector::new();
        // Occupy 0 via FirstFit (worker index 0), RR still at 0.
        pool.assign(slot(0), WorkerFunc::DoWork, &m, &ff).unwrap();
        // RR's first real pick (no prior probe) scans from 0 → busy → 1.
        pool.assign(slot(1), WorkerFunc::DoWork, &m, &rr).unwrap(); // RR picks 1, next=2
        pool.assign(slot(2), WorkerFunc::DoWork, &m, &rr).unwrap(); // RR picks 2, next=3
        // Now 0,1,2 busy. Free 0.
        pool.release(0);
        // FF always picks lowest free → 0
        assert_eq!(ff.select_idle(&pool), Some(0));
        // RR's next was 3, so it picks 3 (not 0) — demonstrates behavioural difference
        assert_eq!(rr.select_idle(&pool), Some(3));
        // Additional check: fresh FF still picks 0, fresh RR(0) would pick 0, but our
        // stateful RR picks 3 — proving two impls behave differently on same pool state.
        let rr_fresh = RoundRobinSelector::new();
        assert_eq!(rr_fresh.select_idle(&pool), Some(0));
    }

    #[test]
    fn test_worker_cleanup_requires_idle() {
        let mut pool = WorkerPool::new();
        let m = Message::default();
        let _ = pool.assign_first_fit(slot(0), WorkerFunc::DoWork, &m);
        assert!(pool.cleanup().is_err());
        pool.release(0);
        // Still pending queued?
        pool.set_allow(false);
        let mut fp = FProc::new_unused();
        let _ = pool.start_first_fit(slot(1), &mut fp, WorkerFunc::DoWork, &m, false);
        // pending==1 → not idle → cleanup fails
        assert!(pool.cleanup().is_err());
        // Drain
        pool.set_allow(true);
        pool.drain_pending(&FirstFitSelector);
        // Now pending cleared but we still have busy slots → not idle
        for i in 0..pool.slots.len() {
            if !pool.slots[i].is_idle() {
                pool.release(i);
            }
        }
        pool.pending = 0;
        pool.pending_q.clear();
        // Now idle → cleanup ok
        assert!(pool.cleanup().is_ok());
        assert!(pool.is_idle());
    }

    #[test]
    fn test_worker_yield_is_noop() {
        let mut pool = WorkerPool::new();
        pool.yield_now(); // must not panic, state unchanged
        assert!(pool.is_idle());
    }

    #[test]
    fn test_worker_trait_has_two_impls() {
        // Gate D: trait SlotSelector has ≥2 behaviourally different impls
        let pool = WorkerPool::new();
        let ff: &dyn SlotSelector = &FirstFitSelector;
        let rr: &dyn SlotSelector = &RoundRobinSelector::new();
        // Both pick slot 0 on empty pool, but diverge after partial fill — see test above
        assert_eq!(ff.select_idle(&pool), Some(0));
        assert_eq!(rr.select_idle(&pool), Some(0));
    }

    #[test]
    fn test_worker_error_to_errno() {
        assert_eq!(WorkerError::AlreadyHasNormal.to_errno(), minix_types::EBUSY);
        assert_eq!(
            WorkerError::BothPendingAndActive.to_errno(),
            minix_types::EINVAL
        );
        assert_eq!(WorkerError::NoFreeSlot.to_errno(), minix_types::ENOSPC);
    }

    #[test]
    fn test_worker_slot_new_and_release() {
        let mut s = WorkerSlot::new(7);
        assert!(s.is_idle());
        assert_eq!(s.self_index, 7);
        s.bind(slot(3), WorkerFunc::DsEvent, &Message::default());
        assert!(!s.is_idle());
        s.release();
        assert!(s.is_idle());
        assert!(s.fp_slot.is_none());
        assert!(s.func.is_none());
    }
}
