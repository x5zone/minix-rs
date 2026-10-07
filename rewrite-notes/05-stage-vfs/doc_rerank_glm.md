# 05-stage-vfs 文档重建蓝图（glm）

## 0. 元数据

```text
执行者 = glm
日期 = 2026-09-19
目标目录 = rewrite-notes/05-stage-vfs/
仓库根目录 = /home/xzhao/github/minix-rs
当前提交号 = 2696568b710ecbf7102b1f3a35fc3a8db2ab5590（feat(pm): S8 首片——mcontext 族两调用接线）
阶段类型判定 = 服务事件循环型（第九部分分类），兼有"系统调用集合型"的并行体特征；
              按"服务为什么存在 → 诞生与初始化 → 消息接口 → 核心数据结构 → 按场景
              分组的请求处理 → 查询与杂项 → 与邻接服务的协议"组织，一次请求的生命
              周期（收消息→解析→执行→回复/挂起→复活）为主线的候选骨架之一，
              最终选择见 §4.2。
```

### 0.1 审查范围

**文档（重建对象）**：`00-vfs-overview.md`、`01`～`31` 号编号文档、`99-global-concepts.md`，共 33 篇，合计 8,542 行。

**参考材料（不重建，只作证据与边界）**：`plan.md`（553 行，2026-08-16 定稿的旧重组计划）、`todo.md`（548 行，2026-09-06/09 两轮架构审查 + Fix #1–#38 修复记录）、`draft/`（旧 fork 主线素材 27 件）、`archive/todo-R1-archive-2026-09-09.md`。

**范围外**：`.review/`（review 中间产物）、隐藏中间产物目录（AGENTS.md 约定正式文档不得引用，本报告不引用其内容，仅在本节声明该约定对重建的约束：旧文档头部引用中间产物快照的行，重建时一律删除）、`minix3/minix/fs/`（FS 服务端，15-stage-fs 范围）、其它 stage 目录。

### 0.2 读取清单（七类输入逐一交代）

| 类别 | 实际读取 | 方式 |
|------|---------|------|
| 1. 目标目录全部文档 | 33 篇编号文档全文或头部+骨架精读；`plan.md`/`todo.md` 全文 | 主会话直读 00/01/02/03–07 头部与概念章/08/09/99 + 两个并行探查 agent 精读 10–22、23–31 并回报结构化摘要 |
| 2. Minix3 VFS C 源码 | `minix3/minix/servers/vfs/` 33 个 .c + 15 个本地 .h（.c+.h 合计 17,742 行，`wc -l` 实测）；协议头 `minix3/minix/include/minix/{com.h,callnr.h,vfsif.h}` | 主会话直接 grep/sed 验证承重锚点（启动链、主循环、call_vec、协议常量、容量常量、锁字段、worker 函数面） |
| 3. 非 C 制品 | `servers/vfs/Makefile`（PROG=vfs、SRCS 33 文件、`LDADD -lmthread`、MKCOVERAGE→gcov.c）；SEF 回调注册面（main.c:374-390）；DS 订阅串（main.c:441 `"drv\\.[bc]..\\..*"`）；`minix/include/minix/com.h` 线协议常量；`vfsif.h` REQ/TRNS 面 | 主会话直读 |
| 4. 阶段边界材料 | `00-master-plan/README.md`（boot 两层语义、启动因果链）；`edge_todo.md`（E1/E5/E9/E-REQWIRE/E-VFSWIRE 状态）；本 stage `plan.md`/`todo.md`（含 §5.4 排除表、§8 接线矩阵 W1–W9） | 主会话直读 |
| 5. 前一个 stage 的 overview | `04-stage-pm/` 文档清单（00–20+99，22 篇 + plan/todo）；master-plan 对 PM/VFS 启动顺序的叙述 | 清单级核对（PM 侧已讲 `mproc`/fork 发送端/`05-vfs-interaction`，本 stage 不许重复展开的部分在 §3.4 越界表） |
| 6. Rust 实现入口 | `os/servers/vfs/src/` 34 文件 24,784 行（含测试）；`minix-types/src/ipc/{vfs.rs,fs_driver.rs}`；`minix-fs/src/protocol.rs` | 并行探查 agent 盘点 + 主会话抽验（route_message、syscalls.rs 64 臂、CallTable 删除残留、VfsState 15 字段） |
| 7. 写法范例 | `01-stage-kernel/06-todo.md` 的"新文档契约"写法（讲什么/不讲什么/下放给谁/验收是什么）——只学写法不搬内容 | 范式引用 |

### 0.3 使用的命令与关键输出（证据摘录）

```text
# C 文件与行数
$ wc -l minix3/minix/servers/vfs/*.c minix3/minix/servers/vfs/*.h | tail -1
  17742 total
# 启动链核对（main.c）
$ sed -n '54,145p' main.c      → main 主循环八路分发全序
$ sed -n '300,530p' main.c     → SEF 六回调 + sef_cb_init_fresh 十一段 + do_init_root
# 调用面
$ grep -c 'CALL(' servers/vfs/table.c → 64 行 CALL 项（VFS_READ..VFS_SHUTDOWN）
$ grep -n 'define VFS_' minix/include/minix/callnr.h → VFS_BASE 0x100（:68），64 调用号（:72-135）
# PM 协议面
$ grep -n 'VFS_PM_RQ\|VFS_PM_RS\|FS_BASE\|CDEV_RS_BASE\|BDEV_RS_BASE\|SDEV_RS_BASE\|VFS_TRANSID' com.h
  → 0x900(:513)/0x980(:514)/12 RQ(:520-531)/11 RS(:534-544)/FS_BASE 0xA00(:589)/
    0x480(:920)/0x580(:964)/0x1980(:1038)/掩码 ~0x7f(:923,:967,:1041)
# REQ 面
$ grep -n '#define REQ_\|NREQS\|TRNS_' minix/include/minix/vfsif.h
  → REQ_GETNODE..REQ_BPEEK 共 33 常量(:41-73)，REQ_GETNODE 注释 "Should be removed"(:41)，
    NREQS 34(:75)，TRNS_GET_ID/ADD_ID/DEL_ID(:79-81)
# 容量常量
$ grep -n 'NR_FILPS\|NR_LOCKS\|NR_MNTS\|NR_VNODES\|NR_NONEDEVS\|SYMLOOP\|CTTY' servers/vfs/const.h
  → 1024(:5)/8(:6)/**16(:7)**/1024(:8)/NR_WTHREADS 9(:9)/=NR_MNTS(:12)/16(:32)/CTTY_ENDPT=VFS_PROC_NR(:52)
# 锁字段（tll 归属的关键证据）
$ grep -n 'tll\|filp_lock' servers/vfs/file.h servers/vfs/vnode.h servers/vfs/vmnt.h
  → file.h:14 mutex_t filp_lock（filp 不用 tll）；vnode.h:22 tll_t v_lock；vmnt.h:9 tll_t m_lock
# 引用普查
$ grep -o 'NN-xxx\.md' *.md | wc -l   → 逐篇统计（§8.3 热点表）
$ grep -rn '05-stage-vfs|[0-9][0-9]-[a-z-]*\.md' os/servers/vfs/src/ → 82 行 86 次（§8.3）
```

### 0.4 范围外发现（末置，按步骤 0 要求）

1. **mfs 侧 25 项 Pending**（`os/fs/mfs/src/table.rs:57` 的 `fs_lookup` 等）——REQ 协议的对端执行半，归 15-stage-fs 与接线矩阵 W9（`plan.md §8`），本 stage 只保留契约面。
2. **Rust `sdev.rs` 仅 228 行**（C `sdev.c` 1,114 行）——线协议半已外移至 minix-sockdriver crate，Rust 文件自述"调用方半边"。22 号文档的"Rust 模块"声明必须按此改写（现为过时口径）。
3. **`lib.rs` 头注释过时**（`os/servers/vfs/src/lib.rs:10-22` 自称 mthread 多线程、只列 5 模块；实际 34 模块单线程状态机）——代码注释问题，登记给修复轨道，本蓝图在 99 号契约中要求文档按现状描述。
4. **`VFS_SHUTDOWN` 调用号实为 socket `shutdown(2)`**（`table.c` 末项 `CALL(VFS_SHUTDOWN)=do_shutdown`，实现是 `socket.c` 的套接字关断）——不是"服务器关机"。24 号归它，无遗漏。

---

## 1. C 真序

### 1.1 阶段类型判定

VFS 是**服务事件循环型**：一个用户态服务器，`main` 线程收消息、按类型分发、回复或挂起（`main.c:69-143`）。同时它挂载 64 个系统调用（`table.c` call_vec），有明显的**系统调用集合型**切面。因此真序表分两段：**启动段**（进程诞生→初始化→根挂载→进入循环）与**循环段**（消息接收→九级分发→处理→回复/挂起→复活）。全部锚点由主会话直接从 C 源码验证，不从现有文档转述。

### 1.2 真序表·启动段

| 步骤 | 动作 | C 锚点 | 说明 |
|------|------|--------|------|
| S01 | `main()` 进入 | `main.c:54` | 极薄入口：SEF 启动后直接进主循环 |
| S02 | `sef_local_startup()` 注册生命周期回调 | `main.c:374-390` | **6 个** `sef_setcb_*`：init_fresh、init_restart(SEF_CB_INIT_RESTART_STATEFUL)、init_lu、lu_prepare、lu_state_changed、lu_state_isvalid(standard)，然后 `sef_startup()` |
| S03 | `sef_cb_lu_prepare`（LU 预备） | `main.c:303-323` | 仅 REQUEST_FREE/PROTOCOL_FREE 态可备，要求 `worker_idle()` 全闲，`worker_cleanup()` 收工；余者 ENOTREADY |
| S04 | `sef_cb_lu_state_changed`（LU 回滚重建） | `main.c:325-336`一带 | 失败回滚到 NULL 态时 `worker_init()` 重建工人 |
| S05 | `sef_cb_init_lu`（新实例 LU 初始化） | `main.c:338-358`一带 | 默认状态迁移后按 prepare_state 重建工人 |
| S06 | `sef_cb_init_fresh`：fproc 槽清零 | `main.c:405-408` | 全表 `fp_endpoint=NONE; fp_pid=PID_FREE` 双哨兵 |
| S07 | VFS_PM_INIT 握手循环 | `main.c:410-436` | 逐条 `sef_receive(PM)`，每条填一槽（fp_flags=NOFLAGS、uid/gid=SYS_UID/SYS_GID、umask=~0）；`VFS_PM_ENDPT==NONE` 终止（:428-431）；`ipc_send(PM, OK)` 同步屏障（:435-436） |
| S08 | `system_hz = sys_hz()` | `main.c:438` | 时钟频率，select 定时器依赖 |
| S09 | `ds_subscribe("drv\\.[bc]..\\..*")` | `main.c:441` | 订阅块/字符驱动上下线事件（DSF_INITIAL\|DSF_OVERWRITE） |
| S10 | `worker_init()` | `main.c:445` → `worker.c:27` | 创建 NR_WTHREADS=9（`const.h:9`）个 mthread 工人；`pending/busy/block_all` 零化（`worker.c:10-12`） |
| S11 | `bsf_lock` 互斥初始化 | `main.c:448` | 块特殊文件全局锁（读写在 16 号讲） |
| S12 | `init_dmap()` / `init_smap()` | `main.c:451-452` | 设备表/套接字表初始化 |
| S13 | rproctab 拷贝 + `map_service` 循环 | `main.c:455-467` | 从 RS 拷 boot 服务表，逐个映射进 dmap（确立"哪些 endpoint 是 FS/驱动"） |
| S14 | 第二遍 fproc 循环 | `main.c:469-483` | 每槽 `mutex_init(fp_lock)`、`fp_worker=NULL`、清 `fp_filp[OPEN_MAX]` 与 `fp_rd/fp_wd` |
| S15 | `init_vnodes()` / `init_vmnts()` / `init_select()` / `init_filps()` | `main.c:486-489` | 四表初始化（注意 C 的初始化顺序：vnode→vmnt→select→filp） |
| S16 | `worker_start(do_init_root)` | `main.c:492-495` | 根挂载在第一个工人任务里执行 |
| S17 | `do_init_root`：门控→PFS→根 FS→放行 | `main.c:501-523` | `worker_allow(FALSE)`(:507) → `mount_pfs()`(:510) → `mount_fs(DEV_IMGRD,"bootramdisk","/",MFS_PROC_NR,…)`(:513-518，硬编码 "mfs"/"fs_imgrd" 带 FIXME) → 失败 panic → `worker_allow(TRUE)`(:522) |

### 1.3 真序表·循环段

| 步骤 | 动作 | C 锚点 | 说明 |
|------|------|--------|------|
| L01 | `worker_yield()` | `main.c:70`（循环体 `main.c:69-143`） | 交出 `self` TLS，让工人运行 |
| L02 | `send_work()` | `main.c:72` → `comm.c:37` | 扫各 vmnt 的 m_comm 窗口，刷新排队 FS 请求 |
| L03 | `get_work()` | `main.c:77` → `main.c:580` | **复活优先**：`reviving != 0` 时先扫 `FP_REVIVED` 进程走 `unblock`（:590 一带），否则 `sef_receive(ANY)` 循环 + endpoint 一致性校验（:599 一带）；返回 FALSE 表示"无新消息但已启动复活工作" |
| L04 | FS 回复路（最高优先） | `main.c:80-90` | `TRNS_GET_ID` 提取 transid（`vfsif.h:79`），`IS_VFS_FS_TRANSID` 命中 → `worker_get(transid-VFS_TRANSID)` 定位工人 → `TRNS_DEL_ID` 剥壳 → `do_reply(wp)`（`main.c:187` 一带） |
| L05 | PM 路 | `main.c:91-93` | `who_e == PM_PROC_NR` → `service_pm()`（`main.c:764`；延期半 `service_pm_postponed` `main.c:668` 一带） |
| L06 | notify 路 | `main.c:95-117` | DS→`ds_event`（经 `worker_can_start` 守门 + `handle_work`）；KERNEL→`mthread_stacktraces`；CLOCK→`expire_timers(timestamp)`（select 超时） |
| L07 | task 忽略路 | `main.c:118-124` | `who_p < 0`（内核任务伪 endpoint）的非 notify 消息直接忽略 |
| L08 | 驱动回复三路 | `main.c:126-134` | `IS_BDEV_RS`（`com.h:967`，基 0x580）→`bdev_reply`；`IS_CDEV_RS`（0x480）→`cdev_reply`；`IS_SDEV_RS`（0x1980）→`sdev_reply` |
| L09 | 普通 syscall | `main.c:135-138` → `handle_work`(`main.c:146`) → `do_work` | `handle_work` 做 FP_SRV_PROC/VMNT_CALLBACK/worker_available 死锁防护后以 spare 起工人；`do_work` 查 `call_vec[call_index]`（`main.c:286-290`）执行 handler，`error != SUSPEND` 才 `reply`（`main.c:297`） |
| L10 | 回复原语 | `reply` `main.c:638`；`replycode` `main.c:655` | `ipc_sendnb` 非阻塞回复 |
| L11 | 复活执行 | `unblock` `main.c:921-973` | 按 `fp_blocked_on` 重建请求消息：PIPE→`worker_start(do_pending_pipe)` 返回 FALSE；FLOCK→重入 `do_work`；清 `FP_REVIVED`、`reviving--` |
| L12 | 分发表 | `table.c:17-82` | `CALL(n)=[n-VFS_BASE]` 64 项函数指针；别名：`VFS_RMDIR→do_unlink`、`VFS_FCHMOD→do_chmod`、`VFS_FCHOWN→do_chown`、`VFS_SENDMSG/RECVMSG→do_sockmsg` |

### 1.4 真序表·核心数据结构与依赖（支撑 §4 编号设计的证据）

| 结构 | 锚点 | 锁 | 谁依赖谁 |
|------|------|----|----|
| `struct fproc` | `fproc.h`（全字段 ：15-82；标志 ：91-98；阻塞 union ：30-61；fproc_light :111-115） | `mutex_t fp_lock`（:71 一带，**普通互斥**） | 被 PM 握手填充（S07）；被 fork 整表复制（`misc.c:577-634`） |
| `worker_thread` | `threads.h:20-33`（w_tid/w_event_mutex/w_event/w_fp/w_m_in/w_m_out/w_sendrec/w_drv_sendrec/w_task/w_dmap/w_next） | mthread mutex+cond | `fproc.fp_worker` ↔ `w_fp` 双向绑定 |
| `tll_t` | `tll.h`（六字段 ：9-18）+ `tll.c`（READ/READSER/WRITE 三态、t_write 优先 t_serial 写偏序） | 自身即锁 | **被 vnode/vmnt 使用**（下两行）；其 EBUSY 排队依赖 worker 的等待原语（`tll.c` 调 `worker_wait`） |
| `struct filp` | `file.h`（`filp_count` :5 空闲哨兵；`mutex_t filp_lock` :14——**不用 tll**） | mutex + VFS 层 softlock | 被 fd 表（`fp_filp[]`）指向；指向 vnode |
| `struct vnode` | `vnode.h`（`v_ref_count` :13 / `v_fs_count` :14 / `tll_t v_lock` :22） | tll | 被 filp 指向；`v_vmnt` 指向 vmnt；256 阈值延迟同步 `vnode.c:246,263-264,305` |
| `struct vmnt` | `vmnt.h`（`tll_t m_lock` :9；`m_mount_path` :17 一带） | tll | 聚合 m_comm 窗口（`comm.c` 的 comm_t）；`m_mounted_on`/`m_root_node` 支撑挂载穿越 |
| `dmap`/`smap` | `dmap.h:16-25`（8 字段，NR_DEVICES=135 :82，CTTY 例外 :26）/`type.h:41-49`（smap 六字段） | lock_dmap 排他 | 启动期 S12 初始化；驱动死亡级联的触发源 |
| `file_lock` | `lock.h:7-13` + `glo.h:15` nr_locks | — | NR_LOCKS=8 槽（`const.h:6`） |
| `comm_t`（m_comm） | `type.h`（vmnt 内嵌，`comm.c:41` 使用） | — | `c_max_reqs/c_cur_reqs/c_req_queue` 窗口 |
| `glo.h` 全局 | `glo.h:13-34`（fp:13/susp_count:14/nr_locks:15/reviving:16/sending:17/verbose:18/ROOT_DEV:20/ROOT_FS_E:21/system_hz:22/m_in:25/self:34） | — | ARCH A-4 的聚合对象 |

### 1.5 真序表·协议面常量（wire 契约，绝对值必须 pin）

| 名字空间 | 基址 | 判别宏 | C 锚点 | 消费文档 |
|----------|------|--------|--------|---------|
| VFS 系统调用 | `VFS_BASE 0x100` | — | `callnr.h:68`，64 调用 `:72-135` | 09（分发表）、99 |
| PM 控制面 | `VFS_PM_RQ_BASE 0x900` / `VFS_PM_RS_BASE 0x980` | `& ~0x7f`（`com.h:516-517`） | 12 RQ `com.h:520-531`、11 RS `:534-544` | 01（INIT）、10 |
| VFS→FS 请求 | `FS_BASE 0xA00` | `& ~0xff` | `com.h:589`；REQ_* 33 常量 `vfsif.h:41-73`（REQ_GETNODE 死）；NREQS 34 `:75` | 11/12 |
| transid | `VFS_TRANSID = VFS_TRANSACTION_BASE+1`（0xB01） | TRNS_GET_ID/ADD_ID/DEL_ID `&0xFFFF`/`<<16` | `com.h:911`；`vfsif.h:79-81` | 09/11 |
| 驱动回复 | `CDEV_RS_BASE 0x480` / `BDEV_RS_BASE 0x580` / `SDEV_RS_BASE 0x1980` | `& ~0x7f` | `com.h:920/964/1038`，宏 `:923/967/1041` | 09/20/21/22 |

### 1.6 阶段内的运行时因果要点（教学序设计的依据）

1. **先表后服务**：全部核心表在 `sef_cb_init_fresh` 内初始化（S06–S15），主循环（L01 起）才消费它们——"数据模型先行、事件循环在后"既有运行时依据也有教学依据。
2. **worker 先于表初始化**：C 的 `worker_init`（S10，main.c:445）在四表初始化（S15，main.c:486-489）**之前**——现行文档把 worker 排在全部表之后（08 号）反而偏离了运行时序，这是本次重排的证据之一。
3. **tll 是 vnode/vmnt 的字段类型，不是 filp 的**：`vnode.h:22`/`vmnt.h:9` 是 `tll_t`，`file.h:14` 是 `mutex_t`。现行 plan.md D-1 声称"tll 是 filp/vnode/vmnt 锁的基础"对 filp 不成立；现行目录把 tll（07）排在 vnode（05）/vmnt（06）之后，读者在 05/06 遇到 `tll_t v_lock`、`VNODE_READ→TLL_*` 映射时无从理解——这是硬性前向引用（§3.1 判定）。
4. **挂载发生在启动期**：do_init_root（S17）先于一切请求（L01 之后才有用户请求），但它依赖路径解析与 REQ 协议（`mount_fs` 发 `req_readsuper`；`do_mount` 要 `eat_path` 解析挂载点）——教学序把 mount 机制放在路径解析之后是合理序差，必须入《序差表》（§4.3）。
5. **一次请求的生命周期**：L03 收 → L09 执行 →（挂起：SUSPEND 不回复，`main.c:297`）→（恢复：pipe revive / select_return / cdev_reply / sdev_reply / lock_revive 五条路径，或 L03 的复活优先）→ L10 回复。这条生命周期线是并行主题（64 调用）的统一框架。

---

## 2. 知识点全集

### 2.0 编制说明

- 编号 `K-001` 起连续编号；**对齐键 = 名称 + C 锚点**（多 AI 汇总时用）。
- 类型缩写：概=概念、机=机制、构=数据结构、协=接口与协议、约=约束与不变量、演=架构演进、工=工具与工程、测=测试性质。
- 来源类型只有存量（本表全部为存量；新增知识点在 §3.1 覆盖缺口表逐条落实并给证据锚点，不受去向规则约束）。
- "去向"列直接写**新编号**（映射见 §4.1 重排表）；"备注"列标记勘误（本报告核实旧文档有错）、重复与主讲述点。
- 读者收益列从略——每条的去向契约（§5）以"一句话定位 + 验收标准"承载同一信息，避免 300 行表格逐行重复。此为对骨架列的显式裁剪：知识点池的七列中"读者收益"一列由 §5 契约的定位行与验收行接管。

### 2.1 存量池总表

**00-vfs-overview（5 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-001 | VFS 三方裁判定位（POSIX 进程/FS 驱动/设备驱动的唯一共享词汇表） | 概 | 00 §1.1 | `servers/vfs/` 全域 | 00 | |
| K-002 | VFS 是 Minix3 唯一 mthread 服务器；minix-rs 单事件循环+9 请求槽的演进论证 | 演 | 00 §1.1 | `const.h:9`、`threads.h` | 00/04 | 主讲述点 04 |
| K-003 | 启动主线图（main→SEF→握手→init_*→根挂载→主循环） | 概 | 00 §1.2 | `main.c:54-143,374-527` | 00/01 | 与 01 重复，主讲述点 01；00 只留导航图 |
| K-004 | 服务面三数字（64 VFS 调用/12 VFS_PM/REQ 族） | 协 | 00 §1.3 | `callnr.h:68`、`com.h:513`、`vfsif.h:41-75` | 00/99 | **勘误**：00 写"33 个活 REQ"，真相 32 活+1 死（K-133） |
| K-005 | C 33 文件分组地图 + Rust 模块地图 | 工 | 00 §2/§4 | `Makefile:5-11` SRCS | 00 | 行数/测试数等快照数字一律去稳定化（§5.0 契约通用规则） |

**01-vfs-init-main（8 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-006 | 启动依赖链 11 站（fproc 清零→握手→hz→DS 订阅→worker→bsf→dmap/smap→rproctab→二遍 fproc→四表→根挂载） | 机 | 01 §1.2/§2.3 | `main.c:393-499` | 01 | |
| K-007 | VFS_PM_INIT 握手协议（逐条填槽+NONE 终止+OK 同步屏障；boot 进程 SYS_UID/umask=~0） | 协 | 01 §1.3/§2.3 | `main.c:410-436`；`com.h:520,547-551` | 01 | 协议归属 01，PM 侧对端归 `../04-stage-pm/01` |
| K-008 | SEF 生命周期六回调（fresh/restart/lu 三回调）与 LU 的 worker 收尾重建 | 机 | 01 §2.2/§2.4 | `main.c:303-390` | 01 | **勘误**：01/00 称"5 回调"，实注册 6 个 sef_setcb |
| K-009 | do_init_root 根挂载门控（worker_allow(FALSE)→mount_pfs→mount_fs→TRUE） | 机 | 01 §2.5 | `main.c:501-523` | 01 | 机制细节下放 19 |
| K-010 | lock_proc 可睡眠进程锁（trylock 快道+suspend 慢道） | 机 | 01 §2.6 | `main.c:528-553` | 01→99 | 有意省略（99 台账已有 lock_proc/unlock_proc 行）；01 保留 C 语义讲述 |
| K-011 | ARCH A-1/A-4/A-5 在启动链的落点（BootPhase 阶段枚举、门控字段、ReplyIntent 契约声明） | 演 | 01 §3.1/§3.4 | `os/servers/vfs/src/main_loop.rs` | 01/09 | |
| K-012 | VfsPmInit 类型化进 minix-types（两端单一事实源；decode 校验类型与槽界） | 演 | 01 §3.2/§4.2 | `minix-types/src/ipc/vfs.rs:187-231` | 01 | |
| K-013 | LuState/lu_prepare 槽模型落地（worker_cleanup/init 按构造为空操作；init_restart ≡ init_fresh） | 演 | 01 §3.1 | `main_loop.rs`（Fix #26） | 01 | |
| K-014 | Redox/Linux/seL4 服务注册与根挂载对照 | 概 | 01 §3.6 | （外部参照） | 01 | |

**02-fproc-struct（12 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-015 | 四张进程表投影模型（kernel proc/PM mproc/VFS fproc/VM vmproc，endpoint 关联） | 概 | 02 §1.1 | `fproc.h:11` 同界注释 | 03 | |
| K-016 | struct fproc 全字段分组（文件面/身份面/阻塞面/并发辅助面） | 构 | 02 §2.1 | `fproc.h:15-82` | 03 | |
| K-017 | fp_flags 六位（NOFLAGS/SRV_PROC/REVIVED/SESLDR/PENDING/EXITING/PM_WORK） | 构 | 02 §2.2 | `fproc.h:91-98` | 03 | |
| K-018 | fp_blocked_on 七态 + fp_u 五类 union（SELECT 无载荷） | 构 | 02 §2.3 | `const.h:19-28`、`fproc.h:30-61` | 03 | ARCH A-3 主落点 |
| K-019 | 凭证五字段+补充组+umask | 构 | 02 §2.4 | `fproc.h:63-69` | 03 | |
| K-020 | fp_lock 属槽不属进程；fp_worker/fp_func/fp_msg/fp_pm_msg 请求上下文 | 构 | 02 §2.5 | `fproc.h:71-75` | 03/04 | ARCH A-6 |
| K-021 | 生命周期：两遍初始化+fork 整表复制（锁保留、filp_count++、dup_vnode） | 机 | 02 §2.6 | `main.c:405-483`、`misc.c:577-634` | 03/10 | fork 协议主讲述点 10 |
| K-022 | BlockedOn 标签枚举建模（判别器+载荷绑定；PipeIo/FlockCmd/SdevCall/SdevAux） | 演 | 02 §3.1 | `fproc.rs:94` 一带 | 03 | |
| K-023 | 类型化字段映射（DevId/Mode/Pid/Uid/Gid；哨兵 vs Option 取舍） | 演 | 02 §3.2/§3.5/§3.6 | `fproc.rs` | 03 | A-8 |
| K-024 | fproc 轻表 fproc_light（MIB 投影三字段） | 构 | 02 §2.7 | `fproc.h:111-115`、`misc.c:81-97` | 04 | 主讲述点 04（03 篇），02 只留指针；**过时注释勘误**：fpl_task 实取自 cdev/smap 端点（misc.c:88-93） |
| K-025 | Linux fs_struct/files_struct/cred 分治 vs Minix3 平铺取舍 | 概 | 02 §1.3 | （外部参照） | 03 | |
| K-026 | fp_name/fp_tty（控制终端设备号） | 构 | 02 §2.1/§3.5 | `fproc.h:27,77` | 03 | |

**03-fproc-table（8 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-027 | fproc[NR_PROCS] 固定表与三服务同界（NR_PROCS=256） | 构 | 03 §1.1 | `fproc.h:11`、`glo.h:26-28` | 04 | |
| K-028 | PID_FREE/NONE 双哨兵空闲判据 | 约 | 03 §1.2 | `main.c:405-408` | 04 | |
| K-029 | isokendpt 三守卫与 okendpt 致命分化 | 机 | 03 §1.3/§2 | `utility.c:94-127` | 04 | |
| K-030 | fproc_addr/who_p 宏的 O(1) 双向映射 | 机 | 03 §1.1 | `glo.h:26-28` | 04/09 | who_e 语义主讲述点 09 |
| K-031 | endpoint 槽位编码与内核 proc 同下标约束 | 约 | 03 §1 | `fproc.h:11` 注释 | 04 | |
| K-032 | fproc_light 表操作（SI_PROCLIGHT_TAB 探测） | 机 | 03 §2 | `fproc.h:117-124`、`misc.c:81-97` | 04 | A-7 defer 标注 |
| K-033 | FProcTable Rust 建模（is_ok_endpoint 三守卫） | 演 | 03 §3/§4 | `fproc.rs` | 04 | |
| K-034 | NR_PROCS 槽号两侧一致约束（childno 双检） | 约 | 03 §2 | `misc.c:599-601` | 04/10 | 协议面主讲述点 10 |

**04-filp-table（10 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-035 | filp 中介与 fd→filp→vnode 二跳（fd 私有索引/filp 共享描述/vnode 全局投影） | 概 | 04 §1.1 | `file.h:5` | 06 | |
| K-036 | filp[NR_FILPS=1024] 与 filp_count==0 空闲哨兵 | 构 | 04 §1.2/§2 | `const.h:5`、`file.h:5` | 06 | |
| K-037 | fork/dup 共享分化（count 语义、filp_pos 共享偏移） | 机 | 04 §1.2/§2 | `misc.c:577-634`、`filedes.c` | 06/10 | fork 协议主讲述点 10 |
| K-038 | init_filps/get_filp/get_filp2/find_filp/find_filp_by_sock_dev | 机 | 04 §2 | `filedes.c:73-249` | 06 | |
| K-039 | get_filp2 的 FILP_CLOSED 门与 OPCL 特权（close 特权穿透） | 约 | 04 §2 | `filedes.c:186-199` | 06/14 | 与 14 重复：主讲述点 06（原语），14 讲 close(2) 组合 |
| K-040 | filp_lock/softlock/ioctl_fp 三态借用（FilpLockMode 三态化） | 机 | 04 §2/§3 | `filedes.c:313-352`、`filp.rs` | 06 | A-6；Fix #7 |
| K-041 | lock_filp FIFO 读升级写；check_filp_locks(_by_me) 调试族 | 机 | 04 §2 | `filedes.c:26-71,313` | 06/99 | 调试族入 99 省略台账 |
| K-042 | close_filp 三特殊分流（CHR/BLK/SOCK+FIFO+count 归零放 vnode） | 机 | 04 §2（14/15 亦述） | `filedes.c:413-519` | 06 | **重复**：主讲述点 06（本篇，filedes.c 原文归属）；15 留调用点 |
| K-043 | FSF_* 位集（select/驱动协同的可观测状态） | 构 | 04 §2 | `file.h` | 06/23 | select 字段语义主讲述点 23 |
| K-044 | alloc_filp 延迟置位契约（分配点 open.c:134 立即 count=1） | 约 | 04 §4/（Fix #6 测试踩坑注） | `open.c:134`、`filedes.c` | 06 | |

**05-vnode-table（10 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-045 | vnode 缓存定位（inode 投影；find_vnode(fs_e,ino) O(1024) 线性） | 概 | 05 §1.1 | `vnode.h`、`vnode.c:110` | 07 | |
| K-046 | vnode[NR_VNODES=1024] 与 ref==0 && !locked 双条件空闲 | 构 | 05 §1.2/§2 | `const.h:8`、`vnode.c:85` | 07 | |
| K-047 | v_ref_count/v_fs_count 双层引用与 256 阈值延迟同步 | 约 | 05 §1.2/标题 | `vnode.c:246,263-264,305` | 07 | 不变量汇总主讲述点 99 §3 |
| K-048 | dup_vnode/put_vnode 快慢路径（ref>1 快速；==1 走 req_putnode） | 机 | 05 §2 | `vnode.c:227-303` | 07 | |
| K-049 | lock_vnode/unlock/upgrade 与 VNODE_*→TLL_* 映射 | 机 | 05 §2/标题 | `vnode.c:156-225`、`vnode.h:22` | 07 | **重排依据**：tll 语义须先讲（新 05） |
| K-050 | init_vnodes/get_free_vnode/find_vnode/is_vnode_locked | 机 | 05 §2 | `vnode.c:85-155` | 07 | |
| K-051 | vnode_clean_refs（>256 降回 1） | 机 | 05 §2/标题 | `vnode.c:246,305` | 07 | 锚点已验证 |
| K-052 | vnode 七字段缓存填充（advance 未命中建表） | 构 | 05 §2/13 § | `vnode.h`、`path.c:40-127` | 07/13 | advance 主讲述点 13 |
| K-053 | VnodeTable Rust 建模（VnodeId 类型化、try_lock 抽象） | 演 | 05 §3/§4 | `vnode.rs` | 07 | |
| K-054 | 双 1024 缓存对偶（filp/vnode 同界） | 概 | 05 §1.1 | `const.h:5,8` | 07 | |

**06-vmnt-table（10 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-055 | vmnt 边界表定位（device→FS 映射；v_vmnt/v_dev 与穿越判定） | 概 | 06 §1.1 | `vmnt.h` | 08 | |
| K-056 | vmnt[NR_MNTS] 容量 | 构 | 06 标题/§1.1/§2 多处 | `const.h:7` | 08 | **勘误（P0 级）**：06 全篇写 8，C 真值 16（代码已修 Fix #33，文档未同步）；13 号"vmnt[8]/O(8)"同错 |
| K-057 | m_dev==NO_DEV 空闲哨兵与四表哨兵对比 | 约 | 06 §1.2 | `const.h:132` 一带 | 08 | |
| K-058 | get_free_vmnt/clear_vmnt 分配前清零 | 机 | 06 §1.2/§2 | `vmnt.c:65-95` | 08 | **勘误**：06 称"六字段清零"实列七项且漏 m_mount_path（`vmnt.h:17`） |
| K-059 | find_vmnt(fs_e,dev)；vmnt_unmap_by_endpt 四步级联 | 机 | 06 §2 | `vmnt.c:112-179,180` | 08 | |
| K-060 | VMNT_* 标志映射 TLL 三级锁；lock/unlock/downgrade/upgrade | 机 | 06 §2/§2.5 | `vmnt.h`、`vmnt.c:150-258` | 08 | 同 K-049 重排依据；Fix #20 |
| K-061 | m_flags READONLY/CALLBACK/MOUNTING 位集 | 构 | 06 §2 | `vmnt.h` | 08 | |
| K-062 | m_comm 窗口字段归属 vmnt | 构 | 06 §1.2（clear_vmnt 清单） | `vmnt.h`、`type.h` | 08/11 | 机制主讲述点 11 |
| K-063 | m_label/m_mount_path 与三名字拷贝 | 构 | 06 §1.1/（28 号 Fix #22） | `vmnt.h:17`、`stadir.c:283-285` | 08/28 | getvfsstat 名字主讲述点 28；**越界勘误**：06"不讲什么"称 fetch_vmnt_paths 归 18——实为 C 死代码入省略台账，与 18 无关 |
| K-064 | VmntTable Rust 建模（NR_MNTS 单真相 re-export） | 演 | 06 §3/§4 | `vmnt.rs:14`（=16） | 08 | Fix #33 |

**07-tll-lock（9 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-065 | tll_t 六字段（t_current/t_status/t_readonly/t_owner/t_write/t_serial） | 构 | 07 §1.2/§2 | `tll.h:9-18` | 05 | |
| K-066 | 三态语义：READ 多读者/READSER 串行读/WRITE 独占 | 概 | 07 §1.1/标题 | `tll.c:163,197,208` | 05 | |
| K-067 | 写偏序（t_write 队列优先于 t_serial 的选头唤醒） | 约 | 07 §1.1 | `tll.c:274` | 05 | |
| K-068 | tll_lock 五路分发与 tll_append 双队列尾插 | 机 | 07 §2 | `tll.c:139`、`:11` | 05 | |
| K-069 | tll_unlock 选头唤醒 + worker_wait 排队（阻塞原语依赖） | 机 | 07 §1.5/§2 | `tll.c:230-278` | 05 | **重排依据**：worker（新 04）须先于 tll |
| K-070 | tll_downgrade/upgrade 时序；TLL_NONE↔0 空锁不变式 | 机 | 07 §2 | `tll.c:74,306` | 05 | |
| K-071 | 锁状态正交（t_current/t_status/t_readonly） | 约 | 07 §1.2 | `tll.h` | 05 | |
| K-072 | tll.rs Rust 建模（Busy 拒绝升级硬化等） | 演 | 07 §3/§4 | `tll.rs` | 05 | |
| K-073 | LOCK_DEBUG cfg（A-9）与断言族省略 | 演 | 07 §3/99 台账 | `fproc.h:9` | 05/99 | |

**08-worker-thread（12 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-074 | 为什么只有 VFS 需要并发（五处必阻塞语义） | 概 | 08 §1.1 | `pipe.c/select.c/cdev.c/sdev.c/comm.c/lock.c` 各挂起点 | 04 | |
| K-075 | NR_WTHREADS=9 真实线程与 mthread 映射（threads.h 宏族） | 构 | 08 §1.1/§2 | `const.h:9`、`threads.h:1-17` | 04 | |
| K-076 | pending/busy/block_all 三计数与 worker_may_do_pending 三条件 | 机 | 08 §1.2 | `worker.c:10-12,147-156` | 04 | |
| K-077 | spare 线程保留不变量（needed=use_spare?1:2；死锁避免） | 约 | 08 §1.2 | `worker.c:147-151,331-339` | 04 | |
| K-078 | worker_allow 门控两阶段（关仅标记/开才释放） | 机 | 08 §1.3 | `worker.c:162-185` | 04 | |
| K-079 | w_fp↔fp_worker 双向绑定与 worker_can_start 四象限 | 机 | 08 §1.4 | `threads.h:27`、`worker.c:138,283-286,295-325` | 04 | |
| K-080 | suspend/resume 三件套协程语义；wait/signal 条件变量睡眠 | 机 | 08 §1.5 | `worker.c:474-520,443-468` | 04 | |
| K-081 | worker_start 四象限判定与 worker_yield/self 交出 | 机 | 08 §2 | `worker.c:360-440` | 04/09 | 与主循环的衔接主讲述点 09 |
| K-082 | worker_stop/stop_by_endpt（EIO 注入停工人）与 thread_cleanup/VMNT_CALLBACK 回收 | 机 | 08 §2 | `worker.c:535-570`、`main.c:557`一带 | 04/18 | 级联编排主讲述点 18 |
| K-083 | ARCH A-1 演进总论证（Linux workqueue/Redox async/seL4 endpoint 三方对照） | 演 | 08 §1.6/§3 | `worker.rs:1-33` | 04 | |
| K-084 | WorkerState 四态槽状态机与 WorkerFunc 五变体 | 演 | 08 §3/（todo Fix #37） | `worker.rs:51-88` | 04 | |
| K-085 | worker_set_proc 的 reboot 例外（"incredibly ugly"注释） | 约 | 08 §1.4 | `worker.c:586-606` | 04/10 | reboot 序列主讲述点 10 |

**09-main-loop（12 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-086 | 主循环心脏职责与阻塞不传染 | 概 | 09 §1.1 | `main.c:69-143` | 09 | |
| K-087 | get_work 复活优先 + ANY 接收 + endpoint 一致性校验 + TRUE/FALSE 双返回 | 机 | 09 §1.1/§2.4 | `main.c:580-633` | 09 | |
| K-088 | 八路优先序与编码不重叠论证 | 约 | 09 §1.2 | `main.c:80-138` | 09 | |
| K-089 | call_vec[64] 平表与四个别名（RMDIR/FCHMOD/FCHOWN/SENDMSG-RECVMSG） | 构 | 09 §1.3/§2.2 | `table.c:17-82` | 09 | 锚点已验证 |
| K-090 | do_work 的 call_index 分发与 SUSPEND 不回复 | 机 | 09 §1.4/§2 | `main.c:283-302` | 09 | `:286-290,:297` 已验证 |
| K-091 | reply/replycode 的 ipc_sendnb 非阻塞 | 机 | 09 §1.4 | `main.c:638,655` | 09 | |
| K-092 | unblock 复活分叉（PIPE→do_pending_pipe/FLOCK→重入） | 机 | 09 §2.5 | `main.c:921-973` | 09 | |
| K-093 | handle_work 死锁防护（CALLBACK/EAGAIN/spare） | 机 | 09 §1.2 | `main.c:146-184,160` | 09/11 | CALLBACK 语义主讲述点 11 |
| K-094 | glo.h 五宏六全局与 who_e 的 TLS 分叉 | 构 | 09 §1.5/§2.1 | `glo.h:13-44` | 09/99 | A-4 |
| K-095 | ARCH A-2：call_vec→VfsCallNum 枚举+from_raw 单真相（CallTable 已删） | 演 | 09 §1.3/（Fix #31） | `call_table.rs:28-93,200-202` | 09 | |
| K-096 | route_message 九变体 Route 与 RS 前缀真值（~0x7f/0x580/0x480/0x1980） | 演 | 09 §（Fix #27） | `main_loop.rs:143-164,564-616` | 09 | |
| K-097 | dispatch_syscall 64 臂穷举 + run_once + Route::Enosys（W3 完成态） | 演 | （todo Fix #37/#38；文档未记） | `syscalls.rs:24-208`、`main_loop.rs:623` | 09 | **新增缺口闭合**（N4） |

**10-pm-protocol（15 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-098 | VFS_PM 协议面总览（0x900 前缀守门；12 RQ/11 RS 与 m_type 域不重叠） | 协 | 10 §1/§2 | `com.h:513-544` | 10 | |
| K-099 | service_pm 三级调度（立即 7/延期 4/独立 1） | 机 | 10 §1.2 | `main.c:764-783` 一带 | 10 | |
| K-100 | PM_WORK/FP_PM_WORK 延期机制（EXEC/EXIT/DUMPCORE/UNPAUSE 四路） | 机 | 10 §1.2 | `worker.c:418,270` | 10/04 | worker 半归属 04 |
| K-101 | pm_fork 四步（守门→整表拷贝锁保留→filp_count++→dup_vnode+NOFLAGS） | 机 | 10 §2 | `misc.c:577-634` | 10 | fork 次主线主讲述点 |
| K-102 | PID_FREE 双检 childno（BogusChild/InUse） | 约 | 10 §2 | `misc.c:599-601` | 10 | |
| K-103 | free_proc 的 FP_EXITING 分水岭（Free vs Exiting 五级联） | 机 | 10 §2 | `misc.c:639-708` | 10 | |
| K-104 | pm_reboot 双轮释放八步序列（RebootStep） | 机 | 10 §（Fix #24） | `misc.c:500-572` | 10 | **勘误**：10 现文锚点"misc.c:503（工具生成锚 do_vm_call）"标签错位，应为 pm_reboot 函数体 |
| K-105 | 凭证单写族 setuid/setgid/setgroups/setsid（okendpt→slot→eff/real 双写；groups fail-closed） | 机 | 10 §2 | `misc.c:726-792` | 10 | |
| K-106 | SRV_FORK 追加凭证注入 | 协 | 10 §2 | `misc.c:868-870` | 10 | |
| K-107 | DUMPCORE csig==0→panic 约束 | 约 | 10 §2 | `misc.c:903` 一带 | 10/26 | 转储本体主讲述点 26 |
| K-108 | SESLDR tty 撤销级联 | 机 | 10 §2 | `misc.c:683` | 10/21 | cdev 侧对位 21 |
| K-109 | PmRequest/PmResponse 枚举（m7_i2 复用 alias 显式化） | 构 | 10 §3 | `com.h:520-544`、`ipc/dispatcher.rs` | 10 | |
| K-110 | PmHandler trait 面（11 VfsCall 变体；fetch_group_list ENOSYS 接缝） | 演 | 10 §3/§4（Fix #8） | `ipc/dispatcher.rs:85-116` | 10 | |
| K-111 | fproc_light 探测消费（do_getsysinfo SI_PROC_TAB） | 机 | 10 §2 | `misc.c:55,81-97` | 10/04 | |
| K-112 | NR_PROCS 同界跨端约束（Endpoint/UserSlot） | 约 | 10 §2 | `fproc.h:11`、`minix-types` | 10 | |

**11-fs-comm（14 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-113 | comm_t 窗口三字段与 0≤cur≤max 不变量 | 构 | 11 §1/§2 | `type.h`、`comm.c:41` | 11 | |
| K-114 | 每挂载点独立窗口（非全局队列） | 概 | 11 §1 | `vmnt` 内嵌 | 11 | NR_MNTS 引用须为 16 |
| K-115 | sendmsg 投递（cur++/transid/w_task/asynsend3） | 机 | 11 §2 | `comm.c:11-32` | 11 | |
| K-116 | do_reply 回收与 sendmsg 配对 | 机 | 11 §2 | `comm.c:209`一带、`main.c:81-95` | 11/09 | 路由主讲述点 09 |
| K-117 | VFS_TRANSID 编码与 TransIdCodec 单一事实源 | 协 | 11 §2 | `com.h:909-912`、`vfsif.h:79-81`、`fs_comm.rs:76-99` | 11 | Fix #13 收敛 |
| K-118 | queuemsg 尾插/fs_sendmore 头移（四守门） | 机 | 11 §2 | `comm.c:66-84,223-244` | 11 | |
| K-119 | send_work 全局扫表与 sending 计数不变量 | 机 | 11 §2 | `comm.c:37-45`、`glo.h:17` | 11 | |
| K-120 | fs_cancel 清空（worker_stop EIO 注入） | 机 | 11 §2 | `comm.c:50-61` | 11/04 | |
| K-121 | fs_sendrec 二守门（窗口+EIO/EDEADLK+ERESTART 抑制） | 机 | 11 §2 | `comm.c:134-168` | 11 | |
| K-122 | drv_sendrec dmap 排他（CTTY→EIO；块驱动窗口恒 1） | 机 | 11 §2 | `comm.c:89-129` | 11/18 | |
| K-123 | vm_sendrec/vm_vfs_procctl_handlemem | 机 | 11 §2 | `comm.c:173-218` | 11 | |
| K-124 | RES_THREADED 能力协商与窗口声明 | 约 | 11 §1/（18 号） | `mount.c:309-313` | 11/19 | 协商时机主讲述点 19 |
| K-125 | CALLBACK 抑制两处守门收敛 | 约 | 11 §2 | `comm.c:76`、`main.c:160` | 11 | |
| K-126 | GlobalComm/FsComm Rust 建模（Lifo/Fifo 策略、BlockingTransport） | 演 | 11 §3/§4 | `fs_comm.rs:133-200,355-449` | 11 | |

**12-request-wrappers（14 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-127 | FS_BASE 0xA00 与 IS_FS_RQ（~0xff）前缀 | 协 | 12 §2（Fix #1 后） | `com.h:589` | 12 | |
| K-128 | REQ 族计数真相（33 常量=32 活+1 死；NREQS 34 容量；36 个 req_* 函数） | 协 | 12 §2（Fix #2 后） | `vfsif.h:41-75` | 12/00 | **勘误主落点**：00 仍写"33 活" |
| K-129 | 32 活 REQ 类型谱系（BREAD..BPEEK）与 FsReq 枚举双射 | 构 | 12 §2/§3 | `vfsif.h:41-73`、`request.rs` | 12 | |
| K-130 | node_details 7 字段/lookup_res 9 字段类型化响应 | 构 | 12 §2 | `request.h:12,25` | 12 | |
| K-131 | CPF_TRY 二阶段重试（grant→send→revoke FAULTED→ERESTART→retry） | 机 | 12 §2 | `request.c:30-80` | 12 | |
| K-132 | req_lookup 的 PATH_GET_UCRED 凭证透传（双 grant） | 机 | 12 §2 | `request.c:424-495`、`vfsif.h:16` | 12 | |
| K-133 | REQ_GETNODE 死常量（"Should be removed"）与 decode→Unknown | 约 | 12 §2（D6） | `vfsif.h:41` | 12 | |
| K-134 | RES_64BIT INT_MAX 守门（32 位 FS 的 EINVAL 早拒） | 约 | 12 §2 | `request.c:274,323,856` | 12 | |
| K-135 | EENTERMOUNT/ELEAVEMOUNT/ESYMLINK 三特殊码（-301..-303） | 协 | 12 §2 | `vfsif.h` | 12/13 | 消费主讲述点 13 |
| K-136 | req_getdents direct/magic 分化；req_peek/bpeek grant=-1 直通 | 机 | 12 §2 | `request.c:288-343,902,89` | 12/16 | |
| K-137 | req_readsuper 能力回带（RES_* 与 fs_flags） | 机 | 12 §2 | `request.c:780-818` | 12/19 | mount 消费主讲述点 19 |
| K-138 | req_rename 双 grant；req_putnode 对端回收 | 机 | 12 §2 | `request.c:926-945,700-705` | 12 | |
| K-139 | REQ_SYNC 无包装（do_sync 直接调用） | 约 | 12 §2 | `misc.c:276`、`request.h` | 12/31 | |
| K-140 | E-REQWIRE 收敛后归属（常量上收 minix-types::fs_driver；minix-fs re-export） | 演 | （edge_todo；12 未记） | `minix-types/src/ipc/fs_driver.rs:21-127`、`minix-fs/src/protocol.rs:30,66` | 12/99 | **新增缺口闭合**（N5）；B 相按现状核对 request.rs 的消费形态 |

**13-path-lookup（14 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-141 | PATH_MAX 1024/NAME_MAX 60 与 TooLong 守门 | 约 | 13 §2 | `path.c:405`、syslimits | 13 | |
| K-142 | SYMLOOP=16 双守门与 ELOOP | 约 | 13 §2 | `const.h:32`、`path.c:468,349` | 13 | |
| K-143 | DO_POSIX_PATHNAME_RES=0 历史行为开关（A-10） | 约 | 13 §2（D5） | `path.c:23-35` | 13 | **勘误**：13 §5 仍列 test_slash_handler_two_impls，与 D5"虚构抽象已删"矛盾，重建时清理 |
| K-144 | lookup 结构 5 字段（l_path 可变缓冲+锁请求+输出指针）与 lookup_init | 构 | 13 §2 | `path.h`、`path.c:574-588` | 13 | |
| K-145 | advance 两相（get_free→req_lookup→find 命中 fs_count++/未命中建表；OPCL→READ 降级） | 机 | 13 §2 | `path.c:40-127` | 13 | |
| K-146 | eat_path 起点分化（/→rd 非/→wd） | 机 | 13 §2 | `path.c:133-140` | 13 | |
| K-147 | last_dir 切分+rdlink 重试+相对链接 loop_start 重启 | 机 | 13 §2 | `path.c:145-378,289-303` | 13 | |
| K-148 | lookup 三特殊码循环（READ↔WRITE 升降级；memmove 游标） | 机 | 13 §2 | `path.c:384-569` | 13 | Fix #35 |
| K-149 | EENTERMOUNT 穿越扫描与 ELEAVEMOUNT 伪路径守卫 | 机 | 13 §2 | `path.c:478`一带 | 13 | **勘误**：扫描上界应写 NR_MNTS=16（现文 vmnt[8]） |
| K-150 | copy_path/fetch_name 与 PathFetcher seam（Direct/Safecopy 二分） | 机 | 13 §2 | `utility.c:24-93`、`path.rs` | 13 | |
| K-151 | get_name dirent 扫描（EBADF/流尽 ENOENT） | 机 | 13 §2 | `path.c:593-642` | 13 | |
| K-152 | canonical_path（rdlink 展开+.. 爬升+fs 根跨出+真根到顶） | 机 | 13 §2 | `path.c:648-798` | 13 | Fix #36 |
| K-153 | chroot 边界与 ACCESS 凭证选择（real vs eff） | 约 | 13 §2 | `path.c:423-429` | 13 | |
| K-154 | do_socketpath 入口三门（SPATH 分类+路径长+super_user） | 机 | 13 §（Fix #25 边界注）/24 | `path.c:803-836` | 13/24 | 主讲述点 24（调用面），13 保留路径行走 |

**14-filedes（13 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-155 | fd 私有索引 vs filp 共享池（fork 共享 count 二次 close 才落 vnode） | 概 | 14 §1 | `filedes.c`、`file.h` | 14 | |
| K-156 | OPEN_MAX=255/NR_FILPS=1024 与 EMFILE/ENFILE 双耗尽管 | 约 | 14 §2 | `syslimits.h`、`const.h:5` | 14/99 | |
| K-157 | get_fd 最低空闲 + start 参数（F_DUPFD 下界）；NextFitDemo 仅 cfg(test) | 机 | 14 §2（D2） | `filedes.c:110-140` | 14 | Fix #14 |
| K-158 | check_fds nfds 窗口 | 机 | 14 §2 | `filedes.c:88-105` | 14/23 | select 消费 |
| K-159 | get_filp2 EBADF/EIO 双守门（OPCL 放行） | 约 | 14 §2（D4） | `filedes.c:186-199` | 14/06 | 原语主讲述点 06 |
| K-160 | FILP_CLOSED 只关语义与 invalidate 传播 | 约 | 14 §2（D6） | `filedes.c:250-308` | 14 | Fix #5/#6 |
| K-161 | invalidate 三变体（by_endpoint/by_char_major/by_sock_drv） | 机 | 14 §2 | `filedes.c:250-308` | 14 | 死亡级联的 filedes 面 |
| K-162 | find_filp/find_filp_by_sock_dev 反查 | 机 | 14 §2 | `filedes.c:205-245` | 14/17 | |
| K-163 | lock_filp softlock（suspend→lock→resume） | 机 | 14 §2 | `filedes.c:313-352` | 14/06 | 与 06 重复：机制主讲述点 06 |
| K-164 | close_fd 拆除序（NULL 先行→close_filp→FD_CLR→nr_locks 释放→lock_revive） | 机 | 14 §2 | `open.c:690-727` | 14/30 | 锁释放对位 30 |
| K-165 | do_copyfd 四 kind（From/To/Close/Cloexec）与三守门（EPERM/ioctl 持有者/EDEADLK） | 机 | 14 §2（D5） | `filedes.c:524-656` | 14 | Fix #10/R2-P0-2 |
| K-166 | cloexec 位图 | 构 | 14 §2 | `open.c:710`、`fproc` | 14 | |
| K-167 | CopyFdCtx 显式环境注入 | 演 | 14 §3 | `filedes.rs` | 14 | |

**15-open-close（14 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-168 | 名字与偏移分离两跳绑定 | 概 | 15 §1 | `open.c:133-135` | 15 | |
| K-169 | do_open/do_creat 对偶守门（O_CREAT 互斥） | 机 | 15 §2（D2） | `open.c:38-99` | 15 | |
| K-170 | mode_map 意图编码（O_ACCMODE 两比特第四槽双义） | 构 | 15 §2（D1） | `open.c:29,97-99` | 15 | |
| K-171 | common_open 七步管线（解码→预留→解析/创建→认领→权限→分派→提交/回滚） | 机 | 15 §2 | `open.c:83-293` | 15 | |
| K-172 | 六路类型分派（REG/DIR/CHR/BLK/FIFO/SOCK；Delegate 移交 20/21） | 机 | 15 §2（D6） | `open.c:148-274` | 15 | |
| K-173 | new_node 创建机（悬垂 symlink EEXIST 重解递归） | 机 | 15 §2（D3） | `open.c:299-477` | 15 | |
| K-174 | pipe_open 配对机（读写同开 ENXIO/无对端挂 POPEN） | 机 | 15 §2（D4） | `open.c:483-508` | 15/17 | |
| K-175 | do_mknod/do_mkdir（super 门/umask/PATH_RET_SYMLINK） | 机 | 15 §2 | `open.c:514-598` | 15 | |
| K-176 | actual_lseek（ESPIPE/EOVERFLOW/位置不变跳过 inhibread） | 机 | 15 §2（D5） | `open.c:603-669` | 15 | |
| K-177 | do_close 与 close_filp 调用点 | 机 | 15 §2（D7） | `open.c:674-727` | 15/14 | close_filp 机制主讲述点 06 |
| K-178 | forbidden 先行 + X_BIT 替换（for_exec 读让位执行） | 约 | 15 §2 | `open.c:146` | 15/29 | 判定主讲述点 29 |
| K-179 | O_TRUNC 先补 W_BIT；truncate_vnode 归 27 | 约 | 15 §2 | `open.c:151-156`、`link.c:366` | 15/27 | |
| K-180 | SUSPEND 豁免回滚 | 约 | 15 §2 | `open.c:282` | 15/09 | |
| K-181 | OpenError 13 变体→errno 无自创 | 约 | 15 §3/§4 | `open.rs` | 15 | |

**16-read-write（14 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-182 | 三向同机 READING/WRITING/PEEKING | 概 | 16 §1 | `read.c:150` | 16 | |
| K-183 | filp_pos 独享推进与 cum_io 同源 | 机 | 16 §2 | `read.c:145,262` | 16 | |
| K-184 | O_APPEND 起点重定 | 机 | 16 §2（D5） | `read.c:234` | 16/30 | SETFL 对位 30 |
| K-185 | 五路分派（FIFO/CHR/SOCK/BLK/REG）与 PEEK 三拒两放 | 机 | 16 §2（D4） | `read.c:135-251,155-238` | 16 | |
| K-186 | bsf 全局锁快慢道（trylock/worker_suspend）与 check_bsf_lock 卸载断言 | 机 | 16 §2（D2） | `read.c:49-87`、`glo.h:36`一带、`mount.c:576` | 16 | 主讲述点 16；20 明确不碰 |
| K-187 | FIXME 乐观推进（字符挂起 I/O 串行化代偿） | 约 | 16 §2 | `read.c:182-199` | 16 | |
| K-188 | 偷看不推进（bpeek/peek 无回写） | 机 | 16 §2 | `read.c:219-239` | 16/12 | |
| K-189 | 管道定量与存量（pipe_chunk/pipe_apply；position=0 未用） | 机 | 16 §2（D7） | `read.c:323-393` | 16/17 | pipe 语义主讲述点 17 |
| K-190 | PIPE_BUF=32768 原子阈（__minix） | 约 | 16 §2 | `syslimits.h` | 16/17 | |
| K-191 | 头校验三闩（锁分化+EBADF+零短路） | 机 | 16 §2（D3） | `read.c:101-116` | 16 | |
| K-192 | SIGPIPE 击发矩阵（EPIPE×写×非 O_NOSIGPIPE） | 机 | 16 §2（D6） | `read.c:264-271` | 16/17 | unpause 对位 17 |
| K-193 | NO_DEV panic→ENXIO 加固 | 演 | 16 §2（D4） | `read.c:168,208,214` | 16 | |
| K-194 | do_getdents 双 EBADF | 机 | 16 §2 | `read.c:282-317` | 16 | |
| K-195 | do_write 并入（write.c 25 行） | 机 | 16 §2 | `write.c:15` | 16 | |

**17-pipe（13 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-196 | 等待三处境（空管有写者等/无写者回零/写无读者 broken） | 概 | 17 §1 | `pipe.c:195-199` | 17 | |
| K-197 | v_size 存量语义（读写不经 filp_pos） | 概 | 17 §1 | `pipe.c:373-380` | 17 | |
| K-198 | pipe_check 读三路写五路 verdict | 机 | 17 §2（D1） | `pipe.c:187-288` | 17 | |
| K-199 | EAGAIN 快失败仍唤醒 | 约 | 17 §2 | `pipe.c:229-230` | 17 | |
| K-200 | susp_count/reviving 双账本（dec_checked 守卫） | 构 | 17 §2（D3） | `glo.h:14,16`、`pipe.c:304-306,459,518` | 17/09 | reviving 消费主讲述点 09 |
| K-201 | suspend/pipe_suspend 五参登记 | 机 | 17 §2 | `pipe.c:294-328` | 17 | |
| K-202 | release 两相扫描（select 相清位+proc 相六元合取） | 机 | 17 §2（D4） | `pipe.c:363-429` | 17/23 | |
| K-203 | revive 延迟标记（FP_REVIVED+reviving++；SDEV panic→EIO 加固） | 机 | 17 §2（D5） | `pipe.c:435-492` | 17 | |
| K-204 | unpause 中断六分支（有进展回数余 EINTR；socket 自回复例外） | 机 | 17 §2（D6） | `pipe.c:498-561,548-549` | 17 | **勘误落点**：22 §1.3 误引"21 §1.6"，自回复例外实为本锚点 |
| K-205 | create_pipe 七步回滚（义务随阶段单调增长） | 机 | 17 §2（D2） | `pipe.c:39-144` | 17 | |
| K-206 | map_vnode EBUSY 免解（配对义务唯一例外） | 约 | 17 §2（D7） | `pipe.c:151-182` | 17 | |
| K-207 | 命名/匿名统一（I_NAMED_PIPE 经 PFS 寄养） | 概 | 17 §1 | `pipe.c:101,172` | 17 | |
| K-208 | unsuspend_by_endpt 驱散分类（classify_driver_waiter） | 机 | 17 §2 | `pipe.c:335-357,347-350` | 17/18 | 编排主讲述点 18 |

**18-mount（原编号，14 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-209 | 嫁接模型（m_root_node/m_mounted_on 缝合；穿越消费对端） | 概 | 18 §1 | `mount.c` | 19 | |
| K-210 | major/minor/makedev 编解码（12+20 位入 64） | 构 | 18 §2（D1） | `types.h:290-295` | 19 | |
| K-211 | is_nonedev 三元合取与 nonedev 16 位图 | 构 | 18 §2（D2） | `dmap.h:17-18`、`mount.c:33-37,628-653` | 19 | |
| K-212 | mount_fs 五段提交与回滚边界 | 机 | 18 §2（D3） | `mount.c:156-385` | 19 | |
| K-213 | EBUSY 同码不同义（设备已嫁 vs 目录忙）；挂载点引用恰 1 | 约 | 18 §2（D4） | `mount.c:191-196,218` | 19 | |
| K-214 | RES_THREADED 线程协商（窗口=NR_WTHREADS 余 1） | 约 | 18 §2 | `mount.c:309-313` | 19/11 | |
| K-215 | have_root 两次根（RootStage 三态饱和；ramdisk→boot 盘） | 机 | 18 §2（D5） | `mount.c:31,205-209,318-349` | 19 | |
| K-216 | MAKEROOT 全员换家（ROOT_DEV/ROOT_FS_E 落定） | 机 | 18 §2 | `mount.c:318-349,327-328` | 19 | |
| K-217 | update_bspec 块改道（扫 vnode 改 v_bfs_e） | 机 | 18 §2 | `mount.c:46-80` | 19 | |
| K-218 | unmount 七步拆除（忙三元/PFS 无根例外/改道回根） | 机 | 18 §2（D6） | `mount.c:430-546` | 19 | |
| K-219 | unmount_all 扫荡（NR_MNTS 轮自外向内；残留加固） | 机 | 18 §2（D7） | `mount.c:552-585` | 19/10 | reboot 消费主讲述点 10 |
| K-220 | mount_pfs 罐装挂载（PfsMountPlan；失败仅打印不阻断） | 机 | 18 §2.7（Fix #25） | `mount.c:391-425` | 19/01 | 启动调用点主讲述点 01 |
| K-221 | name_to_dev 三分类（块直取/挂载根回退/ENOTBLK） | 机 | 18 §2 | `mount.c:590-622` | 19 | |
| K-222 | dmap 标签查询（mount 提交第 2 段查标签问驱动） | 机 | 18 §2.6 | `mount.c`+`dmap.c` | 19/18 | **重排依据**：device-map（新 18）须先于 mount（新 19） |

**19-device-map（原编号，13 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-223 | 号与人分离（major 不变端点可换；135 线性扫描够用） | 概 | 19 §1 | `dmap.c` | 18 | |
| K-224 | dmap 八字段与 NR_DEVICES=135、CTTY major 例外 | 构 | 19 §2 | `dmap.h:16-25,82,26`、`const.h:52` | 18 | |
| K-225 | lock_dmap 排他（挂起取锁与 bsf 同型） | 机 | 19 §2（D7） | `dmap.c:27-47` | 18 | |
| K-226 | map_driver 增删（NONE 即删并失效字符 filp） | 机 | 19 §2 | `dmap.c:61-104`一带 | 18/14 | |
| K-227 | do_mapdriver 双表提交（RS 门 EPERM；dmap 先行 smap 失败逆序回滚） | 机 | 19 §2（D2/D3） | `dmap.c:123-173` | 18 | |
| K-228 | map_service/map_driver 的启动期调用（rproctab 面） | 机 | 19 §2/01 §2.3 | `dmap.c:200`一带、`main.c:455-467` | 18/01 | 启动调用点主讲述点 01 |
| K-229 | dmap_endpt_up 恢复机（恢复中又坏停服/首坏 bdev_up/字符停工人+失效） | 机 | 19 §2（D4） | `dmap.c:275-309` | 18 | |
| K-230 | smap 六字段与行号<<32\|id 套接字号编码 | 构 | 19 §2 | `type.h:41-49`、`smap.c:200-208` | 18 | |
| K-231 | smap_map 注册机（NR_DOMAIN 门/同标签幂等/端点变才驱散） | 机 | 19 §2 | `smap.c:54-138` | 18 | |
| K-232 | smap 查询三函数 + smap_endpt_up 上线失效 | 机 | 19 §2 | `smap.c:148-273` | 18/14 | |
| K-233 | do_ioctl 三路分派（块置 filp_ioctl_fp 守卫）与 make_ioctl_grant 交叉解码 | 机 | 19 §2（D5/D6） | `device.c:18-95` | 18 | |
| K-234 | 驱动生死四态契约（映射→服务→消失→恢复的不对称） | 概 | 19 §1/§2.7 | `dmap.c/smap.c` | 18 | |
| K-235 | 死亡级联三面编排（classify_driver_waiter 分派 filedes 失效/sdev 停尸/select 唤醒；编排挂 W8） | 机 | （todo Fix #11 边界；19 文档部分） | `device_map.rs`、`pipe.rs`、`filedes.rs`、`sdev.rs` | 18 | **新增缺口闭合**（N11） |

**20-bdev（11 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-236 | 块直达管辖分叉（读写经 FS/开关控直达驱动） | 概 | 20 §1 | `bdev.c:1-9` | 20 | |
| K-237 | BDEV_RQ_BASE 0x500 选择子与 R/W_BIT | 协 | 20 §2 | `com.h:963-983` | 20 | |
| K-238 | 重试五次封顶（ERESTART 原报文重发；熔断 EIO） | 机 | 20 §2（D2） | `bdev.c:33-73` | 20 | |
| K-239 | 死信三分类（Dead 清表/Locked 记录/Fatal 加固） | 机 | 20 §2（D3） | `bdev.c:60-70` | 20 | |
| K-240 | bdev_reply 三重门与"Must not block"铁律 | 约 | 20 §2（D5） | `bdev.c:190-220` | 20 | |
| K-241 | bdev_up 换人两轮（filp 重开全弃 vs vmnt 通告继续的不对称；根兜底宁滥勿缺） | 机 | 20 §2（D6） | `bdev.c:226-282,277-281` | 20 | |
| K-242 | 重开四元合取 | 约 | 20 §2 | `bdev.c:243-259` | 20 | |
| K-243 | bdev_open/close 对偶（越界/缺席双门 ENXIO）与 bdev_ioctl 授权机 | 机 | 20 §2（D4） | `bdev.c:78-186` | 20 | |
| K-244 | bsf 边界声明（本文件不碰 bsf 锁） | 约 | 20 §1/§2 | `bdev.c:78` | 20/16 | |
| K-245 | 守卫义务复用（BLOCK_NEEDS_GUARD 单源） | 约 | 20 §2（D7） | `device.c:36-40` | 20/18 | |
| K-246 | Rust RetryState/ScriptedTransport trait 化 | 演 | 20 §3/§4 | `bdev.rs` | 20 | |

**21-cdev（12 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-247 | 字符挂起 vs 块同步（建连 vs 数据） | 概 | 21 §1 | `cdev.c:1-9` | 21 | |
| K-248 | cdev_map /dev/tty 改道（幂等单次；越界 NO_DEV） | 机 | 21 §2（D1） | `cdev.c:35-56` | 21 | |
| K-249 | cdev_get 三合一门（ENXIO vs EIO 差异在调用点） | 机 | 21 §2（D2） | `cdev.c:62-89` | 21 | |
| K-250 | cdev_clone 克隆换号（CLONED/PFS 临时节点/失败关新号） | 机 | 21 §2（D5） | `cdev.c:96-135` | 21 | |
| K-251 | cdev_opcl 开合机（CTTY 短路；AMF_NOREPLY 发+挂线程等复） | 机 | 21 §2（D3/D4） | `cdev.c:148-247` | 21 | |
| K-252 | 控制终端归属三条件（会话首领/无主/O_NOCTTY 缺席） | 约 | 21 §2 | `cdev.c:185-191` | 21/10 | setsid 对位 10 |
| K-253 | cdev_io 读写挂起（表丢 EIO；TIOCSCTTY FIXME 诚实保留） | 机 | 21 §2（D6） | `cdev.c:279-341` | 21 | |
| K-254 | 授权交叉复用（读配 CPF_WRITE 写配 CPF_READ，与 18 同源） | 约 | 21 §2 | `cdev.c:306-312` | 21/18 | |
| K-255 | cdev_cancel/cdev_reply 去程回程换码（EAGAIN↔EINTR） | 机 | 21 §2（D7） | `cdev.c:380-474` | 21 | |
| K-256 | cdev_select 旁路（发即返，回复走 23） | 机 | 21 §2 | `cdev.c:346-374` | 21/23 | |
| K-257 | CDEV_* 操作码族 | 协 | 21 §2 | `com.h:926-956` | 21 | |
| K-258 | tty_redirect/TtySource Rust 建模 | 演 | 21 §3/§4 | `cdev.rs` | 21 | |

**22-sdev（13 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-259 | 长短问三态（RoundTrip/Suspend/FireAndForget；非阻塞仍长问） | 概 | 22 §1（D1） | `sdev.c:8-16` | 22 | |
| K-260 | SDEV_RQ_BASE 0x1900/0x1980 与 17 操作号 | 协 | 22 §2 | `com.h:1037-1060` | 22 | |
| K-261 | sdev_suspend 三授权登记（数据/控制/地址按需） | 机 | 22 §2（D3） | `sdev.c:82-110` | 22 | |
| K-262 | 三授权配给与无条件撤销不变式 | 约 | 22 §2（D4） | `sdev.c:355-384,771-776` | 22 | |
| K-263 | 取消交叉语义（发取消信等原回答；驱动不认识的取消不回答） | 协 | 22 §2 | `sdev.c:29-35` | 22 | |
| K-264 | sdev_finish 三组分流（关闭归一 OK 除 EINPROGRESS） | 机 | 22 §2（D6） | `sdev.c:758-903` | 22 | |
| K-265 | sdev_reply 接受生线程（全 VFS 唯一"为回复而生线程"；双查防重） | 机 | 22 §2（D7） | `sdev.c:988-1112` | 22 | ARCH A-1 下的对照点 |
| K-266 | sdev_stop 死亡统一 EIO（短问收尸/长问停尸/复活验尸） | 机 | 22 §2（Fix #11） | `sdev.c:911-927` | 22/18 | |
| K-267 | sdev_socket 建字对偶坏关首号 | 机 | 22 §2 | `sdev.c:118-171` | 22 | |
| K-268 | sdev_close 双模式（SO_LINGER 注释；可挂则挂）与标志拼合 | 机 | 22 §2（D5） | `sdev.c:604-640,205,399-402` | 22 | |
| K-269 | sdev_select 发即返 | 机 | 22 §2 | `sdev.c:646-668` | 22/23 | |
| K-270 | fp_u 复用 TODO 与映射空页 TODO（诚实保留） | 约 | 22 §2 | `sdev.c:1094-1107,100-102` | 22 | |
| K-271 | **勘误**：22 §1.3"21 §1.6 的 sdev_cancel 自发回复"引文错位（sdev_cancel 属本篇；自回复例外在 17，`pipe.c:548-549`） | — | 22 §1.3 | `pipe.c:548-549` | 22 | 重建时修正引文 |

**23-select（17 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-272 | select"反向"群查询与中断式代价 | 概 | 23 §1 | `select.c:1-15` | 23 | |
| K-273 | 三问定去留（现在有吗/愿等吗/等多久；全否即 poll） | 概 | 23 §1 | `select.c:299-315` | 23 | |
| K-274 | fdtypes 四分型与未知类型 EBADF | 构 | 23 §2 | `select.c:85-90,368-404` | 23 | |
| K-275 | 两波回答与延期门（starting 旗或 UPDATE\|BUSY） | 机 | 23 §2 | `select.c:346-362` | 23 | |
| K-276 | 忙不重发护栏（FSF_BUSY 不发第二问） | 约 | 23 §2 | `select.c:453-454,509-519` | 23 | |
| K-277 | 挂起账本三层（MAXSELECTS 25 表+filp 五旗三数一设备+BlockedOn::Select 无载荷） | 构 | 23 §2 | `select.c:30-91`、`file.h:26-32`、`const.h` | 23 | |
| K-278 | 挂起即保活（selectors 计数配对） | 约 | 23 §1 | `select.c` 头注 | 23 | |
| K-279 | 超时三态（C 两布尔拼三态 vs Rust 枚举消灭第四态；TMRDIFF_MAX） | 构 | 23 §2 | `select.c:140-167,320-336` | 23 | |
| K-280 | 驱动死亡标就绪（RD\|WR 而非报错；与 22 EIO 口径对偶） | 机 | 23 §2 | `select.c:918-935` | 23/18 | |
| K-281 | select_filter 过滤机（非阻塞快路剪枝→置 UPDATE→忙则挂起；Query 义务字段） | 机 | 23 §2（Fix #9） | `select.c:406-457,517-522` | 23 | |
| K-282 | 管道试探拼合（pipe_check 探一字节+与原兴趣取交+暂存原兴趣） | 机 | 23 §2 | `select.c:577-616` | 23/17 | |
| K-283 | 位图双译（tab2ops/ops2tab 去重三条件+howmany） | 构 | 23 §2 | `select.c:621-708` | 23 | |
| K-284 | 取消机（cancel_all 交卷清账/cancel_filp 标 stale）与驱散机（进程退场整槽取消） | 机 | 23 §2 | `select.c:710-778,880-951` | 23 | |
| K-285 | 复活机（select_return/filp_status 扇出/restart_filps 只重发非 BUSY；管道同锁重入死锁排除） | 机 | 23 §2 | `select.c:780-816,1243-1313` | 23 | |
| K-286 | 超时机（钟早当 poll 办）与去留双门三处复用 | 机 | 23 §2 | `select.c:861-878,299,874,1311` | 23 | |
| K-287 | SelectDriver trait 化（一 trait 覆盖字符/套接字双路） | 工 | 23 §3/§4 | `select.rs:490-667` | 23 | |
| K-288 | 锁型（读兴趣共享读/写错兴趣独占写） | 构 | 23 §2 | `select.c:1337-1351` | 23 | |

**24-socket（17 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-289 | 上下两层分工（BSD 调用面通用/驱动面按域） | 概 | 24 §1 | `socket.c:1-7` | 24 | |
| K-290 | 十六种调用消息头表（SockCall+reply_layout 三列） | 协 | 24 §2 | `socket.c:9-32,77-124` | 24 | |
| K-291 | 创建三步检查（域检查→fd 预检→分配安装） | 机 | 24 §2 | `socket.c:172-218` | 24 | |
| K-292 | socketpair 双份复用 | 机 | 24 §2 | `socket.c:220-267` | 24 | |
| K-293 | 失败清理表（谁分配谁释放：fd VFS 关/驱动套接字 sdev_close） | 约 | 24 §2 | `socket.c:214-215,252-261,461-466` | 24 | |
| K-294 | SOCK 三旗换算（CLOEXEC/NONBLOCK/NOSIGPIPE→O_*；余位即类型本体静默忽略） | 构 | 24 §2 | `socket.c:40-57,149,165` | 24 | |
| K-295 | check_sock_fds 只查槽位（阻塞期间槽位只减不增） | 约 | 24 §2 | `socket.c:59-75` | 24 | |
| K-296 | make_sock_fd 九步（PFS 分配 S_IFSOCK\|0777+vnode 九字段+filp 三字段） | 机 | 24 §2 | `socket.c:77-170` | 24 | |
| K-297 | fd 有效性检查（EBADF/ENOTSOCK 分流；accept 恢复例外重查） | 约 | 24 §2 | `socket.c:276-302` | 24 | |
| K-298 | resume_accept 四分支（监听已关 EIO 不关新套接字/地址失败先关再返/成功掩码继承三标志） | 机 | 24 §2 | `socket.c:382-477` | 24 | |
| K-299 | recv 恢复 MUST NOT block（类型上保证：只取纯输入不取分配器） | 约 | 24 §2 | `socket.c:521-537,597-651` | 24 | |
| K-300 | msghdr 单项限制（iovlen>1 EMSGSIZE）与收发消息头不对称 | 约 | 24 §2 | `socket.c:566-574,589-594` | 24 | |
| K-301 | listen backlog 钳零与 shutdown 方向检查 | 机 | 24 §2 | `socket.c:355-356,758-759` | 24 | |
| K-302 | get/setsockopt 与 getsockname/getpeername（成功回写 len） | 机 | 24 §2 | `socket.c:653-741` | 24 | |
| K-303 | 未知调用号复用 VfsCallNum（single source 在 09） | 约 | 24 §3（D1） | `call_table.rs` | 24/09 | |
| K-304 | do_socketpath 上层入口（归 13 的路径行走+本篇的三门） | 机 | 24 §2/13 | `path.c:803-836` | 24/13 | |
| K-305 | compensate/CloseList 类型化清理 | 演 | 24 §3/§4 | `socket.rs:355-421` | 24 | |

**25-exec（16 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-306 | exec=地址空间替换（fd 表独留 CLOEXEC 除外；同步到底无续作） | 概 | 25 §1 | `exec.c:1-15` | 25 | |
| K-307 | 十一步管线与 vfs_exec_info 十字段 | 机 | 25 §2 | `exec.c:1-11,43-84` | 25 | |
| K-308 | get_read_vp 三项检查（REG→X 权限→stat；顺序即优先级） | 机 | 25 §2 | `exec.c:89-154` | 25 | |
| K-309 | sugid 提权只用一次（标志表 1,1→1,0→0,0；解释器不应用） | 约 | 25 §2 | `exec.c:137-147,254-313` | 25 | |
| K-310 | 脚本 #! 处理（双字节+换行门+patch_stack 换 argv[0]+逆向分词） | 机 | 25 §2 | `exec.c:519-602` | 25 | |
| K-311 | insert_arg 插参算术（对齐公式逐算符镜像；ARG_MAX 限高） | 机 | 25 §2 | `exec.c:604-683,226-227` | 25 | |
| K-312 | 动态链接双文件（主文件被读/解释器被运行；基址栈下 10MB） | 机 | 25 §2 | `exec.c:278-314` | 25 | |
| K-313 | 装载双路径（mmap 直映 PEEK+非 mem→vfs_memmap vs read_seg 分段） | 机 | 25 §2 | `exec.c:161-180,685-715` | 25 | |
| K-314 | exec_loaders 表驱动（首个 OK 即胜；全败 ENOEXEC） | 构 | 25 §2 | `exec.c:78-81,350-354` | 25 | |
| K-315 | stack_prepare_elf 辅向量（七个+AT_NULL 封口容溢皆封） | 构 | 25 §2 | `exec.c:404-517` | 25 | |
| K-316 | map_header 首块读（8 字节对齐之猜） | 机 | 25 §2 | `exec.c:736-763` | 25 | |
| K-317 | clo_exec 收尾扫描（(void) 忽略关闭错误：扫完不回滚） | 机 | 25 §2（Fix #21） | `exec.c:717-731` | 25 | |
| K-318 | 收尾三步（关 CLOEXEC→allow_setuid 更新 eff uid/gid→写进程名）+终局清理 | 机 | 25 §2 | `exec.c:358-402` | 25 | |
| K-319 | FAILCHECK→? 早返与栈参数限界（4MB） | 工 | 25 §2 | `exec.c:156-159,226-227` | 25 | |
| K-320 | VM 交互边界（mmap/装载/清零的执行归 `../02-stage-vm/20`；本篇只给标志转换与辅向量准备） | 约 | 25 §边界/§6 | `exec.c:161` | 25 | |
| K-321 | PM 通告边界（新生通告归 `../04-stage-pm/17`；本篇只给通告点） | 约 | 25 §边界 | `exec.c` | 25 | |

**26-coredump（14 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-322 | 转储只对已停止进程（代理原因；第一步 unpause） | 约 | 26 §1 | `misc.c:903-945` | 26 | |
| K-323 | 文件四部分固定序（ELF 头→程序头→注释段→段数据；偏移连续累加） | 构 | 26 §2 | `coredump.c` write_elf_core_file | 26 | |
| K-324 | 注释段双段结构（身份段+寄存器段；共享名 "MINIX-CORE"；衬垫计入长度） | 协 | 26 §2 | `coredump.c:38,120-156` | 26 | |
| K-325 | 段数据三策略（有页复制/缺页补零续写/超 LONG_MAX 截断记一次） | 机 | 26 §2 | `coredump.c` dump_segments | 26 | |
| K-326 | MAX_REGIONS 100 上限（告警即返非截断续写） | 约 | 26 §2 | `coredump.c:37` | 26 | |
| K-327 | 准备三步（unpause→core.<pid> 0777→复制进程名零终止；core_name 手工拼十进制） | 机 | 26 §2 | `misc.c:916-931` | 26 | |
| K-328 | 收尾归退出（fd 不在此关，随 free_proc(FP_EXITING) 统一回收） | 约 | 26 §2 | `misc.c:938-942` | 26/10 | |
| K-329 | fill_elf_header/fill_prog_header（ELFMAG；arch 相关作参数不硬编码） | 构 | 26 §2 | `coredump.c:80-116` | 26 | |
| K-330 | adjust_offsets 算写分离 | 机 | 26 §2 | `coredump.c` adjust_offsets | 26 | |
| K-331 | write_buf 与 fd=-1 TODO（常规文件永不挂起） | 约 | 26 §2 | `coredump.c:179-184` | 26 | |
| K-332 | get_memory_regions 游标批取（MAX_VRI_COUNT 批+负错误透传+零即结束） | 机 | 26 §2 | `coredump.c` get_memory_regions | 26 | |
| K-333 | dump_notes 五次写入同构 | 机 | 26 §2 | `coredump.c` dump_notes | 26 | |
| K-334 | 分块复制（CLICK_SIZE 步进 sys_datacopy_try） | 机 | 26 §2 | `coredump.c:311-315` | 26 | |
| K-335 | CoreWriter/RegionSource trait 化 | 工 | 26 §3/§4 | `coredump.rs:268-356,527-571` | 26 | |

**27-link（13 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-336 | 目录项与 inode 分离（删除只减计数归零才回收） | 概 | 27 §1 | `link.c:1-3` | 27 | |
| K-337 | 硬链接三项检查（存在/同设备 EXDEV/目标目录 W\|X） | 机 | 27 §2 | `link.c:29-86,298-338` | 27 | |
| K-338 | unlink 兼管 rmdir（共用解析/权限/粘滞位，按调用号分流） | 机 | 27 §2 | `link.c:91-164` | 27 | |
| K-339 | 粘滞位保护（EPERM；root SU_UID=0 例外；两处收敛一函数） | 约 | 27 §2 | `link.c:132-152,197-217` | 27 | |
| K-340 | rename 四项检查（旧父→粘滞→缓存旧名→新父；双目录 W\|X+同设备） | 机 | 27 §2 | `link.c:169-271` | 27 | |
| K-341 | 同目录自死锁断言 | 约 | 27 §2 | `link.c:240` | 27 | |
| K-342 | 截断同长优化（POSIX 保留时间戳）与 truncate_vnode 不比较长度 | 约 | 27 §2 | `link.c:312-313,348-381` | 27 | |
| K-343 | ftruncate 写模式 EBADF | 约 | 27 §2 | `link.c:346-347` | 27 | |
| K-344 | 符号链接创建（空路径 ENOENT/超长 ENAMETOOLONG/末尾 NUL 不写入） | 机 | 27 §2 | `link.c:387-424` | 27 | |
| K-345 | readlink 双入口（rdlink_direct 内核参数 vs do_rdlink 请求者参数；端点与标志必须配对） | 机 | 27 §2 | `link.c:430-508` | 27/13 | canonical_path 消费对位 13 |
| K-346 | LinkLookup 九种规格表（锁意图与锁状态分离） | 构 | 27 §2 | `link.c:87-131` | 27 | |
| K-347 | 借用即归还（释放锁并归还 vnode 引用） | 约 | 27 §2 | `link.c:79-84` | 27 | |
| K-348 | FsLink 七方法 trait 化 | 工 | 27 §3/§4 | `link.rs:319-421` | 27 | |

**28-stadir（14 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-349 | 工作目录=相对路径起点、根目录=.. 终止点 | 概 | 28 §1 | `stadir.c:117-135` | 28 | |
| K-350 | 切换三项检查（相同跳过→ENOTDIR→X 权限归 29） | 机 | 28 §2 | `stadir.c:121-128` | 28 | |
| K-351 | 引用替换顺序（先加新再放旧最后交换——顺序颠倒旧引用泄漏） | 约 | 28 §2 | `stadir.c:131-133` | 28 | |
| K-352 | fchdir/chdir 共用 change_into（手持 vnode vs 先解析） | 机 | 28 §2 | `stadir.c:32-78` | 28 | |
| K-353 | chroot 仅 root（EPERM 无例外；调用级检查与 vnode 级分层） | 约 | 28 §2（D3） | `stadir.c:83-94` | 28 | |
| K-354 | stat 双入口与 lstat 的 PATH_RET_SYMLINK 尾差 | 机 | 28 §2 | `stadir.c:140-192,418-446,433,155` | 28 | |
| K-355 | statvfs 实时与缓存（fresh 失败 EIO；ST_NOWAIT 先零填再复制） | 机 | 28 §2 | `stadir.c:244-274` | 28 | |
| K-356 | 只读标志叠加（ST_RDONLY 或入不覆盖） | 约 | 28 §2 | `stadir.c:276-277` | 28 | |
| K-357 | fsid 双格式（POSIX 与 NetBSD 各一份） | 机 | 28 §2 | `stadir.c:279-285` | 28 | |
| K-358 | 名字三拷贝（MountNames；fetch_vmnt_paths 判死代码入省略台账） | 构 | 28 §2（Fix #22） | `stadir.c:283-285`、`vmnt.c:246` | 28 | |
| K-359 | getvfsstat 遍历填充（无缓冲只计数/缓冲满即停/跳过不可上报/锁后复验） | 机 | 28 §2 | `stadir.c:351-413,388` | 28 | |
| K-360 | 头注释漏列 fchdir（以实现为准九个） | 约 | 28 §2 | `stadir.c:1-13,32` | 28 | |
| K-361 | update_statvfs 十七字段搬运 | 构 | 28 §2 | `stadir.c:197-229` | 28 | |
| K-362 | StatFs trait 化 | 工 | 28 §3/§4 | `stadir.rs:239-296` | 28 | |

**29-protect（13 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-363 | 九位三档查表（位移 6/3/0 取三位；"查表不是算术"） | 机 | 29 §2 | `protect.c:268-272` | 29 | |
| K-364 | 补充组并查（主组+补充组；in_group 未命中 EINVAL 视为不在组） | 机 | 29 §2 | `utility.c:128-141` | 29 | |
| K-365 | root 特权边界（默认全过/目录恒可查找/非目录须一位执行位） | 约 | 29 §2 | `protect.c:258-266` | 29 | |
| K-366 | 过期 id -1 直接 EACCES | 约 | 29 §2 | `protect.c:251` | 29 | |
| K-367 | real/eff 选择（access 用 real 其余用 eff） | 约 | 29 §2 | `protect.c:255-256` | 29 | |
| K-368 | chmod 属主检查（属主或 root→只读；属主在前）与 chown 三禁（转交）检查顺序相反 | 约 | 29 §2 | `protect.c:67-70,140-150` | 29 | |
| K-369 | setgid 清除（非属主组借组保留 setgid 是提权通道；模式位以 FS 返回值为准） | 约 | 29 §2 | `protect.c:72-81,78-80` | 29 | |
| K-370 | -1 哨兵 None 化与 id 越界（2^31-2 EINVAL） | 构 | 29 §2 | `protect.c:155-159` | 29 | |
| K-371 | umask 取反存储（存取皆取反；存返颠倒由测试锁定） | 机 | 29 §2 | `protect.c:182-192` | 29 | |
| K-372 | access 模式检查（R/W/X 外 EINVAL；F_OK 空集；同值不同域） | 约 | 29 §2 | `protect.c:217-218` | 29 | |
| K-373 | 只读检查在 forbidden 五段之末 | 约 | 29 §2 | `protect.c:282-302` | 29 | |
| K-374 | FS 返回值同步（uid/gid/模式位以 FS 返回为准本地不复算） | 约 | 29 §2 | `protect.c:160-165` | 29 | |
| K-375 | forbidden 判定核的消费者地图（open/stat/access/chdir 等） | 概 | 29 §1/15/25/28 | `protect.c:238`一带 | 29 | |

**30-fcntl-lock（16 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-376 | 十三命令多路复用（FcntlCmd；GETOWN 等无分支 EINVAL） | 构 | 30 §2 | `misc.c:135-267` | 30 | |
| K-377 | DUPFD 目标下限（arg∈[0,OPEN_MAX)；之上最低空闲；count++ 共享） | 约 | 30 §2 | `misc.c:139` | 30/14 | |
| K-378 | GETFL/SETFL 不对称（读三写二；ACCMODE 打开即定） | 约 | 30 §2 | `misc.c:165-175` | 30 | |
| K-379 | cloexec/哨兵双向（查一位设一位） | 机 | 30 §2 | `misc.c:150-163,238-246` | 30 | |
| K-380 | 劝告锁语义（只约束上锁不阻止读写；读读相容/读写互斥/同进程不冲突） | 概 | 30 §2 | `lock.c:69-99` | 30 | |
| K-381 | 固定八槽锁表（NR_LOCKS=8；表满 ENOLCK；指针判等译 VnodeKey 值判等） | 构 | 30 §2 | `lock.h:7-13`、`glo.h:15`、`const.h:6` | 30 | |
| K-382 | 区域计算（whence+start；checked_add 双向溢出拒；len=0 锁到 MAX_FILE_POS） | 机 | 30 §2 | `lock.c:51-67` | 30 | |
| K-383 | 五步类型权限检查（读锁 R_BIT 写锁 W_BIT——R_BIT 复用 29 同源两处） | 机 | 30 §2 | `lock.c:36-49` | 30/29 | |
| K-384 | SETLK vs SETLKW（EAGAIN vs 挂起存档三项；GETLK 不挂起只回填） | 机 | 30 §2 | `lock.c:83-99,133-152` | 30 | |
| K-385 | 查询回填（命中五项/未命中 F_UNLCK） | 机 | 30 §2 | `lock.c:133-152` | 30 | |
| K-386 | 解锁四分支（全清/头缩/尾缩/中裂需空槽） | 机 | 30 §2 | `lock.c:101-132` | 30 | |
| K-387 | 广播唤醒（遍历全表 FP_BLOCKED_ON_FLOCK；误唤醒经 unblock 重判无害） | 机 | 30 §2 | `lock.c:172-192`、`main.c:946-954` | 30/09 | |
| K-388 | close 时清锁（锁的终点不在解锁在 close；有清除则唤醒） | 约 | 30 §2 | `open.c:713-724` | 30/14 | |
| K-389 | FREESP 文件打洞（三钳制；零长截尾；req_ftrunc 下发） | 机 | 30 §2 | `misc.c:184-237` | 30 | |
| K-390 | FLUSH_FS_CACHE（root 门；块/常规两路 req_flush；C "Meaning unclear" 原样保留） | 机 | 30 §2 | `misc.c:247-264` | 30 | |
| K-391 | FlockWait 载荷（命令恒 F_SETLKW 不重存） | 构 | 30 §2 | `lock.c`、`fproc` | 30/03 | |

**31-misc-queries（12 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-392 | 杂项八类三分类（状态查询/落盘同步/请求应答） | 概 | 31 §1 | `misc.c` 全域 | 31 | |
| K-393 | do_getsysinfo 整表复制（root 先行+表长后置；轻量表现场填写） | 机 | 31 §2 | `misc.c:52-112,81-97` | 31 | |
| K-394 | sync 广播与 fsync 定点（三条件收窄） | 机 | 31 §2 | `misc.c:276-325` | 31 | |
| K-395 | dupvm 反向查询（peek+常规块检查；VM 表置位+count++） | 机 | 31 §2 | `misc.c:328-375` | 31 | 与 25 vmfd_gate 同 peek 不同次序不合并 |
| K-396 | vm_call 三种请求（FDLOOKUP/FDCLOSE/FDIO；非 VM ENOSYS；未知 panic→拒绝；VM_VFS_REPLY 0xC1E 异步恒 SUSPEND） | 协 | 31 §2 | `misc.c:380-498`、`com.h:702-707` | 31 | |
| K-397 | fd 查询应答两支（块设备无 inode 无界页 BLK_PAGES_UNBOUNDED 线值 pin） | 机 | 31 §2 | `misc.c:434-445` | 31 | |
| K-398 | svrctl F 族（设置仅 verbose；获取三键含 NR_WTHREADS-空闲） | 机 | 31 §2 | `misc.c:797-898` | 31 | |
| K-399 | utimens 双入口三态（NOFOLLOW/属主两段/NOW-OMIT-显值/只读最后） | 机 | 31 §2 | `time.c:26-155` | 31 | |
| K-400 | gcov_flush 五项检查（空标签拒绝=MINIX3 BUG 修复，模式 78） | 机 | 31 §2（Fix #12） | `gcov.c:10-73,31-44` | 31 | |
| K-401 | getrusage 废弃恒 OK（PM 接管 TODO） | 约 | 31 §2 | `misc.c:1000-1005`、`table.c` 注释 | 31 | |
| K-402 | ds_event 分类/门/分派（三前缀+DS_DRIVER_UP+块/字符/socket 分派；订阅循环挂 W5） | 机 | 31 §2（Fix #23） | `misc.c:948-986`、`ds.h:32` | 31/18 | |
| K-403 | 调用面复用 VfsCallNum（MiscCall 两处真源反例判定） | 约 | 31 §3（D1） | `table.c:34-66` | 31/09 | |

**99-global-concepts（12 条）**

| 编号 | 名称 | 类型 | 现有位置 | C 锚点 | 去向 | 备注 |
|------|------|------|---------|--------|------|------|
| K-404 | 容量常量族与机制依据（9 常量表含 NR_MNTS=16 勘误史） | 约 | 99 §1.1 | `const.h:5-12`、syslimits | 99 | |
| K-405 | 阻塞原因枚举类型化与死亡级联分流 | 演 | 99 §1.2 | `const.h:19-28`、`fproc.rs:94` | 99/03 | |
| K-406 | m_type 四名字空间不重叠（0x100/0xA00/0x900/0xB00；RS 三族 ~0x7f） | 协 | 99 §1.3 | `callnr.h`、`com.h` | 99 | |
| K-407 | glo.h 归属映射（A-4 拆进 VfsState） | 演 | 99 §2 | `glo.h:13-34` | 99 | |
| K-408 | 引用计数双层不变量（filp_count↔v_ref_count↔v_fs_count；失效族正确性基础） | 约 | 99 §3 | `file.h:5`、`vnode.h:13-14` | 99 | 主讲述点（04/05/06 引用） |
| K-409 | endpoint/transid 两条定位机制与 sys_datacopy_wrapper | 协 | 99 §4 | `utility.c:142-186`、`fs_comm.rs` | 99 | |
| K-410 | 类型映射 A-8 与 LOCK_DEBUG A-9 | 演 | 99 §4 | `minix-types` | 99 | |
| K-411 | 有意省略表（进程锁族/死锁断言族/select 清理族/死代码/panic_hook/LU 调用点 + 判别标准） | 约 | 99 台账 | 各 C 锚点 | 99 | |
| K-412 | 待接线边界提醒（决策已备只缺消息回路的判别标准） | 约 | 99 台账尾 | — | 99 | |
| K-413 | 不变量类测试落点导航 | 测 | 99 §5 | `call_table.rs:440`、`request.rs` 绝对值 pin | 99 | |
| K-414 | 术语速查（who_p/who_e/call_nr 宏） | 工 | 99 §4 | `glo.h:26-32` | 99 | |
| K-415 | minix-types 协议归属原则（"归属即依赖方向"） | 工 | 99 §1.1 尾 | `minix-types` | 99 | |

### 2.2 统计摘要

- 总条数 **415**；按类型：概 38、机 172、构 88、协 44、约 61、演 47、工 18、测 1（K-413）——按主类型计，一条多点时归主类型。
- 按现有文档分布：00:5、01:8、02:12、03:8、04:10、05:10、06:10、07:9、08:12、09:12、10:15、11:14、12:14、13:14、14:13、15:14、16:14、17:13、18:14、19:13、20:11、21:12、22:13、23:17、24:17、25:16、26:14、27:13、28:14、29:13、30:16、31:12、99:12。
- 重复标记 6 组（K-003、K-021、K-039、K-042、K-163、K-244 方向声明），主讲述点已逐一指明；勘误 9 处（K-004/K-008/K-024/K-056/K-058/K-063/K-104/K-143/K-149/K-271，其中 K-056 为 P0 级事实错误）。
- **新增知识点 12 条不在本表**：N1–N12 见 §3.1 覆盖缺口表，逐条带证据锚点。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

主题全集四路来源：① C 源码符号（33 .c + 15 .h，见 §1.4/§1.5 与 §2.1 逐条锚点）；② 操作系统通用概念（进程投影表、打开文件描述、inode 缓存、挂载边界、路径解析、多路复用、劝告锁、核心转储——已在各契约"讲什么"承载）；③ 非 C 制品主题（§3.4 逐项回答）；④ 阶段边界契约（plan.md §5 覆盖契约 + todo.md Fix 台账 + edge_todo.md 的 E-REQWIRE/E-VFSWIRE）。

与知识点池对账后得到三张表。

### 3.2 覆盖缺口表（新增知识点 N1–N12，全部落实去向）

| 编号 | 主题 | 为什么重要 | 证据锚点 | 去向 | 验收 |
|------|------|-----------|---------|------|------|
| N1 | 构建制品（Makefile：PROG=vfs、SRCS 33 文件、`-lmthread` 唯一链接依赖、MKCOVERAGE→gcov.c、minix.service.mk 安装） | VFS 是唯一链 mthread 的服务器，这一构建事实是 ARCH A-1 的物证；现在无任何文档讲它 | `minix3/minix/servers/vfs/Makefile:1-30` | 99 新增"工程与构建制品"小节 | 读者能回答"VFS 的构建有什么独有依赖、为什么" |
| N2 | ToErrno 统一映射通道（minix-types 的 trait + vfs 30 个错误枚举全部 impl；固有 to_errno 兼容通道） | 错误码纪律（errno 不自创）的 Rust 落地形态，Fix #19 后无文档记载 | `os/libs/minix-types/src/types/errno.rs:507`、`os/servers/vfs/src/` 30 处 `impl ToErrno` | 99 新增"错误映射通道"段 | 读者能说出"新错误枚举必须接入哪条通道" |
| N3 | 测试基建与替身模式（决策纯函数直测策略；Scripted*/trait seam 家族——SelectDriver/FsLink/StatFs/CoreWriter/ScriptedTransport/FakeFetcher；cfg(test) 纪律与"凡 cfg 门控修改必须 cargo check+test 双跑"教训） | 34 个模块 365 个 `#[test]` 的统一方法论散落各篇，无一处汇总 | `grep -c '#[test]' os/servers/vfs/src` = 365；`select.rs:490-667`、`link.rs:319-421` 等 | 99 新增"测试基建"小节；各篇测试表去数字 stabilization | 读者能复述"什么可测、用什么替身、纪律是什么" |
| N4 | dispatch_syscall 64 臂穷举 + run_once 唯一入口 + Route::Enosys（W3 完成态） | A-2 的完成形态（文档停在前半：CallTable 删除）；64 臂中 4 臂真处理器/60 臂 Nosys 的诚实契约 | `os/servers/vfs/src/syscalls.rs:24-208`、`main_loop.rs:623` | 09 | 读者能对照 C call_vec 说出 Rust 分发三层（from_raw→route→dispatch） |
| N5 | REQ 常量上收 minix-types::fs_driver（E-REQWIRE 收敛：FS_BASE 0xA00 单一事实源、minix-fs re-export、RequestNumber 33 变体联动） | wire 契约双侧收敛的终态；12 号文档的 Rust 归属声明过时 | `minix-types/src/ipc/fs_driver.rs:21-127`、`minix-fs/src/protocol.rs:30,66` | 12（主）+99 §1.3（一句） | 读者能说出"REQ 常量的家在哪、两端如何保证不漂移" |
| N6 | SEF 六回调完整清单（fresh/restart/lu_prepare/lu_state_changed/init_lu/lu_state_isvalid） | 现 00/01 写"5 个"，实测 `sef_local_startup` 有 6 个 sef_setcb | `main.c:374-390` | 01（勘误）+00（导航同步） | 计数与 `main.c` 逐行对得上 |
| N7 | REQ 计数口径勘误（33 常量=32 活+1 死；NREQS 34=表容量） | 00 写"33 个活 REQ"与 12 号/Fix #2 矛盾 | `vfsif.h:41-75` | 00（勘误）+12（主讲述点，已对） | 全 stage 无"33 活"残留 |
| N8 | NR_MNTS=16 勘误 | 06 号全篇写 8、13 号写"vmnt[8]"；C `const.h:7`=16，代码已修（Fix #33）文档未同步 | `const.h:7`、`vmnt.rs:14` | 08（重写勘误）+13（一句）+00/99（数字已对） | `grep -n 'NR_MNTS' *.md` 无 8 残留 |
| N9 | WorkerFunc 五变体与 WorkerSlot 字段面 | worker 槽状态机的 Rust 落地细节，08 号未记全 | `worker.rs:51-115` | 04 | 读者能画出槽五态迁移 |
| N10 | VfsState 15 字段组合完备（W2：七表聚合）+ run() 仍为 mock 的诚实边界 | A-4 的组合完成态与 W1 通电前的现状声明 | `main_loop.rs:275-312,797-828` | 09 +99 §2 | 读者能回答"七张表现在都挂在哪、什么还没通电" |
| N11 | 驱动死亡级联三面编排（filedes 失效族+sdev 停尸+select 唤醒共享同一触发事件；编排归 W8） | 决策函数已齐、编排视角无文档；这是失效族的"总开关"故事 | `filedes.rs` invalidate 族、`sdev.rs` StopPlan、`select.rs` DeathKind/unsuspend_hit、`pipe.rs` classify_driver_waiter | 18 新增"死亡级联编排"小节 | 读者能复述"驱动死了以后发生什么（三面）+ 什么还没接" |
| N12 | sdev 的 Rust 分工（调用方半边 228 行在 vfs，线协议半在 minix-sockdriver crate） | 22 号"Rust 模块"声明若照抄会误导（C 1114 行 ≠ Rust 228 行的差距原因） | `os/servers/vfs/src/sdev.rs:1-12` | 22 | 读者能回答"另一半在哪、契约面是什么" |

### 3.3 重复主题表（6 组，主讲述点裁决）

| 主题 | 出现位置 | 裁决 |
|------|---------|------|
| 启动主线图 | 00 §1.2 与 01 §1.2/§2.3 | 主讲述点 01；00 只留五站导航图+锚，不展开段落 |
| fork 复制（字段级/协议级） | 02 §2.6、04 §1.2、10 §2 | 协议与四步主讲述点 10；02 只讲字段生命周期、06（filp）只讲 count 语义，均一句话回指 10 |
| get_filp2 OPCL 门 | 06（filp 原语）与 14（close 组合） | 主讲述点 06（filp 表原语）；14 引用不重推门规则 |
| close_filp 三特殊分流 | 06（filedes.c 原文归属）、14 §2.7、15 §2（D7） | 主讲述点 06；15 保留 close(2) 调用点与拆除序，分流细节回指 |
| lock_filp softlock | 06 与 14 | 主讲述点 06；14 留"读写前取锁"调用点 |
| bsf 锁 | 16（全套机制）与 20（边界声明） | 维持现状：主讲述点 16，20 明确"不碰"（方向正确的正向依赖，不改） |

### 3.4 越界主题表

| 越界项 | 现状 | 裁决 |
|--------|------|------|
| 06 号"不讲什么"称 fetch_vmnt_paths 归 18 | 该函数是 C 死代码（`vmnt.c:246` 定义、`proto.h:371` 悬空声明、全树零调用），已入 99 省略台账；其行为真相（三名字拷贝）归 28 | 重建时 06 删除该条目；28 为主讲述点（Fix #22 已定） |
| 22 §1.3 引"21 §1.6 的 sdev_cancel 自发回复" | sdev_cancel 属 22 本篇；socket 自回复例外在 17（`pipe.c:548-549` unpause） | 22 重建时修正为"17 号 unpause 的 socket 自回复例外"（K-271） |
| 13 §5 残留 test_slash_handler_two_impls | 与本篇 D5"SlashHandler 已删"自相矛盾（Fix #32 后过时） | 13 重建时清理该行与相应叙述 |
| 10 §2 pm_reboot 锚点标签"misc.c:503（工具生成锚 do_vm_call）" | 标签错位；pm_reboot 体在 `misc.c:500-572` | 10 重建时修正锚点（K-104） |
| 24 号与 13 号的 do_socketpath 双写 | 24 管入口三门+调用面，13 管路径行走（Fix #25 已划界） | 维持划分，两边交叉引用措辞统一为"入口三门归 24、行走归 13" |
| PM 侧 fork 发送端/`tell_vfs` 状态机 | 属 `../04-stage-pm/05-vfs-interaction.md` | 本 stage 各篇只留对端锚，不展开（现状已守界，重申） |
| mproc/凭证语义 | 属 04-stage-pm | 02/03 号引用即可（现状已守界） |

### 3.5 非 C 主题逐项回答（固定清单）

| 主题 | 在哪里讲 / 为什么不在本 stage |
|------|------------------------------|
| 链接与加载 | 不在本 stage。VFS ELF 由 RS 运行时加载（`00-master-plan/README.md` 启动因果链），归 03-stage-rs 与 14-stage-runtime；本 stage 01 只讲"被加载后"的 SEF 起点 |
| 镜像与内存布局 | 不在本 stage（boot image 布局归 01-stage-kernel）；VFS 相关的仅有 boot 服务表 rproctab（01 §2.3 段 8 讲调用点，dmap 落点归 18） |
| 汇编入口与陷阱进入 | 不在本 stage（内核/运行时范围）。VFS 用户态服务器的 IPC 进入半归 E1 trap 桥（edge_todo E1）；本 stage 09 的 run() mock 现状即诚实边界（N10） |
| 启动装配 | 01（SEF 六回调+握手+根挂载门控）——已覆盖 |
| 构建与工具链 | **缺口已补**：99 新增"工程与构建制品"小节（N1，Makefile 锚点） |
| 跨模块接口与线格式 | 99 §1.3（四名字空间）+10（VFS_PM 面）+11/12（REQ/TRNS 面）+20/21/22（驱动 RS 面）+01（VfsPmInit 编解码）+12（N5 上收 minix-types）——已覆盖，N5 后为单一事实源 |
| 错误路径 | 各 syscall 族契约的"错误族→errno 无自创"约束 + 99 新增 ToErrno 通道（N2）——已覆盖 |
| 关闭与退出 | 10（pm_exit/free_proc/pm_reboot 八步）+19（unmount/unmount_all）+18（dmap/smap unmap 级联）+26（coredump 收尾归退出）——已覆盖；注意 `VFS_SHUTDOWN` 是 socket shutdown(2)（§0.4 第 4 条） |
| 并发与同步 | 04（worker 槽/阻塞原语）+05（tll）+09（事件循环/reviving）+99（A-4/A-6 单线程模型）——已覆盖 |
| 测试基建 | **缺口已补**：99 新增"测试基建"小节（N3） |

### 3.6 防漏声明（哪些面本轮"复核认定无缺口"）

依据 todo.md §9.3 的复核结论并经本轮抽验维持：device.c 决策层完整（ioctl 三函数已入 18 号契约 K-233）；12 个 VFS_PM 请求覆盖完整（ipc/dispatcher.rs 11 变体 + INIT 在 01）；64 调用号三层矩阵齐（枚举/路由/决策 64/64/62+2⚠，两个 ⚠ 已分别有 N4/GcovFlush 门/Fix #12 闭合记录）；FsReq 32 变体与活 REQ 双射（K-129）。时间/杂项文件（time.c/gcov.c/utility.c/device.c）全部有归属（31/31/04+13+29+99/18）。

---

## 4. 新目录

### 4.1 新篇章总表（34 篇：重编号 7 篇，其余保持；编号即阅读序）

**操作总集：重排（重编号）7 篇、拆分 0、合并 0、新建 0 篇新文件（新增内容以小节并入既有 6 篇）、归档 33（全部旧文件退居归档目录）。** 不拆不并的理由：33 篇全部落在 85–531 行的"单篇单语义"健康区间；§3.3 的 6 组重复均可用"主讲述点+回指"在原篇内解决，物理搬移只会引入新的断链而无信息增益。

| 新编号 | 新标题 | 一句话定位 | 分组 | 旧编号 |
|--------|--------|-----------|------|--------|
| 00 | VFS 整体架构概览 | 全文档导航：三方裁判定位、启动主线速览、三数字、ARCH 地图、双阅读路径 | 导航 | 00（不变） |
| 01 | 启动入口与初始化骨架 | VFS 如何按依赖序把自己初始化成"文件系统语义权威"（SEF 六回调/握手/init_* 调用点/根挂载门控） | 一、启动 | 01（不变） |
| 02 | fproc 结构与标志 | 进程的文件系统身份：能碰哪些文件/以什么身份/卡在哪 | 二、数据模型与并发 | 02（不变） |
| 03 | fproc 表与 isokendpt 三守卫 | 槽位映射的可信入口：双哨兵/三守卫/fproc_light | 二 | 03（不变） |
| 04 | worker 池：请求槽状态机 | 并发基础（一）：为什么必须能阻塞、9 槽如何隔离阻塞副作用 | 二 | **08** |
| 05 | tll 三级锁 | 并发基础（二）：READ/READSER/WRITE 与写偏序——vnode/vmnt 锁的原语 | 二 | **07** |
| 06 | filp 表 | fd→filp→vnode 二跳的中介层：count/哨兵/软锁 | 二 | **04** |
| 07 | vnode 表 | inode 投影缓存：双层引用与 256 阈值延迟同步 | 二 | **05** |
| 08 | vmnt 表 | 挂载边界表：NO_DEV 哨兵/m_comm 窗口归属/锁族 | 二 | **06** |
| 09 | 主循环与分发 | 运行时心脏：复活优先、八路优先、call_vec 64、Rust 三层分发 | 二 | 09（不变） |
| 10 | PM 协议 | 唯一非 syscall 入口：12 请求/三级调度/fork 次主线/free_proc 级联/重启序列 | 三、协议面 | 10（不变） |
| 11 | FS 通信原语 | 每挂载点窗口与 transid 路由：sendmsg/fs_sendmore/send_work/fs_sendrec | 三 | 11（不变） |
| 12 | REQ_* 包装 | VFS↔FS 的类型化协议面：32 活类型/CPF_TRY 重试/上收 minix-types | 三 | 12（不变） |
| 13 | 路径解析 | 所有名字类调用的前置：lookup 循环/三特殊码/last_dir/canonical_path | 四、名字与文件 | 13（不变） |
| 14 | 文件描述符表 | fd 的分配与拆除：get_fd/close_fd/copyfd/invalidate 失效族 | 四 | 14（不变） |
| 15 | open/close/lseek | 路径到 fd 的绑定与六路分派 | 四 | 15（不变） |
| 16 | read/write | 三向同机五路分派、bsf 块串行、位置推进 | 四 | 16（不变） |
| 17 | pipe | 配额定量、挂起账本、唤醒扫描、unpause 信号中断 | 四 | 17（不变） |
| 18 | device-map | 驱动通讯录（dmap/smap）+ioctl 分派+死亡级联编排 | 五、设备与挂载 | **19** |
| 19 | mount | 挂载的提交与拆除：五段提交/根换装/卸载扫荡 | 五 | **18** |
| 20 | bdev | 块驱动直达对话：重试熔断/死信分流/换人通告 | 五 | 20（不变） |
| 21 | cdev | 字符驱动对话：tty 改道/开合/挂起读写/取消换码 | 五 | 21（不变） |
| 22 | sdev | 套接字驱动对话：长短问/三授权/取消交叉/复活分流 | 五 | 22（不变） |
| 23 | select | 就绪多路复用：一问等一群、两波收场 | 六、多路复用与网络 | 23（不变） |
| 24 | socket | BSD 套接字调用面：检查/分配/清理/恢复 | 六 | 24（不变） |
| 25 | exec | 地址空间替换的 VFS 半：脚本/动态链接/装载/收尾 | 七、进程执行关联 | 25（不变） |
| 26 | coredump | ELF 核心转储：四部分固定序/缺页补零/收尾归退出 | 七 | 26（不变） |
| 27 | link | 目录项操作：硬链接/删除/重命名/截断/符号链接 | 八、名字空间元数据 | 27（不变） |
| 28 | stadir | 工作目录与查询：chdir/stat/statvfs/getvfsstat | 八 | 28（不变） |
| 29 | protect | 权限判定核：九位查表/chmod/chown/umask/access | 八 | 29（不变） |
| 30 | fcntl-lock | 十三命令与八槽劝告锁 | 九、控制与杂项 | 30（不变） |
| 31 | misc-queries | 杂项收束：getsysinfo/sync/vm_call/svrctl/utimens/gcov/ds_event | 九 | 31（不变） |
| 99 | VFS 全局概念 | 横向索引：常量/全局状态/引用计数不变量/术语/工程制品/测试基建/省略台账 | 收尾 | 99（不变） |

### 4.2 阅读路径与主线选择

- **主线（默认编号序）**：导航与启动（00–01）→ 数据模型与并发（02–09）→ 协议面（10–12）→ 名字与文件（13–17）→ 设备与挂载（18–22）→ 多路复用与网络（23–24）→ 进程执行（25–26）→ 元数据（27–29）→ 控制杂项（30–31）→ 收尾（99）。主线选择理由：数据模型（02–08）先于事件循环（09）既是运行时序（初始化先建表、worker 先于表初始化），又让 09 的八路分发每个分支都有已讲解的接收方；协议面（10–12）在文件操作（13–17）之前，因为后者的每一步都经 REQ 往返。
- **可跳读支线**：23/24（不关心多路复用与网络可跳）；25/26（不关心进程执行关联可跳）；30/31（按需查）。
- **启动链速读路径**：00 → 01 → 04（worker 门控）→ 09（主循环）→ 19（根挂载机制）→ 10（握手协议对端）。
- **一次请求生命周期路径**：09（收与分发）→ 13（解析）→ 06/07（表查找）→ 12（REQ 往返）→ 11（窗口排队）→ 09（回复或 SUSPEND）→ 17（以 pipe 为例的挂起/复活）。
- **99 的定位声明**（对硬标准 1 的显式解释）：99 是**查阅篇**，不属于线性阅读路径；所有编号文档在使用跨篇常量时就地给值+锚点，99 只做汇总与机制依据。因此"02 使用 OPEN_MAX 而 99 在最后"不构成前向引用——02 已就地给出 `OPEN_MAX=255` 与 syslimits 锚点。此解释适用于全 stage。
- **并行体的组织**（标准 5.2）：64 个 syscall 是并行体。统一框架篇 = 09（分派框架）+06/14（fd/filp 框架）+12（FS 对话框架）；按场景分组成篇 = §4.1 的九组；组内代表成员精讲（15 精讲 open、16 精讲 read、22 精讲 accept、24 精讲 socket 创建与 accept 恢复）+差异表（各篇"不讲什么"与同名 handler 别名表承担）。

### 4.3 序差表（运行时序 vs 教学序，硬标准 2 的法定记录）

| # | 运行时序事实（锚点） | 教学序选择 | 理由 | 回指补偿 |
|---|---------------------|-----------|------|---------|
| D1 | 根挂载在启动期（`main.c:501-523` do_init_root），先于一切请求 | mount 机制篇放 19（文件 I/O 族之后、设备篇之间） | do_mount 需要路径解析（13）与 REQ 协议（12）作前置；机制叙述依赖读者已会"名字如何解析" | 01 承载调用点与时序+门控；19 开篇回指 01 |
| D2 | init_dmap/init_smap 在启动期（`main.c:451-452`），先于根挂载 | device-map 篇放 18，紧邻 mount（19）之前 | 与运行时序一致（先设备表后挂载），且消除旧 18→19 的数据依赖前向（mount 查 dmap 标签，K-222） | 01 承载调用点 |
| D3 | init_select 在启动期（`main.c:488`） | select 篇放 23（cdev/sdev 之后） | select 机制依赖 filp 标志与字符/套接字两条驱动对话路 | 01 承载调用点；23 开篇回指 |
| D4 | worker_init 先于四表初始化（`main.c:445` vs `:486-489`） | worker 篇放 04、表篇 06–08 | 与运行时序一致；且 tll 的阻塞原语依赖 worker（K-069），worker 必须先讲——这是对现行目录（worker 在 08、全部表之后）的修正 | 01 的 init 序列图按真实顺序列出 |
| D5 | 四表初始化顺序 vnode→vmnt→select→filp（`main.c:486-489`） | 表篇顺序 filp(06)→vnode(07)→vmnt(08) | 四表初始化互不依赖（清零），教学序按数据流 fd→filp→vnode→vmnt 递进，每篇只依赖前一篇 | 01 的序列图保留 C 真实顺序 |
| D6 | VFS_PM_INIT 握手在启动期（`main.c:410-436`），先于主循环 | PM 协议篇放 10（主循环之后） | 12 请求协议面的理解需要 03（槽位）与 09（service_pm 分发点）先行 | 01 §1.3 承载握手协议本体；10 开篇回指 |
| D7 | tll 是 vnode/vmnt 的字段类型（`vnode.h:22`/`vmnt.h:9`），不是 filp 的（`file.h:14` 是 mutex） | tll 篇放 05，在 filp(06)/vnode(07)/vmnt(08) 之前 | 消除现行目录的硬性前向引用（05/06 号读者先遇 `tll_t`/`VNODE_*→TLL_*` 后遇 07 号解释）；filp 用普通互斥，不受影响 | filp 篇声明"本表不用 tll"以免读者误推 |

### 4.4 重排的形式验证（硬标准 5.4 预演）

- **前向引用扫描**：新序下逐篇"前置"只指向更小编号（§5 各契约已按此写）；仅存的跨编号引用全部是"声明边界/调用点导航"（§5.0 通用规则 3），概念与机制的首次讲解严格满足"首次出现即完整"。
- **依赖图无环**：以"前置"为边构图——00→{01..31,99}（导航边，不构成机制依赖）；01→{00}；02→{01}；03→{02,01}；04→{02,03}；05→{04}；06→{02,03}；07→{06,05}；08→{07,05,11?否}→{07,05}；09→{01,04,02,03}；10→{03,09,04}；11→{08,09,04,05→否}→{08,09,04}；12→{11,08,06,07}；13→{07,08,05,11,12}；14→{02,06,05,07}；15→{13,06,14}；16→{15,06,14}；17→{16,02,06}；18→{06→否}→{08,02,09,14}；19→{08,13,01,18,12}；20→{18,16,08}；21→{18,15,02}；22→{18,02,21}；23→{06,02,17,18,21,22}；24→{22,23,14,18}；25→{10,13,02,06}；26→{10,25,15,16}；27→{13,14,02,15}；28→{13,14,02,08}；29→{13,02,10}；30→{14,06,02,09}；31→{09,08,12,29}；99→{00}。逐边核对全部指向更小编号，**无环**。（13↔19 的消费/生产对偶按 D1 单向化：19 的前置含 13，13 的前置不含 19——穿越语义的承载者是 08 号 vmnt 表。）
- **覆盖率**：§2.1 的 415 条存量 + §3.2 的 12 条新增，每条在 §5 契约的知识点清单中有 K-/N- 编号——无"无去向"条目；明确删除项见 §3.4（5 处过时叙述删除，非知识点删除）。
- **断链成本**：§8 逐项列表（重编号 7 篇的入边 151 处文档引用 + 13 处代码注释 + plan/todo 处置），成本与收益在 §9 结论权衡。

---

## 5. 每篇契约

### 5.0 契约通用规则（B 相写作时对全部 34 篇生效，各契约不再重复）

1. **文风统一**：以 15–22 号新叙事风格为基准（导语定问题、正文按执行流展开、"与其他 OS 对照"收束 §1）；废除 03–09/10–14 的"本文讲清 A 如何在 B 约束下以 C 建立 D"长压缩句式（一句五 clause 的密度以损失可读性为代价）。
2. **头部统一**：每篇恢复四行结构化头部（定位/源码锚点/Rust 模块/边界摘要），但不引用任何中间产物目录（AGENTS.md 隐藏目录约定），不写"快照见某目录"类引用。
3. **前向引用纪律**：跨编号引用只允许两种形态——(a) 前置依赖（编号更小）；(b) 声明式边界指针（"X 的机制归 NN 号，本篇只……"）。禁止以未讲解的术语承载本篇核心不变量。
4. **数字去稳定化**：删除全部"截至日期 N passed/0 failed"累计计数、模块级测试数、C/Rust 全文件行数；测试小节只写"测试策略 + 关键测试名"；整 stage 的总量数字（C 行数、测试总数）只允许出现在 99 一处并带"截至日期"。
5. **勘误落地**：§2.1 备注列与 §3.2/§3.4 所列勘误，在对应契约中标注"重建时修正"，B 相必须逐条执行。
6. **锚点复核**：契约"事实底线"的 C 锚点为本蓝图验证值；B 相写作时按 fix-guard 重读目标行 ±5 行，行号漂移以 C 源码为准回写。

### 5.1 各篇契约

### 00-VFS 整体架构概览

- 一句话定位：回答"VFS 是什么、为什么是这个形状、整库怎么读"，是唯一不假设读者读过本 stage 其它篇章的入口。
- 讲什么：K-001、K-002（一段）、K-003（五站导航图）、K-004（勘误后口径）、K-005、ARCH 四条高层导航（A-1/A-2/A-4/A-5 各一句+落点编号）。
- 不讲什么：一切机制细节（各有其篇）；启动链分段叙述（→01）；测试数字（→99）。
- 前置：无（唯一零前置篇）。
- 后置：全部篇章的导航入口；99 的术语预告。
- 事实底线：`main.c:54,69-143`；`table.c:17-82`；`callnr.h:68`；`com.h:513,589`；`vfsif.h:41-75`；`os/servers/vfs/src/` 模块清单（34 文件）；`Makefile:1-30`。
- 知识点清单：见 §2.1 00 号 5 条（K-001..005），全部存量原位。
- 验收标准：①三数字与 §1.5 表逐值一致且含"32 活+1 死"勘误口径；②导航图中每一站都有锚点且与 01 的序列图一致；③读者只读本篇能说出 34 篇各自回答什么问题。

### 01-启动入口与初始化骨架

- 一句话定位：讲清 VFS 按什么依赖序初始化、谁来喂它初始状态（PM/RS）、为什么根挂载完成前拒绝服务。
- 讲什么：K-006..014；N6（六回调勘误）；SEF 生命周期与 LU 的槽模型（K-013）；握手协议本体（K-007）。
- 不讲什么：fproc 字段语义（→02）；worker 池调度（→04）；主循环分发（→09）；mount_fs 内部与 req_readsuper（→19/12）；dmap 细节（→18）；PM 侧对端状态机（→`../04-stage-pm/01`）。
- 前置：00；`../01-stage-kernel/06-proc-init-boot-proc.md`（boot image）；`../04-stage-pm/01-pm-init-main.md`（PM 侧握手对端，只作对照阅读）。
- 后置：02–09（初始化调用点被全部基础设施篇引用）；19（根挂载时序）；10（握手协议的 PM 面）。
- 事实底线：`main.c:54,69-141,303-390,393-499,501-553`；`worker.c:27,162`；`mount.c:391-431`（调用点）；`com.h:513-551`；`const.h:16-17`（SYS_UID）；`minix-types/src/ipc/vfs.rs:187-231`。
- 知识点清单：K-006..K-014 + N6，来源全部为存量 01 号原位（K-010 的省略判定已在 99 台账，正文保留 C 语义讲述）。
- 验收标准：①初始化 11 站序列与 `main.c:393-499` 逐行可对；②"5 回调"勘误落地（6 个 sef_setcb 逐一列出）；③读者能回答"为什么握手是被动接收、为什么挂载期间要门控"。

### 02-fproc 结构与标志

- 一句话定位：讲清 VFS 为每个进程记住什么、为什么必须私有记（最小知识原则）、阻塞状态为什么是"挂起—恢复"上下文。
- 讲什么：K-015..026 中归属本篇的字段与概念面（K-015/016/017/018/019/020/023/025/026）；fork 字段级生命周期一句话回指 10。
- 不讲什么：表操作与验证（→03）；fproc_light 表操作（→04）；filp/vnode/vmnt（→06..08）；PM 协议流程（→10）；各阻塞恢复路径（→17/21/22/23）。
- 前置：01（握手填的就是这些槽）。
- 后置：03/04（表与轻表）；06（fp_filp 指向 filp）；10（fork/exit 改这些字段）；29（凭证字段消费者）。
- 事实底线：`fproc.h:15-98,111-115`；`const.h:19-28`；`misc.c:577-634`（fork 字段级）；`fproc.rs`（BlockedOn/FpFlags）。
- 知识点清单：见 §2.1 02 号表，去向本篇的条目。
- 验收标准：①全字段表与 `fproc.h` 逐字段对得上（含 LOCK_DEBUG 调试字段）；②fp_u 五类 union 的载荷-挂起点-恢复路径三列矩阵完整；③读者能说出"四表靠 endpoint 对齐，谁在什么时机写每个字段"。

### 03-fproc 表与 isokendpt 三守卫

- 一句话定位：讲清"endpoint→fproc 槽"的映射为什么可信：同界约束、双哨兵、三守卫。
- 讲什么：K-027..031、K-033、K-034（槽号同界半）；K-024 归 04 的说明。
- 不讲什么：结构字段语义（→02）；轻表机制（→04）；endpoint 的内核生成与 generation（→`../01-stage-kernel/06`）。
- 前置：02、01。
- 后置：04/10（childno 双检）；09（get_work 的 endpoint 一致性校验消费本篇守卫）；全部 syscall 篇（isokendpt 是通用入口步）。
- 事实底线：`utility.c:94-127`；`fproc.h:11`；`glo.h:26-28`；`misc.c:599-601`。
- 知识点清单：§2.1 03 号表去向本篇条目。
- 验收标准：①三守卫逐条与 `isokendpt_f` 对应；②读者能解释"为什么用数组不用哈希、空闲为什么用双哨兵"。

### 04-worker 池：请求槽状态机

- 一句话定位：讲清 VFS 为什么是唯一必须"能阻塞"的服务器、9 个槽如何隔离阻塞副作用、门控如何挡住半初始化期的请求。
- 讲什么：K-074..085、N9。
- 不讲什么：主循环谁调 worker_start（→09）；tll 的 EBUSY 排队（→05，本篇只提供 wait/signal 原语）；各 SUSPEND 恢复路径（→17/21/22/23）；PM 延期的协议语义（→10，本篇只讲 FP_PM_WORK 的载体）。
- 前置：02（FP_* 标志与 BlockedOn）、03（表）。
- 后置：05（tll 排队用本篇原语）；09（分发落槽）；10（延期半）；16（bsf 慢道）。
- 事实底线：`const.h:9`；`threads.h:1-33`；`worker.c:10-12,27,109,119,138,147-156,162-185,239,295-325,331-360,431-520,535-606`；`worker.rs:42-344`。
- 知识点清单：§2.1 08 号表全部 12 条 + N9。
- 验收标准：①spare 保留不变量的两个反例（回调+FS 回调）都能讲出；②ARCH A-1 三方对照（Linux workqueue/Redox async/seL4）保留且锚到 `worker.rs:1-33`；③槽五态迁移图（Idle/Busy/WaitingForFs/Suspended + pending）与 `worker.rs` 对得上。

### 05-tll 三级锁

- 一句话定位：讲清 vnode/vmnt 的锁原语：三态语义、写偏序、升级降级时序——先于使用它的两篇出现。
- 讲什么：K-065..073。
- 不讲什么：lock_vnode/lock_vmnt 的使用方语义（→07/08）；LOCK_DEBUG 的省略判定细节（→99 台账）。
- 前置：04（worker_wait/signal 是 EBUSY 排队的底座）。
- 后置：07（lock_vnode）；08（lock_vmnt）；13（路径解析的锁升级降级）。
- 事实底线：`tll.h:9-18`；`tll.c:11,74,113,126,132,139,163-208,220,230-278,306`；`tll.rs`。
- 知识点清单：§2.1 07 号表全部 9 条。
- 验收标准：①READ/READSER/WRITE 三态与 `tll_lock` 五路分发逐支对上；②写偏序（t_write 优先 t_serial）能画出选头唤醒图；③读者能回答"为什么 vnode 要可升级读锁而 vmnt 要串行读"。

### 06-filp 表

- 一句话定位：讲清 fd→filp→vnode 二跳的中介层：count 语义、空闲哨兵、软锁三态——并声明本表**不用 tll**。
- 讲什么：K-035..044。
- 不讲什么：fd 表条目管理（→14）；select/pipe 对 FSF_* 的消费（→23/17）；close(2) 的调用序（→14/15）。
- 前置：02、03。
- 后置：07（filp→vnode）；14（fd 操作）；23（select 标志）。
- 事实底线：`file.h:5-32`；`filedes.c:26-71,73-249,313-430,413-519`；`open.c:134`（count=1 分配点）；`filp.rs`。
- 知识点清单：§2.1 04 号表全部 10 条（K-039/K-042 为本篇主讲述点并标注）。
- 验收标准：①"为什么需要中介"的 fork/dup 分化论证完整；②close_filp 三特殊分流图完整（K-042 主讲述点）；③头部声明"filp_lock 是 mutex_t 不是 tll_t"（file.h:14），消除 D-1 时代误解。

### 07-vnode 表

- 一句话定位：讲清 inode 投影缓存：双层引用计数、256 阈值延迟同步、可升级的 tll 读锁。
- 讲什么：K-045..054。
- 不讲什么：vmnt 表与 v_vmnt 穿越语义（→08/13）；req_putnode 的通信细节（→12）；路径解析对 vnode 的使用（→13）。
- 前置：06（filp 指向它）、05（tll）。
- 后置：08（v_vmnt 指向 vmnt）；13（advance 建/查缓存）；99（双层不变量汇总）。
- 事实底线：`vnode.h:13-22`；`vnode.c:85-155,156-225,227-303,246,263-264,305`；`vnode.rs`。
- 知识点清单：§2.1 05 号表全部 10 条。
- 验收标准：①put_vnode 快慢路径与 256 阈值时序图完整（锚点 `vnode.c:246,263-264,305` 已验证）；②读者能复述双层计数不变量并指出 99 是其汇总处。

### 08-vmnt 表

- 一句话定位：讲清挂载边界表：设备↔FS 映射、m_comm 窗口归属、锁族与端点回收。
- 讲什么：K-055..064；**N8 勘误（NR_MNTS=16）在本篇重点落地**。
- 不讲什么：挂载流程（→19）；m_comm 机制（→11）；fetch_vmnt_paths（删除——C 死代码，§3.4）；canonical_path（→13）。
- 前置：07（v_vmnt/mounted_on/root_node 指涉）、05（tll）。
- 后置：11（窗口）；13（穿越扫描消费者）；18（unmap 级联）；19（挂载提交）。
- 事实底线：`vmnt.h:9-24`（含 m_mount_path）；`vmnt.c:65-141,150-258,180`；`const.h:7`（=16）；`vmnt.rs:14`。
- 知识点清单：§2.1 06 号表全部 10 条（K-056/K-058/K-063 带勘误执行）。
- 验收标准：①全篇容量口径=16 且与 `const.h:7`、`vmnt.rs` 双锚；②clear_vmnt 清单与 `vmnt.c` 逐字段对（含 m_mount_path）；③四表哨兵对比表保留（fproc/filp/vnode/vmnt 各自的空闲判据）。

### 09-主循环与分发

- 一句话定位：讲清运行时心脏：复活优先、八路优先序为何不可换、call_vec 的 Rust 三层演进完成态。
- 讲什么：K-086..097；N4；N10。
- 不讲什么：service_pm 内容（→10）；m_comm 队列（→11）；三驱动 reply 内部（→20/21/22）；expire_timers 的 select 语义（→23）；do_pending_pipe 的 pipe 语义（→17）；ds_event 细节（→18/31）。
- 前置：01（门控）、04（槽）、02/03（进程与表）。
- 后置：10–31 全部（它们都是本篇分发的接收方）。
- 事实底线：`main.c:54-141,146-184,187,263-302,286-290,297,580-633,638,655,764,921-973`；`table.c:17-82`；`main_loop.rs:143-164,275-312,564-616,623,797-828`；`syscalls.rs:24-208`；`call_table.rs:16-93,200-202`。
- 知识点清单：§2.1 09 号表全部 12 条 + N4 + N10。
- 验收标准：①八路优先序与"交换两路会怎样"论证逐路保留；②Rust 分发三层图（from_raw→route_message→dispatch_syscall）与 C call_vec 对照表完整（含 4 真/60 Nosys 现状与 W1 通电契约）；③四个 handler 别名（RMDIR/FCHMOD/FCHOWN/SENDMSG-RECVMSG）明确列出。

### 10-PM 协议

- 一句话定位：讲清 VFS 唯一的非 syscall 入口：12 请求、三级调度、fork 次主线、退出与重启的级联。
- 讲什么：K-098..112。
- 不讲什么：syscall 面（→14–31）；m_comm（→11）；三驱动 reply（→20–22）；get_fd/close_fd 单步（→14）；dup_vnode/put_vnode（→07）。
- 前置：03（槽）、09（service_pm 分发点）、04（延期载体）。
- 后置：25（pm_exec 入口）；26（pm_dumpcore 入口）；14（free_proc 的 close 级联）；`../04-stage-pm/05-vfs-interaction.md`（对端）。
- 事实底线：`main.c:668-920`；`misc.c:500-572,577-1010`；`com.h:513-544`；`ipc/dispatcher.rs:85-239,388,416,561`。
- 知识点清单：§2.1 10 号表全部 15 条（K-104 带锚点修正执行）。
- 验收标准：①12 RQ/11 RS 与 `com.h:520-544` 逐值对；②三级调度表（立即 7/延期 4/独立 1）与 `service_pm` 分支对；③fork 四步路径图保留且每步带 misc.c 行号。

### 11-FS 通信原语

- 一句话定位：讲清 VFS↔FS 的窗口排队与 transid 路由：为什么每挂载点一个窗口、请求怎么排队与放行。
- 讲什么：K-113..126。
- 不讲什么：req_* 包装内容（→12）；具体调用链（→14–31）；asynsend3/safecopy 内核侧（→99+`../01-stage-kernel/12`）。
- 前置：08（窗口归属 vmnt）、09（send_work/do_reply 调用点）、04（EIO 注入停工人）。
- 后置：12（包装层）；19（RES_THREADED 协商）；16/17（fs_sendrec 的阻塞消费方）。
- 事实底线：`comm.c:11-244`；`type.h`（comm_t）；`com.h:909-912`；`vfsif.h:79-81`；`fs_comm.rs:39-200,355-449`。
- 知识点清单：§2.1 11 号表全部 14 条。
- 验收标准：①窗口不变量（0≤cur≤max）与四守门（NONE/空队/窗满/CALLBACK）完整；②transid 编解码用图讲清"高 16 位找槽、低 16 位是类型"；③NR_MNTS 口径=16。

### 12-REQ_* 包装

- 一句话定位：讲清 VFS↔FS 的类型化协议面：32 活类型、CPF_TRY 授权重试、类型化响应——以及常量上收 minix-types 后的单一事实源。
- 讲什么：K-127..140；N5。
- 不讲什么：窗口机制（→11）；FS 服务端实现（→15-stage-fs）；三特殊码的消费者（→13）；readsuper 的 vmnt 生命周期（→19）。
- 前置：11、08、06、07。
- 后置：13（req_lookup 消费）；15–17、19、27–31（各自 req_* 调用）；99（四名字空间）。
- 事实底线：`request.c:30-1213`；`vfsif.h:8-81`；`com.h:589`；`request.h:12,25`；`minix-types/src/ipc/fs_driver.rs:21-127`；`minix-fs/src/protocol.rs:30,66-136`；`request.rs`。
- 知识点清单：§2.1 12 号表全部 14 条 + N5。
- 验收标准：①"33 常量=32 活+1 死、NREQS=34 容量、36 个 req_* 函数"三重口径保留（K-128）；②CPF_TRY 时序图（grant→send→FAULTED→ERESTART→retry）完整；③绝对值 pin 测试（REQ_LOOKUP=0xA1A 等）作为验收锚点写入。

### 13-路径解析

- 一句话定位：讲清所有名字类调用的公共前置：从字符串到 vnode 的循环、跨挂载穿越、符号链接预算。
- 讲什么：K-141..154。
- 不讲什么：req_lookup 的 FS 端 grant（→12）；open 的创建分支（→15）；exec 的脚本解析（→25）；socketpath 入口三门归属 24、行走归本篇（§3.4 划界措辞统一）。
- 前置：07、08、05、11、12。
- 后置：15（open/creat）；19（do_mount 的挂载点解析）；25（exec 路径）；27/28（名字族）；24（socketpath 行走）。
- 事实底线：`path.c:23-35,40-127,133-140,145-378,384-569,478,574-642,648-798,803-836`；`path.h`；`utility.c:24-93`；`const.h:32`；`path.rs`（Fix #35/#36 后全函数化）。
- 知识点清单：§2.1 13 号表全部 14 条（K-143/K-149 带勘误执行：删 SlashHandler 残留行、NR_MNTS=16）。
- 验收标准：①三特殊码循环图（EENTERMOUNT 找行切根/ELEAVEMOUNT 伪路径/ESYMLINK 重启）完整且与 `Lookup::resolve` 对应；②SYMLOOP=16 双守门（lookup 与 last_dir）分别锚定；③13↔19 的消费/生产分工声明保留（穿越语义承载者是 vmnt 表）。

### 14-文件描述符表

- 一句话定位：讲清 fd 的分配、拆除、跨进程复制与失效传播——fd 生命周期全操作面。
- 讲什么：K-155..167。
- 不讲什么：filp 结构与锁原语（→06）；close_filp 三分流细节（→06 主讲述点）；PM fork 整表拷贝（→10）；select 的 filp 标志消费（→23）；记录锁释放对位（→30）。
- 前置：02、06、05、07。
- 后置：15（open/close 消费）；17（find_filp）；23（check_fds）；30（close 清锁）；18（失效族被级联调用）。
- 事实底线：`filedes.c:88-140,205-245,250-308,313-352,524-656`；`open.c:690-727,710`；`filedes.rs`。
- 知识点清单：§2.1 14 号表全部 13 条。
- 验收标准：①get_fd 的 start 参数化与 NextFitDemo 的"仅测试对照"声明保留；②copyfd 四 kind×三守门矩阵完整（方向由 kind 决定）；③失效三变体与"驱动死亡→谁调它"的指向（→18 级联编排）连通。

### 15-open/close/lseek

- 一句话定位：讲清路径到 fd 的绑定全流程：七步管线、六路类型分派、创建机与配对机。
- 讲什么：K-168..181。
- 不讲什么：路径穿越（→13）；fd 表管理（→14）；驱动打开的实现（→18/20/21）；forbidden 判定核（→29）；truncate_vnode（→27）；管道阻塞语义（→17）。
- 前置：13、06、14。
- 后置：16/17（I/O 与管道）；20/21（Delegate 目标）；29（权限）；27（O_TRUNC）。
- 事实底线：`open.c:29-99,83-293,133-135,146-156,148-274,282,299-477,483-508,514-598,603-727`；`open.rs`。
- 知识点清单：§2.1 15 号表全部 14 条。
- 验收标准：①七步管线与回滚边界图完整；②六路分派表含"Delegate 到哪篇"列；③mode_map 第四槽双义与 SUSPEND 豁免回滚两个反直觉点各有断言级讲述。

### 16-read/write

- 一句话定位：讲清三向同机的 I/O 管线：五路分派、bsf 块串行、位置推进与 SIGPIPE。
- 讲什么：K-182..195。
- 不讲什么：pipe 阻塞执行（→17）；驱动数据面（→20/21/22）；forbidden（→29）；req_readwrite 协议（→12）。
- 前置：15、06、14。
- 后置：17（rw_pipe 与 SIGPIPE 对位）；20（bsf 的边界声明对端）；12（req_breadwrite/peek）。
- 事实底线：`read.c:30-393`（关键：49-87,101-116,135-251,182-199,219-271,282-317,323-393）；`write.c:15`；`glo.h`（bsf_lock）；`read_write.rs`。
- 知识点清单：§2.1 16 号表全部 14 条。
- 验收标准：①五路分派+PEEK 三拒两放矩阵完整；②bsf 快慢道与 check_bsf_lock 断言保留（16 是主讲述点的地位由 20 号边界声明反证）；③FIXME 乐观推进的诚实注释保留。

### 17-pipe

- 一句话定位：讲清管道的定量、挂起账本与唤醒：VFS 挂起/复活机制的最完整样本。
- 讲什么：K-196..208。
- 不讲什么：req_newnode/readwrite（→12）；select 扇出（→23）；sdev/cdev 取消执行（→22/21，本篇只给驱散分类与自回复例外）；FLOCK 挂起（→30）。
- 前置：16、02、06。
- 后置：15（pipe_open）；23（select 试探与回调）；18（驱散分类的编排消费）。
- 事实底线：`pipe.c:39-144,151-182,187-288,229-230,294-328,304-306,335-357,347-350,363-429,373-380,435-492,459,498-561,518,548-549`；`glo.h:14,16`；`pipe.rs`。
- 知识点清单：§2.1 17 号表全部 13 条。
- 验收标准：①读三路写五路 verdict 矩阵完整；②susp_count/reviving 双账本的生产/消费配对图完整（reviving 消费在 09）；③22 号的历史错引（K-271）在本篇的 548-549 锚点处得到正名。

### 18-device-map

- 一句话定位：讲清驱动通讯录（dmap/smap）、ioctl 分派、以及"驱动死亡/复活"的三面级联编排总视图。
- 讲什么：K-223..235；N11。
- 不讲什么：驱动数据面对话（→20/21/22）；invalidate 执行（→14）；worker_stop（→04）；select 唤醒细节（→23）；grant 内核实现（→99）；DS 标签命名族（→16-stage-drivers，本篇只讲消费）。
- 前置：08、02、09、14。
- 后置：19（mount 查标签）；20/21/22（三族对话的表底座）；31（ds_event 分派执行）。
- 事实底线：`dmap.h:16-25,82,26`；`dmap.c:27-47,61-104,123-173,180,200,230-252,275-309,317`；`smap.c:22-273`；`device.c:18-95`；`const.h:52`；`device_map.rs`（906 行来源地图注释）。
- 知识点清单：§2.1 19 号表全部 13 条 + N11。
- 验收标准：①死亡级联三面图（filedes 失效/sdev 停尸/select 唤醒）共享同一触发事件且标注"编排归 W8"的现状；②套接字号编码（行号<<32|id）与 major/minor 编码不重叠论证完整；③"号与人分离"的四态契约（映射→服务→消失→恢复）保留。

### 19-mount

- 一句话定位：讲清挂载的提交与拆除：五段提交、根换装、卸载扫荡——启动期与运行期两条进入路径。
- 讲什么：K-209..222。
- 不讲什么：vmnt 槽存储与锁（→08）；req_readsuper/mountpoint/unmount 包装（→12）；dmap 表（→18）；块改道投递执行（→20）；statvfs（→28）；sys_datacopy（→99）。
- 前置：08、13、01、18、12。
- 后置：01（根挂载机制的归属篇）；20（v_bfs_e 改道终点）；28（三名字来源）；10（unmount_all 被 reboot 消费）。
- 事实底线：`mount.c:31,33-37,46-80,156-385,191-196,205-209,218,309-313,318-349,391-431,430-546,552-585,576,590-653`；`dmap.h:17-18`；`types.h:290-295`；`mount.rs`。
- 知识点清单：§2.1 18 号表全部 14 条。
- 验收标准：①五段提交的回滚边界图（"回滚边界=拿取边界"）完整；②have_root 三态与 MAKEROOT 换家时序保留；③开篇即回指 01 的启动时序（序差 D1 补偿）。

### 20-bdev

- 一句话定位：讲清 VFS 直达块驱动的窄对话面：开/关/控、重试熔断、死信分流、换人通告。
- 讲什么：K-236..246。
- 不讲什么：req_breadwrite（→12）；dmap 表（→18）；bsf 锁（→16，边界声明保留）；worker_signal（→04）；grant 内核（→99）。
- 前置：18、16、08。
- 后置：19（bdev_up 通告对端）；18（恢复机调用本篇）。
- 事实底线：`bdev.c:33-73,78-186,190-220,226-282`；`com.h:963-983`；`bdev.rs`。
- 知识点清单：§2.1 20 号表全部 11 条。
- 验收标准：①重试五次+死信三分类状态机图完整；②bdev_reply"Must not block"铁律与三重门保留；③与 21（可挂起）的对比表（建连 vs 数据）保留。

### 21-cdev

- 一句话定位：讲清字符驱动对话：tty 改道、克隆换号、开合对话、可挂起的读写与取消/回程换码。
- 讲什么：K-247..258。
- 不讲什么：dmap 表（→18）；asynsend3 内核（→99）；req_newnode（→12）；select 回复执行（→23）；终端行规程（→16-stage-drivers/tty）。
- 前置：18、15、02。
- 后置：23（SEL1/SEL2 回复）；10（SESLDR 撤销对位）。
- 事实底线：`cdev.c:35-56,62-89,96-135,148-247,185-191,279-341,306-312,346-374,380-474`；`com.h:920-956`；`cdev.rs`。
- 知识点清单：§2.1 21 号表全部 12 条。
- 验收标准：①cdev_opcl 开合时序（AMF_NOREPLY 发+挂线程等复）与三条件归属判定完整；②EAGAIN↔EINTR 去程/回程换码对照表完整；③授权交叉复用与 18 号同源的声明保留。

### 22-sdev

- 一句话定位：讲清套接字驱动对话：长短问三态、三授权挂起登记、取消交叉、复活分流——并声明 Rust 侧的分工（线协议半在 minix-sockdriver）。
- 讲什么：K-259..271；N12。
- 不讲什么：smap 表（→18）；socket 调用面与 resume_*（→24）；SEL 回复执行（→23）；grant 内核（→99）。
- 前置：18、02、21。
- 后置：24（上层续作）；23（sdev 侧回复）；18（死亡级联的 sdev 面）。
- 事实底线：`sdev.c:8-16,29-35,59-110,118-171,205,220-640,647-668,758-1112`（关键：82-110,355-384,604-640,758-927,911-927,988-1112）；`com.h:1037-1060`；`sdev.rs:1-12`；minix-sockdriver crate（分工声明）。
- 知识点清单：§2.1 22 号表全部 13 条（K-271 勘误执行：引文改指 17 号 `pipe.c:548-549`）。
- 验收标准：①长短问三态与"非阻塞仍长问"的反直觉点保留；②三授权"全置才可无条件撤销"不变式有断言级讲述；③N12 分工声明在头部与 §3 各出现一次。

### 23-select

- 一句话定位：讲清就绪多路复用：一问等一群、两波才收场、挂起账本与超时。
- 讲什么：K-272..288。
- 不讲什么：pipe_check 试探执行（→17）；cdev/sdev 投递执行（→21/22）；suspend/revive/set_timer 等待执行（→04/09）；fd_set 用户拷贝（→`../01-stage-kernel/18`）；socket 调用面（→24）。
- 前置：06、02、17、18、21、22。
- 后置：24（套接字侧登记）；31（svrctl 的 select 统计键引用本篇机制）。
- 事实底线：`select.c:30-91,96-167,299-457,462-616,621-778,780-951,1243-1313,1337-1351`；`file.h:26-32`；`const.h`（MAXSELECTS 25 于 `select.c:31`）；`select.rs`。
- 知识点清单：§2.1 23 号表全部 17 条。
- 验收标准：①两波回答+延期门的时序图完整；②三层账本（选择表/filp 字段/BlockedOn）配对讲述（每笔登记配一笔注销）；③驱动死亡"标就绪而非报错"与 22 的 EIO 口径对偶表保留。

### 24-socket

- 一句话定位：讲清 BSD 套接字调用面：协议域检查、fd 分配安装、失败清理表与阻塞恢复的类型化保证。
- 讲什么：K-289..305。
- 不讲什么：sdev 驱动对话（→22）；选择登记（→23）；check_fds/get_fd/close_fd（→14）；req_newnode（→12）；socketpath 行走（→13，入口三门归本篇）。
- 前置：22、23、14、18。
- 后置：13（socketpath 行走的调用方）；25（无——exec 不依赖 24，删除旧文此向导句）。
- 事实底线：`socket.c:1-32,40-124,149-302,308-477,483-651,653-759,803-836`；`call_table.rs`（D1 复用声明）；`socket.rs`。
- 知识点清单：§2.1 24 号表全部 17 条。
- 验收标准：①十六调用三列头表（调用号/请求布局/回执布局）完整；②"恢复函数类型上不阻塞"的论证（只取纯输入）保留；③失败清理表"谁分配谁释放"两行规则有测试名支撑。

### 25-exec

- 一句话定位：讲清 exec 在 VFS 的半场：验权、脚本与动态链接、装载参数准备、地址空间替换的收尾。
- 讲什么：K-306..321。
- 不讲什么：mmap/装载/清零的 VM 执行（→`../02-stage-vm/20-vm-mmap.md`）；PM 侧新生通告（→`../04-stage-pm/17-exec.md`）；路径解析（→13）；forbidden（→29）；fd 表（→14）。
- 前置：10、13、02、06。
- 后置：26（coredump 消费 exec 的地址空间视图概念）。
- 事实底线：`exec.c:1-180,185-402,404-517,519-763`（关键：43-84,89-154,137-147,161-180,278-354,358-402,519-683,685-763）；`exec.rs`。
- 知识点清单：§2.1 25 号表全部 16 条。
- 验收标准：①十一步管线图与 FAILCHECK 早返路径完整；②sugid"只用一次"标志表（1,1→1,0→0,0）与两个解释器反例保留；③跨阶段边界（VM/PM）的职责切分表保留。

### 26-coredump

- 一句话定位：讲清 ELF 核心转储的全流程：以终止进程之名写四部分文件，缺页补零也要写完整。
- 讲什么：K-322..335。
- 不讲什么：VM 区表查询执行（→VM 侧）；寄存器读取与内存拷贝（→内核侧）；FS 直写（→16）；open 执行（→15）；free_proc（→10）；信号语义（→`../04-stage-pm/11-13`）。
- 前置：10、25、15、16。
- 后置：无（27 起的"下一站"指针仅导航）。
- 事实底线：`coredump.c:32-327`；`misc.c:903-945,916-942`；`coredump.rs`。
- 知识点清单：§2.1 26 号表全部 14 条。
- 验收标准：①四部分固定序与偏移累加图完整；②段数据三策略表（复制/补零续写/截断记一次）完整；③"缺页补零并继续"标注为 P0 级行为偏移红线（遇缺即错是错的）。

### 27-link

- 一句话定位：讲清目录项操作族：硬链接/删除/重命名/截断/符号链接的检查序列与 FS 下发。
- 讲什么：K-336..348。
- 不讲什么：寻路执行（→13）；权限判定核（→29）；FS 请求包装（→12）；fd 取 vnode 规则（→14）；锁执行细节（→05/07/08）。
- 前置：13、14、02、15。
- 后置：28（stat 侧查询的邻接）；15（O_TRUNC 的 truncate_vnode 消费）。
- 事实底线：`link.c:29-508`（关键：29-86,79-84,87-131,91-164,132-152,169-271,197-217,240,276-381,387-424,430-508）；`link.rs`。
- 知识点清单：§2.1 27 号表全部 13 条。
- 验收标准：①LinkLookup 九种规格表完整（锁意图与状态分离）；②truncate 的两个时间戳语义（同长跳过 vs truncate_vnode 不比较）对照保留；③粘滞位两处收敛一函数的防漂移论证保留。

### 28-stadir

- 一句话定位：讲清工作目录/根目录切换与 stat/statvfs 查询族：引用替换顺序与实时/缓存两条读法。
- 讲什么：K-349..362。
- 不讲什么：寻路（→13）；X 权限判定（→29）；FS 下发（→12）；锁执行（→08）；用户缓冲拷贝（→内核侧）。
- 前置：13、14、02、08。
- 后置：29（权限检查点的邻接）；19（三名字的来源表）。
- 事实底线：`stadir.c:1-13,32-78,83-135,117-135,140-229,197-229,234-446,244-285,283-285,294-413,418-446`；`vmnt.c:246`（死代码判定引用）；`stadir.rs`。
- 知识点清单：§2.1 28 号表全部 14 条。
- 验收标准：①引用替换顺序的"颠倒即泄漏"论证保留；②getvfsstat 遍历五规则（计数/满停/跳过/锁后复验/先解锁后返回）完整；③三名字拷贝与 fetch_vmnt_paths 死代码判定的对照保留。

### 29-protect

- 一句话定位：讲清权限判定核 forbidden 及其写侧调用（chmod/chown/umask/access）。
- 讲什么：K-363..375。
- 不讲什么：凭证设置（→10 pm_setuid 族）；凭证结构定义（→02）；寻路与 fd 取 vnode（→13/14）；FS 下发（→12）。
- 前置：13、02、10。
- 后置：15/25/28/30（forbidden 与 R_BIT/W_BIT 的消费者）。
- 事实底线：`protect.c:25-302`（关键：25-94,67-81,98-165,145-159,182-192,198-302,238-302）；`utility.c:128-141`；`protect.rs`。
- 知识点清单：§2.1 29 号表全部 13 条。
- 验收标准：①九位三档查表与 root 边界三例外完整；②chmod/chown 检查顺序相反的对照表保留；③"FS 返回值为准、本地不复算"标注为 P0 级红线。

### 30-fcntl-lock

- 一句话定位：讲清 fcntl 十三命令与八槽劝告锁：区域计算、冲突判定、挂起与广播唤醒。
- 讲什么：K-376..391。
- 不讲什么：fd 分配实现与 close_fd 检查（→14）；filp 标志语义（→06）；阻塞载荷结构定义（→03）；FS 下发（→12）。
- 前置：14、06、02、09。
- 后置：31（同文件 misc.c 的分工对邻）；16（O_APPEND 对位）。
- 事实底线：`misc.c:117-275`；`lock.c:21-192`；`lock.h:7-13`；`glo.h:15`；`const.h:6`；`main.c:946-954`；`open.c:713-724`；`fcntl.rs`。
- 知识点清单：§2.1 30 号表全部 16 条。
- 验收标准：①十三命令分类表（复制二/查询设置四/锁三/打洞一/哨兵二/清缓存一）完整；②解锁四分支图（全清/头缩/尾缩/中裂）与"无空槽 ENOLCK"保留；③广播唤醒+误唤醒无害论证（unblock 重判）保留。

### 31-misc-queries

- 一句话定位：讲清杂项收束面：整表查询、同步广播、VM 反向查询、svrctl、时间戳与 gcov。
- 讲什么：K-392..403。
- 不讲什么：pm_reboot/free_proc 执行（→10，本篇只留 do_sync 调用点）；DS 分发执行（→18/09）；表锁与 fd 分配（→08/14）；寻路判权（→13/29）；FS 下发（→12）。
- 前置：09、08、12、29。
- 后置：99（省略台账的 panic_hook 判定对端）。
- 事实底线：`misc.c:52-112,276-375,380-498,797-898,948-986,998-1005`；`time.c:26-155`；`gcov.c:10-73`；`com.h:702-707`；`misc.rs`。
- 知识点清单：§2.1 31 号表全部 12 条。
- 验收标准：①vm_call 三请求两应答支路图完整（含 0xC1E 异步恒 SUSPEND）；②gcov 空标签 BUG 修复的对照（C :39-44 越界 vs Rust 拒绝）保留；③ds_event 分类-门-分派三步与"订阅循环挂 W5"边界保留。

### 99-VFS 全局概念

- 一句话定位：横向索引与收束：常量的机制依据、全局状态归属、跨表不变量、术语、工程制品、测试基建与省略台账——查阅篇，不属于线性阅读路径。
- 讲什么：K-404..415；N1（工程与构建制品小节）；N2（ToErrno 通道）；N3（测试基建小节）；N5 的一句归属声明。
- 不讲什么：一切机制（各有其篇）。
- 前置：00（唯一前置；查阅篇豁免线性序，见 §4.2 的 99 定位声明）。
- 后置：无（被全部篇引用）。
- 事实底线：`const.h:5-12,19-32,52`；`glo.h:13-44`；`utility.c:142-186`；`callnr.h:68`；`com.h:513,589,911,920,964,1038`；`vfsif.h:41-81`；`Makefile:1-30`；`minix-types/src/types/errno.rs:507`；各表容量常量的 Rust 落点。
- 知识点清单：§2.1 99 号表全部 12 条 + N1/N2/N3/N5(一句)。
- 验收标准：①容量常量表逐个有"为什么是这个值"；②省略台账维持判别标准（决策前提消失才入表）；③全 stage 唯一的总量数字落点（带截至日期）；④新增三小节（工程/错误通道/测试基建）各自有锚点。

---

## 6. 变更表

统一一张表（操作类型 ∈ {重排, 内容修正, 内容扩充, 归档}；本次设计**无拆分与合并**，理由见 §4.1）。

| 操作号 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|--------|------|--------|--------|------|-----------|----------|
| C-01 | 重排 | 08-worker-thread.md | 04-worker-pool.md | 运行时序 worker_init 先于表初始化（`main.c:445` vs `:486-489`）；tll 的阻塞原语依赖 worker（K-069） | K-074..085, N9 | 原样搬移+改写（去数字稳定化+文风统一） |
| C-02 | 重排 | 07-tll-lock.md | 05-tll-lock.md | 消除硬性前向引用：vnode/vmnt 用 `tll_t`（`vnode.h:22`/`vmnt.h:9`）而 tll 在后讲 | K-065..073 | 原样搬移+改写 |
| C-03 | 重排 | 04-filp-table.md | 06-filp-table.md | 表组按数据流 filp→vnode→vmnt 排列；filp 不依赖 tll，置于 tll 之后不产生新依赖 | K-035..044 | 原样搬移+改写（新增"本表不用 tll"声明） |
| C-04 | 重排 | 05-vnode-table.md | 07-vnode-table.md | 同 C-03 数据流序 | K-045..054 | 原样搬移+改写 |
| C-05 | 重排 | 06-vmnt-table.md | 08-vmnt-table.md | 同 C-03；本篇承载 N8 勘误（NR_MNTS=16） | K-055..064 | 原样搬移+改写（勘误执行） |
| C-06 | 重排 | 19-device-map.md | 18-device-map.md | mount 查 dmap 标签是数据依赖（K-222），表先于使用方 | K-223..235, N11 | 原样搬移+改写（新增死亡级联编排小节） |
| C-07 | 重排 | 18-mount.md | 19-mount.md | 同 C-06 对偶；开篇回指 01（序差 D1 补偿） | K-209..222 | 原样搬移+改写 |
| C-08 | 内容修正 | 00-vfs-overview.md §1.3 | 00（不变号） | "33 个活 REQ"→32 活+1 死（N7）；"5 个回调"→6 个（N6） | K-004, K-008 | 勘误执行 |
| C-09 | 内容修正 | 13-path-lookup.md §2/§5 | 13（不变号） | vmnt[8]→NR_MNTS=16（N8）；删 SlashHandler 残留测试行（K-143） | K-143, K-149 | 勘误执行 |
| C-10 | 内容修正 | 22-sdev.md §1.3 | 22（不变号） | "21 §1.6"错引→17 号 `pipe.c:548-549`（K-271）；Rust 模块声明按 N12 改写 | K-271 | 勘误执行 |
| C-11 | 内容修正 | 10-pm-protocol.md §2 | 10（不变号） | pm_reboot 锚点标签修正（K-104） | K-104 | 勘误执行 |
| C-12 | 内容修正 | 06（旧编号，现 08）§不讲什么 | 08 | 删 fetch_vmnt_paths 归 18 的错误条目（死代码，§3.4） | K-063 | 勘误执行 |
| C-13 | 内容扩充 | 09-main-loop.md | 09（不变号） | 补 dispatch_syscall/run_once 完成态（N4）与 VfsState 15 字段现状（N10） | K-096/097 | 新增（来源：`syscalls.rs:24-208`、`main_loop.rs:275-312,623`） |
| C-14 | 内容扩充 | 12-request-wrappers.md | 12（不变号） | 补 E-REQWIRE 收敛后的常量归属（N5） | K-140 | 新增（来源：`minix-types/src/ipc/fs_driver.rs`） |
| C-15 | 内容扩充 | 99-global-concepts.md | 99（不变号） | 新增三小节：工程与构建制品（N1）/ToErrno 通道（N2）/测试基建（N3） | K-404..415 | 新增（来源：`Makefile:1-30`、`errno.rs:507`、`grep -c '#[test]'`=365） |
| C-16 | 内容修正 | 全部 33 篇 §5 测试小节 | 对应篇 | 删除"截至日期 N passed"累计计数与模块级测试数；只留策略+关键测试名（§5.0 规则 4） | 全部 | 改写 |
| C-17 | 内容修正 | 全部 33 篇头部 | 对应篇 | 头部统一四行；删除对隐藏中间产物目录的全部引用（§5.0 规则 2） | — | 改写 |
| C-18 | 归档 | 33 篇旧文件（旧编号名） | `archive/doc-rerank-v1/`（B 相执行时定名） | 归档不删；旧文件作为知识来源保留 | 全部 | 整体搬入归档目录 |

---

## 7. 缺漏新篇

非 C 主题固定清单已逐项落实（§3.5）：两项缺口以**内容扩充**而非新建文件落地——N1/N2/N3 并入 99（查阅篇正是横向工程信息的归属地，新建第 32 号"工程篇"反而打破"机制 31 篇+导航+收尾"的稳定骨架）；N4/N5/N9/N10/N11/N12 分别并入 09/12/04/09/18/22（各自机制的归属篇）。无新建篇章、无否决项——12 条缺口全部落实，无"待定"。

---

## 8. 锚点迁移与断链成本

### 8.1 重编号映射表（7 篇）

| 旧文件名 | 新文件名 | 迁移类型 | 断链风险 |
|----------|---------|---------|---------|
| 08-worker-thread.md | 04-worker-pool.md | 原样搬移+改写 | 高（被 16 篇文档引用 23 次） |
| 07-tll-lock.md | 05-tll-lock.md | 原样搬移+改写 | 中（10 篇 17 次） |
| 04-filp-table.md | 06-filp-table.md | 原样搬移+改写 | 高（17 篇 34 次） |
| 05-vnode-table.md | 07-vnode-table.md | 原样搬移+改写 | 中（9 篇 15 次） |
| 06-vmnt-table.md | 08-vmnt-table.md | 原样搬移+改写 | 高（13 篇 28 次） |
| 19-device-map.md | 18-device-map.md | 原样搬移+改写 | 中（12 篇 22 次） |
| 18-mount.md | 19-mount.md | 原样搬移+改写 | 中（8 篇 12 次） |

其余 26 篇编号不变；标题微调不产生断链（引用均用文件名）。

### 8.2 锚点迁移表（按"节"粒度的去向总账）

由于 26 篇为"原位改写"，其锚点迁移已在 §5 各契约的"讲什么/事实底线/知识点清单"中逐条给出（每条 K-编号即一个迁移单元，415 条存量全部有去向）。本节只列**发生位置变化的节级迁移**：

| 旧位置 | 旧内容（一句话） | 新位置 | 迁移类型 | 备注 |
|--------|-----------------|--------|---------|------|
| 00 §1.2 启动主线图 | 全展开时序 | 00（压缩为五站导航）+01 §1.2（全图保留） | 拆分（节级） | 重复主讲述点 01 |
| 00 §3 ARCH 导航 | A-1/A-2/A-4/A-5 四条 | 00 保留（更新 A-2 为完成态表述） | 改写 | A-2 完成态见 N4 |
| 01 §2.6 lock_proc | 可睡眠锁机制 | 01 保留 C 语义；99 省略台账已有 Rust 判定 | 保留+回指 | |
| 02 §2.6 fork 复制 | 字段级生命周期 | 02 保留字段面；协议四步主讲述点在 10 | 保留+回指 | §3.3 裁决 |
| 04（旧）§2 close_filp | 三特殊分流 | 06（新）保留主讲述点；15 只留调用点 | 保留+回指 | §3.3 裁决 |
| 05（旧）标题与 §2 tll 映射 | VNODE_*→TLL_* | 07（新）保留；tll 语义已由 05（新）先行讲解 | 保留 | 依赖方向修复 |
| 06（旧）§不讲什么 fetch_vmnt_paths 条 | 归 18 的错误声明 | 删除；死代码判定在 99 台账；三名字拷贝在 28 | 删除+回指 | §3.4 |
| 08（旧）§1.1 主循环复活叙述 | main.c:590-596 一段 | 移入 09 §get_work（04 只讲原语） | 搬移 | 消除 04→09 前向 |
| 12 §3 Rust 归属 | request.rs 本地常量口径 | 12 补 N5（minix-types::fs_driver 单一事实源） | 改写 | E-REQWIRE 终态 |
| 18（旧）§2.6 dmap 标签查询段 | 挂载流程中的查表 | 19（新）保留；dmap 表本体在 18（新） | 保留+回指 | 依赖方向修复 |
| 19（旧）§2.7 恢复机 bdev_up 调用 | 恢复机叙述 | 18（新）保留（调用点级，20 的执行对端声明不变） | 保留 | 声明式边界，非前向违规 |
| 23 §6 过渡 | "下一站 24"导航句 | 23 保留（24 编号不变，无断链） | 原样 | |
| 24 §6 过渡 | "下一站 25"导航句 | 24 保留 | 原样 | |

### 8.3 引用迁移表

**文档间引用**（目录内 `grep -o 'NN-xxx\.md' *.md` 普查，总量 761 次；受重编号影响的 7 篇入边合计 **151 次**，逐篇入边数见 §8.1）。B 相批量修改方式：

```bash
# 在 05-stage-vfs 目录内（归档目录除外）按序执行：
sed -i 's/08-worker-thread\.md/04-worker-pool.md/g; s/07-tll-lock\.md/05-tll-lock.md/g; s/04-filp-table\.md/06-filp-table.md/g; s/05-vnode-table\.md/07-vnode-table.md/g; s/06-vmnt-table\.md/08-vmnt-table.md/g; s/19-device-map\.md/18-device-map.md/g; s/18-mount\.md/19-mount.md/g' *.md
# 安全性说明：以上替换模式均为完整文件名且互不为对方输出的子串，
# 单遍按序执行不会链式覆盖（已逐对推演）；但 B 相执行后必须以
# `grep -rn '旧名' *.md` 全量归零 + git diff 人工复核兜底。
# 另需归一两类无后缀异写：05-vnode.md→07-vnode-table.md；06-vmnt.md→08-vmnt-table.md。
```

热点文件（引用重编号文档最多，需人工复核 diff）：`09-main-loop.md`（47 次总引用中含 07/08 旧名）、`14-filedes.md`（43 次）、`99-global-concepts.md`（42 次）、`02-fproc-struct.md`（39 次）、`13-path-lookup.md`（34 次）。

**代码注释引用**（`os/servers/vfs/src/` 82 行 86 次；受重编号影响的完整清单，B 相逐条迁移并验证 `grep` 归零）：

| 代码位置 | 旧引用 | 新引用 |
|----------|--------|--------|
| `read_write.rs:195` | 08-worker-thread.md | 04-worker-pool.md |
| `tll.rs:6` | 07-tll-lock.md | 05-tll-lock.md |
| `fcntl.rs:19`、`filp.rs:6` | 04-filp-table.md | 06-filp-table.md |
| `link.rs:84`（异写 05-vnode.md）、`vnode.rs:6` | 05-vnode(-table).md | 07-vnode-table.md |
| `mount.rs:18`、`vnode.rs:16`、`stadir.rs:20`（异写 06-vmnt.md） | 06-vmnt(-table).md | 08-vmnt-table.md |
| `cdev.rs:17`、`mount.rs:9` | 18-mount.md | 19-mount.md |
| `cdev.rs:16`、`device_map.rs:17` | 19-device-map.md | 18-device-map.md |

其余 69 处代码注释引用的文档编号不变，无需迁移；但其中 `worker.rs:236` 引用"05-stage-vfs plan §5.4"指向 plan.md——plan.md 处置见下。

**plan.md / todo.md 处置**：二者是 stage 工作档案，不参与重编号迁移（其历史叙述保持原样）；B 相在新目录定稿后于 plan.md 顶部追加一行注记："本计划已被 doc_rerank 共识蓝图取代，现行编号以 §新目录 为准；plan.md §6 状态表已过时（08/09/10/11/12/13/14/23-29 实际均已改写）"。`worker.rs:236` 的 plan 引用同步改为指向新蓝图。

### 8.4 断链成本摘要

- 受影响引用总量：**164 处**（文档 151 + 代码注释 13），另有 2 处无后缀异写并入处理；plan/todo 不迁移仅注记。
- 热点：文档侧 `04-filp-table`（34）与 `08-worker-thread`（23）；代码侧 `06-vmnt` 族（3 处）。
- 批量修改方式：§8.3 的两阶段 sed + 全量 diff 复核 + `grep -rn '旧名' *.md os/servers/vfs/src` 归零验证；代码注释迁移属生产文件改动，逐条走 fix-guard（一次一处、修后 grep 验证）。
- 成本判定：164 处机械替换 + 7 篇改写，对比收益（消除 2 处硬性前向引用、修复 9 处事实错误、统一 4 代文风、补 12 条缺口）——成本可接受。历史教训对照：`01-stage-kernel/todo.md` I-14 曾因"重编号断链风险大于收益"停手，当时无本表的逐节成本账；本次先算账后动刀，且 33 篇中 26 篇零编号变动，风险面已收窄到 7 篇。

---

## 9. 验证与自检门

### 9.1 四种机械检查

1. **前向引用扫描**：§4.4 已逐篇核对"前置"字段——全部指向更小编号；99 以"查阅篇豁免"声明处理（§4.2），正文常量就地给值。**通过**。
2. **依赖图无环**：§4.4 列出全部前置边，逐边指向更小编号；13↔19 对偶已单向化（19→13）。**通过**。
3. **覆盖率**：存量 415 条全部在 §2.1 表内带去向列；新增 12 条在 §3.2 带证据锚点与去向；删除项 5 处（§3.4）均为过时叙述而非知识点。合计 427 条，100% 有归属。**通过**。
4. **断链成本统计**：§8.4——164 处受影响引用、热点清单、两阶段批量方案齐备。**通过**。

### 9.2 自检门逐门结果

| 门 | 结果 | 证据 |
|----|------|------|
| G1 C 真序逐条可核对（抽十条） | **通过** | 抽验：①S06 fproc 清零 `main.c:405-408`（sed 实读）；②S07 NONE 终止 `:428-431`；③S10 worker_init `main.c:445`+`worker.c:27`；④S15 四表 `:486-489`；⑤S17 门控三步 `:507/510/522`；⑥L08 三 RS 基址 `com.h:920/964/1038`+掩码 `~0x7f`；⑦L12 call_vec 64 项与四别名（table.c 全文实读）；⑧K-051 256 阈值 `vnode.c:246,263-264`（grep 实证）；⑨K-056 NR_MNTS=16 `const.h:7`；⑩filp 不用 tll `file.h:14` vs `vnode.h:22`/`vmnt.h:9` |
| G2 知识点池完整（每个 C 文件/非 C 制品有归属） | **通过** | 33 个 .c 全部落入 §5 契约"事实底线"（time.c/gcov.c→31；device.c→18；utility.c→03/13/29/99；write.c→16；其余整文件篇）；15 个本地 .h 全部落篇（§2.1 各表锚点列）；非 C 制品清单 §3.5 十项逐项回答，无"未回答"项 |
| G3 前向引用为零（逐篇前置扫描） | **通过** | §4.4 前置边清单；声明式边界指针与机制引用的判别标准写在 §4.2/§5.0 规则 3 |
| G4 依赖图无环 | **通过** | §4.4 逐边核对；唯二的双向对偶（13↔19、16↔20）均已单向化或声明方向（16→20 是 20 的边界声明，机制在 16） |
| G5 覆盖率 100% | **通过** | 427 条全有去向或删除理由（§9.1 第 3 条）；新增 12 条全带证据锚点；无凭空知识点 |
| G6 拆合写去向/新建写来源（抽十处） | **通过**（无拆合） | 本设计零拆分零合并，去向规则无适用对象；六处内容扩充的来源锚点：C-13（syscalls.rs/main_loop.rs）、C-14（fs_driver.rs/protocol.rs）、C-15（Makefile/errno.rs/test 计数）、C-06（invalidate 族/StopPlan/DeathKind 三文件）、C-08（vfsif.h:41-75、main.c:374-390）、C-12（vmnt.c:246 死代码判定） |
| G7 每篇契约七要素齐全（34 篇） | **通过** | §5.1 逐篇含：定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单（以 §2.1 表+K 编号承载，声明见 §2.0）/验收标准；00 与 99 的特殊前置处理有显式声明 |
| G8 迁移表覆盖每节；引用迁移覆盖文档+代码注释 | **通过** | §8.2 节级迁移 13 行（变化项全覆盖，26 篇原位改写以契约承载）；§8.3 文档引用两阶段方案+代码注释 13 处逐条列净 |
| G9 事实断言有锚点（抽十条） | **通过** | 抽验：①6 个 sef_setcb（main.c:374-390 实读逐行）；②64 调用号 callnr.h:72-135（实读）；③0x900/0x980 com.h:513-514；④REQ_GETNODE "Should be removed" vfsif.h:41；⑤TRNS 三宏 vfsif.h:79-81；⑥SYMLOOP=16 const.h:32；⑦NR_WTHREADS=9 const.h:9；⑧MAXSELECTS 25 select.c:31（grep）；⑨run() mock 现状 main_loop.rs:797-828（agent 盘点+todo Fix #37 交叉印证）；⑩365 个 #[test]（grep -c 统计）。**推测项标注**：§3.2 N5 中"request.rs 是否已改消费 minix-types"未逐行核实，已显式标注"B 相按现状核对"；K-030 的 glo.h:26-28 行号沿用多文档一致口径未单行验证，B 相写作时按 §5.0 规则 6 复核 |

### 9.3 结论与待用户裁决的问题

**结论**：蓝图完成。新目录 34 篇（重编号 7 篇、零拆合、零新建文件、12 条缺口并入既有篇），锚点迁移总账 164 处，九道自检门全过。B 相拿到本蓝图后无需再做取舍判断：每篇按 §5 契约取料（存量按 K-编号定位旧文档段落，新增按 N-编号的证据锚点取 C/制品），按 §5.0 通用规则统一文风与头部，旧文件整体归档（C-18），引用按 §8.3 两阶段方案迁移。

**待用户裁决**：
1. 重编号涉及 7 篇文件改名与 164 处引用迁移——若多数 AI 蓝图倾向"零重编号+只修内容"，汇总收敛时可对 C-01..C-07 单独表决；本蓝图的立场是：两处硬性前向引用（tll、mount/device-map）只有重编号能根治，且成本账（§8.4）已收窄到可接受。
2. 99 号"查阅篇豁免线性序"的解释（§4.2）是否被共识接受——若不接受，替代方案是把 99 拆为"术语篇（前置化）+省略台账（收尾）"，代价是再增一篇与全量引用迁移。
3. `01-stage-kernel/06-todo.md` 写法范例只影响了本蓝图的契约骨架；若共识蓝图要求"契约含逐测试名清单"，需在 B 相前补充（本蓝图按 §5.0 规则 4 刻意去数字化，立场是测试名已足够、计数必腐）。
