# 04-physical-memory: 物理内存分配

> **分类**: VM库\
> **源码**: `minix3/minix/servers/vm/alloc.c`\
> **说明**: VM 提供的物理内存分配接口，其他服务通过 IPC 调用

***

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

- **上半部分**：PM 处理 `fork()` 系统调用时，通过 IPC 向 VM 发送 `VM_FORK` 请求。VM 收到请求后，内部分配页表和物理内存（调用 `pt_new()` → `alloc_mem()`），并复制父进程的内存映射。fork 的整体逻辑由 PM 协调。（注意：`alloc_mem()` 是 VM 内部函数，不是 IPC 接口）
- **下半部分**：进程退出时，PM 发送 `VM_EXIT` 请求，VM 调用 `free_proc()` 回收其占用的物理页和页表
- **底部**：其他系统服务（RS、驱动等）通过 `VM_MMAP`、`VM_MAP_PHYS`、`VM_ADDDMA` 等 IPC 接口请求内存，VM 内部调用 `alloc_mem()` 完成分配

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

| 特性      | Click               | Page                 |
| ------- | ------------------- | -------------------- |
| **大小**  | 4096 bytes (固定)     | 通常 4096 bytes        |
| **用途**  | 逻辑分配单位              | 硬件内存管理单位             |
| **历史**  | Minix 传统            | 现代操作系统通用             |
| **API** | `alloc_mem(clicks)` | `alloc_pages(order)` |

**为什么 Minix3 使用 Click？**

1. **历史兼容性**：早期 Minix 使用 1024-byte click，后来统一到 4096
2. **命名传统**：click 是 Minix 的命名习惯，本质等于 page
3. **简化计算**：`CLICK_SHIFT = 12` 便于位运算

### 1.3 VM 内存分配接口

**核心 API**

```c
// minix3/minix/servers/vm/proto.h

/* 分配物理内存（返回 click 编号，非字节地址） */
phys_clicks alloc_mem(phys_clicks clicks, u32_t flags);

/* 释放物理内存 */
void free_mem(phys_clicks base, phys_clicks clicks);

/* 查询内存统计 */
void memstats(int *nodes, int *pages, int *largest);
```

> **类型说明**：
>
> - `phys_clicks`: 物理 click 编号（1 click = 4KB），用于分配/释放
> - `phys_bytes`: 物理字节地址，用于实际内存访问
> - 转换：`CLICK2ABS(clicks)` → 字节地址，`ABS2CLICK(bytes)` → click 编号

**分配标志 (flags)**

```c
// minix3/minix/servers/vm/vm.h

#define PAF_CLEAR       0x01    /* 清零物理内存 */
#define PAF_CONTIG      0x02    /* 要求物理连续（定义但未使用） */
#define PAF_ALIGN64K    0x04    /* 64KB 对齐 */
#define PAF_LOWER16MB   0x08    /* 低端内存 (<16MB，用于 DMA) */
#define PAF_LOWER1MB    0x10    /* 1MB 以下内存 */
#define PAF_ALIGN16K    0x40    /* 16KB 对齐 */
```

**使用场景**

| 调用者         | 用途               | 典型大小     |
| ----------- | ---------------- | -------- |
| **PM**      | fork 时分配新进程的页表和栈 | 几 clicks |
| **VM**      | 页表、缓存、内部结构       | 按需       |
| **Kernel**  | DMA 缓冲区          | 连续、低端内存  |
| **Drivers** | 设备缓冲区            | 连续内存     |

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
- **保留页队列**：预分配备用页（spare pages），用于页表操作等关键路径，确保内存紧张时也能成功分配。
- **内存统计**：记录总页数、空闲页数、最低可用地址等信息
- **物理页分配器**：基于位图进行分配，支持分配连续物理页（DMA 等场景需要）

> **Rust 实现状态**：保留页队列在 Rust 版本中暂未实现，这是待补充的重要功能（见 §3.7）。 TODO

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

***

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

#### 2.0.1 空闲页位图 (free\_pages\_bitmap)

**结构**：

```
bitchunk_t = uint32_t（32 位）
每个 chunk 管理 32 个物理页

位图大小 = BITMAP_CHUNKS(1M) = 1M / 32 = 32768 个 uint32_t = 128KB
```

**含义**：

| 位值 | 含义         |
| -- | ---------- |
| 1  | 物理页空闲，可分配  |
| 0  | 物理页已用，不可分配 |

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

#### 2.0.2 单页缓存 (free\_page\_cache)

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

#### 2.1.1 alloc\_mem - 分配物理内存

**函数签名**

```c
// minix3/minix/servers/vm/alloc.c
phys_clicks alloc_mem(phys_clicks clicks, u32_t memflags);
```

**功能说明**

`alloc_mem` 是 VM 提供的核心物理内存分配函数，使用\*\*首次适应（First Fit）\*\*算法从空闲内存列表中分配连续的物理内存块。

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
>
> - Minix 1/2 运行在 8086/80286 上，没有分页 MMU，必须物理连续
> - Minix3 虽然运行在 386+ 有 MMU，但 `alloc_mem` 保留了连续分配语义以简化实现
> - `PAF_CONTIG` 定义了但在 `alloc_mem` 层是 no-op，说明曾有计划在分配器层区分"连续"和"非连续"
>
> **但实际上 Minix3 在上层 region 系统实现了非连续分配**：
>
> - `mem_type_anon`（默认路径）：**按需分配（lazy）**，没有 `ev_new` handler，region 创建时不预分配物理内存。进程首次访问某页时触发 pagefault → `map_pf()` → `anon_pagefault()` → `alloc_mem(1, ...)` 分配 1 页并映射。N 页虚拟区域经过 N 次独立 pagefault 后得到 N 个物理上不连续的页——因为每次分配的是当时空闲的任意一页，自然不会连续。heap、stack、普通 mmap 都走这条路。若指定 `MAP_PREALLOC`（不含 `MAP_CONTIG`），则 region 创建后立即逐页触发 pagefault 预分配，物理页仍然不连续。
> - `mem_type_anon_contig`（特殊路径）：**预分配（eager）**，有 `ev_new` handler，在 region 创建时（`map_page_region` → `ev_new` → `anon_contig_new`）一次调 `alloc_mem(N, ...)` 分配 N 页连续物理内存并全部映射。仅在 `mmap` 时指定 `MAP_CONTIG | MAP_PREALLOC` 才使用（单独 `MAP_CONTIG` 会被拒绝，返回 EINVAL）。且不能 fork、不能 resize、不能 pagefault。
>   - 不能 fork：`ev_reference` handler 返回 ENOMEM（打印 "cannot fork with physically contig memory"），但 `map_copy_region` 未检查该返回值，因此 fork 实际上仍会继续，只是 contig 区域在子进程中引用关系不正确——Minix3 的已知限制
>   - 不能 resize：`ev_resize` 返回 ENOMEM，contig 区域不可扩展
>   - 不能 pagefault：`ev_pagefault` 直接 panic，物理内存已在 `ev_new` 时预分配，不应再触发缺页
>
> 因此"连续 vs 非连续"的区分不在 `alloc_mem` 层，而在 region 层。`PAF_CONTIG` 是分配器层的未实现尝试，而 `MAP_CONTIG` 是 region 层已实现的功能。

**实现逻辑**

```c
// alloc_mem() - 对齐处理 + 调用 alloc_pages()
phys_clicks alloc_mem(phys_clicks clicks, u32_t memflags)
{
    phys_clicks mem = NO_MEM, align_clicks = 0;

    // 1. 处理对齐要求：预分配额外空间
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

    // 3. 调整对齐：释放前缀部分
    if(align_clicks) {
        phys_clicks o = mem % align_clicks;
        if(o > 0) {
            phys_clicks e = align_clicks - o;
            free_mem(mem, e);        // 释放前缀
            mem += e;                // 调整起始地址
        }
    }

    // 注意：PAF_CLEAR 在 alloc_pages() 中处理，不在这里
    return mem;
}

// alloc_pages() - 实际分配 + 清零处理
static phys_bytes alloc_pages(int pages, int memflags)
{
    // ... 位图扫描和分配 ...

    // 标记页为已用
    for(i = mem; i < mem + pages; i++) {
        UNSET_BIT(free_pages_bitmap, i);
    }

    // PAF_CLEAR 在这里处理
    if(memflags & PAF_CLEAR) {
        int s;
        if ((s= sys_memset(NONE, 0, CLICK_SIZE*mem,
            VM_PAGE_SIZE*pages)) != OK) 
            panic("alloc_mem: sys_memset failed: %d", s);
    }

    return mem;
}
```

**关键设计点**

1. **首次适应算法**: 从空闲列表中找到第一个足够大的块
2. **对齐处理**: 先分配额外空间，再释放前缀以达到对齐要求
3. **缓存回收**: 分配失败时尝试回收页缓存后重试
4. **清零处理**: 在 `alloc_pages()` 中通过 `sys_memset()` 实现
5. **物理地址**: 返回的是 click 编号，需要 `CLICK2ABS()` 转换为字节地址

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

**核心实现：alloc\_pages**

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

    // 7. 清零内存（如果请求 PAF_CLEAR）
    if(memflags & PAF_CLEAR) {
        int s;
        if ((s= sys_memset(NONE, 0, CLICK_SIZE*mem,
            VM_PAGE_SIZE*pages)) != OK) 
            panic("alloc_mem: sys_memset failed: %d", s);
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

| 策略         | 说明                           |
| ---------- | ---------------------------- |
| **页缓存**    | 单页分配优先从缓存获取，减少位图扫描           |
| **循环扫描**   | `lastscan` 记住上次位置，避免每次都从高位扫描 |
| **块跳过优化**  | `findbit` 中跳过整个空位图块，加速扫描     |
| **连续分配优先** | 尽量从上次位置附近分配，保持大块连续           |

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

| 操作   | 时间复杂度 | 说明                  |
| ---- | ----- | ------------------- |
| 单页分配 | O(1)  | 优先从缓存获取             |
| 多页分配 | O(n)  | n = 物理页数，最坏情况扫描整个位图 |
| 释放   | O(1)  | 标记位图位，可能加入缓存        |

### 2.2 物理内存释放

#### 2.2.1 free\_mem - 释放物理内存

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

**核心实现：free\_pages**

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

### 2.3 保留页队列（Reserved Queue）

> **核心目的**：预分配备用页，确保页表操作等关键路径在内存紧张时也能成功。

#### 2.3.1 为什么需要保留页队列？

**问题场景**：

```
fork() 系统调用
    ↓
需要为新进程分配页表
    ↓
调用 alloc_mem() 分配物理页
    ↓
如果系统内存不足，alloc_mem() 返回 NO_MEM
    ↓
fork() 失败 → 进程无法创建
```

**关键问题**：页表操作是系统关键路径，不能因为内存不足而失败。否则可能导致：

- 进程无法创建
- 缺页处理失败
- 系统死锁

**解决方案**：预先保留一部分物理页（spare pages），专门用于这些关键操作。

#### 2.3.2 数据结构

```c
// minix3/minix/servers/vm/alloc.c

#define MAXRESERVEDPAGES  300   // 每个队列最大页数
#define MAXRESERVEDQUEUES  15   // 最大队列数

static struct reserved_pages {
    struct reserved_pages *next;   // 链表连接
    int max_available;             // 队列容量
    int npages;                    // 每槽页数（通常为1）
    int mappedin;                  // 是否需要映射到内核地址空间
    int n_available;               // 当前可用槽数
    int allocflags;                // 分配标志
    struct reserved_pageslot {
        phys_bytes phys;           // 物理地址
        void *vir;                 // 虚拟地址（如果 mappedin）
    } slots[MAXRESERVEDPAGES];
    u32_t magic;                   // 魔数用于验证
} reservedqueues[MAXRESERVEDQUEUES];
```

#### 2.3.3 API

| 函数                                              | 说明                      |
| ----------------------------------------------- | ----------------------- |
| `reservedqueue_new(max, npages, mapped, flags)` | 创建保留队列                  |
| `reservedqueue_alloc(queue, &phys, &vir)`       | 从队列分配一槽                 |
| `reservedqueue_add(queue, vir, phys)`           | 向队列添加一槽                 |
| `reservedqueue_fill(queue)`                     | 自动填充队列（内部调用 alloc\_mem） |

#### 2.3.4 使用场景：spare pages

**初始化**（`pagetable.c:pt_init()`）：

```c
#define SPAREPAGES 200        // i386: 200 个备用页
#define STATIC_SPAREPAGES 190 // 静态预分配 190 个

// 创建保留队列
spare_pagequeue = reservedqueue_new(SPAREPAGES, 1, 1, 0);

// 添加静态预分配的页
for(s = 0; s < STATIC_SPAREPAGES; s++) {
    void *v = (void *) (sparepages_mem + s*VM_PAGE_SIZE);
    phys_bytes ph;
    sys_umap(SELF, VM_D, (vir_bytes)v, VM_PAGE_SIZE, &ph);
    reservedqueue_add(spare_pagequeue, v, ph);
}
```

**使用**（`pagetable.c:vm_getsparepage()`）：

```c
static void *vm_getsparepage(phys_bytes *phys)
{
    void *ptr;
    if(reservedqueue_alloc(spare_pagequeue, phys, &ptr) != OK) {
        return NULL;  // 保留页也用完了，系统处于极端状态
    }
    return ptr;
}
```

**补充机制**：

当保留页被消耗后，系统会在后台自动补充：

```c
// alloc.c: 检查并补充缺失的 spare pages
for(rq = first_reserved_inuse; rq && missing_spares > 0; rq = rq->next) {
    reservedqueue_fill(rq);  // 调用 alloc_mem 补充
}
```

#### 2.3.5 设计要点

| 要点       | 说明                         |
| -------- | -------------------------- |
| **预分配**  | 启动时分配，避免运行时竞争              |
| **映射**   | 页表操作需要虚拟地址，所以 `mappedin=1` |
| **容量**   | 200 页（约 800KB），足够应对突发需求    |
| **自动补充** | 后台任务补充消耗的备用页               |

> **Rust 实现状态**：当前 Rust 版本未实现保留页队列。这是待补充的重要功能，否则在内存紧张时页表操作可能失败。

### 2.4 内存统计与资源限制

#### 2.4.1 系统级内存统计

**total\_pages**: 系统总物理内存页数

- 在 VM 初始化时从内核获取
- 用于计算内存使用率和内存压力

**内存压力检测**:

- 当空闲内存低于阈值时，触发内存回收
- 可能涉及交换（swapping）或 OOM 处理

#### 2.4.2 进程级内存统计 (vm\_total / vm\_total\_max)

**vm\_total**: 当前进程已分配的虚拟内存总量

- 单位: bytes
- 更新时机: 分配/释放虚拟区域时

**vm\_total\_max**: 历史最大虚拟内存使用量（high water mark）

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

***

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

**核心操作**（与 Minix3 alloc.c API 精确对应）：

| 操作 | Minix3 C                   | 语义                    | trait              |
| -- | -------------------------- | --------------------- | ------------------ |
| 分配 | `alloc_mem(clicks, flags)` | 找到 k 个连续空闲页，标记为已用     | `PhysAllocator`    |
| 释放 | `free_mem(base, clicks)`   | 将 k 个连续页标记为空闲         | `PhysAllocator`    |
| 查询 | `memstats(&n, &p, &l)`     | 返回空闲块数、空闲页数、最大连续空闲块   | `PhysAllocatorStats` |
| 总量 | `total_pages` 全局变量        | 系统物理页总数（初始化时设定，不再变化）  | `PhysAllocator`    |

> **设计说明**：Minix3 的 `memstats()` 通过 3 个指针参数隐式返回结果，这是 C 语言常见的多返回值模式。Rust 中用命名结构体 `PhysMemStats` 替代，字段语义一目了然。分配/释放是核心路径（高频调用），查询是诊断路径（低频调用），因此拆分为两个 trait，职责清晰。

**约束条件**（通过 `flags` 参数传递）：

| 约束   | flag                     | 说明                  |
| ---- | ------------------------ | ------------------- |
| 对齐   | `ALIGN64K` / `ALIGN16K`  | 起始地址必须对齐到指定边界       |
| 地址范围 | `LOWER16MB` / `LOWER1MB` | 限制在低端内存（DMA 需求）     |
| 清零   | `CLEAR`                  | 分配后清零物理页            |
| 连续   | `CONTIG`                 | 要求物理连续（bitmap 默认行为） |

**抽象为 trait**：

```rust
pub trait PhysAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysBytes, AllocError>;
    fn free_mem(&mut self, base: PhysBytes, clicks: usize);
    fn total_count(&self) -> usize;
}

pub trait PhysAllocatorStats {
    fn memstats(&self) -> PhysMemStats;
}
```

### 3.1 为什么需要 Early Heap？

#### 3.1.1 问题建模需要元数据存储

要实现上述问题建模，需要数据结构来跟踪空闲区间：

| 分配器        | 元数据                                                               |
| ---------- | ----------------------------------------------------------------- |
| **Bitmap** | `bitmap: [u64]` — 1 bit = 1 页                                     |
| **Buddy**  | `free_list_heads: [u32]`, `page_next: [u32]`, `page_orders: [u8]` |
| **线段树**    | `nodes: [SegmentNode]` — 每节点记录区间信息                                |

**核心问题**：这些元数据存储在哪？

#### 3.1.2 Minix3 的方案：静态 BSS 数组

Minix3 使用静态 BSS 数组存储 bitmap：

```c
// minix3/minix/servers/vm/alloc.c
#define NUMBER_PHYSICAL_PAGES (int)(0x100000000ULL/VM_PAGE_SIZE)  // 4GB / 4KB = 1M 页
static bitchunk_t free_pages_bitmap[PAGE_BITMAP_CHUNKS];          // ~128KB
```

**为什么可行？**

| 维度        | Minix3 (32位)   | 说明         |
| --------- | -------------- | ---------- |
| 物理内存上限    | 4GB            | 32 位地址空间限制 |
| Bitmap 大小 | 128KB          | 固定，可接受     |
| BSS 段     | 启动时由 kernel 清零 | 无需动态分配     |

**Minix3 的技巧**：直接按**虚拟地址空间大小**（4GB）分配 bitmap，而非实际物理内存大小。因为 32 位系统物理内存不可能超过 4GB（虚拟地址空间限制），所以无论实际物理内存是 512MB 还是 4GB，bitmap 统一 128KB 都能覆盖。

#### 3.1.3 64 位系统的困境

| 维度        | 64 位系统        | 问题           |
| --------- | ------------- | ------------ |
| 物理内存      | 8GB \~ 256GB+ | 变化范围大，无法静态预留 |
| Bitmap 大小 | 256KB \~ 8MB+ | 随物理内存线性增长    |
| BSS 段     | 大小固定          | 无法适应不同硬件     |

**示例**：

| 物理内存   | Bitmap 大小 | Buddy 元数据 |
| ------ | --------- | --------- |
| 4 GB   | 128 KB    | 5 MB      |
| 64 GB  | 2 MB      | 80 MB     |
| 256 GB | 8 MB      | 320 MB    |

**结论**：无法预先确定静态数组大小，必须动态分配。

#### 3.1.4 循环依赖问题

动态分配需要堆，但：

```
物理分配器 → 管理所有物理页
    ↓
堆分配器 (GlobalAlloc) → 需要物理页
    ↓
物理分配器 → 需要元数据
    ↓
堆分配器 → 需要堆来分配元数据
    ↓
循环依赖！
```

> 💡 **为什么不能直接用 `Vec` 或 `Box`？** 见 [附录 A：为什么物理分配器不能使用堆](#附录-a为什么物理分配器不能使用堆)。

#### 3.1.5 解决方案：Early Heap

**核心思想**：在物理内存管理初始化**之前**，预留一块物理页，专门用于存储元数据。

```
┌─────────────────────────────────────────────────────────────┐
│                    初始化流程                                │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  阶段 1: 获取物理内存信息（从 kernel IPC）                   │
│  阶段 2: 计算元数据大小                                      │
│  阶段 3: 切出 early_heap 物理页  ← 预留，不被物理分配器管理   │
│  阶段 4: 初始化 early_heap (bump allocator)                 │
│  阶段 5: 初始化物理分配器（元数据在 early_heap）             │
│  阶段 6: 创建永久堆                                          │
│  阶段 7: 切换到永久堆                                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**关键设计**：

- Early heap 是 bump allocator，极简实现
- 物理分配器**不管理** early heap 的物理页
- 元数据类型为 `&'static mut [T]`，生命周期为 `'static`

**Early heap 存在的意义**：

1. **为什么需要它**：预留物理页后，完全可以手写指针运算构建元数据，但这需要 unsafe、手动对齐、容易出错
2. **它是什么**：一个封装好的 bump allocator，提供 `alloc_slice::<T>()` 方法，类型安全、自动对齐
3. **生命周期**：初始化完成后不再增长，但分配出去的内存（`&'static mut [T]`）由物理分配器持有，继续使用

| 方式 | 代码 | 难度 |
|------|------|------|
| **手写低级** | `let ptr = phys as *mut u64; ptr.write_bytes(0, n);` | unsafe、手动对齐、易错 |
| **Early heap** | `let slice: &'static mut [u64] = early_heap.alloc_slice(n);` | 类型安全、自动对齐 |

简言之：Early heap 不提供"新能力"，只是让低级编程更简单、更不容易出错。

***

## 4. Early Heap 设计

### 4.1 设计目标

| 目标        | 说明                       |
| --------- | ------------------------ |
| **极简实现**  | Bump allocator，只分配不释放    |
| **无循环依赖** | 物理分配器不管理 early heap      |
| **一次性使用** | 初始化完成后，early heap 不再增长   |
| **类型安全**  | 提供 `alloc_slice<T>()` 方法 |

### 4.2 EarlyHeap 实现

**本质**：预留物理页 + 简单的 bump 指针运算 + 类型安全封装。

```rust
pub struct EarlyHeap {
    start: *mut u8,
    current: *mut u8,
    end: *mut u8,
}

impl EarlyHeap {
    pub fn init(&mut self, start: *mut u8, size: usize) {
        self.start = start;
        self.current = start;
        self.end = unsafe { start.add(size) };
    }
    
    pub fn alloc_slice<T>(&mut self, count: usize) -> &'static mut [T] {
        let size = count * core::mem::size_of::<T>();
        let align = core::mem::align_of::<T>();
        
        let ptr = self.alloc_aligned(size, align);
        unsafe {
            core::slice::from_raw_parts_mut(ptr as *mut T, count)
        }
    }
    
    fn alloc_aligned(&mut self, size: usize, align: usize) -> *mut u8 {
        let aligned = (self.current as usize + align - 1) & !(align - 1);
        let new_current = aligned + size;
        
        if new_current > self.end as usize {
            panic!("early heap exhausted");
        }
        
        self.current = new_current as *mut u8;
        aligned as *mut u8
    }
}
```

### 4.3 使用示例

```rust
// 初始化
let mut early_heap = EarlyHeap::empty();
early_heap.init(phys_addr, size);

// 分配元数据
let bitmap: &'static mut [u64] = early_heap.alloc_slice(1024);
let heads: &'static mut [u32] = early_heap.alloc_slice(64);
```

### 4.4 Early Heap 物理页来源

> **TODO**: 本节为设计草案，待内核启动流程实现后完善。

#### 问题：物理内存大小不可预知

64 位系统物理内存大小动态变化（16GB ~ 256GB+），元数据大小也随之变化：
- 16GB 物理内存，Buddy 元数据约 19MB
- 256GB 物理内存，Buddy 元数据约 305MB

内核无法预知最终需要的元数据大小，因此无法一次性预留足够的 early heap。

#### 解决方案：两阶段初始化 + 搬迁

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    VM 初始化流程                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  阶段 1: 内核加载 VM                                                         │
│  ├── 分配 VM 代码/数据/BSS/栈                                               │
│  └── 额外预留 early_heap（2MB，足够管理 ~512MB 物理内存）                     │
│                                                                             │
│  阶段 2: VM 启动                                                             │
│  ├── early_heap 已被内核映射到 VM 地址空间                                   │
│  └── early_heap 不在 memmap 中（不属于物理分配器管理范围）                    │
│                                                                             │
│  阶段 3: 初始化物理分配器（临时）                                             │
│  ├── 从 memmap 获取可用内存信息                                             │
│  ├── 使用 early_heap 分配临时元数据                                         │
│  └── 物理分配器管理 memmap 中的内存                                         │
│                                                                             │
│  阶段 4: 搬迁元数据                                                          │
│  ├── 计算最终元数据大小（PhysAllocType::metadata_size）                      │
│  ├── 从物理分配器分配新空间                                                  │
│  ├── 复制元数据到新空间                                                      │
│  ├── 更新分配器内部指针                                                      │
│  └── early_heap 物理页归还给物理分配器                                       │
│                                                                             │
│  阶段 5: VM 完全初始化，管理所有物理内存                                       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Minix3 对比

| 方面 | Minix3 | rs 版本 |
|------|--------|---------|
| **元数据存储** | 静态 BSS（固定 4GB 上限） | 动态分配 + 搬迁 |
| **物理页来源** | 内核从 memmap 末尾分配，不在 memmap 中 | 同样由内核预留 |
| **是否搬迁** | 否（BSS 生命周期 = 进程生命周期） | 是（适应动态内存大小） |

#### 搬迁实现要点

搬迁需要更新元数据指针，因此分配器内部使用**裸指针**而非 `&'static mut [T]`：

```rust
pub struct BuddyAllocator {
    free_lists: *mut u32,     // 裸指针，支持搬迁时更新
    page_next: *mut u32,
    page_orders: *mut u8,
    // ...
}

impl BuddyAllocator {
    pub fn relocate(&mut self, new_base: *mut u8, new_size: usize) -> Result<()> {
        // 复制数据到新位置
        // 更新内部指针
        // 返回旧内存给物理分配器
    }
}
```

**搬迁安全性**：VM 初始化期间持有 Big Kernel Lock，相当于 stop the world，搬迁风险可控。

***

## 5. 物理分配器实现

### 5.1 Trait 定义

trait 定义见 §3 核心操作。此处补充 `PhysMemStats` 和 `PageAllocFlags` 的完整定义：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysMemStats {
    pub free_nodes: usize,    // memstats.nodes: number of free blocks
    pub free_pages: usize,    // memstats.pages: total free pages
    pub largest_free: usize,  // memstats.largest: largest contiguous free block
}

bitflags::bitflags! {
    pub struct PageAllocFlags: u32 {
        const CLEAR = 0x01;       // 清零物理内存
        const CONTIG = 0x02;      // 要求物理连续
        const ALIGN64K = 0x04;    // 64KB 对齐
        const LOWER16MB = 0x08;   // 低端内存
        const LOWER1MB = 0x10;    // 1MB 以下
        const ALIGN16K = 0x40;    // 16KB 对齐
    }
}
```

### 5.2 三种实现方案

如 §3.0 所述，物理内存分配的本质是**连续区间分配问题**——在空闲区间集合中找到满足约束的连续区间，标记为已用；释放时合并相邻空闲区间。对这个问题的求解，从朴素到精巧，有三种思路：

| | 本质 | 与问题域的关系 |
| -- | ---- | ---------- |
| **Bitmap** | 朴素解法 | 对问题域无任何假设，暴力扫描 |
| **线段树** | 标准解法 | 对问题域无约束，理论最优 O(log n) |
| **Buddy** | 特化解法 | 对问题域施加约束（块大小必须 2^n），换取更好的性质 |

Buddy 的约束——所有块大小必须是 2^n 且 2^n 对齐——让问题大幅简化：

1. **合并规则极简**：线段树需维护 `left_free`/`right_free`/`max_free` 来判断任意区间能否合并；buddy 只需检查 `page ^ (1 << order)` 这一个地址的 buddy 是否空闲
2. **空间复杂度降低**：线段树每节点 4 个 usize（32 字节），buddy 每页只需 1 字节（order + flag）
3. **天然抗碎片**：2^n 约束 + buddy 合并保证外部碎片不可能累积——任何释放的块最终都能合并回大块

代价是**内部碎片**：分配 3 页实际占用 4 页。这是特化的典型 trade-off：用精度换性质。严格来说，buddy 不是线段树的"特化"——它们的数据结构完全不同。更准确的说法是：**buddy 是对问题域施加约束后得到的特化算法**。约束让问题变简单了，所以不需要线段树那么重的数据结构，用轻得多的 SoA 数组就够了。线段树是**无约束区间分配**的标准解；buddy 是**2^n 约束区间分配**的最优解。这正是 buddy 成为内核广泛采用的物理内存算法的原因：**用精度换性质，用约束换简洁**。

默认使用 **BuddyAllocator**（启用 `buddy_alloc` feature 时），回退到 **BitmapAllocator**（与 Minix3 行为一致），线段树仅作教学/实验用途。

### 5.3 BitmapAllocator

> 源码：`os/servers/vm/src/phys_mem/bitmap_alloc.rs`

**核心设计**：使用位图管理物理页状态，与 Minix3 `alloc.c` 一致。每个 bit 代表一页，1 = 空闲，0 = 已用。

```rust
pub struct BitmapAllocator {
    bitmap: &'static mut [u64],    // 1 bit = 1 页，1 = 空闲
    total_pages: usize,
    free_pages: usize,
}
```

**初始化**：

```rust
pub fn init(early_heap: &mut EarlyHeap, regions: &[BootMemRegion]) -> Self {
    // 从 early heap 分配 bitmap，然后将 boot memory regions 标记为空闲
}
```

**核心方法**：

```rust
fn alloc_pages(&mut self, pages: usize) -> Option<usize> {
    // 首次适应，从高地址向低地址扫描（与 Minix3 一致）
    // 内部调用 find_bit + mark_allocated
}

fn find_bit(&self, low: usize, start_scan: usize, pages: usize) -> Option<usize> {
    // 反向扫描 bitmap，找连续 pages 个空闲位
    // 利用 chunk 全零快速跳过已分配区域
}

fn free_pages_internal(&mut self, start_page: usize, num_pages: usize) {
    // 逐位设置 bitmap = 1，更新 free_pages 计数
}

fn mark_allocated(&mut self, start_page: usize, num_pages: usize) {
    // 逐位清除 bitmap = 0，更新 free_pages 计数
}

pub fn page_is_free(&self, page: usize) -> bool {
    // 查询单页状态，对应 Minix3 GET_BIT(free_pages_bitmap, i)
}

fn memstats_internal(&self) -> (usize, usize, usize) {
    // 扫描 bitmap 统计：(空闲块数, 空闲页数, 最大连续空闲块)
}
```

**PhysAllocator / PhysAllocatorStats 实现**：

```rust
impl PhysAllocator for BitmapAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysBytes, AllocError> {
        // 页缓存：单页分配且无地址限制时，优先从 page_cache 弹出（LIFO）
        // 对齐：ALIGN64K/ALIGN16K → 多分配 align_clicks 页，释放前缀未对齐部分
        // 低内存：LOWER16MB/LOWER1MB → 限制搜索范围 max_page（此时不用 page_cache）
        // 重试：分配失败时调用 cache_freepages() 回收文件缓存页，再重试
        // 清零：CLEAR → TODO（VM 仅拥有物理内存描述数据，无物理内存直接访问权，需内核 IPC sys_memset）
        // 连续：CONTIG → no-op（bitmap 总是连续分配，与 Minix3 一致）
    }
    fn free_mem(&mut self, base: PhysBytes, clicks: usize) {
        // 将 PhysBytes 转为页号，调用 free_pages_internal
        // free_pages_internal 同时将每页压入 page_cache（LIFO）
    }
    fn total_count(&self) -> usize;
}

impl PhysAllocatorStats for BitmapAllocator {
    fn memstats(&self) -> PhysMemStats {
        // 内部调用 memstats_internal()
    }
}
```

**与 Minix3 的关键差异**：

| 项目 | Minix3 C | Rust |
| -- | -------- | ---- |
| 存储位置 | 静态 BSS 数组 `free_pages_bitmap[]` | Early heap 分配的 `&'static mut [u64]` |
| 大小 | 固定 128KB（4GB 地址空间） | 按需计算 `compute_memory_bounds()` |
| chunk 类型 | `bitchunk_t` (u32) | `u64`（64-bit 更高效） |
| 初始化 | `mem_init()` 写 BSS | `init()` 从 early heap 分配 |
| page cache | `free_page_cache[]` + `cache_freepages()` | `page_cache: &'static mut [usize]` + `cache_freepages()` placeholder |
| PAF_CLEAR | `sys_memset()` 清零 | TODO（需内核 IPC） |

**辅助方法**：

```rust
pub fn total_memory(&self) -> usize       // total_pages * CLICK_SIZE
pub fn free_memory(&self) -> usize        // free_pages * CLICK_SIZE
pub fn is_under_pressure(&self) -> bool   // free_pages * 10 < total_pages
```

### 5.4 BuddyAllocator (SoA 结构)

> 源码：`os/servers/vm/src/phys_mem/buddy_alloc.rs`

**Buddy 系统简介**：Buddy 分配器是经典物理内存管理算法，Linux 内核亦采用此方案。核心思想是将内存按 2^n 划分：每个块大小为 2^order 页，两个大小相同、地址相邻的块互为"buddy"。分配时从匹配的 order 链表取块，不足则向上分裂大块（一分为二）；释放时检查 buddy 是否空闲，若空闲则自动合并，如此递归。优势是 O(log n) 分配/释放且天然抗外部碎片，代价是内部碎片（向上取整到 2^n）。

**SoA 设计选择**：传统 buddy 实现使用指针链表（如 Linux 的 `struct free_area`），但指针在 Rust 中引入所有权和生命周期问题，且对 early heap 分配不友好。本实现采用 SoA（Structure of Arrays）结构——三个平行的数组替代指针链表，更 cache 友好，也更适合从 early heap 一次性分配。

```rust
const FLAG_ALLOCATED: u8 = 0x80;
const ORDER_MASK: u8 = 0x7F;
const ORDER_INVALID: u8 = 0xFF;

pub struct BuddyAllocator {
    free_list_heads: &'static mut [u32],  // 每个 order 的空闲链表头
    page_next: &'static mut [u32],        // 每个页的 next 指针
    page_orders: &'static mut [u8],       // 每个页的 order + 分配标志
    total_pages: usize,
    max_order: usize,
    free_pages: usize,
}
```

**初始化**：

```rust
pub fn init(early_heap: &mut EarlyHeap, regions: &[BootMemRegion]) -> Self {
    // 从 early heap 分配三个数组
    // 将 boot memory regions 按 2^n 对齐切分后加入空闲链表
}
```

**核心方法**：

```rust
fn alloc_block(&mut self, order: usize) -> Option<usize> {
    // 从指定 order 的空闲链表分配；不足则向上分裂大块
}

fn add_free_region(&mut self, start: usize, count: usize) {
    // 按 2^n 对齐切分后逐块调用 free_block_internal
}

fn free_block_internal(&mut self, page: usize, order: usize) {
    // 释放单块，加入空闲链表，然后调用 try_merge 尝试合并
}

fn try_merge(&mut self, page: usize, mut order: usize) {
    // 释放时严格检查五条件后合并 buddy：
    //   1. buddy 在有效范围内
    //   2. buddy 已进入 buddy 系统（非 ORDER_INVALID）
    //   3. buddy 未被分配（无 FLAG_ALLOCATED）
    //   4. buddy 的 order 匹配
    //   5. buddy 在空闲链表中（remove_from_free_list 成功）
}

fn buddy_of(page: usize, order: usize) -> usize {
    // 计算 buddy 地址：page ^ (1 << order)
}

fn push_free(&mut self, order: usize, page: usize) {
    // 空闲链表头插法
}

fn pop_free(&mut self, order: usize) -> Option<usize> {
    // 空闲链表弹出头部
}

fn remove_from_free_list(&mut self, order: usize, target: usize) -> bool {
    // 从链表中移除指定节点（用于合并时摘除 buddy）
}

fn order_for_pages(pages: usize) -> usize {
    // 计算所需最小 order（向上取整到 2^n）
}

pub fn largest_free(&self) -> usize {
    // 从高 order 向低扫描，返回最大空闲块大小
}
```

**PhysAllocator / PhysAllocatorStats 实现**：

```rust
impl PhysAllocator for BuddyAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysBytes, AllocError> {
        // 对齐：ALIGN64K/ALIGN16K → 多分配后释放前缀未对齐部分
        // 低内存：LOWER16MB/LOWER1MB → 分配后检查是否超出 max_page，超出则释放返回 Err
        // 清零：CLEAR → TODO（需内核 IPC sys_memset）
        // 连续：CONTIG → no-op（buddy 总是连续分配）
        // 注意：buddy 向上取整到 2^n 页，可能浪费内存
    }
    fn free_mem(&mut self, base: PhysBytes, clicks: usize) {
        // 将 PhysBytes 转为页号，调用 add_free_region
    }
    fn total_count(&self) -> usize;
}

impl PhysAllocatorStats for BuddyAllocator {
    fn memstats(&self) -> PhysMemStats {
        // free_nodes = 0（buddy 无此概念），largest_free 调用 self.largest_free()
    }
}
```

> **注意**：Buddy 分配器无法精确分配任意大小，总是向上取整到 2^n 页。`free_nodes` 填 0，因为 buddy 系统的"空闲块"概念与 bitmap 不同。

**辅助方法**：

```rust
pub fn total_memory(&self) -> usize       // total_pages * CLICK_SIZE
pub fn free_memory(&self) -> usize        // free_pages * CLICK_SIZE
pub fn is_under_pressure(&self) -> bool   // free_pages * 10 < total_pages
```

### 5.5 SegmentTreeAllocator (实验性)

> 源码：`os/servers/vm/src/phys_mem/segment_tree_alloc.rs`

**线段树简介**：区间分配问题的标准数据结构。将 n 个页组织为完全二叉树，每个节点维护其区间的聚合信息（最大连续空闲、左端连续空闲、右端连续空闲），分配时从根向叶 O(log n) 定位，释放时自底向上 O(log n) 更新，实现精确的任意大小分配。本实现采用静态线段树（一次性分配全部节点），4GB 物理内存需约 64MB 元数据——是 bitmap 的 500 倍。若改用动态开点线段树（按需创建节点），初始仅一个区间时开销极小，但 early heap 要求按最坏情况预分配，动态方案无法适用。

> **警告**：本分配器仅用于教学示意，不推荐生产使用，需启用 `segment_tree_alloc` feature。

```rust
#[cfg(feature = "segment_tree_alloc")]
#[derive(Debug, Clone, Copy)]
struct SegmentNode {
    max_free: usize,    // 区间内最大连续空闲
    left_free: usize,   // 左端连续空闲
    right_free: usize,  // 右端连续空闲
    len: usize,         // 区间长度
}

#[cfg(feature = "segment_tree_alloc")]
pub struct SegmentTreeAllocator {
    n: usize,
    offset: usize,
    tree: &'static mut [SegmentNode],
    total_pages: usize,
    free_pages: usize,
}
```

**初始化**：

```rust
pub fn init(early_heap: &mut EarlyHeap, regions: &[BootMemRegion]) -> Self {
    // 从 early heap 分配 2 * next_power_of_two(n) 个节点
    // 叶节点初始化后自底向上 merge()
}
```

**核心方法**：

```rust
fn find_first_fit(&self, k: usize) -> Option<usize> {
    // 从根节点向下查找 k 连续空闲页
}

fn find_in_subtree(&self, node: usize, k: usize) -> Option<usize> {
    // 递归搜索：左子树 → 跨越中间 → 右子树
}

fn set_range(&mut self, start: usize, count: usize, free: bool) {
    // 批量设置叶节点，然后 pull_up 更新祖先
}

fn pull_up(&mut self, mut idx: usize) {
    // 自底向上重新 merge() 祖先节点
}

fn merge(left: SegmentNode, right: SegmentNode) -> SegmentNode {
    // 合并两个子节点信息（关键操作）
    // 计算跨越中间的连续空闲：left.right_free + right.left_free
}

pub fn page_is_free(&self, page: usize) -> bool {
    // 查询单页状态
}

pub fn largest_free(&self) -> usize {
    // 直接读 tree[1].max_free，O(1)
}
```

**PhysAllocator / PhysAllocatorStats 实现**：

```rust
impl PhysAllocator for SegmentTreeAllocator {
    fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysBytes, AllocError> {
        // 对齐：ALIGN64K/ALIGN16K → 多分配后 set_range 释放前缀未对齐部分
        // 低内存：LOWER16MB/LOWER1MB → 分配后检查是否超出 max_page
        // 清零：CLEAR → TODO（需内核 IPC sys_memset）
        // 连续：CONTIG → no-op（线段树总是连续分配）
    }
    fn free_mem(&mut self, base: PhysBytes, clicks: usize) {
        // 将 PhysBytes 转为页号，调用 set_range(..., true)
    }
    fn total_count(&self) -> usize;
}

impl PhysAllocatorStats for SegmentTreeAllocator {
    fn memstats(&self) -> PhysMemStats {
        // free_nodes = 0, largest_free = tree[1].max_free
    }
}
```

**特点**：

| 维度 | 说明 |
| -- | -- |
| 时间复杂度 | O(log n) 分配/释放 |
| 精确分配 | ✅ 任意大小（优于 buddy） |
| 内存开销 | ~8x（每节点 4 个 usize，树大小 2n） |
| 推荐度 | ❌ 不推荐生产使用 |

### 5.6 三种实现对比

| 维度 | Bitmap | Buddy | 线段树 |
| -- | ------ | ----- | ------ |
| **数据结构** | 位图（1 bit/页） | 阶数分组 + 空闲链表 | 完全二叉树 |
| **时间复杂度** | O(n) 扫描 | O(log n) | O(log n) |
| **精确分配** | ✅ 任意大小 | ❌ 向上取整到 2^n | ✅ 任意大小 |
| **抗碎片** | ❌ 无自动合并 | ✅ buddy 自动合并 | ❌ 无自动合并 |
| **内存开销** | 1x | ~2x | ~8x |
| **实现复杂度** | ⭐ 低 | ⭐⭐ 中 | ⭐⭐⭐⭐ 高 |
| **Minix3 对应** | ✅ 原始实现 | ❌ 新增 | ❌ 新增 |
| **推荐场景** | 小内存系统 | 通用场景 | 教学示意 |

### 5.7 元数据大小计算

根据内核传递的物理内存大小（`total_pages`），各分配器需计算元数据所需空间，以便从 early heap 中一次性分配。`PhysAllocType` 枚举提供了 `metadata_size` / `metadata_size_exact` 方法，在初始化 early heap 前即可预知所需空间：

| 分配器        | 计算策略      | 原因                 |
| ---------- | --------- | ------------------ |
| **Bitmap** | 精确计算      | 1 bit = 1 页 + page_cache |
| **Buddy**  | 精确计算 | 每页固定 5 字节（page_next × 4 + page_orders × 1）+ 链表头 |
| **线段树** | 精确计算 | 静态线段树已按最坏情况展开，大小完全确定 |

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysAllocType {
    Bitmap,
    Buddy,
    SegmentTree,
}

impl PhysAllocType {
    pub fn metadata_size(&self, total_pages: usize) -> usize {
        let exact = self.metadata_size_exact(total_pages);
        (exact + CLICK_SIZE - 1) & !(CLICK_SIZE - 1)  // page-align
    }

    pub fn metadata_size_exact(&self, total_pages: usize) -> usize {
        match self {
            PhysAllocType::Bitmap => {
                let bitmap_chunks = (total_pages + 63) / 64;
                bitmap_chunks * size_of::<u64>()       // 1 bit per page
                    + 10000 * size_of::<usize>()       // page_cache
            }
            PhysAllocType::Buddy => {
                let max_order = if total_pages == 0 { 0 }
                    else { total_pages.next_power_of_two().trailing_zeros() as usize };
                (max_order + 1) * size_of::<u32>()   // free_list_heads
                + total_pages * size_of::<u32>()      // page_next
                + total_pages * size_of::<u8>()       // page_orders
            }
            PhysAllocType::SegmentTree => {
                let offset = if total_pages == 0 { 1 }
                    else { total_pages.next_power_of_two() };
                let tree_size = if total_pages > 0 { 2 * offset } else { 2 };
                tree_size * size_of::<SegmentNode>()
            }
        }
    }
}
```

**元数据大小对比**（64 位系统，含 page_cache）：

| 物理内存 | total_pages | Bitmap | Buddy | 线段树 | Buddy 占比 |
| -- | ----------- | ------ | ----- | ------ | ---------- |
| 4 GB | 1M | ~206 KB | ~4.8 MB | ~64 MB | 0.12% |
| 16 GB | 4M | ~590 KB | ~19 MB | ~256 MB | 0.12% |
| 64 GB | 16M | ~2.1 MB | ~76 MB | ~1 GB | 0.12% |
| 256 GB | 64M | ~8.1 MB | ~305 MB | ~4 GB | 0.12% |

**结论**：

- Bitmap：每页 1 bit + 固定 80 KB page_cache，开销最小
- Buddy：每页 5 字节（page_next × 4 + page_orders × 1）+ 链表头，占比约 0.12%
- 线段树：每页约 64 字节（SegmentNode × 2 节点），开销过大，仅教学示意

***

## 6. 辅助类型

### 6.1 PhysBytes — 物理地址 newtype

对应 Minix3 的 `phys_bytes` 类型（`typedef u32`），Rust 版本扩展为 `u64` 以支持 64 位地址空间：

```rust
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PhysBytes(u64);

impl PhysBytes {
    pub const fn new(addr: u64) -> Self { PhysBytes(addr) }
    pub const fn as_u64(&self) -> u64 { self.0 }
    pub fn from_page_index(idx: usize) -> Self {
        PhysBytes((idx as u64) * PAGE_SIZE as u64)
    }
    pub fn page_index(&self) -> usize {
        (self.0 as usize) / PAGE_SIZE
    }
}
```

**与 Minix3 C 的对比**：

| Minix3 C                   | Rust                             |
| -------------------------- | -------------------------------- |
| `phys_bytes` (typedef u32) | `PhysBytes(u64)` newtype          |
| `NO_MEM` sentinel (0)      | `Result<PhysBytes, AllocError>`    |
| `CLICK2ABS(v)` 宏           | `PhysBytes::from_page_index(idx)` |
| `ABS2CLICK(a)` 宏           | `addr.page_index()`              |

### 6.2 PageAllocFlags — 分配标志

```rust
bitflags::bitflags! {
    pub struct PageAllocFlags: u32 {
        const CLEAR = 0x01;       // 清零物理内存
        const CONTIG = 0x02;      // 要求物理连续
        const ALIGN64K = 0x04;    // 64KB 对齐
        const LOWER16MB = 0x08;   // 低端内存
        const LOWER1MB = 0x10;    // 1MB 以下
        const ALIGN16K = 0x40;    // 16KB 对齐
    }
}
```

### 6.3 AllocError — 错误类型

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocError {
    OutOfMemory,        // 总内存不足
    LowMemoryExhausted, // 低端内存耗尽
}
```

***

## 7. 与 Minix3 的对比

| 维度            | Minix3 (32位)  | Rust (64位)                      |
| ------------- | ------------- | ------------------------------- |
| **元数据存储**     | 静态 BSS 数组     | Early heap + `&'static mut [T]` |
| **Bitmap 大小** | 固定 128KB（4GB） | 按需计算                            |
| **堆就绪时机**     | `pt_init()` 后 | `mem_init()` 前就需要 early heap    |
| **分配器选择**     | 仅 Bitmap      | Bitmap / Buddy / 线段树            |
| **Buddy 安全性** | N/A           | 状态标志 + 严格检查                     |

**关键差异**：

1. **64 位系统无法使用静态 BSS**
   - 物理内存无上限，bitmap 大小不确定
   - 必须动态分配 → 需要 early heap
2. **初始化顺序不同**
   - Minix3：`mem_init()` 不需要堆
   - Rust：`mem_init()` 前必须初始化 early heap
3. **Buddy 安全性增强**
   - 使用 `FLAG_ALLOCATED` 防止误判已分配页
   - 使用 `ORDER_INVALID` 防止误判未初始化页

***

## 8. 文件结构

```
phys_mem/
├── mod.rs                    # 模块入口
├── early_heap.rs             # Bump allocator
├── alloc_trait.rs            # PhysAllocator + PhysAllocatorStats traits, PhysMemStats
├── types.rs                  # PhysBytes, PageAllocFlags, AllocError
├── bitmap_alloc.rs           # Bitmap 分配器
├── buddy_alloc.rs            # Buddy 分配器 (SoA)
├── segment_tree_alloc.rs     # 线段树分配器 (实验性)
├── stats.rs                  # 运维统计 (alloc/free 计数)
└── allocator_tests.rs        # 统一测试套件
```

***

## 9. 测试与验证

> 测试代码位于 `os/servers/vm/src/phys_mem/allocator_tests.rs`。

统一测试套件覆盖三种分配器：

| 模块              | 测试内容              |
| --------------- | ----------------- |
| `basic`         | 基本分配/释放、零页分配、单页分配 |
| `exhaustion`    | 内存耗尽、全部分配后全部释放    |
| `free_realloc`  | 释放后重新分配、合并验证      |
| `flags`         | 对齐、地址限制           |
| `fragmentation` | 碎片场景              |
| `stress`        | 随机分配/释放压力测试       |
| `buddy_safety`  | Buddy 状态标志、合并安全   |

***

## 10. 参见

- [00-vm-overview.md §2.3](00-vm-overview.md) - 启动阶段物理内存初始化链路
- [heap-bootstrap.md](heap-bootstrap.md) - Early Heap 详细设计
- [03-acl.md](03-acl.md) - ACL 权限控制
- [08-slab-allocator.md](08-slab-allocator.md) - VM 内部使用 Slab
- [17-vm-fork.md](17-vm-fork.md) - fork 时的内存分配
- [19-vm-map.md](19-vm-map.md) - VM\_MAP 服务中的内存分配

***

*分类: VM库 | 接口性质: 其他服务通过 IPC 调用*

---

## 附录 A：为什么物理分配器不能使用堆

### A.1 Rust 的 GlobalAlloc 机制

Rust 的 `alloc` crate 提供堆分配能力（`Vec`, `Box`, `String` 等）。使用前需要：

1. **实现 `GlobalAlloc` trait**：
   ```rust
   unsafe trait GlobalAlloc {
       unsafe fn alloc(&self, layout: Layout) -> *mut u8;
       unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout);
   }
   ```

2. **注册为全局分配器**：
   ```rust
   #[global_allocator]
   static ALLOC: MyAllocator = MyAllocator;
   ```

3. **填充 stub 后即可使用**：
   ```rust
   let v: Vec<u64> = Vec::new();  // 调用 GlobalAlloc::alloc
   let b: Box<[u8]> = Box::new([0; 1024]);
   ```

### A.2 循环依赖问题

物理分配器**不能**使用 `GlobalAlloc`，因为：

```
┌─────────────────────────────────────────────────────────────────────┐
│                    循环依赖链                                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  GlobalAlloc::alloc()                                               │
│      ↓ 需要物理页                                                    │
│  PhysAllocator::alloc_mem()                                       │
│      ↓ 需要元数据存储                                                │
│  元数据: Vec<u64> / Box<[T]>                                        │
│      ↓ 调用 GlobalAlloc::alloc()                                    │
│  GlobalAlloc::alloc()  ←─────────────── 循环！                      │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

**关键点**：
- `GlobalAlloc` 需要物理页才能工作
- 物理分配器管理物理页
- 物理分配器的元数据如果用 `Vec`，就需要 `GlobalAlloc`
- 但 `GlobalAlloc` 还没初始化（没有物理页可用）

### A.3 解决方案

**物理分配器元数据必须存储在"自己不管理"的内存中**：

| 方案 | 存储 | 优点 | 缺点 |
|------|------|------|------|
| **Early Heap** | 预留物理页 | 灵活，适配任意内存大小 | 需要额外初始化步骤 |
| **静态 BSS** | 编译时固定 | 简单 | 64 位系统不可行 |
| **保留页池** | 启动时预留 | Minix3 使用 | 需要预估大小 |

**Rust 实现**：使用 `&'static mut [T]` 而非 `Vec<T>`：

```rust
// ❌ 错误：Vec 需要 GlobalAlloc
pub struct BitmapAllocator {
    bitmap: Vec<u64>,  // 循环依赖！
}

// ✅ 正确：静态切片，从 early heap 分配
pub struct BitmapAllocator {
    bitmap: &'static mut [u64],  // 不依赖 GlobalAlloc
}
```

### A.4 详细设计

完整方案见 [heap-bootstrap.md](heap-bootstrap.md)。
