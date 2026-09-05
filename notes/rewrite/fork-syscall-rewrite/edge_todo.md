# 跨 Stage Edge TODO（fork-syscall-rewrite）

> 来源：2026-09-06 V11 架构审查（02-stage-vm/todo.md §14）拆分出的跨 stage 条目。
> 定位：**跨 stage 边界条目的唯一入口**，后续单线程逐条执行，避免并发修改各 stage 的 todo.md 时发生冲突。
> Edge 判定规则（三类）：① 共享契约/基础设施层——minix-types 布局、minix-sys trap 层与 SYS_* wrapper、os/arch 的 pt_alloc；② 对方 stage 目录里的生产代码（如 kernel 侧填充 handoff 字段）；③ 多进程联调测试（QEMU 端到端）。
> stage 内生产代码（消费既有稳定契约，含 seam + mock 测试）**不属于** edge，在所属 stage 的 todo.md 内实施。
> 执行约定：一次一条；每条完成后在本文件标注状态与日期；涉及 02-stage-vm 的条目同步回写其 todo.md 对应 V11 条目。

---

## 0. 02-stage-vm 实施 campaign 顺序表（进度真相源）

对应 `02-stage-vm/todo.md` §14 的 V11 条目与本文件 edge 条目的依赖关系。VM 侧口径：依赖共享 trap 层的条目，VM 侧逻辑完备（seam + mock 测试）即标 ✅，真实通电挂对应 edge 条目（模式 60 诚实契约）。

| 迭代 | 批次 | 内容 | 对应条目 | 状态 |
|---|---|---|---|---|
| T1 | 0 | 文档测试名 4 处 + §5.4 计数刷新 | V11-P2-6 | ✅ 2026-09-06（todo.md §15 Fix #19） |
| T2 | 0 | 过时注释 4 处（X86_64Paging 已实现） | V11-P2-4 | ✅ 2026-09-06（todo.md §15 Fix #20） |
| T3 | 0 | clippy 回归收敛 + 卫生批次 | V11-P2-3 + V11-P3-1 | ✅ 2026-09-06（todo.md §15 Fix #21；all-features 剩 :301 归 T4） |
| T4 | 0 | 删 DefaultAllocator 双真相源 + 组合语义测试 | V11-P1-3 | ⬜ |
| T5 | 1 | VmContext 第一步：parts_mut 消灭 | V11-P1-2（1/2） | ⬜ |
| T6 | 1 | VmContext 第二步：dispatcher 收 &mut VmContext + fdref/table 收敛 | V11-P1-2（2/2） | ⬜ |
| T7 | 1 | per-call codec 注册表 + dispatch 表驱动化 | V9-P2-1 + V9-P2-2 | ⬜ |
| T8 | 1 | 错误枚举收敛（errno 映射样板归一） | V10-P2-3 + P2-2 | ⬜ |
| T9 | 2 | KernelIpcTransport VM 侧完备 + KernelGateway seam | V11-P1-1（通电→E1/E2） | ⬜ |
| T10 | 2 | VFS_FDCLOSE 发送 + region close 入队 | （V11-P1-1 建议 2 / P1-3 链） | ⬜ |
| T11 | 2 | fork.rs sys_fork 真实语义 | fork.rs stub（通电→E2） | ⬜ |
| T12 | 2 | RS_PREPARE map_proc_dyn_data | rs.rs:250 DEFERRED | ⬜ |
| T13 | 2 | RS_UPDATE 步骤 5-7（VM 侧）+ 步骤 4 走 Gateway | rs.rs:328 DEFERRED（通电→E2） | ⬜ |
| T14 | 2 | exec_bootproc（minix-elf + VM 映射 + Gateway.sys_exec） | vm_server.rs:385 DEFERRED（通电→E2） | ⬜ |
| T15 | 2 | audit 日志转发（Gateway.diagctl） | audit.rs:16（通电→E2） | ⬜ |
| T16 | 2 | sanity_checks feature + usedpages 等价物 | V10-P2-1 sanity 行 + G-V11-2 | ⬜ |
| T17 | 2 | bitmap cache_freepages 三步路径 | bitmap_alloc.rs:347 DEFERRED | ⬜ |
| T18 | 2 | alloc_stats 周期 check_leak 接线 | alloc_stats.rs:46 DEFERRED | ⬜ |
| T19 | 2 | exec_newmem / DMA 三条 parity 处置（删 dead stub，不实现） | dispatcher.rs:820/:1223 | ⬜ |
| T20 | 2 | 大匿名映射懒分配落地（Reserved → fault 实化） | V9-P3-2 | ⬜ |
| T21 | 3 | 页表可注入化 + VM 内 SimPaging | V11-P2-1（QEMU 冒烟→E5） | ⬜ |
| T22 | 3 | MemType / PhysAllocator 方法级补测 | V11-P2-2 | ⬜ |
| T23 | 3 | rs_handshake/init pin 测试 + run_once 分支补测 + CI 矩阵 | V11-P2-5 | ⬜ |

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

## E5 端到端联调测试包

**问题**：VM 与其他服务器的协作当前零端到端覆盖——`os/tests/pm_vm_fork{,_test}.rs` 正文整体注释停用（自注 "DEPRECATED: permanently disabled"）；VFS fdclose、RS live-update、QEMU VM paging 冒烟均无。

**建议**：在 E1/E2 落地后建联调包：(a) PM↔VM fork 全链路（恢复/重写旧测试，改走 minix-sys 消息层而非 crate 内类型——旧失效原因正是 `pub(crate)` 边界收紧）；(b) VM↔VFS fdclose 往返；(c) RS live-update 全链路（RS_PREPARE → UPDATE → resume）；(d) QEMU VM paging 冒烟（boot shim 拉起 VM → `init_vm_self_pt` → map/query/unmap 测试页 → 串口结果，复用 `os/qemu-tests/` 基建）。
