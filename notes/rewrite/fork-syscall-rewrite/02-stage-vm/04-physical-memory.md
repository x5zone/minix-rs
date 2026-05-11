# 04-physical-memory: 物理内存分配

> **分类**: VM库\
> **源码**: [alloc.c](minix3/minix/servers/vm/alloc.c)\
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
│   PM (fork) ──► VM (内部分配) ──► 物理页分配 ──► 进程     │
│        ▲                                            │       │
│        │                                            ▼       │
│   VM (内部释放) ◄──────────────────────────── 进程退出     │
│                                                             │
│   其他服务 ──► VM (内部分配) ──► 共享内存、DMA 等         │
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
// [alloc.c:242](minix3/minix/servers/vm/alloc.c#L242)
phys_clicks alloc_mem(phys_clicks clicks, u32_t memflags);

/* 释放物理内存 */
// [alloc.c:289](minix3/minix/servers/vm/alloc.c#L289)
void free_mem(phys_clicks base, phys_clicks clicks);

/* 查询内存统计 */
// [alloc.c:348](minix3/minix/servers/vm/alloc.c#L348)
void memstats(int *nodes, int *pages, int *largest);
```

> **类型说明**：
>
> - `phys_clicks`: 物理 click 编号（1 click = 4KB），用于分配/释放
> - `phys_bytes`: 物理字节地址，用于实际内存访问
> - 转换：`CLICK2ABS(clicks)` → 字节地址，`ABS2CLICK(bytes)` → click 编号

**分配标志 (flags)**

| 标志 | 值 | 含义 |
|------|-----|------|
| `PAF_CLEAR` | 0x01 | 清零物理内存 |
| `PAF_CONTIG` | 0x02 | 要求物理连续（定义但未使用） |
| `PAF_ALIGN64K` | 0x04 | 64KB 对齐 |
| `PAF_LOWER16MB` | 0x08 | 低端内存（<16MB，用于 DMA） |
| `PAF_LOWER1MB` | 0x10 | 1MB 以下内存 |
| `PAF_ALIGN16K` | 0x40 | 16KB 对齐 |

> 完整 `#define` 定义见 [§2.1.1](#221-alloc_mem---分配物理内存)。

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
- **保留页队列**：预分配备用页（spare pages），打破 VM 页表分配的循环依赖（`vm_allocpages → vm_mappages → pt_writemap → vm_allocpages` 递归时从备用池取页）
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

***

## 2. C 源码分析

### 2.0 核心数据结构

Minix3 物理内存管理使用三个核心数据结构：

```c
// [alloc.c:33-41](minix3/minix/servers/vm/alloc.c#L33-L41)

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

> **关于 `PAF_CONTIG`**：`PAF_CONTIG` 在 `alloc_mem` 层是 no-op——`alloc_mem` 总是分配物理连续内存。Minix3 的"连续 vs 非连续"区分实际上在 region 层实现（`mem_type_anon` vs `mem_type_anon_contig`），详见 [附录 A：PAF_CONTIG 与 region 层的连续语义](#附录-apaf_contig-与-region-层的连续语义)。

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
```

`alloc_pages()` 的完整实现见 [§2.1.2](#2212-分配策略)。

**关键设计点**

1. **反向首次适应算法**: 从高地址向低地址扫描，找到第一个足够大的连续空闲块
2. **对齐处理**: 先分配额外空间，再释放前缀以达到对齐要求
3. **缓存回收**: 分配失败时尝试回收页缓存后重试
4. **清零处理**: 在 `alloc_pages()` 中通过 `sys_memset()` 实现
    - Minix3 中 VM 只拥有物理页的描述数据（位图），不拥有物理页本身。清零操作必须通过 `sys_memset()` 委托 kernel 完成（VM → kernel IPC）。Rust 版本有 direct map 后，通过 `vm_phys_to_virt()` 可直接访问，不需此委托
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
空闲页位图示例（从高地址向低地址扫描）：
页号:  0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15
状态:  U U F F F U U F F F F  F  U  U  F  F
       ↑已用  ↑空闲块(3页)  ↑空闲块(5页)  ↑空闲块(2页)

分配 2 页：从页 15 向下扫描，首先遇到页 14-15（空闲 2 页），分配页 14-15
分配 4 页：页 14-15 不够，跳过页 13-12（已用），遇到页 7-11（空闲 5 页），分配页 7-10
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
| **块跳过优化**  | `findbit` 中跳过整个已用位图块，加速扫描     |
| **循环扫描**   | `lastscan` 记住上次分配位置，避免重复扫描已扫描区域（性能优化） |

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

> **核心目的**：打破 VM 页表分配的循环依赖——`vm_allocpages → vm_mappages → pt_writemap → vm_allocpages` 递归时，从预分配的备用池直接取页。

#### 2.3.1 为什么需要保留页队列？

**Minix3 源码注释**（[pagetable.c:55-57](minix3/minix/servers/vm/pagetable.c#L55-L57)）：

```c
/* Spare memory, ready to go after initialization, to avoid a
 * circular dependency on allocating memory and writing it into VM's
 * page table.
 */
```

**Minix3 源码注释**（[pagetable.c:1126-1130](minix3/minix/servers/vm/pagetable.c#L1126-L1130)）：

```c
/* Spare pages are used to allocate memory before VM has its own page
 * table that things (i.e. arbitrary physical memory) can be mapped into.
 * We get it by pre-allocating it in our bss (allocated and mapped in by
 * the kernel) in static_sparepages.
 */
```

**循环依赖**：VM 分配页表页时存在递归。

```
vm_allocpages() 分配物理页
    ↓
vm_mappages() 将物理页映射到 VM 地址空间
    ↓
pt_writemap() 写页表 → 可能需要分配新的页表页
    ↓
vm_allocpages() ← 递归！
```

**为什么递归是个问题？** Minix3 的 VM 运行在 x86-32 用户空间，32 位地址空间只有 4GB，VM 自身还需要容纳代码段、数据段、BSS、栈等，剩余可用于映射物理内存的虚拟地址空间有限。虽然理论上可以划出一段地址空间做 direct map（Linux x86-32 用 ~896MB 线性映射区覆盖 ZONE_NORMAL，这是**虚拟地址空间的预留**，不消耗实际物理内存），但 Minix3 选择了更简单的方案：不建 direct map，所有物理页访问都通过 `vm_mappages` 动态映射。这导致 `vm_mappages` 需要为新页表页分配 VA 时，它本身可能触发页错误，需要再分配页表页——这就是递归。备用页池是"递归发生时的兜底"。

**两个触发场景**：

1. **初始化阶段**（`!pt_init_done`）：VM 还没有自己的页表，无法通过 `vm_mappages` 将任意物理内存映射到自己的地址空间，只能使用内核预映射的 BSS 静态页
2. **运行时递归**（`level > 1`）：`vm_allocpages → vm_mappages → pt_writemap → vm_allocpages` 递归时，不能走正常路径（会无限递归），从 spare pool 直接取一个预分配好的页

**解决方案**：预先保留一部分物理页（spare pages），在递归发生时直接从备用池取页，打破循环依赖。

**注意**：spare pages 不是"防止内存不足"的机制——如果系统真的内存不足，spare pages 用完也一样失败（`vm_getsparepage` 返回 NULL，打印 "VM: warning: out of spare pages"）。它是打破递归的机制。

#### 2.3.2 数据结构

```c
// minix3/minix/servers/vm/alloc.c

#define MAXRESERVEDPAGES  300   // 每个队列最大页数
#define MAXRESERVEDQUEUES  15   // 最大队列数

static struct reserved_pages {
    struct reserved_pages *next;   // first_reserved_inuse 链表，alloc_cycle() 遍历此链表补充 spare pages
    int max_available;             // 队列容量
    int npages;                    // 每槽页数（通常为1）
    int mappedin;                  // 是否需要映射到 VM 进程的虚拟地址空间
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

**初始化**（[pagetable.c:1151](minix3/minix/servers/vm/pagetable.c#L1151) `pt_init()`）：

```c
// SPAREPAGES 值取决于编译配置：
//   SANITYCHECKS: SPAREPAGES=200, STATIC_SPAREPAGES=190
//   i386 生产版:   SPAREPAGES=20,  STATIC_SPAREPAGES=15
//   其他:          SPAREPAGES=150, STATIC_SPAREPAGES=140
// 以下以 SANITYCHECKS 配置为例：
#define SPAREPAGES 200
#define STATIC_SPAREPAGES 190

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

**使用**（[pagetable.c:333](minix3/minix/servers/vm/pagetable.c#L333) `vm_allocpages()`）：

```c
void *vm_allocpages(phys_bytes *phys, int reason, int pages)
{
    static int level = 0;
    level++;
    assert(level <= 2);  // 最多递归 2 层

    // 递归中 或 pt_init 未完成 → 从 spare page 取，打破循环依赖
    if((level > 1) || !pt_init_done) {
        void *s;
        if(pages == 1) s = vm_getsparepage(phys);
        else if(pages == 4) s = vm_getsparepagedir(phys);
        else panic("%d pages", pages);
        level--;
        if(!s) printf("VM: warning: out of spare pages\n");
        return s;
    }

    // 正常路径：alloc_mem → vm_mappages（可能触发递归）
    newpage = alloc_mem(pages, mem_flags);
    ret = vm_mappages(*phys, pages);  // ← 这里可能递归
    level--;
    return ret;
}
```

**关键逻辑**：`level` 变量检测递归深度。正常路径（`level == 1` 且 `pt_init_done`）走 `alloc_mem → vm_mappages`；递归时（`level > 1`）直接从 spare pool 取页，不再走 `vm_mappages`，打破循环。

**补充机制**：

当保留页被消耗后，系统会在 VM 主循环的空闲时间延迟补充：

```c
// VM 主循环中（main.c:118-119）：
if(missing_spares > 0) {
    alloc_cycle();  // 在等待下一个 IPC 前补充 spare pages
}

// 信号处理后（main.c:745-746）：
if(missing_spares > 0) {
    alloc_cycle();  // 确保信号处理期间消耗的 spare pages 被补充
}

// alloc_cycle() 内部（alloc.c:227-237）：
void alloc_cycle(void)
{
    for(rq = first_reserved_inuse; rq && missing_spares > 0; rq = rq->next) {
        reservedqueue_fill(rq);  // 调用 reservedqueue_addslot → alloc_mem + vm_mappages 补充
    }
}
```

**关键点**：spare pages 的补充是**延迟的**（在 VM 主循环的空闲时间），不是立即的。这进一步说明 spare pages 的目的是"打破递归"而非"保证供应"——如果目的是保证供应，补充应该是紧急的、立即的。

#### 2.3.5 设计要点

| 要点       | 说明                         |
| -------- | -------------------------- |
| **预分配**  | 启动时分配，打破 `vm_allocpages → vm_mappages → pt_writemap → vm_allocpages` 的递归循环 |
| **映射**   | 页表操作需要虚拟地址，所以 `mappedin=1` |
| **容量**   | 200 页（约 800KB），足够应对递归深度 |
| **自动补充** | 后台任务补充消耗的备用页 |


### 2.4 内存统计与资源限制

#### 2.4.1 系统级内存统计

**total\_pages**: 系统总物理内存页数

- VM 初始化时通过 `mem_add_total_pages()` 累加计算：内核静态/动态分配页 + 各模块占用页
- 初始化完成后不再变化，作为内存使用率的计算基准

**memstats**: 空闲内存统计

- 遍历空闲页位图，返回：空闲块数、空闲页总数、最大连续空闲块大小
- 用于诊断和 `printmemstats()` 输出，无自动内存回收或 OOM 逻辑

#### 2.4.2 进程级内存统计 (vm\_total / vm\_total\_max)

每个进程在 VM 中对应一个 `struct vmproc`（[vmproc.h:13-32](minix3/minix/servers/vm/vmproc.h#L13-L32)），其中包含两个统计字段：

```c
struct vmproc {
    // ... 页表、region 等字段 ...
    vir_bytes   vm_total;       // 当前已映射物理页总字节数
    vir_bytes   vm_total_max;   // 历史峰值（high water mark）
    u64_t       vm_minor_page_fault;
    u64_t       vm_major_page_fault;
};
```

**这两个字段的语义**

| 字段 | 含义 | 更新方向 |
|------|------|---------|
| `vm_total` | 当前进程**实际持有**的物理页总字节数 | 分配时 `+`，释放时 `-` |
| `vm_total_max` | 运行期间 `vm_total` 达到过的最大值 | 只增不减 |

> **注意**：统计的是**已映射的物理页**，不是虚拟地址空间大小。进程可能拥有很大的虚拟 region（如 mmap 预留），但只要没触发 page fault 分配物理页，就不计入 `vm_total`。

**更新逻辑**（[region.c:80-91](minix3/minix/servers/vm/region.c#L80-L91)）

`physblock_set()` 在物理页映射/解映射时更新统计：

```c
void physblock_set(struct vir_region *region, vir_bytes offset,
                   struct phys_region *newphysr)
{
    struct vmproc *proc = region->parent;
    if (newphysr) {
        // 新映射一页物理内存
        proc->vm_total += VM_PAGE_SIZE;
        if (proc->vm_total > proc->vm_total_max)
            proc->vm_total_max = proc->vm_total;
    } else {
        // 解映射一页物理内存
        proc->vm_total -= VM_PAGE_SIZE;
    }
}
```

触发 `physblock_set()` 的典型场景：
- **page fault**：首次访问匿名页 → `anon_pagefault()` → 分配物理页 → `physblock_set(region, offset, newphysr)`
- **COW 解引用**：写时复制触发 → 分配新物理页 → 替换原 `phys_region` → `physblock_set()`
- **释放 region**：`map_free_proc()` → 遍历 region 的 `physblocks[]` → `physblock_set(region, offset, NULL)`

**读取路径：getrusage**

PM 处理 `getrusage(2)` 时，通过 `vm_getrusage()` IPC 向 VM 查询。VM 的 `do_getrusage()`（[utility.c:424-461](minix3/minix/servers/vm/utility.c#L424-L461)）将 `vm_total_max` 转换为 KB 填入 `ru_maxrss`：

```c
r_usage.ru_maxrss = vmp->vm_total_max / 1024L;  // 单位 KB
```

VM 也将其暴露给 MIB 服务（[region.c:1444](minix3/minix/servers/vm/region.c#L1444)）：

```c
vui->vui_maxrss = vmp->vm_total_max / 1024L;
```

**生命周期：何时继承，何时清零**

| 场景 | 源码位置 | 对 `vm_total` / `vm_total_max` 的影响 |
|------|---------|-----------------------------------|
| **fork** | [vm/fork.c](minix3/minix/servers/vm/fork.c) `*vmc = *vmp` | **继承父值**。子进程的 `vmproc` 是父进程的完整副本，包括这两个字段 |
| **exec** | [libexec/exec_general.c:54](minix3/minix/lib/libexec/exec_general.c#L54) → [vm/exit.c:130-138](minix3/minix/servers/vm/exit.c#L130-L138) | **清零**。exec 加载新程序前，VFS/RS 调用 `vm_procctl_clear()` → VM 执行 `VMPPARAM_CLEAR` → `free_proc()` → `reset_vm_rusage()` 将两字段置 0 |
| **exit** | [vm/exit.c:33-43](minix3/minix/servers/vm/exit.c#L33-L43) | **清零**。`free_proc()` → `reset_vm_rusage()` |
| **运行中** | [region.c:85-90](minix3/minix/servers/vm/region.c#L85-L90) | **独立维护**。fork 后子进程有独立的 `vmproc` 实例，`region->parent` 指向子进程，page fault/COW 只修改子进程的字段 |

**exec 清零的完整调用链**：

```
VFS/RS exec 路径
    libexec_clearproc_vm_procctl(execi)     // libexec/exec_general.c:54
        → vm_procctl_clear(proc_e)          // libsys/vm_procctl.c:28
            → send VM_PROCCTL(VMPPARAM_CLEAR) to VM
                → do_procctl()              // vm/exit.c:117
                    → free_proc(vmp)        // vm/exit.c:134
                        → reset_vm_rusage(vmp)   // vm/exit.c:42
                            → vm_total = 0, vm_total_max = 0
```

**`reset_vm_rusage()` 源码**（[exit.c:25-31](minix3/minix/servers/vm/exit.c#L25-L31)）：

```c
static void reset_vm_rusage(struct vmproc *vmp)
{
    vmp->vm_total = 0;
    vmp->vm_total_max = 0;
    vmp->vm_minor_page_fault = 0;
    vmp->vm_major_page_fault = 0;
}
```

**总结**：`vm_total` / `vm_total_max` 的生命周期与进程地址空间的生命周期绑定——fork 时复制父进程的当前状态，exec 和 exit 时随地址空间清空而清零。它们不是资源限制，只是反映"该进程实际占用了多少物理页"的统计信息。

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

| 操作 | Minix3 C                   | 语义                    | Rust trait 方法                        |
| -- | -------------------------- | --------------------- | ------------------------------------- |
| 分配 | `alloc_mem(clicks, flags)` | 找到 k 个连续空闲页，标记为已用     | `PhysAllocator::alloc_mem()`          |
| 释放 | `free_mem(base, clicks)`   | 将 k 个连续页标记为空闲         | `PhysAllocator::free_mem()`           |
| 查询 | `memstats(&n, &p, &l)`     | 返回空闲块数、空闲页数、最大连续空闲块   | `PhysAllocatorStats::memstats()`      |
| 总量 | `total_pages` 全局变量        | 系统物理页总数（初始化时设定，不再变化）  | `PhysAllocator::total_count()`        |

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
    fn reserve_pages(&mut self, base_page: usize, count: usize);
}

pub trait PhysAllocatorStats {
    fn memstats(&self) -> PhysMemStats;
}
```

### 3.1 为什么需要预映射内存？

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

**Minix3 的技巧**：直接按**物理地址空间大小**（32 位系统上限 4GB）分配 bitmap，而非实际物理内存大小。因为 32 位系统物理内存不可能超过 4GB，所以无论实际物理内存是 512MB 还是 4GB，bitmap 统一 128KB 都能覆盖。

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

**为什么不能直接用 `Vec` 或 `Box`？** Rust 的 `GlobalAlloc` trait 是堆分配的基础——`Vec`、`Box`、`String` 等都依赖它。但 `GlobalAlloc` 需要物理页才能工作，物理分配器管理物理页，而物理页分配器的元数据如果用 `Vec`，就需要 `GlobalAlloc`——此时物理页分配器还没初始化（没有物理页可用），循环依赖无法打破。

**解决方案**：物理分配器元数据必须存储在"自己不管理"的内存中：

| 方案 | 存储 | 优点 | 缺点 |
|------|------|------|------|
| **预映射内存** | kernel 预映射的物理页 | 灵活，适配任意内存大小 | 需要额外初始化步骤 |
| **静态 BSS** | 编译时固定 | 简单 | 64 位系统不可行 |
| **保留页池** | 启动时预留 | Minix3 使用 | 需要预估大小 |

**Rust 实现**：使用 `&'static mut [T]` 而非 `Vec<T>`：

```rust
// ❌ 错误：Vec 需要 GlobalAlloc
pub struct BitmapAllocator {
    bitmap: Vec<u64>,  // 循环依赖！
}

// ✅ 正确：静态切片，从预映射内存分配
pub struct BitmapAllocator {
    bitmap: &'static mut [u64],  // 不依赖 GlobalAlloc
}
```

#### 3.1.5 解决方案：预映射内存 + BumpBuf

**核心思想**：VM 是用户态进程，启动时无法自己分配物理页。但 kernel 在启动 VM 之前，已经为 VM 建立了部分页表——这些页表映射了一块物理内存，VM 启动后可以直接读写。物理分配器的元数据就从这块**预映射内存**中分配。

**预映射内存**：kernel 在 VM 的页表中预先建立好映射，VM 启动时就能访问。当前设计采用 Direct Map 方案：kernel 初始页表提供 1GB direct map，VM 通过 `vm_phys_to_virt(phys) = VM_DIRECT_MAP_BASE + phys` 直接访问物理内存。BumpBuf 从 direct map 区域中分配元数据 slice，类型安全、自动对齐。

> 💡 **为什么是 Direct Map？** kernel 也可以为 VM 预先映射一些物理页（如 BSS 段，已有虚拟地址），但这些预映射只覆盖有限页面。VM 运行后需要访问新分配的物理页时，仍需通过 `vm_mappages` 动态建立映射——这又回到了递归问题。而 Direct Map 通过固定公式 `vm_phys_to_virt(phys) = VM_DIRECT_MAP_BASE + phys` 保证了**每一个物理页都自动拥有虚拟地址**，递归无从发生。详见 [05-vm-allocpage.md](05-vm-allocpage.md)。

**初始化流程**：

1. 获取物理内存信息（从 kernel IPC）
2. 计算元数据大小
3. 从预映射内存中分配元数据（通过 BumpBuf）
4. 初始化物理分配器（元数据已就位）
5. 如需搬迁（分配器迁移时），分配新元数据并迁移

**关键设计**：

- 物理分配器的元数据页**不应出现在分配器的空闲池中**——这些页在分配器初始化前已被占用。有两种方式保证这一点：

  **方式（a）排除法**：kernel 传递的 `BootMemRegion` 列表已排除元数据页，分配器初始化时自然不会将它们标记为空闲。`init()` 只将 `regions` 参数中的页标记为空闲，只要传入的 regions 排除了元数据页，分配器就不会管理它们。

  **方式（b）置位法**：分配器覆盖全部物理地址空间，初始化时 bitmap 全 0（已用），只对可用内存调用 `free_pages_internal()`，元数据页天然保持"已用"状态。需要时可通过 `reserve_pages()` 显式标记。Minix3 采用此方式：`free_pages_bitmap` 覆盖全部 4GB 物理地址空间（[alloc.c:33-35](minix3/minix/servers/vm/alloc.c#L33-L35)），`mem_init()` 先 `memset(0)` 全部置为已用，再对 `mem_chunks` 调用 `free_mem()` 标记空闲（[alloc.c:321-334](minix3/minix/servers/vm/alloc.c#L321-L334)）。

  **方式（a）的问题**：（1）`total_count()` 返回的是 regions 覆盖的地址范围，而非系统实际物理页总数——与 Minix3 语义不对齐；（2）元数据页不在任何分配器视野中，初始化完成后无法回收——成为"孤儿页"；（3）迁移到新分配器时，只能传入相同的 regions，无法将元数据页纳入新分配器的管理。

  **方式（b）更优**：分配器拥有完整的物理内存视图，`total_count()` 和 `memstats()` 统计准确；元数据页可随时通过 `free_mem()` 回收；迁移时只需在新分配器中重建相同状态。

  **当前 Rust 代码已采用方式（b）**。`PhysAllocator` trait 定义了 `reserve_pages()` 方法（[alloc_trait.rs:14](os/servers/vm/src/phys_mem/alloc_trait.rs#L14)），各分配器的 `init()` 接收 `total_pages` 和 `free_regions` 两个参数——`total_pages` 覆盖全部物理地址空间，`free_regions` 仅列出可用内存区域。初始化时 bitmap 全 0（已用），再对 `free_regions` 调用 `free_pages_internal()` 标记空闲，元数据页天然保持"已用"状态。若需显式标记已占用的页（如元数据页），可调用 `reserve_pages()`。以 `BitmapAllocator` 为例：

  ```rust
  // alloc_trait.rs — PhysAllocator trait
  pub trait PhysAllocator {
      fn alloc_mem(&mut self, clicks: usize, flags: PageAllocFlags) -> Result<PhysBytes, AllocError>;
      fn free_mem(&mut self, base: PhysBytes, clicks: usize);
      fn total_count(&self) -> usize;
      fn reserve_pages(&mut self, base_page: usize, count: usize);
  }
  ```

  ```rust
  // bitmap_alloc.rs — init() 签名
  pub fn init(metadata: &mut [u8], total_pages: usize, free_regions: &[BootMemRegion]) -> Self {
      let bitmap_chunks = (total_pages + BITS_PER_CHUNK - 1) / BITS_PER_CHUNK;
      let mut buf = BumpBuf::new(metadata);
      let bitmap = buf.alloc_slice::<u64>(bitmap_chunks);
      for chunk in bitmap.iter_mut() { *chunk = 0; }  // 全部置为已用
      // ...
      let mut alloc = Self { bitmap, total_pages, free_pages: 0, /* ... */ };
      for region in free_regions {                     // 只对可用内存标记空闲
          alloc.free_pages_internal(base_page, num_pages);
      }
      alloc
  }
  ```

  ```rust
  // bitmap_alloc.rs — reserve_pages() 实现
  fn reserve_pages(&mut self, base_page: usize, count: usize) {
      for i in base_page..base_page + count {
          if i >= self.bitmap_len() { break; }
          if self.page_is_free(i) {
              let chunk = i / BITS_PER_CHUNK;
              let bit = i % BITS_PER_CHUNK;
              self.bitmap[chunk] &= !(1u64 << bit);
              self.free_pages -= 1;
          }
      }
  }
  ```

  **物理内存分配器自己占用的内存怎么办？** 方式（b）下，分配器元数据（bitmap、free list 等）占用的物理页通过 `reserve_pages()` 标记为已用——这些页在分配器的管理视野中，`total_count()` 统计了它们，`free_pages` 排除了它们。当分配器迁移或元数据搬迁后，这些页可通过 `free_mem()` 回收。这比方式（a）的"孤儿页"问题更合理：分配器知道自己管理了多少物理页，也知道哪些页被谁占用了。

- 元数据类型为 `&'static mut [T]`，生命周期为 `'static`

**BumpBuf 的意义**：

预映射内存是一块连续的原始字节。物理分配器的元数据（bitmap、free list heads 等）需要从这块内存中切出类型安全的 slice。BumpBuf 就是做这件事的便利工具：

1. **为什么需要它**：有了预映射内存后，完全可以手写指针运算构建元数据，但这需要 unsafe、手动对齐、容易出错
2. **它是什么**：一个封装好的 bump allocator，提供 `alloc_slice::<T>()` 方法，类型安全、自动对齐
3. **生命周期**：初始化完成后不再增长，但分配出去的内存（`&'static mut [T]`）由物理分配器持有，继续使用

| 方式 | 代码 | 难度 |
|------|------|------|
| **手写低级** | `let ptr = phys as *mut u64; ptr.write_bytes(0, n);` | unsafe、手动对齐、易错 |
| **BumpBuf** | `let slice: &'static mut [u64] = bump_buf.alloc_slice(n);` | 类型安全、自动对齐 |

简言之：BumpBuf 不提供"新能力"，只是让低级编程更简单、更不容易出错。

***

## 4. BumpBuf 设计

§3.1.5 介绍了"预映射内存 + BumpBuf"的解决方案。本节详细设计 BumpBuf——从预映射内存中切出类型安全 slice 的便利工具。

### 4.1 设计约束

§3.1.4 已论述物理分配器不能使用堆（循环依赖）。本节从另一角度补充：即使不考虑`GlobalAlloc`，分配器也不应为自身分配元数据——

- **初始化时**：分配器尚未就绪，`alloc_mem()` 不存在可调用的对象
- **运行时扩展**：分配器为自身分配元数据，则它在同一调用中既是服务者又是消费者，正确性难以保证（例如链表式 buddy 拆分时需动态分配 ListNode，若 ListNode 来自 `alloc_mem()`，则 `alloc_mem()` 在拆分过程中再次调用自身——递归）

从职责分离的角度，分配器应只服务于外部请求，而非同时管理自身的存储需求——将二者混为一体，在概念上构成自指循环，破坏了设计的清晰性。因此，分配器的元数据必须一次性从外部获取，不再增长。

- **按最坏估计一次性分配**：`init()` 时按最坏情况预留全部元数据
- **不可扩展，只能搬迁**：如需管理更大物理内存，必须废弃当前分配器、整体迁移
- **BumpBuf 适配**：只分配不释放不增长，bump allocator 即可满足；`alloc_slice<T>()` 提供类型安全封装

### 4.2 BumpBuf 实现

**本质**：预映射内存 + 简单的 bump 指针运算 + 类型安全封装。

```rust
struct BumpBuf {
    ptr: *mut u8,
    offset: usize,
    len: usize,
}

impl BumpBuf {
    fn new(buf: &mut [u8]) -> Self {
        Self {
            ptr: buf.as_mut_ptr(),
            offset: 0,
            len: buf.len(),
        }
    }

    fn alloc_slice<T>(&mut self, count: usize) -> &'static mut [T] {
        if count == 0 { return &mut []; }
        let size = count * core::mem::size_of::<T>();
        let align = core::mem::align_of::<T>();
        let current = self.ptr as usize + self.offset;
        let aligned = (current + align - 1) & !(align - 1);
        let padding = aligned - current;
        let new_offset = self.offset + padding + size;
        assert!(
            new_offset <= self.len,
            "metadata buffer exhausted: need {} bytes, have {}",
            size,
            self.len - self.offset - padding,
        );
        self.offset = new_offset;
        unsafe { core::slice::from_raw_parts_mut(aligned as *mut T, count) }
    }
}
```

**当前 Rust 代码中的使用**：三个分配器（`BitmapAllocator`、`BuddyAllocator`、`SegmentTreeAllocator`）各自内嵌 `BumpBuf`，从 `init()` 接收的 `metadata: &mut [u8]` 参数中分配 slice。metadata buffer 来自 direct map 区域——物理内存前 1GB 内，通过 `vm_phys_to_virt()` 计算 VA。BumpBuf 只关心一块连续的 `&mut [u8]`，不关心这块内存的 VA 是怎么来的。

> **注意**：BumpBuf 是各分配器文件内的私有结构体，不是公共 API。每个分配器独立定义自己的 BumpBuf，代码相同但互不共享。

### 4.3 使用示例

```rust
// 初始化（metadata 来自预映射内存）
let mut buf = BumpBuf::new(metadata);

// 分配元数据
let bitmap: &'static mut [u64] = buf.alloc_slice(1024);
let heads: &'static mut [u32] = buf.alloc_slice(64);
```

### 4.4 物理内存初始化流程

§3.1.5 给出了 5 步初始化流程的概要。本节展开 Direct Map 方案下的具体实现，分为 3 个阶段。核心洞察是 **"1GB 足以启动世界"**——bitmap 元数据永远能放进 1GB（即使 4TB 物理内存也只需 ~128MB）。

**VM 物理内存初始化：3 阶段启动**

- **Phase 1: Bootstrap** — 1GB direct map → bitmap allocator
  - Kernel 初始页表已建立 1GB direct map（1 个 1GB huge page）
  - VM 启动时，前 1GB 物理内存已通过 `vm_phys_to_virt()` 可访问
  - 从 boot_info 获取 memmap，计算 bitmap 大小
  - bitmap 元数据分配在物理内存前 1GB 内（通过 BumpBuf + `vm_phys_to_virt`）
  - bitmap allocator 可用，管理全部物理页的分配状态

- **Phase 2: Direct Map 扩展** — bitmap 分配页表页 → 扩展覆盖全部物理内存
  - 如果物理内存 ≤ 1GB：无需扩展，跳过此阶段
  - 如果物理内存 > 1GB：
    - `bitmap.alloc_phys()`在前 1GB 物理内存中分配新页表页（PDPT/PD）——只有前 1GB 已有 VA
    - 通过 `vm_phys_to_virt()` 获取 VA 后直接读写页表页内容，零递归
    - 扩展 direct map 覆盖全部物理内存
  - 扩展完成后，`vm_phys_to_virt()` 覆盖全部物理内存

- **Phase 3: 分配器选择** — 根据物理内存大小选择策略
  - bitmap 是 O(n) 分配（线性扫描找空闲位），buddy 是 O(log n)
  - **≤ 1GB 物理内存**：bitmap 仅 32KB，扫描开销可忽略，简单性占优
  - **> 1GB 物理内存**：bitmap 扫描随内存线性增长，buddy 的 O(log n) 不受内存规模影响，应迁移到 buddy
  - 迁移时 buddy 元数据通过 `vm_phys_to_virt()` 访问

#### Phase 1 不是"临时方案"，而是"自举基础"

1GB direct map 不覆盖全部物理内存（现代系统通常 16GB+），但 bitmap 元数据永远在前 1GB 内——因为 bitmap 占用的空间远小于被管理的物理内存。这意味着自举阶段不需要额外的中间存储，1GB direct map 足以让 bitmap allocator 启动并管理任意大小的物理内存。但 bitmap 的 O(n) 分配性能随物理内存线性增长，大内存下可能需要迁移到 buddy（见 Phase 3）。

| 物理内存 | Bitmap 大小 | 能放进 1GB 吗 |
|----------|------------|--------------|
| 16GB | 512KB | ✅ |
| 128GB | 4MB | ✅ |
| 512GB | 16MB | ✅ |
| 1TB | 32MB | ✅ |
| 4TB | 128MB | ✅ |

Bitmap 本身占用的物理页也在前 1GB 内，通过 direct map 直接读写。初始化阶段不需要额外的中间存储。但分配器迁移时可能需要搬迁——见 Phase 3 讨论。

#### Phase 2 不是"补救"，而是"扩展"

如果物理内存 > 1GB，需要扩展 direct map。但扩展本身不需要任何新的自举机制：

1. `bitmap.alloc_phys()` 分配新页表页（PDPT 或 PD 页）——前 1GB 已有 VA
2. 通过 `vm_phys_to_virt(pt_phys)` 获取 VA 后直接读写新页表页——此时 direct map 至少覆盖 1GB，新页表页的物理地址一定在前 1GB 内
3. 写入映射条目，扩展 direct map 覆盖范围

**零递归**：操作页表页不需要 `vm_mappages` / `pt_ptalloc_in_range`，因为页表页通过 `vm_phys_to_virt()` 直接访问。

> `vm_mappages` / `pt_ptalloc_in_range` 详见 [07-pagetable-ops.md](07-pagetable-ops.md)。

**完备性论证**：一个 PDPT 页能容纳 512 个 1GB 条目，覆盖 512GB 物理内存。超过 512GB 需要新 PDPT 页，但此时 VM 已有至少 1GB direct map，可以 `alloc_phys() → vm_phys_to_virt()` 直接操作新 PDPT 页。仍然零递归、零额外自举风险。

#### Phase 3 不是"必须"，而是"策略选择"

Bitmap allocator 的 `alloc_mem()` 是线性扫描，O(n) 复杂度。Buddy allocator 是 O(log n)。但 VM 的物理页分配模式是非高频的（fork、mmap、page fault 时才分配），bitmap 可能就够用。

迁移到 buddy 的条件是**性能测量表明 bitmap 成为瓶颈**，而不是"buddy 理论上更优"。

**存在搬迁机制，但可以选择不执行**。分配器迁移时，新分配器的元数据需要分配在物理内存中。元数据远小于被管理的物理内存（例如管理 16GB 的 buddy 元数据约 20MB），通常都能放进前 1GB。只有当元数据本身可能超出前 1GB 的可用空间时，才需要先确保 direct map 已扩展（Phase 2），再分配新元数据。

#### Minix3 对比

| 方面 | Minix3 | Minix-RS（Direct Map） |
|------|--------|---------|
| **元数据存储** | 静态 BSS（`free_pages_bitmap[128KB]`，硬编码 4GB 上限） | BumpBuf 从预映射区域分配 |
| **元数据可访问性** | BSS 段天然有 VA，无需额外映射 | 元数据通过 direct map 获得可访问性（BumpBuf 从预映射区域分配） |
| **是否搬迁** | 否（BSS 生命周期 = 进程生命周期） | 视物理内存大小和算法效率而定（≤ 1GB 保留 bitmap，> 1GB 迁移 buddy） |
| **地址空间限制** | 32 位 | 64 位（512GB+ 可扩展） |
| **便利工具** | 无（直接操作 BSS 数组） | BumpBuf（类型安全、自动对齐） |

***

## 5. 物理分配器实现

> **类型说明**：本节使用的 `PhysBytes`、`PageAllocFlags`、`AllocError` 等辅助类型定义见 [§6](#6-辅助类型)。

### 5.1 Trait 定义

trait 定义见 §3 核心操作。此处补充 `PhysMemStats` 和 `PageAllocFlags` 的完整定义：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysMemStats {
    pub free_nodes: usize,    // memstats.nodes: number of free blocks
    pub free_pages: usize,    // memstats.pages: total free pages
    pub largest_free: usize,  // memstats.largest: largest contiguous free block
}

> `PageAllocFlags` 完整定义见 [§6.2](#62-pageallocflags--分配标志)。

### 5.2 三种实现方案

如 §3.0 所述，物理内存分配的本质是**连续区间分配问题**——在空闲区间集合中找到满足约束的连续区间，标记为已用；释放时合并相邻空闲区间。对这个问题的求解，从朴素到精巧，有三种思路：

| | 本质 | 与问题域的关系 |
| -- | ---- | ---------- |
| **Bitmap** | 朴素解法 | 对问题域无任何假设，暴力扫描 |
| **线段树** | 通用解法 | 对问题域无约束，理论最优 O(log n) |
| **Buddy** | 特化解法 | 对问题域施加约束（块大小必须 2^n），换取更好的性质 |

Buddy 的约束——所有块大小必须是 2^n 且 2^n 对齐——让问题大幅简化：

1. **合并规则极简**：线段树需维护 `left_free`/`right_free`/`max_free` 来判断任意区间能否合并；buddy 只需检查 `page ^ (1 << order)` 这一个地址的 buddy 是否空闲
2. **空间复杂度降低**：线段树每节点 4 个 usize（32 字节），buddy 每页只需 1 字节（order + flag）
3. **天然抗碎片**：2^n 约束 + buddy 合并保证外部碎片不可能累积——任何释放的块最终都能合并回大块

代价是**内部碎片**：分配 3 页实际占用 4 页。这是特化的典型 trade-off：用精度换性质。严格来说，buddy 不是线段树的"特化"——它们的数据结构完全不同。更准确的说法是：**buddy 是对问题域施加约束后得到的特化算法**。约束让问题变简单了，所以不需要线段树那么重的数据结构，用轻得多的 SoA 数组就够了。线段树是**无约束区间分配**的通用解；buddy 是**2^n 约束区间分配**的最优解。这正是 buddy 成为内核广泛采用的物理内存算法的原因：**用精度换性质，用约束换简洁**。

当前代码使用 **`PhysAlloc` 枚举**实现运行时分配器选择，兼顾静态分派与灵活性：

```rust
pub(crate) enum PhysAlloc {
    Bitmap(BitmapAllocator),
    #[cfg(feature = "buddy_alloc")]
    Buddy(BuddyAllocator),
    #[cfg(feature = "segment_tree_alloc")]
    SegmentTree(SegmentTreeAllocator),
}
```

**机制**：`PhysAlloc` 为 `PhysAllocator` trait 的枚举包装，所有方法通过 `match` 分派到具体实现——编译期确定分支，零虚表开销。初始化时默认为 `Bitmap`（自举友好，元数据小），后续可根据物理内存大小（> 1GB）迁移到 `Buddy`。`Buddy` 和 `SegmentTree` 通过 feature gate 控制：`buddy_alloc` 启用 Buddy，`segment_tree_alloc` 启用线段树（可替换 Buddy 用于教学/实验）。`Bitmap` 始终可用，作为自举方案和小内存下的长期方案。

### 5.3 BitmapAllocator

> 源码：`os/servers/vm/src/phys_mem/bitmap_alloc.rs`

**核心设计**：使用位图管理物理页状态，与 Minix3 `alloc.c` 一致。每个 bit 代表一页，1 = 空闲，0 = 已用。

```rust
pub struct BitmapAllocator {
    bitmap: &'static mut [u64],    // 1 bit = 1 页，1 = 空闲
    total_pages: usize,
    free_pages: usize,
    page_cache: &'static mut [usize],  // 单页快速分配缓存（Minix3 free_page_cache）
    page_cache_size: usize,            // 缓存条目数
    stats: MemStats,                   // 分配/释放统计
}
```

**初始化**：

```rust
pub fn init(metadata: &mut [u8], total_pages: usize, free_regions: &[BootMemRegion]) -> Self {
    let mut buf = BumpBuf::new(metadata);
    let bitmap = buf.alloc_slice::<u64>(bitmap_chunks);
    let page_cache = buf.alloc_slice::<usize>(PAGE_CACHE_MAX);
    // ... 初始化 bitmap 为全 0，然后将 free_regions 标记为空闲（同时入 page_cache）
}
```

metadata buffer 来自 direct map 区域，BumpBuf 从中依次切出 `bitmap` 和 `page_cache` 两个 slice。`metadata_size()` 按需计算所需空间（bitmap + cache + 对齐余量），确保 BumpBuf 不会溢出。

**核心方法**：

```rust
fn alloc_pages(&mut self, pages: usize, max_page: usize, use_cache: bool) -> Option<usize> {
    // 单页 + use_cache 时，优先从 page_cache LIFO 弹出 → O(1)
    // cache 未命中或需多页时，退化为 find_bit 反向扫描
}

fn find_bit(&self, low: usize, start_scan: usize, pages: usize) -> Option<usize> {
    // 反向扫描 bitmap（高→低），找连续 pages 个空闲位
    // 利用 chunk 全零快速跳过已分配区域
}

fn free_pages_internal(&mut self, start_page: usize, num_pages: usize) {
    // 逐位设置 bitmap = 1，更新 free_pages 计数
    // 同时将每页压入 page_cache（LIFO），供后续单页快速分配
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
        // 页缓存：单页分配且无地址限制时，优先从 page_cache 弹出（LIFO）→ O(1)
        // 对齐：ALIGN64K/ALIGN16K → 多分配 align_clicks 页，释放前缀未对齐部分
        // 低内存：LOWER16MB/LOWER1MB → 限制搜索范围 max_page（此时不用 page_cache）
        // 重试：分配失败时调用 cache_freepages() 回收文件缓存页（Minix3 完整实现；RS 当前为 placeholder，返回 0，TODO）
        // 清零：CLEAR → vm_phys_to_virt() + write_bytes() 清零（direct map 下直接访问，无需 Minix3 的 sys_memset IPC）
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
| 存储位置 | 静态 BSS 数组 `free_pages_bitmap[]` | BumpBuf 分配的 `&'static mut [u64]` |
| 大小 | 固定 128KB（4GB 地址空间） | 按需计算 `metadata_size()` |
| chunk 类型 | `bitchunk_t` (u32) | `u64` |
| 初始化 | `mem_init()` 写 BSS | `init()` 从 BumpBuf 分配 |
| page cache | `free_page_cache[]` + `cache_freepages()` | `page_cache: &'static mut [usize]` + `cache_freepages()` placeholder |
| PAF_CLEAR | `sys_memset()` 清零（VM→kernel IPC） | `vm_phys_to_virt()` + `write_bytes()`（direct map 直接访问） |
| 统计 | 无 | `MemStats` 跟踪分配/释放次数 |

**辅助方法**：

```rust
pub fn total_memory(&self) -> usize       // total_pages * CLICK_SIZE
pub fn free_memory(&self) -> usize        // free_pages * CLICK_SIZE
pub fn is_under_pressure(&self) -> bool   // free_pages * 10 < total_pages
```

### 5.4 BuddyAllocator (SoA 结构)

> 源码：`os/servers/vm/src/phys_mem/buddy_alloc.rs`

**Buddy 系统简介**：Buddy 分配器是经典物理内存管理算法，Linux 内核亦采用此方案。核心思想是将内存按 2^n 划分：每个块大小为 2^order 页，两个大小相同、地址相邻的块互为"buddy"。分配时从匹配的 order 链表取块，不足则向上分裂大块（一分为二）；释放时检查 buddy 是否空闲，若空闲则自动合并，如此递归。优势是 O(log n) 分配/释放且天然抗外部碎片，代价是内部碎片（向上取整到 2^n）。

**SoA 设计选择**：传统 buddy 实现使用指针链表（如 Linux 的 `struct free_area`），但指针在 Rust 中引入所有权和生命周期问题，且对 BumpBuf 分配不友好。本实现采用 SoA（Structure of Arrays）结构——三个平行的数组替代指针链表，更 cache 友好，也更适合从 BumpBuf 一次性分配。

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
    stats: MemStats,
}
```

**初始化**：

```rust
pub fn init(metadata: &mut [u8], total_pages: usize, free_regions: &[BootMemRegion]) -> Self {
    let mut buf = BumpBuf::new(metadata);
    // 从 BumpBuf 分配三个数组
    // 将 free_regions 按 2^n 对齐切分后加入空闲链表
}
```

metadata buffer 来自预映射内存，BumpBuf 从中切出类型安全的 slice。

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
        // 对齐：ALIGN64K/ALIGN16K → 计算 align_order，取 size_order.max(align_order) 作为分配 order
        //   buddy 块天然 2^n 对齐，order >= align_order 即满足对齐要求，无需释放前缀
        // 低内存：LOWER16MB/LOWER1MB → 分配后检查是否超出 max_page，超出则释放返回 Err
        // 清零：CLEAR → vm_phys_to_virt() + write_bytes()（direct map 直接访问，无需 Minix3 的 sys_memset IPC）
        // 连续：CONTIG → no-op（buddy 总是连续分配）
        // 注意：buddy 向上取整到 2^n 页分配，但会释放尾部多余部分（block_size - clicks）以减少内部碎片
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

**线段树简介**：区间分配问题的标准数据结构。将 n 个页组织为完全二叉树，每个节点维护其区间的聚合信息（最大连续空闲、左端连续空闲、右端连续空闲），分配时从根向叶 O(log n) 定位，释放时自底向上 O(log n) 更新，实现精确的任意大小分配。本实现采用静态线段树（一次性分配全部节点），4GB 物理内存需约 64MB 元数据——是 bitmap 的 500 倍。若改用动态开点线段树（按需创建节点），初始仅一个区间时开销极小，但 BumpBuf 要求按最坏情况预分配，动态方案无法适用。

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
    stats: MemStats,
}
```

**初始化**：

```rust
pub fn init(metadata: &mut [u8], total_pages: usize, free_regions: &[BootMemRegion]) -> Self {
    let mut buf = BumpBuf::new(metadata);
    // 从 BumpBuf 分配 2 * next_power_of_two(n) 个节点
    // 先将所有节点设为 used，再对 free_regions 中的页设为 free
    // 最后自底向上 merge() 重建内部节点
}
```

metadata buffer 来自预映射内存，BumpBuf 从中切出类型安全的 slice。

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
        // 清零：CLEAR → vm_phys_to_virt() + write_bytes()（direct map 直接访问，无需 Minix3 的 sys_memset IPC）
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

根据内核传递的物理内存大小（`total_pages`），各分配器需计算元数据所需空间，以便从预映射内存中一次性分配。`PhysAllocType` 枚举提供了 `metadata_size` / `metadata_size_exact` 方法，在初始化 BumpBuf 前即可预知所需空间：

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
                tree_size * size_of::<(usize, usize, usize, usize)>()  // SegmentNode = (max_free, left_free, right_free, len)
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
    pub fn new(addr: u64) -> Self {
        assert!(addr % CLICK_SIZE as u64 == 0, "PhysBytes must be page-aligned, got {addr:#x}");
        PhysBytes(addr)
    }
    pub const fn as_u64(&self) -> u64 { self.0 }
    pub const fn as_usize(&self) -> usize { self.0 as usize }
    pub fn from_page_index(idx: usize) -> Self {
        PhysBytes((idx as u64) * CLICK_SIZE as u64)
    }
    pub fn page_index(&self) -> usize {
        (self.0 as usize) / CLICK_SIZE
    }
    pub fn add(&self, offset: usize) -> Self {
        let new_addr = self.0 + offset as u64;
        assert!(new_addr >= self.0, "PhysBytes::add overflow");
        PhysBytes(new_addr)
    }
}
```

> **注意**：`new()` 非 `const fn`（含 `assert!`），且强制页对齐——传入非页对齐地址会 panic。`from_page_index()` 和 `add()` 是安全的构造方式。

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
| **元数据存储**     | 静态 BSS 数组     | 预映射内存 + BumpBuf + `&'static mut [T]` |
| **Bitmap 大小** | 固定 128KB（4GB） | 按需计算                            |
| **堆就绪时机**     | `pt_init()` 后 | `mem_init()` 前就需要预映射内存         |
| **分配器选择**     | 仅 Bitmap      | Bitmap / Buddy / 线段树            |
| **Buddy 安全性** | N/A           | 状态标志 + 严格检查                     |

**关键差异**：

1. **64 位系统无法使用静态 BSS**
   - 物理内存无上限，bitmap 大小不确定
   - 必须动态分配 → 需要预映射内存
2. **初始化顺序不同**
   - Minix3：`mem_init()` 不需要堆
   - Rust：`mem_init()` 前必须准备好预映射内存
3. **Buddy 安全性增强**
   - 使用 `FLAG_ALLOCATED` 防止误判已分配页
   - 使用 `ORDER_INVALID` 防止误判未初始化页

***

## 8. 文件结构

```
phys_mem/
├── mod.rs                    # 模块入口
├── alloc_trait.rs            # PhysAllocator + PhysAllocatorStats traits, PhysMemStats
├── types.rs                  # PhysBytes, PageAllocFlags, AllocError
├── bitmap_alloc.rs           # Bitmap 分配器
├── buddy_alloc.rs            # Buddy 分配器 (SoA)
├── segment_tree_alloc.rs     # 线段树分配器 (实验性)
├── stats.rs                  # 运维统计 (alloc/free 计数)
└── allocator_tests.rs        # 统一测试套件

src/ (顶层)
├── direct_map.rs             # Direct map 常量与 vm_phys_to_virt()
└── alloc_page.rs             # VmPageAllocator
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
- [05-vm-allocpage.md](05-vm-allocpage.md) - VM 堆初始化与保留页池
- [03-acl.md](03-acl.md) - ACL 权限控制
- [08-slab-allocator.md](08-slab-allocator.md) - VM 内部使用 Slab
- [17-vm-fork.md](17-vm-fork.md) - fork 时的内存分配
- [19-vm-map.md](19-vm-map.md) - VM\_MAP 服务中的内存分配

***

*分类: VM库 | 接口性质: 其他服务通过 IPC 调用*

---

## 附录 A：PAF_CONTIG 与 region 层的连续语义

> **来源**：§2.1.1 中 `alloc_mem()` 分析的延伸讨论。

Minix3 的 `alloc_mem()` **总是分配物理连续内存**。这是历史遗留设计：

- Minix 1/2 运行在 8086/80286 上，没有分页 MMU，必须物理连续
- Minix3 虽然运行在 386+ 有 MMU，但 `alloc_mem` 保留了连续分配语义以简化实现
- `PAF_CONTIG` 定义了但在 `alloc_mem` 层是 no-op，说明曾有计划在分配器层区分"连续"和"非连续"

**但实际上 Minix3 在上层 region 系统实现了非连续分配**：

- `mem_type_anon`（默认路径）：**按需分配（lazy）**，没有 `ev_new` handler，region 创建时不预分配物理内存。进程首次访问某页时触发 pagefault → `map_pf()` → `anon_pagefault()` → `alloc_mem(1, ...)` 分配 1 页并映射。N 页虚拟区域经过 N 次独立 pagefault 后得到 N 个物理上不连续的页——因为每次分配的是当时空闲的任意一页，自然不会连续。heap、stack、普通 mmap 都走这条路。若指定 `MAP_PREALLOC`（不含 `MAP_CONTIG`），则 region 创建后立即逐页触发 pagefault 预分配，物理页仍然不连续。
- `mem_type_anon_contig`（特殊路径）：**预分配（eager）**，有 `ev_new` handler，在 region 创建时（`map_page_region` → `ev_new` → `anon_contig_new`）一次调 `alloc_mem(N, ...)` 分配 N 页连续物理内存并全部映射。仅在 `mmap` 时指定 `MAP_CONTIG | MAP_PREALLOC` 才使用（单独 `MAP_CONTIG` 会被拒绝，返回 EINVAL）。且不能 fork、不能 resize、不能 pagefault。
  - 不能 fork：`ev_reference` handler 返回 ENOMEM（打印 "cannot fork with physically contig memory"），但 `map_copy_region` 未检查该返回值，因此 fork 实际上仍会继续，只是 contig 区域在子进程中引用关系不正确——Minix3 的已知限制
  - 不能 resize：`ev_resize` 返回 ENOMEM，contig 区域不可扩展
  - 不能 pagefault：`ev_pagefault` 直接 panic，物理内存已在 `ev_new` 时预分配，不应再触发缺页

因此"连续 vs 非连续"的区分不在 `alloc_mem` 层，而在 region 层。`PAF_CONTIG` 是分配器层的未实现尝试，而 `MAP_CONTIG` 是 region 层已实现的功能。
