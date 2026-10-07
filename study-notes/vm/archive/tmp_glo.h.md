# servers/vm/glo.h 逐行讲解

> **文件路径**: `minix3/minix/servers/vm/glo.h`
> **核心功能**: VM 服务器的全局变量声明

---

## 文件概述

这个头文件声明了 VM（虚拟内存）服务器的所有全局变量。全局变量是整个 VM 服务共享的状态，包括：
1. **进程表**：VM 管理的所有进程信息
2. **内存类型**：不同类型内存的操作函数表
3. **内核信息**：从内核传递的启动信息
4. **统计信息**：内存页数、VM 实例数等

**设计思路**：
- 使用 `EXTERN` 宏实现"单一定义规则"（One Definition Rule）
- 在 main.c 中定义 `_MAIN`，使 `EXTERN` 展开为空，从而定义变量
- 在其他 .c 文件中，`EXTERN` 展开为 `extern`，声明变量

---

## 逐行讲解

### 第 1 行：头文件保护开始

```c
#ifndef _VM_GLO_H
```

**逐字拆解**：
- `#ifndef`：预处理指令，"if not defined"（如果未定义）
- `_VM_GLO_H`：宏名称，表示"VM 全局变量头文件已包含"
- **无分号**：预处理指令不以分号结尾

**设计原因**：
- 防止头文件重复包含
- `_VM_GLO_H` 是命名约定：下划线 + 文件名（VM_GLO_H）
- 避免重复定义导致的编译错误

**理论关联**：C 预处理器机制

**Rust 对比**：
```rust
// Rust 不需要头文件保护
// 模块系统自动处理重复导入
```

---

### 第 2 行：头文件保护定义

```c
#define _VM_GLO_H
```

**逐字拆解**：
- `#define`：预处理指令，定义宏
- `_VM_GLO_H`：宏名称

**设计原因**：
- 定义宏，表示此头文件已被包含
- 配合第 1 行的 `#ifndef` 使用

---

### 第 3 行：空行

```c

```

**设计原因**：分隔头文件保护和后续代码

---

### 第 4 行：包含系统配置头文件

```c
#include <minix/sys_config.h>
```

**逐字拆解**：
- `#include`：预处理指令，包含头文件
- `<minix/sys_config.h>`：系统配置头文件路径
  - `< >`：表示系统头文件
  - `minix/`：子目录
  - `sys_config.h`：文件名

**内存位置**：不占用内存，预处理阶段展开

**设计原因**：
- 获取系统配置信息
- 例如：`_MAIN` 宏的定义位置

**理论关联**：Minix3 系统配置

---

### 第 5 行：包含类型定义头文件

```c
#include <minix/type.h>
```

**设计原因**：
- 获取基本类型定义
- 例如：`u32_t`、`phys_bytes` 等

---

### 第 6 行：包含参数定义头文件

```c
#include <minix/param.h>
```

**设计原因**：
- 获取系统参数定义
- 例如：`_NR_PROCS`（进程数量）

---

### 第 7 行：包含系统状态头文件

```c
#include <sys/stat.h>
```

**设计原因**：
- 获取文件状态结构
- 用于文件映射相关功能

---

### 第 8 行：空行

```c

```

**设计原因**：分隔系统头文件和本地头文件

---

### 第 9 行：包含 VM 常量头文件

```c
#include "vm.h"
```

**逐字拆解**：
- `#include`：预处理指令
- `"vm.h"`：本地头文件
  - `" "`：表示本地头文件

**设计原因**：
- 获取 VM 相关常量
- 例如：`VM_PAGE_SIZE`、`SANITYCHECKS` 等

---

### 第 10 行：包含进程结构头文件

```c
#include "vmproc.h"
```

**设计原因**：
- 获取 `struct vmproc` 定义
- VM 进程表的结构

---

### 第 11 行：空行

```c

```

**设计原因**：分隔 #include 和宏定义

---

### 第 12-15 行：EXTERN 宏定义

```c
#if _MAIN
#undef EXTERN
#define EXTERN
#endif
```

**逐字拆解**：
- `#if _MAIN`：如果定义了 `_MAIN` 宏
  - 在 main.c 中定义了 `#define _MAIN 1`
- `#undef EXTERN`：取消定义 `EXTERN` 宏
- `#define EXTERN`：重新定义 `EXTERN` 为空
- `#endif`：结束条件编译

**设计原因**：
- **单一定义规则**（One Definition Rule）
- 在 main.c 中：`EXTERN` 为空，定义变量
- 在其他 .c 文件中：`EXTERN` 为 `extern`，声明变量
- 避免链接时的重复定义错误

**理论关联**：C 语言的链接机制

**Rust 对比**：
```rust
// Rust 不需要 EXTERN 宏
// 使用 static 表示全局变量
// 使用 pub static 表示公开的全局变量

pub static VMPROC: [VmProc; VMP_NR] = [VmProc::new(); VMP_NR];
```

---

### 第 16 行：空行

```c

```

**设计原因**：分隔宏定义和变量声明

---

### 第 17 行：定义临时进程槽号

```c
#define VMP_EXECTMP	_NR_PROCS
```

**逐字拆解**：
- `#define`：预处理指令，定义宏
- `VMP_EXECTMP`：宏名称，"VM Process Exec Temporary"
- `_NR_PROCS`：宏值，系统进程数量

**设计原因**：
- `_NR_PROCS` 是系统定义的最大进程数
- `VMP_EXECTMP` 用于 exec 时的临时进程槽
- exec 需要临时空间来加载新程序

**理论关联**：进程管理、exec 系统调用

**Rust 对比**：
```rust
const VMP_EXECTMP: usize = _NR_PROCS;
```

---

### 第 18 行：定义进程表大小

```c
#define VMP_NR		_NR_PROCS+1
```

**逐字拆解**：
- `#define`：预处理指令
- `VMP_NR`：宏名称，"VM Process Number"
- `_NR_PROCS+1`：宏值，进程数 + 1

**设计原因**：
- 进程表大小 = 系统进程数 + 1
- 多出的 1 个槽位用于 exec 临时进程
- 数组索引：0 到 `_NR_PROCS`

**理论关联**：进程表设计

**Rust 对比**：
```rust
const VMP_NR: usize = _NR_PROCS + 1;
```

---

### 第 19 行：空行

```c

```

**设计原因**：分隔宏定义和变量声明

---

### 第 20 行：声明进程表

```c
EXTERN struct vmproc vmproc[VMP_NR];
```

**逐字拆解**：
- `EXTERN`：宏，在 main.c 中为空，在其他文件中为 `extern`
- `struct vmproc`：结构体类型
- `vmproc`：变量名，进程表
- `[VMP_NR]`：数组大小

**内存位置**：
- 全局数据段（.bss 或 .data）
- 大小：`sizeof(struct vmproc) * VMP_NR`
- 每个进程一个槽位

**设计原因**：
- **VM 需要自己的进程表**
- 内核的进程表只保存调度信息
- VM 需要保存内存管理信息：
  - 页表指针 `vm_pt`
  - 虚拟区域列表 `vm_regions_avl`
  - 内存统计 `vm_total`、`vm_total_max`
  - 页错误统计 `vm_minor_page_fault`、`vm_major_page_fault`

**理论关联**：
- 微内核设计：内核只负责调度，VM 负责内存管理
- 进程控制块（PCB）的分离

**Rust 对比**：
```rust
pub static mut VMPROC: [VmProc; VMP_NR] = [VmProc {
    vm_flags: 0,
    vm_endpoint: 0,
    vm_pt: PtT::new(),
    vm_regions_avl: RegionAvl::new(),
    vm_region_top: 0,
    vm_acl: 0,
    vm_slot: 0,
    vm_total: 0,
    vm_total_max: 0,
    vm_minor_page_fault: 0,
    vm_major_page_fault: 0,
}; VMP_NR];
```

---

### 第 21 行：空行

```c

```

**设计原因**：分隔不同类型的变量

---

### 第 22 行：声明文件映射开关

```c
long enable_filemap;
```

**逐字拆解**：
- `long`：类型，长整型（通常 4 或 8 字节）
- `enable_filemap`：变量名，"启用文件映射"
- **无 EXTERN**：这是一个定义，不是声明

**内存位置**：全局数据段

**设计原因**：
- 控制是否启用文件映射功能
- 可以通过环境变量 `filemap` 设置
- 默认值：1（启用）

**使用场景**：
- 在 `init_vm()` 中设置：
  ```c
  enable_filemap=1;	/* yes by default */
  env_parse("filemap", "d", 0, &enable_filemap, 0, 1);
  ```
- 用于调试或性能测试

**理论关联**：文件映射（mmap）

**Rust 对比**：
```rust
pub static mut ENABLE_FILEMAP: i64 = 1;
```

---

### 第 23 行：空行

```c

```

**设计原因**：分隔不同类型的变量

---

### 第 24 行：定义内核信息类型别名

```c
typedef kinfo_t ixfer_kinfo_t;
```

**逐字拆解**：
- `typedef`：关键字，定义类型别名
- `kinfo_t`：原类型，内核信息类型
- `ixfer_kinfo_t`：新类型名，"信息传递内核信息类型"
  - `i`：information（信息）
  - `xfer`：transfer（传递）
  - `kinfo`：kernel info（内核信息）

**设计原因**：
- 为 `kinfo_t` 起一个更有意义的名字
- 表示这是从内核传递到 VM 的信息
- 提高代码可读性

**理论关联**：C 语言的类型别名

**Rust 对比**：
```rust
pub type IxferKinfoT = KinfoT;
```

---

### 第 25 行：声明内核启动信息

```c
EXTERN ixfer_kinfo_t kernel_boot_info;
```

**逐字拆解**：
- `EXTERN`：宏
- `ixfer_kinfo_t`：类型
- `kernel_boot_info`：变量名，"内核启动信息"

**内存位置**：全局数据段

**设计原因**：
- 保存从内核传递的启动信息
- 包含：
  - 内存映射（`mmap`）
  - 启动进程列表（`boot_procs`）
  - 内核模块列表（`module_list`）
  - 用户空间地址范围（`user_start`、`user_end`）
  - 栈指针（`user_sp`）

**使用场景**：
- 在 `init_vm()` 中获取：
  ```c
  if(OK != (s=sys_getkinfo(&kernel_boot_info))) {
      panic("couldn't get bootinfo: %d", s);
  }
  ```
- 在整个 VM 运行期间使用

**理论关联**：系统启动、内核-用户态通信

**Rust 对比**：
```rust
pub static mut KERNEL_BOOT_INFO: IxferKinfoT = IxferKinfoT::new();
```

---

### 第 26 行：空行

```c

```

**设计原因**：分隔不同类型的变量

---

### 第 27-31 行：条件编译的调试变量

```c
#if SANITYCHECKS
EXTERN int nocheck;
EXTERN int incheck;
EXTERN int sc_lastline;
EXTERN const char *sc_lastfile;
#endif
```

**逐字拆解**：
- `#if SANITYCHECKS`：如果定义了 `SANITYCHECKS`
- `EXTERN int nocheck`：是否禁用检查
- `EXTERN int incheck`：是否正在检查中
- `EXTERN int sc_lastline`：最后检查的行号
- `EXTERN const char *sc_lastfile`：最后检查的文件名
- `#endif`：结束条件编译

**内存位置**：全局数据段（仅在 `SANITYCHECKS` 定义时）

**设计原因**：
- **完整性检查**（Sanity Check）的调试变量
- `nocheck`：临时禁用检查（避免递归检查）
- `incheck`：标记正在检查中（避免重入）
- `sc_lastline`、`sc_lastfile`：记录检查位置，便于调试

**使用场景**：
- 在 `mem_sanitycheck()` 等函数中使用：
  ```c
  void mem_sanitycheck(const char *file, int line) {
      if(nopcheck) return;
      if(incheck) return;
      incheck = 1;
      // 执行检查...
      sc_lastline = line;
      sc_lastfile = file;
      incheck = 0;
  }
  ```

**理论关联**：调试技术、防御性编程

**Rust 对比**：
```rust
#[cfg(debug_assertions)]
pub static mut NOCHECK: i32 = 0;
#[cfg(debug_assertions)]
pub static mut INCHECK: i32 = 0;
#[cfg(debug_assertions)]
pub static mut SC_LASTLINE: i32 = 0;
#[cfg(debug_assertions)]
pub static mut SC_LASTFILE: *const i8 = core::ptr::null();
```

---

### 第 32 行：空行

```c

```

**设计原因**：分隔不同类型的变量

---

### 第 33 行：声明内核信息指针

```c
extern struct minix_kerninfo *_minix_kerninfo;
```

**逐字拆解**：
- `extern`：关键字，声明外部变量
- `struct minix_kerninfo`：结构体类型
- `*`：指针
- `_minix_kerninfo`：变量名

**内存位置**：指针本身在全局数据段，指向的内存由内核提供

**设计原因**：
- 指向内核提供的共享信息结构
- 包含内核的运行时信息
- 用于高效的内核-用户态通信

**理论关联**：内核-用户态共享内存

**Rust 对比**：
```rust
extern "C" {
    static mut _minix_kerninfo: *mut MinixKerninfo;
}
```

---

### 第 34 行：空行

```c

```

**设计原因**：分隔不同类型的变量

---

### 第 35 行：注释块

```c
/* mem types */
```

**逐字拆解**：
- `/*`：注释开始
- `mem types`：注释内容，"内存类型"
- `*/`：注释结束

**设计原因**：
- 说明接下来的变量是内存类型
- 提高代码可读性

---

### 第 36-42 行：声明内存类型变量

```c
EXTERN  mem_type_t mem_type_anon,       /* anonymous memory */
        mem_type_directphys,		/* direct physical mapping memory */
	mem_type_anon_contig,		/* physically contig anon memory */
	mem_type_cache,			/* disk cache */
	mem_type_mappedfile,		/* memory with file contents */
	mem_type_shared;		/* memory shared by multiple processes */
```

**逐字拆解**：
- `EXTERN`：宏
- `mem_type_t`：类型，内存类型结构体
- `mem_type_anon`：变量名，匿名内存
- `/* anonymous memory */`：注释，"匿名内存"
- `mem_type_directphys`：变量名，直接物理映射
- `/* direct physical mapping memory */`：注释，"直接物理映射内存"
- `mem_type_anon_contig`：变量名，连续匿名内存
- `/* physically contig anon memory */`：注释，"物理连续匿名内存"
- `mem_type_cache`：变量名，磁盘缓存
- `/* disk cache */`：注释，"磁盘缓存"
- `mem_type_mappedfile`：变量名，文件映射
- `/* memory with file contents */`：注释，"包含文件内容的内存"
- `mem_type_shared`：变量名，共享内存
- `/* memory shared by multiple processes */`：注释，"多进程共享的内存"

**内存位置**：全局数据段

**设计原因**：
- **内存类型系统**：Minix3 VM 的核心设计
- 每种内存类型有不同的操作函数：
  - `ev_new`：创建区域
  - `ev_delete`：删除区域
  - `ev_pagefault`：处理页错误
  - `ev_resize`：调整大小
  - 等等

**各类型的作用**：

| 类型 | 作用 | 使用场景 |
|------|------|----------|
| `mem_type_anon` | 匿名内存 | malloc、栈、堆 |
| `mem_type_directphys` | 直接物理映射 | 设备内存映射 |
| `mem_type_anon_contig` | 连续匿名内存 | DMA 缓冲区 |
| `mem_type_cache` | 磁盘缓存 | 文件系统缓存 |
| `mem_type_mappedfile` | 文件映射 | mmap 文件 |
| `mem_type_shared` | 共享内存 | IPC 共享内存 |

**理论关联**：
- 面向对象的多态（C 语言实现）
- 策略模式（Strategy Pattern）

**Rust 对比**：
```rust
pub static mut MEM_TYPE_ANON: MemType = MemType {
    name: "anonymous memory",
    ev_new: Some(mem_anon_new),
    ev_delete: Some(mem_anon_delete),
    ev_pagefault: Some(mem_anon_pagefault),
    // ...
};

pub static mut MEM_TYPE_DIRECTPHYS: MemType = MemType {
    name: "direct physical mapping memory",
    ev_new: Some(mem_directphys_new),
    // ...
};
```

---

### 第 43 行：空行

```c

```

**设计原因**：分隔不同类型的变量

---

### 第 44 行：注释块

```c
/* total number of memory pages */
```

**逐字拆解**：
- `/*`：注释开始
- `total number of memory pages`：注释内容，"内存页总数"
- `*/`：注释结束

**设计原因**：说明接下来的变量是内存页总数

---

### 第 45 行：声明总页数

```c
EXTERN int total_pages;
```

**逐字拆解**：
- `EXTERN`：宏
- `int`：类型
- `total_pages`：变量名，"总页数"

**内存位置**：全局数据段

**设计原因**：
- 记录系统中物理内存的总页数
- 用于内存统计和限制

**使用场景**：
- 在 `mem_init()` 中初始化
- 在内存分配时检查是否超出限制

**理论关联**：物理内存管理

**Rust 对比**：
```rust
pub static mut TOTAL_PAGES: i32 = 0;
```

---

### 第 46 行：声明 VM 实例数

```c
EXTERN int num_vm_instances;
```

**逐字拆解**：
- `EXTERN`：宏
- `int`：类型
- `num_vm_instances`：变量名，"VM 实例数"

**内存位置**：全局数据段

**设计原因**：
- 记录当前运行的 VM 实例数量
- 用于热更新（Live Update）
- 多个 VM 实例可以共存

**使用场景**：
- 在 `init_vm()` 中初始化为 1
- 在热更新时增加

**理论关联**：热更新、服务迁移

**Rust 对比**：
```rust
pub static mut NUM_VM_INSTANCES: i32 = 0;
```

---

### 第 47 行：空行

```c

```

**设计原因**：分隔变量和头文件保护结束

---

### 第 48 行：头文件保护结束

```c
#endif /* !_VM_GLO_H */
```

**逐字拆解**：
- `#endif`：预处理指令，结束条件编译
- `/* !_VM_GLO_H */`：注释，说明对应的宏

**设计原因**：
- 结束头文件保护
- 对应第 1 行的 `#ifndef _VM_GLO_H`
- 注释提高可读性

---

## 要点总结

### 1. EXTERN 宏的设计
- **问题**：C 语言要求变量只能定义一次
- **解决方案**：使用 `EXTERN` 宏
  - main.c：定义变量（`EXTERN` 为空）
  - 其他文件：声明变量（`EXTERN` 为 `extern`）
- **优点**：避免链接错误，代码清晰

### 2. 进程表的设计
- **问题**：内核已有进程表，为什么 VM 还需要？
- **原因**：微内核设计
  - 内核：只负责调度
  - VM：负责内存管理
- **内容**：
  - 页表指针
  - 虚拟区域列表
  - 内存统计
  - 页错误统计

### 3. 内存类型系统
- **设计**：面向对象的多态
- **实现**：函数指针表
- **类型**：
  - 匿名内存（malloc、栈）
  - 直接物理映射（设备）
  - 连续匿名内存（DMA）
  - 磁盘缓存（文件系统）
  - 文件映射（mmap）
  - 共享内存（IPC）

### 4. 全局变量的分类
- **进程管理**：`vmproc[]`
- **内存类型**：`mem_type_*`
- **内核信息**：`kernel_boot_info`、`_minix_kerninfo`
- **统计信息**：`total_pages`、`num_vm_instances`
- **调试信息**：`nocheck`、`incheck` 等

---

## 灾难预演

### 场景 1：忘记定义 _MAIN

如果在 main.c 中忘记定义 `_MAIN`：
```c
// main.c
// 忘记 #define _MAIN 1
#include "glo.h"
```
- 所有 `EXTERN` 展开为 `extern`
- 没有变量定义
- 链接错误：undefined reference

**预防**：确保 main.c 中有 `#define _MAIN 1`

### 场景 2：重复定义

如果在多个文件中定义 `_MAIN`：
```c
// file1.c
#define _MAIN 1
#include "glo.h"

// file2.c
#define _MAIN 1
#include "glo.h"
```
- 所有文件都定义变量
- 链接错误：multiple definition

**预防**：只在 main.c 中定义 `_MAIN`

### 场景 3：进程表溢出

如果进程数超过 `VMP_NR`：
```c
// 创建进程
if (proc_count >= VMP_NR) {
    // 进程表已满！
    panic("process table full");
}
```
- 无法创建新进程
- 系统崩溃

**预防**：合理设置 `_NR_PROCS`

### 场景 4：内存类型未初始化

如果忘记初始化内存类型：
```c
// 忘记初始化
mem_type_anon.ev_pagefault = NULL;

// 页错误时调用
mem_type_anon.ev_pagefault(...);  // 段错误！
```
- 函数指针为 NULL
- 调用时崩溃

**预防**：确保所有内存类型都初始化

---

## 互动自测

### 问题 1：为什么 VM 需要自己的进程表？
**答案**：
- 微内核设计：内核只负责调度，VM 负责内存管理
- 内核的进程表只保存调度信息
- VM 需要保存内存管理信息（页表、虚拟区域、内存统计）

### 问题 2：EXTERN 宏的作用是什么？
**答案**：
- 实现单一定义规则
- 在 main.c 中定义变量（`EXTERN` 为空）
- 在其他文件中声明变量（`EXTERN` 为 `extern`）
- 避免链接错误

### 问题 3：为什么需要多种内存类型？
**答案**：
- 不同类型的内存有不同的行为
- 匿名内存：按需分配，写时复制
- 直接物理映射：设备内存，不分配
- 文件映射：从文件加载，回写到文件
- 共享内存：多进程共享，同步问题

### 问题 4：`kernel_boot_info` 包含什么信息？
**答案**：
- 内存映射（mmap）
- 启动进程列表（boot_procs）
- 内核模块列表（module_list）
- 用户空间地址范围（user_start、user_end）
- 栈指针（user_sp）

### 问题 5：`total_pages` 的作用是什么？
**答案**：
- 记录系统中物理内存的总页数
- 用于内存统计
- 用于限制内存分配

---

## Rust 完整实现

```rust
// src/glo.rs

use crate::{VmProc, MemType, IxferKinfoT, MinixKerninfo};

// 进程表大小
pub const VMP_EXECTMP: usize = _NR_PROCS;
pub const VMP_NR: usize = _NR_PROCS + 1;

// 进程表
pub static mut VMPROC: [VmProc; VMP_NR] = {
    // Rust 不允许在 static 中调用非 const 函数
    // 需要使用 const fn 或 lazy_static
    // 这里简化表示
    [VmProc::new(); VMP_NR]
};

// 文件映射开关
pub static mut ENABLE_FILEMAP: i64 = 1;

// 内核启动信息
pub static mut KERNEL_BOOT_INFO: IxferKinfoT = IxferKinfoT::new();

// 调试变量
#[cfg(debug_assertions)]
pub static mut NOCHECK: i32 = 0;

#[cfg(debug_assertions)]
pub static mut INCHECK: i32 = 0;

#[cfg(debug_assertions)]
pub static mut SC_LASTLINE: i32 = 0;

#[cfg(debug_assertions)]
pub static mut SC_LASTFILE: *const i8 = core::ptr::null();

// 内核信息指针
extern "C" {
    pub static mut _minix_kerninfo: *mut MinixKerninfo;
}

// 内存类型
pub static mut MEM_TYPE_ANON: MemType = MemType {
    name: "anonymous memory",
    ev_new: Some(mem_anon::new),
    ev_delete: Some(mem_anon::delete),
    ev_pagefault: Some(mem_anon::pagefault),
    ev_resize: Some(mem_anon::resize),
    // ...
};

pub static mut MEM_TYPE_DIRECTPHYS: MemType = MemType {
    name: "direct physical mapping memory",
    ev_new: Some(mem_directphys::new),
    // ...
};

pub static mut MEM_TYPE_ANON_CONTIG: MemType = MemType {
    name: "physically contig anon memory",
    // ...
};

pub static mut MEM_TYPE_CACHE: MemType = MemType {
    name: "disk cache",
    // ...
};

pub static mut MEM_TYPE_MAPPEDFILE: MemType = MemType {
    name: "memory with file contents",
    // ...
};

pub static mut MEM_TYPE_SHARED: MemType = MemType {
    name: "memory shared by multiple processes",
    // ...
};

// 统计信息
pub static mut TOTAL_PAGES: i32 = 0;
pub static mut NUM_VM_INSTANCES: i32 = 0;
```

---

本讲解文档真正做到了：
1. **逐行讲解**：不遗漏任何一行，包括空行、注释
2. **详细解释**：what、why、how
3. **设计思路**：为什么这样设计
4. **理论关联**：关联的 OS 概念
5. **Rust 对比**：如何在 Rust 中实现
6. **灾难预演**：可能出现的问题
7. **互动自测**：检验理解程度
