# Fork 系统调用纵向切片重构计划 — Part 3：VM/VFS/Kernel 实现

> **范围**: 阶段 7~10（VM vmproc、Kernel proc、VFS fproc、跨服务协调）
> **前置**: 完成 [Part 2](fork-syscall-plan-part2.md) 中的阶段 5~6

---

## 第七阶段：VM vmproc 结构体与 fork

**状态**: ❌ 待实现

**目标**: 实现 VM 服务器的 vmproc 结构体和 fork 逻辑

### 7.1 Minix3 C 源码分析

#### vmproc 结构体

**文件**: `minix3/minix/servers/vm/vmproc.h`

```c
struct vmproc {
  int        vm_flags;           /* 进程标志位 */
  endpoint_t vm_endpoint;        /* 进程端点标识符 */
  pt_t       vm_pt;              /* 页表数据 */
  struct boot_image *vm_boot;    /* 启动时进程的引导映像指针 */
  region_avl vm_regions_avl;     /* 虚拟地址空间中的区域 AVL 树 */
  vir_bytes  vm_region_top;      /* 最后插入的最高虚拟地址 */
  int        vm_acl;             /* ACL 访问控制列表索引 */
  int        vm_slot;            /* 进程表槽位号 */
  vir_bytes  vm_total;           /* 总虚拟内存大小 */
  vir_bytes  vm_total_max;       /* 最大虚拟内存大小 */
  u64_t      vm_minor_page_fault;/* 次缺页中断计数 */
  u64_t      vm_major_page_fault;/* 主缺页中断计数 */
};

/* vm_flags 位定义 */
#define VMF_INUSE       0x001    /* 槽位包含一个进程 */
#define VMF_EXITING     0x002    /* PM 正在清理此进程 */
#define VMF_VM_INSTANCE 0x010    /* 这是一个 VM 进程实例 */
```

#### VM do_fork() 核心逻辑

**文件**: `minix3/minix/servers/vm/fork.c`

```c
int do_fork(message *msg) {
  // 1. 验证父进程 endpoint 和子进程槽号
  if(vm_isokendpt(msg->VMF_ENDPOINT, &proc) != OK) return EINVAL;
  childproc = msg->VMF_SLOTNO;
  if(childproc < 0 || childproc >= NR_PROCS) return EINVAL;

  vmp = &vmproc[proc];      /* 父进程 */
  vmc = &vmproc[childproc]; /* 子进程 */

  // 2. 整体复制 vmproc（C 风格）
  origpt = vmc->vm_pt;
  *vmc = *vmp;
  vmc->vm_slot = childproc;
  region_init(&vmc->vm_regions_avl);
  vmc->vm_endpoint = NONE;
  vmc->vm_pt = origpt;

  // 3. 创建新页表
  if(pt_new(&vmc->vm_pt) != OK) return ENOMEM;

  // 4. 复制内存区域（COW）
  if(map_proc_copy(vmc, vmp) != OK) {
    pt_free(&vmc->vm_pt);
    return ENOMEM;
  }

  // 5. 只继承 VMF_INUSE 标志
  vmc->vm_flags &= VMF_INUSE;

  // 6. ACL 处理
  acl_fork(vmc);

  // 7. 通知内核
  if((r=sys_fork(vmp->vm_endpoint, childproc,
    &vmc->vm_endpoint, PFF_VMINHIBIT, &msgaddr)) != OK) {
    panic("do_fork can't sys_fork: %d", r);
  }

  // 8. 绑定页表
  if((r=pt_bind(&vmc->vm_pt, vmc)) != OK)
    panic("fork can't pt_bind: %d", r);

  // 9. 返回子进程 endpoint
  msg->VMF_CHILD_ENDPOINT = vmc->vm_endpoint;
  return OK;
}
```

### 7.2 Rust vmproc 结构体设计

**文件**: `os/servers/vm/src/vmproc.rs`（新建）

```rust
use minix_types::{Endpoint, VirBytes, NR_PROCS};

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct VmFlags: u32 {
        const IN_USE = 0x001;
        const EXITING = 0x002;
        const VM_INSTANCE = 0x010;
    }
}

/// 虚拟内存区域（简化版，不实现 AVL 树）
#[derive(Debug, Clone)]
pub struct VmRegion {
    pub vaddr: VirBytes,
    pub length: usize,
    pub flags: u32,
    pub phys_pages: Vec<u64>,  // Mock: 用 Vec 模拟物理页
}

/// 页表（Mock 版本）
#[derive(Debug, Clone)]
pub struct PageTable {
    pub entries: Vec<u64>,  // Mock: 用 Vec 模拟页表项
}

impl PageTable {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }
}

/// VM 进程结构体
#[derive(Debug, Clone)]
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
```

### 7.3 VM fork 核心实现

**文件**: `os/servers/vm/src/fork.rs`（新建）

```rust
impl VmProc {
    /// VM fork：从父进程创建子进程
    ///
    /// 对应 Minix3 的 vm/fork.c do_fork()
    pub fn fork_from(
        parent: &VmProc,
        child_slot: usize,
    ) -> Result<Self, VmForkError> {
        // 1. 整体复制（显式构造，非 C 的 *vmc = *vmp）
        let mut child = VmProc {
            flags: parent.flags & VmFlags::IN_USE,  // 只继承 IN_USE
            endpoint: Endpoint::NONE,  // 暂时无效，等内核分配
            page_table: PageTable::new(),  // 新页表
            regions: Vec::new(),  // 将在下面复制
            region_top: parent.region_top,
            acl: if parent.acl == USER_ACL { USER_ACL } else { NO_ACL },
            slot: child_slot,
            total: parent.total,
            total_max: parent.total_max,
            minor_page_fault: 0,
            major_page_fault: 0,
        };

        // 2. 复制内存区域（COW：增加引用计数，不复制物理页）
        for region in &parent.regions {
            child.regions.push(region.fork_copy());
        }

        Ok(child)
    }
}

impl VmRegion {
    /// COW 复制：共享物理页，增加引用计数
    pub fn fork_copy(&self) -> Self {
        Self {
            vaddr: self.vaddr,
            length: self.length,
            flags: self.flags,
            phys_pages: self.phys_pages.clone(),  // Mock: 共享引用
        }
    }
}
```

### 7.4 验证目标

- [ ] vmproc 结构体字段与 Minix3 C 代码对应
- [ ] fork 只继承 VMF_INUSE 标志
- [ ] 内存区域复制使用 COW 语义
- [ ] ACL 继承规则正确

---

## 第八阶段：Kernel proc 结构体与 sys_fork

**状态**: ❌ 待实现

**目标**: 实现内核的 proc 结构体和 sys_fork 逻辑

### 8.1 Minix3 C 源码分析

#### proc 结构体（关键字段）

**文件**: `minix3/minix/kernel/proc.h`

```c
struct proc {
  struct stackframe_s p_reg;   /* 进程寄存器，保存在栈帧中 */
  struct segframe p_seg;       /* 段描述符 */
  proc_nr_t p_nr;              /* 进程号 */
  struct priv *p_priv;         /* 系统特权结构指针 */
  volatile u32_t p_rts_flags;  /* 运行时标志，为零时进程才可运行 */
  volatile u32_t p_misc_flags; /* 杂项标志 */
  char p_priority;             /* 当前进程优先级 */
  u64_t p_cpu_time_left;       /* 剩余 CPU 时间 */
  unsigned p_quantum_size_ms;  /* 时间片（毫秒） */
  struct proc *p_scheduler;    /* 调度器 */
  clock_t p_user_time;         /* 用户态时间 */
  clock_t p_sys_time;          /* 内核态时间 */
  clock_t p_virt_left;         /* 虚拟定时器剩余 */
  clock_t p_prof_left;         /* profile 定时器剩余 */
  struct proc *p_nextready;    /* 下一个就绪进程 */
  sigset_t p_pending;          /* 待处理的内核信号 */
  char p_name[PROC_NAME_LEN]; /* 进程名 */
  endpoint_t p_endpoint;       /* endpoint（含 generation） */
  message p_delivermsg;        /* 投递给此进程的消息 */
  vir_bytes p_delivermsg_vir;  /* 消息存放的虚拟地址 */
};
```

#### RTS 标志位

| 标志 | 值 | 含义 |
|------|-----|------|
| `RTS_SLOT_FREE` | 0x01 | 进程槽空闲 |
| `RTS_PROC_STOP` | 0x02 | 进程已停止 |
| `RTS_SENDING` | 0x04 | 发送消息阻塞 |
| `RTS_RECEIVING` | 0x08 | 接收消息阻塞 |
| `RTS_SIGNALED` | 0x10 | 新内核信号到达 |
| `RTS_SIG_PENDING` | 0x20 | 信号处理中 |
| `RTS_P_STOP` | 0x40 | 进程被追踪 |
| `RTS_NO_PRIV` | 0x80 | 系统进程 fork 后禁止运行 |
| `RTS_NO_ENDPOINT` | 0x100 | 进程不能收发消息 |
| `RTS_VMINHIBIT` | 0x200 | 等待 VM 设置页表 |
| `RTS_NO_QUANTUM` | 0x8000 | 时间片用完 |

**核心规则**：进程可运行当且仅当 `p_rts_flags == 0`。

#### 内核 do_fork() 核心逻辑

**文件**: `minix3/minix/kernel/system/do_fork.c`

```c
int do_fork(struct proc * caller, message * m_ptr) {
  // 1. 验证参数
  rpp = proc_addr(p_proc);  // 父进程
  rpc = proc_addr(m_ptr->m_lsys_krn_sys_fork.slot);  // 子进程

  // 2. 整体复制 proc 结构体
  gen = _ENDPOINT_G(rpc->p_endpoint);
  *rpc = *rpp;  // C 的结构体赋值

  // 3. 递增 generation，生成新 endpoint
  if(++gen >= _ENDPOINT_MAX_GENERATION) gen = 1;
  rpc->p_endpoint = _ENDPOINT(gen, rpc->p_nr);

  // 4. 子进程返回值为 0
  rpc->p_reg.retreg = 0;

  // 5. 清零时间统计
  rpc->p_user_time = 0;
  rpc->p_sys_time = 0;

  // 6. 清除不应继承的标志
  rpc->p_misc_flags &= ~(MF_VIRT_TIMER | MF_PROF_TIMER | MF_SC_TRACE | MF_STEP);

  // 7. 设置不可运行标志
  RTS_SET(rpc, RTS_NO_QUANTUM);

  // 8. 特权进程处理
  if (priv(rpp)->s_flags & SYS_PROC) {
    rpc->p_priv = priv_addr(USER_PRIV_ID);
    rpc->p_rts_flags |= RTS_NO_PRIV;
  }

  // 9. VM 抑制
  if(m_ptr->m_lsys_krn_sys_fork.flags & PFF_VMINHIBIT) {
    RTS_SET(rpc, RTS_VMINHIBIT);
  }

  // 10. 清除信号
  RTS_UNSET(rpc, (RTS_SIGNALED | RTS_SIG_PENDING | RTS_P_STOP));
  sigemptyset(&rpc->p_pending);

  // 11. 清除页表基址
  rpc->p_seg.p_cr3 = 0;

  // 12. 返回子进程 endpoint
  m_ptr->m_krn_lsys_sys_fork.endpt = rpc->p_endpoint;
  return OK;
}
```

### 8.2 Rust KProcess 结构体设计

**文件**: `os/kernel/src/proc.rs`（重写）

```rust
use minix_types::{Endpoint, Pid, Clock, VirBytes, ProcIndex, NR_PROCS};

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
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
    #[derive(Debug, Clone, Copy)]
    pub struct MiscFlags: u32 {
        const VIRT_TIMER = 0x01;
        const PROF_TIMER = 0x02;
        const SC_TRACE = 0x04;
        const STEP = 0x08;
    }
}

/// 内核进程结构体
#[derive(Debug, Clone)]
pub struct KProcess {
    pub slot: usize,
    pub endpoint: Endpoint,
    pub rts_flags: RtsFlags,
    pub misc_flags: MiscFlags,
    pub priority: i8,
    pub user_time: Clock,
    pub sys_time: Clock,
    pub virt_left: Clock,
    pub prof_left: Clock,
    pub name: [u8; 16],
    pub pending_signals: u64,
    pub is_system_proc: bool,
    pub ret_reg: i32,  // fork 返回值寄存器
}
```

### 8.3 内核 sys_fork 实现

**文件**: `os/kernel/src/system/do_fork.rs`（新建）

```rust
/// Endpoint generation 最大值
pub const ENDPOINT_MAX_GENERATION: i32 = 65535;

/// Endpoint 生成：_ENDPOINT(generation, slot)
pub fn make_endpoint(generation: i32, slot: usize) -> Endpoint {
    Endpoint::new((generation << 15) | (slot as i32))
}

/// 从 endpoint 提取 generation
pub fn endpoint_generation(ep: Endpoint) -> i32 {
    (ep.get() >> 15) & 0x7FFF
}

/// 从 endpoint 提取 slot
pub fn endpoint_slot(ep: Endpoint) -> usize {
    (ep.get() & 0x7FFF) as usize
}

impl KProcess {
    /// 内核 fork：从父进程创建子进程
    ///
    /// 对应 Minix3 的 kernel/system/do_fork.c
    pub fn sys_fork(
        parent: &KProcess,
        child_slot: usize,
        flags: u32,
        generations: &mut [i32; NR_PROCS],
    ) -> Result<(Self, Endpoint), SysForkError> {
        // 1. 递增 generation
        generations[child_slot] += 1;
        if generations[child_slot] >= ENDPOINT_MAX_GENERATION {
            generations[child_slot] = 1;
        }
        let child_endpoint = make_endpoint(generations[child_slot], child_slot);

        // 2. 显式构造子进程
        let mut child = KProcess {
            slot: child_slot,
            endpoint: child_endpoint,
            rts_flags: parent.rts_flags,
            misc_flags: parent.misc_flags & !(MiscFlags::VIRT_TIMER | MiscFlags::PROF_TIMER | MiscFlags::SC_TRACE | MiscFlags::STEP),
            priority: parent.priority,
            user_time: 0,
            sys_time: 0,
            virt_left: 0,
            prof_left: 0,
            name: parent.name,
            pending_signals: 0,
            is_system_proc: parent.is_system_proc,
            ret_reg: 0,  // 子进程 fork 返回 0
        };

        // 3. 设置不可运行
        child.rts_flags |= RtsFlags::NO_QUANTUM;

        // 4. 特权进程处理
        if parent.is_system_proc {
            child.is_system_proc = false;
            child.rts_flags |= RtsFlags::NO_PRIV;
        }

        // 5. VM 抑制
        if flags & PFF_VMINHIBIT != 0 {
            child.rts_flags |= RtsFlags::VMINHIBIT;
        }

        // 6. 清除信号
        child.rts_flags &= !(RtsFlags::SIGNALED | RtsFlags::SIG_PENDING | RtsFlags::P_STOP);
        child.pending_signals = 0;

        Ok((child, child_endpoint))
    }
}
```

### 8.4 Endpoint Generation 管理

**文件**: `os/kernel/src/proc.rs`

```rust
/// 进程表
pub struct ProcTable {
    pub procs: [Option<KProcess>; NR_PROCS],
    pub generations: [i32; NR_PROCS],
}

impl ProcTable {
    pub fn new() -> Self {
        Self {
            procs: std::array::from_fn(|_| None),
            generations: [0; NR_PROCS],
        }
    }

    /// 验证 endpoint 是否有效
    pub fn is_valid_endpoint(&self, ep: Endpoint) -> bool {
        let slot = endpoint_slot(ep);
        if slot >= NR_PROCS { return false; }
        match &self.procs[slot] {
            None => false,
            Some(proc) => proc.endpoint == ep,  // generation 必须匹配
        }
    }
}
```

### 8.5 验证目标

- [ ] KProcess 结构体字段与 Minix3 C 代码对应
- [ ] RTS 标志位完整定义
- [ ] sys_fork 正确递增 generation
- [ ] 特权进程降级逻辑正确
- [ ] VMINHIBIT 标志正确设置
- [ ] 子进程 fork 返回值为 0

---

## 第九阶段：VFS fproc 结构体与 pm_fork

**状态**: ❌ 待实现

**目标**: 实现 VFS 服务器的 fproc 结构体和 fork 逻辑

### 9.1 Minix3 C 源码分析

#### fproc 结构体

**文件**: `minix3/minix/servers/vfs/fproc.h`

```c
struct fproc {
  unsigned fp_flags;               /* 进程标志位 */
  pid_t fp_pid;                    /* 进程 ID */
  endpoint_t fp_endpoint;          /* 内核 endpoint 编号 */
  struct vnode *fp_wd;             /* 工作目录 */
  struct vnode *fp_rd;             /* 根目录 */
  struct filp *fp_filp[OPEN_MAX];  /* 文件描述符表 */
  fd_set fp_cloexec_set;           /* FD_CLOEXEC 位图 */
  dev_t fp_tty;                    /* 控制终端 */
  uid_t fp_realuid;                /* 真实用户 ID */
  uid_t fp_effuid;                 /* 有效用户 ID */
  gid_t fp_realgid;                /* 真实组 ID */
  gid_t fp_effgid;                 /* 有效组 ID */
  int fp_ngroups;                  /* 补充组数量 */
  gid_t fp_sgroups[NGROUPS_MAX];   /* 补充组 */
  mode_t fp_umask;                 /* umask */
  char fp_name[PROC_NAME_LEN];    /* 进程名 */
};
```

#### VFS pm_fork() 核心逻辑

**文件**: `minix3/minix/servers/vfs/misc.c`

```c
void pm_fork(endpoint_t pproc, endpoint_t cproc, pid_t cpid) {
  struct fproc *cp;
  int i, parentno, childno;
  mutex_t c_fp_lock;

  okendpt(pproc, &parentno);
  childno = _ENDPOINT_P(cproc);

  // 1. 整体复制 fproc
  c_fp_lock = fproc[childno].fp_lock;
  fproc[childno] = fproc[parentno];       /* C 结构体赋值 */
  fproc[childno].fp_lock = c_fp_lock;     /* 保留子进程自己的互斥锁 */

  // 2. 增加 filp 引用计数
  cp = &fproc[childno];
  for (i = 0; i < OPEN_MAX; i++)
    if (cp->fp_filp[i] != NULL) cp->fp_filp[i]->filp_count++;

  // 3. 设置新 PID 和 endpoint
  cp->fp_pid = cpid;
  cp->fp_endpoint = cproc;

  // 4. 清除标志
  cp->fp_flags = FP_NOFLAGS;

  // 5. 增加目录 vnode 引用计数
  if (cp->fp_rd) dup_vnode(cp->fp_rd);
  if (cp->fp_wd) dup_vnode(cp->fp_wd);
}
```

### 9.2 Rust fproc 结构体设计

**文件**: `os/servers/vfs/src/fproc.rs`（新建）

```rust
use minix_types::{Pid, Endpoint, Uid, Gid};

/// 最大打开文件数
pub const OPEN_MAX: usize = 128;

/// 最大补充组数
pub const NGROUPS_MAX: usize = 32;

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct FpFlags: u32 {
        const SRV_PROC = 0x0001;
        const REVIVED = 0x0002;
        const SESLDR = 0x0004;
        const PENDING = 0x0010;
        const EXITING = 0x0020;
        const PM_WORK = 0x0040;
    }
}

/// 文件描述符条目（引用计数）
#[derive(Debug, Clone)]
pub struct Filp {
    pub count: u32,
    pub pos: i64,       // 文件偏移量
    pub flags: u32,     // 文件标志
    pub vnode_index: usize,  // Mock: 用索引代替指针
}

/// VNode 引用（Mock 版本）
#[derive(Debug, Clone)]
pub struct VNodeRef {
    pub index: usize,
    pub ref_count: u32,
}

/// VFS 进程结构体
#[derive(Debug, Clone)]
pub struct FProc {
    pub flags: FpFlags,
    pub pid: Pid,
    pub endpoint: Endpoint,
    pub root_dir: Option<usize>,           // Mock: 根目录 vnode 索引
    pub work_dir: Option<usize>,           // Mock: 工作目录 vnode 索引
    pub filps: [Option<usize>; OPEN_MAX],  // Mock: filp 索引表
    pub cloexec_set: u128,                 // FD_CLOEXEC 位图
    pub real_uid: Uid,
    pub eff_uid: Uid,
    pub real_gid: Gid,
    pub eff_gid: Gid,
    pub ngroups: usize,
    pub supplemental_groups: [Gid; NGROUPS_MAX],
    pub umask: u32,
    pub name: [u8; 16],
}
```

### 9.3 VFS pm_fork 实现

**文件**: `os/servers/vfs/src/fork.rs`（新建）

```rust
impl FProc {
    /// VFS fork：从父进程创建子进程
    ///
    /// 对应 Minix3 的 vfs/misc.c pm_fork()
    pub fn fork_from(
        parent: &FProc,
        child_pid: Pid,
        child_endpoint: Endpoint,
        filps: &mut Vec<Filp>,
        vnodes: &mut Vec<VNodeRef>,
    ) -> Self {
        // 1. 复制文件描述符表（共享 filp，增加引用计数）
        let mut child_filps = parent.filps;
        for filp_idx in child_filps.iter_mut().flatten() {
            if *filp_idx < filps.len() {
                filps[*filp_idx].count += 1;
            }
        }

        // 2. 增加目录 vnode 引用计数
        if let Some(idx) = parent.root_dir {
            if idx < vnodes.len() { vnodes[idx].ref_count += 1; }
        }
        if let Some(idx) = parent.work_dir {
            if idx < vnodes.len() { vnodes[idx].ref_count += 1; }
        }

        // 3. 构造子进程 fproc
        FProc {
            flags: FpFlags::empty(),  // 清除所有标志
            pid: child_pid,
            endpoint: child_endpoint,
            root_dir: parent.root_dir,
            work_dir: parent.work_dir,
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

### 9.4 验证目标

- [ ] fproc 结构体字段与 Minix3 C 代码对应
- [ ] 文件描述符复制使用共享 filp + 引用计数
- [ ] FD_CLOEXEC 位图正确继承
- [ ] 目录 vnode 引用计数正确增加
- [ ] 子进程标志位清除

---

## 第十阶段：跨服务协调与集成测试

**状态**: ❌ 待实现

**目标**: 将 PM/VM/VFS/Kernel 的 fork 逻辑串联起来

### 10.1 完整 Fork 调用链

```rust
/// 全局 fork 协调器
pub struct ForkCoordinator<'a> {
    pub pm: &'a mut PmContext<'a>,
    pub vm_table: &'a mut VmProcTable,
    pub kernel_table: &'a mut KernelProcTable,
    pub vfs_table: &'a mut VfsProcTable,
}

impl<'a> ForkCoordinator<'a> {
    /// 完整的 fork 系统调用
    pub fn do_fork(&mut self) -> Result<ForkResult, ForkError> {
        // === 阶段 1: PM 前置检查（可以失败） ===
        let prepare = self.pm.do_fork_prepare()?;

        // === 阶段 2: VM fork（可以失败） ===
        let vm_result = self.vm_table.fork(
            self.pm.current_proc().identity.endpoint,
            prepare.child_index,
        ).map_err(|e| {
            self.pm.table.release_slot(prepare.child_index);
            e
        })?;

        // === 阶段 3: Kernel sys_fork（不能失败） ===
        let (kernel_result, child_endpoint) = self.kernel_table.sys_fork(
            self.pm.current_proc().identity.endpoint,
            prepare.child_index,
            PFF_VMINHIBIT,
        ).expect("sys_fork must not fail after vm_fork");

        // === 阶段 4: VM 绑定页表（不能失败） ===
        self.vm_table.bind_page_table(prepare.child_index, child_endpoint);

        // === 阶段 5: PM 进程结构初始化（不能失败） ===
        self.pm.fork_child_from_parent(
            prepare.child_index,
            prepare.child_pid,
            child_endpoint,
        );

        // === 阶段 6: VFS pm_fork（不能失败） ===
        self.vfs_table.fork(
            self.pm.current_proc().identity.endpoint,
            child_endpoint,
            prepare.child_pid,
        );

        // === 阶段 7: 通知 VFS（异步） ===
        // TODO: 实现 IPC 通知

        Ok(ForkResult {
            child_index: prepare.child_index,
            child_pid: prepare.child_pid,
            child_endpoint,
        })
    }
}
```

### 10.2 集成测试

```rust
#[test]
fn test_full_fork_flow() {
    let mut coord = create_test_coordinator();

    let result = coord.do_fork().unwrap();

    // 验证 PM
    let pm_child = coord.pm.table.get(result.child_index).unwrap();
    assert_eq!(pm_child.identity.id.pid, result.child_pid);
    assert_eq!(pm_child.identity.endpoint, result.child_endpoint);

    // 验证 VM
    let vm_child = &coord.vm_table.procs[result.child_index];
    assert!(vm_child.flags.contains(VmFlags::IN_USE));
    assert_eq!(vm_child.regions.len(), /* parent regions count */);

    // 验证 Kernel
    let kernel_child = coord.kernel_table.procs[result.child_index].as_ref().unwrap();
    assert_eq!(kernel_child.endpoint, result.child_endpoint);
    assert!(kernel_child.rts_flags.contains(RtsFlags::NO_QUANTUM));
    assert!(kernel_child.rts_flags.contains(RtsFlags::VMINHIBIT));
    assert_eq!(kernel_child.ret_reg, 0);  // 子进程返回 0

    // 验证 VFS
    let vfs_child = &coord.vfs_table.procs[result.child_index];
    assert_eq!(vfs_child.pid, result.child_pid);
    assert_eq!(vfs_child.endpoint, result.child_endpoint);
    assert!(vfs_child.flags.is_empty());
}
```

### 10.3 不变量验证

```rust
#[test]
fn test_fork_invariants() {
    let mut coord = create_test_coordinator();
    let result = coord.do_fork().unwrap();

    // 1. 四份进程表的 endpoint 一致
    let pm_ep = coord.pm.table.get(result.child_index).unwrap().identity.endpoint;
    let vm_ep = coord.vm_table.procs[result.child_index].endpoint;
    let kernel_ep = coord.kernel_table.procs[result.child_index].as_ref().unwrap().endpoint;
    let vfs_ep = coord.vfs_table.procs[result.child_index].endpoint;
    assert_eq!(pm_ep, vm_ep);
    assert_eq!(pm_ep, kernel_ep);
    assert_eq!(pm_ep, vfs_ep);

    // 2. PID 一致
    let pm_pid = coord.pm.table.get(result.child_index).unwrap().identity.id.pid;
    let vfs_pid = coord.vfs_table.procs[result.child_index].pid;
    assert_eq!(pm_pid, vfs_pid);

    // 3. 子进程不可运行
    let kernel_child = coord.kernel_table.procs[result.child_index].as_ref().unwrap();
    assert!(kernel_child.rts_flags.contains(RtsFlags::NO_QUANTUM));

    // 4. 文件描述符共享
    let parent_filps = &coord.vfs_table.procs[coord.pm.current].filps;
    let child_filps = &coord.vfs_table.procs[result.child_index].filps;
    for (i, (p, c)) in parent_filps.iter().zip(child_filps.iter()).enumerate() {
        assert_eq!(p, c, "fd {} should be shared", i);
    }
}
```

### 10.4 验证清单

- [ ] 四份进程表的 endpoint 一致
- [ ] PID 在 PM 和 VFS 中一致
- [ ] 子进程在内核中不可运行（RTS_NO_QUANTUM + RTS_VMINHIBIT）
- [ ] VM 内存区域正确复制（COW）
- [ ] VFS 文件描述符正确共享（引用计数增加）
- [ ] VFS 目录 vnode 引用计数正确
- [ ] PM 在 vm_fork 失败时正确回滚
- [ ] PM 在 vm_fork 成功后不失败

---

## 附录：Minix3 Fork 完整调用链参考

```
用户进程: fork()
    │
    ▼ libc
sys_fork() → 内核 IPC → PM
    │
    ▼
PM: do_fork()                    [forkexit.c]
    ├── ① 检查进程表是否已满
    ├── ② 查找空闲槽位 (next_child)
    ├── ③ vm_fork(parent_ep, child_slot, &child_ep)
    │       │
    │       ▼ IPC: VM_FORK
    │   VM: do_fork()             [vm/fork.c]
    │       ├── 验证父进程 endpoint
    │       ├── *vmc = *vmp (复制 vmproc)
    │       ├── pt_new() (创建新页表)
    │       ├── map_proc_copy() (COW 复制内存区域)
    │       ├── vmc->vm_flags &= VMF_INUSE
    │       ├── acl_fork()
    │       ├── sys_fork(parent_ep, child_slot, &child_ep, PFF_VMINHIBIT, &msgaddr)
    │       │       │
    │       │       ▼ 内核调用: SYS_FORK
    │       │   Kernel: do_fork()  [kernel/system/do_fork.c]
    │       │       ├── 验证参数
    │       │       ├── *rpc = *rpp (复制 proc)
    │       │       ├── 递增 generation，生成新 endpoint
    │       │       ├── retreg = 0 (子进程返回 0)
    │       │       ├── RTS_SET(RTS_NO_QUANTUM)
    │       │       ├── 特权进程 → RTS_NO_PRIV
    │       │       ├── PFF_VMINHIBIT → RTS_VMINHIBIT
    │       │       ├── 清除信号和追踪
    │       │       └── 返回 child_endpoint
    │       │
    │       ├── pt_bind() (绑定页表，清除 RTS_VMINHIBIT)
    │       └── 返回 child_endpoint 给 PM
    │
    ├── ④ *rmc = *rmp (复制 mproc)
    ├── ⑤ 恢复 sigact 指针
    ├── ⑥ rmc->mp_parent = who_p
    ├── ⑦ 清除追踪器
    ├── ⑧ 特权进程 scheduler = SCHED_PROC_NR
    ├── ⑨ rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED)
    ├── ⑩ 重置统计/定时器
    ├── ⑪ rmc->mp_pid = get_free_pid()
    ├── ⑫ tell_vfs(VFS_PM_FORK)
    │       │
    │       ▼ IPC: VFS_PM_FORK (异步)
    │   VFS: pm_fork()            [vfs/misc.c]
    │       ├── okendpt(pproc, &parentno)
    │       ├── fproc[childno] = fproc[parentno]
    │       ├── filp_count++ (所有打开的文件)
    │       ├── cp->fp_pid = cpid
    │       ├── cp->fp_endpoint = cproc
    │       ├── cp->fp_flags = FP_NOFLAGS
    │       ├── dup_vnode(fp_rd)
    │       ├── dup_vnode(fp_wd)
    │       └── 回复 PM (VFS_PM_FORK_REPLY)
    │
    ├── ⑬ 追踪器处理 (SIGSTOP)
    └── ⑭ return SUSPEND
```
