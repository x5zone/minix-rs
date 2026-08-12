# Fork 全栈纵向切片 — 分阶段任务规划书

> **核心原则**: 所有硬件操作全部Mock，仅实现OS内部逻辑
> **硬件Mock范围**: MMU/页表操作、寄存器读写、时钟中断、磁盘IO等全部硬件相关逻辑均使用Mock实现
> **实现顺序**: PM(已完成基础结构) → VM → Kernel → VFS → PM(状态机) → 集成测试
> **阶段原则**: 每阶段为独立可验证的逻辑单元，无需依赖下阶段即可完成单元测试
> **参考文档**: [spec.md](spec.md) — 完整Minix3源码级规格定义

---

---

## 前置说明: 环境准备与Mock规则
### Mock 约定
所有硬件相关操作统一使用Mock实现，接口固定：
```rust
// 内存分配Mock: 返回虚拟物理地址
type MockPhysAlloc = dyn FnMut() -> u64;
// 寄存器读写Mock
type MockRegAccess = dyn FnMut(Reg, u64) -> Result<u64, ()>;
// 时钟Mock: 返回当前滴答数
type MockClock = dyn FnMut() -> Clock;
// IPC通信Mock: 消息发送/接收
type MockIpcSend = dyn FnMut(Endpoint, &Message) -> Result<(), IpcError>;
```

---

## 阶段 1: PM — 进程结构复制与初始化

**状态**: ✅ 已完成
**硬件依赖**: 时钟Mock (getticks)
**外部依赖**: 无

### 1.1 任务列表

| # | 任务 | 状态 | 目标文件 |
|---|------|------|---------|
| 1.1 | `Process::fork_from()` 显式构造 | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.2 | `identity.id.index = child_index` | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.3 | `identity.id.pid = child_pid` | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.4 | `identity.endpoint = child_endpoint` | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.5 | `identity.procgrp` 继承 | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.6 | `state.lifecycle = Running` | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.7 | `state.guardianship.parent = parent_index` | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.8 | `resources.child_utime = 0` | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.9 | `resources.started = getticks()` | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.10 | `resources.intervals = [0; NR_ITIMERS]` | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.11 | `resources.flags` 只保留 `TAINTED` | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.12 | 特权进程 `scheduler → Endpoint::RS` | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.13 | `ipc` 全部重置为 default | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.14 | `do_fork_prepare()` 槽位分配 | ✅ | `os/servers/pm/src/mproc/fork.rs` |
| 1.15 | 10 个单元测试 | ✅ | `os/servers/pm/src/mproc/fork.rs` |

---

## 阶段 2: VM — 地址空间克隆与CoW机制实现

**状态**: ❌ 待实现
**硬件依赖**: 物理内存分配Mock、页表操作Mock
**Mock说明**: 所有MMU硬件操作、物理内存实际分配全部使用Mock，仅实现CoW引用计数逻辑
**Minix3 源码参考**: `minix/servers/vm/fork.c`, `region.c`, `phys.c`

### 2.1 任务列表

| # | 任务 | 依赖 | 目标文件 |
|---|------|------|---------|
| 2.1 | 定义 `VmFlags` 位标志 (INUSE/EXITING/VM_INSTANCE) | 无 | `os/servers/vm/src/vmproc.rs` |
| 2.2 | 定义 `VmRegionFlags` 位标志 (WRITABLE/SHARED/ANON/DIRECT) | 无 | `os/servers/vm/src/vmproc.rs` |
| 2.3 | 定义 `PhysBlock` 结构体 (phys_addr, ref_count, first_region) | 无 | `os/servers/vm/src/vmproc.rs` |
| 2.4 | 定义 `PhysRegionRef` 结构体 (phys_block_idx, offset, next_in_list) | 无 | `os/servers/vm/src/vmproc.rs` |
| 2.5 | 定义 `VmRegion` 结构体 (vaddr, length, flags, phys_refs) | 2.3, 2.4 | `os/servers/vm/src/vmproc.rs` |
| 2.6 | 定义 `PageTable` Mock 结构体 | 无 | `os/servers/vm/src/pagetable.rs` |
| 2.7 | 定义 `VmProc` 结构体 | 2.1-2.6 | `os/servers/vm/src/vmproc.rs` |
| 2.8 | 定义 `VmProcTable` 结构体 (procs + phys_blocks) | 2.7 | `os/servers/vm/src/vmproc.rs` |
| 2.9 | 实现 `pb_link()` — 头插法链表 + refcount++ | 2.3, 2.4 | `os/servers/vm/src/cow.rs` |
| 2.10 | 实现 `pb_reference()` — 共享 phys_block | 2.9 | `os/servers/vm/src/cow.rs` |
| 2.11 | 实现 `pb_unreferenced()` — refcount-- + 释放 | 2.9 | `os/servers/vm/src/cow.rs` |
| 2.12 | 实现 `pb_new()` — 创建新 phys_block | 2.3 | `os/servers/vm/src/cow.rs` |
| 2.13 | 实现 `anon_writable()` — CoW 判定 (refcount==1 可写) | 2.3, 2.5 | `os/servers/vm/src/cow.rs` |
| 2.14 | 实现 `mem_cow()` — 写时复制 (alloc + copy + refcount adjust) | 2.10-2.12 | `os/servers/vm/src/cow.rs` |
| 2.15 | 实现 `VmRegion::fork_copy()` — 逐区域 CoW 复制 | 2.5, 2.10 | `os/servers/vm/src/fork.rs` |
| 2.16 | 实现 `map_proc_copy()` — 遍历父进程所有 region | 2.15 | `os/servers/vm/src/fork.rs` |
| 2.17 | 实现 `pt_new()` — 创建新页表 (Mock) | 2.6 | `os/servers/vm/src/pagetable.rs` |
| 2.18 | 实现 `pt_bind()` — 绑定页表到进程 (Mock) | 2.6 | `os/servers/vm/src/pagetable.rs` |
| 2.19 | 实现 `acl_fork()` — ACL 继承规则 | 2.7 | `os/servers/vm/src/fork.rs` |
| 2.20 | 实现 `VmProc::fork_from()` — vmproc 复制逻辑 | 2.7-2.19 | `os/servers/vm/src/fork.rs` |
| 2.21 | 实现 `vm_fork()` — VM fork 主流程 | 2.20 | `os/servers/vm/src/fork.rs` |
| 2.22 | 更新 `os/servers/vm/src/lib.rs` 模块导出 | 2.21 | `os/servers/vm/src/lib.rs` |
| 2.23 | 更新 `os/servers/vm/Cargo.toml` 依赖 | 2.21 | `os/servers/vm/Cargo.toml` |
| 2.24 | 编写 VM 层单元测试 | 2.21 | `os/servers/vm/src/cow.rs` + `fork.rs` |

### 2.2 关键实现细节

#### 2.2.1 `pb_link()` — 头插法链表 + refcount++

对应 Minix3 `minix/servers/vm/phys.c` — `pb_link()`:

```rust
pub fn pb_link(
    phys_blocks: &mut Vec<PhysBlock>,
    phys_refs: &mut Vec<PhysRegionRef>,
    pb_idx: usize,
    offset: VirBytes,
    region_idx: usize,
) -> usize {
    let new_pr_idx = phys_refs.len();
    phys_refs.push(PhysRegionRef {
        phys_block_idx: pb_idx,
        offset,
        next_in_list: phys_blocks[pb_idx].first_region,
    });
    phys_blocks[pb_idx].first_region = Some(new_pr_idx);
    phys_blocks[pb_idx].ref_count += 1;
    new_pr_idx
}
```

#### 2.2.2 `mem_cow()` — 写时复制

对应 Minix3 `minix/servers/vm` — `mem_cow()`:

```rust
pub fn mem_cow(
    region: &mut VmRegion,
    pref_idx: usize,
    phys_blocks: &mut Vec<PhysBlock>,
    phys_refs: &mut Vec<PhysRegionRef>,
    mock_alloc: &mut dyn FnMut() -> u64,
) -> Result<(), VmForkError> {
    let old_pb_idx = region.phys_refs[pref_idx].phys_block_idx;
    let old_phys = phys_blocks[old_pb_idx].phys_addr;

    let new_phys = mock_alloc();

    let new_pb = PhysBlock {
        phys_addr: new_phys,
        ref_count: 0,
        first_region: None,
    };
    let new_pb_idx = phys_blocks.len();
    phys_blocks.push(new_pb);

    phys_blocks[old_pb_idx].ref_count -= 1;

    let new_pr_idx = pb_link(phys_blocks, phys_refs, new_pb_idx,
        region.phys_refs[pref_idx].offset, 0);
    region.phys_refs[pref_idx].phys_block_idx = new_pb_idx;

    Ok(())
}
```

#### 2.2.3 `VmRegion::fork_copy()` — CoW 复制

对应 Minix3 `minix/servers/vm/region.c` — `map_copy_region()`:

```rust
impl VmRegion {
    pub fn fork_copy(
        &self,
        phys_blocks: &mut Vec<PhysBlock>,
        phys_refs: &mut Vec<PhysRegionRef>,
    ) -> Self {
        let mut child = VmRegion {
            vaddr: self.vaddr,
            length: self.length,
            flags: self.flags,
            phys_refs: Vec::with_capacity(self.phys_refs.len()),
        };
        for pref in &self.phys_refs {
            let new_pr_idx = pb_link(
                phys_blocks, phys_refs,
                pref.phys_block_idx, pref.offset, 0,
            );
            child.phys_refs.push(PhysRegionRef {
                phys_block_idx: pref.phys_block_idx,
                offset: pref.offset,
                next_in_list: None,
            });
        }
        child
    }
}
```

### 2.3 必须通过的测试

```
test_pb_link_increments_refcount()
test_pb_unreferenced_decrements_refcount()
test_pb_unreferenced_frees_at_zero()
test_anon_writable_exclusive()
test_anon_writable_shared()
test_mem_cow_creates_new_block()
test_mem_cow_decrements_old_refcount()
test_vm_region_fork_copy_shares_blocks()
test_vm_proc_fork_from()
test_acl_fork_user_inherits()
test_acl_fork_privileged_downgrades()
test_shared_region_not_cow()
```

---

## 阶段 3: Kernel — PCB克隆与上下文伪造实现

**状态**: ❌ 待实现
**硬件依赖**: 寄存器读写Mock、FPU上下文Mock
**Mock说明**: 所有寄存器硬件访问、FPU状态保存/恢复全部使用Mock，仅实现上下文伪造(ret_reg=0)、RTS标志管理、Endpoint生成逻辑
**Minix3 源码参考**: `minix/kernel/system/do_fork.c`

### 3.1 任务列表

| # | 任务 | 依赖 | 目标文件 |
|---|------|------|---------|
| 3.1 | 定义 `RtsFlags` 位标志 (完整 16 个标志) | 无 | `os/kernel/src/proc.rs` |
| 3.2 | 定义 `MiscFlags` 位标志 | 无 | `os/kernel/src/proc.rs` |
| 3.3 | 定义 `KProcess` 结构体 (对齐 struct proc 关键字段) | 3.1, 3.2 | `os/kernel/src/proc.rs` |
| 3.4 | 定义 `ProcTable` 结构体 (procs + generations) | 3.3 | `os/kernel/src/proc.rs` |
| 3.5 | 实现 `make_endpoint(generation, slot)` | 无 | `os/kernel/src/endpoint.rs` |
| 3.6 | 实现 `endpoint_generation(ep)` | 3.5 | `os/kernel/src/endpoint.rs` |
| 3.7 | 实现 `endpoint_slot(ep)` | 3.5 | `os/kernel/src/endpoint.rs` |
| 3.8 | 实现 `is_valid_endpoint()` — generation 验证 | 3.5, 3.4 | `os/kernel/src/endpoint.rs` |
| 3.9 | 实现 `KProcess::sys_fork()` — PCB 克隆 + 上下文伪造 | 3.3-3.8 | `os/kernel/src/system/do_fork.rs` |
| 3.10 | 实现 `ret_reg = 0` — 子进程 fork 返回 0 | 3.9 | `os/kernel/src/system/do_fork.rs` |
| 3.11 | 实现 generation 递增 + 回绕 (到 1 不是 0) | 3.5, 3.9 | `os/kernel/src/system/do_fork.rs` |
| 3.12 | 实现 RTS 标志管理 (NO_QUANTUM/VMINHIBIT/NO_PRIV) | 3.1, 3.9 | `os/kernel/src/system/do_fork.rs` |
| 3.13 | 实现 FPU 保存区修复逻辑 | 3.3, 3.9 | `os/kernel/src/system/do_fork.rs` |
| 3.14 | 实现特权进程降级 | 3.9 | `os/kernel/src/system/do_fork.rs` |
| 3.15 | 实现进程名 "*F" 追加 | 3.9 | `os/kernel/src/system/do_fork.rs` |
| 3.16 | 更新 `os/kernel/src/lib.rs` 模块导出 | 3.9 | `os/kernel/src/lib.rs` |
| 3.17 | 编写 Kernel 层单元测试 | 3.9 | `os/kernel/src/system/do_fork.rs` |

### 3.2 关键实现细节

#### 3.2.1 `make_endpoint()` — Endpoint 生成

对应 Minix3 `minix/include/minix/com.h` — `_ENDPOINT(g, p)`:

```rust
pub const ENDPOINT_GENERATION_SHIFT: u32 = 15;
pub const ENDPOINT_MAX_GENERATION: i32 = 65535;

pub fn make_endpoint(generation: i32, slot: usize) -> Endpoint {
    Endpoint::new((generation << ENDPOINT_GENERATION_SHIFT as i32) | (slot as i32))
}

pub fn endpoint_generation(ep: Endpoint) -> i32 {
    (ep.get() >> ENDPOINT_GENERATION_SHIFT as i32) & 0x7FFF
}

pub fn endpoint_slot(ep: Endpoint) -> usize {
    (ep.get() & 0x7FFF) as usize
}
```

#### 3.2.2 `KProcess::sys_fork()` — PCB 克隆

对应 Minix3 `minix/kernel/system/do_fork.c` — `do_fork()`:

```rust
impl KProcess {
    pub fn sys_fork(
        parent: &KProcess,
        child_slot: usize,
        flags: u32,
        generations: &mut [i32; NR_PROCS],
    ) -> Result<(Self, Endpoint), SysForkError> {
        let gen = generations[child_slot] + 1;
        generations[child_slot] = if gen >= ENDPOINT_MAX_GENERATION { 1 } else { gen };
        let child_endpoint = make_endpoint(generations[child_slot], child_slot);

        let mut child = KProcess {
            slot: child_slot,
            endpoint: child_endpoint,
            rts_flags: parent.rts_flags,
            misc_flags: parent.misc_flags
                & !(MiscFlags::VIRT_TIMER | MiscFlags::PROF_TIMER
                    | MiscFlags::SC_TRACE | MiscFlags::STEP),
            priority: parent.priority,
            quantum_size_ms: parent.quantum_size_ms,
            user_time: 0,
            sys_time: 0,
            virt_left: 0,
            prof_left: 0,
            name: parent.name,
            pending_signals: 0,
            is_system_proc: parent.is_system_proc,
            ret_reg: 0,
            cr3: 0,
            fpu_state: parent.fpu_state.clone(),
        };

        let namelen = child.name.iter().position(|&c| c == 0).unwrap_or(16);
        if namelen + 2 < 16 {
            child.name[namelen] = b'*';
            child.name[namelen + 1] = b'F';
        }

        child.rts_flags |= RtsFlags::NO_QUANTUM;

        if parent.is_system_proc {
            child.is_system_proc = false;
            child.rts_flags |= RtsFlags::NO_PRIV;
        }

        if flags & PFF_VMINHIBIT != 0 {
            child.rts_flags |= RtsFlags::VMINHIBIT;
        }

        child.rts_flags &= !(RtsFlags::SIGNALED | RtsFlags::SIG_PENDING | RtsFlags::P_STOP);
        child.pending_signals = 0;

        Ok((child, child_endpoint))
    }
}
```

### 3.3 必须通过的测试

```
test_make_endpoint()
test_endpoint_generation()
test_endpoint_slot()
test_endpoint_roundtrip()
test_sys_fork_ret_reg_zero()
test_sys_fork_generation_increment()
test_sys_fork_generation_wraparound()
test_sys_fork_rts_no_quantum()
test_sys_fork_rts_vminhibit()
test_sys_fork_privileged_demotion()
test_sys_fork_signal_cleared()
test_sys_fork_name_appended()
test_sys_fork_fpu_preserved()
test_sys_fork_cr3_cleared()
```

---

## 阶段 4: VFS — 文件描述符复制与引用计数实现

**状态**: ❌ 待实现
**硬件依赖**: 磁盘IO Mock
**Mock说明**: 所有磁盘读写、inode操作全部使用Mock，仅实现filp引用计数递增、vnode引用计数管理逻辑
**Minix3 源码参考**: `minix/servers/vfs/misc.c` — `pm_fork()`

### 4.1 任务列表

| # | 任务 | 依赖 | 目标文件 |
|---|------|------|---------|
| 4.1 | 定义 `FpFlags` 位标志 | 无 | `os/servers/vfs/src/fproc.rs` |
| 4.2 | 定义 `FilpRef` 结构体 (index 指向全局 filp 表) | 无 | `os/servers/vfs/src/fproc.rs` |
| 4.3 | 定义 `VNodeRef` 结构体 (index 指向全局 vnode 表) | 无 | `os/servers/vfs/src/fproc.rs` |
| 4.4 | 定义 `Filp` 结构体 (mode, flags, count, vnode, pos) | 4.3 | `os/servers/vfs/src/fproc.rs` |
| 4.5 | 定义 `FProc` 结构体 (对齐 struct fproc) | 4.1-4.4 | `os/servers/vfs/src/fproc.rs` |
| 4.6 | 定义 `VfsProcTable` + 全局 `filps` + `vnodes` | 4.4, 4.5 | `os/servers/vfs/src/fproc.rs` |
| 4.7 | 实现 `dup_vnode()` — v_ref_count++ | 4.3 | `os/servers/vfs/src/fork.rs` |
| 4.8 | 实现 `FProc::fork_from()` — fproc 复制逻辑 | 4.5-4.7 | `os/servers/vfs/src/fork.rs` |
| 4.9 | 实现 filp_count++ 循环 | 4.8 | `os/servers/vfs/src/fork.rs` |
| 4.10 | 实现 fp_lock 保留逻辑 | 4.8 | `os/servers/vfs/src/fork.rs` |
| 4.11 | 实现 `okendpt()` 验证 | 4.5 | `os/servers/vfs/src/fork.rs` |
| 4.12 | 实现 `close_filp()` — filp_count-- 递减 | 4.4 | `os/servers/vfs/src/close.rs` |
| 4.13 | 实现 `put_vnode()` — v_ref_count-- 延迟同步 | 4.3 | `os/servers/vfs/src/close.rs` |
| 4.14 | 更新 `os/servers/vfs/src/lib.rs` 模块导出 | 4.8 | `os/servers/vfs/src/lib.rs` |
| 4.15 | 更新 `os/servers/vfs/Cargo.toml` 依赖 | 4.8 | `os/servers/vfs/Cargo.toml` |
| 4.16 | 编写 VFS 层单元测试 | 4.8 | `os/servers/vfs/src/fork.rs` + `close.rs` |

### 4.2 关键实现细节

#### 4.2.1 `FProc::fork_from()` — fproc 复制

对应 Minix3 `minix/servers/vfs/misc.c` — `pm_fork()`:

```rust
impl FProc {
    pub fn fork_from(
        parent: &FProc,
        child_pid: Pid,
        child_endpoint: Endpoint,
        filps: &mut Vec<Filp>,
        vnodes: &mut Vec<VNodeEntry>,
    ) -> Self {
        let mut child_filps = parent.filps;
        for filp_ref in child_filps.iter().flatten() {
            if filp_ref.index < filps.len() {
                filps[filp_ref.index].count += 1;
            }
        }

        if let Some(ref rd) = parent.root_dir {
            if rd.index < vnodes.len() {
                vnodes[rd.index].ref_count += 1;
            }
        }
        if let Some(ref wd) = parent.work_dir {
            if wd.index < vnodes.len() {
                vnodes[wd.index].ref_count += 1;
            }
        }

        FProc {
            flags: FpFlags::empty(),
            pid: child_pid,
            endpoint: child_endpoint,
            root_dir: parent.root_dir.clone(),
            work_dir: parent.work_dir.clone(),
            filps: child_filps,
            cloexec_set: parent.cloexec_set,
            real_uid: parent.real_uid,
            eff_uid: parent.eff_uid,
            real_gid: parent.real_gid,
            eff_gid: parent.eff_gid,
            ngroups: parent.ngroups,
            supplemental_groups: parent.supplemental_groups,
            umask: parent.umask,
            name: parent.name,
        }
    }
}
```

#### 4.2.2 `close_filp()` — 引用计数递减

对应 Minix3 `minix/servers/vfs/filedes.c` — `close_filp()`:

```rust
impl Filp {
    pub fn close(&mut self, vnodes: &mut Vec<VNodeEntry>) -> bool {
        self.count -= 1;
        if self.count == 0 {
            if let Some(ref vn) = self.vnode {
                vnodes[vn.index].ref_count -= 1;
            }
            self.vnode = None;
            return true;
        }
        false
    }
}
```

### 4.3 必须通过的测试

```
test_fproc_fork_from_filp_count_incremented()
test_fproc_fork_from_vnode_ref_count_incremented()
test_fproc_fork_from_flags_cleared()
test_fproc_fork_from_pid_endpoint_set()
test_fproc_fork_from_cloexec_inherited()
test_close_filp_decrements_count()
test_close_filp_releases_vnode_at_zero()
test_close_filp_keeps_vnode_above_zero()
test_dup_vnode_increments_ref_count()
test_okendpt_validates_endpoint()
```

---

## 阶段 5: PM — do_fork完整状态机与跨服务协调实现

**状态**: ❌ 待实现
**硬件依赖**: IPC通信Mock
**Mock说明**: 所有跨服务IPC通信使用Mock实现，仅实现状态机流转、失败回滚、异步通知逻辑
**Minix3 源码参考**: `minix/servers/pm/forkexit.c` — `do_fork()`

### 5.1 任务列表

| # | 任务 | 依赖 | 目标文件 |
|---|------|------|---------|
| 5.1 | 实现 `vm_fork()` IPC 调用 (模拟) | 阶段 2 | `os/servers/pm/src/mproc/fork.rs` |
| 5.2 | 实现 vm_fork 失败时的回滚 | 5.1 | `os/servers/pm/src/mproc/fork.rs` |
| 5.3 | 实现 `tell_vfs()` 异步通知 (模拟) | 阶段 4 | `os/servers/pm/src/mproc/fork.rs` |
| 5.4 | 实现 `VFS_CALL` 标志管理 | 5.3 | `os/servers/pm/src/mproc/fork.rs` |
| 5.5 | 实现 `do_fork()` 完整状态机 | 5.1-5.4 | `os/servers/pm/src/mproc/fork.rs` |
| 5.6 | 实现 SUSPEND 返回机制 | 5.5 | `os/servers/pm/src/mproc/fork.rs` |
| 5.7 | 实现 `do_fork_reply()` VFS 回复处理 | 5.5 | `os/servers/pm/src/mproc/fork.rs` |
| 5.8 | 实现调度失败回滚 (exit_proc) | 5.7 | `os/servers/pm/src/mproc/fork.rs` |
| 5.9 | 编写 PM 状态机单元测试 | 5.5 | `os/servers/pm/src/mproc/fork.rs` |

### 5.2 关键实现细节

#### 5.2.1 `do_fork()` 完整状态机

```rust
impl<'a> PmContext<'a> {
    pub fn do_fork(&mut self) -> Result<i32, ForkError> {
        let prepare = self.do_fork_prepare()?;

        let vm_result = self.call_vm_fork(
            self.current_proc().identity.endpoint,
            prepare.child_index,
        ).map_err(|e| {
            self.table.release_slot(prepare.child_index);
            e
        })?;

        self.fork_child_from_parent(
            prepare.child_index,
            prepare.child_pid,
            vm_result.child_endpoint,
        );

        self.tell_vfs(VfsPmForkRequest {
            child_endpoint: vm_result.child_endpoint,
            parent_endpoint: self.current_proc().identity.endpoint,
            child_pid: prepare.child_pid,
            real_uid: -1,
            real_gid: -1,
        });

        Ok(SUSPEND)
    }
}
```

### 5.3 必须通过的测试

```
test_do_fork_full_flow()
test_do_fork_vm_fork_failure_rollback()
test_do_fork_returns_suspend()
test_do_fork_reply_wakes_parent()
test_do_fork_scheduling_failure_rollback()
```

---

## 阶段 6: 全链路集成与验证测试

**状态**: ❌ 待实现
**硬件依赖**: 全栈Mock环境
**Mock说明**: 使用统一的Mock层连接四个服务，模拟完整的跨服务交互流程

### 6.1 任务列表

| # | 任务 | 依赖 | 目标文件 |
|---|------|------|---------|
| 6.1 | 定义 `ForkCoordinator` — 全局 fork 协调器 | 阶段 2-5 | `os/servers/pm/src/mproc/coordinator.rs` |
| 6.2 | 实现 PM → VM IPC 消息发送 | 6.1 | `os/servers/pm/src/mproc/coordinator.rs` |
| 6.3 | 实现 VM → Kernel sys_fork 调用 | 6.1 | `os/servers/pm/src/mproc/coordinator.rs` |
| 6.4 | 实现 VM pt_bind 后清除 RTS_VMINHIBIT | 6.3 | `os/servers/pm/src/mproc/coordinator.rs` |
| 6.5 | 实现 PM → VFS 异步通知 | 6.1 | `os/servers/pm/src/mproc/coordinator.rs` |
| 6.6 | 实现 VFS 回复后唤醒父进程 | 6.5 | `os/servers/pm/src/mproc/coordinator.rs` |
| 6.7 | 编写集成测试: 四表 endpoint 一致性 | 6.1-6.6 | 集成测试文件 |
| 6.8 | 编写集成测试: CoW 验证 | 6.7 | 集成测试文件 |
| 6.9 | 编写集成测试: filp 引用计数验证 | 6.7 | 集成测试文件 |
| 6.10 | 编写集成测试: vnode 引用计数验证 | 6.7 | 集成测试文件 |
| 6.11 | 编写集成测试: ret_reg = 0 验证 | 6.7 | 集成测试文件 |
| 6.12 | 编写集成测试: endpoint generation 验证 | 6.7 | 集成测试文件 |

### 6.2 必须通过的测试

```
test_full_fork_flow()
test_cow_write_triggers_copy()
test_fork_close_does_not_close_file()
test_endpoint_generation_increment()
test_ret_reg_zero()
test_shared_memory_not_cow()
test_privileged_process_demotion()
test_four_table_endpoint_consistency()
test_pid_pm_vfs_consistency()
```

---

## 附录: 文件变更总览

### 新建文件

| 文件 | 阶段 | 说明 |
|------|------|------|
| `os/servers/vm/src/vmproc.rs` | 2 | VmProc, VmRegion, PhysBlock, PhysRegionRef |
| `os/servers/vm/src/cow.rs` | 2 | pb_reference, pb_unreferenced, anon_writable, mem_cow |
| `os/servers/vm/src/fork.rs` | 2 | vm_fork, map_proc_copy, VmRegion::fork_copy |
| `os/servers/vm/src/pagetable.rs` | 2 | PageTable Mock, pt_new, pt_bind |
| `os/kernel/src/endpoint.rs` | 3 | make_endpoint, endpoint_generation, endpoint_slot |
| `os/kernel/src/system/do_fork.rs` | 3 | KProcess::sys_fork |
| `os/servers/vfs/src/fproc.rs` | 4 | FProc, Filp, VNodeRef, FilpRef |
| `os/servers/vfs/src/fork.rs` | 4 | FProc::fork_from, dup_vnode, okendpt |
| `os/servers/vfs/src/close.rs` | 4 | close_filp, put_vnode |
| `os/servers/pm/src/mproc/coordinator.rs` | 6 | ForkCoordinator |

### 修改文件

| 文件 | 阶段 | 说明 |
|------|------|------|
| `os/servers/vm/src/lib.rs` | 2 | 添加模块导出 |
| `os/servers/vm/Cargo.toml` | 2 | 添加 minix-types, bitflags 依赖 |
| `os/kernel/src/proc.rs` | 3 | 重写 KProcess, 添加 RtsFlags, ProcTable |
| `os/kernel/src/system.rs` | 3 | 添加 do_fork 模块 |
| `os/kernel/src/lib.rs` | 3 | 添加 endpoint, system 模块 |
| `os/servers/vfs/src/lib.rs` | 4 | 添加模块导出 |
| `os/servers/vfs/Cargo.toml` | 4 | 添加 minix-types, bitflags 依赖 |
| `os/servers/pm/src/mproc/fork.rs` | 5 | 添加 do_fork 完整状态机 |
| `os/servers/pm/src/mproc/mod.rs` | 5 | 添加 coordinator 模块 |
