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
                index: parent.identity.id.index,  // ❌ 错误
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
            started: parent.resources.started,  // ❌ 错误
            timer: None,
            intervals: parent.resources.intervals,  // ❌ 错误
            nice: parent.resources.nice,
            scheduler: parent.resources.scheduler,  // ⚠️ 需特殊处理
            flags: parent.resources.flags,  // ❌ 错误
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
| 3 | `resources.intervals` | `parent.resources.intervals` | `[0; 3]` | 🔴 高 |
| 4 | `resources.flags` | `parent.resources.flags` | 只保留 TAINTED | 🔴 高 |
| 5 | `resources.scheduler` | 直接继承 | 特权进程需设为 SCHED | 🟡 中 |
| 6 | 缺少 `child_index` 参数 | — | 需要添加 | 🔴 高 |

---

## 四、实现方案对比

### 方案一：整体拷贝 + 字段修正（C 风格）

#### 设计思路

模仿 C 的 `*rmc = *rmp`，先整体克隆，再修正特定字段。

#### 代码实现

```rust
impl Process {
    pub fn fork_from_c_style(
        parent: &Process,
        child_index: usize,
        child_pid: Pid,
        child_endpoint: Endpoint,
    ) -> Self {
        let mut child = parent.clone();
        
        // 修正身份字段
        child.identity.id.index = ProcIndex::new(child_index);
        child.identity.id.pid = child_pid;
        child.identity.endpoint = child_endpoint;
        
        // 修正状态字段
        child.state.lifecycle = Lifecycle::Running;
        child.state.guardianship = Guardianship::Normal {
            parent: parent.identity.id.index,
        };
        child.state.trace = TraceState::default();
        
        // 修正资源字段
        child.resources.child_utime = 0;
        child.resources.child_stime = 0;
        child.resources.started = getticks();
        child.resources.intervals = [0; NR_ITIMERS];
        child.resources.timer = None;
        
        // 标志位处理：只保留 TAINTED
        let mut flags = RemainingFlags::empty();
        if parent.resources.flags.contains(RemainingFlags::TAINTED) {
            flags |= RemainingFlags::TAINTED;
        }
        child.resources.flags = flags;
        
        // 特权进程处理
        if matches!(parent.resources.privilege, Privilege::Kernel) {
            child.resources.scheduler = Endpoint::RS; // SCHED_PROC_NR
        }
        
        // IPC 字段重置
        child.ipc.reply = None;
        child.ipc.event_subscriber = None;
        
        child
    }
}
```

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
```

---

### 方案二：显式字段构造（推荐）

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
        // 标志位处理：只保留 TAINTED
        let mut flags = RemainingFlags::empty();
        if parent.resources.flags.contains(RemainingFlags::TAINTED) {
            flags |= RemainingFlags::TAINTED;
        }
        
        // 特权进程的 scheduler 处理
        let scheduler = match &parent.resources.privilege {
            Privilege::Kernel => Endpoint::RS, // SCHED_PROC_NR
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

#### 代码实现

```rust
pub struct ProcessForkBuilder<'a> {
    parent: &'a Process,
    child_index: usize,
    child_pid: Pid,
    child_endpoint: Endpoint,
    inherit_tracer: bool,
    inherit_priv_flags: bool,
}

impl<'a> ProcessForkBuilder<'a> {
    pub fn new(parent: &'a Process) -> Self {
        Self {
            parent,
            child_index: 0,
            child_pid: 0,
            child_endpoint: Endpoint::NONE,
            inherit_tracer: false,
            inherit_priv_flags: false,
        }
    }
    
    pub fn child_index(mut self, index: usize) -> Self {
        self.child_index = index;
        self
    }
    
    pub fn child_pid(mut self, pid: Pid) -> Self {
        self.child_pid = pid;
        self
    }
    
    pub fn child_endpoint(mut self, endpoint: Endpoint) -> Self {
        self.child_endpoint = endpoint;
        self
    }
    
    pub fn inherit_tracer(mut self, inherit: bool) -> Self {
        self.inherit_tracer = inherit;
        self
    }
    
    pub fn build(self) -> Process {
        let mut flags = RemainingFlags::empty();
        if self.parent.resources.flags.contains(RemainingFlags::TAINTED) {
            flags |= RemainingFlags::TAINTED;
        }
        
        let guardianship = if self.inherit_tracer {
            self.parent.state.guardianship.clone()
        } else {
            Guardianship::Normal {
                parent: self.parent.identity.id.index,
            }
        };
        
        Process {
            identity: ProcessIdentity {
                id: ProcessId {
                    index: ProcIndex::new(self.child_index),
                    pid: self.child_pid,
                },
                endpoint: self.child_endpoint,
                procgrp: self.parent.identity.procgrp,
                name: self.parent.identity.name,
            },
            state: ProcessState {
                lifecycle: Lifecycle::Running,
                block: BlockState::default(),
                wait: WaitState::default(),
                guardianship,
                trace: if self.inherit_tracer {
                    self.parent.state.trace.clone()
                } else {
                    TraceState::default()
                },
            },
            resources: ProcessResources {
                privilege: self.parent.resources.privilege.clone(),
                signals: self.parent.resources.signals.clone(),
                child_utime: 0,
                child_stime: 0,
                started: getticks(),
                timer: None,
                intervals: [0; NR_ITIMERS],
                nice: self.parent.resources.nice,
                scheduler: self.parent.resources.scheduler,
                flags,
            },
            ipc: ProcessIpc::default(),
        }
    }
}

// 使用方式
let child = ProcessForkBuilder::new(&parent)
    .child_index(child_index)
    .child_pid(child_pid)
    .child_endpoint(child_endpoint)
    .build();
```

#### 优缺点分析

| 优点 | 缺点 |
|------|------|
| ✅ 可选参数清晰 | ❌ 过度工程化 |
| ✅ 易于扩展 | ❌ 增加代码复杂度 |
| ✅ 链式调用优雅 | ❌ 性能略差（多次函数调用） |
| ✅ 支持不同 fork 场景 | ❌ 需要额外的 Builder 结构体 |

---

## 五、方案对比总结

### 5.1 对比表

| 维度 | 方案一：整体拷贝 | 方案二：显式构造 | 方案三：Builder |
|------|-----------------|-----------------|----------------|
| **代码量** | ⭐⭐⭐ 少 | ⭐⭐ 中等 | ⭐ 多 |
| **类型安全** | ⭐ 低 | ⭐⭐⭐ 高 | ⭐⭐⭐ 高 |
| **维护性** | ⭐ 差 | ⭐⭐⭐ 好 | ⭐⭐ 中 |
| **性能** | ⭐⭐⭐ 好 | ⭐⭐ 中 | ⭐ 一般 |
| **扩展性** | ⭐ 差 | ⭐⭐ 中 | ⭐⭐⭐ 好 |
| **与 C 一致性** | ⭐⭐⭐ 高 | ⭐⭐ 中 | ⭐ 低 |

### 5.2 推荐方案

**推荐方案二：显式字段构造**

#### 推荐理由

1. **类型安全**：编译器帮助发现遗漏的字段
2. **维护性好**：新增字段时编译器强制检查
3. **代码审查友好**：每个字段的行为一目了然
4. **符合 Rust 哲学**：显式优于隐式

#### 适用场景

- ✅ 操作系统内核开发（正确性优先）
- ✅ 需要长期维护的项目
- ✅ 团队协作开发

#### 不适用场景

- ❌ 快速原型开发
- ❌ 性能极度敏感的热路径

---

## 六、实现细节

### 6.1 需要添加的依赖

```rust
// 获取当前时间（需要实现）
fn getticks() -> Clock {
    // TODO: 调用内核获取时钟滴答数
    0
}
```

### 6.2 需要修改的函数签名

```rust
// 当前签名
pub fn fork_from(parent: &Process, child_pid: Pid, child_endpoint: Endpoint, parent_index: usize) -> Self

// 修正后的签名
pub fn fork_from(
    parent: &Process,
    child_index: usize,      // 新增：子进程索引
    child_pid: Pid,
    child_endpoint: Endpoint,
) -> Self
```

### 6.3 测试用例

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
        let child = Process::fork_from(&parent, 5, 200, Endpoint::new(5));
        
        assert_eq!(child.identity.id.index, ProcIndex::new(5));
        assert_eq!(child.identity.id.pid, 200);
    }

    #[test]
    fn test_fork_inherited_fields() {
        let parent = create_parent_process();
        let child = Process::fork_from(&parent, 5, 200, Endpoint::new(5));
        
        // 继承的字段
        assert_eq!(child.identity.procgrp, parent.identity.procgrp);
        assert_eq!(child.resources.nice, parent.resources.nice);
    }

    #[test]
    fn test_fork_cleared_fields() {
        let parent = create_parent_process();
        let child = Process::fork_from(&parent, 5, 200, Endpoint::new(5));
        
        // 清零的字段
        assert_eq!(child.resources.child_utime, 0);
        assert_eq!(child.resources.child_stime, 0);
        assert_eq!(child.resources.intervals, [0; NR_ITIMERS]);
    }

    #[test]
    fn test_fork_flags_handling() {
        let mut parent = create_parent_process();
        parent.resources.flags = RemainingFlags::TAINTED | RemainingFlags::ALARM_ON;
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint::new(5));
        
        // 只保留 TAINTED
        assert!(child.resources.flags.contains(RemainingFlags::TAINTED));
        assert!(!child.resources.flags.contains(RemainingFlags::ALARM_ON));
    }

    #[test]
    fn test_fork_parent_relationship() {
        let parent = create_parent_process();
        let child = Process::fork_from(&parent, 5, 200, Endpoint::new(5));
        
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
        
        let child = Process::fork_from(&parent, 5, 200, Endpoint::new(5));
        
        // 追踪器应被清除
        assert!(child.tracer().is_none());
    }
}
```

---

## 七、验证清单

### 7.1 字段验证

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

### 7.2 特殊场景验证

- [ ] 特权进程的 `scheduler` 正确设置
- [ ] 追踪进程 fork 时追踪器被清除
- [ ] 污染标记正确继承
- [ ] 信号处理函数正确继承

---

## 八、下一步行动

1. **修正 `Process::fork_from()` 函数**
   - 添加 `child_index` 参数
   - 修正所有字段复制逻辑
   - 添加标志位处理

2. **实现 `getticks()` 函数**
   - 调用内核获取时钟滴答数
   - 或使用 Mock 版本用于测试

3. **添加单元测试**
   - 覆盖所有字段验证
   - 覆盖特殊场景

4. **更新 `do_fork_prepare()` 调用**
   - 传递正确的 `child_index`
