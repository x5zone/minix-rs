# 09-vm-relocation.md 审查修改记录

## 审查规则

1. Ch1（概述）和 Ch2（C 源码分析）不得包含 Rust 内容
2. 所有 C 代码引用（文件路径、行号）必须与 Minix3 实际源码一致
3. 所有数值常量必须与 Minix3 源码一致
4. 文档中的 Rust 代码必须与实际 Rust 源码一致
5. 最后一章必须是"参见"
6. Ch3 聚焦"为什么"（设计决策），Ch4 聚焦"怎么做"（实现）

## 审查结果

### 规则 1：Ch1/Ch2 无 Rust 内容 ✅

Ch1（§1 基本概念）和 Ch2（§2 Minix3 C 源码分析）均无 Rust 内容，通过。

### 规则 2：C 代码行号引用修正

以下行号引用与 Minix3 实际源码不符，已修正：

| 位置 | 原引用 | 修正后 | 原因 |
|------|--------|--------|------|
| §2.2 `mem_init()` | `[alloc.c:306-345]` | `[alloc.c:306-335]` | 函数结束于第 335 行，非 345 |
| §2.3 `alloc_mem()` | `[alloc.c:242-281]` | `[alloc.c:242-279]` | 函数结束于第 279 行，非 281 |
| §2.4 `alloc_pages()` | `[alloc.c:404-462]` | `[alloc.c:404-460]` | 函数结束于第 460 行，非 462 |
| §2.5 `findbit()` | `[alloc.c:369-401]` | `[alloc.c:369-399]` | 函数结束于第 399 行，非 401 |
| §2.6 `free_pages()` | `[alloc.c:465-484]` | `[alloc.c:465-481]` | 函数结束于第 481 行，非 484 |
| §2.8 `pt_init_done = 1` | `[pagetable.c:1309]` | `[pagetable.c:1311]` | 实际位于第 1311 行，非 1309 |

以下行号引用经核实正确，无需修改：

- §2.1 `[alloc.c:32-38]` ✅
- §2.6 `[alloc.c:289-301]`（`free_mem()`）✅
- §2.7 `[pagetable.c:59-109]` ✅
- §2.7 `[alloc.c:60-72]`（`reserved_pages` 结构体）✅
- §2.7 `[pagetable.c:1116-1162]` ✅
- §2.8 `[pagetable.c:328]`（`pt_init_done` 声明）✅
- §2.8 `[pagetable.c:333-364]`（`vm_allocpages()` 首段）✅
- §2.9 `[pagetable.c:1313-1352]` ✅
- §2.10 `[pagetable.c:155-230]`（`findhole()`）✅
- §2.12 `[pagetable.c:494-540]`（`pt_ptalloc()`）✅
- §2.13 `[alloc.c:348-367]`（`memstats()`）✅

### 规则 3：数值常量修正

| 位置 | 原文 | 修正后 | 原因 |
|------|------|--------|------|
| §2.1 `NUMBER_PHYSICAL_PAGES` 计算 | `0x100000 / 4096 = 1048576` | `0x100000000 / 4096 = 0x100000 = 1048576` | 被除数应为 `0x100000000`（4GB），原文误写为 `0x100000`（1MB），导致除法不成立 |

其余数值常量经核实均与源码一致：
- `PAGE_CACHE_MAX = 10000` ✅
- `NR_MEMS = 16` ✅
- `SPAREPAGES = 20`（i386）/ `150`（arm）/ `200`（SANITYCHECKS）✅
- `STATIC_SPAREPAGES = 15`（i386）/ `140`（arm）/ `190`（SANITYCHECKS）✅
- `bitchunk_t = uint32_t` ✅
- `MAP_NONE = 0xFFFFFFFE` ✅
- `MAXRESERVEDPAGES = 300` ✅
- `MAXRESERVEDQUEUES = 15` ✅
- `RESERVEDMAGIC = 0x6e4c74d5` ✅
- `CLICK_SIZE = 4096`, `CLICK_SHIFT = 12` ✅

### 规则 4：Rust 代码修正

| 位置 | 原文 | 修正后 | 原因 |
|------|------|--------|------|
| §4.1 `relocate_phys_allocator()` | 使用 `.expect("PtRegion must be initialized before relocation")` | 使用 `.unwrap()` | 实际代码（alloc_page.rs:80）使用 `.unwrap()` |
| §4.4 `BitmapAllocator::reloc_array_info()` | 元素大小硬编码为 `8` | 改为 `core::mem::size_of::<u64>()` 和 `core::mem::size_of::<usize>()` | 实际代码（bitmap_alloc.rs:311-312）使用 `core::mem::size_of` 而非硬编码数值 |
| §4.5 `BuddyAllocator::reloc_array_info()` | 元素大小硬编码为 `4` 和 `1` | 改为 `core::mem::size_of::<u32>()` 和 `core::mem::size_of::<u8>()` | 实际代码（buddy_alloc.rs:355-357）使用 `core::mem::size_of` 而非硬编码数值 |

### 规则 5：最后一章为"参见" ✅

最后一章为"6. 参见"，通过。

### 规则 6：Ch3 聚焦"为什么"，Ch4 聚焦"怎么做" ✅

- Ch3（§3 Rust 设计决策）讨论三种搬迁策略的取舍理由，聚焦"为什么"选择复制搬迁
- Ch4（§4 实现详解）展示具体接口和实现代码，聚焦"怎么做"

通过。

### 其他修正

| 位置 | 修正内容 | 原因 |
|------|----------|------|
| §2.6 `free_mem()` 注释 | 补全为完整注释 | 原文截断了 Minix3 源码中的完整注释，已补全为与源码一致的版本 |

## 无需修改的项

- §4.6 搬迁调用链标记为"示意"，与实际代码存在差异（如 `UninitPhysAllocator`、`from_boot_info` 等不存在），但作为示意性伪代码可接受
- §5 测试代码为文档自写的示例性测试，非实际源码中的测试，作为文档示例可接受
