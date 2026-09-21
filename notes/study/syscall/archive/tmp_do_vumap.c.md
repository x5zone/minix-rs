# do_vumap.c 详细讲解

**文件位置**: `minix3/minix/kernel/system/do_vumap.c`

**总行数**: 131 行

**作用**: 实现 `SYS_VUMAP` 系统调用，将虚拟地址向量映射为物理地址向量

---

## 一、文件概述

### 1.1 是什么（What）

`do_vumap.c` 实现了 MINIX3 的**向量地址映射系统调用**：

| 系统调用 | 功能 |
|---------|------|
| `SYS_VUMAP` | 将虚拟地址向量批量映射为物理地址向量 |

**核心功能**：批量处理多个虚拟地址到物理地址的映射，专为 DMA 优化设计。

### 1.2 为什么需要（Why）

**设计原因**：
- DMA 操作通常需要多个不连续的内存块
- 单次调用处理多个映射，减少系统调用开销
- 驱动程序可以一次性获取所有需要的物理地址

**与 do_umap 的区别**：

| 特性 | do_umap | do_vumap |
|------|---------|----------|
| 处理数量 | 单个地址 | 多个地址（向量） |
| 使用场景 | 简单映射 | DMA scatter-gather |
| 效率 | 多次调用 | 单次调用 |

### 1.3 使用场景（When）

| 场景 | 说明 |
|------|------|
| DMA scatter-gather | 设备需要多个不连续的内存块 |
| 网络驱动 | 处理分散的缓冲区 |
| 块设备驱动 | 处理不连续的页面 |
| 高性能 I/O | 减少系统调用次数 |

---

## 二、逐行讲解

### 2.1 文件头注释（第 1-14 行）

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_VUMAP
 *
 * The parameters for this kernel call are:
 *   m_lsys_krn_sys_vumap.endpt		(grant owner, or SELF for local addresses)
 *   m_lsys_krn_sys_vumap.vaddr		(address of virtual (input) vector)
 *   m_lsys_krn_sys_vumap.vcount	(number of elements in virtual vector)
 *   m_lsys_krn_sys_vumap.offset	(offset into first entry of input vector)
 *   m_lsys_krn_sys_vumap.access	(safecopy access requested for input)
 *   m_lsys_krn_sys_vumap.paddr		(address of physical (output) vector)
 *   m_lsys_krn_sys_vumap.pmax		(maximum number of physical vector elements)
 *   m_krn_lsys_sys_vumap.pcount	(upon return: number of elements filled)
 */
```

**逐字段解析**：

| 字段 | 类型 | 含义 |
|------|------|------|
| `endpt` | `endpoint_t` | 授权拥有者端点 |
| `vaddr` | `vir_bytes` | 输入向量地址 |
| `vcount` | `int` | 输入向量元素数 |
| `offset` | `size_t` | 第一个元素的偏移 |
| `access` | `int` | 访问权限（读/写） |
| `paddr` | `vir_bytes` | 输出向量地址 |
| `pmax` | `int` | 输出向量最大元素数 |
| `pcount` | `int` | 返回实际填充的元素数 |

### 2.2 头文件包含（第 16-18 行）

```c
#include "kernel/system.h"

#include <assert.h>
```

| 头文件 | 作用 |
|--------|------|
| `"kernel/system.h"` | 系统调用框架，`struct proc` 定义 |
| `<assert.h>` | 断言宏 |

### 2.3 do_vumap 函数签名（第 20-22 行）

```c
/*===========================================================================*
 *				do_vumap				     *
 *===========================================================================*/
int do_vumap(struct proc *caller, message *m_ptr)
```

**参数**：
- `caller` - 调用者进程指针
- `m_ptr` - 消息指针，包含映射参数

**返回值**：
- `OK` - 映射成功
- `EINVAL` - 无效参数
- `EFAULT` - 内存访问错误

### 2.4 函数注释（第 23-27 行）

```c
/* Map a vector of grants or local virtual addresses to physical addresses.
 * Designed to be used by drivers to perform an efficient lookup of physical
 * addresses for the purpose of direct DMA from/to a remote process.
 */
```

**翻译**：将授权或本地虚拟地址向量映射为物理地址。专为驱动程序设计，用于高效查找物理地址以进行 DMA 操作。

### 2.5 局部变量（第 28-37 行）

```c
  endpoint_t endpt, source, granter;
  struct proc *procp;
  struct vumap_vir vvec[MAPVEC_NR];
  struct vumap_phys pvec[MAPVEC_NR];
  vir_bytes vaddr, paddr, vir_addr;
  phys_bytes phys_addr;
  int i, r, proc_nr, vcount, pcount, pmax, access;
  size_t size, chunk, offset;
```

| 变量 | 类型 | 大小 | 用途 |
|------|------|------|------|
| `endpt` | `endpoint_t` | 4 字节 | 调用者端点 |
| `source` | `endpoint_t` | 4 字节 | 授权拥有者端点 |
| `granter` | `endpoint_t` | 4 字节 | 实际授权者端点 |
| `procp` | `struct proc *` | 8 字节 | 目标进程指针 |
| `vvec[MAPVEC_NR]` | `struct vumap_vir[]` | 数组 | 输入虚拟地址向量 |
| `pvec[MAPVEC_NR]` | `struct vumap_phys[]` | 数组 | 输出物理地址向量 |
| `vaddr` | `vir_bytes` | 4/8 字节 | 输入向量地址 |
| `paddr` | `vir_bytes` | 4/8 字节 | 输出向量地址 |
| `vir_addr` | `vir_bytes` | 4/8 字节 | 当前虚拟地址 |
| `phys_addr` | `phys_bytes` | 4/8 字节 | 当前物理地址 |
| `vcount` | `int` | 4 字节 | 输入向量元素数 |
| `pcount` | `int` | 4 字节 | 输出向量元素数 |
| `pmax` | `int` | 4 字节 | 输出向量最大元素数 |
| `access` | `int` | 4 字节 | 访问权限 |
| `size` | `size_t` | 4/8 字节 | 当前处理大小 |
| `chunk` | `size_t` | 4/8 字节 | 当前物理块大小 |
| `offset` | `size_t` | 4/8 字节 | 偏移量 |

### 2.6 获取调用者端点（第 39 行）

```c
  endpt = caller->p_endpoint;
```

### 2.7 解析输入参数（第 41-49 行）

```c
  /* Retrieve and check input parameters. */
  source = m_ptr->m_lsys_krn_sys_vumap.endpt;
  vaddr = m_ptr->m_lsys_krn_sys_vumap.vaddr;
  vcount = m_ptr->m_lsys_krn_sys_vumap.vcount;
  offset = m_ptr->m_lsys_krn_sys_vumap.offset;
  access = m_ptr->m_lsys_krn_sys_vumap.access;
  paddr = m_ptr->m_lsys_krn_sys_vumap.paddr;
  pmax = m_ptr->m_lsys_krn_sys_vumap.pmax;
```

### 2.8 参数验证（第 51-54 行）

```c
  if (vcount <= 0 || pmax <= 0)
	return EINVAL;

  if (vcount > MAPVEC_NR) vcount = MAPVEC_NR;
  if (pmax > MAPVEC_NR) pmax = MAPVEC_NR;
```

**MAPVEC_NR 常量**：限制最大向量元素数，防止栈溢出。

### 2.9 访问权限转换（第 56-62 行）

```c
  /* Convert access to safecopy access flags. */
  switch (access) {
  case VUA_READ:		access = CPF_READ; break;
  case VUA_WRITE:		access = CPF_WRITE; break;
  case VUA_READ|VUA_WRITE:	access = CPF_READ|CPF_WRITE; break;
  default:			return EINVAL;
  }
```

**访问权限映射**：

| 输入标志 | 输出标志 | 含义 |
|---------|---------|------|
| `VUA_READ` | `CPF_READ` | 读取权限 |
| `VUA_WRITE` | `CPF_WRITE` | 写入权限 |
| `VUA_READ|VUA_WRITE` | `CPF_READ|CPF_WRITE` | 读写权限 |

### 2.10 复制输入向量（第 64-67 行）

```c
  /* Copy in the vector of virtual addresses. */
  size = vcount * sizeof(vvec[0]);

  if (data_copy(endpt, vaddr, KERNEL, (vir_bytes) vvec, size) != OK)
	return EFAULT;
```

**内存布局**：

```
用户空间:
┌─────────────────────────────────────────────────────────────────────────┐
│  vaddr ──► [vvec[0], vvec[1], vvec[2], ..., vvec[vcount-1]]             │
└─────────────────────────────────────────────────────────────────────────┘
                    │
                    │ data_copy()
                    ▼
内核栈:
┌─────────────────────────────────────────────────────────────────────────┐
│  vvec[MAPVEC_NR]                                                        │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.11 初始化输出计数（第 69 行）

```c
  pcount = 0;
```

### 2.12 主循环（第 71-109 行）

```c
  /* Go through the input entries, one at a time. Stop early in case the output
   * vector has filled up.
   */
  for (i = 0; i < vcount && pcount < pmax; i++) {
	size = vvec[i].vv_size;
	if (size <= offset)
		return EINVAL;
	size -= offset;

	if (source != SELF) {
		r = verify_grant(source, endpt, vvec[i].vv_grant, size, access,
			offset, &vir_addr, &granter, NULL);
		if (r != OK)
			return r;
	} else {
		vir_addr = vvec[i].vv_addr + offset;
		granter = endpt;
	}

	okendpt(granter, &proc_nr);
	procp = proc_addr(proc_nr);

	/* Each virtual range is made up of one or more physical ranges. */
	while (size > 0 && pcount < pmax) {
		chunk = vm_lookup_range(procp, vir_addr, &phys_addr, size);

		if (!chunk) {
			/* Try to get the memory allocated, unless the memory
			 * is supposed to be there to be read from.
			 */
			if (access & CPF_READ)
				return EFAULT;

			/* This call may suspend the current call, or return an
			 * error for a previous invocation.
			 */
			return vm_check_range(caller, procp, vir_addr, size, 1);
		}

		pvec[pcount].vp_addr = phys_addr;
		pvec[pcount].vp_size = chunk;
		pcount++;

		vir_addr += chunk;
		size -= chunk;
	}

	offset = 0;
  }
```

**处理流程**：

```
┌─────────────────────────────────────────────────────────────────────────┐
│  主循环处理流程                                                          │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  for (i = 0; i < vcount && pcount < pmax; i++) {                        │
│                                                                         │
│    1. 获取虚拟地址范围大小                                               │
│       size = vvec[i].vv_size - offset                                   │
│                                                                         │
│    2. 解析虚拟地址                                                      │
│       ├── source != SELF: 验证授权                                      │
│       └── source == SELF: 直接使用地址                                  │
│                                                                         │
│    3. 内层循环：处理一个虚拟范围到多个物理范围                           │
│       while (size > 0 && pcount < pmax) {                               │
│         ├── vm_lookup_range() 获取物理地址和连续块大小                  │
│         ├── 如果失败且需要写入，调用 vm_check_range()                   │
│         ├── 填充 pvec[pcount]                                           │
│         └── 更新 vir_addr 和 size                                       │
│       }                                                                 │
│                                                                         │
│    4. offset = 0（只对第一个元素使用偏移）                               │
│  }                                                                      │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**关键点**：
- 一个虚拟地址范围可能映射到多个不连续的物理块
- `vm_lookup_range()` 返回连续物理块的大小

### 2.13 复制输出向量（第 111-119 行）

```c
  /* Copy out the resulting vector of physical addresses. */
  assert(pcount > 0);

  size = pcount * sizeof(pvec[0]);

  r = data_copy_vmcheck(caller, KERNEL, (vir_bytes) pvec, endpt, paddr, size);

  if (r == OK)
	m_ptr->m_krn_lsys_sys_vumap.pcount = pcount;

  return r;
```

**内存布局**：

```
内核栈:
┌─────────────────────────────────────────────────────────────────────────┐
│  pvec[MAPVEC_NR]                                                        │
│  ├── pvec[0] = {phys_addr_0, size_0}                                    │
│  ├── pvec[1] = {phys_addr_1, size_1}                                    │
│  └── ...                                                                │
└─────────────────────────────────────────────────────────────────────────┘
                    │
                    │ data_copy_vmcheck()
                    ▼
用户空间:
┌─────────────────────────────────────────────────────────────────────────┐
│  paddr ──► [pvec[0], pvec[1], ..., pvec[pcount-1]]                      │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 三、向量结构详解

### 3.1 vumap_vir 结构（输入）

```c
struct vumap_vir {
    cp_grant_id_t vv_grant;   // 授权 ID（source != SELF 时使用）
    vir_bytes vv_addr;        // 虚拟地址（source == SELF 时使用）
    size_t vv_size;           // 大小
};
```

### 3.2 vumap_phys 结构（输出）

```c
struct vumap_phys {
    phys_bytes vp_addr;       // 物理地址
    size_t vp_size;           // 连续块大小
};
```

### 3.3 映射示例

```
输入向量 (vvec):
┌─────────────────────────────────────────────────────────────────────────┐
│  vvec[0]: grant=100, size=4096                                          │
│  vvec[1]: grant=101, size=2048                                          │
│  vvec[2]: grant=102, size=8192                                          │
└─────────────────────────────────────────────────────────────────────────┘

可能输出向量 (pvec):
┌─────────────────────────────────────────────────────────────────────────┐
│  pvec[0]: phys_addr=0x10000, size=2048  (vvec[0] 的前半部分)            │
│  pvec[1]: phys_addr=0x20000, size=2048  (vvec[0] 的后半部分)            │
│  pvec[2]: phys_addr=0x30000, size=2048  (vvec[1] 全部)                  │
│  pvec[3]: phys_addr=0x40000, size=4096  (vvec[2] 的前半部分)            │
│  pvec[4]: phys_addr=0x50000, size=4096  (vvec[2] 的后半部分)            │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 四、DMA Scatter-Gather 示例

### 4.1 网络驱动场景

```
┌─────────────────────────────────────────────────────────────────────────┐
│  网络驱动 DMA 场景                                                       │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  1. 用户进程提供分散的缓冲区:                                           │
│     ├── 缓冲区 A: 0x1000-0x1FFF (4KB)                                  │
│     ├── 缓冲区 B: 0x3000-0x3FFF (4KB)                                  │
│     └── 缓冲区 C: 0x5000-0x5FFF (4KB)                                  │
│                                                                         │
│  2. 驱动调用 SYS_VUMAP:                                                 │
│     ├── vvec[0] = {grant_A, 4096}                                      │
│     ├── vvec[1] = {grant_B, 4096}                                      │
│     └── vvec[2] = {grant_C, 4096}                                      │
│                                                                         │
│  3. 内核返回物理地址:                                                   │
│     ├── pvec[0] = {0x80000, 4096}                                      │
│     ├── pvec[1] = {0x90000, 4096}                                      │
│     └── pvec[2] = {0xA0000, 4096}                                      │
│                                                                         │
│  4. 驱动配置 DMA 引擎:                                                  │
│     └── 使用 scatter-gather 列表直接传输                               │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 五、现代硬件适配建议

| 方面 | C 语言实现 | 现代硬件适配 |
|------|-----------|-------------|
| 向量处理 | 软件循环 | 使用 SIMD 优化 |
| DMA 映射 | 软件查找 | 使用 IOMMU |
| 内存分配 | 按需分配 | 预分配缓冲池 |
| 缓存一致性 | 手动刷新 | 硬件自动维护 |

---

## 六、Rust 重构建议

```rust
use core::result::Result;

#[derive(Debug, Clone, Copy)]
pub enum VumapError {
    InvalidParam,
    InvalidAccess,
    CopyFailed,
    GrantVerificationFailed,
    AddressLookupFailed,
}

bitflags! {
    pub struct VuaAccess: u32 {
        const VUA_READ  = 0x01;
        const VUA_WRITE = 0x02;
    }
}

pub struct VumapVir {
    pub grant: cp_grant_id_t,
    pub addr: VirtAddr,
    pub size: usize,
}

pub struct VumapPhys {
    pub addr: PhysAddr,
    pub size: usize,
}

pub struct VumapRequest {
    pub endpt: Endpoint,
    pub vaddr: VirtAddr,
    pub vcount: usize,
    pub offset: usize,
    pub access: VuaAccess,
    pub paddr: VirtAddr,
    pub pmax: usize,
}

pub struct VumapResult {
    pub pcount: usize,
}

pub fn do_vumap(
    caller: &Proc,
    request: &VumapRequest,
) -> Result<VumapResult, VumapError> {
    if request.vcount == 0 || request.pmax == 0 {
        return Err(VumapError::InvalidParam);
    }

    let vcount = request.vcount.min(MAPVEC_NR);
    let pmax = request.pmax.min(MAPVEC_NR);

    let access = match request.access {
        VuaAccess::VUA_READ => CpfFlags::CPF_READ,
        VuaAccess::VUA_WRITE => CpfFlags::CPF_WRITE,
        VuaAccess::VUA_READ | VuaAccess::VUA_WRITE => {
            CpfFlags::CPF_READ | CpfFlags::CPF_WRITE
        }
        _ => return Err(VumapError::InvalidAccess),
    };

    let mut vvec: [VumapVir; MAPVEC_NR] = unsafe { MaybeUninit::uninit().assume_init() };
    let mut pvec: [VumapPhys; MAPVEC_NR] = unsafe { MaybeUninit::uninit().assume_init() };

    let size = vcount * core::mem::size_of::<VumapVir>();
    data_copy(
        caller.endpoint(),
        request.vaddr,
        Endpoint::KERNEL,
        &mut vvec as *mut _ as VirtAddr,
        size,
    ).map_err(|_| VumapError::CopyFailed)?;

    let mut pcount = 0;
    let mut offset = request.offset;

    for i in 0..vcount {
        if pcount >= pmax {
            break;
        }

        let mut size = vvec[i].size;
        if size <= offset {
            return Err(VumapError::InvalidParam);
        }
        size -= offset;

        let (vir_addr, granter) = if request.endpt != Endpoint::SELF {
            let (addr, ep) = verify_grant(
                request.endpt,
                caller.endpoint(),
                vvec[i].grant,
                size,
                access,
                offset,
            ).map_err(|_| VumapError::GrantVerificationFailed)?;
            (addr, ep)
        } else {
            (vvec[i].addr + offset, caller.endpoint())
        };

        let procp = Proc::from_endpoint(granter).ok_or(VumapError::InvalidParam)?;

        while size > 0 && pcount < pmax {
            let (chunk, phys_addr) = vm_lookup_range(procp, vir_addr, size)
                .ok_or(VumapError::AddressLookupFailed)?;

            if chunk == 0 {
                if access.contains(CpfFlags::CPF_READ) {
                    return Err(VumapError::AddressLookupFailed);
                }
                return vm_check_range(caller, procp, vir_addr, size, true);
            }

            pvec[pcount] = VumapPhys {
                addr: phys_addr,
                size: chunk,
            };
            pcount += 1;

            vir_addr += chunk;
            size -= chunk;
        }

        offset = 0;
    }

    assert!(pcount > 0);

    let size = pcount * core::mem::size_of::<VumapPhys>();
    data_copy_vmcheck(
        caller,
        Endpoint::KERNEL,
        &pvec as *const _ as VirtAddr,
        caller.endpoint(),
        request.paddr,
        size,
    ).map_err(|_| VumapError::CopyFailed)?;

    Ok(VumapResult { pcount })
}
```

---

## 七、要点总结

### 核心知识点

1. **向量映射提高效率**：
   - 单次调用处理多个地址
   - 减少 DMA 场景的系统调用开销

2. **一个虚拟范围可能映射到多个物理块**：
   - 虚拟连续不等于物理连续
   - 需要内层循环处理

3. **offset 只对第一个元素生效**：
   - 用于跳过缓冲区开头的部分
   - 后续元素从偏移 0 开始

---

## 八、灾难预演

### 场景 1：如果删掉 pcount < pmax 检查

```
后果：
1. 输出向量溢出
2. 栈破坏
3. 系统崩溃
```

### 场景 2：如果不清除 offset 重置

```
后果：
1. 后续元素使用错误的偏移
2. 地址计算错误
3. 数据损坏
```

### 场景 3：如果 vm_lookup_range 返回 0 但继续

```
后果：
1. 无限循环
2. 系统挂起
```

---

## 九、互动自测

1. **问题**：为什么需要内层 while 循环？
   **答案**：一个虚拟地址范围可能映射到多个不连续的物理块，需要逐个处理。

2. **问题**：`offset` 参数的作用是什么？
   **答案**：跳过第一个元素的开始部分，后续元素从偏移 0 开始。

3. **问题**：为什么输出向量元素数可能大于输入向量元素数？
   **答案**：一个虚拟范围可能映射到多个物理块，导致输出元素数增加。

---

## 十、参考文献

| 文件 | 描述 |
|------|------|
| `kernel/vm.h` | vm_lookup_range 函数声明 |
| `kernel/system/do_umap.c` | 单地址映射 |
| `include/minix/vumap.h` | 向量结构定义 |
| `drivers/*/dma.c` | DMA 使用示例 |

---

*讲解日期：2026-03-30*
*讲解者：Minix-rs 学习助手*
