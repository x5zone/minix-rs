# 18-syscall-copy-outline.md — 文档结构契约

> **文档**: `notes/rewrite/fork-syscall-rewrite/03-stage-kernel/18-syscall-copy.md`
> **C 源码**: `minix3/minix/kernel/system/do_copy.c` (91 行), `do_safecopy.c` (448 行), `do_umap.c` (39 行), `do_umap_remote.c` (122 行), `do_vumap.c` (131 行), `do_memset.c` (28 行), `do_safememset.c` (57 行)
> **Rust 实现**: `os/kernel/src/syscall_copy.rs` (1971 行)
> **创建**: 2026-08-01
> **依据**: `18-syscall-copy-glm-structure.md`（知识点全集 + 诊断）
> **方法**: C 源码 → OS 理论 → Rust 对照（非反向）

---

## 一、章节骨架与主语

### Ch1 主语：内存/安全（"内核如何安全跨地址空间拷贝数据？"）

核心问题：**内核如何在"高效拷贝"与"权限验证"之间取舍？**

Minix3 的回答：**按信任级别分层**——vircopy 信任调用者（系统进程直接拷贝），safecopy 不信任调用者（grant 表验证权限），umap 查询映射（供 DMA），memset 跨空间填充。64 位下 Direct Map 用一行加法替代 createpde 临时映射。

| 节 | 标题 | 灵魂本质（一句话） | 概念组 |
|----|------|-------------------|--------|
| §1.1 | vircopy/physcopy：直接拷贝（信任调用者） | "vircopy/physcopy 是信任调用者地址空间访问权的直接拷贝——内核不验证权限，仅替换 SELF、验证 endpoint、执行拷贝" | A |
| §1.2 | safecopy：grant 表授权拷贝（不信任调用者） | "safecopy 是不信任调用者的 grant 表授权拷贝——verify_grant 验证 grant ID、序列号、间接链、权限、范围后才执行拷贝" | B |
| §1.3 | umap/vumap：地址映射查询 | "umap 将虚拟地址映射为物理地址供 DMA 使用；vumap 批量映射一组 grant/虚拟地址为物理地址向量" | C, D |
| §1.4 | memset/safememset：跨地址空间填充 | "memset 直接填充进程地址空间；safememset 先验证 grant(CPF_WRITE) 权限再填充" | E |
| §1.5 | Direct Map：64 位下一行加法替代 createpde 临时映射 | "Direct Map 在 64 位地址空间预留区域，PA+base=KV 一行加法替代 32 位 createpde 临时映射——跨空间拷贝等价于 memcpy" | F, G |

### Ch2 主语：C 源码符号（file:line 锚定）

每节以 C 符号为单元，附 file:line，说明语义与调用关系。

### Ch3 主语：设计决策（hypothesis-driven）

采用"如果 X 设计会有 Y 问题所以用 Z"格式，禁止"旧版/最初/后来/我们改成"迭代叙事。

### Ch4 主语：Rust 实现（真实代码，非 stub）

贴 syscall_copy.rs 真实代码片段，标注 file:line。缺失函数诚实标注 DEFERRED + 理由。

### Ch5 主语：测试函数（可 grep 验证）

列出实际 `fn test_*` 函数名，每个测试对应一个被测行为。

---

## 二、详细大纲

### Ch1. 概念建构（concept-driven）

#### §1.1 vircopy/physcopy：直接拷贝（信任调用者）

**灵魂本质**: vircopy/physcopy 是信任调用者地址空间访问权的直接拷贝——内核不验证权限，仅替换 SELF、验证 endpoint、执行拷贝。

**WHY → WHAT → HOW 弧线**:
- **WHY**: 系统进程（PM/VFS/RS/MEM/VM）需要在彼此的地址空间之间拷贝数据（如 fork 时 PM 拷贝子进程上下文）。这些进程已被内核信任，逐字节验证权限开销过大。
- **WHAT**: vircopy 按虚拟地址拷贝，physcopy 按物理地址拷贝，两者共用 `do_copy()` handler——区别仅在地址类型。内核仅做最小验证：SELF 替换、endpoint 有效性、溢出检查。
- **HOW**: C 用 `do_copy()` (do_copy.c:22-90) 解析消息 → SELF 替换 (do_copy.c:64-65) → isokendpt 验证 (do_copy.c:66-71) → 溢出检查 (do_copy.c:77) → `virtual_copy_vmcheck()` (do_copy.c:87-88)。

**关键约束**:
1. 仅系统进程可调用（权限由调用者身份保证，非内核验证）
2. SELF 表示"调用者自身"，内核在验证前替换为 caller endpoint
3. NONE endpoint 跳过验证（物理地址拷贝，无进程上下文）
4. CP_FLAG_TRY 是 VFS 专用标志：拷贝失败返回 EFAULT 而非 VMSUSPEND（避免内存映射文件死锁）

#### §1.2 safecopy：grant 表授权拷贝（不信任调用者）

**灵魂本质**: safecopy 是不信任调用者的 grant 表授权拷贝——verify_grant 验证 grant ID、序列号、间接链、权限、范围后才执行拷贝。

**WHY → WHAT → HOW 弧线**:
- **WHY**: 用户态进程通过系统服务器（如 VFS）请求跨空间拷贝。内核不信任用户态进程，需要一种机制让授权方（granter）声明"允许被授权方（grantee）访问我的某段内存"。
- **WHAT**: grant 表是 per-process 的授权表（`s_grant_table` + `s_grant_entries`），每个 grant 项（`cp_grant_t`）定义授权范围。三种 grant 类型：DIRECT（直接授权）、MAGIC（可重定向，仅 VFS/MIB）、INDIRECT（指向另一个 grant）。
- **HOW**: `verify_grant()` (do_safecopy.c:41-266) 是核心验证函数：endpoint 验证 → grant ID 有效性 → grant 表范围 → flags/序列号验证 → 间接链处理（MAX_INDIRECT_DEPTH=5）→ 权限验证（CPF_READ/CPF_WRITE）→ 范围验证 → 返回实际地址+真实 granter。

**grant 验证流程**:
1. 验证 granter endpoint 有效性（isokendpt）
2. 验证 grant ID 有效性（GRANT_VALID）
3. 验证 grant 表存在（HASGRANTTABLE）
4. 验证 grant 索引在表范围内
5. 从 granter 地址空间拷入 grant 项（data_copy）
6. 验证 flags（CPF_USED | CPF_VALID）
7. 验证序列号（防 ABA：grant 释放后重用）
8. 处理间接 grant 链（循环 + depth 计数，最多 5 层）
9. 验证访问权限（CPF_READ/CPF_WRITE）
10. 验证拷贝范围在 grant 范围内
11. 返回实际虚拟地址 + 真实 granter endpoint（magic grant 可能重定向）

**CPF_TRY 软故障**: magic grant 可设置 CPF_TRY 标志，拷贝失败时向 grant 表写 faulted 标记（grant ID 含序列号，防 CPU 并发），返回 EFAULT 而非 VMSUSPEND。

#### §1.3 umap/vumap：地址映射查询

**灵魂本质**: umap 将虚拟地址映射为物理地址供 DMA 使用；vumap 批量映射一组 grant/虚拟地址为物理地址向量。

**WHY → WHAT → HOW 弧线**:
- **WHY**: 驱动程序需要物理地址执行 DMA。用户态进程通过 grant 授权驱动访问其内存，但驱动需要物理地址而非虚拟地址。
- **WHAT**: umap 是 umap_remote 的子集（只允许自身地址空间和 grant）；vumap 批量映射，将虚拟地址向量转换为物理地址向量。
- **HOW**: `do_umap()` (do_umap.c:25-37) 安全检查后委托 `do_umap_remote()` (do_umap_remote.c:26-120)：endpoint 验证 → grantee 验证 → segment type 分发 → MEM_GRANT 走 verify_grant / VIR_ADDR 直接用 offset → `vm_lookup()` VA→PA → 连续性检查 `vm_lookup_range()`。`do_vumap()` (do_vumap.c:22-131) 拷入向量 → 逐元素 verify_grant/vm_lookup_range → 拷出物理向量。

**umap vs umap_remote**:
- umap 只允许映射自身地址空间（endpt==SELF）或 grant（seg_index==MEM_GRANT）
- umap_remote 允许映射任意有效 endpoint 的地址空间

**vumap 的 DMA 用途**: 驱动程序用 vumap 将一组 grant 批量转换为物理地址向量，直接进行 DMA 操作，避免逐个映射的开销。

#### §1.4 memset/safememset：跨地址空间填充

**灵魂本质**: memset 直接填充进程地址空间；safememset 先验证 grant(CPF_WRITE) 权限再填充。

**WHY → WHAT → HOW 弧线**:
- **WHY**: 内核或系统进程需要清零或填充另一个进程的内存（如 fork 时清零子进程的 BSS）。
- **WHAT**: memset 直接调用 `vm_memset()` 填充指定进程的地址空间；safememset 先验证 grant 的 CPF_WRITE 权限，再调用 `vm_memset()`。
- **HOW**: `do_memset()` (do_memset.c:17-25) 委托 `vm_memset(caller, process, base, pattern, count)`。`do_safememset()` (do_safememset.c:20-57) endpoint 验证 → grant 表检查 → `verify_grant(CPF_WRITE)` → `vm_memset()`。pattern 是 int 类型，vm_memset 内部 `& 0xFF` 截断为字节。

#### §1.5 Direct Map：64 位下一行加法替代 createpde 临时映射

**灵魂本质**: Direct Map 在 64 位地址空间预留区域，PA+base=KV 一行加法替代 32 位 createpde 临时映射——跨空间拷贝等价于 memcpy。

**WHY → WHAT → HOW 弧线**:
- **WHY**: 32 位下内核地址空间有限，无法直接映射所有物理内存。跨空间拷贝需要 `createpde()` 临时建立 PDE 映射，拷贝完成后销毁——开销大且复杂。64 位地址空间足够大（48 位虚拟地址 = 256TB），可以预留一大块区域直接映射全部物理内存。
- **WHAT**: Direct Map 是 64 位地址空间中的固定区域（x86_64: `0xFFFF_8000_0000_0000` 起始），物理地址 PA 加上基地址即得到内核虚拟地址 KV。跨空间拷贝从"createpde + lin_lin_copy"两步简化为"PA→KV + memcpy"一步。
- **HOW**: C 32 位用 `createpde()` + `lin_lin_copy()` (memory.c)。Rust 64 位用 `DirectMapArch::kernel_phys_to_virt(pa)` (direct_map.rs:44-46) 一行加法，然后 `core::ptr::copy_nonoverlapping` memcpy。

**Direct Map 的限制**:
1. 仅翻译 PA→KV（物理到内核虚拟），不翻译 VA→PA（用户虚拟到物理）
2. VA→PA 仍需 PTE walk（`vm_lookup`），每架构页表格式不同
3. 缺页时仍需 VMSUSPEND 协议（挂起源/目标进程，通知 VM 处理）

**Direct Map 演进表**:

| C 机制 | 64 位 Direct Map 替代 | 当前状态 |
|--------|---------------------|---------|
| `createpde()` 临时映射 | `kernel_phys_to_virt(pa)` 一行加法 | ✅ 已实现 |
| `lin_lin_copy()` | `memcpy(kernel_phys_to_virt(src_pa), kernel_phys_to_virt(dst_pa), n)` | ⚠️ primitive 已实现；跨进程 VA→PA DEFERRED |
| `vm_memset()` (正常路径) | `memset(kernel_phys_to_virt(pa), pattern, n)` | DEFERRED |
| `vm_lookup()` | 保留（仍需查询页表映射 VA→PA） | DEFERRED |
| `virtual_copy_vmcheck()` | 简化：VA→PA→Direct Map→memcpy，缺页时仍 VMSUSPEND | ⚠️ primitive 已实现；VMSUSPEND DEFERRED |

---

### Ch2. C 源码分析（file:line 锚定）

#### §2.1 do_copy.c — VIRCOPY/PHYSCOPY

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_copy()` | do_copy.c:22-90 | 共用处理器：解析消息 → SELF 替换 → endpoint 验证 → 溢出检查 → virtual_copy_vmcheck |
| SELF 替换 | do_copy.c:64-65 | `if (vir_addr[i].proc_nr_e == SELF) vir_addr[i].proc_nr_e = caller->p_endpoint` |
| isokendpt 验证 | do_copy.c:66-71 | `if(! isokendpt(vir_addr[i].proc_nr_e, &p)) return(EINVAL)` |
| 溢出检查 | do_copy.c:77 | `if (bytes != (phys_bytes)(vir_bytes) bytes) return(E2BIG)` |
| CP_FLAG_TRY | do_copy.c:80-85 | VFS 专用 try-copy：`assert(caller == VFS)`; 失败返回 EFAULT |
| virtual_copy_vmcheck | do_copy.c:87-88 | 实际拷贝 + 缺页挂起 |

#### §2.2 do_safecopy.c — SAFECOPYFROM/TO/VSAFECOPY

| 符号 | 位置 | 说明 |
|------|------|------|
| `MAX_INDIRECT_DEPTH` | do_safecopy.c:21 | 间接 grant 最大深度 = 5 |
| `cp_sfinfo` | do_safecopy.c:31-36 | 软故障信息（CPF_TRY 标志） |
| `verify_grant()` | do_safecopy.c:41-266 | grant 验证：endpoint → grant_idx → 序列号 → 间接链 → 权限 → 范围 |
| 间接链处理 | do_safecopy.c:148-173 | `if (depth == MAX_INDIRECT_DEPTH) return ELOOP`；循环 follow |
| magic grant 重定向 | do_safecopy.c:217-250 | `*e_granter = g.cp_u.cp_magic.cp_who_from` |
| CPF_TRY 软故障 | do_safecopy.c:258-263 | 写 faulted 标记到 grant 表 |
| `safecopy()` | do_safecopy.c:271-372 | 验证 grant → 确定源/目标 → virtual_copy_vmcheck |
| `do_safecopy_to()` | do_safecopy.c:377-383 | CPF_WRITE 方向（caller→granter） |
| `do_safecopy_from()` | do_safecopy.c:388-394 | CPF_READ 方向（granter→caller） |
| `do_vsafecopy()` | do_safecopy.c:399-447 | 批量：拷入向量 → 逐元素 SELF 方向解析 → 逐元素 safecopy |

#### §2.3 do_umap.c + do_umap_remote.c — UMAP/UMAP_REMOTE

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_umap()` | do_umap.c:25-37 | 安全检查（seg_index != MEM_GRANT && endpt != SELF → EPERM）→ 委托 do_umap_remote |
| `do_umap_remote()` | do_umap_remote.c:26-120 | endpoint 验证 → grantee 验证 → segment type 分发 → vm_lookup → 连续性检查 |
| SELF 替换 | do_umap_remote.c:40-41 | `if (endpt == SELF) okendpt(caller->p_endpoint, &proc_nr)` |
| grantee 验证 | do_umap_remote.c:47-55 | SELF→caller；NONE/ANY/非 MEM_GRANT/无效 → EINVAL |
| MEM_GRANT 路径 | do_umap_remote.c:60-82 | verify_grant → newoffset/newep → 重新 lookup |
| VIR_ADDR 路径 | do_umap_remote.c:84-85 | `phys_addr = lin_addr = offset` |
| vm_lookup | do_umap_remote.c:94 | VA→PA 翻译 |
| 连续性检查 | do_umap_remote.c:106-109 | `vm_lookup_range(targetpr, lin_addr, NULL, count) != count → EFAULT` |

#### §2.4 do_vumap.c — VUMAP

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_vumap()` | do_vumap.c:22-131 | 拷入向量 → access 转换 → 逐元素 verify_grant/vm_lookup_range → 拷出物理向量 |
| MAPVEC_NR | do_vumap.c:30-31 | 栈数组上限；超出截断 |
| access 转换 | do_vumap.c:54-60 | VUA_READ→CPF_READ; VUA_WRITE→CPF_WRITE; VUA_READ\|VUA_WRITE→CPF_READ\|CPF_WRITE |
| 逐元素映射 | do_vumap.c:73-118 | 每个虚拟范围可能映射到多个物理范围；循环填入物理向量 |
| 物理向量拷出 | do_vumap.c:122-128 | `data_copy_vmcheck(caller, KERNEL, pvec, endpt, paddr, size)` |

#### §2.5 do_memset.c + do_safememset.c — MEMSET/SAFEMEMSET

| 符号 | 位置 | 说明 |
|------|------|------|
| `do_memset()` | do_memset.c:17-25 | 委托 `vm_memset(caller, process, base, pattern, count)` |
| `do_safememset()` | do_safememset.c:20-57 | endpoint 验证 → grant 表检查 → verify_grant(CPF_WRITE) → vm_memset |
| endpoint 验证 | do_safememset.c:36-40 | `dst_endpt == NONE` → EFAULT；`!endpoint_lookup` → EINVAL；无 grant 表 → EINVAL |

#### §2.6 调用关系图

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

### Ch3. 设计决策（hypothesis-driven）

#### D1. 跨地址空间拷贝：createpde 临时映射 vs Direct Map

**假设性推理**:
- 如果用 `createpde()` 临时映射（C 32 位方式）：每次跨空间拷贝需建立 PDE 临时映射，拷贝完成后销毁——开销大；且 64 位下内核地址空间足够大，临时映射是不必要的复杂度。
- 如果用 `Direct Map`：64 位地址空间预留区域，PA+base=KV 一行加法；跨空间拷贝等价于"VA→PA→Direct Map→memcpy"。无需建立/销毁临时映射。
- 所以用 `Direct Map`：一行加法替代 createpde，memcpy 替代 lin_lin_copy。

**实现**: `DirectMapArch::kernel_phys_to_virt(pa)` (direct_map.rs:44-46)。跨进程 VA→PA 的 PTE walk (`virt_to_phys`) 是 DEFERRED blocker。

#### D2. grant 表访问：data_copy 从用户空间读 vs 内核缓存

**假设性推理**:
- 如果内核直接读用户空间 grant 表：需要 PTE walk 翻译 grant 表地址，可能缺页触发 VMSUSPEND——而 VMSUSPEND 处理本身可能需要读 grant 表，形成递归 VMSUSPEND。
- 如果用 `data_copy()` 从 granter 地址空间拷入 grant 项到内核栈（C 方式）：`data_copy(granter, grant_table_addr, KERNEL, &g, sizeof(g))` (do_safecopy.c:121-123)——一次性拷入，后续验证在内核内存完成，避免递归。
- 所以用 `data_copy()` 拷入内核缓存：避免递归 VMSUSPEND。

**实现**: 对齐 C 方式，verify_grant 中用 data_copy 拷入 grant 项。

#### D3. verify_grant 返回值：多个输出参数 vs GrantVerifyResult 结构体

**假设性推理**:
- 如果用多个输出参数（C 方式）：`verify_grant(..., vir_bytes *offset_result, endpoint_t *e_granter, struct cp_sfinfo *sfinfo)` (do_safecopy.c:41-51)——调用者需声明多个变量传指针，类型不安全，易传错。
- 如果用结构体：`GrantVerifyResult { offset, granter, sfinfo }`——Rust 惯用法，类型安全，调用者直接解构。
- 所以用 `GrantVerifyResult` 结构体：Rust 惯用法，类型安全。

**实现**: `pub struct GrantVerifyResult { offset: u64, granter: Endpoint, sfinfo: Option<SoftFaultInfo> }` (syscall_copy.rs:179-186)。

#### D4. 间接 grant 链：递归 vs 循环 + depth

**假设性推理**:
- 如果用递归：间接链可能形成环（虽有 depth 限制），递归有栈溢出风险；且 no_std 下栈空间有限。
- 如果用循环 + depth 计数（C 方式）：`do { ... if (depth == MAX_INDIRECT_DEPTH) return ELOOP; depth++; ... } while (g.cp_flags & CPF_INDIRECT)` (do_safecopy.c:59,148-173)——无栈溢出风险，与 C 一致。
- 所以用循环 + depth 计数：与 C 一致，避免栈溢出。

**实现**: 对齐 C 方式，循环 + `MAX_INDIRECT_DEPTH = 5` 计数。

#### D5. VUMAP 物理向量：栈数组 vs Vec

**假设性推理**:
- 如果用 `Vec<VumapPhys>`：需动态分配，no_std 下需 `alloc` crate；且 DMA 路径应避免动态分配（性能 + 失败模式）。
- 如果用栈数组 `[VumapPhys; MAPVEC_NR]`（C 方式）：`struct vumap_phys pvec[MAPVEC_NR]` (do_vumap.c:31)——编译期分配在栈，no_std 兼容，无动态分配失败。
- 所以用栈数组：no_std 兼容，对齐 C。

**实现**: `[VumapPhys; MAPVEC_NR]` 栈数组。MAPVEC_NR 与 C 一致。

#### D6. CP_FLAG_TRY / CPF_TRY：保留 vs 删除

**假设性推理**:
- 如果删除 CP_FLAG_TRY / CPF_TRY：VFS 内存映射文件场景下，拷贝缺页会触发 VMSUSPEND，而 VMSUSPEND 处理可能需要文件系统回调——文件系统正在等待此次拷贝，形成死锁。
- 如果保留：CPF_TRY 路径走 `virtual_copy()`（不触发 VMSUSPEND），失败返回 EFAULT；VFS 重试而非死锁。
- 所以保留：VFS 依赖此语义避免死锁。

**实现**: 保留 CP_FLAG_TRY / CPF_TRY 常量与分支。

#### D7. vm_memset / vm_lookup：保留 vs Direct Map 替代

**假设性推理**:
- 如果用 Direct Map 完全替代 vm_memset/vm_lookup：Direct Map 只翻译 PA→KV，不翻译 VA→PA。用户空间虚拟地址仍需 PTE walk 获取物理地址。
- 如果保留接口、实现简化：正常路径用 Direct Map（memset(kernel_phys_to_virt(pa), pattern, n)），缺页时仍需 VMSUSPEND 协助换入页面。
- 所以保留接口，实现简化：64 位 Direct Map 下大部分场景不需要 VM 协助，但缺页时仍需。

**实现**: 保留 `vm_memset` / `vm_lookup` 接口；实现 DEFERRED（依赖 Direct Map PTE walk + VMSUSPEND）。

#### D8. do_umap → do_umap_remote 委托：保留 vs 合并

**假设性推理**:
- 如果保留 C 的 `#if USE_UMAP` / `#if USE_UMAP_REMOTE` 条件编译：Rust 不需要条件编译（所有系统调用总是编译），保留委托增加间接调用无益。
- 如果合并为 `dispatch_umap`：安全检查（seg_index != MEM_GRANT && endpt != SELF → EPERM）内联到入口，然后直接调用 `dispatch_umap_remote_impl`——减少间接调用，Rust 惯用法。
- 所以合并为 `dispatch_umap`：Rust 不需要 C 的条件编译。

**实现**: `dispatch_umap` 合并安全检查 + 委托 `dispatch_umap_remote_impl` (syscall_copy.rs:706-730)。

#### D9. safecopy 的 sfinfo：结构体 vs Option

**假设性推理**:
- 如果 sfinfo 总是存在（C 方式）：`struct cp_sfinfo sfinfo` (do_safecopy.c:288)——大部分场景不使用 CPF_TRY，sfinfo 字段无意义但始终占用栈空间。
- 如果用 `Option<SoftFaultInfo>`：`sfinfo: Option<SoftFaultInfo>` (syscall_copy.rs:185)——仅 CPF_TRY 场景构造 Some，其他场景 None；类型表达"可能不存在"语义。
- 所以用 `Option<SoftFaultInfo>`：大部分场景不需要，Option 表达"可能不存在"语义。

**实现**: `GrantVerifyResult.sfinfo: Option<SoftFaultInfo>` (syscall_copy.rs:185)。

---

### Ch4. 实现详解（真实代码）

#### §4.1 核心类型

贴 `GrantVerifyResult` / `SoftFaultInfo` / `SafecopyAccess` / `CopyError` struct/enum 完整定义 + C 字段对应关系。

#### §4.2 dispatch_copy — VIRCOPY/PHYSCOPY

贴 `dispatch_copy` 真实代码：SELF 替换 → endpoint 验证 → CP_FLAG_TRY 分支 → virtual_copy_vmcheck。标注 C 对应 do_copy.c:22-90。

#### §4.3 safecopy_common_impl — SAFECOPYFROM/TO

贴 `safecopy_common_impl` 真实代码：参数提取 → endpoint 验证 → grant ID 验证。标注 DEFERRED：verify_grant + virtual_copy。

#### §4.4 dispatch_umap_remote_impl — UMAP/UMAP_REMOTE

贴 `dispatch_umap_remote_impl` 真实代码：endpoint 验证 → grantee 验证 → segment type 分发。标注 DEFERRED：vm_lookup。

#### §4.5 dispatch_vumap — VUMAP

贴 `dispatch_vumap` 真实代码：参数验证 → access 转换 → 向量处理框架。标注 DEFERRED：verify_grant + vm_lookup_range + 拷出。

#### §4.6 dispatch_memset / dispatch_safememset — MEMSET/SAFEMEMSET

贴 `dispatch_memset` / `dispatch_safememset` 真实代码：endpoint 验证 → grant 表检查 → pattern 截断。标注 DEFERRED：vm_memset / verify_grant。

#### §4.7 virtual_copy_vmcheck — Direct Map primitive

贴 `virtual_copy_vmcheck` 真实代码：溢出检查 → Direct Map 区域检查 → copy_nonoverlapping。标注 DEFERRED：跨进程 PTE walk。

#### §4.8 DEFERRED 函数诚实标注

以下函数在 C 源码中存在但 Rust 尚未实现，标注 DEFERRED + 理由:

| 函数 | C 位置 | DEFERRED 理由 |
|------|--------|--------------|
| `verify_grant` 完整实现 | do_safecopy.c:41-266 | 需 VM 侧 grant 表 API（内核无直接访问） |
| `verify_grant` 间接链循环 | do_safecopy.c:148-173 | 依赖 verify_grant 主体 |
| `verify_grant` magic 重定向 | do_safecopy.c:217-250 | 同上 |
| `CPF_TRY` 软故障标记 | do_safecopy.c:258-263,336-369 | 依赖 verify_grant + data_copy |
| `CP_FLAG_TRY` try-copy 路径 | do_copy.c:80-85 | 依赖 virtual_copy（非 vmcheck 版本） |
| `virtual_copy_vmcheck` 跨进程 PTE walk | memory.c:507-535 | 依赖 `virt_to_phys` trait（DEFERRED blocker） |
| `vm_lookup` | do_umap_remote.c:94 | 依赖 PTE walk |
| `vm_lookup_range` 连续性检查 | do_umap_remote.c:106-109 | 依赖 PTE walk |
| `vm_memset` | do_memset.c:20 | 依赖 Direct Map PTE walk + VMSUSPEND |
| `do_vsafecopy` 向量拷入 | do_safecopy.c:399-447 | 依赖 virtual_copy_vmcheck |
| `do_vumap` 物理向量拷出 | do_vumap.c:122-128 | 依赖 vm_lookup_range + data_copy_vmcheck |
| VMSUSPEND 协议 | memory.c + VM 服务器 | 依赖 kernel IPC core |

**DEFERRED 共同 blocker**: `minix_arch::Paging::virt_to_phys`（VA→PA PTE walk）未稳定。5 处核心路径依赖此 trait：dispatch_copy / dispatch_umap_remote / dispatch_vumap / dispatch_memset / dispatch_safecopy_*。

---

### Ch5. 测试（可 grep 函数名）

#### §5.1 现有测试（55 个，已实现）

**常量与布局**（7 个）:
- `test_safecopy_access_flags` — SafecopyAccess::Read/Write → CPF_READ/CPF_WRITE
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

**dispatch_umap**（2 个）:
- `test_dispatch_umap_rejects_non_self_non_grant` — 非 SELF 非 grant → EPERM
- `test_umap_security_check` — UMAP 安全检查逻辑

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

#### §5.2 待补充测试（DEFERRED 函数实现后）

| 测试函数 | 验证行为 | 依赖 |
|---------|---------|------|
| `test_verify_grant_rejects_invalid_granter` | 无效 granter endpoint → EINVAL | verify_grant 实现 |
| `test_verify_grant_rejects_invalid_grant_id` | 无效 grant ID → EINVAL | verify_grant 实现 |
| `test_verify_grant_indirect_chain_depth` | 间接链超过 5 层 → ELOOP | verify_grant 间接链 |
| `test_verify_grant_magic_redirect` | magic grant 重定向 granter | verify_grant magic |
| `test_verify_grant_range_exceeded` | 超出 grant 范围 → EPERM | verify_grant 范围检查 |
| `test_virtual_copy_vmcheck_cross_process` | 跨进程 VA→PA→Direct Map→memcpy | virt_to_phys PTE walk |
| `test_vm_lookup_returns_phys_addr` | VA→PA 翻译 | vm_lookup 实现 |
| `test_vm_memset_fills_pattern` | 跨空间填充字节模式 | vm_memset 实现 |

---

### Ch6. 参见

- [16-smp.md](16-smp.md) — BKL 保证 safecopy 跨 CPU 安全
- [17-syscall-process.md](17-syscall-process.md) — fork/exec 使用 vircopy 拷贝进程上下文
- [20-syscall-device.md](20-syscall-device.md) — VMCTL 子命令（不属本文档）
- [24-cross-space-runtime.md](24-cross-space-runtime.md) — VMSUSPEND 协议和 vm_memset/vm_lookup 运行时

---

## 三、知识点覆盖矩阵

| 概念组 | Ch1 | Ch2 | Ch3 | Ch4 | Ch5 |
|--------|-----|-----|-----|-----|-----|
| A. vircopy/physcopy | §1.1 | §2.1 | D1, D6, D8 | §4.2 dispatch_copy | test_dispatch_copy_* |
| B. safecopy | §1.2 | §2.2 | D3, D4, D6, D9 | §4.3 safecopy_common_impl | test_dispatch_safecopy_* |
| C. umap | §1.3 | §2.3 | D8 | §4.4 dispatch_umap_remote | test_dispatch_umap_* |
| D. vumap | §1.3 | §2.4 | D5 | §4.5 dispatch_vumap | test_dispatch_vumap_* |
| E. memset | §1.4 | §2.5 | D7 | §4.6 dispatch_memset | test_dispatch_memset_* |
| F. Direct Map | §1.5 | §2.6 | D1 | §4.7 virtual_copy_vmcheck | test_virtual_copy_vmcheck_* |
| G. VMSUSPEND | §1.5 | §2.6 | — | §4.8 (DEFERRED) | §5.2 待补充 |
| H. redox 对比 | — | — | — | 附录 | — |

---

## 四、断裂修复表

| 断裂点 | 修复方案 |
|--------|---------|
| §3 D1 巨型 DEFERRED 块 + P-XX ID 泄漏 | D1 仅保留设计决策；DEFERRED 状态移至 Ch4 §4.8 表；删除 P1-05/P0-10/P0-02 ID |
| §4.4 Direct Map 演进表声称已实现但 5 处 TODO | 演进表保留（设计目标），增加"当前状态"列诚实标注 DEFERRED |
| §6 "来源：tmp-13" | 删除 tmp 引用 |
| 测试 bullet 不可 grep | Ch5 §5.1 列出 55 个实际 `fn test_*` 函数名 |
| "补充"节含 VMCTL（属 20-syscall-device） | 删除整个补充节；VMCTL 内容归 20-syscall-device.md |
| verify_grant 全断裂 | Ch2 §2.2 补 file:line；Ch4 §4.8 DEFERRED 表 |
| vm_lookup / vm_memset 全断裂 | Ch2 §2.3/§2.5 补 file:line；Ch4 §4.8 DEFERRED 表 |
| VMSUSPEND 全断裂 | Ch1 §1.5 提及；Ch4 §4.8 DEFERRED 表 |
| 开发文档味（日期/迭代叙事） | 重写时全部删除日期 + 迭代叙事 |

---

## 五、自检

- [x] Ch1 主语是内存/安全，非函数名/结构体名
- [x] Ch1 每节有"灵魂本质"一句话
- [x] Ch2 每个符号带 file:line
- [x] Ch3 采用 hypothesis-driven（"如果 X 设计会有 Y 问题所以用 Z"）
- [x] Ch3 无迭代叙事（"旧版/最初/后来/我们改成"）
- [x] Ch4 贴真实代码，缺失函数诚实标注 DEFERRED + 理由
- [x] Ch5 测试函数可 grep 验证（`fn test_*`）
- [x] 知识点覆盖矩阵完整（A-H 八组）
- [x] 断裂修复表完整（9 处断裂 + 修复方案）
- [x] 无 tmp 文件引用
- [x] 无内部 review ID（P0-XX/P1-XX）
- [x] 无迭代叙事日期
- [x] 跨架构统一抽象（DirectMapArch trait）
- [x] anti-translate 体现（GrantVerifyResult/SafecopyAccess/Option<SoftFaultInfo>/栈数组）
- [x] VMCTL 内容已移除（属 20-syscall-device）
- [x] Direct Map DEFERRED 诚实标注（不假装已实现）
- [x] Ch3 D1 hypothesis-driven（"如果用 createpde 会有什么问题？→ 临时映射开销 + 64位下不必要 → Direct Map"）
