# Fork 系统调用实现文档 — Part 4：进程结构复制与初始化

> **范围**: 阶段 4 — do_fork 核心逻辑（下）：进程结构复制与初始化
> **前置**: 完成 [fork-rewr-03.md](fork-rewr-03.md) 中的 PID 生成器
> **参考**: [fork-syscall-plan-part2.md](fork-syscall-plan-part2.md) 第四阶段

---

## 一、Minix3 源码分析

### 1.1 源码位置

**文件**: `minix3/minix/servers/pm/forkexit.c` — `do_fork()` 第 90~145 行

### 1.2 完整 C 源码

```c
  // ④ 获取子进程槽位指针，增加计数
  rmc = &mproc[next_child];
  procs_in_use++;

  // ⑤ 复制父进程 mproc 到子进程（整体拷贝）
  *rmc = *rmp;

  // ⑥ 恢复 mp_sigact 指针（因为 *rmc = *rmp 会覆盖指针）
  rmc->mp_sigact = mpsigact[next_child];
  memcpy(rmc->mp_sigact, rmp->mp_sigact, sizeof(mpsigact[next_child]));

  // ⑦ 设置父子关系
  rmc->mp_parent = who_p;  // who_p 是发起 fork 的进程索引

  // ⑧ 清除追踪器（非 TRACEFORK 场景）
  if (!(rmc->mp_trace_flags & TO_TRACEFORK)) {
    rmc->mp_tracer = NO_TRACER;
    rmc->mp_trace_flags = 0;
    sigemptyset(&rmc->mp_sigtrace);
  }

  // ⑨ 特权进程处理
  if (rmc->mp_flags & PRIV_PROC) {
    assert(rmc->mp_scheduler == NONE);
    rmc->mp_scheduler = SCHED_PROC_NR;
  }

  // ⑩ 继承/重置标志位和统计信息
  rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED);
  rmc->mp_child_utime = 0;
  rmc->mp_child_stime = 0;
  rmc->mp_exitstatus = 0;
  rmc->mp_sigstatus = 0;
  rmc->mp_endpoint = child_ep;  // 从 VM 返回的端点
  for (i = 0; i < NR_ITIMERS; i++)
    rmc->mp_interval[i] = 0;
  rmc->mp_started = getticks();

  assert(rmc->mp_eventsub == NO_EVENTSUB);

  // ⑪ 分配 PID
  new_pid = get_free_pid();
  rmc->mp_pid = new_pid;
```

### 1.3 核心操作分解

| 步骤 | 操作 | 说明 |
|------|------|------|
| ④ | 获取槽位指针 | `rmc = &mproc[next_child]` |
| ⑤ | 整体拷贝 | `*rmc = *rmp`（C 的结构体赋值） |
| ⑥ | 恢复 sigact 指针 | 特殊处理，因为是指针 |
| ⑦ | 设置父进程 | `rmc->mp_parent = who_p` |
| ⑧ | 清除追踪器 | 除非 `TO_TRACEFORK` |
| ⑨ | 特权进程处理 | 设置 `SCHED_PROC_NR` |
| ⑩ | 重置字段 | 标志位、统计信息、定时器 |
| ⑪ | 分配 PID | `get_free_pid()` |

---

## 二、字段复制规则详解

### 2.1 字段分类

根据 fork 行为，将字段分为以下几类：

#### A. 完全继承（直接复制）

| Minix3 字段 | Rust 字段 | 说明 |
|------------|----------|------|
| `mp_procgrp` | `identity.procgrp` | 子进程加入同一进程组 |
| `mp_name` | `identity.name` | 进程名相同 |
| `mp_realuid` | `resources.privilege.real_uid` | 真实 UID |
| `mp_effuid` | `resources.privilege.eff_uid` | 有效 UID |
| `mp_svuid` | `resources.privilege.sav_uid` | 保存的 UID |
| `mp_realgid` | `resources.privilege.real_gid` | 真实 GID |
| `mp_effgid` | `resources.privilege.eff_gid` | 有效 GID |
| `mp_svgid` | `resources.privilege.sav_gid` | 保存的 GID |
| `mp_nice` | `resources.nice` | 调度优先级 |
| `mp_sigact` | `resources.signals` | 信号处理函数 |

#### B. 新分配/设置

| Minix3 字段 | Rust 字段 | 来源 |
|------------|----------|------|
| `mp_pid` | `identity.id.pid` | `get_free_pid()` |
| `mp_endpoint` | `identity.endpoint` | VM 返回 |
| `mp_parent` | `state.guardianship.parent` | 调用者索引 |
| `mp_started` | `resources.started` | `getticks()` |

#### C. 清零/重置

| Minix3 字段 | Rust 字段 | 新值 |
|------------|----------|------|
| `mp_child_utime` | `resources.child_utime` | `0` |
| `mp_child_stime` | `resources.child_stime` | `0` |
| `mp_exitstatus` | — | `0` |
| `mp_sigstatus` | — | `0` |
| `mp_interval[]` | `resources.intervals` | `[0; 3]` |
| `mp_tracer` | `state.guardianship.tracer` | `None` |
| `mp_trace_flags` | `state.trace` | `default()` |
| `mp_reply` | `ipc.reply` | `None` |
| `mp_eventsub` | `ipc.event_subscriber` | `None` |

#### D. 标志位处理

```c
rmc->mp_flags &= (IN_USE|DELAY_CALL|TAINTED);
```

**只保留三个标志**：

| 标志 | 含义 | 是否保留 |
|------|------|---------|
| `IN_USE` | 槽位已使用 | ✅ 保留（新进程必须） |
| `DELAY_CALL` | 延迟调用 | ✅ 保留（继承） |
| `TAINTED` | 污染标记 | ✅ 保留（继承） |
| `WAITING` | 父进程在等待 | ❌ 清除 |
| `ZOMBIE` | 僵尸状态 | ❌ 清除 |
| `EXITING` | 正在退出 | ❌ 清除 |
| `VFS_CALL` | 等待 VFS | ❌ 清除 |
| `PRIV_PROC` | 特权进程 | ❌ 特殊处理（见⑨） |

#### E. 特殊处理

**特权进程的 scheduler**：

```c
if (rmc->mp_flags & PRIV_PROC) {
    assert(rmc->mp_scheduler == NONE);
    rmc->mp_scheduler = SCHED_PROC_NR;
}
```

**追踪器处理**：

```c
if (!(rmc->mp_trace_flags & TO_TRACEFORK)) {
    rmc->mp_tracer = NO_TRACER;
    rmc->mp_trace_flags = 0;
    sigemptyset(&rmc->mp_sigtrace);
}
```

---

## 三、当前 Rust 实现分析

### 3.1 现有代码

**文件**: `os/servers/pm/src/mproc/fork.rs`

```rust
pub fn fork_from(parent: &Process, child_pid: Pid, child_endpoint: Endpoint, parent_index: usize) -> Self {
    Self {
        identity: ProcessIdentity {
            id: ProcessId {
                index: parent.identity.id.index,  // ❌ 错误：应该是 child_index
                pid: child_pid,
            },
            endpoint: child_endpoint,
            procgrp: parent.identity.procgrp,
            name: parent.identity.name,
        },
        state: ProcessState {
            lifecycle: Lifecycle::Running,
            block: BlockState::default(),
            wait: WaitState::default(),
            guardianship: Guardianship::Normal { 
                parent: ProcIndex::new(parent_index) 
            },
            trace: TraceState::default(),
        },
        resources: ProcessResources {
            privilege: parent.resources.privilege.clone(),
            signals: parent.resources.signals.clone(),
            child_utime: 0,
            child_stime: 0,
            started: parent.resources.started,  // ❌ 错误：应该是 getticks()
            timer: None,
            intervals: parent.resources.intervals,  // ❌ 错误：应该是 [0; NR_ITIMERS]
            nice: parent.resources.nice,
            scheduler: parent.resources.scheduler,  // ⚠️ 需特殊处理
            flags: parent.resources.flags,  // ❌ 错误：应该只保留 TAINTED
        },
        ipc: ProcessIpc::default(),
    }
}
```

### 3.2 问题清单

| # | 问题 | 当前值 | 正确值 | 严重程度 |
|---|------|--------|--------|---------|
| 1 | `identity.id.index` | `parent.identity.id.index` | `child_index` | 🔴 高 |
| 2 | `resources.started` | `parent.resources.started` | `getticks()` | 🔴 高 |
| 3 | `resources.intervals` | `parent.resources.intervals` | `[0; NR_ITIMERS]` | 🔴 高 |
| 4 | `resources.flags` | `parent.resources.flags` | 只保留 TAINTED | 🔴 高 |
| 5 | `resources.scheduler` | 直接继承 | 特权进程需设为 SCHED | 🟡 中 |
| 6 | 缺少 `child_index` 参数 | — | 需要添加 | 🔴 高 |

---

## 四、实现方案对比

### 方案一：整体拷贝 + 字段修正（C 风格）

#### 设计思路

模仿 C 的 `*rmc = *rmp`，先整体克隆，再修正特定字段。

#### 优缺点分析

| 优点 | 缺点 |
|------|------|
| ✅ 与 C 代码逻辑一致 | ❌ 隐式依赖 `Clone` 实现 |
| ✅ 实现简单直接 | ❌ 容易遗漏需要修正的字段 |
| ✅ 性能好（一次拷贝） | ❌ 不符合 Rust 显式风格 |
| ✅ 代码量少 | ❌ 维护困难（新增字段易遗漏） |

#### 风险分析

```
风险：新增字段时容易遗漏修正

例如：如果未来在 Process 中添加了新字段 `foo`，
     它会被自动继承（因为 clone()），
     但可能需要清零或特殊处理。

更深的坑：如果 Process 结构体中包含了智能指针（如 Arc<Mutex<T>>）
         或裸指针，clone() 会导致浅拷贝（Shallow Copy）。
         后果：父子进程会共享同一块内存资源。
```

---

### 方案二：显式字段构造（基础推荐）

#### 设计思路

显式构造每个字段，强迫开发者检查每个字段的 fork 行为。

#### 代码实现

```rust
impl Process {
    pub fn fork_from(
        parent: &Process,
        child_index: usize,
        child_pid: Pid,
        child_endpoint: Endpoint,
    ) -> Self {
        let mut flags = RemainingFlags::empty();
        if parent.resources.flags.contains(RemainingFlags::TAINTED) {
            flags |= RemainingFlags::TAINTED;
        }
        
        let scheduler = match &parent.resources.privilege {
            Privilege::Kernel => Endpoint::RS,
            Privilege::User(_) => parent.resources.scheduler,
        };

        Self {
            identity: ProcessIdentity {
                id: ProcessId {
                    index: ProcIndex::new(child_index),
                    pid: child_pid,
                },
                endpoint: child_endpoint,
                procgrp: parent.identity.procgrp,
                name: parent.identity.name,
            },
            state: ProcessState {
                lifecycle: Lifecycle::Running,
                block: BlockState::default(),
                wait: WaitState::default(),
                guardianship: Guardianship::Normal {
                    parent: parent.identity.id.index,
                },
                trace: TraceState::default(),
            },
            resources: ProcessResources {
                privilege: parent.resources.privilege.clone(),
                signals: parent.resources.signals.clone(),
                child_utime: 0,
                child_stime: 0,
                started: getticks(),
                timer: None,
                intervals: [0; NR_ITIMERS],
                nice: parent.resources.nice,
                scheduler,
                flags,
            },
            ipc: ProcessIpc::default(),
        }
    }
}
```

#### 优缺点分析

| 优点 | 缺点 |
|------|------|
| ✅ 强迫检查每个字段 | ❌ 代码较长 |
| ✅ 编译器帮助发现遗漏 | ❌ 新增字段需手动添加 |
| ✅ 符合 Rust 显式风格 | ❌ 初次编写工作量大 |
| ✅ 易于代码审查 | — |
| ✅ 维护时不易出错 | — |

#### 编译器帮助

```rust
// 如果 Process 新增字段，编译器会报错：
// error[E0063]: missing field `new_field` in initializer of `Process`
```

---

### 方案三：Builder 模式

#### 设计思路

使用 Builder 模式，提供更灵活的构造方式。

#### 优缺点分析

| 优点 | 缺点 |
|------|------|
| ✅ 可选参数清晰 | ❌ 过度工程化 |
| ✅ 易于扩展 | ❌ 增加代码复杂度 |
| ✅ 链式调用优雅 | ❌ 性能略差（多次函数调用） |
| ✅ 支持不同 fork 场景 | ❌ 需要额外的 Builder 结构体 |

#### 不推荐理由

> fork 是一个原子操作，它的参数和行为是内核协议规定的，
> 通常不需要像构造 UI 组件那样提供灵活的可选配置。
> 内核代码追求的是路径的可预测性。

---

### 方案四：语义拆分构造（最终推荐）

#### 设计思路

> 把 fork 变成"语义操作"，而不是"结构体构造"。
> 
> 核心思想：不要直接构造 `Process`，先构造 `ForkContext`，
> 然后按语义拆分构造。

这是在方案二基础上的升级，将"字段正确"升级为"语义正确"。

#### Step 1：定义 ForkContext

```rust
/// fork 上下文
///
/// 封装 fork 所需的所有参数，便于传递和扩展
pub struct ForkContext<'a> {
    /// 父进程引用
    pub parent: &'a Process,
    /// 子进程索引
    pub child_index: usize,
    /// 子进程 PID
    pub child_pid: Pid,
    /// 子进程 Endpoint
    pub child_endpoint: Endpoint,
}
```

#### Step 2：按语义拆分构造

```rust
impl Process {
    pub fn fork_from(ctx: ForkContext) -> Self {
        Self {
            identity: Self::fork_identity(&ctx),
            state: Self::fork_state(&ctx),
            resources: Self::fork_resources(&ctx),
            ipc: ProcessIpc::default(),
        }
    }
}
```

#### Step 3：每个语义块独立实现

```rust
impl Process {
    /// fork 身份信息
    ///
    /// 语义：纯继承 + 覆盖新值
    fn fork_identity(ctx: &ForkContext) -> ProcessIdentity {
        ProcessIdentity {
            id: ProcessId {
                index: ProcIndex::new(ctx.child_index),
                pid: ctx.child_pid,
            },
            endpoint: ctx.child_endpoint,
            procgrp: ctx.parent.identity.procgrp,
            name: ctx.parent.identity.name,
        }
    }

    /// fork 状态信息
    ///
    /// 语义：重置为新进程状态
    fn fork_state(ctx: &ForkContext) -> ProcessState {
        ProcessState {
            lifecycle: Lifecycle::Running,
            block: BlockState::default(),
            wait: WaitState::default(),
            guardianship: Guardianship::Normal {
                parent: ctx.parent.identity.id.index,
            },
            trace: TraceState::default(),
        }
    }

    /// fork 资源信息
    ///
    /// 语义：继承权限/信号，重置统计/定时器
    fn fork_resources(ctx: &ForkContext) -> ProcessResources {
        ProcessResources {
            privilege: ctx.parent.resources.privilege.clone(),
            signals: ctx.parent.resources.signals.clone(),
            child_utime: 0,
            child_stime: 0,
            started: getticks(),
            timer: None,
            intervals: [0; NR_ITIMERS],
            nice: ctx.parent.resources.nice,
            scheduler: Self::fork_scheduler(ctx.parent),
            flags: Self::fork_flags(ctx.parent),
        }
    }

    /// fork 标志位
    ///
    /// 语义：只保留 TAINTED
    fn fork_flags(parent: &Process) -> RemainingFlags {
        let mut flags = RemainingFlags::empty();
        if parent.resources.flags.contains(RemainingFlags::TAINTED) {
            flags |= RemainingFlags::TAINTED;
        }
        flags
    }

    /// fork 调度器
    ///
    /// 语义：特权进程使用 RS，普通进程继承
    fn fork_scheduler(parent: &Process) -> Endpoint {
        match &parent.resources.privilege {
            Privilege::Kernel => Endpoint::RS,
            Privilege::User(_) => parent.resources.scheduler,
        }
    }
}
```

#### 优缺点分析

| 优点 | 缺点 |
|------|------|
| ✅ fork 语义被"函数化" | ❌ 代码量增加 |
| ✅ 新字段不会 silent bug | ❌ 需要额外的 ForkContext 结构体 |
| ✅ 接近"内核文档本身" | — |
| ✅ 易于扩展和测试 | — |
| ✅ 便于添加审计注释 | — |

#### 语义函数的价值

```rust
// 现在可以读代码：
fork_resources()  // 一看就知道是 fork 资源
fork_state()      // 一看就知道是 fork 状态
fork_identity()   // 一看就知道是 fork 身份

// 这已经接近"内核文档本身"
```

---

### 方案五：子组件封装（配合方案四）

#### 设计思路

将 `ProcessResources`、`ProcessIdentity` 等子结构体分别实现 `fork_to_child()` 方法。

#### 代码实现

```rust
impl ProcessResources {
    /// fork 到子进程
    ///
    /// 封装所有资源字段的 fork 逻辑
    pub fn fork_to_child(&self) -> Self {
        Self {
            privilege: self.privilege.clone(),
            signals: self.signals.clone(),
            child_utime: 0,
            child_stime: 0,
            started: getticks(),
            timer: None,
            intervals: [0; NR_ITIMERS],
            nice: self.nice,
            scheduler: self.fork_scheduler(),
            flags: self.fork_flags(),
        }
    }

    fn fork_flags(&self) -> RemainingFlags {
        let mut flags = RemainingFlags::empty();
        if self.flags.contains(RemainingFlags::TAINTED) {
            flags |= RemainingFlags::TAINTED;
        }
        flags
    }

    fn fork_scheduler(&self) -> Endpoint {
        match &self.privilege {
            Privilege::Kernel => Endpoint::RS,
            Privilege::User(_) => self.scheduler,
        }
    }
}
```

#### 使用方式

```rust
impl Process {
    pub fn fork_from(ctx: ForkContext) -> Self {
        Self {
            identity: ctx.parent.identity.fork_to_child(ctx.child_index, ctx.child_pid, ctx.child_endpoint),
            state: ProcessState::fork_new(ctx.parent.identity.id.index),
            resources: ctx.parent.resources.fork_to_child(),
            ipc: ProcessIpc::default(),
        }
    }
}
```

#### 优缺点分析

| 优点 | 缺点 |
|------|------|
| ✅ 职责分散到各子组件 | ❌ 需要修改多个结构体 |
| ✅ 主函数非常清晰 | ❌ 子组件需要访问父进程信息 |
| ✅ 便于独立测试 | — |

---

## 五、方案对比总结

### 5.1 对比表

| 维度 | 方案一 | 方案二 | 方案三 | 方案四 | 方案五 |
|------|--------|--------|--------|--------|--------|
| **代码量** | ⭐⭐⭐ 少 | ⭐⭐ 中 | ⭐ 多 | ⭐⭐ 中 | ⭐⭐ 中 |
| **类型安全** | ⭐ 低 | ⭐⭐⭐ 高 | ⭐⭐⭐ 高 | ⭐⭐⭐ 高 | ⭐⭐⭐ 高 |
| **语义清晰** | ⭐ 差 | ⭐⭐ 中 | ⭐⭐ 中 | ⭐⭐⭐ 高 | ⭐⭐⭐ 高 |
| **维护性** | ⭐ 差 | ⭐⭐⭐ 好 | ⭐⭐ 中 | ⭐⭐⭐ 好 | ⭐⭐⭐ 好 |
| **可测试性** | ⭐ 差 | ⭐⭐ 中 | ⭐⭐ 中 | ⭐⭐⭐ 好 | ⭐⭐⭐ 好 |
| **扩展性** | ⭐ 差 | ⭐⭐ 中 | ⭐⭐⭐ 好 | ⭐⭐⭐ 好 | ⭐⭐⭐ 好 |

### 5.2 推荐方案

**推荐：方案四（语义拆分构造）+ 方案五（子组件封装）**

#### 推荐理由

1. **语义正确**：从"字段正确"升级为"语义正确"
2. **类型安全**：编译器帮助发现遗漏的字段
3. **可读性强**：代码接近"内核文档本身"
4. **易于维护**：新增字段时编译器强制检查
5. **便于测试**：每个语义函数可独立测试

---

## 六、实现细节

### 6.1 审计注释

在每个字段赋值处增加简短注释，说明 fork 行为：

```rust
fn fork_resources(ctx: &ForkContext) -> ProcessResources {
    ProcessResources {
        // 继承：权限凭证
        privilege: ctx.parent.resources.privilege.clone(),
        
        // 继承：信号处理函数（深拷贝）
        signals: ctx.parent.resources.signals.clone(),
        
        // 重置：子进程时间统计
        child_utime: 0,
        child_stime: 0,
        
        // 新值：进程启动时间
        started: getticks(),
        
        // 重置：定时器
        timer: None,
        intervals: [0; NR_ITIMERS],
        
        // 继承：调度优先级
        nice: ctx.parent.resources.nice,
        
        // 特殊：调度器端点
        scheduler: Self::fork_scheduler(ctx.parent),
        
        // 过滤：只保留 TAINTED
        flags: Self::fork_flags(ctx.parent),
    }
}
```

### 6.2 不变量验证

添加 fork 后的不变量验证：

```rust
impl Process {
    /// 验证 fork 后的状态
    ///
    /// 在 debug 模式下检查 fork 不变量
    #[cfg(debug_assertions)]
    fn validate_after_fork(&self) {
        debug_assert!(self.resources.child_utime == 0, "child_utime must be 0");
        debug_assert!(self.resources.child_stime == 0, "child_stime must be 0");
        debug_assert!(self.ipc.reply.is_none(), "reply must be None");
        debug_assert!(self.ipc.event_subscriber.is_none(), "event_subscriber must be None");
        debug_assert!(self.resources.timer.is_none(), "timer must be None");
        debug_assert_eq!(self.resources.intervals, [0; NR_ITIMERS], "intervals must be zeroed");
        debug_assert!(matches!(self.state.lifecycle, Lifecycle::Running), "lifecycle must be Running");
    }
}
```

### 6.3 深拷贝注意事项

**信号处理的深拷贝**：

```rust
// Minix3 源码：
// rmc->mp_sigact = mpsigact[next_child];
// memcpy(rmc->mp_sigact, rmp->mp_sigact, sizeof(mpsigact[next_child]));

// Rust 实现：
// 确保 signals.clone() 执行的是深拷贝
// 如果 signals 内部持有引用，需要特殊处理
```

**检查清单**：
- [ ] `signals.clone()` 是深拷贝
- [ ] `privilege.clone()` 是深拷贝
- [ ] `name` 数组是值拷贝（正确）

### 6.4 Drop 语义陷阱

如果 `Process` 结构体包含实现了 `Drop` 的字段：

```rust
// 风险：在"显式构造"模式下，如果构造到一半发生 Panic，
// Rust 会自动调用已构造字段的 Drop 函数。

// 建议：确保 Drop 函数是幂等的且无副作用的
// 或者使用原子性构造模式（先构造临时对象，再 swap）
```

### 6.5 需要修改的函数签名

```rust
// 当前签名
pub fn fork_from(parent: &Process, child_pid: Pid, child_endpoint: Endpoint, parent_index: usize) -> Self

// 修正后的签名（方案二）
pub fn fork_from(
    parent: &Process,
    child_index: usize,      // 新增：子进程索引
    child_pid: Pid,
    child_endpoint: Endpoint,
) -> Self

// 方案四签名
pub fn fork_from(ctx: ForkContext) -> Self
```

---

## 七、测试策略

### 7.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn create_parent_process() -> Process {
        let mut proc = Process::new(0, 100);
        proc.identity.procgrp = 100;
        proc.resources.nice = 5;
        proc.resources.flags = RemainingFlags::TAINTED | RemainingFlags::ALARM_ON;
        proc.state.lifecycle = Lifecycle::Running;
        proc
    }

    #[test]
    fn test_fork_child_index() {
        let parent = create_parent_process();
        let ctx = ForkContext {
            parent: &parent,
            child_index: 5,
            child_pid: 200,
            child_endpoint: Endpoint::new(5),
        };
        let child = Process::fork_from(ctx);
        
        assert_eq!(child.identity.id.index, ProcIndex::new(5));
        assert_eq!(child.identity.id.pid, 200);
    }

    #[test]
    fn test_fork_inherited_fields() {
        let parent = create_parent_process();
        let ctx = ForkContext { parent: &parent, child_index: 5, child_pid: 200, child_endpoint: Endpoint::new(5) };
        let child = Process::fork_from(ctx);
        
        assert_eq!(child.identity.procgrp, parent.identity.procgrp);
        assert_eq!(child.resources.nice, parent.resources.nice);
    }

    #[test]
    fn test_fork_cleared_fields() {
        let parent = create_parent_process();
        let ctx = ForkContext { parent: &parent, child_index: 5, child_pid: 200, child_endpoint: Endpoint::new(5) };
        let child = Process::fork_from(ctx);
        
        assert_eq!(child.resources.child_utime, 0);
        assert_eq!(child.resources.child_stime, 0);
        assert_eq!(child.resources.intervals, [0; NR_ITIMERS]);
    }

    #[test]
    fn test_fork_flags_handling() {
        let mut parent = create_parent_process();
        parent.resources.flags = RemainingFlags::TAINTED | RemainingFlags::ALARM_ON;
        
        let ctx = ForkContext { parent: &parent, child_index: 5, child_pid: 200, child_endpoint: Endpoint::new(5) };
        let child = Process::fork_from(ctx);
        
        assert!(child.resources.flags.contains(RemainingFlags::TAINTED));
        assert!(!child.resources.flags.contains(RemainingFlags::ALARM_ON));
    }

    #[test]
    fn test_fork_parent_relationship() {
        let parent = create_parent_process();
        let ctx = ForkContext { parent: &parent, child_index: 5, child_pid: 200, child_endpoint: Endpoint::new(5) };
        let child = Process::fork_from(ctx);
        
        assert_eq!(child.parent(), ProcIndex::new(0));
    }

    #[test]
    fn test_fork_tracer_cleared() {
        let mut parent = create_parent_process();
        parent.state.guardianship = Guardianship::Traced {
            parent: ProcIndex::new(0),
            tracer: ProcIndex::new(1),
            trace_exit: false,
            trace_options: TraceOptions::empty(),
        };
        
        let ctx = ForkContext { parent: &parent, child_index: 5, child_pid: 200, child_endpoint: Endpoint::new(5) };
        let child = Process::fork_from(ctx);
        
        assert!(child.tracer().is_none());
    }

    #[test]
    fn test_fork_privilege_scheduler() {
        let mut parent = create_parent_process();
        parent.resources.privilege = Privilege::Kernel;
        
        let ctx = ForkContext { parent: &parent, child_index: 5, child_pid: 200, child_endpoint: Endpoint::new(5) };
        let child = Process::fork_from(ctx);
        
        assert_eq!(child.resources.scheduler, Endpoint::RS);
    }
}
```

### 7.2 Fuzz 测试建议

```rust
// 建议：随机生成父进程状态，疯狂调用 fork，检查是否有内存泄漏或断言失败
#[cfg(test)]
mod fuzz_tests {
    use super::*;
    use rand::Rng;

    fn random_process(rng: &mut impl Rng) -> Process {
        let mut proc = Process::default();
        proc.identity.id.pid = rng.gen();
        proc.resources.nice = rng.gen_range(-20..=20);
        proc.resources.flags = RemainingFlags::from_bits(rng.gen()).unwrap_or_default();
        proc
    }

    #[test]
    fn fuzz_fork_invariants() {
        let mut rng = rand::thread_rng();
        
        for _ in 0..1000 {
            let parent = random_process(&mut rng);
            let ctx = ForkContext {
                parent: &parent,
                child_index: rng.gen_range(0..100),
                child_pid: rng.gen(),
                child_endpoint: Endpoint::new(rng.gen()),
            };
            
            let child = Process::fork_from(ctx);
            
            // 验证不变量
            assert_eq!(child.resources.child_utime, 0);
            assert_eq!(child.resources.child_stime, 0);
            assert!(child.ipc.reply.is_none());
        }
    }
}
```

---

## 八、验证清单

### 8.1 字段验证

- [ ] `identity.id.index` = `child_index`（不是父进程索引）
- [ ] `identity.id.pid` = `child_pid`
- [ ] `identity.endpoint` = `child_endpoint`
- [ ] `identity.procgrp` = 父进程的 procgrp
- [ ] `identity.name` = 父进程的 name
- [ ] `state.lifecycle` = `Lifecycle::Running`
- [ ] `state.guardianship.parent` = 父进程索引
- [ ] `state.guardianship.tracer` = `None`
- [ ] `resources.child_utime` = `0`
- [ ] `resources.child_stime` = `0`
- [ ] `resources.started` = `getticks()`
- [ ] `resources.intervals` = `[0; 3]`
- [ ] `resources.flags` 只保留 `TAINTED`
- [ ] `ipc.reply` = `None`
- [ ] `ipc.event_subscriber` = `None`

### 8.2 特殊场景验证

- [ ] 特权进程的 `scheduler` 正确设置
- [ ] 追踪进程 fork 时追踪器被清除
- [ ] 污染标记正确继承
- [ ] 信号处理函数正确继承（深拷贝）

### 8.3 不变量验证

- [ ] 添加 `validate_after_fork()` 函数
- [ ] 在 debug 模式下自动验证

---

## 九、下一步行动

1. **修正 `Process::fork_from()` 函数**
   - 添加 `child_index` 参数
   - 修正所有字段复制逻辑
   - 添加标志位处理

2. **实现 `getticks()` 函数**
   - 调用内核获取时钟滴答数
   - 或使用 Mock 版本用于测试

3. **选择实现方案**
   - 推荐：方案四（语义拆分）+ 方案五（子组件封装）
   - 添加审计注释
   - 添加不变量验证

4. **添加单元测试**
   - 覆盖所有字段验证
   - 覆盖特殊场景
   - 添加 Fuzz 测试

5. **更新 `do_fork_prepare()` 调用**
   - 传递正确的 `child_index`
   - 使用 `ForkContext` 封装参数
