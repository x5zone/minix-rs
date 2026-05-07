# 07-pagetable-ops.md 修订记录

## 修订日期: 2026-05-07

## 变更列表

### 1. C 源码行号修正

| 位置 | 原文 | 修正 | 原因 |
|------|------|------|------|
| §2.1.3 pagedir_mappings 初始化 | `pagetable.c:1039` | `pagetable.c:1035` | `pt_allocate_kernel_mapped_pagetables` 函数定义始于第 1035 行，1039 是 for 循环体 |
| §2.3.1 pt_writemap 调用场景 | `region.c:283` | `region.c:280` | `pt_writemap` 调用始于第 280 行，283 是 SANITYCHECKS 条件分支 |
| §2.4.3 pt_clearmapcache 调用场景 | `pagetable.c:118` | `pagetable.c:115` | `pt_assert` 函数定义始于第 115 行，118 是其内部的 `pt_clearmapcache()` 调用 |
| 附录 A.2 vm_lookup | `memory.c:325-370` | `memory.c:325-372` | 函数体结束于第 372 行（含闭合大括号） |

### 2. 章节编号修正

| 位置 | 原文 | 修正 | 原因 |
|------|------|------|------|
| pt_writable 标题 | `#### 2.3.5` | `#### 2.4.2` | 该节位于 §2.4 页表遍历之下，应编为 2.4.2 |
| pt_clearmapcache 标题 | `#### 2.3.6` | `#### 2.4.3` | 该节位于 §2.4 页表遍历之下，应编为 2.4.3 |

### 3. Rust 函数映射修正（§3.1）

| 位置 | 原文 | 修正 | 原因 |
|------|------|------|------|
| §3.1 Minix3 函数映射表 | `pt_bind()` → `Paging::switch()` | `pt_bind()` → `VmPagingExt::bind_to_process()` | `pt_bind()` 的语义是绑定页表到进程（通知内核），对应 `VmPagingExt::bind_to_process()`；`Paging::switch()` 仅对应内核的 CR3 加载（激活页表），与 §4.3 的映射表一致 |

### 4. Rust 代码与实际源码对齐

#### 4.1 VirBytes / PhysBytes 类型定义（§3.2）

- VirBytes: 添加 `#[repr(transparent)]`、`Hash`、`Default` derive
- PhysBytes: 添加 `#[repr(transparent)]`、`Hash` derive

实际源码位置: `os/libs/minix-types/src/types/address.rs`

#### 4.2 Paging trait 定义（§3.2）

原文仅列出 `map()`、`map_range()`、`switch()`、`destroy()` 四个方法，与实际 trait 定义不符。更新为包含完整方法签名：

- 添加 `const PAGE_SIZE: usize`
- 添加 `fn new()`
- 添加 `fn remap()` （原子覆盖映射，对应 WMF_OVERWRITE）
- 添加 `fn unmap()` （取消映射，返回原物理地址）
- 添加 `fn update_flags()` （更新标志位，对应 WMF_WRITEFLAGSONLY）
- 添加 `fn query()` （查询映射）
- 添加 `fn root_paddr()` （获取页表根物理地址）
- 添加 `unsafe fn flush_tlb()` / `unsafe fn flush_tlb_addr()`
- `map_range()` 补充默认实现体
- 添加 `fn unmap_range()` 及默认实现体

实际源码位置: `os/arch/src/paging.rs`

#### 4.3 MockPaging 实现（§3.2）

- 添加 `const PAGE_SIZE: usize = 4096`
- 添加 `fn new()` 实现
- `map()` 方法添加对齐检查和 `AlreadyMapped` 检查（与实际代码一致）
- `switch()` 方法使用 `AtomicUsize::store` 替代 `Option` 赋值
- 添加 `destroy()` 实现
- 添加 `// ... 其他方法省略` 注释

实际源码位置: `os/arch/src/paging.rs` mock 模块

#### 4.4 PageTableError 枚举（§3.3）

- 添加 `Copy` derive（实际代码为 `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`）

实际源码位置: `os/arch/src/paging.rs`

## 已验证无误的内容

### C 源码行号（全部正确）

- `pt_new`: pagetable.c:990 ✅
- `pt_free`: pagetable.c:1427 ✅
- `pt_bind`: pagetable.c:1358 ✅
- `pt_writemap`: pagetable.c:784 ✅
- `pt_checkrange`: pagetable.c:943 ✅
- `vm_mappages`: pagetable.c:295 ✅
- `pt_map_in_range`: pagetable.c:631 ✅
- `pt_writable`: pagetable.c:761 ✅
- `pt_clearmapcache`: pagetable.c:751 ✅
- `pt_mapkernel`: pagetable.c:1442 ✅
- `pagedir_mappings` 结构定义: pagetable.c:38 ✅

### C 源码调用场景（全部正确）

- fork.c:94 (pt_bind) ✅
- main.c:211,355,717-718 (pt_bind) ✅
- main.c:212,719,749 (pt_clearmapcache) ✅
- exit.c:35-36 (map_free_proc / pt_free) ✅
- exit.c:137 (pt_bind) ✅
- pagetable.c:1329,1343 (pt_bind) ✅
- region.c:153,1139 (pt_writemap) ✅
- pagetable.c:425 (pt_writemap + WMF_WRITEFLAGSONLY) ✅
- mmap.c:500 (pt_writemap + WMF_FREE) ✅
- utility.c:326,331 (pt_map_in_range) ✅
- region.c:55 (pt_writable) ✅
- region.c:747 (pt_checkrange) ✅
- pagefaults.c:153 (pt_clearmapcache) ✅

### 数值常量（全部正确）

- MAX_PAGEDIR_PDES = 5 ✅
- WMF_OVERWRITE = 0x01 ✅
- WMF_WRITEFLAGSONLY = 0x02 ✅
- WMF_FREE = 0x04 ✅
- WMF_VERIFY = 0x08 ✅

### 规则合规性

- Ch1/Ch2 不含 Rust 内容 ✅
- 最后一章为"参见" ✅
- Ch3 聚焦"why"（设计决策动机） ✅
- Ch4 聚焦"how"（实现语义对应） ✅
