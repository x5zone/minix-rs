# do_umap.c 详细讲解

**文件位置**: `minix3/minix/kernel/system/do_umap.c`

**总行数**: 39 行

**作用**: 实现 `SYS_UMAP` 系统调用，将虚拟地址映射为物理地址

---

## 一、文件概述

### 1.1 是什么（What）

`do_umap.c` 实现了 MINIX3 的**地址映射系统调用**：

| 系统调用 | 功能 |
|---------|------|
| `SYS_UMAP` | 将虚拟地址转换为物理地址 |

**核心功能**：给定一个进程的虚拟地址，返回对应的物理地址。

### 1.2 为什么需要（Why）

**使用场景**：

| 场景 | 说明 |
|------|------|
| DMA 传输 | 设备需要物理地址进行 DMA |
| 内存映射 I/O | 驱动程序需要访问设备内存 |
| 跨进程内存访问 | 需要知道目标进程的物理地址 |

**设计原因**：
- 内核知道虚拟地址到物理地址的映射
- 用户进程无法直接访问页表
- 需要一个安全的接口来获取物理地址

### 1.3 与 do_umap_remote 的关系

```
┌─────────────────────────────────────────────────────────────────────────┐
│  do_umap (本文件)                                                       │
│  ├── 只能映射调用者自己的地址空间                                       │
│  ├── 或者映射调用者作为被授权者的授权                                   │
│  └── 安全检查后调用 do_umap_remote                                     │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│  do_umap_remote                                                         │
│  ├── 可以映射任意进程的地址空间                                         │
│  ├── 支持授权验证                                                       │
│  └── 实际执行地址映射                                                   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 二、逐行讲解

### 2.1 文件头注释（第 1-11 行）

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_UMAP
 *
 * The parameters for this kernel call are:
 *   m_lsys_krn_sys_umap.src_endpt	(process number)
 *   m_lsys_krn_sys_umap.segment	(segment where address is: T, D, or S)
 *   m_lsys_krn_sys_umap.src_addr	(virtual address)
 *   m_krn_lsys_sys_umap.dst_addr	(returns physical address)
 *   m_lsys_krn_sys_umap.nr_bytes	(size of datastructure)
 */
```

**逐字段解析**：

| 字段 | 类型 | 含义 |
|------|------|------|
| `src_endpt` | `endpoint_t` | 目标进程端点 |
| `segment` | `int` | 段类型（代码段/数据段/栈段） |
| `src_addr` | `vir_bytes` | 虚拟地址 |
| `dst_addr` | `phys_bytes` | 返回的物理地址 |
| `nr_bytes` | `size_t` | 数据大小 |

**段类型常量**：

```c
#define T            0    /* 文本段（代码段） */
#define D            1    /* 数据段 */
#define S            2    /* 栈段 */
#define MEM_GRANT    3    /* 授权内存 */
```

### 2.2 头文件包含（第 13-15 行）

```c
#include "kernel/system.h"

#include <minix/endpoint.h>
```

| 头文件 | 作用 |
|--------|------|
| `"kernel/system.h"` | 系统调用框架，`struct proc` 定义 |
| `<minix/endpoint.h>` | 端点类型定义，`SELF`、`NONE` 宏 |

### 2.3 条件编译（第 17-21 行）

```c
#if USE_UMAP

#if ! USE_UMAP_REMOTE
#undef do_umap_remote
#endif
```

**设计原因**：
- `USE_UMAP` 控制是否编译此功能
- 如果 `USE_UMAP_REMOTE` 未启用，则取消 `do_umap_remote` 定义

### 2.4 do_umap 函数签名（第 23-25 行）

```c
/*==========================================================================*
 *				do_umap					    *
 *==========================================================================*/
int do_umap(struct proc * caller, message * m_ptr)
```

**参数**：
- `caller` - 调用者进程指针
- `m_ptr` - 消息指针，包含映射参数

**返回值**：
- `OK` - 映射成功
- `EPERM` - 权限不足
- 其他错误码由 `do_umap_remote` 返回

### 2.5 局部变量（第 27-28 行）

```c
  int seg_index = m_ptr->m_lsys_krn_sys_umap.segment & SEGMENT_INDEX;
  int endpt = m_ptr->m_lsys_krn_sys_umap.src_endpt;
```

| 变量 | 类型 | 大小 | 用途 |
|------|------|------|------|
| `seg_index` | `int` | 4 字节 | 段索引（从 segment 字段提取） |
| `endpt` | `int` | 4 字节 | 目标进程端点 |

**SEGMENT_INDEX 宏**：

```c
#define SEGMENT_TYPE   0xF0    /* 段类型掩码 */
#define SEGMENT_INDEX  0x0F    /* 段索引掩码 */
```

**设计原因**：`segment` 字段同时编码了段类型和段索引，需要用掩码提取。

### 2.6 安全检查（第 30-34 行）

```c
  /* This call is a subset of umap_remote, it allows mapping virtual addresses
   * in the caller's address space and grants where the caller is specified as
   * grantee; after the security check we simply invoke do_umap_remote
   */
  if (seg_index != MEM_GRANT && endpt != SELF) return EPERM;
```

**安全规则**：

| 条件 | 结果 |
|------|------|
| `seg_index == MEM_GRANT` | 允许（授权内存） |
| `endpt == SELF` | 允许（自己的地址空间） |
| 其他情况 | 返回 `EPERM` |

**设计原因**：
- `SYS_UMAP` 是受限版本的 `SYS_UMAP_REMOTE`
- 只允许映射自己的地址空间
- 或者映射自己作为被授权者的授权

### 2.7 设置被授权者（第 35 行）

```c
  m_ptr->m_lsys_krn_sys_umap.dst_endpt = SELF;
```

**设计原因**：
- `do_umap_remote` 需要知道被授权者是谁
- 对于 `SYS_UMAP`，被授权者就是调用者自己
- 设置 `dst_endpt = SELF` 表示"我自己是被授权者"

### 2.8 调用 do_umap_remote（第 36 行）

```c
  return do_umap_remote(caller, m_ptr);
```

**设计原因**：
- 避免代码重复
- `do_umap_remote` 实现了完整的映射逻辑
- `do_umap` 只是添加了安全检查

### 2.9 条件编译结束（第 38 行）

```c
#endif /* USE_UMAP */
```

---

## 三、do_umap_remote 核心逻辑

由于 `do_umap` 调用了 `do_umap_remote`，这里简要介绍其核心逻辑：

### 3.1 主要步骤

```
┌─────────────────────────────────────────────────────────────────────────┐
│  do_umap_remote 执行流程                                                │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  1. 验证目标进程端点                                                    │
│     └── endpt == SELF ? 使用调用者 : 验证端点有效性                     │
│                                                                         │
│  2. 验证被授权者端点                                                    │
│     └── grantee == SELF ? 使用调用者 : 验证端点有效性                   │
│                                                                         │
│  3. 根据 seg_type 执行映射                                              │
│     ├── LOCAL_VM_SEG:                                                   │
│     │   ├── MEM_GRANT: 验证授权，获取新偏移和端点                       │
│     │   └── VIR_ADDR: 直接使用虚拟地址                                  │
│     └── 其他: 返回 EINVAL                                              │
│                                                                         │
│  4. 调用 vm_lookup 获取物理地址                                         │
│                                                                         │
│  5. 验证地址范围连续性                                                  │
│                                                                         │
│  6. 返回物理地址                                                        │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.2 授权验证流程

```c
if(verify_grant(targetpr->p_endpoint, grantee, grant, count,
        0, 0, &newoffset, &newep, NULL) != OK) {
    // 授权验证失败
    return EFAULT;
}
```

**verify_grant 参数**：
- `targetpr->p_endpoint` - 授权者端点
- `grantee` - 被授权者端点
- `grant` - 授权 ID
- `count` - 访问大小
- `newoffset` - 返回新的偏移
- `newep` - 返回新的端点（magic grant）

---

## 四、使用场景详解

### 4.1 DMA 场景

```
用户进程                    内核                      设备
   │                         │                         │
   │  1. 请求 DMA 传输       │                         │
   │  (虚拟地址)             │                         │
   ├────────────────────────►│                         │
   │                         │  2. SYS_UMAP            │
   │                         │  虚拟地址 → 物理地址    │
   │                         │                         │
   │                         │  3. 配置 DMA            │
   │                         ├────────────────────────►│
   │                         │  (物理地址)             │
   │                         │                         │
   │                         │  4. DMA 传输            │
   │                         │◄────────────────────────┤
   │                         │                         │
```

### 4.2 授权内存映射

```
进程 A (授权者)             内核                      进程 B (被授权者)
   │                         │                         │
   │  1. 创建授权            │                         │
   ├────────────────────────►│                         │
   │                         │                         │
   │                         │  2. B 使用 SYS_UMAP     │
   │                         │◄────────────────────────┤
   │                         │  映射授权内存           │
   │                         │                         │
   │                         │  3. 验证授权            │
   │                         │  返回物理地址           │
   │                         ├────────────────────────►│
   │                         │                         │
```

---

## 五、现代硬件适配建议

| 方面 | C 语言实现 | 现代硬件适配 |
|------|-----------|-------------|
| 地址翻译 | 软件查找 | 利用 IOMMU |
| DMA 安全 | 手动验证 | IOMMU 硬件隔离 |
| 授权机制 | 软件验证 | capability 系统 |
| 缓存一致性 | 手动刷新 | 硬件自动维护 |

---

## 六、Rust 重构建议

```rust
use core::result::Result;

#[derive(Debug, Clone, Copy)]
pub enum UmapError {
    InvalidEndpoint,
    InvalidSegment,
    InvalidAddress,
    PermissionDenied,
    GrantVerificationFailed,
    NotContiguous,
}

bitflags! {
    pub struct SegmentFlags: u32 {
        const SEGMENT_TYPE  = 0xF0;
        const SEGMENT_INDEX = 0x0F;
        const MEM_GRANT     = 0x03;
        const VIR_ADDR      = 0x00;
        const LOCAL_VM_SEG  = 0x00;
    }
}

pub struct UmapRequest {
    pub src_endpoint: Endpoint,
    pub segment: SegmentFlags,
    pub src_addr: VirtAddr,
    pub nr_bytes: usize,
}

pub struct UmapResult {
    pub phys_addr: PhysAddr,
}

pub fn do_umap(
    caller: &Proc,
    request: &UmapRequest,
) -> Result<UmapResult, UmapError> {
    let seg_index = request.segment & SegmentFlags::SEGMENT_INDEX;
    let endpt = request.src_endpoint;

    if seg_index != SegmentFlags::MEM_GRANT && endpt != Endpoint::SELF {
        return Err(UmapError::PermissionDenied);
    }

    let remote_request = UmapRemoteRequest {
        src_endpoint: request.src_endpoint,
        dst_endpoint: Endpoint::SELF,
        segment: request.segment,
        src_addr: request.src_addr,
        nr_bytes: request.nr_bytes,
    };

    do_umap_remote(caller, &remote_request)
}

pub fn do_umap_remote(
    caller: &Proc,
    request: &UmapRemoteRequest,
) -> Result<UmapResult, UmapError> {
    let target_proc = if request.src_endpoint == Endpoint::SELF {
        caller
    } else {
        Proc::from_endpoint(request.src_endpoint)
            .ok_or(UmapError::InvalidEndpoint)?
    };

    let grantee = if request.dst_endpoint == Endpoint::SELF {
        caller.endpoint()
    } else {
        request.dst_endpoint
    };

    let seg_type = request.segment & SegmentFlags::SEGMENT_TYPE;
    let seg_index = request.segment & SegmentFlags::SEGMENT_INDEX;

    let (lin_addr, final_proc) = match seg_type {
        SegmentFlags::LOCAL_VM_SEG => {
            if seg_index == SegmentFlags::MEM_GRANT {
                let grant = request.src_addr as cp_grant_id_t;
                let (new_offset, new_ep) = verify_grant(
                    target_proc.endpoint(),
                    grantee,
                    grant,
                    request.nr_bytes,
                ).map_err(|_| UmapError::GrantVerificationFailed)?;

                let new_proc = Proc::from_endpoint(new_ep)
                    .ok_or(UmapError::InvalidEndpoint)?;
                (new_offset, new_proc)
            } else if seg_index == SegmentFlags::VIR_ADDR {
                (request.src_addr, target_proc)
            } else {
                return Err(UmapError::InvalidSegment);
            }
        }
        _ => return Err(UmapError::InvalidSegment),
    };

    let phys_addr = vm_lookup(final_proc, lin_addr)
        .map_err(|_| UmapError::InvalidAddress)?;

    if vm_lookup_range(final_proc, lin_addr, request.nr_bytes) != request.nr_bytes {
        return Err(UmapError::NotContiguous);
    }

    Ok(UmapResult { phys_addr })
}
```

---

## 七、要点总结

### 核心知识点

1. **SYS_UMAP 是受限版本**：
   - 只能映射自己的地址空间
   - 或者映射自己作为被授权者的授权

2. **安全检查是关键**：
   - `seg_index != MEM_GRANT && endpt != SELF` 返回 `EPERM`
   - 防止任意进程映射其他进程的内存

3. **委托给 do_umap_remote**：
   - 避免代码重复
   - 实际映射逻辑在 `do_umap_remote` 中

---

## 八、灾难预演

### 场景 1：如果删掉安全检查

```
后果：
1. 任何进程可以映射其他进程的内存
2. 完全破坏进程隔离
3. 安全灾难
```

### 场景 2：如果允许任意端点

```
后果：
1. 可以获取任意进程的物理地址
2. 可以绕过内存保护
3. 数据泄露
```

### 场景 3：如果返回错误的物理地址

```
后果：
1. DMA 传输到错误位置
2. 数据损坏
3. 系统崩溃
```

---

## 九、互动自测

1. **问题**：为什么 `SYS_UMAP` 要限制只能映射自己的地址空间？
   **答案**：安全原因。如果允许映射任意进程的地址，就破坏了进程隔离。

2. **问题**：`MEM_GRANT` 是什么？
   **答案**：表示这是一个授权内存，需要通过授权验证才能访问。

3. **问题**：`do_umap` 和 `do_umap_remote` 的区别是什么？
   **答案**：`do_umap` 是受限版本，只能映射自己的地址；`do_umap_remote` 可以映射任意进程的地址，但需要授权验证。

---

## 十、参考文献

| 文件 | 描述 |
|------|------|
| `kernel/system/do_umap_remote.c` | 完整的地址映射实现 |
| `kernel/vm.h` | vm_lookup 函数声明 |
| `kernel/system/do_safecopy.c` | verify_grant 函数实现 |
| `include/minix/com.h` | 段类型常量定义 |

---

*讲解日期：2026-03-30*
*讲解者：Minix-rs 学习助手*
