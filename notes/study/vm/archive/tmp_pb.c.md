# servers/vm/pb.c 逐行讲解

> **文件路径**: `minix3/minix/servers/vm/pb.c`
> **代码行数**: 168 行
> **核心功能**: 物理块（Physical Block）管理，包括创建、释放、引用计数、写时复制

---

## 文件概述

pb.c 实现了物理块（`phys_block`）的生命周期管理，是 Minix3 虚拟内存管理的核心组件。

**核心功能**：
1. **物理块创建与释放**：`pb_new`、`pb_free`
2. **引用计数管理**：`pb_link`、`pb_unreferenced`
3. **写时复制**：`mem_cow`

**设计思路**：
- 物理块是物理内存页的抽象
- 引用计数支持共享内存和 COW
- Slab 分配器提高分配效率

---

## 逐行讲解

### 第 1-25 行：头文件包含

```c
#define	_SYSTEM 1

#include <minix/com.h>
#include <minix/callnr.h>
#include <minix/type.h>
#include <minix/config.h>
#include <minix/const.h>
#include <minix/sysutil.h>
#include <minix/syslib.h>
#include <minix/debug.h>
#include <minix/bitmap.h>
#include <minix/hash.h>

#include <sys/mman.h>

#include <limits.h>
#include <string.h>
#include <errno.h>
#include <assert.h>
#include <stdint.h>
#include <sys/param.h>

#include "vm.h"
#include "proto.h"
#include "util.h"
#include "glo.h"
#include "region.h"
#include "sanitycheck.h"
#include "memlist.h"
```

**是什么**：包含系统头文件和 VM 内部头文件。

**为什么**：
- `<minix/type.h>`：定义 `phys_bytes`、`vir_bytes` 等类型
- `"vm.h"`：VM 常量定义（`VM_PAGE_SIZE`、`MAP_NONE`）
- `"region.h"`：虚拟区域和物理区域结构定义
- `"sanitycheck.h"`：`USE` 宏，用于调试

**应用场景**：所有 VM 功能的基础。

---

### 第 27-47 行：pb_new 函数

```c
struct	phys_block *pb_new(phys_bytes phys)
{
	struct phys_block *newpb;

	if(!SLABALLOC(newpb)) {
		printf("vm: pb_new: couldn't allocate phys block\n");
		return NULL;
	}

	if(phys != MAP_NONE)
		assert(!(phys % VM_PAGE_SIZE));
	
USE(newpb,
	newpb->phys = phys;
	newpb->refcount = 0;
	newpb->firstregion = NULL;
	newpb->flags = 0;
	);

	return newpb;
}
```

**是什么**：创建新的物理块结构。

**逐行解析**：
- **第 31 行**：`SLABALLOC(newpb)` - 从 Slab 分配器分配内存
- **第 32-35 行**：分配失败，打印错误并返回 NULL
- **第 37-38 行**：如果指定了物理地址，检查是否页对齐
- **第 40-45 行**：初始化物理块字段（使用 `USE` 宏包裹，用于调试）

**为什么**：
- **Slab 分配器**：提高分配效率，减少内存碎片
- **页对齐检查**：确保物理地址是页的起始地址
- **`USE` 宏**：在调试模式下执行额外检查

**应用场景**：
- 分配新的物理页时
- COW 时创建新的物理块

---

### 第 49-55 行：pb_free 函数

```c
void pb_free(struct phys_block *pb)
{
	if(pb->phys != MAP_NONE)
		free_mem(ABS2CLICK(pb->phys), 1);
	SLABFREE(pb);
}
```

**是什么**：释放物理块。

**逐行解析**：
- **第 52-53 行**：如果物理块有物理内存，释放物理页
- **第 54 行**：释放物理块结构本身

**为什么**：
- **`ABS2CLICK`**：将物理地址转换为页框号
- **两层释放**：先释放物理内存，再释放结构体

**应用场景**：
- 引用计数降为 0 时
- 进程退出时

---

### 第 57-66 行：pb_link 函数

```c
void pb_link(struct phys_region *newphysr, struct phys_block *newpb,
	vir_bytes offset, struct vir_region *parent)
{
USE(newphysr,
	newphysr->offset = offset;
	newphysr->ph = newpb;
	newphysr->parent = parent;
	newphysr->next_ph_list = newpb->firstregion;
	newpb->firstregion = newphysr;);
	newpb->refcount++;
}
```

**是什么**：将物理区域链接到物理块。

**逐行解析**：
- **第 61-65 行**：初始化物理区域字段
  - `offset`：在虚拟区域中的偏移
  - `ph`：指向物理块
  - `parent`：所属的虚拟区域
  - `next_ph_list`：插入物理块的链表头部
- **第 66 行**：增加物理块的引用计数

**为什么**：
- **链表管理**：一个物理块可以被多个物理区域引用（共享内存）
- **引用计数**：跟踪有多少个物理区域引用此物理块

**应用场景**：
- 创建新的物理区域时
- COW 后链接新的物理块

---

### 第 68-84 行：pb_reference 函数

```c
struct	phys_region *pb_reference(struct phys_block *newpb,
	vir_bytes offset, struct vir_region *region, mem_type_t *memtype)
{
	struct phys_region *newphysr;

	if(!SLABALLOC(newphysr)) {
	printf("vm: pb_reference: couldn't allocate phys region\n");
	return NULL;
	}

	newphysr->memtype = memtype;

	/* New physical region. */
	pb_link(newphysr, newpb, offset, region);

	physblock_set(region, offset, newphysr);

	return newphysr;
}
```

**是什么**：创建物理区域并引用物理块。

**逐行解析**：
- **第 73-76 行**：分配物理区域结构
- **第 78 行**：设置内存类型
- **第 81 行**：调用 `pb_link` 链接物理块
- **第 83 行**：在虚拟区域的 `physblocks` 数组中设置指针

**为什么**：
- **内存类型**：不同类型的内存（匿名、文件映射）有不同的处理方式
- **二级映射**：虚拟区域 → 物理区域数组 → 物理块

**应用场景**：
- 缺页处理时创建新的物理区域
- 共享内存时引用已有的物理块

---

### 第 86-121 行：pb_unreferenced 函数

```c
void pb_unreferenced(struct vir_region *region, struct phys_region *pr, int rm)
{
	struct phys_block *pb;

	pb = pr->ph;
	assert(pb->refcount > 0);
	USE(pb, pb->refcount--;);
/*	assert(pb->refcount >= 0); */ /* always true */

	if(pb->firstregion == pr) {
		USE(pb, pb->firstregion = pr->next_ph_list;);
	} else {
		struct phys_region *others;

		for(others = pb->firstregion; others;
			others = others->next_ph_list) {
			assert(others->ph == pb);
			if(others->next_ph_list == pr) {
				USE(others, others->next_ph_list = pr->next_ph_list;);
				break;
			}
		}

		assert(others); /* Otherwise, wasn't on the list. */
	}

	if(pb->refcount == 0) {
		assert(!pb->firstregion);
		int r;
		if((r = pr->memtype->ev_unreference(pr)) != OK)
			panic("unref failed, %d", r);

		SLABFREE(pb);
	}

	pr->ph = NULL;

	if(rm) physblock_set(region, pr->offset, NULL);
}
```

**是什么**：取消物理区域对物理块的引用。

**逐行解析**：
- **第 91-92 行**：获取物理块，断言引用计数 > 0
- **第 93 行**：减少引用计数
- **第 95-108 行**：从物理块的链表中移除物理区域
  - 如果是第一个，直接更新 `firstregion`
  - 否则，遍历链表找到并移除
- **第 110-116 行**：如果引用计数为 0，释放物理块
  - 调用内存类型的 `ev_unreference` 回调
  - 释放物理块结构
- **第 118 行**：清空物理区域的 `ph` 指针
- **第 120 行**：如果 `rm` 为真，从虚拟区域的数组中移除

**为什么**：
- **链表操作**：维护物理块的引用链表
- **引用计数**：当引用计数为 0 时释放物理内存
- **内存类型回调**：不同类型的内存有不同的清理逻辑

**应用场景**：
- 释放内存区域时
- COW 时取消对旧物理块的引用

---

### 第 123-168 行：mem_cow 函数

```c
int mem_cow(struct vir_region *region,
        struct phys_region *ph, phys_bytes new_page_cl, phys_bytes new_page)
{
        struct phys_block *pb;

        if(new_page == MAP_NONE) {
                u32_t allocflags;
                allocflags = vrallocflags(region->flags);

                if((new_page_cl = alloc_mem(1, allocflags)) == NO_MEM)
                        return ENOMEM;

                new_page = CLICK2ABS(new_page_cl);
        }

	assert(ph->ph->phys != MAP_NONE);

        if(sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE) != OK) {
                panic("VM: abscopy failed\n");
                return EFAULT;
        }

        if(!(pb = pb_new(new_page))) {
                free_mem(new_page_cl, 1);
                return ENOMEM;
        }

        pb_unreferenced(region, ph, 0);
        pb_link(ph, pb, ph->offset, region);
	ph->memtype = &mem_type_anon;

        return OK;
}
```

**是什么**：执行写时复制（Copy-on-Write）。

**逐行解析**：
- **第 128-135 行**：如果没有提供新页面，分配一个
  - `vrallocflags`：根据区域标志计算分配标志
  - `alloc_mem`：分配物理页
- **第 137 行**：断言旧物理块有物理内存
- **第 139-142 行**：使用 `sys_abscopy` 拷贝数据
- **第 144-147 行**：创建新的物理块
- **第 149 行**：取消对旧物理块的引用
- **第 150 行**：链接新的物理块
- **第 151 行**：设置内存类型为匿名内存

**为什么**：
- **COW 优化**：fork 后不立即拷贝，只在写入时拷贝
- **`sys_abscopy`**：内核提供的物理内存拷贝函数
- **内存类型切换**：COW 后变为匿名内存

**应用场景**：
- fork 后写入共享页面时
- 写入只读映射的页面时

---

## 要点总结

1. **物理块管理**：创建、释放、引用计数
2. **引用计数**：支持共享内存和 COW
3. **链表管理**：一个物理块可被多个物理区域引用
4. **写时复制**：延迟拷贝，提高 fork 性能

---

## 互动自测

1. **问题**: 为什么物理块需要引用计数？
   **答案**: 支持共享内存和写时复制，多个虚拟区域可以共享同一个物理页。

2. **问题**: COW 的优势是什么？
   **答案**: fork 时不立即拷贝内存，只在写入时才拷贝，提高性能。

---

## 现代化对比

### 与 Linux 的对比

| 方面 | Minix3 | Linux | 对比分析 |
|------|--------|-------|---------|
| **物理页管理** | `phys_block` 结构 | `page` 结构体 | 类似的设计 |
| **引用计数** | `refcount` 字段 | `_refcount` + `mapcount` | Linux 更复杂，支持反向映射 |
| **COW 实现** | `mem_cow` 函数 | `do_wp_page` | 类似的逻辑 |
| **分配器** | Slab 分配器 | Slab/Slub/Slob | 都使用 Slab |

---

### Rust 重构建议

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct PhysBlock {
    pub phys: PhysAddr,
    pub refcount: Arc<AtomicU32>,
    pub first_region: Option<Box<PhysRegion>>,
    pub flags: u32,
}

impl PhysBlock {
    pub fn new(phys: PhysAddr) -> Self {
        Self {
            phys,
            refcount: Arc::new(AtomicU32::new(0)),
            first_region: None,
            flags: 0,
        }
    }
    
    pub fn inc_refcount(&self) {
        self.refcount.fetch_add(1, Ordering::SeqCst);
    }
    
    pub fn dec_refcount(&self) -> u32 {
        self.refcount.fetch_sub(1, Ordering::SeqCst)
    }
}

pub fn mem_cow(
    region: &mut VirRegion,
    ph: &mut PhysRegion,
) -> Result<(), VmError> {
    // 分配新页面
    let new_page = alloc_mem(1, region.flags)?;
    
    // 拷贝数据
    sys_abscopy(ph.ph.phys, new_page, VM_PAGE_SIZE)?;
    
    // 创建新的物理块
    let new_pb = Arc::new(PhysBlock::new(new_page));
    
    // 取消旧引用
    pb_unreferenced(region, ph, false);
    
    // 链接新物理块
    pb_link(ph, new_pb, ph.offset, region);
    
    Ok(())
}
```

**Rust 优势**：
1. **自动引用计数**：`Arc` 自动管理引用计数
2. **线程安全**：`AtomicU32` 保证并发安全
3. **错误处理**：`Result` 强制处理错误
4. **内存安全**：所有权系统防止 use-after-free

---

## 参考源码

- **region.h**：虚拟区域和物理区域结构定义
- **memtype.h**：内存类型定义
- **alloc.c**：内存分配函数
