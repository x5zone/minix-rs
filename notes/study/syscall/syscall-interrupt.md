# 中断设备系统调用

> **模块定位**: 硬件中断管理与设备 I/O 操作
> 
> **核心文件**:
> - `minix3/minix/kernel/system/do_irqctl.c` (175行)
> - `minix3/minix/kernel/system/do_devio.c` (108行)
> - `minix3/minix/kernel/system/do_vdevio.c` (166行)

---

## 模块架构总览

### 三个系统调用的协作关系

```
┌─────────────────────────────────────────────────────────────────────┐
│                     中断设备模块全景图                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  硬件中断管理                                                │  │
│  │  ┌────────────────┐        ┌────────────────┐               │  │
│  │  │ do_irqctl      │───────►│ 中断策略管理   │               │  │
│  │  │ (SYS_IRQCTL)   │        │ - IRQ_ENABLE   │               │  │
│  │  └────────────────┘        │ - IRQ_DISABLE  │               │  │
│  │                            │ - IRQ_SETPOLICY│               │  │
│  │                            │ - IRQ_RMPOLICY │               │  │
│  │                            └────────────────┘               │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                              │                                      │
│                              │ 中断触发                             │
│                              ▼                                      │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  中断处理流程                                                │  │
│  │  ┌──────────┐   ┌──────────┐   ┌──────────┐                │  │
│  │  │ 硬件中断 │──►│ 通用处理 │──►│ 通知驱动 │                │  │
│  │  │ (IRQ)    │   │ 函数     │   │ 进程     │                │  │
│  │  └──────────┘   └──────────┘   └──────────┘                │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  设备 I/O 操作                                               │  │
│  │  ┌────────────────┐        ┌────────────────┐               │  │
│  │  │ do_devio       │        │ do_vdevio      │               │  │
│  │  │ (单端口 I/O)   │        │ (批量 I/O)     │               │  │
│  │  └────────────────┘        └────────────────┘               │  │
│  │         │                           │                        │  │
│  │         └───────────┬───────────────┘                        │  │
│  │                     ▼                                        │  │
│  │              I/O 端口权限检查                                │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 核心设计理念

**1. 中断的异步通知机制**

```
传统中断处理（Linux）:
┌─────────────────────────────────────────────────────────────────────┐
│  硬件中断 ──► 内核中断处理函数 ──► 驱动程序中断处理函数           │
│                                                                     │
│  特点：                                                            │
│  - 中断上下文执行                                                  │
│  - 不能睡眠                                                        │
│  - 快速处理                                                        │
└─────────────────────────────────────────────────────────────────────┘

Minix3 异步通知机制:
┌─────────────────────────────────────────────────────────────────────┐
│  硬件中断 ──► 内核通用处理函数 ──► 发送通知消息 ──► 驱动进程     │
│                                     │                               │
│                                     └─► 驱动进程在用户态处理       │
│                                                                     │
│  特点：                                                            │
│  - 内核只做最小工作                                                │
│  - 驱动在用户态处理                                                │
│  - 可以睡眠、调用系统调用                                          │
│  - 微内核架构的体现                                                │
└─────────────────────────────────────────────────────────────────────┘
```

**2. I/O 端口的权限隔离**

```
传统系统（Linux）:
┌─────────────────────────────────────────────────────────────────────┐
│  内核模块 ──► 可以访问所有 I/O 端口                                │
│  用户程序 ──► 通过 iopl() 获取权限（需要 root）                    │
│                                                                     │
│  问题：                                                            │
│  - 权限粒度粗（全部或无）                                          │
│  - 安全风险                                                        │
└─────────────────────────────────────────────────────────────────────┘

Minix3 权限隔离:
┌─────────────────────────────────────────────────────────────────────┐
│  每个进程有独立的 I/O 端口范围：                                   │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  进程 A (驱动 1)                                              │  │
│  │  └─► s_io_tab[0] = {0x3F8, 0x3FF}  // COM1                  │  │
│  │  └─► s_io_tab[1] = {0x2F8, 0x2FF}  // COM2                  │  │
│  └──────────────────────────────────────────────────────────────┘  │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │  进程 B (驱动 2)                                              │  │
│  │  └─► s_io_tab[0] = {0x1F0, 0x1F7}  // IDE                   │  │
│  │  └─► s_io_tab[1] = {0x170, 0x177}  // IDE2                  │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  优点：                                                            │
│  - 细粒度权限控制                                                  │
│  - 进程隔离                                                        │
│  - 安全性高                                                        │
└─────────────────────────────────────────────────────────────────────┘
```

**3. 批量 I/O 的性能优化**

```
单次 I/O 操作:
┌─────────────────────────────────────────────────────────────────────┐
│  驱动初始化设备（需要写 10 个寄存器）                              │
│      │                                                              │
│      ├─► sys_devio(port1, value1)  ──► 系统调用开销 1             │
│      ├─► sys_devio(port2, value2)  ──► 系统调用开销 2             │
│      ├─► sys_devio(port3, value3)  ──► 系统调用开销 3             │
│      └─► ...                                                        │
│                                                                     │
│  总开销 = 10 次系统调用                                            │
└─────────────────────────────────────────────────────────────────────┘

批量 I/O 操作:
┌─────────────────────────────────────────────────────────────────────┐
│  驱动初始化设备（需要写 10 个寄存器）                              │
│      │                                                              │
│      └─► sys_vdevio(ports[], values[], 10)                        │
│          └─► 一次系统调用完成所有操作                              │
│                                                                     │
│  总开销 = 1 次系统调用                                             │
│  性能提升 = 10 倍                                                  │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 硬件中断管理

### do_irqctl - 中断控制核心

**源代码位置**: [do_irqctl.c](file://../minix3/minix/kernel/system/do_irqctl.c)

**核心功能**: 实现 `SYS_IRQCTL` 系统调用，管理硬件中断的注册、启用、禁用和策略设置

#### 核心数据结构

```c
/* IRQ Hook 结构 - 每个中断一个 */
typedef struct irq_hook {
    struct irq_hook *next;      // 链表指针（共享 IRQ）
    int proc_nr_e;              // 处理进程端点
    int notify_id;              // 通知标识符
    int policy;                 // 中断策略
    int irq;                    // IRQ 线号
} irq_hook_t;

/* 全局 IRQ Hook 表 */
irq_hook_t irq_hooks[NR_IRQ_HOOKS];  // 通常 16-32 个
```

**内存布局**:

```
┌─────────────────────────────────────┐
│ next:     8 字节（指针）            │
│ proc_nr_e: 4 字节（端点号）         │
│ notify_id: 4 字节（通知 ID）        │
│ policy:   4 字节（策略）            │
│ irq:      4 字节（IRQ 线号）        │
│ 填充:     4 字节                    │
└─────────────────────────────────────┘
总大小: 28 字节（有填充）
```

#### 支持的命令

| 命令 | 功能 | 权限检查 |
|------|------|----------|
| `IRQ_ENABLE` | 启用 IRQ | Hook 所有者 |
| `IRQ_DISABLE` | 禁用 IRQ | Hook 所有者 |
| `IRQ_SETPOLICY` | 设置中断策略 | IRQ 白名单 |
| `IRQ_RMPOLICY` | 移除中断策略 | Hook 所有者 |

#### IRQ_SETPOLICY 实现逻辑

```c
int do_irqctl(struct proc * caller, message * m_ptr)
{
    int irq_vec;
    int irq_hook_id;
    int notify_id;
    int r = OK;
    int i;
    irq_hook_t *hook_ptr;
    struct priv *privp;

    /* Hook identifiers start at 1 and end at NR_IRQ_HOOKS. */
    irq_hook_id = m_ptr->m_lsys_krn_sys_irqctl.hook_id - 1;
    irq_vec = m_ptr->m_lsys_krn_sys_irqctl.vector;

    switch(m_ptr->m_lsys_krn_sys_irqctl.request) {
    case IRQ_SETPOLICY:
        /* Check if IRQ line is acceptable. */
        if (irq_vec < 0 || irq_vec >= NR_IRQ_VECTORS) return(EINVAL);

        privp= priv(caller);
        if (!privp) {
            printf("do_irqctl: no priv structure!\n");
            return EPERM;
        }
        
        /* IRQ 权限检查 */
        if (privp->s_flags & CHECK_IRQ) {
            for (i= 0; i<privp->s_nr_irq; i++) {
                if (irq_vec == privp->s_irq_tab[i])
                    break;
            }
            if (i >= privp->s_nr_irq) {
                printf(
                "do_irqctl: IRQ check failed for proc %d, IRQ %d\n",
                    caller->p_endpoint, irq_vec);
                return EPERM;
            }
        }

        /* 通知 ID 范围检查 */
        notify_id = m_ptr->m_lsys_krn_sys_irqctl.hook_id;
        if (notify_id > CHAR_BIT * sizeof(irq_id_t) - 1) return(EINVAL);

        /* 查找现有 Hook 或分配新的 */
        hook_ptr = NULL;
        for (i=0; !hook_ptr && i<NR_IRQ_HOOKS; i++) {
            if (irq_hooks[i].proc_nr_e == caller->p_endpoint
                && irq_hooks[i].notify_id == notify_id) {
                irq_hook_id = i;
                hook_ptr = &irq_hooks[irq_hook_id];
                rm_irq_handler(&irq_hooks[irq_hook_id]);
            }
        }
        
        // ... 填充 Hook 并注册处理函数 ...
        break;
    }
}
```

#### 通用中断处理函数

```c
static int generic_handler(irq_hook_t * hook)
{
    /* 1. 收集随机数（用于 /dev/random） */
    get_randomness(&krandom, hook->irq);
    
    /* 2. 检查处理进程是否有效 */
    if(!isokendpt(hook->proc_nr_e, &proc_nr))
        panic("invalid interrupt handler");
    
    /* 3. 设置中断待处理位图 */
    priv(proc_addr(proc_nr))->s_int_pending |= (1 << hook->notify_id);
    
    /* 4. 发送通知消息 */
    mini_notify(proc_addr(HARDWARE), hook->proc_nr_e);
    
    /* 5. 返回是否重新启用 IRQ */
    return(hook->policy & IRQ_REENABLE);
}
```

#### 中断处理完整流程

```
硬件产生中断
    │
    ├── 1. CPU 切换到内核态
    │
    ├── 2. 调用汇编入口 irq_entry
    │
    ├── 3. 遍历 irq_hooks 链表
    │   └── 对每个 hook 调用 generic_handler
    │
    ├── 4. generic_handler:
    │   ├── 收集随机数（用于 /dev/random）
    │   ├── 设置 s_int_pending 位图
    │   └── 发送通知给驱动进程
    │
    ├── 5. 如果 policy & IRQ_REENABLE，重新启用 IRQ
    │
    └── 6. 返回用户态
    
驱动进程收到通知
    └── 读取 s_int_pending 位图，知道哪个 IRQ 触发
```

#### IRQ 共享机制

```
IRQ 共享场景（多个设备共享同一 IRQ 线）:
┌─────────────────────────────────────────────────────────────────────┐
│  IRQ 11（PCI 设备共享）                                             │
│      │                                                              │
│      ├─► irq_hooks[0] ──► 驱动 A (网卡)                            │
│      │   └─► notify_id = 0                                         │
│      │                                                              │
│      ├─► irq_hooks[1] ──► 驱动 B (声卡)                            │
│      │   └─► notify_id = 1                                         │
│      │                                                              │
│      └─► irq_hooks[2] ──► 驱动 C (USB)                             │
│          └─► notify_id = 2                                         │
│                                                                     │
│  中断发生时：                                                      │
│  1. 内核遍历链表，调用所有 generic_handler                         │
│  2. 每个驱动收到通知                                               │
│  3. 驱动检查自己的设备状态                                         │
│  4. 如果不是自己的设备，忽略通知                                   │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 设备 I/O 操作

### do_devio - 单端口设备 I/O

**源代码位置**: [do_devio.c](file://../minix3/minix/kernel/system/do_devio.c)

**核心功能**: 执行单个 I/O 端口操作（读/写 字节/字/长字）

#### 实现逻辑

```c
int do_devio(struct proc * caller, message * m_ptr)
{
    struct priv *privp;
    port_t port;
    struct io_range *iorp;
    int i, size, nr_io_range;
    int io_type, io_dir;

    io_type = m_ptr->m_lsys_krn_sys_devio.request & _DIO_TYPEMASK;
    io_dir  = m_ptr->m_lsys_krn_sys_devio.request & _DIO_DIRMASK;

    switch (io_type) {
        case _DIO_BYTE: size= 1; break;
        case _DIO_WORD: size= 2; break;
        case _DIO_LONG: size= 4; break;
        default: size= 4; break;
    }

    privp= priv(caller);
    if (!privp) {
        printf("no priv structure!\n");
        goto doit;
    }
    
    /* I/O 端口权限检查 */
    if (privp->s_flags & CHECK_IO_PORT) {
        port= m_ptr->m_lsys_krn_sys_devio.port;
        nr_io_range= privp->s_nr_io_range;
        for (i= 0, iorp= privp->s_io_tab; i<nr_io_range; i++, iorp++) {
            if (port >= iorp->ior_base && port+size-1 <= iorp->ior_limit)
                break;
        }
        if (i >= nr_io_range) {
            printf("do_devio: port 0x%x (size %d) not allowed\n",
                m_ptr->m_lsys_krn_sys_devio.port, size);
            return EPERM;
        }
    }

doit:
    /* 端口对齐检查 */
    if (m_ptr->m_lsys_krn_sys_devio.port & (size-1)) {
        printf("do_devio: unaligned port 0x%x (size %d)\n",
            m_ptr->m_lsys_krn_sys_devio.port, size);
        return EPERM;
    }

    /* 执行 I/O 操作 */
    if (io_dir == _DIO_INPUT) { 
        switch (io_type) {
            case _DIO_BYTE:
                m_ptr->m_krn_lsys_sys_devio.value =
                    inb(m_ptr->m_lsys_krn_sys_devio.port);
                break;
            case _DIO_WORD:
                m_ptr->m_krn_lsys_sys_devio.value =
                    inw(m_ptr->m_lsys_krn_sys_devio.port);
                break;
            case _DIO_LONG:
                m_ptr->m_krn_lsys_sys_devio.value =
                    inl(m_ptr->m_lsys_krn_sys_devio.port);
                break;
            default: return(EINVAL);
        } 
    } else { 
        switch (io_type) {
            case _DIO_BYTE:
                outb(m_ptr->m_lsys_krn_sys_devio.port,
                    m_ptr->m_lsys_krn_sys_devio.value);
                break;
            case _DIO_WORD:
                outw(m_ptr->m_lsys_krn_sys_devio.port,
                    m_ptr->m_lsys_krn_sys_devio.value);
                break;
            case _DIO_LONG:
                outl(m_ptr->m_lsys_krn_sys_devio.port,
                    m_ptr->m_lsys_krn_sys_devio.value);
                break;
            default: return(EINVAL);
        }
    }
    return(OK);
}
```

#### I/O 端口权限检查

```
权限检查流程:
┌─────────────────────────────────────────────────────────────────────┐
│  1. 获取进程特权结构                                              │
│     privp = priv(caller)                                           │
│                                                                     │
│  2. 检查是否需要权限检查                                          │
│     if (privp->s_flags & CHECK_IO_PORT)                            │
│                                                                     │
│  3. 遍历 I/O 范围表                                               │
│     for (i= 0, iorp= privp->s_io_tab; i<nr_io_range; i++, iorp++) │
│         if (port >= iorp->ior_base && port+size-1 <= iorp->ior_limit)│
│             break;                                                  │
│                                                                     │
│  4. 如果未找到匹配范围，返回 EPERM                                │
│     if (i >= nr_io_range) return EPERM                             │
└─────────────────────────────────────────────────────────────────────┘

I/O 范围表示例:
┌─────────────────────────────────────────────────────────────────────┐
│  进程: 串口驱动                                                    │
│  s_io_tab[0] = {0x3F8, 0x3FF}  // COM1 (8 个端口)                 │
│  s_io_tab[1] = {0x2F8, 0x2FF}  // COM2 (8 个端口)                 │
│                                                                     │
│  允许访问: 0x3F8-0x3FF, 0x2F8-0x2FF                                │
│  拒绝访问: 其他所有端口                                            │
└─────────────────────────────────────────────────────────────────────┘
```

#### 端口对齐检查

```
对齐规则:
┌─────────────────────────────────────────────────────────────────────┐
│  BYTE (1 字节):                                                    │
│    - 无对齐要求                                                    │
│    - 可以访问任意端口                                              │
│                                                                     │
│  WORD (2 字节):                                                    │
│    - 必须偶数对齐                                                  │
│    - port & 1 == 0                                                 │
│    - 例如: 0x3F8 ✓, 0x3F9 ✗                                       │
│                                                                     │
│  LONG (4 字节):                                                    │
│    - 必须 4 字节对齐                                               │
│    - port & 3 == 0                                                 │
│    - 例如: 0x3F8 ✓, 0x3FA ✗                                       │
└─────────────────────────────────────────────────────────────────────┘

为什么需要对齐？
  - 硬件要求：某些设备只支持对齐访问
  - 性能优化：对齐访问更快
  - 安全性：防止跨边界访问
```

### do_vdevio - 批量设备 I/O

**源代码位置**: [do_vdevio.c](file://../minix3/minix/kernel/system/do_vdevio.c)

**核心功能**: 批量执行多个 I/O 端口操作

#### 核心设计

```c
/* 静态缓冲区，避免栈分配 */
static char vdevio_buf[VDEVIO_BUF_SIZE];

/* 三种类型的端口-值对 */
static pvb_pair_t * const pvb = (pvb_pair_t *) vdevio_buf;  // 字节
static pvw_pair_t * const pvw = (pvw_pair_t *) vdevio_buf;  // 字
static pvl_pair_t * const pvl = (pvl_pair_t *) vdevio_buf;  // 长字
```

#### 数据结构

```c
/* 字节端口-值对 */
typedef struct {
    port_t port;    // 端口号
    u8_t value;     // 字节值
} pvb_pair_t;

/* 字端口-值对 */
typedef struct {
    port_t port;    // 端口号
    u16_t value;    // 字值
} pvw_pair_t;

/* 长字端口-值对 */
typedef struct {
    port_t port;    // 端口号
    u32_t value;    // 长字值
} pvl_pair_t;
```

**内存布局**:

```
pvb_pair_t (字节):
┌─────────────────────────────────────┐
│ port:  2 字节（端口号）             │
│ value: 1 字节（字节值）             │
│ 填充:  1 字节                       │
└─────────────────────────────────────┘
总大小: 4 字节

pvw_pair_t (字):
┌─────────────────────────────────────┐
│ port:  2 字节（端口号）             │
│ value: 2 字节（字值）               │
└─────────────────────────────────────┘
总大小: 4 字节

pvl_pair_t (长字):
┌─────────────────────────────────────┐
│ port:  2 字节（端口号）             │
│ 填充:  2 字节                       │
│ value: 4 字节（长字值）             │
└─────────────────────────────────────┘
总大小: 8 字节
```

#### 实现逻辑

```c
int do_vdevio(struct proc * caller, message * m_ptr)
{
    int io_type, io_dir;
    int vec_size, i;
    vir_bytes vec_addr;
    phys_bytes bytes;
    
    io_type = m_ptr->m_lsys_krn_sys_vdevio.request & _DIO_TYPEMASK;
    io_dir  = m_ptr->m_lsys_krn_sys_vdevio.request & _DIO_DIRMASK;
    vec_size = m_ptr->m_lsys_krn_sys_vdevio.vec_size;
    vec_addr = m_ptr->m_lsys_krn_sys_vdevio.vec_addr;
    
    /* 计算缓冲区大小 */
    switch (io_type) {
        case _DIO_BYTE: bytes = vec_size * sizeof(pvb_pair_t); break;
        case _DIO_WORD: bytes = vec_size * sizeof(pvw_pair_t); break;
        case _DIO_LONG: bytes = vec_size * sizeof(pvl_pair_t); break;
        default: return EINVAL;
    }
    
    /* 检查缓冲区大小 */
    if (bytes > VDEVIO_BUF_SIZE) return E2BIG;
    
    /* 从用户空间拷贝数据 */
    if (data_copy(caller->p_endpoint, vec_addr, 
                  KERNEL, (vir_bytes) vdevio_buf, bytes) != OK)
        return EFAULT;
    
    /* I/O 端口权限检查 */
    for (i = 0; i < vec_size; i++) {
        port_t port;
        switch (io_type) {
            case _DIO_BYTE: port = pvb[i].port; break;
            case _DIO_WORD: port = pvw[i].port; break;
            case _DIO_LONG: port = pvl[i].port; break;
        }
        
        if (!check_io_port_permission(caller, port, io_type))
            return EPERM;
    }
    
    /* 执行批量 I/O */
    switch (io_type) {
        case _DIO_BYTE:
            if (io_dir == _DIO_INPUT) {
                for (i = 0; i < vec_size; i++)
                    pvb[i].value = inb(pvb[i].port);
            } else {
                for (i = 0; i < vec_size; i++)
                    outb(pvb[i].port, pvb[i].value);
            }
            break;
        case _DIO_WORD:
            if (io_dir == _DIO_INPUT) {
                for (i = 0; i < vec_size; i++)
                    pvw[i].value = inw(pvw[i].port);
            } else {
                for (i = 0; i < vec_size; i++)
                    outw(pvw[i].port, pvw[i].value);
            }
            break;
        case _DIO_LONG:
            if (io_dir == _DIO_INPUT) {
                for (i = 0; i < vec_size; i++)
                    pvl[i].value = inl(pvl[i].port);
            } else {
                for (i = 0; i < vec_size; i++)
                    outl(pvl[i].port, pvl[i].value);
            }
            break;
    }
    
    /* 如果是输入，拷贝结果回用户空间 */
    if (io_dir == _DIO_INPUT) {
        if (data_copy(KERNEL, (vir_bytes) vdevio_buf,
                      caller->p_endpoint, vec_addr, bytes) != OK)
            return EFAULT;
    }
    
    return OK;
}
```

#### 与 do_devio 的对比

| 特性 | do_devio | do_vdevio |
|------|----------|-----------|
| **操作数量** | 单个端口 | 批量端口 |
| **性能** | 多次系统调用 | 一次系统调用 |
| **缓冲区** | 消息参数 | 静态缓冲区 |
| **使用场景** | 简单 I/O | 初始化序列 |
| **原子性** | 单次操作 | 批量操作 |
| **最大操作数** | 1 | VDEVIO_BUF_SIZE / sizeof(pair) |

#### 批量 I/O 的使用场景

```
场景 1: 设备初始化
┌─────────────────────────────────────────────────────────────────────┐
│  网卡初始化（需要写 20 个寄存器）                                  │
│                                                                     │
│  传统方案:                                                          │
│  - 20 次 sys_devio() 调用                                          │
│  - 20 次系统调用开销                                               │
│  - 总时间: ~200 微秒                                               │
│                                                                     │
│  批量方案:                                                          │
│  - 1 次 sys_vdevio() 调用                                          │
│  - 1 次系统调用开销                                                │
│  - 总时间: ~20 微秒                                                │
│  - 性能提升: 10 倍                                                 │
└─────────────────────────────────────────────────────────────────────┘

场景 2: DMA 设置
┌─────────────────────────────────────────────────────────────────────┐
│  DMA 控制器设置（需要写多个寄存器）                                │
│  - 基地址寄存器                                                    │
│  - 计数寄存器                                                      │
│  - 模式寄存器                                                      │
│  - 命令寄存器                                                      │
│                                                                     │
│  批量 I/O 确保原子性：                                             │
│  - 所有寄存器在一次系统调用中设置                                  │
│  - 避免中间状态                                                    │
│  - 防止竞争条件                                                    │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 核心函数逐行分析

### do_irqctl - 中断控制核心

**源代码位置**: [do_irqctl.c](file://../minix3/minix/kernel/system/do_irqctl.c)

**功能概述**: 实现 `SYS_IRQCTL` 系统调用，管理硬件中断的注册、启用、禁用和策略设置。

#### 逐行代码分析

```c
int do_irqctl(struct proc * caller, message * m_ptr)
{
  /* 第 1-8 行：参数提取 */
  int irq_vec;
  int irq_hook_id;
  int notify_id;
  int r = OK;
  int i;
  irq_hook_t *hook_ptr;
  struct priv *privp;

  /* Hook identifiers start at 1 and end at NR_IRQ_HOOKS. */
  irq_hook_id = m_ptr->m_lsys_krn_sys_irqctl.hook_id - 1;
  irq_vec = m_ptr->m_lsys_krn_sys_irqctl.vector;
```

**参数说明**:
- `irq_hook_id`: Hook ID（用户传入，从 1 开始）
- `irq_vec`: IRQ 向量号（0-255）
- `hook_id - 1`: 转换为数组索引（从 0 开始）

**关键点**: Hook ID 从 1 开始
- 用户看到的 Hook ID 从 1 开始
- 内核内部使用数组索引（从 0 开始）
- 这样设计是为了避免 0 作为有效 ID

```c
  /* 第 9-25 行：IRQ_ENABLE 和 IRQ_DISABLE */
  switch(m_ptr->m_lsys_krn_sys_irqctl.request) {

  case IRQ_ENABLE:           
  case IRQ_DISABLE: 
      if (irq_hook_id >= NR_IRQ_HOOKS || irq_hook_id < 0 ||
          irq_hooks[irq_hook_id].proc_nr_e == NONE) return(EINVAL);
      if (irq_hooks[irq_hook_id].proc_nr_e != caller->p_endpoint) return(EPERM);
      if (m_ptr->m_lsys_krn_sys_irqctl.request == IRQ_ENABLE) {
          enable_irq(&irq_hooks[irq_hook_id]);	
      }
      else 
          disable_irq(&irq_hooks[irq_hook_id]);	
      break;
```

**启用/禁用流程**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  1. 检查 Hook ID 有效性                                            │
│     - irq_hook_id < NR_IRQ_HOOKS                                  │
│     - irq_hook_id >= 0                                            │
│     - irq_hooks[irq_hook_id].proc_nr_e != NONE                    │
│                                                                     │
│  2. 检查所有权                                                      │
│     irq_hooks[irq_hook_id].proc_nr_e == caller->p_endpoint        │
│     - 只有 Hook 所有者才能启用/禁用                                │
│                                                                     │
│  3. 执行操作                                                        │
│     - IRQ_ENABLE: enable_irq(&irq_hooks[irq_hook_id])             │
│     - IRQ_DISABLE: disable_irq(&irq_hooks[irq_hook_id])           │
└─────────────────────────────────────────────────────────────────────┘
```

**关键点**: 所有权检查
- 防止进程启用/禁用其他进程的中断
- 安全性：避免恶意进程干扰其他驱动

```c
  /* 第 26-58 行：IRQ_SETPOLICY */
  case IRQ_SETPOLICY:  

      /* Check if IRQ line is acceptable. */
      if (irq_vec < 0 || irq_vec >= NR_IRQ_VECTORS) return(EINVAL);

      privp= priv(caller);
      if (!privp)
      {
	printf("do_irqctl: no priv structure!\n");
	return EPERM;
      }
```

**IRQ 向量检查**:
- `irq_vec < 0`: 无效向量号
- `irq_vec >= NR_IRQ_VECTORS`: 超出范围（通常 256）
- `priv(caller)`: 获取进程特权结构
- 无特权结构：返回 `EPERM`

```c
      if (privp->s_flags & CHECK_IRQ)
      {
	for (i= 0; i<privp->s_nr_irq; i++)
	{
		if (irq_vec == privp->s_irq_tab[i])
			break;
	}
	if (i >= privp->s_nr_irq)
	{
		printf(
		"do_irqctl: IRQ check failed for proc %d, IRQ %d\n",
			caller->p_endpoint, irq_vec);
		return EPERM;
	}
    }
```

**IRQ 权限检查**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  CHECK_IRQ 标志检查                                                │
│      │                                                              │
│      ├─► 标志未设置: 跳过检查（系统进程）                          │
│      │                                                              │
│      └─► 标志已设置: 检查 IRQ 白名单                               │
│          │                                                          │
│          └─► 遍历 s_irq_tab 数组                                   │
│              - 查找 irq_vec 是否在白名单中                         │
│              - 如果不在白名单中，返回 EPERM                        │
└─────────────────────────────────────────────────────────────────────┘
```

**关键点**: IRQ 白名单机制
- 每个进程有独立的 IRQ 白名单
- 只能注册白名单中的 IRQ
- 防止恶意进程注册任意 IRQ

```c
      /* When setting a policy, the caller must provide an identifier that
       * is returned on the notification message if a interrupt occurs.
       */
      notify_id = m_ptr->m_lsys_krn_sys_irqctl.hook_id;
      if (notify_id > CHAR_BIT * sizeof(irq_id_t) - 1) return(EINVAL);
```

**通知 ID 检查**:
- `notify_id`: 中断发生时返回的标识符
- `CHAR_BIT * sizeof(irq_id_t) - 1`: 最大值（通常 31）
- 用于设置 `s_int_pending` 位图

**关键点**: 位图限制
- `s_int_pending` 是一个位图
- `notify_id` 对应位图中的某一位
- 最大值受限于 `irq_id_t` 的位数

```c
      /* Try to find an existing mapping to override. */
      hook_ptr = NULL;
      for (i=0; !hook_ptr && i<NR_IRQ_HOOKS; i++) {
          if (irq_hooks[i].proc_nr_e == caller->p_endpoint
              && irq_hooks[i].notify_id == notify_id) {
              irq_hook_id = i;
              hook_ptr = &irq_hooks[irq_hook_id];	/* existing hook */
              rm_irq_handler(&irq_hooks[irq_hook_id]);
          }
      }
```

**查找现有 Hook**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  遍历 irq_hooks 数组                                               │
│      │                                                              │
│      └─► 查找匹配项：                                              │
│          - proc_nr_e == caller->p_endpoint                         │
│          - notify_id == notify_id                                  │
│                                                                     │
│  如果找到：                                                        │
│      - hook_ptr = &irq_hooks[irq_hook_id]                         │
│      - rm_irq_handler(&irq_hooks[irq_hook_id])                    │
│      - 移除旧的处理函数                                            │
└─────────────────────────────────────────────────────────────────────┘
```

**关键点**: 覆盖机制
- 允许进程重新设置相同 notify_id 的 Hook
- 先移除旧的 Hook，再添加新的
- 支持动态更新中断策略

```c
      /* If there is nothing to override, find a free hook for this mapping. */
      for (i=0; !hook_ptr && i<NR_IRQ_HOOKS; i++) {
          if (irq_hooks[i].proc_nr_e == NONE) {
              irq_hook_id = i;
              hook_ptr = &irq_hooks[irq_hook_id];	/* free hook */
          }
      }
      if (hook_ptr == NULL) return(ENOSPC);
```

**分配新 Hook**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  遍历 irq_hooks 数组                                               │
│      │                                                              │
│      └─► 查找空闲项：                                              │
│          - proc_nr_e == NONE                                       │
│                                                                     │
│  如果找到：                                                        │
│      - hook_ptr = &irq_hooks[irq_hook_id]                         │
│                                                                     │
│  如果未找到：                                                      │
│      - 返回 ENOSPC（无可用 Hook）                                  │
└─────────────────────────────────────────────────────────────────────┘
```

**关键点**: 静态分配
- `irq_hooks` 是静态数组
- 大小为 `NR_IRQ_HOOKS`（通常 16-32）
- 用完返回 `ENOSPC`

```c
      /* Install the handler. */
      hook_ptr->proc_nr_e = caller->p_endpoint;	/* process to notify */
      hook_ptr->notify_id = notify_id;		/* identifier to pass */   	
      hook_ptr->policy = m_ptr->m_lsys_krn_sys_irqctl.policy;	/* policy for interrupts */
      put_irq_handler(hook_ptr, irq_vec, generic_handler);
      DEBUGBASIC(("IRQ %d handler registered by %s / %d\n",
			      irq_vec, caller->p_name, caller->p_endpoint));

      /* Return index of the IRQ hook in use. */
      m_ptr->m_krn_lsys_sys_irqctl.hook_id = irq_hook_id + 1;
      break;
```

**安装处理函数**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  1. 填充 Hook 结构                                                 │
│     - proc_nr_e: 进程端点号                                        │
│     - notify_id: 通知标识符                                        │
│     - policy: 中断策略                                             │
│                                                                     │
│  2. 注册处理函数                                                    │
│     put_irq_handler(hook_ptr, irq_vec, generic_handler)           │
│     - 将 Hook 添加到 IRQ 链表                                      │
│     - generic_handler: 通用中断处理函数                            │
│                                                                     │
│  3. 返回 Hook ID                                                    │
│     m_ptr->m_krn_lsys_sys_irqctl.hook_id = irq_hook_id + 1        │
│     - 转换回用户可见的 ID（从 1 开始）                             │
└─────────────────────────────────────────────────────────────────────┘
```

**关键点**: generic_handler
- 所有中断共用同一个处理函数
- 通过 Hook 结构区分不同进程
- 微内核架构的体现

```c
  /* 第 59-69 行：IRQ_RMPOLICY */
  case IRQ_RMPOLICY:
      if (irq_hook_id < 0 || irq_hook_id >= NR_IRQ_HOOKS ||
               irq_hooks[irq_hook_id].proc_nr_e == NONE) {
           return(EINVAL);
      } else if (caller->p_endpoint != irq_hooks[irq_hook_id].proc_nr_e) {
           return(EPERM);
      }
      /* Remove the handler and return. */
      rm_irq_handler(&irq_hooks[irq_hook_id]);
      irq_hooks[irq_hook_id].proc_nr_e = NONE;
      break;
```

**移除策略流程**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  1. 检查 Hook ID 有效性                                            │
│     - irq_hook_id >= 0 && irq_hook_id < NR_IRQ_HOOKS              │
│     - irq_hooks[irq_hook_id].proc_nr_e != NONE                    │
│                                                                     │
│  2. 检查所有权                                                      │
│     caller->p_endpoint == irq_hooks[irq_hook_id].proc_nr_e        │
│                                                                     │
│  3. 移除处理函数                                                    │
│     rm_irq_handler(&irq_hooks[irq_hook_id])                       │
│     - 从 IRQ 链表中移除                                            │
│                                                                     │
│  4. 标记为空闲                                                      │
│     irq_hooks[irq_hook_id].proc_nr_e = NONE                       │
└─────────────────────────────────────────────────────────────────────┘
```

```c
  /* 第 70-72 行：默认情况 */
  default:
      r = EINVAL;				/* invalid IRQ REQUEST */
  }
  return(r);
}
```

#### generic_handler - 通用中断处理函数

```c
static int generic_handler(irq_hook_t * hook)
{
  int proc_nr;

  /* As a side-effect, the interrupt handler gathers random information by 
   * timestamping the interrupt events. This is used for /dev/random.
   */
  get_randomness(&krandom, hook->irq);
```

**随机数收集**:
- `get_randomness`: 收集中断时间戳
- 用于 `/dev/random` 设备
- 增加系统随机性

```c
  /* Check if the handler is still alive.
   * If it's dead, this should never happen, as processes that die 
   * automatically get their interrupt hooks unhooked.
   */
  if(!isokendpt(hook->proc_nr_e, &proc_nr))
     panic("invalid interrupt handler: %d", hook->proc_nr_e);
```

**进程有效性检查**:
- `isokendpt`: 验证端点号有效性
- 如果进程已死亡，触发 `panic`
- 正常情况下不应该发生（进程死亡时自动清理 Hook）

```c
  /* Add a bit for this interrupt to the process' pending interrupts. When 
   * sending the notification message, this bit map will be magically set
   * as an argument. 
   */
  priv(proc_addr(proc_nr))->s_int_pending |= (1 << hook->notify_id);
```

**设置中断待处理位图**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  s_int_pending 位图                                                │
│  ┌───┬───┬───┬───┬───┬───┬───┬───┐                               │
│  │ 7 │ 6 │ 5 │ 4 │ 3 │ 2 │ 1 │ 0 │  (位索引)                     │
│  └───┴───┴───┴───┴───┴───┴───┴───┘                               │
│    ▲                                                               │
│    └─► 1 << hook->notify_id                                       │
│        设置对应的位                                                │
│                                                                     │
│  作用：                                                            │
│  - 驱动进程可以通过位图知道哪个 IRQ 触发                           │
│  - 支持多个 IRQ 共享同一处理函数                                   │
└─────────────────────────────────────────────────────────────────────┘
```

```c
  /* Build notification message and return. */
  mini_notify(proc_addr(HARDWARE), hook->proc_nr_e);
  return(hook->policy & IRQ_REENABLE);
}
```

**发送通知**:
- `mini_notify`: 发送异步通知消息
- `proc_addr(HARDWARE)`: 硬件进程（伪进程）
- `hook->proc_nr_e`: 目标驱动进程
- 返回值：是否重新启用 IRQ

**关键点**: 返回值
- `hook->policy & IRQ_REENABLE`: 检查策略
- 如果设置，重新启用 IRQ
- 如果未设置，IRQ 保持禁用（需要驱动手动启用）

---

### do_devio - 单端口设备 I/O

**源代码位置**: [do_devio.c](file://../minix3/minix/kernel/system/do_devio.c)

**功能概述**: 执行单个 I/O 端口操作（读/写 字节/字/长字）。

#### 逐行代码分析

```c
int do_devio(struct proc * caller, message * m_ptr)
{
    struct priv *privp;
    port_t port;
    struct io_range *iorp;
    int i, size, nr_io_range;
    int io_type, io_dir;

    io_type = m_ptr->m_lsys_krn_sys_devio.request & _DIO_TYPEMASK;
    io_dir  = m_ptr->m_lsys_krn_sys_devio.request & _DIO_DIRMASK;
```

**参数提取**:
- `io_type`: I/O 类型（字节/字/长字）
- `io_dir`: I/O 方向（输入/输出）
- `_DIO_TYPEMASK`: 类型掩码
- `_DIO_DIRMASK`: 方向掩码

```c
    switch (io_type)
    {
	case _DIO_BYTE: size= 1; break;
	case _DIO_WORD: size= 2; break;
	case _DIO_LONG: size= 4; break;
	default: size= 4; break;	/* Be conservative */
    }
```

**类型到大小转换**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  _DIO_BYTE:  1 字节                                                │
│  _DIO_WORD:  2 字节                                                │
│  _DIO_LONG:  4 字节                                                │
│  default:    4 字节（保守策略）                                    │
└─────────────────────────────────────────────────────────────────────┘
```

**关键点**: 保守策略
- 如果类型无效，默认使用 4 字节
- 避免权限检查时遗漏

```c
    privp= priv(caller);
    if (!privp)
    {
	printf("no priv structure!\n");
	goto doit;
    }
```

**特权结构检查**:
- `priv(caller)`: 获取进程特权结构
- 如果无特权结构，跳过权限检查
- 系统进程可能无特权结构

```c
    if (privp->s_flags & CHECK_IO_PORT)
    {
	port= m_ptr->m_lsys_krn_sys_devio.port;
	nr_io_range= privp->s_nr_io_range;
	for (i= 0, iorp= privp->s_io_tab; i<nr_io_range; i++, iorp++)
	{
		if (port >= iorp->ior_base && port+size-1 <= iorp->ior_limit)
			break;
	}
	if (i >= nr_io_range)
	{
			printf("do_devio: port 0x%x (size %d) not allowed\n",
				m_ptr->m_lsys_krn_sys_devio.port, size);
		return EPERM;
	}
    }
```

**I/O 端口权限检查**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  CHECK_IO_PORT 标志检查                                            │
│      │                                                              │
│      ├─► 标志未设置: 跳过检查（系统进程）                          │
│      │                                                              │
│      └─► 标志已设置: 检查 I/O 范围表                               │
│          │                                                          │
│          └─► 遍历 s_io_tab 数组                                    │
│              - 查找 port 是否在允许范围内                          │
│              - 检查范围: [port, port+size-1]                       │
│              - 如果不在范围内，返回 EPERM                          │
└─────────────────────────────────────────────────────────────────────┘
```

**关键点**: 范围检查
- 检查 `[port, port+size-1]` 整个范围
- 防止跨边界访问
- 例如：访问 0x3F8 的字（2 字节），需要检查 0x3F8-0x3F9

```c
doit:
    if (m_ptr->m_lsys_krn_sys_devio.port & (size-1))
    {
		printf("do_devio: unaligned port 0x%x (size %d)\n",
			m_ptr->m_lsys_krn_sys_devio.port, size);
	return EPERM;
    }
```

**端口对齐检查**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  BYTE (1 字节):                                                    │
│    - port & 0 == 0（总是对齐）                                     │
│    - 无对齐要求                                                    │
│                                                                     │
│  WORD (2 字节):                                                    │
│    - port & 1 == 0                                                 │
│    - 必须偶数对齐                                                  │
│    - 例如: 0x3F8 ✓, 0x3F9 ✗                                       │
│                                                                     │
│  LONG (4 字节):                                                    │
│    - port & 3 == 0                                                 │
│    - 必须 4 字节对齐                                               │
│    - 例如: 0x3F8 ✓, 0x3FA ✗                                       │
└─────────────────────────────────────────────────────────────────────┘
```

**关键点**: 为什么需要对齐？
- 硬件要求：某些设备只支持对齐访问
- 性能优化：对齐访问更快
- 安全性：防止跨边界访问

```c
/* Process a single I/O request for byte, word, and long values. */
    if (io_dir == _DIO_INPUT) { 
      switch (io_type) {
	/* maybe "it" should not be called ports */
        case _DIO_BYTE:
		m_ptr->m_krn_lsys_sys_devio.value =
			inb(m_ptr->m_lsys_krn_sys_devio.port);
		break;
        case _DIO_WORD:
		m_ptr->m_krn_lsys_sys_devio.value =
			inw(m_ptr->m_lsys_krn_sys_devio.port);
		break;
        case _DIO_LONG:
		m_ptr->m_krn_lsys_sys_devio.value =
			inl(m_ptr->m_lsys_krn_sys_devio.port);
		break;
    	default: return(EINVAL);
      } 
    }
```

**输入操作**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  _DIO_BYTE:                                                        │
│    - inb(port): 读取 1 字节                                        │
│    - 返回值: u8                                                    │
│                                                                     │
│  _DIO_WORD:                                                        │
│    - inw(port): 读取 2 字节                                        │
│    - 返回值: u16                                                   │
│                                                                     │
│  _DIO_LONG:                                                        │
│    - inl(port): 读取 4 字节                                        │
│    - 返回值: u32                                                   │
└─────────────────────────────────────────────────────────────────────┘
```

**关键点**: inb/inw/inl
- x86 汇编指令
- `in al, dx`: 从端口读取字节
- `in ax, dx`: 从端口读取字
- `in eax, dx`: 从端口读取长字

```c
    else { 
      switch (io_type) {
	case _DIO_BYTE:
		outb(m_ptr->m_lsys_krn_sys_devio.port,
			m_ptr->m_lsys_krn_sys_devio.value);
		break;
	case _DIO_WORD:
		outw(m_ptr->m_lsys_krn_sys_devio.port,
			m_ptr->m_lsys_krn_sys_devio.value);
		break;
	case _DIO_LONG:
		outl(m_ptr->m_lsys_krn_sys_devio.port,
			m_ptr->m_lsys_krn_sys_devio.value);
		break;
    	default: return(EINVAL);
      } 
    }
    return(OK);
}
```

**输出操作**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  _DIO_BYTE:                                                        │
│    - outb(port, value): 写入 1 字节                                │
│    - 参数: u8                                                      │
│                                                                     │
│  _DIO_WORD:                                                        │
│    - outw(port, value): 写入 2 字节                                │
│    - 参数: u16                                                     │
│                                                                     │
│  _DIO_LONG:                                                        │
│    - outl(port, value): 写入 4 字节                                │
│    - 参数: u32                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

**关键点**: outb/outw/outl
- x86 汇编指令
- `out dx, al`: 向端口写入字节
- `out dx, ax`: 向端口写入字
- `out dx, eax`: 向端口写入长字

---

### do_vdevio - 批量设备 I/O

**源代码位置**: [do_vdevio.c](file://../minix3/minix/kernel/system/do_vdevio.c)

**功能概述**: 批量执行多个 I/O 端口操作。

#### 逐行代码分析

```c
/* Buffer for SYS_VDEVIO to copy (port,value)-pairs from/ to user. */
static char vdevio_buf[VDEVIO_BUF_SIZE];      
static pvb_pair_t * const pvb = (pvb_pair_t *) vdevio_buf;           
static pvw_pair_t * const pvw = (pvw_pair_t *) vdevio_buf;      
static pvl_pair_t * const pvl = (pvl_pair_t *) vdevio_buf;     
```

**静态缓冲区**:
- `vdevio_buf`: 静态缓冲区，避免栈分配
- `VDEVIO_BUF_SIZE`: 缓冲区大小（通常 4KB）
- 三种类型的指针：字节/字/长字

**关键点**: 为什么使用静态缓冲区？
- 内核栈空间有限（通常 8KB）
- 避免动态分配（内核不使用 malloc）
- 静态分配保证可用性

```c
int do_vdevio(struct proc * caller, message * m_ptr)
{
  int vec_size;               /* size of vector */
  int io_in;                  /* true if input */
  size_t bytes;               /* # bytes to be copied */
  port_t port;
  int i, j, io_size, nr_io_range;
  int io_dir, io_type;
  struct priv *privp;
  struct io_range *iorp;
  int r;
```

```c
  /* Get the request, size of the request vector, and check the values. */
  io_dir = m_ptr->m_lsys_krn_sys_vdevio.request & _DIO_DIRMASK;
  io_type = m_ptr->m_lsys_krn_sys_vdevio.request & _DIO_TYPEMASK;
  if (io_dir == _DIO_INPUT) io_in = TRUE;
  else if (io_dir == _DIO_OUTPUT) io_in = FALSE;
  else return(EINVAL);
  if ((vec_size = m_ptr->m_lsys_krn_sys_vdevio.vec_size) <= 0) return(EINVAL);
```

**参数验证**:
- `io_dir`: I/O 方向（输入/输出）
- `io_type`: I/O 类型（字节/字/长字）
- `vec_size`: 向量大小（必须 > 0）

```c
  switch (io_type) {
      case _DIO_BYTE:
	bytes = vec_size * sizeof(pvb_pair_t);
	io_size= sizeof(u8_t);
	break;
      case _DIO_WORD:
	bytes = vec_size * sizeof(pvw_pair_t);
	io_size= sizeof(u16_t);
	break;
      case _DIO_LONG:
	bytes = vec_size * sizeof(pvl_pair_t);
	io_size= sizeof(u32_t);
	break;
      default:  return(EINVAL);   /* check type once and for all */
  }
  if (bytes > sizeof(vdevio_buf))  return(E2BIG);
```

**缓冲区大小计算**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  _DIO_BYTE:                                                        │
│    - bytes = vec_size * sizeof(pvb_pair_t)                        │
│    - io_size = 1                                                  │
│                                                                     │
│  _DIO_WORD:                                                        │
│    - bytes = vec_size * sizeof(pvw_pair_t)                        │
│    - io_size = 2                                                  │
│                                                                     │
│  _DIO_LONG:                                                        │
│    - bytes = vec_size * sizeof(pvl_pair_t)                        │
│    - io_size = 4                                                  │
│                                                                     │
│  检查: bytes > sizeof(vdevio_buf)                                  │
│    - 如果超出缓冲区大小，返回 E2BIG                                │
└─────────────────────────────────────────────────────────────────────┘
```

**关键点**: E2BIG 错误
- 缓冲区大小有限
- 防止缓冲区溢出
- 调用者需要分批处理

```c
  /* Copy (port,value)-pairs from user. */
  if((r=data_copy(caller->p_endpoint, m_ptr->m_lsys_krn_sys_vdevio.vec_addr,
    KERNEL, (vir_bytes) vdevio_buf, bytes)) != OK)
	return r;
```

**从用户空间拷贝数据**:
- `data_copy`: 跨地址空间的数据拷贝
- `caller->p_endpoint`: 源进程
- `KERNEL`: 目标进程（内核）
- `vdevio_buf`: 目标地址
- `bytes`: 拷贝字节数

```c
  privp= priv(caller);
  if (privp && (privp->s_flags & CHECK_IO_PORT))
  {
	/* Check whether the I/O is allowed */
	nr_io_range= privp->s_nr_io_range;
	for (i=0; i<vec_size; i++)
	{
		switch (io_type) {
		case _DIO_BYTE: port= pvb[i].port; break;
		case _DIO_WORD: port= pvw[i].port; break;
		default:	port= pvl[i].port; break;
		}
		for (j= 0, iorp= privp->s_io_tab; j<nr_io_range; j++, iorp++)
		{
			if (port >= iorp->ior_base &&
				port+io_size-1 <= iorp->ior_limit)
			{
				break;
			}
		}
		if (j >= nr_io_range)
		{
			printf(
		"do_vdevio: I/O port check failed for proc %d, port 0x%x\n",
				caller->p_endpoint, port);
			return EPERM;
		}
	}
  }
```

**批量权限检查**:
```
┌─────────────────────────────────────────────────────────────────────┐
│  遍历所有端口-值对                                                 │
│      │                                                              │
│      └─► 对每个端口：                                              │
│          │                                                          │
│          ├─► 提取端口号                                            │
│          │   - pvb[i].port / pvw[i].port / pvl[i].port             │
│          │                                                          │
│          └─► 检查权限                                              │
│              - 遍历 s_io_tab 数组                                  │
│              - 查找 port 是否在允许范围内                          │
│              - 如果不在范围内，返回 EPERM                          │
└─────────────────────────────────────────────────────────────────────┘
```

**关键点**: 逐个检查
- 所有端口都必须通过权限检查
- 一个端口失败，整个操作失败
- 保证原子性

```c
  /* Perform actual device I/O for byte, word, and long values */
  switch (io_type) {
  case _DIO_BYTE: 					 /* byte values */
      if (io_in) for (i=0; i<vec_size; i++) 
		pvb[i].value = inb( pvb[i].port); 
      else      for (i=0; i<vec_size; i++)
		outb( pvb[i].port, pvb[i].value); 
      break; 
```

**字节 I/O 操作**:
- 输入：`pvb[i].value = inb(pvb[i].port)`
- 输出：`outb(pvb[i].port, pvb[i].value)`
- 无对齐检查（字节总是对齐）

```c
  case _DIO_WORD:					  /* word values */
      if (io_in)
      {
	for (i=0; i<vec_size; i++)  
	{
		port= pvw[i].port;
		if (port & 1) goto bad;
		pvw[i].value = inw( pvw[i].port);  
	}
      }
      else
      {
	for (i=0; i<vec_size; i++) 
	{
		port= pvw[i].port;
		if (port & 1) goto bad;
		outw( pvw[i].port, pvw[i].value); 
	}
      }
      break; 
```

**字 I/O 操作**:
- 对齐检查：`if (port & 1) goto bad`
- 输入：`pvw[i].value = inw(pvw[i].port)`
- 输出：`outw(pvw[i].port, pvw[i].value)`

**关键点**: 对齐检查
- 字访问必须偶数对齐
- 如果不对齐，触发 `panic`
- 为什么是 `panic` 而不是返回错误？
  - 驱动程序应该知道端口对齐要求
  - 不对齐访问是编程错误
  - 内核不应该隐藏这种错误

```c
  default:            					  /* long values */
      if (io_in)
      {
	for (i=0; i<vec_size; i++)
	{
		port= pvl[i].port;
		if (port & 3) goto bad;
		pvl[i].value = inl(pvl[i].port);  
	}
      }
      else
      {
	for (i=0; i<vec_size; i++)
	{
		port= pvl[i].port;
		if (port & 3) goto bad;
		outl( pvb[i].port, pvl[i].value); 
	}
      }
  }
```

**长字 I/O 操作**:
- 对齐检查：`if (port & 3) goto bad`
- 输入：`pvl[i].value = inl(pvl[i].port)`
- 输出：`outl(pvl[i].port, pvl[i].value)`

**注意**: 代码中有一个 bug
- `outl(pvb[i].port, pvl[i].value)` 应该是 `outl(pvl[i].port, pvl[i].value)`
- 这是一个源码 bug，可能导致访问错误的端口

```c
  /* Almost done, copy back results for input requests. */
  if (io_in) 
	if((r=data_copy(KERNEL, (vir_bytes) vdevio_buf,
	  caller->p_endpoint, m_ptr->m_lsys_krn_sys_vdevio.vec_addr,
	  (phys_bytes) bytes)) != OK)
		return r;
  return(OK);

bad:
	panic("do_vdevio: unaligned port: %d", port);
	return EPERM;
}
```

**返回结果**:
- 输入操作：拷贝结果回用户空间
- 输出操作：直接返回 OK
- 不对齐：触发 `panic`

**关键点**: 为什么输入需要拷贝回用户空间？
- 输入操作读取的值存储在内核缓冲区
- 需要拷贝回用户空间供调用者使用
- 输出操作不需要拷贝回用户空间

---

## 模块级 Rust 重构建议

### 1. 类型安全的 IRQ Hook

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrqVector(u8);

impl IrqVector {
    pub const NR_IRQ_VECTORS: usize = 256;
    
    pub fn new(vec: u8) -> Result<Self, IrqError> {
        if vec < Self::NR_IRQ_VECTORS as u8 {
            Ok(Self(vec))
        } else {
            Err(IrqError::InvalidVector)
        }
    }
    
    pub fn as_u8(&self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotifyId(u8);

impl NotifyId {
    pub const MAX: usize = 32;  // sizeof(irq_id_t) * CHAR_BIT
    
    pub fn new(id: u8) -> Result<Self, IrqError> {
        if (id as usize) < Self::MAX {
            Ok(Self(id))
        } else {
            Err(IrqError::InvalidNotifyId)
        }
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct IrqPolicy: u32 {
        const REENABLE = 0x01;  // 自动重新启用 IRQ
    }
}

#[derive(Debug)]
pub struct IrqHook {
    next: Option<&'static mut IrqHook>,
    proc_nr_e: Endpoint,
    notify_id: NotifyId,
    policy: IrqPolicy,
    irq: IrqVector,
}

impl IrqHook {
    pub fn new(
        proc: Endpoint,
        notify_id: NotifyId,
        policy: IrqPolicy,
        irq: IrqVector,
    ) -> Self {
        Self {
            next: None,
            proc_nr_e: proc,
            notify_id,
            policy,
            irq,
        }
    }
    
    pub fn is_owner(&self, proc: Endpoint) -> bool {
        self.proc_nr_e == proc
    }
}
```

### 2. 类型安全的 I/O 端口

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Port(u16);

impl Port {
    pub fn new(port: u16) -> Self {
        Self(port)
    }
    
    pub fn as_u16(&self) -> u16 {
        self.0
    }
    
    pub fn is_aligned(&self, size: IoSize) -> bool {
        match size {
            IoSize::Byte => true,
            IoSize::Word => (self.0 & 1) == 0,
            IoSize::Long => (self.0 & 3) == 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoSize {
    Byte = 1,
    Word = 2,
    Long = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoDirection {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy)]
pub struct IoRange {
    pub base: Port,
    pub limit: Port,
}

impl IoRange {
    pub fn new(base: u16, limit: u16) -> Self {
        Self {
            base: Port::new(base),
            limit: Port::new(limit),
        }
    }
    
    pub fn contains(&self, port: Port, size: IoSize) -> bool {
        let start = port.as_u16();
        let end = start + (size as u16) - 1;
        start >= self.base.as_u16() && end <= self.limit.as_u16()
    }
}
```

### 3. 安全的 I/O 操作

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoError {
    InvalidPort,
    PermissionDenied,
    UnalignedPort,
    InvalidSize,
}

pub fn do_devio(
    caller: &Proc,
    request: IoRequest,
) -> Result<u32, IoError> {
    let port = Port::new(request.port);
    let size = request.size;
    let dir = request.direction;
    
    // 权限检查
    if !check_io_port_permission(caller, port, size) {
        return Err(IoError::PermissionDenied);
    }
    
    // 对齐检查
    if !port.is_aligned(size) {
        return Err(IoError::UnalignedPort);
    }
    
    // 执行 I/O
    let value = match dir {
        IoDirection::Input => {
            unsafe {
                match size {
                    IoSize::Byte => inb(port.as_u16()) as u32,
                    IoSize::Word => inw(port.as_u16()) as u32,
                    IoSize::Long => inl(port.as_u16()),
                }
            }
        }
        IoDirection::Output => {
            unsafe {
                match size {
                    IoSize::Byte => outb(port.as_u16(), request.value as u8),
                    IoSize::Word => outw(port.as_u16(), request.value as u16),
                    IoSize::Long => outl(port.as_u16(), request.value),
                }
            }
            request.value
        }
    };
    
    Ok(value)
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    core::arch::asm!(
        "in al, dx",
        in("dx") port,
        out("al") value,
        options(nomem, nostack)
    );
    value
}

unsafe fn outb(port: u16, value: u8) {
    core::arch::asm!(
        "out dx, al",
        in("dx") port,
        in("al") value,
        options(nomem, nostack)
    );
}
```

### 4. 批量 I/O 的类型安全实现

```rust
#[derive(Debug, Clone, Copy)]
pub struct PortValuePair {
    pub port: Port,
    pub value: u32,
    pub size: IoSize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VdevioError {
    BufferTooLarge,
    InvalidPort,
    PermissionDenied,
    UnalignedPort,
    CopyError,
}

pub fn do_vdevio(
    caller: &Proc,
    vec_addr: VirtAddr,
    vec_size: usize,
    io_dir: IoDirection,
    io_size: IoSize,
) -> Result<(), VdevioError> {
    // 计算缓冲区大小
    let pair_size = match io_size {
        IoSize::Byte => core::mem::size_of::<PvbPair>(),
        IoSize::Word => core::mem::size_of::<PvwPair>(),
        IoSize::Long => core::mem::size_of::<PvlPair>(),
    };
    let bytes = vec_size * pair_size;
    
    // 检查缓冲区大小
    if bytes > VDEVIO_BUF_SIZE {
        return Err(VdevioError::BufferTooLarge);
    }
    
    // 从用户空间拷贝数据
    let mut pairs: Vec<PortValuePair, VDEVIO_MAX_PAIRS> = 
        copy_from_user(caller, vec_addr, vec_size, io_size)?;
    
    // I/O 端口权限检查
    for pair in pairs.iter() {
        if !check_io_port_permission(caller, pair.port, pair.size) {
            return Err(VdevioError::PermissionDenied);
        }
        if !pair.port.is_aligned(pair.size) {
            return Err(VdevioError::UnalignedPort);
        }
    }
    
    // 执行批量 I/O
    match io_dir {
        IoDirection::Input => {
            for pair in pairs.iter_mut() {
                pair.value = unsafe {
                    match pair.size {
                        IoSize::Byte => inb(pair.port.as_u16()) as u32,
                        IoSize::Word => inw(pair.port.as_u16()) as u32,
                        IoSize::Long => inl(pair.port.as_u16()),
                    }
                };
            }
            // 拷贝结果回用户空间
            copy_to_user(caller, vec_addr, &pairs, io_size)?;
        }
        IoDirection::Output => {
            for pair in pairs.iter() {
                unsafe {
                    match pair.size {
                        IoSize::Byte => outb(pair.port.as_u16(), pair.value as u8),
                        IoSize::Word => outw(pair.port.as_u16(), pair.value as u16),
                        IoSize::Long => outl(pair.port.as_u16(), pair.value),
                    }
                }
            }
        }
    }
    
    Ok(())
}
```

### 5. 中断处理的类型安全实现

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqError {
    InvalidVector,
    InvalidNotifyId,
    PermissionDenied,
    HookNotFound,
    HookAlreadyExists,
}

pub fn do_irqctl(
    caller: &Proc,
    request: IrqRequest,
) -> Result<IrqHookId, IrqError> {
    match request {
        IrqRequest::Enable { hook_id } => {
            let hook = get_irq_hook(hook_id)?;
            
            if !hook.is_owner(caller.endpoint()) {
                return Err(IrqError::PermissionDenied);
            }
            
            enable_irq(hook);
            Ok(hook_id)
        }
        
        IrqRequest::Disable { hook_id } => {
            let hook = get_irq_hook(hook_id)?;
            
            if !hook.is_owner(caller.endpoint()) {
                return Err(IrqError::PermissionDenied);
            }
            
            disable_irq(hook);
            Ok(hook_id)
        }
        
        IrqRequest::SetPolicy { irq_vec, notify_id, policy } => {
            let irq = IrqVector::new(irq_vec)?;
            let notify = NotifyId::new(notify_id)?;
            
            // IRQ 权限检查
            if !check_irq_permission(caller, irq) {
                return Err(IrqError::PermissionDenied);
            }
            
            // 查找或分配 Hook
            let hook_id = find_or_allocate_hook(caller.endpoint(), notify)?;
            let hook = get_irq_hook_mut(hook_id)?;
            
            // 填充 Hook
            *hook = IrqHook::new(caller.endpoint(), notify, policy, irq);
            
            // 注册处理函数
            put_irq_handler(hook, irq, generic_handler);
            
            Ok(hook_id)
        }
        
        IrqRequest::RmPolicy { hook_id } => {
            let hook = get_irq_hook(hook_id)?;
            
            if !hook.is_owner(caller.endpoint()) {
                return Err(IrqError::PermissionDenied);
            }
            
            rm_irq_handler(hook);
            Ok(hook_id)
        }
    }
}

fn generic_handler(hook: &IrqHook) -> bool {
    // 收集随机数
    get_randomness(&krandom, hook.irq.as_u8());
    
    // 检查处理进程是否有效
    if !is_valid_endpoint(hook.proc_nr_e) {
        panic!("invalid interrupt handler");
    }
    
    // 设置中断待处理位图
    let proc = get_process(hook.proc_nr_e);
    proc.priv.s_int_pending |= 1 << hook.notify_id.as_u8();
    
    // 发送通知消息
    mini_notify(HARDWARE, hook.proc_nr_e);
    
    // 返回是否重新启用 IRQ
    hook.policy.contains(IrqPolicy::REENABLE)
}
```

---

## 现代 64 位硬件演进

### MSI/MSI-X 中断

```
传统 IRQ:
┌─────────────────────────────────────────────────────────────────────┐
│  - 共享 IRQ 线                                                    │
│  - 最多 16 个 IRQ                                                │
│  - 中断处理需要遍历链表                                          │
│  - 延迟高                                                        │
└─────────────────────────────────────────────────────────────────────┘

MSI (Message Signaled Interrupts):
┌─────────────────────────────────────────────────────────────────────┐
│  - 每个 PCI 设备独占中断向量                                     │
│  - 最多 32 个 MSI 向量                                           │
│  - 直接写入内存地址触发中断                                      │
│  - 延迟低                                                        │
│  - 不需要共享                                                    │
└─────────────────────────────────────────────────────────────────────┘

MSI-X:
┌─────────────────────────────────────────────────────────────────────┐
│  - 每个 PCI 设备可以有多个中断向量                               │
│  - 最多 2048 个 MSI-X 向量                                       │
│  - 每个队列可以独立中断                                          │
│  - 适合高速设备（网卡、存储）                                    │
└─────────────────────────────────────────────────────────────────────┘

Rust 适配建议:
```rust
#[derive(Debug, Clone, Copy)]
pub enum InterruptType {
    LegacyIrq(IrqVector),
    Msi(MsiVector),
    MsiX(MsiXVector),
}

#[derive(Debug, Clone, Copy)]
pub struct MsiVector {
    pub address: u64,
    pub data: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct MsiXVector {
    pub table_entry: usize,
    pub vector: MsiVector,
}
```
```

### IOMMU 与中断重映射

```
IOMMU 中断重映射:
┌─────────────────────────────────────────────────────────────────────┐
│  传统方案:                                                         │
│  - 设备直接发送中断                                               │
│  - 恶意设备可以伪造中断                                           │
│  - 安全风险                                                       │
│                                                                     │
│  IOMMU 方案:                                                       │
│  - 设备发送中断到 IOMMU                                           │
│  - IOMMU 验证中断权限                                             │
│  - 只有授权的中断才能通过                                         │
│  - 防止恶意设备攻击                                               │
└─────────────────────────────────────────────────────────────────────┘
```

### Memory-Mapped I/O (MMIO)

```
传统 Port I/O:
┌─────────────────────────────────────────────────────────────────────┐
│  - 使用独立的 I/O 地址空间                                       │
│  - 使用 in/out 指令                                               │
│  - 需要特权指令                                                   │
│  - 性能较低                                                       │
└─────────────────────────────────────────────────────────────────────┘

Memory-Mapped I/O:
┌─────────────────────────────────────────────────────────────────────┐
│  - 使用内存地址空间                                               │
│  - 使用普通的内存访问指令                                         │
│  - 可以使用 CPU 缓存优化                                          │
│  - 性能更高                                                       │
│  - 现代设备首选                                                   │
└─────────────────────────────────────────────────────────────────────┘

Rust 适配建议:
```rust
#[derive(Debug, Clone, Copy)]
pub enum IoAccess {
    PortIo(Port),
    Mmio(PhysAddr),
}

pub unsafe fn read_register(access: IoAccess, size: IoSize) -> u32 {
    match access {
        IoAccess::PortIo(port) => {
            match size {
                IoSize::Byte => inb(port.as_u16()) as u32,
                IoSize::Word => inw(port.as_u16()) as u32,
                IoSize::Long => inl(port.as_u16()),
            }
        }
        IoAccess::Mmio(addr) => {
            let ptr = addr.as_ptr::<u32>();
            match size {
                IoSize::Byte => core::ptr::read_volatile(ptr as *const u8) as u32,
                IoSize::Word => core::ptr::read_volatile(ptr as *const u16) as u32,
                IoSize::Long => core::ptr::read_volatile(ptr),
            }
        }
    }
}
```
```

---

## 要点总结

### 核心知识点

1. **异步通知机制**: Minix3 使用异步通知而非传统中断处理，驱动在用户态处理中断
2. **权限隔离**: I/O 端口和 IRQ 都有细粒度的权限控制，每个进程只能访问授权的资源
3. **批量优化**: vdevio 提供批量 I/O 操作，显著减少系统调用开销

### 灾难预演

**场景 1: 删除 IRQ 权限检查**

```c
// 如果删除 IRQ 权限检查
if (privp->s_flags & CHECK_IRQ) { ... }
```

后果:
- 任意进程可以注册任意 IRQ
- 恶意进程可以劫持中断
- 系统崩溃或安全漏洞

**场景 2: 删除 I/O 端口对齐检查**

```c
// 如果删除对齐检查
if (m_ptr->m_lsys_krn_sys_devio.port & (size-1)) { ... }
```

后果:
- 不对齐的端口访问
- 硬件异常
- 数据损坏

**场景 3: 忘记重新启用 IRQ**

```c
// 如果 generic_handler 总是返回 false
return 0;  // 不重新启用 IRQ
```

后果:
- IRQ 被禁用后无法再次触发
- 设备停止工作
- 系统挂起

### 互动自测

1. **问题**: Minix3 的中断处理与传统 OS 有什么不同？
   **答案**: Minix3 使用异步通知机制，内核只做最小工作，驱动在用户态处理中断，可以睡眠和调用系统调用。

2. **问题**: 为什么需要 I/O 端口权限检查？
   **答案**: 防止恶意进程访问未授权的 I/O 端口，提供细粒度的权限隔离。

3. **问题**: do_vdevio 相比 do_devio 有什么优势？
   **答案**: 批量操作减少系统调用开销，提高性能，同时保证原子性。

4. **问题**: MSI 相比传统 IRQ 有什么优势？
   **答案**: 独占中断向量，不需要共享，延迟低，适合高速设备。

5. **问题**: 为什么需要端口对齐检查？
   **答案**: 硬件要求、性能优化、防止跨边界访问。

---

**文档版本**: 2026-03-31
