# 08-rs-slot-config: 服务 slot 配置

> **分类**: 阶段 4 — 服务创建与配置（配置落地）
> **源码**: `minix3/minix/servers/rs/request.c:1265-1308`（`check_request`）、`minix3/minix/servers/rs/manager.c:135-169`（`copy_rs_start`/`copy_label`）、`manager.c:1460-1703`（`edit_slot`）、`manager.c:1708-1795`（`init_slot`）、`manager.c:289-323`（`build_cmd_dep`）、`manager.c:1303-1329`（`inherit_service_defaults`）、`minix3/minix/include/minix/rs.h:24-52,104-151`（`RSS_*` 与 `struct rs_start`）
> **Rust 模块**: `os/servers/rs/src/slot.rs`（`RsStart`/`RssFlags`/`check_request`/`build_cmd_dep`）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md`（槽位字段）、`03-rs-privilege.md`（`r_priv` 字段）、`05-rs-ipc-sendmask.md`（`init_privs` 调用点）、`07-rs-period-heartbeat.md`（`r_period` 消费）
> **说明**: 服务配置从"用户请求的参数"（`rs_start`）到"RS 表内的槽位状态"的落地。`RS_UP`/`RS_EDIT` 都走同一套配置管线：`check_request`（参数校验）→ `copy_rs_start`（拷入）→ `init_slot`/`edit_slot`（字段落地）。本文档是服务生命周期**创建路径的第一站**——槽位配置正确，后续 exec（09）/创建（10）/发布（11）才有输入。

---

## 1. 概念：从声明到槽位

### 1.0 章节引言

服务不是凭空出现的：启动一个服务（`RS_UP`，13）需要描述"这个服务是什么"——命令、IPC 列表、调度参数、信号管理器、replica 策略……这份描述叫 **`rs_start`**（rs.h:104-151），由请求方（通常是 INIT 脚本或 `service` 命令）构造。本文档回答的问题是：**`rs_start` 里的每个字段怎么被校验、怎么拷入 RS 表、怎么变成槽位的可执行状态**。

> **本章不讲什么**（机制一律移交）:
> - priv 结构构造与 privctl 提交（`03-rs-privilege.md`）
> - IPC 掩码计算（`05-rs-ipc-sendmask.md`）——本文档只陈述 `init_privs` 调用点（manager.c:1700）
> - exec 二进制加载（`09-rs-exec.md`）——本文档只陈述 `read_exec`/`share_exec` 调用点（`RSS_COPY` 分支）
> - 服务进程创建/发布（`10/11`）
> - 脚本执行（`15 run_script`）
>
> 本章只回答一个问题：**配置管线的每一步（校验→拷贝→落地）的 C 行号、字段与规则**。

### 1.1 为什么需要单独配置层（WHY）

`rs_start` 是**请求方的声明**，槽位是 **RS 的可变状态**。两者之间需要三层转换：

```
请求方（INIT/service 命令）
  │  rs_start（用户空间指针，rs.h:104）
  ▼
① check_request（request.c:1265）     —— 参数合法性（调度/CPU/信号管理器）
  ▼
② copy_rs_start/copy_label（manager.c:135-169）—— sys_datacopy 拷入 RS 地址空间
  ▼
③ init_slot / edit_slot（manager.c:1708/1460）—— 字段落地 + 派生（掩码/argv/exec copy）
  ▼
槽位（rproc/rprocpub）就绪，交给 create_service（10）
```

为什么要分三层？① 防**坏参数**（越界的调度优先级/CPU 号会导致内核 panic）；② 防**野指针**（`rs_start` 在请求方地址空间，必须经 `sys_datacopy` 拷贝，不能直接解引用）；③ 把**声明**翻译成**状态**（默认值展开、标志位继承、派生字段）。

### 1.2 关键区分：`init_slot` vs `edit_slot`（WHAT）

| | `init_slot`（manager.c:1708） | `edit_slot`（manager.c:1460） |
|--|------------------------------|------------------------------|
| 场景 | 新服务首次创建（RS_UP） | 编辑已有服务（RS_EDIT）；init_slot 尾部也调它 |
| 默认 | `DSRV_SF`/`DSRV_F`/`DSRV_I`/`DSRV_T` + uid/dev/domain/pci 初始化 | 字段覆盖 |
| 关系 | 初始化后 **`return edit_slot(...)`**（manager.c:1794） | init_slot 的收尾 |

所以**真正的字段落地都在 `edit_slot`**——本文档以它为主体，`init_slot` 只负责"新服务特有"的默认值。

---

## 2. C 源码分析

### 2.1 `check_request`（request.c:1265-1308）——参数校验

```c
static int check_request(struct rs_start *rs_start)    /* request.c:1265 */
{
  /* 调度器必须是 KERNEL 或合法特殊进程（0..LAST_SPECIAL_PROC_NR=11） */
  if (rs_start->rss_scheduler != KERNEL &&
	(rs_start->rss_scheduler < 0 ||
	rs_start->rss_scheduler > LAST_SPECIAL_PROC_NR)) {   /* 1268-1274 */
	return EINVAL;
  }
  if (rs_start->rss_priority >= NR_SCHED_QUEUES) {       /* 1275-1279 */
	return EINVAL;
  }
  if (rs_start->rss_quantum <= 0) {                      /* 1280-1284 */
	return EINVAL;
  }
  /* CPU 解析（1286-1296）：
   *   RS_CPU_BSP(-2) → machine.bsp_id
   *   RS_CPU_DEFAULT(-1) → 保持
   *   < 0 → EINVAL
   *   > processors_count → 告警 + BSP
   */
  if (rs_start->rss_cpu == RS_CPU_BSP)
	  rs_start->rss_cpu = machine.bsp_id;
  else if (rs_start->rss_cpu == RS_CPU_DEFAULT) { }
  else if (rs_start->rss_cpu < 0) return EINVAL;
  else if (rs_start->rss_cpu > machine.processors_count) { /* 1292 */
	  rs_start->rss_cpu = machine.bsp_id;
  }
  /* 信号管理器必须是 SELF 或合法特殊进程 */
  if (rs_start->rss_sigmgr != SELF &&
	(rs_start->rss_sigmgr < 0 ||
	rs_start->rss_sigmgr > LAST_SPECIAL_PROC_NR)) {       /* 1299-1305 */
	return EINVAL;
  }
  return OK;
}
```

要点：

- **调度器**：`KERNEL`（内核调度）或 0~11（`LAST_SPECIAL_PROC_NR`，com.h:70——即 PM/SCHED/VFS 等特殊服务号）。调度器是用户调度器（如 SCHED）时，`sched_init_proc` 走 `SCHEDULING_START` 消息（03 §1.5）。
- **优先级**：`< NR_SCHED_QUEUES`（config.h:66，=16）。
- **量子**：`> 0`。
- **CPU**：`RS_CPU_BSP(-2)`/`RS_CPU_DEFAULT(-1)`（rs.h:63-64）两个特殊值；越界（> processors_count）不报错而是**回退 BSP**（带告警）；负的其他值 → EINVAL。
- **信号管理器**：`SELF` 或 0~11。
- `check_request` 是静态函数，被 `do_up` 调用（request.c:43）——RS_UP 专属校验；RS_EDIT 不复用（编辑不改调度器为无效值）。

Rust 侧 `check_request(rs_start, machine: &Machine)`（slot.rs，R32.1）：C 读全局 `machine`（request.c:1289/:1296），Rust 传 `Machine` 快照——字段保持活跃且参数不可换位（01/19 的 `sys_getmachine` 结果），返回解析后的 CPU 值。测试覆盖四态 CPU 与全部越界分支。**R13（2026-08-16）**：C 就地改写 `rs_start->rss_cpu`（request.c:1286-1296），Rust 返回解析值——12 接线**必须消费该返回值**写回槽位 `cpu`（丢弃则 `RS_CPU_BSP` 残留在槽里，调度参数错误）。

> **D5 修复（2026-08-15）**：`RsStart::default` 对齐 C 调用方注入的默认值
> （minix-service parse.c:1164-1169：`sigmgr=RS`/`scheduler=SCHED`/
> `priority=USER_Q=7`/`quantum=USER_QUANTUM=200`/`cpu=-1`）。此前 `SELF`/`KERNEL`/
> `quantum=1` 是"未设置"哨兵而非 C 默认，任何"先 Default 再局部填充"的路径都会得到与 C
> 差 200 倍的调度配置且 `check_request` 无法识别（SELF/KERNEL 均合法）。

### 2.2 `copy_rs_start`/`copy_label`（manager.c:135-169）——sys_datacopy 拷贝

```c
int copy_rs_start(src_e, src_rs_start, dst_rs_start)   /* manager.c:135 */
{
  r = sys_datacopy(src_e, (vir_bytes) src_rs_start,
  	SELF, (vir_bytes) dst_rs_start, sizeof(struct rs_start)); /* 143-146 */
  return r;
}

int copy_label(src_e, src_label, src_len, dst_label, dst_len) /* manager.c:151 */
{
  len = MIN(dst_len-1, src_len);                       /* manager.c:160 */
  s = sys_datacopy(src_e, (vir_bytes) src_label,
	SELF, (vir_bytes) dst_label, len);               /* 162-165 */
  if (s != OK) return s;
  dst_label[len] = 0;                                  /* manager.c:166 */
  return OK;
}
```

- `copy_rs_start`：整结构拷入（`sizeof(struct rs_start)`），错误透传。
- `copy_label`：**`MIN(dst_len-1, src_len)` 截断 + 强制 NUL**（manager.c:160,166）——保证目标缓冲永远有终止符。Rust 的 `Label::from_bytes`（`strlcpy` 语义，service_slot.rs）等价。
- 这两个函数是 `sys_datacopy`（19 接线）的薄封装——Rust 侧 DEFERRED，编辑管线最终经 minix-sys 接线。

### 2.3 `init_slot`（manager.c:1708-1795）——新服务默认值

```c
int init_slot(rp, rs_start, source)                    /* manager.c:1708 */
{
  rpub->sys_flags = DSRV_SF;            /* 动态服务系统标志 */      /* 1723 */
  rp->r_priv.s_flags = DSRV_F;          /* DYN_PRIV_ID|SYS_PROC|PREEMPTIBLE */ /* 1724 */
  rp->r_priv.s_init_flags = DSRV_I;     /* 0 */                    /* 1725 */
  rp->r_priv.s_trap_mask = DSRV_T;      /* ~0 */                   /* 1726 */
  rp->r_priv.s_bak_sig_mgr = NONE;      /* 无备份信号管理器 */        /* 1727 */
  rp->r_uid = rs_start->rss_uid;        /* uid */                  /* 1730 */
  rpub->dev_nr = rs_start->rss_major;   /* 主设备号 */               /* 1738 */
  rpub->nr_domain = rs_start->rss_nr_domain;                       /* 1739 */
  for (i = 0; i < rs_start->rss_nr_domain; i++)                    /* 1740 */
	rpub->domain[i] = rs_start->rss_domain[i];
  rpub->devman_id = rs_start->devman_id;                           /* 1742 */
  /* PCI ACL（1744-1775）——A-10 defer */
  rp->r_asr_count = 0;                 /* 无 ASR 更新 */            /* 1777 */
  rp->r_restarts = 0;                  /* 无重启 */                /* 1778 */
  rp->r_old_rp = rp->r_new_rp = rp->r_prev_rp = rp->r_next_rp = NULL; /* 1779-1782 */
  rp->r_exec = NULL; rp->r_exec_len = 0;                           /* 1783-1784 */
  rp->r_script[0] = '\0';              /* 无恢复脚本 */              /* 1785 */
  rpub->label[0] = '\0';               /* 无 label */              /* 1786 */
  rp->r_scheduler = -1;                /* 无调度器 */               /* 1787 */
  rp->r_priv.s_sig_mgr = -1;           /* 无信号管理器 */            /* 1788 */
  rp->r_map_prealloc_addr = rp->r_map_prealloc_len = 0;            /* 1789-1790 */
  rp->r_init_err = ERESTART;           /* 默认初始化错误 = 重启 */    /* 1791 */
  return edit_slot(rp, rs_start, source);                          /* 1794 */
}
```

要点：

- **动态服务默认**：`DSRV_F = SRV_F | DYN_PRIV_ID`（priv.h:46）——动态 priv id（内核分配）；`DSRV_SF`（const.h:67）/`DSRV_T`（priv.h:62）。所有动态服务同一起点，差异全部由 `edit_slot` 覆盖。
- **四链置空**（manager.c:1779-1782）：replica/update 链初始无链接（A-3）。
- **`r_init_err = ERESTART`**（manager.c:1791）：初始化失败默认按"重启"处理（15 用）。
- 尾部 `return edit_slot(...)`（manager.c:1794）——**init_slot 是 edit_slot 的前置**，字段落地统一。

### 2.4 `edit_slot` 字段覆盖表（manager.c:1460-1703）

`edit_slot` 是配置管线的主体。按字段分组（行号均 grep 实证）：

| 组 | 字段 | C 行号 | 规则 |
|----|------|--------|------|
| IPC 列表 | `r_ipc_list` | 1475-1483 | 空或超 `MAX_IPC_LIST` → EINVAL；`sys_datacopy` + 补 NUL |
| IRQ | `r_nr_irq`/`r_irq_tab` | 1485-1501 | `RSS_IRQ_ALL`(17) → 清空；否则置 `CHECK_IRQ`；超 `NR_IRQ` → EINVAL |
| I/O 范围 | `r_nr_io`/`r_io_tab` | 1503-1524 | `RSS_IO_ALL`(17) → 清空；否则置 `CHECK_IO_PORT`；超 `NR_IO_RANGE` → EINVAL |
| kernel call mask | `s_k_call_mask` | 1527-1532 | `RSS_SYS_BASIC_CALLS` → `fill_call_mask(SYS_BASIC_CALLS)` |
| vm call mask | `s_vm_call_mask` | 1535-1540 | `RSS_VM_BASIC_CALLS` → `fill_call_mask(VM_BASIC_CALLS)` |
| control labels | `r_control`/`r_nr_control` | 1542-1564 | 超 `RS_NR_CONTROL`(8) → EINVAL；逐个 `copy_label` |
| 信号管理器 | `s_sig_mgr` | 1567 | = `rss_sigmgr` |
| 调度器 | `r_scheduler` | 1569-1575 | `r_scheduler != NONE` 时 = `rss_scheduler` |
| 命令 | `r_cmd` | 1577-1586 | 超 `MAX_COMMAND_LEN-1` → E2BIG；**必须以 `/` 开头**（EINVAL）；`build_cmd_dep` |
| progname | `proc_name` | 1588-1593 | 超 `sizeof(proc_name)-1` → E2BIG；拷入 |
| label | `label` | 1595-1615 | 空 label 时：有自定义 label → `copy_label`；否则默认 = `proc_name` |
| 脚本 | `r_script` | 1617-1626 | 超 `MAX_SCRIPT_LEN-1` → E2BIG；**仅非 `SF_CORE_SRV`**；置 `SF_USE_SCRIPT` |
| 二进制副本 | `r_exec` | 1628-1661 | `RSS_COPY` 且未 `SF_USE_COPY` → `read_exec`（09）；`RSS_REUSE` 扫同名 `SF_USE_COPY` 槽 `share_exec`；置 `SF_USE_COPY` |
| replica | — | 1662-1664 | `RSS_REPLICA` → `SF_USE_REPL` |
| 无二进制退避 | — | 1665-1667 | `RSS_NO_BIN_EXP` → `SF_NO_BIN_EXP` |
| detach | — | 1668-1673 | `RSS_DETACH` → `SF_DET_RESTART`（编辑可清） |
| 不重启 | — | 1674-1682 | `RSS_NORESTART` → `SF_NORESTART`；**core 服务 EPERM** |
| 周期 | `r_period` | 1684-1687 | 非 RS 自身时 = `rss_period` |
| 重启计数 | `r_restarts` | 1689-1692 | `rss_restarts` 非零时覆盖 |
| ASR 计数 | `r_asr_count` | 1694-1697 | `rss_asr_count >= 0` 时覆盖 |
| **重算掩码** | `s_ipc_to` | 1700 | **`init_privs(rp, &rp->r_priv)`**（→05） |

关键规则细节：

1. **`RSS_IRQ_ALL`/`RSS_IO_ALL`**（rs.h:27-28）= `RSS_NR_IRQ+1`/`RSS_NR_IO+1` = 17：请求方声明"要所有 IRQ/IO"（驱动专用）→ RS 清空计数（`rss_nr_irq=0`）且**不置** `CHECK_IRQ`/`CHECK_IO_PORT`——驱动可在运行时申请任意 IRQ/IO；**列出具体 IRQ/IO 时才置** `CHECK_IRQ`/`CHECK_IO_PORT`（const.h:148-149 内核资源检查位），内核把驱动限制在声明的资源内（do_irqctl.c:68、do_privctl.c:293）。
2. **call mask 的"basic calls"**：`RSS_SYS_BASIC_CALLS`/`RSS_VM_BASIC_CALLS`（rs.h:46-47）让服务获得基础内核调用集（`SYS_BASIC_CALLS`/`VM_BASIC_CALLS`）——否则只保留 `edit_slot` 里已有的调用。
3. **`r_cmd[0] != '/'` → EINVAL**（manager.c:1583）：服务命令必须是绝对路径。
4. **label 默认 = procname**（manager.c:1607-1610）：没提供自定义 label 时用可执行名。
5. **脚本仅非 core**（manager.c:1620）：`SF_CORE_SRV` 服务拒绝脚本（恢复脚本是动态服务的特权）。
6. **`RSS_NORESTART` + core → EPERM**（manager.c:1674-1678）：核心服务必须可重启。
7. **`r_period` 排除 RS 自身**（manager.c:1686）：RS 不给自己的心跳设周期。
8. **`init_privs` 收尾**（manager.c:1700）：所有字段就绪后重算 IPC 掩码（05 §2.7）。

### 2.5 `build_cmd_dep`（manager.c:289-323）——argv 解析

```c
void build_cmd_dep(struct rproc *rp)                   /* manager.c:289 */
{
  strcpy(rp->r_args, rp->r_cmd);		       /* manager.c:301 */
  arg_count = 0;
  rp->r_argv[arg_count++] = rp->r_args;		       /* manager.c:303 */
  cmd_ptr = rp->r_args;
  while(*cmd_ptr != '\0') {			       /* manager.c:305 */
      if (*cmd_ptr == ' ') {			       /* manager.c:306 */
          *cmd_ptr = '\0';			       /* manager.c:307 */
	  while (*++cmd_ptr == ' ') ;		       /* manager.c:308 */
	  if (*cmd_ptr == '\0') break;		       /* manager.c:309 */
	  if (arg_count>=ARGV_ELEMENTS-1) {	       /* manager.c:311 */
		break;				       /* 313 */
	  }
          rp->r_argv[arg_count++] = cmd_ptr;	       /* manager.c:316 */
      }
      cmd_ptr ++;
  }
  rp->r_argv[arg_count] = NULL;			       /* manager.c:321 */
  rp->r_argc = arg_count;			       /* manager.c:322 */
}
```

argv 格式：`path, arguments..., NULL`（注释 manager.c:298-299）。Rust 侧 `build_cmd_dep(cmd) -> Vec<&[u8]>`（slot.rs）保持：空格分词、尾部空参丢弃（manager.c:309）、`ARGV_ELEMENTS-1` 上限（manager.c:311-313）、**遇 NUL 停止**（manager.c:305-306，对应 C 的 `strcpy`/`while(*cmd_ptr != '\0')` 语义——固定大小 `cmd` 缓冲中 NUL 之后的 0 填充不属于命令）。

> **N11 修复（2026-08-16，todo §11）——argv[0] 恒存在**：C 在解析前**无条件**
> `r_argv[0] = r_args`（manager.c:299-300），所以空命令/纯空格命令得到 `argv = [""]`
> （`r_argc = 1`，exec 空路径 → ENOENT）。旧 Rust 版返回空 Vec → `argc = 0`，09/10 的
> exec 重建会走不同的失败形态。现在 `build_cmd_dep` 对无 token 输入返回 `vec![&[]]`
> （`rebuild_args` 因此恒写 `args[0] = 0`、`argc >= 1`），与 C 对齐。测试：
> `test_build_cmd_dep_empty_cmd_keeps_argv0`（空串/纯空格/NUL 开头三种边界）。

> **S1 修复（2026-08-15）**：token 容器从 `Vec<Label>` 改为 `Vec<&[u8]>`（借用 `cmd` 切片）。C 把完整 token 字节写入 `r_args`（manager.c:301-302），argv 指向其中；`Label` 的 16 字节截断（`strlcpy` 语义）只适用于 label/域名字段，**不适用于命令与参数**——超过 16 字节的路径/参数此前被静默截断，09/10 exec 落地后必然出错。`Vec<&[u8]>` 正是 C argv 布局（指针进 `r_args`）的 Rust 等价表达。

### 2.6 `inherit_service_defaults`（manager.c:1303-1329）——副本继承

```c
void inherit_service_defaults(def_rp, rp)              /* manager.c:1303 */
{
  /* 设备/域/PCI：不可变（1314-1319） */
  rpub->dev_nr = def_rpub->dev_nr;
  rpub->nr_domain = def_rpub->nr_domain;
  for (i = 0; i < def_rpub->nr_domain; i++)
	rpub->domain[i] = def_rpub->domain[i];
  rpub->pci_acl = def_rpub->pci_acl;
  /* 不可变系统/权限标志（1321-1325）：仅继承 IMM_SF/IMM_F 位 */
  rpub->sys_flags &= ~IMM_SF;
  rpub->sys_flags |= (def_rpub->sys_flags & IMM_SF);
  rp->r_priv.s_flags &= ~IMM_F;
  rp->r_priv.s_flags |= (def_rp->r_priv.s_flags & IMM_F);
  /* 陷阱掩码不可变（1327-1328） */
  rp->r_priv.s_trap_mask = def_rp->r_priv.s_trap_mask;
}
```

用途：`create_service`（10）为**副本/replica**（`RSS_REPLICA`）继承原服务的不可变属性——`IMM_SF`（rs.h，不可变系统标志：CORE_SRV/…）与 `IMM_F`（priv.h:50，`ROOT_SYS_PROC|VM_SYS_PROC|PREEMPTIBLE`）。只继承**不可变位**，可变位保持副本自己的配置。

---

## 3. Rust 设计决策

### 3.1 slot.rs：`RsStart` 模型（D1）

```rust
pub struct RsStart {
    pub flags: RssFlags,       // rss_flags（rs.h:106）
    pub uid: u32, pub sigmgr: Endpoint, pub scheduler: Endpoint,
    pub priority: i32, pub quantum: i32, pub cpu: i32,
    pub period: i64, pub restarts: i64, pub asr_count: i64,
    pub cmd: [u8; MAX_COMMAND_LEN], pub cmdlen: usize,   // 指针字段 → 数组+长度
    pub ipc_list: [u8; MAX_IPC_LIST], pub ipclen: usize,
    pub progname: Label, pub nr_control: i32, pub control: [Label; RS_NR_CONTROL],
    pub nr_irq: i32, pub irq: [i32; RSS_NR_IRQ],
    pub nr_io: i32, pub io: [IoRange; RSS_NR_IO],
}
pub struct RssFlags(bitflags);   // 20 个标志（rs.h:33-52）
```

设计差异：

- **指针字段 → 数组 + 长度**：C 的 `char *rss_cmd`/`char *rss_ipc`/`struct rss_label` 是指向请求方地址空间的指针，Rust 用固定数组 + 长度表示"拷入后的内容"（`sys_datacopy` 的产物）。
- **`RssFlags` 用 bitflags**：20 个标志类型安全；`RSS_*` 值断言测试防漂移。
- **计数域用 `i32`（对齐 C `int`，R20a）**：`rss_nr_irq`/`rss_nr_io`/`rss_nr_control` 在 C 中都是
  `int`（rs.h:122/124/136）。`i32` 保留 `edit_slot` 校验（manager.c:1486-1521）前的三态——
  `RSS_IRQ_ALL`/`RSS_IO_ALL` 哨兵（17）、0、负值（非法）；`usize` 会把非法负值包成巨大正数，
  与 `> NR_IRQ`/`> NR_IO_RANGE` 检查错位（R2/Fix #27 同理由，`ServiceSlot.nr_control` 已是 i32）。
- **`rss_io` 表用 `privilege::IoRange`（N9 单一权威）**：C 的 `rss_io`（rs.h:125，匿名
  `{unsigned base; unsigned len;}`）与内核 `struct io_range`（priv.h:13-16）同构，`edit_slot`
  逐项拷入 `s_io_tab`（manager.c:1516-1518）。Rust 收敛为同一类型，消除 C 双表同构（D6 收敛方向）。
- 未建模字段（PCI 表/state data/domains/script）标注 defer（A-10/17），使用时扩展。

### 3.2 `check_request` 纯函数（D2）

```rust
pub fn check_request(rs_start: &RsStart, machine: &Machine)
    -> Result<i32, Errno>
```

`machine.bsp_id`/`processors_count` 参数注入（`sys_getmachine` 结果，01/19），函数不触全局；返回解析后的 CPU（`RS_CPU_BSP` 的决策点）。C 的"越界 CPU 告警 + 回退 BSP"（request.c:1293）在返回值中体现。

### 3.3 `build_cmd_dep`（D3）

`Vec<&[u8]>` 返回 argv（借用 `cmd`，长度任意、不截断），`None` 终止符由 Vec 的末尾语义替代（`argv[argc] = NULL`，manager.c:322 不需要显式建模）。`ARGV_ELEMENTS-1` 上限（manager.c:311-313）与遇 NUL 停止（manager.c:305-306）均在测试中断言。

### 3.4 `edit_slot`/`init_slot` 的 sys_datacopy 依赖（D4，DEFERRED→19）

`copy_rs_start`/`copy_label`/`edit_slot`/`init_slot` 依赖 `sys_datacopy`（19 的 minix-sys 接线）。Rust 侧契约：`edit_slot` 的字段覆盖表（§2.4）作为**规格**，`RsStart` 提供类型化输入，`slot.rs` 的 `check_request`/`build_cmd_dep` 先行落地，`edit_slot` 本体在 19 接线后实现（DEFERRED 标注，同 `KernelApi` 模式）。

---

## 4. 实现详解（slot.rs）

模块结构（已实现，208 tests 总盘中 slot 24 个）：

```
slot.rs
├─ RSS_NR_IRQ/RSS_NR_IO/RSS_IRQ_ALL/RSS_IO_ALL（rs.h:25-28）
├─ RS_CPU_DEFAULT/RS_CPU_BSP/LAST_SPECIAL_PROC_NR（NR_SCHED_QUEUES 从 sched.rs 导入，N9）
├─ RssFlags（20 标志，rs.h:33-52）
├─ RsStart（rs.h:104-151 子集 + Default；计数域 i32 + io 表 [IoRange; RSS_NR_IO]）
├─ check_request（request.c:1265-1308 纯化）
├─ build_cmd_dep（manager.c:289-323 纯化）
└─ #[cfg(test)] 24 个测试（§5）
```

关键不变量：

1. **校验先于拷贝**：C 管线顺序（check_request → copy → edit_slot）保持；`check_request` 的 EINVAL 在 `sys_datacopy` 之前拦截坏参数。
2. **绝对路径强制**：`r_cmd[0] != '/'` → EINVAL（manager.c:1583）在 `RsStart` 语义中保留（`build_cmd_dep` 的调用方校验）。
3. **掩码重算收尾**：`init_privs`（manager.c:1700）在字段全落地后执行——Rust 的 `update_ipc_mask`（05）是它的等价。
4. **不可变继承只取位**：`inherit_service_defaults` 的 `IMM_SF`/`IMM_F` 位运算（manager.c:1322-1325）在 10 落地时保持精确位语义。

---

## 5. 测试要点

`cargo test -p minix-rs --lib`（208 passed，slot.rs 相关 14 项）：

| 测试 | 覆盖 |
|------|------|
| `test_check_request_ok` | 合法参数（SCHED 调度器/优先级/量子/默认 CPU/SELF sigmgr） |
| `test_check_request_scheduler` | KERNEL/合法特殊进程 OK；> LAST_SPECIAL_PROC_NR EINVAL |
| `test_check_request_priority_quantum` | priority ≥ NR_SCHED_QUEUES / quantum ≤ 0 → EINVAL |
| `test_check_request_cpu` | BSP→bsp_id / 正常→自身 / 越界→BSP / 负非特例→EINVAL |
| `test_check_request_sigmgr` | SELF/PM OK；越界 EINVAL |
| `test_rs_start_default_matches_c_caller` | Default 对齐 C 调用方：调度默认 + 资源计数/表全零（parse.c:1160） |
| `test_rs_start_resource_counts_are_i32_like_c_int` | 计数域 i32：哨兵 17 与负值可表示（C `int` 校验前语义） |
| `test_build_cmd_dep` | 多参数分词 |
| `test_build_cmd_dep_trailing_spaces` | 尾部空格丢弃（manager.c:308） |
| `test_build_cmd_dep_empty_cmd_keeps_argv0` | 空命令/纯空格/NUL 开头 → `[""]`，argc≥1（N11） |
| `test_build_cmd_dep_stops_at_nul` | NUL 提前终止（manager.c:305-306），NUL 后填充不入参 |
| `test_build_cmd_dep_long_token_not_truncated` | >16 字节 token 不截断（S1） |
| `test_build_cmd_dep_argv_cap` | `ARGV_ELEMENTS-1` 上限（manager.c:311-313） |
| `test_rss_constants` | `RSS_IRQ_ALL=17`/`RSS_IO_ALL=17`/CPU 特例值/标志位 |

---

## 6. 过渡：从"配置"到"执行"

槽位配置完成后，服务有了完整的"身份"（label/proc_name/uid）、"能力"（priv 字段 + IPC 掩码 + call masks）、"策略"（replica/script/backoff/restarts）。但**二进制还没加载**——`RSS_COPY` 分支调用的 `read_exec`（manager.c:1629 附近）是 09 的入口。

下一篇 `09-rs-exec.md`：服务二进制加载与执行——`read_exec`/`share_exec`/`free_exec`（`Arc<[u8]>` 共享副本，A-5）、`srv_execve` 的栈帧构建（A-8）、`exec_restart` 的 PM 交互。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/02-rs-process-table.md` — 槽位字段与四链
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/03-rs-privilege.md` — `r_priv` 字段、CHECK_IRQ/CHECK_IO_PORT、DSRV_* 默认
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/05-rs-ipc-sendmask.md` — `init_privs` 调用点（manager.c:1700）
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/07-rs-period-heartbeat.md` — `r_period` 消费
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/09-rs-exec.md` — read_exec/share_exec/free_exec
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/10-rs-service-create.md` — inherit_service_defaults 消费方
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/13-rs-control-requests.md` — do_up/do_edit 入口
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/19-rs-external-interfaces.md` — sys_datacopy 接线
- `minix3/minix/servers/rs/request.c:1265-1308`、`manager.c:135-169,289-323,1303-1329,1460-1703`、`include/minix/rs.h:24-52,104-151` — ground truth
- `os/servers/rs/src/slot.rs` — Rust 实现
