---
name: review-patterns-skill
description: Minix-RS Review 常见错误模式。包含 §0 P0 必检清单（5项机械化检查）、文档15个模式、跨文档3个模式、代码14个模式（含4个 Kernel SMP 并发模式）、5个跨阶段通用模式、6个测试模式、7个卓越性模式，以及验证命令。当 Agent 在 Review 过程中需要对照检查典型错误时调用此 Skill。
---

# Minix-RS Review 常见错误模式

## §0 P0 必检清单（Gate D，每项必须显式回答 ✅/❌ + grep 证据）

| # | 检查项 | grep 命令模板 | 判定标准 | 未通过 |
|---|-------|--------------|---------|--------|
| **1** | 文档 §5 测试是否真实存在？ | `rg "fn {test_name}" {rust_dir} --type rust -n` | 文档 §5 列出的每个测试函数必须存在；缺失/未找到→P0（测试缺失） | P0 |
| **2** | 文档声明的 trait 是否有 ≥1 impl？ | `rg "impl.*{TraitName}" {rust_dir} --type rust -n` | 0 impl → P0（死代码/虚构 trait）；只有默认 impl 没有真实 arch impl 也算未通过 | P0 |
| **3** | 文档声明的函数是否在声明的文件中？ | `rg "fn {name}" {file}` | 文档说"在 file.rs 中定义 fn foo"，但 grep 无结果→P0（虚构位置）；找到但 signature 完全不符也按未通过处理 | P0 |
| **4** | 核心算法是否是 stub？ | `rg "spin_loop\|todo!\|unimplemented!\|unreachable!" {rust_dir} --type rust -n` | 文档描述的算法在代码中体现为 `todo!`/`unimplemented!`/`unreachable!`/`spin_loop!` → P0（实现缺失） | P0 |
| **5** | 文档 §4 签名是否与实际一致？ | 逐函数对比 `rg "fn {name}" {file}` 输出 vs 文档 §4 | 参数/返回值/可见性/泛型约束不一致→P0（签名偏移）；有一项不符即整项 ❌ | P0 |

**严格通过标准**：
- 5 项每一项必须为 ✅。
- **出现 PARTIAL / ⚠️ / 部分通过 / "基本通过" 中的任何一种，该项按 ❌ 处理，Gate D 整体未通过。**
- 任何一项 ❌ → scan.md 标记 DRAFT，禁止写入 STATE.md。
- 若某项确实不适用（如文档无 §5），需明确说明原因并单独列为一行 "N/A + 原因"，不能直接跳过。

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

> 详见 [review-doc-excellence.md](../review-rules/review-doc-excellence.md) 和 [review-code-excellence.md](../review-rules/review-code-excellence.md)

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

