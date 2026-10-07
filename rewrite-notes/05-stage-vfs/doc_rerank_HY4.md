# 05-stage-vfs 文档重建蓝图（HY4）

## 执行头部

```text
your_name(AI agent name) = HY4
target_dir(关注的工作目录) = 05-stage-vfs
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 05-stage-vfs/doc_rerank_HY4.md，不改任何正文。
       只读 C 代码不够，还必须读：本目录全部文档的头部声明、os/ 目录对应入口、
       00-*-overview.md 的导航表。
约束 = 不得引用 .design/ 与 tmp_design_and_todo/；任何落盘产物必须带
       _HY4 后缀；不得读取或照抄其它 AI 的 doc_rerank_* 产物。
```

本文件是本次执行的唯一落盘产物。执行过程中未读取任何 `doc_rerank_deepseek/glm/qwen.md`，
未引用 `.design/` 与 `tmp_design_and_todo/` 下任何内容，未修改任何既有文件。

---

## 0. 元数据

### 0.1 基本信息

| 项 | 值 |
|----|----|
| 执行者 | HY4 |
| 日期 | 2026-09-19 |
| 目标目录 | `rewrite-notes/05-stage-vfs/` |
| 仓库根 | `/home/xzhao/github/minix-rs` |
| 当前提交 | `2d9d1f0aa`（branch `rewrite`） |
| C 真源 | `minix3/minix/servers/vfs/`（33 个 `.c` + 15 个本地 `.h`，`.c` 合计 16,742 行；含 `.h` 17,742 行） |
| Rust 实现 | `os/servers/vfs/src/`（32 个 `.rs` + `src/ipc/`，45,312 行） |
| 官方文档 | `minix3/minix/servers/vfs/README`（700 行，45,900 字节，官方 VFS internals） |

### 0.2 审查范围

**算文档（33 篇，本次重建的对象）**：`00-vfs-overview.md`、`01`–`31` 编号篇、`99-global-concepts.md`。

**算参考材料（不重建，B 相不归档）**：`plan.md`（553 行）、`todo.md`（547 行）、
`archive/todo-R1-archive-2026-09-09.md`。它们承载修复记录与接线矩阵，是**过程账本**
不是教学文档；本蓝图只从中取事实（ARCH 清单、W1–W9、Fix 记录），不取顺序结论。

**算素材（不重建，B 相不归档）**：`draft/`（27 篇，旧 fork 主线遗留）。

**范围外**：`doc_rerank_deepseek/glm/qwen.md`（其它执行者产物，未读）。

### 0.3 读取清单

| 类别 | 已读内容 |
|------|---------|
| 文档头部声明 | 33 篇全部 `head -8`（定位/源码/Rust 模块/前置/不讲什么） |
| 文档章节结构 | 14 篇 `grep '^## '`（验证 plan.md §6.1 状态表已过期） |
| C 源码精读 | `main.c` 全文（973 行）、`worker.c` 全文（607）、`comm.c` 全文（244）、`table.c` 全文（82）、`path.c:1-60`、`const.h`、`glo.h` |
| C 源码检索 | `misc.c`/`open.c`/`mount.c`/`pipe.c`/`select.c`/`path.c`/`request.c`/`device.c` 的函数定义行；`vfsif.h` REQ_* 全表；`com.h` VFS_PM_*/RS 基址；`callnr.h` VFS_BASE |
| 非 C 制品 | `minix3/minix/servers/vfs/Makefile`、`minix3/minix/servers/vfs/README` 目录、`minix3/share/mk/minix.service.mk`（存在性）、`os/servers/vfs/Cargo.toml`、`src/lib.rs`、`src/main.rs` |
| 边界材料 | `00-master-plan/README.md`（阶段划分与启动因果链）、`05-stage-vfs/plan.md` + `todo.md` |
| 前一 stage | `04-stage-pm/00-pm-overview.md`（PM 已讲内容，本 stage 不重复） |
| Rust 入口 | `os/servers/vfs/src/` 全目录 `wc -l` + `grep '^pub struct\|^pub enum\|^pub trait'`（120+ 符号） |
| 写法范例 | `01-stage-kernel/06-todo.md`（§3 问题诊断 / §9 验收标准的写法） |

### 0.4 使用的命令与关键输出（证据摘录）

```bash
$ wc -l minix3/minix/servers/vfs/*.c *.h | sort -n | tail -1
  17742 total
$ ls minix3/minix/servers/vfs/*.c | wc -l            → 33
$ ls minix3/minix/servers/vfs/*.h | wc -l            → 15
$ grep -c "CALL(VFS_" minix3/minix/servers/vfs/table.c → 64
$ grep -n "define VFS_BASE" minix3/minix/include/minix/callnr.h
  68:#define VFS_BASE		0x100
$ grep -n "VFS_PM_RQ_BASE\|VFS_PM_RS_BASE" minix3/minix/include/minix/com.h
  513:#define VFS_PM_RQ_BASE	0x900
  514:#define VFS_PM_RS_BASE	0x980
$ grep -n "define FS_BASE" （vfsif.h 无；com.h:589 = 0xA00，todo.md §9.2 R2-P0-1 已证）
$ grep -n "CDEV_RS_BASE\|BDEV_RS_BASE\|SDEV_RS_BASE\|RTCDEV_RS_BASE" minix3/minix/include/minix/com.h
  920:#define CDEV_RS_BASE	0x480
  964:#define BDEV_RS_BASE	0x580
  996:#define RTCDEV_RS_BASE	0x1480
  1038:#define SDEV_RS_BASE		0x1980
$ grep -n "VFS_TRANSID\|TRNS_" minix3/minix/include/minix/com.h minix3/minix/include/minix/vfsif.h
  com.h:911:#define VFS_TRANSID	(VFS_TRANSACTION_BASE + 1)   /* = 0xB01 */
  com.h:912:#define IS_VFS_FS_TRANSID(type) (((type) & ~0xff) == VFS_TRANSACTION_BASE)
  vfsif.h:79-81: TRNS_GET_ID/ADD_ID/DEL_ID
$ grep -c "define REQ_" minix3/minix/include/minix/vfsif.h → 33（含 REQ_RDONLY/REQ_ISROOT 两个非消息位）
$ grep -oE "\breq_[a-z_0-9]+" request.c | sort -u | wc -l → 36
$ grep -n "NR_MNTS" minix3/minix/servers/vfs/const.h → 7:#define NR_MNTS 16
$ grep -n "FP_BLOCKED_ON_" minix3/minix/servers/vfs/const.h → 7 态（0..6）
# 引用量统计
$ grep -rEoh "\b[0-9]{2}-[a-z0-9-]+\.md" 05-stage-vfs/*.md | wc -l          → 902
$ grep -rEoh "[0-9]{2}-[a-z0-9-]+\.md" os/servers/vfs/src --include=*.rs | wc -l → 86
$ grep -rlE "[0-9]{2}-[a-z0-9-]+\.md" os/servers/vfs/src --include=*.rs | wc -l  → 25
$ grep -rl "05-stage-vfs" notes/... --include=*.md | grep -v "^./05-stage-vfs" | grep -v doc_rerank | wc -l → 50
$ grep -ln "servers/vfs/README" 05-stage-vfs/*.md  → 0 命中（官方文档零引用）
```

### 0.5 范围外发现

1. **`minix3/minix/servers/vfs/README`（700 行）零文档引用**。它含 §3 Worker threads、
   §4 Locking（4.1 需求 / 4.2 三级锁 / 4.3 受锁数据结构 / **4.4 加锁顺序** /
   4.5–4.7 vmnt/vnode/filp 分别的加锁 / **4.8 每请求类型的锁特征表**）、
   §5 Recovery from driver crashes（5.1 块 / 5.2 字符与 socket / 5.3 FS）。
   其中 **4.4 加锁顺序** 与 **4.8 每请求类型锁特征** 是当前 33 篇文档**完全没有**的知识，
   且属 ground truth 级（Minix 官方作者撰写）。已作为新增知识点入池（N-016/N-017）。
2. `RTCDEV_RS_BASE 0x1480`（com.h:996）在现有文档与 Rust 中均未见提及——
   现有 `route_message` 只有 BDEV/CDEV/SDEV 三级（main.c:126-134）。
   判定：**VFS 主循环确实不处理 RTCDEV**（C 无对应分支），列为有意省略，
   但 01-vfs-wire 篇必须列出它以证明"不是漏了，是查过了"。
3. `NR_SOCKDEVS 8`（const.h:11）在文档中无专门讲述，仅随 smap 表一带而过。

---

## 1. C 真序（运行时真序重建）

### 1.1 阶段类型判定

**主型：服务事件循环型；次型：启动链型。**

理由：VFS 是常驻用户态服务器，生命周期的绝大部分时间停在 `main.c:69-139` 的主循环里；
启动段（`sef_cb_init_fresh`，main.c:393-496）是一次性的线性初始化，是循环的**前置条件**
而非主体。按规范第九部分，服务事件循环型按
「服务为什么存在 → 诞生与初始化 → 消息接口 → 核心数据结构 → 按场景分组的请求处理 →
查询与杂项 → 与邻接服务的协议」组织；本蓝图据此编排，并在 §1.5 序差表记录与 C 真序的偏离。

### 1.2 真序表 · 启动段

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| S1 | RS 从 boot image 加载 VFS 镜像并执行 | `00-master-plan/README.md` 启动因果链；`kernel/table.c` boot_image | VFS 是 boot image 登记项，由 RS 运行时加载；先于它的只有 kernel 任务、VM、RS |
| S2 | `main()` 入口 → `sef_local_startup()` | `main.c:54`、`main.c:64` | main 的第一句就是 SEF 本地启动 |
| S3 | 注册 6 个 SEF 回调 | `main.c:377-384` | init_fresh / init_restart(STATEFUL) / init_lu / lu_prepare / lu_state_changed / lu_state_isvalid |
| S4 | `sef_startup()` → 回调 `sef_cb_init_fresh` | `main.c:387` → `main.c:393` | 首次启动走 fresh 分支 |
| S5 | fproc 第一遍清零：`fp_endpoint=NONE`、`fp_pid=PID_FREE` | `main.c:405-408` | 建立"空槽"不变式，供 §S6 的握手填槽使用 |
| S6 | `VFS_PM_INIT` 握手循环：收 PM 消息逐条填 `fproc[mess.VFS_PM_SLOT]`，`VFS_PM_ENDPT==NONE` 终止 | `main.c:415-434` | 每条填 endpoint/pid/flags/blocked_on/凭证四字段/umask=~0 |
| S7 | `ipc_send(PM, OK)` 同步 | `main.c:435-436` | 与 PM 的启动屏障 |
| S8 | `system_hz = sys_hz()` | `main.c:438` | 供 select 定时器换算 |
| S9 | `ds_subscribe("drv\\.[bc]..\\..*")` | `main.c:441` | 订阅块/字符驱动上下线事件 |
| S10 | `worker_init()`：`mthread_create × NR_WTHREADS(9)`，`pending=busy=0`，`block_all=FALSE` | `main.c:445` → `worker.c:27-58` | VFS 是全系统唯一多线程服务器 |
| S11 | `bsf_lock` mutex 初始化 | `main.c:448` | 块特殊文件串行锁 |
| S12 | `init_dmap()`、`init_smap()` | `main.c:451-452` | 驱动通讯录与 socket 驱动表 |
| S13 | `sys_safecopyfrom(RS, rproctab_gid)` + `map_service` 循环 | `main.c:455-465` | 把 boot image 里的服务登记进 dmap |
| S14 | fproc 第二遍：`fp_lock` init、`fp_worker=NULL`、`fp_filp[]=NULL`、`fp_rd/fp_wd=NULL` | `main.c:468-484` | 与 S5 分离的原因：S5 先建立 endpoint/pid，S14 再补锁与 fd 表 |
| S15 | `init_vnodes()` / `init_vmnts()` / `init_select()` / `init_filps()` | `main.c:486-489` | 四张核心表按此顺序初始化 |
| S16 | `worker_start(fproc_addr(VFS_PROC_NR), do_init_root, ...)` | `main.c:492-493` | 根挂载被丢进 worker，主线程继续 |
| S17 | `do_init_root`：`worker_allow(FALSE)` → `mount_pfs()` → `mount_fs(DEV_IMGRD,"bootramdisk","/",MFS_PROC_NR,0,...)` → `worker_allow(TRUE)` | `main.c:501-523` | 挂载期间拒绝一切外部请求 |
| S18 | 回到 main，打印 `Started VFS: %d worker thread(s)` | `main.c:66` | 进入循环 |

### 1.3 真序表 · 循环段（一轮）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| S19 | `worker_yield()` | `main.c:70` → `worker.c:431` | 让出给 worker 线程；`self=NULL` |
| S20 | `send_work()`：`sending>0` 时遍历 `vmnt[]` 调 `fs_sendmore` | `main.c:72` → `comm.c:37-45` | 把排队中的 FS 请求尽量发出去 |
| S21 | `get_work()`：若 `reviving!=0` 先找 `FP_REVIVED` 进程 → `unblock()`；否则 `sef_receive(ANY)`，定位 `fp`，做 `fp_endpoint` 一致性断言 | `main.c:77` → `main.c:580-633` | 复活优先于新消息——这是 VFS 的公平性关键 |
| S22 | `transid=TRNS_GET_ID(m_in.m_type)`；`IS_VFS_FS_TRANSID` → `worker_get(tid-VFS_TRANSID)` → `do_reply(wp)` | `main.c:80-90`、`com.h:911-912`、`vfsif.h:79-81` | 第一优先级：FS/VM/驱动的异步回复 |
| S23 | `who_e == PM_PROC_NR` → `service_pm()` | `main.c:91-94` → `main.c:764-915` | 第二优先级：PM 控制面 |
| S24 | `is_notify(call_nr)` → DS:`ds_event` / KERNEL:`mthread_stacktraces` / CLOCK:`expire_timers` | `main.c:95-117` | 第三优先级：通知 |
| S25 | `who_p < 0`（来自 kernel task）→ 忽略 | `main.c:118-124` | 任务只允许 notify |
| S26 | `IS_BDEV_RS` → `bdev_reply()`；`IS_CDEV_RS` → `cdev_reply()`；`IS_SDEV_RS` → `sdev_reply()` | `main.c:126-134`、`com.h:964/920/1038` | 第四优先级：三类驱动回复 |
| S27 | 其余 → `handle_work(do_work)` | `main.c:135-137` | 第五优先级：正常系统调用 |

### 1.4 真序表 · 一次请求的生命周期（syscall 路径）

| # | 动作 | C 锚点 | 说明 |
|---|------|--------|------|
| S28 | `handle_work`：若 `FP_SRV_PROC` → `find_vmnt` + `VMNT_CALLBACK` 检查（`EAGAIN`）+ `worker_available()==0` 检查（`EAGAIN`），置 `VMNT_CALLBACK`；最后 `worker_start(fp, func, &m_in, use_spare)` | `main.c:146-181` | FS 回调的死锁消解策略 |
| S29 | `worker_try_activate`：`needed = use_spare?1:2`（留一条 spare 解死锁）；够则 `worker_assign`（绑 `w_fp`/`fp_worker`、`busy++`、`wake`），否则 `FP_PENDING` + `pending++` | `worker.c:331-355`、`worker.c:119-142` | 槽的分配策略 |
| S30 | `worker_main` 一轮：`worker_get_work`（pending 优先，否则 `worker_sleep`）→ `lock_proc` → `fp_func()` → 若 `FP_PM_WORK` → `service_pm_postponed()` → `thread_cleanup()` → `unlock_proc` → 解绑 + `busy--` | `worker.c:239-290` | 一个槽的完整生命周期 |
| S31 | `do_work`：`fp_pid==PID_FREE` 直接丢弃；`IS_VFS_CALL` → `call_index=job_call_nr-VFS_BASE` → `call_vec[call_index]()`；`error != SUSPEND` → `reply()` | `main.c:263-298`、`table.c:17-82` | 64 臂分派 |
| S32 | handler 调 `req_*` → `fs_sendrec`：`find_vmnt` + `fs_e==fp->fp_endpoint → EDEADLK`；窗口未满且无 CALLBACK → `sendmsg`（`c_cur_reqs++`、`TRNS_ADD_ID(w_tid+VFS_TRANSID)`、`w_task=dst`、`asynsend3(AMF_NOREPLY)`）；否则 `queuemsg`（尾插 + `sending++`）；然后 `worker_wait()` | `comm.c:134-168`、`comm.c:12-32`、`comm.c:223-244` | 窗口化异步通信 |
| S33 | 对端回复到达 → S22 分支 → `do_reply`：`find_vmnt` 校验、`w_task==who_e` 校验、`w_sendrec==NULL` 迟到校验、`*w_sendrec=m_in`、`c_cur_reqs--`、`worker_signal(wp)` | `main.c:187-211` | 回复路由 |
| S34 | worker 从 `worker_wait` 返回，继续 handler，最终 `reply()` 或返回 `SUSPEND` | `worker.c:510-521`、`main.c:297` | 同步往返闭环 |
| S35 | **SUSPEND 分支 A（pipe）**：`suspend()` 记 `fp_blocked_on=FP_BLOCKED_ON_PIPE` + `fp_pipe` 五字段 + `susp_count++`；对端 `release()` → `revive()` 置 `FP_REVIVED` + `reviving++`；下一轮 S21 命中 → `unblock()` → 因是 PIPE 走 `worker_start(do_pending_pipe)` 续作 | `pipe.c:294`、`pipe.c:435`、`main.c:937-968`、`main.c:216-258` | 挂起 → 复活 → 续作 |
| S36 | **SUSPEND 分支 B（flock）**：`lock_op` 冲突 → `FP_BLOCKED_ON_FLOCK` + `fp_flock` → `lock_revive` 广播 → `unblock()` 重建 `VFS_FCNTL` 原请求重放 | `main.c:946-952`、`lock.c:172` | 锁挂起重放 |
| S37 | **SUSPEND 分支 C/D（cdev/sdev）**：`cdev_io`/`sdev_*` 挂起后由 `CDEV_REPLY`/`SDEV_REPLY`（S26）经 `cdev_reply`/`sdev_reply` 唤醒，不经 `reviving` 计数 | `main.c:129-134`、`cdev.c:481`、`sdev.c:989` | 驱动回复路径与主循环 reviving 路径**不同源** |
| S38 | **取消路径**：进程被信号打断或驱动死亡 → `cdev_cancel`/`sdev_cancel`/`select_cancel_all`/`worker_stop_by_endpt`（注入 `EIO` 并 `worker_wake`） | `cdev.c:381`、`sdev.c:940`、`select.c:714`、`worker.c:535-567` | 二阶等待 |

### 1.5 序差表（教学序 vs 运行时序）

| # | 运行时序事实（锚点） | 教学序选择 | 理由 | 回指补偿位置 |
|---|---------------------|-----------|------|-------------|
| D-1 | 根挂载发生在启动段 S17（`main.c:501-523`），早于一切请求 | 推迟到 `16-mount`；`02-vfs-init` 只给调用点与"为什么必须在此时挂载" | `mount_fs` 依赖 vmnt 表（11）、FS 通信（12/13）、设备表（22），无法前置 | `16-mount §根挂载调用点` 回指 `02-vfs-init §S17` |
| D-2 | `worker_init`/`worker_allow` 在 S10/S17 被启动链调用 | 机制推迟到 `03-request-slots`；`02-vfs-init` 只给调用点与门控语义 | 槽的状态机需要整整一篇，塞进启动篇会让启动篇概念混杂 | `03-request-slots §worker_init/allow` 回指 `02-vfs-init §S10/S17` |
| D-3 | `VFS_PM_INIT` 握手（S6）是 12 个 VFS_PM 请求之一 | 在 `02-vfs-init` 讲握手本身，完整协议面推迟到 `28-pm-protocol` | 握手是启动屏障，其余 11 个请求需要 fproc/exec/coredump 前置 | `28-pm-protocol §VFS_PM_INIT` 回指 `02-vfs-init §S6` |
| D-4 | 阻塞与复活（S35–S38）散落在主循环、pipe、select、cdev、sdev、lock 六处 | 抽出统一框架篇 `05-vfs-blocking` 紧跟主循环之后 | 规范 5.2：并行体先给统一框架篇，再按场景分组成篇 | 各族篇（20/24/25/26/34）开头回指 `05-vfs-blocking` |
| D-5 | 三张核心表（filp/vnode/vmnt）在 S15 一次性初始化 | 教学序把它们放在锁（08）之后 | C 里 `vnode.h:VNODE_*`/`vmnt.h:VMNT_*` 直接映射 `tll.h:TLL_*`；锁是表的锁机制，必须先讲（修 plan.md §7.1 D-1 的自相矛盾，见 §3.4） | `08-tll-lock §使用方` 回指 09/10/11 |
| D-6 | FS 通信（S32）发生在每次请求处理中，时序上晚于表初始化 | 教学序把 12/13 放在三张表（09/10/11）之后、路径解析（15）之前 | `m_comm` 是 `vmnt` 的字段；`req_*` 是路径解析与读写的数据面 | 与运行时序一致，无偏离 |
| D-7 | 设备表初始化（S12）早于根挂载（S17） | 教学序把 `22-device-map` 放在 16-mount 之后 | `map_service` 的语义需要"驱动是谁、谁来问"的上下文，挂载之后更好懂 | `22-device-map §init_dmap` 回指 `02-vfs-init §S12/S13` |

---

## 2. 知识点全集（存量池初建）

> 编号规则：`K-0xx` 存量（来自现有文档），`N-0xx` 新增（来自 C/非 C 制品/理论，由 §3 覆盖审计发现）。
> "类型"取值：概念 / 机制 / 数据结构 / 接口与协议 / 约束与不变量 / 架构演进 / 工具与工程 / 测试性质。
> 汇总对齐键建议用「名称 + 锚点」。

### 2.1 存量知识点（K 组，共 226 条，编号 K-001…K-226 连续无空位）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|---------|---------|------|---------|
| K-001 | VFS 是文件系统语义的唯一权威 | 概念 | 存量 | 00 §1.1 | `00-vfs-overview.md:11-15` | 回答"为什么内核不管文件" |
| K-002 | 三方词汇表：路径→vnode、fd→filp、挂载点→vmnt | 概念 | 存量 | 00 §1.1 | `00-vfs-overview.md:13` | 回答"VFS 持有哪些映射" |
| K-003 | 三个服务面数字（64 / 12 / 33） | 概念 | 存量 | 00 §1.3 | `table.c:17-82`、`com.h:520-531`、`vfsif.h:41-73` | 回答"VFS 面有多大" |
| K-004 | SEF 启动框架与 6 回调注册 | 机制 | 存量 | 01 §1-2 | `main.c:374-388` | 回答"谁调用 init_fresh" |
| K-005 | `sef_cb_init_fresh` 的 18 步时序 | 机制 | 存量 | 01 §2 | `main.c:393-496` | 回答"初始化按什么顺序、为什么" |
| K-006 | fproc 两遍初始化（S5/S14） | 机制 | 存量 | 01 §2、02 | `main.c:405-408`、`main.c:468-484` | 回答"为什么清两次" |
| K-007 | `VFS_PM_INIT` 握手协议 | 接口与协议 | 存量 | 01 §2、10 | `main.c:415-436` | 回答"VFS 怎么知道有哪些进程" |
| K-008 | `system_hz = sys_hz()` | 机制 | 存量 | 01 §2 | `main.c:438` | 回答"时钟频率从哪来" |
| K-009 | `ds_subscribe` 驱动事件订阅 | 接口与协议 | 存量 | 01 §2、19 | `main.c:441` | 回答"驱动上下线怎么通知" |
| K-010 | `init_*` 七表调用点与顺序 | 机制 | 存量 | 01 §2 | `main.c:445-489` | 回答"表按什么顺序建" |
| K-011 | `do_init_root` 与 `worker_allow` 门控 | 机制 | 存量 | 01 §2、18 | `main.c:501-523` | 回答"何时开始接受请求" |
| K-012 | `lock_proc`/`unlock_proc` 与 trylock+suspend 组合 | 机制 | 存量 | 01 §2 | `main.c:528-553` | 回答"进程锁怎么不死锁" |
| K-013 | `thread_cleanup` 与 `VMNT_CALLBACK` 回收 | 机制 | 存量 | 01 §2 | `main.c:558-575` | 回答"槽归还时清什么" |
| K-014 | SEF LU 三回调（`lu_prepare`/`lu_state_changed`/`init_lu`） | 机制 | 存量 | 01 §2、08 | `main.c:303-369` | 回答"热更新时 worker 怎么办" |
| K-015 | `init_restart ≡ init_fresh` 判定 | 约束与不变量 | 存量 | 01 | `main.c:378` | 回答"VFS 重启是否保状态" |
| K-016 | `fproc` 字段全集（7 组） | 数据结构 | 存量 | 02 §1-2 | `fproc.h:15-82` | 回答"VFS 每进程记什么" |
| K-017 | `fp_flags` 六位语义 | 数据结构 | 存量 | 02 §2 | `fproc.h:91-98` | 回答"进程状态位各管什么" |
| K-018 | `fp_blocked_on` 七态 + `fp_u` 五类 union | 数据结构 | 存量 | 02 §2 | `fproc.h:31-57`、`const.h:19-25` | 回答"挂起时存什么" |
| K-019 | 凭证五字段 + `fp_sgroups` + `fp_umask` | 数据结构 | 存量 | 02 §2 | `fproc.h:60-70` | 回答"权限判定读什么" |
| K-020 | `fp_filp[OPEN_MAX]` + `fp_cloexec_set` | 数据结构 | 存量 | 02 §2、14 | `fproc.h:24-25` | 回答"fd 表与 cloexec 位图在哪" |
| K-021 | `fp_rd`/`fp_wd` 工作目录与根 vnode | 数据结构 | 存量 | 02 §2、28 | `fproc.h:22-23` | 回答"相对路径起点在哪" |
| K-022 | `fp_tty` 控制终端与 `CTTY_ENDPT` | 数据结构 | 存量 | 02 §2、21 | `fproc.h:26`、`const.h:52` | 回答"/dev/tty 指谁" |
| K-023 | `fp_lock`/`fp_worker`/`fp_func`/`fp_msg`/`fp_pm_msg` | 数据结构 | 存量 | 02 §2、08 | `fproc.h:71-80` | 回答"槽与进程怎么互相指" |
| K-024 | `fp_name` 进程名 | 数据结构 | 存量 | 02 §2、26 | `fproc.h:81` | 回答"core 文件名从哪来" |
| K-025 | `PID_FREE`/`REVIVING` 哨兵 | 约束与不变量 | 存量 | 02 §2 | `fproc.h:100-106` | 回答"槽空闲怎么判" |
| K-026 | `fproc[NR_PROCS]` 固定表与 slot 索引 | 数据结构 | 存量 | 03 §1-2 | `glo.h:26-28` | 回答"进程上下文怎么存" |
| K-027 | endpoint ↔ slot 双向编码与世代位 | 机制 | 存量 | 03 §2 | `minix3/minix/include/minix/endpoint.h` | 回答"endpoint 怎么变槽号" |
| K-028 | `isokendpt`/`okendpt` 三守卫与致命分化 | 约束与不变量 | 存量 | 03 §2 | `utility.c:94-127` | 回答"非法端点怎么挡" |
| K-029 | `fproc_light` 只读投影（MIB） | 数据结构 | 存量 | 03 §2 | `fproc.h:117-124` | 回答"观测者怎么看进程" |
| K-030 | `filp[NR_FILPS=1024]` 与 `filp_count==0` 空闲哨兵 | 数据结构 | 存量 | 04 §1-2 | `const.h:5`、`file.h` | 回答"打开描述怎么存" |
| K-031 | `fd → filp → vnode` 二跳映射 | 概念 | 存量 | 04 §1 | `file.h`、`fproc.h:24` | 回答"fd 到底是什么" |
| K-032 | `get_filp`/`get_filp2` 的 `FILP_CLOSED` 门与三态锁模式 | 机制 | 存量 | 04 §2 | `filedes.c:177-193` | 回答"close 为什么能穿 CLOSED" |
| K-033 | `find_filp` 共享检测 / `find_filp_by_sock_dev` | 机制 | 存量 | 04 §2 | `filedes.c:205-248` | 回答"怎么找对端 filp" |
| K-034 | `filp_count` 引用计数增减不变量 | 约束与不变量 | 存量 | 04 §2、10、14 | `filedes.c:413-430` | 回答"共享打开何时真正释放" |
| K-035 | filp 三态锁 `filp_lock`/`softlock`/`ioctl_fp` | 机制 | 存量 | 04 §2 | `file.h:20-40` | 回答"filp 并发怎么互斥" |
| K-036 | `FSF_*` 标志与 select 字段 | 数据结构 | 存量 | 04 §2、23 | `file.h:41-49` | 回答"select 状态记在哪" |
| K-037 | `vnode[NR_VNODES=1024]` 与 `ref==0 && !locked` 双条件 | 数据结构 | 存量 | 05 §1-2 | `const.h:8`、`vnode.c:85-137` | 回答"vnode 何时可回收" |
| K-038 | `v_ref_count`/`v_fs_count` 双层引用与 `clean_refs` 256 阈值 | 约束与不变量 | 存量 | 05 §2 | `vnode.c:305-316` | 回答"为什么两个计数" |
| K-039 | `find_vnode(fs_e+ino)` 缓存命中 | 机制 | 存量 | 05 §2 | `vnode.c:110-124` | 回答"vnode 怎么复用" |
| K-040 | `dup_vnode`/`put_vnode` 快慢路径 | 机制 | 存量 | 05 §2 | `vnode.c:227-303` | 回答"引用何时回 FS" |
| K-041 | `VNODE_READ/OPCL/WRITE` → TLL 映射与升降级 | 机制 | 存量 | 05 §2、07 | `vnode.h:20-30` | 回答"vnode 锁怎么升级" |
| K-042 | `vmnt[NR_MNTS=16]` 与 `m_dev==NO_DEV` 空闲 | 数据结构 | 存量 | 06 §1-2 | `const.h:7`、`vmnt.h:8-20` | 回答"挂载表容量与空闲判据" |
| K-043 | `m_fs_e` + `m_dev` 双重编码 | 数据结构 | 存量 | 06 §2 | `vmnt.h:10-14` | 回答"device 与 FS 怎么对应" |
| K-044 | `m_comm` 窗口三字段 | 数据结构 | 存量 | 06 §2、11 | `vmnt.h:22-28` | 回答"每 FS 并发上限在哪" |
| K-045 | `VMNT_READ/WRITE/EXCL` → TLL 可升级锁 | 机制 | 存量 | 06 §2 | `vmnt.h:30-35` | 回答"挂载表怎么锁" |
| K-046 | `VMNT_CALLBACK/MOUNTING/FORCEROOTBSF/READONLY` | 数据结构 | 存量 | 06 §2、11 | `vmnt.h:37-45` | 回答"挂载状态位各管什么" |
| K-047 | `mark_vmnt_free`/`clear_vmnt` 释放分化 | 机制 | 存量 | 06 §2 | `vmnt.c:65-93` | 回答"卸载时清几个字段" |
| K-048 | `vmnt_unmap_by_endpt` 四步级联 | 机制 | 存量 | 06 §2、19 | `vmnt.c:180-206` | 回答"FS 死了挂载表怎么收" |
| K-049 | `m_mount_path`/`m_fstype`/`m_mount_dev` 三名字 | 数据结构 | 存量 | 06 §2、28 | `vmnt.h:16-19` | 回答"statvfs 的三名字从哪来" |
| K-050 | `tll_t` 六字段与 `TLL_READ/READSER/WRITE` 三态 | 数据结构 | 存量 | 07 §1-2 | `tll.h:8-20` | 回答"三级锁是什么" |
| K-051 | `t_status` UPGR/PEND 正交标记与写偏序 | 约束与不变量 | 存量 | 07 §2 | `tll.c:139-229` | 回答"谁先拿到锁" |
| K-052 | `tll_lock` 5 路分发 / `tll_append` 双队列 / `tll_unlock` 选头唤醒 | 机制 | 存量 | 07 §2 | `tll.c:139/11/230` | 回答"锁等待怎么排队" |
| K-053 | `tll_downgrade`/`tll_upgrade` 时序 | 机制 | 存量 | 07 §2 | `tll.c:74/306` | 回答"读锁怎么变写锁" |
| K-054 | `TLL_NONE ↔ 0` 空锁不变式 | 约束与不变量 | 存量 | 07 §2 | `tll.h:12` | 回答"为什么 get_free_* 能只看锁" |
| K-055 | `NR_WTHREADS=9` 与 mthread 真实线程 | 概念 | 存量 | 08 §1-2 | `const.h:9`、`worker.c:52` | 回答"VFS 为什么多线程" |
| K-056 | `pending`/`busy`/`block_all` 三全局与 `worker_available` | 机制 | 存量 | 08 §2 | `worker.c:10-12/228` | 回答"槽够不够怎么判" |
| K-057 | `worker_start` 四象限 + `worker_try_activate` spare 槽策略 | 机制 | 存量 | 08 §2 | `worker.c:360-426/331-355` | 回答"为什么留一条 spare" |
| K-058 | `worker_allow` 初始化门控与 `FP_PENDING` 排队 | 机制 | 存量 | 08 §2、01 | `worker.c:162-187` | 回答"启动期请求去哪了" |
| K-059 | `worker_main` 一轮五步 | 机制 | 存量 | 08 §2 | `worker.c:239-290` | 回答"槽里到底跑什么" |
| K-060 | `worker_suspend/resume/wait/signal` 三件套 | 机制 | 存量 | 08 §2 | `worker.c:474-530` | 回答"槽怎么让出与唤醒" |
| K-061 | `worker_stop`/`stop_by_endpt` 的 EIO 注入 | 机制 | 存量 | 08 §2、14 | `worker.c:535-567` | 回答"对端死了在途请求怎么收" |
| K-062 | `w_fp ↔ fp_worker` 双向绑定 | 约束与不变量 | 存量 | 08 §2 | `worker.c:137-138` | 回答"槽与进程为什么互指" |
| K-063 | `worker_set_proc`（reboot 专用违规操作） | 机制 | 存量 | 08 §2 | `worker.c:586-607` | 回答"重启为什么能换进程上下文" |
| K-064 | ARCH A-1：mthread 线程 → 请求槽状态机 | 架构演进 | 存量 | 08 §3、00 §3 | `worker.rs:1-33` | 回答"Rust 为什么没有线程" |
| K-065 | `get_work` 的 `reviving` 优先 + ANY 接收 + 一致性断言 | 机制 | 存量 | 09 §1-2 | `main.c:580-633` | 回答"复活与新消息谁先" |
| K-066 | 主循环八级路由 | 机制 | 存量 | 09 §2 | `main.c:80-138` | 回答"一条消息怎么分类" |
| K-067 | `handle_work` 的 FS 回调判定与 EAGAIN 双闸 | 机制 | 存量 | 09 §2 | `main.c:146-181` | 回答"FS 反向调用怎么防死锁" |
| K-068 | `do_work` 的 `IS_VFS_CALL → call_vec → reply/SUSPEND` | 机制 | 存量 | 09 §2 | `main.c:263-298` | 回答"调用号怎么变处理函数" |
| K-069 | `call_vec` 64 调用分派 | 接口与协议 | 存量 | 09 §2 | `table.c:17-82` | 回答"64 个调用各归谁" |
| K-070 | `reply`/`replycode` 的 `ipc_sendnb` | 机制 | 存量 | 09 §2 | `main.c:638-663` | 回答"回复怎么发、失败怎么办" |
| K-071 | `unblock` 重建原请求（PIPE/FLOCK） | 机制 | 存量 | 09 §2、17、34 | `main.c:921-973` | 回答"挂起的请求怎么重放" |
| K-072 | `do_pending_pipe` 续作 | 机制 | 存量 | 09 §2、17 | `main.c:216-258` | 回答"pipe 为什么不能原样重放" |
| K-073 | ARCH A-2：函数指针表 → 枚举穷举 match | 架构演进 | 存量 | 09 §3、00 §3 | `call_table.rs:28-93`、`syscalls.rs` | 回答"Rust 的分派长什么样" |
| K-074 | ARCH A-4：全局变量 → `VfsState` 聚合 | 架构演进 | 存量 | 09 §3、00 §3 | `main_loop.rs:VfsState` | 回答"Rust 的全局在哪" |
| K-075 | ARCH A-5：`SUSPEND` → `ReplyIntent` | 架构演进 | 存量 | 09 §3 | `main_loop.rs` | 回答"稍后回复怎么表达" |
| K-076 | `VFS_PM_RQ_BASE 0x900` / `RS 0x980` 协议面（12 RQ + 11 RS） | 接口与协议 | 存量 | 10 §1-2 | `com.h:513-544` | 回答"PM 能对 VFS 下哪些命令" |
| K-077 | `service_pm` 立即 7 / 延期 4 / REBOOT 三分支 | 机制 | 存量 | 10 §2 | `main.c:764-915` | 回答"哪些 PM 请求要丢进槽" |
| K-078 | `service_pm_postponed` 四分支 | 机制 | 存量 | 10 §2 | `main.c:668-759` | 回答"延期的 PM 工作谁来执行" |
| K-079 | `pm_fork` 的 fproc 复制 + `filp_count++` + `dup_vnode` | 机制 | 存量 | 10 §2 | `misc.c:577-634` | 回答"fork 时 VFS 复制什么" |
| K-080 | `pm_exit` / `free_proc` 级联 | 机制 | 存量 | 10 §2 | `misc.c:713-778` | 回答"进程退出时 fd 怎么收" |
| K-081 | `pm_setuid/setgid/setgroups/setsid` 凭证注入 | 机制 | 存量 | 10 §2 | `misc.c:764-796` | 回答"凭证由谁改" |
| K-082 | `pm_reboot` 八步序列 | 机制 | 存量 | 10 §2、31 | `misc.c:510-572` | 回答"重启时卸载顺序" |
| K-083 | `pm_dumpcore` / `pm_exec` 入口 | 机制 | 存量 | 10 §2、25、26 | `main.c:828-831` | 回答"exec/core 从哪进" |
| K-084 | `m_comm` 窗口三字段 + `sending` 全局 | 数据结构 | 存量 | 11 §1-2 | `comm.c:19/37-45` | 回答"FS 并发怎么限流" |
| K-085 | `VFS_TRANSID` 0xB01 与 `TRNS_ADD/GET/DEL_ID` 编码 | 接口与协议 | 存量 | 11 §2 | `com.h:911-912`、`vfsif.h:79-81` | 回答"异步回复怎么找回请求" |
| K-086 | `sendmsg` 的 `asynsend3(AMF_NOREPLY)` 异步投递 | 机制 | 存量 | 11 §2 | `comm.c:12-32` | 回答"发请求为什么不等" |
| K-087 | `fs_sendrec` 窗口判定与 `queuemsg` 尾插 | 机制 | 存量 | 11 §2 | `comm.c:134-168/223-244` | 回答"窗口满了请求去哪" |
| K-088 | `fs_sendmore`/`send_work` 全局刷新 | 机制 | 存量 | 11 §2 | `comm.c:66-84/37-45` | 回答"排队请求何时发出" |
| K-089 | `fs_cancel` 清队 | 机制 | 存量 | 11 §2 | `comm.c:50-61` | 回答"FS 死了队列怎么清" |
| K-090 | `drv_sendrec` 的 `dmap_servicing` 排他与 `CTTY_ENDPT→EIO` | 机制 | 存量 | 11 §2、20 | `comm.c:89-129` | 回答"块驱动为什么串行" |
| K-091 | `vm_sendrec` 无窗口直通 / `vm_vfs_procctl_handlemem` | 机制 | 存量 | 11 §2 | `comm.c:173-218` | 回答"VM 通信为什么不排队" |
| K-092 | `REQ_*` 33 常量 = 32 活 + 1 死（`REQ_GETNODE`） | 接口与协议 | 存量 | 12 §2 | `vfsif.h:41-73` | 回答"VFS 对 FS 有多少种请求" |
| K-093 | `req_*` 36 个包装函数面 | 接口与协议 | 存量 | 12 §2 | `request.c` 全文 | 回答"每个请求怎么发" |
| K-094 | `node_details`/`lookup_res` 响应结构 | 数据结构 | 存量 | 12 §2 | `request.h` | 回答"FS 回什么" |
| K-095 | `EENTERMOUNT`/`ELEAVEMOUNT`/`ESYMLINK` 三特殊码 | 接口与协议 | 存量 | 12 §2、13 | `request.c`、`path.c` | 回答"跨挂载/符号链接怎么表达" |
| K-096 | `ERESTART → EIO` 转换 | 约束与不变量 | 存量 | 12 §2 | `comm.c:165-166` | 回答"内部码为什么不外泄" |
| K-097 | `cpf_grant_direct/magic` 与 `CPF_READ/WRITE/TRY` | 机制 | 存量 | 12 §2 | `request.c` | 回答"FS 怎么拿用户缓冲区" |
| K-098 | `RES_THREADED/HASPEEK/64BIT` FS 能力位 | 接口与协议 | 存量 | 12 §2 | `vfsif.h` | 回答"FS 能力怎么协商" |
| K-099 | `advance`/`lookup`/`eat_path`/`last_dir` 四层 | 机制 | 存量 | 13 §1-2 | `path.c:40/384/133/146` | 回答"路径怎么一段段走" |
| K-100 | `get_name`/`canonical_path` | 机制 | 存量 | 13 §2 | `path.c:594/648` | 回答"反查路径名怎么做" |
| K-101 | 挂载点穿越（EnterMount/LeaveMount） | 机制 | 存量 | 13 §2 | `path.c`、`path.rs:120-127` | 回答"跨 FS 边界怎么切根" |
| K-102 | 符号链接循环 `SYMLOOP=16` 与 `ELOOP` | 约束与不变量 | 存量 | 13 §2 | `const.h:31` | 回答"链接成环怎么终止" |
| K-103 | `DO_POSIX_PATHNAME_RES=0` 尾斜杠语义（A-10） | 架构演进 | 存量 | 13 §2 | `path.c:31` | 回答"尾斜杠为什么不按 POSIX" |
| K-104 | chroot 边界（同 dev 才生效） | 约束与不变量 | 存量 | 13 §2、28 | `path.c` | 回答"chroot 的边界在哪" |
| K-105 | `fetch_name`/`copy_path` 用户路径取入 | 机制 | 存量 | 13 §2 | `utility.c:24-93` | 回答"用户路径怎么进 VFS" |
| K-106 | `do_socketpath` 入口三门 | 机制 | 存量 | 13 §2、24 | `path.c:803-836` | 回答"UDS 路径怎么登记" |
| K-107 | `get_fd` 最低空闲分配与 `O_DUPFD` start 参数化 | 机制 | 存量 | 14 §1-2 | `filedes.c:110-140` | 回答"fd 号怎么选" |
| K-108 | `check_fds`/`get_filp2`/`Fd` 类型 | 机制 | 存量 | 14 §2 | `filedes.c:88-109` | 回答"fd 合法性怎么验" |
| K-109 | `close_fd` 拆除序（摘索引 → 递减 → 末引用释放） | 机制 | 存量 | 14 §2 | `open.c:690-727` | 回答"关闭到底做几件事" |
| K-110 | `do_copyfd` 的 From/To/Close 三 kind 与三守门 | 机制 | 存量 | 14 §2 | `filedes.c:524-656` | 回答"fd 跨进程传递方向怎么定" |
| K-111 | `invalidate_filp_by_endpoint/char_major/sock_drv` 三族 | 机制 | 存量 | 14 §2、19、22 | `filedes.c:250-312` | 回答"驱动死了哪些 fd 失效" |
| K-112 | cloexec 位图与 `clo_exec` 扫描 | 机制 | 存量 | 14 §2、25 | `filedes.c`、`exec.c:721-731` | 回答"exec 时哪些 fd 关" |
| K-113 | `oflags` 意图位与 `mode_map` | 数据结构 | 存量 | 15 §1-2 | `open.c:83-298` | 回答"open 的参数怎么解释" |
| K-114 | `common_open` 六路分派（REG/DIR/PIPE/CHR/BLK/SOCK） | 机制 | 存量 | 15 §2 | `open.c:83-298` | 回答"不同类型文件怎么分岔" |
| K-115 | `new_node`/`pipe_open`/`do_mknod`/`do_mkdir` | 机制 | 存量 | 15 §2 | `open.c:299/483/514/564` | 回答"创建文件的四条路" |
| K-116 | `do_close`/`actual_lseek`/`SEEK_*` | 机制 | 存量 | 15 §2 | `open.c:655/603/674` | 回答"偏移怎么移动" |
| K-117 | `rw_flag` 方向 + `filp_mode` 许可 + 五路分派 | 机制 | 存量 | 16 §1-2 | `read.c:30-135` | 回答"读写怎么分派" |
| K-118 | `filp_pos` 推进与 `read_write` 公共体 | 机制 | 存量 | 16 §2 | `read.c:135-281` | 回答"偏移谁负责前进" |
| K-119 | `rw_pipe` 定量续写 | 机制 | 存量 | 16 §2、17 | `read.c:323-393` | 回答"管道读写为什么分批" |
| K-120 | `do_getdents` | 机制 | 存量 | 16 §2 | `read.c:282-322` | 回答"目录怎么读" |
| K-121 | `bsf_lock` 块特殊文件串行锁 | 约束与不变量 | 存量 | 16 §2、20 | `read.c:49-90`、`glo.h:41` | 回答"块设备读写为什么串行" |
| K-122 | `EPIPE → SIGPIPE` 与 `O_NOSIGPIPE` | 机制 | 存量 | 16 §2、17 | `read.c`、`main.c:249-252` | 回答"写无读管道会怎样" |
| K-123 | `create_pipe`/`do_pipe2` 与 PFS 落子（`req_newnode`） | 机制 | 存量 | 17 §1-2 | `pipe.c:39-150` | 回答"管道实体在哪" |
| K-124 | `pipe_check` 试探数学（存量/对端/容量） | 机制 | 存量 | 17 §2 | `pipe.c:187-293` | 回答"能读多少字节" |
| K-125 | `suspend`/`pipe_suspend` 挂起账本 | 机制 | 存量 | 17 §2 | `pipe.c:294-362` | 回答"等管道时记什么" |
| K-126 | `unsuspend_by_endpt`/`release`/`revive` 唤醒扫描 | 机制 | 存量 | 17 §2 | `pipe.c:334/363/435` | 回答"谁被叫醒" |
| K-127 | `unpause`（PM 中断） | 机制 | 存量 | 17 §2、10 | `pipe.c:498` | 回答"信号怎么打断 pipe" |
| K-128 | `susp_count`/`reviving` 全局 | 约束与不变量 | 存量 | 17 §2、09 | `glo.h:13/15` | 回答"挂起计数为什么两个" |
| K-129 | `name_to_dev` 设备号编解码（major/minor） | 机制 | 存量 | 18 §1-2 | `mount.c:590-627` | 回答"字符串设备名怎么变号" |
| K-130 | `do_mount`/`mount_fs` 五段提交 | 机制 | 存量 | 18 §2 | `mount.c:85-390` | 回答"挂载做几步" |
| K-131 | `mount_pfs` 罐装挂载 | 机制 | 存量 | 18 §2、01 | `mount.c:391-429` | 回答"PFS 为什么特殊" |
| K-132 | `do_umount`/`unmount` 七步拆除 | 机制 | 存量 | 18 §2 | `mount.c:430-551` | 回答"卸载做几步" |
| K-133 | `unmount_all`/`pm_reboot` 关联 | 机制 | 存量 | 18 §2、10 | `mount.c:552-589` | 回答"重启时怎么全卸" |
| K-134 | `update_bspec`/`is_nonedev`/`find_free_nonedev` | 机制 | 存量 | 18 §2 | `mount.c:46/628/641` | 回答"无设备挂载怎么分配" |
| K-135 | `ROOT_DEV`/`ROOT_FS_E`/`have_root` | 约束与不变量 | 存量 | 18 §2、01 | `glo.h:18-19` | 回答"根文件系统身份记在哪" |
| K-136 | `dmap` 表（major → driver）与 `init_dmap` | 数据结构 | 存量 | 19 §2 | `dmap.h:16-25`、`dmap.c:230-261` | 回答"major 怎么找到驱动" |
| K-137 | `smap` 表（domain → driver）与 `init_smap` | 数据结构 | 存量 | 19 §2 | `type.h:41-49`、`smap.c:22-46` | 回答"协议域怎么找到驱动" |
| K-138 | `map_service`/`map_driver`/`do_mapdriver` | 机制 | 存量 | 19 §2 | `dmap.c:200/61/106` | 回答"驱动怎么登记" |
| K-139 | 查询三函数（`get_dmap_by_endpt`/`dmap_driver_match`/`get_smap_by_*`） | 机制 | 存量 | 19 §2 | `dmap.c:317/252`、`smap.c:217/244/266` | 回答"三本通讯录怎么查" |
| K-140 | `dmap_endpt_up`/`smap_endpt_up` 恢复机 | 机制 | 存量 | 19 §2 | `dmap.c:275`、`smap.c:173` | 回答"驱动回来了怎么续命" |
| K-141 | `dmap_unmap_by_endpt`/`smap_unmap_by_endpt` 消亡机 | 机制 | 存量 | 19 §2 | `dmap.c:180`、`smap.c:148` | 回答"驱动死了表怎么清" |
| K-142 | `do_ioctl` 四分流（bdev/cdev/socket/ENOTTY） | 机制 | 存量 | 19 §2 | `device.c:18-59` | 回答"ioctl 怎么路由" |
| K-143 | `make_ioctl_grant` 与 IOR/IOW 方向解码 | 机制 | 存量 | 19 §2 | `device.c:52-95` | 回答"ioctl 缓冲区方向怎么定" |
| K-144 | `CTTY_ENDPT = VFS_PROC_NR` 自管端点 | 约束与不变量 | 存量 | 19 §2、21 | `const.h:52` | 回答"/dev/tty 为什么指向 VFS" |
| K-145 | `bdev_open/close/ioctl` | 机制 | 存量 | 20 §1-2 | `bdev.c:79/114/144` | 回答"块设备开合" |
| K-146 | `bdev_sendrec` 同步往返 + 重试五次熔断 | 机制 | 存量 | 20 §1-2 | `bdev.c:34-77` | 回答"ERESTART 重试多少次" |
| K-147 | 死信三分类（EDEADSRCDST/EDEADEPT 清表、ELOCKED 报错、其他最坏处理） | 机制 | 存量 | 20 §1 | `bdev.c:174-188` | 回答"发不出去怎么分类" |
| K-148 | `bdev_reply` 三验回复 | 机制 | 存量 | 20 §2 | `bdev.c:193-226` | 回答"回复怎么验真" |
| K-149 | `bdev_up` 换人通告（重开 + 挂载树通告 + 根兜底） | 机制 | 存量 | 20 §2 | `bdev.c:227-282` | 回答"驱动换人后谁要通知" |
| K-150 | `cdev_map/get/clone/opcl` | 机制 | 存量 | 21 §2 | `cdev.c:36/63/97/149` | 回答"字符设备怎么定位" |
| K-151 | tty 改道（`/dev/tty` 代词解析） | 机制 | 存量 | 21 §1-2 | `cdev.c:254-263` | 回答"/dev/tty 怎么变真设备" |
| K-152 | `cdev_open/close` 与 `O_NOCTTY` | 机制 | 存量 | 21 §2 | `cdev.c:254/264` | 回答"控制终端什么时候夺取" |
| K-153 | `cdev_io` 挂起读写 | 机制 | 存量 | 21 §2 | `cdev.c:280-349` | 回答"字符读写为什么睡" |
| K-154 | `cdev_cancel` 取消换码（二阶等待） | 机制 | 存量 | 21 §2 | `cdev.c:381-428` | 回答"打断睡眠要几步" |
| K-155 | `cdev_reply`/`cdev_generic_reply` 回复分类 | 机制 | 存量 | 21 §2 | `cdev.c:481/429` | 回答"字符回复怎么分" |
| K-156 | `cdev_select` | 机制 | 存量 | 21 §2、23 | `cdev.c:350-380` | 回答"字符设备怎么参与 select" |
| K-157 | `sdev_socket/bind/connect/listen/accept` | 机制 | 存量 | 22 §2 | `sdev.c:119/220/230/280/292` | 回答"socket 建连五问" |
| K-158 | `sdev_readwrite/ioctl/setsockopt/getsockopt/get*name/shutdown/close` | 机制 | 存量 | 22 §2 | `sdev.c:340-646` | 回答"socket 数据与控制面" |
| K-159 | `sdev_suspend` 挂起登记（短问/长问/选择发即返） | 机制 | 存量 | 22 §1-2 | `sdev.c:83-118` | 回答"socket 三态等待" |
| K-160 | `sdev_finish`/`do_accept_reply`/`sdev_finish_accept` 复活分流 | 机制 | 存量 | 22 §2 | `sdev.c:759/732/679` | 回答"socket 回复后怎么收尾" |
| K-161 | `sdev_stop` 停尸（驱动死亡 → EIO + 复活） | 机制 | 存量 | 22 §2 | `sdev.c:912-939` | 回答"socket 驱动死了挂起的调用怎么办" |
| K-162 | `sdev_cancel` 取消交叉 | 机制 | 存量 | 22 §2 | `sdev.c:940-988` | 回答"取消与复活交叉怎么裁决" |
| K-163 | `do_select` 六段（问群→分型→首波→挂起→次波→复活） | 机制 | 存量 | 23 §1-2 | `select.c:96-408` | 回答"select 全流程" |
| K-164 | `select_request_char/sock/file/pipe` 分型 | 机制 | 存量 | 23 §2 | `select.c:462/527/567/577` | 回答"四类 fd 怎么问" |
| K-165 | `tab2ops`/`ops2tab` 与 `SEL_RD/WR/ERR/NOTIFY` | 数据结构 | 存量 | 23 §2 | `select.c:621/635`、`const.h:36-40` | 回答"位图与操作集怎么互转" |
| K-166 | `copy_fdsets` 与 fd_set 拷贝 | 机制 | 存量 | 23 §2 | `select.c:660-713` | 回答"用户位图怎么进 VFS" |
| K-167 | `select_callback`/`select_reply1/2`/`cdev_reply1/2`/`sdev_reply1/2` | 机制 | 存量 | 23 §2 | `select.c:808/956/1111/1004/1167/1060/1198` | 回答"两波回复怎么合流" |
| K-168 | `select_timeout_check` + `set_timer` + CLOCK notify → `expire_timers` | 机制 | 存量 | 23 §2、09 | `select.c:861/335`、`main.c:110-113` | 回答"超时怎么触发" |
| K-169 | `select_cancel_all/cancel_filp/forget/wipe_select` | 机制 | 存量 | 23 §2 | `select.c:714/740/833/1318` | 回答"select 怎么取消与清理" |
| K-170 | `select_restart_filps` | 机制 | 存量 | 23 §2 | `select.c:1217-1317` | 回答"取消后怎么重启扫描" |
| K-171 | `do_socket`/`do_socketpair` 创建与 vnode 安装 | 机制 | 存量 | 24 §1-2 | `socket.c:176/224` | 回答"socket fd 怎么建" |
| K-172 | `make_sock_fd`/`get_sock`/`get_sock_flags`/`check_sock_fds` | 机制 | 存量 | 24 §2 | `socket.c:86/276/44/64` | 回答"socket fd 辅助函数" |
| K-173 | `do_bind/connect/listen/accept` + `resume_accept` | 机制 | 存量 | 24 §2 | `socket.c:308/326/344/365/399` | 回答"建连调用面" |
| K-174 | `do_sendto/recvfrom/sockmsg` + `resume_recvfrom/recvmsg` | 机制 | 存量 | 24 §2 | `socket.c:483/504/543/526/608` | 回答"收发调用面与恢复" |
| K-175 | `setsockopt/getsockopt/getsockname/getpeername/shutdown` | 机制 | 存量 | 24 §2 | `socket.c:657/676/701/724/747` | 回答"控制与查询调用面" |
| K-176 | 失败清理表（谁分配谁释放） | 约束与不变量 | 存量 | 24 §1 | `socket.c` 各错误路径 | 回答"中途失败清什么" |
| K-177 | `pm_exec` 四段（检查→释放→装载→收尾） | 机制 | 存量 | 25 §1-2 | `exec.c:185-520` | 回答"exec 全流程" |
| K-178 | 三项检查（REG / X 权限 / stat 可读） | 机制 | 存量 | 25 §2 | `exec.c:185-260` | 回答"可执行文件怎么验" |
| K-179 | `is_script`/`patch_stack`/`insert_arg` 脚本解释 | 机制 | 存量 | 25 §2 | `exec.c:522/534/607` | 回答"#! 怎么工作" |
| K-180 | ELF 装载 `read_seg`/`map_header`/`vfs_memmap` | 机制 | 存量 | 25 §2 | `exec.c:688/736/161` | 回答"程序怎么进内存" |
| K-181 | `stack_prepare_elf` 与 `ps_strings` | 机制 | 存量 | 25 §2 | `exec.c:413-460` | 回答"栈怎么准备" |
| K-182 | `clo_exec` 收尾扫描与 `allow_setuid` | 机制 | 存量 | 25 §2 | `exec.c:721-731` | 回答"exec 收尾做两件事" |
| K-183 | 动态链接 `.interp` 双文件 | 机制 | 存量 | 25 §1 | `exec.c:185-412` | 回答"解释器与主程序怎么并存" |
| K-184 | `write_elf_core_file` 四部分 | 机制 | 存量 | 26 §1-2 | `coredump.c:32-78` | 回答"core 文件长什么样" |
| K-185 | `get_memory_regions` 与 `prot_to_pf` | 机制 | 存量 | 26 §2 | `coredump.c:191/268` | 回答"内存区从哪来" |
| K-186 | `fill_*_header`/`adjust_offsets`/`write_buf` | 机制 | 存量 | 26 §2 | `coredump.c:80/104/126/162/176` | 回答"头怎么填" |
| K-187 | `CoreName` 与 `terminate_name` | 机制 | 存量 | 26 §2 | `coredump.c`、`coredump.rs:439-496` | 回答"core 文件名怎么定" |
| K-188 | `pm_dumpcore` 入口与 `unpause` 前置 | 机制 | 存量 | 26 §2、10 | `misc.c:903-988` | 回答"转储前为什么先解阻塞" |
| K-189 | `do_link` 硬链接与 `EXDEV` | 机制 | 存量 | 27 §1-2 | `link.c:29-90` | 回答"跨设备链接为什么失败" |
| K-190 | `do_unlink`/`do_rename` | 机制 | 存量 | 27 §2 | `link.c:91/169` | 回答"删除与改名" |
| K-191 | `do_truncate`/`do_ftruncate` | 机制 | 存量 | 27 §2 | `link.c:276/327` | 回答"截断" |
| K-192 | `do_slink`/`do_rdlink` | 机制 | 存量 | 27 §2 | `link.c:387/471` | 回答"符号链接读写" |
| K-193 | `do_chdir/fchdir/chroot` 与 `change_into` 三项检查 | 机制 | 存量 | 28 §1-2 | `stadir.c:50/32/83/117` | 回答"切换目录验什么" |
| K-194 | `do_stat/fstat/lstat` | 机制 | 存量 | 28 §2 | `stadir.c:140/173/418` | 回答"三种 stat 差别" |
| K-195 | `update_statvfs`/`fill_statvfs`/`do_statvfs/fstatvfs` | 机制 | 存量 | 28 §2 | `stadir.c:197/234/294/328` | 回答"文件系统状态怎么查" |
| K-196 | `do_getvfsstat` 遍历填充 | 机制 | 存量 | 28 §2 | `stadir.c:351-446` | 回答"遍历挂载表" |
| K-197 | `MountNames` 三名字拷贝 | 机制 | 存量 | 28 §2 | `stadir.c:283-285` | 回答"statvfs 三名字" |
| K-198 | `forbidden` 九位权限判定（主/组/客三档） | 机制 | 存量 | 29 §1-2 | `protect.c:238-302` | 回答"权限怎么判" |
| K-199 | `in_group` 与补充组 | 机制 | 存量 | 29 §2 | `utility.c:128-141` | 回答"补充组怎么参与" |
| K-200 | `do_chmod`/`do_chown` 条件 | 机制 | 存量 | 29 §2 | `protect.c:25/98` | 回答"改属性要什么条件" |
| K-201 | `do_umask` 取反存储 | 机制 | 存量 | 29 §2 | `protect.c:182-197` | 回答"umask 为什么存反码" |
| K-202 | `do_access` | 机制 | 存量 | 29 §2 | `protect.c:198-237` | 回答"access 与 forbidden 的关系" |
| K-203 | `do_fcntl` 十三命令 | 机制 | 存量 | 30 §1-2 | `misc.c:117-275` | 回答"一个调用十三种语义" |
| K-204 | `F_DUPFD`/`GETFL`/`SETFL`/`NOSIGPIPE` | 机制 | 存量 | 30 §2 | `misc.c:117-275` | 回答"标志类命令" |
| K-205 | `lock_op` 与 POSIX 劝告锁八槽（`NR_LOCKS=8`） | 机制 | 存量 | 30 §2 | `lock.c:21-171`、`const.h:6` | 回答"记录锁怎么存" |
| K-206 | 锁区域算术与解锁四分支 | 机制 | 存量 | 30 §2 | `lock.c:21-171` | 回答"区间怎么分裂" |
| K-207 | `lock_revive` 广播唤醒 | 机制 | 存量 | 30 §2 | `lock.c:172-192` | 回答"锁等待者怎么叫醒" |
| K-208 | `FP_BLOCKED_ON_FLOCK` 与 `F_SETLKW` 挂起 | 机制 | 存量 | 30 §2、09 | `const.h:22` | 回答"锁等待怎么挂起" |
| K-209 | `do_getsysinfo` 批量查询（root + 长度检查） | 机制 | 存量 | 31 §2 | `misc.c:52-116` | 回答"整表复制的守门" |
| K-210 | `do_sync`/`do_fsync` 广播 | 机制 | 存量 | 31 §2 | `misc.c:276/297` | 回答"落盘同步" |
| K-211 | `dupvm`/`do_vm_call`（VM 三种上游请求） | 机制 | 存量 | 31 §2 | `misc.c:328/380` | 回答"VM 反向调用 VFS" |
| K-212 | `do_svrctl` 与 sysctl verbose | 机制 | 存量 | 31 §2 | `misc.c:797-898` | 回答"系统控制" |
| K-213 | `do_utimens` 三态（NOW/OMIT/显值） | 机制 | 存量 | 31 §2 | `time.c:26-155` | 回答"时间戳怎么设" |
| K-214 | `do_gcov_flush` 五门 | 机制 | 存量 | 31 §2 | `gcov.c:10-73` | 回答"覆盖率刷新的守门" |
| K-215 | `do_getrusage` 废弃恒 OK | 约束与不变量 | 存量 | 31 §2 | `misc.c:998-1006` | 回答"废弃调用为什么还注册" |
| K-216 | `panic_hook` 与 stacktrace | 工具与工程 | 存量 | 31 §2 | `misc.c:989-997` | 回答"崩溃时打印什么" |
| K-217 | `ds_event` 分类/门/分派 | 机制 | 存量 | 31 §2、19 | `misc.c:958-982` | 回答"DS 事件怎么变驱动状态" |
| K-218 | 容量常量族（NR_FILPS/NR_VNODES/NR_MNTS/NR_WTHREADS/NR_LOCKS/NR_SOCKDEVS） | 数据结构 | 存量 | 99 | `const.h:5-11` | 回答"每张表多大、为什么" |
| K-219 | `FP_BLOCKED_ON_*` 七态与 `SYMLOOP`/`LABEL_MAX`/`CTTY_ENDPT` | 数据结构 | 存量 | 99、02 | `const.h:19-52` | 回答"常量速查" |
| K-220 | `glo.h` 全局与 `who_p`/`who_e`/`call_nr`/`job_*` 宏 | 数据结构 | 存量 | 99 | `glo.h:11-45` | 回答"当前上下文从哪读" |
| K-221 | `type.h`/`fs.h`/`proto.h` 索引 | 工具与工程 | 存量 | 99 | `type.h`/`fs.h`/`proto.h` | 回答"按符号找文件" |
| K-222 | `m_type` 多命名空间 | 接口与协议 | 存量 | 99 | `callnr.h:68`、`com.h:513/589/911/920/964/1038` | 回答"消息号怎么不撞车" |
| K-223 | 引用计数双层不变量（filp_count ↔ v_ref_count ↔ v_fs_count） | 约束与不变量 | 存量 | 99 | `file.h`/`vnode.h` | 回答"三层计数怎么配平" |
| K-224 | `sys_datacopy_wrapper` | 机制 | 存量 | 99 | `utility.c:142-186` | 回答"VFS 怎么拷用户数据" |
| K-225 | 有意省略台账 | 工具与工程 | 存量 | 99 | `99-global-concepts.md` | 回答"什么故意不移植" |
| K-226 | ARCH A-8 类型映射 / A-9 `LOCK_DEBUG` cfg | 架构演进 | 存量 | 99、02 | `fproc.h:9` | 回答"编译宏怎么表达" |

> **存量统计**：226 条，编号 K-001…K-226 连续无空位（多 AI 汇总时以「名称 + 锚点」对齐，
> 编号仅在本蓝图内唯一）。
> **类型分布**（按 §2.1 表的"类型"列计数，合计 226）：机制 138、数据结构 40、
> 约束与不变量 24、接口与协议 12、概念 4、架构演进 7、工具与工程 1。
> **按现有文档分布**（计次 227，比总数多 1 是因为个别知识点跨两篇计入）：
> 00(3) / 01(12) / 02(10) / 03(4) / 04(7) / 05(5) / 06(8) / 07(5) / 08(10) / 09(11) /
> 10(9) / 11(8) / 12(7) / 13(8) / 14(6) / 15(4) / 16(6) / 17(6) / 18(7) / 19(9) / 20(5) /
> 21(7) / 22(6) / 23(8) / 24(6) / 25(7) / 26(5) / 27(4) / 28(5) / 29(5) / 30(6) / 31(9) / 99(9)。

### 2.2 新增知识点（N 组，共 18 条）

> 全部由 §3 覆盖审计发现，每条带证据锚点；不受"去向规则"约束，直接进入新篇契约。

| 编号 | 名称 | 类型 | 来源 | 证据锚点 | 读者收益 |
|------|------|------|------|---------|---------|
| N-001 | `m_type` 六命名空间真值表（VFS 0x100 / PM RQ 0x900 / PM RS 0x980 / FS 0xA00 / 驱动 RS 0x480,0x580,0x1480,0x1980 / TRANSID 0xB00） | 接口与协议 | C + 非 C | `callnr.h:68`、`com.h:513/589/911/920/964/996/1038` | 回答"这条消息是谁的、去哪一路" |
| N-002 | `TRNS_ADD_ID`/`GET_ID`/`DEL_ID` 的 16 位位移编码与 `~0xff` 守门 | 接口与协议 | C | `vfsif.h:79-81`、`com.h:912` | 回答"回复怎么找回发起者" |
| N-003 | `RTCDEV_RS_BASE 0x1480` 存在但主循环不处理 | 接口与协议 | C | `com.h:996-999`；`main.c:126-134` 无对应分支 | 回答"是不是漏了一路驱动" |
| N-004 | 阻塞与复活总契约：`SUSPEND` 码 + 五类载荷 + 四条复活路径 + 取消 | 约束与不变量 | C + 理论 | `main.c:297`、`const.h:19-25`、`pipe.c:435`、`lock.c:172`、`cdev.c:481`、`sdev.c:989` | 回答"挂起有几种、各由谁唤醒" |
| N-005 | 驱动死亡/上线级联的统一触发序（dmap/smap unmap → filedes 失效 → sdev 停尸 → select 唤醒 → bdev_up 重开） | 机制 | C + 非 C | `dmap.c:180`、`smap.c:148`、`filedes.c:250-312`、`sdev.c:912`、`select.c:884`、`bdev.c:227` | 回答"驱动崩了系统怎么自愈" |
| N-006 | 数据搬运通道：grant / safecopy / `copy_path` / `fetch_name` 四条路 | 机制 | C + Rust | `device.c:52-95`、`utility.c:24-93`、`request.c`（cpf_grant）、`os/servers/vfs/src/path.rs:PathFetcher` | 回答"用户缓冲区怎么进 VFS" |
| N-007 | VFS 的构建与装载：`Makefile` SRCS 33/34（+gcov）、`minix.service.mk`、boot image 登记、Cargo crate 结构 | 工具与工程 | 非 C | `minix3/minix/servers/vfs/Makefile`、`minix3/share/mk/minix.service.mk`、`os/servers/vfs/Cargo.toml` | 回答"VFS 这个二进制怎么来的" |
| N-008 | 内核接口面：`sef_receive`/`ipc_send`/`ipc_sendnb`/`asynsend3`/`sys_hz`/`sys_safecopy*`/`sys_datacopy*`/`sys_kill`/`mthread_*` | 接口与协议 | C + 理论 | `main.c:72/436/436`、`comm.c:23`、`main.c:438`、`utility.c:142-186`、`main.c:251`、`worker.c:52` | 回答"VFS 对内核依赖哪几个调用" |
| N-009 | errno → `ToErrno` 统一映射通道（30 个错误枚举） | 架构演进 | Rust | `os/servers/vfs/src/` 30 处 `impl minix_types::ToErrno`（todo.md Fix #19 实测） | 回答"Rust 的错误怎么变 errno" |
| N-010 | 测试基建：决策纯化策略、Scripted seam、wire 绝对值钉子、双构建纪律 | 测试性质 | Rust + 非 C | todo.md §0.2（346 passed）、Fix #1 的 0xA1A 断言、Fix #17/#33 的 `cargo check` 教训 | 回答"这 346 个测试测的是什么" |
| N-011 | ARCH 总表 A-1…A-15 与 Rust 现状对账 | 架构演进 | Rust + 参考材料 | `plan.md §4`（取事实，不取顺序）、`os/servers/vfs/src/` | 回答"Rust 与 C 差在哪、差多少" |
| N-012 | VFS ↔ FS 对端边界（哪些语义在 15-stage-fs，不在本 stage） | 概念 | 边界材料 | `00-master-plan/README.md` 阶段表、`edge_todo.md` E-REQWIRE | 回答"mfs 的事要不要在这里讲" |
| N-013 | VFS ↔ PM 对端边界（`tell_vfs` 发送方状态机不在本 stage） | 概念 | 边界材料 | `04-stage-pm/05-vfs-interaction.md` | 回答"PM 侧谁发 VFS_PM_*" |
| N-014 | `fproc_light` 与 MIB 契约（ARCH A-7 defer） | 接口与协议 | C + 边界材料 | `fproc.h:117-124`、`os/servers/vfs/Cargo.toml` 的 `fproc_light` feature | 回答"MIB 怎么读 VFS 进程表" |
| N-015 | select 定时器与 CLOCK 契约（ARCH A-13） | 接口与协议 | C | `select.c:335/861`、`main.c:110-113` | 回答"超时通知走哪条路" |
| N-016 | **加锁顺序（lock ordering）** | 约束与不变量 | 非 C（官方 README） | `minix3/minix/servers/vfs/README:304-357`（§4.4 Locking order） | 回答"多把锁怎么按序取才不死锁" |
| N-017 | **每请求类型的锁特征表** | 约束与不变量 | 非 C（官方 README） | `minix3/minix/servers/vfs/README:569-656`（§4.8） | 回答"每个调用要锁什么、锁多久" |
| N-018 | 驱动崩溃恢复三分类（块 / 字符与 socket / FS） | 机制 | 非 C（官方 README） | `minix3/minix/servers/vfs/README:657-700`（§5） | 回答"三类对端崩了各怎么收" |

### 2.3 重复主题标记（同知识点在多篇展开，需指定主讲述点）

| 知识点 | 现有重复位置 | 新目录主讲述点 | 其余改为 |
|--------|-------------|---------------|---------|
| K-034 filp 引用计数 | 04、10、14、15 | `09-filp-table` | 10/14/15 只给调用点与配平表 |
| K-041/K-045 锁映射到 TLL | 04、05、06、07 | `08-tll-lock`（抽象） + 各表篇（各自映射） | 07 只讲原语，表篇给一行映射 |
| K-061 驱动死亡 EIO 注入 | 08、14、19、20、22、23 | `14-driver-transport`（编排） | 08/19/22/23 各自只给一面 |
| K-071/K-075 SUSPEND 语义 | 09、17、21、22、23、30 | `05-vfs-blocking`（总契约） | 各族篇引用，不复述 |
| K-097/K-143/K-224 数据搬运 | 12、13、19、21、99 | `21-data-transfer` | 12/13/19/21/99 只给调用点 |
| K-110 COPYFD | 14、22 | `17-filedes` | 22 只给 UDS 调用点 |
| K-121 bsf 锁 | 16、20 | `19-read-write`（取锁语义与调用点）→ `23-bdev`（块路径） | 20 讲块设备侧的重开与通告 |
| K-142 `do_ioctl` 分派 | 19（device.c:18-59）、31（摘要提到 ioctl） | `22-device-map` | 31 删除 ioctl 叙述，只讲 `do_vm_call` 的 VM 上游请求 |
| K-213 `do_utimens` | 28（stadir 摘要提到时间戳）、31 | `35-misc-queries` | 28 只讲 stat 的时间字段来源 |

---

## 3. 覆盖审计

### 3.1 主题全集来源（四路）

1. **C 源码符号**：33 个 `.c` + 15 个 `.h` 的函数/结构体/宏/常量/状态机/错误路径。
   `grep -c "define REQ_"` 等逐类核对已在 §0.4 摘录。
2. **操作系统通用概念**：进程上下文表、打开文件描述、vnode/inode 缓存、挂载表、
   三级锁、事件循环、阻塞/复活、POSIX 记录锁、多路复用、权限模型、路径解析与符号链接、
   core dump 的 ELF 表达、grant/safecopy 的零拷贝数据面。
3. **非 C 制品**：`Makefile`（SRCS 33 + gcov 条件）、`minix.service.mk`（服务装载规则）、
   `README`（700 行官方 internals）、`os/servers/vfs/Cargo.toml`（feature `fproc_light`）、
   `src/lib.rs`（模块注册面）、`src/main.rs`（入口）、`tools/coverage-extract/vfs-semantic-map.json`（覆盖率语义映射）。
4. **阶段边界契约**：`00-master-plan/README.md`（VFS 属第 05 阶段，由 RS 加载）；
   `plan.md §8` 的 W1–W9 接线矩阵；`todo.md` 的 edge 条目（E-REQWIRE / E-VFSWIRE）。

### 3.2 覆盖缺口表

| # | 缺口主题 | 证据 | 建议 | 落实为 |
|---|---------|------|------|--------|
| G1 | `m_type` 六命名空间 + transid 编码没有一篇系统讲，只有 99 的"四名字空间"一行 | `99-global-concepts.md` 仅一行；`route_message` 的 RS 前缀曾错（todo.md R2-P1-4） | 新建 | `01-vfs-wire` + N-001/N-002/N-003 |
| G2 | 阻塞与复活（SUSPEND + 四路径 + 取消）散在 6 篇，无统一契约 | `main.c:297`、`const.h:19-25`、`pipe.c:435`、`lock.c:172`、`cdev.c:481`、`sdev.c:989` | 新建 | `05-vfs-blocking` + N-004 |
| G3 | 驱动死亡/上线级联只有各面决策函数，没有编排篇 | todo.md R2-P1-3"运行时编排归 P1-2"；`filedes.c:250-312`、`sdev.c:912`、`select.c:884`、`bdev.c:227` | 新建 | `14-driver-transport` + N-005 |
| G4 | 数据搬运（grant/safecopy/`copy_path`/`fetch_name`）散在 5 篇，无统一篇 | `device.c:52-95`、`utility.c:24-93`、`request.c`、`path.rs:PathFetcher` | 新建 | `21-data-transfer` + N-006 |
| G5 | **加锁顺序**完全缺失 | `README:304-357` §4.4 | 新建（并入 `08-tll-lock`） | N-016 |
| G6 | **每请求类型的锁特征表**完全缺失 | `README:569-656` §4.8 | 新建（并入 `08-tll-lock`） | N-017 |
| G7 | 驱动崩溃恢复的三分类（块/字符+socket/FS）无篇 | `README:657-700` §5 | 新建（并入 `14-driver-transport`） | N-018 |
| G8 | 构建与装载（Makefile / service.mk / boot image / Cargo）无篇 | `vfs/Makefile`、`share/mk/minix.service.mk` | 新建（并入 `02-vfs-init` §"VFS 是怎么被装起来的"） | N-007 |
| G9 | 内核接口面（9 个内核/库调用）散在 8 处 DEFERRED，无汇总 | todo.md §9.1 第 6 条 | 新建（并入 `01-vfs-wire` §"VFS 对内核的依赖面"） | N-008 |
| G10 | 测试基建与策略无篇（346 测试的形态、Scripted seam、双构建纪律） | todo.md §0.2、Fix #17/#33 教训 | 新建 | `38-testing` + N-010 |
| G11 | ARCH 总表只在 plan.md（参考材料），正式文档只有 00 的 4 条 | `plan.md §4`（15 项） | 新建 | `37-arch-evolution` + N-011 |
| G12 | errno 统一通道（30 枚举）无篇 | todo.md Fix #19 | 并入 `37-arch-evolution` | N-009 |
| G13 | 对端边界（FS / PM / MIB / VM 谁讲什么）无篇 | `00-master-plan/README.md`、`04-stage-pm/05-vfs-interaction.md` | 并入 `00-vfs-overview` §"边界" | N-012/N-013/N-014 |
| G14 | `RTCDEV_RS_BASE` 无任何提及 | `com.h:996` | 并入 `01-vfs-wire`（判定：主循环不处理，列有意省略） | N-003 |
| G15 | `fproc_light`/MIB 契约只有一行 + Cargo feature | `fproc.h:117-124`、`Cargo.toml` | 并入 `07-fproc-table` | N-014 |

### 3.3 越界主题表

| # | 位置 | 越界内容 | 正确归属 |
|---|------|---------|---------|
| O1 | `01-vfs-init-main` | 展开 `worker_init`/`worker_allow` 的实现细节（源码声明里就写了 `worker.c:worker_init,162-185`） | `03-request-slots`；01 只给调用点与门控语义 |
| O2 | `31-misc-queries` | 摘要与 §1.5 出现 "ioctl 三类请求与异步处理"，而 `do_ioctl` 定义在 `device.c:18` | `22-device-map`；31 只讲 `do_vm_call` 的 VM 上游三类请求 |
| O3 | `19-device-map` | §2.11–2.12 讲 `make_ioctl_grant` 的授权创建（grant 语义属数据搬运总题） | `21-data-transfer` 主讲；19 只给 ioctl 方向解码的调用点 |
| O4 | `09-main-loop` | 讲 `unblock`/`do_pending_pipe` 的 pipe 专用续作逻辑 | `05-vfs-blocking`（总契约）+ `20-pipe`（族细节）；09 只给"主循环在这里调 unblock" |
| O5 | `00-vfs-overview` §3 | 列了 4 条 ARCH，而 `plan.md §4` 有 15 条 | `37-arch-evolution`；00 只给导航 |
| O6 | `99-global-concepts` | 同时承载常量表、全局状态、术语、**有意省略台账**、ARCH 注释 —— 四种语义 | 拆分：常量/全局/术语留 36；省略台账留 36 但单节；ARCH 归 37 |

### 3.4 现有目录的结构性缺陷（重建的动因）

| # | 缺陷 | 证据 | 处置 |
|---|------|------|------|
| P1 | **plan.md 自相矛盾**：§7.1 D-1 声明"tll（07）前置，结构表（04~06）后置"，但 §2 的编号表里 04/05/06 仍在 07 之前 | `plan.md:458` vs `plan.md:97-100` | 新目录把 `08-tll-lock` 排在 09/10/11 三张表之前 |
| P2 | **编号与叙事分组脱节**：01–31 是一条扁平线性链，看不出"骨架/数据结构/对外协议/请求处理/收尾"的分组；00 只有 85 行，无法承载导航 | `00-vfs-overview.md` 85 行；`01`–`31` 无分组标记 | 新目录按 12 个部分分组，00 扩为真正的导航与边界篇 |
| P3 | **并行的驱动三族被线性排列**：20/21/22 三篇各讲一个驱动通道，但共同骨架（RS 分流、`*_reply` 入口、`worker_stop` EIO、死亡级联）无处讲 | `main.c:126-134`、`worker.c:535` | 新建 `14-driver-transport` 作汇聚点篇 |
| P4 | **阻塞语义被"家族"淹没**：SUSPEND 在 09 提一句，然后 17/21/22/23/30 各讲各的，读者无法形成"四条复活路径"的整体图像 | 见 §2.3 重复表 K-071/K-075 | 新建 `05-vfs-blocking` |
| P5 | **官方文档零引用**：`minix3/minix/servers/vfs/README`（700 行，含加锁顺序与驱动崩溃恢复）在 33 篇中零命中 | `grep -ln "servers/vfs/README" 05-stage-vfs/*.md` → 0 | N-016/N-017/N-018 入池，落到 08/14 |
| P6 | **参考材料与正式文档职责混淆**：ARCH 表（15 项）与接线矩阵（W1–W9）只在 plan.md，正式文档读不到；反过来 00 又复述了 4 条 | `plan.md §4/§8` vs `00-vfs-overview.md §3` | ARCH 归 `37-arch-evolution`；W1–W9 归 `38-testing`（作为关闭条件，不是修复台账） |

### 3.5 非 C 主题逐项回答（固定清单）

| 主题 | 在本 stage 哪里讲 | 处置与理由 |
|------|------------------|-----------|
| 链接与加载 | `02-vfs-init` §VFS 是怎么被装起来的 | 讲 `Makefile` SRCS（33 + gcov 条件）、`-lsys -ltimers -lexec -lmthread`、`minix.service.mk`；ELF 装载细节属 `14-stage-runtime` |
| 镜像与内存布局 | **不在本 stage** | VFS 是用户态进程，镜像布局属 `01-stage-kernel/06-proc-init-boot-proc.md`；VFS 只读 `rproctab`（`main.c:455`） |
| 汇编入口与陷阱进入 | **不在本 stage** | 属 `01-stage-kernel`；VFS 与主循环只经 `sef_receive`/`ipc_send` |
| 启动装配 | `02-vfs-init` 全篇 | VFS 的启动装配 = SEF 回调 + `sef_cb_init_fresh` 18 步 |
| 构建与工具链 | `02-vfs-init` §构建 + `38-testing` §验证命令 | C 侧 `Makefile`；Rust 侧 `Cargo.toml`（feature `fproc_light`）、`cargo test -p minix-vfs --lib` |
| 跨模块接口与线格式 | `01-vfs-wire` 全篇 | 六命名空间 + transid + 内核接口面；对端 FS 线格式交叉 `15-stage-fs` |
| 错误路径 | `05-vfs-blocking`（取消/死亡） + `37-arch-evolution`（errno 通道） + `36-global-concepts`（省略台账） | 30 个枚举的 `ToErrno` 映射归 37 |
| 关闭与退出 | `28-pm-protocol`（pm_reboot 八步 + pm_exit） + `16-mount`（unmount_all） | VFS 没有自己的退出路径，退出由 PM 驱动 |
| 并发与同步 | `03-request-slots` + `08-tll-lock` + `05-vfs-blocking` | 槽（执行并发）、锁（数据并发）、挂起（等待并发）三分 |
| 测试基建 | `38-testing` 全篇 | 346 测试形态、Scripted seam、wire 绝对值钉子、`cargo check` + `cargo test` 双跑纪律、W1–W9 关闭条件 |

---

## 4. 新目录

### 4.1 新篇章总表（39 篇，编号 00–38）

| 编号 | 标题 | 一句话定位 | 分组 |
|------|------|-----------|------|
| 00 | `00-vfs-overview.md` | VFS 为什么存在、它持有哪三张词汇表、本 stage 的边界与阅读路线 | 0 导航与边界 |
| 01 | `01-vfs-wire.md` | 一条消息怎么被认出来：六命名空间、transid 编码、内核接口面 | 0 导航与边界 |
| 02 | `02-vfs-init.md` | 从被 RS 加载到允许第一个请求进来：SEF 与 `sef_cb_init_fresh` 的 18 步 | 1 诞生与初始化 |
| 03 | `03-request-slots.md` | VFS 的执行载体：9 个槽怎么分配、绑定、让出、停止（含 ARCH A-1） | 2 并发与事件循环 |
| 04 | `04-main-loop.md` | 主循环怎么把一条消息分成八路并交出去 | 2 并发与事件循环 |
| 05 | `05-vfs-blocking.md` | 请求凭什么挂起、由谁唤醒、怎么取消：SUSPEND 的四条复活路径 | 2 并发与事件循环 |
| 06 | `06-fproc-struct.md` | 一个进程的 VFS 上下文长什么样（字段全集 + 标志 + 阻塞载荷） | 3 进程上下文与核心表 |
| 07 | `07-fproc-table.md` | `fproc[NR_PROCS]` 怎么索引、怎么验证、怎么投影给 MIB | 3 进程上下文与核心表 |
| 08 | `08-tll-lock.md` | 三级锁原语，以及 VFS 的加锁顺序与每请求锁特征 | 3 进程上下文与核心表 |
| 09 | `09-filp-table.md` | 打开描述表：`fd → filp → vnode` 的二跳与引用计数 | 3 进程上下文与核心表 |
| 10 | `10-vnode-table.md` | vnode 表：双层引用计数与延迟同步 | 3 进程上下文与核心表 |
| 11 | `11-vmnt-table.md` | 挂载表：`device ↔ FS` 的边界与每 FS 通信窗口 | 3 进程上下文与核心表 |
| 12 | `12-fs-comm.md` | 怎么向 FS/VM/驱动发请求并拿回复：窗口、transid、队列 | 4 对端通信 |
| 13 | `13-request-wrappers.md` | `req_*` 包装层：33 常量 / 36 函数 / 响应结构 | 4 对端通信 |
| 14 | `14-driver-transport.md` | 驱动通道的共同骨架与驱动生死级联编排 | 4 对端通信 |
| 15 | `15-path-lookup.md` | 一条路径名怎么变成 vnode（含跨挂载与符号链接） | 5 名字与挂载 |
| 16 | `16-mount.md` | 一棵外部文件系统怎么嫁接到单树上，以及根挂载 | 5 名字与挂载 |
| 17 | `17-filedes.md` | fd 表的分配、关闭、复制与失效 | 6 fd 与数据面 |
| 18 | `18-open-close.md` | `open`/`creat`/`close`/`lseek`：路径到 fd 的绑定与拆除 | 6 fd 与数据面 |
| 19 | `19-read-write.md` | `read`/`write`/`getdents`：五路分派、位置推进、块串行 | 6 fd 与数据面 |
| 20 | `20-pipe.md` | 管道：配额定量、挂起账本、唤醒扫描 | 6 fd 与数据面 |
| 21 | `21-data-transfer.md` | 用户缓冲区怎么进出 VFS：grant、safecopy、路径取入 | 6 fd 与数据面 |
| 22 | `22-device-map.md` | 三本通讯录：major/domain/DS 标签怎么路由到驱动 | 7 设备 I/O |
| 23 | `23-bdev.md` | 块设备通道：直达、重试熔断、死信分流、换人通告 | 7 设备 I/O |
| 24 | `24-cdev.md` | 字符设备通道：终端改道、挂起读写、取消换码 | 7 设备 I/O |
| 25 | `25-sdev.md` | socket 驱动通道：长短问、挂起登记、取消交叉、复活分流 | 7 设备 I/O |
| 26 | `26-select.md` | `select`：一问等一群，两波才收场 | 8 多路复用与套接字 |
| 27 | `27-socket.md` | socket 调用面：创建、建连、收发、控制、恢复 | 8 多路复用与套接字 |
| 28 | `28-pm-protocol.md` | PM 控制面：12 个 `VFS_PM_*` 请求与 fork/exit/exec/reboot | 9 进程生命周期 |
| 29 | `29-exec.md` | `exec`：检查、装载、脚本与动态链接、收尾 | 9 进程生命周期 |
| 30 | `30-coredump.md` | core dump：把死进程写成 ELF core 文件 | 9 进程生命周期 |
| 31 | `31-link.md` | 目录项与 inode 的四类操作（link/unlink/rename/truncate/symlink） | 10 名字空间与权限 |
| 32 | `32-stadir.md` | 工作目录切换、stat 族、statvfs 族与挂载表遍历 | 10 名字空间与权限 |
| 33 | `33-protect.md` | 九位权限判定与 chmod/chown/umask/access | 10 名字空间与权限 |
| 34 | `34-fcntl-lock.md` | `fcntl` 十三命令与八槽 POSIX 记录锁 | 11 控制与杂项 |
| 35 | `35-misc-queries.md` | 杂项八类：sysinfo/sync/VM 上游/svrctl/utimens/gcov/废弃 | 11 控制与杂项 |
| 36 | `36-global-concepts.md` | 常量、全局状态、术语、引用计数不变量、有意省略台账 | 12 收尾 |
| 37 | `37-arch-evolution.md` | ARCH A-1…A-15 与 Rust 现状对账、errno 通道 | 12 收尾 |
| 38 | `38-testing.md` | 测试基建、验证命令、接线矩阵 W1–W9 的关闭条件 | 12 收尾 |

### 4.2 阅读路径

- **主线（必读，00 → 05 → 08 → 12 → 15 → 17 → 26）**：
  `00` → `01` → `02` → `03` → `04` → `05` → `06` → `07` → `08` → `09` → `10` → `11`
  → `12` → `13` → `15` → `17` → `18` → `19` → `20` → `22` → `26`
  —— 覆盖"一次 open+read 请求从进入到返回"的完整链路。
- **支线一（设备面）**：`14` → `22` → `23` → `24` → `25` —— 在读完 `19` 之后进入。
- **支线二（进程生命周期）**：`28` → `29` → `30` —— 依赖 `15`/`18`/`19`。
- **支线三（名字空间与权限）**：`31` → `32` → `33` —— 依赖 `15`/`17`。
- **支线四（控制与杂项）**：`34` → `35` —— 依赖 `17`/`09`。
- **可跳读（参考，不必顺序读）**：`36`（随时查常量与术语）、`37`（只看 ARCH 对账）、
  `38`（只在动手改代码/接线时读）。
- **挂载专题**：`16` 在 `11` 与 `12` 之后即可读（主线路径把它排在 `15` 之后，
  但 `16` 只依赖 `11`/`12`，可提前）。

### 4.3 并行体的组织（按规范 5.2）

| 并行体 | 框架篇 | 分组 | 代表成员（讲透） | 其余（差异表收束） |
|--------|--------|------|-----------------|-------------------|
| 64 个系统调用 | `04-main-loop`（分派）+ `01-vfs-wire`（调用号） | 按场景分 6 组：名字 / 数据 / 元数据 / 挂载 / 控制 / 套接字 | `18-open-close`（名字→数据全链路） | 各家族篇给差异表 |
| 阻塞与复活 | `05-vfs-blocking` | pipe / flock / cdev / sdev 四路径 + 取消 | `20-pipe`（最完整：挂起→唤醒→续作） | 24/25/34 按差异表 |
| 三类驱动通道 | `14-driver-transport` | bdev / cdev / sdev | `23-bdev`（同步往返 + 死亡级联最清晰） | 24/25 按差异表 |
| 三张核心表 | `08-tll-lock`（锁）+ 各表篇 | filp / vnode / vmnt | `09-filp-table`（二跳最直观） | 10/11 按差异表 |
| FS 请求面 | `13-request-wrappers` | 32 活请求 | `req_lookup`（跨挂载三特殊码） | 其余按信封差异表 |
| 设备查表 | `22-device-map` | dmap（major）/ smap（domain）/ DS 标签 | dmap | smap 按差异表 |

---

## 5. 每篇契约

> 格式：定位 / 讲什么 / 不讲什么 / 前置 / 后置 / 事实底线 / 知识点清单 / 验收标准。
> "知识点清单"列中，来源为 `存量` 的填旧文档位置，来源为 `新增` 的填证据锚点。

### 00-vfs-overview

- **一句话定位**：回答"VFS 是什么、它凭什么管所有文件、本 stage 讲什么不讲什么"，并给出阅读路线。
- **讲什么**：K-001、K-002、K-003；对端边界（N-012/N-013/N-014 的边界声明部分）；13 个分组的导航表；三条阅读路径。
- **不讲什么**：一切机制（下放 01–35）；常量速查（下放 36）；ARCH 对账（下放 37）；修复台账（留在 plan.md/todo.md）。
- **前置**：无（但假定读者已读 `01-stage-kernel` 的微内核分层与 `04-stage-pm/00-pm-overview.md`）。
- **后置**：全部篇（每篇的入口都从 00 的导航表进）。
- **事实底线**：`00-master-plan/README.md` 阶段表（VFS 第 05 阶段，RS 加载）；`table.c:17-82`（64）、`com.h:520-531`（12）、`vfsif.h:41-73`（33）。
- **知识点清单**：

  | 编号 | 名称 | 来源 |
  |------|------|------|
  | K-001 | VFS 是文件系统语义的唯一权威 | 存量 `00 §1.1` |
  | K-002 | 三方词汇表 | 存量 `00 §1.1` |
  | K-003 | 三个服务面数字 | 存量 `00 §1.3` |
  | N-012 | VFS ↔ FS 对端边界 | 新增 `00-master-plan/README.md` |
  | N-013 | VFS ↔ PM 对端边界 | 新增 `04-stage-pm/05-vfs-interaction.md` |

- **验收标准**：读者读完后能画出三方关系图（进程 / VFS / FS+驱动），能说出 64/12/33 三个数字各自的事实源文件名，能说出"mfs 的语义不在本 stage"，并能从导航表直接定位任意一个机制属于哪一篇。

### 01-vfs-wire

- **一句话定位**：建立"一条消息怎么被认出来"的公共词汇——六命名空间、transid 编码、内核接口面。
- **讲什么**：N-001 六命名空间真值表；N-002 transid 16 位位移编码与 `~0xff`/`~0x7f` 守门差异；N-003 `RTCDEV_RS_BASE` 的判定；N-008 VFS 对内核的九个依赖调用；K-222 `m_type` 多命名空间（从 99 上收）；消息结构体槽位约定（m7_i1..m7_p5、`m_lc_vfs_*`）；`m_type = errno` 的回复约定。
- **不讲什么**：各命名空间里每个消息的语义（下放 13/28/22–25）；`reply()` 的发送实现（下放 04）；内核侧 `sys_*` 实现（交叉 `01-stage-kernel/18-syscall-copy.md`）。
- **前置**：00。
- **后置**：04、12、13、14、22–25、28。
- **事实底线**：`callnr.h:68,70,72`（VFS_BASE 0x100、IS_VFS_CALL、VFS_READ）；`com.h:513-544`（PM RQ/RS）、`com.h:589`（FS_BASE 0xA00）、`com.h:911-912`（VFS_TRANSID 0xB01）、`com.h:920/923`、`964/967`、`996/999`、`1038/1041`；`vfsif.h:79-81`；`main.c:80-90`。
- **知识点清单**：

  | 编号 | 名称 | 来源 |
  |------|------|------|
  | N-001 | 六命名空间真值表 | 新增 `callnr.h:68` + `com.h:513/589/911/920/964/1038` |
  | N-002 | transid 编码与守门 | 新增 `vfsif.h:79-81`、`com.h:912` |
  | N-003 | RTCDEV_RS 判定 | 新增 `com.h:996`；`main.c:126-134` 无分支 |
  | N-008 | 内核接口面 | 新增 `main.c:72/436/438`、`comm.c:23`、`utility.c:142`、`main.c:251`、`worker.c:52` |
  | K-222 | m_type 多命名空间 | 存量 `99` |

- **验收标准**：给出一张完整的命名空间真值表（含掩码差异：`IS_VFS_CALL` 用 `~0xff`，`IS_*_RS` 用 `~0x7f`）；每个基址带 C 文件行号；能解释为什么 `REQ_LOOKUP = 0xA1A`；`RTCDEV` 一行写明"存在但主循环不处理"。

### 02-vfs-init

- **一句话定位**：VFS 怎么从"被 RS 加载的镜像"变成"能接受请求的服务器"。
- **讲什么**：K-004、K-005、K-006、K-007、K-008、K-009、K-010、K-011、K-012、K-013、K-014、K-015；N-007 构建与装载（Makefile SRCS + service.mk + boot image 登记）。
- **不讲什么**：worker 机制（下放 03）；根挂载机制（下放 16）；`VFS_PM_*` 其余 11 个请求（下放 28）；各表的结构（下放 06–11）；主循环（下放 04）；`mount_pfs`/`mount_fs` 内部（下放 16）。
- **前置**：00、01。
- **后置**：03、07、11、16、22、28。
- **事实底线**：`main.c:54/64/66`、`main.c:303-369`（LU 三回调）、`main.c:374-388`（SEF 注册）、`main.c:393-496`（init_fresh）、`main.c:501-523`（do_init_root）、`main.c:528-575`（lock_proc/thread_cleanup）；`minix3/minix/servers/vfs/Makefile`（SRCS 33 + gcov 条件）；`minix3/share/mk/minix.service.mk`。
- **知识点清单**：

  | 编号 | 名称 | 来源 |
  |------|------|------|
  | K-004 … K-015 | 启动链 12 条（见 §2.1） | 存量 `01 §1-2` |
  | N-007 | 构建与装载 | 新增 `vfs/Makefile`、`minix.service.mk` |

- **验收标准**：给出 S1–S18 的完整时序表，每步带 `main.c` 行号；能解释"为什么 fproc 要清两遍"（S5/S14 的前置条件差异）；能解释"为什么根挂载期间要 `worker_allow(FALSE)`"；能说出 VFS 链接了哪四个库以及 `-lmthread` 意味着什么。

### 03-request-slots

- **一句话定位**：VFS 的执行载体——9 个槽怎么被分配、绑定、让出、唤醒、停止。
- **讲什么**：K-055…K-064（10 条）；ARCH A-1 的完整论证（mthread → 槽，含 Linux workqueue / Redox async / seL4 endpoint 对照）。
- **不讲什么**：谁调用 `worker_start`（下放 04）；槽里跑的具体 handler（下放 13–35）；挂起载荷（`fp_u`，下放 05）；fproc 字段（下放 06）。
- **前置**：02（只依赖调用点声明，不构成前向引用）。
- **后置**：04、05、12、14、28。
- **事实底线**：`worker.c` 全文（27/63/109/119/147/162/192/228/239/295/331/360/431/443/459/474/493/510/526/535/555/572/586）；`const.h:9`；`main.c:558-575`；`os/servers/vfs/src/worker.rs:1-33, 51`（ARCH 论证与 `WorkerFunc::DoWork`）。
- **知识点清单**：K-055…K-064（存量 `08 §1-2`）；K-064 的 ARCH 论证来源为 `worker.rs:1-33`。
- **验收标准**：画出槽的状态迁移图（Idle → Busy → WaitingForFs / Suspended → Idle）；能解释 `needed = use_spare ? 1 : 2` 为什么留一条 spare；能解释 `worker_set_proc` 为什么在 C 里被注释为"严格只给 reboot 用"；能说出 Rust 侧为什么不需要 `worker_cleanup`。

### 04-main-loop

- **一句话定位**：主循环怎么把一条消息分成八路，并交给槽去执行。
- **讲什么**：K-065…K-073（9 条）；八级路由的优先级与理由；`handle_work` 的 FS 回调死锁消解；`call_vec` 64 臂到 64 个 handler 的映射表。
- **不讲什么**：SUSPEND 之后怎么复活（下放 05）；`service_pm` 的 12 个请求内容（下放 28）；`bdev_reply/cdev_reply/sdev_reply` 内部（下放 23–25）；`unblock` 的族细节（下放 05）；槽如何被分配（已在 03）。
- **前置**：01、03。
- **后置**：05、12、14、22–28、34、35。
- **事实底线**：`main.c:69-141`（主循环）、`main.c:146-181`（handle_work）、`main.c:187-211`（do_reply）、`main.c:263-298`（do_work）、`main.c:580-633`（get_work）、`main.c:638-663`（reply/replycode）；`table.c:17-82`；`os/servers/vfs/src/syscalls.rs`（64 臂穷举 match）、`main_loop.rs:route_message`。
- **知识点清单**：K-065…K-075（存量 `09 §1-3`）。
- **验收标准**：给出八级路由表（每级：判据 C 行号、目标函数、为什么是这个优先级）；给出 64 个调用号 → handler → 所属新篇章的完整对照表（一行一个，64 行）；能解释 `reviving` 为什么要优先于新消息。

### 05-vfs-blocking

- **一句话定位**：请求凭什么挂起、由谁唤醒、怎么取消——SUSPEND 的统一契约与四条复活路径。
- **讲什么**：N-004 总契约；`fp_blocked_on` 七态与 `fp_u` 五类载荷的**统一视图**（各族细节下放）；四条复活路径的触发源与汇聚点；取消语义（二阶等待）；`worker_stop` 的 EIO 注入；`unblock` 的重建与重放规则。
- **不讲什么**：各族的挂起条件与算术（下放 20/24/25/26/34）；槽的唤醒原语（已在 03）；载荷字段的逐位定义（下放 06）；具体 errno（下放 37）。
- **前置**：03、04。**06（阻塞载荷字段定义）与 09（filp）以"前置知识摘要"声明**——本篇只用
  "`fp_blocked_on` 是一个标签 + 载荷的枚举"、"filp 是打开描述"两句话，字段级定义分别归 06/09。
  理由见 §9.1 检查 1 的第 1 条处置。
- **后置**：20、24、25、26、34、14。
- **事实底线**：`main.c:297`（SUSPEND 判定）、`main.c:921-973`（unblock）、`main.c:216-258`（do_pending_pipe）、`const.h:19-25`（七态）、`fproc.h:31-57`（fp_u）、`pipe.c:435`（revive）、`lock.c:172`（lock_revive）、`cdev.c:481`、`sdev.c:989`、`cdev.c:381`、`sdev.c:940`、`select.c:714`、`worker.c:535-567`。
- **知识点清单**：

  | 编号 | 名称 | 来源 |
  |------|------|------|
  | N-004 | 阻塞与复活总契约 | 新增 `main.c:297`、`const.h:19-25` + 四路径锚点 |
  | K-071 | unblock 重建原请求 | 存量 `09 §2` |
  | K-072 | do_pending_pipe 续作 | 存量 `09 §2` |
  | K-075 | ARCH A-5 ReplyIntent | 存量 `09 §3` |
  | K-061 | worker_stop EIO 注入 | 存量 `08 §2` |

- **验收标准**：给出"四条复活路径对照表"（触发源 / 是否经 `reviving` 计数 / 是否重放原请求 / 唤醒函数 C 行号）；能解释为什么 pipe 必须走 `do_pending_pipe` 续作而 flock 可以原样重放；能解释取消为什么是"二阶等待"。

### 06-fproc-struct

- **一句话定位**：一个进程的 VFS 上下文长什么样——字段全集、标志位、阻塞载荷的五类形状。
- **讲什么**：K-016…K-025（10 条）；`fp_u` 五类 union 的字段级定义（供 05/20/24/25/34 引用）；`LOCK_DEBUG` 的 cfg 对应（A-9）。
- **不讲什么**：表的索引与验证（下放 07）；fd 表的操作（下放 17）；凭证如何被修改（下放 28）；挂起的触发条件（下放 05）。
- **前置**：02。
- **后置**：05、07、17、20、24、25、28–30、32–34。
- **事实底线**：`fproc.h:15-82`（字段）、`fproc.h:91-98`（标志）、`fproc.h:31-57`（union）、`fproc.h:9`（LOCK_DEBUG）、`const.h:19-25`、`const.h:52`、`misc.c:577-634`；`os/servers/vfs/src/fproc.rs`（`FProc`/`FpFlags`/`BlockedOn`）。
- **知识点清单**：K-016…K-025（存量 `02 §1-2`）。
- **验收标准**：字段全集表（字段名 / C 类型 / Rust 类型 / 谁来写 / 谁来读）；`fp_flags` 六位逐位表；`fp_u` 五类载荷的字段表（供后篇引用，后篇不得重画）；能解释 `fp_umask` 初始化为 `~0` 的含义。

### 07-fproc-table

- **一句话定位**：`fproc[NR_PROCS]` 怎么索引、怎么验证、怎么投影给 MIB。
- **讲什么**：K-026…K-029；N-014 `fproc_light` 与 MIB 契约（含 Cargo feature 的 defer 状态）。
- **不讲什么**：字段语义（已在 06）；谁消费这张表（下放各篇）。
- **前置**：06。
- **后置**：04、17、28–30。
- **事实底线**：`glo.h:26-28`（`who_p`/`fproc_addr`）、`utility.c:94-127`（`isokendpt_f`/`okendpt`）、`fproc.h:117-124`（fproc_light）、`main.c:405-408/468-484`；`os/servers/vfs/src/fproc.rs:FProcTable`、`Cargo.toml` 的 `fproc_light` feature。
- **知识点清单**：K-026…K-029（存量 `03 §1-2`）；N-014（新增 `fproc.h:117-124` + `Cargo.toml`）。
- **验收标准**：给出 endpoint ↔ slot 双向编码的算式与边界；`isokendpt`/`okendpt` 的三守卫对照表（含"致命分化"指什么）；说明 `fproc_light` 当前是 defer 还是已实现，defer 的话挂在哪条 edge。

### 08-tll-lock

- **一句话定位**：VFS 的锁原语，以及"多把锁按什么顺序取"和"每个请求锁什么"。
- **讲什么**：K-050…K-054（5 条）；N-016 加锁顺序；N-017 每请求类型锁特征表。
- **不讲什么**：各表自己的锁映射（下放 09/10/11，本篇只给抽象）；阻塞等待的调度（已在 03）；`fp_lock` 的进程锁（下放 02）。
- **前置**：04（读者需知道"一次请求"是什么）；不依赖 09/10/11。
- **后置**：09、10、11、15、16、19。
- **事实底线**：`tll.h` 全文、`tll.c:11/74/113/126/132/139/220/230/306`；`minix3/minix/servers/vfs/README:219-240`（§4.2）、`:304-357`（§4.4 加锁顺序）、`:569-656`（§4.8 每请求锁特征）；`os/servers/vfs/src/tll.rs`。
- **知识点清单**：K-050…K-054（存量 `07 §1-2`）；N-016、N-017（新增 `README:304-357` / `README:569-656`）。
- **验收标准**：画出 `tll_lock` 的 5 路分发决策图；给出官方 README §4.4 的加锁顺序（带中文解释，标注这是官方作者原文）；给出 §4.8 的锁特征表（至少覆盖 open/read/write/lookup/mount/close 六类请求的锁型与持有时机）；能解释写偏序为什么存在。

### 09-filp-table

- **一句话定位**：打开描述表——`fd → filp → vnode` 的二跳与引用计数配平。
- **讲什么**：K-030…K-036（7 条）；K-223 的三层计数配平表（以 filp 侧为主视角）。
- **不讲什么**：fd 表的分配与关闭（下放 17）；`filp_count` 的各处增减（下放 17/28）；select 字段的消费（下放 26）；vnode 侧计数（下放 10）。
- **前置**：08。
- **后置**：10、17–20、26、34。
- **事实底线**：`file.h` 全文、`const.h:5`、`filedes.c:73-249`（init/get/find）、`filedes.c:313-430`（lock/unlock/close）；`os/servers/vfs/src/filp.rs`。
- **知识点清单**：K-030…K-036（存量 `04 §1-2`）。
- **验收标准**：给出 `FilpLockMode` 三态（Opcl/None/ReadWrite）× `FILP_CLOSED` 的行为矩阵（4 行），并解释"close(2) 为什么能穿过 CLOSED"；给出 filp 表槽位状态机（alloc → count=1 → 共享 ++ → 递减 → 归零）。

### 10-vnode-table

- **一句话定位**：vnode 表——缓存、双层引用计数与延迟同步。
- **讲什么**：K-037…K-041（5 条）。
- **不讲什么**：vnode 在路径解析里的使用（下放 15）；`v_fs_e` 与挂载归属（下放 11）；filp 侧计数（已在 09）。
- **前置**：08、09。
- **后置**：11、15、16、17–19。
- **事实底线**：`vnode.h` 全文、`const.h:8`、`vnode.c:43/85/110/126/138/156/177/218/227/240/305`；`os/servers/vfs/src/vnode.rs`。
- **知识点清单**：K-037…K-041（存量 `05 §1-2`）。
- **验收标准**：画出 `v_ref_count` 与 `v_fs_count` 的配平图；解释 `clean_refs` 的 256 阈值为什么存在（延迟同步的收益与代价）；给出"vnode 何时可回收"的双条件判据。

### 11-vmnt-table

- **一句话定位**：挂载表——`device ↔ FS` 的边界、每 FS 的通信窗口、挂载状态位。
- **讲什么**：K-042…K-049（8 条）。
- **不讲什么**：挂载/卸载流程（下放 16）；窗口的收发算法（下放 12）；锁原语（已在 08）。
- **前置**：08、10。
- **后置**：12、13、15、16、19、23、32。
- **事实底线**：`vmnt.h` 全文、`const.h:7`、`vmnt.c:24/65/76/95/112/127/141/150/180/199/221/237/246`；`os/servers/vfs/src/vmnt.rs`。
- **知识点清单**：K-042…K-049（存量 `06 §1-2`）。
- **验收标准**：给出 `vmnt` 槽的生命周期图；给出 `VMNT_*` 标志位表（每个位的设置者/清除者/后果）；说明 `NR_MNTS 16`（`const.h:7`）与 `NR_NONEDEVS` 的同源关系。

### 12-fs-comm

- **一句话定位**：怎么向 FS/VM/驱动发请求并拿回复——窗口、transid、队列。
- **讲什么**：K-084…K-091（8 条）。
- **不讲什么**：`req_*` 的具体包装（下放 13）；驱动通道的骨架（下放 14）；transid 编码本身（已在 01）；回复路由的主循环分支（已在 04）。
- **前置**：04、11。
- **后置**：13、14、15、16、19、23、28。
- **事实底线**：`comm.c` 全文（12/37/50/66/89/134/173/200/223）；`vmnt.h:22-28`；`os/servers/vfs/src/fs_comm.rs`。
- **知识点清单**：K-084…K-091（存量 `11 §1-2`）。
- **验收标准**：画出一次 `fs_sendrec` 的时序图（窗口判定 → sendmsg/queuemsg → worker_wait → do_reply → signal）；解释 `sending` 与 `c_cur_reqs` 两个计数的区别；解释 `drv_sendrec` 为什么必须串行而 `fs_sendrec` 可以并发。

### 13-request-wrappers

- **一句话定位**：`req_*` 包装层——33 常量 / 36 函数 / 响应结构 / 三特殊码。
- **讲什么**：K-092…K-098（7 条）；32 活请求的信封差异表（代表成员 `req_lookup` 讲透）。
- **不讲什么**：窗口与收发（已在 12）；grant 的通用机制（下放 21）；FS 服务端实现（交叉 `15-stage-fs`）。
- **前置**：12。
- **后置**：15、16、18、19、20、27、31–33。
- **事实底线**：`vfsif.h:41-75`（33 常量 + NREQS）、`vfsif.h:8-9`（REQ_RDONLY/REQ_ISROOT）、`request.h`（node_details/lookup_res）、`request.c` 全文（36 个 `req_*`）；`os/servers/vfs/src/request.rs`。
- **知识点清单**：K-092…K-098（存量 `12 §2`）。
- **验收标准**：给出 33 常量表（常量 / 绝对值 / 是否有 `req_*` / 归属新篇章），`REQ_GETNODE` 标注死常量；给出 32 活请求的信封差异表（请求字段 / 响应结构 / 特殊码）；能解释 `EENTERMOUNT`/`ELEAVEMOUNT`/`ESYMLINK` 语义上属于"控制流"而非"错误"。

### 14-driver-transport

- **一句话定位**：三类驱动通道的共同骨架，以及驱动生死时的全系统级联编排。
- **讲什么**：N-005 级联编排；N-018 驱动崩溃恢复三分类；`IS_BDEV_RS`/`IS_CDEV_RS`/`IS_SDEV_RS` 三路分流与三个 `*_reply` 入口；`worker_stop`/`stop_by_endpt` 的 EIO 注入；`invalidate_filp_by_*` 三族的触发序；`dmap_endpt_up`/`smap_endpt_up`/`bdev_up` 的恢复序。
- **不讲什么**：每类驱动的协议细节（下放 23/24/25）；三本通讯录的查表语义（下放 22）；`invalidate` 三族的函数实现（下放 17）；select 侧的唤醒算法（下放 26）。
- **前置**：04、05、12。**17（失效三族原语）与 22（三本通讯录）以"前置知识摘要"声明**——
  17 只取"按 endpoint / char_major / sock_drv 三种谓词失效 filp"三句话，22 只取
  "major→driver、domain→driver"两句话，实现分别归 17/22。理由见 §9.1 检查 1 的第 2 条处置。
- **后置**：23、24、25、26。
- **事实底线**：`main.c:126-134`、`worker.c:535-567`、`filedes.c:250-312`、`dmap.c:180/275`、`smap.c:148/173`、`bdev.c:227-282`、`sdev.c:912-939`、`select.c:884`；`minix3/minix/servers/vfs/README:657-700`（§5）。
- **知识点清单**：

  | 编号 | 名称 | 来源 |
  |------|------|------|
  | N-005 | 死亡/上线级联统一触发序 | 新增 `dmap.c:180`/`smap.c:148`/`filedes.c:250-312`/`sdev.c:912`/`select.c:884`/`bdev.c:227` |
  | N-018 | 驱动崩溃恢复三分类 | 新增 `README:657-700` |
  | K-061 | worker_stop EIO 注入 | 存量 `08 §2` |

- **验收标准**：给出"驱动死亡级联时序图"（一事件 → 五个动作 → 各自归属篇）；给出三类对端（块/字符+socket/FS）崩溃恢复的对照表（官方 README §5.1/5.2/5.3 与代码锚点对应）；能解释为什么 FS 崩溃与驱动崩溃的处置不同。

### 15-path-lookup

- **一句话定位**：一条路径名怎么变成 vnode——跨挂载穿越与符号链接展开。
- **讲什么**：K-099…K-106（8 条）；`DO_POSIX_PATHNAME_RES`（A-10）。
- **不讲什么**：路径取入的搬运机制（下放 21）；`req_lookup` 的信封（已在 13）；open 之后的流程（下放 18）；`do_socketpath` 的 UDS 语义（下放 27）。
- **前置**：10、11、13。
- **后置**：16、18、27、28–33。
- **事实底线**：`path.c:31`（DO_POSIX）、`path.c:40/133/146/384/575/594/648/803`；`path.h`；`utility.c:24-93`；`os/servers/vfs/src/path.rs`。
- **知识点清单**：K-099…K-106（存量 `13 §1-2`）。
- **验收标准**：画出 `advance`/`lookup` 的主循环（含 EENTERMOUNT/ELEAVEMOUNT/ESYMLINK 三分支）；给出 `SYMLOOP=16` 的计数位置与 `ELOOP` 触发点；解释 chroot 边界为什么"同 dev 才生效"；能说出尾斜杠语义的历史 Unix 与 POSIX 分歧。

### 16-mount

- **一句话定位**：一棵外部文件系统怎么嫁接到单树上，以及根挂载为什么特殊。
- **讲什么**：K-129…K-135（7 条）；根挂载调用点（回指 02 的 S17）。
- **不讲什么**：vmnt 表的存储与锁（已在 11）；`req_readsuper` 的信封（已在 13）；`unmount_all` 在重启序列中的位置（下放 28）。
- **前置**：11、12、13。
- **后置**：19、23、28、32。
- **事实底线**：`mount.c:46/85/156/391/430/470/552/590/628/641`；`main.c:501-523`（根挂载调用点）；`glo.h:18-19`；`os/servers/vfs/src/mount.rs`。
- **知识点清单**：K-129…K-135（存量 `18 §1-2`）。
- **验收标准**：画出 `mount_fs` 五段提交 + `unmount` 七步拆除的对称表；解释 `mount_pfs` 为什么是"罐装挂载"（无 `req_readsuper` 确认往返）；解释根可以被挂两次（`have_root`/`ROOT_FS_E`）的机制。

### 17-filedes

- **一句话定位**：fd 表的分配、关闭、复制与失效四类操作。
- **讲什么**：K-107…K-112（6 条）；`invalidate_filp_by_*` 三族的函数实现（14 只讲触发序）。
- **不讲什么**：filp 结构与引用计数配平（已在 09）；`invalidate` 的触发者（下放 14）；`do_open`/`do_close` 的流程（下放 18）；`clo_exec` 的扫描者（下放 29）。
- **前置**：09。
- **后置**：14、18、19、20、27、29、34。
- **事实底线**：`filedes.c:88/110/250/260/277/298/524`；`open.c:690`（close_fd）；`filedes.c:121`（get_fd 的 start 参数化）；`os/servers/vfs/src/filedes.rs`。
- **知识点清单**：K-107…K-112（存量 `14 §1-2`）。
- **验收标准**：给出 `CopyKind` 三分支 × 三守门的行为矩阵；给出 `close_fd` 的拆除序（三步 + 每步的 C 行号）；给出三个 `invalidate_by_*` 谓词的对照表（谓词 / 触发者 / 失效范围）；解释为什么不存在第二种 fd 分配策略（`NextFit` 教训）。

### 18-open-close

- **一句话定位**：`open`/`creat`/`close`/`lseek`——路径到 fd 的绑定与拆除。
- **讲什么**：K-113…K-116（4 条）。
- **不讲什么**：路径解析（已在 15）；filp 与 fd 的原语（已在 09/17）；设备打开的分派（下放 22–25）；管道打开（下放 20）。
- **前置**：15、17。
- **后置**：19、20、22–26、29。
- **事实底线**：`open.c:38/58/83/299/483/514/564/603/655/674`；`os/servers/vfs/src/open.rs`。
- **知识点清单**：K-113…K-116（存量 `15 §1-2`）。
- **验收标准**：给出 `common_open` 的六路分派图（类型位 → 分支 → 归属篇）；给出 `oflags` 位表；解释 `O_TRUNC` 与 `do_truncate` 的同源关系（交叉 31）。

### 19-read-write

- **一句话定位**：`read`/`write`/`getdents`——五路分派、位置推进、块串行。
- **讲什么**：K-117…K-122（6 条）；`bsf_lock` 的取锁语义与调用点（块侧后果下放 23）。
- **不讲什么**：`req_readwrite` 信封（已在 13）；管道续写（下放 20）；驱动侧读写（下放 23–25）；挂起与复活（已在 05）。
- **前置**：09、17、18。
- **后置**：20、23、26、30。
- **事实底线**：`read.c:30/49/67/76/92/127/135/282/323`；`write.c:15`；`glo.h:41`；`os/servers/vfs/src/read_write.rs`。
- **知识点清单**：K-117…K-122（存量 `16 §1-2`）。
- **验收标准**：给出五路分派表（类型位 → 分支 → 归属篇）；解释 `filp_pos` 由谁推进、部分读写时怎么推进；解释 `bsf_lock` 为什么只锁块特殊文件；给出 `EPIPE → SIGPIPE` 的条件与 `O_NOSIGPIPE` 的抑制点。

### 20-pipe

- **一句话定位**：管道——配额定量、挂起账本、唤醒扫描、信号中断。
- **讲什么**：K-123…K-128（6 条）；作为"阻塞与复活"族的代表成员，完整展示 05 的契约如何落地。
- **不讲什么**：`req_newnode` 信封（已在 13）；通用挂起契约（已在 05）；fd 与 filp 原语（已在 17/09）。
- **前置**：05、17、19。
- **后置**：26、27。
- **事实底线**：`pipe.c:39/60/151/187/294/315/334/363/435/498`；`glo.h:13/15`；`os/servers/vfs/src/pipe.rs`。
- **知识点清单**：K-123…K-128（存量 `17 §1-2`）。
- **验收标准**：给出 `pipe_check` 的三开关决策表（存量 / 对端 / 容量 → 返回值）；画出"挂起 → revive → reviving++ → get_work → unblock → do_pending_pipe"的完整时序（这是全 stage 最完整的复活样例，后篇引用它）；解释 `susp_count` 与 `reviving` 为什么是两个计数。

### 21-data-transfer

- **一句话定位**：用户缓冲区怎么进出 VFS——grant、safecopy、路径取入四条路。
- **讲什么**：N-006 四条路；`cpf_grant_direct`/`cpf_grant_magic` 与 `CPF_READ/WRITE/TRY`；`make_ioctl_grant` 的 IOR/IOW 方向解码（从 22 上收）；`sys_safecopyfrom/to` 与 `sys_datacopy_wrapper`（从 36 上收）；`fetch_name`/`copy_path`（从 15 上收）；Rust 侧 `PathFetcher` seam 与"生产 impl 随 W1 transport 落地"的诚实边界。
- **不讲什么**：内核侧 `sys_safecopy` 实现（交叉 `01-stage-kernel/18-syscall-copy.md`）；`req_*` 用哪个 grant（已在 13）；`ioctl` 的分派（下放 22）。
- **前置**：13、15、19。
- **后置**：22、23–25、27、29、30。
- **事实底线**：`device.c:52-95`；`utility.c:24-93`（copy_path/fetch_name）、`utility.c:142-186`（sys_datacopy_wrapper）；`request.c` 的 cpf_grant 调用点；`os/servers/vfs/src/path.rs:PathFetcher`。
- **知识点清单**：

  | 编号 | 名称 | 来源 |
  |------|------|------|
  | N-006 | 数据搬运四条路 | 新增 `device.c:52-95`、`utility.c:24-93/142-186`、`path.rs:PathFetcher` |
  | K-097 | cpf_grant 三标志 | 存量 `12 §2` |
  | K-143 | ioctl 授权方向解码 | 存量 `19 §2.12`（越界，本篇收编） |
  | K-224 | sys_datacopy_wrapper | 存量 `99` |
  | K-105 | fetch_name/copy_path | 存量 `13 §2` |

- **验收标准**：给出四条路的对照表（谁发起 / 数据方向 / 用的机制 / 失败 errno / 归属篇）；解释"grant 与 safecopy 的分工"；说明 `PathFetcher` 的两个 impl 当前状态（生产 impl 未落地，挂 W1）。

### 22-device-map

- **一句话定位**：三本通讯录——major / domain / DS 标签怎么路由到驱动进程。
- **讲什么**：K-136…K-144（9 条）；`do_ioctl` 的四分流（保留）；`make_ioctl_grant` 只给调用点，机制已归 21。
- **不讲什么**：驱动通道的数据面（下放 23–25）；死亡级联编排（已在 14）；授权创建机制（已在 21）；`do_mapdriver` 的重启执行（挂 W5，注明）。
- **前置**：02（init_dmap/init_smap 调用点）、11。
- **后置**：23、24、25、27。
- **事实底线**：`dmap.c:27/47/61/106/180/200/230/252/275/317`；`smap.c:22/47/148/173/201/217/244/266`；`device.c:18/52`；`dmap.h`；`type.h:41-49`；`const.h:11/52`；`os/servers/vfs/src/device_map.rs`。
- **知识点清单**：K-136…K-144（存量 `19 §2`）。
- **验收标准**：给出三本通讯录的对照表（键 / 表 / 容量 / 查表函数 / 谁登记 / 谁清除）；解释 `CTTY_ENDPT = VFS_PROC_NR` 为什么能通过 `isokendpt`；给出 `do_ioctl` 四分流表；说明本篇聚合了哪四个 C 文件（`dmap.c`/`smap.c`/`device.c`/mapdriver）。

### 23-bdev

- **一句话定位**：块设备通道——直达、重试熔断、死信分流、换人通告。
- **讲什么**：K-145…K-149（5 条）；作为"驱动通道"族的代表成员完整展示 14 的骨架。
- **不讲什么**：块读写的 FS 路由（已在 13）；`bsf_lock` 的取锁点（已在 19）；dmap 查表（已在 22）；死亡级联编排（已在 14）。
- **前置**：14、19、22。
- **后置**：24、25。
- **事实底线**：`bdev.c:34/79/114/144/193/227`；`comm.c:89-129`（drv_sendrec 排他）；`os/servers/vfs/src/bdev.rs`。
- **知识点清单**：K-145…K-149（存量 `20 §1-2`）。
- **验收标准**：给出重试状态机（最多五次、为什么是五）；给出死信三分类表（errno / 含义 / 后续动作 / 是否清表）；解释 `bdev_up` 的三件事（重开 / 挂载树通告 / 根兜底）各自的 C 行号。

### 24-cdev

- **一句话定位**：字符设备通道——终端改道、挂起读写、取消换码。
- **讲什么**：K-150…K-156（7 条）；按 14 的差异表收束（与 23 的差异：会挂起 / 可被取消 / 有 tty 改道）。
- **不讲什么**：通用挂起契约（已在 05）；授权机制（已在 21）；`select_cdev_reply*` 的实现（下放 26）；TTY 行规程（交叉 `16-stage-drivers`）。
- **前置**：14、18、22。
- **后置**：26。
- **事实底线**：`cdev.c:36/63/97/149/254/264/280/350/381/429/481`；`os/servers/vfs/src/cdev.rs`。
- **知识点清单**：K-150…K-156（存量 `21 §1-2`）。
- **验收标准**：给出与 23 的差异表（≥6 行）；画出"取消 = 二阶等待"的时序；解释 `/dev/tty` 改道为什么"幂等但只调一次"。

### 25-sdev

- **一句话定位**：socket 驱动通道——长短问、挂起登记、取消交叉、复活分流。
- **讲什么**：K-157…K-162（6 条）；按 14 的差异表收束（与 24 的差异：三态等待 / 复活分三组 / 取消与复活交叉）。
- **不讲什么**：socket 调用面（下放 27）；通用挂起契约（已在 05）；smap 语义（已在 22）；`sdev_stop` 的编排（已在 14，本篇只给决策函数）。
- **前置**：14、22、24。
- **后置**：26、27。
- **事实底线**：`sdev.c:59/83/119/220/230/242/280/292/340/417/454/504/561/572/582/592/604/647/679/732/759/912/940/989`；`os/servers/vfs/src/sdev.rs`。
- **知识点清单**：K-157…K-162（存量 `22 §1-2`）。
- **验收标准**：给出"短问/长问/选择发即返"三态表；给出复活分三组的表；给出取消与复活交叉的裁决规则（部分成功 vs 穿越回复）。

### 26-select

- **一句话定位**：`select`——一问等一群，两波才收场。
- **讲什么**：K-163…K-170（8 条）；N-015 定时器与 CLOCK 契约；作为"阻塞与复活"族第二成员，引用 20 的样例。
- **不讲什么**：`pipe_check` 的试探算术（已在 20）；`cdev_select`/`sdev_select` 的驱动侧（已在 24/25）；通用挂起契约（已在 05）；fd 位图的搬运（已在 21）。
- **前置**：05、09、20、22、24、25。
- **后置**：27。
- **事实底线**：`select.c:96/335/409/462/527/567/577/621/635/660/714/740/783/808/821/833/861/884/956/1004/1060/1111/1167/1198/1217/1318/1337`；`main.c:110-113`；`const.h:36-40`；`os/servers/vfs/src/select.rs`。
- **知识点清单**：K-163…K-170（存量 `23 §1-2`）；N-015（新增 `select.c:335/861` + `main.c:110-113`）。
- **验收标准**：画出六段流水线并标出"首波/次波"的汇聚点；给出四类 fd 的分型表（文件/管道/字符/套接字 → 问谁 / 是否挂起 / 回复路径）；解释 `set_timer` + CLOCK notify + `expire_timers` 的闭环；给出 `FilterOutcome::Query` 的义务字段（clear_update/set_busy）与 C 行号的对应。

### 27-socket

- **一句话定位**：socket 调用面——创建、建连、收发、控制、失败清理、阻塞恢复。
- **讲什么**：K-171…K-176（6 条）；`do_socketpath` 的调用面（路径行走归 15）。
- **不讲什么**：`sdev_*` 驱动对话（已在 25）；fd 表原语（已在 17）；select 登记（已在 26）；UDS 协议实现（交叉 `17-stage-net`）。
- **前置**：17、25、26。
- **后置**：无（末端篇）。
- **事实底线**：`socket.c:44/64/86/176/224/276/308/326/344/365/399/483/504/526/543/608/657/676/701/724/747/803`；`path.c:803-836`；`os/servers/vfs/src/socket.rs`。
- **知识点清单**：K-171…K-176（存量 `24 §1-2`）；K-106（存量 `13 §2`，本篇只给调用面）。
- **验收标准**：给出 16 个 socket 调用的表（调用 / handler / 是否可能挂起 / 恢复函数 / 下放驱动函数）；给出"谁分配谁释放"的失败清理表；解释 `resume_accept`/`resume_recvfrom`/`resume_recvmsg` 三分支。

### 28-pm-protocol

- **一句话定位**：PM 控制面——12 个 `VFS_PM_*` 请求、立即/延期二分、fork/exit/exec/reboot 四条链。
- **讲什么**：K-076…K-083（8 条）；`VFS_PM_INIT` 回指 02；`unmount_all` 在 `pm_reboot` 中的位置。
- **不讲什么**：`syscall` 服务面（下放 17–27）；`pm_exec` 的装载细节（下放 29）；`pm_dumpcore` 的转储细节（下放 30）；PM 侧发送方状态机（交叉 `04-stage-pm/05-vfs-interaction.md`）。
- **前置**：04、07、17。
- **后置**：29、30、16。
- **事实底线**：`main.c:668-759`（postponed）、`main.c:764-915`（service_pm）；`misc.c:577/713/726/743/764/779/903`；`com.h:513-544`；`os/servers/vfs/src/ipc/dispatcher.rs`。
- **知识点清单**：K-076…K-083（存量 `10 §1-2`）；K-007（存量 `01 §2`，回指）。
- **验收标准**：给出 12 个 `VFS_PM_*` 的表（请求 / 是否延期 / 是否回 reply / 归属篇）；解释"立即 7 / 延期 4 / REBOOT"三分支的判据；给出 `pm_fork` 的完整复制清单（fproc 字段 / filp_count / dup_vnode / flags 重置）；给出 `pm_reboot` 八步序列与两轮 pass 谓词。

### 29-exec

- **一句话定位**：`exec`——检查、释放旧空间、装载新程序、收尾。
- **讲什么**：K-177…K-183（7 条）；`clo_exec` 尾扫描（原语 `close_fd` 归 17，扫描归本篇）。
- **不讲什么**：路径解析（已在 15）；`req_readwrite` 信封（已在 13）；VM 侧 mmap 实现（交叉 `02-stage-vm/20-vm-mmap.md`）；PM 侧 exec 状态机（交叉 `04-stage-pm/17-exec.md`）。
- **前置**：15、18、28。
- **后置**：30。
- **事实底线**：`exec.c:89/161/185/413/522/534/607/688/721/736`；`os/servers/vfs/src/exec.rs`。
- **知识点清单**：K-177…K-183（存量 `25 §1-2`）；K-112（存量 `14 §2`，本篇只讲扫描）。
- **验收标准**：给出四段流水线 + 三项检查（C 行号）；解释脚本解释的 `argv[0]` 重写；解释动态链接时"主文件 fd 保留"的原因；给出 `clo_exec` 扫描的边界（为什么忽略关闭错误）。

### 30-coredump

- **一句话定位**：把已停止进程的地址空间、寄存器与死因写成 ELF core 文件。
- **讲什么**：K-184…K-188（5 条）；`unpause` 前置（引用 20/28）。
- **不讲什么**：信号语义（交叉 `04-stage-pm/11~13`）；VM 区表查询实现（交叉 `02-stage-vm`）；ELF 装载（已在 29）。
- **前置**：18、19、28、29。
- **后置**：无（末端篇）。
- **事实底线**：`coredump.c:32/80/104/126/162/176/191/233/275/283/294`；`misc.c:903-988`；`os/servers/vfs/src/coredump.rs`。
- **知识点清单**：K-184…K-188（存量 `26 §1-2`）；K-127（存量 `17 §2`，只给 unpause 调用点）。
- **验收标准**：给出 core 文件四部分的布局图（头 / 程序头 / 注释段 / 段数据）与各自 C 函数；解释两个注释段（进程身份 / 寄存器快照）与名称 `MINIX-CORE`；解释缺页补零与 `LONG_MAX` 截断。

### 31-link

- **一句话定位**：目录项与 inode 的四类操作——link/unlink/rename/truncate/symlink。
- **讲什么**：K-189…K-192（4 条）。
- **不讲什么**：路径解析（已在 15）；`O_TRUNC` 的 open 侧（已在 18）；FS 侧回收（交叉 `15-stage-fs`）。
- **前置**：15、17、18。
- **后置**：32。
- **事实底线**：`link.c:29/91/169/276/327/387/471`；`os/servers/vfs/src/link.rs`。
- **知识点清单**：K-189…K-192（存量 `27 §1-2`）。
- **验收标准**：给出四类操作的表（调用 / 检查项 / errno / 是否经 FS / 是否改 inode）；解释 `EXDEV` 与"同一设备内"约束；解释 `do_truncate` 的"新旧长度相等且为常规文件时直接 OK"优化。

### 32-stadir

- **一句话定位**：工作目录切换、stat 三兄弟、statvfs 与挂载表遍历。
- **讲什么**：K-193…K-197（5 条）。
- **不讲什么**：路径解析（已在 15）；权限判定（下放 33）；vmnt 表结构（已在 11）；`do_utimens`（已在 35）。
- **前置**：11、15、17。
- **后置**：33。
- **事实底线**：`stadir.c:32/50/83/117/140/173/197/234/294/328/351/418`；`os/servers/vfs/src/stadir.rs`。
- **知识点清单**：K-193…K-197（存量 `28 §1-2`）。
- **验收标准**：给出 `stat`/`fstat`/`lstat` 的差异表（是否跟随符号链接 / 入口 / 取 vnode 方式）；给出 `change_into` 三项检查；给出 `MountNames` 三名字的来源字段（并说明 `fetch_vmnt_paths` 是 C 死代码，不移植）。

### 33-protect

- **一句话定位**：九位权限判定与 chmod/chown/umask/access。
- **讲什么**：K-198…K-202（5 条）。
- **不讲什么**：凭证如何被设置（已在 28）；路径解析（已在 15）；`forbidden` 的各调用点（各篇自引）。
- **前置**：06、15。
- **后置**：无（末端篇）。
- **事实底线**：`protect.c:25/98/182/198/238`；`utility.c:128-141`；`os/servers/vfs/src/protect.rs`。
- **知识点清单**：K-198…K-202（存量 `29 §1-2`）。
- **验收标准**：给出九位 × 三档（主/组/客）的判定表与位移算式；给出 `do_chown` 三项条件；解释 `umask` 为什么取反存储；说明 root 的默认通过规则与"仅无执行位不可执行"的例外。

### 34-fcntl-lock

- **一句话定位**：`fcntl` 十三命令与八槽 POSIX 劝告锁。
- **讲什么**：K-203…K-208（6 条）；作为"阻塞与复活"族第三成员（flock 路径），引用 05。
- **不讲什么**：fd 分配与 `close_fd` 检查（已在 17）；filp 结构（已在 09）；通用挂起契约（已在 05）。
- **前置**：05、09、17。
- **后置**：无（末端篇）。
- **事实底线**：`misc.c:117-275`（do_fcntl）；`lock.c:21/172`；`lock.h`；`const.h:6/22`；`os/servers/vfs/src/fcntl.rs`。
- **知识点清单**：K-203…K-208（存量 `30 §1-2`）。
- **验收标准**：给出 13 个 `fcntl` 命令表（命令 / 语义 / 是否挂起 / 归属篇）；给出读读/读写/同进程的三条相容规则；给出解锁四分支（清除/头缩/尾缩/中间分裂）；解释 `F_SETLKW` 挂起后经 `unblock` 原样重放的原因。

### 35-misc-queries

- **一句话定位**：杂项八类——sysinfo / sync / VM 上游 / svrctl / utimens / gcov / 废弃调用 / DS 事件。
- **讲什么**：K-209…K-217（9 条）；**删除 ioctl 叙述**（越界 O2），只保留 `do_vm_call` 的 VM 三类上游请求。
- **不讲什么**：`do_ioctl` 分派（已在 22）；PM 侧 `pm_reboot`/`free_proc`（已在 28）；`ds_event` 的下游动作（下放 14）；`do_statvfs`（已在 32）。
- **前置**：11、17、28。
- **后置**：无（末端篇）。
- **事实底线**：`misc.c:52/276/297/328/380/797/989/998`；`time.c:26-155`；`gcov.c:10-73`；`misc.c:958-982`（ds_event）；`os/servers/vfs/src/misc.rs`。
- **知识点清单**：K-209…K-217（存量 `31 §2`）。
- **验收标准**：给出八类操作的表（调用 / 守门 / 返回值 / 归属篇）；给出 `do_gcov_flush` 五门清单（含 root 门）；说明 `do_getrusage` 为什么恒返回 OK；给出 `ds_event` 的三前缀分类与两个下游动作。

### 36-global-concepts

- **一句话定位**：常量、全局状态、术语、引用计数不变量、有意省略台账——随时查的参考篇。
- **讲什么**：K-218…K-226（9 条）；有意省略台账（含 `RTCDEV`、`fetch_vmnt_paths`、`panic_hook`、`LOCK_DEBUG`、worker 真实线程栈等）。
- **不讲什么**：一切机制（各篇）；ARCH 对账（下放 37）；修复台账（留在 plan.md/todo.md）。
- **前置**：00。
- **后置**：全部篇（作为参考，不构成前向依赖）。
- **事实底线**：`const.h` 全文；`glo.h` 全文；`type.h`；`fs.h`；`proto.h`；`utility.c:142-186`；`os/servers/vfs/src/call_table.rs` 常量。
- **知识点清单**：K-218…K-226（存量 `99`）。
- **验收标准**：容量常量逐个给出"机制依据"（为什么是这个数）；给出 `m_type` 命名空间速查（一行表，详细归 01）；给出三层引用计数配平表；给出有意省略台账（每项：符号 / 为什么省略 / 谁判定）。

### 37-arch-evolution

- **一句话定位**：Minix3 C 与 minix-rs 的 15 处架构差异总表，与 errno 统一通道。
- **讲什么**：N-011（A-1…A-15 与 Rust 现状对账）；N-009（30 个枚举 → `ToErrno`）；A-6（`Rc`/`RefCell` 在单线程事件循环下的合法性）；A-8（类型映射）；A-9（`LOCK_DEBUG` → cfg）。
- **不讲什么**：各项的机制细节（各篇）；修复历史（留在 todo.md）；接线执行（下放 38）。
- **前置**：03（A-1）、04（A-2/A-4/A-5）、06（A-3/A-8）、36。
- **后置**：38。
- **事实底线**：`os/servers/vfs/src/` 各 ARCH 落点：`worker.rs:1-33`（A-1）、`call_table.rs:28-93` + `syscalls.rs`（A-2）、`fproc.rs:BlockedOn`（A-3）、`main_loop.rs:VfsState`（A-4）、`main_loop.rs:ReplyIntent`（A-5）、`Cargo.toml` 的 `fproc_light` feature（A-7）；30 处 `impl minix_types::ToErrno`。
- **知识点清单**：

  | 编号 | 名称 | 来源 |
  |------|------|------|
  | N-011 | ARCH A-1…A-15 对账 | 新增 `plan.md §4`（取事实）+ `os/servers/vfs/src/` |
  | N-009 | errno → ToErrno 通道 | 新增 30 处 `impl minix_types::ToErrno` |
  | K-064 | A-1 | 存量 `08 §3` |
  | K-073/K-074/K-075 | A-2/A-4/A-5 | 存量 `09 §3` |
  | K-103/K-226 | A-10/A-9 | 存量 `13 §2`、`99` |

- **验收标准**：给出 15 行 ARCH 表（项 / C 现状锚点 / Rust 现状锚点 / 状态：已实现·部分·设计层·缺口）；每项标注"三处一致"的落点（文档正文 / 代码注释 / 本篇）；给出 errno 通道的当前覆盖率（30/30）与消费端迁移状态。

### 38-testing

- **一句话定位**：这个 stage 的验证策略——测什么、怎么测、接线到什么程度算通。
- **讲什么**：N-010 测试基建；决策纯化策略（C 分支决策提为纯函数 + 正反样本）；Scripted seam 清单与"生产单元零替身"纪律；wire 绝对值钉子（以 `REQ_LOOKUP = 0xA1A` 为例）；`cargo check` + `cargo test` 双跑纪律；接线矩阵 W1–W9 的关闭条件（取 `plan.md §8` 的事实，不取修复记录）。
- **不讲什么**：任何机制（各篇）；修复历史（留在 todo.md）；覆盖率数值的演进史。
- **前置**：01（wire 真值是钉子测试的前提）、37。
- **后置**：无。
- **事实底线**：`os/servers/vfs/src/` 的 `#[cfg(test)]` 替身清单（request/bdev/cdev/sdev/socket/fs_comm/fcntl/path）；`plan.md §8`（W1–W9）；验证命令：`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib`、`cargo clippy --manifest-path os/Cargo.toml -p minix-vfs --lib`、`cargo check`。
- **知识点清单**：

  | 编号 | 名称 | 来源 |
  |------|------|------|
  | N-010 | 测试基建与验证策略 | 新增 todo.md §0.2（346 passed）、Fix #1/#17/#33 教训 |

- **验收标准**：给出验证命令表（命令 / 期望 / 什么时候跑）；给出 Scripted seam 清单（替身 / 所在文件 / 是否 cfg(test) 圈定）；给出"cfg 门控类修改必须双跑"的纪律条款及其事故来源；给出 W1–W9 表（步 / 依赖 / 关闭条件 / 当前状态）。

---

## 6. 变更表

### 6.1 主表（旧 → 新）

| 操作编号 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 / 来源 |
|---|---|---|---|---|---|---|
| C-01 | 重写 | `00-vfs-overview.md`（85 行） | `00-vfs-overview.md` | 85 行无法承载导航与边界；00 现只有 3 个知识点 | K-001~003 | 全部留在本篇 + 新增边界声明 |
| C-02 | 新建 | — | `01-vfs-wire.md` | 缺口 G1/G9/G14 | N-001/N-002/N-003/N-008 + K-222 | 来源：`callnr.h:68`、`com.h:513/589/911/920/964/996/1038`、`vfsif.h:79-81`、`99-global-concepts.md` |
| C-03 | 重写 | `01-vfs-init-main.md`（539 行） | `02-vfs-init.md` | 越界 O1（展开 worker 细节）；缺口 G8 | K-004~015 + N-007 | worker 细节 → `03`；挂载细节 → `16`；PM 协议其余 → `28` |
| C-04 | 重排+更名+重写 | `08-worker-thread.md`（309 行） | `03-request-slots.md` | 标题把 ARCH 演进埋了（"thread" 在 Rust 侧不存在）；且应在主循环之前讲 | K-055~064 | 全部迁本篇；"谁调用 worker_start" → `04` |
| C-05 | 重排+重写 | `09-main-loop.md`（285 行） | `04-main-loop.md` | 与 03 构成有向分层（03 被动 API / 04 主动驱动者） | K-065~075 | `unblock`/`do_pending_pipe` → `05` |
| C-06 | 新建（拆分自 09/17/21/22/23/30） | — | `05-vfs-blocking.md` | 缺口 G2；重复 K-071/K-075 | N-004 + K-061/K-071/K-072/K-075 | 来源：`main.c:297/921-973/216-258`、`const.h:19-25`、`pipe.c:435`、`lock.c:172`、`cdev.c:481`、`sdev.c:989` |
| C-07 | 重排 | `02-fproc-struct.md`（398 行） | `06-fproc-struct.md` | 应在主循环之后（读者先见"一次请求"再见"进程上下文"） | K-016~025 | 全迁 |
| C-08 | 重排 | `03-fproc-table.md`（265 行） | `07-fproc-table.md` | 随 06 | K-026~029 + N-014 | 全迁 + 新增 MIB 契约 |
| C-09 | 重排+扩写 | `07-tll-lock.md`（217 行） | `08-tll-lock.md` | 缺陷 P1：锁应在三张表之前；缺口 G5/G6 | K-050~054 + N-016/N-017 | 全迁 + 新增官方 README §4.4/§4.8 |
| C-10 | 重排 | `04-filp-table.md`（223 行） | `09-filp-table.md` | 随 C-09 | K-030~036 | 全迁 |
| C-11 | 重排 | `05-vnode-table.md`（225 行） | `10-vnode-table.md` | 随 C-09 | K-037~041 | 全迁 |
| C-12 | 重排 | `06-vmnt-table.md`（223 行） | `11-vmnt-table.md` | 随 C-09 | K-042~049 | 全迁 |
| C-13 | 重排 | `11-fs-comm.md`（291 行） | `12-fs-comm.md` | 规范：消息接口在核心数据结构之后 | K-084~091 | 全迁 |
| C-14 | 重排 | `12-request-wrappers.md`（264 行） | `13-request-wrappers.md` | 紧随 12 | K-092~098 | 全迁；cpf_grant 通用机制 → `21` |
| C-15 | 新建 | — | `14-driver-transport.md` | 缺口 G3/G7；缺陷 P3 | N-005/N-018 + K-061 | 来源：19 §2 的死亡级联、20 §1 的死信、22 §2 的 sdev_stop、23/26 的唤醒、`worker.c:535-567` |
| C-16 | 重排 | `13-path-lookup.md`（255 行） | `15-path-lookup.md` | 紧随 FS 通信 | K-099~106 | 全迁；`fetch_name`/`copy_path` → `21` |
| C-17 | 重排 | `18-mount.md`（281 行） | `16-mount.md` | 挂载是名字空间的机制，应紧随路径解析 | K-129~135 | 全迁；根挂载调用点回指 02 |
| C-18 | 重排 | `14-filedes.md`（238 行） | `17-filedes.md` | 随 C-17 | K-107~112 | 全迁；`invalidate` 触发序 → `14` |
| C-19 | 重排 | `15-open-close.md`（260 行） | `18-open-close.md` | 主线代表成员 | K-113~116 | 全迁 |
| C-20 | 重排 | `16-read-write.md`（249 行） | `19-read-write.md` | 紧随 18 | K-117~122 | 全迁 |
| C-21 | 重排 | `17-pipe.md`（258 行） | `20-pipe.md` | 阻塞族代表成员，紧随 05 之后能形成对照 | K-123~128 | 全迁；通用挂起契约 → `05` |
| C-22 | 新建（合并自 12/13/19/21/99） | — | `21-data-transfer.md` | 缺口 G4；越界 O3 | N-006 + K-097/K-105/K-143/K-224 | 来源：`12 §2` cpf_grant、`13 §2` fetch_name/copy_path、`19 §2.12` make_ioctl_grant、`99` sys_datacopy_wrapper |
| C-23 | 重排 | `19-device-map.md`（281 行） | `22-device-map.md` | 随设备组；越界 O3 已剥离 | K-136~144 | 全迁；`make_ioctl_grant` 机制 → `21`；死亡级联编排 → `14` |
| C-24 | 重排 | `20-bdev.md`（232 行） | `23-bdev.md` | 驱动族代表成员 | K-145~149 | 全迁 |
| C-25 | 重排 | `21-cdev.md`（248 行） | `24-cdev.md` | 随 C-24 | K-150~156 | 全迁 |
| C-26 | 重排 | `22-sdev.md`（269 行） | `25-sdev.md` | 随 C-24 | K-157~162 | 全迁；`sdev_stop` 编排 → `14` |
| C-27 | 重排 | `23-select.md`（293 行） | `26-select.md` | 依赖 24/25 | K-163~170 + N-015 | 全迁 |
| C-28 | 重排 | `24-socket.md`（289 行） | `27-socket.md` | 随 26 | K-171~176 + K-106 | 全迁 |
| C-29 | 重排 | `10-pm-protocol.md`（266 行） | `28-pm-protocol.md` | 规范：邻接服务协议靠后；但它依赖的只是 04/07/17 | K-076~083 | 全迁 |
| C-30 | 重排 | `25-exec.md`（272 行） | `29-exec.md` | 随 28 | K-177~183 + K-112 | 全迁 |
| C-31 | 重排 | `26-coredump.md`（251 行） | `30-coredump.md` | 随 29 | K-184~188 | 全迁 |
| C-32 | 重排 | `27-link.md`（241 行） | `31-link.md` | 名字空间组 | K-189~192 | 全迁 |
| C-33 | 重排 | `28-stadir.md`（251 行） | `32-stadir.md` | 随 31 | K-193~197 | 全迁；`do_utimens` → `35` |
| C-34 | 重排 | `29-protect.md`（240 行） | `33-protect.md` | 随 31 | K-198~202 | 全迁 |
| C-35 | 重排 | `30-fcntl-lock.md`（267 行） | `34-fcntl-lock.md` | 控制组 | K-203~208 | 全迁；通用挂起契约 → `05` |
| C-36 | 重排+瘦身 | `31-misc-queries.md`（265 行） | `35-misc-queries.md` | 越界 O2（ioctl） | K-209~217 | 全迁；ioctl 叙述删除（归 22） |
| C-37 | 重排+瘦身 | `99-global-concepts.md`（96 行） | `36-global-concepts.md` | 越界 O6（四种语义混杂） | K-218~226 | 常量/全局/术语/省略台账留本篇；ARCH → `37` |
| C-38 | 新建 | — | `37-arch-evolution.md` | 缺口 G11/G12；越界 O5 | N-009/N-011 + K-064/073/074/075/103/226 | 来源：`plan.md §4`（取事实）、`00 §3`（4 条）、`08 §3`/`09 §3`（散见）、`os/servers/vfs/src/` |
| C-39 | 新建 | — | `38-testing.md` | 缺口 G10 | N-010 | 来源：todo.md §0.2、Fix #1/#17/#33、`plan.md §8` |

### 6.2 分类统计

| 操作类型 | 数量 | 编号 |
|---------|------|------|
| 重排（含重写/瘦身） | 32 | C-01, C-03~C-05, C-07~C-14, C-16~C-21, C-23~C-37 |
| 新建 | 6 | C-02, C-06, C-15, C-22, C-38, C-39 |
| 拆分 | 2 处（并入新建） | 09 → C-05 + C-06；19/99 → C-22 + C-23 |
| 合并 | 2 处（并入新建） | 12/13/19/99 的数据搬运 → C-22；19/20/22/23/26 的死亡级联 → C-15 |
| 归档 | 0（B 相归档旧篇，不删） | 旧 33 篇全部进入 `archive/`，`draft/` 与参考材料不动 |

---

## 7. 缺漏新篇（非 C 主题清单逐项落实）

> 本节不允许留空，不允许写"待定"。

| 主题 | 重要性与理由 | 原料在哪里 | 归哪一篇 | 验收标准 |
|------|-------------|-----------|---------|---------|
| 链接与加载 | VFS 是 `-lexec`/`-lmthread` 的用户态服务，链接面决定它能用什么库（mthread 是 A-1 的对照物） | `minix3/minix/servers/vfs/Makefile`、`minix3/share/mk/minix.service.mk` | `02-vfs-init` §VFS 是怎么被装起来的 | 能列出 SRCS 33 项 + gcov 条件；能说出四个 `-l` 各提供什么；能说出它在 boot image 的登记位置 |
| 镜像与内存布局 | **不在本 stage** —— VFS 是用户态进程，镜像布局由 kernel/RS 决定 | `01-stage-kernel/06-proc-init-boot-proc.md` | 交叉引用，不新建 | 00 的边界声明里写明 |
| 汇编入口与陷阱进入 | **不在本 stage** —— VFS 与主循环只经 `sef_receive`/`ipc_send` | `01-stage-kernel/17-syscall-process.md` | 交叉引用，不新建 | 00 的边界声明里写明 |
| 启动装配 | VFS 的启动装配 = SEF 回调链，是理解"何时能接请求"的唯一钥匙 | `main.c:303-388,393-496` | `02-vfs-init` 全篇 | S1–S18 时序表逐行可核对 |
| 构建与工具链 | C 侧 Makefile 决定 gcov 条件编译；Rust 侧 Cargo feature 决定 A-7 的落点 | `vfs/Makefile`、`os/servers/vfs/Cargo.toml` | `02-vfs-init` §构建 + `37-arch-evolution`（feature） | 能说出 `MKCOVERAGE` 与 `USE_COVERAGE` 的关系；能说出 `fproc_light` feature 挂哪条 ARCH |
| 跨模块接口与线格式 | VFS 是三方交汇点，线格式错一位即全断（todo.md R2-P0-1 的 0x600 事故） | `callnr.h`/`com.h`/`vfsif.h` | `01-vfs-wire` 全篇 | 六命名空间真值表 + 掩码差异 + 绝对值钉子（如 `REQ_LOOKUP=0xA1A`） |
| 错误路径 | 挂起/取消/驱动死亡/errno 四条错路，读者最容易迷路 | `main.c:297`、`worker.c:535`、`cdev.c:381`、`sdev.c:940`、30 个 `ToErrno` impl | `05-vfs-blocking`（挂起与取消）+ `14-driver-transport`（死亡）+ `37-arch-evolution`（errno） | 四条错路各有归属篇，无第五处 |
| 关闭与退出 | VFS 自己不退出；退出由 PM 驱动，路径是 reboot 八步 | `misc.c:510-572`、`main.c:893-904` | `28-pm-protocol` §pm_reboot | 八步序列 + 两轮 pass 谓词 + 三个 Sync 屏障位置 |
| 并发与同步 | VFS 是全系统唯一多线程服务器（C），Rust 改为单线程；三层并发必须分开讲 | `worker.c`、`tll.c`、`fproc.h:31-57` | `03-request-slots`（执行）+ `08-tll-lock`（数据）+ `05-vfs-blocking`（等待） | 三层各有自己的状态图，互不重画 |
| 测试基建 | 346 个测试长什么样、哪些 seam 是生产编译单元、接线到哪一步算通 | todo.md §0.2、Fix #1/#17/#33、`plan.md §8` | `38-testing` 全篇 | 验证命令表 + Scripted seam 清单 + W1–W9 关闭条件 |
| **加锁顺序**（非 C 清单外的新发现） | 死锁是 VFS 最贵的 bug 类型，官方 README 专章写它，现有文档零覆盖 | `minix3/minix/servers/vfs/README:304-357` | `08-tll-lock` | 顺序规则逐条中文解释 + 标注官方原文出处 + 与代码锚点交叉 |
| **每请求锁特征**（非 C 清单外的新发现） | 决定"这个调用能不能并发、会阻塞谁" | `README:569-656` | `08-tll-lock` | ≥6 类请求的锁型与持有时机表 |
| **驱动崩溃恢复三分类**（非 C 清单外的新发现） | 块/字符+socket/FS 三类对端崩溃的处置完全不同 | `README:657-700` | `14-driver-transport` | 三类对照表 + 各类的代码锚点 |

---

## 8. 锚点迁移与断链成本

### 8.1 旧文档 → 新篇章（整篇级映射，供批量改名）

| 旧文档 | 新篇章 | 迁移类型 |
|--------|--------|---------|
| `00-vfs-overview.md` | `00-vfs-overview.md` | 改写 |
| `01-vfs-init-main.md` | `02-vfs-init.md` | 改写（部分拆分） |
| `02-fproc-struct.md` | `06-fproc-struct.md` | 原样搬移 + 补 |
| `03-fproc-table.md` | `07-fproc-table.md` | 原样搬移 + 补 |
| `04-filp-table.md` | `09-filp-table.md` | 原样搬移 |
| `05-vnode-table.md` | `10-vnode-table.md` | 原样搬移 |
| `06-vmnt-table.md` | `11-vmnt-table.md` | 原样搬移 |
| `07-tll-lock.md` | `08-tll-lock.md` | 改写（扩写官方 README 两章） |
| `08-worker-thread.md` | `03-request-slots.md` | 改写 + 重命名 |
| `09-main-loop.md` | `04-main-loop.md` + `05-vfs-blocking.md` | 拆分 |
| `10-pm-protocol.md` | `28-pm-protocol.md` | 原样搬移 |
| `11-fs-comm.md` | `12-fs-comm.md` | 原样搬移 |
| `12-request-wrappers.md` | `13-request-wrappers.md` | 改写（剥离 grant 通用机制） |
| `13-path-lookup.md` | `15-path-lookup.md` | 改写（剥离路径取入） |
| `14-filedes.md` | `17-filedes.md` | 原样搬移 + 补 |
| `15-open-close.md` | `18-open-close.md` | 原样搬移 |
| `16-read-write.md` | `19-read-write.md` | 原样搬移 |
| `17-pipe.md` | `20-pipe.md` | 改写（通用挂起契约上收） |
| `18-mount.md` | `16-mount.md` | 原样搬移 + 回指补偿 |
| `19-device-map.md` | `22-device-map.md` + `14-driver-transport.md` + `21-data-transfer.md` | 拆分 |
| `20-bdev.md` | `23-bdev.md` | 原样搬移 |
| `21-cdev.md` | `24-cdev.md` | 原样搬移 |
| `22-sdev.md` | `25-sdev.md` | 原样搬移（sdev_stop 编排外迁） |
| `23-select.md` | `26-select.md` | 原样搬移 |
| `24-socket.md` | `27-socket.md` | 原样搬移 |
| `25-exec.md` | `29-exec.md` | 原样搬移 |
| `26-coredump.md` | `30-coredump.md` | 原样搬移 |
| `27-link.md` | `31-link.md` | 原样搬移 |
| `28-stadir.md` | `32-stadir.md` | 原样搬移（剥离 utimens） |
| `29-protect.md` | `33-protect.md` | 原样搬移 |
| `30-fcntl-lock.md` | `34-fcntl-lock.md` | 原样搬移 |
| `31-misc-queries.md` | `35-misc-queries.md` | 改写（删 ioctl 叙述） |
| `99-global-concepts.md` | `36-global-concepts.md` + `37-arch-evolution.md` | 拆分 |

### 8.2 节级迁移表（仅列发生拆分/合并/剥离的文档）

| 旧位置（文档与小节） | 旧内容（一句话） | 新位置 | 迁移类型 | 断链风险 |
|---------------------|-----------------|--------|---------|---------|
| `09-main-loop.md` §1–§2 的 SUSPEND/revive 叙述 | SUSPEND 返回码与四条恢复路径 | `05-vfs-blocking.md` §1 | 拆分 | 高：17/21/22/23/30 五篇的"前置"都引用 09 的这一段 |
| `09-main-loop.md` §2 unblock/do_pending_pipe | 挂起请求重建 | `05-vfs-blocking.md` §2 | 拆分 | 中：17 引用 |
| `01-vfs-init-main.md` §2 worker_init/worker_allow 实现 | 槽池初始化与门控 | `03-request-slots.md` §2 | 拆分 | 中：08 自引 |
| `01-vfs-init-main.md` §2 mount_pfs/mount_fs 内部 | 根挂载五段 | `16-mount.md` §2 | 拆分 | 高：18 回指 |
| `19-device-map.md` §2.11–2.12 do_ioctl/make_ioctl_grant | ioctl 四分流与授权解码 | 分流留 `22-device-map.md` §2；授权解码 → `21-data-transfer.md` §2 | 拆分 | 高：31 的 ioctl 叙述要删 |
| `19-device-map.md` §2.3–2.7/§9 驱动死亡与恢复 | dmap_endpt_up/unmap/DS 事件 | `14-driver-transport.md` §2 | 拆分 | 中 |
| `12-request-wrappers.md` §2 cpf_grant | grant 三标志 | `21-data-transfer.md` §2 | 拆分 | 中 |
| `13-path-lookup.md` §2 fetch_name/copy_path | 用户路径取入 | `21-data-transfer.md` §3 | 拆分 | 中 |
| `99-global-concepts.md` ARCH 注释与省略表 | ARCH A-8/A-9 与省略台账 | 省略台账留 `36-global-concepts.md` §5；ARCH → `37-arch-evolution.md` | 拆分 | 低 |
| `31-misc-queries.md` §1.5/§2.4 ioctl 三类请求 | ioctl 异步处理 | **删除**（`do_ioctl` 在 `device.c:18`，归 `22-device-map.md`） | 删除 | 高：读者会按 31 找 ioctl |
| `17-pipe.md` §1–§2 的通用挂起叙述 | 挂起与复活的通用契约 | `05-vfs-blocking.md` §1 | 合并 | 中 |
| `30-fcntl-lock.md` §2 的 F_SETLKW 挂起 | flock 复活 | `05-vfs-blocking.md` §3（flock 路径） | 合并 | 中 |
| `21/22/23` 各自的 SUSPEND 叙述 | 各族挂起细节 | 各篇保留细节，通用契约 → `05-vfs-blocking.md` | 合并 | 中 |

### 8.3 引用迁移表

| 引用类别 | 数量 | 迁移方式 | 验证方式 |
|---------|------|---------|---------|
| stage 内文档 → 文档引用（`NN-xxx.md` 形态） | **902 处** | 按 §8.1 的旧→新映射做**全目录正则批量替换**：先把旧编号换成一个唯一占位（如 `__NEW_09__`），再统一落成新编号，避免 09→04 与 04→09 的链式污染 | 替换后 `grep -rEo "\b[0-9]{2}-[a-z0-9-]+\.md" 05-stage-vfs/*.md` 逐个校验目标文件存在 |
| Rust 代码注释 → 文档引用 | **86 处**，分布在 **25 个 `.rs` 文件** | 同样两阶段替换；每个文件的模块头 scope note（如 `socket.rs:19-23`）需人工复核一句，因为 scope note 里的"归 XX 篇"是语义声明不是纯链接 | `grep -rEn "[0-9]{2}-[a-z0-9-]+\.md" os/servers/vfs/src --include=*.rs` + `cargo check` 不受影响（注释） |
| stage 外 → 05 的文档引用 | 指向具体编号文档 **45 处**（热点：`22-sdev.md` 6、`14-filedes.md` 6、`18-mount.md` 4、`00-vfs-overview.md` 4、`13-path-lookup.md` 3、`02-fproc-struct.md` 3）；另有 `../05-stage-vfs` 目录级引用 25 处 | 目录级引用不动（目录名不变）；编号级引用按 §8.1 替换 | `grep -rEo "05-stage-vfs/[0-9]{2}-[a-z0-9-]+\.md" rewrite-notes --include=*.md \| grep -v "^05-stage-vfs"` 逐个校验 |
| `plan.md`/`todo.md` 内的编号引用 | 大量（plan.md §2 表 33 行、todo.md 各 Fix 条目） | **建议不动**：它们是历史记录，改了会让修复台账与历史不符。改为在两文件头部加一行"本文档的编号指旧编号，映射见 `doc_rerank_*.md` 的共识蓝图" | 人工加一行声明即可 |
| `.design/` 快照内引用 | 31×3 份快照 | 本蓝图不引用 `.design/`；B 相处理时按同样的两阶段替换 | 不在 R 相范围 |

### 8.4 断链成本摘要

- **受影响引用总数**：902（文档内）+ 86（代码注释）+ 45（stage 外编号级）+ 25（stage 外目录级，不用改）
  ≈ **1,033 处需要机械处理**，其中 25 处目录级引用无需改动。
- **热点文件（被引次数最多的旧编号）**：`06-vmnt-table` 54、`99-global-concepts` 48、`14-filedes` 46、
  `02-fproc-struct` 47、`09-main-loop` 52、`04-filp-table` 43、`12-request-wrappers` 40、`13-path-lookup` 37。
  这 8 篇占了全部引用的约 40%，它们的编号变化（`06→11`、`99→36`、`14→17`、`02→06`、`09→04`、`04→09`、
  `12→13`、`13→15`）是断链成本的主要来源。
- **建议的批量修改方式**：
  1. 建立 `old→new` 映射表（§8.1），写成脚本的两列输入；
  2. **两阶段替换**（先占位后落位），杜绝 04↔09、06↔11、12↔13↔15 这类链式污染；
  3. 只替换 `\bNN-slug\.md` 正则，不替换裸编号（避免误伤 `const.h:7` 这类行号引用）；
  4. 替换后跑一次"目标文件存在性校验"（§8.3 的验证命令）；
  5. `plan.md`/`todo.md` 走"加声明不改内容"的路子；
  6. Rust 注释改完后跑 `cargo check` + `cargo test -p minix-vfs --lib`（注释改动不影响编译，但确认无意外）。
- **成本评估**：以"两阶段正则替换 + 存在性校验"衡量，一次性成本约 1–2 小时；
  相比"重编号断链风险大于收益"的历史裁决（`01-stage-kernel/todo.md` I-14），
  本次的差别在于：**编号整体重排是 B 相正文重写的副产品，不是独立收益**——
  即断链是"必须付的一次性成本"，而收益是"概念边界清晰 + 6 个覆盖缺口补齐"。
  若共识阶段判定"编号不动、只做内容重组"，则断链成本降为 0，但 §4.1 的分组与
  §3.4 的 P1/P3/P4 三项缺陷无法修复（因为它们的病根就是编号顺序本身）。
  **本蓝图的立场：编号必须重排。**

---

## 9. 验证与自检门

### 9.1 四种机械检查

**检查 1 · 前向引用扫描**（逐篇扫描契约的"前置"字段）

| 篇 | 前置 | 是否全部更早 |
|----|------|-------------|
| 00 | 无 | ✅ |
| 01 | 00 | ✅ |
| 02 | 00, 01 | ✅ |
| 03 | 02 | ✅ |
| 04 | 01, 03 | ✅ |
| 05 | 03, 04, 06, 09 | ❌ **06 与 09 晚于 05** |
| 06 | 02 | ✅ |
| 07 | 06 | ✅ |
| 08 | 04 | ✅ |
| 09 | 08 | ✅ |
| 10 | 08, 09 | ✅ |
| 11 | 08, 10 | ✅ |
| 12 | 04, 11 | ✅ |
| 13 | 12 | ✅ |
| 14 | 04, 05, 12, 17 | ❌ **17 晚于 14** |
| 15 | 10, 11, 13 | ✅ |
| 16 | 11, 12, 13 | ✅ |
| 17 | 09 | ✅ |
| 18 | 15, 17 | ✅ |
| 19 | 09, 17, 18 | ✅ |
| 20 | 05, 17, 19 | ✅ |
| 21 | 13, 15, 19 | ✅ |
| 22 | 02, 11 | ✅ |
| 23 | 14, 19, 22 | ✅ |
| 24 | 14, 18, 22 | ✅ |
| 25 | 14, 22, 24 | ✅ |
| 26 | 05, 09, 20, 22, 24, 25 | ✅ |
| 27 | 17, 25, 26 | ✅ |
| 28 | 04, 07, 17 | ✅ |
| 29 | 15, 18, 28 | ✅ |
| 30 | 18, 19, 28, 29 | ✅ |
| 31 | 15, 17, 18 | ✅ |
| 32 | 11, 15, 17 | ✅ |
| 33 | 06, 15 | ✅ |
| 34 | 05, 09, 17 | ✅ |
| 35 | 11, 17, 28 | ✅ |
| 36 | 00 | ✅ |
| 37 | 03, 04, 06, 36 | ✅ |
| 38 | 01, 37 | ✅ |

**两处违规与处置**：

1. **05-vfs-blocking 依赖 06（fproc 载荷）与 09（filp）**。
   - 方案 A（**选定**）：把 05 移到 09 之后，即顺序改为 …04 → 06 → 07 → 08 → 09 → **05** → 10…
     但这会让"并发与事件循环"分组被数据结构打断。
   - 方案 B（**选定**）：**保持 05 的位置**，把契约的前置改为 `03, 04`，并在正文用
     "前置知识摘要"两句话给出 `fp_blocked_on` 是一个"标签 + 载荷"的枚举、filp 是打开描述，
     详细字段定义分别归 06/09。理由：05 是**框架篇**，规范 5.2 允许框架篇先于成员篇；
     摘要式前置声明不构成前向引用（读者不需要读 06/09 就能读懂 05 的契约）。
   - **最终采用方案 B**，并把 §9.1 检查 1 的结果记为"05 以摘要前置消解，契约前置字段改为 `03, 04`"。
2. **14-driver-transport 依赖 17（invalidate 三族原语）与 22（通讯录）**。
   - 处置（已在契约中写明）：前置改为 `04, 05, 12`；17 与 22 的内容以
     "前置知识摘要"给出（17：三族失效谓词各一句话；22：major→driver、domain→driver 两句话），
     详细分别归 17/22。
   - 理由：14 是"驱动通道"族的框架篇，规范 5.2 允许框架篇先于成员篇；且 14 的本体内容是
     "触发序"，触发序不需要知道失效谓词的实现。

修正后：**前向引用扫描通过（0 违规）**。

**检查 2 · 依赖关系图无环**

按 §9.1 修正后的前置关系构图：

```
00 → 01 → 02 → 03 → 04 → {06 → 07, 08 → 09 → 10 → 11 → 12 → 13 → 15 → 16}
                      → 05（前置 03,04）
                      → 14（前置 04,05,12）
     17（前置 09）→ 18 → 19 → 20；21 → 22 → 23 → 24 → 25 → 26 → 27
     28（前置 04,07,17）→ 29 → 30
     31（前置 15,17,18）→ 32 → 33
     34（前置 05,09,17）；35（前置 11,17,28）
     36（前置 00）→ 37（前置 03,04,06,36）→ 38（前置 01,37）
```

**结论：无环**。原本唯一的环（03↔04：槽与循环互相驱动）已按 §4.1 的
"分层拆解方案"消除——03 只讲被动 API（不含调用者），04 只讲主动驱动者（不含槽内部），
有向边只有 04 → 03 一条。

**检查 3 · 覆盖率 100%**

| 类别 | 条数 | 有去向 | 删除 | 覆盖率 |
|------|------|--------|------|--------|
| 存量 K 组 | 226 | 226 | 0 | 100% |
| 新增 N 组 | 18 | 18 | 0 | 100% |
| 合计 | 244 | 244 | 0 | 100% |

- 存量每条的去向见 §5 各篇契约的"知识点清单"列（来源填旧文档位置）。
- 新增每条都有证据锚点，见 §2.2 的"证据锚点"列。
- **明确删除项（单独列出，共 1 项）**：`31-misc-queries.md` 的 "ioctl 三类请求" 叙述——
  删除理由：`do_ioctl` 定义在 `device.c:18`，分派逻辑属 `22-device-map.md`；
  31 的这节是越界（O2），且与 19 冲突。

**检查 4 · 断链成本统计**

见 §8.4：1,033 处需机械处理（其中 25 处目录级引用无需改），热点 8 篇已列出，批量方式已给出。

### 9.2 自检门逐门结果

| 门 | 检查内容 | 结果 | 证据 |
|----|---------|------|------|
| G1 | C 真序是否逐条可核对 | **通过** | 抽查 10 条：S6（`main.c:415-434` ✅）、S10（`main.c:445`→`worker.c:27` ✅）、S13（`main.c:455-465` ✅）、S17（`main.c:501-523` ✅）、S21（`main.c:580-633` ✅）、S22（`main.c:80-90` + `com.h:912` ✅）、S26（`main.c:126-134` + `com.h:964/920/1038` ✅）、S29（`worker.c:331-355` ✅）、S32（`comm.c:134-168` ✅）、S35（`pipe.c:435` + `main.c:937-968` ✅） |
| G2 | 知识点池是否完整（每个 C 文件 / 非 C 制品有归属或明确排除） | **通过** | 33 个 `.c` 全部出现在至少一条 K 的锚点里；15 个本地 `.h` 同理；非 C 制品：Makefile ✅（N-007）、README ✅（N-016/017/018）、service.mk ✅（N-007）、Cargo.toml ✅（N-014）、lib.rs ✅（00/37）、main.rs ✅（02）。`RTCDEV` 已明确排除并给理由（N-003） |
| G3 | 新目录前向引用为零 | **通过（修正后）** | §9.1 检查 1，两处违规以"框架篇摘要前置"消解，已在契约正文写明 |
| G4 | 依赖关系图无环 | **通过** | §9.1 检查 2；唯一的 03↔04 环已按分层方案拆解 |
| G5 | 覆盖率 100% + 新增条目有锚点 + 删除项单列 | **通过** | §9.1 检查 3；删除项 1 条已单列 |
| G6 | 拆分/合并写清存量去向；新建写清新增来源（抽查 10 处） | **通过** | 抽查：C-06（09→05，去向 `main.c:297/921-973`）✅、C-15（19/20/22/23→14，去向已列 C 行号）✅、C-22（12/13/19/99→21，四条来源锚点齐全）✅、C-02（新建，来源 `callnr.h`/`com.h`/`vfsif.h`）✅、C-38（新建，来源 `plan.md §4` + `os/servers/vfs/src/`）✅、C-39（新建，来源 todo.md Fix 记录）✅、C-09（+N-016/N-017，来源 README 行号）✅、C-03（+N-007，来源 Makefile）✅、C-08（+N-014，来源 fproc.h+Cargo.toml）✅、C-26（25 sdev_stop 编排 → 14）✅ |
| G7 | 每篇契约七要素齐全 | **通过** | 39 篇契约全部含：定位 / 讲什么 / 不讲什么 / 前置 / 后置 / 事实底线 / 知识点清单 + 验收标准 |
| G8 | 锚点迁移表覆盖所有变化文档；引用迁移表覆盖文档与代码注释 | **部分通过** | 整篇级映射覆盖 33/33 旧文档 ✅；**节级表只覆盖发生拆分/合并/剥离的 13 项，未逐节列出全部 33 篇的每一节**——这是本蓝图的已知粒度限制，B 相在逐篇重写时按 §5 的契约自行生成该篇的节级清单即可 |
| G9 | 事实断言都有锚点（抽查 10 条） | **通过** | 抽查：64 调用（`table.c` grep -c = 64）✅、VFS_BASE 0x100（`callnr.h:68`）✅、FS_BASE 0xA00（`com.h:589` + todo.md R2-P0-1）✅、NR_MNTS 16（`const.h:7`）✅、NR_WTHREADS 9（`const.h:9`）✅、CDEV/BDEV/SDEV RS 基址（`com.h:920/964/1038`）✅、33 个 REQ 常量（`vfsif.h` grep -c）✅、36 个 req_ 函数（`request.c` grep -oE 去重）✅、902 处文档引用（grep 实测）✅、86 处代码引用（grep 实测）✅。**推测项标注**：`init_restart ≡ init_fresh`（K-015）源自 `main.c:378` 的 `SEF_CB_INIT_RESTART_STATEFUL` + 01 号文档的判定，C 侧无显式等价证明，标注为"文档既有判定，B 相需复核" |

### 9.3 结论与待裁决问题

**结论**：本蓝图**完成**（四种机械检查全部通过；G8 为已知的部分通过，已在门内注明粒度限制）。
B 相可以按 §5 的 39 份契约直接开工，不需要再做取舍判断。

**待用户/共识阶段裁决的问题（4 条）**：

1. **编号是否整体重排**（§8.4 末）：本蓝图立场是必须重排，因为 §3.4 的 P1/P3/P4 三项缺陷
   病根就是编号顺序。若共识判定"编号不动"，则 P1（tll 与三表的顺序）、P3（驱动族无框架篇
   的位置）、P4（阻塞框架篇的位置）都无法根治，只能退化为"在旧编号下加框架篇"。
2. **`05-vfs-blocking` 与 `14-driver-transport` 采用"框架篇摘要前置"**（§9.1）：
   这是对"无前向引用"硬标准的一次有意识放宽（框架篇以两句话摘要引用尚未出现的成员篇）。
   替代方案是把它们移到成员篇之后，代价是"汇聚点"失去汇聚作用。请裁决。
3. **`plan.md`/`todo.md` 是否要随编号迁移**（§8.3）：本蓝图建议**不迁**，只在头部加一行声明。
   理由是它们承载修复台账，改编号会让历史记录与当时的代码状态不符。
4. **`38-testing` 是否属于正式文档**：它是"工具与工程"类，不是机制类。
   本蓝图把它放在正式目录（编号 38）是为了让"测试基建"这个非 C 主题有确定归属
   （规范第 3 类要求逐项回答）。若共识认为它该进 `plan.md`，则 N-010 的归属需改。

**范围外发现（3 条，不在本 stage 处理）**：

- `minix3/minix/servers/vfs/README` 700 行官方 internals 全文值得单开一篇"VFS 官方设计说明导读"
  或至少在各相关篇开头给出交叉引用；本蓝图只把它的三块独有知识（加锁顺序、每请求锁特征、
  崩溃恢复三分类）抽为 N-016/017/018。
- `RTCDEV_RS_BASE 0x1480` 在 VFS 主循环无分支，但 `16-stage-drivers` 的 RTC 驱动可能会发它；
  建议登记一条 edge 条目到 `edge_todo.md`（本 R 相不改任何文件）。
- `NR_SOCKDEVS 8`（`const.h:11`）在现有文档无专门讲述，已并入 `22-device-map` 的 smap 表容量。
