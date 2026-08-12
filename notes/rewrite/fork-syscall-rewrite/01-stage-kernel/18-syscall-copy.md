# 18-syscall-copy: 跨进程内存拷贝

> **分类**: 系统调用服务
> **C 源码**: `minix3/minix/kernel/system/do_copy.c` (91 行), `do_safecopy.c` (448 行), `do_umap.c` (39 行), `do_umap_remote.c` (122 行), `do_vumap.c` (131 行), `do_memset.c` (28 行), `do_safememset.c` (57 行)
> **Rust 实现**: `os/kernel/src/syscall_copy.rs` (1971 行)
> **覆盖**: VIRCOPY/PHYSCOPY 直接拷贝、SAFECOPYFROM/TO/VSAFECOPY grant 授权拷贝、UMAP/UMAP_REMOTE/VUMAP 地址映射、MEMSET/SAFEMEMSET 跨空间填充、Direct Map 替代 createpde 临时映射
> **前置**: [16-smp.md](16-smp.md)（BKL 保证 safecopy 跨 CPU 安全）, [17-syscall-process.md](17-syscall-process.md)（fork/exec 使用 vircopy 拷贝进程上下文）, [24-cross-space-runtime.md](24-cross-space-runtime.md)（VMSUSPEND 协议）

---

## 1. 概念建构

**核心问题**: 内核如何在"高效拷贝"与"权限验证"之间取舍？

内核为系统服务（PM/VFS/RS/MEM/VM）提供跨地址空间拷贝原语，但调用者身份不同信任级别不同。Minix3 的回答是**按信任级别分层**：vircopy 信任调用者（系统进程直接拷贝，内核不验证权限），safecopy 不信任调用者（grant 表验证权限后才拷贝），umap 查询映射供 DMA，memset 跨空间填充。64 位下 Direct Map 用一行加法替代 32 位的 createpde 临时映射。

> **架构范围**：本章描述的拷贝原语语义跨三架构（x86_64/aarch64/riscv64）一致；地址翻译机制（Direct Map 基地址、PTE walk 层级）是架构特有，标注处注明。

### 1.1 vircopy/physcopy：直接拷贝（信任调用者）

**灵魂本质**: vircopy/physcopy 是信任调用者地址空间访问权的直接拷贝——内核不验证权限，仅替换 SELF、验证 endpoint、执行拷贝。

**WHY → WHAT → HOW 弧线**:

- **WHY**: 系统进程（PM/VFS/RS/MEM/VM）需要在彼此的地址空间之间拷贝数据（如 fork 时 PM 拷贝子进程上下文）。这些进程已被内核信任，逐字节验证权限开销过大。
- **WHAT**: vircopy 按虚拟地址拷贝，physcopy 按物理地址拷贝，两者共用同一 handler——区别仅在地址类型。内核仅做最小验证：SELF 替换、endpoint 有效性、溢出检查。
- **HOW**: C 用 `do_copy()` (do_copy.c:22-90) 解析消息 → SELF 替换 (do_copy.c:64-65) → isokendpt 验证 (do_copy.c:66-71) → 溢出检查 (do_copy.c:77) → `virtual_copy_vmcheck()` (do_copy.c:87-88)。

**关键约束**:

1. 仅系统进程可调用——权限由调用者身份保证，非内核验证
2. SELF 表示"调用者自身"，内核在验证前替换为 caller endpoint
3. NONE endpoint 跳过验证（物理地址拷贝，无进程上下文）
4. CP_FLAG_TRY 是 VFS 专用标志：拷贝失败返回 EFAULT 而非 VMSUSPEND（避免内存映射文件死锁，详见 do_copy.c:80-85）

### 1.2 safecopy：grant 表授权拷贝（不信任调用者）

**灵魂本质**: safecopy 是不信任调用者的 grant 表授权拷贝——verify_grant 验证 grant ID、序列号、间接链、权限、范围后才执行拷贝。

**WHY → WHAT → HOW 弧线**:

- **WHY**: 用户态进程通过系统服务器（如 VFS）请求跨空间拷贝。内核不信任用户态进程，需要一种机制让授权方（granter）声明"允许被授权方（grantee）访问我的某段内存"。
- **WHAT**: grant 表是 per-process 的授权表（`s_grant_table` + `s_grant_entries`），每个 grant 项（`cp_grant_t`）定义授权范围。三种 grant 类型：DIRECT（直接授权）、MAGIC（可重定向，仅 VFS/MIB）、INDIRECT（指向另一个 grant）。
- **HOW**: `verify_grant()` (do_safecopy.c:41-266) 是核心验证函数，按 11 步验证后返回实际地址与真实 granter。

**grant 验证 11 步流程**:

| 步 | 验证内容 | C 位置 |
|----|---------|--------|
| 1 | granter endpoint 有效性（isokendpt） | do_safecopy.c:60-70 |
| 2 | grant ID 有效性（GRANT_VALID） | do_safecopy.c:80-100 |
| 3 | grant 表存在（HASGRANTTABLE） | do_safecopy.c:28 |
| 4 | grant 索引在表范围内 | do_safecopy.c:105-110 |
| 5 | 从 granter 地址空间拷入 grant 项（data_copy） | do_safecopy.c:121-123 |
| 6 | flags 验证（CPF_USED \| CPF_VALID） | do_safecopy.c:130-135 |
| 7 | 序列号验证（防 ABA：grant 释放后重用） | do_safecopy.c:136-145 |
| 8 | 间接 grant 链（循环 + depth 计数，最多 5 层） | do_safecopy.c:148-173 |
| 9 | 访问权限（CPF_READ/CPF_WRITE） | do_safecopy.c:175-181 |
| 10 | 拷贝范围在 grant 范围内 | do_safecopy.c:202-212 |
| 11 | 返回实际虚拟地址 + 真实 granter（magic grant 可能重定向） | do_safecopy.c:215-216, 249-250 |

**CPF_TRY 软故障**: magic grant 可设置 CPF_TRY 标志，拷贝失败时向 grant 表写 faulted 标记（grant ID 含序列号，防 CPU 并发），返回 EFAULT 而非 VMSUSPEND (do_safecopy.c:258-263)。

### 1.3 umap/vumap：地址映射查询

**灵魂本质**: umap 将虚拟地址映射为物理地址供 DMA 使用；vumap 批量映射一组 grant/虚拟地址为物理地址向量。

**WHY → WHAT → HOW 弧线**:

- **WHY**: 驱动程序需要物理地址执行 DMA。用户态进程通过 grant 授权驱动访问其内存，但驱动需要物理地址而非虚拟地址。
- **WHAT**: umap 是 umap_remote 的子集（只允许自身地址空间和 grant）；vumap 批量映射，将虚拟地址向量转换为物理地址向量。
- **HOW**: `do_umap()` (do_umap.c:25-37) 安全检查后委托 `do_umap_remote()` (do_umap_remote.c:26-120)：endpoint 验证 → grantee 验证 → segment type 分发 → MEM_GRANT 走 verify_grant / VIR_ADDR 直接用 offset → `vm_lookup()` VA→PA (do_umap_remote.c:94) → 连续性检查 `vm_lookup_range()` (do_umap_remote.c:106)。`do_vumap()` (do_vumap.c:22-131) 拷入向量 → 逐元素 verify_grant/vm_lookup_range → 拷出物理向量。

**umap vs umap_remote**:

- umap 只允许映射自身地址空间（endpt==SELF）或 grant（seg_index==MEM_GRANT），其余返回 EPERM (do_umap.c:34)
- umap_remote 允许映射任意有效 endpoint 的地址空间

**vumap 的 DMA 用途**: 驱动程序用 vumap 将一组 grant 批量转换为物理地址向量，直接进行 DMA 操作，避免逐个映射的系统调用开销。

### 1.4 memset/safememset：跨地址空间填充

**灵魂本质**: memset 直接填充进程地址空间；safememset 先验证 grant(CPF_WRITE) 权限再填充。

**WHY → WHAT → HOW 弧线**:

- **WHY**: 内核或系统进程需要清零或填充另一个进程的内存（如 fork 时清零子进程的 BSS）。
- **WHAT**: memset 直接调用 `vm_memset()` 填充指定进程的地址空间；safememset 先验证 grant 的 CPF_WRITE 权限，再调用 `vm_memset()`。
- **HOW**: `do_memset()` (do_memset.c:17-25) 委托 `vm_memset(caller, process, base, pattern, count)`。`do_safememset()` (do_safememset.c:20-57) endpoint 验证 → grant 表检查 (do_safememset.c:36-45) → `verify_grant(CPF_WRITE)` (do_safememset.c:48-49) → `vm_memset()` (do_safememset.c:56)。pattern 是 int 类型，vm_memset 内部 `& 0xFF` 截断为字节。

### 1.5 Direct Map：64 位下一行加法替代 createpde 临时映射

> **架构范围**：Direct Map 是 64 位架构共性（x86_64/aarch64/riscv64 均可用），基地址由各架构 MMU 配置决定。

**灵魂本质**: Direct Map 在 64 位地址空间预留区域，PA+base=KV 一行加法替代 32 位 createpde 临时映射——跨空间拷贝等价于 memcpy。

**WHY → WHAT → HOW 弧线**:

- **WHY**: 32 位下内核地址空间有限，无法直接映射所有物理内存。跨空间拷贝需要 `createpde()` 临时建立 PDE 映射，拷贝完成后销毁——开销大且复杂。64 位地址空间足够大（48 位虚拟地址 = 256TB），可以预留一大块区域直接映射全部物理内存。
- **WHAT**: Direct Map 是 64 位地址空间中的固定区域，物理地址 PA 加上基地址即得到内核虚拟地址 KV。跨空间拷贝从"createpde + lin_lin_copy"两步简化为"PA→KV + memcpy"一步。
- **HOW**: C 32 位用 `createpde()` + `lin_lin_copy()`。Rust 64 位用 `DirectMapArch::kernel_phys_to_virt(pa)` 一行加法，然后 `core::ptr::copy_nonoverlapping` memcpy。

**Direct Map 的限制**:

1. 仅翻译 PA→KV（物理到内核虚拟），不翻译 VA→PA（用户虚拟到物理）
2. VA→PA 仍需 PTE walk（`vm_lookup`），每架构页表格式不同
3. 缺页时仍需 VMSUSPEND 协议（挂起源/目标进程，通知 VM 处理）

**Direct Map 演进表**（设计目标 + 当前状态）:

| C 机制 | 64 位 Direct Map 替代 | 当前状态 |
|--------|---------------------|---------|
| `createpde()` 临时映射 | `kernel_phys_to_virt(pa)` 一行加法 | ✅ 已实现（`os/kernel/src/syscall_copy.rs:411-468`） |
| `lin_lin_copy()` | `memcpy(kernel_phys_to_virt(src_pa), kernel_phys_to_virt(dst_pa), n)` | ✅ 已实现；跨进程 VA→PA 经 `cross_space.rs::data_copy_vmcheck` + PTE walk，dispatch 已接入 |
| `vm_memset()` (正常路径) | `memset(kernel_phys_to_virt(pa), pattern, n)` | ✅ 已实现；`cross_space::memset_vmcheck` 处理 VMSUSPEND，dispatch_memset 已接入 |
| `vm_lookup()` | 保留（仍需查询页表映射 VA→PA） | ✅ 已实现三架构（`minix_arch::CurrentPteWalk::walk`，trait 分发，无 `#[cfg(target_arch)]`）；`vm::lookup_in_table` 已接入 dispatch_umap_remote |
| `virtual_copy_vmcheck()` | 简化：VA→PA→Direct Map→memcpy，缺页时仍 VMSUSPEND | ✅ 已实现；`data_copy_vmcheck` 封装跨进程 PTE walk + Direct Map + VMSUSPEND，dispatch_copy 已接入 |

**本章小结**: 五类拷贝原语按信任级别分层——vircopy/physcopy 信任调用者、safecopy 不信任用 grant 表、umap/vumap 查询映射、memset/safememset 跨空间填充，Direct Map 在 64 位下统一简化了底层 PA→KV 翻译。后续章节按此分层展开 C 源码、设计决策与 Rust 实现。

---

## 2. C 源码分析

### 2.1 do_copy.c — VIRCOPY/PHYSCOPY

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_copy()` | do_copy.c:22-90 | 共用处理器：解析消息 → SELF 替换 → endpoint 验证 → 溢出检查 → virtual_copy_vmcheck |
| SELF 替换 | do_copy.c:64-65 | `if (vir_addr[i].proc_nr_e == SELF) vir_addr[i].proc_nr_e = caller->p_endpoint` |
| isokendpt 验证 | do_copy.c:66-71 | `if(! isokendpt(vir_addr[i].proc_nr_e, &p)) return(EINVAL)` |
| 溢出检查 | do_copy.c:77 | `if (bytes != (phys_bytes)(vir_bytes) bytes) return(E2BIG)`（32 位遗留，64 位下始终通过） |
| CP_FLAG_TRY | do_copy.c:80-85 | VFS 专用 try-copy：`assert(caller == VFS_PROC_NR)`; 失败返回 EFAULT |
| virtual_copy_vmcheck | do_copy.c:87-88 | 实际拷贝 + 缺页挂起 |

### 2.2 do_safecopy.c — SAFECOPYFROM/TO/VSAFECOPY

| 符号 | 位置 | 说明 |
|------|------|------|
| `MAX_INDIRECT_DEPTH` | do_safecopy.c:21 | 间接 grant 最大深度 = 5 |
| `cp_sfinfo` | do_safecopy.c:31-36 | 软故障信息（CPF_TRY 标志） |
| `verify_grant()` | do_safecopy.c:41-266 | grant 验证：endpoint → grant_idx → 序列号 → 间接链 → 权限 → 范围 |
| 间接链处理 | do_safecopy.c:148-173 | `if (depth == MAX_INDIRECT_DEPTH) return ELOOP`；`do { ... } while (CPF_INDIRECT)` 循环 |
| magic grant 重定向 | do_safecopy.c:217-250 | `*e_granter = g.cp_u.cp_magic.cp_who_from` |
| CPF_TRY 软故障 | do_safecopy.c:258-263 | 写 faulted 标记到 grant 表 |
| `safecopy()` | do_safecopy.c:271-372 | 验证 grant → 确定源/目标 → virtual_copy_vmcheck |
| `do_safecopy_to()` | do_safecopy.c:377-383 | CPF_WRITE 方向（caller→granter） |
| `do_safecopy_from()` | do_safecopy.c:388-394 | CPF_READ 方向（granter→caller） |
| `do_vsafecopy()` | do_safecopy.c:399-447 | 批量：拷入向量 → 逐元素 SELF 方向解析 → 逐元素 safecopy |

### 2.3 do_umap.c + do_umap_remote.c — UMAP/UMAP_REMOTE

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_umap()` | do_umap.c:25-37 | 安全检查（seg_index != MEM_GRANT && endpt != SELF → EPERM）→ 委托 do_umap_remote |
| `do_umap_remote()` | do_umap_remote.c:26-120 | endpoint 验证 → grantee 验证 → segment type 分发 → vm_lookup → 连续性检查 |
| SELF 替换 | do_umap_remote.c:40-41 | `if (endpt == SELF) okendpt(caller->p_endpoint, &proc_nr)` |
| grantee 验证 | do_umap_remote.c:48-55 | SELF→caller；NONE/ANY/非 MEM_GRANT/无效 → EINVAL |
| MEM_GRANT 路径 | do_umap_remote.c:60-82 | verify_grant → newoffset/newep → 重新 lookup |
| VIR_ADDR 路径 | do_umap_remote.c:84-85 | `phys_addr = lin_addr = offset` |
| vm_lookup | do_umap_remote.c:94 | VA→PA 翻译 |
| 连续性检查 | do_umap_remote.c:106-109 | `vm_lookup_range(targetpr, lin_addr, NULL, count) != count → EFAULT` |

### 2.4 do_vumap.c — VUMAP

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_vumap()` | do_vumap.c:22-131 | 拷入向量 → access 转换 → 逐元素 verify_grant/vm_lookup_range → 拷出物理向量 |
| `vvec`/`pvec` 栈数组 | do_vumap.c:30-31 | `struct vumap_vir vvec[MAPVEC_NR]` / `struct vumap_phys pvec[MAPVEC_NR]`，编译期分配在栈 |
| vcount/pmax 边界 | do_vumap.c:48-52 | `<= 0` → EINVAL；`> MAPVEC_NR` 截断到 MAPVEC_NR |
| access 转换 | do_vumap.c:54-60 | VUA_READ→CPF_READ; VUA_WRITE→CPF_WRITE; VUA_READ\|VUA_WRITE→CPF_READ\|CPF_WRITE; default→EINVAL |
| 逐元素映射 | do_vumap.c:73-118 | 每个虚拟范围可能映射到多个物理范围；循环填入物理向量 |
| 物理向量拷出 | do_vumap.c:120-128 | `data_copy_vmcheck(caller, KERNEL, pvec, endpt, paddr, size)` |

### 2.5 do_memset.c + do_safememset.c — MEMSET/SAFEMEMSET

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_memset()` | do_memset.c:17-25 | 委托 `vm_memset(caller, process, base, pattern, count)` |
| `do_safememset()` | do_safememset.c:20-57 | endpoint 验证 → grant 表检查 → verify_grant(CPF_WRITE) → vm_memset |
| endpoint 验证 | do_safememset.c:36-40 | `dst_endpt == NONE` → EFAULT；`!endpoint_lookup` → EINVAL |
| grant 表检查 | do_safememset.c:42-45 | `!(priv(dst_p) && priv(dst_p)->s_grant_table)` → EINVAL |

### 2.6 调用关系

**vircopy 拷贝时序**:

```
do_copy(caller, m_ptr)  [do_copy.c:22]
  ├─ SELF 替换 (src/dst)
  ├─ isokendpt 验证 (src/dst)
  ├─ 溢出检查
  ├─ if CP_FLAG_TRY:
  │   └─ virtual_copy() → EFAULT on fault (VFS 专用)
  └─ virtual_copy_vmcheck(caller, src, dst, bytes)
       └─ 缺页 → VMSUSPEND → VM 处理 → 恢复拷贝
```

**safecopy 拷贝时序**:

```
do_safecopy_from/to(caller, m_ptr)  [do_safecopy.c:388/377]
  └─ safecopy(caller, granter, grantee, grantid, bytes, g_offset, addr, access)  [do_safecopy.c:271]
       ├─ 确定 src/dst (CPF_READ: granter→grantee; CPF_WRITE: grantee→granter)
       ├─ verify_grant(granter, grantee, grantid, bytes, access, ...)  [do_safecopy.c:41]
       │    ├─ endpoint 验证
       │    ├─ grant ID 有效性
       │    ├─ grant 表范围
       │    ├─ data_copy 拷入 grant 项
       │    ├─ flags + 序列号验证
       │    ├─ if CPF_INDIRECT: 循环 follow (depth ≤ 5)
       │    ├─ 权限验证 (CPF_READ/CPF_WRITE)
       │    ├─ 范围验证
       │    └─ 返回 offset_result + e_granter + sfinfo
       ├─ granter = new_granter (magic grant 重定向)
       └─ if CPF_TRY: virtual_copy() + 软故障标记
          else: virtual_copy_vmcheck()
```

---

## 3. Rust 设计决策

> 每个决策采用"如果 X 设计会有 Y 问题所以用 Z"格式，不追溯迭代历史。

### D1. 跨地址空间拷贝：createpde 临时映射 vs Direct Map

如果用 `createpde()` 临时映射（C 32 位方式）：每次跨空间拷贝需建立 PDE 临时映射，拷贝完成后销毁——开销大；且 64 位下内核地址空间足够大，临时映射是不必要的复杂度。如果用 Direct Map：64 位地址空间预留区域，PA+base=KV 一行加法；跨空间拷贝等价于"VA→PA→Direct Map→memcpy"。所以用 Direct Map——一行加法替代 createpde，memcpy 替代 lin_lin_copy。实现：`DirectMapArch::kernel_phys_to_virt(pa)`。跨进程 VA→PA 的 PTE walk 由 `minix_arch::CurrentPteWalk::walk`（三架构 trait 分发）+ `vm::lookup_in_table` 提供，dispatch 层经 `cross_space::data_copy_vmcheck` 统一封装后已全路径接入（详见 §4.2-4.6）。

### D2. grant 表访问：data_copy 从用户空间读 vs 内核缓存

如果内核直接读用户空间 grant 表：需要 PTE walk 翻译 grant 表地址，可能缺页触发 VMSUSPEND——而 VMSUSPEND 处理本身可能需要读 grant 表，形成递归 VMSUSPEND。如果用 `data_copy()` 从 granter 地址空间拷入 grant 项到内核栈（C 方式，do_safecopy.c:121-123）——一次性拷入，后续验证在内核内存完成，避免递归。所以用 data_copy 拷入内核缓存。

### D3. verify_grant 返回值：多个输出参数 vs GrantVerifyResult 结构体

如果用多个输出参数（C 方式，do_safecopy.c:41-51 三个指针参数）：调用者需声明多个变量传指针，类型不安全，易传错。如果用结构体：`GrantVerifyResult { offset, granter, sfinfo }`——Rust 惯用法，类型安全，调用者直接解构。所以用 `GrantVerifyResult` 结构体（anti-translate）。

### D4. 间接 grant 链：递归 vs 循环 + depth

如果用递归：间接链可能形成环（虽有 depth 限制），递归有栈溢出风险；且 no_std 下栈空间有限。如果用循环 + depth 计数（C 方式，do_safecopy.c:148-173）：`do { ... if (depth == MAX_INDIRECT_DEPTH) return ELOOP; depth++; ... } while (CPF_INDIRECT)`——无栈溢出风险，与 C 一致。所以用循环 + depth 计数。

### D5. VUMAP 物理向量：栈数组 vs Vec

如果用 `Vec<VumapPhys>`：需动态分配，no_std 下需 `alloc` crate；且 DMA 路径应避免动态分配（性能 + 失败模式）。如果用栈数组 `[VumapPhys; MAPVEC_NR]`（C 方式，do_vumap.c:31）：编译期分配在栈，no_std 兼容，无动态分配失败。所以用栈数组（anti-translate）。

### D6. CP_FLAG_TRY / CPF_TRY：保留 vs 删除

如果删除 CP_FLAG_TRY / CPF_TRY：VFS 内存映射文件场景下，拷贝缺页会触发 VMSUSPEND，而 VMSUSPEND 处理可能需要文件系统回调——文件系统正在等待此次拷贝，形成死锁。如果保留：CPF_TRY 路径走 `virtual_copy()`（不触发 VMSUSPEND），失败返回 EFAULT；VFS 重试而非死锁。所以保留——VFS 依赖此语义避免死锁。

### D7. vm_memset / vm_lookup：保留 vs Direct Map 替代

如果用 Direct Map 完全替代 vm_memset/vm_lookup：Direct Map 只翻译 PA→KV，不翻译 VA→PA。用户空间虚拟地址仍需 PTE walk 获取物理地址。如果保留接口、实现简化：正常路径用 Direct Map，缺页时仍需 VMSUSPEND 协助换入页面。所以保留接口，实现简化——64 位 Direct Map 下大部分场景不需要 VM 协助，但缺页时仍需。

### D8. do_umap → do_umap_remote 委托：保留 vs 合并

如果保留 C 的 `#if USE_UMAP` / `#if USE_UMAP_REMOTE` 条件编译：Rust 不需要条件编译（所有系统调用总是编译），保留委托增加间接调用无益。如果合并为 `dispatch_umap`：安全检查（seg_index != MEM_GRANT && endpt != SELF → EPERM）内联到入口，然后直接调用实现——减少间接调用，Rust 惯用法。所以合并为 `dispatch_umap`。

### D9. safecopy 的 sfinfo：结构体 vs Option

如果 sfinfo 总是存在（C 方式，do_safecopy.c:288）：大部分场景不使用 CPF_TRY，sfinfo 字段无意义但始终占用栈空间。如果用 `Option<SoftFaultInfo>`：仅 CPF_TRY 场景构造 Some，其他场景 None；类型表达"可能不存在"语义。所以用 `Option<SoftFaultInfo>`（anti-translate）。

---

## 4. 实现详解

> 本章贴 `os/kernel/src/syscall_copy.rs` 真实代码，缺失路径诚实标注 DEFERRED + 理由。`#![no_std]` 约束：除 `#[cfg(test)]` 外不依赖 `std`/`alloc`。

### 4.1 核心类型

> 设计决策：§3 D3（GrantVerifyResult）、§3 D9（Option\<SoftFaultInfo\>）、§3 D3（SafecopyAccess）

```rust
// os/kernel/src/syscall_copy.rs:179-199
// grant 验证结果——D3 用结构体替代 C 三个输出指针参数
pub struct GrantVerifyResult {
    /// 验证后的偏移量（虚拟地址空间内）。C: *offset_result
    pub offset: u64,
    /// 真正的授权方 endpoint（magic grant 可能重定向）。C: *e_granter
    pub granter: Endpoint,
    /// 软故障信息（仅 CPF_TRY 场景）。D9: Option 替代 C 总是存在的 cp_sfinfo
    pub sfinfo: Option<SoftFaultInfo>,
}

// 软故障信息——C: struct cp_sfinfo (do_safecopy.c:31-36)
#[derive(Debug, Clone)]
pub struct SoftFaultInfo {
    pub endpoint: Endpoint,
    pub addr: u64,
    pub value: i32,
}
```

`Endpoint` 是 newtype（`pub struct Endpoint(pub i32)`），编译期防止与其他 `i32`（如 errno、grant_id）混淆——C 中 `endpoint_t` 是裸 `int`，易与其他整数混传。

```rust
// os/kernel/src/syscall_copy.rs:202-218
// safecopy 访问方向——D3 用 enum 替代 C 的 CPF_READ/CPF_WRITE 裸位
// safecopy() 的 access 参数只接受单方向（CPF_READ 或 CPF_WRITE），不接受位组合
pub enum SafecopyAccess {
    Read,   // 从授权方读取。C: CPF_READ
    Write,  // 向授权方写入。C: CPF_WRITE
}

impl SafecopyAccess {
    pub fn to_flags(self) -> u32 {
        match self {
            SafecopyAccess::Read => CPF_READ,
            SafecopyAccess::Write => CPF_WRITE,
        }
    }
}
```

```rust
// os/kernel/src/syscall_copy.rs:370-376
// virtual_copy_vmcheck 的错误变体
pub enum CopyError {
    Fault,   // 缺页（源/目标未映射）。C: VMSUSPEND 路径
    TooBig,  // nr_bytes 溢出。C: E2BIG (do_copy.c:77)
}
```

### 4.2 dispatch_copy — VIRCOPY/PHYSCOPY

> 设计决策：§3 D1（Direct Map）。对应 C: do_copy.c:22-90。

`dispatch_vircopy` 与 `dispatch_physcopy` 都是 `dispatch_copy` 的薄包装（C 中两者共用同一 handler）。dispatch 完整接入 `data_copy_vmcheck`（正常路径）+ `cross_space_copy`（CP_FLAG_TRY 路径，缺页返回 EFAULT 而非 VMSUSPEND）。

```rust
// os/kernel/src/syscall_copy.rs:301-363（节选关键路径）
fn dispatch_copy(caller: &mut KProcess, msg: &Message,
                 proc_table: &ProcessTable) -> KcallResult {
    let m = msg_copy(msg);
    let mut src_endpt = m.src_endpt;
    let mut dst_endpt = m.dst_endpt;

    // C: do_copy.c:64-65 — SELF 替换
    if src_endpt == SELF { src_endpt = caller.p_endpoint.0; }
    if dst_endpt == SELF { dst_endpt = caller.p_endpoint.0; }

    // C: do_copy.c:66-71 — endpoint 验证（NONE 跳过，物理地址拷贝）
    if src_endpt != NONE {
        if proc_table.endpoint_to_nr(Endpoint(src_endpt)).is_none() {
            return KcallResult::Ok(EINVAL);
        }
    }
    if dst_endpt != NONE {
        if proc_table.endpoint_to_nr(Endpoint(dst_endpt)).is_none() {
            return KcallResult::Ok(EINVAL);
        }
    }

    // C: do_copy.c:80-85 — CP_FLAG_TRY 分支：VFS 专用 try-copy
    // 走 cross_space_copy，缺页返回 EFAULT 而非 VMSUSPEND，避免 VFS
    // 内存映射文件死锁（已实现，syscall_copy.rs:340-360）
    if flags & CP_FLAG_TRY != 0 {
        return match cross_space_copy(...) {
            CrossSpaceResult::Completed(Ok(())) => KcallResult::Ok(OK),
            CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
            CrossSpaceResult::Suspended(_) => KcallResult::Ok(EFAULT),
        };
    }

    // C: do_copy.c:87-88 — virtual_copy_vmcheck（已接入 data_copy_vmcheck）
    match data_copy_vmcheck(caller, src, dst, nr_bytes, &proc_cr3) {
        CrossSpaceResult::Completed(Ok(())) => KcallResult::Ok(OK),
        CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
        CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
    }
}
```

### 4.3 safecopy_common_impl — SAFECOPYFROM/TO

> 设计决策：§3 D2（data_copy 拷入内核缓存）、§3 D3（GrantVerifyResult）。对应 C: do_safecopy.c:271-394。

`dispatch_safecopy_from`（CPF_READ）与 `dispatch_safecopy_to`（CPF_WRITE）都委托 `safecopy_common_impl`。验证 + `verify_grant` + `data_copy_vmcheck` 拷贝均已完整实现。

```rust
// os/kernel/src/syscall_copy.rs:543-585（节选验证 + grant 解析 + 拷贝）
fn safecopy_common_impl(caller: &mut KProcess, msg: &Message,
                        proc_table: &ProcessTable, access: u32) -> KcallResult {
    let m = msg_safecopy(msg);
    let granter = m.from_to;
    let grant_id = m.grant_id;

    // C: do_safecopy.c:284-286 — endpoint 验证
    if granter == NONE || caller.p_endpoint.0 == NONE {
        return KcallResult::Ok(EFAULT);
    }

    // C: do_safecopy.c:73-76 — granter 必须存在
    let _granter_nr = match proc_table.endpoint_to_nr(Endpoint(granter)) {
        Some(nr) => nr,
        None => return KcallResult::Ok(EINVAL),
    };

    // grant_id 为 i32，负值无效（-1 是 INVALID_GRANT 哨兵）
    if grant_id < 0 {
        return KcallResult::Ok(EINVAL);
    }

    // C: do_safecopy.c:305-312 — verify_grant 解析 grant → offset + 真实 granter
    let outcome = verify_grant(caller, Endpoint(granter), caller.p_endpoint,
                               grant_id, bytes, CpFlags::from(access),
                               g_offset, proc_table, priv_table, &proc_cr3);
    let result = match outcome {
        VerifyGrantOutcome::Ok(r) => r,
        VerifyGrantOutcome::Err(e) => return KcallResult::Ok(e),
        VerifyGrantOutcome::Suspended(_) => return KcallResult::VmSuspend,
    };

    // C: do_safecopy.c:321-372 — virtual_copy_vmcheck dispatch（已接入 data_copy_vmcheck）
    match data_copy_vmcheck(caller, src, dst, bytes, &proc_cr3) {
        CrossSpaceResult::Completed(Ok(())) => KcallResult::Ok(OK),
        CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
        CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
    }
}
```

### 4.4 dispatch_umap_remote_impl — UMAP/UMAP_REMOTE

> 设计决策：§3 D8（合并 dispatch_umap）。对应 C: do_umap_remote.c:26-120。

`dispatch_umap`（do_umap.c:25-37）合并了 C 的安全检查，然后委托 `dispatch_umap_remote_impl`。dispatch 已完整接入 `verify_grant` + `lookup_in_table` + `lookup_range_in_table`（连续性检查）+ 消息回填。

```rust
// os/kernel/src/syscall_copy.rs:772-836（节选）
fn dispatch_umap_remote_impl(caller: &mut KProcess, msg: &Message,
        grantee: i32, proc_table: &ProcessTable) -> KcallResult {
    let m = msg_umap(msg);
    let seg_type = m.segment & SEGMENT_TYPE_MASK;
    let seg_index = m.segment & SEGMENT_INDEX_MASK;

    // C: do_umap_remote.c:40-44 — endpoint 验证 + SELF 替换
    let target_endpoint = if endpt == SELF {
        caller.p_endpoint
    } else {
        // isokendpt 三检查：范围 + 槽占用 + 代际匹配
        if proc_table.endpoint_to_nr(Endpoint(endpt)).is_none() {
            return KcallResult::Ok(EINVAL);
        }
        Endpoint(endpt)
    };

    // C: do_umap_remote.c:48-55 — grantee 验证
    let grantee_endpoint = if grantee == SELF {
        caller.p_endpoint
    } else if grantee == NONE || grantee == ANY {
        return KcallResult::Ok(EINVAL);
    } else if seg_index != MEM_GRANT {
        return KcallResult::Ok(EINVAL);  // 非 SELF grantee 仅对 MEM_GRANT 有效
    } else { /* isokendpt */ };

    // C: do_umap_remote.c:58-104 — segment type 分发
    match seg_type {
        LOCAL_VM_SEG => {
            if seg_index != MEM_GRANT && seg_index != VIR_ADDR {
                return KcallResult::Ok(EFAULT);  // bogus seg_index
            }
            // MEM_GRANT 路径：verify_grant 解析 grant → newoffset/newep
            //   （syscall_copy.rs:856-895，对齐 do_umap_remote.c:60-82）
            // VIR_ADDR 路径：直接用 offset 作 lin_addr
            // C: do_umap_remote.c:94 — vm_lookup → 已接入 lookup_in_table
            let phys_addr = match lookup_in_table::<minix_arch::CurrentDirectMap>(
                proc_table, target_endpoint, lin_addr) {
                Some((pa, _)) => pa,
                None => return KcallResult::Ok(EFAULT),
            };
            // C: do_umap_remote.c:106-109 — 连续性检查 → 已接入 lookup_range_in_table
            match lookup_range_in_table::<minix_arch::CurrentDirectMap>(
                proc_table, target_endpoint, lin_addr, count) {
                n if n >= count => { /* 物理地址连续 */ },
                _ => return KcallResult::Ok(EFAULT),
            }
            // 回填物理地址到 reply message
            KcallResult::Ok(OK)
        }
        _ => KcallResult::Ok(EINVAL),
    }
}
```

### 4.5 dispatch_vumap — VUMAP

> 设计决策：§3 D5（栈数组）。对应 C: do_vumap.c:22-131。

验证层 + 向量拷入（`data_copy_vmcheck`）+ 逐元素 `verify_grant`/`lookup_range_in_table` + 物理向量拷出均已完整实现。

```rust
// os/kernel/src/syscall_copy.rs:875-937（节选验证层 + 向量拷入 + 逐元素映射）
pub fn dispatch_vumap(caller: &mut KProcess, msg: &Message,
                      proc_table: &ProcessTable) -> KcallResult {
    let m = msg_vumap(msg);
    let vcount = m.vcount;
    let pmax = m.pmax;

    // C: do_vumap.c:43 — caller 必须有有效 endpoint
    if caller.p_endpoint.0 == NONE { return KcallResult::Ok(EFAULT); }

    // C: do_vumap.c:48-52 — vcount/pmax 边界 + MAPVEC_NR 截断
    if vcount <= 0 || pmax <= 0 { return KcallResult::Ok(EINVAL); }
    let vcount = if (vcount as usize) > MAPVEC_NR { MAPVEC_NR as i32 } else { vcount };

    // C: do_vumap.c:54-60 — access 转换
    let _access = match access_raw {
        VUA_READ => CPF_READ,
        VUA_WRITE => CPF_WRITE,
        x if x == (VUA_READ | VUA_WRITE) => CPF_READ | CPF_WRITE,
        _ => return KcallResult::Ok(EINVAL),
    };

    // C: do_vumap.c:79 — source != SELF 时 granter 必须存在
    if source != SELF && source != NONE
        && proc_table.endpoint_to_nr(Endpoint(source)).is_none() {
        return KcallResult::Ok(EINVAL);
    }

    // C: do_vumap.c:67-70 — data_copy_vmcheck 拷入 vvec（已实现，syscall_copy.rs:660）
    let copy_result = crate::cross_space::data_copy_vmcheck(
        caller, src, dst, vcount * size_of::<VumapVir>(), &proc_cr3);
    match copy_result { /* Completed(Ok) | Suspended → VmSuspend */ }

    // C: do_vumap.c:73-118 — 逐元素映射
    //   source != SELF → verify_grant 解析 grant → vir_addr + granter（已实现）
    //   内层 while: lookup_range_in_table 填 pvec[pcount]（已实现）
    // C: do_vumap.c:120-128 — data_copy_vmcheck 拷出 pvec（已实现）
    KcallResult::Ok(OK)
}
```

> `MAPVEC_NR` 当前值为 64（`os/kernel/src/syscall_copy.rs:164`），与 C 栈数组上限对齐。`VumapPhys` 结构体（D5 栈数组元素类型）已落地，作为 `lookup_range_in_table` 输出的物理范围载体。

### 4.6 dispatch_memset / dispatch_safememset — MEMSET/SAFEMEMSET

> 设计决策：§3 D7（保留 vm_memset 接口）。对应 C: do_memset.c:17-25, do_safememset.c:20-57。

```rust
// os/kernel/src/syscall_copy.rs:974-1013（dispatch_memset 节选）
pub fn dispatch_memset(caller: &mut KProcess, msg: &Message,
                       proc_table: &ProcessTable) -> KcallResult {
    let m = msg_memset(msg);
    let process = m.process;
    let pattern = m.pattern;

    // C: vm_memset:531-533 — caller 必须有效
    if caller.p_endpoint.0 == NONE { return KcallResult::Ok(EFAULT); }

    // C: vm_memset:537-539 — process != NONE 时必须存在，否则 ESRCH
    if process != NONE {
        if proc_table.endpoint_to_nr(Endpoint(process)).is_none() {
            return KcallResult::Ok(ESRCH);
        }
    }

    // C: vm_memset:541 — pattern & 0xFF 截断为字节
    let _pattern_byte = pattern & 0xFF;

    // C: vm_memset 主体 — 已接入 cross_space::memset_vmcheck
    //   （Direct Map + PTE walk + VMSUSPEND，syscall_copy.rs:1273+）
    match crate::cross_space::memset_vmcheck(caller, dst, pattern_byte, count, &proc_cr3) {
        CrossSpaceResult::Completed(Ok(())) => KcallResult::Ok(OK),
        CrossSpaceResult::Completed(Err(_)) => KcallResult::Ok(EFAULT),
        CrossSpaceResult::Suspended(_) => KcallResult::VmSuspend,
    }
}
```

`dispatch_safememset` 额外检查 grant 表：通过 `PrivTable::get(priv_id)` 取 `KPriv`，检查 `runtime.s_grant_table != 0`（对应 C `priv(dst_p)->s_grant_table`），随后调用 `verify_grant(CPF_WRITE)` + `memset_vmcheck` 完成填充（均已实现）。

### 4.7 virtual_copy_vmcheck — Direct Map primitive

> 设计决策：§3 D1（Direct Map）。对应 C: virtual_copy_vmcheck (memory.c)。

这是 Direct Map 的 primitive——步骤 3-4（PA→KV + memcpy）已实现，步骤 1-2（跨进程 VA→PA PTE walk）由 `minix_arch::CurrentPteWalk::walk` 三架构 trait 分发 + `vm::lookup_in_table` 提供，`cross_space.rs::data_copy_vmcheck` 将两者统一封装（跨进程 PTE walk + Direct Map + VMSUSPEND）。dispatch 层（dispatch_copy / dispatch_safecopy / dispatch_vsafecopy / dispatch_vumap / dispatch_memset / dispatch_safememset）均已通过 `data_copy_vmcheck` 接入此 primitive。

```rust
// os/kernel/src/syscall_copy.rs:411-468（节选）
pub fn virtual_copy_vmcheck(src_addr: VirBytes, dst_addr: VirBytes,
                            nr_bytes: u64) -> Result<(), CopyError> {
    use minix_arch::direct_map::DirectMapArch;

    if nr_bytes == 0 { return Ok(()); }  // 零字节拷贝是 no-op

    // 溢出检查：src_addr + nr_bytes 不能回绕
    let src_end = src_addr.0.checked_add(nr_bytes).ok_or(CopyError::TooBig)?;

    // 必须落在 Direct Map 区域内（kernel_phys_to_virt 翻译的范围）
    let kmap_base = minix_arch::CurrentDirectMap::KERNEL_DIRECT_MAP_BASE;
    if src_addr.0 < kmap_base || src_end < kmap_base {
        return Err(CopyError::Fault);  // 低于 Direct Map 区域需 PTE walk
    }

    // Direct Map memcpy——仅当物理页不别名时安全
    unsafe {
        core::ptr::copy_nonoverlapping(
            src_addr.0 as *const u8,
            dst_addr.0 as *mut u8,
            nr_bytes as usize,
        );
    }
    Ok(())
}
```

`DirectMapArch` 是跨架构统一抽象 trait（`os/arch/src/direct_map.rs`）：x86_64 基地址 `0xFFFF_8000_0000_0000`，aarch64/riscv64 由 MMU 配置。内核代码无 `#[cfg(target_arch)]` 行为选择。

### 4.8 实现完成状态（原 DEFERRED 项已全部落地）

以下函数在 C 源码中存在，此前标注为 DEFERRED，现已全部实现：

| 函数 | C 位置 | 实现状态 |
|------|--------|---------|
| `verify_grant` 完整实现 | do_safecopy.c:41-266 | ✅ 已实现（`grant.rs`，11 步验证 + `VerifyGrantOutcome` 三态返回） |
| `verify_grant` 间接链循环 | do_safecopy.c:148-173 | ✅ 已实现（循环 + depth ≤ MAX_INDIRECT_DEPTH=5，D4） |
| `verify_grant` magic 重定向 | do_safecopy.c:217-250 | ✅ 已实现（`effective_granter` 重定向） |
| `CPF_TRY` 软故障标记 | do_safecopy.c:258-263, 336-369 | ✅ 已实现（`Option<SoftFaultInfo>`，D9） |
| `CP_FLAG_TRY` try-copy 路径 | do_copy.c:80-85 | ✅ 已实现（`cross_space_copy` 返回 EFAULT 而非 VmSuspend） |
| `virtual_copy_vmcheck` 跨进程 PTE walk | memory.c | ✅ 已实现（`CurrentPteWalk::walk` + `data_copy_vmcheck`） |
| `vm_lookup` | do_umap_remote.c:94 | ✅ 已实现（`vm::lookup_in_table`，接入 dispatch_umap_remote） |
| `vm_lookup_range` 连续性检查 | do_umap_remote.c:106-109 | ✅ 已实现（`vm::lookup_range_in_table`，接入 dispatch_umap_remote + dispatch_vumap） |
| `vm_memset` | do_memset.c:20 | ✅ 已实现（`cross_space::memset_vmcheck`，接入 dispatch_memset） |
| `do_vsafecopy` 向量拷入 + 逐元素循环 | do_safecopy.c:399-447 | ✅ 已实现（`data_copy_vmcheck` 拷入 + 逐元素 `verify_grant`） |
| `do_vumap` 物理向量拷出 | do_vumap.c:120-128 | ✅ 已实现（`data_copy_vmcheck` 拷出 pvec） |
| VMSUSPEND 协议 | memory.c + VM 服务器 | ✅ 已接入（`data_copy_vmcheck` 返回 `VmSuspend`，由上层处理 VM round-trip） |

**基础设施总览**: `minix_arch::PteWalkArch` trait（`os/arch/src/arch/pte_walk_arch.rs`）三架构实现（x86_64 4-level / aarch64 4-level / riscv64 Sv39 3-level），通过 `CurrentPteWalk::walk` 类型别名 trait 分发，内核代码无 `#[cfg(target_arch)]` 行为选择。`vm::lookup_in_table`/`lookup_range_in_table`、`cross_space::data_copy_vmcheck`/`memset_vmcheck`、`grant::verify_grant`（`VerifyGrantOutcome` Ok/Err/Suspended 三态）均已就绪，7 个 dispatch 函数全部接入真实拷贝/映射路径（详见文件头注释 `syscall_copy.rs:44-75`）。

> **redox 对比**: redox 用 `paging::map_physical` 临时映射 + `copy_to_user`/`copy_from_user` 安全函数做跨空间拷贝，无 grant 机制（用 capability 模型）。minix-rs 对齐 C grant 表语义，Direct Map 一行加法比 redox 的临时映射更高效；PTE walk 落地后已提供与 redox `copy_to_user` 等效的安全封装（`pte_walk.rs::copy_from_user`/`copy_to_user`），dispatch 层已全路径启用 `data_copy_vmcheck`。

---

## 5. 测试

> 所有测试函数名可 grep 验证：`rg "fn test_" os/kernel/src/syscall_copy.rs --type rust -n`

### 5.1 现有测试（55 个，已实现）

**常量与布局**（8 个）:

- `test_safecopy_access_flags` — SafecopyAccess::Read/Write → CPF_READ/CPF_WRITE
- `test_umap_security_check` — UMAP 安全检查逻辑
- `test_grant_constants_match_c` — CPF_* 常量值与 C 一致
- `test_max_indirect_depth` — MAX_INDIRECT_DEPTH == 5
- `test_segment_constants_match_c` — SEGMENT_TYPE/INDEX/LOCAL_VM_SEG/MEM_GRANT/VIR_ADDR
- `test_mess_lsys_krn_sys_copy_layout` — 消息布局 ≤ 56 字节
- `test_mess_lsys_krn_sys_umap_layout` — 消息布局 ≤ 56 字节
- `test_vscp_vec_struct_size_matches_c_layout` — vscp_vec 结构体大小与 C 一致

**virtual_copy_vmcheck**（4 个）:

- `test_virtual_copy_vmcheck_zero_bytes_is_noop` — 零字节拷贝是 no-op
- `test_virtual_copy_vmcheck_overflow_returns_too_big` — u64::MAX 返回 TooBig
- `test_virtual_copy_vmcheck_below_kmap_returns_fault` — 低于 Direct Map 区域返回 Fault
- `test_copy_error_variants_match_minix3` — CopyError 变体一致性

**dispatch_copy**（4 个）:

- `test_dispatch_copy_rejects_invalid_src_endpoint` — 无效 src endpoint → EINVAL
- `test_dispatch_copy_rejects_invalid_dst_endpoint` — 无效 dst endpoint → EINVAL
- `test_dispatch_copy_accepts_none_endpoint` — NONE endpoint 不返回 EINVAL
- `test_dispatch_copy_self_replacement_and_valid_endpoint` — SELF 替换 + 有效 endpoint

**dispatch_umap_remote**（9 个）:

- `test_dispatch_umap_remote_self_endpoint_valid` — SELF endpoint 有效
- `test_dispatch_umap_remote_invalid_src_endpoint` — 无效 src → EINVAL
- `test_dispatch_umap_remote_valid_src_endpoint` — 有效 src
- `test_dispatch_umap_remote_grantee_none_rejected` — grantee=NONE → EINVAL
- `test_dispatch_umap_remote_grantee_any_rejected` — grantee=ANY → EINVAL
- `test_dispatch_umap_remote_grantee_invalid_endpoint_rejected` — 无效 grantee → EINVAL
- `test_dispatch_umap_remote_grantee_valid_for_grant_segment` — grant 段有效 grantee
- `test_dispatch_umap_remote_invalid_segment_type` — 无效 segment type → EINVAL
- `test_dispatch_umap_remote_bogus_seg_index_for_vm_seg` — VM 段无效 seg_index

**dispatch_umap**（1 个）:

- `test_dispatch_umap_rejects_non_self_non_grant` — 非 SELF 非 grant → EPERM

**dispatch_safememset**（4 个）:

- `test_dispatch_safememset_rejects_none_dst` — NONE dst → EFAULT
- `test_dispatch_safememset_rejects_invalid_dst_endpoint` — 无效 dst → EINVAL
- `test_dispatch_safememset_rejects_dst_without_grant_table` — 无 grant 表 → EINVAL
- `test_dispatch_safememset_valid_setup_returns_ok` — 有效配置 → OK

**dispatch_safecopy_from**（4 个）:

- `test_dispatch_safecopy_from_rejects_none_granter` — NONE granter → EFAULT
- `test_dispatch_safecopy_from_rejects_invalid_granter` — 无效 granter → EINVAL
- `test_dispatch_safecopy_from_rejects_negative_grant_id` — 负 grant ID → EINVAL
- `test_dispatch_safecopy_from_valid_setup_returns_ok` — 有效配置 → OK

**dispatch_safecopy_to**（4 个）:

- `test_dispatch_safecopy_to_rejects_none_granter` — NONE granter → EFAULT
- `test_dispatch_safecopy_to_rejects_invalid_granter` — 无效 granter → EINVAL
- `test_dispatch_safecopy_to_rejects_negative_grant_id` — 负 grant ID → EINVAL
- `test_dispatch_safecopy_to_valid_setup_returns_ok` — 有效配置 → OK

**dispatch_memset**（4 个）:

- `test_dispatch_memset_rejects_invalid_process` — 无效 process → ESRCH
- `test_dispatch_memset_valid_process_returns_ok` — 有效 process → OK
- `test_dispatch_memset_physical_address_returns_ok` — 物理地址（process=NONE）→ OK
- `test_dispatch_memset_pattern_truncation_logic` — pattern & 0xFF 截断

**dispatch_vsafecopy**（5 个）:

- `test_dispatch_vsafecopy_rejects_none_caller` — NONE caller → EFAULT
- `test_dispatch_vsafecopy_rejects_zero_vec_size` — 零向量 → EINVAL
- `test_dispatch_vsafecopy_rejects_negative_vec_size` — 负向量 → EINVAL
- `test_dispatch_vsafecopy_rejects_overflow_vec_size` — 溢出向量 → EINVAL
- `test_dispatch_vsafecopy_valid_setup_returns_ok` — 有效配置 → OK

**dispatch_vumap**（8 个）:

- `test_dispatch_vumap_rejects_none_caller` — NONE caller → EFAULT
- `test_dispatch_vumap_rejects_zero_vcount` — 零 vcount → EINVAL
- `test_dispatch_vumap_rejects_zero_pmax` — 零 pmax → EINVAL
- `test_dispatch_vumap_rejects_unknown_access` — 未知 access → EINVAL
- `test_dispatch_vumap_rejects_invalid_source_endpoint` — 无效 source → EINVAL
- `test_dispatch_vumap_self_source_returns_ok` — SELF source → OK
- `test_dispatch_vumap_valid_grant_source_returns_ok` — 有效 grant source → OK
- `test_dispatch_vumap_clamps_oversize_vcount` — 超大 vcount 截断到 MAPVEC_NR

### 5.2 待补充测试（端到端 / 集成测试）

原 DEFERRED 函数（verify_grant / vm_lookup / vm_memset / 跨进程 PTE walk）现已实现，下列测试转为端到端 / 集成测试目标（需 QEMU 或 mock grant 表构造真实跨进程场景）：

| 测试函数 | 验证行为 | 依赖 |
|---------|---------|------|
| `test_verify_grant_rejects_invalid_granter` | 无效 granter endpoint → EINVAL | verify_grant（已实现，需构造 mock grant 表） |
| `test_verify_grant_rejects_invalid_grant_id` | 无效 grant ID → EINVAL | verify_grant（已实现） |
| `test_verify_grant_indirect_chain_depth` | 间接链超过 5 层 → ELOOP | verify_grant 间接链（已实现） |
| `test_verify_grant_magic_redirect` | magic grant 重定向 granter | verify_grant magic（已实现） |
| `test_verify_grant_range_exceeded` | 超出 grant 范围 → EPERM | verify_grant 范围检查（已实现） |
| `test_virtual_copy_vmcheck_cross_process` | 跨进程 VA→PA→Direct Map→memcpy | data_copy_vmcheck（已接入 dispatch） |
| `test_vm_lookup_returns_phys_addr` | VA→PA 翻译 | lookup_in_table（已接入 dispatch） |
| `test_vm_memset_fills_pattern` | 跨空间填充字节模式 | memset_vmcheck（已接入 dispatch） |

> **测试统计**（截至 2026-08-01）：`os/kernel/src/syscall_copy.rs` 含 55 个 `fn test_*`，覆盖验证层与 Direct Map primitive。PTE walk 基础设施（`PteWalkArch` trait 三架构实现）已落地，`pte_walk.rs` 与 `cross_space.rs` 层已有测试覆盖。原 DEFERRED 的拷贝/映射层（verify_grant / vm_lookup dispatch / vm_memset dispatch）已全部实现并接入 dispatch，上表 8 个测试转为端到端集成测试目标（需 QEMU + mock grant 表构造跨进程场景）。

---

## 6. 参见

- [16-smp.md](16-smp.md) — BKL 保证 safecopy 跨 CPU 安全；VMSUSPEND 路径需在挂起前释放 BKL、恢复后重获 BKL
- [17-syscall-process.md](17-syscall-process.md) — fork/exec 使用 vircopy 拷贝进程上下文
- [20-syscall-device.md](20-syscall-device.md) — VMCTL 子命令（不属本文档）
- [24-cross-space-runtime.md](24-cross-space-runtime.md) — VMSUSPEND 协议和 vm_memset/vm_lookup 运行时
