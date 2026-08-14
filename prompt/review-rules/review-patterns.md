# 常见错误模式

> 本文档汇总文档 Review 和跨文档联动检查中的典型错误模式。
> 适用：文档 Review 时对照检查

---

## §0 P0 必检清单（Gate D，每项必须显式回答 ✅/❌ + grep 证据）

| # | 检查项 | grep 命令模板 | 判定标准 | 未通过 |
|---|-------|--------------|---------|--------|
| **1** | 文档 §5 测试是否真实存在？ | `rg "fn {test_name}" {rust_dir} --type rust -n` | 文档 §5 列出的每个测试函数必须存在；缺失/未找到→P0（测试缺失） | P0 |
| **2** | 文档声明的 trait 是否有 ≥2 行为不同的 impl？ | `rg "impl.*{TraitName}" {rust_dir} --type rust -n` | 0 impl → P0（死代码/虚构 trait）；1 impl → P1（可能死代码，trait 抽象需 ≥2 行为不同的实现）；≥2 impl → ✅ | P0/P1 |

> **2026-08-15 修复 C-P1-3（0 impl 优先于测试覆盖）**：当 trait 0 impl 时，即便有测试覆盖，仍判 P0（trait 无任何实现 = 死代码 / 虚构）。判定优先级：**0 impl → P0 > 测试覆盖度判定**。判定流程：(a) 先检查 trait 是否有 ≥1 impl？否 → P0 死代码；(b) 有 impl → 检查测试覆盖度，0 测试覆盖 → P0-test-missing，< 3 测试 → P1。
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

**与 design 的关系**：
- 若 design 已规定 `unsafe` 使用规则但代码违反 → **P0-design-deviation**（code Refactor）
- 若 design 未规定 safety 注释要求 → **P0-design-missing**（design Refactor）
- safety 论据与 Minix3 原始假设冲突 → **P0-design-wrong**（design Refactor 必须）

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

这一分类同时覆盖 `Box<dyn Error>`（Rust 标准错误）→ 应改为具体的 `VmError` 等领域错误。

**与 design 的关系**：
- 若 design 已规定错误码但 Rust 未对齐 → **P0-design-deviation**（code Refactor：修 code）
- 若 design 未规定错误码策略但实现应规定 → **P0-design-missing**（design Refactor）
- 若 design 规定与 Minix3 不一致 → **P0-design-wrong**（design Refactor 必须）

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

**与 design 的关系**：
- 若 design 已规定 `as` 使用规则但代码裸用 → **P0-design-deviation**（code Refactor）
- 若 design 未规定但实现需要规定 → **P0-design-missing**（design Refactor）
- 截断阈值与 Minix3 不一致 → **P0-design-wrong**（design Refactor 必须）

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

**与 design 的关系**：
- 若 design 已规定硬件抽象但实现直接用寄存器 → **P0-design-deviation**（code Refactor）
- 若 design 未规定硬件抽象机制 → **P0-design-missing**（design Refactor）
- 跨架构差异未在 design 中标注 → **P0-design-wrong**（design Refactor 必须）

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

**与 design 的关系**：
- 若 design 已规定 `no_std` 但实现用 `std::` → **P0-design-deviation**（code Refactor）
- 若 design 未规定标准库策略 → **P0-design-missing**（design Refactor）
- `std::` 使用违反 `no_std` 整体约束 → **P0-design-wrong**（design Refactor 必须）

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

> 以下模式覆盖 03-kmain-cstart 重构案例暴露的叙事/概念层问题。详见 [review-doc-checklist.md §1.Ch1](review-doc-checklist.md#1ch1-ch1-强制骨架概念章专项强制) Ch1 骨架检查、[review.md §概念抽象原则](review.md#概念抽象原则)。

### 模式 48：因果链编造（P0）

```markdown
❌ 错误：claim 正确但解释的因果链技术上错误
        "memcpy(&kinfo, local_cbi, ...) 是必要的" → 正确
        解释："调用链推进后栈帧被覆盖" → 错误（C 语义上 kmain 未返回时栈帧存在）

✅ 正确解释："local_cbi 作用域限于 kmain 调用链，非 kmain 链代码（如中断处理）需通过 kinfo 访问"
```

**判定**：因果链机制技术上错误 → P0（即使 claim 本身正确）
**与模式 34 边界**：模式 34 是代码注释理由虚假；本模式是文档正文因果链编造。

### 模式 49：元注释泄漏（P1）

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

### 模式 50：架构范围未标注（P1）

```markdown
❌ 错误：x86 特有机制当共性讲
        "段寄存器依赖 GDT"  ← 未标注，读者以为是三架构共性

✅ 正确：标注架构范围
        "> 架构范围：x86-64"
        "段寄存器依赖 GDT（x86-64 特有）"
```

**判定**：未标注 → P1；ISA 寄存器角色描述错误（如 RISC-V SPP 当作"当前特权级"）→ P0。

### 模式 51：实现驱动概念章（P1）

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

### 模式 52：单向心智模型（P1）

```markdown
❌ 错误：进入类机制只讲"进入"
        "trap 入口保存上下文，跳到 handler"  ← 只讲进入，不讲返回

✅ 正确：双向闭环
        "trap 入口保存上下文 → handler → 恢复上下文 → 返回"
        显式覆盖"返回"路径（iret/eret/sret）
```

**判定**：进入类机制（trap/syscall/IPC）只单向 → P1。

### 模式 53：跨架构共性未提取（P1）

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

### 模式 54：视角漂移（P2）

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

### 模式 55：架构特有机制喧宾夺主（P2）

```markdown
❌ 错误：架构特有遗留机制当核心讲
        Ch1 大篇幅讲 x86 GDT 的 8 字节描述符格式

✅ 正确：降级为补充章节
        主文一句话建模型："GDT 是 x86 段机制的遗留表"
        附录展开细节，标题用疑问式："x86 为什么还保留 GDT"
```

**判定**：架构特有遗留机制在 Ch1 占核心篇幅 → P2。

### 模式 56：决策日志体 Ch3（P1）

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

### 模式 57：例子选择有"读者前置知识泄漏"（P2）

```markdown
❌ 错误：概念引入示例引入与核心机制无关的细节
        讲"保护"时用 `*(0xffffffff80000000)=0` 作为例子
        ← 读者会问"为什么这地址是内核地址"（与保护机制无关）

✅ 正确：示例最小化
        讲"保护"时用 `*((int*)0)=0`
        ← 读者只关心"为什么空指针写失败"
```

**判定**：示例引入额外问题 → P2；本章开门第一个例子就有此问题 → P1。

### 模式 58：跨文档阶段状态表漂移（P1）

```markdown
❌ 错误：文档含 §N 实施状态表（"Phase 1: 待实施 / Phase 2: 待实施 / Phase 3: 待实施"），
        但实际代码已经实现多个阶段，状态表长期未同步。

        → reader 误判项目成熟度，文档与代码事实不一致。

✅ 正确：每个阶段完成时，原子提交中同步更新 §N 实施状态表
        "Phase 1: ✅ 已完成（见 file.rs:N）+ 实现要点"
        "Phase 2: ✅ 已完成（见 file.rs:M）+ 实现要点"
        "Phase 3: 🚧 进行中（待 issue #N）"
```

**判定**：文档含阶段状态表但与代码实现进度不符 → P1；状态表整体伪造或全部 "待实施" 但代码已实现 → P0。
**自动检测**：`tools/review-state-validate.py` 应读取 doc §N 阶段状态表 + grep 代码 TODO/FIXME/impl 标记，输出不一致列表。
**来源案例**：`04-platform-discovery.md:595-599` §13 阶段表原写全部"待实施"，但 `os/libs/minix-platform/src/{device_tree.rs, acpi.rs}` 已实现（已在后续修复）。

### 模式 59：文档字段计数漂移（P1）

```markdown
❌ 错误：文档 §X 写"Rust 保留 9 字段"，代码 struct 实际有 12 字段；或
        文档代码示例只展示 8 个字段，但 struct 公有字段共 12 个。

        → reader 凭文档写代码时会漏掉 4 个字段；doc-code 一致性破缺。

✅ 正确：struct 字段增/删/恢复时，doc 字段计数与代码示例必须同一 commit 更新
        "Rust 保留 12 字段（见 file.rs:N-M）"
        代码示例展示全部字段
```

**判定**：doc §X 字段计数 ≠ 实际 struct 公有字段数；或 doc 代码示例遗漏字段 → P1。
**三类漂移**：
1. 字段新增未文档化（最常见，~70%）
2. 字段保留但 doc 误标"已删除"（~20%）
3. doc 代码示例只展示字段子集（~10%）

**自动检测建议**（未来实施）：`tools/doc-freshness-check.sh` 对比 `rg "^pub " struct_file.rs` 与 doc §X 字段计数。
**来源案例**：`01-boot-shim-bootstrap.md` §3.5 原写"9 字段"，`os/libs/minix-boot/src/kernel_info.rs` 实际 12 字段（已在后续修复）。

### 模式 60：诚实显式 TODO 模式（P1，推广现有最佳实践）

```markdown
❌ 错误：TODO 静默遗漏
        // TODO: integrate later
        // 文档也没提到此处未实现

        → 后续维护者不知此处未完成，盲信代码"看起来对"。

✅ 正确：TODO 显式 + 双向标记
        // TODO(P0): Wire BootProcArch::load_vm_elf() into init_proc_and_boot()
        //           for VM process — see doc §4.5 L1133-1138.
        //           Current: placeholder (VirBytes(0), VirBytes(0), VirBytes(0)).
        //           Fix: let vm_load = CurrentBootProcArch::load_vm_elf(...);

        + 文档 §4.5 同步标 TODO + 原因 + 预期修复路径

        四要素: (a) file:line 引用 (b) 严重度 P0/P1/P2 (c) 一行解释 (d) doc 章节引用
```

**判定**：TODO 注释缺少 file:line/严重度/解释/doc 引用任一项 → P1；TODO 与 doc 完全无对应 → P0（违反 Claims-Evidence）。
**推广原因**：Doc 06 §4.5 的 4 个 TODO（1 P0 + 2 P1 + 1 P2）是金标准实践，已成可复用模板。
**反例**：`// TODO` + `// fix later` 等无四要素注释 → P1。
**来源案例**：`06-proc-init-boot-proc.md:1133-1170` 4 个显式 TODO + 代码注释双向同步。

---

> **编号说明**：模式 61-62 在演进过程中合并至模式 60（诚实显式 TODO），编号保留不补，以便历史 review 报告中的"模式 61/62"引用可追溯。模式 66 因 2026-07-16 新增时插入位置靠前，未按数字顺序排列——模式按编号检索，不影响功能。

### 模式 66: 参考代码路径漂移（Reference Code Path Drift, RCPD）（NEW 2026-07-16）

```markdown
❌ 错误：TODO/issue 描述引用 file:line，但被审时期存在、review 时已不存在
        TODO-04-2 引用 `os/arch/src/{x86_64,riscv64,arm64}/proc_arch.rs:350/252/279`
        实际：`06-design-final.md` 删除了 `proc_arch` 模块
        → 多次 AI bagging 共识"P0 真实 bug"基于已删除路径，T2 严重度从 P0 降为 P1

✅ 正确：TODO 验证 Step 0.7.1 path existence validation
        1. grep `ls <file>` 或 `rg "fn <name>" <file>` 验证路径存在
        2. 不存在 → 标"路径失效" + 触发 doc 重写而非 code 修复
        3. 多 AI 共识必须满足"全共识 × 全部 path 存在验证通过"才采纳为真实 bug
```

**判定**：
- TODO 描述引用已删除/重构路径导致虚假 P0 共识 → **P1 误报来源**（T2 案例：P0 → P1 降级）
- AI/人类 review 基于"过时路径"误判问题严重度 → 同上
- review 阶段对引用路径未做 ls/rg 验证 → 模式 66 触发

**规则草案**：
- (a) **TODO 描述必须有"path existence check"步骤**（Step 0.7.1 强制）
- (b) **review 阶段对每条 TODO 引用的 file:line 执行 `ls -la` 或 `rg -l` 验证**
- (c) **验证不通过** → 标"路径失效" + 触发 doc 重写而非 code 修复
- (d) **多 AI 共识**必须经过 grep/Read 验证才采纳为真实 bug
- (e) **任何"严重度降级"必须有 L1 证据**（`ls`/`rg` 命中或失效证明）

**建议落地**：
- [review-process.md §Step 0.7.1](../review-rules/review-process.md) 新增 path existence validation 子步骤
- 工具：`tools/todo-reference-validate.sh` 一键扫描所有 TODO file:line（未来实施）
- Session #8+ 每个 review 必跑 Step 0.7.1

**来源案例**：
- 04-platform-discovery §11 (Session #8)：`0108-todo-final.md` L601-622 (TODO-04-2) 引用 `os/arch/src/{x86_64,riscv64,arm64}/proc_arch.rs:350/252/279` 三处，实际 `os/arch/src/arch/mod.rs:20` 已明确 "proc_arch was removed in 06-design-final.md"——T2 P0 严重缺陷降级为 P1 事实纠正（2 AI 共识但共识基础已删除）
- I2 案例（2026-07-16 VERIFY-CHECK 暴露）：doc L416 引用 `global.rs:38-51`，实际 `PLATFORM` 静态在 L44——行号漂移 +6 行，P2 偏差（不影响行为但违反 file:line 精确标准）

**与现有模式的关系**：
- 模式 60（诚实显式 TODO）：互补——模式 60 关注 TODO 注释的 4 要素；模式 66 关注 TODO 描述引用的代码路径真实性
- 模式 5（代码与文档不一致）：范围更窄——仅 doc-code；模式 66 涵盖 TODO 描述中所有 file:line 引用

---

### 模式 63: Design-Missing 反模式

> **背景**：在 Minix-RS Rust 重写场景下，design 本身是核心交付物。如果 review 流程只发现实现问题而漏掉 design 缺失，会反复局部修复却始终漏掉核心概念。

**定义**：review 流程只发现实现问题，不发现 design 缺失，导致反复局部修复却始终漏掉核心概念。

**触发场景**：
- 设计文档（design.md/design-final.md）不存在或 outdated
- review 输出大量 P0-fact / P0-code-bug，但 P0 数量持续不收敛
- 反复修复同一类问题（如文档结构反复调整）

**典型症状**：
- "为什么我修了又出现新问题？"——因为 design 缺失，每次修复都是局部的
- "为什么 Ch1 和 Ch3 对不上？"——因为 Ch1 漏了某个概念，design 未定义
- "为什么 06 文档一直不满意？"——因为 design 未抓到三类运行态实体的本质区分

**判定信号**：
- 单文档 review 中发现 ≥3 处"概念缺失"类问题
- 多次 review 循环中同一模块的 P0 数量不减反增
- review 输出中出现"design 未定义"标注 ≥2 次

**修复路径**：
1. 中断 review（IN_DESIGN 状态）
2. 生成缺失的 design（用文档重写工作流 Step 1-3，参见 [review-process.md §〇 设计优先模式](review-process.md#设计优先模式design-first)）
3. design 通过 Gate H 后恢复 review
4. 此时 review 应能收敛

**教训**：**review 不能替代 design**。先 design 后 review 是 Rust 重写场景的基本要求。

> **对应 P0 类型**：P0-design-missing / P0-design-wrong（参见 [review.md §4.1 P0 六分类](review.md#41-p0-六分类含-p0-test-missing--明确-refactor-类型)）

---

### 模式 64: 开发文档味反模式

**定义**：文档使用"旧版→新版"、"最初→后来"、"我们改成"等迭代叙事，违反"教学材料"原则。

**触发场景**：
- 文档中含"旧版"、"最初"、"后来"、"我们改成"等关键词
- 文档讲述开发过程而非机制本质
- 文档使用"已实现"、"待实现"、"未完成"等状态标记

**判定信号**（grep 自动检测）：
```bash
# 禁用词清单（命中即 P1）
grep -nE "旧版|最初|后来|我们改成|已实现|待实现|未完成|TODO|FIXME" \
    prompt/../doc.md
```

**典型症状**：
- "我们最初用 X 实现，后来发现 Y，改成了 Z"（开发文档味）
- "该函数已实现"（应改为确定性机制说明）
- "下一步会实现"（应改为设计意图说明）

**修复路径**：
1. 删除"旧版→新版"叙事
2. 删除"我们改成"等开发主体
3. 删除"已实现/待实现"等状态标记
4. 用"如果 X 设计会有 Y 问题，所以用 Z"替代
5. 用确定性机制说明替代过程性描述

**教训**：**文档是教学材料，不是开发迭代记录**。读者关心知识点，不关心开发过程。

### 模式 64 扩展：禁用词清单

> **编号说明**：本小节为模式 64 的扩展（禁用词清单），复用编号 64 而非新开编号，以便与模式 64 本体保持一致引用。

| 分组 | 禁用词 | 替代模式 |
|------|--------|---------|
| **迭代叙事** | 旧版/最初/后来/我们改成/过去/当年/当初/之前/改成/替代/替换/替换为/升级/废弃/不再/放弃/修正 | 删除或转"如果 X 设计会有 Y 问题，所以用 Z" |
| **开发主体** | 我/我们/本项目/当前/今后/接下来 | 改为客观描述（"代码"、"文档"）|
| **过程性动词** | 实现/完成/添加/删除/重构（作为现状描述时）| 改为确定性机制说明 |
| **步骤标记** | 第一步/第二步/首先/然后/接着/最后 | 删除（用结构表达）|
| **状态标记** | 已实现/待实现/未完成/TODO/FIXME/XXX/NOTE | 删除或迁移到 design.md（非 bagging）/ design-final.md（bagging）|

**grep 自动检测（增强版）**：
```bash
grep -nE "旧版|最初|后来|我们改成|已实现|待实现|未完成|TODO|FIXME|过去|当年|当初|之前|改成|替代|替换|替换为|升级|废弃|不再|放弃|修正|我|我们|本项目|今后|接下来|添加|删除|重构|第一步|第二步|首先|然后|接着|最后|XXX|NOTE" \
    prompt/../doc.md
```

> **注意**：TODO 在代码中允许保留（标记待做事项），但禁止在文档正文中使用 TODO/已实现/待实现（与模式 15 区分）。

---

### 模式 65: Translate 倾向反模式

> 教训回顾：——"rewrite 不是 translate"。

**定义**：Rust 代码是 C 代码的 1:1 翻译，没有用 Rust 类型系统重新表达。

**触发场景**：
- Rust 函数签名与 C 函数高度相似（命名、参数顺序）
- 用 `i32`/`u32` 替代 C 的 `int`/`unsigned int`，但语义未变（应改用 newtype）
- 用 `Vec<T>` 替代 C 的动态数组，但生命周期管理未利用 Rust 借用
- C 用宏/全局变量，Rust 也用宏/`static mut`（应改用 trait/const）
- C 用函数指针数组做分派（如 call_vec），Rust 也用 `fn(...)` 数组（应改用 enum + match）

**判定信号**（grep 自动检测）：
```bash
# 反模式信号 1：函数签名与 C 高度相似
# 对比 C 源码和 Rust 实现，函数命名/参数顺序一致
diff <(grep "^void\|^int\|^static.*(" *.c) <(grep "fn " *.rs)

# 反模式信号 2：static mut 出现
grep -nE "static mut" prompt/../code.rs

# 反模式信号 3：函数指针数组
grep -nE "fn\([^)]*\)\s*\[.*\]" prompt/../code.rs

# 反模式信号 4：unsafe 块密度过高
grep -c "unsafe" prompt/../code.rs
# 期望：每 100 行代码 ≤ 1 个 unsafe
```

**典型症状**：
- "直接翻译 Minix3 的 `call_vec[]` 函数指针数组"——应改用 `enum Syscall + match`
- "用 `PhysBytes(u64)` 替代 C 的 `phys_bytes` typedef"——正确做法
- "用 `PhysBytes(u64)` 直接当 `u64` 用"——这是 translate，缺少类型语义
- "保留 `freepdes[2]` 全局数组"——应改用 typestate 表达"已分配"状态

**修复路径**：
1. 删除与 C 高度相似的函数命名（C 是 `do_fork` → Rust 不应也是 `do_fork`，应改为 `Process::fork`）
2. 将 int/uint 改为 newtype（带语义的不透明类型）
3. 将函数指针数组改为 enum + match（利用 Rust 穷尽检查）
4. 将 `static mut` 全局变量改为 `const` + 受限访问
5. 将 `unsafe` 块压缩到最小（除非必需，否则用 safe Rust 重新表达）

**教训**：**Rust 重写的价值在于类型系统，不是语法转换**。如果代码看起来像 C-with-semicolons，那是一次失败的 rewrite。

> **配套机制**：[review.md §核心术语 Rewrite](review.md) 定义目标、"禁止 Translate"是基本约束、模式 16/17 也是反 Translate 味道的检查项。

---

### 模式 67: C 函数名 vs OS 概念误判（C Function Name vs OS Concept Confusion, CFNOC）（NEW 2026-07-16）

```markdown
❌ 错误：AI 把 C 函数名（如 `init_clock()`）误读为 OS 概念对象（如 "CLOCK task"）
        ds (P1-4) + seed (TODO-006) 提议："05 文档提到 CLOCK task 时，读者可能不理解其与普通进程的区别"
        实际：05 文档讨论的是 `init_clock()` C 函数（initialize clock），不是 "CLOCK task" 内核子系统
        → 2 个 AI 共识产生 P1 误报

✅ 正确：AI claim grep verification（Step 0.7.2 强制）
        1. AI 报告中"X task" / "X subsystem" 类描述必须 `rg "X"` 验证存在
        2. C 函数名 `xxx_init()` / `init_xxx()` 永远表示"对 xxx 的初始化动作"，不是 xxx 本身
        3. C 全大写宏 `CLOCK_TASK` / `TASK_CLOCK` / `SYSTEM` 才表示任务类型常量
        4. 任务作为概念对象在 Minix3 中由 `proc[NR_TASKS+N] / proc_ptr` 表示
```

**判定**：
- AI 把"操作名"（`init_clock()`）错认为"对象名"（"CLOCK task"）→ **❌ 误报**（前提错误）
- AI 报告中"X task" / "X subsystem" 但 `rg "X"` 0 命中 → 模式 67 触发

**规则草案**：
- (a) **AI 报告"X task/subsystem" 类描述必须 `rg "X"` 验证存在**（Step 0.7.2 强制）
- (b) **C 函数名 `xxx_init()` / `init_xxx()`** 永远表示"对 xxx 的初始化动作"
- (c) **C 全大写宏 `CLOCK_TASK` / `TASK_CLOCK` / `SYSTEM`** 才表示任务类型常量
- (d) **任务作为概念对象**在 Minix3 中由 `proc[NR_TASKS+N] / proc_ptr` 表示
- (e) **多 AI 共识 + 概念对象关键词**必须经过 grep 验证才采纳

**建议落地**：
- [review-process.md §Step 0.7.2](../review-rules/review-process.md) 新增 AI claim grep verification 子步骤
- 工具：`tools/ai-claim-verify.sh` 一键扫描所有 "X task/subsystem" 类 AI claim（未来实施）

**来源案例**：
- 05-clock-interrupt-init Session #10：`tmp_design_and_todo/0108-todo-final.md` L680 (TODO-05-1) 由 ds (P1-4) + seed (TODO-006) 提议："05 文档提到 CLOCK task 时，读者可能不理解其与普通进程的区别"；`rg "CLOCK task\|clock task\|System Task\|Kernel Subsystem" 05-clock-interrupt-init.md` 0 命中 → 误报

**与现有模式的关系**：
- 模式 66 (RCPD)：关注 TODO 描述引用的代码路径真实性
- 模式 67 (CFNOC)：关注 AI 报告引用的概念对象真实性
- 两者都是"AI claim verification"类模式，但 66 验证路径，67 验证概念

---

### 模式 68: Ch2 C 源码展示 ≠ Ch4 Rust 实现误判（Doc Section Confusion, DSC）（NEW 2026-07-16）

```markdown
❌ 错误：AI 看到 Ch2（C 源码展示章节）含 `#ifdef` 即报告"Rust 代码散落分支"
        m3 (TODO #13) 提议："05 §2.4/§2.5 arch_init 仍有 30+ 行 ifndef CONFIG_SMP 散落分支"
        实际：Ch2 是 C 展示章节，应保留 C 预处理指令；Rust 已用 trait 静态分派隔离
        → 1 个 AI 提议产生 P2 误报

✅ 正确：Doc chapter context awareness（Step 0.7.3 强制）
        1. AI 报告 `#ifdef` 散落时必须区分：Ch2 C 展示（正确）/ Ch4 Rust 实现（可能问题）
        2. 报告前必须 `rg "os/**/*.rs"` 验证 Rust 实现是否真的有 `#[cfg(...)]` / `ifndef`
        3. 若 Rust 实现已用 trait 隔离，文档展示 C 是为了对比 Rust 的清晰度，不构成缺陷
```

**判定**：
- AI 把 Ch2 C 源码展示误认为 Ch4 Rust 实现问题 → **❌ 误报**（章节上下文错位）
- AI 报告 `#ifdef` / `#ifndef` 散落但 `rg "os/"` 验证无 `#[cfg(...)]` 分支 → 模式 68 触发

**规则草案**：
- (a) **AI 报告 `#ifdef` 散落必须区分章节**：Ch2 C 展示（正确）/ Ch4 Rust 实现（可能问题）
- (b) **报告前必须 `rg "os/**/*.rs"` 验证 Rust 实现**：是否有 `#[cfg(...)]` / `ifndef`
- (c) **若 Rust 实现已用 trait 隔离**（如 `ArchInit`、`ClockArch`），文档展示 C 是为了对比 Rust 的清晰度，**不构成缺陷**
- (d) **章节上下文强制检查**（Step 0.7.3）：AI 报告必须明确"这是 Ch2 C 展示问题还是 Ch4 Rust 实现问题"

**建议落地**：
- [review-process.md §Step 0.7.3](../review-rules/review-process.md) 新增 doc chapter context awareness 子步骤
- 工具：`tools/doc-chapter-classify.sh` 自动识别 doc 章节类型（Ch1/Ch2/Ch3/Ch4/Ch5）（未来实施）

**来源案例**：
- 05-clock-interrupt-init Session #10：`tmp_design_and_todo/0108-todo-final.md` L702 (TODO-05-2) 由 m3 (TODO #13) 提议："05 §2.4/§2.5 arch_init 仍有 30+ 行 ifndef CONFIG_SMP 散落分支"；`rg "ifndef CONFIG_SMP\|ifdef CONFIG_SMP\|#\[cfg\(smp\|CONFIG_SMP" os/` 仅 3 注释提及，Rust impl 无 `#[cfg(smp)]` 分支 → 误报

**与现有模式的关系**：
- 模式 66 (RCPD)：关注 TODO 描述引用的代码路径真实性
- 模式 67 (CFNOC)：关注 AI 报告引用的概念对象真实性
- 模式 68 (DSC)：关注 AI 报告引用的章节上下文真实性

### 模式 69: 缺失 per-doc 快照（Per-doc Snapshot Missing, PSMD）（NEW 2026-07-16）

```markdown
❌ 错误：AI 进入 Step 0 后以"已有 CONVERGED 状态"/"incremental review"/"复用 03/04 design"等理由
        跳过 design + outline 预检（ls {NN}-outline.v*.md / {NN}-design.v*.md）
        结果：scan.md Block Gates 表把 Gate H 标 "N/A" / "[SIMPLIFIED]"，但实际：
        - 06-proc-init-boot-proc.md 自 R2 (2026-06-23) CONVERGED 后从未生成 {06}-outline.v*.md 或 {06}-design.v*.md
        - 04/05 类似（Session #11 复盘发现）
        - 错误标"CONVERGED"但流程违反 review-process.md §Step 0 强制规则

✅ 正确：硬阻断 + Step 0 预检表 + 工具支持
        1. Step 0 启动时**必须**跑 4 条 ls 命令，结果写入 scan.md `§Step 0 预检结果` 段
        2. 缺失判定 + 嵌入生成（2026-07-17）：
           - outline.v*.md 缺失 → Gate H.6 FAIL → Step 0.3.2 嵌入生成
           - outline-review.v*.md 缺失 → Gate H.6 FAIL → Step 0.3.3 嵌入生成（AI 自审）
           - design.v*.md 缺失 → Gate H.1 FAIL → Step 0.3.4 嵌入生成
         3. 工具 `tools/design-coverage-check.sh {module}` 自动扫描所有 stage 的 .design/，输出缺失报告
         4. STATE.md Resume Point 必含 `ls .design/` 命令（避免续 session 跳过）
```

**判定**：
- AI 跳过 `ls .design/{NN}-*.md` 命令 → 模式 69 触发
- scan.md Block Gates 把 Gate H 标 "N/A" / "[SIMPLIFIED]" 而非 PASS/FAIL → 模式 69 触发
- 用"已有 CONVERGED 状态"/"incremental review"为由跳过预检 → 模式 69 触发（P0-process-violation）
- 用"复用 03/04 design"为由跳过预检 → 模式 69 触发（P0-process-violation，禁止跨文档复用）

**规则草案**：
- (a) **所有 review 模式强制预检**：Step 0 启动时**必须**跑 4 条 `ls` 命令
- (b) **缺失即 FAIL**：无任何"复用"或"豁免"借口（除用户显式一次性豁免）
- (c) **存在旧快照也必须重新评估**：v2 快照语义要求每次 review 重新执行 Step 0.3 产出 `.v{N+1}.md`
- (d) **scan.md 必须含 `§Step 0 预检结果` 段**：不可省略
- (e) **决策记录豁免仅一次性**：Session #11 用户决策"04/05 不回填"仅适用当时已 CONVERGED 的 04/05，不可泛化

**建议落地**：
- [review-process.md §Step 0 硬阻断规则](../review-rules/review-process.md) NEW 2026-07-16
- 工具：`tools/design-coverage-check.sh {module}`（Session #12 落地）
- 工具：`tools/todo-staleness-check.sh {todo-file}`（NEW，模式 70 配套）

**来源案例**：
- **Session #12 (06-proc-init-boot-proc.md)**：R12 review 时发现 `.design/` 目录只有 01/02/03 快照，06 完全没有；Session #11 模式 69 发现 04/05 缺失时已记录此为 P0-process-violation，但缺少硬阻断机制导致 Session #12 仍跳过预检。
- **Session #11 (04/05 复盘)**：模式 69 首次发现 — 04/05 自 CONVERGED 后从未生成 {04/05}-outline.v*.md / {04/05}-design.v*.md。

**与现有模式的关系**：
- 模式 66 (RCPD)：关注 TODO 描述引用的代码路径真实性
- 模式 67 (CFNOC)：关注 AI 报告引用的概念对象真实性
- 模式 68 (DSC)：关注 AI 报告引用的章节上下文真实性
- **模式 69 (PSMD)**：关注 review 流程本身的元数据完整性（快照存在性 + 跨文档独立性）

### 模式 70: 跨轮状态陈旧（Cross-Turn Outdated Staleness, CTOS）（NEW 2026-07-16）

```markdown
❌ 错误：AI 拿到 `tmp_design_and_todo/0108-todo-final.md` 后直接采纳所有 TODO 为真
        结果：8 个 TODO-06 中 3 个 (37.5%) 是误报，主要原因为 TODO 列表跨多轮 review 累积：
        - TODO-06-2 假设"1 个 trait 而非 3 个"，但实际 Rust 已有 3 个 trait（前提错误）
        - TODO-06-4 假设"TODO-01-3 阻塞"，但 TODO-01-3 早已修复接通（前提失效）
        - TODO-06-7 假设"需 3 trait"，但 Rust 已正确实现 3 trait（已修复）
        浪费 ~30 分钟在误报排查 + 二次 grep

✅ 正确：TODO staleness check（Step 0.7.4 强制）
        1. 对每个 TODO 描述中的"基于状态"前提（如"X 阻塞"/"Y 未实现"），用 `rg` 验证当前状态
        2. 前提失效 → 标"前提失效，TODO 不适用" + 严重度自动降级（与模式 66 RCPD 同规则）
        3. 批量模式：`tools/todo-staleness-check.sh {todo-file}` 自动扫描
```

**判定**：
- `tmp_design_and_todo/` 中 TODO 数 > 5 + 含"基于..."/"依赖..."/"待..."等时间敏感词 → **必须跑** Step 0.7.4
- TODO 描述引用"TODO-XX-N 未修复"但当前 rg 0 命中 → 前提失效，标"前提失效"
- TODO 描述引用"待 X 完成"但当前 X 已存在 → 前提失效，标"前提失效"
- 未跑 staleness check 即采纳全部 TODO 为真 → 模式 70 触发

**规则草案**：
- (a) **触发条件**：TODO 数 > 5 + 含时间敏感词 → 必须跑 Step 0.7.4
- (b) **前提验证**：每个 TODO 的"基于状态"前提必须用 `rg` 验证当前状态
- (c) **严重度降级**：P0 + 前提失效 → P1，P1 + 前提失效 → P2，P2 + 前提失效 → 误报
- (d) **批量工具**：`tools/todo-staleness-check.sh {todo-file}` 自动输出 staleness 报告
- (e) **TODO 强制格式**：`[P0/P1/P2] [code/doc] [factual/design] 问题描述 (file:line) (基于状态) (修复方案)`，缺字段 → TODO 不可信

**建议落地**：
- [review-process.md §Step 0.7.4 TODO Staleness Check](../review-rules/review-process.md) NEW 2026-07-16
- 工具：`tools/todo-staleness-check.sh {todo-file}`（已落地 2026-07-17）

**来源案例**：
- **Session #12 (06-proc-init-boot-proc.md)**：`tmp_design_and_todo/0108-todo-final.md` 中 8 个 TODO-06 经 staleness check 后：
  - 3 误报（前提错误/失效/已修复）
  - 5 真实，其中 1 项已正确实现无需修改
  - 实际修复 4 项，节省 ~30 分钟
- 历史均值：Session #5-#11 TODO 误报率约 25%（5/20），Session #12 升至 37.5%（3/8）反映 TODO 列表跨轮累积问题加剧。

**与现有模式的关系**：
- 模式 66 (RCPD)：关注 TODO 描述引用的**代码路径**真实性
- 模式 67 (CFNOC)：关注 AI 报告引用的**概念对象**真实性
- 模式 68 (DSC)：关注 AI 报告引用的**章节上下文**真实性
- 模式 69 (PSMD)：关注 review **流程元数据**完整性（快照存在性）
- **模式 70 (CTOS)**：关注 TODO 列表**跨轮时间一致性**（前提失效检测）
- **66 + 70 互补**：66 关注"代码路径在不在"，70 关注"前提状态对不对"

### 模式 71: 决策泛化误用（Decision Over-Generalization, DOG）（NEW 2026-07-16）

```markdown
❌ 错误：AI 在 Session #11 收到用户决策"04/05 不回填 design 快照"，
        Session #12 启动时**错误泛化**为"06 也有 CONVERGED 状态 → 也跳过 design 预检"
        实际：用户决策仅适用于 04/05，06 是新 review 文档，**不能泛化**
        结果：Session #12 跳过 design 预检 → 06 缺快照但 scan.md 标 CONVERGED → 模式 69 PSMD 触发

✅ 正确：决策记录豁免的强约束
        1. 用户决策"X 不回填"仅适用于当时已 CONVERGED 的 X，不可泛化到后续 review 的其他文档
        2. 任何"已有 CONVERGED 状态"豁免必须满足：
           a) 用户当时显式说"该 doc 豁免"
           b) 豁免仅对该 doc 有效
           c) 豁免记录在 STATE.md `§豁免列表` 段
        3. AI 不得自行泛化用户决策；如认为某文档需要类似豁免，必须先询问用户
```

**判定**：
- AI 把"X 文档豁免"泛化到"Y 文档" → 模式 71 触发
- 没有任何豁免记录在 STATE.md，但 AI 声称"已有 CONVERGED 状态" → 模式 71 触发
- 用户决策仅指明"X"，但 AI 把范围扩大到"X+Y+Z" → 模式 71 触发

**规则草案**：
- (a) **决策仅适用当时对象**：用户决策"X 不回填"仅对 X 有效，不可泛化
- (b) **豁免必须显式记录**：豁免必须在 STATE.md `§豁免列表` 段登记
- (c) **泛化必须询问**：AI 认为需要类似豁免时，必须先询问用户
- (d) **"CONVERGED 状态"不是豁免依据**：CONVERGED 是结果状态，不是豁免资格

**来源案例**：
- **Session #12**：用户 Session #11 决策"04/05 不回填"被错误泛化到 06，导致 06 缺 design 快照但仍标 CONVERGED。

**与现有模式的关系**：
- 模式 69 (PSMD)：关注 review **流程元数据**完整性
- **模式 71 (DOG)**：关注 review **决策范围约束**（泛化误用）
- **69 + 71 互补**：69 关注"硬阻断是否被执行"，71 关注"豁免决策是否被正确理解"
- 三者构成"AI claim verification 三元组"：路径 + 概念 + 章节

### 模式 72: 跨章节步骤数不一致（Cross-Section Step Count Mismatch, CSSCM）（NEW 2026-07-17）

```markdown
❌ 错误：文档 §2 C 源码分析列出 bsp_finish_booting 共 12 步（cpu_identify / vm_running=0 /
        krandom_init / bill_ptr=proc_ptr=idle / announce / RTS_UNSET / cycles_accounting_init /
        boot_cpu_init_timer / fpu_init / cpu_set_flag / kernel_may_alloc=0 / switch_to_user），
        §4 Rust 实现章节只列 9 步，**未明确说明**为何 3 步缺失。
        读者看到 §2 列 12 步、§4 实现 9 步，无法判断：
        - 是 Rust 实现遗漏（P0 bug）？
        - 是 C 步骤在 Rust 中已合并到其他步骤（设计决策）？
        - 是 C 步骤在 64 位 Rust 中不需要（架构演进）？

✅ 正确：§4 Rust 实现章节末尾添加"与 C N 步的差异说明"表格
        | C 步骤 | C 位置 | 未实现原因 |
        |--------|--------|----------|
        | cpu_identify() | main.c:45 | CPU 识别在 boot-shim 阶段已完成，Rust 无需重复 |
        | krandom_init() | main.c:62 | Rust 尚未实现内核随机数源；后续安全模块实现时补齐 |
        | cpu_set_flag(bsp, CPU_IS_READY) | main.c:92 | Rust 用 CpuState::Ready 枚举表达，步骤 5 隐式完成 |
```

**判定**：
- §2 列出 N 步，§4 实现 M 步（M < N），且 §4 无差异说明表 → 模式 72 触发（P1）
- §2 列出 N 步，§4 实现 M 步（M > N），且 §4 无"Rust 新增步骤"说明 → 模式 72 触发（P1）
- §4 有差异说明但原因空泛（如"Rust 不需要"、"架构差异"无具体说明） → 模式 72 触发（P1）
- §2 与 §4 步骤数一致但步骤顺序重排且未说明 → 模式 72 触发（P2）

**规则草案**：
- (a) **步骤数对齐**：文档 §2 C 源码分析列出的步骤数必须与 §4 Rust 实现的步骤数一致；若不一致，必须在 §4 末尾添加"与 C N 步的差异说明"表格
- (b) **差异分类**：每条差异必须归类为以下之一：
  - **架构演进**（64 位 Rust 不需要，如 4GB 截断）
  - **设计决策**（合并到其他步骤，如 `cpu_set_flag` 由枚举状态转换表达）
  - **已知缺口**（尚未实现，需 TODO 标注 + 计划补齐时间）
  - **C 源码 bug**（C 中多余/错误的步骤，Rust 正确省略）
- (c) **每条差异附 C 行号**：差异表必须包含 C 源码行号，便于读者定位验证
- (d) **不限于 §2/§4**：该规则适用于任何"分析章节 vs 实现章节"的步骤数对比（如 §2.x 子节 vs §4.x 子节）

**严重度**：P1（默认）/ P0（若未实现步骤涉及安全/并发/内存管理关键路径，且未标注"已知缺口"）

**来源案例**：
- **Session #20 (08 文档 full-review)**：§2.3 列 `bsp_finish_booting` 12 步，§4.6 实现 9 步，3 步缺失未说明。Review 发现后，§4.6 添加差异说明表（3 步分别归类为"架构演进/已知缺口/设计决策"）。

**与现有模式的关系**：
- 模式 11（设计与实现脱节）：关注 design.md 与 code.rs 的整体脱节
- 模式 60（诚实显式 TODO 模式）：关注未实现功能是否标注 TODO
- **模式 72 (CSSCM)**：关注**文档内部** §2 与 §4 的步骤数一致性（doc 内部一致性）
- **72 ≠ 11**：11 是 design↔code 跨制品；72 是 §2↔§4 同文档内跨章节
- **72 ≠ 60**：60 关注"是否标注 TODO"；72 关注"是否说明步骤数差异"（已实现步骤也可能触发 72，只要步骤数对不上）

**检查命令**：
```bash
# 提取 §2 步骤数（grep "步骤" 关键字 + 表格行）
rg -c "^\| \d+ \|" notes/rewrite/{module}/{stage}/{doc}.md
# 提取 §4 步骤数
rg -c "^\| \d+ \|" notes/rewrite/{module}/{stage}/{doc}.md
# 检查差异说明表是否存在
rg "与 C .* 步的差异说明|步骤数差异|未实现步骤" notes/rewrite/{module}/{stage}/{doc}.md
```

---

### 模式 73: 文档代码示例 Rust 2024 Edition Drift（NEW 2026-07-30）

**定义**：文档中的代码示例使用 Rust 2024 edition 已 deprecated 或非 idiomatic 的写法（特别是 `static mut`），但实际代码已迁移至 `Atomic*` / `UnsafeCell` / `AtomicBool` 等替代方案。导致 doc-code 示例不一致。

**触发场景**：
- 文档 §4 / §3 的代码示例含 `static mut`
- 实际代码已用 `AtomicU64` / `AtomicBool` / `UnsafeCell<T>` 替代
- 路径引用过时（如 `arch/src/pt_alloc.rs` → 实际 `arch/src/arch/pt_alloc.rs` 因目录重组）

**典型案例**：
```
❌ 文档代码示例（已过时）：
// arch/src/pt_alloc.rs
type PtAllocFn = fn() -> Result<...>;
static mut PT_ALLOC: PtAllocFn = uninit_alloc;

// hello-boot（调用者）
static mut BOOT_PT_NEXT: u64 = 0;
static mut BOOT_PT_END: u64 = 0;

✅ 实际代码（2026-07-30）：
// os/arch/src/arch/pt_alloc.rs
struct PtAllocSlot(UnsafeCell<PtAllocFn>);
unsafe impl Sync for PtAllocSlot {}
static PT_ALLOC: PtAllocSlot = PtAllocSlot(UnsafeCell::new(uninit_alloc));
static PT_REGISTERED: AtomicBool = AtomicBool::new(false);

// os/kernel/src/boot_alloc.rs
static BOOT_PT_NEXT: AtomicU64 = AtomicU64::new(0);
static BOOT_PT_END: AtomicU64 = AtomicU64::new(0);
```

**判定信号**：
- `rg "static mut" {doc}` 命中非"说明性注释"位置（即实际代码示例使用 `static mut`）
- `rg "static mut" {rust_dir} -t rust` 在 `pub fn` / 函数体内 → 0 hits（说明实际代码已迁移）
- 文档代码示例中的路径 `find` 不到，但变体路径存在（目录重组）

**严重度**：P1（默认）/ P0（若代码示例被复制作 boot-shim 模板，会导致新代码引入 Rust 2024 UB）

**规则草案**：
- (a) 文档代码示例必须反映**当前 idiomatic Rust 写法**（特别是 Rust 2024 edition 兼容性）
- (b) 涉及同步原语（`static` 全局状态）必须用 `Atomic*` 或 `UnsafeCell<T>` + 显式 `Sync` impl
- (c) 路径引用必须与实际仓库结构一致；目录重组后必须同步更新
- (d) 当代码迁移发生时（`git log` 显示 `static mut` → `Atomic*` 替换），doc 必须同步更新

**检查命令**：
```bash
# Doc-side 静态扫描
rg "static mut" notes/rewrite/{module}/{stage}/{doc}.md --type md

# Rust-side 实际状态（应 0 hits）
rg "static mut" os/ -t rust --type-add 'rust:*.rs'

# 路径一致性（doc 写 path1 vs 实际 path2）
rg "arch/src/(pt_alloc|paging\.rs|paging_ext)" notes/rewrite/{module}/{stage}/{doc}.md
find os/arch/src -name "pt_alloc.rs" -o -name "paging.rs" -o -name "paging_ext.rs"
```

**修复建议**（≤10 分钟）：
1. 复制实际代码到文档代码块（去除 `static mut`）
2. 加注释说明 `// Rust 2024 edition 兼容：用 UnsafeCell / AtomicBool 而非 static mut`
3. 路径错误时按 `find` 结果更新（注意 `/arch/` 子目录等重组）

**与已有模式区分**：
- 模式 5（代码与文档不一致）= 行为层面不一致
- 模式 59（文档字段计数漂移）= struct 字段计数错误
- 模式 73 = **Rust 习惯用法 / 路径同步漂移**（2024 edition 升级期特有）

**已知子类型**：
- 73a: `static mut` → `Atomic*` / `UnsafeCell`
- 73b: 路径目录重组（`X/` → `X/X/` 子目录化）
- 73c: API 签名小升级（如 `fetch_add` 回滚 → `compare_exchange`）

**首次发现**：2026-07-30 01-boot-shim-bootstrap review（`.review/claude/03-stage-kernel/01-boot-shim-bootstrap/scan.md`，Pattern #73）

---

### 模式 74: 文档路径约定漂移（Doc Path Convention Drift, NEW 2026-07-30）

**定义**：文档内 Rust crate 路径引用缺 `os/` workspace 根前缀，与 CLAUDE.md `os/` 目录约定不一致（典型错误：`kernel/src/...` 应为 `os/kernel/src/...`）。常因 doc 在 `os/Cargo.toml` workspace 之外撰写，作者直觉省略 workspace 根。

**触发场景**：
- doc 引用 `kernel/src/...`、`arch/src/...`、`boot-shim/src/...` 等裸路径
- 实际仓库布局为 `os/kernel/src/...`、`os/arch/src/...`、`os/boot-shim/src/...`
- 同一 stage 不同 doc 之间路径风格不一致（如 doc 01 用 `os/`，doc 02 漏 `os/`）

**典型案例**：
```
❌ Doc 02 引用（18 处）：`kernel/src/boot/higher_half.rs`、`kernel/src/lib.rs`、
   `kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld`
✅ 实际路径：`os/kernel/src/boot/higher_half.rs`、`os/kernel/src/lib.rs`、
   `os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld`
✅ 跨文档一致：doc 01 全部用 `os/xxx/src/...` 形式
```

**判定信号**：
- `rg "kernel/src/" {doc}.md | wc -l` > 0 但 `rg "os/kernel/src/" {doc}.md | wc -l` 低（典型 18 vs 2）
- 同一 stage 内多个 doc 的 `os/` 前缀使用率差异大
- 新写 doc 前未比对早期 doc 的路径风格

**严重度**：P1（默认）/ P2（仅个别遗漏）

**规则草案**：
- (a) **Rust crate 路径必须含 `os/` workspace 根前缀**（与 CLAUDE.md 目录布局约定一致）
- (b) **minix3 C 源路径用 `minix3/...` 前缀**（不带 `os/`，与 Rust 路径区分）
- (c) **跨文档统一路径约定**——新写 doc 前**必须**比对同一 stage 早期 doc 的路径风格
- (d) **避免双重前缀**：`os/os/...` 是 sed 批量替换的常见副作用，必须 rg 复查

**检查命令**：
```bash
# 路径约定检查（Step 1.0c NEW）
rg "kernel/src/|boot-shim/src/|arch/src/|servers/vm/|servers/pm/|servers/vfs/|servers/rs/" \
    notes/rewrite/{module}/{stage}/{doc}.md
# 应仅匹配 minix3/... 路径或 0 hits

# 反向验证：os/ 前缀正确性
rg "os/(kernel|boot-shim|arch|servers|libs)" notes/rewrite/{module}/{stage}/{doc}.md | wc -l
# 应与该 doc 的 Rust 路径总数接近

# 双重前缀检查
rg "os/os/" notes/rewrite/{module}/{stage}/{doc}.md
# 必须 0 hits
```

**修复建议**（≤10 分钟）：
1. 用 `sed` 批量加 `os/` 前缀：`sed -i 's|kernel/src/|os/kernel/src/|g' {doc}.md`
2. 复查双重前缀：`sed -i 's|os/os/|os/|g' {doc}.md`
3. 注意不要误改 minix3 C 源路径（`rg "minix3/.*kernel/src/" doc` 验证 C 源路径未受影响）
4. 建议在 stage 级别建立"path style baseline"——同一 stage 早期 doc 的路径风格作为参考

**与已有模式区分**：
- 模式 73a = doc code example Rust idiom drift（`static mut` → `Atomic*`）
- 模式 73b = **路径目录重组**（`X/` → `X/X/`，如 `arch/src/` → `arch/src/arch/`）
- 模式 73c = API 签名小升级
- **模式 74 = workspace 根路径省略**（doc 写作时漏写 `os/`）—— 与 73b 不同，73b 是目录重组，74 是路径**引用**漂移

**典型修复流程**（来自 02-higher-half-kernel review）：
1. Pre-fix rg 统计：`rg "kernel/src/" doc | wc -l = 18`
2. `sed -i 's|kernel/src/|os/kernel/src/|g' {doc}.md`
3. Post-fix rg 复查：`rg "os/os/" doc` 应为 0 hits（避免 sed 双重前缀副作用）
4. 最终验证：`rg "os/kernel/src/" doc | wc -l` 应 ≥ 18

**已知子类型**：
- 74a: 漏 `os/` workspace 根（本次 02 doc）
- 74b: 跨 doc 路径风格不一致（doc 01 用 `os/`，doc 02 不用）
- 74c: 双重前缀（`os/os/`，sed 副作用）

**首次发现**：2026-07-30 02-higher-half-kernel review（`.review/claude/03-stage-kernel/02-higher-half-kernel/scan.md`，Pattern #74）

### 模式 75: 文档"参见"范围漂移（Doc See-Also Range Drift, NEW 2026-07-31）

> **背景**：doc 中"参见 X.rs:Y-Z"形式引用（行号范围）容易出现两类漂移——起止行号偏移（如 :55-76 实际 :56-78）+ **上界范围过短**（如 :55-399 实际 :56-523）。前者是 +1 行号漂移，后者是 doc 写作时文件较小、未随代码演化更新。

**典型场景**：
```markdown
❌ 参见 `os/libs/minix-platform/src/device_tree.rs:55-399`（含 cfg-gated 字段定义、arch 特化解析函数、trait impl）
✅ 参见 `os/libs/minix-platform/src/device_tree.rs:56-423`（含 cfg-gated 字段定义、arch 特化解析函数、`impl PlatformDesc`）
```

**检查命令**（Step 1.0d 强制）：
```bash
# 1. 抽取"参见"型引用
rg -o "参见 \`[^\`]+\.rs:[0-9]+-[0-9]+\`" {doc}.md

# 2. 对每个范围验证上界是否覆盖到 impl 结束
# 例：device_tree.rs 需覆盖到 `impl PlatformDesc` 结束 + 测试开始
# acpi.rs 同理
rg -n "^impl PlatformDesc for DeviceTreeDesc|^impl fmt::Display" os/libs/minix-platform/src/device_tree.rs
# → impl PlatformDesc 起始行 + 下一个 item 起始行 = 范围上界
```

**判定**：
- 起止行号偏移 ±1 → **P2 行号偏移**
- 上界 < 实际 impl 结束 → **P2 范围过短**（doc 写作时文件较小，未随代码演化更新）
- 起止偏移 > 1 → **P1 行号漂移**

**修复**（≤5 分钟）：
```bash
# 1. 修正 +1 偏移
sed -i 's|device_tree.rs:55-|device_tree.rs:56-|g' {doc}.md

# 2. 更新上界到当前 impl 结束
#   需先用 rg 找出 `impl PlatformDesc for X` + 下一个 `impl`/`fn test_`/`fn parse` 的位置
#   然后 sed 替换
sed -i 's|device_tree.rs:55-399|device_tree.rs:56-423|g' {doc}.md

# 3. 验证
rg "device_tree.rs:" {doc}.md
wc -l os/libs/minix-platform/src/device_tree.rs  # 当前实际行数
```

**已知子类型**：
- 75a: 参见行号 +1 偏移（如 55 → 56）
- 75b: 参见范围上界过短（如 399 → 实际 523）
- 75c: 参见下界偏移（少见）

**与已有模式区分**：
- **模式 73 / 74** = 路径相关漂移
- **模式 75 = 行号 / 范围漂移**（与 Step 1.0a 主动抽样互补，但 Step 1.0a 倾向"单点行号"，75 倾向"范围引用"）

**典型漏检原因**：Step 1.0a 主动抽样只检查"`// path:line`"形式的代码注释引用，**漏检"参见 path:line-line"形式的范围引用**。本次 04 doc review 即因此漏检 2 处 L831/L883 范围漂移。

**首次发现**：2026-07-31 04-platform-discovery review（`.review/claude/03-stage-kernel/04-platform-discovery/scan.md`，Pattern #75）

### 模式 76: 跨文档归属漂移（Cross-Doc Attribution Drift, NEW 2026-07-31）

> **背景**：代码注释中"covered in NN" / "see XX-doc.md §Y"等**指向特定 doc 编号或文件名的引用**，容易因 (a) doc 编号重排 或 (b) doc 改名 而**系统性过时**。本次 05 doc review 发现 `os/kernel/src/lib.rs` 3 处 `(covered in NN)` 注释错位（实为 04/05/06 而非 05/06/07），同时发现**至少 9 处代码注释引用旧 doc 命名**（如 `04-clock-interrupt-init.md`，当前 04 实际是 `platform-discovery.md`）。

**典型场景**：
```rust
❌ // init_clock_and_interrupts();  // (covered in 04)   ← 实际由 05 实现
   // (covered in NN) 注释随 doc 编号重排而过时
   // "see 04-clock-interrupt-init.md" 注释引用旧 doc 文件名

✅ // 每次 doc 编号重排或改名时，跑 rg + sed 同步所有引用
   // 或：去掉 "covered in NN" 注释，改用语义化注释（"详见 init_clock_and_interrupts 实现"）
```

**已知子类型**：
- **76a** `(covered in NN)` 注释错位（本次 05 review 发现 3 处）
- **76b** `see XX-doc.md` 注释引用旧 doc 文件名（本次同时发现 9+ 处，如 `04-clock-interrupt-init.md` / `05-exception-interrupt.md` / `06-arch-post-init.md` 等已过时）
- **76c** 设计文档引用旧编号（design.md 中 `(详见 §3.2)` 等）

**检查命令**（Step 1.0e 强制）：
```bash
# 1. 扫描代码中所有 "covered in NN" / "see NN-doc.md" 引用
rg "covered in 0[0-9]|see 0[0-9]-.+\.md" os/ -t rust -n

# 2. 验证当前 doc 编号是否一致
ls notes/rewrite/{module}/{stage}/ | rg "^[0-9]+"

# 3. 对每个引用，验证是否指向有效 doc
for ref in $(rg "covered in 0[0-9]" os/ -t rust -o); do
    doc_num=$(echo "$ref" | rg -o "[0-9]+")
    doc_file=$(ls notes/.../0${doc_num}-*.md 2>/dev/null | head -1)
    echo "$ref → $doc_file"
done

# 4. 验证 "see XX-doc.md" 注释的文件名
for ref in $(rg "see [0-9]+-.+\.md" os/ -t rust -o); do
    doc_file=$(echo "$ref" | rg -o "[0-9]+-.+\.md")
    if [ ! -f "notes/.../$doc_file" ]; then
        echo "❌ STALE: $ref"
    fi
done
```

**判定**：
- 注释错位 (covered in NN 不一致) → **P1 注释错位**
- 注释引用已删除 doc → **P1 注释失效**
- 注释引用已重命名 doc → **P1 注释失效**

**修复**（批量 sed）：
```bash
# 1. 修 (covered in NN) 注释（按上下文判断目标 doc）
sed -i 's|init_clock_and_interrupts.*covered in 04|init_clock_and_interrupts (covered in 05|' os/kernel/src/lib.rs

# 2. 修 "see XX-doc.md" 注释引用（doc 改名后批量更新）
#   例如 04-clock-interrupt-init.md → 05-clock-interrupt-init.md（doc 04 重命名为 platform-discovery）
sed -i 's|04-clock-interrupt-init.md|05-clock-interrupt-init.md|g' os/arch/src/arch/{clock.rs,arch_init.rs}
sed -i 's|05-exception-interrupt.md|14-exception-interrupt.md|g' os/arch/src/arch/*.rs os/plat/src/interrupt.rs os/kernel/src/irq_manager.rs
sed -i 's|06-arch-post-init.md|08-system-init-boot-finish.md|g' os/arch/src/arch/post_init.rs

# 3. 验证
rg "covered in 0[0-9]|see 0[0-9]-.+\.md" os/ -t rust | wc -l  # 应等于 0
```

**与已有模式区分**：
- **Pattern #66** = Reference Code Path Drift（代码路径引用 `file:line` 不存在）
- **Pattern #76** = **Cross-Doc Attribution Drift**（doc 编号/文件名引用不一致）

**典型漏检原因**：之前 review 只检查 doc 内容 vs 代码内容，**未深入代码注释交叉引用**。本次 05 review 是首次系统检查，发现 Pattern #76 实质化（不只是 1-2 处，而是 9+ 处）。

**首次发现**：2026-07-31 05-clock-interrupt-init review（`.review/claude/03-stage-kernel/05-clock-interrupt-init/scan.md`，Pattern #76）

### 模式 77: 代码注释行号漂移（Code Comment Line Drift, CCLD, NEW 2026-07-31）

> **背景**：代码注释中引用的 `file:line`（如 `// see proc_table.rs:129`）可能因代码增量而**漂移**（文件行号下移）。doc 复述这些注释时，会产生**传递性 drift**（doc 错误根因在代码注释）。
>
> 本次 08 doc review 发现 3 处 P2 行号偏移全部源自 `os/kernel/src/lib.rs:1170/1181/1193` 的代码注释错误（不是 doc 错）：
> - `proc_table.rs:129` 实际 L276（rts_unset，+147 偏移，最大）
> - `smp.rs:127-132` 实际 L200（set_running，+73）
> - `smp.rs:80-145` 实际 L135（CpuLocal struct，+55）

**典型场景**：
```rust
// Rust: ProcessTable::rts_unset auto-enqueues a newly-runnable process
// (see proc_table.rs:129). Iterate from 0 ...
// ❌ 实际 rts_unset 在 L276（行号漂移 +147）

// Rust: CpuLocal::set_running(IDLE) — see smp.rs:127-132.
// ❌ 实际 set_running 在 L200（+73）
```

**检查命令**（Step 1.0f 强制）：
```bash
# 1. 扫描所有代码注释中的 file:line 引用
rg "see [a-z_/0-9]+\.rs:[0-9]+|see [a-z_/0-9]+\.rs:[0-9]+-[0-9]+" os/ -t rust -n

# 2. 对每个引用验证实际行号
for ref in $(rg "see [a-z_/0-9]+\.rs:[0-9]+" os/ -t rust -o); do
    file=$(echo "$ref" | rg -o "[a-z_/0-9]+\.rs")
    line=$(echo "$ref" | rg -o "[0-9]+")
    actual=$(rg -n "^pub fn|^fn |^pub struct|^pub enum|^impl|^pub trait" "$file" | awk -F: -v target=$line '($1 <= target)' | tail -1)
    echo "$ref → $actual"
done
```

**判定**：
- 引用行号 ±1 偏移 → ✅（允许小漂移）
- 引用行号偏差 > 5 → **P2 代码注释漂移**
- 引用行号偏差 > 50 → **P1 代码注释显著漂移**

**修复策略**（双修避免传递性 drift）：
```bash
# 1. 修代码注释（root cause）
sed -i 's|(see proc_table.rs:129)|(see proc_table.rs:276)|' os/kernel/src/lib.rs

# 2. 同步修所有复述的 doc（如果 doc 复述了错误注释）
sed -i 's|proc_table.rs:129|proc_table.rs:276|g' notes/.../{doc}.md

# 3. 验证全项目干净
rg "proc_table\.rs:129|smp\.rs:127-132|smp\.rs:80-145" os/ notes/
# (empty = ✅)
```

**与已有模式区分**：
- **Pattern #66** = Reference Code Path Drift（代码路径引用 `file:line` 不存在）—— Pattern #77 是 Pattern #66 的子类型（行号漂移）
- **Pattern #73** = Doc Code Example Rust 2024 Edition Drift（Rust idiom 漂移）
- **Pattern #75** = Doc See-Also Range Drift（doc 范围引用漂移）
- **Pattern #77** = **Code Comment Line Drift**（**代码注释行号漂移**）—— 重点是代码注释而非 doc 引用

**已知偏差**（本次 08 review 发现）：
- `os/kernel/src/lib.rs:1170` `smp.rs:80-145` → 实际 `smp.rs:135`（已修）
- `os/kernel/src/lib.rs:1181` `smp.rs:127-132` → 实际 `smp.rs:200`（已修）
- `os/kernel/src/lib.rs:1193` `proc_table.rs:129` → 实际 `proc_table.rs:276`（已修）

**首次发现**：2026-07-31 08-system-init-boot-finish review（`.review/claude/03-stage-kernel/08-system-init-boot-finish/scan.md`，Pattern #77）

### 模式 78：C 源码 bug 未显式标注（C Source Bug Unlabeled, CSBU）（NEW 2026-08-14）

**严重度**：P1

**问题描述**：Rust 实现修复了 Minix3 C 源码中的 bug，但代码注释和文档中未显式标注"C 源码此处有 bug，Rust 修复方式为 X"。后续维护者看到 Rust 与 C 行为不一致时，会以为是 Rust bug 而改回去，重新引入 C 的 bug。

**判定标准**：
- Rust 代码行为与 C 源码不一致，且原因是 C 源码有 bug（非设计差异）
- 代码注释中无 `// MINIX3 BUG:` 标注
- 文档 §2（语义对齐）对应位置无 bug 说明

**验证命令**：
```bash
# 查找 Rust 代码中修复了 C bug 的位置
rg "// MINIX3 BUG:" os/ --type rust -n
# 反向检查：Rust 与 C 行为差异处是否有标注
rg -A5 "C source.*bug\|minix3.*bug\|原版.*bug" os/ --type rust -i -n | head -20
```

**正确示例**：
```rust
// MINIX3 BUG: region.c:841-842 ignores ev_reference return value
// Rust fix: ev_copy returns Err(NotSupported) instead of silently ignoring
fn ev_copy(&self, ...) -> Result<...> { ... }
```

**错误示例**：
```rust
// Rust 版本直接返回错误，不忽略返回值
fn ev_copy(&self, ...) -> Result<...> { ... }
// ❌ 没有标注 C 源码 bug，维护者可能"修复"回 C 的行为
```

**来源案例**：
- `region.c:841-842` 忽略 `ev_reference` 返回值 → Rust `ev_copy` 返回 `Err(NotSupported)`
- `enter_queue` 写入 current 而非 target process → Rust 修复为 target
- `anon_pagefault()` 部分分支不释放 `alloc_mem()` → Rust 先检查再分配

**首次发现**：2026-08-14 规则集优化过程中，从 project_memory 沉淀的多个 C bug 修复案例抽象。

---

## 附录：Pattern→Step 交叉引用表

> **用途**：AI 在执行某 Step 时，快速找到该 Step 最相关的模式。非穷举——所有模式都可能在任何阶段触发。

| Step | 最相关模式 | 模式主题 |
|------|-----------|---------|
| Step 0（预检） | 66 RCPD, 67 CFNOC, 68 DSC, 69 PSMD, 70 CTOS, 71 DOG | 路径漂移/claim 未验/章节误判/快照缺失/TODO 陈旧/决策泛化 |
| Step 0.5（structure.md） | 1-15（文档错误）, A/B/C（跨文档） | 文档基础错误 + 跨文档联动 |
| Step 1（C 源码） | 5（代码与文档不一致）, 11（设计与实现脱节）, 78 CSBU | C-Rust 语义对齐 + C bug 标注 |
| Step 1.5（覆盖率） | — | 覆盖率由 coverage-extract.py 穷举，模式 1-15 兜底 |
| Step 2（Diff Extraction） | 16-20（代码质量模式）, 48（因果链编造） | Translate 味道/unsafe/errno + 因果链验证 |
| Step 2.5（链接验证） | — | 链接由 grep 验证，无专属模式 |
| Step 3（Sanity Check） | 21-25（语义对齐模式）, 26-29（Kernel SMP 并发）, 30（外部知识误导） | C 引用验证 + 概念准确性 + SMP 并发 |
| Step 3.5（Precision Check） | 30-34（精度模式）, 50（arch scope 未标注） | 精度抽样 + 架构范围 |
| Step 3.5a（纵向链路） | 51-53（链路模式） | 纵向链路完整性 |
| Step 3.5b（因果链） | 54-57（叙事模式） | 视角漂移/架构喧宾/决策日志/知识泄漏 |
| Step 4（跨文档联动） | 58（跨文档状态表漂移）, A/B/C（跨文档联动） | 跨文档一致性检查 |
| Step 4.5（测试验证，Gate E） | 35-40（测试模式） | 测试存在性 |
| Step 5（输出） | 63 Design-Missing, 64 开发文档味, 65 Translate 倾向 | Design-First + 叙事质量 |
| Gate H（design 门控） | 63, 64, 65 | Design 缺失/开发文档味/Translate 倾向 |
| 卓越性 | 73-77（文档漂移模式） | Edition/路径/参见/归属/注释行号 |
