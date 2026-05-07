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
