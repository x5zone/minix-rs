---
name: "review-patterns-skill"
description: "Minix-RS Review 常见错误模式。包含文档15个模式、跨文档3个模式、代码10个模式、以及验证命令。当 Agent 在 Review 过程中需要对照检查典型错误时调用此 Skill。"
---

# Minix-RS Review 常见错误模式

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

## 四、代码错误模式（10个）

### 模式15：裸整数表达语义（Translate 味道）
```rust
❌ fn alloc_pages(count: u32, flags: u32) -> i32 { ... }
✅ fn alloc_pages(count: PageCount, flags: PageFlags) -> Result<PhysAddr, AllocError> { ... }
```

### 模式16：C 式空指针/哨兵值
```rust
❌ const NO_PHYS: PhysAddr = PhysAddr(0);  let parent_id = -1 as i32;
✅ let parent_id: Option<ProcessId> = None; let phys: Option<PhysAddr> = None;
```

### 模式17：unsafe 滥用
```rust
❌ unsafe { *ptr = value; }  // 无 safety 注释
✅ /// SAFETY: addr must be a valid, aligned physical address...
   unsafe fn write_phys(addr: PhysAddr, value: u8) { ... }
```

### 模式18：错误码不对齐
```rust
❌ return Err(Error::NotFound);  // 自己造的错误码
✅ return Err(VmError::Einval);  // 对应 Minix3 的 EINVAL
```

### 模式19：裸 as 截断无说明
```rust
❌ let old_count = pages as u16;  // 无注释
✅ // Page count bounded by MAX_PAGES (1024), fits in u16.
   let old_count = pages as u16;
```

### 模式20：硬件语义泄漏到 OS 层
```rust
❌ struct PageTable { cr3_value: u64; }  fn enable_paging(cr3: u64)
✅ trait Paging { fn load_table(&self, table: PhysAddr); fn enable(&self); }
```

### 模式21：no_std 违规
```rust
❌ 在 vm_main.rs 中（非 test）：use std::collections::HashMap;
✅ 生产代码：use alloc::collections::BTreeMap;  #[cfg(test)] 中可用 std
```

### 模式22：pub 滥用
```rust
❌ pub struct VmProc { pub id: u32, pub state: State, pub page_table: PageTable }
✅ pub struct VmProc { pub(crate) id: u32, pub(crate) state: State, page_table: PageTable }
```
口诀：「这个 pub 是因为外部需要，还是懒得组织？」

### 模式23：类型安全过度（复杂度失控）
```rust
❌ struct InitVmProc {...} struct RunningVmProc {...} struct BlockedVmProc {...} // 类型爆炸
✅ enum VmState { Init, Running, Blocked, Dying }  struct VmProc { state: VmState }
```

### 模式24：不必要的 trait 抽象
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
