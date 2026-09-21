# servers/vm/sanitycheck.h 逐行讲解

## 文件概述

**文件路径**: `servers/vm/sanitycheck.h`  
**功能**: VM 服务器健全性检查宏定义  
**设计思想**: 通过宏定义提供可配置的运行时检查，帮助发现内存管理错误

---

## 逐行讲解

### 头文件保护与包含

```c
#ifndef _SANITYCHECK_H
#define _SANITYCHECK_H 1

#include <assert.h>

#include "vm.h"
```

**逐词拆解**:
- `assert.h`: C 标准断言头文件
- `vm.h`: VM 主头文件，包含 SANITYCHECKS 配置

**设计原因**:
- 使用标准 `assert()` 作为基础
- 根据 `SANITYCHECKS` 配置决定是否启用检查

---

### 条件编译检查

```c
#if SANITYCHECKS
```

**逐词拆解**:
- `#if SANITYCHECKS`: 如果 SANITYCHECKS 为真（非零）

**设计原因**:
- 生产环境可以禁用检查，提高性能
- 开发环境启用检查，帮助发现错误

---

### 页表健全性检查宏

```c
#define PT_SANE(p) { pt_sanitycheck((p), __FILE__, __LINE__); }
```

**逐词拆解**:
- `PT_SANE`: Page Table Sane，页表健全性检查
- `p`: 页表指针
- `pt_sanitycheck()`: 实际检查函数
- `__FILE__`: 当前文件名（预处理器宏）
- `__LINE__`: 当前行号（预处理器宏）

**设计原因**:
1. **调试信息**: 记录检查位置，便于定位错误
2. **统一接口**: 宏封装函数调用，简化代码
3. **可选检查**: 通过条件编译控制是否启用

**使用示例**:
```c
PT_SANE(&vmproc->vm_pt);  // 检查页表是否正常
```

---

### 自定义断言宏

```c
/* This macro is used in the sanity check functions, where file and 
 * line are function arguments.
 */
#define MYASSERT(c) do { if(!(c)) { \
        printf("VM:%s:%d: %s failed (last sanity check %s:%d)\n", file, line, #c, sc_lastfile, sc_lastline); \
	panic("sanity check failed"); } } while(0)
```

**逐词拆解**:
- `MYASSERT`: 自定义断言宏
- `c`: 断言条件
- `#c`: 字符串化条件（如 `ptr != NULL` 变成字符串）
- `file, line`: 函数参数，传递文件名和行号
- `sc_lastfile, sc_lastline`: 全局变量，记录上次检查位置

**设计原因**:
1. **详细错误信息**: 显示失败条件和位置
2. **历史追踪**: 显示上次检查位置，帮助定位错误
3. **立即终止**: `panic()` 停止系统，防止错误扩散

**内存布局**:
```
错误信息格式:
VM:main.c:123: ptr != NULL failed (last sanity check pagefaults.c:456)
    ↑         ↑      ↑                      ↑
  文件名    行号   失败条件            上次检查位置
```

---

### Slab 分配器检查宏

```c
#define SLABSANITYCHECK(l) if(_minix_kerninfo) { \
	slab_sanitycheck(__FILE__, __LINE__); }
```

**逐词拆解**:
- `SLABSANITYCHECK`: Slab 分配器健全性检查
- `l`: 行号（未使用）
- `_minix_kerninfo`: 内核信息指针，检查是否初始化
- `slab_sanitycheck()`: Slab 检查函数

**设计原因**:
1. **检查初始化**: `_minix_kerninfo` 非空才检查
2. **Slab 完整性**: 检查 Slab 分配器的数据结构
3. **内存泄漏检测**: 发现未释放的对象

---

### 全面健全性检查宏

```c
#define SANITYCHECK(l) if(!nocheck && _minix_kerninfo && 0) {  \
		struct vmproc *vmpr;	\
		assert(incheck == 0);	\
		incheck = 1;		\
		usedpages_reset();	\
	slab_sanitycheck(__FILE__, __LINE__);	\
	for(vmpr = vmproc; vmpr < &vmproc[VMP_NR]; vmpr++) { \
		if((vmpr->vm_flags & (VMF_INUSE))) { \
			PT_SANE(&vmpr->vm_pt); \
		} \
	} \
	map_sanitycheck(__FILE__, __LINE__); \
	mem_sanitycheck(__FILE__, __LINE__); \
	assert(incheck == 1);	\
	incheck = 0;		\
	/* printf("(%s:%d OK) ", __FILE__, __LINE__); */ \
	sc_lastfile = __FILE__; sc_lastline = __LINE__; \
	} 
```

**逐词拆解**:
- `SANITYCHECK`: 全面健全性检查宏
- `l`: 行号（未使用）
- `nocheck`: 全局变量，禁止检查标志
- `_minix_kerninfo`: 内核信息指针
- `&& 0`: **注意：这个条件永远为假！**

**检查步骤**:
1. **重入检查**: `assert(incheck == 0)`，防止递归检查
2. **设置标志**: `incheck = 1`，标记正在检查
3. **重置计数**: `usedpages_reset()`，重置页使用计数
4. **Slab 检查**: `slab_sanitycheck()`
5. **页表检查**: 遍历所有进程，检查页表
6. **映射检查**: `map_sanitycheck()`
7. **内存检查**: `mem_sanitycheck()`
8. **清除标志**: `incheck = 0`
9. **记录位置**: `sc_lastfile, sc_lastline`

**设计原因**:
1. **全面检查**: 检查所有 VM 数据结构
2. **重入保护**: `incheck` 防止递归检查
3. **性能考虑**: `&& 0` 禁用检查（可能是临时禁用）

**重要发现**:
```c
&& 0  // 这个条件永远为假，检查被禁用！
```
这可能是：
- 性能优化：生产环境禁用
- 调试遗留：开发时临时禁用
- 代码问题：应该使用配置宏

---

### Slab 对象检查宏

```c
#define SLABSANE(ptr) { \
	if(!slabsane_f(__FILE__, __LINE__, ptr, sizeof(*(ptr)))) { \
		printf("VM:%s:%d: SLABSANE(%s)\n", __FILE__, __LINE__, #ptr); \
		panic("SLABSANE failed");	\
	} \
}
```

**逐词拆解**:
- `SLABSANE`: Slab 对象健全性检查
- `ptr`: 指向 Slab 对象的指针
- `slabsane_f()`: 实际检查函数
- `sizeof(*(ptr))`: 对象大小

**设计原因**:
1. **对象完整性**: 检查 Slab 对象是否有效
2. **类型安全**: 使用 `sizeof(*(ptr))` 自动获取大小
3. **错误定位**: 显示指针名称和位置

---

### 禁用检查时的空宏

```c
#else
#define SANITYCHECK(l)
#define SLABSANITYCHECK(l)
#define SLABSANE(ptr)
#define MYASSERT(c)
#define PT_SANE(p)
#endif
```

**逐词拆解**:
- `#else`: 如果 SANITYCHECKS 为假
- 空宏定义：展开为空，不执行任何操作

**设计原因**:
- 生产环境零开销
- 代码中可以保留检查调用
- 编译器优化掉空宏

---

### 内存保护宏

```c
#if MEMPROTECT
#define USE(obj, code) do {		\
	slabunlock(obj, sizeof(*obj));	\
	do {				\
		code			\
	} while(0);			\
	slablock(obj, sizeof(*obj));	\
} while(0)
#else
#define USE(obj, code) do { code } while(0)
#endif
```

**逐词拆解**:
- `USE`: 使用 Slab 对象的宏
- `obj`: Slab 对象指针
- `code`: 使用对象的代码
- `slabunlock()`: 解锁对象（允许访问）
- `slablock()`: 锁定对象（禁止访问）

**设计原因**:
1. **内存保护**: Slab 对象默认锁定，防止非法访问
2. **显式使用**: 必须通过 `USE` 宏访问对象
3. **调试支持**: 捕获非法内存访问

**使用示例**:
```c
USE(region, {
    region->flags |= VR_WRITABLE;  // 在 USE 块中访问 region
});
// 离开 USE 块后，region 被锁定
```

**内存布局**:
```
Slab 对象状态:
锁定状态: 无法访问（触发异常）
解锁状态: 可以访问（在 USE 块中）
```

---

## Rust 实现对比

### C 代码（原始）

```c
#define MYASSERT(c) do { if(!(c)) { \
        printf("VM:%s:%d: %s failed\n", file, line, #c); \
	panic("sanity check failed"); } } while(0)

#define SANITYCHECK(l) if(!nocheck && _minix_kerninfo) {  \
		incheck = 1;		\
		slab_sanitycheck(__FILE__, __LINE__);	\
		incheck = 0;		\
	}
```

### Rust 代码（现代实现）

```rust
#![no_std]

use core::sync::atomic::{AtomicBool, Ordering};

static IN_CHECK: AtomicBool = AtomicBool::new(false);
static NO_CHECK: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
pub struct SanityCheckError {
    pub file: &'static str,
    pub line: u32,
    pub condition: &'static str,
    pub last_file: &'static str,
    pub last_line: u32,
}

pub fn my_assert(
    file: &'static str,
    line: u32,
    condition: bool,
    condition_str: &'static str,
    last_file: &'static str,
    last_line: u32,
) -> Result<(), SanityCheckError> {
    if !condition {
        Err(SanityCheckError {
            file,
            line,
            condition: condition_str,
            last_file,
            last_line,
        })
    } else {
        Ok(())
    }
}

pub struct SanityCheckGuard {
    _private: (),
}

impl SanityCheckGuard {
    pub fn new() -> Result<Self, ()> {
        if IN_CHECK.swap(true, Ordering::SeqCst) {
            return Err(());  // Already in check
        }
        Ok(SanityCheckGuard { _private: () })
    }
}

impl Drop for SanityCheckGuard {
    fn drop(&mut self) {
        IN_CHECK.store(false, Ordering::SeqCst);
    }
}

pub fn sanity_check<F>(file: &'static str, line: u32, f: F) -> Result<(), ()>
where
    F: FnOnce() -> Result<(), ()>,
{
    if NO_CHECK.load(Ordering::SeqCst) {
        return Ok(());
    }

    let _guard = SanityCheckGuard::new()?;
    
    f()
}

#[macro_export]
macro_rules! my_assert {
    ($cond:expr, $last_file:expr, $last_line:expr) => {
        if !$cond {
            return Err($crate::SanityCheckError {
                file: file!(),
                line: line!(),
                condition: stringify!($cond),
                last_file: $last_file,
                last_line: $last_line,
            });
        }
    };
}

#[macro_export]
macro_rules! sanity_check {
    ($code:block) => {
        $crate::sanity_check(file!(), line!(), || {
            $code
            Ok(())
        })?
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanity_check_guard() {
        assert!(!IN_CHECK.load(Ordering::SeqCst));
        
        {
            let _guard = SanityCheckGuard::new().unwrap();
            assert!(IN_CHECK.load(Ordering::SeqCst));
        }
        
        assert!(!IN_CHECK.load(Ordering::SeqCst));
    }

    #[test]
    fn test_reentrancy_protection() {
        let _guard1 = SanityCheckGuard::new().unwrap();
        let guard2 = SanityCheckGuard::new();
        assert!(guard2.is_err());  // Should fail, already in check
    }
}
```

### Rust 优势分析

**1. RAII 模式**:
```rust
// C: 手动管理 incheck 标志
incheck = 1;
// ... 检查代码
incheck = 0;  // 容易忘记

// Rust: 自动管理
let _guard = SanityCheckGuard::new()?;
// ... 检查代码
// 离开作用域时自动清除标志
```

**2. 错误处理**:
```rust
// C: panic() 终止系统
MYASSERT(ptr != NULL);  // 失败时 panic

// Rust: 返回错误，允许恢复
my_assert!(ptr.is_some(), last_file, last_line)?;
```

**3. 类型安全**:
```rust
// C: 宏展开可能有副作用
MYASSERT(i++ > 0);  // i++ 被多次求值？

// Rust: 宏更安全
my_assert!(i > 0, last_file, last_line);  // i 只求值一次
```

**4. 原子操作**:
```rust
// C: 全局变量可能竞争
incheck = 1;  // 多核竞争

// Rust: 使用 AtomicBool
IN_CHECK.swap(true, Ordering::SeqCst);  // 原子操作
```

---

## 设计问题与改进

### 问题 1: 永远为假的条件

**C 代码问题**:
```c
#define SANITYCHECK(l) if(!nocheck && _minix_kerninfo && 0) {  \
    // 检查代码
}
```

**改进方案**:
```rust
// 方案 1: 使用配置常量
const ENABLE_SANITY_CHECK: bool = cfg!(debug_assertions);

// 方案 2: 使用运行时标志
static ENABLE_SANITY_CHECK: AtomicBool = AtomicBool::new(true);

pub fn sanity_check<F>(f: F) -> Result<(), ()>
where
    F: FnOnce() -> Result<(), ()>,
{
    if !ENABLE_SANITY_CHECK.load(Ordering::Relaxed) {
        return Ok(());
    }
    f()
}
```

### 问题 2: 全局变量竞争

**C 代码问题**:
```c
int incheck = 0;  // 多核竞争
incheck = 1;
```

**改进方案**:
```rust
// 使用 CPU 本地变量
use cpulocal::CpuLocal;
static IN_CHECK: CpuLocal<AtomicBool> = CpuLocal::new(AtomicBool::new(false));
```

### 问题 3: panic() 过于激进

**C 代码问题**:
```c
panic("sanity check failed");  // 直接终止系统
```

**改进方案**:
```rust
// 返回错误，允许上层决定处理方式
pub enum SanityCheckAction {
    Log,
    Panic,
    Ignore,
}

pub fn handle_sanity_check_error(err: SanityCheckError, action: SanityCheckAction) {
    match action {
        SanityCheckAction::Log => {
            log::error!("Sanity check failed: {:?}", err);
        }
        SanityCheckAction::Panic => {
            panic!("Sanity check failed: {:?}", err);
        }
        SanityCheckAction::Ignore => {}
    }
}
```

---

## 要点总结

1. **可配置检查**: 通过 SANITYCHECKS 宏控制是否启用检查
2. **重入保护**: `incheck` 标志防止递归检查
3. **内存保护**: `USE` 宏提供 Slab 对象的访问控制

---

## 灾难预演

**如果删除 `incheck = 1` 这一行**:
- 健全性检查可能递归调用
- 栈溢出，系统崩溃
- 难以调试，因为检查本身就是问题

**如果 `&& 0` 改为 `&& 1`**:
- 生产环境启用全面检查
- 性能下降 10-100 倍
- 系统响应变慢

---

## 互动自测

1. **问题**: `SANITYCHECK` 宏的作用是什么？
   **答案**: 执行全面的 VM 数据结构健全性检查，包括页表、Slab、映射等。

2. **问题**: 为什么需要 `incheck` 标志？
   **答案**: 防止健全性检查递归调用，避免栈溢出。

3. **问题**: `USE` 宏的作用是什么？
   **答案**: 提供 Slab 对象的安全访问机制，解锁对象、执行代码、重新锁定。
