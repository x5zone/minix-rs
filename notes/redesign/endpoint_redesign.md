# Endpoint 协议：Redesign 方案

> **目标**：探索一种更现代、类型更安全的 Endpoint 设计方案，作为未来 Redesign 的参考。
>
> **状态**：实验性设计，**跨越了 Rewrite 的红线**，展示了 Redesign 的可能性。
>
> **警告**：本方案改变了 Minix3 的语义模型，属于 Redesign 范畴，不是 Rewrite。

---

## 1. 设计哲学

### 1.1 核心原则：ABI 与语义分离 + 类型驱动

**当前 Rewrite 方案的问题**：
```rust
// ABI 和语义混在一起
pub struct Endpoint(pub i32);

// 运行时判断类型
if ep.is_kernel_task() { ... }
if ep.is_any() { ... }
```

**Redesign 方案的核心思想**：
```
┌─────────────────────────────────────┐
│  Endpoint (语义层) - 类型驱动         │
│  - 编译期区分：Process/Kernel/Special │
│  - 非法状态不可表示                   │
└──────────────┬──────────────────────┘
               │ encode/decode (显式转换)
               ▼
┌─────────────────────────────────────┐
│  EndpointRaw (ABI 层)                │
│  - 纯 i32，完全对齐 Minix3           │
│  - 仅用于 IPC/系统调用边界            │
└─────────────────────────────────────┘
```

### 1.2 为什么需要类型驱动？

| 问题 | Rewrite 方案 | Redesign 方案 |
|------|-------------|---------------|
| ANY 误用 | 运行时检查 `is_any()` | 编译期禁止：`ProcessEndpoint` 不能是 ANY |
| Kernel 进 vmproc | 运行时检查 `is_user_proc()` | 编译期禁止：`VmProc` 只接受 `ProcessEndpoint` |
| 忘记 generation 检查 | 容易遗漏 | `decode()` 强制验证 |
| Capability 化 | 困难 | 自然：`Capability<Process>` |

---

## 2. 完整类型设计

### 2.1 类型层次（Redesign 版）

```rust
// ABI 层：完全对齐 Minix3
#[repr(transparent)]
pub struct EndpointRaw(i32);

// 语义层：统一枚举，编译期区分
pub enum Endpoint {
    Process(ProcessEndpoint),    // 用户进程
    Kernel(KernelTask),          // 内核任务
    Special(SpecialEndpoint),    // ANY, NONE, SELF
}

// 进程端点（有 generation）
pub struct ProcessEndpoint {
    slot: UserSlot,
    generation: Generation,
}

// 内核任务（无 generation）
pub enum KernelTask {
    Kernel,    // -1
    System,    // -2
    Clock,     // -3
    Idle,      // -4
    Asyncm,    // -5
}

// 特殊端点
pub enum SpecialEndpoint {
    Any,       // 31744
    None,      // 31743
    Self_,     // 31742
}

// Slot 类型
pub struct UserSlot(usize);      // 0 ~ NR_PROCS-1
pub struct KernelSlot(usize);    // 0 ~ NR_TASKS+NR_PROCS-1

// Generation 类型
pub struct Generation(u16);
```

### 2.2 与 Minix3 的关键差异

| 方面 | Minix3 (C) | Rewrite (Rust) | Redesign (本方案) |
|------|-----------|----------------|-------------------|
| 表示 | `int` | `struct Endpoint { slot, gen }` | `enum Endpoint { Process, Kernel, Special }` |
| 类型检查 | 运行时 | 运行时 | **编译期** |
| ANY 处理 | `if (ep == ANY)` | `if ep.is_any()` | `match Endpoint::Special(SpecialEndpoint::Any)` |
| Kernel 区分 | `slot < 0` | `is_kernel_task()` | **类型区分** |
| 非法状态 | 可能 | 可能 | **不可能** |

---

## 3. 核心实现

### 3.1 EndpointRaw（ABI 层）

```rust
/// ABI 层端点表示
/// 
/// 完全对齐 Minix3 的 `endpoint_t`，用于 IPC、系统调用。
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EndpointRaw(i32);

impl EndpointRaw {
    /// 从原始值构造（unsafe：仅用于从 C/IPC 接收）
    /// 
    /// # Safety
    /// 调用者必须确保 raw 是合法的 endpoint 值
    pub unsafe fn from_raw_unchecked(raw: i32) -> Self {
        Self(raw)
    }
    
    /// 获取原始值（用于传递给 C/IPC）
    pub fn into_inner(self) -> i32 {
        self.0
    }
}
```

### 3.2 Endpoint（语义层 - 统一枚举）

```rust
/// 统一端点类型（Redesign 版）
/// 
/// 编译期区分进程、内核任务、特殊端点。
/// 非法状态不可表示。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    Process(ProcessEndpoint),
    Kernel(KernelTask),
    Special(SpecialEndpoint),
}

impl Endpoint {
    /// 编码为 ABI 表示
    pub fn encode(self) -> EndpointRaw {
        match self {
            Self::Process(p) => p.encode(),
            Self::Kernel(k) => k.encode(),
            Self::Special(s) => s.encode(),
        }
    }
    
    /// 从 ABI 表示解码
    pub fn decode(raw: EndpointRaw) -> Result<Self, EndpointError> {
        let slot = Self::extract_slot(raw);
        
        // 特殊端点
        if slot >= SLOT_TOP - 3 && slot < SLOT_TOP {
            return SpecialEndpoint::decode(raw).map(Self::Special);
        }
        
        // 内核任务
        if slot < 0 {
            return KernelTask::decode(raw).map(Self::Kernel);
        }
        
        // 用户进程
        ProcessEndpoint::decode(raw).map(Self::Process)
    }
    
    fn extract_slot(raw: EndpointRaw) -> i32 {
        ((raw.0 + MAX_NR_TASKS as i32) & (GENERATION_SIZE - 1)) - MAX_NR_TASKS as i32
    }
}
```

### 3.3 ProcessEndpoint（用户进程）

```rust
/// 用户进程端点
/// 
/// 保证：
/// - slot 在有效范围内 (0 ~ NR_PROCS-1)
/// - 不是特殊端点
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessEndpoint {
    slot: UserSlot,
    generation: Generation,
}

impl ProcessEndpoint {
    pub fn new(slot: UserSlot, generation: Generation) -> Self {
        Self { slot, generation }
    }
    
    pub fn encode(self) -> EndpointRaw {
        let slot_i32 = self.slot.0 as i32;
        let gen_i32 = self.generation.0 as i32;
        let raw = (gen_i32 << GENERATION_SHIFT) + slot_i32;
        unsafe { EndpointRaw::from_raw_unchecked(raw) }
    }
    
    pub fn decode(raw: EndpointRaw) -> Result<Self, EndpointError> {
        let slot_val = Self::extract_slot(raw);
        let gen_val = Self::extract_generation(raw);
        
        // 验证：必须是用户进程
        if slot_val < 0 {
            return Err(EndpointError::NotUserProcess);
        }
        
        // 验证：不能是特殊端点
        if slot_val >= SLOT_TOP - 3 {
            return Err(EndpointError::SpecialEndpoint);
        }
        
        // 验证：slot 在有效范围内
        let slot_usize = slot_val as usize;
        if slot_usize >= NR_PROCS {
            return Err(EndpointError::InvalidSlot);
        }
        
        Ok(Self {
            slot: UserSlot(slot_usize),
            generation: Generation(gen_val),
        })
    }
    
    pub fn slot(&self) -> UserSlot { self.slot }
    pub fn generation(&self) -> Generation { self.generation }
    
    /// 转换为内核表索引
    pub fn to_kernel_slot(&self) -> KernelSlot {
        KernelSlot(self.slot.0 + MAX_NR_TASKS)
    }
}
```

### 3.4 KernelTask（内核任务）

```rust
/// 内核任务枚举
/// 
/// 编译期保证：只能是预定义的内核任务。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelTask {
    Kernel,    // -1
    System,    // -2
    Clock,     // -3
    Idle,      // -4
    Asyncm,    // -5
}

impl KernelTask {
    pub fn encode(self) -> EndpointRaw {
        let slot = match self {
            Self::Kernel => -1,
            Self::System => -2,
            Self::Clock => -3,
            Self::Idle => -4,
            Self::Asyncm => -5,
        };
        unsafe { EndpointRaw::from_raw_unchecked(slot) }
    }
    
    pub fn decode(raw: EndpointRaw) -> Result<Self, EndpointError> {
        let slot = Self::extract_slot(raw);
        
        match slot {
            -1 => Ok(Self::Kernel),
            -2 => Ok(Self::System),
            -3 => Ok(Self::Clock),
            -4 => Ok(Self::Idle),
            -5 => Ok(Self::Asyncm),
            _ => Err(EndpointError::NotKernelTask),
        }
    }
    
    /// 转换为内核表索引
    pub fn to_kernel_slot(&self) -> KernelSlot {
        let idx = match self {
            Self::Kernel => MAX_NR_TASKS - 1,
            Self::System => MAX_NR_TASKS - 2,
            Self::Clock => MAX_NR_TASKS - 3,
            Self::Idle => MAX_NR_TASKS - 4,
            Self::Asyncm => MAX_NR_TASKS - 5,
        };
        KernelSlot(idx)
    }
}
```

### 3.5 SpecialEndpoint（特殊端点）

```rust
/// 特殊端点枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialEndpoint {
    Any,       // 31744
    None,      // 31743
    Self_,     // 31742
}

impl SpecialEndpoint {
    pub fn encode(self) -> EndpointRaw {
        let slot = match self {
            Self::Any => SLOT_TOP - 1,
            Self::None => SLOT_TOP - 2,
            Self::Self_ => SLOT_TOP - 3,
        };
        unsafe { EndpointRaw::from_raw_unchecked(slot) }
    }
    
    pub fn decode(raw: EndpointRaw) -> Result<Self, EndpointError> {
        let slot = Self::extract_slot(raw);
        
        match slot {
            s if s == SLOT_TOP - 1 => Ok(Self::Any),
            s if s == SLOT_TOP - 2 => Ok(Self::None),
            s if s == SLOT_TOP - 3 => Ok(Self::Self_),
            _ => Err(EndpointError::NotSpecialEndpoint),
        }
    }
}
```

---

## 4. 使用示例

### 4.1 IPC 场景（编译期安全）

```rust
// 接收消息
fn handle_ipc(raw_source: EndpointRaw, msg: Message) -> Result<(), Error> {
    let source = Endpoint::decode(raw_source)
        .map_err(|_| Error::InvalidEndpoint)?;
    
    match source {
        // 用户进程：保证有 vmproc
        Endpoint::Process(proc) => {
            let vmproc = vm_lookup(proc)
                .ok_or(Error::ProcessNotFound)?;
            handle_user_msg(vmproc, msg)?;
        }
        
        // 内核任务：直接处理
        Endpoint::Kernel(task) => match task {
            KernelTask::System => handle_system_msg(msg)?,
            KernelTask::Clock => handle_clock_msg(msg)?,
            _ => return Err(Error::UnexpectedKernelTask),
        }
        
        // 特殊端点
        Endpoint::Special(SpecialEndpoint::Any) => {
            handle_broadcast(msg)?;
        }
        Endpoint::Special(SpecialEndpoint::None) => {
            return Err(Error::InvalidSource);
        }
        _ => {}
    }
    
    Ok(())
}

// 发送消息（只能发送给进程，不能发给 ANY）
fn send_to_process(dest: ProcessEndpoint, msg: Message) -> Result<(), Error> {
    let raw = dest.encode();
    ipc_send(raw.into_inner(), msg)?;
    Ok(())
}
```

### 4.2 进程表操作（类型安全）

```rust
struct ProcessTable {
    slots: [Option<Process>; NR_PROCS],
}

impl ProcessTable {
    /// 通过端点查找进程
    /// 
    /// 参数类型保证：只能是 ProcessEndpoint，不可能是 ANY 或 Kernel
    fn find(&self, ep: ProcessEndpoint) -> Option<&Process> {
        let slot = ep.slot().0;
        let proc = self.slots.get(slot)?.as_ref()?;
        
        // 验证 generation
        if proc.generation != ep.generation() {
            return None;
        }
        
        Some(proc)
    }
    
    /// 分配新进程
    fn allocate(&mut self) -> Option<ProcessEndpoint> {
        let slot = self.find_free_slot()?;
        let generation = self.next_generation(slot);
        
        let proc = Process::new(slot, generation);
        self.slots[slot.0] = Some(proc);
        
        Some(ProcessEndpoint::new(slot, generation))
    }
}
```

### 4.3 Capability 化（自然延伸）

```rust
/// Capability：带权限的端点引用
pub struct Capability<T: KernelObject> {
    endpoint: ProcessEndpoint,
    rights: Rights,
    _marker: PhantomData<T>,
}

impl<T: KernelObject> Capability<T> {
    /// 发送消息（权限检查）
    pub fn send(&self, msg: Message) -> Result<(), Error> {
        if !self.rights.contains(Rights::SEND) {
            return Err(Error::PermissionDenied);
        }
        
        let raw = self.endpoint.encode();
        ipc_send(raw.into_inner(), msg)?;
        Ok(())
    }
}

// 使用
fn handle_request(cap: Capability<VmObject>) {
    // 编译期保证：cap 指向 VmObject，不是 Process，不是 Device
    // 运行时保证：cap 有 SEND 权限
}
```

---

## 5. 与 Rewrite 方案的对比

### 5.1 类型安全

| 场景 | Rewrite | Redesign |
|------|---------|----------|
| ANY 传给 vmproc | 运行时 panic | **编译期错误** |
| Kernel 进用户进程表 | 运行时检查 | **类型禁止** |
| 忘记 generation 检查 | 容易遗漏 | **decode 强制验证** |
| 非法 slot | 可能 | **不可能** |

### 5.2 代码清晰度

**Rewrite**：
```rust
fn vm_brk(ep: Endpoint) {
    if ep.is_any() { return Err(...); }  // 运行时检查
    if ep.is_kernel_task() { return Err(...); }  // 运行时检查
    let slot = ep.to_user_slot()?;  // 可能失败
    // ...
}
```

**Redesign**：
```rust
fn vm_brk(ep: ProcessEndpoint) {
    // 编译期保证：不是 ANY，不是 Kernel
    let slot = ep.slot();  // 直接访问，不会失败
    // ...
}
```

### 5.3 性能

| 操作 | Rewrite | Redesign | 差异 |
|------|---------|----------|------|
| 编码 | 直接计算 | match + 计算 | 无（inline） |
| 解码 | 直接计算 | match + 验证 | 多几次分支 |
| 类型检查 | 运行时 if | 编译期 | **零开销** |

---

## 6. 设计权衡

### 6.1 优势

1. **编译期安全**：非法状态不可表示
2. **零成本抽象**：类型检查在编译期完成
3. **Capability-ready**：自然延伸到权限系统
4. **自文档化**：类型即文档

### 6.2 劣势

1. **跨越 Rewrite 红线**：改变了 Minix3 的语义模型
2. **与 C 交互复杂**：需要显式转换层
3. **代码量增加**：需要更多类型定义
4. **学习成本**：开发者需要理解类型层次

### 6.3 适用场景

| 场景 | 推荐方案 |
|------|----------|
| 复刻 Minix3 | Rewrite（保持语义一致） |
| 长期维护/多人协作 | Redesign（类型安全） |
| Capability 系统 | Redesign（自然延伸） |
| 现代 OS 设计 | Redesign（最佳实践） |

---

## 7. 结论

这个设计方案展示了如何用 Rust 的类型系统来**超越** Minix3 的设计，而不是仅仅**复刻**它。

核心思想：

> **ABI 层保持兼容，语义层追求安全。**

这不是对 Minix3 的否定，而是展示了**现代类型系统**在 OS 设计中的可能性。

---

### 关键区别总结

| 维度 | Minix3 (C) | Rewrite (Rust) | Redesign (本方案) |
|------|-----------|----------------|-------------------|
| 目标 | 实现 | 复刻 | **超越** |
| 类型安全 | ❌ 无 | ⚠️ 部分 | ✅ **完整** |
| 编译期检查 | ❌ 无 | ⚠️ 部分 | ✅ **完整** |
| 运行时检查 | ✅ 多 | ✅ 多 | **少** |
| Capability | ❌ 难 | ⚠️ 难 | ✅ **自然** |
| 与 Minix3 兼容 | ✅ | ✅ | ⚠️ 需转换层 |

---

*参考：本设计基于与 GPT 的讨论，探索了 Rust 类型系统在 OS 设计中的极限。*
