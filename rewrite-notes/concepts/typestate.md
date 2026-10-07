# Typestate 模式

> 用类型系统编码状态，将"非法状态"从 runtime error 变成 compile-time error。

## 0. 为什么需要 Typestate

### 没有 Typestate 会发生什么？

- 所有操作都是 runtime check
- 状态机分散在代码各处
- 不合法操作只能在运行时报错
- API 使用者必须记住隐含规则（易错）

**具体例子：**

```rust
// 没有 typestate 的代码
struct VmProc {
    flags: VmFlags,
    endpoint: Endpoint,
}

fn do_fork(proc: &mut VmProc) {
    // 问题1：可以在任何状态下调用
    // 问题2：可以忘记设置某些字段
    // 问题3：状态转换没有约束
    proc.flags |= VmFlags::IN_USE;
    proc.endpoint = Endpoint::PM;
    // 忘记设置 total？编译器不会报错！
}

fn cleanup(proc: &mut VmProc) {
    // 问题：可以在 Active 状态下直接清理，跳过 Exiting 状态
    proc.flags = VmFlags::empty();
}
```

### 目标

- 将"非法状态"从 runtime error → compile-time error
- 将"隐含规则"变为类型系统约束
- 状态转换由编译器强制执行

## 1. 什么是 Typestate

Typestate 是一种将对象状态编码到类型系统中的设计模式。

**核心思想：**

- 每个状态对应一个类型
- 状态转换 = 类型转换
- 编译器在编译期验证状态转换的合法性

**传统例子：文件句柄**

```rust
// 状态编码到类型
struct OpenFile { fd: i32 }
struct ClosedFile;

impl OpenFile {
    fn close(self) -> ClosedFile {  // 消耗 self，返回新状态
        unsafe { close(self.fd); }
        ClosedFile
    }
    
    fn read(&self, buf: &mut [u8]) -> usize {
        // 只有 Open 状态才能 read
    }
}

// ClosedFile 没有 read 方法！
// 编译期保证：关闭的文件不能读取
```

**关键特性：**

1. **状态编码到类型系统** - 类型即状态
2. **编译期状态验证** - 非法操作无法编译
3. **状态转换由类型系统强制** - 必须按正确顺序转换

## 2. 核心公式（The Essence）

> **Typestate 的本质不是消除 runtime state，**
> **而是为 runtime state 提供 compile-time 的安全投影。**

```
Typestate View = Runtime State + Compile-time Projection
```

**等价解释（直觉版）：**

```
Typestate View = Checked Borrow
```

- **Borrow**：`&mut VmProc` - 借用数据
- **Checked**：通过 `as_active()` 做合法性验证

这个公式的含义：

| 组成部分 | 含义 | 实现方式 |
|----------|------|----------|
| Runtime State | 运行时的真实状态 | `VmFlags` 字段 |
| Compile-time Projection | 编译期的类型投影 | `ActiveProc<'a>` 类型 |
| View | 状态的"视图"或"窗口" | 借用 + 约束 |

对应实现：

```
VmFlags (runtime truth)
    ↓
as_active() (runtime check)
    ↓
ActiveProc<'a> (compile-time guarantee)
```

**为什么需要两层？**

1. **Runtime State 必须存在** - 因为数据存储需要状态
2. **Compile-time Projection 提供安全** - 编译期保证操作合法性
3. **View 不拥有数据** - 适合内核场景（地址稳定、大数据）
4. **状态可能被外部系统修改** - 如 RS、调度器、IPC，编译期无法完全掌控状态，必须有 runtime source of truth

**对比传统 typestate：**

| 方式 | 状态存储 | 类型约束 |
|------|----------|----------|
| 传统 | 类型参数 | 类型参数 |
| View | 数据字段 | 借用类型 |

## 3. 传统实现 vs Typestate View

### 3.1 传统 Typestate（所有权转移）

每个状态一个类型，状态转换消耗旧类型，返回新类型。

```rust
// 传统 typestate 实现
struct Slot<State> {
    index: usize,
    _state: PhantomData<State>,
}

struct Empty;
struct Active;
struct Exiting;

impl Slot<Empty> {
    fn init(self, proc: VmProc) -> Slot<Active> {
        // 初始化槽位
        Slot { index: self.index, _state: PhantomData }
    }
}

impl Slot<Active> {
    fn mark_exiting(self) -> Slot<Exiting> {
        // 标记退出
        Slot { index: self.index, _state: PhantomData }
    }
}

impl Slot<Exiting> {
    fn cleanup(self) -> Slot<Empty> {
        // 清理资源
        Slot { index: self.index, _state: PhantomData }
    }
}
```

**优点：**
- 编译期保证非常强
- 状态转换清晰明确
- 不可能忘记状态

**缺点：**
- 需要 move 或 replace 数据
- 对于大数据结构开销大
- 对于需要地址稳定的场景不适用

### 3.2 Typestate View（借用约束）

不拥有数据，只借用 + 约束。状态存储在数据本身。

```rust
// Typestate View 实现
pub struct VmProc {
    slot: UserSlot,
    flags: VmFlags,  // 状态存储在这里
    // ... 其他字段
}

pub struct ActiveProc<'a> {
    inner: &'a mut VmProc,  // 借用，不拥有
}

pub struct ExitingProc<'a> {
    inner: &'a mut VmProc,
}

impl<'a> ActiveProc<'a> {
    pub fn mark_exiting(self) -> ExitingProc<'a> {
        self.inner.flags.insert(VmFlags::EXITING);  // in-place 修改
        ExitingProc { inner: self.inner }
    }
}
```

**优点：**
- 无需 move 数据
- 地址稳定
- 零额外内存开销
- 适合内核场景

**缺点：**
- 需要运行时入口检查
- 生命周期管理更复杂

## 4. 对比分析

| 特性 | 传统 Typestate | Typestate View |
|------|----------------|----------------|
| 数据所有权 | 转移 | 借用 |
| 状态存储 | 类型参数 | 数据字段 |
| 内存开销 | 可能需要 replace | 无额外开销 |
| 地址稳定性 | 可能改变 | 保持不变 |
| 编译期保证 | 100% | 入口后 100% |
| 适用场景 | 小数据、简单状态 | 大数据、内核场景 |

**选择依据：**

1. **数据大小** - 小数据用传统，大数据用 View
2. **地址稳定性** - 需要稳定地址必须用 View
3. **状态复杂度** - 简单状态用传统，复杂状态用 View
4. **性能要求** - View 零开销，传统可能有 replace 开销

## 5. Minix-rs 中的应用

### 5.1 VmProc 状态管理

```rust
// 数据结构
pub struct VmProc {
    pub slot: UserSlot,
    pub flags: VmFlags,      // 状态存储
    pub endpoint: Endpoint,
    // ... 其他字段
}

// 状态标志
bitflags! {
    pub struct VmFlags: u32 {
        const IN_USE = 1;      // 槽位已初始化
        const EXITING = 2;     // 进程正在退出
        const VM_INSTANCE = 4; // VM 服务实例
    }
}

// Typestate View
pub struct ActiveProc<'a> {
    inner: &'a mut VmProc,  // 私有，capability 模式
}

pub struct ExitingProc<'a> {
    inner: &'a mut VmProc,
}
```

**职责分离：**

| 组件 | 职责 |
|------|------|
| `VmFlags` | 存储运行时状态 |
| `ActiveProc<'a>` | 提供活跃状态的编译期约束 |
| `ExitingProc<'a>` | 提供退出状态的编译期约束 |
| `VmProcTable` | 管理状态转换入口 |

### 5.2 状态转换流程

```
┌─────────────────────────────────────────────────────────────┐
│                      VmProcTable                             │
│                                                              │
│  as_active(slot)                                             │
│      │                                                       │
│      │ runtime check: IN_USE && !EXITING                    │
│      ▼                                                       │
│  ┌─────────────────┐                                         │
│  │  ActiveProc<'a> │  ← 编译期保证：只能 mark_exiting()      │
│  └────────┬────────┘                                         │
│           │ mark_exiting()                                   │
│           │ in-place: flags |= EXITING                       │
│           ▼                                                  │
│  ┌─────────────────┐                                         │
│  │ ExitingProc<'a> │  ← 编译期保证：只能 cleanup()           │
│  └────────┬────────┘                                         │
│           │ cleanup()                                        │
│           │ unsafe: clear()                                  │
│           ▼                                                  │
│  ┌─────────────────┐                                         │
│  │   未使用状态     │  flags = empty()                       │
│  └─────────────────┘                                         │
└─────────────────────────────────────────────────────────────┘
```

**❌ 非法路径（被类型系统阻止）：**

```
ActiveProc → cleanup()         // 方法不存在，编译错误
ExitingProc → mark_exiting()   // 方法不存在，编译错误
ActiveProc → ActiveProc        // 不能重复获取（借用检查）
```

**编译期保证：**

```rust
// ✅ 合法：正确的状态转换顺序
let active = table.as_active(slot)?;
let exiting = active.mark_exiting();
unsafe { exiting.cleanup(); }

// ❌ 非法：ActiveProc 没有 cleanup 方法
let active = table.as_active(slot)?;
active.cleanup();  // 编译错误！

// ❌ 非法：ExitingProc 没有 mark_exiting 方法
let exiting = ...;
exiting.mark_exiting();  // 编译错误！

// ❌ 非法：不能重复获取 ActiveProc
let active1 = table.as_active(slot)?;
let active2 = table.as_active(slot)?;  // 借用检查器会报错
```

## 6. 适用场景

Typestate View 适用于：

| 场景 | 原因 |
|------|------|
| 状态复杂但数据大 | 无法 move，需要借用 |
| 内核 / 长生命周期对象 | 地址必须稳定 |
| 需要地址稳定的场景 | 静态分配或固定位置 |
| 多模块协作的状态管理 | 通过 capability 控制访问 |
| 需要防止 API 被误用 | typestate 可以消除"调用顺序错误" |

**不适合的场景：**

| 场景 | 建议替代方案 |
|------|-------------|
| 小数据、简单状态 | 传统 typestate |
| 状态很少变化 | 简单的 enum |
| 不需要编译期保证 | runtime check |

## 7. 设计原则

### 7.1 状态存储与状态约束分离

```
状态存储: VmFlags (runtime)
状态约束: ActiveProc<'a> (compile-time)
```

**为什么分离？**

1. 运行时需要知道真实状态（调度、调试）
2. 编译期需要约束操作（安全）
3. 分离后可以独立演化

### 7.2 运行时检查入口，编译期保证内部

```rust
// 入口：运行时检查
pub fn as_active(&mut self, slot: UserSlot) -> Option<ActiveProc<'_>> {
    let proc = self.get_proc_mut(slot)?;
    if proc.flags.contains(IN_USE) && !proc.flags.contains(EXITING) {
        Some(ActiveProc::new(proc))  // 通过检查后进入编译期保证
    } else {
        None
    }
}

// 内部：编译期保证
impl<'a> ActiveProc<'a> {
    // 只能调用这些方法，不能做其他事
    pub fn mark_exiting(self) -> ExitingProc<'a> { ... }
    pub fn set_endpoint(&mut self, ep: Endpoint) { ... }
}
```

### 7.3 In-place 状态迁移

```rust
// 不需要 move，不需要 replace
pub fn mark_exiting(self) -> ExitingProc<'a> {
    self.inner.flags.insert(VmFlags::EXITING);  // 直接修改
    ExitingProc { inner: self.inner }           // 返回新 view
}
```

**好处：**

1. 零内存开销
2. 地址不变
3. 无数据复制

### 7.4 总结

> **Typestate View 的核心不是"用类型表示状态"，**
> **而是：用类型限制"在某个已验证状态下，你能做什么"**
>
> 它不消除 runtime state，
> 而是将"状态是否合法"这个问题前移到类型系统中。

**核心洞察：**

```
Runtime = truth（运行时状态是真相）
Type system = constraint layer（类型系统是约束层）
```

> 类型系统不是用来"存状态"的，而是用来"限制状态使用方式"的。

## 8. 常见误区

### 8.1 试图完全消除 runtime state ❌

```rust
// 错误：把所有状态都放到类型中
struct Proc<State> { ... }

// 问题：无法表达动态变化（内核/并发场景）
```

**正确做法：** Runtime state + Compile-time projection

### 8.2 滥用 typestate ❌

```rust
// 错误：所有 struct 都搞 typestate
struct Config<State> { ... }
struct Logger<State> { ... }

// 问题：复杂度爆炸，收益有限
```

**正确做法：** 只在状态复杂、误用代价高的场景使用

### 8.3 绕过入口函数 ❌

```rust
// 错误：直接访问底层数据
let proc = table.get_proc_mut(slot)?;
proc.flags = VmFlags::IN_USE;  // 绕过了 as_active() 的检查

// 问题：破坏编译期保证
```

**正确做法：** 始终通过 `as_active()` / `as_exiting()` 获取 view

### 8.4 忘记生命周期约束 ❌

```rust
// 错误：试图保存 ActiveProc
let active = table.as_active(slot)?;
some_list.push(active);  // 生命周期问题

// 问题：view 是临时借用，不能长期持有
```

**正确做法：** 在作用域内完成操作，不要跨作用域持有 view

## 9. 参考资料

- [Typestate Pattern in Rust](https://cliffle.com/blog/rust-typestate/)
- [Rust API Patterns: Typestate](https://rust-lang.github.io/api-guidelines/typestate.html)
- [Minix3 VM Source](minix3/minix/servers/vm/)
