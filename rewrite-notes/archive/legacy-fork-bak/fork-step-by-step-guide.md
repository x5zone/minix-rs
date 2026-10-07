# Minix3 Fork 全栈纵向切片 — 逐步开发指南

> **顺序**: PM → VM → VFS → Kernel → 跨服务协调
> **原则**: 每阶段必须触及的逻辑点检查清单
> **参考**: [fork-all-layers-deep-analysis.md](fork-all-layers-deep-analysis.md)

---

## 阶段 1：PM — 进程结构复制与初始化

**状态**: ✅ 已完成

### 1.1 必须触及的逻辑点检查清单

- [x] `Process::fork_from()` — 显式构造子进程
- [x] `identity.id.index` = `child_index`（不是父进程索引）
- [x] `identity.id.pid` = `child_pid`
- [x] `identity.endpoint` = `child_endpoint`
- [x] `identity.procgrp` = 父进程的 procgrp（继承）
- [x] `state.lifecycle` = `Lifecycle::Running`
- [x] `state.guardianship.parent` = 父进程索引
- [x] `resources.child_utime` = 0
- [x] `resources.child_stime` = 0
- [x] `resources.started` = `getticks()`
- [x] `resources.intervals` = `[0; NR_ITIMERS]`
- [x] `resources.flags` 只保留 `TAINTED`
- [x] 特权进程 `scheduler` → `Endpoint::RS`
- [x] `ipc` 全部重置为 default

### 1.2 文件

- `os/servers/pm/src/mproc/fork.rs` — `Process::fork_from()`
- `os/servers/pm/src/mproc/pid_gen.rs` — PID 生成器
- `os/servers/pm/src/mproc/table.rs` — 进程表管理

---

## 阶段 2：PM — do_fork 完整状态机

**状态**: ❌ 待实现

### 2.1 必须触及的逻辑点检查清单

- [ ] `do_fork_prepare()` — 槽位分配（已实现）
- [ ] `vm_fork()` 调用 — IPC 发送 VM_FORK 消息
- [ ] vm_fork 失败时的回滚 — 释放已分配的槽位
- [ ] vm_fork 成功后不可失败 — 注释/断言标记
- [ ] `fork_child_from_parent()` — 进程结构复制（已实现）
- [ ] `tell_vfs()` — 异步发送 VFS_PM_FORK
- [ ] `VFS_CALL` 标志管理 — 防止重复发送
- [ ] 返回 `SUSPEND` — 父进程挂起
- [ ] `do_fork_reply()` — VFS 回复后唤醒父进程
- [ ] 追踪器处理 — `SIGSTOP`

### 2.2 关键实现

```rust
impl<'a> PmContext<'a> {
    pub fn do_fork(&mut self) -> Result<i32, ForkError> {
        // ① 检查和槽位分配（可以失败）
        let prepare = self.do_fork_prepare()?;

        // ② VM fork（可以失败，需回滚）
        let vm_result = self.call_vm_fork(
            self.current_proc().identity.endpoint,
            prepare.child_index,
        ).map_err(|e| {
            self.table.release_slot(prepare.child_index);
            e
        })?;

        // ③ 进程结构初始化（不能失败！）
        // ⚠️ PM may not fail fork after call to vm_fork()
        self.fork_child_from_parent(
            prepare.child_index,
            prepare.child_pid,
            vm_result.child_endpoint,
        );

        // ④ 通知 VFS（异步）
        self.tell_vfs(VfsPmForkRequest {
            child_endpoint: vm_result.child_endpoint,
            parent_endpoint: self.current_proc().identity.endpoint,
            child_pid: prepare.child_pid,
            real_uid: -1,
            real_gid: -1,
        });

        // ⑤ 返回 SUSPEND
        Ok(SUSPEND)
    }
}
```

### 2.3 文件

- `os/servers/pm/src/mproc/fork.rs` — `do_fork()` 完整流程

---

## 阶段 3：VM — vmproc 结构体与地址空间克隆

**状态**: ❌ 待实现

### 3.1 必须触及的逻辑点检查清单

- [ ] `VmProc` 结构体定义 — 对齐 `vmproc.h`
- [ ] `VmFlags` 位标志 — `IN_USE`, `EXITING`, `VM_INSTANCE`
- [ ] `VmRegion` 结构体 — 对齐 `vir_region`
- [ ] `VmRegionFlags` — `WRITABLE`, `SHARED`, `ANON`, `DIRECT`
- [ ] `PhysBlock` 结构体 — **必须包含 `ref_count` 字段**
- [ ] `PhysRegionRef` 结构体 — 虚拟区域到物理块的桥梁
- [ ] `PageTable` 结构体 — Mock 版本
- [ ] `VmProc::fork_from()` — vmproc 复制逻辑
- [ ] **`pb_reference()` — 共享 phys_block，refcount++（CoW 核心）**
- [ ] **`pb_unreferenced()` — 解除引用，refcount--**
- [ ] **`mem_cow()` — 写时复制：分配新页 + 复制内容 + refcount 调整**
- [ ] **`anon_writable()` — CoW 判定：refcount==1 可写，>=2 只读**
- [ ] `map_proc_copy()` — 遍历父进程所有 region
- [ ] `map_copy_region()` — 逐区域复制（创建新 vir_region，共享 phys_block）
- [ ] `pt_new()` — 创建新页表
- [ ] `pt_bind()` — 绑定页表到进程
- [ ] `acl_fork()` — ACL 继承规则
- [ ] `sys_fork()` 调用 — 传入 `PFF_VMINHIBIT`
- [ ] `handle_memory_once()` — fork 消息页的 CoW 处理

### 3.2 CoW 核心实现

```rust
impl PhysBlock {
    /// pb_reference: 共享物理页，refcount++
    /// 这是 CoW 的核心操作 — 不复制 4KB 物理页，只增加引用计数
    pub fn reference(&mut self, region: &mut VmRegion, offset: VirBytes) -> usize {
        self.ref_count += 1;
        let pr_idx = region.add_phys_ref(self.index(), offset);
        pr_idx
    }

    /// pb_unreferenced: 解除引用，refcount--
    /// 如果 refcount 降到 0，释放物理内存
    pub fn unreferenced(&mut self) -> bool {
        self.ref_count -= 1;
        self.ref_count == 0  // 返回 true 表示物理页应释放
    }
}

impl VmRegion {
    /// map_copy_region: CoW 复制单个区域
    /// 创建新的 VmRegion，但共享所有 PhysBlock
    pub fn fork_copy(&self, child_slot: usize, phys_blocks: &mut Vec<PhysBlock>) -> Self {
        let mut child = VmRegion {
            vaddr: self.vaddr,
            length: self.length,
            flags: self.flags,
            phys_refs: Vec::new(),
        };
        for pref in &self.phys_refs {
            phys_blocks[pref.phys_block_idx].ref_count += 1;  // pb_reference!
            child.phys_refs.push(PhysRegionRef {
                phys_block_idx: pref.phys_block_idx,
                offset: pref.offset,
            });
        }
        child
    }
}

/// anon_writable: CoW 判定
/// refcount == 1 → 可写（独占）
/// refcount >= 2 → 只读（CoW 保护）
pub fn is_writable(region: &VmRegion, phys_block: &PhysBlock) -> bool {
    region.flags.contains(VmRegionFlags::WRITABLE) && phys_block.ref_count == 1
}

/// mem_cow: 写时复制
/// 1. 分配新物理页
/// 2. 复制旧页内容到新页
/// 3. 解除旧 phys_block 引用 (refcount--)
/// 4. 链接到新 phys_block (refcount = 1)
pub fn mem_cow(
    region: &mut VmRegion,
    pref_idx: usize,
    phys_blocks: &mut Vec<PhysBlock>,
    mock_phys_alloc: &mut dyn FnMut() -> u64,
) -> Result<(), VmForkError> {
    let old_pb_idx = region.phys_refs[pref_idx].phys_block_idx;
    let old_phys = phys_blocks[old_pb_idx].phys_addr;

    // 1. 分配新物理页
    let new_phys = mock_phys_alloc();

    // 2. 复制内容（Mock: 记录复制操作）
    // sys_abscopy(old_phys, new_phys, VM_PAGE_SIZE)

    // 3. 创建新 phys_block
    let new_pb = PhysBlock {
        phys_addr: new_phys,
        ref_count: 1,
        first_region: None,
    };
    let new_pb_idx = phys_blocks.len();
    phys_blocks.push(new_pb);

    // 4. 解除旧引用
    phys_blocks[old_pb_idx].ref_count -= 1;

    // 5. 更新 region 的引用
    region.phys_refs[pref_idx].phys_block_idx = new_pb_idx;

    Ok(())
}
```

### 3.3 VM fork 主流程

```rust
pub fn vm_fork(
    parent: &VmProc,
    child_slot: usize,
    phys_blocks: &mut Vec<PhysBlock>,
) -> Result<VmProc, VmForkError> {
    // 1. 验证参数
    if child_slot >= NR_PROCS {
        return Err(VmForkError::InvalidSlot);
    }

    // 2. 构造子进程 vmproc
    let mut child = VmProc {
        flags: parent.flags & VmFlags::IN_USE,  // 只继承 IN_USE
        endpoint: Endpoint::NONE,  // 暂时无效
        page_table: PageTable::new(),
        regions: Vec::new(),
        region_top: parent.region_top,
        acl: if parent.acl == USER_ACL { USER_ACL } else { NO_ACL },
        slot: child_slot,
        total: parent.total,
        total_max: parent.total_max,
        minor_page_fault: 0,
        major_page_fault: 0,
    };

    // 3. CoW 复制内存区域（核心！）
    for region in &parent.regions {
        child.regions.push(region.fork_copy(child_slot, phys_blocks));
    }

    Ok(child)
}
```

### 3.4 文件

- `os/servers/vm/src/vmproc.rs`（新建）— VmProc, VmRegion, PhysBlock
- `os/servers/vm/src/fork.rs`（新建）— vm_fork(), mem_cow()
- `os/servers/vm/src/cow.rs`（新建）— pb_reference, pb_unreferenced, anon_writable

---

## 阶段 4：Kernel — KProcess 结构体与 sys_fork

**状态**: ❌ 待实现

### 4.1 必须触及的逻辑点检查清单

- [ ] `KProcess` 结构体 — 对齐 `struct proc` 关键字段
- [ ] `RtsFlags` 位标志 — 完整定义所有 16 个标志
- [ ] `MiscFlags` 位标志 — VIRT_TIMER, PROF_TIMER, SC_TRACE, STEP
- [ ] `ProcTable` 结构体 — 进程表 + generation 数组
- [ ] **`make_endpoint()` — `_ENDPOINT(generation, slot)` 精确算法**
- [ ] **`endpoint_generation()` — 从 endpoint 提取 generation**
- [ ] **`endpoint_slot()` — 从 endpoint 提取 slot**
- [ ] **`KProcess::sys_fork()` — PCB 克隆 + 上下文伪造**
- [ ] **`ret_reg = 0` — 子进程 fork 返回 0（rax/eax 伪造）**
- [ ] **generation 递增 — 每次槽位重用 +1，溢出回绕到 1**
- [ ] `RTS_NO_QUANTUM` 设置 — 子进程不可运行
- [ ] `RTS_VMINHIBIT` 条件设置 — PFF_VMINHIBIT 标志
- [ ] `RTS_NO_PRIV` 条件设置 — 特权进程降级
- [ ] `RTS_SIGNALED | RTS_SIG_PENDING | RTS_P_STOP` 清除
- [ ] `p_pending` 信号集清空
- [ ] `cr3` 页表基址清零
- [ ] FPU 保存区修复 — 保留子槽自己的缓冲区
- [ ] 进程名追加 `"*F"`
- [ ] 记账统计全部归零
- [ ] `is_valid_endpoint()` — generation 验证

### 4.2 sys_fork 核心实现

```rust
impl KProcess {
    pub fn sys_fork(
        parent: &KProcess,
        child_slot: usize,
        flags: u32,
        generations: &mut [i32; NR_PROCS],
    ) -> Result<(Self, Endpoint), SysForkError> {
        // 1. 递增 generation
        generations[child_slot] += 1;
        if generations[child_slot] >= ENDPOINT_MAX_GENERATION {
            generations[child_slot] = 1;  // 回绕到 1，不是 0
        }
        let child_endpoint = make_endpoint(generations[child_slot], child_slot);

        // 2. 显式构造子进程（对应 *rpc = *rpp + 修复）
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
            ret_reg: 0,              // ← 上下文伪造！子进程 fork 返回 0
            cr3: 0,                  // 页表基址清零
            fpu_state: parent.fpu_state.clone(),  // FPU 内容复制
        };

        // 3. 追加 "*F"
        let namelen = child.name.iter().position(|&c| c == 0).unwrap_or(16);
        if namelen + 2 < 16 {
            child.name[namelen] = b'*';
            child.name[namelen + 1] = b'F';
        }

        // 4. 设置不可运行
        child.rts_flags |= RtsFlags::NO_QUANTUM;

        // 5. 特权进程降级
        if parent.is_system_proc {
            child.is_system_proc = false;
            child.rts_flags |= RtsFlags::NO_PRIV;
        }

        // 6. VM 抑制
        if flags & PFF_VMINHIBIT != 0 {
            child.rts_flags |= RtsFlags::VMINHIBIT;
        }

        // 7. 清除信号和追踪
        child.rts_flags &= !(RtsFlags::SIGNALED | RtsFlags::SIG_PENDING | RtsFlags::P_STOP);
        child.pending_signals = 0;

        Ok((child, child_endpoint))
    }
}

/// Endpoint 生成：_ENDPOINT(generation, slot)
/// generation 占高 15 位，slot 占低 17 位
pub fn make_endpoint(generation: i32, slot: usize) -> Endpoint {
    Endpoint::new((generation << 15) | (slot as i32))
}

pub fn endpoint_generation(ep: Endpoint) -> i32 {
    (ep.get() >> 15) & 0x7FFF
}

pub fn endpoint_slot(ep: Endpoint) -> usize {
    (ep.get() & 0x7FFF) as usize
}
```

### 4.3 文件

- `os/kernel/src/proc.rs`（重写）— KProcess, ProcTable, RtsFlags
- `os/kernel/src/system/do_fork.rs`（新建）— sys_fork 实现
- `os/kernel/src/endpoint.rs`（新建）— endpoint 生成/解析

---

## 阶段 5：VFS — fproc 结构体与 pm_fork

**状态**: ❌ 待实现

### 5.1 必须触及的逻辑点检查清单

- [ ] `FProc` 结构体 — 对齐 `struct fproc`
- [ ] `FpFlags` 位标志 — SRV_PROC, REVIVED, SESLDR, PENDING, EXITING, PM_WORK
- [ ] `Filp` 结构体 — **必须包含 `count` 字段（引用计数）**
- [ ] `Filp` 包含 `pos` 字段（共享的文件偏移量）
- [ ] `VNodeRef` 结构体 — **必须包含 `ref_count` 字段**
- [ ] `FProc::fork_from()` — fproc 复制逻辑
- [ ] **filp 引用计数递增 — `filp_count++`（核心！）**
- [ ] **vnode 引用计数递增 — `dup_vnode()` → `v_ref_count++`**
- [ ] `fp_flags` 清除为 `FP_NOFLAGS`
- [ ] `fp_pid` 和 `fp_endpoint` 设置为新值
- [ ] `fp_cloexec_set` 完整继承
- [ ] `fp_lock` 保留子进程自己的（不继承父进程的）
- [ ] close 时的引用计数递减 — `filp_count--`，降到 0 才真正关闭
- [ ] `okendpt()` 验证 — 父进程 endpoint 验证
- [ ] 子进程 slot 验证 — `fp_pid == PID_FREE`

### 5.2 pm_fork 核心实现

```rust
impl FProc {
    pub fn fork_from(
        parent: &FProc,
        child_pid: Pid,
        child_endpoint: Endpoint,
        filps: &mut Vec<Filp>,
        vnodes: &mut Vec<VNodeRef>,
    ) -> Self {
        // 1. 复制文件描述符表（共享 filp，增加引用计数）
        let mut child_filps = parent.filps;
        for filp_ref in child_filps.iter_mut().flatten() {
            if filp_ref.index < filps.len() {
                filps[filp_ref.index].count += 1;  // filp_count++！
            }
        }

        // 2. 增加目录 vnode 引用计数
        if let Some(ref rd) = parent.root_dir {
            if rd.index < vnodes.len() {
                vnodes[rd.index].ref_count += 1;   // v_ref_count++！
            }
        }
        if let Some(ref wd) = parent.work_dir {
            if wd.index < vnodes.len() {
                vnodes[wd.index].ref_count += 1;   // v_ref_count++！
            }
        }

        // 3. 构造子进程 fproc
        FProc {
            flags: FpFlags::empty(),  // 清除所有标志
            pid: child_pid,
            endpoint: child_endpoint,
            root_dir: parent.root_dir.clone(),
            work_dir: parent.work_dir.clone(),
            filps: child_filps,
            cloexec_set: parent.cloexec_set,  // 完整继承
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

### 5.3 close 时的引用计数递减

```rust
impl Filp {
    pub fn close(&mut self, vnodes: &mut Vec<VNodeRef>) -> bool {
        self.count -= 1;
        if self.count == 0 {
            // 最后一个引用消失，释放 vnode
            if let Some(ref vn) = self.vnode {
                vnodes[vn.index].ref_count -= 1;
                if vnodes[vn.index].ref_count == 0 {
                    // 通知底层 FS 关闭 inode
                    return true;
                }
            }
            self.vnode = None;
        }
        self.count == 0
    }
}
```

### 5.4 文件

- `os/servers/vfs/src/fproc.rs`（新建）— FProc, Filp, VNodeRef
- `os/servers/vfs/src/fork.rs`（新建）— pm_fork 实现
- `os/servers/vfs/src/close.rs`（新建）— close 时的引用计数递减

---

## 阶段 6：跨服务协调与集成测试

**状态**: ❌ 待实现

### 6.1 必须触及的逻辑点检查清单

- [ ] `ForkCoordinator` — 全局 fork 协调器
- [ ] PM → VM IPC 消息发送
- [ ] VM → Kernel sys_fork 调用
- [ ] VM pt_bind 后清除 RTS_VMINHIBIT
- [ ] PM → VFS 异步通知
- [ ] VFS 回复后唤醒父进程
- [ ] **四份进程表 endpoint 一致性验证**
- [ ] **PID 在 PM 和 VFS 中一致性验证**
- [ ] **子进程不可运行验证（RTS_NO_QUANTUM + RTS_VMINHIBIT）**
- [ ] **CoW 验证：fork 后写入触发页面复制**
- [ ] **filp 引用计数验证：fork 后 close 不关闭文件**
- [ ] **vnode 引用计数验证：fork 后 close 不释放 vnode**

### 6.2 集成测试

```rust
#[test]
fn test_full_fork_flow() {
    let mut coord = create_test_coordinator();
    let result = coord.do_fork().unwrap();

    // 1. 四份进程表 endpoint 一致
    let pm_ep = coord.pm.table.get(result.child_index).unwrap().identity.endpoint;
    let vm_ep = coord.vm_table.procs[result.child_index].endpoint;
    let kernel_ep = coord.kernel_table.procs[result.child_index].as_ref().unwrap().endpoint;
    let vfs_ep = coord.vfs_table.procs[result.child_index].endpoint;
    assert_eq!(pm_ep, vm_ep);
    assert_eq!(pm_ep, kernel_ep);
    assert_eq!(pm_ep, vfs_ep);

    // 2. 子进程不可运行
    let kernel_child = coord.kernel_table.procs[result.child_index].as_ref().unwrap();
    assert!(kernel_child.rts_flags.contains(RtsFlags::NO_QUANTUM));
    assert!(kernel_child.rts_flags.contains(RtsFlags::VMINHIBIT));
    assert_eq!(kernel_child.ret_reg, 0);

    // 3. CoW 验证：共享 phys_block
    let parent_regions = &coord.vm_table.procs[coord.pm.current].regions;
    let child_regions = &coord.vm_table.procs[result.child_index].regions;
    for (p, c) in parent_regions.iter().zip(child_regions.iter()) {
        for (pp, cp) in p.phys_refs.iter().zip(c.phys_refs.iter()) {
            assert_eq!(pp.phys_block_idx, cp.phys_block_idx);  // 共享同一 phys_block
        }
    }
    assert!(coord.phys_blocks[0].ref_count >= 2);  // refcount >= 2

    // 4. filp 引用计数验证
    let parent_filps = &coord.vfs_table.procs[coord.pm.current].filps;
    let child_filps = &coord.vfs_table.procs[result.child_index].filps;
    for (i, (p, c)) in parent_filps.iter().zip(child_filps.iter()).enumerate() {
        assert_eq!(p, c, "fd {} should be shared", i);
    }
    // 所有打开的 fd 的 filp_count 应该是 2
    for filp_ref in child_filps.iter().flatten() {
        assert_eq!(coord.filps[filp_ref.index].count, 2);
    }
}

#[test]
fn test_cow_write_triggers_copy() {
    let mut coord = create_test_coordinator();
    let result = coord.do_fork().unwrap();

    // 写入子进程的 CoW 页面
    let child_regions = &mut coord.vm_table.procs[result.child_index].regions;
    let pb_idx_before = child_regions[0].phys_refs[0].phys_block_idx;
    let old_refcount = coord.phys_blocks[pb_idx_before].ref_count;

    // 触发 CoW
    mem_cow(&mut child_regions[0], 0, &mut coord.phys_blocks, &mut coord.phys_alloc).unwrap();

    // 验证：子进程现在有独立的 phys_block
    let pb_idx_after = child_regions[0].phys_refs[0].phys_block_idx;
    assert_ne!(pb_idx_before, pb_idx_after);
    assert_eq!(coord.phys_blocks[pb_idx_after].ref_count, 1);  // 新页 refcount=1
    assert_eq!(coord.phys_blocks[pb_idx_before].ref_count, old_refcount - 1);  // 旧页 refcount--
}

#[test]
fn test_fork_close_does_not_close_file() {
    let mut coord = create_test_coordinator();
    let result = coord.do_fork().unwrap();

    // 父进程 close fd 0
    let parent = &mut coord.vfs_table.procs[coord.pm.current];
    let filp_idx = parent.filps[0].unwrap().index;
    parent.filps[0] = None;

    // close_filp
    coord.filps[filp_idx].count -= 1;
    assert_eq!(coord.filps[filp_idx].count, 1);  // 还有子进程引用
    assert!(coord.filps[filp_idx].vnode.is_some());  // 文件未关闭
}
```

### 6.3 文件

- `os/servers/pm/src/mproc/coordinator.rs`（新建）— ForkCoordinator
- 集成测试文件

---

## 附录：Mock 边界

| 层次 | 允许 Mock | 不允许 Mock |
|------|----------|------------|
| 物理页分配 | `alloc_mem()` → `Vec<u64>` | — |
| 页表硬件 | `pt_writemap()` → `Vec<u64>` | — |
| FPU 上下文 | `save_fpu()` → `Vec<u8>` | — |
| CPU 调度 | `enqueue/dequeue` → 空操作 | — |
| CR3/TTBR 加载 | `switch_address_space()` → 空操作 | — |
| VM region 遍历 | — | **必须实现 AVL 或 Vec 遍历** |
| CoW refcount | — | **必须实现 pb_reference/pb_unreferenced** |
| filp 引用计数 | — | **必须实现 filp_count++/--** |
| vnode 引用计数 | — | **必须实现 v_ref_count++/--** |
| PCB 克隆 | — | **必须实现显式构造 + 修复** |
| retreg 伪造 | — | **必须实现 ret_reg = 0** |
| endpoint generation | — | **必须实现递增 + 回绕算法** |
