# 08-slab-allocator: Slab 分配器

> **分类**: VM私有 ✅
> **源码**: [slaballoc.c](minix3/minix/servers/vm/slaballoc.c)  
> **说明**: VM 专用的内存分配器，用于分配固定大小的对象

---

## 1. 概述

**Slab 分配器原理**

Slab 分配器是一种内存管理技术，用于高效分配固定大小的对象。它通过预分配大块内存（slab）并将其分割成相同大小的对象来减少内存碎片和分配开销。

```
┌─────────────────────────────────────────────────────────────┐
│                        Slab 分配器架构                        │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │  Slab 0     │  │  Slab 1     │  │  Slab 2     │  ...     │
│  │  (8 bytes)  │  │  (16 bytes) │  │  (32 bytes) │          │
│  ├─────────────┤  ├─────────────┤  ├─────────────┤          │
│  │ ┌─────────┐ │  │ ┌─────────┐ │  │ ┌─────────┐ │          │
│  │ │ Object 0│ │  │ │ Object 0│ │  │ │ Object 0│ │          │
│  │ ├─────────┤ │  │ ├─────────┤ │  │ ├─────────┤ │          │
│  │ │ Object 1│ │  │ │ Object 1│ │  │ │ Object 1│ │          │
│  │ ├─────────┤ │  │ ├─────────┤ │  │ ├─────────┤ │          │
│  │ │ Object 2│ │  │ │ Object 2│ │  │ │ Object 2│ │          │
│  │ ├─────────┤ │  │ ├─────────┤ │  │ ├─────────┤ │          │
│  │ │   ...   │ │  │ │   ...   │ │  │ │   ...   │ │          │
│  │ └─────────┘ │  │ └─────────┘ │  │ └─────────┘ │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
└─────────────────────────────────────────────────────────────┘
```

**核心概念**

| 概念 | 说明 | 优势 |
|-----|------|------|
| **Slab** | 一个或多个物理页组成的内存块 | 减少系统调用次数 |
| **Object** | 固定大小的内存单元 | 消除内部碎片 |
| **Bitmap** | 标记对象使用状态的位图 | O(1) 分配/释放 |
| **Cache** | 特定大小对象的 slab 集合 | 提高缓存命中率 |

**为什么 VM 使用 Slab 而非完全依赖 malloc？**

> ⚠️ **澄清**：VM **可以**使用 malloc。VM 有自己的 `brk` 快速路径（`utility.c:_brk()`），直接调用 `alloc_mem()` 分配物理页并映射到自己的地址空间。VM 实际上也使用了 `calloc`/`realloc`/`free`（如 `region->physblocks` 数组）。
> 
> 以上是 Minix3（32 位 C 实现）的现状。Rust 版本中，Direct Map 解决了"物理页可达性"（`vm_phys_to_virt()` 直接访问），HeapArena 解决了"虚拟连续性"（预留连续 VA 区间，逐页映射物理页）。VM 不再需要 brk/sbrk，但需要 HeapArena 为 Rust 堆提供连续 VA。详见 §4.1 和 07-pagetable-ops.md §3.0.4。

**Slab 的价值在于性能优化**：

1. **O(1) 分配复杂度**：固定大小对象直接定位，无需搜索
2. **消除内部碎片**：每个 slab 只存储一种大小的对象
3. **缓存友好**：对象连续存储，提高缓存命中率
4. **调试支持**：内置泄漏检测和 sanity check

**VM 的混合分配策略**：

| 分配类型 | 使用方式 | 原因 |
|---------|---------|------|
| vir_region/phys_block 等结构体 | **Slab** | 高频分配、固定大小 |
| region->physblocks 数组 | **malloc** | 可变大小、低频分配 |
| 临时缓冲区 | **malloc** | 不确定大小 |

```
对比：malloc vs Slab 分配器

malloc:
  用户请求 64 bytes
  ↓
  libc 查找合适的内存块
  ↓
  可能分割大块内存
  ↓
  返回地址（可能不连续）
  
Slab:
  用户请求 64 bytes
  ↓
  直接定位到 64-byte slab
  ↓
  从位图找到空闲对象
  ↓
  返回地址（固定偏移，连续）
```

**Minix3 VM Slab 的独特设计**

**1. VM 私有 vs Linux 全局**

| 特性 | Minix3 VM | Linux Kernel |
|-----|-----------|--------------|
| **作用域** | VM 进程私有 | 全局共享 |
| **初始化** | VM 启动时初始化 | 内核启动时初始化 |
| **内存来源** | VM 自己的地址空间 | 全局内核内存池 |
| **使用场景** | 仅 VM 内部结构体 | 所有内核对象 |

**设计原因**：
- Minix3 采用微内核架构，VM 是独立的用户态进程
- VM 有自己的地址空间，需要自己的内存分配器
    - **为什么不用内核 slab？** 每次分配都需要 IPC 调用，性能开销大
    - **为什么不用 malloc？** slab 针对固定大小对象优化，性能更好（详见上文）
    - **私有 slab 的优势**：避免了 IPC 开销，同时保留 slab 的性能优势

**2. 设计特点与权衡**

**支持的对象大小范围**：
- 最小：8 字节
- 最大：200 字节
- 支持 8 字节对齐的多种固定大小对象

**设计特点**：
- **简化实现**：直接用 `size - MINSIZE` 作为索引，避免复杂的查表逻辑
- **固定大小策略**：每个 slab 只存储一种大小的对象，消除内部碎片
- **8 字节对齐**：所有对象大小对齐到 8 字节边界，简化地址计算

**设计权衡**：
- **优点**：
  - O(1) 分配复杂度，直接定位
  - 实现简单，无需复杂的查表或对齐计算
  - 消除内部碎片（每个 slab 只存储一种大小）
  
- **缺点**：
  - 内存效率不高（详见 §2.1 源码分析）
  - GETSLAB 宏的索引方式导致 200 个 slabheader 中仅 ~25 个被使用（§2.1 详述）

## 2. C 源码分析

### 2.0 物理页面的获取

Slab allocator 需要物理页面来存储对象，调用链如下：

**Minix3 链路**（32 位，需要 `vm_mappages` 获取 VA）：

```
slaballoc()
  └─> newslabdata()              // 分配新的 slab
       └─> vm_allocpage()        // 分配一个物理页
            └─> vm_allocpages()  // 分配物理页（支持多页）
                 ├─> alloc_mem() // 从物理内存池分配
                 └─> vm_mappages() // 映射到 VM 的虚拟地址空间
```

**Rust 版本链路**（64 位，Direct Map + HeapArena）：

```
Box::new() / Vec::push()
  └─> GlobalAlloc::alloc()
       └─> VmAllocator::alloc()
            └─> bump within arena
            └─> arena exhausted? → refill_arena()
                 └─> HeapArena::grow(pages, page_alloc)
                      ├─> alloc_phys(1) × N    // 逐页分配物理页（可碎片化）
                      └─> vm_self_mappages()    // 映射到 HeapArena 连续 VA
```

关键差异：Minix3 通过 `vm_mappages()` 获取 VA（需要 `find_hole` + 页表映射），Rust 版本通过 HeapArena 获取连续 VA（预留区间 + 逐页映射），Direct Map 仅用于物理页管理。

**newslabdata 实现**

```c
// slaballoc.c

static struct slabdata *newslabdata(void)
{
    struct slabdata *n;
    phys_bytes p;

    assert(sizeof(*n) == VM_PAGE_SIZE);

    // 分配一个物理页，返回虚拟地址和物理地址
    if(!(n = vm_allocpage(&p, VMP_SLAB))) {
        printf("newslabdata: vm_allocpage failed\n");
        return NULL;
    }

    // 初始化位图（清零表示所有槽位空闲）
    memset(n->sdh.usebits, 0, sizeof(n->sdh.usebits));

    // 跟踪已分配的 slab 页数（用于统计和调试）
    pages++;

    // 保存物理地址（仅用于 SANITYCHECKS 跟踪物理页使用）
    n->sdh.phys = p;

#if SANITYCHECKS
    // 设置魔数，用于检测内存损坏
    n->sdh.magic1 = MAGIC1;
    n->sdh.magic2 = MAGIC2;
#endif

    // 初始化使用计数和空闲猜测位置
    n->sdh.nused = 0;
    n->sdh.freeguess = 0;

#if SANITYCHECKS
    // MEMPROTECT 模式下：初始化可写标记并锁定页（设为只读）
    n->sdh.writable = WRITABLE_HEADER;
    SLABDATAUNWRITABLE(n);
#endif

    return n;
}
```

**释放流程**

```c
// slaballoc.c

void slabfree(void *mem, int bytes)
{
    // ...

    // 如果 slab 完全空闲，释放物理页
    if(f->sdh.nused == 0) {
        UNLINKNODE(f);
        vm_freepages((vir_bytes) f, 1);  // 释放物理页
    }
}
```

**总结**

- Slab allocator 通过 `vm_allocpage()` 获取物理页
- 物理页来自 VM 的物理内存池（`alloc_mem()`）
- 物理页被映射到 VM 的虚拟地址空间（`vm_mappages()`）
- 完全空闲的 slab 会被释放回物理内存池（`vm_freepages()`）

**依赖关系**

| 函数 | 功能 | 文档 |
|------|------|------|
| `vm_allocpage()` | 分配物理页并映射到 VM 地址空间 | [05-vm-allocpage.md](05-vm-allocpage.md) |
| `vm_freepages()` | 释放物理页并取消映射 | [07-pagetable-ops.md](07-pagetable-ops.md) |

### 2.1 Slab 数据结构

**Minix3 C 实现**

```c
// slaballoc.c

/* Slab 数据头 - 管理 slab 的元数据 */
struct sdh {
#if SANITYCHECKS
    u32_t magic1;           // 魔数1，用于内存完整性检查
#endif
    int freeguess;          // 猜测的空闲位置，加速分配
    struct slabdata *next;  // 下一个 slab（链表）
    struct slabdata *prev;  // 上一个 slab（链表）
    elements_t usebits;     // 使用位图，标记哪些对象被占用
    phys_bytes phys;        // 物理地址（仅用于 SANITYCHECKS 跟踪物理页使用）
#if SANITYCHECKS
    int writable;           // 可写标记（WRITABLE_NONE/WRITABLE_HEADER/对象号）
    u32_t magic2;           // 魔数2
#endif
    u16_t nused;            // 已使用对象数
};

/* Slab 数据块 - 一个物理页 */
struct slabdata {
    u8_t data[DATABYTES];   // 实际存储对象的数据区
    struct sdh sdh;         // Slab 头信息（放在末尾便于地址计算）
};

/* Slab 头 - 管理特定大小的所有 slab */
struct slabheader {
    struct slabdata *list_head;  // slab 链表头
};

/* 全局 slab 数组 - 每个元素管理一种大小的对象 */
static struct slabheader slabs[SLABSIZES];

/* 关键宏定义 */
#define DATABYTES    (VM_PAGE_SIZE - sizeof(struct sdh))  // 数据区大小
#define USEELEMENTS  (1 + (VM_PAGE_SIZE / MINSIZE / 8))   // 位图元素数
#define MINSIZE      8                                    // 最小对象大小
#define SLABSIZES    200                                  // 支持的尺寸种类
#define OBJALIGN     8                                    // 对象对齐边界

/* 每页可容纳的对象数 */
#define ITEMSPERPAGE(bytes) (int)(DATABYTES / (bytes))
```

**内存布局**

```
┌─────────────────────────────────────────────────────────────┐
│                     物理页 (4KB)                             │
├─────────────────────────────────────────────────────────────┤
│  ← 低地址                                                    │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                    数据区 (DATA)                     │   │
│  │  ┌─────────┐ ┌─────────┐ ┌─────────┐     ┌────────┐ │   │
│  │  │ Object 0│ │ Object 1│ │ Object 2│ ... │Object N│ │   │
│  │  │ 8 bytes │ │ 8 bytes │ │ 8 bytes │     │8 bytes │ │   │
│  │  └─────────┘ └─────────┘ └─────────┘     └────────┘ │   │
│  │                                                     │   │
│  │  每个对象大小相同，通过位图标记使用状态              │   │
│  │                                                     │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              Slab 头 (struct sdh)                   │   │
│  │  ┌─────────────┐                                    │   │
│  │  │ freeguess   │  猜测的空闲位置（启发式优化）      │   │
│  │  ├─────────────┤                                    │   │
│  │  │ next        │  链表指针（下一个 slab）           │   │
│  │  ├─────────────┤                                    │   │
│  │  │ prev        │  链表指针（上一个 slab）           │   │
│  │  ├─────────────┤                                    │   │
│  │  │ usebits[]   │  位图：0=空闲, 1=已使用           │   │
│  │  ├─────────────┤                                    │   │
│  │  │ phys        │  物理地址                          │   │
│  │  ├─────────────┤                                    │   │
│  │  │ nused       │  已使用对象计数                    │   │
│  │  └─────────────┘                                    │   │
│  └─────────────────────────────────────────────────────┘   │
│  ← 高地址                                                    │
└─────────────────────────────────────────────────────────────┘
```

**sdh_usebits 位图详解**

```c
// 位图用于标记对象的使用状态
typedef u8_t element_t;     // 每个元素 8 位
typedef element_t elements_t[USEELEMENTS];  // 位图数组

// 计算对象索引对应的位位置
#define ELBITS      (sizeof(element_t) * 8)  // 8 bits
#define BITPAT(b)   (1UL << ((b) % ELBITS))  // 位掩码
#define BITEL(f, b) ((f)->sdh.usebits[(b) / ELBITS])  // 元素索引

// 位操作宏
#define GETBIT(f, b)   (BITEL(f, b) & BITPAT(b))    // 获取位状态

// 设置位：OFF 断言位当前为 0（前置条件），然后设置为 1
#define SETBIT(f, b)   { OFF(f,b); SLABDATAUSE(f, BITEL(f,b)|= BITPAT(b); (f)->sdh.nused++;); }

// 清除位：ON 断言位当前为 1（前置条件），然后清除为 0，同时更新 freeguess
#define CLEARBIT(f, b) { ON(f, b); SLABDATAUSE(f, BITEL(f,b)&=~BITPAT(b); (f)->sdh.nused--; (f)->sdh.freeguess = (b);); }

// 注：SLABDATAUSE 在非 MEMPROTECT 编译时无页保护代码 wrapper，直接执行 code 语句
// OFF(f,b) 状态断言：位当前为 0；ON(f,b) 状态断言：位当前为 1
```

**Sanity Check 机制**

```c
// 调试魔数定义
#define MAGIC1 0x1f5b842f  // 头部魔数
#define MAGIC2 0x8bb5a420  // 尾部魔数
#define JUNK   0xdeadbeef  // 已释放对象的标记
#define NOJUNK 0xc0ffee    // 新分配对象的标记（coffee 的 hex 拼写）

// Sanity check 级别
#define SCL_FUNCTIONS  2   // 函数入口/出口检查
#define SCL_DETAIL     3   // 详细检查（遍历链表等）

// 主 sanity check 宏
#define SLABSANITYCHECK(l) if(_minix_kerninfo) { \
    slab_sanitycheck(__FILE__, __LINE__); \
}
```

**魔数的作用**：

| 魔数 | 值 | 用途 |
|------|-----|------|
| **MAGIC1** | `0x1f5b842f` | 检测 slab 头是否被意外覆盖 |
| **MAGIC2** | `0x8bb5a420` | 双重校验内存完整性 |
| **JUNK** | `0xdeadbeef` | 标记已释放的对象（检测 use-after-free） |
| **NOJUNK** | `0xc0ffee` | 标记新分配的对象（区分未初始化和已释放） |

**`nojunkwarning` 变量**：

```c
#if SANITYCHECKS
// 用于抑制 JUNK 警告的计数器
// 在 MEMPROTECT 解锁/锁定期间，对象会临时包含 JUNK
// 需要抑制警告避免误报
static int nojunkwarning = 0;
#endif
```

**MEMPROTECT：虚拟页保护机制**

MEMPROTECT 不是物理页保护，而是 **VM 进程虚拟地址空间的页保护**：

```c
#if MEMPROTECT
#define SLABDATAWRITABLE(data, wr) do {    \
    vm_pagelock(data, 0);  /* 解锁：设置为可写 */ \
    data->sdh.writable = wr;               \
} while(0)

#define SLABDATAUNWRITABLE(data) do {      \
    vm_pagelock(data, 1);  /* 锁定：设置为只读 */ \
    data->sdh.writable = WRITABLE_NONE;    \
} while(0)

#define SLABDATAUSE(data, code) do {       \
    SLABDATAWRITABLE(data, WRITABLE_HEADER);  /* 解锁 */ \
    code                                   /* 修改元数据 */ \
    SLABDATAUNWRITABLE(data);              /* 锁定 */ \
} while(0)
#else
#define SLABDATAUSE(data, code) do { code } while(0)  /* 无保护 */
#endif
```

**工作原理**：

1. **默认只读**：slab 页面默认设置为只读（`vm_pagelock(data, 1)`）
2. **临时解锁**：需要修改元数据时，临时设置为可写（`vm_pagelock(data, 0)`）
3. **修改后锁定**：修改完成后，再次设置为只读

**vm_pagelock 实现**（[pagetable.c:403](minix3/minix/servers/vm/pagetable.c#L403)）：

```c
void vm_pagelock(void *vir, int lockflag)
{
    vir_bytes m = (vir_bytes) vir;
    int r;
    u32_t flags = ARCH_VM_PTE_PRESENT | ARCH_VM_PTE_USER;
    pt_t *pt;

    pt = &vmprocess->vm_pt;

    assert(!(m % VM_PAGE_SIZE));

    if(!lockflag)
        flags |= ARCH_VM_PTE_RW;
#if defined(__arm__)
    else
        flags |= ARCH_VM_PTE_RO;

    flags |= ARM_VM_PTE_CACHED;
#endif

    /* Update flags. */
    if((r=pt_writemap(vmprocess, pt, m, 0, VM_PAGE_SIZE,
        flags, WMF_OVERWRITE | WMF_WRITEFLAGSONLY)) != OK) {
        panic("vm_lockpage: pt_writemap failed");
    }

    if((r=sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)) != OK) {
        panic("VMCTL_FLUSHTLB failed: %d", r);
    }

    return;
}
```

**关键细节**：
- `pt_writemap` 的 `physaddr` 参数为 `0`，配合 `WMF_WRITEFLAGSONLY` 表示只更新 flags 不修改物理地址
- ARM 架构有额外的 `ARCH_VM_PTE_RO` 和 `ARM_VM_PTE_CACHED` 标志
- 每次调用都刷新 TLB（`VMCTL_FLUSHTLB`），这是 MEMPROTECT 性能开销大的主要原因

**为什么需要这个机制？**

| 目的 | 说明 |
|-----|------|
| **防止野指针** | 错误写入 slab 元数据会触发 page fault |
| **检测内存损坏** | 任何修改都必须显式解锁 |
| **调试辅助** | 帮助发现内存相关的 bug |

**类比**：类似 Linux 的 `mprotect()` 系统调用

```c
// Linux
mprotect(addr, size, PROT_READ);              // 只读
mprotect(addr, size, PROT_READ|PROT_WRITE);   // 可写

// Minix3 VM
vm_pagelock(data, 1);  // 只读
vm_pagelock(data, 0);  // 可写
```

**注意**：这是 VM 进程自己的虚拟地址空间保护，不是物理页保护。

**性能开销分析**

MEMPROTECT 的性能开销非常大：

| 操作 | 开销 | 说明 |
|-----|------|------|
| **系统调用** | 高 | `sys_vmctl(SELF, VMCTL_FLUSHTLB, 0)` 需要陷入内核 |
| **TLB 刷新** | 极高 | `reload_cr3()` 清空整个 TLB |
| **TLB miss** | 高 | 后续所有内存访问都会 TLB miss，直到 TLB 重新填充 |

**reload_cr3() 的实现**（x86 架构）：

```asm
ENTRY(reload_cr3)
    mov	%cr3, %eax
    mov	%eax, %cr3      // 重新加载 CR3，清空整个 TLB！
    ret
```

**为什么这么昂贵？**

1. **每次修改 slab 元数据**都需要解锁/锁定页面
2. **每次锁定**都需要系统调用 + TLB 刷新
3. **TLB 刷新**清空整个 TLB，导致后续所有内存访问都 TLB miss

**Minix3 有更好的选项但没用**：

```c
case VMCTL_I386_INVLPG:
    i386_invlpg(m_ptr->SVMCTL_VALUE);  // 只刷新单个页的 TLB entry
    return OK;
```

但 VM 的 slab 代码使用了更昂贵的 `VMCTL_FLUSHTLB`。

**这就是为什么 MEMPROTECT 默认关闭**：

```c
#if MEMPROTECT
    // 页保护代码（仅用于调试）
#else
#define SLABDATAUSE(data, code) do { code } while(0)  // 生产环境：无保护
#endif
```

**示例：8 字节对象的 slab**

```
对象大小：8 bytes
页大小：4096 bytes
Slab 头大小：~64 bytes
数据区大小：4032 bytes
可存储对象数：4032 / 8 = 504 个

位图大小：504 / 8 = 63 bytes = 63 个 u8_t（最小需求；实际 `usebits` 是固定 65 个 `element_t` 的数组，见 `USEELEMENTS` 宏）

usebits[0] 的 8 位对应 Object 0-7
usebits[1] 的 8 位对应 Object 8-15
...
usebits[62] 的 8 位对应 Object 496-503
```

**phys 字段的作用**

`phys_bytes phys` 字段存储 slab 所在物理页的物理地址，但**仅用于 SANITYCHECKS 调试模式**：

```c
// 设置物理地址（slaballoc.c:173）
n->sdh.phys = p;  // p 来自 vm_allocpage()

// 仅在 sanity check 中使用（slaballoc.c:206）
MYASSERT(usedpages_add(n->sdh.phys, VM_PAGE_SIZE) == OK);
```

**为什么需要物理地址？**
- 用于跟踪物理页的使用情况
- 检测重复分配或内存泄漏
- 仅在调试模式下有用

**是否必要？**
- 对于 slab 的正常功能，**虚拟地址足够**
- 如果禁用 SANITYCHECKS，这个字段浪费 8 字节

**slabs 数组索引计算**

```c
// 根据请求大小找到对应的 slabheader
#define GETSLAB(bytes, slabhdr) {           \
    int idx = (bytes) - MINSIZE;            \
    assert(idx >= 0 && idx < SLABSIZES);    \
    slabhdr = &slabs[idx];                  \
}

// 示例（bytes 已经过 roundup(bytes, OBJALIGN) 对齐）：
// 请求 8 bytes  -> bytes_aligned=8  -> idx = 8-8  = 0  -> slabs[0]
// 请求 16 bytes -> bytes_aligned=16 -> idx = 16-8 = 8  -> slabs[8]
// 请求 64 bytes -> bytes_aligned=64 -> idx = 64-8 = 56 -> slabs[56]
```

**链表操作宏**

```c
// 将新 slab 添加到链表头部（SLABDATAUSE 在 MEMPROTECT 时解锁页保护）
#define ADDHEAD(new_slab, slabhdr) {                    \
    SLABDATAUSE(new_slab,                               \
        (new_slab)->sdh.next = (slabhdr)->list_head;    \
        (new_slab)->sdh.prev = NULL;);                  \
    (slabhdr)->list_head = new_slab;                    \
    if ((new_slab)->sdh.next) {                         \
        SLABDATAUSE((new_slab)->sdh.next,               \
            (new_slab)->sdh.next->sdh.prev = new_slab;); \
    }                                                   \
}

// 从链表中移除节点
#define UNLINKNODE(node)	{\
	struct slabdata *next, *prev;\
	prev = (node)->sdh.prev;\
	next = (node)->sdh.next;\
	if(prev) { SLABDATAUSE(prev, prev->sdh.next = next;); }\
	if(next) { SLABDATAUSE(next, next->sdh.prev = prev;); }\
}
```

**大小策略的详细分析**

Minix3 VM 的 slab 分配器定义了 **200 个 slabheader**，但实际使用情况需要仔细分析：

```c
#define SLABSIZES 200           // 定义 200 个 slabheader
#define MINSIZE 8               // 最小 8 字节
#define MAXSIZE (SLABSIZES-1+MINSIZE)  // 207（但这个定义有误导性）

// 实际的索引计算（在 slaballoc 中）：
bytes = roundup(bytes, OBJALIGN);  // 先对齐到 8 字节边界
index = bytes - MINSIZE;            // 再计算索引
```

**关键：对齐导致大量 slabheader 未被使用**

由于 `roundup(bytes, 8)` 的存在，实际使用的 slabheader 只有约 25 个：

| 请求大小 | 对齐后 | 索引 | 使用的 slabheader |
|---------|--------|------|------------------|
| 8 字节 | 8 | 0 | `slabs[0]` ✓ |
| 9~16 字节 | 16 | 8 | `slabs[8]` ✓ |
| 17~24 字节 | 24 | 16 | `slabs[16]` ✓ |
| 25~32 字节 | 32 | 24 | `slabs[24]` ✓ |
| ... | ... | ... | ... |
| 193~200 字节 | 200 | 192 | `slabs[192]` ✓ |

**未被使用的 slabheader**：`slabs[1-7]`, `slabs[9-15]`, `slabs[17-23]`, ... 共约 175 个在 8 字节对齐下永远不会被直接索引到。

**设计分析**：

1. **slabs 数组大小**：`SLABSIZES = 200`，覆盖 `MINSIZE(8)` 到 `MAXSIZE(207)` 的范围
2. **MAXSIZE 定义**：`#define MAXSIZE (SLABSIZES-1+MINSIZE)` = 207，但实际最大对齐后大小为 200（`roundup(200, 8) = 200`）。请求 201~207 字节会被对齐到 208，超出 `slabs[199]` 的范围
3. **索引计算**：`GETSLAB` 宏中 `_gsi = (b) - MINSIZE`，其中 `b` 已经过 `roundup(bytes, OBJALIGN)` 处理。因此索引总是 8 的倍数

**代码证据**（`slaballoc.c:267,133`）：

```c
// slaballoc.c:267
bytes = roundup(bytes, OBJALIGN);  // 对齐到 8 字节

// slaballoc.c:133 (GETSLAB 宏)
_gsi = (b) - MINSIZE;  // 索引 = bytes - 8

// 推导：
// 请求 8 字节  → roundup(8, 8) = 8   → index = 0  → slabs[0]
// 请求 9 字节  → roundup(9, 8) = 16  → index = 8  → slabs[8]
// 请求 16 字节 → roundup(16, 8) = 16 → index = 8  → slabs[8]
// 请求 17 字节 → roundup(17, 8) = 24 → index = 16 → slabs[16]
// ...
// 所以 slabs[1-7], slabs[9-15], slabs[17-23] 等不会被 GETSLAB 访问
```

**设计意图推测**：

Minix3 的设计者可能预留了非 8 字节对齐的使用场景（如未来支持 4 字节对齐），或者为了简化索引计算而接受数组稀疏。`slabs` 数组仅 200 个指针（`struct slabheader` 只有一个 `list_head` 指针，约 1600 字节），空间开销极小，不构成实际问题。

### 2.1 内存分配

#### 2.1.1 slaballoc - 分配对象

**源码位置**: [slaballoc.c:259](minix3/minix/servers/vm/slaballoc.c#L259)

```c
void *slaballoc(int bytes)
{
    int i;
    int count = 0;
    struct slabheader *s;
    struct slabdata *newslab;
    char *ret;

    // 1. 对齐请求大小到 8 字节边界
    bytes = roundup(bytes, OBJALIGN);

    // 2. 执行 sanity check（仅在 SANITYCHECKS 模式下）
    SLABSANITYCHECK(SCL_FUNCTIONS);

    // 3. 根据大小找到对应的 slabheader
    GETSLAB(bytes, s);
    assert(s);

    // 4. 检查是否有可用的 slab
    if(!(newslab = s->list_head)) {
        // 没有可用 slab，分配新的物理页
        newslab = newslabdata();
        if(!newslab) return NULL;
        ADDHEAD(newslab, s);
        assert(newslab->sdh.nused == 0);
    } else {
        assert(newslab->sdh.nused > 0);
    }
    assert(newslab->sdh.nused < ITEMSPERPAGE(bytes));

    // 5. 执行详细 sanity check
    SLABSANITYCHECK(SCL_DETAIL);

#if SANITYCHECKS
    // 验证魔数（检测内存损坏）
    assert(newslab->sdh.magic1 == MAGIC1);
    assert(newslab->sdh.magic2 == MAGIC2);
#endif

    // 6. 从 freeguess 开始查找空闲槽位
    for(i = newslab->sdh.freeguess;
        count < ITEMSPERPAGE(bytes); count++, i++) {
        i = i % ITEMSPERPAGE(bytes);  // 循环查找

        if(!GETBIT(newslab, i))  // 找到空闲位
            break;
    }

    assert(count < ITEMSPERPAGE(bytes));
    assert(i >= 0 && i < ITEMSPERPAGE(bytes));

    // 7. 标记该槽位为已使用
    SETBIT(newslab, i);

    // 8. 如果 slab 已满，从链表中移除
    // 注：满的 slab 从链表移除后，仍可通过地址定位（详见释放流程）
    if(newslab->sdh.nused == ITEMSPERPAGE(bytes)) {
        UNLINKNODE(newslab);
        s->list_head = newslab->sdh.next;
    }

    // 9. 计算返回地址
    ret = ((char *) newslab) + i*bytes;

    // 10. 执行 sanity check
    SLABSANITYCHECK(SCL_FUNCTIONS);

#if SANITYCHECKS
    // MEMPROTECT 模式下：临时解锁页以初始化对象
#if MEMPROTECT
    nojunkwarning++;  // 抑制 JUNK 警告（新分配的对象不应有 JUNK）
    slabunlock(ret, bytes);
    nojunkwarning--;
    assert(!nojunkwarning);
#endif
    // 设置 NOJUNK 魔数（表示对象已分配，不是释放状态）
    *(u32_t *) ret = NOJUNK;
#if MEMPROTECT
    // 重新锁定页（设为只读）
    slablock(ret, bytes);
#endif
#endif

    // 11. 更新 freeguess 为下一个位置（启发式优化）
    SLABDATAUSE(newslab, newslab->sdh.freeguess = i+1;);

#if SANITYCHECKS
    // 检查异常大的请求
    if(bytes >= SLABSIZES+MINSIZE) {
        printf("slaballoc: odd, bytes %d?\n", bytes);
    }

    // 验证分配的指针合法性
    if(!slabsane_f(__FILE__, __LINE__, ret, bytes))
        panic("slaballoc: slabsane failed");
#endif

    // 12. 验证返回地址对齐
    assert(!((vir_bytes) ret % OBJALIGN));

    return ret;
}
```

**分配流程**

```
slaballoc(64) 请求分配 64 字节对象
│
├─► 1. 对齐大小: roundup(64, 8) = 64
│
├─► 2. 查找 slabheader: GETSLAB(64, s) → slabs[56]
│
├─► 3. 检查可用 slab
│   │
│   ├─ 有可用 slab (list_head != NULL)
│   │   └─ 使用链表头部的 slab
│   │
│   └─ 无可用 slab
│       ├─ 调用 newslabdata() 分配物理页
│       ├─ 初始化 slab 头
│       └─ ADDHEAD() 添加到链表
│
├─► 4. 查找空闲槽位
│   │
│   ├─ 从 freeguess 位置开始查找
│   ├─ 使用 GETBIT() 检查位图
│   └─ 循环直到找到空闲位或遍历完所有槽位
│
├─► 5. 标记已使用: SETBIT(slab, index)
│
├─► 6. 检查 slab 是否已满
│   │
│   └─ nused == ITEMSPERPAGE(bytes)
│       └─ UNLINKNODE() 从链表移除（已满 slab 不再参与分配）
│
├─► 7. 计算对象地址: slab_base + index * bytes
│
└─► 8. 更新 freeguess = index + 1
```

**关键算法：空闲槽位查找**

```c
// 启发式查找：从上次分配位置的下一个位置开始
// 利用局部性原理，减少缓存未命中

for(i = newslab->sdh.freeguess;           // 从猜测位置开始
    count < ITEMSPERPAGE(bytes);           // 最多遍历所有槽位
    count++, i++) {

    i = i % ITEMSPERPAGE(bytes);           // 循环回绕

    if(!GETBIT(newslab, i))                // 检查位图
        break;                             // 找到空闲槽位
}
```

**优化策略**

| 优化点 | 实现 | 效果 |
|-------|------|------|
| **大小对齐** | `roundup(bytes, 8)` | 减少碎片，简化索引计算 |
| **freeguess** | 记录上次分配位置 | 利用局部性，平均 O(1) 查找 |
| **循环查找** | `i % ITEMSPERPAGE` | 避免每次都从 0 开始 |
| **满 slab 移出** | 已满 slab 从链表移除 | 减少无效遍历 |

**地址计算**

```
对象地址 = Slab 基地址 + 索引 × 对象大小

示例：
  Slab 基地址: 0x100000
  对象大小: 64 bytes
  分配索引: 5
  
  对象地址 = 0x100000 + 5 × 64
           = 0x100000 + 320
           = 0x100140
```

**错误处理**

```c
// 物理页分配失败
if(!newslab) return NULL;

// 断言检查（调试模式）
assert(newslab->sdh.nused < ITEMSPERPAGE(bytes));  // slab 未满
assert(count < ITEMSPERPAGE(bytes));                // 必能找到空闲位
assert(i >= 0 && i < ITEMSPERPAGE(bytes));          // 索引合法
```

#### 2.1.2 SLABALLOC 宏

**Minix3 C 实现**

```c
// proto.h

/* SLABALLOC 宏 - 类型安全的 slab 分配 */
#define SLABALLOC(var) (var = slaballoc(sizeof(*var)))
```

**设计目的**

SLABALLOC 宏解决了 C 语言中手动调用 `slaballoc` 时的两个常见问题：

1. **类型不匹配**：手动计算 `sizeof(type)` 容易出错
2. **重复代码**：每次分配都需要写 `var = slaballoc(sizeof(*var))`

**使用示例**

```c
// 传统方式（容易出错）
struct vir_region *vr;
vr = slaballoc(sizeof(struct vir_region));  // 如果写错类型，编译器不报错

// 使用 SLABALLOC 宏（类型安全）
struct vir_region *vr;
if(!(SLABALLOC(vr))) {  // 自动推导 sizeof(*vr)
    printf("VM: alloc region failed\n");
    return NULL;
}

// 其他使用场景
struct phys_region *pr;
if(!(SLABALLOC(pr))) {
    return NULL;
}

struct fdref *fd;
if(!(SLABALLOC(fd))) {
    return NULL;
}
```

**宏展开分析**

```c
SLABALLOC(vr)
// 展开为:
(vr = slaballoc(sizeof(*vr)))

// 如果 vr 是 struct vir_region* 类型
// sizeof(*vr) 自动计算为 sizeof(struct vir_region)
// 无需手动指定类型，避免类型不匹配错误
```

### 2.2 内存释放

#### 关键设计：通过地址定位 slab

**问题**：分配时，满的 slab 会从链表移除，那释放时如何找到这个 slab？

**答案**：通过地址计算直接定位，不需要链表！

**原理**：slab 是完整的物理页（4KB），可以通过地址对齐直接定位：

```c
// 给定对象地址 ptr，计算其所在的 slab
slab = (struct slabdata *) ((char *) ptr - (vir_bytes) ptr % VM_PAGE_SIZE);

// 示例：
// ptr = 0x1040
// slab = 0x1040 - (0x1040 % 0x1000) = 0x1040 - 0x40 = 0x1000
```

**内存布局示例**：

```
内存地址空间：
┌─────────────────────────────────────────────────────────────┐
│  Slab 1 (页 1)    │  Slab 2 (页 2)    │  Slab 3 (页 3)    │
│  0x1000-0x1FFF    │  0x2000-0x2FFF    │  0x3000-0x3FFF    │
├───────────────────┼───────────────────┼───────────────────┤
│  Object 0         │  Object 0         │  Object 0         │
│  0x1000           │  0x2000           │  0x3000           │
│  Object 1         │  Object 1         │  Object 1         │
│  0x1040           │  0x2040           │  0x3040           │
│  ...              │  ...              │  ...              │
│  Slab Header      │  Slab Header      │  Slab Header      │
│  0x1FC0-0x1FFF    │  0x2FC0-0x2FFF    │  0x3FC0-0x3FFF    │
└───────────────────┴───────────────────┴───────────────────┘

释放对象 0x1040：
1. 向下对齐到页边界：0x1040 -> 0x1000（找到 Slab 1）
2. 计算索引：(0x1040 - 0x1000) / 64 = 1（Object 1）
```

**链表的作用**：
- **分配时**：快速找到有空闲槽位的 slab
- **释放时**：不需要链表，直接通过地址定位

**生命周期**：

```
1. 分配对象：
   - 从链表头部的 slab 分配
   - 如果 slab 满了，从链表移除（但仍可通过地址定位）

2. 释放对象：
   - 通过地址计算找到 slab（不需要链表！）
   - 清除位图中的对应位
   - 如果 slab 从满变成部分空闲，重新加入链表
   - 如果 slab 完全空闲，释放整个页面
```

#### 2.2.1 slabfree - 释放对象

**源码位置**: [slaballoc.c:406](minix3/minix/servers/vm/slaballoc.c#L406)

```c
void slabfree(void *mem, int bytes)
{
    int i;
    struct slabheader *s;
    struct slabdata *f;

    // 1. 对齐大小
    bytes = roundup(bytes, OBJALIGN);

    // 2. 执行 sanity check
    SLABSANITYCHECK(SCL_FUNCTIONS);

    // 3. 获取对象统计信息（验证指针合法性）
    if(objstats(mem, bytes, &s, &f, &i) != OK) {
        panic("slabfree objstats failed");
    }

#if SANITYCHECKS
    // 4. 检查重复释放（如果对象已包含 JUNK 魔数）
    if(*(u32_t *) mem == JUNK) {
        printf("VM: WARNING: likely double free, JUNK seen\n");
    }
#endif

#if SANITYCHECKS
    // 5. MEMPROTECT 模式下：解锁页以允许写入
#if MEMPROTECT
    slabunlock(mem, bytes);
#endif
    // 填充垃圾数据（帮助发现 use-after-free）
#if JUNKFREE
    memset(mem, 0xa6, bytes);
#endif
    // 设置 JUNK 魔数（标记对象已释放）
    *(u32_t *) mem = JUNK;

    // 抑制 JUNK 警告（避免后续操作触发误报）
    nojunkwarning++;
#if MEMPROTECT
    // 重新锁定页
    slablock(mem, bytes);
#endif
    nojunkwarning--;
    assert(!nojunkwarning);
#endif

    // 6. 清除位图标记
    CLEARBIT(f, i);

    // 7. 处理 slab 状态变化
    if(f->sdh.nused == 0) {
        // 7a. slab 完全空闲，释放物理页
        UNLINKNODE(f);
        if(f == s->list_head) s->list_head = f->sdh.next;
        vm_freepages((vir_bytes) f, 1);
        SLABSANITYCHECK(SCL_DETAIL);
    } else if(f->sdh.nused == ITEMSPERPAGE(bytes)-1) {
        // 7b. slab 从满变为有空间，加入可用链表
        ADDHEAD(f, s);
    }

    // 8. 执行 sanity check
    SLABSANITYCHECK(SCL_FUNCTIONS);

    return;
}
```

**释放流程**

```
slabfree(ptr, 64) 释放 64 字节对象
│
├─► 1. 对齐大小: roundup(64, 8) = 64
│
├─► 2. 验证指针合法性 (objstats)
│   │
│   ├─ 计算 slab 基地址（页对齐）
│   ├─ 计算对象索引: (ptr - slab_base) / bytes
│   ├─ 验证索引在范围内
│   ├─ 验证位图标记为已分配
│   └─ 返回 (slabheader, slabdata, index)
│
├─► 3. 调试检查 (SANITYCHECKS)
│   │
│   ├─ 检查 JUNK 魔数（防止重复释放）
│   ├─ 填充 0xa6（脏数据模式）
│   └─ 设置 JUNK 魔数
│
├─► 4. 清除位图: CLEARBIT(slab, index)
│   └─ nused 减 1
│
├─► 5. 处理 slab 状态
│   │
│   ├─ nused == 0（完全空闲）
│   │   ├─ UNLINKNODE() 从链表移除
│   │   └─ vm_freepages() 释放物理页
│   │
│   ├─ nused == ITEMSPERPAGE-1（从满变有空闲）
│   │   └─ ADDHEAD() 加入可用链表头部
│   │
│   └─ 其他情况（中间状态）
│       └─ 无需操作，仍在链表中
│
└─► 6. 返回
```

**objstats 函数详解**

**源码位置**: [slaballoc.c:344](minix3/minix/servers/vm/slaballoc.c#L344)

```c
// 验证对象指针并返回统计信息
static inline int objstats(void *mem, int bytes,
    struct slabheader **sp, struct slabdata **fp, int *ip)
{
    struct slabheader *s;
    struct slabdata *f;
    int i;

    // 1. 获取 slabheader
    GETSLAB(bytes, s);

    // 2. 计算 slabdata 基地址（页对齐）
    f = (struct slabdata *) ((char *) mem - (vir_bytes) mem % VM_PAGE_SIZE);

    // 3. 验证对象在数据区内
    assert((char *) mem >= (char *) f->data);
    assert((char *) mem < (char *) f->data + sizeof(f->data));

    // 4. 计算索引
    i = (char *) mem - (char *) f->data;
    assert(!(i % bytes));  // 必须对齐
    i = i / bytes;

    // 5. 验证已分配
    assert(GETBIT(f, i));

    // 6. 返回结果
    *ip = i;
    *fp = f;
    *sp = s;
    return OK;
}
```

**Slab 状态转换**

```
状态转换图:

┌─────────────┐    分配所有对象    ┌─────────────┐
│   部分使用   │ ────────────────► │    已满     │
│ (在链表中)   │                   │ (不在链表)  │
└──────┬──────┘                   └──────┬──────┘
       │                                  │
       │ 释放对象                         │ 释放对象
       │ (仍有对象)                       │ (变为部分使用)
       │                                  │
       ▼                                  ▼
┌─────────────┐    释放所有对象    ┌─────────────┐
│   完全空闲   │ ◄───────────────  │   部分使用   │
│ (释放物理页) │                   │ (加入链表)   │
└─────────────┘                   └─────────────┘
```

**空闲页合并策略**

```c
// Minix3 采用即时释放策略
// 当 slab 完全空闲时，立即释放物理页

if(f->sdh.nused == 0) {
    // 从链表移除
    UNLINKNODE(f);
    if(f == s->list_head) s->list_head = f->sdh.next;

    // 释放物理页
    vm_freepages((vir_bytes) f, 1);
}

// 优点：
// 1. 内存及时回收，避免浪费
// 2. 实现简单，无需延迟释放逻辑
// 3. 与 vm_allocpage/vm_freepages 对称

// 缺点：
// 1. 频繁分配/释放可能导致页抖动
// 2. 没有保留空闲 slab 作为缓存
```

**调试功能（SANITYCHECKS）**

```c
#if SANITYCHECKS
    // 1. 重复释放检测
    if(*(u32_t *) mem == JUNK) {
        printf("VM: WARNING: likely double free\n");
    }

    // 2. 填充脏数据（帮助发现 use-after-free）
    memset(mem, 0xa6, bytes);

    // 3. 设置 JUNK 魔数
    *(u32_t *) mem = JUNK;  // 0xdeadbeef
#endif

// 内存布局示例（释放后）：
// ┌─────────────────────────────────────┐
// │ a6 a6 a6 a6 ef be ad de a6 a6 a6 ... │  <- 填充 0xa6 和 JUNK 魔数
// └─────────────────────────────────────┘
```

#### 2.2.2 SLABFREE 宏

**Minix3 C 实现**

```c
// proto.h

/* SLABFREE 宏 - 类型安全的 slab 释放 */
#define SLABFREE(ptr) do { \
    slabfree(ptr, sizeof(*(ptr))); \
    (ptr) = NULL; \
} while(0)
```

**设计目的**

SLABFREE 宏解决了 C 语言中手动调用 `slabfree` 时的三个常见问题：

1. **类型不匹配**：手动计算 `sizeof(type)` 容易出错
2. **悬空指针**：释放后忘记将指针置为 NULL
3. **重复代码**：每次释放都需要写两行代码

**使用示例**

```c
// 传统方式（容易出错）
struct vir_region *vr = ...;
slabfree(vr, sizeof(struct vir_region));  // 可能写错类型
// vr 仍然是悬空指针！

// 使用 SLABFREE 宏（类型安全）
struct vir_region *vr = ...;
SLABFREE(vr);  // 自动推导 sizeof(*vr) 并置 NULL
// vr 现在是 NULL，不会悬空

// 其他使用场景
struct phys_region *pr = ...;
SLABFREE(pr);  // 释放并置 NULL

struct fdref *fd = ...;
SLABFREE(fd);  // 释放并置 NULL
```

**宏展开分析**

```c
SLABFREE(vr)
// 展开为:
do {
    slabfree(vr, sizeof(*(vr)));
    (vr) = NULL;
} while(0)

// 关键点：
// 1. sizeof(*(vr)) 自动推导类型大小
// 2. (vr) = NULL 防止悬空指针
// 3. do { ... } while(0) 确保宏在 if-else 中正确工作
```

**do { ... } while(0) 技巧**

```c
// 问题：没有 do-while 的宏在 if-else 中会出错
#define BAD_FREE(ptr) slabfree(ptr, sizeof(*(ptr))); (ptr) = NULL;

if (condition)
    BAD_FREE(vr);  // 展开为: slabfree(vr, ...); (vr) = NULL;
else              // 错误！else 前有多条语句
    ...

// 解决：使用 do-while(0) 包裹
#define SLABFREE(ptr) do { \
    slabfree(ptr, sizeof(*(ptr))); \
    (ptr) = NULL; \
} while(0)

if (condition)
    SLABFREE(vr);  // 展开为: do { ... } while(0);
else              // 正确！单条语句
    ...
```

### 2.3 内存保护

**设计目的**

MEMPROTECT 是 Minix3 Slab 分配器的可选调试功能，用于检测内存损坏问题：

1. **use-after-free**：访问已释放的对象
2. **越界写入**：写入超出对象边界的内存
3. **数据竞争**：并发访问 slab 元数据

**核心机制**

通过 `vm_pagelock()` 系统调用控制物理页的读写权限：

```c
// 锁定页（只读）
vm_pagelock(addr, 1);

// 解锁页（可读写）
vm_pagelock(addr, 0);
```

**Minix3 C 实现**

```c
// slaballoc.c

#if MEMPROTECT

/* 可写性标记值 */
#define WRITABLE_NONE   -2  // 页被锁定，不可写
#define WRITABLE_HEADER -1  // 只有 slab 头可写

/* 使 slab 可写 */
#define SLABDATAWRITABLE(data, wr) do {         \
    assert(data->sdh.writable == WRITABLE_NONE); \
    assert(wr != WRITABLE_NONE);                \
    vm_pagelock(data, 0);       /* 解锁页 */    \
    data->sdh.writable = wr;    /* 设置标记 */  \
} while(0)

/* 锁定 slab（只读） */
#define SLABDATAUNWRITABLE(data) do {           \
    assert(data->sdh.writable != WRITABLE_NONE); \
    data->sdh.writable = WRITABLE_NONE;         \
    vm_pagelock(data, 1);       /* 锁定页 */    \
} while(0)

/* 安全操作封装：临时解锁 -> 执行代码 -> 重新锁定 */
#define SLABDATAUSE(data, code) do {            \
    SLABDATAWRITABLE(data, WRITABLE_HEADER);    \
    code                                        \
    SLABDATAUNWRITABLE(data);                   \
} while(0)

#else
/* MEMPROTECT 禁用时，宏为空操作 */
#define SLABDATAWRITABLE(data, wr)
#define SLABDATAUNWRITABLE(data)
#define SLABDATAUSE(data, code) do { code } while(0)
#endif
```

**保护策略**

```
正常状态（分配/释放之间）：
┌─────────────────────────────────────┐
│           Slab 页                   │
│  ┌─────────────────────────────┐   │
│  │        数据区                │   │
│  │  ┌─────┐ ┌─────┐ ┌─────┐   │   │
│  │  │ Obj │ │ Obj │ │ Obj │   │   │
│  │  │ RO  │ │ RO  │ │ RO  │   │   │  <- 只读（受保护）
│  │  └─────┘ └─────┘ └─────┘   │   │
│  └─────────────────────────────┘   │
│  ┌─────────────────────────────┐   │
│  │        Slab 头              │   │
│  │  next, prev, usebits...     │   │
│  │  writable = WRITABLE_NONE   │   │  <- 页被锁定
│  └─────────────────────────────┘   │
└─────────────────────────────────────┘
         ↓ vm_pagelock(addr, 1)

操作期间（临时解锁）：
┌─────────────────────────────────────┐
│           Slab 页                   │
│  ┌─────────────────────────────┐   │
│  │        数据区                │   │
│  │  ┌─────┐ ┌─────┐ ┌─────┐   │   │
│  │  │ Obj │ │ Obj │ │ Obj │   │   │
│  │  │ RW  │ │ RW  │ │ RW  │   │   │  <- 可读写
│  │  └─────┘ └─────┘ └─────┘   │   │
│  └─────────────────────────────┘   │
│  ┌─────────────────────────────┐   │
│  │        Slab 头              │   │
│  │  writable = WRITABLE_HEADER │   │  <- 页已解锁
│  └─────────────────────────────┘   │
└─────────────────────────────────────┘
         ↓ vm_pagelock(addr, 0)
```

**页锁定/解锁函数**

**源码位置**: `slablock` 在 [slaballoc.c:464](minix3/minix/servers/vm/slaballoc.c#L464)，`slabunlock` 在 [slaballoc.c:483](minix3/minix/servers/vm/slaballoc.c#L483)

```c
#if MEMPROTECT

/* 锁定对象（设为只读） */
void slablock(void *mem, int bytes)
{
    int i;
    struct slabheader *s;
    struct slabdata *f;

    bytes = roundup(bytes, OBJALIGN);

    // 验证指针合法性
    if(objstats(mem, bytes, &s, &f, &i) != OK)
        panic("slablock objstats failed");

    // 锁定整个 slab 页
    SLABDATAUNWRITABLE(f);
}

/* 解锁对象（设为可读写） */
void slabunlock(void *mem, int bytes)
{
    int i;
    struct slabheader *s;
    struct slabdata *f;

    bytes = roundup(bytes, OBJALIGN);

    // 验证指针合法性
    if(objstats(mem, bytes, &s, &f, &i) != OK)
        panic("slabunlock objstats failed");

    // 解锁 slab 页，并标记哪个对象可写
    SLABDATAWRITABLE(f, i);
}

#endif
```

**使用场景**

```c
// 场景1：分配对象时临时解锁
void *slaballoc(int bytes)
{
    // ... 查找空闲槽位 ...

    // 临时解锁以修改位图
    SLABDATAUSE(newslab,
        SETBIT(newslab, i);  // 在解锁状态下执行
    );

    // 返回前重新锁定
    slablock(ret, bytes);

    return ret;
}

// 场景2：释放对象时临时解锁
void slabfree(void *mem, int bytes)
{
    // 先解锁
    slabunlock(mem, bytes);

    // ... 填充 JUNK 数据 ...

    // 临时解锁以修改位图
    SLABDATAUSE(f,
        CLEARBIT(f, i);
    );

    // 注意：释放后不重新锁定，对象已无效
}

// 场景3：用户代码访问对象前解锁
void process_object(struct vir_region *vr)
{
    // 用户代码需要修改对象
    slabunlock(vr, sizeof(*vr));

    vr->flags = NEW_FLAGS;  // 修改对象

    // 完成后重新锁定
    slablock(vr, sizeof(*vr));
}
```

**性能影响**

| 配置 | 性能 | 安全性 | 适用场景 |
|------|------|--------|----------|
| MEMPROTECT=0 | 最高 | 无额外保护 | 生产环境 |
| MEMPROTECT=1 | 中等 | 检测内存损坏 | 调试/测试 |

**编译配置**

```makefile
# Makefile
# 启用内存保护（调试模式）
CFLAGS += -DMEMPROTECT=1

# 禁用内存保护（发布模式）
CFLAGS += -DMEMPROTECT=0
```

**注意事项**

1. **性能开销**：频繁的页锁定/解锁会导致 TLB 刷新，影响性能
2. **粒度限制**：保护粒度是整个物理页，无法单独保护单个对象
3. **仅适用于调试**：MEMPROTECT 主要用于开发和测试阶段
4. **内核支持**：需要内核提供 `vm_pagelock` 系统调用支持

**用户侧接口：`USE()` 宏**

`sanitycheck.h` 中定义了 `USE(obj, code)` 宏，作为 MEMPROTECT 机制在用户代码中的入口：

```c
#if MEMPROTECT
#define USE(obj, code) do {        \
    slabunlock(obj, sizeof(*obj)); \
    do { code } while(0);          \
    slablock(obj, sizeof(*obj));   \
} while(0)
#else
#define USE(obj, code) do { code } while(0)
#endif
```

与 `SLABDATAUSE`（操作 slab 内部元数据）不同，`USE()` 保护的是**非 slab 数据结构的字段修改**。VM 中通过 slab 分配的结构体（`vir_region`、`phys_region`、`phys_block`、`fdref` 等），在 MEMPROTECT 模式下其所在页被锁定为只读；用户代码修改这些结构体的字段时，需要先用 `USE(obj, { ... })` 包裹，临时解锁页面、执行修改、再重新锁定。例如：

```c
USE(vr, vr->parent = vmp;);
USE(pr, pr->offset -= len;);
```

这确保了即使分配器之外的用户代码，也不会意外破坏 slab 对象的只读保护。

---

## 3. Rust 设计决策

### 3.0 问题建模

VM 进程内部的内存分配，本质上是一个**高频小对象分配问题**：

```
输入：
  - 高频/极高频率分配：vir_region（高）、phys_block（极高）、phys_region（高）等结构体（详细频率分析见 §5.1）
  - 对象特征：固定大小（几十~几百字节）、生命周期短
  - 环境约束：用户态单线程进程、64 位地址空间

核心问题：
  是否需要为这些高频对象实现专用的 Slab 分配器？
  还是直接使用 Rust 的 alloc 体系？
```

两个候选方案的形式化对比：

| 维度 | 方案 A：专用 Slab | 方案 B：Rust alloc |
|------|-------------------|---------------------|
| 分配复杂度 | O(1) 位图查找 | O(1) fast path |
| 类型安全 | 无（`void*`） | 编译期保证 |
| 内存安全 | 手动管理 | RAII + 所有权 |
| 开发成本 | 高（自研 ~500 行） | 低（生态复用） |
| 维护成本 | 高（需持续修复 bug） | 低（社区维护） |
| 碎片控制 | 位图级精确 | size class 自动 |

### 3.1 结论：不实现专用 Slab 分配器

> **本项目不实现 Minix3 式的专用 slab allocator，统一使用 Rust `alloc` 体系。**

这是一个经过权衡后的工程决策，而非对 Minix3 原始设计的否定。理解这个决策，需要先理解 Minix3 为什么要写 slab，以及这些原因在 Rust 重写环境中是否仍然成立。

### 3.2 Minix3 为什么需要 Slab —— 历史背景分析

Minix3 的 VM 服务器实现私有 slab 分配器，是特定历史条件和技术环境下的合理选择。以下从四个维度展开分析。

**1. C 语言的"三无环境"**

| 缺失 | 后果 | Slab 如何补偿 |
|------|------|--------------|
| 无类型系统 | `void*` 无类型信息 | 固定大小 = 隐式类型 |
| 无自动析构 | 手动 free 易泄漏 | 集中管理，批量释放 |
| 无标准 allocator | malloc 不可靠 | 自建可控分配器 |

**2. 微内核的 IPC 开销**

在微内核架构中，如果每次内存分配都需要跨进程通信：

```
通用 malloc 路径：VM → IPC → 内核 → IPC → 内存服务 → 返回
Slab 路径：       VM → 纯用户态位图操作 → 返回
```

Slab 将高频操作保留在进程边界内，避免了 IPC 开销。在微内核架构中，减少跨进程通信是性能优化的关键考量。

**3. 32 位地址空间的稀缺性**

- 4GB 地址空间下，每字节都要算计
- 位图管理：每对象 1 bit，理论最低元数据开销
- 密集 8 字节步进（200/8=25 级）：在有限的 32 位地址空间内提供更细颗粒度，够用且简单

**4. VM 的工作负载特征**

- 高频分配/释放（每次 mmap/munmap/fork）
- 固定大小对象（vir_region ~64B, phys_block ~32B）
- 这正是 slab 的"甜点场景"

专用分配器的价值取决于工作负载特征和通用分配器的不足，两个条件缺一不可。

### 3.3 为什么现在不需要了 —— 环境变迁分析

同样的约束，在新的技术环境下已经消解。

**1. 现代分配器已经"内置了 Slab"**

Rust 的 `alloc` 体系底层对接的分配器（无论是系统默认还是 jemalloc/mimalloc），内部已经实现了 slab 的核心机制：

| Slab 优势 | Rust alloc 是否解决 | 机制 |
|-----------|---------------------|------|
| 小对象复用 | ✅ | size class + thread cache |
| O(1) 分配 | ✅ | fast path |
| cache locality | ✅ | arena / size class |
| 减少碎片 | ✅ | 分级管理 |

关键认识：**不是放弃 slab，而是把 slab 的职责交给更成熟的分配器。** Rust 的 alloc 生态是 20+ 年工程经验的结晶，一个 200 行的私有 slab 不可能比它更好。

**2. VM 是用户态进程，不是 kernel runtime**

- 不需要绕过不可靠的 malloc（Rust 的 alloc 是可靠的）
- 不需要避免 IPC（VM 自己就是内存管理服务器，物理页分配走内部函数）
- 单线程，无锁竞争

**3. 64 位地址空间的红利**

- 不再需要在 4GB 里算计每一字节
- 更关注代码可维护性和内存安全性
- RAII 从根源上杜绝内存泄漏

**4. Rust 的类型系统解决了 C 的核心痛点**

| 问题 | C (Minix3) | Rust |
|------|------------|------|
| 类型安全 | `void*` | 编译期检查 |
| 内存泄漏 | 手动管理 | RAII + 所有权 |
| use-after-free | 常见 bug | 编译期阻止 |
| 双重释放 | 常见 bug | 所有权系统阻止 |

### 3.4 VM 内存管理架构

基于上述决策，VM 的内存管理分为四个层次：

```
┌─────────────────────────────────────────────────────────────┐
│                    VM 内存管理层次                            │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  第 0 层：物理页分配器 (PhysAllocator)                        │
│  ├── 直接对接内核，管理物理页                                  │
│  ├── 实现 PhysAllocator trait（详见 04-physical-memory.md）   │
│  └── 提供 alloc_mem() / free_mem()                          │
│                                                             │
│  第 1 层：Rust 全局分配器 (GlobalAlloc)                       │
│  ├── 对接物理页分配器（通过 #[global_allocator]）             │
│  ├── 提供 Box<T>、Vec<T>、BTreeMap 等标准容器                │
│  └── 内部已包含 size class、thread cache 等优化              │
│                                                             │
│  第 2 层：VM 数据结构                                        │
│  ├── Box<VirRegion>      ← 虚拟内存区域                     │
│  ├── Vec<PhysBlock>      ← 物理块数组                       │
│  ├── Box<PhysRegion>     ← 物理区域映射                     │
│  └── Box<VfsRequestNode> ← VFS 请求节点                     │
│                                                             │
│  第 3 层（预留）：关键路径预分配池                             │
│  └── 仅在 profiling 确认瓶颈后启用                           │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**关键设计说明**：

- 物理页分配器是 VM 自己控制的（不是系统的 mmap）
- 全局分配器底层对接 VM 自己的物理页分配器
- VM 自身的动态内存分配不触发内核 IPC。HeapArena 扩展时通过 `vm_self_mappages()` 修改 VM 自身页表，但页表页通过 Direct Map 可达，无递归风险。完整链路：`alloc_phys → HeapArena::grow → vm_self_mappages → 页内切分 → 返回指针`。

### 3.5 关于"确定性"的处理

Minix3 slab 的核心价值不是 slab 本身，而是**将关键路径上的内存分配，转化为可控的、预分配的资源使用**。在 Rust 版本中，我们不需要 slab，但需要**控制分配行为**——控制的粒度是"哪些路径需要保证"，而不是"每个字节怎么管理"。

**Rust 版本的应对策略**：

| 路径类型 | 策略 | 示例 |
|---------|------|------|
| **关键路径**（page fault） | 避免动态分配，或提前预分配 | 预分配 phys_block 池 |
| **半关键路径**（mmap/munmap） | 使用 alloc，但监控统计 | `Box<VirRegion>` |
| **非关键路径**（进程创建） | 标准 alloc | `Vec`、`BTreeMap` |

**预分配策略设计**：

```rust
/// 关键路径预分配池
///
/// 为 page fault 处理等关键路径提供有界分配保证。
/// 池在 VM 启动时预分配，关键路径从池中获取，非关键路径使用全局 alloc。
pub(crate) struct CriticalPool<T> {
    pool: Vec<Box<T>>,
    min_reserved: usize,
}

impl<T: Default> CriticalPool<T> {
    /// 创建预分配池，capacity 为初始大小，min_reserved 为最低保留阈值
    fn new(capacity: usize, min_reserved: usize) -> Self;

    /// 从池中获取一个对象，池空时返回 None（关键路径调用）
    fn take(&mut self) -> Option<Box<T>>;

    /// 将对象归还池中
    fn restore(&mut self, obj: Box<T>);

    /// 检查是否需要补充（当池中对象数低于 min_reserved 时返回 true）
    fn needs_refill(&self) -> bool;

    /// 补充池到指定容量（非关键路径调用）
    fn refill(&mut self, capacity: usize);

    // 完整实现见 §4.3
}
```

### 3.6 可选优化路径（三阶段策略）

```
阶段 1（当前）：全部使用 Rust alloc
  ├── Box / Vec / BTreeMap
  ├── 不引入 slab / arena
  └── 目标：先跑通系统

阶段 2（观察）：添加分配统计
  ├── 记录分配次数、大小分布、热路径
  ├── 如果 VirRegion 分配 > 10万次/秒
  └── 考虑 typed-arena 或对象池

阶段 3（极端）：专用 Slab
  ├── 仅当 profiling 确认 allocator 成为瓶颈
  ├── 只对 2~3 个确认的热点类型
  └── 不做通用 slab allocator
```

三阶段策略的核心原则：先使用通用分配器跑通系统，通过 profiling 确认瓶颈后再考虑专用优化。在没有数据的情况下引入 slab，是在解决一个可能不存在的问题。

### 3.7 教学保留：Rust 版 Slab 设计草案

> **本节为设计思路展示，不纳入实际代码。** 仅用于理解 Slab 分配器的核心机制和 Rust 实现的要点。

#### 3.7.1 核心数据结构设计

```rust
/// Slab 分配器（设计草案）
///
/// 管理多个 SlabCache，每个缓存服务一种对象大小。
/// 大小策略采用 2 的幂次（8, 16, 32, 64, 128, 256...），
/// 而非 Minix3 的密集 8 字节步进（8, 16, 24, 32...）。
/// 2^n 用更少的缓存条目覆盖相同范围，代价是相邻大小间
/// 的间隙更大（如 32 和 64 之间无中间档位）。
pub struct SlabAllocator {
    caches: [SlabCache; NUM_CACHES],
}

/// Slab 缓存 — 管理特定大小的所有 slab
///
/// 对应 Minix3 的 struct slabheader
pub struct SlabCache {
    object_size: usize,
    objects_per_slab: usize,
    partial: LinkedList<Slab>,   // 部分使用的 slab
    full: LinkedList<Slab>,      // 完全使用的 slab
    stats: CacheStats,
}

/// 单个 Slab — 一个物理页
///
/// 对应 Minix3 的 struct slabdata。
/// Minix3 将数据区放在前面、header 在后面，
/// 是为了通过地址掩码快速定位 slab。Rust 不需要这种 trick。
#[repr(C)]
pub struct Slab {
    header: SlabHeader,          // 元数据
    data: [u8; SLAB_DATA_SIZE],  // 对象存储区
}

pub struct SlabHeader {
    free_hint: u16,              // 猜测的空闲位置（启发式优化）
    used_count: u16,             // 已使用对象数
    bitmap: [u64; BITMAP_WORDS], // 使用位图：1=已用, 0=空闲
    next: Option<NonNull<Slab>>, // 链表指针
}
```

#### 3.7.2 分配流程设计

```
slab_alloc(size):
  1. 对齐 size 到 8 字节边界
  2. 计算 cache 索引：index = size.next_power_of_two().trailing_zeros() - 3
  3. 如果 partial 链表为空 → 分配新物理页，构造新 Slab，加入 partial
  4. 从 free_hint 开始在位图中查找第一个 0 位
     - 使用 u64::trailing_ones() 快速定位（单条 CPU 指令）
  5. 置位，used_count++，更新 free_hint
  6. 如果 used_count == objects_per_slab → 移到 full 链表
  7. 返回对象地址：data + index * object_size
```

#### 3.7.3 释放流程设计

```
slab_free(ptr, size):
  1. 根据 ptr 找到所属 Slab（通过页对齐地址遍历链表）
  2. 计算对象索引：index = (ptr - slab.data) / object_size
  3. 断言该位为 1（检测 double free）
  4. 清除位，used_count--
  5. 如果之前是 full → 移回 partial
  6. 如果 used_count == 0 → 释放物理页回系统
```

#### 3.7.4 与 Minix3 的关键差异

| 维度 | Minix3 (C) | Rust 设计草案 |
|------|------------|--------------|
| 大小步进 | 密集 8B 步进（8, 16, 24...） | 2^n（8, 16, 32, 64...） |
| 类型 | `void*` | 泛型 `T` |
| 位图 | `u8[]` 手动位运算 | `u64[]` + `trailing_ones()` |
| 链表 | 手动 prev/next 指针 | `NonNull` + Option |
| 内存布局 | data 在前，header 在后 | header 在前（或分离存储） |
| 对齐 | 手动计算 | `#[repr(C)]` + 编译器 |
| 析构 | 无（C 结构体） | `Drop` 自动调用 |
| 页保护 | MEMPROTECT（仅调试） | 不需要（类型系统保证） |

#### 3.7.5 开发要点

以下列出如果真的要实现，需要关注的技术要点。

**要点 1：位图操作的性能**

Minix3 使用 `u8[]` 逐字节扫描，Rust 可以用 `u64` + `trailing_ones()` 一次检查 64 位。在 512 个对象的 slab 中，最坏情况从 512 次字节检查降为 8 次 u64 检查。

```rust
fn find_free_slot(bitmap: &[u64; 8], hint: usize) -> Option<usize> {
    let start_word = hint / 64;
    for offset in 0..8 {
        let word_idx = (start_word + offset) % 8;
        let word = bitmap[word_idx];
        if word != u64::MAX {
            let bit_idx = word.trailing_ones() as usize;
            let slot = word_idx * 64 + bit_idx;
            return Some(slot);
        }
    }
    None
}
```

**要点 2：指针归属问题**

释放时需要知道对象属于哪个 slab。三种方案：

| 方案 | 做法 | 优点 | 缺点 |
|------|------|------|------|
| 页对齐查找 | `ptr & !0xFFF` 得到页地址，遍历链表 | 零额外开销 | O(n) 遍历 |
| Slab 头部指针 | 每个对象前存 slab 指针 | O(1) 查找 | 每对象 8 字节开销 |
| 分离元数据 | HashMap<page_addr, Slab> | O(1) 查找 | 额外内存 |

Minix3 使用方案 1（data 在前，header 在后，页对齐即得 header）。Rust 设计草案也推荐方案 1，因为 VM 的 slab 数量通常很少（< 100），遍历开销可忽略。

**要点 3：增长与收缩**

- Minix3：满的 slab 从链表移除但不释放（对象仍在使用中）；完全空闲的 slab 立即通过 `vm_freepages()` 释放
- Rust 设计草案：同样策略 — 空闲 slab 立即释放回物理页分配器
- 权衡：释放减少内存占用，但下次分配需要重新申请页

**要点 4：安全性考量**

Rust 的 unsafe 边界设计：

```rust
// 安全接口：调用者不需要 unsafe
impl SlabAllocator {
    pub fn alloc<T: Default>(&mut self) -> Option<&mut T> { ... }
}

// unsafe 接口：调用者负责生命周期
impl SlabAllocator {
    pub unsafe fn alloc_raw<T>(&mut self) -> Option<*mut T> { ... }
    pub unsafe fn free_raw<T>(&mut self, ptr: *mut T) { ... }
}
```

必须防范的错误：
- Double free：位图断言检测
- 类型混淆：泛型参数 T 编译期保证
- 使用已释放内存：Rust 借用检查器保证（安全接口）

**要点 5：为什么不复刻 Minix3 的内存布局**

Minix3 将 `data` 放在前面、`sdh` 放在后面，是为了让数据区对齐到页边界——通过 `ptr & ~0xFFF` 即可定位 slab header。这是 C 语言在无类型系统下的 trick。

Rust 不需要这种 trick：
- 索引管理比地址掩码更安全
- `Vec<Box<T>>` 对象池模式更符合 Rust 习惯
- 如果确实需要页对齐，可以用 `#[repr(C)]` 精确控制布局

### 3.8 总结对比表

| 维度 | Minix3 (C) | Rust 重写 |
|------|------------|-----------|
| 分配策略 | 私有 slab | Rust `alloc` |
| 类型安全 | 无 | 编译期保证 |
| 内存安全 | 手动 | RAII + 所有权 |
| 确定性 | 预分配保证 | 关键路径预分配 |
| 开发成本 | 高（自研） | 低（生态复用） |
| 维护成本 | 高 | 低 |
| 性能 | O(1) 确定 | O(1) fast path |

---

## 4. 实现详解

### 4.1 全局分配器接入

Rust 通过 `#[global_allocator]` 机制允许替换全局内存分配器：实现 `GlobalAlloc` trait，然后用 `#[global_allocator]` 标注一个静态变量，之后所有 `Box`、`Vec`、`String` 等标准容器的堆分配都会走这个分配器。

VM 是一个 freestanding 用户态进程，不能依赖宿主 OS 的 libc `malloc`。它管理自己的物理内存池，因此需要实现自己的全局分配器，底层对接 VM 的物理页分配器。

#### 设计知识点：`GlobalAlloc` 下必须自己做 sub-page 管理

Rust `alloc` crate 的调用链是纯透传的——`Box::new()` → `__rust_alloc(size, align)` → `GLOBAL.alloc(layout)`，**中间没有任何缓冲、cache 或 sub-page 管理**。`alloc` crate 不帮你切页、不做 size class、没有 free list。它只是一个抽象接口层，`GlobalAlloc` 实现者拿到什么返回什么。

因此 `GlobalAlloc::alloc()` 的实现者必须自己做页内切割。如果直接把 `alloc_phys()` 的整页返回（"页级桥接"），每次 `Box::new(24B)` 消耗一整页 4096B——单页可容纳 170 个 `PhysBlock`，在桥接模式下被浪费掉。

Minix3 的 slab allocator 正是承担了这个角色——它从 `vm_allocpage()` 拿页，内部切割为固定大小对象（§2.1）。本节设计的 bump allocator 是 Rust 版本的等价物：从 `VmPageAllocator` 拿页，在 HeapArena 区域内切分。

#### 为什么不能在 Direct Map 区域内切分？

Direct Map 提供 `VA = PA + BASE` 的稳定映射，但**物理不连续则 VA 不连续**。Bump allocator 的 cursor 假设 `[arena_base, arena_base + ARENA_BYTES)` 是连续 VA——如果 arena 跨越物理空洞，cursor 会跳到未映射的 VA，导致 page fault。

HeapArena 解决了这个问题：预留一段连续 VA 区间（`VM_HEAP_BASE .. VM_HEAP_BASE + VM_HEAP_SIZE`），将不连续的物理页逐页映射进去。物理页可以碎片化，但 VA 始终连续。

#### Direct Map 与 HeapArena 的职责分工

| 问题 | 机制 | 一句话定义 |
|------|------|-----------|
| 物理页可达性 | Direct Map | 任意物理页有稳定 VA，无需分配 |
| 虚拟连续性 | HeapArena | 预留连续 VA 区间，按需映射物理页 |

Direct Map 用于物理页管理（页表操作、元数据访问、CoW 拷贝），HeapArena 用于 Rust 堆（`Box`/`Vec`/`String`）。

#### BumpBuf vs HeapArena：两种 bump 的本质差异

读者可能注意到 BumpBuf（04-physical-memory.md §4）和 HeapArena 都是 bump 模式——预留空间、cursor 单调递增、不回收。但它们有本质差异：

| 维度 | BumpBuf（自举阶段） | HeapArena（运行阶段） |
|------|-------------------|---------------------|
| **粒度** | 字节（`alloc_slice<T>()`） | 页（`grow(pages)`） |
| **VA 来源** | Direct Map（`VA = PA + BASE`） | 预留 VA 区间 + `vm_self_mappages()` |
| **VA 连续性** | **跟随 PA 连续性** | **独立于 PA 连续性** |
| **物理页要求** | **必须连续** | 可以碎片化 |
| **是否管页表** | 否（Direct Map 已映射） | 是（`vm_self_mappages()` 写 PTE） |
| **用途** | 分配器元数据（bitmap 等） | Rust 堆（Box/Vec/String） |
| **生命周期** | 自举阶段，一次性 | 运行阶段，持续增长 |
| **搬迁关系** | 搬迁源（连续 PA 约束） | 搬迁目标（碎片化 PA + 连续 VA） |

**核心差异**：BumpBuf 的 VA 连续性来自 PA 连续性（Direct Map 的 `VA = PA + BASE`），因此**强制要求连续物理页**。HeapArena 的 VA 连续性由预留 VA 区间 + 逐页映射保证，物理页可以碎片化。

**为什么自举阶段不能使用 HeapArena？** HeapArena 依赖 `vm_self_mappages()`，而 `vm_self_mappages()` 依赖已初始化的页表。页表初始化又依赖物理页分配器（页表页通过 `alloc_phys()` 分配）。在物理页分配器初始化之前，HeapArena 不可用。这是自举的鸡生蛋问题——BumpBuf 从 Direct Map 的连续物理页中分配元数据，绕过了这个循环。

**为什么运行阶段不再需要连续物理页？** 自举完成后，HeapArena 就位。`VmAllocator::refill_arena()` 调用 `HeapArena::grow()`，后者逐页分配物理页（`alloc_phys(1)` × N）并映射到连续 VA。物理页碎片化不再是问题——HeapArena 将碎片化的物理页缝合为连续 VA。

**实现**：

```rust
// os/servers/vm/src/global.rs

use minix_types::AssumeSyncCell;
use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicPtr, Ordering};

use crate::alloc_page::VmPageAllocator;
use crate::heap_arena::HeapArena;
use crate::phys_mem::CLICK_SIZE;

/// 全局分配器 —— bump allocator，在 HeapArena 区域内切分
///
/// 预分配 16 页（64KB）作为 arena，内部用 cursor 切分。
/// arena 耗尽时自动申请新 arena（旧 arena 不再使用，但不立即归还）。
/// dealloc 是 no-op——bump allocator 不回收单个对象，
/// arena 页在 VM 进程生命周期内保持映射。
///
/// 分配链路：
///   Box::new → GlobalAlloc::alloc → VmAllocator::alloc
///     → bump within current arena
///     → arena exhausted? → refill_arena()
///         → HeapArena::grow(ARENA_PAGES, page_alloc)
///             → alloc_phys(1) × N  (物理页，可碎片化)
///             → vm_self_mappages()  (映射到 HeapArena 连续 VA)
///         → new arena_base = HeapArena VA
pub(crate) struct VmAllocator {
    arena_base: AssumeSyncCell<*mut u8>,
    cursor: AssumeSyncCell<usize>,
}

static PAGE_ALLOC_PTR: AtomicPtr<VmPageAllocator> = AtomicPtr::new(core::ptr::null_mut());

static HEAP_ARENA: HeapArena = HeapArena::new();

pub(crate) fn register_page_alloc(alloc: &mut VmPageAllocator) {
    PAGE_ALLOC_PTR.store(alloc as *mut VmPageAllocator, Ordering::SeqCst);
}

impl VmAllocator {
    const ARENA_PAGES: usize = 16;
    const ARENA_BYTES: usize = Self::ARENA_PAGES * CLICK_SIZE;

    fn refill_arena(&self) -> bool {
        let alloc_ptr = PAGE_ALLOC_PTR.load(Ordering::SeqCst);
        if alloc_ptr.is_null() {
            return false;
        }
        let alloc = unsafe { &mut *alloc_ptr };
        match HEAP_ARENA.grow(Self::ARENA_PAGES, alloc) {
            Ok(va) => {
                unsafe { *self.arena_base.get() = va as *mut u8; }
                unsafe { *self.cursor.get() = 0; }
                true
            }
            Err(_) => false,
        }
    }

    fn ensure_arena(&self) -> bool {
        if unsafe { (*self.arena_base.get()).is_null() } {
            return self.refill_arena();
        }
        true
    }
}

unsafe impl GlobalAlloc for VmAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !self.ensure_arena() {
            return core::ptr::null_mut();
        }

        let size = layout.size();
        let align = layout.align();

        let base = unsafe { *self.arena_base.get() };
        let cursor = unsafe { *self.cursor.get() };
        let ptr = unsafe { base.add(cursor) };
        let offset = ptr.align_offset(align);
        let alloc_start = unsafe { ptr.add(offset) };
        let total = offset + size;

        if cursor + total > Self::ARENA_BYTES {
            if !self.refill_arena() {
                return core::ptr::null_mut();
            }
            return self.alloc(layout);
        }

        unsafe { *self.cursor.get() = cursor + total; }
        alloc_start
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // no-op：bump allocator 不回收单个对象。
        // arena 页在 VM 进程生命周期内保持映射。
        //
        // 这是有意为之：VM server 是长生命周期系统服务，
        // 绝大多数动态分配的结构体（VirRegion, PhysRegion, PhysBlock...）
        // 存活期与 VM 进程绑定，不存在"高频分配-立即释放"的临时对象模式。
        // 如果未来 profiling 发现内存压力，可按需引入 slab（附录 A.3）。
    }
}

#[cfg_attr(not(test), global_allocator)]
static GLOBAL: VmAllocator = VmAllocator {
    arena_base: AssumeSyncCell::new(core::ptr::null_mut()),
    cursor: AssumeSyncCell::new(0),
};
```

注册时机在 `VmServer::new()` 中：

```rust
// os/servers/vm/src/vm_server.rs

impl VmServer {
    pub fn new(total_pages: usize, free_regions: &[BootMemRegion]) -> Self {
        let phys_alloc = Self::create_default_allocator(total_pages, free_regions);
        let mut page_alloc = VmPageAllocator::new(phys_alloc);
        crate::global::register_page_alloc(&mut page_alloc);

        // 在模块级静态存储中创建并注册 VM 自身页表
        // 页表存储在 vm_self_map 模块的 static 中，地址稳定，不受 VmServer 移动影响
        crate::pagetable::init_vm_self_pt();

        Self {
            page_alloc,
            // ...
        }
    }
}
```

注意初始化顺序：`register_page_alloc()` 和 `init_vm_self_pt()` 必须在首次堆分配之前完成。HeapArena::grow() 依赖 `vm_self_mappages()`，而后者依赖已初始化的页表。页表存储在 `vm_self_map` 模块的静态存储中，不随 VmServer 移动，指针始终有效。

**设计权衡**：

| 维度 | bump allocator（当前） | slab（未来备选） | free list |
|------|----------------------|-----------------|-----------|
| 实现复杂度 | ~50 行 | ~200 行 | ~150 行 |
| dealloc 回收 | no-op | 逐对象回收 | 逐对象回收 |
| 内部碎片 | 无（按需切分） | 无（固定槽位） | 有边界情况 |
| 外部碎片 | arena 切换浪费 | 无 | 有 |
| 适用场景 | VM 长生命周期对象 | 高频分配/释放的热点类型 | 通用 heap |

选择 bump 的理由：
- **VM 单线程**，无并发竞争
- **VM 是长生命周期进程**，绝大多数动态分配与进程同寿，不需要逐对象 dealloc
- **Rust 所有权系统**自动处理 drop，dealloc 的 no-op 不会导致 use-after-free
- **16 页 arena** 足够容纳启动阶段的所有结构体；arena 耗尽后才触发 refill

**关键设计决策**：

- **为什么必须用 alloc_phys + vm_phys_to_virt？** VM 是 `no_std` freestanding 进程，没有 libc。它管理自己的物理内存池，GlobalAlloc 直接对接 `alloc_phys() → vm_phys_to_virt()`，使分配链路完全自包含：请求内存 → 从自己的物理池分配 → 通过 Direct Map 访问。

- **为什么走 VmPageAllocator 而非直接操作 PhysAlloc？** GlobalAlloc 通过 `AtomicPtr<VmPageAllocator>` 指向 `VmServer` 持有的 `VmPageAllocator` 实例，这样 GlobalAlloc 的分配/释放也经过 `VmAllocStats` 统计追踪，与 `alloc_page()` / `free_page()` 路径一致，保证统计完整。

- **为什么用 AtomicPtr 而非 AssumeSyncCell？** `GlobalAlloc::alloc(&self)` 接收不可变引用，而 `VmPageAllocator::alloc_phys(&mut self)` 需要可变引用。`AtomicPtr` 允许在 `&self` 内部拿到 `*mut` 指针并 `unsafe` 转为 `&mut`——VM 单线程运行，无数据竞争。

- **与 05-vm-allocpage.md 的呼应**：05 中 `alloc_page()` 底层走 `alloc_phys() → vm_phys_to_virt()`，返回 `(VirBytes, AlignedPhysBytes)` 元组；08 的 GlobalAlloc 走同样底层路径，只返回 `*mut u8`（虚拟地址）。两者共享同一个 `VmPageAllocator` 和同一个 Direct Map——`vm_phys_to_virt()` 是所有"物理页 → 虚拟地址"转换的唯一路径。

- **后续升级路径**：参见[附录 A.3](#a3-未来方向) 关于 slab 和第三方分配器的讨论。

#### Direct Map 下的 Slab 元数据访问

> Direct Map 架构下，slab 元数据的访问从"需要先映射到 VA"变为"物理页天然有 VA"。

Minix3 的 slab 元数据访问需要经过 `vm_pagelock` 解锁/锁定机制（详见 §2.1.5 MEMPROTECT）。这个机制的核心开销不在 `vm_pagelock` 本身，而在于它需要 `pt_writemap` 修改页表项——而 `pt_writemap` 内部需要通过 `createpde` 临时映射窗口来操作页表页。

Direct Map 方案下，slab 元数据的访问路径简化为：

```
Minix3:  slab 元数据在 VM 地址空间 → vm_pagelock 解锁 → pt_writemap → createpde → 修改页表项 → TLB 刷新
Direct Map:  slab 元数据在 direct map 中 → 直接读写（vm_phys_to_virt 已提供 VA）
```

但更深层的变化是：Minix3 的 MEMPROTECT 机制（`vm_pagelock`）在 direct map 下需要重新审视。Direct map 的 PTE 权限是 U/S=1（用户态可访问），VM 可以直接读写所有物理页。如果需要 slab 元数据的写保护，不能再用 `vm_pagelock`（它修改的是 VM 地址空间的 PTE，而 direct map 的 PTE 是共享的），需要考虑其他机制（如 mprotect on direct map 范围，或软件层面的写保护）。

这是 `vm_phys_to_virt()` 统一性的一个具体例证——slab 元数据和其他物理页一样，通过同一个 direct map 访问，不再有特殊的映射路径。

#### `vm_pagelock` 消除决策

Minix3 的 `vm_pagelock` 在 minix-rs 中**完全消除**，理由有三层：

**1. 无调用者**：`vm_pagelock` 的唯一调用者是 slab 分配器的 `SLABDATAUSE` 宏（§2.1.5 MEMPROTECT）。minix-rs 不实现专用 slab 分配器（§3.1 决策），改用 Rust `alloc` 体系，因此 `vm_pagelock` 不存在调用者。

**2. Direct Map 下机制失效**：`vm_pagelock` 修改 VM 地址空间中 slab 元数据页的 PTE 权限（只读↔读写），但 Direct Map 的 PTE 是所有进程共享的 kernel/VM 映射——不能为了 slab 保护而把 direct map 的某页设为只读，这会阻塞所有通过 direct map 访问该物理页的路径。即使未来需要 slab 元数据写保护，也不能用修改 PTE 的方式实现。

**3. Rust 安全模型替代调试价值**：MEMPROTECT 本身是 `#if MEMPROTECT` 条件编译的可选调试机制（§2.1.5 "生产环境：无保护"），其价值在于检测 C 语言的悬空指针访问。Rust 的所有权系统和借用检查器在编译期阻止大部分悬空指针问题；对于 `unsafe` 代码，Rust 生态使用 `#[cfg(debug_assertions)]` 或 Miri 做运行时检测，无需通过页表权限实现。

> **07 文档影响**：`Paging::update_flags()` 的 API 契约中增加约束——Direct Map 下不能通过修改 VM 地址空间 PTE 实现 slab 写保护（PTE 共享），详见本节。

### 4.2 分配统计与可观测性

为支持阶段 2 的观察和 profiling，VM 需要内置分配统计能力：

```rust
// os/servers/vm/src/alloc_stats.rs

//! VM page allocation statistics module.
//!
//! # Single-threaded Assumption
//!
//! All fields are plain `usize`. The VM server is single-threaded;
//! atomic operations are unnecessary and misleading.

pub(crate) struct VmAllocStats {
    total_allocations: usize,
    total_deallocations: usize,
    total_alloc_clicks: usize,
    total_dealloc_clicks: usize,
    allocation_failures: usize,
}

impl VmAllocStats {
    pub(crate) const fn new() -> Self {
        Self {
            total_allocations: 0,
            total_deallocations: 0,
            total_alloc_clicks: 0,
            total_dealloc_clicks: 0,
            allocation_failures: 0,
        }
    }

    pub(crate) fn record_alloc(&mut self, clicks: usize) {
        self.total_allocations += 1;
        self.total_alloc_clicks += clicks;
    }

    pub(crate) fn record_dealloc(&mut self, clicks: usize) {
        debug_assert!(
            self.total_deallocations < self.total_allocations,
            "record_dealloc underflow: more deallocs than allocs"
        );
        self.total_deallocations += 1;
        self.total_dealloc_clicks += clicks;
    }

    pub(crate) fn record_failure(&mut self) {
        self.allocation_failures += 1;
    }

    pub(crate) fn active_allocations(&self) -> usize {
        self.total_allocations - self.total_deallocations
    }

    pub(crate) fn active_pages(&self) -> usize {
        self.total_alloc_clicks - self.total_dealloc_clicks
    }

    pub(crate) fn check_leak(&self) -> Option<usize> {
        let active = self.active_pages();
        if active > 0 {
            Some(active)
        } else {
            None
        }
    }
}
```

**设计说明**：

- 统计信息不是 slab 专属的，而是 VM 全局的。这符合"不实现专用 slab"的决策——我们监控的是整个 VM 的分配行为，而非某个分配器的内部状态。
- 所有字段使用 `usize` 而非 `AtomicUsize`。VM 是单线程服务器，原子操作不必要且具有误导性——使用原子类型会暗示存在并发访问，这是错误的心智模型。方法签名使用 `&mut self` 而非 `&self`，在类型层面明确独占访问。
- `record_alloc`/`record_dealloc` 接受 `clicks` 参数，追踪页级粒度。`check_leak()` 返回 `active_pages()`（活跃页数），而非 `active_allocations()`（活跃分配次数），因为页级泄漏比分配级泄漏更有意义。

### 4.3 关键路径预分配策略

对于 page fault 等关键路径，需要保证分配不会失败。`CriticalPool` 提供有界分配保证：

```rust
// os/servers/vm/src/critical_pool.rs

/// 关键路径对象池
///
/// 为 page fault 处理等关键路径提供有界分配保证。
/// 池在 VM 启动时预分配，关键路径从池中获取，非关键路径使用全局 alloc。
///
/// 类型参数 T 必须是 VM 内部结构体，如 PhysBlock。
pub(crate) struct CriticalPool<T> {
    pool: Vec<Box<T>>,
    min_reserved: usize,
}

impl<T: Default> CriticalPool<T> {
    /// 创建预分配池
    ///
    /// `capacity` 为池的初始大小，`min_reserved` 为最低保留阈值。
    /// 当池中对象数低于 `min_reserved` 时，在非关键路径上触发补充。
    pub(crate) fn new(capacity: usize, min_reserved: usize) -> Self {
        let mut pool = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            pool.push(Box::new(T::default()));
        }
        Self { pool, min_reserved }
    }

    /// 从池中获取一个对象（关键路径调用）
    ///
    /// 如果池为空，返回 None。调用者应处理此情况
    /// （例如回退到全局 alloc 或返回错误）。
    pub(crate) fn take(&mut self) -> Option<Box<T>> {
        self.pool.pop()
    }

    /// 将对象归还池中
    pub(crate) fn restore(&mut self, obj: Box<T>) {
        self.pool.push(obj);
    }

    /// 检查是否需要补充（非关键路径调用）
    pub(crate) fn needs_refill(&self) -> bool {
        self.pool.len() < self.min_reserved
    }

    /// 补充池到初始容量（非关键路径调用）
    pub(crate) fn refill(&mut self, capacity: usize) {
        while self.pool.len() < capacity {
            self.pool.push(Box::new(T::default()));
        }
    }
}
```

**使用示例**：

```rust
// VM 启动时
let mut phys_block_pool = CriticalPool::<PhysBlock>::new(64, 16);

// Page fault 处理中（关键路径）
fn handle_page_fault(pool: &mut CriticalPool<PhysBlock>) -> Result<(), VmError> {
    let block = pool.take()
        .ok_or(VmError::CriticalPoolExhausted)?;
    // ... 使用 block 处理缺页 ...
    Ok(())
}

// 非关键路径上检查并补充
fn periodic_maintenance(pool: &mut CriticalPool<PhysBlock>) {
    if pool.needs_refill() {
        pool.refill(64);
    }
}
```

**设计权衡**：

| 方面 | 预分配池 | 专用 Slab |
|------|---------|----------|
| 实现复杂度 | ~50 行 | ~500 行 |
| 类型安全 | 泛型保证 | 无 |
| 适用场景 | 1~3 个关键类型 | 所有固定大小对象 |
| 内存开销 | 预分配 N 个对象 | 按需分配页 |

`take()` 使用 `Vec::pop()` 是 LIFO（后进先出）：最近归还的对象优先取出，利用 CPU 缓存局部性。对于 page fault 等关键路径，LIFO 比 FIFO 更有利。

---

## 5. VM 内部使用分析

### 5.1 各结构体的使用场景

Minix3 VM 中通过 slab 分配的结构体及其在 Rust 版本中的对应方式：

| Minix3 结构体 | Rust 大小 (x86_64) | 分配频率 | Rust 版本 |
|--------------|------|---------|-----------|
| `vir_region` | ~112B | 高（每次 mmap） | `Box<VirRegion>` |
| `phys_region` | ~48B | 高（每次映射物理页） | `Box<PhysRegion>` |
| `phys_block` | ~24B | 极高（page fault） | `CriticalPool<PhysBlock>` |
| `fdref` | ~32B | 低 | `Box<FdRef>` |
| `vfs_request_node` | ~40B | 中 | `Box<VfsRequest>` |
| `cached_page` | ~16B | 中（VFS 文件映射） | `Box<CachedPage>` |

**为什么 phys_block 走预分配池？**

phys_block 在 page fault 处理中被频繁分配。page fault 是关键路径——如果分配失败，进程会收到 SIGSEGV。因此 phys_block 是唯一需要预分配保证的类型。

其他结构体（vir_region、phys_region 等）的分配失败可以被上层优雅处理（返回错误码），不需要预分配保证。

### 5.2 为什么其他服务器不用 slab

Minix3 中只有 VM 服务器实现了私有 slab，其他服务器（VFS、PM、RS）使用标准 malloc。原因：

1. **VM 的工作负载特殊**：VM 处理 mmap/munmap/fork/page fault，这些操作涉及大量固定大小结构体的分配/释放
2. **其他服务器的分配模式不同**：VFS 主要分配可变大小缓冲区，PM 主要管理进程表（数组而非链表）
3. **VM 对延迟敏感**：page fault 处理直接影响用户进程的响应时间

---

## 6. 测试与验证

### 6.1 分配统计测试

```rust
#[test]
fn test_alloc_stats_basic() {
    let mut stats = VmAllocStats::new();

    assert_eq!(stats.active_allocations(), 0);
    assert_eq!(stats.active_pages(), 0);
    assert_eq!(stats.check_leak(), None);

    stats.record_alloc(1);
    stats.record_alloc(4);
    assert_eq!(stats.active_allocations(), 2);
    assert_eq!(stats.active_pages(), 5);

    stats.record_dealloc(1);
    assert_eq!(stats.active_allocations(), 1);
    assert_eq!(stats.active_pages(), 4);

    stats.record_dealloc(4);
    assert_eq!(stats.active_allocations(), 0);
    assert_eq!(stats.active_pages(), 0);
    assert_eq!(stats.check_leak(), None);
}

#[test]
fn test_alloc_stats_leak_detection() {
    let mut stats = VmAllocStats::new();

    stats.record_alloc(1);
    stats.record_alloc(4);
    stats.record_dealloc(1);

    // 活跃页 = 1 + 4 - 1 = 4，存在泄漏
    assert_eq!(stats.check_leak(), Some(4));
}

#[test]
fn test_alloc_stats_failure_tracking() {
    let mut stats = VmAllocStats::new();

    stats.record_failure();
    stats.record_failure();

    // 验证失败计数不影响活跃分配和活跃页
    assert_eq!(stats.active_allocations(), 0);
    assert_eq!(stats.active_pages(), 0);
}
```

### 6.2 预分配池测试

```rust
#[test]
fn test_critical_pool_take_restore() {
    let mut pool = CriticalPool::<TestObj>::new(4, 2);

    // 初始状态：4 个对象
    assert!(!pool.needs_refill());

    // 取出 3 个
    let _a = pool.take().unwrap();
    let b = pool.take().unwrap();
    let _c = pool.take().unwrap();

    // 剩余 1 个，低于 min_reserved(2)
    assert!(pool.needs_refill());

    // 归还 1 个
    pool.restore(b);

    // 剩余 2 个，不低于阈值
    assert!(!pool.needs_refill());
}

#[test]
fn test_critical_pool_exhaustion() {
    let mut pool = CriticalPool::<TestObj>::new(2, 1);

    let _a = pool.take().unwrap();
    let _b = pool.take().unwrap();

    // 池已空
    assert!(pool.take().is_none());
}

#[test]
fn test_critical_pool_refill() {
    let mut pool = CriticalPool::<TestObj>::new(4, 2);

    // 取空
    let _a = pool.take().unwrap();
    let _b = pool.take().unwrap();
    let _c = pool.take().unwrap();
    let _d = pool.take().unwrap();
    assert!(pool.take().is_none());

    // 补充
    pool.refill(4);
    assert_eq!(pool.take().is_some(), true);
}
```

### 6.3 内存压力测试

```rust
#[test]
fn test_alloc_stress() {
    let mut stats = VmAllocStats::new();

    for _ in 0..10000 {
        stats.record_alloc(1);
    }

    for _ in 0..10000 {
        stats.record_dealloc(1);
    }

    assert_eq!(stats.active_allocations(), 0);
    assert_eq!(stats.active_pages(), 0);
    assert_eq!(stats.check_leak(), None);
}
```

---

## 附录 A：Rust 全局分配器选型分析

### A.0 接入指南：`#[global_allocator]` 工作原理

Rust 程序的所有堆分配（`Box`、`Vec`、`String` 等）最终都通过全局分配器完成。标准环境下，Rust 默认链接系统的 libc `malloc`/`free`。在 `no_std` 或自定义内存管理的场景下，可以通过 `#[global_allocator]` 替换。

**三步接入**：

**第一步：实现 `GlobalAlloc` trait**

```rust
use core::alloc::{GlobalAlloc, Layout};

struct MyAllocator;

unsafe impl GlobalAlloc for MyAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // layout.size()  — 请求的字节数
        // layout.align() — 对齐要求
        // 返回：指向分配内存的指针，失败返回 null
        todo!()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // ptr    — alloc() 返回的指针
        // layout — 必须与 alloc() 时的 layout 一致
        todo!()
    }
}
```

**第二步：注册为全局分配器**

```rust
#[global_allocator]
static GLOBAL: MyAllocator = MyAllocator;
```

此后所有 `Box::new(...)`、`vec.push(...)` 等操作都会调用 `MyAllocator::alloc()`。

**第三步（可选）：测试隔离**

```rust
#[cfg_attr(not(test), global_allocator)]  // 仅在非测试时注册
static GLOBAL: MyAllocator = MyAllocator;
```

`cfg_attr(not(test), ...)` 确保 `cargo test` 时使用标准分配器，避免自定义分配器与测试框架冲突。

**VM 的特殊之处**：

VM 的 `alloc()` 不走 libc `malloc`，而是直接调用自己的物理页分配器：

```
Box::new(vir_region)
  → GlobalAlloc::alloc(layout)
    → VmAllocator::alloc()
      → bump within arena (HeapArena VA)
      → arena exhausted?
          → HeapArena::grow()
              → alloc_phys()           // 从 VM 的物理内存池分配页
              → vm_self_mappages()     // 映射到 HeapArena 连续 VA
          → 返回 VA 指针
```

整个链路不经过内核，不依赖 C 运行时。Direct Map 提供物理页可达性，HeapArena 提供虚拟连续性，二者分工明确。

---

### A.1 当前实现：bump allocator

§4.1 的 `VmAllocator` 是一个 bump allocator——预分配 16 页（64KB）arena，内部用 cursor 切分。dealloc 是 no-op。

**为什么 bump**：

- **VM 单线程**，无并发竞争，不需要锁或原子 sync
- **VM 是长生命周期进程**。绝大多数动态分配的结构体（`VirRegion` ~112B、`PhysRegion` ~48B、`PhysBlock` ~24B）存活期与 VM 进程绑定，不存在"高频分配-立即释放"的临时对象模式。dealloc 的 no-op 不会导致内存泄漏——这些对象本身就是永久性的
- **Rust 所有权系统**自动处理 drop，dealloc 的 no-op 不会造成 use-after-free

**当前局限**：bump 不回收单个对象。arena 页在 VM 进程生命周期内保持映射。如果未来某些类型出现了高频分配/释放模式（例如 `VfsRequestNode`），bump 会在 arena 耗尽时频繁 refill，产生外部碎片。

### A.2 设计层次

将分配链路展开，`VmAllocator` 占据中间两层：

```
Layer 1: PhysAlloc / VmPageAllocator（页提供者）
    ↓  alloc_phys(clicks) → 物理页
Layer 2: Direct Map（物理页可达性）+ HeapArena（虚拟连续性）
    ↓  Direct Map: vm_phys_to_virt(phys) = VM_DIRECT_MAP_BASE + phys  （页表/元数据访问）
    ↓  HeapArena:  grow(pages) → alloc_phys × N + vm_self_mappages    （Rust 堆）
Layer 3: VmAllocator 内部 bump（页内切割）
    ↓  cursor += size，在 arena 内切出字节
Layer 4: Rust GlobalAlloc trait（接口入口）
    ↓  Box::new() → __rust_alloc → GLOBAL.alloc(layout)
Layer 5: Box / Vec / String（rust 标准容器）
```

`#[global_allocator]` 只是接口入口。真正的分配逻辑在 Layer 3——页内切割。Layer 3 和 Layer 4 都在 `VmAllocator` 内部实现。

注意 Layer 2 的双机制：Direct Map 解决"物理页可达性"（页表操作、元数据访问），HeapArena 解决"虚拟连续性"（Rust 堆）。两者不是替代关系，而是互补关系。

**Layer 2 的状态转换——搬迁**：自举阶段，分配器元数据通过 BumpBuf 从 Direct Map 区域分配（Layer 2 的 Direct Map 侧）。HeapArena 就位后，`VmServer::relocate()` 将元数据从 Direct Map 侧迁移到 HeapArena 侧——这是 Layer 2 内部的状态转换。搬迁后，Direct Map 侧的连续 PA 页被释放回 Layer 1，分配器元数据通过 HeapArena VA 访问（详见 09-vm-relocation.md）。

### A.3 未来方向

**如果需要更精细的内存回收**：

可引入一个简单的 slab 缓存，为热点类型（`PhysBlock` ~24B、`PhysRegion` ~48B）各维护一个固定大小对象池。slab 的好处是 $O(1)$ dealloc——归还的槽位立即重用。但优先级低，因为当前 bump 已经消除了"每对象一页"的浪费。

**如果需要支持多线程或更通用的分配模式**：

可评估 jemalloc/mimalloc 等第三方分配器。但需要注意：这些分配器默认面向"已有 OS 的 userspace"（通过 `mmap`/`munmap` 向 OS 申请内存），不是"自己管理 physical memory 的 VM server"。接入它们需要实现 allocator backend——让它们调用 `alloc_phys()` 而非 `mmap()`。这是一个 FFI 适配工程，不是简单的 `#[global_allocator]` 替换。

**决策原则**：profiling 驱动。在没有数据的情况下引入更复杂的分配器，是在解决一个可能不存在的问题。当前 64KB bump arena 可容纳 ~570 个 `PhysBlock`，远超 VM 启动阶段的分配需求。

---

## 附录 B：Minix3 Slab 的 32 位局限性

Minix3 的 slab 设计深受 32 位地址空间限制的影响：

**1. 4GB 地址空间上限**

32 位系统虚拟地址空间最大 4GB。Minix3 的 VM 服务器作为一个用户态进程，可用的地址空间更小（通常 < 2GB）。这迫使设计者精打细算：

- 位图管理：每对象 1 bit，理论最低元数据开销
- 密集 8 字节步进（200/8=25 级）：在 32 位下提供细粒度缓存，减少内部碎片
- 即时释放：空闲 slab 立即归还，不保留缓存

**2. 物理内存上限**

32 位系统物理内存通常 < 4GB。VM 管理的物理页数量有限（< 1M 页），bitmap 大小可控（< 128KB）。

**3. 64 位下的变化**

| 维度 | 32 位 | 64 位 |
|------|-------|-------|
| 虚拟地址空间 | 4GB | 256TB |
| 物理内存上限 | 4GB | 256TB+ |
| Bitmap 大小 | < 128KB | < 8MB |
| 元数据占比 | < 3% | < 0.003% |

在 64 位下，元数据开销几乎可以忽略，不再需要为了节省几 KB 而引入复杂的位图管理。保留 Slab 的设计方案，是为了在 profiling 确认分配器成为瓶颈时，有一个可选的优化路径。

---

## 7. 参见

- [04-physical-memory.md](04-physical-memory.md) - 物理页分配器（全局分配器的底层）
- [05-vm-allocpage.md](05-vm-allocpage.md) - VM 物理页分配（`alloc_page()`/`free_pages()`）
- [07-pagetable-ops.md](07-pagetable-ops.md) - 页表操作（Direct Map 相关）
- [12-vir-region.md](12-vir-region.md) - VirRegion 结构体
- [14-phys-region.md](14-phys-region.md) - PhysRegion 结构体
- [10-phys-block.md](10-phys-block.md) - PhysBlock 结构体

---

*分类: VM私有 | 使用范围: 仅 VM 内部*