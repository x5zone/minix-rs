# 05-stage-vfs 文档重组计划（plan.md）

> **状态**: 生效中（2026-08-16 首版，深度 review + minix3 源码回归 review 后定稿）
> **范围**: `notes/rewrite/fork-syscall-rewrite/05-stage-vfs/`
> **目标**: 以 **VFS server 启动顺序为主线**重组 VFS 全部文档；`fork` 系统调用降为次主线；最终覆盖 Minix3 VFS server 全部语义（33 个 .c + 15 个本地 .h，16,735 行 C，64 个 VFS 调用 + 12 个 VFS_PM 请求 + 35 个 REQ_ 协议面），支撑 VFS server 的彻底 Rust 重写
> **对照**: `01-stage-kernel/`（讲述结构参照）、`02-stage-vm/plan.md` + `04-stage-pm/plan.md`（同型重组范本）、`minix3/minix/servers/vfs/`（ground truth）、`os/servers/vfs/src/`（Rust 实现）

---

## 1. 背景与动机

### 1.1 旧主线（fork 主线）的问题

旧文档（已移入 `draft/`）以 fork 系统调用为主线：21 篇文档沿 fork 路径展开（fproc 结构 → filp/vnode/vmnt → tll/worker → 主循环 → service_pm → pm_fork 拆成 5 篇 → filedes/comm/pm_exit）。实践发现三类问题：

1. **覆盖严重不足**——VFS 语义主体（路径解析 path.c 933 行 / select.c 1416 行 / sdev.c 1114 行 / request.c 1213 行 / misc.c 1006 行 / main.c 973 行 / socket.c 762 行 / exec.c 763 行 / mount.c 653 行 / filedes.c 656 行）**完全没有进入文档**。fork 只是 `table.c` 注册的 64 个 VFS 调用 + 12 个 VFS_PM 请求中的 1 个流程；以 fork 为主线的文档对"VFS 是什么"的回答是残缺的。
2. **组件与流程错位**——`fproc` 结构、filp/vnode/vmnt 表本应在 VFS 启动时按序初始化（`main.c:sef_cb_init_fresh`），却被"fork 需要什么"的倒推逻辑打散；VFS 运行的"心脏"（`main.c` 主循环、`table.c` 分发、`get_work`/`send_work`、FS transid 回复路由）只有一篇 `10-main-loop` 且未含 `table.c` 的 64 调用分发面；PM 协议（`service_pm`）与 fork 拆成 7 篇（11 + 12~16 + 19），而 `pm_exec`/`pm_exit`/`pm_setuid` 等其余 11 个 VFS_PM 请求缺席。
3. **架构演进（ARCH）标注分散/缺失**——VFS 是 Minix3 唯一使用 mthread 多线程的服务器（9 worker 线程），minix-rs 的单线程事件循环演进（`worker.rs` 状态机已存在）散见于 draft 文档注释，未形成统一 ARCH 清单。

### 1.2 新主线：VFS server 启动顺序

与 `01-stage-kernel`、`02-stage-vm`、`04-stage-pm` 相同，采用**读者学习顺序 = 系统实际执行顺序**的组织原则。VFS 的启动链（`main.c`）：

```
RS 加载 VFS（boot image，见 01-stage-kernel/06-proc-init-boot-proc.md）
  │
  ▼  main.c:54  main()
  └─ sef_local_startup()                ← 01：SEF 回调注册（init_fresh + init_lu + lu_prepare）
       └─ sef_startup() → sef_cb_init_fresh()   main.c:393
            ├─ fproc 表清零（fp_endpoint=NONE / fp_pid=PID_FREE）  ← 02/03（main.c:406-408）
            ├─ VFS_PM_INIT 握手循环（收 PM 消息填 fproc 槽，NONE 终止） ← 01/10（main.c:416-436）
            ├─ ipc_send(PM, OK) 同步                              ← 01/10（main.c:436）
            ├─ system_hz = sys_hz()                               ← 01/99（main.c:438）
            ├─ ds_subscribe("drv\\.[bc]..\\..*")                  ← 19/24（main.c:441，驱动事件）
            ├─ worker_init()                                      ← 08（main.c:445）
            ├─ bsf_lock mutex_init                                ← 16/20（main.c:448，块特殊文件锁）
            ├─ init_dmap()                                        ← 19（main.c:451）
            ├─ init_smap()                                        ← 19/22（main.c:452）
            ├─ sys_safecopyfrom(RS rproctab) + map_service 循环    ← 19（main.c:455-466，boot 服务映射）
            ├─ fproc fp_lock init + fp_filp 清空                   ← 02/03（main.c:469-483）
            ├─ init_vnodes()                                      ← 05（main.c:486）
            ├─ init_vmnts()                                       ← 06（main.c:487）
            ├─ init_select()                                      ← 23（main.c:488）
            ├─ init_filps()                                       ← 04（main.c:489）
            └─ worker_start(VFS_PROC_NR, do_init_root)            ← 01/18（main.c:492-494）
                 ├─ worker_allow(FALSE)（拒绝请求直到根挂载完成）    ← 08/18
                 ├─ mount_pfs()                                   ← 18（pipe FS）
                 └─ mount_fs(DEV_IMGRD, "bootramdisk", "/", MFS)  ← 18（根 FS）
  │
  ▼  main.c:69-143  主循环（运行时）
  ├─ worker_yield()                     ← 08（让其他 worker 运行）
  ├─ send_work()                        ← 11（FS 请求队列刷新，main.c:72）
  ├─ get_work()                         ← 09（收消息，main.c:580）
  ├─ 分发：
  │   ├─ IS_VFS_FS_TRANSID → do_reply() ← 11/12（FS/驱动回复，worker 唤醒，main.c:81-95）
  │   ├─ who_e == PM → service_pm()     ← 10（PM 协议，main.c:96-98）
  │   ├─ is_notify() → DS/KERNEL/CLOCK  ← 19/09/23（ds_event/栈追踪/select 定时器，main.c:99-115）
  │   ├─ 负 endpoint task 消息忽略        ← 09（main.c:116-124）
  │   ├─ IS_BDEV_RS → bdev_reply()      ← 20（main.c:126-128）
  │   ├─ IS_CDEV_RS → cdev_reply()      ← 21（main.c:129-131）
  │   ├─ IS_SDEV_RS → sdev_reply()      ← 22（main.c:132-134）
  │   └─ 正常 syscall → handle_work(do_work) ← 09 + call_vec → 各服务（14~31，main.c:135-142）
  └─ result != SUSPEND → reply()        ← 09（SUSPEND = 稍后由 revive/驱动回复路径回复）
```

**每篇文档必须能回答一个问题：它位于 VFS 启动时序（sef_cb_init_fresh）或主循环（dispatch）的哪个位置。** 这是从 `01-stage-kernel/00-kernel-overview.md §3` 继承的组织原则。

### 1.3 fork 次主线

fork 不再充当"概念引入的驱动"，而是作为**阶段 4（PM 协议）的核心服务之一**展开，其路径图在 `10-pm-protocol` 内部绘制：

```
VFS_PM_FORK 到达（主循环 dispatch，09）
  ├─ 03 进程表：childno = _ENDPOINT_P(cproc) + fproc[childno].fp_pid 空闲检查
  ├─ 10：fproc[childno] = fproc[parentno]（保留 fp_lock，槽位锁属于槽）
  ├─ 04：fp_filp[i] 非空 → filp_count++（fd 表共享）
  ├─ 05：fp_rd/fp_wd → dup_vnode()（目录引用计数）
  ├─ 02：cp->fp_pid = cpid / cp->fp_endpoint = cproc / fp_flags = FP_NOFLAGS
  ├─ 10：VFS_PM_FORK_REPLY 回 PM（SRV_FORK 另附 setuid/setgid）
  └─ 后续：pm_exit（10）→ free_proc（close_fd 递减 filp_count + put_vnode 回收）
```

---

## 2. 新文档编号与阶段划分

> 编号规则与 `01-stage-kernel` 一致：`NN-短横线语义名.md`；`00-` 为总览；`99-` 为全局概念。原 `draft/` 文档保留旧编号（作为素材），新编号在顶层重新建立。

### 阶段总览

| 阶段 | 编号 | 文档 | 语义模块 | C 源码 | Rust 模块 | draft 来源 | 变更 |
|------|------|------|---------|--------|-----------|-----------|------|
| 0 总览 | 00 | `00-vfs-overview.md` | VFS 是什么、启动主线图、文档导航、设计原则 | `servers/vfs/` 全部 | 全部 | `draft/00` + `draft/99` | **重写**：导航改为启动主线叙事 |
| 1 启动入口与进程模型 | 01 | `01-vfs-init-main.md` | main()/SEF 三回调/VFS_PM_INIT 握手/init_* 调用点/do_init_root/mount_pfs/worker_allow | `main.c:54-141,374-499,501-523` | `main.rs`、`main_loop.rs:run/init_fresh` | `draft/10` 启动部分 | **拆分**：启动链独立成篇 |
| 1 | 02 | `02-fproc-struct.md` | struct fproc 全字段、fp_flags、fp_blocked_on + fp_u 五类阻塞状态 union、凭证字段 | `fproc.h` | `fproc.rs:FProc/FpFlags/BlockedOn` | `draft/01`+`draft/02`+`draft/03`+`draft/fproc-design` | **合并**：结构/标志/凭证三篇合一 |
| 1 | 03 | `03-fproc-table.md` | fproc[NR_PROCS] 表、okendpt/isokendpt_f 验证、PID_FREE 槽语义、fproc_light（MIB） | `utility.c:94-127`、`fproc.h:117-124`、`glo.h` | `fproc.rs:FProcTable` | `draft/09` 部分 | **新建**：表操作与结构分离 |
| 2 核心数据结构 | 04 | `04-filp-table.md` | struct filp、init_filps、get_filp/get_filp2/find_filp*、filp 引用计数与锁、FSF_* 标志 | `file.h`、`filedes.c:73-249,313-430` | （未实现）filp 模块 | `draft/04`+`draft/filp-refcount` | 沿用 + 补表操作 |
| 2 | 05 | `05-vnode-table.md` | struct vnode、init_vnodes、get_free_vnode/find_vnode、lock_vnode 族、dup/put_vnode、vnode_clean_refs、v_fs_count 延迟同步 | `vnode.h`、`vnode.c` | （未实现）vnode 模块 | `draft/05`+`draft/vnode-refcount` | 沿用 + 补引用计数不变量 |
| 2 | 06 | `06-vmnt-table.md` | struct vmnt、init_vmnts、get_free_vmnt/find_vmnt、lock_vmnt 族、mark_vmnt_free、vmnt_unmap_by_endpt、VMNT_* 标志 | `vmnt.h`、`vmnt.c` | （未实现）vmnt 模块 | `draft/06` | 沿用 |
| 3 并发基础 | 07 | `07-tll-lock.md` | 三级锁 tll_t、tll_lock/unlock/downgrade/upgrade、TLL_* 语义、锁等待队列 | `tll.h`、`tll.c` | （未实现）锁模块 | `draft/07` | 沿用 |
| 3 | 08 | `08-worker-thread.md` | worker 池（NR_WTHREADS=9）、worker_init/start/stop/yield/wait/suspend/resume、worker_allow/block_all、w_fp 关联、thread_cleanup | `threads.h`、`worker.c`、`glo.h` | `worker.rs:WorkerPool/WorkerThread` | `draft/08` | 沿用 + ARCH（A-1）显式章节 |
| 3 | 09 | `09-main-loop.md` | main 主循环、get_work/do_work/do_reply、call_vec 64 调用（table.c）、reply/replycode、transid 路由、notify 处理、unblock/revive 入口 | `main.c:54-192,263-302,580-663,921-973`、`table.c`、`glo.h` | `main_loop.rs:VfsState/run`、`call_table.rs:CallTable/VfsCallNum` | `draft/10` 主循环部分 | 沿用 + 补分发面 |
| 4 PM 协议（fork 次主线） | 10 | `10-pm-protocol.md` | service_pm 全 12 个 VFS_PM 请求、pm_fork（fork 次主线核心）、pm_exit/free_proc、pm_exec 转发、pm_setuid/setgid/setgroups/setsid、pm_reboot、pm_dumpcore、service_pm_postponed、VFS_PM_* 消息族 | `main.c:764-920,668-763`、`misc.c:577-1010`、`com.h:513-544` | `main_loop.rs:PmMessageType`、`ipc/dispatcher.rs` | `draft/11`+`draft/12~16`+`draft/19`+`draft/pm-fork-impl` | **合并**：PM 协议 8 篇合一 + 补全 12 请求 |
| 5 FS 通信协议 | 11 | `11-fs-comm.md` | comm.c：sendmsg/send_work/fs_sendmore/fs_cancel/fs_sendrec/drv_sendrec/vm_sendrec/vm_vfs_procctl_handlemem/queuemsg、VFS_TRANSID 编码、m_comm 请求队列、VMNT_CALLBACK、sending 计数 | `comm.c`、`request.h`、`com.h:909-912` | （未实现）fs 通信模块 | `draft/18` | 沿用 |
| 5 | 12 | `12-request-wrappers.md` | request.c 全部 req_* 包装（req_lookup/req_readwrite/req_create/req_readsuper/req_breadwrite/...）、35 个 REQ_* 协议面、node_details/lookup_res | `request.c`、`vfsif.h:41-73`、`request.h` | （未实现）fs 客户端 | 无 | **新建** |
| 6 路径解析 | 13 | `13-path-lookup.md` | lookup/advance/eat_path/last_dir/get_name/canonical_path/lookup_init、fetch_name/copy_path、挂载点穿越、符号链接循环（SYMLOOP=16）、DO_POSIX_PATHNAME_RES | `path.c`、`path.h`、`utility.c:24-93` | （未实现）path 模块 | `draft/00` 部分 | **新建** |
| 7 文件描述符与文件 I/O | 14 | `14-filedes.md` | get_fd/check_fds/close_fd/do_copyfd/invalidate_filp*/find_filp_by_*、fd 表与 cloexec、FD 复用语义 | `filedes.c:88-140,250-312,430-656` | `fproc.rs:fp_filp` 字段 | `draft/17` | 沿用 |
| 7 | 15 | `15-open-close.md` | do_open/do_creat/common_open/new_node/pipe_open/do_mknod/do_mkdir/do_close/do_lseek/actual_lseek、oflags/mode_map | `open.c` | （未实现）open 模块 | 无 | **新建** |
| 7 | 16 | `16-read-write.md` | do_read/do_write/do_read_write_peek/read_write/do_getdents/rw_pipe、bsf 锁（lock_bsf/unlock_bsf）、跨 FS 读写路径 | `read.c`、`write.c`、`glo.h:bsf_lock` | （未实现）read/write 模块 | 无 | **新建** |
| 7 | 17 | `17-pipe.md` | do_pipe2/create_pipe/pipe_check/suspend/pipe_suspend/unsuspend_by_endpt/revive/release/unpause、susp_count/reviving 全局、map_vnode | `pipe.c` | （未实现）pipe 模块 | 无 | **新建** |
| 8 挂载管理 | 18 | `18-mount.md` | do_mount/mount_fs/mount_pfs/do_umount/unmount/unmount_all/name_to_dev/update_bspec/is_nonedev/find_free_nonedev、ROOT_DEV/ROOT_FS_E、have_root | `mount.c` | （未实现）mount 模块 | 无 | **新建** |
| 9 设备 I/O | 19 | `19-device-map.md` | dmap/smap 设备表、init_dmap/init_smap、do_mapdriver/map_service/map_driver、dmap_* 族、smap_* 族、do_ioctl/make_ioctl_grant、CTTY_ENDPT、设备恢复（dmap_endpt_up） | `dmap.c`、`smap.c`、`device.c`、`dmap.h` | （未实现）dmap/smap 模块 | `draft/09` 部分 | **新建** |
| 9 | 20 | `20-bdev.md` | 块设备：bdev_open/close/ioctl、bdev_sendrec/bdev_reply、bsf 缓存语义、bdev_up | `bdev.c` | （未实现）bdev 模块 | 无 | **新建** |
| 9 | 21 | `21-cdev.md` | 字符设备：cdev_map/get/clone/opcl/open/close/io/select/cancel、cdev_reply、grant 机制、CTTY 重定向 | `cdev.c` | （未实现）cdev 模块 | 无 | **新建** |
| 9 | 22 | `22-sdev.md` | socket 驱动：sdev_socket/bind/connect/listen/accept/readwrite/ioctl/setsockopt/getsockopt/getsockname/getpeername/shutdown/close/select、sdev_reply、resume_* 续作 | `sdev.c` | （未实现）sdev 模块 | 无 | **新建** |
| 10 多路复用 | 23 | `23-select.md` | do_select、select_request_*、select_filter、tab2ops/ops2tab、copy_fdsets、select_callback、select_*_reply、select_timeout_check、expire_timers、select 阻塞状态 | `select.c` | （未实现）select 模块 | 无 | **新建** |
| 11 网络 | 24 | `24-socket.md` | do_socket/socketpair/bind/connect/listen/accept/sendto/recvfrom/sockmsg/setsockopt/getsockopt/getsockname/getpeername/shutdown/socketpath、resume_accept/recvfrom/recvmsg | `socket.c` | （未实现）socket 模块 | 无 | **新建** |
| 12 进程执行与退出 | 25 | `25-exec.md` | pm_exec、Get_read_vp、脚本解释（is_script/patch_stack/insert_arg）、ELF 加载（read_seg/map_header）、vfs_memmap、clo_exec、stack_prepare_elf | `exec.c` | （未实现）exec 模块 | 无 | **新建** |
| 12 | 26 | `26-coredump.md` | write_elf_core_file、get_memory_regions、fill_*_header/dump_*/write_buf、pm_dumpcore（调用点 misc.c:903） | `coredump.c`、`misc.c:903-988` | （未实现）coredump 模块 | 无 | **新建** |
| 13 目录/链接/权限 | 27 | `27-link.md` | do_link/do_unlink/do_rename/do_truncate/do_ftruncate/do_slink/do_rdlink | `link.c` | （未实现）link 模块 | 无 | **新建** |
| 13 | 28 | `28-stadir.md` | do_chdir/fchdir/chroot、do_stat/fstat/lstat、do_statvfs/fstatvfs/getvfsstat、update_statvfs/fill_statvfs、change_into | `stadir.c` | （未实现）stadir 模块 | 无 | **新建** |
| 13 | 29 | `29-protect.md` | do_chmod/do_chown/do_umask/do_access、forbidden、in_group、权限位语义（R_BIT/W_BIT/X_BIT） | `protect.c`、`utility.c:128-141` | （未实现）protect 模块 | 无 | **新建** |
| 14 控制与杂项 | 30 | `30-fcntl-lock.md` | do_fcntl（F_DUPFD/F_GETFL/F_SETFL/F_GETLK/F_SETLK/F_SETLKW）、lock_op/lock_revive、NR_LOCKS=8 锁表、FP_BLOCKED_ON_FLOCK、flock 结构 | `misc.c:117-275`、`lock.c`、`lock.h`、`const.h:NR_LOCKS` | （未实现）fcntl/lock 模块 | 无 | **新建** |
| 14 | 31 | `31-misc-queries.md` | do_sync/do_fsync/do_getsysinfo/do_vm_call/do_svrctl/do_getrusage、do_utimens、do_gcov_flush、panic_hook、dupvm | `misc.c:52-116,276-576,989-1006`、`time.c`、`gcov.c` | （未实现）misc 模块 | 无 | **新建** |
| 99 全局概念 | 99 | `99-global-concepts.md` | 常量表（const.h）、全局状态（glo.h）、类型（type.h/fs.h）、引用计数模型（filp/vnode 双层）、endpoint/transid 术语、sys_datacopy_wrapper | `const.h`、`glo.h`、`type.h`、`fs.h`、`proto.h`、`utility.c:142-186` | `minix-types`、`call_table.rs` 常量 | `draft/09`+`draft/99` | **合并**：全局概念定稿 |

### 2.1 阶段间的叙事衔接

每个阶段结尾的文档需含"过渡"小节（参照 `01-stage-kernel` 每篇 §6"过渡"），说明本阶段在启动时序中的位置与下一阶段的入口：

```
01（启动骨架）→ 02/03（进程模型：结构 + 表 + endpoint）
→ 04~06（核心数据结构：filp/vnode/vmnt 表）
→ 07/08（并发基础：三级锁 + worker 线程池）
→ 09（主循环与分发：运行时心脏）
→ 10（PM 协议：VFS 唯一的"非 syscall 入口"，fork 次主线）
→ 11/12（FS 通信：请求队列 + req_* 协议面）
→ 13（路径解析：所有名字类调用的前置）
→ 14~17（fd 表 + open/close + read/write + pipe）
→ 18（挂载）→ 19~22（设备表 + bdev/cdev/sdev）
→ 23（select）→ 24（socket）
→ 25/26（exec + coredump：进程执行关联）
→ 27~29（link/stat/权限）
→ 30/31（fcntl 锁 + 杂项查询）
→ 99（全局概念收尾）
```

---

## 3. 讲述结构规范（参照 01-stage-kernel）

### 3.1 每篇文档的章节模板

与 `01-stage-kernel` 一致（见 `03-kmain-cstart.md`、`04-platform-discovery.md` 等）：

1. **概念**——为什么需要这个机制、类比、边界声明（前置依赖/本篇不覆盖什么）
2. **C 源码分析**——ground truth，必须 grep 实证，禁止凭记忆
3. **Rust 设计决策**——类型系统建模、与 C 的差异（含 ARCH 标注）
4. **实现详解**——Rust 模块结构、关键不变量
5. **测试要点**——相关测试函数 + 测试总数声明
6. **过渡**——在启动时序/主循环中的位置，下一篇入口
7. **参见**——跨文档引用（新编号）

### 3.2 组织原则（继承 01-stage-kernel §3）

1. **概念首次出现即完整解释**——后续只引用不复述
2. **禁止前向引用**——依赖机制前置（本计划的编号已保证：filp/vnode/vmnt（04~06）在 fd 操作（14）之前；worker（08）在主循环（09）之前；FS 通信（11/12）在路径解析（13）与文件 I/O（14~17）之前；PM 协议（10）在 fork 次主线之前）
3. **每篇一个语义单元**——读者可独立阅读
4. **位置可回答性**——每篇回答"它在 sef_cb_init_fresh / 主循环分发的哪个位置"

### 3.3 引用规则

- 各文档之间用新编号交叉引用（如 `10-pm-protocol.md` §fork 次主线）
- 与 kernel 文档交叉引用时用 `../01-stage-kernel/NN-*.md`（如 VFS 启动 → `06-proc-init-boot-proc.md`；syscall 转发 → `17-syscall-process.md`；safecopy → `18-syscall-copy.md`）
- 与 VM 文档交叉引用时用 `../02-stage-vm/NN-*.md`（如 `25-exec.md` → `20-vm-mmap.md`；`10-pm-protocol.md` → VM fork 交互）
- 与 PM 文档交叉引用时用 `../04-stage-pm/NN-*.md`（如 `01-vfs-init-main.md` → `05-vfs-interaction.md`）
- 对 draft 素材的引用一律指向 `draft/NN-*.md`，并标注"素材"；正式文档绝不引用 review 产物

### 3.4 每篇文档边界声明（执行级）

> 写作时必须包含"前置依赖/职责/不覆盖"边界声明，防止内容交叉（模式 45 教训，02-stage-vm plan D-19）。

| 文档 | 前置依赖 | 职责 | 不覆盖（移交） |
|------|---------|------|---------------|
| 00 | 无 | 全局叙事 + 导航 | 一切机制 |
| 01 | 00 + kernel 06 | 启动时序、SEF、VFS_PM_INIT 握手、init_* 调用点 | 各表结构（02~07）、主循环（09）、PM 协议细节（10）、挂载细节（18） |
| 02 | 01 | fproc 结构字段、标志、阻塞状态 union | 表操作（03） |
| 03 | 01/02 | fproc 表、endpoint 验证、fproc_light | 结构字段（02）、槽位使用方（10/14） |
| 04 | 01（init_filps 调用点） | filp 结构、查找、引用计数 | fd 表条目管理（14）、pipe/select 的 filp 字段使用（17/23） |
| 05 | 01（init_vnodes 调用点） | vnode 结构、生命周期、引用计数不变量 | 挂载表关联（06）、路径解析使用（13） |
| 06 | 01（init_vmnts 调用点） | vmnt 结构、挂载表管理、锁 | mount 流程（18）、FS 通信队列（11） |
| 07 | 无（独立机制） | 三级锁原语 | 锁使用方（04/05/06/14） |
| 08 | 01（worker_init 调用点） | worker 池状态机、调度、阻塞原语 | 主循环（09）、消息内容处理（10~31） |
| 09 | 01/08 | 主循环、分发、回复、transid、call_vec | service_pm 内容（10）、FS 队列细节（11）、驱动回复细节（20~22） |
| 10 | 03/09 | service_pm 全协议、fork 次主线、pm_* 族 | syscall 面（14~31）、FS 协议（11/12） |
| 11 | 06（m_comm 字段）/09 | 请求队列、transid 编码、fs/drv/vm 收发 | req_* 包装函数（12）、具体调用流程（14~31） |
| 12 | 11 | req_* 包装、REQ_* 协议面 | 队列机制（11）、FS 服务端实现（不在 VFS 范围） |
| 13 | 05/06 | 路径解析、名字拷贝、挂载点穿越 | open 流程（15）、exec 的脚本解析（25） |
| 14 | 02/04 | fd 表操作、close_fd、copyfd、invalidate | filp 结构（04）、PM 的 fork 复制（10） |
| 15 | 13/14 | open/creat/close/lseek/mknod/mkdir | 路径解析（13）、设备打开（19~22） |
| 16 | 04/14 | read/write/getdents、bsf 锁 | pipe 阻塞语义（17）、驱动收发（20~22） |
| 17 | 04/16 | pipe 语义、suspend/revive 机制 | 通用阻塞状态机（09/02）、select 阻塞（23） |
| 18 | 06/11 | mount/umount 流程、根挂载 | vmnt 表结构（06）、FS 协议（12） |
| 19 | 01（init_dmap/smap 调用点） | 设备表、驱动映射、ioctl 分派 | 具体驱动 I/O（20~22） |
| 20 | 19 | 块设备 I/O、bsf 缓存、bdev_reply | dmap 表（19）、FS REQ 协议（12） |
| 21 | 19 | 字符设备 I/O、grant、cdev_reply | dmap 表（19）、select 的 cdev 路径（23） |
| 22 | 19 | socket 驱动 I/O、sdev_reply、resume | smap 表（19）、socket syscall（24） |
| 23 | 04/09/21/22 | select 全流程、定时器、回复路径 | socket syscall（24）、pipe 阻塞（17） |
| 24 | 22/23 | socket 系统调用族 | 驱动协议（22）、select 机制（23） |
| 25 | 10（pm_exec 入口）/13 | exec 全流程、脚本、ELF 加载、VM 交互 | VM mmap 实现（02-stage-vm/20）、PM exec 状态机（04-stage-pm/17） |
| 26 | 10（pm_dumpcore 入口） | core dump 生成 | 信号语义（04-stage-pm/11~13） |
| 27 | 13/14 | link/unlink/rename/truncate/symlink/readlink | 路径解析（13） |
| 28 | 13 | chdir/chroot/stat 族/statvfs 族 | 路径解析（13）、权限检查（29） |
| 29 | 02（凭证字段） | chmod/chown/umask/access、forbidden | 凭证设置（10 pm_setuid 族） |
| 30 | 04/09 | fcntl 全命令、POSIX 记录锁、lock_revive | fd 表（14） |
| 31 | 03/09 | sync/fsync/getsysinfo/vm_call/svrctl/getrusage/utimens/gcov | 具体服务（14~30） |
| 99 | 00 | 常量、全局状态、术语、跨文档工具 | 一切机制 |

### 3.5 测试基线（截至 2026-08-16）

- `cargo test -p minix-vfs --lib`：**35 passed / 0 failed**（实测基线，2026-08-16；模块分布：fproc 8、worker 8、main_loop 11、call_table 若干）
- 每篇改写完成时在文末更新该模块测试统计（review-doc-skill §2.4j）

### 3.6 Review gate 要求（每篇改写必检）

- 每篇新文档创建时同步生成 `.design/{NN}-outline.md` + `{NN}-outline-review.md` + `{NN}-design.md`（Step 0.3 嵌入生成，Gate H.6，不允许 N/A）
- 每篇改写后按 review 工作流跑 Blocker Gates（0/A/B/C/D/D-6/E/G/H），scan 产物写入 `.review/codex/vfs/{NN}-{name}/`
- P0 未清不得标完成；doc 与 code 保持同步

---

## 4. 架构演进（ARCH）清单

> 按 review-core 要求，ARCH 必须三处一致标注：Minix3 行为对照点 + design doc + 代码注释。以下为 05-stage-vfs 范围内的 ARCH 项，写文档时必须逐项落实。状态以 `os/servers/vfs/src/` 实际代码为准。

| # | ARCH 项 | Minix3 现状 | minix-rs 演进 | 涉及新文档 | 状态 |
|---|---------|------------|--------------|-----------|------|
| A-1 | **执行模型：mthread 多线程 → 单线程事件循环 + 请求槽状态机** | VFS 是 Minix3 唯一多线程服务器：main 线程 + 9 个 worker 线程（`NR_WTHREADS=9`，`worker.c`，`worker_main` 真实线程 + `w_event_mutex`/`w_event` 条件变量） | worker 抽象保留为**请求槽状态机**（`worker.rs:WorkerState::Idle/Busy/WaitingForFs`），无真实线程；阻塞 I/O 由异步状态机/回复队列建模 | 08/09 | 部分实现（状态枚举已存在，无线程） |
| A-2 | **call_vec 函数指针表 → 类型化枚举分发** | `int (* const call_vec[NR_VFS_CALLS])(void)`（`table.c`），64 个调用 | `VfsCallNum` enum（64 变体）+ `CallTable::dispatch` match 路由（`call_table.rs`） | 09/99 | 已实现（枚举面） |
| A-3 | **fp_blocked_on union → 类型化阻塞枚举** | `fp_u` 五类 union（u_pipe/u_popen/u_flock/u_cdev/u_sdev，`fproc.h:31-57`） | `BlockedOn` 标签枚举 + 五类载荷结构（`fproc.rs`: Pipe/PipeOpen/Flock/Select/Cdev/Sdev，含 PipeIo/FlockCmd/SdevCall/SdevAux） | 02/09 | **已实现**（2026-08-17，02-fproc-struct） |
| A-4 | **全局变量 → VfsState 聚合** | `glo.h` 全局（fp/susp_count/reviving/sending/verbose/m_in/self/workers/err_code/bsf_lock...） | `VfsState` 聚合全部子系统状态（`main_loop.rs:VfsState`：fproc_table/worker_pool/call_table/reviving/current_message） | 09/99 | 部分实现 |
| A-5 | **SUSPEND/revive 机制显式化** | `return SUSPEND` 表示稍后回复；pipe/select/驱动三条恢复路径（`pipe.c:revive`、`select.c:select_return`、`sdev.c:sdev_finish`） | 异步回复意图枚举（`ReplyIntent::Reply/ReplyLater/NoReply`），回复队列 | 09/17/23 | **缺口**：未实现，标注语义契约 |
| A-6 | **mthread 锁/条件变量 → Rust 内部可变性** | `fp_lock` mutex、`filp_lock` mutex、`w_event_mutex`/`w_event` cond（`threads.h` 宏映射） | 单线程事件循环下 `Rc`/`RefCell`（`!Send`/`!Sync` 安全），锁降级为借用规则 + 显式状态 | 02/04/08 | 设计层 |
| A-7 | **fproc_light（MIB 轻量表）** | `fproc_light[NR_PROCS]`（`fproc.h:117-124`）由 MIB 服务拉取 | 取决于 MIB 服务实现；若保留则用只读快照 + codec | 03 | **缺口**：标注 defer |
| A-8 | **64 位类型映射** | `pid_t`/`uid_t`/`gid_t`/`dev_t`/`ino_t`/`off_t`/`endpoint_t` 平台相关 | `Pid`/`Uid`/`Gid`/`Endpoint`/`UserSlot`（`minix-types`），`UserSlot` 表达槽位 | 02/03/99 | 已实现 |
| A-9 | **LOCK_DEBUG 编译宏** | `#define LOCK_DEBUG 0`（`fproc.h:9`） | `#[cfg(feature = "lock_debug")]` 替代 | 07/99 | 设计差异 |
| A-10 | **POSIX 路径解析开关** | `DO_POSIX_PATHNAME_RES 0`（`path.c:28-35`，历史 Unix 行为：忽略尾部斜杠） | 显式配置常量 + 文档声明 | 13 | 设计决策 |
| A-11 | **socket 驱动模型** | `socket.c` + smap → socket 驱动（`sdev.c`），`sdev_sendrec` 同步往返 + `sdev_suspend` | 类型化 socket 模块 + 驱动协议 trait | 22/24 | 未实现 |
| A-12 | **grant 机制** | `cpf_grant_magic`/`make_ioctl_grant`（`device.c:52`），cdev 数据面 | 类型化 safecopy/grant 抽象（`minix-sys`） | 21/99 | 未实现 |
| A-13 | **select 定时器** | CLOCK notify → `expire_timers`（`main.c:112`），`set_timer`（`select.c:335`） | 类型化 `Timeout`/`Clock` + 定时器队列（minix-types） | 23/09 | 未实现 |
| A-14 | **DS 驱动事件订阅** | `ds_subscribe("drv\\.[bc]..\\..*")`（`main.c:441`）+ DS notify → `ds_event` | DS 客户端抽象 + 事件处理 | 01/19/09 | 未实现 |
| A-15 | **exec 的 VM 交互** | `vfs_memmap`/`map_header` 经 VM_MMAP 映射段（`exec.c:161`），`minix_get_user_sp` | 与 02-stage-vm `20-vm-mmap.md` 交互，类型化 mmap 请求 | 25 | 未实现 |

---

## 5. 覆盖完整性核对（对照 minix3 源码）

> 本节是 plan.md 的语义覆盖契约。所有断言以 grep 实证为准（review-core：禁止凭记忆）。逐项核对见 §7.2。

### 5.1 C 源文件 → 新文档映射（33 个 .c）

| C 源文件 | 行数 | 语义 | 新文档 | 说明 |
|---------|------|------|--------|------|
| `main.c` | 973 | 启动 + 主循环 + 回复 + PM 协议 | 01/09/10 | 按函数拆分（§5.3） |
| `table.c` | 82 | call_vec 分发表 | 09 | 64 个调用完整列出 |
| `utility.c` | 186 | 名字拷贝/endpoint 验证/工具 | 03/13/29/99 | `copy_path`/`fetch_name` 归 13；`isokendpt_f` 归 03；`in_group` 归 29；`sys_datacopy_wrapper` 归 99 |
| `misc.c` | 1006 | PM 协议 + fcntl + 杂项 | 10/30/31 | `pm_*`/`free_proc`/`panic_hook` 归 10/26/31；`do_fcntl` 归 30；其余归 31 |
| `open.c` | 727 | open/creat/close/lseek/mknod/mkdir | 15 | `close_fd`(690) 语义归 14 |
| `read.c` | 393 | read/write_peek/getdents/bsf 锁 | 16 | 整文件 |
| `write.c` | 25 | do_write | 16 | 并入 read-write |
| `pipe.c` | 561 | pipe + suspend/revive | 17 | 整文件 |
| `filedes.c` | 656 | filp 表 + fd 表操作 | 04/14 | `init_filps`/`get_filp*`/`find_filp*`/`lock_filp` 归 04；`get_fd`/`check_fds`/`invalidate_*`/`do_copyfd` 归 14（`close_fd` 定义于 `open.c:690`，语义归 14） |
| `mount.c` | 653 | mount/umount 全流程 | 18 | 整文件 |
| `vmnt.c` | 292 | vmnt 表 | 06 | 整文件 |
| `dmap.c` | 328 | 设备表 | 19 | 整文件 |
| `device.c` | 95 | do_ioctl/grant | 19 | 并入 device-map |
| `bdev.c` | 282 | 块设备 I/O | 20 | 整文件 |
| `cdev.c` | 508 | 字符设备 I/O | 21 | 整文件 |
| `sdev.c` | 1114 | socket 驱动 I/O | 22 | 整文件 |
| `smap.c` | 273 | socket 驱动表 | 19 | 并入 device-map |
| `select.c` | 1416 | select 全流程 | 23 | 整文件 |
| `socket.c` | 762 | socket 系统调用 | 24 | 整文件 |
| `path.c` | 933 | 路径解析 | 13 | 整文件 |
| `vnode.c` | 316 | vnode 表 | 05 | 整文件 |
| `tll.c` | 324 | 三级锁 | 07 | 整文件 |
| `lock.c` | 192 | POSIX 记录锁 | 30 | 整文件 |
| `worker.c` | 607 | worker 线程池 | 08 | 整文件 |
| `comm.c` | 244 | FS 通信原语 | 11 | 整文件 |
| `link.c` | 508 | link/unlink/rename/truncate/symlink | 27 | 整文件 |
| `exec.c` | 763 | exec 全流程 | 25 | 整文件 |
| `stadir.c` | 446 | chdir/stat/statvfs | 28 | 整文件 |
| `protect.c` | 302 | chmod/chown/umask/access | 29 | 整文件 |
| `time.c` | 155 | do_utimens | 31 | 并入 misc-queries |
| `coredump.c` | 327 | core dump 生成 | 26 | 整文件 |
| `gcov.c` | 73 | do_gcov_flush | 31 | 并入 misc-queries |

### 5.2 头文件覆盖

| 头文件 | 内容 | 新文档 |
|--------|------|--------|
| `fproc.h` | struct fproc + fp_flags + 阻塞 union + fproc_light | 02/03/99 |
| `file.h` | struct filp + FSF_* | 04/99 |
| `vnode.h` | struct vnode + VNODE_* 锁映射 | 05 |
| `vmnt.h` | struct vmnt + VMNT_* 标志 + 锁映射 | 06 |
| `tll.h` | TLL_* 锁类型 | 07 |
| `threads.h` | thread/mutex/cond 宏 + worker_thread 结构 | 08 |
| `const.h` | 表大小 + FP_BLOCKED_ON_* + CTTY_ENDPT + SYMLOOP + SEL_* | 02/09/13/17/19/99 |
| `glo.h` | 全局变量 + who_p/who_e/call_nr 宏 | 03/09/99 |
| `type.h` | 本地类型 | 99 |
| `fs.h` | master header（包含关系） | 99 |
| `proto.h` | 全函数原型 | 各文档（函数索引） |
| `path.h` | lookup 结构 + PATH_* 标志 | 13 |
| `lock.h` | file_lock 结构 + NR_LOCKS 表 | 30 |
| `dmap.h` | struct dmap | 19 |
| `request.h` | node_details/lookup_res 响应结构 | 11/12 |
| `minix/vfsif.h` | REQ_* 35 个协议面 + TRNS_* | 11/12/99 |
| `minix/callnr.h` | VFS_BASE + 64 个调用号 | 09/99 |
| `minix/com.h` | VFS_PM_*（12 RQ + 11 RS）、VFS_TRANSID、IS_BDEV_RS 等 | 09/10/11/99 |
| `minix/dmap.h` | 通用 dmap（drv 接口） | 19 |

### 5.3 语义模块覆盖清单（函数级）

> 逐函数 grep 实证（命令见 §7.2）。函数 → 文档归属如下：

| 新文档 | 覆盖函数 |
|--------|---------|
| 01 | `main.c:main(54)/sef_local_startup(374)/sef_cb_init_fresh(393)/sef_cb_init_lu(352)/sef_cb_lu_prepare(303)/do_init_root(501)/lock_proc(528)/unlock_proc(547)`；`mount.c:mount_pfs(391)`（调用点）；`worker.c:worker_init(27)`（调用点）；init_* 全部调用点（§1.2 时序图） |
| 02 | `fproc.h` 全字段：fp_flags 6 位、fp_pid/fp_endpoint、fp_wd/fp_rd、fp_filp[]、fp_cloexec_set、fp_tty、fp_blocked_on + fp_u 五类、凭证 5 字段 + fp_sgroups、fp_umask、fp_lock/fp_worker/fp_func/fp_msg/fp_pm_msg、fp_name；`FP_*` 标志与 `PID_FREE`/`REVIVING` |
| 03 | `utility.c:isokendpt_f(94)/okendpt`；`fproc_addr`/`who_p` 宏（glo.h:26-28）；fproc_light（fproc.h:117-124） |
| 04 | `filedes.c:init_filps(73)/get_filp(162)/get_filp2(177)/find_filp(205)/find_filp_by_sock_dev(229)/lock_filp(313)/unlock_filp(357)/unlock_filps(382)/close_filp(413)/check_filp_locks_by_me(26)/check_filp_locks(48)`；`file.h` 全字段 |
| 05 | `vnode.c:init_vnodes(138)/get_free_vnode(85)/find_vnode(110)/lock_vnode(156)/unlock_vnode(177)/upgrade_vnode_lock(218)/dup_vnode(227)/put_vnode(240)/vnode_clean_refs(305)/check_vnode_locks_by_me(43)/is_vnode_locked(126)`；`vnode.h` 全字段 |
| 06 | `vmnt.c:init_vmnts(127)/get_free_vmnt(95)/find_vmnt(112)/lock_vmnt(150)/unlock_vmnt(199)/downgrade_vmnt_lock(221)/upgrade_vmnt_lock(237)/mark_vmnt_free(65)/clear_vmnt(76)/vmnt_unmap_by_endpt(180)/check_vmnt_locks_by_me(24)/fetch_vmnt_paths(246)/is_vmnt_locked(141)` |
| 07 | `tll.c:tll_init(113)/tll_lock(139)/tll_unlock(230)/tll_downgrade(74)/tll_upgrade(306)/tll_islocked(126)/tll_locked_by_me(132)/tll_haspendinglock(220)/tll_append(11)`；`tll.h` 锁类型 |
| 08 | `worker.c:worker_init(27)/worker_cleanup(63)/worker_idle(109)/worker_assign(119)/worker_may_do_pending(147)/worker_allow(162)/worker_get_work(192)/worker_available(228)/worker_main(239)/worker_can_start(295)/worker_try_activate(331)/worker_start(360)/worker_yield(431)/worker_sleep(443)/worker_wake(459)/worker_suspend(474)/worker_resume(493)/worker_wait(510)/worker_signal(526)/worker_stop(535)/worker_stop_by_endpt(555)/worker_get(572)/worker_set_proc(586)`；`main.c:thread_cleanup(558)` |
| 09 | `main.c:main 主循环(69-143)/get_work(580)/do_work(263)/do_reply(187)/handle_work(146)/reply(638)/replycode(655)/unblock(921)`；`table.c:call_vec(17)` 64 调用；`glo.h` who_e/call_nr/is_notify 宏 |
| 10 | `main.c:service_pm(764)/service_pm_postponed(668)`；`misc.c:pm_fork(577)/pm_exit(713)/pm_setuid(764)/pm_setgid(726)/pm_setgroups(743)/pm_setsid(779)/pm_dumpcore(903)`；`com.h:VFS_PM_*` 12 RQ + 11 RS；fork 次主线路径图 |
| 11 | `comm.c:sendmsg(12)/send_work(37)/fs_cancel(50)/fs_sendmore(66)/drv_sendrec(89)/fs_sendrec(134)/vm_sendrec(173)/vm_vfs_procctl_handlemem(200)/queuemsg(223)`；transid 编码（vfsif.h:79-81、com.h:909-912） |
| 12 | `request.c` 全部 req_*：`req_lookup(424)/req_create(166)/req_readwrite(878)/req_breadwrite(64)/req_getdents(343)/req_readsuper(780)/req_newnode(624)/req_mountpoint(608)/req_putnode/req_unlink/req_rmdir/req_mkdir(528)/req_mknod(567)/req_link(390)/req_rename/req_slink(1048)/req_rdlink(753)/req_stat(1111)/req_chmod(108)/req_chown(136)/req_utime(1195)/req_statvfs(235)/req_ftrunc(261)/req_flush(219)/req_inhibread(374)/req_peek(903)/req_bpeek(89)/req_rdonly 标志`；`request.h` 响应结构 |
| 13 | `path.c:lookup(384)/advance(40)/eat_path(133)/last_dir(146)/lookup_init(575)/get_name(594)/canonical_path(648)/do_socketpath(803)`；`utility.c:copy_path(24)/fetch_name(60)`；`path.h` lookup 结构 |
| 14 | `filedes.c:check_fds(88)/get_fd(110)/do_copyfd(524)/invalidate_filp(250)/invalidate_filp_by_char_major(260)/invalidate_filp_by_sock_drv(277)/invalidate_filp_by_endpt(298)`；`open.c:close_fd(690)`；fd 表与 cloexec 语义 |
| 15 | `open.c:do_open(38)/do_creat(58)/common_open(83)/new_node(299)/pipe_open(483)/do_mknod(514)/do_mkdir(564)/actual_lseek(603)/do_lseek(655)/do_close(674)` |
| 16 | `read.c:do_read(30)/do_read_write_peek(127)/actual_read_write_peek(92)/read_write(135)/do_getdents(282)/rw_pipe(323)/lock_bsf(49)/unlock_bsf(67)/check_bsf_lock(76)`；`write.c:do_write(15)` |
| 17 | `pipe.c:do_pipe2(39)/create_pipe(60)/pipe_check(187)/suspend(294)/pipe_suspend(315)/unsuspend_by_endpt(334)/release(363)/revive(435)/unpause(498)/map_vnode(151)`；susp_count/reviving 全局 |
| 18 | `mount.c:do_mount(85)/mount_fs(156)/mount_pfs(391)/do_umount(430)/unmount(470)/unmount_all(552)/name_to_dev(590)/update_bspec(46)/is_nonedev(628)/find_free_nonedev(641)`；have_root/ROOT_DEV/ROOT_FS_E |
| 19 | `dmap.c:init_dmap(230)/do_mapdriver(106)/map_driver(61)/map_service(200)/lock_dmap(27)/unlock_dmap(47)/dmap_unmap_by_endpt(180)/dmap_driver_match(252)/dmap_endpt_up(275)/get_dmap_by_endpt(317)`；`smap.c:init_smap(22)/smap_map(47)/smap_unmap_by_endpt(148)/smap_endpt_up(173)/make_smap_dev(201)/get_smap_by_dev(217)/get_smap_by_endpt(244)/get_smap_by_domain(266)`；`device.c:do_ioctl(18)/make_ioctl_grant(52)` |
| 20 | `bdev.c:bdev_open(79)/bdev_close(114)/bdev_ioctl(144)/bdev_sendrec(34)/bdev_reply(193)/bdev_up(227)` |
| 21 | `cdev.c:cdev_open(254)/cdev_close(264)/cdev_io(280)/cdev_map(36)/cdev_get(63)/cdev_clone(97)/cdev_opcl(149)/cdev_select(350)/cdev_cancel(381)/cdev_generic_reply(429)/cdev_reply(481)` |
| 22 | `sdev.c:sdev_socket(119)/sdev_bind(220)/sdev_connect(230)/sdev_listen(280)/sdev_accept(292)/sdev_readwrite(340)/sdev_ioctl(417)/sdev_setsockopt(454)/sdev_getsockopt(561)/sdev_getsockname(572)/sdev_getpeername(582)/sdev_shutdown(592)/sdev_close(604)/sdev_select(647)/sdev_suspend(83)/sdev_sendrec(59)/sdev_simple(242)/sdev_get(504)/sdev_finish_accept(679)/do_accept_reply(732)/sdev_finish(759)/sdev_stop(912)/sdev_cancel(940)/sdev_reply(989)` |
| 23 | `select.c:do_select(96)/select_filter(409)/select_request_char(462)/select_request_sock(527)/select_request_file(567)/select_request_pipe(577)/tab2ops(621)/ops2tab(635)/copy_fdsets(660)/select_cancel_all(714)/select_cancel_filp(740)/select_return(783)/select_callback(808)/init_select(821)/select_forget(833)/select_timeout_check(861)/select_unsuspend_by_endpt(884)/select_reply1(956)/select_cdev_reply1(1004)/select_sdev_reply1(1060)/select_reply2(1111)/select_cdev_reply2(1167)/select_sdev_reply2(1198)/select_restart_filps(1217)/wipe_select(1318)/select_lock_filp(1337)`；filp select 字段（file.h） |
| 24 | `socket.c:do_socket(176)/do_socketpair(224)/do_bind(308)/do_connect(326)/do_listen(344)/do_accept(365)/resume_accept(399)/do_sendto(483)/do_recvfrom(504)/resume_recvfrom(526)/do_sockmsg(543)/resume_recvmsg(608)/do_setsockopt(657)/do_getsockopt(676)/do_getsockname(701)/do_getpeername(724)/do_shutdown(747)/do_socketpath(803)/get_sock_flags(44)/check_sock_fds(64)/make_sock_fd(86)/get_sock(276)` |
| 25 | `exec.c:pm_exec(185)/Get_read_vp(89)/is_script(522)/patch_stack(534)/insert_arg(607)/read_seg(688)/map_header(736)/vfs_memmap(161)/stack_prepare_elf(413)/clo_exec(721)` |
| 26 | `coredump.c:write_elf_core_file(32)/fill_elf_header(80)/fill_prog_header(104)/fill_note_segment_and_entries_hdrs(126)/adjust_offsets(162)/write_buf(176)/get_memory_regions(191)/dump_notes(233)/dump_elf_header(275)/dump_program_headers(283)/dump_segments(294)`；`misc.c:pm_dumpcore(903)` |
| 27 | `link.c:do_link(29)/do_unlink(91)/do_rename(169)/do_truncate(276)/do_ftruncate(327)/do_slink(387)/do_rdlink(471)` |
| 28 | `stadir.c:do_chdir(50)/do_fchdir(32)/do_chroot(83)/change_into(117)/do_stat(140)/do_fstat(173)/do_lstat(418)/update_statvfs(197)/fill_statvfs(234)/do_statvfs(294)/do_fstatvfs(328)/do_getvfsstat(351)` |
| 29 | `protect.c:do_chmod(25)/do_chown(98)/do_umask(182)/do_access(198)/forbidden(238)`；`utility.c:in_group(128)` |
| 30 | `misc.c:do_fcntl(117)`；`lock.c:lock_op(21)/lock_revive(172)`；`lock.h` file_lock 结构；NR_LOCKS/FP_BLOCKED_ON_FLOCK |
| 31 | `misc.c:do_sync(276)/do_fsync(297)/do_getsysinfo(52)/do_vm_call(380)/do_svrctl(797)/do_getrusage(998)/dupvm(328)/panic_hook(989)/free_proc(639)`（free_proc 主语义归 10，调用点归 26）；`time.c:do_utimens(26)`；`gcov.c:do_gcov_flush(10)` |
| 99 | `const.h` 全常量、`glo.h` 全全局、`type.h`、`fs.h`、`proto.h` 函数索引、`utility.c:sys_datacopy_wrapper(142)`、minix 外部头（callnr/com/vfsif）常量 |

### 5.4 明确排除 / 跳过的项

| 项 | 说明 | 处置 |
|----|------|------|
| 底层 FS 服务端实现（MFS/PFS/ext2/isofs...） | `minix3/minix/fs/` 是 VFS 的"对端"，非 VFS server 语义 | 交叉引用 `minix3/minix/fs/`，不在 05-stage-vfs 展开 |
| `REQ_GETNODE` | `vfsif.h:41` 注释 "Should be removed"；全 VFS 无 `req_getnode` 包装函数（仅协议常量，无调用者） | 12 标注排除表 |
| `DO_POSIX_PATHNAME_RES`（=0） | 历史 Unix 行为开关（`path.c:28-35`） | 13 以常量声明（A-10） |
| `LOCK_DEBUG`（=0） | 编译宏锁调试（`fproc.h:9`） | 99 标注 cfg feature（A-9） |
| `ENABLE_SYSCALL_STATS`/`calls_stats` | 编译宏可选统计（若有） | 31 标注 cfg feature（A-9 同款模式） |
| 内核侧 `sys_*` 接口实现 | `sys_safecopyfrom/sys_datacopy/sys_hz/asynsend3/...` | 交叉引用 `../01-stage-kernel/18-syscall-copy.md` 等，不在 VFS 展开 |
| VM 侧 `VM_MMAP`/`VM_*` 实现 | 调用点在 VFS（`do_vm_call`/`vfs_memmap`），实现在对端 | 交叉引用 `../02-stage-vm/20-vm-mmap.md` 等 |
| PM 侧 `VFS_PM_*` 发送方状态机 | 对端实现（`tell_vfs` 等） | 交叉引用 `../04-stage-pm/05-vfs-interaction.md` |
| `do_getrusage`（table.c 标注 obsolete） | 已废弃但保留注册 | 31 标注（保持外部行为） |
| `panic_hook`/调试打印 | 非系统调用面 | 31 简注，不展开 |
| worker 真实线程栈分配（`worker.c:worker_main` 的 mthread 创建） | ARCH A-1 的结构消除 | 08 以 ARCH 章节说明，不照搬 |

---

## 6. 实施路线

### 6.1 文档改写状态跟踪

> 初始状态：全部为"最小骨架"（本计划交付物），按下列顺序逐篇改写为完整文档。改写顺序 = 阅读顺序（01 → 02 → ... → 31 → 99）。

| 编号 | 文档 | 状态 | 首次改写日期 | 最后 review 日期 |
|------|------|------|-------------|-----------------|
| 00 | `00-vfs-overview.md` | 骨架 | — | — |
| 01 | `01-vfs-init-main.md` | **已改写**（2026-08-17） | 2026-08-17 | — |
| 02 | `02-fproc-struct.md` | **已改写**（2026-08-17） | 2026-08-17 | — |
| 03 | `03-fproc-table.md` | **已改写**（2026-09-03） | 2026-09-03 | 2026-09-03 |
| 04 | `04-filp-table.md` | **已改写**（2026-09-03） | 2026-09-03 | 2026-09-03 |
| 05 | `05-vnode-table.md` | **已改写**（2026-09-03） | 2026-09-03 | 2026-09-03 |
| 06 | `06-vmnt-table.md` | **已改写**（2026-09-03） | 2026-09-03 | 2026-09-03 |
| 07 | `07-tll-lock.md` | **已改写**（2026-09-03） | 2026-09-03 | 2026-09-03 |
| 08 | `08-worker-thread.md` | 骨架 | — | — |
| 09 | `09-main-loop.md` | 骨架 | — | — |
| 10 | `10-pm-protocol.md` | 骨架 | — | — |
| 11 | `11-fs-comm.md` | 骨架 | — | — |
| 12 | `12-request-wrappers.md` | 骨架 | — | — |
| 13 | `13-path-lookup.md` | 骨架 | — | — |
| 14 | `14-filedes.md` | 骨架 | — | — |
| 15 | `15-open-close.md` | 骨架 | — | — |
| 16 | `16-read-write.md` | 骨架 | — | — |
| 17 | `17-pipe.md` | 骨架 | — | — |
| 18 | `18-mount.md` | 骨架 | — | — |
| 19 | `19-device-map.md` | 骨架 | — | — |
| 20 | `20-bdev.md` | 骨架 | — | — |
| 21 | `21-cdev.md` | 骨架 | — | — |
| 22 | `22-sdev.md` | 骨架 | — | — |
| 23 | `23-select.md` | 骨架 | — | — |
| 24 | `24-socket.md` | 骨架 | — | — |
| 25 | `25-exec.md` | 骨架 | — | — |
| 26 | `26-coredump.md` | 骨架 | — | — |
| 27 | `27-link.md` | 骨架 | — | — |
| 28 | `28-stadir.md` | 骨架 | — | — |
| 29 | `29-protect.md` | 骨架 | — | — |
| 30 | `30-fcntl-lock.md` | 骨架 | — | — |
| 31 | `31-misc-queries.md` | 骨架 | — | — |
| 99 | `99-global-concepts.md` | 骨架 | — | — |

### 6.2 改写优先级

1. **01 → 03 → 09 → 08 → 10**（启动 + 进程模型 + 主循环 + worker + PM 协议）：VFS 的骨架语义，其余文档的前置
2. **02 → 04 → 05 → 06 → 07**（结构 + 核心表 + 锁）：数据结构底座
3. **11 → 12 → 13 → 14 → 15 → 16 → 17**（FS 通信 + 路径 + fd + 文件 I/O）：主要服务面
4. **18 → 19 → 20 → 21 → 22**（挂载 + 设备）：设备 I/O 面
5. **23 → 24 → 25 → 26 → 27 → 28 → 29 → 30 → 31**（select/socket/exec/链接/杂项）
6. **99 → 00**（全局概念与总览收尾）

---

## 7. Review 记录

> 本节记录 plan.md 自身的 review 过程（深度 review + minix3 回归 review），与最终 plan.md 同文档交付，保证"覆盖完整性核对"可追溯。

### 7.1 深度 review（2026-08-16）

**方法**：对照 `02-stage-vm/plan.md` + `04-stage-pm/plan.md`（已定稿范本）+ `01-stage-kernel` 讲述结构，逐节审查：主线时序准确性、语义模块拆分合理性、覆盖完备性、ARCH 项与 Rust 现状一致性、边界声明完整性、编号顺序无前向引用。

**证据**：

```bash
# VFS 启动链核对
rg -n 'sef_cb_init_fresh|VFS_PM_INIT|worker_init|init_dmap|init_smap|init_vnodes|init_vmnts|init_select|init_filps|do_init_root|mount_pfs|mount_fs' minix3/minix/servers/vfs/main.c minix3/minix/servers/vfs/mount.c
# 主循环分发面核对
rg -n 'IS_VFS_FS_TRANSID|service_pm|is_notify|IS_BDEV_RS|IS_CDEV_RS|IS_SDEV_RS|handle_work' minix3/minix/servers/vfs/main.c
# 调用面核对
rg -n 'CALL\(' minix3/minix/servers/vfs/table.c
# PM 协议面核对
rg -n 'VFS_PM_' minix3/minix/include/minix/com.h
# Rust 现状核对
rg -n 'VfsCallNum|WorkerState|BlockedOn|VfsState|PmMessageType' os/servers/vfs/src/*.rs os/servers/vfs/src/ipc/*.rs
```

**结论与修复**：

| # | 发现 | 等级 | 修复 |
|---|------|------|------|
| D-1 | 旧编号把 filp/vnode/vmnt 表（结构）排在 tll/worker 之前，但 tll 是 filp/vnode/vmnt 锁的基础（`vnode.h:VNODE_*` 直接映射 TLL_*） | P1 | 新编号 tll（07）前置，结构表（04~06）后置，消除前向引用 |
| D-2 | 旧编号 `17-filedes` 只覆盖 close 路径，`get_fd`/`check_fds`/`do_copyfd`/`invalidate_*` 缺席 | P1 | 14-filedes 完整覆盖 fd 表操作面（§5.3） |
| D-3 | `service_pm`（main.c:764）与 pm_fork 拆成 8 篇（11 + 12~16 + 19），且 pm_exec/pm_setuid 等 9 个 VFS_PM 请求完全缺席 | P1 | 10-pm-protocol 合并全部 12 个 VFS_PM 请求（§5.3） |
| D-4 | 设备面（dmap/smap/bdev/cdev/sdev）在旧文档中完全缺席（draft 仅 `09-globals-const` 提及常量） | P1 | 19~22 四篇新建，覆盖 2,600 行设备代码 |
| D-5 | select/socket/exec/mount/path/request 六个大语义面（合计 ~6,000 行）完全缺席 | P1 | 12/13/18/23/24/25 新建 |
| D-6 | 主循环分发面未含 `table.c:call_vec` 的 64 调用与 bdev/cdev/sdev reply 三路分流 | P1 | 09-main-loop 含 call_vec + 主循环全分发（§1.2 时序图 + §5.3） |
| D-7 | `utility.c` 五个函数未定位（copy_path/fetch_name/isokendpt_f/in_group/sys_datacopy_wrapper） | P1 | 分别归 13/13/03/29/99（§5.1 说明） |
| D-8 | `filedes.c` 按文件整篇归一篇会导致"结构表（04）+ fd 表（14）"两篇都依赖它，边界不清 | P1 | 按函数拆分：init/find/lock 族归 04，get_fd/close/copyfd/invalidate 族归 14（§5.1） |
| D-9 | `misc.c` 1006 行按文件整篇归一篇会超载 | P1 | 三向拆分：pm_* 归 10、do_fcntl 归 30、其余归 31（§5.1） |
| D-10 | 缺每篇"前置依赖/职责/不覆盖"边界声明，写作时易内容交叉 | P1 | §3.4 边界表（33 篇全列） |
| D-11 | 无测试基线，§测试 无法对账 | P2 | §3.5 实测基线 35 passed / 0 failed |
| D-12 | 计数核对：C 文件 33、本地头文件 15、调用 64、VFS_PM RQ 12、REQ_ 35 | P2 | 全部 grep 实证（§7.2），标题与 §5 数据一致 |
| D-13 | `do_getrusage` 已废弃（table.c 注释 obsolete）但保持注册，需标注 | P2 | §5.4 排除表标注（31 保持外部行为） |
| D-14 | `REQ_GETNODE`/`req_getnode` 死导出需显式排除 | P2 | §5.4 排除表标注（12） |
| D-15 | select 阻塞状态与 pipe 的 suspend 机制重复（都涉及 fp_blocked_on 与回复路径），需声明边界 | P2 | §3.4：17 管 pipe 专用阻塞，23 管 select 专用，通用状态机归 02/09 |
| D-16 | ARCH A-1（执行模型）是 VFS 最大架构差异，需显式章节而非散见注释 | P1 | §4 A-1 + 08 写作要求显式章节 |

### 7.2 minix3 源码回归 review（2026-08-16）

**方法**：对 `minix3/minix/servers/vfs/` 全部 33 个 .c + 15 个本地 .h 逐一 grep 核对 §5.1/§5.2 映射表，并逐函数核对 §5.3 函数级清单。

**证据**：

```bash
ls minix3/minix/servers/vfs/*.c                      # 33 个文件，与 §5.1 表一致
wc -l minix3/minix/servers/vfs/*.c | tail -1         # 16,735 行
ls minix3/minix/servers/vfs/*.h                      # 15 个本地头文件，与 §5.2 表一致
rg -n '^int do_|^void pm_|^void mount_|^int mount_|^void worker_|^int worker_|^struct worker_thread \*worker_|^void cdev_|^int cdev_|^void bdev_|^int bdev_|^void sdev_|^int sdev_|^void select_|^int select_|^void tll_|^int tll_|^void lock_|^int lock_' minix3/minix/servers/vfs/*.c   # 函数面核对
rg -n 'CALL\(' minix3/minix/servers/vfs/table.c      # 64 个调用注册核对
rg -n 'VFS_PM_' minix3/minix/include/minix/com.h      # VFS_PM_* 12 RQ + 11 RS 核对
rg -n '#define REQ_' minix3/minix/include/minix/vfsif.h  # 35 个 REQ_* 协议面核对
rg -n 'VFS_TRANSID|IS_VFS_FS_TRANSID|TRNS_' minix3/minix/include/minix/com.h minix3/minix/include/minix/vfsif.h  # transid 核对
```

**结论**：
- 33 个 .c 全部映射到新文档，无遗漏；15 个本地头文件 + 4 个外部头文件（callnr/com/vfsif/dmap）全部进入头文件覆盖表。
- `table.c` 注册的 64 个调用（VFS_READ~VFS_SHUTDOWN）逐一落到 09（分发表）与 14~31（各 handler）：
  - 文件操作族（READ/WRITE/LSEEK/OPEN/CREAT/CLOSE/PIPE2/GETDENTS）→ 15/16/17
  - 名字族（LINK/UNLINK/RENAME/SYMLINK/READLINK/MKDIR/MKNOD/CHDIR/FCHDIR/CHROOT）→ 13/27/28
  - 元数据族（STAT/FSTAT/LSTAT/CHMOD/FCHMOD/CHOWN/FCHOWN/UMASK/ACCESS/TRUNCATE/FTRUNCATE/UTIMENS）→ 28/29/31
  - 挂载族（MOUNT/UMOUNT/STATVFS1/FSTATVFS1/GETVFSSTAT）→ 18/28
  - 控制族（IOCTL/FCNTL/SELECT/SYNC/FSYNC/VMCALL/COPYFD/MAPDRIVER/GETSYSINFO/SVRCTL/GCOV_FLUSH/GETRUSAGE）→ 19/23/30/31
  - socket 族（SOCKET/SOCKETPAIR/BIND/CONNECT/LISTEN/ACCEPT/SENDTO/SENDMSG/RECVFROM/RECVMSG/SETSOCKOPT/GETSOCKOPT/GETSOCKNAME/GETPEERNAME/SHUTDOWN/SOCKETPATH）→ 24
- 12 个 VFS_PM 请求（INIT/SETUID/SETGID/SETSID/EXIT/DUMPCORE/EXEC/FORK/SRV_FORK/UNPAUSE/REBOOT/SETGROUPS）全部落入 01/10/26。
- 35 个 REQ_* 协议面全部落入 11/12（REQ_GETNODE 标注排除）。
- ARCH 项（A-1~A-15）与 minix3 现状对照成立；A-5/A-7 显式标注为未实现缺口（fail-closed/defer 契约），A-1/A-2/A-3/A-4/A-8 与 `os/servers/vfs/src/` 现状一致。
- **覆盖完整性通过**：无 VFS 语义（函数/调用/协议/设备面）遗漏。

### 7.3 写作前置设计决策：worker 抽象与 SUSPEND 语义契约（2026-08-16）

**决策 1（08/09）**：Minix3 的 worker 线程（`worker.c`，mthread 真实线程）在 minix-rs 中降为**请求槽状态机**（`worker.rs:WorkerState`），阻塞 I/O 由"挂起请求 + 回复队列"建模。主循环 `get_work` 与 `do_work` 的"spawn thread"语义改写为"取空闲请求槽 → 执行 handler → 结果入回复队列"。FS 异步回复（`do_reply`）改写为"按 transid 找到挂起请求槽 → 恢复执行"。

**决策 2（09/17/23/22）**：VFS 的 `SUSPEND` 语义（pipe/select/cdev/sdev 四条回复路径：`pipe.c:revive`、`select.c:select_return`、`cdev.c:cdev_reply`、`sdev.c:sdev_finish`）在 Rust 中建模为显式回复意图枚举：

```rust
enum ReplyIntent { Reply(i32), ReplyLater, NoReply }
// ReplyLater = C 的 SUSPEND：本次主循环不回复，稍后由 revive/驱动回复路径回复
```

**依据**（grep 实证）：主循环 `handle_work(do_work)` 后按 `do_work` 内部 `reply()` 路径回复（main.c:263-302），`SUSPEND` 的接收方是 pipe/select/驱动阻塞路径，四条恢复路径必须在 17/23/21/22 分别建模，通用枚举归 09。

**同步**：plan.md §2 08/09/17/23 行、§4 A-1/A-5、§3.4 边界表、后续各文档 §Rust 设计决策 四处一致。

---

## 8. 参见

- `draft/` — 旧主线全部素材（README 索引 + 21 篇旧文档 + 6 篇早期素材）
- `../01-stage-kernel/00-kernel-overview.md` — 讲述结构参照（阶段划分/组织原则/过渡章节）
- `../01-stage-kernel/18-syscall-copy.md` — 内核 safecopy（VFS 数据拷贝交叉参照）
- `../02-stage-vm/plan.md` — 同型重组范本（plan 结构/ARCH 清单/覆盖契约模式）
- `../04-stage-pm/plan.md` — 同型重组范本（PM 协议对端状态机）
- `../04-stage-pm/05-vfs-interaction.md` — PM 侧 VFS 协议（对端状态机）
- `../00-master-plan/08-phase4-vfs-guide.md` — 早期 VFS 实现指南（素材）
- `minix3/minix/servers/vfs/` — C 源码（ground truth，33 .c + 15 .h）
- `minix3/minix/include/minix/vfsif.h` — REQ_*/TRNS_* 协议面
- `minix3/minix/fs/` — 底层 FS 服务端（VFS 对端，交叉参照）
- `os/servers/vfs/src/` — Rust 实现（fproc/worker/call_table/main_loop/ipc）
