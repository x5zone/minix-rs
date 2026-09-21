# do_copy.c 详细讲解

**文件位置**: `minix3/minix/kernel/system/do_copy.c`

**总行数**: 91 行

**作用**: 实现 `SYS_VIRCOPY` 和 `SYS_PHYSCOPY` 系统调用，提供基础的内存拷贝功能

---

## 一、文件概述

### 1.1 是什么（What）

`do_copy.c` 实现了 MINIX3 的**基础内存拷贝系统调用**：

| 系统调用 | 功能 |
|---------|------|
| `SYS_VIRCOPY` | 虚拟地址空间拷贝 |
| `SYS_PHYSCOPY` | 物理地址空间拷贝 |

### 1.2 为什么需要（Why）

**与 safecopy 的区别**：

| 特性 | do_copy | do_safecopy |
|------|---------|-------------|
| 授权机制 | 无 | 需要授权 ID |
| 安全性 | 依赖调用者特权 | 细粒度权限控制 |
| 使用者 | 特权进程（PM、VFS、RS） | 任何进程 |
| 灵活性 | 直接指定地址 | 通过授权间接访问 |

**设计原因**：特权进程需要直接内存访问能力，不需要授权机制的额外开销。

### 1.3 使用场景（When）

| 场景 | 系统调用 | 说明 |
|------|---------|------|
| fork() 复制进程结构 | `SYS_VIRCOPY` | PM 复制父进程的进程控制块 |
| exec() 加载程序 | `SYS_VIRCOPY` | 加载新程序到进程地址空间 |
| 内核调试 | `SYS_PHYSCOPY` | 直接访问物理内存 |

---

## 二、逐行讲解

### 2.1 文件头注释（第 1-14 行）

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_VIRCOPY, SYS_PHYSCOPY
 *
 * The parameters for this kernel call are:
 *   m_lsys_krn_sys_copy.src_addr		source offset within segment
 *   m_lsys_krn_sys_copy.src_endpt		source process number
 *   m_lsys_krn_sys_copy.dst_addr		destination offset within segment
 *   m_lsys_krn_sys_copy.dst_endpt		destination process number
 *   m_lsys_krn_sys_copy.nr_bytes		number of bytes to copy
 *   m_lsys_krn_sys_copy.flags
 */
```

**逐字段解析**：

| 字段 | 类型 | 含义 |
|------|------|------|
| `src_addr` | `vir_bytes` | 源地址偏移 |
| `src_endpt` | `endpoint_t` | 源进程端点 |
| `dst_addr` | `vir_bytes` | 目标地址偏移 |
| `dst_endpt` | `endpoint_t` | 目标进程端点 |
| `nr_bytes` | `phys_bytes` | 拷贝字节数 |
| `flags` | `int` | 标志位（如 CP_FLAG_TRY） |

### 2.2 头文件包含（第 16-19 行）

```c
#include "kernel/system.h"
#include "kernel/vm.h"
#include <assert.h>
```

| 头文件 | 作用 |
|--------|------|
| `"kernel/system.h"` | 系统调用框架，`struct proc` 定义 |
| `"kernel/vm.h"` | 虚拟内存接口，`virtual_copy()` 函数 |
| `<assert.h>` | 断言宏 |

### 2.3 条件编译（第 21 行）

```c
#if (USE_VIRCOPY || USE_PHYSCOPY)
```

**设计原因**：允许在配置中禁用这些系统调用，减少内核攻击面。

### 2.4 do_copy 函数签名（第 26-29 行）

```c
/*===========================================================================*
 *				do_copy					     *
 *===========================================================================*/
int do_copy(struct proc * caller, message * m_ptr)
```

**参数**：
- `caller` - 调用者进程指针
- `m_ptr` - 消息指针，包含拷贝参数

**返回值**：
- `OK` - 拷贝成功
- `EINVAL` - 无效参数
- `E2BIG` - 拷贝大小溢出
- `EFAULT` - 内存访问错误

### 2.5 函数注释（第 30-34 行）

```c
/* Handle sys_vircopy() and sys_physcopy().  Copy data using virtual or
 * physical addressing. Although a single handler function is used, there 
 * are two different kernel calls so that permissions can be checked. 
 */
```

**关键信息**：
- `SYS_VIRCOPY` 和 `SYS_PHYSCOPY` 共用同一个处理函数
- 分成两个系统调用是为了权限检查

### 2.6 局部变量（第 35-38 行）

```c
  struct vir_addr vir_addr[2];	/* virtual source and destination address */
  phys_bytes bytes;		/* number of bytes to copy */
  int i;
```

| 变量 | 类型 | 大小 | 用途 |
|------|------|------|------|
| `vir_addr[2]` | `struct vir_addr[2]` | 16 字节 | 源和目标虚拟地址 |
| `bytes` | `phys_bytes` | 4/8 字节 | 拷贝字节数 |
| `i` | `int` | 4 字节 | 循环计数器 |

**vir_addr 结构**：

```c
struct vir_addr {
    endpoint_t proc_nr_e;  // 进程端点（4 字节）
    vir_bytes offset;      // 虚拟地址偏移（4/8 字节）
};
```

### 2.7 调试代码（第 40-55 行）

```c
#if 0
  if (caller->p_endpoint != PM_PROC_NR && caller->p_endpoint != VFS_PROC_NR &&
	caller->p_endpoint != RS_PROC_NR && caller->p_endpoint != MEM_PROC_NR &&
	caller->p_endpoint != VM_PROC_NR)
  {
	static int first=1;
	if (first)
	{
		first= 0;
		printf(
"do_copy: got request from %d (source %d, destination %d)\n",
			caller->p_endpoint,
			m_ptr->m_lsys_krn_sys_copy.src_endpt,
			m_ptr->m_lsys_krn_sys_copy.dst_endpt);
	}
  }
#endif
```

**设计原因**：调试代码被 `#if 0` 禁用。原本用于检测非预期调用者。

**允许的调用者**：
- `PM_PROC_NR` - 进程管理器
- `VFS_PROC_NR` - 虚拟文件系统
- `RS_PROC_NR` - 重生服务器
- `MEM_PROC_NR` - 内存服务
- `VM_PROC_NR` - 虚拟内存管理器

### 2.8 解析消息参数（第 57-63 行）

```c
  /* Dismember the command message. */
  vir_addr[_SRC_].proc_nr_e = m_ptr->m_lsys_krn_sys_copy.src_endpt;
  vir_addr[_DST_].proc_nr_e = m_ptr->m_lsys_krn_sys_copy.dst_endpt;

  vir_addr[_SRC_].offset = m_ptr->m_lsys_krn_sys_copy.src_addr;
  vir_addr[_DST_].offset = m_ptr->m_lsys_krn_sys_copy.dst_addr;
  bytes = m_ptr->m_lsys_krn_sys_copy.nr_bytes;
```

**内存布局**：

```
消息结构体:
┌─────────────────────────────────────────────────────────────────┐
│  m_lsys_krn_sys_copy.src_endpt  ──► vir_addr[_SRC_].proc_nr_e   │
│  m_lsys_krn_sys_copy.src_addr   ──► vir_addr[_SRC_].offset      │
│  m_lsys_krn_sys_copy.dst_endpt  ──► vir_addr[_DST_].proc_nr_e   │
│  m_lsys_krn_sys_copy.dst_addr   ──► vir_addr[_DST_].offset      │
│  m_lsys_krn_sys_copy.nr_bytes   ──► bytes                       │
└─────────────────────────────────────────────────────────────────┘
```

### 2.9 端点验证循环（第 65-80 行）

```c
  /* Now do some checks for both the source and destination virtual address.
   * This is done once for _SRC_, then once for _DST_. 
   */
  for (i=_SRC_; i<=_DST_; i++) {
	int p;
      /* Check if process number was given implicitly with SELF and is valid. */
      if (vir_addr[i].proc_nr_e == SELF)
	vir_addr[i].proc_nr_e = caller->p_endpoint;
      if (vir_addr[i].proc_nr_e != NONE) {
	if(! isokendpt(vir_addr[i].proc_nr_e, &p)) {
	  printf("do_copy: %d: %d not ok endpoint\n", i, vir_addr[i].proc_nr_e);
          return(EINVAL); 
        }
      }
  }
```

**逐行解析**：

| 步骤 | 代码 | 功能 |
|------|------|------|
| 1 | `for (i=_SRC_; i<=_DST_; i++)` | 遍历源和目标 |
| 2 | `if (vir_addr[i].proc_nr_e == SELF)` | 检查是否使用 SELF 宏 |
| 3 | `vir_addr[i].proc_nr_e = caller->p_endpoint` | 将 SELF 替换为调用者端点 |
| 4 | `if (vir_addr[i].proc_nr_e != NONE)` | 检查端点是否有效 |
| 5 | `isokendpt()` | 验证端点有效性 |

**SELF 宏的作用**：

```c
#define SELF    (-1)     /* 约定：-1 表示"我自己" */
```

**设计原因**：调用者不需要知道自己的端点号，使用 `SELF` 更方便。

### 2.10 溢出检查（第 82-85 行）

```c
  /* Check for overflow. This would happen for 64K segments and 16-bit 
   * vir_bytes. Especially copying by the PM on do_fork() is affected. 
   */
  if (bytes != (phys_bytes) (vir_bytes) bytes) return(E2BIG);
```

**设计原因**：
- `vir_bytes` 可能是 16 位（嵌入式系统）
- `phys_bytes` 是 32 位或 64 位
- 如果拷贝大小超过 65535 字节，转换会丢失高位

**检查逻辑**：
```
bytes (32 位) → (vir_bytes) 截断 → (phys_bytes) 扩展 → 与原值比较
```

### 2.11 执行拷贝（第 87-95 行）

```c
  /* Now try to make the actual virtual copy. */
  if(m_ptr->m_lsys_krn_sys_copy.flags & CP_FLAG_TRY) {
	int r;
	assert(caller->p_endpoint == VFS_PROC_NR);
	r = virtual_copy(&vir_addr[_SRC_], &vir_addr[_DST_], bytes);
	if(r == EFAULT_SRC || r == EFAULT_DST) return r = EFAULT;
	return r;
  } else {
	return( virtual_copy_vmcheck(caller, &vir_addr[_SRC_],
			  	&vir_addr[_DST_], bytes) );
  }
```

**两种拷贝模式**：

| 模式 | 标志 | 函数 | 特点 |
|------|------|------|------|
| 尝试拷贝 | `CP_FLAG_TRY` | `virtual_copy()` | 遇故障返回错误，不阻塞 |
| 普通拷贝 | 无 | `virtual_copy_vmcheck()` | 通过 VM 处理页面故障 |

**CP_FLAG_TRY 的使用**：
- 只有 VFS 可以使用
- 用于避免死锁（内存映射文件场景）

### 2.12 条件编译结束（第 96 行）

```c
#endif /* (USE_VIRCOPY || USE_PHYSCOPY) */
```

---

## 三、与 safecopy 的对比

### 3.1 架构对比

```
┌─────────────────────────────────────────────────────────────────────────┐
│  do_copy: 直接内存拷贝                                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  调用者 ──► 指定源地址和目标地址 ──► 内核直接拷贝                        │
│                                                                         │
│  安全性：依赖调用者特权（只有 PM/VFS/RS 等可以调用）                     │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│  do_safecopy: 授权内存拷贝                                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  调用者 ──► 指定授权 ID ──► 内核验证授权 ──► 拷贝                       │
│                                                                         │
│  安全性：细粒度权限控制（任何进程可以调用，但需要有效授权）              │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.2 使用场景对比

| 场景 | 推荐使用 | 原因 |
|------|---------|------|
| fork() 复制进程结构 | `do_copy` | PM 是特权进程，直接访问更高效 |
| 文件 I/O | `do_safecopy` | 需要跨进程访问用户缓冲区 |
| 内核调试 | `do_copy` | 需要直接访问物理内存 |
| 驱动程序 DMA | `do_safecopy` | 需要安全地访问用户内存 |

---

## 四、现代硬件适配建议

| 方面 | C 语言实现 | 现代硬件适配 |
|------|-----------|-------------|
| 地址翻译 | 软件查找 | 利用 EPT/NPT |
| 拷贝操作 | `virtual_copy()` | 使用 `copy_from_user()` 类似接口 |
| 溢出检查 | 手动检查 | 编译器内置检查 |
| 权限检查 | 隐式（调用者特权） | 显式（capability） |

---

## 五、Rust 重构建议

```rust
use core::result::Result;

#[derive(Debug, Clone, Copy)]
pub enum CopyError {
    InvalidEndpoint,
    InvalidAddress,
    Overflow,
    Fault,
    PermissionDenied,
}

pub struct CopyRequest {
    pub src_endpoint: Endpoint,
    pub src_addr: VirtAddr,
    pub dst_endpoint: Endpoint,
    pub dst_addr: VirtAddr,
    pub bytes: usize,
    pub flags: CopyFlags,
}

bitflags! {
    pub struct CopyFlags: u32 {
        const CP_FLAG_TRY = 0x01;
    }
}

pub fn do_copy(
    caller: &Proc,
    request: &CopyRequest,
) -> Result<(), CopyError> {
    let mut src = VirAddr {
        endpoint: request.src_endpoint,
        offset: request.src_addr,
    };
    let mut dst = VirAddr {
        endpoint: request.dst_endpoint,
        offset: request.dst_addr,
    };

    if src.endpoint == SELF {
        src.endpoint = caller.endpoint();
    }
    if dst.endpoint == SELF {
        dst.endpoint = caller.endpoint();
    }

    if !src.endpoint.is_valid() || !dst.endpoint.is_valid() {
        return Err(CopyError::InvalidEndpoint);
    }

    if request.flags.contains(CopyFlags::CP_FLAG_TRY) {
        ensure!(caller.endpoint() == VFS_PROC_NR, CopyError::PermissionDenied);
        match virtual_copy(&src, &dst, request.bytes) {
            Ok(()) => Ok(()),
            Err(VirtualCopyError::FaultSrc) | Err(VirtualCopyError::FaultDst) => {
                Err(CopyError::Fault)
            }
            Err(e) => Err(CopyError::InvalidAddress),
        }
    } else {
        virtual_copy_vmcheck(caller, &src, &dst, request.bytes)
            .map_err(|_| CopyError::Fault)
    }
}
```

---

## 六、要点总结

### 核心知识点

1. **do_copy 是特权操作**：
   - 只有 PM、VFS、RS 等特权进程可以调用
   - 不需要授权机制，直接指定地址

2. **SELF 宏的便利性**：
   - 调用者不需要知道自己的端点号
   - 使用 `SELF` 表示"我自己"

3. **两种拷贝模式**：
   - 普通模式：通过 VM 处理页面故障
   - 尝试模式：遇故障返回错误（仅 VFS）

---

## 七、灾难预演

### 场景 1：如果删掉端点验证

```
后果：
1. 可以访问任意进程的内存
2. 安全漏洞
3. 系统不稳定
```

### 场景 2：如果删掉溢出检查

```
后果：
1. 16 位系统上大拷贝会截断
2. 数据损坏
3. 难以调试的 bug
```

### 场景 3：如果允许非特权进程调用

```
后果：
1. 任何进程可以读取其他进程内存
2. 完全破坏进程隔离
3. 安全灾难
```

---

## 八、互动自测

1. **问题**：为什么 `SYS_VIRCOPY` 和 `SYS_PHYSCOPY` 共用同一个处理函数？
   **答案**：它们的核心逻辑相同，只是地址解释方式不同。分开是为了权限检查。

2. **问题**：`SELF` 宏的作用是什么？
   **答案**：让调用者不需要知道自己的端点号，使用 `SELF` 表示"我自己"。

3. **问题**：`CP_FLAG_TRY` 标志的作用是什么？
   **答案**：尝试拷贝，遇到页面故障不阻塞而是返回错误。用于避免死锁。

---

## 九、参考文献

| 文件 | 描述 |
|------|------|
| `kernel/vm.h` | virtual_copy 函数声明 |
| `kernel/system.h` | 系统调用框架 |
| `include/minix/com.h` | SELF 宏定义 |
| `kernel/system/do_safecopy.c` | 安全拷贝实现 |

---

*讲解日期：2026-03-30*
*讲解者：Minix-rs 学习助手*
