# 04-physical-memory: 物理内存分配

> **分类**: VM库  
> **源码**: `minix3/minix/servers/vm/alloc.c`  
> **说明**: VM 提供的物理内存分配接口，其他服务通过 IPC 调用

---

## 1. 概述

### 1.0 物理内存信息的来源

> VM 不直接探测物理内存。它看到的"物理内存"是 kernel 通过 IPC 传递的数据描述，而非物理内存本身。

**初始化链路**（详见 [00-vm-overview.md §2.3](00-vm-overview.md)）：

```
Bootloader (GRUB) → Multiboot info → Kernel pre_init()
    → kinfo.memmap[]（扣除 kernel/modules 占用）
    → sys_getkinfo() IPC → VM: kernel_boot_info.memmap[]
    → get_mem_chunks() → mem_chunks[]（click 单位）
    → mem_init() → free_pages_bitmap[]（本文件的内容）
```

**VM 的起点**: `mem_init(mem_chunks)` 接收的 `mem_chunks` 是 kernel 已经过滤好的可用物理内存区域列表。VM 在此基础上构建分配策略，不关心这些数据最初来自 UEFI 还是 e820。

### 1.1 物理内存管理在 Minix3 中的角色

**VM 作为系统内存分配服务**

在 Minix3 微内核架构中，VM (Virtual Memory) 服务器不仅是虚拟内存管理器，还充当**系统级物理内存分配服务**的角色：

```
┌─────────────────────────────────────────────────────────────┐
│                    物理内存分配架构                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   PM (fork) ──► VM.alloc_mem() ──► 物理页分配 ──► 进程     │
│        ▲                                            │       │
│        │                                            ▼       │
│   VM.free_mem() ◄──────────────────────────── 进程退出     │
│                                                             │
│   其他服务 ──► VM.alloc_mem() ──► 共享内存、DMA 等         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**图解**：

- **上半部分**：PM 处理 `fork()` 系统调用时，需要为子进程分配内存，因此通过 IPC 向 VM 发送 `alloc_mem` 请求（不是让 VM 执行 fork，fork 的逻辑由 PM 协调）
- **下半部分**：进程退出时，VM 回收其占用的物理页
- **底部**：其他系统服务（RS、驱动等）也可能请求物理内存，用于共享内存、DMA 等场景

**为什么需要 VM 管理物理内存？**

1. **统一分配**：避免多个服务各自管理物理内存导致的碎片
2. **安全隔离**：VM 可以验证请求的合法性（防止越界访问）
3. **资源限制**：VM 可以实施进程级内存配额
4. **与虚拟内存集成**：物理分配与页表映射统一管理

### 1.2 Click 单位系统

**什么是 Click？**

Click 是 Minix3 中的内存分配基本单位，类似于 Linux 的 Page：

```c
// minix3/minix/include/minix/const.h
#define CLICK_SIZE      4096    /* unit in which memory is allocated */
#define CLICK_SHIFT       12    /* log2 of CLICK_SIZE */

// 常用宏
#define CLICK_FLOOR(n)  (((vir_bytes)(n) / CLICK_SIZE) * CLICK_SIZE)
#define CLICK_CEIL(n)   CLICK_FLOOR((vir_bytes)(n) + CLICK_SIZE-1)
#define CLICK2ABS(v)    ((v) << CLICK_SHIFT)  /* click -> bytes */
#define ABS2CLICK(a)    ((a) >> CLICK_SHIFT)  /* bytes -> click */
```

**Click vs Page**

| 特性 | Click | Page |
|------|-------|------|
| **大小** | 4096 bytes (固定) | 通常 4096 bytes |
| **用途** | 逻辑分配单位 | 硬件内存管理单位 |
| **历史** | Minix 传统 | 现代操作系统通用 |
| **API** | `alloc_mem(clicks)` | `alloc_pages(order)` |

**为什么 Minix3 使用 Click？**

1. **历史兼容性**：早期 Minix 使用 1024-byte click，后来统一到 4096
2. **命名传统**：click 是 Minix 的命名习惯，本质等于 page
3. **简化计算**：`CLICK_SHIFT = 12` 便于位运算

### 1.3 VM 内存分配接口

**核心 API**

```c
// minix3/minix/servers/vm/proto.h

/* 分配物理内存 */
phys_bytes alloc_mem(phys_bytes clicks, int flags);

/* 释放物理内存 */
void free_mem(phys_bytes addr, phys_bytes clicks);

/* 查询内存统计 */
int memstats(struct vm_stats *stats);
```

**分配标志 (flags)**

```c
// minix3/minix/servers/vm/alloc.c

#define AF_CONTIG   0x01    /* 要求连续物理内存 */
#define AF_ALIGN4K  0x02    /* 4KB 对齐 */
#define AF_ZERO     0x04    /* 清零内存 */
#define AF_LOW      0x08    /* 低端内存 (<16MB，用于 DMA) */
```

**使用场景**

| 调用者 | 用途 | 典型大小 |
|--------|------|----------|
| **PM** | fork 时分配新进程的页表和栈 | 几 clicks |
| **VM** | 页表、缓存、内部结构 | 按需 |
| **Kernel** | DMA 缓冲区 | 连续、低端内存 |
| **Drivers** | 设备缓冲区 | 连续内存 |

### 1.4 物理内存管理架构

```
┌─────────────────────────────────────────────────────────────┐
│                    VM 物理内存管理器                         │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ 空闲页位图    │  │ 保留页队列    │  │ 内存统计     │      │
│  │ (bitmap)     │  │ (reserved)   │  │ (stats)      │      │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘      │
│         │                 │                 │              │
│         ▼                 ▼                 ▼              │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              物理页分配器 (alloc.c)                  │   │
│  │  • 首次适应算法                                       │   │
│  │  • 连续物理页分配（DMA 等场景需要）                    │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**图解**：

- **空闲页位图**：核心数据结构，1 bit 表示 1 个物理页的空闲/占用状态
- **保留页队列**：内核占用的物理内存区域，不会被分配给用户进程
- **内存统计**：记录总页数、空闲页数、最低可用地址等信息
- **物理页分配器**：基于位图进行分配，支持分配连续物理页（DMA 等场景需要）

**注意**：普通进程内存不需要物理连续（通过页表映射即可），只有 DMA、大页等特殊场景才需要连续物理页。

### 1.5 与 PM 的交互

**真实流程**：PM 通过 IPC 调用 VM，VM 内部分配物理内存。

```c
// PM: minix3/minix/servers/pm/forkexit.c
int do_fork(void) {
    // PM 只调用 vm_fork IPC，不直接分配内存
    if ((s = vm_fork(rmp->mp_endpoint, next_child, &child_ep)) != OK) {
        return s;
    }
    // PM 不接触物理地址，内存分配由 VM 完成
}

// VM: minix3/minix/servers/vm/fork.c
int do_fork(message *msg) {
    // VM 内部分配页表
    if (pt_new(&vmc->vm_pt) != OK) {
        return ENOMEM;
    }
    // ...
}

// VM: minix3/minix/servers/vm/pagetable.c
int pt_new(pt_t *pt) {
    // 分配页目录，内部调用 alloc_mem
    if (!(pt->pt_dir = vm_allocpages(&pt->pt_dir_phys, ...))) {
        return ENOMEM;
    }
    // ...
}

// VM: minix3/minix/servers/vm/pagetable.c
void *vm_allocpages(phys_bytes *phys, int reason, int pages) {
    // 最终调用 alloc_mem 分配物理页
    if ((newpage = alloc_mem(pages, mem_flags)) == NO_MEM) {
        return NULL;
    }
    // ...
}
```

**调用链**：
```
PM.do_fork() 
    └── vm_fork() [IPC]
            ↓
VM.do_fork() 
    └── pt_new() 
            └── vm_allocpages() 
                    └── alloc_mem()  ← 物理内存分配
```

**关键点**：
- PM **不直接调用** `alloc_mem`，只通过 IPC 请求 VM
- PM **看不到物理地址**，物理内存管理完全由 VM 负责
- `alloc_mem()` 是 VM 内部函数，定义在 `vm/alloc.c`

---

## 2. C 源码分析

### 2.0 核心数据结构

Minix3 物理内存管理使用三个核心数据结构：

```c
// minix3/minix/servers/vm/alloc.c

#define NUMBER_PHYSICAL_PAGES (int)(0x100000000ULL/VM_PAGE_SIZE)  // 4GB / 4KB = 1M 页
#define PAGE_BITMAP_CHUNKS BITMAP_CHUNKS(NUMBER_PHYSICAL_PAGES)

// 1. 空闲页位图
static bitchunk_t free_pages_bitmap[PAGE_BITMAP_CHUNKS];

// 2. 单页缓存
#define PAGE_CACHE_MAX 10000
static int free_page_cache[PAGE_CACHE_MAX];
static int free_page_cache_size = 0;

// 3. 地址范围（用于 sanity check）
static phys_bytes mem_low, mem_high;
```

#### 2.0.1 空闲页位图 (free_pages_bitmap)

**结构**：
```
bitchunk_t = uint32_t（32 位）
每个 chunk 管理 32 个物理页

位图大小 = BITMAP_CHUNKS(1M) = 1M / 32 = 32768 个 uint32_t = 128KB
```

**含义**：
| 位值 | 含义 |
|------|------|
| 1 | 物理页空闲，可分配 |
| 0 | 物理页已用，不可分配 |

**操作宏**：
```c
#define page_isfree(i) GET_BIT(free_pages_bitmap, i)   // 检查页 i 是否空闲
SET_BIT(free_pages_bitmap, i)   // 标记为空闲
UNSET_BIT(free_pages_bitmap, i) // 标记为已用
```

**位图示例**：
```
物理页号:    0    1    2    3    4    5    6    7    ...
位图位:      1    1    0    0    0    1    1    1   ...
含义:       空闲 空闲 已用 已用 已用 空闲 空闲 空闲
```

#### 2.0.2 单页缓存 (free_page_cache)

**结构**：
```c
static int free_page_cache[PAGE_CACHE_MAX];  // 页号数组
static int free_page_cache_size = 0;          // 当前缓存数量
```

**目的**：加速单页分配，避免频繁扫描位图。

**工作原理**：
```c
// 释放单页时:
  if (free_page_cache_size < PAGE_CACHE_MAX) {
      free_page_cache[free_page_cache_size++] = page_no;  // 压栈
  }

// 分配单页时:
  if (free_page_cache_size > 0) {
      page_no = free_page_cache[--free_page_cache_size];  // 弹栈
      if (page_isfree(page_no)) return page_no;           // 验证仍空闲
  }
  // 缓存为空或失效，回退到位图扫描
```

**策略**：LIFO（后进先出），最近释放的页优先被分配。

#### 2.0.3 数据结构关系

```
┌─────────────────────────────────────────────────────────────┐
│                     物理内存管理数据结构                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   free_pages_bitmap[32768]     free_page_cache[10000]      │
│   ┌─────────────────────┐     ┌─────────────────────┐      │
│   │ 1 bit = 1 page      │     │ LIFO 栈             │      │
│   │ 1 = 空闲, 0 = 已用   │     │ 缓存最近释放的单页   │      │
│   │ 管理 1M 个物理页     │     │ 加速单页分配        │      │
│   └──────────┬──────────┘     └──────────┬──────────┘      │
│              │                           │                  │
│              └───────────┬───────────────┘                  │
│                          │                                  │
│                          ▼                                  │
│              ┌─────────────────────┐                        │
│              │   alloc_mem()       │                        │
│              │   free_mem()        │                        │
│              └─────────────────────┘                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### 2.1 物理内存分配

#### 2.1.1 alloc_mem - 分配物理内存

**函数签名**

```c
// minix3/minix/servers/vm/alloc.c
phys_clicks alloc_mem(phys_clicks clicks, u32_t memflags);
```

**功能说明**

`alloc_mem` 是 VM 提供的核心物理内存分配函数，使用**首次适应（First Fit）**算法从空闲内存列表中分配连续的物理内存块。

- **输入**: `clicks` - 请求的内存大小（以 click 为单位，1 click = 4KB）
- **输入**: `memflags` - 分配标志（见下方）
- **输出**: 物理内存的起始 click 地址，失败返回 `NO_MEM`

**分配标志 (memflags)**

```c
// minix3/minix/servers/vm/vm.h
#define PAF_CLEAR       0x01    /* 清零物理内存 */
#define PAF_CONTIG      0x02    /* 要求物理连续（定义但未使用） */
#define PAF_ALIGN64K    0x04    /* 64KB 对齐 */
#define PAF_LOWER16MB   0x08    /* 低端内存 (<16MB，用于 DMA) */
#define PAF_LOWER1MB    0x10    /* 1MB 以下内存 */
#define PAF_ALIGN16K    0x40    /* 16KB 对齐 */
```

> **历史注记**：Minix3 的 `alloc_mem()` **总是分配物理连续内存**。这是历史遗留设计：
> - Minix 1/2 运行在 8086/80286 上，没有分页 MMU，必须物理连续
> - Minix3 虽然运行在 386+ 有 MMU，但 `alloc_mem` 保留了连续分配语义以简化实现
> - `PAF_CONTIG` 定义了但在 `alloc_mem` 层是 no-op，说明曾有计划在分配器层区分"连续"和"非连续"
>
> **但实际上 Minix3 在上层 region 系统实现了非连续分配**：
> - `mem_type_anon`（默认路径）：**按需分配（lazy）**，没有 `ev_new` handler，region 创建时不预分配物理内存。进程首次访问某页时触发 pagefault → `map_pf()` → `anon_pagefault()` → `alloc_mem(1, ...)` 分配 1 页并映射。N 页虚拟区域经过 N 次独立 pagefault 后得到 N 个物理上不连续的页——因为每次分配的是当时空闲的任意一页，自然不会连续。heap、stack、普通 mmap 都走这条路。若指定 `MAP_PREALLOC`（不含 `MAP_CONTIG`），则 region 创建后立即逐页触发 pagefault 预分配，物理页仍然不连续。
> - `mem_type_anon_contig`（特殊路径）：**预分配（eager）**，有 `ev_new` handler，在 region 创建时（`map_page_region` → `ev_new` → `anon_contig_new`）一次调 `alloc_mem(N, ...)` 分配 N 页连续物理内存并全部映射。仅在 `mmap` 时指定 `MAP_CONTIG | MAP_PREALLOC` 才使用（单独 `MAP_CONTIG` 会被拒绝，返回 EINVAL）。且不能 fork、不能 resize、不能 pagefault。
>   - 不能 fork：`ev_reference` handler 返回 ENOMEM（打印 "cannot fork with physically contig memory"），但 `map_copy_region` 未检查该返回值，因此 fork 实际上仍会继续，只是 contig 区域在子进程中引用关系不正确——Minix3 的已知限制
>   - 不能 resize：`ev_resize` 返回 ENOMEM，contig 区域不可扩展
>   - 不能 pagefault：`ev_pagefault` 直接 panic，物理内存已在 `ev_new` 时预分配，不应再触发缺页
>
> 因此"连续 vs 非连续"的区分不在 `alloc_mem` 层，而在 region 层。`PAF_CONTIG` 是分配器层的未实现尝试，而 `MAP_CONTIG` 是 region 层已实现的功能。

**实现逻辑**

```c
phys_clicks alloc_mem(phys_clicks clicks, u32_t memflags)
{
    phys_clicks mem = NO_MEM, align_clicks = 0;

    // 1. 处理对齐要求
    if(memflags & PAF_ALIGN64K) {
        align_clicks = (64 * 1024) / CLICK_SIZE;  // 16 clicks
        clicks += align_clicks;
    } else if(memflags & PAF_ALIGN16K) {
        align_clicks = (16 * 1024) / CLICK_SIZE;  // 4 clicks
        clicks += align_clicks;
    }

    // 2. 尝试分配（带缓存回收重试）
    do {
        mem = alloc_pages(clicks, memflags);
    } while(mem == NO_MEM && cache_freepages(clicks) > 0);

    if(mem == NO_MEM)
        return mem;

    // 3. 调整对齐（如果请求了对齐）
    if(align_clicks) {
        phys_clicks o = mem % align_clicks;
        if(o > 0) {
            phys_clicks e = align_clicks - o;
            free_mem(mem, e);        // 释放前缀
            mem += e;                // 调整起始地址
        }
    }

    // 4. 清零内存（如果请求）
    if(memflags & PAF_CLEAR) {
        // 使用 sys_memset 清零物理内存
        sys_memset(CLICK2ABS(mem), 0, CLICK_SIZE * clicks);
    }

    return mem;
}
```

**关键设计点**

1. **首次适应算法**: 从空闲列表中找到第一个足够大的块
2. **对齐处理**: 先分配额外空间，再释放前缀以达到对齐要求
3. **缓存回收**: 分配失败时尝试回收页缓存后重试
4. **物理地址**: 返回的是 click 编号，需要 `CLICK2ABS()` 转换为字节地址

**使用示例**

```c
// 分配 16KB 普通内存
phys_clicks mem = alloc_mem(4, 0);
if(mem == NO_MEM) {
    return ENOMEM;
}
phys_bytes addr = CLICK2ABS(mem);  // 转换为字节地址

// 分配 64KB 对齐的 DMA 内存
phys_clicks dma_mem = alloc_mem(16, PAF_ALIGN64K | PAF_LOWER16MB);
if(dma_mem == NO_MEM) {
    return ENOMEM;
}
```

**错误处理**

- 内存不足: 返回 `NO_MEM`
- 对齐要求无法满足: 返回 `NO_MEM`
- 低端内存耗尽: 返回 `NO_MEM`

#### 2.1.2 分配策略

**首次适应算法（First Fit）**

Minix3 使用首次适应算法从空闲页位图中分配物理内存：

```
空闲页位图示例：
页号:  0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15
状态:  U U F F F U U F F F F  F  U  U  F  F
       ↑已用  ↑空闲块(3页)  ↑空闲块(5页)

分配 2 页：找到第一个足够大的块（页 2-4），分配页 2-3
分配 4 页：找到第一个足够大的块（页 7-11），分配页 7-10
```

**核心实现：alloc_pages**

```c
// minix3/minix/servers/vm/alloc.c
static phys_bytes alloc_pages(int pages, int memflags)
{
    phys_bytes boundary16 = 16 * 1024 * 1024 / VM_PAGE_SIZE;
    phys_bytes boundary1  =  1 * 1024 * 1024 / VM_PAGE_SIZE;
    phys_bytes mem = NO_MEM, i;
    int maxpage = NUMBER_PHYSICAL_PAGES - 1;
    static int lastscan = -1;  // 记住上次扫描位置
    int startscan, run_length;

    // 1. 根据标志限制搜索范围
    if(memflags & PAF_LOWER16MB)
        maxpage = boundary16 - 1;      // 限制在 16MB 以下
    else if(memflags & PAF_LOWER1MB)
        maxpage = boundary1 - 1;       // 限制在 1MB 以下
    else {
        // 2. 单页分配：优先从页缓存获取
        if(pages == 1) {
            while(free_page_cache_size > 0) {
                i = free_page_cache[free_page_cache_size-1];
                if(page_isfree(i)) {
                    free_page_cache_size--;
                    mem = i;
                    break;
                }
                free_page_cache_size--;
            }
        }
    }

    // 3. 确定扫描起始位置（循环扫描优化）
    if(lastscan < maxpage && lastscan >= 0)
        startscan = lastscan;
    else
        startscan = maxpage;

    // 4. 使用 findbit 查找空闲块（首次适应）
    if(mem == NO_MEM)
        mem = findbit(0, startscan, pages, memflags, &run_length);
    if(mem == NO_MEM)
        mem = findbit(0, maxpage, pages, memflags, &run_length);
    if(mem == NO_MEM)
        return NO_MEM;

    // 5. 记住位置供下次使用
    lastscan = mem;

    // 6. 标记页为已用
    for(i = mem; i < mem + pages; i++) {
        UNSET_BIT(free_pages_bitmap, i);
    }

    return mem;
}
```

**findbit：首次适应查找**

```c
static int findbit(int low, int startscan, int pages, int memflags, int *len)
{
    int run_length = 0, i;            // run_length: 当前连续空闲页数
    int freerange_start = startscan;  // 当前空闲块起始页号

    for(i = startscan; i >= low; i--) {  // 从高地址向低地址扫描
        if(!page_isfree(i)) {
            // 遇到已用页，尝试跳过整个位图块（优化）
            int chunk = i/BITCHUNK_BITS, moved = 0;
            run_length = 0;  // 重置连续计数
            // 检查当前 chunk 是否全部已用，若是则跳到前一个 chunk
            while(chunk > 0 && !MAP_CHUNK(free_pages_bitmap, chunk*BITCHUNK_BITS)) {
                chunk--;
                moved = 1;
            }
            // 跳到有空闲页的 chunk 末尾，下次循环 i-- 后从该 chunk 开始检查
            if(moved) { i = chunk * BITCHUNK_BITS + BITCHUNK_BITS; }
            continue;
        }
        // 当前页空闲，累积连续空闲页计数
        if(!run_length) { 
            freerange_start = i;  // 新空闲块起始
            run_length = 1; 
        } else { 
            freerange_start--;    // 起始位置前移（向低地址）
            run_length++; 
        }
        
        if(run_length == pages) {
            *len = run_length;
            return freerange_start;  // 找到足够大的连续块
        }
    }
    return NO_MEM;  // 未找到足够大的连续块
}
```

**内存碎片管理**

| 策略 | 说明 |
|------|------|
| **页缓存** | 单页分配优先从缓存获取，减少位图扫描 |
| **循环扫描** | `lastscan` 记住上次位置，避免每次都从高位扫描 |
| **块跳过优化** | `findbit` 中跳过整个空位图块，加速扫描 |
| **连续分配优先** | 尽量从上次位置附近分配，保持大块连续 |

**碎片问题**

```
初始：一个大块 [0-400]

分配 100, 100, 100 → [0-99][100-199][200-299] 已用，[300-400] 空闲（101页）
释放中间块 → [0-99]已用 [100-199]空闲（100页） [200-299]已用 [300-400]空闲（101页）

问题：需要 150 页连续内存时，总空闲 = 201 > 150，但最大连续块 = 101 < 150，无法分配
```

**缓解策略**

1. **保留队列（Reserved Queue）**：预分配连续内存页，用于内核 spare pages 等关键场景（见 `reservedqueues[]`）
2. **循环扫描**：`lastscan` 记住上次分配位置，减少重复扫描已分配区域，是性能优化
3. **内存压缩**：Minix3 **不支持**，碎片严重时依赖重启清理

**性能特征**

| 操作 | 时间复杂度 | 说明 |
|------|-----------|------|
| 单页分配 | O(1) | 优先从缓存获取 |
| 多页分配 | O(n) | n = 物理页数，最坏情况扫描整个位图 |
| 释放 | O(1) | 标记位图位，可能加入缓存 |

### 2.2 物理内存释放

#### 2.2.1 free_mem - 释放物理内存

**函数签名**

```c
// minix3/minix/servers/vm/alloc.c
void free_mem(phys_clicks base, phys_clicks clicks);
```

**功能说明**

`free_mem` 释放由 `alloc_mem` 分配的物理内存块，将其归还到系统的空闲页位图中。

- **输入**: `base` - 要释放的内存起始 click 地址
- **输入**: `clicks` - 要释放的内存大小（以 click 为单位）
- **注意**: 释放的内存必须是由 `alloc_mem` 分配的连续块

**实现逻辑**

```c
void free_mem(phys_clicks base, phys_clicks clicks)
{
    // 空块直接返回
    if (clicks == 0) return;

    // 确保 click 大小等于页大小
    assert(CLICK_SIZE == VM_PAGE_SIZE);
    
    // 调用底层释放函数
    free_pages(base, clicks);
}
```

**核心实现：free_pages**

```c
static void free_pages(phys_bytes pageno, int npages)
{
    int i, lim = pageno + npages - 1;

#if JUNKFREE
    // 调试模式：用特定模式填充释放的内存
    if(sys_memset(NONE, 0xa5a5a5a5, VM_PAGE_SIZE * pageno,
            VM_PAGE_SIZE * npages) != OK)
        panic("free_pages: sys_memset failed");
#endif

    // 遍历所有页，标记为空闲
    for(i = pageno; i <= lim; i++) {
        // 在位图中设置该页为空闲
        SET_BIT(free_pages_bitmap, i);
        
        // 单页缓存：加速后续单页分配
        if(free_page_cache_size < PAGE_CACHE_MAX) {
            free_page_cache[free_page_cache_size++] = i;
        }
    }
}
```

**释放流程**

```
释放前：
页号:  0 1 2 3 4 5 6 7 8 9
状态:  U U U U U F F F F F
           ↑释放页 2-4（已用）

释放后：
页号:  0 1 2 3 4 5 6 7 8 9
状态:  U U F F F F F F F F
           ↑页 2-4 变为空闲
```

**使用示例**

```c
// 分配 16KB 内存
phys_clicks mem = alloc_mem(4, 0);
if(mem == NO_MEM) {
    return ENOMEM;
}

// 使用内存...

// 释放内存
free_mem(mem, 4);
```

**注意事项**

1. **地址对齐**: 释放的地址必须是 click 边界对齐的
2. **大小匹配**: 释放的大小必须与分配时一致（或为其子集）
3. **重复释放**: 系统不检测重复释放，可能导致位图不一致
4. **无效地址**: 释放未分配的地址可能导致位图损坏

### 2.3 内存统计与资源限制

#### 2.3.1 系统级内存统计

**total_pages**: 系统总物理内存页数
- 在 VM 初始化时从内核获取
- 用于计算内存使用率和内存压力

**内存压力检测**:
- 当空闲内存低于阈值时，触发内存回收
- 可能涉及交换（swapping）或 OOM 处理

#### 2.3.2 进程级内存统计 (vm_total / vm_total_max)

**vm_total**: 当前进程已分配的虚拟内存总量
- 单位: bytes
- 更新时机: 分配/释放虚拟区域时

**vm_total_max**: 历史最大虚拟内存使用量（high water mark）
- 记录进程运行期间 `vm_total` 达到的最大值
- 用于 `getrusage()` 系统调用的 `ru_maxrss` 字段
- **不是资源限制**，只是统计信息

**更新逻辑**（见 `region.c`）:
```c
// 分配新页时
proc->vm_total += VM_PAGE_SIZE;
if (proc->vm_total > proc->vm_total_max)
    proc->vm_total_max = proc->vm_total;  // 更新历史最大值

// 释放页时
proc->vm_total -= VM_PAGE_SIZE;
// vm_total_max 不减少，保留历史峰值
```

**getrusage 返回**:
```c
r_usage.ru_maxrss = vmp->vm_total_max / 1024L;  // 单位 KB
```

**fork 时的处理**:
- 子进程继承父进程的 `vm_total`
- `vm_total_max` 初始化为 `vm_total`（因为子进程初始内存就是当前值）
- 子进程后续独立维护自己的统计

---

## 3. Rust 设计决策

### 3.0 问题建模

物理内存分配的本质是一个**连续区间分配问题**：

```
输入：
  - 空闲区间集合：[(0, 100), (200, 300), (500, 600)]
  - 请求大小：k 页
  - 约束条件：对齐、地址范围限制

输出：
  - 分配结果：起始页号（或失败）
  - 剩余空闲区间更新
```

**核心操作只有两个**（与 Minix3 API 精确对应）：

| 操作 | Minix3 C | 语义 |
|------|----------|------|
| 分配 | `alloc_mem(clicks, flags)` | 找到 k 个连续空闲页，标记为已用 |
| 释放 | `free_mem(base, clicks)` | 将 k 个连续页标记为空闲 |

**约束条件**（通过 `flags` 参数传递）：

| 约束 | flag | 说明 |
|------|------|------|
| 对齐 | `ALIGN64K` / `ALIGN16K` | 起始地址必须对齐到指定边界 |
| 地址范围 | `LOWER16MB` / `LOWER1MB` | 限制在低端内存（DMA 需求） |
| 清零 | `CLEAR` | 分配后清零物理页 |
| 连续 | `CONTIG` | 要求物理连续（bitmap 默认行为） |

**抽象为 trait**：

```rust
pub(crate) trait PhysMemAlloc {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysAddr, AllocError>;
    fn free_mem(&mut self, base: PhysAddr, clicks: usize);
}
```

这个 trait 与 Minix3 的 `alloc_mem()` / `free_mem()` 精确对应，不添加额外方法。三种实现（Bitmap / SegmentTree / Buddy）共享同一接口，可以互换。

### 3.1 PhysAddr — 物理地址 newtype

**设计目标**: 类型安全地区分物理地址和虚拟地址，防止混用。

```rust
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PhysAddr(u64);

impl PhysAddr {
    pub(crate) const fn new(addr: u64) -> Self { PhysAddr(addr) }
    pub(crate) const fn as_u64(&self) -> u64 { self.0 }
    pub(crate) const fn as_usize(&self) -> usize { self.0 as usize }
    pub(crate) fn from_page_index(idx: usize) -> Self {
        PhysAddr((idx as u64) * CLICK_SIZE as u64)
    }
    pub(crate) fn page_index(&self) -> usize {
        (self.0 as usize) / CLICK_SIZE
    }
    pub(crate) const fn add(&self, offset: usize) -> Self {
        PhysAddr(self.0 + offset as u64)
    }
}
```

**与 Minix3 C 的对比**:

| Minix3 C | Rust |
|----------|------|
| `phys_bytes` (typedef u32) | `PhysAddr(u64)` newtype |
| `NO_MEM` sentinel (0) | `Option<PhysAddr>` (None = 分配失败) |
| `CLICK2ABS(v)` 宏 | `PhysAddr::from_page_index(idx)` |
| `ABS2CLICK(a)` 宏 | `addr.page_index()` |

**关键改进**:
- 移除 `NO_MEM` 哨兵值：物理地址 0 是合法地址（实模式 IVT），用 `Option<PhysAddr>` 替代
- 字段私有：`PhysAddr(u64)` 而非 `PhysAddr(pub u64)`，通过方法访问
- 64 位：Minix3 用 `u32` 表示物理地址，Rust 用 `u64`

### 3.2 PageAllocFlags — PAF_* 的 bitflags 表达

```rust
bitflags::bitflags! {
    pub(crate) struct PageAllocFlags: u32 {
        const CLEAR = 0x01;       // PAF_CLEAR: 清零物理内存
        const CONTIG = 0x02;      // PAF_CONTIG: 要求物理连续
        const ALIGN64K = 0x04;    // PAF_ALIGN64K: 64KB 对齐
        const LOWER16MB = 0x08;   // PAF_LOWER16MB: 低端内存
        const LOWER1MB = 0x10;    // PAF_LOWER1MB: 1MB 以下
        const ALIGN16K = 0x40;    // PAF_ALIGN16K: 16KB 对齐
    }
}
```

**与 Minix3 C 的对比**:

| Minix3 C | Rust | 值 |
|----------|------|----|
| `#define PAF_CLEAR 0x01` | `PageAllocFlags::CLEAR` | 0x01 |
| `#define PAF_CONTIG 0x02` | `PageAllocFlags::CONTIG` | 0x02 |
| `#define PAF_ALIGN64K 0x04` | `PageAllocFlags::ALIGN64K` | 0x04 |
| `#define PAF_LOWER16MB 0x08` | `PageAllocFlags::LOWER16MB` | 0x08 |
| `#define PAF_LOWER1MB 0x10` | `PageAllocFlags::LOWER1MB` | 0x10 |
| `#define PAF_ALIGN16K 0x40` | `PageAllocFlags::ALIGN16K` | 0x40 |

**PAF_CLEAR 的处理**: Minix3 中 `PAF_CLEAR` 调用 `sys_memset()` 让 kernel 清零物理页。VM 不直接操作物理内存，因此此 flag 仅记录在分配请求中，由调用方通过 kernel IPC 执行清零。

### 3.3 BitmapAllocator — bitmap 分配器（默认实现）

**核心设计**: 与 Minix3 的 `alloc.c` 一致，使用位图管理物理页状态。实现 `PhysMemAlloc` trait。

```rust
pub(crate) struct BitmapAllocator {
    bitmap: Vec<u64>,          // free_pages_bitmap: 1 bit = 1 page
    total_pages: usize,        // 系统总物理页数
    free_pages: usize,         // 当前空闲页数
    page_cache: Vec<usize>,    // 单页缓存（LIFO）
    last_scan: Option<usize>,  // 循环分配：上次扫描位置
    stats: MemStats,           // 内存统计
    mem_low: usize,            // 最低物理地址
    mem_high: usize,           // 最高物理地址
}
```

**与 Minix3 C 的对比**:

| Minix3 C | Rust |
|----------|------|
| `bitchunk_t free_pages_bitmap[]` | `Vec<u64>` bitmap |
| `int free_page_cache[]` | `Vec<usize>` page_cache |
| `static int total_pages` | `total_pages: usize` |
| `static int lastscan` | `last_scan: Option<usize>` |
| `phys_clicks alloc_mem()` | `fn alloc_mem() -> Result<PhysAddr, AllocError>` |
| `void free_mem()` | `fn free_mem()` |
| `void mem_init()` | `fn init(regions: &[BootMemRegion])` |

**初始化入口**: `PhysMemAllocator::init(regions: &[BootMemRegion])` 对应 Minix3 的 `mem_init(struct memory *chunks)`，从 kernel 传入的内存区域初始化位图。

**64 位系统的 bitmap 大小**：

Minix3 预分配整个 32 位地址空间的 bitmap（128KB），但 64 位系统不能这样做：
```
64 位地址空间 = 2^64 bytes → bitmap 需要 512 PB（不可行）
```

实际做法：只管理**实际存在的物理内存**。从 `BootMemRegion` 计算最高物理地址 `mem_high`，按需分配 bitmap：
```
max_page = mem_high / 4KB
bitmap 大小 = max_page / 64 个 u64

例如 16GB 物理内存：
  max_page = 16GB / 4KB = 4M 页
  bitmap = 4M / 64 = 62500 个 u64 = 500KB
```

**算法**: 首次适应 + 从高地址向低地址扫描 + 单页缓存，与 Minix3 `alloc_pages()` / `findbit()` 一致。

### 3.4 AllocError — 区分失败原因

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AllocError {
    OutOfMemory,        // 总内存不足
    LowMemoryExhausted, // 低端内存耗尽
    // ContiguityFailed — Minix3 定义了 PAF_CONTIG 标志，说明曾考虑过非连续物理分配，
    // 但最终 alloc_pages 始终返回连续物理页，PAF_CONTIG 实际上是 no-op。
    // 事实上只有 DMA 等少数硬件需要连续物理地址，普通进程通过页表映射后
    // 虚拟地址连续即可，物理地址是否连续无关紧要。
    // 若未来实现非连续物理分配（scatter-gather），可恢复此变体：
    // ContiguityFailed,
}
```

**与 Minix3 C 的对比**: Minix3 只返回 `NO_MEM`，不区分失败原因。`AllocError` enum 是合格的 Rewrite——外部语义不变（分配失败），内部表达更精确。`AlignmentFailed` 已移除：对齐分配通过多分配 + 释放 excess 实现，对齐失败等价于内存不足，用 `OutOfMemory` 即可。`ContiguityFailed` 保留为注释：当前只实现连续分配，若未来支持 scatter-gather 可恢复。

### 3.5 PhysMemAlloc trait — 可替换的分配器接口

```rust
pub(crate) trait PhysMemAlloc {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysAddr, AllocError>;
    fn free_mem(&mut self, base: PhysAddr, clicks: usize);
}
```

trait 只包含两个方法，与 Minix3 的 `alloc_mem()` / `free_mem()` 精确对应。各实现可自行提供额外查询方法（如 `largest_free()`、`stats()`），但这些不属于 trait 接口。

三种实现均实现此 trait，可通过 `dyn PhysMemAlloc` 动态派发互换：

```rust
let mut alloc: &mut dyn PhysMemAlloc = &mut BitmapAllocator::init(&regions);
let addr = alloc.alloc_mem(4, PageAllocFlags::empty())?;
alloc.free_mem(addr, 4);
```

### 3.6 MemStats — 非 atomic 统计

```rust
pub(crate) struct MemStats {
    total_allocations: usize,
    total_deallocations: usize,
    active_allocations: usize,
    allocation_failures: usize,
    total_allocated_bytes: usize,
    total_freed_bytes: usize,
    current_allocated_bytes: usize,
    peak_allocated_bytes: usize,
}
```

**关键设计**: 所有字段为普通 `usize`，不使用 `AtomicUsize`。VM 是单线程服务，原子操作是不必要的性能开销和误导。

### 3.7 删除的设计

以下设计在 review 后被删除：

| 删除项 | 原因 |
|--------|------|
| `PhysFrame` (Rc/RefCell/Weak) | VM 是 no_std 单线程服务，不应使用 std 堆分配智能指针。Minix3 用显式 `free_mem()` 管理 |
| `PhysMemGlobalAlloc` | VM 是 no_std 服务，不应实现 `GlobalAlloc`。物理内存分配 ≠ 堆分配 |
| `AllocFlags` (旧值) | 与 Minix3 PAF_* 值不对应，已修正为 `PageAllocFlags` |
| `NO_MEM` 哨兵值 | 物理地址 0 是合法地址，改用 `Option<PhysAddr>` |
| `ReservedQueue` (magic number) | 使用 `0x6e4c74d5` 验证状态是 C 式设计，待后续用 `Option` 重新实现 |

### 3.8 三种实现方案对比

三种分配器均实现 `PhysMemAlloc` trait，共享相同接口，但内部算法和性能特征截然不同：

| 维度 | BitmapAllocator | SegmentTreeAllocator | BuddyAllocator |
|------|----------------|---------------------|----------------|
| **时间复杂度** | O(n) 扫描 | O(log n) 查询 | O(log n) 分裂/合并 |
| **精确分配** | ✅ 任意大小 | ✅ 任意大小 | ❌ 向上取整到 2^n |
| **抗碎片** | ❌ 无自动合并 | ❌ 无自动合并 | ✅ buddy 自动合并 |
| **内存开销** | 1x (1 bit/page) | ~8x (4 usize/node) | ~2x (order+free per page) |
| **实现复杂度** | ⭐ 低 | ⭐⭐⭐⭐ 高 | ⭐⭐ 中 |
| **cache 友好** | ⭐⭐⭐⭐ 顺序访问 | ⭐ 树跳跃 | ⭐⭐⭐ 链表遍历 |
| **Minix3 对应** | ✅ 原始实现 | ❌ 新增 | ❌ 新增 |

**BitmapAllocator** — Minix3 的原始方案，简单可靠：

```
位图：[1][1][0][0][0][1][1][1][0]...
         ↑  连续3页空闲

分配：线性扫描找连续 k 个 1
释放：将对应位设为 1
```

- 优点：实现简单，内存开销最小，cache 友好（顺序扫描）
- 缺点：O(n) 扫描，大内存时性能下降；释放后不自动合并相邻空闲区

**SegmentTreeAllocator** — 算法最优解，工程上少用：

```
线段树节点：[max_free, left_free, right_free, len]

        [7,0,7,8]           ← 根节点：最大连续7页
       /          \
   [3,0,3,4]    [4,4,4,4]   ← 左子树最大3页，右子树4页
   /      \      /      \
 [1,0,1,2][2,2,2,2] ...    ← 叶节点：每页状态

分配：O(log n) 沿树查找
释放：O(log n) 自底向上 merge
```

- 优点：O(log n) 分配/释放，精确分配任意大小，可查询 `largest_free()`
- 缺点：内存开销大（每节点 4 个 usize），实现复杂（跨边界匹配、merge 逻辑），cache 不友好
- 关键实现细节：`merge(left, right)` 计算 `left_free`、`right_free`、`max_free`（含跨边界连续空闲），`find_in_subtree` 优先查左子树，再查跨边界，最后查右子树

**BuddyAllocator** — Linux 内核方案，工程最佳实践：

```
阶数  free_list
 3    [0]           ← 1 个 8 页块
 2    []            
 1    []            
 0    []            

分配 3 页 → order=2 (4页块) → 分裂 order3 → 得到 [0-3]，剩余 [4-7] 进 order2
释放 [0-3] → 检查 buddy [4-7] 是否空闲 → 合并为 order3
```

- 优点：O(log n) 分配/释放，buddy 自动合并抗碎片，实现复杂度适中
- 缺点：内部碎片（分配 3 页实际占 4 页），只支持 2^n 大小块
- 关键实现细节：`buddy_of(page, order) = page ^ (1 << order)`，`try_merge` 循环合并直到无法合并

### 3.9 设计哲学：为什么 Linux 选 Buddy 而非 Segment Tree？

三种方案代表三种不同的设计哲学：

**Bitmap = 暴力搜索**
- 不做任何优化，直接遍历
- 适合小规模（Minix3 面向小系统，内存通常 < 4GB）
- Minix3 的设计哲学：简单至上

**Segment Tree = 更强算法**
- 用更复杂的数据结构解决同一个问题
- 理论最优，但工程代价高（内存开销、实现复杂度、cache 不友好）
- "用更锋利的刀切同一块肉"

**Buddy = 改变问题**
- 不是用更强算法解决"连续区间分配"，而是**改变问题本身**
- 将"任意大小连续分配"改为"2 的幂次大小分配"
- 通过结构约束（只允许 2^n 块）换取自动抗碎片
- "换一块更好切的肉"

**OS 设计的核心洞察**：

> 不是"更强算法"，而是"更匹配 workload"。

物理内存分配的典型 workload 特征：
1. **请求大小分布**：大部分请求是 1 页或少量页，大块连续请求罕见
2. **生命周期**：分配-释放频繁交替，需要快速合并
3. **碎片危害**：外部碎片导致大块连续分配失败，远比内部碎片（多占几页）严重

Buddy 的 2^n 约束带来的内部碎片（平均浪费 25%）远小于 bitmap/segment tree 的外部碎片风险。这是 OS 设计中"**用确定性代价换不确定性风险**"的典型模式。

**Minix3 选择 Bitmap 的原因**：
- 系统规模小，O(n) 扫描可接受
- 实现简单，代码量少
- Minix3 的设计目标是教学和简洁，而非极致性能

**如果 minix-rs 要追求工程最优**：BuddyAllocator 是更好的选择，但当前默认使用 BitmapAllocator 以保持与 Minix3 一致。

---

## 4. 实现详解

> 实际代码位于 `os/servers/vm/src/phys_mem/`。
> 核心实现在 `bitmap_alloc.rs` / `segment_tree_alloc.rs` / `buddy_alloc.rs`，测试在各文件内联 `#[cfg(test)]` 模块和 `allocator_tests.rs`。

### 4.1 文件结构

| 文件 | 职责 |
|------|------|
| `mod.rs` | 模块入口，导出类型，Click 单位转换工具函数，三种分配器对比文档 |
| `types.rs` | `PhysAddr` newtype，`PageAllocFlags` bitflags，`AllocError` enum |
| `alloc_trait.rs` | `PhysMemAlloc` trait 定义 |
| `bitmap_alloc.rs` | `BitmapAllocator` — bitmap 分配器（默认，与 Minix3 一致） |
| `segment_tree_alloc.rs` | `SegmentTreeAllocator` — 线段树分配器（O(log n)，精确分配） |
| `buddy_alloc.rs` | `BuddyAllocator` — buddy 分配器（O(log n)，抗碎片） |
| `stats.rs` | `MemStats` 非 atomic 内存统计 |
| `allocator_tests.rs` | 统一测试套件（三种分配器共享测试用例） |

### 4.2 核心算法（与 Minix3 对应关系）

| Minix3 函数 | Rust 方法 | 说明 |
|-------------|-----------|------|
| `mem_init(chunks)` | `BitmapAllocator::init(regions)` | 从 kernel 传入的内存区域初始化位图 |
| `alloc_mem(clicks, flags)` | `alloc_mem(clicks, flags)` | bitmap 首次适应分配（trait 方法） |
| `alloc_pages(pages, flags)` | `alloc_pages(pages, flags)` | 内部分配，含页缓存和循环扫描 |
| `findbit(low, startscan, pages)` | `find_bit(low, start_scan, pages)` | 从高地址向低地址扫描连续空闲页 |
| `free_mem(base, clicks)` | `free_mem(base, clicks)` | 位图标记 + 页缓存更新（trait 方法） |
| `free_pages(pageno, npages)` | `free_pages_internal(start, num)` | 内部释放，逐页 SET_BIT |
| `cache_freepages(needed)` | `cache_freepages(needed)` | 保留页回收（当前为 stub） |
| `memstats(nodes, pages, largest)` | `memstats()` | 遍历位图统计空闲区域 |

### 4.3 位图操作

Rust 使用 `Vec<u64>` 替代 C 的 `bitchunk_t[]`：

| Minix3 C | Rust | 说明 |
|----------|------|------|
| `bitchunk_t` (uint32_t) | `u64` | 64 位，每个 chunk 管理 64 页 |
| `BITCHUNK_BITS = 32` | `BITS_PER_CHUNK = 64` | 位数翻倍 |
| `GET_BIT/SET_BIT/UNSET_BIT` 宏 | 内联方法 | 类型安全 |

```rust
fn is_free(&self, page: usize) -> bool {
    let chunk = page / 64;
    let bit = page % 64;
    (self.bitmap[chunk] >> bit) & 1 == 1
}

fn set_free(&mut self, page: usize) {
    let chunk = page / 64;
    let bit = page % 64;
    self.bitmap[chunk] |= 1 << bit;
}

fn set_used(&mut self, page: usize) {
    let chunk = page / 64;
    let bit = page % 64;
    self.bitmap[chunk] &= !(1 << bit);
}
```

### 4.4 单页缓存

Rust 实现与 Minix3 一致，使用 `Vec<usize>` 作为 LIFO 栈：

```rust
const PAGE_CACHE_MAX: usize = 10000;

page_cache: Vec<usize>,      // LIFO 栈
```

**差异**：Rust 版本在分配时会验证缓存中的页是否仍空闲（与 Minix3 一致）。

---

## 5. IPC 接口与权限控制

> IPC 消息处理和 ACL 权限检查不属于物理内存分配模块。
> 详见 [03-acl.md](03-acl.md) 和 [ipc 模块](../../)。

---

## 6. 测试与验证

> 测试代码位于 `os/servers/vm/src/phys_mem/allocator_tests.rs`。
> 运行: `cargo test -p minix-vm -- phys_mem`

统一测试套件覆盖三种分配器，测试模块包括：

| 模块 | 测试内容 |
|------|----------|
| `basic` | 基本分配/释放、零页分配、单页分配 |
| `exhaustion` | 内存耗尽、全部分配后全部释放 |
| `free_realloc` | 释放后重新分配、合并验证 |
| `flags` | ALIGN16K/ALIGN64K 对齐、LOWER16MB/LOWER1MB 地址限制、CONTIG 连续性 |
| `fragmentation` | 碎片场景、largest_free 追踪 |
| `multi_region` | 多内存区域、间隔区域 |
| `stress` | 随机分配/释放压力测试 |
| `trait_object` | `dyn PhysMemAlloc` 动态派发、实现互换 |
| `buddy_internal_frag` | Buddy 内部碎片特性 |

---

## 7. 参见

- [00-vm-overview.md §2.3](00-vm-overview.md) - 启动阶段物理内存初始化链路
- [03-acl.md](03-acl.md) - ACL 权限控制
- [08-slab-allocator.md](08-slab-allocator.md) - VM 内部使用 Slab
- [15-vm-fork.md](15-vm-fork.md) - fork 时的内存分配
- [17-vm-map.md](17-vm-map.md) - VM_MAP 服务中的内存分配

---

*分类: VM库 | 接口性质: 其他服务通过 IPC 调用*
