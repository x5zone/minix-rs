# Fork 全栈纵向切片"灵魂复刻" — 规格说明书

> **目标**: 像素级还原 Minix3 fork 的核心状态流转
> **原则**: 只有硬件允许 Mock，OS 内部算法和资源管理逻辑必须触及
> **禁止**: 使用简单 `Clone` trait 代替深度克隆逻辑
> **必须**: 体现微内核跨进程协同的消息协议设计

---

## 1. 范围

### 1.1 实现范围

本规格覆盖 Minix3 `fork()` 系统调用在四个服务中的完整纵向切片：

| 层次 | Minix3 源码 | Rust crate | 核心职责 |
|------|------------|-----------|---------|
| PM | `servers/pm/forkexit.c` | `minix-pm` | 进程表管理、PID 分配、VFS 异步通知、SUSPEND 机制 |
| VM | `servers/vm/fork.c` + `region.c` + `phys.c` | `minix-vm` | vmproc 地址空间克隆、CoW 引用计数、页表生命周期 |
| VFS | `servers/vfs/misc.c` | `minix-vfs` | fproc 文件描述符复制、filp/vnode 引用计数 |
| Kernel | `kernel/system/do_fork.c` | `minix-kernel` | PCB 克隆、上下文伪造(retreg=0)、endpoint generation |

### 1.2 Mock 边界

| 允许 Mock | 不允许 Mock |
|-----------|------------|
| 物理页分配 `alloc_mem()` → `Vec<u64>` | VM region 遍历（AVL 或 Vec） |
| 页表硬件 `pt_writemap()` → `Vec<u64>` | CoW refcount（pb_reference/pb_unreferenced） |
| FPU 上下文 `save_fpu()` → `Vec<u8>` | filp 引用计数（filp_count++/--） |
| CPU 调度 `enqueue/dequeue` → 空操作 | vnode 引用计数（v_ref_count++/--） |
| CR3/TTBR 加载 → 空操作 | PCB 克隆（显式构造 + 修复） |
| 寄存器硬件加载 → 空操作 | retreg 伪造（ret_reg = 0） |
| | endpoint generation（递增 + 回绕算法） |

---

## 2. PM 层规格

### 2.1 源码对齐: `minix/servers/pm/forkexit.c` — `do_fork()`

#### 2.1.1 完整状态机

```
do_fork()
  │
  ├── ① 容量检查
  │     procs_in_use == NR_PROCS → EAGAIN
  │     procs_in_use >= NR_PROCS - LAST_FEW && uid != 0 → EAGAIN
  │
  ├── ② 轮转查找空闲槽位
  │     next_child = (next_child + 1) % NR_PROCS  (static 变量)
  │     循环直到找到 !IN_USE 的槽位
  │
  ├── ③ vm_fork(parent_ep, child_slot, &child_ep)  ← 唯一可失败点
  │     IPC: sendrec(VM, VM_FORK)
  │     失败 → 直接返回错误码（无需回滚，尚未修改任何状态）
  │
  ├── ④ procs_in_use++
  │
  ├── ⑤ *rmc = *rmp  (整体拷贝 mproc)
  │
  ├── ⑥ mp_sigact 指针修复（Rust 不需要，值类型无指针）
  │
  ├── ⑦ rmc->mp_parent = who_p
  │
  ├── ⑧ Tracer 处理
  │     if !(TO_TRACEFORK): mp_tracer = NO_TRACER, mp_trace_flags = 0
  │
  ├── ⑨ 特权进程调度器修正
  │     if PRIV_PROC: mp_scheduler = SCHED_PROC_NR
  │
  ├── ⑩ 标志位过滤: mp_flags &= (IN_USE | DELAY_CALL | TAINTED)
  │
  ├── ⑪ 重置管理字段
  │     child_utime = 0, child_stime = 0
  │     exitstatus = 0, sigstatus = 0
  │     endpoint = child_ep
  │     intervals = [0; NR_ITIMERS]
  │     started = getticks()
  │
  ├── ⑫ get_free_pid() → mp_pid
  │
  ├── ⑬ tell_vfs(VFS_PM_FORK)  ← 异步 asynsend3, 设置 VFS_CALL
  │
  ├── ⑭ if tracer != NO_TRACER: sig_proc(SIGSTOP)
  │
  └── ⑮ return SUSPEND  ← 父进程挂起，等 VFS 回复
```

#### 2.1.2 关键不变量

1. **vm_fork() 是最后一个可失败操作**: 成功后 PM 不得让 fork 失败，因为 VM 已调用 `sys_fork()` 创建内核进程
2. **SUSPEND 机制**: PM 返回 SUSPEND 给内核，父进程挂起；VFS 回复 VFS_PM_FORK_REPLY 后唤醒
3. **调度失败回滚**: `sched_start_user()` 失败时调用 `exit_proc()` 销毁子进程，通知父进程 -1

#### 2.1.3 PID 循环回收算法

源码: `minix/servers/pm/utility.c` — `get_free_pid()`

```
next_pid: static pid_t = INIT_PID + 1  (初始值 2)
PID 范围: [2, 30000]  (NR_PIDS = 30000)
算法:
  do {
    next_pid = (next_pid < NR_PIDS) ? next_pid + 1 : INIT_PID + 1
    冲突检测: 遍历 mproc 表，检查 mp_pid == next_pid || mp_procgrp == next_pid
  } while (冲突)
```

#### 2.1.4 tell_vfs 异步机制

源码: `minix/servers/pm/utility.c` — `tell_vfs()`

```
前置检查: if (VFS_CALL | EVENT_CALL) → panic("not idle")
发送: asynsend3(VFS_PROC_NR, msg, AMF_NOREPLY)
标记: mp_flags |= VFS_CALL
```

#### 2.1.5 VFS_PM_FORK_REPLY 处理

源码: `minix/servers/pm/main.c` — `handle_vfs_reply()` L369-L396

```
清除 VFS_CALL | NEW_PARENT 标志
sched_start_user(scheduler, child_proc)
  成功 → reply(child, OK); reply(parent, child_pid)
  失败 → exit_proc(child, -1); reply(parent, -1)
```

### 2.2 Rust 结构定义

```rust
pub struct ForkResult {
    pub child_index: usize,
    pub child_pid: Pid,
    pub child_endpoint: Endpoint,
}

pub enum ForkError {
    TableFull,
    ReservedForRoot,
    ResourceExhausted,
    InternalError,
}

pub enum ForkState {
    Prepare,
    VmForkPending,
    VfsNotifyPending,
    Scheduling,
    Completed,
    Failed,
}
```

### 2.3 已实现 vs 待实现

| 逻辑点 | 状态 | 文件 |
|--------|------|------|
| `Process::fork_from()` 显式构造 | ✅ 已实现 | `os/servers/pm/src/mproc/fork.rs` |
| `do_fork_prepare()` 槽位分配 | ✅ 已实现 | `os/servers/pm/src/mproc/fork.rs` |
| `PidGenerator::get_free_pid()` | ✅ 已实现 | `os/servers/pm/src/mproc/pid_gen.rs` |
| `ProcTable::find_free_slot()` 轮转 | ✅ 已实现 | `os/servers/pm/src/mproc/table.rs` |
| `vm_fork()` IPC 调用 | ❌ 待实现 | `os/servers/pm/src/mproc/fork.rs` |
| `tell_vfs()` 异步通知 | ❌ 待实现 | `os/servers/pm/src/mproc/fork.rs` |
| `do_fork_reply()` VFS 回复处理 | ❌ 待实现 | `os/servers/pm/src/mproc/fork.rs` |
| `do_fork()` 完整状态机 | ❌ 待实现 | `os/servers/pm/src/mproc/fork.rs` |
| SUSPEND 返回机制 | ❌ 待实现 | `os/servers/pm/src/mproc/fork.rs` |

---

## 3. VM 层规格

### 3.1 源码对齐: `minix/servers/vm/fork.c` — `do_fork()`

#### 3.1.1 完整算法流程

```
VM do_fork(msg)
  │
  ├── ① 验证父进程 endpoint: vm_isokendpt(VMF_ENDPOINT, &proc)
  ├── ② 验证子进程槽号: 0 <= VMF_SLOTNO < NR_PROCS
  ├── ③ 获取父子 vmproc: vmp = &vmproc[proc], vmc = &vmproc[childproc]
  ├── ④ 保存子进程旧页表，整体复制 vmproc
  │     origpt = vmc->vm_pt
  │     *vmc = *vmp
  │     vmc->vm_slot = childproc
  │     region_init(&vmc->vm_regions_avl)
  │     vmc->vm_endpoint = NONE
  │     vmc->vm_pt = origpt  (恢复子进程自己的页表)
  ├── ⑤ 创建新页表: pt_new(&vmc->vm_pt)
  ├── ⑥ CoW 复制内存区域: map_proc_copy(vmc, vmp)
  │     └── 遍历 vmp 的所有 vir_region (AVL 中序遍历)
  │         └── map_copy_region(dst, src, region)
  │             └── 对每个物理页: pb_reference(pb, offset, region, memtype)
  │                 └── pb_link(): refcount++ (不复制 4KB 物理页!)
  ├── ⑦ 只继承 VMF_INUSE: vmc->vm_flags &= VMF_INUSE
  ├── ⑧ ACL 继承: acl_fork(vmc)
  │     USER_ACL → 继承; 其他 → NO_ACL
  ├── ⑨ 调用内核: sys_fork(vmp->vm_endpoint, childproc, &vmc->vm_endpoint, PFF_VMINHIBIT)
  ├── ⑩ 绑定页表: pt_bind(&vmc->vm_pt, vmc) → 清除 RTS_VMINHIBIT
  └── ⑪ 返回 child_endpoint: msg->VMF_CHILD_ENDPOINT = vmc->vm_endpoint
```

#### 3.1.2 CoW 核心算法

**pb_reference()** — 共享物理页（不复制 4KB）:

源码: `minix/servers/vm/phys.c`

```
pb_reference(pb, offset, region, memtype):
  newpr = SLABALLOC()           // 分配几十字节的 phys_region
  pb_link(newpr, pb, offset, region)  // refcount++
  physblock_set(region, offset, newpr)

pb_link(newpr, pb, offset, parent):
  newpr->ph = pb
  newpr->parent = parent
  newpr->next_ph_list = pb->firstregion  // 头插法
  pb->firstregion = newpr
  pb->refcount++               // ★ 核心操作
```

**pb_unreferenced()** — 解除引用:

```
pb_unreferenced(region, pr, flags):
  从 pb->firstregion 链表中移除 pr
  pb->refcount--
  if refcount == 0: free_mem(pb->phys)  // 释放物理页
  SLABFREE(pr)
```

**anon_writable()** — CoW 判定:

源码: `minix/servers/vm` 匿名内存类型

```
anon_writable(pr):
  if pr->ph->phys == MAP_NONE: return 0
  if pr->parent->remaps > 0: return 1
  return pr->ph->refcount == 1  // ★ 独占可写，共享只读
```

**mem_cow()** — 写时复制:

```
mem_cow(region, ph, new_page_cl, new_page):
  1. if new_page == MAP_NONE: new_page = alloc_mem(1, allocflags)
  2. sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE)  // 复制 4KB
  3. pb = pb_new(new_page)  // 新 phys_block, refcount=0
  4. pb_unreferenced(region, ph, 0)  // 旧 refcount--
  5. pb_link(ph, pb, ph->offset, region)  // 新 refcount=1
  6. ph->memtype = &mem_type_anon
```

#### 3.1.3 CoW 完整状态机

```
fork 前:
  父: phys_region P1 → phys_block B (refcount=1, phys=0x1000)
  PTE: vaddr → 0x1000, PTF_WRITE

fork 后 (map_proc_copy + map_writept):
  父: P1 → B (refcount=2, phys=0x1000), PTE: PTF_READ
  子: P2 → B (refcount=2, phys=0x1000), PTE: PTF_READ
  共享链表: B.firstregion → P2 → P1 → NULL

子进程写入 vaddr:
  1. CPU 写只读页 → 写保护异常 → 内核转发到 VM
  2. map_pf() → anon_writable() 返回 0 (refcount=2)
  3. anon_pagefault() → mem_cow()
     a. alloc_mem() → 0x2000
     b. sys_abscopy(0x1000, 0x2000)
     c. pb_unreferenced(): B.refcount 2→1
     d. pb_link(P2, C): C.refcount 0→1
  4. map_ph_writept(): refcount==1 → PTF_WRITE

写缺页处理后:
  父: P1 → B (refcount=1, phys=0x1000), PTF_WRITE
  子: P2 → C (refcount=1, phys=0x2000), PTF_WRITE
```

#### 3.1.4 共享内存段 (VR_SHARED) 处理

- fork 时 `pb_reference()` 仍被调用，refcount++
- 但 PTE 标记为 PTF_WRITE（不设 CoW 保护）
- 写操作不触发缺页，直接修改同一物理页
- 这是 POSIX 共享内存语义

#### 3.1.5 页表生命周期

```
pt_new()       → 分配空页目录，映射内核空间
map_proc_copy() → CoW 复制区域，写入 PTE（只读）
sys_fork(PFF_VMINHIBIT) → 内核创建 PCB，设 RTS_VMINHIBIT
pt_bind()      → 通知内核页表地址，清除 RTS_VMINHIBIT
```

### 3.2 Rust 结构定义

```rust
bitflags::bitflags! {
    pub struct VmFlags: u32 {
        const INUSE = 0x001;
        const EXITING = 0x002;
        const VM_INSTANCE = 0x010;
    }
}

bitflags::bitflags! {
    pub struct VmRegionFlags: u16 {
        const WRITABLE = 0x001;
        const SHARED = 0x040;
        const ANON = 0x100;
        const DIRECT = 0x200;
    }
}

pub struct VmProc {
    pub flags: VmFlags,
    pub endpoint: Endpoint,
    pub page_table: PageTable,
    pub regions: Vec<VmRegion>,
    pub region_top: VirBytes,
    pub acl: i32,
    pub slot: usize,
    pub total: VirBytes,
    pub total_max: VirBytes,
    pub minor_page_fault: u64,
    pub major_page_fault: u64,
}

pub struct VmRegion {
    pub vaddr: VirBytes,
    pub length: usize,
    pub flags: VmRegionFlags,
    pub phys_refs: Vec<PhysRegionRef>,
}

pub struct PhysBlock {
    pub phys_addr: u64,
    pub ref_count: u8,
    pub first_region: Option<usize>,
}

pub struct PhysRegionRef {
    pub phys_block_idx: usize,
    pub offset: VirBytes,
    pub next_in_list: Option<usize>,
}
```

### 3.3 待实现清单

| 逻辑点 | 状态 | 目标文件 |
|--------|------|---------|
| `VmProc` 结构体 | ❌ | `os/servers/vm/src/vmproc.rs` |
| `VmRegion` 结构体 | ❌ | `os/servers/vm/src/vmproc.rs` |
| `PhysBlock` 结构体 | ❌ | `os/servers/vm/src/vmproc.rs` |
| `PhysRegionRef` 结构体 | ❌ | `os/servers/vm/src/vmproc.rs` |
| `VmProc::fork_from()` | ❌ | `os/servers/vm/src/fork.rs` |
| `pb_reference()` refcount++ | ❌ | `os/servers/vm/src/cow.rs` |
| `pb_unreferenced()` refcount-- | ❌ | `os/servers/vm/src/cow.rs` |
| `pb_link()` 头插法链表 | ❌ | `os/servers/vm/src/cow.rs` |
| `anon_writable()` CoW 判定 | ❌ | `os/servers/vm/src/cow.rs` |
| `mem_cow()` 写时复制 | ❌ | `os/servers/vm/src/cow.rs` |
| `map_proc_copy()` 区域遍历 | ❌ | `os/servers/vm/src/fork.rs` |
| `map_copy_region()` 逐区域复制 | ❌ | `os/servers/vm/src/fork.rs` |
| `pt_new()` 创建新页表 | ❌ | `os/servers/vm/src/pagetable.rs` |
| `pt_bind()` 绑定页表 | ❌ | `os/servers/vm/src/pagetable.rs` |
| `acl_fork()` ACL 继承 | ❌ | `os/servers/vm/src/fork.rs` |
| `vm_fork()` 主流程 | ❌ | `os/servers/vm/src/fork.rs` |

---

## 4. VFS 层规格

### 4.1 源码对齐: `minix/servers/vfs/misc.c` — `pm_fork()`

#### 4.1.1 完整算法流程

```
pm_fork(pproc, cproc, cpid)
  │
  ├── ① okendpt(pproc, &parentno)  ← 验证父进程 endpoint
  │     检查: endpoint != NONE
  │     检查: _ENDPOINT_P(endpoint) 在 [0, NR_PROCS) 范围
  │     检查: fproc[slot].fp_endpoint == endpoint
  │
  ├── ② childno = _ENDPOINT_P(cproc)  ← 不用 isokendpt（子进程 endpoint 尚未设置）
  │     范围检查: 0 <= childno < NR_PROCS
  │
  ├── ③ assert(fproc[childno].fp_pid == PID_FREE)  ← 确保子进程槽空闲
  │
  ├── ④ 保存子进程锁，整体拷贝 fproc
  │     c_fp_lock = fproc[childno].fp_lock
  │     fproc[childno] = fproc[parentno]
  │     fproc[childno].fp_lock = c_fp_lock  ← 恢复子进程自己的锁
  │
  ├── ⑤ filp 引用计数递增（核心！）
  │     for i in 0..OPEN_MAX:
  │       if cp->fp_filp[i] != NULL:
  │         cp->fp_filp[i]->filp_count++  ← 共享 filp，refcount++
  │
  ├── ⑥ 设置子进程标识
  │     cp->fp_pid = cpid
  │     cp->fp_endpoint = cproc
  │
  ├── ⑦ 清除标志: cp->fp_flags = FP_NOFLAGS
  │
  ├── ⑧ vnode 引用计数递增
  │     if cp->fp_rd: dup_vnode(cp->fp_rd)  ← v_ref_count++
  │     if cp->fp_wd: dup_vnode(cp->fp_wd)  ← v_ref_count++
  │     注意: dup_vnode 只递增 v_ref_count，不递增 v_fs_count
  │
  └── ⑨ fp_cloexec_set 通过结构体拷贝自动继承
```

#### 4.1.2 引用计数关系图

```
fork 前:
  父进程 fproc
    fp_filp[0] ──→ filp (filp_count=1)
    fp_filp[1] ──→ filp (filp_count=1)
    fp_rd ────────→ vnode (v_ref_count=1)
    fp_wd ────────→ vnode (v_ref_count=1)

fork 后:
  父进程 fproc                   子进程 fproc
    fp_filp[0] ──┐                 fp_filp[0] ──┘  → filp (filp_count=2)
    fp_filp[1] ──┐                 fp_filp[1] ──┘  → filp (filp_count=2)
    fp_rd ───────┐                 fp_rd ───────┘   → vnode (v_ref_count=2)
    fp_wd ───────┐                 fp_wd ───────┘   → vnode (v_ref_count=2)
    fp_lock (自己的)               fp_lock (自己的，被保留)
```

#### 4.1.3 close_filp() 引用计数递减

源码: `minix/servers/vfs/filedes.c` — `close_filp()`

```
close_filp(f):
  if f->filp_count - 1 == 0 && f->filp_mode != FILP_CLOSED:
    // 最后一个引用者，处理特殊文件（字符设备、块设备、套接字）
    cdev_close / bdev_close / sdev_close
    f->filp_mode = FILP_CLOSED

  if --f->filp_count == 0:
    // 引用计数归零，释放 vnode
    put_vnode(f->filp_vno)  // v_ref_count--
    f->filp_vno = NULL
  else if f->filp_count < 0:
    panic("invalid filp count")
  else:
    unlock_vnode(f->filp_vno)  // 还有其他引用者，只解锁
```

#### 4.1.4 put_vnode() 延迟同步机制

源码: `minix/servers/vfs/vnode.c` — `put_vnode()`

```
put_vnode(vp):
  if vp->v_ref_count > 1:
    vp->v_ref_count--
    if vp->v_fs_count > 256: vnode_clean_refs(vp)  // 批量压缩
    return
  // v_ref_count == 1，即将降为 0
  req_putnode(vp->v_fs_e, vp->v_inode_nr, vp->v_fs_count)  // 通知底层 FS
  vp->v_fs_count = 0
  vp->v_ref_count = 0
```

**关键**: `v_fs_count` 不随 `v_ref_count` 同步递减，而是在 `v_ref_count` 降为 0 时一次性释放。这是性能优化。

#### 4.1.5 为什么 fork 只对 fp_rd/fp_wd 调用 dup_vnode

因为 `fp_filp[i]` 指向的 vnode 的引用关系已通过 `filp_count++` 间接维护。filp 是 vnode 的"代理"：只要 filp 还在使用（filp_count > 0），它就持有对 vnode 的引用。fork 增加了 filp 的使用者数量，但没创建新的 filp-vnode 关系，所以不需要改变 v_ref_count。

而 `fp_rd` 和 `fp_wd` 是**直接**指向 vnode 的，没有中间的 filp 层，所以必须通过 `dup_vnode()` 显式增加 `v_ref_count`。

### 4.2 Rust 结构定义

```rust
pub const OPEN_MAX: usize = 128;
pub const NGROUPS_MAX: usize = 32;
pub const NR_FILPS: usize = 16384;
pub const NR_VNODES: usize = 16384;

bitflags::bitflags! {
    pub struct FpFlags: u32 {
        const SRV_PROC = 0x001;
        const REVIVED = 0x002;
        const SESLDR = 0x004;
        const PENDING = 0x010;
        const EXITING = 0x020;
        const PM_WORK = 0x040;
    }
}

pub struct FProc {
    pub flags: FpFlags,
    pub pid: Pid,
    pub endpoint: Endpoint,
    pub root_dir: Option<VNodeRef>,
    pub work_dir: Option<VNodeRef>,
    pub filps: [Option<FilpRef>; OPEN_MAX],
    pub cloexec_set: u128,
    pub real_uid: Uid,
    pub eff_uid: Uid,
    pub real_gid: Gid,
    pub eff_gid: Gid,
    pub ngroups: usize,
    pub supplemental_groups: [Gid; NGROUPS_MAX],
    pub umask: u32,
    pub name: [u8; 16],
}

pub struct Filp {
    pub mode: u32,
    pub flags: i32,
    pub count: u32,       // 引用计数！
    pub vnode: Option<VNodeRef>,
    pub pos: i64,         // 共享的文件偏移量
}

pub struct VNodeRef {
    pub index: usize,
}

pub struct FilpRef {
    pub index: usize,     // 指向全局 filp 表的索引
}
```

### 4.3 待实现清单

| 逻辑点 | 状态 | 目标文件 |
|--------|------|---------|
| `FProc` 结构体 | ❌ | `os/servers/vfs/src/fproc.rs` |
| `Filp` 结构体 | ❌ | `os/servers/vfs/src/fproc.rs` |
| `VNodeRef` 结构体 | ❌ | `os/servers/vfs/src/fproc.rs` |
| `FilpRef` 结构体 | ❌ | `os/servers/vfs/src/fproc.rs` |
| `FProc::fork_from()` | ❌ | `os/servers/vfs/src/fork.rs` |
| filp_count++ 循环 | ❌ | `os/servers/vfs/src/fork.rs` |
| `dup_vnode()` v_ref_count++ | ❌ | `os/servers/vfs/src/fork.rs` |
| `fp_lock` 保留 | ❌ | `os/servers/vfs/src/fork.rs` |
| `close_filp()` refcount-- | ❌ | `os/servers/vfs/src/close.rs` |
| `put_vnode()` 延迟同步 | ❌ | `os/servers/vfs/src/close.rs` |
| `okendpt()` 验证 | ❌ | `os/servers/vfs/src/fork.rs` |

---

## 5. Kernel 层规格

### 5.1 源码对齐: `minix/kernel/system/do_fork.c` — `do_fork()`

#### 5.1.1 完整算法流程

```
do_fork(caller, m_ptr)
  │
  ├── ① 验证参数
  │     isokendpt(m_ptr->endpt, &p_proc)
  │     rpp = proc_addr(p_proc)  // 父进程
  │     rpc = proc_addr(m_ptr->slot)  // 子进程
  │     assert(!isemptyp(rpp) && isemptyp(rpc))
  │     assert(RTS_ISSET(rpp, RTS_RECEIVING))  // 父进程必须阻塞在接收状态
  │
  ├── ② 保存 FPU 上下文
  │     save_fpu(rpp)
  │     gen = _ENDPOINT_G(rpc->p_endpoint)
  │     old_fpu_save_area_p = rpc->p_seg.fpu_state  // 保存子槽的 FPU 缓冲区
  │
  ├── ③ 整体复制 PCB: *rpc = *rpp
  │
  ├── ④ 修复 FPU 缓冲区指针
  │     rpc->p_seg.fpu_state = old_fpu_save_area_p
  │     if proc_used_fpu(rpp):
  │       memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE)
  │
  ├── ⑤ 递增 generation，生成新 endpoint
  │     if ++gen >= _ENDPOINT_MAX_GENERATION: gen = 1  // 回绕到 1，不是 0
  │     rpc->p_nr = m_ptr->slot
  │     rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr)
  │
  ├── ⑥ ★ 伪造子进程返回值: rpc->p_reg.retreg = 0
  │     // retreg = eax (x86) / r0 (ARM)
  │     // 子进程恢复执行时 eax=0，即 fork() 返回 0
  │
  ├── ⑦ 清零时间统计
  │     rpc->p_user_time = 0
  │     rpc->p_sys_time = 0
  │
  ├── ⑧ 清除杂项标志
  │     rpc->p_misc_flags &= ~(MF_VIRT_TIMER | MF_PROF_TIMER | MF_SC_TRACE | MF_STEP)
  │     rpc->p_virt_left = 0
  │     rpc->p_prof_left = 0
  │
  ├── ⑨ 追加进程名 "*F"
  │     if strlen(rpc->p_name) + 2 < sizeof(rpc->p_name):
  │       strcat(rpc->p_name, "*F")
  │
  ├── ⑩ 设置不可运行
  │     RTS_SET(rpc, RTS_NO_QUANTUM)
  │     reset_proc_accounting(rpc)
  │     rpc->p_cpu_time_left = 0
  │     rpc->p_cycles = 0
  │     rpc->p_kcall_cycles = 0
  │     rpc->p_kipc_cycles = 0
  │     rpc->p_tick_cycles = 0
  │     cpuavg_init(&rpc->p_cpuavg)
  │
  ├── ⑪ 特权进程降级
  │     if priv(rpp)->s_flags & SYS_PROC:
  │       rpc->p_priv = priv_addr(USER_PRIV_ID)
  │       rpc->p_rts_flags |= RTS_NO_PRIV
  │
  ├── ⑫ 返回子进程 endpoint 和消息地址
  │     m_ptr->m_krn_lsys_sys_fork.endpt = rpc->p_endpoint
  │     m_ptr->m_krn_lsys_sys_fork.msgaddr = rpp->p_delivermsg_vir
  │
  ├── ⑬ VM 抑制
  │     if m_ptr->flags & PFF_VMINHIBIT:
  │       RTS_SET(rpc, RTS_VMINHIBIT)
  │
  ├── ⑭ 清除信号和追踪
  │     RTS_UNSET(rpc, RTS_SIGNALED | RTS_SIG_PENDING | RTS_P_STOP)
  │     sigemptyset(&rpc->p_pending)
  │
  └── ⑮ 清除页表基址
        rpc->p_seg.p_cr3 = 0
        rpc->p_seg.p_cr3_v = NULL
```

#### 5.1.2 上下文伪造: retreg = 0

```
1. 父进程调用 sys_fork()，处于 RTS_RECEIVING 状态
2. *rpc = *rpp 复制父进程的完整寄存器状态（包括 eax）
3. rpc->p_reg.retreg = 0 将子进程的 eax 强制设为 0
4. 子进程被调度运行时，restore_user_context() 从 p_reg 恢复所有寄存器
5. 子进程用户态看到 eax = 0，即 fork() 返回 0
```

#### 5.1.3 Endpoint Generation 算法

源码: `minix/include/minix/com.h`

```
#define _ENDPOINT_GENERATION_SHIFT  15
#define _ENDPOINT_MAX_GENERATION    65535

endpoint = (generation << 15) | slot
generation = (endpoint >> 15) & 0x7FFF
slot = endpoint & 0x7FFF

fork 时:
  gen = _ENDPOINT_G(rpc->p_endpoint)  // 取子槽当前 generation
  if ++gen >= 65535: gen = 1           // 回绕到 1，不是 0
  rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr)
```

#### 5.1.4 RTS 标志位

| 标志 | 值 | fork 中的操作 |
|------|-----|-------------|
| `RTS_NO_QUANTUM` | 0x8000 | **设置** — 子进程无时间片 |
| `RTS_VMINHIBIT` | 0x200 | **条件设置** — 等 VM 设置页表 |
| `RTS_NO_PRIV` | 0x80 | **条件设置** — 特权进程子进程降级 |
| `RTS_SIGNALED` | 0x10 | **清除** — 不继承信号 |
| `RTS_SIG_PENDING` | 0x20 | **清除** — 不继承信号处理 |
| `RTS_P_STOP` | 0x40 | **清除** — 不继承追踪 |

**核心规则**: `p_rts_flags == 0` 时进程才可运行。

#### 5.1.5 FPU 保存区修复

```
问题: *rpc = *rpp 会覆盖子槽的 p_seg.fpu_state 指针
修复:
  1. 保存子槽的 FPU 缓冲区指针: old_fpu_save_area_p = rpc->p_seg.fpu_state
  2. 执行 *rpc = *rpp（此时子槽的 fpu_state 被覆盖为父进程的）
  3. 恢复子槽自己的缓冲区指针: rpc->p_seg.fpu_state = old_fpu_save_area_p
  4. 如果父进程使用了 FPU，复制 FPU 内容到子槽的缓冲区
```

### 5.2 Rust 结构定义

```rust
bitflags::bitflags! {
    pub struct RtsFlags: u32 {
        const SLOT_FREE = 0x01;
        const PROC_STOP = 0x02;
        const SENDING = 0x04;
        const RECEIVING = 0x08;
        const SIGNALED = 0x10;
        const SIG_PENDING = 0x20;
        const P_STOP = 0x40;
        const NO_PRIV = 0x80;
        const NO_ENDPOINT = 0x100;
        const VMINHIBIT = 0x200;
        const NO_QUANTUM = 0x8000;
    }
}

bitflags::bitflags! {
    pub struct MiscFlags: u32 {
        const VIRT_TIMER = 0x01;
        const PROF_TIMER = 0x02;
        const SC_TRACE = 0x04;
        const STEP = 0x08;
    }
}

pub struct KProcess {
    pub slot: usize,
    pub endpoint: Endpoint,
    pub rts_flags: RtsFlags,
    pub misc_flags: MiscFlags,
    pub priority: i8,
    pub quantum_size_ms: u32,
    pub user_time: Clock,
    pub sys_time: Clock,
    pub virt_left: Clock,
    pub prof_left: Clock,
    pub name: [u8; 16],
    pub pending_signals: u64,
    pub is_system_proc: bool,
    pub ret_reg: i32,       // fork 返回值寄存器
    pub cr3: u64,           // 页表基址（Mock）
    pub fpu_state: Vec<u8>, // FPU 保存区（Mock）
}

pub struct ProcTable {
    pub procs: [Option<KProcess>; NR_PROCS],
    pub generations: [i32; NR_PROCS],
}
```

### 5.3 待实现清单

| 逻辑点 | 状态 | 目标文件 |
|--------|------|---------|
| `KProcess` 结构体 | ❌ (当前仅空壳) | `os/kernel/src/proc.rs` |
| `RtsFlags` 位标志 | ❌ | `os/kernel/src/proc.rs` |
| `MiscFlags` 位标志 | ❌ | `os/kernel/src/proc.rs` |
| `ProcTable` + generations | ❌ | `os/kernel/src/proc.rs` |
| `make_endpoint()` | ❌ | `os/kernel/src/endpoint.rs` |
| `endpoint_generation()` | ❌ | `os/kernel/src/endpoint.rs` |
| `endpoint_slot()` | ❌ | `os/kernel/src/endpoint.rs` |
| `KProcess::sys_fork()` | ❌ | `os/kernel/src/system/do_fork.rs` |
| `ret_reg = 0` 上下文伪造 | ❌ | `os/kernel/src/system/do_fork.rs` |
| generation 递增 + 回绕 | ❌ | `os/kernel/src/system/do_fork.rs` |
| RTS 标志管理 | ❌ | `os/kernel/src/system/do_fork.rs` |
| FPU 保存区修复 | ❌ | `os/kernel/src/system/do_fork.rs` |
| 特权降级 | ❌ | `os/kernel/src/system/do_fork.rs` |
| 进程名 "*F" 追加 | ❌ | `os/kernel/src/system/do_fork.rs` |

---

## 6. 跨层消息协议

### 6.1 PM → VM: VM_FORK

```
输入: VMF_ENDPOINT (m1_i1) = 父进程 endpoint
      VMF_SLOTNO   (m1_i2) = 子进程槽号
输出: VMF_CHILD_ENDPOINT (m1_i3) = 子进程 endpoint
```

### 6.2 VM → Kernel: SYS_FORK

```
输入: m_lsys_krn_sys_fork.endpt  = 父进程 endpoint
      m_lsys_krn_sys_fork.slot   = 子进程槽号
      m_lsys_krn_sys_fork.flags  = PFF_VMINHIBIT (0x01)
输出: m_krn_lsys_sys_fork.endpt   = 子进程的新 endpoint
      m_krn_lsys_sys_fork.msgaddr = 子进程的消息缓冲区虚拟地址
```

### 6.3 PM → VFS: VFS_PM_FORK

```
VFS_PM_ENDPT  (m7_i1) = 子进程 endpoint
VFS_PM_PENDPT (m7_i2) = 父进程 endpoint
VFS_PM_CPID   (m7_i3) = 子进程 PID
VFS_PM_REUID  (m7_i4) = -1 (普通 fork 不使用)
VFS_PM_REGID  (m7_i5) = -1 (普通 fork 不使用)
```

### 6.4 VFS → PM: VFS_PM_FORK_REPLY

```
VFS_PM_ENDPT = 子进程 endpoint（确认）
```

### 6.5 完整调用链

```
用户进程: fork()
    │
    ▼
PM: do_fork()                        [forkexit.c]
    ├── ① 检查进程表
    ├── ② 查找空闲槽位
    ├── ③ vm_fork() ──── IPC ────→ VM: do_fork()          [vm/fork.c]
    │   │                                ├── 验证 endpoint/slot
    │   │                                ├── *vmc = *vmp (复制 vmproc)
    │   │                                ├── pt_new() (新页表)
    │   │                                ├── map_proc_copy() (CoW)
    │   │                                │   └── map_copy_region()
    │   │                                │       └── pb_reference() (refcount++)
    │   │                                ├── sys_fork() ──→ Kernel: do_fork()  [do_fork.c]
    │   │                                │                    ├── *rpc = *rpp (复制 PCB)
    │   │                                │                    ├── generation++, 新 endpoint
    │   │                                │                    ├── retreg = 0 (子返回 0)
    │   │                                │                    ├── RTS_NO_QUANTUM
    │   │                                │                    ├── RTS_VMINHIBIT
    │   │                                │                    └── 特权降级
    │   │                                ├── pt_bind() (绑定页表, 清 RTS_VMINHIBIT)
    │   │                                └── 返回 child_endpoint
    │   └── ← child_endpoint
    ├── ④ *rmc = *rmp (复制 mproc)
    ├── ⑤ 修复 sigact 指针 (Rust 不需要)
    ├── ⑥ 设置父进程关系
    ├── ⑦ 清除追踪器
    ├── ⑧ 特权进程 scheduler
    ├── ⑨ 标志位过滤 (只保留 TAINTED)
    ├── ⑩ 重置统计/定时器
    ├── ⑪ get_free_pid()
    ├── ⑫ tell_vfs(VFS_PM_FORK) ──→ VFS: pm_fork()        [vfs/misc.c]
    │   │                                ├── fproc[child] = fproc[parent]
    │   │                                ├── filp_count++ (所有打开的 fd)
    │   │                                ├── fp_pid = cpid
    │   │                                ├── fp_flags = FP_NOFLAGS
    │   │                                ├── dup_vnode(fp_rd)
    │   │                                ├── dup_vnode(fp_wd)
    │   │                                └── 回复 VFS_PM_FORK_REPLY
    ├── ⑬ 追踪器 SIGSTOP
    └── ⑭ return SUSPEND
```

---

## 7. 集成测试规格

### 7.1 必须验证的硬核点

1. **四份进程表 endpoint 一致性**: PM/VM/VFS/Kernel 的子进程 endpoint 必须相同
2. **PID 在 PM 和 VFS 中一致性**
3. **子进程不可运行**: RTS_NO_QUANTUM + RTS_VMINHIBIT
4. **CoW 验证**: fork 后写入触发页面复制，refcount 从 2 降为 1
5. **filp 引用计数验证**: fork 后 close 不关闭文件
6. **vnode 引用计数验证**: fork 后 close 不释放 vnode
7. **ret_reg = 0 验证**: 子进程 fork 返回 0
8. **endpoint generation 验证**: 槽位重用后 generation 递增

### 7.2 测试用例

```rust
test_full_fork_flow()           // 完整 fork 流程
test_cow_write_triggers_copy()  // CoW 写时复制
test_fork_close_does_not_close_file()  // filp 引用计数
test_endpoint_generation_increment()    // generation 递增
test_ret_reg_zero()             // 子进程返回 0
test_shared_memory_not_cow()    // VR_SHARED 不触发 CoW
test_privileged_process_demotion()  // 特权降级
test_fork_table_full()          // 进程表满
test_pid_allocation_no_conflict()  // PID 冲突检测
```
