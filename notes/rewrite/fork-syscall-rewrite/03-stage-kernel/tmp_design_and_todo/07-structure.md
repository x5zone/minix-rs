# 07-structure.md — 知识点结构与重组诊断

> **生成时间**: 2026-06-23
> **目标文档**: `07-cross-space-init.md`
> **诊断结论**: 现有文档根叙事（ptproc + freepdes）与 VM server 的 direct_map 决策直接冲突，需 redesign 重写。
> **Ground Truth 优先级**: Minix3 C 源码 > VM server 设计文档（direct_map 决策）> 现有 Rust 实现 > 现有 07 文档

---

## 一、知识点全集

### 概念组 A：跨地址空间访问问题（OS 理论维度）

| 编号 | 知识点 | C 源码锚点 | OS 理论/机制依据 | Rust 现状（批判性） |
|---|---|---|---|---|
| A.0 | 内核为何需要访问用户进程内存 | `memory.c:80` createpde 调用链 | IPC 消息拷贝（lin_lin_copy）、vm_memset 等内核服务需要读写目标进程地址空间 | 现有文档讲了，但归因到 freepdes 而非本质问题 |
| A.1 | 32 位时代的临时窗口机制 | `memory.c:80-145` createpde / `memory.c:32` freepdes[] | 虚拟地址空间受限（4GB），用页目录槽位临时挂载目标 PDE | **待 redesign**：freepdes 整体废弃 |
| A.2 | ptproc 语义：当前 CR3 装的是谁 | `cpulocals.h:56` ptproc / `klib.S:597` __switch_address_space | 进程切换时换 CR3 + 记录"现在页目录是谁"，freepdes 借此页目录放临时映射 | **待 redesign**：direct_map 下 kernel 有自己的 direct map，不借任何进程页目录 |
| A.3 | 64 位时代的 direct_map 替代 | 无 C 源码（Minix3 全线无 direct map，见 VM 06-pagetable-struct.md:685） | 虚拟地址空间充裕（256TB+），建立 pa→va 固定线性偏移映射，Linux/Windows/BSD 通用模式 | **VM 已实现** DirectMapArch trait（06-pagetable-struct.md:776），07 应对接 |
| A.4 | 双视图地址空间（硬件特权级要求） | 无 C 源码（Minix3 32 位无此设计） | x86-64 U/S 位不能同时 0 和 1，VM(ring3) 和 Kernel(ring0) 必须各有 direct map 窗口 | VM 文档已定义（06-pagetable-struct.md:693），07 未覆盖 |
| A.5 | 内核映射在每个进程地址空间 | `pg_utils.c:186` pg_mapkernel | 中断/系统调用进入 ring 0 时必须能执行内核代码，故每个进程页表都映射内核 | 现有文档未讲此本质，只讲了 freepdes |

### 概念组 B：阶段 D 的 C 源码事实（Ground Truth）

| 编号 | 知识点 | C 源码锚点 | OS 理论/机制依据 | Rust 现状（批判性） |
|---|---|---|---|---|
| B.0 | arch_post_init() 三步 | `protect.c:370-377` (x86) / `protect.c:97-104` (ARM) | proc_addr(VM) → ptproc=VM → pg_info() | 现有文档翻译正确，但 ptproc 语义在 direct_map 下变了 |
| B.1 | pg_info() 记录 bootstrap 页表地址 | `pg_utils.c:312-316` | *pagedir_ph=vir2phys(pagedir); *pagedir_v=pagedir | 现有 VmPageTableInfo{phys_root,virt_root} 翻译，direct_map 下语义需重审 |
| B.2 | pg_mapkernel() 建立内核映射 | `pg_utils.c:186-206` (x86) / `earm/pg_utils.c:184` | 4MB 大页映射内核代码/数据段，返回 freepde_start | **07 文档完全未讲 pg_mapkernel**——这是 freepde_start 的来源，关键遗漏 |
| B.3 | pre_init 阶段记录 freepde_start | `pre_init.c:232` (x86) / `pre_init.c:407` (ARM) | kinfo.freepde_start = pg_mapkernel() | 现有文档提到 kinfo.freepde_start 但未讲来源 |
| B.4 | memory_init() 分配 2 个 freepdes | `memory.c:707-717` (x86) / `memory.c:612-622` (ARM) | freepdes[nfreepdes++]=kinfo.freepde_start++ ×2 | **待 redesign**：direct_map 下整体废弃 |
| B.5 | createpde() 消费 freepdes | `memory.c:80-145` | 借 ptproc 页目录放临时 PDE，返回临时窗口 VA | 后续文档内容，07 仅导览 |
| B.6 | mem_clear_mapcache() 清理 | `memory.c:35-50` | 用完清空 freepdes 槽位，防残留 | 后续文档内容 |
| B.7 | IPCNAME 调试宏（阶段 D 中间夹） | `main.c:277-290` | IPC 调用类型编号→字符串映射，仅调试用 | 现有文档讲了，但与跨空间主题无关，可精简或移出 |
| B.8 | ptproc 是 per-CPU 变量 | `cpulocals.h:56` | 每个 CPU 记录自己的"当前页表进程" | 现有文档讲了，direct_map 下需重审是否还需要 |

### 概念组 C：direct_map 设计（VM server 决策，07 必须对接）

| 编号 | 知识点 | C 源码锚点 | OS 理论/机制依据 | Rust 现状（批判性） |
|---|---|---|---|---|
| C.0 | Minix3 全线无 direct map | 无（VM 06-pagetable-struct.md:685 明确） | pmap.h 的 PMAP_DIRECT_MAP 宏受 #ifdef __HAVE_DIRECT_MAP 保护且从未定义 | 07 现有文档未提及此事实 |
| C.1 | direct_map 是 Rust 版全新引入 | 无 C 源码 | Allowed Evolution：架构位宽演进 + 硬件抽象 | 07 现有文档未提及 |
| C.2 | DirectMapArch trait | 无 C 源码 | va = pa + BASE，跨架构仅 BASE 不同 | VM 已实现（06-pagetable-struct.md:776），07 应对接 |
| C.3 | VM direct map（U/S=1） | 无 C 源码 | VM 用户态访问物理内存的窗口，kernel 建立 | 07 未覆盖 |
| C.4 | Kernel direct map（U/S=0, G=1） | 无 C 源码 | 内核访问物理内存的窗口，VM 的 map_kernel 建立 | 07 未覆盖 |
| C.5 | map_kernel 职责演进 | `pagetable.c:1442` pt_mapkernel (C) | Minix3: 内核段+page_directories+freepdes；Rust: 内核段+Kernel direct map | VM 07-pagetable-ops.md:1218 已定义，07 未对接 |
| C.6 | 初始页表由 kernel 建立 | `pre_init.c` / `protect.c` | VM 还没运行，初始页表（含 VM direct map）是 VM 进程创建前提 | VM 06-pagetable-struct.md:821 已定义，07 未对接 |
| C.7 | 4 页初始页表结构 | 无 C 源码（Minix3 用 2 级页表） | PML4+PDPT_A+PD_A+PDPT_B，PDPT_B[0]=1GB direct map | VM 06-pagetable-struct.md:741 已定义 |

### 概念组 D：Rust 实现现状（批判性，非权威）

| 编号 | 知识点 | C 源码锚点 | OS 理论/机制依据 | Rust 现状（批判性） |
|---|---|---|---|---|
| D.0 | PostInitArch trait + set_ptproc() | `protect.c:370` | 占位 no-op（let _ = vm_page_table） | **待 redesign**：direct_map 下 set_ptproc 语义需重定义或废弃 |
| D.1 | MemoryInitArch trait + allocate_free_pdes() | `memory.c:707` | 返回 FreePdeSlots | **待 redesign**：整体废弃 |
| D.2 | VmPageTableInfo{phys_root, virt_root:Option} | `pg_utils.c:312` | 翻译 p_cr3/p_cr3_v | direct_map 下 phys_root 仍有意义（satp/CR3），virt_root 语义变了 |
| D.3 | CrossSpaceInit{vm_page_table, free_pde_slots} | 无直接 C 对应 | 聚合结构体 | free_pde_slots 字段应删 |
| D.4 | FreePdeSlots 结构体 | `memory.c:32` freepdes[] | 固定数组+len | **待 redesign**：整体废弃 |
| D.5 | FREE_PDE_SLOTS static mut | `memory.c:32` | 全局可变状态 | **待 redesign**：应删 |
| D.6 | FREE_UPPER_IDX AtomicUsize | `pre_init.c:232` kinfo.freepde_start | 全局原子 | **待 redesign**：应删 |
| D.7 | init_post_and_memory() 主流程 | `main.c:283-285` | 阶段 D 入口 | 流程需重写为 direct_map 语义 |

### 概念组 E：架构演进与跨架构统一

| 编号 | 知识点 | C 源码锚点 | OS 理论/机制依据 | Rust 现状（批判性） |
|---|---|---|---|---|
| E.0 | 32 位→64 位页表层级变化 | `memory.c` I386_VM_DIR_ENTRIES | 2级(PD+PT)→4级(PML4+PDPT+PD+PT) | 现有文档 §3.3 讲了，但归因到 freepdes 而非 direct_map |
| E.1 | 三架构 direct map BASE 差异 | 无 C 源码 | x86-64/arm64/riscv64 Sv39 各自的虚拟地址空间布局 | VM 06-pagetable-struct.md:802 已定义，07 未对接 |
| E.2 | 1GB huge page CPU 支持检查 | 无 C 源码 | CPUID.80000001H:EDX.GBPAGES，不支持回退 2MB | VM 06-pagetable-struct.md:808 已定义 |
| E.3 | Global 位（G=1）TLB 优化 | `pg_utils.c:243` vm_enable_paging PGE | CR3 切换不刷新 G=1 的 TLB 条目 | VM TODO.md:136 已讨论，07 未覆盖 |

---

## 二、诊断

### 2.1 纵向链路断裂点

| # | 断裂点 | 现状 | 修复方向 |
|---|--------|------|---------|
| 1 | **根叙事断裂**：07 讲 freepdes/ptproc，VM 已用 direct_map 替代 | 07 §1.0 标题"ptproc 与 freepdes" | 重写为"direct_map 替代 freepdes"，§1.0 讲跨空间访问本质 + 32位临时窗口→64位 direct_map 演进 |
| 2 | **pg_mapkernel 遗漏**：freepde_start 的来源未讲 | 07 §2.4 提到 kinfo.freepde_start 但未讲 pg_mapkernel | Ch2 补 pg_mapkernel 源码分析（内核映射建立 + 返回 freepde_start） |
| 3 | **direct_map 双视图未覆盖**：VM direct map + Kernel direct map | 07 完全未提 | Ch1/Ch3 补双视图概念，讲清硬件特权级要求 |
| 4 | **map_kernel 职责演进未对接**：VM 的 map_kernel 建立 Kernel direct map | 07 未提 | Ch3 补 map_kernel 职责对比表（Minix3 pt_mapkernel vs Rust map_kernel） |
| 5 | **DirectMapArch trait 未对接**：VM 已实现，07 应使用 | 07 自造 PostInitArch/MemoryInitArch | Ch3/Ch4 改为对接 DirectMapArch，废弃 MemoryInitArch |
| 6 | **初始页表建立者未讲**：kernel 为 VM 建初始页表（含 VM direct map） | 07 未提 | Ch1/Ch3 补初始页表 4 页结构 + 建立者时序 |
| 7 | **08 文档反向引用断裂**：08 §1.3 写"06: ptproc 已设置、freepdes 已分配" | 08 引用旧叙事 | 07 重写后同步修正 08 §1.3 |
| 8 | **阶段 D 定位断裂**：direct_map 下 freepdes/ptproc 废弃，阶段 D 剩什么？ | 07 现有定位"ptproc=VM、freepdes 分配" | redesign：阶段 D 重新定位为"记录 VM 页表信息 + 确认 direct map 就绪"或吸收/简化 |

### 2.2 开发文档味

| # | 位置 | 旧叙事 | 修复方向 |
|---|------|--------|---------|
| 1 | §3.5 "Rust 实现状态（2026-06-21）" | 进度跟踪 + ✅🚧🔒 emoji | 删除进度跟踪，用假设性推理替代 |
| 2 | §3.5 "ptproc per-CPU 变量未实现" | "待实现"叙事 | 改为"如果用 ptproc 临时窗口，会泄漏 32 位模型；用 direct_map 重新表达" |
| 3 | §4.4 "当前状态"表 | ✅/🚧/TODO 状态标记 | 删除状态标记，讲机制本质 |
| 4 | §4.5 "为何不聚合为 KernelState" | 实现选型讨论 | 移到 Ch3 假设性推理或删除 |
| 5 | §5 测试表"状态"列 | ✅/DEFERRED | 测试要点应讲覆盖什么，不是进度 |
| 6 | §3.6 "设计决策的替代方案" | 部分是开发选型 | 保留本质推理，删除实现细节 |

### 2.3 知识点遗漏

| # | 遗漏知识点 | 来源 | 应放章节 |
|---|-----------|------|---------|
| 1 | pg_mapkernel() 源码分析 | `pg_utils.c:186` | Ch2 |
| 2 | Minix3 全线无 direct map 的事实 | VM 06-pagetable-struct.md:685 | Ch1/Ch2 |
| 3 | direct_map 是 Rust 全新引入 | VM 06-pagetable-struct.md:689 | Ch1/Ch3 |
| 4 | 双视图地址空间（VM/Kernel direct map） | VM 06-pagetable-struct.md:693 | Ch1 |
| 5 | DirectMapArch trait | VM 06-pagetable-struct.md:776 | Ch3/Ch4 |
| 6 | map_kernel 职责演进 | VM 07-pagetable-ops.md:1218 | Ch3 |
| 7 | 初始页表 4 页结构 + 建立者 | VM 06-pagetable-struct.md:741 | Ch1/Ch3 |
| 8 | Kernel direct map 的 G=1 TLB 优化 | VM TODO.md:136 | Ch3 |
| 9 | 1GB huge page CPU 支持检查 | VM 06-pagetable-struct.md:808 | Ch3 |

### 2.4 Rust 实现错误点（redesign 候选）

| # | 错误点 | 现状 | redesign 方向 |
|---|--------|------|--------------|
| 1 | MemoryInitArch trait + allocate_free_pdes() | 翻译 freepdes | **废弃**：direct_map 不需要 freepdes |
| 2 | FreePdeSlots 结构体 | 翻译 freepdes[] | **废弃** |
| 3 | FREE_PDE_SLOTS static mut | 全局可变状态 | **废弃** |
| 4 | FREE_UPPER_IDX AtomicUsize | 翻译 kinfo.freepde_start | **废弃** |
| 5 | PostInitArch::set_ptproc() 占位 | no-op | **重定义**：direct_map 下记录 VM 页表 root（satp/CR3），不再设 ptproc |
| 6 | VmPageTableInfo.virt_root | 翻译 p_cr3_v | **重审**：direct_map 下 kernel 用 kernel_phys_to_virt，不依赖 virt_root |
| 7 | CrossSpaceInit.free_pde_slots 字段 | 聚合 freepdes | **删除**该字段 |
| 8 | 整个 init_post_and_memory() 流程 | 翻译 arch_post_init+memory_init | **重写**为 direct_map 语义 |

### 2.5 概念组覆盖矩阵（→ 章节映射，待 outline 填充）

| 概念组 | Ch1 概述 | Ch2 C源码 | Ch3 Rust设计 | Ch4 实现 | Ch5 测试 |
|--------|---------|----------|-------------|---------|---------|
| A 跨空间访问问题 | ? | ? | ? | ? | ? |
| B 阶段D C源码 | ? | ? | ? | ? | ? |
| C direct_map设计 | ? | ? | ? | ? | ? |
| D Rust实现现状 | ? | ? | ? | ? | ? |
| E 架构演进 | ? | ? | ? | ? | ? |

> "?" 待 outline.md 填充具体小节。

---

## 三、redesign 核心方向

### 3.1 文档定位重定义

| 维度 | 旧定位 | 新定位 |
|------|--------|--------|
| 主题 | ptproc + freepdes 临时窗口 | direct_map 替代 freepdes：跨空间访问的架构演进 |
| Ch1 主语 | ptproc / freepdes（函数/变量名） | CPU / 地址空间 / 特权级（机制本质） |
| 阶段 D 语义 | 设 ptproc=VM + 分配 freepdes | 记录 VM 页表 root + 确认 direct map 就绪（freepdes/ptproc 废弃） |
| 叙事弧 | 翻译 C 的两行代码 | 32位临时窗口→64位 direct_map 的架构演进 + rewrite not translate |

### 3.2 关键设计决策（待 outline 展开）

1. **freepdes/ptproc 整体废弃**：direct_map 下 kernel 有 Kernel direct map，不需要借页目录放临时映射
2. **对接 DirectMapArch trait**：07 不自造 PostInitArch/MemoryInitArch，改用 VM 已实现的 DirectMapArch
3. **map_kernel 职责对接**：Kernel direct map 由 VM 的 map_kernel 建立，07 讲清此协作关系
4. **阶段 D 简化**：arch_post_init 的 pg_info 等价（记录 VM 页表 root）保留，memory_init 整体废弃
5. **假设性推理**：如果翻译 Minix3 的 freepdes，会泄漏 32 位临时窗口模型到 64 位 OS 层

### 3.3 待用户确认的设计问题

| # | 问题 | 选项 | 倾向 |
|---|------|------|------|
| Q1 | 阶段 D 在 direct_map 下是否还需要独立文档？ | (a) 保留为独立文档，讲 direct_map 替代 + 阶段D简化 (b) 合并到 06 或 08 | (a) 保留，因为跨空间访问是独立主题 |
| Q2 | ptproc per-CPU 变量是否完全废弃？ | (a) 完全废弃 (b) 保留用于"指向当前页表 root"但访问走 direct_map | (a) 完全废弃 |
| Q3 | VmPageTableInfo 是否保留？ | (a) 保留 phys_root（satp/CR3），删 virt_root (b) 整体废弃 | (a) 保留 phys_root |
| Q4 | init_post_and_memory() 重写后做什么？ | (a) 仅记录 VM 页表 root (b) 完全删除阶段 D | (a) 仅记录 VM 页表 root |
