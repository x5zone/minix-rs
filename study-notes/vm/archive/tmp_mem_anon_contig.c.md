# servers/vm/mem_anon_contig.c 逐行讲解

&gt; **文件路径**: `minix3/minix/servers/vm/mem_anon_contig.c`
&gt; **代码行数**: 131 行
&gt; **核心功能**: 物理连续的匿名内存实现

---

## 文件概述

这个文件实现了物理连续的匿名内存（Physically Contiguous Anonymous Memory）。

**核心特点**：
- 分配物理连续的内存页
- 不支持 fork（不能复制）
- 不支持 resize（不能调整大小）
- 不支持 split（不能拆分）
- 主要用于需要物理连续内存的特殊场景（如 DMA）

**设计思路**：
- 这是 Minix3 的 5 种内存类型之一
- 继承自 `mem_type_anon` 的部分方法
- 但重写了不支持的操作，让它们返回错误

---

## 逐行讲解

### 第 1 行：空行

**是什么**：文件开头的空行。

**应用场景**：提高可读性。

---

### 第 2 行：文件功能注释

```c
/* This file implements the methods of physically contiguous anonymous memory. */
```

**是什么**：说明文件功能。

**注释翻译**：这个文件实现了物理连续的匿名内存的方法。

**应用场景**：理解文件用途。

---

### 第 4-9 行：头文件包含

```c
#include &lt;assert.h&gt;

#include "proto.h"
#include "vm.h"
#include "region.h"
#include "glo.h"
```

**是什么**：包含必要的头文件。

**应用场景**：提供类型和函数声明。

---

### 第 11-22 行：静态函数原型声明

```c
static int anon_contig_reference(struct phys_region *, struct phys_region *);
static int anon_contig_unreference(struct phys_region *pr);
static int anon_contig_pagefault(struct vmproc *vmp, struct vir_region *region, 
	struct phys_region *ph, int write, vfs_callback_t cb, void *state,
	int len, int *io);
static int anon_contig_sanitycheck(struct phys_region *pr, const char *file, int line);
static int anon_contig_writable(struct phys_region *pr);
static void anon_contig_split(struct vmproc *vmp, struct vir_region *vr,
                        struct vir_region *r1, struct vir_region *r2);
static int anon_contig_resize(struct vmproc *vmp, struct vir_region *vr, vir_bytes l);
static int anon_contig_new(struct vir_region *vr);
static int anon_contig_pt_flags(struct vir_region *vr);
```

**是什么**：声明所有静态函数。

**应用场景**：C 语言要求函数在使用前声明。

---

### 第 24-35 行：mem_type_anon_contig 结构定义

```c
struct mem_type mem_type_anon_contig = {
	.name = "anonymous memory (physically contiguous)",
	.ev_new = anon_contig_new,
	.ev_reference = anon_contig_reference,
	.ev_unreference = anon_contig_unreference,
	.ev_pagefault = anon_contig_pagefault,
	.ev_resize = anon_contig_resize,
	.ev_split = anon_contig_split,
	.ev_sanitycheck = anon_contig_sanitycheck,
	.writable = anon_contig_writable,
	.pt_flags = anon_contig_pt_flags,
};
```

**是什么**：定义物理连续匿名内存的内存类型结构体。

**逐字段解析**：
- `.name`：名称，用于调试
- `.ev_new`：创建新区域时调用
- `.ev_reference`：引用时调用
- `.ev_unreference`：取消引用时调用
- `.ev_pagefault`：缺页时调用
- `.ev_resize`：调整大小时调用
- `.ev_split`：拆分时调用
- `.ev_sanitycheck`：健全性检查时调用
- `.writable`：检查可写性时调用
- `.pt_flags`：获取页表标志时调用

**应用场景**：VM 服务器通过这个结构体调用内存类型的方法。

---

### 第 37-43 行：anon_contig_pt_flags 函数

```c
static int anon_contig_pt_flags(struct vir_region *vr){
#if defined(__arm__)
	return  ARM_VM_PTE_DEVICE;
#else
	return  0;
#endif
}
```

**是什么**：返回页表标志。

**应用场景**：ARM 架构可能需要特殊标志。

---

### 第 45-50 行：anon_contig_pagefault 函数

```c
static int anon_contig_pagefault(struct vmproc *vmp, struct vir_region *region,
	struct phys_region *ph, int write, vfs_callback_t cb, void *state,
	int len, int *io)
{
	panic("anon_contig_pagefault: pagefault cannot happen");
}
```

**是什么**：缺页处理函数，直接 panic。

**为什么**：
- 物理连续内存在创建时就分配好了所有物理页
- 不应该发生缺页
- 如果发生了，说明有 bug

**应用场景**：不应该被调用。

---

### 第 52-95 行：anon_contig_new 函数

```c
static int anon_contig_new(struct vir_region *region)
{
        u32_t allocflags;
	phys_bytes new_pages, new_page_cl, cur_ph;
	phys_bytes p, pages;

        allocflags = vrallocflags(region-&gt;flags);

	pages = region-&gt;length/VM_PAGE_SIZE;

	assert(physregions(region) == 0);

	for(p = 0; p &lt; pages; p++) {
		struct phys_block *pb = pb_new(MAP_NONE);
		struct phys_region *pr = NULL;
		if(pb)
			pr = pb_reference(pb, p * VM_PAGE_SIZE, region, &amp;mem_type_anon_contig);
		if(!pr) {
			if(pb) pb_free(pb);
			map_free(region);
			return ENOMEM;
		}
	}

	assert(physregions(region) == pages);

	if((new_page_cl = alloc_mem(pages, allocflags)) == NO_MEM) {
		map_free(region);
		return ENOMEM;
	}

	cur_ph = new_pages = CLICK2ABS(new_page_cl);

	for(p = 0; p &lt; pages; p++) {
		struct phys_region *pr = physblock_get(region, p * VM_PAGE_SIZE);
		assert(pr);
		assert(pr-&gt;ph);
		assert(pr-&gt;ph-&gt;phys == MAP_NONE);
		assert(pr-&gt;offset == p * VM_PAGE_SIZE);
		pr-&gt;ph-&gt;phys = cur_ph + pr-&gt;offset;
	}

	return OK;
}
```

**是什么**：创建物理连续的匿名内存区域。

**核心步骤**：
1. 计算需要的页数
2. 创建物理块和物理区域（但不分配物理内存）
3. 分配连续的物理页
4. 设置每个物理块的物理地址

**为什么**：
- 先创建结构，再分配物理内存
- 如果分配失败，可以清理已创建的结构
- 确保物理页是连续的

**应用场景**：创建需要物理连续内存的区域（如 DMA）。

---

### 第 97-101 行：anon_contig_resize 函数

```c
static int anon_contig_resize(struct vmproc *vmp, struct vir_region *vr, vir_bytes l)
{
	printf("VM: cannot resize physically contiguous memory.\n");
	return ENOMEM;
}
```

**是什么**：调整大小函数，返回错误。

**为什么**：
- 物理连续内存不能调整大小
- 调整大小可能破坏物理连续性
- 所以直接返回错误

**应用场景**：不支持调整大小。

---

### 第 103-108 行：anon_contig_reference 函数

```c
static int anon_contig_reference(struct phys_region *pr,
	struct phys_region *newpr)
{
	printf("VM: cannot fork with physically contig memory.\n");
	return ENOMEM;
}
```

**是什么**：引用函数，返回错误。

**为什么**：
- 物理连续内存不能 fork
- fork 需要复制，可能破坏物理连续性
- 所以直接返回错误

**应用场景**：不支持 fork。

---

### 第 110-125 行：继承自匿名内存的方法

```c
/* Methods inherited from the anonymous memory methods. */

static int anon_contig_unreference(struct phys_region *pr)
{
	return mem_type_anon.ev_unreference(pr);
}

static int anon_contig_sanitycheck(struct phys_region *pr, const char *file, int line)
{
	return mem_type_anon.ev_sanitycheck(pr, file, line);
}

static int anon_contig_writable(struct phys_region *pr)
{
	return mem_type_anon.writable(pr);
}
```

**是什么**：继承自 `mem_type_anon` 的方法。

**为什么**：
- 这些操作和普通匿名内存一样
- 不需要重写
- 直接调用 `mem_type_anon` 的对应函数

**应用场景**：代码复用。

---

### 第 127-131 行：anon_contig_split 函数

```c
static void anon_contig_split(struct vmproc *vmp, struct vir_region *vr,
                        struct vir_region *r1, struct vir_region *r2)
{
	return;
}
```

**是什么**：拆分函数，什么都不做。

**为什么**：
- 物理连续内存不能拆分
- 拆分可能破坏物理连续性
- 所以什么都不做

**应用场景**：不支持拆分。

---

## 要点总结

1. **物理连续**：分配物理连续的内存页
2. **限制多**：不支持 fork、resize、split
3. **继承实现**：部分方法继承自 `mem_type_anon`
4. **特殊用途**：主要用于 DMA 等需要物理连续内存的场景

---

## 互动自测

1. **问题**: 物理连续内存为什么不支持 fork？
   **答案**: fork 需要复制，可能破坏物理连续性。

2. **问题**: 物理连续内存的典型用途是什么？
   **答案**: DMA（直接内存访问）等需要物理连续内存的场景。

3. **问题**: 为什么有些方法继承自 mem_type_anon？
   **答案**: 这些操作和普通匿名内存一样，不需要重写。

---

## 现代化对比

### 与 Linux 的对比

| 方面 | Minix3 | Linux | 对比分析 |
|------|--------|-------|---------|
| **连续内存** | `mem_type_anon_contig` | `alloc_pages` + `__GFP_COMP` | Linux 更灵活 |
| **限制** | 不支持 fork/resize/split | 支持，但需要特殊处理 | Linux 功能更全 |
| **DMA 支持** | 手动管理 | `dma_alloc_coherent` | Linux 有标准 API |

---

### Rust 重构建议

```rust
pub struct AnonContigMem {
    phys_base: PhysAddr,
    num_pages: usize,
}

impl MemType for AnonContigMem {
    fn new(&amp;self, region: &amp;mut VirRegion) -&gt; Result&lt;(), VmError&gt; {
        let pages = region.length / VM_PAGE_SIZE;
        
        // 分配连续物理页
        let phys_base = alloc_contig_pages(pages)?;
        
        // 创建物理块和物理区域
        for p in 0..pages {
            let pb = PhysBlock::new(phys_base + p * VM_PAGE_SIZE);
            let pr = PhysRegion::new(pb, p * VM_PAGE_SIZE);
            region.link_phys_region(pr);
        }
        
        Ok(())
    }
    
    fn pagefault(
        &amp;self,
        _region: &amp;VirRegion,
        _ph: &amp;mut PhysRegion,
    ) -&gt; Result&lt;(), VmError&gt; {
        panic!("pagefault should not happen for contiguous memory");
    }
    
    fn resize(
        &amp;self,
        _vmp: &amp;VmProc,
        _vr: &amp;mut VirRegion,
        _new_len: usize,
    ) -&gt; Result&lt;(), VmError&gt; {
        Err(VmError::NotSupported)
    }
}
```

**Rust 优势**：
1. **类型安全**：强类型防止错误
2. **错误处理**：`Result` 明确表示不支持的操作
3. **panic!**：明确表示不应该发生的情况
4. **所有权系统**：确保物理内存被正确管理
