# 17-syscall-copy: 跨进程内存拷贝

> **分类**: 系统调用服务
> **源码**: `minix3/minix/kernel/system/do_copy.c`, `do_safecopy.c`, `do_umap.c`, `do_umap_remote.c`, `do_vumap.c`, `do_memset.c`, `do_safememset.c`
> **前置**: 16（进程管理调用）, 14（时钟/定时器——alarm timer）
> **C 总行数**: ~920 行

---

## Ch1: 概念

**核心问题**: 内核如何安全地在不同进程的地址空间之间拷贝数据？

Minix3 的跨进程拷贝分为两层：

1. **虚拟/物理拷贝**（vircopy/physcopy）：内核直接按虚拟/物理地址拷贝，调用者必须是有权限的系统进程（PM/VFS/RS/MEM/VM）
2. **安全拷贝**（safecopy）：基于 grant 表的受控拷贝，grant 表由授权方设置，内核验证权限后执行拷贝

### 1.1 虚拟/物理拷贝

| 系统调用 | 语义 | C 处理函数 |
|---------|------|-----------|
| SYS_VIRCOPY | 按虚拟地址跨进程拷贝 | `do_copy()` |
| SYS_PHYSCOPY | 按物理地址跨进程拷贝 | `do_copy()` |

两者共用 `do_copy()`，区别仅在于地址类型。核心流程：

1. 解析消息中的 src_endpt/src_addr/dst_endpt/dst_addr/nr_bytes
2. SELF 替换为 caller endpoint
3. 验证 endpoint 有效性
4. 溢出检查（32 位遗留，64 位下始终通过）
5. 调用 `virtual_copy_vmcheck()` 执行实际拷贝

**CP_FLAG_TRY**：VFS 专用标志，拷贝失败时返回 EFAULT 而非触发缺页。

### 1.2 安全拷贝

| 系统调用 | 语义 | C 处理函数 |
|---------|------|-----------|
| SYS_SAFECOPYFROM | 从授权方读取数据 | `do_safecopy_from()` |
| SYS_SAFECOPYTO | 向授权方写入数据 | `do_safecopy_to()` |
| SYS_VSAFECOPY | 批量安全拷贝 | `do_vsafecopy()` |

安全拷贝的核心是 **grant 表**：

- 每个 system process 有一个 grant 表（`s_grant_table` + `s_grant_entries`）
- grant 表项（`cp_grant_t`）定义了授权方允许被授权方访问的内存范围
- 三种 grant 类型：
  - **CPF_DIRECT**: 直接授权，指定起始地址和长度
  - **CPF_MAGIC**: 魔术授权（仅 VFS/MIB），可重定向到第三方
  - **CPF_INDIRECT**: 间接授权，指向另一个 grant

**verify_grant()** 是安全拷贝的核心验证函数：

1. 验证 granter endpoint 有效性
2. 验证 grant ID 有效性（索引范围 + 序列号）
3. 处理间接 grant 链（最多 5 层）
4. 验证访问权限（CPF_READ/CPF_WRITE）
5. 验证拷贝范围在 grant 范围内
6. 返回实际虚拟地址和真实 granter endpoint

### 1.3 地址映射

| 系统调用 | 语义 | C 处理函数 |
|---------|------|-----------|
| SYS_UMAP | 将虚拟地址映射为物理地址 | `do_umap()` |
| SYS_UMAP_REMOTE | 映射远程进程的虚拟地址 | `do_umap_remote()` |
| SYS_VUMAP | 批量映射虚拟地址到物理地址 | `do_vumap()` |

UMAP 是 UMAP_REMOTE 的子集（只允许映射自身地址空间和 grant）。

VUMAP 用于 DMA：将一组 grant 或虚拟地址批量转换为物理地址向量，供驱动程序直接进行 DMA 操作。

### 1.4 内存设置

| 系统调用 | 语义 | C 处理函数 |
|---------|------|-----------|
| SYS_MEMSET | 在进程地址空间中填充字节 | `do_memset()` |
| SYS_SAFEMEMSET | 通过 grant 安全填充字节 | `do_safememset()` |

MEMSET 直接调用 `vm_memset()`；SAFEMEMSET 先验证 grant 权限（CPF_WRITE），再调用 `vm_memset()`。

---

## Ch2: C 源码分析

### do_copy.c (91 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 30-91 | `do_copy()` | 解析消息 → SELF 替换 → endpoint 验证 → 溢出检查 → virtual_copy_vmcheck |

关键字段：
- `m_lsys_krn_sys_copy.src_endpt` / `src_addr` / `dst_endpt` / `dst_addr` / `nr_bytes` / `flags`

### do_safecopy.c (448 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 25-26 | `MAX_INDIRECT_DEPTH` | 间接 grant 最大深度 = 5 |
| 28-35 | `cp_sfinfo` | 软故障信息（CPF_TRY 标志） |
| 42-225 | `verify_grant()` | grant 验证：endpoint → grant_idx → 序列号 → 间接链 → 权限 → 范围 |
| 227-317 | `safecopy()` | 验证 grant → 确定源/目标 → virtual_copy_vmcheck |
| 319-327 | `do_safecopy_to()` | CPF_WRITE 方向 |
| 329-337 | `do_safecopy_from()` | CPF_READ 方向 |
| 339-393 | `do_vsafecopy()` | 批量：拷贝向量 → 逐元素 safecopy |

### do_umap.c (39 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 25-38 | `do_umap()` | 安全检查 → 委托给 do_umap_remote |

### do_umap_remote.c (122 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 26-120 | `do_umap_remote()` | endpoint 验证 → grant 验证（MEM_GRANT 段）→ vm_lookup → 连续性检查 |

### do_vumap.c (131 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 30-131 | `do_vumap()` | 拷入向量 → 逐元素 verify_grant/vm_lookup → 拷出物理向量 |

### do_memset.c (28 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 19-27 | `do_memset()` | 委托 vm_memset |

### do_safememset.c (57 行)

| 行号 | 函数/逻辑 | 说明 |
|------|----------|------|
| 20-57 | `do_safememset()` | endpoint 验证 → verify_grant(CPF_WRITE) → vm_memset |

---

## Ch3: Rust 设计决策

| # | 决策 | 选项 | 结论 | 理由 |
|---|------|------|------|------|
| D1 | 跨地址空间拷贝 | createpde + lin_lin_copy vs Direct Map | **Direct Map** | 64 位下一行加法替代临时映射（D11 全局决策） |
| D2 | grant 表访问 | data_copy 从用户空间读 vs 内核缓存 | **内核缓存** | 避免递归 VMSUSPEND |
| D3 | verify_grant 返回值 | 多个输出参数 vs 结构体 | **`GrantVerifyResult` 结构体** | Rust 惯用法，类型安全 |
| D4 | 间接 grant 链 | 循环 + depth 计数 vs 递归 | **循环 + depth** | 与 C 一致，避免栈溢出 |
| D5 | VUMAP 物理向量 | 栈数组 vs Vec | **栈数组 `[VumapPhys; MAPVEC_NR]`** | no_std 环境，避免动态分配 |
| D6 | CP_FLAG_TRY / CPF_TRY | 保留 vs 删除 | **保留** | VFS 依赖此语义 |
| D7 | vm_memset / vm_lookup | 保留 vs Direct Map 替代 | **保留接口，实现简化** | 64 位 Direct Map 下大部分场景不需要 VM 协助，但缺页时仍需 |
| D8 | do_umap → do_umap_remote 委托 | 保留 vs 合并 | **合并为 `dispatch_umap`** | Rust 不需要 C 的 #if USE_UMAP 条件编译 |
| D9 | safecopy 的 sfinfo | 结构体 vs Option | **`Option<SoftFaultInfo>`** | 大部分场景不需要 |

> **⚠️ D1 (Direct Map) 当前状态：DEFERRED**
>
> Ch3 §D1 承诺 Direct Map 替换 `createpde + lin_lin_copy`，Ch4 §4.4 给出 4 行等价替换表。但 **当前实现尚未接通 Direct Map**——`syscall_copy.rs` 中 5 处显式 `TODO: implement with Direct Map` 注释（行 252/374/397/416/439），`dispatch_copy` / `dispatch_umap_remote` / `dispatch_vumap` / `dispatch_memset` / `dispatch_safecopy_*` 的核心路径仍是 stub。
>
> **DEFERRED 根因**：
>
> 1. **arch 层 Direct Map trait 未稳定**：`minix_arch::Paging::kernel_phys_to_virt()` 在 x86_64 上通过 `-KERN_VIRT_BASE` 偏移实现，在 aarch64/riscv64 上由 MMU 直接配置；跨架构统一接口尚未冻结。
> 2. **VMSUSPEND 协议未接入**：C 中 `virtual_copy_vmcheck` 在缺页时挂起源/目标进程并通知 VM 处理；Rust 端 `VmRequest` 投递链路（`vm.rs:185` `todo!("arch-specific page table walk via Direct Map")`）尚未接通。
> 3. **测试覆盖率不足**：当前 stub 不返回 OK 也不返回错误，而是落入 silent no-op——QEMU 端到端测试无法识别这种 drift。
>
> **优先级路径**：Direct Map 是 P1-05 / P0-10 / P0-02 的共同依赖，需在三者中至少 P1-05 完成后才能逐项填实 Ch4 §4.4 的等价替换。

---

## Ch4: 实现要点

### 4.1 核心类型

```rust
/// Grant 验证结果。
pub struct GrantVerifyResult {
    /// 验证后的偏移量（虚拟地址空间内）。
    pub offset: u64,
    /// 真正的授权方 endpoint（magic grant 可能重定向）。
    pub granter: Endpoint,
    /// 软故障信息（仅 CPF_TRY 场景）。
    pub sfinfo: Option<SoftFaultInfo>,
}

/// 软故障信息。
pub struct SoftFaultInfo {
    pub endpoint: Endpoint,
    pub addr: u64,
    pub value: i32,
}
```

### 4.2 拷贝方向

```rust
/// 安全拷贝的访问方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SafecopyAccess {
    /// 从授权方读取（CPF_READ）。
    Read,
    /// 向授权方写入（CPF_WRITE）。
    Write,
}
```

### 4.3 关键函数

- `verify_grant()`: 验证 grant 权限，返回实际地址和 granter
- `safecopy()`: 验证 + 执行拷贝
- `virtual_copy_vmcheck()`: Direct Map 下的跨地址空间拷贝
- `vm_memset()`: Direct Map 下的跨地址空间填充

### 4.4 Direct Map 演进

| C 机制 | 64 位 Direct Map 替代 |
|--------|---------------------|
| `createpde()` 临时映射 | `kernel_phys_to_virt(pa)` 一行加法 |
| `lin_lin_copy()` | `memcpy(kernel_phys_to_virt(src_pa), kernel_phys_to_virt(dst_pa), n)` |
| `vm_memset()` (正常路径) | `memset(kernel_phys_to_virt(pa), pattern, n)` |
| `vm_lookup()` | 保留（仍需查询页表映射） |
| `virtual_copy_vmcheck()` | 简化：VA→PA→Direct Map→memcpy，缺页时仍 VMSUSPEND |

---

## 测试

- 单元：verify_grant 正确拒绝无效 grant / 越界访问 / 错误方向
- 单元：safecopy 正确区分 CPF_READ/CPF_WRITE
- 单元：vircopy SELF 替换
- 单元：vsafecopy 向量解析
- 集成：vircopy 跨进程拷贝端到端（需要 mock 进程表）

---

## 补充：内存系统调用详细分析

> 来源：tmp-13-syscall-memory.md

### VMCTL 子命令

定义于 `minix3/minix/include/minix/com.h:394-409`：

| 常量 | 值 | 含义 |
|------|-----|------|
| `VMCTL_CLEAR_PAGEFAULT` | 12 | 清除进程的页缺失标志 |
| `VMCTL_GET_PDBR` | 13 | 获取进程页表根地址（CR3） |
| `VMCTL_MEMREQ_GET` | 14 | 获取下一个 VM 内存请求 |
| `VMCTL_MEMREQ_REPLY` | 15 | 回复 VM 内存请求 |
| `VMCTL_NOPAGEZERO` | 18 | 不将页面零初始化 |
| `VMCTL_I386_KERNELLIMIT` | 19 | 设置内核段限制（x86） |
| `VMCTL_I386_INVLPG` | 25 | 使指定 TLB 条目无效（x86） |
| `VMCTL_FLUSHTLB` | 26 | 刷新全部 TLB |
| `VMCTL_KERN_PHYSMAP` | 27 | 获取内核物理内存映射信息 |
| `VMCTL_KERN_MAP_REPLY` | 28 | 回复内核物理映射 |
| `VMCTL_SETADDRSPACE` | 29 | 设置进程地址空间 |
| `VMCTL_VMINHIBIT_SET` | 30 | 设置 VMINHIBIT 标志 |
| `VMCTL_VMINHIBIT_CLEAR` | 31 | 清除 VMINHIBIT 标志 |
| `VMCTL_CLEARMAPCACHE` | 32 | 清除映射缓存 |
| `VMCTL_BOOTINHIBIT_CLEAR` | 33 | 清除 BOOTINHIBIT 标志 |

### VM 请求链（vmrequest chain）

当进程访问的内存不在物理内存中时，内核将进程挂起并加入 VM 请求链。VM 通过 `VMCTL_MEMREQ_GET` 获取请求信息，处理完页缺失后通过 `VMCTL_MEMREQ_REPLY` 通知内核恢复进程。

### VMINHIBIT 机制

VM 在修改进程地址空间前，通过 `VMCTL_VMINHIBIT_SET` 通知内核将该进程标记为"VM 干护中"。内核在此期间跳过该进程的异步消息投递（SMP 下地址空间可能不一致），修改完成后通过 `VMCTL_VMINHIBIT_CLEAR` 解除。

### 内存系统调用行为规则

1. **仅 VM 可调用**：`sys_vmctl` 的大部分子命令仅允许 VM 进程调用
2. **页缺失清除**：`VMCTL_CLEAR_PAGEFAULT` 清除 `RTS_PAGEFAULT` 标志，使进程恢复可运行
3. **VM 请求获取**：`VMCTL_MEMREQ_GET` 遍历请求链，跳过 IPC 过滤器拒绝的请求
4. **VM 请求回复**：`VMCTL_MEMREQ_REPLY` 根据 `p_vmrequest.type` 恢复不同类型的挂起操作
5. **VMINHIBIT 设置**：SMP 下若进程在其他 CPU 上运行，需先迁移到本地 CPU
6. **VMINHIBIT 清除**：SMP 下清除 `MF_SENDA_VM_MISS` 并重新尝试异步消息投递
7. **地址映射权限**：`sys_umap` 检查 `CHECK_MEM` 标志和 `s_mem_tab` 范围

### 内存系统调用函数列表

| 功能 | 函数 | 源文件 |
|------|------|--------|
| VM 控制操作 | `do_vmctl()` | system/do_vmctl.c |
| 虚拟→物理地址映射 | `do_umap()` | system/do_umap.c |
| 远程地址映射 | `do_umap_remote()` | system/do_umap_remote.c |
| 向量化地址映射 | `do_vumap()` | system/do_vumap.c |
| 虚拟地址间复制 | `do_vircopy()` | system/do_vircopy.c |
| 物理地址间复制 | `do_copy()` | system/do_copy.c |
| 安全复制（从） | `do_safecopy_from()` | system/do_safecopy.c |
| 安全复制（到） | `do_safecopy_to()` | system/do_safecopy.c |
| 向量化安全复制 | `do_vsafecopy()` | system/do_vsafecopy.c |
| 内存填充 | `do_memset()` | system/do_memset.c |
| 安全内存填充 | `do_safememset()` | system/do_safememset.c |

---

## 参见

- [14-clock-timer.md](14-clock-timer.md) — alarm timer 用于 SAFECOPY 的超时
- [16-syscall-process.md](16-syscall-process.md) — fork/exec 使用 vircopy
- [23-cross-space-runtime.md](23-cross-space-runtime.md) — VMSUSPEND 协议和 vm_memset/vm_lookup
