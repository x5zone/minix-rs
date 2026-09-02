# 01-vfs-init-main: 启动入口与初始化骨架

> **状态**: 生效中（2026-08-17 完整改写）
> **定位**: 阶段 1 — 启动入口与进程模型（锚点文档）
> **源码**: `main.c:54-141,303-499,501-553`（main/SEF 三回调/VFS_PM_INIT 握手/do_init_root/lock_proc）+ `worker.c:27-31,162-185`（worker_init/worker_allow 调用点）+ `mount.c:391-431`（mount_pfs 调用点）+ `com.h:513-551`（VFS_PM_INIT 协议面）
> **Rust 模块**: `os/servers/vfs/src/main.rs`、`os/servers/vfs/src/main_loop.rs`（`VfsState::init_fresh/pm_handshake_step/finish_init/do_init_root`）、`os/libs/minix-types/src/ipc/vfs.rs`（`VfsPmInit` 编解码）
> **draft 素材**: `draft/10-main-loop.md` 启动部分（素材）

---

## 1. 概念：启动链——VFS 如何建立"文件系统世界"

### 1.0 章节引言

**目标读者**：已理解 Minix3 微内核基本结构（内核/系统服务/用户进程分层、IPC 消息传递）、boot image 概念（`01-stage-kernel/06-proc-init-boot-proc.md`）与 PM 的进程表初始化（`../04-stage-pm/01-pm-init-main.md`）的开发者。

> **本章不讲什么**：
> - `fproc` 结构字段语义与标志位（`02-fproc-struct.md`）
> - 进程表查找、endpoint 验证、槽位管理（`03-fproc-table.md`）
> - filp/vnode/vmnt 表结构字段（`04~06`）
> - 三级锁原语（`07-tll-lock.md`）
> - worker 池调度细节与阻塞原语（`08-worker-thread.md`）
> - 主循环五路分发与 call_vec 表（`09-main-loop.md`）
> - `service_pm` 各请求内容（`10-pm-protocol.md`）
> - `mount_fs` 内部机制与 FS 通信协议（`18-mount.md`、`11-fs-comm.md`）
>
> 本章只回答一个问题：**VFS 按什么顺序、为什么按这个顺序，把自己初始化成"文件系统语义的权威"，然后才允许外部请求进入**。

### 1.1 VFS 在启动链中的位置：文件系统世界的入口

内核完成自举后，按 boot image 启动 PM 与 VFS（`01-stage-kernel/06-proc-init-boot-proc.md`）。PM 是"进程语义的权威"（进程表 `mproc`），VFS 是"文件系统语义的权威"——它是所有路径解析、文件描述符、挂载点、设备 I/O 的**唯一入口**：用户进程不直接与 MFS/PFS/驱动通信，一切经 VFS 转发。

这带来一个核心问题：**VFS 的初始状态从哪来？** 三件事 VFS 自己不知道：

1. **进程清单**——每个用户进程对应哪个 endpoint/槽号/PID？VFS 需要自己的进程表（`fproc`）与 PM 的 `mproc` 对齐（同一进程在 PM 有一个槽、在 VFS 有一个槽，靠 endpoint 关联）。
2. **硬件/服务环境**——时钟频率（`system_hz`）、boot image 里有哪些文件系统服务/驱动需要映射进设备表（`dmap`/`smap`）。
3. **根文件系统**——`/` 从哪来？`/` 是 MFS 挂载的 boot ramdisk，在这之前任何路径解析都会失败。

于是 VFS 的启动链是一个**依赖链**：先与 PM 对齐进程表，再读取内核/RS 提供的服务清单，然后初始化自己的核心表，最后挂载根文件系统，才进入主循环接收请求。

### 1.2 启动依赖链图

```
内核 boot image（编译时进程清单）
  │
  ▼  main.c:54  main()
  └─ sef_local_startup()                ← §2.2：注册 SEF 回调（init_fresh/lu_prepare/init_lu）
       └─ sef_startup() → sef_cb_init_fresh()   main.c:393
            ├─ ① fproc 槽清零（endpoint=NONE / pid=PID_FREE）        main.c:405-408
            ├─ ② VFS_PM_INIT 握手循环（收 PM 消息填槽，NONE 终止）   main.c:410-436
            ├─ ③ system_hz = sys_hz()                               main.c:438
            ├─ ④ ds_subscribe("drv\\.[bc]..\\..*")（驱动事件）       main.c:441
            ├─ ⑤ worker_init()（9 worker 线程）                     main.c:445
            ├─ ⑥ bsf_lock 互斥锁                                    main.c:448
            ├─ ⑦ init_dmap() / init_smap()（设备表/套接字表）        main.c:451-453
            ├─ ⑧ sys_safecopyfrom(rproctab) + map_service()（服务映射）main.c:455-467
            ├─ ⑨ fp_lock + filp/rd/wd 清零（第二遍 fproc 循环）      main.c:468-483
            ├─ ⑩ init_vnodes() / init_vmnts() / init_select() / init_filps()  main.c:485-489
            └─ ⑪ worker_start(do_init_root)                          main.c:492-497
                 └─ do_init_root()                                    main.c:501-523
                      ├─ worker_allow(FALSE)（拒绝新请求）            main.c:507
                      ├─ mount_pfs()                                  main.c:510
                      ├─ mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR)  main.c:516-517
                      └─ worker_allow(TRUE)（恢复接受请求）            main.c:522
```

顺序背后的不变量：

- **先对端后本地**：`fproc` 表必须先与 PM 对齐（②），VFS 才知道每个 endpoint 对应谁；`worker_allow(FALSE)` 门控保证在此之前（⑪ 挂载期间）外部请求进不来。
- **先基础设施后核心表**：worker（⑤）、设备表（⑦）、服务映射（⑧）都是核心表（⑩）与后续运行的前置。
- **根挂载最后**：只有根文件系统就绪，路径解析才有意义；挂载期间拒绝请求防止 `init(8)` 在 `/` 可用前发起文件操作。

### 1.3 VFS_PM_INIT 握手：两张进程表如何对齐

PM 与 VFS 各自维护进程表（PM 的 `mproc` / VFS 的 `fproc`），靠 endpoint 关联。**PM 填充一个 boot 进程，就发一条 `VFS_PM_INIT` 消息给 VFS**（槽号 + PID + endpoint），VFS 据此创建对应的 `fproc` 槽。全部发完后，PM 发**最后一条 `VFS_PM_INIT`（endpoint = NONE）并同步等待回复**——这是握手屏障（对端状态机详见 `../04-stage-pm/01-pm-init-main.md` §2.1 第 6 步）：

- 逐条 `sef_receive`（阻塞收，main.c:415-434）：每条填一个槽。
- 末条 `endpoint = NONE` 终止（main.c:428-431）：表示"没有更多系统进程"。
- `ipc_send(PM, OK)` 同步（main.c:435-436）：告诉 PM "我处理完了，你可以继续"。

为什么 VFS 不主动向 PM 要清单？因为 **PM 是权威**——PM 从内核拷了 boot image 并实例化 mproc 表，VFS 只需被动接收；这也避免了 VFS 在 PM 就绪前发起请求造成互相等待。这种"**先倾倒全部数据，再一次性确认**"的表同步模式，是微内核服务间启动对齐的经典做法（VM 与内核的 boot 协议同理，`../02-stage-vm/01-vm-init-main.md`）。

每条消息填槽时还会写入 boot 进程的初始身份（main.c:419-425）：`uid/gid = SYS_UID/SYS_GID (0)`、`umask = ~0`、`FP_NOFLAGS`、`FP_BLOCKED_ON_NONE`。**系统进程与 INIT 一律 root 身份启动**——这是 const.h:16-17 的显式设计（`SYS_UID` 注释 "uid_t for system processes and INIT"）。

### 1.4 为什么根挂载要门控

`do_init_root`（main.c:501-523）挂载 PFS 与根文件系统。挂载涉及与 MFS/PFS 的 IPC 往返（`req_readsuper` 等，归 18），期间如果 `init(8)` 的打开文件请求进来，路径解析会撞上"根还没就绪"。所以 C 用 `worker_allow(FALSE)`（main.c:507）把请求**挡在 worker 分配之前**：新请求标 `FP_PENDING`（worker.c:176-184），挂载完成后再 `worker_allow(TRUE)`（main.c:522）把它们放行。这是"**服务未就绪时不假装可用**"的启动期门控模式。

### 1.5 本章小结

VFS 的启动链回答了三件事：

1. **进程表对齐**：VFS_PM_INIT 逐条 + NONE 屏障，让 `fproc` 与 PM 的 `mproc` 一致。
2. **依赖顺序**：进程表 → 时钟/订阅 → worker/设备表/服务映射 → 核心表 → 根挂载。
3. **就绪门控**：根挂载期间拒绝请求，避免"半初始化"状态暴露给外部。

---

## 2. C 源码分析

### 2.1 main()：入口与主循环框架（main.c:54-141）

```c
int main(void)
{
  int transid;
  struct worker_thread *wp;

  /* SEF local startup. */
  sef_local_startup();
  printf("Started VFS: %d worker thread(s)\n", NR_WTHREADS);

  /* main loop: get_work → dispatch → reply, never terminates */
  while (TRUE) {
	worker_yield();	/* let other threads run */
	send_work();
	if (!get_work()) continue;
	/* ...五路分发：FS 回复 / PM / 通知 / 驱动回复 / 普通 syscall... */
  }
}
```

要点：`main()` 本体极薄——**真正的初始化全在 SEF 回调里**。`sef_local_startup()` 注册回调后，`sef_startup()` 决定走哪条初始化路径（fresh/LU/restart），初始化完成返回后 `main()` 才进入主循环。主循环的五路分发细节归 `09-main-loop.md`，这里只记录它的形状：`worker_yield()` 让出、`send_work()` 处理 PM 延迟请求、`get_work()` 收消息。

### 2.2 sef_local_startup()：SEF 回调注册（main.c:374-389）

```c
static void sef_local_startup(void)
{
  sef_setcb_init_fresh(sef_cb_init_fresh);
  sef_setcb_init_restart(SEF_CB_INIT_RESTART_STATEFUL);
  sef_setcb_init_lu(sef_cb_init_lu);
  sef_setcb_lu_prepare(sef_cb_lu_prepare);
  sef_setcb_lu_state_changed(sef_cb_lu_state_changed);
  sef_setcb_lu_state_isvalid(sef_cb_lu_state_isvalid_standard);
  sef_startup();
}
```

SEF（Server Event Framework）是 Minix3 服务端的生命周期框架：**注册回调 → `sef_startup()` 状态机按启动类型（fresh/LU/restart）调用对应回调**。对 VFS 而言，`sef_cb_init_fresh` 是唯一真正走完整初始化路径的入口；LU 三回调（§2.4）只处理 live update 期间的 worker 线程收尾/重建，restart 复用 stateful 恢复（minix-rs 均未实现，见 §3.1 D1 的 DEFERRED 标注）。

### 2.3 sef_cb_init_fresh()：九段初始化链（main.c:393-499）

这是本篇的核心。函数体按依赖顺序分为九段：

**段 1（main.c:405-408）——fproc 槽清零**：

```c
for (rfp = &fproc[0]; rfp < &fproc[NR_PROCS]; rfp++) {
	rfp->fp_endpoint = NONE;
	rfp->fp_pid = PID_FREE;
}
```

BSS 全局虽然初始为零，但显式清零有两个原因：语义自明（"这些槽未使用"）与 restart 路径复用（LU/restart 后表是脏的）。

**段 2（main.c:410-436）——VFS_PM_INIT 握手**（§1.3 已述协议形状）：

```c
do {
	if ((s = sef_receive(PM_PROC_NR, &mess)) != OK)
		panic("VFS: couldn't receive from PM: %d", s);
	if (mess.m_type != VFS_PM_INIT)
		panic("unexpected message from PM: %d", mess.m_type);
	if (NONE == mess.VFS_PM_ENDPT) break;

	rfp = &fproc[mess.VFS_PM_SLOT];
	rfp->fp_flags = FP_NOFLAGS;
	rfp->fp_pid = mess.VFS_PM_PID;
	rfp->fp_endpoint = mess.VFS_PM_ENDPT;
	rfp->fp_blocked_on = FP_BLOCKED_ON_NONE;
	rfp->fp_realuid = (uid_t) SYS_UID;
	rfp->fp_effuid = (uid_t) SYS_UID;
	rfp->fp_realgid = (gid_t) SYS_GID;
	rfp->fp_effgid = (gid_t) SYS_GID;
	rfp->fp_umask = ~0;
} while (TRUE);
mess.m_type = OK;
s = ipc_send(PM_PROC_NR, &mess);	/* send synchronization message */
```

注意三个失败语义：`sef_receive` 失败 panic、**消息类型不对 panic**（协议外的消息在启动期没有容忍余地）、**`fproc[mess.VFS_PM_SLOT]` 无界数组访问**（PM 是可信对端，槽号约定在 `[0, NR_PROCS)`）。Rust 侧把最后一个改为显式范围检查（§3.2 D2，fail-closed）。

**段 3（main.c:438）——系统时钟**：`system_hz = sys_hz();` 内核 syscall 查询时钟频率，供 select 定时器等使用（值语义归 99）。

**段 4（main.c:441）——DS 驱动事件订阅**：`ds_subscribe("drv\\.[bc]..\\..*", DSF_INITIAL | DSF_OVERWRITE);` 订阅所有块/字符驱动的注册/注销事件——模式匹配 `drv.blk.*`/`drv.chr.*` 命名族（块驱动发布前缀 `drv.blk.`，libblockdriver/driver.c:101；字符驱动前缀 `drv.chr.`，libchardriver/chardriver.c:105），驱动上下线时 VFS 会收到 DS 通知（ARCH A-14，见 §3.4）。

**段 5-6（main.c:445-448）——worker 与全局锁**：`worker_init()` 创建 9 个 mthread worker 线程（`NR_WTHREADS`，细节归 08）；`mthread_mutex_init(&bsf_lock)` 初始化块特殊文件锁（归 07/16）。

**段 7（main.c:451-453）——设备表**：`init_dmap()` 初始化块/字符设备映射表，`init_smap()` 初始化套接字驱动表（细节归 19）。

**段 8（main.c:455-467）——服务映射**：

```c
sys_safecopyfrom(RS_PROC_NR, info->rproctab_gid, 0,
		 (vir_bytes) rprocpub, sizeof(rprocpub));
for (i = 0; i < NR_BOOT_PROCS; i++) {
	if (rprocpub[i].in_use) {
		if ((s = map_service(&rprocpub[i])) != OK)
			panic("VFS: unable to map service: %d", s);
	}
}
```

RS 在 `sef_init_info_t` 里给了 rproctab 的 grant，VFS 用 `sys_safecopyfrom` 拷到本地 `rprocpub[NR_BOOT_PROCS]`，然后逐个 `map_service()` 把 boot image 里的文件系统/驱动服务映射进设备表（dmap 的 endpoint 表项）。这确立了"**哪些 endpoint 是 FS/驱动**"的初始知识。

**段 9（main.c:468-483）——第二遍 fproc 循环**：为每个槽初始化 `fp_lock` 互斥锁（可睡眠锁，§2.6）、`fp_worker = NULL`，清空 `fp_filp[OPEN_MAX]` 与 `fp_rd/fp_wd`（进程目录指针）。注释明确说明：`mount_fs` 稍后会设置正确的目录值。

**段 10（main.c:485-489）——核心表**：`init_vnodes()` / `init_vmnts()` / `init_select()` / `init_filps()` 初始化 vnode 表、挂载表、select 结构与 filp 表（各自结构归 04~06、23）。

**段 11（main.c:492-497）——启动根挂载**：

```c
worker_start(fproc_addr(VFS_PROC_NR), do_init_root, &mess /*unused*/,
	FALSE /*use_spare*/);
```

`worker_start` 把 `do_init_root` 作为第一个 worker 任务调度（VFS 进程自己作为 `w_fp`）。根挂载由此在 worker 上下文里执行（§2.5）。

### 2.4 SEF 生命周期：live update 三回调（main.c:303-373）

```c
static int sef_cb_lu_prepare(int state)          /* main.c:303 */
{
  switch (state) {
  case SEF_LU_STATE_REQUEST_FREE:
  case SEF_LU_STATE_PROTOCOL_FREE:
	if (!worker_idle()) { printf("VFS: worker threads not idle, blocking update\n"); break; }
	worker_cleanup();
	return OK;
  }
  return ENOTREADY;
}

static void sef_cb_lu_state_changed(int old_state, int state)  /* main.c:330 */
{
  if (state != SEF_LU_STATE_NULL) return;
  switch (old_state) {
  case SEF_LU_STATE_REQUEST_FREE:
  case SEF_LU_STATE_PROTOCOL_FREE:
	worker_init();   /* 失败回滚后重建 worker */
  }
}

static int sef_cb_init_lu(int type, sef_init_info_t *info)     /* main.c:352 */
{
  if ((r = SEF_CB_INIT_LU_DEFAULT(type, info)) != OK) return r;  /* 常规状态迁移 */
  switch (info->prepare_state) { ... worker_init(); }
  return OK;
}
```

live update 对 VFS 的核心难点是 **worker 线程栈**——9 个真实线程的栈无法做状态迁移，所以 LU 期间 shutdown worker（`lu_prepare`）、迁移后重建（`init_lu`/`state_changed`）。这三回调在 minix-rs 均未实现（DEFERRED，§3.1），只记录语义：**LU 的协议约束是"request-free / protocol-free 状态下 worker 必须全部空闲"**。

### 2.5 do_init_root()：根挂载与 worker 门控（main.c:501-523）

```c
static void do_init_root(void)
{
  char *mount_type, *mount_label;
  int r;

  /* Disallow requests from e.g. init(8) while doing the initial mounting. */
  worker_allow(FALSE);                  /* main.c:507 */

  mount_pfs();                          /* main.c:510 */
  mount_type = "mfs";
  mount_label = "fs_imgrd";
  r = mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR, 0, mount_type,
	mount_label);                        /* main.c:516-517 */
  if (r != OK)
	panic("Failed to initialize root");

  worker_allow(TRUE);                   /* main.c:522 */
}
```

- `worker_allow(FALSE)`（main.c:507）：把全局 `block_all` 置真，此后新请求标 `FP_PENDING` 排队（worker.c:162-185，细节归 08）。
- `mount_pfs()`（mount.c:391-431）：把 PFS（管道文件系统）当作普通文件系统挂载——分配 nonedev、vmnt 项、发 mount 请求，让 PFS 进 vmnt 表（细节归 18）。
- `mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR, 0, "mfs", "fs_imgrd")`：根文件系统是 **MFS 上的 boot ramdisk**（设备 `0x0106`，`dmap.h:113`），标签 `fs_imgrd`（RS 里的 boot 服务名）。注释 `FIXME: use boot image process name instead` / `FIXME: obtain this from RS` 表明挂载参数目前硬编码。
- `worker_allow(TRUE)`（main.c:522）：根就绪，放行 pending 请求。

### 2.6 lock_proc/unlock_proc：可睡眠锁（main.c:528-553）

```c
void lock_proc(struct fproc *rfp)
{
  int r;
  struct worker_thread *org_self;

  r = mutex_trylock(&rfp->fp_lock);
  if (r == 0) return;

  org_self = worker_suspend();       /* 让出 worker，等锁释放 */
  if ((r = mutex_lock(&rfp->fp_lock)) != 0)
	panic("unable to lock fproc lock: %d", r);
  worker_resume(org_self);
}
```

`fp_lock` 是**可睡眠互斥锁**：拿不到锁时 `worker_suspend()` 把当前 worker 线程挂起（腾出位置给其他请求），锁释放后再 `worker_resume`。这是 VFS 多线程模型的核心原语之一——`mutex_trylock` 快路径 + `worker_suspend` 慢路径。锁原语细节归 `07-tll-lock.md`，本篇只记录调用点与"可睡眠"语义（单线程演进后锁降级为借用规则，§3.4 D4）。

---

## 3. Rust 设计决策

> 以下为正文自包含的设计决策（Step 0.3 嵌入生成，与代码三处一致标注 ARCH；设计快照为中间产物，正式文档不引用）。

### 3.1 D1：SEF 框架消除——`VfsState::init_fresh()` 直连启动链

- **C**：`sef_local_startup()` 注册 5 回调 + `sef_startup()` 状态机（sef.c），按启动类型分发到 `sef_cb_init_fresh`。
- **Rust**：`VfsState::init_fresh()` 直接承载启动链（等价 `sef_cb_init_fresh` 的函数体）；`run()` 内先 `VfsState::new()` 再 `init_fresh()`。与 02-stage-vm 的 D4（`rs_handshake` 直连）同型：**保留协议语义、去掉 setcb 注册 + startup 状态机**。
- **理由**：启动框架的"注册 → 状态机 → 回调"间接层，在单进程单入口下没有信息增益；直接调用让启动链可审计、可测试。VFS 没有 VM 那样的 is_first_time 门控——fresh/LU/restart 三路径中只有 fresh 被实现。
- **DEFERRED（fail-closed）**：`sef_cb_lu_prepare`/`sef_cb_lu_state_changed`/`sef_cb_init_lu`（main.c:303-373）与 `SEF_CB_INIT_RESTART_STATEFUL` 均未实现——live update / restart 是独立特性，不假装支持；`init_fresh` 用 `assert!(!initialized)` 禁止二次初始化（与 PM 侧 `PmServer` 同款契约）。
- **行为契约**：`finish_init()` 前必须完成握手（`assert!(boot_phase == InitTables)`），`run()` 前必须 `init_fresh()`。

### 3.2 D2：VFS_PM_INIT 类型化——`VfsPmInit` 进 minix-types

- **C**：`mess.VFS_PM_ENDPT(m7_i1)/VFS_PM_SLOT(m7_i2)/VFS_PM_PID(m7_i3)`（com.h:547-551），`m_type = VFS_PM_INIT = 0x900`（com.h:513/520）。
- **Rust**：`VfsPmInit { slot: i32, pid: Pid, endpoint: Endpoint }` 放入 `minix-types/src/ipc/vfs.rs`（协议层，PM/VFS 两端单一事实源），提供 `encode()`/`decode()`。PM 侧 `os/servers/pm/src/init.rs` 改为 `pub use minix_types::{VfsPmInit, VFS_PM_INIT, ...}`——**发送方与接收方共享同一编解码，消灭"两边各写一份、漂移"的协议实现类 bug**。
- **验证（fail-closed）**：`decode()` 校验 `m_type`（非 VFS_PM_INIT → `WrongMessageType`）与槽号范围（越界 → `SlotOutOfRange`）。C 的 `fproc[mess.VFS_PM_SLOT]` 无界数组访问在 Rust 中变为显式错误——PM 虽是可信对端，但"信任"与"边界检查"不互斥。
- **NONE 终止符**：endpoint 原样保留（`Endpoint::NONE` 是合法协议值），由握手逻辑判别（等价 C 的 `if (NONE == mess.VFS_PM_ENDPT) break;`）。

### 3.3 D3：构造即空 + 显式启动阶段——`BootPhase`

- **C**：BSS 全局隐式归零 + 两遍 fproc 循环（main.c:405-408 清零、468-483 锁/目录清零）。
- **Rust**：`FProcTable::new()` 的槽位编译期默认即"未使用"（`endpoint = NONE`、`pid = PID_FREE`）；`init_fresh()` 仍显式 `reset_all()`（等价段 1 清零，为未来 restart 路径保留诚实语义）；`finish_init()` 调 `init_phase2()`（等价段 9 的 filp/rd/wd 清零）。
- **启动阶段枚举**：`BootPhase { PmHandshake, InitTables, Mounting, Running }` 让"哪些设施已可用"成为可检查状态（`assert_eq!(boot_phase, ...)`），消灭"顺序靠约定"的时序陷阱类 bug。
- **理由**："清零+重建"在 Rust 中即"构造即空"；显式阶段是 C 隐式顺序的编译期镜像。

### 3.4 D4：执行模型——请求槽状态机 + 门控 + ReplyIntent（ARCH A-1/A-4/A-5）

Minix3 的 VFS 是唯一使用 mthread 多线程的服务器（main 线程 + 9 worker 线程）。minix-rs 的演进（与 `os/servers/vfs/src/` 现状一致）：

| ARCH | Minix3 现状 | minix-rs 演进 | 状态 |
|------|------------|--------------|------|
| **A-1** | `NR_WTHREADS=9` 真实 mthread worker（worker.c） | `WorkerPool` 请求槽状态机（`WorkerState::Idle/Busy/WaitingForFs`），无真实线程；`worker_allow` 门控 → `VfsState::set_accept_requests(bool)` + `pending` 计数 + `FP_PENDING`（worker.c:176-184 语义） | 部分实现（本篇落地门控；槽调度归 08/09） |
| **A-4** | glo.h 全局（fproc/susp_count/reviving/block_all/...） | `VfsState` 聚合全部子系统状态（`fproc_table/worker_pool/call_table/reviving/accept_requests/pending`） | 部分实现（本篇新增门控字段） |
| **A-5** | `return SUSPEND` 表示稍后回复；pipe/select/驱动三条恢复路径 | `ReplyIntent { Reply(i32), ReplyLater, NoReply }`（plan.md §7.3 决策 2） | **缺口**：枚举契约已声明，revive 路径归 17/23/21/22 |

三个设计要点：

1. **单线程事件循环**（VFS 是用户态服务器）：`!Send`/`!Sync`、`&mut` 借用规则安全；**禁止** Kernel SMP 的 `Arc`/`Mutex` 模型。
2. **门控语义**：`set_accept_requests(false)` 期间，`dispatch()` 把用户请求标 `FP_PENDING` 并递增 `pending`（等价 worker_allow 的"挡在 worker 分配之前"），放行后由 08/09 的槽调度恢复。挂载完成前绝不假装可用。
3. **A-5 缺口标注**：`ReplyIntent` 枚举在 01 落地为**契约声明**（`ReplyLater = C 的 SUSPEND`），消费方（pipe/select/cdev/sdev 回复路径）未实现前不接入主循环——fail-closed，不静默吞掉 SUSPEND 语义。

另注：`ds_subscribe("drv\\.[bc]..\\..*")`（main.c:441）为 **ARCH A-14** 缺口（DS 客户端抽象未实现，DEFERRED 归 19/24）；`system_hz` 调用点记录、值语义归 99；`rproctab + map_service` 调用点记录、dmap 细节归 19。

### 3.5 D5：根挂载——启动阶段状态机 + 挂载细节 DEFERRED

- **C**：`do_init_root`（main.c:501-523）：`worker_allow(FALSE)` → `mount_pfs()` → `mount_fs(...)` → `worker_allow(TRUE)`。
- **Rust**：`VfsState::do_init_root()` 落地**门控契约**（`Mounting` 阶段 + `set_accept_requests(false/true)`）；`mount_pfs`/`mount_fs` 的 IPC 内部机制 **DEFERRED 归 18-mount**（与 02-stage-vm 的 exec_bootproc 分步策略同型：先落地调用点与时序，机制细节留给专项文档）。
- **理由**：01 的职责是"根挂载的调用点与时序"，FS 通信协议（`req_readsuper` 等）是 18/12 的语义单元；提前展开会违反"每篇一个语义单元"（plan.md §3.2）。

### 3.6 对照：Redox / Linux 的启动与方案注册模型

本档的核心机制——"VFS 被动收 PM 消息对齐进程表 + 从 RS 拷 rproctab 映射服务 + 根挂载门控"——是 Minix3 微内核**用户态多权威**架构的产物。对照主流 OS 的服务注册与根挂载模型：

| 维度 | Redox | Linux | Minix3（本档） |
|------|-------|-------|---------------|
| 服务/方案注册 | 内核维护 **SchemeList 注册表**，`KernelScheme` trait 集中注册（默认方法兜底返回 `EOPNOTSUPP`/`EBADF`）；用户态 scheme 经 SQE/CQE 队列异步接入 | 文件系统注册表 `file_systems`（`register_filesystem`），VFS 挂载时按名字查表 | 用户态 VFS 从 RS 拷 rproctab 后 `map_service()` 逐个映射进 dmap（main.c:455-467） |
| 根文件系统 | init 进程启动后挂载（`initfs` → 用户态 scheme 接管） | 内核按 `root=` 参数 `mount_root` | VFS `do_init_root` 挂 MFS boot ramdisk（main.c:501-523） |
| 就绪门控 | scheme 未注册时路径解析直接失败 | 无显式门控（内核初始化是原子的） | `worker_allow(FALSE)` 显式门控（main.c:507/522） |
| 路径/端点验证 | 内核在方案落地前完成路径验证 | 内核 VFS 层统一验证 | VFS 启动时以 `VFS_PM_INIT` 建立 endpoint→槽 映射，后续 `fproc_addr(e)` 校验（归 03） |

**最佳实践对比结论**：Redox 把 scheme 注册集中在**内核注册表**、trait 化兜底（默认方法），Linux 把文件系统注册集中在**内核链表**——两者都避免"服务映射状态散落各处"。Minix3 的 `map_service` 把映射放在用户态 dmap，换来驱动热插拔的灵活性（DS 事件，A-14），代价正是本档的启动映射序列。Rust 重写能借鉴的**不是架构**（dmap 必须保留），而是**表达质量**：

- **注册表集中管理** → `map_service` 的调用点集中在一处，dmap 表项不变量由类型保证（归 19）。
- **trait 默认实现兜底** → 未映射的 endpoint 访问返回显式错误（`VfsError::NotImplemented`/errno），绝不 UB（`ipc/vfs.rs` 的 `VfsError::to_errno` 已按此建模）。
- **端点验证前置** → `VfsPmInit::decode` 在握手期就校验槽号范围（§3.2），等价 Redox"路径验证前置"的精神：**边界检查在数据进入状态机之前完成**。

---

## 4. 实现详解

### 4.1 `main_loop.rs`：BootPhase 与启动链落地

`VfsState`（ARCH A-4 聚合）新增四个字段支撑启动链：

```rust
pub boot_phase: BootPhase,        // PmHandshake → InitTables → Mounting → Running
pub accept_requests: bool,        // worker_allow 门控（worker.c:162-185）
pub pending: usize,               // FP_PENDING 计数（worker.c:176-184）
pub initialized: bool,            // finish_init() 完成标记（run 前置断言）
```

启动链分三个公开方法，对应 C 的三段可区分语义：

| Rust 方法 | C 对应 | 职责 |
|-----------|--------|------|
| `init_fresh()` | main.c:393-408 | 阶段置 `PmHandshake` + `fproc_table.reset_all()`（段 1 清零） |
| `pm_handshake_step(&Message)` | main.c:410-436 | 逐条解码填槽；NONE 终止 → `Ok(true)` + 阶段 `InitTables` |
| `finish_init()` | main.c:438-497 | 段 3~11：`init_phase2()` + `do_init_root()` → `Running` + `initialized = true` |

`pm_handshake_step` 逐条填槽与 C 完全对应（main.c:419-425）：`flags = NOFLAGS`、`pid`/`endpoint` 来自消息、`blocked_on = None`、`uid/gid = SYS_UID/SYS_GID`、`umask = !0`。三个失败路径全部显式：

- `decode` 拒绝非 `VFS_PM_INIT` 类型（`WrongMessageType`）与越界槽号（`SlotOutOfRange`）；
- 阶段不匹配 `assert`（内部不变量，不是外部输入）；
- 槽位查不到返回 `SlotOutOfRange`（防御性，decode 已保证不触发）。

`do_init_root()`（main.c:501-523）落地门控契约：

```rust
pub fn do_init_root(&mut self) {
    assert_eq!(self.boot_phase, BootPhase::InitTables, "...");
    self.boot_phase = BootPhase::Mounting;
    self.set_accept_requests(false);   // main.c:507 worker_allow(FALSE)
    // main.c:510 mount_pfs() — DEFERRED（归 18）
    // main.c:516-517 mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR, ...) — DEFERRED（归 18）
    self.set_accept_requests(true);    // main.c:522 worker_allow(TRUE)
    self.boot_phase = BootPhase::Running;
}
```

`dispatch()` 在五路分发骨架（PM → `Continue`，通知/内核 task → `Ignored`，用户请求 → `SpawnWorker`）之上叠加门控：`!accept_requests` 时调 `mark_request_pending(slot)`（置 `FP_PENDING`、`pending += 1`，去重）并返回 `Continue`——等价"挡在 worker 分配之前"。

### 4.2 `minix-types/src/ipc/vfs.rs`：VfsPmInit 编解码（单一事实源）

```rust
pub const VFS_PM_RQ_BASE: i32 = 0x900;   // com.h:513
pub const VFS_PM_INIT: i32 = VFS_PM_RQ_BASE + 0;  // com.h:520

pub struct VfsPmInit { pub slot: i32, pub pid: Pid, pub endpoint: Endpoint }
impl VfsPmInit {
    pub fn encode(&self) -> Message;     // m_m7: m7i1=endpoint, m7i2=slot, m7i3=pid
    pub fn decode(&msg: &Message) -> Result<Self, VfsPmInitError>;
}
pub enum VfsPmInitError { WrongMessageType(i32), SlotOutOfRange(i32) }
```

`decode` 的槽号范围用 `NR_PROCS`（minix-types 常量，与 `FProcTable` 同源）。PM 侧 `os/servers/pm/src/init.rs` 改为 `pub use minix_types::{VfsPmInit, VFS_PM_INIT, ...}`——对端状态机（逐条发送 + 末条 NONE）不变，见 `../04-stage-pm/01-pm-init-main.md` §2.1 第 6 步。

### 4.3 `run()`：mock 主循环

```rust
pub fn run() -> ! {
    let mut state = VfsState::new();
    state.init_fresh();
    // 握手（main.c:410-436）：真实路径由 sef_receive(PM_PROC_NR) 驱动；
    // 内核 IPC 未落地前以占位 NONE 终止符推进状态机（mock）。
    let terminator = VfsPmInit { slot: 0, pid: 0, endpoint: Endpoint::NONE }.encode();
    let _ = state.pm_handshake_step(&terminator);
    state.finish_init();
    loop { /* worker_yield / send_work / get_work / dispatch（mock） */ }
}
```

与既有代码风格一致：IPC 依赖处标注 mock 与真实路径差异（内核 IPC 落地后由 `sef_receive` 循环驱动握手）。

### 4.4 关键不变量与缺口清单

**不变量**：

1. 启动阶段单调推进：`PmHandshake → InitTables → Mounting → Running`，越级即 `assert` 失败。
2. `finish_init()` 只执行一次（`assert!(!initialized)`）；`run()` 前必须 `init_fresh()`。
3. 握手期间每条合法消息恰好填一个槽；越界消息不产生任何副作用（fail-closed）。
4. 门控关闭期间用户请求只标 `FP_PENDING`（去重计数），绝不进入 worker 槽分配。
5. `VfsPmInit` 编解码是 PM/VFS 两端唯一实现（minix-types），不存在第二份拷贝。

**缺口（DEFERRED，fail-closed 标注）**：

| 缺口 | C 调用点 | 归属 |
|------|---------|------|
| `system_hz = sys_hz()` | main.c:438 | 99（值语义）/ 内核 IPC |
| `ds_subscribe`（A-14） | main.c:441 | 19/24 |
| `init_dmap/init_smap` | main.c:451-453 | 19 |
| `sys_safecopyfrom(rproctab) + map_service` | main.c:455-467 | 19 |
| `init_vnodes/vmnts/select/filps` | main.c:485-489 | 04~06 |
| `mount_pfs/mount_fs` | main.c:510-517 | 18 |
| LU 三回调 / restart | main.c:303-373 | 独立特性（未规划） |

---

## 5. 测试要点

### 5.1 新增测试

**`minix-types`（ipc/vfs.rs，`vfs_pm_init_tests` 模块，5 个）**：

- `test_vfs_pm_init_encode_decode_roundtrip`：encode → decode 往返一致（字段全等）。
- `test_vfs_pm_init_decode_none_terminator`：endpoint = NONE 终止符可解码（`is_none()`）。
- `test_vfs_pm_init_decode_wrong_type`：非 `VFS_PM_INIT` 类型 → `WrongMessageType`。
- `test_vfs_pm_init_decode_slot_out_of_range`：槽号 ≥ `NR_PROCS` → `SlotOutOfRange`。
- `test_vfs_pm_init_constants`：`VFS_PM_RQ_BASE = 0x900`、`VFS_PM_INIT = 0x900`。

**`minix-vfs`（main_loop.rs，`tests` 模块，+12 个）**：

- 握手：`test_pm_handshake_fills_slot`（槽填充 + boot 身份：uid/gid=0、umask=!0、NOFLAGS）、`test_pm_handshake_none_terminates`（NONE → `Ok(true)` + 阶段 `InitTables`）、`test_pm_handshake_wrong_type`、`test_pm_handshake_slot_out_of_range`（fail-closed：表状态不变）。
- 启动阶段：`test_vfs_state_init_fresh`（reset 语义）、`test_finish_init_runs_to_running`（`Running` + `initialized` + 门控开放）、`test_finish_init_requires_handshake`（`#[should_panic]` 阶段断言）、`test_root_mount_gate_contract`（do_init_root 门控往返）。
- 门控：`test_dispatch_gated_marks_pending`（`FP_PENDING` + 计数 + `Continue`）、`test_dispatch_gated_dedups_pending`（重复请求不重复计数）、`test_dispatch_accepts_when_open`（开放 → `SpawnWorker`）。
- 契约枚举：`test_boot_phase_ordering`、`test_reply_intent_variants`。

### 5.2 测试总数声明

- `cargo test -p minix-types`：91 passed（含新增 5）。
- `cargo test -p minix-vfs`：47 passed（含新增 12，基线 35）。
- `cargo test -p minix-pm`：91 passed（`VfsPmInit` 上移后回归通过）。
- 全部 `cargo clippy` 无新增警告。

---

## 6. 过渡

启动链在 `finish_init()` 结束后进入主循环，VFS 的世界从此由消息驱动：

```
01（启动骨架：进程表对齐 + init_* 调用点 + 根挂载门控）
  → 02/03（进程模型：fproc 结构 + 表 + endpoint 验证）
  → 04~06（核心数据结构：filp/vnode/vmnt 表）
  → 07/08（并发基础：三级锁 + worker 池状态机）
  → 09（主循环与分发：运行时心脏）
  → 10（PM 协议：VFS 唯一的"非 syscall 入口"，fork 次主线）
```

下一篇读 `02-fproc-struct.md`（fproc 结构字段与标志位）——本篇握手填的就是它的槽位；`worker_allow` 门控的完整调度语义在 `08-worker-thread.md`；根挂载的 `mount_fs` 内部机制在 `18-mount.md`。

---

## 7. 参见

- `../01-stage-kernel/06-proc-init-boot-proc.md` — boot image 与内核启动（VFS 启动的前置）
- `../04-stage-pm/01-pm-init-main.md` — PM 侧启动链 + VFS_PM_INIT 对端状态机（§1.5/§2.1 第 6 步）
- `../02-stage-vm/01-vm-init-main.md` — VM 启动链（D4 rs_handshake 同型对照）
- `plan.md` §1.2（时序图）/§3.4（边界表）/§5.3（01 函数清单）/§7.3（决策 2：ReplyIntent）
- `minix3/minix/servers/vfs/main.c` — ground truth（54-68/303-499/501-560）
- `minix3/minix/servers/vfs/worker.c` — worker_init/worker_allow（27-31/155-183）
- `minix3/minix/servers/vfs/mount.c` — mount_pfs 调用点（391-431）
- `minix3/minix/include/minix/com.h` — VFS_PM_INIT 协议面（513-551）
- `minix3/minix/servers/vfs/const.h` — SYS_UID/SYS_GID（16-17）
- `draft/10-main-loop.md` — 旧素材（启动部分）
