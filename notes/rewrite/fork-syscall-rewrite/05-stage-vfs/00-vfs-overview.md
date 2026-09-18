# 00-vfs-overview: VFS 整体架构概览

> **状态**: 已按 00-outline.v1 契约改写（2026-09-09，R2-P2-2；快照见 `.design/00-*.v1.md`）
> **定位**: 全文档导航（阶段 0 总览）
> **源码**: `minix3/minix/servers/vfs/`（33 个 .c + 15 个 .h，16,735 行）
> **Rust 模块**: `os/servers/vfs/src/`（33 文件，约 23,000 行）
> **draft 素材**: `draft/00-vfs-overview.md` + `draft/99-global-concepts.md`

## 1 概念：VFS 是什么，为什么它是这个形状

### 1.1 一个用户态服务器，管所有"文件"这个名字

VFS（虚拟文件系统服务器）是 Minix3 微内核里的用户态进程：内核不管文件，所有的 `open`/`read`/`stat` 语义都由它裁决。它站在三方中间——POSIX 进程递上来系统调用，文件系统驱动（mfs 等）提供真正的磁盘语义，设备驱动（tty、块盘、socket 驱动）提供设备语义。VFS 是这三方唯一共享的词汇表：路径名翻译成 vnode，文件描述符翻译成 filp，挂载点翻译成 vmnt。

Minix3 原版的 VFS 是全系统唯一使用线程池（mthread，9 条）的服务器：一个阻塞的文件系统调用不能拖死整个服务器，所以每个可能阻塞的调用被丢进一个工人线程里等。minix-rs 把这九条线程改写成一个单线程事件循环加九个请求槽（ARCH A-1）——线程的"隔离阻塞副作用"能力由槽的状态机（Idle → Busy → WaitingForFs / Suspended）承接，而线程的调度成本、栈管理、锁竞争全部消失。这不是省事：Redox 的文件系统同样是"单 daemon 承载全部文件系统状态"（其官方路线图亦承认单 daemon 是吞吐瓶颈），单事件循环 + 挂起复活达成的正是同一外部性质。

### 1.2 启动主线：从被加载到接第一个请求

VFS 的一生按这条线走（每一站对应一篇机制文档）：

```
RS 加载 VFS 镜像
  └─ main() (main.c:54)
       └─ sef_local_startup() (main.c:374)      注册 5 个生命周期回调
       └─ sef_cb_init_fresh() (main.c:393)      ← 01-vfs-init-main
            ├─ fproc 清零 + VFS_PM_INIT 握手 (main.c:410-436)   ← 10-pm-protocol
            ├─ init_dmap / init_smap / rproctab 映射 (main.c:445-467)  ← 19-device-map
            ├─ init_vnodes / init_vmnts / init_select / init_filps
            └─ worker_start(do_init_root) (main.c:501-527)       ← 18-mount
                 ├─ mount_pfs()      管道文件系统罐装挂载
                 └─ mount_fs(bootramdisk → "/")  根文件系统
       └─ 主循环 (main.c:80-138)                                ← 09-main-loop
            get_work → reviving 优先 → 八级分发
            (FS reply → PM → notify → task → 块/字符/套接字 → syscall)
```

fork 是次主线：PM 在进程创建时发 `VFS_PM_FORK`，VFS 复制 fproc 表并递增全部共享 filp 的引用计数（`10-pm-protocol`）。

### 1.3 服务面：三个数字

64 个 VFS 系统调用（`table.c` 的 `call_vec`，`VFS_BASE 0x100` 起）、12 个 VFS_PM 控制请求（`com.h:520-531`，`VFS_PM_RQ_BASE 0x900`）、33 个活 `REQ_*` 文件系统请求（`minix3/minix/include/minix/vfsif.h:REQ_GETNODE`，`FS_BASE 0xA00`——绝对值是 wire 契约，见 12 号的历史教训）。三个数字各自的单一事实源分别是 `call_table.rs`、`minix-types/src/ipc/vfs.rs`、`request.rs`。

### 1.4 设计原则（全阶段文档共守）

- **位置可回答性**：任何机制问题都有一个确定的篇章可以回答，不靠全库搜索
- **禁止前向引用**：阅读路径只向后依赖，机制细节在所属篇章，调用点只留锚
- **每篇一个语义单元**：一篇讲透一个机制，不做"杂物章"
- **ARCH 三处一致**：架构级改写必须同时标注在文档正文、design 快照、代码注释三处

## 2 C 源码分析：33 个文件的分组地图

- **启动与生命周期**：`main.c`（启动链 + 主循环 + worker 门控）、`worker.c`（9 mthread 池）—— 01/08
- **三张核心表**：`fproc.h` 进程表、`file.h/filedes.c` fd 与 filp、`vnode.h/vnode.c`、`vmnt.h/vmnt.c` —— 02/04/05/06
- **锁**：`tll.h/tll.c` 三级锁（None/Read/Write + 串行读）—— 07
- **协议**：`comm.c`（FS 通信窗口）、`request.c`（REQ_* 包装）、`table.c`（调用表）—— 11/12/09
- **路径**：`path.c`（解析循环 + 跨挂载）—— 13
- **系统调用族**：`open.c`、`read.c`/`write.c`、`link.c`、`stadir.c`、`protect.c`、`fcntl.c`/`lock.c`、`misc.c`、`exec.c`、`pipe.c`、`select.c`、`socket.c`、`coredump.c` —— 15/16/27/28/29/30/31/25/17/23/24/26
- **设备层**：`dmap.c`、`smap.c`、`device.c`、`bdev.c`、`cdev.c`、`sdev.c` —— 19/20/21/22
- **挂载**：`mount.c` —— 18
- **横向**：`const.h`/`glo.h`/`type.h`/`fs.h`/`proto.h` —— 99

## 3 Rust 设计决策：架构级 ARCH 导航

架构级改写清单的权威源是 `plan.md` §4；本篇只导航最高层的四条：

- **A-1（worker 线程 → 请求槽状态机）**：9 个内核可见线程变成 9 个用户可见槽；论证（Linux workqueue / Redox async / seL4 endpoint 三方对照）在 08-worker-thread
- **A-4（全局单例 → VfsState 聚合）**：C 的 `glo.h` 散装全局收进一个 `VfsState`，七张表 + 槽池 + 启动相位（Fix #30 完成组合完备）
- **A-2（函数指针表 → 枚举分发）**：`call_vec` 的 64 项函数指针变成 `VfsCallNum` 枚举 + `from_raw` 单真相；绑定 match（`dispatch_syscall`）是接线矩阵 W3
- **A-5（SUSPEND 返回码 → ReplyIntent）**：C 用整数返回码区分"已回复/稍后回复"，Rust 用 `ReplyIntent` 枚举

## 4 实现详解：模块地图

`os/servers/vfs/src/` 33 个文件的归属：`main_loop.rs`（状态聚合 + 路由，09）、`worker.rs`（槽池，08）、`ipc/dispatcher.rs`（PM 控制面，10）、`fs_comm.rs`（FS 通信窗口，11）、`request.rs`（REQ 包装，12）、`path.rs`（路径解析，13）、`filedes.rs`/`filp.rs`/`fproc.rs`（fd 三层，14/02/04）、`vnode.rs`/`vmnt.rs`/`tll.rs`（表与锁，05/06/07）、`open.rs`/`read_write.rs`/`link.rs`/`stadir.rs`/`protect.rs`/`fcntl.rs`（syscall 族，15/16/27/28/29/30）、`misc.rs`（杂项，31）、`exec.rs`/`coredump.rs`（执行体，25/26）、`pipe.rs`/`select.rs`/`socket.rs`（进程间，17/23/24）、`bdev.rs`/`cdev.rs`/`sdev.rs`/`device_map.rs`（设备层，20/21/22/19）、`mount.rs`（挂载，18）、`call_table.rs`（调用号，09）。

## 5 测试要点

全 crate 346 个内联测试（计数随修复轮演进；`cargo test --manifest-path os/Cargo.toml -p minix-vfs --lib`），策略是决策纯函数直测：每个 C 分支决策被提为纯函数后配正反样本。集成测试（跨模块消息回路）是接线矩阵 W1-W9 的关闭条件，见 plan.md §8。

## 6 过渡与 7 参见

两条阅读路径：沿启动主线读 01 → 08 → 09 → 10 → 18；沿 syscall 族读 14 → 15/16 → 各族篇。横向常量与术语随时查 99-global-concepts。

- C 源：`minix3/minix/servers/vfs/`（33 .c + 15 .h）
- 接线矩阵：`plan.md` §8（W1-W9）
- 省略台账：`99-global-concepts.md` 有意省略表
