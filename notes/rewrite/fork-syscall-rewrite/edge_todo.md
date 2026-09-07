# 跨 Stage Edge TODO（fork-syscall-rewrite）

> 来源：2026-09-06 V11 架构审查（02-stage-vm/todo.md §14）拆分出的跨 stage 条目。
> 2026-09-06 增补：04-stage-pm 架构审查（04-stage-pm/todo.md §9）拆分出的跨 stage 条目 E6-E7。
> 2026-09-06 增补：03-stage-rs 的 §18.10 E-11 生产接线面登记为 E9（KernelApi 五域面真实传输，E-2 拆分后的接线形态）。
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
| T11 | 2 | fork.rs sys_fork 真实语义 | fork.rs stub（通电→E2） | ⬜ |
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
| T33 | 6 | fork eager CoW（msgaddr 若缺 kernel 对端 → 登记 E-FORKMSG） | T11 收尾 | ⬜ |
| T34 | 6 | MemType 收敛设计与实施 | V9-P2-3 | ⬜ |
| T35 | 7 | 剩余判定批次（WouldBlock/heap shrink/vm_self_query/force_clear/VmProcIter/as_buddy/用量查询/bitmap perf/cow_resolve_region/acl mask/G-V12-4） | todo.md §16 | ⬜ |
| T36 | 7 | 收尾回归：todo/edge 对账 + checklist §8 刷新 + Gate E + 四矩阵全绿 | 收敛审计 | ⬜ |

---

## E1 minix-sys 用户态 trap 层落地

**问题**：minix-sys 的两个 transport 都是 `-EIO` stub——`DirectTrapTransport`（`os/libs/minix-sys/src/ipc.rs:529-559` 全部方法返回 `Err(TrapStatus(EIO))`，注释 :523-525 自述 "The real trap instruction sequences will replace these bodies when the 64-bit trap wiring lands (stage plan item A-6)"）与 `DirectKernelCallTransport`（`os/libs/minix-sys/src/syscall.rs:144-148` 返回 `-EIO`）。全仓不存在任何用户态 trap 指令序列/入口。

**证据**：kernel 侧接收端已就绪——IPC 经 IDT 向量 33 进入（`os/kernel/src/syscall.rs:548-556`，对齐 C `protect.c:147`）；kernel-call 走向量 32 + a7 调用号分发。用户态 `TrapVector`（minix-sys ipc.rs:81-103：KernelCall=32、InterProcess=33）与 `IpcStatus` 位解析（:135-178）已定义完备，缺的只是真实 trap 体。

**影响**：VM/PM/VFS 等全部用户态服务器的 IPC 与 kernel-call 在真实硬件上不可运行；02-stage-vm campaign 的 T9-T15（transport、fdclose、fork、RS、exec、audit）的"真实通电"全部挂在本条。

**建议**：
1. 按 stage plan A-6 落地 64 位 trap wiring：x86_64 优先（IPC 向量 33 与 kernel-call 向量 32 的用户态封装，`core::arch::asm!` 内联），clobber 约定与 kernel 侧 `ipc_entry`/`dispatch_ipc_entry` 的寄存器契约逐一对齐；
2. `DirectTrapTransport` 各方法把 stub 换成真实 trap 序列，保留 `CannedTransport` 测试路径不变；
3. 落地后逐条回写 T9-T15 的通电标注，并跑 E5 的联调测试包。

**解锁**：T9 / T10 / T11 / T13 / T14 / T15 的真实通电；E5 前置。

---

## E2 minix-sys SYS_* kernel-call 包装函数

**问题**：minix-sys 没有任何 `SYS_*` 内核调用包装（grep 零命中）——VM 侧需要的 SYS_FORK（`os/kernel/src/syscall_process.rs:122-215` 已真实）、SYS_UPDATE（`os/kernel/src/misc.rs:1635`，12 步全实现）、SYS_SAFECOPYFROM/TO（`os/kernel/src/syscall_copy.rs:367/:381`）、SYS_EXEC（`os/kernel/src/syscall_process.rs:231`）、SYS_DIAGCTL code 1 控制台输出（`os/kernel/src/syscall.rs:2212-2240`）都缺用户态入口。

**影响**：VM 的 KernelGateway seam（T9）只能以 mock 实现这些调用；fork/RS live-update/safecopy/exec/audit 的端到端链路缺最后一层。

**建议**：在 `os/libs/minix-sys/src/syscall.rs` 复用既有 `perform_kernel_call`（:201，ENOTREADY 重试）与 `KernelCallTransport`（:132-217）机制，为上述 6 类调用各加一个包装函数（签名对齐 kernel dispatch 的消息布局，message 构造用 minix-types 现有 union 成员）；每个包装带一个"CannedTransport 回放"单元测试。

**解锁**：T11 / T13（步骤 4）/ T14 / T15 的真实通电。

---

## E3 VmBootHandoff 补 kernel text/data span（= 02-stage-vm V11-P2-7）

**问题**：`minix-types/src/types/boot.rs:107-147` 的 `VmBootHandoff` 没有 kernel text/data 的 `(paddr, pages)` 字段（kernel image 只隐含在 `deducted` 记录里）；VM 侧 `KernelLayout` 只能填 mock（`os/servers/vm/src/vm_server.rs:450-460` 的 `0xFFFF_FFFF_8000_0000` 等，TODO 自认 boot-info 未接线）。

**跨 stage 文件**：`os/libs/minix-types/src/types/boot.rs`（加字段 + version 递增）、`os/kernel/src/vm_handoff.rs`（`build_vm_handoff` :289-304 填充；kernel text paddr 可用 `kern_phys_base()`）、`os/servers/vm/src/boot.rs`（`read_boot_params` 解析）+ `vm_server.rs`（`init_global_state` 消费 mock 替换）。

**建议**：三处一次改齐；`debug_assert` 保证 mock 值与真实值不共存；handoff `version` 字段同步递增并在 VM 侧做版本协商（不认识新字段时保持 mock + 显式警告）。

---

## E4 pt_alloc free 注册 + 三架构 destroy 中间页回收（= 02-stage-vm V11-P2-8）

**问题**：`os/arch/src/arch/pt_alloc.rs` 只有 `register/is_registered/alloc_pt_page`（:69/:90/:101），没有 free；`x86_64/paging.rs:487-497` 的 `destroy` 只清零根 PML4、自述 "accept the intermediate-table leak"——每次进程退出泄漏 1-3 个中间页表页（C 的 `pt_free` pagetable.c:1427-1437 会回收；Redox `Drop for Table` + Linux `free_pgtables` 均回收）。

**跨 stage 文件**：`os/arch/src/arch/pt_alloc.rs`（加 free 注册槽）、`os/arch/src/x86_64/paging.rs`（destroy 四级遍历回收）、需核查 aarch64/riscv64 的同型 destroy 是否同样只清零（UNVERIFIED）。

**建议**：pt_alloc 注册槽从单函数指针扩为 `{ alloc, free }`（或 trait）；destroy 逐级回收中间页并归还注册来源的分配器，保持"先清零根防 UAF"语义与 `exit.rs:188-189` 的 SAFETY 前提不变；补"destroy 后中间页归还"测试。

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

**问题**：RS 的 `RS_UP`（do_up，request.c:15-106）与 `RS_EDIT`（do_edit，request.c:298-385）第一步都是 `copy_rs_start`——把调用方内存里的完整 `struct rs_start`（rs.h:107-166，约 230 字节：rss_flags/rss_cmd/rss_uid/位图数组/irq·io·pci 表/rss_label/…）按 C ABI 整结构拷入 RS。该结构含 `bitchunk_t rss_system[SYS_CALL_MASK_SIZE]`、`bitchunk_t rss_vm[VM_CALL_MASK_SIZE]` 与 `uid_t rss_uid`，而 `bitchunk_t` 在本 minix3 子树**只有使用没有 typedef**（bitmap.h:12 引用 `sizeof(bitchunk_t)`，全树 grep 无定义），`uid_t` 亦属 sys/types.h 外部类型——字节偏移无法从本树 pinning，猜偏移违反 Ground Truth 链（同 E-RSWIRE 判据）。

**影响**：13-rs-control-requests 的 `do_up`/`do_edit` 两臂停在缝上：权限/查槽/编排（create_service/edit_slot/run_service——决策与编排已全就绪，Fix #46-#52）就等这条解码；RS 侧其余 label 型控制臂（down/refresh/restart/clone/unclone/lookup/fi/getsysinfo/sysctl）已全部 live（Fix #71/#74/#75/#76），不依赖本条。

**解锁后工作（约一个完整迭代）**：
1. 从完整 Minix3 源码树 pin `bitchunk_t`/`uid_t` 尺寸 → 计算 `rs_start_t` 偏移表（逐字段断言测试锚定字节布局，风格同 E-RSWIRE 的 RprocpubWire）；
2. minix-types 增 `RsStartWire`（repr(C)）+ `decode` + 偏移断言；
3. rs 侧 `RsServer::do_up`/`do_edit` 接线：label 改取 rs_start 内的 rss_label，编排消费 `check_create_preconditions`/`create_service`/`edit_slot`/`run_service` 全链（sched_stop→edit_slot→privctl(UpdateSys)→sched_init 序列含 E-7 的类型化锚）。

**依赖**：完整 Minix3 C 源码参照（或补全本树头文件中 `bitchunk_t`/`uid_t` 的定义链）；无 E1/E2 依赖（解码纯单测可验证）。

---

## E-RSWIRE rprocpub 字节 ABI pinning + rproctab 解码（T9 step3 的余件）

**背景**：RS_INIT 握手的 grant 贯通与 fail-closed 已落地（Fix #34）；`ipc_call_rs_init` 目前诚实返回 `NotImplemented`（pin 测试 `test_run_once_rs_init_fails_closed_until_erswire` 定格），假成功 `Ok(RprocTab::empty())` 已消灭。

**阻塞点**：`struct rprocpub` 的字节 ABI 无法从本仓 minix3 子树 pinning——`devmajor_t`、`bitchunk_t`、`struct rs_pci` 在此树中被引用但**无定义**（rs.h/type.h 不完整）。按 Ground Truth 链（C 源 > doc > code），缺 C 事实就不能猜偏移。且该布局是 RS↔VM 共享契约（rs 服务器工作流同样消费），落点应在 minix-types。

**解锁后工作（约一个完整迭代）**：
1. 从完整 Minix3 源码树 pin 三类型定义 → 计算偏移表（ILP32：in_use short@0、sys_flags@4、endpoint@8、old_endpoint@12、new_endpoint@16、dev_nr、nr_domain、domain[8]、label[16]、proc_name[16]、vm_call_mask[BITMAP_CHUNKS(49)]、rs_pci、devman_id）；
2. minix-types 增 `RprocpubWire`（repr(C)）+ `decode` + 偏移断言测试（手排字节十六进制锚定）；
3. VM 侧：`Gateway::sys_safecopyfrom(granter, gid, offset, buf)`（wire 已定：SYS_SAFECOPYFROM=31、`MessLsysKernSafecopy{from_to,grant_id,offset,address,bytes}`，kernel 应答 Ok(0)=成功）+ `ipc_call_rs_init` 真实体（拷贝 + 解码 → RprocTab）+ 翻转 pin 测试 + 恢复 `rs_handshake` ACL 循环为可达。

**依赖**：完整 Minix3 C 源码参照（或补全本树头文件）；无 E1/E2 依赖（解码可纯单测）。

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

## E5 端到端联调测试包

**问题**：VM 与其他服务器的协作当前零端到端覆盖——`os/tests/pm_vm_fork{,_test}.rs` 正文整体注释停用（自注 "DEPRECATED: permanently disabled"）；VFS fdclose、RS live-update、QEMU VM paging 冒烟均无。

**建议**：在 E1/E2 落地后建联调包：(a) PM↔VM fork 全链路（恢复/重写旧测试，改走 minix-sys 消息层而非 crate 内类型——旧失效原因正是 `pub(crate)` 边界收紧）；(b) VM↔VFS fdclose 往返；(c) RS live-update 全链路（RS_PREPARE → UPDATE → resume）；(d) QEMU VM paging 冒烟（boot shim 拉起 VM → `init_vm_self_pt` → map/query/unmap 测试页 → 串口结果，复用 `os/qemu-tests/` 基建）。

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

---

## E7 minix-types PM 协议面系统化（= 04-stage-pm/todo.md P1-4 + P2-3 抽取）

**问题**：minix-types 对 PM 的协议面是三个碎片。(a) `os/libs/minix-types/src/ipc/pm.rs:13-38` 的 `PmRequest`/`PmResponse` 是零使用死代码——整个工作区只有 `PmError` 被 `os/servers/pm/src/init.rs:390` 消费。(b) PM 调用号双址：`os/servers/pm/src/ipc/calls.rs:29-124` 的 `PmCall` 枚举（47 值，calls.rs:16-17 注释自述"将来内核侧需要调用号时再上移 minix-types"）与 minix-types 散落常量（如 `PM_PROCEVENTMASK`，被 `os/servers/pm/src/init.rs:368` 使用）并存，同一事实两处表达。(c) 47 个调用的消息布局没有系统化 wire 类型——C `m_lc_pm_*`/`m_pm_lc_*` union 字段（minix/com.h）当前只在 PM 主循环内联 unsafe 访问（`os/servers/pm/src/init.rs:430/438-440/459-460/480-481`）。另有调度协议常量缺失：`SEND_PRIORITY`/`SEND_TIME_SLICE`（C `minix3/minix/servers/pm/const.h:19-20`）全仓无对应。

**证据**：`os/libs/minix-types/src/ipc/pm.rs` 全文 111 行仅 Fork 一个请求变体；VM 侧同型先例是 `NR_VM_CALLS` 双定义（02-stage-vm/todo.md:1047，`minix-types/src/ipc/vm.rs:149` vs `os/servers/vm/src/vm_server.rs:1173` 私有副本）；共享层"上移 minix-types"的 OQ 决策先例见 G1（01-stage-kernel/todo.md:342，ProcNr）。

**影响**：PM 的分发接线（04-stage-pm/todo.md P1-1，40 个待点亮调用）每一臂都要先回答"消息怎么解码"；没有系统化 wire 类型，unsafe 解码将被复制约 40 份，wire 布局错误无类型层防护；调用号双址使 callnr.h 的单一真值破口随消费方（libc/commands）增多而扩大。

**建议**：与 04-stage-pm P1-4 协同一次做齐——(1) 按调用族在 `os/libs/minix-types/src/ipc/pm.rs` 建 wire 结构体（对照 C union 逐字段 + `size_of` 断言，风格对齐 `ipc/message.rs` 既有成员如 `MessPmSchedSchedulingSetNice` :1165）；(2) 处置 `PmRequest`/`PmResponse` 死代码：要么作为新 wire 层的入口枚举重构，要么删除（待 P1-4 设计时定，不允许默认保留）；(3) 调用号收敛二选一：47 个 `pub const PM_*` 上移 minix-types（PmCall 枚举随之迁移，成为 callnr.h 的 Rust 等价物，倾向此案）或 minix-types 常量清空、pm crate 为唯一真值——需 OQ 确认归属；(4) 补 `SEND_PRIORITY`/`SEND_TIME_SLICE` 常量。

**解锁**：04-stage-pm/todo.md P1-1（每臂解码）/ P1-4 / P2-3 的实施前提；未来 libc/commands 侧 PM 调用发起方的常量消费。

> **进度（2026-09-06）**：首个切片已落地——`MessPmLcWait4 { status }` + `m_pm_lc_wait4` arm（`message.rs`，56 字节断言 `test_pm_wait4_message_layouts`），wait4 回复载荷契约（04-stage-pm/todo.md D-26/Fix #22）闭环；其余 wire 族照本切片的风格推进。

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
