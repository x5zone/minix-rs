# Kernel 进程结构体 (KProcess) 设计

> 本文档详细说明内核进程结构体设计和 sys_fork 实现。

---

## 1. C 源码分析

### 1.1 proc 结构体（关键字段）

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

### 1.2 RTS 标志位

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

### 1.3 内核 do_fork() 核心逻辑

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

---

## 2. Rust 实现设计

### 2.1 KProcess 结构体

**文件**: `os/kernel/src/proc.rs`

```rust
use minix_types::{Endpoint, Pid, Clock, VirBytes, ProcIndex, NR_PROCS};
use bitflags::bitflags;

bitflags! {
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

bitflags! {
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

### 2.2 Endpoint Generation 管理

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

### 2.3 内核 sys_fork 实现

**文件**: `os/kernel/src/system/do_fork.rs`

```rust
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

---

## 3. 验证目标

- [ ] KProcess 结构体字段与 Minix3 C 代码对应
- [ ] RTS 标志位完整定义
- [ ] sys_fork 正确递增 generation
- [ ] 特权进程降级逻辑正确
- [ ] VMINHIBIT 标志正确设置
- [ ] 子进程 fork 返回值为 0
