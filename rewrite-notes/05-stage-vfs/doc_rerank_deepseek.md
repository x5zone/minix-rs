# 05-stage-vfs 文档重建蓝图（deepseek）

## 0. 元数据

- 执行者：deepseek（AI agent 名称，用于本目录产物后缀）
- 日期：2026-09-19
- 目标目录：`rewrite-notes/05-stage-vfs/`
- 仓库根目录：`/home/xzhao/github/minix-rs`
- 当前提交号：`6965e6885`（`git log --oneline -1`）；工作树另有未提交修改与未跟踪文件，本报告的数据以工作树实测为准，并在每处标注命令。
- 交付物：本文件。除本文件外不修改、不重命名、不移动、不删除任何文件；未提交任何变更。

### 0.1 审查范围

**范围内的正式文档**（共 33 篇，全部进入知识点池与重建契约）：

```
00-vfs-overview.md     01-vfs-init-main.md    02-fproc-struct.md
03-fproc-table.md      04-filp-table.md       05-vnode-table.md
06-vmnt-table.md       07-tll-lock.md         08-worker-thread.md
09-main-loop.md        10-pm-protocol.md      11-fs-comm.md
12-request-wrappers.md 13-path-lookup.md      14-filedes.md
15-open-close.md       16-read-write.md       17-pipe.md
18-mount.md            19-device-map.md       20-bdev.md
21-cdev.md             22-sdev.md             23-select.md
24-socket.md           25-exec.md             26-coredump.md
27-link.md             28-stadir.md           29-protect.md
30-fcntl-lock.md       31-misc-queries.md     99-global-concepts.md
```

**参考材料**（不作为重建对象，只作为线索与阶段边界依据）：`plan.md`、`todo.md`、`draft/`（旧 fork 主线素材，21 篇 + README）、`archive/todo-R1-archive-2026-09-09.md`、`00-master-plan/README.md` 与 `00-master-plan/08-phase4-vfs-guide.md`、`edge_todo.md`。

**范围外**（明确不进入本阶段正式文档，只做交叉引用）：

- 底层文件系统服务端（`minix3/minix/fs/` 的 mfs/pfs/ext2/isofs/vbfs/hgfs 等）与其框架库（`libfsdriver`/`libminixfs`/`libvtreefs`/`libsffs`）→ 15-stage-fs。
- 设备驱动本体（tty、块盘、socket 驱动实现）→ 16-stage-drivers / 17-stage-net。
- 内核侧 `sys_*` 接口实现（`sys_safecopy*`、`sys_datacopy`、`sys_hz`、`asynsend3`、grant 表）→ 01-stage-kernel。
- 用户态 C 库 syscall 桩（`minix3/minix/lib/libc/sys/*.c`）与运行时装载（crt0、SEF）→ 14-stage-runtime 与 01-stage-kernel。
- 系统调用从用户进程到 VFS 的完整路径（trap、内核 syscall task、IPC 投递）→ 01-stage-kernel 12-ipc-core / 17-syscall-process。
- 同目录下其它执行者的重建产物：不在本报告的阅读、引用、对照范围。

### 0.2 读取清单

**文档**：上述 33 篇全部（正文由四个并行只读探查任务各通读一组并逐条核对锚点；主会话对承重结论复核）。另读 `plan.md` 全文、`todo.md` 前 314 行与 315-547 行标题、`draft/README.md`、`00-master-plan/README.md`、`04-stage-pm/00-pm-overview.md`（前一 stage 总览）、`04-stage-pm/05-vfs-interaction.md` 标题表、`15-stage-fs/00-fs-overview.md` 标题、`02-stage-vm/23-vfs-interaction.md` 标题表、`edge_todo.md` 中 VFS 相关条目。

**C 源码**（ground truth）：`minix3/minix/servers/vfs/` 全部 33 个 `.c` + 15 个本地 `.h`（17,742 行，其中 `.c` 16,735 行、`.h` 1,007 行，`wc -l` 实测）。主会话全文精读 `main.c`（973 行）、`table.c`（82 行）、`fproc.h`（117 行）、`vfsif.h`（84 行），选段精读 `worker.c`（worker_start/worker_main）、`mount.c`（mount_fs 头部）、`misc.c`（do_vm_call）、`write.c`（全文）、`select.c` 函数表尾、`write.c`。其余文件按函数清单与行锚核对。

**外部头文件与线格式**：`minix3/minix/include/minix/vfsif.h`（REQ_*/TRNS_*/RES_*）、`minix3/minix/include/minix/com.h` 的 VFS_PM（505-583）、FS_BASE（589）、VFS_TRANSACTION（905-912）、CDEV/BDEV（921-990）、SDEV（1034-1050）段、`minix3/minix/include/minix/callnr.h` 的 `VFS_BASE` 与 64 个调用号（68-137）、`minix3/minix/servers/rs/table.c`（boot image priv/sys 表）、`minix3/minix/kernel/table.c`（boot image 名称表）。

**非 C 制品**：`minix3/minix/servers/vfs/Makefile`（SRCS 清单、`LDADD=-lsys -ltimers -lexec -lmthread`、`MKCOVERAGE` 条件编译 `gcov.c`）、`minix3/minix/lib/libsys/timers.c`（`expire_timers`/`set_timer` 的真实归属）、`minix3/minix/lib/libmthread/`（worker 线程库，08 的 ARCH 对照）、`os/servers/vfs/Cargo.toml`（crate 依赖与 feature）、`os/libs/minix-types/src/ipc/vfs.rs`（`VfsCall` 11 变体 + `VfsPmInit`）、`os/libs/minix-types/src/ipc/fs_driver.rs`（E-REQWIRE 收敛后的 REQ 权威）、`os/servers/vfs/src/`（34 个 `.rs`，24,784 行，365 个 `#[test]`，实测）、`os/libs/minix-sockdriver/src/sdev.rs`（sdev 线协议实现迁移后的宿主）。

**测试基建**：`os/qemu-tests/`（现有测试内核均不涉及 VFS；无 VFS 端到端用例）、`os/tests/`（仅 `pm_vm_fork*.rs`，且正文停用）。此项作为非 C 主题在 §3.5 与 §7 作答。

### 0.3 使用的命令与关键输出（证据摘录）

```text
$ git log --oneline -1
6965e6885 feat(pm): S8 D-31/D-32——KernelGateway::diag_write 生产委托 + syscall_stats 调用计数

$ wc -l minix3/minix/servers/vfs/*.c | tail -1
 16735 total
$ wc -l minix3/minix/servers/vfs/*.h | tail -1
 1007 total

$ ls minix3/minix/servers/vfs/*.c | wc -l        → 33
$ ls minix3/minix/servers/vfs/*.h | wc -l        → 15

$ grep -cE '^\s*CALL\(' minix3/minix/servers/vfs/table.c   → 64
$ grep -c '#define REQ_' minix3/minix/include/minix/vfsif.h → 33（含死常量 REQ_GETNODE）
$ grep -c '#define VFS_PM_' minix3/minix/include/minix/com.h（VFS_PM_RQ/RS 段）→ 12 请求 + 11 回复

$ find os/servers/vfs/src -name '*.rs' | xargs wc -l | tail -1
 24784 total
$ grep -rc '#\[test\]' os/servers/vfs/src/*.rs os/servers/vfs/src/ipc/*.rs | awk -F: '{s+=$2} END {print s}'
365

$ grep -rn '05-stage-vfs' os/ tools/ --include='*.rs' --include='*.toml' --include='*.md' | wc -l
4（os/servers/vfs/src/worker.rs:236、os/servers/pm/src/exit.rs:405、
   os/libs/minix-types/src/ipc/fs_driver.rs:11、os/libs/minix-types/src/ipc/vfs.rs:107）

$ grep -rhoE "[0-9]{2}-[a-z0-9-]+\.md" rewrite-notes/05-stage-vfs/*.md | wc -l
765（stage 内文档互引计数，用于 §8 断链成本）

$ sed -n '15,20p' minix3/minix/servers/rs/table.c
{RS_PROC_NR, "rs", RSYS_F },
{VM_PROC_NR, "vm", VM_F },
{PM_PROC_NR, "pm", SRV_F },
{SCHED_PROC_NR,"sched", SRV_F },
{VFS_PROC_NR, "vfs", SRV_F },
```

---

## 1. C 真序

### 1.1 阶段类型判定与理由

本 stage 判定为**服务事件循环型**，而不是启动链型或集合型。理由（全部带锚点）：

1. VFS 是一个常驻的用户态服务器：`main()`（`main.c:54`）在末尾进入 `while (TRUE)`（`main.c:69`），永不退出；启动只是进入循环的前置。
2. 启动段之后，服务器的一切行为都由"收消息—分派—处理—回复"驱动：`get_work()`（`main.c:580`）阻塞收消息，主循环按八路优先级分派（`main.c:80-138`），处理由 9 个工作线程（`worker.c`）承接，回复由 `reply()`（`main.c:638`）或异步回复路径完成。
3. 请求面是并行的 64 个系统调用（`table.c:17-82` 的 `call_vec`）加 12 个 PM 请求（`com.h:521-531`）加 35 个 FS 协议常量（其中 32 个活，`vfsif.h:41-73`），不是线性调用链；按场景分组的请求处理才是主线。

因此本报告按"启动段 + 循环段 + 一次请求的生命周期"三段重建真序；并行请求面的分组归 §4 阅读路径。

### 1.2 真序表

**启动段**（从被 RS 加载到进入主循环，`main.c:54-527`）：

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| S01 | RS 从 boot image 启动 VFS | `rs/table.c:16` `{VFS_PROC_NR,"vfs",SRV_F}`；`kernel/table.c:57` | VFS 是 boot image 直接登记的服务；加载细节归 01-stage-kernel/06 与 03-stage-rs |
| S02 | `main()` 入口 | `main.c:54-64` | 先调 `sef_local_startup()`，再打印线程数 |
| S03 | SEF 本地启动，注册 5 组回调 | `main.c:374-388` | `init_fresh`/`init_restart`/`init_lu`/`lu_prepare`/`lu_state_changed`（`lu_state_isvalid` 用标准实现） |
| S04 | `sef_startup()` → `sef_cb_init_fresh()` | `main.c:387,393` | 初始化主入口 |
| S05 | fproc 槽清零 | `main.c:405-408` | `fp_endpoint=NONE`、`fp_pid=PID_FREE` |
| S06 | `VFS_PM_INIT` 握手循环 | `main.c:415-434`；`com.h:521` | 每条消息填一个槽（slot/pid/endpoint/凭证/umask=`~0`）；`VFS_PM_ENDPT==NONE` 终止 |
| S07 | 回 `OK` 同步 PM | `main.c:435-436` | 双向确认 |
| S08 | `system_hz = sys_hz()` | `main.c:438` | 时钟频率缓存；`sys_hz` 内核侧 |
| S09 | `ds_subscribe("drv\\.[bc]..\\..*")` | `main.c:441` | 驱动事件订阅（DS 客户端）；失败即 panic |
| S10 | `worker_init()` | `main.c:445`；`worker.c:27` | 建 9 个 worker 线程（`NR_WTHREADS=9`，`const.h:9`） |
| S11 | `bsf_lock` 初始化 | `main.c:448` | 块特殊文件全局锁（mthread mutex） |
| S12 | `init_dmap()` / `init_smap()` | `main.c:451-452`；`dmap.c:230`、`smap.c:22` | 设备表初始化；CTTY 等预置 |
| S13 | 取 RS 的 `rproctab` 并 `map_service()` | `main.c:455-465` | `sys_safecopyfrom(RS_PROC_NR, info->rproctab_gid, ...)` 整表拷贝 + 逐条映射 boot 服务 |
| S14 | 第二遍 fproc：锁初始化、fd 清空、wd/rd 置 NULL | `main.c:468-484` | `fp_worker=NULL`；`LOCK_DEBUG` 计数清零 |
| S15 | `init_vnodes` / `init_vmnts` / `init_select` / `init_filps` | `main.c:486-489`；`vnode.c:138`、`vmnt.c:127`、`select.c:821`、`filedes.c:73` | 四张核心表建立 |
| S16 | `worker_start(fproc_addr(VFS_PROC_NR), do_init_root, ...)` | `main.c:492-493` | 根挂载在 worker 线程里执行，不阻塞初始化返回 |
| S17 | `do_init_root()`：先 `worker_allow(FALSE)` | `main.c:501-507` | 根未挂好前拒绝用户请求（请求只排队） |
| S18 | `mount_pfs()` | `main.c:510`；`mount.c:391` | 罐装挂载管道文件系统（三个固定标签，`PFS_PROC_NR`） |
| S19 | `mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS_PROC_NR, 0, "mfs", "fs_imgrd")` | `main.c:513-519`；`mount.c:156` | 查 dmap 找驱动、找空闲 vmnt、`req_readsuper` 往返、建根 vnode、`have_root` 递进（`mount.c:205-209`）、`MAKEROOT` 换根（`mount.c:327-328`）；失败 panic |
| S20 | `worker_allow(TRUE)` | `main.c:522` | 开闸，排队请求可以执行 |
| S21 | 进程进入 `main.c:69` 主循环 | `main.c:495` | `sef_cb_init_fresh` 返回 OK |

**循环段**（`main.c:69-141`，一轮的优先级顺序即真序）：

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| L01 | `worker_yield()` | `main.c:70`；`worker.c:431` | 主线程让出；`self=NULL` |
| L02 | `send_work()` | `main.c:72`；`comm.c:37` | 扫描全部 vmnt 的请求队列，向空闲的 FS 补发（`glo.h:17 sending` 短路） |
| L03 | `get_work()`：`reviving != 0` 时优先复活 | `main.c:580-597` | 扫描 `FP_REVIVED` 槽，`unblock()` 重建被挂起的请求（不阻塞收消息） |
| L04 | `get_work()`：`sef_receive(ANY, &m_in)` | `main.c:599-631` | 收到消息后算 `who_p`、核对 `fp_endpoint` 一致性（`main.c:621-628`）；endpoint 未知/为 NONE 的消息丢弃 |
| L05 | FS 异步回复路由 | `main.c:80-90`；`com.h:909-912` | `TRNS_GET_ID(m_in.m_type)` 命中 `IS_VFS_FS_TRANSID` → `worker_get(tid - VFS_TRANSID)` → 校验 `w_fp` → `TRNS_DEL_ID` → `do_reply(wp)` |
| L06 | PM 控制面路由 | `main.c:91-94` | `who_e == PM_PROC_NR` → `service_pm()`（不进 call_vec） |
| L07 | 通知路由 | `main.c:95-117` | `is_notify(call_nr)`：DS → `handle_work(ds_event)`（先过 `worker_can_start`）；KERNEL → `mthread_stacktraces()`；CLOCK → `expire_timers()`（实现在 `libsys/timers.c:97`） |
| L08 | 任务消息忽略 | `main.c:118-124` | `who_p < 0`（内核任务）只允许 notify，普通消息打印并丢弃 |
| L09 | 设备回复三路 | `main.c:126-134` | `IS_BDEV_RS` → `bdev_reply()`；`IS_CDEV_RS` → `cdev_reply()`；`IS_SDEV_RS` → `sdev_reply()` |
| L10 | 普通系统调用 | `main.c:135-138`；`main.c:146-181` | `handle_work(do_work)`：若来源是 FS 端点（`FP_SRV_PROC`）则先做 `VMNT_CALLBACK`/可用线程二重守门，否则 `replycode(EAGAIN)` |
| L11 | 绑定到进程并启动执行 | `main.c:180`；`worker.c:360-428` | `worker_start(fp, do_work, &m_in, use_spare)`：登记 `fp_msg`/`fp_func`，`worker_try_activate` 找空闲线程或排队 |
| L12 | worker 主循环执行 | `worker.c:239-280` | `worker_get_work()` 取活 → `lock_proc` → `fp->fp_func()`（即 `do_work`）→ 若有 `FP_PM_WORK` 再跑 `service_pm_postponed()` → `thread_cleanup()`（清 `VMNT_CALLBACK`，`main.c:558-575`） |
| L13 | `do_work()` 分发 | `main.c:263-298` | `PID_FREE` 的请求直接丢弃；`IS_VFS_CALL` 且下标 `<NR_VFS_CALLS` 且表项非 NULL → `(*call_vec[call_index])()`；否则 `ENOSYS` |
| L14 | handler 执行并可能挂起 | 各族 `do_*`；`pipe.c:294`、`select.c:96`、`cdev.c:279`、`sdev.c:340` | 返回 `SUSPEND` 表示"稍后回复"：该请求的回复改由复活路径补齐 |
| L15 | 同步回复 | `main.c:296-297`；`main.c:638-663` | `error != SUSPEND` → `reply(&job_m_out, fp->fp_endpoint, error)`（`ipc_sendnb`，失败只打印） |
| L16 | 挂起后的三条复活路径 | `pipe.c:435`、`select.c:783`、`cdev.c:481`、`sdev.c:759`、`main.c:921-973` | 管道：`revive` 标记 + `reviving` 计数 → 主循环 `unblock` 重建请求（`do_pending_pipe` 或整请求重放）；select/驱动：回复路径直接恢复挂起槽 |
| L17 | 工作线程回收 | `worker.c:265-280`、`main.c:558` | 清 `fp_func`/`fp_worker`/`w_fp`，`busy--`；回到 L01 |

**一次 FS 往返的完整生命周期**（贯穿 11/12 与 09，用于契约验收）：

| 步 | 动作 | C 锚点 |
|----|------|--------|
| F01 | handler 调 `req_*` 包装构造消息（带 grant） | `request.c` 各函数 |
| F02 | `fs_sendrec`/`fs_sendmsg` 占用挂载窗口并发送 | `comm.c:134-168`、`comm.c:12-32` |
| F03 | 窗口满/CALLBACK 属性 → `queuemsg` 入 `m_comm.m_req_queue` 排队 | `comm.c:223-244` |
| F04 | 主循环 `send_work()` 在 FS 空闲时补发 | `comm.c:37-45` |
| F05 | FS 回复带 `TRNS_ADD_ID` 编码的消息 | `vfsif.h:80`、`comm.c:20-23` |
| F06 | 主循环 L05 路由到 `do_reply`，写入 `wp->w_sendrec`、`worker_signal` | `main.c:187-211` |
| F07 | worker 从等待点继续，`req_*` 的 `_actual` 路径回填响应 | `request.c` 各 `_actual` 函数 |

### 1.3 序差表（运行时序与教学序不一致处）

| # | 运行时事实（带锚点） | 教学序选择 | 理由 | 回指补偿 |
|---|---------------------|-----------|------|----------|
| D-1 | `VFS_PM_INIT` 握手是全系统最早的消息交互（`main.c:415-434`） | 01 只讲信封与 handshake 状态机，10 才展开 PM 协议全貌 | fork/exit/exec 语义依赖 fproc/filp/vnode 表（02-06），无法在启动篇讲完 | 01 §握手只描述"收一条填一槽"；10 开头回指 01 |
| D-2 | 根挂载 `mount_pfs`/`mount_fs` 在启动段执行（`main.c:501-523`） | 挂载机制整篇后置到 18 | 挂载依赖 dmap（19）与 req_readsuper（12）；启动篇讲细节会产生前向引用 | 01 §do_init_root 只讲门控与调用点；18 开头回指 01 |
| D-3 | DS 驱动事件订阅在启动段完成（`main.c:441`） | 事件处理 `ds_event` 与 dmap 恢复在 19 展开 | 需要先懂 dmap/smap 表 | 01 §订阅记一条"做什么、失败语义"；19 展开分类处理 |
| D-4 | 主循环先于任何请求处理（`main.c:69`） | 08（worker）/09（主循环）排在核心表之后 | 主循环的分派对象是 fproc/filp/vnode/vmnt，先有对象再讲调度 | 02-07 每篇结尾声明"它在启动时序的哪一步" |
| D-5 | CLOCK 通知直接调 `expire_timers`（`main.c:112`），实现却在 `libsys/timers.c:97` | 09 只讲分发，23 讲 select 超时 | 定时器实现属用户态运行时库，不属 VFS 进程代码 | 09 §notify、23 §超时各注明实现在 `libsys/timers.c` |
| D-6 | `do_work` 对 `RMDIR` 与 `UNLINK` 共用 `do_unlink`（`table.c:36`） | 27 讲删除语义，09 只讲分发表 | 别名是分发表事实，语义在 link 篇 | 09 §call_vec 标注别名；27 回指 |
| D-7 | `do_pending_pipe` 由主循环通过 `unblock` 重放（`main.c:216-258,921-973`） | 17 讲管道挂起/复活，09 只讲 `unblock` 二路 | 管道定量机是 16/17 的前置知识 | 09 §unblock 标注二路去向；17 开头回指 |
| D-8 | `mount_fs` 内部调用 `eat_path`/`lookup`（`mount.c:186-199`） | 13 讲路径解析，18 只引用 | 挂载点解析复用解析机 | 18 §提交序引用 13 的解析机接口 |
| D-9 | `worker_stop` 由 dmap/smap 端点回收触发（`dmap.c:180-199`、`smap.c:148-170`） | 19 讲触发，20-22 讲各族停尸，22 收束总谱 | 触发事件一处，后果分族 | 19 §unmap 列出后果清单；22 §总谱汇总 |
| D-10 | `do_vm_call` 是 VM 发来的上游请求，回复恒异步（`misc.c:380-500`） | 32 独立成篇，放在设备面之后 | VM 请求依赖 fd/filp（04/14）与 VM 侧 fdref 概念（02-stage-vm/23） | 32 开头回指 04/14；14 的 copyfd 对照 |
| D-11 | `exec` 的装载由 VFS 请求 VM `mmap` 完成（`exec.c:161-180`） | 25 讲调用面，VM 侧实现归 02-stage-vm/20 | 实现主体在对端 | 25 §装载双路径标注对端指针 |
| D-12 | 设备回复路由在主循环（`main.c:126-134`） | 09 讲路由，20/21/22 讲解码 | 解码需先懂驱动协议面 | 09 §设备三路只给判定宏；20/21/22 回指 |

**阶段类型补充说明**：本 stage 兼具"库与框架型"的局部特征（FS 通信窗口、req_* 包装是框架层），但框架层同样是 VFS 进程内部机制，且被请求处理主线依赖，故不单独按框架型组织，而是保持"消息面（09-12）→ 请求族（13-30）"的先后关系。

---

## 2. 知识点全集

### 2.1 说明

- 编号规则：`K-NNN`，按新目录（§4）的篇章顺序分块编号，块间留空号，便于汇总收敛时插入。
- 来源类型只有两种：**存量**（来自现有 33 篇文档，后续必须回答"搬到哪里去"）；**新增**（现有文档没有讲、由 C 源码/非 C 制品/操作系统理论承载，必须有证据锚点）。同一知识点在多篇重复出现的合并为一条，并在"现有位置"列全部列出、用加粗标主讲述点。
- 锚点列使用"C 源码或制品或文档"的短锚：C 使用 `文件:行号` 或 `文件:函数名`（minix3 源码冻结，行号可作为稳定锚）；Rust 只允许符号名（行号不稳定）；外部制品给路径。
- 类型取值：概念／机制／数据结构／接口与协议／约束与不变量／架构演进／工具工程／测试性质。

### 2.2 池总表

#### 00-vfs-overview（新：00）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置（旧文档与小节） | 锚点 | 读者收益 |
|------|------|------|----------|--------------------------|------|----------|
| K-001 | VFS 的角色与三方边界 | 概念 | 存量 | 00 §1.1 | `servers/vfs/` 全部；`rs/table.c:16` | 明白 VFS 管什么、不管什么 |
| K-002 | 一次请求的生命周期总览 | 机制 | 存量 | 00 §1.2；09 §2.3-2.9 | `main.c:580-663`、`table.c:17-82` | 建立"收—分—处—回"心智模型 |
| K-003 | 三个协议面规模（64 调用/12 PM/35 REQ 常量） | 接口与协议 | 存量 | 00 §1.3 | `callnr.h:68-137`、`com.h:513-544`、`vfsif.h:41-73` | 知道对外契约的边界与数量 |
| K-004 | ARCH 改写清单导航（A-1…A-16） | 架构演进 | 存量 | 00 §3；plan §4 | `os/servers/vfs/src/` | 定位每一项改写决策 |
| K-005 | 阅读路径（主线/支线/可跳读） | 工具工程 | 存量 | 00 §6（两条路径） | — | 按目标选路 |
| K-006 | 全阶段文档共守的四条设计原则 | 约束与不变量 | 存量 | 00 §1.4 | — | 读法约定 |
| K-007 | 阶段在启动链中的位置（前接 PM，后接 FS/驱动） | 概念 | 存量 | 00 §1.1-1.2；01 §1.1 | `00-master-plan/README.md`（阶段因果链） | 知道 VFS 对谁负责 |
| K-008 | 32/33 新篇与 VM/系统信息协议的一页索引 | 接口与协议 | 新增 | —（旧文无） | `misc.c:380`、`misc.c:52` | 找旁路协议不迷路 |

#### 01-vfs-init-main（新：01）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-010 | VFS 由 RS 从 boot image 启动（非 VFS 自举） | 概念 | 存量 | 01 §1.1 | `rs/table.c:16`、`kernel/table.c:57` | 启动归属与依赖 |
| K-011 | 启动依赖链与三条顺序不变量 | 约束与不变量 | 存量 | 01 §1.2 | `main.c:393-523` | 为什么是这个初始化顺序 |
| K-012 | VFS_PM_INIT 握手协议（填槽、终止、回 OK） | 接口与协议 | 存量 | **01 §1.3/§2.3**；10 §1.1/§2.2；02 §2.6 | `main.c:410-436`、`com.h:521-527` | 两张进程表如何对齐 |
| K-013 | boot 进程身份（SYS_UID/SYS_GID/umask=~0） | 约束与不变量 | 存量 | 01 §1.3 | `main.c:419-433`、`const.h:16-17` | 系统进程凭证来源 |
| K-014 | SEF 五组回调注册（fresh/restart/LU） | 机制 | 存量 | 01 §2.2 | `main.c:374-388` | 服务生命周期框架接法 |
| K-015 | 初始化链十一项清单 | 机制 | 存量 | 01 §2.3 | `main.c:393-499` | 启动全景清单 |
| K-016 | `system_hz` 的取得与用途 | 接口与协议 | 存量 | 01 §2.3 | `main.c:438` | 时钟换算来源 |
| K-017 | DS 驱动事件订阅（`ds_subscribe` 模式串） | 接口与协议 | 存量 | 01 §2.3；19 §1.3 | `main.c:441` | 驱动热插拔入口 |
| K-018 | `bsf_lock` 初始化 | 机制 | 存量 | 01 §2.3；16 §1.4 | `main.c:448`、`glo.h:36` | 块串行锁的起点 |
| K-019 | `init_dmap`/`init_smap` 调用点 | 机制 | 存量 | 01 §2.3；19 §2.5/§2.8 | `main.c:451-452` | 设备表建立时机 |
| K-020 | boot 服务映射（rproctab 整表取回 + `map_service`） | 机制 | 存量 | 01 §2.3；19 §2.5 | `main.c:455-465` | 启动期端点知识的来源 |
| K-021 | 两遍 fproc 初始化的调用点与分工 | 机制 | 存量 | 01 §2.3；**03 §2.6**；02 §2.6 | `main.c:405-408`、`main.c:468-484` | 为什么清两遍 |
| K-022 | 四张核心表的建立顺序 | 机制 | 存量 | 01 §2.3 | `main.c:486-489` | 启动完成的判定 |
| K-023 | 根挂载门控（`do_init_root` + `worker_allow`） | 机制 | 存量 | 01 §1.4/§2.5；08 §1.3 | `main.c:501-523`、`worker.c:162` | 服务未就绪如何拒绝请求 |
| K-024 | LU 三回调与 worker 停机约束 | 机制 | 存量 | 01 §2.4 | `main.c:303-373`、`worker.c:63` | live update 的约束 |
| K-025 | `lock_proc`/`unlock_proc` 可睡眠锁 | 机制 | 存量 | 01 §2.6；02 §2.5 | `main.c:528-553` | 槽锁快慢路径 |
| K-026 | 构建与链接制品（Makefile SRCS/`-lsys -ltimers -lexec -lmthread`/gcov 条件编译） | 工具工程 | 新增 | —（旧文无） | `servers/vfs/Makefile`、`os/servers/vfs/Cargo.toml` | 知道 VFS 怎么被构建 |
| K-027 | 头文件包含关系（`fs.h` 主包含） | 数据结构 | 存量 | 99 §（散见）；plan §5.2 | `fs.h:1-43` | 文件依赖地图 |

#### 02-fproc-struct（新：02）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-040 | fproc 是四张进程表之一 | 概念 | 存量 | 02 §1.1 | `struct proc/mproc/vmproc` | fproc 的角色边界 |
| K-041 | fproc 全字段分组（状态/目录/fd/阻塞/凭证/锁/请求） | 数据结构 | 存量 | 02 §2.1/§3.6 | `fproc.h:15-82` | 字段清单与心智分组 |
| K-042 | `fp_flags` 六位（SRV_PROC/REVIVED/SESLDR/PENDING/EXITING/PM_WORK） | 数据结构 | 存量 | 02 §2.2 | `fproc.h:91-98` | 进程状态位契约 |
| K-043 | `fp_blocked_on` 七态 | 数据结构 | 存量 | 02 §2.3；`const.h:19-25` | `fproc.h:29` | 阻塞原因枚举 |
| K-044 | `fp_u` 五类载荷与各自恢复方 | 数据结构 | 存量 | 02 §2.3 | `fproc.h:30-61`、`pipe.c:435`、`lock.c:173`、`select.c:783`、`cdev.c:481`、`sdev.c:759` | 每类阻塞的恢复路径 |
| K-045 | 挂起唯一入口 `suspend()` 与不可叠加不变量 | 约束与不变量 | 存量 | 02 §2.3；17 §2.4 | `pipe.c:294-311` | 阻塞状态机的正确性根基 |
| K-046 | 凭证五字段与判定语义（real/eff uid/gid、补充组、umask） | 数据结构 | 存量 | 02 §2.4；29 §2.6 | `fproc.h:63-69`、`protect.c:255-256` | 权限检查的输入 |
| K-047 | `fp_lock` 属于槽位（fork 不共享） | 约束与不变量 | 存量 | 02 §2.5；10 §1.3 | `fproc.h:71`、`misc.c:606-608` | 槽锁的归属 |
| K-048 | 请求执行期四元组（`fp_worker`/`fp_func`/`fp_msg`/`fp_pm_msg`） | 数据结构 | 存量 | 02 §2.5；08 §2.4 | `fproc.h:72-75`、`worker.c:119-137` | 活动请求的状态载体 |
| K-049 | `fp_name` 记名 | 数据结构 | 存量 | 02 §2.1 | `fproc.h:77` | 进程名的来源 |
| K-050 | BlockedOn 标签枚举（A-3） | 架构演进 | 存量 | 02 §3.1 | `fproc.rs:BlockedOn` | 判别式与载荷的类型合并 |
| K-051 | 字段类型化（A-8：Uid/Gid/DevId/Mode/GrantId） | 架构演进 | 存量 | 02 §3.2；99 §4 | `minix-types/types/id.rs` | 裸标量消除 |
| K-052 | `FProcTable` 堆存储规避大栈帧 | 工具工程 | 存量 | 02 §4.4 | `fproc.rs:FProcTable` | 1.09 MiB 表的存放选择 |
| K-053 | 哨兵 vs `Option` 的取舍 | 架构演进 | 存量 | 02 §3.5 | `fproc.rs`、`minix-types` | C 哨兵语义的映射纪律 |

#### 03-fproc-table（新：03）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-060 | `NR_PROCS` 固定表与内核同界 | 数据结构 | 存量 | 03 §1.1 | `fproc.h:10-12,82`、`sys_config.h` | 为什么用定容数组 |
| K-061 | `PID_FREE`/`NONE` 双哨兵与空闲判定 | 约束与不变量 | 存量 | 03 §1.2/§2.2 | `fproc.h:101-103`、`main.c:405-408` | 空闲槽判据 |
| K-062 | `isokendpt` 三守卫（越界/哨兵/失配） | 机制 | 存量 | 03 §1.3/§2.3 | `utility.c:94-127` | 端点消息的可信验证 |
| K-063 | 致命分化（`isokendpt_f` 的 panic 与 EDEADEPT 两用） | 接口与协议 | 存量 | 03 §1.4/§2.4 | `utility.c:119-120`、`proto.h:357-358` | 调用点意图 |
| K-064 | `fproc_addr`/`who_p`/`who_e` 宏与 O(1) 索引 | 接口与协议 | 存量 | 03 §2.5；09 §2.1 | `glo.h:26-27` | 索引即权威 |
| K-065 | 两遍初始化的表级语义（什么时候能判空闲） | 机制 | 存量 | 03 §2.6；**02 §2.6** 并入 | `main.c:405-408,468-484` | 清零时序 |
| K-066 | `fproc_light` 轻量快照（三字段）与 MIB 消费 | 数据结构 | 存量 | 03 §2.7；02 §2.7；10 §D6 | `fproc.h:111-115`、`misc.c:52-96` | 观测的低成本投影 |
| K-067 | `fproc_light` 的 A-7 缺口与生产面（FProcSnap 52B） | 架构演进 | 新增 | —（旧文标缺口） | `minix-types/types/fproc.rs`、`fproc.rs:to_fproc_snap` | producer 现状 |

#### 04-filp-table（新：04）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-080 | filp 中介三不变量（fd→filp→vnode） | 概念 | 存量 | 04 §1.1 | `file.h:5` | 为什么需要中间层 |
| K-081 | `struct filp` 字段全景 | 数据结构 | 存量 | 04 §2.1 | `file.h:8-33` | 字段清单 |
| K-082 | `NR_FILPS` 容量与 `init_filps` | 数据结构 | 存量 | 04 §2.2 | `const.h:5`、`filedes.c:73-86` | 表规模 |
| K-083 | filp 引用计数共享语义（fork/dup 的分化） | 约束与不变量 | 存量 | **04 §1.2/§2.6**；02 §1.4；10 §1.3 | `file.h:11`、`misc.c:629` | 共享与独享的边界 |
| K-084 | `get_filp`/`get_filp2` 与 `FILP_CLOSED` 门 | 机制 | 存量 | 04 §2.4；14 §2.3 | `filedes.c:162-199` | 访问特权（OPCL 例外） |
| K-085 | `find_filp`/`find_filp_by_sock_dev` | 机制 | 存量 | 04 §2.5 | `filedes.c:205-246` | 共享检测与套接字反查 |
| K-086 | `close_filp` 归零与 `put_vnode` | 机制 | 存量 | 04 §2.6；14 §2.7 | `filedes.c:414-519` | 生命周期闭环 |
| K-087 | filp 三态锁（`filp_lock`/`softlock`/`ioctl_fp`） | 数据结构 | 存量 | 04 §1.4/§2.7；14 §2.6 | `file.h:14-18`、`filedes.c:313-380` | 借用与自锁 |
| K-088 | `FSF_*` 位集与选择字段 | 数据结构 | 存量 | 04 §1.5/§2.8；23 §1.4 | `file.h:26-48` | 驱动协同状态 |
| K-089 | `FilpTable`/位集/`FilpId` 类型化 | 架构演进 | 存量 | 04 D1-D5 | `filp.rs` | 改写要点 |

#### 05-vnode-table（新：05）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-100 | vnode 缓存与 inode 投影 | 概念 | 存量 | 05 §1.1 | `vnode.h:4-23` | 缓存定位 |
| K-101 | 双层引用计数（`v_ref_count`/`v_fs_count`）与 256 阈值 | 数据结构 | 存量 | **05 §1.2/§2.6**；02/04 散见 | `vnode.h:13-14`、`vnode.c:263-264,305-315` | 延迟同步的经济学 |
| K-102 | `get_free_vnode`/`find_vnode` 条件 | 机制 | 存量 | 05 §2.3/§2.4 | `vnode.c:85-124` | 分配与命中 |
| K-103 | `dup_vnode`/`put_vnode` 快慢路径 | 机制 | 存量 | 05 §2.6 | `vnode.c:227-299` | 生命周期 |
| K-104 | vnode 锁族与升级（`VNODE_*`→TLL） | 机制 | 存量 | 05 §2.5；07 §2.7 | `vnode.h:26-29`、`vnode.c:156-224` | 读写锁协议 |
| K-105 | 设备与挂载关联字段（`v_dev`/`v_vmnt`/`v_sdev`/`v_bfs_e`） | 数据结构 | 存量 | 05 §2.7 | `vnode.h:16-21` | 设备语义四元组 |
| K-106 | 分配时清零（`v_sdev`/`v_mapfs_e`） | 机制 | 存量 | 05 §2.3 | `vnode.c:92-97` | 字段复位 |
| K-107 | `vnode_clean_refs` 阈值回收 | 机制 | 存量 | 05 §2.6 | `vnode.c:305-315` | 防计数环绕 |
| K-108 | `VnodeTable`/锁类型化 | 架构演进 | 存量 | 05 D1-D5 | `vnode.rs` | 改写要点 |

#### 06-vmnt-table（新：06）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-115 | vmnt 边界表（device→FS 映射） | 概念 | 存量 | 06 §1.1 | `vmnt.h:7-21` | 挂载边界 |
| K-116 | `NR_MNTS=16`（纠正旧文 8） | 数据结构 | 存量（纠错） | 06 §1.1/§2.2（旧文写 8） | `const.h:7` | 容量真值 |
| K-117 | `get_free_vmnt`/`find_vmnt` | 机制 | 存量 | 06 §2.3/§2.4 | `vmnt.c:95-120` | 分配与命中 |
| K-118 | vmnt 锁族与 EXCL 的 WRITE 化、EDEADLK 自锁拒 | 机制 | 存量 | 06 §2.5；07 §2.7 | `vmnt.h:31-33`、`vmnt.c:150-197` | 排他语义 |
| K-119 | `mark_vmnt_free` 与 `clear_vmnt` 的分化 | 机制 | 存量 | 06 §2.6 | `vmnt.c:65-90` | 快速失效与全清 |
| K-120 | `vmnt_unmap_by_endpt` 四步级联 | 机制 | 存量 | 06 §2.7；19 §2.7 | `vmnt.c:180-192` | FS 崩溃回收 |
| K-121 | 挂载元数据字段（label/path/fstype/statvfs 缓存） | 数据结构 | 存量 | 06 §2.1；28 §2.8 | `vmnt.h:14-20` | 挂载描述 |
| K-122 | `m_comm` 字段归属（窗口归 11） | 数据结构 | 存量 | 06 §2.1（旧文展开）；**11 §1.2** | `type.h:1-10` | 边界收束 |
| K-123 | `fetch_vmnt_paths` 系 C 死代码的判定 | 工具工程 | 存量 | 06 §1.5/§2.7；99 省略表 | `vmnt.c:246`、`proto.h:371` | 不移植死代码 |
| K-124 | `VmntTable`/`VmntLock` 类型化 | 架构演进 | 存量 | 06 D1-D5 | `vmnt.rs` | 改写要点 |

#### 07-tll-lock（新：07）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-130 | 三级锁三态（NONE/READ/READSER/WRITE） | 概念 | 存量 | 07 §1.1 | `tll.h:6` | 读并发/串行读/独占 |
| K-131 | `tll_t` 状态字段与正交性（current/status/readonly） | 数据结构 | 存量 | 07 §1.2/§2.1 | `tll.h:9-18` | 状态机三维 |
| K-132 | 等待队列写偏序（write_q 优先 serial_q） | 机制 | 存量 | 07 §1.3/§2.5 | `tll.c:11-71`、`tll.c:230-303` | 写不被饿死 |
| K-133 | `tll_lock` 五路分发 | 机制 | 存量 | 07 §2.4 | `tll.c:139-219` | 直授与 EBUSY |
| K-134 | `tll_unlock` 队头唤醒与 UPGR/PEND 标记 | 机制 | 存量 | 07 §2.6 | `tll.c:230-303` | 唤醒选择 |
| K-135 | `tll_downgrade`/`tll_upgrade` 时序 | 机制 | 存量 | 07 §2.7 | `tll.c:74-111,306-323` | 升降级 |
| K-136 | 三谓词与空锁不变量 | 约束与不变量 | 存量 | 07 §1.5/§2.3 | `tll.c:113-136,220-221` | is_locked 语义 |
| K-137 | `TllError` 与非阻塞契约（A-6） | 架构演进 | 存量 | 07 D1-D5 | `tll.rs:TllError` | 单线程下锁的降级 |

#### 08-worker-thread（新：08）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-145 | mthread 九线程 → 请求槽状态机（A-1） | 架构演进 | 存量 | 08 §1.1/D1 | `worker.c`、`worker.rs:WorkerState` | 执行模型的全阶段最大改写 |
| K-146 | 两级并发（`pending` 进程级 / `busy` 线程级） | 数据结构 | 存量 | 08 §1.2 | `worker.c:10-11` | 排队与绑定的区分 |
| K-147 | `w_fp`↔`fp_worker` 双向绑定 | 数据结构 | 存量 | 08 §1.4 | `threads.h:27`、`worker.c:138,283-286` | O(1) 可用性查询 |
| K-148 | `worker_start` 登记与 sanity（pending/active 互斥） | 机制 | 存量 | 08 §2.6/§2.9 | `worker.c:360-428` | 请求登记 |
| K-149 | `block_all`/`worker_allow` 两阶段门控 | 机制 | 存量 | **08 §1.3**；01 §2.5 | `worker.c:162-185` | 根挂载期排队 |
| K-150 | `worker_can_start` 四象限 | 机制 | 存量 | 08 §1.4/§2.7 | `worker.c:295-325` | DS 事件安全守门 |
| K-151 | `suspend`/`resume` 协程三件套 | 机制 | 存量 | 08 §1.5/§2.9 | `worker.c:474-504` | `err_code` 可逆保存 |
| K-152 | `wait`/`signal` 睡眠队列 | 机制 | 存量 | 08 §1.5/§2.9；07 | `worker.c:510-530` | tll 阻塞的落点 |
| K-153 | `worker_yield`/`set_proc` 上下文交接 | 机制 | 存量 | 08 §2.9 | `worker.c:431-438,586-607` | 仅 reboot 可用 |
| K-154 | `worker_stop`/`stop_by_endpt` EIO 注入 | 机制 | 存量 | 08 §2.9；19 §2.7 | `worker.c:535-567` | 对端退出快速失败 |
| K-155 | `thread_cleanup` 清 `VMNT_CALLBACK` | 机制 | 存量 | **08 §2.10**；09 §2.6 | `main.c:558-575` | 回调标志回收 |
| K-156 | 线程栈尺寸（`TH_STACKSIZE` 三档；Rust 消除） | 约束与不变量 | 存量 | 08 §2.2 | `worker.c:15-20` | 内存预算参照 |
| K-157 | `WorkerPool`/`WorkerFunc`/`SuspendToken` 类型化 | 架构演进 | 存量 | 08 D2/D3/D5 | `worker.rs` | 改写要点 |

#### 09-main-loop（新：09）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-165 | 八路分发优先级不可交换 | 约束与不变量 | 存量 | 09 §1.2/§2.3 | `main.c:80-138` | 路由正确性的根基 |
| K-166 | `reviving` 优先（复活不饥饿） | 机制 | 存量 | 09 §1.1/§2.4 | `main.c:590-597` | 挂起进程的公平性 |
| K-167 | FS 异步回复的 transid 路由 | 机制 | 存量 | 09 §1.2/§2.7；**11 §1.3** | `main.c:80-90,187-211` | worker 如何被找回 |
| K-168 | PM 守门（`who_e==PM_PROC_NR`） | 机制 | 存量 | 09 §1.2 | `main.c:91-94` | 控制面旁路 |
| K-169 | notify 三路（DS/KERNEL/CLOCK） | 机制 | 存量 | 09 §1.2 | `main.c:95-117` | 事件分流 |
| K-170 | 内核任务消息忽略（`who_p<0`） | 机制 | 存量 | 09 §1.2 | `main.c:118-124` | 任务只许 notify |
| K-171 | 设备回复三路判定宏 | 机制 | 存量 | **09 §1.2**；20/21/22 | `main.c:126-134`、`com.h:922,963,1041` | RS 前缀识别 |
| K-172 | `call_vec[64]` 平表、NULL 哨兵、`RMDIR→do_unlink` 别名 | 数据结构 | 存量 | 09 §1.3/§2.2 | `table.c:17-82` | O(1) 分发与别名 |
| K-173 | `handle_work` 的 CALLBACK/可用线程守门 | 机制 | 存量 | 09 §2.6 | `main.c:146-181` | FS 回调死锁防护 |
| K-174 | `do_work` 与 `SUSPEND` 回复契约 | 接口与协议 | 存量 | 09 §1.4/§2.8 | `main.c:263-298` | 何时回复、何时挂起 |
| K-175 | `reply`/`replycode` 非阻塞发送 | 机制 | 存量 | 09 §2.9 | `main.c:638-663` | 回复失败只打印 |
| K-176 | `unblock` 二路重建（管道重放/锁整请求） | 机制 | 存量 | 09 §2.5；17/30 | `main.c:921-973` | 复活机制 |
| K-177 | `VfsState` 聚合（A-4） | 架构演进 | 存量 | 09 §1.5/D5 | `main_loop.rs:VfsState` | 全局状态收敛 |
| K-178 | `Route`/`PollResult` 类型化 | 架构演进 | 存量 | 09 D1/D6 | `main_loop.rs` | 优先级可审计 |
| K-179 | `ENABLE_SYSCALL_STATS` 编译开关 | 工具工程 | 存量 | 09 §1.3/§2.2；31 §D2；**33 §K-416** | `main.c:32-34,286-289` | 统计埋点的 cfg 语义 |

#### 10-pm-protocol（新：10）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-185 | PM 是 mproc 权威、VFS 是 fproc 权威 | 概念 | 存量 | 10 §1.1 | `com.h:59`、`fproc.h:11` | 两侧表同界 |
| K-186 | `VFS_PM_*` 12 请求与 11 回复 | 数据结构 | 存量 | 10 §2.1；99 §1.3 | `com.h:513-544` | 协议面全表 |
| K-187 | 三级调度（立即 6 路/延期 4 路/独立 1 路 + INIT） | 机制 | 存量 | 10 §1.2/§2.2（旧文计数错） | `main.c:783-915` | 阻塞目标进程的串行化 |
| K-188 | `service_pm_postponed` 四分支 | 机制 | 存量 | 10 §2.3 | `main.c:668-762` | 延期请求的消费 |
| K-189 | `pm_fork` 四步共享（锁保留/fd 计数/vnode dup/标志重置） | 机制 | 存量 | 10 §1.3/§2.4 | `misc.c:577-634` | fork 次主线核心 |
| K-190 | `SRV_FORK` 追加 setuid/setgid | 机制 | 存量 | 10 §2.2 | `main.c:867-871` | 服务身份注入 |
| K-191 | `free_proc` 两阶段与 `FP_EXITING` 分水岭 | 机制 | 存量 | 10 §1.4/§2.5 | `misc.c:639-708` | 退出回收 |
| K-192 | SESLDR 的 tty 撤销级联 | 机制 | 存量 | 10 §2.5 | `misc.c:683-700` | 会话首领退出 |
| K-193 | 凭证四请求（setuid/setgid/setgroups/setsid） | 接口与协议 | 存量 | 10 §1.5/§2.6 | `misc.c:726-791` | 凭证单写入口 |
| K-194 | `pm_reboot` 八步序列 | 机制 | 存量 | 10 §2.7 | `misc.c:504-575` | 重启/关机 |
| K-195 | `pm_dumpcore` 入口与收尾 | 机制 | 存量 | 10 §2.7；26 §2.10（收束到 10） | `misc.c:903-943` | core 触发链 |
| K-196 | 进程退出的资源回收顺序总表（关闭与退出主家） | 机制 | 存量 | 10 §2.5（旧文散在 14/05） | `misc.c:639-708`、`open.c:690`、`vnode.c:240` | 级联清单可核对 |
| K-197 | `PmHandler` 类型化与 fail-closed 缺口语义 | 架构演进 | 存量 | 10 D1-D5 | `ipc/dispatcher.rs` | 改写要点 |

#### 11-fs-comm（新：11）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-205 | 每挂载窗口 `comm_t`（max/cur/queue） | 数据结构 | 存量 | 11 §1.1/§2.1 | `type.h:1-10`、`comm.c:41` | 多 FS 计数隔离 |
| K-206 | `sendmsg`/`send_work`/`fs_sendmore` | 机制 | 存量 | 11 §2.2/§2.3/§2.5 | `comm.c:12-84` | 窗口占用与补发 |
| K-207 | `fs_cancel`/`queuemsg` | 机制 | 存量 | 11 §2.4/§2.10 | `comm.c:50-61,223-244` | 崩溃清理与排队 |
| K-208 | `fs_sendrec`/`drv_sendrec`/`vm_sendrec` 三类对端 | 机制 | 存量 | 11 §2.6-2.9 | `comm.c:89-218` | 三类等待通道 |
| K-209 | CALLBACK/窗口/EDEADLK 三重守门 | 约束与不变量 | 存量 | 11 §2.5/§2.7 | `comm.c:66-84,134-168` | 死锁防护 |
| K-210 | `VFS_TRANSID` 编码（低 16 位 id，高 16 位原类型） | 接口与协议 | 存量 | **11 §1.3/§2.11**；09 §2.10（方向写反） | `com.h:909-912`、`vfsif.h:79-81` | 异步回复定位 |
| K-211 | ERESTART→EIO 抑制 | 机制 | 存量 | 11 §2.7/D5 | `comm.c:165` | 内部哨兵不外泄 |
| K-212 | `GlobalComm`/`FsComm`/`TransId` 类型化 | 架构演进 | 存量 | 11 D1/D2 | `fs_comm.rs` | 改写要点 |

#### 12-request-wrappers（新：12）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-220 | `REQ_*` 33 常量=32 活+1 死；`FS_BASE=0xA00`；`IS_FS_RQ` 掩码 | 接口与协议 | 存量（纠错） | 12 §1.2/§2.1；99 §1.3 | `vfsif.h:41-77`、`com.h:589` | wire 绝对真值 |
| K-221 | `req_*` 包装的三重复收敛 | 机制 | 存量 | 12 §1.1 | `request.c` 全文件 | 调用方只关心语义 |
| K-222 | grant 两段与 `ERESTART` 重试 | 机制 | 存量 | 12 §1.4/§2.3 | `request.c:38-80` | 用户缓冲授权的重试机制 |
| K-223 | `node_details`/`lookup_res` 响应结构 | 数据结构 | 存量 | **12 §1.3/§2.2**；13 §2.6（消费） | `request.h:12,25`、`vfsif.h:26-28` | 响应回填字段 |
| K-224 | `RES_*` 能力协商（THREADED/HASPEEK/64BIT） | 约束与不变量 | 存量 | 12 §1.5/D4 | `vfsif.h:20-23`、`request.c:274,323` | 32 位 FS 早拒绝 |
| K-225 | 响应回填的 `_actual` 重试后半 | 机制 | 存量 | 12 §2.4-2.9 | `request.c` 各 `_actual` | 重试后的状态回填 |
| K-226 | `REQ_GETNODE` 死常量排除 | 工具工程 | 存量 | 12 D6；99 省略表 | `vfsif.h:41` | 不复活死协议 |
| K-227 | `FsReq`/`FsResp`/`GrantScope` 类型化 | 架构演进 | 存量 | 12 D1/D2/D5 | `request.rs` | 改写要点 |
| K-228 | REQ 契约已收敛 `minix-types::ipc::fs_driver`（E-REQWIRE 落地） | 架构演进 | 新增 | —（旧文按本地定义写） | `os/libs/minix-types/src/ipc/fs_driver.rs` | 单一权威的新事实 |

#### 13-path-lookup（新：13）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-235 | 路径解析是名字类调用的公共前置 | 概念 | 存量 | 13 §1.1 | `path.c:1-39` | 调用链位置 |
| K-236 | `lookup` 结构六字段与 `PATH_*` 标志 | 数据结构 | 存量 | 13 §2.1（旧文末尾写 5 字段） | `path.h:4-12` | 解析状态载体 |
| K-237 | `advance` 双相（命中递增/建表落缓存） | 机制 | 存量 | 13 §2.3 | `path.c:40-127` | vnode 缓存的来源 |
| K-238 | `eat_path`/`last_dir`（起点选择与末组件切分） | 机制 | 存量 | 13 §2.4/§2.5 | `path.c:133-380` | 解析机主体 |
| K-239 | 跨挂载穿越（`EENTERMOUNT`/`ELEAVEMOUNT`/`ESYMLINK`） | 机制 | 存量 | 13 §1.2/§2.6 | `path.c:384-573`、`vfsif.h:26-28` | 单树跨 FS 的缝合点 |
| K-240 | 符号链接循环与 `_POSIX_SYMLOOP_MAX`=16 | 约束与不变量 | 存量（纠错） | 13 §1.3（旧文锚 `const.h:32`） | `path.c:349,468`、POSIX 头 | 环防护 |
| K-241 | 解析期锁协议（VMNT READ→WRITE、VNODE READ→OPCL/降级） | 约束与不变量 | 存量 | 13 §1.4（旧文越界讲 tll） | `path.c:54-61,435,554` | 穿越期锁语义 |
| K-242 | `canonical_path`/`get_name` | 机制 | 存量 | 13 §2.8 | `path.c:594-798` | 绝对化与目录项反查 |
| K-243 | `copy_path`/`fetch_name` 用户路径取回与错误码 | 接口与协议 | 存量（纠错） | 13 §2.2/§2.10（旧文错码） | `utility.c:24-93` | 路径进入内核的第一站 |
| K-244 | `DO_POSIX_PATHNAME_RES=0` 历史行为 | 架构演进 | 存量 | 13 §2.9；plan A-10 | `path.c:28-35` | 尾斜杠行为显式 |
| K-245 | `do_socketpath` 入口（归 13，语义归 24） | 接口与协议 | 存量 | 13 §2.9；24 §7 | `path.c:803-933` | 路径式套接字入口 |

#### 14-filedes（新：14）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-250 | fd 私有索引 vs filp 共享池 | 概念 | 存量 | 14 §1.1 | `fproc.h:24`、`file.h` | 两跳模型 |
| K-251 | `get_fd` 最低空闲分配与 EMFILE/ENFILE | 机制 | 存量 | **14 §1.2/§2.2**；04 §2.3（重复） | `filedes.c:110-156` | fd 分配语义 |
| K-252 | `check_fds` 的 nfds 窗口 | 机制 | 存量 | 14 §1.3/§2.1 | `filedes.c:88-105` | select 预检 |
| K-253 | `close_fd` 三段拆除序（先摘索引） | 机制 | 存量 | **14 §1.4/§2.8**；15 §2.8（重复） | `open.c:690-727` | 拆除正确性 |
| K-254 | `invalidate_filp` 三族（char_major/sock_drv/endpt） | 机制 | 存量 | **14 §1.6/§2.5**；06/10/19 散见 | `filedes.c:250-308` | 端点失效传播 |
| K-255 | `do_copyfd` FROM/TO/CLOSE 与四项守门 | 接口与协议 | 存量 | 14 §1.5/§2.9 | `filedes.c:524-656` | 驱动间 fd 传递 |
| K-256 | `Fd`/`FilpLockMode`/`CopyFdCtx` 类型化 | 架构演进 | 存量 | 14 D1-D6 | `filedes.rs` | 改写要点 |
| K-257 | 测试替身与虚构第二实现的清理纪律 | 工具工程 | 存量 | 14 D2/§5；todo R2-P1-1 | `filedes.rs:NextFitDemo`（cfg(test)） | 抽象纪律的先例 |

#### 15-open-close（新：15）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-265 | open/creat 对偶入口（copy_path vs fetch_name） | 接口与协议 | 存量 | 15 §1.2/§2.2 | `open.c:38-78` | 收名方式的差异 |
| K-266 | `mode_map` 访问意图两比特编码 | 数据结构 | 存量 | 15 §1.3/§2.1 | `open.c:29,97-99` | 0/1/2/非法四值 |
| K-267 | `common_open` 七步管线与六分支分派 | 机制 | 存量 | 15 §1.4/§2.3 | `open.c:83-293` | 主管线 |
| K-268 | `new_node` 创建机（EEXIST/符号链接重解/七字段落定） | 机制 | 存量 | 15 §2.4 | `open.c:299-477` | 创建路径 |
| K-269 | `pipe_open` 读写配对 | 机制 | 存量 | **15 §2.5**；17 §2.1 | `open.c:483-508` | 管道配对 |
| K-270 | `do_mknod`/`do_mkdir` 与特权门 | 接口与协议 | 存量 | 15 §2.6 | `open.c:514-598` | FIFO 放行与 EPERM |
| K-271 | `actual_lseek` 三原点与溢出双守卫 | 机制 | 存量 | 15 §1.5/§2.7 | `open.c:603-669` | 定位语义 |
| K-272 | 关闭三段拆除序（语义归 14，调用点归 15） | 机制 | 存量 | 15 §1.6（旧文与 14 重复） | `open.c:674-727` | 关闭的调用序 |
| K-273 | `AccessMode`/`checked_add` 类型化 | 架构演进 | 存量 | 15 D1/D5 | `open.rs` | 改写要点 |

#### 16-read-write（新：16）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-280 | 读写偷看三向同机 | 概念 | 存量 | 16 §1.1/§2.1 | `read.c:127-251`、`write.c:15-24` | 三方向一条管线 |
| K-281 | 五路分派（PIPE/CHR/SOCK/BLK/REG） | 机制 | 存量 | 16 §1.3/§2.4 | `read.c:154-251` | 数据面路由 |
| K-282 | 头校验三闩与零字节短路 | 约束与不变量 | 存量 | 16 §2.3 | `read.c:92-116` | 校验顺序 |
| K-283 | 位置推进与 O_APPEND 起点重定 | 机制 | 存量 | 16 §1.2/§2.4 | `read.c:145,234,262` | 返回值即推进量 |
| K-284 | bsf 全局锁（快慢道与卸载断言） | 机制 | 存量 | 16 §1.4/§2.2 | `read.c:49-87`、`glo.h:36` | 块设备串行 |
| K-285 | 偷看不推进位置 | 机制 | 存量 | 16 §1.5 | `read.c:219-239` | peek 语义 |
| K-286 | `do_getdents` 目录读 | 接口与协议 | 存量 | 16 §2.6 | `read.c:282-317` | 目录项读取 |
| K-287 | SIGPIPE 击发矩阵 | 机制 | 存量 | 16 §2.5 | `read.c:264-271` | 管道写异常信号 |
| K-288 | `BsfLock`/`BsfGuard` Drop 配对 | 架构演进 | 存量 | 16 D2 | `read_write.rs` | 改写要点 |

#### 17-pipe（新：17）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-295 | 管道等待的三处境 | 概念 | 存量 | 17 §1.1 | `pipe.c:195-199` | 为何必须等 |
| K-296 | `pipe_check` 读三路、写五路 | 机制 | 存量 | **17 §2.3**；16 §2.7（重复） | `pipe.c:187-288` | 定量机 |
| K-297 | `create_pipe` 七步与回滚边界 | 机制 | 存量 | 17 §1.2/§2.1 | `pipe.c:60-144` | 建管 |
| K-298 | `map_vnode` 幂等映射 | 机制 | 存量 | **17 §2.2**；15 §2.3（重复） | `pipe.c:151-182` | PFS 映射 |
| K-299 | `suspend`/`pipe_suspend` 登记五参数 | 机制 | 存量 | 17 §2.4；02 §2.3 | `pipe.c:294-328` | 挂起现场保存 |
| K-300 | `susp_count`/`reviving` 全局账本 | 数据结构 | 存量 | 17 §1.4 | `glo.h:14,16` | 全服等待者计数 |
| K-301 | `release` 扫描与唤醒谓词六元合取 | 机制 | 存量 | 17 §2.5/D4 | `pipe.c:363-429` | 精确唤醒 |
| K-302 | `revive`/`unpause` 标记与中断 | 机制 | 存量 | 17 §2.6/§2.7 | `pipe.c:435-561` | 两阶段复活 |
| K-303 | `unsuspend_by_endpt` 驱散分类 | 机制 | 存量 | 17 §2.8；19/22 | `pipe.c:334-357` | 端点死亡唤醒 |
| K-304 | 匿名/命名管道统一于 PFS | 概念 | 存量 | 17 §1.6 | `pipe.c:101-102` | 统一 |
| K-305 | `SuspLedger`/`release_match`/`MapVerdict` 类型化 | 架构演进 | 存量 | 17 D3/D4/D7 | `pipe.rs` | 改写要点 |

#### 18-mount（新：18）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-310 | 挂载嫁接模型（单树跨 FS） | 概念 | 存量 | 18 §1.1 | `mount.c:161-166`、`vmnt.h:14-15` | 挂载的本质 |
| K-311 | 设备号编解码三宏 | 数据结构 | 存量 | 18 §1.2/§2.1 | `sys/types.h` major/minor/makedev | 往返律 |
| K-312 | 伪设备与 nonedev 位图 | 数据结构 | 存量 | 18 §1.3/§2.2/§2.3 | `mount.c:33-37,628-653`、`const.h:12` | 无盘 FS 的占位 |
| K-313 | `mount_fs` 提交五段与回滚边界 | 机制 | 存量 | 18 §1.4/§2.6 | `mount.c:156-385` | 挂载主体 |
| K-314 | EBUSY 双义（设备已挂/目录忙） | 约束与不变量 | 存量 | 18 §1.4 | `mount.c:191-196,218` | 同码不同主体 |
| K-315 | `update_bspec` 路由改道（尽力而为） | 机制 | 存量 | 18 §2.4 | `mount.c:46-80` | 挂载后旧 bspec 处理 |
| K-316 | `do_mount` 入口守门链 | 接口与协议 | 存量 | 18 §2.5 | `mount.c:85-150` | 超管/标签/端点/名长 |
| K-317 | `have_root`/`MAKEROOT` 根特例 | 机制 | 存量 | 18 §1.6 | `mount.c:31,205-209,327-328` | 根可挂两次 |
| K-318 | `mount_pfs` 三标签罐装挂载 | 机制 | 存量 | **18 §2.7**；01 §2.3（调用点） | `mount.c:391-425` | PFS 固定身份 |
| K-319 | `do_umount`/`unmount` 七步拆除、`unmount_all` 扫荡 | 机制 | 存量 | 18 §1.5/§2.8/§2.9 | `mount.c:430-585` | 卸载与关机 |
| K-320 | `name_to_dev` 三分类 | 机制 | 存量 | 18 §2.10 | `mount.c:590-622` | 设备名解析 |
| K-321 | `RootStage`/`MountPhase`/`UnmountPlan` 类型化 | 架构演进 | 存量 | 18 D3/D5/D6 | `mount.rs` | 改写要点 |

#### 19-device-map（新：19）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-330 | dmap 表与"号人分离" | 数据结构 | 存量 | 19 §1.1/§2.1 | `dmap.h:16-25` | major↔endpoint 路由 |
| K-331 | CTTY major=5 自管 | 机制 | 存量 | 19 §1.6/§2.1 | `dmap.h:26`、`dmap.c:244-246` | 控制终端特例 |
| K-332 | smap 表与套接字号拼合 | 数据结构 | 存量 | 19 §2.8/§2.10 | `type.h:41-49`、`smap.c:201-213` | 套接字寻址 |
| K-333 | `map_driver`/`do_mapdriver` 增删与双表回滚 | 机制 | 存量 | 19 §2.3/§2.4 | `dmap.c:61-175` | 驱动注册 |
| K-334 | `map_service`/`init_dmap` 启动映射 | 机制 | 存量 | 19 §2.5 | `dmap.c:200-246` | boot 服务入表 |
| K-335 | `dmap_endpt_up` 恢复三态 | 机制 | 存量 | 19 §2.7 | `dmap.c:275-312` | 驱动重启续命 |
| K-336 | `smap_map`/`smap_unmap_by_endpt` 注册与驱散 | 机制 | 存量 | 19 §2.9/§2.10 | `smap.c:44-187` | 套接字驱动生命周期 |
| K-337 | `do_ioctl` 三路分派 | 接口与协议 | 存量 | 19 §2.11 | `device.c:18-59` | 控制命令路由 |
| K-338 | `make_ioctl_grant` 方向交叉与尺寸分流 | 机制 | 存量 | **19 §2.12**；21 §2.6；22 §2.7 | `device.c:65-95` | 授权解码 |
| K-339 | `ds_event` 驱动事件分类 | 机制 | 存量 | 19 §1.3/§2.7（旧文薄） | `misc.c:949-988` | 热插拔处理 |
| K-340 | 驱动死亡触发总纲（unmap→失效→停尸→唤醒） | 约束与不变量 | 存量（收束） | 19 §2.7；14/21/22/23 重复 | `dmap.c:275`、`filedes.c:250-308`、`sdev.c:912`、`select.c:884` | 级联的统一起点 |
| K-341 | `EndpointDirectory`/注册计划/恢复 verdict 类型化 | 架构演进 | 存量 | 19 D1-D6 | `device_map.rs` | 改写要点 |

#### 20-bdev（新：20）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-350 | 块开/关/控直达，读写走 FS 的管辖分工 | 概念 | 存量 | 20 §1.1 | `bdev.c:1-9` | 路径归属 |
| K-351 | 重试五次与 ERESTART 恢复 | 机制 | 存量 | 20 §1.2/§2.1 | `bdev.c:34-73` | 重试熔断 |
| K-352 | 死信三分类（死/锁死/其他） | 机制 | 存量 | 20 §1.3/§2.1 | `bdev.c:60-70` | 失败分类 |
| K-353 | `bdev_reply` 三重门（来源/任务/配对） | 约束与不变量 | 存量 | 20 §1.4/§2.4 | `bdev.c:193-220` | 回复校验 |
| K-354 | `bdev_up` 两轮通知（重开轮遇错全弃/通告轮尽力） | 机制 | 存量 | 20 §1.5/§2.5 | `bdev.c:227-282` | 驱动换人 |
| K-355 | bsf 守卫设→调→清 | 约束与不变量 | 存量 | 20 §1.6；16/19 | `device.c:36-40` | ioctl 与读写不交错 |
| K-356 | `SendTransport`/`RetryState`/`ReplyCheck` 类型化 | 架构演进 | 存量 | 20 D1-D6 | `bdev.rs` | 改写要点 |

#### 21-cdev（新：21）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-365 | 字符挂起与块同步的分叉 | 概念 | 存量 | 21 §1.1 | `cdev.c:1-9` | 开合不可挂、读写可挂 |
| K-366 | tty 改道（`/dev/tty` 魔术设备） | 机制 | 存量 | 21 §1.2/§2.1 | `cdev.c:36-56` | 代词解析 |
| K-367 | `cdev_get` 查表四步（dmap/驱动存活/端点复核） | 机制 | 存量 | 21 §2.2 | `cdev.c:63-89` | 表项校验 |
| K-368 | `cdev_clone` 打开成功换号 | 机制 | 存量 | 21 §1.3/§2.3 | `cdev.c:97-146` | pty 的诞生 |
| K-369 | `cdev_opcl` 开合对话与 NOCTTY 三条件 | 机制 | 存量 | 21 §1.4/§2.4 | `cdev.c:149-251` | 控制终端归属 |
| K-370 | `cdev_io` 挂起登记与授权方向交叉 | 机制 | 存量 | 21 §1.5/§2.6 | `cdev.c:279-341` | 读写挂起 |
| K-371 | `cdev_cancel`/`cdev_reply` 的 EAGAIN↔EINTR 换码 | 机制 | 存量 | 21 §1.6/§2.8/§2.9 | `cdev.c:381-508` | 取消与复活 |
| K-372 | `cdev_select` 无改道旁路 | 机制 | 存量 | 21 §2.7 | `cdev.c:350-374` | select 路径差异 |
| K-373 | `tty_redirect`/`ReplyClass`/`OpenEffects` 类型化 | 架构演进 | 存量 | 21 D1-D7 | `cdev.rs` | 改写要点 |

#### 22-sdev（新：22）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-380 | 长短问三态（短问钉线程/长问挂进程/选择即返） | 概念 | 存量 | 22 §1.1 | `sdev.c:8-16` | 等待形态的分流 |
| K-381 | `sdev_suspend` 登记三形状与非法形状断言 | 机制 | 存量 | 22 §2.3 | `sdev.c:83-110` | 挂起载荷 |
| K-382 | 三授权配给与方向交叉 | 机制 | 存量 | **22 §1.2/§2.7**；19/21（重复） | `sdev.c:355-402` | grant 配给 |
| K-383 | `sdev_socket` 建字（域门/拼号/坏次号回收） | 接口与协议 | 存量 | 22 §2.4 | `sdev.c:119-171` | 短问族 |
| K-384 | `sdev_bindconn`/`sdev_simple` | 接口与协议 | 存量 | 22 §2.5/§2.6 | `sdev.c:178-273` | 绑定/监听/关闭 |
| K-385 | `sdev_readwrite`/`sdev_ioctl`/选项族 | 接口与协议 | 存量 | 22 §2.7/§2.8 | `sdev.c:340-586` | 长问族 |
| K-386 | `sdev_stop`/`sdev_cancel`/`sdev_reply` 停尸与路由 | 机制 | 存量 | 22 §2.12/§2.13 | `sdev.c:912-1114` | 死亡与取消 |
| K-387 | accept 特办（回复生线程/双查防线） | 机制 | 存量 | 22 §1.4/§2.11/§2.13 | `sdev.c:679-758,1058-1108` | 续作的唯一例外 |
| K-388 | 死亡统一 EIO 口径 | 约束与不变量 | 存量 | 22 §1.6 | `sdev.c:921-927` | 上层只需处理一种死因 |
| K-389 | 驱动死亡级联总谱（汇 19/14/20/21/23 的钩子） | 约束与不变量 | 新增 | —（旧文无统一视图） | `dmap.c:275`、`filedes.c:250-308`、`sdev.c:912`、`select.c:884`、`vmnt.c:180` | 故障治理的统一入口 |
| K-390 | `SockChannel`/`ReplyRoute`/`grant_trio` 类型化（实现宿主 `minix-sockdriver`） | 架构演进 | 存量（纠错归属） | 22 D1-D7（旧文指向 vfs/src/sdev.rs，已迁移） | `os/libs/minix-sockdriver/src/sdev.rs` | 改写与归属的新事实 |

#### 23-select（新：23）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-395 | 群查询与两波回答 | 概念 | 存量 | 23 §1.1/§1.3 | `select.c:1-15,33` | select 的反向调用本质 |
| K-396 | 四分型表与优先级（字符/套接字/文件/管道） | 数据结构 | 存量 | 23 §1.2/§2.2 | `select.c:85-91` | 先问谁、未知类型拒 |
| K-397 | 选择表 25 槽与 filp 选择字段 | 数据结构 | 存量 | 23 §1.4/§2.2 | `select.c:31-53`、`file.h:26-32` | 账本规模 |
| K-398 | `select_filter` 三出口（剪枝/挂起/投递） | 机制 | 存量 | 23 §2.7 | `select.c:409-457` | 请求生成 |
| K-399 | 驱动请求与管道一字节试探 | 机制 | 存量 | **23 §2.8/§2.9**；17（重复侧） | `select.c:462-616` | 异步就绪来源 |
| K-400 | fd 位图双译与集合拷贝 | 数据结构 | 存量 | 23 §2.10 | `select.c:621-708` | 用户集与内核集互译 |
| K-401 | 取消/复活/两波回复的配对 | 机制 | 存量 | 23 §2.11-2.16 | `select.c:714-1269` | 无悬空等待 |
| K-402 | 超时三态与 CLOCK 定时器（`set_timer` 实现于 libsys） | 机制 | 存量 | 23 §1.5/§2.3 | `select.c:96-174,335`、`libsys/timers.c:97` | 超时语义与库边界 |
| K-403 | 死亡善后两路（整槽取消/标就绪） | 机制 | 存量 | 23 §1.6/§2.14 | `select.c:884-951` | 端点死亡 |
| K-404 | `FdKind`/`SelectDriver`/`SelectVerdict` 类型化 | 架构演进 | 存量 | 23 D1-D7 | `select.rs` | 改写要点 |

#### 24-socket（新：24）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-410 | socket 调用面与驱动面两层分工 | 概念 | 存量 | 24 §1.1 | `socket.c:1-7` | 层界 |
| K-411 | 创建三步检查与失败清理表 | 机制 | 存量 | 24 §1.2/§1.3/§2.5 | `socket.c:176-267` | 分配释放配对 |
| K-412 | `make_sock_fd` 九步安装 | 机制 | 存量 | 24 §2.4 | `socket.c:77-170` | fd/filp/vnode/PFS 一次配齐 |
| K-413 | `get_sock` fd/类型检查与解锁说明 | 接口与协议 | 存量 | 24 §1.4/§2.7 | `socket.c:276-302` | EBADF/ENOTSOCK |
| K-414 | 阻塞恢复四分支与"不再阻塞"约束 | 机制 | 存量 | 24 §1.5/§2.11/§2.13 | `socket.c:399-477,522-524` | 唤醒分类 |
| K-415 | msghdr 单项限制与标志换算 | 接口与协议 | 存量 | 24 §1.6/§2.2/§2.14 | `socket.c:44-57,566-574` | 消息头 |
| K-416 | accept 继承掩码与 backlog 钳零 | 约束与不变量 | 存量 | 24 §2.9/§2.11 | `socket.c:355-356,459` | 参数归一 |
| K-417 | `SockError`/补偿表/`BuildStep` 类型化 | 架构演进 | 存量 | 24 D1-D7 | `socket.rs` | 改写要点 |

#### 25-exec（新：25）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-425 | exec=地址空间替换（fd 表独留） | 概念 | 存量 | 25 §1.1 | `exec.c:1-11` | 与 fork 的对照 |
| K-426 | 打开文件三项检查（REG→X→stat） | 机制 | 存量 | 25 §1.2/§2.3 | `exec.c:112-147` | 检查优先级 |
| K-427 | 脚本解释三步与两处 ENOEXEC | 机制 | 存量 | 25 §1.3/§2.10 | `exec.c:522-602` | `#!` 装载 |
| K-428 | 动态链接切换（主文件 fd 保留） | 机制 | 存量 | 25 §1.4/§2.6 | `exec.c:283-314` | ELF interp |
| K-429 | 装载双路径与 `exec_loaders[]` 首胜 | 机制 | 存量 | 25 §1.5/§2.7 | `exec.c:320-356` | 表驱动 |
| K-430 | 辅向量七项与 `stack_prepare_elf` | 数据结构 | 存量 | 25 §2.9/D6 | `exec.c:404-517` | 动态链接器续命 |
| K-431 | `clo_exec` 扫描（错误忽略） | 机制 | 存量 | 25 §2.13 | `exec.c:721-731` | 关闭克隆 fd |
| K-432 | suid 只用一次 | 约束与不变量 | 存量 | 25 §2.6 | `exec.c:254,270,313` | 防双重提权 |
| K-433 | 收尾三步与清理顺序 | 机制 | 存量 | 25 §1.6/§2.8 | `exec.c:358-402` | 错序即泄漏 |
| K-434 | `ExecPhase`/`AuxKind`/`DynSwitch` 类型化 | 架构演进 | 存量 | 25 D1-D7 | `exec.rs` | 改写要点 |

#### 26-coredump（新：26）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-440 | core 文件四部分与写入序 | 数据结构 | 存量 | 26 §1.2/§2.1 | `coredump.c:32-74` | 布局目录 |
| K-441 | ELF 头与程序头填充（六字段） | 机制 | 存量 | 26 §2.2/§2.3 | `coredump.c:80-116` | 头字段 |
| K-442 | 注释双段结构与对齐衬垫 | 数据结构 | 存量 | 26 §1.3/§2.4 | `coredump.c:126-145` | 双 Nhdr |
| K-443 | 段数据三策略（有页复制/缺页补零/超限截断） | 机制 | 存量 | 26 §1.4/§2.9 | `coredump.c:294-327` | 完整性优先 |
| K-444 | VM 区表批取与百区上限 | 接口与协议 | 存量 | 26 §2.7 | `coredump.c:191-227` | 批查询模式 |
| K-445 | 准备三步与收尾归进程退出 | 机制 | 存量 | 26 §1.5/§2.10（收尾归 10） | `misc.c:903-943` | 生命周期归属 |
| K-446 | `DumpPhase`/`ElfTarget` 类型化 | 架构演进 | 存量 | 26 D1-D7 | `coredump.rs` | 改写要点 |

#### 27-link（新：27）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-455 | 目录项与 inode 分离、链接计数 | 概念 | 存量 | 27 §1.1 | `link.c:1-3` | 删除只减计数 |
| K-456 | 硬链接三检查与 EXDEV | 机制 | 存量 | 27 §1.2/§2.2 | `link.c:29-86` | 创建门槛 |
| K-457 | `do_unlink`/rmdir 分流与粘滞位保护 | 机制 | 存量 | 27 §1.3/§1.4/§2.3 | `link.c:91-164` | 删除保护 |
| K-458 | `do_rename` 四项检查与双目录锁断言 | 机制 | 存量 | 27 §1.5/§2.4 | `link.c:169-271` | 重命名 |
| K-459 | `truncate`/`ftruncate` 共用 `truncate_vnode` 与长度相同跳过 | 机制 | 存量 | 27 §1.6/§2.5-§2.7 | `link.c:276-381` | 截断 |
| K-460 | `do_slink` 合法域与 `readlink` 双入口 | 接口与协议 | 存量 | 27 §2.8/§2.9 | `link.c:387-508` | 符号链接 |
| K-461 | `LinkLookup`/`FsLink` 类型化 | 架构演进 | 存量 | 27 D2/D7 | `link.rs` | 改写要点 |

#### 28-stadir（新：28）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-470 | 工作目录=相对路径起点、根=`..` 终止点 | 概念 | 存量 | 28 §1.1 | `stadir.c:1-13` | 锚位语义 |
| K-471 | 切换三项检查与引用替换顺序（先加新再放旧） | 机制 | 存量 | 28 §1.2/§2.5 | `stadir.c:117-135` | 防泄漏 |
| K-472 | `chdir`/`fchdir` 双入口共用 `change_into` | 机制 | 存量 | 28 §1.2/§2.2/§2.3 | `stadir.c:32-78` | 切换 |
| K-473 | `chroot` 仅 root 且边界闭包 | 约束与不变量 | 存量 | 28 §1.3/§2.4 | `stadir.c:83-112` | 根边界 |
| K-474 | stat 双入口与 lstat 尾符号链接保留 | 机制 | 存量 | 28 §1.4/§2.6/§2.11 | `stadir.c:140-192,418-446` | 元数据查询 |
| K-475 | statvfs 实时/缓存分流与 ST_RDONLY 叠加、三名称直拷 | 机制 | 存量 | 28 §1.5/§2.7/§2.8 | `stadir.c:197-289` | 文件系统信息 |
| K-476 | `do_getvfsstat` 两阶段遍历与锁后复验 | 机制 | 存量 | 28 §1.6/§2.10 | `stadir.c:351-413` | 全表枚举 |
| K-477 | `StatvfsFresh`/`MountNames` 类型化 | 架构演进 | 存量 | 28 D5 | `stadir.rs` | 改写要点 |

#### 29-protect（新：29）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-485 | 九位权限三档、按 uid-gid 选档 | 机制 | 存量 | 29 §1.1/§2.6 | `protect.c:238-287` | 判定核心 |
| K-486 | 过期 id（-1）与 root 边界 | 约束与不变量 | 存量 | 29 §1.2 | `protect.c:251,258-266` | 特权边界 |
| K-487 | 主组+补充组并查（`in_group`） | 机制 | 存量 | 29 §1.3/§2.8 | `utility.c:128-141` | 组判定 |
| K-488 | `do_chmod` 属主检查→只读→清 setgid | 机制 | 存量 | 29 §1.4/§2.2 | `protect.c:25-92` | 属性修改 |
| K-489 | `do_chown` 禁止转交与-1 哨兵 | 机制 | 存量 | 29 §1.5/§2.3/D3 | `protect.c:98-177` | 属主变更 |
| K-490 | `do_umask` 取反存储 | 机制 | 存量 | 29 §1.6/§2.4 | `protect.c:182-192` | 掩码 |
| K-491 | `do_access` 模式检查与 F_OK 空集 | 接口与协议 | 存量 | 29 §2.5 | `protect.c:198-232` | access 语义 |
| K-492 | `read_only` 只读挂载检查与 forbidden 结尾 | 约束与不变量 | 存量 | 29 §2.6/§2.7 | `protect.c:282-302` | EROFS |
| K-493 | `forbidden_decision`/`umask_swap` 类型化 | 架构演进 | 存量 | 29 D3/D4/D6 | `protect.rs` | 改写要点 |

#### 30-fcntl-lock（新：30）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-500 | fcntl 十三命令分派与无分支命令 EINVAL | 接口与协议 | 存量 | 30 §1.1/§2.1/§2.6 | `misc.c:117-267` | 多路复用 |
| K-501 | F_DUPFD 目标下限与复制共享 | 机制 | 存量 | 30 §1.2/§2.2 | `misc.c:136-148` | fd 复制 |
| K-502 | GETFL/SETFL 不对称、CLOEXEC 与 NOSIGPIPE | 机制 | 存量 | 30 §1.3/§2.3/§2.6 | `misc.c:150-175,238-246` | 标志语义 |
| K-503 | 记录锁兼容矩阵与"解锁不限持有者" | 约束与不变量 | 存量 | 30 §1.4/§2.9 | `lock.c:69-132` | 劝告锁 |
| K-504 | 区域计算与 `MAX_FILE_POS`、len=0 到文件尾 | 机制 | 存量 | 30 §1.5/§2.8 | `lock.c:51-67` | 区域溢出 |
| K-505 | SETLK→EAGAIN 与 SETLKW→SUSPEND 及挂起存档 | 机制 | 存量 | 30 §1.6/§2.9 | `lock.c:83-99` | 阻塞策略 |
| K-506 | 解锁四分支与广播唤醒 | 机制 | 存量 | 30 §1.7/§2.9/§2.11 | `lock.c:101-132,172-192` | 区域维护 |
| K-507 | close 时清锁（同进程同 vnode） | 约束与不变量 | 存量 | **30 §2.12**；14（收束） | `open.c:713-724` | 锁终点 |
| K-508 | FREESP 打洞与 FLUSH_FS_CACHE 分流 | 接口与协议 | 存量 | 30 §2.5/§2.6 | `misc.c:184-264` | 打洞与清缓存 |
| K-509 | `LockRegion`/`LockTable`/`FcntlCmd` 类型化 | 架构演进 | 存量 | 30 D1-D7 | `fcntl.rs` | 改写要点 |

#### 31-sync-and-time（新：31，拆分自旧 31）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-515 | `do_sync` 全卷广播（不收集结果） | 机制 | 存量 | 31 §1.3/§2.2 | `misc.c:276-296` | 落盘广播 |
| K-516 | `do_fsync` 定点（fd→卷→同条件） | 机制 | 存量 | 31 §1.3/§2.2 | `misc.c:297-326` | 定点落盘 |
| K-517 | `do_utimens` 双入口与 NOW/OMIT/显值三态 | 接口与协议 | 存量 | 31 §1.7/§2.6 | `time.c:26-155` | 时间戳 |
| K-518 | 写回触发时机与"无返回值"契约 | 约束与不变量 | 存量 | 31 §2.2 | `misc.c:276-326`、`request.c:1134` | 为何 sync 静默 |
| K-519 | 同步与只读挂载的交互（只读卷跳过） | 约束与不变量 | 新增 | — | `misc.c:286-295`、`protect.c:292-302` | 边界 |

#### 32-vm-call（新：32，拆分自旧 31）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-525 | `VFS_VMCALL` 消息六域（req/fd/reqid/endpoint/offset/length） | 接口与协议 | 存量 | 31 §1.5/§2.4 | `misc.c:380-399`、`mess_10` | VM 请求的线格式 |
| K-526 | 三种上游请求语义（FDLOOKUP/FDCLOSE/FDIO） | 接口与协议 | 存量 | 31 §1.5/§2.4 | `misc.c:415-497`、`com.h:702-704` | 请求族 |
| K-527 | 来源门（非 VM→ENOSYS）与恒异步回复（`VM_VFS_REPLY`） | 约束与不变量 | 存量 | 31 §1.5 | `misc.c:401-403`、`com.h:707` | 安全与回复契约 |
| K-528 | `dupvm` 两项检查与 filp 计数共享 | 机制 | 存量 | 31 §1.4/§2.3 | `misc.c:328-378` | VM 借 fd 的合法性 |
| K-529 | VM 侧 fdref 协作（对端概念，交叉引用） | 概念 | 新增 | — | `02-stage-vm/23-vfs-interaction.md` | 两侧引用计数配对 |
| K-530 | `VmVfsReq` 解码与 C 绝对值断言纪律 | 架构演进 | 新增 | —（E-VFSWIRE 落地） | `os/servers/vfs/src/misc.rs:VmVfsReq`、`minix-types` | wire 定稿纪律 |

#### 33-system-info（新：33，拆分自旧 31）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-535 | `do_getsysinfo` 快照族与 root/长度双门 | 接口与协议 | 存量 | 31 §1.2/§2.1 | `misc.c:52-116` | 批量查询 |
| K-536 | `fproc_light`/`dmap` 快照的生产面与布局 | 数据结构 | 新增 | 31 §1.2（旧文只写消费） | `misc.c:73-96`、`minix-types/types/fproc.rs` | 快照字段 |
| K-537 | `do_svrctl` 的 verbose/统计开关族 | 接口与协议 | 存量 | 31 §1.6/§2.5 | `misc.c:797-902` | 运行期开关 |
| K-538 | `do_gcov_flush` 五门（含 super_user） | 接口与协议 | 存量 | 31 §2.7 | `gcov.c:10-73` | 覆盖率接口 |
| K-539 | `do_getrusage` 废弃恒 OK | 工具工程 | 存量 | 31 §2.8 | `misc.c:998-1006`、`table.c:60` | 保持外部行为 |
| K-540 | `panic_hook` 有意省略 | 工具工程 | 存量 | 31 §2.8；99 省略表 | `misc.c:989-997` | 不移植死钩子 |
| K-541 | 调用统计编译开关（`ENABLE_SYSCALL_STATS`） | 工具工程 | 存量（归并） | 09 §1.3/§2.2；31 D2 收束到本篇 | `main.c:32-34,286-289` | cfg 语义 |

#### 99-global-concepts（新：99）

| 编号 | 名称 | 类型 | 来源类型 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|----------|----------|------|----------|
| K-550 | 容量常量族与单一真源规则 | 数据结构 | 存量 | 99 §1.1 | `const.h:5-12`、`syslimits.h` | 资源上限一览 |
| K-551 | 阻塞原因枚举的跨模块契约（A-3） | 接口与协议 | 存量 | 99 §1.2；02 §2.3 | `fproc.h:29-61` | 阻塞协议 |
| K-552 | wire 前缀总表与判别宏 | 接口与协议 | 存量（扩充） | 99 §1.3 | `callnr.h:68`、`com.h:513,589,919,963,1037`、`vfsif.h:77`、`com.h:909-912` | 命名空间全貌 |
| K-553 | `glo.h` 全局→`VfsState` 映射 | 架构演进 | 存量 | 99 §2 | `glo.h`、`main_loop.rs:VfsState` | A-4 |
| K-554 | 三层引用计数不变量（filp/vnode/fs） | 约束与不变量 | 存量 | 99 §3；04/05/14 | `file.h:11`、`vnode.h:13-14` | 失效族的正确性基础 |
| K-555 | endpoint 与 transid 两条定位机制 | 概念 | 存量 | 99 §4 | `fs_comm.rs`、`minix-types` | 消息寻址 |
| K-556 | `who_p`/`who_e`/`call_nr` 上下文宏的 Rust 对应 | 接口与协议 | 存量 | 99 §4；09 §2.1 | `glo.h:26-33` | 当前请求读取 |
| K-557 | `sys_datacopy_wrapper` 跨空间拷贝 | 机制 | 存量 | 99 §4 | `utility.c:142-186` | 内核边界 |
| K-558 | A-8 类型映射表（Pid/Uid/Gid/DevId/Mode/Off） | 架构演进 | 存量 | 99 §4 | `minix-types` | 域类型契约 |
| K-559 | 有意省略台账（死代码/死钩子/调试设施） | 工具工程 | 存量 | 99 省略表 | 各条 C 锚点 | 防误报缺口 |
| K-560 | 错误码纪律与 `ToErrno` 通道 | 约束与不变量 | 新增（扩充） | 99 §（新） | `minix-types/types/errno.rs`、`*_error` 枚举 | 不自创 errno |
| K-561 | 构建与工具链事实（C Makefile / Rust Cargo） | 工具工程 | 新增 | — | `servers/vfs/Makefile`、`os/servers/vfs/Cargo.toml` | 工程制品索引 |
| K-562 | 测试基建现状与缺口（365 内联单测；无 VFS 端到端） | 测试性质 | 新增 | 00 §5（旧数 346 过时） | `os/servers/vfs/src`、`os/qemu-tests` | 验证边界诚实声明 |
| K-563 | 守卫与失败模式清单（fail-closed 契约） | 约束与不变量 | 新增 | —（散见 D 系列） | `syscalls.rs:dispatch_syscall` 各臂、`ipc/dispatcher.rs` | 通电前的行为契约 |

### 2.3 重复与主讲述点标记

下表列出跨篇重复出现的知识点与新目录中的唯一主讲述点（"其余引用"= B 相正文只允许一句话回指，不允许重讲）：

| 主题 | 旧文出现处 | 主讲述点（新） |
|------|-----------|----------------|
| filp 引用计数与 fork 共享 | 02 §1.4/§2.6、04 §1.2/§2.6、10 §1.3 | 04（结构）＋10（fork 协议） |
| vnode 双层引用与 put | 05 §1.2/§2.6、04 §2.6、10 §2.5 | 05 |
| tll 锁原语与升级 | 07 全篇、04 D3、05 §1.4、06 §1.3、13 §1.4、30 §D5 | 07 |
| 两遍 fproc 初始化 | 01 §2.3、02 §2.6、03 §2.6 | 03 |
| fproc_light | 02 §2.7、03 §2.7、10 D6、11 D6 | 03（数据）＋33（生产面） |
| SUSPEND/revive 总状态机 | 08 §1.1、09 §1.4、17 §1.3、23 §1.5 | 09（契约）＋各族篇（路径） |
| transid 编码与路由 | 09 §1.2/§2.10（写反）、11 §1.3/§2.11 | 11（编码）＋09（路由） |
| lookup_res/EENTERMOUNT | 11 §2.11、12 §1.3/§2.5、13 §1.6 | 12（结构）＋13（使用） |
| grant 方向交叉 | 19 §2.12、21 §2.6、22 §2.7 | 19 |
| select 与 pipe 试探 | 16 §2.7、17 §2.3、23 §2.9 | 17（定量机）＋23（调度） |
| close_filp 分族 | 14 §2.7、15 §2.8、30 §2.12 | 14 |
| 退出级联（fd/vnode/vmnt） | 10 §2.5、14 §2.5、05 §2.6、06 §2.7 | 10（顺序）＋14/05/06（各自实现） |
| 驱动死亡级联 | 06 §2.7、14 §2.5、19 §2.7、20 §2.5、21 §2.8、22 §2.12、23 §2.14 | 19（触发）＋22 §总谱（统一视图） |
| mount_pfs/根挂载 | 01 §2.3/§2.5、18 §2.7 | 18 |
| ELF 结构（Ehdr/Phdr） | 25 §2.2、26 §2.2/§2.3 | 25（装载视角）＋26（core 视角，不再互述） |
| 测试基线数字 | 各篇 §5（互相抄写且过期） | 00（策略）＋各篇只报本模块快照 |
| `fetch_vmnt_paths` 死代码 | 06 §1.5/§2.7、28 §D5 | 99 省略台账 |

### 2.4 统计摘要

- 知识点总数：**343 条**（含 13 条新增）。按类型分布：概念 25、机制 154、数据结构 41、接口与协议 38、约束与不变量 34、架构演进 38、工具工程 12、测试性质 1。
- 按新篇章分布（条数）：00 8、01 18、02 14、03 8、04 10、05 9、06 10、07 8、08 13、09 15、10 13、11 8、12 9、13 11、14 8、15 9、16 9、17 11、18 12、19 12、20 7、21 9、22 11、23 10、24 8、25 10、26 7、27 7、28 8、29 9、30 10、31 5、32 6、33 7、99 14。
- 存量/新增：存量 330、新增 13。新增的来源分类：非 C 制品与构建 2（K-026 构建链接、K-561 工具链）；跨阶段对端与仓库共享层 2（K-529 VM fdref、K-536 快照布局）；项目工程事实 4（K-067 快照生产面、K-228 REQ 契约收敛、K-530 VMCALL 解码纪律、K-562 测试基建）；契约纪律与边界 4（K-389 死亡级联总谱、K-519 只读同步交互、K-560 错误码纪律、K-563 fail-closed 守卫清单）；结构索引 1（K-008 旁路协议索引）。全部带证据锚。
- 覆盖率：池内 343 条全部在本报告 §5 契约中有归属；无删除项（不存在"无家可归"的知识点）。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

主题全集由四路合成，逐路列出证据：

1. **C 源码符号**：33 个 `.c` 的函数面（279 个函数定义，`grep` 计数）、结构体（`struct fproc/filp/vnode/vmnt/dmap/selectentry/worker_thread/...`）、宏与常量（`const.h`/`glo.h`/`vfsif.h`/`com.h`）、状态机（worker 状态、select 两波、pipe 挂起、锁三级、进程挂起七态）、错误路径（各 `do_*` 的 errno 分支与 panic 断言）、按架构分支的部分（`LOCK_DEBUG`、`ENABLE_SYSCALL_STATS`、`MKCOVERAGE`、`DO_POSIX_PATHNAME_RES`、`__MINIX` 的 `PIPE_BUF`/`OPEN_MAX`）。
2. **机制对应的操作系统通用概念**：进程镜像与凭证、文件描述符共享、vnode/inode 缓存、挂载树、文件锁（劝告锁）、多路复用两波模型、驱动注册与热插拔、异步请求-回复与超时取消、ELF 装载与 core dump、路径解析（符号链接、跨挂载）、命名空间与特权（root/setuid/setgid）、微内核服务重启语义。
3. **非 C 制品承载的主题**：见 §3.5 十项清单（Makefile、boot image 表、外部头文件线格式、libsys 定时器库、minix-sockdriver 库、minix-types wire crate、Cargo 构建、qemu 测试基建）。
4. **阶段边界契约**：`00-master-plan/README.md` 的启动因果链、`04-stage-pm/05-vfs-interaction.md`（PM 侧对端状态机）、`02-stage-vm/23-vfs-interaction.md`（VM 侧 fdref 与 VMCALL 发送半）、`15-stage-fs/00-fs-overview.md`（FS 侧 REQ 服务端）、`edge_todo.md` 的 E-REQWIRE/E-VFSWIRE/E-MIBPROD/E-DMWIRE/E-DEVWIRE 等跨阶段条目。

### 3.2 C 源文件 → 新文档映射（逐文件核对）

`wc -l` 实测；"新文档"为本报告 §4 的编号。全部 33 个 `.c` 与 15 个本地 `.h` 均有归属，无遗漏。

| C 文件 | 行数 | 关键函数数 | 新文档 | 核对说明 |
|--------|------|-----------|--------|----------|
| `main.c` | 973 | 22 | 01/08/09/10 | 按函数拆：启动/锁 → 01；主循环/回复/unblock → 09；service_pm 族 → 10；thread_cleanup → 08 |
| `table.c` | 82 | 1 | 09 | `call_vec` 64 项（`CALL(` 计数 64） |
| `utility.c` | 186 | 5 | 03/13/29/99 | `isokendpt_f`→03；`copy_path`/`fetch_name`→13；`in_group`→29；`sys_datacopy_wrapper`→99 |
| `misc.c` | 1006 | 22 | 10/30/31/32/33 | `pm_*`/`free_proc`→10；`do_fcntl`→30；`do_sync`/`do_fsync`→31；`do_vm_call`/`dupvm`→32；`do_getsysinfo`/`do_svrctl`/`do_getrusage`/`panic_hook`/`ds_event`→33/19 |
| `open.c` | 727 | 13 | 15（`close_fd` → 14） | `close_fd` 的定义在 `open.c:690`，语义归 14 |
| `read.c` | 393 | 9 | 16 | `rw_pipe` 归 17 语义，16 只给分派 verdict |
| `write.c` | 25 | 1 | 16 | 全文 |
| `pipe.c` | 561 | 12 | 17 | 整文件 |
| `filedes.c` | 656 | 15 | 04/14 | `init/get_filp/find/lock/close_filp`→04；`get_fd/check_fds/invalidate*/copyfd`→14 |
| `mount.c` | 653 | 14 | 18 | `mount_pfs` 机制归 18，01 只留调用点 |
| `vmnt.c` | 292 | 16 | 06 | `fetch_vmnt_paths`（246）死代码 → 99 省略台账 |
| `dmap.c` | 328 | 11 | 19 | 整文件 |
| `smap.c` | 273 | 8 | 19 | 整文件 |
| `device.c` | 95 | 2 | 19 | `do_ioctl`/`make_ioctl_grant` |
| `bdev.c` | 282 | 6 | 20 | 整文件 |
| `cdev.c` | 508 | 11 | 21 | 整文件 |
| `sdev.c` | 1114 | 25 | 22 | 整文件；Rust 实现宿主为 `minix-sockdriver` |
| `select.c` | 1416 | 55 | 23 | `select_dump`/`select_forget`/`wipe_select` 的部分行为入 99 省略台账 |
| `socket.c` | 762 | 21 | 24 | 整文件 |
| `path.c` | 933 | 9 | 13（`do_socketpath` 定义在 path.c，调用面归 24） | 整文件 |
| `vnode.c` | 316 | 12 | 05 | 整文件 |
| `tll.c` | 324 | 10 | 07 | 整文件 |
| `lock.c` | 192 | 2 | 30 | `lock_op`/`lock_revive` |
| `worker.c` | 607 | 26 | 08 | 整文件 |
| `comm.c` | 244 | 11 | 11 | `vm_vfs_procctl_handlemem` 归 11/32 交界，主家 11 |
| `link.c` | 508 | 9 | 27 | 整文件 |
| `exec.c` | 763 | 10 | 25 | 整文件 |
| `stadir.c` | 446 | 13 | 28 | 整文件 |
| `protect.c` | 302 | 6 | 29 | 整文件（第 7 个 `in_group` 在 utility.c） |
| `time.c` | 155 | 1 | 31 | `do_utimens` |
| `coredump.c` | 327 | 21 | 26 | 整文件 |
| `gcov.c` | 73 | 1 | 33 | `do_gcov_flush` |

头文件映射：`fproc.h`→02/03/99；`file.h`→04/99；`vnode.h`→05；`vmnt.h`→06；`tll.h`→07；`threads.h`→08；`const.h`/`glo.h`/`type.h`/`fs.h`/`proto.h`→99（`proto.h` 作为函数索引）；`path.h`→13；`lock.h`→30；`dmap.h`→19；`request.h`→11/12。

### 3.3 覆盖缺口表

| # | 主题 | 重要性与证据 | 现状 | 建议（新建/并入） |
|---|------|--------------|------|-------------------|
| G-01 | 构建与链接制品（SRCS、`-lsys -ltimers -lexec -lmthread`、`MKCOVERAGE`→`gcov.c`、Cargo 依赖） | 读者无法从任何文档回答"VFS 由哪些源文件、链接哪些库、gcov 何时编入"；证据 `servers/vfs/Makefile`、`os/servers/vfs/Cargo.toml` | 无一篇讲 | 并入 99 §工程制品（K-561）；01 §启动装配一句话引用 |
| G-02 | `VFS_VMCALL` 的线格式与三种上游请求 | VM 侧已按 `02-stage-vm/23` 发送（`VmVfsReq`），VFS 侧解码已落地；旧 31 §2.4 只给流程不给六域线格式表 | 31 部分覆盖 | **新建 32**（K-525…K-530）；`dupvm` 一并迁入 |
| G-03 | 调用统计编译开关语义 | `main.c:32-34,286-289`；旧 09 与 31 各提一句、无归属 | 散见 | 并入 33（K-541）；99 省略台账备注 |
| G-04 | 驱动死亡级联的统一视图 | 触发在 `dmap.c:275`/`smap.c:148`，后果散在 `filedes.c:250-308`、`sdev.c:912`、`select.c:884`、`vmnt.c:180`；旧文五篇各讲一部分 | 无统一视图 | 并入 19（触发与清单）＋22 §总谱（K-340/K-389） |
| G-05 | wire 命名空间总表（VFS/PM/FS/TRANS/CDEV/BDEV/SDEV/VM 八族） | 旧 99 §1.3 只有四族且锚点错误（`com.h:512`/`1038`）；绝对值为 wire 纪律的根基 | 部分且错 | 并入 99（K-552），以 C 绝对值重写 |
| G-06 | 错误码纪律（30 个错误枚举 → errno 的统一通道） | 各篇 §3/§4 分散；项目规则"错误类型必须映射 errno" | 无总表 | 并入 99（K-560） |
| G-07 | 测试基建现状（内联单测规模、无 VFS 端到端、接线矩阵 W1-W9 是关闭条件） | 00 §5 记 346 个（实测 365，`#[test]` 静态计数）；`os/qemu-tests/` 无 VFS 用例 | 数字过期、缺口未声明 | 并入 00 §验证基建与 99（K-562） |
| G-08 | `fproc_light`/`dmap` 快照的生产面布局 | 旧 03/10/11 声称 A-7 缺口；工作树已落 `FProcSnap`（52B，`minix-types/types/fproc.rs`）与 `to_fproc_snap` | 三方重复且过期 | 并入 03（K-067）＋33（K-536） |
| G-09 | sdev Rust 实现宿主已迁至 `minix-sockdriver` | 旧 22 §4.2/§7 全部指向 `os/servers/vfs/src/sdev.rs`（现仅 228 行）；真实实现在 `os/libs/minix-sockdriver/src/sdev.rs`（710 行） | 归属错误 | 22 契约强制修正（K-390） |
| G-10 | 定时器实现的库边界（`expire_timers`/`set_timer` 在 `libsys/timers.c`，不在 VFS） | `main.c:112` 调用；`libsys/timers.c:97,115` | 旧 23 未声明归属 | 并入 23（K-402） |
| G-11 | REQ_* 契约已收敛到 `minix-types::ipc::fs_driver` | 旧 12 按 `request.rs` 本地定义叙述；E-REQWIRE 已闭单（VFS 与 minix-fs 双侧改消费） | 叙事过期 | 并入 12（K-228）＋99 |
| G-12 | `do_getrusage` 废弃但保留注册的理由 | `table.c:60` 注释 obsolete；旧 31 §2.8 只有一句 | 薄 | 并入 33（K-539），保持外部行为 |
| G-13 | VFS 自身的关闭与退出（reboot 路径；VFS 无常规退出） | `pm_reboot`（`misc.c:504-575`）经 `worker_start(..., PM)` 专线程执行；`unmount_all(1)` 扫荡后 `sys_abort` 类动作在 PM 侧 | 10/18 各半 | 并入 10 §关闭与退出小节（K-194/K-196） |
| G-14 | `selectentry` 14/20 字段数与 `cancel_*` 函数名 | 旧 23 §2.2/§2.11 写"十四字段"与 `cancel_all/cancel_filp`；C 实为 20 字段、`select_cancel_all`/`select_cancel_filp` | 事实错误 | 23 契约修正（K-396/K-401） |
| G-15 | `NAME_MAX`/`OPEN_MAX`/`TRANSACTION_BASE` 等跨篇数值真值 | 旧 13 写 NAME_MAX 60（C 为 511）、旧 14/04 写 OPEN_MAX 256（C 为 255）、旧 09 D3 写 TRANSACTION_BASE 0x1000（C 为 0xB00） | 数值错误 | 99 汇总真值表 + 各篇修正 |

### 3.4 重复主题表（保留主讲述点，其余引用）

见 §2.3。原则：同一机制在新目录中只有一个"首次完整讲述点"；其余篇章允许出现的最多形式是"名称 + 一句结论 + 指向主讲述点的引用"。特别列出的高重复主题：filp/vnode 引用计数（旧文 5 处）、tll 锁（旧文 6 处）、SUSPEND/revive（旧文 4 处）、transid（旧文 2 处且方向矛盾）、grant 方向（旧文 3 处）、驱动死亡（旧文 7 处）、测试基线（旧文每篇互抄）。

### 3.5 越界主题表（某篇讲了声明边界之外的主题）

| # | 旧位置 | 越界内容 | 正确归属（新） |
|---|--------|----------|----------------|
| O-01 | 02 §1.4/§2.6 | filp 计数递增与 `dup_vnode` 细节 | 04/05（02 只留字段） |
| O-02 | 02 §2.6、03 §2.6 | VFS_PM_INIT 握手流程 | 01（表级只留两遍初始化） |
| O-03 | 03 §2.5 | `fproc_addr`/`who_p` 宏用于主循环解引用 | 09/99（03 只留寻址定义） |
| O-04 | 04 §2.3/§2.4 | `get_fd` 双表扫描（fd 层分配） | 14 |
| O-05 | 05 §1.4、06 §1.3 | tll 原语教学与升级/等待语义 | 07（05/06 只留映射表） |
| O-06 | 06 §1.5/§2.7 | `fetch_vmnt_paths` 路径重建展开（C 死代码） | 99 省略台账（只留一行判定） |
| O-07 | 07 §1.3 | EBUSY 排队与 `worker_wait` 的关系 | 08 |
| O-08 | 08 §1.1 | pipe/select/cdev/sdev 四条阻塞路径细节 | 17/23/21/22 |
| O-09 | 09 §2.10 | TRNS 编解码实现（且方向写反） | 11 |
| O-10 | 10 §1.4/§2.5 | `close_fd` 逐行与 `put_vnode` 细节 | 14/05（10 只留顺序） |
| O-11 | 11 §2.11、D6 | `node_details/lookup_res` 与 `fproc_light` | 12/03 |
| O-12 | 12 §1.3/§2.5 | `lookup_res` 的消费与 `EENTERMOUNT` 处理 | 13 |
| O-13 | 13 §1.4 | tll 三级锁与 vnode/vmnt 锁族教学 | 07/05/06 |
| O-14 | 14 §2.6/§2.7 | `lock_filp` FIFO 细节、`close_filp` 的 S_ISCHR/BLK/SOCK 分支 | 04（锁）＋20/21/22（分支） |
| O-15 | 15 §2.3/§2.4/§2.8 | BLK/FIFO 分支、`close_filp` 分流 | 19/20（BLK）、17（FIFO）、14（close） |
| O-16 | 16 §2.7 | `rw_pipe` 定量与续写 | 17 |
| O-17 | 17 §2.2/§2.5 | `map_vnode` 与 15 重复、select 位清与 23 重复 | 17（主家）＋23（select 细节） |
| O-18 | 18 §2.7 | `mount_pfs` 与 01 重复展开 | 18（主家），01 只留调用点 |
| O-19 | 19 §2.11/§2.12 | ioctl 在 bdev/cdev/sdev 的再展开 | 19 主家；20/21/22 引用 |
| O-20 | 20 §2.5 | filp/vmnt 扫描的表操作细节 | 04/06（20 只留四元谓词） |
| O-21 | 21 §2.3/§2.4 | `req_newnode` 与 vnode 引用细节、CTTY 字段 | 12/05、02 |
| O-22 | 22 §2.9/§2.13 | `sdev_select` 与 accept 续作细节 | 23、24 |
| O-23 | 23 §2.8/§2.10 | cdev/sdev 请求投递执行与 `copy_fdsets` 执行 | 21/22、内核 18-syscall-copy |
| O-24 | 24 §2.4/§2.10 | PFS `req_newnode` 执行与 fd 槽预检实现 | 12、14 |
| O-25 | 25 §2.3/§2.4 | `forbidden`/凭证规则、`vfs_memmap` VM 侧执行 | 29/02、02-stage-vm/20 |
| O-26 | 26 §2.10 | `pm_dumpcore` 与 `free_proc` 全流程 | 10 |
| O-27 | 28 §2.10 | 挂载表加锁与锁后复验细节 | 06（28 只留遍历规则） |
| O-28 | 30 §2.12 | `close_fd` 全函数展开 | 14 |
| O-29 | 31 全篇 | 一篇承载 8 个操作族（完整性/VM 协议/系统控制/调试） | 拆为 31/32/33（本报告核心结构操作） |
| O-30 | 99 §3 | 引用计数不变量逐字段重述（与 04/05 重复） | 保留（跨模块不变量是 99 的正当内容），但后续 04/05 只允许"不变量见 99"式回指 |

### 3.6 非 C 主题逐项回答（固定清单）

| 主题 | 在哪里讲 | 依据/说明 |
|------|----------|-----------|
| 链接与加载 | 不在本 stage 展开，01 §启动装配一句话 + 交叉引用 | VFS 的 ELF 由 RS 从 boot image 加载（`rs/table.c:16`、`kernel/table.c:57`）；加载器在 01-stage-kernel/06-proc-init-boot-proc.md 与 03-stage-rs。VFS 自身不加载别的服务 |
| 镜像与内存布局 | 08（线程栈预算，ARCH 消除）＋02（fproc 表堆化）＋99（表容量） | `worker.c:15-20` 的 `TH_STACKSIZE`（28/40/64 KiB）；`fproc.rs:FProcTable` 堆存储规避 1.09 MiB 栈帧；VFS 无独立镜像布局 |
| 汇编入口与陷阱进入 | 不在本 stage（明确声明范围外） | VFS 是 C 程序，经 SEF/服务运行时进入 `main()`（`main.c:54`）；trap 与内核 IPC 入口归 01-stage-kernel/12-ipc-core.md 与用户态运行时（14-stage-runtime） |
| 启动装配 | 01（SEF 回调注册 + boot image 登记 + RS 启动 + `VFS_PM_INIT` 握手） | `main.c:374-388`、`main.c:415-436`、`rs/table.c:16` |
| 构建与工具链 | 99 §工程制品；01 §启动装配引用 | `servers/vfs/Makefile`（SRCS 33+gcov、`LDADD=-lsys -ltimers -lexec -lmthread`）、`os/servers/vfs/Cargo.toml`（`minix-types`/`minix-sockdriver`/`minix-sys`/`bitflags`；feature `fproc_light`） |
| 跨模块接口与线格式 | 10/11/12/19/20/21/22/32，汇总在 99 §wire | VFS_PM（`com.h:513-583`）、FS REQ（`vfsif.h:41-81`、`com.h:589`）、TRANS（`com.h:909-912`）、CDEV/BDEV/SDEV（`com.h:921-990,1034+`）、VMCALL（`com.h:702-707`）、DS notify 与 CLOCK（`com.h` 通知段） |
| 错误路径 | 各篇 §3/§4 的 errno 映射 + 99 §错误码纪律 | 30 个 `*Error` 枚举已接 `ToErrno`（`minix-types/types/errno.rs`）；ARCH fail-closed 守卫（`syscalls.rs:dispatch_syscall`、`ipc/dispatcher.rs`） |
| 关闭与退出 | 10（进程退出的资源顺序、`free_proc` 两阶段、reboot）；18（`unmount_all`）；14/19-22（驱动死亡级联）；22 §总谱统一；VFS 自身无常规退出（服务器由 RS 重启，见 03-stage-rs） | `misc.c:639-708,504-575`；`mount.c:552-585`；`filedes.c:250-308`；`sdev.c:912` |
| 并发与同步 | 07（tll 三级锁）、08（请求槽与睡眠）、02/04（锁归属）、99（引用计数不变量）；ARCH A-6 在 02/04/08 声明 | `tll.c`、`worker.c`、`fproc.h:71`、`file.h:14-18` |
| 测试基建 | 00 §验证基建 + 各篇 §5；99 声明缺口 | 实测 `os/servers/vfs/src` 共 365 个 `#[test]`；`os/qemu-tests/` 无 VFS 端到端用例；跨模块消息回路的关闭条件是 `plan.md` §8 的 W1-W9 矩阵（参考材料） |

---

## 4. 新目录

### 4.0 结构变更总览与理由

**结论：保留 00-31 + 99 的既有主线编号，只做一处拆分（旧 31 → 新 31/32/33），其余 32 篇编号不变、内容按本蓝图重建。**

理由（先证据后判断）：

1. **既有主线不是待推翻的对象，而是已被本阶段验证过的骨架**。旧编号方案（`plan.md` §1.2/§2）以启动顺序为主线，我在 §1 独立从 C 源码重建真序后逐篇核对"前置"字段，未发现前向引用：结构表（04-06）在锁（07）与执行（08/09）之前、FS 通信（11/12）在路径（13）与文件操作（14-17）之前、设备（19-22）在 select（23）与 socket（24）之前，顺序自洽。
2. **缺陷在内容与边界，不在顺序**。探查发现的高频问题是：重复展开（§2.3 的 16 组）、越界（§3.5 的 30 条）、数值与锚点过期（`NR_MNTS=8`、`OPEN_MAX=256`、`NAME_MAX=60`、`TRANSACTION_BASE=0x1000`、TRNS 方向写反、Rust 行锚大面积漂移）。这些问题只能用"按契约重写"解决，换编号解决不了。
3. **唯一必须拆的是旧 31**：它自己声明承载"八类操作"，横跨四个触发面（用户调用的 sync/fsync/utimens；VM 发来的上游协议；管理面 getsysinfo/svrctl/gcov/getrusage；调试钩子），违反"单篇单语义"。拆为 31（落盘与时间）、32（VFS↔VM 调用协议）、33（系统信息与控制面）后，每篇只有一个语义单元，且 32 对外是一条真正的服务间协议（与 10/11 同族）。
4. **不合并 02 与 03**（已评估，否决）：两篇虽同讲 fproc，但 02 是字段契约（被 10/16/17/23/29 依赖），03 是表与验证契约（被全部 handler 依赖），消费者几乎不相交；合并后单篇 700 行且会让 03 的"端点验证"淹没在字段清单里。重复的两遍初始化改归 03、`fproc_light` 改归 03 即可消重，无需动编号。
5. **不为"驱动死亡级联"新开一篇**（已评估，否决）：级联的触发点在 19（`dmap_unmap_by_endpt`/`smap_unmap_by_endpt`），但它的四条后果路径分别依赖 20/21/22/23，任何独立成篇的位置都会造成前向引用（放 19 之后则 20-23 未讲，放 23 之后则 21/22 读者的"死亡语义"无处落地）。替代方案：19 讲触发与总清单，各设备族篇讲本族停尸，22 在全部设备族讲完后用一节《死亡级联总谱》收束；99 保留不变量台账。
6. **不整体重编号**：stage 内 765 处 `NN-*.md` 互引 + 代码注释与其它 stage 的引用（实测 `os/` 内 4 处、`05-stage-vfs/doc basename` 若干），而拆分只影响指向旧 31 的少量引用；收益/成本比在此处最优。范围外发现（新篇 32/33）不改变任何已有编号。

### 4.1 新篇章总表

| 组 | 编号 | 标题 | 一句话定位 |
|----|------|------|-----------|
| 0 总览 | 00 | VFS 整体架构与阅读导航 | 回答"VFS 是什么、怎么读这套文档、每个问题去哪一篇找" |
| 1 诞生与进程上下文 | 01 | 启动链与初始化骨架 | 从 RS 加载到 `main()` 进入循环，VFS 如何建立文件系统世界 |
| | 02 | fproc：进程文件系统上下文 | 一个进程在 VFS 眼里的全部字段：状态、目录、fd 表、阻塞载荷、凭证 |
| | 03 | fproc 表与端点验证 | 256 个槽如何管理：空闲判据、两遍初始化、`isokendpt` 信任边界 |
| 2 核心表与并发基础 | 04 | filp 表：打开文件的共享与计数 | fd 与 vnode 之间的中间层：共享偏移、引用计数、三态锁、选择位 |
| | 05 | vnode 表：inode 缓存与引用 | VFS 侧的 inode 投影：双层引用计数、分配/回收、设备关联 |
| | 06 | vmnt 表：挂载边界 | 设备号到 FS 端点的映射点：容量、锁、端点回收、挂载元数据 |
| | 07 | tll 三级锁 | VFS 并发的原语：读/串行读/写三态与等待队列写偏序 |
| | 08 | 请求槽与工作调度 | 从 mthread 九线程到九请求槽：登记、门控、挂起恢复、停机 |
| 3 消息面 | 09 | 主循环与调用分发 | 运行时心脏：八路优先、`call_vec` 分发、同步/异步回复 |
| | 10 | PM 协议与进程生命周期 | VFS_PM 十二请求：fork 次主线、凭证、退出回收、重启 |
| | 11 | FS 通信窗口与事务标识 | 与所有文件系统的异步对话：每挂载窗口、排队、transid |
| | 12 | FS 请求包装与响应 | 32 个 REQ 的信封构造：grant、重试、响应回填、能力协商 |
| | 13 | 路径解析 | 名字到 vnode 的翻译机：跨挂载穿越、符号链接、锁协议 |
| 4 文件与描述符 | 14 | 文件描述符表与失效 | fd 表的分配与拆除、失效族、copyfd、close_fd |
| | 15 | open/close/lseek | 路径落到 fd 的六段管线：意图编码、创建、分派、定位、拆除 |
| | 16 | read/write/getdents | 数据面五路分派：位置推进、bsf 串行、peek、目录读 |
| | 17 | pipe 与挂起复活 | 定量、挂起、唤醒、中断：VFS 阻塞语义的家 |
| 5 挂载与设备 | 18 | 挂载与卸载 | 单树跨 FS 的嫁接与拆除：根挂载、伪设备、扫荡卸载 |
| | 19 | 设备表与驱动生死 | dmap/smap 通讯录、注册/恢复/驱散、ioctl 分派、DS 事件 |
| | 20 | 块设备通信 | 开/关/控直达与重试熔断、死信分类、换人通知 |
| | 21 | 字符设备通信 | 终端改道、克隆、挂起读写、取消与回复换码 |
| | 22 | 套接字驱动通信与死亡级联总谱 | 长短问三态、挂起登记、接受特办；并在末尾收束全设备族的死亡级联 |
| 6 多路复用与网络 | 23 | select 多路复用 | 一问等一群：四分型、两波回答、超时、死亡善后 |
| | 24 | socket 系统调用 | 调用面：创建三步、失败清理、阻塞恢复、消息头 |
| 7 执行与元数据 | 25 | exec 与地址空间替换 | 打开、脚本/动态链接、装载、收尾 |
| | 26 | core dump | 已停止进程的 ELF 转储：四部分布局、注释、段策略 |
| | 27 | 链接、删除、重命名与截断 | 目录项与 inode 的分离语义、粘滞位、`readlink` |
| | 28 | 目录切换、stat 与 statvfs | cwd/root 锚位、切换检查、元数据与文件系统信息查询 |
| | 29 | 权限判定与 chmod/chown/umask | 九位权限、root 边界、`forbidden`、`access`、只读挂载 |
| | 30 | fcntl 与记录锁 | 十三命令、fd 复制与标志、八槽劝告锁与广播唤醒 |
| 8 旁路调用 | 31 | 数据落盘与时间戳 | `sync`/`fsync`/`utimens`：写回触发与时间语义 |
| | 32 | VFS↔VM 调用协议 | `VFS_VMCALL` 六域、FDLOOKUP/FDCLOSE/FDIO、`dupvm` |
| | 33 | 系统信息与控制面 | `getsysinfo` 快照、`svrctl` 开关、`gcov`、废弃调用与调试钩子 |
| 9 全局 | 99 | 全局常量、线格式与不变量 | 容量、wire 命名空间、全局状态映射、引用计数不变量、省略台账、错误码纪律 |

新的 H1 标题统一格式：`# <编号> — <标题>` 或沿用 `# <编号>-<语义名>: <标题>`；每篇头部必须有统一声明块：`状态 / 定位 / 前置 / 边界（讲什么、不讲什么） / 源码 / Rust 模块`。

### 4.2 阅读路径

- **主线（第一次读，按编号顺序）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 16 → 17 → 18 → 19 → 20/21/22（选一族精读）→ 23 → 24 → 25/26 → 27/28/29 → 30 → 31/32/33 → 99。
- **启动主线（只关心"服务器怎么起来"）**：00 §启动主线 → 01 → 03（两遍初始化）→ 08（门控）→ 09（循环）→ 18（根挂载）。
- **一次 read 的路径（只关心"文件怎么读"）**：09（分发）→ 14（fd→filp）→ 15（open 的产物）→ 16（五路分派与位置推进）→ 13（打开时的路径解析）→ 11/12（FS 往返）。
- **一次 fork 的路径（fork 次主线）**：10 §fork → 03（槽检查）→ 02（整体复制）→ 04（fd 计数）→ 05（目录 dup）→ 14（后续 close 的逆操作）。
- **故障治理路径（驱动/FS 死了怎么办）**：19 §unmap（触发）→ 14 §invalidate（描述符失效）→ 20-22 各族停尸 → 22 §总谱（统一）→ 23 §死亡善后（select）→ 99 §不变量。
- **支线与可跳读**：26（core dump）、30（fcntl/记录锁）、31（sync/time）、32（VM 调用）、33（系统信息与调试）、99（工具台账）在首读时可跳过；18-22 的设备族如果不关心驱动，可只读 19 + 22 总谱。
- **并行主题代表成员**：64 个系统调用按 §4.3 分组，每组一个"讲透"的代表：文件 I/O 组代表 `read`（16）、名字组代表 `open`（15）、元数据组代表 `stat`（28）、挂载组代表 `mount`（18）、控制组代表 `fcntl`（30）、socket 组代表 `socket`/`accept`（24）。

### 4.3 并行主题的分组与代表成员

| 组 | 成员（64 调用 + 12 PM 请求 + REQ 面） | 框架篇 | 代表成员精讲 | 差异表位置 |
|----|--------------------------------------|--------|--------------|-----------|
| 文件 I/O | READ/WRITE/LSEEK/GETDENTS | 16 | `read` | 16 §五路分派 |
| 打开与拆除 | OPEN/CREAT/CLOSE/PIPE2 | 15 | `open` | 15 §六分支 |
| 名字与链接 | LINK/UNLINK/RENAME/RMDIR/SYMLINK/READLINK/MKDIR/MKNOD | 13+27 | `link` 与 `unlink` | 27 §分流表 |
| 元数据与权限 | STAT/FSTAT/LSTAT/CHMOD/FCHMOD/CHOWN/FCHOWN/UMASK/ACCESS/TRUNCATE/FTRUNCATE/UTIMENS | 28/29/31 | `stat` 与 `chmod` | 29 §九位判定 |
| 挂载 | MOUNT/UMOUNT/STATVFS1/FSTATVFS1/GETVFSSTAT | 18 | `mount` | 18 §提交序 |
| 控制与查询 | IOCTL/FCNTL/SELECT/SYNC/FSYNC/VMCALL/COPYFD/MAPDRIVER/GETSYSINFO/SVRCTL/GCOV_FLUSH/GETRUSAGE/SOCKETPATH | 19/23/30/31/32/33 | `fcntl` | 各篇差异表 |
| 网络 | SOCKET/SOCKETPAIR/BIND/CONNECT/LISTEN/ACCEPT/SENDTO/SENDMSG/RECVFROM/RECVMSG/SETSOCKOPT/GETSOCKOPT/GETSOCKNAME/GETPEERNAME/SHUTDOWN | 24 | `socket` 与 `accept` | 24 §恢复分类 |
| PM 请求 | INIT/SETUID/SETGID/SETSID/EXIT/DUMPCORE/EXEC/FORK/SRV_FORK/UNPAUSE/REBOOT/SETGROUPS | 10 | `pm_fork` | 10 §三级调度表 |
| FS 请求 | 32 个活 REQ | 11+12 | `req_readwrite` 与 `req_lookup` | 12 §包装族 |

---

## 5. 每篇契约

契约是 B 相的任务书。所有契约共同遵守两条写作纪律：

- **锚点纪律**：C 锚点用 `文件:行号/符号`（ground truth 冻结）；Rust 锚点只允许 `文件:符号名`，行号、测试数与实现状态必须标注"快照日期"（写入当日 `cargo test -p minix-vfs --lib` 实测），不允许把易变的行号写进正文断言。
- **引用纪律**：不引用隐藏中间产物目录；不引用其它执行者的重建产物；跨阶段引用只用正式 stage 文档路径。

### 00-vfs-overview

- 一句话定位：回答"VFS 是什么、它站在谁和谁之间、这套文档怎么读、每个问题去哪一篇找"。
- 讲什么：K-001（三方角色）、K-002（一次请求的生命周期总览：收—分—处—回复出的四站）、K-003（三个协议面规模与绝对值）、K-004（ARCH 改写清单导航 A-1…A-16）、K-005（阅读路径）、K-006（四条文档设计原则）、K-007（阶段在启动链中的位置）、K-008（旁路协议一页索引）。
- 不讲什么：一切机制细节（交给 01-33）；跨阶段启动链细节（01-stage-kernel/06、03-stage-rs）；Rust 实现的逐模块说明（各篇 §实现详解）。
- 前置：无。
- 后置：全部篇章（每篇都从 00 取得编号导航）。
- 事实底线：
  - C：`servers/vfs/` 的 33 个 `.c` 与 15 个本地 `.h`（`wc -l` 实测：`.c` 16,735 行、`.h` 1,007 行）；`callnr.h:68-137`（64 调用）；`com.h:513-544`（12 请求）；`vfsif.h:41-75`（33 常量、32 活）。
  - 非 C：`rs/table.c:16` 与 `kernel/table.c:57`（VFS 的 boot image 位置）；`servers/vfs/Makefile`；`os/servers/vfs/Cargo.toml`；Rust 测试数快照（写入当日实测，禁止沿用旧数字）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-001 | VFS 三方角色与边界 | 概念 | `servers/vfs/` 全部 | 全景篇的立篇之本 | 00 §1.1 |
  | K-002 | 一次请求的生命周期总览 | 机制 | `main.c:580-663`、`table.c:17-82` | 后文所有机制都挂在它上面 | 00 §1.2 |
  | K-003 | 三个协议面规模与绝对值 | 接口与协议 | `callnr.h:68-137`、`com.h:513-544`、`vfsif.h:41-75` | 数字是阅读路标 | 00 §1.3 |
  | K-004 | ARCH 清单导航 | 架构演进 | `os/servers/vfs/src/` | 改写决策总索引 | 00 §3；plan §4 |
  | K-005 | 阅读路径 | 工具工程 | — | 不同读者入口 | 00 §6（改写） |
  | K-006 | 文档设计原则 | 约束与不变量 | — | 全阶段共守 | 00 §1.4 |
  | K-007 | 阶段在启动链中的位置 | 概念 | `rs/table.c:16` | 阶段边界 | 00 §1.1（补锚点） |
  | K-008 | 旁路协议索引（32/33） | 接口与协议 | `misc.c:52,380` | 避免旁路内容失踪 | 新增 |

- 验收标准：能回答三个问题——"VFS 管什么、不管什么"；"一次 read 从消息到回复经过哪几站"；"驱动死亡级联去哪些篇找"。文中出现的每个数字都能在 C 头文件或实测命令中复核；Rust 相关数字带快照日期；全文无隐藏目录引用。

### 01-vfs-init-main

- 一句话定位：从 RS 加载到进入主循环，VFS 如何按固定顺序建立自己的世界，以及为什么根挂载必须门控。
- 讲什么：K-010（RS 启动）、K-011（启动依赖链与顺序不变量）、K-012（VFS_PM_INIT 握手）、K-013（boot 身份）、K-014（SEF 五回调）、K-015（十一项初始化清单）、K-016（system_hz）、K-017（DS 订阅）、K-018（bsf_lock）、K-019（init_dmap/smap）、K-020（rproctab 映射）、K-021（两遍 fproc 调用点）、K-022（四表建立）、K-023（根挂载门控）、K-024（LU 三回调）、K-025（lock_proc/unlock_proc）、K-026（构建与链接制品）、K-027（头文件包含关系）。
- 不讲什么：fproc 结构字段（02）、表级空闲判据（03）、worker 内部状态机（08）、主循环分发（09）、PM 协议其余 11 个请求（10）、挂载机制与回滚（18）、dmap/smap 表细节（19）、LU 的 RS 侧状态机（03-stage-rs）。
- 前置：00。
- 后置：02、03、04、05、06、07、08、09、10、18、99。
- 事实底线：
  - C：`main.c:54-141`（main 与循环框架）、`main.c:303-499`（SEF 回调与初始化链）、`main.c:501-553`（根挂载与进程锁）；`worker.c:27`、`worker.c:162-185`；`mount.c:391-425`（调用点）；`com.h:513-551`；`rs/table.c:16`；`kernel/table.c:57`。
  - 非 C：`servers/vfs/Makefile`（SRCS 与 `LDADD`）；`os/servers/vfs/Cargo.toml`。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-010…K-025 | 见 §2.2 | 混合 | 见 §2.2 | 全部发生在 `sef_cb_init_fresh`（`main.c:393-499`） | 01 §1-§2 + 跨篇去重后 |
  | K-026 | 构建与链接制品 | 工具工程 | `Makefile`、`Cargo.toml` | 启动装配的制品面 | 新增 |
  | K-027 | 头文件包含关系 | 数据结构 | `fs.h:1-43` | 启动篇交代源码地图 | 99/plan 散见 |

- 验收标准：按 §1.2 的 S01-S21 逐步复述启动链，每步带锚点；能指出握手失败、DS 订阅失败、根挂载失败各自的行为（panic 或返回）；能解释 `worker_allow(FALSE)` 期间到达的用户请求去了哪里（`worker_pending`，`worker.c:162-185`）；LU 回调与 worker 停机/重启的对应关系（`main.c:303-373`）说清；不再出现旧文"九段/段 11"的标题正文不一致。

### 02-fproc-struct

- 一句话定位：一个进程在 VFS 眼里的全部字段——状态、目录、fd 表、阻塞载荷、凭证——以及单线程改写下这些字段的类型化去向。
- 讲什么：K-040（四表定位）、K-041（字段分组）、K-042（六标志）、K-043（阻塞七态）、K-044（五类载荷与恢复方）、K-045（suspend 唯一入口/不可叠加）、K-046（凭证五字段）、K-047（fp_lock 属于槽）、K-048（请求执行四元组）、K-049（fp_name）、K-050（BlockedOn 类型化 A-3）、K-051（字段类型化 A-8）、K-052（FProcTable 堆存储）、K-053（哨兵 vs Option）。
- 不讲什么：表级容量、哨兵、`isokendpt` 与两遍初始化（03）；fork 的整表复制协议（10）；filp/vnode 引用计数（04/05，只留字段类型）；worker 调度与激活（08）；每条阻塞恢复路径的完整机制（17/23/21/22，只留"恢复方"一行指针）。
- 前置：01。
- 后置：03、08、10、14、17、23、29。
- 事实底线：`fproc.h:15-82`（struct 与 union）、`fproc.h:84-98`（快捷宏与标志）、`fproc.h:101-103`（哨兵）、`const.h:19-25`（阻塞常量）、`main.c:528-553`（锁）；Rust：`os/servers/vfs/src/fproc.rs`（`FProc`/`FpFlags`/`BlockedOn`，符号锚）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-040…K-053 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 `struct fproc` 为对象 | 02 §1-§4 |

- 验收标准：字段映射总表逐字段列出 C 字段 → Rust 字段（含哨兵处理方式）；六标志与七种阻塞状态的取值表可以逐项对照 C 常量；每个 `fp_u` 载荷标注恢复路径（`pipe.c:435`、`lock.c:173`、`select.c:783`、`cdev.c:481`、`sdev.c:759`）；不再出现旧文的伪锚点（如把 `LOCK_DEBUG` 行号当成字段行号）。

### 03-fproc-table

- 一句话定位：256 个 fproc 槽如何被管理：容量、空闲判据、两遍初始化、端点验证的信任边界、轻量快照。
- 讲什么：K-060（NR_PROCS 同界）、K-061（双哨兵）、K-062（isokendpt 三守卫）、K-063（致命分化）、K-064（寻址宏）、K-065（两遍整表初始化）、K-066（fproc_light 三字段）、K-067（快照生产面）。
- 不讲什么：字段定义（02）；fork 复制（10）；`who_p`/`who_e` 在主循环解引用（09）；MIB 消费端（10-stage-mib）；fproc_light 的 sys_datacopy 细节（33）。
- 前置：01、02。
- 后置：09、10、14、33 及一切使用端点的 handler。
- 事实底线：`utility.c:94-127`（`isokendpt_f`/`okendpt`）、`fproc.h:10-12,82`（表声明）、`fproc.h:101-115`（哨兵与 light 结构）、`glo.h:26-27`（寻址宏）、`main.c:405-408,468-484`（两遍初始化）、`misc.c:52-116`（light 填充）；Rust：`fproc.rs` 的 `FProcTable`/`is_ok_endpoint`/`ok_endpoint`/`at` 符号。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-060…K-067 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 `fproc[NR_PROCS]` 表为对象 | 02 §2.6/§2.7、03 全篇、10/11 的 light 段（去重） |

- 验收标准：`isokendpt` 三守卫与两种返回（OK/EDEADEPT，`utility.c:119-120`）有正反用例；两遍初始化解释"为什么空闲判定必须等到第二遍"；`fproc_light` 三字段与 `FProcSnap`（52 字节，`minix-types/types/fproc.rs`）的字段映射表列全；旧文引用但代码不存在的符号（`slot_of`）全部清除。

### 04-filp-table

- 一句话定位：fd 与 vnode 之间的中间层——打开文件对象的共享、计数、三态锁与选择位。
- 讲什么：K-080（中介三不变量）、K-081（struct filp）、K-082（容量与 init）、K-083（引用计数共享）、K-084（`get_filp` 特权）、K-085（查找族）、K-086（close_filp）、K-087（三态锁）、K-088（FSF 位集与选择字段）、K-089（类型化）。
- 不讲什么：fd 槽分配（`get_fd`，14）；`close_filp` 的 S_ISCHR/BLK/SOCK 分支（20/21/22，本篇只给分流表）；select 状态机的使用（23，本篇只给字段定义）；tll 原语（07）。
- 前置：01（init_filps 调用点）、03。
- 后置：05、10、14、15、17、23、30。
- 事实底线：`file.h:8-33`、`const.h:5`、`filedes.c:73-86,162-246,313-380,414-519`；Rust：`filp.rs`（`Filp`/`FilpTable`/`FsfFlags`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-080…K-089 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 `struct filp` 与 `filp[]` 为对象 | 04 全篇、02 §1.4（去重）、14 §2.6（去重） |

- 验收标准：`count==0` 空闲判据与 `get_fd` 的"双表预留"关系说清；`close_filp` 只给分支表并指向 20/21/22；`FSF_*` 位与 filp 选择字段的写入方/读取方各一行；Rust 现状以符号锚给出（旧文 `alloc_fd` 之类的错名消除）。

### 05-vnode-table

- 一句话定位：VFS 侧的 inode 缓存：双层引用计数、分配回收、锁与设备关联。
- 讲什么：K-100（缓存定位）、K-101（双层计数与 256 阈值）、K-102（分配/命中）、K-103（dup/put）、K-104（锁族与升级）、K-105（设备四元组）、K-106（分配清零）、K-107（clean_refs）、K-108（类型化）。
- 不讲什么：tll 原语（07，本篇只把"锁请求类型"与"锁定即不可分配"作为 vnode 自身语义讲完整，不展开等待/升级机制）；挂载表与穿越（06/13）；`req_putnode` 协议细节（12）；路径解析的使用（13）。
- 前置：01（init_vnodes 调用点）、04。（**不列 07**：vnode 篇自含"锁定与配对义务"的完整概念，07 是公共锁设施的下钻延伸，不作为读懂本篇的前提。）
- 后置：13、14、18、25。
- 事实底线：`vnode.h:4-29`、`const.h:8`、`vnode.c:85-124,138-152,156-224,227-315`；Rust：`vnode.rs`（`Vnode`/`VnodeTable`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-100…K-108 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 `struct vnode` 与 `vnode[]` 为对象 | 05 全篇、04 §2.6（去重）、06 §2.7（去重） |

- 验收标准：256 阈值（`vnode.c:263-264`）与 `vnode_clean_refs` 的触发条件、`req_putnode` 的批量语义可核对；`v_ref_count` 与 `v_fs_count` 两个计数的增减方各列一行；锁映射表逐条给出 `vnode.h:26-29` 的映射。

### 06-vmnt-table

- 一句话定位：设备号到 FS 端点的映射点：容量、锁、端点回收与挂载元数据。
- 讲什么：K-115（边界表）、K-116（`NR_MNTS=16` 真值）、K-117（分配/命中）、K-118（锁与 EXCL）、K-119（mark/clear 分化）、K-120（unmap 四步级联）、K-121（挂载元数据）、K-122（`m_comm` 字段归属）、K-123（死代码判定）、K-124（类型化）。
- 不讲什么：挂载流程（18）；FS 通信窗口（11，本篇只留字段声明）；路径穿越（13）；`fetch_vmnt_paths` 机制（99 台账，一行判定）；statvfs 填充（28）；tll 原语（07，同 05 的处理：本篇自含"锁定即不可分配/排他"概念）。
- 前置：01（init_vmnts 调用点）、05。（**不列 07**。）
- 后置：11、13、18、19、28。
- 事实底线：`vmnt.h:7-33`、`const.h:7`、`vmnt.c:65-90,95-197,180-192,246-287`（后者只留死代码判定）；Rust：`vmnt.rs`（`Vmnt`/`VmntTable`/`VmntLock`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-115…K-124 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 `struct vmnt` 与 `vmnt[]` 为对象 | 06 全篇（含纠错）、11 §1.2（去重） |

- 验收标准：`NR_MNTS=16` 与 `NR_NONEDEVS=NR_MNTS` 有 C 锚；`mark_vmnt_free` 与 `clear_vmnt` 的字段数差异表；`vmnt_unmap_by_endpt` 的四个动作（`fs_cancel`/`invalidate_filp_by_endpt`/`put_vnode`/标记）与 19 的触发顺序一致。

### 07-tll-lock

- 一句话定位：VFS 并发的原语——读/串行读/写三态锁与等待队列的写偏序。
- 讲什么：K-130（三态）、K-131（状态字段正交）、K-132（写偏序）、K-133（五路分发）、K-134（唤醒）、K-135（升降级）、K-136（谓词与空锁不变量）、K-137（类型化与 A-6）。
- 不讲什么：各使用方的语义（04/05/06/13/30 只留映射行）；worker 等待队列（08）；VFS 之外的锁（内核 BKL、mthread 内部）。
- 前置：无（独立原语）。
- 后置：04、05、06、13、30。
- 事实底线：`tll.h:6-18`、`tll.c:11-71,74-111,113-136,139-219,220-221,230-303,306-323`；Rust：`tll.rs`（`Tll`/`TllError`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-130…K-137 | 见 §2.2 | 混合 | 见 §2.2 | tll 是唯一主题 | 07 全篇 |

- 验收标准：`tll_lock` 的五条路径（直授读/直授写/升级等待/EBUSY 排队/写锁互斥）逐条给出条件与返回；`tll_unlock` 的队头选择规则用 `t_write`/`t_serial` 写清；`tll_t` 尺寸以实测结构字段重算（不再沿用旧文 24 字节的错误）；Rust 侧给出"单线程下等待如何表达"的明确契约（非阻塞返回 + 调用方状态机）。

### 08-worker-thread

- 一句话定位：从 mthread 九线程到九个请求槽——请求如何登记、门控、执行、挂起、复活、停机。
- 讲什么：K-145（A-1 执行模型）、K-146（两级计数）、K-147（双向绑定）、K-148（worker_start）、K-149（block_all/allow）、K-150（can_start 四象限）、K-151（suspend/resume）、K-152（wait/signal）、K-153（yield/set_proc）、K-154（stop 族）、K-155（thread_cleanup）、K-156（栈预算）、K-157（类型化）。
- 不讲什么：主循环的路由（09）；四条挂起路径的语义（17/23/21/22，每族一行）；tll 原语（07）；PM 请求内容（10）。
- 前置：02、03、07。
- 后置：09、10、17、23、33。
- 事实底线：`worker.c:1-607`（重点 `10-20`、`27-105`、`119-185`、`192-325`、`360-438`、`443-567`、`586-607`）、`threads.h:1-38`、`const.h:9`、`main.c:558-575`；Rust：`worker.rs`（`WorkerPool`/`WorkerState`/`WorkerFunc`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-145…K-157 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 worker 池为对象 | 08 全篇（ARCH A-1 章节为新增） |

- 验收标准：ARCH A-1 必须单列一节，给出 mthread 事实（`worker.c:52` `mthread_create`、`threads.h` 的 mutex/cond 宏）与 Rust 槽状态机的对照，并说明"阻塞隔离"能力如何由槽状态承担；`worker_start` 的四条 sanity 检查（`worker.c:381-407`）逐条对应；`worker_stop_by_endpt` 的 EIO 注入与 19 的端点回收衔接（`worker.c:555-567`）；旧文的 `WorkerFunc` 变体名错误（真实第五变体为 `PmPostponed`，`worker.rs:64`）与固定槽数组的描述全部修正。

### 09-main-loop

- 一句话定位：运行时心脏——八路优先的消息分派、64 项调用表、同步与异步回复、挂起重放。
- 讲什么：K-165（八路优先级）、K-166（reviving 优先）、K-167（transid 路由的定位面）、K-168（PM 守门）、K-169（notify 三路）、K-170（task 忽略）、K-171（设备回复三路）、K-172（call_vec 与别名）、K-173（handle_work 守门）、K-174（do_work 与 SUSPEND）、K-175（reply/replycode）、K-176（unblock 二路）、K-177（VfsState）、K-178（Route/PollResult）、K-179（统计开关的引用）。
- 不讲什么：transid 的编解码（11，本篇只讲"路由到哪里"）；`service_pm` 的请求内容（10）；`bdev_reply`/`cdev_reply`/`sdev_reply` 的解码（20/21/22）；`expire_timers` 的实现（23 标注库归属）；管道与记录锁的复活细节（17/30，本篇只讲 `unblock` 二路的选择）；worker 状态机（08）。
- 前置：01、08、02、03。
- 后置：10、11、13-33 全部（分发入口）。
- 事实底线：`main.c:54-141`（循环）、`main.c:146-298`（handle_work/do_reply/do_work）、`main.c:580-663`（get_work/reply）、`main.c:921-973`（unblock）、`table.c:17-82`、`glo.h:26-33`；Rust：`main_loop.rs`（`VfsState`/`Route`/`PollResult`）、`call_table.rs`（`VfsCallNum`）、`syscalls.rs`（`dispatch_syscall`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-165…K-179 | 见 §2.2 | 混合 | 见 §2.2 | 全部发生在 `main()` 主循环一帧内 | 09 全篇（transid 编码段迁 11） |

- 验收标准：八路顺序与 C 的 if/else 链一一对应且给出"不可交换"的理由；`call_vec` 64 项与 `callnr.h` 逐项对表（含 `RMDIR→do_unlink`、`RECVMSG→do_sockmsg`、`FCHMOD→do_chmod` 三处别名）；SUSPEND 的语义用"本次不回复、由谁补"的表格说明；`unblock` 二路（管道重放 FALSE / 锁整请求 TRUE）与 `main.c:965-972` 一致；transid 段的引用改指向 11 且不再出现 0x1000 之类的错值。

### 10-pm-protocol

- 一句话定位：VFS_PM 十二请求与进程生命周期——fork 次主线、凭证、退出回收、重启。
- 讲什么：K-185（两侧权威）、K-186（12 请求/11 回复）、K-187（三级调度）、K-188（postponed 四分支）、K-189（pm_fork 四步）、K-190（SRV_FORK）、K-191（free_proc 两阶段）、K-192（SESLDR/tty）、K-193（凭证四请求）、K-194（reboot 序列）、K-195（dumpcore 入口）、K-196（退出的资源回收顺序）、K-197（PmHandler 类型化）。
- 不讲什么：syscall 面 64 调用（14-33）；FS 通信队列（11）；驱动回复（20-22）；`close_fd`/`put_vnode` 的单步实现（14/05，本篇只给顺序与调用点）；exec/dumpcore 的机制（25/26，本篇只给调度与信封）；`service_pm_postponed` 中 EXEC/DUMPCORE 的载荷语义（25/26）。
- 前置：01、02、03、08、09。
- 后置：14（退出逆操作）、17（UNPAUSE）、25、26、31。
- 事实底线：`main.c:668-762`（postponed）、`main.c:764-915`（service_pm）、`misc.c:504-575`（reboot）、`misc.c:577-708`（fork/exit/free_proc）、`misc.c:726-791`（凭证）、`misc.c:903-943`（dumpcore）、`com.h:513-583`；`04-stage-pm/05-vfs-interaction.md`（对端状态机，交叉引用）；Rust：`ipc/dispatcher.rs`（`PmHandler`/`FreeKind`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-185…K-197 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 VFS_PM 协议为对象 | 10 全篇（退出顺序从 14/05 收拢） |

- 验收标准：12 请求逐个给出"调度级（立即/延期/独立）+ 目标进程关联 + 回复类型"；pm_fork 的四步在 `misc.c:577-634` 逐行可核对，且明确 `fp_lock` 保留属于槽；`free_proc` 的 FP_EXITING 分水岭解释"何时只清一部分"；"关闭与退出"单列小节，给出 fd→filp→vnode→vmnt 的回收顺序表并指向 14/05/06；旧文"立即 7 路含 SETGROUPS 两次"的计数错误修正为 6 路。

### 11-fs-comm

- 一句话定位：VFS 与所有文件系统的异步对话窗口——每挂载计数、排队补发、transid 定位。
- 讲什么：K-205（comm_t 窗口）、K-206（sendmsg/send_work/sendmore）、K-207（fs_cancel/queuemsg）、K-208（三类 sendrec）、K-209（三重守门）、K-210（transid 编码）、K-211（ERESTART 抑制）、K-212（类型化）。
- 不讲什么：`req_*` 包装与响应结构（12）；路径解析（13）；具体调用流程（14-33）；`vm_vfs_procctl_handlemem` 的 VM 语义（32 交叉引用）。
- 前置：06、08、09。
- 后置：12、13、16、18、20-22、25、31、32。
- 事实底线：`comm.c:12-244`、`type.h:1-10`、`glo.h:16-17`、`com.h:909-912`、`vfsif.h:79-81`；Rust：`fs_comm.rs`（`FsComm`/`GlobalComm`/`TransId`/`FsTransport`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-205…K-212 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 `comm.c` 与 `comm_t` 为对象 | 11 全篇（response 结构迁 12） |

- 验收标准：`sendmsg` 的窗口占用、transid 附加、`w_task` 记录、投递失败处理逐行可核对（`comm.c:12-32`）；`send_work` 的 sending 短路条件（`comm.c:37-45`）说清；`fs_sendmore` 的四守门（窗口/回调/队列/计数）表列全）；transid 宏的方向以 `vfsif.h:79-81` 为准（低 16 位为 id、高 16 位为原类型），并与旧文 09 的错误写法划清；`TransId` 的 newtype 与绝对值 pin 测试（`fs_comm.rs`）作为 Rust 事实列出。

### 12-request-wrappers

- 一句话定位：32 个活 REQ 的信封构造——grant 两段重试、响应回填、能力协商、死常量排除。
- 讲什么：K-220（REQ 常量与基址）、K-221（包装三重复）、K-222（grant 与 ERESTART）、K-223（node_details/lookup_res 结构）、K-224（RES 能力协商）、K-225（`_actual` 回填）、K-226（GETNODE 死常量）、K-227（类型化）、K-228（minix-types 收敛）。
- 不讲什么：窗口与队列（11）；`lookup_res` 的消费与 `EENTERMOUNT` 状态机（13）；底层 FS 服务端实现（15-stage-fs）；具体调用流程（14-33）。
- 前置：04、05、06、11。
- 后置：13、15、16、18、25、27、28。
- 事实底线：`request.c` 全部 `req_*` 与 `_actual`（36 个符号）、`request.h:12,25`、`vfsif.h:20-81`、`com.h:589`；`os/libs/minix-types/src/ipc/fs_driver.rs`（E-REQWIRE 后的单一权威）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-220…K-228 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 REQ 协议面与包装函数为对象 | 12 全篇 + edge E-REQWIRE 结果（新增 K-228） |

- 验收标准：33 常量/32 活的计数与 `NREQS=34` 的关系说清（`vfsif.h:41-75`）；`FS_BASE=0xA00` 与 `IS_FS_RQ` 掩码（`vfsif.h:77`）有绝对值测试锚；grant 两段用 `req_readwrite`/`req_breadwrite` 两个代表讲透，其余按差异表收束；`RES_64BIT` 的早拒绝点（`request.c:274,323`）列出；旧文的 `FsFlags::RES_64BIT` 命名与 `Fifo/Lifo` 越界内容修正。

### 13-path-lookup

- 一句话定位：名字到 vnode 的翻译机——解析循环、跨挂载穿越、符号链接环防护、锁协议。
- 讲什么：K-235（公共前置）、K-236（lookup 结构）、K-237（advance 双相）、K-238（eat_path/last_dir）、K-239（穿越三码）、K-240（SYMLOOP 16）、K-241（解析期锁协议）、K-242（canonical/get_name）、K-243（copy_path/fetch_name）、K-244（DO_POSIX）、K-245（do_socketpath 入口）。
- 不讲什么：`req_lookup` 的 grant 构造（12）；vnode/vmnt 表操作与锁原语（05/06/07，只留协议表）；`O_CREAT` 的创建机（15）；exec 的脚本路径（25）；内核 safecopy（01-stage-kernel/18-syscall-copy）。
- 前置：05、06、07、11、12。
- 后置：14-18、25、27、28、32。
- 事实底线：`path.c:40-933`（重点 `40-146`、`384-573`、`594-798`）、`path.h:4-12`、`utility.c:24-93`、`vfsif.h:11-28`；Rust：`path.rs`（`lookup`/`advance`/`eat_path`/`last_dir`/`LookupRes`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-235…K-245 | 见 §2.2 | 混合 | 见 §2.2 | 全部以路径解析为对象 | 13 全篇（name/symloop 纠错） |

- 验收标准：解析循环用一张图画出"起始 vnode → advance → 命中/穿越 → 末组件 → 返回 vnode/last_dir"；`EENTERMOUNT`/`ELEAVEMOUNT`/`ESYMLINK` 的处理位置与 `char_processed` 回带（`path.c:465-495`）可核对；符号链接循环计数用 `_POSIX_SYMLOOP_MAX` 而非死常量 `const.h:32`；`fetch_name` 的错误码与长度检查逐条对照 `utility.c:60-93`（旧文有三处错误）。

### 14-filedes

- 一句话定位：fd 表的分配与拆除、驱动死亡时的失效族、跨进程 fd 复制。
- 讲什么：K-250（fd/filp 两跳）、K-251（get_fd 最低空闲）、K-252（check_fds）、K-253（close_fd 拆除序）、K-254（invalidate 三族）、K-255（copyfd）、K-256（类型化）、K-257（虚构实现清理纪律）。
- 不讲什么：filp 结构与引用的管理（04）；`close_filp` 的分族动作（20/21/22，本篇给分流表）；vnode 回收（05）；select/记录锁的清理（23/30）；PM fork 的整表拷贝（10）。
- 前置：02、04、05、07。
- 后置：15、17、23、24、30、32。
- 事实底线：`filedes.c:88-105,110-156,250-308,524-656`、`open.c:690-727`；Rust：`filedes.rs`（`Fd`/`close_fd`/`copy_fd`/`invalidate_*`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-250…K-257 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 fd 表为对象 | 14 全篇（get_fd 从 04 收拢） |

- 验收标准：`get_fd` 的 start 下界与 EMFILE/ENFILE 两个耗尽点分清；`close_fd` 的"先摘索引"顺序与可以失败的位置表；`invalidate_*` 三族的匹配键（char major / sock drv / endpoint）与 `v_fs_e` 探针逐条对应（`filedes.c:250-308`）；copyfd 的 FROM/TO 方向以调用者/远端两个角色重述，四项守门（CLOEXEC 剥离、S_ISSOCK EDEADLK、`filp_ioctl_fp` EBADF、super_user）逐条列出。

### 15-open-close

- 一句话定位：一条路径名如何变成一个可读写的 fd——意图编码、创建/打开、类型分派、定位、拆除。
- 讲什么：K-265（对偶入口）、K-266（mode_map）、K-267（common_open 主管线）、K-268（new_node）、K-269（pipe_open 配对）、K-270（mknod/mkdir）、K-271（lseek）、K-272（关闭调用点）、K-273（类型化）。
- 不讲什么：路径解析（13）；fd 与 filp 的表操作（14/04，只留调用点）；设备三族打开（19/20/21，只留分派行）；管道阻塞语义（17）；权限判定（29，只留 `forbidden` 调用点）。
- 前置：04、13、14。
- 后置：16、17、21、24、27、30。
- 事实底线：`open.c:29-727`；Rust：`open.rs`（`common_open`/`new_node`/`seek_pos` 符号）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-265…K-273 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 open 族调用为对象 | 15 全篇（FIFO/BLK 分支与 close_filp 移出） |

- 验收标准：`common_open` 的七步管线与六分支分派用一张流程图给出，每步带行锚；`mode_map` 四值表（0/1/2/非法）与 `O_TRUNC` 的交互；`new_node` 的 `EEXIST`、符号链接重解、七字段落定（`NodeDetails`）逐项；`actual_lseek` 的三原点与 `EOVERFLOW`/`ESPIPE` 两个错误；不再重述 `close_filp` 的驱动分支。

### 16-read-write

- 一句话定位：数据面的五路分派与位置推进——读、写、偷看共用一台机器。
- 讲什么：K-280（三向同机）、K-281（五路分派）、K-282（头校验与零短路）、K-283（位置推进与 O_APPEND）、K-284（bsf 全局锁）、K-285（peek）、K-286（getdents）、K-287（SIGPIPE）、K-288（类型化）。
- 不讲什么：管道定量与挂起（17，本篇只给"FIFO 分支转 rw_pipe"）；字符/驱动数据面（21/20/22）；权限与只读（29，读写只查 filp 模式）；`req_readwrite` 协议（12）；select 的 peek 使用（23）。
- 前置：04、14、15。
- 后置：17、20、21、22、23。
- 事实底线：`read.c:30-393`、`write.c:15-25`、`const.h:77-79`、`glo.h:36`；Rust：`read_write.rs`（`IoRoute`/`BsfLock`/`apply_append`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-280…K-288 | 见 §2.2 | 混合 | 见 §2.2 | 全部以读写偷看为对象 | 16 全篇（pipe 数学移出） |

- 验收标准：`read_write` 的五路条件与各路的"读/写/偷看"三向合法性矩阵；`filp_pos` 推进与返回值的同源关系（`read.c:262`）；`O_APPEND` 的重定起点（`read.c:234`）；`lock_bsf`/`unlock_bsf` 的快慢道与 `check_bsf_lock` 的卸载断言（`read.c:49-87`）；SIGPIPE 击发条件（EPIPE + 写给管道 + 非 `O_NOSIGPIPE`）与 `O_NOSIGPIPE` 的来源（30 的 fcntl）。

### 17-pipe

- 一句话定位：VFS 阻塞语义的家——定量、挂起、唤醒、中断的完整闭环。
- 讲什么：K-295（等待三处境）、K-296（pipe_check 读三路写五路）、K-297（create_pipe 回滚）、K-298（map_vnode）、K-299（suspend 登记）、K-300（susp_count/reviving 账本）、K-301（release 唤醒谓词）、K-302（revive/unpause）、K-303（unsuspend_by_endpt）、K-304（统一 PFS）、K-305（类型化）。
- 不讲什么：读写机的五路分派（16）；select 的登记与回复（23，只留回调点）；字符/socket 的取消执行（21/22，只留分类）；`replycode` 与主循环（09）；`req_newnode` 协议（12，只留调用点）。
- 前置：04、08、09、16。
- 后置：23、24、30。
- 事实底线：`pipe.c:39-561`、`glo.h:14,16`、`fproc.h:31-37`；Rust：`pipe.rs`（`SuspLedger`/`release_match`/`MapVerdict`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-295…K-305 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 pipe.c 为对象 | 17 全篇（FIFO 从 15 收拢） |

- 验收标准：`pipe_check` 读三路（非空全给/空有写者等/空无写者回 0）与写五路（EPIPE/原子 EAGAIN/部分/满挂起/容量足）逐条；`susp_count` 与 `reviving` 的生产/消费点全表（含 `unblock`，`main.c:959`）；`release` 的唤醒六元合取与 `select_callback` 的联动；`unpause` 六分支（含 socket 自回复例外，`pipe.c:548-549`）给出"有进展回余数 + EINTR"的规则。

### 18-mount

- 一句话定位：单树跨 FS 的嫁接与拆除——根挂载、伪设备、挂载点粘合、卸载扫荡。
- 讲什么：K-310（嫁接模型）、K-311（设备编解码）、K-312（伪设备）、K-313（mount_fs 提交回滚）、K-314（EBUSY 双义）、K-315（update_bspec）、K-316（do_mount 守门）、K-317（根特例）、K-318（mount_pfs）、K-319（卸载与扫荡）、K-320（name_to_dev）、K-321（类型化）。
- 不讲什么：vmnt 表槽与锁（06）；`req_readsuper`/`req_mountpoint`/`req_unmount` 信封（12）；dmap 表的完整机制（19；本篇只把"按设备号找驱动端点与标签"作为挂载入口的自含前置讲完，19 是该表的展开与生死治理）；块 I/O 改道（20）；statvfs 填充（28）；`mount_pfs` 的启动调用点（01，只留指针）。
- 前置：01、06、12、13。（**不列 19**：dmap 查找的最小概念在 18 本地讲完整，19 后置展开。）
- 后置：20、28、31。
- 事实底线：`mount.c:31-653`（重点 `46-80`、`85-150`、`156-385`、`391-425`、`430-585`、`590-653`）；`sys/types.h` 的设备宏；Rust：`mount.rs`（`MountPhase`/`RootStage`/`UnmountPlan`/`PfsMountPlan`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-310…K-321 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 mount.c 为对象 | 18 全篇（pfs 从 01 收拢） |

- 验收标准：`mount_fs` 提交五段（查重/占槽/挂载点/根 vnode/端点与能力）与失败回滚的"拿什么回什么"逐段对应；`have_root` 0/1/2 三态与 `MAKEROOT` 换根（`mount.c:327-328`）说清"根可以挂两次"；`mount_pfs` 三标签（`mount.c:391-425`）与 PFS 端点；`unmount` 七步与 `req_unmount` 失败忽略的例外（`mount.c:522-524`）、PFS 无根例外（`mount.c:530-535`）；`unmount_all` 的三锁断言。

### 19-device-map

- 一句话定位：驱动通讯录与生死治理——dmap/smap 表、注册与恢复、ioctl 分派、DS 事件、死亡级联的触发点。
- 讲什么：K-330（dmap 表）、K-331（CTTY）、K-332（smap 表）、K-333（注册与回滚）、K-334（启动映射）、K-335（恢复三态）、K-336（smap 注册驱散）、K-337（ioctl 三路）、K-338（grant 授权）、K-339（ds_event）、K-340（死亡触发总纲）、K-341（类型化）。
- 不讲什么：各族数据面（20/21/22）；`invalidate_filp` 三族的实现（14，只留调用点）；select 唤醒（23）；worker_stop（08）；内核 grant 表（01-stage-kernel/18-syscall-copy）；DS 服务端（07-stage-ds）。
- 前置：01、04、06、09。
- 后置：20、21、22、23。
- 事实底线：`dmap.h:16-33`、`dmap.c:27-328`、`smap.c:22-273`、`device.c:18-95`、`misc.c:949-988`、`const.h:10,34`、`com.h:919-990`；Rust：`device_map.rs`（`DmapEntry`/`SmapEntry`/`RegisterPlan`/`RecoverVerdict`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-330…K-341 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 dmap/smap/device 表与分派为对象 | 19 全篇 + 死亡触发清单（新增 K-340） |

- 验收标准：dmap 八字段与"路由四位 + 执行四态"的分组表；CTTY 只在落表时出现（`dmap.c:244-246`）；`do_mapdriver` 的双表回滚（smap 失败拆 dmap）；`dmap_endpt_up` 三态与块/字符分化；`make_ioctl_grant` 的方向交叉（IOR→CPF_WRITE、IOW→CPF_READ）与尺寸分流（`device.c:65-95`）；死亡触发小节列出"unmap → 四类后果（filp 失效/sdev 停尸/select 唤醒/vmnt 清理）"并指向 14/22/23/06。

### 20-bdev

- 一句话定位：块设备的开/关/控直达与重试熔断、死信分类、换人通知。
- 讲什么：K-350（管辖分工）、K-351（重试五次）、K-352（死信三分类）、K-353（reply 三重门）、K-354（bdev_up 两轮）、K-355（bsf 守卫）、K-356（类型化）。
- 不讲什么：块读写走 FS 的路径（12/16）；`drv_sendrec` 的传输实现（11）；dmap 表与恢复调用点（19）；`req_newdriver` 协议（12）；bsf 锁原语（16）。
- 前置：11、16、19。
- 后置：23（字符族对照）、99。
- 事实底线：`bdev.c:1-282`、`device.c:36-40`、`com.h:963-990`、`errno.h` 的服务端错误语义；Rust：`bdev.rs`（`SendTransport`/`RetryState`/`classify_send`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-350…K-356 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 bdev.c 为对象 | 20 全篇 |

- 验收标准：重试五次的判定与 `ERESTART` 恢复分支（`bdev.c:34-73`）逐行；死信三分类的三种后续动作（清表/记录/返回）；`bdev_reply` 三重门（来源端点/w_task/请求配对）与"必须非阻塞"的铁律（`bdev.c:190` 注释）；`bdev_up` 两轮（重开轮遇错全弃、通告轮遇错继续）与根兜底（`bdev.c:277-281`）。

### 21-cdev

- 一句话定位：字符设备的改道、克隆、开合对话、挂起读写与取消换码。
- 讲什么：K-365（挂起/同步分叉）、K-366（tty 改道）、K-367（查表四步）、K-368（clone）、K-369（opcl/CTTY/NOCTTY）、K-370（io 挂起与授权）、K-371（cancel/reply 换码）、K-372（select 旁路）、K-373（类型化）。
- 不讲什么：`asynsend3` 传输（01-stage-kernel）；worker 等待与复活调度（08/09，只留"挂起登记"）；`req_newnode`/PFS 细节（12，只留调用点）；select 的 cdev 回复解码（23）；tty 行规程（16-stage-drivers）；`invalidate_filp_by_char_major` 实现（14，只留调用点）。
- 前置：02、14、15、19。
- 后置：23。
- 事实底线：`cdev.c:1-508`、`com.h:921-956`、`dmap.h:25-26`；Rust：`cdev.rs`（`tty_redirect`/`resolve_gate`/`OpenEffects`/`ReplyClass`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-365…K-373 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 cdev.c 为对象 | 21 全篇 |

- 验收标准：`cdev_map` 改道与 `fp_tty` 的来源（`fproc.h:27`）；`cdev_clone` 的"旧结换新"与九字段更新；`cdev_opcl` 的 CTTY 短路与 NOCTTY 三条件（会话首领/无主/没说不，`cdev.c:185-191`）；`cdev_cancel` 的去程 EAGAIN→EINTR 与 `cdev_reply` 的回程 EINTR→EAGAIN（`cdev.c:418,473`）双向换码配对；授权方向的"看用户许可、看驱动方向"（读配 CPF_WRITE）与 19 同源声明。

### 22-sdev

- 一句话定位：套接字驱动的长短问、挂起登记、取消与复活，并在末节收束全部设备族的死亡级联。
- 讲什么：K-380（长短问三态）、K-381（suspend 三形状）、K-382（三授权）、K-383（建字）、K-384（绑定族）、K-385（长问族）、K-386（停尸与路由）、K-387（accept 特办）、K-388（EIO 口径）、K-389（死亡级联总谱）、K-390（类型化与宿主迁移）。
- 不讲什么：`resume_accept`/`recvfrom`/`recvmsg` 的续作执行（24，只留分类）；select 的 sdev 回复解码（23）；授权内核侧（01-stage-kernel）；套接字域与选项语义（24）；`smap` 查表（19）。
- 前置：02、14、19、21。
- 后置：23、24。
- 事实底线：`sdev.c:1-1114`（重点 `59-110`、`119-273`、`340-586`、`679-758`、`759-1114`）、`com.h:1034-1050`；Rust：`os/libs/minix-sockdriver/src/sdev.rs`（`SdevRequest`/`SockChannel`/`ReplyRoute`/`grant_trio`）与 `os/servers/vfs/src/sdev.rs`（`suspend_aux`/`finish_kind`/`stop`）两个宿主。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-380…K-390 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 sdev.c 为对象 | 22 全篇 + 总谱（新增 K-389） |

- 验收标准：短问/长问/选择即返的分界与"非阻塞仍按长问处理"的规则；`sdev_suspend` 三形状（accept 要 fd、recvmsg 要 buf、其余皆无）与非法形状断言；三授权的"按需分配 + 挂起时全置 + 无条件撤销"不变式；`sdev_stop` 的 EIO 注入与 `sdev_cancel` 的"不认识的取消不回答"；**《死亡级联总谱》必须包含**：触发事件（`dmap_unmap_by_endpt`/`smap_unmap_by_endpt`）、四类后果与各自锚点（`filedes.c:250-308`、`sdev.c:912`、`select.c:884`、`vmnt.c:180`）、以及"调用者先死的取消分支"（Redox cancellation 对照）；旧文所有指向 `os/servers/vfs/src/sdev.rs` 的 Rust 锚点改指 `minix-sockdriver` 或本地小文件。

### 23-select

- 一句话定位：一问等一群——四分型、两波回答、超时与死亡善后。
- 讲什么：K-395（群查询本质）、K-396（四分型）、K-397（选择表与 filp 字段）、K-398（filter 三出口）、K-399（驱动请求与管道试探的调度面）、K-400（位图双译）、K-401（取消/复活/回复）、K-402（超时与 CLOCK）、K-403（死亡善后）、K-404（类型化）。
- 不讲什么：cdev/sdev 请求的组包与发送（21/22，本篇只给"投递门与挂起"）；pipe_check 的定量（17，本篇只给"试探的调用"）；`copy_fdsets` 的内核拷贝实现（01-stage-kernel/18-syscall-copy，本篇只给取整与方向）；定时器库实现（`libsys/timers.c`，标注库归属）。
- 前置：02、04、09、17、21、22。
- 后置：24。
- 事实底线：`select.c:1-1416`（重点 `31-91`、`96-341`、`409-616`、`621-708`、`714-1269`、`884-951`、`1318-1416`）、`file.h:26-32`；`libsys/timers.c:97`（`expire_timers` 归属）；Rust：`select.rs`（`FdKind`/`SelectDriver`/`SelectVerdict`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-395…K-404 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 select.c 为对象 | 23 全篇（字段数/函数名纠错） |

- 验收标准：四分型条件的精确表述（`select.c:85-91`）与未知类型 `EBADF`；两波的触发条件与 `UPDATE|BUSY` 延期门（`select.c:346-362`）；`selectentry` 字段数（20）与 `select_cancel_all`/`select_cancel_filp` 的正确命名；超时三态（不阻塞/无限/有限）与 `TMRDIFF_MAX` 截断规则；CLOCK notify 的入口（`main.c:112`）与库实现归属；死亡善后两路（整槽取消/标就绪，`select.c:903-935`）。

### 24-socket

- 一句话定位：socket 系统调用的上层调用面——创建、清理、阻塞恢复与消息头。
- 讲什么：K-410（两层分工）、K-411（创建与清理表）、K-412（make_sock_fd）、K-413（fd/类型检查）、K-414（恢复四分支）、K-415（msghdr 与标志）、K-416（accept 继承/backlog）、K-417（类型化）。
- 不讲什么：`sdev_*` 驱动对话执行（22）；select 登记与恢复（23）；fd 表执行（14，只留预检规则）；`req_newnode` 分配（12，只留规格）；`do_socketpath` 路径行走（13，本篇只给边界）；挂起/回复调度（08/09）。
- 前置：14、19、22、23。
- 后置：99。
- 事实底线：`socket.c:1-762`（重点 `40-170`、`176-302`、`340-477`、`539-651`、`653-762`）、`sys/socket.h` 的 `SOCK_*`/`SHUT_*`/`MSG_*`；Rust：`socket.rs`（`SockError`/`BuildStep`/补偿表）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-410…K-417 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 socket.c 为对象 | 24 全篇（函数数/常量错锚修正） |

- 验收标准：创建三步检查（域→槽位→分配安装）与失败清理表逐行（含 socketpair 的双路清理）；`make_sock_fd` 九步与"失败返回负码、清理归调用者"；`get_sock` 的两个错误分流与解锁说明（参数判定后即解锁，`socket.c:292-300`）；四类恢复分支与 accept 的监听 fd 验证、recv 侧"类型上不可能再阻塞"；`sendmsg`/`recvmsg` 的 msghdr 单项限制与标志换算表。

### 25-exec

- 一句话定位：进程执行体替换的 VFS 侧——打开与检查、脚本与动态链接、装载、收尾。
- 讲什么：K-425（地址空间替换）、K-426（三项检查）、K-427（脚本）、K-428（动态链接）、K-429（装载双路径）、K-430（辅向量）、K-431（clo_exec）、K-432（suid 一次性）、K-433（收尾顺序）、K-434（类型化）。
- 不讲什么：`VFS_PM_EXEC` 的调度与信封（10）；路径解析（13）；权限判定与凭证规则（29/02，只留检查点）；`req_*` 读文件（12）；VM 侧 mmap/清零执行（02-stage-vm/20-vm-mmap.md）；PM 侧新进程通告（04-stage-pm/17-exec.md）。
- 前置：10、13、14、15。
- 后置：26、33。
- 事实底线：`exec.c:43-763`（重点 `89-180`、`185-402`、`404-517`、`519-731`）、`syslimits.h` 的 `ARG_MAX`/`PATH_MAX`/`DEFAULT_STACK_LIMIT`；Rust：`exec.rs`（`ExecPhase`/`AuxKind`/`check_seg`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-425…K-434 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 exec.c 为对象 | 25 全篇 |

- 验收标准：三项检查顺序（REG→X 权限→stat）与 `for_exec` 的换位（X_BIT 替换）；脚本三步与两处 `ENOEXEC` 条件；动态链接的"主文件 fd 保留、interp 文件另开"；`exec_loaders[]` 的首胜 + 哨兵结构；辅向量七项与 `AT_NULL` 封口、越界 `ENOEXEC`；`suid` 只用一次的三步轨迹（1,1→1,0→0,0）；收尾三步的顺序理由。

### 26-coredump

- 一句话定位：已停止进程的 ELF 转储——四部分布局、注释段、段数据策略。
- 讲什么：K-440（四部分与写入序）、K-441（ELF/程序头）、K-442（注释双段与衬垫）、K-443（段数据三策略）、K-444（VM 区表批取）、K-445（准备与收尾）、K-446（类型化）。
- 不讲什么：`pm_dumpcore` 的调度与 `free_proc(EXITING)` 收尾（10）；VM 区表查询实现（02-stage-vm）；FS 直写的实现（16）；`open` 的实现（15）；信号语义（04-stage-pm/11-13）。
- 前置：10、15、16、25。
- 后置：99。
- 事实底线：`coredump.c:32-327`（重点 `32-116`、`126-227`、`233-327`）、`misc.c:903-943`（入口与收尾）；`sys/elf_core.h` 的 `MINIX-CORE` 名称；Rust：`coredump.rs`（`DumpPhase`/`ElfTarget`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-440…K-446 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 coredump.c 为对象 | 26 全篇（步数与测试表错位修正） |

- 验收标准：四部分（ELF 头/程序头表/注释段/段数据）的写入顺序与 offset 计算（`adjust_offsets`）；程序头条数 = 1 + 注释 + 段数，`MAX_REGIONS=100` 上限行为（告警即返回而非截断）；注释段双 Nhdr 与 `PAD_LEN` 对齐；段数据三策略（有页复制/缺页补零/超 `LONG_MAX` 截断）与寄存器读失败"记而不断"；内部计数统一为 `DumpPhase` 的七步。

### 27-link

- 一句话定位：目录项与 inode 的分离语义——链接、删除、重命名、截断、符号链接。
- 讲什么：K-455（目录项/inode 分离）、K-456（硬链接三检查）、K-457（unlink/rmdir 与粘滞位）、K-458（rename 约束）、K-459（truncate 族与长度相同跳过）、K-460（slink/readlink）、K-461（类型化）。
- 不讲什么：路径解析（13）；权限判定与 `forbidden`（29，只留检查点）；fd 取 vnode（14，只留规则）；`req_*` 信封（12）；`O_TRUNC` 的打开语义（15）。
- 前置：13、14、15。
- 后置：99。
- 事实底线：`link.c:1-508`（重点 `29-164`、`169-271`、`276-381`、`387-508`）、`const.h:15`（`SU_UID`）、`_POSIX_SYMLINK_MAX`；Rust：`link.rs`（`LinkLookup`/`FsLink`/`sticky_check`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-455…K-461 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 link.c 为对象 | 27 全篇 |

- 验收标准：硬链接的"同设备（EXDEV）+ 目标目录 W|X + 双路径非目录"三检查；删除的"先验证再减计数"与 rmdir 分流（`link.c:156-159`）；粘滞位规则（目录属主/文件属主/root 三者关系）与二次解析；rename 的"同目录自死锁断言"（`link.c:240`）与旧文件名缓存；truncate 的"长度相同跳过"与 `truncate_vnode` 仅 REG/PIPE、不比较长度（被 `O_TRUNC` 复用）。

### 28-stadir

- 一句话定位：cwd/root 锚位、切换检查、stat 族与 statvfs 族的查询面。
- 讲什么：K-470（锚位语义）、K-471（切换检查与引用替换）、K-472（chdir/fchdir）、K-473（chroot）、K-474（stat 双入口/lstat）、K-475（statvfs 实时/缓存与三名称）、K-476（getvfsstat 遍历）、K-477（类型化）。
- 不讲什么：路径解析（13）；权限判定（29，只留执行位检查点）；`req_stat`/`req_statvfs` 信封（12）；fd 取 vnode（14）；挂载表锁（06，只留"锁后复验"的规则）；用户缓冲拷贝执行（01-stage-kernel/18）。
- 前置：02、06、12、13、14。
- 后置：29、31。
- 事实底线：`stadir.c:32-446`（重点 `32-135`、`140-192`、`197-289`、`294-446`）、`vmnt.h:14-20`；Rust：`stadir.rs`（`StadirCall`/`StatvfsFresh`/`MountNames`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-470…K-477 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 stadir.c 为对象 | 28 全篇（顺序矛盾与代码块缺陷修正） |

- 验收标准：`change_into` 的三项检查与"先加新引用、再放旧引用、最后交换"的顺序理由（`stadir.c:131-133`）；chroot 的 root 门与边界闭包；stat 的 `PATH_RET_SYMLINK` 差一标志（`stadir.c:433`）；statvfs 的 17 字段回填与 `ST_RDONLY` 叠加、`fsid` 双格式、三名称直拷（`stadir.c:279-285`）；`getvfsstat` 的两阶段（无缓冲只计数）与锁后复验、部分填充语义。

### 29-protect

- 一句话定位：九位权限的判定与修改——chmod/chown/umask/access 与只读挂载。
- 讲什么：K-485（九位判定）、K-486（过期 id 与 root 边界）、K-487（组并查）、K-488（chmod）、K-489（chown）、K-490（umask）、K-491（access）、K-492（read_only）、K-493（类型化）。
- 不讲什么：凭证字段与 pm_setuid 族（02/10，只留"判定所用 id"）；路径解析与 fd 取 vnode（13/14）；`req_chmod`/`req_chown` 信封（12）；exec 的 X 检查点（25，本篇提供 `forbidden`）。
- 前置：02、06、10、13、14。
- 后置：15、25、27、28、31。
- 事实底线：`protect.c:25-302`、`utility.c:128-141`、`minix/include/minix/const.h` 的 `R_BIT`/`W_BIT`/`X_BIT`/`I_SET_*`；Rust：`protect.rs`（`forbidden_decision`/`umask_swap`/`readonly_gate`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-485…K-493 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 protect.c 为对象 | 29 全篇（函数数纠错） |

- 验收标准：九位判定的"按 uid/gid 选档 + `(perm|want)==perm` 子集判断"逐行；过期 id（-1）与 root 的边界（含无 X 位时 root 的通过与拒绝）；chmod 的"属主检查→只读→清 setgid"顺序与"非属主组清 setgid"；chown 的禁止转交（普通用户只能保持 uid/gid 不变）与 -1 哨兵、2^31-2 越界；umask 的取反存返配对；access 的 real/eff 选择与 F_OK 空集；只读检查作为 `forbidden` 最后一步（`protect.c:282-284`）。

### 30-fcntl-lock

- 一句话定位：fcntl 十三命令与八槽劝告锁——复制、标志、打洞、区域锁与广播唤醒。
- 讲什么：K-500（命令分派）、K-501（DUPFD）、K-502（GETFL/SETFL/CLOEXEC/NOSIGPIPE）、K-503（兼容矩阵）、K-504（区域计算）、K-505（SETLK/SETLKW）、K-506（解锁四分支）、K-507（close 清锁）、K-508（FREESP/FLUSH）、K-509（类型化）。
- 不讲什么：fd 分配与 close 实现（14，只留下限检查与清锁调用点）；filp 字段与计数（04，只留标志读写）；阻塞载荷结构（02，只留存档内容）；`VFS_FCNTL` 分发（09）；FS 对话执行（12）。
- 前置：02、04、09、14。
- 后置：31（flush 与同步的关联）。
- 事实底线：`misc.c:117-267`、`lock.c:21-192`、`lock.h:7-13`、`const.h:6,21`；Rust：`fcntl.rs`（`LockRegion`/`LockTable`/`FcntlCmd`/`FlockWait`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-500…K-509 | 见 §2.2 | 混合 | 见 §2.2 | 全部以 `do_fcntl` 与 `lock.c` 为对象 | 30 全篇（F_DUPFD 锚点纠错） |

- 验收标准：十三命令表（含四个无行为命令 `EINVAL`）；DUPFD 的 `[0, OPEN_MAX)` 下限与复制共享；GETFL/SETFL 的不对称与 `O_NOSIGPIPE` 双向哨兵；兼容矩阵三条（读读相容/读写冲突/同进程不冲突）与"解锁不限持有者"；区域三数双向溢出检查与 `len=0` 至文件尾 `MAX_FILE_POS`；SETLK→EAGAIN 与 SETLKW→SUSPEND 的挂起存档三项；解锁四分支（清除/头缩/尾缩/分裂）与 `lock_revive` 的广播 + `unblock` 重判；close 清锁只扫同进程同 vnode。

### 31-sync-and-time（新，拆分自旧 31）

- 一句话定位：数据与元数据的持久化时点——sync 广播、fsync 定点、utimens 双入口。
- 讲什么：K-515（sync）、K-516（fsync）、K-517（utimens）、K-518（写回触发与无返回值）、K-519（只读同步交互）。
- 不讲什么：VM 上游请求（32）；系统信息与控制面（33）；FS 侧落盘实现（15-stage-fs）；`req_flush`/`req_sync` 信封（12，只留调用点）；挂载表锁（06，只留"锁不上即停"的规则）。
- 前置：06、12、13、14、29。
- 后置：99。
- 事实底线：`misc.c:276-326`、`time.c:26-155`、`request.c:1134`（`req_sync`）；Rust：`misc.rs`（`sync`/`fsync`/`utimens` 决策函数）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-515…K-519 | 见 §2.2 | 混合 | 见 §2.2 | 三者的共同语义是"写回触发" | 旧 31 §1.3/§1.7/§2.2/§2.6 拆出 |

- 验收标准：sync 的三条件（有设备号/有 FS 端点/有根 vnode）与"不收集结果"；fsync 的 fd→卷定位与无效 fd 的返回；utimens 的 NOW/OMIT/显值三态与 `UTIME_NOW`/`UTIME_OMIT`、路径与 fd 双入口；只读挂载卷被跳过的规则与证据；Rust 侧三函数以符号锚列出。

### 32-vm-call（新，拆分自旧 31）

- 一句话定位：VM 对 VFS 的上游协议——`VFS_VMCALL` 六域、三种请求、异步回复与 `dupvm`。
- 讲什么：K-525（消息六域）、K-526（FDLOOKUP/FDCLOSE/FDIO）、K-527（来源门与异步回复）、K-528（dupvm）、K-529（fdref 对端）、K-530（wire 定稿纪律）。
- 不讲什么：VM 侧发送半与 fdref 实现（02-stage-vm/23-vfs-interaction.md）；fd/filp 表操作（04/14，只留调用）；FS 侧缺页 I/O 的后续（15-stage-fs）；`req_peek`/`req_bpeek` 信封（12）。
- 前置：04、05、14。
- 后置：33（同一文件的旁路调用分家声明）。
- 事实底线：`misc.c:328-500`、`com.h:702-707`、`mess_10` 字段布局；`os/servers/vfs/src/misc.rs`（`VmVfsReq`/`VM_VFS_REPLY`）、`minix-types`（`MessVmVfsCall`/`MessVmVfsReply`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-525…K-530 | 见 §2.2 | 混合 | 见 §2.2 | 共同语义是"VM↔VFS 的专用协议" | 旧 31 §1.4/§1.5/§2.3/§2.4 拆出 |

- 验收标准：六域字段与 C 的 `VFS_VMCALL_*` 宏逐项对照；三个请求的语义与返回字段；来源门（`m_source != VM_PROC_NR → ENOSYS`）与"回复恒为 `VM_VFS_REPLY`、结果恒异步"的契约；`dupvm` 两项检查（HASPEEK、REG）与 filp 计数共享；wire 绝对值以 `minix-types` 常量与 C 断言测试为准（不再出现本地重复定义）。

### 33-system-info（新，拆分自旧 31）

- 一句话定位：VFS 的系统信息与控制面——快照查询、运行期开关、覆盖率、废弃调用与调试钩子。
- 讲什么：K-535（getsysinfo）、K-536（快照布局与生产面）、K-537（svrctl）、K-538（gcov_flush）、K-539（getrusage）、K-540（panic_hook）、K-541（调用统计开关）。
- 不讲什么：VM 上游协议（32）；sync/time（31）；IS/MIB 的消费端（08-stage-is/10-stage-mib）；内核 `sys_safecopy` 实现（01-stage-kernel/18）；`ds_event`（19）。
- 前置：03、06、09、19。
- 后置：99（省略台账与错误码纪律）。
- 事实底线：`misc.c:52-116`（getsysinfo）、`misc.c:797-902`（svrctl）、`misc.c:989-1006`（panic_hook/getrusage）、`gcov.c:10-73`、`main.c:32-34,286-289`（统计）、`table.c:60`（obsolete 注释）；`minix-types/types/fproc.rs`（`FProcSnap` 52 字节）与 `os/servers/vfs/src/fproc.rs`（`to_fproc_snap`）；Rust：`misc.rs` 的决策组（`gcov_*`/`svrctl_*`）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-535…K-541 | 见 §2.2 | 混合 | 见 §2.2 | 共同语义是"管理/观测面查询与控制" | 旧 31 §1.2/§1.6/§1.8/§2.1/§2.5/§2.7/§2.8 拆出 |

- 验收标准：getsysinfo 的 root 门与长度门、四类表（`SI_PROC_TAB`/`SI_DMAP_TAB`/`SI_PROCLIGHT_TAB`/`SI_CALL_STATS`）与 `fproc_light` 三字段的生产映射；`svrctl` 的 verbose/统计开关；gcov 五门（label 长度/端点/grant/目标/特权）；`getrusage` 的"外部行为是恒 OK"与 `table.c:60` 的 obsolete 注释；`ENABLE_SYSCALL_STATS` 的 cfg 语义（默认关闭、无表）；`panic_hook` 不移植的理由与 99 省略台账的互引。

### 99-global-concepts

- 一句话定位：横跨全阶段的词汇表——容量、wire 命名空间、全局状态映射、引用计数不变量、省略台账、错误码纪律、工程与验证事实。
- 讲什么：K-550（容量常量）、K-551（阻塞枚举契约）、K-552（wire 前缀总表）、K-553（全局状态映射）、K-554（三层引用计数不变量）、K-555（endpoint/transid）、K-556（上下文宏）、K-557（sys_datacopy_wrapper）、K-558（A-8 类型映射）、K-559（省略台账）、K-560（错误码纪律）、K-561（构建与工具链）、K-562（测试基建）、K-563（fail-closed 守卫清单）。
- 不讲什么：任何机制的完整流程（只允许"一行定义 + 指向主篇"）；各表的结构字段（04-06）；Rust 模块的逐文件说明（00 §模块地图）。
- 前置：00（本篇是索引，天然依赖全阶段）。
- 后置：无（收尾篇）。
- 事实底线：`const.h`/`glo.h`/`type.h`/`fs.h`/`proto.h`、`callnr.h:68`、`com.h:513/589/702-707/909-912/919/963/1037`、`vfsif.h:20-81`、`utility.c:142-186`；`minix-types/types/errno.rs` 与各 `*Error` 枚举；`servers/vfs/Makefile`、`os/servers/vfs/Cargo.toml`；Rust 测试实测数（快照日期）。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|--------------|------|
  | K-550…K-563 | 见 §2.2 | 混合 | 见 §2.2 | 跨模块共享且无其他自然归属 | 99 全篇 + 新增（错误码/构建/测试/守卫） |

- 验收标准：wire 总表八族（VFS 0x100、PM 0x900/0x980、FS 0xA00、TRANS 0xB00/0xB01、CDEV 0x400/0x480、BDEV 0x500/0x580、SDEV 0x1900/0x1980、VM 101-103/0xC1E）每行带 C 绝对锚与判别宏；容量九常量每行带 C 锚与 Rust 单一真源位置；三层引用计数不变量的增减方与失效族的关系图；错误码纪律给出"30 个枚举 → `ToErrno`"的机械检查命令；省略台账逐条带 C 锚和"不移植"理由；工程与验证小节给出 Makefile/Cargo 清单与"无 VFS 端到端测试"的诚实声明。

---

## 6. 变更表

### 6.1 操作总表

| 操作编号 | 操作类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 |
|----------|----------|--------|--------|------|-----------|------|
| OP-01 | 拆分 | `31-misc-queries.md` 全篇 | 新 31 `31-sync-and-time.md` | 旧 31 承载八个操作族，违反单篇单语义；三篇各有一个语义单元 | K-515…K-519 | 详见 §6.2 逐节表 |
| OP-02 | 拆分 | 旧 31 §1.4/§1.5/§2.3/§2.4 | 新 32 `32-vm-call.md` | VFS↔VM 是独立服务间协议，与 10/11 同族 | K-525…K-530 | 详见 §6.2 |
| OP-03 | 拆分 | 旧 31 §1.2/§1.6/§1.8/§2.1/§2.5/§2.7/§2.8 | 新 33 `33-system-info.md` | 管理/观测面查询与控制是一个语义单元 | K-535…K-541 | 详见 §6.2 |
| OP-04 | 重排 | 02 §2.6（两遍初始化） | 03（表级） | 与 03 §2.6 重复；初始化是表操作 | K-021/K-065 | 01 只留调用点 |
| OP-05 | 重排 | 02 §2.7 `fproc_light` | 03 | 与 03 §2.7 重复；数据定义与该篇同族 | K-066 | 33 保留生产面半 |
| OP-06 | 重排 | 02 §1.4/§2.6（filp 计数、dup_vnode） | 04/05/10 | 越界（§3.5 O-01） | K-083/K-101 | 02 只留字段 |
| OP-07 | 重排 | 03 §2.5（`who_p` 宏用法） | 09/99 | 宏定义归 03，使用归 09/99 | K-064 | 03 保留定义 |
| OP-08 | 重排 | 04 §2.3/§2.4（`get_fd`/fd 槽语义） | 14 | fd 层分配不属 filp 表 | K-251 | 04 只留 filp 查找 |
| OP-09 | 重排 | 05 §1.3/§1.4、06 §1.3（tll 教学） | 07 | 锁原语只在 07 完整讲 | K-104/K-118 | 05/06 只留映射与配对义务 |
| OP-10 | 压缩 | 06 §1.5/§2.7 `fetch_vmnt_paths` | 99 省略台账 | C 死代码（`vmnt.c:246` 零调用） | K-123 | 06 只留一行判定 |
| OP-11 | 重排 | 07 §1.3（EBUSY 排队与 worker_wait） | 08 | 等待落点属 worker | K-152 | 07 只留锁侧返回 |
| OP-12 | 重排 | 08 §1.1（四条阻塞路径细节） | 17/23/21/22 | 每族一篇（§3.5 O-08） | K-151 | 08 只留槽状态 |
| OP-13 | 重排 | 09 §2.10（TRNS 编解码） | 11 | 编码单一主家；且旧文方向写反 | K-210 | 09 只留路由 |
| OP-14 | 重排 | 10 §1.4/§2.5（close/put 单步细节） | 14/05 | 10 只留回收顺序 | K-196 | 见 10 契约 |
| OP-15 | 重排 | 11 §2.11（response 结构）、D6（light） | 12、03 | 结构与消费主家分离 | K-223/K-066 | 11 只留窗口 |
| OP-16 | 重排 | 12 §1.3/§2.5（lookup_res 消费） | 13 | 状态机属路径解析 | K-223/K-239 | 12 只留结构定义 |
| OP-17 | 重排 | 13 §1.4（tll 与锁族教学） | 07/05/06 | 解析篇只留解析期锁协议 | K-241 | — |
| OP-18 | 重排 | 14 §2.6/§2.7（lock_filp 细节与 close 分支） | 04、20/21/22 | 分族动作各归其篇 | K-087/K-253 | 14 留分流表 |
| OP-19 | 重排 | 15 §2.3/§2.4/§2.8（BLK/FIFO/close 细节） | 19/20、17、14 | §3.5 O-15 | K-269/K-272 | 15 只留调用点 |
| OP-20 | 重排 | 16 §2.7（`rw_pipe` 定量） | 17 | 定量机主家 17 | K-296 | 16 只留分派 |
| OP-21 | 重排 | 17 §2.2/§2.5（map_vnode、select 位清） | 17 主家、23 | 与 15/23 重复侧收束 | K-298/K-401 | — |
| OP-22 | 重排 | 18 §2.7（mount_pfs 调用点） | 18 主家；01 只留指针 | 重复展开 | K-318 | — |
| OP-23 | 重排 | 19 §2.11/§2.12（ioctl 分族再述） | 19 主家；20/21/22 引用 | 授权与分派单主家 | K-337/K-338 | — |
| OP-24 | 重排 | 20 §2.5（表扫描细节） | 04/06 | 谓词留在 20，表操作归表篇 | K-354 | — |
| OP-25 | 重排 | 21 §2.3/§2.4（newnode/CTTY 细节） | 12/05、02 | §3.5 O-21 | K-368/K-369 | — |
| OP-26 | 重排 | 22 §2.9/§2.13（select 与 accept 续作） | 23、24 | §3.5 O-22 | K-387 | — |
| OP-27 | 重排 | 23 §2.8/§2.10（驱动投递与拷贝执行） | 21/22、内核 18-syscall-copy | §3.5 O-23 | K-399/K-400 | — |
| OP-28 | 重排 | 24 §2.4/§2.10（PFS 分配与 fd 预检） | 12、14 | §3.5 O-24 | K-412/K-413 | — |
| OP-29 | 重排 | 25 §2.3/§2.4（权限/凭证、VM mmap） | 29/02、02-stage-vm/20 | §3.5 O-25 | K-426 | — |
| OP-30 | 重排 | 26 §2.10（pm_dumpcore 全流程） | 10 | 入口与收尾归 PM 协议 | K-445 | 26 只留转储 |
| OP-31 | 重排 | 28 §2.10（挂载表锁细节） | 06 | 表锁单主家 | K-476 | — |
| OP-32 | 重排 | 30 §2.12（close_fd 全函数） | 14 | 拆除序单主家 | K-507 | — |
| OP-33 | 新增 | — | 22 §死亡级联总谱 | 统一散在 7 篇的死亡处理（§3.3 G-04） | K-340/K-389/K-298（旧编号） | 19 触发清单 + 22 总谱 + 99 不变量 |
| OP-34 | 新增 | — | 99 §wire 总表 | 八族命名空间与判别宏（旧 99 只有四族且有错锚） | K-552 | — |
| OP-35 | 新增 | — | 99 §错误码纪律 | 规则"错误必须映射 errno"的总表 | K-560 | — |
| OP-36 | 新增 | — | 99 §工程与验证 | 构建制品与测试基建现状（§3.3 G-01/G-07） | K-561/K-562 | — |
| OP-37 | 新增 | — | 03/33 快照生产面 | A-7 缺口的现实翻转（Fix 后 `FProcSnap` 已落） | K-067/K-536 | — |
| OP-38 | 校正 | 22 §4.2/§7（Rust 宿主） | `minix-sockdriver` / 本地小文件 | 实现已迁移，旧锚全错 | K-390 | — |
| OP-39 | 校正 | 12 §1.2/§2.1（REQ 本地定义） | `minix-types::ipc::fs_driver` | E-REQWIRE 已收敛 | K-228 | — |
| OP-40 | 内容重建 | 其余编号不变的篇章（00-30 + 99） | 原编号原地重建 | 事实纠错 + 去重 + 锚点纪律（非结构性） | 池内全部 | §5 各契约 |

### 6.2 OP-01 拆分的存量知识点去向（旧 31 → 新 31/32/33）

双方向规则之存量方向：旧 31 的每一节逐条给出新位置，全部有去向，无删除。

| 旧 31 位置 | 旧内容 | 新位置 | 迁移类型 |
|-----------|--------|--------|----------|
| §1.0/§1.1 | 引言与"为什么杂项成篇" | 31/32/33 各自的 §1 引言 | 改写 |
| §1.2 批量查询 | getsysinfo 整表复制 | 33 §1.2 与 §2.1 | 原样搬移 |
| §1.3 落盘同步 | sync/fsync 广播与定点 | 31 §1.2 与 §2.1/§2.2 | 原样搬移 |
| §1.4 按 fd 查询 | `dupvm` | 32 §1.3/§2.3 | 原样搬移 |
| §1.5 上游请求 | `do_vm_call` 三请求 | 32 §1.2/§2.2 | 改写（补六域 wire） |
| §1.6 sysctl 开关 | `do_svrctl` verbose/统计 | 33 §1.3/§2.3 | 原样搬移 |
| §1.7 时间戳更新 | `do_utimens` | 31 §1.3/§2.3 | 原样搬移 |
| §1.8 参数探针与废弃调用 | gcov/getrusage/panic_hook | 33 §1.4/§2.4/§2.5 | 原样搬移 |
| §1.9 其他 OS 对照 | 杂项对照 | 三篇各留一段与其主题相关部分 | 拆分 |
| §1.10 小结 | 杂项收束 | 三篇各自小结 | 改写 |
| §2.1 `do_getsysinfo` | 流程 | 33 §2.1 | 原样搬移 |
| §2.2 落盘同步 | `do_sync`/`do_fsync` | 31 §2.1/§2.2 | 原样搬移 |
| §2.3 `dupvm` | 流程 | 32 §2.3 | 原样搬移 |
| §2.4 `do_vm_call` | 流程 | 32 §2.2 | 改写（接 `VmVfsReq` 现实） |
| §2.5 `do_svrctl` | 流程 | 33 §2.3 | 原样搬移 |
| §2.6 `do_utimens` | 流程 | 31 §2.3 | 原样搬移 |
| §2.7 `do_gcov_flush` | 流程 | 33 §2.4 | 原样搬移 |
| §2.8 废弃/钩子 | getrusage/panic_hook | 33 §2.5 | 原样搬移 |
| §2.9 域外三函数 | pm_reboot/free_proc/ds_event 边界注记 | 10（reboot/free_proc）、19（ds_event） | 搬移 |
| §3 D1 调用面复用 | 调用枚举 | 32 §D1（VmVfsReq）/33 §D1 | 拆分 |
| §3 D2 批量查询检查 | getsysinfo 门 | 33 §D2 | 原样搬移 |
| §3 D3 落盘同步选卷 | sync 选择 | 31 §D2 | 原样搬移 |
| §3 D4 按 fd 查询检查 | dupvm 门 | 32 §D3 | 原样搬移 |
| §3 D5 VM 上游请求 | vm_call 类型化 | 32 §D4/D5 | 改写 |
| §3 D6 sysctl 与时间戳 | 开关与 utimens | 31 §D3、33 §D4 | 拆分 |
| §3 D7 杂项三则与 FS 对话 | gcov/rusage/panic + 对话 trait | 33 §D5 | 原样搬移 |
| §3 ARCH 总表 | 跨篇 ARCH 行 | 99（汇总表）＋各篇 | 搬移 |
| §4.1 模块结构 | `misc.rs` 等 | 31/32/33 各自 §实现 | 拆分 |
| §4.2 核心符号表 | Rust 符号 | 31/32/33 各自符号表（符号锚） | 拆分 |
| §4.3 不变量 | 杂项不变量 | 31/32/33 各自不变量 | 拆分 |
| §5 测试要点 | 8 个测试声明 | 按函数归属拆到 31/32/33 的测试表 | 拆分 |
| §6 过渡 | 杂项 → 99 | 31 → 32 → 33 → 99 | 改写 |
| §7 参见 | 引用 | 三篇各自参见 | 拆分 |

新增方向（双方向规则之新增方向）：32 的新增知识点 K-525…K-530 的证据锚点为 `misc.c:380-500`（六域与三请求）、`com.h:702-707`（`VMVFSREQ_*`/`VM_VFS_REPLY` 绝对值）、`minix-types/ipc/vm.rs` 与 `os/servers/vfs/src/misc.rs`（`VmVfsReq` 解码）；33 的 K-536 证据为 `minix-types/types/fproc.rs`（`FProcSnap` 52 字节）与 `fproc.rs:to_fproc_snap`；31 的 K-519 证据为 `misc.c:286-295`。以上均非凭空出现。

### 6.3 已评估但否决的操作（留痕防止回潮）

| 编号 | 候选操作 | 否决理由 |
|------|----------|----------|
| R-01 | 合并 02 与 03 | 消费者几乎不相交；合并后 700 行且端点验证被字段清单淹没；消重可用 OP-04/05 达成（§4.0 第 4 条） |
| R-02 | 为"驱动死亡级联"新建独立篇 | 触发点（19）与四条后果路径（20-23）之间存在必然的前向引用，无位置可放（§4.0 第 5 条） |
| R-03 | 把 07-tll-lock 前移到 04 以消除 05/06 的锁依赖 | 05/06 可自含"锁定即不可分配"完整概念，07 是下钻；前移会连锁重编号 04-07（约 160 处引用），成本大于收益 |
| R-04 | 整体重编号（按请求族重新分组） | 现行顺序已无前向引用，缺陷在内容；765 处 stage 内互引与约 90 处代码注释引用将全部作废 |
| R-05 | 删除旧文档 | 项目规则：B 相旧文档整体归档、不删除 |

### 6.4 归档与明确删除项

- **归档**：B 相完成后，旧 `00-31 + 99` 共 33 篇整体移入 `archive/`（保留不删）；`draft/` 与 `archive/todo-R1-archive-2026-09-09.md` 维持现状。
- **明确删除的表述（非知识点）**：① `.design/` 引用——31 篇各 1 处，按 Hidden Folder Convention 必须清除；② "（工具生成）"占位锚——全 stage 实测 547 处，全部重锚或删除；③ 各篇互抄的测试基线数字段——改为只报本篇模块快照 + 日期；④ 旧 31 的"杂项"框架叙事与旧 09 的 TRNS 错误方向描述。

---

## 7. 缺漏新篇

按第三部分固定清单逐项落实（无空缺、无"待定"）：

| # | 主题 | 为什么重要 | 原料在哪里 | 归哪一篇 | 验收标准 |
|---|------|-----------|-----------|----------|----------|
| N-01 | 链接与加载（VFS 的服务装载） | 读者需知道 VFS 不是自举，`main()` 之前发生了什么 | `rs/table.c:16`、`kernel/table.c:57`；01-stage-kernel/06、03-stage-rs | 01（一段 + 交叉引用） | 能说出"谁、用什么、从哪加载 VFS"，且不重述内核加载器细节 |
| N-02 | 镜像与内存布局 | 表容量与线程栈是 VFS 的内存事实 | `const.h`、`worker.c:15-20`、`fproc.rs:FProcTable` | 08（栈预算/消除）＋02（表堆化）＋99（容量） | 线程栈三档与表规模有锚；Rust 的存储选择有理由 |
| N-03 | 汇编入口与陷阱进入 | 需明确声明不在本 stage，防止读者误找 | — | 范围外声明（01 头部 + 00 §范围） | 00/01 明示"归内核与运行时 stage"，并给正式文档路径 |
| N-04 | 启动装配 | 启动顺序的制品与登记事实 | `main.c:374-499`、`rs/table.c`、`Makefile` | 01（装配）＋99（制品） | 十一项初始化链与 boot image 登记可逐条核对 |
| N-05 | 构建与工具链 | 源文件清单、链接库、条件编译是"可重建"的前提 | `servers/vfs/Makefile`、`os/servers/vfs/Cargo.toml` | 99 §工程（K-561）；01 引用 | Makefile 的 SRCS/LDADD/`MKCOVERAGE` 与 Cargo 依赖逐个列出 |
| N-06 | 跨模块接口与线格式 | 项目已两次为 wire 绝对值付学费（FS_BASE、RS 前缀） | `com.h`、`vfsif.h`、`callnr.h`、`minix-types` | 99 §wire（K-552）＋10/11/12/19/20/21/22/32 | 八族总表每行带 C 绝对锚与判别宏；VMCALL 六域单列 |
| N-07 | 错误路径 | 项目规则"错误必须映射 errno"，现状 30 个枚举分散 | 各篇 §3/§4、`minix-types/types/errno.rs` | 99 §错误码纪律（K-560）＋各篇 | 机械检查命令可复核（`impl ToErrno` 计数 ≥ 枚举数） |
| N-08 | 关闭与退出 | 资源回收与故障级联是正确性高危区 | `misc.c:639-708`、`mount.c:552-585`、`filedes.c:250-308`、`sdev.c:912` | 10（进程）＋18（卸载）＋14/19-22（驱动）＋22 §总谱 | 回收顺序表与级联图可核对；VFS 自身退出语义明示 |
| N-09 | 并发与同步 | 单线程改写的前提是把并发事实讲清 | `tll.c`、`worker.c`、`fproc.h:71`、`file.h:14-18` | 07、08、02/04、99 | 三级锁五路、槽状态机、锁归属三处无矛盾 |
| N-10 | 测试基建 | 需诚实声明"有单测、无端到端" | `os/servers/vfs/src`（365 个 `#[test]` 实测）、`os/qemu-tests`（无 VFS 用例） | 00 §验证（K-562）＋各篇 §5 | 测试数带日期；W1-W9 只作 roadmap 引用；无虚构测试名 |
| N-11 | 调用统计编译开关 | 默认关闭的埋点易被误读为死代码 | `main.c:32-34,286-289` | 33（K-541） | 说明默认关闭、无表、cfg 语义 |
| N-12 | fproc/dmap 快照生产面 | A-7 曾被标缺口，现已有生产实现，必须翻转 | `minix-types/types/fproc.rs`、`fproc.rs:to_fproc_snap` | 03（K-067）、33（K-536） | 字段映射与 52 字节布局有锚；MIB 消费端只作交叉引用 |
| N-13 | sdev Rust 宿主迁移 | 旧文指针全错，读者会找不到实现 | `os/libs/minix-sockdriver/src/sdev.rs`、`os/servers/vfs/src/sdev.rs` | 22（K-390）、24 | 两处宿主职责分别说明，行号不写入正文 |
| N-14 | 定时器库边界 | `expire_timers` 实现不在 VFS，归属易误 | `main.c:112`、`libsys/timers.c:97` | 23（K-402） | 入口与实现分离标注，库名与函数名给出 |
| N-15 | REQ 契约收敛结果 | 旧文按本地定义叙述，与现状不符 | `minix-types/src/ipc/fs_driver.rs` | 12（K-228）、99 | 单一权威路径与绝对值 pin 测试位置给出 |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表

**范围说明**：编号不变的 32 篇（00-30 + 99）在原编号原地重建，其"逐节 → 新节"关系即 §5 契约的"讲什么/不讲什么"；其中发生**节级搬移**的按下表迁移。旧 31 的全节迁移见 §6.2（即其锚点迁移表）。所有"迁移类型"按原样搬移/改写/合并/拆分/删除五值标注。

| 旧位置（文档与小节） | 旧内容（一句话） | 新位置（新篇章与小节） | 迁移类型 | 备注（断链风险） |
|---------------------|------------------|------------------------|----------|------------------|
| 02 §2.6 | 两遍初始化 + fork 整体复制概述 | 03 §2.6（表级）；10 §fork | 拆分 | 02/03 双引：B 相 02 只留"字段在第二遍被清"一句 |
| 02 §2.7 | `fproc_light` 结构 | 03 §2.7 | 原样搬移 | 02/03/10/11 四处重复，主家 03 |
| 03 §2.5 | `who_p`/`fproc_addr` 使用示例 | 09 §2.1（使用）；03 只留定义 | 拆分 | 09 引用 03（反向，合法） |
| 04 §2.3 | `get_fd` 双表扫描 | 14 §2.2 | 原样搬移 | 04/14 重复，主家 14 |
| 04 §2.4 | `get_filp` 特权（保留） | 04（不变） | 改写 | — |
| 05 §1.3/§1.4 | vnode 锁状态与升级教学 | 07（机制）；05 只留"锁定即不可分配" | 拆分 | 05 前置不列 07（§9.1 检查） |
| 06 §1.3 | vmnt 锁的 EXCL/WRITE 教学细则 | 07（原语）；06 只留映射 | 拆分 | 同上 |
| 06 §1.5/§2.7 | `fetch_vmnt_paths` 全流程 | 99 省略台账（一行判定） | 压缩 | C 零调用（`vmnt.c:246`），旧文展开属介绍死代码 |
| 07 §1.3 | EBUSY 排队与 worker 等待的边界 | 08 §wait/signal | 改写 | 07 只留锁侧返回语义 |
| 08 §1.1 | 四条阻塞恢复路径清单 | 17/23/21/22 各一篇 | 拆分 | 08 只留槽状态 |
| 09 §2.10 | TRNS 编解码 | 11 §1.3/§2.11 | 原样搬移（纠错） | 旧文方向写反 + `0x1000` 错值必须消灭 |
| 10 §2.5 | `close_fd`/`put_vnode` 单步展开 | 14 §2.8 / 05 §2.6 | 拆分 | 10 只留回收顺序表 |
| 11 §2.11 | `node_details`/`lookup_res` 结构 | 12 §2.2 | 原样搬移 | 11/12 重复，主家 12 |
| 11 D6 | `fproc_light` 缺口段 | 03 / 33 | 压缩 | 与 10 D6 逐字重复 |
| 12 §1.3/§2.5 | `lookup_res` 消费与 `EENTERMOUNT` | 13 §1.2/§2.6 | 搬移 | 12/13 双向重复（旧文互引），主家 13 |
| 13 §1.4 | tll 三级锁机制与升级细节 | 07；13 只留解析期锁协议 | 压缩 | 13 前置含 07，迁移合法 |
| 14 §2.6 | `lock_filp` 的 FIFO/升级细则 | 04 §2.7（锁字段与借用） | 搬移 | 14 只留调用点 |
| 14 §2.7 | `close_filp` 的 S_ISCHR/BLK/SOCK 分支 | 20/21/22 各族篇；14 留分流表 | 拆分 | 三族篇各引用 14 |
| 15 §2.3/§2.4 | BLK/FIFO 打开分支细节 | 19/20、17 | 拆分 | 15 只留分派行 |
| 15 §2.8 | `close_filp` 全流程 | 14 §2.8 | 重复删除 | 15/14 重复 |
| 16 §2.7 | `rw_pipe` 定量与续写 | 17 §2.3 | 原样搬移 | 16 只留 FIFO 分派 |
| 17 §2.5 | select 位清与 `select_ack` | 23 §2.12 | 搬移 | 17 只留 `select_callback` 调用点 |
| 18 §2.7 | `mount_pfs` 与 01 重复 | 18（主家）；01 只留调用点 | 重复删除 | 01/18 双述 |
| 19 §2.11/§2.12 | ioctl 在 bdev/cdev/sdev 的再述 | 19（主家）；20/21/22 引用 | 压缩 | 授权方向单主家 |
| 20 §2.5 | filp/vmnt 表扫描操作细节 | 04/06；20 只留四元谓词 | 压缩 | — |
| 21 §2.3 | `req_newnode`/vnode 引用细节 | 12/05 | 压缩 | 21 只留调用点 |
| 21 §2.4 | CTTY 授终端细节 | 02（`fp_tty` 归属） | 搬移 | — |
| 22 §2.9/§2.13 | `sdev_select`/accept 续作执行 | 23 / 24 | 搬移 | 22 只留分类 |
| 23 §2.8/§2.10 | 驱动投递执行、`copy_fdsets` 执行 | 21/22、01-stage-kernel/18 | 压缩 | 23 只留调度与取整规则 |
| 24 §2.4/§2.10 | PFS 分配与 fd 预检执行 | 12 / 14 | 压缩 | 24 只留规格 |
| 25 §2.3/§2.4 | 权限/凭证规则、VM mmap 执行 | 29/02、02-stage-vm/20 | 压缩 | 25 只留检查点 |
| 26 §2.10 | `pm_dumpcore` 全流程 | 10 §2.7 | 搬移 | 26 只留转储格式 |
| 27 §2.9 | `rdlink_direct` 的内部使用 | 13（内部读入口） | 搬移 | 27 只留 `do_rdlink` |
| 28 §2.10 | 挂载表加锁与锁后复验细节 | 06；28 只留遍历规则 | 压缩 | — |
| 30 §2.12 | `close_fd` 全函数 | 14 §2.8 | 搬移 | 30 只留清锁扫描 |
| 旧 31 全节 | 八类杂项 | 31/32/33 | 拆分 | 见 §6.2（无删除项） |
| 全 stage 各篇头部/§3 | `.design/` 引用（31 处） | 删除 | 删除 | Hidden Folder Convention；B 相机械清扫归零 |
| 全 stage 各篇 §3/§4/§5 | "（工具生成）"占位锚（547 处） | 重锚或删除 | 改写 | 工具产物污染，禁止进入正文 |

### 8.2 引用迁移表

| 旧引用 | 出现位置（实测） | 新目标 | 验证方式 |
|--------|------------------|--------|----------|
| `31-misc-queries.md`（杂项/下一站语境） | `30-fcntl-lock.md:255,258,265`（3 处） | 按语境分别指向 `31-sync-and-time.md`、`32-vm-call.md`、`33-system-info.md` | `rg -n '31-misc-queries' 30-fcntl-lock.md` 归零 |
| `31-misc-queries.md §3` | `os/servers/vfs/src/misc.rs:8`（1 处） | 三篇的 Rust 决策节（或改为 `31/32/33` 联合注记） | `rg -n '31-misc-queries' os/` 归零 |
| `31-misc-queries.md` 台账行 | `plan.md:124,417`；（参考材料） | 拆分/合并为三行 | B 相随 plan 更新 |
| `31-misc-queries.md` 修复记录 | `todo.md:369,447`；（参考材料） | 按函数改指 31/33 | B 相随 todo 更新 |
| `05-stage-vfs/`（目录引用） | `notes/` 117 处、`os/` 4 处 | 目录名不变，无需迁移；其中指向具体文档的引用已逐条核对，**没有任何跨 stage 引用指向旧 31** | `rg -n '05-stage-vfs/31-' notes/ os/` 归零 |
| `NN-*.md` 编号互引 | stage 内 765 处（实测按名字匹配计数） | 编号未变者零改动；B 相从契约重生成引用文本 | `rg -no '[0-9]{2}-[a-z-]+\.md' <stage> | sort | uniq -c` 与蓝图对表 |
| `05-*.md` 等 Rust 代码注释引用 | `os/servers/vfs/src` 约 90 处（逐文件实测） | 编号未变者不动；仅 `misc.rs:8` 一处需改 | `rg -n '31-misc-queries' os/servers/vfs/src` 归零 |
| `VFS_PROC_NR`/`MIB_PROC_NR` 等跨 stage 常量的文档引用 | 其它 stage 文档 | 与 VFS 文档编号无关（常量引用） | 不改 |

### 8.3 断链成本摘要

- **编号变化导致的硬性引用修改：8 处**（3 处文档正文 + 1 处代码注释 + 4 处参考材料台账）。这是"只拆旧 31、不动其它编号"方案的直接收益。
- **节级搬移涉及的旧节：约 35 个**（§8.1 表行数）。这些不需要"找链接改链接"，只需 B 相按契约从知识点池取料重写；成本计入写作，不产生断链。
- **卫生清理规模**：`.design/` 引用 31 处；"（工具生成）"占位锚 **547 处**（实测逐文件计数，最密集：21-cdev 45、22-sdev 38、19-device-map 36、06-vmnt 32、17-pipe 31）；互抄测试基线段 30 余处。全部在本阶段内清零。
- **热点文件**：`30-fcntl-lock.md`（唯一同时引用旧 31 与多个后续篇的文档）；`os/servers/vfs/src/misc.rs`（唯一指向旧 31 的代码注释）；`00-vfs-overview.md`（45 处编号引用，导航性质，B 相必须整体重生成）。
- **建议的批量修改方式**：① 不为旧编号做全局替换（避免误伤）；② B 相每篇写完后跑 `rg -n '31-misc-queries|\.design/|工具生成' <stage>` 归零检查；③ 代码注释只做 `misc.rs:8` 一处，跑 `rg -n '31-misc-queries' os/` 归零；④ stage 内互引以契约中的新编号为准，逐篇对账（Gate E 的测试名对账同批做）。

---

## 9. 验证与自检门

### 9.1 四种机械检查

**检查一：前向引用扫描（逐篇"前置"字段）**

方法：抽出 §5 全部 35 个契约的"前置"字段，检查每个编号小于本篇编号。结果：全部通过。逐篇前置如下（编号即通过条件）：

| 新篇 | 前置 | 新篇 | 前置 |
|------|------|------|------|
| 00 | 无 | 18 | 01、06、12、13 |
| 01 | 00 | 19 | 01、04、06、09 |
| 02 | 01 | 20 | 11、16、19 |
| 03 | 01、02 | 21 | 02、14、15、19 |
| 04 | 01、03 | 22 | 02、14、19、21 |
| 05 | 01、04 | 23 | 02、04、09、17、21、22 |
| 06 | 01、05 | 24 | 14、19、22、23 |
| 07 | 无 | 25 | 10、13、14、15 |
| 08 | 02、03、07 | 26 | 10、15、16、25 |
| 09 | 01、02、03、08 | 27 | 13、14、15 |
| 10 | 01、02、03、08、09 | 28 | 02、06、12、13、14 |
| 11 | 06、08、09 | 29 | 02、06、10、13、14 |
| 12 | 04、05、06、11 | 30 | 02、04、09、14 |
| 13 | 05、06、07、11、12 | 31 | 06、12、13、14、29 |
| 14 | 02、04、05、07 | 32 | 04、05、14 |
| 15 | 04、13、14 | 33 | 03、06、09、19 |
| 16 | 04、14、15 | 99 | 00 |
| 17 | 04、08、09、16 | | |

两处"延伸指针"（非前置）已显式声明：05/06 的锁细节延伸读 07；18 的 dmap 展开延伸读 19。契约文字明确它们"不作为读懂本篇的前提"。

**检查二：依赖关系图检查**

方法：以前置字段为有向边构图（约 70 条边）。因为每条边都从小号指向大号，图是无环 DAG，无需拆环方案。跨阶段依赖（01→00-stage 内、25→02-stage-vm/20、26→04-stage-pm/11-13 等）不是本 stage 图内的边，已各自标注对端文档。

**检查三：覆盖率检查**

方法：逐条检查知识点池 343 条是否在 §5 契约中有"讲什么"归属。结果：**343/343 有归属**（00 8、01 18、02 14、03 8、04 10、05 9、06 10、07 8、08 13、09 15、10 13、11 8、12 9、13 11、14 8、15 9、16 9、17 11、18 12、19 12、20 7、21 9、22 11、23 10、24 8、25 10、26 7、27 7、28 8、29 9、30 10、31 5、32 6、33 7、99 14）。**明确删除项：无**。13 条新增知识点全部有证据锚（C 源码、制品路径或跨阶段正式文档）。

**检查四：断链成本统计**

见 §8.3：硬性引用修改 8 处；卫生清理 31+547 处；热点文件与批量方式已列出。

### 9.2 自检门逐门结果

| 门 | 结果 | 证据与说明 |
|----|------|-----------|
| G1 C 真序逐条可核对 | 通过 | 随机抽十条当场核对：S06 `main.c:415-436`（握手循环）、S10 `main.c:445`→`worker.c:27`、S13 `main.c:455-465`、S15 `main.c:486-489`、S16 `main.c:492-493`、L05 `main.c:80-90`、L09 `main.c:126-134`、L12 `worker.c:239-280`、F02 `comm.c:134-168`、F05 `vfsif.h:80`。其余条目来自 `main.c` 全文与函数清单 grep，全部带锚 |
| G2 知识点池完整 | 通过 | 33 个 `.c` 全部有归属（§3.2）；15 个本地 `.h` 与 4 个外部头文件有归属；非 C 制品十项逐项在 §3.6 作答；死代码/死钩子/调试设施单列 99 省略台账；无"无家可归"的 C 文件或制品 |
| G3 前向引用为零 | 通过 | §9.1 检查一；两处延伸指针已在契约中声明为非前置 |
| G4 依赖图无环 | 通过 | §9.1 检查二；所有边指向更小编号 |
| G5 覆盖率 100% | 通过 | §9.1 检查三；343/343 有去向；新增 13 条全部带锚；删除项为空 |
| G6 拆分/合并/新建双方向核对（抽查十处） | 通过 | 抽查：OP-01/02/03（旧 31 拆分，§6.2 逐节去向 + 新知识锚）、OP-04（02→03 搬移）、OP-08（04→14）、OP-10（死代码压缩）、OP-13（09→11）、OP-16（12→13）、OP-18（14→20/21/22）、OP-33（死亡总谱的新来源）、OP-37（快照生产面新来源）。每处都写清旧知识去向或新知识证据 |
| G7 每篇契约要素齐全 | 通过 | 35 个契约全部含：一句话定位、讲什么（K 编号）、不讲什么（带去向）、前置、后置、事实底线（C 与非 C 带锚）、知识点清单（编号/名称/类型/锚点/为什么归本篇/来源）、验收标准 |
| G8 锚点迁移与引用迁移覆盖 | 通过 | §8.1 覆盖旧 31 全部节（§6.2）与 35 个节级搬移；§8.2 覆盖文档正文、代码注释、参考材料与目录级引用；实测计数：stage 内 765 处互引、`os/` 约 90 处代码引用、跨 stage 目录引用 117 处 |
| G9 事实断言锚点与推测标注 | 通过 | 抽查十条：`NR_MNTS=16`（`const.h:7`）、`OPEN_MAX=255`（`syslimits.h`）、`FS_BASE=0xA00`（`com.h:589`）、`VFS_TRANSID=0xB01`（`com.h:911`）、`CDEV_RS_BASE=0x480`（`com.h:919`）、`BDEV_RS_BASE=0x580`（`com.h:963`）、`SDEV_RS_BASE=0x1980`（`com.h:1038`）、`VM_VFS_REPLY=0xC1E`（`com.h:707`）、`PIPE_BUF`（`syslimits.h`）、`_NR_PROCS=256`（`fproc.h:10` 注释与内核同界）。无未标注的推测；两处需在 B 相写入日重新实测的易变事实已声明（Rust 测试数、Rust 符号存在性——用符号锚 + 快照日期规避） |

### 9.3 结论与待用户裁决的问题

**结论**：本蓝图完成（343 条知识点全有去向，35 篇契约齐全，门 G1-G9 全通过）。重建方案为：保留 00-31 + 99 主线编号，仅拆分旧 31 为 31/32/33，并对 35 处节级边界与 547 处占位锚做重建级清理。B 相可直接按 §5 契约取料写作。

**待用户裁决的问题**（按影响排序）：

1. **旧 31 拆分方案**（新 31/32/33）：是否同意？若选择不拆，则 32/33 的内容必须回并，且旧 31 的单篇单语义缺陷与"VM 调用协议无独立家"的问题将保留。
2. **22 篇增设《死亡级联总谱》节**：是否同意把 19 触发、14/20/21/22/23 分族的死亡处理在 22 末尾统一收束？若不同意，替代方案是 99 里保留一张总表（但 99 的定位是横向索引，不适合承载顺序性机制）。
3. **02/03 不合并的裁决**：本蓝图建议保持两篇、用 OP-04/05 消重；若评审倾向合并，需要连锁重编号 03-33，断链成本将从 8 处升到约 1000 处。
4. **Rust 章节的锚点纪律**：是否确认"符号锚 + 快照日期 + 禁行号断言"作为 B 相强制规范？这是本次探查发现的最大规模质量问题（547 处占位锚 + 大面积行号漂移）的根治手段。
5. **sdev 实现宿主的归属裁决**：`minix-sockdriver` 是 sdev 协议的正确宿主（E-SDEVOWN 待 17-stage 裁决）还是应回迁 VFS；本蓝图按现状（宿主在 `minix-sockdriver`）写契约，若回迁则 22/24 的 Rust 小节需改写。

**未完成声明**：无。若共识蓝图采纳不同的结构决策（如问题 1/2/3 的替代方案），本报告的第 2/4/5/6/8/9 节需要相应重算，重算范围已在各节标注依赖关系。

