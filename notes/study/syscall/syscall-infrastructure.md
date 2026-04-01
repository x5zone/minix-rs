# 系统调用基础设施

> **核心文件**: 
> - `minix/kernel/system.c` - 系统调用分发主逻辑
> - `minix/kernel/system.h` - 函数声明与宏定义
> - `minix/include/minix/com.h` - 系统调用编号定义

---

## 一、功能概述（What）

系统调用基础设施是内核与用户态服务之间的桥梁，负责：

1. **接收系统调用请求**：从用户态服务接收消息格式的系统调用
2. **验证调用合法性**：检查调用号和权限
3. **分发到处理函数**：调用对应的 `do_xxx()` 函数
4. **返回处理结果**：将结果复制回用户空间

---

## 二、设计动机（Why）

### 2.1 微内核架构的需求

Minix3 采用微内核架构，大部分操作系统服务运行在用户态：

```
用户态服务（PM, VFS, RS, 驱动程序等）
    ↓ 需要访问内核功能
内核（进程管理、内存管理、中断处理等）
```

系统调用基础设施提供**统一、安全、高效**的内核访问接口。

### 2.2 设计目标

| 目标 | 实现方式 |
|------|----------|
| **安全性** | 权限检查、调用号验证 |
| **高效性** | 函数指针数组，O(1) 分发 |
| **可扩展性** | 添加新调用只需一行 `map()` |
| **可靠性** | 编译期检查调用号合法性 |

---

## 三、使用场景（When）

### 3.1 系统调用触发时机

```
用户态服务调用 libc 函数
    ↓ 例如：fork(), exec(), read()
libc 封装为系统调用
    ↓ 构造消息，触发 trap
进入内核
    ↓ kernel_call() 处理
分发到具体处理函数
    ↓ do_fork(), do_exec(), do_devio()
返回结果
```

### 3.2 典型调用者

| 调用者 | 典型系统调用 | 用途 |
|--------|-------------|------|
| PM（进程管理器） | SYS_FORK, SYS_EXEC, SYS_EXIT | 进程生命周期管理 |
| VFS（文件系统） | SYS_SAFECOPYFROM, SYS_SAFECOPYTO | 安全内存拷贝 |
| RS（重启服务） | SYS_PRIVCTL, SYS_UPDATE | 服务管理与更新 |
| 驱动程序 | SYS_IRQCTL, SYS_DEVIO | 中断与设备 I/O |

---

## 四、核心数据结构

### 4.1 系统调用分发表

**定义位置**：`minix/kernel/system.c:52`

```c
static int (*call_vec[NR_SYS_CALLS])(struct proc * caller, message *m_ptr);
```

**数据结构分析**：

| 属性 | 值 | 说明 |
|------|-----|------|
| 类型 | 函数指针数组 | 每个元素指向一个处理函数 |
| 大小 | `NR_SYS_CALLS = 58` | 定义在 `minix/include/minix/com.h:270` |
| 索引 | `call_nr` | 系统调用号（从 0 开始） |
| 元素类型 | `int (*)(struct proc*, message*)` | 返回错误码，参数为调用者和消息 |

**内存布局**：

```
call_vec[58]  (约 464 字节，每个指针 8 字节)
┌─────────────────────────────────────────────────────────────┐
│ [0]  call_vec[0]  ──► do_fork()    或 NULL                  │
│ [1]  call_vec[1]  ──► do_exec()    或 NULL                  │
│ [2]  call_vec[2]  ──► do_clear()   或 NULL                  │
│ ...                                                         │
│ [57] call_vec[57] ──► do_padconf() 或 NULL                  │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 系统调用编号定义

**定义位置**：`minix/include/minix/com.h:205-270`

```c
#define KERNEL_CALL	0x600	/* 系统调用基地址 */

#define SYS_FORK       (KERNEL_CALL + 0)	/* 进程创建 */
#define SYS_EXEC       (KERNEL_CALL + 1)	/* 执行程序 */
#define SYS_CLEAR      (KERNEL_CALL + 2)	/* 清理进程 */
#define SYS_SCHEDULE   (KERNEL_CALL + 3)	/* 调度进程 */
#define SYS_PRIVCTL    (KERNEL_CALL + 4)	/* 特权控制 */
#define SYS_TRACE      (KERNEL_CALL + 5)	/* 进程跟踪 */
#define SYS_KILL       (KERNEL_CALL + 6)	/* 发送信号 */
// ... 共 58 个系统调用

#define NR_SYS_CALLS	58	/* 系统调用总数 */
```

**设计要点**：

1. **基地址 `KERNEL_CALL = 0x600`**：
   - 与其他消息类型区分（如 IPC 消息类型从 0x000 开始）
   - 便于快速判断消息类型

2. **连续编号**：
   - 从 `KERNEL_CALL + 0` 到 `KERNEL_CALL + 57`
   - 数组索引 = 调用号 - `KERNEL_CALL`

3. **编号间隙**：
   - 部分编号未使用（如 `KERNEL_CALL + 11, +12`）
   - 对应的 `call_vec` 元素为 `NULL`

### 4.3 消息结构

**定义位置**：`minix/include/minix/ipc.h`

```c
typedef struct {
    int m_source;           /* 发送者端点 */
    int m_type;             /* 消息类型（系统调用号） */
    union {
        int m1_i1, m1_i2, m1_i3;      /* 整数参数 */
        long m1_l1, m1_l2;
        char *m1_p1, *m1_p2;
        // ... 其他字段组合
    } m_u;
} message;
```

**系统调用参数传递**：

```
用户态调用：
    sys_fork(parent_ep, child_slot)

消息构造：
    msg.m_type = SYS_FORK           // 调用号
    msg.m_lsys_krn_sys_fork.endpt = parent_ep
    msg.m_lsys_krn_sys_fork.slot = child_slot
```

---

## 五、核心函数逐行分析

### 5.1 kernel_call() - 系统调用总入口

**定义位置**：`minix/kernel/system.c:121-151`

**函数签名**：

```c
void kernel_call(message *m_user, struct proc * caller)
```

**参数说明**：

| 参数 | 类型 | 说明 |
|------|------|------|
| `m_user` | `message*` | 用户空间的系统调用消息地址 |
| `caller` | `struct proc*` | 调用者的进程控制块指针 |

**逐行分析**：

```c
void kernel_call(message *m_user, struct proc * caller)
{
  int result = OK;
  message msg;

  /* 第1步：保存用户消息地址 */
  caller->p_delivermsg_vir = (vir_bytes) m_user;
  
  /*
   * 第2步：从用户空间拷贝消息
   * 
   * 为什么需要拷贝？
   * - 用户空间地址不可信，可能无效
   * - 内核需要访问消息内容
   * - copy_msg_from_user() 会检查地址有效性
   * 
   * 调用者进程的 ldt 和 cr3 已加载：
   * - 刚 trap 进入内核，或
   * - switch_to_user() 已设置
   */
  if (copy_msg_from_user(m_user, &msg) == 0) {
      /* 拷贝成功，设置消息来源 */
      msg.m_source = caller->p_endpoint;
      
      /* 第3步：分发系统调用 */
      result = kernel_call_dispatch(caller, &msg);
  }
  else {
      /* 拷贝失败：用户指针无效 */
      printf("WARNING wrong user pointer 0x%08x from process %s / %d\n",
              m_user, caller->p_name, caller->p_endpoint);
      
      /* 向进程发送 SIGSEGV 信号 */
      cause_sig(proc_nr(caller), SIGSEGV);
      return;
  }

  /* 第4步：记录调用者（用于时间计费） */
  kbill_kcall = caller;

  /* 第5步：完成系统调用 */
  kernel_call_finish(caller, &msg, result);
}
```

**关键点**：

1. **消息拷贝的必要性**：
   - 用户空间地址不可信
   - 内核需要稳定的消息副本
   - 拷贝失败会导致 `SIGSEGV`

2. **设置消息来源**：
   - `msg.m_source = caller->p_endpoint`
   - 防止伪造消息来源

3. **错误处理**：
   - 拷贝失败立即返回
   - 发送 `SIGSEGV` 信号

### 5.2 kernel_call_dispatch() - 系统调用分发

**定义位置**：`minix/kernel/system.c:92-118`

**函数签名**：

```c
static int kernel_call_dispatch(struct proc * caller, message *msg)
```

**逐行分析**：

```c
static int kernel_call_dispatch(struct proc * caller, message *msg)
{
  int result = OK;
  int call_nr;

#if DEBUG_IPC_HOOK
  hook_ipc_msgkcall(msg, caller);
#endif
  
  /* 第1步：提取调用号 */
  call_nr = msg->m_type - KERNEL_CALL;
  
  /*
   * 为什么减去 KERNEL_CALL？
   * - msg->m_type = SYS_FORK = KERNEL_CALL + 0 = 0x600
   * - call_nr = 0x600 - 0x600 = 0
   * - call_vec[0] 就是 do_fork
   */

  /* 第2步：边界检查 */
  if (call_nr < 0 || call_nr >= NR_SYS_CALLS) {
      printf("SYSTEM: illegal request %d from %d.\n",
              call_nr, msg->m_source);
      result = EBADREQUEST;  /* 无效的调用号 */
  }
  /* 第3步：权限检查 */
  else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr)) {
      /*
       * 检查调用者是否有权调用此系统调用
       * 
       * s_k_call_mask 是一个位图：
       * - 每一位对应一个系统调用
       * - 1 表示允许调用，0 表示禁止
       * 
       * 例如：
       * - 用户进程：只能调用基本系统调用
       * - 系统进程：可以调用更多系统调用
       */
      printf("SYSTEM: denied request %d from %d.\n",
              call_nr, msg->m_source);
      result = ECALLDENIED;  /* 权限不足 */
  } 
  /* 第4步：调用处理函数 */
  else {
      if (call_vec[call_nr])
          result = (*call_vec[call_nr])(caller, msg);
      else {
          /*
           * call_vec[call_nr] == NULL
           * 
           * 可能原因：
           * 1. 系统调用未实现
           * 2. 编译时被禁用（如 #if !USE_FORK）
           * 3. 编号未分配
           */
          printf("Unused kernel call %d from %d\n",
                  call_nr, caller->p_endpoint);
          result = EBADREQUEST;
      }
  }

  return result;
}
```

**权限检查详解**：

```c
/* s_k_call_mask 定义 */
typedef struct {
    bitchunk_t chunk[SYS_CALL_MASK_SIZE];
} sys_map_t;

/* GET_BIT 宏展开 */
#define GET_BIT(map, bit) \
    ( MAP_CHUNK((map).chunk, bit) & (1 << CHUNK_OFFSET(bit)) )

/* 示例：检查 SYS_FORK 权限 */
call_nr = 0;  /* SYS_FORK - KERNEL_CALL */
GET_BIT(priv(caller)->s_k_call_mask, 0)
  = priv(caller)->s_k_call_mask.chunk[0] & (1 << 0)
  = 检查第 0 位是否为 1
```

### 5.3 kernel_call_finish() - 结果返回

**定义位置**：`minix/kernel/system.c:58-89`

**函数签名**：

```c
static void kernel_call_finish(struct proc * caller, message *msg, int result)
```

**逐行分析**：

```c
static void kernel_call_finish(struct proc * caller, message *msg, int result)
{
  if(result == VMSUSPEND) {
      /*
       * 特殊情况：系统调用被 VM 挂起
       * 
       * 何时发生？
       * - 系统调用需要访问用户内存
       * - 用户内存被换出（swapped out）
       * - VM 需要先换入页面
       * 
       * 处理流程：
       * 1. VM 已被通知
       * 2. 保存消息，等待 VM 响应
       * 3. VM 响应后，重新执行系统调用
       */
      assert(RTS_ISSET(caller, RTS_VMREQUEST));
      assert(caller->p_vmrequest.type == VMSTYPE_KERNELCALL);
      
      /* 保存请求消息 */
      caller->p_vmrequest.saved.reqmsg = *msg;
      
      /* 标记需要恢复系统调用 */
      caller->p_misc_flags |= MF_KCALL_RESUME;
  } 
  else {
      /*
       * 正常情况：系统调用完成
       * 
       * 清除保存的消息（如果之前被挂起过）
       */
      caller->p_vmrequest.saved.reqmsg.m_source = NONE;
      
      /* 如果需要回复 */
      if (result != EDONTREPLY) {
          /* 设置回复消息 */
          msg->m_source = SYSTEM;      /* 来源：SYSTEM 任务 */
          msg->m_type = result;        /* 类型：错误码 */
          
#if DEBUG_IPC_HOOK
          hook_ipc_msgkresult(msg, caller);
#endif
          
          /* 将结果复制到用户空间 */
          if (copy_msg_to_user(msg, (message *)caller->p_delivermsg_vir)) {
              /*
               * 复制失败：用户指针无效
               * 
               * 这是一种严重错误：
               * - 用户空间的栈可能被破坏
               * - 或者用户程序传递了错误的指针
               */
              printf("WARNING wrong user pointer 0x%08x from "
                      "process %s / %d\n",
                      caller->p_delivermsg_vir,
                      caller->p_name,
                      caller->p_endpoint);
              
              /* 发送 SIGSEGV 信号 */
              cause_sig(proc_nr(caller), SIGSEGV);
          }
      }
      /*
       * result == EDONTREPLY
       * 
       * 不需要回复的情况：
       * - 某些系统调用不需要返回值
       * - 例如：SYS_EXIT（进程退出）
       */
  }
}
```

**VMSUSPEND 处理流程**：

```
系统调用执行
    ↓
需要访问用户内存
    ↓
用户内存被换出
    ↓
返回 VMSUSPEND
    ↓
保存消息，设置 MF_KCALL_RESUME
    ↓
进程等待 VM 响应
    ↓
VM 换入页面，发送响应
    ↓
内核重新执行系统调用
```

### 5.4 system_init() - 初始化分发表

**定义位置**：`minix/kernel/system.c:154-250`

**函数签名**：

```c
void system_init(void)
```

**逐行分析**：

```c
void system_init(void)
{
  register struct priv *sp;
  int i;

  /* 第1步：初始化 IRQ 钩子 */
  for (i=0; i<NR_IRQ_HOOKS; i++) {
      irq_hooks[i].proc_nr_e = NONE;  /* 标记为未使用 */
  }

  /* 第2步：初始化所有进程的闹钟定时器 */
  for (sp=BEG_PRIV_ADDR; sp < END_PRIV_ADDR; sp++) {
    tmr_inittimer(&(sp->s_alarm_timer));
  }

  /* 第3步：初始化分发表为 NULL */
  for (i=0; i<NR_SYS_CALLS; i++) {
      call_vec[i] = NULL;
  }

  /* 第4步：注册系统调用处理函数 */
  
  /* 进程管理 */
  map(SYS_FORK, do_fork);        /* 进程创建 */
  map(SYS_EXEC, do_exec);        /* 执行程序 */
  map(SYS_CLEAR, do_clear);      /* 清理进程 */
  map(SYS_EXIT, do_exit);        /* 进程退出 */
  map(SYS_PRIVCTL, do_privctl);  /* 特权控制 */
  map(SYS_TRACE, do_trace);      /* 进程跟踪 */
  map(SYS_SETGRANT, do_setgrant);/* 设置授权 */
  map(SYS_RUNCTL, do_runctl);    /* 运行控制 */
  map(SYS_UPDATE, do_update);    /* 进程更新 */
  map(SYS_STATECTL, do_statectl);/* 状态控制 */

  /* 信号处理 */
  map(SYS_KILL, do_kill);        /* 发送信号 */
  map(SYS_GETKSIG, do_getksig);  /* 获取信号 */
  map(SYS_ENDKSIG, do_endksig);  /* 结束信号处理 */
  map(SYS_SIGSEND, do_sigsend);  /* 发送 POSIX 信号 */
  map(SYS_SIGRETURN, do_sigreturn);/* 信号返回 */

  /* 设备 I/O */
  map(SYS_IRQCTL, do_irqctl);    /* 中断控制 */
#if defined(__i386__)
  map(SYS_DEVIO, do_devio);      /* 单端口 I/O */
  map(SYS_VDEVIO, do_vdevio);    /* 批量 I/O */
#endif

  /* 内存管理 */
  map(SYS_MEMSET, do_memset);    /* 内存填充 */
  map(SYS_VMCTL, do_vmctl);      /* VM 控制 */

  /* 地址映射 */
  map(SYS_UMAP, do_umap);        /* 虚拟地址映射 */
  map(SYS_UMAP_REMOTE, do_umap_remote);/* 远程映射 */
  map(SYS_VUMAP, do_vumap);      /* 向量映射 */
  map(SYS_VIRCOPY, do_vircopy);  /* 虚拟拷贝 */
  map(SYS_PHYSCOPY, do_copy);    /* 物理拷贝 */
  map(SYS_SAFECOPYFROM, do_safecopy_from);/* 安全拷贝（从） */
  map(SYS_SAFECOPYTO, do_safecopy_to);    /* 安全拷贝（到） */
  map(SYS_VSAFECOPY, do_vsafecopy);       /* 向量安全拷贝 */
  map(SYS_SAFEMEMSET, do_safememset);     /* 安全填充 */

  /* 时钟功能 */
  map(SYS_TIMES, do_times);      /* 获取时间 */
  map(SYS_SETALARM, do_setalarm);/* 设置闹钟 */
  map(SYS_STIME, do_stime);      /* 设置启动时间 */
  map(SYS_SETTIME, do_settime);  /* 设置系统时间 */
  map(SYS_VTIMER, do_vtimer);    /* 虚拟定时器 */

  /* 系统控制 */
  map(SYS_ABORT, do_abort);      /* 中止系统 */
  map(SYS_GETINFO, do_getinfo);  /* 获取系统信息 */
  map(SYS_DIAGCTL, do_diagctl);  /* 诊断控制 */

  /* 性能分析 */
  map(SYS_SPROF, do_sprofile);   /* 统计性能分析 */

  /* 架构相关 */
#if defined(__arm__)
  map(SYS_PADCONF, do_padconf);  /* 引脚配置 */
#endif

#if defined(__i386__)
  map(SYS_READBIOS, do_readbios);/* 读取 BIOS */
  map(SYS_IOPENABLE, do_iopenable);/* 启用 I/O */
  map(SYS_SDEVIO, do_sdevio);    /* 安全设备 I/O */
#endif

  /* 机器上下文 */
  map(SYS_SETMCONTEXT, do_setmcontext);/* 设置上下文 */
  map(SYS_GETMCONTEXT, do_getmcontext);/* 获取上下文 */

  /* 调度 */
  map(SYS_SCHEDULE, do_schedule);/* 调度进程 */
  map(SYS_SCHEDCTL, do_schedctl);/* 调度控制 */
}
```

**map() 宏展开**：

```c
#define map(call_nr, handler)                                          \
    {   int call_index = call_nr-KERNEL_CALL;                          \
        assert(call_index >= 0 && call_index < NR_SYS_CALLS);          \
        call_vec[call_index] = (handler); }

/* 示例：map(SYS_FORK, do_fork) 展开为 */
{
    int call_index = SYS_FORK - KERNEL_CALL;  /* = 0x600 - 0x600 = 0 */
    assert(call_index >= 0 && call_index < 58);
    call_vec[0] = do_fork;
}
```

**编译期检查**：

- `assert()` 在编译期检查调用号合法性
- 如果 `call_index < 0` 或 `>= NR_SYS_CALLS`，编译失败
- 确保不会注册无效的系统调用

---

## 六、辅助函数分析

### 6.1 send_sig() - 向系统进程发送信号

**定义位置**：`minix/kernel/system.c:367-385`

**函数签名**：

```c
int send_sig(endpoint_t ep, int sig_nr)
```

**功能**：向系统进程发送信号通知

**逐行分析**：

```c
int send_sig(endpoint_t ep, int sig_nr)
{
  register struct proc *rp;
  struct priv *priv;
  int proc_nr;

  /* 第1步：验证端点号 */
  if(!isokendpt(ep, &proc_nr) || isemptyn(proc_nr))
      return EINVAL;

  /* 第2步：获取进程和特权结构 */
  rp = proc_addr(proc_nr);
  priv = priv(rp);
  if(!priv) return ENOENT;
  
  /* 第3步：设置挂起信号 */
  sigaddset(&priv->s_sig_pending, sig_nr);
  
  /* 第4步：发送通知 */
  mini_notify(proc_addr(SYSTEM), rp->p_endpoint);

  return OK;
}
```

**使用场景**：

| 调用者 | 信号 | 场景 |
|--------|------|------|
| 内核 | SIGSEGV | 内存访问错误 |
| TTY | SIGINT | 用户按下 Ctrl+C |
| FS | SIGPIPE | 管道破裂 |

### 6.2 cause_sig() - 处理用户进程信号

**定义位置**：`minix/kernel/system.c:387-424`

**函数签名**：

```c
void cause_sig(proc_nr_t proc_nr, int sig_nr)
```

**功能**：向用户进程发送信号，通知信号管理器（通常是 PM）

**逐行分析**：

```c
void cause_sig(proc_nr_t proc_nr, int sig_nr)
{
  register struct proc *rp;
  struct priv *priv;

  rp = proc_addr(proc_nr);

  /* 第1步：设置挂起信号位图 */
  sigaddset(&rp->p_pending, sig_nr);
  
  /* 第2步：设置 RTS_SIGNALED 标志 */
  RTS_SET(rp, RTS_SIGNALED);
  
  /* 第3步：通知信号管理器 */
  priv = priv(rp);
  if (priv->s_sig_mgr != SELF) {
      /* 信号管理器不是自己，发送通知 */
      if (!RTS_ISSET(proc_addr(priv->s_sig_mgr), RTS_NO_PRIV)) {
          mini_notify(proc_addr(SYSTEM), priv->s_sig_mgr);
      }
  }
}
```

**与 send_sig() 的区别**：

| 函数 | 目标进程 | 信号管理器 | 通知方式 |
|------|---------|-----------|---------|
| `send_sig()` | 系统进程 | 自己 | 直接通知进程 |
| `cause_sig()` | 用户进程 | PM | 通知信号管理器 |

### 6.3 get_priv() - 分配特权结构

**定义位置**：`minix/kernel/system.c:252-277`

**函数签名**：

```c
int get_priv(register struct proc *rc, int priv_id)
```

**功能**：为进程分配特权结构

**逐行分析**：

```c
int get_priv(register struct proc *rc, int priv_id)
{
  register struct priv *sp;

  if(priv_id == NULL_PRIV_ID) {
      /* 动态分配：从动态特权槽位池中查找空闲槽位 */
      for (sp = BEG_DYN_PRIV_ADDR; sp < END_DYN_PRIV_ADDR; ++sp)
          if (sp->s_proc_nr == NONE) break;
      
      if (sp >= END_DYN_PRIV_ADDR)
          return ENOSPC;  /* 无可用槽位 */
  }
  else {
      /* 静态分配：使用指定的特权 ID */
      if(!is_static_priv_id(priv_id)) {
          return EINVAL;  /* 无效的静态特权 ID */
      }
      if(priv[priv_id].s_proc_nr != NONE) {
          return EBUSY;   /* 槽位已被占用 */
      }
      sp = &priv[priv_id];
  }
  
  /* 建立进程与特权结构的关联 */
  rc->p_priv = sp;
  rc->p_priv->s_proc_nr = proc_nr(rc);
  
  return OK;
}
```

**静态 vs 动态分配**：

| 分配方式 | priv_id | 使用场景 | 示例 |
|---------|---------|---------|------|
| 静态 | 0-15 | 系统核心服务 | PM, VFS, RS |
| 动态 | NULL_PRIV_ID | 动态创建的服务 | 用户启动的驱动 |

---

## 七、调用链分析

### 7.1 完整的系统调用流程

```
用户态服务调用系统调用
    ↓
libc 封装函数
    │ 例如：sys_fork(parent_ep, child_slot)
    ↓
构造消息，触发 trap
    │ msg.m_type = SYS_FORK
    │ msg.m_lsys_krn_sys_fork.endpt = parent_ep
    │ msg.m_lsys_krn_sys_fork.slot = child_slot
    ↓
进入内核（trap handler）
    ↓
kernel_call(m_user, caller)
    │
    ├─► copy_msg_from_user(m_user, &msg)
    │       └─► 检查用户指针有效性
    │
    ├─► kernel_call_dispatch(caller, &msg)
    │       │
    │       ├─► call_nr = msg->m_type - KERNEL_CALL
    │       │
    │       ├─► 边界检查：call_nr < 0 || call_nr >= NR_SYS_CALLS
    │       │
    │       ├─► 权限检查：GET_BIT(priv(caller)->s_k_call_mask, call_nr)
    │       │
    │       └─► result = call_vec[call_nr](caller, msg)
    │               └─► do_fork(caller, msg)
    │
    └─► kernel_call_finish(caller, &msg, result)
            │
            ├─► if (result == VMSUSPEND)
            │       └─► 保存消息，等待 VM 响应
            │
            └─► else if (result != EDONTREPLY)
                    └─► copy_msg_to_user(msg, user_buffer)
```

### 7.2 错误处理路径

```
错误类型 1：用户指针无效
    ↓
copy_msg_from_user() 失败
    ↓
cause_sig(caller, SIGSEGV)
    ↓
进程收到 SIGSEGV 信号

错误类型 2：调用号无效
    ↓
call_nr < 0 || call_nr >= NR_SYS_CALLS
    ↓
返回 EBADREQUEST

错误类型 3：权限不足
    ↓
!GET_BIT(priv(caller)->s_k_call_mask, call_nr)
    ↓
返回 ECALLDENIED

错误类型 4：处理函数未注册
    ↓
call_vec[call_nr] == NULL
    ↓
返回 EBADREQUEST

错误类型 5：用户内存被换出
    ↓
系统调用返回 VMSUSPEND
    ↓
保存消息，等待 VM 响应
    ↓
VM 换入页面后重新执行
```

---

## 八、关键宏定义

### 8.1 map() 宏

**定义位置**：`minix/kernel/system.c:55-58`

```c
#define map(call_nr, handler)                                          \
    {   int call_index = call_nr-KERNEL_CALL;                          \
        assert(call_index >= 0 && call_index < NR_SYS_CALLS);          \
        call_vec[call_index] = (handler); }
```

**作用**：

1. 计算数组索引：`call_index = call_nr - KERNEL_CALL`
2. 编译期检查：`assert()` 确保索引合法
3. 注册处理函数：`call_vec[call_index] = handler`

### 8.2 GET_BIT() 宏

**定义位置**：`minix/kernel/const.h:17-18`

```c
#define get_sys_bit(map,bit) \
	( MAP_CHUNK((map).chunk,bit) & (1 << CHUNK_OFFSET(bit) ))
```

**作用**：检查位图中的某一位是否为 1

**展开示例**：

```c
/* 检查调用者是否有权调用 SYS_FORK (call_nr = 0) */
GET_BIT(priv(caller)->s_k_call_mask, 0)
  = MAP_CHUNK(priv(caller)->s_k_call_mask.chunk, 0) & (1 << CHUNK_OFFSET(0))
  = priv(caller)->s_k_call_mask.chunk[0] & (1 << 0)
  = 检查第 0 位
```

### 8.3 RTS_SET() 宏

**定义位置**：`minix/kernel/proc.h`

```c
#define RTS_SET(rp, f)                          \
    do {                                        \
        const int rts = (rp)->p_rts_flags;      \
        (rp)->p_rts_flags |= (f);               \
        if(rts_f_is_runnable(rts) && !proc_is_runnable(rp)) { \
            dequeue(rp);                        \
        }                                       \
    } while(0)
```

**作用**：设置进程运行时状态标志，自动管理调度队列

**自动调度逻辑**：

```
设置标志前：进程可运行（在运行队列中）
    ↓
设置标志后：进程不可运行
    ↓
自动从运行队列中移除
```

---

## 九、设计模式分析

### 9.1 函数指针数组分发

**优点**：

| 优点 | 说明 |
|------|------|
| **高效** | O(1) 时间复杂度，无 switch-case 开销 |
| **可扩展** | 添加新调用只需一行 `map()` |
| **编译期检查** | `assert()` 确保调用号合法 |
| **易于维护** | 处理函数独立文件，职责清晰 |

**对比 switch-case**：

```c
/* 传统方式 */
switch (call_nr) {
    case SYS_FORK: result = do_fork(caller, msg); break;
    case SYS_EXEC: result = do_exec(caller, msg); break;
    // ... 58 个 case
}

/* 函数指针方式 */
result = call_vec[call_nr](caller, msg);  /* 一行搞定 */
```

### 9.2 静态内存分配

**特点**：

- 所有数据结构在编译期分配
- 无动态内存分配
- 确定性高，无碎片问题

**示例**：

```c
/* 系统调用分发表 */
static int (*call_vec[NR_SYS_CALLS])(struct proc*, message*);

/* 进程表 */
EXTERN struct proc proc[NR_TASKS + NR_PROCS];

/* 特权表 */
EXTERN struct priv priv[NR_SYS_PROCS];
```

---

## 十、注意事项与易错点

### 10.1 用户指针验证

**问题**：用户空间地址不可信

**解决**：

```c
/* 错误：直接访问用户指针 */
message *msg = m_user;  /* 危险！ */

/* 正确：通过拷贝函数验证 */
if (copy_msg_from_user(m_user, &msg) == 0) {
    /* 安全访问 msg */
}
```

### 10.2 权限检查顺序

**正确顺序**：

1. 边界检查（`call_nr < 0 || call_nr >= NR_SYS_CALLS`）
2. 权限检查（`GET_BIT(priv(caller)->s_k_call_mask, call_nr)`）
3. 空指针检查（`call_vec[call_nr] != NULL`）

**原因**：

- 边界检查防止数组越界
- 权限检查防止权限提升
- 空指针检查防止调用未实现的系统调用

### 10.3 VMSUSPEND 处理

**问题**：系统调用可能被挂起

**解决**：

```c
if (result == VMSUSPEND) {
    /* 保存消息，等待 VM 响应 */
    caller->p_vmrequest.saved.reqmsg = *msg;
    caller->p_misc_flags |= MF_KCALL_RESUME;
}
```

**注意**：

- 不要立即返回结果
- 必须保存消息以便恢复
- VM 响应后会重新执行系统调用

### 10.4 错误码使用

**常见错误码**：

| 错误码 | 含义 | 使用场景 |
|--------|------|---------|
| `EBADREQUEST` | 无效的请求 | 调用号无效或未实现 |
| `ECALLDENIED` | 权限不足 | 无权调用此系统调用 |
| `EDONTREPLY` | 不需要回复 | 某些系统调用不需要返回值 |
| `VMSUSPEND` | 被 VM 挂起 | 需要等待 VM 换入页面 |

---

## 十一、模块级 Rust 重构建议

### 11.1 类型安全的系统调用分发

```rust
pub trait SyscallHandler {
    fn handle(&self, caller: &mut Proc, msg: &Message) -> Result<(), SyscallError>;
}

pub struct SyscallDispatcher {
    handlers: [Option<&'static dyn SyscallHandler>; NR_SYS_CALLS],
}

impl SyscallDispatcher {
    pub const fn new() -> Self {
        Self {
            handlers: [None; NR_SYS_CALLS],
        }
    }
    
    pub fn dispatch(
        &self,
        call_nr: usize,
        caller: &mut Proc,
        msg: &Message,
    ) -> Result<(), SyscallError> {
        let handler = self.handlers.get(call_nr)
            .and_then(|h| h.as_ref())
            .ok_or(SyscallError::InvalidCallNumber)?;
        
        handler.handle(caller, msg)
    }
    
    pub const fn register(&mut self, call_nr: usize, handler: &'static dyn SyscallHandler) {
        self.handlers[call_nr] = Some(handler);
    }
}
```

### 11.2 类型安全的权限检查

```rust
bitflags::bitflags! {
    pub struct SyscallMask: u64 {
        const FORK = 1 << 0;
        const EXEC = 1 << 1;
        const CLEAR = 1 << 2;
        // ...
    }
}

impl Privilege {
    pub fn can_call(&self, syscall: SyscallMask) -> bool {
        self.syscall_mask.contains(syscall)
    }
}
```

### 11.3 错误处理

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyscallError {
    InvalidCallNumber,
    PermissionDenied,
    NotImplemented,
    VmSuspend,
    UserPointerInvalid,
}

impl From<SyscallError> for i32 {
    fn from(err: SyscallError) -> i32 {
        match err {
            SyscallError::InvalidCallNumber => EBADREQUEST,
            SyscallError::PermissionDenied => ECALLDENIED,
            SyscallError::NotImplemented => EBADREQUEST,
            SyscallError::VmSuspend => VMSUSPEND,
            SyscallError::UserPointerInvalid => EFAULT,
        }
    }
}
```

---

## 十二、要点总结

| 概念 | 要点 |
|------|------|
| **分发机制** | 函数指针数组，O(1) 时间复杂度 |
| **调用号** | `KERNEL_CALL = 0x600`，共 58 个系统调用 |
| **权限检查** | `s_k_call_mask` 位图，每位对应一个系统调用 |
| **错误处理** | 用户指针验证、边界检查、权限检查 |
| **VMSUSPEND** | 用户内存被换出时挂起，等待 VM 响应 |
| **编译期检查** | `map()` 宏中的 `assert()` 确保调用号合法 |

---

## 十三、灾难预演

### 13.1 如果 call_vec 未初始化

**后果**：

1. 调用 `NULL` 函数指针
2. 内核崩溃，系统重启

**预防**：

```c
/* system_init() 中初始化所有元素为 NULL */
for (i=0; i<NR_SYS_CALLS; i++) {
    call_vec[i] = NULL;
}
```

### 13.2 如果权限检查遗漏

**后果**：

1. 用户进程获得系统进程权限
2. 可以调用任意系统调用
3. 系统安全被破坏

**预防**：

```c
/* kernel_call_dispatch() 中强制检查 */
else if (!GET_BIT(priv(caller)->s_k_call_mask, call_nr)) {
    result = ECALLDENIED;
}
```

### 13.3 如果用户指针未验证

**后果**：

1. 内核访问无效地址
2. 内核崩溃或安全漏洞

**预防**：

```c
/* kernel_call() 中验证用户指针 */
if (copy_msg_from_user(m_user, &msg) == 0) {
    /* 安全访问 */
} else {
    cause_sig(proc_nr(caller), SIGSEGV);
    return;
}
```

---

**文档版本**: 2026-04-01
**源码版本**: Minix 3.4.0
