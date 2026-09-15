# 跨 Stage Edge TODO（fork-syscall-rewrite）

> 来源：2026-09-06 V11 架构审查（02-stage-vm/todo.md §14）拆分出的跨 stage 条目。
> 2026-09-06 增补：04-stage-pm 架构审查（04-stage-pm/todo.md §9）拆分出的跨 stage 条目 E6-E7。
> 2026-09-06 增补：03-stage-rs 的 §18.10 E-11 生产接线面登记为 E9（KernelApi 五域面真实传输，E-2 拆分后的接线形态）。
> 2026-09-08 增补：02-stage-vm V12 轮（02-stage-vm/todo.md §17）登记 E-VMMCPWIRE（vmmcp wire 字宽必现截断）与 E-VMMOCK（G-V12-5 余件：minix-arch default features 收口），并在 E5 增补"缺页故障完整回路"验收面。
> 2026-09-09 增补：02-stage-vm V13 轮（02-stage-vm/todo.md §18）登记 E-VMTLB（kernel 侧目标进程 TLB 刷新机制缺失，真 SMP 正确性前提），并在 E-RSWIRE 增补 VM 侧三项落地要求（call_mask u64、RS_SET_PRIV 真掩码、五消息结构专属 wire struct）。
> 2026-09-09 增补：05-stage-vfs 第二轮架构审查（05-stage-vfs/todo.md §9）登记 E-REQWIRE（REQ_* VFS↔FS 共享契约双侧独立定义，含 FS_BASE 基址已分叉的事实），并在 E-VFSWIRE 增补 VFS 侧接收半要求与绝对值断言纪律。
> 2026-09-09 增补：06-stage-sched 第二轮架构审查（06-stage-sched/todo.md V2 §5）登记 E-SCHEDNICED / E-PREEMPTFLAG / E-SCHEDSMP / E-MINTYPES-SYS 四条，并在 E5 增补 (e) PM↔SCHED 联调验收面。
> 2026-09-14 增补：07-stage-ds 首轮架构审查（07-stage-ds/todo.md）登记 E-DSWIRE（minix-sys DS 客户端模块缺失 + DS 服务器 transport 通电 + 联调零覆盖，三缺一注册），并在 E-MINTYPES-SYS 增补 DS 段两件（SI_DATA_STORE / NOTIFY_MESSAGE 常量）、E5 增补 (f) DS 发布/订阅联调验收面。
> 2026-09-15 增补：10-stage-mib 首轮架构审查（10-stage-mib/todo.md §5）登记 E-RMIBWIRE（minix-sys rmib 客户端协议半整缺）、E-MIBPROD（MIB 快照 vs kernel/PM/VFS producer 布局对账）、E-MIBGRANT（kernel grant.rs 端点常量与 C 不符）三条，并增补 E-DSWIRE（mib_get_label 消费方）、E-ISWIRE（mib 为 minix-sef 第二消费方）、E5（(g) MIB/sysctl 联调验收面）、E-MINTYPES-SYS（DS 段现状更新）、E-MINSYS-HYGIENE（锚点复核）。
> 2026-09-15 增补：11-stage-devman 首轮架构审查（11-stage-devman/todo.md §6）登记 E-DMWIRE（devman 生产接线四缺：server transport + 请求分类器 + 装配半 + client/RS 侧生产传输）与 E-DMCLIENT（minix-devman-client 孤儿 crate 处置，涉 16-stage-drivers）两条，并增补 E-REQWIRE（devman 为 VTreeFS wire 第三消费方）、E-ISWIRE（devman 为 minix-sef 第三消费方）、E-DSWIRE（devman 客户端 init 的 DS label 查询消费方）、E5（(h) devman 生命周期联调验收面）。
> 定位：**跨 stage 边界条目的唯一入口**，后续单线程逐条执行，避免并发修改各 stage 的 todo.md 时发生冲突。
> Edge 判定规则（三类）：① 共享契约/基础设施层——minix-types 布局、minix-sys trap 层与 SYS_* wrapper、os/arch 的 pt_alloc；② 对方 stage 目录里的生产代码（如 kernel 侧填充 handoff 字段）；③ 多进程联调测试（QEMU 端到端）。
> stage 内生产代码（消费既有稳定契约，含 seam + mock 测试）**不属于** edge，在所属 stage 的 todo.md 内实施。
> 执行约定：一次一条；每条完成后在本文件标注状态与日期；涉及对应 stage 的条目同步回写其 todo.md（02-stage-vm 对应 V11 条目、04-stage-pm 对应 P/D 条目）。
> 2026-09-15 清理 campaign：已闭单四项（§0 表、E3、E-RSSTART、E-VMMCPWIRE）迁入 [edge_todo_archive.md](./edge_todo_archive.md)；同日全条目三路 grep 对账，过期锚点与已闭合子句已在各条目以「复核(2026-09-15)」修正——执行每条前仍须按 fix-guard 重新读目标行核实，不依赖本文件锚点的时效性。
> campaign 执行序（2026-09-15 批准）：依赖波次——A 内核正确性快修（E-MIBGRANT/E-PREEMPTFLAG/E-SCHEDNICED/E-VMTLB/E-DMCLIENT）→ B 共享契约收敛（E-MINTYPES-SYS/E-REQWIRE/E-MINTYPES-RS/E-FORKMSG）→ C minix-sys 客户端库（E2 / E-DSWIRE 余件+E-MINSYS-HYGIENE 同轮 / E6 / E-RMIBWIRE 纯函数层 / E-ISWIRE(1)）→ D 布局对账与 wire 系统化（E-ISPROD+E-MIBPROD / E7 / E-ISKMESS / E-KERNINFO）→ E VM 大件（E-BOOTFRAME / E-RSWIRE VM 侧 / E-VMMOCK 余件）→ F E1 trap 桥（设计先行，整条在本 campaign 做）与通电族（E8/E9/E-VFSWIRE/E-DSWIRE 通电/E-ISWIRE(2)(3)/E-DMWIRE/E5/收尾批）。每条一次一个，走 todo-fix 三段式（讲明白 → 多方案对比 Linux/Redox/OS 理论 → 实施）+ fix-guard + 文档-代码同步 + 测试 5 维自查 + 回归 review 后 commit；新发现的跨 stage 条目一律追加进本文件，不并发改各 stage 的 todo.md。

---

## 0. 已闭单条目(归档指针)

以下四项已闭单,完整原文(含判定过程)迁入 [edge_todo_archive.md](./edge_todo_archive.md),本文件不再保留正文:

| 条目 | 闭单性质 | 闭单日期 |
|---|---|---|
| §0 02-stage-vm campaign 顺序表(T1–T36) | 全部完成 | 2026-09-07 |
| E3 VmBootHandoff 补 kernel text/data span | 完成 | 2026-09-08 |
| E-RSSTART rs_start_t 字节 ABI + copy_rs_start 解码 | 改判关单(接线转入 03-stage-rs §21) | 2026-09-07 |
| E-VMMCPWIRE vmmcp reply.addr 64 位化 | 完成(宽度对账余件转低优先扫描项) | 2026-09-08 |

---

## E1 minix-sys 用户态 trap 层落地（2026-09-07 阻塞精化：前置是 01-stage-kernel 的 trap 桥）

**问题**：minix-sys 的两个 transport 都是 `-EIO` stub——`DirectTrapTransport`（`os/libs/minix-sys/src/ipc.rs` 全部方法返回 `Err(TrapStatus(EIO))`）与 `DirectKernelCallTransport`（`os/libs/minix-sys/src/syscall.rs` 返回 `-EIO`）。全仓不存在任何用户态 trap 指令序列/入口。

**2026-09-07 阻塞精化（stale-premise 复查）**：原描述"kernel 侧接收端已就绪，缺的只是真实 trap 体"**不完整**。kernel 侧现状：
- **已就绪**：`dispatch_ipc_entry` / `kernel_call_dispatch` 的 C 级分派（syscall.rs，含 BKL/权限/死锁检测，测试完备）；IDT 门配置机制（`os/arch/src/x86_64/trap_entry.rs`：gate 33 DPL=3、SYSCALL MSR、`configure_ipc_entry/configure_syscall`）。
- **未落地**：① IDT handler 地址为占位 0——"后续 boot 阶段 `set_handler()` + `load()`"的阶段尚未存在（kernel/src/lib.rs `init_protection` 注释自证）；② **trap 桥不存在**：捕获用户寄存器 → 保存用户上下文到 KProcess → 定位当前进程 → 从用户内存拷贝消息 → 调用 `dispatch_ipc_entry`/`kernel_call_dispatch` 的入口 asm/Rust 胶水（`dispatch_ipc_entry` 生产调用方为零，仅测试）；③ `switch_to_user` 仅存在于文档引用（sched.rs §3.3），无实现——内核从不返回用户态。
- **推论**：用户侧 trap asm 的寄存器/clobber 约定（C i386 先例：eax=端点/ebx=消息指针/ecx=IPC 调用号/int $33——本树 libc 仅 arm+i386 变体，无 amd64 参照）**无法对齐一个尚未设计的内核桥**。按反 guess 纪律（E-RSWIRE 先例），用户侧实现须待 01-stage-kernel 落地 trap 桥设计（入口 stub + 寄存器约定 + 用户上下文布局）后再动。

**影响**：不变——全部用户态服务器的 IPC/kernel-call 真实通电挂本条；且本条真实前置是 **01-stage-kernel 的 trap 桥 + switch_to_user**（该 stage 的 V12 工作流进行中）。

**建议（更新后的执行序）**：
1. 01-stage-kernel：落地 trap 桥设计（x86_64 入口 asm：IDT gate 32/33 handler、用户上下文保存/恢复布局、switch_to_user）——寄存器约定在桥设计时定稿并文档化；
2. minix-sys（本条主体）：按定稿约定写用户侧 `int $0x21/0x20`（或 syscall）序列，`DirectTrapTransport`/`DirectKernelCallTransport` 换真实 trap 体（CannedTransport 测试路径不变）；
3. 回写 T9-T15 通电标注 + E5 联调包。

**解锁**：T9 / T10 / T11 / T13 / T14 / T15 的真实通电；E5 前置；E6/E8/E9 的传输半。

---

## E2 minix-sys SYS_* kernel-call 包装函数

**问题**：minix-sys 没有任何 `SYS_*` 内核调用包装（grep 零命中）——VM 侧需要的 SYS_FORK（`os/kernel/src/syscall_process.rs:122-215` 已真实）、SYS_UPDATE（`os/kernel/src/misc.rs:1635`，12 步全实现）、SYS_SAFECOPYFROM/TO（`os/kernel/src/syscall_copy.rs:367/:381`）、SYS_EXEC（`os/kernel/src/syscall_process.rs:231`）、SYS_DIAGCTL code 1 控制台输出（`os/kernel/src/syscall.rs:2212-2240`）都缺用户态入口。

**影响**：VM 的 KernelGateway seam（T9）只能以 mock 实现这些调用；fork/RS live-update/safecopy/exec/audit 的端到端链路缺最后一层。

**建议**：在 `os/libs/minix-sys/src/syscall.rs` 复用既有 `perform_kernel_call`（:201，ENOTREADY 重试）与 `KernelCallTransport`（:132-217）机制，为上述 6 类调用各加一个包装函数（签名对齐 kernel dispatch 的消息布局，message 构造用 minix-types 现有 union 成员）；每个包装带一个"CannedTransport 回放"单元测试。

**解锁**：T11 / T13（步骤 4）/ T14 / T15 的真实通电。

> **复核（2026-09-15）**：锚点更新——`perform_kernel_call` 现 :539（`KernelCallTransport` 区间基本未变）；minix-sys 现有 11 个 `sys_*` wrapper（kill/abort/times/sigsend/getksig/endksig/trace/runctl/resume/vircopy/clear），本条 VM 侧 6 类（fork/update/safecopyfrom/safecopyto/exec/diagctl）grep 零命中待建；minix-types `ipc/sysinfo.rs` 已有 SYS_GETINFO/SYS_DIAGCTL/SYS_SAFECOPYFROM/SYS_SAFECOPYTO 等常量可直接消费。

---

## E4 pt_alloc free 注册 + 三架构 destroy 中间页回收（= 02-stage-vm V11-P2-8）

> **进度（2026-09-08，x86_64 + riscv64 完成）**：riscv64 Sv39 三级树同款回收落地——`free_child_tables` 以 V 位 + 非 leaf（R|W|X=0）判定表页（leaf = 数据页，归 region/exit 路径），L0 的子即数据故 level ≥ 2 停止下探；`channel_to_ptr` 同款 cfg(test+mock) 路由。**验证方式差异**：riscv64 模块 `target_arch` 门控，宿主不编译、QEMU/harness 均无 std——实现经 `cargo check --target riscv64gc-unknown-none-elf --no-default-features --features riscv64` 编译验证（kernel 同款 shape，0 error），运行时验证归 QEMU（E5 族）；原 riscv64 宿主测试草稿因 no_std 目标无 std harness 已删。x86_64 部分此前已完成（宿主测试在位）。**余件：aarch64（arch/src 无独立 paging 文件，目标可用时处置）+ kernel/VM 侧 `register_free` 接线**（随 VM 进程退出路径完善时补）。
>
> 原始问题描述（x86_64 部分已修复）：`pt_alloc.rs` 只有 `register/is_registered/alloc_pt_page`，没有 free；`x86_64/paging.rs` 的 `destroy` 只清零根 PML4——每次进程退出泄漏中间页表页（C `pt_free` pagetable.c:1427-1437 回收；Redox/Linux 同样回收）。

---

## E-VFSWIRE VFS_VMCALL 消息 wire 定稿 + VM 发送排水（T10 发送半）

**背景**：T10 入队半已落地——fdref 归零/进程退出的 FdClose、以及 mmap 的 FDLOOKUP/FDIO 均入 `VfsRequestQueue`（callback-less 或带回调）。但"把队首请求构造成 `VFS_VMCALL` 消息经 transport 发给 VFS"的发送排水未实现（历史上 transport-gated，见 doc 23 的 transport 缺口）。

**阻塞点（跨 stage）**：wire 格式属 VM↔VFS 共享契约，而 vfs 服务器（并行工作流）目前只有**决策原语**——`VmVfsReq::from_raw`（servers/vfs/src/misc.rs:266-281，101/102/103）与 `VM_VFS_REPLY=0xC1E`（:305），消息级解码（`do_vm_call` 的 union 成员/字段映射，C vfs.c:60-104 `VFS_VMCALL_REQ/FD/REQID/ENDPOINT/OFFSET/LENGTH`）尚未建。VM 侧先行编码会与 vfs 侧未来的解码漂移。

**解锁后 VM 侧工作（约半迭代）**：
1. `VfsRequestQueue::take_pending_vfs_call()`——从 active 请求构造 `VFS_VMCALL` 消息（字段映射 C vfs.c:70-78，req_id/endpoint/fd/offset/length）并标记已发送；
2. `VmServer::run_once` 排水步——`transport.send(VFS_PROC_NR, msg)`，失败保留请求下轮重试（pre-E1 每轮 -EIO，行为同今日的"永不发送"，无回归）；
3. 测试：Canned transport 上断言 VFS_VMCALL 字段逐项正确。

**依赖**：E1（trap 层）落地后发送才真实可达；vfs 侧 do_vm_call 消息面定稿（09/13-stage 工作流）后定 wire。

> **2026-09-09 增补（05-stage-vfs R2 轮）**：VFS 侧接收半现状——`VmVfsReq::from_raw` 决策原语已有（os/servers/vfs/src/misc.rs:262-281，101=FdLookup/102=FdClose/103=FdIo）与 `VM_VFS_REPLY=0xC1E`（misc.rs:305 一带），**消息级解码仍未建**（do_vm_call 的 union 字段映射，C vfs.c:60-104）。wire 定稿追加一条纪律：m_type 与字段的**绝对值**必须以 C 源断言（教训：vfs 侧 request.rs:15 把 REQ 基址写成 0x600 而 C com.h:589 为 0xA00，两侧偏移对齐掩盖基址分叉——详见 05-stage-vfs/todo.md §9.2 R2-P0-1）。

---

## E-RSWIRE rprocpub 字节 ABI pinning + rproctab 解码（T9 step3 的余件）

**背景**：RS_INIT 握手的 grant 贯通与 fail-closed 已落地（Fix #34）；`ipc_call_rs_init` 目前诚实返回 `NotImplemented`（pin 测试 `test_run_once_rs_init_fails_closed_until_erswire` 定格），假成功 `Ok(RprocTab::empty())` 已消灭。

**阻塞点**：`struct rprocpub` 的字节 ABI 无法从本仓 minix3 子树 pinning——`devmajor_t`、`bitchunk_t`、`struct rs_pci` 在此树中被引用但**无定义**（rs.h/type.h 不完整）。按 Ground Truth 链（C 源 > doc > code），缺 C 事实就不能猜偏移。且该布局是 RS↔VM 共享契约（rs 服务器工作流同样消费），落点应在 minix-types。

> **拆分注记（2026-09-07，同 E-RSSTART 改判）**：阻塞前提同样不成立——`devmajor_t = int32_t`（`minix3/sys/sys/types.h:286-288`）、`bitchunk_t = uint32_t`（:124）、`struct rs_pci` 完整（`minix3/minix/include/minix/rs.h:154-162`）、`struct rprocpub` 完整（rs.h:165-183）。本条目拆两半：**RS 侧半已彻底完成**（2026-09-08，§21 R7+R12）——`rprocpub` wire（Fix #85）与 `struct rproc` 内部表 wire（Fix #89：witness 派生偏移，sizeof=3752，传递 pinning struct priv/minix_timer_t/sys_map_t/sigset_t 全部在树内核实）落地 `minix-types::ipc::{rprocpub, rproc}`，`do_getsysinfo` 三臂（SI_PROCPUB_TAB/SI_PROC_TAB/SI_PROCALL_TAB）全量 live；**VM 侧半**（下文第 3 步：`Gateway::sys_safecopyfrom` + `ipc_call_rs_init` 真实体 + pin 测试翻转）仍留本条目——属 VM stage 生产代码（edge 判定②），其 wire 消费面（`RprocPubWire`/`decode_rproc_pub`）已就绪。原文偏移表的 ILP32 假设作废：x86-64 LP64，风格模板 = `minix-types::ipc::{rs_start, rprocpub}`（Fix #81/#85）。

**解锁后工作（约一个完整迭代）**：
1. 从完整 Minix3 源码树 pin 三类型定义 → 计算偏移表（ILP32：in_use short@0、sys_flags@4、endpoint@8、old_endpoint@12、new_endpoint@16、dev_nr、nr_domain、domain[8]、label[16]、proc_name[16]、vm_call_mask[BITMAP_CHUNKS(49)]、rs_pci、devman_id）；
2. minix-types 增 `RprocpubWire`（repr(C)）+ `decode` + 偏移断言测试（手排字节十六进制锚定）；
3. VM 侧：`Gateway::sys_safecopyfrom(granter, gid, offset, buf)`（wire 已定：SYS_SAFECOPYFROM=31、`MessLsysKernSafecopy{from_to,grant_id,offset,address,bytes}`，kernel 应答 Ok(0)=成功）+ `ipc_call_rs_init` 真实体（拷贝 + 解码 → RprocTab）+ 翻转 pin 测试 + 恢复 `rs_handshake` ACL 循环为可达。

> **增补（2026-09-09，02-stage-vm V13 轮，02-stage-vm/todo.md §18.2）**：第 3 步落地时必须一并做三件 VM 侧事，否则 wire 通即引入新缺陷——
> (a) `RprocEntry.call_mask: u32 → u64`（vm_server.rs:1594-1599）：minix-types 的 wire 已是 u64（rprocpub.rs:89，2×u32 little-endian 合并），VM 内部 u32 会截断调用号 +32..+48 的 9 个调用授权（消费点 vm_server.rs:1340 `entry.call_mask as u64`）；
> (b) `RS_SET_PRIV` 真掩码：E2 的 SYS_SAFECOPYFROM 可用后，dispatcher.rs:1091-1102 的硬编码 `mask = None` 换成 C rs.c:41-56 的 sys_datacopy 语义（M2 的 m2l1 是掩码缓冲指针）；
> (c) 五个 C 消息结构（getphys/getref/info/rusage/update）补专属 wire struct + `size_of`/`offset_of` 断言（现 overlay 解码字段语义与 C 错位：getref addr C@4 vs Rust m1p1@16、info next C@16 vs Rust m2l2@24、rusage children C@8 vs Rust@4；C 锚 ipc.h:928-934/:1486-1492/:1494-1502/:1513-1520），并把其余 VM payload 缺失的 56 字节断言补齐（12 个结构仅 MessVmmcpReply 有）。

**依赖**：~~完整 Minix3 C 源码参照（或补全本树头文件）~~（已解除，同 E-RSSTART）；无 E1/E2 依赖（解码可纯单测）。

---

## E-BOOTFRAME boot 初始栈帧 ABI（T14 余件：minix_stack_params/fill 复刻）

**背景**：`exec_bootproc` 的装载半（ELF 段映射）与 `sys_exec` 半已落地（Fix #41）。C main.c:381-408 在两者之间构造最小初始栈帧——`minix_stack_params(path, argv, envp, …)` 计算尺寸 + `minix_stack_fill(…)` 产出字节精确的 frame，再 `sys_datacopy(SELF, frame, endpoint, vsp, …)` 拷入目标地址空间，`psp = vsp + (psp - frame)` 作为 ps_strings 指针传 `sys_exec`。

**为何 edge**：frame 布局是 VM↔libc↔kernel 三方共享 ABI——argv/envp 数组、字符串区、`struct ps_strings {argv, argc, envp, envc}` 的排布与对齐，消费方是 minix3 libc crt0（`_start` 如何取 argc）与内核 `arch_proc_init`（sp/ps_str 的寄存器约定）。仓内无既有物（PM 的 exec frame 由 VFS 构造传入，路径不同）。复刻前必须对照 minix3 libc 源码（crt0.S + libminixfw 的 stack.c）与本项目未来用户态约定，属跨模块契约。

**解锁后 VM 侧工作（约一个迭代）**：
1. `struct PsStrings` + `stack_params`/`stack_fill` 复刻（boot 简化路径：argv={proc_name,NULL}、envp={NULL}）；
2. `exec_bootproc` 中建栈 region（`user_sp` 向下一页）+ frame 写入（Direct Map）+ `handle_memory_once` 实化；
3. `sys_exec` 的 stack/ps_str 换真值；
4. 测试：SimPaging 下断言 frame 字节布局 + `last_exec` 的 stack/ps_str。

**依赖**：minix3 libc 源码参照（crt0 约定）；与 E1/E2 无序（frame 构造纯 VM 内）。

---

## E-FORKMSG kernel sys_fork 应答补 msgaddr 出参（= 02-stage-vm T33 余件，2026-09-07 登记）

**问题**：C 的 `sys_fork` 有第五个出参 `msgaddr`（fork.c:90，内核自 `p_delivermsg_vir` 报告 PM 的 fork 消息在父地址空间的位置）；`do_fork` 用它对父子两侧的交付消息缓冲做 eager CoW（fork.c:100-108，`handle_memory_once` ×2），防止内核写 fork 应答时撞上 VM 单线程死锁。minix-rs kernel 的 `dispatch_fork`（syscall_process.rs:122-215）只回 child endpoint，VM 侧 `do_fork` 的 eager-CoW 相以 `fork_msgaddr == None` 门控跳过（fork.rs，V11/T33）。

**跨 stage 文件**：`os/kernel/src/syscall_process.rs`（dispatch_fork 应答补 msgaddr——来源 `caller.p_delivermsg` 等价物）、`os/libs/minix-sys` 或 reply wire（KcallResult/`m_krn_lsys_sys_fork` 加字段）、`os/servers/vm/src/kernel_gateway.rs`（`sys_fork` 的 `None` 换真值，VM 侧零改动——消费代码已就位并有 mock 测试）。

**解锁**：T33 的 eager-CoW 相在真实硬件上生效；E5(a) PM↔VM fork 联调的正确性前提（内核写应答不撞 CoW 死锁）。

> **复核（2026-09-15）**：锚点更新——dispatch_fork 体 syscall_process.rs:133-215，应答仅 `KcallResult::Ok(child_endpoint.0)`（:214）；gateway kernel_gateway.rs:197-199 返回 `None`（注释自引本条）；fork.rs eager-CoW 门在 :364。另核实 minix-types 无 `m_krn_lsys_sys_fork` 成员——msgaddr 出参需在应答 wire 上新增字段，与本条「跨 stage 文件」第 2 项一致。

## E5 端到端联调测试包

**问题**：VM 与其他服务器的协作当前零端到端覆盖——`os/tests/pm_vm_fork{,_test}.rs` 正文整体注释停用（自注 "DEPRECATED: permanently disabled"）；VFS fdclose、RS live-update、QEMU VM paging 冒烟均无。

**建议**：在 E1/E2 落地后建联调包：(a) PM↔VM fork 全链路（恢复/重写旧测试，改走 minix-sys 消息层而非 crate 内类型——旧失效原因正是 `pub(crate)` 边界收紧）；(b) VM↔VFS fdclose 往返；(c) RS live-update 全链路（RS_PREPARE → UPDATE → resume）；(d) QEMU VM paging 冒烟（boot shim 拉起 VM → `init_vm_self_pt` → map/query/unmap 测试页 → 串口结果，复用 `os/qemu-tests/` 基建）。

> **验收面增补（2026-09-08，02-stage-vm V12 轮）**：(d) 冒烟必须覆盖**缺页故障完整回路**——进程触发缺页 → 内核 VM_PAGEFAULT 送达 VM → VM 解析（CoW 拷贝或新页分配）→ **VM 写进程硬件 PTE**（G-V12-8，当前缺失的最后一环）→ 内核清 RTS_PAGEFAULT 恢复进程 → 指令重执行不再故障。没有 PTE 写入的通电冒烟会以"同一地址反复缺页活锁"的形式失败，这正是该回路必须成为 (d) 的显式断言项的原因。

> **验收面增补（2026-09-09，06-stage-sched V2 轮）**：(e) **PM↔SCHED 调度链**——sched 服务器是全仓唯一"每个行为都依赖他方主动来电"的服务（PM 的 START/INHERIT/STOP/SET_NICE、内核的 NO_QUANTUM、自身的 5 秒平衡闹钟），端到端零覆盖。冒烟三链：init 的 START（PM 发 SCHEDULING_START → SCHED `sys_schedctl` 接管 → `sys_schedule` 下发 → 内核实调）；fork 子进程的 INHERIT（优先级/时间片从父槽继承，06 篇）；一条 NO_QUANTUM 回环（内核 notify_scheduler → SCHED 降一级 → `sys_schedule` 回写）。前置 E1 + E8；E-PREEMPTFLAG/E-SCHEDNICED 修不修都应在冒烟中显式观察——未修时优先级 ≥1 的进程可跑通全链，恰可作"断环分界"用例（队列 0 进程验证 NO_QUANTUM 缺失）。

> **验收面增补（2026-09-14，07-stage-ds 首轮架构审查）**：(f) **DS 发布/订阅链**——C 的行为契约在 minix3/minix/tests/ds/（dstest.c 178 行：publish/retrieve/delete/getsysinfo 全类型往返；subs.c 91 行：subscribe/check/notify 回环），Rust 侧零承接。冒烟三链：(1) RS boot 映射后 `ds_retrieve_label_endpt` 可解析服务端点；(2) 客户端 A `ds_publish_u32` → 客户端 B `ds_check` 收到通知并取回键名与类型（ipc_notify 回环）；(3) label 级联删除（RS 删 label → 同名主条目与订阅全清，store.c:613-636）。前置 E-DSWIRE；A-2 regex 引擎落地后追加元字符 pattern 订阅用例（VFS 同款 `drv\.[bc]..\..*`，main.c:441）。

> **验收面增补（2026-09-15，10-stage-mib 首轮架构审查）**：(g) **MIB/sysctl 链**——冒烟四链：(1) MIB_SYSCTL 往返（sysctl(2) 调用方 → MIB 解码 → 树查找 → 数据返回，含 ENOMEM 部分拷贝 + 完整长度回写语义 main.c:341-356）；(2) rmibtest 契约（minix3/minix/tests/rmibtest/rmibtest.c：8 个远端注册拒绝场景 :126-177、遮蔽注册顺序 :183-201、handler 函数偏转 :79-85）；(3) 远程子树 ERESTART 续走（服务死亡 → mib_down → 本地续走，remote.c:455-459 + tree.c:1410-1416）；(4) minix.mib.* 统计子树读数一致（TreeCounts 计数与真实节点数对账）。前置：10-stage-mib/todo.md P1-1/P1-2（stage 内执行半）+ E1 + E-RMIBWIRE。

> **验收面增补（2026-09-15，11-stage-devman 首轮架构审查）**：(h) **devman 设备生命周期链**——冒烟四链：(1) VFS mount devman 后 `devices`/`events` 可见（懒建树触发，11-stage-devman/todo.md DM-P1-3）；(2) 驱动进程 `devman_add_device`（minix-sys 客户端）→ devman 树出现目录与 `devman_id` 文件 → 读 `events` 文件两读排空到 ADD 行；(3) RS publish → DEVMAN_BIND 转发给设备 owner → 驱动 `handle_msg` 回调 → 回 RS OK（C manager.c:840-851 全链）；(4) UNBIND + DEL → REMOVE 事件 + 树内消失 + 父设备引用级联回收。对照 C 行为契约 minix3/minix/commands/devmand/main.c 的属性匹配 DSL（13 篇）可用 usb 设备属性样例覆盖。前置：E-DMWIRE + E-REQWIRE（VTreeFS wire 面）+ E1。

---

## E6 minix-sys PM 所需 SYS_* wrapper 扩充（= 04-stage-pm/todo.md §9 抽取）

**问题**：E2 只覆盖 VM 侧 6 类 SYS_* 包装；PM 侧生产代码声明的内核调用面同样没有用户态入口。04-stage-pm/todo.md §6 的 24 项 D-XX 登记中有 6 项的解除条件是"对应内核调用经 minix-sys 可达"，但 minix-sys 侧没有任何 PM 专用 SYS_* 包装（`os/libs/minix-sys/src/` 全源仅 lib.rs 的 14 个 POSIX 形顶层函数）。

**证据**（kernel 对端现状，2026-09-06 grep 核实）：
- 对端已实现、可直接包 wrapper：SYS_TIMES（`os/kernel/src/syscall_clock.rs:90`）、SYS_CLEAR（`os/kernel/src/syscall_process.rs:366`）、SYS_KILL（`os/kernel/src/syscall_signal.rs:148`）、SYS_SIGSEND（`os/kernel/src/syscall_signal.rs:544`）、SYS_ABORT（`os/kernel/src/syscall.rs:1793`）、SYS_SAFECOPYFROM/TO（`os/kernel/src/syscall_copy.rs:361/:376`，E2 已列）。
- 对端也不存在：SYS_GETMONPARAMS/SYS_GETIMAGE（`os/kernel/src/` 全源 grep 零命中）——PM 的 `BootParams::placeholder()`（`os/servers/pm/src/main.rs:15`）没有真实对端；需先与 01-stage-kernel 裁决启动参数/映像的传递路径（boot-shim handoff 是否已取代 C 的 monitor 参数语义），再双侧新建。

**影响**：04-stage-pm/todo.md 的 D-02（启动参数）、D-09（sys_abort）、D-13（sys_kill）、D-18（sys_clear）、D-21（rusage safecopy）、D-24（sys_times/getticks）全部停在"逻辑完备、通电无门"；PM 的 `KernelIpcTransport`（`os/servers/pm/src/ipc/transport.rs:86-96` unimplemented）除 trap 层（E1）外也缺这层调用面。

**建议**：复用 E2 的 `perform_kernel_call`（`os/libs/minix-sys/src/syscall.rs:201`，ENOTREADY 重试）与 `KernelCallTransport` 机制，为对端已实现的 6 类各加一个包装函数（签名对齐 kernel dispatch 的消息布局，message 构造用 minix-types 现有 union 成员），每个包装带一个 CannedTransport 回放单元测试；SYS_GETMONPARAMS/SYS_GETIMAGE 单独立项，待 01-stage-kernel 裁决传递路径后双侧补齐。

**解锁**：04-stage-pm/todo.md D-02 / D-09 / D-13 / D-18 / D-21 / D-24 的真实通电；E5(a) PM↔VM fork 联调的信号与回收链前置。

> **进度（2026-09-06）**：`sys_kill` wrapper 已落地（`syscall.rs` 的 `sys_kill` + `SYS_KILL_CALL` 常量，wire 断言测试 ×2；`CannedKernelCallTransport` 增 `sent` 逐调用消息记录供 wire 形状断言）——消费侧 04-stage-pm/todo.md D-13/Fix #23 同轮闭环；余下进度：SYS_CLEAR（轮 24）、SYS_ABORT（轮 25）、SYS_TIMES（轮 26）、SYS_RUNCTL/SYS_RESUME（轮 28）已落地；待做 SYS_GETMONPARAMS/SYS_GETIMAGE wrapper（kernel 对端亦缺，需双侧新建）。
>
> **进度（2026-09-08，V2 轮扩充）**：已落地 wrapper 达 6 个（sys_kill/sys_clear/sys_abort/sys_times/sys_runctl+sys_resume/sys_vircopy）。04-stage-pm 第二轮审查（04-stage-pm/todo.md §11.1.1 接线批次表）给出 PM 侧完整需求清单，待做 wrapper 及 kernel 对端现状：`sys_setalarm`（kernel 对端已有 `syscall_clock.rs:160`，E8 已列）、`sys_vtimer`（ITIMER_VIRTUAL/PROF，对端待核实）、`sys_datacopy`（注意≠sys_vircopy：SELF 本地拷贝语义，getgroups/setgroups/itimer value 双向拷贝需要）、`sys_settime`/`sys_stime`（time.c do_settime/do_stime）、GETUPTIME 面（time.c getuptime 依赖，PM `ClockSource` 的生产实现）、`sys_getmcontext`/`sys_setmcontext`（mcontext.c 直通）、`sys_sprof`（profile.c，feature 门）、`sys_sigreturn`（C signal.c:176-192 do_sigreturn 调用，kernel 对端待核实）、`sys_diagctl_stacktrace`（C signal.c:556-558 coredump 诊断，04-stage-pm V2-P3-4a）、SYS_GETMONPARAMS/SYS_GETIMAGE（维持"双侧新建"结论）。
>
> **进度（2026-09-09，V3 轮增补，来源 04-stage-pm/todo.md §12 V3-P1-2 / V3-P1-3）**：清单增两项。① **`sys_delay_stop`**——13-signal-flow 的 `KernelStop` 生产实现前置（`os/servers/pm/src/signal_flow.rs:115-140` `stop_proc` 的内核停止 seam）；04-stage-pm V3-P1-3（sig_proc 的 VFS_CALL 分支接真实 stop_proc）mock 层实施不受阻，生产语义完备依赖本项。② **内核 ksig 对端核实**——C 的内核→PM 信号回环入口是 SEF 拦截 SIGKSIG 通知后调 `process_ksig`（`minix3/minix/servers/pm/main.c:121` + `minix3/minix/lib/libsys/sef_signal.c:104-108`），C 的 process_ksig 内部走 sys_getksig/sys_endksig 内核信号队列循环；Rust 侧 `process_ksig`（`os/servers/pm/src/signal.rs:374`）现无生产调用者（04-stage-pm V3-P1-2，新接线批次 H）——需核实 os/kernel 是否已实现 getksig/endksig 或等价通知面；若无，wrapper 归本条、kernel 对端归 01-stage-kernel 工作流（双侧新建，同 SYS_GETMONPARAMS 先例）。

> **进度（2026-09-09，V3-P1-1 切片）**：`sys_trace` wrapper 已落地（`os/libs/minix-sys/src/syscall.rs`：`sys_trace` + `SYS_TRACE_CALL = 5`，载荷 `m_lsys_krn_sys_trace` 布局（request@0/endpt@4/address@8/data@16，≠ m_m1），读值经同偏移写回；wire 断言测试 ×2）——kernel 对端 `dispatch_trace` 早已真实（`os/kernel/src/misc.rs:209`，非待核实项）；消费侧 04-stage-pm V3-P1-1（PTRACE 臂 + trace_stop）同轮闭环。**同轮新登记**：`sys_delay_stop`（见上①）之外再确认 kernel 侧无 SYS_TRACE 之外的 PM trace 依赖；`sys_datacopy` 语义（T_GETRANGE 的参数块取入）当前由 `KernelGateway::copy_from_user` 委托 sys_vircopy 承接（Fix #27 同型先例），独立 `sys_datacopy` wrapper 维持原清单判断。

> **进度（2026-09-09，V3-P1-2 切片）**：`sys_getksig`（SYS_GETKSIG=7）/`sys_endksig`（SYS_ENDKSIG=8）wrapper 已落地（wire 断言测试 ×2）——kernel 对端 `dispatch_getksig`/`dispatch_endksig` 早已真实（`os/kernel/src/syscall_signal.rs:393/475`），原"②内核 ksig 对端核实"结论：**getksig/endksig 已存在，缺的只是 wrapper**（本切片闭环）。消费侧 04-stage-pm V3-P1-2（SYSTEM notify 触发 + `process_sigmgr_signals` 拉取循环）同轮落地。**新增跨层登记（SigSet 128 位拓宽）**：Rust `SigSet(u64)` 装不下内核信号位 70/73/74（SIGSNDELAY/SIGKSIGSM/SIGKSIG，kernel `syscall_signal.rs:88-94` 已自声明"widening tracked separately"）——影响 GET_PROCTAB/GET_PRIVTAB/notify/GETKSIG 消息字段 + PM 镜像，属于共享契约层 wire 变更，需单独立项（建议挂本条目或 E7 追加判定）；PM 侧 batch H 的触发判定已按"SYSTEM 通知即拉取"适配并注释声明。

---

## E7 minix-types PM 协议面系统化（= 04-stage-pm/todo.md P1-4 + P2-3 抽取）

**问题**：minix-types 对 PM 的协议面是三个碎片。(a) `os/libs/minix-types/src/ipc/pm.rs:13-38` 的 `PmRequest`/`PmResponse` 是零使用死代码——整个工作区只有 `PmError` 被 `os/servers/pm/src/init.rs:390` 消费。(b) PM 调用号双址：`os/servers/pm/src/ipc/calls.rs:29-124` 的 `PmCall` 枚举（47 值，calls.rs:16-17 注释自述"将来内核侧需要调用号时再上移 minix-types"）与 minix-types 散落常量（如 `PM_PROCEVENTMASK`，被 `os/servers/pm/src/init.rs:368` 使用）并存，同一事实两处表达。(c) 47 个调用的消息布局没有系统化 wire 类型——C `m_lc_pm_*`/`m_pm_lc_*` union 字段（minix/com.h）当前只在 PM 主循环内联 unsafe 访问（`os/servers/pm/src/init.rs:430/438-440/459-460/480-481`）。另有调度协议常量缺失：`SEND_PRIORITY`/`SEND_TIME_SLICE`（C `minix3/minix/servers/pm/const.h:19-20`）全仓无对应。

**证据**：`os/libs/minix-types/src/ipc/pm.rs` 全文 111 行仅 Fork 一个请求变体；VM 侧同型先例是 `NR_VM_CALLS` 双定义（02-stage-vm/todo.md:1047，`minix-types/src/ipc/vm.rs:149` vs `os/servers/vm/src/vm_server.rs:1173` 私有副本）；共享层"上移 minix-types"的 OQ 决策先例见 G1（01-stage-kernel/todo.md:342，ProcNr）。

**影响**：PM 的分发接线（04-stage-pm/todo.md P1-1，40 个待点亮调用）每一臂都要先回答"消息怎么解码"；没有系统化 wire 类型，unsafe 解码将被复制约 40 份，wire 布局错误无类型层防护；调用号双址使 callnr.h 的单一真值破口随消费方（libc/commands）增多而扩大。

**建议**：与 04-stage-pm P1-4 协同一次做齐——(1) 按调用族在 `os/libs/minix-types/src/ipc/pm.rs` 建 wire 结构体（对照 C union 逐字段 + `size_of` 断言，风格对齐 `ipc/message.rs` 既有成员如 `MessPmSchedSchedulingSetNice` :1165）；(2) 处置 `PmRequest`/`PmResponse` 死代码：要么作为新 wire 层的入口枚举重构，要么删除（待 P1-4 设计时定，不允许默认保留）；(3) 调用号收敛二选一：47 个 `pub const PM_*` 上移 minix-types（PmCall 枚举随之迁移，成为 callnr.h 的 Rust 等价物，倾向此案）或 minix-types 常量清空、pm crate 为唯一真值——需 OQ 确认归属；(4) 补 `SEND_PRIORITY`/`SEND_TIME_SLICE` 常量。

**解锁**：04-stage-pm/todo.md P1-1（每臂解码）/ P1-4 / P2-3 的实施前提；未来 libc/commands 侧 PM 调用发起方的常量消费。

> **进度（2026-09-06）**：首个切片已落地——`MessPmLcWait4 { status }` + `m_pm_lc_wait4` arm（`message.rs`，56 字节断言 `test_pm_wait4_message_layouts`），wait4 回复载荷契约（04-stage-pm/todo.md D-26/Fix #22）闭环；其余 wire 族照本切片的风格推进。
>
> **进度（2026-09-08，V2 轮增补）**：(1) **仓库内先例确立**：`minix-types::ipc::rs_start`（约 900 行 per-server 类型化 wire 模块，commit 764d738af，03-stage-rs Fix #81）为建议 (1) 的"按调用族建 wire 结构体"提供了本仓样式模板，实施时应对照该模块的组织方式（wire 结构 + `size_of` 断言 + 解码函数）。(2) **wire 成员清单按 04-stage-pm/todo.md §11.1.1 接线批次表逐批落地**：A 凭证 13 调用（`m_lc_pm_getuid`/`setuid`/`groups`/`getsid` 等）→ B 信号控制 6 → C 时间 6 → D itimer → E exec 3（`m_lexec_pm_exec_new`/`m_rs_pm_exec_restart`）→ F 调度 2（含补 `SEND_PRIORITY`/`SEND_TIME_SLICE` 常量）→ G 杂项 9。(3) A 批次 wire 设计时需决定 gid 载荷宽度以恢复 C 的 GID_MAX 拒绝语义（04-stage-pm/todo.md §11 V2-P3-2，GID_MAX=2^31-1，`minix3/sys/sys/syslimits.h:53`）。
>
> **进度（2026-09-09，V3 轮增补，来源 04-stage-pm/todo.md §12 V3-P3-5）**：双址清单增一处——`os/servers/pm/src/ipc/dispatcher.rs:32` 本地定义 `PROC_EVENT_REPLY: i32 = 0xE80`，而 minix-types 已有同值常量（`os/libs/minix-types/src/ipc/event.rs:27`，带常量锁定测试）。建议 (3) 调用号收敛时一并处置（PM 侧改为消费 minix-types 的常量，与 `VFS_PM_RS_BASE` 经 `dispatcher.rs:28` 的 re-export 先例同型）。另记一条 PM 侧预备建议：40 臂接线期间，pm crate 内建 `ipc/decode.rs` 集中各调用的 unsafe union 解码（每调用一个函数 + C ipc.h 字段对照注释 + 布局断言），E7 wire 落地后该模块改为委托 wire 类型、调用点零改动（04-stage-pm/todo.md §12.3 观察 2，属 PM stage 内工作，此处仅登记衔接关系）。

---

## E8 minix-sys SCHED 所需内核调用真实通电（= 06-stage-sched/todo.md P1-3 抽取，2026-09-06）

**问题**：SCHED 服务器的传输接缝已按最终形态落地（`os/servers/sched/src/kernel_api/transport.rs`：`IpcTransport` 收发 + `KernelApi` 五调用，打包布局与内核对端逐字段对齐），但真实端委托的 minix-sys 直传当前是 stub——`DirectTrapTransport`（`os/libs/minix-sys/src/ipc.rs:529-559`，EIO）与 `DirectKernelCallTransport`（`os/libs/minix-sys/src/syscall.rs:144-148`，-EIO）。主循环全部 79 个测试在 mock 上运行；生产二进制 `minix-sched` 一进 `run()` 即因传输失败计数到上限而 panic（诚实的失败，非静默）。

**证据**（内核对端现状，2026-09-06 核实）：SYS_SCHEDULE（`os/kernel/src/syscall.rs:821`）、SYS_SCHEDCTL（`syscall_process.rs:592`）、SYS_SETALARM（`syscall_clock.rs:160`）、GETMINFO GET_MACHINE/GET_HZ（`misc.rs:1033-1050`，指针 safecopy 契约）全部已实现；缺的只是 trap 层本体。

**影响**：SCHED 的真实通电（启动读机器 → 设闹钟 → 接收分发回复全链）；E5 联调包若含 PM↔SCHED 的 fork 接管链，本条是前置。

**建议**：随 E1 一次解决——trap 层落地后 `DirectTrapTransport`/`DirectKernelCallTransport` 换真实 trap 序列，SCHED 侧零改动（接缝已按最终形态写好）；若 E1 前需要单独验证 SCHED 通电，可在 `os/libs/minix-sys/src/syscall.rs` 复用 `perform_kernel_call` 为 SCHED 五调用加命名包装（仿 E6 模板），但当前 `transport.rs` 的打包已完整、包装层无增量价值，倾向不单独建。

**解锁**：06-stage-sched/todo.md P1-3 的真实通电；E5 的 PM↔SCHED 链路（若立项）。

---

## E9 RS 生产接线面：KernelApi 五域面的真实传输（= 03-stage-rs/todo.md §18.10 E-11，2026-09-06）

**问题**：RS 服务器的外部边界 trait 已按最终形态拆为五个域面（2026-09-06 E-2 修复：`SysApi` 内核 8 方法 / `SchedApi` 调度器 2 / `PmApi` PM 进程生命周期 8 / `VmApi` VM 2 / `IpcApi` RS 自身 IPC 2，`KernelApi` 为 supertrait 组合，见 03-stage-rs/01-rs-boot-init.md §3.5），生产实现 `UnimplementedKernelApi` 全部 `Err(ENOSYS)` fail-closed（T7 门 PASS，主循环 `run()` 以 Err 退出而非自旋）。真实传输不存在：trap 层（edge E1）、RS 所需 SYS_* 用户态包装（E2/E6 只覆盖 VM/PM 面）、PM/VM 消息构造面全缺。

**证据**（对端现状，2026-09-06 核实）：内核侧——SYS_PRIVCTL（`os/kernel/src/` 对端已有，do_privctl 语义）、SYS_UPDATE（`misc.rs:1635`，12 步全实现，E2 已列）、SYS_KILL（`syscall_signal.rs:148`，E6 已列）、GETMINFO GET_MACHINE/GET_HZ（`misc.rs:1033-1050`，E8 已列）、SYS_SETALARM（`syscall_clock.rs:160`）；VM 侧——`os/servers/vm/src/rs.rs` 的 RS_PREPARE/RS_UPDATE 对端部分 DEFERRED（02-stage-vm T12/T13）；PM 侧——`srv_fork`/PM_GETEPINFO 对端属 04-stage-pm（部分在建）。

**影响**：03-stage-rs 的 19 号主线（19-rs-external-interfaces.md）通电；E-1（RS 自升级，18 号接线轮）整个链条压在本条的 `PmApi::srv_fork` 上；E5(c) RS live-update 联调链的前置。

**建议**：按 03-stage-rs A5 路线图分域推进，每域一个可独立验证的切片——
1. `IpcApi`/`SysApi` 面：依赖 edge E1（trap 层）+ 在 `os/libs/minix-sys/src/syscall.rs` 复用 `perform_kernel_call` 补 RS 所需 SYS_* 命名包装（SYS_PRIVCTL/GETPRIV/UPDATE/KILL/SETALARM/GETINFO 族，仿 E2/E6 模板，各配 CannedTransport 回放测试）；
2. `PmApi` 面：PM 消息构造（PM_SRV_FORK/PM_GETEPINFO/EXEC 面）——与 04-stage-pm 的对端工作流协同定 wire，`srv_execve` 的 C 实现是 RS 内 ELF 装载 + 内核分配/拷贝 + PM 终步（exec.c:21-64/:102/:127），装载与拷贝可先行（内核调用面），PM 终步随后；
3. `VmApi` 面：VM_RS_MEM_*（com.h:741-745）——依赖 02-stage-vm T12/T13 的 rs.rs 对端；
4. `SchedApi` 面：KERNEL 分支走 SYS_SCHEDCTL（E8 已列对端），SCHED 分支走 SCHEDULING_* 消息（依赖 06-stage-sched 服务器）。
全部落定后逐条回写 03-stage-rs/todo.md §18.10 E-11 与 A5 路线图第 6 步。

**解锁**：03-stage-rs 19 号主线通电；E-1 自升级；E5(c) 联调链。

---

## E-KERNINFO MINIX_KERNINFO 内核信息共享 + release/version 字段（= 01-stage-kernel todo.md I-2，2026-09-07 移交）

**问题**：C 的 `MINIX_KERNINFO=6`（`minix3/minix/include/minix/ipcconst.h:12`）是 `do_ipc` 内的内核信息共享原语——调用者经它取得内核信息表（含 `release[]`/`version[]` 等）；Rust 内核侧未实现（`os/kernel/src/ipc.rs:1775` 自注 "not yet implemented"，`ipc.rs:2061` 测试钉住 `IpcCall::from_raw(6) == None`）。连带 `KernelInfo` 无 `release`/`version` 字段（C 生产者 main.c:432-433，banner 直打 OS_RELEASE 故内核自身无消费）。

**为何 edge**：真实消费方是**用户态进程**（进程初始化时取内核信息页），用户态 trap 层未落地（E1）前无法端到端验证；共享契约面（kerninfo 的 grant/映射机制与 minix-types wire 布局）符合 edge 判定①③。

**解锁后工作**：(1) minix-types 定 kerninfo wire（对照 C `struct minix_kerninfo`/`kinfo` 布局）；(2) kernel `ipc.rs` 增 KernInfo 分支（grant 共享或 safecopy，对齐 C `do_ipc` 该分支语义）+ `KernelInfo` 补 `release`/`version` 填充；(3) 翻转 `ipc.rs:2061` 钉子测试；(4) 用户态首个消费方（libc/服务器初始化）接线后端到端验证。

**依赖**：E1（trap 层）落地后才有用户态调用方；内核侧实现本身可先行（stage 内单测覆盖），但无消费方即无法验证可观察行为——按 todo-fix 规则保持 DEFERRED 登记，不假完成。

> **复核（2026-09-15）**：锚点 +2——ipc.rs:1777（"not yet implemented"）与 :2063（钉子测试）。新增现状：minix-rt 已有用户侧脚手架（init.rs:108-116 `KerninfoSource` trait、:128-136 `DirectTrapSource` 的 `query_kerninfo` 为 Err(EIO) 桩、:145-149 CannedSource 测试源）——内核臂落地后该桩有现成接缝；RS 侧已建模 trap 掩码位（os/servers/rs/src/privilege.rs:152 `MINIX_KERNINFO = 1 << 6`）。

---

## E-MINTYPES-RS minix-types RS 消息层的三个可重构观察（2026-09-07 扫描登记，低优先）

**来源**：03-stage-rs 扫描轮（03-stage-rs/todo.md §20）P3 共享基建审查。三项均为
"可重构/设计脆弱"级而非 bug，当前唯一消费者是 RS（VM/PM 未来接入 rs 消息族时收益），
故登记 edge 由后续单线程裁决，不阻塞任何 stage。

1. **`Message::is_rs_req_arm` 范围守卫脆弱**（message.rs）：`m_rs_req` 臂归属用
   `RS_RQ_BASE..=RS_RQ_BASE+24` 硬编码范围 + 排除 `RS_INIT`/`RS_LU_PREPARE` 判定。
   新增 RS 调用号超出 +24 时守卫**静默失配**（返回 None 而非编译期报错）。建议：
   改枚举/表驱动（RS 消息号的 union 臂归属在类型层表达），或至少加"新增调用号时
   守卫同步"的编译期断言。
2. **message.rs 单文件 3,831 行**：union 臂约 40+，RS/LSYS/内核/PM/VFS 各族混居。
   按族拆分为 `message/{rs,lsys,kernel,pm,vfs}.rs` 子模块是纯机械重构；动手时机
   建议挂在下一次大批消息臂新增前（避免拆分与新增的冲突窗口）。
3. **COMMON_RQ_BASE 族常量寄居 ipc/rs.rs**：`SIGS_SIGNAL_RECEIVED`（com.h:597）与
   `COMMON_REQ_FI_CTL`（com.h:607）属 common 请求族（非 RS 专属），当前仅 RS 消费
   故寄居 rs 模块可接受；PM/VFS（fi_ctl 的接收方是服务自身）未来消费时应上移
   `ipc/common.rs` 或等价归属。

**解锁后工作**：三项互相独立，均为 minix-types 内部重构（接口不变、调用方零改动
或纯 re-export 调整），各约 0.5 个迭代。无 E1/E2 依赖。

> **复核（2026-09-15）**：观察 1 的守卫仍在（ipc/message.rs:364-368）但 `RS_RQ_BASE` 已提取为共享常量（ipc/rs.rs:14），范围形状（BASE..=BASE+24 排除 RS_INIT/RS_LU_PREPARE）未变——脆弱性判定不变；观察 2 的文件现为 ipc/message.rs 3941 行（继续膨胀，拆分时机与 E7 批量新增协调）；观察 3 不变。

---

## E-VMMOCK minix-arch default features 泄漏收口 + "mock" 命名澄清（02-stage-vm G-V12-5 余件，2026-09-08 登记）

**问题**：`os/servers/vm/Cargo.toml:18` 以裸 path 依赖引入 `minix-arch`（`minix-arch = { path = "../../arch" }`），绕过了 workspace 表的 `default-features = false`（`os/Cargo.toml:235`）；而 `os/arch/Cargo.toml:18` 的 `default = ["mock"]` —— mock feature 就此泄漏进 VM 生产构建。kernel 与 boot-shim 均已正确关闭默认（`os/kernel/Cargo.toml:13`、`os/boot-shim/Cargo.toml:17`），VM 是唯一漏网点。同时 `mock` feature 实际门控的是"运行时窗口基址变体"（Direct Map 窗口基址是内核动态授予的运行时值，E3 已接真值），命名与生产用途混淆——这是 02-stage-vm/todo.md §16.1 G-V12-5 的原始登记内容，原定"处置归 E3"，但 **E3 的完成注记（2026-09-08）未包含依赖收口**，为防孤儿单列本条。

**建议**：(1) `os/servers/vm/Cargo.toml` 的 minix-arch 依赖补 `default-features = false`，跑三 feature 矩阵回归（G-V12-8 之前这主要影响编译面与 arch 内 mock 项的 dead_code 噪音，预期零行为差异——若有差异即暴露了生产代码误依赖 mock 项，需逐处修正）；(2) arch 侧把 `mock` feature 更名为诚实表达运行时窗口语义的名字（如 `runtime-window`，或直接内联为非 feature 代码路径），同步 kernel/boot-shim 的引用；(3) 在 02-stage-vm/todo.md §2 表 G-V12-5 行回写闭单。

**解锁**：VM 生产依赖面的单一真相；arch 命名与语义一致。无 E1/E2 依赖，可独立先行。

> **进度（2026-09-08，建议 (1) 完成）**：`os/servers/vm/Cargo.toml` 已补 `default-features = false`，三 feature 矩阵回归零差异（490/507/490 passed）——VM 生产代码无 mock 项依赖，收口无行为影响。**余件**：建议 (2) arch 侧 `mock` 更名（涉 kernel/boot-shim 引用同步）与 (3) G-V12-5 行闭单回写（待 (2) 一并完成）。依据记录：02-stage-vm/todo.md §17.9 Fix #62。
>
> **复核（2026-09-15）**：G-V12-5 行已随 V12 归档迁至 02-stage-vm/archive/todo-V12-archive-2026-09-09.md:108（不在当前 todo.md）——余件 (3) 的回写落点改为该 archive 文件；mock 门控点实测约 119 处（集中在 os/arch/src/lib.rs 的三架构三元组），余件 (2) 是机械批量更名 + kernel/boot-shim 引用同步 + 三 feature 矩阵回归。

---

## E-MINSYS-HYGIENE minix-sys clippy 卫生项 5 条（03-stage-rs §22 扫描登记，2026-09-09，低优先）

**问题**：`cargo clippy` 全链仅存的 crate 本体告警集中在 minix-sys（5 条）：`misc.rs:219/:220`
与 `rmib.rs:85` collapsible-if（3 处可合并 let 链）、`syscall.rs:308` doc 注释后空行、
`rmib.rs:127` `MountTable` 缺 `Default` impl（已有 `const fn new()`）。另有 minix-types
1 条既有 known（`ipc/vm.rs:667` `VmReply` large-size-difference）与 workspace profile
声明 4 条（kernel/boot-shim 的非根 profile，属 Cargo.toml 归位）。

**为何 edge**：minix-sys 是全 stage 共享的用户态 syscall 层（edge 判定①类共享基建），
且其主要增量工作（E1 trap 层、E2 SYS_* wrapper）尚未发生——卫生清理与功能改动会触碰
同文件，分散做必然产生冲突窗口，合并到 E1/E2 的工作轮顺带清零即可。rs crate 本体
clippy 已零告警（03-stage-rs/todo.md §6.0 基线），本条不影响任何 stage 的正确性。

**建议**：E1 或 E2 动工的首轮，`cargo clippy --fix -p minix-sys` + 手工核对 3 处
collapsible-if 的合并语义（`checked_mul`/`checked_div` 链合并不改变短路行为，但需逐处
确认可读性）+ `MountTable` 补 `impl Default`（委托 `new()`）+ profile 声明上移
workspace 根。验收 = 全 workspace `cargo clippy` crate 本体告警清零（依赖宏展开的
误报除外）。

**解锁**：全 workspace clippy 零告警的 CI 门禁前提（当前"触碰文件零告警"纪律是逐轮
人工维持的，机械清零后可升级为全局门）。无前置依赖，但刻意等 E1/E2 避免冲突。

> **复核（2026-09-15，10-stage-mib 首轮架构审查）**：锚点仍有效——`cargo clippy -p minix-sys` 实测 5 条告警（rmib.rs collapsible-if 与 `MountTable` 缺 `Default` 的建议均仍在）；本轮无新增卫生项，条目维持原判。

---

## E-VMTLB kernel 侧目标进程 TLB 刷新机制缺失（02-stage-vm V13 轮登记，2026-09-09）

**问题**：VM 服务器直接写进程硬件 PTE 之后（Fix #60 起故障路径、更早起 munmap/brk/fork 路径），C 对"目标进程在其他 CPU 上的陈旧 TLB 翻译"有显式机制，Rust 内核没有任何对应物：

- C 机制：进程级 `MF_FLUSH_TLB` 标志 + 调度点刷新——`minix3/minix/kernel/proc.c:345-347`（`if (p->p_misc_flags & MF_FLUSH_TLB && get_cpulocal_var(ptproc) == p) tlb_must_refresh = 1`，随后 `switch_address_space(p)` 重载根寄存器）。
- Rust 现状：`rg "MF_FLUSH_TLB|tlb_must_refresh" os/kernel/src` 零命中；且调度器 `switch_address_space` 带"同根跳过重载"优化（os/kernel/src/syscall.rs:2410-2417 注释自述 mirror 比对语义——镜像必须跟上每次根变更，否则首次分发会无谓重载）。
- 风险窗口：真 SMP + 共享页场景（fork 后 CoW 父子页、shm 多映射方）——VM 翻 RO/解除映射时，另一 CPU 上仍在运行的共享方进程的 TLB 保留旧翻译（RO 旧项使写持续故障、已 unmap 旧项使进程继续写已回收物理页）。当前单核 + "VM 只改非运行进程 PTE"的结构纪律（IPC 阻塞 + RTS 停止）下不可达；E5 通电若开 SMP 即暴露。

**为何 edge**：机制主体在 kernel（01-stage 生产代码，edge 判定②），与 VM 的 PTE 写路径（02-stage）协同才可验收（edge 判定③）。

**解锁后工作**：(1) kernel 进程结构补 MF_FLUSH_TLB 等价标志 + 调度点刷新（对照 proc.c:345-347）；(2) 与 switch_address_space 的同根跳过优化对账（刷新标志必须在比较之前生效）；(3) 同步评估 C VM 侧四处 `sys_vmctl(SELF, VMCTL_FLUSHTLB)`（pagetable.c:119/255/319/430，宿主 pt_assert[SANITY]/vm_freepages×2/vm_pagelock[MEMPROTECT]）——它们依附 C 的"进程内存别名映射进 VM 地址空间"模型，minix-rs 的 direct map 下翻译恒定、VM 自刷不需要（该 ARCH 偏差在 02-stage-vm/todo.md §18.2 V13-P2-1(b) 登记）；(4) E5 若含 SMP 配置，冒烟须覆盖"fork 后父子并发写 CoW 页"。

**依赖**：无 E1/E2 硬依赖（机制可先行 + 单测），但验收（E5 SMP 冒烟）前置 E1/E2；与 01-stage-kernel 的 SMP 工作窗（smp_gpt 系列设计迭代）协同排期。

> **复核（2026-09-15，现状更新）**：「grep 零命中」已失效——`MiscFlagsBits::FLUSH_TLB` 已定义（proc.rs:181/:206 别名）且被 `vmctl_vminhibit_set` 设置（syscall.rs:2327），但全内核无消费者（dispatch/restore 路径不读该标志；lib.rs:2582-2584 自注 single-CPU parity 省略；vmctl_vminhibit_clear 注释 :2336 自述 "stale TLB fill not yet implemented"）。同根跳过优化本体已迁至 lib.rs:2534-2555（:2550-2552）。剩余工作不变：调度点消费（刷新先于同根比较）+ 设置点对账 + 单测。

---

## E-REQWIRE REQ_* VFS↔FS 消息契约双侧独立定义（05-stage-vfs R2 轮登记，2026-09-09）

**问题**：VFS 侧与 FS 驱动侧对同一 REQ 协议各自独立定义，之间无任何编译期或测试期联动：

- VFS 侧：`os/servers/vfs/src/request.rs:15` `FS_BASE=0x600`（**错值**，C 为 0xA00，止血见 05-stage-vfs/todo.md §9.2 R2-P0-1）+ `enum FsReq` 32 变体（request.rs:122）+ `m_type()` 编码（:318）。
- FS 侧：`os/libs/minix-fs/src/protocol.rs:25` `FS_BASE=0xA00`（正确）+ `enum RequestNumber`（:58）+ `TransactionId`（:226）；mfs 经 `use minix_fs::protocol::RequestNumber` 消费（os/fs/mfs/src/table.rs:12），pfs 同样引用 minix-fs。
- `os/servers/vfs/Cargo.toml` 不依赖 minix-fs（grep 零命中）；两侧数值当前纯靠人工对齐——偏移对齐（26=Lookup 两侧一致）掩盖了基址分叉，R2-P0-1 就是该模式的第一个实际事故。transid 侧机制相容（VFS fs_comm.rs:30-56 的 0xB00/0xB01 与 minix-fs TransactionId 的 `(type<<16)|id` 打包），但 id 语义未联动约束。

**为何 edge**：edge 判定①共享契约/基础设施层——minix-fs 是 FS 驱动族（mfs/pfs 已消费）的共享 crate，REQ wire 是 VFS 与全部 FS 驱动的双向契约；收敛动作落在单个 stage 之外。

**解锁后工作**：
1. 方案 A（推荐）：REQ face 迁 minix-types（先例：PM↔VFS 面 `minix-types/src/ipc/vfs.rs`）——`FS_BASE`/`REQ_*` 常量、请求/响应类型化编码收一处；vfs 与 minix-fs（可 re-export）共消本地定义；沿用 rs_start_t 先例的 offset_of 布局见证 + 编译期断言。
2. 方案 B（最低限度）：保留双侧定义 + 全量对账测试——断言 `FsReq::m_type()` 与 `RequestNumber` 逐项相等、`FS_BASE` 相等、transid 打包一致。
3. 附带：request.rs:120 的 "33 variants" 计数注释随迁移/对账修正（05-stage-vfs todo R2-P3-1）。

**依赖**：无硬前置（方案 B 的对账测试立即可写）；执行顺序与 05-stage-vfs R2-P0-1 协调——先在 VFS 侧止血基址，再做本条结构收敛。验收挂钩 E5：VFS↔mfs 第一条真实 REQ 往返（REQ_READSUPER）即本条验收面。

> **复核（2026-09-15，数值半已愈）**：R2-P0-1 已修（05-stage-vfs/todo.md §9.9 Fix #1，2026-09-09）——vfs request.rs:21 与 minix-fs protocol.rs:25 均 0xA00，两侧各有 C 绝对值 pin 测试（vfs `test_fs_wire_values_match_c_absolute` :569 / minix-fs protocol.rs:474-497）。剩余为**结构半**：仍无双侧逐项对账测试、vfs 不依赖 minix-fs、devman 第三消费方（见下方增补）待接；"33 variants" 注释与实际 32 变体的计数修正随迁移一并做。

> **增补（2026-09-15，11-stage-devman 首轮架构审查）**：devman 是本契约的**第三消费方**——其内联 VTreeFS（os/servers/devman/src/vtreefs/mod.rs:50-56）自注"生产分类器（raw IPC → Request）需要 VFS 侧 wire 布局（fsdriver_data、REQ_* 字段宏），lands with the IPC transport"，dirent/stat 的 wire 编码同样声明"属传输层"（mod.rs:41-42/:198）。该传输层就是本条的 REQ_* 契约域：REQ 收敛到 minix-types（方案 A）后，devman 的分类器与 dirent/stat 编码直接消费同一权威，不再自建。登记于 E-DMWIRE 第 2 缺，wire 权威以本条裁决为准。

---

## E-SCHEDNICED kernel 侧 SYS_SCHEDULE 丢弃线上 niced 字段（06-stage-sched V2 轮登记，2026-09-09）

**问题**：C 的 do_schedule 从 SYS_SCHEDULE 消息读 niced（`niced = !!(m_ptr->m_lsys_krn_schedule.niced)`，minix3/minix/kernel/system/do_schedule.c:27），sched_proc 据此写 MF_NICED（system.c:692-694）；该标志在时钟中断的 CPU 记账分类中消费（minix3/minix/kernel/arch/i386/arch_clock.c:318 `else if (p->p_misc_flags & MF_NICED) counter = CP_NICE`）。Rust 内核的 dispatch_schedule 读出线上 niced 后丢弃、硬编码 false（os/kernel/src/syscall.rs:923 `let niced = false;`），注释把理由归于"SYS_NICE is not yet wired up"——但 **SYS_NICE 在 C 树中不存在**（rg 全树仅 system.h:12 的 2005 年变更日志提到 nice(2) kernel call），C 的真实数据源就是 SYS_SCHEDULE 消息本身。SCHED 服务器每次 fanout 都诚实发送 niced（os/servers/sched/src/kernel_api/schedule.rs:152-154 `wire_niced`，值取 `is_niced(max_priority)`，同 C schedule.c:319 公式）；kernel 侧 sched_proc 的落盘机制已备（os/kernel/src/sched.rs:413-423 Step 8 写 `MiscFlagsBits::NICED`，测试 sched.rs:649-657）——断环只在 dispatch_schedule 一行。另有下半：Rust 的 clock.rs 记账没有 CP_NICE/CP_USER/CP_SYS 分类（rg "niced" os/kernel/src/clock.rs 零命中），即使传值也无人消费——修复是"接线 + 消费端评估"两半。

**影响**：niced 进程的 CPU 记账归入 CP_USER 而非 CP_NICE（utilization/cpuavg 统计失真）；无调度行为影响（C 的 MF_NICED 不参与抢占与队列决策）。

**为何 edge**：kernel 生产代码（01-stage-kernel 域，edge 判定②）；SCHED 侧 wire 半已正确、零改动。

**解锁后工作**：
1. dispatch_schedule 改 `let niced = sched.niced != 0;` 传入 sched_proc（os/kernel/src/syscall.rs:923），删除"SYS_NICE"注释、改引 do_schedule.c:27（连带修正 sched.rs:286-294 SchedParams 文档里对 "SYS_NICE" 的同款虚构引用）；
2. 评估 clock.rs 记账是否补 CP_NICE 分类（对照 i386 arch_clock.c:315-319 的 counter 选择链）；若 Rust 记账面尚无此分类，先登记不实现——MF_NICED 位本身先正确落盘；
3. 测试：dispatch_schedule 收 niced=1 的 SYS_SCHEDULE → 目标进程 NICED 位置位（sched_proc 半已有测试，缺 dispatch 半的翻转）。

**依赖**：无 E1/E2 依赖（纯内核侧行为 + 单测可验）。
**解锁**：12 篇契约（06-stage-sched）的 niced 半闭环；06-stage-sched/todo.md P2-3 指针表的对应行。

> **复核（2026-09-15，下半已闭合）**：clock.rs 记账半已落地——`CP_NICE`（clock.rs:451）、`classify_cpu_state` 读 `MiscFlagsBits::NICED` 归桶（:479，测试 :1527）、`sched_proc` 真实置/清 MF_NICED（sched.rs:418-421，测试 :649）。剩余仅上半接线：dispatch_schedule 的 `let niced = false;`（syscall.rs:923）与 `let _niced = sched.niced;`（:893）换真值；`classify_cpu_state` 尚无生产调用点（lib.rs 生产路径只记 CP_INTR）——接线时一并评估，不接则登记理由。sched.rs:286-294 的 "SYS_NICE" 虚构引用仍在。

---

## E-PREEMPTFLAG PREEMPTIBLE 特权标志缺失——priority != 0 近似当成了语义（06-stage-sched V2 轮登记，2026-09-09）

**问题**：C 有两处调度决策以特权标志 PREEMPTIBLE 为门：(1) quantum 耗尽只对"用户调度 + PREEMPTIBLE"进程通知调度者（minix3/minix/kernel/proc.c:1895 `if (!proc_kernel_scheduler(p) && priv(p)->s_flags & PREEMPTIBLE)`）；(2) enqueue 抢占只对 PREEMPTIBLE 的当前进程生效（proc.c:1638）。Rust 内核两处都以 `priority != 0` 近似（os/kernel/src/proc_table.rs:766 `let pre = p.get_priority().get() != 0;`；:674（enqueue Phase 3 元组第三元素的 `cur.get_priority().get() != 0`））；连 sched.rs:233 的 `is_preemptible` 助手——文档声称"C: priv(p)->s_flags & PREEMPTIBLE"——实现也是同一优先级近似，且 `#[allow(dead_code)]` 未接线。近似与 C 语义的分叉点：MAX_USER_Q == TASK_Q == 0（minix3 config.h:67-68，用户进程可合法登顶队列 0），SCHED 接管的进程经 START/NICE 可达 priority 0（os/servers/sched/src/scheduling/start.rs:116 的门放行 0..15）——此后 quantum 耗尽走"内核调度者续量"分支，**SCHED 永远收不到 NO_QUANTUM，该进程被无限续量**，MLFQ 的降级臂与平衡回升臂对它双双失效；C 下它仍走 notify_scheduler。

**影响**：priority-0 的 SCHED 接管进程脱离用户态调度策略（不降级、无需回升——它从未降过）；enqueue 抢占对 priority-0 的当前进程永不发生（同因）。单核现状即可达，非 SMP 专属。

**为何 edge**：kernel 生产代码（01-stage-kernel 域，edge 判定②）；PRIV_TABLE 的标志位归 kernel 域。

**解锁后工作**：
1. priv 表补 PREEMPTIBLE 位（对照 C 的 USER_PRIV 模板：用户进程默认置位、内核任务缺省；落点 os/kernel/src/priv_table.rs + 进程初始化路径）；
2. `is_preemptible` 改读特权标志（os/kernel/src/sched.rs:233），接线到 sched_proc_no_time（proc_table.rs:766 替换优先级近似）与 sched_enqueue Phase 3（:674）；
3. 文档修正：sched.rs:231 的声称与实现对齐（消除"文档说 priv、代码读优先级"的漂移）；
4. 测试：priority-0 + 用户调度 → proc_no_time 走通知分支（现测试 test_proc_no_time_user_scheduled_preemptible 用 USER_Q=7，未覆盖 0）；enqueue 抢占对非 PREEMPTIBLE 当前进程的抑制。

**依赖**：无 E1/E2 依赖（特权位 + 单测可先行）。
**解锁**：SCHED 的 NO_QUANTUM 契约对所有优先级成立；E5(e) 冒烟的"断环分界"用例转正。

> **复核（2026-09-15，前提已变）**：PREEMPTIBLE 位**已存在**——capability.rs:69-70（0x0000_0002，已入 SRV_F/USR_F 模板）+ kpriv.rs:164-173 的 `KPrivFlags::is_preemptible()`（`#[allow(dead_code)]`，注释自述 "scheduler preemption check not yet wired"）+ KPriv 包装（:491-495）。原「步骤 1 priv 表补位」作废；余下：两处优先级近似（proc_table.rs:679 enqueue Phase 3、:771 sched_proc_no_time，锚点从 674/766 漂移）改读特权标志、sched.rs:232-233 助手接线并修文档漂移、补 priority-0 + 用户调度用例与非 PREEMPTIBLE 抑制用例。

---

## E-SCHEDSMP SCHED cpu 下发链的 SMP 三件套缺口（01-stage-kernel SMP 工作窗，06-stage-sched V2 轮升级登记，2026-09-09）

**问题**：06-stage-sched 第一轮 todo 的"边界外观察"第 2/3 条（观察无去向即丢失的实例），本轮升级登记。SCHED 服务器的 CPU 选择与下发（10/09 篇）在内核侧断三环：
1. 每核运行队列不存在——`sched_for_cpu`/`sched_for_cpu_mut` 恒返 BSP 队列（os/kernel/src/proc_table.rs:573-586，TODO 自注）；SCHED 下发的 cpu 字段被 dispatch_schedule 存入 p_sched.cpu（sched.rs:407-411）但 pick/enqueue 不按它分发；
2. EBADCPU 不可达——sched_proc 的 CPU 校验是桩（os/kernel/src/sched.rs:355-360 `let _ = params.cpu;`；C: system.c:650-654 的 cpu_is_ready 检查），SCHED 服务器侧的 EBADCPU 重试环（os/servers/sched/src/server.rs:391-399，C schedule.c:227-231）在当前内核上永不触发；
3. 跨 CPU 迁移未接线——sched.rs:308-315 文档声称"dispatch_schedule calls schedule_migrate_proc when the CPU changes"，但 dispatch_schedule 无该调用（C: system.c:673-677 `p->p_cpu != cpuid && cpu != -1 && cpu != p->p_cpu → smp_schedule_migrate_proc`）；schedule_migrate_proc 本体已实现（os/kernel/src/smp.rs:699）。

**影响**：单核下全部不可观察（cpu 恒 0、单队列恒正确）；多核落地时 SCHED 的负载均衡与死核规避（pick_cpu/balance 的全部产出）对内核无效。

**为何 edge**：kernel 生产代码（01-stage-kernel 域，edge 判定②），且与 01-stage SMP 工作窗（smp_gpt 系列设计迭代）协同排期。

**解锁后工作**：随 01-stage-kernel SMP 落地一并——(1) per-CPU Scheduler 入 CpuLocal，sched_for_cpu 按 cpu_id 分发；(2) sched_proc 补 cpu_is_ready 校验（EBADCPU）与迁移调用（对照 system.c:650-654 / :673-677）；(3) SCHED 侧零改动（重试环已按最终形态写好并有测试 `test_start_retries_after_dead_cpu` 钉住）；(4) 联调验收挂 E5(e)。

**依赖**：01-stage-kernel SMP 工作窗；E5(e) 验收。
**解锁**：06 篇重试环与 10 篇 pick 的生产语义；S-11 组合层在全拓扑下的行为完备。

---

## E-MINTYPES-SYS SYS_* kernel-call 调用号常量 minix-types 缺位（06-stage-sched V2 轮登记，2026-09-09，低优先）

**问题**：kernel-call 调用号（C com.h:210-262 一族）目前多处各自表达：minix-types 无 SYS_* 常量（grep 零命中）；SCHED 服务器本地镜像两个（os/servers/sched/src/kernel_api/transport.rs:41 `SYS_SCHEDULE = 0x600 + 3`、:45 `SYS_SCHEDCTL = 0x600 + 54`，注释自述 "minix-types' absence"）；kernel 侧以枚举成员相对 KERNEL_CALL 表达（os/kernel/src/syscall.rs:69 `Schedule = 3`、:114 `Schedctl = 54`）。minix-sys 的 SYS_* wrapper 家族（E2/E6/E8）落地时还会再添消费方。与 06-stage-sched/todo.md P2-3（SUSPEND 本地定义）同型：com.h 常量一个走共享镜像、一个各处手抄。

**影响**：数值当前有 wire 断言测试钉住，无漂移事故；成本是每新增一个 SYS_* 消费 crate 就多一份手抄对账对象。

**为何 edge**：minix-types 是共享契约层（edge 判定①）；收敛动作跨 kernel/sched/未来 crate。

**建议**：minix-types 补 SYS_* 调用号常量模块（对照 com.h:210-262 全族，先例：types/com.rs 的 SCHEDULING 系常量镜像），kernel 枚举与 SCHED 镜像改消费 + 删本地定义；与 P2-3 的 SUSPEND 上移合并为一轮 minix-types 常量收敛。

> **增补（2026-09-14，07-stage-ds 首轮架构审查）**：DS 段两件同型并入本轮收敛——(a) `SI_DATA_STORE = 5`（sysinfo.h:13）现本地定义于 os/servers/ds/src/getsysinfo.rs:27，11 篇 D2 自注「minix-types 暂无 sysinfo 模块（A-1 余部）」；(b) `NOTIFY_MESSAGE = 0x1000`（com.h:92，is_notify 的常量基）现硬编码于 os/servers/ds/src/dispatch.rs:94-96 的 is_notify 实现。DS 侧消费面（getsysinfo.rs / dispatch.rs）随 minix-types 落地改走共享常量并删本地定义。
>
> **现状更新（2026-09-15，10-stage-mib 首轮架构审查复核）**：(a) **半解**——minix-types 已建 sysinfo 模块并有 `SI_DATA_STORE`（os/libs/minix-types/src/ipc/sysinfo.rs:137-138），但 ds 侧本地定义仍在（getsysinfo.rs:26-27）且 server.rs:1058 消费的是本地常量，切换未做；(b) **维持**——minix-types 仍无 `NOTIFY_MESSAGE` 常量（仅 ipc/notify.rs:28 文档注释提及），ds dispatch.rs:94-96 仍内联 `0x1000`。

**依赖**：无。
**解锁**：06-stage-sched/todo.md P2-3；E2/E6/E8 wrapper 的常量消费面。

> **复核（2026-09-15，主体半解）**：minix-types `ipc/sysinfo.rs` 已有 SYS_* 常量模块——SYS_GETINFO/SYS_DIAGCTL/SYS_SETALARM/SYS_TIMES/SYS_SAFECOPYFROM/SYS_SAFECOPYTO/SYS_VSAFECOPY/SYS_SETGRANT/SYS_EXIT/SYS_STATECTL/SYS_SAFEMEMSET（:17-59）+ SI_DATA_STORE（:138）。「minix-types 无 SYS_* 常量」主体判定作废，剩余收敛面：SYS_SCHEDULE/SYS_SCHEDCTL 等缺号对照 com.h:210-262 补齐；NOTIFY_MESSAGE 仍无常量（仅 ipc/notify.rs:28 注释提及）；四处本地定义/内联待切换删除——sched transport.rs:41/:45、ds getsysinfo.rs:27（server.rs:1058 消费本地常量）、ds dispatch.rs:95 内联 0x1000。

---

## E-DSWIRE DS 传输面三缺：minix-sys 客户端模块、服务器 transport 通电、联调零覆盖（07-stage-ds 首轮架构审查登记，2026-09-14）

**问题**：DS 判定层（plan/apply/verdict，os/servers/ds 18 文件 3572 行、81 个在跑测试）已成形，但执行半整层缺席，三个落点分属不同域：

1. **minix-sys 无 DS 客户端模块**（edge 判定①）。C 的 libsys/ds.c（minix3/minix/lib/libsys/ds.c，219 行，18 个 API：ds_publish_u32/str/mem/label、ds_retrieve_u32/str/mem/label_name/label_endpt、ds_delete_u32/str/mem/label、ds_subscribe、ds_check，加上 do_invoke_ds 的 grant 生命周期 ds.c:7-33）是**全体 DS 消费方的公共入口**（RS manager.c:513,800、VFS main.c:441 + misc.c:960-985、input.c:665、storage/filter main.c:385、i2c.c:452、IS dmp_ds.c）。minix-sys 现有 13 个模块 6284 行（vm/vfs/rs/pm/socket 等）但**无 ds.rs**（ls os/libs/minix-sys/src/ 实测）。plan.md:182 的 A-8 描述已过时（"minix-sys 是 stub，sendrec/notify 为 todo!()"——现 grep todo!/unimplemented 零命中）。服务器侧契约镜像已就位（os/servers/ds/src/client.rs：key_grant 尺寸/方向 ds.c:13-19、terminate 钉尾 ds.c:84/:155、flags 组装、CheckReply 回信复用 ds.c:209-219），缺的只是 minix-sys 的传输半。附带：os/servers/ds/Cargo.toml 声明了 minix-sys 依赖但 src 零引用（grep 零命中，死依赖——A-8 落地时转正或删除）。
2. **DS 服务器 transport 未通电**（edge 判定②+①复合）。main.rs:27-28 是空转 loop；C 主循环（main.c:45-88：get_work → is_notify 拒绝 → 七路分派 → reply/EDONTREPLY）的收发装配、get_key_name 的 sys_safecopyfrom（store.c:167-172）、do_getsysinfo 的 sys_datacopy 整表搬运（store.c:672-675）、update_subscribers 的 ipc_notify 发送（store.c:222）、sef_cb_init_fresh 的 rproctab grant 拉取（store.c:267-269）——Rust 侧全部只有判定半（dispatch::triage、plan_*/apply_*），缺 seam trait + 装配线（SCHED 先例：IpcTransport + KernelApi 双 trait + MockIpc/MockKernel，os/servers/sched/src/server.rs）。其中 sef_receive/send 的用户态 wrapper 归 minix-sys（E1/E2 面），safecopy/datacopy 的内核对端归 kernel（E2 家族）——两处对端就绪前，DS 侧以 seam + mock 达到「逻辑完备」，真实通电挂本条。
3. **联调零覆盖**（edge 判定③）。C 的行为契约 minix3/minix/tests/ds/（dstest.c 178 行 + subs.c 91 行）无 Rust 承接 → E5(f)。

**影响**：服务器不能服务（空转自白，main.rs:25-26 注释「must not pretend otherwise」）；全部消费方无客户端库可用；与 A-2 叠加时 VFS 同款订阅场景（main.c:441）双缺同根。

**为何 edge**：① minix-sys DS 模块是 VFS/PM/RS/驱动/IS 全体消费方的共享契约层；② safecopy/datacopy/notify 的内核对端是 01-stage-kernel 生产代码；③ 多进程联调。DS stage 内生产代码（seam trait、装配线、mock 测试）不属本条——留 07-stage-ds/todo.md P1-4。

**建议**（执行序）：
- 方案 A（推荐，沿 SCHED 先例）：07-stage 先落 transport seam（IpcTransport 形 trait + 脚本化 mock 测试，stage 内 P1-4）；随后 minix-sys 补 ds.rs（消息构造 + `_taskcall` + grant 生命周期，复用 client.rs 三契约）；E1/E2 通电后接真实传输收尾本条。
- 方案 B（否决）：minix-sys ds.rs 只做纯函数消息层、传输无限期留置——Rust 侧消费方（VFS/IS 重写）仍无法编译，违背「各 stage 可独立完整实现」。
- 传输基建上移评估（06-stage-sched/todo.md V2 §4.1 预留的触发点）：DS 将成为第 4 个用户态服务器 seam 消费者；本轮判定**暂不上移**（四台服务器的 seam 形状各异且 DS 面最窄，纯 receive/send + safecopy），等 E1 trap 层落定后随本条执行时再评估一次公共 UserSpaceTransport 抽取。

**依赖**：seam + mock 阶段无依赖；真实传输前置 E1/E2。
**解锁**：07-stage-ds/todo.md P1-4 执行半；E5(f) 联调；A-2 regex 决策的端到端验证面；minix-sys 死依赖处置（ds crate Cargo.toml）。

> **进度（2026-09-15，07-stage-ds 执行轮）**：DS 侧 seam 已落地（`server.rs` 双 trait + 七臂 + run/run_once，`heap.rs` A-3 固定池），真实端 `SysIpc` 委托 minix-sys `IpcTransport`（E1 通电即活）、`SysKernel` 三拷贝动词 EIO 诚实桩（等 minix-sys 补 SYS_SAFECOPY*/SYS_DATACOPY 包装）；minix-types MessageUnion 已补 `m_ds_req`/`m_ds_reply` 两臂（IS 轮同款先例）。ds crate 的 minix-sys 死依赖已转正（server.rs 真实端消费）。**剩余**：minix-sys ds.rs 客户端模块（libsys/ds.c 18 API）、E1/E2 通电、E5(f) 联调。
>
> **增补（2026-09-15，10-stage-mib 首轮架构审查）**：新增消费方——MIB 服务器的 `mib_get_label`（C remote.c:88 `ds_retrieve_label_name`）是远程子树注册的第一步；os/servers/mib/src/remote.rs:204-209 目前只有 label 界判定（ENAMETOOLONG），DS 查询执行半随本条 minix-sys ds.rs 落地后接线（10-stage-mib/todo.md P1-1 的 `MibServices` seam 消费）。

> **增补（2026-09-15，11-stage-devman 首轮架构审查）**：新增消费方——devman 驱动侧客户端的 `init`（minix-sys/src/devman_client.rs:135-137，注入闭包形式）与 RS 侧 `RsTransport::devman_endpoint`（os/servers/devman/src/rs_contract.rs:24-26）都依赖 `ds_retrieve_label_endpt("devman")`（C generic.c:193 / manager.c:841/898）；两处的生产接线随本条 minix-sys ds.rs 落地（E-DMWIRE 第 4 缺消费）。

---

## E-ISWIRE IS 生产 transport 接线：minix-sef/minix-sys 替换 fail-closed 占位（08-stage-is V1 轮登记，2026-09-14）

**问题**：IS 的生产传输面全部未接——`os/servers/is/src/main.rs:17-23` 用 `IsServer::new(UnimplementedTransport, UnimplementedFkeyCtl)` fail-closed（运行即 panic，注释自述接线 pending）；`SefTransport` 的 `startup/receive/send`（os/servers/is/src/sef.rs:128-139）与 `FkeyCtlTransport`（os/servers/is/src/tty_fkey.rs:178-182）只有 panic 占位实现。依赖侧：`os/libs/minix-sef` 是 5 行占位 crate（`sef_startup`/`sef_receive` 的 ping 透明拦截逻辑——A-11 的 `sef.c:208-214` 语义——无着落）；`minix-sys` 的 `_taskcall` 属 E2/E6 wrapper 家族。

**影响**：IS 无法出生（main 直接 panic）；86 个单测全部跑在 fake 上，生产语义零验证。

**为何 edge**：minix-sef/minix-sys 是共享基建（edge 判定①）；SEF ping 拦截与 `_taskcall` 由多个 server 共享，IS 只是首个深度消费方。

**解锁后工作**：(1) minix-sef 实装 `sef_startup`/`sef_receive`（含 SEF_PING_REQUEST_TYPE 拦截应答，对照 `minix3/minix/lib/libsys/sef.c:150,208-214` + `sef_ping.c:21`）；(2) minix-sys `_taskcall` 可用（依赖 E1/E2）；(3) 写 `SefTransport`/`FkeyCtlTransport` 生产 impl 并替换 main.rs 占位（IS 侧是 stage 内生产代码，随本条目一并做）；(4) 重跑 `cargo test -p minix-is` 基线对照 86 passed + 复原 V1-P2-1 删掉的 minix-sys 依赖。

**依赖**：E1（trap 层）、E2（SYS_* wrapper）。
**解锁**：E-ISBOOT；08-stage-is/todo.md V1-P2-1 的复原。

> **增补（2026-09-15，10-stage-mib 首轮架构审查）**：minix-sef 的第二消费方浮出——os/servers/mib/src/sef.rs:13-14 声称 SEF transport 留在 minix-sef，但 mib crate 的 Cargo.toml 未声明该依赖；MIB 侧 `sef_startup`/`sef_receive_status` 接线（10-stage-mib/todo.md P1-1 的 `MibIpc`，含 status 字的 notify 检测——与 DS 的 call-number 猜测路径不同）随本条 (1) 一并做。

> **增补（2026-09-15，11-stage-devman 首轮架构审查）**：minix-sef 的**第三消费方**是 devman——`SefHooks` trait（os/servers/devman/src/hooks.rs:268-271：init_server/on_signal）与 `SefLifecycle` 枚举（:257-263）目前只有测试替身实现（hooks.rs:336-365），生产实现被 STATE.md OQ-1 挡在 minix-sef 门前（hooks.rs:255 自注 "minix-sef is currently a stub"）。C 参照是 vtreefs.c:54-59 的三注册 + sef_local_startup（devman 侧经 libvtreefs 间接消费 libsys/sef）。devman 侧 `SefHooks` 生产 impl 随本条 (1) 一并做；若 minix-sef 长期不到场，11-stage-devman/todo.md DM-P1-2 的分派面统一将连带评估该 trait 的去留（模式 80：无生产实现者的占位抽象）。

---

## E-ISPROD GETSYSINFO/GET_*/VM_INFO producer 布局与 IS 快照对齐（08-stage-is V1 轮登记，2026-09-14）

**问题**：IS 六组 `#[repr(C)]` 快照（`KProcSnap`/`KPrivSnap`/`MProcSnap`/`FProcSnap`/`DmapSnap`/`RprocpubSnap`/`RprocSnap`/`DsEntrySnap`/`Vm*Snap`，A-4 wire 契约提案）与现存 producer 是双源，其中 kernel 侧已坐实二进制不兼容：kernel `ProcInfoStruct`（os/kernel/src/misc.rs:371 起，GET_PROCTAB 生产布局：p_nr 打头、时间字段 u64、含 p_misc_flags/p_cpu/p_cpu_time_left/p_cycles/p_pending）与 IS `KProcSnap`（os/servers/is/src/dump_kernel.rs:28：p_rts_flags 打头、时间字段 i32、无上述五字段）字段序、字段集、类型三重不一致。PM/VFS/RS 的 `do_getsysinfo` 生产侧布局同样待对账（IS 侧 6 个 TODO(P1) 注释：dump_pm.rs:13、dump_vfs.rs:13、dump_rs.rs:13、dump_ds.rs:13、dump_vm.rs:12、dump_kernel.rs:16）；DS 生产侧已有模块（os/servers/ds/src/lib.rs:70）。附带一处常量双源：os/servers/vfs/src/misc.rs:66 本地定义 `SI_PROC_TAB: u32 = 2`，与 minix-types 权威（i32）重复。

**影响**：`run_dump` 填体（08-stage-is/todo.md V1-P1-2）后，IS 按 `KProcSnap` 解释 producer 拷来的字节会全盘错位——这是行为级事故而不只是卫生问题。

**为何 edge**：对端 stage 的生产代码（edge 判定②）：kernel `do_getinfo` 的输出布局、PM/VFS/RS/DS 的 `do_getsysinfo` payload、VM 的 `vm_info` 三结构都归各自 stage 所有权。

**解锁后工作**：(1) 先裁决快照权威：上收 minix-types 单一权威（kernel `ProcInfoStruct` 迁入 + IS 删 `KProcSnap` 改 import）vs 各 crate `#[repr(C)]` 对齐约定 + wire 断言测试互钉（两案需在 02-stage-vm/04-stage-pm/05-stage-vfs/03-stage-rs/07-stage-ds 各 todo 留同步指针）；(2) kernel GET_PROCTAB/GET_KINFO/GET_IMAGE/GET_PRIVTAB/GET_IRQHOOKS/GET_IRQACTIDS/GET_MONPARAMS/GET_MACHINE 八臂布局对齐；(3) PM/VFS/RS/DS/VM 五 producer 逐一对齐 + 删 VFS 本地 SI_PROC_TAB；(4) 双侧补布局断言测试（`size_of` + 字段偏移，minix-types tty.rs:103 的 `_ASSERT_MSG_SIZE` 先例）。

**依赖**：无硬依赖（裁决可先行）；实施建议在 E-ISWIRE 之后（有真实 IPC 才能端到端断言）。
**解锁**：08-stage-is/todo.md V1-P1-2 的实施半（设计半不依赖本条）。

---

## E-ISKMESS A-3 的 GET_KMESSAGES 等价通道：kernel 侧新子请求（08-stage-is V1 轮登记，2026-09-14）

**问题**：C 的 `kmessages_dmp` 经 `.usermapped` 直读内核消息环形缓冲（`get_minix_kerninfo()->kmessages`，dmp_kernel.c:71）；minix-rs 64 位不移植 `.usermapped`（01-stage-kernel/28-usermapped-data.md 既定），04 篇 A-3 设计定为「新增 `GET_KMESSAGES` 等价 `sys_getinfo` 子请求（kernel 拷贝环形缓冲）」。现状：kernel 侧无该子请求、minix-types 无 GET_KMESSAGES 常量（grep 实测双零命中）；IS 侧 `KerninfoTransport::kmessages_available` fail-closed panic（os/servers/is/src/acquire.rs:195-199）。消费面已备好：`KmessagesSnap`/`kmess_start`/`KMESS_BUF_SIZE`（dump_kernel.rs:99-110/264-272）。

**影响**：F7（内核消息转储）永远不可用；acquire.rs 的 fail-closed 是 86 测试里唯一锁住「通道缺失」的活 panic 点。

**为何 edge**：kernel `do_getinfo` 新增子请求是 kernel 生产代码（判定②），minix-types 增常量是共享契约（判定①）。

**解锁后工作**：(1) minix-types `ipc::sysinfo` 增 `GET_KMESSAGES` 常量（值需对照 kernel com.h GET_* 未用号段裁决，缺号注释体例同 04 篇 §2.1）；(2) kernel `do_getinfo`（os/kernel/src/misc.rs）增臂：拷贝 kmessages 环形缓冲到调用方（布局含 km_next/km_size + 缓冲体，对照 05 篇 §4.1 快照）；(3) IS `KerninfoTransport` 填实（建议签名扩为数据出口形态，与 08-stage-is/todo.md V1-P1-2 的方案①同型）；(4) F7 链路单测。

**依赖**：E2（`_kernel_call` wrapper）；与 E-KERNINFO（kerninfo 共享家族）交叉引用——按 04 篇既定决策走 sys_getinfo 子请求，不依赖 MINIX_KERNINFO 映射机制。
**解锁**：08-stage-is DumpId::Kmessages 臂。

> **复核（2026-09-15）**：IS 侧锚点更新——fail-closed panic 现于 acquire.rs:289（should_panic 测试 :644-645）；kernel 侧与 minix-types 的 GET_KMESSAGES 仍双零命中（仅 syscall.rs:2591/2604-2609/3828 关于 kmess 缓冲已移除的注释）。

---

## E-ISBOOT IS 启动链与端到端联调：条件启动 + RS 动态加载 + TTY 观察者（08-stage-is V1 轮登记，2026-09-14）

**问题**：全仓库零消费方引用 minix-is（`rg -rln "minix_is|minix-is" os` 实测仅 crate 自身与 Cargo.lock）——IS 没有任何启动路径。C 侧完整链是三件：rc 条件启动（`etc/rc.minix:117` `up -n is -period 5HZ`，仅 `sysenv debug_fkeys != 0`）、RS 运行时加载 + 动态分配 endpoint（A-9，无 IS_PROC_NR）、TTY 侧 fkey 观察者（`drivers/tty` keyboard.c，02 篇 §2.7/§2.8 契约）。minix-rs 侧三件均无着落：rc/system.conf 等价物不存在、RS 动态加载服务的能力归 03-stage-rs campaign、TTY crate 未 staging。

**影响**：IS 的外部行为（唯一的存在理由）零验证；86 个单测只覆盖库内语义。

**为何 edge**：多进程联调（判定③）+ 对方 stage 生产代码（RS 加载面、TTY 观察者面，判定②）。

**解锁后工作**：(1) E5 联调包内立 IS 用例：起 IS → 模拟 TTY notify → F-key 位图 → dump 输出断言；(2) RS 动态加载 IS 的注册面（依赖 03-stage-rs campaign 的 service 加载能力）；(3) rc/system.conf 等价物的 debug_fkeys 条件启动语义（归集成配置层，随 E5 定载体）；(4) TTY crate 落地后补 `do_fkey_ctl` 真实对端（现 FakeTty 镜像退役）。

**依赖**：E5（端到端联调包）、E-ISWIRE、03-stage-rs 动态加载；TTY 观察者面依赖 TTY staging（无条目，遥领）。
**解锁**：IS 外部行为验收（A-11 的 `-period 5HZ` 存活 ping 端到端）。

---

## E-RMIBWIRE minix-sys rmib 客户端协议半整缺（10-stage-mib 首轮架构审查登记，2026-09-15）

**问题**：`os/libs/minix-sys/src/rmib.rs`（259 行）只有簿记半——常量（RMIB_MAX_SUBTREES/STACKBUF/FLAG_AUTH，rmib.rs:24-44）、稀疏查找判定（:70-107）、`MountTable` 槽占用位（:116-179）；`MountTable` 仅 `used: [bool; 16]`，缺 C 槽三元组的另外两半（挂载路径 `rno_name[CTL_SHORTNAME]` + 根节点 `rno_node`，rmib.c:53-57）。C `libsys/rmib.c`（1089 行）的协议半整缺，分四组：(a) 注册/注销/重注册——`rmib_register`/`rmib_deregister`/`rmib_reregister`/`rmib_send_reg`（rmib.c:888-980,862-882，`asynsend3(MIB_PROC_NR, AMF_NOREPLY)` 驱动，注册失败 panic :881）；(b) 请求处理——`rmib_process`（:1036-1089，来源门禁 :1044 + COMMON_MIB_INFO/CALL 分派 + REPLY 回信 :1082-1084）、`rmib_call` 下行遍历（:678-824）、`rmib_query`/`rmib_describe`（:267-380/:414-475）、打包层 `rmib_copyout_node`（剥 CTLFLAG_SPARSE :209-214）与 `rmib_copyout_desc`（:324-380）；(c) grant 拷贝族——`rmib_copyout`/`rmib_vcopyout`/`rmib_copyin`/`rmib_readwrite`（:94-188,652-669，sys_safecopyto/sys_safecopyfrom/sys_vsafecopy）；(d) 纯函数层三件理论可先行但未提供——`rmib_inrange`/`rmib_getoldlen`/`rmib_getptr`（:64-87,:482-509）。消费方：C 侧 ipc（main.c:91/:112/:249）、lwip（mibtree.c:56/:61/:66 + bpfdev.c:136 + rtsock.c:102 + lwip.c:348）、uds（stat.c:174/:185 + uds.c:1408）；Rust 侧 os/servers/ipc-server 死依赖 minix-sys 且文档声称复用 `MountTable` 但代码零引用（`ipc-server/src/mib_tree.rs:16` 注释声称、:18 实际 import 仅 minix-types；13-stage-ipc/03 篇 §2.5/§3 D4/§4.1 三处同款失真，需 13-stage 侧同步修正）。

**影响**：13-stage-ipc 的 kern.ipc 远程注册、17-stage-net 的 lwip/uds 子树注册与转发应答全部无客户端库可用；MIB 服务器侧远程子树挂载（10-stage-mib 12 篇）无对端；E5(g) 的 rmibtest 契约链无承载。

**为何 edge**：edge 判定①——minix-sys 是全体 RMIB 消费方（ipc/lwip/uds/未来驱动）的共享契约层；客户端与服务器的 COMMON_MIB_* 消息构造两侧目前各自为政（客户端 rmib.rs 甚至未 import 这些常量，服务器 remote.rs 用布尔参数替代消息类型判定 remote.rs:179-187），收敛必须落在共享层。

**建议**：
- 方案 A（推荐）：rmib.rs 就地扩协议半，两层推进——纯函数层先行（rmib_call 下行遍历/rmib_query/rmib_describe 的判定可单测，sysctlnode 打包复用 10-stage-mib/todo.md P1-3 的布局锚定产物；`MountTable` 扩为三元组槽，客户端树由各服务自持、槽存路径 + 根索引）；传输半（asynsend3/sendrec wrapper + grant 拷贝）挂 E1/E2 通电。CannedTransport 回放测试对齐 E2/E6 先例。
- 方案 B（否决）：各消费服务自建注册/应答逻辑——C 把 rmib.c 做成公共库正是因为三个服务共用同一协议，复制 ×3 违背单一真值，且 13/17 两个 stage 并行实现必然漂移。

**依赖**：E1（asynsend3/sendrec wrapper 真实通电）；10-stage-mib/todo.md P1-3（sysctlnode/sysctldesc 布局锚定先行）。纯函数层无硬依赖。
**解锁**：13-stage-ipc 03 篇接线及其文档失真修正；17-stage-net lwip/uds 注册面；10-stage-mib 12 篇的远程挂载对端；E5(g) 联调。

> **复核（2026-09-15，前置解除）**：10-stage-mib P1-3 已闭环（minix-types::sysctl_abi：SysctlNode 96B/SysctlDesc 16B/KinfoLwp 128B/KinfoProc2 680B，offset_of 钉死，196 passed）——纯函数层的 sysctlnode 打包锚定已就绪，可立即开工；P1-4（transport 双 trait seam）亦已闭环（126 passed）。本条现状核实：rmib.rs 仍簿记半（无 rmib_register/rmib_process/rmib_call），MountTable 仅 `used: [bool; 16]`；传输半仍挂 E1。

---

## E-MIBPROD MIB 快照消费面 vs kernel/PM/VFS producer 布局对账（10-stage-mib 首轮架构审查登记，2026-09-15）

**问题**：MIB 的进程信息五篇（16~20）消费三张跨服务快照（proc_tab/mproc_tab/fproc_tab，C proc.c:34-38），producer 与 consumer 双侧现状错位：
(a) kernel 侧 GET_PROCTAB/GET_PRIVTAB 已实现 chunked 整表拷贝（os/kernel/src/misc.rs:894/:934，`ProcInfoStruct` 布局 misc.rs:371 起，与 E-ISPROD 对 IS 的警告同源）；MIB 侧快照结构未定（P1-3 裁决前无法对账）。
(b) PM 侧 getsysinfo 的 SI_PROC_TAB 数据路径 fail-closed ENOSYS（04-stage-pm todo D-29 已知），SI_PROCLIGHT_TAB 无 producer。
(c) VFS 侧 fproc_tab producer 无着落。
(d) 服务查询面对端未建/未核实：auth 的 `getnuid`（PM，main.c:265-268）、CTL_VM 子树的 `vm_info_stats`/`vm_info_usage`（VM，vm.c 依赖）、`svrctl(PMGETPARAM)`（PM）；DS label 查询归 E-DSWIRE。
C 的消费侧语义：`update_tables` 每 tick 至多一次 + 失败闩锁（tabs_valid=FALSE 不再重试）+ magic 校验（PMAGIC/MP_MAGIC）——Rust 判定半已备（os/servers/mib/src/proc/tables.rs:35-73），执行半缺（stage 内 P1-5）。

**影响**：P1-5 表拉取执行半无数据源；通电后若双侧布局未对账，MIB 按猜测布局解释 producer 字节 = E-ISPROD 对 IS 警告的同型行为级事故（字段序/字段集/宽度三重错位）。

**为何 edge**：edge 判定②（kernel/PM/VFS 的生产代码归各自 stage）+①（快照布局权威裁决跨 stage，与 E-ISPROD 的裁决是同一次决策）。

**建议**：布局权威裁决与 E-ISPROD 合并一次做（方案 A 上收 minix-types 单一权威 repr(C) 结构 vs 方案 B 各 crate repr(C) + 跨 crate 布局断言互钉——沿用 E-ISPROD 已列两案，不另立第三案）；PM/VFS producer 缺位由本条跟踪、在 04/05 各自 todo 的对应条目闭合后划账。MIB 侧消费结构随 10-stage-mib/todo.md P1-3 裁决产出。

**依赖**：10-stage-mib/todo.md P1-3（布局裁决先行）；无 E1 硬依赖（布局断言测试可先行）。
**解锁**：10-stage-mib/todo.md P1-5（表拉取执行半）；E5(g) 的进程信息用例；E-ISPROD 的裁决复用。

> **复核（2026-09-15）**：10-stage-mib P1-3 已闭环，但产出范围是 **sysctl ABI**（minix-types::sysctl_abi 四结构）——proc_tab/mproc_tab/fproc_tab 的快照布局裁决仍开放，随 E-ISPROD 合并轮一并定；P1-4（transport seam）与 P1-5（Tables 拉取状态机，seam 半）亦已闭环——P1-5 的生产数据源仍随本条对账后接通。

---

## E-MIBGRANT kernel grant.rs magic-grant 门端点常量与 C 不符（10-stage-mib 首轮架构审查登记，2026-09-15）

**问题**：`os/kernel/src/grant.rs:293` `const VFS_PROC_NR: i32 = 4`、`:295` `const MIB_PROC_NR: i32 = 8`，而 C com.h:59-78 的权威值是 VFS=1、MIB=7（MIB_PROC_NR 见 com.h:66）。两值自首次提交（e1d2c4977）即如此，非中途改动；全仓其余各处一律 C 原值（minix-types `types/endpoint.rs:61-72`、kernel `proc.rs:68-118` proc_nr 模块、pm/vm/vfs/ds/sched/ipc-server 各侧）——单点偏差，非系统性重编号。消费点 `grant.rs:546` 的 magic grant 门 `granter.0 != VFS_PROC_NR && granter.0 != MIB_PROC_NR`（对照 C do_safecopy.c:221 同一判断用 VFS=1/MIB=7）：现行值把 SCHED(4) 与 VM(8) 的 grant 误放行、真正的 VFS(1)/MIB(7) grant 被拒。文件内无测试钉这两个值。

**影响**：magic grant 语义反转——VFS/MIB 的 magic grant 会被拒（MIB 的 remote relay 三 grant 通电后必然失败，阻塞 10-stage-mib/todo.md P1-4），SCHED/VM 的普通 grant 反而获得豁免待遇（安全面松动）。

**为何 edge**：kernel 生产代码（edge 判定②），修复落点 os/kernel/src/grant.rs；MIB stage 不代修。

**建议**：两常量删除硬编码改用 `minix_types::Endpoint::VFS.get()`/`Endpoint::MIB.get()`（单一真值，endpoint.rs 已有）；补门测试（VFS/MIB endpoint 的 grant 通过、SCHED/VM 不通过）钉死。

**依赖**：无。可独立先行，小时级。
**解锁**：E-MIBPROD 的 grant 链正确性；10-stage-mib/todo.md P1-4（relay 执行半）通电前提。

> **复核（2026-09-15，锚点确认 + 解锁行更新）**：常量在 grant.rs:292-295、门在 :546（分支起 :542），grant.rs 全部 8 个测试无一覆盖 MAGIC 分支——补测试空间确认；minix-types 权威值 endpoint.rs:62 `VFS = Endpoint(1)` / :68 `MIB = Endpoint(7)`，且 `granter` 参数类型即 `Endpoint`（grant.rs:355），门可直接比较；本仓 C 学习笔记 02-stage-vm/22-vm-exit.md:275 同证 VFS_PROC_NR=1。解锁行更新：P1-4 已于 2026-09-15 闭环 seam 半，本条仍是 relay 通电前提。

---

## E-DMWIRE devman 生产接线四缺：server transport、请求分类器、装配半、client/RS 侧生产传输（11-stage-devman 首轮架构审查登记，2026-09-15）

**问题**：devman 语义面 84 个测试全部跑在注入 seam 上（78 server + 6 client），生产执行半整层缺席，四个落点分属不同域：

1. **server 侧生产 transport 不存在**（edge 判定①+③复合）。`main.rs:35-37` 是自旋停车（注释自认 "still `todo!()` — calling it would panic, so park"）；`VTreeFs::run`（os/servers/devman/src/vtreefs/mod.rs:322）与 `Server::handle_other`（server.rs:91）两条分派面的生产传输半都空——`Transport` 只有 VecTransport 测试实现（STATE.md P1-1T），生产 impl 需要 minix-sys 的 receive/send 面（E1 前置）。
2. **请求分类器（raw IPC → `Request`/`DevmanMsg`）不存在**（edge 判定①）。VFS 面的 `Request` 分类需要 FS 驱动协议 wire（REQ_* 字段宏、fsdriver dirent/stat 编码）——该 wire 归 E-REQWIRE 契约域，devman 是消费方；DEVMAN 面的 ADD 还需要 `sys_safecopyfrom` grant 拷贝（C device.c:239-240，add_device.rs:7-9 注释自认"transport business"），内核对端与 wrapper 归 E2 家族。
3. **装配半不存在**（stage 内与 edge 的接缝）。`Server::new` 之后到事件循环之间的装配线（SEF 启动 → mount → 建树 → 循环 → OutAction 执行）没有任何代码承载；其结构定案在 11-stage-devman/todo.md DM-P1-2（双分派面统一）/DM-P1-3（启动序列裁决），stage 内实施不属本条，本条只管"真实通电"。
4. **client 与 RS 侧生产传输**（edge 判定①+②）。`ClientTransport` 的 grant/sendrec 生产 impl（C：cpf_grant_direct + ipc_sendrec，generic.c:112-122）需要 minix-sys 的 grant wrapper 与 E1；`RsTransport` 的生产 impl（C：ds_retrieve_label_endpt + ipc_sendrec，manager.c:841-849）需要 minix-sys ds.rs（E-DSWIRE）+ E1，落点在 RS stage（os/servers/rs/src/publish.rs:39-70 的 devman 臂目前只有决策半）。

**影响**：devman 不能出生（停车自白）；驱动侧 `add_device`/`del_device`/`handle_msg`（minix-sys/src/devman_client.rs:144/:189/:224）全部只能跑 FakeTransport；RS 的 publish/unpublish devman 臂无法端到端验证。

**为何 edge**：① minix-sys 传输/grant/DS wrapper 是共享基建（E1/E-DSWIRE 域）；② RS 侧生产代码归 03-stage-rs；③ 与 VFS 的 wire 契约归 E-REQWIRE。devman stage 内的装配与 seam 实施（DM-P1-2/P1-3）不属本条。

**建议**（执行序，沿 DS 先例 E-DSWIRE 的两段式）：
- 先 stage 内：11-stage-devman/todo.md DM-P1-2 定死 `Server::run` 单分派面 + Reply 错误通道形状，DM-P1-3 定死懒启动序列——传输实现者据此写生产 `Transport`（否则两套分派面的歧义会被带进传输层）。
- 后 edge：minix-sys 侧补 IPC receive/send 与 grant 拷贝 wrapper（E1 通电即活）；devman 侧写生产 `Transport`（classify raw message → Request / DevmanMsg）+ `main.rs` 装配替换停车循环 + `ClientTransport`/`RsTransport` 生产 impl；RS 侧 publish.rs 的 devman 臂接 `RsTransport` 生产端。
- minix-devman-client crate 的处置（删或收编）归 E-DMCLIENT，不阻塞本条。

**依赖**：E1（trap 层）；VFS wire 面 E-REQWIRE；DS 查询 E-DSWIRE；grant 拷贝 wrapper 归 E2 家族（SYS_SAFECOPY* 同款先例）。
**解锁**：E5(h) devman 生命周期联调；STATE.md backlog P1-6/P1-1T/P1-10/P1-12 全部关单；12-gpio-devman 的驱动注册链真实化。

---

## E-DMCLIENT minix-devman-client 孤儿 crate 处置：与 minix-sys 客户端职责重叠（11-stage-devman 首轮架构审查登记，2026-09-15）

**问题**：`os/libs/minix-devman-client/`（3 文件 395 行，6 测试）在仓库里没有消费者：workspace 成员注册（os/Cargo.toml:194）之外，无任何 Cargo.toml 依赖它，无任何 .rs 引用其符号（grep 全仓实测）。它声称的职责是"驱动侧设备记账 + USB 跟踪 + 序列化尺寸"（device.rs:1-8、usb.rs:1-8），但 doc 10/11 钦定的正式实现在 `minix-sys/src/devman_client.rs`（encode_device 字节兼容 serialize_dev + ClientTransport + handle_msg）与 `usb_model.rs`（UsbDevice/属性生成/add_usb/remove_usb）——两侧存在概念级重叠（device.rs `Registry`/`DeviceRecord` vs devman_client `ClientDevice`；usb.rs `UsbDevice`/`UsbTracker` vs usb_model `UsbDevice`；device.rs `serialized_size` vs encode_device 整体编码），且孤儿侧功能是子集（无 wire 编码、无 IPC）。唯一的外部引用是 16-stage-drivers 的评审文档 `notes/rewrite/fork-syscall-rewrite/16-stage-drivers/12-gpio-devman.md:200` 把它列为测试目标（`cargo test -p minix-driver-gpio -p minix-devman-client --lib`），但 gpio 驱动 crate（os/drivers/system/gpio）的 Cargo.toml 并不依赖它——连带 gpio 的 lib.rs:5-6 文档注释字面写的"see the `minix-devman` crate"指向的是 **server crate 的包名**（驱动依赖服务器 crate 是分层违例），实际意图应是 minix-sys 的 devman_client 模块；引用关系三方（孤儿 crate、server 包名、正式客户端模块）纠缠不清。

**影响**：第二真相源存活——未来驱动作者搜"devman client"会先撞到孤儿 crate，按它记账（handle 复用语义与 devman 服务端的 dev_id 单调不复用**直接矛盾**：device.rs:101-111 的 `add` 复用已删 handle，而服务端 device_tree.rs:155 的 id 永不复用），写出的驱动注册逻辑与真实协议不符。

**为何 edge**：处置决策涉 16-stage-drivers（12-gpio-devman 的测试引用与 gpio 注释清理）；crate 本体在共享 libs/ 目录（edge 判定①②交界）。

**建议**：
- **方案 A（推荐）**：删除孤儿 crate（`os/libs/minix-devman-client/` 目录 + os/Cargo.toml:194 成员行），16-stage-drivers 的 12-gpio-devman.md:200 命令去掉 `-p minix-devman-client`，gpio lib.rs:6 注释改指 `minix-sys::devman_client`。理由：doc 10/11 已钦定 minix-sys 为客户端唯一实现，孤儿侧无消费者、语义有偏差（handle 复用 vs dev_id 单调）、其"未来给 gpio 用"的假想需求应由正式客户端承接。
- **方案 B**：保留并转正为 gpio 的记账层（gpio 声明依赖、文档更新）。仅当 16-stage-drivers 明确要"纯记账、不带 wire"的分层时成立；届时必须先修 handle 复用语义（改为 dev_id 单调对齐服务端）。
- 两侧共同动作：gpio lib.rs:6 的指代注释必须澄清（无论 A/B）。

**依赖**：无。
**解锁**：E-DMWIRE 的 client 侧实现者不再有两套 API 可选；16-stage-drivers 的驱动注册链设计定案。

> **复核（2026-09-15）**：现状与条目基本一致，两点修正——(1) 该 crate 已在 workspace 成员表（os/Cargo.toml:194，与条目记载一致），但仍零依赖者；(2) gpio lib.rs:4-6 注释引用的是 server crate 包名 `minix-devman`（引用对象本身就不该是任何 devman crate，应是 minix-sys 的 devman_client 模块）。方案 A 删除时四处一并清：crate 目录 + 成员行 + 12-gpio-devman.md:200 测试命令 + gpio 注释。
