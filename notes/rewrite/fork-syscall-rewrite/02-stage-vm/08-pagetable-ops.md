# 08-pagetable-ops: 页表操作——生命周期、映射写入与跨页表复制

> **分类**: 阶段 3 — 页与页表（操作面）
> **源码**: `minix3/minix/servers/vm/pagetable.c`（`pt_ptalloc` :494 / `pt_ptalloc_in_range` :545 / `ptestr` :587 / `pt_map_in_range` :631 / `pt_ptmap` :685 / `pt_clearmapcache` :751 / `pt_writable` :761 / `pt_writemap` :784 / `pt_checkrange` :943 / `pt_new` :990 / `freepde` :1028 / `pt_allocate_kernel_mapped_pagetables` :1035 / `pt_copy` :1069 / `pt_bind` :1358 / `pt_free` :1427 / `pt_mapkernel` :1442）+ `minix3/minix/servers/vm/vm.h:55-61`（WMF 宏族 + `MAP_NONE`）
> **Rust 模块**: `os/arch/src/arch/paging.rs`（`Paging` trait 操作面 + `clone_range` + `map_kernel`）+ `os/arch/src/x86_64/paging.rs`（`walk_alloc` + `write_pte_dm` 逐条 invlpg）+ `os/servers/vm/src/vmproc/vmproc_handle.rs`（`init_page_table`/`free_page_table`/`write_page_table_mappings`）+ `os/servers/vm/src/pagetable/vm_self_map.rs`（`VmSelfPageTable::adopt` A1 adoption）+ 消费方 `fork.rs`/`exit.rs`/`munmap.rs`/`heap_arena.rs`
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/07-pagetable-struct.md`（页表结构、Direct Map、`Paging` trait 结构）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/06-page-allocator.md`（页分配 + `vm_pt_alloc` 供给页表页）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md`（`init_vm`/`pt_init` 调用点）
> **说明**: 页表**操作**语义模块：**生命周期（`pt_new`/`pt_free`/`pt_bind`）、映射写入（`pt_writemap` + WMF 标志族）、页表页按需分配（`pt_ptalloc`/`pt_ptalloc_in_range`）、跨页表复制（`pt_copy`/`pt_map_in_range`/`pt_ptmap`）、查询校验（`pt_checkrange`/`pt_writable`）、内核协作（`pt_mapkernel`/`pt_clearmapcache`/`pt_allocate_kernel_mapped_pagetables`）**。**不覆盖**：页表结构（07）、页分配（06）、fork/mmap/pagefault/exit/LU 服务流程（18/20/21/16/22/25）。

---

## 1. 概念：页表操作——VM 对地址空间的增删改查

### 1.0 章节引言

07 文档回答了"页表是什么、长什么样、VM 自己的页表怎么访问"；本文档回答 07 §6 过渡段承诺的问题：**页表怎么建、怎么改、怎么删、怎么绑、怎么抄、怎么查**。它在启动时序中的位置：

```
init_vm() ──► pt_init()（pagetable.c:1088，07 结构面已述）
              │
              ▼ 运行期服务路径（本文档的操作面）
   fork ──► pt_new → map_proc_copy（pt_writemap）→ pt_bind → pt_free（回滚）
   exec ──► pt_free → pt_new → pt_bind（VMPPARAM_CLEAR，exit.c:135-137）
   mmap ──► pt_writemap（region.c:280，逐 region 写 PTE）
   munmap ─► pt_writemap（mmap.c:500，WMF_FREE 释放物理页）
   pagefault ─► pt_writemap + pt_checkrange（region.c:153/746-751）
   live update ─► pt_ptalloc_in_range + pt_ptmap + pt_map_in_range（rs.c:251-267、utility.c:326-333）
```

07 建立的结构基础（`PageTable` 类型 + Direct Map + `Paging` trait）在这里被**逐操作消费**。06 提供的页分配（`vm_pt_alloc` 供给页表页）在这里被 `walk_alloc` 调用。三篇合起来构成"页与页表"阶段的完整语义。

### 1.1 页表操作的四类语义

页表操作可以按"对地址空间做什么"分成四类，这是理解 `pt_*` 函数族的主线：

| 类别 | 操作 | C 函数 | 对地址空间的影响 |
|------|------|--------|-----------------|
| 生命周期 | 创建/绑定/释放 | `pt_new`/`pt_bind`/`pt_free` | 地址空间的生与死 |
| 映射写入 | 建立/覆盖/清除/改标志/校验 | `pt_writemap`（+ WMF 族） | 地址空间的映射内容 |
| 跨表复制 | 全量/范围/结构 | `pt_copy`/`pt_map_in_range`/`pt_ptmap` | 两个地址空间之间的搬运 |
| 查询校验 | 范围检查/可写查询 | `pt_checkrange`/`pt_writable` | 只读诊断（SANITYCHECKS） |

外加两个**内核协作**操作：`pt_mapkernel`（每进程页表映射内核区）和 `pt_clearmapcache`（通知内核清 TLB 缓存）。这四类 + 协作操作对应了 C 侧全部 14 个 pt_* 操作函数（plan §5.3 08 契约）。

**为什么分这四类**：生命周期决定"页表何时存在"，映射写入决定"映射长什么样"，跨表复制决定"映射如何在进程间转移"，查询校验决定"我们怎么确认页表是对的"。它们分别被 fork/exec/exit、mmap/pagefault、live update、sanity 检查消费——服务路径的多样性要求操作面按"语义类别"组织，而不是按函数名字母序。

### 1.2 生命周期：创建—绑定—释放

一个进程地址空间的生命周期由三个操作驱动：

```
fork/boot:  pt_new（空页表）→ map_proc_copy（写映射）→ pt_bind（激活）
exec:       pt_free（清旧）→ pt_new（建新）→ pt_bind（激活）→ handle_memory（逐步映射）
exit:       pt_free（释放）
```

**创建（`pt_new`）**：分配一个页目录物理页并清零，然后立即调用 `pt_mapkernel` 映射内核区。关键约束是"**页目录不重复分配**"——一旦某个进程槽位的页目录被创建，就不会释放或重新分配（pagetable.c:994-997 注释给出两个理由：略快 + 避免失效内核 `page_directories` 中指向页目录的映射）。这意味着 C 中页目录的生命周期与**进程槽位**绑定，而非与进程绑定。

**绑定（`pt_bind`）**：把页目录的物理地址登记进 `pagedir_mappings` 登记册（让内核能通过虚拟地址访问该进程的页目录），并调用 `sys_vmctl_set_addrspace` 通知内核把根地址写入进程控制块（`p_seg.p_cr3`），若进程正在运行则内核立即 `write_cr3` 切换。**绑定是地址空间从"VM 的私有数据结构"变成"CPU 可执行的地址空间"的边界**。

**释放（`pt_free`）**：只释放二级页表页，**不释放页目录和物理页框**——页目录与槽位绑定复用，物理页框由 region/phys_block 引用计数管理（11 文档）。这体现了 C 的职责划分：页表层只管"页表页"这种 VM 元数据页，用户物理页的生命周期归内存管理模块。

### 1.3 映射写入：WMF 标志族是"四种写模式"的契约

`pt_writemap` 是 VM 最核心的页表操作，但它不是一个"写映射"函数——它是**四种写模式的统一入口**，由 `writemapflags` 参数区分：

| 标志 | 值 | 语义 | C 典型调用 |
|------|-----|------|-----------|
| `WMF_OVERWRITE` | 0x01 | 允许覆盖已有映射（**常态**） | region.c:285、pagetable.c:713/743 |
| `WMF_WRITEFLAGSONLY` | 0x02 | 保留原物理地址，只更新标志位 | pagetable.c:426（vm_lockpage） |
| `WMF_FREE` | 0x04 | 解除映射并释放物理页 | mmap.c:500（munmap_vm_lin） |
| `WMF_VERIFY` | 0x08 | 只校验页表项是否匹配，不写入 | region.c:153（map_ph_writept） |

`MAP_NONE`（0xFFFFFFFE，vm.h:61）是"清除映射"的物理地址哨兵：`physaddr == MAP_NONE` 时 `flags` 必须为 0（断言 `physaddr != MAP_NONE || !flags`，pagetable.c:824-825），PTE 写入 `MAP_NONE` 后 PRESENT 位为 0，映射即清除。

**为什么要强调"覆盖是常态"**：C 侧几乎所有写映射调用都带 `WMF_OVERWRITE`——因为 region 映射路径经常在同一地址上重新写入（如 pagefault 处理、mmap 扩展）。Rust 把这一事实变成设计输入：严格 `map`（拒绝已映射）只用于"此处不应有映射"的场景（fork 子表），其余场景用原子覆盖 `remap`。这是 §3.1 D1 的动机。

### 1.4 页表页按需分配与递归副作用

`pt_writemap` 写入 PTE 之前，目标 PDE 对应的二级页表必须存在。C 用 `pt_ptalloc_in_range` 在**写任何 PTE 之前**先为整个地址范围预分配全部二级页表——注释明说理由："在中途失败后撤销工作很痛苦"（pagetable.c:828-830）。这是"先保证结构完备，再改内容"的两阶段策略。

`pt_ptalloc` 分配二级页表时有一个微妙的**递归副作用**：`vm_allocpage`（06 文档）在分配页表页时可能递归触发 `vm_mappages` → `pt_writemap` → `pt_ptalloc`，内层调用可能已经完成了同一个 PDE 的分配。因此外层 `pt_ptalloc` 在分配后必须检查 `pt->pt_pt[pde]` 是否已被内层设置——若是，则释放刚分配的页并直接返回（pagetable.c:513-517）。

这个递归的存在前提是：**32 位 VM 分配物理页后必须先把该页映射进自己的地址空间才能访问它**（VM 没有 Direct Map，`vm_mappages` 动态找 VA 映射）。minix-rs 的 Direct Map 使物理页天然可访问，递归路径结构性消失——这是 §3.2 D2 的核心论证。

### 1.5 跨页表复制三兄弟

C 有三个"把映射从一个页表弄到另一个"的函数，复制对象不同：

| 函数 | 复制对象 | 方法 | 用途 |
|------|---------|------|------|
| `pt_copy` | 用户区**全部 PTE 值** | `memcpy` 整页表批量 | pt_init 阶段 6 动态化重建（仅此一处） |
| `pt_map_in_range` | 指定范围**逐 PTE 值** | 循环赋值 | LU `swap_proc_dyn_data`（utility.c:326/331） |
| `pt_ptmap` | 页表**结构本身**（页目录 + 二级页表的 VA→PA 映射） | `pt_writemap` 逐项 | LU 新 VM 实例访问旧 VM 页表结构（rs.c:263/267） |

前两者复制"映射值"（用户空间内容），后者复制"页表页的可达性"（让目标进程能通过虚拟地址访问源进程的页表数据结构）。区分这两类非常重要：**复制映射值是把内容抄过去，复制结构映射是让抄写者能看到原文**——LU 场景需要后者是因为新 VM 实例要先读旧 VM 的页表（经 `pt_map_in_range` 抄内容）之前，必须先能看到旧 VM 的页表结构本身。

`pt_copy` 与 `pt_map_in_range` 的关系：前者是后者的全量版本（遍历用户区全部 PDE）。C 用 `memcpy` 批量复制是 32 位特有的优化——PTE 是 u32、一个页表恰好 4KB 一页。x86-64 下 PTE 是 u64，页表结构不同，这个优化不成立（见 §3.5 D5）。

### 1.6 查询校验：只读诊断语义

`pt_checkrange`（范围 Present + 可写检查）和 `pt_writable`（单页可写查询）在 C 中**全部调用点都被 `#if SANITYCHECKS` 包裹**——它们是调试期断言，不是生产 API：

- `pt_checkrange` 仅 region.c:746-751（`map_pf` 写 PTE 后验证映射确实建立）
- `pt_writable` 仅 region.c:55（调试输出打印物理页是 R 还是 W）

`ptestr`（pagetable.c:587）是 PTE 调试打印辅助，仅被 `pt_writemap` 的 verify 失败路径和诊断输出使用。

它们的架构差异也值得注意：i386 用 `PTF_WRITE` 正逻辑（置位=可写），ARM 用 `ARCH_VM_PTE_RO` 反逻辑（置位=只读）——`pt_writable` 里 `#if` 分支（pagetable.c:772-777）直接体现了这一点。minix-rs 的 `PageFlags::WRITABLE` 是 OS 层统一语义，反相位在 arch 的 `flags_to_pte`/`pte_to_flags` 翻译层处理（07 D4 已述）。

### 1.7 内核协作：每进程内核映射与 TLB 维护

两个操作把"VM 的页表"与"内核的执行环境"连接起来：

**`pt_mapkernel`**：每个进程页表必须映射内核地址空间，否则系统调用/中断进入 ring 0 时内核代码无法执行（Minix3 内核不切换页表，直接使用当前进程的页表）。C 的三段映射：
1. **内核代码段**：从 `kern_start_pde` 开始以 4MB 大页（x86，`ARCH_VM_BIGPAGE`）映射 `kern_size` 字节，带 `global_bit`（PGE 支持时，CR3 切换不刷新）
2. **pagedir_mappings 登记册 PDE**：把 5 个登记册 PDE（`MAX_PAGEDIR_PDES`）写入页目录，使内核能经这些 PDE 访问所有进程的页目录
3. **kern_mappings 特殊映射**：内核通过 `sys_vmctl_get_mapping` 预留的特殊映射（视频内存、APIC 等），逐条 `pt_writemap`

**`pt_clearmapcache`**：`sys_vmctl(SELF, VMCTL_CLEARMAPCACHE, 0)`——通知内核清除其内部的页表映射缓存，确保内核在使用当前页表建立新映射前使 TLB 失效。调用点在 main.c:212/719/749（VM 自身页表建立/LU 切换后）和 pagefaults.c:153。

### 1.8 TLB 维护的架构差异：C 的全局清缓存 vs Direct Map 逐条 invlpg

这是 08 最核心的架构论证（[ARCH: A-1] 的操作面后果）：

- **C 侧**：VM 批量修改 PTE 后，内核的 TLB 可能还缓存旧映射。由于 32 位内核用临时映射窗口（`freepdes`，见 draft/07 素材附录 A）访问非当前进程的内存，缓存失效必须**全局**通知——`VMCTL_FLUSHTLB`/`VMCTL_CLEARMAPCACHE` 是"我改了页表，请让所有相关 TLB 条目失效"的粗粒度协议。
- **minix-rs 侧**：`Paging` 实现每次写 PTE 即对目标虚拟地址执行单条 `invlpg`（x86-64 `write_pte_dm`，paging.rs:142-152：`write_volatile` 写 PTE 后紧跟 `invlpg`）。Direct Map 使"写 PTE 的地址"（页表页的 DM VA）与"被修改映射的虚拟地址"都直接可达——逐条失效足够且更精确，**全局清缓存机制结构性消失**。

类比：C 是"改完一整面墙后请刷新整块屏幕"，minix-rs 是"每改一个像素就刷新该像素"。后者在单线程 VM 模型下开销可忽略，且消除了"改完忘刷新"的整类 bug（刷新与写入绑定在同一函数内，无遗漏窗口）。

注意一个实现细节：`write_pte_dm` 对**中间表项**（PML4/PDPT/PD）传 `vaddr_for_flush=0`（paging.rs:139-140 注释：中间表没有 leaf TLB 条目，保守 no-op）；只有 leaf PTE 写入才真正失效目标地址。这个"区分中间表/叶表"的处理是 4 级层级（[ARCH: A-2]）带来的 TLB 语义细化。

### 1.9 对照 Redox / Linux

现代 OS 的页表操作与 minix-rs 的抽象同构：

- **Redox**（rmm/src/page/mapper.rs）：`PageMapper<A, F>` 用 `FrameAllocator` 分配中间页表页——与 minix-rs `walk_alloc` 的"按需分配中间表"同构；Redox 2020-12 起也用 `virt = phys + KERNEL_OFFSET` 线性映射访问物理页，与 Direct Map 同构。
- **Linux**：内核线性映射区（`va = pa + PAGE_OFFSET`）+ `set_pte_at`/`pte_modify` 分层操作 + 显式 `flush_tlb_*` 系列。Linux 的 `pte_modify` 保留物理地址只改标志，对应 `update_flags`；`set_pte_at` 写后由架构层负责 TLB 失效，对应 `write_pte_dm` 的写+invlpg 绑定。
- **Minix3 的独特性**：32 位无 Direct Map，VM 必须动态映射页表页/自用页才能访问，因此出现 `pagedir_mappings` 登记册（内核访问进程页目录的中介）、`freepdes` 临时窗口（内核访问任意物理页）、写后全局刷 TLB 三件套。这三件套在 64 位 Direct Map 下全部消失——不是"优化"，是**机制被替代**（§3.6 D6 / §3.8 D8）。

### 1.10 本章小结

页表操作的四类语义 + 内核协作对应 C 侧 14 个函数。理解主线的钥匙是"**C 的写映射要自己管三件事：页表页分配、覆盖策略、事后刷 TLB；minix-rs 把三件事分别内化进 walk_alloc、方法分治、写时 invlpg**"。接下来的 C 分析逐函数展开这些语义，Rust 设计决策（§3）给出每个差异的设计理由。

---

## 2. C 源码分析

### 2.0 本章定位

本章逐函数分析 pagetable.c 的 14 个 pt_* 操作函数 + 2 个静态辅助（`ptestr`/`freepde`）+ WMF 宏族。行号以 2026-08-15 grep 实证为准。函数清单：

| 函数 | 行号 | 类别 | 用途 |
|------|------|------|------|
| `pt_ptalloc`（static） | 494 | 映射写入前置 | 分配二级页表 + 写 PDE |
| `pt_ptalloc_in_range` | 545 | 映射写入前置 | 范围预分配二级页表 |
| `ptestr`（static） | 587 | 查询校验 | PTE 调试打印 |
| `pt_map_in_range` | 631 | 跨表复制 | 范围复制映射值 |
| `pt_ptmap` | 685 | 跨表复制 | 转移页表结构映射 |
| `pt_clearmapcache` | 751 | 内核协作 | 清内核 TLB 缓存 |
| `pt_writable` | 761 | 查询校验 | 单页可写查询 |
| `pt_writemap` | 784 | 映射写入 | 核心写映射 |
| `pt_checkrange` | 943 | 查询校验 | 范围 Present + 可写检查 |
| `pt_new` | 990 | 生命周期 | 创建页表 |
| `freepde`（static） | 1028 | 内核协作 | 分配登记册 PDE 编号 |
| `pt_allocate_kernel_mapped_pagetables` | 1035 | 内核协作 | pagedir_mappings 登记册初始化 |
| `pt_copy`（static） | 1069 | 跨表复制 | 全量复制用户区 PTE |
| `pt_bind` | 1358 | 生命周期 | 绑定页表到进程 |
| `pt_free` | 1427 | 生命周期 | 释放二级页表 |
| `pt_mapkernel` | 1442 | 内核协作 | 每进程内核映射 |

（`pt_init` :1088 的结构面已在 07 §2.5 分析；本章只引用其调用关系，不重复操作细节。）

### 2.1 pt_ptalloc：分配二级页表 + 写 PDE（pagetable.c:494）

```c
static int pt_ptalloc(pt_t *pt, int pde, u32_t flags)
{
	int i;
	phys_bytes pt_phys;
	u32_t *p;

	/* 参数合法 + 不覆盖已有项 */
	assert(pde >= 0 && pde < ARCH_VM_DIR_ENTRIES);
	assert(!(flags & ~(PTF_ALLFLAGS)));
	assert(!(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT));
	assert(!pt->pt_pt[pde]);

	/* 分配页表页。vm_allocpage 可能递归创建同一 PDE（副作用），
	 * 此时释放刚分配的页，直接返回 OK。
	 */
	if (!(p = vm_allocpage(&pt_phys, VMP_PAGETABLE)))
		return ENOMEM;
	if (pt->pt_pt[pde]) {
		vm_freepages((vir_bytes) p, 1);
		assert(pt->pt_pt[pde]);
		return OK;
	}
	pt->pt_pt[pde] = p;

	for(i = 0; i < ARCH_VM_PT_ENTRIES; i++)
		pt->pt_pt[pde][i] = 0;	/* 空表项 */

#if defined(__i386__)
	pt->pt_dir[pde] = (pt_phys & ARCH_VM_ADDR_MASK) | flags
		| ARCH_VM_PDE_PRESENT | ARCH_VM_PTE_USER | ARCH_VM_PTE_RW;
#elif defined(__arm__)
	pt->pt_dir[pde] = (pt_phys & ARCH_VM_PDE_MASK)
		| ARCH_VM_PDE_PRESENT | ARM_VM_PDE_DOMAIN; //LSC FIXME
#endif

	return OK;
}
```

**语义要点**：
- **PDE 恒置 `PRESENT|USER|RW`**（i386）——保护完全依赖 PTE，PDE 层不做权限区分。这是两级页表的常见约定：PDE 只负责"页表在不在"，权限在叶表统一表达。
- **递归副作用**（pagetable.c:513-517）：`vm_allocpage` 分配页时可能递归触发内层 `pt_ptalloc` 完成同一 PDE——外层检查 `pt->pt_pt[pde]` 已设置则释放多余页返回 OK。这个检查是 `vm_allocpage`（06）与 `pt_ptalloc` 之间的隐式协议。
- 分配失败返回 `ENOMEM`（errno 语义，非自定义错误码）。

### 2.2 pt_ptalloc_in_range：范围预分配（pagetable.c:545）

```c
int pt_ptalloc_in_range(pt_t *pt, vir_bytes start, vir_bytes end,
	u32_t flags, int verify)
{
	int pde, first_pde, last_pde;

	first_pde = ARCH_VM_PDE(start);
	last_pde = ARCH_VM_PDE(end-1);
	assert(first_pde >= 0);
	assert(last_pde < ARCH_VM_DIR_ENTRIES);

	for(pde = first_pde; pde <= last_pde; pde++) {
		assert(!(pt->pt_dir[pde] & ARCH_VM_BIGPAGE));
		if(!(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT)) {
			int r;
			if(verify) {
				printf("pt_ptalloc_in_range: no pde %d\n", pde);
				return EFAULT;
			}
			assert(!pt->pt_dir[pde]);
			if((r=pt_ptalloc(pt, pde, flags)) != OK) {
				/* 失败不回收已分配的页表——它们仍可写、
				 * 不指向无意义内容、状态一致（注释自述）。
				 */
				return r;
			}
			assert(pt->pt_pt[pde]);
		}
		assert(pt->pt_pt[pde]);
		assert(pt->pt_dir[pde]);
		assert(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT);
	}

	return OK;
}
```

**语义要点**：
- **verify 模式**：缺 PDE 直接 `EFAULT`，不分配——用于"预期映射已存在，只是确认"的场景（对应 §2.4 的 WMF_VERIFY 前置）。
- **失败不回收**：中途 `pt_ptalloc` 失败时，已分配的页表保留（注释给出的理由：仍可写、不指向 nonsense、pt_ptalloc 使目录和数据保持一致状态）。这是 C 的两阶段策略的容错设计——已分配的部分无害，放弃整个操作即可。
- `end` 是开区间上界（`end-1` 取最后 PDE），调用方（`pt_writemap`）传 `v + VM_PAGE_SIZE*pages`。

### 2.3 ptestr：PTE 调试打印（pagetable.c:587）

```c
static const char *ptestr(u32_t pte)
{
	static char str[30];
	if(!(pte & ARCH_VM_PTE_PRESENT)) {
		return "not present";
	}
	str[0] = '\0';
#if defined(__i386__)
	FLAG(ARCH_VM_PTE_RW, "W");
#elif defined(__arm__)
	if(pte & ARCH_VM_PTE_RO) { strcat(str, "R "); }
	else { strcat(str, "W "); }
#endif
	FLAG(ARCH_VM_PTE_USER, "U");
	/* i386: PWT/PCD/ACC/DIRTY/PS/G/AV1/AV2/AV3；arm: S/SH/WB/WT */
	return str;
}
```

仅被 `pt_writemap` 的 verify 失败诊断（打印 found/masked/expected 三个 PTE 的字符串形式）和调试输出使用。i386 用 `FLAG(RW, "W")` 正逻辑、ARM 用 `RO` 判断取反——与 `pt_writable` 相同的架构反相模式（§1.6）。minix-rs 的 `PageFlags` `Display` 实现（paging.rs:48-72，`P/W/U/X/G/WT/NC/A/D/GUARD`）承担了同样的诊断角色，但**无架构反相**——统一 OS 语义。

### 2.4 pt_writemap：核心写映射（pagetable.c:784）

```c
int pt_writemap(struct vmproc * vmp,
			pt_t *pt,
			vir_bytes v,
			phys_bytes physaddr,
			size_t bytes,
			u32_t flags,
			u32_t writemapflags)
{
	int p, pages;
	int verify = 0;
	int ret = OK;

	/* WMF_VERIFY：只校验不写 */
	if(writemapflags & WMF_VERIFY)
		verify = 1;

	assert(!(bytes % VM_PAGE_SIZE));
	assert(!(flags & ~(PTF_ALLFLAGS)));

	pages = bytes / VM_PAGE_SIZE;

	/* MAP_NONE 表示清除映射；两个断言约束 physaddr 与 flags 的组合 */
	assert(physaddr == MAP_NONE || (flags & ARCH_VM_PTE_PRESENT));
	assert(physaddr != MAP_NONE || !flags);

	/* 阶段 1：预分配全部二级页表（避免中途失败难回滚） */
	ret = pt_ptalloc_in_range(pt, v, v + VM_PAGE_SIZE*pages, flags, verify);
	if(ret != OK) {
		printf("VM: writemap: pt_ptalloc_in_range failed\n");
		goto resume_exit;
	}

	/* 阶段 2：逐页写 PTE */
	for(p = 0; p < pages; p++) {
		u32_t entry;
		int pde = ARCH_VM_PDE(v);
		int pte = ARCH_VM_PTE(v);

		assert(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT);
		assert(!(pt->pt_dir[pde] & ARCH_VM_BIGPAGE));
		assert(pt->pt_pt[pde]);

		/* WRITEFLAGSONLY/FREE：保留原物理地址，只改标志或释放 */
		if(writemapflags & (WMF_WRITEFLAGSONLY|WMF_FREE)) {
			physaddr = pt->pt_pt[pde][pte] & ARCH_VM_ADDR_MASK;
		}

		if(writemapflags & WMF_FREE) {
			free_mem(ABS2CLICK(physaddr), 1);
		}

		entry = (physaddr & ARCH_VM_ADDR_MASK) | flags;

		if(verify) {
			/* 宽松比较：i386 掩 ACC/DIRTY；可写期望接受只读；
			 * ARM 掩 WB/WT。不匹配 → EFAULT + 诊断打印。
			 */
			u32_t maskedentry = pt->pt_pt[pde][pte];
#if defined(__i386__)
			maskedentry &= ~(I386_VM_ACC|I386_VM_DIRTY);
			if(entry & ARCH_VM_PTE_RW) maskedentry |= ARCH_VM_PTE_RW;
#elif defined(__arm__)
			if(!(entry & ARCH_VM_PTE_RO)) maskedentry &= ~ARCH_VM_PTE_RO;
			maskedentry &= ~(ARM_VM_PTE_WB|ARM_VM_PTE_WT);
#endif
			if(maskedentry != entry) {
				/* 打印 ptestr(found/masked/expected) */
				ret = EFAULT;
				goto resume_exit;
			}
		} else {
			pt->pt_pt[pde][pte] = entry;
		}

		physaddr += VM_PAGE_SIZE;
		v += VM_PAGE_SIZE;
	}

resume_exit:
	/* SMP：VMINHIBIT_SET/CLEAR 包裹整个操作（CONFIG_SMP） */
	return ret;
}
```

**语义要点**：
1. **两阶段**：先 `pt_ptalloc_in_range`（§2.2）保证全部 PDE 存在，再逐页写 PTE。verify 模式下预分配阶段也传 verify（缺 PDE 即 EFAULT）。
2. **WMF_WRITEFLAGSONLY/WMF_FREE 先取原物理地址**（pagetable.c:858-864）：调用方传的 `physaddr` 被忽略（vm_lockpage 传 0），从现有 PTE 提取物理地址。
3. **WMF_FREE 释放物理页**（pagetable.c:866-868）：`free_mem(ABS2CLICK(physaddr), 1)`——写映射与物理页释放耦合在同一函数。
4. **verify 的宽松比较**：i386 掩掉 ACC/DIRTY（硬件可能已置位），"期望可写"接受"实际只读"（CoW 中间态）；ARM 掩掉 WB/WT 缓存位。不匹配返回 `EFAULT` 并打印三个 PTE 的诊断（found/masked/expected，经 `ptestr`）。
5. **SMP 包裹**（CONFIG_SMP，pagetable.c:787-808）：操作前对目标进程 `VMCTL_VMINHIBIT_SET` 暂停，操作后 `VMCTL_VMINHIBIT_CLEAR` 恢复——防止运行中的进程在页表被改写时访问到不一致状态。minix-rs 的 VM 是单线程事件循环（无 SMP 竞争），此机制无对应（执行模型差异，AGENTS.md 已述）。

**调用场景**（WMF 组合实证）：

| 场景 | 位置 | flags | writemapflags |
|------|------|-------|---------------|
| 建立映射（map_writept） | region.c:280 | `PTF_PRESENT\|PTF_USER\|rw` | `WMF_OVERWRITE` |
| 验证映射（map_pf） | region.c:153 | 同上 | `WMF_VERIFY` |
| 取消映射（map_unmap_region） | region.c:1139 | 0 | `WMF_OVERWRITE` |
| 修改权限（vm_lockpage） | pagetable.c:425 | 新 flags | `WMF_OVERWRITE\|WMF_WRITEFLAGSONLY` |
| 释放映射（munmap_vm_lin） | mmap.c:500 | 0 | `WMF_OVERWRITE\|WMF_FREE` |

### 2.5 pt_checkrange：范围 Present + 可写检查（pagetable.c:943）

```c
int pt_checkrange(pt_t *pt, vir_bytes v,  size_t bytes, int write)
{
	int p, pages;
	assert(!(bytes % VM_PAGE_SIZE));
	pages = bytes / VM_PAGE_SIZE;

	for(p = 0; p < pages; p++) {
		int pde = ARCH_VM_PDE(v);
		int pte = ARCH_VM_PTE(v);

		if(!(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT))
			return EFAULT;
		assert((pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT) && pt->pt_pt[pde]);
		if(!(pt->pt_pt[pde][pte] & ARCH_VM_PTE_PRESENT))
			return EFAULT;
#if defined(__i386__)
		if(write && !(pt->pt_pt[pde][pte] & ARCH_VM_PTE_RW))
#elif defined(__arm__)
		if(write && (pt->pt_pt[pde][pte] & ARCH_VM_PTE_RO))
#endif
			return EFAULT;
		v += VM_PAGE_SIZE;
	}
	return OK;
}
```

**语义要点**：
- 三级检查：PDE Present → PTE Present → （可选）PTE 可写。任一失败返回 `EFAULT`。
- **全部调用点仅 region.c:746-751**（`map_pf` 内，`#if SANITYCHECKS`）——页错误处理写 PTE 后验证映射确实建立。这是调试期断言，不是生产路径。
- 注意它对 bigpage PDE 的隐患：`pt->pt_pt[pde]` 对 bigpage 为 NULL，`pt->pt_pt[pde][pte]` 会解引用空指针——因此调用范围不能含大页（VM 的用户区映射全为 4KB 页，此前提成立）。

### 2.6 pt_writable：单页可写查询（pagetable.c:761）

```c
int pt_writable(struct vmproc *vmp, vir_bytes v)
{
	u32_t entry;
	pt_t *pt = &vmp->vm_pt;
	assert(!(v % VM_PAGE_SIZE));
	int pde = ARCH_VM_PDE(v);
	int pte = ARCH_VM_PTE(v);

	assert(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT);
	assert(pt->pt_pt[pde]);

	entry = pt->pt_pt[pde][pte];

#if defined(__i386__)
	return((entry & PTF_WRITE) ? 1 : 0);
#elif defined(__arm__)
	return((entry & ARCH_VM_PTE_RO) ? 0 : 1);
#endif
}
```

**语义要点**：
- 断言页表存在（PDE Present + 页表已分配）——调用方必须保证地址已映射。
- **i386/ARM 反相**：`PTF_WRITE` 正逻辑 vs `ARCH_VM_PTE_RO` 反逻辑。minix-rs 的 `PageFlags::WRITABLE` 统一语义（反相在 `pte_to_flags` 翻译层）。
- **唯一调用点 region.c:55**（SANITYCHECKS 调试输出打印 R/W）——与 `pt_checkrange` 一样是诊断语义。

### 2.7 pt_map_in_range：范围复制映射值（pagetable.c:631）

```c
int pt_map_in_range(struct vmproc *src_vmp, struct vmproc *dst_vmp,
	vir_bytes start, vir_bytes end)
{
	pt = &src_vmp->vm_pt;
	dst_pt = &dst_vmp->vm_pt;

	end = end ? end : VM_DATATOP;   /* 默认扫到用户空间顶部 */
	assert(start % VM_PAGE_SIZE == 0);
	assert(end % VM_PAGE_SIZE == 0);
	assert(start <= end);

	for(viraddr = start; viraddr <= end; viraddr += VM_PAGE_SIZE) {
		pde = ARCH_VM_PDE(viraddr);
		if(!(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT)) {
			if(viraddr == VM_DATATOP) break;
			continue;
		}
		pte = ARCH_VM_PTE(viraddr);
		if(!(pt->pt_pt[pde][pte] & ARCH_VM_PTE_PRESENT)) {
			if(viraddr == VM_DATATOP) break;
			continue;
		}
		dst_pt->pt_pt[pde][pte] = pt->pt_pt[pde][pte];
		assert(dst_pt->pt_pt[pde]);
		if(viraddr == VM_DATATOP) break;
	}
	return OK;
}
```

**语义要点**：
- **跳过缺失**：源 PDE/PTE 不存在时静默跳过（只复制已存在的映射）。
- **前置条件**：目标二级页表必须已分配（`assert(dst_pt->pt_pt[pde])` 在写入后检查，位置不当——但调用场景保证前置成立：LU 前 `pt_ptalloc_in_range` 已预分配，见 rs.c:251/256）。
- **调用场景**：仅 LU `swap_proc_dyn_data`（utility.c:326/331）——转移 VM 的堆/mmap 区域（`VM_OWN_HEAPBASE`→`VM_OWN_MMAPTOP`）和栈区域（`VM_STACKTOP`→`VM_DATATOP`）的映射到新 VM 实例。
- 循环的 `if(viraddr == VM_DATATOP) break;` 是防溢出保护（`viraddr += VM_PAGE_SIZE` 到 `VM_DATATOP` 后停止）。

### 2.8 pt_ptmap：转移页表结构映射（pagetable.c:685）

```c
int pt_ptmap(struct vmproc *src_vmp, struct vmproc *dst_vmp)
{
	pt = &src_vmp->vm_pt;

	/* 1. 转移页目录映射：src 的 pt_dir_phys 映射到 dst 的 pt_dir VA */
	viraddr = (vir_bytes) pt->pt_dir;
	physaddr = pt->pt_dir_phys & ARCH_VM_ADDR_MASK;
	if((r=pt_writemap(dst_vmp, &dst_vmp->vm_pt, viraddr, physaddr,
		VM_PAGE_SIZE, ARCH_VM_PTE_PRESENT|ARCH_VM_PTE_USER|ARCH_VM_PTE_RW,
		WMF_OVERWRITE)) != OK) {
		return r;
	}

	/* 2. 转移全部用户区二级页表映射 */
	for(pde=0; pde < kern_start_pde; pde++) {
		if(!(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT)) continue;
		if(!pt->pt_pt[pde]) { panic("pde %d empty\n", pde); }

		viraddr = (vir_bytes) pt->pt_pt[pde];
		physaddr = pt->pt_dir[pde] & ARCH_VM_ADDR_MASK;
		if((r=pt_writemap(dst_vmp, &dst_vmp->vm_pt, viraddr, physaddr,
			VM_PAGE_SIZE, ARCH_VM_PTE_PRESENT|ARCH_VM_PTE_USER|ARCH_VM_PTE_RW,
			WMF_OVERWRITE)) != OK) {
			return r;
		}
	}
	return OK;
}
```

**语义要点**：
- 复制对象是**页表结构的 VA→PA 映射**：页目录页本身（`pt_dir`→`pt_dir_phys`）+ 每个已存在 PDE 的二级页表页（`pt_pt[pde]` VA → PDE 中的物理地址）。
- 复制手段是 `pt_writemap`（WMF_OVERWRITE）——目标地址空间里建立"能看到源页表结构"的映射。
- **调用场景**：仅 LU 流程 rs.c:263/267——`pt_ptmap(this_vm, new_vm)`（新 VM 看到旧 VM 的页表结构）+ `pt_ptmap(new_vm, new_vm)`（新 VM 自映射，让新 VM 看到自己的页表结构）。前置是 rs.c:251/256 的 `pt_ptalloc_in_range`（为整个地址空间预分配页表）。
- **minix-rs 无直接对应**：新 VM 实例经 `kernel_phys_to_virt(cr3_phys)` 直接读旧 VM 的页表页，无需"结构映射窗口"（§3.5 D5，A-1 结构性消除）。

### 2.9 pt_clearmapcache：清内核 TLB 缓存（pagetable.c:751）

```c
void pt_clearmapcache(void)
{
	if(sys_vmctl(SELF, VMCTL_CLEARMAPCACHE, 0) != OK)
		panic("VMCTL_CLEARMAPCACHE failed");
}
```

**语义要点**：
- 通知内核：当前页表（VM 自身的）的映射缓存要清除，确保内核在用当前页表建立新映射前使 TLB 失效。
- 调用点：main.c:212（VM 自身页表建立后）、main.c:719（LU 新旧 VM 槽位重新绑定后）、main.c:749（LU 流程）、pagefaults.c:153（页错误处理后）。
- 语义前提是 §1.8 的架构差异：C 的"批量改 PTE + 事后全局清缓存"协议；minix-rs 以逐条 invlpg 结构性替代。

### 2.10 pt_new / pt_free / pt_bind：生命周期三件套

**pt_new（pagetable.c:990）**：

```c
int pt_new(pt_t *pt)
{
	/* 页目录不重复分配（与槽位绑定）：略快 + 避免失效
	 * 内核 page_directories 中指向页目录的映射 */
        if(!pt->pt_dir &&
          !(pt->pt_dir = vm_allocpages((phys_bytes *)&pt->pt_dir_phys,
	  	VMP_PAGEDIR, ARCH_PAGEDIR_SIZE/VM_PAGE_SIZE))) {
		return ENOMEM;
	}
	assert(!((u32_t)pt->pt_dir_phys % ARCH_PAGEDIR_SIZE));

	for(i = 0; i < ARCH_VM_DIR_ENTRIES; i++) {
		pt->pt_dir[i] = 0;	/* PRESENT bit = 0 */
		pt->pt_pt[i] = NULL;
	}
	pt->pt_virtop = 0;	/* 冗余字段，从未被读取（07 §2.1 已述） */

        if((r=pt_mapkernel(pt)) != OK)
		return r;
	return OK;
}
```

要点：分配页目录（`VMP_PAGEDIR`，页对齐）+ 清零 + `pt_mapkernel`。**"不重复分配"**（pagetable.c:994-997 注释）是 C 的关键不变量——页目录生命周期绑定进程槽位。

**pt_free（pagetable.c:1427）**：

```c
void pt_free(pt_t *pt)
{
	for(i = 0; i < ARCH_VM_DIR_ENTRIES; i++)
		if(pt->pt_pt[i])
			vm_freepages((vir_bytes) pt->pt_pt[i], 1);
}
```

要点：只释放二级页表（`vm_freepages`，06 范围）；**不释放页目录、不释放用户物理页框**——页目录与槽位复用，用户页归 region/phys_block 生命周期。

**pt_bind（pagetable.c:1358）** 五步（§1.2 已述结构）：

```c
	/* 1. 定位登记册槽位 */
	procslot = who->vm_slot;
	pdm = &pagedir_mappings[procslot/slots_per_pde];
	pdeslot = procslot%slots_per_pde;
	pagedir_pde = pdm->pdeno;
	/* 2. 提取物理地址 */
	phys = pt->pt_dir_phys & ARCH_VM_ADDR_MASK;
	/* 3. 写入登记册（page_directories 页表对应槽位） */
	pdm->page_directories[pdeslot] = phys | ARCH_VM_PDE_PRESENT|ARCH_VM_PTE_RW;
	/* 4. 计算内核访问该页目录的虚拟地址 */
	pdes = (void *) (pagedir_pde*ARCH_BIG_PAGE_SIZE + pdeslot*VM_PAGE_SIZE);
	/* 5. 通知内核（写 p_cr3，运行中则立即 write_cr3） */
	return sys_vmctl_set_addrspace(who->vm_endpoint, pt->pt_dir_phys, pdes);
```

**地址视角总结**：页目录物理地址 `pt_dir_phys` 在三方视角下不同——CPU 硬件从 CR3 加载（PA）、VM 经 `pt->pt_dir` 访问（VM 地址空间 VA）、内核经 `pagedir_mappings` 窗口访问（内核地址空间 VA，`p_cr3_v`）。这正是 07 §1.2 双视图问题在绑定操作上的体现。

**调用场景**：

| 场景 | 位置 | 说明 |
|------|------|------|
| fork | fork.c:94 | 子进程页表创建后绑定 |
| boot | main.c:355 | boot 进程页表绑定 |
| VM 自身初始化 | main.c:211 | pt_init 阶段 5 |
| VM 热更新 | main.c:717-718 | LU 交换槽位后重新绑定 |
| exec（VMPPARAM_CLEAR） | exit.c:137 | 清旧建新后绑定 |
| RS 服务重启 | rs.c:198-199 | src/dst VM 页表绑定 |

### 2.11 pt_mapkernel / freepde / pt_allocate_kernel_mapped_pagetables：内核协作三件套

**freepde（pagetable.c:1028）**：从 `kernel_boot_info.freepde_start` 递增分配 PDE 编号——这些编号是内核预留的页目录槽位，供 `pagedir_mappings` 登记册 PDE 使用。

**pt_allocate_kernel_mapped_pagetables（pagetable.c:1035）**：初始化 `pagedir_mappings[MAX_PAGEDIR_PDES]`（pagetable.c:37-43）——为每个登记册条目分配 PDE 编号（`freepde`）+ 分配一个 `page_directories` 页表（记录所有进程页目录物理地址）+ 计算该页表在页目录中的 PDE 值（`pdm->val`）。这是"内核访问任意进程页目录"登记册的基础设施。

**pt_mapkernel（pagetable.c:1442）** 三段映射：

```c
int pt_mapkernel(pt_t *pt)
{
	int kern_pde = kern_start_pde;
	phys_bytes addr, mapped = 0;

	assert(bigpage_ok);
	addr = kern_mb_mod->mod_start;

	/* 段 1：内核代码段——4MB 大页 + global_bit，PDE 级映射 */
	while(mapped < kern_size) {
		pt->pt_dir[kern_pde] = addr | ARCH_VM_PDE_PRESENT |
			ARCH_VM_BIGPAGE | ARCH_VM_PTE_RW | global_bit;
		kern_pde++;  mapped += ARCH_BIG_PAGE_SIZE;  addr += ARCH_BIG_PAGE_SIZE;
	}

	/* 段 2：pagedir_mappings 登记册 PDE */
	for(pd = 0; pd < MAX_PAGEDIR_PDES; pd++) {
		assert(pdm->pdeno > kern_pde);
		pt->pt_dir[pdm->pdeno] = pdm->val;
	}

	/* 段 3：kern_mappings 特殊映射（视频内存/APIC 等） */
	for(i = 0; i < kernmappings; i++) {
		if((r=pt_writemap(NULL, pt,
			kern_mappings[i].vir_addr, kern_mappings[i].phys_addr,
			kern_mappings[i].len, kern_mappings[i].flags, 0)) != OK)
			return r;
	}
	return OK;
}
```

要点：内核代码段是**大页 PDE 直写**（无二级页表），登记册段把 5 个 PDE 拷入，特殊映射段经 `pt_writemap` 逐条。`kern_start_pde` 在 pt_init 阶段 1 设置（pagetable.c:1114，`kernel_boot_info.vir_kern_start / ARCH_BIG_PAGE_SIZE`）。

### 2.12 pt_copy：全量复制用户区 PTE（pagetable.c:1069）

```c
static void pt_copy(pt_t *dst, pt_t *src)
{
	for(pde=0; pde < kern_start_pde; pde++) {
		if(!(src->pt_dir[pde] & ARCH_VM_PDE_PRESENT)) continue;
		assert(!(src->pt_dir[pde] & ARCH_VM_BIGPAGE));
		if(!src->pt_pt[pde]) { panic("pde %d empty\n", pde); }
		if(pt_ptalloc(dst, pde, 0) != OK)
			panic("pt_ptalloc failed");
		memcpy(dst->pt_pt[pde], src->pt_pt[pde],
			ARCH_VM_PT_ENTRIES * sizeof(*dst->pt_pt[pde]));
	}
}
```

**语义要点**：
- 遍历用户区（`pde < kern_start_pde`），对每个已存在的 PDE：`pt_ptalloc` 分配目标二级页表 + `memcpy` 整页表（4KB，1024 个 u32 PTE）。
- **仅 pt_init 阶段 6 使用**（07 §2.5 已述）：把 VM 自身页表从静态 BSS 页版本重建为纯动态版本（`pt_new(&newpt_dyn); pt_copy(&newpt_dyn, newpt); memcpy(newpt, &newpt_dyn, ...)`，pagetable.c:1324-1343）。
- 与 `pt_map_in_range` 的关系：全量版本。`memcpy` 批量是 32 位优化（PTE=u32、页表恰一页）；64 位下 PTE=u64、页表结构不同，此优化不成立（§3.5 D5）。

### 2.13 消费方全景

| 消费方 | 位置 | 调用 | 场景 |
|--------|------|------|------|
| region.c | 153/280/1139/746-751 | `pt_writemap`（WMF_VERIFY / WMF_OVERWRITE）/ `pt_checkrange` | 页错误写 PTE + 验证、region 映射、region 解映射 |
| mmap.c | 500 | `pt_writemap`（MAP_NONE + WMF_OVERWRITE\|WMF_FREE） | `munmap_vm_lin` 释放 VM 线性映射 |
| fork.c | 70/78/94 | `pt_new`/`pt_free`/`pt_bind` | fork 子进程页表 |
| exit.c | 36/135-137 | `pt_free`/`pt_new`/`pt_bind` | exit / VMPPARAM_CLEAR |
| main.c | 211-212/352-356/717-719/749 | `pt_bind`/`pt_clearmapcache`/`pt_new` | VM 自举、boot 进程、LU |
| rs.c | 198-199/251/256/263/267 | `pt_bind`/`pt_ptalloc_in_range`/`pt_ptmap` | RS 服务重启（LU） |
| utility.c | 326-333 | `pt_map_in_range` | `swap_proc_dyn_data`（LU） |
| pagefaults.c | 153 | `pt_clearmapcache` | 页错误处理 |
| pagetable.c 内部 | 244/309/425 | `pt_writemap` | `vm_freepages`/`vm_mappages`/`vm_lockpage`（06 范围） |

---

## 3. Rust 设计决策

> 本篇设计决策为 D1-D8（D 编号与设计契约快照保持一致，快照属中间产物、非正式文档引用对象，正式文档以本节为准）。每项决策的判定优先级：`Minix3 C 源码行为 > design 契约 > Rust 实现 > 文档`。

### 3.1 D1: WMF 标志族 → `map`/`remap`/`unmap`/`update_flags` 方法族

C 的 `pt_writemap` 用一个函数承载四种写模式（§2.4）；minix-rs 把它分治为 `Paging` trait 的四个方法（os/arch/src/arch/paging.rs）：

| WMF 模式 | Rust 方法 | 语义差异 |
|----------|-----------|---------|
| （无标志，严格写） | `map`（:233） | 已映射 → `AlreadyMapped`——用于"此处不应有映射" |
| `WMF_OVERWRITE` | `remap`（:256） | 原子覆盖，返回旧 `(PhysBytes, PageFlags)` |
| `WMF_WRITEFLAGSONLY` | `update_flags`（:261） | 保留物理地址只改标志 |
| `WMF_FREE` | `unmap`（:259） + 引用计数解耦 | `unmap` 返回物理地址，**不释放页** |
| `WMF_VERIFY` | `query`（:264）对比 | 无独立 verify API |

**为什么分治**：
1. **类型安全**：`map`/`remap` 的"是否允许覆盖"是调用点语义，C 用标志位表达、靠调用方自觉；Rust 用方法签名表达——fork 子表写入（严格 map）与 region 覆盖（remap）不可能混用。
2. **`remap` 的原子性**：`remap` 单次调用完成"读旧 + 写新"，避免 `unmap`+`map` 之间的无映射窗口。x86-64 PTE 写入是 8 字节自然对齐原子操作（Intel SDM Vol3 §4.10.4），单线程事件循环下逻辑原子。
3. **`WMF_FREE` 解耦**：C 把"解除映射"与"释放物理页"耦合在 `pt_writemap`；Rust 的 `unmap` 只取消映射并返回原物理地址，物理页释放由 `PageFrames` 引用计数管理（11 文档）——页表层只管映射，物理页生命周期归内存管理模块。这是职责分离：同一个物理页可能被多个 PTE 引用（CoW），"解除一个映射就释放页"在共享场景是错误的。
4. **`WMF_VERIFY` 的宽松度**：C 的 verify 有架构特定宽松（i386 掩 ACC/DIRTY、可写期望接受只读）；Rust 不内置 verify API——调用方用 `query()` 拿回 `(PhysBytes, PageFlags)` 自行比较，宽松度由调用方定义。当前无生产调用方（region 写 PTE 的验证在 Rust 侧由测试承担）。

**`MAP_NONE` 哨兵消失**：`unmap` 的"清除映射"是方法语义，不需要 `0xFFFFFFFE` 哨兵。C 的断言 `physaddr != MAP_NONE || !flags` 在 Rust 中变成 `unmap` 不接受 flags 参数——不可能写出"清除但带标志"的非法组合。

### 3.2 D2: `pt_ptalloc`/`pt_ptalloc_in_range` → `walk_alloc` 内部化

C 的"写映射前先手动预分配二级页表"在 minix-rs 中被**内化进 arch 实现**：

- `walk_alloc`（os/arch/src/x86_64/paging.rs:277）：沿 PML4→PDPT→PD→PT 逐级检查，遇到不存在的中间表就分配一页并写入上层表项（`alloc_pt_page`，06 的 `vm_pt_alloc` 供给物理页）。`map` 调用它获得 leaf PTE 地址后写入。
- **VM 层无"预分配页表"API**——`pt_ptalloc_in_range` 没有直接对应。调用方不再需要知道"这次写映射可能分配几个页表页"。

**递归副作用消失的原因**：C 的递归前提是"VM 分配物理页后必须先映射进自己的地址空间才能访问"（`vm_allocpage` → `vm_mappages` → `pt_writemap`）。Direct Map 下物理页经 `phys_to_ptr_dm`（x86_64/paging.rs:120）直接可写，分配页表页与写页表页之间没有"先映射"步骤——`walk_alloc` 分配后直接写 DM 地址，不存在"内层已分配、外层需检查"的竞争。

**C 两阶段策略在 Rust 的对应**：`map_range`（paging.rs:314）保留了"先保证可写，再动手"的精神——Phase 1 逐页 `query` 验证无冲突（对应 verify 语义），Phase 2 逐页 `map`（此时不会因 AlreadyMapped 失败，只有 AllocationFailed 可能），失败无部分映射残留（all-or-nothing）。C 的"预分配失败不回收"（§2.2）在 Rust 中变成"验证失败无副作用"。

### 3.3 D3: `pt_new`/`pt_free`/`pt_bind` → `ActiveProc` 生命周期包装 + SetAddrSpace 根登记

**VM 侧包装**（os/servers/vm/src/vmproc/vmproc_handle.rs）：

| C | Rust | 位置 | 差异 |
|----|------|------|------|
| `pt_new` | `init_page_table` | :351 | `#[cfg(not(test))]` 走 `Paging::new` + `map_kernel`；test 走 MaybeUninit stub |
| `pt_bind` | 无独立函数 | — | 第 5 步（内核通知）由 VMCTL SetAddrSpace 通道承接；步骤 1-4 登记册簿记被 Direct Map 结构性消除（D8） |
| `pt_free` | `free_page_table` | :411 | `unsafe destroy` + 重置 `vm_pt_initialized` |

**`pt_new` 的"页目录不重复分配"语义消失**：C 中页目录生命周期绑定进程槽位（§2.10）；Rust 的 `VmProcInner.vm_pt` 是 `MaybeUninit<PageTable>` + `vm_pt_initialized` 布尔状态机（vmproc.rs:35，02 文档）——`init_page_table` 每次调用都重建（先 `free_page_table` 再 init，如 exit.rs:189-190）。"不重复分配"是 32 位下避免失效 `page_directories` 映射的性能/正确性权衡，Direct Map 下内核不再持有指向页目录的映射（D8），这个约束失去存在理由。

**`pt_bind` 的归属（D7 裁决：本专项不保留任何 bind API）**：arch 层曾有 `bind_to_process` stub（仅参数校验——arch crate 依赖方向 kernel → arch，无法调用内核 IPC），VM 侧曾有 `bind_page_table` wrapper 转调它。二者是已验证的 no-op，已整体删除（死代码删除，重锚定见 07-pagetable-struct.md §4.2）。绑定的真实语义由两条既有通道分别承接：

- **普通进程**（VM 为 fork/exec 的目标进程建的页表）：`VmCtlParam::SetAddrSpace`（os/kernel/src/syscall.rs:2003）——VM 把建好的根 PA 写进目标进程 `p_seg.phys_root`，对应 `pt_bind` 第 5 步的 `sys_vmctl_set_addrspace`；
- **VM 自身**：不经 SetAddrSpace——bootstrap root 在 boot 期已登记为 VM 的根（`p_seg.phys_root = root_phys`，kernel boot 路径），VM 启动时以 A1 adoption 包装同一根（07 §3.4），之后永不换根。

C 的 `pt_bind` 五步里，步骤 1-4（写 `pagedir_mappings` 登记册 + 计算内核访问 VA）是 32 位登记册机制（D8 消除），步骤 5（通知内核换根）由 SetAddrSpace 通道原样承接——**"绑定"没有消失，是它的簿记外壳消失了**。

**`free_page_table` 的 unsafe 契约**：调用方必须保证页表未激活（无 CR3 指向它）。fork 回滚路径（fork.rs）有 9 处 SAFETY 注释逐条论证前置条件（Active typestate、无 CR3、无 live PTE、单线程）。

### 3.4 D4: `pt_mapkernel` → `map_kernel`（三段 → 两段）

`map_kernel`（os/arch/src/arch/paging.rs:494）执行两段映射：

| 段 | C `pt_mapkernel` | Rust `map_kernel` |
|----|-----------------|-------------------|
| 内核代码/数据段 | 必须（4MB 大页 PDE 直写） | 必须（逐页 `map`，`kernel_read_write`） |
| 内核 direct map | 不存在 | 必须（逐页 `map`，U/S=0） |
| `pagedir_mappings` 登记册 PDE | 必须（5 个 PDE） | **消除**（Direct Map 替代） |
| `kern_mappings` 特殊映射 | 必须（逐条 `pt_writemap`） | **消除**（设备内存经 VM_MAP_PHYS，20/21 文档） |

**消除的理由**（[ARCH: A-1]）：
- 登记册段：内核访问任意进程页目录 = `kernel_phys_to_virt(cr3_phys)`，不需要在每进程页表里预留登记册 PDE（§3.8 D8）。
- `kern_mappings` 段：内核预留映射（视频内存/APIC 等）在 minix-rs 中由 `KernelLayout`（global.rs，`VmServer::init` 设置）+ 显式设备映射路径处理，不再由每个页表继承一段"内核特殊映射"。

**只读不变量**：`map_kernel` 建立后，kernel direct map 的 PTE/PDE/PDPT 表项**不再修改**。这使 Global 位（G=1）安全——TLB 条目跨 CR3 切换存活，因为内容永不变化。这个不变量是 D6（逐条 invlpg）得以成立的前提之一：只有可变的用户区映射才需要逐条失效，不变的内核映射不需要刷新策略。

**`KernelLayout` 的 fail-fast**：`init_page_table` 在 `#[cfg(not(test))]` 路径调用 `crate::global::kernel_layout()`——若 `VmServer::init()` 未设置布局则 panic（global.rs 注释），避免"用错误的布局静默映射错误内存"。这是对 C 的"内核启动时通过 `sys_vmctl_get_mapping` 提供"机制的显式化：**布局由 VM 初始化时注入，而不是运行时向内核查询**。

### 3.5 D5: `pt_copy`/`pt_map_in_range`/`pt_ptmap` → `clone_range` + 消费方现状

**`clone_range`**（os/arch/src/arch/paging.rs:453）是三个 C 函数中前两个的统一：

```rust
pub fn clone_range<P: Paging>(
    src: &P, dst: &mut P,
    start: VirBytes, end: VirBytes,
) -> Result<(), PageTableError> {
    // 校验 start/end 页对齐（未对齐 → InvalidAddress）
    // 遍历 [start, end)：
    //   src.query(vaddr) 有映射 → dst.map(vaddr, paddr, flags)
    //   src 无映射 → 静默跳过（匹配 C pt_map_in_range）
    //   dst 已有映射 → AlreadyMapped（严格语义）
}
```

- `pt_copy` 等价 `clone_range(src, dst, 0, VM_USER_TOP)`（全量用户区）。
- `pt_map_in_range` 等价带具体 `start`/`end` 的调用。
- **为什么不是 trait 方法**：它不操作单个 PTE 的硬件位编码，而是在两个页表之间搬运已有映射——是 `query`+`map` 的组合操作，与硬件机制无关（操作分层：硬件机制层 `Paging` / VM 策略层 `map_kernel` / 跨表操作层 `clone_range`）。
- **memcpy → query+map**：C 的 `memcpy` 批量是 x86-32 优化（PTE=u32、页表恰 4KB 一页）；x86-64 下 PTE=u64、页表 4KB 装 512 项，结构不同。query+map 架构无关且正确。若未来性能关键，arch 实现可提供批量覆盖（如整 PT 页 memcpy），但这是实现优化不是 trait 接口。

**`pt_ptmap` 无直接对应**（A-1 结构性消除，不是缺口）：LU（25 文档）即使实现，新 VM 实例也经 `kernel_phys_to_virt` 直接读旧 VM 的页表页（页目录/二级页表都是物理页，DM 高半窗口可见）。C 需要 `pt_ptmap` 是因为新 VM 必须"把旧 VM 的页表结构映射进自己的地址空间"才能访问——Direct Map 下这个映射天然存在。

**消费方现状（诚实标注）**：当前 fork 路径**不消费 `clone_range`**——`do_fork`（fork.rs:186）用 `write_page_table_mappings`（vmproc_handle.rs:505）逐 region 逐页 `pt.map`。对照 C：`map_proc_copy` 逐 region 写 PTE（region.c:280，WMF_OVERWRITE）。Rust 的严格 map 在"子进程全新空表"上语义等价（空表无覆盖需求），且比 C 更早发现重复映射 bug。`clone_range` 的预期消费方是 LU（25 承接）——它服务的场景（两个已存在的页表之间复制）在 fork 中不出现（fork 是"新表 ← 老表"，且映射写由 region 元数据驱动而非 PTE 复制驱动）。**测试契约**：`clone_range` 当前 0 测试，本文档要求补齐（§5.1）。

### 3.6 D6: `pt_clearmapcache` → 逐条 invlpg（结构性消除）

`pt_clearmapcache` 在 minix-rs 中**无直接对应**（[ARCH: A-1] 的操作面后果，§1.8 已论证）：

- C：批量改 PTE 后 `sys_vmctl(SELF, VMCTL_CLEARMAPCACHE, 0)` 通知内核清缓存——因为 32 位内核的临时映射窗口（`freepdes`）可能缓存旧 PDE，且 VM 无法精确知道哪些 TLB 条目受影响。
- minix-rs：`Paging` 实现的每次 PTE 写入自带失效——`write_pte_dm`（x86_64/paging.rs:142）写后立即 `invlpg`（对 leaf 传目标 vaddr；对中间表传 0，保守 no-op）。写与失效绑定在同一函数，不存在"改完忘刷新"的窗口。

**为什么足够**：VM 是单线程事件循环，修改的页表在修改时不是当前运行页表（用户进程页表经 region 路径修改，进程本身不并发运行）；VM 自身页表（HeapArena 映射）修改后目标 vaddr 的 invlpg 立即生效。全局 `flush_tlb`/`flush_tlb_addr` 保留在 trait 中（paging.rs:288/297）供批量/特殊场景（如 `PagingWithId` 的 ASID 切换）。

### 3.7 D7: `pt_checkrange`/`pt_writable` → `query()` 组合

两个查询函数在 trait 中**没有独立对应**——2026-05 移除 `check_range`（paging.rs:369 的移除注释自述理由）：

1. `pt_checkrange` 全源码仅一个调用点且被 `#if SANITYCHECKS` 包裹（region.c:746-751）——调试期断言，不是生产 API。
2. 检查本身没有硬件优化空间——就是 `query()` 循环（PDE Present → PTE Present → 可写）。
3. VM 层需要时用 `query()` 自行组合：`query(v).is_some()` 等价 Present 检查，`flags.contains(PageFlags::WRITABLE)` 等价写权限检查。

`pt_writable` 同理：`query()` + `WRITABLE` 标志。i386/ARM 的反相逻辑（§2.6）已被 `pte_to_flags`/`flags_to_pte` 翻译层统一（07 D4）——VM 层永远看到 `WRITABLE` 正语义，不感知架构反相。

**语义改进**：C 的 `pt_checkrange` 对 bigpage PDE 会空指针崩溃（`pt->pt_pt[pde]` 为 NULL）；Rust `query` 正确处理 huge page（`WalkResult::Huge1G`/`Huge2M`，x86_64/paging.rs:510-518）。这是"移除调试 API"之外的附带收益——新实现不存在 C 的未定义行为。

**sanity 延后**：VM 侧 sanity 模块（sanity.rs:20-26 注释）的 `map_sanitycheck_pt`（验证页表映射与 region 元数据一致）延后，预期用 `query()` 循环实现（07-P2-9 承接）。当前 `verify_refcounts`（sanity.rs:39）只验证物理页引用计数，不查页表——页表层验证的消费方尚未落地，诚实标注为预期工作。

### 3.8 D8: `pagedir_mappings`/`freepde`/`kern_mappings` → 结构性消除

这是 08 最大的结构性消除（[ARCH: A-1]）：32 位"内核经登记册访问任意进程页目录"机制（§2.11）整体消失：

| C 机制 | 用途 | minix-rs 替代 |
|--------|------|--------------|
| `pagedir_mappings[MAX_PAGEDIR_PDES]`（pagetable.c:37-43） | 记录"哪些进程页目录被映射到内核地址空间" | `kernel_phys_to_virt(root_paddr)` 直接访问（DM 高半窗口） |
| `freepde`（:1028） | 从内核预留槽位分配登记册 PDE 编号 | 无（不需要预留槽位） |
| `pt_allocate_kernel_mapped_pagetables`（:1035） | 初始化登记册基础设施 | 无 |
| `pt_bind` 步骤 1-4 | 写登记册 + 计算内核访问 VA | 无（登记册簿记整体消除；步骤 5 由 SetAddrSpace 通道承接，§3.3） |
| `kern_mappings`（pt_init 阶段 3） | 内核预留特殊映射列表 | `KernelLayout` + 显式设备映射（20/21） |

**为什么是"机制替代"而非"优化"**：32 位内核空间只有 1GB，必须用登记册 + 临时窗口（`freepdes`，2×4MB 轮转）精打细算地访问物理内存；64 位地址空间放得下 kernel direct map（1GB 起，可扩展到全部物理内存），"所有物理页在固定偏移可达"取代了"临时借用窗口"。这与 draft/07 素材附录 A（createpde/freepde 的历史意义）的论证一致：**理解 32 位机制的极端限制，才能理解 64 位 Direct Map 是架构决定的方案而非可选优化**。

### 3.9 语义差异清单（C ↔ Rust 诚实标注）

| # | C 行为 | Rust 行为 | 类型 |
|---|--------|----------|------|
| 1 | `pt_writemap` 单一函数 + WMF 标志 | `map`/`remap`/`unmap`/`update_flags` 四方法 | 设计分治（D1） |
| 2 | `pt_bind` 5 步（登记册 + 通知内核） | 登记册簿记消除；步骤 5 由 VMCTL SetAddrSpace 通道承接（os/kernel/src/syscall.rs:2003） | 结构消除 + 通道承接（D3/D8） |
| 3 | `pt_new` 页目录与槽位绑定复用 | `init_page_table` 每次重建（状态机） | 设计差异（D3） |
| 4 | `pt_mapkernel` 三段（代码/登记册/特殊映射） | `map_kernel` 两段（代码/direct map） | 结构消除（D4） |
| 5 | `pt_copy`/`pt_map_in_range` memcpy 批量 | `clone_range` query+map 逐页 | 设计差异（D5，消费方 LU 25） |
| 6 | `pt_ptmap` 转移页表结构映射 | 无直接对应（DM 消除） | 结构消除（D5） |
| 7 | `pt_clearmapcache` 全局清 TLB 缓存 | 逐条 invlpg（写时绑定） | 结构消除（D6） |
| 8 | `pt_checkrange`/`pt_writable` 直查数组 | `query()` 组合（含 huge page 正确处理） | 设计差异（D7） |
| 9 | `pagedir_mappings`/`freepde`/`kern_mappings` | 无对应（kernel_phys_to_virt） | 结构消除（D8） |
| 10 | `pt_writemap` SMP VMINHIBIT 包裹 | 无（VM 单线程事件循环） | 执行模型差异 |
| 11 | `pt_ptalloc` 递归副作用检查 | 无（Direct Map 消除递归路径） | 结构消除（D2） |
| 12 | `ptestr` PTE 诊断（架构反相） | `PageFlags::Display`（统一语义） | 设计差异（D1/D7） |

类型说明：**设计分治** = 同一语义的表达方式重构；**设计差异** = 语义有等价对应但实现路径不同；**结构消除** = C 机制整体被 Direct Map/64 位替代（ARCH 标注）；**执行模型差异** = VM 单线程 vs C SMP 处理。

---

## 4. 实现详解

### 4.1 `Paging` trait 操作面：map/remap/unmap/update_flags/query

**trait 方法族**（os/arch/src/arch/paging.rs:152-427，07 D4 已述结构）：

| 方法 | 行号 | 语义 | 错误 |
|------|------|------|------|
| `map` | :260 | 严格写映射（已映射拒绝） | `InvalidAddress`（未对齐）/`AlreadyMapped`/`AllocationFailed` |
| `remap` | :283 | 原子覆盖，返回旧映射 | `InvalidAddress`/`NotMapped`（walk_read 无 leaf 表） |
| `unmap` | :286 | 清除并返回原物理地址 | `InvalidAddress`/`NotMapped` |
| `update_flags` | :288 | 保留物理地址只改标志 | `InvalidAddress`/`NotMapped` |
| `query` | :291 | 查询映射 | 返回 `Option`（无错误） |
| `root_paddr` | :293 | 页表根物理地址 | 无 |
| `map_range` | :341 | 批量映射（两阶段 all-or-nothing） | `InvalidAddress`/`AlreadyMapped`/`AllocationFailed` |
| `unmap_range` | :380 | 批量解映射 | `InvalidAddress`/`NotMapped` |

**MockPaging 实现**（paging.rs:658-865）：`BTreeMap<u64, (u64, PageFlags)>` 软件模拟，`root_phys = 0x1000 + id*0x1000`。契约测试（§5.1）同时约束 MockPaging 与 trait 契约——任何 Paging 实现（含未来真实架构）都必须通过这些行为契约测试（`run_paging_contract_*` 泛型函数 + MockPaging 实例化，paging.rs:1250-1421）。

### 4.2 x86-64：`walk_alloc` + `write_pte_dm` + TLB

**4 级 walk（os/arch/src/x86_64/paging.rs）**：

```
walk_alloc(root, vaddr)           ← map 路径：按需分配中间表
  PML4 项不存在 → alloc_pt_page + 写 PML4 项（PRESENT|WRITABLE）
  PDPT 项是 1GB 大页 → AlreadyMapped（无法装 4KB）
  PDPT 项不存在 → alloc_pt_page + 写 PDPT 项
  PD 项是 2MB 大页 → AlreadyMapped（同上）
  PD 项不存在 → alloc_pt_page + 写 PD 项
  → 返回 PT 叶表项地址（8 字节对齐）
```

- 中间表分配走 `crate::pt_alloc::alloc_pt_page`（06 的 `vm_pt_alloc` 供给物理页）；失败返回 `AllocationFailed`。
- `walk_read`（只读路径）供 `remap`/`unmap`/`update_flags`/`query` 用，返回 `WalkResult`（Leaf/Huge1G/Huge2M/NotMapped）——**huge page 在只读路径被正确识别**（query 返回大页映射），而 C 的 `pt_checkrange` 对 bigpage 会崩溃（§3.7）。

**PTE 写入 + TLB（`write_pte_dm`，paging.rs:142-152）**：

```rust
unsafe fn write_pte_dm(paddr: u64, value: u64, vaddr_for_flush: u64) {
    core::ptr::write_volatile(phys_to_ptr_dm(paddr), value);
    asm!("invlpg [{}]", in(reg) vaddr_for_flush, ...);
}
```

- 写目标 = `phys_to_ptr_dm(paddr)`（Direct Map 高半窗口，07 D2）——写页表页无需先映射。
- `vaddr_for_flush`：leaf PTE 传被修改的虚拟地址（invlpg 精确失效）；中间表传 0（无 leaf TLB 条目，保守 no-op，paging.rs:139-140 注释）。
- 每个 PTE 修改操作（map/remap/unmap/update_flags）都经 `write_pte_dm` 完成——**写与失效绑定**（D6）。

**对齐与索引**：map/remap 检查 `vaddr.0 & 0xFFF != 0` 返回 `InvalidAddress`；索引函数 `pml4_index`/`pdpt_index`/`pd_index`/`pt_index`（x86_64/paging.rs，07 D4 已述）按 9 位切分 48 位地址。

### 4.3 独立函数：`clone_range`/`map_kernel` + 覆盖建立通道

**`clone_range`**（paging.rs:453，§3.5 已述）——跨页表组合操作。实现要点：
- start/end 页对齐校验（未对齐 `InvalidAddress`）；
- `while vaddr < end` 逐页 `src.query` → `dst.map`；src 无映射静默跳过；dst 已映射 `AlreadyMapped`；
- `checked_add` 防溢出。

**`map_kernel`**（paging.rs:494，§3.4 已述）——两段映射（内核代码/数据段 + direct map）。注意实现细节：当前 `map_kernel` 对内核代码/数据段都用 `kernel_read_write()`（rw 而非 rx）——C 的 i386 用 `ARCH_VM_PTE_RW` 大页（也是 rw）。W^X 分离（代码段只读可执行）是未来改进点，接口签名（`kernel_text_pages`/`kernel_data_pages` 分开）已为此预留。

**`pt_init` 没有单函数对应物，其操作面语义由三个通道分别落地**：
- **覆盖建立**（kernel boot 期）：`establish_boot_dm`（os/kernel/src/dm_coverage.rs:66）在 bootstrap root 上建立 kernel DM + VM DM 双窗口——候选两源并集、资格过滤、资源包含性粒度选择，全部细节 07 §4.4 已述；
- **地址空间接管**（VM 启动期）：`VmSelfPageTable::adopt`（vm_self_map.rs:95）经 `Paging::adopt_active_root`（paging.rs:229）包装 handoff 根——A1 adoption，无拷贝；
- **根登记**（fork/exec 运行期）：`VmCtlParam::SetAddrSpace`（os/kernel/src/syscall.rs:2003）承接 `pt_bind` 第 5 步。

C `pt_init` 的"继承 + 登记 + 绑定"三段在 minix-rs 里不再是同一时间点的三个连续步骤：覆盖在 kernel boot 就绪，接管在 VM 启动，登记只在根变化时发生（VM 自身根永不变化）。

### 4.4 VM 侧：`ActiveProc` 页表生命周期（vmproc_handle.rs）

**`init_page_table`**（:351）——对应 `pt_new`+`pt_mapkernel`：

```rust
pub(crate) fn init_page_table(&mut self) -> Result<(), PageTableError> {
    #[cfg(test)]
    { /* MaybeUninit::zeroed() stub —— mock 物理内存不可访问 */ }

    #[cfg(not(test))]
    {
        let mut pt = <PageTable as Paging>::new()?;
        let layout = crate::global::kernel_layout();   // 未设置则 panic（fail-fast）
        map_kernel(&mut pt, layout.kernel_text_vbase, ..., layout.dm_pages)?;
        self.inner.vm_pt.write(pt);
        self.inner.vm_pt_initialized = true;
        Ok(())
    }
}
```

- test 构建走零初始化 stub（`MaybeUninit::zeroed()`，`root_paddr=0`）——因为 test 的 `CurrentPaging` 是 MockPaging，但 VM 测试基建不建真实页表（fork/exit/mmap 测试都注释"Skip init_page_table"避免访问 mock 物理内存）。
- 生产路径 `PageTable::new()`（分配根页）→ `map_kernel`（KernelLayout 提供布局）→ 写入 `vm_pt`。新建的根在 `sys_fork`/SetAddrSpace 时经 VMCTL 通道登记给内核（§3.3）——`init_page_table` 本身只建树，不做内核通知。

**`free_page_table`**（:411）：`unsafe { self.inner.vm_pt.assume_init_mut().destroy(); }` + 重置 `vm_pt_initialized=false`。对应 `pt_free`（但 `destroy` 释放整个页表含根页——C 的 `pt_free` 只释放二级页表，差异源于"页目录不复用"的 Rust 语义，§3.3）。

**`write_page_table_mappings`**（:515，fork 用）——对应 C `map_proc_copy` 的 PTE 写入部分：

```rust
pub(crate) unsafe fn write_page_table_mappings(
    &mut self, frames: &PageFrames,
) -> Result<(), PageTableError> {
    // 遍历全部 region 的 physblocks（PageSlot）：
    //   vaddr = region.vaddr + i * PAGE_SIZE
    //   paddr = frames.pfn_to_phys(slot.pfn)
    //   writable = region.is_writable() && refcount == 1
    //   flags = writable ? read_write() : read_only()
    // 收集后统一写入：pt.map(vaddr, paddr, flags)
}
```

- **writable 判定**：`region.is_writable() && frames.get(slot.pfn).refcount == 1`——refcount > 1 说明共享（CoW 待分裂），先映射为只读。对应 C `map_proc_copy` 的 CoW 写保护（17 文档详述）。
- 用严格 `pt.map`（非 remap）：子进程页表全新，空表上 map 等价 C 的 WMF_OVERWRITE（§3.5）。
- 两阶段（先收集后写入）：收集阶段借用 `regions_mut()`，写入阶段借用 `page_table_mut()`——避免同时持有两个 `&mut`（单线程下无别名冲突，但借用检查器需要分离）。

### 4.5 消费链：fork / exit / munmap / heap_arena

**fork**（os/servers/vm/src/fork.rs:183 `do_fork`）：

```
child.init_page_table()          (pt_new + map_kernel 对应)
child.init_regions()
fork_regions(&parent_regions)    (复制 region 元数据 + 物理页引用，18 文档)
child.setup_cow_for_all_regions  (标记 CoW：refcount>1 的页写保护)
child.write_page_table_mappings  (逐 region 写 PTE，§4.4)
失败回滚：free_page_table()（9 处 SAFETY 注释）
```

`write_page_table_mappings` 成功后即调用 `sys_fork`——**sys_fork 之前是最后一个可恢复点**：之后内核已提交子进程，回滚不再可能。C 的 `pt_bind` 步骤（fork.c:94）在 Rust 中由 `sys_fork` 的内核侧地址空间登记承担（A1/SetAddrSpace 语义，§3.3）——不存在独立的 bind 步骤。

**exit / VMPPARAM_CLEAR**（os/servers/vm/src/exit.rs:164 `handle_procctl_clear`）：

```
free_process_phys → regions.clear() → free_page_table() → init_page_table()
   （对应 C exit.c:36 pt_free → exit.c:135-137 pt_new + pt_bind）
```

C 序列以 `pt_bind` 收尾（exit.c:137）——其内容是 `sys_vmctl_set_addrspace` 根通知加 i386 登记册簿记；Rust 版没有独立 bind 步骤，内核侧重登记由 VMCTL SetAddrSpace 通道承载（exit.rs:193-197 注释，§3.3）。

**munmap**（os/servers/vm/src/munmap.rs:111）：`vm_self_unmappages(addr, pages)`——VM 自身线性映射批量解映射（对应 `munmap_vm_lin` 的 `pt_writemap(MAP_NONE, WMF_OVERWRITE|WMF_FREE)`，21 文档）。

**heap_arena**（os/servers/vm/src/heap_arena.rs:113/118/161）：`vm_self_mappages`（映射堆页）+ `vm_self_unmap`（回滚/收缩解映射，返回物理地址供释放）——09 文档详述。

### 4.6 消费方接线表

| Rust 调用点 | 位置 | C 对应 | 说明 |
|------------|------|--------|------|
| `init_page_table` | vmproc_handle.rs:351 | `pt_new` + `pt_mapkernel`（exit.c:135、fork.c:70、main.c:352） | 生命周期创建 |
| `free_page_table` | vmproc_handle.rs:411 | `pt_free`（exit.c:36、fork.c:78） | 生命周期释放 |
| `write_page_table_mappings` | vmproc_handle.rs:515 | `map_proc_copy` 的 PTE 写入（region.c:280） | fork 映射复制（18 文档） |
| `vm_self_mappages`/`vm_self_unmap` | vm_self_map.rs:203/219 | `pt_writemap`（vm_mappages 路径） | HeapArena 堆映射（09） |
| `vm_self_unmappages` | vm_self_map.rs:242 | `pt_writemap(MAP_NONE)`（mmap.c:500） | munmap（21） |
| `map`/`remap`/`unmap`/`update_flags`/`query` | paging.rs trait | `pt_writemap`/`pt_checkrange`/`pt_writable` | 硬件机制层（未来 region 路径消费） |
| `clone_range` | paging.rs:453 | `pt_copy`/`pt_map_in_range` | 跨表复制（LU 25 消费，当前无消费方） |
| `map_kernel` | paging.rs:494 | `pt_mapkernel` | 每进程内核映射（init_page_table 调用） |
| `VmSelfPageTable::adopt` | vm_self_map.rs:95 | `pt_init` 的接管语义（07 §3.4） | A1 adoption：VM 包装 bootstrap root，无拷贝 |
| `VmCtlParam::SetAddrSpace` | syscall.rs:2003（kernel） | `pt_bind` step 5（pagetable.c:1421） | 内核侧根登记（VM 为其它进程换根） |
| `establish_boot_dm` | kernel/src/dm_coverage.rs:66 | `pt_init` 的覆盖语义 | kernel boot 期双 DM 窗口建立（07 §4.4） |

---

## 5. 测试要点

### 5.1 单元测试清单

**os/arch 侧（`cargo test -p minix-arch --lib`）**：

| 测试 | 位置 | 验证目标 |
|------|------|---------|
| MockPaging 操作契约 14 个（`test_mock_map_unmap`/`test_mock_double_map`/`test_mock_unmapped`/`test_mock_update_flags`/`test_mock_remap_new`/`test_mock_remap_replace`/`test_mock_map_range`/`test_mock_unmap_range`/`test_mock_map_range_all_or_nothing`/`test_mock_alignment_check`/`test_mock_root_paddr`/`test_mock_switch` 等）+ page_flags 4 个 + map_kernel/huge/ASID 5 个 | os/arch/src/arch/paging.rs mock tests | map/remap/unmap/update_flags/query/map_range/unmap_range 行为契约 |
| 泛型契约 10 个（`test_paging_contract_*`，`run_paging_contract_*` 泛型函数） | paging.rs:1250-1421 | 任意 Paging 实现必须满足的 trait 行为契约 |
| x86_64/paging.rs 11 个（flag roundtrip/NX/索引/huge/walk） | os/arch/src/x86_64/paging.rs | PTE 翻译、4 级索引、huge page、零根 walk |
| **`clone_range` 4 个（新增，本文档补）** | paging.rs mock tests | 跨页表复制：基本复制/跳过缺失/AlreadyMapped/对齐校验 |

**os/servers/vm 侧（`cargo test -p minix-vm --lib`）**：

| 测试 | 位置 | 验证目标 |
|------|------|---------|
| fork 集成 8 个（`test_fork_region_basic`/`test_cow_copy_page`/`test_fork_rollback_on_ev_reference_error`/`test_fork_regions_rollback_on_failure`/`test_handle_memory_once_*`） | fork.rs:481-707 | init_page_table → setup_cow → write_page_table_mappings 链 + 回滚 |
| exit VMPPARAM_CLEAR | exit.rs（test 模式 stub 页表） | free → init → bind 链 |
| pagetable/mod.rs 2 个（`test_page_align`/`test_page_size_from_trait`） | pagetable/mod.rs | 页对齐 + PAGE_SIZE |
| vm_self_map 1 个（`test_vm_self_pt_not_initialized_by_default`） | vm_self_map.rs | 静态存储初始 None |

### 5.2 覆盖维度

- **WMF 语义分治**：map（严格）/remap（覆盖）/update_flags（只改标志）/unmap（清除返回 PA）各方法 + 错误路径（AlreadyMapped/NotMapped/InvalidAddress）——D1 的行为契约。
- **两阶段写映射**：`map_range` all-or-nothing（预映射一页 → 整个范围失败 → 无部分残留）——D2 的 C 两阶段策略对应。
- **跨页表复制**：`clone_range` 基本复制/跳过缺失/AlreadyMapped/对齐校验——D5 的契约（**本文档补测试后覆盖**）。
- **生命周期链**：fork（init→setup_cow→write mappings→回滚）与 clear（free→init，C 的 pt_bind 终步无对应调用，§3.3）——D3 的 VM 侧消费链。
- **PTE 翻译**：x86_64 flag roundtrip（含 NX 反相）/索引边界/huge page——D7 的架构翻译层。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 说明 | 状态 |
|------|------|------|
| `clone_range` 生产消费方 | 当前 fork 用 write_page_table_mappings；预期消费方 LU（25） | 25 承接（§3.5） |
| `map_sanitycheck_pt` | 页表-元数据一致性 sanity 延后（sanity.rs:20-26 注释） | 07-P2-9 承接 |
| `pt_ptmap` 直接对应 | 结构性消除（A-1），LU 25 经 kernel_phys_to_virt 访问 | 25 承接（§3.5） |
| VMCTL_FLUSHTLB 全局刷新 | 逐条 invlpg 替代；`flush_tlb` 保留供特殊场景 | 接受（§3.6） |
| `map_kernel` W^X 分离 | 当前内核代码/数据段均 kernel_read_write；C 的 i386 也是 rw | 未来改进（§4.3） |
| 生产路径（X86_64Paging + invlpg） | 真实 DM + TLB 依赖 QEMU 集成 | 06 已注，backlog（QEMU） |

### 5.4 测试统计（截至 2026-08-15）

- `cargo test -p minix-vm --lib`：**347 passed / 1 failed**（`region::vir_region::tests::test_map_lazy`，13 范围 pre-existing，plan §3.5 基线一致）。
- `cargo test -p minix-arch --lib`：**176 passed**（含 08 新增 `clone_range` 4 个）。
- 本文档范围 Rust 测试：paging.rs 37（mock 操作 14 + page_flags 4 + map_kernel/huge/ASID 5 + 泛型契约 10 + **clone_range 4 新增**）+ x86_64 11 + VM 侧 16（pagetable 2 + vm_self_map 1 + fork 8 + exit 5）= **64 个**。
- 统计规则：不引用具体测试文件行号（避免行号漂移传播，Pattern #66 RCPD 主动应用）。

---

## 6. 过渡

本文档在启动时序中的位置：`init_vm()` → `pt_init()`（07 结构面）之后、各类服务路径（fork/mmap/pagefault/exit）运行时。它把 07 的页表结构接进 09 的堆自举：

```
05（物理分配器）→ 06（VM 自用页分配）→ 07（页表结构）→ 08（页表操作）→ 09（堆自举）
                                                              │
                                                  vm_self_mappages/vm_self_unmap（HeapArena 消费）
```

**下一篇入口**：09-slab-allocator 承接 HeapArena + VmAllocator——VM 的 Rust 堆映射物理页到连续 VA 区间，逐页消费本文档的 `vm_self_mappages`/`vm_self_unmap`（HeapArena::grow 的 PTE 写入路径已在 §4.5 接线）。08 是"页表操作 API 的语义定义"，09 是"VM 自身堆如何经这些 API 自举"——前者回答"怎么改页表"，后者回答"VM 自己怎么用改页表来活着"。

**向后衔接的服务文档**：18-vm-fork（`write_page_table_mappings` 消费链）、22-vm-exit（`handle_procctl_clear` 的 free→init）、21-vm-munmap（`vm_self_unmappages`）、25-rs-services（`clone_range` 的 LU 消费）。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/07-pagetable-struct.md` — 页表结构、Direct Map、`Paging` trait 结构（上一篇）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/06-page-allocator.md` — 页分配 + `vm_pt_alloc` 供给页表页（`walk_alloc` 依赖）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/09-slab-allocator.md` — HeapArena 消费 `vm_self_mappages`/`vm_self_unmap`（下一篇）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/18-vm-fork.md`、`22-vm-exit.md`、`21-vm-munmap.md`、`25-rs-services.md` — 页表操作的消费方（fork/exit/munmap/LU）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md` — `init_vm`/`pt_init` 调用点
- `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/07-cross-space-init.md` — Direct Map 双窗口论证（kernel 侧）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/07-pagetable-ops.md` — 旧主线素材（素材，含附录 A createpde/freepde 历史意义）
- `minix3/minix/servers/vm/pagetable.c`、`minix3/minix/servers/vm/vm.h`、`minix3/minix/servers/vm/region.c`、`minix3/minix/servers/vm/fork.c`、`minix3/minix/servers/vm/exit.c`、`minix3/minix/servers/vm/rs.c`、`minix3/minix/servers/vm/utility.c`、`minix3/minix/servers/vm/mmap.c`、`minix3/minix/servers/vm/main.c`、`minix3/minix/servers/vm/pagefaults.c` — C 源码（ground truth）
- `os/arch/src/arch/paging.rs`、`os/arch/src/x86_64/paging.rs`、`os/arch/src/arm64/paging.rs`、`os/arch/src/riscv64/paging.rs` — Rust 实现（trait 操作面 + 三架构 walk）
- `os/servers/vm/src/vmproc/vmproc_handle.rs`、`os/servers/vm/src/fork.rs`、`os/servers/vm/src/exit.rs`、`os/servers/vm/src/munmap.rs`、`os/servers/vm/src/heap_arena.rs`、`os/servers/vm/src/pagetable/mod.rs`、`os/servers/vm/src/pagetable/vm_self_map.rs` — Rust 实现（VM 侧）
