# 03-rs-privilege: 权限结构建模与 privctl 操作面

> **分类**: 阶段 2 — 权限与隔离（boot Step 1 的权限机制）
> **源码**: `minix3/minix/kernel/priv.h`（struct priv）、`minix3/minix/include/minix/priv.h`（静态 id/默认宏）、`minix3/minix/include/minix/const.h:142-153`（s_flags 位）、`minix3/minix/include/minix/com.h:342-353`（SYS_PRIV_* 操作码）、`minix3/minix/servers/rs/main.c:240-345`（boot Step 1）、`minix3/minix/servers/rs/utility.c:82-141,364-422`（fill_*/sched_init_proc/update_sig_mgrs）、`minix3/minix/kernel/system/do_privctl.c`（privctl 内核侧语义）、`minix3/minix/lib/libsys/sys_privctl.c`、`minix3/minix/lib/libsys/sched_start.c`（外部调用面）
> **Rust 模块**: `os/servers/rs/src/privilege.rs`（`Privilege`/`PrivFlags`/`TrapMask`/`CallMask`/`SysMap`/`PrivCtlOp`/`srv_or_usr`/`from_calls`）、`os/servers/rs/src/sched.rs`（`sched_init_proc`）、`boot.rs` 接线（KernelApi::privctl/getpriv/sched_init_proc）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md`（boot 时序）、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md`（r_priv 字段归属）、`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/22-privilege.md`（kernel 侧 priv 语义）
> **说明**: RS 是内核 priv 结构的管理者：boot Step 1 为每个 boot 服务构造 `struct priv`（权限结构），经 `sys_privctl(SYS_PRIV_SET_SYS)` 设置到内核、`sys_getpriv` 同步回本地；运行时经 `SYS_PRIV_UPDATE_SYS` 更新（信号管理器/编辑）、`ALLOW/DISALLOW/YIELD` 门控运行、`SET_USER` 降权、`CLEAR_IPC_REFS` 清理。本文档建模 priv 结构、boot 初始化流程、privctl 全操作面、调度初始化与信号管理器更新原语。

---

## 1. 概念：权限机制——RS 如何决定"谁能做什么"

### 1.0 章节引言

`02-rs-process-table.md` 建立了服务登记表：每个服务一个槽位，槽位里有身份（label/endpoint）、状态（r_flags）、能力（sys_flags）。本文档回答的问题是：**槽位里那个"要传给内核的权限结构"（`r_priv`）到底是什么，RS 怎么把它写进去、改掉、清掉**。

> **本章不讲什么**（机制一律移交）:
> - `r_ipc_list` 的解析与 `add_forward_ipc`/`add_backward_ipc` 的语义组合（`05-rs-ipc-sendmask.md`）——本文档只讲 `fill_send_mask` 的"全置/全清"原语
> - 调用者权限检查（`check_call_permission`/`caller_is_root`/`caller_can_control`，`04-rs-access-control.md`）
> - 服务创建时 priv 的逐字段填充来源（`init_slot`/`edit_slot`，`08-rs-slot-config.md`）
> - 服务进程创建/终止/恢复对 priv 的消费（`10-rs-service-create.md`、`15-rs-terminate-restart.md`）
> - Live Update 的 `SYS_PRIV_YIELD` 交接（`16-rs-live-update.md`）
> - 外部 syscall 签名与消息布局（`19-rs-external-interfaces.md`）
>
> 本章只回答一个问题：**权限结构由哪些字段组成、boot 时怎么构造、privctl 每个操作对内核做了什么**。

### 1.1 核心问题：为什么 RS 需要管理权限

Minix3 的微内核把"谁能调用哪个内核调用、谁能向谁发 IPC、谁能访问哪些 I/O/中断/内存"集中放在内核的 **priv 表**（`EXTERN struct priv priv[NR_SYS_PROCS]`，`kernel/priv.h:94`）里：每个系统服务一个独立的 `struct priv`，所有普通用户进程共享一个（`USER_PRIV_ID`，`priv.h:18`）。

问题在于：**内核自己不知道服务需要什么权限**——这由服务的角色决定，而角色是 RS 管理的（boot 表、RS_UP 请求、服务类型）。所以权限配置的职责落在 RS：

```
服务诞生（boot 表 / RS_UP）
  │
  ▼
RS 构造 struct priv（flags/trap_mask/ipc_to/k_call_mask/sig_mgr）
  │
  ▼  sys_privctl(SYS_PRIV_SET_SYS) —— 写进内核 priv 表
  ▼  sys_getpriv —— 同步回本地（内核可能改写）
  │
  ▼
服务运行（内核按 priv 表强制执行：IPC 过滤/系统调用过滤/资源检查）
```

RS 侧的 `r_priv`（`type.h:88`）就是这个结构的**本地副本**——它不是内核 priv 表的影子，而是 RS 构造权限的"草稿纸"：构造好后经 privctl 提交，提交后经 getpriv 同步成"内核实际采纳的版本"。

### 1.2 权限结构由什么组成

`struct priv`（`kernel/priv.h:21-66`）按语义分四组：

| 组 | 字段 | 作用 | 谁写 |
|----|------|------|------|
| 身份 | `s_proc_nr`/`s_id` | 关联的进程号/priv 表索引（static priv id） | 内核（`get_priv` 分配）+ RS（static id 请求） |
| 能力 | `s_flags`/`s_init_flags` | 策略标志（可抢占/可记账/系统服务/资源检查位）+ 初始化标志 | RS（boot 表 flags + 默认宏） |
| 门控 | `s_trap_mask`/`s_ipc_to`/`s_k_call_mask` | 允许的陷阱/允许的 IPC 目标/允许的内核调用 | RS（默认宏 + fill_* 原语） |
| 服务 | `s_sig_mgr`/`s_bak_sig_mgr` | 系统信号管理器（+备份） | RS（boot 默认 = RS 自己；update_sig_mgrs 运行时改） |
| 资源 | `s_nr_io_range`/`s_io_tab`/`s_nr_mem_range`/`s_mem_tab`/`s_nr_irq`/`s_irq_tab` | I/O 端口/内存/IRQ 白名单 | 驱动经 `SYS_PRIV_ADD_*`（RS 透传，见 §2.6 defer） |
| 内核内部 | `s_asyntab`/`s_notify_pending`/`s_alarm_timer`/`s_stack_guard`/`s_ipcf` 等 | 异步发送表/挂起通知/闹钟/栈守卫/IPC filter | 仅内核，RS 不读写 |

关键认知：**权限是"白名单"不是"角色"**。`s_ipc_to` 是 64 位位图（`NR_SYS_PROCS=64`，`sys_config.h:9`），第 i 位 = 允许向 priv id i 的目标发送 IPC；`s_k_call_mask` 是 `SYS_CALL_MASK_SIZE=2` 个 32 位块（`com.h:270-272`，`NR_SYS_CALLS=58`），第 j 位 = 允许内核调用 j。RS 构造时用 `fill_*` 原语"全开或逐位开"，而不是枚举角色。

### 1.3 boot Step 1：RS 怎么构造 priv

`sef_cb_init_fresh()` 的 Step 1（`main.c:240-345`）对 boot 表中每个服务做一次"权限装配"。装配公式（`main.c:262-280`）：

```
s_id          = static_priv_id(endpoint)              // NR_TASKS + endpoint（priv.h:12）
s_flags       = boot_image_priv_table[i].flags        // RSYS_F/VM_F/SRV_F/USR_F（table.c:15-30）
s_init_flags  = SRV_OR_USR(rp, SRV_I, USR_I)          // 都是 0（priv.h:54-58）
s_trap_mask   = SRV_OR_USR(rp, SRV_T, USR_T)          // ~0 或 (1 << SENDREC)（priv.h:61-63）
s_ipc_to      = fill_send_mask(ipc_to == ALL_M)       // 全 1 或全 0（utility.c:82-95）
s_sig_mgr     = SRV_OR_USR(rp, SRV_SM, USR_SM)        // RS 或 PM（priv.h:83-85）
s_bak_sig_mgr = NONE
s_k_call_mask = fill_call_mask(calls, NR_SYS_CALLS, KERNEL_CALL)  // ALL_C 全 1 或逐位（utility.c:100-137）
```

其中 `SRV_OR_USR(rp, X, Y)`（`const.h:71`）是**三态选择**：`rp->r_priv.s_flags & SYS_PROC`（是系统服务）→ 用 `X`（SRV_* 默认）；否则（用户进程，boot 表里只有 INIT）→ 用 `Y`（USR_* 默认）。

装配后的**提交**（`main.c:280-296`）有两个例外：

```
if (endpoint != RS_PROC_NR && endpoint != VM_PROC_NR)
    sys_privctl(endpoint, SYS_PRIV_SET_SYS, &rp->r_priv);   // 例外：RS/VM 已在运行，跳过
sys_getpriv(&rp->r_priv, endpoint);                          // 例外：所有服务（含 RS/VM）都同步
```

为什么 RS/VM 跳过 `SET_SYS`？`main.c:282-284` 注释：**"RS and VM are exceptions and are already running"**——内核 boot 时已经给它们分配了 priv（RS 是 root sys proc、VM 是 boot 早期页表代理），`SET_SYS` 要求目标进程处于 `RTS_NO_PRIV`（阻塞未授权）状态（`do_privctl.c` SET_SYS 分支第一个检查），已经在跑的服务不满足。

为什么所有服务（含 RS/VM）都做 `sys_getpriv`？因为**内核可能改写提交的结构**：`SET_SYS` 时内核重新分配/恢复 `s_id`、强制 `s_proc_nr`、清零挂起通知/信号（`do_privctl.c:110-131`）。RS 必须把"内核实际采纳的版本"同步回本地，后续的 UPDATE_SYS 才以真实状态为基准。

### 1.4 privctl 操作面：七个 RS 用到的操作

`sys_privctl(endpoint, request, arg_ptr)`（`lib/libsys/sys_privctl.c:3-13`）是 RS 与内核 priv 表交互的唯一通道。11 个操作码（`com.h:342-353`）中 RS 使用 7 个：

| 操作码 | 值 | RS 调用点 | 内核侧语义（`do_privctl.c`） |
|--------|----|----------|------------------------------|
| `SYS_PRIV_SET_SYS` | 3 | boot Step 1（main.c:287）、create_service（manager.c:600）、clone（main.c:478） | 给 `RTS_NO_PRIV` 进程分配 priv id、从调用者拷贝结构、清挂起、应用默认+覆盖 |
| `SYS_PRIV_ALLOW` | 1 | boot Step 2（main.c:379）、init 完成（manager.c:932）、脚本/用户进程（manager.c:1234） | 解除 `RTS_NO_PRIV`，允许运行 |
| `SYS_PRIV_DISALLOW` | 2 | 终止（manager.c:441）、update 回滚（update.c:360） | 设置 `RTS_NO_PRIV`，禁止运行 |
| `SYS_PRIV_SET_USER` | 4 | 脚本子进程（manager.c:1224） | 把进程挂到共享的 `USER_PRIV_ID` 结构 |
| `SYS_PRIV_UPDATE_SYS` | 9 | update_sig_mgrs（utility.c:412）、do_edit（request.c:354） | 用 `update_priv` 覆盖现有结构（flags/掩码/信号管理器/资源） |
| `SYS_PRIV_YIELD` | 10 | LU 新旧交接（main.c:485、update.c:680） | 解除目标 `RTS_NO_PRIV` 并挂起调用者（RS 自己） |
| `SYS_PRIV_CLEAR_IPC_REFS` | 11 | 终止清理（manager.c:442） | `clear_ipc_refs(rp, EDEADSRCDST)` 清挂起 IPC |

驱动面 4 个操作（`SYS_PRIV_ADD_IO`/`ADD_MEM`/`ADD_IRQ`/`QUERY_MEM`）由驱动自己经 `sys_privctl` 调用，**不经过 RS**——RS 只是把 `struct priv` 的 I/O/内存/IRQ 字段透传给内核。minix-rs 无驱动面（A-10 同族），这 4 个操作标注 defer（§3.3）。

### 1.5 两个伴随原语

除了 privctl，boot Step 1 的权限机制还包含两个"附带操作"：

1. **`sched_init_proc`**（`utility.c:364-382`）：`SET_SYS` 之后调用，让服务可调度。断言：用户进程必须无调度器（PM 管）、系统进程必须调度器非空。内部调 `sched_start`：调度器是 `KERNEL` → `sys_schedctl`；是用户调度器（如 SCHED）→ `SCHEDULING_START` 消息。
2. **`update_sig_mgrs`**（`utility.c:387-422`）：运行时更新服务的信号管理器。三步：`sys_getpriv` 同步 → 设 `s_sig_mgr`/`s_bak_sig_mgr` → `SYS_PRIV_UPDATE_SYS` 提交。

---

## 2. C 源码分析

### 2.1 struct priv 全字段（kernel/priv.h:21-66）

```c
struct priv {
  proc_nr_t s_proc_nr;		/* number of associated process */        /* 22 */
  sys_id_t s_id;		/* index of this system structure */       /* 23 */
  short s_flags;		/* PREEMTIBLE, BILLABLE, etc. */           /* 24 */
  int s_init_flags;             /* initialization flags given to the process. */ /* 25 */

  /* Asynchronous sends */
  vir_bytes s_asyntab;		/* addr. of table in process' address space */ /* 28 */
  size_t s_asynsize;		/* number of elements in table. 0 when not in use */ /* 29 */
  endpoint_t s_asynendpoint;    /* the endpoint the asyn table belongs to. */ /* 32 */

  short s_trap_mask;		/* allowed system call traps */           /* 34 */
  sys_map_t s_ipc_to;		/* allowed destination processes */       /* 35 */

  /* allowed kernel calls */
  bitchunk_t s_k_call_mask[SYS_CALL_MASK_SIZE]; /* 38 */

  endpoint_t s_sig_mgr;		/* signal manager for system signals */   /* 40 */
  endpoint_t s_bak_sig_mgr;	/* backup signal manager for system signals */ /* 41 */
  sys_map_t s_notify_pending;  	/* bit map with pending notifications */  /* 42 */
  sys_map_t s_asyn_pending;	/* bit map with pending asyn messages */  /* 43 */
  irq_id_t s_int_pending;	/* pending hardware interrupts */         /* 44 */
  sigset_t s_sig_pending;	/* pending signals */                     /* 45 */
  ipc_filter_t *s_ipcf;         /* ipc filter (NULL when no filter is set) */ /* 46 */

  minix_timer_t s_alarm_timer;	/* synchronous alarm timer */             /* 48 */
  reg_t *s_stack_guard;		/* stack guard word for kernel tasks */  /* 49 */

  char s_diag_sig;		/* send a SIGKMESS when diagnostics arrive? */ /* 51 */

  int s_nr_io_range;		/* allowed I/O ports */                   /* 53 */
  struct io_range s_io_tab[NR_IO_RANGE]; /* 54 */

  int s_nr_mem_range;		/* allowed memory ranges */               /* 56 */
  struct minix_mem_range s_mem_tab[NR_MEM_RANGE]; /* 57 */

  int s_nr_irq;			/* allowed IRQ lines */                   /* 59 */
  int s_irq_tab[NR_IRQ];	/* 60 */
  vir_bytes s_grant_table;	/* grant table address of process, or 0 */ /* 61 */
  int s_grant_entries;		/* no. of entries, or 0 */                /* 62 */
  endpoint_t s_grant_endpoint;  /* the endpoint the grant table belongs to */ /* 63 */
  vir_bytes s_state_table;	/* state table address of process, or 0 */ /* 64 */
  int s_state_entries;		/* no. of entries, or 0 */                /* 65 */
};
```

RS 的本地副本 `r_priv` 是 `ixfer_priv_s`（= `struct priv` 的 typedef，`type.h:55`）**值嵌入**在 `struct rproc`（`type.h:88`）——不是指针。这意味着 RS 构造、拷贝、传给内核的都是**完整结构**（`sys_privctl` 的 `arg_ptr` 经 `data_copy` 整块拷贝，`do_privctl.c:123-126`）。

Rust 建模取舍（完整论证见 §3.2）：RS 实际读写的字段（身份/能力/门控/服务/调度参数）全部建模；资源字段建模为定长数组（透传）；内核内部字段（`s_notify_pending`/`s_asyn_pending`/`s_int_pending`/`s_sig_pending`/`s_alarm_timer`/`s_stack_guard`/`s_diag_sig`/`s_ipcf`）**不建模**——它们是内核运行态，RS 只在 `SET_SYS` 后经 `sys_getpriv` 读回完整结构（未来 minix-sys 接线时按 C 布局序列化，见 19）。

### 2.2 静态 priv id 与默认宏（include/minix/priv.h）

静态 priv id 是内核 priv 表的**固定索引**，boot 服务用它保证"谁在哪个槽"稳定可预期：

```c
#define NR_STATIC_PRIV_IDS  NR_BOOT_PROCS                        /* 10 */
#define is_static_priv_id(id) (id >= 0 && id < NR_STATIC_PRIV_IDS) /* 11 */
#define static_priv_id(n)   (NR_TASKS + (n))                     /* 12 */
#define USER_PRIV_ID  static_priv_id(ROOT_USR_PROC_NR)           /* 18 */
#define NULL_PRIV_ID  (-1)                                       /* 21 */
```

- `NR_TASKS=5`（`com.h:56`）→ 静态 id 区间 = `[5, 5+NR_BOOT_PROCS)`。RS=2 → id 7，INIT=11 → id 16。
- `USER_PRIV_ID` = `static_priv_id(11)` = 16——**所有用户进程共享**的 priv 槽（`SET_USER` 就是把进程挂到 id 16）。

默认宏是 `SRV_OR_USR` 的两侧取值（`priv.h:24-103`）：

| 宏 | 值 | 系统服务（SYS_PROC） | 用户进程（非 SYS_PROC） |
|----|----|---------------------|------------------------|
| `SRV_T`/`USR_T`（traps） | `~0` / `(1 << SENDREC)` | 61-62 | 63 |
| `SRV_M`/`USR_M`（targets） | `ALL_M`（-2） | 67-68 | 69 |
| `SRV_KC`/`USR_KC`（kernel calls） | `ALL_C`（-2）/`NO_C`（-1） | 73-74 | 75 |
| `SRV_VC`/`USR_VC`（vm calls） | `ALL_C`/`ALL_C` | 78-79 | 80 |
| `SRV_SM`/`USR_SM`（sig mgr） | `ROOT_SYS_PROC_NR`（=2）/`PM_PROC_NR`（=0） | 83-84 | 85 |
| `SRV_SCH`/`USR_SCH`（scheduler） | `KERNEL`/`NONE` | 88-89 | 90 |
| `SRV_Q`/`USR_Q`（priority） | `USER_Q` | 93-94 | 95 |
| `SRV_QT`/`USR_QT`（quantum） | `USER_QUANTUM` | 98-99 | 100 |

哨兵常量（`priv.h:24-30`）：`NO_M=-1`（无目标）、`ALL_M=-2`（全部目标）、`NO_C=-1`（无调用）、`ALL_C=-2`（全部调用）、`NULL_C=-3`（调用表终止哨兵）。

预设 flags 组合（`priv.h:45-50`，boot 表逐行使用）：

```
SRV_F  = SYS_PROC | PREEMPTIBLE                     /* 普通系统服务（pm/sched/vfs/ds/tty/memory/mib/pfs/mfs） */
DSRV_F = SRV_F | DYN_PRIV_ID                        /* 动态系统服务（RS_UP 启动的） */
RSYS_F = SRV_F | ROOT_SYS_PROC                      /* root sys proc（RS 自己） */
VM_F   = SYS_PROC | VM_SYS_PROC                     /* vm */
USR_F  = BILLABLE | PREEMPTIBLE                     /* 用户进程（INIT） */
IMM_F  = ROOT_SYS_PROC | VM_SYS_PROC | PREEMPTIBLE  /* 不可变位（inherit_service_defaults 强制保留） */
```

### 2.3 s_flags 位定义（include/minix/const.h:142-153）

| 位 | 宏 | 值 | 语义 |
|----|----|----|------|
| 1 | `PREEMPTIBLE` | 0x002 | 可被抢占（kernel 任务不可抢占） |
| 2 | `BILLABLE` | 0x004 | 会计计费 |
| 3 | `DYN_PRIV_ID` | 0x008 | priv id 动态分配（非 boot 服务） |
| 4 | `SYS_PROC` | 0x010 | 系统服务有独立 priv 结构（`SRV_OR_USR` 的判据） |
| 5 | `CHECK_IO_PORT` | 0x020 | 检查 I/O 请求是否允许 |
| 6 | `CHECK_IRQ` | 0x040 | 检查 IRQ 是否可用 |
| 7 | `CHECK_MEM` | 0x080 | 检查（VM）内存映射请求是否允许 |
| 8 | `ROOT_SYS_PROC` | 0x100 | root system process 实例 |
| 9 | `VM_SYS_PROC` | 0x200 | vm system process 实例 |
| 10 | `LU_SYS_PROC` | 0x400 | live updated 系统进程实例 |
| 11 | `RST_SYS_PROC` | 0x800 | restarted 系统进程实例 |

### 2.4 boot Step 1（servers/rs/main.c:240-345）

完整时序（本节的锚点，行号已 grep 实证）：

| 行 | 代码 | 说明 |
|----|------|------|
| 244 | `for (i=0; boot_image_priv_table[i].endpoint != NULL_BOOT_NR; i++)` | 遍历 priv 表（`table.c:15-30`，13 行 = 12 服务 + `NULL_BOOT_NR` 哨兵） |
| 248-250 | `if(iskerneln(...)) continue;` | 跳过 kernel 任务 |
| 253-254 | `boot_image_info_lookup(endpoint, image, &ip, NULL, &boot_image_sys, &boot_image_dev)` | 三表对齐（01 的机制） |
| 262 | `strcpy(rpub->label, boot_image_priv->label)` | label 进公开表 |
| 265 | `rp->r_priv.s_id = static_priv_id(_ENDPOINT_P(endpoint))` | 静态 priv id |
| 269-275 | `s_flags`/`s_init_flags`/`s_trap_mask`/`ipc_to`/`s_sig_mgr`/`s_bak_sig_mgr` | 默认宏 + `fill_send_mask` |
| 279-280 | `fill_call_mask(calls, NR_SYS_CALLS, s_k_call_mask, KERNEL_CALL, TRUE)` | kernel 调用掩码 |
| 285-291 | `if (endpoint != RS && endpoint != VM) sys_privctl(SET_SYS)` | RS/VM 例外 |
| 293-296 | `sys_getpriv(&rp->r_priv, endpoint)` | 同步（全服务含 RS/VM） |
| 301 | `rpub->sys_flags = boot_image_sys->flags` | sys 属性 |
| 303-306 | `rpub->dev_nr = boot_image_dev->dev_nr`（语句在 306） | dev 属性 |
| 317 | `fill_call_mask(calls, NR_VM_CALLS, rpub->vm_call_mask, VM_RQ_BASE, TRUE)` | VM 调用掩码（**注意目标字段在 rprocpub**） |
| 320-322 | `r_scheduler`/`r_priority`/`r_quantum` | 调度参数（SRV_OR_USR） |
| 328-340 | `r_old_rp`/`r_new_rp`/`r_prev_rp`/`r_next_rp` = NULL；`r_uid=0`；时间戳复位 | 槽位默认值（02 已建模） |
| 342-345 | `r_flags = RS_IN_USE \| RS_ACTIVE`；`rproc_ptr[...] = rp`；`in_use = TRUE` | 槽位激活（02 已建模） |

两个例外必须显式记住（`main.c:282-284` 注释 + `main.c:285-291` 代码）：

1. **`SET_SYS` 例外**：RS/VM 跳过——"are already running"（`main.c:285-291`）。
2. **`getpriv` 不例外**：所有服务（含 RS/VM）都同步——"Synch the privilege structure with the kernel"（`main.c:293`）。

### 2.5 fill_send_mask / fill_call_mask（utility.c:82-141）

两个原语定义在 utility.c，语义归属不同：

- **`fill_send_mask(send_mask, set_bits)`**（`utility.c:82-95`）：把 64 位 send mask 全置 1 或全清 0（循环 `NR_SYS_PROCS` 次 `set/unset_sys_bit`）。**定义处归属本文档（03）**，`ipc_to == ALL_M` 的判定与组合语义归属 `05-rs-ipc-sendmask.md`。
- **`fill_call_mask(calls, tot_nr_calls, call_mask, call_base, is_init)`**（`utility.c:100-137`）：
  - 先数 `calls[]` 里非 `NULL_C` 的项数；
  - 若只有一项且是 `ALL_C` → 整个位图全 1（`call_mask[i] = ~0`）；
  - 否则 `is_init` 时先清零，再逐项 `SET_BIT(call_mask, calls[i] - call_base)`（调用号相对 `call_base` 偏移：kernel 调用基 `KERNEL_CALL`、VM 调用基 `VM_RQ_BASE`）。

### 2.6 privctl 操作面（com.h:342-353 + kernel/system/do_privctl.c）

操作码全表（值必须与 com.h 逐一对齐）：

| 值 | 宏 | RS 用 | 内核侧语义要点（do_privctl.c） |
|----|----|------|-------------------------------|
| 1 | `SYS_PRIV_ALLOW` | ✅ | `RTS_NO_PRIV` 必须已设且 `s_proc_nr != NONE`，否则 `EPERM`；解除 `RTS_NO_PRIV`（do_privctl.c:56-64） |
| 2 | `SYS_PRIV_DISALLOW` | ✅ | 未设 `RTS_NO_PRIV` 才允许；设置 `RTS_NO_PRIV`（do_privctl.c:75-79） |
| 3 | `SYS_PRIV_SET_SYS` | ✅ | 见 §2.4；`RTS_NO_PRIV` 必须已设；`get_priv` 分配 id；从调用者 `data_copy` 整块拷贝；清挂起；`update_priv` 覆盖（do_privctl.c:86-171） |
| 4 | `SYS_PRIV_SET_USER` | ✅ | `priv(rp) = priv_addr(USER_PRIV_ID)`——挂共享槽（do_privctl.c:176-183） |
| 5 | `SYS_PRIV_ADD_IO` | defer | 驱动面；`CHECK_IO_PORT` 才处理（do_privctl.c:187-204） |
| 6 | `SYS_PRIV_ADD_MEM` | defer | 驱动面（do_privctl.c:206-216） |
| 7 | `SYS_PRIV_ADD_IRQ` | defer | 驱动面（do_privctl.c:218-230） |
| 8 | `SYS_PRIV_QUERY_MEM` | defer | 驱动面（do_privctl.c:232-251；`sys_privquery_mem`，sys_privctl.c:16-27） |
| 9 | `SYS_PRIV_UPDATE_SYS` | ✅ | `arg_ptr` 必传；`data_copy` 整块拷贝；`update_priv` 覆盖（do_privctl.c:253-266） |
| 10 | `SYS_PRIV_YIELD` | ✅ | 解除目标 `RTS_NO_PRIV` + 挂起**调用者**（do_privctl.c:66-73） |
| 11 | `SYS_PRIV_CLEAR_IPC_REFS` | ✅ | `clear_ipc_refs(rp, EDEADSRCDST)`（do_privctl.c:81-84） |

> 行号注：`do_privctl.c` 各 case 行号以 `rg -n 'case SYS_PRIV_' kernel/system/do_privctl.c` 实证为准（ALLOW 56 / YIELD 66 / DISALLOW 75 / CLEAR_IPC_REFS 81 / SET_SYS 86 / SET_USER 176 / ADD_IO 187 / ADD_MEM 206 / ADD_IRQ 218 / QUERY_MEM 232 / UPDATE_SYS 253）；上表"语义要点"为行为描述，各 case 精确区间以右列锚点为准。

`SYS_PRIV_UPDATE_SYS` 的 `update_priv` 覆盖规则（`do_privctl.c:280-367`）——只覆盖 6 类内容：

1. `s_flags`/`s_init_flags`/`s_sig_mgr`/`s_bak_sig_mgr`（无条件拷贝）
2. IRQ（`CHECK_IRQ` 位设置才拷贝，校验 `s_nr_irq ∈ [0, NR_IRQ]`）
3. I/O 范围（`CHECK_IO_PORT` 位设置才拷贝，校验 `s_nr_io_range ∈ [0, NR_IO_RANGE]`）
4. 内存范围（`CHECK_MEM` 位设置才拷贝，校验 `s_nr_mem_range ∈ [0, NR_MEM_RANGE]`）
5. `s_trap_mask` + `s_ipc_to`（`fill_sendto_mask` 应用目标掩码）
6. `s_k_call_mask`（无条件 `memcpy` 整块覆盖，do_privctl.c:365-367）

RS 的 7 个调用点（`rg -n 'sys_privctl' servers/rs/*.c` 全量）：

```
main.c:287    SET_SYS      boot Step 1（RS/VM 除外）
main.c:379    ALLOW        boot Step 2
main.c:478    SET_SYS      新 RS 实例（USE_LIVEUPDATE 自升级）
main.c:485    YIELD        新 RS 实例接管
manager.c:441 DISALLOW     terminate_service
manager.c:442 CLEAR_IPC_REFS terminate_service
manager.c:525 ALLOW        restart_service 恢复
manager.c:600 SET_SYS      create_service
manager.c:932 ALLOW        init_service 完成
manager.c:1224 SET_USER    脚本子进程
manager.c:1234 ALLOW        脚本子进程
request.c:354 UPDATE_SYS   do_edit
update.c:360  DISALLOW     end_update 回滚
update.c:680  YIELD        LU 新旧交接
utility.c:412 UPDATE_SYS   update_sig_mgrs
```

### 2.7 sched_init_proc（utility.c:364-382）

```c
int sched_init_proc(struct rproc *rp)
{
  int s;
  int is_usr_proc;

  is_usr_proc = !(rp->r_priv.s_flags & SYS_PROC);
  if(is_usr_proc) assert(rp->r_scheduler == NONE);   /* 用户进程无调度器，PM 管 */
  if(!is_usr_proc) assert(rp->r_scheduler != NONE);  /* 系统进程必有调度器 */

  if ((s = sched_start(rp->r_scheduler, rp->r_pub->endpoint,
      RS_PROC_NR, rp->r_priority, rp->r_quantum, rp->r_cpu,
      &rp->r_scheduler)) != OK) {
      return s;
  }
  return s;
}
```

`sched_start`（`lib/libsys/sched_start.c:46-98`）三分支：

- `scheduler_e == NONE` → 直接 `OK`（不调度；用户进程场景，实际由 PM 接管）
- `scheduler_e == KERNEL` → `sys_schedctl(SCHEDCTL_FLAG_KERNEL, ...)`（boot 服务默认，priv.h:88）
- 其他（用户调度器如 SCHED_PROC_NR）→ 发 `SCHEDULING_START` 消息（`m_lsys_sched_scheduling_start`）

注意 `&rp->r_scheduler` 输出参数：调度器可能把请求转发给另一个调度器，返回值覆盖 `r_scheduler`（sched_start.c:91-94 注释）。

### 2.8 update_sig_mgrs（utility.c:387-422）

```c
int update_sig_mgrs(struct rproc *rp, endpoint_t sig_mgr, endpoint_t bak_sig_mgr)
{
  /* Synch privilege structure with the kernel. */
  if ((r = sys_getpriv(&rp->r_priv, rpub->endpoint)) != OK) return r;

  /* Set signal managers. */
  rp->r_priv.s_sig_mgr = sig_mgr;
  rp->r_priv.s_bak_sig_mgr = bak_sig_mgr;

  /* Update privilege structure. */
  r = sys_privctl(rpub->endpoint, SYS_PRIV_UPDATE_SYS, &rp->r_priv);
  ...
}
```

三步顺序不可交换：**先 getpriv（拿内核真实状态）→ 改信号管理器 → UPDATE_SYS 提交**。`sig_mgr == SELF` 时 RS 把自己（`rpub->endpoint`）设为管理器（verbose 日志显示 `(SELF)`）。调用点：do_update 的 replica 更新（request.c:761）、activate_service（manager.c:771-777）。不可变位约束不在这里：`IMM_SF`（`rs.h:205-207`，约束 `rpub->sys_flags` 的 SF_* 位）与 `IMM_F`（`priv.h:50`，约束 `r_priv.s_flags` 的 `ROOT_SYS_PROC|VM_SYS_PROC|PREEMPTIBLE` 位）的消费点是 `inherit_service_defaults`（manager.c:1321-1324）——更新服务从定义服务继承默认时强制保留这两组位；update_sig_mgrs 本身只改 `s_sig_mgr`/`s_bak_sig_mgr`，不触碰 flags。

---

## 3. Rust 设计决策

> 设计契约见 `.design/03-design.v1.md`（中间产物，正式文档不引用）。以下为决策摘要与理由。

### 3.1 `PrivFlags` bitflags(u16)（D1）

11 个 s_flags 位（const.h:143-153）用 `bitflags` 建模，位值逐一对齐；预设组合（SRV_F/DSRV_F/RSYS_F/VM_F/USR_F/IMM_F，priv.h:45-50）作为关联常量。**理由**：C 用位而非 enum 是因为位正交组合（一个服务可同时是 `SYS_PROC|PREEMPTIBLE|ROOT_SYS_PROC`）；bitflags 保留正交性且编译期检查非法位。

### 3.2 `Privilege` struct（D2）——只建模 RS 读写字段

```rust
pub struct Privilege {
    pub id: PrivId,              // C s_id（kernel/priv.h:23）—— static priv id
    pub flags: PrivFlags,        // C s_flags（:24）
    pub init_flags: u32,         // C s_init_flags（:25）
    pub trap_mask: TrapMask,     // C s_trap_mask（:34）
    pub ipc_to: SysMap,          // C s_ipc_to（:35）—— 组合语义归 05
    pub k_call_mask: CallMask,   // C s_k_call_mask（:38）
    pub sig_mgr: Endpoint,       // C s_sig_mgr（:40）
    pub bak_sig_mgr: Endpoint,   // C s_bak_sig_mgr（:41）
    pub io_ranges: [IoRange; NR_IO_RANGE],   // C s_io_tab（:54）—— 透传
    pub nr_io_range: u16,        // C s_nr_io_range（:53）
    pub mem_ranges: [MemRange; NR_MEM_RANGE], // C s_mem_tab（:57）
    pub nr_mem_range: u16,       // C s_nr_mem_range（:56）
    pub irqs: [u32; NR_IRQ],     // C s_irq_tab（:60）
    pub nr_irq: u16,             // C s_nr_irq（:59）
}
```

**不建模字段（显式列出防遗漏误报）**：`s_proc_nr`（内核关联，getpriv 读回）、`s_asyntab/s_asynsize/s_asynendpoint`（异步发送）、`s_notify_pending/s_asyn_pending/s_int_pending/s_sig_pending`（挂起状态）、`s_ipcf`（IPC filter 指针）、`s_alarm_timer`、`s_stack_guard`、`s_diag_sig`、`s_grant_*`、`s_state_*`——全部是**内核运行态**，RS 不读写（19 接线时按 C 布局序列化整块）。

**调度参数不放这里**：`r_scheduler/r_priority/r_quantum/r_cpu`（type.h:92-95）是 **rproc 字段**（已建模于 ServiceSlot，02 §3.10），不是 priv 字段；`sched_init_proc` 从 `ServiceSlot` 读取它们。这避免在 Privilege 里重复存储。

### 3.3 `PrivCtlOp` enum（D3）——11 操作码全表

```rust
pub enum PrivCtlOp {
    Allow = 1, Disallow = 2, SetSys = 3, SetUser = 4,
    AddIo = 5, AddMem = 6, AddIrq = 7, QueryMem = 8,
    UpdateSys = 9, Yield = 10, ClearIpcRefs = 11,
}
```

判别值对齐 com.h:342-353。驱动面 4 个（AddIo/AddMem/AddIrq/QueryMem）**保留枚举但标注 defer**——minix-rs 无驱动面（A-10 同族），RS 不调用；保留是为了消息层（19）能序列化任意合法操作码。`KernelApi::privctl` 签名（boot.rs:84）从 `Option<&Priv>` 改为 `Option<&Privilege>`。

### 3.4 `srv_or_usr`（D4）——三态选择的类型化

`SRV_OR_USR(rp, X, Y)`（const.h:71）翻译为纯函数：

```rust
pub fn srv_or_usr<T: Copy>(is_sys_proc: bool, srv: T, usr: T) -> T {
    if is_sys_proc { srv } else { usr }
}
```

调用点（boot 装配）用 `flags.contains(PrivFlags::SYS_PROC)` 判定，编译期消除"读 `rp->r_priv.s_flags` 判定自身"的循环依赖（C 里 `SRV_OR_USR` 读的正是刚赋值的 `s_flags`，Rust 侧显式传 bool）。

### 3.5 `CallMask::from_calls`（D5）——fill_call_mask 的类型化

```rust
pub struct CallMask(pub u64);   // 64 位：kernel 58 调用 / VM 49 调用都装得下

impl CallMask {
    pub fn from_calls(calls: &[i32], tot_nr_calls: usize, call_base: i32, is_init: bool) -> Self
}
```

- `calls == [ALL_C]` → `u64::MAX` 截断到 `tot_nr_calls` 位
- 否则逐项 `set_bit(calls[i] - call_base)`；`is_init` 时先清零
- 哨兵常量 `ALL_C=-2`/`NO_C=-1`/`NULL_C=-3`（priv.h:28-30）用显式 const
- **理由**：C 的 `bitchunk_t[2]`（2×u32）在 Rust 里合并为 u64 更简单，位语义完全一致（58/49 位都不跨 64 位边界）；序列化边界归 19

`SysMap`（s_ipc_to，64 位）同样用 `u64` newtype，位 i = priv id i 可发。`TrapMask` 用 bitflags(u16)——`SRV_T=~0` 在 u16 里是 `0xFFFF`。

### 3.6 `sched.rs`（D6）——sched_init_proc 原语

```rust
pub struct SchedulerConfig {
    pub scheduler: Endpoint,   // KERNEL / SCHED_PROC_NR / NONE
    pub endpoint: Endpoint,    // 被调度进程
    pub parent: Endpoint,      // 恒为 RS_PROC_NR
    pub priority: i32,
    pub quantum: i32,
    pub cpu: i32,
}

pub fn sched_init_proc(cfg: &SchedulerConfig, is_sys_proc: bool)
    -> Result<Endpoint, i32>
```

- 断言等价（debug_assert）：`!is_sys_proc → scheduler == NONE`；`is_sys_proc → scheduler != NONE`
- 外部调用（sched_start 的 KERNEL→sys_schedctl / 用户调度器→SCHEDULING_START）经 `KernelApi` trait（01 的 boot.rs 边界）；`KernelApi::sched_init_proc` 已存在（boot.rs:86），19 接线前 fail-closed
- **NONE 短路契约**：`scheduler == NONE` 时 C 的 `sched_start` 直接返回 `OK` 且不发任何系统调用（sched_start.c:57-60，用户进程 INIT 场景）；Rust 侧 `sched_init_proc` 恒经 `KernelApi`，**19 接线时 KernelApi impl 必须复现该短路**（`scheduler == NONE → Ok(())` 不触内核）
- 返回 `Endpoint`（sched_start 的 `*newscheduler_e` 输出），调用方覆盖 `ServiceSlot.scheduler`

### 3.7 `update_sig_mgrs` 原语（D7）

```rust
pub fn update_sig_mgrs(
    priv_: &mut Privilege,
    sys: &mut dyn KernelApi,
    endpoint: Endpoint,
    sig_mgr: Endpoint,      // SELF 由调用方展开为 endpoint
    bak_sig_mgr: Endpoint,
) -> Result<(), i32>
```

顺序固定：`sys.getpriv(endpoint)?` → 设 `sig_mgr`/`bak_sig_mgr` → `sys.privctl(endpoint, PrivCtlOp::UpdateSys, Some(priv_))?`。`SELF` 常量展开（`sig_mgr == SELF ? endpoint : sig_mgr`）在调用方（12/16）做，本原语只接受具体 endpoint。

### 3.8 ARCH 标注（D8）

- **A-12**（错误映射）：所有外部调用返回 `Result<_, i32>`（errno），panic 路径保留 fail-closed（`UnimplementedKernelApi` 显式 `unimplemented!`，boot.rs:97-131）
- **A-11**（编译宏，计划标注）：`PRIV_DEBUG` 是内核侧 `do_privctl.c` 的条件打印（`#if PRIV_DEBUG`），RS 侧无对应打印物，当前不复制；若未来 RS 需要调试打印，计划挂 `#[cfg(feature = "priv-debug")]`（默认关，未落地——Cargo.toml 现无此 feature）。
- **A-10**（PCI/驱动面）：`SYS_PRIV_ADD_IO/MEM/IRQ/QUERY_MEM` 标注 defer（§3.3），`CHECK_*` 位保留在 PrivFlags（数据结构占位，fail-closed）
- 外部 syscall（sys_privctl/sys_getpriv/sched_start/sys_schedctl）统一经 `KernelApi` trait，签名契约归 19

---

## 4. 实现详解

### 4.1 privilege.rs 模块结构

```
os/servers/rs/src/privilege.rs
├─ PrivFlags bitflags(u16) + 预设组合（SRV_F/DSRV_F/RSYS_F/VM_F/USR_F/IMM_F）
├─ TrapMask bitflags(u16)（SRV_T=0xFFFF / USR_T=SENDREC 位）
├─ CallMask(u64) + from_calls + 哨兵常量（ALL_C/NO_C/NULL_C）
├─ SysMap(u64)（s_ipc_to）
├─ PrivId(i32) newtype + static_priv_id(endpoint) -> PrivId
├─ IoRange/MemRange（资源范围，透传）
├─ Privilege struct（§3.2）+ vacant()（C memset 等价）
├─ Privilege::boot_priv(entry, is_sys_proc) —— boot Step 1 装配（§4.2）
├─ srv_or_usr 纯函数
└─ PrivCtlOp enum（11 操作码，§3.3）
```

### 4.2 boot Step 1 装配：`Privilege::boot_priv`

对应 main.c:258-279 的装配公式：

```rust
pub fn boot_priv(flags: PrivFlags, endpoint_slot: i32) -> Privilege {
    // C: main.c:265-296 — boot Step 1 priv 装配（`flags` 来自
    // boot_image_priv_table 行的 flags，table.c:15-30）。
    let is_sys_proc = flags.contains(PrivFlags::SYS_PROC);
    Privilege {
        id: PrivId::static_priv_id(endpoint_slot),         // main.c:265-266
        flags,                                             // main.c:269
        init_flags: 0,                                     // SRV_I/USR_I = 0（main.c:270）
        trap_mask: TrapMask::srv_or_usr(is_sys_proc),      // main.c:271
        ipc_to: SysMap::all(),                             // fill_send_mask(ALL_M)，main.c:272-273
        k_call_mask: CallMask::from_calls(&[ALL_C, NULL_C], NR_SYS_CALLS, KERNEL_CALL, true),
                                                           // main.c:278-280
        sig_mgr: if is_sys_proc { Endpoint::RS } else { Endpoint::PM }, // main.c:274
        bak_sig_mgr: Endpoint::NONE,                       // main.c:275
        io_ranges: [IoRange::default(); NR_IO_RANGE],
        nr_io_range: 0,
        mem_ranges: [MemRange::default(); NR_MEM_RANGE],
        nr_mem_range: 0,
        irqs: [0; NR_IRQ],
        nr_irq: 0,
    }
}
```

`boot.rs` 的 Step 1 改为：装配 `Privilege` → `sys.privctl(ep, PrivCtlOp::SetSys, Some(&priv))`（RS/VM 例外）→ `sys.getpriv(ep)?` 读回并**覆盖本地**（内核改写版本）→ 存入 `ServiceSlot.priv`。

### 4.3 ServiceSlot 接线

`ServiceSlot` 增加两个字段（补齐 02 的机制归属缺口）：

```rust
pub struct ServiceSlot {
    ...
    /// Priv structure to be passed to the kernel. C: `r_priv`（type.h:88，03）。
    pub priv_: Privilege,
    ...
}
pub struct PublicSlot {
    ...
    /// VM call mask. C: `vm_call_mask`（rs.h:179，03/05）。
    pub vm_call_mask: CallMask,
    ...
}
```

`vm_call_mask` 的填充（main.c:317）放 `boot.rs` Step 1（`CallMask::from_calls(ALL_C, NR_VM_CALLS, VM_RQ_BASE, true)`），语义组合归 05。

### 4.4 sched.rs 模块结构

```
os/servers/rs/src/sched.rs
├─ SchedulerConfig struct（§3.6）
├─ sched_init_proc(cfg, is_sys_proc) -> Result<Endpoint, i32>
└─ update_sig_mgrs(priv_, sys, endpoint, sig_mgr, bak_sig_mgr) -> Result<(), i32>
```

`boot.rs` Step 2 调用 `sched_init_proc` 时从 `ServiceSlot` 构造 `SchedulerConfig`（scheduler/endpoint/RS_PROC_NR/priority/quantum/cpu）并经 `KernelApi::sched_init_proc` 发出。

### 4.5 KernelApi 签名变更

`boot.rs` 的 `Priv` 占位结构删除，`KernelApi` 签名更新：

```rust
fn privctl(&mut self, proc: Endpoint, op: PrivCtlOp, priv_: Option<&Privilege>) -> Result<(), i32>;
fn getpriv(&mut self, proc: Endpoint) -> Result<Privilege, i32>;
```

`PrivCtlOp` 从 boot.rs 移到 privilege.rs（全 11 操作码）；boot.rs re-export 保持 `crate::privilege::PrivCtlOp` 兼容性。

---

## 5. 测试要点

### 5.1 标志位对齐（PrivFlags/TrapMask）

- 11 个位值逐项断言 == const.h:143-153（`PREEMPTIBLE=0x002` … `RST_SYS_PROC=0x800`）
- 预设组合断言 == priv.h:45-50（`SRV_F=SYS_PROC|PREEMPTIBLE` 等）
- `TrapMask::SRV_T == 0xFFFF`（u16 的 `~0`）、`USR_T` 含 SENDREC 位

### 5.2 哨兵与 static_priv_id

- `ALL_C=-2`/`NO_C=-1`/`NULL_C=-3` 常量断言
- `static_priv_id(2) == 7`（RS，NR_TASKS=5）、`static_priv_id(11) == 16`（INIT，= USER_PRIV_ID）

### 5.3 CallMask::from_calls

- `[ALL_C]` → 全 1（58 位内全 1，59 位以上为 0）
- 单调用 `[KERNEL_CALL + 4]` → 仅位 4 置位
- `is_init=true` 先清零；`is_init=false` 时 C 不预清零（调用方须传预清零缓冲，utility.c:126-131），Rust 值类型恒新恒 0，等价于预清零（无独立测试，语义 N/A）
- `NULL_C` 截断（calls 数组含 NULL_C 停止计数）

### 5.4 PrivCtlOp 判别

- 11 个操作码判别值 == com.h:342-353（`Allow=1`…`ClearIpcRefs=11`）

### 5.5 sched_init_proc

- 用户进程（!SYS_PROC）scheduler 必须 NONE（debug_assert 触发路径）
- 系统进程 scheduler 非 NONE
- fail-closed 设计（非测试点）：`UnimplementedKernelApi` 的 `sched_init_proc` 显式 `unimplemented!()` panic（19 接线前生产占位；无专项 `#[should_panic]` 测试，防御路径由 Mock 不触发保证）

### 5.6 boot 集成（boot.rs 既有测试扩展）

- Step 1 的 `privctl(SetSys)` 计数仍为 10（RS/VM 例外，main.c:285-291）
- Step 1 新增断言：`getpriv` 调用计数 == 12（全服务含 RS/VM），且每个 slot 的 `priv_` == `MockKernelApi::getpriv` 返回值（`Privilege::vacant()`，boot.rs `test_init_fresh_step_order`）

### 5.7 测试统计（截至 2026-08-15）

03 范围：`privilege.rs` 11 项 + `sched.rs` 4 项 = 15 项稳定值；boot 集成扩展 2 条断言（§5.6）落在 `test_init_fresh_step_order`。全 crate 计数随并行模块增长（2026-08-15 快照 181/181），不以 03 doc 承诺。

---

## 6. 过渡

`01-rs-boot-init.md` 建立了 boot 时序，`02-rs-process-table.md` 建立了登记表，本文档建立了登记表行里的**权限结构**（`r_priv`）与 privctl 操作面。boot Step 1 的权限装配到此完整：flags/trap_mask/ipc_to/k_call_mask/sig_mgr 全部有类型化表达。

下一步 `04-rs-access-control.md`：RS 自己的请求入口怎么校验调用者权限（`check_call_permission`/`caller_is_root`/`caller_can_control`）——内核用 priv 表管"服务能做什么"，RS 用访问控制管"谁有资格命令 RS 做什么"。

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md` — boot 时序（Step 1/2 锚点）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` — r_priv/vm_call_mask 字段归属
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/05-rs-ipc-sendmask.md` — send mask 组合语义（fill_send_mask 的消费方）
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/22-privilege.md` — kernel 侧 priv 表与 privctl 实现
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/19-rs-external-interfaces.md` — sys_privctl/sys_getpriv/sched_start 签名契约
- `minix3/minix/kernel/priv.h`、`minix3/minix/include/minix/priv.h` — priv 结构与默认宏
- `minix3/minix/include/minix/const.h:142-153`、`minix3/minix/include/minix/com.h:342-353` — 标志位与操作码
- `minix3/minix/servers/rs/main.c:240-345`、`utility.c:82-141,364-422` — boot Step 1 与原语
- `minix3/minix/kernel/system/do_privctl.c` — privctl 内核侧语义
- `minix3/minix/lib/libsys/sys_privctl.c`、`minix3/minix/lib/libsys/sched_start.c` — 外部调用面
- `os/servers/rs/src/privilege.rs`、`os/servers/rs/src/sched.rs` — Rust 实现
