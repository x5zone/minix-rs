# Direct Map 设计变动 — 文档修改 TODO

> **创建日期**: 2026-05-08
> **状态**: 待讨论
> **设计依据**: `ptregion_design.md` §8 Direct Map 设计方案

---

## §0 架构 Overview

> 本节是 TODO 执行者的指引，确保修改后的正式文档连贯、有启发性。
> TODO 本身是临时工具，最终语义内容融入 02-stage-vm 文档后，TODO 删除。

### 0.1 VM 与 Kernel 的分工

| 角色 | 职责 | 对 direct map 的关系 |
|------|------|---------------------|
| **Kernel** | 机制执行者：CR3 切换、TLB 刷新、MMU 操作、建立初始页表 | Kernel direct map（U/S=0）是 kernel execution substrate，属于机制不变量 |
| **VM** | 策略制定者：地址空间构造、页表数据编辑、内存分配策略、CoW/fork 策略 | VM direct map（U/S=1）是 VM 执行策略的工具，属于 VM 私有映射 |

**核心原则**：VM 是地址空间的建筑师（address-space architect），Kernel 是施工队。VM 决定地址空间长什么样，Kernel 负责把图纸变成硬件现实。

**更深层**：VM 是 memory manager，不是 MMU controller。VM 把页表当普通数据读写（通过 direct map），kernel 才是 commit 到硬件的执行者。这是策略/机制分离的最深层体现。

**Direct map 的归属**：
- Direct map 是 CPU/MMU architecture mechanism，不是 VM policy——它不表达"哪个进程该拥有什么内存"，只表达"如何稳定访问 physical memory"
- 但"谁使用 ≠ 谁构造"——kernel 使用 direct map，VM 构造 direct map。这比"kernel 建立 direct map"更符合微内核精神
- Kernel direct map：由 VM 在 `map_kernel()` 中建立（Minix3 的 VM 本来就负责 `map_kernel()`），但建立后视为只读不变量，VM 不再修改
- VM direct map：由 VM 在初始化时建立，仅存在于 VM 进程页表

### 0.2 双视图模型

同一物理内存有两个 VA 窗口，这不是冗余，而是特权级硬件的必然要求：

| 视图 | VA 范围 | 权限 | 存在于 | 用途 |
|------|---------|------|--------|------|
| Kernel direct map | 内核空间高半部分 | U/S=0 (supervisor), G=1, NX=1 | 所有进程页表 | Kernel 在任意进程上下文访问物理内存 |
| VM direct map | 用户空间低半部分 | U/S=1 (user), NX=1 | 仅 VM 进程页表 | VM 读写物理页数据、操作页表 |

**为什么需要两个视图**：一个 PTE 的 U/S 位不可能同时为 0 和 1。Kernel 需要 supervisor-only（所有进程可见），VM 需要 user-accessible（仅 VM 可见）。两者映射同一物理内存，只是 VA 窗口和权限不同。

### 0.3 为什么这是 Redesign 而非 Rewrite

根据 review.md 的定义，Rewrite = 外部语义不变内部表达改变，Redesign = 改变系统架构或机制。Direct Map 是 Redesign，理由如下：

**Minix3 的历史限制**：
- x86-32 地址空间仅 4GB，内核无法建立全物理内存 direct map
- 被迫使用 `createpde/freepde` 临时映射窗口（2 个 4MB 窗口轮转）
- 被迫使用 `spare_pagequeue` 备用页池（编译期固定大小，可能耗尽）
- 被迫使用 `pagedir_mappings` 登记册（内核间接跟踪进程页目录）
- 这些机制是 **32 位地址空间限制下的工程权衡**，不是微内核设计原则

**x86-64 的架构优势**：
- 128TB+ 地址空间使 direct map 成为可能
- 1GB 大页使初始映射成本极低（1 个 8 字节表项覆盖 1GB）
- 物理页天然拥有 stable VA：`va = DIRECT_MAP_BASE + pa`
- 递归问题从"需要缓解"变为"结构上不可能发生"

**Redesign 的合理性**：
- 不是理念背叛，而是 x86-64 下的工程升级
- VM 仍然负责 `map_kernel()`，direct map 是其自然扩展
- 策略/机制分离更纯粹：VM 不再需要内核协助（createpde）才能访问物理内存
- 消除了 Minix3 因 32 位限制而引入的全部复杂机制

**概念统一的收益**（这是整个 Redesign 的核心价值）：

| 修改前 | 修改后 |
|--------|--------|
| 普通页：alloc_phys + alloc_virt | 所有物理页：alloc_phys + `vm_phys_to_virt()` |
| 页表页：PtRegion bump allocator | 页表页 = 物理页的一种用途，无特殊 VA 管理 |
| 内核访问物理页：createpde 临时映射 | 内核访问物理页：`kernel_phys_to_virt()` |
| 三套机制，概念割裂 | 一套机制，概念统一 |

**范式转变**：这不是"换了一种 VA 分配方式"，而是从"mapping-centric VM"到"physical-memory-centric VM"的范式转变。旧世界观：physical page → 需要临时 VA → vm_mappages / ensure_tables / 递归。新世界观：physical page → 天然就有 stable VA → DIRECT_MAP_BASE + pa。VA 分配这个步骤本身消失了。

**页表页不是特殊对象**：Direct map 出现前，隐含的模型是"普通物理页 ≠ 页表页"，页表页需要 PtRegion 这样的特殊 VA 管理。Direct map 出现后，"所有 physical pages are equally accessible"——页表页只是物理页的一种用途。这不是"优化了页表页的 VA 分配"，而是"页表页需要特殊 VA 管理"这个概念本身消失了。

### 0.4 安全模型

VM 从"通过 createpde 临时映射访问物理内存"变为"通过 direct map 永久映射访问物理内存"。实际安全边界相同——VM 本来就是 trusted pager，控制所有进程的页表，拥有事实上的全物理内存访问能力。差别只是"显式拥有"（direct map）vs"事实上拥有"（createpde）。

**createpde 是 capability façade**：Minix3 的 createpde 没有真正限制 VM 的物理内存访问权，只是让限制看起来存在。VM 可以通过修改任何进程的页表来读写任意物理内存——它只是多了一步间接操作。Direct map 是"显式拥有"而非"新增权限"。

VM direct map 设置 NX 位，阻止代码执行，提供深度防御。

### 0.5 设计洞察（来自 cross-review / comments / copy-paste）

以下洞察来自 AI review 过程中的关键论述，具有教学价值，应融入最终文档：

#### 洞察 1：VM 是 memory manager，不是 MMU controller

VM 只是 page-table data structure 的拥有者，不是 MMU 的操作者。VM 把页表当普通数据读写（通过 direct map），kernel 才是 commit 到硬件的执行者（CR3 切换、TLB 刷新）。这是策略/机制分离的最深层体现——VM 决定地址空间长什么样，kernel 负责把图纸变成硬件现实。

→ 融入 §0.1 和 TODO-07

#### 洞察 2：页表页不是特殊对象——概念统一的核心

Direct map 出现前，隐含的模型是"普通物理页 ≠ 页表页"，页表页需要 PtRegion 这样的特殊 VA 管理。Direct map 出现后，"所有 physical pages are equally accessible"——页表页只是物理页的一种用途，不需要特殊 VA 管理。这不是"优化了页表页的 VA 分配"，而是"页表页需要特殊 VA 管理"这个概念本身消失了。

→ 融入 §0.3 概念统一收益表和 TODO-05

#### 洞察 3：从"mapping-centric VM"到"physical-memory-centric VM"的范式转变

旧世界观：physical page → 需要临时 VA → vm_mappages / ensure_tables / 递归
新世界观：physical page → 天然就有 stable VA → DIRECT_MAP_BASE + pa

这不是"换了一种 VA 分配方式"，而是"VA 分配这个步骤本身消失了"。整个 VM 代码会开始疯狂简化——因为所有对物理页的操作都统一为 `vm_phys_to_virt()`。

→ 融入 §0.3 和 §0.5 叙事线索

#### 洞察 4：Direct map 是 mechanism，但由 VM 构造

Direct map 是 CPU/MMU architecture mechanism，不是 VM policy——它不表达"哪个进程该拥有什么内存"，只表达"如何稳定访问 physical memory"。但"谁使用 ≠ 谁构造"——kernel 使用 direct map，VM 构造 direct map。这比"kernel 建立 direct map"更符合微内核精神：VM 是 address-space architect，kernel 只是消费者。

→ 融入 §0.1

#### 洞察 5：createpde 是 capability façade——安全边界的精准论证

VM 本来就控制所有进程的页表，拥有事实上的全物理内存访问能力。Minix3 的 createpde 只是一个 capability façade——它没有真正限制 VM 的物理内存访问权，只是让限制看起来存在。Direct map 是"显式拥有"而非"新增权限"。安全边界没有变化，只是从 createpde 的"事实上拥有"变为 direct map 的"显式拥有"。

→ 融入 §0.4

#### 洞察 6：物理内存 > 512GB 的完备性论证

一个 PDPT 页能容纳 512 个 1GB 条目。超过 512GB 需要新 PDPT 页，但此时 VM 已有至少 1GB direct map，可以 `alloc_phys() → vm_phys_to_virt()` 直接操作新 PDPT 页，仍然零递归、零额外自举风险。完备性无懈可击。

→ 融入 TODO-04-2 和 TODO-09-2

#### 洞察 7：1GB 大页 CPU 支持检查 + 2MB 回退路径

并非所有 x86-64 CPU 支持 1GB 大页（需 CPUID.80000001H:EDX.GBPAGES 检查）。不支持时回退到 2MB 大页（每 1GB 段需 1 个 PD 页 = 512 个 2MB 条目）。回退逻辑应在 DirectMapArch 抽象层完成，对上层 `vm_phys_to_virt()` 透明。自举逻辑几乎不变——只是初始页表多一个 PD 页。

→ 融入 TODO-06-2

#### 洞察 8：Rust 所有权与物理内存的 Aliasing 风险

如果一个物理页同时被映射到用户空间（进程页表）和 VM 的 Direct Map，Rust 的 `&mut` 要求独占访问会违反内存模型。建议：(1) 使用 `read_volatile`/`write_volatile` 明确告知编译器该内存可能在外部被修改；(2) 或定义 `PhysPtr<T>` 包装器，通过物理地址在需要时提供访问。这是 Rust OS 开发中的经典问题——物理内存的 aliasing 与 Rust 所有权模型的冲突。

→ 融入 TODO-05 或新增跨文档设计考量

#### 洞察 9：初始页表物理页的冲突避免

初始页表（4 页）放在物理内存前几页，需要确保这些页不会与 reserved_region 冲突，也不会被 bitmap 误分配。由 kernel 从预留区域中划出，并在 boot_info 中标记为 `used`。

→ 融入 TODO-06-1

#### 洞察 10：Kernel direct map Global 位的未来约束

Kernel direct map 设 Global 位后，CR3 切换不刷新这部分 TLB。当前设计 kernel direct map 是"建立后只读不变量"，所以安全。但如果未来需要动态修改 kernel direct map（如内存热插拔），需要设计显式的 TLB 刷新协议（跨核 shootdown）。读者应理解：这个约束不是限制，而是简化——不变的东西不需要管理。

→ 融入 TODO-07-4

### 0.6 叙事线索

02-stage-vm 文档系列应有一条叙事线索，让读者从问题出发，逐步理解设计决策：

```
04 → 05 → 07 是核心路径

04: "VM 如何获得物理内存访问能力？"
    answer: 1GB direct map + bitmap → 扩展到覆盖全部物理内存
    读者应理解：Minix3 的 spare_pagequeue/EarlyHeap 是 32 位限制下的权衡

05: "VM 如何分配页？"
    answer: alloc_phys() + vm_phys_to_virt()，不再需要 alloc_virt()
    读者应理解：递归的根源不是"页表页需要分配"，而是"页表页需要独立 VA 分配路径"
    读者应看到设计演进：BSS → 预留区域+标志 → Typestate+PtRegion → Direct Map

07: "VM 如何操作页表？"
    answer: 通过 direct map 直接写，不再需要 createpde/pagedir_mappings
    读者应理解：双视图模型是特权级硬件的必然要求，不是冗余

15: "CoW 如何工作？"
    answer: vm_phys_to_virt() + copy_nonoverlapping()，不再需要 sys_abscopy
    读者应理解：direct map 使跨进程内存操作从"内核系统调用"降级为"普通 memcpy"
```

每一步都是前一步的自然延伸，读者应感受到设计的连贯性。

---

## 变动总览

Direct Map 设计引入后，文档 01-19 需要的修改分为以下几类：

| 变动类别 | 影响范围 | 核心变化 |
|---------|---------|---------|
| PtRegion 演进为 Direct Map | 05, 07, 09, ptregion_design.md | PtRegion 作为设计插曲保留，最终方案为 `vm_phys_to_virt()` |
| EarlyHeap 删除 | 04, 05 | 所有 EarlyHeap 引用替换为 1GB direct map + bitmap |
| ReservedRegion 简化 | 05, 09 | VA 分配功能删除，仅保留物理页预留语义 |
| `alloc_virt()` 删除 | 05 | `alloc_virt()` → `vm_phys_to_virt()`，不再需要独立 VA 分配 |
| `phys_to_virt()` 双视图 | 06, 07, 08, 10, 14, 16 | 区分 `vm_phys_to_virt()` 与 `kernel_phys_to_virt()` |
| `sys_abscopy` → direct map memcpy | 10, 11, 15, 16 | CoW 页复制不再需要内核系统调用 |
| `pagedir_mappings` 删除 | 07 | 内核通过 direct map 直接访问进程页目录 |
| `createpde/freepde` 删除 | 07 | 不再需要临时映射窗口 |
| `spare_pagequeue` 删除 | 04, 05 | direct map 消除递归根源 |
| 3 阶段启动 | 04, 05 | bitmap → expand → optional migrate |
| VM 初始页表结构 | 06 | 4 页：PML4 + PDPT_A + PD_A + PDPT_B |
| Kernel direct map 只读不变量 | 07 | `map_kernel()` 后不再修改 kernel direct map PTE |
| DirectMapArch trait | 06 | 新增跨架构抽象 |

---

## 04-physical-memory.md — 重度修改

> **文档应传达的设计思考**：04 是读者接触 VM 内存管理的起点。读者应理解：
> - Minix3 的 `spare_pagequeue`/`EarlyHeap` 是 x86-32 地址空间限制下的工程权衡，不是微内核原则
> - x86-64 的 direct map 使 VM 启动即可访问物理内存，彻底消除了"先有鸡还是先有蛋"的自举困境
> - 3 阶段启动不是复杂的分阶段设计，而是"1GB 足以启动世界 → 扩展到覆盖全部 → 可选迁移"的自然递进

### ✅ TODO-04-1: EarlyHeap 整节替换 [已完成]
**位置**: §4.2 EarlyHeap 实现（~L1087-1141）
**现状**: 描述 `EarlyHeap` bump allocator，从 BSS 段分配
**修改**: 整节替换为 "Direct Map + Bitmap 启动"。描述：
- Kernel 传递 1GB direct map（PDPT_B[0] = 1GB huge page）
- VM 启动即可通过 `vm_phys_to_virt()` 访问前 1GB 物理内存
- Bitmap 元数据从前 1GB 可用物理页分配
- EarlyHeap 不再需要，删除 `early_heap.rs`
**设计思考**：Minix3 用 BSS 段预留静态备用页，是因为 32 位下无法建立全物理内存映射。64 位下 1GB direct map 天然解决了"VM 启动时如何访问物理内存"的问题——不再需要从 BSS 段"偷"内存。读者应感受到：这不是"换了一种 early allocator"，而是"early allocator 这个概念本身不再需要"。
**参考**: ptregion_design.md §8.1.5, §8.5 Phase 1

### ✅ TODO-04-2: 物理内存初始化流程重写 [已完成]
**位置**: §4.4 物理分配器初始化流程（~L1046-1050 的阶段描述）
**现状**: 阶段 3-5 描述切出 early_heap → 初始化 early_heap → 初始化物理分配器
**修改**: 替换为 3 阶段启动：
- Phase 1: Bootstrap — 1GB direct map → bitmap allocator
- Phase 2: Direct Map 扩展 — bitmap 分配页表页 → 扩展覆盖全部物理内存
- Phase 3: 分配器迁移 — bitmap → buddy（策略决定）
**设计思考**：3 阶段启动的核心洞察是"1GB 足以启动世界"——bitmap 元数据永远能放进 1GB（即使 4TB 物理内存也只需 ~128MB）。读者应理解：Phase 1 不是"临时方案"，而是"已经够用"；Phase 2 不是"补救"，而是"扩展"；Phase 3 不是"必须"，而是"策略选择"。

**完备性论证**：一个 PDPT 页能容纳 512 个 1GB 条目，覆盖 512GB 物理内存。超过 512GB 需要新 PDPT 页，但此时 VM 已有至少 1GB direct map，可以 `alloc_phys() → vm_phys_to_virt()` 直接操作新 PDPT 页，仍然零递归、零额外自举风险。完备性无懈可击。

**参考**: ptregion_design.md §8.5

### ✅ TODO-04-3: Bitmap/FreeList/Buddy init 签名变更 [已完成]
**位置**: §4.5/4.6/4.7 中 `init(early_heap: &mut EarlyHeap, ...)` 签名
**现状**: 所有 init 函数接受 `&mut EarlyHeap` 参数
**修改**: 替换为 `init(phys_alloc: &mut BitmapAllocator, ...)` 或直接通过 `vm_phys_to_virt()` 访问
**参考**: ptregion_design.md §8.5 Phase 1

### ✅ TODO-04-4: spare_pagequeue 引用更新 [已完成]
**位置**: §2 中 Minix3 spare_pagequeue 相关描述（~L814-831）
**现状**: 描述 Minix3 的 `spare_pagequeue` 机制
**修改**: 保留 Minix3 历史描述（作为背景），在末尾增加分析段落：Minix3 的 spare_pagequeue 是 x86-32 地址空间限制下的必然产物——32 位内核无法建立全物理内存 direct map，只能预分配备用页池来应对递归。x86-64 下 direct map 从结构上消除了递归根源，此机制不再需要。
**设计思考**：读者应理解 spare_pagequeue 不是"设计失误"，而是"32 位时代的正确工程权衡"。保留历史描述有助于读者理解为什么 direct map 是结构性简化而非局部优化。
**参考**: ptregion_design.md §8.9

### ✅ TODO-04-5: 文件结构树更新 [已完成]
**位置**: 末尾的文件结构树（~L1748）
**现状**: 包含 `early_heap.rs`
**修改**: 删除 `early_heap.rs`，新增 `direct_map.rs`
**参考**: ptregion_design.md §8.10

---

## 05-vm-allocpage.md — 重度修改

> **文档应传达的设计思考**：05 是设计演进的核心舞台。读者应看到：
> - 递归的根源不是"页表页需要分配"，而是"页表页需要独立 VA 分配路径"
> - 设计演进：BSS 静态分配 → 预留区域+运行时标志 → Typestate+PtRegion → Direct Map，每一步解决了什么问题，又暴露了什么新问题
> - PtRegion 是 direct map 出现前的正确局部解，其设计推导帮助我们发现真正的问题——不是"如何避免递归"，而是"如何让物理页天然拥有 stable VA"
> - 最终方案不是"删除 PtRegion"，而是"PtRegion 被超越"——概念从"页表页需要特殊 VA 管理"进化为"所有物理页统一通过 vm_phys_to_virt() 访问"

### ✅ TODO-05-1: PtRegion 演进为 Direct Map（保留设计演进叙述） [已完成]
**位置**: §3.1-3.5 方案演进（~L364-594）、§4.1 VmPageAllocator 结构（~L412-449）、§4.3 ReservedRegion（~L642-724）、§5 测试（~L865-900）
**现状**: §3.1-3.3 描述方案一（BSS）、方案二（预留区域+标志）、方案三（Typestate+PtRegion），§3.5 描述 alloc_phys/alloc_virt 拆分
**修改**:
- §3.1-3.3 保留不动——这三段设计演进本身有教学价值，展示了从 Minix3 原方案逐步优化的思考过程
- §3.4（PtRegion 递归消除）保留，但在末尾增加"设计反思"段落：PtRegion 结构性消除了递归，但它本质上是在 VM 里"重新发明了一套 mini direct-map"——给物理页分配 stable VA。当真正的 direct map 出现后，PtRegion 的核心职责被 `vm_phys_to_virt()` 天然替代
- §3.5 之后新增 §3.6 "方案四：Direct Map"，作为演进的终点：
  - `alloc_virt()` 不再需要，替换为 `vm_phys_to_virt(phys)`
  - `alloc_page()` 简化为 `alloc_phys() → vm_phys_to_virt(phys)`
  - 递归从"需要缓解"变为"结构上不可能发生"
  - PtRegion 的 bump allocator、PDPT/PD/PT 层级管理全部由 direct map 替代
- §4.1 VmPageAllocator 结构体：增加"方案四实现"小节，展示简化后的结构（删除 `pt_region`、`pt_ops`、`into_normal()`）
- §4.3 ReservedRegion：标注 `alloc_contig_virt()` 在 direct map 方案下不再需要
- §5 测试：增加方案四的测试场景（alloc_phys + vm_phys_to_virt 的组合）
**设计思考**：读者应感受到设计不是一步到位的，而是通过不断追问"这个问题的本质是什么"逐步逼近的。PtRegion 的价值不在于它被保留，而在于它帮助我们发现——真正的问题不是"如何避免递归"，而是"如何让物理页天然拥有 stable VA"。保留演进叙述，让读者自己走一遍这个思考过程。

**范式转变**：从"mapping-centric VM"到"physical-memory-centric VM"。旧世界观中，物理页需要临时 VA（vm_mappages / ensure_tables / 递归）；新世界观中，物理页天然就有 stable VA（DIRECT_MAP_BASE + pa）。这不是"换了一种 VA 分配方式"，而是"VA 分配这个步骤本身消失了"。

**页表页不是特殊对象**：Direct map 出现前，隐含的模型是"普通物理页 ≠ 页表页"。Direct map 出现后，"所有 physical pages are equally accessible"——页表页只是物理页的一种用途。这不是"优化了页表页的 VA 分配"，而是"页表页需要特殊 VA 管理"这个概念本身消失了。

**Rust 所有权与 Aliasing**：当一个物理页同时被映射到用户空间（进程页表）和 VM 的 Direct Map 时，Rust 的 `&mut` 要求独占访问会违反内存模型。建议使用 `read_volatile`/`write_volatile` 或定义 `PhysPtr<T>` 包装器。这是 Rust OS 开发中的经典问题。

**参考**: ptregion_design.md §8.1.4, §8.6, §8.7

### ✅ TODO-05-2: ReservedRegion 简化 [已完成]
**位置**: §4.2 ReservedRegion（~L642-724）
**现状**: ReservedRegion 包含 VA 分配功能（`alloc_contig_virt`），为 PtRegion 提供 VA
**修改**: ReservedRegion 的 VA 分配功能在方案四中删除（direct map 替代），仅保留物理页预留语义。`alloc_contig_virt()` 标注为"方案三专用，方案四中删除"。ReservedRegion 在方案四中变为纯粹的"已知物理页列表"
**设计思考**：ReservedRegion 的 VA 分配功能是为 PtRegion 服务的——PtRegion 需要连续 VA 来建立 PDPT/PD/PT 链。Direct map 出现后，物理页已有 stable VA，不再需要从预留区域切 VA。读者应理解：ReservedRegion 的简化不是"删减功能"，而是"不再需要这个功能"。
**参考**: ptregion_design.md §8.10 删除清单

### ✅ TODO-05-3: VmPageAllocator 结构体重写 [已完成]
**位置**: §4.1 VmPageAllocator 定义（~L364-449）
**现状**: Bootstrap/Normal 两阶段，Bootstrap 含 `phys_alloc` + `pt_ops`，Normal 含 `pt_region`
**修改**: 在方案四中简化为单阶段（direct map 从启动即可用）：
- 删除 `phys_alloc: Option<...>` 的 Option 包裹
- 删除 `pt_ops: Option<RealPtOps>`
- 删除 `pt_region: Option<PtRegion<O>>`
- 删除 `into_normal()` 转换函数
- 保留 `reserved: ReservedRegion`（但简化为纯物理页列表）
**设计思考**：Typestate 模式（Bootstrap → Normal）是为了在编译期保证"Normal 阶段不可能访问 reserved"。Direct map 出现后，不再有阶段区分——VM 从第一条指令起就能通过 `vm_phys_to_virt()` 访问物理内存。读者应理解：Typestate 是解决"自举阶段区分"的优雅方案，但 direct map 使"自举阶段"这个概念本身消失了。
**参考**: ptregion_design.md §8.5 Phase 1

### ✅ TODO-05-4: spare_pagequeue 引用更新 [已完成]
**位置**: §2 Minix3 allocpage（~L163-171, L246, L311, L517）
**现状**: 描述 Minix3 的 `spare_pagequeue` 和 `STATIC_SPARE_PAGES`
**修改**: 保留 Minix3 历史描述，在末尾增加分析段落（同 TODO-04-4 的处理方式）：spare_pagequeue 是 x86-32 地址空间限制下的必然产物，x86-64 下 direct map 从结构上消除递归根源
**参考**: ptregion_design.md §8.9

### ✅ TODO-05-5: 递归问题分析更新 [已完成]
**位置**: §3.3 递归问题（~L836）、§5.4 测试（~L865-900）
**现状**: 描述 alloc_virt 导致的递归问题及 PtRegion 的解决方案
**修改**: §3.3 保留 PtRegion 的递归消除分析（作为方案三的描述），在 §3.6 方案四中增加对比：
- 方案三（PtRegion）：递归从结构上不可能发生，因为页表页 VA 不走 find_hole + vm_mappages
- 方案四（Direct Map）：递归从更根本的层面不可能发生，因为物理页天然拥有 stable VA，不存在"需要分配 VA"这个步骤
- 对比表：Minix3 spare_pagequeue（缓解递归）→ PtRegion（结构性消除递归）→ Direct Map（递归根源消失）
**设计思考**：读者应理解三种方案的递归处理是递进关系——spare_pagequeue 是"递归发生时兜底"，PtRegion 是"让递归不可能发生"，Direct Map 是"让递归的前提条件消失"。这是从"治标"到"治本"到"不需要药"的跃迁。
**参考**: ptregion_design.md §8.1.4 理由 4

---

## 06-pagetable-struct.md — 中度修改

> **文档应传达的设计思考**：06 是页表结构的定义文档。读者应理解：
> - VM 初始页表不是"复杂的多页结构"，而是"4 页 + 1 个大页表项"的极简设计
> - DirectMapArch trait 将三种架构差异压缩为常量，体现了"抽象的品味"——不同硬件的页表语义被归一化为"在第 2 级页表入口写一个大页表项"

### ✅ TODO-06-1: VM 初始页表结构新增 [已完成]
**位置**: 需要新增一节或在现有初始化描述中添加
**现状**: 未描述 VM 初始页表的具体结构
**修改**: 新增 "VM 初始页表" 描述：
- 4 页结构：PML4 + PDPT_A + PD_A + PDPT_B
- PDPT_B[0] = 1GB direct map（1 个 8 字节表项 = 1GB 映射）
- 三种架构等价结构表
**设计思考**：读者应感受到初始页表的极简性——Kernel 只需 4 页物理内存 + 1 个大页表项，VM 就能启动并访问前 1GB 物理内存。这不是"精心设计的最小集"，而是"硬件大页机制的自然结果"。

**冲突避免**：初始页表（4 页）放在物理内存前几页，需要确保这些页不会与 reserved_region 冲突，也不会被 bitmap 误分配。由 kernel 从预留区域中划出，并在 boot_info 中标记为 `used`。

**参考**: ptregion_design.md §8.4

### ✅ TODO-06-2: DirectMapArch trait 新增 [已完成]
**位置**: 需要新增一节或在架构相关描述中添加
**现状**: 未描述跨架构 direct map 抽象
**修改**: 新增 DirectMapArch trait 定义及三种架构实现
**设计思考**：三种架构（x86-64、arm64、riscv64 Sv39）的 direct map 语义被归一化为"在第 2 级页表入口写一个大页表项"。读者应理解：跨架构适配不是"为每个架构写一套代码"，而是"找到语义上的最大公约数"。

**1GB 大页 CPU 支持检查**：并非所有 x86-64 CPU 支持 1GB 大页（需 CPUID.80000001H:EDX.GBPAGES 检查）。不支持时回退到 2MB 大页（每 1GB 段需 1 个 PD 页 = 512 个 2MB 条目）。回退逻辑应在 DirectMapArch 抽象层完成，对上层 `vm_phys_to_virt()` 透明。自举逻辑几乎不变——只是初始页表多一个 PD 页。可以在 `DirectMapArch` 中增加 `supports_1gb_page()` 功能检查。

**参考**: ptregion_design.md §8.3

### ✅ TODO-06-3: 地址空间布局更新 [已完成]
**位置**: 如有地址空间布局描述
**现状**: 可能使用旧布局（无 VM direct map）
**修改**: 更新为 §8.2 的双视图布局（VM direct map + Kernel direct map）
**设计思考**：地址空间布局图应让读者直观看到：VM direct map 在用户空间低位，Kernel direct map 在内核空间高位，两者映射同一物理内存。这不是"两份映射"，而是"同一物理内存在不同特权级下的两个必要窗口"。
**参考**: ptregion_design.md §8.2

---

## 07-pagetable-ops.md — 重度修改

> **文档应传达的设计思考**：07 是页表操作的核心文档。读者应理解：
> - 双视图模型是特权级硬件的必然要求，不是冗余——一个 PTE 的 U/S 位不可能同时为 0 和 1
> - Minix3 的 createpde/freepde/pagedir_mappings 是 x86-32 地址空间限制下的工程权衡，不是微内核原则
> - Kernel direct map 建立后是只读不变量——VM 不再修改它，这意味着 Global 位的 TLB 条目不需要额外刷新策略

### ✅ TODO-07-1: §3.0 架构决策更新 [已完成]
**位置**: §3.0 架构决策：取消 pagedir_mappings，采用直接映射区（~L778-811）
**现状**: 已描述 kernel direct map 方案，但未涉及 VM direct map 和双视图模型
**修改**: 更新为双视图模型：
- Kernel direct map: U/S=0, 存在于所有进程页表
- VM direct map: U/S=1, 仅存在于 VM 进程页表
- 两者映射同一物理内存，只是 VA 窗口和权限不同
- `phys_to_virt()` 拆分为 `vm_phys_to_virt()` 和 `kernel_phys_to_virt()`
**设计思考**：双视图模型的核心论证——VM 是 trusted pager，原本就控制所有进程的页表，拥有事实上的全物理内存访问能力。U/S=1 的 direct map 是"显式拥有"而非"新增权限"。安全边界没有变化，只是从 createpde 的"事实上拥有"变为 direct map 的"显式拥有"。
**参考**: ptregion_design.md §8.1.3

### ✅ TODO-07-2: pagedir_mappings 历史描述标注 [已完成]
**位置**: §2.1 中 pagedir_mappings 的详细描述（~L237-340）
**现状**: 详细描述了 pagedir_mappings 机制
**修改**: 保留作为 Minix3 历史参考，在末尾增加分析段落：pagedir_mappings 是 x86-32 内核无法直接映射所有物理内存时的间接访问机制。x86-64 下 kernel 通过 direct map 直接访问进程页目录，此机制不再需要。保留历史描述有助于读者理解 direct map 的简化效果。
**参考**: ptregion_design.md §8.9

### ✅ TODO-07-3: pt_bind 语义更新 [已完成]
**位置**: §2.1.2 pt_bind（~L311-388）、§4 Rust 实现（~L1065）
**现状**: pt_bind 包含 pagedir_mappings 登记步骤
**修改**: 明确 pt_bind 不再需要 pagedir_mappings 登记步骤，仅保留 `sys_vmctl_set_addrspace()` 通知内核。已有 §4 的说明（L1065），可进一步强化
**参考**: ptregion_design.md §8.9

### ✅ TODO-07-4: pt_mapkernel 更新 [已完成]
**位置**: §2.1.1 pt_mapkernel 描述
**现状**: 描述映射内核代码段 + pagedir_mappings
**修改**: 更新为映射内核代码段 + kernel direct map（1GB huge pages, U/S=0, G=1）。强调 kernel direct map 只读不变量——`map_kernel()` 建立后 VM 不再修改 kernel direct map 的 PTE/PDE/PDPT 表项
**设计思考**：Kernel direct map 只读不变量是一个重要的架构约束。它意味着：(1) Global 位的 TLB 条目不需要额外刷新策略（CR3 切换不刷新，且内容不变）；(2) 如果未来需要动态修改 kernel direct map（如内存热插拔），需要设计显式的 TLB 刷新协议。读者应理解：这个约束不是限制，而是简化——不变的东西不需要管理。
**参考**: ptregion_design.md §8.8.1

### ✅ TODO-07-5: createpde/freepde 历史描述标注 [已完成]
**位置**: §2 中 createpde/freepde 相关描述
**现状**: 描述 Minix3 的临时映射窗口机制
**修改**: 保留作为 Minix3 历史参考，在末尾增加分析段落：createpde/freepde 是 x86-32 内核只有 2 个空闲 PDE 时的临时映射窗口机制。内核通过轮转 2 个 4MB 窗口来访问非当前进程的物理内存。x86-64 下 kernel direct map 使此机制完全不再需要。保留历史描述有助于读者理解 Minix3 内核地址空间的极端限制。
**参考**: ptregion_design.md §8.9

### ✅ TODO-07-6: sys_datacopy 简化 [已完成]
**位置**: §3.0 中跨进程内存拷贝描述（~L790-799）
**现状**: 描述 freepdes 临时映射 → 拷贝 → 清除
**修改**: 更新为 VM 可直接通过 direct map 完成跨进程复制，无需 kernel 切换 PDE。`sys_datacopy` 简化
**设计思考**：这是 direct map 带来的最直观的简化——跨进程内存复制从"内核系统调用 + 临时映射窗口"降级为"普通 memcpy"。读者应理解：这不是"优化了 sys_datacopy"，而是"sys_datacopy 这个概念不再需要"——VM 已经能直接看到所有物理内存。
**参考**: ptregion_design.md §8.11

---

## 08-slab-allocator.md — 中度修改

> **文档应传达的设计思考**：08 是 VM 从"裸指针世界"进入"Rust alloc 世界"的转折点。读者应理解：
> - 08 的核心不是"实现 slab 分配器"，而是"接入 GlobalAlloc，使 Box<T>/Vec<T> 可用"
> - Minix3 的专用 slab 是 C 语言和 32 位限制下的工程补丁——C 没有 RAII，32 位地址空间需要精打细算
> - Rust alloc 体系已内置 slab 机制（size class + thread cache），不需要自研
> - GlobalAlloc 底层对接 vm_phys_to_virt()——VM 的堆分配全程在用户态完成，不经过内核
> - 08 是 09 的前提：搬迁需要 Box<T>，分配器迁移需要 Box<dyn PhysAllocator>

### ✅ TODO-08-1: Slab 对象访问方式更新 [已完成]
**位置**: 如有通过 PtRegion/alloc_virt 访问 slab 对象的描述
**现状**: 可能通过旧机制访问 slab 元数据
**修改**: 更新为通过 `vm_phys_to_virt()` 访问 slab 对象。Slab 元数据在 direct map 中可直接访问
**设计思考**：Slab 分配器的元数据（free list 链表等）存储在物理页内部。Direct map 出现后，这些元数据的访问从"需要先映射到 VA"变为"物理页天然有 VA"。这是 `vm_phys_to_virt()` 统一性的一个具体例证。
**参考**: ptregion_design.md §8.6

### ✅ TODO-08-2: GlobalAlloc 实现更新 [已完成]
**位置**: §4.1 全局分配器接入
**现状**: `__vm_global_alloc` 当前对接 C 的 malloc/free
**修改**: 更新为对接 vm_phys_to_virt() + PhysAllocator。分配路径：GlobalAlloc → alloc_phys() → vm_phys_to_virt() → 返回 VA。全程用户态，不经过内核
**设计思考**：VM 是内存管理服务器，它的堆分配不应该依赖外部——它自己就是内存的来源。GlobalAlloc 对接 vm_phys_to_virt() 使 VM 的分配链路完全自包含：请求内存 → 从自己的物理池分配 → 通过自己的 direct map 访问。读者应理解：这不是"优化了分配路径"，而是"VM 终于成为了自己内存的主人"。
**参考**: ptregion_design.md §8.6

---

## 09-vm-relocation.md — 重度修改

> **文档应传达的设计思考**：09 是 VM 自举的终点——从此 VM 完全建模所有物理内存。读者应理解：
> - 09 之前：VM 仅有 kernel 传过来的初始 1GB direct map，bitmap 元数据在启动区
> - 09 之后：direct map 覆盖全部物理内存，分配器元数据在堆上，VM 完全自主
> - 搬迁在 direct map 下被大幅简化——PtRegion 的搬迁逻辑（分配 VA + 建立映射 + 复制 + 更新指针）简化为（alloc_phys + vm_phys_to_virt + memcpy + 更新指针）
> - Phase 2（direct map 扩展）是 09 的核心新内容——从 1GB 扩展到覆盖全部物理内存
> - Phase 3（bitmap → buddy）是可选的策略选择，不是必须的

### ✅ TODO-09-1: ReservedRegion::from_boot_info 更新 [已完成]
**位置**: ~L1258 `ReservedRegion::from_boot_info(&boot_info)`
**现状**: ReservedRegion 包含 VA 分配功能
**修改**: 更新 ReservedRegion 为纯物理页预留列表，删除 VA 分配相关逻辑
**参考**: ptregion_design.md §8.10 删除清单

### ✅ TODO-09-2: PtRegion 搬迁逻辑替换 [已完成]
**位置**: 09-todo.md L52-59 中 PtRegion 搬迁逻辑
**现状**: 描述 `relocate_phys_allocator()` 委托给 pt_region
**修改**: 删除 PtRegion 搬迁逻辑，替换为 direct map 扩展逻辑（Phase 2：bitmap 分配页表页 → 扩展 direct map 覆盖全部物理内存）
**设计思考**：PtRegion 的搬迁逻辑是为了在 VM 重定位后重新建立页表页的 VA 映射。Direct map 下不存在这个问题——物理页的 VA 只取决于其物理地址，与 VM 自身的虚拟地址无关。读者应理解：重定位简化不是"优化了搬迁流程"，而是"搬迁这个概念本身被消除了"。

**完备性论证**：Phase 2 扩展 direct map 时，如果物理内存 > 512GB，需要新 PDPT 页。但此时 VM 已有至少 1GB direct map，可以 `alloc_phys() → vm_phys_to_virt()` 直接操作新 PDPT 页，仍然零递归。

**参考**: ptregion_design.md §8.5 Phase 2

### ✅ TODO-09-3: 里程碑定位新增 [已完成]
**位置**: §1 基本概念
**现状**: 09 描述"初始化数据搬迁"，定位为 Bootstrap → Normal 的过渡步骤
**修改**: 更新 09 的定位为"VM 自举的终点"。在 §1 开头增加里程碑描述：
- 09 之前：VM 依赖 kernel 传过来的初始 1GB direct map
- 09 之后：VM 完全建模所有物理内存，alloc crate 完整可用
- 09 的三个阶段对应 VM 自举的三个跃迁：Phase 1（1GB 足以启动世界）→ Phase 2（扩展到覆盖全部物理内存）→ Phase 3（可选的分配器策略升级）
**设计思考**：09 不是一个"收尾文档"，而是 VM 自举叙事的高潮。读者应感受到：从 04 到 09，VM 经历了从"内核馈赠 1GB"到"完全自主管理所有物理内存"的完整自举过程。09 是这个过程的终点——从此 VM 不再需要内核的特殊帮助。
**参考**: ptregion_design.md §8.5

---

## 10-phys-block.md — 中度修改

> **文档应传达的设计思考**：10 是物理页块的管理文档。读者应理解：
> - `sys_abscopy` → `vm_phys_to_virt() + copy_nonoverlapping()` 不是一个简单的 API 替换，而是权限模型的根本变化——VM 从"请求内核代劳"变为"自己直接操作"

### ✅ TODO-10-1: sys_abscopy → direct map memcpy [已完成]
**位置**: ~L278 `sys_abscopy(old_page, new_page, VM_PAGE_SIZE)`
**现状**: 使用内核系统调用 `sys_abscopy` 复制物理页
**修改**: 替换为 `vm_phys_to_virt()` + `copy_nonoverlapping()`：
```rust
core::ptr::copy_nonoverlapping(
    vm_phys_to_virt(old_phys),
    vm_phys_to_virt(new_phys),
    4096,
);
```
**设计思考**：`sys_abscopy` 的存在意味着 VM 不信任自己能直接操作物理内存——需要内核作为中介。Direct map 使 VM 成为物理内存的直接操作者，不再需要"委托内核复制"。读者应理解：这是 VM 从"受信任的请求者"到"物理内存的主人"的角色转变。
**参考**: ptregion_design.md §8.7 CoW 复制

### ✅ TODO-10-2: PhysBlock 物理页访问方式 [已完成]
**位置**: PhysBlock 中通过旧机制访问物理页内容的描述
**现状**: 可能通过 PtRegion 或 createpde 访问
**修改**: 统一通过 `vm_phys_to_virt(phys)` 访问
**参考**: ptregion_design.md §8.6

---

## 11-memtype.md — 中度修改

> **文档应传达的设计思考**：11 是内存类型管理的文档。读者应理解：
> - `alloc_virtual_space` 和 `vm_phys_to_virt` 是两个不同层次的概念——前者是进程地址空间分配（mmap 语义），后者是 VM 访问物理页的机制

### ✅ TODO-11-1: sys_abscopy → direct map memcpy [已完成]
**位置**: ~L1720-1721, ~L1750 中 `sys_abscopy` 引用
**现状**: `on_pagefault` 中 CoW 操作使用 `sys_abscopy`
**修改**: 替换为 `vm_phys_to_virt()` + `copy_nonoverlapping()`
**参考**: ptregion_design.md §8.7 CoW 复制, §8.10 修改清单

### ✅ TODO-11-2: alloc_virtual_space 引用确认 [已完成]
**位置**: ~L4523 `vmp.alloc_virtual_space(length)`
**现状**: 使用 `alloc_virtual_space` 分配虚拟地址空间
**修改**: 确认此函数的语义 — 如果是分配进程虚拟地址空间（mmap 等），则不受影响；如果是分配 VA 来访问物理页，则替换为 `vm_phys_to_virt()`
**设计思考**：`alloc_virtual_space` 在 Minix3 中可能承担双重职责——既为进程分配 VA（mmap），又为 VM 自身分配 VA 来访问物理页。Direct map 消除了后者，但前者仍然需要。读者应区分"进程地址空间管理"和"VM 物理页访问"这两个不同层次。
**参考**: ptregion_design.md §8.6

---

## 12-vir-region.md — 轻度修改

### ✅ TODO-12-1: VrParam::Direct 语义确认 [已完成]
**位置**: VrParam::Direct 相关描述
**现状**: `VrParam::Direct` 使用 `PhysBytes` 标记直接映射
**修改**: 确认 VrParam::Direct 在 direct map 下的语义 — 物理页已有 stable VA，Direct 映射的建立更简单
**参考**: ptregion_design.md §8.7

---

## 13-region-avl.md — 无修改

AVL 树实现与 direct map 无关，不需要修改。

---

## 14-phys-region.md — 中度修改

> **文档应传达的设计思考**：14 是物理区域管理的文档。读者应理解：
> - NonNull 指向的物理页内容访问方式是 direct map 统一性的微观体现——每个 `unsafe { (*ptr).field }` 背后的指针都来自 `vm_phys_to_virt()`

### ✅ TODO-14-1: phys_to_virt() 访问方式更新 [已完成]
**位置**: PhysRegion 中访问物理页内容的描述
**现状**: 可能通过旧机制（PtRegion/createpde）访问物理页
**修改**: 统一通过 `vm_phys_to_virt(phys)` 访问。`link_to_block`/`unlink_from_block` 不受影响（操作链表指针，不访问页内容）
**参考**: ptregion_design.md §8.6, §8.7

### ✅ TODO-14-2: NonNull deref 模式更新 [已完成]
**位置**: NonNull<T> 相关描述
**现状**: NonNull 指向的物理页内容可能通过旧方式访问
**修改**: NonNull 的 deref 应通过 `vm_phys_to_virt()` 获取的指针。确保所有 `unsafe { (*ptr).field }` 模式使用 direct map 地址
**设计思考**：NonNull<T> 是 Rust 中表达"非空裸指针"的类型。在 direct map 下，NonNull 的来源从"alloc_virt() 返回的 VA"变为"vm_phys_to_virt(alloc_phys()) 返回的 VA"。语义不变（都是非空裸指针），但来源统一了——所有物理页的 NonNull 都经过同一条路径。
**参考**: ptregion_design.md §8.6

---

## 15-cow-mechanism.md — 重度修改

> **文档应传达的设计思考**：15 是 CoW 机制的核心文档，也是 direct map 带来最大简化的地方之一。读者应理解：
> - CoW 的核心操作是"复制物理页"，direct map 使这从"系统调用"降级为"内存拷贝"
> - `sys_abscopy` 的消失不是 API 替换，而是 VM 角色的根本变化——从"请求内核代劳"到"自己直接操作"

### ✅ TODO-15-1: mem_cow() 核心实现重写 [已完成]
**位置**: §2 mem_cow 核心流程（~L43, L205-206, L232, L250）
**现状**: `sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE)` 复制页面
**修改**: 替换为 direct map memcpy：
```rust
fn mem_cow(pr: &mut PhysRegion) -> Result<(), VmError> {
    let old_phys = pr.get_phys_addr().ok_or(VmError::NoPhysBlock)?;
    let new_phys = bitmap.alloc_mem(1, PageAllocFlags::empty())?;
    unsafe {
        core::ptr::copy_nonoverlapping(
            vm_phys_to_virt(old_phys),
            vm_phys_to_virt(new_phys),
            4096,
        );
    }
    pr.unlink_from_block();
    pr.link_to_block(new_block, parent);
    Ok(())
}
```
**设计思考**：CoW 的核心操作是"分配新物理页 → 复制旧页内容 → 更新映射"。在 Minix3 中，"复制旧页内容"需要 `sys_abscopy` 系统调用，因为 VM 无法直接访问物理页。Direct map 使"复制旧页内容"变成一行 `copy_nonoverlapping`。读者应理解：CoW 逻辑的简化不是"去掉了系统调用开销"，而是"VM 不再需要内核作为物理内存访问的中介"。
**参考**: ptregion_design.md §8.7 CoW 复制

### ✅ TODO-15-2: sys_abscopy 说明更新 [已完成]
**位置**: §2 中 `sys_abscopy` 系统调用说明（~L250）
**现状**: "内核提供的物理内存复制接口，直接操作物理地址，无需映射到虚拟地址空间"
**修改**: 更新为 "minix-rs 中 VM 拥有 direct map，可直接通过 `vm_phys_to_virt()` 访问物理页并 memcpy，不再需要 `sys_abscopy` 系统调用"。保留 Minix3 历史描述，增加分析段落说明 `sys_abscopy` 存在的原因（VM 无法直接访问物理内存）和 direct map 如何消除这个需求
**参考**: ptregion_design.md §8.7, §8.9

### ✅ TODO-15-3: Rust 实现代码更新 [已完成]
**位置**: §4 Rust 实现（~L986, L1164）
**现状**: Rust 代码中仍使用 `sys_abscopy`
**修改**: 替换为 `vm_phys_to_virt()` + `copy_nonoverlapping()`
**参考**: ptregion_design.md §8.7

### ✅ TODO-15-4: vm_bytecopies 说明更新 [已完成]
**位置**: ~L1756 vm_bytecopies 说明
**现状**: 描述 `vm_bytecopies` 与 `sys_abscopy` 的关系
**修改**: 更新为 VM 通过 direct map 直接 memcpy，`vm_bytecopies` 统计逻辑不变
**参考**: ptregion_design.md §8.7

---

## 16-pagefault.md — 中度修改

> **文档应传达的设计思考**：16 是页错误处理的文档。读者应理解：
> - 页错误处理是 direct map 统一性的"压力测试"——它同时涉及 CoW 复制、页表写入、物理页分配，是所有机制的交汇点
> - 页表项写入通过 `vm_phys_to_virt()` 获取页表页 VA 后直接写入，不再需要 createpde 临时映射

### ✅ TODO-16-1: sys_abscopy → direct map memcpy [已完成]
**位置**: ~L1316-1317, ~L1338, ~L3103 中 `sys_abscopy` 引用
**现状**: 页错误处理中 CoW 使用 `sys_abscopy`
**修改**: 替换为 `vm_phys_to_virt()` + `copy_nonoverlapping()`
**参考**: ptregion_design.md §8.7

### ✅ TODO-16-2: 页表更新方式更新 [已完成]
**位置**: 页表写入操作描述
**现状**: 可能通过旧机制（PtRegion/createpde）写入页表项
**修改**: 页表项写入通过 `vm_phys_to_virt()` 获取页表页 VA，直接写入
**设计思考**：页错误处理中"写入页表项"是 direct map 统一性的关键验证——在 Minix3 中，这需要 createpde 临时映射窗口；在 PtRegion 方案中，这需要 PtRegion 的 bump allocator 提供 VA；在 direct map 方案中，这只需 `vm_phys_to_virt(pt_phys)` 即可。三种方案的复杂度递减，读者应感受到 direct map 的"终极简化"效果。
**参考**: ptregion_design.md §8.6, §8.7

---

## 17-vm-fork.md — 轻度修改

### ✅ TODO-17-1: fork 页表创建简化 [已完成]
**位置**: fork 中创建子进程页表的描述
**现状**: 可能涉及 PtRegion/pagedir_mappings 步骤
**修改**: fork 创建子进程页表简化为：
1. `bitmap.alloc_mem(1)` → 分配新页目录物理页
2. `vm_phys_to_virt(dir_phys)` → 清零
3. `pt_mapkernel(dir_ptr)` → 建立内核映射（含 kernel direct map）
4. 复制父进程的用户空间映射
**参考**: ptregion_design.md §8.7 创建新进程页表

### ✅ TODO-17-2: fork 中 CoW 设置 [已完成]
**位置**: fork 中 CoW 标记设置
**现状**: 使用 `sys_abscopy` 或旧机制
**修改**: CoW 页复制使用 `vm_phys_to_virt()` + `copy_nonoverlapping()`
**参考**: ptregion_design.md §8.7 CoW 复制

---

## 18-vm-brk.md — 轻度修改

### ✅ TODO-18-1: 物理页分配方式确认 [已完成]
**位置**: brk 中分配物理页的描述
**现状**: 可能通过 `alloc_page()` 分配
**修改**: `alloc_page()` 内部已简化为 `alloc_phys() → vm_phys_to_virt()`，brk 调用方不受影响。确认无需额外修改
**参考**: ptregion_design.md §8.7

---

## 19-vm-map.md — 轻度修改

### ✅ TODO-19-1: VM_MAP_PHYS 实现简化 [已完成]
**位置**: VM_MAP_PHYS 服务描述
**现状**: 物理内存映射可能涉及 createpde 临时映射
**修改**: VM_MAP_PHYS 实现简化 — VM 已有 direct map，映射物理地址到进程虚拟地址不需要临时映射窗口
**参考**: ptregion_design.md §8.9

### ✅ TODO-19-2: mmap 物理页访问方式 [已完成]
**位置**: mmap 中分配和访问物理页的描述
**现状**: 可能通过旧机制访问物理页
**修改**: 统一通过 `vm_phys_to_virt()` 访问
**参考**: ptregion_design.md §8.6

---

## 01~03 — 无修改或极轻度修改

### ✅ TODO-01-1: vmproc 结构确认 [已完成]
**01-vmproc-struct.md**: vmproc 结构本身不受 direct map 影响（`vm_pt` 字段语义不变）。确认无需修改。

### ✅ TODO-02-1: vmproc 表确认 [已完成]
**02-vmproc-table.md**: 进程表管理不受 direct map 影响。确认无需修改。

### ✅ TODO-03-1: ACL 确认 [已完成]
**03-acl.md**: 访问控制与 direct map 无关。确认无需修改。

---

## 修改优先级

| 优先级 | 文档 | 原因 |
|--------|------|------|
| P0-必须 | 04, 05 | 核心数据结构变更（EarlyHeap 消除，PtRegion 演进为 Direct Map，启动流程重写） |
| P0-必须 | 07 | 页表操作是核心路径，pagedir_mappings/createpde 删除影响大 |
| P0-必须 | 09 | VM 自举终点，Phase 2 direct map 扩展 + 搬迁简化 |
| P1-重要 | 06, 08 | 页表结构定义 / GlobalAlloc 对接 vm_phys_to_virt() |
| P1-重要 | 10, 11, 14, 15, 16 | 物理页访问方式变更 |
| P2-一般 | 12, 17, 18, 19 | 间接影响，修改量小 |
| P3-确认 | 01, 02, 03, 13 | 大概率无需修改，需确认 |

---

## ptregion_design.md — 归档处理

### ✅ TODO-PTREGION-1: ptregion_design.md 归档标注 [已完成]
**位置**: ptregion_design.md 全文
**现状**: 完整的 PtRegion 设计文档，包含 §1-7 的 PtRegion 设计和 §8 的 Direct Map 方案
**修改**: 
- 在文档开头增加归档标注："本文档记录了 PtRegion 的完整设计过程。PtRegion 作为方案三（Typestate+PtRegion）的完整描述保留，但最终方案为方案四（Direct Map）。§8 描述了 Direct Map 方案及其与 PtRegion 的关系。本文档的设计演进叙述已融入 05-vm-allocpage.md 的 §3.1-3.6。"
- §1-7 不做修改——保留 PtRegion 设计的完整记录，作为设计插曲
- §8 不做修改——Direct Map 方案的详细描述仍以此文档为权威参考
**设计思考**：ptregion_design.md 不是"被废弃的文档"，而是"设计过程的忠实记录"。它的价值在于：(1) §1-7 展示了 PtRegion 的完整设计推导，帮助读者理解"为什么 PtRegion 是正确的局部解"；(2) §8 展示了 Direct Map 如何超越 PtRegion，帮助读者理解"为什么 Direct Map 是更好的全局解"。保留这份文档，就是保留设计思考的轨迹。

---

## 叙事线索与文档依赖

### VM 启动时序与里程碑

VM 启动是一个从"内核馈赠"到"完全自主"的自举过程。每个文档对应一个里程碑：

```
T0: Kernel 启动 VM
    │  传递: boot_info + 4 页初始页表 + 1GB direct map
    │
    ▼
┌─ 04: 物理内存分配 ─────────────────────────────────────────────┐
│  里程碑: VM 获得物理内存访问能力                                 │
│  之前: VM 只能通过内核预留的 BSS/ReservedRegion 访问内存         │
│  之后: 1GB direct map + bitmap allocator，VM 可分配物理页       │
│  Direct Map 变化: EarlyHeap 消除 → 1GB direct map + bitmap     │
└────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─ 05: 页分配器 ─────────────────────────────────────────────────┐
│  里程碑: VM 可以同时获得物理地址和虚拟地址                       │
│  之前: alloc_phys() 可用，但物理页没有 stable VA                │
│  之后: alloc_phys() + vm_phys_to_virt()，物理页天然有 VA        │
│  Direct Map 变化: PtRegion 演进为 Direct Map，alloc_virt() 消除│
└────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─ 06: 页表结构 ─────────────────────────────────────────────────┐
│  里程碑: VM 拥有可操作的页表数据结构                             │
│  之前: 页表是内核创建的黑盒                                     │
│  之后: VM 理解页表结构，可以创建/修改/销毁页表                   │
│  Direct Map 变化: 新增 VM 初始页表结构、DirectMapArch trait      │
└────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─ 07: 页表操作 ─────────────────────────────────────────────────┐
│  里程碑: VM 可以管理进程地址空间                                 │
│  之前: 页表结构已定义，但操作仍依赖内核                          │
│  之后: 双视图模型 + vm_phys_to_virt() 直接写入页表项             │
│  Direct Map 变化: pagedir_mappings/createpde 消除，双视图模型    │
└────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─ 08: Slab 分配器 → GlobalAlloc 接入 ──────────────────────────┐
│  里程碑: alloc crate 可用，Box<T>/Vec<T> 可用                   │
│  之前: VM 只能用裸指针和手动内存管理                             │
│  之后: VM 可以使用 Rust 标准分配体系                             │
│  Direct Map 变化: GlobalAlloc 底层对接 vm_phys_to_virt()        │
│  注意: 08 不实现专用 slab，统一使用 Rust alloc 体系              │
└────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─ 09: VM 重定位 ────────────────────────────────────────────────┐
│  里程碑: VM 完全建模所有物理内存，alloc crate 完整可用           │
│  之前: 仅有 1GB direct map，bitmap 元数据在启动区                │
│  之后: direct map 覆盖全部物理内存，分配器元数据在堆上           │
│  Direct Map 变化: Phase 2 扩展 direct map 到覆盖全部物理内存    │
│                   Phase 3 可选迁移 bitmap → buddy               │
│  关键: 09 是 VM 自举的终点——从此 VM 完全自主                    │
└────────────────────────────────────────────────────────────────┘
    │
    ▼
  T7: 主循环开始，VM 正常运行
```

**08 和 09 的顺序关系**：

08 必须在 09 之前，因为：
1. 09 的搬迁需要 `Box<T>` 来在堆上分配新数组 → 依赖 08 的 GlobalAlloc
2. 09 的分配器迁移（Phase 3）需要 `Box<dyn PhysAllocator>` → 依赖 08 的 GlobalAlloc
3. 08 使 alloc crate 可用，09 使 alloc crate 完整可用（物理页不再受限）

**09 的里程碑意义**：

09 之前，VM 仅有 kernel 传过来的初始 1GB direct map。09 之后：
- Direct map 覆盖全部物理内存（Phase 2）
- 分配器元数据从启动区搬到堆上（搬迁简化）
- 可选迁移到 buddy 分配器（Phase 3）
- **VM 从"依赖内核馈赠"变为"完全自主管理物理内存"**

### 核心叙事路径

```
04 → 05 → 07 → 08 → 09 是核心路径

04: "VM 如何获得物理内存访问能力？"
    answer: 1GB direct map + bitmap → 扩展到覆盖全部物理内存
    读者应理解：Minix3 的 spare_pagequeue/EarlyHeap 是 32 位限制下的权衡

05: "VM 如何分配页？"
    answer: alloc_phys() + vm_phys_to_virt()，不再需要 alloc_virt()
    读者应理解：递归的根源不是"页表页需要分配"，而是"页表页需要独立 VA 分配路径"
    读者应看到设计演进：BSS → 预留区域+标志 → Typestate+PtRegion → Direct Map

07: "VM 如何操作页表？"
    answer: 双视图模型 + vm_phys_to_virt() 直接写入页表项
    读者应理解：双视图是特权级硬件的必然要求，不是冗余

08: "VM 如何使用 Rust 的 alloc 体系？"
    answer: GlobalAlloc 对接 vm_phys_to_virt()，不实现专用 slab
    读者应理解：Minix3 的 slab 是 C 语言和 32 位限制下的工程补丁，Rust alloc 体系已内置 slab 机制

09: "VM 如何从'依赖内核馈赠'变为'完全自主'？"
    answer: Phase 2 扩展 direct map + 搬迁简化 + Phase 3 可选迁移
    读者应理解：09 是 VM 自举的终点——从此 VM 完全建模所有物理内存
```

### 辐射路径

```
06: 页表结构定义（支撑 04/07 的初始页表和地址空间布局）
15: CoW 机制（05 的 alloc_page + 07 的页表写入 + 10 的 phys_block 的交汇点）
10/11/14/16: 物理页访问方式统一（vm_phys_to_virt() 的具体应用场景）
```

### 文档修改依赖顺序

```
第一批（基础定义）：
  06 → 定义 VM 初始页表结构和 DirectMapArch trait
  04 → 基于 06 的初始页表，重写物理内存初始化流程

第二批（核心机制）：
  05 → 基于 04 的 3 阶段启动，重写页分配器（PtRegion 演进为 Direct Map）
  07 → 基于 06 的双视图布局，重写页表操作

第三批（自举完成）：
  08 → GlobalAlloc 对接 vm_phys_to_virt()，不实现专用 slab
  09 → Phase 2 扩展 direct map + 搬迁简化 + Phase 3 可选迁移

第四批（应用场景）：
  10, 11, 14, 15, 16 → 基于 05/07 的 vm_phys_to_virt()，统一物理页访问方式

第五批（间接影响）：
  12, 17, 18, 19 → 轻度修改，确认即可
```

### ptregion_design.md 与正式文档的关系

ptregion_design.md 是"设计过程文档"，02-stage-vm 系列是"最终产品文档"：
- ptregion_design.md §1-7 的 PtRegion 设计内容 → 融入 05-vm-allocpage.md §3.1-3.4（保留为方案三）
- ptregion_design.md §8 的 Direct Map 方案内容 → 分散融入 04/05/06/07 各文档
- 融入完成后，ptregion_design.md 归档保留，不删除（作为设计过程记录）
