# 【存档】02-stage-vm todo V12 轮快照（2026-09-08 审查 + 同日修复批次）

> 本文件是 2026-09-08 V12 轮（第七轮）审查完成时的 todo.md 全文快照（§0–§17，含 Fix #59–#62 修复记录与当时全部锚点）。
> 2026-09-09 V13 轮重写活文件时整体移入；此后 V12 内容以本文件为检索权威。活文件只保留 V12 开口条目的活指针（复核于 2026-09-09）。

# 02-stage-vm Rust 实现架构级 Review TODO

> 来源：2026-08-16 架构级代码审查（关注整体/分层架构，非逐函数审查）。
> 范围：`os/servers/vm/src/` 全部 Rust 代码（与 02-stage-vm 文档对应的实现）。
> 方法：整体分层分析（全局状态 → 主循环/分发 → 各子系统 → 模块），结合 Redox 实现与 Rust/OS 社区最佳实践。
> 定位：本文档是新增的架构改进建议清单，**不同于** `draft/TODO.md`（旧 fork 主线 TODO 存档）。
> **历史轮次归档（2026-09-08）**：第一轮至第六轮（T24+ 收尾 campaign）的全部条目正文与 Fix #1–#58 修复记录原文，已整体移至 [`archive/todo-V11-archive-2026-09-08.md`](archive/todo-V11-archive-2026-09-08.md)（1487 行，除顶部存档横幅外未改动）。Fix #N 编号以存档文件为检索权威；活指针（§0 速览、存量 open 条目、§16.1 缺口表）保留在本文。
> 状态（2026-09-08 更新，V12 轮，见 §17）：第七轮 = 全量查漏（Gate A 重跑 + 175 项 C 符号逐一语义判定）+ 分层架构深审（L0 整体 → L1 模块边界 → L2 子系统 → L3 trait/类型 → L4 实现层）+ Redox/OS 理论/Rust 社区三方联网对照。本轮**先审查 + 登记，后修复通电前卡点**。新发现：**1 项 P0（缺页主链 PTE 写入职责缺失）+ 1 项 wire 字宽 bug（挂 edge E-VMMCPWIRE）+ 3 项 P1 + 9 项 P2 + 2 项 P3**，缺口登记 G-V12-7..13（G-V12-1..6 为上一批次登记，见 §16.1 保留表）。**修复批次（同日，§17.9 Fix #59–#62）：两个 P0（E-VMMCPWIRE、G-V12-8）+ G-V12-10 + E-VMMOCK 依赖收口 + V12-P1-2 全部闭环**，基线升至三矩阵 **490/507/490 passed**、clippy（servers/vm）0 警告。

---

## 0. 审查结论速览

### 0.1 历史轮次（第一轮 ~ V11，详见存档）

| 级别 | 条目 | 一句话 |
|------|------|--------|
| P0 | P0-1 | `map_lazy` ↔ `get_slot` 语义矛盾（**✅ 已修复** 2026-08-16，存档 §8） |
| P1 | P1-1 | 全局 bump allocator 永不回收（**✅ 已修复** 2026-08-16：free-list v2，存档 §10 Fix #6） |
| P1 | P1-2 | 全局可变状态散落，缺单一 Context（**✅ VmContext 收敛** 2026-09-06，T5/T6，存档 §15 Fix #23/#24） |
| P1 | P1-3 | IPC transport 生产路径未落地（**✅ VM 侧完备** 2026-09-06，T9；真实通电挂 E1/E2） |
| P1 | P1-4 | `KERNEL_LAYOUT` 硬编码 mock（**✅ E3 闭环** 2026-09-08：handoff v3 带 kernel text/data span） |
| P2 | P2-1 | Dispatcher 巨型 match（**✅ 表驱动化** 2026-09-06，T7，存档 §15 Fix #26） |
| P2 | P2-2 | 每模块一套错误 enum（**⏸ 判定闭合** 2026-09-06，T8：From 集中表即最优，存档 §15 Fix #27） |
| P2 | P2-3 | `parts_mut()` 4 元组（**✅ 随 VmContext 收敛消灭** 2026-09-06） |
| P2 | P2-4 | 测试全局 static 污染（**✅ 线程本地窗口** 2026-09-07，T26，§16 Fix #46） |
| P3 | P3-1 | clippy 105 warnings（**✅ 两轮归零** 2026-08-16/09-06） |
| V9/V10/V11 全表 | V9-P0-1 … V11-P3-1 | 逐条闭环记录见存档 §9–§16（V9-P1-1 通电件、V11-P1-1 通电件除外，见 §2 存量） |

### 0.2 V12 轮速览（2026-09-08，本轮新增）

| 级别 | 条目 | 一句话 |
|------|------|--------|
| **P0** | **G-V12-8** | **缺页主链不写硬件 PTE**：`vm_pt` 在故障路径零使用，通电后 CoW/新页将二次故障循环（§17.1）（**✅ 已修复** 2026-09-08，§17.9 Fix #60） |
| P0(wire) | V12-P1-3 | vmmcp reply `addr` 字段宽 u32 而截断恒错——C 是 `void *`（64 位）；minix-types 字宽 bug，挂 edge E-VMMCPWIRE（§17.2）（**✅ 已修复** 2026-09-08，§17.9 Fix #59；E-VMMCPWIRE 闭单） |
| P1 | G-V12-7 | `SharedMemory` 无 `ev_delete` 覆写：源区域 `remaps` 永不递减 → GET_REF/`writable` 语义漂移（§17.1） |
| P1 | G-V12-9 | VM inhibit（RS 更新期间暂停服务）无实现，与 C main.c:196/265-267 不等价（§17.1） |
| P1 | G-V12-10 | warm path（`is_first_time=false`）→ `run()` 第一行 panic：C 热重启语义未对齐（§17.1）（**✅ 已修复** 2026-09-08，§17.9 Fix #61） |
| P1 | V12-P1-1 | 三分配器后端对 `PAF_LOWER*` 低内存约束行为不一致（buddy 顶层 pop 绕过 max_page）+ 后端选择零审计（§17.2） |
| P1 | V12-P1-2 | fork 可写性判定绕过 `memtype.writable`（`pr_writable` 对应物是死代码，语义与 C 分叉）（§17.2）（**✅ 已修复** 2026-09-08：`is_page_writable` 补全 pr_writable 语义并获得故障同步 + fork 两处生产调用，§17.9 Fix #62） |
| P2 | V12-P2-1..9 | 模块边界（cache 业务内联/reply 编码层属）、错误类型残余、死状态批、不变量双轨、mapcache 回滚序、contig 分配质量、文档行号漂移、region 两处（§17.2） |
| P3 | V12-P3-1/2 | Redox 对照增强集（零帧批量预映射等）+ 可观测性/卫生批（§17.2） |
| 缺口 | G-V12-11..13 | 页缓存两笔语义债、00/99 骨架文档、checklist.md 全表过时（§17.1） |

验证命令（2026-09-08 实测，V12 轮审查基线；与 2026-09-07 T32 记录的 486/503/486 一致，无回归）：
- `cargo test -p minix-vm --lib`：**486 passed / 0 failed**
- `cargo test -p minix-vm --lib --no-default-features --features segment_tree_alloc`：**503 passed / 0 failed**
- `cargo test -p minix-vm --lib --no-default-features --features buddy_alloc`：**486 passed / 0 failed**
- `cargo clippy -p minix-vm --lib`：minix-vm 自身 **0 warnings**（输出中的 37 条 warning 行全部来自依赖 crate 与 workspace profile 配置提示，见 §17.7 gate-evidence-D）

验证命令（2026-09-08 修复批次后，§17.9 Fix #59–#62）：
- `cargo test -p minix-vm --lib`：**490 passed / 0 failed**（+4：三个 PTE 同步测试 + 一个 vmmcp encode 高位测试）
- `cargo test -p minix-vm --lib --no-default-features --features segment_tree_alloc`：**507 passed / 0 failed**
- `cargo test -p minix-vm --lib --no-default-features --features buddy_alloc`：**490 passed / 0 failed**
- `cargo test -p minix-types`：**183 passed**（含新 `test_vmmcp_reply_layout_64bit_addr`）
- `cargo clippy -p minix-vm --lib`：servers/vm **0 warnings**（存量 4 条 arch 警告属 os/arch 工作区并行 WIP，非本轮引入）

---

## 1. 存量 open 条目（V11 轮遗留，全文保留）

> 以下条目原文自存档 §13/§14.3 移入，编号与锚点不变。已闭环但标题未回写的条目在 §1.1 勘误表中补标记。

### V11-P1-1 【解封】KernelIpcTransport 落地路径（原 P1-3，通电挂 E1/E2）

**状态（2026-09-08）**：VM 侧逻辑已完备——T9 三步全部完成（`KernelIpcTransport` 真实实现、`KernelGateway` seam 收敛、grant 贯通 + 假成功消灭 + fail-closed，见存档 §15 Fix #28/#29/#34）。rproctab 字节解码余件挂 **E-RSWIRE**。**本条剩余工作只有"真实通电"**，依赖 kernel trap 层（edge E1/E2），VM 侧无待办。

**验证**：E1/E2 落地后 QEMU 冒烟（edge E5）中 VM 主循环收到真实消息并回复。

### V11-P2-8 进程退出的中间页表页确定性泄漏（arch 侧 destroy 只清零不回收）

**状态（2026-09-08）**：x86_64 与 riscv64（Sv39 三级树）已完成 destroy 四级/三级回收（git b00ef4bbf / b8e26b558，对应 edge E4 主体）。**余件**：(a) aarch64 同型 destroy 回收未做；(b) VM/kernel 侧 `register_free` 接线（pt_alloc 的 free 半边在生产路径的注册）。仍挂 edge E4，VM 侧无新待办。

**验证**：`rg "fn free" os/arch/src/pt_alloc.rs` 非零（x86_64 已满足）；aarch64 paging destroy 实现后补回收测试。

### ⏸ V10-P2-3 DEFERRED 判定（错误枚举收敛，2026-08-17 判定 / 2026-09-06 T8 维持闭合）

**判定：不做大规模 errno 重构。** V10-P2-1 已把 clippy 归零；15 个 crate 内错误 enum 的"收敛"经 T8 复核判定为：现有 `From<模块错误> for VmError` 集中表（dispatcher.rs:1196-1340）+ 权威 `VmError::to_errno()`（minix-types）即最优形态，getrusage 的上下文相关映射已证明单一 From 无法表达、集中表是正确折中。**V12 轮新发现的两处残余并入本条跟踪（见 §17.2 V12-P2-3）**：`vir_region.rs:420` 的同名 `VmError` 单变体（名字碰撞）与 `CacheError`/`VfsQueueError` 无 errno 收敛路径。

**验证锚点（保留）**：`rg "pub(crate) enum .*Error" os/servers/vm/src --glob '*.rs'` 清单——收敛到 1 个对外（`minix_types::VmError`）+ 少量内部时关闭本条。

### 1.1 标题回写勘误（闭环于 T21–T23/E3/E4，原正文见存档 §14.3）

| 条目 | 闭环记录（edge_todo.md §0） | 状态标记补注 |
|---|---|---|
| V11-P2-1 X86_64Paging 零契约测试 | T21 ✅ 2026-09-06（存档 §15 Fix #39：页表可注入化 + SimPaging） | ✅ 补标（原标题缺 ✅） |
| V11-P2-2 MemType/PhysAllocator 补测矩阵 | T22 ✅ 2026-09-06（存档 §15 Fix #36） | ✅ 补标 |
| V11-P2-5 测试基建三条 | T23 ✅ 2026-09-06（存档 §15 Fix #37；rs_init pin 随 Fix #34） | ✅ 补标 |
| V11-P2-7 VmBootHandoff 补 kernel span | E3 ✅ 2026-09-08（handoff v2→3，P1-4 实质闭环；余件仅 riscv64 Sv39 VA 兼容性备注） | ✅ 补标 |

---

## 2. §16.1 缺口登记表（G-V12-1..6，上批次登记，指针保留）

> 原文见存档 §16.1。此处保留判定状态行——两条仍开口的条目是后续执行的活指针。

| 编号 | 状态 |
|---|---|
| G-V12-1 | ✅ T28（存档 Fix #48：删 PbCache 邮箱，CacheMemory 契约收敛） |
| G-V12-2 | ✅ T29（存档 Fix #49：SIGKMEM seam + do_memory 排空循环；通电挂 E1） |
| G-V12-3 | ✅ T27（存档 Fix #47：dispatcher 四函数 happy-path 补测） |
| G-V12-4 | **维持坍缩（开口）**：`RsError::UpdateKernelFailed` errno 直传的 wire 编码挂 E-RSWIRE / RS 工作流协同定案 |
| G-V12-5 | **余件收口中**：`os/servers/vm/Cargo.toml` 依赖收口已随 Fix #62 完成（default-features = false，三矩阵零差异）；arch 侧 "mock" 更名余件仍在 edge E-VMMOCK |
| G-V12-6 | ✅（dispatch_pagefault 可写性闸落地：SIGSEGV 经 gateway.sys_kill + clear_pagefault） |

---

## 17. 第七轮架构级审查：全量查漏 + 分层深审（2026-09-08，V12 轮）

> 范围：`os/servers/vm/src/` 全部 49 个 .rs（28,358 行，506 个 `#[test]`）+ C 对照面 `minix3/minix/servers/vm/`（24 个 .c 共 11,466 行）+ 26 篇编号文档的声明抽查。
> 方法：先查漏后架构——① Gate A 覆盖穷举重跑（coverage-extract.py，371 个 C 符号）+ 175 项逐一语义判定（130 函数 + 45 宏，每项先 grep 再判定）；② 分层深审 L0（整体架构/生命周期/全局状态）→ L1（模块边界/接缝/错误拓扑）→ L2（五子系统）→ L3（trait/类型/API）→ L4（实现层细节），每层以"如果今天重写会怎么设计"驱动、每个改进点给 ≥2 候选方案；③ Redox（联网核实源码与新闻）/ OS 理论（seL4/Genode/Fuchsia）/ Rust 社区（allocator/rangemap crate）三方对照。
> 产物：SYMBOLS 全量清单 `.review/claude/vm/v12/SYMBOLS.md`；本节为判定汇总与条目。**本轮未修改任何生产代码。**

### 17.0 基线复核与 Step 0 预检

| 项 | 结果 |
|---|---|
| 三 feature 测试矩阵 | 486 / 503 / 486 passed，0 failed（与 09-07 T32 记录一致，无回归） |
| clippy | minix-vm 自身 0 warnings（37 条输出行均属依赖 crate：minix-platform acpi dead_code ×3、minix-arch `SEL_CODE16` ×1、workspace profile 提示 ×24 等） |
| Step 0 预检（Gate H） | `tools/design-coverage-check.sh fork-syscall-rewrite --stage 02-stage-vm`：26/28 篇三件套 PASS；**00-vm-overview 与 99-global-concepts（均为 pending 骨架）缺 outline/outline-review/design** → Gate H.1/H.6 FAIL → 登记 G-V12-12，不阻断代码面审查（两篇骨架非本轮代码审查的依赖输入） |
| Gate A 重跑 | 371 C 符号，doc covered 91.9%（V11 轮 91.4% → 26 篇文档增量），完全缺口 30 项（其中 28 项为 include guard 与 cavl 内部宏，结构性不适用） |

### 17.1 查漏总表（Gate A：175 项 C 符号逐一语义判定）

#### 17.1.1 判定分布

| 判定 | 数量 | 说明（代表性例子） |
|---|---|---|
| COVERED | 103 | 全部给出 Rust 文件:行锚点（如 `do_mapcache` → dispatcher.rs:459、`pt_free` → vmproc_handle.rs:409、utility.c 全函数族 → boot.rs/query.rs/rs.rs/table.rs） |
| ARCH-EVOLVED | 32 | 已登记的架构演进：BTreeMap 替代 cavl（makehash/AVL_*）、Direct Map 结构性消除 reservedqueue/findhole（alloc.c:149-233、pagetable.c:155）、PFN 模型消除 pb.c slab（pb_free/pb_unreferenced → page_state.rs:269-281）、pt_bind/pt_copy 由"内核移交根表 adoption + fork 重建"替代（pagetable.c:1358/1069）、多 VM 实例显式不支持（rs.rs:498-500） |
| DEBUG-ONLY | 27 | C `#if SANITYCHECKS`/`CACHE_SANITY`（vm.h:8-9 均 =0）链 + 纯诊断打印（pt_assert、map_printmap、printmemstats——后者 C 中零调用点）；Rust 对应物为 `sanity_checks` feature 的 verify_refcounts（sanity.rs:65） |
| COMPILE-FLAG-N/A | 4 | VMSTATS（Rust 无条件实现统计，语义更优）、MEMPROTECT、JUNKFREE、`vm_pagelock`（唯一调用者在 slaballoc.c:45/52 的 `#if MEMPROTECT`=0 内，编译期即死——V12 轮 grep 复核，pagetable.c:403 之外无调用点） |
| OBSOLETE | 8 | i386 专用机制（freepde、pt_allocate_kernel_mapped_pagetables、ARCH_VM_PAGE_PRESENT 已由 PageFlags::PRESENT 承载）、earm PTF_CACHEWB/WT/SHARE 位、死定义（AM_AUTO、MINSTACKREGION 全仓零使用） |
| **REAL-GAP** | **1** | G-V12-7（下表） |

#### 17.1.2 本轮新缺口登记（G-V12-7..13）

### ✅ G-V12-8（P0-design-missing）缺页主链不写硬件 PTE：`vm_pt` 在故障路径零使用——已修复 2026-09-08（§17.9 Fix #60）

- **类型**: 设计缺失（关键决策未定义）+ 实现缺口
- **C 行为**: 缺页处理中 VM 自己写进程页表——`map_ph_writept(vmp, region, ph)`（region.c，16-pagefault.md §2 表 :181-182 记录了这一 C 步骤）+ CoW 时 `pt_writemap(..., WMF_WRITEFLAGSONLY)` 翻转写位（pagetable.c:784），完成后 `handle_memory_final` 通知内核恢复进程。
- **Rust 现状**（证据链，均已复核）：
  1. `cow_resolve_core`（cow_exec_pf.rs:205-244）只更新 VM 记账——`PageSlot` 换 pfn + `PageFrames` 引用计数，**全文件无任何 `Paging` 调用**；
  2. `handle_pagefault`（cow_exec_pf.rs:24-59）与 `dispatch_pagefault`（vm_server.rs:1389-1457）的签名与函数体都不接触页表对象；
  3. 进程页表对象确实在 VM 手里：`VmProc.vm_pt: MaybeUninit<PageTable>`（vmproc.rs:40），但只有三处写入方——`init_page_table`（vmproc_handle.rs:349）、fork 的 `write_page_table_mappings`（:501-538）、exit 的 destroy（:409-416）。**缺页路径从不写它**；
  4. 内核侧不做代写：`os/kernel/src/page_fault.rs` 只实现 RTS_PAGEFAULT 置/清状态机（:54-82），VM 回复后内核直接恢复进程，PTE 仍是旧值。
- **后果**：E1/E2 通电后，首次 CoW 写故障或匿名新页故障将这样失败——VM 记账完成并回复 Ok → 内核清 RTS_PAGEFAULT 恢复进程 → 指令重执行 → PTE 依旧只读/不存在 → **再次同一故障**，活锁。当前三矩阵 503 测试全绿，是因为 SimPaging 测试只断言 VM 记账态（`PagefaultAction`），从不断言 PTE 面——测试盲区与缺口精确重合。
- **文档侧**：16-pagefault.md §3.6 差异清单（10 行）恰好缺"PTE 写入"这一行——C 分析节记录了 map_ph_writept（:181-182），Rust 设计节却没有对应决策行，属 design-missing 的直接文档证据。
- **修改方案**（≥2 候选）：
  - **方案 A（选定建议，C-faithful）**：`handle_pagefault` 增加 `&mut PageTable`（或 `&mut dyn Paging`）参数，`dispatch_pagefault` 从 `proc.page_table_mut()` 传入；CoW 解析后对该页执行 `update_flags`（对应 WMF_WRITEFLAGSONLY，trait 方法已存在且当前 VM 零调用——正是"为未来设计"的那个着落点），新页分配走 `map`。`cow_resolve_core` 的 `unmap_page→map_page` 记账序之后追加 PTE 同步。
  - 方案 B（架构演进）：引入"VM 记账 + 批量 PTE 同步"层（fork 的 write_page_table_mappings 模式延伸到故障路径）——被否决：故障天然单页、无批量化收益，徒增两层间不一致窗口。
- **验证**：SimPaging 测试补 PTE 断言（CoW 后该 VA 的 PTE 可写且指向新 pfn；新页故障后 PTE present）——这是 E5 通电冒烟"故障回路"的单机等价物；`rg "update_flags|\.map\(" os/servers/vm/src/cow_exec_pf.rs` 非零。
- **边界**：本条与 edge E2（sys_vmctl wire）正交——写 PTE 是 VM 本地操作，不依赖内核接缝；E2 落地前实现即可被 SimPaging 测试完全驱动。

### G-V12-7（P1 语义偏移）`SharedMemory` 无 `ev_delete` 覆写：源区域 `remaps` 永不递减

- **类型**: 代码-语义不一致（C 有行为、Rust 缺失）
- **C 行为**: `shared_delete`（mem_shared.c:110-123）在共享区域释放时经 `getsrc()` 定位源区域，`assert(remaps > 0)` 后 `src_region->remaps--`。`remaps` 决定两处可观测行为：`anon_writable`（mem_anon.c:105-113，remaps>0 恒可写）与 `refcount = 1 + remaps`（VM_GETREF 返回值）。
- **Rust 现状**: 递增方向已覆盖（dispatcher.rs:1443-1467 设 `VrParam::Shared` + `table.increment_region_remaps`）；递减方向**零命中**——`SharedMemory` 未覆写 `ev_delete`（落 trait 默认 no-op，memtype.rs:34），全仓 grep `remaps` 只有递增（vmproc/table.rs:481）与三处整区复制。
- **影响**: munmap/shm_unmap/进程退出后，源区域 `remaps` 永久虚高 → VM_GET_REF 回错误计数；`AnonymousMemory::writable`（memtype.rs:256-258）的 remaps 分支恒真，偏离 C"共享方全部退出后回落 refcount==1 判定"。不泄漏内存（物理页由 PageFrames 正确维护），但属跨 IPC 可观测漂移。
- **修改方案**: 为 `SharedMemory` 覆写 `ev_delete`——从 `region.param` 取 `Shared{ep,vaddr,id}`，经 table 解析源 slot、校验 `id` 后递减（镜像 C getsrc 校验链）。签名问题：现 `ev_delete(&self, region)` 缺 table 参数——仿 `ev_pagefault` 的带参先例扩展，或在 `free_region_pages`（region/mod.rs:25-88）调用点旁路处理（推荐后者，改动面小）。**验证**: 补"remap → shm_unmap → GET_REF 回到 1"往返测试（现测试只覆盖递增半边）。

### G-V12-9（P1-design-missing）VM inhibit 机制无实现

- **C 行为**: `vm-inhibit`（main.c:196 定义、main.c:265-267 主循环门）——RS live-update 期间暂停普通 VM 请求处理，只放行更新相关消息。
- **Rust 现状**: 全仓 grep `inhibit` 仅 boot.rs:101 一处文档提及（RTS_BOOTINHIBIT 是另一回事——boot 期抑制，非更新期服务门）。`handle_rs_update`（rs.rs:346）执行期间，主循环继续接收并处理普通 VM 调用——与 C 的"更新期间 VM 冻结服务"不等价。
- **影响**: 单线程事件循环下 `handle_rs_update` 是同步完成的（处理期间本来就不收新消息），**当前实际风险低**；但 C 的 inhibit 覆盖的是跨消息的更新窗口（PREPARE 与 UPDATE 之间），Rust 的同步折叠使该窗口消失——这与 V10 批次"SUSPEND→同步化"偏差同族，应在 25-rs-services.md 的偏差表显式登记"inhibit 被同步模型吸收"，或真实 RS 需要跨消息窗口时（E9 后）补实现。**修改方案**: 先文档登记（零代码）；若 E9 联调暴露真实窗口需求，再立项（inhibit 计数器 + dispatch_on_msg 门，对照 C main.c:265-267 的三行实现）。**验证**: 25-rs-services.md 偏差表新增一行。

### ✅ G-V12-10（P1 潜伏 bug）warm path 必 panic：`is_first_time=false` 语义未对齐——已修复 2026-09-08（§17.9 Fix #61）

- **证据**: main.rs:34-36 仅 `params.is_first_time` 为真才调 `server.init()`；vm_server.rs:968 `run()` 第一行 `assert!(self.initialized, ...)`。`is_first_time=false`（C 热重启语义，boot.rs:98-102 文档化 RTS_BOOTINHIBIT 门控）→ 生产路径在 `run()` 第一行 panic。现状不可达仅因 `BootParams::simple` 恒 true（boot.rs:125）。
- **根因**: C 靠静态全局（glo.h）跨热重启存活；Rust 状态在 `VmServer` 实例内，实例随进程消亡。
- **修改方案**（≥2 候选）：A) **诚实契约（推荐）**：构造期显式拒绝 `is_first_time=false`（返回错误/panic 带清晰信息"VM 热重启未支持，见 main.c:101-108 对照"），删除"沉默走到 run() 再 panic"的间接路径；B) 实现 warm 路径（跳过 relocate、复用 BSS 持久状态）——MINIX3 语义上成立但当前无消费方，属超前实现。**验证**: 补一条 `is_first_time=false → 明确错误` 测试。

### G-V12-11（P2）页缓存域两笔语义债（修正旧"数据拷贝"表述）

- **勘误**: 上一批次 open 项指针中"24-page-cache 数据拷贝"经 L2 复核**表述失真**——mapcache/setcache 已是 PFN 零拷贝链接（dispatcher.rs:518 map_page 直链、:661-672），不存在逐字节拷贝。真实差异是两笔：
  1. **ONCE 条目统一走 NeedVfsIo 多一次 VFS 往返**（memtype.rs:1006 起；C 命中缓存直接链接）；
  2. **clearend 尾页清零未建模**（memtype.rs:1078-1081 注释自认 "clearend zeroing is not yet modeled"）。
- **修改方案**: 两笔均挂 24-page-cache 域批次；clearend 需在链接时对 [len % PAGE_SIZE] 尾页做清零或显式登记为 fail-closed 拒绝。**验证**: 24-page-cache.md 偏差表对应行刷新。

### G-V12-12（P2 doc）00/99 骨架文档 + design 快照缺失

00-vm-overview.md 与 99-global-concepts.md 为 pending 骨架，且 `.design/` 三件套缺失（design-coverage-check 判 CRITICAL）。按 Step 0.3 惯例应补 outline/outline-review/design 或正式宣布两篇的完成计划。**验证**: 重跑 coverage-check 全 PASS。

### G-V12-13（P2 doc）checklist.md 全表过时，需系统性复检

checklist.md 生成于 2026-06-12（总体 59%），早于 V9–V11 与 T1–T36 campaign 的大面积落地。本轮已加复检横幅 + 抽分行修正（本轮直接证据覆盖的行）；**系统性刷新**（按 §17.1.1 的 175 项判定逐类重写六张表）另立批次执行——它本身就是一轮完整 coverage 复核的工作量。**验证**: checklist §0 总览表数字与 §17.1.1 判定分布对账。

### 17.2 分层架构审查条目（L0→L4）

### ✅ V12-P1-3（P0，wire 面）vmmcp reply `addr` 按字段宽 bug：u32 恒截断，C 是 64 位 `void *`——已修复 2026-09-08（§17.9 Fix #59，E-VMMCPWIRE 闭单）

- **证据**: C `mess_vmmcp_reply.addr` 是 `void *`（minix3/minix/include/minix/ipc.h:2395-2400，x86_64 上 64 位；C 赋值 `msg->m_vmmcp_reply.addr = (void *) vr->vaddr`，mem_cache.c:170）。Rust 侧 minix-types `MessVmmcpReply.addr: u32`（os/libs/minix-types/src/ipc/message.rs:2512-2515），且该字段的文档注释自述 "payload offset 0, **32-bit pointer**"——注释本身即是与 C ipc.h 矛盾的事实性错误（64 位移植上 `void *` 是 64 位）。VM 编码随之截断：`reply.addr = addr.0 as u32`（vm_server.rs:1704）。mapcache 的分配地址走 MMAP 窗口（MMAP_BASE = 0x1_0000_0000，mmap.rs:204）→ **被截断的高 32 位恒非零，错误恒发生**（非偶发）。同簇：mmap.rs:373 `length: aligned_len.0 as u32`——VFS fd-lookup 请求（`VfsRequest.length: u32`）对 >4GB 映射静默截断请求长度。
- **类型**: wire 契约 bug——minix-types 属共享基础设施（edge 判定①类），**跨 stage 条目挂 edge E-VMMCPWIRE**（§17.4）；VM 侧半边（去截断 + 回放测试）留在本条。
- **修改方案**: A) minix-types 字段 `addr: u64`（对方消费面 minixfs/lib 尚不存在，现在改零成本）+ VM 侧 `as u64`；B) 维持 u32 但约束 mapcache 分配 < 4GB——否决：为迁就字段宽度扭曲分配器布局，本末倒置。**验证**: wire 回放测试断言 addr 高位保全；`rg "as u32" os/servers/vm/src/ipc/ os/servers/vm/src/vm_server.rs` 中地址/长度类截断归零。

### V12-P1-1（P1）三分配器对低内存约束行为不一致 + 后端选择不可观测

- **证据**:
  1. `BuddyAllocator::alloc_block` 顶层 `pop_free`（buddy_alloc.rs:265-270）不检查 `max_page`——带 `PAF_LOWER16MB/LOWER1MB`（phys_mem/types.rs:90-91）的请求若顶层空闲链有块则直接返回，可能越过 16MB/1MB 边界；`max_page` 只在分裂递归（:276）生效。bitmap 的 `find_bit`（:213-264）以扫描区间约束、segment_tree 分配后校验（:270-273）——**同一请求三后端三种行为**，而 parity 测试（allocator_tests.rs，46 条）未覆盖低内存 flag × 后端矩阵。
  2. `choose_allocator_type`（vm_server.rs:359-374）与 `relocate`（:376-444）全程无审计日志——生产上无法事后发现"请求了 buddy 实际跑了 bitmap"。且 relocate 的 Bitmap 回退臂（:421-424/:431-434）在当前 cfg 结构下**不可达**（`choose_allocator_type` 只在对应 feature 开启时才返回该后端，而 feature 开启时对应臂必编译）——防御性死代码伪装成活路径。
- **修改方案**（低内存约束，≥2 候选）：A) buddy 顶层 pop 补 max_page 过滤——各后端各自正确，但三处逻辑继续漂移风险高；**B（选定）把 maxpage 约束上移 `PhysAlloc` 门面**（phys_mem/mod.rs 分发层统一过滤/校验，后端无感知）——单点权威，parity 测试一次覆盖。审计：relocate 完成后 `audit_log!` 一行"后端选择结果 + 页数"；回退臂要么删除要么注释明示"当前 cfg 下不可达"。**验证**: parity 测试补 `PAF_LOWER*` × 三后端矩阵（≥6 条）；`rg "alloc_cycle|relocate" os/servers/vm/src/vm_server.rs` 处审计行存在。

### ✅ V12-P1-2（P1）fork 可写性判定绕过 `memtype.writable`（pr_writable 语义分叉）——已修复 2026-09-08（§17.9 Fix #62）

- **证据**: C 的页写权限判定 `pr_writable = VR_WRITABLE && mem_type->writable()`（region.c:130-133）。Rust 对应物 `VirRegion::is_page_writable`（vir_region.rs:300-314，memtype 参与）**完整实现却标 `#[allow(dead_code)]`**（:299）；fork 的 `write_page_table_mappings`（vmproc_handle.rs:518-526）实际使用简化判定 `region.is_writable() && refcount == 1`，不走 memtype。
- **影响**: memtype 声明不可写但 region 标 WRITABLE 的页（如 MappedFile 恒 false，memtype.rs:995）在 fork 时按简化判定落入只读——方向保守（安全侧），但与 C 语义分叉，且真正实现 C 语义的函数是死代码。`vmproc_handle.rs:527-529` 的注释引用 C pr_writable 却未实现其完整判定。
- **修改方案**: fork 路径改调 `is_page_writable`（顺带删除 dead_code 标注）；若折叠进 PTE 写入改造（G-V12-8）则在该条一并处理。**验证**: 补 mappedfile region fork 后页表 RO 的对偶测试；`rg "allow(dead_code)" os/servers/vm/src/region/vir_region.rs` 该处归零。

### V12-P2-1（P2）模块边界：cache 业务内联在 dispatcher + reply 编码住在 vm_server

- **证据**: `dispatch_mapcache/setcache/forgetcache/clearcache`（dispatcher.rs:459-709，约 250 行）把 mem_cache.c 的对齐校验、页分配、失败回滚内联在分发层；回复编码 `reply_to_errno`/`encode_reply_data`（vm_server.rs:1643-1833，约 190 行纯线格式代码）住在主循环文件而非 ipc/。
- **方案**: cache 四操作下沉 `page_cache.rs`（dispatcher 只留解码→调用→编码，对齐 T7 表驱动化方向）；reply 编码迁 `ipc/`（如 ipc/encode.rs）。两者均为纯移动重构，不改行为。**验证**: dispatcher.rs 行数下降 ≥200；`cargo test` 三矩阵持平。

### V12-P2-2（P2）`memtype` 反向依赖 `vmproc`

- **证据**: memtype.rs:7 `use crate::vmproc::{ActiveProc, VmProcTable}`——内存类型策略层依赖进程表（起因：MappedFile 的 fd 解析需要查进程 fdref）。
- **方案**: 比照 `ev_pagefault` 的先例（table 显式传参），把 `MappedFile` 需要 table 的方法全部改为显式传参，trait 回归"纯策略"定义；或把 fd 解析上移到 dispatcher/fdref 层、memtype 只收解析结果。前者改动面小，推荐。**验证**: `rg "use crate::vmproc" os/servers/vm/src/memtype.rs` 归零（trait 定义处）。

### V12-P2-3（P2）错误类型两处残余（并入 V10-P2-3 收敛面）

vir_region.rs:420 的第二同名 `VmError`（单变体 `InvalidParam`，与 minix_types::VmError 同名异型，跨模块零使用）→ 更名 `VirRegionError` 或折叠；`CacheError`/`VfsQueueError` 无到 VmError/errno 的映射路径（dispatcher.rs:1812 直接外泄 VfsQueueError 结果）→ 补 From。**验证**: `rg "enum VmError" os/servers/vm/src` 命中 0（crate 内）；两错误类型可经 `?` 直达 VmReply::Error。

### V12-P2-4（P2）死状态/死链路批（逐项"删/接线/标注"三分处置，对照 T19 先例）

| 锚点 | 内容 | 处置建议 |
|---|---|---|
| global.rs:16/:51-58 | `TOTAL_PAGES` 生产读者为零（仅 cfg(test) 读） | 删（读者已改走 page_alloc.total_pages()） |
| mmap.rs:137 | `FILEMAP_ENABLED` 无生产写者（写点全在测试） | 删或接真实 enable_filemap 面 |
| dispatcher.rs:1001-1003 + vm_server.rs:1655/:1698 | `VmReply::ExecNewmem` 无生产构造者（调用不注册 + 死编码臂） | 删变体与编码臂 |
| vm_server.rs:802/:971 | `mark_alloc_failure`→`missing_spares`→`alloc_cycle` 生产链悬空（无写入点，钩子永不触发；实际回收走 alloc_pfn_reclaiming funnel，global.rs:498-502） | 接线（alloc 失败处调用）或删除压力计数语义——(vm_server.rs:152-171) 的注释声称的契约与生产现实不符 |
| vir_region.rs:251-288 | lazy 家族（`map_lazy`/`Reserved` 态/`get_slot_any`）全 dead_code | P0-1 修复后遗留；删或补消费方 |
| vmproc_handle.rs:623-656 | `ExitingProc` 的 `slot`/`endpoint`/`flags`/`regions` 四访问器 dead_code（`regions_mut` 是唯一生产消费面，exit.rs:54 在用） | 删四个死访问器，保留 `regions_mut` + `reap` 最小面 |
| vmproc.rs:136-143 | `VmProc::check()` debug 不变量检查零调用 | 在 activate/reap 调用或删 |

**验证**: 每项处置后 `cargo test` 三矩阵持平 + `rg` 锚点归零。

### V12-P2-5（P2）不变量双轨：proc_table 双访问路径 + page_frames 双处置

`VmContext.proc_table` 字段与 `VmProcTable::get_global()` 并存（vm_server.rs:1024/:1319 直接走全局）——同一数据两条访问路径，收敛未完成的痕迹；"page_frames 未初始化"不变量两种处置（dispatcher 20 处 `expect` vs vm_server.rs:1379-1382 软错误）。**方案**: 单线程服务中全局表即唯一真相——删除 ctx 字段（或收为私有别名）；page_frames 统一 expect + 一处集中注释。**验证**: `rg "get_global\(\)" os/servers/vm/src/vm_server.rs` 与字段访问二选一归零。

### V12-P2-6（P2）mapcache 失败回滚顺序与 C 相反

Rust 先逐页映射后插 cache 索引，失败靠手工 `unmap_region_pages` 回滚（dispatcher.rs:501-556）；C do_mapcache 先查 cache/注册再 map_pf（mem_cache.c:141-163），失败路径天然无需回滚已链接页。回滚遗漏即 refcount 泄漏。**方案**: 改为 C 序（命中→链接→映射）或至少把回滚收敛为单函数 + 中途失败测试。**验证**: mapcache 中途失败注入测试（第 N 页失败 → 前页 refcount 归位）。

### V12-P2-7（P2）`ContiguousAnonymous::ev_new` 质量债（与 memtype.rs:808 TODO 同根）

逐页分配后 `windows(2)` 事后验证连续性 + 失败逐页回滚（memtype.rs:747-830），而 `PageAllocFlags::CONTIG`（phys_mem/types.rs:88）与分配器侧连续分配面已存在未用；:803-806 注释声称"fallback 非连续页"与 :811-813 实际行为矛盾。**方案**: 分配器侧实现 `alloc_contiguous`（buddy/segtree 天然支持；bitmap 需连续域扫描——C findbit 即此语义），`ev_new` 改单调用；顺带闭合 :808 TODO 与矛盾注释。**验证**: `rg "alloc_contiguous" os/servers/vm/src` 非零且 ev_new 调用之；contig 测试改断言单次分配语义。

### V12-P2-8（P2）文档-代码同步批（C 锚点漂移 + 过时差异行）

| 锚点 | 问题 | 修正 |
|---|---|---|
| dispatcher.rs:434/465 | 注释称对齐检查在 mem_cache.c:99-101/102-103 | 实际 :108-110/:116 |
| dispatcher.rs:436/470 | 同簇 EINVAL 行号 :107 | 实际 :116 |
| fork.rs:425 | "do_fork.c panics" | 实为 fork.c:91-104（文件名错） |
| 16-pagefault.md §3.6 #5 | "缺页计数生产路径未调用（仅测试）" | 已过时——vm_server.rs:1445-1450 已接线 inc_minor/major_fault（G-V12-6 批次落地） |

**验证**: 按 fix-guard 逐条修后 `sed -n` 复读 C 对应行。

### V12-P2-9（P2）region 两处：find_overlap 线性扫 + 零长区间静默替换

`find_overlap`（vir_region.rs:92-102）自 BTreeMap 首端线性扫到 end（最坏 O(n)），与 insert 注释"只查两个最近邻居"（:179-180）不符 → 改 `range(..=vaddr).next_back()` 邻居判定；`overlaps` 对 length==0 恒 false（:162-164）→ 同 vaddr 的 insert 静默替换旧 region（:176/:184）→ insert 入口拒绝 length==0。**验证**: 大区间数下 find_overlap 基准/复杂度断言（或实现即满足）；零长 insert 测试返回 Err。

### V12-P3-1（P3）Redox 对照增强集（联网核实，均为增强非缺陷）

1. **零帧共享 + 批量预映射**：Redox 以单一零帧只读映射服务全部 lazy 零页、写时才复制，且每次故障批量预映射 `MAX_EAGER_PAGES=16`（redox-os/kernel `src/context/memory.rs`）。对照：VM 的 `alloc_and_map`（cow_exec_pf.rs:173）逐页单帧分配——连续故障场景（exec 后首触）可批量预映射相邻页，减少 IPC 往返。
2. **Provider 五分类**：`Allocated/AllocatedShared/PhysBorrowed/External/FmapBorrowed`（Redox Grant Provider 枚举）是 `VrParam` 演进的现成模板——尤其 `External`（跨地址空间借用）对 RS live-update 的 share_mappings 语义有参照价值。
3. **find_free 双索引**：Redox 用 `BTreeMap<Page, GrantInfo>` + holes_by_addr/holes_by_size 双索引做 O(log n) 空洞查找——`region_map.rs` 当前无 find_free 需求，记账备用。
4. **per-frame 引用计数**：Redox `PageInfo.refcount`（atomic，含 Cow 位编码）与本项目 `PageFrames` 模型同构（cow_resolve_core 的 refcount<=1 快速路径 = Redox wp_page_reuse）——**良好对齐，无动作**。

### V12-P3-2（P3）可观测性与卫生批

1. `audit_log!` 在 release 无 feature 时整体编译剔除（lib.rs:53-56）——全部 fail-closed 降级路径生产不可观测；核对 VM_INFO 的计数器面（pagefault_errors/dropped_messages 已在 VmContext）是否足够，足够则把 lib.rs 注释改为指向计数器，不足则登记 feature 设计议题。
2. memtype.rs 6 个 pub fn 无 `///` 文档（0/6，trait 方法用 `//`）。
3. `IpcTransport` 等 6 类型 `pub` 应 `pub(crate)`（ipc/transport.rs:49-278；crate 内 re-export 面仅 lib.rs:87-90 三项，pub 是过宽）。
4. bitmap `lastscan` 提示优化未做（bitmap_alloc.rs:189 自认 future；C alloc.c:430-441 对照）——保持登记即可。

### 17.3 Redox / OS 理论 / Rust 社区对照注记（URL 均已核实）

| 事实 | 来源 | 对本项目的意义 |
|---|---|---|
| Redox 物理账本与页表全部在 kernel（`src/memory/` 仅 mod/page/kernel_mapper 三文件：NUMA buddy + `PageInfo` per-frame 元数据 + `Section`≤128MiB；旧 rmm crate 已 vendor 进 kernel 仓库） | redox-os/kernel GitLab master | 用户态无 VM 服务——与 MINIX3 的"外置 VM 服务器"互为镜像取舍。本项目维持外置 VM 是 MINIX3 语义忠实，不构成劣势；集中式账本 + 单线程事件循环的模型简单性是真实收益（无需 TLB shootdown 延迟 free） |
| Redox 无内核统一 page cache：fmap 页即用户内存，缓存职责在 redoxfs 等用户态 daemon | 官方新闻 kernel-9 + 源码 | 本项目 VM 内 LRU page_cache 是 MINIX3 集中式做法——需自证不与文件服务重复缓存（24-page-cache 域既有议题，G-V12-11 关联） |
| Redox COW：per-frame `RefCount::One|Shared(n)|Cow(n)` 位编码；refcount=One 时 `wp_page_reuse` 原地复用 | kernel `src/context/memory.rs` | 本项目 `PageFrames` refcount + `cow_resolve_core` refcount<=1 快速路径（cow_exec_pf.rs:218-220）与之同构——**对齐良好** |
| seL4：frame/页表对象由用户 VMM 持有并 retype/map；Genode：core 的 RM session 记账、RAM session 供给；Fuchsia：kernel pager + 用户态供给 | 各官方文档 | 本项目"VM 服务器持全部账本 + vm_pt 对象"与 seL4 的用户态 VMM 模式最接近——G-V12-8 的 PTE 写入职责落 VM 侧与此一致 |
| Rust 社区：`buddy_system_allocator` 0.13（no_std，2026-03 仍活跃；Redox 未使用、自研 NUMA buddy）；`rangemap` 1.8 / `btree_range_map` 0.8 / `range_set_blaze` 0.6.1 | crates.io | 三分配器自研维持合理（教学价值 + 特化语义 max_page/CLEAR）；若要减代码可对照 buddy_system_allocator 评估，但非必要。region_map 的自研 BTreeMap 方案在 no_std 权衡下维持 |

### 17.4 edge 增补指针（本轮新登记，见 edge_todo.md）

| edge 编号 | 内容 | 对应本文件条目 |
|---|---|---|
| **E-VMMCPWIRE**（新） | minix-types `MessVmmcpReply.addr` u32 → 应为 64 位（C `void *`）+ mmap reply length 字宽核查；共享契约①类 | V12-P1-3 |
| **E-VMMOCK**（新） | G-V12-5 余件收口：`os/servers/vm/Cargo.toml:18` 补 `default-features = false` + arch "mock" 命名澄清（运行时窗口语义）——E3 完成注记未含此项，防孤儿 | §2 表 G-V12-5 行 |
| **E5 增补** | 通电冒烟必须覆盖"缺页 → VM 写 PTE → 指令重执行不再故障"完整回路——G-V12-8 的端到端验收面 | G-V12-8 |

### 17.5 测试面轻审计

- 测试分布健康：506 个 `#[test]`（allocator_tests 46 / vm_server 45 / dispatcher 34 / query 25 / bitmap 23 / memtype 22 / rs 20），三 feature 矩阵全绿。
- 本轮各条目的"验证"字段已内嵌测试要求（remaps 往返、PAF_LOWER parity 矩阵、mapcache 中途失败注入、PTE 断言、warm path 拒绝）。
- **测试盲区与缺口重合**：SimPaging 断言面 = VM 记账态，PTE 面零断言（G-V12-8 的直接后果）——SimPaging 基建已就位（T21），补 PTE 断言成本低。
- 完整 5 维 test-audit（完备/自身正确/冗余/无效/虚构）是独立 cmd，本轮不做，建议后续单列。

### 17.6 Rule Discovery（Step 5.7）

1. **新模式候选（建议入库 pattern 84）**："wire 字段宽度必须对照 C ipc.h 结构逐字段断言"——vmmcp_reply.addr 在 C 是 `void *`（64 位），minix-types 手抄为 u32（32 位直觉），错误在两侧类型各自成立、只在对接面暴露。检查命令：对 `os/libs/minix-types/src/ipc/message.rs` 每个 VM 消息结构，与 `minix3/minix/include/minix/ipc.h` 对应 `mess_*` 结构逐字段比宽（含 `_ASSERT_MSG_SIZE` 对应的 56 字节 payload 上限）。
2. **防御性不可达臂**：relocate 的 Bitmap 回退臂注释声称的场景与 cfg 结构矛盾（永远不可达）——防御性代码若其防御场景在当前编译配置下不可达，注释必须明示"当前 cfg 下不可达"，否则伪装成活路径误导维护者。
3. **name-match 覆盖率与语义覆盖的巨大落差第三次验证**：name-match 11.6% vs 语义判定 COVERED 103/130——Gate A 的语义映射兜底规则持续有效，覆盖率工具的 name-match 数字本身不构成缺口信号。

### 17.7 gate-evidence

```
gate-evidence-Step0:
$ bash tools/design-coverage-check.sh fork-syscall-rewrite --stage 02-stage-vm
| 01-vm-init-main … 26-vm-queries | ✅ 全部三件套 | ✅ PASS |
| 99-global-concepts | ❌ | ❌ | ❌ | ❌ CRITICAL (H.1+H.6 FAIL) |
Summary: Total docs 31 / Complete 26 / missing outline 2 / outline-review 2 / design 2
（缺失项 = 00 与 99 两篇 pending 骨架 → G-V12-12）

gate-evidence-A:
$ python3 tools/coverage-extract/coverage-extract.py vm \
    notes/rewrite/fork-syscall-rewrite/02-stage-vm \
    --rust-dir os --c-dir minix3/minix/servers/vm \
    --semantic-map tools/coverage-extract/vm-semantic-map.json \
    --output .review/claude/vm/v12/SYMBOLS.md
Loaded semantic map: 81 entries
Found 371 C symbols (189 funcs, 14 structs, 167 macros, 1 enums)
Coverage Summary for vm:
  Total C symbols: 371
  Doc covered: 341 (91.9%)
  Rust covered (name-match): 43 (11.6%)  ← name-match 低估，语义判定见 §17.1.1

gate-evidence-基线:
$ cargo test -p minix-vm --lib                                    → 486 passed / 0 failed
$ cargo test -p minix-vm --lib --no-default-features --features segment_tree_alloc → 503 passed / 0 failed
$ cargo test -p minix-vm --lib --no-default-features --features buddy_alloc        → 486 passed / 0 failed

gate-evidence-D（Gate D 五项，代码面抽查）:
1. §5 测试存在：本轮新增条目验证字段均给出测试名要求（未实现前不计 ✅）
2. trait ≥2 impl：PhysAllocator ×3 后端 + 门面、MemType ×6、KernelGateway ×2（生产+Mock）、IpcTransport ×2 —— `rg "impl.*PhysAllocator|impl.*MemType|impl KernelGateway|impl IpcTransport" os/servers/vm/src` 非零 → ✅
3. 函数在声明文件：§17.1.1 COVERED 103 项锚点逐一给出 file:line → ✅
4. 核心算法非 stub：`rg "todo!|unimplemented!" os/servers/vm/src --include='*.rs'` 生产代码零命中（18 处 TODO/DEFERRED 均为文字标记非宏）→ ✅
5. §4 签名一致：抽查 16-pagefault/12-memtype/18-vm-fork 的 Rust 签名行与文档一致 → ✅

gate-evidence-关键论断复核（防转述失真，主 agent 亲自 grep/读源）:
- vm_pagelock 唯一调用者：`grep -rn vm_pagelock minix3/minix/servers/vm/` → slaballoc.c:45/52（#if MEMPROTECT=0 内）+ proto.h 声明 + pagetable.c:403 定义
- C vmmcp_reply 宽度：`sed -n '2395,2400p' minix3/minix/include/minix/ipc.h` → `void *addr`
- vm_pt 存储：`grep -n "vm_pt" os/servers/vm/src/vmproc/vmproc.rs` → :40 `MaybeUninit<PageTable>`
- 故障路径无 PTE 写入：`grep -n "Paging\|update_flags\|\.map(" os/servers/vm/src/cow_exec_pf.rs` → 仅 PageSlot/PageFrames 记账
- mock 泄漏：`grep -n "minix-arch" os/servers/vm/Cargo.toml os/Cargo.toml os/arch/Cargo.toml` → path 依赖未关 default + default=["mock"]
- warm path：main.rs:34-36 + vm_server.rs:968 原文已读
```

### 17.8 建议的推进顺序（2026-09-08 修复批次后更新）

1. ~~**E-VMMCPWIRE**~~ ✅（§17.9 Fix #59——minix-types + VM 双侧闭合，E-VMMCPWIRE 闭单）；
2. ~~**G-V12-8**~~ ✅（§17.9 Fix #60——故障/主动两条主链的 PTE 同步全部落地，SimPaging 断言覆盖）；
3. G-V12-7 / V12-P1-1（剩余两个 P1 语义修正，各带测试；V12-P1-2 已随 Fix #62 闭合）；
4. ~~G-V12-10~~ ✅（§17.9 Fix #61）+ V12-P2 批（按 4→8→9→5→6→3→1→2→7 顺序，先不变量后移动重构）；
5. G-V12-11..13 文档批 + V12-P3 批（机会主义）；
6. edge 侧并行：~~E-VMMOCK 依赖收口~~ ✅（§17.9 Fix #62，arch 侧"mock"更名余件仍在 edge）、E5 增补验收面（PTE 回路已有单机 SimPaging 断言，QEMU 冒烟仍待 E1/E2）。

### 17.9 V12 修复记录（2026-09-08，通电前卡点批次）

### ✅ Fix #59: E-VMMCPWIRE — vmmcp reply `addr` u32 → u64（wire 字宽，V12-P1-3 闭合）

- **问题**：minix-types `MessVmmcpReply.addr: u32`（注释自述 "32-bit pointer"），而 VM 的 mapcache 分配地址恒在 MMAP 窗口（≥0x1_0000_0000）——每次回复的高 32 位都被截断。C `mess_vmmcp_reply.addr` 是 `void *`（ipc.h:2395-2400），且该 C 头文件布局按 i386 指针宽书写（4+1+51=56），64 位下本就无法放入 56 字节 payload。
- **设计（对照先例）**：minix-rs 的 wire 目标是 x86_64，采用与 `m_vm_pagefault` 专用 overlay（doc 16 §3.6 #2）相同的"诚实 64 位布局"决策——`addr: u64 @0, flags: u8 @8, padding[47] @9..56`，总宽保持 56 字节。消费方（minixfs/lib 的 vm_map_cacheblock 等价物）尚不存在，现在改零成本。
- **Files**: `os/libs/minix-types/src/ipc/message.rs`（结构体 + 文档注释改写 + Default + 新测试 `test_vmmcp_reply_layout_64bit_addr`）、`os/servers/vm/src/vm_server.rs`（`encode_reply_data` MapCache 臂去 `as u32` + 新测试 `test_encode_mapcache_reply_preserves_high_addr_bits`）、`os/servers/vm/src/vfs_queue.rs`（`VfsRequest.length: u32 → u64`，>4GiB FdLookup 不截断）、`os/servers/vm/src/mmap.rs`（:373 去 `as u32`）、`os/servers/vm/src/cow_exec_pf.rs`（enqueue_fdio 的 length 构造）
- **Verified**: `cargo test -p minix-types` → 183 passed（含新布局测试）；`cargo test -p minix-vm --lib` → 490 passed；`cargo clippy -p minix-vm --lib` servers/vm 0 警告
- **Docs**: edge_todo.md E-VMMCPWIRE 闭单注；本条即判定记录
- **边界**：`VfsRequest.length` 拓宽只改 VM 内部队列结构——VFS wire 本身的字段宽在 E-VFSWIRE（VFS_VMCALL 定稿）时一并对账

### ✅ Fix #60: G-V12-8 — 缺页主链 PTE 同步（`sync_slot_pte`，通电语义最后一公里）

- **问题**：`handle_pagefault` 链只更新 PageSlot/PageFrames 记账，进程页表对象 `vm_pt`（vmproc.rs:40）在故障路径零使用；内核侧只做 RTS_PAGEFAULT 状态机。通电后首次 CoW 写故障/需求页故障将无限二次故障（活锁）。16-pagefault.md §3.6 差异清单恰好缺该行（design-missing）。
- **设计（方案 A，C-faithful）**：`handle_pagefault` 链路贯穿 `&mut PageTable`；新增 `sync_slot_pte(region, frames, offset, pt)`——以 `is_page_writable`（C pr_writable）定权限位，三路分派：`query` 同帧 → `update_flags`（WMF_WRITEFLAGSONLY）、换帧 → `remap`（WMF_OVERWRITE）、无映射 → `map`。挂在四个结算点：`Handled`（memtype 自解析，如缓存命中）、`alloc_and_map`（需求页）、`cow_resolve_core` 慢路（CoW 拷贝后 PTE 指向私页）与快路（refcount≤1 只翻写位）。主动路径同步覆盖：`handle_memory_once`（SIGKMEM/procctl HANDLEMEM/fork 预填充/rs pin）与 `map_pin_memory` 加 `pt` 参数贯通。
- **Files**: `os/servers/vm/src/cow_exec_pf.rs`（sync_slot_pte + 全链签名 + `CowError/CowCoreError::PageTable` 变体）、`os/servers/vm/src/vmproc/vmproc_handle.rs`（新 `mem_parts_mut()` 互斥借访问器——regions 与 vm_pt 是不同字段，`regions_mut`/`page_table_mut` 无法同时借）、`os/servers/vm/src/vm_server.rs`（dispatch_pagefault 可写性闸改只读借用 + mem_parts_mut 分割；handle_kernel_memreq 贯通）、`os/servers/vm/src/fork.rs`（handle_memory_once/cow_copy_page 加 pt；fork 预填充子/父两半）、`os/servers/vm/src/region/mod.rs`（map_pin_memory 加 pt）、`os/servers/vm/src/exit.rs`（procctl HANDLEMEM）、`os/servers/vm/src/rs.rs`（PREPARE src/dst 两处 pin + memctl Pin）
- **测试（新增 3 + 改造 11）**：`test_demand_fault_maps_pte_present`（需求页 → PTE present+RW）、`test_cow_fault_pte_repoints_to_new_frame`（CoW → PTE 指向新帧 RW）、`test_cow_fast_path_flips_pte_writable_without_recopy`（refcount==1 写故障 → memtype 判 `Handled`，PTE 同帧翻写位——修正了"私页写故障走 NeedCow"的错误预期，`anon_pagefault` 对 refcount<2 恒判 Handled）；既有 11 处调用点补 pt 参数
- **Verified**: 三矩阵 490/507/490 passed；clippy servers/vm 0 警告
- **Docs**: 16-pagefault.md §3.6 新增 PTE 行（见该文件 2026-09-08 增补）

### ✅ Fix #61: G-V12-10 — warm path 诚实契约（显式拒绝热重启）

- **问题**：`main.rs` 仅 `is_first_time=true` 调 `init()`，`is_first_time=false` 时静默走到 `run()` 第一行 `assert!(self.initialized)` panic——C 热重启语义（BSS 全局跨重启存活）在 minix-rs 无对应物，失败点远离根因。
- **Files**: `os/servers/vm/src/main.rs`（else 臂显式 panic 并指向 C main.c:101-108 对照与 G-V12-10 登记）
- **Verified**: `rg "warm restart" os/servers/vm/src/main.rs` 命中；binary 入口无测试面（`#[cfg(test)]` 跳过 main），验证为锚点 grep + 逻辑审查——诚实标注 UNVERIFIED-for-test 及原因
- **Docs**: 本条即判定记录

### ✅ Fix #62: E-VMMOCK 依赖收口 + V12-P1-2 — minix-arch default-features 关闭 + pr_writable 补全

- **E-VMMOCK 半 A（本轮完成）**：`os/servers/vm/Cargo.toml` 的 minix-arch 依赖补 `default-features = false`（对齐 kernel/boot-shim 惯例），arch `default=["mock"]` 不再泄漏进 VM 生产构建；三矩阵回归零差异。**余件（仍在 edge E-VMMOCK）**：arch 侧 `mock` feature 更名为运行时窗口语义的诚实名字（涉 kernel/boot-shim 引用，跨 crate 单独执行）。
- **V12-P1-2（全条闭合）**：`VirRegion::is_page_writable` 补全 C `pr_writable` 语义（region.c:130-133：`VR_WRITABLE && mem_type->writable`——原实现缺 VR_WRITABLE 合取且标 dead_code），删除 `#[allow(dead_code)]`；`write_page_table_mappings`（fork）从简化判定 `is_writable() && refcount==1` 切换到 `is_page_writable`——MappedFile 等永不可写类型不再获得 RW PTE；故障路径 `sync_slot_pte` 同源消费（单一权威，无双真相源）。
- **Verified**: 三矩阵 490/507/490 passed；`rg "allow(dead_code)" os/servers/vm/src/region/vir_region.rs` 该处归零；`rg "default-features" os/servers/vm/Cargo.toml` 命中
- **Docs**: 本条即判定记录；V12-P1-2 条目标 ✅

---

## 存档指引

- 第一轮至第六轮全部条目与修复记录：[`archive/todo-V11-archive-2026-09-08.md`](archive/todo-V11-archive-2026-09-08.md)
- 本轮 SYMBOLS 全量清单：`.review/claude/vm/v12/SYMBOLS.md`（中间产物，正式引用以本文件 §17.1 汇总为准）
- 跨 stage 条目唯一入口：`notes/rewrite/fork-syscall-rewrite/edge_todo.md`（§17.4 三条增补）
