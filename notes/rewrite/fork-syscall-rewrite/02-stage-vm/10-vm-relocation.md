# 10-vm-relocation: VM 自举的终点——从静态/临时分配到动态稳态

> **分类**: 阶段 4 — 自举的堆与元数据（自举终点）
> **源码**: `minix3/minix/servers/vm/pagetable.c`（`pt_init` 搬迁段 :1311-1345 / `pt_init_done` :328 / spare page 池 :59-110、:1116-1162 / `vm_allocpages` :333-394 / `vm_freepages` :235-259 / `is_staticaddr` :85）+ `minix3/minix/servers/vm/alloc.c`（`reservedqueue_*` :60-237 / `missing_spares` :74 / `alloc_cycle` :227-237）+ `minix3/minix/servers/vm/utility.c`（`swap_proc_slot` :188 / `transfer_mmap_regions` :228 / `map_proc_dyn_data` :283 / `swap_proc_dyn_data` :312）+ `minix3/minix/servers/vm/region.c`（`map_setparent` :1535）+ `minix3/minix/servers/vm/main.c`（主循环 `alloc_cycle` 钩子 :118-119）
> **Rust 模块**: `os/servers/vm/src/vm_server.rs`（`VmServer::relocate` :182 / `mark_alloc_failure` :396）+ `os/servers/vm/src/global.rs`（`heap_arena_grow` :481）+ `os/servers/vm/src/phys_mem/mod.rs`（`PhysAlloc`/`as_bitmap` :195）+ `os/servers/vm/src/phys_mem/bitmap_alloc.rs`（`metadata_pa_range` :110 / `available_regions` :464）+ `os/servers/vm/src/vmproc/vmproc_handle.rs`（`swap_proc_slot` :620）+ `os/servers/vm/src/rs.rs`（LU 支撑面 DEFERRED :210/:229）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/05-physical-memory.md`（物理页分配器）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/06-page-allocator.md`（页分配器 + `missing_spares` 重解释）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/07-pagetable-struct.md`（页表结构 + Direct Map）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/08-pagetable-ops.md`（页表操作面）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/09-slab-allocator.md`（HeapArena）
> **说明**: VM 自举终点语义模块：**Minix3 的初始化数据搬迁（spare page 池 + 页表结构，pagetable.c:1311-1345）+ Live Update 支撑面（swap_proc_slot / transfer_mmap_regions / map_proc_dyn_data / swap_proc_dyn_data / map_setparent）**。**不覆盖**：物理页分配器（05）、页分配器与保留页池（06）、页表结构/操作（07/08）、堆分配器（09）、RS Live Update 服务流程（25）。

---

## 1. 概念：自举到稳态的转换机制

### 1.0 章节引言

VM server 自举的本质是解决一个**鸡生蛋问题**：物理内存管理器需要元数据（bitmap）来跟踪哪些页空闲，而元数据本身需要内存；VM 的页表和堆分配器在早期又不可用。所有操作系统内核都靠"先用一个临时机制撑过自举期，再切换到正式机制"来打破这个循环。本文档回答三个问题：

1. **为什么自举机制不能长期使用**——静态/临时分配的大小固定、物理地址不可控（§1.1）。
2. **Minix3 如何搬迁**——spare page 池 + 页表结构从 BSS 静态切换到动态（§1.3-§1.4）。
3. **minix-rs 如何搬迁**——分配器元数据从 BumpBuf 语义化迁移到 HeapArena（§1.6-§1.7）。

它在启动时序中的位置：

```
init_vm()（main.c:428）
  ├─ sys_getkinfo / get_mem_chunks / memset(vmproc) ...      ← 01/02/03/05
  ├─ acl_init() / map_region_init()                          ← 04/13
  ├─ mem_init(mem_chunks)                                    ← 05：物理内存布局
  ├─ init_proc(VM_PROC_NR) + pt_init()                       ← 02/07/08：VM 自身页表就绪
  │    └─ pt_init_done = 1（pagetable.c:1311）→ ★ 本文档：搬迁段
  ├─ __minix_init()（main.c:480）                            ← 09：堆可用分界线
  ├─ mem_add_total_pages()（main.c:485-495）                 ← 05：总页数校准
  ├─ exec_bootproc() / CALLMAP / sef_local_startup()         ← 01/15
```

Rust 侧对应位置是 `VmServer::init()` 开头的 `relocate()`（vm_server.rs:182），在 `init_global_state()` 之前执行——语义上对应 C 的 `pt_init()` 搬迁段（§1.6）。

### 1.1 为什么自举机制不能长期使用

Minix3 的自举元数据在 BSS 段（`static_sparepages[]`、`free_pages_bitmap[]`、`free_page_cache[]`），由内核加载 ELF 时映射。BSS 静态分配有两个根本性约束：

1. **大小固定**——编译时确定，运行时无法增长。spare page 池只有 `STATIC_SPAREPAGES` 页（i386 上 15，见 pagetable.c:59-68），页表需要增长时静态分配无法满足。
2. **物理地址不可控**——由内核在加载 ELF 时决定。Live Update 后新 VM 实例有新的地址空间，旧实例的 VA/PA 全部失效。

pagetable.c:1313-1316 的注释直接说明了搬迁动机：

```c
/* VM is now fully functional in that it can dynamically allocate memory
 * for itself.
 *
 * We don't want to keep using the bootstrap statically allocated spare
 * pages though, as the physical addresses will change on liveupdate. So we
 * re-do part of the initialization now with purely dynamically allocated
 * memory. First throw out the static pool.
```

**搬迁（relocation）** 就是指：在 `pt_init()` 末尾把 spare page 池和页表结构从静态/BSS 切换到动态分配，让 VM 的页表基础设施完全由动态内存支撑。搬迁完成后，VM 不再依赖内核加载时映射的任何特殊静态资源。

Rust 侧对应约束是 **BumpBuf 的连续物理页要求**：自举阶段元数据从 Direct Map 范围内的第一个足够大的 free region 分配（05 §3.4），由于 `VA = PA + BASE`，BumpBuf 强制要求**连续物理页**。连续 PA 是稀缺资源（DMA 等场景需要），搬迁到 HeapArena（碎片化 PA + 连续 VA）后释放旧连续页，既消除了约束又回收了稀缺资源。

### 1.2 搬迁的两种对象：页表结构（C）vs 分配器元数据（Rust）

**Minix3 搬迁页表结构**。spare page 池的 slot 结构（`{phys, vir}`）包含 VA 指针，liveupdate 后 VA 失效，必须切换到动态分配；页目录/页表页本身在初始化时用静态 spare page 构建，也需要重建为动态页。

**minix-rs 不搬迁页表结构**。Direct Map（ARCH A-1，07 §3.2）提供稳定 VA（`VM_DIRECT_MAP_BASE + phys`），页表页分配从 T3 起单路径可用（`alloc_page::vm_pt_alloc`，06 §3.3）——C 的"liveupdate 后 PA 失效所以页表要重建"问题在 minix-rs 结构性消失。Rust 的搬迁对象是**分配器元数据**：bitmap 等元数据在自举期占据 BumpBuf 连续 PA，搬迁到 HeapArena（碎片化 PA + 连续 VA）后释放旧连续页。

两者目标一致：**消除自举阶段的临时约束，让 VM 完全由动态分配支撑**。

### 1.3 Minix3 的搬迁流程（pagetable.c:1317-1345）

搬迁分三步（详细逐行见 §2.1）：

1. **用光静态页，动态回填**：
   - `alloc_cycle()`（pagetable.c:1317）——确保分配可用（主循环钩子 main.c:118-119 的同一函数，见 §2.3）。
   - `while(vm_getsparepage(&phys));`（:1319）——从备用队列取出全部静态页"用光"（静态页无法被 `vm_freepages` 释放，`is_staticaddr` 检查会跳过，所以只能取走不归还）。
   - `alloc_cycle()`（:1321）——用 `alloc_mem()` 动态分配新页回填队列。
2. **重分配内核映射页表**：`pt_allocate_kernel_mapped_pagetables()`（:1322）+ `pt_bind`/`pt_mapkernel`（:1323-1324）+ FLUSHTLB（:1326-1329）。
3. **重建 VM 页表**：`pt_new(&newpt_dyn)` + `pt_copy` + `memcpy`（:1331-1336）替换为纯动态页表，再 `pt_bind`/`pt_mapkernel`（:1337-1338）+ FLUSHTLB（:1340-1343）。

`pt_init_done = 1`（pagetable.c:1311）是搬迁的前置阶段切换：此后 `vm_allocpages` 走动态分配路径（§1.4）。

### 1.4 阶段切换：pt_init_done 与双路径分配

`pt_init_done`（pagetable.c:328，:1311 置 1）是自举/运行两阶段的切换标志。`vm_allocpages`（pagetable.c:333-394）按它分流：

```c
static int pt_init_done;               /* pagetable.c:328 */
...
if((level > 1) || !pt_init_done) {     /* pagetable.c:352 */
    void *s;
    if(pages == 1) s=vm_getsparepage(phys);
    else if(pages == 4) s=vm_getsparepagedir(phys);
    ...
}
```

- **自举阶段**（`!pt_init_done`）：从 spare page 池取页（`vm_getsparepage`/`vm_getsparepagedir`）——不依赖 `alloc_mem()`，避免"分配页表页需要先映射页表"的循环依赖。
- **运行阶段**（`pt_init_done`）：走 `alloc_mem()` + `vm_mappages()` 动态路径。
- **`level` 计数器**（:335）：限制递归深度 ≤ 2。`vm_allocpage()` 可能递归调用自身（`pt_ptalloc` → `vm_allocpage` → `vm_mappages` → `pt_writemap` → `pt_ptalloc`），`level > 1` 时强制走 spare 池打破递归。

`is_staticaddr(v)`（pagetable.c:85，`v < VM_OWN_HEAPSTART`）区分静态/动态地址：`vm_freepages`（:235-259）对静态地址打印告警并跳过释放——静态页不属于动态分配，无法回收。

### 1.5 备用页池机制（reservedqueue_*）

备用页池是 C 侧打破自举循环依赖的机制（pagetable.c:55-57 注释 "to avoid a circular dependency on allocating memory and writing it into VM's page table"）。数据结构与操作：

```c
static struct reserved_pages {
    struct reserved_pages *next;    /* next in use */
    int max_available;              /* queue depth use, 0 if not in use at all */
    int npages;                     /* number of consecutive pages */
    int mappedin;                   /* must reserved pages also be mapped? */
    int n_available;                /* number of queue entries */
    int allocflags;                 /* allocflags for alloc_mem */
    struct reserved_pageslot {
        phys_bytes phys;
        void *vir;
    } slots[MAXRESERVEDPAGES];
    u32_t magic;
} reservedqueues[MAXRESERVEDQUEUES], *first_reserved_inuse = NULL;
```

- `missing_spares`（alloc.c:74）是全局计数：所有队列的空缺 slot 总数。`reservedqueue_new` 增加 `max_available`、`reservedqueue_fillslot` 递减、`reservedqueue_alloc` 递增——**守恒量**。
- `reservedqueue_addslot`（:148-177）：`alloc_mem` 分配物理页 → `mappedin` 时 `vm_mappages` 映射 → `fillslot` 填入。
- `reservedqueue_fill`（:191-203）：循环填充到 `max_available`。
- `reservedqueue_alloc`（:206-225）：**LIFO**——从 `slots[n_available-1]` 取出，递减 `n_available`，递增 `missing_spares`。
- `alloc_cycle`（:227-237）：遍历 `first_reserved_inuse` 链，对每个有空缺的队列 `reservedqueue_fill`。主循环在 `missing_spares > 0` 时调用（main.c:118-119）。

**关键洞察**：备用页池是**自举机制**，不是稳态供应——它的唯一目的是在 `alloc_mem` 不可用/递归路径上提供页。`pt_init` 末尾的搬迁（"用光静态页 + 动态回填"）证明：一旦动态路径可用，池子就被动态页接管。minix-rs 中这个循环依赖被 Direct Map 结构性打破（§1.7 D4）。

### 1.6 minix-rs 的搬迁：relocate()

Rust 侧搬迁在 `VmServer::init()` 开头自动执行（vm_server.rs:182，`#[cfg(not(test))]` 门控 :257）：

```
relocate()（vm_server.rs:182）
  ├─ 1. as_bitmap() 断言自举分配器是 Bitmap + metadata_pa_range() 取旧元数据 PA（:184-189）
  ├─ 2. choose_allocator_type(total_pages) 选目标类型（:191）
  │      → total_pages > BUDDY_THRESHOLD_PAGES(1<<20) 且启用 buddy_alloc → Buddy，否则 Bitmap
  ├─ 3. heap_arena_grow(pages) 分配新元数据 VA（:195-198）
  │      → 逐页从旧分配器分配物理页 + vm_self_mappages 映射为连续 VA
  ├─ 4. available_regions() 收集旧分配器全部空闲区域（:206-211）
  │      → 新元数据占用的页已被自动排除
  ├─ 5. init() 语义化重建新分配器（:213-228）
  ├─ 6. *phys_alloc = new_alloc 整体替换（:231）
  └─ 7. free_mem(old_pa, old_pa_pages) 释放旧连续 PA 页（:232-235）
```

与 C 的对应关系：C 搬迁**页表结构**（重建 newpt_dyn），Rust 搬迁**分配器元数据**（重建 bitmap）——两者都是"重新分配 + 语义化复制 + 替换结构 + 释放旧资源"。Rust 不需要重建页表（§1.2）。

### 1.7 ARCH 标注：为什么 Rust 侧不实现备用页池

**`[ARCH: A-1]`（Direct Map）结构消除**（plan.md §4 A-1，06 §3.3 决策）：`reservedqueue_*`（alloc.c:60-237）在 minix-rs **不实现**。依据：

- C 备用页池的唯一目的是打破自举循环依赖（pagetable.c:55-57 注释），且 `pt_init` 末尾被整体替换为动态页（pagetable.c:1311-1345）——它是自举机制，不是稳态供应。
- minix-rs 中 VA 由 Direct Map 常量偏移给出（`VM_DIRECT_MAP_BASE + phys`），页表页分配（`alloc_page::vm_pt_alloc`，注册进 `minix_arch::pt_alloc`）从 T3 起单路径可用——循环依赖被结构性打破，`level` 计数器、`pt_init_done` 阶段切换、BSS 静态页全部消失。
- 保留的语义：`missing_spares`（alloc.c:74）在 Rust 中重解释为**分配压力计数**（`VmServer::mark_alloc_failure`，vm_server.rs:396），主循环 `alloc_cycle` 钩子（main.c:118-119）保留为补充/回收机会（体 DEFERRED 归 24-page-cache）。

代码注释标注位置：`vm_server.rs:53-61`（mark_alloc_failure 注释，见 §4.4）。

### 1.8 Live Update 支撑面

搬迁的另一个消费者是 Live Update（LU）：RS 在更新 VM/其他服务时，需要把旧实例的动态数据搬到新实例。Minix3 的支撑面在 utility.c：

| 函数 | 位置 | 语义 |
|------|------|------|
| `swap_proc_slot` | utility.c:188 | 交换两个 vmproc 槽的全部内容，**保留各自的 endpoint 和 slot**（客户端仍通过原 endpoint 访问，但获得新实例的内存状态） |
| `transfer_mmap_regions` | utility.c:228 | 把源进程 `[start_addr, end_addr)` 范围内的 mmap 区域以 **CoW 共享**方式复制到目标进程；幂等（目标 base 地址已存在则跳过） |
| `map_proc_dyn_data` | utility.c:283 | 对 `[VM_MMAPBASE, VM_MMAPTOP)` + `[VM_STACKTOP, VM_DATATOP)` 两段调用 `transfer_mmap_regions`——共享全部动态 mmap 区域 |
| `swap_proc_dyn_data` | utility.c:312 | VM 自身：先 `pt_map_in_range` 转移堆/栈区域（`VM_OWN_HEAPBASE..VM_OWN_MMAPTOP` + `VM_STACKTOP..VM_DATATOP`）；然后 `map_setparent` 交换区域父指针；非 VM 且无 `SF_VM_ROLLBACK|SF_VM_NOMMAP` 时反向 `map_proc_dyn_data(dst, src)` |
| `map_setparent` | region.c:1535 | 把 vir_region 的 `parent` 指向自己——区域所有权切换到新实例，旧实例不再拥有这些区域 |

Rust 侧状态：`swap_proc_slot` **已实现**（§4.3，typestate 版本）；`transfer_mmap_regions`/`map_proc_dyn_data`/`swap_proc_dyn_data`/`map_setparent` **DEFERRED**（rs.rs:210/:229，诚实标注，见 §4.5）——RS UPDATE 流程（25）消费它们，当前 RS_PREPARE 部分实现、RS_UPDATE 返回 NotImplemented（fail-closed，ARCH A-8）。

### 1.9 对照 Redox / Linux

**Redox**：Redox 内核早期用 `linked_list_allocator` 的 bump/空链表分配器 + 固定帧分配器（`BumpAllocator`/`BuddyAllocator`）管物理帧，恒等映射 + 直接映射提供稳定 VA——自举后**不需要**像 Minix3 那样重建页表结构（无 liveupdate 的"PA 变化"问题）。这与 minix-rs 同构：Direct Map 消除页表重建需求，free-list 分配器（09 `VmAllocator`，A-3 v2 与 `linked_list_allocator` 同形态）对应 Redox 的 `linked_list_allocator::Heap`，物理帧分配器（05/06）对应 Redox `FrameAllocator`。Redox 的上下文切换/进程替换不涉及 VM 侧"页表搬迁"。

**Linux**：`memblock`（早期物理内存跟踪，自举期用）→ `paging_init`/`memblock_free_all`（把 memblock 的空闲区域移交 buddy 分配器）是经典的"先静态后动态"迁移——与 Minix3 的"先 BSS 后动态"、minix-rs 的"先 BumpBuf 后 HeapArena"同构。Linux `kexec`/kpatch 与 Minix3 LU 无直接对应：kexec 是整内核重启加载，kpatch 是函数级热补丁；Minix3 LU 是**进程级热替换**（整服务重启 + 状态转移），minix-rs 用 typestate `swap_proc_slot` + 区域 parent 重定向实现其核心原语。

### 1.10 本章小结

- 自举机制（BSS/BumpBuf）有两个约束：大小固定 + 物理地址不可控（连续 PA 稀缺）。
- Minix3 搬迁页表结构（spare 池 + newpt_dyn），minix-rs 搬迁分配器元数据（BumpBuf → HeapArena）——后者因 Direct Map 结构性消除了页表重建需求。
- `pt_init_done` + `level` 是 C 侧双路径分配/递归限制；minix-rs 单路径，两者都消失。
- 备用页池（reservedqueue_*）是自举机制，minix-rs 不实现（ARCH A-1），`missing_spares` 重解释为压力计数。
- LU 支撑面：`swap_proc_slot` 已实现；动态数据转移（transfer_mmap_regions 等）DEFERRED。

---

## 2. C 源码分析

### 2.0 本章定位

本章逐行验证 §1 的机制链。所有行号以 `sed -n` 实证为准（2026-08-15）。

### 2.1 pt_init() 搬迁段（pagetable.c:1311-1345）

`pt_init_done = 1`（:1311）之后是搬迁主流程：

```c
    pt_init_done = 1;                            /* pagetable.c:1311 */

    alloc_cycle();                               /* :1317 Make sure allocating works */
    while(vm_getsparepage(&phys)) ;              /* :1319 Use up all static pages */
    alloc_cycle();                               /* :1321 Refill spares with dynamic */
    pt_allocate_kernel_mapped_pagetables();      /* :1322 Reallocate in-kernel pages */
    pt_bind(newpt, &vmproc[VM_PROC_NR]);         /* :1323 Recalculate */
    pt_mapkernel(newpt);                         /* :1324 Rewrite pagetable info */
    if((sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)) != OK)   /* :1326-1329 */
        panic("VMCTL_FLUSHTLB failed");

    memset(&newpt_dyn, 0, sizeof(newpt_dyn));    /* :1331 Recreate VM page table */
    pt_new(&newpt_dyn);                          /* :1332 with dynamic-only allocations */
    pt_copy(&newpt_dyn, newpt);                  /* :1333 */
    memcpy(newpt, &newpt_dyn, sizeof(*newpt));   /* :1334 */

    pt_bind(newpt, &vmproc[VM_PROC_NR]);         /* :1337 Recalculate */
    pt_mapkernel(newpt);                         /* :1338 Rewrite pagetable info */
    if((sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)) != OK)   /* :1340-1343 */
        panic("VMCTL_FLUSHTLB failed");
```

语义分解：

1. **:1317 `alloc_cycle()`**——"Make sure allocating works"：此时 `pt_init_done == 1`，`alloc_mem` 动态路径已可用，先验证一遍分配器能回填空缺。
2. **:1319 `while(vm_getsparepage(&phys));`**——"Use up all static pages"：从备用队列 LIFO 取出全部页。静态页无法被 `vm_freepages` 释放（`is_staticaddr` :85 检查，§2.2），所以"用光"= 取走不归还，队列清空。
3. **:1321 `alloc_cycle()`**——"Refill spares with dynamic"：队列空 → `alloc_mem` 动态分配新页回填。此后备用队列全部是动态页（liveupdate 后 PA 仍有效）。
4. **:1322-1324**——`pt_allocate_kernel_mapped_pagetables` 把内核映射页表重分配为动态页，`pt_bind` 重新计算 CR3 信息，`pt_mapkernel` 重写内核映射 PTE。
5. **:1331-1334**——`pt_new(&newpt_dyn)` 用纯动态内存建新页表根，`pt_copy` 遍历复制用户 PDE/PTE，`memcpy` 整体替换 `newpt`。
6. **两次 FLUSHTLB**——内核映射重建后 + 页表整体替换后各一次，确保 TLB 无残留映射。

**为什么用光静态页而不是释放**：静态页的 PA 由内核加载时决定，`vm_freepages` 无法把它们还回 `alloc_mem` 的位图（它们本来就不在位图里）——唯一正确的处理就是"用掉"。

### 2.2 vm_freepages 与 is_staticaddr（pagetable.c:85 / :235-259）

```c
#define is_staticaddr(v) ((vir_bytes) (v) < VM_OWN_HEAPSTART)   /* :85 */

void vm_freepages(vir_bytes vir, int pages)                      /* :235 */
{
    assert(!(vir % VM_PAGE_SIZE));
    if(is_staticaddr(vir)) {                                     /* :239 */
        printf("VM: not freeing static page\n");
        return;
    }
    if(pt_writemap(vmprocess, &vmprocess->vm_pt, vir,            /* :243 */
        MAP_NONE, pages*VM_PAGE_SIZE, 0,
        WMF_OVERWRITE | WMF_FREE) != OK)
        panic("vm_freepages: pt_writemap failed");
    vm_self_pages--;                                             /* :250 */
    ...
}
```

`VM_OWN_HEAPSTART` 是 VM 自身堆的起始（静态地址 < 堆起始 = BSS 加载区）。`vm_self_pages`（:362 递增）是动态自用页计数——调试统计用。minix-rs 无此计数：Direct Map 下没有"静态地址"概念，`vm_freepages` 的对应物（`vm_self_unmap` + 物理页归还）由 08 的页表操作面提供。

### 2.3 reservedqueue 操作族（alloc.c:60-237）

`missing_spares` 守恒量（:74）+ 六个操作：

| 函数 | 位置 | 语义 | 守恒影响 |
|------|------|------|---------|
| `reservedqueue_new` | :109 | 建队列（`max_available` 等） | `missing_spares += max_available` |
| `reservedqueue_add` | :120 | 静态页入队（pt_init 用） | `missing_spares--` |
| `reservedqueue_fillslot` | :136 | 底层填充 {phys, vir} | `missing_spares--`，`n_available++` |
| `reservedqueue_addslot` | :148 | `alloc_mem` + `vm_mappages` + fillslot | 同上 |
| `reservedqueue_fill` | :191 | 循环 addslot 到满 | 同上 |
| `reservedqueue_alloc` | :206 | LIFO 取 slot | `missing_spares++`，`n_available--` |

`alloc_cycle`（:227-237）：

```c
void alloc_cycle(void)
{
    struct reserved_pages *rq;
    sanitycheck_queues();
    for(rq = first_reserved_inuse; rq && missing_spares > 0; rq = rq->next) {
        sanitycheck_rq(rq);
        reservedqueue_fill(rq);
        sanitycheck_rq(rq);
    }
    sanitycheck_queues();
}
```

主循环钩子（main.c:118-119）：`if(missing_spares > 0) { alloc_cycle(); }`——**分配压力驱动的补充机会**。minix-rs 的对应物：`mark_alloc_failure` 压力计数（§4.4）+ alloc_cycle 钩子保留（体 DEFERRED 归 24-page-cache）。

### 2.4 spare page 池初始化（pagetable.c:1116-1162）

```c
/* Get ourselves spare pages. */
sparepages_mem = (vir_bytes) static_sparepages;          /* :1117 */
assert(!(sparepages_mem % VM_PAGE_SIZE));

if(!(spare_pagequeue = reservedqueue_new(SPAREPAGES, 1, 1, 0)))   /* :1151 */
    panic("reservedqueue_new for single pages failed");

assert(STATIC_SPAREPAGES < SPAREPAGES);
for(s = 0; s < STATIC_SPAREPAGES; s++) {                 /* :1155 */
    void *v = (void *) (sparepages_mem + s*VM_PAGE_SIZE);
    phys_bytes ph;
    if((r=sys_umap(SELF, VM_D, (vir_bytes) v,
            VM_PAGE_SIZE*SPAREPAGES, &ph)) != OK)         /* :1158 */
        panic("pt_init: sys_umap failed: %d", r);
    reservedqueue_add(spare_pagequeue, v, ph);           /* :1161 */
}
```

- `SPAREPAGES`/`STATIC_SPAREPAGES`（:59-68）：i386 20/15，arm 150/140，SANITYCHECKS 200/190。
- `sys_umap` 把 VM 自身数据段的 VA 反查为 PA——自举期唯一能拿到"静态页 PA"的途径（内核知道 BSS 加载位置）。
- 队列容量 `SPAREPAGES` 与静态填充 `STATIC_SPAREPAGES` 的差额（5/10/10 页）由 `alloc_cycle` 动态补齐。

### 2.5 vm_allocpages 双路径（pagetable.c:333-394）

`level` 计数器 + `pt_init_done` 双条件（§1.4）。运行阶段路径（:369-394）：

```c
    newpage = alloc_mem(pages, flags);        /* :371 */
    if(newpage == NO_MEM) return NULL;
    newpage = CLICK2ABS(newpage);             /* :375 */
    ret = vm_mappages(newpage, pages);        /* :377 */
    ...
    vm_self_pages++;                          /* :392 */
```

动态路径 = `alloc_mem`（05）+ `vm_mappages`（08）。minix-rs 等价：`alloc_page::vm_pt_alloc`（06 §3.3）单路径。

### 2.6 swap_proc_slot（utility.c:188-210）

```c
int swap_proc_slot(struct vmproc *src_vmp, struct vmproc *dst_vmp)
{
    struct vmproc orig_src_vmproc, orig_dst_vmproc;

    orig_src_vmproc = *src_vmp;               /* Save existing data. */
    orig_dst_vmproc = *dst_vmp;

    *src_vmp = orig_dst_vmproc;               /* Swap slots. */
    *dst_vmp = orig_src_vmproc;

    src_vmp->vm_endpoint = orig_src_vmproc.vm_endpoint;   /* Preserve identities. */
    src_vmp->vm_slot = orig_src_vmproc.vm_slot;
    dst_vmp->vm_endpoint = orig_dst_vmproc.vm_endpoint;
    dst_vmp->vm_slot = orig_dst_vmproc.vm_slot;
    return OK;
}
```

**语义**：整槽 bitwise 交换（C 的 `struct` 赋值 = memcpy），然后还原两边的 `vm_endpoint`/`vm_slot`——"内容交换、身份保留"。这是 C 语言表达"进程槽热替换"的方式。Rust 版（§4.3）用 typestate + `core::ptr::swap` 表达同一语义，身份还原逻辑逐字对应。

### 2.7 动态数据转移族（utility.c:228-335 + region.c:1535）

**transfer_mmap_regions**（utility.c:228-281）三步：

1. `region_search(&src->vm_regions_avl, start_addr, AVL_GREATER_EQUAL)`（:236）找 `>= start_addr` 的第一个区域；空或 `vaddr >= end_addr` → OK（无事可做）。
2. **幂等检查**（:247-259）：`region_search(&dst->vm_regions_avl, start_vr->vaddr, AVL_EQUAL)` 非空 → 已转移过，跳过。注释说明多组件 LU 可能对同一进程调用多次，base 地址检查是简单去重。
3. `end_vr = region_search(&src->vm_regions_avl, end_addr, AVL_LESS)`（:263）→ `map_proc_copy_range(dst, src, start_vr, end_vr)`（:272）CoW 复制。

**map_proc_dyn_data**（utility.c:283-308）：

```c
    r = transfer_mmap_regions(src_vmp, dst_vmp, VM_MMAPBASE, VM_MMAPTOP);
    if (r == OK && VM_STACKTOP < VM_DATATOP)
        r = transfer_mmap_regions(src_vmp, dst_vmp, VM_STACKTOP, VM_DATATOP);
```

两段覆盖：常规 mmap 区 + 栈上方的隐藏区域（栈未映射到 VM_DATATOP 时可能存在）。

**swap_proc_dyn_data**（utility.c:312-335）：

```c
    is_vm = (dst_vmp->vm_endpoint == VM_PROC_NR);
    if(is_vm) {                                   /* VM 分支：先转移堆/栈映射 */
        r = pt_map_in_range(src_vmp, dst_vmp, VM_OWN_HEAPBASE, VM_OWN_MMAPTOP);
        if(r != OK) return r;
        r = pt_map_in_range(src_vmp, dst_vmp, VM_STACKTOP, VM_DATATOP);
        if(r != OK) return r;
    }
    map_setparent(src_vmp);                       /* 交换区域父指针 */
    map_setparent(dst_vmp);
    if(is_vm || (sys_upd_flags & (SF_VM_ROLLBACK|SF_VM_NOMMAP))) return OK;
    return map_proc_dyn_data(dst_vmp, src_vmp);   /* 源/目标有意反向 */
```

VM 更新（is_vm）时：**先**把 VM 自身的堆/栈页表映射转移给新实例（`pt_map_in_range`，因为 VM 的地址空间就是它的工作集），**然后** `map_setparent` 让区域所有权归新实例。非 VM 且非回滚/非 NOMMAP 时，反向 `map_proc_dyn_data(dst, src)`——把**旧实例**的 mmap 区域以 CoW 共享给新实例（回滚场景不需要，NOMMAP 场景显式排除）。

**map_setparent**（region.c:1535-1551）：

```c
void map_setparent(struct vmproc *vmp)
{
    struct vir_region *vr;
    for(vr = region_search_root(&vmp->vm_regions_avl); vr; vr = ...)
        vr->parent = vmp;
}
```

把进程全部 vir_region 的 `parent` 指向该进程——区域所有权切换。minix-rs 中 `vir_region.parent` 对应 `region/vir_region.rs` 的 parent 字段（13-region-mapping 覆盖）。

### 2.8 本章小结

- 搬迁段（pagetable.c:1311-1345）是自举终点：静态页用光 + 动态回填 + 页表重建 + 两次 FLUSHTLB。
- `pt_init_done`/`level`/`is_staticaddr` 是 C 侧自举期控制面；`reservedqueue_*` + `missing_spares` + `alloc_cycle` 是备用池机制。
- LU 支撑面（utility.c）分两族：槽交换（swap_proc_slot）+ 动态数据转移（transfer_mmap_regions/map_proc_dyn_data/swap_proc_dyn_data/map_setparent）。

---

## 3. Rust 设计决策

### 3.1 D1: 搬迁对象不同——元数据而非页表结构（ARCH A-1 主决策）

C 搬迁页表结构（spare 池 + newpt_dyn，§2.1），Rust 搬迁分配器元数据（BumpBuf → HeapArena）。依据（§1.2）：

- Direct Map（`VM_DIRECT_MAP_BASE + phys`）提供稳定 VA，页表页分配单路径可用——C 的"liveupdate 后 PA 失效 → 页表重建"需求结构性消失。
- Rust 自举元数据占据 BumpBuf 连续 PA（稀缺资源），搬迁后释放回分配器（DMA 等场景可复用）。

**语义影响**：`relocate()` 不触碰任何页表结构——`vm_self_mappages`（08）只用于把新元数据的碎片物理页映射为 HeapArena 连续 VA。C 的 `pt_bind`/`pt_mapkernel`/FLUSHTLB 序列在 Rust 无对应。

### 3.2 D2: 语义化搬迁（available_regions + init）

不 memcpy 位图，而是**语义化重建**（§1.6 流程）：

1. 旧分配器状态 → `metadata_pa_range()`（旧元数据 PA）+ `total_count()`（总页数）。
2. 新元数据空间 → `heap_arena_grow(pages)`（从旧分配器逐页取物理页映射为连续 VA——分配即排除）。
3. 空闲区域 → `available_regions(callback)` 枚举旧分配器全部空闲页区间。
4. 重建 → `BitmapAllocator::init(new_metadata, total_pages, &free_regions, 0, 0)`——空闲页数由区域列表推导，**状态一致由 init 语义保证**。
5. 替换 + 释放旧 PA。

**为什么这样设计**：位图本身没有"可搬迁性"——它的内容（空闲位）是分配器状态的编码。直接 memcpy 需要处理 metadata 布局差异（Bitmap/Buddy/SegmentTree 元数据格式不同），语义化重建把"搬迁"抽象为"枚举状态 → 重建状态"，使三种分配器（ARCH A-5）都能搬迁。

### 3.3 D3: 失败语义 = fail-fast

- `as_bitmap().expect("relocate: bootstrap allocator must be Bitmap")`——自举分配器必须是 Bitmap（唯一实现）。
- `assert!(pa_pages > 0, "relocate: no BumpBuf metadata to relocate (already relocated?)")`——防重复搬迁。
- `heap_arena_grow(pages).expect(...)`——新元数据分配失败即 panic。

**测试门控**：`#[cfg(not(test))] self.relocate();`（vm_server.rs:257）——测试环境无真实页表（`vm_self_mappages` 不可用，`X86_64Paging::new` 是 todo!()）。搬迁语义由 `available_regions`/`init` 单元测试覆盖（§5），端到端归 QEMU 集成。

### 3.4 D4: 备用页池结构消除（ARCH A-1）

`reservedqueue_*`（alloc.c:60-237）不实现（§1.7 论证）。保留的语义：

| C 语义 | Rust 对应 | 位置 |
|--------|-----------|------|
| `missing_spares` 压力驱动补充 | `mark_alloc_failure()` 压力计数 | vm_server.rs:396（见 §4.4） |
| `alloc_cycle` 主循环钩子 | 保留钩子（体 DEFERRED 归 24-page-cache） | 主循环（15-ipc-dispatch 覆盖） |
| `pt_init_done` 阶段切换 | 无（单路径分配） | — |
| `level` 递归限制 | 无（08 D2：walk_alloc 递归结构性消失） | — |
| `is_staticaddr`/`vm_self_pages` | 无（Direct Map 无静态/动态二分） | — |

### 3.5 D5: LU 支撑面——swap_proc_slot 实现 + 转移族 DEFERRED

**已实现**：`swap_proc_slot`（vmproc_handle.rs:620，§4.3）——typestate 表达"内容交换、身份保留"，SAFETY 论证 4 条不变量。

**DEFERRED**（诚实标注，不伪装实现）：`transfer_mmap_regions`/`map_proc_dyn_data`/`swap_proc_dyn_data`/`map_setparent`——它们依赖 mmap 区域 CoW 共享（13/17/20 的实施），当前 RS_PREPARE 部分实现（`map_pin_memory` 已实现）、RS_UPDATE 返回 NotImplemented（rs.rs:290）。rs.rs 模块头（:17-18）与函数注释（:162/:210/:229）三处一致声明。

### 3.6 语义差异清单（C ↔ Rust 诚实标注）

| 维度 | Minix3 | minix-rs | 标注 |
|------|--------|----------|------|
| 搬迁对象 | spare 池 + 页表结构 | 分配器元数据 | ARCH A-1 |
| 搬迁触发 | pt_init() 隐式（pagetable.c:1317） | init() 内 relocate() 显式（vm_server.rs:182） | 结构差异 |
| 备用页池 | reservedqueue_* 全量 | 结构消除；missing_spares → 压力计数 | ARCH A-1 |
| 阶段切换 | pt_init_done + level | 无 | ARCH A-1 |
| TLB 刷新 | sys_vmctl(FLUSHTLB) ×2 | arch 层管理，VM 侧无调用 | 架构差异 |
| swap_proc_slot | 整槽 memcpy + 身份还原 | typestate + ptr::swap + 身份还原 | 同语义 |
| 动态数据转移 | 全量实现 | DEFERRED（rs.rs） | ARCH A-8 关联 |
| 自用页计数 | vm_self_pages | 无 | 结构差异 |

---

## 4. 实现详解

### 4.1 `VmServer::relocate()`（vm_server.rs:182-236）

搬迁编排（§1.6 流程的代码落点）：

```rust
fn relocate(&mut self) {
    let (total_pages, old_pa_base, old_pa_pages) = {
        let phys_alloc = self.page_alloc.phys_alloc();
        let bitmap = phys_alloc.as_bitmap()
            .expect("relocate: bootstrap allocator must be Bitmap");
        let (pa_base, pa_pages) = bitmap.metadata_pa_range();
        assert!(pa_pages > 0, "relocate: no BumpBuf metadata to relocate (already relocated?)");
        (bitmap.total_count(), pa_base, pa_pages)
    };

    let alloc_type = Self::choose_allocator_type(total_pages);
    let meta_size = alloc_type.metadata_size(total_pages);
    let pages = bytes_to_clicks(meta_size);
    let new_va = crate::global::heap_arena_grow(pages, &mut self.page_alloc)
        .expect("relocate: failed to allocate new metadata via HeapArena");
    let new_metadata = unsafe {
        core::slice::from_raw_parts_mut(new_va as *mut u8, meta_size)
    };

    let mut free_regions: alloc::vec::Vec<BootMemRegion> = alloc::vec![];
    {
        let phys_alloc = self.page_alloc.phys_alloc();
        phys_alloc.available_regions(&mut |base_page, num_pages| {
            free_regions.push(BootMemRegion {
                base: base_page * CLICK_SIZE,
                size: num_pages * CLICK_SIZE,
            });
        });
    }

    let new_alloc = match alloc_type { /* Bitmap/Buddy/SegmentTree init */ };

    {
        let phys_alloc = self.page_alloc.phys_alloc_mut();
        *phys_alloc = new_alloc;
        let old_pa = AlignedPhysBytes::new(old_pa_base);
        phys_alloc.free_mem(old_pa, old_pa_pages);   /* 释放旧连续 PA 页 */
    }
}
```

关键点：

- `choose_allocator_type`（:172-179）：`buddy_alloc` feature 下 `total_pages > BUDDY_THRESHOLD_PAGES(1<<20)` → Buddy，否则 Bitmap——**4GB 物理内存阈值**（`BUDDY_THRESHOLD_PAGES` 定义于 phys_mem/mod.rs:267）。
- `heap_arena_grow`（global.rs:481）委托 `HEAP_ARENA.grow(pages, page_alloc)`——逐页分配 + 映射（09 §4.1）。
- `free_regions` 收集在 `heap_arena_grow` **之后**：新元数据占用的页已被旧分配器标记为已用，`available_regions` 自动排除。
- 替换（`*phys_alloc = new_alloc`）后立刻 `free_mem(old_pa, old_pa_pages)`——旧连续 PA 页释放回**新**分配器（此时 phys_alloc 已指向新分配器，释放正确）。

### 4.2 `metadata_pa_range` / `available_regions`（bitmap_alloc.rs:110 / :464）

```rust
pub fn metadata_pa_range(&self) -> (u64, usize) {
    (self.meta_phys_base, self.meta_pages)      /* bitmap_alloc.rs:110-112 */
}
```

`meta_phys_base`/`meta_pages` 是 `BitmapAllocator::init` 的构造参数（自举期在 `create_default_allocator` 传入，vm_server.rs:130-170）——**记录旧元数据占据的物理页**，供搬迁释放。搬迁后新分配器传 `(0, 0)`（§5 测试验证清零）。

```rust
fn available_regions(&self, callback: &mut dyn FnMut(usize, usize)) {
    let mut i = 0;
    let total = self.bitmap_len();
    while i < total {
        if !self.page_is_free(i) { i += 1; continue; }
        let start = i;
        while i < total && self.page_is_free(i) { i += 1; }
        callback(start, i - start);             /* bitmap_alloc.rs:464-474 */
    }
}
```

线性扫描位图，把连续空闲页区间逐一交给回调——**空闲状态 → 区域列表**的枚举器，语义化搬迁的数据源。

### 4.3 `swap_proc_slot`（vmproc_handle.rs:620-705）

typestate 版本（§2.6 C 语义的 Rust 表达）：

```rust
pub(crate) fn swap_proc_slot(&mut self, other: &mut ActiveProc<'_>) {
    debug_assert_ne!(self.slot(), other.slot(), "...distinct slots");
    let self_endpoint = self.inner.vm_endpoint;
    let self_slot = self.inner.vm_slot;
    let other_endpoint = other.inner.vm_endpoint;
    let other_slot = other.inner.vm_slot;
    // SAFETY: 1. distinct raw pointers (borrow checker + debug_assert)
    //         2. bitwise-swap safe fields (Copy types + BTreeMap moves with struct)
    //         3. single-threaded VM
    //         4. no CR3 in-flight (both processes quiescent)
    unsafe { core::ptr::swap(self.inner as *mut VmProc, other.inner as *mut VmProc); }
    self.inner.vm_endpoint = self_endpoint;     /* 身份还原 */
    self.inner.vm_slot = self_slot;
    other.inner.vm_endpoint = other_endpoint;
    other.inner.vm_slot = other_slot;
}
```

设计要点：

- **typestate 保证**：`&mut ActiveProc` 只能通过 `VmProcTable` 的 split-borrow API 获得，两个视图不可能指向同一 slot——`debug_assert_ne` 是兜底。
- **位交换安全性**：`VmProc` 全字段要么 `Copy`（endpoint/slot/flags/ACL/地址边界），要么可随结构移动（`RegionMap` 的 BTreeMap 节点指针相对自身、`PageTable` 无 CR3 绑定且旧主人已 unbound）——SAFETY 注释逐字段论证（vmproc_handle.rs:655-695）。
- **身份还原**：与 C 的 `src_vmp->vm_endpoint = orig_src_vmproc.vm_endpoint` 逐字对应（§2.6）。

### 4.4 `mark_alloc_failure`（vm_server.rs:396）

`missing_spares` 重解释为压力计数（06 §3.3）：

```rust
pub fn mark_alloc_failure(&mut self) {
    self.alloc_pressure += 1;      /* vm_server.rs:396-398 */
}
```

- 物理页分配失败（06 `vm_allocpage` 返回 None）时递增。
- 主循环的 alloc_cycle 钩子（对应 main.c:118-119）在压力计数 > 0 时获得补充/回收机会——体 DEFERRED 归 24-page-cache（页缓存回收）。
- 模块头注释（vm_server.rs:53-61）三处一致标注 `[ARCH: A-1]` 结构消除。

### 4.5 LU 支撑面 DEFERRED 声明（rs.rs:17-18 / :210 / :229）

```rust
//! - **PREPARE**: Partially implemented — ... `map_proc_dyn_data` are deferred.   (rs.rs:17)
//! - **UPDATE**: Partially implemented — ... swap_proc_slot/swap_proc_dyn_data deferred. (rs.rs:18)
```

`RS_PREPARE` 的 step 5（rs.rs:210）与 `RS_UPDATE` 的 step 6（rs.rs:229）显式 DEFERRED——**fail-closed**（返回 NotImplemented，ARCH A-8），不伪装实现。25-rs-services 覆盖完整服务流程。

### 4.6 启动时序接线

```
VmServer::new()  → create_default_allocator()（BumpBuf 元数据，vm_server.rs:130-170）
VmServer::init() → #[cfg(not(test))] relocate()（vm_server.rs:257）
                 → init_global_state()（total_pages 等，:259）
                 → init_vm_slot() / account_boot_memory() / init_boot_procs() / mark_vm_instance()
```

C 对照：`pt_init()` 搬迁段（pagetable.c:1311-1345）位于 `init_vm()` 内 `__minix_init()` 之前——Rust 的 `relocate()` 同样在全局状态初始化之前，保证后续所有分配（进程表、区域、页缓存）都走动态路径。

### 4.7 消费链与边界

- **上游**：05（BitmapAllocator::init 自举构造）、06（页分配器 + mark_alloc_failure）、08（vm_self_mappages 供 heap_arena_grow）、09（HeapArena::grow）。
- **下游**：relocate 后所有分配（`#[global_allocator]` free-list、进程表、region、page cache）都在稳态分配器上。
- **边界**：不覆盖 RS 服务流程（25）；不覆盖页缓存回收（24）；`map_proc_dyn_data` 等 LU 转移族归 25 实施。

---

## 5. 测试要点

### 5.1 单元测试清单（grep 实证，2026-08-15）

| 测试 | 位置 | 验证目标 |
|------|------|---------|
| `test_available_regions_init` | bitmap_alloc.rs:748 | 语义化搬迁：收集区域 + init 重建后 `free_pages` 一致 + `metadata_pa_range` 清零 |
| `test_available_regions_basic` | bitmap_alloc.rs:785 | 已分配页不出现在空闲区域 |
| `test_metadata_pa_range` | bitmap_alloc.rs:811 | 旧元数据 PA 范围记录 |
| `test_metadata_pa_range_default` | bitmap_alloc.rs:822 | 默认 0 范围 |
| `test_available_regions_init_clears_pa_range` | bitmap_alloc.rs:833 | 重建后 `metadata_pa_range` 清零（搬迁完成标志） |
| `test_swap_proc_slot_preserves_identities` | vmproc_handle.rs:984 | 交换保留 endpoint/slot（LU 身份不变量） |

### 5.2 覆盖维度

- **语义化搬迁等价性**：`test_available_regions_init` 断言重建后 `free_pages == free_before` + 分配/释放功能正常。
- **搬迁完成标志**：重建后 `metadata_pa_range() == (0, 0)`——新分配器无旧元数据要释放。
- **LU 身份不变量**：`test_swap_proc_slot_preserves_identities` 交换后各槽 endpoint/slot 不变。

### 5.3 覆盖缺口与诚实标注

- **relocate 端到端**：依赖真实页表（`vm_self_mappages`），`#[cfg(not(test))]` 跳过——生产路径归 QEMU 集成（01/10）。单元层由 5.1 的语义测试覆盖。
- **Buddy 分支**：feature-gated（`buddy_alloc`），`BUDDY_THRESHOLD_PAGES` 阈值选择逻辑无单测——记录不阻塞。

### 5.4 测试统计（截至 2026-08-15）

- `cargo test -p minix-vm --lib`：**360 passed / 1 failed**（1 failed 为 pre-existing `region::vir_region::tests::test_map_lazy`，属 13-region-mapping 范围，非本文档引入）。
- 本文档直接相关：bitmap_alloc.rs 语义化搬迁 5 个 + vmproc_handle.rs swap 1 个 = **6 个**。
- 完整测试清单：`rg "^\s*fn test_" os/servers/vm/src/`。

---

## 6. 过渡

本文档是**阶段 4（自举的堆与元数据）的终点**。自举链至此完整：

```
05（物理分配器）→ 06（页分配器）→ 07/08（页表结构/操作）→ 09（堆分配器）→ ★10（自举终点）
```

搬迁完成后，VM 进入**地址空间数据结构**阶段（11-14）：`vir_region`/`phys_region` 映射、6 类 memtype、区域查找——这些数据结构运行在稳态分配器之上。主循环与分发（15）随后把各服务接线起来；页错误 + CoW（16/17）与 IPC 服务（18-22）消费本阶段的页表/分配能力；跨服务协作（23-26）中，25-rs-services 消费 `swap_proc_slot`（已实现）并承接 `map_proc_dyn_data`/`swap_proc_dyn_data` 的实施（DEFERRED）。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/plan.md`（§3.4 边界、§5.3 契约、§4 A-1/A-5）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/05-physical-memory.md`（bitmap 分配器 + BumpBuf）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/06-page-allocator.md`（页分配器 + missing_spares 重解释 + ARCH A-1）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/07-pagetable-struct.md`（页表结构 + Direct Map）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/08-pagetable-ops.md`（vm_self_mappages 操作面）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/09-slab-allocator.md`（HeapArena + VmAllocator）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/09-vm-relocation.md`（素材）
- `os/servers/vm/src/vm_server.rs`、`global.rs`、`phys_mem/mod.rs`、`phys_mem/bitmap_alloc.rs`、`vmproc/vmproc_handle.rs`、`rs.rs`
- `minix3/minix/servers/vm/pagetable.c`、`alloc.c`、`utility.c`、`region.c`、`main.c`
