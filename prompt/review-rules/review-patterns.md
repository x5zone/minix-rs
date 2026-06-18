# 常见错误模式

> 本文档汇总文档 Review 和跨文档联动检查中的典型错误模式。
> 适用：文档 Review 时对照检查

---

## §0 P0 必检清单（Gate D，每项必须显式回答 ✅/❌ + grep 证据）

| # | 检查项 | grep 命令模板 | 判定标准 | 未通过 |
|---|-------|--------------|---------|--------|
| **1** | 文档 §5 测试是否真实存在？ | `rg "fn {test_name}" {rust_dir} --type rust -n` | 文档 §5 列出的每个测试函数必须存在；缺失/未找到→P0（测试缺失） | P0 |
| **2** | 文档声明的 trait 是否有 ≥1 impl？ | `rg "impl.*{TraitName}" {rust_dir} --type rust -n` | 0 impl → P0（死代码/虚构 trait）；只有默认 impl 没有真实 arch impl 也算未通过 | P0 |
| **3** | 文档声明的函数是否在声明的文件中？ | `rg "fn {name}" {file}` | 文档说"在 file.rs 中定义 fn foo"，但 grep 无结果→P0（虚构位置）；找到但 signature 完全不符也按未通过处理 | P0 |
| **4** | 核心算法是否是 stub？ | `rg "spin_loop\|todo!\|unimplemented!\|unreachable!\|panic!" {rust_dir} --type rust -n` | 文档描述的算法在代码中体现为 `todo!`/`unimplemented!`/`unreachable!`/`spin_loop!` → P0（实现缺失）；非 test 代码中的 `panic!` 若表示功能未实现或本不应触发却可能触发 → 按 stub / 未处理路径处理，需在注释中论证其不可达性或可接受性 | P0/P1 |
| **5** | 文档 §4 签名是否与实际一致？ | 逐函数对比 `rg "fn {name}" {file}` 输出 vs 文档 §4 | 参数/返回值/可见性/泛型约束不一致→P0（签名偏移）；有一项不符即整项 ❌ | P0 |

**严格通过标准**：
- 5 项每一项必须为 ✅。
- **出现 PARTIAL / ⚠️ / 部分通过 / "基本通过" 中的任何一种，该项按 ❌ 处理，Gate D 整体未通过。**
- 任何一项 ❌ → scan.md 标记 DRAFT，禁止写入 STATE.md。
- 若某项确实不适用（如文档无 §5），需明确说明原因并单独列为一行 "N/A + 原因"，不能直接跳过。

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

## 四、代码错误模式

> 以下模式覆盖 Rust 代码 Review 中最常见的错误类型。共 14 个模式（基础 10 个 + Kernel SMP 4 个）。

### 模式 16：裸整数表达语义（Translate 味道）
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

### 模式 17：C 式空指针/哨兵值
```rust
❌ 错误：
const NO_PHYS: PhysAddr = PhysAddr(0);  // 用 0 表示"无物理地址"
let parent_id = -1 as i32;             // 用 -1 表示"无父进程"

✅ 正确：
let parent_id: Option<ProcessId> = None;
let phys: Option<PhysAddr> = None;
```
> 原因：C 用哨兵值（0, -1, NULL）表达"不存在"，Rust 应用 `Option<T>`。

### 模式 18：unsafe 滥用
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

### 模式 19：错误码不对齐
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

### 模式 20：裸 as 截断无说明
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

### 模式 21：硬件语义泄漏到 OS 层
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

### 模式 22：no_std 违规
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

### 模式 23：pub 滥用
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

### 模式 24：类型安全过度（复杂度失控）
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

### 模式 25：不必要的 trait 抽象

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

### 模式 26：Kernel SMP 并发违规（BKL 未持有）

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

### 模式 27：Kernel SMP 并发违规（Rc/RefCell 跨 CPU 共享）

```rust
❌ 错误：`Rc` 在多 CPU 内核中共享（`!Send + !Sync`）
        use alloc::rc::Rc;
        static KERNEL_CONFIG: Lazy<Rc<KernelConfig>> = Lazy::new(|| Rc::new(...));

✅ 正确：使用 `Arc`（`Send + Sync`）替代 `Rc`
        use alloc::sync::Arc;
        static KERNEL_CONFIG: Lazy<Arc<KernelConfig>> = Lazy::new(|| Arc::new(...));
        // 或 per-CPU 数据仍用 Rc，但必须注释"per-CPU, no cross-CPU sharing"
```

### 模式 28：Kernel SMP 并发违规（spinlock 内睡眠/调度/等待）

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

### 模式 29：Kernel SMP 并发违规（per-CPU 数据被跨 CPU 访问）

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

---

## 五、跨阶段通用错误模式

> 以下 5 个模式从 Boot 阶段问题提炼而来，跨阶段复用。适用于 Boot/VM/PM/VFS/INET 等所有模块。

### 模式 30：外部知识误导（注释中的硬件/协议/规范错误）

```markdown
❌ 错误：x86-64 长模式下"需设置 CR4.PSE 以支持 2MB 大页"
        // PSE 是 32 位保护模式的遗产，64 位长模式下 2MB 页由 PD.PS 位控制

✅ 正确：注释中引用的外部知识必须与当前上下文匹配
        // x86-64 长模式下，2MB 大页（PD.PS=1）和 1GB 大页（PDPT.PS=1）
        // 均由页表项自身的 PS 位控制，不需要 CR4.PSE

适用阶段：Boot（固件规范）、VM（页表硬件）、PM（进程状态机）、VFS（文件系统协议）
```

### 模式 31：通用接口含上下文特定元素

```markdown
❌ 错误：通用数据结构包含未标注的架构特定字段
        struct KernelInfo {
            syscall_entry: VirBytes,  // 所有架构必填，但只在 x86-64 使用
        }

✅ 正确：标注可选性或移至扩展结构
        struct KernelInfo {
            /// x86-64 专用：配置 LSTAR MSR。其他架构忽略。
            syscall_entry: Option<VirBytes>,
        }

适用阶段：Boot（KernelInfo 跨架构）、VM（vmproc 跨进程类型）、VFS（vnode 跨文件系统）
```

### 模式 32：外部调用返回值被无说明忽略

```markdown
❌ 错误：固件/系统调用返回值被丢弃，无注释说明
        let _mmap = exit_boot_services(image, map_key);  // _mmap 被丢弃

✅ 正确：丢弃返回值时显式说明理由
        let _mmap = exit_boot_services(image, map_key);
        // _mmap 被丢弃：boot-shim 在 ExitBootServices 前已构建内存映射，
        // 固件返回的最终映射仅用于调试，kernel 不依赖它。

适用阶段：Boot（固件调用）、VM（页表操作）、PM（IPC 调用）、Drivers（设备 I/O）
```

### 模式 33：资源获取后无释放路径说明

```markdown
❌ 错误：使用泄漏手段但无回收策略说明
        let memmap: &'static [MemoryRegion] = Box::leak(Box::new(regions));
        // 泄漏后如何回收？未说明

✅ 正确：明确声明不释放的理由或回收路径
        let memmap: &'static [MemoryRegion] = Box::leak(Box::new(regions));
        // 'static 生命周期：boot-shim 是"一次性"程序，kernel 启动后
        // 通过 BootPrepareResult::boot_shim_start/len 标记区域，由 kernel
        // 启动早期回收（对标 Minix3 C 的 add_memmap(bootstrap)）。

适用阶段：Boot（bump 分配器）、VM（物理页）、PM（进程槽位）、VFS（缓冲区缓存）
```

### 模式 34：注释理由虚假或牵强

```markdown
❌ 错误：注释给出的理由在上下文中不成立
        pub kern_size: u64,  // u64 避免 32 位目标截断
        // 微内核不可能超过 4GB，"避免截断"不是真实原因

✅ 正确：给出最真实的理由
        pub kern_size: u64,  // 与地址类型（PhysBytes/VirBytes）保持一致，
                             // 避免 kern_virt_base + kern_size 等运算时类型转换

适用阶段：所有阶段的所有注释和设计决策
```

---

## 六、测试错误模式

> 以下模式覆盖测试 Review 中的常见错误。

### 模式 35：测试未覆盖核心语义（L1 对偶缺失）

```rust
❌ 错误：核心函数 alloc_mem 无 C-Rust 对偶测试
        // 仅测试了 Rust 内部逻辑，未对比 Minix3 行为
        #[test]
        fn test_alloc_mem_returns_ok() {
            let result = alloc_mem(1024);
            assert!(result.is_ok());
        }

✅ 正确：L1 对偶测试，验证与 Minix3 行为一致
        #[test]
        fn test_alloc_mem_parity_with_c() {
            // 对比 Minix3 alloc_mem 行为：
            // - 相同输入产生相同输出
            // - 相同错误条件产生相同 errno
            let result = alloc_mem(PageCount(1024));
            // Minix3 在此输入下返回物理地址 0x10000
            assert_eq!(result.unwrap(), PhysAddr(0x10000));
        }
```
> 原因：核心函数必须有 L1 对偶测试，验证 Rust 实现与 C 行为一致。

### 模式 36：Trait 契约测试缺失（L2 缺失）

```rust
❌ 错误：trait Paging 无契约测试
        trait Paging {
            fn map(&mut self, vaddr: VirBytes, paddr: PhysBytes, flags: PageFlags) -> Result<()>;
        }
        // 仅测试了 X8664Paging 的具体实现，未测试 trait 契约

✅ 正确：L2 契约测试，验证所有实现满足 trait 契约
        #[test]
        fn test_paging_contract_map_unmap_roundtrip<P: Paging>(p: &mut P) {
            let vaddr = VirBytes(0x1000);
            let paddr = PhysBytes(0x2000);
            p.map(vaddr, paddr, PageFlags::READ).unwrap();
            assert_eq!(p.query(vaddr), Some((paddr, PageFlags::READ)));
            p.unmap(vaddr).unwrap();
            assert_eq!(p.query(vaddr), None);
        }
```
> 原因：trait 的所有实现必须满足统一契约，L2 测试验证这一点。

### 模式 37：doctest 缺失（L3 缺失）

```rust
❌ 错误：pub 函数无 doctest
        /// 分配物理内存
        pub fn alloc_mem(count: PageCount) -> Result<PhysAddr, AllocError> {
            // ...
        }

✅ 正确：pub 函数有 doctest
        /// 分配物理内存
        ///
        /// # Example
        /// ```
        /// use minix_vm::alloc_mem;
        /// let addr = alloc_mem(PageCount(1)).unwrap();
        /// assert!(addr.as_u64() > 0);
        /// ```
        pub fn alloc_mem(count: PageCount) -> Result<PhysAddr, AllocError> {
            // ...
        }
```
> 原因：pub 函数应有 L3 doctest，既是文档也是可运行测试。

### 模式 38：测试命名不表达意图

```rust
❌ 错误：测试名不表达意图
        #[test]
        fn test_1() { ... }
        #[test]
        fn test_it_works() { ... }
        #[test]
        fn test_alloc() { ... }

✅ 正确：测试名表达意图
        #[test]
        fn test_alloc_mem_returns_aligned_address() { ... }
        #[test]
        fn test_alloc_mem_fails_when_out_of_memory() { ... }
        #[test]
        fn test_alloc_mem_zero_count_returns_error() { ... }
```

### 模式 39：测试仅覆盖正常路径

```rust
❌ 错误：仅测试正常路径
        #[test]
        fn test_map_page() {
            let mut pt = PageTable::new();
            pt.map(vaddr, paddr, flags).unwrap();  // 仅正常路径
        }

✅ 正确：覆盖正常、边界、错误路径
        #[test]
        fn test_map_page_normal() { ... }
        #[test]
        fn test_map_page_already_mapped_returns_error() { ... }
        #[test]
        fn test_map_page_unaligned_address_returns_error() { ... }
        #[test]
        fn test_map_page_zero_address_returns_error() { ... }
```

### 模式 40：测试依赖全局状态（flaky test）

```rust
❌ 错误：测试依赖全局状态，顺序敏感
        static mut COUNTER: u32 = 0;
        #[test]
        fn test_alloc() {
            unsafe { COUNTER += 1; }
            let result = alloc_mem(1);
            assert_eq!(result.unwrap(), PhysAddr(COUNTER * 4096));  // 依赖执行顺序
        }

✅ 正确：测试独立，无全局状态依赖
        #[test]
        fn test_alloc() {
            let allocator = TestAllocator::new();  // 每次新建
            let result = allocator.alloc(1);
            assert!(result.is_ok());
        }
```

---

## 七、卓越性错误模式

> 以下模式覆盖卓越性 Review 中的常见问题。详见 [review-doc-excellence.md](review-doc-excellence.md) 和 [review-code-excellence.md](review-code-excellence.md)。

### 模式 41：文档叙事弧断裂（卓越性）

```markdown
❌ 错误：章节堆砌，无叙事弧
        ## Ch1 概念
        ## Ch2 源码分析
        ## Ch3 设计决策
        ## Ch4 实现
        → 每章直接开始，无过渡，无动机说明

✅ 正确：有叙事弧和动机
        ## Ch1 概念
        本章解决：什么是 VM 内存分配？为什么需要重新设计？
        ## Ch2 源码分析
        上一章定义了核心概念，本章分析 Minix3 如何实现这些概念。
        ## Ch3 设计决策
        Ch2 揭示了 Minix3 的几个问题（X, Y, Z），本章设计解决方案。
```

### 模式 42：文档术语未定义（卓越性）

```markdown
❌ 错误：术语首次出现无定义
        "vmproc 的 vm_acl 字段控制地址空间权限..."
        → vmproc、vm_acl 首次出现未定义

✅ 正确：术语首次出现有定义
        "**vmproc**（VM Process）是 VM 服务器中描述进程地址空间的结构体。
        其 **vm_acl** 字段控制地址空间权限，取值为 -1/0/1~31。"
```

### 模式 43：代码 API 易误用（卓越性）

```rust
❌ 错误：API 允许无效状态
        struct PageTable {
            is_init: bool,
            root: Option<PhysAddr>,  // is_init=true 但 root=None 是无效状态
        }

✅ 正确：用类型系统防止无效状态
        enum PageTable {
            Uninit,
            Init { root: PhysAddr },
        }
```

### 模式 44：代码错误类型不精确（卓越性）

```rust
❌ 错误：滥用 Box<dyn Error>
        fn alloc_mem(count: PageCount) -> Result<PhysAddr, Box<dyn Error>> {
            if count.0 == 0 {
                return Err("zero count".into());  // 字符串错误，无类型信息
            }
            // ...
        }

✅ 正确：精确错误类型
        enum AllocError {
            ZeroCount,
            OutOfMemory,
            InvalidAlignment,
        }
        fn alloc_mem(count: PageCount) -> Result<PhysAddr, AllocError> {
            if count.0 == 0 {
                return Err(AllocError::ZeroCount);
            }
            // ...
        }
```

### 模式 45：代码冗余注释（卓越性）

```rust
❌ 错误：注释重复代码已表达的信息
        let x = 5;  // x 赋值为 5
        let y = x + 1;  // y 等于 x 加 1

✅ 正确：注释解释"为什么"而非"是什么"
        let page_count = (size + PAGE_SIZE - 1) / PAGE_SIZE;  // 向上取整到页边界
```

### 模式 46：代码副作用隐藏（卓越性）

```rust
❌ 错误：看似纯函数有隐藏副作用
        fn get_process_count() -> u32 {
            PROCESS_COUNT += 1;  // 隐藏的副作用：递增计数器
            PROCESS_COUNT
        }

✅ 正确：副作用显式
        fn get_and_increment_process_count() -> u32 {
            PROCESS_COUNT += 1;
            PROCESS_COUNT
        }
        // 或分离查询和修改
        fn get_process_count() -> u32 { PROCESS_COUNT }
        fn increment_process_count() { PROCESS_COUNT += 1; }
```

### 模式 47：代码全局依赖未注入（卓越性）

```rust
❌ 错误：依赖全局状态，难以测试
        fn alloc_mem(count: PageCount) -> Result<PhysAddr, AllocError> {
            let bitmap = unsafe { &GLOBAL_BITMAP };  // 全局依赖
            bitmap.alloc(count)
        }

✅ 正确：依赖注入
        fn alloc_mem(
            bitmap: &mut Bitmap,
            count: PageCount,
        ) -> Result<PhysAddr, AllocError> {
            bitmap.alloc(count)
        }
        // 测试时可注入 mock bitmap
```

---

## 八、叙事与概念错误模式

> 以下模式覆盖 03-kmain-cstart 重构案例暴露的叙事/概念层问题。详见 [review-doc-checklist.md §1.Ch1](review-doc-checklist.md#1ch1-ch1-强制骨架) Ch1 骨架检查、[review.md §概念抽象原则](review.md#概念抽象原则)。

### 模式 48：因果链编造（P0）`[来源: GLM(主) + M3]`

```markdown
❌ 错误：claim 正确但解释的因果链技术上错误
        "memcpy(&kinfo, local_cbi, ...) 是必要的" → 正确
        解释："调用链推进后栈帧被覆盖" → 错误（C 语义上 kmain 未返回时栈帧存在）

✅ 正确解释："local_cbi 作用域限于 kmain 调用链，非 kmain 链代码（如中断处理）需通过 kinfo 访问"
```

**判定**：因果链机制技术上错误 → P0（即使 claim 本身正确）
**与模式 34 边界**：模式 34 是代码注释理由虚假；本模式是文档正文因果链编造。

### 模式 49：元注释泄漏（P1）`[来源: M3 + GLM + Seed]`

```markdown
❌ 错误：正文出现作者 narrate 写作策略的文字
        "本节只回答…避免重复"     ← 范围声明类
        "一句话带过"               ← 深度策略类
        "必须加"                   ← 作者自我强调类
        "早期文档曾表述…这不准确"  ← 历史修正叙述类

✅ 正确：删掉后读者对主题理解不变 → 是元注释，应删除或改写为正文
```

**判别口诀**："删掉这段后，读者对主题的理解是否减少？" 不变 → 元注释。
**判定**：单处 → P2；>5 处 → P1（系统性风格问题）。
**与模式 15 区别**：模式 15 是"已实现/待实现"进度追踪；本模式是作者 narrate 写作策略。

### 模式 50：架构范围未标注（P1）`[来源: GLM + M3]`

```markdown
❌ 错误：x86 特有机制当共性讲
        "段寄存器依赖 GDT"  ← 未标注，读者以为是三架构共性

✅ 正确：标注架构范围
        "> 架构范围：x86-64"
        "段寄存器依赖 GDT（x86-64 特有）"
```

**判定**：未标注 → P1；ISA 寄存器角色描述错误（如 RISC-V SPP 当作"当前特权级"）→ P0。

### 模式 51：实现驱动概念章（P1）`[来源: GLM(主) + M3 + Seed]`

```markdown
❌ 错误：Ch1 以函数名作为概念定义的起点
        "prot_init() 是配置保护结构的函数…"  ← 主语是函数名
        Ch1 正文 = "调用 prot_init() 然后…"

✅ 正确：Ch1 从"为什么需要"出发
        "CPU 在特权级切换时需要回答三个问题：当前特权级？异常入口？内核栈？"
        prot_init() 作为"回答三问的机制"被引入
```

**判定**：Ch1 开篇主语是函数名 → P1。
**裸概念复述测试**：把 Ch1 中所有函数名/结构体名/trait 名用 `[XXX]` 替换，若核心概念仍能被理解 → ✅；若变成"调用 [XXX] 然后 [YYY]"的无意义流水账 → P1。

### 模式 52：单向心智模型（P1）`[来源: GLM + M3]`

```markdown
❌ 错误：进入类机制只讲"进入"
        "trap 入口保存上下文，跳到 handler"  ← 只讲进入，不讲返回

✅ 正确：双向闭环
        "trap 入口保存上下文 → handler → 恢复上下文 → 返回"
        显式覆盖"返回"路径（iret/eret/sret）
```

**判定**：进入类机制（trap/syscall/IPC）只单向 → P1。

### 模式 53：跨架构共性未提取（P1）`[来源: GLM + Seed]`

```markdown
❌ 错误：多架构文档直接堆三架构细节
        "x86: ring 0/3 + IDT"
        "aarch64: EL1/EL0 + VBAR"
        "riscv64: S/U-mode + stvec"
        ← 读者要自己抽象共性

✅ 正确：先给统一框架
        "三架构都需要回答：当前特权级？异常入口？内核栈？"
        再分架构展示如何回答
```

**判定**：多架构文档无统一抽象层 → P1。

### 模式 54：视角漂移（P2）`[来源: M3]`

```markdown
❌ 错误：同一章节主语频繁切换
        §1.1 主语 = CPU
        §1.2 主语 = prot_init()
        §1.3 主语 = 读者
        ← 读者需要不断切换视角

✅ 正确：同一章节保持一致主语
        Ch1 全章主语 = CPU（概念驱动）
        或显式标注视角切换："以下从 prot_init() 实现视角分析"
```

**判定**：同一章节主语频繁切换且无标注 → P2。

### 模式 55：架构特有机制喧宾夺主（P2）`[来源: GLM + M3]`

```markdown
❌ 错误：架构特有遗留机制当核心讲
        Ch1 大篇幅讲 x86 GDT 的 8 字节描述符格式

✅ 正确：降级为补充章节
        主文一句话建模型："GDT 是 x86 段机制的遗留表"
        附录展开细节，标题用疑问式："x86 为什么还保留 GDT"
```

**判定**：架构特有遗留机制在 Ch1 占核心篇幅 → P2。

### 模式 56：决策日志体 Ch3（P1）`[来源: M3]`

```markdown
❌ 错误：Ch3 变成决策日志
        "决策 1：选 A
         决策 2：选 B
         决策 3：选 C"
        ← 只列结论，不解释为什么

✅ 正确：Ch3 每个决策解释"为什么选 A 而非 B"
        "决策 1：选 A 而非 B，因为 A 在 X 场景下更优，B 在 Y 场景下有 Z 问题"
```

**判定**：Ch3 只罗列决策无推理过程 → P1。

### 模式 57：例子选择有"读者前置知识泄漏"（P2）`[来源: GLM]`

```markdown
❌ 错误：概念引入示例引入与核心机制无关的细节
        讲"保护"时用 `*(0xffffffff80000000)=0` 作为例子
        ← 读者会问"为什么这地址是内核地址"（与保护机制无关）

✅ 正确：示例最小化
        讲"保护"时用 `*((int*)0)=0`
        ← 读者只关心"为什么空指针写失败"
```

**判定**：示例引入额外问题 → P2；本章开门第一个例子就有此问题 → P1。

```
