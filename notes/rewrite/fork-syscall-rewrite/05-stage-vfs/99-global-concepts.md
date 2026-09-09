# 99-global-concepts: VFS 全局概念

> **状态**: 已按 99-outline.v1 契约改写（2026-09-09，R2-P2-2；快照见 `.design/99-*.v1.md`）；有意省略表已先行落地（Fix #28）
> **定位**: 全局概念（阶段收尾）
> **源码**: `const.h`、`glo.h`、`type.h`、`fs.h`、`proto.h`、`utility.c:142-186`、`minix3/sys/sys/syslimits.h`
> **Rust 模块**: `minix-types`、`os/servers/vfs/src/call_table.rs` 常量、各表文件的容量常量
> **draft 素材**: `draft/09-globals-const.md` + `draft/99-global-concepts.md`

## 1 概念：常量不是数字，是资源上限与协议边界

### 1.1 容量常量族——每个数字都是一次资源分配决策

| 常量 | 值 | C 锚点 | 为什么是这个值 |
|------|-----|--------|---------------|
| `NR_FILPS` | 1024 | const.h:5 | filp 全局共享池：系统内同时打开的文件描述（跨进程共享）上限。1024 = 每进程 256 fd × 典型共享率的安全余量 |
| `NR_VNODES` | 1024 | const.h:8 | vnode 池：系统内活跃 inode 缓存上限，与 filp 同量级但独立计数（一个 vnode 可被多个 filp 引用） |
| `NR_MNTS` | 16 | const.h:7 | 挂载表槽位。**R2-P1-5 勘误**：vmnt.rs 曾误为 8（容量减半），Fix #33 归一为 16 |
| `NR_WTHREADS` | 9 | const.h:9 | worker 并发上限：同时挂起的 FS/驱动对话数。C 是 9 条真线程；Rust 是 9 个请求槽（ARCH A-1），数字保留是为了语义对齐而非技术必需 |
| `NR_LOCKS` | 8 | const.h:6 | POSIX 记录锁表槽位 |
| `NR_SOCKDEVS` | 8 | const.h:10 | socket 驱动表（smap）行数 |
| `NR_NONEDEVS` | `= NR_MNTS` | const.h:12 | 伪设备位图宽度——PFS 这类"无真实设备"的挂载从此分配 |
| `OPEN_MAX` | 255 | syslimits.h:38 | 每进程 fd 上限：fd 0..254，255 本身不可用。Rust `fproc.rs:41` 同值；`Fd(u8)` 的新类型边界即此 |
| `NGROUPS_MAX` | 16 | syslimits.h:59 | 补充组数上限；`fproc.rs` 的 `supplemental_groups: [Gid; 16]` 定长数组由此 |

这些常量的 Rust 归属遵循"归属即依赖方向"：协议常量（errno、endpoint、消息布局）入 `minix-types`；VFS 私有容量（`NR_FILPS` 等）入各表文件；跨端复用走 re-export（如 stadir 的 `pub use crate::vmnt::NR_MNTS`，Fix #33 消灭了 8/16 双值分叉）。

### 1.2 阻塞原因枚举——"进程在等谁"的类型化

C 用 `fp_blocked_on` 整数 + `fp_u` 联合体（fproc.h:30-61）表达进程挂在什么上：`FP_BLOCKED_ON_NONE/PIPE/POPEN/FLOCK/SELECT/CDEV/SDEV`。Rust 以 `BlockedOn` 标签枚举承载（fproc.rs:94）——判别器与载荷绑定，读 pipe 参数时编译期不可能拿到 socket 参数（ARCH A-3）。驱动死亡级联（`unsuspend_by_endpt`）正是按这个枚举分流：CDEV → 复活回 EIO，SDEV → `sdev_stop`。

### 1.3 协议边界常量——四个互不重叠的名字空间

`m_type` 域内四段前缀互不重叠：`VFS_BASE 0x100`（call_vec 系统调用，callnr.h）、`FS_BASE 0xA00`（VFS→FS 的 REQ_*，com.h:589——**绝对值是 wire 契约**，R2-P0-1 的 0x600 勘误即此）、`VFS_PM_RQ_BASE 0x900`（PM 控制面，com.h:512）、`VFS_TRANSACTION_BASE 0xB00`（transid 高位编码，com.h:909-911）。判别宏都是"`& ~掩码` == 基址"形态：`IS_FS_RQ` 用 `~0xff`，设备 RS 三族用 `~0x7f`（com.h:919/:963/:1038，基址 0x480/0x580/0x1980）。

## 2 全局状态：glo.h 的每个变量谁写谁读

C 的 `glo.h` 散装全局在 Rust 按"归属即依赖"拆进 `VfsState`（ARCH A-4）：`fp`（当前进程上下文）→ `current_fp_slot`；`reviving`（复活计数）→ `VfsState.reviving`；`sending`（排队等待数）→ `GlobalComm.sending`；`workers` → `WorkerPool`；`verbose` → 启动参数；`err_code` → 决策函数的 `Result` 错误值；`bsf_lock`（阻塞系统调用自旋锁）→ 槽状态机取代。`m_in`（当前消息）→ `current_message`。

## 3 引用计数双层不变量——失效族的正确性基础

三个计数各管一层：`filp_count`（file.h:5，>0 即占用）管 filp 槽的生死；`v_ref_count`（vnode.h:13）管 vnode 内存引用；`v_fs_count`（vnode.h:14）管 FS 侧 inode 引用。不变量：`filp_count` 是 `v_ref_count` 的贡献者之一，`v_fs_count` 只有在 `v_ref_count` 归零后才按阈值释放。失效族（驱动死亡→invalidate→close）的每一步都由这三个计数守卫——改错一层即泄漏或悬垂（首轮 C-3/P0-3 与 Fix #22 的 `fetch_vmnt_paths` 判定都依赖此口径）。

## 4 术语与跨地址空间拷贝

`endpoint`（进程身份，`minix_types::Endpoint`）与 `transid`（`VFS_TRANSID 0xB01 + slot`，`fs_comm.rs:30-69` 的 `TransId`）是两条消息定位机制：endpoint 找进程，transid 在高 16 位找 worker 槽。`who_p`/`who_e`/`call_nr` 三个 C 宏分别是槽号/端点/调用号的当前上下文读取。`sys_datacopy_wrapper`（utility.c:142-186）是 VFS 代理的跨地址空间拷贝：PM 发来的组列表（misc.c:752）、exec 的路径（exec.c）都经它落地；Rust 侧决策口是 `PmHandler::fetch_group_list`（fail-closed ENOSYS 待 W1）。

类型映射（A-8）：`dev_t → DevId(u64)`、`mode_t → Mode(u32)`、`uid_t/gid_t → Uid/Gid(u32)`、`vir_bytes → VirBytes`；`LOCK_DEBUG` 调试 cfg（A-9）未移植（其断言对象——真锁——已被借用模型取代，见有意省略表）。

## 5 测试要点

不变量类测试的落点：引用计数配平在 `filp.rs`/`vnode.rs` 的 inc/dec 测试；容量 fail-closed 在各表边界测试；`m_type` 前缀互斥在 `call_table.rs:440`（`VFS_BASE` vs `TRANSACTION_BASE`）与 `request.rs` 的 `test_fs_wire_values_match_c_absolute`。

## 6 过渡与 7 参见

本篇是横向索引：02~07 的表结构、10 的 PM 协议术语、12 的 REQ 常量都以本篇为术语基准。详见各篇；有意省略的 C 符号见下方台账。

## 有意省略表（intentional omissions）

单线程事件循环（ARCH A-1）消灭 mthread 后，一批 C 函数的存在前提消失。本表集中登记"有意不移植"的 C 符号——C 锚点、删除理由与 ARCH 依据，防止后续轮次把它们误报为缺口（首轮 C-10/P3-2 的落地；第二轮 Fix #22/Fix #23 各追加一笔）。新增省略项必须走本表：函数名 + C 锚点 + 为何在 minix-rs 中无存在前提。

### 进程锁族（mthread 互斥的替代物消失）

| C 符号 | C 锚点 | 删除理由 |
|--------|--------|---------|
| `lock_proc` / `unlock_proc` | main.c:528-547 | C 用 mutex_trylock + worker_suspend 保护"可睡眠的进程锁"；单线程事件循环下同一时刻只有一个请求在被处理，无并发对手（ARCH A-1 推论，首轮 C-10 判定） |
| `thread_cleanup` | main.c:557 一带 | 同上——mthread 栈清理随线程消失 |

### 死锁断言族（LOCK_DEBUG 调试设施）

| C 符号 | C 锚点 | 删除理由 |
|--------|--------|---------|
| `check_filp_locks(_by_me)` | filedes.c:26-71 | 多线程死锁调试断言；借用模型（`locked_by`/`soft_locked`）使非法状态不可表达，断言无对象 |
| `check_vnode_locks(_by_me)` | vnode.c:43-83 | 同上 |
| `check_vmnt_locks(_by_me)` | vmnt.c:24-62 | 同上 |
| `unlock_filps` | filedes.c:383 一带 | 批量解锁随软锁模型收编进 `dec_count`/槽释放路径 |

### select 清理族（部分建模，部分省略）

| C 符号 | C 锚点 | 处置 |
|--------|--------|---------|
| `select_forget` | select.c:833 | 有意省略——进程退出时 select 台账的批量遗忘由 `fproc` 槽释放级联承担（free_proc 的 close_fd×256 自然清账） |
| `wipe_select` | select.c:78/127 | 已建模——`reply1_step`/`reply2` 的 ops 清零规则即其语义 |
| `select_timeout_check` | select.c:79/335 | 已建模——`plan_timeout` 三态计划承接到期语义 |
| `select_dump` | select.c（调试转储） | 有意省略——纯调试转储，无行为契约 |

### 第二轮追加判定

| C 符号 | C 锚点 | 处置与理由 |
|--------|--------|-----------|
| `fetch_vmnt_paths` | vmnt.c:246-288 | **C 死代码**：定义 + `proto.h:371` 悬空声明、全树零调用；行为真相是 `fill_statvfs` 直拷 `m_mount_path`（stadir.c:284）。移植死函数即 translate 死代码（Fix #22 判定反转） |
| `panic_hook` | misc.c:989-993 | ARCH A-1 消灭 mthread 后"打印 mthread 栈"无对象（Fix #23 判定） |
| `worker_cleanup`/`worker_init` 的 LU 调用点 | main.c:314/332/352 | 槽位是数据不是线程——清理/重建工人按构造为空操作；谓词 `lu_rollback_needs_workers`/`init_lu_needs_workers` 记录 C 分支（Fix #26） |

### 未省略、仅待接线的边界提醒

以下**不是**省略项，只是执行半挂接线矩阵（P1-2/edge E1）：`dmap_endpt_up`/`smap_endpt_up` 的恢复驱动（`recover_step` 已备）、`free_proc` 重启轮（`REBOOT_SEQUENCE` 已备）、`req_readsuper` 确认往返（`PfsMountPlan` 已备）。判别标准：决策函数已存在、只缺消息回路的，归 P1-2；连决策前提都消失的，才入本表。
