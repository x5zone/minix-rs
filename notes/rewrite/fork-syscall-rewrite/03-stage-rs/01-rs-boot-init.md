# 01-rs-boot-init: 启动入口与初始化骨架

> **分类**: 阶段 1 — 启动入口与初始化骨架（导航骨架文档）
> **源码**: `minix3/minix/servers/rs/main.c`（834 行）、`minix3/minix/servers/rs/table.c`（50 行）；SEF 库位于 `minix3/minix/lib/libsys/sef*.c`
> **Rust 模块**: `os/servers/rs/src/main.rs`、`os/servers/rs/src/lib.rs`（含 `table`/`boot`/`sef`/`dispatch` 子模块）
> **前置**: `notes/rewrite/fork-syscall-rewrite/03-stage-rs/00-rs-overview.md`、`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/09-vm-boot-protocol.md`（boot 链）
> **说明**: RS 进程从 `main()` 入口（`main.c:38`）经 `sef_local_startup()` 回调注册、`sef_cb_init_fresh()` 四步 boot（`main.c:158`）到主循环 dispatch 入口的全部启动链。本文档是阶段 1~7 全部文档的导航锚点：按 boot 顺序映射 02~18 的机制归属，但**不展开任何机制**（见 §2.2 前向引用豁免）。

---

## 1. 概念：引导自举——RS 如何"在别人依赖它之前，先把自己启动好"

### 1.0 章节引言

本章建立 RS 启动链的概念模型：RS 是"加载并启动其余用户服务"的角色（`00-rs-overview.md`），但它自己也是 boot 映像中的一员。它面临一个**先有鸡还是先有蛋**的问题——RS 要管理所有服务的生命周期，但启动时它自己也只是内核刚拉起来的一个普通用户进程。

> **本章不讲什么**（机制一律移交，只标注归属）:
> - `rproc`/`rprocpub` 结构字段与进程表槽位语义（`02-rs-process-table.md`）
> - priv 结构字段与 `sys_privctl` 每操作的效果（`03-rs-privilege.md`）
> - IPC send mask 与 `fill_send_mask` 语义（`05-rs-ipc-sendmask.md`）
> - 主循环细节：`reply`/`late_reply`/`EDONTREPLY`、`rs_idle_period`、`rs_isokendpt`（`06-rs-main-loop.md`）
> - 心跳与周期检查（`07-rs-period-heartbeat.md`）
> - `RS_INIT` ready 消息协议与 `catch_boot_init_ready` 机制（`12-rs-init-run.md`）
> - Live Update 状态机与 RS 自升级细节（`16-rs-live-update.md`、`18-rs-self-lifecycle.md`）
> - 外部 syscall 签名与消息映射（`19-rs-external-interfaces.md`）
>
> 本章只回答一个问题：**main() 到主循环之间，RS 按什么顺序、为什么按这个顺序，把自己初始化好**。

### 1.1 核心问题：RS 的"引导自举"

Minix3 的 boot 顺序（`minix3/minix/kernel/table.c:52-53`）把 RS 放在**第 2 个用户服务**（DS 第一、RS 紧随其后），因为所有其他用户服务（PM/VFS/SCHED/DS/...）都依赖 RS 来获得运行许可与初始化。但反过来，RS 启动时什么设施都没有：

```
RS 需要管理服务生命周期 → 服务需要 RS 允许才能运行 → RS 必须先启动
RS 启动需要知道 boot 映像 → boot 映像在内核 → RS 要 sys_getimage 拷贝
RS 要让服务运行 → 服务需要 priv/调度/初始许可 → 这些都是 kernel/VM/sched 的机制
```

Minix3 的解法是**分阶段初始化**：`sef_cb_init_fresh()`（`main.c:158-494`）把 boot 拆成 4 步，每一步只依赖前一步已经建立的设施：

1. **Step 1（`main.c:244-346`，四步注释在 main.c:239-243）——建立全表属性**：为 boot 映像中的每个服务在 RS 本地进程表中建立槽位，并准备好 priv/sys/dev 三方面属性（priv 结构、send mask、调度参数、命令串）。此时**没有**向 kernel 提交任何东西（除 RS/VM 例外），纯本地准备。
2. **Step 2（`main.c:348-399`）——允许运行**：此时每个服务的属性已就绪，RS 才通过 `sched_init_proc` + `sys_privctl(SYS_PRIV_ALLOW)` 让服务真正获得调度与运行许可，并发送 `RS_INIT` 初始化消息（RS/VM 例外：它们已在运行，直接 `init_service` 自模拟）。
3. **Step 3（`main.c:401-407`）——收齐初始化完成**：阻塞接收全部服务的 `RS_INIT` 回复（同步 boot 的服务立即收，其余累积后统一收），确认每个服务完成初始化。
4. **Step 4（`main.c:409-433`）——补全协作信息**：向 PM 查询每个服务的 pid（`getnpid`），并设置周期检查闹钟（`sys_setalarm(RS_DELTA_T)`），之后进入主循环。

这个"先本地建表、再统一放行、再收齐确认"的顺序是本文档后半部分所有 C 源码分析的骨架。

### 1.2 boot 主线图

RS 的启动与运行是严格线性的（继承 `01-stage-kernel/00-kernel-overview.md §3` 与 `02-stage-vm/plan.md §1.2` 的组织原则）：

```
kernel 启动 RS（root sysproc，boot_image 中 RS_PROC_NR 紧随 DS，kernel/table.c:52-53）
  │
  ▼  main.c:38  main()
  ├─ sef_local_startup()（main.c:51）          ← 本文档 §2.2：SEF 回调注册（A-7）
  ├─ sys_getmachine(&machine)（main.c:53）     ← 机器信息，本文档调用点；签名归 19
  ▼  sef_cb_init_fresh()（main.c:158，boot 锚点）
  ├─ env_parse("rs_verbose")（main.c:179）     ← 配置；GET_HZ（main.c:181）签名归 19
  ├─ rinit.rproctab_gid grant（main.c:185）    ← 创建点本文档；消费在 12（init_service）
  ├─ RUPDATE_INIT() + shutting_down=FALSE（main.c:192-193）← 全局复位；状态机归 16
  ├─ sys_getimage(image)（main.c:196）         ← boot 映像拷贝，本文档调用点；A-13
  ├─ 计数核对 + 表重置（main.c:200-250）       ← 02（进程表重置）
  ├─ Step 1: 逐服务设 priv/sys/dev 属性（main.c:244-346）
  │     ├─ boot_image_info_lookup 四表查找（main.c:253-254，机制本文档 §2.4）
  │     ├─ priv 初始化 + sys_privctl SET_SYS（RS/VM 例外跳过）（main.c:258-296）→ 03
  │     ├─ fill_send_mask（main.c:273）→ 05
  │     └─ slot 建立（r_flags/rproc_ptr/r_pid 等，main.c:324-345）→ 02
  ├─ Step 2: 允许运行（main.c:348-399）
  │     ├─ RS/VM 例外：init_service 自模拟（main.c:362-373）→ 12
  │     └─ 其他：sched_init_proc + SYS_PRIV_ALLOW + init_service + catch_boot_init_ready（main.c:375-399）→ 03/12
  ├─ Step 3: catch 剩余 init ready（main.c:401-407）→ 12
  ├─ Step 4: getnpid（main.c:413-431）→ 02/19；sys_setalarm(RS_DELTA_T)（main.c:433）→ 07
  └─ USE_LIVEUPDATE: RS 自升级（main.c:436-491）→ 18（clone_slot/srv_fork/update_service/cpf_reload/cleanup_service/vm_memctl pin）
  │
  ▼  main.c:50-131  主循环（运行时）
  ├─ rs_idle_period()（main.c:59）             → 06
  ├─ get_work() → sef_receive_status(ANY)（main.c:62）→ 06
  ├─ rs_isokendpt()（main.c:64）               → 02
  ├─ 消息分类（main.c:70-127）：
  │     ├─ CLOCK notify  → do_period()        → 07
  │     ├─ 其他 notify   → 心跳（r_alive_tm）  → 07
  │     ├─ RS_INIT       → do_init_ready()    → 12
  │     ├─ RS_LU_PREPARE → do_upd_ready()     → 12/16
  │     └─ RS_* 请求     → do_up/do_down/…    → 13/14/16
  └─ reply() / EDONTREPLY（main.c:125-128）    → 06
```

**阅读顺序即执行顺序**：本文档按此图自上而下展开，每一步回答"它在启动时序中的位置"。

### 1.3 SEF 生命周期是什么

SEF（System Event Framework，`minix3/minix/lib/libsys/sef.c`）是 Minix3 用户态服务共享的启动/热更新框架。RS 通过 `sef_local_startup()`（`main.c:136-152`）注册 7 个回调接入 SEF：

| SEF 回调 | 注册时机 | RS 的处理 | 机制归属 |
|---------|---------|----------|---------|
| `sef_cb_init_fresh` | `sef_local_startup`（main.c:139） | 四步 boot 全流程 | **本文档**（§2.3） |
| `sef_cb_init_restart` | 同上（main.c:140） | restart 状态恢复（`SEF_CB_INIT_RESTART_STATEFUL` + update_service + init_service + alarm） | 18 |
| `sef_cb_init_lu` | 同上（main.c:141） | Live Update 接管（`SEF_CB_INIT_LU_DEFAULT` + update_service + 断言链） | 18 |
| `sef_cb_init_response` | 同上（main.c:144） | RS 自模拟 RS_INIT 消息（`do_init_ready` 转发） | 12 |
| `sef_cb_lu_response` | 同上（main.c:145） | RS 自模拟 RS_LU_PREPARE 消息（`do_upd_ready` 转发） | 12 |
| `sef_cb_signal_handler` | 同上（main.c:148） | `SIGCHLD`→`do_sigchld`、`SIGTERM`→`do_shutdown` | 06 |
| `sef_cb_signal_manager` | 同上（main.c:149） | 系统信号转发（终止→terminate_service、VM 免转发、SIGS_SIGNAL_RECEIVED） | 06 |

SEF 的核心价值在 C 中是"统一的服务生命周期协议"；Rust 侧将其简化为显式回调表（见 §3.3，ARCH A-7）。**本文档只负责注册表的建立**；每个回调的机制语义在归属文档中展开。

### 1.4 主循环四类消息分类

`main()` 的主循环（`main.c:50-131`）接收消息后按 `ipc_status` 与 `m_type` 分类（本文档 §2.1 只到分类骨架，处理机制全部移交）：

1. **通知消息（`is_ipc_notify`）**：无需回复。`CLOCK` 通知 → `do_period()`（周期检查，07）；其他来源通知 → 心跳时间戳更新（`r_alive_tm`，07）。
2. **ready 消息**：`RS_INIT` → `do_init_ready`（12）；`RS_LU_PREPARE` → `do_upd_ready`（12，update 分支 → 16）。
3. **控制请求**：15 个 `RS_*` 请求 → `do_up`/`do_down`/`do_refresh`/`do_restart`/`do_shutdown`/`do_update`/`do_clone`/`do_unclone`/`do_edit`/`do_sysctl`/`do_fi`/`do_getsysinfo`/`do_lookup`（13/14/16）。
4. **未知请求**：`default` 分支打印警告并返回 `ENOSYS`（main.c:118-121）。

**每个 handler 的名称即文档锚点**：本文档只列"分类表 + 归属"，不展开任何 handler 机制（plan §2.2 前向引用豁免）。

---

## 2. C 源码分析

> 本文档 ground truth 为 `minix3/minix/servers/rs/main.c` 与 `minix3/minix/servers/rs/table.c`。所有行号以 grep 实证为准。

### 2.1 main()：入口、SEF 启动、主循环骨架（main.c:38-131）

```c
int main(void)
{
  message m;					/* request message */
  int ipc_status;				/* status code */
  int call_nr, who_e, who_p;			/* call number and caller */
  int result;                 			/* result to return */
  int s;

  /* SEF local startup. */
  sef_local_startup();                            /* 51 */
  
  if (OK != (s=sys_getmachine(&machine)))         /* 53 */
	  panic("couldn't get machine info: %d", s);

  /* Main loop - get work and do it, forever. */         
  while (TRUE) {                                  /* 57 */
      /* Perform sensitive background operations when RS is idle. */
      rs_idle_period();                           /* 59 — 06 */

      /* Wait for request message. */
      get_work(&m, &ipc_status);                  /* 62 — 06 */
      who_e = m.m_source;
      if(rs_isokendpt(who_e, &who_p) != OK) {     /* 64 — 02 */
          panic("message from bogus source: %d", who_e);
      }
      call_nr = m.m_type;

      if (is_ipc_notify(ipc_status)) {            /* 70 — 通知消息 */
          switch (who_p) {
          case CLOCK:
	      do_period(&m);	                /* 73 — 07 */
	      continue;
	  default:				/* 心跳通知 */
	      if (rproc_ptr[who_p] != NULL) {      /* 77 — 07 */
		  rproc_ptr[who_p]->r_alive_tm = m.m_notify.timestamp;
	      } else {
		  printf("RS: warning: got unexpected notify message from %d\n",
		      m.m_source);
	      }
	  }
      }
      else {                                      /* 84 — 普通请求 */
          switch(call_nr) {
          /* User requests. */
	  case RS_UP:		result = do_up(&m);		break;   /* 13 */
          case RS_DOWN: 	result = do_down(&m); 		break;   /* 13 */
          case RS_REFRESH: 	result = do_refresh(&m); 	break;   /* 13 */
          case RS_RESTART: 	result = do_restart(&m); 	break;   /* 13 */
          case RS_SHUTDOWN: 	result = do_shutdown(&m); 	break;   /* 13 */
          case RS_UPDATE: 	result = do_update(&m); 	break;   /* 16 */
          case RS_CLONE: 	result = do_clone(&m); 		break;   /* 13 */
	  case RS_UNCLONE: 	result = do_unclone(&m);	break;   /* 13 */
          case RS_EDIT: 	result = do_edit(&m); 		break;   /* 13 */
	  case RS_SYSCTL:	result = do_sysctl(&m);		break;   /* 14 */
	  case RS_FI:	result = do_fi(&m);		break;   /* 14 */
          case RS_GETSYSINFO:  result = do_getsysinfo(&m);     break;   /* 14 */
	  case RS_LOOKUP:	result = do_lookup(&m);		break;   /* 14 */
	  /* Ready messages. */
	  case RS_INIT: 	result = do_init_ready(&m); 	break;   /* 12 */
	  case RS_LU_PREPARE: 	result = do_upd_ready(&m); 	break;   /* 12/16 */
          default: 
              printf("RS: warning: got unexpected request %d from %d\n",
                  m.m_type, m.m_source);
              result = ENOSYS;                            /* 121 */
          }

          /* Finally send reply message, unless disabled. */
          if (result != EDONTREPLY) {                     /* 125 — 06 */
	      m.m_type = result;
              reply(who_e, NULL, &m);                     /* 127 */
          }
      }
  }
}
```

要点：

- **`sef_local_startup()` 是本文档的第一个锚点**（§2.2）：SEF 回调注册完成后，`sef_startup()` 内部通过 IPC 接收 `RS_INIT`/`RS_LU_PREPARE` 并分派到 `sef_cb_init_fresh`（启动期）或 `sef_cb_init_restart`/`sef_cb_init_lu`（恢复期）。
- **`sys_getmachine(&machine)`**（main.c:53）：把机器信息（处理器数、BSP id、APIC 等，`minix3/minix/include/minix/type.h:122-131`）拷贝到全局 `machine`。Rust 侧外部接口契约归 `19-rs-external-interfaces.md`。
- **主循环骨架**：`rs_idle_period`（06）→ `get_work`（06）→ `rs_isokendpt`（02）→ 分类（§1.4）。**每个 handler 只标注归属，机制不在此展开**。
- **`EDONTREPLY` 协议**（main.c:125）：handler 返回 `EDONTREPLY` 表示"稍后自行回复"（late reply，06），主循环不代发。`reply(who_e, NULL, &m)` 的第二个参数 `rp` 为 NULL 表示不查 slot（06）。

### 2.2 sef_local_startup()：SEF 回调注册（main.c:136-152）

```c
static void sef_local_startup()
{
  /* Register init callbacks. */
  sef_setcb_init_fresh(sef_cb_init_fresh);        /* 139 */
  sef_setcb_init_restart(sef_cb_init_restart);    /* 140 */
  sef_setcb_init_lu(sef_cb_init_lu);              /* 141 */

  /* Register response callbacks. */
  sef_setcb_init_response(sef_cb_init_response);  /* 144 */
  sef_setcb_lu_response(sef_cb_lu_response);      /* 145 */

  /* Register signal callbacks. */
  sef_setcb_signal_handler(sef_cb_signal_handler);/* 148 */
  sef_setcb_signal_manager(sef_cb_signal_manager);/* 149 */

  /* Let SEF perform startup. */
  sef_startup();                                  /* 151 */
}
```

**注册表语义**（`minix3/minix/include/minix/sef.h`）：`sef_setcb_*` 是 libsys 的全局函数指针赋值；`sef_startup()` 进入 SEF 状态机——启动期接收 `SEF_INIT` 消息（`m_type == RS_INIT`，payload 含 `sef_init_info_t`），按 `info->init_type`（`SEF_INIT_FRESH=0`/`SEF_INIT_LU=1`/`SEF_INIT_RESTART=2`，sef.h:93-95）分派到对应回调。RS 注册全部 7 个回调，是**唯一注册全量的用户态服务**（其他服务只注册自己需要的子集）。7 个回调的机制归属见 §1.3 表。

### 2.3 sef_cb_init_fresh()：四步 boot（main.c:158-494）

`sef_cb_init_fresh` 是本文档的 boot 锚点。逐段分析：

#### 2.3.1 前置：配置、频率、grant、全局复位、boot 映像拷贝（main.c:178-198）

```c
  /* See if we run in verbose mode. */
  env_parse("rs_verbose", "d", 0, &rs_verbose, 0, 1);         /* 179 */

  if ((s = sys_getinfo(GET_HZ, &system_hz, sizeof(system_hz), 0, 0)) != OK)  /* 181 */
	  panic("Cannot get system timer frequency\n");

  /* Initialize the global init descriptor. */
  rinit.rproctab_gid = cpf_grant_direct(ANY, (vir_bytes) rprocpub,    /* 185 */
      sizeof(rprocpub), CPF_READ);
  if(!GRANT_VALID(rinit.rproctab_gid)) {                          /* 186-189 */
      panic("unable to create rprocpub table grant: %d", rinit.rproctab_gid);
  }

  /* Initialize some global variables. */
  RUPDATE_INIT();                                                 /* 192 */
  shutting_down = FALSE;                                          /* 193 */

  /* Get a copy of the boot image table. */
  if ((s = sys_getimage(image)) != OK) {                          /* 196 */
      panic("unable to get copy of boot image table: %d", s);
  }
```

- **`env_parse("rs_verbose")`**（main.c:179）：从环境/启动参数解析 `rs_verbose` 调试开关（0/1）。Rust 侧构建配置宏 → cargo feature 或 env 注入（A-11，见 §3.6）。
- **`sys_getinfo(GET_HZ, &system_hz)`**（main.c:181）：系统时钟频率（ticks/秒），是 `RS_INIT_T`/`RS_DELTA_T` 等周期常量的基础（const.h:48-49，语义归 07）。
- **`rinit.rproctab_gid` 创建点**（main.c:185）：`cpf_grant_direct` 把 RS 的 `rprocpub` 公开表以只读 grant 形式共享给任意进程——服务通过 `RS_INIT` 消息携带此 gid 读取公开信息。**创建点在本文档，消费点在 12（`init_service` 把 gid 放入 RS_INIT 消息）**（plan D-16）。
- **`RUPDATE_INIT()` + `shutting_down = FALSE`**（main.c:192-193）：全局 update 描述符复位 + 关停标志复位。update 状态机语义归 16。
- **`sys_getimage(image)`**（main.c:196）：把内核的 `boot_image[]` 表（`NR_BOOT_PROCS` 项）运行期拷贝到 RS 本地栈数组。**这是 A-13 的关键**：boot 映像来源在 minix-rs 中如何建模见 §3.1。

#### 2.3.2 计数核对与表重置（main.c:200-250）

```c
  /* Determine the number of system services in the boot image table. */
  nr_image_srvs = 0;
  for(i=0;i<NR_BOOT_PROCS;i++) {                                  /* 203-211 */
      ip = &image[i];
      if(iskerneln(_ENDPOINT_P(ip->endpoint))) continue;   /* 跳过内核任务 */
      nr_image_srvs++;
  }
  /* ... 同样统计 priv 表（跳过内核任务）... */
  if(nr_image_srvs != nr_image_priv_srvs) {                       /* 224-236 */
	panic("boot image table and boot image priv table mismatch");
  }

  /* Reset the system process table. */
  for (rp=BEG_RPROC_ADDR; rp<END_RPROC_ADDR; rp++) {              /* 238-250 */
      rp->r_flags = 0;
      rp->r_init_err = ERESTART;
      rp->r_pub = &rprocpub[rp - rproc];
      rp->r_pub->in_use = FALSE;
      rp->r_pub->old_endpoint = NONE;
      rp->r_pub->new_endpoint = NONE;
  }
```

- **计数核对**（main.c:225-227）：boot 映像表中系统服务数必须等于 RS 自身 priv 表中系统服务数——这是"RS 的表与内核的表一致"的启动协议完整性检查。`iskerneln` 按端点号判断内核任务（负端点）。**R17（2026-08-16）**：`validate_tables` 另校验每行 `endpoint.slot() == proc_nr`——内核 boot 表端点由 `_ENDPOINT(0, proc_nr)` 派生（main.c:196），手写 placeholder 若错位会 boot 错端点；`boot_img` 的 16 字节名钳制（`len.min(16)` 的 const 求值版本）使超长名不再以晦涩的 const 越界编译错误暴露。
- **表重置**（main.c:230-237）：把 `rproc`/`rprocpub` 全表清零（`r_flags=0`、`r_init_err=ERESTART`、`in_use=FALSE`、`old/new_endpoint=NONE`）。进程表字段语义归 02；这里只标注调用点。

#### 2.3.3 Step 1：逐服务设置 priv/sys/dev 属性（main.c:244-346）

```c
  for (i=0; boot_image_priv_table[i].endpoint != NULL_BOOT_NR; i++) {   /* 244 */
      boot_image_priv = &boot_image_priv_table[i];
      if(iskerneln(_ENDPOINT_P(boot_image_priv->endpoint))) continue;

      boot_image_info_lookup(boot_image_priv->endpoint, image,
          &ip, NULL, &boot_image_sys, &boot_image_dev);            /* 253-254 */
      rp = &rproc[boot_image_priv - boot_image_priv_table];        /* 255 */
      rpub = rp->r_pub;

      /* Set privileges. */
      strcpy(rpub->label, boot_image_priv->label);                 /* 262 */
      rp->r_priv.s_id = static_priv_id(_ENDPOINT_P(...));          /* 265-266 */
      rp->r_priv.s_flags = boot_image_priv->flags;                 /* 269 */
      rp->r_priv.s_init_flags = SRV_OR_USR(rp, SRV_I, USR_I);      /* 270 */
      rp->r_priv.s_trap_mask= SRV_OR_USR(rp, SRV_T, USR_T);        /* 271 */
      ipc_to = SRV_OR_USR(rp, SRV_M, USR_M);                       /* 272 */
      fill_send_mask(&rp->r_priv.s_ipc_to, ipc_to == ALL_M);       /* 273 — 05 */
      rp->r_priv.s_sig_mgr= SRV_OR_USR(rp, SRV_SM, USR_SM);        /* 274 */
      rp->r_priv.s_bak_sig_mgr = NONE;                             /* 275 */
      calls = SRV_OR_USR(rp, SRV_KC, USR_KC) == ALL_C ? all_c : no_c;  /* 278 */
      fill_call_mask(calls, NR_SYS_CALLS,                          /* 279-280 */
          rp->r_priv.s_k_call_mask, KERNEL_CALL, TRUE);

      /* RS and VM are exceptions and are already running. */
      if(boot_image_priv->endpoint != RS_PROC_NR &&                /* 285-291 */
         boot_image_priv->endpoint != VM_PROC_NR) {
          if ((s = sys_privctl(ip->endpoint, SYS_PRIV_SET_SYS, &(rp->r_priv))) != OK) {
              panic("unable to set privilege structure: %d", s);
          }
      }
      if ((s = sys_getpriv(&(rp->r_priv), ip->endpoint)) != OK) { /* 294-296 */
          panic("unable to synch privilege structure: %d", s);
      }

      /* Set sys properties. */
      rpub->sys_flags = boot_image_sys->flags;                     /* 301 */
      /* Set dev properties. */
      rpub->dev_nr = boot_image_dev->dev_nr;                       /* 306 */

      strlcpy(rp->r_cmd, ip->proc_name, sizeof(rp->r_cmd));        /* 309 */
      rp->r_script[0]= '\0';                                       /* 310 */
      build_cmd_dep(rp);                                           /* 311 — 08 */
      strlcpy(rpub->proc_name, ip->proc_name, sizeof(rpub->proc_name)); /* 313 */

      calls = SRV_OR_USR(rp, SRV_VC, USR_VC) == ALL_C ? all_c : no_c;  /* 316 */
      fill_call_mask(calls, NR_VM_CALLS, rpub->vm_call_mask, VM_RQ_BASE, TRUE); /* 317 */

      rp->r_scheduler = SRV_OR_USR(rp, SRV_SCH, USR_SCH);          /* 320 */
      rp->r_priority = SRV_OR_USR(rp, SRV_Q, USR_Q);               /* 321 */
      rp->r_quantum = SRV_OR_USR(rp, SRV_QT, USR_QT);              /* 322 */

      rpub->endpoint = ip->endpoint;                               /* 325 */
      rp->r_old_rp = NULL; rp->r_new_rp = NULL;                    /* 328-331 */
      rp->r_prev_rp = NULL; rp->r_next_rp = NULL;
      rp->r_uid = 0; rp->r_check_tm = 0;                           /* 332-333 */
      rp->r_alive_tm = getticks(); rp->r_stop_tm = 0;
      rp->r_asr_count = 0; rp->r_restarts = 0; rp->r_period = 0;   /* 336-338 */
      rp->r_exec = NULL; rp->r_exec_len = 0;                       /* 339-340 */

      rp->r_flags = RS_IN_USE | RS_ACTIVE;                         /* 343 */
      rproc_ptr[_ENDPOINT_P(rpub->endpoint)]= rp;                  /* 344 */
      rpub->in_use = TRUE;                                         /* 345 */
  }
```

要点（**机制全部移交，此处只列结构与归属**）：

- **`boot_image_info_lookup`**（main.c:253-254）：一次调用同时查找 image/priv/sys/dev 四张表（§2.4）。
- **priv 结构初始化**（main.c:258-280）：label、静态 priv id、flags、init flags、trap mask、send mask（`fill_send_mask` → 05）、sig mgr、call mask（kernel 侧）。**priv 结构字段语义归 03**。
- **RS/VM 例外**（main.c:285-291）：`SYS_PRIV_SET_SYS` 对 RS/VM **跳过**——它们"已经在运行"（plan D-9）。其余服务则把 priv 结构提交给 kernel。`sys_getpriv`（main.c:294）再同步回本地结构。
- **sys/dev 属性**（main.c:301-306）：`sys_flags`（`SF_*` 全表在 99）、`dev_nr`（主设备号）。
- **命令/调度属性**（main.c:309-322）：`r_cmd`/`r_script`/`build_cmd_dep`（08）、`vm_call_mask`（VM 侧调用掩码）、`scheduler`/`priority`/`quantum`（03）。
- **slot 默认值 + 激活**（main.c:324-345）：四链指针清 NULL（A-3）、`r_uid=0`、监控字段清 0、`RS_IN_USE | RS_ACTIVE` 激活、`rproc_ptr` 快速索引登记（A-4）、`in_use=TRUE`。字段语义归 02。

#### 2.3.4 Step 2：允许运行（main.c:348-399）

```c
  nr_uncaught_init_srvs = 0;
  for (i=0; boot_image_priv_table[i].endpoint != NULL_BOOT_NR; i++) {   /* 351 */
      boot_image_priv = &boot_image_priv_table[i];
      if(iskerneln(_ENDPOINT_P(boot_image_priv->endpoint))) continue;

      rp = &rproc[boot_image_priv - boot_image_priv_table];        /* 356 */
      rpub = rp->r_pub;

      /* RS/VM are already running as we speak. */
      if(boot_image_priv->endpoint == RS_PROC_NR ||                /* 360-365 */
         boot_image_priv->endpoint == VM_PROC_NR) {
          if ((s = init_service(rp, SEF_INIT_FRESH, rp->r_priv.s_init_flags)) != OK) {
              panic("unable to initialize %d: %d", ...);
          }
          /* VM will still send an RS_INIT message, though. */
          if (boot_image_priv->endpoint != RS_PROC_NR) {
              nr_uncaught_init_srvs++;                             /* 366-367 */
          }
          continue;
      }

      /* Allow the service to run. */
      if ((s = sched_init_proc(rp)) != OK) {                       /* 376 — 03 */
          panic("unable to initialize scheduling: %d", s);
      }
      if ((s = sys_privctl(rpub->endpoint, SYS_PRIV_ALLOW, NULL)) != OK) {  /* 379 — 03 */
          panic("unable to initialize privileges: %d", s);
      }
      if(boot_image_priv->flags & SYS_PROC) {                      /* 386 */
          if ((s = init_service(rp, SEF_INIT_FRESH, rp->r_priv.s_init_flags)) != OK) {  /* 387 */
              panic("unable to initialize service: %d", s);
          }
          if(rpub->sys_flags & SF_SYNCH_BOOT) {                    /* 390 */
              catch_boot_init_ready(rpub->endpoint);               /* 392 — 12 */
          }
          else {
              nr_uncaught_init_srvs++;                             /* 396 */
          }
      }
  }
```

要点：

- **RS/VM 例外**（main.c:362-373）：这两个服务已在内核中运行（RS 自己、VM 先于 RS 启动），因此不经过 `sched_init_proc` + `SYS_PRIV_ALLOW`，而是直接 `init_service` 自模拟初始化（12）。VM 虽然已运行，**仍会异步发回一条 RS_INIT 消息**，计入 `nr_uncaught_init_srvs`（plan D-5）。
- **普通服务**（main.c:375-398）：`sched_init_proc`（调度器登记，03）→ `sys_privctl(SYS_PRIV_ALLOW)`（解除运行抑制，03）→ `init_service`（发送 RS_INIT 初始化消息，12）。`SF_SYNCH_BOOT` 标志（rs.h:192）决定同步等待该服务的 init ready 还是累积到 Step 3 统一收。
- `init_service`/`catch_boot_init_ready` 机制全部归 12；此处只列调用点与标志判定。

#### 2.3.5 Step 3：收齐剩余 init ready（main.c:401-407）

```c
  while(nr_uncaught_init_srvs) {                                    /* 404 */
      catch_boot_init_ready(ANY);                                   /* 405 */
      nr_uncaught_init_srvs--;                                      /* 406 */
  }
```

阻塞接收全部剩余服务的 `RS_INIT` 回复（`catch_boot_init_ready(ANY)`，机制归 12）。**这一步使 boot 严格串行**：所有 boot 服务初始化完成后，RS 才进入 Step 4。

#### 2.3.6 Step 4：getnpid + sys_setalarm（main.c:409-433）

```c
  for (i=0; boot_image_priv_table[i].endpoint != NULL_BOOT_NR; i++) {   /* 413 */
      boot_image_priv = &boot_image_priv_table[i];
      if(iskerneln(_ENDPOINT_P(boot_image_priv->endpoint))) continue;
      rp = &rproc[boot_image_priv - boot_image_priv_table];
      rpub = rp->r_pub;
      rp->r_pid = getnpid(rpub->endpoint);                          /* 426 — 02/19 */
      if(rp->r_pid < 0) {
          panic("unable to get pid: %d", rp->r_pid);                /* 428 */
      }
  }

  /* Set alarm to periodically check service status. */
  if (OK != (s=sys_setalarm(RS_DELTA_T, 0)))                        /* 433 */
      panic("couldn't set alarm: %d", s);
```

- **`getnpid`**（main.c:426）：向 PM 查询每个服务的 pid 存入 slot（`r_pid` 字段语义归 02；`getnpid` 签名归 19）。
- **`sys_setalarm(RS_DELTA_T, 0)`**（main.c:433）：设置周期检查闹钟（`RS_DELTA_T = system_hz`，const.h:49），主循环收到 `CLOCK` 通知后执行 `do_period`（07）。**这是主循环"心跳监控"的启动前提**。

#### 2.3.7 USE_LIVEUPDATE：RS 自升级（main.c:436-491）

```c
#if USE_LIVEUPDATE
  /* Now create a new RS instance and let the current
   * instance live update into the replica. Clone RS' own slot first.
   */
  rp = rproc_ptr[_ENDPOINT_P(RS_PROC_NR)];                          /* 440 */
  if((s = clone_slot(rp, &replica_rp)) != OK) {                     /* 441-443 — 10/18 */
      panic("unable to clone current RS instance: %d", s);
  }
  pid = srv_fork(0, 0);                                             /* 446 — 10 */
  if(pid < 0) panic("unable to fork a new RS instance: %d", pid);   /* 447-449 */
  replica_pid = pid ? pid : getpid();                               /* 450 */
  getprocnr(replica_pid, &replica_endpoint);                        /* 451 */
  replica_rp->r_pid = replica_pid;                                  /* 453 */
  replica_rp->r_pub->endpoint = replica_endpoint;                   /* 454 */

  if(pid == 0) {                                                    /* 456 */
      /* New RS instance running. */
      s = update_service(&rp, &replica_rp, RS_SWAP, 0);             /* 460-463 — 16 */
      cpf_reload();                                                 /* 464 */
      cleanup_service(rp);                                          /* 467 — 15 */
      vm_memctl(RS_PROC_NR, VM_RS_MEM_PIN, 0, 0);                   /* 470-472 — 10/19 */
  }
  else {
      /* Old RS instance running. */
      s = sys_privctl(replica_endpoint, SYS_PRIV_SET_SYS, &(replica_rp->r_priv));  /* 478-481 — 03 */
      sched_init_proc(replica_rp);                                  /* 482 — 03 */
      s = sys_privctl(replica_endpoint, SYS_PRIV_YIELD, NULL);      /* 485 — 03 */
      NOT_REACHABLE;                                                /* 489 */
  }
#endif
```

**语义**：boot 完成后，RS 立即 fork 一个自身副本（`srv_fork(0,0)` 经 PM，A-1），并让自己 live update 到副本中（`update_service(RS_SWAP)`）。子进程（新 RS）接管：`cpf_reload` + `cleanup_service`（清理旧实例）+ `vm_memctl(VM_RS_MEM_PIN)`（钉住内存）；父进程（旧 RS）给副本设 priv + 调度 + `SYS_PRIV_YIELD`（让出）。**完整流程机制归 18**（RS 自生命周期）；本文档只列调用链。`USE_LIVEUPDATE` 为编译期宏（默认 yes，`share/mk/bsd.own.mk:1499`）→ Rust feature gate（A-11，§3.6）。

### 2.4 boot_image_info_lookup()：四表查找（main.c:709-779）

```c
static void boot_image_info_lookup(endpoint, image, ip, pp, sp, dp)
endpoint_t endpoint;
struct boot_image *image;
struct boot_image **ip;
struct boot_image_priv **pp;
struct boot_image_sys **sp;
struct boot_image_dev **dp;
{
  int i;

  /* When requested, locate the corresponding entry in the boot image table
   * or panic if not found. */
  if(ip) {
      for (i=0; i < NR_BOOT_PROCS; i++) {                 /* 723-729 */
          if(image[i].endpoint == endpoint) { *ip = &image[i]; break; }
      }
      if(i == NR_BOOT_PROCS) panic("boot image table lookup failed");   /* 731 */
  }

  /* ... priv table: 同样语义，panics if not found ... */  /* 735-748 */

  /* sys table: 未命中则采用 DEFAULT_BOOT_NR 默认条目 */    /* 750-763 */
  if(sp) {
      for (i=0; boot_image_sys_table[i].endpoint != DEFAULT_BOOT_NR; i++) {
          if(boot_image_sys_table[i].endpoint == endpoint) { *sp = &boot_image_sys_table[i]; break; }
      }
      if(boot_image_sys_table[i].endpoint == DEFAULT_BOOT_NR)
          *sp = &boot_image_sys_table[i];         /* accept the default entry */
  }

  /* dev table: 同样 DEFAULT 兜底语义 */                   /* 765-778 */
}
```

要点（`static`，仅 `sef_cb_init_fresh` Step 1 调用）：

- **image 表 / priv 表**：未命中 → `panic`（启动协议错误，fail-closed）。**priv 表的哨兵是 `NULL_BOOT_NR`**（const.h:61，= `NR_BOOT_PROCS`）；**sys/dev 表的哨兵是 `DEFAULT_BOOT_NR`**（const.h:62），语义不同——sys/dev 未命中是**正常**的（服务无覆盖配置），采用默认条目。
- 输出参数按需填充（`ip`/`pp`/`sp`/`dp` 可为 NULL）。Step 1 调用时传 `&ip, NULL, &boot_image_sys, &boot_image_dev`（main.c:253-254）——只查 image/sys/dev 三表，priv 表条目由外层循环直接索引（`rp = &rproc[boot_image_priv - boot_image_priv_table]`）。

### 2.5 catch_boot_init_ready() 调用点（main.c:784-821）

`catch_boot_init_ready(endpoint)` 阻塞接收并处理一条 init ready 消息（机制归 12）。本文档只列三个调用点：

- **Step 2 同步分支**（main.c:392）：`SF_SYNCH_BOOT` 服务立即同步等待。
- **Step 3 循环**（main.c:405）：`catch_boot_init_ready(ANY)` 收齐剩余消息。
- **VM 异步例外**（main.c:812-815）：`m.m_source != VM_PROC_NR` 才回 `OK` 回复——VM 的 init ready 是异步发的（同步回复会死锁，plan D-5），语义归 12。

### 2.6 get_work() 调用点（main.c:826-833）

```c
static void get_work(m_ptr, status_ptr)
{
    int r;
    if (OK != (r=sef_receive_status(ANY, m_ptr, status_ptr)))
        panic("sef_receive_status failed: %d", r);
}
```

主循环唯一的消息接收原语（`sef_receive_status(ANY, ...)`，ANY 表示接受任意来源）。**机制（含 `ipc_status` 的解析、`is_ipc_notify` 判定）归 06**；本文档只列调用点。`sef_receive_status` 签名归 19。

### 2.7 table.c：三张 boot 表（table.c:15-50）

`minix3/minix/servers/rs/table.c` 定义 RS 启动协议的三张静态表：

**boot_image_priv_table**（table.c:15-30，12 个系统服务 + NULL 哨兵；顺序即 boot 启动顺序）：

| endpoint | label | flags（priv.h:45-49） |
|----------|-------|----------------------|
| `RS_PROC_NR` | `"rs"` | `RSYS_F`（`SRV_F|ROOT_SYS_PROC`，root sys proc） |
| `VM_PROC_NR` | `"vm"` | `VM_F`（`SYS_PROC|VM_SYS_PROC`） |
| `PM_PROC_NR` | `"pm"` | `SRV_F`（`SYS_PROC|PREEMPTIBLE`） |
| `SCHED_PROC_NR` | `"sched"` | `SRV_F` |
| `VFS_PROC_NR` | `"vfs"` | `SRV_F` |
| `DS_PROC_NR` | `"ds"` | `SRV_F` |
| `TTY_PROC_NR` | `"tty"` | `SRV_F` |
| `MEM_PROC_NR` | `"memory"` | `SRV_F` |
| `MIB_PROC_NR` | `"mib"` | `SRV_F` |
| `PFS_PROC_NR` | `"pfs"` | `SRV_F` |
| `MFS_PROC_NR` | `"fs_imgrd"` | `SRV_F` |
| `INIT_PROC_NR` | `"init"` | `USR_F`（`BILLABLE|PREEMPTIBLE`） |
| `NULL_BOOT_NR` | `""` | 0（哨兵） |

**boot_image_sys_table**（table.c:33-42，6 个覆盖项 + DEFAULT 哨兵；`SRVR_SF = SF_CORE_SRV|SF_NEED_REPL`，`VM_SF = SRVR_SF`，`SRV_SF = SF_CORE_SRV`，基 flag 在 rs.h:191/195，派生常量在 const.h:65-68）：

| endpoint | flags |
|----------|-------|
| `RS_PROC_NR` | `SRVR_SF` |
| `VM_PROC_NR` | `VM_SF` |
| `PM_PROC_NR` | `SRVR_SF` |
| `SCHED_PROC_NR` | `SRVR_SF` |
| `VFS_PROC_NR` | `SRVR_SF` |
| `MFS_PROC_NR` | 0 |
| `DEFAULT_BOOT_NR` | `SRV_SF`（默认条目） |

**boot_image_dev_table**（table.c:45-50，2 个驱动 + DEFAULT 哨兵）：

| endpoint | dev_nr |
|----------|--------|
| `TTY_PROC_NR` | `TTY_MAJOR` |
| `MEM_PROC_NR` | `MEMORY_MAJOR` |
| `DEFAULT_BOOT_NR` | 0（默认条目） |

**表语义**：priv 表是 boot 主表（顺序 = 启动顺序，条目数必须与内核 boot_image 表系统服务数一致，§2.3.2）；sys 表只列**覆盖默认 sys 属性**的服务（未列出的服务用 `DEFAULT_BOOT_NR` 条目）；dev 表只列有主设备号的服务。这是 A-13 的静态化基础（§3.1）。

---

## 3. Rust 设计决策

> 每个决策必须可追溯到 §2 的 C 源码分析。Rust 代码位置只以函数/模块名引用（避免行号漂移）。

### 3.1 boot 表来源显式化：BootTables（对应 §2.3.1/§2.7，ARCH A-13）

**C**：`sys_getimage()`（main.c:196）把内核 `boot_image[]` 运行期拷贝到栈数组；priv/sys/dev 三表是 RS 自带的静态表（table.c）。**Rust**：

1. `sys_getimage()` 的拷贝结果建模为构造输入 `&[BootImage]`（`minix-types` 已有 `BootImage`，`os/libs/minix-types/src/types/boot.rs`），由 `main.rs` 在 `sys_getimage` 落地后注入；当前 `minix-sys` 为 stub（`os/libs/minix-sys/src/lib.rs` 标注 stub），生产路径以占位构造 + `validate()` 保证表一致性（对照 VM `BootParams::placeholder()` 同款策略）。
2. priv/sys/dev 三表**静态化**（A-13：`boot_image_priv/sys/dev` 三表在 minix-rs 侧可静态化，同 kernel 表）：`table.rs` 用 `&'static [BootImagePriv]` 等切片常量复刻 table.c 内容，编译期保证条目不漂移。

```rust
pub struct BootTables<'a> {
    pub image: &'a [BootImage],        // C: sys_getimage() 结果（运行期注入）
    pub priv_table: &'static [BootImagePriv],
    pub sys_table: &'static [BootImageSys],
    pub dev_table: &'static [BootImageDev],
}
```

理由：
- **可见性**：构造参数显式暴露启动依赖，`BootInit::new(tables)` 的签名即文档。
- **可测性**：测试构造任意表组合，无需伪造全局（对照 VM `BootParams::simple()`）。
- **哨兵类型化**：C 的 `NULL_BOOT_NR`/`DEFAULT_BOOT_NR`（const.h:61-62）是魔法哨兵值；Rust 用**切片长度 + 查找返回值**表达：priv 表无哨兵条目（遍历到结尾即失败），sys/dev 表查找失败返回默认条目（§3.2）。`NULL_BOOT_NR` 仅保留为常量对齐（99 文档），不参与逻辑。

### 3.2 boot_image_info_lookup 的 Result 化（对应 §2.4）

**C**：`panic("boot image table lookup failed")`（main.c:731）——image/priv 表未命中是启动协议错误；sys/dev 表未命中采用默认条目（main.c:753-762, 768-777）。**Rust**：

- image/priv 查找 → `Result<_, LookupError>`（`LookupError::{ImageTable, PrivTable}`），调用方（四步 boot）遇错即 `panic!` 或向上传播——**保留 fail-closed 语义**（A-12）。
- sys/dev 查找 → `Option` 语义：未命中返回默认条目（`DefaultSys`/`DefaultDev`），不报错。

```rust
pub enum LookupError { ImageTable, PrivTable }
pub fn lookup_image<'a>(image: &'a [BootImage], ep: Endpoint) -> Result<&'a BootImage, LookupError>;
pub fn lookup_priv<'a>(table: &'a [BootImagePriv], ep: Endpoint) -> Result<&'a BootImagePriv, LookupError>;
pub fn lookup_sys<'a>(table: &'a [BootImageSys], ep: Endpoint) -> &'a BootImageSys;   // DEFAULT 兜底
pub fn lookup_dev<'a>(table: &'a [BootImageDev], ep: Endpoint) -> &'a BootImageDev;   // DEFAULT 兜底
```

理由：C 的 panic 是"启动协议错误"的信号；Rust 用 `Result` 让错误显式化，四步 boot 顶层统一处理（A-12）。

### 3.3 SEF 回调注册表：SefCallbacks（对应 §2.2，ARCH A-7）

**C**：`sef_setcb_*` 全局函数指针 + `sef_startup()` 状态机（libsys/sef.c），回调体靠全局变量访问
`rproc[]`/`rupdate`。**Rust**：用户态 SEF 抽象（A-7：RS 先行实现并复用至 PM/VFS）建模为
**trait**（N5 修复，2026-08-16——原 fn 指针结构体无法携带服务器状态，12/18/06 的回调体
将拿不到 `&mut ServerState`；trait 化让回调成为状态机方法，`&mut self` 正是单线程用户态模型）：

```rust
pub trait SefCallbacks {
    fn init_fresh(&mut self, init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno>;
    fn init_restart(&mut self, init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno>; // → 18
    fn init_lu(&mut self, init_type: SefInitType, info: &SefInitInfo) -> Result<i32, Errno>;      // → 18
    fn init_response(&mut self, m: &Message) -> Result<i32, Errno>;  // → 12
    fn lu_response(&mut self, m: &Message) -> Result<i32, Errno>;    // → 12
    fn signal_handler(&mut self, signo: i32);                        // → 06
    fn signal_manager(&mut self, target: Endpoint, signo: i32) -> Result<i32, Errno>;  // → 06
}
```

- `SefInitType::{Fresh, Lu, Restart}` 对应 `SEF_INIT_FRESH=0/LU=1/RESTART=2`（sef.h:93-95）。
- `signal_manager` 的签名逐字对照 C 回调类型 `int(*)(endpoint_t target, int signo)`
  （sef.h:270）：`target` 是信号管理器目标端点、`signo` 是信号号；`target` 用 `Endpoint`
  newtype 而非裸 `i32`，两个参数在调用点不可互换（R26，todo §18）。
- **实现者是 `RsServer` 本身**：`impl SefCallbacks for RsServer`。`RsServer::init(init_type)` 对应
  `sef_startup()` 的分派（main.c:151）——按 `init_type` 路由到 `init_fresh`/`init_lu`/
  `init_restart`；`init_fresh` 即四步 boot（§3.4）。**消息接收/拦截机制归 12**
  （`do_init_ready`/`sef_cb_init_response`）；主循环的 RS_INIT 分支（12）直接调 trait 方法，
  回调体经 `&mut self` 拿到 `RsServer` 状态。12/18/06 未落地的 5 个方法在 `RsServer` 上
  fail-closed（`Err(ENOSYS)`），`signal_handler` 保留 `unimplemented!`
  （无 Result 通道，T7 门禁带 06 契约）。
- **与 VM 01 §3.4 的简化对齐**：VM 用 `rs_handshake()` 替代 SEF 框架（`02-stage-vm/01-vm-init-main.md` §3.4）；RS 是 SEF 的**提供方**（其余服务的 init 协议由 RS 实现），必须保留完整的回调集语义；trait 化去掉 C 的全局函数指针且不引入"注册值 + 回调体无法触达状态"的中间形态。

### 3.4 四步 boot 类型化：BootInit 状态机（对应 §2.3）

**C**：`sef_cb_init_fresh` 一个函数内顺序执行 4 步（main.c:158-494）。**Rust**：`BootInit` 结构体 + 单入口 `init_fresh()`，内部按 §2.3 顺序调用四个私有方法——**顺序不可重排由编排方法保证**：

```rust
pub struct BootInit<'a> {
    tables: BootTables<'a>,     // C: sys_getimage 拷贝 + 三静态表（§3.1）
    machine: Machine,           // C: machine（main.c:53 sys_getmachine，N3）
    rinit: RinitState,          // C: rinit.rproctab_gid（§2.3.1，消费在 12）
    slots: Vec<ServiceSlot>,    // C: rproc/rprocpub 表（字段语义归 02；本模块只做生命周期编排）
    shutting_down: bool,        // C: shutting_down（main.c:193）
    system_hz: u32,             // C: system_hz（main.c:181）
    nr_uncaught_init_srvs: usize, // C: nr_uncaught_init_srvs（main.c:349-406）
}
impl BootInit<'_> {
    pub fn init_fresh(&mut self, sys: &mut dyn KernelApi) -> Result<(), Errno> {
        self.step0_prepare(sys)?;      // main.c:178-237（前置：配置/HZ/grant/复位/计数核对/表重置）
        self.step1_set_attrs(sys)?;    // main.c:244-346
        self.step2_allow_run(sys)?;    // main.c:348-399
        self.step3_catch_init_ready(sys)?; // main.c:401-407
        self.step4_finish(sys)?;       // main.c:409-433
        Ok(())
    }
}
```

**外部 syscall 面**：`KernelApi` trait（§3.5）。四步内部只调用 trait 方法，生产实现最终接线 `minix-sys`（19），测试用 mock 记录调用序列。

> **T1 修复（2026-08-15）——状态 handover**：C 的 boot 状态就是运行期状态（`glo.h` 全局：
> `rproc[]`/`system_hz`/`shutting_down`/`rinit`，boot 后继续存活）。Rust 侧 `BootInit` 增加
> `into_state(self) -> ServerState<'a>`（**消费 self**，boot 后旧机器不可再改）；`RsServer` 持有
> `boot: Option<BootInit>` + `state: Option<ServerState>`，`init(Fresh)` 成功后将状态移交，
> `run()`/主循环（06）经单一访问器 `RsServer::state()` 读取 `table`/`system_hz`/`shutting_down`——
> 避免"5+ getter 垃圾场"方案。typestate（`BootInit<Fresh>` → `RsRunning`）记为 06 接线时的备选。

> **N3 修复（2026-08-16，todo §11）——machine 纳入 ServerState**：`ServerState` 增加
> `machine: Machine` 字段；`step0_prepare` 在启动期一次性 `sys.get_machine()?`
> （对齐 C main.c:53 的位置——`sef_local_startup` 之后、主循环之前），经 `into_state`
> 移交运行期。`check_request` 的 CPU 亲和解析（request.c:1286-1296，`RS_CPU_BSP`/
> 越界回退）读这个快照，而不是在主循环里临时查询（C 是一次性启动快照语义）。
> 对照 Redox：daemon 的 CPU 拓扑经 scheme 按需查询（`sysinfo`），无启动期全局快照；
> Minix RS 是启动期快照，Rust 显式建模为 `ServerState` 字段。

> **T6 修复（2026-08-15）——Step 2/3 fail-closed**：Step 2 对 `SF_SYNCH_BOOT` 服务不再计入
> `nr_uncaught_init_srvs`（C 是同步 `catch_boot_init_ready`，main.c:390-392；12 落地前显式
> `Err(ENOSYS)`）；Step 3 在计数 > 0 时显式 `Err(ENOSYS)`（C 阻塞接收 = fail-closed，main.c:401-407），
> 不再"假装收完"。boot 测试改走私有 step 方法直接驱动各步，并新增 SYNCH_BOOT/Step 3 fail-closed 断言。

### 3.5 外部 syscall 面：KernelApi trait（对应 §2.1/§2.3，外部契约归 19）

C 的 `sys_getmachine`/`sys_getinfo`/`sys_privctl`/`sys_getpriv`/`sys_setalarm`/`getnpid`/`sched_init_proc`/`srv_fork` 等（§2 各调用点）是 libsys 自由函数。Rust 侧**本模块不直接调用 `minix-sys`**（其 stub 未实现，`os/libs/minix-sys/src/lib.rs`），而是定义窄接口：

```rust
pub trait KernelApi {
    fn get_machine(&mut self) -> Result<Machine, Errno>;
    fn get_hz(&mut self) -> Result<u32, Errno>;
    fn privctl(&mut self, proc: Endpoint, op: PrivCtlOp, priv_: Option<&Priv>) -> Result<(), Errno>;
    fn getpriv(&mut self, proc: Endpoint) -> Result<Priv, Errno>;
    fn sched_init_proc(&mut self, cfg: &SchedulerConfig) -> Result<Endpoint, Errno>;
    fn getnpid(&mut self, proc: Endpoint) -> Result<i32, Errno>;
    fn setalarm(&mut self, delay_ticks: u32) -> Result<(), Errno>;
}
```

> **S4 修复（2026-08-15）**：`sched_init_proc` 签名从 `(proc: Endpoint) -> Result<(), Errno>` 改为
> `(cfg: &SchedulerConfig) -> Result<Endpoint, Errno>` —— C 的 `sched_start`（sched_start.c:37-80）
> 需要 scheduler/priority/quantum/cpu 全部四个参数，且回写 `*newscheduler_e`（可能被转发到别的
> 调度器）。`SchedulerConfig`（sched.rs）携带全部参数；boot Step 2 用 `SchedulerConfig::boot_defaults`
> 构造（`SRV_SCH=KERNEL`/`SRV_Q=USER_Q=7`/`SRV_QT=USER_QUANTUM=200`，priv.h:88,93,98 + config.h:69,74）。
> NONE 调度器短路（sched_start.c:45-47）在纯函数 `sched::sched_decision` 内实现，不触内核
> （T5 修复，2026-08-16）：`sched_init_proc` 的"决策 + 执行"拆分为
> `sched_decision(cfg, is_sys_proc) -> SchedAction::{Skip, Start(&cfg)}`，shell（boot Step 2 /
> 19 接线）执行 `Start` → `sys.sched_init_proc(cfg)` 并取得 `*newscheduler_e`。

理由：
- **依赖倒置**：boot 编排逻辑与 syscall 实现解耦；`minix-sys` 落地后实现 `KernelApi`（接线归 19），测试用 `MockKernelApi`。
- **T5 注入边界（2026-08-16）**：`KernelApi` 只在 shell 出现——boot 四步（本模块）、
  `RsServer.kernel` 持有者（lib.rs）与 19 接线层。纯决策模块（access/ipc_mask/sched/ready/
  recovery/monitor）不 import `KernelApi`：查询结果（`getnuid`/`getpriv`）由 shell 注入，
  命令（`privctl`/`sched_init_proc`/`setalarm`）由 shell 执行（monitor 模式，todo §13）。
  测试 mock 收敛为单一共享 `testutil::MockKernelApi`（E1，todo §13）。
- **fail-closed（T2 修复）**：trait 无默认实现（编译期强制每个 impl 全量实现）；生产占位
  `UnimplementedKernelApi` 每个方法返回 `Err(Errno::ENOSYS)`，**不 panic**——RS 是 root system
  process，panic = 整机不可用（内核不重启 RS，`RSYS_F`）；返回 `Err` 让缺口在调用点可见且进程存活。
  入口 `main.rs` 对 boot 失败显式 panic（C 的 boot 错误同样 `panic`，main.c:226），不吞错误。
- 每个方法对应 C 调用点：`get_machine`（main.c:53）、`get_hz`（main.c:181）、`privctl`（main.c:287/379/478/485）、`getpriv`（main.c:294）、`sched_init_proc`（main.c:376/482）、`getnpid`（main.c:426）、`setalarm`（main.c:433）。

### 3.6 USE_LIVEUPDATE：cargo feature（对应 §2.3.7，ARCH A-11）

**C**：`#if USE_LIVEUPDATE`（main.c:436-491，`USE_LIVEUPDATE` 默认 yes，`share/mk/bsd.own.mk:1499`）。**Rust**：cargo feature `live-update`（默认关）：

```rust
#[cfg(feature = "live-update")]
fn self_update(&mut self, sys: &mut dyn KernelApi) -> Result<(), Errno> { ... }
```

- 默认关的理由：RS 自升级依赖 `srv_fork`/`update_service`/`cleanup_service`/`vm_memctl` 等机制，其语义归 10/15/16/18；在机制落地前，feature 关闭时该流程**不编译**（fail-closed，不产生半吊子代码路径）。
- 打开 feature 后，`self_update` 内部按 §2.3.7 调用链编排（`clone_slot`/`srv_fork`/`update_service`/`cpf_reload`/`cleanup_service`/`vm_memctl pin` / `privctl SET_SYS`+`sched_init_proc`+`YIELD`），各步骤机制标注归属 10/15/16/18。

### 3.7 rinit grant 创建点的表达（对应 §2.3.1，D-16）

`rinit.rproctab_gid = cpf_grant_direct(...)`（main.c:185）是**创建点**（本文档语义），消费点在 12（`init_service` 把 gid 放入 RS_INIT 消息）。Rust 中 `RinitState` 字段 `rproctab_gid: Option<GrantId>`（当前以 `Option<u32>` 表达，`GrantId` 类型归 99/19 落地后替换）：

- 创建：`step0_prepare` 阶段通过 `KernelApi` 的 grant 接口设置（`grant_direct` 归 19；当前未接线时以 `None` + 文档标注 DEFERRED）。
- 消费：12 的 `init_service` 读取该字段（12 文档实现）。
- 用 `Option<GrantId>` 表达"boot 已创建 / 未创建"状态，杜绝 C 的 `GRANT_VALID()` 宏手动判定。

### 3.8 主循环 dispatch 骨架的类型化（对应 §2.1，细节归 06）

`main()` 的分类逻辑（§2.1）在 Rust 中建模为纯函数 + 归属表：

```rust
pub enum DispatchKind {
    ClockNotify,          // C: case CLOCK → do_period（07）
    HeartbeatNotify(Endpoint), // C: default 心跳（07）
    InitReady,            // C: RS_INIT → do_init_ready（12）
    LuPrepareReady,       // C: RS_LU_PREPARE → do_upd_ready（12/16）
    Request(i32),         // C: RS_* → do_*（13/14/16）
}
pub fn classify(ipc_status: &IpcStatus, who_p: Endpoint, call_nr: i32) -> DispatchKind;
```

- `is_ipc_notify`（com.h:92，`IPC_STATUS_CALL(status) == NOTIFY`）在 Rust 中由 `IpcStatus` 的位解析表达（对照 VM `os/servers/vm/src/ipc/transport.rs` 的 `IpcStatus`）。
- **15 个请求 handler 的归属表**：`RS_UP..RS_LOOKUP` → 13/14/16，`RS_INIT`/`RS_LU_PREPARE` → 12（§1.4 表）。handler 本体未实现前，`dispatch_request` 对已定义归属的调用号返回 `DispatchResult(ENOSYS)`（newtype 包装 reply 值，对应 C default 分支 main.c:118-121 的 fail-closed 语义）。
- 主循环的 `reply`/`EDONTREPLY`/`late_reply` 机制归 06，本文档不实现。

---

## 4. 实现详解

> 每个实现对应 §3 的设计决策。Rust 代码位置以函数名引用。

### 4.1 `os/servers/rs/src/main.rs`（对应 §3.3/§3.8）

```rust
fn main() {
    #[cfg(not(test))]
    {
        use minix_rs::{RsServer, SefInitType, boot::BootTables};

        // C: main.c:51 sef_local_startup() —— SEF 回调集是 RsServer 实现的
        // trait（N5，§3.3），无独立注册值要构造。

        // C: main.c:53 sys_getmachine() —— 机器信息（KernelApi 接线归 19）
        // C: main.c:196 sys_getimage() —— boot 映像（§3.1，当前占位注入）
        let tables = BootTables::placeholder();   // 生产替换点：sys_getimage 落地后

        let mut server = RsServer::new(tables);

        // C: sef_startup()→sef_cb_init_fresh()——main.c:151,158-494（§3.4 四步 boot）。
        // KernelApi 生产接线 DEFERRED（19）；此前 fresh boot 路径 fail-closed（Err(ENOSYS)）。
        // C 视 boot 失败为致命（main.c:226 panic）——boot 未完成不得进入主循环。
        if let Err(e) = server.init(SefInitType::Fresh) {
            panic!(
                "RS boot failed: {e:?} (kernel API wiring pending — 19-rs-external-interfaces.md)"
            );
        }

        server.run();                             // C: main loop——main.c:50-131（骨架，§3.8；细节归 06）
    }
}
```

- `BootTables::placeholder()`（对照 VM `BootParams::placeholder()`）：使用 `table.rs` 静态三表 + 一个最小合法 `BootImage` 数组（含 RS/VM/PM 等条目），保证生产二进制有合法的 boot 输入；`sys_getimage` 落地后由 `main.rs` 注入真实拷贝。
- `RsServer::run()` 的主循环骨架见 §4.6。

### 4.2 `os/servers/rs/src/lib.rs`（对应 §3.4，no_std 单线程模型）

```rust
#![cfg_attr(not(test), no_std)]
extern crate alloc;

pub mod table;      // §4.3：boot 三表（A-13）
pub mod boot;       // §4.4：BootInit 四步状态机
pub mod sef;        // §4.5：SEF 回调注册表（A-7）
pub mod dispatch;   // §4.6：主循环分类骨架
// 其余 pub mod（access/exec/ipc_mask/live_update/monitor/privilege/process_table/
// publish/query/ready/recovery/request/sched/self_lifecycle/service_create/
// service_slot/slot/state_data）为 02-19 对应模块的 forward-declare，共 22 个。

pub struct RsServer {
    boot: Option<boot::BootInit<'static>>,  // init(Fresh) 成功后消费（T1）
    state: Option<ServerState<'static>>,    // C 全局：rproc[]/system_hz/shutting_down/rinit
    kernel: Box<dyn KernelApi>,             // 生产接线归 19；默认 fail-closed
}
impl RsServer {
    pub fn new(tables: BootTables<'static>) -> Self { ... }   // kernel = UnimplementedKernelApi
    pub fn with_kernel(tables: BootTables<'static>, kernel: Box<dyn KernelApi>) -> Self { ... }
    pub fn init(&mut self, init_type: SefInitType) -> Result<i32, Errno> { ... }  // §3.4 分派
    pub fn state(&self) -> Option<&ServerState<'static>> { ... }  // T1 handover 访问器
    pub fn run(&mut self) -> ! { ... }   // 主循环骨架（§3.8）
}
```

- **单线程模型**：RS 是用户态服务器（执行模型：单线程事件循环），`!Send`/`!Sync` 合理，无跨 CPU 共享（`CLAUDE.md` 执行模型约束）。`BootInit` 内用 `Vec`（alloc）而非 BSS 全局，避免 C 的 `rproc[]` 全局表带来的隐式依赖（A-3 关联）。
- 模块划分理由：`table.rs`（数据）、`boot.rs`（启动编排）、`sef.rs`（注册表）、`dispatch.rs`（分类）各一个语义单元，与 plan §2 的 01 归属一致（Rust 模块列：`main.rs`、`lib.rs`——子模块均为 `lib.rs` 内部结构）。

### 4.3 `os/servers/rs/src/table.rs`（对应 §2.7，A-13）

模块导出三个结构 + 三张静态表：

- `BootImagePriv { endpoint, label, flags }`——C `struct boot_image_priv`（type.h），flags 对应 `RSYS_F`/`VM_F`/`SRV_F`/`USR_F`（`minix3/minix/include/minix/priv.h:45-49`）。
- `BootImageSys { endpoint, flags }` / `BootImageDev { endpoint, dev_nr }`——sys/dev 表条目。
- 静态表常量：`BOOT_IMAGE_PRIV_TABLE`（12 项，对应 table.c:17-28，无哨兵——遍历即终止）、`BOOT_IMAGE_SYS_TABLE`（6 项 + `DEFAULT_SYS` 默认条目，对应 table.c:35-41）、`BOOT_IMAGE_DEV_TABLE`（2 项 + `DEFAULT_DEV`，对应 table.c:47-49）。

哨兵处理：C 的 `NULL_BOOT_NR`/`DEFAULT_BOOT_NR` 不再作为表内条目；`lookup_priv` 遍历失败即 `Err(LookupError::PrivTable)`，`lookup_sys`/`lookup_dev` 未命中返回默认条目（§3.2）。常量 `NULL_BOOT_NR`/`DEFAULT_BOOT_NR` 以 `pub const` 保留对齐（99）。

### 4.4 `os/servers/rs/src/boot.rs`（对应 §2.3-§2.5，四步状态机）

- `BootTables<'a>`：§3.1。
- `LookupError` + 四个查找函数：§3.2（`lookup_image` 对应 main.c:723-733，`lookup_priv` 对应 main.c:738-748，`lookup_sys`/`lookup_dev` 对应 main.c:753-778）。
- `RinitState { rproctab_gid: Option<GrantId> }`：§3.7（创建点 `step0_prepare`，消费在 12）。
- `ServiceSlot`：C `rproc` 的**最小生命周期视图**（`endpoint`/`label`/`sys_flags`/`dev_nr`/`pid`/`in_use`），完整字段语义归 02；本模块只承载四步 boot 需要的属性。
- `BootInit::init_fresh()`：四步编排（§3.4）。各步要点：
  - **与 C 的步骤数差异（CSSCM）**：C 的 `sef_cb_init_fresh` 是"前置 + Step 1~4"（前置在 §2.3.1，未编号）；Rust 显式命名为 `step0_prepare` + `step1_set_attrs`~`step4_finish` 共 5 个私有方法。机制步骤一一对应（无增减），差异仅为命名显式化（架构演进，无 C 行为偏移）。
  - `step0_prepare`：`env_parse` 配置注入（参数）、`get_hz`、grant 创建点、`RUPDATE_INIT` 等价（`RupdateState::default()`）、`sys_getimage` 等价（`tables.image` 已注入）→ 计数核对（`validate_tables`，对应 main.c:225-227）→ 表重置（`slots` 重建，对应 main.c:230-237）。
  - `step1_set_attrs`：遍历 priv 表（对应 main.c:244），每项 `lookup_image/sys/dev` + 填充 `ServiceSlot` 属性；`endpoint == RS || endpoint == VM` 跳过 `privctl(SET_SYS)`（对应 main.c:285-291，RS/VM 例外）；其余经 `privctl(SET_SYS)` + `getpriv`。priv/send mask/call mask 的**完整构造**归 03/05，本模块只做调用编排。
  - `step2_allow_run`：遍历 priv 表；RS/VM → `init_service`（12 语义，本模块经 `KernelApi`/回调占位并标注 DEFERRED）；普通服务 → `sched_init_proc` + `privctl(ALLOW)` + `init_service` + `SF_SYNCH_BOOT` 分支（同步 catch / 累积计数，对应 main.c:375-398）。
  - `step3_catch_init_ready`：循环调用 `catch_boot_init_ready`（12 机制；本模块以计数 + 回调占位表达，DEFERRED 标注）。
  - `step4_finish`：逐服务 `getnpid` 写 `pid`（对应 main.c:413-431）+ `setalarm(RS_DELTA_T)`（对应 main.c:433）。
  - `self_update`（`#[cfg(feature = "live-update")]`）：§3.6 调用链编排（clone_slot/srv_fork/update_service/cpf_reload/cleanup_service/vm_memctl/privctl YIELD，各步骤归属 10/15/16/18）。

### 4.5 `os/servers/rs/src/sef.rs`（对应 §2.2，A-7）

- `SefInitType::{Fresh, Lu, Restart}` + `SefInitInfo`（对应 `sef_init_info_t`，sef.h:53；字段语义在 12 展开）。
- `SefCallbacks` 7 方法 trait（N5，§3.3），由 `RsServer` 实现（lib.rs）；不再有
  `local_startup()`/`startup()`——`RsServer::init(init_type)` 对应 `sef_startup()` 的分派：
  启动期从 IPC 接收 `RS_INIT` 后按 `info.init_type` 调 `init_fresh`/`init_lu`/
  `init_restart`。**消息接收机制归 12**（RS 主循环的 `RS_INIT` 分支）；本模块只定义分派类型。

### 4.6 `os/servers/rs/src/dispatch.rs`（对应 §2.1 分类骨架）

- `IpcStatus`：`ipc_status` 的位解析（`IPC_STATUS_CALL`，com.h:92），`is_notify()` 方法对应 `is_ipc_notify`。
- `DispatchKind` 枚举 + `classify()`：§3.8。`CLOCK` 判定用 `Endpoint::CLOCK` 常量（`minix-types`，对应 C `case CLOCK` main.c:82）。
- `dispatch_request(call_nr) -> DispatchResult`（`pub struct DispatchResult(pub i32)`，即 C 的 reply 值）：15 个调用号 → handler 归属表（`RS_UP → 13` 等）；未实现 handler 返回 `DispatchResult(ENOSYS)`（fail-closed，对应 C default 分支 main.c:118-121）。
- 归属表以 `match` 注释标注（`// → 13`），机制文档 13/14/16 落地后逐项替换为真实 handler 调用。

---

## 5. 测试要点

> 基线：`cargo test -p minix-rs --lib` = **208 passed / 0 failed**（2026-08-16 实测，全 crate）。01 范围四模块 29 项（boot 18 + table 3 + sef 1 + dispatch 7）。测试覆盖四步 boot 顺序、表查找、SEF 回调 fail-closed、dispatch 分类。

### 5.1 table.rs 测试（§2.7，A-13）

| 测试 | 覆盖 |
|------|------|
| `test_priv_table_matches_c` | priv 表 12 项 + 顺序与 table.c:17-28 一致（RS/VM/PM/SCHED/VFS/DS/TTY/MEM/MIB/PFS/MFS/INIT）+ flags 类别 |
| `test_sys_table_has_default` | sys 表 6 项 + 默认条目与 table.c:35-41 一致 |
| `test_dev_table_has_default` | dev 表 2 项 + 默认条目与 table.c:47-49 一致 |

### 5.2 boot.rs 测试（§2.3-§2.5，四步状态机 + 四表查找）

| 测试 | 覆盖 |
|------|------|
| `test_init_fresh_step_order` | `MockKernelApi` 记录调用序列：step1 privctl(SetSys)×10（RS/VM 跳过）→ step2 sched×10+Allow×10 → step4 getnpid×12+setalarm(100)；顺序与 C 一致（main.c:158-433） |
| `test_step1_skips_privctl_for_rs_vm` | RS/VM 跳过 `privctl(SetSys)`（main.c:285-291 例外） |
| `test_validate_tables_mismatch` | image 表与 priv 表系统服务数不一致 → Err（对应 main.c:225-227 panic） |
| `test_validate_tables_rejects_proc_nr_endpoint_mismatch` | 单行 `proc_nr != endpoint.slot()` → Err（R17，fail-closed） |
| `test_boot_img_truncates_long_name` | >16 字节名称钳制到字段（R17，不再 const 越界） |
| `test_lookup_image_not_found` | `lookup_image` 未命中 → `LookupError::ImageTable`（对应 main.c:731 panic） |
| `test_lookup_priv_found` / `test_lookup_priv_not_found` | `lookup_priv` 命中 / 未命中 → `LookupError::PrivTable`（对应 main.c:746） |
| `test_lookup_sys_default_fallback` / `test_lookup_dev_default_fallback` | sys/dev 未命中返回默认条目（对应 main.c:753-762,768-777） |
| `test_placeholder_tables_valid` | `BootTables::placeholder()` 通过 `validate_tables()`（对应 main.c:200-237） |
| `test_unimplemented_kernel_api_fails_closed` | 生产占位 `KernelApi` 全接口 fail-closed（`Err(ENOSYS)`，T2：RS 是根系统进程，panic = 系统级 outage） |
| `test_init_fresh_populates_table` | Step 1 后 12 个 boot 服务占 slot 0..11 且 `IN_USE|ACTIVE`，A-4 索引命中；非 boot slot 保持空闲（对应 main.c:244-346） |
| `test_step4_sets_pid` | Step 4 每个 boot slot 携带 `getnpid` 返回的 pid（对应 main.c:426；mock 返回 100） |
| `test_step3_fails_closed_when_init_ready_pending` | T6：有未收 init-ready 时 Step 3 fail-closed（`Err(ENOSYS)`，对应 main.c:401-407 的阻塞 receive 语义） |
| `test_step2_synch_boot_fails_closed` | T6：`SF_SYNCH_BOOT` 服务同步 catch 未接线时 fail-closed（对应 main.c:390-392），不得静默跳过 sync |
| `test_rs_server_handover_after_fresh_init` | T1：fresh boot 完成后运行时状态归 server 所有（`state()` 可达 table/hz/shutting_down），machine 快照随 boot→run 交接存活；boot 机器被消费（无双重所有权） |
| `test_boot_slot_populates_s2_fields` | S2：boot slot 携带 cmd/args/argc/vm_call_mask/scheduler/priority/quantum/alive_tm（对应 main.c:308-333，07/09/10 依赖） |

### 5.3 sef.rs / dispatch.rs 测试（§2.2/§2.1，A-7）

| 测试 | 覆盖 |
|------|------|
| `test_deferred_callbacks_fail_closed` | 12/18/06 未接线的回调（`init_restart`/`init_lu`/`init_response`/`lu_response`/`signal_manager`）全部 fail-closed（`Err(ENOSYS)`，T2） |
| `test_classify_clock_notify` | `is_notify` + `CLOCK` → `ClockNotify`（对应 main.c:80-83） |
| `test_classify_heartbeat_notify` | 非 CLOCK 通知 → `HeartbeatNotify`（对应 main.c:85-91） |
| `test_classify_ready` | `RS_INIT`/`RS_LU_PREPARE` → `InitReady`/`LuPrepareReady`（对应 main.c:116-117） |
| `test_classify_request` / `test_classify_request_unknown` | 请求分类；未知调用号 → `dispatch_request` 返回 `ENOSYS`（对应 main.c:118-121） |
| `test_dispatch_result_reply_suppression` | `DispatchResult(EDONTREPLY)` 抑制 reply 路径（对应 main.c:124-129） |
| `test_rs_constants_match_c` | 15 个 RS 消息常量与 com.h:465-482 一致 |

### 5.4 测试总数

`cargo test -p minix-rs --lib` 实测 **208 passed / 0 failed**（2026-08-16）。全部测试可 grep 验证：`rg "fn test_" os/servers/rs/src/` = 209 处（含非测试方法 `CallMask::test_bit` 1 处，privilege.rs:205；实际测试 208）。01 范围四模块共 29 项：boot.rs 18、table.rs 3、sef.rs 1、dispatch.rs 7。

## 6. 过渡

本文档回答了 RS 启动链的第一个问题：**main() → sef_local_startup() → sef_cb_init_fresh() 四步 boot → 主循环入口**。

- 启动链的下一步：进程模型——`02-rs-process-table.md`（`rproc`/`rprocpub` 结构语义、`rproc_ptr` 快速索引、`rs_isokendpt`，主循环 caller 验证依赖它）。
- Step 1 的机制：`03-rs-privilege.md`（priv 结构 + `sys_privctl` 每操作效果）、`05-rs-ipc-sendmask.md`（`fill_send_mask`）；`04-rs-access-control.md` 是运行时请求的权限面（与 boot 相对）。
- Step 2/3 的机制：`12-rs-init-run.md`（`init_service`/`catch_boot_init_ready`/`do_init_ready`、`rproctab_gid` 消费点）。
- Step 4 的机制：`07-rs-period-heartbeat.md`（`sys_setalarm` 之后的周期检查与心跳）。
- 主循环：`06-rs-main-loop.md`（`get_work`/`reply`/`EDONTREPLY`/`rs_idle_period` 全量语义）。
- RS 自升级：`18-rs-self-lifecycle.md`（USE_LIVEUPDATE 流程细节）、`16-rs-live-update.md`（update_service 状态机）。
- 外部接口：`19-rs-external-interfaces.md`（`sys_getmachine`/`sys_getimage`/`sys_privctl`/`getnpid` 等签名）。

阅读顺序建议：00 → 01 → 02 → 03 → 05 → 12 → 06 → 07 → 08 → ...（boot 主线优先，04/06 为运行时面）。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/00-rs-overview.md` — RS 总览与启动主线图
- `notes/rewrite/fork-syscall-rewrite/03-stage-rs/plan.md` §1.2/§3.1/§4 A-7/A-11/A-13/§5.3 — 本文档的写作契约与覆盖基线
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md` — 同流程先例（VM 启动链，BootParams/占位注入策略）
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/09-vm-boot-protocol.md` — 内核侧 boot 链（RS 启动的前置）
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/22-privilege.md`、`23-ipc-filter.md` — kernel 侧 priv/IPC filter 语义（Step 1 的 `sys_privctl`/send mask 依赖）
- `minix3/minix/servers/rs/main.c` — 本文档 ground truth
- `minix3/minix/servers/rs/table.c` — boot 三表 ground truth
- `minix3/minix/include/minix/sef.h` — SEF 回调/类型定义（`SEF_INIT_*`/`sef_init_info_t`）
- `minix3/minix/include/minix/rs.h`、`minix3/minix/include/minix/priv.h` — `SF_*`/priv flags（`SRV_F`/`RSYS_F`/`VM_F`/`USR_F`）
- `os/servers/rs/src/` — Rust 实现（本文档对应 `main.rs`/`lib.rs` + `table`/`boot`/`sef`/`dispatch` 子模块）
- `os/libs/minix-sys/src/lib.rs` — syscall 库（stub，`KernelApi` 生产接线归 19）
