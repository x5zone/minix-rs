# 09-slab-allocator: 内核堆分配——从 slab 到 HeapArena + VmAllocator

> **分类**: 阶段 4 — 自举的堆与元数据（堆分配面）
> **源码**: `minix3/minix/servers/vm/slaballoc.c`（528 行：`SLABSIZES` :29 / `ITEMSPERPAGE` :31 / `ELBITS` :33 / `BITPAT` :34 / `BITEL` :35 / `GETBIT` :69 / `SETBIT` :70 / `CLEARBIT` :71 / `OBJALIGN` :73 / `MINSIZE` :75 / `MAXSIZE` :76 / `USEELEMENTS` :77 / `struct sdh` :96 / `DATABYTES` :111 / `MAGIC1` :113 / `MAGIC2` :114 / `JUNK` :115 / `NOJUNK` :116 / `struct slabdata` :118 / `slabs[]` :125 / `GETSLAB` :130 / `ADDHEAD` :140 / `UNLINKNODE` :151 / `newslabdata` :159 / `checklist` :194 / `slab_sanitycheck` :229 / `slabsane_f` :240 / `slaballoc` :259 / `objstats` :344 / `slabfree` :406 / `slablock` :464 / `slabunlock` :483 / `slabstats` :504）+ `minix3/minix/servers/vm/proto.h:133-134`（`SLABALLOC`/`SLABFREE` 宏）+ `minix3/minix/servers/vm/vm.h:13`（MEMPROTECT）+ `minix3/minix/servers/vm/vm.h:52`（VMP_SLAB）+ `minix3/minix/servers/vm/pagetable.c:403`（`vm_pagelock`）
> **Rust 模块**: `os/servers/vm/src/heap_arena.rs`（`HeapArena` + `HeapArenaError`）+ `os/servers/vm/src/global.rs`（`VmAllocator` bump + `#[global_allocator]` + `PAGE_ALLOC_PTR`）+ `os/servers/vm/src/pagetable/vm_self_map.rs`（`vm_self_mappages`/`vm_self_unmap`）+ `os/servers/vm/src/direct_map.rs`（`VM_HEAP_BASE`/`VM_HEAP_SIZE`/`VM_HEAP_LIMIT`）+ `os/servers/vm/src/vm_server.rs`（接线）
> **前置**: `notes/rewrite/fork-syscall-rewrite/02-stage-vm/06-page-allocator.md`（`VmPageAllocator` 给堆供物理页）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/07-pagetable-struct.md`（页表结构 + Direct Map）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/08-pagetable-ops.md`（`vm_self_mappages`/`vm_self_unmap` 操作面）、`notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md`（`init_vm` 时序 + `__minix_init` 分界线）
> **说明**: 内核堆分配语义模块：**Minix3 的 slab 尺寸分类分配器（slaballoc.c 全量）、`SLABALLOC`/`SLABFREE` 类型化宏、MEMPROTECT 调试写保护、slabstats 统计**。**不覆盖**：物理页分配器（06）、页表结构/操作（07/08）、元数据搬迁（10）、内存占用统计（26）。

---

## 1. 概念：内核堆分配——VM 自举的堆

### 1.0 章节引言

VM server 自己也是一个用户态进程，它需要**堆**：libc 的 `malloc`/`printf`、`struct vmproc` 数组之外动态创建的对象（region、phys_block、fdref、page cache 元数据）都要堆。但 VM 的堆有一个特殊性——它是**自举的**：在 VM 自己的页表建立（07/08）之前，没有任何内存管理设施可用；在物理页分配器（06）就绪之后，堆才有物理页来源；在页表操作面（08）就绪之后，堆才有连续虚拟地址空间。

本文档回答三个问题：

1. **堆何时可用**——`init_vm()` 里 `__minix_init()` 之前的"堆不可用"与之后的"堆可用"分界线（启动时序锚点，§1.1）。
2. **堆从哪来、怎么组织**——Minix3 用 slab 尺寸分类分配器（§1.3-§1.5），Rust 侧用 `HeapArena` + `VmAllocator`（bump）替代（§1.8）。
3. **调试期如何保护堆**——MEMPROTECT 写保护硬化语义（§1.6）。

它在启动时序中的位置：

```
init_vm()（main.c:428）
  ├─ sys_getkinfo / get_mem_chunks / memset(vmproc) ...      ← 01/02/03/05
  ├─ acl_init() / map_region_init()                          ← 04/13
  ├─ mem_init(mem_chunks)                                    ← 05：物理内存布局
  ├─ init_proc(VM_PROC_NR) + pt_init()                       ← 02/07/08：VM 自身页表就绪
  ├─ __minix_init()（main.c:480）                            ← ★ 本文档：堆可用分界线
  ├─ mem_add_total_pages()（main.c:485-495）                 ← 05：总页数校准
  ├─ exec_bootproc() / CALLMAP / sef_local_startup()         ← 01/15
```

Rust 侧对应的分界线是 `VmServer::new()` 中的 `register_page_alloc()`（`vm_server.rs:92`）：此后 `#[global_allocator]` 的 bump 分配器才有物理页来源。**C 的分界线与 Rust 的分界线不是同一时刻**——前者是 libc 构造完成，后者是全局分配器获得供页能力（§1.1 详述）。

### 1.1 堆可用分界线：init_vm() 里堆何时可用

C 侧的堆分界线是 `init_vm()`（main.c:428）中的 `__minix_init()`（main.c:480）：

```c
init_proc(VM_PROC_NR);      /* main.c:474 —— VM 自身槽 + 页表初始化 */
pt_init();                  /* main.c:475 —— 页表结构初始化（07） */
__minix_init();             /* main.c:480 —— 获取内核 IPC 向量；libc 构造函数 */
```

`__minix_init` 是 VM 程序链接的 libc 构造函数（`init.c`），它通过 `sys_getkinfo` 等内核调用获取**内核 IPC 向量**（`kernel_call`/`endpoint_lookup` 等函数指针）。在它之前，libc 的 `printf`/`malloc` 等依赖内核服务的函数不可用——因此**堆的可编程使用以 `__minix_init()` 为分界线**。值得注意的是 `__minix_init` 本身的运行不需要堆：libc 构造函数只做函数指针填充，不分配动态内存。

Rust 侧的对应物有两层：

1. **`init_vm_self_pt()`**（VM 自身页表建立，07/08）——`vm_self_mappages` 的前置，HeapArena 写 PTE 依赖它。
2. **`register_page_alloc()`**（`vm_server.rs:92`，`VmServer::new`）——`PAGE_ALLOC_PTR` 指向 `VmPageAllocator`，HeapArena::grow 才有物理页可映射。

两者合起来才是 Rust 的"堆可用分界线"：**页表可写（映射能力）+ 物理页可取（供页能力）**。这个组合对应 C 侧 `pt_init()` + `__minix_init()` 之间的窗口——C 在 `pt_init()` 之后堆结构（`slabs[]`）已是静态全局、物理页可由 `vm_allocpage` 供给，所以 C 的堆实际在 `pt_init()` 后即可用，`__minix_init()` 是"libc 完整可用"的语义分界。

### 1.2 自举的堆：物理页从哪来、VA 为什么必须连续

堆的物理页来源：`newslabdata()`（slaballoc.c:166）调用 `vm_allocpage(&p, VMP_SLAB)`——**VM 自己的物理页分配器（06）给堆供页**，`VMP_SLAB`（vm.h:52，reason=3）是页分配器的用途标记（与 `VMP_PAGETABLE` 等并列，供统计/审计）。这意味着堆页与 VM 管理的其他物理页同池：堆页可被碎片化分配，没有"堆专用内存区"。

堆的 VA 组织：slab 分配器要求对象地址在**连续 VA 内**（`data + i*bytes` 的指针算术），因此 slab 页必须连续映射。32 位 Minix3 的 VM 地址空间是线性映射的（页表直接覆盖整个空间），VA 连续性天然成立；64 位 Rust 重写引入 Direct Map 后，**Direct Map 提供稳定 VA（VA = PA + BASE）但不提供 VA 连续性**——物理洞变成 VA 洞，bump 分配器无法在洞上行走。这是 09 的核心架构问题：

> **物理页可碎片化，VA 必须连续。** Direct Map 解决"任意物理页有稳定 VA"，`HeapArena` 解决"堆的 VA 连续"——两者分工（§3.2 D2）。

### 1.3 Slab 分配器机制：尺寸分类 + 页内位图 + 空闲链

Minix3 的 slaballoc.c 是一个**基于单页 slab 的尺寸分类对象分配器**，四个要素：

1. **尺寸分类**：`SLABSIZES=200`（slaballoc.c:29）个尺寸类，覆盖 8..207 字节（`MINSIZE=8` :75、`OBJALIGN=8` :73）；请求按 `OBJALIGN` 向上取整后落入唯一尺寸类（`GETSLAB` :130）。
2. **单页 slab**：每个 slab 恰好一页（`struct slabdata` :118），页内数据区（`DATABYTES` :111 = `VM_PAGE_SIZE - sizeof(struct sdh)`）+ 页尾头（`struct sdh` :96）；页内对象数 `ITEMSPERPAGE(bytes) = DATABYTES / bytes`（:31）。
3. **页内位图**：`usebits[USEELEMENTS]`（:77）记录页内对象占用，`GETBIT`/`SETBIT`/`CLEARBIT`（:69-71）操作单 bit；`freeguess` 是"下一分配从哪开始找"的提示（slaballoc 从 freeguess 起线性扫描，slabfree 更新它），避免每次都从 0 扫描。
4. **空闲链**：`slabs[200]`（:125）每个尺寸类一条双向空闲链，链上全是**未满**的 slab；满 slab 被 `UNLINKNODE`（:151）摘出，空 slab 被 `vm_freepages`（:449）还回物理分配器。

这个设计的本质是：**C 没有所有权/析构，分配器必须自己管理"哪个对象被占用、释放后如何复用"**——位图 + 空闲链就是 C 侧的"对象生命周期簿记"。对象大小固定（尺寸类）使复用无需大小协商。

### 1.4 分配与释放流程

`slaballoc(bytes)`（slaballoc.c:259）：

1. `roundup(bytes, OBJALIGN)`——对齐到 8 字节（`bytes = roundup(...)`）。
2. `GETSLAB(bytes, s)`——定位尺寸类空闲链头。
3. 链表空 → `newslabdata()`（:159）`vm_allocpage(&p, VMP_SLAB)` 取一物理页，清零位图，挂到链头（`ADDHEAD` :140）。
4. 从 `freeguess` 起位图扫描找空位（`count < ITEMSPERPAGE(bytes)` 上限保护）。
5. `SETBIT` 置占用位 + `nused++`；若该 slab 变满（`nused == ITEMSPERPAGE(bytes)`），`UNLINKNODE` 从空闲链摘出。
6. 返回 `((char*)newslab) + i*bytes`——对象地址 = 页基址 + 对象号 × 对象大小。

`slabfree(mem, bytes)`（slaballoc.c:406）：

1. `objstats`（:344）反查对象所属 slab/尺寸类/对象号（页对齐取页尾头，校验魔数与占用位）。
2. `CLEARBIT` 清占用位 + `nused--` + 更新 `freeguess`。
3. 若页变空（`nused == 0`）→ `UNLINKNODE` + `vm_freepages`（:449）**把整页还给物理分配器**——slab 页不长期占用，内存压力下自动收缩。
4. 若页从满变非满（`nused == ITEMSPERPAGE(bytes)-1`）→ `ADDHEAD` **挂回空闲链**——腾出的对象重新可复用。

两个双向判定的边界：**页空 → 还页；页从满到不满 → 回链**。这是 slab 内存收缩/复用的完整循环。

### 1.5 SLABALLOC/SLABFREE 宏：C 侧的类型化约定

proto.h:133-134 的两个宏把"分配/释放"与"类型大小"绑定：

```c
#define SLABALLOC(var) (var = slaballoc(sizeof(*var)))
#define SLABFREE(ptr) do { slabfree(ptr, sizeof(*(ptr))); (ptr) = NULL; } while(0)
```

- `SLABALLOC(var)` = `var = slaballoc(sizeof(*var))`——按变量类型的实际大小分配并赋值；失败（NULL）由调用方检查。
- `SLABFREE(ptr)` = 按 `sizeof(*ptr)` 释放 + **置 NULL**——防止悬垂指针二次释放。

这是 C 语言里能表达的最接近"类型化分配器"的约定：**分配带类型、释放带类型、释放后防悬挂**。Rust 侧 `Box::new`/`drop` 是它的天然进化——类型由编译器保证、释放由析构保证、用后释放由借用检查禁止（§3.1 D1）。消费方实证（`rg SLABALLOC|SLABFREE *.c`，2026-08-15）：vfs.c:76/135、pb.c:36/58/78/128、region.c:432/451/559/581、cache.c:227/282/285、fdref.c:97/140——覆盖 VM 的核心元数据对象（region/phys_block/fdref/page cache/vfs reqnode）。

### 1.6 MEMPROTECT：调试期写保护硬化

MEMPROTECT（vm.h:13，**默认 0**）是 SANITYCHECKS 门控的调试特性：开启时，slab 页在 PTE 层被翻成**只读**（VM 自己也不可写），只有通过 `SLABDATAUSE` 宏进入临界区时才临时开写。机制链：

1. `SLABDATAWRITABLE(data, wr)`（slaballoc.c:42-48）：断言当前不可写 + 请求非 NONE，`vm_pagelock(data, 0)` 翻页为可写，记 `writable` 标记。
2. `SLABDATAUNWRITABLE(data)`（:49-53）：断言当前可写，记 `WRITABLE_NONE`，`vm_pagelock(data, 1)` 翻页为只读。
3. `SLABDATAUSE(data, code)`（:55-59）：开写 → 执行 `code`（链表/位图操作）→ 关写。
4. `slablock`/`slabunlock`（:464/:483）：对**已分配对象**整页翻只读/可写——slaballoc 返回前 `slabunlock(ret, bytes)`（对象可写）、slabfree 里 `slabunlock(mem, bytes)` 后再写 JUNK。
5. `vm_pagelock`（pagetable.c:403）：`pt_writemap(vmprocess, pt, m, 0, VM_PAGE_SIZE, ARCH_VM_PTE_PRESENT|ARCH_VM_PTE_USER[|RW])`——用 WMF_WRITEFLAGSONLY（08 D1）只翻 PTE RW 位，不改变物理地址。

语义：**检测对已分配对象的越权写/释放后使用**——任何未经 SLABDATAUSE 的对象写都会触发页错误；JUNK/NOJUNK 标记（:115-116）配合检测双重释放。这是调试期的"堆硬化"，生产构建（MEMPROTECT=0）下这些宏全部展开为空（slaballoc.c:63-65）。

### 1.7 调试统计与一致性检查

- `slabstats()`（slaballoc.c:504）：每 1000 次调用（`n%1000`）打印一次各尺寸类利用率与总利用页数（`pages` :79）——VMSTATS 输出供人工审计，非功能路径。
- `slab_sanitycheck`（:229）/`slabsane_f`（:240）/`checklist`（:194）：SANITYCHECKS 门控——魔数（`MAGIC1` :113/`MAGIC2` :114）、双向链一致性、位图-计数一致性、`usedpages_add` 登记。
- slaballoc.c 的注释明说：这个文件**太低层**，数据结构在 alloc/free 中途必然不一致，所以不做全局 SANITYCHECK，只做自己的 `SLABSANITYCHECK`（`SCL_FUNCTIONS=2` 函数级 / `SCL_DETAIL=3` 细节级，vm.h:44-45）。

### 1.8 ARCH A-3：为什么 Rust 侧不实现 slab

> **[ARCH: A-3]** `slaballoc.c`（528 行）→ `HeapArena`（连续 VA 区间）+ 全局 `VmAllocator`（bump）——slab 尺寸分类/位图/空闲链**有意省略**。

不实现 Minix3 式专用 slab 分配器的理由（因果链，不是"Minix3 过时"）：

1. **对象生命周期表达方式不同**：C 无所有权/析构，分配器必须用位图 + 空闲链簿记"谁被占用、释放后如何复用"；Rust 的类型系统（RAII + 借用检查）让对象生命周期由编译器保证——分配器的簿记职责消失。
2. **分配模式不同**：C 侧 slab 服务于**高频分配/释放**（每次 mmap/munmap/fork 都建/毁 region、phys_block、fdref）；Rust 侧 VM 的分配集中在启动期 + 少量长期对象，服务路径的临时对象经 `Box`/`Vec` 由 bump 分配 + 析构处理（dealloc no-op，§3.1）。
3. **32 位地址空间稀缺性消失**：C 侧位图管理（每对象 1 bit）是对 4GB 地址空间的精打细算；64 位 + 64MB 堆区间（`VM_HEAP_SIZE`，os/arch/src/arch/direct_map.rs:69）无需这种粒度。
4. **代价诚实声明**：无对象复用（释放对象的空间不立即归还）、无 per-object 统计、长期进程堆只增不减（arena 页永驻）——对 VM 这种"启动后稳定"的长期服务是可接受的；若未来出现高频建毁模式，硬化轮可再评估（§6 过渡）。

代价与收益的对照在 §3.1 D1 展开，与 Redox/Linux 的对照在 §1.9。

### 1.9 对照 Redox / Linux

| 维度 | Minix3 slab（slaballoc.c） | minix-rs `VmAllocator` + `HeapArena` | Redox `linked_list_allocator` | Linux slab/slub |
|------|---------------------------|-------------------------------------|------------------------------|-----------------|
| 分配对象 | 固定尺寸类对象（8..207B） | 任意 `Layout`（bump 页内切割） | 任意大小（页内切割 + 空闲块链） | 内核对象缓存（kmalloc/专用 cache） |
| 元数据 | 页尾 `sdh`（位图/链表/魔数） | 无（bump cursor） | 空闲块链头 | per-CPU slab + 着色 |
| 复用 | 位图清位后复用 + 空页还回 | 无（dealloc no-op） | 空闲块合并复用 | 对象复用 + slab 收缩 |
| 防碎片 | 尺寸分类隔离 | 连续 VA 区间 + bump（无页内碎片） | 首次适配（可能有碎片） | 尺寸分类 + per-CPU |
| 同步 | 无（单线程 VM） | 无（单线程事件循环） | 无（单进程） | per-CPU + 锁 |
| 定位 | VM 服务自用 | `#[global_allocator]` 全局 | `GlobalAlloc` 全局 | 内核全局 |

对照要点：

- **Redox 同构**：Redox 的 `src/allocator/linked_list.rs` 用 `linked_list_allocator::Heap` + `GlobalAlloc`——先在**连续 VA 区间**上建立分配器（`HEAP_START`/`HEAP_SIZE`），物理帧由内核的 `FrameAllocator`（BumpAllocator/BuddyAllocator）独立管理。minix-rs 的 `VmAllocator`（VA 区间内切割）对应 Redox 的 GlobalAlloc 层，`VmPageAllocator`（06）对应 `FrameAllocator` 层——**"连续 VA 区间 + 独立物理帧分配器"是主流 OS 的两层结构**，minix-rs 的 HeapArena 正是这个结构的 VA 区间管理器。
- **Linux slab/slub**：通用内核对象缓存，per-CPU、着色、SMP 优化——它的存在理由（SMP 并发 + 内核全局高频 kmalloc）在单线程用户态服务器 VM 中不成立；Minix3 slab 是 32 位单线程朴素版（无 per-CPU/着色）。minix-rs 不实现 slab 不是"简化 Linux"，而是"工作负载与执行模型不匹配"。
- **Memkind/arena 惯例**：现代分配器普遍采用"保留连续 VA + 按需映射物理页"策略（`mmap` 保留 + `mprotect`/`mremap` 调整）——`HeapArena::grow`（逐页映射）与 arena 分配器的 lazy-commit 同构。

### 1.10 本章小结

- 堆是自举的：物理页来自 VM 自己的分配器（`vm_allocpage(VMP_SLAB)`），VA 连续性由专门区间保证。
- Minix3 slab = 尺寸分类 + 单页位图 + 空闲链，本质是 C 侧的对象生命周期簿记；`SLABALLOC`/`SLABFREE` 宏是 C 侧的类型化约定。
- MEMPROTECT 是调试期写保护硬化（默认关），生产语义等价于"页可写"。
- **[ARCH: A-3]** Rust 侧用 `HeapArena` + `VmAllocator`（bump）替代 slab——类型系统承担簿记，bump 承担分配，与 Redox 的"连续 VA + GlobalAlloc"同构。

---

## 2. C 源码分析

### 2.0 本章定位

本章逐函数/宏分析 slaballoc.c（528 行）全部语义单元 + proto.h 宏 + MEMPROTECT 依赖的 `vm_pagelock`。行号以 2026-08-15 grep 实证为准。函数/宏清单：

| 符号 | 行号 | 类别 | 用途 |
|------|------|------|------|
| `SLABSIZES`/`ITEMSPERPAGE`/`OBJALIGN`/`MINSIZE`/`MAXSIZE`/`USEELEMENTS` | 29/31/73/75/76/77 | 尺寸类常量 | 分类粒度定义 |
| `ELBITS`/`BITPAT`/`BITEL`/`GETBIT`/`SETBIT`/`CLEARBIT` | 33-35/69-71 | 位图宏 | 页内对象占用簿记 |
| `SLABDATAWRITABLE`/`SLABDATAUNWRITABLE`/`SLABDATAUSE` | 42-59（#else :63-65） | MEMPROTECT 宏 | 调试写保护临界区 |
| `struct sdh`/`struct slabdata`/`DATABYTES` | 96/118/111 | 数据结构 | 页尾头 + 数据区 |
| `MAGIC1`/`MAGIC2`/`JUNK`/`NOJUNK` | 113-116 | 魔数 | 结构校验 + 双释放检测 |
| `slabs[]` | 125 | 数据结构 | 200 尺寸类空闲链 |
| `GETSLAB`/`ADDHEAD`/`UNLINKNODE` | 130/140/151 | 链表宏 | 空闲链操作 |
| `newslabdata`（static） | 159 | 页供给 | `vm_allocpage(VMP_SLAB)` 取页 + 初始化 |
| `checklist`（static）/`slab_sanitycheck`/`slabsane_f` | 194/229/240 | SANITYCHECKS | 结构一致性审计 |
| `slaballoc` | 259 | 分配 | 对象分配主路径 |
| `objstats`（static inline） | 344 | 反查 | 对象 → slab/尺寸类/对象号 |
| `slabfree` | 406 | 释放 | 对象释放 + 页回收/回链 |
| `slablock`/`slabunlock` | 464/483 | MEMPROTECT | 对象整页只读/可写翻转 |
| `slabstats` | 504 | 统计 | 利用率审计（n%1000 节流） |
| `SLABALLOC`/`SLABFREE` | proto.h:133-134 | 宏 | 类型化分配/释放 |
| `vm_pagelock` | pagetable.c:403 | 依赖 | PTE RW 位翻转 |

（`vm_allocpage`/`vm_freepages` 的物理页语义在 06 分析；本章只引用其调用点。）

### 2.1 尺寸类与常量（slaballoc.c:29-90）

```c
#define SLABSIZES 200                 /* :29 尺寸类个数 */
#define ITEMSPERPAGE(bytes) (int)(DATABYTES / (bytes))   /* :31 页内对象数 */
#define ELBITS    (sizeof(element_t)*8)                  /* :33 8 */
#define BITPAT(b) (1UL << ((b) %  ELBITS))               /* :34 位掩码 */
#define BITEL(f, b) (f)->sdh.usebits[(b)/ELBITS]         /* :35 元素索引 */
#define OBJALIGN  8                   /* :73 对齐边界 */
#define MINSIZE 8                     /* :75 最小对象 */
#define MAXSIZE (SLABSIZES-1+MINSIZE) /* :76 207 —— 最大对象 */
#define USEELEMENTS (1+(VM_PAGE_SIZE/MINSIZE/8))         /* :77 位图元素数 */
```

- 尺寸类索引 = `bytes - MINSIZE`（`GETSLAB` :130-138 断言 `b >= MINSIZE && b-MINSIZE < SLABSIZES`），覆盖 8..207 字节。
- `USEELEMENTS = 1 + VM_PAGE_SIZE/8/8 = 1 + 64`（4KB 页）——位图 65 元素 × 8 bit = 520 bit，覆盖理论上最多 512 个最小对象（`VM_PAGE_SIZE/MINSIZE = 512`，按无头页的极端情形）的占用位，`+1` 是余量；实际对象数由 `ITEMSPERPAGE(bytes) = DATABYTES/bytes`（:31）决定（生产配置 `SANITYCHECKS=0`（vm.h:8）下 `sdh` 无魔数/写保护字段，占 112 字节 → 8 字节对象 498 个/页；开 `SANITYCHECKS=1` 则为 120 字节 → 497 个/页）。
- `MAXSIZE = 207` 的命名有误导性：`bytes` 经 `roundup(bytes, OBJALIGN)` 后**可等于 208**（207 对齐到 208），此时 `GETSLAB` 的 `_gsi = 200` 触发断言失败——slaballoc 对 >207 字节请求没有优雅失败路径，是 C 侧硬限制（调用方约定不超 MAXSIZE）。
- `pages`（:79）全局统计 slab 页总数，供 `slabstats` 利用率计算。

### 2.2 数据结构：struct sdh / struct slabdata / slabs[]（slaballoc.c:96-127）

```c
struct sdh {                          /* :96 页尾头 */
#if SANITYCHECKS                      /* vm.h:8 默认 0（生产） */
    u32_t magic1;                     /* SANITYCHECKS 魔数 */
#endif
    int freeguess;                    /* 下一分配扫描起点提示 */
    struct slabdata *next, *prev;     /* 双向空闲链 */
    elements_t usebits;               /* 页内对象位图 */
    phys_bytes phys;                  /* 本页物理地址（审计/登记） */
#if SANITYCHECKS
    int writable;                     /* MEMPROTECT：对象号或 WRITABLE_* */
    u32_t magic2;                     /* SANITYCHECKS 魔数 */
#endif
    u16_t nused;                      /* 已用对象数 */
};
#define DATABYTES (VM_PAGE_SIZE-sizeof(struct sdh))   /* :111 数据区大小 */

struct slabdata {                     /* :118 整页布局 */
    u8_t data[DATABYTES];             /* 对象数据区（页首） */
    struct sdh sdh;                   /* 页尾头 */
};
static struct slabheader {
    struct slabdata *list_head;
} slabs[SLABSIZES];                   /* :125 200 个尺寸类空闲链头 */
```

关键设计：**头在页尾**——`objstats` 用 `(struct slabdata*)(mem - mem%VM_PAGE_SIZE)` 页对齐取头，数据区在页首使对象地址对齐到页基址 + 8 的倍数；`assert(sizeof(*n) == VM_PAGE_SIZE)`（:164）保证整页布局。

### 2.3 位图与链表宏（slaballoc.c:69-71 / 130-157）

```c
#define GETBIT(f, b)   (BITEL(f,b) &   BITPAT(b))
#define SETBIT(f, b)   {OFF(f,b); SLABDATAUSE(f, BITEL(f,b)|=BITPAT(b); (f)->sdh.nused++;); }
#define CLEARBIT(f, b) {ON(f, b); SLABDATAUSE(f, BITEL(f,b)&=~BITPAT(b); (f)->sdh.nused--; (f)->sdh.freeguess=(b);); }
```

- `OFF`/`ON`（:37-38）是断言：`SETBIT` 前断言位为 0（防双分配）、`CLEARBIT` 前断言位为 1（防双释放）——C 侧在**位图操作点**上做双分配/双释放检测，配合 JUNK 标记。
- `SLABDATAUSE` 包裹位图 + 计数更新——MEMPROTECT 下临时开写页（§1.6）。
- `GETSLAB`（:130）：尺寸 → 尺寸类链头，含范围断言。
- `ADDHEAD`（:140）：头插；`UNLINKNODE`（:151）：双向摘除。链表写全部经 `SLABDATAUSE` 保护。

### 2.4 newslabdata：堆页供给（slaballoc.c:159-192）

```c
static struct slabdata *newslabdata(void)
{
    phys_bytes p;
    if(!(n = vm_allocpage(&p, VMP_SLAB))) {   /* :166 物理页 + 映射 */
        printf("newslabdata: vm_allocpage failed\n");
        return NULL;
    }
    memset(n->sdh.usebits, 0, sizeof(n->sdh.usebits));
    pages++;                                    /* :171 全局计数 */
    n->sdh.phys = p;
    /* SANITYCHECKS: magic1/magic2/writable 初始化 */
    n->sdh.nused = 0;
    n->sdh.freeguess = 0;
    return n;
}
```

- `vm_allocpage(&p, VMP_SLAB)` 返回页的**虚拟地址**（`n`），`p` 带回物理地址——06 的"分配 + 映射"语义；`VMP_SLAB`（vm.h:52）标记用途。
- 只清零位图（数据区不清零——首次分配的对象是旧内容，调用方负责初始化；调试下 slaballoc 写 NOJUNK 标记新对象）。
- `assert(sizeof(*n) == VM_PAGE_SIZE)`（:164）——整页布局不变量。

### 2.5 slaballoc：分配路径（slaballoc.c:259-343）

```c
void *slaballoc(int bytes)
{
    bytes = roundup(bytes, OBJALIGN);
    SLABSANITYCHECK(SCL_FUNCTIONS);
    GETSLAB(bytes, s);
    if(!(newslab = s->list_head)) {             /* 链空 → 新页 */
        newslab = newslabdata();
        if(!newslab) return NULL;
        ADDHEAD(newslab, s);
        assert(newslab->sdh.nused == 0);
    } else assert(newslab->sdh.nused > 0);
    assert(newslab->sdh.nused < ITEMSPERPAGE(bytes));

    for(i = newslab->sdh.freeguess;             /* freeguess 起扫描 */
        count < ITEMSPERPAGE(bytes); count++, i++) {
        i = i % ITEMSPERPAGE(bytes);
        if(!GETBIT(newslab, i)) break;
    }
    assert(count < ITEMSPERPAGE(bytes));

    SETBIT(newslab, i);
    if(newslab->sdh.nused == ITEMSPERPAGE(bytes)) {   /* 满 → 摘出 */
        UNLINKNODE(newslab);
        s->list_head = newslab->sdh.next;
    }
    ret = ((char *) newslab) + i*bytes;

    /* SANITYCHECKS: NOJUNK + slabunlock/slablock（MEMPROTECT） */
    SLABDATAUSE(newslab, newslab->sdh.freeguess = i+1;);
    return ret;
}
```

- 链头 slab **保证未满**（`nused < ITEMSPERPAGE`）：满 slab 在 SETBIT 后被摘出，空 slab 在 slabfree 后回链——链上只有"部分占用"的 slab。
- `freeguess` 提示 + 线性扫描：分配摊销 O(1)（通常第一个 bit 就空）。
- 满 slab 摘链的判定在 SETBIT 后（`nused == ITEMSPERPAGE`）——释放侧对称（§2.7 的 `nused == ITEMSPERPAGE-1` 回链）。
- 返回前 `freeguess = i+1`——下次分配从刚分配位置之后开始，减少与刚释放对象的冲突。

### 2.6 objstats：反查定位（slaballoc.c:344-405）

```c
static inline int objstats(void *mem, int bytes,
    struct slabheader **sp, struct slabdata **fp, int *ip)
{
    GETSLAB(bytes, s);
    f = (struct slabdata *)((char *) mem - (vir_bytes) mem % VM_PAGE_SIZE);  /* :376 页对齐取头 */
    /* OBJSTATSCHECK：魔数 / 范围（data..data+DATABYTES）/ 对齐（mem % bytes == 0）/ 占用位 */
    i = (char *) mem - (char *) f->data;
    i = i / bytes;
    OBJSTATSCHECK(GETBIT(f, i));      /* 必须标记为已分配 */
    *ip = i; *fp = f; *sp = s;
    return OK;
}
```

- 从任意对象指针反查三要素（slab/尺寸类/对象号）——`slabfree`/`slabsane_f`/`slablock`/`slabunlock` 全部依赖它。
- 校验项（SANITYCHECKS 门控）：魔数、对象在数据区范围内、对象号整除（对齐）、占用位为 1（**释放未分配对象 → 检测**）。
- `OBJSTATSCHECK` 失败在 SANITYCHECKS 下打印 + 返回 EINVAL，slabfree 收到非 OK → `panic`。

### 2.7 slabfree：释放路径（slaballoc.c:406-463）

```c
void slabfree(void *mem, int bytes)
{
    bytes = roundup(bytes, OBJALIGN);
    if(objstats(mem, bytes, &s, &f, &i) != OK) panic("slabfree objstats failed");
    /* SANITYCHECKS：JUNK 检测（double free）/ memset(0xa6) / JUNK 写入 + lock/unlock */
    CLEARBIT(f, i);                                /* :443 */
    if(f->sdh.nused == 0) {                        /* 页空 → 还页 */
        UNLINKNODE(f);
        if(f == s->list_head) s->list_head = f->sdh.next;
        vm_freepages((vir_bytes) f, 1);            /* :449 */
    } else if(f->sdh.nused == ITEMSPERPAGE(bytes)-1) {  /* 从满变非满 → 回链 */
        ADDHEAD(f, s);
    }
}
```

- 释放后对象内容：SANITYCHECKS 下写 `JUNK`（0xdeadbeef）——二次释放时 objstats 的 JUNK 检测 + `CLEARBIT` 的 ON 断言双保险；`memset(0xa6)`（JUNKFREE 门控）清残留数据。
- **空页还回物理分配器**（`vm_freepages`）是 slab 的内存收缩机制——VM 堆页不长期驻留。
- 从满变非满回链：slaballoc 摘满链的对称操作，保证"链上只有未满 slab"不变量。

### 2.8 slablock/slabunlock + vm_pagelock：MEMPROTECT 机制（slaballoc.c:464-502 + pagetable.c:403）

```c
#if MEMPROTECT
void slablock(void *mem, int bytes)     /* :464 —— 翻只读 */
{ objstats(...); SLABDATAUNWRITABLE(f); }
void slabunlock(void *mem, int bytes)   /* :483 —— 翻可写 */
{ objstats(...); SLABDATAWRITABLE(f, i); }
#endif
```

`vm_pagelock`（pagetable.c:403-431）核心：

```c
flags = ARCH_VM_PTE_PRESENT | ARCH_VM_PTE_USER;   /* 初始无 RW */
if(!lockflag) flags |= ARCH_VM_PTE_RW;             /* unlock → 加 RW */
pt_writemap(vmprocess, pt, m, 0, VM_PAGE_SIZE, flags,
    WMF_OVERWRITE | WMF_WRITEFLAGSONLY);   /* pagetable.c:427 实际传双标志 */
```

- `WMF_WRITEFLAGSONLY`（08 D1）保留物理地址只改标志——`vm_pagelock` 是 MEMPROTECT 的 PTE 执行者。
- slablock/slabunlock 的调用点：slaballoc 返回前 `slabunlock(ret, bytes)`（对象可写）、slabfree 中先 `slabunlock` 再写 JUNK 再 `slablock`（页回写保护）、SLABDATAUSE 临界区。
- 注意 **MEMPROTECT 默认 0**（vm.h:13）：生产构建中 slablock/slabunlock/SLABDATA* 全部展开为空，`writable` 字段与翻转逻辑不存在。

### 2.9 slabstats/slab_sanitycheck/slabsane_f：调试审计（slaballoc.c:194-256 / 504-528）

- `checklist`（static，:194）：遍历单尺寸类链表，校验魔数、双向链一致性、位图-计数一致（`count == nused`）、`usedpages_add(phys)` 登记；返回已用对象数。
- `slab_sanitycheck(file, line)`（:229）：200 个尺寸类全量 `checklist`——SANITYCHECK 框架入口（sanitycheck.h 的 `SLABSANITYCHECK` 门控）。
- `slabsane_f(file, line, mem, bytes)`（:240）：单对象合法性（roundup 后 objstats == OK）——slaballoc 返回前调用，检测"分配出的对象落在非法位置"。
- `slabstats()`（:504）：`n++` 每 1000 次才打印（`n%1000` 节流）；逐尺寸类打印 `VMSTATS: %2d slabs: %d (%dkB)` + 总利用率 `100*totalbytes/(pages*VM_PAGE_SIZE)`。

### 2.10 SLABALLOC/SLABFREE 宏与消费方全景

proto.h:133-134（§1.5 已述）。消费方实证（2026-08-15 `rg SLABALLOC|SLABFREE *.c`）：

| 消费方文件 | 调用点 | 对象 |
|-----------|--------|------|
| `vfs.c` | :76/:135 | VFS 请求节点（reqnode/orignode） |
| `pb.c` | :36/:58/:78/:128 | phys_block + phys_region |
| `region.c` | :432/:451/:559/:581 | vir_region（mmap/munmap/fork 建毁） |
| `cache.c` | :227/:282/:285 | page cache 元数据（hb/cp/pb） |
| `fdref.c` | :97/:140 | 文件描述符引用 |

这些对象在 Rust 侧全部是普通 `struct`（`Box`/`Vec` 分配）——**09 的替换不要求消费方感知分配器**，这是 ARCH A-3 的"无侵入"特性（§3.1）。

### 2.11 本章小结

- slaballoc.c 是自包含的尺寸分类分配器：常量（:29-90）、结构（:96-127）、宏（:69-71/:130-157）、页供给（:159）、分配（:259）、反查（:344）、释放（:406）、调试（:194-256/:464-502/:504-528）。
- 两个核心不变量：**链上只有未满 slab**（满摘空回）、**位图与 nused 一致**（SETBIT/CLEARBIT 同更新）。
- MEMPROTECT 依赖 08 的 `WMF_WRITEFLAGSONLY`（`vm_pagelock`）——08 的操作面在这里被调试硬化消费。

---

## 3. Rust 设计决策

> 本章为设计决策正文版（D1-D6 与 plan.md §7.3 ARCH A-3 对应），代码侧标注 `[ARCH: A-3]`。设计契约快照按项目规范存于独立目录，正文不引用。

### 3.1 D1: slab → VmAllocator bump + HeapArena（ARCH A-3 主决策）

- **C**：slaballoc.c 的 200 尺寸类 + 页内位图 + 空闲链（§2.1-§2.7）。
- **Rust**：`VmAllocator`（global.rs）——连续 VA 区间（arena）内游标 bump：

```text
Box::new → GlobalAlloc::alloc → VmAllocator::alloc
    → bump within current arena（cursor + align_offset）
    → arena exhausted? → refill_arena()
        → HEAP_ARENA.grow(ARENA_PAGES=16, page_alloc)   // 64KB 新区块
        → new arena_base = HeapArena VA
```

- **行为契约**：分配按 `layout.align()` 对齐（`align_offset`）；arena 耗尽自动 refill（`alloc` 递归一次）；refill 失败（物理页不足 / `PAGE_ALLOC_PTR` 未注册）→ 返回 `null_mut`（`Box` 会 abort）；`dealloc` no-op——对象释放不归还空间。
- **差异**：尺寸分类/位图/空闲链/复用 → 结构性省略；`SLABALLOC/SLABFREE` 类型化宏 → `Box::new/drop`（编译器保证类型与释放）；C 的按对象计数 → 无统计。
- **理由**：§1.8 因果链——类型系统承担簿记、分配模式为启动期集中 + 长期稳定、64 位地址空间充裕。**代价**：无对象复用（bump 单调递增）、堆只增不减——对"启动后稳定"的长期服务可接受；Redox 的 `linked_list_allocator` 提供空闲块复用是另一种取舍（§1.9），minix-rs 选 bump 是因为 VM 无高频建毁模式。

### 3.2 D2: 物理页碎片化 → HeapArena 连续 VA（三 VA 模型）

- **C**：slab 页映射在 VM 地址空间线性区，VA 连续性由 32 位线性映射天然保证。
- **Rust**：`HeapArena`（heap_arena.rs）保留连续 VA 区间（`VM_HEAP_BASE .. VM_HEAP_BASE + VM_HEAP_SIZE`，direct_map.rs:16-18），物理页逐页 `vm_self_mappages`（vm_self_map.rs:113）映射——**物理页可碎片化，VA 恒连续**。
- **三 VA 模型**：一物理页用作堆时有至多三个虚拟地址：内核 Direct Map（`KERNEL_DIRECT_MAP_BASE + phys`，Ring 0）、VM Direct Map（`VM_DIRECT_MAP_BASE + phys`，Ring 3）、HeapArena VA（仅堆页，Ring 3）。
- **行为契约**：`grow(pages)` 越界 → `Exhausted{requested, available}`；中途映射失败 → 回滚已映射页（`vm_self_unmap` + `free_page`）+ 释放当前页，返回 `MapFailed`（**all-or-nothing**）；`shrink(pages)` 超量 → `Underflow`；零页 → `ZeroPages`；成功 grow 返回新区间起始 VA（= 旧 limit）。
- **差异**：C 的"堆在 VM 线性区"→ Rust 的"专用 HeapArena 区间 + 自映射"——`[ARCH: A-3]` 的 VA 连续化手段，与 Redox 的 `HEAP_START/HEAP_SIZE` 保留区间同构。

### 3.3 D3: 无递归（自举安全）

- **C**：`newslabdata` 的 `vm_allocpage`（:166）→ `vm_mappages` → `pt_writemap` → `pt_ptalloc` 分配页表页——VM 自身堆映射与页表页分配交织，存在 32 位动态映射的递归副作用（08 §2.1 已述）。
- **Rust**：`HeapArena::grow` 经 `vm_self_mappages`（VM 自身页表已由 `init_vm_self_pt` 建立）写 PTE；页表页经 Direct Map 访问（稳定 VA），**堆映射不依赖堆**——递归路径结构性消失（与 08 D2 同因，本文档引用）。
- **自举安全**：`HeapArena` 仅 3 个 `u64` 字段（`base`/`limit`/`top`，limit 用 `AssumeSyncCell`），`HeapArena::new()` 是 const fn——可作 `static`（BSS 构造），全局分配器初始化不依赖自身分配。
- **行为契约**：`grow` 内不调用 `GlobalAlloc`（无自引用）；`vm_self_mappages` 在 `init_vm_self_pt()` 之前调用 → panic（fail-fast，vm_self_map.rs 注释）。

### 3.4 D4: MEMPROTECT 硬化 → 移交硬化项

- **C**：MEMPROTECT（vm.h:13）+ `SLABDATA*` 宏（:44-65）+ `slablock`/`slabunlock`（:464/:483）+ `vm_pagelock`（pagetable.c:403）。
- **Rust**：`HeapArena::grow` 恒以 `PageFlags::read_write()` 映射（heap_arena.rs）——**未实现 PTE 写保护**。诚实标注：这是移交硬化项（§6 过渡），不是"实现简化"。
- **行为契约**（未实现，硬化实施时预期）：debug-only `protect` API 或独立标志；经 08 D1 的 `update_flags`（vm_self_update_flags）翻转 PTE RW；与 C 的 WMF_WRITEFLAGSONLY 语义一致。
- **为何可延后**：MEMPROTECT 是 SANITYCHECKS 门控的调试特性（默认 0），生产语义等价于"页可写"；Rust 的借用检查在**编译期**消除了 C 在**运行期**靠写保护检测的越权写/释放后使用——硬化收益在 Rust 侧大幅下降。

### 3.5 D5: slabstats/SANITYCHECKS 调试 → 延后

- **C**：slabstats（:504，n%1000 节流）、slab_sanitycheck/slabsane_f/checklist（:194-256）、JUNK/NOJUNK（:113-116）。
- **Rust**：无对应——内存占用统计归 26（`get_usage_info` 语义）；结构一致性 sanity 归 07-P2-9 延后原则（sanity.rs 注释）。
- **行为契约**：`#[cfg_attr(not(test), global_allocator)]`——测试构建下 `GLOBAL` 不注册为全局分配器（用 std 分配器），避免测试间全局状态污染。
- **差异**：C 的调试审计网络 → Rust 以类型系统（无 use-after-free/双释放的表达空间）+ 单元测试替代；这是"正确性手段转移"，不是"少了功能"。

### 3.6 D6: 生命周期接线（PAGE_ALLOC_PTR + register/unregister）

- **C**：`slabs[]`/`pages` 是静态全局，`newslabdata` 直接调 `vm_allocpage`（全局函数，无注册）。
- **Rust**：`GlobalAlloc::alloc(&self)` 无 `&mut` 参数 → `VmAllocator` 无法直接持有 `VmPageAllocator`；`PAGE_ALLOC_PTR: AtomicPtr<VmPageAllocator>`（global.rs）作为间接层：
  - `register_page_alloc(alloc)`（global.rs:415）：`compare_exchange(null → ptr)`——首次注册正常；同指针幂等重注册；不同指针 → 双初始化 bug 检测。**BSS → 堆搬迁场景为防御性设计**：当前 `main.rs` 中 `VmServer` 在栈上创建，无搬迁发生；代码注释保留该路径以防未来 VmServer 被 `Box` 化。
  - `unregister_page_alloc()`（global.rs:451）：VmServer::drop 置 null（vm_server.rs:800）。
  - `refill_arena`：读指针，null → 返回 false（`alloc` 返回 null）。
- **行为契约**：注册必须先于首次分配（`VmServer::new`，vm_server.rs:92）；`page_alloc_mut()` 对 null 指针 panic（fail-fast）；指针生命期 = VmServer 生命期 ≥ GLOBAL 生命期（SAFETY 注释链，global.rs）。
- **执行模型**：VM 是用户态服务器单线程事件循环（CLAUDE.md Execution Model）——`AtomicPtr` + `AssumeSyncCell` 足够，无需 Mutex；与 kernel SMP 的 BKL 约束正交。

### 3.7 语义差异清单（C ↔ Rust 诚实标注）

| # | C 行为 | Rust 行为 | 类型 |
|---|--------|----------|------|
| 1 | `slaballoc`/`slabfree`（尺寸分类 + 位图复用） | `GlobalAlloc::alloc/dealloc`（bump，dealloc no-op） | 结构性替代（D1，ARCH A-3） |
| 2 | `SLABALLOC`/`SLABFREE` 宏（类型化 + 置 NULL） | `Box::new`/`drop`（编译器保证类型/释放） | 语义对应（D1） |
| 3 | `slabs[]`/`struct sdh`/`usebits` 空闲链 + 位图 | `HeapArena`（base/limit/top）+ cursor | 结构性替代（D1/D2） |
| 4 | `newslabdata`/`vm_freepages`（按页取/还） | `HeapArena::grow/shrink`（按 arena 批量 + 回滚） | VA 区间管理（D2/D3） |
| 5 | `vm_allocpage(VMP_SLAB)` 供页 | `VmPageAllocator::alloc_phys` | 语义等价（06） |
| 6 | MEMPROTECT + `slablock`/`slabunlock`/`vm_pagelock` | 未实现（硬化项移交） | 移交（D4） |
| 7 | `slabstats`（n%1000 利用率） | 无（归 26 内存占用统计） | 延后（D5） |
| 8 | JUNK/NOJUNK + `slabsane_f`（双释放检测） | 无（类型系统排除） | 正确性手段转移（D5） |
| 9 | 静态全局 `slabs[]` 直接可达 | `PAGE_ALLOC_PTR` 显式注册/注销 | 执行模型差异（D6） |
| 10 | 32 位线性映射保证 VA 连续 | `HeapArena` 专用区间保证 VA 连续 | 结构差异（D2，ARCH A-3） |
| 11 | `newslabdata` 递归副作用（页表页分配交织） | 无（Direct Map 消除递归路径） | 结构消除（D3） |
| 12 | >207 字节请求断言失败 | 任意大小（arena 内切割，`ARENA_BYTES` 上限内） | 语义改进（D1） |

类型说明：**结构性替代** = C 机制整体被 Rust 分配体系替代（ARCH 标注）；**语义对应** = 语义有等价对应但表达方式不同；**结构消除** = C 机制被 Direct Map/64 位替代；**执行模型差异** = VM 单线程 vs C 全局可达。

---

## 4. 实现详解

### 4.1 `HeapArena`：连续 VA 区间管理（heap_arena.rs）

**结构**（heap_arena.rs:46-50）：

| 字段 | 语义 |
|------|------|
| `base: u64` | 区间起始（= `VM_HEAP_BASE`，BSS 构造） |
| `limit: AssumeSyncCell<u64>` | 当前已映射上界（= 下次 grow 起点） |
| `top: u64` | 区间上限（= `VM_HEAP_LIMIT`） |

**方法**：

- `grow(pages, page_alloc)`（heap_arena.rs:91）：越界检查（`Exhausted`）→ 逐页 `alloc_phys(1)` + `vm_self_mappages(va, phys, read_write())` → 失败回滚（已映射页 `vm_self_unmap` + `free_page`，再释放当前页，返回 `MapFailed`）→ 成功推进 `limit`，返回旧 limit（新区间起始 VA）。
- `shrink(pages, page_alloc)`（heap_arena.rs:141）：`Underflow` 检查 → 从新区间起始逐页 `vm_self_unmap`（返回 PhysBytes）+ `free_page` → 回退 `limit`。**当前无生产调用方**（`rg "\.shrink\(" os/servers/vm/src/` 仅定义处命中）——保留为 HeapArena 契约面，预期 10 元数据搬迁消费；doc §5.3 测试缺口含 shrink。
- `available_va()`/`mapped_bytes()`：区间状态查询（bump 的 refill 决策用）。

**关键设计**：

1. **all-or-nothing 回滚**：grow 的失败路径完整回滚已映射页——不会留下"部分映射 + limit 未推进"的中间态（对应 C 侧 slaballoc 无失败中间态）。
2. **无 `assert_eq!` 连续性检查**：grow 映射的物理页来自 `alloc_phys` 逐页分配（可碎片），VA 由 `old_limit + i*PAGE_SIZE` 计算——**VA 连续性由算术保证**，无需检查物理页连续性（物理页本来就不连续）。
3. **shrink 的 `Err(_) => {}` 吞错**：`vm_self_unmap` 失败（本应已映射的页未映射）不阻断——推进 limit 仍正确（该页 VA 本来就没映射）；这是"容忍异常页表状态"的选择，硬化轮可改为 fail-fast。
4. **`AssumeSyncCell` 而非 Mutex**：单线程 VM 事件循环（CLAUDE.md Execution Model），`limit` 读写无需锁。
5. **测试性**：`HeapArena::new()` const fn + 纯字段查询——常量边界测试（heap_arena.rs tests）零依赖可跑。

### 4.2 `VmAllocator`：bump 分配器 + GlobalAlloc（global.rs）

**结构**（global.rs:395-398）：

| 字段 | 语义 |
|------|------|
| `arena_base: AssumeSyncCell<*mut u8>` | 当前 arena 起始（HeapArena 返回的 VA） |
| `cursor: AssumeSyncCell<usize>` | arena 内偏移游标 |

**分配路径**（`unsafe impl GlobalAlloc`，global.rs:531-579）：

```text
alloc(layout)
  ├─ size > ARENA_BYTES（64KB）？ → 返回 null（fail-fast，不消耗堆）
  ├─ ensure_arena()：arena_base 为 null → refill_arena()
  ├─ ptr = base + cursor；offset = ptr.align_offset(align)
  ├─ cursor + offset + size ≤ ARENA_BYTES（64KB）？ → cursor += total，返回
  └─ 否则 → refill_arena()（新 arena）→ 递归 alloc(layout)
```

- `ARENA_PAGES = 16`（global.rs:489）→ `ARENA_BYTES = 64KB`/块（:490）。
- `refill_arena()`（global.rs:492）：读 `PAGE_ALLOC_PTR`（null → false）→ `HEAP_ARENA.grow(16, alloc)` → 新 `arena_base` + `cursor = 0`。
- `dealloc` no-op（global.rs:574-578）：bump 不回收——注释明示"VM 生命周期内 arena 页永驻"。
- **`#[cfg_attr(not(test), global_allocator)] static GLOBAL`**（global.rs:581-585）：测试构建不注册（避免 std 测试环境冲突）。

**边界行为**：

- 对齐：`align_offset` 满足任意 `layout.align()`（bump 的经典对齐方案）。
- 跨块：当前 arena 放不下（`size ≤ ARENA_BYTES` 但对齐后越界）→ refill 换新块重试一次（递归深度 1）；refill 失败 → `null_mut`。
- **超大分配 fail-fast**：`size > ARENA_BYTES`（64KB）→ 直接返回 `null_mut`——否则 `cursor + total > ARENA_BYTES` 会反复 refill 直到耗尽整个 HeapArena（64MB）再失败；guard 在 global.rs:538-546（2026-08-15 review 修复，P1）。
- `layout.size() == 0`：GlobalAlloc 约定调用方保证非零（`Box` 保证），bump 仍返回合法指针（不特殊处理）。

### 4.3 `PAGE_ALLOC_PTR` 接线（global.rs + vm_server.rs）

- 注册：`VmServer::new()` 内 `register_page_alloc(&mut page_alloc)`（vm_server.rs:92）。
- 注销：`VmServer::drop()` 内 `unregister_page_alloc()`（vm_server.rs:800）。
- `compare_exchange` 语义（global.rs:429-447，`register_page_alloc` :415-447）：null → ptr 正常；同 ptr 幂等（防御 BSS → 堆搬迁，当前 main() 栈上创建 VmServer 无搬迁）；异 ptr → 双初始化 bug 检测（注释明示）。
- **生命期论证**（global.rs 注释链）：`GLOBAL` 是 `static`（程序整个生命期），`VmPageAllocator` 由 `VmServer` 持有（VmServer 生命期 = VM 进程生命期）→ 供页者不短于消费者。

### 4.4 启动时序（init 链）

Rust 侧堆自举的完整顺序（对应 C 侧 `init_vm()`）：

```text
VmServer::new()                                    ← 生产路径（main.rs，main()）
  ├─ params.validate()（boot.rs:144，纯断言无分配）
  ├─ create_default_allocator + VmPageAllocator::new（vm_server.rs:90-91，无堆分配）
  ├─ register_page_alloc(&mut page_alloc)          ← ★ 供页注册（vm_server.rs:92，global.rs:415）
  ├─ pt_alloc::register(vm_pt_alloc)               ← 页表页供给钩子（vm_server.rs:103）
  ├─ init_vm_self_pt()                             ← ★ 映射能力就绪（vm_server.rs:111，07/08 页表面）
  └─ ... 其余字段初始化 ...
VmServer::init()
  ├─ relocate()（vm_server.rs:257）                ← 首次 heap_arena_grow（元数据搬迁，10）
  └─ ... init_global_state / init_vm_slot ...
运行期：首次 Box::new → GLOBAL.alloc → refill_arena → HEAP_ARENA.grow(16)
```

**分界线语义**（对照 §1.1）：C 的 `__minix_init()`（libc 构造）与 Rust 的"供页注册 + 映射就绪"不同时刻——Rust 侧真正的前置是 **`register_page_alloc`（供页）+ `init_vm_self_pt`（映射）两者齐备**：`register_page_alloc` 在 `init_vm_self_pt` 之前（vm_server.rs:92 先于 :111），但两者之间无任何堆分配（`params.validate`/`create_default_allocator` 均无分配），首次 `GLOBAL.alloc` 必然发生在两者之后，故安全；libc 构造语义在 Rust 中不存在（无 libc 依赖的启动）。

### 4.5 消费链与边界

- **消费方**：`vm_server.rs:195` `heap_arena_grow(pages, &mut self.page_alloc)`（VM_HEAP 服务 / 压力计数路径）；一切 Rust 侧 `Box`/`Vec`/`String` → `GLOBAL` bump。
- **边界**：`HeapArena` 不直接暴露给服务层（`pub(crate)`），经 `heap_arena_grow`（global.rs:481）转发；`VmAllocator` 仅以 `GLOBAL` 静态存在，无第二实例。
- **与 08 的接缝**：`grow/shrink` 消费 08 的 `vm_self_mappages`/`vm_self_unmap`/`vm_self_unmappages`（vm_self_map.rs:113/129/151）——08 是"怎么改 VM 自身页表"，09 是"VM 的堆怎么经这些 API 自举"。

---

## 5. 测试要点

### 5.1 单元测试清单

**os/servers/vm 侧（`cargo test -p minix-vm --lib`）**：

| 测试 | 位置 | 验证目标 |
|------|------|---------|
| `test_heap_arena_constants`（1 个） | heap_arena.rs tests | base/top/limit/mapped_bytes/available_va 常量边界 |
| **HeapArena 行为 6 个（新增，本文档补）**：`test_grow_advances_limit_and_maps`/`test_grow_zero_pages_is_error`/`test_grow_exhausted_reports_remaining`/`test_grow_rolls_back_on_map_failure`/`test_shrink_unmaps_and_frees_pages`/`test_shrink_underflow_is_error` | heap_arena.rs tests | grow 推进 limit + 映射可查询（RW 标志）、ZeroPages/Exhausted/Underflow 错误、**失败回滚**（预映射第二页 → MapFailed → 第一页 unmap + limit 不动）、shrink 逐页 unmap + free |
| global 状态 4 个（`test_boot_image_empty`/`test_boot_image_name`/`test_vm_instance_count`/`test_kernel_layout_*`） | global.rs tests | BOOT_INFO/VM_INSTANCE_COUNT/KERNEL_LAYOUT 读写 |
| **VmAllocator bump 6 个（新增，本文档补）**：`test_bump_alignment_and_no_overlap`/`test_bump_oversize_returns_null`/`test_bump_refill_failure_returns_null`/`test_bump_refill_via_heap_arena`/`test_register_page_alloc_idempotent`/`test_register_page_alloc_overwrite_panics` | global.rs tests | 对齐/不重叠、超大分配 fail-fast、refill 失败 null、**全链 refill**（MockPaging + 真实 HeapArena）、注册幂等/覆盖 panic |
| vm_self_map 2 个（`test_vm_self_pt_not_initialized_by_default`/`test_init_vm_self_pt_then_reset`） | vm_self_map.rs | 静态存储初始 None + 单次 init/reset 语义（grow 前置契约） |

### 5.2 覆盖维度

- **HeapArena 常量契约**：base = VM_HEAP_BASE、top = base + VM_HEAP_SIZE、初始 limit = base、mapped_bytes = 0、available_va = VM_HEAP_SIZE——区间不变量（D2）。
- **HeapArena 行为契约**：grow 成功推进 limit + 映射可查询（RW 标志）+ 可续 grow（旧 limit 起点）、`ZeroPages`/`Exhausted{requested,available}`/`Underflow`、**失败 all-or-nothing 回滚**（预映射冲突 → MapFailed → 已映射页 unmap + limit 不动）、shrink 从高端逐页 unmap + free——D2/D3 行为契约全覆盖（MockPaging + 真实 HeapArena 静态）。
- **VmAllocator bump**：对齐（8/32 字节）、连续分配不重叠、**超大分配（> 64KB）fail-fast null**（guard）、refill 失败（PAGE_ALLOC_PTR null）null、**全链 refill**（MockPaging + 真实 HeapArena + 真实 VmPageAllocator）——D1 行为契约 + 2026-08-15 修复项。
- **注册语义**：同指针幂等（防御路径）、异指针 panic（双初始化检测）——D6。
- **全局状态**：boot image 查找/写入、VM 实例计数增减、kernel layout 的 set/get + panic 前置——D6 的接线前置。
- **`vm_self_mappages` 前置 panic + 单次 init**：`init_vm_self_pt` 未调用时 fail-fast、重复调用 panic、reset 语义——D3 的自举安全契约。

### 5.3 覆盖缺口与诚实标注

| 缺口 | 说明 | 状态 |
|------|------|------|
| HeapArena grow/shrink 行为测试 | `grow` 成功推进 limit/返回 VA、`Exhausted`、`ZeroPages`、`shrink` `Underflow`、失败回滚 | ✅ 已补（2026-08-15 review，§5.1 新增 6 个，MockPaging 路径） |
| VmAllocator bump 语义测试 | 对齐/连续分配不重叠/arena 耗尽 refill/refill 失败 null/超大 fail-fast | ✅ 已补（2026-08-15 review，§5.1 新增 6 个，含全链 refill） |
| register_page_alloc 幂等/覆盖检测 | compare_exchange 语义（同指针幂等/异指针检测） | ✅ 已补（2026-08-15 review，§5.1 新增 2 个） |
| `HeapArena::shrink` 生产调用方 | 当前无调用方（`rg "\.shrink\("` 仅定义处命中） | 10 承接（§4.1 已注） |
| MEMPROTECT 硬化 | 未实现（D4），无测试 | 硬化轮承接 |
| 生产路径（真实 HeapArena + X86_64Paging + QEMU） | 自举堆 + 全局分配器 + `init_vm_self_pt` 时序 | 归 01/10 QEMU 集成 |

### 5.4 测试统计（截至 2026-08-15）

- `cargo test -p minix-vm --lib`：**360 passed / 1 failed**（`region::vir_region::tests::test_map_lazy`，13 范围 pre-existing，plan §3.5 基线一致）。
- 本文档范围 Rust 测试：heap_arena 7（1 常量 + 6 行为）+ global 10（4 状态 + 6 bump/注册）+ vm_self_map 2 = **19 个**（2026-08-15 review 新增 13 个：HeapArena 6 + VmAllocator/bump 6 + vm_self_map 1）。
- 统计规则：不引用具体测试文件行号（避免行号漂移传播，Pattern #66 RCPD 主动应用）。

---

## 6. 过渡

本文档在启动时序中的位置：`init_vm()` 里 `pt_init()`（07/08 结构面）之后、`__minix_init()`/`register_page_alloc()` 分界线上。它把 06 的物理页分配接进 07/08 的页表操作面，产出 VM 的 Rust 堆：

```
05（物理内存布局）→ 06（VmPageAllocator）→ 07/08（页表结构/操作 + vm_self_mappages）→ 09（HeapArena + VmAllocator 堆自举）
                                                                        │
                                                      vm_self_mappages/vm_self_unmap（HeapArena::grow/shrink 消费）
```

**下一篇入口**：10-vm-relocation 承接元数据搬迁——C 侧 slab 元数据（`struct sdh`/`slabs[]`）随进程地址空间搬迁的语义，在 HeapArena 下简化为 VA 区间重定位（`[ARCH: A-3]` 的操作面后果）。

**向后衔接的服务文档**：26-vm-queries（`get_usage_info` 内存占用统计——D5 延后的 slabstats 语义归此处）、13-region-mapping（region 对象经 `Box` 分配，无分配器感知）、18-vm-fork / 20-vm-mmap / 21-vm-munmap（建毁 region/phys_block 的分配模式——验证 bump 足够）。

**硬化项清单**（2026-08-15 review 后更新）：

1. MEMPROTECT 等价物：`HeapArena` debug-only PTE 写保护（经 08 `update_flags` 的 `vm_self_update_flags`）——D4 移交，实施轮承接。
2. ~~HeapArena grow/shrink 行为测试 + VmAllocator bump 测试~~ → **已闭环**（2026-08-15 review，§5.1 新增 13 个，含超大分配 fail-fast guard）。
3. `shrink` 的 `Err(_) => {}` 吞错：当前无生产调用方（10 承接），实施时改为 fail-fast 或保留显式容忍注释（§4.1 讨论）。

---

## 7. 参见

- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/06-page-allocator.md` — 物理页分配器（`VmPageAllocator` 给堆供页，上一篇）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/07-pagetable-struct.md` — 页表结构、Direct Map、VM 自身页表（`init_vm_self_pt`）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/08-pagetable-ops.md` — 页表操作面（`vm_self_mappages`/`vm_self_unmap`/`WMF_WRITEFLAGSONLY`，HeapArena 消费）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/01-vm-init-main.md` — `init_vm` 时序 + `__minix_init` 分界线
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/10-vm-relocation.md` — 元数据搬迁（下一篇，HeapArena VA 区间重定位）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/26-vm-queries.md` — 内存占用统计（slabstats 语义归此处）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/plan.md` — §3.4（09 边界）/§5.3（09 契约）/§7.3（ARCH A-3）
- `notes/rewrite/fork-syscall-rewrite/02-stage-vm/draft/08-slab-allocator.md` — 旧主线素材（素材，§3.1-§3.6 历史背景分析可参考；§3.7 早期"专用 Slab 设计草案"已否决——不采用）
- `minix3/minix/servers/vm/slaballoc.c`、`minix3/minix/servers/vm/proto.h`、`minix3/minix/servers/vm/vm.h`、`minix3/minix/servers/vm/pagetable.c`、`minix3/minix/servers/vm/main.c`、`minix3/minix/servers/vm/init.c` — C 源码（ground truth）
- `os/servers/vm/src/heap_arena.rs`、`os/servers/vm/src/global.rs`、`os/servers/vm/src/pagetable/vm_self_map.rs`、`os/servers/vm/src/direct_map.rs`、`os/servers/vm/src/vm_server.rs` — Rust 实现
- `os/arch/src/arch/direct_map.rs` — `VM_HEAP_BASE`/`VM_HEAP_SIZE` 架构常量（x86-64 0xC0000000 / 64MB）
- Redox `src/allocator/linked_list.rs`（`linked_list_allocator::Heap`）、Linux `mm/slub.c` — 对照参考
