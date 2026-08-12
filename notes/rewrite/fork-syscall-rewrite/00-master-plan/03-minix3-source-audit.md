# Minix3 源码审计

## 1. 源码文件映射

### 1.1 Fork系统调用涉及函数列表（执行顺序）

#### 1.1.1 用户态入口层

| 执行顺序 | 函数名 | 路径 | 内部调用/宏 | 定义位置 |
| --- | --- | --- | --- | --- |
| 1 | fork | /minix3/lib/libc/gen/pthread_atfork.c:151 | __weak_alias宏 | /minix3/lib/libc/include/namespace.h |
| | | | SIMPLEQ_*系列宏 | /minix3/include/sys/queue.h |
| | | | MUTEX_INITIALIZER宏 | /minix3/lib/libc/include/reentrant.h |
| | | | mutex_lock/mutex_unlock/mutex_init | /minix3/lib/libc/thread-stub/ |
| | | | malloc/free | /minix3/lib/libc/stdlib/ |
| | | | __fork | /minix3/lib/libc/arch/${arch}/sys/__fork.S（架构相关） |

#### 1.1.2 PM（进程管理器）层

| 执行顺序 | 函数名 | 路径 | 内部调用/宏 | 定义位置 |
| --- | --- | --- | --- | --- |
| 2 | do_fork(PM层) | /minix3/minix/servers/pm/forkexit.c:45 | NR_PROCS宏 | /minix3/include/minix/config.h |
| | | | IN_USE宏 | /minix3/minix/servers/pm/mproc.h |
| | | | panic宏 | /minix3/include/minix/com.h |
| | | | vm_fork | /minix3/minix/servers/vm/fork.c |
| | | | get_free_pid | /minix3/minix/servers/pm/misc.c |
| | | | tell_vfs | /minix3/minix/servers/pm/vfs.c |
| | | | sig_proc | /minix3/minix/servers/pm/signal.c |

#### 1.1.3 VM（虚拟内存管理器）层

| 执行顺序 | 函数名 | 路径 | 内部调用/宏 | 定义位置 |
| --- | --- | --- | --- | --- |
| 3 | do_fork(VM层) | /minix3/minix/servers/vm/fork.c:32 | SANITYCHECK宏 | /minix3/minix/servers/vm/sanitycheck.h |
| | | | PFF_VMINHIBIT宏 | /minix3/include/minix/syslib.h |
| | | | vm_isokendpt | /minix3/minix/servers/vm/vmproc.c |
| | | | region_init | /minix3/minix/servers/vm/region.c |
| | | | pt_new/pt_free/pt_bind | /minix3/minix/servers/vm/pt.c |
| | | | map_proc_copy | /minix3/minix/servers/vm/region.c |
| | | | acl_fork | /minix3/minix/servers/vm/acl.c |
| | | | sys_fork | /minix3/minix/lib/libsys/sys_fork.c |
| | | | handle_memory_once | /minix3/minix/servers/vm/util.c |

#### 1.1.4 系统调用库层

| 执行顺序 | 函数名 | 路径 | 内部调用/宏 | 定义位置 |
| --- | --- | --- | --- | --- |
| 4 | sys_fork | /minix3/minix/lib/libsys/sys_fork.c:3 | SYS_FORK宏 | /minix3/include/minix/callnr.h |
| | | | 消息结构宏m_*_sys_fork.* | /minix3/include/minix/com.h |
| | | | _kernel_call | /minix3/minix/lib/libsys/arch/${arch}/kernel_call.S（架构相关） |

#### 1.1.5 内核层

| 执行顺序 | 函数名 | 路径 | 内部调用/宏 | 定义位置 |
| --- | --- | --- | --- | --- |
| 5 | do_fork(内核层) | /minix3/minix/kernel/system/do_fork.c:28 | isokendpt/_ENDPOINT_*系列宏 | /minix3/include/minix/endpoint.h |
| | | | proc_addr/isemptyp宏 | /minix3/minix/kernel/proc.h |
| | | | RTS_*系列宏 | /minix3/minix/kernel/proc.h |
| | | | FPU_XFP_SIZE宏 | /minix3/include/machine/vm.h（架构相关） |
| | | | save_fpu | /minix3/minix/kernel/arch/${arch}/fpu.c（架构相关） |
| | | | memcpy/strcat/strlen | /minix3/lib/libc/string/ |
| | | | reset_proc_accounting | /minix3/minix/kernel/accounting.c |
| | | | cpuavg_init | /minix3/minix/kernel/sched.c |
| | | | sigemptyset | /minix3/lib/libc/signal/ |

## 2. 关键数据结构

> **详细设计**: [01-stage-pm/mproc-design.md](../01-stage-pm/mproc-design.md)

## 3. 核心函数分析

> **详细分析**: [deep-analysis/fork-all-layers-deep-analysis.md](../deep-analysis/fork-all-layers-deep-analysis.md)

## 4. 算法实现细节

> **详细实现**: 各阶段指南 (06-phase1-pm-guide.md 等)

## 5. 常量与宏定义

> **详细参考**: [10-constants-reference.md](./10-constants-reference.md)

## 6. 与Rust实现的差异

> **设计决策**: [11-decision-records.md](./11-decision-records.md)
