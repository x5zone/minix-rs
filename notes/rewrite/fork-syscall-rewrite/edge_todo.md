# 跨 Stage Edge TODO（fork-syscall-rewrite）

> 来源：2026-09-06 V11 架构审查（02-stage-vm/todo.md §14）拆分出的跨 stage 条目。
> 2026-09-06 增补：04-stage-pm 架构审查（04-stage-pm/todo.md §9）拆分出的跨 stage 条目 E6-E7。
> 2026-09-06 增补：03-stage-rs 的 §18.10 E-11 生产接线面登记为 E9（KernelApi 五域面真实传输，E-2 拆分后的接线形态）。
> 2026-09-08 增补：02-stage-vm V12 轮（02-stage-vm/todo.md §17）登记 E-VMMCPWIRE（vmmcp wire 字宽必现截断）与 E-VMMOCK（G-V12-5 余件：minix-arch default features 收口），并在 E5 增补"缺页故障完整回路"验收面。
> 2026-09-09 增补：02-stage-vm V13 轮（02-stage-vm/todo.md §18）登记 E-VMTLB（kernel 侧目标进程 TLB 刷新机制缺失，真 SMP 正确性前提），并在 E-RSWIRE 增补 VM 侧三项落地要求（call_mask u64、RS_SET_PRIV 真掩码、五消息结构专属 wire struct）。
> 定位：**跨 stage 边界条目的唯一入口**，后续单线程逐条执行，避免并发修改各 stage 的 todo.md 时发生冲突。
> Edge 判定规则（三类）：① 共享契约/基础设施层——minix-types 布局、minix-sys trap 层与 SYS_* wrapper、os/arch 的 pt_alloc；② 对方 stage 目录里的生产代码（如 kernel 侧填充 handoff 字段）；③ 多进程联调测试（QEMU 端到端）。
> stage 内生产代码（消费既有稳定契约，含 seam + mock 测试）**不属于** edge，在所属 stage 的 todo.md 内实施。
> 执行约定：一次一条；每条完成后在本文件标注状态与日期；涉及对应 stage 的条目同步回写其 todo.md（02-stage-vm 对应 V11 条目、04-stage-pm 对应 P/D 条目）。

---

## 0. 02-stage-vm 实施 campaign 顺序表（进度真相源）

对应 `02-stage-vm/todo.md` §14 的 V11 条目与本文件 edge 条目的依赖关系。VM 侧口径：依赖共享 trap 层的条目，VM 侧逻辑完备（seam + mock 测试）即标 ✅，真实通电挂对应 edge 条目（模式 60 诚实契约）。

| 迭代 | 批次 | 内容 | 对应条目 | 状态 |
|---|---|---|---|---|
| T1 | 0 | 文档测试名 4 处 + §5.4 计数刷新 | V11-P2-6 | ✅ 2026-09-06（todo.md §15 Fix #19） |
| T2 | 0 | 过时注释 4 处（X86_64Paging 已实现） | V11-P2-4 | ✅ 2026-09-06（todo.md §15 Fix #20） |
| T3 | 0 | clippy 回归收敛 + 卫生批次 | V11-P2-3 + V11-P3-1 | ✅ 2026-09-06（todo.md §15 Fix #21；all-features 剩 :301 归 T4） |
| T4 | 0 | 删 DefaultAllocator 双真相源 + 组合语义测试 | V11-P1-3 | ✅ 2026-09-06（todo.md §15 Fix #22） |
| T5 | 1 | VmContext 第一步：parts_mut 消灭 | V11-P1-2（1/2） | ✅ 2026-09-06（todo.md §15 Fix #23） |
| T6 | 1 | VmContext 第二步：dispatcher 收 &mut VmContext + fdref/table 收敛 | V11-P1-2（2/2） | ✅ 2026-09-06（todo.md §15 Fix #24；fdref 收敛归 T10） |
| T7 | 1 | per-call codec 注册表 + dispatch 表驱动化 | V9-P2-1 + V9-P2-2 | ✅ 2026-09-06（todo.md §15 Fix #26） |
| T8 | 1 | 错误枚举收敛 → **判定闭合：现有 From 集中表即最优** | V10-P2-3 + P2-2 | ✅ 2026-09-06（todo.md §15 Fix #27，WONTFIX 级设计判定） |
| T9 | 2 | KernelIpcTransport VM 侧完备 + KernelGateway seam | V11-P1-1（通电→E1/E2） | 🔄 step1 ✅（Fix #28）step2 ✅（Fix #29）step3 ✅（Fix #34：grant 贯通 + 假成功消灭 + fail-closed；rproctab 字节解码→**E-RSWIRE**） |
| T10 | 2 | VFS_FDCLOSE 发送 + region close 入队 | （V11-P1-1 建议 2 / P1-3 链） | 🔄 入队半 ✅（Fix #33）；发送半 → **E-VFSWIRE** |
| T11 | 2 | fork.rs sys_fork 真实语义 | fork.rs stub（通电→E2） | ✅ 2026-09-07（stub 删除随 Fix #29；eager-CoW 相随 Fix #53；通电→E2+E-FORKMSG） |
| T12 | 2 | RS_PREPARE map_proc_dyn_data | rs.rs:250 DEFERRED | ✅ 2026-09-06（todo.md §15 Fix #40） |
| T13 | 2 | RS_UPDATE 步骤 5-7（VM 侧）+ 步骤 4 走 Gateway | rs.rs:328 DEFERRED（通电→E2） | ✅ 2026-09-06（todo.md §15 Fix #42） |
| T14 | 2 | exec_bootproc（minix-elf + VM 映射 + Gateway.sys_exec） | vm_server.rs:385 DEFERRED（通电→E2） | 🔄 装载半+sys_exec wire ✅（Fix #41）；栈帧 ABI → **E-BOOTFRAME** |
| T15 | 2 | audit 日志转发（Gateway.diagctl） | audit.rs:16（通电→E2） | ✅ 2026-09-06（todo.md §15 Fix #32） |
| T16 | 2 | sanity_checks feature + usedpages 等价物 | V10-P2-1 sanity 行 + G-V11-2 | ✅ 2026-09-06（todo.md §15 Fix #38；usedpages 语义由 verify_refcounts 覆盖） |
| T17 | 2 | bitmap cache_freepages 三步路径 → **判定闭合：语义已被双层覆盖，钩子删除** | bitmap_alloc.rs:347 DEFERRED | ✅ 2026-09-06（todo.md §15 Fix #31） |
| T18 | 2 | alloc 失败计数接入 InfoStats（周期循环判定不采纳） | alloc_stats.rs:46 DEFERRED | ✅ 2026-09-06（todo.md §15 Fix #30） |
| T19 | 2 | exec_newmem / DMA 三条 parity 处置（删 dead stub，不实现） | dispatcher.rs:820/:1223 | ✅ 2026-09-06（todo.md §15 Fix #25） |
| T20 | 2 | 大匿名映射懒分配 → **判定闭合：demand paging 已是现状**（稀疏表示=证据门控优化） | V9-P3-2 | ✅ 2026-09-06（todo.md §15 Fix #35） |
| T21 | 3 | 页表可注入化 + VM 内 SimPaging | V11-P2-1（QEMU 冒烟→E5） | ✅ 2026-09-06（todo.md §15 Fix #39） |
| T22 | 3 | MemType / PhysAllocator 方法级补测 + buddy reserve 语义修正 | V11-P2-2 | ✅ 2026-09-06（todo.md §15 Fix #36） |
| T23 | 3 | rs_handshake/init pin 测试 + run_once 分支补测 + CI 矩阵 | V11-P2-5 | ✅ 2026-09-06（todo.md §15 Fix #37；rs_init pin 已随 Fix #34） |
| T24 | 4 | 残留标注清理 + parity/死代码判定批次 + 新缺口登记（G-V12-1..4） | todo.md §16 | ✅ 2026-09-07（todo.md §16 Fix #43 pre + Fix #44 清理批次；五篇文档同步） |
| T25 | 4 | pt=None → SimPaging 翻转 ×6（munmap×4/brk×2） | V11-P2-1 收尾 | ✅ 2026-09-07（todo.md §16 Fix #45；mmap helper 连带修复） |
| T26 | 4 | MOCK_BASE_MUTEX + extend_to_static_lifetime 归零（线程本地窗口） | V11-P1-2/V9-P2-4 验收锚点 | ✅ 2026-09-07（todo.md §16 Fix #46；新登记 G-V12-5 归 E3） |
| T27 | 4 | dispatcher 4 函数 happy-path 补测 | G-V12-3 | ✅ 2026-09-07（todo.md §16 Fix #47；四矩阵 476/493/476/476） |
| T28 | 5 | CacheMemory::ev_pagefault 缓存查找 + PbCache 接线 → **判定闭合：邮箱机制删除，契约 fail-closed 化** | G-V12-1 | ✅ 2026-09-07（todo.md §16 Fix #48） |
| T29 | 5 | SIGKMEM 信号 seam + do_memory 排空循环（kernel 对端已落地；通电挂 E1） | G-V12-2 + G-V11-1 | ✅ 2026-09-07（todo.md §16 Fix #49；三矩阵 480/497/480） |
| T30 | 5 | 分配漏斗回收-重试（alloc_pfn_reclaiming；C alloc_mem do-while 语义） | "24-page-cache" 停泊项 | ✅ 2026-09-07（todo.md §16 Fix #50；三矩阵 484/501/484） |
| T31 | 5 | 缺页计数生产者接线 + InfoUsage 槽位判定（Getrusage 为出口，VM_INFO wire C-parity） | vmproc_handle.rs:305 | ✅ 2026-09-07（todo.md §16 Fix #51；新登记 G-V12-6） |
| T32 | 6 | VFS transid 路径 C-parity 修复（真 bug：clean_type 门拒绝真实 transid 消息） | vm_server.rs:1227 | ✅ 2026-09-07（todo.md §16 Fix #52；三矩阵 486/503/486） |
| T33 | 6 | fork eager CoW——VM 侧完成（借用两相 + msgaddr 经 gateway Option）；kernel 缺 msgaddr 出参 → **E-FORKMSG 登记** | T11 收尾 | ✅ 2026-09-07（todo.md §16 Fix #53；三矩阵 488/505/488） |
| T34 | 6 | MemType 收敛设计 → **判定闭合：保留 trait（C vtable 直接对应物；Redox Provider 类比不成立）** | V9-P2-3 | ✅ 2026-09-07（todo.md §16 Fix #55） |
| T35 | 7 | 剩余判定批次——注记批+失真批+per-backend 查询判定 ✅；余 heap-shrink 删除、G-V12-4 errno 直传（下轮，理由见 Fix #54） | todo.md §16 | 🔄 主体 ✅ 2026-09-07（todo.md §16 Fix #54） |
| T36 | 7 | 收尾回归：todo/edge 对账 + checklist §8 刷新 + Gate E + 四矩阵全绿 | 收敛审计 | ✅ 2026-09-07（todo.md §16 Fix #56；campaign 完结——T24–T36 全部闭环） |

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

---

## E3 VmBootHandoff 补 kernel text/data span（= 02-stage-vm V11-P2-7）

> **进度（2026-09-08，完成）**：`VmBootHandoff` 增 `kern_virt_base/kern_phys_base/kern_text_pages/kern_data_pages` 四字段（version 2 → 3；size 断言 ≤ 4096 仍通过）；kernel `build_vm_handoff` 从 `kern_virt_base()/kern_phys_base()/kern_size()` 填充（minix-rs 内核映像为单一连续 span——text_pages = 全映像页数，data_pages = 0）；VM `read_boot_params` 解析为 `BootParams.kernel_layout: Option<KernelLayout>`（handoff v≥3 → `kernel_layout()`，v≤2 → None）；`init_global_state` 消费——`Some` 用真值，`None`（pre-E3 handoff/宿主测试）保留 mock 常量 + 审计告警。minix-types 访问器测试 ×2（v3 报告 span / v2 None）。**P1-4 实质闭环**：真实硬件上 `init_page_table` 的内核映射来自 boot handoff 而非硬编码 mock。余件：riscv64 Sv39 的 `VM_BOOT_HANDOFF_VA`（0x1_0000_0000 < 2^38 ✓ 已兼容）。

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

---

## E-RSSTART rs_start_t 字节 ABI pinning + copy_rs_start 解码（03-stage-rs RS_UP/RS_EDIT 臂，2026-09-07 登记）

> **改判（2026-09-07，用户批准并入 03-stage-rs campaign 后关单）**：阻塞前提
> （"`bitchunk_t`/`uid_t` 在本树无 typedef，rs_start_t 字节 ABI 不可 pinning"）经独立
> 核实**不成立**——minix3/ 是完整 NetBSD 式全树：`bitchunk_t = uint32_t`
> （`minix3/sys/sys/types.h:124`，固定宽度、无架构依赖）、`uid_t = uint32_t`
> （types.h:221 + ansi.h:46）、`struct rs_start` 完整（`minix3/minix/include/minix/rs.h:104-151`）。
> 当初的 grep 只覆盖了 `minix3/minix/` 子树而漏掉 `minix3/sys/`。wire 解码面已落地
> （`minix-types::ipc::rs_start`，Fix #81：偏移常量单点表 + repr(C) 布局见证 +
> 39 个 offset_of 编译期断言 + x86-64 LP64 数据模型声明）；**RS_UP/RS_EDIT/
> RS_UPDATE 三臂接线转入 03-stage-rs/todo.md §21 campaign 执行（R4-R6），本条目
> 关单**。下文保留原文以存档判定过程；原文的"约 230 字节"与 ILP32 假设作废
> （实际 `sizeof(struct rs_start)` = 920，LP64）。

**问题**：RS 的 `RS_UP`（do_up，request.c:15-106）与 `RS_EDIT`（do_edit，request.c:298-385）第一步都是 `copy_rs_start`——把调用方内存里的完整 `struct rs_start`（rs.h:107-166，约 230 字节：rss_flags/rss_cmd/rss_uid/位图数组/irq·io·pci 表/rss_label/…）按 C ABI 整结构拷入 RS。该结构含 `bitchunk_t rss_system[SYS_CALL_MASK_SIZE]`、`bitchunk_t rss_vm[VM_CALL_MASK_SIZE]` 与 `uid_t rss_uid`，而 `bitchunk_t` 在本 minix3 子树**只有使用没有 typedef**（bitmap.h:12 引用 `sizeof(bitchunk_t)`，全树 grep 无定义），`uid_t` 亦属 sys/types.h 外部类型——字节偏移无法从本树 pinning，猜偏移违反 Ground Truth 链（同 E-RSWIRE 判据）。

**影响**：13-rs-control-requests 的 `do_up`/`do_edit` 两臂停在缝上：权限/查槽/编排（create_service/edit_slot/run_service——决策与编排已全就绪，Fix #46-#52）就等这条解码；RS 侧其余 label 型控制臂（down/refresh/restart/clone/unclone/lookup/fi/getsysinfo/sysctl）已全部 live（Fix #71/#74/#75/#76），不依赖本条。

**解锁后工作（约一个完整迭代）**：
1. 从完整 Minix3 源码树 pin `bitchunk_t`/`uid_t` 尺寸 → 计算 `rs_start_t` 偏移表（逐字段断言测试锚定字节布局，风格同 E-RSWIRE 的 RprocpubWire）；
2. minix-types 增 `RsStartWire`（repr(C)）+ `decode` + 偏移断言；
3. rs 侧 `RsServer::do_up`/`do_edit` 接线：label 改取 rs_start 内的 rss_label，编排消费 `check_create_preconditions`/`create_service`/`edit_slot`/`run_service` 全链（sched_stop→edit_slot→privctl(UpdateSys)→sched_init 序列含 E-7 的类型化锚）。

**依赖**：~~完整 Minix3 C 源码参照（或补全本树头文件中 `bitchunk_t`/`uid_t` 的定义链）~~（已解除——定义在树内）；无 E1/E2 依赖（解码纯单测可验证）。

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

## E5 端到端联调测试包

**问题**：VM 与其他服务器的协作当前零端到端覆盖——`os/tests/pm_vm_fork{,_test}.rs` 正文整体注释停用（自注 "DEPRECATED: permanently disabled"）；VFS fdclose、RS live-update、QEMU VM paging 冒烟均无。

**建议**：在 E1/E2 落地后建联调包：(a) PM↔VM fork 全链路（恢复/重写旧测试，改走 minix-sys 消息层而非 crate 内类型——旧失效原因正是 `pub(crate)` 边界收紧）；(b) VM↔VFS fdclose 往返；(c) RS live-update 全链路（RS_PREPARE → UPDATE → resume）；(d) QEMU VM paging 冒烟（boot shim 拉起 VM → `init_vm_self_pt` → map/query/unmap 测试页 → 串口结果，复用 `os/qemu-tests/` 基建）。

> **验收面增补（2026-09-08，02-stage-vm V12 轮）**：(d) 冒烟必须覆盖**缺页故障完整回路**——进程触发缺页 → 内核 VM_PAGEFAULT 送达 VM → VM 解析（CoW 拷贝或新页分配）→ **VM 写进程硬件 PTE**（G-V12-8，当前缺失的最后一环）→ 内核清 RTS_PAGEFAULT 恢复进程 → 指令重执行不再故障。没有 PTE 写入的通电冒烟会以"同一地址反复缺页活锁"的形式失败，这正是该回路必须成为 (d) 的显式断言项的原因。

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

---

## E-VMMCPWIRE vmmcp 消息族字段宽度修正：reply.addr u32 → 64 位（02-stage-vm V12 轮登记，2026-09-08）

**问题**：minix-types 的 `MessVmmcpReply.addr` 是 `u32`（`os/libs/minix-types/src/ipc/message.rs:2512-2515`），而 C 的对应字段是 `void *addr`（`minix3/minix/include/minix/ipc.h:2395-2400`，x86_64 上 64 位；C 赋值 `msg->m_vmmcp_reply.addr = (void *) vr->vaddr`，mem_cache.c:170）。VM 侧编码随之截断：`reply.addr = addr.0 as u32`（`os/servers/vm/src/vm_server.rs:1704`），而 mapcache 的分配地址走 MMAP 窗口（`MMAP_BASE = 0x1_0000_0000`，`os/servers/vm/src/mmap.rs:204`）——**高 32 位恒非零，截断恒发生**，属必现 wire bug 而非边角。同簇疑点：`mmap.rs:373` 的 `length: aligned_len.0 as u32`（>4GB 映射静默截断），以及 `mess_vmmcp` 请求方向字段宽的逐字段核查。

**为何 edge**：minix-types 消息布局是共享契约（edge 判定①类）——字段加宽是 wire ABI 变更，消费面（未来 minixfs/lib 的 vm_map_cacheblock 等价物，C 侧 libsys/vm_cache.c:47-54）尚未存在，现在改零成本、通电后改即破坏二进制契约。

**建议**：(1) `MessVmmcpReply.addr: u32 → u64`（对齐 C `void *`），VM 编码去截断；(2) 对照 `mess_vmmcp`/`mess_vmmcp_reply` 原始结构逐字段核查请求/回复两个方向（含 `_ASSERT_MSG_SIZE` 对应的 56 字节 payload 断言）；(3) wire 回放测试断言大地址高位保全；(4) 顺手按"pattern 84 候选"（02-stage-vm/todo.md §17.6）对 VM 消息族做一次系统性字段宽度对账，同类问题一次清完。

**解锁**：02-stage-vm/todo.md V12-P1-3（VM 侧半边）；E5(b) VFS 缓存协作链的正确性前提。

> **进度（2026-09-08，✅ 闭单）**：建议 (1)(3) 已落地——`MessVmmcpReply.addr: u64`（`addr @0, flags @8, padding[47]`，56 字节保持），VM `encode_reply_data` 去截断，`VfsRequest.length` 同批拓宽 u64；测试 `test_vmmcp_reply_layout_64bit_addr`（minix-types）+ `test_encode_mapcache_reply_preserves_high_addr_bits`（vm_server）。依据记录：02-stage-vm/todo.md §17.9 Fix #59。**余件转入低优先**：建议 (4) 的 VM 消息族系统性字段宽度对账（pattern 84 候选）——`mess_vmmcp` 请求方向初查字段类型与 C 一致（dev/off/ino 皆 64 位 + block/flags_ptr 指针宽待 minix-sys 消费时定），留作后续扫描项，不阻塞通电。

---

## E-VMMOCK minix-arch default features 泄漏收口 + "mock" 命名澄清（02-stage-vm G-V12-5 余件，2026-09-08 登记）

**问题**：`os/servers/vm/Cargo.toml:18` 以裸 path 依赖引入 `minix-arch`（`minix-arch = { path = "../../arch" }`），绕过了 workspace 表的 `default-features = false`（`os/Cargo.toml:235`）；而 `os/arch/Cargo.toml:18` 的 `default = ["mock"]` —— mock feature 就此泄漏进 VM 生产构建。kernel 与 boot-shim 均已正确关闭默认（`os/kernel/Cargo.toml:13`、`os/boot-shim/Cargo.toml:17`），VM 是唯一漏网点。同时 `mock` feature 实际门控的是"运行时窗口基址变体"（Direct Map 窗口基址是内核动态授予的运行时值，E3 已接真值），命名与生产用途混淆——这是 02-stage-vm/todo.md §16.1 G-V12-5 的原始登记内容，原定"处置归 E3"，但 **E3 的完成注记（2026-09-08）未包含依赖收口**，为防孤儿单列本条。

**建议**：(1) `os/servers/vm/Cargo.toml` 的 minix-arch 依赖补 `default-features = false`，跑三 feature 矩阵回归（G-V12-8 之前这主要影响编译面与 arch 内 mock 项的 dead_code 噪音，预期零行为差异——若有差异即暴露了生产代码误依赖 mock 项，需逐处修正）；(2) arch 侧把 `mock` feature 更名为诚实表达运行时窗口语义的名字（如 `runtime-window`，或直接内联为非 feature 代码路径），同步 kernel/boot-shim 的引用；(3) 在 02-stage-vm/todo.md §2 表 G-V12-5 行回写闭单。

**解锁**：VM 生产依赖面的单一真相；arch 命名与语义一致。无 E1/E2 依赖，可独立先行。

> **进度（2026-09-08，建议 (1) 完成）**：`os/servers/vm/Cargo.toml` 已补 `default-features = false`，三 feature 矩阵回归零差异（490/507/490 passed）——VM 生产代码无 mock 项依赖，收口无行为影响。**余件**：建议 (2) arch 侧 `mock` 更名（涉 kernel/boot-shim 引用同步）与 (3) G-V12-5 行闭单回写（待 (2) 一并完成）。依据记录：02-stage-vm/todo.md §17.9 Fix #62。

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

---

## E-VMTLB kernel 侧目标进程 TLB 刷新机制缺失（02-stage-vm V13 轮登记，2026-09-09）

**问题**：VM 服务器直接写进程硬件 PTE 之后（Fix #60 起故障路径、更早起 munmap/brk/fork 路径），C 对"目标进程在其他 CPU 上的陈旧 TLB 翻译"有显式机制，Rust 内核没有任何对应物：

- C 机制：进程级 `MF_FLUSH_TLB` 标志 + 调度点刷新——`minix3/minix/kernel/proc.c:345-347`（`if (p->p_misc_flags & MF_FLUSH_TLB && get_cpulocal_var(ptproc) == p) tlb_must_refresh = 1`，随后 `switch_address_space(p)` 重载根寄存器）。
- Rust 现状：`rg "MF_FLUSH_TLB|tlb_must_refresh" os/kernel/src` 零命中；且调度器 `switch_address_space` 带"同根跳过重载"优化（os/kernel/src/syscall.rs:2410-2417 注释自述 mirror 比对语义——镜像必须跟上每次根变更，否则首次分发会无谓重载）。
- 风险窗口：真 SMP + 共享页场景（fork 后 CoW 父子页、shm 多映射方）——VM 翻 RO/解除映射时，另一 CPU 上仍在运行的共享方进程的 TLB 保留旧翻译（RO 旧项使写持续故障、已 unmap 旧项使进程继续写已回收物理页）。当前单核 + "VM 只改非运行进程 PTE"的结构纪律（IPC 阻塞 + RTS 停止）下不可达；E5 通电若开 SMP 即暴露。

**为何 edge**：机制主体在 kernel（01-stage 生产代码，edge 判定②），与 VM 的 PTE 写路径（02-stage）协同才可验收（edge 判定③）。

**解锁后工作**：(1) kernel 进程结构补 MF_FLUSH_TLB 等价标志 + 调度点刷新（对照 proc.c:345-347）；(2) 与 switch_address_space 的同根跳过优化对账（刷新标志必须在比较之前生效）；(3) 同步评估 C VM 侧四处 `sys_vmctl(SELF, VMCTL_FLUSHTLB)`（pagetable.c:119/255/319/430，宿主 pt_assert[SANITY]/vm_freepages×2/vm_pagelock[MEMPROTECT]）——它们依附 C 的"进程内存别名映射进 VM 地址空间"模型，minix-rs 的 direct map 下翻译恒定、VM 自刷不需要（该 ARCH 偏差在 02-stage-vm/todo.md §18.2 V13-P2-1(b) 登记）；(4) E5 若含 SMP 配置，冒烟须覆盖"fork 后父子并发写 CoW 页"。

**依赖**：无 E1/E2 硬依赖（机制可先行 + 单测），但验收（E5 SMP 冒烟）前置 E1/E2；与 01-stage-kernel 的 SMP 工作窗（smp_gpt 系列设计迭代）协同排期。
