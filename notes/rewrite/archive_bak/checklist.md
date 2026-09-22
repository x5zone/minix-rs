# Fork 全栈纵向切片 — 逻辑点检查清单

> **用途**: 每个阶段完成后，逐项检查是否触及了所有必须的逻辑点
> **规则**: 如果规划中没有提到 vm_region 遍历、filp 引用计数、PCB 现场伪造等硬核点，即视为审计失败
> **参考**: [spec.md](spec.md), [tasks.md](tasks.md)

---

## 一、PM 层检查清单

### 1.1 进程结构复制 (阶段 1 — ✅ 已完成)

| # | 逻辑点 | Minix3 源码 | Rust 实现 | 状态 |
|---|--------|------------|----------|------|
| P-01 | `identity.id.index = child_index` (不是父进程索引) | `forkexit.c:84` | `fork.rs:214` | ✅ |
| P-02 | `identity.id.pid = child_pid` | `forkexit.c:119` | `fork.rs:215` | ✅ |
| P-03 | `identity.endpoint = child_endpoint` | `forkexit.c:112` | `fork.rs:216` | ✅ |
| P-04 | `identity.procgrp` 继承 | `*rmc = *rmp` | `fork.rs:219` | ✅ |
| P-05 | `identity.name` 继承 | `*rmc = *rmp` | `fork.rs:220` | ✅ |
| P-06 | `state.lifecycle = Running` | `mp_flags \|= IN_USE` | `fork.rs:226` | ✅ |
| P-07 | `state.guardianship.parent = parent_index` | `mp_parent = who_p` | `fork.rs:229-231` | ✅ |
| P-08 | `state.trace` 清除 (除非 TO_TRACEFORK) | `forkexit.c:91-95` | `fork.rs:232` | ✅ |
| P-09 | `resources.child_utime = 0` | `forkexit.c:107` | `fork.rs:244` | ✅ |
| P-10 | `resources.child_stime = 0` | `forkexit.c:107` | `fork.rs:245` | ✅ |
| P-11 | `resources.started = getticks()` | `forkexit.c:114` | `fork.rs:246` | ✅ |
| P-12 | `resources.intervals = [0; NR_ITIMERS]` | `forkexit.c:113` | `fork.rs:248` | ✅ |
| P-13 | `resources.flags` 只保留 TAINTED | `forkexit.c:106` | `fork.rs:260-266` | ✅ |
| P-14 | 特权进程 `scheduler → Endpoint::RS` | `forkexit.c:100-103` | `fork.rs:253-257` | ✅ |
| P-15 | `ipc` 全部重置为 default | 无对应 (Rust 新增) | `fork.rs:271` | ✅ |

### 1.2 do_fork 完整状态机 (阶段 5 — ❌ 待实现)

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| P-16 | 容量检查: `procs_in_use == NR_PROCS → EAGAIN` | `forkexit.c:59-65` | ❌ |
| P-17 | LAST_FEW 保留: 非root用户保留2个槽位 | `forkexit.c:62-64` | ✅ (do_fork_prepare) |
| P-18 | 轮转查找空闲槽位 (next_child static) | `forkexit.c:68-75` | ✅ (find_free_slot) |
| P-19 | `vm_fork()` IPC 调用 (sendrec VM) | `forkexit.c:78-80` | ❌ |
| P-20 | vm_fork 失败时无需回滚 (尚未修改状态) | `forkexit.c:81` | ❌ |
| P-21 | **vm_fork 成功后不可失败** (VM 已调 sys_fork) | `forkexit.c:82 注释` | ❌ |
| P-22 | `procs_in_use++` | `forkexit.c:86` | ✅ (alloc_slot) |
| P-23 | `*rmc = *rmp` 整体拷贝 | `forkexit.c:87` | ✅ (fork_from) |
| P-24 | mp_sigact 指针修复 (Rust 不需要，值类型) | `forkexit.c:88-89` | N/A |
| P-25 | `mp_parent = who_p` | `forkexit.c:90` | ✅ (fork_from) |
| P-26 | Tracer 处理: TO_TRACEFORK 检查 | `forkexit.c:91-95` | ❌ (需在完整状态机中) |
| P-27 | 特权进程 scheduler 修正 | `forkexit.c:100-103` | ✅ (fork_from) |
| P-28 | 标志位过滤: `mp_flags &= (IN_USE\|DELAY_CALL\|TAINTED)` | `forkexit.c:106` | ✅ (fork_from) |
| P-29 | `get_free_pid()` PID 分配 | `forkexit.c:119` | ✅ (pid_gen) |
| P-30 | `tell_vfs(VFS_PM_FORK)` 异步通知 | `forkexit.c:122-130` | ❌ |
| P-31 | `VFS_CALL` 标志管理 (防止重复发送) | `utility.c:136` | ❌ |
| P-32 | Tracer SIGSTOP 通知 | `forkexit.c:133-134` | ❌ |
| P-33 | 返回 SUSPEND | `forkexit.c:139` | ❌ |
| P-34 | `do_fork_reply()` VFS 回复后唤醒父进程 | `main.c:369-396` | ❌ |
| P-35 | 调度失败回滚: exit_proc + reply(parent, -1) | `main.c:378-385` | ❌ |
| P-36 | NEW_PARENT 竞态处理 | `main.c:327` | ❌ |

### 1.3 PID 循环回收算法

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| P-37 | PID 范围 [2, 30000] (NR_PIDS) | `utility.c:37` | ✅ |
| P-38 | 轮转分配 (static next_pid) | `utility.c:36` | ✅ |
| P-39 | 冲突检测: mp_pid 和 mp_procgrp | `utility.c:43-46` | ✅ |

---

## 二、VM 层检查清单 (阶段 2 — ❌ 待实现)

### 2.1 数据结构定义

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| V-01 | `VmProc` 结构体 (对齐 vmproc) | `vmproc.h` | ❌ |
| V-02 | `VmFlags` (INUSE/EXITING/VM_INSTANCE) | `vmproc.h` | ❌ |
| V-03 | `VmRegion` 结构体 (对齐 vir_region) | `region.h` | ❌ |
| V-04 | `VmRegionFlags` (WRITABLE/SHARED/ANON/DIRECT) | `region.h` | ❌ |
| V-05 | **`PhysBlock` 结构体 (必须包含 `ref_count` 字段)** | `region.h` | ❌ |
| V-06 | `PhysRegionRef` 结构体 (桥梁) | `region.h` | ❌ |
| V-07 | `PageTable` Mock 结构体 | `vmproc.h:vm_pt` | ❌ |

### 2.2 vmproc 复制逻辑

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| V-08 | `*vmc = *vmp` 整体复制 vmproc | `fork.c:④` | ❌ |
| V-09 | `vmc->vm_slot = childproc` | `fork.c:④` | ❌ |
| V-10 | `region_init(&vmc->vm_regions_avl)` | `fork.c:④` | ❌ |
| V-11 | `vmc->vm_endpoint = NONE` | `fork.c:④` | ❌ |
| V-12 | 保存并恢复子进程自己的页表 (`origpt`) | `fork.c:④` | ❌ |
| V-13 | `vmc->vm_flags &= VMF_INUSE` 只继承 INUSE | `fork.c:⑦` | ❌ |

### 2.3 ★ CoW 核心算法 (硬核点)

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| V-14 | **`pb_reference()` — 共享 phys_block, refcount++** | `phys.c` | ❌ |
| V-15 | **`pb_link()` — 头插法链表 + refcount++** | `phys.c` | ❌ |
| V-16 | **`pb_unreferenced()` — refcount--, 降到0释放物理页** | `phys.c` | ❌ |
| V-17 | `pb_new()` — 创建新 phys_block (refcount=0) | `phys.c` | ❌ |
| V-18 | **`anon_writable()` — CoW 判定: refcount==1 可写, >=2 只读** | 匿名内存类型 | ❌ |
| V-19 | **`mem_cow()` — 写时复制: alloc + copy + refcount adjust** | `phys.c/memtype` | ❌ |
| V-20 | mem_cow 步骤1: 分配新物理页 | `mem_cow():1` | ❌ |
| V-21 | mem_cow 步骤2: sys_abscopy 复制 4KB 内容 | `mem_cow():2` | ❌ |
| V-22 | mem_cow 步骤3: pb_new 创建新 phys_block | `mem_cow():3` | ❌ |
| V-23 | mem_cow 步骤4: pb_unreferenced 旧 refcount-- | `mem_cow():4` | ❌ |
| V-24 | mem_cow 步骤5: pb_link 新 refcount=1 | `mem_cow():5` | ❌ |
| V-25 | mem_cow 步骤6: memtype 变为匿名 | `mem_cow():6` | ❌ |

### 2.4 ★ vm_region 遍历 (硬核点)

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| V-26 | **`map_proc_copy()` — 遍历父进程所有 region** | `region.c` | ❌ |
| V-27 | **`map_copy_region()` — 逐区域复制 (共享 phys_block)** | `region.c` | ❌ |
| V-28 | 共享内存段 (VR_SHARED) 不设 CoW 保护 | `region.c` | ❌ |
| V-29 | `map_writept()` — 写入 PTE (refcount 影响 PTF_WRITE/PTF_READ) | `region.c` | ❌ |
| V-30 | PTE 标记: refcount==1 → PTF_WRITE, refcount>=2 → PTF_READ | `region.c:map_ph_writept` | ❌ |

### 2.5 页表生命周期

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| V-31 | `pt_new()` — 创建新页表 (分配页目录, 映射内核空间) | `pt.c` | ❌ |
| V-32 | `pt_bind()` — 绑定页表到进程 (通知内核, 清 RTS_VMINHIBIT) | `pt.c` | ❌ |
| V-33 | 页表生命周期: pt_new → map_proc_copy → sys_fork → pt_bind | `fork.c:⑤-⑩` | ❌ |

### 2.6 ACL 与其他

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| V-34 | `acl_fork()` — USER_ACL 继承, 其他降级为 NO_ACL | `acl.c` | ❌ |
| V-35 | `sys_fork()` 调用 (传入 PFF_VMINHIBIT) | `fork.c:⑨` | ❌ |
| V-36 | vm_fork 主流程完整实现 | `fork.c` | ❌ |

---

## 三、VFS 层检查清单 (阶段 4 — ❌ 待实现)

### 3.1 数据结构定义

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| F-01 | `FProc` 结构体 (对齐 struct fproc) | `fproc.h` | ❌ |
| F-02 | `FpFlags` (SRV_PROC/REVIVED/SESLDR/PENDING/EXITING/PM_WORK) | `fproc.h:91-98` | ❌ |
| F-03 | **`Filp` 结构体 (必须包含 `count` 引用计数字段)** | `file.h` | ❌ |
| F-04 | Filp 包含 `pos` 字段 (共享的文件偏移量) | `file.h:off_t filp_pos` | ❌ |
| F-05 | `VNodeRef` 结构体 (index 指向全局 vnode 表) | `vnode.h` | ❌ |
| F-06 | `FilpRef` 结构体 (index 指向全局 filp 表) | `fproc.h:fp_filp` | ❌ |

### 3.2 ★ filp 引用计数 (硬核点)

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| F-07 | **fork 时 filp_count++ 循环 (所有打开的 fd)** | `misc.c:pm_fork():⑤` | ❌ |
| F-08 | 父子进程共享同一个 filp 对象 (共享文件偏移量) | `misc.c:pm_fork():④-⑤` | ❌ |
| F-09 | **close_filp() 时 filp_count--** | `filedes.c:close_filp()` | ❌ |
| F-10 | **filp_count 降到 0 才真正关闭 (释放 vnode)** | `filedes.c:close_filp()` | ❌ |
| F-11 | filp_count > 0 时只解锁 vnode | `filedes.c:close_filp()` | ❌ |
| F-12 | fork 后父进程 close(fd) 只将 count 从 2 减为 1 | 推导 | ❌ |

### 3.3 ★ vnode 引用计数 (硬核点)

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| F-13 | **`dup_vnode()` — v_ref_count++ (仅 fp_rd/fp_wd)** | `vnode.c:227-234` | ❌ |
| F-14 | dup_vnode 不递增 v_fs_count | `vnode.c:227-234` | ❌ |
| F-15 | **`put_vnode()` — v_ref_count--** | `vnode.c:240-299` | ❌ |
| F-16 | v_ref_count > 1 时只递减, 不通知 FS | `vnode.c:248-253` | ❌ |
| F-17 | v_ref_count == 1 时 req_putnode 通知 FS | `vnode.c:258-275` | ❌ |
| F-18 | v_fs_count 延迟同步 (>256 时压缩) | `vnode.c:305-315` | ❌ |
| F-19 | 为什么 fork 只对 fp_rd/fp_wd 调 dup_vnode (filp 间接维护) | 设计分析 | ❌ |

### 3.4 pm_fork 其他逻辑

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| F-20 | `okendpt()` 验证父进程 endpoint | `utility.c:94-123` | ❌ |
| F-21 | 子进程 slot 不用 isokendpt (endpoint 尚未设置) | `misc.c:pm_fork():②` | ❌ |
| F-22 | `fp_pid == PID_FREE` 检查 | `misc.c:pm_fork():③` | ❌ |
| F-23 | **`fp_lock` 保留 (属于 slot, 不拷贝!)** | `misc.c:pm_fork():④` | ❌ |
| F-24 | `fp_pid = cpid`, `fp_endpoint = cproc` | `misc.c:pm_fork():⑥` | ❌ |
| F-25 | `fp_flags = FP_NOFLAGS` | `misc.c:pm_fork():⑦` | ❌ |
| F-26 | `fp_cloexec_set` 完整继承 | `misc.c:pm_fork():④` | ❌ |

---

## 四、Kernel 层检查清单 (阶段 3 — ❌ 待实现)

### 4.1 数据结构定义

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| K-01 | `KProcess` 结构体 (对齐 struct proc 关键字段) | `proc.h` | ❌ |
| K-02 | `RtsFlags` 位标志 (完整定义所有标志) | `proc.h` | ❌ |
| K-03 | `MiscFlags` 位标志 (VIRT_TIMER/PROF_TIMER/SC_TRACE/STEP) | `proc.h` | ❌ |
| K-04 | `ProcTable` 结构体 (procs + generations 数组) | `proc.h` | ❌ |

### 4.2 ★ PCB 克隆与上下文伪造 (硬核点)

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| K-05 | **`*rpc = *rpp` PCB 整体复制** | `do_fork.c:③` | ❌ |
| K-06 | **`ret_reg = 0` — 子进程 fork 返回 0 (rax/eax 伪造)** | `do_fork.c:⑥` | ❌ |
| K-07 | retreg = eax (x86) / r0 (ARM) — 寄存器映射 | `stackframe_s` | ❌ |
| K-08 | 子进程恢复执行时从 p_reg 恢复所有寄存器 | `restore_user_context` | ❌ |

### 4.3 ★ Endpoint Generation 算法 (硬核点)

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| K-09 | **`make_endpoint(generation, slot)` — `_ENDPOINT(g, p)` 精确算法** | `com.h` | ❌ |
| K-10 | **`endpoint_generation(ep)` — 从 endpoint 提取 generation** | `com.h` | ❌ |
| K-11 | **`endpoint_slot(ep)` — 从 endpoint 提取 slot** | `com.h` | ❌ |
| K-12 | **generation 递增: 每次 fork +1** | `do_fork.c:⑤` | ❌ |
| K-13 | **generation 回绕: 溢出时回绕到 1 (不是 0!)** | `do_fork.c:⑤` | ❌ |
| K-14 | `is_valid_endpoint()` — generation 验证 | `proc.c` | ❌ |

### 4.4 RTS 标志管理

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| K-15 | `RTS_NO_QUANTUM` 设置 — 子进程无时间片 | `do_fork.c:⑩` | ❌ |
| K-16 | `RTS_VMINHIBIT` 条件设置 — PFF_VMINHIBIT 标志 | `do_fork.c:⑬` | ❌ |
| K-17 | `RTS_NO_PRIV` 条件设置 — 特权进程子进程降级 | `do_fork.c:⑪` | ❌ |
| K-18 | `RTS_SIGNALED \| RTS_SIG_PENDING \| RTS_P_STOP` 清除 | `do_fork.c:⑭` | ❌ |
| K-19 | `p_rts_flags == 0` 时进程才可运行 | `proc.h` | ❌ |

### 4.5 其他 PCB 修复

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| K-20 | FPU 保存区修复: 保留子槽自己的缓冲区 | `do_fork.c:②④` | ❌ |
| K-21 | 父进程使用 FPU 时复制内容到子槽 | `do_fork.c:④` | ❌ |
| K-22 | 特权进程降级: SYS_PROC 子进程 → USER_PRIV | `do_fork.c:⑪` | ❌ |
| K-23 | `p_pending` 信号集清空 | `do_fork.c:⑭` | ❌ |
| K-24 | `p_seg.p_cr3` 页表基址清零 | `do_fork.c:⑮` | ❌ |
| K-25 | 进程名追加 `"*F"` | `do_fork.c:⑨` | ❌ |
| K-26 | 记账统计全部归零 (user_time/sys_time/cycles/cpuavg) | `do_fork.c:⑦⑩` | ❌ |
| K-27 | MiscFlags 清除 (VIRT_TIMER/PROF_TIMER/SC_TRACE/STEP) | `do_fork.c:⑧` | ❌ |

---

## 五、跨层消息协议检查清单

| # | 逻辑点 | Minix3 源码 | 状态 |
|---|--------|------------|------|
| M-01 | PM → VM: VM_FORK (parent_ep, child_slot → child_ep) | `com.h` | ❌ |
| M-02 | VM → Kernel: SYS_FORK (parent_ep, child_slot, PFF_VMINHIBIT) | `syslib.h` | ❌ |
| M-03 | PM → VFS: VFS_PM_FORK (child_ep, parent_ep, child_pid) | `com.h:527-580` | ❌ |
| M-04 | VFS → PM: VFS_PM_FORK_REPLY (child_ep) | `com.h` | ❌ |
| M-05 | VM pt_bind 后清除 RTS_VMINHIBIT | `pt.c` | ❌ |
| M-06 | PM tell_vfs 使用 asynsend3 (AMF_NOREPLY) | `utility.c:134` | ❌ |
| M-07 | PM 返回 SUSPEND 给内核 | `forkexit.c:139` | ❌ |

---

## 六、集成测试检查清单

| # | 逻辑点 | 状态 |
|---|--------|------|
| T-01 | 四份进程表 endpoint 一致性 (PM/VM/VFS/Kernel) | ❌ |
| T-02 | PID 在 PM 和 VFS 中一致性 | ❌ |
| T-03 | 子进程不可运行 (RTS_NO_QUANTUM + RTS_VMINHIBIT) | ❌ |
| T-04 | **CoW 验证: fork 后写入触发页面复制** | ❌ |
| T-05 | **CoW 验证: refcount 从 2 降为 1** | ❌ |
| T-06 | **filp 引用计数: fork 后 close 不关闭文件** | ❌ |
| T-07 | **vnode 引用计数: fork 后 close 不释放 vnode** | ❌ |
| T-08 | **ret_reg = 0: 子进程 fork 返回 0** | ❌ |
| T-09 | endpoint generation: 槽位重用后递增 | ❌ |
| T-10 | 共享内存段不触发 CoW | ❌ |
| T-11 | 特权进程子进程降级 | ❌ |
| T-12 | 进程表满返回 EAGAIN | ❌ |
| T-13 | PID 冲突检测 | ❌ |

---

## 七、审计失败判定标准

以下任何一项缺失即视为审计失败：

1. ❌ 没有 `vm_region` 遍历 (`map_proc_copy` / `map_copy_region`)
2. ❌ 没有 `filp` 引用计数 (`filp_count++` / `filp_count--`)
3. ❌ 没有 `PCB` 现场伪造 (`ret_reg = 0`)
4. ❌ 没有 `phys_block.ref_count` CoW 判定
5. ❌ 没有 `endpoint generation` 递增 + 回绕算法
6. ❌ 没有 `vnode` 引用计数 (`v_ref_count++` / `put_vnode`)
7. ❌ 使用简单 `Clone` trait 代替深度克隆逻辑
8. ❌ 没有体现微内核跨进程协同的消息协议
9. ❌ `fp_lock` 保留逻辑缺失
10. ❌ `FPU` 保存区修复逻辑缺失
