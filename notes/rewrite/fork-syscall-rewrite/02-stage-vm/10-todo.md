# 10-phys-block.md 修订记录

## 审查规则

1. Ch1 (概述) 和 Ch2 (C 源码分析) 不得包含 Rust 内容
2. 所有 C 代码引用必须与 Minix3 源码一致
3. 所有数值常量必须与 Minix3 源码一致
4. 文档中的 Rust 代码必须与 `/workspace/os/servers/vm/src/` 实际代码一致
5. 最后一章必须是"参见"
6. Ch3 聚焦"为什么"设计决策，Ch4 聚焦"如何"实现

## 审查结果

### 规则 1: Ch1/Ch2 无 Rust 内容 — ✅ 通过

Ch1 和 Ch2 不包含任何 Rust 代码。

### 规则 2: C 代码引用 — ✅ 通过

所有 C 代码引用与 Minix3 源码一致：
- `struct phys_block` 定义与 `region.h:23-33` 一致
- `pb_new()`, `pb_free()`, `pb_link()`, `pb_reference()`, `pb_unreferenced()` 与 `pb.c` 一致
- `SLABALLOC/SLABFREE` 宏与 `proto.h:133-134` 一致
- `MAP_NONE = 0xFFFFFFFE` 与 `vm.h:61` 一致
- `PBF_INCACHE = 0x01` 与 `region.h:35` 一致
- `ABS2CLICK/CLICK2ABS` 宏与 `const.h:100-101` 一致
- `CLICK_SHIFT = 12` 与 `const.h:85` 一致
- `anon_unreference()` 与 `mem_anon.c:56-62` 一致

### 规则 3: 数值常量 — ✅ 通过

所有数值常量与 Minix3 源码一致。

### 规则 4: Rust 代码与实际源码一致 — ❌ 大量不符，已修复

以下是与实际 Rust 代码不符的问题及修复：

#### 4.1 PhysBlock 结构体 (§3.1)

| 项目 | 修改前（文档） | 修改后（实际代码） |
|------|---------------|-------------------|
| `phys` 字段类型 | `PhysBytes` (newtype) | `u64` |
| `phys` 可见性 | `pub` | 私有 |
| `firstregion` 字段名 | `firstregion` | `first_region` |
| `firstregion` 类型 | `Option<NonNull<PhysRegion>>` | `Option<*mut PhysRegion>` |
| `refcount` 类型 | `Cell<u8>` | `u8` |
| `flags` 类型 | `PbFlags` (手动 newtype) | `PhysBlockFlags` (bitflags 宏) |
| 结构体可见性 | `pub` | `pub(crate)` |
| `MAP_NONE` 值 | `0xFFFFFFFE` (32位) | `0xFFFF_FFFF_FFFF_FFFE` (64位) |

#### 4.2 PhysBlock 方法 (§3.1)

| 项目 | 修改前 | 修改后 |
|------|--------|--------|
| `has_phys()` | 存在 | 改为 `is_mapped()` |
| `is_shared()` | 存在 | 不存在（CoW 判断在 PhysRegion 上） |
| `is_referenced()` | 存在 | 不存在 |
| `first_region()` | 存在 | 不存在 |
| `inc_refcount()`/`dec_refcount()` | unsafe 方法 | 不存在 |
| `inc_ref()`/`dec_ref()` | panic on overflow/underflow | 改为 `add_ref()`(saturating_add)/`release_ref()`(返回 bool) |
| `needs_cow()`/`is_private()` | 在 PhysBlock 上 | `needs_cow()` 在 PhysRegion 上，`is_private()` 不存在 |

#### 4.3 PbFlags → PhysBlockFlags (§3.1)

| 项目 | 修改前 | 修改后 |
|------|--------|--------|
| 名称 | `PbFlags` | `PhysBlockFlags` |
| 实现方式 | 手动 newtype + impl | `bitflags::bitflags!` 宏 |
| 可见性 | `pub` | `pub(crate)` |
| 常量定义 | `pub const IN_CACHE: Self = Self(0x01)` | `const IN_CACHE = 0x01` |

#### 4.4 引用计数管理 (§3.2)

| 项目 | 修改前 | 修改后 |
|------|--------|--------|
| 类型 | `Cell<u8>` | `u8` |
| 增加 | `self.refcount.set(count + 1)` | `self.refcount.saturating_add(1)` |
| 减少 | `self.refcount.set(count - 1)` | `if self.refcount > 0 { self.refcount -= 1 }` |
| 溢出行为 | panic | 饱和到 255 |
| 下溢行为 | panic | 不递减（条件判断） |
| 验证函数 | `verify_refcount()` | `iterate_block_refs()` |

#### 4.5 操作方法位置 (§3.1, §4.2)

| 项目 | 修改前 | 修改后 |
|------|--------|--------|
| 链接方法 | `PhysBlock::link()` | `PhysRegion::link_to_block()` |
| 解链方法 | `PhysBlock::unlink()` | `PhysRegion::unlink_from_block()` |
| 简化链接 | 不存在 | `PhysRegion::bind_block()` |
| 简化解链 | 不存在 | `PhysRegion::unbind_block()` |
| 参数风格 | `&mut self` (PhysBlock) + `pr: &mut PhysRegion` | `&mut self` (PhysRegion) + `block: *mut PhysBlock` |

#### 4.6 Slab 相关代码 (§3.3, §4.1, §4.3, §4.4)

| 项目 | 修改前 | 修改后 |
|------|--------|--------|
| PhysBlockSlab | 存在 | 不存在（使用 `Box<PhysBlock>`） |
| PB_SLAB / PHYSR_SLAB | 全局 Mutex | 不存在 |
| pb_alloc() | 从 Slab 分配 | 不存在（使用 `Box::new()`） |
| pb_free() | 归还 Slab | 不存在（Box Drop） |
| pb_alloc_with_page() | 存在 | 不存在 |
| pb_alloc_delayed() | 存在 | 不存在 |
| Mutex 保护 | 存在 | 不存在 |
| irq_safe 函数 | 存在 | 不存在 |

#### 4.7 MemoryType trait (§4.3)

| 项目 | 修改前 | 修改后 |
|------|--------|--------|
| 名称 | `MemoryType` | `MemType` |
| 方法名 | `ev_unreference` | `on_unreference` |
| 返回类型 | `Result<(), i32>` | `Result<bool, MemTypeError>` |
| 参数 | `&PhysRegion` | `&mut PhysRegion` |
| 可见性 | `pub` | `pub(crate)` |
| Supertraits | 无 | `Send + Sync` |

#### 4.8 内存类型实现 (§4.3)

| 项目 | 修改前 | 修改后 |
|------|--------|--------|
| `AnonMemoryType` | 直接释放物理页 | `AnonymousMemory`，返回 `Ok(true)`/`Ok(false)` |
| `FileMemoryType` | 存在 | 不存在（实际有 `DirectPhysical` 和 `SharedMemory`） |

#### 4.9 释放策略 (§4.3)

| 项目 | 修改前 | 修改后 |
|------|--------|--------|
| pb_unreferenced() | 独立 unsafe 函数 | 不存在（由 `unlink_from_block` + `on_unreference` 组合） |
| pb_free() | 独立 unsafe 函数 | 不存在（Box Drop） |
| 释放回调语义 | 回调直接释放物理页 | 回调返回是否需要释放，调用者执行 |

#### 4.10 线程安全 (§4.4)

| 项目 | 修改前 | 修改后 |
|------|--------|--------|
| AtomicPhysBlock | 存在 | 不存在 |
| 内存顺序讨论 | 存在 | 移除 |
| Mutex 保护 Slab | 存在 | 不存在 |
| irq_safe 函数 | 存在 | 不存在 |
| 并发测试 | 存在 | 移除 |
| MemType Send+Sync | 未提及 | 已添加说明 |

#### 4.11 CoW 代码 (§5.1, §5.2)

| 项目 | 修改前 | 修改后 |
|------|--------|--------|
| fork 共享 | `fork_share_phys()` 函数 | `clone_region_for_fork()` + `link_phys_blocks()` |
| CoW 实现 | `mem_cow()` 独立函数 | `AnonymousMemory::on_pagefault` 返回 `PagefaultResult::NeedCow` |
| 共享检测 | `PhysBlock::needs_cow()` | `PhysRegion::needs_cow()` |
| CoW 执行 | 直接在 mem_cow 中执行 | 上层根据 PagefaultResult 调度 |

#### 4.12 测试要点 (§6)

| 项目 | 修改前 | 修改后 |
|------|--------|--------|
| 引用计数方法 | `inc_ref`/`dec_ref` | `add_ref`/`release_ref` |
| 溢出/下溢 | panic | 饱和/条件判断 |
| 生命周期 | `pb_alloc` → `pb_free` | `PhysBlock::new()` → Box Drop |
| Slab 测试 | 存在 | 替换为 MemType 测试 |

### 规则 5: 最后一章是"参见" — ✅ 通过

最后一章为"7. 参见"。

### 规则 6: Ch3 聚焦"为什么"，Ch4 聚焦"如何" — ❌ 已修复

**修改前问题**：
- Ch3 §3.1 包含大量实现代码（struct 定义、方法实现、安全性考虑的 unsafe 方法），属于"如何"而非"为什么"
- Ch3 §3.2 包含完整的 link/unlink 方法实现，属于"如何"
- Ch3 §3.3 包含完整的 Slab 实现，属于"如何"

**修改后**：
- Ch3 §3.1 重写为以"为什么"为主线：为什么用 u64 而非 newtype、为什么用普通 u8 而非 Cell、为什么用 Option<*mut> 而非 NonNull、为什么用 bitflags 而非手动 newtype、为什么方法放在 PhysRegion 上、为什么用 saturating_add、为什么 release_ref 返回 bool
- Ch3 §3.2 重写为聚焦"为什么不用 Arc"的设计决策，移除实现代码
- Ch3 §3.3 重写为"为什么不用 Slab"的设计决策，说明当前使用 Box/Vec 的原因和未来集成 Slab 的可能
- Ch4 保留实现代码（"如何"），与实际 Rust 源码一致
