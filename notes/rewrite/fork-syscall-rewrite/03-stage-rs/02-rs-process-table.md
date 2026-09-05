# 02-rs-process-table: 服务登记表（rproc / rprocpub）

> **分类**: 阶段 1 — 启动入口与初始化骨架（boot Step 1 的 slot 建立底座）
> **源码**: `minix3/minix/servers/rs/type.h`（112 行）、`minix3/minix/servers/rs/glo.h`（58 行）、`minix3/minix/servers/rs/const.h`（123 行）、`minix3/minix/servers/rs/manager.c:1935-2109`（槽位管理原语）、`minix3/minix/servers/rs/utility.c:352-359`（`rs_isokendpt`）、`minix3/minix/servers/rs/manager.c:1334-1352`（`get_service_instances`）、`minix3/minix/include/minix/rs.h`（`rprocpub`/`SF_*`）、`minix3/minix/include/minix/sef.h`（`sef_init_info_t`）
> **Rust 模块**: `os/servers/rs/src/service_slot.rs`、`os/servers/rs/src/process_table.rs`（新建）；`os/servers/rs/src/boot.rs`（接线）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/00-rs-overview.md`、`notes/rewrite/fork-syscall-rewrite/03-stage-rs/01-rs-boot-init.md`（boot 骨架，本文档是 Step 1 建立 slot 的数据底座）
> **说明**: 本文档是 RS 的数据底座：系统服务登记表（`rproc`/`rprocpub`/`rproc_ptr`）的完整字段模型、16 位 `r_flags` 与 13 位 `sys_flags` 全表、槽位管理原语（5 个 `lookup_slot_by_*`/`alloc_slot`/`free_slot`/`rs_isokendpt`）、ARCH A-3（裸指针四链 → 索引链）与 A-4（`rproc_ptr` → 数组索引）。**本文档只建立数据结构与槽位原语**；每个字段的机制语义（何时写入、何时读取、状态如何迁移）属于各自的机制文档。

---

## 1. 概念：服务登记表——RS 如何"记住"每一个服务

### 1.0 章节引言

`01-rs-boot-init.md` 回答了 RS 如何把自己启动好：`sef_cb_init_fresh()` 四步 boot。Step 1（`main.c:244-346`）的产出之一就是**在 RS 本地建立每个 boot 服务的槽位**（`main.c:255-345`）。本文档回答的问题是：这个"槽位"到底是什么数据结构，RS 在运行时如何查找、分配、回收它。

> **本章不讲什么**（机制一律移交）:
> - priv 结构字段与 `sys_privctl` 每操作的效果（`03-rs-privilege.md`）
> - IPC send mask 与 `r_ipc_list` 的填充语义（`05-rs-ipc-sendmask.md`）
> - 主循环的 `reply`/`late_reply`/`EDONTREPLY` 与 `rs_isokendpt` 的调用上下文（`06-rs-main-loop.md`）
> - 心跳字段（`r_period`/`r_alive_tm`/`r_check_tm`/`r_stop_tm`）的读写时机（`07-rs-period-heartbeat.md`）
> - `init_slot`/`edit_slot` 如何填充命令串、参数、control 列表（`08-rs-slot-config.md`）
> - exec image（`r_exec`）的加载与共享（`09-rs-exec.md`）
> - `r_prev_rp`/`r_next_rp` 副本链如何被 clone/update 使用（`10-rs-service-create.md`、`16-rs-live-update.md`）
> - 终止/恢复如何清理槽位（`15-rs-terminate-restart.md`）
> - update 描述符（`r_upd`/`rupdate`）的状态机（`16-rs-live-update.md`）
>
> 本章只回答一个问题：**登记表由哪些结构组成、每个字段是什么、槽位如何被查找/分配/回收**。

### 1.1 核心问题：为什么需要一张登记表

RS 是"加载并启动其余用户服务"的角色（`00-rs-overview.md`）。它要回答三类问题：

```
这个服务在不在？        → 按 label/pid/dev_nr/domain 查找（lookup_slot_by_*）
这个服务现在什么状态？   → r_flags（运行中/退出中/已死/更新中...）
我还能不能再启动一个？   → 槽位是否耗尽（alloc_slot → ENOMEM）
```

Minix3 的解法是一张**固定大小的系统服务登记表**：`rproc[NR_SYS_PROCS]`（`glo.h:34`，64 个槽）。它不是进程表（进程表是 PM 的 `mproc` 或内核的 `proc[]`），而是**只登记系统服务（server 与 driver）**的表——普通用户进程不在这里。这张表是 RS 一切工作的数据锚点：启动（Step 1 建槽）、监控（心跳）、控制（RS_UP/RS_DOWN）、恢复（terminate/restart）、更新（live update）都以"某个槽"为操作对象。

### 1.2 双表结构：私有表与公开表

C 里登记表拆成两半：

| 表 | 声明 | 内容 | 谁可见 |
|----|------|------|--------|
| `rproc[NR_SYS_PROCS]` | `glo.h:34` | 全部状态：命令串、exec image、priv 结构、监控时间戳、更新链 | RS 私有 |
| `rprocpub[NR_SYS_PROCS]` | `glo.h:33` | 公开子集：label/proc_name、endpoint、dev_nr、sys_flags、vm_call_mask、PCI ACL | 经 grant 共享给其他服务（`rinit.rproctab_gid`，main.c:185） |

每个 `rproc` 行内有一个 `r_pub` 指针指向**同一索引**的 `rprocpub`（`main.c:233` 表重置时建立：`rp->r_pub = &rprocpub[rp - rproc]`）。公开表是"服务可以被其他服务看到的最小身份信息"——例如 VFS 想按 label 查询某个服务的端点，读的是公开表。`rinit.rproctab_gid`（创建点 main.c:185，消费在 12 的 `init_service`）把这个公开表以只读 grant 暴露给 `ANY`。

**Rust 设计含义**：`r_pub` 是"行内自引用指针"——Rust 里等价物是**把两个结构合并为一行**（`ServiceSlot { pub_: PublicSlot, ... }`），指针变成普通字段访问（见 §3.1，ARCH A-3 同一思路：消灭裸指针）。

### 1.3 状态与策略标志：r_flags / sys_flags

登记表的每一行有两组标志，必须分清：

- **`r_flags`（16 位，`const.h:28-43`）——运行状态**：表示"这个槽现在是什么状态"。`RS_IN_USE`（槽被占用）、`RS_ACTIVE`（是服务的活动实例）、`RS_EXITING`（退出已启动）、`RS_TERMINATED`（已终止）、`RS_DEAD`（可回收）……这些位**正交组合**：一个服务可以同时是 `IN_USE|ACTIVE|INITIALIZING`（正在初始化的活动服务）。位不是互斥枚举，这正是 C 用位而不用 enum 的原因。
- **`sys_flags`（13 位，`rs.h:191-206`）——服务能力/策略**：表示"这个服务需要什么"。`SF_CORE_SRV`（核心服务）、`SF_SYNCH_BOOT`（同步 boot）、`SF_NEED_REPL`（需要副本）……这些位来自 RS_UP 请求或 boot 表，**在服务生命周期内基本不变**（`IMM_SF` 位不可变，rs.h:205-206）。

**为什么拆两组**：状态位决定"现在怎么办"（监控、清理），能力位决定"当初怎么启动、以后怎么恢复"（副本、脚本、更新）。两组位在 C 中一个在 `rproc`（`r_flags`），一个在 `rprocpub`（`sys_flags`）——正好对应"私有状态"与"公开能力"的边界。

### 1.4 槽位生命周期与快速索引

登记表的核心操作模式：

```
alloc_slot() 找空闲槽 ──► 填充身份/能力（label/endpoint/sys_flags...）
    │                          │
    │                          ▼
    │                    RS_IN_USE | RS_ACTIVE（激活）
    │                          │
    │                          ▼
    │                    运行期：lookup_slot_by_* 按需查找
    │                          │
    │                          ▼
    └── free_slot() 清理 ◄── RS_DEAD（待清理）◄── 终止/退出
```

两个性能关键点：

1. **端点→槽的反查要 O(1)**：主循环收到消息后要立刻知道"谁发的"（`rproc_ptr[who_p]`，main.c:86）。C 用 `rproc_ptr[NR_PROCS]`（`glo.h:35`）——以端点槽号为下标的指针数组（ARCH A-4）。
2. **副本链不能悬垂**：一个服务可以有多个实例（旧版本/新版本/副本），C 用四根裸指针 `r_old_rp/r_new_rp/r_prev_rp/r_next_rp`（type.h:68-71）串起来，`get_service_instances`（manager.c:1334-1352）把 rp 及其 prev/next/old/new 收集进一个**静态 5 槽数组**。Rust 用 `Option<SlotId>` 索引链 + 迭代器替代（ARCH A-3）。

---

## 2. C 源码分析

> 本文档 ground truth 为 `minix3/minix/servers/rs/type.h`、`glo.h`、`const.h`、`manager.c:1935-2109`、`utility.c:352-359`、`manager.c:1334-1352` 与 `minix3/minix/include/minix/rs.h`、`sef.h`。所有行号以 grep 实证为准。

### 2.1 全局表声明（glo.h:33-55）

```c
EXTERN struct rprocpub rprocpub[NR_SYS_PROCS];  /* public entries */   /* 33 */
EXTERN struct rproc rproc[NR_SYS_PROCS];                                /* 34 */
EXTERN struct rproc *rproc_ptr[NR_PROCS];       /* mapping for fast access */ /* 35 */

/* Global init descriptor. */                                           /* 38-40 */
EXTERN sef_init_info_t rinit;

/* Global update descriptor. */                                          /* 43-45 */
EXTERN struct rupdate rupdate;

EXTERN long rs_verbose;                                                 /* 48 */
EXTERN int shutting_down;                                               /* 51 */
EXTERN unsigned system_hz;                                              /* 53 */
EXTERN struct machine machine;		/* machine info */               /* 55 */
```

`EXTERN` 是 `#ifdef _TABLE` 下展开为空/`extern` 的惯用法（`table.c` 定义，其余文件引用）。**本文档只关心数据形状**：三张表（`rproc`/`rprocpub`/`rproc_ptr`）+ 两个全局描述符（`rinit`/`rupdate`）+ 三个全局标量。`rs_verbose`/`shutting_down`/`system_hz`/`machine` 的语义分别归 01/16/07/01。

### 2.2 尺寸常量（sys_config.h / com.h）

| 常量 | 值 | 定义处 | 含义 |
|------|----|--------|------|
| `NR_PROCS` | 256 | `config.h:31`（=`_NR_PROCS`，`sys_config.h:8`） | 系统进程槽上限（`rproc_ptr` 下标范围） |
| `NR_SYS_PROCS` | 64 | `config.h:32`（=`_NR_SYS_PROCS`，`sys_config.h:9`） | 系统服务槽数（`rproc`/`rprocpub` 长度） |
| `NR_TASKS` | 5 | `com.h:56` | 内核任务数（`rs_isokendpt` 下界） |
| `NR_BOOT_PROCS` | 17 | `param.h:9` | boot 映像条目数（= `NR_TASKS + LAST_SPECIAL_PROC_NR + 1`） |

**关键约束**：`rproc` 表长 64，但 boot 服务的槽位索引 = **priv 表索引**（`main.c:255` `rp = &rproc[boot_image_priv - boot_image_priv_table]`），即 12 个 boot 服务占用槽 0~11，其余 52 个槽留给运行期 `RS_UP` 动态服务。

### 2.3 struct rproc 全字段（type.h:56-108）

```c
typedef struct priv ixfer_priv_s;                                       /* 55 */
struct rproc {
  struct rprocpub *r_pub;       /* pointer to the corresponding public entry */  /* 57 */
  struct rproc *r_old_rp;       /* pointer to the slot with the old version */   /* 58 */
  struct rproc *r_new_rp;       /* pointer to the slot with the new version */   /* 59 */
  struct rproc *r_prev_rp;      /* pointer to the slot with the prev replica */  /* 60 */
  struct rproc *r_next_rp;      /* pointer to the slot with the next replica */  /* 61 */
  struct rprocupd r_upd;        /* update descriptor */                          /* 62 */
  pid_t r_pid;			/* process id, -1 if the process is not there */   /* 63 */
  int r_asr_count;		/* number of live updates with ASR */              /* 78 */
  int r_restarts;		/* number of restarts (initially zero) */          /* 78 */
  long r_backoff;		/* number of periods to wait before revive */      /* 78 */
  unsigned r_flags; 		/* status and policy flags */                      /* 78 */
  int r_init_err;               /* error code at initialization time */           /* 78 */
  long r_period;		/* heartbeat period (or zero) */                    /* 78 */
  clock_t r_check_tm;		/* timestamp of last check */                     /* 79 */
  clock_t r_alive_tm;		/* timestamp of last heartbeat */                 /* 78 */
  clock_t r_stop_tm;		/* timestamp of SIGTERM signal */                 /* 79 */
  endpoint_t r_caller;		/* RS_LATEREPLY caller */                        /* 78 */
  int r_caller_request;		/* RS_LATEREPLY caller request */                /* 79 */
  char r_cmd[MAX_COMMAND_LEN];	/* raw command plus arguments */                 /* 78 */
  char r_args[MAX_COMMAND_LEN];	/* null-separated raw command plus arguments */ /* 79 */
#define ARGV_ELEMENTS (MAX_NR_ARGS+2) /* path, args, null */                     /* 80 */
  char *r_argv[ARGV_ELEMENTS];                                                   /* 81 */
  int r_argc;  			/* number of arguments */                        /* 82 */
  char r_script[MAX_SCRIPT_LEN]; /* name of the restart script executable */     /* 83 */
  char *r_exec;			/* Executable image */                           /* 86 */
  size_t r_exec_len;		/* Length of image */                            /* 86 */
  ixfer_priv_s r_priv;		/* Privilege structure to be passed to the kernel. */ /* 88-90 */
  uid_t r_uid;                                                                   /* 106 */
  endpoint_t r_scheduler;	/* scheduler */                                    /* 95 */
  int r_priority;		/* negative values are reserved for special meanings */ /* 105 */
  int r_quantum;                                                                 /* 106 */
  int r_cpu;                                                                     /* 95 */
  vir_bytes r_map_prealloc_addr; /* preallocated mmap address */                 /* 105 */
  size_t r_map_prealloc_len;     /* preallocated mmap len */                     /* 106 */
  struct io_range r_io_tab[NR_IO_RANGE];                                         /* 105 */
  int r_nr_io_range;                                                             /* 106 */
  int r_irq_tab[NR_IRQ];                                                         /* 107 */
  int r_nr_irq;                                                                  /* 108 */
  char r_ipc_list[MAX_IPC_LIST];                                                 /* 105 */
  int r_nr_control;                                                              /* 106 */
  char r_control[RS_NR_CONTROL][RS_MAX_LABEL_LEN];                               /* 107 */
};                                                                               /* 108 */
```

字段按语义分组（本文档的字段表，归属标注"→ doc N"表示机制在该文档展开）：

| 分组 | 字段 | 归属 |
|------|------|------|
| 公开指针 | `r_pub` | **本文档**（行内合并，§3.1） |
| 实例链 | `r_old_rp`/`r_new_rp`/`r_prev_rp`/`r_next_rp` | **本文档**（ARCH A-3）+ 10/16 使用 |
| 更新描述符 | `r_upd` | 16 |
| 身份 | `r_pid`（-1 = 无） | **本文档**（Step 4 写入）+ 02 查找 |
| 恢复计数 | `r_asr_count`/`r_restarts`/`r_backoff` | 15（backoff 恢复） |
| 状态 | `r_flags`（16 位） | **本文档**（位表）+ 各机制写入 |
| 初始化结果 | `r_init_err` | 12（init 错误码） |
| 心跳监控 | `r_period`/`r_check_tm`/`r_alive_tm`/`r_stop_tm` | 07 |
| 延迟回复 | `r_caller`/`r_caller_request` | 06（`RS_LATEREPLY`） |
| 命令/参数 | `r_cmd`/`r_args`/`r_argv`/`r_argc` | 08（`init_slot`） |
| 恢复脚本 | `r_script` | 15（`run_script`） |
| exec image | `r_exec`/`r_exec_len` | 09（ARCH A-5） |
| 权限 | `r_priv`（`ixfer_priv_s` = 内核 `struct priv` 的可传输视图） | 03 |
| 身份/调度 | `r_uid`/`r_scheduler`/`r_priority`/`r_quantum`/`r_cpu` | 03（调度参数）+ 10（启动） |
| 更新预分配 | `r_map_prealloc_addr`/`r_map_prealloc_len` | 16（VM multi-component） |
| IO/IRQ 备份 | `r_io_tab`/`r_nr_io_range`/`r_irq_tab`/`r_nr_irq` | 03（priv 备份，`edit_slot` 重建用） |
| IPC 列表 | `r_ipc_list` | 05（`add_forward_ipc`/`add_backward_ipc`） |
| 控制列表 | `r_nr_control`/`r_control` | 08（`RS_UP` 的 `rss_control`） |

**几个必须注意的 C 事实**：

1. **`r_argv` 是指向 `r_args` 内部的指针数组**（type.h:81 + 注释）：`r_args` 是 NUL 分隔的参数字符串，`r_argv[0..r_argc]` 指向其中每个参数的起始位置。这是"指针进缓冲"模式——Rust 移动/复制结构时这些指针会悬垂（§3.8 的演进决策）。
2. **`r_pid` 的 -1 语义**：`free_slot`（manager.c:2106）把 `r_pid` 重置为 -1，`lookup_slot_by_pid`（manager.c:1965-1967）对 `pid < 0` 直接返回 NULL。
3. **`r_io_tab`/`r_irq_tab` 是 priv 结构的备份**：`edit_slot`（08）用它们重建 `r_priv`（`io_range` 定义在 `minix3/minix/include/minix/type.h:133-137`，含 `ior_base`/`ior_limit`）。

### 2.4 struct rprocpub 全字段（rs.h:165-183）

```c
struct rprocpub {
  short in_use; 		  /* set when the entry is in use */       /* 166 */
  unsigned sys_flags; 		  /* sys flags */                          /* 167 */
  endpoint_t endpoint;		  /* process endpoint number */           /* 168 */
  endpoint_t old_endpoint;	  /* old instance endpoint number (for VM, when updating) */ /* 169 */
  endpoint_t new_endpoint;	  /* new instance endpoint number (for VM, when updating) */ /* 170 */
  devmajor_t dev_nr;		  /* major device number or NO_DEV */     /* 171 */
  int nr_domain;		  /* number of socket driver domains */   /* 172 */
  int domain[NR_DOMAIN];	  /* set of socket driver domains */     /* 173 */
  char label[RS_MAX_LABEL_LEN];	  /* label of this service */            /* 174 */
  char proc_name[RS_MAX_LABEL_LEN]; /* process name of this service */    /* 175 */
  bitchunk_t vm_call_mask[VM_CALL_MASK_SIZE]; /* vm call mask */          /* 176 */
  struct rs_pci pci_acl;	  /* pci acl */                          /* 177 */
  int devman_id;                                                            /* 178 */
};                                                                          /* 179 */
```

| 字段 | 写入点 | 归属 |
|------|--------|------|
| `in_use` | boot 表重置（main.c:234）、`free_slot`（manager.c:2107） | **本文档** |
| `sys_flags` | boot Step 1（main.c:301）、`init_slot`（08） | **本文档**（位表）+ 08 |
| `endpoint` | boot Step 1（main.c:325）、`init_slot`（08） | **本文档** |
| `old_endpoint`/`new_endpoint` | 16（update 时记录 VM 旧/新实例端点） | 16 |
| `dev_nr` | boot Step 1（main.c:306）、`init_slot`（08） | **本文档**（查找用） |
| `nr_domain`/`domain` | 08（`init_slot`，`rss_domain`） | **本文档**（查找用）+ 08 |
| `label`/`proc_name` | boot Step 1（main.c:262/313）、`copy_label`（08） | **本文档**（查找用）+ 08 |
| `vm_call_mask` | boot Step 1（main.c:316-317）、`init_slot`（08） | 03/05（掩码填充） |
| `pci_acl` | 11（`publish_service` 的 `pci_set_acl`） | 11 |
| `devman_id` | 11（devman bind） | 11 |

**`NO_DEV` 语义**：`dev_nr` 是 `devmajor_t`（major device number），无设备时为 `NO_DEV`（本树 `NO_DEV = ((dev_t) 0)`，`minix3/minix/include/minix/const.h:132`；rs 代码以 `dev_nr > 0` 判定有效设备，request.c:76 / manager.c:806）。`lookup_slot_by_dev_nr` 对 `dev_nr <= 0` 直接返回 NULL（manager.c:1992-1993），所以 `NO_DEV` 槽永远不会被按设备号查到。

### 2.5 r_flags 16 位（const.h:28-43）与 RS_SRV_IS_IDLE（const.h:45）

```c
#define RS_IN_USE       0x001    /* set when process slot is in use */      /* 28 */
#define RS_EXITING      0x002    /* set when exit is expected */            /* 29 */
#define RS_REFRESHING   0x004    /* set when refresh must be done */        /* 30 */
#define RS_NOPINGREPLY  0x008    /* service failed to reply to a ping request */ /* 31 */
#define RS_TERMINATED   0x010    /* service has terminated */               /* 32 */
#define RS_LATEREPLY    0x020    /* no reply sent to RS_DOWN caller yet */  /* 33 */
#define RS_INITIALIZING 0x040    /* set when init is in progress */         /* 34 */
#define RS_UPDATING     0x080    /* set when update is in progress */       /* 35 */
#define RS_PREPARE_DONE 0x100    /* set when updating and preparation is done */ /* 36 */
#define RS_INIT_DONE    0x200    /* set when updating and init is done */   /* 37 */
#define RS_INIT_PENDING 0x400    /* set when updating and init is pending */ /* 38 */
#define RS_ACTIVE       0x800    /* set for the active instance of a service */ /* 39 */
#define RS_DEAD         0x1000   /* set for an instance ready to be cleaned up */ /* 40 */
#define RS_CLEANUP_DETACH 0x2000 /* detach at cleanup time */               /* 41 */
#define RS_CLEANUP_SCRIPT 0x4000 /* run script at cleanup time */           /* 42 */
#define RS_REINCARNATE    0x8000 /* after exit, restart with a new endpoint */ /* 43 */

#define RS_SRV_IS_IDLE(S) (((S)->r_flags & RS_DEAD) || \
    ((S)->r_flags & ~(RS_IN_USE|RS_ACTIVE|RS_CLEANUP_DETACH|RS_CLEANUP_SCRIPT)) == 0) /* 45 */
```

语义分组：

| 组 | 位 | 含义 | 主要写者 |
|----|----|------|---------|
| 占用 | `RS_IN_USE` | 槽被占用（槽位生命周期锚） | boot Step 1（main.c:343）、`init_slot`（08） |
| 实例 | `RS_ACTIVE` | 服务的活动实例（同一服务可有多个非活动实例：旧版/副本） | boot Step 1、`activate_service`（10） |
| 退出 | `RS_EXITING`/`RS_TERMINATED`/`RS_DEAD` | 退出流程三段：预期退出 → 已终止 → 可回收 | 13/15 |
| 监控 | `RS_NOPINGREPLY` | ping 无回复 | 07（`do_period`） |
| 回复 | `RS_LATEREPLY` | RS_DOWN 等调用者等待延迟回复 | 13/06（`late_reply`） |
| 初始化 | `RS_INITIALIZING`/`RS_INIT_DONE`/`RS_INIT_PENDING` | init 进行中/完成/待 init | 12/16 |
| 更新 | `RS_UPDATING`/`RS_PREPARE_DONE` | 更新进行中/准备完成 | 16 |
| 刷新 | `RS_REFRESHING` | refresh 待执行 | 13 |
| 清理 | `RS_CLEANUP_DETACH`/`RS_CLEANUP_SCRIPT` | 清理时 detach/跑脚本 | 15（仅这两处） |
| 复活 | `RS_REINCARNATE` | 退出后用新端点重启 | 15（仅 terminate_service 消费） |

**`RS_SRV_IS_IDLE(S)` 展开**（const.h:45）：槽空闲 ⟺ `RS_DEAD` 置位，**或** 除 `RS_IN_USE|RS_ACTIVE|RS_CLEANUP_DETACH|RS_CLEANUP_SCRIPT` 之外的位全为 0。直观含义：一个"活着的空闲槽"是 `IN_USE|ACTIVE` 且没有任何进行中的状态（不在退出/初始化/更新/延迟回复中），或者已标记 `DEAD` 待回收。`rs_idle_period`（06）用它判断槽是否可以清理/补副本。

### 2.6 sys_flags 13 位（rs.h:191-206）与预设组合（const.h:65-68）

```c
#define SF_CORE_SRV     0x001    /* set for core system services */   /* 191 */
#define SF_SYNCH_BOOT   0X002    /* set when process needs synch boot init */ /* 192 */
#define SF_NEED_COPY    0x004    /* set when process needs copy to start */   /* 193 */
#define SF_USE_COPY     0x008    /* set when process has a copy in memory */  /* 194 */
#define SF_NEED_REPL    0x010    /* set when process needs replica to start */ /* 195 */
#define SF_USE_REPL     0x020    /* set when process has a replica */          /* 196 */
#define SF_VM_UPDATE    0x040    /* set when process needs vm update */        /* 197 */
#define SF_VM_ROLLBACK  0x080    /* set when vm update is a rollback */        /* 198 */
#define SF_VM_NOMMAP    0x100    /* set when vm update ignores mmapped regions */ /* 199 */
#define SF_USE_SCRIPT   0x200    /* set when process has restart script */     /* 200 */
#define SF_DET_RESTART  0x400    /* set when process detaches on restart */    /* 201 */
#define SF_NORESTART    0x800    /* set when process should not be restarted */ /* 202 */
#define SF_NO_BIN_EXP  0x1000    /* set when we should ignore binary exp. offset */ /* 203 */

#define IMM_SF  (SF_NO_BIN_EXP | SF_CORE_SRV | SF_SYNCH_BOOT | \
                 SF_NEED_COPY | SF_NEED_REPL) /* immutable */          /* 205-206 */
```

预设组合（const.h:65-68，`table.c` 的 sys 表使用）：

```c
#define SRV_SF   (SF_CORE_SRV)                 /* system services */          /* 65 */
#define SRVR_SF  (SRV_SF | SF_NEED_REPL)       /* services needing a replica */ /* 66 */
#define DSRV_SF  (0)                           /* dynamic system services */   /* 67 */
#define VM_SF    (SRVR_SF)     			/* vm */                         /* 68 */
```

`IMM_SF`（rs.h:205-206 注释 "immutable"）是 `init_slot` 时从旧槽继承的**不可变位**（08 的 `inherit_service_defaults` 用它保证：副本/更新后的新实例不能改变 `NO_BIN_EXP|CORE_SRV|SYNCH_BOOT|NEED_COPY|NEED_REPL` 这些启动契约位）。

### 2.7 rprocupd / rupdate：更新描述符（type.h:30-54）

```c
struct rprocupd {
  int lu_flags;		   /* user-specified live update flags */  /* 31 */
  int init_flags;		   /* user-specified init flags */      /* 32 */
  int prepare_state;       /* the state the process has to prepare for the update */ /* 33 */
  endpoint_t state_endpoint; /* the custom process to transfer the state from (if any). */ /* 34 */
  clock_t prepare_tm;      /* timestamp of when the update was scheduled */  /* 35 */
  clock_t prepare_maxtime; /* max time to wait for the process to be ready */ /* 36 */
  struct rproc *rp;        /* the process under update */                     /* 37 */
  struct rs_state_data prepare_state_data; /* state data for the update */   /* 38 */
  cp_grant_id_t prepare_state_data_gid; /* state data gid */                  /* 39 */
  struct rprocupd *prev_rpupd;   /* the previous process under update */      /* 40 */
  struct rprocupd *next_rpupd;   /* the next process under update */          /* 41 */
};
struct rupdate {
  int flags;               /* flags to keep track of the status of the update */ /* 44 */
  int num_rpupds;          /* number of descriptors scheduled for the update */  /* 45 */
  int num_init_ready_pending;   /* number of pending init ready messages */      /* 46 */
  struct rprocupd *curr_rpupd;  /* the current descriptor under update */       /* 47 */
  struct rprocupd *first_rpupd; /* first descriptor scheduled for the update */ /* 48 */
  struct rprocupd *last_rpupd;  /* last descriptor scheduled for the update */  /* 49 */
  struct rprocupd *vm_rpupd;    /* VM descriptor scheduled for the update */    /* 50 */
  struct rprocupd *rs_rpupd;    /* RS descriptor scheduled for the update */    /* 51 */
};
```

这两个结构是 Live Update 的数据载体：每个被更新的服务一个 `rprocupd`（经 `prev_rpupd`/`next_rpupd` 双向链串成更新队列），`rupdate` 是全局更新描述符（当前/首/尾/VM/RS 描述符 + 计数）。**本文档只陈述数据结构**：`rprocupd` 嵌入在 `rproc.r_upd`（type.h:62），`rupdate` 是全局（glo.h:45）；双向链在 Rust 中同样以 `Option<SlotId>` 索引化（A-3，§3.4）。状态机（`RUPDATE_*` 宏、prepare/update/init/end 阶段）全部归 `16-rs-live-update.md`。

### 2.8 rinit：全局 init 描述符（sef.h:42-53）

```c
typedef struct {
    int flags;                    /* 42 */
    cp_grant_id_t rproctab_gid;   /* 43 */
    endpoint_t endpoint;          /* 44 */
    endpoint_t old_endpoint;      /* 45 */
    int restarts;                 /* 46 */
    void* init_buff_start;        /* 47 */
    void* init_buff_cleanup_start;/* 48 */
    size_t init_buff_len;         /* 49 */
    int copy_flags;               /* 50 */
    int prepare_state;            /* 51 */
} sef_init_info_t;                /* 52-53 */
```

`rinit`（glo.h:40）是 RS 为**即将初始化的服务**准备的初始化信息（boot Step 2 的 `init_service` 构造 RS_INIT 消息时使用）。机制归 12；本文档只登记：`rproctab_gid` 的创建点在 boot（main.c:185），字段表见 12。

### 2.9 槽位管理原语（manager.c:1935-2109）

五个查找函数 + 分配/释放。**先看共同骨架**，再看差异：

```c
struct rproc* lookup_slot_by_label(char *label)                          /* 1935 */
{
  for (slot_nr = 0; slot_nr < NR_SYS_PROCS; slot_nr++) {                 /* 1939 */
      rp = &rproc[slot_nr];
      if (!(rp->r_flags & RS_ACTIVE)) {                                  /* 1942：过滤条件 */
          continue;
      }
      rpub = rp->r_pub;
      if (strcmp(rpub->label, label) == 0) {                             /* 1944 */
          return rp;
      }
  }
  return NULL;                                                           /* 1948 */
}
```

**过滤条件差异表**（这是最容易抄错的地方）：

| 函数 | 行号 | 无效入参 | 过滤条件 | 匹配字段 |
|------|------|---------|---------|---------|
| `lookup_slot_by_label` | 1935-1954 | —（label 非空由调用方保证） | **`RS_ACTIVE`** | `rpub->label`（strcmp） |
| `lookup_slot_by_pid` | 1959-1980 | `pid < 0` → NULL | `RS_IN_USE` | `rp->r_pid` |
| `lookup_slot_by_dev_nr` | 1985-2008 | `dev_nr <= 0` → NULL | `RS_IN_USE` | `rpub->dev_nr` |
| `lookup_slot_by_domain` | 2013-2036 | `domain <= 0` → NULL | `RS_IN_USE` | `rpub->domain[0..nr_domain]` |
| `lookup_slot_by_flags` | 2041-2062 | `flags == 0` → NULL | `RS_IN_USE` | `rp->r_flags & flags`（**任一**位命中） |

- `lookup_slot_by_label` 用 `RS_ACTIVE` 而非 `RS_IN_USE`：**只有活动实例可被按 label 找到**（`do_up` 重复检查、`do_lookup` 等都要求"活动服务"）。其余四个用 `RS_IN_USE`（槽被占用即可，不要求活动——例如按 pid 找退出中的服务做清理）。
- `lookup_slot_by_domain` 遍历 `rpub->domain[0..nr_domain]`（套接字驱动域），任一匹配即返回。
- `lookup_slot_by_flags` 是"任一位置位"语义（`rp->r_flags & flags` 非零），不是全位匹配。

**alloc_slot / free_slot**（manager.c:2067-2109）：

```c
int alloc_slot(rpp)                                                      /* 2067 */
{
  for (slot_nr = 0; slot_nr < NR_SYS_PROCS; slot_nr++) {                 /* 2071 */
      *rpp = &rproc[slot_nr];
      if (!((*rpp)->r_flags & RS_IN_USE))                                /* 2073 */
	  break;
  }
  if (slot_nr >= NR_SYS_PROCS) {                                         /* 2076 */
	return ENOMEM;
  }
  return OK;
}

void free_slot(rp)                                                       /* 2088 */
{
  rpub = rp->r_pub;                                                      /* 2091 */
  late_reply(rp, OK);                                                    /* 2094：06（RS_LATEREPLY） */
  if(rpub->sys_flags & SF_USE_COPY) {                                    /* 2097 */
      free_exec(rp);                                                     /* 2098：09（exec image） */
  }
  rp->r_flags = 0;                                                       /* 2104 */
  rp->r_pid = -1;                                                        /* 2105 */
  rpub->in_use = FALSE;                                                  /* 2106 */
  rproc_ptr[_ENDPOINT_P(rpub->endpoint)] = NULL;                         /* 2107 */
}
```

**`free_slot` 的四步清理**：① 若有 `RS_LATEREPLY` 等待者，`late_reply(rp, OK)` 解除阻塞（机制 06）；② 若 `SF_USE_COPY`（有内存中的 exec 副本），`free_exec` 释放（机制 09）；③ 槽位清零（`r_flags=0`、`r_pid=-1`、`in_use=FALSE`）；④ `rproc_ptr` 索引清除。**注意 C 没有清除 `r_pub->label/endpoint` 等字段**——它们留在槽里成为"脏数据"，靠 `r_flags==0`/`in_use==FALSE` 标记空闲；Rust 的 `Default`（vacant）构造使空槽字段全部归零（§3.6）。

### 2.10 rs_isokendpt（utility.c:352-359）

```c
int rs_isokendpt(endpoint_t endpoint, int *proc)
{
	*proc = _ENDPOINT_P(endpoint);          /* 355 */
	if(*proc < -NR_TASKS || *proc >= NR_PROCS)  /* 356 */
		return EINVAL;
	return OK;
}
```

- `_ENDPOINT_P(e)`（`minix3/minix/include/minix/endpoint.h:68-69`）：`(((e)+MAX_NR_TASKS) & (_ENDPOINT_GENERATION_SIZE-1)) - MAX_NR_TASKS` —— 提取端点的**槽号**（含 generation 折叠）。
- 合法槽号范围：`[-NR_TASKS, NR_PROCS)` = `[-5, 256)`。负数槽号是内核任务（CLOCK=-3、SYSTEM=-2...），正数/零是用户服务。
- 主循环（main.c:64-66）用它对**每个收到的消息来源**做校验：非法来源直接 `panic("message from bogus source")`。`who_p`（槽号）随后用于 `rproc_ptr[who_p]` 与 `case CLOCK` 分支。

### 2.11 get_service_instances（manager.c:1334-1352）

```c
void get_service_instances(rp, rps, length)                              /* 1334 */
{
  static struct rproc *instances[5];                                     /* 1340 */
  int nr_instances;

  nr_instances = 0;
  instances[nr_instances++] = rp;                                        /* 1348 */
  if(rp->r_prev_rp) instances[nr_instances++] = rp->r_prev_rp;           /* 1348 */
  if(rp->r_next_rp) instances[nr_instances++] = rp->r_next_rp;           /* 1348 */
  if(rp->r_old_rp) instances[nr_instances++] = rp->r_old_rp;             /* 1348 */
  if(rp->r_new_rp) instances[nr_instances++] = rp->r_new_rp;             /* 1348 */

  *rps = instances;
  *length = nr_instances;
}
```

**顺序固定**：`rp` → `r_prev_rp` → `r_next_rp` → `r_old_rp` → `r_new_rp`（最多 5 个）。调用点：update.c:993（`end_srv_update`）、request.c:1077（`do_getsysinfo`）、manager.c:691（`create_service` 的 RS 备份）、manager.c:1141（`cleanup_service`）。**C 用 `static` 数组**（非重入，一次调用覆盖上一次结果）——Rust 用迭代器替代（§3.4，ARCH A-3）。

### 2.12 boot 消费点（main.c）

登记表在 boot 的三个消费点（本文档只标注调用点，机制在 01）：

| 消费点 | 行号 | 做什么 | 归属 |
|--------|------|--------|------|
| 表重置 | main.c:230-237 | `r_flags=0`、`r_init_err=ERESTART`、`r_pub` 绑定、`in_use=FALSE`、`old/new_endpoint=NONE` | 01（调用点）+ **本文档**（字段语义） |
| Step 1 建槽 | main.c:255-345 | `rp = &rproc[boot_image_priv - boot_image_priv_table]`（255）；填充 label（262）/proc_name（313）/priv（264-296）/sys（301）/dev（306）/命令（308-311）/调度（319-322）；`r_flags = RS_IN_USE\|RS_ACTIVE`（343）；`rproc_ptr[...] = rp`（344）；`in_use = TRUE`（345） | 01（编排）+ 03/05/08（字段）+ **本文档**（槽位激活原语） |
| Step 4 getnpid | main.c:422-431 | `rp = &rproc[...]`（422）；`r_pid = getnpid(endpoint)`（426），`< 0` panic（427-429） | 01（编排）+ **本文档**（`r_pid` 写入） |

**Step 1 的槽位映射是"priv 表索引"而非"扫描空闲槽"**（main.c:255）——boot 服务槽位固定为 0..11，与 `table.c` 的 priv 表顺序一致。`alloc_slot` 只用于运行期 `RS_UP`（request.c:31）。

---

## 3. Rust 设计决策

> 设计契约全文见 `.design/02-design.v1.md`（中间产物，正式文档不引用）。本节是决策摘要。ARCH 项（A-3/A-4）三处一致标注：本文档 + `.design/` + 代码注释。

### 3.1 行内合并：ServiceSlot{ pub_: PublicSlot, ... }（D3）

**C**：`r_pub` 是指向另一数组同索引元素的裸指针（`type.h:57` + `main.c:233` 建立）。**Rust**：把 `rprocpub` 作为 `ServiceSlot.pub_` 内嵌字段，指针变为字段访问 `slot.pub_.label`：

```rust
pub struct ServiceSlot {
    pub pub_: PublicSlot,          // C: r_pub（rprocpub 行内合并，type.h:57）
    pub old_rp: Option<SlotId>,    // C: r_old_rp（type.h:58，A-3）
    ...
}
```

- **优点**：消灭"指针指向哪"的全部歧义；`pub_` 与私有字段生命周期天然一致；表重置时无需 `r_pub = &rprocpub[...]` 绑定步骤。
- **兼容性**：保持 `01-rs-boot-init.md` 已使用的字段名（`endpoint`/`label`/`sys_flags`/`dev_nr`/`pid`/`in_use`）不变——这些字段放在 `pub_` 内或提供访问路径，boot 接线最小化（§4.3）。
- **验证**：boot 集成测试断言 Step 1 后槽位字段与索引一致（§5）。

### 3.2 r_flags → RFlags bitflags(u16)（D1）

**C**：`unsigned r_flags` + 16 个宏（const.h:28-43）。**Rust**：

```rust
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RFlags: u16 {
        const IN_USE        = 0x001;  // const.h:28
        const EXITING       = 0x002;  // const.h:29
        // ... 16 位全表 ...
        const REINCARNATE   = 0x8000; // const.h:43
    }
}
```

- **u16 足够**：最大位 0x8000 < 65536。
- **正交组合保持**：`IN_USE | ACTIVE | INITIALIZING` 等组合与 C 位操作语义完全一致。
- **`RS_SRV_IS_IDLE` 翻译**（const.h:45）为方法：

```rust
impl RFlags {
    pub fn is_idle(self) -> bool {
        self.contains(Self::DEAD)
            || (self & !(Self::IN_USE | Self::ACTIVE
                | Self::CLEANUP_DETACH | Self::CLEANUP_SCRIPT)).is_empty()
    }
}
```

### 3.3 sys_flags → SysFlags bitflags(u16) + 预设组合（D2）

**C**：`unsigned sys_flags` + 13 个宏（rs.h:191-206）+ 预设组合（const.h:65-68）+ `IMM_SF`（rs.h:205-206）。**Rust**：

```rust
bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SysFlags: u16 {
        const CORE_SRV    = 0x001;  // rs.h:191
        // ... 13 位全表 ...
        const NO_BIN_EXP  = 0x1000; // rs.h:203
    }
}
// 预设组合（const.h:65-68）
pub const SRV_SF: SysFlags = SysFlags::CORE_SRV;
pub const SRVR_SF: SysFlags = SysFlags::CORE_SRV.union(SysFlags::NEED_REPL);
pub const DSRV_SF: SysFlags = SysFlags::empty();
pub const VM_SF: SysFlags = SRVR_SF;
// 不可变位（rs.h:205-206）
pub const IMM_SF: SysFlags = SysFlags::NO_BIN_EXP
    .union(SysFlags::CORE_SRV)
    .union(SysFlags::SYNCH_BOOT)
    .union(SysFlags::NEED_COPY)
    .union(SysFlags::NEED_REPL);
```

### 3.4 ARCH A-3：实例链 → Option<SlotId> 索引链（D4）

**C**：`r_old_rp/r_new_rp/r_prev_rp/r_next_rp` 裸指针（type.h:58-61）+ `get_service_instances` 静态 5 槽数组（manager.c:1340）。**Rust**：

```rust
pub struct ServiceSlot {
    ...
    pub old_rp: Option<SlotId>,  // C: r_old_rp — type.h:58
    pub new_rp: Option<SlotId>,  // C: r_new_rp — type.h:59
    pub prev_rp: Option<SlotId>, // C: r_prev_rp — type.h:60
    pub next_rp: Option<SlotId>, // C: r_next_rp — type.h:61
}
```

- `SlotId` 是 `usize` newtype（槽位索引），`Option<SlotId>` 表达"无实例"。**悬垂不可能**：`SlotId` 只能来自表内有效范围（`get` 越界 panic，防御性失败而非 UB；R5：panic 带上下文断言，R8 世代落地后可检出"界内但过期"id）。
- `get_service_instances` → 迭代器（保持 C 顺序 `rp → prev → next → old → new`）：

```rust
pub struct ServiceInstances {
    // C: manager.c:1344-1348 顺序
    current: Option<SlotId>, prev: Option<SlotId>, next: Option<SlotId>,
    old: Option<SlotId>, new: Option<SlotId>,
}
impl Iterator for ServiceInstances { type Item = SlotId; ... }
```

- **三处一致标注**：本表 + `.design/02-design.v1.md` D4 + `service_slot.rs` 注释均标注 `ARCH A-3`。

### 3.5 ARCH A-4：快速索引 → by_endpoint[NR_PROCS]（D5）

**C**：`struct rproc *rproc_ptr[NR_PROCS]`（glo.h:35），以 `_ENDPOINT_P(endpoint)` 为下标。**Rust**：

```rust
pub struct RProcTable {
    slots: Vec<ServiceSlot>,                      // C: rproc[] + rprocpub[]（64 行）
    by_endpoint: [Option<SlotId>; NR_PROCS],      // C: rproc_ptr[NR_PROCS]（A-4）
}
```

- **O(1) 保持**：`by_endpoint[endpoint.slot()]`（`Endpoint::slot()` = `_ENDPOINT_P`，endpoint.rs:88-90）。
- **无悬垂**：槽释放时同步清除索引（`free_slot`，§4.2）；内核任务（负槽号）不建索引——`endpoint_slot()` 对负槽号返回 `None`（内核任务永不为服务，`rproc_ptr` 负下标在 C 中本就是未定义行为，Rust 显式排除）。
- **索引全量（R12）**：`endpoint_slot()`/`set_endpoint_index()` 对 `slot() ∉ [0, NR_PROCS)` 一律 `None`/忽略——`Endpoint::NONE/ANY/SELF` 的槽号在 `NR_PROCS` 之上（endpoint.rs:26-50），裸下标会越界 panic。C 靠主循环 `rs_isokendpt` 前置（main.c:63-66）保住安全；Rust 在索引层补齐（fail-closed），06 接线时 `classify` 前仍须跑 `isokendpt` 拒绝非法源（dispatch.rs 已标注）。
- **原始索引语义（R30，2026-09-06）**：`endpoint_slot()` 刻意**不过滤** `RS_IN_USE`——它镜像的是 C 的裸 `rproc_ptr`（glo.h:35），而重组进行中的行会合法流经它（`swap_slot` 在重写索引前要读两端的旧条目，clone 行在链接时可能还是 vacant）。C 里 IN_USE 过滤是 `caller_can_control` 扫描的**局部**语义（manager.c:52-53），Rust 同样把这一复核放在消费方（access.rs `caller_can_control`，fail-closed）；消息面消费方的槽位状态校验归主循环门（06，R12）。两种 C 构造（裸数组 vs 扫描循环）对应两个 Rust 契约，不是重复 API。测试：`test_endpoint_slot_is_raw_index_mid_restructure`。
- **三处一致标注**：本表 + `.design/02-design.v1.md` D5 + `process_table.rs` 注释均标注 `ARCH A-4`。

### 3.6 lookup/alloc/free 方法化（D6）

**C**：全局函数操作全局表，失败返回 `NULL`/`ENOMEM`。**Rust**：方法 + `Option`/`Result`：

| C | Rust | 签名变化 |
|----|------|---------|
| `lookup_slot_by_label(label)` | `RProcTable::lookup_by_label(&self, &str)` | `NULL → Option<SlotId>` |
| `lookup_slot_by_pid(pid)` | `lookup_by_pid(&self, Pid)` | `pid<0 → None`（与 C 提前返回一致） |
| `lookup_slot_by_dev_nr(dev)` | `lookup_by_dev_nr(&self, u32)` | `dev<=0 → None` |
| `lookup_slot_by_domain(dom)` | `lookup_by_domain(&self, i32)` | `dom<=0 → None` |
| `lookup_slot_by_flags(fl)` | `lookup_by_flags(&self, RFlags)` | `flags 空 → None` |
| `alloc_slot(rpp)` | `alloc_slot(&mut self) -> Result<SlotId, i32>` | 出参 → 返回值；`ENOMEM` 保留 |
| `free_slot(rp)` | `free_slot(&mut self, SlotId)` | 指针 → 索引 |

**`free_slot` 的依赖标注**（D12 + R18）：C 的 `late_reply`（manager.c:2097，机制 06）本表不持有 reply 通道，调用方须保证无 pending `RS_LATEREPLY`；`free_exec`（manager.c:2100-2102，机制 09）**已在表原语内落地**——`SF_USE_COPY` 行释放时调用 `crate::exec::free_exec(table, id)` 丢弃其 exec `Arc`（先取 flags 结束借用，再 free，避免借用冲突）。非 `USE_COPY` 行的 exec 由 09 的 execve 路径释放（manager.c:643-644），不属 `free_slot`。

**vacant 构造**：`ServiceSlot::vacant()` 等价于 C 的"槽清零"（表重置 main.c:230-237 的 Rust 形态）。与 C 的"依赖 `r_flags==0` 隐式空闲"不同，Rust 空槽所有字段归零（`Default`），杜绝脏数据读取。**一处有意的差异**：C 表重置把 `r_init_err` 设为 `ERESTART`（main.c:232），Rust `vacant()` 归零——因为该默认值在 `init_slot` 时重新建立（08，manager.c:1791），空槽的 `init_err` 无观察者。

### 3.7 rs_isokendpt → Result<i32, Errno>（D7）

**C**：出参 `*proc` + 返回 `OK`/`EINVAL`（utility.c:352-359）。**Rust**：

```rust
pub fn isokendpt(endpoint: Endpoint) -> Result<i32, Errno> {
    let slot = endpoint.slot();                          // C: _ENDPOINT_P — endpoint.h:68-69
    if slot < -(minix_types::NR_TASKS as i32) || slot >= minix_types::NR_PROCS as i32 {
        return Err(Errno::EINVAL);                       // utility.c:355-356
    }
    Ok(slot)
}
```

出参消失，槽号直接作为 `Ok` 负载。主循环（06）拿到 `Ok(slot)` 后用于 `by_endpoint` 查找或 `CLOCK` 比较。

### 3.8 身份字段类型化：Option<Pid> 与 Label（D8/D9/D10）

- **`r_pid → Option<Pid>`**（D8）：C 的 `-1 = 无进程`（type.h:63）→ `None`。`lookup_by_pid(None)` 等价 C 的 `pid<0` 提前返回。
- **`label`/`proc_name` → `Label([u8; 16])`**（D9）：C 的 `char[RS_MAX_LABEL_LEN]` 定长 + NUL 终止（`strcmp`/`strlcpy`）。Rust newtype 提供：
  - `from_bytes(&[u8]) -> Label`：**`strlcpy` 语义**——截断到 `RS_MAX_LABEL_LEN - 1` 字节 + 强制尾 NUL（N6 修复：旧版满 16 字节无 NUL，比较/回显与 C 的 15 字节串不一致）；
  - `as_str() -> Option<&str>`：NUL 终止 + UTF-8 校验后的视图（非法字节 → `None`，fail-closed）；
  - `PartialEq`/`PartialEq<&str>`：**统一 strcmp 语义**（N6 修复：派生 16 字节整体比较在"嵌入 NUL + 填充字节"时与 C 分歧；新实现遇 NUL 截断比较，字节级、不要求 UTF-8）；`Hash` 同步只哈希到首个 NUL。`lookup_by_label` 改收 `&Label`（旧 `&str` 无法表达非 UTF-8 label，迫使调用方 `unwrap_or("")` fail-open）。
  - 用定长数组而非 `String`：**内存布局与 C 一致**（公开表经 grant 共享时有二进制兼容需求），且无堆分配。
- **`cmd`/`args`/`script`/`ipc_list` → 定长数组**（D10）：`[u8; MAX_COMMAND_LEN]` 等，C 布局保持；填充机制归 08/05。
- **`r_argv` 不进 struct**（D11）：`r_argv` 是指向 `r_args` **内部**的指针数组（type.h:81），移动/复制即悬垂。Rust 由 08 的**参数解析器**（按需从 `args` 迭代）替代——与 A-3 同类的"裸指针 → 安全抽象"演进，在文档与代码中显式标注。

### 3.9 全局描述符：RinitState / rupdate 链（D11/D13）

- **`rinit` → `RinitState`**（boot.rs 既有，字段 `rproctab_gid: Option<u32>`）：完整 `sef_init_info_t` 字段表在 12 展开；本 doc 登记"创建点在 boot（main.c:185），消费在 12"。
- **`rupdate` → `UpdateChain`（live_update.rs，16）+ `RupdateFlags`**：`UpdateChain` 是唯一的 rupdate 模型——`entries: Vec<UpdateEntry>`（每个 entry 带 `slot: SlotId`）+ `first/curr/last/vm/rs` 链索引，`len()` 对应 `num_rpupds`（type.h:45）；`RupdateFlags`（type.h:44，`RS_UPDATING`/`RS_INITIALIZING`）由 16 的状态机写入。**N8 修复（2026-08-16，todo §11）**：删除 process_table.rs 曾有的 `RupdateDescriptor` 死代码（全 crate 无生产使用者）——它和 `UpdateChain` 双建模同一 C `struct rupdate`（type.h:43-54），双源真相会在 16 落地时漂移；`UpdateChain` 内部 usize 索引是**链内位置**（entries Vec 下标），不是 `SlotId`，16 落地时如需可再加 `ChainIdx` newtype 消除误传（P2 后续）。

### 3.10 字段归属汇总

| 分组 | Rust 表示 | 机制归属 |
|------|-----------|---------|
| 身份（label/proc_name/endpoint/dev_nr/domain/in_use） | `PublicSlot` 全字段 | 02（查找）+ 08（写入） |
| 实例链（old/new/prev/next） | `Option<SlotId>` × 4 | 02（形状）+ 10/16（使用） |
| 生命周期（flags/pid/init_err/asr_count/restarts/backoff） | `RFlags` + 标量 | 02（形状）+ 12/15（写入） |
| 监控（period/check/alive/stop_tm） | `Clock` 标量 | 07 |
| 延迟回复（caller/caller_request） | 标量 | 06 |
| 命令/参数/脚本（cmd/args/argc/script） | 定长数组 | 08 |
| exec（exec/exec_len） | `Option<Box<[u8]>>` | 09（A-5 决定 Arc/Box） |
| 权限（priv/uid/scheduler/priority/quantum/cpu/io/irq） | `Priv` 占位 + 标量 | 03 |
| 更新（upd/map_prealloc） | 占位 + 标量 | 16 |
| IPC/control（ipc_list/nr_control/control） | 定长数组 | 05/08 |

---

## 4. 实现详解

> 本节以 `os/servers/rs/src/service_slot.rs`、`os/servers/rs/src/process_table.rs` 为准（行号随代码演化可能漂移，以语义为准）。

### 4.1 service_slot.rs：常量、标志、标签、槽类型

模块结构：

```
service_slot.rs
  ├─ 常量表        RS_MAX_LABEL_LEN / MAX_COMMAND_LEN / MAX_SCRIPT_LEN / MAX_NR_ARGS /
  │                MAX_IPC_LIST / RS_NR_CONTROL / NR_IO_RANGE / NR_IRQ / NR_DOMAIN
  ├─ RFlags        bitflags(u16) 16 位 + is_idle()          （const.h:28-45）
  ├─ SysFlags      bitflags(u16) 13 位 + SRV_SF/SRVR_SF/DSRV_SF/VM_SF/IMM_SF（rs.h:191-206）
  ├─ SlotId        usize newtype（0..NR_SYS_PROCS）
  ├─ Label         [u8;16] newtype（from_bytes/as_str/PartialEq<str>）
  ├─ IoRange       { base: u32, len: u32 }                  （minix3/minix/include/minix/type.h:133-137）
  ├─ PublicSlot    rprocpub 全字段                          （rs.h:165-183）
  └─ ServiceSlot   rproc 全字段（含 pub_ 行内合并）          （type.h:56-108）
```

**关键不变量**（`ServiceSlot`）：

1. `pub_.in_use == true ⟺ flags.contains(IN_USE)`（双表达同步，由 `activate`/`free_slot` 维护）。
2. `pub_.endpoint` 非 `NONE` 时，`pid` 可能为 `None`（Step 1 后、Step 4 前）。
3. `vacant()` 构造的槽：`flags` 空、`pub_.in_use == false`、`pub_.endpoint == Endpoint::NONE`、`pid == None`、四链全 `None`、定长数组全零。
4. `Label` 恒为 16 字节；`as_str()` 只返回 NUL 终止且 UTF-8 合法的视图。

### 4.2 process_table.rs：RProcTable 槽位管理

```rust
pub struct RProcTable {
    slots: Vec<ServiceSlot>,                 // C: rproc[]/rprocpub[]（64 行，glo.h:33-34）
    by_endpoint: [Option<SlotId>; NR_PROCS], // C: rproc_ptr[NR_PROCS]（glo.h:35，ARCH A-4）
}
```

| 方法 | C 对照 | 语义 |
|------|--------|------|
| `new()` | 表重置 main.c:230-237 | 64 个 vacant 槽 + 全 `None` 索引 |
| `get(id)`/`get_mut(id)` | `&rproc[slot_nr]` | 索引访问；越界 panic 带上下文断言（R5，防御性，`SlotId` 由本表产生） |
| `lookup_by_label(&Label)` | manager.c:1935 | 仅 `ACTIVE`；strcmp 语义 `Label` 比较（N6） |
| `lookup_by_pid(Pid)` | manager.c:1959 | `pid<0 → None`；`IN_USE` |
| `lookup_by_dev_nr(u32)` | manager.c:1985 | `dev==0 → None`（C 是 `<=0`，dev_t 无符号化后 0 即无效） |
| `lookup_by_domain(i32)` | manager.c:2013 | `dom<=0 → None`；遍历 `domain[..nr_domain]` |
| `lookup_by_flags(RFlags)` | manager.c:2041 | 空 flags → None；任一位置位 |
| `alloc_slot()` | manager.c:2067 | 首个 `!IN_USE`；满表 `Err(ENOMEM)` |
| `free_slot(id)` | manager.c:2088 | 表级清理 + 索引清除；`SF_USE_COPY` → `free_exec`（R18）；`late_reply`(06) 调用方前置 |
| `activate_boot_slot(id, ep, proc_name, ...)` | main.c:255-345 | 指定槽激活（priv 表索引映射），填充 label/proc_name/sys/dev/endpoint + `IN_USE\|ACTIVE` + 索引 |
| `endpoint_slot(ep)` | `rproc_ptr[_ENDPOINT_P(ep)]` | O(1) 反查；负槽号/越界（NONE/ANY/SELF）/未登记 → None（R12） |
| `isokendpt(ep)` | utility.c:352 | 边界校验 → `Ok(slot)`/`Err(EINVAL)` |
| `instances_of(id)` | manager.c:1332 | `ServiceInstances` 迭代器（rp/prev/next/old/new） |

**`free_slot` 的 Rust 实现**（manager.c:2088-2109 对照）：

```rust
pub fn free_slot(&mut self, id: SlotId) {
    // C: late_reply(rp, OK) — manager.c:2097（机制 06-rs-main-loop.md，RS_LATEREPLY）
    //   本表不持有 reply 通道；调用方须保证无 pending RS_LATEREPLY。
    // C: if(sys_flags & SF_USE_COPY) free_exec(rp) — manager.c:2100-2102（R18）
    let use_copy = self.slots[id.0].pub_.sys_flags.contains(SysFlags::USE_COPY);
    if use_copy { crate::exec::free_exec(self, id); }   // 先取 flags，再 free（借用顺序）
    let endpoint = self.slots[id.0].pub_.endpoint;
    let slot = &mut self.slots[id.0];
    slot.pub_.in_use = false;          // manager.c:2107
    slot.pub_.endpoint = Endpoint::NONE;  // 比 C 更彻底（C 留脏数据，靠 flags 标记）
    slot.flags = RFlags::empty();      // manager.c:2105
    slot.pid = None;                   // manager.c:2106（-1 → None）
    // C: rproc_ptr[_ENDPOINT_P(rpub->endpoint)] = NULL — manager.c:2108（A-4）
    if !endpoint.is_none() && endpoint.slot() >= 0 {
        self.by_endpoint[endpoint.slot() as usize] = None;
    }
}
```

**`activate_boot_slot`**（main.c:255-345 的表级部分）：

```rust
pub fn activate_boot_slot(
    &mut self, id: SlotId, endpoint: Endpoint, proc_name: Label,
    priv_: &BootImagePriv, sys: &BootImageSys, dev: &BootImageDev,
    privilege: Privilege, ticks: Clock,
) -> Result<(), Errno> {
    let slot = self.slots.get_mut(id.0).ok_or(ENOSYS)?;   // 越界 → fail-closed
    if slot.flags.contains(RFlags::IN_USE) { return Err(ENOSYS); }  // 重复激活防御
    slot.pub_.label = Label::from_bytes(priv_.label.as_bytes());  // main.c:262
    slot.pub_.proc_name = proc_name;                // main.c:313
    slot.pub_.sys_flags = sys.flags;    // main.c:301（N10：BootImageSys.flags 已是 SysFlags）
    slot.pub_.dev_nr = dev.dev_nr;                  // main.c:306
    slot.pub_.endpoint = endpoint;                  // main.c:325
    slot.pub_.in_use = true;                        // main.c:345
    slot.flags = RFlags::IN_USE | RFlags::ACTIVE;   // main.c:343
    // S2 补全（2026-08-15，main.c:308-333）：
    slot.cmd[..16].copy_from_slice(proc_name.as_bytes());  // strlcpy(r_cmd) — 308
    rebuild_args(&mut slot);                          // build_cmd_dep — 310
    slot.pub_.vm_call_mask = CallMask::all();         // SRV_VC=ALL_C — 317-319
    slot.scheduler/priority/quantum/cpu = boot_defaults(endpoint);  // 320-322
    slot.alive_tm = ticks;                            // getticks() — 333
    self.by_endpoint[endpoint.slot() as usize] = Some(id);  // main.c:344（A-4）
    Ok(())
}
```

> **S2（2026-08-15）**：此前 `cmd/script/argc/vm_call_mask/scheduler/priority/
> quantum/alive_tm` 全部缺失——`alive_tm=0` 使心跳超时判定从启动起偏差（07），
> `scheduler=NONE` 会在 `sched_init_proc` 的 debug_assert 下崩溃。现按 main.c:308-333
> 逐字段补全；`getticks()` 经 `KernelApi::get_ticks`（新增，boot Step 1 注入）。

### 4.3 boot.rs 接线

`BootInit` 的槽容器从 `slots: Vec<ServiceSlot>` 换成 `table: RProcTable`：

| 原 boot.rs | 新 boot.rs | 语义 |
|-----------|-----------|------|
| `slots: Vec<ServiceSlot>` | `table: RProcTable` | 01 §3.5 的"表重置"→ `RProcTable::new()` |
| `ServiceSlot::activate(endpoint, priv_, sys_, dev)` push | `table.activate_boot_slot(SlotId::new(i), ...)` | 槽位 = priv 表索引（main.c:255 映射保持） |
| `for slot in &mut self.slots { slot.pid = Some(pid) }` | `for i in 0.. { table.get_mut(...).pid = ... }` | Step 4 `r_pid` 写入（main.c:426） |

boot 的测试保持通过（MockKernelApi 不变），新增断言：Step 1 后 12 个槽 `IN_USE|ACTIVE`、`by_endpoint` 命中、非 boot 槽空闲。

### 4.4 lib.rs

新增 `pub mod service_slot;` 与 `pub mod process_table;`，导出 `RProcTable`/`ServiceSlot`/`PublicSlot`/`RFlags`/`SysFlags`/`SlotId`/`Label` 等。

---

## 5. 测试要点

> 本节列出 `service_slot.rs`/`process_table.rs` 的测试函数（`rg "fn test_"` 可验证）。截至 2026-08-16：`cargo test -p minix-rs --lib` 全部通过（208 passed / 0 failed）。

### 5.1 标志位对齐（RFlags/SysFlags）

- `test_rflags_bits_match_const_h`：16 个位值与 `const.h:28-43` 数值断言（`IN_USE==0x001` ... `REINCARNATE==0x8000`）。
- `test_sysflags_bits_match_rs_h`：13 个位值与 `rs.h:191-203` 数值断言。
- `test_preset_combinations_match_c`：`SRV_SF/SRVR_SF/DSRV_SF/VM_SF/IMM_SF` 与 `const.h:65-68`/`rs.h:205-206` 组合断言。
- `test_is_idle_combinations`：`RS_SRV_IS_IDLE`（const.h:45）真值表——`DEAD` 置位 → true；`IN_USE|ACTIVE` 无其他位 → true；`IN_USE|ACTIVE|EXITING` → false；空 flags → true。

### 5.2 Label

- `test_label_from_bytes_truncates`：>16 字节截断（`strlcpy` 语义）。
- `test_label_as_str_nul_terminated`：NUL 终止视图；非法 UTF-8 → None。
- `test_label_eq_str`：`PartialEq<&str>` 比较（等价 `strcmp`）。
- `test_label_eq_strcmp_semantics`（N6）：嵌入 NUL + 填充字节 == 纯前缀；非 UTF-8 label 自等。
- `test_vacant_slot_is_clean`：`ServiceSlot::vacant()` 构造的槽全字段归零（flags 空 / in_use false / endpoint NONE / pid None / 四链 None / cmd·control 全零）——对应 §3.6 vacant 语义。
- `test_slot_mutations_apply`（R13）：`SlotMutations` 决策载荷一次过应用 set/clear/字段更新（06/12/15 调用方提交的恰是决策语义）。

### 5.3 槽位管理原语

- `test_lookup_by_label_requires_active`：仅 `ACTIVE` 命中（`IN_USE` 但非 `ACTIVE` 的槽不命中）——manager.c:1942。
- `test_lookup_by_pid_negative`：`pid < 0` → None——manager.c:1965-1967。
- `test_lookup_by_dev_nr_zero`：`dev_nr == 0` → None——manager.c:1992-1993（u32 化后 0 即无效）。
- `test_lookup_by_domain`：多域遍历命中 + `domain <= 0` → None。
- `test_lookup_by_domain_corrupt_count_fails_closed`（D3）：`nr_domain` 超 `NR_DOMAIN` → `None`（不 panic，fail-closed）。
- `test_lookup_by_flags_any_bit`：任一位置位命中 + 空 flags → None。
- `test_alloc_slot_roundtrip`：alloc → activate → free → alloc 复用同一槽。
- `test_alloc_slot_full_returns_enomem`：64 槽全占用 → `Err(ENOMEM)`（manager.c:2076-2079）。
- `test_free_slot_clears_table_state`：free 后 flags 空/pid None/in_use false/索引 None（manager.c:2105-2108）。
- `test_free_slot_releases_use_copy_exec`（R18）：`SF_USE_COPY` 行 free 后 exec `Arc` 被释放（manager.c:2100-2102）；非 `USE_COPY` 行保留（execve 路径 09 释放）。
- `test_activate_boot_slot_indexes_endpoint`：激活后 `endpoint_slot(ep) == Some(id)`（A-4，main.c:344）。
- `test_activate_boot_slot_rejects_reuse`：重复激活 → Err（防御性）。
- `test_activate_boot_slot_rejects_out_of_range`：槽号越界 → Err（fail-closed）。
- `test_isokendpt_bounds`：`-NR_TASKS` 到 `NR_PROCS-1` 通过，越界 `EINVAL`（utility.c:356）。
- `test_endpoint_slot_ignores_kernel_tasks`：负槽号（内核任务）不建索引（A-4）。
- `test_endpoint_slot_none_fails_closed`（R12）：NONE/ANY/SELF 越界 → `None`，写侧忽略（不 panic）。
- `test_get_rejects_out_of_range_id`：`get(SlotId::new(len+1))` → `#[should_panic]`（R17：越界读取是程序错误，不静默返回）。
- `test_instances_of_order`：C 顺序 rp/prev/next/old/new（manager.c:1344-1348）。
- `test_instances_of_unlinked`：无链槽只产出自身。
- ~~`test_rupdate_descriptor_new`~~（N8 已删）：`RupdateDescriptor` 移除后，`RUPDATE_INIT()` 语义由
  `UpdateChain::new()`（live_update.rs）承担。

### 5.4 boot 集成

- `test_init_fresh_populates_table`：Step 1 后 12 个 boot 槽 `IN_USE|ACTIVE`，`by_endpoint` 对每个 boot 服务命中。
- `test_step4_sets_pid`：Step 4 后每个 boot 槽 `pid == Some(getnpid)`（main.c:426）。

### 5.5 测试统计（截至 2026-08-16）

- **02 范围 31 项测试全部通过**（service_slot 10 + process_table 19 + boot 集成 2；`cargo test -p minix-rs --lib` 定向过滤实测 0 失败，2026-08-16）。
- 全局 `cargo test -p minix-rs` 通过数随 03/04/05/07 并行模块实现而变化，非本文档承诺范围（快照值以 STATE.md 注记为准）。
- 完整清单：`rg "^\s*fn test_" os/servers/rs/src/service_slot.rs os/servers/rs/src/process_table.rs os/servers/rs/src/boot.rs`。

---

## 6. 过渡

02 是 boot Step 1 的**数据底座**：01 的四步 boot 在 Step 1 调用 `activate_boot_slot` 建立槽位（main.c:255-345），Step 4 写入 `r_pid`（main.c:426）。之后：

```
01 boot 骨架 ──► 02 进程表（本文档：登记表 + 槽位原语）
                   │
                   ├─► 03-rs-privilege：r_priv 结构 + sys_privctl（Step 1 的权限机制）
                   ├─► 05-rs-ipc-sendmask：r_ipc_list 填充（Step 1 的 IPC 掩码机制）
                   ├─► 06-rs-main-loop：主循环用 rs_isokendpt + rproc_ptr 定位消息来源
                   ├─► 07-rs-period-heartbeat：r_period/r_alive_tm/... 读写
                   ├─► 08-rs-slot-config：init_slot/edit_slot 填充 label/cmd/args/control
                   ├─► 09-rs-exec：r_exec 加载与共享（A-5）
                   ├─► 12-rs-init-run：rinit 消费 + RS_INIT 协议
                   ├─► 15-rs-terminate-restart：terminate/cleanup 使用 free_slot/RS_DEAD
                   └─► 16-rs-live-update：r_upd/rupdate 状态机 + 实例链使用
```

下一篇 `03-rs-privilege.md`：`r_priv`（`ixfer_priv_s`）的完整结构、boot Step 1 的 priv 初始化、`sys_privctl` 操作面——登记表"有什么字段"之后，回答"权限机制怎么写进去"。

---

## 7. 参见

- `../03-stage-rs/01-rs-boot-init.md` — boot 四步编排；Step 1 建槽/Step 4 pid 的调用点（§2.12）
- `../03-stage-rs/03-rs-privilege.md` — `r_priv` 结构与 privctl（后续）
- `../03-stage-rs/05-rs-ipc-sendmask.md` — `r_ipc_list`/`add_forward_ipc`/`add_backward_ipc`（后续）
- `../03-stage-rs/06-rs-main-loop.md` — `rs_isokendpt` 调用上下文、`reply`/`late_reply`/`EDONTREPLY`（后续）
- `../03-stage-rs/07-rs-period-heartbeat.md` — 心跳字段读写（后续）
- `../03-stage-rs/08-rs-slot-config.md` — `init_slot`/`edit_slot`/`inherit_service_defaults`（后续）
- `../03-stage-rs/09-rs-exec.md` — `r_exec`/`free_exec`（后续）
- `../03-stage-rs/10-rs-service-create.md` — `clone_service`/`activate_service` 使用实例链（后续）
- `../03-stage-rs/12-rs-init-run.md` — `rinit`/`end_srv_init`（后续）
- `../03-stage-rs/15-rs-terminate-restart.md` — `free_slot` 的 terminate/cleanup 调用点（后续）
- `../03-stage-rs/16-rs-live-update.md` — `r_upd`/`rupdate` 状态机（后续）
- `../03-stage-rs/99-rs-global-concepts.md` — 常量全表（后续）
- `../01-stage-kernel/22-privilege.md` — 内核侧 priv 语义（`io_range` 定义）
- `../02-stage-vm/25-rs-services.md` — VM 侧 RS 服务（`vm_call_mask` 相关）
- `minix3/minix/servers/rs/type.h`、`glo.h`、`const.h`、`manager.c:1935-2109`、`utility.c:352-359` — C ground truth
- `minix3/minix/include/minix/rs.h`、`sef.h`、`param.h`、`endpoint.h` — 协议定义
