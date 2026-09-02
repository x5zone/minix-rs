# 02-fproc-struct: fproc 结构与标志

> **状态**: 生效中（2026-08-17 完整改写）
> **定位**: 阶段 1 — 进程模型：每进程文件系统上下文（锚点文档）
> **源码**: `fproc.h`（15-82 全字段 + 91-98 标志 + 111-115 fproc_light）+ `const.h:19-25`（阻塞常量）+ `misc.c:577-634`（pm_fork 生命周期）+ `main.c:405-408,468-483`（两遍初始化）
> **Rust 模块**: `os/servers/vfs/src/fproc.rs`（FProc/FpFlags/BlockedOn/FProcTable）+ `os/libs/minix-types/src/types/id.rs`（DevId/Mode/GrantId/NO_DEV 新增）
> **draft 素材**: `draft/01-fproc-struct.md` + `draft/02-fproc-flags.md` + `draft/03-fproc-cred.md` + `draft/fproc-design.md`（素材）

---

## 1. 概念：进程的"文件系统身份"

### 1.0 章节引言

**目标读者**：已理解 Minix3 微内核"每服务私有进程表"的分布模型（`../04-stage-pm/02-mproc-struct.md` 的 `mproc`、`../02-stage-vm/01-vm-init-main.md` 的 `vmproc`）与 VFS 启动握手（`01-vfs-init-main.md` §1.3）的开发者。

> **本章不讲什么**：
> - fproc 表的查找、endpoint 验证、槽位复用与 `fproc_light`（`03-fproc-table.md`）
> - filp/vnode/vmnt 表结构（`04~06`）
> - worker 池状态机与请求槽调度（`08-worker-thread.md`）
> - 主循环分发与 SUSPEND 回复意图枚举（`09-main-loop.md`）
> - PM 协议与 `pm_fork`/`pm_exit` 的完整流程（`10-pm-protocol.md`）
> - fd 表操作与 `close_fd`/`copyfd`（`14-filedes.md`）
> - 各阻塞类型的恢复路径：pipe/select/cdev/sdev（`17/23/21/22`）
>
> 本章只回答一个问题：**VFS 为每个进程记住了什么，以及这些状态为什么必须由 VFS 自己记住**。

### 1.1 四张进程表的第三张

Minix3 把"一个进程"拆成四个相互独立的投影，分别由四个服务维护，靠 endpoint 关联：

| 投影 | 维护者 | 关心什么 | 结构 |
|------|--------|---------|------|
| 调度/寄存器/IPC | 内核 | 进程能否运行、消息发给谁 | `struct proc` |
| 生命周期/信号/凭证 | PM | 进程生老病死、信号、父子的树 | `struct mproc` |
| **文件系统状态** | **VFS** | **能碰哪些文件、以什么身份碰、卡在哪个文件操作** | **`struct fproc`** |
| 地址空间 | VM | 虚拟内存布局 | `struct vmproc` |

用户进程做 `open/read/write/exec` 时，消息到达 VFS；VFS 必须**不依赖内核进程表**就能回答三个问题：

1. **这个进程能碰哪些文件？**——fd 表（`fp_filp[]`）、根目录/工作目录（`fp_rd`/`fp_wd`）、`FD_CLOEXEC` 位图（`fp_cloexec_set`）。
2. **以什么身份碰？**——real/effective UID/GID + 补充组 + umask（权限检查 `forbidden` 的输入）。
3. **上一个文件操作卡在哪？**——`fp_blocked_on` + `fp_u` 记录的"挂起点"，等条件满足后恢复执行。

这三个问题分别对应 `fproc` 的三组字段；第四组字段（锁、worker 关联、消息缓冲）服务于 VFS 的多请求并发，见 §2.5。

### 1.2 为什么必须私有维护：微内核的"最小知识"原则

如果 VFS 每次都要向内核/PM 查询"这个进程的 fd 表是什么"，每次文件操作都要跨服务往返，而且 fd 表属于**进程私有语义**（fork 复制、exec 清空、close 调整），把它们放在 VFS 本地让所有操作 O(1) 且不打断其他服务。这与 PM 维护 `mproc` 的动机完全相同：**每个服务只维护自己领域的状态，靠轻量级标识（endpoint）互相关联，而不是共享一张大表**（`../04-stage-pm/02-mproc-struct.md` §1.1）。

代价是**四张表必须对齐**：VFS_PM_INIT 握手（`01-vfs-init-main.md` §1.3）就是 PM 把 boot 进程清单"倾倒"给 VFS、建立 `fproc` 槽的过程；此后进程的生老病死由 `VFS_PM_*` 消息（fork/exec/exit/setsid/…，归 `10-pm-protocol.md`）增量同步。

### 1.3 字段分组的两个心智模型

**模型 A（Minix3 平铺）**：`struct fproc` 是一个平铺结构，所有字段在一个结构体里，fork 时整体复制（`fproc[childno] = fproc[parentno]`，misc.c:607）。

**模型 B（Linux/Redox 分治）**：Linux 把同样的状态拆成多个被引用计数的子结构——`fs_struct`（root/pwd）、`files_struct`（fdtable）、`cred`（凭证）、`tty_struct`（控制终端，挂在 `signal_struct` 上）——因为同一结构被多个子系统独立引用，拆分让"只借一部分"成为可能；Redox 的 fd 表（`FdTable`，`BTreeMap<Fd, Arc<dyn Resource>>`）同样独立于凭证，放在内核进程结构里，文件对象本身走 scheme 驱动的 `Resource` 抽象。

Minix3 选择平铺是**有意为之**：VFS 是单地址空间服务，`fproc` 只在本服务内访问，没有任何跨子系统共享的需求；fork 的"整块复制"语义让 `fp_filp`/凭证/目录锚点一次到位（misc.c:604-633）。本设计的取舍见 §3.1：**保留平铺与复制语义，但让每个字段的类型说话**——不翻译成 C 的裸整数，也不过度拆分成 Linux 式引用计数子结构。

### 1.4 阻塞状态：单请求线程的"挂起—恢复"上下文

VFS 是 Minix3 唯一的多线程服务器（ARCH A-1）：主线程收消息，worker 线程执行具体调用。当一个调用需要等待（管道无数据、文件锁被占、驱动未回复），worker 不能空转，于是进程被标记为**阻塞**：`fp_blocked_on` 记录阻塞原因，`fp_u` 记录恢复所需的全部参数（读哪个 fd、写哪个用户缓冲、等哪个驱动的哪个 grant）。恢复路径（`revive`/`select_return`/`cdev_reply`/`sdev_finish`，归 `17/23/21/22`）按 `fp_blocked_on` 分流，把挂起的调用续完。

这是 C 里 `int fp_blocked_on` + `union fp_u` 的设计动机，也是 Rust 版 `BlockedOn` 标签枚举（§3.2）的直接改造对象。**阻塞状态只在"挂起—恢复"期间有效**：`pm_fork` 断言父进程不可能阻塞（misc.c:625），`free_proc` 只回收不阻塞的槽位。

### 1.5 本章小结

- `fproc` 是 VFS 的每进程文件系统投影，回答"能碰哪些文件 / 以什么身份碰 / 卡在哪"。
- 四张进程表靠 endpoint 关联，VFS 私有维护自己的表，靠 `VFS_PM_*` 消息与 PM 对齐。
- 字段分三组（文件面 / 身份面 / 阻塞状态）+ 一组并发辅助字段。
- 阻塞状态是"挂起—恢复"上下文，`fp_blocked_on` + `fp_u` 是 C 的判别式 + 联合体，Rust 用标签枚举替代。

---

## 2. C 源码分析

### 2.1 struct fproc 全景（fproc.h:15-82）

```c
EXTERN struct fproc {
  unsigned fp_flags;               /* 16: 进程标志 */
  pid_t fp_pid;                    /* 18: 进程 ID（PID_FREE=0 表示槽空闲） */
  endpoint_t fp_endpoint;          /* 19: 内核 endpoint（NONE 表示槽空闲） */
  struct vnode *fp_wd;             /* 21: 工作目录 vnode */
  struct vnode *fp_rd;             /* 22: 根目录 vnode */
  struct filp *fp_filp[OPEN_MAX];  /* 24: fd 表（NULL=空闲） */
  fd_set fp_cloexec_set;           /* 25: FD_CLOEXEC 位图 */
  dev_t fp_tty;                    /* 27: 控制终端设备号 */
  int fp_blocked_on;               /* 29: 阻塞原因（const.h:19-25 的 0-6） */
  union ixfer_fp_u { ... } fp_u;   /* 30-61: 每阻塞类型的状态（§2.3） */
  uid_t fp_realuid;                /* 63: 真实 UID */
  uid_t fp_effuid;                 /* 64: 有效 UID（权限检查用） */
  gid_t fp_realgid;                /* 65: 真实 GID */
  gid_t fp_effgid;                 /* 66: 有效 GID */
  int fp_ngroups;                  /* 67: 补充组数量 */
  gid_t fp_sgroups[NGROUPS_MAX];   /* 68: 补充组数组 */
  mode_t fp_umask;                 /* 69: umask */
  mutex_t fp_lock;                 /* 71: fproc 槽位互斥锁（属于槽，§2.5） */
  struct worker_thread *fp_worker; /* 72: 当前 worker 线程 */
  void (*fp_func)(void);           /* 73: 待处理工作的 handler */
  message fp_msg;                  /* 74: 进程的挂起/活动消息 */
  message fp_pm_msg;               /* 75: 挂起/活动的延迟 PM 请求 */
  char fp_name[PROC_NAME_LEN];     /* 77: 最后 exec 的程序名 */
} fproc[NR_PROCS];                 /* 82: 全局槽位数组 */
```

（`#if LOCK_DEBUG` 下的 `fp_vp_rdlocks`/`fp_vmnt_rdlocks` 属编译期调试计数，fproc.h:78-81，见 plan.md §5.4 A-9。）

槽位空闲判定**不靠 flag**，靠双哨兵：`fp_pid == PID_FREE(0)` 且 `fp_endpoint == NONE`（main.c:405-408 初始化，`is_in_use` 判据）。

### 2.2 fp_flags：六位进程状态（fproc.h:91-98）

| 位 | 值 | 宏 | 含义与设置方 |
|----|-----|----|-------------|
| — | 0000 | `FP_NOFLAGS` | fork 后子进程的初始状态（misc.c:629） |
| 0 | 0001 | `FP_SRV_PROC` | 系统服务进程（`VFS_PM_SRV_FORK` 创建） |
| 1 | 0002 | `FP_REVIVED` | 正在从挂起恢复（pipe/lock 恢复路径置位，pipe.c:455-461） |
| 2 | 0004 | `FP_SESLDR` | 会话领导者（`pm_setsid` 置位，misc.c:790-791） |
| 4 | 0010 | `FP_PENDING` | 有待处理工作（worker_allow 门控期间置位，worker.c:176-184） |
| 5 | 0020 | `FP_EXITING` | 正在退出（`pm_exit` 置位后开始清理 fd，归 10/26） |
| 6 | 0040 | `FP_PM_WORK` | 有延迟的 PM 请求（service_pm_postponed，归 10） |

注意位 3（0x0008）未使用。标志与阻塞状态正交：`FP_REVIVED` 只出现在阻塞恢复路径中，但它修饰"恢复进行中"这一**瞬时状态**，与"阻塞原因"（`fp_blocked_on`）是两个维度（`revive` 里两者并存，pipe.c:455-461）。

### 2.3 fp_blocked_on + fp_u：判别式 + 联合体（const.h:19-25, fproc.h:30-61）

阻塞常量：

```c
#define FP_BLOCKED_ON_NONE	0 /* not blocked */
#define FP_BLOCKED_ON_PIPE	1 /* susp'd on pipe */
#define FP_BLOCKED_ON_FLOCK	2 /* susp'd on file lock */
#define FP_BLOCKED_ON_POPEN	3 /* susp'd on pipe open */
#define FP_BLOCKED_ON_SELECT	4 /* susp'd on select */
#define FP_BLOCKED_ON_CDEV	5 /* blocked on character device I/O */
#define FP_BLOCKED_ON_SDEV	6 /* blocked on socket I/O */
```

`fp_is_blocked(fp)` 宏即 `fp_blocked_on != FP_BLOCKED_ON_NONE`（const.h:28）。五个联合体成员（SELECT 无载荷，fproc.h:46 注释 "nothing for FP_BLOCKED_ON_SELECT for now"）：

| 成员 | 字段 | 行号 | 置位方 | 恢复方 |
|------|------|------|--------|--------|
| `u_pipe` | `callnr`（VFS_READ/VFS_WRITE）、`fd`、`buf`（用户缓冲地址）、`nbytes`（剩余字节）、`cum_io`（部分写入累计） | 31-37 | `pipe_suspend`（pipe.c:315-328） | `pipe_revive` 按 `callnr` 续作（pipe.c:399-407） |
| `u_popen` | `fd` | 38-40 | FIFO open 阻塞（pipe.c:304 计入 susp_count） | `revive` 直接回 fd（pipe.c:463-465） |
| `u_flock` | `fd`、`cmd`（恒为 F_SETLKW）、`arg`（用户 flock 结构地址） | 41-45 | `lock.c:96` suspend(F_SETLKW) | `lock_revive`（lock.c:172） |
| （无） | — | 46 | `select.c:339` suspend(SELECT) | `select_callback`/定时器 |
| `u_cdev` | `dev`、`endpt`（驱动 endpoint）、`grant`（数据 grant） | 47-51 | `cdev.c:339` suspend(CDEV) | `cdev_reply`（cdev.c:481，按 endpt 路由） |
| `u_sdev` | `dev`、`callnr`（原 socket 调用）、`grant[3]`、`aux`（fd 或 buf） | 52-60 | `sdev_suspend`（sdev.c:83-110） | `sdev_finish` 按 `callnr` 分流（sdev.c:783-916） |

C 的联合体语义：**判别式与载荷分离存储**——`fp_pipe` 只是 `fp_u.u_pipe` 的宏别名（fproc.h:85），即使 `fp_blocked_on == FP_BLOCKED_ON_CDEV`，代码仍能写 `fp_pipe.fd`。这一"未定义但可编译"的状态正是 Rust 标签枚举消灭的（§3.2）。

`suspend()` 的通用入口（pipe.c:302-309）断言当前不阻塞，置位 `fp_blocked_on` 并对 PIPE/POPEN 计数 `susp_count`；主循环收到 `SUSPEND` 返回码时不回复（ARCH A-5，归 09）。

### 2.4 凭证字段（fproc.h:63-69）

- **权限检查输入**：`forbidden()`（protect.c:238-）用 `fp_effuid`/`fp_effgid` + 遍历 `fp_sgroups` 判断访问权；`super_user` 宏即 `fp_effuid == SU_UID(0)`（glo.h:33）。
- **umask**：`do_umask` 更新 `fp_umask = ~(new_umask & RWX_MODES)`，返回旧值（protect.c:182-190）；新建文件权限 = 创建模式 `~fp_umask`。
- **boot 身份**：VFS_PM_INIT 握手时系统进程一律 `uid/gid = 0`、`umask = ~0`（main.c:419-425，const.h:16-17 `SYS_UID`）。
- **变更入口**：`pm_setuid`/`pm_setgid`/`pm_setgroups`（归 10）直接改写这些字段；fork 整体复制继承。

### 2.5 槽位关联字段：锁与消息缓冲（fproc.h:71-75）

这四个字段服务于 VFS 的并发模型，不是进程"身份"：

- `fp_lock`：保护 fproc 对象的互斥锁。**关键不变量：锁属于槽，不属于进程**——pm_fork 整体复制前先保存子槽自己的锁，复制后恢复（misc.c:606-608），否则父子会共享一把锁。ARCH A-6。
- `fp_worker`：当前处理该进程请求的 worker 线程（worker.c:137/208/249/283 维护）。
- `fp_func` + `fp_msg` + `fp_pm_msg`：挂起工作的 handler 与消息缓冲——`worker_suspend` 把进程消息存入 `fp_msg`、handler 存入 `fp_func`，`FP_PM_WORK` 时存 `fp_pm_msg`（worker.c:260-283, 389-416）。

这三者（worker 关联 + 消息缓冲）在 Rust 中移到请求槽（§3.4）。

### 2.6 生命周期：两遍初始化 + fork 整体复制

**第一遍**（main.c:405-408）：清空所有槽——`fp_endpoint = NONE`、`fp_pid = PID_FREE`。

**VFS_PM_INIT 握手**（main.c:410-436）：逐条收 PM 消息填槽（`fp_flags = FP_NOFLAGS`、`fp_blocked_on = NONE`、uid/gid=0、`umask = ~0`），`endpoint = NONE` 终止并同步。详见 `01-vfs-init-main.md` §2.3。

**第二遍**（main.c:468-483）：`mutex_init(fp_lock)`、`fp_worker = NULL`、清空 `fp_filp[]` 与 `fp_rd`/`fp_wd`（此时挂载未发生，目录锚点待 `mount_fs` 填写，main.c:477-483，清空循环在 480-483）。

**fork**（misc.c:577-634）：`fproc[childno] = fproc[parentno]` 整体复制 → 恢复子槽自己的 `fp_lock` → 对每个非空 `fp_filp[i]` 递增 `filp_count` → 写 `fp_pid = cpid`/`fp_endpoint = cproc` → `fp_flags = FP_NOFLAGS` → `dup_vnode(fp_rd)`/`dup_vnode(fp_wd)`。NDEBUG 下断言父进程 `fp_blocked_on == NONE`（misc.c:625）。

Rust 侧当前提供 `VfsState::handle_pm_fork` 骨架（main_loop.rs：父槽校验、子槽空闲检查、写 `pid`/`endpoint`/`flags = NOFLAGS`），对应上段的前半部；整体复制主体（`FProc: Clone` + `filp_count`/`dup_vnode`）归 `10-pm-protocol.md`。

### 2.7 fproc_light（fproc.h:111-115）

MIB 服务拉取的轻量投影（`fpl_tty`/`fpl_blocked_on`/`fpl_task`），由 `do_getsysinfo(SI_PROCLIGHT_TAB)` 填充（misc.c:81-97）。**归 `03-fproc-table.md`**（表面操作），本篇不展开；注意 fproc.h:114 的注释 "copy of fproc.fp_task" 是过时注释——实际取自 `fp_cdev.endpt`/`smap_endpt`（misc.c:88-93）。

---

## 3. Rust 设计决策

### 3.0 设计定位：对照 Redox 与 Linux 的取舍

| 维度 | Redox | Linux | Minix3（本设计） |
|------|-------|-------|------------------|
| fd 表位置 | 内核进程内 `FdTable`（`BTreeMap<Fd, Arc<dyn Resource>>`），文件对象走 scheme 驱动的 `Resource` trait | `files_struct` → `fdtable`（fd 数组 + 位图），跨 fork 引用计数共享 | `FProc.filps: [Option<usize>; OPEN_MAX]`——fd → 全局 filp 表下标 |
| 身份/目录/凭证 | 进程内字段，VFS 侧不复制进程表（scheme 消息带 uid/gid） | `fs_struct`/`cred` 分离，引用计数共享 | 平铺在 `FProc`，fork 整体复制 |
| 控制终端 | — | `signal_struct->tty`（会话级） | `fp_tty` 平铺字段 |
| 并发单位 | 单 scheme 请求/异步 | 内核线程 | 单线程事件循环 + 请求槽状态机（A-1） |

三个事实决定了本设计的形状：

1. **`fproc` 只在本服务内被访问**——没有跨子系统共享，所以不需要 Linux 式引用计数子结构，保留平铺结构 + `Clone` 即 fork 复制（misc.c:607 语义）。
2. **fd 表是数组而非 map**——`OPEN_MAX=255` 定长（syslimits.h:38，`__minix` 构建下非 NetBSD 回退值 128）、下标即 fd、O(1) 随机访问，与 Minix3 `fp_filp[OPEN_MAX]` 一致；Redox 用 `BTreeMap` 是因为 fd 空间稀疏且方案不同，这里定长数组 + `Option` 更诚实（04 文档展开 filp 表）。
3. **类型系统替代"注释即契约"**——C 里 `fp_pipe.callnr` 只能注释为 "VFS_READ or VFS_WRITE"；Rust 用 `PipeIo` 枚举让错误值**不可表示**。

**总原则**：不 1:1 翻译（每个字段类型化、联合体改标签枚举、哨兵改 `Option`），也不为抽象而抽象（保留平铺结构与 fork 复制语义，不引入 `Arc`/`RefCell` 等单线程下无必要的间接）。

### 3.1 BlockedOn 标签枚举取代 int + union（ARCH A-3）

```rust
pub enum BlockedOn {
    None,
    Pipe(PipeBlock),
    PipeOpen(PipeOpenBlock),
    Flock(FlockBlock),
    Select,                 // select 无载荷（fproc.h:46）
    Cdev(CdevBlock),
    Sdev(SdevBlock),
}
```

- **判别式与载荷合一**：变体即判别式，载荷是变体的字段。C 里"`fp_blocked_on=CDEV` 但读 `fp_pipe.fd`"能编译；Rust 里从 `BlockedOn::Cdev` 提取 `PipeBlock` 是编译错误。这是 sum type 对"注释即契约"的根本改善。
- **五个载荷结构**对应 C 的五个匿名 struct，字段全部类型化：`PipeBlock { call: PipeIo, fd, buf: VirBytes, nbytes, cum_io }`、`PipeOpenBlock { fd }`、`FlockBlock { fd, cmd: FlockCmd, arg: VirBytes }`、`CdevBlock { dev: DevId, endpt: Endpoint, grant: Option<GrantId> }`、`SdevBlock { dev, call: SdevCall, grants: [Option<GrantId>; 3], aux: SdevAux }`。
- **子类型化**：`PipeIo::{Read,Write}`（pipe.c:399-407 只有这两种）、`FlockCmd::SetLkw`（fproc.h:43 "always F_SETLKW"，F_GETLK/F_SETLK 不阻塞故不可表示）、`SdevCall`（11 个可挂起调用，sdev_finish 分流证据见 §4.2）、`SdevAux::{Fd,Buf,None}`（fproc.h:56-59）。

### 3.2 类型化字段（ARCH A-8）

C 裸标量 → Rust 新类型/别名（`minix-types`）：

| C 类型 | Rust 类型 | 说明 |
|--------|----------|------|
| `pid_t` | `Pid` (i32) | 已有 |
| `endpoint_t` | `Endpoint` | 已有；`NONE` 哨兵保留（`is_none()`） |
| `uid_t`/`gid_t` | `Uid`/`Gid` (u32) | 已有 |
| `dev_t` (u64) | `DevId` (u64) | **本次新增**（minix3/sys/sys/types.h:187 `uint64_t`） |
| `mode_t` (u32) | `Mode` (u32) | **本次新增**（ansi.h:41 `__uint32_t`） |
| `cp_grant_id_t` (i32) | `GrantId` (i32) | **本次新增**（type.h `int32_t`） |
| `vir_bytes` | `VirBytes` (u64) | 已有；用户缓冲地址 |

新增三个别名加 `NO_DEV = 0` 常量（minix3/minix/include/minix/const.h:132），全部落在 `minix-types/src/types/id.rs`，与 Uid/Gid/Pid 同一模式（别名而非新类型，因为它们在 ABI/算术中天然互操作）。`NO_DEV` 保留哨兵而非 `Option<DevId>`，理由见 §3.5。

### 3.3 fp_lock 属于槽：单线程下的降级（ARCH A-6）

C 用 `mutex_t fp_lock` 保护 fproc 并发访问，pm_fork 必须"保存→复制→恢复"子槽自己的锁（misc.c:606-608）。本设计：

- **`FProc` 不含锁字段**——锁属于槽位（表条目），不属于进程对象；Rust 中"槽"即 `FProcTable.slots[i]`，借用规则天然互斥（`get_mut` 同一时刻只有一个可变借用）。
- **单线程事件循环**（A-1）下，C 的 mutex 退化为借用检查：同一请求槽内 `&mut FProc` 独占，无需 `Rc`/`RefCell`/`Mutex` 任何运行时同步（`!Send`/`!Sync` 是 VFS 的既定执行模型，见 AGENTS.md）。
- fork 复制语义保留：`FProc: Clone`（结构复制）+ 显式"保留子槽锁"步骤在 10-pm-protocol 的 `pm_fork` 中体现（锁已不在结构内，天然不会被覆盖）。

### 3.4 fp_worker/fp_func/fp_msg/fp_pm_msg：移到请求槽

这四个字段是**请求执行期状态**而非进程身份：

- `fp_worker`（哪个 worker 在处理）→ 反向关联已在 `WorkerThread.fp_slot: Option<UserSlot>`（worker.rs:48）。只保留一个方向，避免双向指针维护。
- `fp_func`（待执行 handler）→ `WorkerThread.func: Option<WorkerFunc>`（worker.rs:57，`WorkerFunc::DoWork/PmReboot/...` 枚举，替代 C 函数指针）。
- `fp_msg`（进程的挂起消息）→ 请求槽的 `w_m_in`（worker 执行时装载）；`VfsState.current_message` 承载"正在处理的消息"（A-4 聚合）。
- `fp_pm_msg`（延迟的 PM 请求）→ 归 `10-pm-protocol.md` 的 service_pm_postponed 状态机，不在 fproc 结构里。

**理由**：这些字段的生命周期是"一个请求"，而 FProc 的生命周期是"一个进程"；混在一起会让"进程上下文"和"请求上下文"互相污染（C 里 `fp_msg` 在进程不活跃时是死数据）。事件循环下"当前请求"是显式上下文（`VfsState`），进程字段只保留跨请求存活的状态。

### 3.5 哨兵 vs Option：两处取舍

| C 哨兵 | 本设计 | 理由 |
|--------|--------|------|
| `fp_tty = 0`（NO_DEV） | `tty: DevId` + `NO_DEV` 常量 | `fp_tty` 直接参与设备号相等比较（cdev.c:46-49/185-189、misc.c:683-687），`Option<DevId>` 会让每个比较点多一次解包；`NO_DEV` 是命名的领域常量而非魔法 0 |
| `GRANT_INVALID`（-1） | `Option<GrantId>` | grant 只做"有/无"判断（`GRANT_VALID`）与吊销（`cpf_revoke`），`Option` 让"无 grant"不可误用为 id |

### 3.6 字段映射总表

| C 字段（fproc.h） | Rust 位置 | 处理 |
|------------------|----------|------|
| `fp_flags` | `FProc.flags: FpFlags` | bitflags，位值 = C（§4.1 测试锁定） |
| `fp_pid`/`fp_endpoint` | `FProc.pid`/`FProc.endpoint` | 保留；`PID_FREE`/`Endpoint::NONE` 双哨兵 |
| `fp_wd`/`fp_rd` | `work_dir`/`root_dir: Option<usize>` | 全局 vnode 表下标 |
| `fp_filp[]` | `filps: [Option<usize>; OPEN_MAX]` | 全局 filp 表下标 |
| `fp_cloexec_set` | `cloexec_set: Bitmap`（255 位，`size = OPEN_MAX`） | 255 位 fd 位图，匹配 `fd_set`（FD_SETSIZE=255，fd_set.h:60） |
| `fp_tty` | `tty: DevId` | NO_DEV 哨兵（§3.5） |
| `fp_blocked_on`+`fp_u` | `blocked_on: BlockedOn` | 标签枚举（A-3） |
| 凭证 8 字段 | `real_uid`/`eff_uid`/`real_gid`/`eff_gid`/`ngroups`/`supplemental_groups`/`umask: Mode` | 类型化 |
| `fp_lock` | （无） | 槽位借用规则（A-6，§3.3） |
| `fp_worker`/`fp_func` | `WorkerThread.fp_slot`/`func` | 反向关联（§3.4） |
| `fp_msg`/`fp_pm_msg` | `VfsState`/请求槽 | 归 09/10（§3.4） |
| `fp_name` | `name: [u8; PROC_NAME_LEN]` | exec.c:384 写入 |

---

## 4. 实现详解

### 4.1 模块结构（os/servers/vfs/src/fproc.rs）

```
常量      OPEN_MAX=255（syslimits.h:38）/ NGROUPS_MAX=16（syslimits.h:59）/ PROC_NAME_LEN / PID_FREE
类型      FpFlags (bitflags) → BlockedOn (标签枚举) → 5 个载荷结构 + PipeIo/FlockCmd/SdevCall/SdevAux
结构      FProc（§3.6 映射表落地）→ FProcTable（Box<[FProc]>，NR_PROCS 编译期定界）
方法      new_unused / is_blocked / is_idle / is_in_use
表方法    get / get_mut / find_by_endpoint(_mut) / reset_all / init_phase2
```

### 4.2 关键载荷的精确性

**`SdevCall` 的 11 个变体**是"可挂起 socket 调用"的完整集合，证据链：

```bash
$ rg -n 'sdev_suspend\(' minix3/minix/servers/vfs/sdev.c
83:   sdev_suspend(...)                # 定义
213:  return sdev_suspend(...)         # sdev_bindconn → BIND/CONNECT
329:  return sdev_suspend(...)         # sdev_accept → ACCEPT
409:  return sdev_suspend(...)         # sdev_sendrecv → READ/WRITE/SENDTO/RECVFROM/SENDMSG/RECVMSG
447:  return sdev_suspend(...)         # sdev_ioctl → IOCTL
635:  return sdev_suspend(...)         # sdev_close → CLOSE
```

`shutdown` 走同步 `sdev_simple`（sdev.c:592-598），`getsockopt`/`setsockopt`/`getsockname`/`getpeername` 走同步 `sdev_sendrec`（sdev.c:505-546），均**不挂起**，故不在枚举内——注释明示负空间，防未来误加。`SdevAux` 三态（`Fd`/`Buf`/`None`）对应 fproc.h:56-59 的 aux union + "else 分支两者皆无"（sdev.c:104-107 的 assert）。

**`FlockCmd::SetLkw`** 单变体枚举表达"唯一可阻塞的 fcntl 命令"（fproc.h:43 注释），`F_GETLK`/`F_SETLK` 立即返回故不可表示——与 PM 侧 `IpcBlockReason::VfsCall { reply_to_new_parent }` 的"非法组合不可表示"模式同型。

### 4.3 FProc 方法

- `new_unused()`：`const fn`，`pid = PID_FREE`、`endpoint = Endpoint::NONE`、`tty = NO_DEV`、`umask = 0`（未用槽的 umask 无意义，BSS 零值语义；boot 槽握手时再写 `!0`）。
- `is_blocked()`：`blocked_on != None`，对应 `fp_is_blocked` 宏（const.h:28）。
- `is_idle()`：无 `PENDING`/`PM_WORK` 且未阻塞——worker 绑定的就绪判据。
- `is_in_use()`：`pid != PID_FREE`。

### 4.4 FProcTable

`NR_PROCS` 定长语义 + `UserSlot` 直接索引（`fproc_addr(e)` 宏语义，glo.h:27）。存储用 `Box<[FProc]>`（堆分配）而非按值内嵌 `[FProc; NR_PROCS]`：每槽约 4.4 KiB、整表约 1.1 MiB，按值构造时 debug 构建会在多帧间复制数组、撑爆 2 MiB 的默认测试线程栈。C 侧 `fproc[]` 是静态 BSS；Rust 侧由 A-4 聚合（`VfsState` 拥有表）接管，堆是存储的自然归宿（A-4 设计的一部分），`NR_PROCS` 仍是编译期上界，语义与 BSS 零初始化一致。`find_by_endpoint` = `endpoint.to_user_slot()` 范围校验 + 下标访问，越界返回 `None`（C 的越界数组访问改为显式失败）。`init_phase2()` 对应 main.c:480-483 的第二遍清空；`reset_all()` 对应 main.c:405-408，为重启/LU 路径（DEFERRED）保持诚实。

### 4.5 minix-types 新增（types/id.rs）

`DevId = u64`、`Mode = u32`、`GrantId = i32` 三个别名 + `NO_DEV = 0` 常量（minix3/minix/include/minix/const.h:132）。与 Uid/Gid/Pid 同模式，为后续设备文档（19~22）与 fcntl 文档（30）复用。

### 4.6 关键不变量

1. **双哨兵空闲**：`pid == PID_FREE && endpoint == NONE` 才视为未用（`is_in_use` 只查 pid，与 C 的 `fp_pid != PID_FREE` 判据一致）。
2. **阻塞不可叠加**：`BlockedOn` 是单值枚举，天然互斥；`suspend()` 的"先断言 NONE 再置位"（pipe.c:302-309）由类型系统兜底——旧值必然已被消费。
3. **载荷与变体一致**：编译期保证（§3.1）。
4. **fork 时父不阻塞**：misc.c:625 的断言在 Rust 中是 `pm_fork` 的输入契约（归 10）。

---

## 5. 测试要点

### 5.1 本次新增/修订测试

**`minix-types`（types/id.rs，+3 个）**：

- `test_dev_no_dev_sentinel`：`NO_DEV == 0`（minix3/minix/include/minix/const.h:132）。
- `test_mode_is_32bit`：`Mode` 占 4 字节（ansi.h:41）。
- `test_grant_id_is_32bit`：`GrantId` 占 4 字节（type.h）。

**`minix-vfs`（fproc.rs，11 个，含 +3 新增）**：

- `test_fproc_is_blocked_tagged_enum`：Pipe 载荷携带恢复参数；切到 Cdev 变体后提取 Pipe 字段是编译错误（注释锁定类型安全收益）。
- `test_blocked_on_payload_roundtrip`：Pipe/Flock/Sdev 载荷构造与相等性。
- `test_fp_flags_values_match_c`：6 个位值与 fproc.h:92-98 逐位锁定（0x0001/0x0002/0x0004/0x0010/0x0020/0x0040）。
- 其余 8 个（`new_unused`/`is_in_use`/`default`/表操作/`init_phase2`/标志位操作）沿用修订。

### 5.2 测试总数声明

- `cargo test -p minix-vfs`：50 passed（基线 47 → +3，其中 fproc 模块 11 个）。
- `cargo test -p minix-types`：94 passed（基线 91 → +3）。
- `cargo clippy -p minix-vfs`：本模块 0 新警告（仅存量 minix-sys stub / dispatcher 未用导入）。

---

## 6. 过渡

02 回答了"`fproc` 里有什么"；03 回答"怎么找、怎么验证、怎么复用槽位"：

```
01（启动骨架：握手把 boot 进程填进 fproc 槽）
  → 02（结构：字段/标志/阻塞状态——本篇）
  → 03（表操作：FProcTable/endpoint 验证/fproc_light）
  → 04~06（核心数据结构：filp/vnode/vmnt 表）
  → 07/08（并发基础：三级锁 + worker 池状态机）
  → 09（主循环与分发）→ 10（PM 协议：pm_fork/pm_exit 消费本结构的生命周期）
```

下一篇读 `03-fproc-table.md`——`FProcTable` 的 `find_by_endpoint`/`get_mut` 是全部 `fp = &fproc[slot]` 访问的入口；阻塞状态机的通用回复路径（`ReplyIntent`）在 `09-main-loop.md`，pipe/select/cdev/sdev 四条恢复路径分别在 `17/23/21/22`。

---

## 7. 参见

- `plan.md` §1.2（启动时序）/§3.4（边界表 02 行）/§4（ARCH A-3/A-6/A-8）/§5.3（02 函数清单）/§7.3（决策 2：ReplyIntent）
- `01-vfs-init-main.md` — 启动链与 VFS_PM_INIT 握手（fproc 槽的建立）
- `../04-stage-pm/02-mproc-struct.md` — PM 侧进程结构对照（四张表 + IdSet 模式）
- `../04-stage-pm/05-vfs-interaction.md` — PM 侧 VFS 协议（对端状态机）
- `minix3/minix/servers/vfs/fproc.h` — ground truth（15-82/91-98/111-115）
- `minix3/minix/servers/vfs/const.h` — 阻塞常量（19-25）
- `minix3/minix/include/minix/const.h` — NO_DEV（132）
- `minix3/minix/servers/vfs/misc.c` — pm_fork（577-634）、fproc_light 填充（80-97）
- `minix3/minix/servers/vfs/main.c` — 两遍初始化（405-408/468-483）
- `minix3/minix/servers/vfs/pipe.c` — suspend/pipe_suspend/revive（294-333/435-560）
- `minix3/minix/servers/vfs/sdev.c` — sdev_suspend/sdev_finish（83-110/759-916）
- `minix3/sys/sys/types.h` — dev_t 64 位定义（187）
- `draft/01-fproc-struct.md` + `draft/02-fproc-flags.md` + `draft/03-fproc-cred.md` + `draft/fproc-design.md` — 旧素材
