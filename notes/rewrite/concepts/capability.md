# Capability 模式

> 控制访问权限，将"隐含规则"变为类型系统约束。

## 0. 为什么需要 Capability

### 没有 Capability 会发生什么？

- 任意代码可以修改 VmProc 内部状态
- 状态转换没有约束（Active → Exiting → Active?）
- API 使用者必须记住隐含规则（易错）
- 可以忘记设置必要字段
- 可以在错误状态下执行操作
- 可以部分初始化（中间状态暴露）

**具体例子：**

```rust
// 没有 capability 的代码
struct VmProc {
    flags: VmFlags,      // 公开字段
    endpoint: Endpoint,  // 公开字段
    total: VirBytes,     // 公开字段
}

fn do_fork(table: &mut VmProcTable, slot: UserSlot) {
    let child = table.get_proc_mut(slot).unwrap();
    
    // 问题1：可以忘记设置某些字段
    child.flags = VmFlags::IN_USE;
    child.endpoint = new_endpoint;
    // 忘记设置 total？编译器不会报错！
    
    // 问题2：可以在任何状态下修改
    child.flags = VmFlags::empty();  // 直接清空，跳过退出流程
    
    // 问题3：可以设置不一致的状态
    child.flags = VmFlags::IN_USE | VmFlags::EXITING;  // 同时设置？
}

fn cleanup(table: &mut VmProcTable, slot: UserSlot) {
    let proc = table.get_proc_mut(slot).unwrap();
    // 问题：可以在 Active 状态下直接清理
    proc.flags = VmFlags::empty();
    proc.endpoint = Endpoint::NONE;
    // 跳过了 Exiting 状态！
}
```

### 目标

- 将"隐含规则"变为类型系统约束
- 强制单一入口获取访问权限
- 封装操作逻辑，防止误用

## 1. 什么是 Capability（The Essence）

> **Capability 的本质不是"给你数据"，**
> **而是"只给你被允许的操作入口"。**

```
Capability = Data + Access Restriction
```

**等价解释：**

```
Capability = Restricted Access to Data
```

- **Data**：`VmProc` - 原始数据
- **Restricted Access**：`ActiveProc` - 受限访问

对应实现：

```
VmProc (data)
    ↓ private
ActiveProc (restricted access)
```

Capability 是一种访问权限的抽象模式。

**核心思想：**

- "持有即有权访问"（Possession implies authority）
- 通过类型系统控制访问路径
- 将权限编码到类型中

**传统例子：文件描述符**

```rust
// Unix 中的 capability：文件描述符
// 持有 fd = 有权访问文件

struct FileDescriptor(i32);

impl FileDescriptor {
    // 只有通过 open 才能获得 fd
    fn open(path: &str) -> Option<Self> {
        let fd = unsafe { libc::open(path.as_ptr(), 0) };
        if fd >= 0 { Some(Self(fd)) } else { None }
    }
    
    // 持有 fd 才能读
    fn read(&self, buf: &mut [u8]) -> usize {
        unsafe { libc::read(self.0, buf.as_ptr(), buf.len()) }
    }
}
```

**关键特性：**

1. **不可伪造** - 只能通过特定入口获取
2. **权限绑定** - 类型即权限
3. **访问控制** - 通过类型系统强制执行

## 2. Capability 与 Typestate 的关系

```
Capability:
    控制"你能做什么"

Typestate:
    控制"你在什么状态下能做什么"

Typestate View:
    = 带状态约束的 Capability
```

> **Typestate View 本质上是 Capability 的一种特化**

**更抽象的理解：**

```
Capability 解决的是 "空间上的权限问题"（谁能访问）
Typestate 解决的是 "时间上的状态问题"（何时能访问）

Typestate View = 在时间维度受限的 Capability
```

**三者的关系：**

```
┌─────────────────────────────────────────────────────────┐
│                      Capability                          │
│  "你能做什么"（空间维度）                                  │
│                                                          │
│  ┌─────────────────────────────────────────────────┐    │
│  │              Typestate                           │    │
│  │  "你在什么状态下能做什么"（时间维度）               │    │
│  │                                                  │    │
│  │  ┌─────────────────────────────────────────┐    │    │
│  │  │         Typestate View                   │    │    │
│  │  │  = Capability + State Constraint         │    │    │
│  │  │  = 在时间维度受限的 Capability           │    │    │
│  │  └─────────────────────────────────────────┘    │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```

**对比：**

| 模式 | 关注点 | 约束维度 |
|------|--------|----------|
| Capability | 访问权限 | 空间：谁能访问 |
| Typestate | 状态转换 | 时间：何时能访问 |
| Typestate View | 权限 + 状态 | 空间 + 时间 |

## 3. Access Control Pyramid

```
         ┌─────────────────────────────┐
         │   Level 3: API Surface      │
         │   - high-level operations   │
         │   - e.g. fork, exit         │
         │   - 最小权限，最高语义        │
         └─────────────────────────────┘
                    ▲
                    │
         ┌─────────────────────────────┐
         │   Level 2: Capability       │
         │   - ActiveProc / ExitingProc│
         │   - restricts operations    │
         │   - 中等权限，中等语义        │
         └─────────────────────────────┘
                    ▲
                    │
         ┌─────────────────────────────┐
         │   Level 1: Raw Data         │
         │   - VmProc                  │
         │   - private fields          │
         │   - 最大权限，最低语义        │
         └─────────────────────────────┘
```

> **越往上，语义越高，权限越收敛**

> **系统的安全性取决于：有多少代码停留在 Level 1**
>
> Level 1 代码越多，风险越大。

### 3.1 Level 1: Raw Data

```rust
pub struct VmProc {
    pub slot: UserSlot,
    flags: VmFlags,      // 私有字段
    endpoint: Endpoint,  // 私有字段
    // ...
}
```

**特点：**
- 最大权限：可以直接访问所有字段（模块内部）
- 最低语义：没有操作约束
- 风险最高：容易误用

### 3.2 Level 2: Capability

```rust
pub struct ActiveProc<'a> {
    inner: &'a mut VmProc,  // 私有，外部无法访问
}

impl<'a> ActiveProc<'a> {
    pub fn set_endpoint(&mut self, ep: Endpoint) {
        self.inner.endpoint = ep;  // 通过方法访问
    }
}
```

**特点：**
- 中等权限：只能调用提供的方法
- 中等语义：操作有约束
- 风险降低：封装了操作逻辑

### 3.3 Level 3: API Surface

```rust
impl VmProcTable {
    pub fn handle_fork(&mut self, request: &VmForkRequest) -> Result<VmForkResponse, VmForkError> {
        // 高层操作，用户无需关心细节
        let mut child = self.as_active(request.child_slot)?;
        child.init_from_fork(endpoint, total, total_max);
        // ...
    }
}
```

**特点：**
- 最小权限：只能执行高层操作
- 最高语义：业务语义清晰
- 风险最低：完全封装

## 4. Rust 中的 Capability 实现

### 4.1 私有字段 + 公开方法

最基本的 capability 实现方式。

```rust
// 私有字段，外部无法直接访问
pub struct ActiveProc<'a> {
    inner: &'a mut VmProc,  // 私有
}

impl<'a> ActiveProc<'a> {
    // 只能通过方法访问
    pub fn endpoint(&self) -> Endpoint {
        self.inner.endpoint
    }
    
    pub fn set_endpoint(&mut self, ep: Endpoint) {
        self.inner.endpoint = ep;
    }
}
```

**效果：**

```rust
let mut active = table.as_active(slot)?;

// ✅ 可以：通过方法访问
let ep = active.endpoint();
active.set_endpoint(new_ep);

// ❌ 不可以：直接访问字段
active.inner.endpoint = ep;  // 编译错误：字段私有
```

### 4.2 构造函数控制

通过构造函数控制 capability 的获取路径。

```rust
pub struct ActiveProc<'a> {
    inner: &'a mut VmProc,
}

impl<'a> ActiveProc<'a> {
    // pub(crate)：只有 crate 内部可以调用
    pub(crate) fn new(inner: &'a mut VmProc) -> Self {
        Self { inner }
    }
}

// VmProcTable 是唯一入口
impl VmProcTable {
    pub fn as_active(&mut self, slot: UserSlot) -> Option<ActiveProc<'_>> {
        let proc = self.get_proc_mut(slot)?;
        if proc.flags.contains(IN_USE) && !proc.flags.contains(EXITING) {
            Some(ActiveProc::new(proc))  // 唯一的构造路径
        } else {
            None
        }
    }
}
```

**效果：**

```rust
// ✅ 可以：通过入口获取
let active = table.as_active(slot)?;

// ❌ 不可以：直接构造
let active = ActiveProc::new(&mut proc);  // 编译错误：构造函数私有

// ❌ 不可以：绕过检查
let active = ActiveProc { inner: proc };  // 编译错误：字段私有
```

## 5. Minix-rs 中的应用

### 5.1 ActiveProc 的 Capability 设计

```rust
// vmproc_handle.rs

pub struct ActiveProc<'a> {
    inner: &'a mut VmProc,  // 私有字段
}

impl<'a> ActiveProc<'a> {
    // 构造函数：pub(crate)
    pub(crate) fn new(inner: &'a mut VmProc) -> Self {
        Self { inner }
    }
    
    // 只读代理
    pub fn slot(&self) -> UserSlot { self.inner.slot }
    pub fn endpoint(&self) -> Endpoint { self.inner.endpoint }
    pub fn flags(&self) -> VmFlags { self.inner.flags }
    
    // 写操作代理
    pub fn set_endpoint(&mut self, ep: Endpoint) {
        self.inner.endpoint = ep;
    }
    
    // 状态转换
    pub fn mark_exiting(self) -> ExitingProc<'a> {
        self.inner.flags.insert(VmFlags::EXITING);
        ExitingProc::new(self.inner)
    }
    
    // 复合操作
    pub fn init_from_fork(&mut self, ep: Endpoint, total: VirBytes, max: VirBytes) {
        self.inner.flags = VmFlags::IN_USE;
        self.inner.endpoint = ep;
        self.inner.total = total;
        self.inner.total_max = max;
    }
}
```

**设计要点：**

| 要点 | 实现 |
|------|------|
| 字段私有 | `inner` 不可外部访问 |
| 构造受限 | `pub(crate) fn new()` |
| 操作代理 | 所有访问通过方法 |
| 复合封装 | `init_from_fork()` 封装多步操作 |

### 5.2 为什么不用 `inner()` / `inner_mut()`

**有 `inner()` 的问题：**

```rust
// 如果有 inner_mut()
impl<'a> ActiveProc<'a> {
    pub fn inner_mut(&mut self) -> &mut VmProc {
        self.inner
    }
}

// 用户可以这样绕过约束
let mut active = table.as_active(slot)?;
let proc = active.inner_mut();  // 获取原始引用

// 问题1：可以任意修改
proc.flags = VmFlags::empty();  // 清空所有标志

// 问题2：可以设置不一致状态
proc.flags = VmFlags::IN_USE | VmFlags::EXITING;  // 同时设置？

// 问题3：可以跳过状态转换
// 不需要 mark_exiting()，直接修改
```

**没有 `inner()` 的保证：**

```rust
let mut active = table.as_active(slot)?;

// ✅ 只能通过代理方法
active.set_endpoint(ep);
active.init_from_fork(endpoint, total, max);

// ✅ 状态转换受控
let exiting = active.mark_exiting();  // 唯一的退出路径

// ❌ 无法绕过
active.inner_mut();  // 方法不存在
```

**核心原则：**

> **Capability 的价值在于"限制"，而不是"便利"**
>
> 提供 `inner_mut()` 虽然方便，但破坏了整个 capability 的意义。

### 5.3 Fork 操作的 Capability 使用

**之前的问题（绕过 capability）：**

```rust
fn do_fork(&mut self) -> Result<Endpoint, VmForkError> {
    // 直接访问底层数据
    let child = self.table.get_proc_mut(slot)?;
    
    // 问题1：可以忘记设置
    child.flags &= VmFlags::IN_USE;
    child.endpoint = child_endpoint;
    // 忘记设置 total？
    
    // 问题2：可以在错误状态下操作
    // 如果 child 已经是 EXITING 状态？
    
    // 问题3：部分初始化
    // 中间状态可能被其他代码看到
}
```

**之后的保证（使用 capability）：**

```rust
fn do_fork(&mut self) -> Result<Endpoint, VmForkError> {
    // 通过 capability 获取访问权限
    let mut child = self.table.as_active(slot)?;
    
    // 原子操作，封装所有初始化逻辑
    child.init_from_fork(endpoint, total, total_max);
    
    // 不存在"半初始化状态"
    // 编译期保证操作合法性
}
```

**对比：**

| 之前 | 之后 |
|------|------|
| 可以忘记设置 IN_USE | 必须先获取 ActiveProc |
| 可以在 EXITING 状态下初始化 | `as_active()` 会返回 None |
| 可以部分初始化 | `init_from_fork()` 原子操作 |
| 状态不一致风险 | 编译期保证 |
| 中间状态暴露 | 封装在方法内 |

**终极意义：**

> **fork 的语义从"步骤序列"变成了"一个原子操作"**
>
> 不再是：设置 flags → 设置 endpoint → 设置 total → ...
> 而是：`init_from_fork()` 一步完成

## 6. 适用场景

Capability 适用于：

| 场景 | 原因 |
|------|------|
| 需要严格访问控制的系统 | OS, DB, runtime 等 |
| 多模块协作 | 防止模块间误用 |
| 防止误用 API | 编译期强制正确使用 |
| 需要封装复杂操作逻辑 | 复合操作原子化 |
| 安全敏感代码 | 权限边界清晰 |

**不适合的场景：**

| 场景 | 建议替代方案 |
|------|-------------|
| 简单数据结构 | 直接公开字段 |
| 内部实现细节 | 不需要 capability |
| 极端性能敏感且已验证安全的路径 | 可考虑绕过（需严格审计） |

> **注意：** Rust 内联后代理方法几乎零成本，通常不是性能瓶颈。

## 7. 设计原则

### 7.1 最小权限原则

只授予必要的权限，不多给。

```rust
// ❌ 错误：给太多权限
pub fn inner_mut(&mut self) -> &mut VmProc { ... }

// ✅ 正确：只给需要的操作
pub fn set_endpoint(&mut self, ep: Endpoint) { ... }
pub fn add_total(&mut self, v: VirBytes) { ... }
```

### 7.2 单一入口原则

访问权限只能通过一个入口获取。

```rust
// ✅ 正确：唯一入口
impl VmProcTable {
    pub fn as_active(&mut self, slot: UserSlot) -> Option<ActiveProc<'_>> { ... }
}

// ❌ 错误：多个入口
impl VmProcTable {
    pub fn as_active(&mut self, slot: UserSlot) -> Option<ActiveProc<'_>> { ... }
    pub fn get_active_unchecked(&mut self, slot: UserSlot) -> ActiveProc<'_> { ... }
}
```

### 7.3 编译期强制原则

约束应该在编译期执行，而不是运行时。

```rust
// ✅ 正确：编译期强制
let active = table.as_active(slot)?;
active.cleanup();  // 编译错误：方法不存在

// ❌ 错误：运行时检查
let active = table.as_active(slot)?;
if active.can_cleanup() {  // 运行时判断
    active.cleanup();
}
```

### 7.4 设计哲学

> **好的 API 不是让事情"更容易做"，**
> **而是让错误"更难发生"。**

Capability 的价值在于"限制"，而不是"便利"。

提供 `inner_mut()` 虽然方便，但破坏了整个 capability 的意义。

### 7.5 总结

> **Capability 的核心不是"提供访问"，而是"限制访问"**
>
> 它将"谁能做什么"从隐含规则变成类型约束，
> 让编译器成为你的安全守卫。

**核心洞察：**

```
Capability = Token of Authority（权限令牌）
```

> 持有 capability = 持有权限，类型系统保证你无法伪造或绕过。

**更深层理解：**

```
Raw data 是不安全的，
所有访问都必须经过 capability。
```

> 这就是 microkernel / capability system 的核心哲学。

## 8. 与其他模式的对比

### 8.1 Capability vs Builder Pattern

| 特性 | Capability | Builder |
|------|------------|---------|
| 目的 | 访问控制 | 对象构建 |
| 关注点 | 权限 | 配置 |
| 生命周期 | 长期 | 临时 |
| 典型方法 | `set_endpoint()` | `with_endpoint()` |

```rust
// Builder：构建对象
let proc = VmProcBuilder::new()
    .endpoint(ep)
    .flags(flags)
    .build();

// Capability：控制访问
let mut active = table.as_active(slot)?;
active.set_endpoint(ep);
```

### 8.2 Capability vs RAII

| 特性 | Capability | RAII |
|------|------------|------|
| 目的 | 访问控制 | 资源管理 |
| 关注点 | 权限 | 生命周期 |
| 获取方式 | 显式请求 | 构造时获取 |
| 释放方式 | 作用域结束 | Drop |

```rust
// RAII：资源管理
{
    let file = File::open("test.txt")?;  // 获取资源
    // 使用 file
}  // 自动释放

// Capability：访问控制
{
    let active = table.as_active(slot)?;  // 获取权限
    active.set_endpoint(ep);
}  // 权限释放，但数据还在
```

**可以结合使用：**

```rust
// Capability + RAII
pub struct ActiveProc<'a> {
    inner: &'a mut VmProc,
}

impl<'a> Drop for ActiveProc<'a> {
    fn drop(&mut self) {
        // 可以在这里做清理或检查
    }
}
```

### 8.3 Capability vs Encapsulation

很多人会混淆 Capability 和 Encapsulation。

| 特性 | Encapsulation | Capability |
|------|---------------|------------|
| 目的 | 隐藏实现细节 | 限制谁能做什么 |
| 关注点 | "怎么做" | "谁可以做" |
| 安全性 | 不保证安全 | 带约束的封装 |

```rust
// Encapsulation：隐藏细节
pub struct VmProc {
    flags: VmFlags,  // 私有，但只是隐藏
}

impl VmProc {
    pub fn set_flags(&mut self, f: VmFlags) {
        self.flags = f;  // 任何人都可以调用
    }
}

// Capability：限制访问
pub struct ActiveProc<'a> {
    inner: &'a mut VmProc,
}

impl<'a> ActiveProc<'a> {
    pub fn set_flags(&mut self, f: VmFlags) {
        self.inner.flags = f;  // 只有持有 ActiveProc 才能调用
    }
}
```

**关键区别：**

```
Encapsulation ≠ 安全
Capability = 带约束的封装
```

> Encapsulation 只是隐藏，Capability 才是安全边界。

## 9. 常见误区

### 9.1 提供 `inner()` 方法 ❌

```rust
// 错误：暴露内部引用
impl<'a> ActiveProc<'a> {
    pub fn inner(&self) -> &VmProc { self.inner }
    pub fn inner_mut(&mut self) -> &mut VmProc { self.inner }
}

// 问题：完全破坏了 capability 的意义
```

**正确做法：** 只提供必要的代理方法

### 9.2 多个入口 ❌

```rust
// 错误：多个获取 capability 的方式
impl VmProcTable {
    pub fn as_active(&mut self, slot: UserSlot) -> Option<ActiveProc<'_>> { ... }
    pub fn get_active_unchecked(&mut self, slot: UserSlot) -> ActiveProc<'_> { ... }
    pub fn force_active(&mut self, slot: UserSlot) -> ActiveProc<'_> { ... }
}

// 问题：用户可能选择"方便"但危险的入口
```

**正确做法：** 保持单一入口

### 9.3 Capability 太重 ❌

```rust
// 错误：每个字段都搞 capability
pub struct EndpointProc<'a> { inner: &'a mut VmProc }
pub struct FlagsProc<'a> { inner: &'a mut VmProc }
pub struct TotalProc<'a> { inner: &'a mut VmProc }

// 问题：复杂度爆炸
```

**正确做法：** 只在真正需要访问控制的场景使用

### 9.4 忘记生命周期约束 ❌

```rust
// 错误：试图长期持有 capability
struct Processor {
    active: Option<ActiveProc<'static>>,  // 错误！
}

// 问题：capability 是临时借用，不能长期持有
```

**正确做法：** 在作用域内使用，不跨作用域持有

## 10. 参考资料

- [Capability-based Security](https://en.wikipedia.org/wiki/Capability-based_security)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Minix3 VM Source](minix3/minix/servers/vm/)
