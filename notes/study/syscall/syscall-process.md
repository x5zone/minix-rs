# 进程管理系统调用

> **核心文件**: 
> - `minix/kernel/system/do_fork.c` - 进程创建
> - `minix/kernel/system/do_exec.c` - 进程映像替换
> - `minix/kernel/system/do_exit.c` - 进程退出
> - `minix/kernel/system/do_clear.c` - 进程清理
> - `minix/kernel/system/do_kill.c` - 信号发送
> - `minix/kernel/system/do_mcontext.c` - 机器上下文管理
> 
> **相关头文件**:
> - `minix/kernel/proc.h` - 进程表结构定义
> - `minix/include/minix/endpoint.h` - 端点号机制
> - `minix/kernel/priv.h` - 特权结构定义

---

## 一、功能概述（What）

进程管理模块负责进程的完整生命周期管理，包括：

| 系统调用 | 功能 | 调用者 |
|---------|------|--------|
| `SYS_FORK` | 创建子进程，复制父进程的 PCB | PM（进程管理器） |
| `SYS_EXEC` | 替换进程映像，设置新的执行环境 | PM |
| `SYS_EXIT` | 进程退出，发送 SIGABRT 信号 | 系统进程 |
| `SYS_CLEAR` | 清理进程槽位，释放资源 | PM |
| `SYS_KILL` | 向进程发送信号 | PM, VFS 等服务 |
| `SYS_GETMCONTEXT` | 获取进程机器上下文 | 用户态线程库 |
| `SYS_SETMCONTEXT` | 设置进程机器上下文 | 用户态线程库 |

---

## 二、设计动机（Why）

### 2.1 微内核架构的进程管理

Minix3 采用微内核架构，进程管理职责分离：

```
┌─────────────────────────────────────────────────────────────────────┐
│                    进程管理职责分离                                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  用户进程                                                            │
│      │                                                              │
│      │ fork(), exec(), exit()                                       │
│      ▼                                                              │
│  PM（进程管理器）                                                    │
│      │                                                              │
│      │ SYS_FORK, SYS_EXEC, SYS_CLEAR                                │
│      ▼                                                              │
│  内核                                                                │
│      │                                                              │
│      ├── PCB 管理（proc 结构体）                                    │
│      ├── 端点号分配（generation 机制）                              │
│      ├── FPU 状态管理                                               │
│      └── 资源清理（IRQ 钩子、定时器等）                             │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

**职责划分**：
- **PM**：进程槽位分配、进程树维护、资源配额管理
- **内核**：PCB 操作、端点号管理、硬件状态保存/恢复

### 2.2 端点代数机制

**问题**：进程退出后，其他进程可能还保存着旧的端点号引用。

**解决方案**：每个端点号包含代数（generation），进程槽位重用时递增代数。

```
端点号结构（32 bits）:
┌─────────────────────────────────────────────────────────────┐
│   代数 (generation)  │      槽位编号 (nr)       │
│   高 17 bits         │        低 15 bits        │
└─────────────────────────────────────────────────────────────┘

定义位置：minix/include/minix/endpoint.h:45-67

#define _ENDPOINT_GENERATION_SHIFT	15
#define _ENDPOINT_GENERATION_SIZE	(1 << _ENDPOINT_GENERATION_SHIFT)
#define _ENDPOINT_MAX_GENERATION	(INT_MAX/_ENDPOINT_GENERATION_SIZE-1)

#define _ENDPOINT(g, p) \
	((endpoint_t)(((g) << _ENDPOINT_GENERATION_SHIFT) + (p)))
#define _ENDPOINT_G(e) (((e)+MAX_NR_TASKS) >> _ENDPOINT_GENERATION_SHIFT)
#define _ENDPOINT_P(e) \
	((((e)+MAX_NR_TASKS) & (_ENDPOINT_GENERATION_SIZE - 1)) - MAX_NR_TASKS)
```

**示例**：
```
时间线：
T1: 进程 A 占用槽位 5，端点 = _ENDPOINT(1, 5) = 0x00008005
T2: 进程 A 退出，其他进程可能还保存着 0x00008005
T3: fork 创建子进程，占用槽位 5
    gen = 旧代数 + 1 = 2
    新端点 = _ENDPOINT(2, 5) = 0x00010005
    结果：旧引用 0x00008005 不会指向新进程
```

---

## 三、使用场景（When）

### 3.1 fork 系统调用流程

```
用户进程调用 fork()
    │
    ├─► libc 封装
    │       └─► 发送消息给 PM
    │
    ├─► PM 处理
    │       ├─► 分配子进程槽位
    │       ├─► 调用 VM 创建新页表
    │       └─► 发送 SYS_FORK 给内核
    │
    └─► 内核 do_fork()
            ├─► 验证父进程状态
            ├─► 复制 PCB
            ├─► 递增端点代数
            ├─► 初始化子进程状态
            └─► 返回子进程端点号
```

### 3.2 exec 系统调用流程

```
用户进程调用 execve()
    │
    ├─► libc 封装
    │       └─► 发送消息给 PM
    │
    ├─► PM 处理
    │       ├─► 加载新程序映像
    │       ├─► 设置新的栈和指令指针
    │       └─► 发送 SYS_EXEC 给内核
    │
    └─► 内核 do_exec()
            ├─► 设置新的 IP/SP
            ├─► 更新进程名
            ├─► 重置 FPU 状态
            └─► 清除接收状态
```

### 3.3 进程退出流程

```
用户进程调用 exit()
    │
    ├─► libc 封装
    │       └─► 发送消息给 PM
    │
    ├─► PM 处理
    │       ├─► 标记进程为 ZOMBIE
    │       ├─► 通知父进程
    │       └─► 发送 SYS_CLEAR 给内核
    │
    └─► 内核 do_clear()
            ├─► 释放地址空间
            ├─► 移除 IRQ 钩子
            ├─► 清除端点
            ├─► 释放 FPU
            └─► 标记槽位为 FREE
```

---

## 四、核心数据结构

### 4.1 进程表结构（struct proc）

**定义位置**：`minix/kernel/proc.h:22-138`

```c
struct proc {
  struct stackframe_s p_reg;	/* 进程寄存器保存区 */
  struct segframe p_seg;	/* 段描述符 */
  proc_nr_t p_nr;		/* 进程槽位编号 */
  struct priv *p_priv;		/* 特权结构指针 */
  volatile u32_t p_rts_flags;	/* 运行状态标志 */
  volatile u32_t p_misc_flags;	/* 杂项标志 */

  char p_priority;		/* 当前优先级 */
  u64_t p_cpu_time_left;	/* 剩余 CPU 时间 */
  unsigned p_quantum_size_ms;	/* 时间片大小（毫秒）*/
  struct proc *p_scheduler;	/* 调度器进程 */
  unsigned p_cpu;		/* 运行在哪个 CPU */

  /* 统计信息 */
  clock_t p_user_time;		/* 用户态时间 */
  clock_t p_sys_time;		/* 内核态时间 */
  clock_t p_virt_left;		/* 虚拟定时器剩余时间 */
  clock_t p_prof_left;		/* 统计定时器剩余时间 */

  u64_t p_cycles;		/* 使用的 CPU 周期数 */
  u64_t p_kcall_cycles;		/* 内核调用周期数 */
  u64_t p_kipc_cycles;		/* IPC 周期数 */

  /* 调度队列 */
  struct proc *p_nextready;	/* 下一个就绪进程 */
  struct proc *p_caller_q;	/* 发送者队列头 */
  struct proc *p_q_link;	/* 发送者队列链接 */

  /* IPC 相关 */
  endpoint_t p_getfrom_e;	/* 从谁接收 */
  endpoint_t p_sendto_e;	/* 发送给谁 */
  sigset_t p_pending;		/* 待处理信号 */

  char p_name[PROC_NAME_LEN];	/* 进程名 */
  endpoint_t p_endpoint;	/* 端点号 */

  message p_sendmsg;		/* 发送的消息 */
  message p_delivermsg;		/* 待投递的消息 */
  vir_bytes p_delivermsg_vir;	/* 消息缓冲区地址 */

  /* VM 请求相关 */
  struct {
	struct proc *nextrestart;
	struct proc *nextrequestor;
	int type;
	union ixfer_saved {
		message reqmsg;
	} saved;
	int req_type;
	endpoint_t target;
	union ixfer_params {
		struct {
			vir_bytes start, length;
			u8_t writeflag;
		} check;
	} params;
	int vmresult;
  } p_vmrequest;

  int p_found;			/* 一致性检查 */
  int p_magic;			/* 有效性检查 */

  struct { reg_t r1, r2, r3; } p_defer;	/* 延迟 IPC 参数 */
};
```

**内存布局**：
```
struct proc 大小约 1KB
┌─────────────────────────────────────────────────────────────┐
│ p_reg (寄存器)     │ ~200 字节                              │
│ p_seg (段信息)     │ ~100 字节                              │
│ p_nr, p_priv      │ 16 字节                                │
│ p_rts_flags       │ 4 字节                                 │
│ p_misc_flags      │ 4 字节                                 │
│ p_priority        │ 1 字节                                 │
│ ...               │                                        │
│ p_name            │ 16 字节                                │
│ p_endpoint        │ 4 字节                                 │
│ p_sendmsg         │ 64 字节                                │
│ p_delivermsg      │ 64 字节                                │
│ ...               │                                        │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 运行状态标志（RTS flags）

**定义位置**：`minix/kernel/proc.h:142-166`

```c
#define RTS_SLOT_FREE	0x01	/* 进程槽位空闲 */
#define RTS_PROC_STOP	0x02	/* 进程已停止 */
#define RTS_SENDING	0x04	/* 进程阻塞在发送 */
#define RTS_RECEIVING	0x08	/* 进程阻塞在接收 */
#define RTS_SIGNALED	0x10	/* 新信号到达 */
#define RTS_SIG_PENDING	0x20	/* 信号正在处理 */
#define RTS_P_STOP	0x40	/* 进程被跟踪 */
#define RTS_NO_PRIV	0x80	/* fork 的系统进程无特权 */
#define RTS_NO_ENDPOINT	0x100	/* 进程无法发送/接收消息 */
#define RTS_VMINHIBIT	0x200	/* 等待 VM 设置页表 */
#define RTS_PAGEFAULT	0x400	/* 进程有未处理的页错误 */
#define RTS_VMREQUEST	0x800	/* VM 内存请求发起者 */
#define RTS_VMREQTARGET	0x1000	/* VM 内存请求目标 */
#define RTS_PREEMPTED	0x4000	/* 被高优先级进程抢占 */
#define RTS_NO_QUANTUM	0x8000	/* 时间片用完 */
#define RTS_BOOTINHIBIT	0x10000	/* 等待 VM 初始化 */
```

**关键规则**：进程可运行当且仅当 `p_rts_flags == 0`。

```c
#define rts_f_is_runnable(flg)	((flg) == 0)
#define proc_is_runnable(p)	(rts_f_is_runnable((p)->p_rts_flags))
```

### 4.3 杂项标志（Misc flags）

**定义位置**：`minix/kernel/proc.h:234-262`

```c
#define MF_REPLY_PEND	0x001	/* IPC_REQUEST 回复待处理 */
#define MF_VIRT_TIMER	0x002	/* 虚拟定时器运行中 */
#define MF_PROF_TIMER	0x004	/* 统计定时器运行中 */
#define MF_KCALL_RESUME 0x008	/* 内核调用被中断，需恢复 */
#define MF_DELIVERMSG	0x040	/* 运行前需复制消息 */
#define MF_SIG_DELAY	0x080	/* 不再发送时发送信号 */
#define MF_SC_ACTIVE	0x100	/* 系统调用跟踪：正在调用中 */
#define MF_SC_DEFER	0x200	/* 系统调用跟踪：延迟调用 */
#define MF_SC_TRACE	0x400	/* 系统调用跟踪：触发事件 */
#define MF_FPU_INITIALIZED	0x1000  /* FPU 已使用 */
#define MF_SENDING_FROM_KERNEL	0x2000 /* 消息来自内核 */
#define MF_CONTEXT_SET	0x4000	/* 不修改上下文 */
#define MF_SPROF_SEEN	0x8000	/* 性能分析已看到此进程 */
#define MF_FLUSH_TLB	0x10000	/* 需刷新 TLB */
#define MF_SENDA_VM_MISS 0x20000	/* 异步消息 VM 缺失 */
#define MF_STEP		0x40000	/* 单步执行 */
#define MF_MSGFAILED	0x80000	/* 消息失败 */
#define MF_NICED	0x100000	/* 用户降低了优先级 */
```

---

## 五、核心流程

### 5.1 fork 流程图

```
┌─────────────────────────────────────────────────────────────────────┐
│                         fork 流程                                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  PM 发送 SYS_FORK 消息                                              │
│      │                                                              │
│      ▼                                                              │
│  do_fork() 入口                                                     │
│      │                                                              │
│      ├─► 参数验证                                                   │
│      │       ├─► isokendpt(endpt, &p_proc)                         │
│      │       ├─► isemptyp(rpp) - 父进程槽非空                      │
│      │       ├─► !isemptyp(rpc) - 子进程槽为空                     │
│      │       └─► RTS_ISSET(rpp, RTS_RECEIVING) - 同步 fork         │
│      │                                                              │
│      ├─► FPU 状态保存                                               │
│      │       └─► save_fpu(rpp)                                     │
│      │                                                              │
│      ├─► PCB 复制                                                   │
│      │       ├─► gen = _ENDPOINT_G(rpc->p_endpoint)                │
│      │       ├─► *rpc = *rpp (整个结构体复制)                       │
│      │       └─► 恢复子进程的 FPU 保存区指针                        │
│      │                                                              │
│      ├─► 端点代数递增                                               │
│      │       ├─► if(++gen >= _ENDPOINT_MAX_GENERATION) gen = 1     │
│      │       └─► rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr)       │
│      │                                                              │
│      ├─► 子进程初始化                                               │
│      │       ├─► rpc->p_reg.retreg = 0 (子进程返回 0)              │
│      │       ├─► rpc->p_user_time = 0                              │
│      │       ├─► rpc->p_sys_time = 0                               │
│      │       ├─► 清除定时器标志                                    │
│      │       └─► 追加 "*F" 到进程名                                 │
│      │                                                              │
│      ├─► 特权处理                                                   │
│      │       ├─► 如果父进程是系统进程                              │
│      │       │       ├─► rpc->p_priv = priv_addr(USER_PRIV_ID)     │
│      │       │       └─► rpc->p_rts_flags |= RTS_NO_PRIV           │
│      │       └─► 否则继承父进程特权                                │
│      │                                                              │
│      ├─► 状态设置                                                   │
│      │       ├─► RTS_SET(rpc, RTS_NO_QUANTUM)                      │
│      │       ├─► RTS_UNSET(rpc, RTS_SIGNALED | RTS_SIG_PENDING | RTS_P_STOP) │
│      │       └─► sigemptyset(&rpc->p_pending)                      │
│      │                                                              │
│      ├─► 页表处理                                                   │
│      │       ├─► rpc->p_seg.p_cr3 = 0 (i386)                       │
│      │       └─► rpc->p_seg.p_ttbr = 0 (ARM)                       │
│      │                                                              │
│      └─► 返回结果                                                   │
│              ├─► m_ptr->m_krn_lsys_sys_fork.endpt = rpc->p_endpoint│
│              └─► return OK                                          │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 5.2 exec 流程图

```
┌─────────────────────────────────────────────────────────────────────┐
│                         exec 流程                                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  PM 发送 SYS_EXEC 消息                                              │
│      │                                                              │
│      ▼                                                              │
│  do_exec() 入口                                                     │
│      │                                                              │
│      ├─► 参数验证                                                   │
│      │       └─► isokendpt(endpt, &proc_nr)                        │
│      │                                                              │
│      ├─► 清除待投递消息                                             │
│      │       └─► rp->p_misc_flags &= ~MF_DELIVERMSG                │
│      │                                                              │
│      ├─► 复制进程名                                                 │
│      │       └─► data_copy(caller->p_endpoint, name, KERNEL, ...)  │
│      │                                                              │
│      ├─► 架构相关初始化                                             │
│      │       └─► arch_proc_init(rp, ip, stack, ps_str, name)       │
│      │               ├─► arch_proc_reset(pr)                       │
│      │               ├─► strlcpy(pr->p_name, name, ...)            │
│      │               ├─► pr->p_reg.pc = ip                         │
│      │               ├─► pr->p_reg.sp = sp                         │
│      │               └─► pr->p_reg.bx = ps_str                     │
│      │                                                              │
│      ├─► 清除接收状态                                               │
│      │       └─► RTS_UNSET(rp, RTS_RECEIVING)                      │
│      │                                                              │
│      ├─► FPU 重置                                                   │
│      │       ├─► rp->p_misc_flags &= ~MF_FPU_INITIALIZED           │
│      │       └─► release_fpu(rp)                                   │
│      │                                                              │
│      └─► 返回 OK                                                    │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 5.3 clear 流程图

```
┌─────────────────────────────────────────────────────────────────────┐
│                         clear 流程                                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  PM 发送 SYS_CLEAR 消息                                             │
│      │                                                              │
│      ▼                                                              │
│  do_clear() 入口                                                    │
│      │                                                              │
│      ├─► 参数验证                                                   │
│      │       └─► isokendpt(endpt, &exit_p)                         │
│      │                                                              │
│      ├─► 释放地址空间                                               │
│      │       └─► release_address_space(rc)                         │
│      │                                                              │
│      ├─► 检查是否已清理                                             │
│      │       └─► if(isemptyp(rc)) return OK                        │
│      │                                                              │
│      ├─► 移除 IRQ 钩子                                              │
│      │       └─► for (i = 0; i < NR_IRQ_HOOKS; i++)                │
│      │               ├─► if (rc->p_endpoint == irq_hooks[i].proc_nr_e) │
│      │               ├─► rm_irq_handler(&irq_hooks[i])             │
│      │               └─► irq_hooks[i].proc_nr_e = NONE             │
│      │                                                              │
│      ├─► 清除端点                                                   │
│      │       └─► clear_endpoint(rc)                                │
│      │               ├─► RTS_SET(rc, RTS_NO_ENDPOINT)              │
│      │               ├─► clear_ipc(rc)                             │
│      │               ├─► clear_ipc_refs(rc, EDEADSRCDST)           │
│      │               └─► clear_memreq(rc)                          │
│      │                                                              │
│      ├─► 重置定时器                                                 │
│      │       └─► reset_kernel_timer(&priv(rc)->s_alarm_timer)      │
│      │                                                              │
│      ├─► 标记为 FREE                                                │
│      │       └─► RTS_SETFLAGS(rc, RTS_SLOT_FREE)                   │
│      │                                                              │
│      ├─► 释放 FPU                                                   │
│      │       ├─► release_fpu(rc)                                   │
│      │       └─► rc->p_misc_flags &= ~MF_FPU_INITIALIZED           │
│      │                                                              │
│      ├─► 释放特权结构                                               │
│      │       └─► if (priv(rc)->s_flags & SYS_PROC)                 │
│      │               priv(rc)->s_proc_nr = NONE                    │
│      │                                                              │
│      └─► 返回 OK                                                    │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 六、关键代码逐行讲解

### 6.1 do_fork() 逐行分析

**源码位置**：`minix/kernel/system/do_fork.c:32-136`

```c
int do_fork(struct proc * caller, message * m_ptr)
{
/* Handle sys_fork().
 * m_lsys_krn_sys_fork.endpt has forked.
 * The child is m_lsys_krn_sys_fork.slot.
 */
#if defined(__i386__)
  char *old_fpu_save_area_p;
#endif
  register struct proc *rpc;		/* child process pointer */
  struct proc *rpp;			/* parent process pointer */
  int gen;
  int p_proc;
  int namelen;
```

**第 32-45 行**：函数声明和局部变量定义
- `caller`：调用者进程指针（通常是 PM）
- `m_ptr`：消息指针，包含 fork 参数
- `rpc`：子进程指针（register 关键字提示编译器优化）
- `rpp`：父进程指针
- `gen`：端点代数
- `p_proc`：父进程槽位编号
- `old_fpu_save_area_p`：保存子进程原有的 FPU 保存区指针（i386 架构）

```c
  if(!isokendpt(m_ptr->m_lsys_krn_sys_fork.endpt, &p_proc))
	return EINVAL;

  rpp = proc_addr(p_proc);
  rpc = proc_addr(m_ptr->m_lsys_krn_sys_fork.slot);
  if (isemptyp(rpp) || ! isemptyp(rpc)) return(EINVAL);
```

**第 47-53 行**：参数验证
- `isokendpt`：验证端点号有效性，提取槽位编号到 `p_proc`
  - 定义位置：`minix/kernel/proc.h:282`（通过宏展开）
  - 检查端点号是否在有效范围内
- `proc_addr`：根据槽位编号获取进程指针
  - 定义位置：`minix/kernel/proc.h:276`
  - 实现：`#define proc_addr(n) (&(proc[NR_TASKS + (n)]))`
- `isemptyp`：检查进程槽是否为空
  - 定义位置：`minix/kernel/proc.h:281`
  - 实现：`#define isemptyp(p) ((p)->p_rts_flags == RTS_SLOT_FREE)`
- 验证条件：
  - 父进程槽必须非空（正在运行）
  - 子进程槽必须为空（未分配）

```c
  assert(!(rpp->p_misc_flags & MF_DELIVERMSG));

  /* needs to be receiving so we know where the message buffer is */
  if(!RTS_ISSET(rpp, RTS_RECEIVING)) {
	printf("kernel: fork not done synchronously?\n");
	return EINVAL;
  }
```

**第 55-61 行**：同步 fork 验证
- `assert`：断言父进程没有待投递消息
  - 如果有，说明内核状态不一致
- `RTS_ISSET`：检查父进程是否在接收状态
  - 定义位置：`minix/kernel/proc.h:202`
  - 实现：`#define RTS_ISSET(rp, f) (((rp)->p_rts_flags & (f)) == (f))`
- **为什么需要接收状态**：
  - PM 调用 `do_fork` 时，父进程应该在 `receive` 系统调用中阻塞
  - 这样内核知道父进程的消息缓冲区位置
  - 如果不是接收状态，说明 fork 不是同步进行的，违反设计约束

```c
  /* make sure that the FPU context is saved in parent before copy */
  save_fpu(rpp);
```

**第 63-64 行**：保存 FPU 状态
- `save_fpu`：将 FPU 寄存器保存到进程的 FPU 保存区
  - 定义位置：`minix/kernel/arch/i386/arch_system.c:111`（i386 架构）
  - 实现：
    ```c
    void save_fpu(struct proc *pr)
    {
        if (pr->p_misc_flags & MF_FPU_INITIALIZED) {
            if (pr == fpu_owner) {
                fxsave(pr->p_seg.fpu_state);
            }
        }
    }
    ```
  - 只保存已初始化 FPU 的进程
  - 只保存当前 FPU 拥有者的状态
  - 使用 `fxsave` 指令保存完整的 FPU 状态（512 字节）

```c
  /* Copy parent 'proc' struct to child. And reinitialize some fields. */
  gen = _ENDPOINT_G(rpc->p_endpoint);
#if defined(__i386__)
  old_fpu_save_area_p = rpc->p_seg.fpu_state;
#endif
  *rpc = *rpp;				/* copy 'proc' struct */
#if defined(__i386__)
  rpc->p_seg.fpu_state = old_fpu_save_area_p;
  if(proc_used_fpu(rpp))
	memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE);
#endif
```

**第 66-77 行**：PCB 复制
- `_ENDPOINT_G`：提取端点号的代数部分
  - 定义位置：`minix/include/minix/endpoint.h:67`
  - 实现：`#define _ENDPOINT_G(e) (((e)+MAX_NR_TASKS) >> _ENDPOINT_GENERATION_SHIFT)`
- `*rpc = *rpp`：整个结构体复制（约 1KB）
  - 复制所有字段，包括寄存器、状态、统计信息等
- FPU 保存区特殊处理（i386）：
  - 保存子进程原有的 FPU 保存区指针
  - 复制后恢复指针（避免两个进程共享同一保存区）
  - 如果父进程使用了 FPU，复制 FPU 状态到子进程的保存区
  - `FPU_XFP_SIZE = 512` 字节（定义在 `minix/kernel/const.h`）

```c
  if(++gen >= _ENDPOINT_MAX_GENERATION)	/* increase generation */
	gen = 1;			/* generation number wraparound */
  rpc->p_nr = m_ptr->m_lsys_krn_sys_fork.slot;	/* this was obliterated by copy */
  rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);	/* new endpoint of slot */
```

**第 79-82 行**：端点代数递增
- 代数递增，防止旧引用复活
- `_ENDPOINT_MAX_GENERATION`：最大代数值
  - 定义位置：`minix/include/minix/endpoint.h:50`
  - 值：`INT_MAX/_ENDPOINT_GENERATION_SIZE-1` ≈ 65535
- 如果超过最大值，回绕到 1（不使用 0，因为 0 表示初始状态）
- `_ENDPOINT(gen, nr)`：构造新端点号
  - 定义位置：`minix/include/minix/endpoint.h:65-66`
  - 实现：`((endpoint_t)(((g) << _ENDPOINT_GENERATION_SHIFT) + (p)))`

```c
  rpc->p_reg.retreg = 0;	/* child sees pid = 0 to know it is child */
  rpc->p_user_time = 0;		/* set all the accounting times to 0 */
  rpc->p_sys_time = 0;
```

**第 84-86 行**：子进程返回值和统计初始化
- `p_reg.retreg`：返回值寄存器（i386 的 eax，ARM 的 r0）
  - 子进程返回 0，父进程返回子进程 PID
- 统计时间清零：
  - `p_user_time`：用户态运行时间
  - `p_sys_time`：内核态运行时间

```c
  rpc->p_misc_flags &=
	~(MF_VIRT_TIMER | MF_PROF_TIMER | MF_SC_TRACE | MF_SPROF_SEEN | MF_STEP);
  rpc->p_virt_left = 0;		/* disable, clear the process-virtual timers */
  rpc->p_prof_left = 0;
```

**第 88-91 行**：清除定时器和跟踪标志
- 清除的标志：
  - `MF_VIRT_TIMER`：虚拟定时器（ITIMER_VIRTUAL）
  - `MF_PROF_TIMER`：统计定时器（ITIMER_PROF）
  - `MF_SC_TRACE`：系统调用跟踪
  - `MF_SPROF_SEEN`：性能分析已看到
  - `MF_STEP`：单步执行
- 子进程不继承父进程的定时器和跟踪状态

```c
  /* Mark process name as being a forked copy */
  namelen = strlen(rpc->p_name);
#define FORKSTR "*F"
  if(namelen+strlen(FORKSTR) < sizeof(rpc->p_name))
	strcat(rpc->p_name, FORKSTR);
```

**第 93-97 行**：标记进程名
- 在进程名后追加 `*F`，表示这是 fork 的子进程
- 检查长度，避免缓冲区溢出
- `PROC_NAME_LEN = 16`（定义在 `minix/include/minix/com.h`）

```c
  /* the child process is not runnable until it's scheduled. */
  RTS_SET(rpc, RTS_NO_QUANTUM);
  reset_proc_accounting(rpc);
```

**第 99-101 行**：设置运行状态
- `RTS_SET`：设置运行状态标志
  - 定义位置：`minix/kernel/proc.h:206-213`
  - 实现：
    ```c
    #define RTS_SET(rp, f)							\
	do {								\
		const int rts = (rp)->p_rts_flags;			\
		(rp)->p_rts_flags |= (f);				\
		if(rts_f_is_runnable(rts) && !proc_is_runnable(rp)) {	\
			dequeue(rp);					\
		}							\
	} while(0)
    ```
  - 如果进程从可运行变为不可运行，从调度队列移除
- `RTS_NO_QUANTUM`：时间片用完，需要重新调度
- `reset_proc_accounting`：重置统计信息
  - 定义位置：`minix/kernel/system/do_fork.c`（未展示）
  - 重置队列时间、调度次数等

```c
  rpc->p_cpu_time_left = 0;
  rpc->p_cycles = 0;
  rpc->p_kcall_cycles = 0;
  rpc->p_kipc_cycles = 0;

  rpc->p_tick_cycles = 0;
  cpuavg_init(&rpc->p_cpuavg);
```

**第 103-108 行**：清零 CPU 统计
- `p_cpu_time_left`：剩余 CPU 时间
- `p_cycles`：使用的 CPU 周期
- `p_kcall_cycles`：内核调用周期
- `p_kipc_cycles`：IPC 周期
- `p_tick_cycles`：时钟滴答周期
- `cpuavg_init`：初始化 CPU 平均值

```c
  /* If the parent is a privileged process, take away the privileges from the 
   * child process and inhibit it from running by setting the NO_PRIV flag.
   * The caller should explicitly set the new privileges before executing.
   */
  if (priv(rpp)->s_flags & SYS_PROC) {
      rpc->p_priv = priv_addr(USER_PRIV_ID);
      rpc->p_rts_flags |= RTS_NO_PRIV;
  }
```

**第 110-115 行**：特权处理
- `priv(rpp)`：获取父进程的特权结构
  - 定义位置：`minix/kernel/priv.h:79`
  - 实现：`#define priv(rp) ((rp)->p_priv)`
- `SYS_PROC`：系统进程标志
  - 定义位置：`minix/kernel/priv.h:44`
- 如果父进程是系统进程：
  - 子进程设置为用户特权（`USER_PRIV_ID`）
  - 设置 `RTS_NO_PRIV` 标志，阻止运行
  - PM 需要显式设置新特权后才能运行
- **设计原因**：防止 fork 的系统进程继承特权，造成安全漏洞

```c
  /* Calculate endpoint identifier, so caller knows what it is. */
  m_ptr->m_krn_lsys_sys_fork.endpt = rpc->p_endpoint;
  m_ptr->m_krn_lsys_sys_fork.msgaddr = rpp->p_delivermsg_vir;
```

**第 117-119 行**：返回结果
- `m_krn_lsys_sys_fork.endpt`：子进程的端点号
- `m_krn_lsys_sys_fork.msgaddr`：父进程的消息缓冲区地址
  - PM 需要知道这个地址，以便向子进程发送消息

```c
  /* Don't schedule process in VM mode until it has a new pagetable. */
  if(m_ptr->m_lsys_krn_sys_fork.flags & PFF_VMINHIBIT) {
  	RTS_SET(rpc, RTS_VMINHIBIT);
  }
```

**第 121-124 行**：VM 抑制标志
- `PFF_VMINHIBIT`：fork 标志，表示需要 VM 设置页表
  - 定义位置：`minix/include/minix/sys_config.h`
- 设置 `RTS_VMINHIBIT` 标志，阻止调度
- VM 设置页表后，清除这个标志

```c
  /* 
   * Only one in group should have RTS_SIGNALED, child doesn't inherit tracing.
   */
  RTS_UNSET(rpc, (RTS_SIGNALED | RTS_SIG_PENDING | RTS_P_STOP));
  (void) sigemptyset(&rpc->p_pending);
```

**第 126-129 行**：清除信号和跟踪状态
- `RTS_UNSET`：清除运行状态标志
  - 定义位置：`minix/kernel/proc.h:216-224`
  - 实现：
    ```c
    #define RTS_UNSET(rp, f) 						\
	do {								\
		int rts;						\
		rts = (rp)->p_rts_flags;				\
		(rp)->p_rts_flags &= ~(f);				\
		if(!rts_f_is_runnable(rts) && proc_is_runnable(rp)) {	\
			enqueue(rp);					\
		}							\
	} while(0)
    ```
  - 如果进程从不可运行变为可运行，加入调度队列
- 清除的标志：
  - `RTS_SIGNALED`：信号到达
  - `RTS_SIG_PENDING`：信号待处理
  - `RTS_P_STOP`：进程被跟踪
- `sigemptyset`：清空待处理信号集

```c
#if defined(__i386__)
  rpc->p_seg.p_cr3 = 0;
  rpc->p_seg.p_cr3_v = NULL;
#elif defined(__arm__)
  rpc->p_seg.p_ttbr = 0;
  rpc->p_seg.p_ttbr_v = NULL;
#endif

  return OK;
}
```

**第 131-137 行**：清空页表指针
- i386：清空 CR3 寄存器值（页目录物理地址）
- ARM：清空 TTBR 寄存器值（页表基址）
- 子进程需要新的页表，由 VM 设置

### 6.2 do_exec() 逐行分析

**源码位置**：`minix/kernel/system/do_exec.c:27-60`

```c
int do_exec(struct proc * caller, message * m_ptr)
{
/* Handle sys_exec().  A process has done a successful EXEC. Patch it up. */
  register struct proc *rp;
  int proc_nr;
  char name[PROC_NAME_LEN];

  if(!isokendpt(m_ptr->m_lsys_krn_sys_exec.endpt, &proc_nr))
	return EINVAL;

  rp = proc_addr(proc_nr);
```

**第 27-37 行**：参数验证
- 验证端点号有效性
- 获取进程指针

```c
  if(rp->p_misc_flags & MF_DELIVERMSG) {
	rp->p_misc_flags &= ~MF_DELIVERMSG;
  }
```

**第 39-42 行**：清除待投递消息
- 如果有待投递的消息，清除标志
- exec 后，旧的消息不再有效

```c
  /* Save command name for debugging, ps(1) output, etc. */
  if(data_copy(caller->p_endpoint, m_ptr->m_lsys_krn_sys_exec.name,
	KERNEL, (vir_bytes) name,
	(phys_bytes) sizeof(name) - 1) != OK)
  	strncpy(name, "<unset>", PROC_NAME_LEN);

  name[sizeof(name)-1] = '\0';
```

**第 44-51 行**：复制进程名
- `data_copy`：跨地址空间复制数据
  - 定义位置：`minix/kernel/system/do_safecopy.c`
  - 参数：源端点、源地址、目标端点、目标地址、大小
- 从调用者（PM）的地址空间复制进程名到内核
- 如果复制失败，使用默认名称 `<unset>`
- 确保字符串以 null 结尾

```c
  /* Set process state. */
  arch_proc_init(rp,
	(u32_t) m_ptr->m_lsys_krn_sys_exec.ip,
	(u32_t) m_ptr->m_lsys_krn_sys_exec.stack,
	(u32_t) m_ptr->m_lsys_krn_sys_exec.ps_str, name);
```

**第 53-58 行**：架构相关初始化
- `arch_proc_init`：设置新的执行环境
  - 定义位置：`minix/kernel/arch/i386/memory.c:722`（i386 架构）
  - 实现：
    ```c
    void arch_proc_init(struct proc *pr, const u32_t ip, const u32_t sp,
    	const u32_t ps_str, char *name)
    {
    	arch_proc_reset(pr);
    	strlcpy(pr->p_name, name, sizeof(pr->p_name));
    
    	/* set custom state we know */
    	pr->p_reg.pc = ip;
    	pr->p_reg.sp = sp;
    	pr->p_reg.bx = ps_str;
    }
    ```
- 参数：
  - `ip`：新的指令指针（程序入口点）
  - `stack`：新的栈指针
  - `ps_str`：ps_strings 结构地址（用于 ps 命令）
  - `name`：进程名

```c
  /* No reply to EXEC call */
  RTS_UNSET(rp, RTS_RECEIVING);

  /* Mark fpu_regs contents as not significant, so fpu
   * will be initialized, when it's used next time. */
  rp->p_misc_flags &= ~MF_FPU_INITIALIZED;
  /* force reloading FPU if the current process is the owner */
  release_fpu(rp);
  return(OK);
}
```

**第 60-67 行**：清除状态和 FPU 重置
- `RTS_UNSET(rp, RTS_RECEIVING)`：清除接收状态
  - exec 后，进程不再等待回复
- FPU 重置：
  - 清除 `MF_FPU_INITIALIZED` 标志
  - `release_fpu`：释放 FPU 所有权
    - 定义位置：`minix/kernel/proc.c:1961`
    - 实现：
      ```c
      void release_fpu(struct proc * p) {
          if (fpu_owner == p)
              fpu_owner = NULL;
      }
      ```
  - 下次使用 FPU 时，会重新初始化

### 6.3 do_exit() 逐行分析

**源码位置**：`minix/kernel/system/do_exit.c:18-27`

```c
int do_exit(struct proc * caller, message * m_ptr)
{
/* Handle sys_exit. A system process has requested to exit. Generate a
 * self-termination signal.
 */
  int sig_nr = SIGABRT;

  cause_sig(caller->p_nr, sig_nr);      /* send a signal to the caller */

  return(EDONTREPLY);			/* don't reply */
}
```

**逐行分析**：
- **第 18-23 行**：函数声明和信号选择
  - `sig_nr = SIGABRT`：异常终止信号
  - 定义位置：`minix/include/signal.h:59`
  - 值：6（POSIX 标准）
- **第 25 行**：发送信号
  - `cause_sig`：向进程发送信号
    - 定义位置：`minix/kernel/system.c:389`
    - 实现见下文
  - 参数：进程槽位编号、信号编号
- **第 27 行**：不回复
  - `EDONTREPLY`：特殊返回值，表示不回复消息
    - 定义位置：`minix/include/minix/com.h:156`
    - 值：-999

**设计说明**：
- 系统进程调用 `do_exit` 会收到 `SIGABRT` 信号
- PM 负责处理信号，清理进程资源
- 不回复消息，因为进程即将退出

### 6.4 cause_sig() 分析

**源码位置**：`minix/kernel/system.c:389-462`

```c
void cause_sig(proc_nr_t proc_nr, int sig_nr)
{
/* A system process wants to send signal 'sig_nr' to process 'proc_nr'.
 * Examples are:
 *  - HARDWARE wanting to cause a SIGSEGV after a CPU exception
 *  - TTY wanting to cause SIGINT upon getting a DEL
 *  - FS wanting to cause SIGPIPE for a broken pipe
 * Signals are handled by sending a message to the signal manager assigned to
 * the process. This function handles the signals and makes sure the signal
 * manager gets them by sending a notification. The process being signaled
 * is blocked while the signal manager has not finished all signals for it.
 * Race conditions between calls to this function and the system calls that
 * process pending kernel signals cannot exist. Signal related functions are
 * only called when a user process causes a CPU exception and from the kernel
 * process level, which runs to completion.
 */
  register struct proc *rp, *sig_mgr_rp;
  endpoint_t sig_mgr;
  int sig_mgr_proc_nr;
  int s;

  /* Lookup signal manager. */
  rp = proc_addr(proc_nr);
  sig_mgr = priv(rp)->s_sig_mgr;
  if(sig_mgr == SELF) sig_mgr = rp->p_endpoint;
```

**第 389-410 行**：查找信号管理器
- `s_sig_mgr`：进程的信号管理器端点
  - 定义位置：`minix/kernel/priv.h:54`
  - 用户进程通常是 PM
  - 系统进程可以是自身或 RS（重启服务）
- `SELF`：特殊端点，表示进程自身处理信号
  - 定义位置：`minix/include/minix/endpoint.h:56`

```c
  /* If the target is the signal manager of itself, send the signal directly. */
  if(rp->p_endpoint == sig_mgr) {
       if(SIGS_IS_LETHAL(sig_nr)) {
           /* If the signal is lethal, see if a backup signal manager exists. */
           sig_mgr = priv(rp)->s_bak_sig_mgr;
           if(sig_mgr != NONE && isokendpt(sig_mgr, &sig_mgr_proc_nr)) {
               priv(rp)->s_sig_mgr = sig_mgr;
               priv(rp)->s_bak_sig_mgr = NONE;
               sig_mgr_rp = proc_addr(sig_mgr_proc_nr);
               RTS_UNSET(sig_mgr_rp, RTS_NO_PRIV);
               cause_sig(proc_nr, sig_nr); /* try again with the new sig mgr. */
               return;
           }
           /* We are out of luck. Time to panic. */
           proc_stacktrace(rp);
           panic("cause_sig: sig manager %d gets lethal signal %d for itself",
	   	rp->p_endpoint, sig_nr);
       }
       sigaddset(&priv(rp)->s_sig_pending, sig_nr);
       if(OK != send_sig(rp->p_endpoint, SIGKSIGSM))
       	panic("send_sig failed");
       return;
  }
```

**第 412-434 行**：进程自身处理信号
- `SIGS_IS_LETHAL`：判断信号是否致命
  - 定义位置：`minix/include/minix/sigtypes.h`
  - 致命信号：SIGKILL, SIGABRT, SIGSEGV 等
- 如果信号管理器收到致命信号：
  - 尝试切换到备份信号管理器
  - 如果没有备份，内核 panic
- `s_sig_pending`：信号管理器的待处理信号集
- `send_sig`：发送通知给信号管理器
  - 参数：目标端点、通知类型
  - `SIGKSIGSM`：信号管理器收到信号的通知

```c
  s = sigismember(&rp->p_pending, sig_nr);
  /* Check if the signal is already pending. Process it otherwise. */
  if (!s) {
      sigaddset(&rp->p_pending, sig_nr);
      if (! (RTS_ISSET(rp, RTS_SIGNALED))) {		/* other pending */
	  RTS_SET(rp, RTS_SIGNALED | RTS_SIG_PENDING);
          if(OK != send_sig(sig_mgr, SIGKSIG))
	  	panic("send_sig failed");
      }
  }
}
```

**第 436-446 行**：普通进程处理信号
- `sigismember`：检查信号是否已在待处理集合中
- 如果信号未待处理：
  - 添加到待处理信号集
  - 如果进程未被标记为收到信号：
    - 设置 `RTS_SIGNALED` 和 `RTS_SIG_PENDING` 标志
    - 发送通知给信号管理器
    - `SIGKSIG`：进程收到信号的通知

### 6.5 do_clear() 逐行分析

**源码位置**：`minix/kernel/system/do_clear.c:28-80`

```c
int do_clear(struct proc * caller, message * m_ptr)
{
/* Handle sys_clear. Only the PM can request other process slots to be cleared
 * when a process has exited.
 * The routine to clean up a process table slot cancels outstanding timers, 
 * possibly removes the process from the message queues, and resets certain 
 * process table fields to the default values.
 */
  struct proc *rc;
  int exit_p;
  int i;

  if(!isokendpt(m_ptr->m_lsys_krn_sys_clear.endpt, &exit_p)) {
      /* get exiting process */
      return EINVAL;
  }
  rc = proc_addr(exit_p);	/* clean up */

  release_address_space(rc);

  /* Don't clear if already cleared. */
  if(isemptyp(rc)) return OK;
```

**第 28-47 行**：参数验证和地址空间释放
- `release_address_space`：释放进程的地址空间
  - 定义位置：`minix/kernel/system/do_clear.c`（未展示）
  - 调用 VM 释放内存映射
- 检查是否已清理，避免重复清理

```c
  /* Check the table with IRQ hooks to see if hooks should be released. */
  for (i=0; i < NR_IRQ_HOOKS; i++) {
      if (rc->p_endpoint == irq_hooks[i].proc_nr_e) {
        rm_irq_handler(&irq_hooks[i]);	/* remove interrupt handler */
        irq_hooks[i].proc_nr_e = NONE;	/* mark hook as free */
      } 
  }
```

**第 49-56 行**：移除 IRQ 钩子
- `irq_hooks`：IRQ 钩子数组
  - 定义位置：`minix/kernel/proc.h`（未展示）
  - 大小：`NR_IRQ_HOOKS = 16`（定义在 `minix/include/minix/com.h`）
- `rm_irq_handler`：移除中断处理程序
  - 定义位置：`minix/kernel/arch/i386/irq.c`（未展示）
- 遍历所有 IRQ 钩子，移除属于该进程的钩子

```c
  /* Remove the process' ability to send and receive messages */
  clear_endpoint(rc);
```

**第 58 行**：清除端点
- `clear_endpoint`：清除进程的 IPC 能力
  - 定义位置：`minix/kernel/system.c:540`
  - 实现见下文

```c
  /* Turn off any alarm timers at the clock. */   
  reset_kernel_timer(&priv(rc)->s_alarm_timer);

  /* Make sure that the exiting process is no longer scheduled,
   * and mark slot as FREE. Also mark saved fpu contents as not significant.
   */
  RTS_SETFLAGS(rc, RTS_SLOT_FREE);
  
  /* release FPU */
  release_fpu(rc);
  rc->p_misc_flags &= ~MF_FPU_INITIALIZED;
```

**第 60-70 行**：定时器和 FPU 清理
- `reset_kernel_timer`：重置内核定时器
  - 定义位置：`minix/kernel/clock.c`（未展示）
- `RTS_SETFLAGS`：设置运行状态标志
  - 定义位置：`minix/kernel/proc.h:227-231`
  - 实现：
    ```c
    #define RTS_SETFLAGS(rp, f)					\
	do {								\
		if(proc_is_runnable(rp) && (f)) { dequeue(rp); }		\
		(rp)->p_rts_flags = (f);				\
	} while(0)
    ```
  - 直接设置标志值（不是或操作）
- `RTS_SLOT_FREE`：槽位空闲标志
- 释放 FPU 所有权，清除 FPU 初始化标志

```c
  /* Release the process table slot. If this is a system process, also
   * release its privilege structure.  Further cleanup is not needed at
   * this point. All important fields are reinitialized when the 
   * slots are assigned to another, new process. 
   */
  if (priv(rc)->s_flags & SYS_PROC) priv(rc)->s_proc_nr = NONE;

#if 0
  /* Clean up virtual memory */
  if (rc->p_misc_flags & MF_VM) {
  	vm_map_default(rc);
  }
#endif

  return OK;
}
```

**第 72-82 行**：特权结构清理
- 如果是系统进程，释放特权结构
- `s_proc_nr = NONE`：标记特权结构未分配
- 注释掉的代码：虚拟内存清理（已废弃）

### 6.6 clear_endpoint() 分析

**源码位置**：`minix/kernel/system.c:540-593`

```c
void clear_endpoint(struct proc * rc)
{
/* Clean up the slot of the process given as 'rc'. */
  if(isemptyp(rc)) panic("clear_proc: empty process: %d",  rc->p_endpoint);

#if DEBUG_IPC_HOOK
  hook_ipc_clear(rc);
#endif

  /* Make sure that the exiting process is no longer scheduled. */
  RTS_SET(rc, RTS_NO_ENDPOINT);
  if (priv(rc)->s_flags & SYS_PROC)
  {
	priv(rc)->s_asynsize= 0;
  }
```

**第 540-557 行**：设置端点无效标志
- `RTS_NO_ENDPOINT`：进程无法发送/接收消息
- 如果是系统进程，清空异步消息大小

```c
  /* If the process happens to be queued trying to send a
   * message, then it must be removed from the message queues.
   */
  clear_ipc(rc);

  /* Likewise, if another process was sending or receive a message to or from
   * the exiting process, it must be alerted that process no longer is alive.
   * Check all processes.
   */
  clear_ipc_refs(rc, EDEADSRCDST);

  /* Finally, if the process was blocked on a VM request, remove it from the
   * queue of processes waiting to be processed by VM.
   */
  clear_memreq(rc);
}
```

**第 559-577 行**：清理 IPC 相关
- `clear_ipc`：清理进程的 IPC 状态
  - 定义位置：`minix/kernel/proc.c`（未展示）
  - 从消息队列中移除进程
- `clear_ipc_refs`：清理其他进程对该进程的引用
  - 定义位置：`minix/kernel/system.c:597`（未展示）
  - 参数：`EDEADSRCDST` 表示目标进程已死亡
  - 遍历所有进程，通知它们目标已死亡
- `clear_memreq`：清理 VM 内存请求
  - 定义位置：`minix/kernel/proc.c`（未展示）
  - 从 VM 请求队列中移除进程

### 6.7 do_kill() 逐行分析

**源码位置**：`minix/kernel/system/do_kill.c:18-41`

```c
int do_kill(struct proc * caller, message * m_ptr)
{
/* Handle sys_kill(). Cause a signal to be sent to a process. Any request
 * is added to the map of pending signals and the signal manager
 * associated to the process is informed about the new signal. The signal
 * is then delivered using POSIX signal handlers for user processes, or
 * translated into an IPC message for system services.
 */
  proc_nr_t proc_nr, proc_nr_e;
  int sig_nr = m_ptr->m_sigcalls.sig;

  proc_nr_e = (proc_nr_t)m_ptr->m_sigcalls.endpt;

  if (!isokendpt(proc_nr_e, &proc_nr)) return(EINVAL);
  if (sig_nr >= _NSIG) return(EINVAL);
  if (iskerneln(proc_nr)) return(EPERM);

  /* Set pending signal to be processed by the signal manager. */
  cause_sig(proc_nr, sig_nr);

  return(OK);
}
```

**逐行分析**：
- **第 20-23 行**：提取参数
  - `m_sigcalls.sig`：信号编号
  - `m_sigcalls.endpt`：目标进程端点
- **第 25-27 行**：参数验证
  - `isokendpt`：验证端点号有效性
  - `_NSIG`：信号数量上限
    - 定义位置：`minix/include/signal.h:68`
    - 值：64（POSIX 标准）
  - `iskerneln`：检查是否为内核任务
    - 定义位置：`minix/kernel/proc.h:280`
    - 实现：`#define iskerneln(n) ((n) < 0)`
    - 不允许向内核任务发送信号
- **第 29 行**：发送信号
  - 调用 `cause_sig` 发送信号

### 6.8 do_getmcontext() 逐行分析

**源码位置**：`minix/kernel/system/do_mcontext.c:28-66`

```c
int do_getmcontext(struct proc * caller, message * m_ptr)
{
/* Retrieve machine context of a process */

  register struct proc *rp;
  int proc_nr, r;
  mcontext_t mc;

  if (!isokendpt(m_ptr->m_lsys_krn_sys_getmcontext.endpt, &proc_nr))
	return(EINVAL);
  if (iskerneln(proc_nr)) return(EPERM);
  rp = proc_addr(proc_nr);

#if defined(__i386__)
  if (!proc_used_fpu(rp))
	return(OK);	/* No state to copy */
#endif
```

**第 28-43 行**：参数验证
- `mcontext_t`：机器上下文结构
  - 定义位置：`minix/include/machine/mcontext.h`
  - 包含寄存器状态和 FPU 状态
- `proc_used_fpu`：检查进程是否使用了 FPU
  - 定义位置：`minix/kernel/proc.h:194`
  - 实现：`#define proc_used_fpu(p) ((p)->p_misc_flags & (MF_FPU_INITIALIZED))`
- 如果进程未使用 FPU，直接返回 OK（无需复制状态）

```c
  /* Get the mcontext structure into our address space.  */
  if ((r = data_copy(m_ptr->m_lsys_krn_sys_getmcontext.endpt,
		m_ptr->m_lsys_krn_sys_getmcontext.ctx_ptr, KERNEL,
		(vir_bytes) &mc, (phys_bytes) sizeof(mcontext_t))) != OK)
	return(r);

  mc.mc_flags = 0;
```

**第 45-51 行**：复制用户空间的 mcontext 结构
- 从用户空间复制 mcontext 结构到内核
- 这是为了获取用户设置的标志等信息
- 清空标志字段

```c
#if defined(__i386__)
  /* Copy FPU state */
  if (proc_used_fpu(rp)) {
	/* make sure that the FPU context is saved into proc structure first */
	save_fpu(rp);
	mc.mc_flags = (rp->p_misc_flags & MF_FPU_INITIALIZED) ? _MC_FPU_SAVED : 0;
	assert(sizeof(mc.__fpregs.__fp_reg_set) == FPU_XFP_SIZE);
	memcpy(&(mc.__fpregs.__fp_reg_set), rp->p_seg.fpu_state, FPU_XFP_SIZE);
  } 
#endif
```

**第 53-62 行**：保存 FPU 状态
- 如果进程使用了 FPU：
  - 先保存 FPU 状态到进程结构
  - 设置 `_MC_FPU_SAVED` 标志
  - 复制 FPU 状态到 mcontext 结构

```c
  /* Copy the mcontext structure to the user's address space. */
  if ((r = data_copy(KERNEL, (vir_bytes) &mc,
	m_ptr->m_lsys_krn_sys_getmcontext.endpt,
	m_ptr->m_lsys_krn_sys_getmcontext.ctx_ptr,
	(phys_bytes) sizeof(mcontext_t))) != OK)
	return(r);

  return(OK);
}
```

**第 64-71 行**：复制回用户空间
- 将填充好的 mcontext 结构复制回用户空间

### 6.9 do_setmcontext() 逐行分析

**源码位置**：`minix/kernel/system/do_mcontext.c:76-106`

```c
int do_setmcontext(struct proc * caller, message * m_ptr)
{
/* Set machine context of a process */

  register struct proc *rp;
  int proc_nr, r;
  mcontext_t mc;

  if (!isokendpt(m_ptr->m_lsys_krn_sys_setmcontext.endpt, &proc_nr)) return(EINVAL);
  rp = proc_addr(proc_nr);

  /* Get the mcontext structure into our address space.  */
  if ((r = data_copy(m_ptr->m_lsys_krn_sys_setmcontext.endpt,
		m_ptr->m_lsys_krn_sys_setmcontext.ctx_ptr, KERNEL,
		(vir_bytes) &mc, (phys_bytes) sizeof(mcontext_t))) != OK)
	return(r);
```

**第 76-92 行**：参数验证和复制
- 从用户空间复制 mcontext 结构到内核

```c
#if defined(__i386__)
  /* Copy FPU state */
  if (mc.mc_flags & _MC_FPU_SAVED) {
	rp->p_misc_flags |= MF_FPU_INITIALIZED;
	assert(sizeof(mc.__fpregs.__fp_reg_set) == FPU_XFP_SIZE);
	memcpy(rp->p_seg.fpu_state, &(mc.__fpregs.__fp_reg_set), FPU_XFP_SIZE);
  } else
	rp->p_misc_flags &= ~MF_FPU_INITIALIZED;
  /* force reloading FPU in either case */
  release_fpu(rp);
#endif

  return(OK);
}
```

**第 94-106 行**：恢复 FPU 状态
- 如果 `_MC_FPU_SAVED` 标志设置：
  - 设置 `MF_FPU_INITIALIZED` 标志
  - 复制 FPU 状态到进程结构
- 否则清除 FPU 初始化标志
- 释放 FPU 所有权，强制下次使用时重新加载

---

## 七、调用链分析

### 7.1 fork 调用链

```
用户进程调用 fork()
    │
    ├─► libc: fork()
    │       └─► 构造消息，发送给 PM
    │
    ├─► PM: do_fork()
    │       ├─► 分配子进程槽位
    │       ├─► 调用 VM: vm_fork()
    │       │       └─► 创建新页表
    │       └─► 发送 SYS_FORK 给内核
    │
    └─► 内核: do_fork()
            ├─► isokendpt() - 验证端点
            ├─► proc_addr() - 获取进程指针
            ├─► save_fpu() - 保存 FPU 状态
            │       └─► fxsave (汇编指令)
            ├─► _ENDPOINT_G() - 提取代数
            ├─► _ENDPOINT() - 构造新端点
            ├─► RTS_SET() - 设置状态
            │       └─► dequeue() - 从调度队列移除
            └─► 返回 OK
```

### 7.2 exec 调用链

```
用户进程调用 execve()
    │
    ├─► libc: execve()
    │       └─► 构造消息，发送给 PM
    │
    ├─► PM: do_exec()
    │       ├─► 加载新程序映像
    │       ├─► 设置新的栈和指令指针
    │       └─► 发送 SYS_EXEC 给内核
    │
    └─► 内核: do_exec()
            ├─► isokendpt() - 验证端点
            ├─► data_copy() - 复制进程名
            │       └─► safecopy() - 安全内存拷贝
            ├─► arch_proc_init() - 架构相关初始化
            │       ├─► arch_proc_reset() - 重置进程状态
            │       └─► 设置 IP/SP
            ├─► RTS_UNSET() - 清除接收状态
            │       └─► enqueue() - 加入调度队列
            └─► release_fpu() - 释放 FPU
```

### 7.3 clear 调用链

```
PM 处理进程退出
    │
    ├─► PM: do_exit()
    │       ├─► 标记进程为 ZOMBIE
    │       ├─► 通知父进程
    │       └─► 发送 SYS_CLEAR 给内核
    │
    └─► 内核: do_clear()
            ├─► release_address_space() - 释放地址空间
            │       └─► 调用 VM 释放内存
            ├─► rm_irq_handler() - 移除 IRQ 钩子
            │       └─► 清除 IDT 中的中断门
            ├─► clear_endpoint() - 清除端点
            │       ├─► RTS_SET() - 设置 RTS_NO_ENDPOINT
            │       ├─► clear_ipc() - 清理 IPC 状态
            │       ├─► clear_ipc_refs() - 清理 IPC 引用
            │       └─► clear_memreq() - 清理 VM 请求
            ├─► reset_kernel_timer() - 重置定时器
            ├─► RTS_SETFLAGS() - 设置 RTS_SLOT_FREE
            └─► release_fpu() - 释放 FPU
```

---

## 八、注意事项与易错点

### 8.1 fork 同步约束

**问题**：为什么 fork 需要父进程处于 RTS_RECEIVING 状态？

**原因**：
1. PM 调用 `do_fork` 时，父进程应该在 `receive` 系统调用中阻塞
2. 这样内核知道父进程的消息缓冲区位置（`p_delivermsg_vir`）
3. 如果不是接收状态，说明 fork 不是同步进行的，违反设计约束

**代码验证**：
```c
if(!RTS_ISSET(rpp, RTS_RECEIVING)) {
	printf("kernel: fork not done synchronously?\n");
	return EINVAL;
}
```

### 8.2 端点代数回绕

**问题**：端点代数超过最大值怎么办？

**处理**：
```c
if(++gen >= _ENDPOINT_MAX_GENERATION)
	gen = 1;	/* 不使用 0，因为 0 表示初始状态 */
```

**注意**：
- `_ENDPOINT_MAX_GENERATION ≈ 65535`
- 回绕到 1，而不是 0
- 代数 0 表示进程从未被复用过

### 8.3 FPU 状态一致性

**问题**：为什么 fork 和 exec 都需要处理 FPU 状态？

**原因**：
1. **fork**：子进程继承父进程的 FPU 状态
   - 需要先保存父进程的 FPU 状态
   - 然后复制到子进程的 FPU 保存区
   - 每个进程需要独立的 FPU 保存区

2. **exec**：新程序不应该继承旧的 FPU 状态
   - 清除 `MF_FPU_INITIALIZED` 标志
   - 释放 FPU 所有权
   - 下次使用 FPU 时重新初始化

**代码验证**：
```c
// fork
save_fpu(rpp);
*rpc = *rpp;
if(proc_used_fpu(rpp))
	memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE);

// exec
rp->p_misc_flags &= ~MF_FPU_INITIALIZED;
release_fpu(rp);
```

### 8.4 特权继承问题

**问题**：fork 的系统进程如何处理特权？

**处理**：
```c
if (priv(rpp)->s_flags & SYS_PROC) {
    rpc->p_priv = priv_addr(USER_PRIV_ID);
    rpc->p_rts_flags |= RTS_NO_PRIV;
}
```

**原因**：
- 防止 fork 的系统进程继承特权
- 子进程设置为用户特权
- 设置 `RTS_NO_PRIV` 标志，阻止运行
- PM 需要显式设置新特权后才能运行

**安全意义**：
- 防止恶意进程通过 fork 系统进程获取特权
- 确保特权进程的生命周期受控

### 8.5 clear 的幂等性

**问题**：如果 clear 被多次调用会怎样？

**处理**：
```c
if(isemptyp(rc)) return OK;
```

**原因**：
- 检查进程槽是否已清理
- 如果已清理，直接返回 OK
- 避免重复清理导致错误

### 8.6 信号管理器死锁

**问题**：信号管理器收到致命信号怎么办？

**处理**：
```c
if(SIGS_IS_LETHAL(sig_nr)) {
    sig_mgr = priv(rp)->s_bak_sig_mgr;
    if(sig_mgr != NONE && isokendpt(sig_mgr, &sig_mgr_proc_nr)) {
        priv(rp)->s_sig_mgr = sig_mgr;
        priv(rp)->s_bak_sig_mgr = NONE;
        RTS_UNSET(sig_mgr_rp, RTS_NO_PRIV);
        cause_sig(proc_nr, sig_nr);
        return;
    }
    panic("cause_sig: sig manager %d gets lethal signal %d for itself",
        rp->p_endpoint, sig_nr);
}
```

**原因**：
- 信号管理器收到致命信号时，尝试切换到备份信号管理器
- 如果没有备份，内核 panic
- 这是系统设计的安全网，防止信号管理器死锁

### 8.7 页表清空

**问题**：为什么 fork 需要清空页表指针？

**处理**：
```c
#if defined(__i386__)
  rpc->p_seg.p_cr3 = 0;
  rpc->p_seg.p_cr3_v = NULL;
#elif defined(__arm__)
  rpc->p_seg.p_ttbr = 0;
  rpc->p_seg.p_ttbr_v = NULL;
#endif
```

**原因**：
- 子进程需要新的页表，由 VM 设置
- 清空页表指针，避免使用父进程的页表
- 如果不清空，子进程会访问父进程的地址空间

---

## 九、模块级 Rust 重构建议

### 9.1 类型安全的进程结构

```rust
#[derive(Debug, Clone)]
pub struct Proc {
    pub nr: ProcNr,
    pub endpoint: Endpoint,
    pub name: ArrayString<PROC_NAME_LEN>,
    pub regs: Registers,
    pub misc_flags: MiscFlags,
    pub rts_flags: RtsFlags,
    pub user_time: u64,
    pub sys_time: u64,
    pub virt_timer: Option<VirtualTimer>,
    pub prof_timer: Option<ProfileTimer>,
    pub fpu_state: Option<FpuState>,
    pub priv_ptr: *mut Priv,
}

bitflags::bitflags! {
    pub struct RtsFlags: u32 {
        const SLOT_FREE   = 0b0000_0001;
        const PROC_STOP   = 0b0000_0010;
        const SENDING     = 0b0000_0100;
        const RECEIVING   = 0b0000_1000;
        const SIGNALED    = 0b0001_0000;
        const SIG_PENDING = 0b0010_0000;
        const P_STOP      = 0b0100_0000;
        const NO_PRIV     = 0b1000_0000;
        const NO_ENDPOINT = 0b0001_0000_0000;
        const VMINHIBIT   = 0b0010_0000_0000;
        const PAGEFAULT   = 0b0100_0000_0000;
        const VMREQUEST   = 0b1000_0000_0000;
        const VMREQTARGET = 0b0001_0000_0000_0000;
        const PREEMPTED   = 0b0100_0000_0000_0000;
        const NO_QUANTUM  = 0b1000_0000_0000_0000;
        const BOOTINHIBIT = 0b0001_0000_0000_0000_0000;
    }
}

bitflags::bitflags! {
    pub struct MiscFlags: u32 {
        const VIRT_TIMER      = 0b0000_0000_0010;
        const PROF_TIMER      = 0b0000_0000_0100;
        const SC_TRACE        = 0b0000_0100_0000;
        const STEP            = 0b0100_0000_0000;
        const FPU_INITIALIZED = 0b0001_0000_0000_0000;
    }
}

impl Proc {
    pub fn is_runnable(&self) -> bool {
        self.rts_flags == RtsFlags::empty()
    }
    
    pub fn is_empty(&self) -> bool {
        self.rts_flags.contains(RtsFlags::SLOT_FREE)
    }
}
```

### 9.2 类型安全的端点号

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Endpoint {
    generation: u16,
    slot: i16,
}

impl Endpoint {
    const GENERATION_SHIFT: u8 = 15;
    const MAX_GENERATION: u16 = (i32::MAX / (1 << Self::GENERATION_SHIFT) - 1) as u16;
    
    pub fn new(generation: u16, slot: i16) -> Self {
        Self { generation, slot }
    }
    
    pub fn from_raw(raw: i32) -> Self {
        Self {
            generation: ((raw + MAX_NR_TASKS as i32) >> Self::GENERATION_SHIFT) as u16,
            slot: (((raw + MAX_NR_TASKS as i32) & ((1 << Self::GENERATION_SHIFT) - 1)) - MAX_NR_TASKS as i32) as i16,
        }
    }
    
    pub fn to_raw(&self) -> i32 {
        ((self.generation as i32) << Self::GENERATION_SHIFT) + (self.slot as i32)
    }
    
    pub fn next_generation(&self) -> Self {
        let gen = if self.generation >= Self::MAX_GENERATION {
            1
        } else {
            self.generation + 1
        };
        Self::new(gen, self.slot)
    }
    
    pub fn generation(&self) -> u16 {
        self.generation
    }
    
    pub fn slot(&self) -> i16 {
        self.slot
    }
}
```

### 9.3 进程管理器抽象

```rust
pub struct ProcessManager {
    proctab: [Option<Proc>; NR_PROCS + NR_TASKS],
}

impl ProcessManager {
    pub fn fork(&mut self, parent: Endpoint, child_slot: ProcNr) -> Result<Endpoint, ForkError> {
        let parent_proc = self.get_proc(parent)?;
        let child_proc = self.get_slot_mut(child_slot)?;
        
        if !parent_proc.rts_flags.contains(RtsFlags::RECEIVING) {
            return Err(ForkError::NotReceiving);
        }
        
        let mut child = parent_proc.clone();
        
        let new_endpoint = parent_proc.endpoint.next_generation();
        child.endpoint = new_endpoint;
        child.nr = child_slot;
        
        child.regs.retreg = 0;
        
        child.virt_timer = None;
        child.prof_timer = None;
        child.misc_flags.remove(MiscFlags::VIRT_TIMER | MiscFlags::PROF_TIMER);
        
        child.rts_flags.insert(RtsFlags::NO_QUANTUM);
        child.rts_flags.remove(RtsFlags::SIGNALED | RtsFlags::SIG_PENDING | RtsFlags::P_STOP);
        
        *child_proc = Some(child);
        
        Ok(new_endpoint)
    }
    
    pub fn exec(&mut self, proc: Endpoint, ip: VirtAddr, sp: VirtAddr, name: &str) -> Result<(), ExecError> {
        let proc = self.get_proc_mut(proc)?;
        
        proc.regs.pc = ip;
        proc.regs.sp = sp;
        proc.name = ArrayString::from(name).map_err(|_| ExecError::NameTooLong)?;
        
        proc.fpu_state = None;
        proc.misc_flags.remove(MiscFlags::FPU_INITIALIZED);
        
        proc.rts_flags.remove(RtsFlags::RECEIVING);
        
        Ok(())
    }
    
    pub fn clear(&mut self, proc: Endpoint) -> Result<(), ClearError> {
        let slot = self.get_slot_mut(proc)?;
        
        if let Some(ref mut proc) = slot {
            proc.rts_flags.insert(RtsFlags::SLOT_FREE);
        }
        
        *slot = None;
        
        Ok(())
    }
    
    fn get_proc(&self, endpoint: Endpoint) -> Result<&Proc, ProcessError> {
        let slot = endpoint.slot() as usize;
        if slot >= NR_PROCS + NR_TASKS {
            return Err(ProcessError::InvalidEndpoint);
        }
        self.proctab[slot].as_ref().ok_or(ProcessError::ProcessNotFound)
    }
    
    fn get_proc_mut(&mut self, endpoint: Endpoint) -> Result<&mut Proc, ProcessError> {
        let slot = endpoint.slot() as usize;
        if slot >= NR_PROCS + NR_TASKS {
            return Err(ProcessError::InvalidEndpoint);
        }
        self.proctab[slot].as_mut().ok_or(ProcessError::ProcessNotFound)
    }
    
    fn get_slot_mut(&mut self, slot: ProcNr) -> Result<&mut Option<Proc>, ProcessError> {
        let idx = slot as usize;
        if idx >= NR_PROCS + NR_TASKS {
            return Err(ProcessError::InvalidSlot);
        }
        Ok(&mut self.proctab[idx])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkError {
    NotReceiving,
    InvalidEndpoint,
    SlotNotEmpty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecError {
    InvalidEndpoint,
    NameTooLong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearError {
    InvalidEndpoint,
    AlreadyCleared,
}
```

### 9.4 FPU 状态管理

```rust
pub struct FpuManager {
    fpu_owner: Option<Endpoint>,
}

impl FpuManager {
    pub fn save_fpu(&mut self, proc: &mut Proc) {
        if proc.misc_flags.contains(MiscFlags::FPU_INITIALIZED) {
            if let Some(ref owner) = self.fpu_owner {
                if *owner == proc.endpoint {
                    if let Some(ref mut fpu_state) = proc.fpu_state {
                        unsafe {
                            fxsave(fpu_state.as_mut_ptr());
                        }
                    }
                }
            }
        }
    }
    
    pub fn release_fpu(&mut self, proc: &Proc) {
        if let Some(ref owner) = self.fpu_owner {
            if *owner == proc.endpoint {
                self.fpu_owner = None;
            }
        }
    }
}

extern "C" {
    fn fxsave(ptr: *mut u8);
}
```

---

## 十、要点总结

| 系统调用 | 核心操作 | 关键数据结构 | 源码位置 |
|----------|----------|--------------|----------|
| fork | PCB 复制、端点代数递增 | `Endpoint`, `Proc`, `RtsFlags` | `do_fork.c:32-136` |
| exec | 设置 IP/SP、重置 FPU | `Registers`, `MiscFlags` | `do_exec.c:27-60` |
| exit | 发送 SIGABRT | 信号机制 | `do_exit.c:18-27` |
| kill | 发送信号 | `sigset_t`, `cause_sig` | `do_kill.c:18-41` |
| clear | 释放资源、标记 FREE | `RtsFlags::SLOT_FREE` | `do_clear.c:28-80` |
| getmcontext | 获取机器上下文 | `mcontext_t` | `do_mcontext.c:28-66` |
| setmcontext | 设置机器上下文 | `mcontext_t` | `do_mcontext.c:76-106` |

---

## 十一、灾难预演

### 11.1 如果 fork 不增加端点代数

```
后果：
1. 新进程使用旧端点号
2. 其他进程的旧引用指向新进程
3. IPC 消息发错进程
4. 系统行为不可预测
```

**源码验证**：`do_fork.c:79-82`

### 11.2 如果 exec 不重置 FPU

```
后果：
1. 新进程继承旧进程的 FPU 状态
2. 浮点运算结果错误
3. 数据损坏
```

**源码验证**：`do_exec.c:60-67`

### 11.3 如果 clear 不移除 IRQ 钩子

```
后果：
1. 中断发生时，钩子指向已释放的进程
2. 内核崩溃
```

**源码验证**：`do_clear.c:49-56`

### 11.4 如果 fork 不清空页表指针

```
后果：
1. 子进程使用父进程的页表
2. 地址空间隔离失效
3. 安全漏洞
```

**源码验证**：`do_fork.c:131-137`

---

**文档版本**: 2026-04-01
**源码版本**: Minix3
**最后更新**: 基于 minix/kernel/system/do_*.c 完整重写