# servers/vm/utility.c 逐行讲解

## 文件概述

**文件路径**: `servers/vm/utility.c`  
**功能**: VM（虚拟内存服务器）工具函数  
**设计思想**: 提供内存块初始化、端点验证、信息查询等工具函数

---

## 逐行讲解

### 文件注释和包含

```c
/* This file contains some utility routines for VM.  */

#define _SYSTEM		1

#include <minix/callnr.h>
#include <minix/com.h>
#include <minix/config.h>
#include <minix/const.h>
#include <minix/ds.h>
#include <minix/endpoint.h>
#include <minix/minlib.h>
#include <minix/type.h>
#include <minix/ipc.h>
#include <minix/sysutil.h>
#include <minix/syslib.h>
#include <minix/type.h>
#include <minix/bitmap.h>
#include <minix/rs.h>
#include <string.h>
#include <errno.h>
#include <unistd.h>
#include <assert.h>
#include <sys/cdefs.h>
#include <sys/param.h>
#include <sys/mman.h>
#include <sys/resource.h>

#include "proto.h"
#include "glo.h"
#include "util.h"
#include "region.h"
#include "sanitycheck.h"

#include <machine/archtypes.h>
#include "kernel/const.h"
#include "kernel/config.h"
#include "kernel/type.h"
#include "kernel/proc.h"
```

**逐词拆解**:
- `utility routines for VM`: VM 的工具函数
- `_SYSTEM`: 系统定义宏
- 各种 Minix 头文件：提供系统调用、IPC、类型定义等
- VM 内部头文件：proto.h, glo.h, util.h 等
- 内核头文件：kernel/proc.h 等

**设计原因**:
- 包含所有必要的头文件
- 提供系统调用和内核接口

---

### get_mem_chunks 函数

```c
/*===========================================================================*
 *                              get_mem_chunks                               *
 *===========================================================================*/
void get_mem_chunks(
struct memory *mem_chunks)                      /* store mem chunks here */ 
{  
/* Initialize the free memory list from the kernel-provided memory map.  Translate
 * the byte offsets and sizes in this list to clicks, properly truncated.
 */
  phys_bytes base, size, limit;
  int i;
  struct memory *memp;

  /* Initialize everything to zero. */
  memset(mem_chunks, 0, NR_MEMS*sizeof(*mem_chunks));

  /* Obtain and parse memory from kernel environment. */
  /* XXX Any memory chunk in excess of NR_MEMS is silently ignored. */
  for(i = 0; i < MIN(MAXMEMMAP, NR_MEMS); i++) {
  	mem_chunks[i].base = kernel_boot_info.memmap[i].mm_base_addr;
  	mem_chunks[i].size = kernel_boot_info.memmap[i].mm_length;
  }

  /* Round physical memory to clicks. Round start up, round end down. */
  for (i = 0; i < NR_MEMS; i++) {
        memp = &mem_chunks[i];          /* next mem chunk is stored here */
        base = mem_chunks[i].base;
        size = mem_chunks[i].size;
        limit = base + size;
        base = (phys_bytes) (CLICK_CEIL(base));
        limit = (phys_bytes) (CLICK_FLOOR(limit));
        if (limit <= base) {
                memp->base = memp->size = 0;
        } else { 
                memp->base = base >> CLICK_SHIFT;
                memp->size = (limit - base) >> CLICK_SHIFT;
        }
  }
}  
```

**逐词拆解**:
- `get_mem_chunks`: 获取内存块
- `mem_chunks`: 存储内存块的数组
- `kernel_boot_info.memmap`: 内核启动时的内存映射
- `CLICK_CEIL`: 向上对齐到 click 边界
- `CLICK_FLOOR`: 向下对齐到 click 边界
- `CLICK_SHIFT`: click 位移量

**功能详解**:
1. **初始化**: 将内存块数组清零
2. **获取内存映射**: 从内核启动信息中获取内存映射
3. **对齐处理**: 将内存块对齐到 click 边界
4. **单位转换**: 将字节转换为 click 单位

**设计原因**:
- **Click 对齐**: 内存管理以 click（通常 4KB）为单位
- **向上/向下对齐**: 确保内存块在 click 边界上
- **避免浪费**: 对齐后可能减少可用内存，但简化管理

**内存布局**:
```
原始内存映射:
地址 0x00000000 - 0x00000FFF (4KB) - 不可用
地址 0x00001000 - 0x00001FFF (4KB) - 可用
地址 0x00002000 - 0x00002FFF (4KB) - 可用
...

对齐后:
Click 0: 0x00000000 - 0x00000FFF
Click 1: 0x00001000 - 0x00001FFF
Click 2: 0x00002000 - 0x00002FFF
...
```

**理论关联**:
- **内存管理**: Memory Management，管理物理内存
- **内存对齐**: Memory Alignment，提高访问效率
- **内存映射**: Memory Map，描述物理内存布局

---

### vm_isokendpt 函数

```c
/*===========================================================================*
 *                              vm_isokendpt                           	     *
 *===========================================================================*/
int vm_isokendpt(endpoint_t endpoint, int *procn)
{
        *procn = _ENDPOINT_P(endpoint);
        if(*procn < 0 || *procn >= NR_PROCS)
		return EINVAL;
        if(*procn >= 0 && endpoint != vmproc[*procn].vm_endpoint)
                return EDEADEPT;
        if(*procn >= 0 && !(vmproc[*procn].vm_flags & VMF_INUSE))
                return EDEADEPT;
        return OK;
}
```

**逐词拆解**:
- `vm_isokendpt`: 验证端点是否有效
- `endpoint`: 端点标识符
- `procn`: 进程编号输出参数
- `_ENDPOINT_P`: 从端点提取进程编号
- `NR_PROCS`: 最大进程数
- `vm_endpoint`: VM 进程的端点
- `VMF_INUSE`: 进程在使用标志

**功能详解**:
1. **提取进程编号**: 从端点提取进程编号
2. **范围检查**: 检查进程编号是否在有效范围内
3. **端点匹配**: 检查端点是否匹配
4. **使用状态**: 检查进程是否在使用中

**设计原因**:
- **安全性**: 验证端点有效性，防止非法访问
- **一致性**: 确保端点和进程编号一致
- **错误处理**: 返回明确的错误码

**错误码**:
- `EINVAL`: 无效参数（进程编号超出范围）
- `EDEADEPT`: 端点已失效（端点不匹配或进程未使用）

---

### do_info 函数

```c
/*===========================================================================*
 *                              do_info                                      *
 *===========================================================================*/
int do_info(message *m)
{
	struct vm_stats_info vsi;
	struct vm_usage_info vui;
	static struct vm_region_info vri[MAX_VRI_COUNT];
	struct vmproc *vmp;
	vir_bytes addr, size, next, ptr;
	int r, pr, dummy, count, free_pages, largest_contig;

	if (vm_isokendpt(m->m_source, &pr) != OK)
		return EINVAL;
	vmp = &vmproc[pr];

	ptr = (vir_bytes) m->m_lsys_vm_info.ptr;

	switch(m->m_lsys_vm_info.what) {
	case VMIW_STATS:
		vsi.vsi_pagesize = VM_PAGE_SIZE;
		vsi.vsi_total = total_pages;
		memstats(&dummy, &free_pages, &largest_contig);
		vsi.vsi_free = free_pages;
		vsi.vsi_largest = largest_contig;

		get_stats_info(&vsi);

		addr = (vir_bytes) &vsi;
		size = sizeof(vsi);

		break;

	case VMIW_USAGE:
		if(m->m_lsys_vm_info.ep < 0)
			get_usage_info_kernel(&vui);
		else if (vm_isokendpt(m->m_lsys_vm_info.ep, &pr) != OK)
			return EINVAL;
		else get_usage_info(&vmproc[pr], &vui);

		addr = (vir_bytes) &vui;
		size = sizeof(vui);

		break;

	case VMIW_REGION:
		if(m->m_lsys_vm_info.ep == SELF) {
			m->m_lsys_vm_info.ep = m->m_source;
		}
		if (vm_isokendpt(m->m_lsys_vm_info.ep, &pr) != OK)
			return EINVAL;

		count = MIN(m->m_lsys_vm_info.count, MAX_VRI_COUNT);
		next = m->m_lsys_vm_info.next;

		count = get_region_info(&vmproc[pr], vri, count, &next);

		m->m_lsys_vm_info.count = count;
		m->m_lsys_vm_info.next = next;

		addr = (vir_bytes) vri;
		size = sizeof(vri[0]) * count;

		break;

	default:
		return EINVAL;
	}

	if (size == 0)
		return OK;

	/* Make sure that no page faults can occur while copying out. A page
	 * fault would cause the kernel to send a notify to VM, while we would
	 * be waiting for the result of the copy system call, resulting in a
	 * deadlock. Note that no memory mapping can be undone without the
	 * involvement of VM, so we are safe until we're done.
	 */
	r = handle_memory_once(vmp, ptr, size, 1 /*wrflag*/);
	if (r != OK) return r;

	/* Now that we know the copy out will succeed, perform the actual copy
	 * operation.
	 */
	return sys_datacopy(SELF, addr,
		(vir_bytes) vmp->vm_endpoint, ptr, size);
}
```

**逐词拆解**:
- `do_info`: 处理信息查询请求
- `vm_stats_info`: VM 统计信息
- `vm_usage_info`: VM 使用信息
- `vm_region_info`: VM 区域信息
- `VMIW_STATS`: 统计信息查询
- `VMIW_USAGE`: 使用信息查询
- `VMIW_REGION`: 区域信息查询
- `handle_memory_once`: 处理内存访问
- `sys_datacopy`: 系统调用数据拷贝

**功能详解**:
1. **验证端点**: 检查请求进程的有效性
2. **查询类型**: 根据查询类型返回不同信息
3. **统计信息**: 返回页面大小、总页面数、空闲页面数等
4. **使用信息**: 返回进程的内存使用情况
5. **区域信息**: 返回进程的内存区域信息
6. **数据拷贝**: 将信息拷贝到用户空间

**设计原因**:
- **避免死锁**: 先处理内存访问，确保拷贝不会触发页错误
- **安全性**: 验证端点和内存访问权限
- **效率**: 一次性拷贝所有数据

**死锁场景**:
```
如果拷贝时触发页错误:
1. VM 调用 sys_datacopy()
2. sys_datacopy() 触发页错误
3. 内核发送 notify 给 VM
4. VM 正在等待 sys_datacopy() 返回
5. 死锁！
```

**解决方案**:
```
1. 先调用 handle_memory_once() 处理内存
2. 确保拷贝不会触发页错误
3. 然后调用 sys_datacopy()
```

---

### swap_proc_slot 函数

```c
/*===========================================================================*
 *				swap_proc_slot	     			     *
 *===========================================================================*/
int swap_proc_slot(struct vmproc *src_vmp, struct vmproc *dst_vmp)
{
	struct vmproc orig_src_vmproc, orig_dst_vmproc;

#if LU_DEBUG
	printf("VM: swap_proc: swapping %d (%d) and %d (%d)\n",
	    src_vmp->vm_endpoint, src_vmp->vm_slot,
	    dst_vmp->vm_endpoint, dst_vmp->vm_slot);
#endif

	/* Save existing data. */
	orig_src_vmproc = *src_vmp;
	orig_dst_vmproc = *dst_vmp;
```

**逐词拆解**:
- `swap_proc_slot`: 交换进程槽
- `src_vmp`: 源进程
- `dst_vmp`: 目标进程
- `orig_src_vmproc`: 原始源进程数据
- `orig_dst_vmproc`: 原始目标进程数据
- `LU_DEBUG`: Live Update 调试标志

**功能详解**:
1. **保存原始数据**: 保存源和目标进程的原始数据
2. **交换数据**: 交换进程槽的数据
3. **Live Update**: 用于服务热更新

**设计原因**:
- **Live Update**: 支持服务热更新
- **原子操作**: 交换进程槽，保持一致性
- **调试支持**: 条件编译调试输出

---

## Rust 实现对比

### C 代码（原始）

```c
void get_mem_chunks(struct memory *mem_chunks)
{
  phys_bytes base, size, limit;
  int i;
  struct memory *memp;

  memset(mem_chunks, 0, NR_MEMS*sizeof(*mem_chunks));

  for(i = 0; i < MIN(MAXMEMMAP, NR_MEMS); i++) {
  	mem_chunks[i].base = kernel_boot_info.memmap[i].mm_base_addr;
  	mem_chunks[i].size = kernel_boot_info.memmap[i].mm_length;
  }

  for (i = 0; i < NR_MEMS; i++) {
        memp = &mem_chunks[i];
        base = mem_chunks[i].base;
        size = mem_chunks[i].size;
        limit = base + size;
        base = (phys_bytes) (CLICK_CEIL(base));
        limit = (phys_bytes) (CLICK_FLOOR(limit));
        if (limit <= base) {
                memp->base = memp->size = 0;
        } else { 
                memp->base = base >> CLICK_SHIFT;
                memp->size = (limit - base) >> CLICK_SHIFT;
        }
  }
}

int vm_isokendpt(endpoint_t endpoint, int *procn)
{
        *procn = _ENDPOINT_P(endpoint);
        if(*procn < 0 || *procn >= NR_PROCS)
		return EINVAL;
        if(*procn >= 0 && endpoint != vmproc[*procn].vm_endpoint)
                return EDEADEPT;
        if(*procn >= 0 && !(vmproc[*procn].vm_flags & VMF_INUSE))
                return EDEADEPT;
        return OK;
}
```

### Rust 代码（现代实现）

```rust
#![no_std]

use core::mem;

pub const NR_MEMS: usize = 10;
pub const NR_PROCS: usize = 1024;
pub const CLICK_SHIFT: usize = 12;  // 4KB

#[derive(Debug, Clone, Copy)]
pub struct Memory {
    pub base: usize,
    pub size: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct KernelBootInfo {
    pub memmap: [MemoryMapEntry; 10],
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryMapEntry {
    pub mm_base_addr: usize,
    pub mm_length: usize,
}

pub fn get_mem_chunks(mem_chunks: &mut [Memory; NR_MEMS], kernel_boot_info: &KernelBootInfo) {
    mem_chunks.fill(Memory { base: 0, size: 0 });

    let max_memmap = 10.min(NR_MEMS);
    for i in 0..max_memmap {
        mem_chunks[i].base = kernel_boot_info.memmap[i].mm_base_addr;
        mem_chunks[i].size = kernel_boot_info.memmap[i].mm_length;
    }

    for i in 0..NR_MEMS {
        let base = mem_chunks[i].base;
        let size = mem_chunks[i].size;
        let limit = base + size;
        
        let aligned_base = (base + (1 << CLICK_SHIFT) - 1) & !((1 << CLICK_SHIFT) - 1);
        let aligned_limit = limit & !((1 << CLICK_SHIFT) - 1);
        
        if aligned_limit <= aligned_base {
            mem_chunks[i].base = 0;
            mem_chunks[i].size = 0;
        } else {
            mem_chunks[i].base = aligned_base >> CLICK_SHIFT;
            mem_chunks[i].size = (aligned_limit - aligned_base) >> CLICK_SHIFT;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmError {
    InvalidEndpoint,
    DeadEndpoint,
}

pub fn vm_isokendpt(endpoint: i32, vmproc: &[VmProc; NR_PROCS]) -> Result<usize, VmError> {
    let procn = ((endpoint >> 8) & 0xFF) as usize;
    
    if procn >= NR_PROCS {
        return Err(VmError::InvalidEndpoint);
    }
    
    if endpoint != vmproc[procn].vm_endpoint {
        return Err(VmError::DeadEndpoint);
    }
    
    if !vmproc[procn].vm_flags.contains(VmFlags::INUSE) {
        return Err(VmError::DeadEndpoint);
    }
    
    Ok(procn)
}

#[derive(Debug, Clone, Copy)]
pub struct VmProc {
    pub vm_endpoint: i32,
    pub vm_flags: VmFlags,
}

bitflags::bitflags! {
    pub struct VmFlags: u32 {
        const INUSE = 0x01;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_mem_chunks() {
        let mut mem_chunks = [Memory { base: 0, size: 0 }; NR_MEMS];
        let kernel_boot_info = KernelBootInfo {
            memmap: [
                MemoryMapEntry { mm_base_addr: 0x1000, mm_length: 0x2000 },
                MemoryMapEntry { mm_base_addr: 0, mm_length: 0 },
                // ... 其他条目
            ],
        };
        
        get_mem_chunks(&mut mem_chunks, &kernel_boot_info);
        
        assert!(mem_chunks[0].size > 0);
    }

    #[test]
    fn test_vm_isokendpt() {
        let mut vmproc = [VmProc {
            vm_endpoint: 0,
            vm_flags: VmFlags::empty(),
        }; NR_PROCS];
        
        vmproc[0].vm_endpoint = 0x100;
        vmproc[0].vm_flags = VmFlags::INUSE;
        
        let result = vm_isokendpt(0x100, &vmproc);
        assert_eq!(result, Ok(0));
        
        let result = vm_isokendpt(0x200, &vmproc);
        assert_eq!(result, Err(VmError::DeadEndpoint));
    }
}
```

### Rust 优势分析

**1. Result 类型**:
```rust
// C: 使用错误码
int vm_isokendpt(endpoint_t endpoint, int *procn) {
    if (error) return EINVAL;
    return OK;
}

// Rust: 使用 Result
pub fn vm_isokendpt(endpoint: i32, vmproc: &[VmProc; NR_PROCS]) -> Result<usize, VmError> {
    if error {
        return Err(VmError::InvalidEndpoint);
    }
    Ok(procn)
}
```

**2. 数组初始化**:
```rust
// C: 使用 memset
memset(mem_chunks, 0, NR_MEMS*sizeof(*mem_chunks));

// Rust: 使用 fill
mem_chunks.fill(Memory { base: 0, size: 0 });
```

**3. 位操作**:
```rust
// C: 使用宏
#define CLICK_CEIL(x) (((x) + CLICK_SIZE - 1) & ~(CLICK_SIZE - 1))

// Rust: 使用内联函数
let aligned_base = (base + (1 << CLICK_SHIFT) - 1) & !((1 << CLICK_SHIFT) - 1);
```

**4. 类型安全**:
```rust
// C: 使用整数
int procn;

// Rust: 使用类型
let procn: usize;
```

---

## 要点总结

1. **内存块初始化**: get_mem_chunks 从内核获取内存映射并对齐到 click 边界
2. **端点验证**: vm_isokendpt 验证端点有效性，防止非法访问
3. **信息查询**: do_info 提供统计、使用、区域信息查询，避免死锁

---

## 灾难预演

**如果 get_mem_chunks 对齐错误**:
- 内存块不在 click 边界上
- 页表映射错误
- 内存访问失败

**如果 vm_isokendpt 验证不严格**:
- 非法端点访问
- 进程数据泄露
- 系统崩溃

**如果 do_info 拷贝时触发页错误**:
- VM 等待 sys_datacopy 返回
- 内核发送 notify 给 VM
- 死锁！

---

## 互动自测

1. **问题**: get_mem_chunks 的作用是什么？
   **答案**: 从内核获取内存映射并对齐到 click 边界。

2. **问题**: 为什么需要 CLICK_CEIL 和 CLICK_FLOOR？
   **答案**: 确保内存块在 click 边界上，简化内存管理。

3. **问题**: do_info 如何避免死锁？
   **答案**: 先调用 handle_memory_once 处理内存，确保拷贝不会触发页错误。
