# 08-slab-allocator: Slab 分配器

> **分类**: VM私有 ✅（修正：不是全局基建）  
> **源码**: [slaballoc.c](minix3/minix/servers/vm/slaballoc.c)  
> **说明**: VM 专用的内存分配器，用于分配固定大小的对象
> **状态**: ⚠️ Rust 实现尚未完成。本文档 §1-§2 为 Minix3 C 源码分析，§3+ 为设计分析，具体 Rust 实现代码待补充。

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
  - 不如 Linux 的几何级数策略高效

**对比：Minix3 vs Linux 设计哲学**

| 维度 | Minix3 VM | Linux Kernel |
|-----|-----------|--------------|
| **设计目标** | 实现简单，易于维护 | 性能极致，内存高效 |
| **大小策略** | 线性增长（8, 16, 24, 32...） | 几何级数（8, 16, 32, 64, 128...） |
| **适用场景** | 微内核 VM，对象种类有限 | 通用内核，对象种类多样 |
| **实现复杂度** | 低 | 高 |

---

## 2. C 源码分析

### 2.0 物理页面的获取

Slab allocator 需要物理页面来存储对象，调用链如下：

```
slaballoc()
  └─> newslabdata()              // 分配新的 slab
       └─> vm_allocpage()        // 分配一个物理页
            └─> vm_allocpages()  // 分配物理页（支持多页）
                 ├─> alloc_mem() // 从物理内存池分配
                 └─> vm_mappages() // 映射到 VM 的虚拟地址空间
```

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
│                                                             │
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
#define SCL_FUNCTIONS  0   // 函数入口/出口检查
#define SCL_DETAIL     1   // 详细检查（遍历链表等）

// 主 sanity check 宏
#define SLABSANITYCHECK(level) do { \
    if(SANITYCHECKS) { \
        slab_sanitycheck(__FILE__, __LINE__); \
    } \
} while(0)
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
// 用于抑制 JUNK 警告的计数器
// 在 MEMPROTECT 解锁/锁定期间，对象会临时包含 JUNK
// 需要抑制警告避免误报
static int nojunkwarning = 0;
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

**Rust 重写的建议**：
- 生产环境：禁用 MEMPROTECT
- 调试环境：可以考虑使用 INVLPG 而不是 reload_cr3
- 或者使用更轻量的调试机制（如 RedZone、canary）

**示例：8 字节对象的 slab**

```
对象大小：8 bytes
页大小：4096 bytes
Slab 头大小：~64 bytes
数据区大小：4032 bytes
可存储对象数：4032 / 8 = 504 个

位图大小：504 / 8 = 63 bytes = 63 个 u8_t

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
- Rust 重写时可以考虑移除或条件编译

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
    *sp = s;
    *fp = f;
    *ip = i;
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

---

## 3. Rust 设计决策

### 3.0 问题建模

VM 进程内部的内存分配，本质上是一个**高频小对象分配问题**：

```
输入：
  - 高频分配：vir_region、phys_block、phys_region 等结构体
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

> **设计启发**：Minix3 的 slab 本质上是在**用 C 的位图模拟类型系统**。当你发现自己在用底层机制模拟高层抽象时，说明语言选型可能需要重新考虑。

**2. 微内核的 IPC 开销**

在微内核架构中，如果每次内存分配都需要跨进程通信：

```
通用 malloc 路径：VM → IPC → 内核 → IPC → 内存服务 → 返回
Slab 路径：       VM → 纯用户态位图操作 → 返回
```

Slab 将高频操作保留在进程边界内，避免了 IPC 开销。这是微内核架构下性能优化的第一原则。

> **设计启发**：在微内核架构中，"避免 IPC"是性能优化的第一原则。Slab 的价值不在于"slab 本身"，而在于**将高频操作保留在进程边界内**。

**3. 32 位地址空间的稀缺性**

- 4GB 地址空间下，每字节都要算计
- 位图管理：每对象 1 bit，理论最低元数据开销
- 线性 8-200 字节分级：够用且简单

> **设计启发**：硬件约束塑造软件架构。32 位下的"精巧设计"在 64 位下可能变成"过度优化"。好的架构师知道**何时让旧设计退役**。

**4. VM 的工作负载特征**

- 高频分配/释放（每次 mmap/munmap/fork）
- 固定大小对象（vir_region ~64B, phys_block ~32B）
- 这正是 slab 的"甜点场景"

> **设计启发**：专用分配器的价值 = 工作负载特征 × 通用分配器的不足。两个条件缺一不可。

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

> **设计启发**：好的系统设计不是"什么都自己做"，而是**知道什么该委托给下层**。

**2. VM 是用户态进程，不是 kernel runtime**

- 不需要绕过不可靠的 malloc（Rust 的 alloc 是可靠的）
- 不需要避免 IPC（VM 自己就是内存管理服务器，物理页分配走内部函数）
- 单线程，无锁竞争

> **设计启发**：Minix3 的 slab 是"在受限环境下不得不做的工程补丁"。当限制消失，补丁也应该消失。

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

> **设计启发**：Minix3 的 slab 用位图模拟类型系统。Rust 已经有了真正的类型系统，不需要这种模拟。**当语言提供了更好的机制，用语言机制替代手动管理。**

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
- 这意味着 VM 的内存分配**全程在用户态完成**，不经过内核

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
    free_list: Vec<Box<T>>,
    min_reserved: usize,  // 低于此阈值时触发补充
}
```

> **设计启发**：你不需要 slab，但你需要**控制分配行为**。控制的粒度是"哪些路径需要保证"，而不是"每个字节怎么管理"。

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

> **设计启发**："先相信 allocator，再用数据推翻它。" 过早优化是万恶之源。在没有任何 profiling 数据的情况下引入 slab，是在解决一个可能不存在的问题。

### 3.7 教学保留：Rust 版 Slab 设计草案

> **本节为设计思路展示，不纳入实际代码。** 仅用于理解 Slab 分配器的核心机制和 Rust 实现的要点。

#### 3.7.1 核心数据结构设计

```rust
/// Slab 分配器（设计草案）
///
/// 管理多个 SlabCache，每个缓存服务一种对象大小。
/// 大小策略采用几何级数（8, 16, 32, 64, 128, 256...），
/// 而非 Minix3 的线性策略（8, 16, 24, 32...）。
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
| 大小策略 | 线性 8, 16, 24, 32... | 几何级数 8, 16, 32, 64... |
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

- Minix3 的 slab 永不收缩（满的 slab 从链表移除但不释放）
- Rust 设计草案建议：空闲 slab 立即释放回物理页分配器
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
- `Vec<T>` 或 `free_list` 模式更符合 Rust 习惯
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

本章对应实际代码，而非设计草案。

### 4.1 全局分配器接入

VM 通过 `#[global_allocator]` 接入 Rust 的全局分配器体系，底层对接 VM 自己的物理页分配器：

```rust
// os/servers/vm/src/global.rs

use core::alloc::{GlobalAlloc, Layout};

/// VM 的全局分配器
///
/// 底层对接 VM 自己的物理页分配器（PhysAllocator），
/// 而非系统的 mmap。这意味着 VM 的内存分配全程在用户态完成。
pub(crate) struct VmAllocator;

unsafe impl GlobalAlloc for VmAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // 将 Layout 转换为页数，调用 PhysAllocator::alloc_mem()
        // 映射到 VM 地址空间后返回指针
        todo!("对接 PhysAllocator")
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // 调用 PhysAllocator::free_mem() 释放物理页
        todo!("对接 PhysAllocator")
    }
}

#[global_allocator]
static GLOBAL: VmAllocator = VmAllocator;
```

**关键设计决策**：

- **为什么不用系统的 mmap？** VM 是内存管理服务器，物理页由自己管理。使用系统 mmap 会引入对内核的依赖，且无法精确控制物理页的分配策略。
- **为什么不用 jemalloc/mimalloc？** 当前阶段使用系统默认分配器即可。关于分配器的选型分析，详见附录 A。
- **可替换性**：通过 `#[global_allocator]` 机制，替换分配器只需修改一处代码，无需改动任何业务逻辑。

### 4.2 分配统计与可观测性

为支持阶段 2 的观察和 profiling，VM 需要内置分配统计能力：

```rust
// os/servers/vm/src/alloc_stats.rs

use core::sync::atomic::{AtomicUsize, Ordering};

/// VM 全局分配统计
///
/// 使用原子计数器，支持无锁并发访问。
/// 当前 VM 为单线程，原子操作仅用于未来扩展。
pub(crate) struct VmAllocStats {
    total_allocations: AtomicUsize,
    total_deallocations: AtomicUsize,
    allocation_failures: AtomicUsize,
}

impl VmAllocStats {
    pub(crate) const fn new() -> Self {
        Self {
            total_allocations: AtomicUsize::new(0),
            total_deallocations: AtomicUsize::new(0),
            allocation_failures: AtomicUsize::new(0),
        }
    }

    pub(crate) fn record_alloc(&self) {
        self.total_allocations.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_dealloc(&self) {
        self.total_deallocations.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_failure(&self) {
        self.allocation_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// 当前活跃分配数 = 总分配 - 总释放
    pub(crate) fn active_allocations(&self) -> usize {
        self.total_allocations.load(Ordering::Relaxed)
            - self.total_deallocations.load(Ordering::Relaxed)
    }

    /// 检测潜在的内存泄漏
    pub(crate) fn check_leak(&self) -> Option<usize> {
        let active = self.active_allocations();
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
- `check_leak()` 方法提供轻量级泄漏检测。在 VM 退出时调用，如果活跃分配数 > 0，说明存在内存泄漏。
- 原子操作的开销极小（单条 CPU 指令），不会成为性能瓶颈。

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

---

## 5. VM 内部使用分析

### 5.1 各结构体的使用场景

Minix3 VM 中通过 slab 分配的结构体及其在 Rust 版本中的对应方式：

| Minix3 结构体 | 大小 | 分配频率 | Rust 版本 |
|--------------|------|---------|-----------|
| `vir_region` | ~64B | 高（每次 mmap） | `Box<VirRegion>` |
| `phys_region` | ~32B | 高（每次映射物理页） | `Box<PhysRegion>` |
| `phys_block` | ~32B | 极高（page fault） | `CriticalPool<PhysBlock>` |
| `fdref` | ~16B | 低 | `Box<FdRef>` |
| `vfs_request_node` | ~48B | 中 | `Box<VfsRequestNode>` |

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
    let stats = VmAllocStats::new();

    assert_eq!(stats.active_allocations(), 0);
    assert_eq!(stats.check_leak(), None);

    stats.record_alloc();
    stats.record_alloc();
    assert_eq!(stats.active_allocations(), 2);

    stats.record_dealloc();
    assert_eq!(stats.active_allocations(), 1);

    stats.record_dealloc();
    assert_eq!(stats.active_allocations(), 0);
    assert_eq!(stats.check_leak(), None);
}

#[test]
fn test_alloc_stats_leak_detection() {
    let stats = VmAllocStats::new();

    stats.record_alloc();
    stats.record_alloc();
    stats.record_dealloc();

    // 活跃分配 = 2 - 1 = 1，存在泄漏
    assert_eq!(stats.check_leak(), Some(1));
}

#[test]
fn test_alloc_stats_failure_tracking() {
    let stats = VmAllocStats::new();

    stats.record_failure();
    stats.record_failure();

    // 验证失败计数不影响活跃分配
    assert_eq!(stats.active_allocations(), 0);
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
    let a = pool.take().unwrap();
    let b = pool.take().unwrap();
    let c = pool.take().unwrap();

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
    let a = pool.take().unwrap();
    let b = pool.take().unwrap();
    let c = pool.take().unwrap();
    let d = pool.take().unwrap();
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
    let stats = VmAllocStats::new();
    let mut ptrs = Vec::new();

    // 模拟高频分配/释放
    for _ in 0..10000 {
        stats.record_alloc();
        ptrs.push(0usize); // 模拟分配
    }

    for _ in 0..10000 {
        stats.record_dealloc();
        ptrs.pop(); // 模拟释放
    }

    assert_eq!(stats.active_allocations(), 0);
    assert_eq!(stats.check_leak(), None);
}
```

---

## 7. 参见

- [04-physical-memory.md](04-physical-memory.md) - 物理页分配器（全局分配器的底层）
- [12-vir-region.md](12-vir-region.md) - VirRegion 结构体
- [14-phys-region.md](14-phys-region.md) - PhysRegion 结构体
- [10-phys-block.md](10-phys-block.md) - PhysBlock 结构体

---

## 附录 A：Rust 全局分配器选型分析

### A.1 候选分配器概览

Rust 通过 `#[global_allocator]` 机制支持替换全局分配器。以下是主流选项：

| 分配器 | 定位 | Rust crate | 维护状态 |
|--------|------|-----------|---------|
| 系统默认 (glibc malloc) | 通用 | 内置 | 活跃 |
| jemalloc | 高性能通用 | `tikv-jemallocator` | 活跃 |
| mimalloc | 低延迟 | `mimalloc-rust` | 活跃 |
| snmalloc | 消息传递优化 | `snmalloc-rs` | 活跃 |
| tlsf | 实时系统 | 无官方 crate | 社区 |

### A.2 各分配器详细分析

#### A.2.1 系统默认 (glibc malloc)

Linux 下默认使用 glibc 的 ptmalloc2，基于 Doug Lea's malloc。

**优点**：
- 零配置，开箱即用
- 久经考验，稳定性极高
- 与系统深度集成

**缺点**：
- 多线程场景下碎片率较高
- 性能中等，不如专用分配器
- 对微内核场景无特殊优化

**适用场景**：开发阶段、非性能敏感场景。

#### A.2.2 jemalloc

由 Jason Evans 开发，最初为 FreeBSD 设计，后被 Facebook 大规模采用。Rust 编译器自身就使用 jemalloc。

**核心优势**：
- **碎片控制极好**：基于 arena 的独立堆，避免跨线程碎片
- **NUMA aware**：感知 NUMA 拓扑，优化跨节点访问
- **profiling 支持**：内置 heap profiling、leak checking
- **大规模验证**：在 Facebook 数百万台服务器上运行

**核心劣势**：
- **体积较大**：二进制增加 ~200KB
- **配置复杂**：大量调优选项，学习曲线陡峭
- **单线程场景无明显优势**：多线程优化在单线程下用不上

**适用场景**：多线程服务、大规模部署。

#### A.2.3 mimalloc

由 Microsoft Research 开发，专注于极低延迟。

**核心优势**：
- **延迟极低**：free list sharding + local free list，分配/释放延迟为业界最低之一
- **体积小**：~10KB，适合嵌入式/微内核
- **单线程优秀**：无锁设计在单线程下同样高效
- **安全性**：内置 heap corruption detection、guard pages
- **Rust 集成成熟**：`mimalloc-rust` crate 维护良好

**核心劣势**：
- **相对较新**：2019 年发布，不如 jemalloc 久经考验
- **NUMA 优化不如 jemalloc**：对大规模 NUMA 系统支持较弱

**适用场景**：用户态服务、微内核、低延迟系统。

#### A.2.4 snmalloc

由 Microsoft Research 开发，专为消息传递系统设计。

**核心优势**：
- **消息传递优化**：allocator 状态随消息传递，减少跨核心同步
- **微内核友好**：设计理念与微内核架构高度契合
- **安全**：基于 CHERI 架构的内存安全

**核心劣势**：
- **生态较小**：社区和文档不如 jemalloc/mimalloc
- **Rust crate 不够成熟**：`snmalloc-rs` 维护频率较低

**适用场景**：微内核系统、消息传递架构。

#### A.2.5 tlsf

Two-Level Segregated Fit，专为实时系统设计。

**核心优势**：
- **O(1) 分配/释放**：有界响应时间，适合硬实时系统
- **实现简单**：~400 行 C 代码

**核心劣势**：
- **碎片率较高**：实时性优先于内存效率
- **无 Rust 官方 crate**：需要自行封装 FFI

**适用场景**：硬实时嵌入式系统。

### A.3 对比矩阵

| 维度 | 系统默认 | jemalloc | mimalloc | snmalloc | tlsf |
|------|---------|----------|----------|----------|------|
| 分配延迟 | 中 | 低 | **极低** | 低 | O(1) |
| 碎片控制 | 中 | **极好** | 好 | 好 | 差 |
| 多线程 | 中 | **极好** | 好 | 好 | N/A |
| 单线程 | 中 | 好 | **极好** | 好 | 好 |
| 二进制大小 | 0 | ~200KB | **~10KB** | ~50KB | ~5KB |
| NUMA | 差 | **极好** | 中 | 中 | N/A |
| 安全性 | 中 | 中 | **好** | 好 | 中 |
| Rust 集成 | 内置 | 成熟 | **成熟** | 一般 | 无 |
| 成熟度 | **极高** | 极高 | 高 | 中 | 高 |

### A.4 本项目选择及理由

```
阶段 1（当前）：系统默认分配器

  理由：
  1. 零配置，先跑通系统
  2. VM 是单线程用户态进程，默认分配器已足够
  3. 没有 profiling 数据证明需要更换
  4. 不引入额外依赖，降低复杂度

阶段 2（如需优化）：mimalloc

  理由：
  1. 单线程场景延迟最低
  2. 体积小（~10KB），适合微内核
  3. 与 Rust 集成成熟（mimalloc-rust crate）
  4. 内置安全检测（heap corruption detection）

不推荐 jemalloc 的原因：
  1. VM 是单线程，jemalloc 的多线程优化用不上
  2. 体积较大（~200KB），引入不必要的复杂度
  3. 配置复杂，维护成本高

不推荐 snmalloc 的原因：
  1. Rust crate 不够成熟
  2. 消息传递优化在 VM 场景下收益有限
```

### A.5 未来切换指南

如果 profiling 确认需要切换分配器，只需修改 `global.rs`：

```rust
// 切换到 mimalloc
use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;
```

无需修改任何业务代码。这是 `#[global_allocator]` 机制的核心优势——分配器是**可插拔**的。

---

## 附录 B：Minix3 Slab 的 32 位局限性

Minix3 的 slab 设计深受 32 位地址空间限制的影响：

**1. 4GB 地址空间上限**

32 位系统虚拟地址空间最大 4GB。Minix3 的 VM 服务器作为一个用户态进程，可用的地址空间更小（通常 < 2GB）。这迫使设计者精打细算：

- 位图管理：每对象 1 bit，理论最低元数据开销
- 线性 8-200 字节分级：避免几何级数带来的 slabheader 浪费
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

在 64 位下，元数据开销几乎可以忽略。这从根本上改变了设计权衡——不再需要为了节省几 KB 而引入复杂的位图管理。

> **设计启发**：软件架构应服务于当前的硬件现实。在地址空间充沛的 64 位时代，过度复杂的私有管理逻辑只会增加 Bug 的温床。保留 Slab 的设计方案，是为了在未来面对"每秒百万级"的特定对象分配需求时，依然握有一把手术刀级的优化工具。

---

*分类: VM私有 | 使用范围: 仅 VM 内部*