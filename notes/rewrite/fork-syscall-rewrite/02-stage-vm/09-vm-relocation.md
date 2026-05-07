# 09-vm-relocation: 初始化数据搬迁

> **分类**: VM私有
> **源码**: `minix3/minix/servers/vm/alloc.c`、`minix3/minix/servers/vm/pagetable.c`
> **说明**: Bootstrap 阶段结束后，将预留区域中的数据搬迁到堆上

---

## 1. 基本概念

### 1.1 什么是初始化数据搬迁

在操作系统启动过程中，VM（Virtual Memory）服务器需要管理物理内存。但 VM 自身在初始化阶段也面临"先有鸡还是先有蛋"的问题：

- **物理内存管理器需要元数据**（如 bitmap、free list 等）来跟踪哪些物理页是空闲的。
- **这些元数据本身也需要内存来存储**。
- **在 VM 的页表和堆分配器就绪之前**，VM 无法像普通进程一样从自己的堆中分配内存。

因此，Minix3 采用了一个**两阶段初始化**策略：

1. **Bootstrap 阶段**：VM 使用内核预先映射好的静态内存（BSS 段中的数组）来存储物理内存管理器的元数据。此时 VM 还没有自己的页表，所有内存操作都依赖内核在加载 ELF 时已经建立好的映射。
2. **Normal 阶段**：VM 建立了自己的页表和堆分配能力后，可以从堆中动态分配内存。

**搬迁**就是指：在 Bootstrap 阶段结束后，将那些原本存放在静态内存（或预留区域）中的管理元数据，转移到 Normal 阶段的动态堆中。搬迁完成后，静态内存可以被释放或重新利用。

```
Bootstrap 阶段                    Normal 阶段
┌──────────────────┐            ┌──────────────────┐
│ BSS / Reserved   │            │ VM Heap          │
│  ┌─────────────┐ │            │  ┌─────────────┐ │
│  │ bitmap[]    │ │  搬迁 ──►  │  │ bitmap[]    │ │
│  │ page_cache[]│ │            │  │ page_cache[]│ │
│  │ free_lists[]│ │            │  │ free_lists[]│ │
│  │ ...         │ │            │  │ ...         │ │
│  └─────────────┘ │            │  └─────────────┘ │
└──────────────────┘            └──────────────────┘
```

**搬迁不是简单的 memcpy**。搬迁后的数据位于新的虚拟地址，所有指向旧地址的引用必须更新。如果管理结构内部包含指针（如链表头指向节点），搬迁后这些指针会失效，必须逐一修正。

### 1.2 为什么需要搬迁

根本原因：**Bootstrap 阶段的内存是受限的、静态的，而 Normal 阶段需要灵活的、动态的内存管理**。

在 Minix3 的具体实现中：

- **BSS 段中的 bitmap 大小固定**：`free_pages_bitmap` 在编译期就确定为 `PAGE_BITMAP_CHUNKS` 大小，对应 32 位地址空间的全部物理页（约 4GB）。如果实际物理内存只有 256MB，bitmap 仍然占用 128KB，造成浪费；如果物理内存超过 4GB（如 PAE 模式），bitmap 不够用。
- **静态 spare page 池大小固定**：`static_sparepages` 在编译期确定（i386 上为 15 页），如果初始化过程中需要更多页，会耗尽并 panic。
- **静态内存的物理地址在 liveupdate 时会变化**：Minix3 支持 liveupdate（运行时更新），静态 spare page 的物理地址在更新后会改变，导致页表中的映射失效。

因此，Minix3 在 `pt_init()` 完成后，会执行一个**隐式的搬迁过程**：

1. 用光所有静态 spare page（强制后续分配走动态路径）。
2. 重新分配页表等关键结构（使用动态分配的内存）。
3. 复制页表内容到新的动态结构中。
4. 丢弃旧的静态结构。

### 1.3 搬迁涉及的数据

在 Minix3 中，Bootstrap 阶段依赖的静态数据主要包括：

| 数据 | 位置 | 大小 | 用途 |
|------|------|------|------|
| `free_pages_bitmap[]` | BSS | 固定（对应 4GB） | 物理页空闲位图 |
| `free_page_cache[]` | BSS | 固定（10000 项） | 单页分配缓存 |
| `static_sparepages[]` | BSS | 15 页（i386） | 初始化阶段的备用页池 |
| `sparepagedirs[]` | BSS | 1 项（arm） | 初始化阶段的备用页目录池 |
| `pagemap[]`（SANITYCHECKS） | BSS | 对应全部物理页 | 调试用的页使用追踪 |

**关键特征**：
- `free_pages_bitmap` 和 `free_page_cache` 是**扁平数组**（SoA，Structure of Arrays），没有指针、没有嵌套结构、没有链表。
- `static_sparepages` 是**物理页数组**，每个元素是一页物理内存，由内核在加载 ELF 时分配并映射到 VM 的虚拟地址空间。
- 搬迁时，`free_pages_bitmap` 和 `free_page_cache` 不需要搬迁（它们一直在 BSS 中，只是大小固定）。真正需要搬迁的是**页表结构**和**spare page 池**——从静态分配切换到动态分配。

### 1.4 搬迁在初始化时序中的位置

```
T0: Kernel 启动 VM 进程
    ├── 加载 VM 的 ELF，映射 BSS 段（含 static_sparepages）
    └── 传递 boot_info

T1: main() → init_vm()
    │
    ├── T2: get_mem_chunks(mem_chunks)
    │       → 从 kernel_boot_info 解析内存布局
    │
    ├── T3: mem_init(mem_chunks)
    │       → 初始化 free_pages_bitmap（BSS 中，无需分配）
    │       → 标记所有可用物理页为空闲
    │
    ├── T4: pt_init()
    │       → 创建 VM 自己的页表
    │       → 分配 spare page 池（从 static_sparepages）
    │       → pt_init_done = 1
    │       → 【隐式搬迁开始】
    │       → 用光所有静态 spare page
    │       → 重新分配页表（动态内存）
    │       → 复制页表内容
    │       → 丢弃旧页表
    │       → 【隐式搬迁结束】
    │
    └── init_vm() 返回

T5: 主循环开始，所有分配走动态路径
```

### 1.5 核心边界条件

**分配失败**：
- Bootstrap 阶段：如果静态 spare page 池耗尽（如 `static_sparepages` 的 15 页用完），`vm_allocpage()` 返回 NULL，可能导致 panic。
- Normal 阶段：如果堆分配失败（`alloc_mem()` 返回 `NO_MEM`），系统无法继续运行，通常 panic。

**内存对齐**：
- `alloc_mem()` 支持 `PAF_ALIGN64K` 和 `PAF_ALIGN16K` 标志。对齐分配时，先分配比请求更大的块，然后释放头部未对齐的部分。这要求空闲位图能够正确标记部分释放的页。

**并发安全**：
- Minix3 的 VM 是单线程的（一个进程，一个消息循环），因此分配器不需要考虑并发竞争。所有操作都是同步的、不可中断的。

**物理内存限制**：
- `PAF_LOWER16MB` 和 `PAF_LOWER1MB` 限制分配只能在低地址区域。这在 DMA 设备需要物理连续内存时使用。

---

## 2. Minix3 C 源码分析

### 2.1 物理内存管理器的核心数据结构

Minix3 的物理内存管理器位于 `minix3/minix/servers/vm/alloc.c`，其核心数据结构如下：

```c
// minix3/minix/servers/vm/alloc.c:32-38

/* Number of physical pages in a 32-bit address space */
#define NUMBER_PHYSICAL_PAGES (int)(0x100000000ULL/VM_PAGE_SIZE)
#define PAGE_BITMAP_CHUNKS BITMAP_CHUNKS(NUMBER_PHYSICAL_PAGES)
static bitchunk_t free_pages_bitmap[PAGE_BITMAP_CHUNKS];
#define PAGE_CACHE_MAX 10000
static int free_page_cache[PAGE_CACHE_MAX];
static int free_page_cache_size = 0;
```

- **`NUMBER_PHYSICAL_PAGES`**：32 位地址空间的总页数。`0x100000000ULL` 是 4GB，`VM_PAGE_SIZE` 在 i386 上是 4096，所以总页数为 `0x100000 / 4096 = 1048576` 页（即 4GB / 4KB = 1M 页）。
- **`PAGE_BITMAP_CHUNKS`**：`BITMAP_CHUNKS(NUMBER_PHYSICAL_PAGES)` 计算位图需要的 `bitchunk_t` 数量。`bitchunk_t` 是 `uint32_t`，每个 chunk 管理 32 页。`1M / 32 = 32768` 个 chunk，每个 chunk 4 字节，总大小为 `32768 * 4 = 131072` 字节（128KB）。
- **`free_pages_bitmap[]`**：静态数组，在 BSS 段中。每一位对应一个物理页，`1` 表示空闲，`0` 表示已分配。
- **`free_page_cache[]`**：静态数组，大小为 10000 项。用于缓存最近释放的单页，加速单页分配。
- **`free_page_cache_size`**：缓存当前使用的项数，初始为 0。

**位图操作宏**（来自 `minix/include/minix/bitmap.h`）：

```c
#define BITCHUNK_BITS   (sizeof(bitchunk_t) * CHAR_BIT)   // 32
#define BITMAP_CHUNKS(nr_bits) (((nr_bits)+BITCHUNK_BITS-1)/BITCHUNK_BITS)
#define MAP_CHUNK(map,bit) (map)[((bit)/BITCHUNK_BITS)]
#define CHUNK_OFFSET(bit) ((bit)%BITCHUNK_BITS)
#define GET_BIT(map,bit) ( MAP_CHUNK(map,bit) & (1 << CHUNK_OFFSET(bit)) )
#define SET_BIT(map,bit) ( MAP_CHUNK(map,bit) |= (1 << CHUNK_OFFSET(bit)) )
#define UNSET_BIT(map,bit) ( MAP_CHUNK(map,bit) &= ~(1 << CHUNK_OFFSET(bit)) )
```

- **`GET_BIT`**：检查某一位是否为 1。
- **`SET_BIT`**：将某一位设为 1（标记为空闲）。
- **`UNSET_BIT`**：将某一位清零（标记为已分配）。

### 2.2 初始化流程：`mem_init()`

```c
// minix3/minix/servers/vm/alloc.c:306-335

void mem_init(struct memory *chunks)
{
  int i, first = 0;

  total_pages = 0;

  memset(free_pages_bitmap, 0, sizeof(free_pages_bitmap));

  /* Use the chunks of physical memory to allocate holes. */
  for (i=NR_MEMS-1; i>=0; i--) {
  	if (chunks[i].size > 0) {
		phys_bytes from = CLICK2ABS(chunks[i].base),
			to = CLICK2ABS(chunks[i].base+chunks[i].size)-1;
		if(first || from < mem_low) mem_low = from;
		if(first || to > mem_high) mem_high = to;
		free_mem(chunks[i].base, chunks[i].size);
		total_pages += chunks[i].size;
		first = 0;
	}
  }
}
```

**逐行解析**：

- **`total_pages = 0`**：重置总页数计数器。`total_pages` 是全局变量，定义在 `glo.h` 中，用于统计系统中物理内存的总页数。
- **`memset(free_pages_bitmap, 0, sizeof(free_pages_bitmap))`**：将整个位图清零。注意：这里清零意味着所有位都是 0，但后续 `free_mem()` 会将可用内存对应的位设为 1（空闲）。这是一个容易混淆的地方——**0 表示已分配，1 表示空闲**。
- **`for (i=NR_MEMS-1; i>=0; i--)`**：从后向前遍历内存块数组。`NR_MEMS` 是 16，表示最多 16 个内存块。
- **`chunks[i].size > 0`**：只处理有效的内存块。`struct memory` 的定义是 `{ phys_bytes base; phys_bytes size; }`，`base` 和 `size` 都以 click（4096 字节）为单位。
- **`CLICK2ABS(chunks[i].base)`**：将 click 转换为字节地址。`CLICK2ABS` 是 `((v) << CLICK_SHIFT)`，即乘以 4096。
- **`mem_low` 和 `mem_high`**：记录最低和最高的物理内存地址，用于调试和 sanity check。
- **`free_mem(chunks[i].base, chunks[i].size)`**：将这块内存标记为空闲。注意这里调用的是 `free_mem()`，它会进一步调用 `free_pages()`，将对应的位图位设为 1。
- **`total_pages += chunks[i].size`**：累加总页数。

**为什么从后向前遍历？**

Minix3 的 `alloc_pages()` 使用**从高地址向低地址扫描**的策略（见 2.4 节）。初始化时从后向前调用 `free_mem()`，可以确保高地址的内存块先被标记为空闲，从而优先被分配。这是一种经验性的优化，让低地址内存保留更久（低地址内存对某些设备更友好）。

### 2.3 分配流程：`alloc_mem()`

```c
// minix3/minix/servers/vm/alloc.c:242-279

phys_clicks alloc_mem(phys_clicks clicks, u32_t memflags)
{
  phys_clicks mem = NO_MEM, align_clicks = 0;

  if(memflags & PAF_ALIGN64K) {
  	align_clicks = (64 * 1024) / CLICK_SIZE;
	clicks += align_clicks;
  } else if(memflags & PAF_ALIGN16K) {
	align_clicks = (16 * 1024) / CLICK_SIZE;
	clicks += align_clicks;
  }

  do {
	mem = alloc_pages(clicks, memflags);
  } while(mem == NO_MEM && cache_freepages(clicks) > 0);

  if(mem == NO_MEM)
  	return mem;

  if(align_clicks) {
  	phys_clicks o;
  	o = mem % align_clicks;
  	if(o > 0) {
  		phys_clicks e;
  		e = align_clicks - o;
	  	free_mem(mem, e);
	  	mem += e;
	}
  }

  return mem;
}
```

**逐行解析**：

- **`phys_clicks mem = NO_MEM, align_clicks = 0`**：`NO_MEM` 定义为 `((phys_clicks) MAP_NONE)`，即 `0xFFFFFFFE`，表示分配失败。`align_clicks` 用于对齐分配。
- **`if(memflags & PAF_ALIGN64K)`**：如果需要 64KB 对齐，计算对齐需要的额外 click 数。`64 * 1024 / 4096 = 16` clicks。将 `clicks` 增加 16，确保即使分配的起始地址未对齐，也有足够的空间可以调整到对齐边界。
- **`do { mem = alloc_pages(clicks, memflags); } while(...)`**：尝试分配。如果失败（`mem == NO_MEM`），调用 `cache_freepages(clicks)` 尝试从页缓存中释放足够数量的页，然后重试。`cache_freepages()` 在 `cache.c` 中实现，它会遍历 LRU 缓存，释放未被引用的页。
- **`if(align_clicks)`**：如果请求了对齐，检查分配结果是否确实对齐。
  - **`o = mem % align_clicks`**：计算起始地址相对于对齐边界的偏移。
  - **`if(o > 0)`**：如果未对齐，释放前 `e = align_clicks - o` 个 click，将起始地址调整到对齐边界。
  - **例如**：请求 1 页，64K 对齐。`alloc_pages` 分配了 17 页（1 + 16）。如果返回的地址是 click 5，偏移 `5 % 16 = 5`，释放前 11 页，起始地址变为 click 16（64KB 对齐）。

**对齐分配的代价**：

对齐分配会浪费内存。 worst case 下，需要额外分配 `align_clicks - 1` 个 click，然后释放它们。这些被释放的 click 会回到空闲池，但可能造成碎片。

### 2.4 底层分配：`alloc_pages()`

```c
// minix3/minix/servers/vm/alloc.c:404-460

static phys_bytes alloc_pages(int pages, int memflags)
{
	phys_bytes boundary16 = 16 * 1024 * 1024 / VM_PAGE_SIZE;
	phys_bytes boundary1  =  1 * 1024 * 1024 / VM_PAGE_SIZE;
	phys_bytes mem = NO_MEM, i;
	int maxpage = NUMBER_PHYSICAL_PAGES - 1;
	static int lastscan = -1;
	int startscan, run_length;

	if(memflags & PAF_LOWER16MB)
		maxpage = boundary16 - 1;
	else if(memflags & PAF_LOWER1MB)
		maxpage = boundary1 - 1;
	else {
		/* no position restrictions: check page cache */
		if(pages == 1) {
			while(free_page_cache_size > 0) {
				i = free_page_cache[free_page_cache_size-1];
				if(page_isfree(i)) {
					free_page_cache_size--;
					mem = i;
					assert(mem != NO_MEM);
					run_length = 1;
					break;
				}
				free_page_cache_size--;
			}
		}
	}

	if(lastscan < maxpage && lastscan >= 0)
		startscan = lastscan;
	else	startscan = maxpage;

	if(mem == NO_MEM)
		mem = findbit(0, startscan, pages, memflags, &run_length);
	if(mem == NO_MEM)
		mem = findbit(0, maxpage, pages, memflags, &run_length);
	if(mem == NO_MEM)
		return NO_MEM;

	/* remember for next time */
	lastscan = mem;

	for(i = mem; i < mem + pages; i++) {
		UNSET_BIT(free_pages_bitmap, i);
	}

	if(memflags & PAF_CLEAR) {
		int s;
		if ((s= sys_memset(NONE, 0, CLICK_SIZE*mem,
			VM_PAGE_SIZE*pages)) != OK)
			panic("alloc_mem: sys_memset failed: %d", s);
	}

	return mem;
}
```

**逐行解析**：

- **`boundary16 = 16 * 1024 * 1024 / VM_PAGE_SIZE`**：16MB 边界对应的页号。`16MB / 4KB = 4096` 页。
- **`boundary1 = 1 * 1024 * 1024 / VM_PAGE_SIZE`**：1MB 边界对应的页号。`1MB / 4KB = 256` 页。
- **`maxpage = NUMBER_PHYSICAL_PAGES - 1`**：默认扫描到最高页号（对应 4GB 地址空间的最后一页）。
- **`if(memflags & PAF_LOWER16MB)`**：如果限制在低 16MB，将 `maxpage` 设为 4095。这用于 DMA 设备，因为某些旧设备只能访问低 16MB 物理内存。
- **`else if(memflags & PAF_LOWER1MB)`**：限制在低 1MB，用于更老的设备（如 BIOS）。
- **`else { ... }`**：没有位置限制时，先检查单页缓存。
  - **`if(pages == 1)`**：只有单页分配才使用缓存。多页分配不走缓存，因为缓存只存单页。
  - **`while(free_page_cache_size > 0)`**：从缓存末尾开始检查（LIFO）。
  - **`i = free_page_cache[free_page_cache_size-1]`**：取出最后一项。
  - **`if(page_isfree(i))`**：检查该页是否仍然空闲。注意：缓存中的页可能被之前的分配占用（如果缓存没有正确维护），所以需要双重检查。
  - **`free_page_cache_size--`**：无论是否命中，都将该项从缓存中移除。如果命中，返回该页；如果未命中，继续检查前一项。
- **`static int lastscan = -1`**：静态变量，记录上次成功分配的页号。下次分配从这里开始扫描，利用局部性原理。
- **`startscan = lastscan`**：如果 `lastscan` 在有效范围内，从这里开始扫描。
- **`mem = findbit(0, startscan, pages, memflags, &run_length)`**：从高地址向低地址扫描，寻找连续的 `pages` 个空闲页。
- **`mem = findbit(0, maxpage, pages, memflags, &run_length)`**：如果第一次扫描失败，从最高页号重新开始扫描（覆盖整个范围）。
- **`lastscan = mem`**：记录本次成功分配的页号，供下次使用。
- **`UNSET_BIT(free_pages_bitmap, i)`**：将分配出去的页标记为已分配（位清零）。
- **`if(memflags & PAF_CLEAR)`**：如果请求清零，通过 `sys_memset()` 系统调用将物理内存清零。注意：`sys_memset` 的参数是 `CLICK_SIZE*mem`（字节地址）和 `VM_PAGE_SIZE*pages`（字节长度）。

**页缓存的设计意图**：

`free_page_cache` 是一个简单的 LIFO 缓存，用于加速单页分配。当页被释放时（`free_pages()`），如果缓存未满，页号被加入缓存。分配单页时，优先从缓存取，避免扫描位图。但缓存不保证一致性——缓存中的页可能被其他路径分配（虽然 Minix3 单线程，但 sanity check 等路径可能绕过缓存），所以分配时需要再次检查 `page_isfree()`。

### 2.5 位图扫描：`findbit()`

```c
// minix3/minix/servers/vm/alloc.c:369-399

static int findbit(int low, int startscan, int pages, int memflags, int *len)
{
	int run_length = 0, i;
	int freerange_start = startscan;

	for(i = startscan; i >= low; i--) {
		if(!page_isfree(i)) {
			int pi;
			int chunk = i/BITCHUNK_BITS, moved = 0;
			run_length = 0;
			pi = i;
			while(chunk > 0 &&
			   !MAP_CHUNK(free_pages_bitmap, chunk*BITCHUNK_BITS)) {
				chunk--;
				moved = 1;
			}
			if(moved) { i = chunk * BITCHUNK_BITS + BITCHUNK_BITS; }
			continue;
		}
		if(!run_length) { freerange_start = i; run_length = 1; }
		else { freerange_start--; run_length++; }
		assert(run_length <= pages);
		if(run_length == pages) {
			/* good block found! */
			*len = run_length;
			return freerange_start;
		}
	}

	return NO_MEM;
}
```

**逐行解析**：

- **`for(i = startscan; i >= low; i--)`**：从高地址向低地址扫描。`low` 通常是 0，`startscan` 是上次分配的页号或最高页号。
- **`if(!page_isfree(i))`**：如果当前页已分配，重置连续空闲计数。
- **`chunk = i / BITCHUNK_BITS`**：计算当前页属于哪个 `bitchunk_t`。
- **`while(chunk > 0 && !MAP_CHUNK(...))`**：如果当前 chunk 的所有位都是 0（即该 chunk 对应的所有页都已分配），跳过整个 chunk。
  - **`MAP_CHUNK(free_pages_bitmap, chunk*BITCHUNK_BITS)`**：取该 chunk 的值。如果为 0，表示这 32 页全部已分配。
  - **`chunk--`**：向前跳一个 chunk。
  - **`moved = 1`**：标记发生了跳转。
  - **`i = chunk * BITCHUNK_BITS + BITCHUNK_BITS`**：将 `i` 设置为跳转后 chunk 的末尾（最高位对应的页号），继续扫描。
- **跳转优化**：这个 `while` 循环是 `findbit` 的关键优化。位图扫描的最坏情况是逐位检查，时间复杂度 O(n)。通过跳过全 0 的 chunk，可以将扫描速度提升约 32 倍（一个 chunk 32 位）。
- **`if(!run_length)`**：如果当前是连续空闲区的第一页，记录起始页号。
- **`else { freerange_start--; run_length++; }`**：继续扩展连续空闲区。注意：`freerange_start` 递减，因为扫描方向是从高到低。
- **`assert(run_length <= pages)`**：断言连续长度不超过请求长度。如果超过，说明逻辑有误。
- **`if(run_length == pages)`**：找到足够大的连续块，返回起始页号。

**为什么从高地址向低地址扫描？**

Minix3 采用这种策略是为了让低地址内存保留更久。低地址内存（尤其是低 1MB、低 16MB）对某些硬件设备（如 DMA 控制器）有特殊要求。优先使用高地址内存，可以延长低地址内存的可用时间。

### 2.6 释放流程：`free_mem()` 和 `free_pages()`

```c
// minix3/minix/servers/vm/alloc.c:289-301

void free_mem(phys_clicks base, phys_clicks clicks)
{
  if (clicks == 0) return;

  assert(CLICK_SIZE == VM_PAGE_SIZE);
  free_pages(base, clicks);
  return;
}
```

- **`clicks == 0`**：空释放，直接返回。
- **`assert(CLICK_SIZE == VM_PAGE_SIZE)`**：断言 click 大小等于页大小。Minix3 要求这两者相等（`#if CLICK_SIZE != VM_PAGE_SIZE #error`）。
- **`free_pages(base, clicks)`**：调用底层释放函数。

```c
// minix3/minix/servers/vm/alloc.c:465-481

static void free_pages(phys_bytes pageno, int npages)
{
	int i, lim = pageno + npages - 1;

#if JUNKFREE
       if(sys_memset(NONE, 0xa5a5a5a5, VM_PAGE_SIZE * pageno,
               VM_PAGE_SIZE * npages) != OK)
                       panic("free_pages: sys_memset failed");
#endif

	for(i = pageno; i <= lim; i++) {
		SET_BIT(free_pages_bitmap, i);
		if(free_page_cache_size < PAGE_CACHE_MAX) {
			free_page_cache[free_page_cache_size++] = i;
		}
	}
}
```

**逐行解析**：

- **`lim = pageno + npages - 1`**：计算最后一页的页号。
- **`#if JUNKFREE`**：如果定义了 `JUNKFREE`，将释放的内存填充为 `0xa5a5a5a5`。这是一种调试技术，帮助发现"使用已释放内存"的 bug。
- **`SET_BIT(free_pages_bitmap, i)`**：将位设为 1，标记为空闲。
- **`if(free_page_cache_size < PAGE_CACHE_MAX)`**：如果缓存未满，将页号加入缓存。
- **`free_page_cache[free_page_cache_size++] = i`**：LIFO 追加。注意：这里没有检查该页是否已经在缓存中。如果同一页被释放两次，缓存中会有两个相同的项。这不会导致错误（因为分配时会检查 `page_isfree()`），但会降低缓存效率。

### 2.7 备用页池：`spare_pagequeue` 与 `reservedqueue`

Minix3 的页表操作（如 `pt_ptalloc()`）需要分配物理页来存储页表。但 `vm_allocpage()` 在初始化阶段（`pt_init_done == 0`）不能调用 `alloc_mem()`，因为物理内存管理器可能还没完全就绪。为了解决这个递归依赖，Minix3 引入了**备用页池（spare page pool）**。

```c
// minix3/minix/servers/vm/pagetable.c:59-109

#if SANITYCHECKS
#define SPAREPAGES 200
#define STATIC_SPAREPAGES 190
#else
#ifdef __arm__
# define SPAREPAGES 150
# define STATIC_SPAREPAGES 140
#else
# define SPAREPAGES 20
# define STATIC_SPAREPAGES 15
#endif
#endif

static void *spare_pagequeue;
static char static_sparepages[VM_PAGE_SIZE*STATIC_SPAREPAGES]
	__aligned(VM_PAGE_SIZE);
```

- **`SPAREPAGES`**：备用页池的总容量。i386 上为 20，arm 上为 150，开启 SANITYCHECKS 时为 200。
- **`STATIC_SPAREPAGES`**：静态备用页的数量。i386 上为 15，arm 上为 140。这些页在 BSS 段中，由内核预先映射。
- **`spare_pagequeue`**：指向 `reservedqueue` 结构的指针，管理备用页池。
- **`static_sparepages[]`**：静态数组，存储备用页的虚拟地址。`__aligned(VM_PAGE_SIZE)` 确保数组页对齐。

**`reservedqueue` 结构**：

```c
// minix3/minix/servers/vm/alloc.c:60-72

static struct reserved_pages {
	struct reserved_pages *next;	/* next in use */
	int max_available;	/* queue depth use, 0 if not in use at all */
	int npages;		/* number of consecutive pages */
	int mappedin;		/* must reserved pages also be mapped? */
	int n_available;	/* number of queue entries */
	int allocflags;		/* allocflags for alloc_mem */
	struct reserved_pageslot {
		phys_bytes	phys;
		void		*vir;
	} slots[MAXRESERVEDPAGES];
	u32_t magic;
} reservedqueues[MAXRESERVEDQUEUES], *first_reserved_inuse = NULL;
```

- **`reservedqueues[]`**：静态数组，最多 15 个队列。每个队列管理一类备用页。
- **`max_available`**：队列的最大容量。
- **`npages`**：每个 slot 占用的连续页数（spare page 为 1）。
- **`mappedin`**：是否需要虚拟地址映射（spare page 需要，因为 VM 需要访问它们）。
- **`n_available`**：当前可用的 slot 数。
- **`slots[]`**：每个 slot 记录物理地址和虚拟地址。
- **`magic`**：魔数 `0x6e4c74d5`，用于 sanity check。

**`pt_init()` 中的备用页池初始化**：

```c
// minix3/minix/servers/vm/pagetable.c:1116-1162

/* Get ourselves spare pages. */
sparepages_mem = (vir_bytes) static_sparepages;
assert(!(sparepages_mem % VM_PAGE_SIZE));

if(!(spare_pagequeue = reservedqueue_new(SPAREPAGES, 1, 1, 0)))
	panic("reservedqueue_new for single pages failed");

assert(STATIC_SPAREPAGES < SPAREPAGES);
for(s = 0; s < STATIC_SPAREPAGES; s++) {
	void *v = (void *) (sparepages_mem + s*VM_PAGE_SIZE);
	phys_bytes ph;
	if((r=sys_umap(SELF, VM_D, (vir_bytes) v,
                VM_PAGE_SIZE*SPAREPAGES, &ph)) != OK)
			panic("pt_init: sys_umap failed: %d", r);
	reservedqueue_add(spare_pagequeue, v, ph);
}
```

**逐行解析**：

- **`sparepages_mem = (vir_bytes) static_sparepages`**：获取静态备用页数组的虚拟地址。
- **`reservedqueue_new(SPAREPAGES, 1, 1, 0)`**：创建一个备用页队列，容量 20，每 slot 1 页，需要映射，无特殊分配标志。
- **`for(s = 0; s < STATIC_SPAREPAGES; s++)`**：遍历静态备用页。
- **`sys_umap(SELF, VM_D, (vir_bytes) v, VM_PAGE_SIZE*SPAREPAGES, &ph)`**：通过内核系统调用，查询虚拟地址 `v` 对应的物理地址 `ph`。`VM_D` 表示数据段。
- **`reservedqueue_add(spare_pagequeue, v, ph)`**：将虚拟地址和物理地址加入队列。

**注意**：这里只初始化了 `STATIC_SPAREPAGES`（15 页），而队列容量是 `SPAREPAGES`（20 页）。剩余的 5 页在 `alloc_cycle()` 中通过 `alloc_mem()` 动态分配并加入队列。

### 2.8 阶段切换：`pt_init_done`

```c
// minix3/minix/servers/vm/pagetable.c:328

static int pt_init_done;
```

`pt_init_done` 是一个静态整型标志，初始值为 0（C 语言静态变量默认初始化）。

```c
// minix3/minix/servers/vm/pagetable.c:333-364

void *vm_allocpages(phys_bytes *phys, int reason, int pages)
{
	phys_bytes newpage;
	static int level = 0;
	void *ret;
	u32_t mem_flags = 0;

	assert(reason >= 0 && reason < VMP_CATEGORIES);
	assert(pages > 0);

	level++;

	assert(level >= 1);
	assert(level <= 2);

	if((level > 1) || !pt_init_done) {
		void *s;

		if(pages == 1) s=vm_getsparepage(phys);
		else if(pages == 4) s=vm_getsparepagedir(phys);
		else panic("%d pages", pages);

		level--;
		if(!s) {
			util_stacktrace();
			printf("VM: warning: out of spare pages\n");
		}
		if(!is_staticaddr(s)) vm_self_pages++;
		return s;
	}
	...
}
```

**逐行解析**：

- **`static int level = 0`**：递归深度计数器。`vm_allocpage()` 可能递归调用自身（例如 `pt_ptalloc()` 调用 `vm_allocpage()` 分配页表，而 `vm_allocpage()` 又需要映射页，映射页又需要页表...）。
- **`level++` 和 `assert(level <= 2)`**：限制递归深度最多为 2。如果超过，panic。
- **`if((level > 1) || !pt_init_done)`**：如果递归深度大于 1，或者 `pt_init_done == 0`，使用备用页池。
  - **`level > 1`**：表示当前处于递归路径。递归路径上的分配不能走正常路径（因为正常路径可能再次触发递归）。
  - **`!pt_init_done`**：表示初始化尚未完成。此时 VM 的页表和堆分配器还没就绪，不能调用 `alloc_mem()`。
- **`vm_getsparepage(phys)`**：从备用页队列取一个页。返回虚拟地址，并通过指针参数返回物理地址。
- **`is_staticaddr(s)`**：检查地址是否是静态地址（低于 `VM_OWN_HEAPSTART`）。如果是静态地址，不需要追踪（因为静态页不会被释放）；如果是动态地址，增加 `vm_self_pages` 计数。

**`pt_init_done = 1` 的位置**：

```c
// minix3/minix/servers/vm/pagetable.c:1311

pt_init_done = 1;
```

这行代码位于 `pt_init()` 的末尾，但在"隐式搬迁"之前。具体来说，`pt_init()` 的执行顺序是：

1. 初始化 spare page 池（静态页）。
2. 创建 VM 自己的页表（使用静态 spare page）。
3. `pt_init_done = 1`。
4. **隐式搬迁**：用光静态页，重新分配动态页，复制页表。

这意味着 `pt_init_done = 1` 之后，VM 仍然使用静态页一段时间，直到搬迁完成。

### 2.9 隐式搬迁：`pt_init()` 的后半段

```c
// minix3/minix/servers/vm/pagetable.c:1311-1352

pt_init_done = 1;

/* VM is now fully functional in that it can dynamically allocate memory
 * for itself.
 *
 * We don't want to keep using the bootstrap statically allocated spare
 * pages though, as the physical addresses will change on liveupdate. So we
 * re-do part of the initialization now with purely dynamically allocated
 * memory. First throw out the static pool.
 *
 * Then allocate the kernel-shared-pagetables and VM pagetables with dynamic
 * memory.
 */

alloc_cycle();                          /* Make sure allocating works */
while(vm_getsparepage(&phys)) ;		/* Use up all static pages */
alloc_cycle();                          /* Refill spares with dynamic */
pt_allocate_kernel_mapped_pagetables(); /* Reallocate in-kernel pages */
pt_bind(newpt, &vmproc[VM_PROC_NR]);    /* Recalculate */
pt_mapkernel(newpt);                    /* Rewrite pagetable info */

/* Flush TLB just in case any of those mappings have been touched */
if((sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)) != OK) {
	panic("VMCTL_FLUSHTLB failed");
}

/* Recreate VM page table with dynamic-only allocations */
memset(&newpt_dyn, 0, sizeof(newpt_dyn));
pt_new(&newpt_dyn);
pt_copy(&newpt_dyn, newpt);
memcpy(newpt, &newpt_dyn, sizeof(*newpt));

pt_bind(newpt, &vmproc[VM_PROC_NR]);    /* Recalculate */
pt_mapkernel(newpt);                    /* Rewrite pagetable info */

/* Flush TLB just in case any of those mappings have been touched */
if((sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)) != OK) {
	panic("VMCTL_FLUSHTLB failed");
}
```

**逐行解析**：

- **`alloc_cycle()`**：触发备用页队列的填充。`alloc_cycle()` 遍历所有 `reservedqueue`，如果某个队列的可用 slot 数小于最大容量，调用 `reservedqueue_fill()` 动态分配新页并加入队列。这里的作用是确保备用页池中有动态分配的页。
- **`while(vm_getsparepage(&phys))`**：循环取出所有备用页，直到队列为空。这会耗尽静态 spare page（因为静态页在队列前面，动态页在后面）。但注意：`vm_getsparepage()` 只是取出页，并不释放它们。这些页被"消耗"了——它们被用于后续的重新分配。
- **`alloc_cycle()`**：再次触发填充。此时队列已空，`alloc_cycle()` 会通过 `alloc_mem()` 动态分配新页加入队列。这些新页是动态分配的，物理地址不会随 liveupdate 改变。
- **`pt_allocate_kernel_mapped_pagetables()`**：重新分配内核映射的页表。这些页表之前可能是用静态页分配的，现在用动态页重新分配。
- **`pt_bind(newpt, &vmproc[VM_PROC_NR])`**：将新的页表绑定到 VM 进程。`pt_bind()` 更新内核中的页目录映射，让内核知道 VM 的新页表位置。
- **`pt_mapkernel(newpt)`**：重新映射内核地址空间到新页表。
- **`sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)`**：刷新 TLB，确保新的页表映射生效。
- **`memset(&newpt_dyn, 0, sizeof(newpt_dyn))`**：清零一个新的页表结构。
- **`pt_new(&newpt_dyn)`**：用动态分配的内存创建一个新的页表根（页目录）。`pt_new()` 会调用 `vm_allocpage()` 分配页目录和页表，此时 `pt_init_done == 1`，所以走正常分配路径（`alloc_mem()` + `vm_mappages()`）。
- **`pt_copy(&newpt_dyn, newpt)`**：将旧页表的内容复制到新页表。`pt_copy()` 遍历所有 PDE，如果旧页表中有页表，在新页表中分配新的页表并复制内容。
- **`memcpy(newpt, &newpt_dyn, sizeof(*newpt))`**：用新页表结构覆盖旧页表结构。此时 `newpt` 指向的页表完全由动态分配的内存组成。
- **`pt_bind()` 和 `pt_mapkernel()` 再次执行**：更新内核的页表绑定和内核映射。
- **再次刷新 TLB**。

**搬迁的本质**：

Minix3 的"搬迁"不是显式的 `memcpy` + 更新指针，而是通过**重新分配 + 复制内容 + 替换结构**来实现的。具体来说：

1. **页目录**：`pt_new()` 分配新的页目录（动态页），`memcpy` 替换旧的页目录指针。
2. **页表**：`pt_copy()` 为每个 PDE 分配新的页表（动态页），复制旧页表的内容。
3. **spare page 池**：用光静态页，重新填充动态页。

这种设计的优点是**不需要更新所有指向旧结构的引用**——因为 `newpt` 是一个局部结构，`memcpy` 直接替换了它的内容，所有通过 `newpt` 访问的代码自动使用新结构。

### 2.10 虚拟地址分配：`findhole()`

```c
// minix3/minix/servers/vm/pagetable.c:155-230

static u32_t findhole(int pages)
{
	u32_t curv;
	int pde = 0, try_restart;
	static void *lastv = 0;
	pt_t *pt = &vmprocess->vm_pt;
	vir_bytes vmin, vmax;
	u32_t holev = NO_MEM;
	int holesize = -1;

	vmin = VM_OWN_MMAPBASE;
	vmax = VM_OWN_MMAPTOP;

	/* Input sanity check. */
	assert(vmin + VM_PAGE_SIZE >= vmin);
	assert(vmax >= vmin + VM_PAGE_SIZE);
	assert((vmin % VM_PAGE_SIZE) == 0);
	assert((vmax % VM_PAGE_SIZE) == 0);
	assert(pages > 0);

	curv = (u32_t) lastv;
	if(curv < vmin || curv >= vmax)
		curv = vmin;

	try_restart = 1;

	/* Start looking for a free page starting at vmin. */
	while(curv < vmax) {
		int pte;

		assert(curv >= vmin);
		assert(curv < vmax);

		pde = ARCH_VM_PDE(curv);
		pte = ARCH_VM_PTE(curv);

		if((pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT) &&
		   (pt->pt_pt[pde][pte] & ARCH_VM_PTE_PRESENT)) {
			/* there is a page here - so keep looking for holes */
			holev = NO_MEM;
			holesize = 0;
		} else {
			/* there is no page here - so we have a hole, a bigger
			 * one if we already had one
			 */
			if(holev == NO_MEM) {
				holev = curv;
				holesize = 1;
			} else holesize++;

			assert(holesize > 0);
			assert(holesize <= pages);

			/* if it's big enough, return it */
			if(holesize == pages) {
				lastv = (void*) (curv + VM_PAGE_SIZE);
				return holev;
			}
		}

		curv+=VM_PAGE_SIZE;

		/* if we reached the limit, start scanning from the beginning if
		 * we haven't looked there yet
		 */
		if(curv >= vmax && try_restart) {
			try_restart = 0;
			curv = vmin;
		}
	}

	printf("VM: out of virtual address space in vm\n");

	return NO_MEM;
}
```

**逐行解析**：

- **`vmin = VM_OWN_MMAPBASE`**：VM 自己的 mmap 基地址。定义为 `VM_OWN_HEAPBASE + 1024*1024*1024`（即堆基地址 + 1GB）。
- **`vmax = VM_OWN_MMAPTOP`**：VM 自己的 mmap 顶地址。定义为 `VM_OWN_MMAPBASE + 100 * 1024 * 1024`（即 100MB 的 mmap 区域）。
- **`assert(vmin + VM_PAGE_SIZE >= vmin)`**：检查溢出。如果 `vmin` 接近 0xFFFFFFFF，加 `VM_PAGE_SIZE` 会溢出。
- **`curv = (u32_t) lastv`**：从上次分配的地址之后开始扫描。`lastv` 是静态变量，初始为 0。
- **`try_restart = 1`**：允许从头开始扫描一次。
- **`pde = ARCH_VM_PDE(curv)`**：计算虚拟地址对应的页目录项索引。在 i386 上，`ARCH_VM_PDE` 是 `(addr >> 22)`，即取高 10 位。
- **`pte = ARCH_VM_PTE(curv)`**：计算页表项索引。在 i386 上，`ARCH_VM_PTE` 是 `((addr >> 12) & 0x3FF)`，即取中间 10 位。
- **`if((pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT) && (pt->pt_pt[pde][pte] & ARCH_VM_PTE_PRESENT))`**：检查该虚拟地址是否已有映射。
  - **`pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT`**：页目录项是否存在。
  - **`pt->pt_pt[pde][pte] & ARCH_VM_PTE_PRESENT`**：页表项是否存在。
  - 如果两者都存在，表示该地址已被占用，重置 `holev` 和 `holesize`。
- **`else { ... }`**：该地址未被占用，扩展空洞。
  - **`if(holev == NO_MEM)`**：新空洞的开始。
  - **`holesize == pages`**：空洞足够大，返回起始地址。
- **`curv += VM_PAGE_SIZE`**：检查下一个页。
- **`if(curv >= vmax && try_restart)`**：如果扫描到顶且未找到，从头开始再扫描一次。
- **`return NO_MEM`**：没有找到足够大的空洞，虚拟地址空间耗尽。

**为什么 mmap 区域只有 100MB？**

Minix3 的 VM 是一个用户态进程，它自己的地址空间也需要管理。`VM_OWN_MMAPBASE` 到 `VM_OWN_MMAPTOP` 是 VM 用于映射物理页的区域（如页表、缓存页等）。100MB 对于 VM 的内部使用来说足够，但如果 VM 需要管理大量缓存页，可能会耗尽。

### 2.11 页表映射：`vm_mappages()`

```c
// minix3/minix/servers/vm/pagetable.c:295-320

void *vm_mappages(phys_bytes p, int pages)
{
	vir_bytes loc;
	int r;
	pt_t *pt = &vmprocess->vm_pt;

	/* Where in our virtual address space can we put it? */
	loc = findhole(pages);
	if(loc == NO_MEM) {
		printf("vm_mappages: findhole failed\n");
		return NULL;
	}

	/* Map this page into our address space. */
	if((r=pt_writemap(vmprocess, pt, loc, p, VM_PAGE_SIZE*pages,
		ARCH_VM_PTE_PRESENT | ARCH_VM_PTE_USER | ARCH_VM_PTE_RW
#if defined(__arm__)
		| ARM_VM_PTE_CACHED
#endif
		, 0)) != OK) {
		printf("vm_mappages writemap failed\n");
		return NULL;
	}

	if((r=sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)) != OK) {
		panic("VMCTL_FLUSHTLB failed: %d", r);
	}

	return (void *) loc;
}
```

**逐行解析**：

- **`findhole(pages)`**：在 VM 的虚拟地址空间中找到一个足够大的空洞。
- **`pt_writemap(vmprocess, pt, loc, p, VM_PAGE_SIZE*pages, flags, 0)`**：将物理地址 `p` 开始的 `pages` 页，映射到虚拟地址 `loc`。
  - **`ARCH_VM_PTE_PRESENT`**：页存在。
  - **`ARCH_VM_PTE_USER`**：用户态可访问。
  - **`ARCH_VM_PTE_RW`**：可读写。
  - **`ARM_VM_PTE_CACHED`**：ARM 架构的缓存标志。
- **`sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)`**：刷新 TLB。因为页表被修改了，CPU 的 TLB 中可能还有旧的映射，需要刷新。
- **`return (void *) loc`**：返回虚拟地址。

**`pt_writemap()` 的作用**：

`pt_writemap()` 是 Minix3 页表操作的核心函数。它会：
1. 检查目标虚拟地址范围是否需要页表（即对应的 PDE 是否存在）。
2. 如果不存在，调用 `pt_ptalloc()` 分配页表。
3. 设置页表项（PTE），将虚拟地址映射到物理地址。
4. 处理各种标志（如 `WMF_OVERWRITE`、`WMF_FREE` 等）。

### 2.12 页表分配：`pt_ptalloc()`

```c
// minix3/minix/servers/vm/pagetable.c:494-540

static int pt_ptalloc(pt_t *pt, int pde, u32_t flags)
{
	int i;
	phys_bytes pt_phys;
	u32_t *p;

	/* Argument must make sense. */
	assert(pde >= 0 && pde < ARCH_VM_DIR_ENTRIES);
	assert(!(flags & ~(PTF_ALLFLAGS)));

	/* We don't expect to overwrite page directory entry, nor
	 * storage for the page table.
	 */
	assert(!(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT));
	assert(!pt->pt_pt[pde]);

	/* Get storage for the page table. The allocation call may in fact
	 * recursively create the directory entry as a side effect. In that
	 * case, we free the newly allocated page and do nothing else.
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
		pt->pt_pt[pde][i] = 0;	/* Empty entry. */

	/* Make page directory entry.
	 * The PDE is always 'present,' 'writable,' and 'user accessible,'
	 * relying on the PTE for protection.
	 */
#if defined(__i386__)
	pt->pt_dir[pde] = (pt_phys & ARCH_VM_ADDR_MASK) | flags
		| ARCH_VM_PDE_PRESENT | ARCH_VM_PTE_USER | ARCH_VM_PTE_RW;
#elif defined(__arm__)
	pt->pt_dir[pde] = (pt_phys & ARCH_VM_PDE_MASK)
		| ARCH_VM_PDE_PRESENT | ARM_VM_PDE_DOMAIN;
#endif

	return OK;
}
```

**逐行解析**：

- **`assert(!(pt->pt_dir[pde] & ARCH_VM_PDE_PRESENT))`**：断言该 PDE 尚未存在。`pt_ptalloc()` 不应该被调用来覆盖已有的页目录项。
- **`assert(!pt->pt_pt[pde])`**：断言该 PDE 对应的页表指针为空。
- **`vm_allocpage(&pt_phys, VMP_PAGETABLE)`**：分配一页物理内存用于存储页表。`VMP_PAGETABLE` 表示分配原因是页表。
- **`if (pt->pt_pt[pde])`**：这是一个**防御性检查**。`vm_allocpage()` 在内部可能调用 `pt_writemap()`，而 `pt_writemap()` 可能递归调用 `pt_ptalloc()`。如果递归路径上已经分配了该 PDE 的页表，那么 `pt->pt_pt[pde]` 会被设置。此时释放刚分配的页（避免泄漏），直接返回。
- **`pt->pt_pt[pde] = p`**：记录页表的虚拟地址。
- **`pt->pt_pt[pde][i] = 0`**：初始化所有页表项为 0（无效）。
- **`pt->pt_dir[pde] = ...`**：设置页目录项，指向页表的物理地址，并标记为存在、可写、用户态可访问。

**递归问题的根源**：

`pt_ptalloc()` → `vm_allocpage()` → `vm_mappages()` → `pt_writemap()` → `pt_ptalloc()`

这是一个潜在的无限递归。Minix3 通过以下机制解决：
1. **`level` 计数器**：`vm_allocpage()` 中的 `level` 限制递归深度为 2。
2. **`pt_init_done` 标志**：初始化阶段使用备用页池，不走 `alloc_mem()` + `vm_mappages()` 路径。
3. **`pt->pt_pt[pde]` 检查**：如果递归路径上已经分配了页表，直接复用。

### 2.13 内存统计：`memstats()`

```c
// minix3/minix/servers/vm/alloc.c:348-367

void memstats(int *nodes, int *pages, int *largest)
{
	int i;
	*nodes = 0;
	*pages = 0;
	*largest = 0;

	for(i = 0; i < NUMBER_PHYSICAL_PAGES; i++) {
		int size = 0;
		while(i < NUMBER_PHYSICAL_PAGES && page_isfree(i)) {
			size++;
			i++;
		}
		if(size == 0) continue;
		(*nodes)++;
		(*pages)+= size;
		if(size > *largest)
			*largest = size;
	}
}
```

**逐行解析**：

- **`nodes`**：空闲块的数量。一个"块"是连续的物理页序列。
- **`pages`**：总空闲页数。
- **`largest`**：最大的连续空闲块大小。
- **`for(i = 0; i < NUMBER_PHYSICAL_PAGES; i++)`**：遍历所有物理页。
- **`while(i < NUMBER_PHYSICAL_PAGES && page_isfree(i))`**：如果遇到空闲页，继续向后统计连续空闲页数。
- **`(*nodes)++`**：找到一个空闲块，块计数加 1。
- **`(*pages) += size`**：累加空闲页数。
- **`if(size > *largest)`**：更新最大块大小。

**时间复杂度**：O(NUMBER_PHYSICAL_PAGES)，即 O(1M)。对于 32 位系统来说，这个开销是可接受的（约 1M 次位检查）。但对于 64 位系统，这个算法不可行。

---

## 3. Rust 设计决策

### 3.1 搬迁策略选择

搬迁的核心问题是：**如何把数据从旧位置移到新位置，同时保证系统在搬迁期间的一致性**。有三种策略：

#### 策略一：原地升级（In-place Upgrade）

**思路**：不搬迁。直接把预留区域的内存"标记"为堆的一部分，PhysAllocator 的元数据永远留在原地。

```
搬迁前:  [ Reserved Region | ... ]
搬迁后:  [ Reserved Region | ... ]  ← 同一块内存，只是"身份"变了
          ↑
          └── 现在属于 VM Heap
```

**实现**：在 `into_normal()` 时，不释放预留区域，而是将其虚拟地址范围注册到 VM 的虚拟地址分配器中，标记为"已占用"。后续堆分配从预留区域之后开始。

**优点**：
- 零拷贝，最快
- 不需要更新任何引用
- 实现最简单

**缺点**：
- 预留区域的位置由内核决定，可能不在 VM 期望的堆区域
- 预留区域大小有限（通常几 MB），堆的起始位置被"钉"在这里
- 如果预留区域在虚拟地址空间的中间，会造成地址空间碎片
- 语义不干净：预留区域的物理页是内核分配的，VM 堆的物理页是 VM 自己分配的，混在一起管理复杂

#### 策略二：复制搬迁（Copy Relocation）

**思路**：在堆上分配新内存，把数据复制过去，更新引用，释放旧内存。

```
搬迁前:
  Reserved Region:  [bitmap][page_cache][free_lists]
  VM Heap:          [.......................................]

搬迁后:
  Reserved Region:  [free    ][free      ][free       ]  ← 归还
  VM Heap:          [bitmap][page_cache][free_lists][...]
```

**实现步骤**：
1. 在堆上分配与旧数组相同大小的新数组
2. memcpy 数据
3. 更新 PhysAllocator 内部指针指向新数组
4. 释放预留区域中的旧数组

**优点**：
- 堆的布局完全由 VM 控制
- 预留区域可以完全释放
- 语义清晰：Bootstrap 数据是"临时"的，Normal 数据是"永久"的

**缺点**：
- 需要一次完整拷贝
- 需要更新引用（但 SoA 结构使这很简单——只需更新几个指针）
- 搬迁期间 PhysAllocator 处于不一致状态（需要短暂暂停分配）

#### 策略三：双缓冲（Double Buffering）

**思路**：分配新数组，复制数据，然后**原子地**切换 PhysAllocator 的内部指针。搬迁期间，旧数组仍然可用。

```
搬迁前:
  PhysAllocator.bitmap ──► [Old Bitmap in Reserved Region]
  PhysAllocator.new_bitmap ──► NULL

搬迁中:
  PhysAllocator.bitmap ──► [Old Bitmap in Reserved Region]  ← 仍可用
  PhysAllocator.new_bitmap ──► [New Bitmap in Heap]          ← 已复制

切换后:
  PhysAllocator.bitmap ──► [New Bitmap in Heap]
  PhysAllocator.new_bitmap ──► NULL
  [Old Bitmap] 释放
```

**优点**：
- 搬迁期间 PhysAllocator 始终可用（旧指针仍有效）
- 切换是原子的（单条赋值语句）
- 如果搬迁失败，可以回滚（保留旧数组）

**缺点**：
- 短暂的双倍内存占用
- 实现复杂度最高
- 对于 SoA 扁平数组来说，过度设计

#### 选择：复制搬迁

对于当前场景，**复制搬迁**是最佳选择：

1. **SoA 结构使引用更新极简**：只需更新 3~5 个 slice 指针，不需要遍历对象图。
2. **搬迁窗口极短**：memcpy 几个数组只需要微秒级时间。搬迁期间暂停分配是可接受的（初始化阶段没有并发请求）。
3. **双缓冲过度设计**：双缓冲的价值在于"搬迁期间系统仍可用"，但初始化阶段没有其他线程竞争 PhysAllocator。
4. **原地升级有长期代价**：把预留区域钉在地址空间中间，后续的虚拟地址管理会变复杂。

### 3.2 搬迁的边界条件

**搬迁期间 PhysAllocator 的一致性**：

搬迁期间，PhysAllocator 的内部指针指向旧数组。搬迁完成后，指针切换到新数组。在切换瞬间，PhysAllocator 处于不一致状态。解决方案：

- 搬迁在 `init_vm()` 中执行，此时还没有其他线程
- 搬迁期间不调用 `alloc_mem()` / `free_mem()`
- 搬迁是同步的、不可中断的

**搬迁失败的处理**：

如果堆分配失败（内存不足），搬迁无法完成。此时系统无法进入 Normal 阶段。处理方式：
- panic（初始化阶段，没有恢复的必要）
- 或者回退到原地升级（保留预留区域，标记为堆的一部分）

### 3.3 与 Typestate 的配合

搬迁是 `into_normal()` 之后的独立步骤。Typestate 保证了搬迁的调用时机：

```rust
// 编译期保证：只有 Normal 阶段才能调用搬迁
impl VmPageAllocator<Normal> {
    pub(crate) fn relocate_phys_allocator(&mut self) {
        // self.pt_region 可用（由 into_normal 初始化）
        // self.phys_alloc 不可用（已被 into_normal 消耗）
        // 搬迁通过 pt_region 分配新内存
    }
}
```

---

## 4. 实现详解

### 4.1 搬迁接口

```rust
// os/servers/vm/src/alloc_page.rs

impl VmPageAllocator<Normal, RealPtOps> {
    /// 将 PhysAllocator 的元数据从预留区域搬迁到堆上。
    pub(crate) fn relocate_phys_allocator(&mut self) {
        let pt_region = self.pt_region.as_mut()
            .expect("PtRegion must be initialized before relocation");
        pt_region.relocate_phys_allocator();
    }
}
```

### 4.2 PtRegion 中的搬迁逻辑

搬迁逻辑集中在 `PtRegion::relocate_phys_allocator()` 中，而非 PhysAllocator trait 的默认方法。这样设计的原因是：搬迁需要同时使用 PhysAllocator（分配物理页）和 PtRegion（分配虚拟地址并建立映射），将逻辑放在 PtRegion 中可以自然地访问两者。

```rust
// os/servers/vm/src/pt_region.rs

impl<O: PtOps> PtRegion<O> {
    pub(crate) fn relocate_phys_allocator(&mut self) {
        let count = self.phys_alloc.reloc_array_count();
        if count == 0 {
            return;
        }

        // 1. 在堆上分配新数组（物理页 + 虚拟地址 + 映射）
        let mut new_virts: [Option<VirBytes>; 4] = [None; 4];
        let mut total_bytes: [usize; 4] = [0; 4];

        for i in 0..count {
            let (_old_ptr, elem_count, elem_size) = self.phys_alloc.reloc_array_info(i);
            let bytes = elem_count * elem_size;
            total_bytes[i] = bytes;
            let pages = (bytes + PAGE_SIZE - 1) / PAGE_SIZE;

            for _ in 0..pages {
                let phys = self.phys_alloc.alloc_mem(1, PageAllocFlags::empty())
                    .expect("relocation: failed to allocate physical page");
                let virt = self.alloc_pt_page()
                    .expect("relocation: failed to allocate virtual address");
                if new_virts[i].is_none() {
                    new_virts[i] = Some(virt);
                }
                self.write_data_pte(virt, phys);
            }
        }

        // 2. 复制数据
        for i in 0..count {
            let (old_ptr, _elem_count, _elem_size) = self.phys_alloc.reloc_array_info(i);
            let new_virt = new_virts[i].unwrap();
            unsafe {
                core::ptr::copy_nonoverlapping(old_ptr, new_virt.0 as *mut u8, total_bytes[i]);
            }
        }

        // 3. 更新 PhysAllocator 内部指针
        let new_ptrs: [*mut u8; 4] = [
            new_virts[0].map(|v| v.0 as *mut u8).unwrap_or(core::ptr::null_mut()),
            new_virts[1].map(|v| v.0 as *mut u8).unwrap_or(core::ptr::null_mut()),
            new_virts[2].map(|v| v.0 as *mut u8).unwrap_or(core::ptr::null_mut()),
            new_virts[3].map(|v| v.0 as *mut u8).unwrap_or(core::ptr::null_mut()),
        ];
        self.phys_alloc.update_relocated_arrays(&new_ptrs[..count]);
    }
}
```

### 4.3 PhysAllocator trait 的搬迁接口

搬迁接口使用分步查询设计（`reloc_array_count()` + `reloc_array_info()`），而非返回 `Vec`。这避免了搬迁期间对堆分配器的依赖（搬迁时 alloc 可能尚未就绪）：

```rust
// os/servers/vm/src/phys_mem/alloc_trait.rs

pub trait PhysAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysBytes, AllocError>;
    fn free_mem(&mut self, base: PhysBytes, clicks: usize);
    fn total_count(&self) -> usize;

    /// 需要搬迁的数组数量。
    fn reloc_array_count(&self) -> usize { 0 }

    /// 获取第 index 个数组的信息。
    /// 返回 (当前指针, 元素个数, 元素大小)。
    fn reloc_array_info(&self, _index: usize) -> (*const u8, usize, usize) {
        (core::ptr::null(), 0, 0)
    }

    /// 更新内部指针，指向新数组。
    /// new_ptrs 的顺序与 reloc_array_info() 的 index 顺序一致。
    fn update_relocated_arrays(&mut self, _new_ptrs: &[*mut u8]) {}
}
```

### 4.4 BitmapAllocator 的搬迁

```rust
// os/servers/vm/src/phys_mem/bitmap_alloc.rs

impl PhysAllocator for BitmapAllocator {
    fn reloc_array_count(&self) -> usize {
        2  // bitmap + page_cache
    }

    fn reloc_array_info(&self, index: usize) -> (*const u8, usize, usize) {
        match index {
            0 => (self.bitmap.as_ptr() as *const u8, self.bitmap.len(), 8),
            1 => (self.page_cache.as_ptr() as *const u8, self.page_cache.len(), 8),
            _ => (core::ptr::null(), 0, 0),
        }
    }

    fn update_relocated_arrays(&mut self, new_ptrs: &[*mut u8]) {
        unsafe {
            self.bitmap = core::slice::from_raw_parts_mut(
                new_ptrs[0] as *mut u64,
                self.bitmap.len(),
            );
            self.page_cache = core::slice::from_raw_parts_mut(
                new_ptrs[1] as *mut usize,
                self.page_cache.len(),
            );
        }
    }
}
```

### 4.5 BuddyAllocator 的搬迁

```rust
// os/servers/vm/src/phys_mem/buddy_alloc.rs

impl PhysAllocator for BuddyAllocator {
    fn reloc_array_count(&self) -> usize {
        3  // free_list_heads + page_next + page_orders
    }

    fn reloc_array_info(&self, index: usize) -> (*const u8, usize, usize) {
        match index {
            0 => (self.free_list_heads.as_ptr() as *const u8, self.free_list_heads.len(), 4),
            1 => (self.page_next.as_ptr() as *const u8, self.page_next.len(), 4),
            2 => (self.page_orders.as_ptr() as *const u8, self.page_orders.len(), 1),
            _ => (core::ptr::null(), 0, 0),
        }
    }

    fn update_relocated_arrays(&mut self, new_ptrs: &[*mut u8]) {
        unsafe {
            self.free_list_heads = core::slice::from_raw_parts_mut(
                new_ptrs[0] as *mut u32,
                self.free_list_heads.len(),
            );
            self.page_next = core::slice::from_raw_parts_mut(
                new_ptrs[1] as *mut u32,
                self.page_next.len(),
            );
            self.page_orders = core::slice::from_raw_parts_mut(
                new_ptrs[2] as *mut u8,
                self.page_orders.len(),
            );
        }
    }
}
```

### 4.6 搬迁的完整调用链

```rust
// os/servers/vm/src/main.rs (示意)

fn init_vm(boot_info: BootInfo) -> VmPageAllocator<Normal, RealPtOps> {
    // T2: Bootstrap
    let reserved = ReservedRegion::from_boot_info(&boot_info);
    let phys_alloc = Box::new(UninitPhysAllocator);
    let mut alloc = VmPageAllocator::<Bootstrap>::new(reserved, phys_alloc);

    // T3: 初始化 PhysAllocator（从预留区域分配元数据）
    let mem_chunks = get_mem_chunks(&boot_info);
    let phys_alloc = BitmapAllocator::init(&mut alloc, &mem_chunks);
    // ... 将 phys_alloc 注入到 alloc 中 ...

    // T4: 初始化页表
    pt_init(&mut alloc);

    // T5: 切换到 Normal
    let mut alloc = alloc.into_normal();

    // T6: 搬迁 PhysAllocator 元数据
    alloc.relocate_phys_allocator();

    alloc
}
```

---

## 5. 测试

### 5.1 搬迁后数据一致性

```rust
#[test]
fn test_relocation_data_integrity() {
    let mut bootstrap = mock_bootstrap_with_phys_alloc();
    let mut normal = bootstrap.into_normal_for_test();

    // 搬迁前记录 PhysAllocator 状态
    let stats_before = normal.pt_region.as_ref().unwrap().phys_alloc.memstats();

    // 执行搬迁
    normal.relocate_phys_allocator();

    // 搬迁后状态一致
    let stats_after = normal.pt_region.as_ref().unwrap().phys_alloc.memstats();
    assert_eq!(stats_before.free_pages, stats_after.free_pages);
    assert_eq!(stats_before.largest_free, stats_after.largest_free);
}
```

### 5.2 搬迁后可正常分配

```rust
#[test]
fn test_allocation_after_relocation() {
    let mut bootstrap = mock_bootstrap_with_phys_alloc();
    let mut normal = bootstrap.into_normal_for_test();
    normal.relocate_phys_allocator();

    // 搬迁后仍可正常分配
    let phys = normal.alloc_phys(1, PageAllocFlags::empty());
    assert!(phys.is_ok());
}
```

### 5.3 搬迁后释放再分配

```rust
#[test]
fn test_free_realloc_after_relocation() {
    let mut bootstrap = mock_bootstrap_with_phys_alloc();
    let mut normal = bootstrap.into_normal_for_test();

    // 搬迁前分配一页
    let phys = normal.alloc_phys(1, PageAllocFlags::empty()).unwrap();

    // 搬迁
    normal.relocate_phys_allocator();

    // 释放
    normal.pt_region.as_mut().unwrap().phys_alloc.free_mem(phys, 1);

    // 重新分配同一页
    let phys2 = normal.alloc_phys(1, PageAllocFlags::empty()).unwrap();
    assert_eq!(phys.as_u64(), phys2.as_u64());
}
```

---

## 6. 参见

- [04-physical-memory.md](04-physical-memory.md) - 物理页分配器（搬迁的主要对象）
- [05-vm-allocpage.md](05-vm-allocpage.md) - 页分配器（Typestate 模式，Bootstrap/Normal 阶段）
- [07-pagetable-ops.md](07-pagetable-ops.md) - 页表操作（搬迁依赖的映射能力）
- [08-slab-allocator.md](08-slab-allocator.md) - Slab 分配器（搬迁后堆的主要使用者）

---

*分类: VM库 | 使用范围: 仅 VM 内部，初始化阶段*
