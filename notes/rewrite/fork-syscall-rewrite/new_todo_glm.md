# new_todo_glm.md — 三架构 QEMU 启动 minix-rs 并运行 18-stage-commands 全部程序：剩余实现逻辑清单（glm 独立扫描）

> **来源**：2026-09-20 glm 独立扫描（代码 + todo 双路，与另一 AI 的 new_todo_* 相互隔离，互不参考）。
> **目标对齐**：`edge4.md` §7 最终验收阶梯——"目标：三架构在 QEMU 跑起来并执行 18-stage cmds"。本文件是 T2~T5 未竟工作的**目标导向**清单；跨 stage 既有条目（E1/E2/E5/E-FSBDEV/E-FSRUNTIME/E-INITSYS 等）已在 [edge_todo.md](./edge_todo.md) 登记，本文件**只引用不重述**，两者冲突时以 edge_todo.md 为准。
> **执行纪律**：每条锚点按 fix-guard 重读目标行 ±5 行核实后再动工——本清单行号是扫描时点快照（模式 66 RCPD 自证过锚点会漂移）。标注 ⚓ = 本人（glm）直接 grep/sed 实证；标注 🔍 = 探查子代理报告、未逐行复核（动工前先自行核实）。
> **优先级约定**：P0 = 目标链路上的硬阻塞（先确保有）；P1 = 目标达成后的补齐项；P2 = 二优先级（保真度缺陷 / translate 漂移 / 代码 bug，确保好）。

---

## §1 目标链路总览（谁阻塞谁）

```
QEMU 三架构启动 (T0 已达成)
  → 多进程 boot 载体（C-29 硬阻塞）
  → boot 镜像组装面（xtask 骨架 + loader 模块清单 + /etc 装机）
  → RS 通电（boot tab 静态序列可跑）→ PM 通电（主循环已真）
  → VFS 真挂载根（do_init_root 空壳）+ mfs 真块源（恒 EIO）
  → exec 文件链（VFS exec worker 桩 + PM do_exec 门错位【bug】）
  → 页故障完整回路（trap_dispatch 用户异常臂 panic——任何 CoW/堆增长即内核死）
  → console 输出（tty NullBackend 丢字节——echo 的最后一公里）
  → init 真跑 /etc/rc（init 已真、盘上 /etc 全缺）
  → 18-stage 命令（65 bins 宿主 std 态；no_std 切换 + 缺失 syscall 面 + sh/ls/cat 未接线）
  → 三架构复跑 + SMP 正确性（T5）
```

当前真机最远点：x86_64 `test-sysboot` 载体 boot 到 smp_init、VM 槽**单**镜像装载验证通过；第二镜像撞 C-29。riscv64/aarch64 各自的单用户诞生链（rt-birth）三标记 PASS，但用户↔内核 trap 腿活在测试载体里、不在内核。

---

## §2 P0 硬阻塞清单（按依赖波次）

### W1 内核可运行面（任何用户程序的前提）

#### G-P0-1 ⚓ 用户态页故障/异常转发臂未接线（S-6/S-7）——页故障回路断在最后一跳
- **现状**：内核把用户 #PF 已正确分类为 `ForwardToVm`（`os/kernel/src/arch/exception_dispatcher.rs:176-215`），`page_fault.rs` 的 `set_pagefault_pending`/`build_vm_pagefault_msg`（:70/:203）已备；VM 侧完整回路已真（`vm/src/vm_server.rs:1341-1367` → CoW/PTE 直写 → `sys_vmctl` ClearPageFault）。但 `os/kernel/src/trap_dispatch.rs:223-250` 的用户源 outcome 臂直接 **panic**（自注 "needs per-CPU process context (S-6/S-7)"）。rt-birth 能过只因 `load_vm_elf` 预物化全部页面——任何真实程序一旦 CoW/堆增长/按需页即内核死。
- **缺口**：① trap 路径捕获 per-CPU 当前进程上下文（faulting endpoint 填充）；② `ForwardToVm(VmPagefaultIn)` 臂发 VM_PAGEFAULT 消息；③ `ExceptionOutcome::Signal` 臂（exception_dispatcher.rs:220-243 分类已备，无内核消费者——用户态段错误变信号的路径）。
- **验收**：`test-paging-faultloop`（E5(d) 载体，K17 已入 run_all）真机 PASS。
- **解锁**：一切用户程序的内存故障安全网；E5(d) 闭单。

#### G-P0-2 🔍→⚓ C-29：boot loader 的 per-process 地址空间/栈放置（多进程 boot 硬阻塞）
- **现状**（本人复核 edge4.md:61 登记）：`os/arch/src/arch/boot.rs:471` `load_vm_elf` 栈顶取**全局** `kernel_info.user_sp()`——所有 boot 镜像共享同一 64 KiB 栈窗口；`os/kernel/src/lib.rs:1112/1288/1372` 仅 VM 分支真装载，其余 boot 模块 `EntrySpec::DEFERRED`（:1449-1455）。真机实证：test-sysboot 第二镜像撞 RX 已占栈窗口 → `MappingFailed`。C 对位 `arch_boot_proc`（protect.c:388）为每个 boot 进程建独立地址空间。
- **修法候选**（裁决待定）：① `load_vm_elf` 增 per-process `stack_high` 参数（快，但共享根下段面互撞仍在）；② boot loader 建 per-process 根（C 对位，动 kernel/arch 两层，**设计裁决先行**）。
- **配套**：`test-sysboot` 载体 + sysboot-rx/tx 载荷 + 判定脚本已入库即插即跑；PASS 后接 `os/qemu-tests/run_all.sh`（edge4 登记流水 2026-09-20 行）。
- **解锁**：C-27 全系统自举载体（kernel → VM/RS → 服务集 → init）；T2 全链。

#### G-P0-3 🔍 riscv64/aarch64 内核 trap 派发体缺失（三架构门槛余件）
- **现状**：两架构 `trap_vector` 只是诊断 stub（`os/arch/src/riscv64/trap_entry.rs:55-63` 捕 scause/stval 打印即 halt；`os/arch/src/arm64/trap_entry.rs:58-65` 下半 EL 槽 `exc_bad_mode`）。rt-birth-riscv64/aarch64 的用户 trap 腿由**测试载体自带**（test-rt-birth-riscv64/src/main.rs:14-26 自装 U-ecall handler；aarch64 载体自带 VBAR_EL1 表）——内核本体无用户↔内核往返。
- **缺口**：完整帧保存 + sscratch/SP_EL0 交换 + cause 分发 + 内核态定时器/IPI IRQ 腿的真机验证（`notes/TODO.md` QEMU backlog：L4 异常交付、L5 中断交付链均"单元层 ✅，QEMU E2E 未实现"）。
- **配套**：新增 `test-timer-irq-{riscv64,aarch64}`（x86 已有 test-timer-irq，PASS）。
- **解锁**：T1 收尾；T5 三架构复跑的前提。

#### G-P0-4 🔍 minix-rt 堆只有静态 64 KiB 池，无 VM 页供应商
- **现状**：`os/libs/minix-rt/src/alloc.rs`（`GLOBAL_POOL_BYTES = 16*4096`，lib.rs:121-136）——分配超限即 OOM；`PageSupplier` trait 文档自注 "planned for later"。VM 侧客户端通道已备（`minix-sys/src/vm.rs:246` `break_via`、mmap 族）。
- **缺口**：VM-backed PageSupplier 生产实现（brk/mmap 通道向 VM 要页）。服务进程（PM proc 表、VFS filp 表）与多数命令的分配硬前置。
- **裁决点**：按 E-BOOTFRAME/rt-birth 判例，走 `VM_BRK` taskcall 还是 mmap 通道，设计先行。

#### G-P0-5 ⚓ panic handler 自旋 + sigreturn trampoline 地址为 0
- **现状**：minix-rt panic handler 渲染后**自旋**（`os/libs/minix-rt/src/lib.rs:279-308`，A-8 step3 "经 PM 终止"未做）；PM 已存用户侧 sigreturn trampoline 地址（`pm/src/signal_handlers.rs:86-134`），但 init 侧 trampoline 填 0（`os/commands/sbin/init/src/host.rs:296-305`）——C libc 的 `__sigreturn` 对位物不存在。
- **缺口**：① panic → `minix_sys::exit`/PM 终止链；② minix-rt 提供真实 sigreturn trampoline（一小段 `sigreturn` 系统调用桩 + 符号导出），信号 handler 返回路径才闭环。
- **解锁**：init 五个 handler（transition/alrm/disaster/minixreboot/minixpowerdown，init.c:310-334 等）真实可用。

### W2 boot 镜像与装机面（S32 装机轨道）

#### G-P0-6 🔍 镜像组装面整层缺失（xtask 骨架 + loader 清单 + 命令产物）
- **现状**：`os/xtask/src/main.rs:123-142` 的 `image()`/`qemu()` 是 TODO 骨架；UEFI loader 期望 `/EFI/minix/kernel.elf` + `/EFI/minix/modules/{vm,pm,vfs,rs,ds,inet}`（`os/boot-shim/src/loader.rs:39-52`）——**清单里没有 init/tty/mfs/pfs**（:54 的模块数组）。没有任何产物构建出这套镜像。
- **缺口**：① xtask image 组装（kernel.elf + 模块 ELF 集 + per-arch 布局：x86_64 UEFI/OVMF、riscv64 SBI 直 `-kernel`、aarch64 UEFI/AAVMF）；② 模块清单扩 init/tty/mfs/pfs（RS boot tab 对应扩容）；③ 服务/命令 bin 的 `no_std` 目标交叉构建（见 G-P1-1）。
- **解锁**：T2 载体有真实镜可装；T4 根镜像（S32）的前置。

#### G-P0-7 ⚓ /etc 盘上内容全缺（rc 链的最后输入）
- **现状**：`os/etc/` 只有 README.md（rc/rc.conf/fstab/passwd/TTYS 全部"未实装"自白）。init 侧消费面已完整（`runcom.rs:26` `RUNCOM_SCRIPT="/etc/rc"`、rc_argv 构建 `sh /etc/rc`、runetcrc fork/exec/waitpid 全链 :115-201）。
- **裁决已定**（S42 批五注记）：OQ-3 裁走盘上文件面 → 归 S32 装机面。载体 = mkfs_mfs 原型文件填充（diskfmt 的 mkfs_mfs 已真实，含 prototype 填充 + mkfs→fsck 往返冒烟）或 diskimg 装机。
- **缺口**：/etc/rc（服务启动序列，对照 minix3/etc/rc 的最小子集）、/dev/console 设备节点、TTY 表、passwd。内容最小化即可（不追求 C 全量 rc）。
- **解锁**：T3 init 真跑；login/getty 后续。

### W3 用户态服务器通电链（T2）

#### G-P0-8 🔍 VFS 根挂载是空壳（do_init_root 不发 IPC）
- **现状**：`os/servers/vfs/src/main_loop.rs:517-540` 只翻 worker 门，真 `mount_pfs()`/`mount_fs()`（req_readsuper → mfs on DEV_IMGRD）DEFERRED 到 18-mount W 批次（:528-534 注释自认）。VFS 主循环/PM_INIT 握手/64 臂分派/FS 对话冲刷均已真。
- **缺口**：18-mount 批次的根挂载执行半——req_readsuper 真发 REQ_READSUPER 给 mfs、根 dmap/fproc 装配。
- **解锁**：一切文件命令（exec 读 /bin/*、open /dev/console）的前提。

#### G-P0-9 🔍 mfs 块源恒 EIO（PendingBlockSource）
- **现状**：`os/fs/fs-rt/src/source.rs:19-33`——每个块读写答 EIO（E-FSBDEV 登记的生产缺口）；mfs 能启动/握手（V1-P0-1 已闭环，main.rs:26-47 经 `minix_fs_rt::serve`）但读不到任何 superblock。
- **缺口**：**最小半** = imgrd 内存盘 BlockSource（boot 传入内存盘基址/尺寸 → 内存读写）；真块驱动半（minix-bdev/blockdriver）沿 E-FSBDEV 既有条目，不在本目标关键路径。
- **解锁**：G-P0-8 的对端；根文件系统可读。

#### G-P0-10 🔍 VFS exec worker 是桩（exec 文件链的 VFS 半）
- **现状**：`os/servers/vfs/src/ipc/dispatcher.rs:200-215` `VfsCall::Exec` 校验端点后**原样回 status=0/pc=0/newsp=0**，不读 ELF（C `minix3/minix/servers/vfs/exec.c` 的 read_header/载入段 worker 未移植）。wire 已备（`minix-types/src/ipc/vfs.rs` VfsCall::Exec/Reply，0x906/0x986）。VM→kernel 侧（exec_bootproc 的 PT_LOAD walk + sys_exec、栈 ABI）已真——VFS 供出真实 entry/stack 即通。
- **缺口**：VFS exec worker：open 文件 → read_header（#!脚本 ESCRIPT 分支可后置）→ ELF 段装载（经 VM exec 面或 datacopy）→ PM_EXEC_NEW 载荷。C 锚：minix3/minix/servers/vfs/exec.c。
- **解锁**：`execve("/bin/echo")` 全链；sh/命令的一切文件执行。

#### G-P0-11 ⚓【bug，二优先级但阻塞 T3】PM do_exec 调用者门放错位置
- **C 真值**（本人实读）：`do_exec`（minix3/minix/servers/pm/exec.c:38-56）**无任何调用者门**——用户 execve 到 PM 后无条件 tell_vfs 转发 + SUSPEND；`VFS|RS` 门属于 `do_newexec`（exec.c:70-71 `if (who_e != VFS_PROC_NR && who_e != RS_PROC_NR) return EPERM;`）。
- **Rust 现状**：`os/servers/pm/src/exec.rs` 的 `do_exec` 把 VFS|RS 门放在转发前（"C exec.c:70-71：exec 只能由 VFS 或 RS 发起"的注释把两个函数的语义缝在一起了）。后果：init 以自己端点调 execve → PM do_exec → **EPERM**，/etc/rc 链第一步即死。
- **修法**：do_exec 删门（无条件 forward_exec + SUSPEND），门保留在 ExecNew/ExecRestart 臂（对位 do_newexec/do_execrestart）。属正确性修复，走 todo-fix 三段式，不随本清单顺手修。

#### G-P0-12 🔍 RS 执行半与 fail-close 家族（非首位但有登记义务）
- **现状**：RS main 真（BootTables::acquire_from → GET_MONPARAMS → init(Fresh) → run，boot tab 静态序列 + SYNCH_BOOT 已修）；但 `rs/src/dispatch.rs:152-164` 对 RS_DOWN/REFRESH/RESTART/SHUTDOWN/CLONE/UNCLONE/UPDATE/SYSCTL/FI/GETSYSINFO/LOOKUP 全部 fail-close ENOSYS；`rs/src/exec.rs:8-31` 无 read_exec（C manager.c:1372-1420），服务从盘加载不可用（等 VFS 通电）。
- **目标内最小需求**：boot tab 静态序列已够 init 之前的链；read_exec 挂 VFS 通电后（19 篇）。登记在此防漏，不新增条目。

#### G-P0-13 🔍 tty 输出后端是 Null（console 最后一公里）
- **现状**（本人实读 backend.rs 头注）：`os/drivers/tty/tty/src/lib.rs:46-47` 默认 `NullBackend`——`DEV_WRITE` 到达 tty 的字节**被丢弃**；行不保留输出字节缓冲。候选通道三选一：串口（driver 进程无 port I/O wrapper、I/O 特权归系统任务）、video-text（需活 mem server + 字节拷贝）、内核 diagctl（debug 通道非数据面）。
- **缺口**：① 输出通道设计裁决（建议：QEMU 目标下串口最直接——需要给 driver 进程一个受控的 port I/O / MMU 映射面，或经系统任务的 sys_vout 对位）；② 行输出缓冲 + backend 字节管道；③ `/dev/console` cdev 路径联调（VFS cdev → DEV_WRITE → tty）。
- **解锁**：echo/ls/cat 在 QEMU 里有可见输出；E5 输入链的输出半。

#### G-P0-14 🔍 PM exec/misc 批次 + Reboot 臂
- **现状**：PM 主循环/fork/exit/wait/信号已真；`04-stage-pm/todo.md` §11.1.1 批次表中 **E（exec wire）/ G（misc）** 未接——exec 链 PM 侧载荷（ExecNew/ExecRestart 已有骨架，与 VFS worker 对接时装配）；`PmCall::Reboot`（37）落入 catch-all ENOSYS（init 的 minixreboot/minixpowerdown 需要，C do_reboot 在 20-misc-queries 域）。
- **缺口**：批次 E/G 的接线 + Reboot 臂（可最小化：sys_abort 等价）。

### W4 init 真跑（T3）

#### G-P0-15 🔍 init 残余 ENOSYS 面
- **现状**：init 主循环/信号注册/会话/fork-exec-wait 全真（P0-1 完成，`driver.rs:203-214` `-> !` 循环；no_std 翻转完成 7147b945c）。残余：securitylevel live 半（`host.rs:288` ENOSYS）、init_root（`host.rs:292` ENOSYS——等 G-P0-8 根挂载）、trampoline addr=0（归 G-P0-5）。
- **缺口**：securitylevel 的 PM/sysctl 对位臂核实后接；init_root 随 VFS 挂载自然解锁。

---

## §3 三架构专项（T5 复跑的差距）

| # | 项 | 现状 | 缺口 |
|---|---|---|---|
| G-A1 🔍 | riscv64 trap 派发体 | 诊断 stub（riscv64/trap_entry.rs:55-63） | 同 G-P0-3；SSIE IPI 已绿（aclint=on），trap 体是下一块 |
| G-A2 🔍 | aarch64 trap 派发体 | 下半 EL `exc_bad_mode`（arm64/trap_entry.rs:58-65） | VBAR_EL1 内核表全量 + SP_EL0 交换 + 用户往返 |
| G-A3 🔍 | 两架构 timer IRQ 真机测试 | 无 test-timer-irq-{riscv64,aarch64}（notes/TODO.md L5 行） | 新增 QEMU 载体；CLINT/Generic Timer 驱动已写（各 ~150 行）但无真机中断交付验证 |
| G-A4 🔍 | 命令/服务 bin 交叉构建 | 65 bins 全 x86_64 宿主 std | `riscv64gc-unknown-none-elf` / `aarch64-unknown-none` 目标构建矩阵（E5-ARCH，edge4.md:123）；crt0 三架构 `_start` 已备（minix-rt/src/crt0.rs:337/360/381）——理论就绪，从未批量构建过 |
| G-A5 🔍 | riscv64 PMP | 仅 entry 0（allow-all） | notes/TODO.md 登记的已知限制；单核启动不阻塞，T5 收尾前补 |
| G-A6 ⚓ | workspace 全量 `--bins` 构建断裂 | `cargo build --workspace --bins` 死于 test-shutdown-riscv64（riscv64 符号泄入宿主 + duplicate panic_impl） | 交叉目标泄漏修复——非命令问题，但会打断装机面批量构建（见 §4 P2-B4） |

---

## §4 二优先级：保真度缺陷 / translate 漂移 / 代码 bug 扫描（先确保有，再确保好）

> 项目目标一直是 rewrite not translate。以下为本次扫描实锤的偏差；修复一律走 full-review/todo-fix，不在启动链工作中顺手修（fix-guard 纪律）。

| # | 严重度 | 项 | 证据 | 说明 |
|---|---|---|---|---|
| B1 | P0-code-bug | PM `do_exec` 门错位（见 G-P0-11） | ⚓ minix3/minix/servers/pm/exec.c:38-56 vs os/servers/pm/src/exec.rs | C do_exec 无门；门属 do_newexec（:70-71）。阻塞 init execve——虽列二优先级，实际是 T3 硬阻塞，两清单都登记 |
| B2 | P1 | C-28：srv_fork 子进程 PRIV_PROC 保留缺失 | 🔍 edge4.md C-28 行（2026-09-20 登记，☐ 待认领） | `mproc/fork.rs` 落 `Privilege::User`，C forkexit.c:199-200 保留 PRIV_PROC；影响 is_kernel_process 四处判据（退出/致命信号/事件订阅/调度接管）。修法需 `Privilege::Kernel` 承载凭证的 PM 类型设计裁决 |
| B3 | P1 | 边界守卫回归：`tools/check-command-boundary.sh` 40 violations | ⚓ 实跑 FAIL | C-7 曾"全绿"（2026-09-17）后回归。两类：① termctl stty 测试代码直用 `minix_types::Termios/ONLCR...`（30+ 处，均在 `#[cfg(test)]`——守卫是否豁免测试代码需裁决）；② `sbin/init/src/host.rs:30`、`execve.rs:46`、`games/text-games/src/bin/random.rs:9` 直用 `minix_sys::ipc::DirectTrapTransport`——命令不允许触 IPC 层，缺顶层封装则按守卫提示登记 14-stage |
| B4 | P1 | workspace `--bins` 全量构建被交叉目标泄漏打断 | ⚓ 实测 test-shutdown-riscv64 unresolved + duplicate panic_impl | 测试内核 bin 的 target 泄入宿主构建图；`cargo build -p {crate} --bins` 不受影响 |
| B5 | P2 | 06-file-ops.md:182 文档漂移 | 🔍 子代理报告 | cat 阻塞原因仍写 "open 现有路径 ENOSYS（lib.rs:189）"——open_existing_via 早已真实（99 篇路径布局定稿时落地）；cat 真实阻塞只剩 no_std/exec 交付链 |
| B6 | P2 | L4/L5 QEMU E2E 未验证 | ⚓ notes/TODO.md QEMU backlog 表 | 异常/中断交付链只有单元层；归 G-P0-3 配套载体 |
| B7 | P2 | E-THREAD-MODEL 挂起 | 🔍 18-stage todo §6.0 表 | parked 14/04-stage；不阻塞本目标（单线程事件循环已够） |

**主动核查过、确认不缺的**（防第二个 AI 重复登记）：E-SYSCALL-SIGN 双腿已修（`perform_syscall` + `perform_taskcall` 均 transport 失败短路 Err，⚓ syscall.rs:100-112/:135-139）；E-CMDSYSFACE ①errno 再导出（⚓ minix-sys/src/lib.rs:36）/②argv-env 访问器（⚓ minix-rt crt0.rs:129-200）均已落地；PM 信号/uid 族 match 臂已全接（⚓ calls.rs:390-603 逐一在位）——**edge_todo.md E-INITSYS 进度注记（2026-09-18）中"PM dispatch 缺信号/uid 族 match 臂"已过期**；边界守卫 C-2/C-3 条目代码事实已变。

---

## §5 18-stage-commands 视角：到"全部程序可运行"还差什么

**现状基线** ⚓：24 域 crate / 65 个 thin bin（`find os/commands -path "*/src/bin/*" | wc -l` = 65）全部宿主可构建；约 45k 行 + 1025 测试。对照 minix3 原树 328 命令，接线率约 20%。

| 层 | 缺口 | 关联条目 |
|---|---|---|
| 构建 | no_std 目标切换（echo 式双 seam 模式已立：echo.rs:11-23 先 std 后 minix-rt） | G-A4 |
| libc 面 | minix-sys 顶层缺失（本次 grep 零命中）：`pipe/pipe2`（sh 管道硬阻塞）、`mkdir/rmdir/chmod/chown/link/unlink/rename/mknod/truncate/utime`（06 篇文件家族）、`getcwd`、`brk/sbrk` 顶层；`getpid/dup2/lseek/chroot` 等 module-only 需顶层化。**登记归 14-stage-runtime 轨道（E-CMDSYSFACE 先例），18 侧不越界** | G-P1 |
| 服务器面 | 文件命令依赖 G-P0-8/9/10（挂载/块源/exec）；终端命令依赖 G-P0-13；进程命令（ps/kill）依赖 MIB proc_tab 通电（E-MIBPROD 已登记） | §2 W3 |
| 接线长尾 | sh 执行器未开工（bin/shell 仅 lexer/parser 36 测试，批 26 §3.5 计划在案；前置 pipe2 + G-P0-10）；06-file-ops 26 个 bins 未接（cat/cp/mv/rm/ln/ls/mkdir...）；term-games 7 个待 raw 终端模式；T4 判定命令 = **echo（已接）/ ls / cat / sh**，优先接线这三个 | G-P1 |
| 验收 | 18-stage 命令冒烟脚本（QEMU 内执行，断言输出）——挂 S32 装机面 + G-P0-13 输出通道后才有可断言物 | edge4 §7 T4 判定行 |

---

## §6 建议执行序（glm 视角，一次一条，沿 todo-fix 三段式）

1. **W1 先行**（不依赖任何服务器）：G-P0-1 页故障转发（载体已在 run_all）→ G-P0-5 panic/exit+trampoline → G-P0-4 页供应商。
2. **G-P0-2 C-29**（设计裁决先行：per-process 根 vs stack_high 参数）→ test-sysboot 双镜像 PASS → run_all 接线（C-27 收口）。
3. **G-P0-3 三架构 trap 体** + timer-irq 载体（可与 2 并行，独立载体）。
4. **W2 装机面**：G-P0-6 xtask image + loader 清单扩容 → G-P0-9 imgrd 内存盘 BlockSource → G-P0-7 /etc 内容（mkfs_mfs 原型填充）。
5. **W3 通电链**（x86_64 先行）：G-P0-8 VFS 根挂载 → B1/G-P0-11 PM do_exec 门修复（ todo-fix）→ G-P0-10 VFS exec worker → G-P0-13 tty 输出通道（设计裁决：串口 vs video-text）→ G-P0-12/14 RS read_exec + PM 批次 E/G/Reboot。
6. **W4**：G-P0-15 init 残余面 → QEMU 内 init 进 multi-user 雏形（T3 判定）。
7. **W5**：G-P1-1 no_std 切换 → T4 判定命令（ls/cat/sh）接线 + 冒烟脚本 → minix-sys 顶层 API 登记批（pipe2 等，归 14-stage）→ B3 守卫回归修复。
8. **T5**：G-A1~A5 三架构复跑 T2~T4 + SMP 正确性（E-VMTLB 的 SMP 冒烟用例在此收口）。

---

## §7 扫描方法与边界声明（glm）

- 输入：edge_todo.md 全文（1096 行）、edge4.md §7 阶梯表、notes/TODO.md、01-stage-kernel/18-stage-commands/09-stage-init 等 stage todo（经子代理）、os/ 全源定向 grep。
- 子代理四路：启动链 / 18-stage / 服务器链 / 内核缺口；其报告锚点标 🔍，本人复核过的标 ⚓。锚点行号会漂移（模式 66/77），执行前按 fix-guard 重读。
- 本文件不修改 edge_todo.md / 各 stage todo.md（并发写冲突禁令，edge_todo.md 执行约定行）；新发现的跨 stage 事实中，B1（do_exec 门）建议由权威文件认领时并回 edge_todo.md，本文件仅登记。
- 与另一 AI 产出对账时：§4 B1/B3/B5 及 §7"主动核查过"清单是本次独立实证的差异化内容，可作交叉验证点。
