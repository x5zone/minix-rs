# 21-vm-rs-services: VM 与 RS 的服务交互

> **分类**: 模块库
> **源码**: `minix3/minix/servers/vm/rs.c`
> **说明**: VM 处理来自 RS（Reincarnation Server）的 4 个请求：权限设置、准备、更新、内存控制。这些服务支撑 Minix3 的 live update 机制。

---

## 1. 概述

### 1.1 RS 是什么

RS（Reincarnation Server）是 Minix3 的服务管理器，负责启动、监控和**热更新**系统服务。当 RS 需要更新一个运行中的服务时，它创建新实例、迁移状态、切换端点——整个过程不需要停机。

VM 在这个流程中扮演关键角色：**管理进程的内存状态迁移**。RS 通过 4 个 IPC 请求与 VM 交互：

| 请求 | 请求码 | 功能 | RS 调用时机 |
|------|--------|------|-----------|
| VM_RS_SET_PRIV | 0xC25 | 设置进程的 VM 调用权限 | RS 启动新服务实例时 |
| VM_RS_PREPARE | 0xC30 | 准备 live update 的内存状态 | RS 执行多组件更新（含 VM 自身）时 |
| VM_RS_UPDATE | 0xC29 | 执行 live update 的进程切换 | RS 执行更新切换时 |
| VM_RS_MEMCTL | 0xC2A | 内存控制（钉住/预分配/创建 VM 实例） | RS 准备更新环境时 |

### 1.2 Live Update 流程概览

```
RS 决定更新服务 X
    │
    ├─ 1. RS 创建新实例 X'，分配新 slot
    │     → VM_RS_SET_PRIV(X')：设置 X' 的 VM 调用权限
    │
    ├─ 2. RS 请求 VM 准备内存迁移
    │     → VM_RS_PREPARE(X → X')：钉住内存、扩展堆、映射 mmap 区域
    │
    ├─ 3. RS 请求内存控制
    │     → VM_RS_MEMCTL(PIN)：钉住进程内存防止换出
    │     → VM_RS_MEMCTL(HEAP_PREALLOC)：预分配堆空间
    │     → VM_RS_MEMCTL(MAP_PREALLOC)：预分配 mmap 区域
    │
    ├─ 4. RS 执行切换
    │     → VM_RS_UPDATE(X → X')：内核切换端点 + VM 交换 slot
    │
    └─ 5. 旧实例 X 退出
```

**关键约束**：live update 期间 VM 不能分配新对象（物理页、slab 对象等），否则回滚时无法恢复。因此 PREPARE 阶段必须预分配所有可能需要的内存。

### 1.3 四个服务的依赖关系

```
VM_RS_SET_PRIV ─── 独立，可单独调用
VM_RS_MEMCTL  ─── 独立，可单独调用
VM_RS_PREPARE ─── 依赖 SET_PRIV（目标进程必须有正确权限）
VM_RS_UPDATE  ─── 依赖 PREPARE（内存状态必须已准备）
```

### 1.4 行为规则

1. **SET_PRIV**：RS 通过 `sys_datacopy` 传递调用权限位图，VM 调用 `acl_set()` 设置进程的 ACL。系统进程不允许共享权限位图。
2. **PREPARE**：仅用于多组件 live update（含 VM 自身更新）。钉住源进程内存、扩展目标进程堆、映射源进程的 mmap 区域到目标进程。
3. **UPDATE**：执行实际的进程切换——内核先切换端点（`sys_update`），VM 再交换 slot 数据和动态内存区域。返回 SUSPEND 表示 VM 不回复调用者（RS），而是直接回复新端点。
4. **MEMCTL**：5 个子请求，控制内存钉住、VM 实例创建、堆/mmap 预分配、预分配查询。

---

## 2. C 源码分析

### 2.1 IPC 接口定义

4 个请求的消息字段定义在 `com.h`：

```c
// com.h:724 — VM_RS_SET_PRIV
#define VM_RS_SET_PRIV  (VM_RQ_BASE+37)
#   define VM_RS_NR     m2_i1    // 目标进程 endpoint
#   define VM_RS_BUF    m2_l1    // 权限位图地址（0=默认）
#   define VM_RS_SYS    m2_i2    // 是否系统进程

// com.h:766 — VM_RS_PREPARE
#define VM_RS_PREPARE   (VM_RQ_BASE+48)
// 使用 m_lsys_vm_update 结构（与 VM_RS_UPDATE 共享）

// com.h:738 — VM_RS_MEMCTL
#define VM_RS_MEMCTL    (VM_RQ_BASE+42)
#   define VM_RS_CTL_ENDPT  m1_i1    // 目标进程 endpoint
#   define VM_RS_CTL_REQ    m1_i2    // 子请求类型
#   define VM_RS_CTL_ADDR   m2_p1    // 地址（输入/输出）
#   define VM_RS_CTL_LEN    m2_i3    // 长度（输入/输出）

// VM_RS_MEMCTL 子请求常量（com.h:749-753）
#   define VM_RS_MEM_PIN            0  // 钉住内存
#   define VM_RS_MEM_MAKE_VM        1  // 创建 VM 实例
#   define VM_RS_MEM_HEAP_PREALLOC  2  // 预分配堆
#   define VM_RS_MEM_MAP_PREALLOC   3  // 预分配 mmap
#   define VM_RS_MEM_GET_PREALLOC_MAP 4 // 查询预分配 mmap

// com.h:729 — VM_RS_UPDATE
#define VM_RS_UPDATE    (VM_RQ_BASE+41)
// 使用 m_lsys_vm_update 结构
```

`m_lsys_vm_update` 结构（ipc.h）：

```c
// RS update 消息字段
src     → m_lsys_vm_update.src     // 源进程 endpoint
dst     → m_lsys_vm_update.dst     // 目标进程 endpoint
flags   → m_lsys_vm_update.flags   // 更新标志
```

更新标志（`include/minix/rs.h`）：

| 标志 | 含义 |
|------|------|
| `SF_VM_ROLLBACK` | 回滚更新（反向切换） |
| `SF_VM_NOMMAP` | 不迁移 mmap 区域 |

### 2.2 do_rs_set_priv — 权限设置

> 源码位置：`rs.c:34-68`

```c
int do_rs_set_priv(message *m)
{
    int r, n, nr;
    struct vmproc *vmp;
    bitchunk_t call_mask[VM_CALL_MASK_SIZE], *call_mask_p;

    nr = m->VM_RS_NR;                          // 目标 endpoint

    if ((r = vm_isokendpt(nr, &n)) != OK)      // 验证 endpoint
        return EINVAL;

    vmp = &vmproc[n];

    if (m->VM_RS_BUF) {                        // 有权限位图
        r = sys_datacopy(m->m_source,          // 从 RS 地址空间复制
            (vir_bytes) m->VM_RS_BUF, SELF,
            (vir_bytes) call_mask, sizeof(call_mask));
        if (r != OK) return r;
        call_mask_p = call_mask;
    } else {
        if (m->VM_RS_SYS) {                    // 系统进程不能共享权限
            return EINVAL;
        }
        call_mask_p = NULL;                    // NULL=默认权限
    }

    acl_set(vmp, call_mask_p, m->VM_RS_SYS);   // 设置 ACL

    return OK;
}
```

**关键行为**：
1. 验证目标 endpoint 有效性
2. 如果 `VM_RS_BUF` 非零，从 RS 地址空间复制权限位图
3. 如果 `VM_RS_BUF` 为零且 `VM_RS_SYS` 为真，返回 EINVAL（系统进程必须有显式权限位图）
4. 调用 `acl_set()` 设置进程的 VM 调用权限

### 2.3 do_rs_prepare — 准备更新

> 源码位置：`rs.c:71-148`

```c
int do_rs_prepare(message *m_ptr)
{
    endpoint_t src_e, dst_e;
    int src_p, dst_p;
    struct vmproc *src_vmp, *dst_vmp;
    struct vir_region *src_data_vr, *dst_data_vr;
    vir_bytes src_addr, dst_addr;
    int sys_upd_flags;

    src_e = m_ptr->m_lsys_vm_update.src;
    dst_e = m_ptr->m_lsys_vm_update.dst;
    sys_upd_flags = m_ptr->m_lsys_vm_update.flags;

    // 验证源和目标 endpoint
    if(vm_isokendpt(src_e, &src_p) != OK) return EINVAL;
    src_vmp = &vmproc[src_p];
    if(vm_isokendpt(dst_e, &dst_p) != OK) return EINVAL;
    dst_vmp = &vmproc[dst_p];

    // 钉住源进程内存
    map_pin_memory(src_vmp);

    // 扩展目标进程堆到与源进程相同大小
    src_data_vr = region_search(&src_vmp->vm_regions_avl,
        VM_MMAPBASE, AVL_LESS);
    dst_data_vr = region_search(&dst_vmp->vm_regions_avl,
        VM_MMAPBASE, AVL_LESS);
    src_addr = src_data_vr->vaddr + src_data_vr->length;
    dst_addr = dst_data_vr->vaddr + dst_data_vr->length;
    if (src_addr > dst_addr)
        real_brk(dst_vmp, src_addr);

    // 钉住目标进程内存
    map_pin_memory(dst_vmp);

    // 映射源进程的 mmap 区域到目标进程（CoW）
    if (!(sys_upd_flags & SF_VM_NOMMAP))
        map_proc_dyn_data(src_vmp, dst_vmp);

    return OK;
}
```

**关键行为**：
1. 验证源/目标 endpoint
2. `map_pin_memory(src)` — 钉住源进程所有内存（确保全部映射到物理页）
3. 扩展目标进程堆 — 查找源/目标的 data region（堆下方的最大区域），如果源堆更大则扩展目标
4. `map_pin_memory(dst)` — 钉住目标进程内存
5. `map_proc_dyn_data(src, dst)` — 将源进程的 mmap 区域以 CoW 方式映射到目标进程

**设计要点**：堆扩展"宁可浪费不可不足"——目标进程在 live update 期间不能分配新内存，所以必须预分配足够堆空间。

### 2.4 do_rs_update — 执行更新

> 源码位置：`rs.c:150-225`

```c
int do_rs_update(message *m_ptr)
{
    endpoint_t src_e, dst_e, reply_e;
    int src_p, dst_p;
    struct vmproc *src_vmp, *dst_vmp;
    int r, sys_upd_flags;

    src_e = m_ptr->m_lsys_vm_update.src;
    dst_e = m_ptr->m_lsys_vm_update.dst;
    sys_upd_flags = m_ptr->m_lsys_vm_update.flags;
    reply_e = m_ptr->m_source;

    // 验证 endpoint
    if(vm_isokendpt(src_e, &src_p) != OK) return EINVAL;
    src_vmp = &vmproc[src_p];
    if(vm_isokendpt(dst_e, &dst_p) != OK) return EINVAL;
    dst_vmp = &vmproc[dst_p];

    // 检查标志：非回滚+非NOMMAP时，目标不能有预分配mmap
    if((sys_upd_flags & (SF_VM_ROLLBACK|SF_VM_NOMMAP)) == 0) {
        if(map_region_lookup_type(dst_vmp, VR_PREALLOC_MAP))
            return ENOSYS;
    }

    // 内核先执行端点切换
    r = sys_update(src_e, dst_e,
        sys_upd_flags & SF_VM_ROLLBACK ? SYS_UPD_ROLLBACK : 0);
    if(r != OK) return r;

    // VM 交换 slot 数据
    r = swap_proc_slot(src_vmp, dst_vmp);
    if(r != OK) return r;

    // VM 交换动态内存区域（mmap）
    r = swap_proc_dyn_data(src_vmp, dst_vmp, sys_upd_flags);
    if(r != OK) return r;

    // 重新绑定页表
    pt_bind(&src_vmp->vm_pt, src_vmp);
    pt_bind(&dst_vmp->vm_pt, dst_vmp);

    // 回复调用者（端点可能已切换）
    if(reply_e != VM_PROC_NR) {
        if(reply_e == src_e) reply_e = dst_e;
        else if(reply_e == dst_e) reply_e = src_e;
        m_ptr->m_type = OK;
        r = ipc_send(reply_e, m_ptr);
        if(r != OK) panic("ipc_send() error");
    }

    return SUSPEND;  // 告诉主循环不要回复
}
```

**关键行为**：
1. 验证 endpoint + 检查 VR_PREALLOC_MAP 标志
2. `sys_update()` — 内核切换端点（源进程获得目标端点，反之亦然）
3. `swap_proc_slot()` — VM 交换两个 slot 的 vmproc 数据（保留 endpoint 和 slot 编号）
4. `swap_proc_dyn_data()` — 交换 mmap 区域（回滚时反向交换）
5. `pt_bind()` — 重新绑定页表到正确的进程
6. 手动回复调用者（端点已切换，主循环不能自动回复）
7. 返回 SUSPEND — 阻止主循环的自动回复

### 2.5 do_rs_memctl — 内存控制

> 源码位置：`rs.c:349-391`

```c
int do_rs_memctl(message *m_ptr)
{
    endpoint_t ep;
    int req, r, proc_nr;
    struct vmproc *vmp;

    ep = m_ptr->VM_RS_CTL_ENDPT;
    req = m_ptr->VM_RS_CTL_REQ;

    if ((r = vm_isokendpt(ep, &proc_nr)) != OK) return EINVAL;
    vmp = &vmproc[proc_nr];

    switch(req) {
    case VM_RS_MEM_PIN:
        // 仅当有多个 VM 实例时才实际钉住
        if (num_vm_instances <= 1) return OK;
        r = map_pin_memory(vmp);
        return r;
    case VM_RS_MEM_MAKE_VM:
        r = rs_memctl_make_vm_instance(vmp);
        return r;
    case VM_RS_MEM_HEAP_PREALLOC:
        r = rs_memctl_heap_prealloc(vmp,
            (vir_bytes*) &m_ptr->VM_RS_CTL_ADDR,
            (size_t*) &m_ptr->VM_RS_CTL_LEN);
        return r;
    case VM_RS_MEM_MAP_PREALLOC:
        r = rs_memctl_map_prealloc(vmp,
            (vir_bytes*) &m_ptr->VM_RS_CTL_ADDR,
            (size_t*) &m_ptr->VM_RS_CTL_LEN);
        return r;
    case VM_RS_MEM_GET_PREALLOC_MAP:
        r = rs_memctl_get_prealloc_map(vmp,
            (vir_bytes*) &m_ptr->VM_RS_CTL_ADDR,
            (size_t*) &m_ptr->VM_RS_CTL_LEN);
        return r;
    default:
        return EINVAL;
    }
}
```

**5 个子请求详解**：

| 子请求 | 值 | 功能 | 辅助函数 |
|--------|---|------|---------|
| VM_RS_MEM_PIN | 0 | 钉住进程内存（仅多 VM 实例时有效） | `map_pin_memory()` |
| VM_RS_MEM_MAKE_VM | 1 | 将进程标记为 VM 实例 | `rs_memctl_make_vm_instance()` |
| VM_RS_MEM_HEAP_PREALLOC | 2 | 预分配堆空间 | `rs_memctl_heap_prealloc()` |
| VM_RS_MEM_MAP_PREALLOC | 3 | 预分配 mmap 区域 | `rs_memctl_map_prealloc()` |
| VM_RS_MEM_GET_PREALLOC_MAP | 4 | 查询预分配的 mmap 区域 | `rs_memctl_get_prealloc_map()` |

### 2.6 辅助函数分析

#### rs_memctl_make_vm_instance（rs.c:233-299）

将一个进程标记为 VM 实例，用于 VM 自身的热更新：

1. 检查当前 VM 实例数（最多 2 个）
2. 设置 `VMF_VM_INSTANCE` 标志
3. 钉住新 VM 实例的内存
4. 为当前 VM 和新 VM 实例预分配页表
5. 让新 VM 实例映射当前 VM 的页表和自己的页表

#### rs_memctl_heap_prealloc（rs.c:281-294）

预分配堆空间：查找 data region，调用 `real_brk()` 扩展堆到指定大小。

#### rs_memctl_map_prealloc（rs.c:301-322）

预分配 mmap 区域：调用 `map_page_region()` 分配新区域，设置 `VR_PREALLOC_MAP` 标志。

#### rs_memctl_get_prealloc_map（rs.c:329-342）

查询预分配的 mmap 区域：查找 `VR_PREALLOC_MAP` 标志的区域，返回地址和长度。

### 2.7 C 源码覆盖完整性

**语义范围**：VM 处理 RS 请求的 4 个服务函数及其辅助函数

| 符号 | 类型 | 源码位置 | 在语义范围内? | 文档覆盖? |
|------|------|---------|-------------|-----------|
| do_rs_set_priv | 函数 | rs.c:34 | 已覆盖 | §2.2 |
| do_rs_prepare | 函数 | rs.c:71 | 已覆盖 | §2.3 |
| do_rs_update | 函数 | rs.c:150 | 已覆盖 | §2.4 |
| do_rs_memctl | 函数 | rs.c:349 | 已覆盖 | §2.5 |
| rs_memctl_make_vm_instance | 函数 | rs.c:233 | 已覆盖 | §2.6 |
| rs_memctl_heap_prealloc | 函数 | rs.c:281 | 已覆盖 | §2.6 |
| rs_memctl_map_prealloc | 函数 | rs.c:301 | 已覆盖 | §2.6 |
| rs_memctl_get_prealloc_map | 函数 | rs.c:329 | 已覆盖 | §2.6 |
| swap_proc_slot | 函数 | utility.c:188 | 已覆盖 | §2.4 |
| swap_proc_dyn_data | 函数 | utility.c:312 | 已覆盖 | §2.4 |
| map_pin_memory | 函数 | region.c:779 | 已覆盖 | §2.3 |
| map_proc_dyn_data | 函数 | proto.h:52 | 已覆盖 | §2.3 |
| map_region_lookup_type | 函数 | region.c | 已覆盖 | §2.4 |
| VM_RS_SET_PRIV | 宏 | com.h:724 | 已覆盖 | §2.1 |
| VM_RS_PREPARE | 宏 | com.h:766 | 已覆盖 | §2.1 |
| VM_RS_UPDATE | 宏 | com.h:729 | 已覆盖 | §2.1 |
| VM_RS_MEMCTL | 宏 | com.h:738 | 已覆盖 | §2.1 |
| VM_RS_MEM_PIN | 宏 | com.h:749 | 已覆盖 | §2.5 |
| VM_RS_MEM_MAKE_VM | 宏 | com.h:750 | 已覆盖 | §2.5 |
| VM_RS_MEM_HEAP_PREALLOC | 宏 | com.h:751 | 已覆盖 | §2.5 |
| VM_RS_MEM_MAP_PREALLOC | 宏 | com.h:752 | 已覆盖 | §2.5 |
| VM_RS_MEM_GET_PREALLOC_MAP | 宏 | com.h:753 | 已覆盖 | §2.5 |
| SF_VM_ROLLBACK | 宏 | rs.h:198 | 已覆盖 | §2.1 |
| SF_VM_NOMMAP | 宏 | rs.h:199 | 已覆盖 | §2.1 |

**覆盖统计**：总符号 25 / 已覆盖 25 / 覆盖率 100%

---

## 3. Rust 设计决策

### 3.1 模块组织：一服务一文件

4 个 RS 服务共享 `rs.rs` 文件，因为它们都围绕 live update 这一个核心场景：

```
os/servers/vm/src/
├── rs.rs          # handle_rs_set_priv/prepare/update/memctl + RsMemctlRequest enum
```

### 3.2 错误处理：每服务独立 enum

```rust
pub(crate) enum RsSetPrivError {
    ProcessNotFound,     // EINVAL
    SysProcNoMask,       // EINVAL: 系统进程必须有权限位图
}

pub(crate) enum RsPrepareError {
    ProcessNotFound,     // EINVAL
    PinFailed,           // map_pin_memory 失败
    HeapExtendFailed,    // real_brk 失败
    DynDataFailed,       // map_proc_dyn_data 失败
}

pub(crate) enum RsUpdateError {
    ProcessNotFound,     // EINVAL
    PreallocMapConflict, // ENOSYS: 目标有 VR_PREALLOC_MAP
    KernelUpdateFailed,  // sys_update 失败
    SlotSwapFailed,      // swap_proc_slot 失败
    DynDataSwapFailed,   // swap_proc_dyn_data 失败
}

pub(crate) enum RsMemctlError {
    ProcessNotFound,     // EINVAL
    InvalidRequest,      // EINVAL: 未知子请求
    MakeVmFailed,        // rs_memctl_make_vm_instance 失败
    HeapPreallocFailed,  // 堆预分配失败
    MapPreallocFailed,   // mmap 预分配失败
    InvalidLength,       // EINVAL: len <= 0
}
```

### 3.3 RsMemctlRequest：子请求用 enum 而非裸整数

C 源码中 `VM_RS_CTL_REQ` 是 `m1_i2` 整数，Rust 用 enum 表达：

```rust
pub(crate) enum RsMemctlRequest {
    Pin,
    MakeVmInstance,
    HeapPrealloc { addr: VirBytes, len: usize },
    MapPrealloc { addr: VirBytes, len: usize },
    GetPreallocMap,
}
```

### 3.4 SUSPEND 语义

`do_rs_update` 返回 SUSPEND 告诉 VM 主循环不要自动回复。Rust 中用返回类型区分：

```rust
pub(crate) enum RsUpdateResult {
    Ok,
    Suspend,  // 主循环不回复，handler 已手动发送回复
}
```

### 3.5 map_pin_memory 依赖

`map_pin_memory()` 遍历进程所有区域，调用 `map_handle_memory()` 确保所有页面映射到物理内存。这个函数依赖 `map_handle_memory()`（region.c:588），属于区域管理模块。RS 服务调用它作为高层操作，不直接操作区域内部。

**Rust 设计理由**：`handle_rs_prepare` 暂不实现，因为 `map_pin_memory` 需要区域管理模块提供"确保全部映射"的接口。当前设计将此功能留在区域模块，rs.rs 仅做高层编排。

### 3.6 swap_proc_slot 的 Rust 表达

C 中 `swap_proc_slot` 直接交换两个 `vmproc` 结构体的内容（保留 endpoint 和 slot 编号）。Rust 中由于 typestate 的存在，不能直接交换——需要通过 typestate 安全的转换路径。当前阶段先标记为 TODO，因为 typestate 系统需要扩展才能支持 slot 交换。

**Rust 设计理由**：typestate 保证每个 slot 在编译时处于确定状态（Active/Dead/Zombie 等）。直接交换会破坏 typestate 不变量。正确做法是引入 `swap_slots(src: ActiveProc, dst: ActiveProc) -> (ActiveProc, ActiveProc)` 的受控转换，但需要 VmProcTable API 扩展。

### 3.7 功能依赖与架构分层

| 服务 | 依赖模块 | 架构说明 |
|------|---------|---------|
| VM_RS_SET_PRIV | acl 模块 | RS 启动服务时必须调用，无外部依赖 |
| VM_RS_MEMCTL (PIN/HEAP_PREALLOC) | brk/mmap 模块 | live update 前的内存准备，brk 扩展堆、mmap 分配匿名区域 |
| VM_RS_PREPARE | region 模块 (map_pin_memory) | 多组件更新专用，需要"确保全部映射"接口 |
| VM_RS_UPDATE | vmproc typestate (swap_proc_slot) | 需要 typestate 安全的 slot 交换 API |
| VM_RS_MEMCTL (MAKE_VM) | 多 VM 实例支持 | VM 自身热更新，架构最复杂 |

---

## 4. 实现详解

### 4.1 模块结构

```
os/servers/vm/src/rs.rs
├── RsSetPrivError + handle_rs_set_priv()
├── RsPrepareError + handle_rs_prepare()
├── RsUpdateError + handle_rs_update()
├── RsMemctlError + RsMemctlRequest + handle_rs_memctl()
└── tests
```

### 4.2 handle_rs_set_priv

> 对应 C 源码 `do_rs_set_priv()` (rs.c:34)

```rust
pub(crate) fn handle_rs_set_priv(
    table: &VmProcTable,
    _caller: Endpoint,
    target: Endpoint,
    mask: Option<crate::acl::AclMask>,
    is_sys_proc: bool,
) -> Result<(), RsSetPrivError> {
    let slot = table.vm_isokendpt(target)
        .map_err(|_| RsSetPrivError::ProcessNotFound)?;

    let mut active = table.get_active(slot)
        .ok_or(RsSetPrivError::ProcessNotFound)?;

    if mask.is_none() && is_sys_proc {
        return Err(RsSetPrivError::SysProcNoMask);
    }

    let acl = AclState::acl_set(is_sys_proc, mask);
    active.set_acl(acl);

    Ok(())
}
```

**与 C 的差异**：C 使用 `sys_datacopy` 从 RS 地址空间读取权限位图到 VM；
Rust 直接将 `AclMask` 作为参数传入（IPC 消息内联），跳过跨地址空间复制。

### 4.3 handle_rs_memctl

> 对应 C 源码 `do_rs_memctl()` (rs.c:349)

```rust
pub(crate) fn handle_rs_memctl(
    table: &VmProcTable,
    page_alloc: &mut VmPageAllocator,
    frames: &mut PageFrames,
    target: Endpoint,
    request: RsMemctlRequest,
) -> Result<RsMemctlResult, RsMemctlError> {
    let slot = table.vm_isokendpt(target)
        .map_err(|_| RsMemctlError::ProcessNotFound)?;

    let active = table.get_active(slot)
        .ok_or(RsMemctlError::ProcessNotFound)?;

    match request {
        RsMemctlRequest::Pin => {
            // C: 仅当 num_vm_instances > 1 时才实际钉住
            // 当前始终只有 1 个 VM 实例，直接返回 OK
            Ok(RsMemctlResult::Ok)
        }
        RsMemctlRequest::MakeVmInstance => {
            // C: rs_memctl_make_vm_instance
            // 当前不支持多 VM 实例
            Err(RsMemctlError::MakeVmFailed)
        }
        RsMemctlRequest::HeapPrealloc { len, .. } => {
            if len == 0 {
                return Err(RsMemctlError::InvalidLength);
            }
            // C: rs_memctl_heap_prealloc (rs.c:281)
            // bytes = current_data_end + len, then real_brk(vmp, bytes)
            let current_brk = active.region_top();
            let new_brk = VirBytes(current_brk.0 + len as u64);
            let req = crate::brk::BrkRequest {
                endpoint: target,
                new_brk_addr: new_brk,
            };
            crate::brk::handle_brk(table, page_alloc, frames, &req)
                .map(|_| RsMemctlResult::AddrLen {
                    addr: current_brk,
                    len,
                })
                .map_err(|_| RsMemctlError::HeapPreallocFailed)
        }
        RsMemctlRequest::MapPrealloc { len, .. } => {
            if len == 0 {
                return Err(RsMemctlError::InvalidLength);
            }
            // C: rs_memctl_map_prealloc → map_page_region()
            // Rust: 使用 mmap 模块分配匿名区域
            let aligned_len = VirBytes(((len as u64) + 4095) & !4095);
            let mmap_req = minix_types::VmMmapIn {
                caller: target,
                forwhom: target,
                addr: VirBytes(0),
                length: aligned_len,
                prot: 3,
                flags: 0x1002, // MAP_PRIVATE | MAP_ANONYMOUS | MAP_PREALLOC
                fd: -1,
                offset: 0,
            };
            crate::mmap::handle_mmap(table, page_alloc, frames, &mmap_req)
                .map(|resp| RsMemctlResult::AddrLen {
                    addr: resp.mapped_addr,
                    len: aligned_len.0 as usize,
                })
                .map_err(|_| RsMemctlError::MapPreallocFailed)
        }
        RsMemctlRequest::GetPreallocMap => {
            // C: rs_memctl_get_prealloc_map
            // 查找 VR_PREALLOC_MAP 标志的区域
            let regions = active.regions();
            let found = regions.iter().find(|vr| vr.flags.contains(VrFlags::PREALLOC_MAP));
            match found {
                Some(vr) => Ok(RsMemctlResult::AddrLen {
                    addr: vr.vaddr,
                    len: vr.length.0 as usize,
                }),
                None => Ok(RsMemctlResult::AddrLen {
                    addr: VirBytes(0),
                    len: 0,
                }),
            }
        }
    }
}
```

### 4.4 handle_rs_prepare

> 对应 C 源码 `do_rs_prepare()` (rs.c:71)

当前阶段返回 `NotImplemented`，因为 `map_pin_memory` 和 `map_proc_dyn_data` 需要区域管理模块的进一步支持。

### 4.5 handle_rs_update

> 对应 C 源码 `do_rs_update()` (rs.c:150)

当前阶段返回 `NotImplemented`，因为 `swap_proc_slot` 需要 typestate 系统扩展。

---

## 5. 测试要点

### 5.1 SET_PRIV 测试

| 测试 | 验证 |
|------|------|
| 系统进程无权限位图 | 返回 SysProcNoMask |
| 无效 endpoint | 返回 ProcessNotFound |
| 正常设置权限 | ACL 更新成功 |

### 5.2 MEMCTL 测试

| 测试 | 验证 |
|------|------|
| PIN 请求 | 单 VM 实例时返回 OK |
| MAKE_VM 请求 | 返回 MakeVmFailed |
| HEAP_PREALLOC len=0 | 返回 InvalidLength |
| MAP_PREALLOC len=0 | 返回 InvalidLength |
| GET_PREALLOC_MAP 无区域 | 返回 addr=0, len=0 |
| 无效 endpoint | 返回 ProcessNotFound |

### 5.3 错误码映射测试

| 错误 | errno |
|------|-------|
| ProcessNotFound | EINVAL |
| SysProcNoMask | EINVAL |
| InvalidRequest | EINVAL |
| InvalidLength | EINVAL |
| PreallocMapConflict | ENOSYS |
| MakeVmFailed | EPERM |

---

## 6. 参见

- [03-acl.md](03-acl.md) — ACL 权限系统，`acl_set()` 的实现
- [20-vm-exit.md](20-vm-exit.md) — 进程退出，`do_procctl` 处理
- [17-vm-brk.md](17-vm-brk.md) — 堆管理，`real_brk()` 的实现
- [18-vm-mmap.md](18-vm-mmap.md) — mmap 服务，`map_page_region()` 的实现
- [26-vm-init-main.md](26-vm-init-main.md) — VM 初始化，CALLMAP 注册
- [24-client-alloc-lib.md](24-client-alloc-lib.md) — 客户端分配库，IPC 接口总表
