# servers/pm/exec.c 逐行讲解

> **文件路径**: `minix3/minix/servers/pm/exec.c`
> **核心功能**: exec 系统调用实现——加载新程序替换当前进程映像

---

## 文件概述

### 是什么（功能说明）

这个文件实现了 **exec 系统调用**，它是 Unix/Linux 系统中最核心的系统调用之一。exec 的作用是：**用一个新的程序替换当前进程的内存映像**，但进程 ID（PID）保持不变。

想象一下：你有一个正在运行的进程（比如 shell），当它执行 `execve("/bin/ls", ...)` 时，这个进程从"运行 shell 代码"变成了"运行 ls 代码"，但 PID 没变，打开的文件描述符也没变（除非设置了 FD_CLOEXEC）。

### 为什么（设计原因）

**为什么 exec 和 fork 是两个独立的系统调用？**

这是 Unix 的经典设计哲学：
1. **fork**：创建进程的副本（复制）
2. **exec**：加载新程序（替换）

这种分离设计带来了极大的灵活性：
- `fork()` + `exec()` = 创建新进程运行新程序
- 单独 `fork()` = 创建当前程序的副本（并行处理）
- 单独 `exec()` = 当前进程变身（不创建新进程）

**为什么 PM 不直接读取可执行文件？**

Minix3 是微内核架构，文件系统操作由 VFS（虚拟文件系统服务器）负责。PM（进程管理器）只管理进程元数据，不直接访问文件系统。所以 exec 流程是：

```
用户进程 --exec--> PM --转发--> VFS --读取文件--> PM --完成--> 内核
```

### 什么情景使用（应用场景）

1. **Shell 执行命令**：用户输入 `ls -l`，shell fork 后 exec `/bin/ls`
2. **编辑器启动子进程**：vim 执行 `:!make` 时 fork + exec make
3. **守护进程启动**：init 进程启动系统服务
4. **脚本执行**：`#!/bin/bash` 脚本被 exec 执行

---

## 逐行讲解

### 文件头注释

```c
/* This file handles the EXEC system call.  It performs the work as follows:
 *    - see if the permissions allow the file to be executed
 *    - read the header and extract the sizes
 *    - fetch the initial args and environment from the user space
 *    - allocate the memory for the new process
 *    - copy the initial stack from PM to the process
 *    - read in the text and data segments and copy to the process
 *    - take care of setuid and setgid bits
 *    - fix up 'mproc' table
 *    - tell kernel about EXEC
 *    - save offset to initial argc (for procfs)
```

**翻译**：
> 这个文件处理 EXEC 系统调用。它按以下步骤执行工作：
> - 检查权限是否允许执行该文件
> - 读取文件头并提取各段大小
> - 从用户空间获取初始参数和环境变量
> - 为新进程分配内存
> - 将初始栈从 PM 复制到进程
> - 读取代码段和数据段并复制到进程
> - 处理 setuid 和 setgid 位
> - 修正 'mproc' 表
> - 通知内核关于 EXEC 的信息
> - 保存初始 argc 的偏移量（供 procfs 使用）

**设计思路讲解**：

这 10 个步骤展示了 exec 的完整流程。注意：**这些步骤大部分由 VFS 执行**，PM 只负责协调和更新进程表。这种分工是微内核设计的体现——每个服务器只做自己擅长的事。

---

### 入口点声明

```c
 * The entry points into this file are:
 *   do_exec:	 perform the EXEC system call
 *   do_newexec: handle PM part of exec call after VFS
 *   do_execrestart: finish the special exec call for RS
 *   exec_restart: finish a regular exec call
 */
```

**翻译**：
> 这个文件的入口点有：
> - do_exec：执行 EXEC 系统调用
> - do_newexec：处理 VFS 完成后的 exec 调用的 PM 部分
> - do_execrestart：完成 RS（重启服务）的特殊 exec 调用
> - exec_restart：完成常规 exec 调用

**设计原因**：

为什么有这么多入口点？因为 exec 是一个**异步多阶段**操作：

| 函数 | 调用者 | 时机 |
|------|--------|------|
| `do_exec` | 用户进程 | 用户发起 exec 系统调用 |
| `do_newexec` | VFS | VFS 完成文件加载后通知 PM |
| `do_execrestart` | RS | RS 服务重启时恢复执行状态 |
| `exec_restart` | PM 内部 | 最终完成 exec，通知内核 |

---

### 头文件包含

```c
#include "pm.h"
#include <sys/stat.h>
#include <minix/callnr.h>
#include <minix/endpoint.h>
#include <minix/com.h>
#include <minix/vm.h>
#include <signal.h>
#include <libexec.h>
#include <sys/ptrace.h>
#include "mproc.h"
```

**逐个解释**：

| 头文件 | 作用 | 关键内容 |
|--------|------|----------|
| `"pm.h"` | PM 主头文件 | 包含 PM 需要的所有公共定义 |
| `<sys/stat.h>` | 文件状态 | `struct stat`，文件权限位（S_IXUSR 等） |
| `<minix/callnr.h>` | 系统调用号 | `NR_EXEC` 等系统调用编号 |
| `<minix/endpoint.h>` | 端点定义 | `endpoint_t` 类型，进程标识 |
| `<minix/com.h>` | 通信定义 | `VFS_PM_EXEC` 等消息类型 |
| `<minix/vm.h>` | 虚拟内存 | VM 相关常量 |
| `<signal.h>` | 信号处理 | `SIG_DFL`，`_NSIG` 等 |
| `<libexec.h>` | 执行库 | `struct exec_info`，程序加载辅助函数 |
| `<sys/ptrace.h>` | 调试支持 | `TO_NOEXEC` 等调试标志 |
| `"mproc.h"` | 进程结构 | `struct mproc`，PM 的进程控制块 |

**内存布局**：

```
进程地址空间
┌─────────────────┐ 高地址
│    栈 (stack)   │ ← sp 指向这里
│       ↓         │   包含 argv, envp, 辅助向量
├─────────────────┤
│                 │
│   未分配空间     │
│                 │
├─────────────────┤
│    数据段        │ ← 全局变量, 静态变量
├─────────────────┤
│    代码段        │ ← 程序指令, 只读
└─────────────────┘ 低地址
         ↑
        pc 指向代码段入口点
```

---

### 宏定义

```c
#define ESCRIPT	(-2000)	/* Returned by read_header for a #! script. */
#define PTRSIZE	sizeof(char *) /* Size of pointers in argv[] and envp[]. */
```

**逐行解释**：

**第 1 行**：`#define ESCRIPT (-2000)`
- **是什么**：定义脚本文件的错误码
- **为什么**：当文件以 `#!` 开头（shebang）时，表示这是一个脚本文件，需要用解释器执行。VFS 会返回这个特殊值告诉 PM "这是个脚本，需要找解释器"
- **值 -2000**：负数表示错误，但不是标准 errno（标准 errno 通常在 -1 到 -100 范围）
- **应用场景**：执行 `./script.sh` 时，内核发现 `#!`，会改为执行 `/bin/sh ./script.sh`

**第 2 行**：`#define PTRSIZE sizeof(char *)`
- **是什么**：定义指针大小（4 或 8 字节，取决于 32/64 位系统）
- **为什么**：argv 和 envp 是指针数组，需要知道每个元素的大小来正确遍历
- **内存模型**：
  ```
  栈布局（32位系统）：
  ┌────────────┐ 高地址
  │ envp[n]    │ 字符串内容
  │ ...        │
  │ argv[m]    │
  ├────────────┤
  │ NULL       │ envp 结束标记
  │ envp[2]    │ 4字节指针
  │ envp[1]    │
  │ envp[0]    │
  ├────────────┤
  │ NULL       │ argv 结束标记
  │ argv[2]    │
  │ argv[1]    │
  │ argv[0]    │
  ├────────────┤
  │ argc       │ 4字节整数
  └────────────┘ 低地址 ← sp 指向这里
  ```

---

### do_exec 函数

```c
/*===========================================================================*
 *				do_exec					     *
 *===========================================================================*/
int
do_exec(void)
{
	message m;

	/* Forward call to VFS */
	memset(&m, 0, sizeof(m));
	m.m_type = VFS_PM_EXEC;
	m.VFS_PM_ENDPT = mp->mp_endpoint;
	m.VFS_PM_PATH = (void *)m_in.m_lc_pm_exec.name;
	m.VFS_PM_PATH_LEN = m_in.m_lc_pm_exec.namelen;
	m.VFS_PM_FRAME = (void *)m_in.m_lc_pm_exec.frame;
	m.VFS_PM_FRAME_LEN = m_in.m_lc_pm_exec.framelen;
	m.VFS_PM_PS_STR = m_in.m_lc_pm_exec.ps_str;

	tell_vfs(mp, &m);

	/* Do not reply */
	return SUSPEND;
}
```

**逐行详细解释**：

**第 1-3 行**：函数签名和注释
```c
int
do_exec(void)
```
- **是什么**：exec 系统调用的 PM 入口函数
- **返回值**：`int` 类型，返回 `SUSPEND` 表示挂起当前进程，等待 VFS 回复
- **参数**：无参数，所有输入通过全局变量 `m_in`（输入消息）和 `mp`（当前进程指针）获取

**第 4 行**：`message m;`
- **是什么**：声明一个消息结构体，用于发送给 VFS
- **内存位置**：栈上分配，约 64 字节（Minix 消息固定大小）
- **为什么需要**：PM 和 VFS 通过 IPC 消息通信，需要构造请求消息

**第 6 行**：`memset(&m, 0, sizeof(m));`
- **是什么**：将消息结构体清零
- **为什么**：确保所有未设置的字段都是 0，避免垃圾数据
- **内存操作**：
  ```
  清零前: [?? ?? ?? ?? ?? ?? ?? ?? ...] (随机值)
  清零后: [00 00 00 00 00 00 00 00 ...] (全零)
  ```

**第 7 行**：`m.m_type = VFS_PM_EXEC;`
- **是什么**：设置消息类型为 `VFS_PM_EXEC`
- **值**：`VFS_PM_EXEC = (VFS_PM_RQ_BASE + 6)`，约 0x4006
- **为什么**：VFS 收到消息后根据 `m_type` 判断请求类型，分发给对应的处理函数

**第 8 行**：`m.VFS_PM_ENDPT = mp->mp_endpoint;`
- **是什么**：设置目标进程的端点号
- **`mp`**：全局变量，指向当前调用进程的 `mproc` 结构
- **`mp_endpoint`**：进程的唯一标识符（endpoint），格式：`(slot << 16) | generation`
- **为什么需要**：VFS 需要知道要为哪个进程执行 exec

**第 9 行**：`m.VFS_PM_PATH = (void *)m_in.m_lc_pm_exec.name;`
- **是什么**：设置可执行文件路径的指针
- **`m_in`**：全局变量，保存用户进程发来的系统调用消息
- **`m_lc_pm_exec.name`**：用户空间中文件路径字符串的地址
- **内存布局**：
  ```
  用户进程空间:
  ┌─────────────────────┐
  │ "/bin/ls\0"         │ ← m_in.m_lc_pm_exec.name 指向这里
  └─────────────────────┘
  PM 空间:
  ┌─────────────────────┐
  │ m.VFS_PM_PATH       │ = 用户空间地址（需要 VFS 访问用户内存）
  └─────────────────────┘
  ```

**第 10 行**：`m.VFS_PM_PATH_LEN = m_in.m_lc_pm_exec.namelen;`
- **是什么**：路径字符串的长度（包括结尾的 '\0'）
- **为什么需要**：避免 VFS 逐字节扫描字符串长度，提高效率

**第 11 行**：`m.VFS_PM_FRAME = (void *)m_in.m_lc_pm_exec.frame;`
- **是什么**：设置栈帧（包含 argv 和 envp）的指针
- **栈帧内容**：
  ```
  用户空间栈帧:
  ┌─────────────────┐
  │ argc = 2        │
  │ argv[0] ────────┼──→ "ls"
  │ argv[1] ────────┼──→ "-l"
  │ NULL            │
  │ envp[0] ────────┼──→ "PATH=/bin"
  │ NULL            │
  │ 字符串数据...    │
  └─────────────────┘
  ```

**第 12 行**：`m.VFS_PM_FRAME_LEN = m_in.m_lc_pm_exec.framelen;`
- **是什么**：栈帧的总字节数

**第 13 行**：`m.VFS_PM_PS_STR = m_in.m_lc_pm_exec.ps_str;`
- **是什么**：ps_strings 结构的地址（用于 `ps` 命令显示进程信息）
- **为什么需要**：`ps` 命令需要读取进程的命令行参数，这个指针指向用户空间的 argv 区域

**第 15 行**：`tell_vfs(mp, &m);`
- **是什么**：异步发送消息给 VFS
- **实现**（来自 utility.c）：
  ```c
  void tell_vfs(rmp, m_ptr)
  struct mproc *rmp;
  message *m_ptr;
  {
    int r;
    if (rmp->mp_flags & (VFS_CALL | EVENT_CALL))
      panic("tell_vfs: not idle: %d", m_ptr->m_type);
    r = asynsend3(VFS_PROC_NR, m_ptr, AMF_NOREPLY);
    if (r != OK)
      panic("unable to send to VFS: %d", r);
    rmp->mp_flags |= VFS_CALL;
  }
  ```
- **关键点**：
  - `asynsend3`：异步发送，不等待回复
  - `AMF_NOREPLY`：告诉系统不需要立即回复
  - `mp->mp_flags |= VFS_CALL`：标记进程正在等待 VFS 回复

**第 17-18 行**：`return SUSPEND;`
- **是什么**：返回 `SUSPEND` 常量（值为 -1000 左右）
- **为什么**：告诉 PM 主循环**不要回复用户进程**，进程将被挂起，直到 VFS 发回 `VFS_PM_EXEC_REPLY`
- **流程**：
  ```
  用户进程调用 exec
       ↓
  PM 收到请求 (do_exec)
       ↓
  PM 发送 VFS_PM_EXEC 给 VFS
       ↓
  PM 返回 SUSPEND（不回复用户）
       ↓
  用户进程被挂起（在内核中等待）
       ↓
  VFS 加载程序，发送 VFS_PM_EXEC_REPLY
       ↓
  PM 收到回复 (exec_restart)
       ↓
  PM 通知内核完成 exec
       ↓
  用户进程从新程序的 main() 开始执行
  ```

---

### do_newexec 函数

```c
/*===========================================================================*
 *				do_newexec				     *
 *===========================================================================*/
int do_newexec(void)
{
	int proc_e, proc_n, allow_setuid;
	vir_bytes ptr;
	struct mproc *rmp;
	struct exec_info args;
	int r;

	if (who_e != VFS_PROC_NR && who_e != RS_PROC_NR)
		return EPERM;
```

**逐行解释**：

**第 1-10 行**：函数签名和变量声明
- **`proc_e`**：进程端点号（endpoint）
- **`proc_n`**：进程槽位号（slot number，0 到 NR_PROCS-1）
- **`allow_setuid`**：是否允许 setuid 执行（0=不允许，1=允许）
- **`ptr`**：`struct exec_info` 在调用者空间的地址
- **`rmp`**：指向目标进程的 `mproc` 结构指针
- **`args`**：`struct exec_info` 本地副本（栈上分配，约 100+ 字节）
- **`r`**：函数返回值/临时变量

**第 11-12 行**：权限检查
```c
if (who_e != VFS_PROC_NR && who_e != RS_PROC_NR)
    return EPERM;
```
- **是什么**：只允许 VFS 或 RS（重启服务）调用此函数
- **为什么**：这是内部接口，普通进程不能直接调用
- **`who_e`**：全局变量，消息发送者的端点号
- **`EPERM`**：错误码 "Operation not permitted"（值 1）

```c
	proc_e= m_in.m_lexec_pm_exec_new.endpt;
	if (pm_isokendpt(proc_e, &proc_n) != OK) {
		panic("do_newexec: got bad endpoint: %d", proc_e);
	}
	rmp= &mproc[proc_n];
	ptr= m_in.m_lexec_pm_exec_new.ptr;
	r= sys_datacopy(who_e, ptr, SELF, (vir_bytes)&args, sizeof(args));
	if (r != OK)
		panic("do_newexec: sys_datacopy failed: %d", r);
```

**第 13-21 行**：获取进程信息和复制 exec_info

**第 13 行**：`proc_e = m_in.m_lexec_pm_exec_new.endpt;`
- 从消息中提取目标进程的端点号

**第 14-16 行**：端点验证
```c
if (pm_isokendpt(proc_e, &proc_n) != OK) {
    panic("do_newexec: got bad endpoint: %d", proc_e);
}
```
- **`pm_isokendpt`**：验证端点号是否有效
- 实现逻辑：
  1. 从端点提取槽位号：`proc_n = _ENDPOINT_P(endpoint)`
  2. 检查槽位号范围：`0 <= proc_n < NR_PROCS`
  3. 检查端点匹配：`mproc[proc_n].mp_endpoint == endpoint`
  4. 检查进程在用：`mproc[proc_n].mp_flags & IN_USE`
- **`panic`**：如果验证失败，PM 崩溃（这是严重错误，不应该发生）

**第 17 行**：`rmp = &mproc[proc_n];`
- 获取目标进程的 `mproc` 结构指针

**第 18 行**：`ptr = m_in.m_lexec_pm_exec_new.ptr;`
- 获取 `struct exec_info` 在调用者（VFS/RS）空间的地址

**第 19-21 行**：复制 exec_info 结构
```c
r = sys_datacopy(who_e, ptr, SELF, (vir_bytes)&args, sizeof(args));
if (r != OK)
    panic("do_newexec: sys_datacopy failed: %d", r);
```
- **`sys_datacopy`**：内核系统调用，在进程间复制数据
- 参数：
  - `who_e`：源进程端点（VFS 或 RS）
  - `ptr`：源地址
  - `SELF`：目标进程是 PM 自己
  - `&args`：目标地址
  - `sizeof(args)`：复制字节数
- **内存操作**：
  ```
  VFS 空间:                    PM 空间:
  ┌──────────────┐             ┌──────────────┐
  │ exec_info    │ ──copy──→   │ args         │
  │ {            │             │ {            │
  │   proc_e,    │             │   proc_e,    │
  │   progname,  │             │   progname,  │
  │   new_uid,   │             │   new_uid,   │
  │   ...        │             │   ...        │
  │ }            │             │ }            │
  └──────────────┘             └──────────────┘
  ```

```c
	allow_setuid = 0;	/* Do not allow setuid execution */
	rmp->mp_flags &= ~TAINTED;	/* By default not tainted */

	if (rmp->mp_tracer == NO_TRACER) {
		/* Okay, setuid execution is allowed */
		allow_setuid = 1;
	}
```

**第 22-28 行**：setuid 权限检查

**第 22 行**：`allow_setuid = 0;`
- 默认不允许 setuid 执行

**第 23 行**：`rmp->mp_flags &= ~TAINTED;`
- 清除 TAINTED 标志（后面可能重新设置）
- **TAINTED 含义**：进程正在以特权身份运行（setuid/setgid）

**第 24-27 行**：检查是否被调试
```c
if (rmp->mp_tracer == NO_TRACER) {
    allow_setuid = 1;
}
```
- **关键安全规则**：如果进程被 ptrace 跟踪，**禁止 setuid 执行**
- **为什么**：防止攻击者通过 ptrace 附加到进程，然后利用 setuid 程序获取特权
- **生活类比**：就像银行不会让戴面具的人进入金库——如果进程被"跟踪"（可疑），就不给它特权

```c
	if (allow_setuid && args.allow_setuid) {
		rmp->mp_effuid = args.new_uid;
		rmp->mp_effgid = args.new_gid;
	}

	/* Always update the saved user and group ID at this point. */
	rmp->mp_svuid = rmp->mp_effuid;
	rmp->mp_svgid = rmp->mp_effgid;
```

**第 29-35 行**：更新用户/组 ID

**第 29-32 行**：setuid/setgid 处理
```c
if (allow_setuid && args.allow_setuid) {
    rmp->mp_effuid = args.new_uid;
    rmp->mp_effgid = args.new_gid;
}
```
- **`args.allow_setuid`**：VFS 检查文件 setuid/setgid 位后设置
- **`args.new_uid`/`new_gid`**：文件所有者的 uid/gid
- **更新有效 ID**：`mp_effuid` 和 `mp_effgid` 是进程的有效用户/组 ID
- **ID 类型**：
  | 字段 | 含义 | 来源 |
  |------|------|------|
  | `mp_realuid` | 真实 UID | 登录用户 |
  | `mp_effuid` | 有效 UID | 用于权限检查 |
  | `mp_svuid` | 保存的 UID | exec 时保存的 effuid |

**第 34-35 行**：保存 UID/GID
```c
rmp->mp_svuid = rmp->mp_effuid;
rmp->mp_svgid = rmp->mp_effgid;
```
- **为什么总是更新**：POSIX 要求 exec 时保存有效 ID，用于后续的 `setuid()` 系统调用

```c
	/* A process is considered 'tainted' when it's executing with
	 * setuid or setgid bit set, or when the real{u,g}id doesn't
	 * match the eff{u,g}id, respectively. */
	if (allow_setuid && args.allow_setuid) {
		/* Program has setuid and/or setgid bits set */
		rmp->mp_flags |= TAINTED;
	} else if (rmp->mp_effuid != rmp->mp_realuid ||
		   rmp->mp_effgid != rmp->mp_realgid) {
		rmp->mp_flags |= TAINTED;
	}
```

**第 36-45 行**：设置 TAINTED 标志

**翻译注释**：
> 当进程以 setuid 或 setgid 位执行，或当 real{u,g}id 与 eff{u,g}id 不匹配时，进程被认为是"被污染的"。

**两种 TAINTED 情况**：
1. **setuid/setgid 程序**：文件有 setuid 位，进程获得了额外特权
2. **ID 不匹配**：进程通过 `setuid()` 改变了有效 ID

**为什么需要 TAINTED 标志**：
- 安全审计：知道哪些进程运行在特权模式
- 调试：`ps` 命令可以显示进程是否被"污染"
- 未来扩展：可能限制被污染进程的某些操作

```c
	/* System will save command line for debugging, ps(1) output, etc. */
	strncpy(rmp->mp_name, args.progname, PROC_NAME_LEN-1);
	rmp->mp_name[PROC_NAME_LEN-1] = '\0';

	/* Save offset to initial argc (for procfs) */
	rmp->mp_frame_addr = (vir_bytes) args.stack_high - args.frame_len;
	rmp->mp_frame_len = args.frame_len;
```

**第 46-52 行**：保存进程名和栈帧信息

**第 47-48 行**：保存程序名
```c
strncpy(rmp->mp_name, args.progname, PROC_NAME_LEN-1);
rmp->mp_name[PROC_NAME_LEN-1] = '\0';
```
- **`PROC_NAME_LEN`**：通常是 16 字节
- **为什么用 strncpy**：防止缓冲区溢出
- **为什么手动加 '\0'**：strncpy 不保证以 null 结尾（当源字符串超过长度限制时）

**第 50-52 行**：保存栈帧地址
```c
rmp->mp_frame_addr = (vir_bytes) args.stack_high - args.frame_len;
rmp->mp_frame_len = args.frame_len;
```
- **`stack_high`**：栈的高地址（栈顶）
- **`frame_len`**：栈帧大小
- **计算栈底**：`stack_high - frame_len`
- **用途**：`/proc/[pid]/cmdline` 读取进程的命令行参数

```c
	/* Kill process if something goes wrong after this point. */
	rmp->mp_flags |= PARTIAL_EXEC;

	mp->mp_reply.m_pm_lexec_exec_new.suid = (allow_setuid && args.allow_setuid);

	return r;
}
```

**第 53-58 行**：设置 PARTIAL_EXEC 标志并返回

**第 54 行**：`rmp->mp_flags |= PARTIAL_EXEC;`
- **是什么**：标记进程处于"部分 exec"状态
- **为什么**：如果后续步骤失败，进程必须被杀死（因为旧程序映像已被清除）
- **灾难预演**：如果忘记设置这个标志，exec 失败后进程可能继续运行旧代码，但内存已被破坏！

**第 56 行**：设置回复消息
```c
mp->mp_reply.m_pm_lexec_exec_new.suid = (allow_setuid && args.allow_setuid);
```
- 告诉调用者（VFS）是否执行了 setuid

**第 58 行**：`return r;`
- 返回 OK（r 在前面被设置为 sys_datacopy 的返回值）

---

### do_execrestart 函数

```c
/*===========================================================================*
 *				do_execrestart				     *
 *===========================================================================*/
int do_execrestart(void)
{
	int proc_e, proc_n, result;
	struct mproc *rmp;
	vir_bytes pc, ps_str;

	if (who_e != RS_PROC_NR)
		return EPERM;

	proc_e = m_in.m_rs_pm_exec_restart.endpt;
	if (pm_isokendpt(proc_e, &proc_n) != OK) {
		panic("do_execrestart: got bad endpoint: %d", proc_e);
	}
	rmp = &mproc[proc_n];
	result = m_in.m_rs_pm_exec_restart.result;
	pc = m_in.m_rs_pm_exec_restart.pc;
	ps_str = m_in.m_rs_pm_exec_restart.ps_str;

	exec_restart(rmp, result, pc, rmp->mp_frame_addr, ps_str);

	return OK;
}
```

**逐行解释**：

**第 1-7 行**：函数签名和变量声明
- **`proc_e`**：进程端点号
- **`proc_n`**：进程槽位号
- **`result`**：exec 操作的结果（OK 或错误码）
- **`rmp`**：目标进程的 mproc 指针
- **`pc`**：程序计数器（入口点地址）
- **`ps_str`**：ps_strings 地址

**第 9-10 行**：权限检查
```c
if (who_e != RS_PROC_NR)
    return EPERM;
```
- 只允许 RS（重启服务）调用此函数
- RS 使用这个接口来重启系统服务

**第 12-22 行**：提取参数并调用 exec_restart
- 从消息中提取端点、结果、pc、ps_str
- 调用 `exec_restart` 完成最终操作

**第 24 行**：`return OK;`
- 返回 OK，表示消息已处理

---

### exec_restart 函数

```c
/*===========================================================================*
 *				exec_restart				     *
 *===========================================================================*/
void exec_restart(struct mproc *rmp, int result, vir_bytes pc, vir_bytes sp,
       vir_bytes ps_str)
{
	int r, sn;

	if (result != OK)
	{
		if (rmp->mp_flags & PARTIAL_EXEC)
		{
			/* Use SIGKILL to signal that something went wrong */
			sys_kill(rmp->mp_endpoint, SIGKILL);
			return;
		}
		reply(rmp-mproc, result);
		return;
	}
```

**逐行解释**：

**第 1-5 行**：函数签名
- **参数**：
  - `rmp`：目标进程的 mproc 指针
  - `result`：exec 结果（OK 或错误码）
  - `pc`：程序入口点
  - `sp`：栈指针
  - `ps_str`：ps_strings 地址

**第 7-17 行**：错误处理

**第 8 行**：`if (result != OK)`
- 如果 exec 失败，进入错误处理分支

**第 10-14 行**：PARTIAL_EXEC 情况
```c
if (rmp->mp_flags & PARTIAL_EXEC)
{
    sys_kill(rmp->mp_endpoint, SIGKILL);
    return;
}
```
- **是什么**：如果进程处于"部分 exec"状态，发送 SIGKILL 杀死进程
- **为什么**：PARTIAL_EXEC 意味着旧程序已被清除，新程序加载失败，进程无法恢复
- **`sys_kill`**：内核系统调用，向进程发送信号

**第 15-16 行**：非 PARTIAL_EXEC 情况
```c
reply(rmp-mproc, result);
return;
```
- 如果不是 PARTIAL_EXEC，回复错误码给进程
- 进程可以继续运行（exec 失败但不影响原有程序）

```c
	rmp->mp_flags &= ~PARTIAL_EXEC;

	/* Fix 'mproc' fields, tell kernel that exec is done, reset caught
	 * sigs.
	 */
	for (sn = 1; sn < _NSIG; sn++) {
		if (sigismember(&rmp->mp_catch, sn)) {
			sigdelset(&rmp->mp_catch, sn);
			rmp->mp_sigact[sn].sa_handler = SIG_DFL;
			sigemptyset(&rmp->mp_sigact[sn].sa_mask);
		}
	}
```

**第 18-30 行**：exec 成功的处理

**第 18 行**：`rmp->mp_flags &= ~PARTIAL_EXEC;`
- 清除 PARTIAL_EXEC 标志

**第 20-29 行**：重置信号处理
```c
for (sn = 1; sn < _NSIG; sn++) {
    if (sigismember(&rmp->mp_catch, sn)) {
        sigdelset(&rmp->mp_catch, sn);
        rmp->mp_sigact[sn].sa_handler = SIG_DFL;
        sigemptyset(&rmp->mp_sigact[sn].sa_mask);
    }
}
```
- **POSIX 规则**：exec 后，捕获的信号恢复为默认处理
- **`_NSIG`**：信号总数（通常是 64）
- **`mp_catch`**：信号捕获掩码（记录哪些信号被捕获）
- **`SIG_DFL`**：默认信号处理函数
- **不被重置的信号**：
  - 被忽略的信号（SIG_IGN）保持忽略
  - 被阻塞的信号保持阻塞

**内存模型**：
```
信号处理数组 mp_sigact[64]:
┌──────────────────────────────────────────────────┐
│ [0]  未使用                                        │
│ [1]  SIGHUP   → SIG_DFL (如果之前被捕获)           │
│ [2]  SIGINT   → SIG_DFL                           │
│ [3]  SIGQUIT  → SIG_DFL                           │
│ [9]  SIGKILL  → SIG_DFL (不能被捕获，无需重置)      │
│ ...                                               │
│ [11] SIGSEGV  → SIG_DFL                           │
│ ...                                               │
│ [15] SIGTERM  → SIG_DFL                           │
│ ...                                               │
└──────────────────────────────────────────────────┘
```

```c
	/* Cause a signal if this process is traced.
	 * Do this before making the process runnable again!
	 */
	if (rmp->mp_tracer != NO_TRACER && !(rmp->mp_trace_flags & TO_NOEXEC))
	{
		sn = (rmp->mp_trace_flags & TO_ALTEXEC) ? SIGSTOP : SIGTRAP;

		check_sig(rmp->mp_pid, sn, FALSE /* ksig */);
	}
```

**第 31-40 行**：ptrace 支持

**翻译注释**：
> 如果进程被跟踪，产生一个信号。必须在进程再次可运行之前执行此操作！

**第 35 行**：检查是否被跟踪
```c
if (rmp->mp_tracer != NO_TRACER && !(rmp->mp_trace_flags & TO_NOEXEC))
```
- **`mp_tracer`**：跟踪者的进程槽位号
- **`TO_NOEXEC`**：标志位，表示不在 exec 时通知跟踪者

**第 37 行**：选择信号类型
```c
sn = (rmp->mp_trace_flags & TO_ALTEXEC) ? SIGSTOP : SIGTRAP;
```
- **`TO_ALTEXEC`**：使用 SIGSTOP 而非 SIGTRAP
- **SIGTRAP**：调试器通常用这个信号在 exec 后暂停
- **SIGSTOP**：无条件停止进程

**第 39 行**：`check_sig(rmp->mp_pid, sn, FALSE);`
- 发送信号给进程

```c
	/* Call kernel to exec with SP and PC set by VFS. */
	r = sys_exec(rmp->mp_endpoint, sp, (vir_bytes)rmp->mp_name, pc, ps_str);
	if (r != OK) panic("sys_exec failed: %d", r);
}
```

**第 41-45 行**：通知内核完成 exec

**第 42 行**：`sys_exec(...)`
- **是什么**：内核系统调用，完成 exec 的最后一步
- **参数**：
  - `rmp->mp_endpoint`：进程端点
  - `sp`：新栈指针
  - `rmp->mp_name`：程序名（用于内核调试）
  - `pc`：程序入口点
  - `ps_str`：ps_strings 地址

**sys_exec 实现**（来自 lib/libsys/sys_exec.c）：
```c
int sys_exec(endpoint_t proc_ep, vir_bytes stack_ptr, vir_bytes progname,
    vir_bytes pc, vir_bytes ps_str)
{
    message m;
    m.m_lsys_krn_sys_exec.endpt = proc_ep;
    m.m_lsys_krn_sys_exec.stack = stack_ptr;
    m.m_lsys_krn_sys_exec.name = progname;
    m.m_lsys_krn_sys_exec.ip = pc;
    m.m_lsys_krn_sys_exec.ps_str = ps_str;
    return _kernel_call(SYS_EXEC, &m);
}
```

**内核做什么**：
1. 设置进程的程序计数器（PC）为新入口点
2. 设置栈指针（SP）为新栈顶
3. 清除旧的内存映射
4. 设置新的内存映射（代码段、数据段、栈）
5. 进程下次调度时从新程序开始执行

---

## 整体设计思路

### exec 流程图

```
┌─────────────────────────────────────────────────────────────────┐
│                     exec 完整流程                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  用户进程                                                        │
│  ┌──────────────────┐                                           │
│  │ execve("/bin/ls")│                                           │
│  └────────┬─────────┘                                           │
│           │ 系统调用                                             │
│           ↓                                                     │
│  ┌──────────────────┐                                           │
│  │      内核        │                                           │
│  │  (消息传递)       │                                           │
│  └────────┬─────────┘                                           │
│           │                                                     │
│           ↓                                                     │
│  ┌──────────────────┐                                           │
│  │   PM: do_exec    │                                           │
│  │  1. 构造消息      │                                           │
│  │  2. 发送给 VFS   │                                           │
│  │  3. 返回 SUSPEND │                                           │
│  └────────┬─────────┘                                           │
│           │ 异步消息                                             │
│           ↓                                                     │
│  ┌──────────────────┐                                           │
│  │      VFS         │                                           │
│  │  1. 打开文件      │                                           │
│  │  2. 读取 ELF 头  │                                           │
│  │  3. 分配内存      │                                           │
│  │  4. 加载段       │                                           │
│  │  5. 设置栈       │                                           │
│  └────────┬─────────┘                                           │
│           │                                                     │
│           ↓                                                     │
│  ┌──────────────────┐                                           │
│  │ PM: do_newexec   │                                           │
│  │  1. 更新 UID/GID │                                           │
│  │  2. 设置 TAINTED │                                           │
│  │  3. 保存进程名    │                                           │
│  └────────┬─────────┘                                           │
│           │                                                     │
│           ↓                                                     │
│  ┌──────────────────┐                                           │
│  │ VFS: 回复 PM     │                                           │
│  │ (VFS_PM_EXEC_REPLY)                                          │
│  └────────┬─────────┘                                           │
│           │                                                     │
│           ↓                                                     │
│  ┌──────────────────┐                                           │
│  │PM: exec_restart  │                                           │
│  │  1. 重置信号      │                                           │
│  │  2. 通知内核      │                                           │
│  └────────┬─────────┘                                           │
│           │                                                     │
│           ↓                                                     │
│  ┌──────────────────┐                                           │
│  │     内核         │                                           │
│  │  sys_exec 完成   │                                           │
│  │  设置 PC/SP      │                                           │
│  └────────┬─────────┘                                           │
│           │                                                     │
│           ↓                                                     │
│  ┌──────────────────┐                                           │
│  │ 用户进程恢复执行  │                                           │
│  │ 从 ls 的 main()  │                                           │
│  │ 开始运行         │                                           │
│  └──────────────────┘                                           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 为什么这样设计？

1. **微内核分离**：
   - PM 只管理进程元数据（PID、UID、信号等）
   - VFS 负责文件系统操作（打开文件、读取数据）
   - 内核负责底层资源（内存映射、寄存器设置）

2. **异步消息传递**：
   - exec 是耗时操作（可能需要从磁盘读取大文件）
   - 使用异步消息避免阻塞 PM 主循环
   - PM 可以同时处理其他请求

3. **安全检查分层**：
   - VFS 检查文件权限和 setuid 位
   - PM 检查 ptrace 状态（防止 setuid 提权攻击）
   - 内核确保内存隔离

---

## Rust 实现与对比

### 核心结构 Rust 化

```rust
#![no_std]
#![feature(never_type)]

use core::mem::size_of;

pub const PROC_NAME_LEN: usize = 16;
pub const _NSIG: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ExecResult {
    Ok = 0,
    Suspend = -1000,
    Eperm = 1,
    Einval = 22,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct ExecInfo {
    pub proc_e: i32,
    pub progname: [u8; PROC_NAME_LEN],
    pub new_uid: u32,
    pub new_gid: u32,
    pub allow_setuid: i32,
    pub stack_size: usize,
    pub load_offset: usize,
    pub text_size: usize,
    pub data_size: usize,
    pub filesize: u64,
    pub load_base: usize,
    pub pc: usize,
    pub stack_high: usize,
    pub frame_len: usize,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Default)]
    pub struct ProcFlags: u32 {
        const IN_USE = 0x001;
        const ZOMBIE = 0x002;
        const VFS_CALL = 0x004;
        const EVENT_CALL = 0x008;
        const PARTIAL_EXEC = 0x010;
        const TAINTED = 0x020;
        const EXITING = 0x040;
        const PRIV_PROC = 0x080;
    }
}

#[derive(Clone, Copy, Default)]
pub struct SigAction {
    pub sa_handler: usize,
    pub sa_mask: u64,
    pub sa_flags: i32,
}

pub const NO_TRACER: i32 = -1;
pub const SIG_DFL: usize = 0;

pub struct MProc {
    pub mp_flags: ProcFlags,
    pub mp_endpoint: i32,
    pub mp_pid: u32,
    pub mp_parent: i32,
    pub mp_tracer: i32,
    pub mp_trace_flags: u32,
    pub mp_realuid: u32,
    pub mp_effuid: u32,
    pub mp_svuid: u32,
    pub mp_realgid: u32,
    pub mp_effgid: u32,
    pub mp_svgid: u32,
    pub mp_name: [u8; PROC_NAME_LEN],
    pub mp_frame_addr: usize,
    pub mp_frame_len: usize,
    pub mp_catch: u64,
    pub mp_sigact: [SigAction; _NSIG],
}

impl MProc {
    pub const fn new() -> Self {
        Self {
            mp_flags: ProcFlags::empty(),
            mp_endpoint: 0,
            mp_pid: 0,
            mp_parent: 0,
            mp_tracer: NO_TRACER,
            mp_trace_flags: 0,
            mp_realuid: 0,
            mp_effuid: 0,
            mp_svuid: 0,
            mp_realgid: 0,
            mp_effgid: 0,
            mp_svgid: 0,
            mp_name: [0; PROC_NAME_LEN],
            mp_frame_addr: 0,
            mp_frame_len: 0,
            mp_catch: 0,
            mp_sigact: [SigAction::default(); _NSIG],
        }
    }
}
```

### do_exec Rust 实现

```rust
use core::mem::MaybeUninit;

pub const VFS_PROC_NR: i32 = 3;
pub const RS_PROC_NR: i32 = 4;

#[derive(Clone, Copy)]
#[repr(C)]
pub struct Message {
    pub m_type: i32,
    pub data: [u8; 60],
}

impl Default for Message {
    fn default() -> Self {
        Self {
            m_type: 0,
            data: [0; 60],
        }
    }
}

#[derive(Debug)]
pub enum PmError {
    InvalidEndpoint,
    NotPermitted,
    DataCopyFailed(i32),
    SysExecFailed(i32),
}

pub fn do_exec(
    mp: &MProc,
    m_in: &Message,
) -> Result<!, PmError> {
    let mut m = Message::default();
    
    m.m_type = VFS_PM_EXEC;
    m.set_vfs_pm_endpt(mp.mp_endpoint);
    m.set_vfs_pm_path(m_in.get_lc_pm_exec_name());
    m.set_vfs_pm_path_len(m_in.get_lc_pm_exec_namelen());
    m.set_vfs_pm_frame(m_in.get_lc_pm_exec_frame());
    m.set_vfs_pm_frame_len(m_in.get_lc_pm_exec_framelen());
    m.set_vfs_pm_ps_str(m_in.get_lc_pm_exec_ps_str());
    
    tell_vfs(mp, &m)?;
    
    Err(PmError::Suspend)
}

fn tell_vfs(rmp: &MProc, m_ptr: &Message) -> Result<(), PmError> {
    if rmp.mp_flags.intersects(ProcFlags::VFS_CALL | ProcFlags::EVENT_CALL) {
        panic!("tell_vfs: not idle");
    }
    
    asynsend3(VFS_PROC_NR, m_ptr, AMF_NOREPLY)?;
    
    rmp.mp_flags.insert(ProcFlags::VFS_CALL);
    Ok(())
}
```

### do_newexec Rust 实现

```rust
pub fn do_newexec(
    who_e: i32,
    m_in: &Message,
    mproc: &mut [MProc],
) -> Result<bool, PmError> {
    if who_e != VFS_PROC_NR && who_e != RS_PROC_NR {
        return Err(PmError::NotPermitted);
    }
    
    let proc_e = m_in.get_lexec_pm_exec_new_endpt();
    let proc_n = pm_isokendpt(proc_e, mproc)?;
    let rmp = &mut mproc[proc_n];
    
    let ptr = m_in.get_lexec_pm_exec_new_ptr();
    let args: ExecInfo = sys_datacopy_struct(who_e, ptr)?;
    
    let mut allow_setuid = false;
    rmp.mp_flags.remove(ProcFlags::TAINTED);
    
    if rmp.mp_tracer == NO_TRACER {
        allow_setuid = true;
    }
    
    if allow_setuid && args.allow_setuid != 0 {
        rmp.mp_effuid = args.new_uid;
        rmp.mp_effgid = args.new_gid;
    }
    
    rmp.mp_svuid = rmp.mp_effuid;
    rmp.mp_svgid = rmp.mp_effgid;
    
    if (allow_setuid && args.allow_setuid != 0) ||
       (rmp.mp_effuid != rmp.mp_realuid || rmp.mp_effgid != rmp.mp_realgid) {
        rmp.mp_flags.insert(ProcFlags::TAINTED);
    }
    
    let name_len = args.progname.len().min(PROC_NAME_LEN - 1);
    rmp.mp_name[..name_len].copy_from_slice(&args.progname[..name_len]);
    rmp.mp_name[name_len] = 0;
    
    rmp.mp_frame_addr = args.stack_high.wrapping_sub(args.frame_len);
    rmp.mp_frame_len = args.frame_len;
    
    rmp.mp_flags.insert(ProcFlags::PARTIAL_EXEC);
    
    Ok(allow_setuid && args.allow_setuid != 0)
}

fn pm_isokendpt(endpoint: i32, mproc: &[MProc]) -> Result<usize, PmError> {
    let proc = _ENDPOINT_P(endpoint) as usize;
    if proc >= mproc.len() {
        return Err(PmError::InvalidEndpoint);
    }
    if mproc[proc].mp_endpoint != endpoint {
        return Err(PmError::InvalidEndpoint);
    }
    if !mproc[proc].mp_flags.contains(ProcFlags::IN_USE) {
        return Err(PmError::InvalidEndpoint);
    }
    Ok(proc)
}
```

### exec_restart Rust 实现

```rust
pub fn exec_restart(
    rmp: &mut MProc,
    result: i32,
    pc: usize,
    sp: usize,
    ps_str: usize,
) -> Result<(), PmError> {
    if result != 0 {
        if rmp.mp_flags.contains(ProcFlags::PARTIAL_EXEC) {
            sys_kill(rmp.mp_endpoint, Signal::Sigkill)?;
            return Ok(());
        }
        return Err(PmError::from_errno(result));
    }
    
    rmp.mp_flags.remove(ProcFlags::PARTIAL_EXEC);
    
    for sn in 1.._NSIG {
        if sigismember(rmp.mp_catch, sn) {
            sigdelset(&mut rmp.mp_catch, sn);
            rmp.mp_sigact[sn].sa_handler = SIG_DFL;
            rmp.mp_sigact[sn].sa_mask = 0;
        }
    }
    
    if rmp.mp_tracer != NO_TRACER && (rmp.mp_trace_flags & TO_NOEXEC) == 0 {
        let sig = if (rmp.mp_trace_flags & TO_ALTEXEC) != 0 {
            Signal::Sigstop
        } else {
            Signal::Sigtrap
        };
        check_sig(rmp.mp_pid, sig, false)?;
    }
    
    sys_exec(
        rmp.mp_endpoint,
        sp,
        rmp.mp_name.as_ptr() as usize,
        pc,
        ps_str,
    )?;
    
    Ok(())
}

enum Signal {
    Sigkill = 9,
    Sigstop = 19,
    Sigtrap = 5,
}

fn sys_exec(
    proc_ep: i32,
    stack_ptr: usize,
    progname: usize,
    pc: usize,
    ps_str: usize,
) -> Result<(), PmError> {
    let mut m = Message::default();
    m.m_type = SYS_EXEC;
    m.set_endpt(proc_ep);
    m.set_stack(stack_ptr);
    m.set_name(progname);
    m.set_ip(pc);
    m.set_ps_str(ps_str);
    
    kernel_call(&m)?;
    Ok(())
}
```

### Rust vs C 对比

| 特性 | C 实现 | Rust 实现 |
|------|--------|-----------|
| **错误处理** | 返回 int 错误码，容易被忽略 | `Result<T, E>` 强制处理 |
| **内存安全** | 手动管理，可能越界 | 编译时检查，运行时边界检查 |
| **标志位** | 位运算 `&= ~FLAG` | `bitflags!` 宏，类型安全 |
| **空指针** | NULL 可能导致崩溃 | `Option<T>` 强制检查 |
| **并发安全** | 无保护 | `Send`/`Sync` trait |
| **初始化** | 可能忘记初始化 | `Default` trait 保证初始化 |

### unsafe 使用说明

在内核代码中，某些操作必须使用 `unsafe`：

```rust
unsafe fn sys_datacopy_struct<T: Copy>(src_ep: i32, src_addr: usize) -> Result<T, PmError> {
    let mut dest: MaybeUninit<T> = MaybeUninit::uninit();
    let r = sys_datacopy_raw(
        src_ep,
        src_addr,
        SELF,
        dest.as_mut_ptr() as usize,
        size_of::<T>(),
    );
    if r != 0 {
        return Err(PmError::DataCopyFailed(r));
    }
    Ok(dest.assume_init())
}
```

**unsafe 保证**：
1. `src_addr` 必须是有效的用户空间地址
2. 调用者必须确保 `src_ep` 进程的内存可读
3. `size_of::<T>()` 不能超过允许的复制大小

---

## 要点总结

1. **exec 是异步多阶段操作**：PM 转发给 VFS，VFS 加载程序后通知 PM，PM 最终通知内核
2. **setuid 安全检查**：被 ptrace 跟踪的进程禁止 setuid 执行，防止提权攻击
3. **PARTIAL_EXEC 保护**：如果 exec 在中间步骤失败，进程必须被杀死，因为旧程序已被清除

---

## 灾难预演

**如果删除 `rmp->mp_flags |= PARTIAL_EXEC;` 这一行会怎样？**

场景：用户执行一个损坏的可执行文件
1. VFS 开始加载程序，分配内存
2. 加载失败（文件损坏）
3. VFS 通知 PM 失败
4. `exec_restart` 检查 `PARTIAL_EXEC`，发现未设置
5. PM 回复错误给进程，进程继续运行
6. **灾难**：进程的内存已被部分覆盖，但进程还在运行旧代码！
7. 结果：段错误、数据损坏、安全漏洞

**如果删除 `if (rmp->mp_tracer == NO_TRACER)` 检查会怎样？**

场景：攻击者用 ptrace 附加到 setuid 程序
1. 攻击者附加到 `/usr/bin/passwd`（setuid root）
2. passwd 执行 exec
3. 没有 tracer 检查，allow_setuid = true
4. 攻击者获得 root 权限
5. **灾难**：本地提权漏洞

---

## 互动自测

1. **问题**：为什么 `do_exec` 返回 `SUSPEND` 而不是直接等待 VFS 回复？
   **答案**：PM 是单线程服务器，如果阻塞等待，其他进程的系统调用都会被阻塞。返回 SUSPEND 让进程挂起，PM 主循环可以继续处理其他请求。

2. **问题**：`PARTIAL_EXEC` 标志在什么时候设置？什么时候清除？
   **答案**：在 `do_newexec` 中设置（此时旧程序资源已开始释放），在 `exec_restart` 成功时清除。如果 exec 失败且 PARTIAL_EXEC 已设置，进程必须被杀死。

3. **问题**：为什么 exec 后要重置被捕获的信号？
   **答案**：POSIX 规定。新程序可能不知道旧程序安装了什么信号处理函数，如果保留会导致不可预期的行为。但被忽略的信号保持忽略，因为这是显式的安全策略。
