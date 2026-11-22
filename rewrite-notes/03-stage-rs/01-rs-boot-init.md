# 01-rs-boot-init: 启动入口与初始化骨架

> **定位**: 阶段 1 — RS 启动入口与 boot 初始化机制。
> **源码**: `minix3/minix/servers/rs/main.c`（834 行）、`minix3/minix/servers/rs/table.c`（50 行）；SEF 库位于 `minix3/minix/lib/libsys/sef*.c`
> **Rust 模块**: `os/servers/rs/src/main.rs`、`os/servers/rs/src/lib.rs`（含 `table`/`boot`/`sef` 子模块；主循环 `dispatch` 归 06）
> **前置**: `rewrite-notes/03-stage-rs/00-rs-overview.md`、`rewrite-notes/01-stage-kernel/09-vm-boot-protocol.md`（boot 链）
> **覆盖**: `main()`、`sef_local_startup()` 的 7 个回调注册、`sef_cb_init_fresh()` 四步 boot、`boot_image_*` 三表与查找、boot 尾部自升级的结果。机制细节归 02/03/05/12/16/18/20。

---

## 1. 概念：引导自举——RS 如何"在别人依赖它之前，先把自己启动好"

RS 是 boot 映像里的普通用户进程，却要在 PM/VFS/SCHED/DS 等成员运行之前给它们授权、放行、收 ready。它自己启动时没有进程表、没有权限表、也没有可用的服务描述；这一章先说明这个自举矛盾，以及 Minix3 用哪四步把它拆开。

### 1.0 章节引言

本章建立 RS 启动链的概念模型：RS 是"给 boot 成员授权放行、按需读盘加载非 boot 服务"的角色（`00-rs-overview.md`），但它自己也是 boot 映像中的一员——和它管的 PM/VFS 一样，它的镜像也是由 VM 装载的。它面临一个**先有鸡还是先有蛋**的问题——RS 要管理所有服务的生命周期，但启动时它自己也只是刚刚被 VM 装载、被内核放行的一个普通用户进程。

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

先说清这是哪一张序。`minix3/minix/kernel/table.c` 第 44 行的 `struct boot_image image[NR_BOOT_PROCS]` 是 **登记序（模块摆放序）**：它把 DS 摆在第 1 个用户服务、RS 紧随其后（第 2），这张序由 GRUB 交来的模块顺序硬性决定。内核表头注释把数组序说成 NOTIFY 投递优先级的意图声明；实际的 NOTIFY 选择序以 kernel 侧代码为准，本文不展开。登记序跟“谁先启动运行”是两码事：真正决定谁先跑的是 `minix3/minix/kernel/main.c:kmain` 的 **可调度序**——除内核任务外，第一个能运行的用户服务是 **VM**，RS 作为 root system process 紧随其后（其余进程连 RS 自己都被 `RTS_VMINHIBIT` 抑制，等 VM 建好页表才放行）。RS 的独特地位来自它是“给其余 boot 成员授权放行、按需读盘加载非 boot 服务”的那个角色——PM/VFS/SCHED/DS 等确实都依赖它拿运行许可与初始化——而不是来自它在数组里排第几。但反过来，RS 启动时自己什么设施都没有：

```
RS 需要管理服务生命周期 → 服务需要 RS 允许才能运行 → RS 必须先启动
RS 启动需要知道 boot 映像 → boot 映像在内核 → RS 要 sys_getimage 拷贝
RS 要让服务运行 → 服务需要 priv/调度/初始许可 → 这些都是 kernel/VM/sched 的机制
```

Minix3 的解法是**分阶段初始化**：`sef_cb_init_fresh()`（`main.c:158-494`）把 boot 拆成 4 步，每一步只依赖前一步已经建立的设施：

1. **Step 1（`main.c:244-346`，四步注释在 main.c:239-243）——建立全表属性**：为 boot 映像中的每个服务在 RS 本地进程表中建立槽位，并准备好 priv/sys/dev 三方面属性（priv 结构、send mask、调度参数、命令串）。此时它把非 RS/VM 服务的 priv 结构提交内核（`SYS_PRIV_SET_SYS` + `sys_getpriv`），RS/VM 跳过 `SET_SYS`；sys/dev/命令/调度属性在本地准备。**Step 1 不放行**。
2. **Step 2（`main.c:348-399`）——允许运行**：此时 `SYS_PROC` 服务的属性已就绪，RS 才通过 `sched_init_proc` + `sys_privctl(SYS_PRIV_ALLOW)` 让它们获得调度与运行许可，并发送 `RS_INIT` 初始化消息（RS/VM 例外：已在运行，直接 `init_service` 自模拟；INIT 是 `USR_F`，不参加 RS_INIT 握手）。
3. **Step 3（`main.c:401-407`）——收齐初始化完成**：收齐 Step 2 未同步收走的 `RS_INIT` ready（`SF_SYNCH_BOOT` 服务已在 Step 2 立即收；其余累积计数）。只覆盖 `SYS_PROC` 服务；INIT 是 `USR_F`，不参加 RS_INIT 握手。
4. **Step 4（`main.c:409-433`）——补全协作信息**：向 PM 查询每个服务的 pid（`getnpid`），并设置周期检查闹钟（`sys_setalarm(RS_DELTA_T)`），之后进入主循环。

这个"先本地建表、再统一放行、再收齐确认"的顺序是本文档后半部分所有 C 源码分析的骨架。

### 1.2 SEF 注册与四步 boot 的边界

RS 的入口不是裸写 `main()`：它先通过 `sef_local_startup()` 把 7 个生命周期回调注册给 SEF。RS 是 `ROOT_SYS_PROC`，`sef_startup()` 走 `do_sef_rs_init()` 本地合成 fresh 信息并调用 `sef_cb_init_fresh()`，不等待自己发来的 `RS_INIT`；其他服务才走接收 `RS_INIT` 的路径。本文只讲 **RS 注册了哪些回调、`sef_cb_init_fresh()` 的四步做了什么**；SEF 的拦截循环、消息状态机和 LU 状态机归 `20-rs-sef-framework.md`，主循环分类归 `06-rs-main-loop.md`。

#### 最小示例

看 RS 注册了哪 7 个回调：

```bash
$ grep -n 'sef_setcb_' minix3/minix/servers/rs/main.c
139:  sef_setcb_init_fresh(sef_cb_init_fresh);
140:  sef_setcb_init_restart(sef_cb_init_restart);
141:  sef_setcb_init_lu(sef_cb_init_lu);
144:  sef_setcb_init_response(sef_cb_init_response);
145:  sef_setcb_lu_response(sef_cb_lu_response);
148:  sef_setcb_signal_handler(sef_cb_signal_handler);
149:  sef_setcb_signal_manager(sef_cb_signal_manager);
```

#### 失败对照

如果 `sef_setcb_init_fresh` 没有注册，SEF 的 fresh 分派就没有目标，RS 的 `do_sef_rs_init()` 走到空回调或默认失败；四步 boot 根本不会开始。

### 1.3 本章回顾

#### 小结

- RS 自己是 boot 映像中的普通用户进程，却要在其他服务运行前给它们授权放行。
- `sef_cb_init_fresh()` 用四步切断自举环：Step 1 本地属性、Step 2 放行、Step 3 收齐 ready、Step 4 pid/alarm。
- SEF 只负责把 `RS_INIT` 分派到 fresh/restart/LU 回调；框架本体不在本篇。
- 主循环的分类与 handler 机制归 06/12/13/14/16。

#### 术语

- **boot 自举**：RS 在自己还没有完整运行环境时，按依赖顺序建立管理其他服务所需的状态。
- **`sef_cb_init_fresh`**：SEF 首次启动回调，RS 的四步 boot 都在这里。
- **boot 映像表**：内核交给 RS 的 `boot_image[]`，描述每个 boot 服务的端点、名称与模块位置。
- **Step 1/2/3/4**：本地属性、放行、收齐 ready、pid+alarm 四步。
- **`RS_INIT`**：服务初始化消息；boot 期用于放行后确认，运行期用于 RS_UP 的 ready 握手。

## 2. C 源码分析

> 本文档 ground truth 为 `minix3/minix/servers/rs/main.c` 与 `minix3/minix/servers/rs/table.c`。所有行号以 grep 实证为准。
>

第 1 章给出了四步依赖。本章沿 Minix3 的 `main()`、`sef_local_startup()`、`sef_cb_init_fresh()`、`boot_image_info_lookup()` 与 `table.c` 走一遍，重点看每一步的 C 证据与顺序理由。

### 2.0 本章要回答的问题

读者需要能回答：SEF 在 `main()` 里注册了哪些回调；`sef_cb_init_fresh()` 的四步各做什么；`boot_image_priv/sys/dev` 三张表怎样与内核 `boot_image[]` 对齐；为什么 Step 1 不能提前放行。

#### 最小示例

```bash
$ grep -n 'int main(void)\|static void sef_local_startup\|static int sef_cb_init_fresh' minix3/minix/servers/rs/main.c
38:int main(void)
136:static void sef_local_startup(void)
158:static int sef_cb_init_fresh(int type, sef_init_info_t *info)
```

#### 失败对照

如果跳过 Step 1 直接放行，`sys_privctl(SYS_PRIV_ALLOW)` 放行的槽还没有 priv/send-mask/调度参数；服务在第一次 IPC 或调度时会被内核拒绝。所以“先本地建表、再统一放行”不是风格选择，而是依赖顺序。

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
  sef_local_startup();
  
  if (OK != (s=sys_getmachine(&machine)))
	  panic("couldn't get machine info: %d", s);

  /* Main loop - get work and do it, forever. */         
  while (TRUE) {
      /* Perform sensitive background operations when RS is idle. */
      rs_idle_period();

      /* Wait for request message. */
      get_work(&m, &ipc_status);
      who_e = m.m_source;
      if(rs_isokendpt(who_e, &who_p) != OK) {
          panic("message from bogus source: %d", who_e);
      }
      call_nr = m.m_type;

      if (is_ipc_notify(ipc_status)) {
          switch (who_p) {
          case CLOCK:
	      do_period(&m);
	      continue;
	  default:				/* 心跳通知 */
	      if (rproc_ptr[who_p] != NULL) {
		  rproc_ptr[who_p]->r_alive_tm = m.m_notify.timestamp;
	      } else {
		  printf("RS: warning: got unexpected notify message from %d\n",
		      m.m_source);
	      }
	  }
      }
      else {
          switch(call_nr) {
          /* User requests —— 生命周期类操作，机制归 13 */
          case RS_UP:         result = do_up(&m);         break;   /* 拉起服务 */
          case RS_DOWN:       result = do_down(&m);       break;   /* 停用服务 */
          case RS_REFRESH:    result = do_refresh(&m);    break;   /* 刷新服务 */
          case RS_RESTART:    result = do_restart(&m);    break;   /* 重启服务 */
          case RS_SHUTDOWN:   result = do_shutdown(&m);   break;   /* 关闭服务 */
          case RS_UPDATE:     result = do_update(&m);     break;   /* 例外：Live Update 接管，机制归 16 */
          case RS_CLONE:      result = do_clone(&m);      break;   /* 派生子服务 */
          case RS_UNCLONE:    result = do_unclone(&m);    break;   /* 回收子服务 */
          case RS_EDIT:       result = do_edit(&m);       break;   /* 改服务属性 */

          case RS_SYSCTL:     result = do_sysctl(&m);     break;
          case RS_FI:         result = do_fi(&m);         break;
          case RS_GETSYSINFO: result = do_getsysinfo(&m); break;
          case RS_LOOKUP:     result = do_lookup(&m);     break;
          /* Ready messages —— 就绪回执，机制归 12 */
          case RS_INIT:       result = do_init_ready(&m); break;   /* 出生回执 */
          case RS_LU_PREPARE: result = do_upd_ready(&m);  break;
          default: 
              printf("RS: warning: got unexpected request %d from %d\n",
                  m.m_type, m.m_source);
              result = ENOSYS;
          }

          /* Finally send reply message, unless disabled. */
          if (result != EDONTREPLY) {
	      m.m_type = result;
              reply(who_e, NULL, &m);
          }
      }
  }
}
```

要点：

- **`sef_local_startup()` 是本文档的第一个锚点**（§2.2）：SEF 回调注册完成后，`sef_startup()` 对 RS 走 `do_sef_rs_init()` 本地合成 fresh 信息并调用 `sef_cb_init_fresh`；`RS_LU_PREPARE` 是之后主循环的消息，不经过 `sef_startup()` 的 init 分派。
- **`sys_getmachine(&machine)`**（main.c:53）：把机器信息（处理器数、BSP id、APIC 等，`minix3/minix/include/minix/type.h` 的 `struct machine`）拷贝到全局 `machine`。Rust 侧外部接口契约归 `19-rs-external-interfaces.md`。
- **主循环骨架**：`rs_idle_period`（06）→ `get_work`（06）→ `rs_isokendpt`（02）→ 分类（`06-rs-main-loop.md`）。**每个 handler 只标注归属，机制不在此展开**。
- **`EDONTREPLY` 协议**（main.c:125）：handler 返回 `EDONTREPLY` 表示"稍后自行回复"（late reply，06），主循环不代发。`reply(who_e, NULL, &m)` 的第二个参数 `rp` 为 NULL 表示不查 slot（06）。

### 2.2 sef_local_startup()：SEF 回调注册（main.c:136-152）

```c
static void sef_local_startup()
{
  /* Register init callbacks. */
  sef_setcb_init_fresh(sef_cb_init_fresh);
  sef_setcb_init_restart(sef_cb_init_restart);
  sef_setcb_init_lu(sef_cb_init_lu);

  /* Register response callbacks. */
  sef_setcb_init_response(sef_cb_init_response);
  sef_setcb_lu_response(sef_cb_lu_response);

  /* Register signal callbacks. */
  sef_setcb_signal_handler(sef_cb_signal_handler);
  sef_setcb_signal_manager(sef_cb_signal_manager);

  /* Let SEF perform startup. */
  sef_startup();
}
```

**注册表语义**（`minix3/minix/include/minix/sef.h`）：`sef_setcb_*` 是 libsys 的全局函数指针赋值。RS 是 `ROOT_SYS_PROC`：`sef_startup()` 调 `do_sef_rs_init()`（sef.c:109-115）在本地构造 `sef_init_info_t`，按 `LU_SYS_PROC`/`RST_SYS_PROC` 选 `SEF_INIT_FRESH`/`LU`/`RESTART`，再调用对应回调；其他服务才接收 `RS_INIT` 并按 `m_rs_init.type` 分派。RS 注册全部 7 个回调，是注册最全的用户态服务；7 个回调的机制归属见 §3.3 的 `SefCallbacks`。

### 2.3 sef_cb_init_fresh()：四步 boot（main.c:158-494）

`sef_cb_init_fresh` 是本文档的 boot 锚点。逐段分析：

#### 2.3.1 前置：配置、频率、grant、全局复位、boot 映像拷贝（main.c:178-198）

```c
  /* See if we run in verbose mode. */
  env_parse("rs_verbose", "d", 0, &rs_verbose, 0, 1);

  if ((s = sys_getinfo(GET_HZ, &system_hz, sizeof(system_hz), 0, 0)) != OK)
	  panic("Cannot get system timer frequency\n");

  /* Initialize the global init descriptor. */
  rinit.rproctab_gid = cpf_grant_direct(ANY, (vir_bytes) rprocpub,
      sizeof(rprocpub), CPF_READ);
  if(!GRANT_VALID(rinit.rproctab_gid)) {
      panic("unable to create rprocpub table grant: %d", rinit.rproctab_gid);
  }

  /* Initialize some global variables. */
  RUPDATE_INIT();
  shutting_down = FALSE;

  /* Get a copy of the boot image table. */
  if ((s = sys_getimage(image)) != OK) {
      panic("unable to get copy of boot image table: %d", s);
  }
```

- **`env_parse("rs_verbose")`**（main.c:179）：从环境/启动参数解析 `rs_verbose` 调试开关（0/1）。Rust 侧构建配置宏 → cargo feature 或 env 注入（feature 门控，见 §3.6）。
- **`sys_getinfo(GET_HZ, &system_hz)`**（main.c:181）：系统时钟频率（ticks/秒），是 `RS_INIT_T`/`RS_DELTA_T` 等周期常量的基础（const.h:48-49，语义归 07）。
- **`rinit.rproctab_gid` 创建点**（main.c:185）：`cpf_grant_direct` 把 RS 的 `rprocpub` 公开表以只读 grant 形式共享给任意进程——服务通过 `RS_INIT` 消息携带此 gid 读取公开信息。**创建点在本文档，消费点在 12（`init_service` 把 gid 放入 RS_INIT 消息）**（plan grant 分工）。
- **`RUPDATE_INIT()` + `shutting_down = FALSE`**（main.c:192-193）：全局 update 描述符复位 + 关停标志复位。update 状态机语义归 16。
- **`sys_getimage(image)`**（main.c:196）：把内核的 `boot_image[]` 表（`NR_BOOT_PROCS` 项）运行期拷贝到 RS 本地栈数组。**这是 表静态化 的关键**：boot 映像来源在 minix-rs 中如何建模见 §3.1。

#### 2.3.2 计数核对与表重置（main.c:200-250）

```c
  /* Determine the number of system services in the boot image table. */
  nr_image_srvs = 0;
  for(i=0;i<NR_BOOT_PROCS;i++) {
      ip = &image[i];
      if(iskerneln(_ENDPOINT_P(ip->endpoint))) continue;   /* 跳过内核任务 */
      nr_image_srvs++;
  }
  /* ... 同样统计 priv 表（跳过内核任务）... */
  if(nr_image_srvs != nr_image_priv_srvs) {
	panic("boot image table and boot image priv table mismatch");
  }

  /* Reset the system process table. */
  for (rp=BEG_RPROC_ADDR; rp<END_RPROC_ADDR; rp++) {
      rp->r_flags = 0;
      rp->r_init_err = ERESTART;
      rp->r_pub = &rprocpub[rp - rproc];
      rp->r_pub->in_use = FALSE;
      rp->r_pub->old_endpoint = NONE;
      rp->r_pub->new_endpoint = NONE;
  }
```

- **计数核对**（main.c:225-227）：boot 映像表中系统服务数必须等于 RS 自身 priv 表中系统服务数——这是"RS 的表与内核的表一致"的启动协议完整性检查。`iskerneln` 按端点号判断内核任务（负端点）。`validate_tables` 另校验每行 `endpoint.slot() == proc_nr`——内核 boot 表端点由 `_ENDPOINT(0, proc_nr)` 派生（kernel 侧 boot 表生成处），手写 placeholder 若错位会 boot 错端点；`boot_img` 的 16 字节名钳制（`len.min(16)` 的 const 求值版本）使超长名不再以晦涩的 const 越界编译错误暴露。
- **表重置**（main.c:230-237）：把 `rproc`/`rprocpub` 全表清零（`r_flags=0`、`r_init_err=ERESTART`、`in_use=FALSE`、`old/new_endpoint=NONE`）。进程表字段语义归 02；这里只标注调用点。

#### 2.3.3 Step 1：逐服务设置 priv/sys/dev 属性（main.c:244-346）

```c
  for (i=0; boot_image_priv_table[i].endpoint != NULL_BOOT_NR; i++) {
      boot_image_priv = &boot_image_priv_table[i];
      if(iskerneln(_ENDPOINT_P(boot_image_priv->endpoint))) continue;

      boot_image_info_lookup(boot_image_priv->endpoint, image,
          &ip, NULL, &boot_image_sys, &boot_image_dev);
      rp = &rproc[boot_image_priv - boot_image_priv_table];
      rpub = rp->r_pub;

      /* Set privileges. */
      strcpy(rpub->label, boot_image_priv->label);
      rp->r_priv.s_id = static_priv_id(_ENDPOINT_P(...));
      rp->r_priv.s_flags = boot_image_priv->flags;
      rp->r_priv.s_init_flags = SRV_OR_USR(rp, SRV_I, USR_I);
      rp->r_priv.s_trap_mask= SRV_OR_USR(rp, SRV_T, USR_T);
      ipc_to = SRV_OR_USR(rp, SRV_M, USR_M);
      fill_send_mask(&rp->r_priv.s_ipc_to, ipc_to == ALL_M);
      rp->r_priv.s_sig_mgr= SRV_OR_USR(rp, SRV_SM, USR_SM);
      rp->r_priv.s_bak_sig_mgr = NONE;
      calls = SRV_OR_USR(rp, SRV_KC, USR_KC) == ALL_C ? all_c : no_c;
      fill_call_mask(calls, NR_SYS_CALLS,
          rp->r_priv.s_k_call_mask, KERNEL_CALL, TRUE);

      /* RS and VM are exceptions and are already running. */
      if(boot_image_priv->endpoint != RS_PROC_NR &&
         boot_image_priv->endpoint != VM_PROC_NR) {
          if ((s = sys_privctl(ip->endpoint, SYS_PRIV_SET_SYS, &(rp->r_priv))) != OK) {
              panic("unable to set privilege structure: %d", s);
          }
      }
      if ((s = sys_getpriv(&(rp->r_priv), ip->endpoint)) != OK) {
          panic("unable to synch privilege structure: %d", s);
      }

      /* Set sys properties. */
      rpub->sys_flags = boot_image_sys->flags;
      /* Set dev properties. */
      rpub->dev_nr = boot_image_dev->dev_nr;

      strlcpy(rp->r_cmd, ip->proc_name, sizeof(rp->r_cmd));
      rp->r_script[0]= '\0';
      build_cmd_dep(rp);
      strlcpy(rpub->proc_name, ip->proc_name, sizeof(rpub->proc_name));

      calls = SRV_OR_USR(rp, SRV_VC, USR_VC) == ALL_C ? all_c : no_c;
      fill_call_mask(calls, NR_VM_CALLS, rpub->vm_call_mask, VM_RQ_BASE, TRUE);

      rp->r_scheduler = SRV_OR_USR(rp, SRV_SCH, USR_SCH);
      rp->r_priority = SRV_OR_USR(rp, SRV_Q, USR_Q);
      rp->r_quantum = SRV_OR_USR(rp, SRV_QT, USR_QT);

      rpub->endpoint = ip->endpoint;
      rp->r_old_rp = NULL; rp->r_new_rp = NULL;
      rp->r_prev_rp = NULL; rp->r_next_rp = NULL;
      rp->r_uid = 0; rp->r_check_tm = 0;
      rp->r_alive_tm = getticks(); rp->r_stop_tm = 0;
      rp->r_asr_count = 0; rp->r_restarts = 0; rp->r_period = 0;
      rp->r_exec = NULL; rp->r_exec_len = 0;

      rp->r_flags = RS_IN_USE | RS_ACTIVE;
      rproc_ptr[_ENDPOINT_P(rpub->endpoint)]= rp;
      rpub->in_use = TRUE;
  }
```

要点（**机制全部移交，此处只列结构与归属**）：

- **`boot_image_info_lookup`**（main.c:253-254）：一次调用同时查找 image/priv/sys/dev 四张表（§2.4）。
- **priv 结构初始化**（main.c:258-280）：label、静态 priv id、flags、init flags、trap mask、send mask（`fill_send_mask` → 05）、sig mgr、call mask（kernel 侧）。**priv 结构字段语义归 03**。
- **RS/VM 例外**（main.c:285-291）：`SYS_PRIV_SET_SYS` 对 RS/VM **跳过**——它们"已经在运行"（plan RS/VM 例外）。其余服务则把 priv 结构提交给 kernel。`sys_getpriv`（main.c:294）再同步回本地结构。
- **sys/dev 属性**（main.c:301-306）：`sys_flags`（`SF_*` 全表在 `99-rs-global-concepts.md`）、`dev_nr`（主设备号）。
- **命令/调度属性**（main.c:309-322）：`r_cmd`/`r_script`/`build_cmd_dep`（08）、`vm_call_mask`（VM 侧调用掩码）、`scheduler`/`priority`/`quantum`（03）。
- **slot 默认值 + 激活**（main.c:324-345）：四链指针清 NULL（四链指针）、`r_uid=0`、监控字段清 0、`RS_IN_USE | RS_ACTIVE` 激活、`rproc_ptr` 快速索引登记（快速索引）、`in_use=TRUE`。字段语义归 02。

#### 2.3.4 Step 2：允许运行（main.c:348-399）

```c
  nr_uncaught_init_srvs = 0;
  for (i=0; boot_image_priv_table[i].endpoint != NULL_BOOT_NR; i++) {
      boot_image_priv = &boot_image_priv_table[i];
      if(iskerneln(_ENDPOINT_P(boot_image_priv->endpoint))) continue;

      rp = &rproc[boot_image_priv - boot_image_priv_table];
      rpub = rp->r_pub;

      /* RS/VM are already running as we speak. */
      if(boot_image_priv->endpoint == RS_PROC_NR ||
         boot_image_priv->endpoint == VM_PROC_NR) {
          if ((s = init_service(rp, SEF_INIT_FRESH, rp->r_priv.s_init_flags)) != OK) {
              panic("unable to initialize %d: %d", ...);
          }
          /* VM will still send an RS_INIT message, though. */
          if (boot_image_priv->endpoint != RS_PROC_NR) {
              nr_uncaught_init_srvs++;
          }
          continue;
      }

      /* Allow the service to run. */
      if ((s = sched_init_proc(rp)) != OK) {
          panic("unable to initialize scheduling: %d", s);
      }
      if ((s = sys_privctl(rpub->endpoint, SYS_PRIV_ALLOW, NULL)) != OK) {
          panic("unable to initialize privileges: %d", s);
      }
      if(boot_image_priv->flags & SYS_PROC) {
          if ((s = init_service(rp, SEF_INIT_FRESH, rp->r_priv.s_init_flags)) != OK) {
              panic("unable to initialize service: %d", s);
          }
          if(rpub->sys_flags & SF_SYNCH_BOOT) {
              catch_boot_init_ready(rpub->endpoint);
          }
          else {
              nr_uncaught_init_srvs++;
          }
      }
  }
```

要点：

- **RS/VM 例外**（main.c:362-373）：这两个服务已在内核中运行（RS 自己、VM 先于 RS 启动），因此不经过 `sched_init_proc` + `SYS_PRIV_ALLOW`，而是直接 `init_service` 自模拟初始化（12）。VM 虽然已运行，**仍会异步发回一条 RS_INIT 消息**，计入 `nr_uncaught_init_srvs`（plan VM 异步）。
- **普通服务**（main.c:375-398）：`sched_init_proc`（调度器登记，03）→ `sys_privctl(SYS_PRIV_ALLOW)`（解除运行抑制，03）→ `init_service`（发送 RS_INIT 初始化消息，12）。`SF_SYNCH_BOOT` 标志（minix3/minix/include/minix/rs.h 的 `SF_SYNCH_BOOT`）决定同步等待该服务的 init ready 还是累积到 Step 3 统一收。
- `init_service`/`catch_boot_init_ready` 机制全部归 12；此处只列调用点与标志判定。

#### 2.3.5 Step 3：收齐剩余 init ready（main.c:401-407）

```c
  while(nr_uncaught_init_srvs) {
      catch_boot_init_ready(ANY);
      nr_uncaught_init_srvs--;
  }
```

阻塞接收剩余 `SYS_PROC` 服务的 `RS_INIT` 回复（`catch_boot_init_ready(ANY)`，机制归 12）。Step 3 是 boot 屏障：RS 收齐后才进入 Step 4；服务本身在 Step 2 已放行并发初始化，串行的只是 RS 的推进。

#### 2.3.6 Step 4：getnpid + sys_setalarm（main.c:409-433）

```c
  for (i=0; boot_image_priv_table[i].endpoint != NULL_BOOT_NR; i++) {
      boot_image_priv = &boot_image_priv_table[i];
      if(iskerneln(_ENDPOINT_P(boot_image_priv->endpoint))) continue;
      rp = &rproc[boot_image_priv - boot_image_priv_table];
      rpub = rp->r_pub;
      rp->r_pid = getnpid(rpub->endpoint);
      if(rp->r_pid < 0) {
          panic("unable to get pid: %d", rp->r_pid);
      }
  }

  /* Set alarm to periodically check service status. */
  if (OK != (s=sys_setalarm(RS_DELTA_T, 0)))
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
  rp = rproc_ptr[_ENDPOINT_P(RS_PROC_NR)];
  if((s = clone_slot(rp, &replica_rp)) != OK) {
      panic("unable to clone current RS instance: %d", s);
  }
  pid = srv_fork(0, 0);
  if(pid < 0) panic("unable to fork a new RS instance: %d", pid);
  replica_pid = pid ? pid : getpid();
  getprocnr(replica_pid, &replica_endpoint);
  replica_rp->r_pid = replica_pid;
  replica_rp->r_pub->endpoint = replica_endpoint;

  if(pid == 0) {
      /* New RS instance running. */
      s = update_service(&rp, &replica_rp, RS_SWAP, 0);
      cpf_reload();
      cleanup_service(rp);
      vm_memctl(RS_PROC_NR, VM_RS_MEM_PIN, 0, 0);
  }
  else {
      /* Old RS instance running. */
      s = sys_privctl(replica_endpoint, SYS_PRIV_SET_SYS, &(replica_rp->r_priv));
      sched_init_proc(replica_rp);
      s = sys_privctl(replica_endpoint, SYS_PRIV_YIELD, NULL);
      NOT_REACHABLE;
  }
#endif
```

**语义**：boot 完成后，RS 立即 fork 一个自身副本（`srv_fork(0,0)` 经 PM，自升级复制），并让自己 live update 到副本中（`update_service(RS_SWAP)`）。子进程（新 RS）接管：`cpf_reload` + `cleanup_service`（清理旧实例）+ `vm_memctl(VM_RS_MEM_PIN)`（钉住内存）；父进程（旧 RS）给副本设 priv + 调度 + `SYS_PRIV_YIELD`（让出）。**完整流程机制归 18**（RS 自生命周期）；本文档只列调用链。`USE_LIVEUPDATE` 为编译期宏（默认 yes，`share/mk/bsd.own.mk:1499`）→ Rust feature gate（feature 门控，§3.6）。

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
      for (i=0; i < NR_BOOT_PROCS; i++) {
          if(image[i].endpoint == endpoint) { *ip = &image[i]; break; }
      }
      if(i == NR_BOOT_PROCS) panic("boot image table lookup failed");
  }

  /* ... priv table: 同样语义，panics if not found ... */  

  /* sys table: 未命中则采用 DEFAULT_BOOT_NR 默认条目 */    
  if(sp) {
      for (i=0; boot_image_sys_table[i].endpoint != DEFAULT_BOOT_NR; i++) {
          if(boot_image_sys_table[i].endpoint == endpoint) { *sp = &boot_image_sys_table[i]; break; }
      }
      if(boot_image_sys_table[i].endpoint == DEFAULT_BOOT_NR)
          *sp = &boot_image_sys_table[i];         /* accept the default entry */
  }

  /* dev table: 同样 DEFAULT 兜底语义 */                   
}
```

要点（`static`，仅 `sef_cb_init_fresh` Step 1 调用）：

- **image 表 / priv 表**：image 未命中 → `panic`（启动协议错误，fail-closed）；priv 分支的 C panic 条件 `i == NULL_BOOT_NR` 实际不可达（循环下标最大 12，哨兵是 17），Rust 用 `Err(LookupError::PrivTable)` 保留 fail-closed 意图。**priv 表的哨兵是 `NULL_BOOT_NR`**（const.h:61，= `NR_BOOT_PROCS`）；**sys/dev 表的哨兵是 `DEFAULT_BOOT_NR`**（const.h:62），语义不同——sys/dev 未命中是**正常**的（服务无覆盖配置），采用默认条目。
- 输出参数按需填充（`ip`/`pp`/`sp`/`dp` 可为 NULL）。Step 1 调用时传 `&ip, NULL, &boot_image_sys, &boot_image_dev`（main.c:253-254）——只查 image/sys/dev 三表，priv 表条目由外层循环直接索引（`rp = &rproc[boot_image_priv - boot_image_priv_table]`）。

### 2.5 table.c：三张 boot 表（table.c:15-50）

`minix3/minix/servers/rs/table.c` 定义 RS 启动协议的三张静态表：

**boot_image_priv_table**（table.c:15-30，12 个系统服务 + NULL 哨兵；顺序是 RS 的准备/放行遍历顺序（不是内核可调度序））：

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

**boot_image_sys_table**（table.c:33-42，6 个覆盖项 + DEFAULT 哨兵；`SRVR_SF = SF_CORE_SRV|SF_NEED_REPL`，`VM_SF = SRVR_SF`，`SRV_SF = SF_CORE_SRV`，基 flag 在 minix3/minix/include/minix/rs.h:191，派生常量在 const.h:65-68）：

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

**表语义**：priv 表是 boot 主表（顺序是 RS 的准备/放行遍历顺序，实际运行序由内核可调度序决定，条目数必须与内核 boot_image 表系统服务数一致，§2.3.2）；sys 表只列**覆盖默认 sys 属性**的服务（未列出的服务用 `DEFAULT_BOOT_NR` 条目）；dev 表只列有主设备号的服务。这是 表静态化 的静态化基础（§3.1）。

### 2.6 本章回顾

#### 小结

- `main()` 先 `sef_local_startup()` 注册回调，再进入主循环；boot 由 `sef_cb_init_fresh()` 承担。
- 四步顺序是：priv 提交与本地属性 → 放行 → 收齐 ready → pid+alarm；Step 1 不放行。
- `boot_image_info_lookup()` 用 endpoint 在内核 boot 表与 RS 三张表之间对齐。
- `table.c` 的 priv/sys/dev 三表以 priv 表为主表，sys/dev 只列覆盖项。
- 任何表数/端点不一致都在 boot 期 panic，不留到运行期。

#### 术语

- **boot 映像表**：内核 `boot_image[]`，RS 用 `sys_getimage` 拷贝。
- **priv/sys/dev 表**：RS 自带的三张静态表，分别描述权限、覆盖的 sys 属性、主设备号。
- **`iskerneln`**：按端点号跳过内核任务的判断。
- **Step 1/2/3/4**：本地属性、放行、收齐 ready、pid+alarm。
- **`RS_INIT`**：服务初始化消息；boot 期用于 ready 确认。

---

## 3. Rust 设计决策

> 每个决策必须可追溯到 §2 的 C 源码分析。Rust 代码位置只以函数/模块名引用（避免行号漂移）。

C 的四步 boot 依赖三张全局表和一组 libsys 回调。Rust 版本要回答的是：这些隐式全局怎样变成显式输入，四步顺序怎样在类型上表达，哪些外部调用仍留在 syscall 边界。

### 3.0 本章要回答的问题

读者需要能回答：`BootTables` 为什么把 `sys_getimage` 的拷贝与三张静态表放在一起；`boot_image_info_lookup` 为什么返回 `Result`；`SefCallbacks` 与 C 的 7 个 `sef_setcb_*` 怎样对应；`BootInit` 状态机怎样保证 Step 1→4 不可乱序。

#### 最小示例

```bash
$ grep -n 'pub struct BootTables\|pub enum BootError\|pub trait SefCallbacks\|pub struct BootInit' os/servers/rs/src/boot.rs os/servers/rs/src/sef.rs
os/servers/rs/src/boot.rs:554:pub struct BootTables<'a> {
os/servers/rs/src/boot.rs:760:pub enum BootError {
os/servers/rs/src/sef.rs:92:pub trait SefCallbacks {
os/servers/rs/src/boot.rs:878:pub struct BootInit<'a> {
```

#### 失败对照

如果 lookup 返回 `Option` 并在缺失时静默跳过，boot 会带着一个空槽继续进入 Step 2，错误要到第一次 IPC 才暴露；Rust 用 `Result<_, LookupError>` 在查找点 fail-closed。

### 3.1 boot 表来源显式化：BootTables（对应 §2.3.1/§2.5，ARCH 表静态化）

**C**：`sys_getimage()`（main.c:196）把内核 `boot_image[]` 运行期拷贝到栈数组；priv/sys/dev 三表是 RS 自带的静态表（table.c）。**Rust**：

1. `sys_getimage()` 的拷贝结果建模为构造输入 `&[BootImage]`；`acquire_from()` 把解码后的表 `Box::leak` 成 `'static` 切片，由 `BootTables` 借用，生命周期到进程结束（对应 C 的 boot 表静态存储）（`minix-types` 已有 `BootImage`，`os/libs/minix-types/src/types/boot.rs`），由 `main.rs` 通过 `BootTables::acquire_from()` 从内核 `GET_IMAGE` 取真实拷贝；失败即 panic。`placeholder()` 只用于测试与宿主开发。
2. priv/sys/dev 三表**静态化**（表静态化：`boot_image_priv/sys/dev` 三表在 minix-rs 侧可静态化，同 kernel 表）：`table.rs` 用 `&'static [BootImagePriv]` 等切片常量复刻 table.c 内容，编译期保证条目不漂移。

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
- **哨兵类型化**：C 的 `NULL_BOOT_NR`/`DEFAULT_BOOT_NR`（const.h:61-62）是魔法哨兵值；Rust 用**切片长度 + 查找返回值**表达：priv 表无哨兵条目（遍历到结尾即失败），sys/dev 表查找失败返回默认条目（§3.2）。`NULL_BOOT_NR` 仅保留为常量对齐（`99-rs-global-concepts.md`），不参与逻辑。

### 3.2 boot_image_info_lookup 的 Result 化（对应 §2.4）

**C**：image 未命中 `panic("boot image table lookup failed")`（main.c:731）；priv 分支的 panic 条件实际不可达（见 §2.4）；sys/dev 表未命中采用默认条目（main.c:753-762, 768-777）。**Rust**：

- image/priv 查找 → `Result<_, LookupError>`（`LookupError::{ImageTable, PrivTable}`），调用方（四步 boot）遇错即 `panic!` 或向上传播——**保留 fail-closed 语义**（错误显式化）。
- sys/dev 查找 → `Option` 语义：未命中返回默认条目（`DefaultSys`/`DefaultDev`），不报错。

```rust
pub enum LookupError { ImageTable, PrivTable }
pub fn lookup_image<'a>(image: &'a [BootImage], ep: Endpoint) -> Result<&'a BootImage, LookupError>;
pub fn lookup_priv<'a>(table: &'a [BootImagePriv], ep: Endpoint) -> Result<&'a BootImagePriv, LookupError>;
pub fn lookup_sys<'a>(table: &'a [BootImageSys], ep: Endpoint) -> &'a BootImageSys;   // DEFAULT 兜底
pub fn lookup_dev<'a>(table: &'a [BootImageDev], ep: Endpoint) -> &'a BootImageDev;   // DEFAULT 兜底
```

理由：C 的 panic 是"启动协议错误"的信号；Rust 用 `Result` 让错误显式化，四步 boot 顶层统一处理（错误显式化）。

### 3.3 SEF 回调注册表：SefCallbacks（对应 §2.2，ARCH SEF 回调抽象）

**C**：`sef_setcb_*` 全局函数指针 + `sef_startup()` 状态机（libsys/sef.c），回调体靠全局变量访问
`rproc[]`/`rupdate`。**Rust**：用户态 SEF 抽象（SEF 回调抽象：RS 先行实现并复用至 PM/VFS）建模为
**trait**（——原 fn 指针结构体无法携带服务器状态，12/18/06 的回调体
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

- `SefInitType::{Fresh, Lu, Restart}` 对应 `SEF_INIT_FRESH=0/LU=1/RESTART=2`（minix3/minix/include/minix/sef.h 的 `SEF_INIT_FRESH`）。
- `signal_manager` 的签名逐字对照 C 回调类型 `int(*)(endpoint_t target, int signo)`
  （minix3/minix/include/minix/sef.h 的 `IS_SEF_SIGNAL_REQUEST`）：`target` 是信号管理器目标端点、`signo` 是信号号；`target` 用 `Endpoint`
  newtype 而非裸 `i32`，两个参数在调用点不可互换。
- **实现者是 `RsServer` 本身**：`impl SefCallbacks for RsServer`。`RsServer::init(init_type)` 对应
  `sef_startup()` 的分派（main.c:151）——按 `init_type` 路由到 `init_fresh`/`init_lu`/
  `init_restart`；`init_fresh` 即四步 boot（§3.4）。**消息接收/拦截机制归 12**
  （`do_init_ready`/`sef_cb_init_response`）；主循环的 RS_INIT 分支（12）直接调 trait 方法，
  回调体经 `&mut self` 拿到 `RsServer` 状态。12/18/06 未落地的 5 个方法在 `RsServer` 上
  fail-closed（`Err(ENOSYS)`），`signal_handler` 保留 `unimplemented!`
  （无 Result 通道，契约归 06）。
- **restart 回调的分派目标是运行期状态**：C 在启动时注册 RS 自有 restart
  handler（main.c:140），`sef_cb_init_lu` 在 LU 开始时把它重绑为 stateful 转移通用体
  （main.c:558）。trait 保持静态，重绑由 `RsServer.restart_cb: sef::RestartCb`（`Rs`/`Stateful`）
  承载、`init_restart` 内 match 分派——单点重绑用一个枚举字段表达，比把整个回调集合 enum 化
  更贴近 C 的"只换一个表项"。详见 `18-rs-self-lifecycle.md`。
- **与 VM 01 §3.4 的简化对齐**：VM 用 `rs_handshake()` 承接 SEF 框架（`02-stage-vm/01-vm-init-main.md` §3.4）；RS 是 SEF 的**提供方**（其余服务的 init 协议由 RS 实现），必须保留完整的回调集语义；trait 化去掉 C 的全局函数指针且不引入"注册值 + 回调体无法触达状态"的中间形态。

### 3.4 四步 boot 类型化：BootInit 状态机（对应 §2.3）

**C**：`sef_cb_init_fresh` 一个函数内顺序执行 4 步（main.c:158-494）。**Rust**：`BootInit` 结构体 + 单入口 `init_fresh()`，内部按 §2.3 顺序调用四个私有方法——**顺序不可重排由编排方法保证**：

```rust
pub struct BootInit<'a> {
    tables: BootTables<'a>,     // C: sys_getimage 拷贝 + 三静态表（§3.1）
    machine: Machine,           // C: machine（main.c:53 sys_getmachine）
    rinit: RinitState,          // C: rinit.rproctab_gid（§2.3.1，消费在 12）
    table: RProcTable,          // C: rproc/rprocpub 表（字段语义归 02；本模块只做生命周期编排）
    shutting_down: bool,        // C: shutting_down（main.c:193）
    system_hz: u32,             // C: system_hz（main.c:181）
    nr_uncaught_init_srvs: usize, // C: nr_uncaught_init_srvs（main.c:349-406）
}
impl BootInit<'_> {
    pub fn init_fresh(&mut self, sys: &mut dyn KernelApi) -> Result<(), BootError> {
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

> **状态 handover**：C 的 boot 状态就是运行期状态（`glo.h` 全局：
> `rproc[]`/`system_hz`/`shutting_down`/`rinit`，boot 后继续存活）。Rust 侧 `BootInit` 增加
> `into_state(self) -> ServerState<'a>`（**消费 self**，boot 后旧机器不可再改）；`RsServer` 持有
> `boot: Option<BootInit>` + `state: Option<ServerState>`，`init(Fresh)` 成功后将状态移交，
> `run()`/主循环（06）经单一访问器 `RsServer::state()` 读取 `table`/`system_hz`/`shutting_down`——
> 避免"5+ getter 垃圾场"方案。typestate（`BootInit<Fresh>` → `RsRunning`）记为 06 接线时的备选。

> **machine 纳入 ServerState**：`ServerState` 增加
> `machine: Machine` 字段；`step0_prepare` 在启动期一次性 `sys.get_machine()?`
> （C 在 `sef_local_startup` 的 fresh boot 之后取 machine；Rust 提前到 step0，属于时序变化），经 `into_state`
> 移交运行期。`check_request` 的 CPU 亲和解析（request.c:1286-1296，`RS_CPU_BSP`/
> 越界回退）读这个快照，而不是在主循环里临时查询（C 是一次性启动快照语义）。
> 对照 Redox：daemon 的 CPU 拓扑经 scheme 按需查询（`sysinfo`），无启动期全局快照；
> Minix RS 是启动期快照，Rust 显式建模为 `ServerState` 字段。

> **Step 2/3 fail-closed**：Step 2 对 `SF_SYNCH_BOOT` 服务不再计入
> `nr_uncaught_init_srvs`（C 是同步 `catch_boot_init_ready`，main.c:390-392；12 落地前显式
> `Err(BootError::Kernel(ENOSYS))`）；Step 3 循环 `catch_boot_init_ready`：C 是阻塞等待；Rust 生产 `IpcApi` 未接线时第一次 catch 返回 `Err(Kernel(ENOSYS))`，mock inbox 有消息时消费并递减。12 落地后恢复阻塞语义。boot 测试改走私有 step 方法直接驱动各步，
> 测试覆盖 SYNCH_BOOT 与 Step 3 两条路径。

> **boot 错误域二分**：boot 的失败在 C 里只有一种结局——`panic()`（main.c:226
> 计数不符、main.c:427-429 拿不到 pid、main.c:731/746 查表未命中），因为半启动的 RS 无法运行。
> Rust 保留 fail-closed `Err` 契约，但把混在一个 `ENOSYS` 里的两族失败拆进
> `BootError`：**内核调用失败**（`BootError::Kernel(e)`——19 接线前即 `Kernel(ENOSYS)`，
> "机制尚不存在"）与 **boot 不变式违例**（`Lookup(LookupError)`/`CountMismatch`/
> `EndpointMismatch`/`InvalidPid(Pid)`——boot 表或内核应答不可用，即 C 的 panic 族）。
> 线面映射（`From<BootError> for Errno`）：`Kernel(e)` 保留原 errno；不变式违例 → `EINVAL`——
> `ENOSYS` 从此只表示"未接线"，"boot 数据不可用"在调用点即可区分。SEF 回调面只携带 errno
> （C `int` 面），类型化原因存于 `RsServer::boot_diagnostic()`，main.rs 的致命启动报告直接
> 打印它（对照 C：panic 消息本身携带原因，main.c:226）。

### 3.5 外部边界：KernelApi 与五个域面（对应 §2.1/§2.3，外部契约归 19）

C 的 `sys_getmachine`/`sys_getinfo`/`sys_privctl`/`sys_getpriv`/`sys_setalarm`/`getnpid`/`sched_init_proc`/`srv_fork` 等（§2 各调用点）是 libsys 自由函数，但它们**并非发往同一处**：`sys_*` 走 SYSTASK（内核）；`sched_*` 的传输目标是复合的——调度器是 KERNEL 时走 `sys_schedctl` 内核调用，否则是发给调度器端点的 `SCHEDULING_START/STOP` 消息（minix3/minix/lib/libsys/sched_start.c:sched_start、minix3/minix/lib/libsys/sched_stop.c:sched_stop，本重写里调度器是 SCHED 服务器）；`getnpid`/`getnuid`/`srv_fork`/`setuid` 等走 `_taskcall(PM_PROC_NR, ...)`；`vm_*` 走 VM；接收与回复是 RS 自己的 IPC 面。Rust 侧的 boot 编排只走 `KernelApi` 的五个域面；例外是 `BootTables::acquire_from()` 直接调 `minix_sys::syscall::sys_getimage` 取 boot 映像。grant 入口是 `IpcApi::grant_read`，生产实现是 `TrapKernelApi`（直接陷入内核）（supertrait 组合）：

```rust
pub trait SysApi {
    // 内核系统调用面（SYSTASK）：
    // get_machine（main.c:53）/ get_hz（main.c:181）/ get_ticks（main.c:333）
    // privctl（main.c:287/379/478/485）/ getpriv（main.c:294）/ setalarm（main.c:433）
    // sys_kill（manager.c:399）/ sys_update（update.c:272-275）
}

pub trait SchedApi {
    // 调度器面（复合传输目标）：sched_init_proc（main.c:376/482）/ sched_stop（manager.c:462）
}

pub trait PmApi {
    // PM 进程生命周期面（_taskcall(PM_PROC_NR)）：
    // getnuid/getnpid/getprocnr → PM_GETEPINFO（lib/libsys/getepinfo.c:15-47）
    // srv_fork（PM_SRV_FORK）/ srv_execve（manager.c:634，复合体见 10 §3.2）
    // srv_kill（manager.c:469）/ waitpid（request.c:1063）/ setuid（manager.c:656）
}

pub trait VmApi {
    // VM 消息面：vm_memctl（manager.c:604-693）/ vm_set_priv（manager.c:698）
}

pub trait IpcApi {
    // RS 自身 IPC 面：receive（main.c:826-833）/ reply（utility.c:309）/
    // notify（request.c:1035，do_period 状态 ping）
}

pub trait KernelApi: SysApi + SchedApi + PmApi + VmApi + IpcApi {}
impl<T> KernelApi for T where T: SysApi + SchedApi + PmApi + VmApi + IpcApi {}
```

`&mut dyn KernelApi` 的调用点不变——supertrait 方法经组合对象的 vtable 直接可调；拆分的收益是"这条调用发给谁"在类型上可见，且 19 接线可以按面逐个实现传输（内核面配 minix-sys 的 SYS_* 包装、PM 面配 PM 消息构造……），测试也可以只实现被测的那个面（`test_domain_face_implementable_in_isolation`）。

`sched_init_proc` 的签名值得单独说明：C 的 `sched_start`（minix3/minix/lib/libsys/sched_start.c:sched_inherit）需要
scheduler/priority/quantum/cpu 全部四个参数，且回写 `*newscheduler_e`（可能被转发到别的调度器），
所以 Rust 侧收 `&SchedulerConfig`（sched.rs，携带全部参数）并返回 `Result<Endpoint, Errno>`。
boot Step 2 用 `SchedulerConfig::boot_defaults` 构造（`SRV_SCH=KERNEL`/`SRV_Q=USER_Q=7`/
`SRV_QT=USER_QUANTUM=200`，priv.h:88,93,98 + config.h:69,74）；NONE 调度器短路
（minix3/minix/lib/libsys/sched_start.c:sched_inherit）在纯函数 `sched::sched_decision` 内实现，不触面：
`sched_decision(cfg, is_sys_proc) -> SchedAction::{Skip, Start(&cfg)}`，shell（boot Step 2 /
19 接线）执行 `Start` → `sched_init_proc(cfg)` 并取回新调度器端点。

理由：
- **域拆分**：旧单体 trait 22 个方法把消息目标埋在方法名里。本拆分是对 域拆分 原草案（`PrivApi/SchedApi/PmApi/VmApi/IpcApi`）的显式修正：`privctl`/`getpriv` 的 C 面就是 `sys_*` 内核调用，单拆 `PrivApi` 会把一个传输面拆到两个 trait，对接线没有增量价值；而 sched 的传输目标确实是复合的（KERNEL→内核调用，否则→SCHED 消息），独立成面。对照 Redox：`redox_syscall` 每个 syscall 独立、用户态逻辑按 scheme 分层——域面是同一思想在 trait 上的表达。
- **依赖倒置**：boot 编排逻辑与 syscall 实现解耦；`minix-sys` 落地后实现各域面（接线归 19），测试用 `MockKernelApi`。
- **注入边界**：`KernelApi` 只在 shell 出现——boot 四步（本模块）、
  `RsServer.kernel` 持有者（lib.rs）与 19 接线层。纯决策模块（access/ipc_mask/sched/ready/
  recovery/monitor）不 import `KernelApi`：查询结果（`getnuid`/`getpriv`）由 shell 注入，
  命令（`privctl`/`sched_init_proc`/`setalarm`）由 shell 执行（monitor 模式，todo §13）。
  测试 mock 收敛为单一共享 `testutil::MockKernelApi`（E1，todo §13）。
- **fail-closed**：五个域面均无默认实现（编译期强制每个 impl 全量实现）；测试替身
  `UnimplementedKernelApi` 每个域面的每个方法返回 `Err(Errno::ENOSYS)`，**不 panic**；生产实现是 `TrapKernelApi`——RS 是 root system
  process，panic = 整机不可用（内核不重启 RS，`RSYS_F`）；返回 `Err` 让缺口在调用点可见且进程存活。
  入口 `main.rs` 对 boot 失败显式 panic（C 的 boot 错误同样 `panic`，main.c:226），不吞错误。

### 3.6 USE_LIVEUPDATE：cargo feature（对应 §2.3.7，ARCH feature 门控）

**C**：`#if USE_LIVEUPDATE`（main.c:436-491，`USE_LIVEUPDATE` 默认 yes，`share/mk/bsd.own.mk:1499`）。**Rust**：cargo feature `live-update`（默认关）：

```rust
#[cfg(feature = "live-update")]
fn self_update(&mut self, sys: &mut dyn KernelApi, update: &mut UpdateState) -> Result<(), Errno> { ... }
```

- 默认关的理由：RS 自升级依赖 `srv_fork`/`update_service`/`cleanup_service`/`vm_memctl` 等机制，其语义归 10/15/16/18；在机制落地前，feature 关闭时该流程**不编译**（fail-closed，不产生半吊子代码路径）。
- 打开 feature 后，`self_update` 内部按 §2.3.7 调用链编排（`clone_slot`/`srv_fork`/`update_service`/`cpf_reload`/`cleanup_service`/`vm_memctl pin` / `privctl SET_SYS`+`sched_init_proc`+`YIELD`），各步骤机制标注归属 10/15/16/18。
- feature 打开时 `init_fresh` 末尾调用 `self_update`；feature 默认关，因此默认 Rust boot 比 C 默认 yes 少这一尾步，这是当前默认构建的行为差异。

### 3.7 rinit grant 创建点的表达（对应 §2.3.1，grant 分工）

`rinit.rproctab_gid = cpf_grant_direct(...)`（main.c:185）是**创建点**（本文档语义），消费点在 12（`init_service` 把 gid 放入 RS_INIT 消息）。Rust 中 `RinitState` 字段 `rproctab_gid: Option<u32>`（mock 的 step0 grant id 为 1；生产由 `cpf_grant_direct` 返回，失败即 boot 失败）：

- 创建：**已落地**——`step0_prepare` 经 `IpcApi::grant_read`（`IpcApi` 面，实现在 `trap_api::TrapKernelApi` → `minix_sys::grant::GrantTable::grant_direct`）创建 `CPF_READ|ANY` 授权，覆盖 `RProcTable` 的 wire 镜像（`pub_wire_bytes()`，64×40 字节行，C 静态数组 `rprocpub[]` 的替身；表重置先于授权，因为 Rust 授权钉的是镜像地址，行内容由 step1 的 `sync_pub_wire` 填充）。
- 消费：12 的 `init_service` 读取该字段（12 文档实现）。
- 用 `Option<u32>` 表达"boot 已创建 / 未创建"状态；`GRANT_VALID` 的合法范围语义由创建点校验。

### 3.8 本章回顾

#### 小结

- `BootTables` 把 `sys_getimage` 的拷贝与三张静态表放在一个显式输入里。
- `boot_image_info_lookup` 的 `Result` 让缺失/错位在查找点失败。
- `SefCallbacks` 用 trait 表达 7 个回调，SEF 框架本体不在这层。
- `BootInit` 状态机保证 Step 1→4 的顺序，外部 syscall 留在 `KernelApi` 边界。
- 自升级只留调用点，机制归 18。

#### 术语

- **`BootTables`**：boot 输入（内核映像表 + 三张静态表）的 Rust 显式建模。
- **`LookupError` / `BootError`**：查找失败与 boot 失败的分类错误。
- **`SefCallbacks`**：RS 注册给 SEF 的回调 trait。
- **`BootInit`**：四步 boot 的状态机载体。
- **`KernelApi`**：内核/调度/PM/VM/IPC 外部调用的 trait 边界。

---

## 4. 实现详解

> 每个实现对应 §3 的设计决策。Rust 代码位置以函数名引用。

第 3 章定下了类型与边界。本章看它们在 `main.rs`、`lib.rs`、`table.rs`、`boot.rs`、`sef.rs` 里的实际落点；`dispatch.rs` 的主循环分类归 `06-rs-main-loop.md`，读完后应能把一次 boot 映射回具体函数。

### 4.0 本章要回答的问题

读者需要能回答：`main()` 怎样把 `RsServer` 构造出来；`BootInit` 的 Step 1/2/3/4 各落在哪个函数；`table.rs` 的静态表与 `boot.rs` 的查找如何配合。

#### 最小示例

```bash
$ grep -n 'fn main\|pub struct RsServer\|pub struct BootInit' os/servers/rs/src/main.rs os/servers/rs/src/lib.rs os/servers/rs/src/boot.rs
os/servers/rs/src/main.rs:42:fn main() {
os/servers/rs/src/lib.rs:129:pub struct RsServer {
os/servers/rs/src/boot.rs:878:pub struct BootInit<'a> {
```

#### 失败对照

如果先构造主循环、再补 boot 表，`BootInit` 的 Step 1 会读到未初始化的槽；当前入口先构造 `RsServer` 与 `BootTables`，再执行 `BootInit`，最后才进入 `dispatch`。

### 4.1 `os/servers/rs/src/main.rs`（对应 §3.3/§3.4）

```rust
fn main() {
    #[cfg(not(test))]
    {
        use minix_rs::{RsServer, SefInitType, boot::BootTables};

        // C: main.c:51 sef_local_startup() —— SEF 回调集是 RsServer 实现的
        // trait（§3.3），无独立注册值要构造。

        // C: main.c:196 sys_getimage() —— 从内核取 boot 映像真值。
        let tables = BootTables::acquire_from(
            &minix_sys::syscall::DirectKernelCallTransport,
        ).unwrap_or_else(|e| panic!("RS boot: sys_getimage failed: errno {e}"));

        let mut server = RsServer::new(tables);

        // C: sef_startup()→sef_cb_init_fresh()——main.c:151,158-494（§3.4 四步 boot）。
        // C 视 boot 失败为致命（main.c:226 panic）——boot 未完成不得进入主循环。
        if let Err(e) = server.init(SefInitType::Fresh) {
            panic!("RS boot failed: {e:?} (diagnostic: {:?})", server.boot_diagnostic());
        }

        if let Err(e) = server.run() { panic!("RS main loop terminated: {e:?}"); }
    }
}
```

- `BootTables::placeholder()` 只用于测试与宿主开发：使用 `table.rs` 静态三表 + 一个最小合法 `BootImage` 数组；生产入口用 `acquire_from()` 从内核 `GET_IMAGE` 取真实拷贝，失败即 panic。
- `RsServer::run()` 的主循环骨架见 06-rs-main-loop.md。

### 4.2 `os/servers/rs/src/lib.rs`（对应 §3.4，no_std 单线程模型）

```rust
#![cfg_attr(not(test), no_std)]
extern crate alloc;

pub mod table;      // §4.3：boot 三表（表静态化）
pub mod boot;       // §4.4：BootInit 四步状态机
pub mod sef;        // §4.5：SEF 回调注册表（SEF 回调抽象）
pub mod dispatch;   // 主循环分类，细节归 06
// 其余 pub mod（access/exec/ipc_mask/live_update/monitor/privilege/process_table/
// publish/query/ready/recovery/request/sched/self_lifecycle/service_create/
// service_slot/slot/state_data）为 02-19 对应模块的 forward-declare，共 22 个。

pub struct RsServer {
    boot: Option<boot::BootInit<'static>>,  // init(Fresh) 成功后消费
    state: Option<ServerState<'static>>,    // C 全局：rproc[]/system_hz/shutting_down/rinit
    kernel: Box<dyn KernelApi>,             // 生产默认 TrapKernelApi；测试可注入
}
impl RsServer {
    pub fn new(tables: BootTables<'static>) -> Self { ... }   // kernel = TrapKernelApi
    pub fn with_kernel(tables: BootTables<'static>, kernel: Box<dyn KernelApi>) -> Self { ... }
    pub fn init(&mut self, init_type: SefInitType) -> Result<i32, Errno> { ... }  // §3.4 分派
    pub fn state(&self) -> Option<&ServerState<'static>> { ... }  // 状态 handover 访问器
    pub fn run(&mut self) -> Result<(), Errno> { ... }   // 主循环骨架，细节归 06
}
```

- **单线程模型**：RS 是用户态服务器（执行模型：单线程事件循环），`!Send`/`!Sync` 合理，无跨 CPU 共享（`AGENTS.md` 执行模型约束）。`BootInit` 内用 `Vec`（alloc）而非 BSS 全局，避免 C 的 `rproc[]` 全局表带来的隐式依赖（四链指针 关联）。
- 模块划分理由：`table.rs`（数据）、`boot.rs`（启动编排）、`sef.rs`（注册表）（`dispatch.rs` 的分类归 06，不在本篇），与 plan §2 的 01 归属一致（Rust 模块列：`main.rs`、`lib.rs`——子模块均为 `lib.rs` 内部结构）。

### 4.3 `os/servers/rs/src/table.rs`（对应 §2.5，表静态化）

模块导出三个结构 + 三张静态表：

- `BootImagePriv { endpoint, label, flags }`——C `struct boot_image_priv`（type.h），flags 对应 `RSYS_F`/`VM_F`/`SRV_F`/`USR_F`（`minix3/minix/include/minix/priv.h` 的 `SRV_F`）。
- `BootImageSys { endpoint, flags }` / `BootImageDev { endpoint, dev_nr }`——sys/dev 表条目。
- 静态表常量：`BOOT_IMAGE_PRIV_TABLE`（12 项，对应 table.c:17-28，无哨兵——遍历即终止）、`BOOT_IMAGE_SYS_TABLE`（6 项 + `DEFAULT_SYS` 默认条目，对应 table.c:35-41）、`BOOT_IMAGE_DEV_TABLE`（2 项 + `DEFAULT_DEV`，对应 table.c:47-49）。

哨兵处理：C 的 `NULL_BOOT_NR`/`DEFAULT_BOOT_NR` 不再作为表内条目；`lookup_priv` 遍历失败即 `Err(LookupError::PrivTable)`，`lookup_sys`/`lookup_dev` 未命中返回默认条目（§3.2）。常量 `NULL_BOOT_NR`/`DEFAULT_BOOT_NR` 以 `pub const` 保留对齐（`99-rs-global-concepts.md`）。

### 4.4 `os/servers/rs/src/boot.rs`（对应 §2.3-§2.4，四步状态机）

- `BootTables<'a>`：§3.1。
- `LookupError` + 四个查找函数：§3.2（`lookup_image` 对应 main.c:723-733，`lookup_priv` 对应 main.c:738-748，`lookup_sys`/`lookup_dev` 对应 main.c:753-778）。
- `RinitState { rproctab_gid: Option<u32> }`：§3.7（创建点 `step0_prepare`，消费在 12）。
- `RProcTable`（内部 `Vec<ServiceSlot>`）承接 C 的 `rproc[]/rprocpub[]`；`ServiceSlot` 的完整字段语义归 02，Step 1 经 `activate_boot_slot` 建槽。
- `BootInit::init_fresh()`：四步编排（§3.4）。各步要点：
  - **与 C 的步骤数差异（步骤命名）**：C 的 `sef_cb_init_fresh` 是"前置 + Step 1~4"（前置在 §2.3.1，未编号）；Rust 显式命名为 `step0_prepare` + `step1_set_attrs`~`step4_finish` 共 5 个私有方法。机制步骤一一对应（无增减），差异仅为命名显式化（架构演进，无 C 行为偏移）。
  - `step0_prepare`：`get_hz`（`rs_verbose` 由 `main.rs` 的 `parse_rs_verbose` 读取后注入，不在 step0）、`RUPDATE_INIT` 等价（`RupdateState::default()`）、`sys_getimage` 等价（`tables.image` 已注入）→ 计数核对（`validate_tables`，对应 main.c:225-227）→ 表重置（`slots` 重建，对应 main.c:230-237）→ rproctab 授权创建（§3.7，main.c:185-189；Rust 侧表重置先行——授权钉的是 wire 镜像地址，行内容 step1 才填；已落地）。
  - `step1_set_attrs`：遍历 priv 表（对应 main.c:244），每项 `lookup_image/sys/dev` + 填充 `ServiceSlot` 属性；`endpoint == RS || endpoint == VM` 跳过 `privctl(SET_SYS)`（对应 main.c:285-291，RS/VM 例外）；其余经 `privctl(SET_SYS)` + `getpriv`。priv/send mask/call mask 的**完整构造**归 03/05，本模块只做调用编排。
  - `step2_allow_run`：遍历 priv 表；RS/VM → `init_service`（12 语义已接线——`mark_initializing` 三时间戳 + RS 自身 `ROOT_SYS_PROC` 早退不发消息、VM/普通服务经 `IpcApi::asynsend` 缝发出 `RS_INIT`，生产面 ENOSYS fail-closed、mock 记录全载荷）；普通服务 → `sched_init_proc` + `privctl(ALLOW)` + `init_service` + `SF_SYNCH_BOOT` 分支（同步 catch / 累积计数，对应 main.c:375-398）。
  - `step3_catch_init_ready`：循环调用 `catch_boot_init_ready`（12 机制，receive 缝已接线）。当前经 `IpcApi::receive` 只收 init-ready；`sef_receive_status` 对信号/控制消息的拦截未复刻，12 落地时补。
  - `step4_finish`：逐服务 `getnpid` 写 `pid`（对应 main.c:413-431）+ `setalarm(RS_DELTA_T)`（对应 main.c:433）。
  - `self_update`（`#[cfg(feature = "live-update")]`，`init_fresh` 末尾调用）：§3.6 调用链编排（clone_slot/srv_fork/update_service/cpf_reload/cleanup_service/vm_memctl/privctl YIELD，各步骤归属 10/15/16/18）。

### 4.5 `os/servers/rs/src/sef.rs`（对应 §2.2，SEF 回调抽象）

- `SefInitType::{Fresh, Lu, Restart}` + `SefInitInfo`（对应 `sef_init_info_t`，sef.h:53；字段语义在 12 展开）。
- `SefCallbacks` 7 方法 trait（§3.3），由 `RsServer` 实现（lib.rs）；不再有
  `local_startup()`/`startup()`——`RsServer::init(init_type)` 对应 `sef_startup()` 的分派：
  启动期按回调参数 `type`（RS 由 `do_sef_rs_init` 本地构造）调 `init_fresh`/`init_lu`/
  `init_restart`。**消息接收机制归 12**（RS 主循环的 `RS_INIT` 分支）；本模块只定义分派类型。

### 4.6 本章回顾

#### 小结

- `main.rs` 构造 `RsServer` 并执行 `BootInit`，最后进入主循环。
- `lib.rs` 持有 `RsServer`/`ServerState`，保持 no_std 单线程模型。
- `table.rs` 用静态切片复刻 table.c 的三张表。
- `boot.rs` 承载四步状态机与查找；`sef.rs` 承载回调 trait。
- handler 机制不在本篇。

#### 术语

- **`RsServer`**：RS 的运行时对象。
- **`ServerState`**：boot 完成后的可查询状态。
- **`BootImagePriv/Sys/Dev`**：三张静态表的条目类型。
- **`SefCallbacks`**：7 个回调的 trait。
- **`KernelApi`**：外部调用边界。

---

## 5. 测试要点

> 基线：`cargo test -p minix-rs --lib` = **359 passed / 0 failed**（当前工作树，全 crate）。01 范围三模块 42 项（boot 37 + table 3 + sef 2）；dispatch 8 项归 06；全 crate 另有 2 个 `live-update` feature-gated 测试，默认不运行。测试覆盖四步 boot 顺序、表查找、SEF 回调 fail-closed、dispatch 分类。

启动链的正确性不能只靠读代码。本章看 `table.rs`、`boot.rs`、`sef.rs` 的测试覆盖了哪些形状、哪些顺序、哪些负控，并给出当前基线。

### 5.0 本章要回答的问题

读者需要能回答：测试覆盖了四步 boot 的哪些顺序断言；lookup/SefCallbacks 的负控有哪些；当前基线是多少；哪些行为只能由真机门证明。

#### 最小示例

```bash
$ cd os && cargo test -p minix-rs --lib
# 当前基线：359 passed / 0 failed
```

#### 失败对照

如果把历史读数当现状，会误报缺口或掩盖回归；测试计数必须在当前工作树上重跑。

### 5.1 table.rs 测试（§2.5，表静态化）

| 测试 | 覆盖 |
|------|------|
| `test_priv_table_matches_c` | priv 表 12 项 + 顺序与 table.c:17-28 一致（RS/VM/PM/SCHED/VFS/DS/TTY/MEM/MIB/PFS/MFS/INIT）+ flags 类别 |
| `test_sys_table_has_default` | sys 表 6 项 + 默认条目与 table.c:35-41 一致 |
| `test_dev_table_has_default` | dev 表 2 项 + 默认条目与 table.c:47-49 一致 |

### 5.2 boot.rs 测试（§2.3-§2.4，四步状态机 + 四表查找）

| 测试 | 覆盖 |
|------|------|
| `test_init_fresh_step_order` | `MockKernelApi` 记录调用序列：step1 privctl(SetSys)×10 + getpriv×12 → step2 sched×10+Allow×10；Step 3 空邮箱 fail-closed（`Err(Kernel(ENOSYS))`），Step 4 的 getnpid/setalarm 不执行 |
| `test_step1_skips_privctl_for_rs_vm` | RS/VM 跳过 `privctl(SetSys)`（main.c:285-291 例外） |
| `test_validate_tables_mismatch` | image 表与 priv 表系统服务数不一致 → `Err(BootError::CountMismatch)`（错误域二分 类型化；对应 main.c:225-227 panic） |
| `test_validate_tables_rejects_proc_nr_endpoint_mismatch` | 单行 `proc_nr != endpoint.slot()` → `Err(BootError::EndpointMismatch)` |
| `test_boot_error_wire_mapping` | 错误域二分 线面映射：`Kernel(e)` 保留原 errno，四类不变式违例 → `EINVAL`（`ENOSYS` 专属"未接线"） |
| `test_boot_img_truncates_long_name` | >16 字节名称钳制到字段 |
| `test_lookup_image_not_found` | `lookup_image` 未命中 → `LookupError::ImageTable`（对应 main.c:731 panic） |
| `test_lookup_priv_found` / `test_lookup_priv_not_found` | `lookup_priv` 命中 / 未命中 → `LookupError::PrivTable`（对应 main.c:746） |
| `test_lookup_sys_default_fallback` / `test_lookup_dev_default_fallback` | sys/dev 未命中返回默认条目（对应 main.c:753-762,768-777） |
| `test_placeholder_tables_valid` | `BootTables::placeholder()` 通过 `validate_tables()`（对应 main.c:200-237） |
| `test_step0_creates_rproctab_grant_over_wire_mirror` | step0 对 wire 镜像创建 `cpf_grant_direct` 授权（main.c:185） |
| `test_unimplemented_kernel_api_fails_closed` | 测试替身 `KernelApi` 全接口 fail-closed（`Err(ENOSYS)`） |
| `test_domain_face_implementable_in_isolation` | 测试 double 只实现单个域面（`SysApi`）即可经 `&mut dyn SysApi` 使用——单体 trait 时代每个 shell 测试被迫全量 22 方法 |
| `test_init_fresh_populates_table` | Step 1 后 12 个 boot 服务占 slot 0..11 且 `IN_USE|ACTIVE`，索引命中；非 boot slot 保持空闲（对应 main.c:244-346） |
| `test_step4_sets_pid` | Step 4 每个 boot slot 携带 `getnpid` 返回的 pid（对应 main.c:426；mock 返回 100） |
| `test_step4_negative_pid_reports_invalid_pid` | 错误域二分：`getnpid` 负值 → `Err(BootError::InvalidPid(-1))`，线面 `EINVAL`（对应 main.c:427-429 panic） |
| `test_boot_failure_propagates_from_step0` | `fail_calls` 注入 `get_machine` 失败 → `Err(BootError::Kernel(ENOSYS))` 且后续步骤零调用（C 无恢复路径，main.c:53） |
| `test_step1_lookup_failure_propagates` | 计数一致但成员错位的表 → step1 `Err(BootError::Lookup(ImageTable))`（C 无 miss 分支，main.c:253-254→731） |
| `test_boot_failure_propagates_setalarm` | 末步 setalarm 失败 → 整个 boot 失败，且闹钟调用已发出（C 检查返回值并 panic，main.c:433-434） |
| `test_step3_fails_closed_when_init_ready_pending` | 有未收 init-ready 时 Step 3 fail-closed（`Err(BootError::Kernel(ENOSYS))`，对应 main.c:401-407 的阻塞 receive 语义） |
| `test_step2_synch_boot_fails_closed` | `SF_SYNCH_BOOT` 服务同步 catch 未接线时 fail-closed（`Err(BootError::Kernel(ENOSYS))`，对应 main.c:390-392），不得静默跳过 sync |
| `test_step2_init_service_marks_and_sends` | step2 全分支经 `init_service`——三槽 `INITIALIZING` + `alive_tm`/`check_tm`（utility.c:19-21）；RS 自身零发送（ROOT_SYS_PROC 早退，utility.c:29-31）、VM/PM 各收一条 `RS_INIT`（逐字段断言：FRESH/gid=1/old=NONE/restarts=1）；计数 = 2（main.c:362-399） |
| `test_step3_vm_init_roundtrip_clears_initializing` | step2 发出 → mock inbox 脚本 VM 的 ready 回包 → step3 catch 清 `INITIALIZING`、计数归零，且对 VM 零同步 reply（main.c:812-815） |
| `test_rs_server_handover_after_fresh_init` | fresh boot 完成后运行时状态归 server 所有（`state()` 可达 table/hz/shutting_down），machine 快照随 boot→run 交接存活；boot 机器被消费（无双重所有权） |
| `test_boot_slot_populates_s2_fields` | boot slot 携带 cmd/args/argc/vm_call_mask/scheduler/priority/quantum/alive_tm（对应 main.c:308-333，07/09/10 依赖） |

### 5.3 sef.rs / dispatch.rs 测试（§2.2/§2.1，SEF 回调抽象）

| 测试 | 覆盖 |
|------|------|
| `test_deferred_callbacks_fail_closed` | 12/18/06 未接线的回调（`init_restart`/`init_lu`/`init_response`/`lu_response`/`signal_manager`）全部 fail-closed（`Err(ENOSYS)`）；补充：经失败内核面的 fresh boot 线面保持 `ENOSYS` 且 `boot_diagnostic()` 携带 `BootError::Kernel(ENOSYS)` |
| `test_classify_clock_notify` | `is_notify` + `CLOCK` → `ClockNotify`（对应 main.c:80-83） |
| `test_classify_heartbeat_notify` | 非 CLOCK 通知 → `HeartbeatNotify { source, timestamp }`（对应 main.c:85-91） |
| `test_classify_heartbeat_carries_timestamp` | 心跳 timestamp 穿过分类原样携带 |
| `test_classify_ready` | `RS_INIT`/`RS_LU_PREPARE` → `InitReady`/`LuPrepareReady`（对应 main.c:116-117） |
| `test_classify_request` / `test_classify_request_unknown` | 请求分类；未知调用号 → `dispatch_request` 返回 `ENOSYS`（对应 main.c:118-121） |
| `test_dispatch_result_reply_suppression` | `DispatchResult(EDONTREPLY)` 抑制 reply 路径（对应 main.c:124-129） |
| `test_rs_constants_match_c` | 15 个 RS 消息常量与 com.h:465-482 一致 |

### 5.4 测试总数

`cargo test -p minix-rs --lib` 实测 **359 passed / 0 failed**。全 crate 的 `#[test]` 属性共 361 个，其中 2 个受 `live-update` feature 门控、默认不运行。01 范围三模块共 42 项：boot.rs 37、table.rs 3、sef.rs 2；dispatch.rs 8 项归 06；其余为 02–19 模块的测试。

### 5.5 本章回顾

#### 小结

- `table.rs` 测试校验三张表的条目与顺序。
- `boot.rs` 测试覆盖四步顺序、lookup 负控与失败传播。
- `sef.rs`/`dispatch.rs` 测试覆盖回调 fail-closed 与分类。
- 当前 `minix-rs` 宿主基线是 359 passed / 0 failed。
- 宿主测试证明形状与负控；真机 boot 终点仍需独立门。

#### 术语

- **宿主测试**：`cargo test` 在当前工作树上跑的逻辑/形状/负控测试。
- **负控**：故意构造非法输入、要求代码拒绝或 panic 的测试。
- **基线读数**：当前工作树上重跑的 passed/failed 计数。
- **真机门**：QEMU/真板按串口路标判定的启动验证。

---

## 6. 过渡

本文档回答了 RS 启动链的第一个问题：**main() → sef_local_startup() → sef_cb_init_fresh() 四步 boot → 主循环入口**。

- 启动链的下一步：进程模型——`02-rs-process-table.md`（`rproc`/`rprocpub` 结构语义、`rproc_ptr` 快速索引、`rs_isokendpt`，主循环 caller 验证依赖它）。
- Step 1 的机制：`03-rs-privilege.md`（priv 结构 + `sys_privctl` 每操作效果）、`05-rs-ipc-sendmask.md`（`fill_send_mask`）；`04-rs-access-control.md` 是运行时请求的权限面（与 boot 相对）。
- Step 2/3 的机制：`12-rs-init-run.md`（`init_service`/`catch_boot_init_ready`/`do_init_ready`、`rproctab_gid` 消费点）。
- Step 4 的机制：`07-rs-period-heartbeat.md`（`sys_setalarm` 之后的周期检查与心跳）。
- 主循环：`06-rs-main-loop.md`（`get_work`/`reply`/`EDONTREPLY`/`rs_idle_period` 全量语义）。
- RS 自升级：`18-rs-self-lifecycle.md`（USE_LIVEUPDATE 流程细节）、`16-rs-live-update.md`（update_service 状态机）。
- 外部接口：`19-rs-external-interfaces.md`（`sys_getmachine`/`sys_getimage`/`sys_privctl`/`getnpid` 等签名）。

阅读顺序建议：00 → 01 → 02 → 03 → 05 → 12 → 06 → 07 → 08 → 09 → 10 → 11 → 13 → 14 → 15 → 16 → 17 → 18 → 19 → 99；04 是访问控制运行时面，可在 03 之后读。

---

## 7. 参见

- `rewrite-notes/03-stage-rs/00-rs-overview.md` — RS 总览与启动主线图
- `rewrite-notes/03-stage-rs/plan.md` §1.2/§3.1/§4 SEF 回调抽象/feature 门控/表静态化/§5.3 — 背景计划，不是本篇的权威来源；权威是 C 源码与本篇引用的 Rust 代码
- `rewrite-notes/02-stage-vm/01-vm-init-main.md` — 同流程先例（VM 启动链，BootParams/占位注入策略）
- `rewrite-notes/01-stage-kernel/09-vm-boot-protocol.md` — 内核侧 boot 链（RS 启动的前置）
- `rewrite-notes/01-stage-kernel/22-privilege.md`、`23-ipc-filter.md` — kernel 侧 priv/IPC filter 语义（Step 1 的 `sys_privctl`/send mask 依赖）
- `minix3/minix/servers/rs/main.c` — 本文档 ground truth
- `minix3/minix/servers/rs/table.c` — boot 三表 ground truth
- `minix3/minix/include/minix/sef.h` — SEF 回调/类型定义（`SEF_INIT_*`/`sef_init_info_t`）
- `minix3/minix/include/minix/rs.h`、`minix3/minix/include/minix/priv.h` — `SF_*`/priv flags（`SRV_F`/`RSYS_F`/`VM_F`/`USR_F`）
- `os/servers/rs/src/` — Rust 实现（本文档对应 `main.rs`/`lib.rs` + `table`/`boot`/`sef`/`dispatch` 子模块）
- `os/libs/minix-sys/src/lib.rs` — syscall 库；`KernelApi` 的生产实现由 `TrapKernelApi` 提供
