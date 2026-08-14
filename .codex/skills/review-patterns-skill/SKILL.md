---
name: review-patterns-skill
description: "Minix-RS Review 常见错误模式。包含 §0 P0 必检清单和 79 个枚举模式：文档、跨文档、代码、Kernel SMP、测试、卓越性、叙事概念、Design-First 与流程漂移模式，附验证命令。当 Agent 在 Review 过程中需要对照检查典型错误时调用此 Skill。"
---

# Minix-RS Review 常见错误模式

## §0 P0 必检清单（Gate D，每项必须显式回答 ✅/❌ + grep 证据）

| # | 检查项 | grep 命令模板 | 判定标准 | 未通过 |
|---|-------|--------------|---------|--------|
| **1** | 文档 §5 测试是否真实存在？ | `rg "fn {test_name}" {rust_dir} --type rust -n` | 文档 §5 列出的每个测试函数必须存在；缺失/未找到→P0（测试缺失） | P0 |
| **2** | 文档声明的 trait 是否有 ≥2 行为不同的 impl？ | `rg "impl.*{TraitName}" {rust_dir} --type rust -n` | 0 impl → P0（死代码/虚构 trait）；1 impl → P1（可能死代码，trait 抽象需 ≥2 行为不同的实现）；≥2 impl → ✅ | P0/P1 |
> **2026-08-15 修复 C-P1-3（0 impl 优先于测试覆盖）**：当 trait 0 impl 时，即便有测试覆盖，仍判 P0。判定优先级：**0 impl → P0 > 测试覆盖度判定**。 |
| **3** | 文档声明的函数是否在声明的文件中？ | `rg "fn {name}" {file}` | 文档说"在 file.rs 中定义 fn foo"，但 grep 无结果→P0（虚构位置）；找到但 signature 完全不符也按未通过处理 | P0 |
| **4** | 核心算法是否是 stub？ | `rg "spin_loop\|todo!\|unimplemented!\|unreachable!\|panic!" {rust_dir} --type rust -n` | 文档描述的算法在代码中体现为 `todo!`/`unimplemented!`/`unreachable!`/`spin_loop!` → P0（实现缺失）；非 test 代码中的 `panic!` 若表示功能未实现或本不应触发却可能触发 → 按 stub / 未处理路径处理，需在注释中论证其不可达性或可接受性 | P0/P1 |
| **5** | 文档 §4 签名是否与实际一致？ | 逐函数对比 `rg "fn {name}" {file}` 输出 vs 文档 §4 | 参数/返回值/可见性/泛型约束不一致→P0（签名偏移）；有一项不符即整项 ❌ | P0 |

**严格通过标准**：
- 5 项每一项必须为 ✅。
- **出现 PARTIAL / ⚠️ / 部分通过 / "基本通过" 中的任何一种，该项按 ❌ 处理，Gate D 整体未通过。**
- 任何一项 ❌ → scan.md 标记 DRAFT，禁止写入 STATE.md。
- 若某项确实不适用（如文档无 §5），需明确说明原因并单独列为一行 "N/A + 原因"，不能直接跳过。

### P0 六分类（2026-08-15 修复 C-P0-1 同步）

> 来源：[review-rules/review.md §4.1](../../../prompt/review-rules/review.md) P0 六分类表。

| P0 类型 | 含义 | 处理 | 触发 Refactor |
|--------|------|------|---------------|
| **P0-fact** | 事实错误（行号/函数名/引用与实际不符，但代码本身可运行）| 渐进修复 | 否 |
| **P0-code-bug** | 代码 bug（编译失败 / 行为错误 / panic）| 渐进修复 | 否 |
| **P0-design-deviation** | design 已规定但实现偏离 | 渐进修复（修 code 回到 design）| **code Refactor** |
| **P0-design-missing** | design 未规定但应该有 | **design Refactor**（先补 design）| **design Refactor** |
| **P0-design-wrong** | design 本身错 | **design Refactor 必须** | **design Refactor** |
| **P0-test-missing** | 测试作为正确性证明缺失（§5 测试无对应实现 / 核心 trait 0 测试）| 渐进修复（补测试），**阻断 CONVERGED** | 否 |

> **2026-08-15 修复 C-P0-1 处理要点**：P0-test-missing 不阻塞 review **发现**（可在 review 中报告），但**阻断 CONVERGED**（未修复不能 CONVERGED）。修复其他 P0 时若未补测试 → 自动升级为 P0-test-missing。

**输出格式**（写入 scan.md）：
```markdown
### Gate D: P0 必检清单

| # | 检查项 | grep 命令 | grep 结果 | 判定 |
|---|--------|----------|----------|------|
| 1 | §5 测试存在 | `rg "fn test_foo" os/` | 0 matches | ❌ P0 |
| 2 | trait FooImpl 有 impl | `rg "impl.*FooImpl" os/` | 0 matches | ❌ P0 |
| 3 | fn bar 在 baz.rs | `rg "fn bar" os/baz.rs` | baz.rs:42 | ✅ |
| 4 | 核心算法非 stub | `rg "todo!\|unimplemented!" os/` | 0 matches | ✅ |
| 5 | §4 签名一致 | 逐函数对比 | 一致 | ✅ |

**统计**：5 项中 N 项未通过 → N 个 P0
```

> **强制要求**：scan.md 必须含此表格，否则 Gate D 未通过，scan.md 标记 DRAFT。

---

## 一、文档错误模式（15个）

### 模式1：概念混淆
```
❌ Slab 最大对象 200 字节
✅ Slab 支持 200 种尺寸（SLABSIZES=200），最大 207 字节
```

### 模式2：条件编译遗漏
```
❌ SPAREPAGES=200
✅ SANITYCHECKS 构建=200 / ARM 生产=150 / x86 生产=20
```

### 模式3：行为简化误导
```
❌ alloc_pages 从高向低扫描
✅ 从 lastscan 向低扫描→失败则从 maxpage 重新开始→更新 lastscan
```

### 模式4：架构差异未说明
```
❌ pt_t 含 pt_pt[1024]
✅ pt_pt[1024] 是 Minix3 x86-32 设计，minix-rs x86-64 用动态分配
```

### 模式5：代码与文档不一致
```
❌ 文档描述 alloc_virt(phys,clicks) 分配虚拟地址并映射到 phys
✅ 源码中 alloc_virt 忽略 phys 参数，仅分配虚拟地址槽位
```

### 模式6：ASCII 图滥用（无必要）
```
❌ 用 ASCII 图解释"为什么 refcount 初始化为 0"
✅ 纯文字：分离创建和引用、灵活性、一致性
```

### 模式7：ASCII 图质量差（对齐混乱）
```
❌ 右边界参差不齐，箭头错位
✅ 严格对齐或用表格替代
```

### 模式8：路径使用绝对路径
```
❌ file:///home/user/...  绝对路径
✅ minix3/minix/servers/vm/pagetable.c#L333 项目相对路径
```

### 模式9：C 源码覆盖不完整
```
❌ vm_mappages 是核心函数但文档没分析
❌ pt_t 是核心结构体但字段不全
❌ ARCH_VM_PTE_* 系列宏没有单独分析
✅ 语义范围内所有函数/结构体/宏必须完整覆盖
```

### 模式10：设计决策缺乏依据
```
❌ Ch3 直接说"使用 typestate"，Ch1&2 无分析
✅ Ch3 说"Ch2§2.3 分析了三种状态，使用 typestate 在类型层面分离"
```

### 模式11：设计与实现脱节
```
❌ Ch3 设计 typestate，Ch4 仍用 int flags
✅ Ch4 必须体现 Ch3，不一致就改代码或更新设计
```

### 模式12：测试未覆盖设计决策
```
❌ Ch3 设计 ACL typestate 转换，测试只测"权限检查"
✅ 测试应含状态转换测试+非法转换拒绝测试
```

### 模式13：no_std 违规
```
❌ 使用 std::collections::HashMap
✅ alloc::collections::BTreeMap 或 hashbrown::HashMap
```

### 模式14：硬件未抽象为 trait
```rust
❌ struct PageTable { pde: [u32; 1024] }  // 直接编码硬件
❌ #[cfg(target_arch)] 选择硬件行为  // 分散条件编译

✅ trait Paging {
    const PAGE_SIZE: usize;
    fn map(&mut self, vaddr, paddr, flags) -> Result<(), PageTableError>;
    fn unmap(&mut self, vaddr) -> Result<PhysBytes, PageTableError>;
}
```

### 模式15：开发记录风格（文档定位偏移）
```
❌ "已实现：handle_munmap 函数"  "待实现：munmap_vm_lin"
❌ "✅ Split-First 原则  ❌ 权限检查"  "## 实现清单"  "## 现有代码状态"
✅ "handle_munmap 函数负责处理取消映射请求"
✅ "munmap_vm_lin 处理 VM 进程自身的取消映射"
✅ "## 模块组成与测试"  "## Rust 实现组件一览"

核心区分：开发记录关注"做了什么/没做什么"；知识讲解关注"系统怎么工作"
判定："已实现/待实现/未实现"→P1；✅❌🚧→P1；进度标题→P1；>5处→系统性重写
注意：TODO 允许保留，TODO 标记待做事项与进度标记不同
```

---

## 二、跨文档联动错误模式（3个）

### 模式 A：重复定义
```
# 04 文档：CLICK_SIZE=4096
# 05 文档：CLICK_SIZE=4096  ❌ 重复，应引用 04
```

### 模式 B：定义矛盾
```
# 03 文档：vm_acl 取值 1~31
# 01 文档：vm_acl 取值 1~32  ❌ 矛盾
```

### 模式 C：遗漏引用
```
# 05 文档：vm_allocpage 使用 pt_init_done...
# 缺少引用 → 应加：参见 [07-pagetable-ops.md]
```

---

## 三、跨文档检查范围与命令

**主要范围**：本文档所在目录下所有 `.md` 文件
**扩展范围**："参见"章节引用的其他目录文档
**排除范围**：未引用的其他模块文档

**检查清单**：共享数据结构是否在别处有更权威定义？共享常量是否重复定义？IPC 接口是否匹配"参见"文档？前置依赖是否有对应文档？后置引用是否会被本文档变化破坏？

**验证命令**：
```bash
DIR="notes/rewrite/{module}/"
rg "CONSTANT\s*=" "$DIR" --type md -n       # 常量是否多处定义
rg "struct struct_name" "$DIR" --type md -n  # 结构体是否多处描述
rg "CALL_NUMBER" "$DIR" --type md -n         # IPC 调用号是否一致
rg "\[.*\]\(.*\.md\)" "$DIR" --type md -n    # 文档间交叉引用
rg "\[.*\]\((\.\./.*\.md)\)" "$DIR" --type md -n  # "参见"文档是否存在
```

---

## 四、代码错误模式（14个）

### 模式16：裸整数表达语义（Translate 味道）
```rust
❌ fn alloc_pages(count: u32, flags: u32) -> i32 { ... }
✅ fn alloc_pages(count: PageCount, flags: PageFlags) -> Result<PhysAddr, AllocError> { ... }
```

### 模式17：C 式空指针/哨兵值
```rust
❌ const NO_PHYS: PhysAddr = PhysAddr(0);  let parent_id = -1 as i32;
✅ let parent_id: Option<ProcessId> = None; let phys: Option<PhysAddr> = None;
```

### 模式18：unsafe 滥用
```rust
❌ unsafe { *ptr = value; }  // 无 safety 注释
✅ /// SAFETY: addr must be a valid, aligned physical address...
   unsafe fn write_phys(addr: PhysAddr, value: u8) { ... }
```

### 模式19：错误码不对齐
```rust
❌ return Err(Error::NotFound);  // 自己造的错误码
✅ return Err(VmError::Einval);  // 对应 Minix3 的 EINVAL
```

### 模式20：裸 as 截断无说明
```rust
❌ let old_count = pages as u16;  // 无注释
✅ // Page count bounded by MAX_PAGES (1024), fits in u16.
   let old_count = pages as u16;
```

### 模式21：硬件语义泄漏到 OS 层
```rust
❌ struct PageTable { cr3_value: u64; }  fn enable_paging(cr3: u64)
✅ trait Paging { fn load_table(&self, table: PhysAddr); fn enable(&self); }
```

### 模式22：no_std 违规
```rust
❌ 在 vm_main.rs 中（非 test）：use std::collections::HashMap;
✅ 生产代码：use alloc::collections::BTreeMap;  #[cfg(test)] 中可用 std
```

### 模式23：pub 滥用
```rust
❌ pub struct VmProc { pub id: u32, pub state: State, pub page_table: PageTable }
✅ pub struct VmProc { pub(crate) id: u32, pub(crate) state: State, page_table: PageTable }
```
口诀：「这个 pub 是因为外部需要，还是懒得组织？」

### 模式24：类型安全过度（复杂度失控）
```rust
❌ struct InitVmProc {...} struct RunningVmProc {...} struct BlockedVmProc {...} // 类型爆炸
✅ enum VmState { Init, Running, Blocked, Dying }  struct VmProc { state: VmState }
```

### 模式25：不必要的 trait 抽象
```rust
❌ trait VmPagingExt { fn bind_to_process(&self, proc: &VmProc); }
   // 所有实现行为相同，都调用同一个 syscall → 自由函数即可

❌ trait PhysAllocatorStats { fn memstats(&self) -> PhysMemStats; }
   // 单方法 trait 从未作为 trait bound → 固有方法 + enum 分发

✅ trait Paging {
    fn map(&mut self, vaddr, paddr, flags) -> Result<(), PageTableError>;
   }
   // x86-64 四级页表，aarch64 不同描述符格式
   // fn paging_init<P: Paging>(p: &mut P) 使用 trait bound → 合理
```

**判断标准**：≥2个行为不同的实现 + 被用作 trait bound → ✅；任一不满足 → 考虑简化。

### 模式26：Kernel SMP 并发违规（BKL 未持有）
```rust
❌ // 内核全局变量访问未受 BKL 保护
   static mut CPU_READY: bool = false;
   fn check_cpu_ready() -> bool {
       unsafe { CPU_READY }  // 其他 CPU 可能正在写 CPU_READY
   }

✅ // 明确标注 BKL 保护的前提
   /// SAFETY: Caller must hold BKL (BKL_LOCK() already acquired).
   static mut CPU_READY: bool = false;
   fn check_cpu_ready() -> bool {
       // 调用者已持有 BKL，单写者保证
       unsafe { CPU_READY }
   }
```

### 模式27：Kernel SMP 并发违规（Rc/RefCell 跨 CPU 共享）
```rust
❌ use alloc::rc::Rc;
   // Rc: !Send + !Sync，在多 CPU 内核中共享是 UB
   static KERNEL_CONFIG: Lazy<Rc<KernelConfig>> = Lazy::new(|| Rc::new(...));

✅ use alloc::sync::Arc;
   // Arc: Send + Sync，适合跨 CPU 共享（配合 BKL 或 RwLock）
   static KERNEL_CONFIG: Lazy<Arc<KernelConfig>> = Lazy::new(|| Arc::new(...));
   // 或：per-CPU 数据用 Rc，但必须注释"per-CPU, no cross-CPU sharing"
```

### 模式28：Kernel SMP 并发违规（spinlock 内睡眠/调度/等待）
```rust
❌ BKL_LOCK();
   let reply = ipc_sendrec(PM_PROC_NR, &msg);  // 可能 block，spinlock 内禁止！
   BKL_UNLOCK();

✅ BKL_UNLOCK();
   let reply = ipc_sendrec(PM_PROC_NR, &msg);  // 释放 BKL 后等待 IPC
   BKL_LOCK();
   // 注意：重新获取 BKL 后，共享状态可能已被其他 CPU 修改
   // 需要重新验证共享状态的 invariants
```
> 原因：BKL 是 spinlock（busy-wait），spinlock 内任何可能导致当前 CPU 让出执行权的操作（睡眠、调度、等待锁、等待 IPC 响应）都可能导致 deadlock。

### 模式29：Kernel SMP 并发违规（per-CPU 数据被跨 CPU 访问）
```rust
❌ fn get_cpu_ticks(cpu_id: u32) -> u64 {
       unsafe { PER_CPU_DATA[cpu_id as usize].ticks }
       // 直接读取其他 CPU 的 local 数据，无保护
   }

✅ // per-CPU 数据不暴露跨 CPU 读取接口
   fn get_my_ticks() -> u64 {
       let cpu = get_cpu_var();
       unsafe { PER_CPU_DATA[cpu].ticks }
   }

✅ // 如果确实需要跨 CPU 读取，使用 Atomic 类型 + 注释说明
   struct PerCpuData {
       ticks: AtomicU64,  // Atomic 保证跨 CPU 可见性
   }
```
---

## 五、跨阶段通用错误模式（5个）

### 模式30：外部知识误导
```
❌ x86-64 长模式下"需设置 CR4.PSE 以支持 2MB 大页"
✅ x86-64 长模式下 2MB/1GB 页由页表项 PS 位控制，不需要 CR4.PSE
适用：Boot/VM/PM/VFS 中引用的硬件/协议/规范
```

### 模式31：通用接口含上下文特定元素
```
❌ struct KernelInfo { syscall_entry: VirBytes } // 所有架构必填，仅 x86-64 使用
✅ struct KernelInfo { /// x86-64 专用，其他忽略\n    syscall_entry: Option<VirBytes> }
适用：跨架构/跨模块/跨进程类型的共享数据结构
```

### 模式32：外部调用返回值被无说明忽略
```
❌ let _mmap = exit_boot_services(image, map_key); // 无说明丢弃
✅ let _mmap = exit_boot_services(image, map_key);
   // 已提前构建映射，此返回值仅用于调试
适用：固件/系统/库/硬件抽象的外部调用
```

### 模式33：资源获取后无释放路径说明
```
❌ let memmap = Box::leak(Box::new(regions)); // 泄漏后如何回收？未说明
✅ let memmap = Box::leak(Box::new(regions));
   // boot-shim 一次性，kernel 通过 boot_shim_start/len 回收
适用：Boot/VM/PM/VFS 中所有资源分配
```

### 模式34：注释理由虚假或牵强
```
❌ pub kern_size: u64 // u64 避免 32 位截断（微内核不可能超 4GB）
✅ pub kern_size: u64 // 与地址类型一致，避免运算时类型转换
适用：所有阶段的所有注释和设计决策
```

---

## 六、测试错误模式（6个）

### 模式35：测试未覆盖核心语义（L1 对偶缺失）
```rust
❌ // 仅测试 Rust 内部逻辑，未对比 Minix3 行为
   #[test] fn test_alloc_mem_returns_ok() { assert!(alloc_mem(1024).is_ok()); }

✅ // L1 对偶测试，验证与 Minix3 行为一致
   #[test] fn test_alloc_mem_parity_with_c() {
       let result = alloc_mem(PageCount(1024));
       assert_eq!(result.unwrap(), PhysAddr(0x10000));  // Minix3 在此输入下返回 0x10000
   }
```
> 核心函数必须有 L1 对偶测试。

### 模式36：Trait 契约测试缺失（L2 缺失）
```rust
❌ // 仅测试具体实现，未测试 trait 契约
   #[test] fn test_x8664_paging() { ... }

✅ // L2 契约测试，验证所有实现满足 trait 契约
   #[test] fn test_paging_contract<P: Paging>(p: &mut P) {
       p.map(vaddr, paddr, flags).unwrap();
       assert_eq!(p.query(vaddr), Some((paddr, flags)));
       p.unmap(vaddr).unwrap();
       assert_eq!(p.query(vaddr), None);
   }
```

### 模式37：doctest 缺失（L3 缺失）
```rust
❌ /// 分配物理内存
   pub fn alloc_mem(count: PageCount) -> Result<PhysAddr, AllocError> { ... }

✅ /// 分配物理内存
   /// # Example
   /// ```
   /// use minix_vm::alloc_mem;
   /// let addr = alloc_mem(PageCount(1)).unwrap();
   /// assert!(addr.as_u64() > 0);
   /// ```
   pub fn alloc_mem(count: PageCount) -> Result<PhysAddr, AllocError> { ... }
```

### 模式38：测试命名不表达意图
```rust
❌ fn test_1() / fn test_it_works() / fn test_alloc()
✅ fn test_alloc_mem_returns_aligned_address()
   fn test_alloc_mem_fails_when_out_of_memory()
   fn test_alloc_mem_zero_count_returns_error()
```

### 模式39：测试仅覆盖正常路径
```rust
❌ #[test] fn test_map_page() { pt.map(vaddr, paddr, flags).unwrap(); }

✅ #[test] fn test_map_page_normal() { ... }
   #[test] fn test_map_page_already_mapped_returns_error() { ... }
   #[test] fn test_map_page_unaligned_address_returns_error() { ... }
   #[test] fn test_map_page_zero_address_returns_error() { ... }
```

### 模式40：测试依赖全局状态（flaky test）
```rust
❌ static mut COUNTER: u32 = 0;
   #[test] fn test_alloc() {
       unsafe { COUNTER += 1; }
       assert_eq!(alloc_mem(1).unwrap(), PhysAddr(COUNTER * 4096));  // 顺序敏感
   }

✅ #[test] fn test_alloc() {
       let allocator = TestAllocator::new();  // 每次新建
       assert!(allocator.alloc(1).is_ok());
   }
```

---

## 七、卓越性错误模式（7个）

> 详见 [review-doc-excellence.md](../../../prompt/review-rules/review-doc-excellence.md) 和 [review-code-excellence.md](../../../prompt/review-rules/review-code-excellence.md)

### 模式41：文档叙事弧断裂（卓越性）
```
❌ 章节堆砌，无叙事弧
   ## Ch1 概念  ## Ch2 源码分析  ## Ch3 设计决策  ## Ch4 实现
   → 每章直接开始，无过渡，无动机说明

✅ 有叙事弧和动机
   ## Ch1 概念  本章解决：什么是 VM 内存分配？为什么需要重新设计？
   ## Ch2 源码分析  上一章定义了核心概念，本章分析 Minix3 如何实现
   ## Ch3 设计决策  Ch2 揭示了几个问题，本章设计解决方案
```

### 模式42：文档术语未定义（卓越性）
```
❌ "vmproc 的 vm_acl 字段控制地址空间权限..."  → 术语首次出现无定义
✅ "**vmproc**（VM Process）是 VM 服务器中描述进程地址空间的结构体。
   其 **vm_acl** 字段控制地址空间权限，取值为 -1/0/1~31。"
```

### 模式43：代码 API 易误用（卓越性）
```rust
❌ struct PageTable { is_init: bool, root: Option<PhysAddr> }  // 无效状态可表达
✅ enum PageTable { Uninit, Init { root: PhysAddr } }  // 类型系统防止无效状态
```

### 模式44：代码错误类型不精确（卓越性）
```rust
❌ fn alloc_mem(count: PageCount) -> Result<PhysAddr, Box<dyn Error>> { ... }
✅ enum AllocError { ZeroCount, OutOfMemory, InvalidAlignment }
   fn alloc_mem(count: PageCount) -> Result<PhysAddr, AllocError> { ... }
```

### 模式45：代码冗余注释（卓越性）
```rust
❌ let x = 5;  // x 赋值为 5
   let y = x + 1;  // y 等于 x 加 1
✅ let page_count = (size + PAGE_SIZE - 1) / PAGE_SIZE;  // 向上取整到页边界
```

### 模式46：代码副作用隐藏（卓越性）
```rust
❌ fn get_process_count() -> u32 { PROCESS_COUNT += 1; PROCESS_COUNT }  // 隐藏副作用
✅ fn get_process_count() -> u32 { PROCESS_COUNT }
   fn increment_process_count() { PROCESS_COUNT += 1; }
```

### 模式47：代码全局依赖未注入（卓越性）
```rust
❌ fn alloc_mem(count: PageCount) -> Result<PhysAddr, AllocError> {
       let bitmap = unsafe { &GLOBAL_BITMAP };  // 全局依赖，难以测试
       bitmap.alloc(count)
   }

✅ fn alloc_mem(bitmap: &mut Bitmap, count: PageCount) -> Result<PhysAddr, AllocError> {
       bitmap.alloc(count)  // 依赖注入，可测试
   }
```

---

## 八、叙事与概念错误模式（10个）

> **背景**：03-kmain-cstart 重构案例暴露一批"正确性通过但教学性失败"的模式。
> **详见**：[review-rules/review.md §概念抽象原则](../../../prompt/review-rules/review.md)、[review-rules/review-doc-checklist.md §1.Ch1](../../../prompt/review-rules/review-doc-checklist.md)。

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

**自动检测**：`tools/doc-freshness-check.sh` 对比 `rg "^pub " struct_file.rs` 与 doc §X 字段计数。
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

## §X.5 Design-First 反模式（模式 63-65）

> 原有模式（18-25）增强"与 design 的关系"段落，并新增 3 个 design 导向反模式。
> **完整定义**：见 [review-rules/review-patterns.md](../../../prompt/review-rules/review-patterns.md)。

### 模式 63: Design-Missing 反模式

**定义**：review 流程只发现实现问题，不发现 design 缺失，导致反复局部修复却始终漏掉核心概念。

**触发**：
- 设计文档（`design.md` 非 bagging / `design-final.md` bagging）不存在或 outdated
- review 输出大量 P0-fact / P0-code-bug，但 P0 数量持续不收敛
- 反复修复同一类问题（如文档结构反复调整）

**判定信号**：
- 单文档 review 中发现 ≥3 处"概念缺失"类问题
- 多次 review 循环中同一模块的 P0 数量不减反增

**修复路径**：
1. 中断 review（IN_DESIGN 状态）
2. 生成缺失的 design（用文档重写工作流 Step 1-3）
3. design 通过 Gate H 后恢复 review

> **对应 P0 类型**：P0-design-missing / P0-design-wrong

### 模式 64: 开发文档味反模式

**定义**：文档使用"旧版→新版"、"最初→后来"、"我们改成"等迭代叙事，违反"教学材料"原则。

**判定信号**（grep 自动检测，30+ 禁用词）：
```bash
grep -nE "旧版|最初|后来|我们改成|已实现|待实现|未完成|TODO|FIXME|过去|当年|当初|之前|改成|替代|替换|替换为|升级|废弃|不再|放弃|修正|我|我们|本项目|今后|接下来|添加|删除|重构|第一步|第二步|首先|然后|接着|最后|XXX|NOTE" \
    prompt/../doc.md
```

**修复路径**：
1. 删除"旧版→新版"叙事 + 删除"我们改成"开发主体
2. 删除"已实现/待实现"状态标记
3. 用"如果 X 设计会有 Y 问题，所以用 Z"替代

### 模式 65: Translate 倾向反模式

> 教训回顾：——"rewrite 不是 translate"。

**定义**：Rust 代码是 C 代码的 1:1 翻译，没有用 Rust 类型系统重新表达。

**判定信号**：
- Rust 函数签名与 C 函数高度相似（命名、参数顺序）
- 用 `static mut` / 函数指针数组 / 裸 `unsafe` 块密度过高
- 没利用 newtype / enum / match / Result 等 Rust 抽象

**修复路径**：
1. 删除与 C 高度相似的函数命名
2. 将 int/uint 改为 newtype（带语义的不透明类型）
3. 将函数指针数组改为 enum + match（利用 Rust 穷尽检查）

---

## §X.6 Review 流程反模式（模式 66-77，NEW 2026-07-16/17/30/31）

> 关注 review 流程本身的元数据完整性与 AI claim 真实性。**完整定义**：见 [review-rules/review-patterns.md](../../../prompt/review-rules/review-patterns.md)。

### 模式 66: 参考代码路径漂移（Reference Code Path Drift, RCPD）

**定义**：TODO/issue 描述引用的 `file:line` 在被审时期存在、review 时已不存在（重构/重命名）。

**判定信号**：
- TODO 描述引用的 file:line `ls`/`rg` 验证失效
- 多 AI 共识基于已删除路径产生虚假 P0

**规则**：Step 0.7.1 path existence validation 强制；验证不通过 → 标"路径失效" + 触发 doc 重写而非 code 修复。

**严重度降级**：P0 + 路径失效 → P1；必须有 L1 证据（`ls`/`rg` 命中或失效证明）。

**Doc 主动应用案例（NEW 2026-07-31, Doc 07 review）**：doc 07 §5.4 显式声明"不引用 syscall_copy.rs 具体行号避免漂移传播（Pattern #66 RCPD）"—— **首个 doc 主动标注已应用 review pattern**。这表明 doc 作者已具备 review pattern 意识，主动避免引入 Pattern #66 风险。

**建议**：其他 doc 在 §5 测试章节引用行号时，可借鉴 doc 07 的做法：
- 显式声明"不引用 X.rs 行号，避免漂移传播"
- 或：使用 grep 命令而非具体行号（如"通过 `rg fn X os/Y.rs` 找到实现"）
- 或：行号引用加版本/时间戳（如"截至 YYYY-MM-DD, X.rs:Y"）

### 模式 67: C 函数名 vs OS 概念误判（C Function Name vs OS Concept Confusion, CFNOC）

**定义**：AI 把 C 函数名（如 `init_clock()`）误读为 OS 概念对象（如 "CLOCK task"）。

**判定信号**：
- AI 报告中"X task" / "X subsystem" 但 `rg "X"` 0 命中
- 把 `xxx_init()` / `init_xxx()` 函数名误认为 xxx 对象本身

**规则**：Step 0.7.2 AI claim grep verification 强制；C 全大写宏（`CLOCK_TASK`/`TASK_CLOCK`/`SYSTEM`）才表示任务类型常量。

### 模式 68: Ch2 C 源码展示 ≠ Ch4 Rust 实现误判（Doc Section Confusion, DSC）

**定义**：AI 把 Ch2（C 源码展示章节）含 `#ifdef` 误报为"Rust 代码散落分支"。

**判定信号**：
- AI 报告 `#ifdef` 散落但未区分 Ch2 C 展示 vs Ch4 Rust 实现
- `rg "os/**/*.rs"` 验证无 `#[cfg(...)]` 分支但 AI 报告有

**规则**：Step 0.7.3 doc chapter context awareness 强制；报告前必须明确"这是 Ch2 C 展示问题还是 Ch4 Rust 实现问题"。

### 模式 69: 缺失 per-doc 快照（Per-doc Snapshot Missing, PSMD）

**定义**：AI 以"已有 CONVERGED 状态"/"incremental review"/"复用其他文档 design"为由跳过 design + outline 预检，导致 scan.md 标 CONVERGED 但 `{NN}-outline.v*.md` / `{NN}-design.v*.md` 实际缺失。

**判定信号**：
- AI 跳过 `ls .design/{NN}-*.v*.md` 命令
- scan.md Block Gates 把 Gate H 标 "N/A" / "[SIMPLIFIED]" 而非 PASS/FAIL
- 用"已有 CONVERGED 状态"或"复用 03/04 design"为由跳过预检

**规则**：P0-process-violation。Step 0 硬阻断强制 4 条 `ls`；缺失 → Step 0.3 嵌入生成（不中断 review）；禁止跨文档复用快照。

### 模式 70: 跨轮状态陈旧（Cross-Turn Outdated Staleness, CTOS）

**定义**：TODO 列表跨多轮 review 累积，部分 TODO 前提已失效但未被检测，导致 AI 采纳全部 TODO 为真。

**判定信号**：
- `tmp_design_and_todo/` 中 TODO 数 > 5 + 含"基于..."/"依赖..."/"待..."等时间敏感词
- TODO 描述引用"TODO-XX-N 未修复"但当前 rg 0 命中
- TODO 描述引用"待 X 完成"但当前 X 已存在

**规则**：Step 0.7.4 TODO staleness check 强制。前提失效 → 标"前提失效" + 严重度降级（P0→P1，P1→P2，P2→误报）。**批量工具**：`tools/todo-staleness-check.sh {todo-file}`。

**TODO 强制格式**：`[P0/P1/P2] [code/doc] [factual/design] 问题描述 (file:line) (基于状态) (修复方案)`，缺字段 → TODO 不可信。

### 模式 71: 决策泛化误用（Decision Over-Generalization, DOG）

**定义**：AI 把"X 文档豁免"用户决策泛化到"Y/Z 文档"。

**判定信号**：
- AI 把"X 文档豁免"泛化到"Y 文档"
- 没有任何豁免记录在 STATE.md，但 AI 声称"已有 CONVERGED 状态"
- 用户决策仅指明"X"，但 AI 把范围扩大到"X+Y+Z"

**规则**：
- (a) 决策仅适用当时对象：用户决策"X 不回填"仅对 X 有效，不可泛化
- (b) 豁免必须显式记录：豁免必须在 STATE.md `§豁免列表` 段登记
- (c) 泛化必须询问：AI 认为需要类似豁免时，必须先询问用户
- (d) "CONVERGED 状态"不是豁免依据：CONVERGED 是结果状态，不是豁免资格

### 模式 72: 跨章节步骤数不一致（Cross-Section Step Count Mismatch, CSSCM）（NEW 2026-07-17）

**定义**：文档 §2 C 源码分析列出的步骤数与 §4 Rust 实现的步骤数不一致，且 §4 未明确说明差异原因。

**判定信号**：
- §2 列出 N 步，§4 实现 M 步（M ≠ N），且 §4 无"与 C N 步的差异说明"表
- §4 有差异说明但原因空泛（"Rust 不需要"/"架构差异"无具体说明）
- §2 与 §4 步骤数一致但顺序重排且未说明（P2）

**严重度**：P1（默认）/ P0（若未实现步骤涉及安全/并发/内存管理关键路径，且未标注"已知缺口"）

**规则草案**：
- (a) §2 步骤数必须与 §4 步骤数一致；若不一致，§4 末尾必须添加"与 C N 步的差异说明"表
- (b) 每条差异必须归类：架构演进 / 设计决策 / 已知缺口 / C 源码 bug
- (c) 每条差异附 C 源码行号
- (d) 适用于任何"分析章节 vs 实现章节"的步骤数对比

**检查命令**：
```bash
rg "与 C .* 步的差异说明|步骤数差异|未实现步骤" notes/rewrite/{module}/{stage}/{doc}.md
```

**与模式 11/60 区分**：
- 模式 11 = design↔code 跨制品脱节
- 模式 60 = 未实现功能是否标注 TODO
- 模式 72 = §2↔§4 同文档内跨章节步骤数一致性（已实现步骤也可能触发）

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

---

### 模式 78：C 源码 bug 未显式标注（CSBU）（P1）

- Rust 修复了 C 源码 bug 但未标注 `// MINIX3 BUG:` → P1
- 验证：`rg "// MINIX3 BUG:" os/ --type rust`
- 来源：region.c:841 ev_reference 忽略、enter_queue 写错进程、anon_pagefault 内存泄漏

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
