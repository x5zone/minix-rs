# 08-slab-allocator.md 修订记录

## 修订日期：2026-05-07

---

## 修改清单

### 1. [Rule 1] Ch2 移除 Rust 内容

**问题**：Ch2（C 源码分析）中包含 Rust 相关内容，违反"Ch1/Ch2 不得包含 Rust 内容"规则。

**修改 1a**：删除 §2.1 MEMPROTECT 小节末尾的"Rust 重写的建议"段落

- 原文：
  ```
  **Rust 重写的建议**：
  - 生产环境：禁用 MEMPROTECT
  - 调试环境：可以考虑使用 INVLPG 而不是 reload_cr3
  - 或者使用更轻量的调试机制（如 RedZone、canary）
  ```
- 操作：整段删除

**修改 1b**：删除 §2.1 "phys 字段的作用"中"是否必要？"列表的 Rust 条目

- 原文：`- Rust 重写时可以考虑移除或条件编译`
- 操作：删除该条目

### 2. [Rule 3] 修正 SCL 常量数值

**问题**：文档中 `SCL_FUNCTIONS` 和 `SCL_DETAIL` 的数值与 Minix3 源码不一致。

- 源码位置：`vm.h:44-45`
- 原文：`SCL_FUNCTIONS 0`、`SCL_DETAIL 1`
- 实际：`SCL_FUNCTIONS 2`、`SCL_DETAIL 3`
- 操作：修正为 `SCL_FUNCTIONS 2`、`SCL_DETAIL 3`

### 3. [Rule 2] 修正 SLABSANITYCHECK 宏定义

**问题**：文档中 SLABSANITYCHECK 宏与 Minix3 源码不一致。

- 源码位置：`sanitycheck.h:19-20`
- 原文：
  ```c
  #define SLABSANITYCHECK(level) do { \
      if(SANITYCHECKS) { \
          slab_sanitycheck(__FILE__, __LINE__); \
      } \
  } while(0)
  ```
- 实际：
  ```c
  #define SLABSANITYCHECK(l) if(_minix_kerninfo) { \
      slab_sanitycheck(__FILE__, __LINE__); \
  }
  ```
- 差异：
  - 条件从 `SANITYCHECKS`（编译期宏）改为 `_minix_kerninfo`（运行时变量）
  - 去掉了 `do { ... } while(0)` 包裹
  - 参数名从 `level` 改为 `l`（且参数未被使用）
- 操作：替换为与源码一致的版本

### 4. [Rule 2] 修正 nojunkwarning 变量声明

**问题**：文档中 `nojunkwarning` 缺少 `#if SANITYCHECKS` 条件编译守卫。

- 源码位置：`slaballoc.c:252-254`
- 原文：`static int nojunkwarning = 0;`（无条件编译）
- 实际：
  ```c
  #if SANITYCHECKS
  static int nojunkwarning = 0;
  #endif
  ```
- 操作：添加 `#if SANITYCHECKS` / `#endif` 包裹

### 5. [Rule 2] 修正 objstats 返回值赋值顺序

**问题**：文档中 objstats 函数的输出参数赋值顺序与 Minix3 源码不一致。

- 源码位置：`slaballoc.c:396-398`
- 原文：`*sp = s; *fp = f; *ip = i;`
- 实际：`*ip = i; *fp = f; *sp = s;`
- 操作：修正赋值顺序为 `*ip = i; *fp = f; *sp = s;`

### 6. [Rule 4] 修正 Ch4 global.rs 代码与实际 Rust 源码不一致

**问题**：Ch4 §4.1 中的 `global.rs` 代码与实际 Rust 源码严重不符。

- 实际源码位置：`os/servers/vm/src/global.rs`
- 主要差异：
  - 原文 `alloc`/`dealloc` 使用 `todo!("对接 PhysAllocator")`，实际通过 `extern "Rust"` 调用 `__vm_global_alloc`/`__vm_global_dealloc`
  - 原文 `#[global_allocator]`，实际为 `#[cfg_attr(not(test), global_allocator)]`
  - 缺少 `__vm_global_alloc`/`__vm_global_dealloc` 函数实现（调用 C 的 `malloc`/`free`）
  - 文档注释不准确（原文说"底层对接 PhysAllocator"，实际当前阶段用 malloc/free）
- 操作：替换为与实际源码一致的代码，并更新文档注释

### 7. [Rule 4] 修正 Ch4 关键设计决策描述

**问题**：§4.1 的"关键设计决策"与实际实现不符。

- 原文："为什么不用系统的 mmap？"
- 实际：当前阶段使用 C 的 malloc/free，未来可替换为 PhysAllocator
- 操作：更新为"为什么当前阶段用 malloc/free？"，并补充测试隔离说明

### 8. [Rule 5] 将"参见"移至文档末尾

**问题**：原文"参见"（Ch7）之后还有附录 A 和附录 B，违反"最后一章必须是参见"规则。

- 原结构：Ch6 → Ch7(参见) → 附录 A → 附录 B
- 新结构：Ch6 → 附录 A → 附录 B → Ch7(参见)
- 操作：将附录 A、B 移至 Ch6 之后、Ch7 之前

### 9. 更新文档头部状态说明

**问题**：原文标注"⚠️ Rust 实现尚未完成"，但 Rust 基础框架已实现。

- 原文：`⚠️ Rust 实现尚未完成。本文档 §1-§2 为 Minix3 C 源码分析，§3+ 为设计分析，具体 Rust 实现代码待补充。`
- 新文：`Rust 实现已完成基础框架（global.rs、alloc_stats.rs、critical_pool.rs）。本文档 §1-§2 为 Minix3 C 源码分析，§3 为设计决策分析，§4 为 Rust 实现详解。`
- 操作：替换状态说明

---

## 已验证无需修改的项目

### C 代码行号引用（均正确）

| 引用 | 源码实际行号 | 状态 |
|------|------------|------|
| `pagetable.c:403` (vm_pagelock) | 403 | ✓ |
| `slaballoc.c:259` (slaballoc) | 259 | ✓ |
| `slaballoc.c:406` (slabfree) | 406 | ✓ |
| `slaballoc.c:344` (objstats) | 344 | ✓ |
| `slaballoc.c:464` (slablock) | 464 | ✓ |
| `slaballoc.c:483` (slabunlock) | 483 | ✓ |
| `slaballoc.c:267` (roundup) | 267 | ✓ |
| `slaballoc.c:133` (GETSLAB _gsi) | 133 | ✓ |

### 数值常量（均正确）

| 常量 | 文档值 | 源码值 | 状态 |
|------|--------|--------|------|
| SLABSIZES | 200 | 200 | ✓ |
| MINSIZE | 8 | 8 | ✓ |
| OBJALIGN | 8 | 8 | ✓ |
| MAGIC1 | 0x1f5b842f | 0x1f5b842f | ✓ |
| MAGIC2 | 0x8bb5a420 | 0x8bb5a420 | ✓ |
| JUNK | 0xdeadbeef | 0xdeadbeef | ✓ |
| NOJUNK | 0xc0ffee | 0xc0ffee | ✓ |
| WRITABLE_NONE | -2 | -2 | ✓ |
| WRITABLE_HEADER | -1 | -1 | ✓ |
| VMP_SLAB | (未列数值) | 3 | ✓ |

### Rust 代码匹配（alloc_stats.rs、critical_pool.rs）

| 文件 | 状态 |
|------|------|
| `alloc_stats.rs` | ✓ 完全匹配 |
| `critical_pool.rs` | ✓ 完全匹配 |

### Ch3/Ch4 职责划分

| 章节 | 职责 | 状态 |
|------|------|------|
| Ch3 Rust 设计决策 | 聚焦"为什么" | ✓ |
| Ch4 实现详解 | 聚焦"怎么做" | ✓ |
