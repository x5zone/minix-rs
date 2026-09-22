# Fork 系统调用实现文档 — Part 5：VM Fork 调用（Mock 阶段）

> **范围**: 阶段 5 — VM Fork 调用 Mock 实现
> **前置**: 完成 [fork-rewr-04.md](fork-rewr-04.md) 中的进程结构复制
> **参考**: [fork-syscall-plan-part2.md](fork-syscall-plan-part2.md) 第五阶段

---

## 一、Minix3 源码分析

### 1.1 调用点

**文件**: `minix3/minix/servers/pm/forkexit.c` — `do_fork()` 第 82 行

```c
if((s=vm_fork(rmp->mp_endpoint, next_child, &child_ep)) != OK) {
    return s;
}
```

### 1.2 参数说明

| 参数 | 类型 | 说明 |
|------|------|------|
| `rmp->mp_endpoint` | `endpoint_t` | 父进程的 endpoint |
| `next_child` | `int` | 子进程的槽位索引 |
| `&child_ep` | `endpoint_t *` | 输出参数，VM 返回的子进程 endpoint |

### 1.3 VM 内部行为

**文件**: `minix3/minix/servers/vm/fork.c`

```c
int vm_fork(endpoint_t parent_ep, int child_pid, endpoint_t *child_ep) {
    struct vmproc *vmp;
    int r, slot;
    
    // 1. 查找父进程
    if ((r = vm_proc_lookup(parent_ep, &vmp)) != OK)
        return r;
    
    // 2. 分配子进程槽位
    slot = child_pid;  // PM 已经分配好了槽位
    if (slot < 0 || slot >= NR_PROCS)
        return EINVAL;
    
    // 3. 初始化子进程 vmproc
    vmproc[slot].vm_endpoint = _ENDPOINT(0, slot);
    vmproc[slot].vm_flags = vmp->vm_flags;
    // ... 复制地址空间（Copy-on-Write）
    
    // 4. 调用内核创建进程
    if ((r = sys_fork(parent_ep, vmproc[slot].vm_endpoint)) != OK)
        return r;
    
    // 5. 返回子进程 endpoint
    *child_ep = vmproc[slot].vm_endpoint;
    return OK;
}
```

### 1.4 关键约束

```c
/* PM may not fail fork after call to vm_fork(), as VM calls sys_fork(). */
```

**含义**：PM 调用 `vm_fork()` 后不能再失败，因为 VM 已经调用了 `sys_fork()` 创建了内核进程。

**后果**：如果 PM 在 `vm_fork()` 后失败，会导致孤儿进程。

### 1.5 Endpoint 生成规则

**文件**: `minix3/include/minix/sysutil.h`

```c
#define _ENDPOINT(g, p) (((g) << 16) | (p))
#define _ENDPOINT_G(e)   ((e) >> 16)
#define _ENDPOINT_P(e)   ((e) & 0xFFFF)
```

| 组成部分 | 位数 | 说明 |
|----------|------|------|
| generation | 高 16 位 | 代数，每次槽位重用递增 |
| slot | 低 16 位 | 进程槽位索引 |

**示例**：
- `endpoint = 0x00010005` → generation=1, slot=5
- `endpoint = 0x00020005` → generation=2, slot=5（同一个槽位被重用）

---

## 二、当前 Rust 实现分析

### 2.1 现有代码

**文件**: `os/servers/pm/src/mproc/table.rs`

```rust
impl ProcTable {
    /// 计算子进程的 endpoint
    ///
    /// 对应 Minix3 的 `_ENDPOINT(0, slot)`
    pub fn calculate_endpoint(slot: usize) -> Endpoint {
        Endpoint::new(slot as i32)
    }
}
```

### 2.2 问题清单

| # | 问题 | 说明 |
|---|------|------|
| 1 | 缺少 generation 管理 | endpoint 的 generation 部分始终为 0 |
| 2 | 缺少 vm_fork 函数 | 没有模拟 VM 的 fork 行为 |
| 3 | 缺少错误处理 | 没有处理 VM 失败的情况 |
| 4 | 缺少约束检查 | 没有实现 "PM 在 vm_fork 后不能失败" 的约束 |

---

## 三、实现方案对比

### 方案一：简单 Mock（推荐）

#### 设计思路

只模拟 VM 的核心行为：生成新的 endpoint。不实现真正的地址空间复制。

#### 代码实现

```rust
/// VM fork 结果
#[derive(Debug, Clone, Copy)]
pub struct VmForkResult {
    /// 子进程的 endpoint（由 VM/Kernel 生成）
    pub child_endpoint: Endpoint,
}

/// VM fork 错误
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmForkError {
    /// 无效的父进程 endpoint
    InvalidParent,
    /// 无效的子进程槽位
    InvalidSlot,
    /// 内核 fork 失败
    KernelError,
}

/// VM fork mock
///
/// 模拟 VM 的核心行为：
/// 1. 生成新的 endpoint
/// 2. 返回给 PM
///
/// # Mock 行为
/// - 不实现真正的地址空间复制
/// - 不调用内核的 sys_fork
/// - 只生成正确的 endpoint 格式
pub fn vm_fork_mock(parent_endpoint: Endpoint, child_index: usize) -> Result<VmForkResult, VmForkError> {
    if child_index >= NR_PROCS {
        return Err(VmForkError::InvalidSlot);
    }
    
    let child_endpoint = calculate_endpoint(child_index);
    
    Ok(VmForkResult { child_endpoint })
}

/// 计算子进程的 endpoint
///
/// 对应 Minix3 的 `_ENDPOINT(generation, slot)`
///
/// # 当前实现
/// - generation 固定为 0（简化版）
/// - 后续可扩展为动态 generation 管理
fn calculate_endpoint(slot: usize) -> Endpoint {
    Endpoint::new(slot as i32)
}
```

#### 优缺点分析

| 优点 | 缺点 |
|------|------|
| ✅ 实现简单 | ❌ 不模拟真实 VM 行为 |
| ✅ 足够用于 PM 测试 | ❌ 不验证地址空间复制 |
| ✅ 不依赖外部服务 | ❌ 不测试 CoW 机制 |
| ✅ 快速迭代 | — |

#### 推荐理由

> 当前阶段目标是验证 PM 的 fork 逻辑，不需要真实的 VM 行为。
> Mock 版本足够测试 PM 的错误处理和 endpoint 管理。

---

### 方案二：带 Generation 的 Endpoint

#### 设计思路

完整实现 endpoint 的 generation 管理，每次槽位重用时 generation 递增。

#### 代码实现

```rust
/// Endpoint 生成器
///
/// 管理每个槽位的 generation
pub struct EndpointGenerator {
    /// 每个槽位的当前 generation
    generations: [u16; NR_PROCS],
}

impl EndpointGenerator {
    pub fn new() -> Self {
        Self {
            generations: [0; NR_PROCS],
        }
    }
    
    /// 生成新的 endpoint
    ///
    /// 对应 Minix3 的 `_ENDPOINT(generation, slot)`
    pub fn generate(&mut self, slot: usize) -> Endpoint {
        let gen = self.generations[slot];
        let endpoint = ((gen as i32) << 16) | (slot as i32);
        Endpoint::new(endpoint)
    }
    
    /// 释放槽位，递增 generation
    ///
    /// 对应 Minix3 中进程退出时的行为
    pub fn release(&mut self, slot: usize) {
        self.generations[slot] = self.generations[slot].wrapping_add(1);
    }
    
    /// 从 endpoint 提取槽位
    pub fn slot_from_endpoint(endpoint: Endpoint) -> usize {
        (endpoint.get() & 0xFFFF) as usize
    }
    
    /// 从 endpoint 提取 generation
    pub fn gen_from_endpoint(endpoint: Endpoint) -> u16 {
        ((endpoint.get() >> 16) & 0xFFFF) as u16
    }
}

/// VM fork mock（带 generation）
pub fn vm_fork_with_generation(
    gen: &mut EndpointGenerator,
    child_index: usize,
) -> Result<VmForkResult, VmForkError> {
    if child_index >= NR_PROCS {
        return Err(VmForkError::InvalidSlot);
    }
    
    let child_endpoint = gen.generate(child_index);
    
    Ok(VmForkResult { child_endpoint })
}
```

#### 优缺点分析

| 优点 | 缺点 |
|------|------|
| ✅ 完整实现 endpoint 语义 | ❌ 增加复杂度 |
| ✅ 可检测过期 endpoint | ❌ 需要维护 generation 状态 |
| ✅ 更接近 Minix3 行为 | ❌ 当前阶段可能过度设计 |

#### 不推荐理由

> 当前阶段不需要检测过期 endpoint。generation 管理应该在真正需要时再实现。

---

### 方案三：完整 VM Mock

#### 设计思路

模拟完整的 VM 行为，包括：
- vmproc 槽位管理
- 地址空间复制（模拟）
- 与内核的 sys_fork 交互（模拟）

#### 代码实现

```rust
/// VM 进程结构体（简化版）
#[derive(Debug, Clone)]
pub struct VmProc {
    /// 进程 endpoint
    pub endpoint: Endpoint,
    /// 进程标志
    pub flags: VmFlags,
    /// 地址空间大小（模拟）
    pub as_size: usize,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct VmFlags: u32 {
        /// 进程在内存中
        const IN_USE = 0x01;
        /// 被交换出
        const SWAPPED = 0x02;
    }
}

/// VM 进程表
pub struct VmTable {
    procs: [VmProc; NR_PROCS],
}

impl VmTable {
    pub fn new() -> Self {
        Self {
            procs: std::array::from_fn(|i| VmProc {
                endpoint: Endpoint::new(i as i32),
                flags: VmFlags::empty(),
                as_size: 0,
            }),
        }
    }
    
    /// VM fork（完整模拟）
    pub fn fork(
        &mut self,
        parent_endpoint: Endpoint,
        child_index: usize,
    ) -> Result<Endpoint, VmForkError> {
        // 1. 查找父进程
        let parent_slot = EndpointGenerator::slot_from_endpoint(parent_endpoint);
        if parent_slot >= NR_PROCS {
            return Err(VmForkError::InvalidParent);
        }
        
        // 2. 验证子进程槽位
        if child_index >= NR_PROCS {
            return Err(VmForkError::InvalidSlot);
        }
        
        // 3. 复制地址空间（模拟）
        let parent = &self.procs[parent_slot];
        let child = &mut self.procs[child_index];
        child.as_size = parent.as_size;  // 模拟 CoW
        child.flags = parent.flags | VmFlags::IN_USE;
        
        // 4. 生成 endpoint
        let child_endpoint = Endpoint::new(child_index as i32);
        child.endpoint = child_endpoint;
        
        // 5. 模拟 sys_fork
        // TODO: 调用内核（当前跳过）
        
        Ok(child_endpoint)
    }
}
```

#### 优缺点分析

| 优点 | 缺点 |
|------|------|
| ✅ 完整模拟 VM 行为 | ❌ 过度设计 |
| ✅ 可测试更多场景 | ❌ 增加维护负担 |
| ✅ 为未来扩展做准备 | ❌ 当前阶段不需要 |

#### 不推荐理由

> 过度工程化。当前阶段只需要验证 PM 的 fork 逻辑，不需要完整的 VM 模拟。
> 这会增加不必要的复杂度和维护负担。

---

### 方案四：Trait 抽象

#### 设计思路

定义 `VmFork` trait，支持不同的实现（Mock、真实 VM、测试替身）。

#### 代码实现

```rust
/// VM fork 接口
///
/// 抽象 VM 的 fork 行为，支持不同实现
pub trait VmFork {
    /// 执行 VM fork
    fn fork(&mut self, parent_endpoint: Endpoint, child_index: usize) -> Result<VmForkResult, VmForkError>;
}

/// Mock 实现
pub struct MockVm;

impl VmFork for MockVm {
    fn fork(&mut self, _parent_endpoint: Endpoint, child_index: usize) -> Result<VmForkResult, VmForkError> {
        if child_index >= NR_PROCS {
            return Err(VmForkError::InvalidSlot);
        }
        Ok(VmForkResult {
            child_endpoint: Endpoint::new(child_index as i32),
        })
    }
}

/// 真实 VM 实现（未来）
pub struct RealVm {
    // 与 VM 服务的 IPC 连接
}

impl VmFork for RealVm {
    fn fork(&mut self, parent_endpoint: Endpoint, child_index: usize) -> Result<VmForkResult, VmForkError> {
        // 通过 IPC 调用 VM 服务
        todo!("实现与 VM 的 IPC 通信")
    }
}

/// PM 使用
impl<'a> PmContext<'a> {
    pub fn do_fork<V: VmFork>(&mut self, vm: &mut V) -> Result<ForkResult, ForkError> {
        // ... 前置检查 ...
        
        let vm_result = vm.fork(parent_endpoint, child_index)
            .map_err(|_| ForkError::ResourceExhausted)?;
        
        // ... 后续处理 ...
        
        Ok(ForkResult {
            child_index,
            child_pid,
            child_endpoint: vm_result.child_endpoint,
        })
    }
}
```

#### 优缺点分析

| 优点 | 缺点 |
|------|------|
| ✅ 灵活可扩展 | ❌ 增加抽象层 |
| ✅ 便于测试 | ❌ 泛型增加编译时间 |
| ✅ 支持多种实现 | ❌ 当前只有一个实现 |

#### 不推荐理由

> 当前只有一个实现（Mock），不需要 trait 抽象。
> 过早抽象会增加不必要的复杂度。应该在需要第二个实现时再引入 trait。

---

## 四、方案对比总结

### 4.1 对比表

| 维度 | 方案一 | 方案二 | 方案三 | 方案四 |
|------|--------|--------|--------|--------|
| **代码量** | ⭐⭐⭐ 少 | ⭐⭐ 中 | ⭐ 多 | ⭐⭐ 中 |
| **复杂度** | ⭐⭐⭐ 低 | ⭐⭐ 中 | ⭐ 高 | ⭐⭐ 中 |
| **可测试性** | ⭐⭐ 好 | ⭐⭐⭐ 好 | ⭐⭐⭐ 好 | ⭐⭐⭐ 好 |
| **扩展性** | ⭐ 差 | ⭐⭐ 中 | ⭐⭐⭐ 好 | ⭐⭐⭐ 好 |
| **当前适用性** | ⭐⭐⭐ 高 | ⭐⭐ 中 | ⭐ 低 | ⭐ 低 |

### 4.2 推荐方案

**推荐：方案一（简单 Mock）**

#### 推荐理由

1. **足够当前需求**：只需验证 PM 的 fork 逻辑
2. **快速迭代**：不引入不必要的复杂度
3. **易于理解**：代码简洁明了
4. **后续可扩展**：需要时可以升级到方案二或方案四

---

## 五、实现细节

### 5.1 错误处理

```rust
impl VmForkError {
    /// 转换为错误码
    pub fn to_errno(&self) -> i32 {
        match self {
            Self::InvalidParent => 3,   // ESRCH
            Self::InvalidSlot => 22,    // EINVAL
            Self::KernelError => 12,    // ENOMEM
        }
    }
}
```

### 5.2 约束检查

```rust
/// PM fork 状态机
///
/// 用于确保 PM 在 vm_fork 后不失败
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkPhase {
    /// 初始阶段，可以失败
    Initial,
    /// 槽位已分配，可以回滚
    SlotAllocated,
    /// VM fork 已调用，不能失败
    VmForkCalled,
    /// 进程结构已初始化
    ProcessInitialized,
    /// 完成
    Completed,
}

impl<'a> PmContext<'a> {
    /// 检查是否可以失败
    pub fn can_fail(&self, phase: ForkPhase) -> bool {
        matches!(phase, ForkPhase::Initial | ForkPhase::SlotAllocated)
    }
}
```

### 5.3 与现有代码集成

```rust
impl<'a> PmContext<'a> {
    /// fork 系统调用（完整流程）
    pub fn do_fork(&mut self) -> Result<ForkResult, ForkError> {
        // 阶段 1: 检查和槽位分配（可以失败）
        let prepare_result = self.do_fork_prepare()?;
        
        // 阶段 2: VM fork（可以失败）
        let vm_result = vm_fork_mock(
            self.current_proc().identity.endpoint,
            prepare_result.child_index,
        ).map_err(|_| ForkError::ResourceExhausted)?;
        
        // 阶段 3: 进程结构初始化（不能失败！）
        // ⚠️ 这里开始不能失败，因为 VM 已经创建了内核进程
        self.fork_child_from_parent(
            prepare_result.child_index,
            prepare_result.child_pid,
            vm_result.child_endpoint,
        );
        
        Ok(ForkResult {
            child_index: prepare_result.child_index,
            child_pid: prepare_result.child_pid,
            child_endpoint: vm_result.child_endpoint,
        })
    }
}
```

---

## 六、测试策略

### 6.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_fork_success() {
        let result = vm_fork_mock(Endpoint::PM, 5).unwrap();
        assert_eq!(result.child_endpoint, Endpoint::new(5));
    }

    #[test]
    fn test_vm_fork_invalid_slot() {
        let result = vm_fork_mock(Endpoint::PM, NR_PROCS + 1);
        assert!(matches!(result, Err(VmForkError::InvalidSlot)));
    }

    #[test]
    fn test_vm_fork_endpoint_format() {
        let result = vm_fork_mock(Endpoint::new(100), 42).unwrap();
        // 当前实现：endpoint = slot
        assert_eq!(result.child_endpoint.get(), 42);
    }
}
```

### 6.2 集成测试

```rust
#[test]
fn test_fork_full_flow() {
    let mut ctx = create_test_context();
    
    let result = ctx.do_fork().unwrap();
    
    assert!(result.child_index < NR_PROCS);
    assert!(result.child_pid > 0);
    assert!(result.child_endpoint.is_valid());
    
    let child = ctx.table.get(result.child_index).unwrap();
    assert_eq!(child.identity.endpoint, result.child_endpoint);
}
```

---

## 七、验证清单

### 7.1 功能验证

- [ ] `vm_fork_mock` 返回正确的 endpoint
- [ ] 错误情况正确处理
- [ ] 与 `do_fork_prepare` 正确集成
- [ ] 与 `fork_child_from_parent` 正确集成

### 7.2 约束验证

- [ ] PM 在 `vm_fork` 后不失败的约束得到遵守
- [ ] 错误路径正确回滚

### 7.3 测试验证

- [ ] 单元测试通过
- [ ] 集成测试通过

---

## 八、下一步行动

1. **实现 `vm_fork_mock` 函数**
   - 创建 `os/servers/pm/src/mproc/vm.rs`
   - 实现 `VmForkResult` 和 `VmForkError`
   - 实现 `vm_fork_mock` 函数

2. **更新 `do_fork_prepare`**
   - 集成 `vm_fork_mock` 调用
   - 添加约束检查

3. **编写测试**
   - 单元测试
   - 集成测试

4. **更新文档**
   - 更新 `fork-syscall-plan-part2.md` 状态
