# 02-fproc-flags: FProc 标志与阻塞状态

> 本文档分析 `minix3/minix/servers/vfs/fproc.h` 和 `const.h` 中的进程标志和阻塞状态机制。

---

## 1. 概述

### 1.1 标志系统的作用

`fp_flags` 控制 VFS 进程的状态标志——位图编码，每位表示一种进程状态
`fp_blocked_on` + `fp_u` 联合体管理进程在 VFS 中的阻塞状态——枚举值标识阻塞类型，联合体保存阻塞时的上下文
与 Kernel 的 `p_rts_flags` 对比——VFS 标志更高级、更具体。内核 `p_rts_flags` 是位图（进程可同时处于多种阻塞状态），VFS `fp_blocked_on` 是枚举（进程同一时刻只能阻塞在一种文件系统操作上）

---

## 2. fp_flags 标志位

### 2.1 标志定义

所有 fp_flags 标志位定义（来自 fproc.h）
FP_NOFLAGS (0x0000) — 无标志，fork 后子进程的初始状态
FP_SRV_PROC (0x0001) — 系统服务进程标志，通过 VFS_PM_SRV_FORK 创建的子进程会设置此标志
FP_REVIVED (0x0002) — 进程正在被恢复（从阻塞中唤醒），用于防止重复唤醒
FP_SESLDR (0x0004) — 会话领导者，由 setsid() 设置，fork 时不继承
FP_PENDING (0x0010) — 有待处理操作，VFS 需要重新处理该进程的请求
FP_EXITING (0x0020) — 进程正在退出，pm_exit() 设置此标志后开始清理文件描述符
FP_PM_WORK (0x0040) — 有 PM 工作待处理，PM 发来的请求需要延迟处理

### 2.2 标志与 fork 的关系

fork 后子进程 `fp_flags = FP_NOFLAGS`——清除所有标志
为什么清除所有标志——子进程不应继承父进程的退出/恢复/PM 状态。子进程是全新的进程，不应处于退出中、不应有挂起的恢复操作、不应有延迟的 PM 请求

---

## 3. 阻塞状态机制

### 3.1 fp_blocked_on

`int fp_blocked_on` 字段——进程被阻塞的原因，枚举值标识阻塞类型
阻塞类型常量（来自 const.h），见下方代码块

```
FP_BLOCKED_ON_NONE   0  — 未阻塞
FP_BLOCKED_ON_PIPE   1  — 阻塞在管道 I/O
FP_BLOCKED_ON_FLOCK  2  — 阻塞在文件锁
FP_BLOCKED_ON_POPEN  3  — 阻塞在管道打开
FP_BLOCKED_ON_SELECT 4  — 阻塞在 select
FP_BLOCKED_ON_CDEV   5  — 阻塞在字符设备 I/O
FP_BLOCKED_ON_SDEV   6  — 阻塞在 socket I/O
```

### 3.2 fp_is_blocked 宏

`fp_is_blocked(fp)` 宏：判断进程是否处于阻塞状态，等价于 `fp_blocked_on != FP_BLOCKED_ON_NONE`
fork 断言：pm_fork() 中 `assert(!fp_is_blocked(fp))` 保证父进程不可能被阻塞——因为 PM 只会在进程可运行时才发送 VFS_PM_FORK

---

## 4. 阻塞状态联合体 (ixfer_fp_u)

### 4.1 u_pipe — 管道阻塞

管道阻塞时保存的状态：callnr (VFS_READ 或 VFS_WRITE), fd (文件描述符), buf (用户缓冲区地址), nbytes (剩余字节数), cum_io (部分写入累计)
管道读写导致阻塞：当管道缓冲区为空时读阻塞，缓冲区满时写阻塞。VFS 将进程标记为 FP_BLOCKED_ON_PIPE 并保存 I/O 上下文，等待数据就绪后通过 pipe_revive() 唤醒

### 4.2 u_popen — 管道打开阻塞

管道打开阻塞时保存的状态：fd (正在打开的文件描述符)
FIFO 打开时 O_RDONLY/O_WRONLY 的阻塞行为：以 O_RDONLY 打开 FIFO 时若无写者则阻塞，以 O_WRONLY 打开时若无读者则阻塞。O_NONBLOCK 标志可避免阻塞

### 4.3 u_flock — 文件锁阻塞

文件锁阻塞时保存的状态：fd (文件描述符), cmd (fcntl 命令，总是 F_SETLKW), arg (用户空间 flock 结构体地址)
F_SETLKW (阻塞式锁) 的行为：当请求的锁与已有锁冲突时，进程阻塞直到锁可用。VFS 将进程标记为 FP_BLOCKED_ON_FLOCK 并保存锁请求参数，锁释放后通过 lock_revive() 唤醒

### 4.4 u_cdev — 字符设备阻塞

字符设备阻塞时保存的状态：dev (设备号), endpt (驱动程序 endpoint), grant (数据授权 ID)
字符设备读写导致阻塞：当设备缓冲区为空（读）或满（写）时，VFS 向驱动发送请求后将进程标记为 FP_BLOCKED_ON_CDEV，驱动完成操作后通过 REVIVE 消息唤醒进程

### 4.5 u_sdev — socket 设备阻塞

socket 阻塞时保存的状态：dev (socket 设备号), callnr (VFS socket 调用号), grant[3] (最多 3 个数据授权), aux (辅助数据：fd 或 buf 地址)
socket 操作导致阻塞：accept/recvmsg/connect 等操作在无连接/无数据时阻塞。VFS 将进程标记为 FP_BLOCKED_ON_SDEV 并保存请求参数，网络服务完成操作后通过 REVIVE 消息唤醒

### 4.6 select 阻塞

FP_BLOCKED_ON_SELECT 没有额外的联合体字段——select 的状态通过 select 机制内部管理，不需要在 fproc 中保存额外上下文
select 状态通过 select 机制内部管理，VFS 只需知道进程阻塞在 select 上，就绪后通过 select_callback() 唤醒

---

## 5. 阻塞与 fork

### 5.1 pm_fork 的断言

`pm_fork()` 中的断言：`assert(!fp_is_blocked(fp))`——A forking process cannot possibly be suspended on anything
为什么这个不变量成立——PM 只会在进程可运行时才发送 VFS_PM_FORK。进程发起 fork 系统调用时必然处于运行状态，PM 处理 fork 请求时进程不可能在 VFS 中阻塞

### 5.2 子进程初始阻塞状态

子进程的 fp_blocked_on 被继承为 FP_BLOCKED_ON_NONE——因为整体复制后父进程未被阻塞（断言保证），所以子进程的 fp_blocked_on 自然也是 FP_BLOCKED_ON_NONE
如果父进程有残留的阻塞状态字段会发生什么——不会发生，因为断言保证父进程未被阻塞，fp_u 联合体中的值无意义（fp_blocked_on 为 NONE 时不访问 fp_u）

---

## 6. C 源码

**文件**: `minix3/minix/servers/vfs/fproc.h` (阻塞状态联合体)

```c
int fp_blocked_on;		/* what is it blocked on */
union ixfer_fp_u {		/* state per blocking type */
	struct {			/* FP_BLOCKED_ON_PIPE */
		int callnr;
		int fd;
		vir_bytes buf;
		size_t nbytes;
		size_t cum_io;
	} u_pipe;
	struct {			/* FP_BLOCKED_ON_POPEN */
		int fd;
	} u_popen;
	struct {			/* FP_BLOCKED_ON_FLOCK */
		int fd;
		int cmd;
		vir_bytes arg;
	} u_flock;
	struct {			/* FP_BLOCKED_ON_CDEV */
		dev_t dev;
		endpoint_t endpt;
		cp_grant_id_t grant;
	} u_cdev;
	struct {			/* FP_BLOCKED_ON_SDEV */
		dev_t dev;
		int callnr;
		cp_grant_id_t grant[3];
		union ixfer_u_aux {
			int fd;
			vir_bytes buf;
		} aux;
	} u_sdev;
```

**文件**: `minix3/minix/servers/vfs/const.h` (阻塞常量)

```c
#define FP_BLOCKED_ON_NONE	0
#define FP_BLOCKED_ON_PIPE	1
#define FP_BLOCKED_ON_FLOCK	2
#define FP_BLOCKED_ON_POPEN	3
#define FP_BLOCKED_ON_SELECT	4
#define FP_BLOCKED_ON_CDEV	5
#define FP_BLOCKED_ON_SDEV	6
```
