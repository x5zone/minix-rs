# 常见错误模式

> 本文档汇总文档 Review 和跨文档联动检查中的典型错误模式。
> 适用：文档 Review 时对照检查

---

## 一、文档错误模式

### 模式1：概念混淆
```markdown
❌ 错误：Slab 最大对象大小为 200 字节
✅ 正确：Slab 支持 200 种尺寸（SLABSIZES=200），
        最大对象大小为 207 字节（MAXSIZE = SLABSIZES-1+MINSIZE = 207）
```

### 模式2：条件编译遗漏
```markdown
❌ 错误：SPAREPAGES = 200
✅ 正确：SPAREPAGES 根据构建配置不同：
        - SANITYCHECKS 构建：200
        - ARM 生产构建：150
        - x86 生产构建：20
```

### 模式3：行为简化误导
```markdown
❌ 错误：alloc_pages 从高地址向低地址扫描
✅ 正确：alloc_pages 使用循环扫描策略：
        1. 从 lastscan 位置开始向低地址扫描
        2. 如果失败，从 maxpage 重新开始扫描
        3. 更新 lastscan 为本次分配位置
```

### 模式4：架构差异未说明
```markdown
❌ 错误：pt_t 结构体包含 pt_pt[1024] 数组
✅ 正确：pt_t.pt_pt[1024] 是 Minix3 x86-32 设计。
        minix-rs 使用 x86-64，页表层级更多，采用动态分配策略。
```

### 模式5：代码与文档不一致
```markdown
❌ 错误：文档描述 `alloc_virt(phys, clicks)` 分配虚拟地址并映射到 phys
✅ 正确：实际代码中 `alloc_virt` 忽略 phys 参数，仅分配虚拟地址槽位，
        不建立映射。应重命名为 `alloc_virt_slot` 以明确语义。
```

### 模式6：ASCII 图滥用（无必要）
```markdown
❌ 错误：用大段 ASCII 图解释"为什么 refcount 初始化为 0"
        ┌─────────────────────────────────────┐
        │  设计原因:                           │
        │  1. 分离创建和引用                    │
        │  2. 灵活性                           │
        │  3. 一致性                           │
        └─────────────────────────────────────┘
✅ 正确：纯文字即可，无需画图：
        refcount 初始化为 0 的原因：
        1. 分离创建和引用：pb_new() 仅创建，pb_link() 建立引用
        2. 灵活性：可预分配，延迟链接
        3. 一致性：refcount 始终等于链表长度
```

### 模式7：ASCII 图质量差（对齐混乱）
```markdown
❌ 错误：右边界参差不齐，箭头错位
        ┌─────────────────────────────────────┐
        │ phys_block                           │
        │ firstregion ────────────┐            │
        └─────────────────────────┼────────────┘
                                  │
        ┌─────────────────────────┼──────────┐
        │ phys_region A           │          │
        │ next_ph_list ───────────┼────┐     │
        └─────────────────────────┼────┼─────┘
                                  │    │
                                  │    │  ← 箭头和边框未对齐

✅ 正确：要么严格对齐，要么用文字/表格替代
        推荐使用表格或列表描述链表关系：
        | phys_block | phys_region A | phys_region B |
        |------------|---------------|---------------|
        | phys       | offset: 0x0   | offset: 0x1000 |
        | refcount:3 | parent: procA | parent: procB  |
```

### 模式8：路径使用用户环境绝对路径
```markdown
❌ 错误：使用 `file:///home/user/...` 等用户环境绝对路径
        [pagetable.c:333-389](file:///home/xzhao/github/minix-rs/minix3/minix/servers/vm/pagetable.c#L333-L389)

✅ 正确：使用项目根相对路径
        [pagetable.c:333-389](minix3/minix/servers/vm/pagetable.c#L333-L389)

原因：
1. **可移植性**：绝对路径绑定到特定用户环境，其他开发者无法使用
2. **可维护性**：项目路径变化时无需逐个修改文档
3. **一致性**：所有文档统一使用项目根为基准，便于 grep 搜索
4. **IDE 兼容**：主流 IDE 支持项目相对路径的跳转
```

### 模式9：C 源码覆盖不完整
```markdown
❌ 错误：vm_mappages 是页表操作的核心函数，
        但 07-pagetable-ops.md 没有分析它
        → 导致页表操作文档缺功能

❌ 错误：pt_t 是页表的核心结构体，
        但 06-pagetable-struct.md 没有完整分析所有字段
        → 导致页表实现缺少类型定义

❌ 错误：ARCH_VM_PTE_* 系列宏没有单独分析
        → 导致 Rust 实现可能遗漏某些标志位

✅ 正确：
        - 06-pagetable-struct.md 包含 pt_t 完整定义
        - 07-pagetable-ops.md 包含 vm_mappages 完整分析
        - 07-pagetable-ops.md 包含 ARCH_VM_PTE_* 标志位说明
```

### 模式10：设计决策缺乏依据
```markdown
❌ 错误：Ch3 直接说"使用 typestate 模式"，
        但 Ch1&2 没有分析为什么需要 typestate

✅ 正确：Ch3 说"Ch2§2.3 分析了 vm_acl 的三种状态（NO_ACL/USER_ACL/SYS_ACL），
        其中 NO_ACL 是生命周期状态而非权限策略。为防止混淆，使用 typestate
        将 NO_ACL 阶段与权限阶段在类型层面分离。"
```

### 模式11：设计与实现脱节
```markdown
❌ 错误：Ch3 设计了 typestate 模式，但 Ch4 代码仍用 int flags
        → 设计是设计，代码是代码，两者无关

✅ 正确：Ch4 的代码必须体现 Ch3 的设计决策。
        如果不一致，要么修改代码以匹配设计，要么更新设计并说明理由
```

### 模式12：测试未覆盖设计决策
```markdown
❌ 错误：Ch3 设计了 ACL typestate 转换（NO_ACL→USER_ACL、NO_ACL→SYS_ACL），
        但测试章节只测了"权限检查是否正确"
        → 测试只验证了实现细节，没验证设计决策

✅ 正确：测试章节应包含：
        - NO_ACL→USER_ACL 转换测试
        - NO_ACL→SYS_ACL 转换测试
        - 非法转换（如 USER_ACL→NO_ACL）应被拒绝
        这些测试对应 Ch3 的 typestate 设计决策
```

### 模式13：no_std 违规
```markdown
❌ 错误：设计或代码使用 `std::collections::HashMap`
        → OS 内核不可能依赖 std

✅ 正确：使用 `alloc::collections::BTreeMap` 或 `hashbrown::HashMap`（no_std 兼容）
        或使用固定大小数组 + 自定义查找
```

### 模式14：硬件未抽象为 trait
```markdown
❌ 错误：数据结构直接编码硬件寄存器布局
        struct PageTable {
            pde: [u32; 1024],  // x86-32 PDE 数组
        }
        → 上层代码直接操作 PDE 位，与架构紧耦合

❌ 错误：使用 #[cfg(target_arch)] 选择硬件行为
        #[cfg(target_arch = "x86_64")]
        fn map_page(...) { /* x86-64 PTE 操作 */ }
        #[cfg(target_arch = "aarch64")]
        fn map_page(...) { /* ARM64 描述符操作 */ }
        → 条件编译分散在各处，新增架构需改动所有调用点

✅ 正确：抽象机制为 trait，描述"做什么"而非"怎么做"
        trait Paging {
            const PAGE_SIZE: usize;
            fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags) -> Result<(), PageTableError>;
            fn unmap(&mut self, vaddr: VirBytes) -> Result<PhysBytes, PageTableError>;
            fn query(&self, vaddr: VirBytes) -> Option<(PhysBytes, PageFlags)>;
        }
        → 上层仅依赖 trait 接口，各架构自行实现
```

### 模式15：开发记录风格（文档定位偏移）
```markdown
❌ 错误：文档中出现开发进度标记
        "已实现：handle_munmap 函数"
        "待实现：munmap_vm_lin 特殊路径"
        "✅ Split-First 原则  ❌ 权限检查"
        "## 实现清单"
        "## 现有代码状态"
        → 读者关心"系统怎么工作"，不关心"开发者做了什么"

✅ 正确：面向读者的知识讲解风格
        "handle_munmap 函数负责处理取消映射请求"
        "munmap_vm_lin 处理 VM 进程自身的取消映射"
        "Split-First 原则"（去掉状态标记）
        "## 模块组成与测试"
        "## Rust 实现组件一览"

核心区分：
- 开发记录："X 已做了 / Y 还没做" → 关注开发者状态
- 知识讲解："X 的作用是 A / Y 处理 B 场景" → 关注系统行为

判定标准：
- "已实现/待实现/未实现" → P1
- ✅❌🚧 状态 emoji → P1
- "实现清单/代码状态"等进度标题 → P1
- 大量进度标记（>5处）→ 需系统性重写相关章节
- TODO 允许保留（TODO 标记待做事项，与进度标记不同）
```

---

## 二、跨文档联动错误模式

### 模式 A：重复定义
```markdown
# 文档 04-physical-memory.md
#define CLICK_SIZE 4096

# 文档 05-vm-allocpage.md  
#define CLICK_SIZE 4096  // ❌ 重复定义，应引用 04 文档
```

### 模式 B：定义矛盾
```markdown
# 文档 03-acl.md
vm_acl 取值：-1 (NO_ACL), 0 (USER_ACL), 1~31 (系统 ACL)

# 文档 01-vmproc-struct.md
vm_acl 取值：-1, 0, 1~32  // ❌ 与 03 文档矛盾（32 vs 31）
```

### 模式 C：遗漏引用
```markdown
# 文档 05-vm-allocpage.md
vm_allocpage 使用 pt_init_done 标志...

# 缺少引用：pt_init_done 在 07-pagetable-ops.md 中详细定义
# 应添加：参见 [07-pagetable-ops.md](07-pagetable-ops.md)
```

---

## 三、检查范围与验证方法

### 跨文档检查范围

**主要范围**：本文档所在目录下的所有 `.md` 文件（同一模块内聚）
**扩展范围**：本文档"参见"章节中引用的其他目录文档
**排除范围**：未引用的其他模块文档（如 VM 文档不检查 PM 文档，除非显式引用）

### 检查清单

- [ ] **共享数据结构**：本文档使用的结构体是否在同目录其他文档中有更权威的定义？描述是否一致？
- [ ] **共享常量/配置**：本文档引用的常量是否与同目录其他文档一致？是否有重复定义？
- [ ] **跨模块调用**：本文档描述的 IPC 接口是否与"参见"引用的其他模块文档匹配？
- [ ] **前置依赖**：本文档假设读者已了解的概念，在同目录下是否有对应的文档？是否正确引用？
- [ ] **后置引用**：同目录下其他文档是否依赖本文档的内容？本文档的变更是否会破坏那些文档？

### 验证方法

```bash
# 假设当前文档位于 notes/rewrite/fork-syscall-rewrite/02-stage-vm/
DIR="notes/rewrite/fork-syscall-rewrite/02-stage-vm"

# 1. 检查同目录下常量是否多处定义
rg "CLICK_SIZE\s*=" "$DIR" --type md -n

# 2. 检查同目录下结构体是否多处描述
rg "struct vmproc" "$DIR" --type md -n

# 3. 检查同目录下 IPC 调用号是否一致
rg "VM_RQ_BASE" "$DIR" --type md -n

# 4. 检查文档间交叉引用
rg "\[.*\]\(.*\.md\)" "$DIR" --type md -n

# 5. 检查"参见"引用的文档是否存在
rg "\[.*\]\((\.\./.*\.md)\)" "$DIR" --type md -n
```

---

## 三、代码错误模式

> 以下模式覆盖 Rust 代码 Review 中最常见的错误类型。

### 模式 15：裸整数表达语义（Translate 味道）
```rust
❌ 错误：
fn alloc_pages(count: u32, flags: u32) -> i32 {
    // ...
}

✅ 正确：
fn alloc_pages(count: PageCount, flags: PageFlags) -> Result<PhysAddr, AllocError> {
    // ...
}
```
> 原因：裸 `u32` 不表达语义，`i32` 返回负数为错误码是 C 风格。应用 newtype 和 `Result`。

### 模式 16：C 式空指针/哨兵值
```rust
❌ 错误：
const NO_PHYS: PhysAddr = PhysAddr(0);  // 用 0 表示"无物理地址"
let parent_id = -1 as i32;             // 用 -1 表示"无父进程"

✅ 正确：
let parent_id: Option<ProcessId> = None;
let phys: Option<PhysAddr> = None;
```
> 原因：C 用哨兵值（0, -1, NULL）表达"不存在"，Rust 应用 `Option<T>`。

### 模式 17：unsafe 滥用
```rust
❌ 错误：
let ptr = addr as *mut u8;
unsafe { *ptr = value; }  // 无 safety 注释，无契约说明

✅ 正确：
/// SAFETY: `addr` must be a valid, aligned physical address within the
/// mapped page range. Caller guarantees the page is writable.
unsafe fn write_phys(addr: PhysAddr, value: u8) {
    let ptr = addr.as_mut_ptr::<u8>();
    unsafe { *ptr = value; }
}
```
> 原因：每个 `unsafe` 块必须有 safety 注释说明契约和调用方责任。

### 模式 18：错误码不对齐
```rust
❌ 错误：
fn vm_mappages(...) -> Result<(), Error> {
    if page_not_found {
        return Err(Error::NotFound);  // 自己造的错误码
    }
    // Minix3 中应返回 EINVAL
}

✅ 正确：
fn vm_mappages(...) -> Result<(), VmError> {
    if page_not_found {
        return Err(VmError::Einval);  // 对应 Minix3 的 EINVAL
    }
```
> 原因：错误码必须与 Minix3 原始 errno 严格对应，禁止自创或合并错误语义。

### 模式 19：裸 as 截断无说明
```rust
❌ 错误：
let pages = (size + PAGE_SIZE - 1) / PAGE_SIZE;
let old_count = pages as u16;  // u64 → u16 可能截断，无注释

✅ 正确：
let pages = (size + PAGE_SIZE - 1) / PAGE_SIZE;
// Page count is bounded by MAX_PAGES (1024), fits in u16.
let old_count = pages as u16;
```
> 原因：所有可能截断的 `as` 转换必须注释说明安全性（特别是 64→32 位转换）。

### 模式 20：硬件语义泄漏到 OS 层
```rust
❌ 错误：
struct PageTable {
    cr3_value: u64,  // x86 特定寄存器
}

fn enable_paging(cr3: u64) {
    unsafe { asm!("mov cr3, {}", in(reg) cr3); }
}

✅ 正确：
trait Paging {
    fn load_table(&self, table: PhysAddr);
    fn enable(&self);
}
// x86-64 实现
impl Paging for X8664Paging {
    fn load_table(&self, table: PhysAddr) {
        unsafe { asm!("mov cr3, {}", in(reg) table.as_u64()); }
    }
}
```
> 原因：OS 层不感知 CR3 等具体硬件寄存器，所有硬件操作通过 trait 抽象。

### 模式 21：no_std 违规
```rust
❌ 错误：
// 在 vm_main.rs 中（非 test 模块）
use std::collections::HashMap;
let map = HashMap::new();

✅ 正确：
// 在 #[cfg(test)] 模块中使用 std 是允许的
// 在生产代码中，使用 alloc 或自定义集合
use alloc::collections::BTreeMap;
let map = BTreeMap::new();
```
> `std::` 的使用规则：仅在 `#[cfg(test)]` 和 mock 中允许。
> 生产代码必须 `no_std` 兼容。

### 模式 22：pub 滥用
```rust
❌ 错误：
pub struct VmProc {
    pub id: u32,         // 所有字段 pub
    pub state: State,
    pub page_table: PageTable,
}

✅ 正确：
pub struct VmProc {
    pub(crate) id: u32,       // 模块内可见
    pub(crate) state: State,
    page_table: PageTable,    // 私有，通过方法访问
}
```
> 口诀：「这个 pub 是因为外部需要，还是因为内部懒得组织？」

### 模式 23：类型安全过度（复杂度失控）
```rust
❌ 错误：
// 每个状态都变成一个类型，导致类型爆炸
struct InitVmProc { ... }
struct RunningVmProc { ... }
struct BlockedVmProc { ... }
struct DyingVmProc { ... }
impl InitVmProc {
    fn to_running(self) -> Result<RunningVmProc, ...> { ... }
}
// 当状态转换是运行时决定时，typestate 模式收益不大

✅ 正确：
enum VmState {
    Init,
    Running,
    Blocked,
    Dying,
}
struct VmProc {
    state: VmState,  // 简单 enum + 运行时检查
    // ...
}
```
> 原因：类型安全是有成本的。如果复杂度超过收益，降级为 enum + 运行时检查。

### 模式 24：不必要的 trait 抽象

```rust
❌ 错误：创建 trait 但所有实现行为相同
        trait VmPagingExt {
            fn bind_to_process(&self, proc: &VmProc);
        }
        impl VmPagingExt for X8664Paging {
            fn bind_to_process(&self, proc: &VmProc) {
                sys_vmctl_set_addrspace(proc.endpoint, self.cr3_value());
                // x86-64 和 aarch64 实现完全相同，都调用同一个 syscall
            }
        }
        → trait 没有提供任何多态价值，改为自由函数即可

❌ 错误：单方法 trait 从未作为 trait bound 使用
        trait PhysAllocatorStats {
            fn memstats(&self) -> PhysMemStats;
        }
        // 从未写过 fn foo<T: PhysAllocatorStats>(t: &T)
        // 只在具体类型上调用 bitmap.memstats() / buddy.memstats()
        → 改为各分配器的固有方法 + PhysAlloc enum 分发

✅ 正确：trait 有多个不同实现，且作为泛型约束使用
        trait Paging {
            fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags)
                -> Result<(), PageTableError>;
        }
        // x86-64 用 4 级页表，aarch64 用不同的描述符格式
        // fn paging_init<P: Paging>(p: &mut P) 使用 trait bound
        → 合理的 trait 设计

> 原因：trait 的价值在于多态——不同实现、不同行为。如果所有实现相同，
> trait 只是增加了间接层而没有实际收益。判断标准：
> 1. 是否有 ≥2 个行为不同的实现？
> 2. 是否被用作泛型约束（trait bound）？
> 两个条件都满足 → 合理的 trait；任一不满足 → 考虑简化。

### 模式 25：Kernel SMP 并发违规（BKL 未持有）

```rust
❌ 错误：内核全局变量访问未受 BKL 保护
        static mut CPU_READY: bool = false;
        fn check_cpu_ready() -> bool {
            unsafe { CPU_READY }  // 其他 CPU 可能正在写 CPU_READY
        }

✅ 正确：明确标注 BKL 保护的前提
        /// SAFETY: Caller must hold BKL (BKL_LOCK() already acquired).
        static mut CPU_READY: bool = false;
        fn check_cpu_ready() -> bool {
            // 调用者已持有 BKL，单写者保证
            unsafe { CPU_READY }
        }
```
> 原因：内核 SMP 环境下，全局可变状态必须被 BKL、per-CPU 隔离、或 Atomic 保护。

### 模式 26：Kernel SMP 并发违规（Rc/RefCell 跨 CPU 共享）

```rust
❌ 错误：`Rc` 在多 CPU 内核中共享（`!Send + !Sync`）
        use alloc::rc::Rc;
        static KERNEL_CONFIG: Lazy<Rc<KernelConfig>> = Lazy::new(|| Rc::new(...));

✅ 正确：使用 `Arc`（`Send + Sync`）替代 `Rc`
        use alloc::sync::Arc;
        static KERNEL_CONFIG: Lazy<Arc<KernelConfig>> = Lazy::new(|| Arc::new(...));
        // 或 per-CPU 数据仍用 Rc，但必须注释"per-CPU, no cross-CPU sharing"
```

### 模式 27：Kernel SMP 并发违规（spinlock 内睡眠/调度/等待）

```rust
❌ 错误：BKL 内等待 IPC 响应（spinlock 内禁止 block）
        BKL_LOCK();
        let reply = ipc_sendrec(PM_PROC_NR, &msg);  // 可能 block
        BKL_UNLOCK();

✅ 正确：释放 BKL 后等待 IPC，重新获取后验证共享状态
        BKL_UNLOCK();
        let reply = ipc_sendrec(PM_PROC_NR, &msg);
        BKL_LOCK();
        // 注意：重新获取 BKL 后，共享状态可能已被其他 CPU 修改
        // 需要重新验证共享状态 invariants
```
> 原因：BKL 是 spinlock（busy-wait），spinlock 内任何可能导致当前 CPU 让出执行权的操作（睡眠、调度、等待锁、等待 IPC 响应）都可能导致 deadlock。

### 模式 28：Kernel SMP 并发违规（per-CPU 数据被跨 CPU 访问）

```rust
❌ 错误：直接读取其他 CPU 的 local 数据，无保护
        fn get_cpu_ticks(cpu_id: u32) -> u64 {
            unsafe { PER_CPU_DATA[cpu_id as usize].ticks }
        }

✅ 正确：per-CPU 数据不暴露跨 CPU 读取接口
        fn get_my_ticks() -> u64 {
            let cpu = get_cpu_var();
            unsafe { PER_CPU_DATA[cpu].ticks }
        }

✅ 正确：如需跨 CPU 读取，使用 Atomic 类型
        struct PerCpuData {
            ticks: AtomicU64,  // Atomic 保证跨 CPU 可见性
        }
```
```
