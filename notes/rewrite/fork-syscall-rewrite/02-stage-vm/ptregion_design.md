# PtRegion 设计文档

> **归档标注**：本文档记录了 PtRegion 的完整设计过程。PtRegion 作为方案三（Typestate+PtRegion）的完整描述保留，但最终方案为方案四（Direct Map）。§8 描述了 Direct Map 方案及其与 PtRegion 的关系。本文档的设计演进叙述已融入 05-vm-allocpage.md 的 §3.1-3.6。
>
> **保留价值**：(1) §1-7 展示了 PtRegion 的完整设计推导，帮助读者理解"为什么 PtRegion 是正确的局部解"；(2) §8 展示了 Direct Map 如何超越 PtRegion，帮助读者理解"为什么 Direct Map 是更好的全局解"。保留这份文档，就是保留设计思考的轨迹。

> **分类**: 模块私有  
> **源码**: `minix3/minix/servers/vm/pagetable.c`, `minix3/minix/servers/vm/alloc.c`  
> **说明**: 页表页专用虚拟地址区域设计，解决 vm_allocpage 递归分配问题  
> **状态**: 已实现于 `os/servers/vm/src/pt_region.rs`

---

## 1. 问题背景

### 1.1 vm_allocpage 的递归困境

在 Minix3 中，`vm_allocpage()` 是 VM 服务器的核心函数，用于为进程分配物理页并建立页表映射。其典型调用链如下：

```
vm_allocpage(pages) → alloc_mem(pages) → 分配物理页
                    → vm_mappages(virt, phys) → 建立页表映射
                                            → pt_ptalloc_in_range(virt)
                                            → pt_ptalloc(pde) 分配新页表页
                                            → vm_allocpage(1)  ← 递归！
```

**递归发生的条件**：
- 当 `vm_mappages` 映射的虚拟地址落入一个新的 2MB 区域（x86-64 的 PT 覆盖范围）
- 而该区域的页表页（PT）尚未分配时
- `pt_ptalloc_in_range` → `pt_ptalloc` 需要调用 `vm_allocpage` 来分配页表页
- 这就形成了递归

### 1.2 递归的危险性

递归在操作系统内核中是极其危险的：

1. **栈溢出风险**：内核栈通常很小（如 4KB-16KB），递归深度不可控可能导致栈溢出
2. **死锁风险**：如果递归路径需要持有锁，可能与外层调用形成死锁
3. **资源耗尽**：递归调用可能持续分配资源，无法终止
4. **调试困难**：递归 bug 难以复现和定位

在 `vm_allocpage` 的场景中，递归还特别危险，因为：
- 分配页表页需要虚拟地址
- 获取虚拟地址需要 `find_hole` 搜索地址空间
- 搜索可能落入另一个未映射区域
- 再次触发页表页分配...

### 1.3 Minix3 的解决方案：spare_pagequeue

Minix3 采用 **备用页池（spare page queue）** 机制来解决这个问题。

#### 1.3.1 核心思想

预先分配一批页表页，存放在一个专门的队列中。当 `ensure_tables` 需要页表页时，直接从队列中取，而不是调用 `vm_allocpage`。

#### 1.3.2 关键数据结构

```c
// minix3/minix/servers/vm/alloc.c

// 备用页池大小（根据构建配置不同）
#ifdef SANITYCHECKS
#define SPAREPAGES 200    // 调试构建：200页
#elif defined(__arm__)
#define SPAREPAGES 150    // ARM 生产构建：150页
#else
#define SPAREPAGES 20     // x86 生产构建：20页
#endif

// 备用页池队列
static struct spare_page {
    phys_clicks phys;     // 物理页框号
    struct spare_page *next;
} spare_page_queue[SPAREPAGES];

static int spare_page_count = 0;
static struct spare_page *spare_page_head = NULL;
```

#### 1.3.3 关键函数

**初始化备用页池**（`pt_init` 时调用）：

```c
// minix3/minix/servers/vm/alloc.c:pt_init_alloc
void pt_init_alloc(void) {
    int i;
    struct spare_page *sp;
    
    // 预先分配 SPAREPAGES 个物理页
    for (i = 0; i < SPAREPAGES; i++) {
        sp = &spare_page_queue[i];
        sp->phys = alloc_mem(1);  // 分配1页
        if (sp->phys == NO_MEM) {
            // 分配失败，标记为无效
            sp->phys = NO_MEM;
            continue;
        }
        // 加入队列
        sp->next = spare_page_head;
        spare_page_head = sp;
        spare_page_count++;
    }
}
```

**从备用页池取页**（`pt_ptalloc` 使用）：

```c
// minix3/minix/servers/vm/alloc.c:get_spare_page
phys_clicks get_spare_page(void) {
    struct spare_page *sp;
    phys_clicks phys;
    
    if (spare_page_head == NULL) {
        // 备用页池耗尽！这是一个严重错误
        // Minix3 会尝试紧急分配，或 panic
        printf("VM: spare page queue exhausted!\n");
        return alloc_mem(1);  // 冒险直接分配
    }
    
    // 从队列头部取一个页
    sp = spare_page_head;
    spare_page_head = sp->next;
    spare_page_count--;
    
    phys = sp->phys;
    sp->phys = NO_MEM;  // 标记为已使用
    sp->next = NULL;
    
    return phys;
}
```

**补充备用页池**（`vm_allocpage` 成功后）：

```c
// minix3/minix/servers/vm/alloc.c:replenish_spare_pages
void replenish_spare_pages(void) {
    struct spare_page *sp;
    
    // 如果备用页池未满，补充一个页
    if (spare_page_count < SPAREPAGES) {
        // 找一个空闲槽位
        for (sp = spare_page_queue; sp < &spare_page_queue[SPAREPAGES]; sp++) {
            if (sp->phys == NO_MEM) {
                sp->phys = alloc_mem(1);
                if (sp->phys != NO_MEM) {
                    sp->next = spare_page_head;
                    spare_page_head = sp;
                    spare_page_count++;
                }
                break;
            }
        }
    }
}
```

#### 1.3.4 使用场景

在 `pt_ptalloc` 中，当需要分配新页表页时：

```c
// minix3/minix/servers/vm/pagetable.c:pt_ptalloc (简化)
static int pt_ptalloc(pt_t *pt, int pde, u32_t flags) {
    // ... 计算索引 ...
    
    if (pt->pt_pt[pde] == NULL) {
        // 需要分配新页表页
        phys_bytes pt_phys;
        
        // 关键：使用备用页池，而不是 alloc_mem！
        pt_phys = get_spare_page();
        if (pt_phys == NO_MEM) {
            return ENOMEM;
        }
        
        // 清零页表页
        // ...
        
        // 设置 PDE
        pt->pt_pt[pt_index] = pt_phys | I386_VM_PRESENT | I386_VM_WRITE;
    }
    
    // ... 继续处理下一级 ...
}
```

#### 1.3.5 Minix3 方案的问题

1. **固定大小**：`SPAREPAGES` 是编译时常量，无法动态调整
   - 太小：频繁耗尽，触发紧急分配
   - 太大：浪费物理内存

2. **耗尽风险**：如果递归频繁发生（如大量稀疏映射），备用页池可能耗尽
   - 耗尽后 Minix3 尝试紧急分配，这本身可能触发新的递归

3. **复杂性**：需要维护队列、计数器、补充逻辑
   - 代码分散在多个文件（`alloc.c`, `pagetable.c`）
   - 状态管理复杂（`spare_page_count`, `spare_page_head`）

4. **难以验证**：备用页池的正确性依赖于运行时行为
   - 静态分析难以证明"不会耗尽"
   - 测试需要构造极端场景

---

## 2. PtRegion 设计

### 2.1 核心思想转变

Minix3 的方案是 **"预先准备，以防万一"**：
- 担心递归发生，所以预先分配好备用页
- 递归发生时，从备用池中取

PtRegion 的方案是 **"结构性消除递归根源"**：
- 不让递归有发生的机会
- 页表页的虚拟地址不走 `find_hole + vm_mappages`
- 而是从一个**预留给页表页的专用区域**中直接切出

### 2.2 关键洞察

递归的根源是：**页表页的虚拟地址分配需要走通用路径，而通用路径可能需要分配页表页**。

如果页表页的虚拟地址分配是**独立的、不依赖通用映射机制**的，递归就无从发生。

### 2.3 内核基础设施：物理内存的偏移映射

PtRegion 的设计依赖于一个关键的内核基础设施决策：**所有物理内存通过偏移映射（direct map / linear map）映射到内核虚拟地址空间**。

#### 2.3.1 设计决策

**决策**：内核启动时在高位虚拟地址建立所有物理内存的线性映射，关系为：
```
va = pa + DIRECT_MAP_BASE
```

其中 `DIRECT_MAP_BASE` 通常选择高位地址，如 `0xffff_8880_0000_0000`（Linux x86-64 典型值）。

**与 Minix3 的差异**：
- Minix3（x86-32）：使用 `pagedir_mappings` 登记册，内核通过特殊页表间接访问进程页目录
- minix-rs（x86-64）：使用直接映射区，内核通过 `phys_to_virt(pa)` 直接访问任意物理页

#### 2.3.2 对 PtRegion 的影响

这个决策使 PtRegion 的扩展机制成为可能：

```rust
// PtRegion::expand 中的关键操作
fn expand(&mut self) -> Option<()> {
    // 1. 分配物理页
    let pt_phys = self.phys_alloc.alloc_pages(1)?;
    
    // 2. 通过偏移映射直接访问物理页（无需建立页表映射）
    let pt_virt = phys_to_virt(pt_phys);  // 简单加法：va = pa + DIRECT_MAP_BASE
    
    // 3. 直接清零页表页（通过内核虚拟地址）
    unsafe { core::ptr::write_bytes(pt_virt.as_mut_ptr(), 0, PAGE_SIZE) };
    
    // 4. 写 PTE/PDE（操作的是 PtRegion 的页表页，已映射）
    self.pt_ops.write_pte(self.current_pt, pt_index, pt_phys.as_u64() | flags);
    
    Some(())
}
```

**关键点**：
- 新分配的物理页**不需要**通过 `vm_mappages` 建立映射
- 内核通过偏移映射直接访问，避免了递归
- PtRegion 只需要管理**页表页自身的虚拟地址**（用于进程页表），而不需要为**页表页的内容**分配虚拟地址

#### 2.3.3 关键发现：Minix3 内核的地址空间限制

通过深入分析 Minix3 内核源码，发现一个关键事实：**Minix3 内核并没有真正的"恒等映射所有物理内存"**。

**Minix3 内核页表的真实结构**（`pg_utils.c`）：

```c
// 1. 恒等映射（identity mapping）- 仅用于启动
void pg_identity(kinfo_t *cbi) {
    for(i = 0; i < I386_VM_DIR_ENTRIES; i++) {
        phys = i * I386_BIG_PAGE_SIZE;  // 4MB 对齐
        // 映射 0-4GB 物理内存，但超过 mem_high_phys 的标记为 non-cacheable
        pagedir[i] = phys | flags;
    }
}

// 2. 内核映射 - 仅映射内核自身
int pg_mapkernel(void) {
    pde = kern_vir_start / I386_BIG_PAGE_SIZE;
    while(mapped < kern_kernlen) {
        pagedir[pde] = kern_phys | I386_VM_PRESENT | I386_VM_BIGPAGE | I386_VM_WRITE;
        mapped += I386_BIG_PAGE_SIZE;
        kern_phys += I386_BIG_PAGE_SIZE;
        pde++;
    }
    return pde;  // 返回第一个空闲 PDE
}
```

**关键点**：
1. **启动时的恒等映射**：`pg_identity()` 创建 0-4GB 的恒等映射，但这只是为了**启动期间**代码能正常运行
2. **启动后**：`pg_mapkernel()` 只映射**内核自身**（通常几百 KB 到几 MB）
3. **空闲 PDE**：`pg_mapkernel()` 返回的 `freepde_start` 之后的 PDE 是**空闲的**，用于动态映射

**这就是为什么 Minix3 需要 `createpde` 机制**：

```c
// kernel/arch/i386/memory.c:createpde
static phys_bytes createpde(...) {
    if(pr && ((pr == get_cpulocal_var(ptproc)) || iskernelp(pr))) {
        // 内核任务或当前进程已在页表中，直接访问
        return linaddr;
    }
    
    // 需要临时映射其他进程的内存或物理内存
    // 使用 freepdes[] 中的一个空闲 PDE 建立 4MB 窗口映射
    pde = freepdes[free_pde_idx];
    get_cpulocal_var(ptproc)->p_seg.p_cr3_v[pde] = pdeval;  // 临时映射
    return I386_BIG_PAGE_SIZE*pde + offset;
}
```

**Minix3 内核任务共享同一个页目录**，但这个页目录**只包含**：
- 内核自身的映射（`pg_mapkernel` 建立的）
- 当前用户进程的页目录映射（通过 `pagedir_mappings`）
- **2 个空闲 PDE**（`freepdes[0]` 和 `freepdes[1]`）用于临时映射

**这就是为什么 `sys_datacopy` 需要 `createpde`**：
- 内核页表**没有**映射所有物理内存
- 访问其他进程的内存或任意物理地址时，需要**临时借用**一个 PDE 建立 4MB 窗口映射
- 这就是为什么叫 "freepde"（空闲 PDE）机制

#### 2.3.4 架构对比

| 特性 | Minix3 (x86-32) | minix-rs (x86-64) |
|------|-----------------|-------------------|
| 内核页表内容 | 仅内核自身 + 2 个临时映射窗口 | 直接映射区（所有物理内存） |
| 访问物理内存 | 通过 `createpde` 临时映射 | `phys_to_virt()` 直接访问 |
| 访问其他进程内存 | 通过 `createpde` 临时映射 | 通过进程页表切换或共享映射 |
| 临时映射窗口 | 2 个 4MB 窗口（`freepdes[]`） | 不需要 |
| 复杂度 | 高（需要管理临时映射） | 低（直接访问） |

**结论**：
- Minix3 的"恒等映射"只是**启动时**的临时措施，启动后内核页表**非常精简**
- minix-rs 的"直接映射区"是**真正的**所有物理内存映射，内核可以直接访问任意物理地址
- 这是 x86-64 架构的优势（大地址空间），也是 minix-rs 能够简化设计的关键

**对 PtRegion 的影响**：
- Minix3 中，VM 服务器作为用户态进程，其页表由 VM 自己管理
- 内核通过 `freepde` 机制临时映射 VM 的页表来访问
- minix-rs 中，内核可以直接访问 VM 的页表（通过直接映射区），不需要临时映射机制

#### 2.3.5 内核任务的页表情况

通过分析 Minix3 内核源码，发现 **5 个内核任务（ASYNCM, IDLE, CLOCK, SYSTEM, HARDWARE）都没有独立的页表**：

```c
// kernel/arch/i386/memory.c:28
#define HASPT(procptr) ((procptr)->p_seg.p_cr3 != 0)

// kernel/proc.c:454 - 只断言用户进程有页表
assert(p->p_seg.p_cr3 != 0);  // 内核任务 p_cr3 = 0，不触发此断言
```

**内核任务的 p_cr3 值**：

| 任务 | p_cr3 | 页表情况 |
|------|-------|----------|
| ASYNCM | 0 | 无独立页表，使用当前进程的页表 |
| IDLE | 0 | 无独立页表，`switch_address_space_idle()` 切换到 VM 的页表 |
| CLOCK | 0 | 无独立页表 |
| SYSTEM | 0 | 无独立页表 |
| HARDWARE | 0 | 无独立页表 |

**内核任务如何访问内存**：

```c
// kernel/arch/i386/memory.c:85
createpde(...) {
    if(pr && ((pr == get_cpulocal_var(ptproc)) || iskernelp(pr))) {
        // 内核任务（iskernelp(pr)）被认为"已经在当前页表中"
        // 直接返回线性地址，不建立新映射
        return linaddr;
    }
    // ... 需要临时映射的情况
}
```

**关键结论**：
- 内核任务**不切换 CR3**，它们复用当前加载的页表
- 当 IDLE 任务运行时，它会显式切换到 VM 的页表（`switch_address_space_idle()`）
- 其他内核任务（CLOCK, SYSTEM 等）在执行时，依赖当前被调度的进程的页表

#### 2.3.6 关于"Minix3 保持内核纯净"的说法辨析

**常见误解**："Minix3 保持内核纯净，是教条的微内核架构，所以才没有在内核中保留内存映射"

**事实**：
- ❌ **不是**因为"教条式微内核架构"
- ✅ 是因为 **x86-32 的地址空间限制**（4GB）
- ✅ 内核只映射自身（几百 KB 到几 MB）是为了**节省页表项和地址空间**
- ✅ `freepde` 机制（2 个 4MB 临时窗口）是**工程权衡**，不是设计原则

**对比**：
- Minix3（x86-32）：地址空间紧张，需要精打细算
- minix-rs（x86-64）：128TB+ 地址空间，可以直接映射所有物理内存

**待思考的问题**：minix-rs 的直接映射区是**利用 x86-64 架构优势的工程改进**，但是否与 PtRegion 设计存在不一致？

---

## 3. 待解决的设计问题

### 3.1 内核偏移映射与 VM PtRegion 的关系

**观察到的现象**：
- 内核使用偏移映射（Direct Map）访问所有物理内存
- VM 使用 PtRegion 管理页表页的虚拟地址
- 两套机制似乎解决相似问题（虚拟地址 ↔ 物理地址映射）

**用户的疑问**：
1. 这是否是"一个问题，两套解决方案"？
2. 是否应该统一为一套机制？
3. 或者保持"内核特殊，VM 特殊"的分层设计？

### 3.2 需要查证的关键问题

#### 3.2.1 Minix3 中内核是否映射到每个进程地址空间？

查证结果（`minix3/minix/servers/vm/pagetable.c:pt_mapkernel`）：
```c
int pt_mapkernel(pt_t *pt)
{
    // 每个进程的页表都映射内核地址空间
    while(mapped < kern_size) {
        pt->pt_dir[kern_pde] = addr | ARCH_VM_PDE_PRESENT |
            ARCH_VM_BIGPAGE | ARCH_VM_PTE_RW | global_bit;
        kern_pde++;
        mapped += ARCH_BIG_PAGE_SIZE;
        addr += ARCH_BIG_PAGE_SIZE;
    }
    // ...
}
```

**发现**：Minix3 中，**每个用户进程的页表都包含内核的映射**（通过 `pt_mapkernel`）。

#### 3.2.2 如果 minix-rs 也采用类似设计，加上偏移映射，会有什么后果？

**假设的场景**：
```
进程地址空间布局：
0x0000_0000_0000_0000  ┬── 用户空间（低地址）
                       │    （进程代码、数据、堆、栈）
                       │
0x0000_7FFF_FFFF_FFFF  ┴── 用户空间结束（128TB）

0xFFFF_8000_0000_0000  ┬── 内核空间（高地址）
                       │   ┌── 直接映射区（所有物理内存）
                       │   │   va = pa + DIRECT_MAP_BASE
                       │   │   【问题：这是否在每个进程页表中都存在？】
                       │   └── ...
```

**潜在问题**：
- 如果每个进程页表都包含完整的物理内存映射（偏移映射区）
- 用户进程可能通过某种方式访问内核的直接映射区
- 这是否构成**安全隐患**？

#### 3.2.3 安全考虑

**如果进程页表包含偏移映射区**：

| 风险 | 说明 |
|------|------|
| 信息泄露 | 进程可以读取其他进程的物理页内容 |
| 权限绕过 | 进程可以直接修改内核数据结构 |
| 隔离破坏 | 破坏了用户/内核的隔离边界 |

**可能的缓解措施**：
- 用户态无法访问高地址（需要 ring 0）
- 但页表项本身如果可被用户态修改...

### 3.3 可能的设计方向（待评估）

#### 方向 A：保持分层（当前设计）
- 内核：使用偏移映射（内核专属，不在进程页表中）
- VM：使用 PtRegion（VM 作为用户态进程的自我管理）
- **优点**：职责清晰，安全边界明确
- **缺点**：两套机制，可能冗余

#### 方向 B：统一机制（待论证）
- 内核和 VM 都使用偏移映射
- 或者都使用类似 PtRegion 的机制
- **优点**：机制统一
- **缺点**：VM 作为用户态进程，如何访问内核偏移映射区？

#### 方向 C：内核特殊化（待论证）
- 内核使用偏移映射
- VM 通过系统调用请求内核建立映射
- **优点**：内核完全控制映射
- **缺点**：性能开销，增加内核-VM 交互

### 3.4 建议的后续调查

1. **查证 Linux/x86-64 的做法**：
   - 内核直接映射区是否出现在用户进程页表中？
   - 如何防止用户态访问直接映射区？

2. **评估安全模型**：
   - 如果进程页表包含偏移映射区，实际风险是什么？
   - 硬件保护（ring 0/3）是否足够？

3. **性能评估**：
   - PtRegion 的 bump allocator  vs  偏移映射的直接访问
   - 哪种方式对 VM 操作更高效？

---

**注**：以上问题需要进一步研究和论证，当前文档仅记录问题，不下结论。

### 2.4 设计概述

```
VM 虚拟地址空间布局（x86-64）：

0x0000_0000_0000_0000  ┬── 用户空间（低地址）
                       │    （进程代码、数据、堆、栈）
                       │
0x0000_7FFF_FFFF_FFFF  ┴── 用户空间结束（128TB）

0x0000_7F00_0000_0000  ┬── PtRegion：页表页专用区域（起始 2MB，可增长）
                       │   ┌── PDPT page (4KB)     ← 占用 slot 0
                       │   ├── PD page   (4KB)     ← 占用 slot 1
                       │   ├── PT[0]     (4KB)     ← 占用 slot 2, 提供 512 slots
                       │   ├── slot 3: 可用
                       │   ├── ...
                       │   ├── slot 511: 可用
                       │   │
                       │   │  快用完时扩展:
                       │   ├── PT[1]     (4KB)     ← 占用 1 slot, 提供 512 slots
                       │   ├── ...
0x0000_7F00_0020_0000  ┴── PtRegion 结束（初始 2MB）

0x0000_7F00_0020_0000  ─── 通用映射区域（find_hole 从这里开始）
                       │    （进程 mmap、共享内存等）
                       │
0x0000_7FFF_FFFF_FFFF  ┴── 用户空间结束

0xFFFF_8000_0000_0000  ─── 内核空间（高地址）
```

**关键设计点**：

1. **专用区域**：`0x7F00_0000_0000` 开始的区域专门用于页表页
2. **Bump Allocator**：按序分配，不搜索，不递归
3. **自举友好**：初始 3 页（PDPT/PD/PT[0]）在初始化时切出，不依赖堆
4. **动态扩展**：当 slot 用完时，分配新 PT 页并继续，扩展过程不递归

### 2.5 与 Minix3 的对比

| 维度 | Minix3 (spare_pagequeue) | PtRegion |
|------|-------------------------|----------|
| **核心思想** | 预先准备，应对递归 | 结构性消除递归根源 |
| **虚拟地址获取** | `find_hole` 搜索地址空间 | Bump allocator 直接切出 |
| **物理页获取** | 从备用池取（可能耗尽） | 直接 bitmap 分配 |
| **容量限制** | 固定 `SPAREPAGES` | 动态扩展（1GB 上限） |
| **实现复杂度** | 队列管理、补充逻辑 | 简单的 bump allocator |
| **可验证性** | 运行时依赖，难静态分析 | 结构清晰，易分析 |
| **内存开销** | 预分配 `SPAREPAGES` 页 | 按需分配，无预留 |

### 2.6 数据结构

```rust
// os/servers/vm/src/pt_region.rs

/// 页表页专用虚拟地址区域
/// 
/// 管理一段预留给页表页的虚拟地址空间，使用 bump allocator 按序分配。
/// 消除 vm_allocpage 递归的根源：页表页的虚拟地址不再走 find_hole + vm_mappages。
pub(crate) struct PtRegion<O: PtOps> {
    /// 区域起始虚拟地址
    start: VirBytes,
    /// 已映射区域的结束（exclusive）
    mapped_end: VirBytes,
    /// 下一个可用 slot 的虚拟地址
    next: VirBytes,
    
    /// PDPT 页虚拟地址（固定，初始化时设置）
    pdpt_page: VirBytes,
    /// 当前 PD 页虚拟地址（初始时设置，扩展时可能变化）
    pd_page: VirBytes,
    /// 当前 PT 页虚拟地址（bump allocator 当前工作页）
    current_pt: VirBytes,
    /// 当前 PT 页覆盖的虚拟地址基址
    current_pt_base: VirBytes,
    
    /// 物理页分配器
    phys_alloc: Box<dyn PhysAllocator>,
    /// 页表操作抽象（真实硬件或 mock）
    pt_ops: O,
}

/// 页表操作 trait，用于测试注入
pub(crate) trait PtOps {
    fn write_pte(&mut self, table_virt: VirBytes, index: usize, entry: u64);
    fn write_pde(&mut self, table_virt: VirBytes, index: usize, entry: u64);
    fn write_pdpte(&mut self, table_virt: VirBytes, index: usize, entry: u64);
    fn read_pte(&self, table_virt: VirBytes, index: usize) -> u64;
    fn read_pde(&self, table_virt: VirBytes, index: usize) -> u64;
    fn read_pdpte(&self, table_virt: VirBytes, index: usize) -> u64;
}
```

### 2.7 关键操作

#### 2.7.1 初始化

```rust
impl PtRegion<RealPtOps> {
    pub(crate) fn from_reserved(
        reserved: &mut ReservedRegion,
        phys_alloc: Box<dyn PhysAllocator>,
    ) -> Self {
        Self::from_reserved_with_ops(reserved, phys_alloc, RealPtOps)
    }
}

impl<O: PtOps> PtRegion<O> {
    pub(crate) fn from_reserved_with_ops(
        reserved: &mut ReservedRegion,
        phys_alloc: Box<dyn PhysAllocator>,
        pt_ops: O,
    ) -> Self {
        // 从预留区域切出 3 页连续虚拟地址
        let start = reserved.alloc_contig_virt(3);
        let pd_page = VirBytes(start.0 + PAGE_SIZE as u64);
        let current_pt = VirBytes(start.0 + 2 * PAGE_SIZE as u64);

        // 获取物理地址（内核保证已映射）
        let pdpt_phys = reserved.virt_to_phys(start);
        let pd_phys = reserved.virt_to_phys(pd_page);
        let pt0_phys = reserved.virt_to_phys(current_pt);

        // 初始化页表结构
        // ... 清零页表页 ...
        // ... 设置自映射 ...
        
        PtRegion {
            start,
            mapped_end: VirBytes(start.0 + 3 * PAGE_SIZE as u64),
            next: VirBytes(start.0 + 3 * PAGE_SIZE as u64),
            pdpt_page: start,
            pd_page,
            current_pt,
            current_pt_base: start,
            phys_alloc,
            pt_ops,
        }
    }
}
```

**关键点**：
- 从 `ReservedRegion` 切出 3 页，不依赖堆分配
- 物理页由内核保证已映射（Bootstrap 阶段）
- 初始化后即可用于分配页表页

#### 2.7.2 分配页表页虚拟地址

```rust
impl<O: PtOps> PtRegion<O> {
    /// 分配一个页表页的虚拟地址
    /// 
    /// 使用 bump allocator，按序分配。如果当前 PT 页已满，自动扩展。
    pub(crate) fn alloc_pt_page(&mut self) -> Option<VirBytes> {
        // 检查是否还有可用 slot
        if self.next >= self.mapped_end {
            // 需要扩展
            self.expand()?;
        }
        
        let virt = self.next;
        self.next = VirBytes(self.next.0 + PAGE_SIZE as u64);
        Some(virt)
    }
}
```

**关键点**：
- 简单的 bump allocator，O(1) 时间
- 不搜索地址空间，不调用 `find_hole`
- 不触发 `vm_mappages`

#### 2.7.3 扩展（关键：不递归）

```rust
impl<O: PtOps> PtRegion<O> {
    /// 扩展 PtRegion，分配新的 PT 页
    /// 
    /// 这是唯一可能"分配页表页"的操作，但它是自举的：
    /// - 新 PT 页的虚拟地址来自当前 PT 的 slot（已映射）
    /// - 新 PT 页的物理页来自 phys_alloc（bitmap，不递归）
    /// - 写 PTE/PDE 操作的是已映射的页表页
    fn expand(&mut self) -> Option<()> {
        // 1. 分配物理页（bitmap，纯物理分配，不递归）
        let pt_phys = self.phys_alloc.alloc_pages(1)?;
        
        // 2. 获取虚拟地址（当前 PT 的下一个 slot，已映射）
        let pt_virt = self.next;
        
        // 3. 计算索引
        let pt_index = ((pt_virt.0 - self.current_pt_base.0) / PAGE_SIZE as u64) as usize;
        
        // 4. 写 PTE：建立虚拟地址到新物理页的映射
        //    current_pt 已映射，直接写，不触发 pt_ptalloc
        self.pt_ops.write_pte(self.current_pt, pt_index, pt_phys.as_u64() | flags);
        
        // 5. 清零新 PT 页（通过刚映射的虚拟地址）
        //    直接访问，不触发页错误
        unsafe { core::ptr::write_bytes(pt_virt.0 as *mut u8, 0, PAGE_SIZE) };
        
        // 6. 检查是否需要新的 PD
        let pd_index = ((pt_virt.0 >> 21) & 0x1FF) as usize;  // 2MB 区域索引
        // ... 如果 PD 已满，类似地分配新 PD ...
        
        // 7. 更新状态
        self.current_pt = pt_virt;
        self.current_pt_base = VirBytes(pt_virt.0 & !(0x1FFFFF));  // 2MB 对齐
        self.mapped_end = VirBytes(self.mapped_end.0 + PAGE_SIZE as u64);
        
        Some(())
    }
}
```

**扩展过程的递归检查**：

| 步骤 | 操作 | 需要什么 | 来源 | 触发递归？ |
|------|------|----------|------|-----------|
| 1 | `alloc_phys` | 物理页 | `phys_alloc` bitmap | ❌ 纯物理分配 |
| 2 | 取虚拟地址 | slot | `next`（当前 PT 已映射） | ❌ 已映射 |
| 3 | 写 PTE | 写页表 | `current_pt`（已映射） | ❌ 已映射 |
| 4 | 清零 | 写内存 | `pt_virt`（刚映射） | ❌ 已映射 |
| 5 | 写 PDE | 写页表 | `pd_page`（已映射） | ❌ 已映射 |

**全部操作的对象都是已映射的页表页，不触发 `pt_ptalloc`，不递归。**

### 2.8 递归的结构性消除

对比 Minix3 和 PtRegion 的递归处理：

**Minix3**：
```
vm_allocpage → vm_mappages → pt_ptalloc_in_range → pt_ptalloc → 需要页表页
                                                              ↓
                                                        get_spare_page（从备用池取）
                                                              ↓
                                                        如果备用池耗尽 → 紧急分配 → 可能递归
```

**PtRegion**：
```
vm_allocpage → alloc_phys（物理页，bitmap）
             → alloc_virt（虚拟地址，PtRegion bump allocator）
                          ↓
                    PtRegion::alloc_pt_page
                          ↓
                    如果 slot 可用 → 直接返回（O(1)，不递归）
                    如果 slot 用完 → expand
                                          ↓
                                    alloc_phys（物理页，bitmap，不递归）
                                    写 PTE（current_pt 已映射，不递归）
                                    写 PDE（pd_page 已映射，不递归）
```

**关键区别**：
- Minix3：递归可能发生，通过备用池"缓解"
- PtRegion：递归**结构上不可能发生**，因为页表页的虚拟地址分配不依赖通用映射机制

### 2.9 容量与增长

**初始容量**：
- 1 个 PT 页 = 512 slots
- 初始已用 3 页（PDPT/PD/PT[0]），剩余 509 slots
- 约 2MB 页表空间（512 × 4KB）

**扩展**：
- 当 PT[0] 满时，分配 PT[1]，新增 512 slots
- 1 个 PD 页可管理 512 个 PT 页 = 262,144 slots
- 262,144 × 4KB = 1GB 页表空间

**实际需求**：
- VM 自身映射只需几 MB
- 进程映射（假设 1000 个进程，每个 1GB 地址空间）约需 1000 × 512 = 512,000 slots
- 1GB 页表空间绰绰有余

### 2.10 地址选择

**PtRegion 起始地址**：`0x7F00_0000_0000`

选择理由：

1. **用户空间高位**：x86-64 Linux 用户空间为 `0x0000_0000_0000` 到 `0x0000_7FFF_FFFF_FFFF`（128TB）
   - `0x7F00...` 位于用户空间高位，与典型栈地址（`0x7FFF...`）有足够距离

2. **避开堆/栈**：
   - 用户堆从低地址向上增长
   - 用户栈从高地址向下增长
   - `0x7F00...` 位于中间，冲突风险低

3. **对齐友好**：`0x7F00_0000_0000` 是 1GB 对齐，便于页表计算

4. **可配置**：定义为常量 `PT_REGION_START`，便于调整

```rust
// os/servers/vm/src/pt_region.rs

/// PtRegion 起始虚拟地址
/// 
/// 选择 0x7F00_0000_0000 的理由：
/// 1. 位于 x86-64 用户空间高位（0x7FFF_FFFF_FFFF 以下）
/// 2. 与用户栈（典型 0x7FFF...）有足够距离
/// 3. 1GB 对齐，便于页表计算
/// 4. 远离用户堆（从低地址增长）
pub(crate) const PT_REGION_START: u64 = 0x7F00_0000_0000;

/// PtRegion 初始大小（2MB = 1 个 PT 页覆盖范围）
pub(crate) const PT_REGION_INITIAL_SIZE: usize = 2 * 1024 * 1024;  // 2MB
```

---

## 3. 设计决策与权衡

### 3.1 为什么选择 Bump Allocator？

**选项对比**：

| 方案 | 优点 | 缺点 |
|------|------|------|
| **Bump Allocator** | O(1) 分配，无碎片，实现简单 | 无释放（页表页通常不释放） |
| **Bitmap** | 可释放，紧凑 | 需要搜索，O(n) 或更复杂 |
| **链表** | 可释放，灵活 | 需要堆，初始化复杂 |

**决策**：选择 Bump Allocator

理由：
1. **页表页的生命周期**：页表页一旦分配，通常与进程生命周期绑定，很少单独释放
2. **性能**：O(1) 分配，无搜索开销
3. **简单性**：实现简单，易于验证正确性
4. **无碎片**：按序分配，无外部碎片

### 3.2 为什么从 ReservedRegion 初始化？

**背景**：VM 初始化时，堆尚未就绪（`alloc` crate 的全局分配器未初始化）。

**方案对比**：

| 方案 | 优点 | 缺点 |
|------|------|------|
| **ReservedRegion** | 不依赖堆，自举友好 | 需要预留区域 |
| **静态数组** | 简单 | 大小固定，浪费内存 |
| **早期堆** | 灵活 | 需要实现早期分配器 |

**决策**：从 `ReservedRegion` 切出 3 页初始化

理由：
1. **自举友好**：不依赖堆，VM 启动早期即可使用
2. **足够小**：只需 3 页（12KB），预留区域可承受
3. **与现有机制整合**：复用 `ReservedRegion` 的 `alloc_contig_virt`

### 3.3 为什么使用 Trait 抽象页表操作？

**PtOps trait**：

```rust
pub(crate) trait PtOps {
    fn write_pte(&mut self, table_virt: VirBytes, index: usize, entry: u64);
    fn write_pde(&mut self, table_virt: VirBytes, index: usize, entry: u64);
    // ...
}
```

**实现**：
- `RealPtOps`：操作真实硬件页表
- `MockPtOps`：测试中模拟页表操作

**决策理由**：
1. **可测试性**：单元测试不需要真实硬件，避免访问非法内存
2. **架构无关**：trait 定义 OS 语义，不暴露硬件细节
3. **零开销**：静态分派，编译后完全内联

### 3.4 与 Minix3 的兼容性

**外部可观察行为**：
- `vm_allocpage` 的 IPC 接口不变
- 返回值（虚拟地址、物理地址）语义不变
- 页表建立后的效果（进程可访问内存）不变

**内部实现差异**：
- Minix3：备用页池 + 通用映射路径
- PtRegion：专用虚拟地址区域 + bump allocator

**这是否是 Redesign？**

根据 review.md 的定义：
- **Rewrite**：外部语义不变，内部表达改变 ✅
- **Redesign**：改变系统架构、机制或协议 ❌

PtRegion 属于 **Rewrite**：
- 外部 IPC 接口不变
- 内存分配语义不变（分配一页，建立映射）
- 改变的是**内部实现策略**，从"备用池应对递归"变为"结构性消除递归"

这类似于：
- Minix3 用链表管理空闲页，Rust 版本用 bitmap —— 数据结构改变，语义不变
- Minix3 用 `int flags`，Rust 版本用 `enum` —— 表达改变，语义不变

### 3.5 替代方案考虑

**方案 A：保留 Minix3 的 spare_pagequeue**

```rust
struct SparePageQueue {
    pages: [Option<PhysBytes>; SPAREPAGES],
    head: usize,
    count: usize,
}
```

优点：
- 与 Minix3 代码对应清晰
- 实现简单

缺点：
- 固定大小，容量限制
- 耗尽风险
- 需要维护补充逻辑

**方案 B：动态备用池**

```rust
struct SparePageQueue {
    pages: Vec<PhysBytes>,  // 动态增长
    min_reserve: usize,
}
```

优点：
- 容量动态调整

缺点：
- 需要堆（初始化问题）
- 递归风险仍在，只是缓解

**最终选择：PtRegion**

理由：
- 结构性消除递归，而非缓解
- 无容量限制（动态扩展）
- 实现简洁，易于验证
- 符合 Rust 的"零成本抽象"理念

---

## 4. 实现状态

### 4.1 已完成

- [x] `PtRegion` 结构体定义
- [x] `PtOps` trait 定义及 `RealPtOps` / `MockPtOps` 实现
- [x] `from_reserved` 初始化
- [x] `alloc_pt_page` bump allocator
- [x] `expand` 自动扩展
- [x] 与 `VmPageAllocator` 集成

### 4.2 待验证

- [ ] 跨 2MB 边界扩展的正确性
- [ ] 多 PD 场景（>1GB 页表空间）
- [ ] 与进程 fork 的交互
- [ ] 性能基准（与 Minix3 对比）

### 4.3 相关文件

| 文件 | 说明 |
|------|------|
| `os/servers/vm/src/pt_region.rs` | PtRegion 实现 |
| `os/servers/vm/src/alloc_page.rs` | VmPageAllocator 集成 |
| `os/servers/vm/src/alloc_page.rs` | ReservedRegion 定义 |

---

## 5. 参见

- [05-vm-allocpage.md](05-vm-allocpage.md) - 页分配器整体设计
- [06-pagetable-struct.md](06-pagetable-struct.md) - 页表结构设计
- [07-pagetable-ops.md](07-pagetable-ops.md) - 页表操作
- [09-vm-relocation.md](09-vm-relocation.md) - 搬迁策略

---

## 6. 源码查证：VM 启动时内核 direct map 是否在 VM 页表中？

### 6.1 查证目标

回答核心问题：**VM 启动时，内核的高位 direct map 是否已经在 VM 的页表中？**

这个问题决定了 PtRegion 是否可以被 `phys_to_virt()` 替代。

### 6.2 Minix3 源码查证

#### 6.2.1 `pt_mapkernel` — 内核映射被放入每个进程页表

[pt_mapkernel](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/pagetable.c#L1442) 在 `pt_new()` 中被调用（[L1022](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/pagetable.c#L1022)），将内核映射写入每个进程的页目录：

```c
int pt_mapkernel(pt_t *pt)
{
    int kern_pde = kern_start_pde;
    phys_bytes addr = kern_mb_mod->mod_start;

    // 用 BIGPAGE (4MB) 条目映射内核自身
    while(mapped < kern_size) {
        pt->pt_dir[kern_pde] = addr | ARCH_VM_PDE_PRESENT |
            ARCH_VM_BIGPAGE | ARCH_VM_PTE_RW | global_bit;
        kern_pde++;
        mapped += ARCH_BIG_PAGE_SIZE;
        addr += ARCH_BIG_PAGE_SIZE;
    }

    // 映射 pagedir_mappings（页目录登记册）
    for(pd = 0; pd < MAX_PAGEDIR_PDES; pd++) {
        pt->pt_dir[pdm->pdeno] = pdm->val;
    }

    // 映射内核额外请求的映射（BIOS, APIC, 显存等）
    for(i = 0; i < kernmappings; i++) {
        pt_writemap(NULL, pt,
            kern_mappings[i].vir_addr,
            kern_mappings[i].phys_addr,
            kern_mappings[i].len,
            kern_mappings[i].flags, 0);
    }
}
```

**结论**：Minix3 中，**内核映射确实存在于每个进程的页表中**，包括 VM 自己的页表。

#### 6.2.2 权限位分析

关键证据在 `pt_init()` 的 `kern_mappings` 收集逻辑（[pagetable.c:L1181-L1237](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/pagetable.c#L1181-L1237)）：

```c
kern_mappings[pindex].flags = ARCH_VM_PTE_PRESENT;
if(flags & VMMF_USER)
    kern_mappings[pindex].flags |= ARCH_VM_PTE_USER;
if(flags & VMMF_WRITE)
    kern_mappings[pindex].flags |= ARCH_VM_PTE_RW;
```

**默认情况**（内核自身映射）：`ARCH_VM_PTE_RW | global_bit`，**没有** `ARCH_VM_PTE_USER`。

**特殊允许**（`VMMF_USER`）：某些特定的内核映射可以标记为 user-accessible。例如允许用户态直接访问帧缓冲（`VMMF_USER | VMMF_WRITE`）。

**结论**：内核映射在 VM 页表中**默认是 supervisor-only（U/S=0）**。VM 作为 Ring 3 进程，**无法访问这些映射**。

#### 6.2.3 内核 direct map 的权限

Minix3（x86-32）**没有**完整的物理内存 direct map——它只有内核自身 + 设备映射。但 minix-rs（x86-64）的设计决定引入 direct map。

如果按 Minix3 的模式将 direct map 放入进程页表：
- direct map 条目将是 supervisor-only（U/S=0）
- VM（Ring 3）**可以看到页表结构，但不能访问**
- VM 仍然需要自己的 VA → PA 映射机制

### 6.3 VM 初始化时序

从 [05-vm-allocpage.md §4.4](05-vm-allocpage.md) 的初始化时序：

```
T0: Kernel 启动 VM 进程
    ├── 创建初始页表（映射 .text, .rodata, .data, .bss）
    ├── 映射 reserved_region 到 VM 地址空间
    └── 传递 boot_info

T4: pt_init()
    → 创建页表系统

T5: allocator.into_normal()
    → 从 reserved 切出 PtRegion（3 页）
    → phys_alloc 所有权转移给 PtRegion
    → 此时 alloc_page() 走 alloc_phys + alloc_virt
```

**关键发现**：PtRegion 在 T5 创建，此时 `pt_init()` 已经完成。VM 的页表已经初始化，包含内核映射（supervisor-only）。

### 6.4 关键推论

1. **VM 页表中存在内核映射**（通过 `pt_mapkernel`），但是 supervisor-only（U/S=0）
2. **VM 不能通过这些映射访问物理内存**（Ring 3 进程）
3. **PtRegion 创建的虚拟地址是 user-accessible**（U/S=1），VM 可以访问
4. 如果 direct map 以 U/S=0 放入 VM 页表 → VM 不能使用 → PtRegion 仍然必要
5. 如果 direct map 以 U/S=1 放入 VM 页表 → VM 可以直接 `phys_to_virt()` → PtRegion 可以被简化

### 6.5 回答原始问题

> VM 启动时，内核的高位 direct map 是否已经在 VM 的页表中？

根据 Minix3 源码分析（这也是 minix-rs 应遵循的架构）：

- **是**，如果 direct map 遵循 `pt_mapkernel` 的模式（放在所有进程页表的高位地址中）
- **但是**，它会是 supervisor-only（U/S=0），VM（Ring 3）**不能直接使用**

---

## 7. 设计演进分析与建议

### 7.1 PtRegion 的历史起源

**用户自述原文**：
> 我就是在 allocpage 的过程中，发现循环，然后才设计了 pt_region，给物理页一个 VA。

PtRegion 的原始动机是**纯粹的自举工具**：
1. VM 需要分配页表页 → 页表页是 physical page → 需要 VA 来访问 → 获取 VA 走 `vm_mappages` → `vm_mappages` 需要页表页 → 递归！
2. PtRegion 的解法：预留一段专用 VA 区域 + bump allocator → 分配 VA 不触发 `vm_mappages` → 打破递归

这个原始动机**与 direct map 完全无关**——因为设计 PtRegion 时，内核 direct map 尚未被考虑。

### 7.2 direct map 出现后的演化

**用户自述原文**：
> 我之前就决定了 kernel 应该有偏移映射（但根本没考虑怎么实现，在哪里实现）。

这是一条关键的**时间线信息**：
1. **先有**：allocpage → 发现递归 → 设计 PtRegion
2. **后有**：决定内核应该有 direct map（独立决策）
3. **当前**：两个决策相遇，产生张力

**张力本质**：

| 机制 | 提供 | 给谁用 |
|------|------|--------|
| kernel direct map | `phys_to_virt(pa)` = stable VA for all physical pages | 内核（Ring 0） |
| PtRegion | `alloc_pt_page()` = stable VA for page table pages | VM（Ring 3） |

两个机制都解决 "stable VA for physical pages" 问题，但**权限域不同**。

### 7.3 如果让 VM 页表中的 direct map 为 U/S=1

**可行性分析**：
- x86-64 页表权限由各级页表项 U/S 位 AND 决定
- 可以在 VM 的页表中设置 `U/S=1`，在其他进程页表中保持 `U/S=0`
- VM 作为 trusted pager，原本就控制所有进程的页表，拥有事实上的全物理内存访问能力

**如何实现**：
- 在 `pt_mapkernel` 或 `pt_new` 中，为 VM 的页表专门设置 direct map 为 `U/S=1`
- 或者让 VM 在初始化后，自己通过 `pt_writemap` 建立 user-accessible 的 direct map 映射

**如果这样做，PtRegion 的演化方向**：

PtRegion 可以简化为**纯 bootstrap 机制**：
1. VM 启动早期（T0-T4）：PtRegion 提供自举页表页（此时 direct map 尚未在 VM 页表中以 U/S=1 形式建立）
2. VM 建立 direct map（U/S=1 in VM's page table）后：所有后续物理页访问走 `phys_to_virt()`
3. PtRegion 退役：不再需要 bump allocator 分配 VA

简化后的 PtRegion：
```rust
// Bootstrap phase only
fn vm_bootstrap_init() {
    // 1. 用 PtRegion 自举（3 页），建立初始页表能力
    let pt_region = PtRegion::from_reserved(reserved);
    
    // 2. 用初始页表能力建立 VM 私有的 direct map（U/S=1）
    build_vm_direct_map(&pt_region);
    
    // 3. 此后，所有物理页访问走 vm_phys_to_virt()
    //    PtRegion 不再需要分配 VA
}
```

**统一后的系统概念**：
- 所有 physical page（包括页表页）通过同一套 `vm_phys_to_virt()` 访问
- 不再区分"普通页"和"页表页"的 VA 分配策略
- 去除 PtRegion 作为"第二套 VA 分配器"的复杂性

### 7.4 建议

**短期**（设计未定，暂不修改代码）：
- PtRegion 作为 bootstrap 机制保留
- direct map 仍在设计阶段，需进一步确定：是否以 U/S=1 形式放入 VM 页表

**中期**（设计确定后）：
- 如果 direct map 在 VM 页表中是 U/S=1：PtRegion 简化为纯 bootstrap
- 如果 direct map 在 VM 页表中是 U/S=0：PtRegion 保持当前角色（或通过系统调用让内核代为访问 direct map）

**核心决策**（需用户自行判断）：
1. **direct map 在 VM 地址空间中的权限是什么？** U/S=1 or U/S=0？
2. **是否接受 VM 拥有全物理内存的 Ring 3 可访问能力？** 这决定了安全模型的边界
3. **如果接受 U/S=1**，PtRegion 的 bump allocator + PDPT/PD/PT 层级管理可以直接由 `vm_phys_to_virt()` 替代
4. **如果不接受 U/S=1**，PtRegion 仍有存在价值，但可以考虑通过系统调用让内核代为访问 direct map（`sys_vmctl_phys_to_virt`），这样至少概念上统一

---

## 8. Direct Map 设计方案

### 8.1 架构决策

#### 8.1.1 VM 建立 direct map（不是 kernel）

**决策**：VM 是地址空间的建筑师（address-space architect），由 VM 建立 direct map。

**理由**：

1. **策略/机制分离更纯粹**：
   - Kernel = 机制执行者（CR3 切换、TLB 刷新、MMU 操作）
   - VM = 策略制定者（地址空间构造、页表编辑、内存分配策略）
   - Direct map 是地址空间构造的一部分，属于 VM 职责

2. **Kernel 不需要知道 VM 的内部数据结构**：
   - 如果 kernel 建立 direct map，还需要知道 VM 的分配器元数据大小
   - 这违反策略/机制分离：kernel 不应知道 VM 用 bitmap 还是 buddy

3. **保留 Minix3 精神**：
   - Minix3 的 VM 本来就负责 `map_kernel()`（为所有进程建立内核映射）
   - Direct map 是 `map_kernel()` 的自然扩展
   - 不是理念背叛，而是 x86-64 下的工程升级

#### 8.1.2 VM 保持 ring3（不升级到 ring0）

**决策**：VM 仍然是普通用户态进程，运行在 ring3。

**理由**：

1. VM 不执行任何特权指令（不切 CR3、不刷新 TLB、不操作 MMU）
2. VM 把页表当作"普通数据"读写，通过 IPC 让 kernel 加载
3. Kernel 甚至不需要知道 VM direct map 的存在——对 kernel 而言，VM 的页表内存就是普通用户空间页面

#### 8.1.3 双视图模型：同一物理内存，两个 VA 窗口

**决策**：建立两个 direct map 视图，映射同一物理内存，权限不同。

| 视图 | VA 范围 | 权限 | 存在于 | 用途 |
|------|---------|------|--------|------|
| Kernel direct map | 内核空间高半部分 | U/S=0 (supervisor) | 所有进程页表 | Kernel 访问物理内存 |
| VM direct map | 用户空间低半部分 | U/S=1 (user) | 仅 VM 进程页表 | VM 读写物理页数据 |

**关键**：这不是"复制物理内存"，只是创建两组 virtual aliases。成本极低——每个 1GB 映射只需 1 个页表表项（8 字节）。

**为什么需要两个视图**：一个 PTE 的 U/S 位不可能同时为 0 和 1。Kernel 需要 supervisor-only 的映射（所有进程可见），VM 需要 user-accessible 的映射（仅 VM 进程可见）。两者映射同一物理内存，只是 VA 窗口和权限不同。

#### 8.1.4 PtRegion 完全删除

**决策**：引入 direct map 后，PtRegion 失去存在意义，完全删除。

**理由**：

1. PtRegion 的核心用途是"给物理页分配 stable VA"——direct map 已经天然完成
2. PtRegion 的 `alloc_pt_page()` 本质上是在 VM 里"重新发明一套 mini direct-map"
3. Direct map 出现后，所有物理页天然就有 stable VA：`va = DIRECT_MAP_BASE + pa`
4. 递归问题根源直接消失：新 PT 页 = `alloc_phys() → vm_phys_to_virt()`，不需要 map、不需要 find_hole、不需要 pt_ptalloc

#### 8.1.5 EarlyHeap 完全消除

**决策**：1GB direct map + bitmap 替代 EarlyHeap。

**理由**：

1. EarlyHeap 存在的原因是"VM 无法访问物理内存，需要从 BSS 段分配"
2. 有了 1GB direct map，VM 启动即可访问前 1GB 物理内存
3. Bitmap 元数据永远能放进 1GB（即使 4TB 物理内存也只需 128MB）
4. EarlyHeap 的所有功能被 direct map + bitmap 完全替代

### 8.2 地址空间布局

#### 8.2.1 x86-64 布局

```
0x0000_0000_0000_0000  ┬── VM 代码/数据/堆/栈（普通用户空间）
                       │
0x0000_1000_0000_0000  ┬── VM Direct Map（user-accessible, U/S=1）
                       │    va = VM_DIRECT_MAP_BASE + pa
                       │    仅存在于 VM 进程的页表
                       │    映射所有物理内存
                       │    权限：P | RW | US | NX | PS(1GB)
                       │
0x0000_7FFF_FFFF_FFFF  ┴── 用户空间结束

                       ─── 非规范地址空洞（不可用）───

0xFFFF_8800_0000_0000  ┬── Kernel Direct Map（supervisor-only, U/S=0）
                       │    va = KERNEL_DIRECT_MAP_BASE + pa
                       │    存在于所有进程页表
                       │    仅 ring0 可访问
                       │    权限：P | RW | G | NX | PS(1GB)
                       │
0xFFFF_C800_0000_0000  ├── 内核代码/数据（.text, .rodata, .data, .bss）
                       │    U/S=0, Global
                       │
0xFFFF_FFFF_FFFF_FFFF  ┴── 内核空间结束
```

#### 8.2.2 arm64 布局

```
0x0000_0000_0000       ┬── VM 代码/数据/堆/栈
0x0000_1000_0000_0000  ┬── VM Direct Map（user-accessible）
                       │    1GB block mapping
                       │
0x0000_FFFF_FFFF_FFFF  ┴── 用户空间结束（48位VA）

0xFFFF_8000_0000_0000  ┬── Kernel Direct Map（supervisor-only）
                       │    1GB block mapping
0xFFFF_FFFF_FFFF_FFFF  ┴── 内核空间结束
```

#### 8.2.3 riscv64 Sv39 布局

```
0x0000_0000_0000       ┬── VM 代码/数据/堆/栈
0x0000_0010_0000_0000  ┬── VM Direct Map（user-accessible）
                       │    1GB gigapage mapping
                       │    注意：Sv39 只有 512GB 用户空间
                       │    1GB direct map 占 0.2%，可行
                       │
0x0000_3FFF_FFFF_FFFF  ┴── 用户空间结束（39位VA）

0xFFFF_FC00_0000_0000  ┬── Kernel Direct Map（supervisor-only）
0xFFFF_FFFF_FFFF_FFFF  ┴── 内核空间结束
```

### 8.3 架构抽象

```rust
pub trait DirectMapArch {
    const VM_DIRECT_MAP_BASE: u64;
    const KERNEL_DIRECT_MAP_BASE: u64;
    const HUGE_PAGE_SIZE: u64;
    const INITIAL_MAP_SIZE: u64;

    /// 运行时检测是否支持 1GB 大页
    /// x86-64: CPUID.80000001H:EDX[26] (PDPE1GB)
    /// arm64: 始终支持 1GB block (PUD level)
    /// riscv64: Sv39 始终支持 1GB gigapage (PMD level)
    fn supports_1gb_page() -> bool;

    /// 回退大页大小（当 1GB 大页不可用时）
    /// x86-64: 2MB (PDE.PS=1)
    /// arm64: 2MB (PMD block)
    /// riscv64: 2MB (megapage)
    const FALLBACK_HUGE_PAGE_SIZE: u64;
}

impl DirectMapArch for X86_64 {
    const VM_DIRECT_MAP_BASE: u64     = 0x0000_1000_0000_0000;
    const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_8800_0000_0000;
    const HUGE_PAGE_SIZE: u64         = 1 << 30; // 1GB
    const INITIAL_MAP_SIZE: u64       = 1 << 30; // 1GB
    const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21; // 2MB

    fn supports_1gb_page() -> bool {
        cpuid_check_pdpe1gb() // CPUID.80000001H:EDX[26]
    }
}

impl DirectMapArch for Arm64 {
    const VM_DIRECT_MAP_BASE: u64     = 0x0000_1000_0000_0000;
    const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;
    const HUGE_PAGE_SIZE: u64         = 1 << 30; // 1GB block
    const INITIAL_MAP_SIZE: u64       = 1 << 30; // 1GB
    const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21; // 2MB block

    fn supports_1gb_page() -> bool { true }
}

impl DirectMapArch for Riscv64 {
    const VM_DIRECT_MAP_BASE: u64     = 0x0000_0010_0000_0000;
    const KERNEL_DIRECT_MAP_BASE: u64 = 0xFFFF_FC00_0000_0000;
    const HUGE_PAGE_SIZE: u64         = 1 << 30; // 1GB gigapage
    const INITIAL_MAP_SIZE: u64       = 1 << 30; // 1GB
    const FALLBACK_HUGE_PAGE_SIZE: u64 = 1 << 21; // 2MB megapage

    fn supports_1gb_page() -> bool { true } // Sv39 gigapage always supported
}
```

### 8.4 VM 初始页表结构

Kernel 创建 VM 进程时，建立最小初始页表：

```
x86-64 初始页表（4 页 = 16KB）：

PML4[0]   → PDPT_A → PD_A → 2MB huge pages（VM 代码/数据）
PML4[32]  → PDPT_B → PDPT_B[0] = phys 0 | P | RW | US | NX | PS  ← 1GB direct map
```

| 页 | 用途 | 物理位置 |
|----|------|---------|
| PML4 | 根页表 | 物理内存前几页 |
| PDPT_A | VM 代码/数据映射 | 同上 |
| PD_A | VM 代码/数据（2MB 大页） | 同上 |
| PDPT_B | VM direct map | 同上 |

**PDPT_B[0] 那一个 8 字节的表项，就是 1GB direct map**。

三种架构等价结构：

| 架构 | 第 1 级 | 第 2 级 | 第 3 级 = 1GB direct map |
|------|---------|---------|------------------------|
| x86-64 | PML4 | PDPT | PDPT[i] = 1GB huge page |
| arm64 | PGD | PUD | PUD[i] = 1GB block |
| riscv64 | PGD | PMD | PMD[i] = 1GB gigapage |

### 8.5 统一 3 阶段启动机制

```
┌──────────────────────────────────────────────────────┐
│            统一的 3 阶段启动机制                          │
│                                                        │
│  Phase 1: Bootstrap（永远执行）                           │
│    1GB direct map → bitmap allocator                    │
│                                                        │
│  Phase 2: Direct Map 扩展（永远执行）                      │
│    bitmap 分配页表页 → 扩展到覆盖全部物理内存                 │
│                                                        │
│  Phase 3: 分配器迁移（机制永远在，策略决定是否执行）           │
│    bitmap → buddy（如果策略决定）                          │
│    bitmap → bitmap（如果策略决定不迁移）                    │
│                                                        │
│  机制 = Phase 1+2+3 的代码路径永远存在                      │
│  策略 = Phase 3 是否执行、迁移到什么分配器                    │
└──────────────────────────────────────────────────────┘
```

#### Phase 1: Bootstrap（永远执行）

```
Kernel → VM:
  初始页表（4 页）+ 1GB direct map（1 个大页表项）
  boot_info（物理内存范围 + 可用页列表 + VM 页表物理地址）
  reserved_region（~10 页：boot_info + 初始栈 + 初始页表页）

VM:
  1. vm_phys_to_virt() 可用（前 1GB）
  2. 读取 boot_info，获取 total_pages
  3. 计算 bitmap 元数据大小（total_pages / 8 + page_cache）
  4. 从前 1GB 的可用物理页中分配 bitmap 元数据
     → 永远够用（即使 4TB 物理内存也只需 ~128MB）
  5. 初始化 bitmap allocator
  6. bitmap 管理全部物理内存（不仅仅是前 1GB）
```

**为什么 bitmap 永远能放进 1GB**：

| 物理内存 | Bitmap 元数据 | 占 1GB 比例 |
|---------|-------------|-----------|
| 4 GB | ~206 KB | 0.02% |
| 64 GB | ~2.1 MB | 0.2% |
| 256 GB | ~8.1 MB | 0.8% |
| 1 TB | ~32 MB | 3.1% |
| 4 TB | ~128 MB | 12.5% |

#### Phase 2: Direct Map 扩展（永远执行）

```
VM:
  1. 通过 bitmap allocator 分配物理页（用于页表页，如果需要）
  2. 通过 1GB direct map 访问 PDPT_B 页
  3. 写入额外的大页表项：
     PDPT_B[1] = phys 1GB | P | RW | US | NX | PS
     PDPT_B[2] = phys 2GB | P | RW | US | NX | PS
     ...
  4. 现在 direct map 覆盖全部物理内存
  5. 建立 kernel direct map（map_kernel 的一部分）：
     在 VM 页表的高半部分写入同样的 1GB 大页表项，但 U/S=0, G=1
```

**关键**：1GB 大页映射不需要分配新的页表页。PDPT_B 页已经在初始页表中，只需写入更多表项。即使物理内存 = 64GB，也只需写 63 个额外的 PDPT 表项（每个 8 字节），0 额外物理页分配。

**边界情况：物理内存 > 512GB**。一个 PDPT 页最多容纳 512 个 1GB 表项（覆盖 512TB），但 PML4 中每个条目对应一个 PDPT 页，每个 PDPT 覆盖 512GB。当物理内存超过 512GB 时，需要分配新的 PDPT 页并写入 PML4。此时 VM 已拥有至少 1GB direct map，可以：
1. `bitmap.alloc_mem(1)` → 获得新 PDPT 物理页
2. `vm_phys_to_virt(new_pdpt_phys)` → 清零并写入 1GB 大页表项
3. 通过 direct map 访问 PML4 页（在 reserved_region 中，物理地址在前 1GB 内），写入新 PML4 条目指向新 PDPT

**仍然不需要递归，0 额外自举风险**。代码路径统一，只是多了一步"分配 PDPT 页"。

**代码路径统一**：即使物理内存只有 512MB（1GB direct map 已覆盖全部），扩展逻辑也只是"发现无需扩展"然后跳过。代码路径统一，只是循环次数为 0。

#### Phase 3: 分配器迁移（机制永远在，策略决定）

```rust
fn should_migrate_to_buddy(total_pages: usize) -> bool {
    total_pages > 128 * 1024 // > 512MB 时迁移
}
```

迁移流程：

```
1. 计算 buddy 元数据大小
2. 通过 bitmap allocator 分配 buddy 元数据物理页
   → 此时 direct map 已覆盖全部物理内存，可以分配任意位置的页
3. 通过 direct map 初始化 buddy 元数据
4. 从 bitmap 迁移空闲页信息到 buddy
5. 切换到 buddy allocator
6. 释放 bitmap 元数据物理页
```

**机制统一**：迁移代码永远存在。策略函数返回 true 则执行迁移，返回 false 则跳过。物理内存大小只影响策略决策，不改变代码路径。

### 8.6 核心函数

```rust
/// 物理地址 → 虚拟地址（通过 VM direct map）
#[inline(always)]
pub(crate) fn vm_phys_to_virt(phys: PhysBytes) -> *mut u8 {
    (phys.as_u64() + VM_DIRECT_MAP_BASE) as *mut u8
}

/// 物理地址 → 虚拟地址（通过 kernel direct map）
/// 仅在构建其他进程的页表时使用
#[inline(always)]
pub(crate) fn kernel_phys_to_virt(phys: PhysBytes) -> *mut u8 {
    (phys.as_u64() + KERNEL_DIRECT_MAP_BASE) as *mut u8
}
```

### 8.7 操作简化示例

#### 创建新进程页表

```rust
fn pt_new() -> PageTable {
    let dir_phys = bitmap.alloc_mem(1)?;
    let dir_ptr = vm_phys_to_virt(dir_phys) as *mut u64;
    unsafe { core::ptr::write_bytes(dir_ptr as *mut u8, 0, 4096); }
    pt_mapkernel(dir_ptr);
}
```

#### CoW 复制

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

#### 进程退出

```rust
fn free_proc(vmp: &mut ActiveProc) {
    for region in vmp.regions_mut().drain() {
        for phys_opt in region.physblocks.iter() {
            if let Some(phys) = phys_opt {
                let ptr = vm_phys_to_virt(phys.get_phys_addr().unwrap());
                unsafe { core::ptr::write_bytes(ptr, 0, 4096); }
                bitmap.free_mem(phys.get_phys_addr().unwrap(), 1);
            }
        }
    }
    pt_free(vmp);
}
```

### 8.8 安全分析

#### 8.8.1 VM direct map 的安全边界

| 威胁 | 分析 | 结论 |
|------|------|------|
| VM 被攻破，通过 direct map 读写其他进程内存 | VM 已经控制所有进程的页表，攻破 VM = 攻破整个内存隔离 | U/S=1 不增加攻击面 |
| 用户进程通过 VM 的 direct map 访问物理内存 | VM direct map 仅存在于 VM 进程页表，其他进程页表中不存在 | 不受影响 |
| VM 通过 direct map 执行物理页中的代码 | VM direct map 默认 NX (No-Execute) | 硬件阻止代码执行 |
| VM 修改 kernel direct map 的页表结构 | Kernel direct map 建立后视为只读不变量，VM 代码结构保证不再修改 | 代码层面保证 |

**Kernel direct map 只读不变量约束**：`map_kernel()` 建立 kernel direct map 后，VM 不再修改其页表结构。这意味着：
- Kernel direct map 的 PTE/PDE/PDPT 表项在 `map_kernel()` 返回后不再变化
- Global 位的 TLB 条目不需要额外刷新策略（CR3 切换不刷新，且内容不变）
- 如果未来需要动态修改 kernel direct map（如内存热插拔），需要设计显式的 TLB 刷新协议

#### 8.8.2 与 Minix3 安全模型的对比

| 维度 | Minix3 | minix-rs |
|------|--------|----------|
| VM 访问物理内存 | `createpde` 临时映射（内核协助） | VM direct map 永久映射 |
| 权限检查 | 内核在 `createpde` 中检查 | 硬件 U/S 位检查（一次性，VM 页表建立时设置） |
| 审计能力 | 每次临时映射都有内核介入 | 启动时一次性设置 |
| 实际安全边界 | VM 能获取任意物理页映射 → 等价于全访问 | VM 有全物理页 direct map → 等价于全访问 |

**结论**：两者实际安全边界相同。差别只是"显式拥有"（direct map）vs"事实上拥有"（createpde）。

#### 8.8.3 侧信道考虑

- 当前不考虑 KPTI（Meltdown 缓解），VM direct map 在 VM 页表中是 user-accessible
- 如果未来需要 KPTI，VM 进程的 direct map 仍然可以保持 U/S=1（VM 是 trusted 组件，不需要隔离）
- Direct map 使用 Global 页（kernel 视图），CR3 切换不刷新 kernel direct map 的 TLB 条目

### 8.9 消除的 Minix3 机制

| Minix3 机制 | 存在原因 | Direct Map 后 |
|-------------|----------|-------------|
| `freepde` 临时映射窗口 | 内核未映射所有物理内存 | ❌ 不需要 |
| `createpde` | 访问非当前进程的物理内存 | ❌ 不需要 |
| `pagedir_mappings` | 内核跟踪进程页目录 | ❌ 不需要 |
| `spare_pagequeue` | 避免页表页分配递归 | ❌ 不需要（direct map 消除递归根源） |
| `switch_address_space_idle` | IDLE 任务需要切换到 VM 页表 | ❌ 不需要 |
| PtRegion | 给物理页分配 stable VA | ❌ 不需要（direct map 天然提供） |
| EarlyHeap | VM 启动时无法访问物理内存 | ❌ 不需要（1GB direct map 替代） |

### 8.10 VM 代码改动清单

#### 新增

| 文件 | 内容 |
|------|------|
| `direct_map.rs` | `vm_phys_to_virt()`, `kernel_phys_to_virt()`, `DirectMapArch` trait, direct map 扩展逻辑 |

#### 修改

| 文件 | 改动 |
|------|------|
| `alloc_page.rs` | `alloc_virt()` → `vm_phys_to_virt()`；移除 PtRegion 依赖 |
| `memtype.rs` | `on_pagefault` 中 CoW 操作使用 `vm_phys_to_virt()` |
| `phys_region.rs` | 物理页内容访问通过 `vm_phys_to_virt()` |
| `vmproc/vmproc_handle.rs` | `write_page_table_mappings()` 使用 `vm_phys_to_virt()` |
| `fork.rs` | `clone_region_for_fork()` 中物理页复制通过 `vm_phys_to_virt()` |
| `global.rs` | 添加 `VM_DIRECT_MAP_BASE` / `KERNEL_DIRECT_MAP_BASE` 常量 |

#### 删除

| 代码 | 原因 |
|------|------|
| `pt_region.rs` 整个文件 | Direct map 替代 PtRegion 的所有功能 |
| `EarlyHeap` 分配器 | 1GB direct map + bitmap 替代 |
| `ReservedRegion` 的 VA 分配功能 | Direct map 替代 |

### 8.11 Kernel 侧改动

| 改动 | 说明 |
|------|------|
| 创建 VM 时建立初始页表 | PML4 + PDPT_A + PD_A + PDPT_B（4 页） |
| 写入 1 个 1GB 大页表项 | PDPT_B[0] = phys 0, U/S=1, PS=1 |
| 传递 boot_info | 物理内存范围 + 可用页列表 + VM 页表物理地址 |
| 传递 reserved_region | ~10 页（boot_info + 初始栈 + 初始页表页） |
| `sys_datacopy` 简化 | VM 可直接通过 direct map 完成跨进程复制，无需 kernel 切换 PDE |
| `createpde` 移除 | 不再需要临时映射窗口 |

### 8.12 实施路线

```
Phase 1: Kernel 建立初始页表 + 1GB direct map
  ├── Kernel 创建 VM 时建立 4 页初始页表
  ├── 写入 1 个 1GB 大页表项（U/S=1）
  ├── 传递 boot_info + reserved_region
  └── 验证：VM 启动后可通过 vm_phys_to_virt() 访问前 1GB

Phase 2: VM 初始化 bitmap + 扩展 direct map
  ├── VM 从前 1GB 物理页分配 bitmap 元数据
  ├── 初始化 bitmap allocator（管理全部物理内存）
  ├── 扩展 VM direct map 到覆盖全部物理内存
  ├── 建立 kernel direct map（map_kernel）
  └── 验证：vm_phys_to_virt() 可访问任意物理页

Phase 3: VM 代码迁移到 direct map
  ├── 页表操作：通过 direct map 直接写页表项
  ├── CoW：mem_cow 通过 direct map 复制
  ├── 进程退出：free_proc 通过 direct map 释放
  └── 删除 PtRegion 相关代码

Phase 4: 清理
  ├── 删除 pt_region.rs
  ├── 删除 EarlyHeap
  ├── 删除 createpde/freepde 机制
  ├── 删除 pagedir_mappings
  └── 可选：bitmap → buddy 迁移
```

### 8.13 风险与缓解

| 风险 | 缓解 |
|------|------|
| 1GB direct map 不够覆盖 Buddy 元数据 | 不可能：Buddy 元数据 ≤0.12%，Bitmap 元数据更小 |
| 物理内存 >1GB 时需要扩展 direct map | 统一机制：写 PDPT 表项，0 额外页分配，代码路径与 ≤1GB 相同 |
| VM direct map U/S=1 被滥用 | VM 是 trusted 组件；NX 位阻止代码执行；其他进程无此映射 |
| TLB 压力 | Kernel direct map 使用 Global 页；VM direct map 仅 VM 进程使用 |
| 1GB 大页 CPU 支持 | x86-64 需 CPUID 检查；不支持时回退到 2MB 大页（每 1GB 段需 1 个 PD 页） |
| riscv64 Sv39 地址空间紧张 | 512GB 用户空间中 1GB direct map 占 0.2%，可行；Sv48 可扩展 |
| 物理内存有 hole（NUMA 等） | 当前不考虑；未来可通过 sparse direct map 处理 |
