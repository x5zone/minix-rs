# Minix3 Fork 全栈纵向切片 — 逐层深度分析

> **目标**: 像素级还原 Minix3 fork 的核心状态流转
> **原则**: 只有硬件允许 Mock，OS 逻辑必须触及
> **方法**: 逐层拆解 Minix3 源码逻辑点，标注实现该点所需的 Rust 结构定义

---

## 一、PM 层 — `do_fork()` 完整状态机

### 1.1 源码位置

`minix3/minix/servers/pm/forkexit.c` — `do_fork()`

### 1.2 完整状态机

```
do_fork()
  │
  ├── ① 检查进程表是否已满
  │     if (procs_in_use == NR_PROCS) → EAGAIN
  │     if (procs_in_use >= NR_PROCS-LAST_FEW && uid != 0) → EAGAIN
  │
  ├── ② 查找空闲槽位 (next_child 轮询)
  │     do { next_child = (next_child+1) % NR_PROCS; n++; }
  │     while (mproc[next_child].mp_flags & IN_USE && n <= NR_PROCS)
  │
  ├── ③ vm_fork(parent_ep, child_slot, &child_ep)  ← 可以失败！
  │     │
  │     ▼ IPC: VM_FORK (同步 sendrec)
  │     VM: do_fork() → ... → 返回 child_ep
  │     │
  │     失败 → return 错误码（无需回滚，因为还没分配槽位）
  │
  ├── ④ 获取子进程槽位指针，增加计数
  │     rmc = &mproc[next_child]
  │     procs_in_use++
  │
  ├── ⑤ 整体拷贝父进程 mproc → 子进程
  │     *rmc = *rmp
  │
  ├── ⑥ 恢复 mp_sigact 指针（因为 *rmc = *rmp 覆盖了指针）
  │     rmc->mp_sigact = mpsigact[next_child]
  │     memcpy(rmc->mp_sigact, rmp->mp_sigact, sizeof(mpsigact[next_child]))
  │
  ├── ⑦ 设置父子关系
  │     rmc->mp_parent = who_p
  │
  ├── ⑧ 清除追踪器
  │     if (!(rmc->mp_trace_flags & TO_TRACEFORK)):
  │         rmc->mp_tracer = NO_TRACER
  │         rmc->mp_trace_flags = 0
  │         sigemptyset(&rmc->mp_sigtrace)
  │
  ├── ⑨ 特权进程处理
  │     if (rmc->mp_flags & PRIV_PROC):
  │         assert(rmc->mp_scheduler == NONE)
  │         rmc->mp_scheduler = SCHED_PROC_NR
  │
  ├── ⑩ 继承/重置标志位和统计信息
  │     rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED)
  │     rmc->mp_child_utime = 0
  │     rmc->mp_child_stime = 0
  │     rmc->mp_exitstatus = 0
  │     rmc->mp_sigstatus = 0
  │     rmc->mp_endpoint = child_ep
  │     for (i = 0; i < NR_ITIMERS; i++)
  │         rmc->mp_interval[i] = 0
  │     rmc->mp_started = getticks()
  │
  ├── ⑪ 分配 PID
  │     new_pid = get_free_pid()
  │     rmc->mp_pid = new_pid
  │
  ├── ⑫ 通知 VFS (异步)
  │     m.m_type = VFS_PM_FORK
  │     m.VFS_PM_ENDPT = rmc->mp_endpoint
  │     m.VFS_PM_PENDPT = rmp->mp_endpoint
  │     m.VFS_PM_CPID = rmc->mp_pid
  │     m.VFS_PM_REUID = -1
  │     m.VFS_PM_REGID = -1
  │     tell_vfs(rmc, &m)  ← asynsend, 不等待回复
  │
  ├── ⑬ 追踪器处理
  │     if (rmc->mp_tracer != NO_TRACER)
  │         sig_proc(rmc, SIGSTOP, TRUE, FALSE)
  │
  └── ⑭ 返回 SUSPEND
        return SUSPEND  ← 父进程挂起，等 VFS 回复后唤醒
```

### 1.3 关键逻辑点

#### 1.3.1 mp_sigact 指针修复

**问题**: `*rmc = *rmp` 是 C 的结构体赋值，会把父进程的 `mp_sigact` 指针原样复制过来。但每个进程槽有自己的 `mpsigact[slot]` 缓冲区。

**修复**:
```c
rmc->mp_sigact = mpsigact[next_child];                    // 恢复子进程自己的指针
memcpy(rmc->mp_sigact, rmp->mp_sigact, sizeof(...));      // 复制内容到子进程缓冲区
```

**Rust 对应**: 不存在此问题。Rust 的 `Clone` 是深拷贝，`SignalState` 包含值类型，无指针。

#### 1.3.2 vm_fork 失败的回滚

**关键**: `vm_fork()` 在 `procs_in_use++` **之前**调用。如果失败，无需回滚。

但注意：如果 `vm_fork()` 成功，后续代码**不能失败**，因为 VM 已经调用了 `sys_fork()` 创建了内核进程。

#### 1.3.3 SUSPEND 机制

PM 返回 `SUSPEND` 给内核，表示父进程需要挂起。VFS 处理完 `VFS_PM_FORK` 后回复 `VFS_PM_FORK_REPLY`，PM 收到后唤醒父进程，父进程的用户态 `fork()` 返回子进程 PID。

#### 1.3.4 tell_vfs 异步机制

```c
void tell_vfs(rmp, m_ptr) {
    if (rmp->mp_flags & (VFS_CALL | EVENT_CALL))
        panic("tell_vfs: not idle");
    r = asynsend3(VFS_PROC_NR, m_ptr, AMF_NOREPLY);
    rmp->mp_flags |= VFS_CALL;
}
```

- 使用 `asynsend3`（异步发送，不阻塞）
- 设置 `VFS_CALL` 标志防止重复发送
- VFS 回复后，PM 在 `do_fork_reply()` 中清除 `VFS_CALL` 并唤醒父进程

### 1.4 Rust 结构定义

```rust
// PM 进程结构体（已实现）
pub struct Process {
    pub identity: ProcessIdentity,
    pub state: ProcessState,
    pub resources: ProcessResources,
    pub ipc: ProcessIpc,
}

// PM fork 结果
pub struct ForkResult {
    pub child_index: usize,
    pub child_pid: Pid,
    pub child_endpoint: Endpoint,
}

// PM fork 错误
pub enum ForkError {
    TableFull,         // EAGAIN
    ReservedForRoot,   // EAGAIN (非 root 保留槽位)
    ResourceExhausted, // ENOMEM (VM fork 失败)
    InternalError,     // EINVAL
}
```

---

## 二、VM 层 — `do_fork()` 地址空间克隆

### 2.1 源码位置

`minix3/minix/servers/vm/fork.c` — `do_fork()`

### 2.2 核心数据结构

#### vmproc

```c
struct vmproc {
    int          vm_flags;           // VMF_INUSE | VMF_EXITING | VMF_VM_INSTANCE
    endpoint_t   vm_endpoint;
    pt_t         vm_pt;              // 页表数据
    region_avl   vm_regions_avl;     // 虚拟地址区域 AVL 树
    vir_bytes    vm_region_top;
    int          vm_acl;
    int          vm_slot;
    vir_bytes    vm_total;
    vir_bytes    vm_total_max;
    u64_t        vm_minor_page_fault;
    u64_t        vm_major_page_fault;
};
```

#### vir_region（虚拟内存区域）

```c
struct vir_region {
    vir_bytes     vaddr;             // 虚拟地址
    vir_bytes     length;            // 字节长度
    phys_region **physblocks;        // 物理块指针数组（每页一个槽位）
    u16_t         flags;             // VR_WRITABLE | VR_SHARED | VR_ANON ...
    vmproc       *parent;            // 拥有此区域的进程
    mem_type_t   *def_memtype;       // 内存类型函数表
    union { ... } param;             // 类型特定参数
    // AVL 树节点
    vir_region *lower, *higher;
    int factor;
};
```

#### phys_block（物理内存块 — CoW 的灵魂）

```c
struct phys_block {
    phys_bytes      phys;            // 物理内存地址
    phys_region    *firstregion;     // 共享链表头
    u8_t            refcount;        // 引用计数！CoW 判定核心
    u8_t            flags;
};
```

#### phys_region（虚拟区域到物理块的桥梁）

```c
struct phys_region {
    phys_block   *ph;                // 指向共享的物理块
    vir_region   *parent;            // 所属的虚拟区域
    vir_bytes     offset;            // 在区域内的偏移
    mem_type_t   *memtype;           // 内存类型
    phys_region  *next_ph_list;      // 同一 phys_block 的共享链表
};
```

### 2.3 CoW 核心算法

#### 2.3.1 fork 时：pb_reference() — 共享物理页

```c
struct phys_region *pb_reference(phys_block *pb, vir_bytes offset,
    vir_region *region, mem_type_t *memtype) {
    phys_region *newpr = SLABALLOC();
    newpr->memtype = memtype;
    pb_link(newpr, pb, offset, region);   // refcount++ 在这里！
    physblock_set(region, offset, newpr);
    return newpr;
}

void pb_link(phys_region *newpr, phys_block *pb,
    vir_bytes offset, vir_region *parent) {
    newpr->ph = pb;
    newpr->parent = parent;
    newpr->next_ph_list = pb->firstregion;  // 头插法
    pb->firstregion = newpr;
    pb->refcount++;                          // 引用计数 +1
}
```

**关键**: `pb_reference()` 只分配了几十字节的 `phys_region`，**没有复制 4KB 物理页**。`refcount` 从 1 变为 2。

#### 2.3.2 CoW 判定：anon_writable()

```c
static int anon_writable(phys_region *pr) {
    if (pr->ph->phys == MAP_NONE) return 0;
    if (pr->parent->remaps > 0) return 1;
    return pr->ph->refcount == 1;  // 只有独占时才可写！
}
```

**CoW 判定规则**: `refcount == 1` → 可写；`refcount >= 2` → 只读（CoW 保护）。

#### 2.3.3 写缺页时：mem_cow() — 真正复制物理页

```c
int mem_cow(vir_region *region, phys_region *ph,
    phys_bytes new_page_cl, phys_bytes new_page) {
    // 1. 分配新物理页
    if (new_page == MAP_NONE)
        new_page = CLICK2ABS(alloc_mem(1, allocflags));

    // 2. 复制旧页内容到新页
    sys_abscopy(ph->ph->phys, new_page, VM_PAGE_SIZE);

    // 3. 创建新 phys_block
    pb = pb_new(new_page);

    // 4. 解除旧 phys_block 的引用（refcount--）
    pb_unreferenced(region, ph, 0);

    // 5. 链接到新 phys_block（refcount = 1）
    pb_link(ph, pb, ph->offset, region);

    // 6. 内存类型变为匿名
    ph->memtype = &mem_type_anon;
    return OK;
}
```

#### 2.3.4 PTE 标记机制

```c
int map_ph_writept(vmproc *vmp, vir_region *vr, phys_region *pr) {
    int flags = PTF_PRESENT | PTF_USER;
    if (pr_writable(vr, pr))
        flags |= PTF_WRITE;     // refcount==1 → 可写
    else
        flags |= PTF_READ;      // refcount>=2 → 只读（CoW 保护）
    pt_writemap(vmp, &vmp->vm_pt, vr->vaddr + pr->offset,
        pb->phys, VM_PAGE_SIZE, flags, WMF_OVERWRITE);
}
```

### 2.4 CoW 完整状态机

```
                    fork 前
  父进程: phys_region P1 → phys_block B (refcount=1, phys=0x1000)
  PTE: vaddr → 0x1000, PTF_WRITE

                    fork 后 (map_proc_copy + map_writept)
  父进程: phys_region P1 → phys_block B (refcount=2, phys=0x1000)
  PTE: vaddr → 0x1000, PTF_READ (只读！CoW 保护)
  子进程: phys_region P2 → phys_block B (refcount=2, phys=0x1000)
  PTE: vaddr → 0x1000, PTF_READ (只读！CoW 保护)
  共享链表: B.firstregion → P2 → P1 → NULL

                    子进程写入 vaddr
  1. CPU 写只读页 → 写保护异常 → 内核转发到 VM
  2. map_pf() → anon_writable() 返回 0 (refcount=2)
  3. anon_pagefault() → mem_cow()
     a. alloc_mem() 分配新物理页 0x2000
     b. sys_abscopy(0x1000, 0x2000) 复制内容
     c. pb_unreferenced(): B.refcount 2→1
     d. pb_link(P2, C): C.refcount 0→1
  4. map_ph_writept(): refcount==1 → PTF_WRITE

                    写缺页处理后
  父进程: P1 → B (refcount=1, phys=0x1000), PTF_WRITE
  子进程: P2 → C (refcount=1, phys=0x2000), PTF_WRITE
```

### 2.5 页表生命周期

```
pt_new()           → 分配空页目录，映射内核空间
map_proc_copy()    → CoW 复制区域，写入 PTE（只读）
sys_fork(PFF_VMINHIBIT) → 内核创建 PCB，设 RTS_VMINHIBIT
pt_bind()          → 通知内核页表地址，清除 RTS_VMINHIBIT
```

### 2.6 Rust 结构定义

```rust
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
    pub phys_blocks: Vec<Option<PhysRegionRef>>,
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

bitflags::bitflags! {
    pub struct VmRegionFlags: u16 {
        const WRITABLE = 0x001;
        const SHARED = 0x040;
        const ANON = 0x100;
        const DIRECT = 0x200;
    }
}
```

---

## 三、VFS 层 — `pm_fork()` 文件描述符复制

### 3.1 源码位置

`minix3/minix/servers/vfs/misc.c` — `pm_fork()`

### 3.2 核心数据结构

#### fproc

```c
struct fproc {
    unsigned fp_flags;
    pid_t fp_pid;
    endpoint_t fp_endpoint;
    vnode *fp_wd;                    // 工作目录
    vnode *fp_rd;                    // 根目录
    filp *fp_filp[OPEN_MAX];         // 文件描述符表
    fd_set fp_cloexec_set;           // FD_CLOEXEC 位图
    uid_t fp_realuid, fp_effuid;
    gid_t fp_realgid, fp_effgid;
    int fp_ngroups;
    gid_t fp_sgroups[NGROUPS_MAX];
    mode_t fp_umask;
    mutex_t fp_lock;                 // 属于 slot，不拷贝！
    char fp_name[PROC_NAME_LEN];
};
```

#### filp（全局文件表条目）

```c
struct filp {
    mode_t filp_mode;
    int filp_flags;
    int filp_count;                  // 引用计数！fork 时 ++
    vnode *filp_vno;
    off_t filp_pos;                  // 共享的文件偏移量
    mutex_t filp_lock;
};
```

#### vnode

```c
struct vnode {
    endpoint_t v_fs_e;
    ino_t v_inode_nr;
    mode_t v_mode;
    off_t v_size;
    int v_ref_count;                 // 引用计数！dup_vnode 时 ++
    int v_fs_count;
    vmnt *v_vmnt;
    tll_t v_lock;
};
```

### 3.3 pm_fork() 逐行分析

```c
void pm_fork(endpoint_t pproc, endpoint_t cproc, pid_t cpid) {
    // 1. 验证父进程 endpoint
    okendpt(pproc, &parentno);

    // 2. 提取子进程 slot 号（不能用 isokendpt，因为子进程 endpoint 还未设置）
    childno = _ENDPOINT_P(cproc);
    assert(fproc[childno].fp_pid == PID_FREE);

    // 3. 整体复制 fproc（但保留子进程自己的 mutex）
    c_fp_lock = fproc[childno].fp_lock;
    fproc[childno] = fproc[parentno];       // C 结构体赋值
    fproc[childno].fp_lock = c_fp_lock;     // 恢复子进程自己的 mutex

    // 4. 增加 filp 引用计数（核心！）
    cp = &fproc[childno];
    for (i = 0; i < OPEN_MAX; i++)
        if (cp->fp_filp[i] != NULL)
            cp->fp_filp[i]->filp_count++;   // 共享 filp，refcount++

    // 5. 设置子进程自己的 PID 和 endpoint
    cp->fp_pid = cpid;
    cp->fp_endpoint = cproc;

    // 6. 清除所有标志
    cp->fp_flags = FP_NOFLAGS;

    // 7. 增加目录 vnode 引用计数
    if (cp->fp_rd) dup_vnode(cp->fp_rd);   // v_ref_count++
    if (cp->fp_wd) dup_vnode(cp->fp_wd);   // v_ref_count++
}
```

### 3.4 引用计数关系图

```
fork 前:
  父进程 fproc
    fp_filp[0] ──→ filp (filp_count=1)
    fp_filp[1] ──→ filp (filp_count=1)
    fp_rd ──→ vnode (v_ref_count=1)
    fp_wd ──→ vnode (v_ref_count=1)

fork 后:
  父进程 fproc                   子进程 fproc
    fp_filp[0] ──┐                 fp_filp[0] ──┘  → filp (filp_count=2)
    fp_filp[1] ──┐                 fp_filp[1] ──┘  → filp (filp_count=2)
    fp_rd ───────┐                 fp_rd ───────┘   → vnode (v_ref_count=2)
    fp_wd ───────┐                 fp_wd ───────┘   → vnode (v_ref_count=2)
    fp_lock (自己的)               fp_lock (自己的，被保留)
    fp_pid = parent_pid            fp_pid = cpid
    fp_flags = (原值)              fp_flags = FP_NOFLAGS
```

### 3.5 close() 时的引用计数递减

```c
// close_fd() → close_filp()
if (--f->filp_count == 0) {
    // 最后一个引用消失，释放 vnode
    put_vnode(f->filp_vno);    // v_ref_count--
    f->filp_vno = NULL;
} else {
    // 还有其他引用，只解锁
    unlock_vnode(f->filp_vno);
}
```

**fork 后**: 父进程 close(fd) 只将 `filp_count` 从 2 减为 1，文件不会真正关闭。只有当 `filp_count` 降到 0 时才释放 vnode。

### 3.6 fp_lock 保留的原因

`fp_lock` 属于 fproc slot，不属于进程。VFS 是多线程的，fork 发生时可能有其他线程正在锁住子进程 slot 的 mutex。如果覆盖了这个 mutex，会导致死锁或崩溃。

### 3.7 Rust 结构定义

```rust
pub const OPEN_MAX: usize = 128;
pub const NGROUPS_MAX: usize = 32;

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
    pub count: u32,           // 引用计数！
    pub vnode: Option<VNodeRef>,
    pub pos: i64,             // 共享的文件偏移量
}

pub struct VNodeRef {
    pub index: usize,
    pub ref_count: u32,       // 引用计数！
}

pub struct FilpRef {
    pub index: usize,         // 指向全局 filp 表的索引
}
```

---

## 四、Kernel 层 — `do_fork()` PCB 克隆与上下文伪造

### 4.1 源码位置

`minix3/minix/kernel/system/do_fork.c` — `do_fork()`

### 4.2 核心数据结构

#### proc（PCB — 进程控制块）

```c
struct proc {
    stackframe_s p_reg;        // 保存的寄存器（含 retreg = eax/r0）
    segframe p_seg;            // 段描述符（含 fpu_state, p_cr3）
    proc_nr_t p_nr;            // 进程号
    priv *p_priv;              // 特权结构指针
    u32_t p_rts_flags;         // 运行时标志（==0 才可运行）
    u32_t p_misc_flags;        // 杂项标志
    char p_priority;           // 优先级
    u64_t p_cpu_time_left;     // 剩余 CPU 时间
    unsigned p_quantum_size_ms;// 时间片
    proc *p_scheduler;         // 调度器
    clock_t p_user_time;       // 用户态时间
    clock_t p_sys_time;        // 内核态时间
    clock_t p_virt_left;       // 虚拟定时器剩余
    clock_t p_prof_left;       // 剖析定时器剩余
    sigset_t p_pending;        // 待处理信号
    char p_name[PROC_NAME_LEN];// 进程名
    endpoint_t p_endpoint;     // endpoint（含 generation）
    message p_delivermsg;      // 待投递消息
    vir_bytes p_delivermsg_vir;// 消息缓冲区虚拟地址
};
```

#### stackframe_s（x86 寄存器保存区）

```c
struct stackframe_s {
    u16_t gs, fs, es, ds;
    reg_t di, si, fp, bx, dx, cx;
    reg_t retreg;    // = eax！fork 返回值寄存器
    reg_t pc;        // = eip
    reg_t cs;
    reg_t psw;       // = eflags
    reg_t sp;        // = esp
    reg_t ss;
};
```

### 4.3 do_fork() 逐行分析

```c
int do_fork(proc *caller, message *m_ptr) {
    // 1. 验证参数
    isokendpt(m_ptr->m_lsys_krn_sys_fork.endpt, &p_proc);
    rpp = proc_addr(p_proc);   // 父进程
    rpc = proc_addr(m_ptr->m_lsys_krn_sys_fork.slot);  // 子进程
    assert(!isemptyp(rpp) && isemptyp(rpc));
    assert(!(rpp->p_misc_flags & MF_DELIVERMSG));
    assert(RTS_ISSET(rpp, RTS_RECEIVING));  // 父进程必须阻塞在接收状态

    // 2. 保存 FPU 上下文
    save_fpu(rpp);
    gen = _ENDPOINT_G(rpc->p_endpoint);
    old_fpu_save_area_p = rpc->p_seg.fpu_state;  // 保存子槽的 FPU 缓冲区

    // 3. 整体复制 PCB
    *rpc = *rpp;  // C 结构体赋值 = memcpy

    // 4. 修复 FPU 缓冲区指针
    rpc->p_seg.fpu_state = old_fpu_save_area_p;
    if (proc_used_fpu(rpp))
        memcpy(rpc->p_seg.fpu_state, rpp->p_seg.fpu_state, FPU_XFP_SIZE);

    // 5. 递增 generation，生成新 endpoint
    if (++gen >= _ENDPOINT_MAX_GENERATION) gen = 1;
    rpc->p_nr = m_ptr->m_lsys_krn_sys_fork.slot;
    rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);

    // 6. 伪造子进程返回值：rax = 0
    rpc->p_reg.retreg = 0;

    // 7. 清零时间统计
    rpc->p_user_time = 0;
    rpc->p_sys_time = 0;

    // 8. 清除杂项标志
    rpc->p_misc_flags &= ~(MF_VIRT_TIMER | MF_PROF_TIMER | MF_SC_TRACE | MF_STEP);
    rpc->p_virt_left = 0;
    rpc->p_prof_left = 0;

    // 9. 追加进程名 "*F"
    if (strlen(rpc->p_name) + 2 < sizeof(rpc->p_name))
        strcat(rpc->p_name, "*F");

    // 10. 设置不可运行
    RTS_SET(rpc, RTS_NO_QUANTUM);
    reset_proc_accounting(rpc);
    rpc->p_cpu_time_left = 0;
    rpc->p_cycles = 0;
    rpc->p_kcall_cycles = 0;
    rpc->p_kipc_cycles = 0;
    rpc->p_tick_cycles = 0;
    cpuavg_init(&rpc->p_cpuavg);

    // 11. 特权进程降级
    if (priv(rpp)->s_flags & SYS_PROC) {
        rpc->p_priv = priv_addr(USER_PRIV_ID);
        rpc->p_rts_flags |= RTS_NO_PRIV;
    }

    // 12. 返回子进程 endpoint 和消息地址
    m_ptr->m_krn_lsys_sys_fork.endpt = rpc->p_endpoint;
    m_ptr->m_krn_lsys_sys_fork.msgaddr = rpp->p_delivermsg_vir;

    // 13. VM 抑制
    if (m_ptr->m_lsys_krn_sys_fork.flags & PFF_VMINHIBIT)
        RTS_SET(rpc, RTS_VMINHIBIT);

    // 14. 清除信号和追踪
    RTS_UNSET(rpc, RTS_SIGNALED | RTS_SIG_PENDING | RTS_P_STOP);
    sigemptyset(&rpc->p_pending);

    // 15. 清除页表基址
    rpc->p_seg.p_cr3 = 0;
    rpc->p_seg.p_cr3_v = NULL;

    return OK;
}
```

### 4.4 上下文伪造：retreg = 0 如何使子进程从 fork 返回 0

1. 父进程调用 `sys_fork()`，处于 `RTS_RECEIVING` 状态
2. `*rpc = *rpp` 复制父进程的完整寄存器状态（包括 eax）
3. `rpc->p_reg.retreg = 0` 将子进程的 eax 强制设为 0
4. 子进程被调度运行时，`restore_user_context()` 从 `p_reg` 恢复所有寄存器
5. 子进程用户态看到 eax = 0，即 `fork()` 返回 0

### 4.5 Endpoint Generation 算法

```c
#define _ENDPOINT_GENERATION_SHIFT  15
#define _ENDPOINT_MAX_GENERATION    65535

endpoint = (generation << 15) | slot
generation = (endpoint + MAX_NR_TASKS) >> 15
slot = ((endpoint + MAX_NR_TASKS) & 0x7FFF) - MAX_NR_TASKS
```

fork 时：取子槽当前 generation，+1，若溢出则回绕到 1（不是 0）。

### 4.6 RTS 标志位

| 标志 | 值 | fork 中的操作 |
|------|-----|-------------|
| `RTS_NO_QUANTUM` | 0x8000 | **设置** — 子进程无时间片 |
| `RTS_VMINHIBIT` | 0x200 | **条件设置** — 等 VM 设置页表 |
| `RTS_NO_PRIV` | 0x80 | **条件设置** — 特权进程子进程降级 |
| `RTS_SIGNALED` | 0x10 | **清除** — 不继承信号 |
| `RTS_SIG_PENDING` | 0x20 | **清除** — 不继承信号处理 |
| `RTS_P_STOP` | 0x40 | **清除** — 不继承追踪 |

**核心规则**: `p_rts_flags == 0` 时进程才可运行。

### 4.7 Rust 结构定义

```rust
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
    pub ret_reg: i32,           // fork 返回值寄存器
    pub cr3: u64,               // 页表基址（Mock）
    pub fpu_state: Vec<u8>,     // FPU 保存区（Mock）
}

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

pub struct ProcTable {
    pub procs: [Option<KProcess>; NR_PROCS],
    pub generations: [i32; NR_PROCS],
}
```

---

## 五、跨层消息协议

### 5.1 PM → VM: VM_FORK

```c
VMF_ENDPOINT       = m1_i1  // 父进程 endpoint（输入）
VMF_SLOTNO         = m1_i2  // 子进程槽号（输入）
VMF_CHILD_ENDPOINT = m1_i3  // 子进程 endpoint（输出）
```

### 5.2 VM → Kernel: SYS_FORK

```c
// 输入
m_lsys_krn_sys_fork.endpt  // 父进程 endpoint
m_lsys_krn_sys_fork.slot   // 子进程槽号
m_lsys_krn_sys_fork.flags  // PFF_VMINHIBIT (0x01)

// 输出
m_krn_lsys_sys_fork.endpt   // 子进程的新 endpoint
m_krn_lsys_sys_fork.msgaddr // 子进程的消息缓冲区虚拟地址
```

### 5.3 PM → VFS: VFS_PM_FORK

```c
VFS_PM_ENDPT  = m7_i1  // 子进程 endpoint
VFS_PM_PENDPT = m7_i2  // 父进程 endpoint
VFS_PM_CPID   = m7_i3  // 子进程 PID
VFS_PM_REUID  = m7_i4  // 真实 uid (-1 for regular fork)
VFS_PM_REGID  = m7_i5  // 真实 gid (-1 for regular fork)
```

### 5.4 VFS → PM: VFS_PM_FORK_REPLY

```c
VFS_PM_ENDPT  // 子进程 endpoint（确认）
```

---

## 六、完整调用链参考

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
    ├── ⑤ 修复 sigact 指针
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
