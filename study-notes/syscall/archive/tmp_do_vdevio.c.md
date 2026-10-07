# do_vdevio.c 详细讲解

**文件位置**: `minix3/minix/kernel/system/do_vdevio.c`

**总行数**: 165 行

**作用**: 实现 `SYS_VDEVIO` 系统调用，提供批量设备 I/O 端口访问功能

---

## 一、文件概述

### 1.1 是什么（What）

`do_vdevio.c` 实现了 MINIX3 的**向量设备 I/O 系统调用**：

| 系统调用 | 功能 |
|---------|------|
| `SYS_VDEVIO` | 批量对设备 I/O 端口进行读写操作 |

**核心功能**：
- 一次性执行多个 I/O 端口操作
- 支持字节/字/双字操作
- 减少系统调用开销

### 1.2 为什么需要（Why）

**设计原因**：

`do_devio` 每次只能执行一个 I/O 操作，当驱动程序需要：
- 初始化设备（写入多个寄存器）
- 批量读取状态
- 高速数据传输

每次系统调用都有开销，批量操作可以显著提高效率：

```
┌─────────────────────────────────────────────────────────────────────────┐
│  使用 do_devio 执行 10 次 I/O                                            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  用户态 ──► 内核态 ──► 用户态 ──► 内核态 ──► ... (10 次)                │
│                                                                         │
│  系统调用开销: 10 × 系统调用成本                                        │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│  使用 do_vdevio 执行 10 次 I/O                                           │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  用户态 ──► 内核态 (执行 10 次 I/O) ──► 用户态                          │
│                                                                         │
│  系统调用开销: 1 × 系统调用成本                                         │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 1.3 使用场景（When）

| 场景 | I/O 类型 | 说明 |
|------|---------|------|
| 设备初始化 | 批量输出 | 写入多个配置寄存器 |
| 状态轮询 | 批量输入 | 读取多个状态寄存器 |
| 数据传输 | 批量输入/输出 | 高速数据块传输 |
| VGA 编程 | 批量输出 | 设置多个 VGA 寄存器 |

---

## 二、逐行讲解

### 2.1 文件头注释（第 1-10 行）

```c
/* The kernel call implemented in this file:
 *   m_type:	SYS_VDEVIO
 *
 * The parameters for this kernel call are:
 *    m_lsys_krn_sys_vdevio.request	(request input or output)
 *    m_lsys_krn_sys_vdevio.vec_addr	(pointer to port/ value pairs)
 *    m_lsys_krn_sys_vdevio.vec_size	(number of ports to read or write)
 */
```

**翻译**：
```
本文件实现的内核调用：
  m_type: SYS_VDEVIO

此内核调用的参数：
  m_lsys_krn_sys_vdevio.request   - 请求输入或输出
  m_lsys_krn_sys_vdevio.vec_addr  - 指向端口/值对的指针
  m_lsys_krn_sys_vdevio.vec_size  - 要读写的端口数量
```

**参数详解**：

| 字段 | 方向 | 类型 | 含义 |
|------|------|------|------|
| `request` | 输入 | `int` | I/O 请求类型和方向 |
| `vec_addr` | 输入 | `vir_bytes` | 用户空间向量地址 |
| `vec_size` | 输入 | `int` | 向量大小（操作数量） |

### 2.2 头文件包含（第 12-16 行）

```c
#include "kernel/system.h"
#include <minix/devio.h>
#include <minix/endpoint.h>

#if USE_VDEVIO
```

| 头文件 | 作用 |
|--------|------|
| `"kernel/system.h"` | 系统调用框架、`struct proc` 定义 |
| `<minix/devio.h>` | I/O 请求类型定义、端口值对结构 |
| `<minix/endpoint.h>` | 端点类型定义 |

**条件编译**：`USE_VDEVIO` 控制是否编译此功能。

### 2.3 静态缓冲区定义（第 18-22 行）

```c
/* Buffer for SYS_VDEVIO to copy (port,value)-pairs from/ to user. */
static char vdevio_buf[VDEVIO_BUF_SIZE];      
static pvb_pair_t * const pvb = (pvb_pair_t *) vdevio_buf;           
static pvw_pair_t * const pvw = (pvw_pair_t *) vdevio_buf;      
static pvl_pair_t * const pvl = (pvl_pair_t *) vdevio_buf;     
```

**翻译注释**：`Buffer for SYS_VDEVIO to copy (port,value)-pairs from/ to user.` = "SYS_VDEVIO 用于从/向用户复制（端口，值）对的缓冲区。"

**设计原因**：
- 使用静态缓冲区避免动态内存分配
- 三种不同类型的指针指向同一缓冲区
- 根据操作类型使用不同的指针

**端口值对结构**：

```c
// 字节端口值对
typedef struct {
    port_t port;    // I/O 端口号
    u8_t value;     // 值
} pvb_pair_t;

// 字端口值对
typedef struct {
    port_t port;    // I/O 端口号
    u16_t value;    // 值
} pvw_pair_t;

// 双字端口值对
typedef struct {
    port_t port;    // I/O 端口号
    u32_t value;    // 值
} pvl_pair_t;
```

### 2.4 do_vdevio 函数签名（第 24-28 行）

```c
/*===========================================================================*
 *			        do_vdevio                                    *
 *===========================================================================*/
int do_vdevio(struct proc * caller, message * m_ptr)
```

**参数**：
- `caller` - 调用者进程指针（驱动程序进程）
- `m_ptr` - 消息指针，包含 I/O 请求参数

**返回值**：
- `OK` - 操作成功
- `EINVAL` - 无效参数
- `EPERM` - 权限不足
- `E2BIG` - 向量太大
- `EFAULT` - 数据复制失败

### 2.5 函数注释（第 29-35 行）

```c
/* Perform a series of device I/O on behalf of a non-kernel process. The 
 * I/O addresses and I/O values are fetched from and returned to some buffer
 * in user space. The actual I/O is wrapped by lock() and unlock() to prevent
 * that I/O batch from being interrupted.
 * This is the counterpart of do_devio, which performs a single device I/O. 
 */ 
```

**翻译**：
```
代表非内核进程执行一系列设备 I/O。
I/O 地址和 I/O 值从用户空间的某个缓冲区获取并返回。
实际的 I/O 被 lock() 和 unlock() 包装，以防止 I/O 批次被中断。
这是 do_devio 的对应版本，do_devio 执行单个设备 I/O。
```

### 2.6 局部变量声明（第 36-46 行）

```c
  int vec_size;               /* size of vector */
  int io_in;                  /* true if input */
  size_t bytes;               /* # bytes to be copied */
  port_t port;
  int i, j, io_size, nr_io_range;
  int io_dir, io_type;
  struct priv *privp;
  struct io_range *iorp;
  int r;
```

**翻译注释**：
- `size of vector` = "向量大小"
- `true if input` = "如果是输入则为真"
- `# bytes to be copied` = "要复制的字节数"

| 变量 | 类型 | 大小 | 用途 |
|------|------|------|------|
| `vec_size` | `int` | 4 字节 | 向量大小（操作数量） |
| `io_in` | `int` | 4 字节 | 是否为输入操作 |
| `bytes` | `size_t` | 8 字节 | 要复制的字节数 |
| `port` | `port_t` | 4 字节 | I/O 端口号 |
| `i, j` | `int` | 4 字节 | 循环计数器 |
| `io_size` | `int` | 4 字节 | 单个 I/O 数据大小 |
| `nr_io_range` | `int` | 4 字节 | 允许的 I/O 范围数量 |
| `io_dir` | `int` | 4 字节 | I/O 方向 |
| `io_type` | `int` | 4 字节 | I/O 数据类型 |
| `privp` | `struct priv *` | 8 字节 | 指向特权结构的指针 |
| `iorp` | `struct io_range *` | 8 字节 | 指向 I/O 范围的指针 |
| `r` | `int` | 4 字节 | 返回值 |

### 2.7 解析请求参数（第 48-56 行）

```c
  /* Get the request, size of the request vector, and check the values. */
  io_dir = m_ptr->m_lsys_krn_sys_vdevio.request & _DIO_DIRMASK;
  io_type = m_ptr->m_lsys_krn_sys_vdevio.request & _DIO_TYPEMASK;
  if (io_dir == _DIO_INPUT) io_in = TRUE;
  else if (io_dir == _DIO_OUTPUT) io_in = FALSE;
  else return(EINVAL);
```

**翻译注释**：`Get the request, size of the request vector, and check the values.` = "获取请求、请求向量的大小，并检查值。"

### 2.8 验证向量大小（第 57-58 行）

```c
  if ((vec_size = m_ptr->m_lsys_krn_sys_vdevio.vec_size) <= 0) return(EINVAL);
```

**设计原因**：向量大小必须为正数。

### 2.9 计算缓冲区大小（第 59-73 行）

```c
  switch (io_type) {
      case _DIO_BYTE:
	bytes = vec_size * sizeof(pvb_pair_t);
	io_size= sizeof(u8_t);
	break;
      case _DIO_WORD:
	bytes = vec_size * sizeof(pvw_pair_t);
	io_size= sizeof(u16_t);
	break;
      case _DIO_LONG:
	bytes = vec_size * sizeof(pvl_pair_t);
	io_size= sizeof(u32_t);
	break;
      default:  return(EINVAL);   /* check type once and for all */
  }
```

**翻译注释**：`check type once and for all` = "一次性检查类型"

**计算示例**：
```
vec_size = 10, io_type = _DIO_BYTE:
  bytes = 10 * sizeof(pvb_pair_t) = 10 * 8 = 80 字节
  io_size = 1

vec_size = 10, io_type = _DIO_WORD:
  bytes = 10 * sizeof(pvw_pair_t) = 10 * 8 = 80 字节
  io_size = 2

vec_size = 10, io_type = _DIO_LONG:
  bytes = 10 * sizeof(pvl_pair_t) = 10 * 12 = 120 字节
  io_size = 4
```

### 2.10 检查缓冲区大小限制（第 74 行）

```c
  if (bytes > sizeof(vdevio_buf))  return(E2BIG);
```

**设计原因**：
- 静态缓冲区大小有限
- 防止缓冲区溢出
- `E2BIG` 表示参数太大

### 2.11 从用户空间复制数据（第 76-79 行）

```c
  /* Copy (port,value)-pairs from user. */
  if((r=data_copy(caller->p_endpoint, m_ptr->m_lsys_krn_sys_vdevio.vec_addr,
    KERNEL, (vir_bytes) vdevio_buf, bytes)) != OK)
	return r;
```

**翻译注释**：`Copy (port,value)-pairs from user.` = "从用户复制（端口，值）对。"

**data_copy 参数**：
| 参数 | 值 | 说明 |
|------|-----|------|
| 源端点 | `caller->p_endpoint` | 调用者进程 |
| 源地址 | `vec_addr` | 用户空间向量地址 |
| 目标端点 | `KERNEL` | 内核 |
| 目标地址 | `vdevio_buf` | 内核缓冲区 |
| 大小 | `bytes` | 复制字节数 |

### 2.12 I/O 端口权限检查（第 81-104 行）

```c
  privp= priv(caller);
  if (privp && (privp->s_flags & CHECK_IO_PORT))
  {
	/* Check whether the I/O is allowed */
	nr_io_range= privp->s_nr_io_range;
	for (i=0; i<vec_size; i++)
	{
		switch (io_type) {
		case _DIO_BYTE: port= pvb[i].port; break;
		case _DIO_WORD: port= pvw[i].port; break;
		default:	port= pvl[i].port; break;
		}
		for (j= 0, iorp= privp->s_io_tab; j<nr_io_range; j++, iorp++)
		{
			if (port >= iorp->ior_base &&
				port+io_size-1 <= iorp->ior_limit)
			{
				break;
			}
		}
		if (j >= nr_io_range)
		{
			printf(
		"do_vdevio: I/O port check failed for proc %d, port 0x%x\n",
				caller->p_endpoint, port);
			return EPERM;
		}
	}
  }
```

**翻译注释**：`Check whether the I/O is allowed` = "检查 I/O 是否被允许"

**权限检查流程**：
```
┌─────────────────────────────────────────────────────────────────────────┐
│  do_vdevio 权限检查流程                                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  for (i = 0; i < vec_size; i++)                                        │
│  {                                                                      │
│      1. 获取第 i 个端口                                                 │
│         └── 根据类型从 pvb/pvw/pvl 获取                                │
│                                                                         │
│      2. 检查端口是否在允许范围内                                        │
│         └── for (j = 0; j < nr_io_range; j++)                          │
│                                                                         │
│      3. 如果不在任何范围内，拒绝访问                                    │
│         └── return EPERM                                               │
│  }                                                                      │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.13 执行字节 I/O（第 106-114 行）

```c
  /* Perform actual device I/O for byte, word, and long values */
  switch (io_type) {
  case _DIO_BYTE: 					 /* byte values */
      if (io_in) for (i=0; i<vec_size; i++) 
		pvb[i].value = inb( pvb[i].port); 
      else      for (i=0; i<vec_size; i++)
		outb( pvb[i].port, pvb[i].value); 
      break; 
```

**翻译注释**：
- `Perform actual device I/O for byte, word, and long values` = "执行字节、字和双字值的实际设备 I/O"
- `byte values` = "字节值"

### 2.14 执行字 I/O（第 115-131 行）

```c
  case _DIO_WORD:					  /* word values */
      if (io_in)
      {
	for (i=0; i<vec_size; i++)  
	{
		port= pvw[i].port;
		if (port & 1) goto bad;
		pvw[i].value = inw( pvw[i].port);  
	}
      }
      else
      {
	for (i=0; i<vec_size; i++) 
	{
		port= pvw[i].port;
		if (port & 1) goto bad;
		outw( pvw[i].port, pvw[i].value); 
	}
      }
      break; 
```

**翻译注释**：`word values` = "字值"

**对齐检查**：
- `port & 1` 检查端口是否为奇数
- 字访问需要偶数端口
- 如果对齐失败，跳转到 `bad` 标签

### 2.15 执行双字 I/O（第 132-147 行）

```c
  default:            					  /* long values */
      if (io_in)
      {
	for (i=0; i<vec_size; i++)
	{
		port= pvl[i].port;
		if (port & 3) goto bad;
		pvl[i].value = inl(pvl[i].port);  
	}
      }
      else
      {
	for (i=0; i<vec_size; i++)
	{
		port= pvl[i].port;
		if (port & 3) goto bad;
		outl( pvb[i].port, pvl[i].value); 
	}
      }
  }
```

**翻译注释**：`long values` = "双字值"

**对齐检查**：
- `port & 3` 检查端口是否为 4 的倍数
- 双字访问需要 4 的倍数端口

**注意**：第 145 行有一个 bug：`outl( pvb[i].port, pvl[i].value)` 应该是 `outl( pvl[i].port, pvl[i].value)`。

### 2.16 复制结果回用户空间（第 149-155 行）

```c
  /* Almost done, copy back results for input requests. */
  if (io_in) 
	if((r=data_copy(KERNEL, (vir_bytes) vdevio_buf,
	  caller->p_endpoint, m_ptr->m_lsys_krn_sys_vdevio.vec_addr,
	  (phys_bytes) bytes)) != OK)
		return r;
  return(OK);
```

**翻译注释**：`Almost done, copy back results for input requests.` = "快完成了，将输入请求的结果复制回去。"

**设计原因**：
- 只有输入操作需要复制结果
- 输出操作不需要复制（值已经写入设备）

### 2.17 错误处理（第 157-160 行）

```c
bad:
	panic("do_vdevio: unaligned port: %d", port);
	return EPERM;
}
```

**设计原因**：
- 对齐错误是严重问题
- 使用 `panic` 打印错误信息
- 实际上 `panic` 可能不会返回

### 2.18 条件编译结束（第 162 行）

```c
#endif /* USE_VDEVIO */
```

---

## 三、向量 I/O 流程

### 3.1 完整流程

```
┌─────────────────────────────────────────────────────────────────────────┐
│  do_vdevio 完整执行流程                                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  1. 解析请求参数                                                        │
│     ├── io_dir = request & _DIO_DIRMASK                                │
│     └── io_type = request & _DIO_TYPEMASK                              │
│                                                                         │
│  2. 验证向量大小                                                        │
│     └── vec_size > 0                                                   │
│                                                                         │
│  3. 计算缓冲区大小                                                      │
│     └── bytes = vec_size * sizeof(pair)                                │
│                                                                         │
│  4. 检查缓冲区限制                                                      │
│     └── bytes <= VDEVIO_BUF_SIZE                                       │
│                                                                         │
│  5. 从用户空间复制数据                                                  │
│     └── data_copy(user → kernel)                                       │
│                                                                         │
│  6. 权限检查                                                            │
│     └── 检查所有端口是否在允许范围内                                    │
│                                                                         │
│  7. 执行 I/O 操作                                                       │
│     ├── 输入: inb/inw/inl 循环                                         │
│     └── 输出: outb/outw/outl 循环                                      │
│                                                                         │
│  8. 复制结果回用户空间（仅输入）                                        │
│     └── data_copy(kernel → user)                                       │
│                                                                         │
│  9. 返回 OK                                                             │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### 3.2 内存布局

```
用户空间:
┌─────────────────────────────────────────────────────────────────────────┐
│  pvb_pair_t vec[10] = {                                                 │
│    {0x3F8, 0},  // 端口 0x3F8, 值待填充                                 │
│    {0x3F9, 0},  // 端口 0x3F9, 值待填充                                 │
│    ...                                                                  │
│  };                                                                     │
└─────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼ data_copy (user → kernel)
内核空间:
┌─────────────────────────────────────────────────────────────────────────┐
│  vdevio_buf (静态缓冲区):                                               │
│  pvb[0] = {0x3F8, 0}                                                    │
│  pvb[1] = {0x3F9, 0}                                                    │
│  ...                                                                    │
└─────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼ 执行 I/O
┌─────────────────────────────────────────────────────────────────────────┐
│  for (i = 0; i < 10; i++)                                              │
│      pvb[i].value = inb(pvb[i].port);                                  │
└─────────────────────────────────────────────────────────────────────────┘
                    │
                    ▼ data_copy (kernel → user, 仅输入操作)
用户空间:
┌─────────────────────────────────────────────────────────────────────────┐
│  pvb_pair_t vec[10] = {                                                 │
│    {0x3F8, 0x5A},  // 已填充读取的值                                    │
│    {0x3F9, 0x3C},  // 已填充读取的值                                    │
│    ...                                                                  │
│  };                                                                     │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 四、与 do_devio 的对比

### 4.1 性能对比

| 操作 | do_devio | do_vdevio |
|------|----------|-----------|
| 10 次端口读取 | 10 次系统调用 | 1 次系统调用 |
| 数据复制 | 每次复制消息 | 一次复制向量 |
| 权限检查 | 每次检查 | 批量检查 |
| 适用场景 | 偶发 I/O | 批量 I/O |

### 4.2 使用示例对比

```c
// 使用 do_devio 读取 10 个端口
for (i = 0; i < 10; i++) {
    sys_devio(_DIO_INPUT | _DIO_BYTE, ports[i], &values[i]);
}

// 使用 do_vdevio 读取 10 个端口
pvb_pair_t vec[10];
for (i = 0; i < 10; i++) {
    vec[i].port = ports[i];
}
sys_vdevio(_DIO_INPUT | _DIO_BYTE, vec, 10);
// 结果在 vec[i].value 中
```

---

## 五、现代硬件适配建议

| 方面 | C 语言实现 | 现代硬件适配 |
|------|-----------|-------------|
| 批量 I/O | 软件循环 | 使用 SIMD 批量操作 |
| 权限控制 | 软件检查 | IOMMU 硬件隔离 |
| 缓冲区 | 静态数组 | 动态分配或 per-CPU |
| 电源管理 | 无 | PCI 电源管理 |

---

## 六、Rust 重构建议

```rust
use core::result::Result;

#[derive(Debug, Clone, Copy)]
pub enum VdevioError {
    InvalidRequest,
    InvalidVectorSize,
    BufferTooLarge,
    PermissionDenied,
    UnalignedPort,
    CopyFailed,
}

pub struct PortValuePair {
    pub port: u16,
    pub value: u32,
}

pub struct VdevioParams {
    pub request: DevioRequest,
    pub vec: *mut PortValuePair,
    pub vec_size: usize,
}

pub fn do_vdevio(
    caller: &Proc,
    params: &VdevioParams,
) -> Result<(), VdevioError> {
    let io_dir = params.request & DevioRequest::DIR_MASK;
    let io_type = params.request & DevioRequest::TYPE_MASK;
    
    if params.vec_size <= 0 {
        return Err(VdevioError::InvalidVectorSize);
    }
    
    let (pair_size, io_size) = match io_type {
        DevioRequest::BYTE => (core::mem::size_of::<PvbPair>(), 1),
        DevioRequest::WORD => (core::mem::size_of::<PvwPair>(), 2),
        DevioRequest::LONG => (core::mem::size_of::<PvlPair>(), 4),
        _ => return Err(VdevioError::InvalidRequest),
    };
    
    let bytes = params.vec_size * pair_size;
    if bytes > VDEVIO_BUF_SIZE {
        return Err(VdevioError::BufferTooLarge);
    }
    
    let mut buf = [0u8; VDEVIO_BUF_SIZE];
    data_copy(
        caller.endpoint(),
        params.vec as usize,
        Endpoint::KERNEL,
        buf.as_mut_ptr() as usize,
        bytes,
    ).map_err(|_| VdevioError::CopyFailed)?;
    
    check_io_permissions(caller, &buf, params.vec_size, io_type, io_size)?;
    
    match io_type {
        DevioRequest::BYTE => {
            let pvb = unsafe { 
                core::slice::from_raw_parts_mut(
                    buf.as_mut_ptr() as *mut PvbPair, 
                    params.vec_size
                )
            };
            if io_dir == DevioRequest::INPUT {
                for pair in pvb.iter_mut() {
                    pair.value = unsafe { inb(pair.port) };
                }
            } else {
                for pair in pvb.iter() {
                    unsafe { outb(pair.port, pair.value) };
                }
            }
        }
        // ... 类似处理 WORD 和 LONG
        _ => return Err(VdevioError::InvalidRequest),
    }
    
    if io_dir == DevioRequest::INPUT {
        data_copy(
            Endpoint::KERNEL,
            buf.as_ptr() as usize,
            caller.endpoint(),
            params.vec as usize,
            bytes,
        ).map_err(|_| VdevioError::CopyFailed)?;
    }
    
    Ok(())
}
```

---

## 七、要点总结

### 核心知识点

1. **批量 I/O 提高效率**：
   - 一次系统调用执行多个 I/O 操作
   - 减少用户态/内核态切换开销
   - 适合设备初始化和批量数据传输

2. **静态缓冲区设计**：
   - 使用预分配的静态缓冲区
   - 避免动态内存分配
   - 有大小限制（`VDEVIO_BUF_SIZE`）

3. **权限批量检查**：
   - 在执行 I/O 前检查所有端口
   - 任何一个端口无权限则拒绝整个操作

---

## 八、灾难预演

### 场景 1：如果删掉缓冲区大小检查

```
后果：
1. 用户可以传递任意大的向量
2. 静态缓冲区溢出
3. 内核数据损坏，系统崩溃
```

### 场景 2：如果删掉权限检查

```
后果：
1. 驱动程序可以访问任意端口
2. 可能干扰其他设备
3. 安全漏洞
```

### 场景 3：如果忘记复制结果回用户空间

```
后果：
1. 输入操作的结果丢失
2. 驱动程序收到垃圾数据
3. 设备行为异常
```

---

## 九、互动自测

1. **问题**：为什么使用静态缓冲区而不是动态分配？
   **答案**：内核代码尽量避免动态内存分配，静态缓冲区更可靠、更安全。

2. **问题**：`pvb`、`pvw`、`pvl` 三个指针指向同一块内存，这样做有什么好处？
   **答案**：节省内存，根据操作类型使用不同的指针类型访问同一缓冲区。

3. **问题**：为什么输出操作不需要复制结果回用户空间？
   **答案**：输出操作的值已经写入设备端口，不需要返回给用户。

---

## 十、参考文献

| 文件 | 描述 |
|------|------|
| `include/minix/devio.h` | 端口值对结构定义 |
| `kernel/system/do_devio.c` | 单个设备 I/O |
| `kernel/proc.h` | `struct priv` 定义 |
| `servers/drivers/tty/serial.c` | 使用示例 |

---

*讲解日期：2026-03-30*
*讲解者：Minix-rs 学习助手*
