# 07-outline.md — 跨地址空间初始化（Design Review / Redesign）

> **生成时间**: 2026-06-23
> **基于**: `07-structure.md` 诊断 + VM server direct_map 决策（02-stage-vm/06-pagetable-struct.md §3.6, 07-pagetable-ops.md §3.0.7）
> **Redesign 性质**: 现有文档根叙事（ptproc + freepdes）是 Minix3 的 32 位临时窗口设计；VM server 已决策用 64 位 direct_map 替代。本文档是 rewrite-not-translate 的典范——不翻译 freepdes，用 direct_map 重新表达跨地址空间访问。
> **用户决策**: 保留独立文档讲演进+简化；ptproc 完全废弃；VmPageTableInfo 整体废弃；init_post_and_memory 改为确认 direct_map 就绪。

---

## Ch1. 概述（概念导向，建立心智模型）

> Ch1 主语：CPU / 地址空间 / 特权级 / 矛盾。禁止函数名/结构体名作主语。

### 1.0 本章讲什么

- **讲什么**：
  - 全景表：kmain 六阶段中阶段 D 的位置（A 入口→B cstart→C 进程表→**D post-init**→E system→F finish）
  - 核心矛盾：内核运行在自己的地址空间，却要读写用户进程的内存（IPC 消息拷贝、vm_memset 等服务）——跨地址空间访问是内核的核心能力
  - 两种解法的架构演进：32 位临时窗口（借页目录放临时映射）→ 64 位 direct_map（建立 pa→va 固定偏移映射）
  - 本章立场：minix-rs 选择 direct_map，废弃 32 位的临时窗口机制；阶段 D 从"分配临时窗口"简化为"确认 direct_map 就绪"
- **知识点**：A.0、A.5、B.0、C.0、C.1
- **教学要点**：
  - 本质一句话：跨地址空间访问是"内核如何看到用户内存"的问题，解法随虚拟地址空间位宽演进
  - 读者为什么应该关心：这是内核运行时核心能力的基础；理解 32 位→64 位的架构演进能避免把历史包袱带进新设计
  - 与 C/Rust 的对应：C 用 freepdes/ptproc（32 位），Rust 用 direct_map（64 位全新引入）
- **目标读者**：已理解 Minix3 微内核基本结构、了解 x86 分页机制的开发者
- **本章不讲什么**：
  - direct_map 的完整建立过程（VM server 06-pagetable-struct.md §3.6 已详述，本文仅引用）
  - createpde() 的运行时使用（后续 24-cross-space-runtime.md）
  - map_kernel() 的完整实现（VM server 07-pagetable-ops.md §3.0.7，本文仅讲协作关系）

### 1.1 核心矛盾：内核如何"看"别人的地址空间

- **讲什么**：
  - 每个进程有独立页表（CR3 装的是当前进程的页表根）——这是 OS 核心机制
  - 内核映射在每个进程的地址空间里（中断/系统调用进入 ring 0 时必须能执行内核代码）——所以进程切换时内核段始终可见
  - 但内核要访问**别的**进程的用户态内存时，目标内存不在当前页表的用户空间映射里——这就是跨地址空间访问问题
  - 灵魂本质：跨地址空间访问 = "CPU 当前页表解释不了目标 VA，如何临时让它解释得了"
- **知识点**：A.0、A.5
- **教学要点**：
  - 本质一句话：进程隔离让每个进程有自己的用户空间视图，内核服务却要穿越这个隔离
  - 读者为什么应该关心：IPC、fork、exec 都依赖此能力
  - 与 C/Rust 的对应：C 的 lin_lin_copy/vm_memset → createpde；Rust 的 direct_map

### 1.2 32 位解法：临时窗口（历史包袱）

- **讲什么**：
  - 32 位虚拟地址空间只有 4GB，内核和用户共享，没有余量建立全物理内存的固定映射
  - 解法：在当前页目录里预留 2 个槽位（freepdes），需要访问目标进程内存时，把目标 PDE 临时写入槽位，用完清掉
  - "借谁的页目录"：借当前 CPU 正在用的页目录（ptproc 指向它），因为 MMU 只按 CR3 装的页目录解释 VA
  - 三个丑陋之处：①污染当前页目录视图（写完要清）②TLB 反复 flush ③4MB 粒度限制
  - 灵魂本质：临时窗口 = 在别人的页目录上开两个临时后门，用完堵上
- **知识点**：A.1、A.2、B.2、B.3、B.4、B.5、B.6、B.8
- **教学要点**：
  - 本质一句话：32 位地址空间太挤，只能"借"不能"建"
  - 读者为什么应该关心：理解历史包袱才能理解为什么 64 位要换方案
  - 与 C/Rust 的对应：C 的 freepdes/ptproc/createpde/mem_clear_mapcache；Rust 废弃全部
- **架构范围标注**：x86-32 特有设计（4MB 大页、1024 项页目录）；64 位不沿用

### 1.3 64 位解法：direct_map（现代方案）

- **讲什么**：
  - 64 位虚拟地址空间 256TB+，有余量建立"全物理内存 → 固定虚拟地址区间"的线性映射（va = pa + BASE）
  - 内核要访问任意物理内存时，直接 kernel_phys_to_virt(pa) 得到 VA，MMU 按 direct map 的 PTE 解释——不需要临时窗口、不污染任何页目录、不需要清理
  - 双视图地址空间（硬件特权级要求）：同一物理内存需要两个窗口——VM direct map（U/S=1，VM 用户态用）+ Kernel direct map（U/S=0，内核态用）。x86-64 的 U/S 位不能同时 0 和 1，两个窗口是硬件必然要求，不是冗余
  - 灵魂本质：direct_map = 给所有物理内存一个永久的虚拟地址，临时窗口的"借/还"整个消失
- **知识点**：A.3、A.4、C.0、C.1、C.2、C.3、C.4、E.0、E.1
- **教学要点**：
  - 本质一句话：64 位地址空间够大，"临时借"升级为"永久有"
  - 读者为什么应该关心：这是 Linux/Windows/BSD 主流内核的通用模式，minix-rs 与主流对齐
  - 与 C/Rust 的对应：C 无（Minix3 全线无 direct map）；Rust 全新引入 DirectMapArch trait
- **架构范围标注**：三架构统一抽象（DirectMapArch），BASE 值各架构不同

### 1.4 阶段 D 的位置与简化

- **讲什么**：
  - 阶段 D 在 C 里是两行代码：arch_post_init（设 ptproc + pg_info）+ memory_init（分配 freepdes）
  - direct_map 下这两行的语义变化：ptproc 不再需要（kernel 有自己的 direct map，不借页目录）；freepdes 不再需要（direct map 是永久映射）；pg_info 记录的 bootstrap 页表地址不再有同样作用
  - 阶段 D 简化为：确认 direct_map 已就绪（初始页表由 kernel 在阶段 C 建立 VM 进程时建好，含 VM direct map）
  - 灵魂本质：阶段 D 从"分配临时窗口"降级为"确认永久窗口已开"
- **知识点**：B.0、B.1、B.4、C.6、D.7
- **教学要点**：
  - 本质一句话：direct_map 让阶段 D 几乎无事可做，但"确认就绪"本身是必要的安全检查
  - 读者为什么应该关心：理解 kmain 流程为何能简化
  - 与 C/Rust 的对应：C 两行代码；Rust 一个确认步骤

### 1.5 本章小结

- **讲什么**：
  - 跨地址空间访问是内核核心能力，解法随位宽演进
  - 32 位临时窗口（freepdes/ptproc）是地址空间受限的妥协；64 位 direct_map 是地址空间充裕的自然解
  - minix-rs 选择 direct_map，阶段 D 简化为确认就绪
  - 关键不变量：direct_map 建立后只读不变（VM 不再修改 Kernel direct map 的 PTE）
- **知识点**：A.0-E.3 综述
- **教学要点**：rewrite not translate 的典范——不翻译历史包袱，用现代方案重新表达

---

## Ch2. C 源码分析（Ground Truth）

> 每节标注 file:line 锚点。本章讲 Minix3 的 32 位实现，作为"被替代的历史方案"分析。

### 2.1 arch_post_init()：设置 ptproc + 记录页表地址

- **讲什么**：
  - `protect.c:370-377` (x86) / `protect.c:97-104` (ARM) 三步：proc_addr(VM) → ptproc=VM → pg_info()
  - 为什么是 VM：VM 是第一个拥有完整页表的进程；从此刻到 VM 通过 VMCTL_SETADDRSPACE 切换到自己页表前，内核和 VM 共享 bootstrap 页表
  - ptproc 是 per-CPU 变量（`cpulocals.h:56`），记录"当前 CR3 装的是谁"
- **知识点**：B.0、B.8
- **教学要点**：ptproc 的本质是"当前页目录归属"，freepdes 借此页目录放临时映射

### 2.2 pg_info()：记录 bootstrap 页表地址

- **讲什么**：
  - `pg_utils.c:312-316`：*pagedir_ph=vir2phys(pagedir); *pagedir_v=pagedir
  - 把全局 pagedir（bootstrap 页目录）的物理/虚拟地址写入 VM 的 p_seg.p_cr3/p_cr3_v
  - VM 运行后会通过 VMCTL_SETADDRSPACE 替换为自己的页表，所以 pg_info 记录的是初始页表地址
- **知识点**：B.1
- **教学要点**：pg_info 是"告诉 VM 你的初始页表在哪"，direct_map 下此语义变化

### 2.3 pg_mapkernel()：建立内核映射 + 返回 freepde_start（原07遗漏，新增）

- **讲什么**：
  - `pg_utils.c:186-206` (x86) / `earm/pg_utils.c:184` (ARM)：用 4MB 大页映射内核代码/数据段（kern_vir_start → kern_phys_start，长度 kern_kernlen）
  - 返回映射之后的第一个空闲 PDE 索引——这就是 freepde_start 的来源
  - `pre_init.c:232`：kinfo.freepde_start = pg_mapkernel()——在 pre_init 阶段（比阶段 D 更早）就记录了
  - 这是"内核映射在每个进程地址空间"的 C 实现：每个进程页表的高位 PDE 都指向内核
- **知识点**：B.2、B.3、A.5
- **教学要点**：pg_mapkernel 是 freepde_start 的源头，原07文档完全遗漏；direct_map 下内核映射变成 Kernel direct map（1GB huge page），freepde_start 不再需要

### 2.4 memory_init()：分配 freepdes

- **讲什么**：
  - `memory.c:707-717` (x86) / `memory.c:612-622` (ARM)：freepdes[nfreepdes++]=kinfo.freepde_start++ ×2
  - 为什么是 2 个：createpde 的调用者 virtual_copy_f/vm_memset 需要源和目标两个临时映射
  - assert(kinfo.freepde_start < I386_VM_DIR_ENTRIES) 越界检查
- **知识点**：B.4
- **教学要点**：memory_init 是"领取两个临时窗口槽位"，direct_map 下整体废弃

### 2.5 createpde() + mem_clear_mapcache()：临时窗口的使用与清理（导览）

- **讲什么**：
  - `memory.c:80-145` createpde：若目标==ptproc 或内核→直接返回 VA；否则读目标 PDE 写入 freepdes 槽位，返回临时窗口 VA
  - `memory.c:35-50` mem_clear_mapcache：用完清空 freepdes 槽位，防残留
  - BKL 下安全性：createpde 修改全局页目录项，但 BKL 保证单 CPU 执行
- **知识点**：B.5、B.6
- **教学要点**：createpde 是 freepdes 的唯一消费者，完整实现见后续 24-cross-space-runtime.md；direct_map 下 createpde 退化为 kernel_phys_to_virt 一行加法

### 2.6 IPCNAME 调试宏（阶段 D 中间夹，精简）

- **讲什么**：
  - `main.c:277-290`：IPC 调用类型编号→字符串映射，仅调试用（proc.c:497 打印 IPC 统计）
  - 与跨空间访问主题无关，仅因时序位置在阶段 D 中间被提及
  - Rust 替代：enum IpcCall + impl Display，无需全局数组
- **知识点**：B.7
- **教学要点**：可精简为一段说明，不展开

---

## Ch3. Rust 设计决策（本质机制，约束驱动）

> 每节用"如果 X 设计，会有 Y 问题，所以用 Z"的假设性推理。禁止"旧版/最初/后来"叙事。

### 3.1 本质：direct_map 替代 freepdes（rewrite not translate）

- **讲什么**：
  - 本质：跨地址空间访问的解法随位宽演进，64 位下 direct_map 是自然解
  - 约束驱动：64 位虚拟地址空间充裕（256TB+），可以建立全物理内存的固定映射；no_std 下避免全局可变状态
  - 假设性推理：如果翻译 Minix3 的 freepdes/ptproc，会泄漏 32 位临时窗口模型到 64 位 OS 层——污染页目录视图、需要清理、TLB 反复 flush、4MB 粒度限制全部继承，且 64 位地址空间本可避免这些
  - 决策：废弃 freepdes/ptproc/memory_init 整套，用 direct_map 重新表达
- **知识点**：A.3、C.0、C.1、D.1、D.2、D.4、D.5
- **教学要点**：rewrite not translate 的典范——不是改 freepdes 的实现，是换整个机制

### 3.2 双视图地址空间：VM direct map + Kernel direct map

- **讲什么**：
  - 本质：同一物理内存需要两个窗口，因为 x86-64 的 U/S 位不能同时 0 和 1
  - VM direct map（U/S=1，VM 用户态用）：kernel 在创建 VM 进程时建立（VM 还没运行）
  - Kernel direct map（U/S=0, G=1，内核态用）：VM 启动后通过 map_kernel 建立
  - 假设性推理：如果只有一个 direct map，要么 VM 访问不了（U/S=0）要么内核安全降级（U/S=1）——硬件特权级要求两个窗口
  - 关键不变量：Kernel direct map 建立后只读不变（G=1，CR3 切换不刷新 TLB）
- **知识点**：A.4、C.3、C.4、E.3
- **教学要点**：双视图不是冗余，是硬件特权级的必然要求

### 3.3 对接 DirectMapArch trait（不自造 PostInitArch/MemoryInitArch）

- **讲什么**：
  - 本质：direct_map 的核心是地址空间布局（va = pa + BASE），跨架构仅 BASE 不同
  - 约束驱动：VM server 已实现 DirectMapArch trait（06-pagetable-struct.md §3.6.3），kernel 侧应直接对接，避免重复抽象
  - 假设性推理：如果 kernel 自造 PostInitArch/MemoryInitArch trait，会与 VM 的 DirectMapArch 形成两套抽象，违反"硬件抽象唯一性"原则
  - 决策：废弃 PostInitArch/MemoryInitArch/FreePdeSlots/VmPageTableInfo，kernel 侧通过 DirectMapArch::kernel_phys_to_virt(pa) 直接访问
- **知识点**：C.2、D.0、D.1、D.2、D.3
- **教学要点**：硬件抽象要唯一，direct_map 的抽象已在 VM 层定义，kernel 层对接而非重造

### 3.4 map_kernel 职责演进：内核映射的建立者

- **讲什么**：
  - 本质：每个进程页表必须映射内核（中断/系统调用进入 ring 0 时能执行内核代码）
  - Minix3 的 pt_mapkernel（`pagetable.c:1442`）：内核代码/数据段 + page_directories 登记册 + freepdes 槽位预留
  - Rust 的 map_kernel（VM server）：内核代码/数据段 + Kernel direct map（不需要 page_directories，不需要 freepdes）
  - 假设性推理：如果保留 page_directories 登记册，会维护一套"哪些进程页目录映射了内核"的元数据——direct_map 下每个进程都有 Kernel direct map（map_kernel 建立），登记册冗余
  - 建立者时序：VM direct map 由 kernel 建立（阶段 C，VM 启动前）；Kernel direct map 由 VM 的 map_kernel 建立（VM 启动后）
- **知识点**：C.5、A.5、C.6
- **教学要点**：map_kernel 的职责随 direct_map 简化——加 Kernel direct map，去 page_directories/freepdes

### 3.5 阶段 D 简化：确认 direct_map 就绪

- **讲什么**：
  - 本质：direct_map 在阶段 C（kernel 建立 VM 初始页表）已就绪，阶段 D 从"分配临时窗口"降级为"确认就绪"
  - 约束驱动：direct_map 是永久映射，不需要运行时分配/清理；但"确认就绪"是必要的安全检查（避免运行时才发现 direct map 缺失）
  - 假设性推理：如果完全删除阶段 D，kmain 流程少一步，但失去"direct map 就绪"的显式断言点——运行时 createpde 等价函数（kernel_phys_to_virt）失败时难定位
  - 决策：阶段 D 保留为"确认 direct_map 就绪"的验证步骤，废弃 arch_post_init 的 ptproc/pg_info 和 memory_init 的 freepdes 分配
- **知识点**：B.0、B.1、B.4、C.6、D.7
- **教学要点**：阶段 D 不是"做事"而是"确认事已做好"——这是 direct_map 简化的自然结果

### 3.6 废弃清单与假设性推理汇总

- **讲什么**：
  - 废弃：ptproc per-CPU 变量、freepdes[]、memory_init()、PostInitArch trait、MemoryInitArch trait、FreePdeSlots、VmPageTableInfo、FREE_PDE_SLOTS static、FREE_UPPER_IDX static
  - 保留/对接：DirectMapArch trait（VM 已实现）、kernel_phys_to_virt()
  - 假设性推理表：每个废弃项的"如果保留会怎样"
- **知识点**：D.0-D.7
- **教学要点**：废弃不是删除代码，是换机制——每个废弃项都有 direct_map 的替代

---

## Ch4. 实现详解（HOW）

> 对应 Ch3 每个决策，讲具体接口/流程。本章较瘦，因为大量实现被废弃。

### 4.1 DirectMapArch 接口（引用 VM 已实现）

- **讲什么**：
  - DirectMapArch trait 定义（VM 06-pagetable-struct.md §3.6.3）：VM_DIRECT_MAP_BASE / KERNEL_DIRECT_MAP_BASE 常量 + vm_phys_to_virt / kernel_phys_to_virt / virt_to_phys 方法
  - 三架构 BASE 值（x86-64 / arm64 / riscv64 Sv39）
  - kernel 侧使用方式：通过 CurrentDirectMap 类型别名编译期选择实现
  - 1GB huge page CPU 支持检查（supports_1gb_page()，不支持回退 2MB）
- **知识点**：C.2、E.1、E.2
- **教学要点**：kernel 不重造 direct_map 抽象，直接用 VM 层定义的 trait

### 4.2 阶段 D 入口：确认 direct_map 就绪

- **讲什么**：
  - init_post_and_memory() 重写为确认步骤：验证 VM 进程的初始页表已含 VM direct map（阶段 C 建立）
  - 确认 KernelInfo 中 direct_map 相关字段已由 boot-shim 填充
  - 不再分配 freepdes、不再设 ptproc、不再记录 VmPageTableInfo
  - 流程：获取 VM 进程 → 确认其页表 root 有效 → 确认 direct map base 已配置 → 返回
- **知识点**：D.7、C.6
- **教学要点**：阶段 D 是"断言式"步骤，不是"建设式"步骤

### 4.3 废弃的 trait/结构体清单（迁移说明）

- **讲什么**：
  - 列出废弃项及其替代：PostInitArch→无（direct_map 不需要 set_ptproc）；MemoryInitArch→无（不分配 freepdes）；FreePdeSlots→无；VmPageTableInfo→无（kernel 用 DirectMapArch 直接访问）；FREE_PDE_SLOTS/FREE_UPPER_IDX static→无
  - 迁移影响：后续 24-cross-space-runtime.md 的 createpde 等价函数改用 kernel_phys_to_virt
  - 代码清理范围：os/arch/src/arch/post_init.rs、os/arch/src/{x86_64,arm64,riscv64}/post_init.rs、os/kernel/src/lib.rs 的 FREE_PDE_SLOTS/FREE_UPPER_IDX
- **知识点**：D.0-D.7
- **教学要点**：废弃清单是 redesign 的落地，每项都有 direct_map 替代或确认不需要

---

## Ch5. 测试要点

> 覆盖 Ch3+Ch4 的每个核心决策。

### 5.1 direct_map 就绪验证

- **讲什么**：
  - 测试：阶段 D 确认步骤能正确识别 direct map 已就绪（VM 进程页表 root 有效 + direct map base 已配置）
  - 测试：direct map 未就绪时确认步骤 panic（fail-fast）
  - 测试：kernel_phys_to_virt(pa) 返回正确 VA（对接 DirectMapArch 的 mock 实现）
- **知识点**：D.7、C.2
- **教学要点**：确认步骤本身需要测试，避免"确认"变成空操作

### 5.2 废弃路径不存在的断言

- **讲什么**：
  - 测试：freepdes 相关代码已删除（编译期保证：FreePdeSlots/PostInitArch/MemoryInitArch 不存在）
  - 测试：ptproc 相关代码已删除
  - 测试：VmPageTableInfo 不存在
- **知识点**：D.0-D.7
- **教学要点**：废弃的验证是"不存在"而非"存在"——编译期类型系统保证

### 5.3 DirectMapArch 对接测试

- **讲什么**：
  - 测试：kernel 侧通过 CurrentDirectMap 正确调用 vm_phys_to_virt/kernel_phys_to_virt
  - 测试：三架构 BASE 常量正确（x86-64/arm64/riscv64）
  - 测试：1GB huge page 不支持时回退 2MB
- **知识点**：C.2、E.1、E.2
- **教学要点**：direct_map 的测试在 VM 层已覆盖，kernel 侧仅测对接

---

## 附录

### A. 阶段 D 时序图（direct_map 版）

```
阶段 C: kernel 建立 VM 初始页表（含 VM direct map, 4页结构）
  ↓
阶段 D: 确认 direct_map 就绪（init_post_and_memory 简化版）
  - 确认 VM 进程页表 root 有效
  - 确认 direct map base 已配置
  - 不分配 freepdes、不设 ptproc
  ↓
阶段 E-F: system_init + bsp_finish_booting
  ↓
VM 启动后: map_kernel 建立 Kernel direct map（U/S=0, G=1）
```

### B. Minix3 vs minix-rs 阶段 D 对照

| 方面 | Minix3 C (32位) | minix-rs (64位 direct_map) |
|------|----------------|---------------------------|
| 跨空间访问机制 | freepdes 临时窗口 | direct_map 永久映射 |
| ptproc | per-CPU 变量，记录当前页目录 | 废弃 |
| memory_init | 分配 2 个 freepdes | 废弃 |
| pg_info | 记录 bootstrap 页表地址 | 废弃（direct_map 不依赖） |
| 阶段 D 内容 | 设 ptproc + 分配 freepdes | 确认 direct_map 就绪 |
| 内核映射建立 | pg_mapkernel (4MB 大页) | map_kernel (1GB huge page + Kernel direct map) |
| 建立者 | kernel (pre_init) | kernel (VM direct map) + VM (Kernel direct map) |

---

## Ch6. 参见

- [06-proc-init-boot-proc-new.md](06-proc-init-boot-proc-new.md) — 阶段 C：进程表初始化与 VM ELF 加载（含初始页表建立）
- [08-system-init-boot-finish.md](08-system-init-boot-finish.md) — 阶段 E-F：系统调用注册与启动完成
- [24-cross-space-runtime.md](24-cross-space-runtime.md) — 运行时跨空间访问（createpde 等价函数 = kernel_phys_to_virt）
- [02-stage-vm/06-pagetable-struct.md](../02-stage-vm/06-pagetable-struct.md) §3.6 — direct_map 完整设计（双视图、4页结构、DirectMapArch trait）
- [02-stage-vm/07-pagetable-ops.md](../02-stage-vm/07-pagetable-ops.md) §3.0.7 — map_kernel 职责简化
- `minix3/minix/kernel/arch/i386/protect.c:370-377` — arch_post_init
- `minix3/minix/kernel/arch/i386/pg_utils.c:186-206` — pg_mapkernel
- `minix3/minix/kernel/arch/i386/pg_utils.c:312-316` — pg_info
- `minix3/minix/kernel/arch/i386/memory.c:707-717` — memory_init
- `minix3/minix/kernel/arch/i386/memory.c:35-145` — mem_clear_mapcache + createpde
- `minix3/minix/kernel/cpulocals.h:56` — ptproc per-CPU 变量

---

## 大纲完整性自检

### 知识点覆盖矩阵（A.0-E.3 → 章节）

| 概念组 | 知识点 | Ch1 | Ch2 | Ch3 | Ch4 | Ch5 |
|--------|--------|-----|-----|-----|-----|-----|
| A 跨空间访问 | A.0 矛盾 | §1.1 | — | §3.1 | — | — |
| | A.1 32位临时窗口 | §1.2 | §2.4-2.5 | §3.1 | — | — |
| | A.2 ptproc | §1.2 | §2.1 | §3.1 | §4.3 | §5.2 |
| | A.3 64位 direct_map | §1.3 | — | §3.1 | §4.1 | — |
| | A.4 双视图 | §1.3 | — | §3.2 | — | — |
| | A.5 内核映射每进程 | §1.1 | §2.3 | §3.4 | — | — |
| B C源码 | B.0 arch_post_init | §1.4 | §2.1 | §3.5 | §4.2 | — |
| | B.1 pg_info | — | §2.2 | §3.5 | §4.3 | — |
| | B.2 pg_mapkernel | — | §2.3 | §3.4 | — | — |
| | B.3 freepde_start | — | §2.3 | §3.5 | — | — |
| | B.4 memory_init | §1.4 | §2.4 | §3.5 | §4.3 | §5.2 |
| | B.5 createpde | §1.2 | §2.5 | §3.1 | — | — |
| | B.6 mem_clear_mapcache | §1.2 | §2.5 | §3.1 | — | — |
| | B.7 IPCNAME | — | §2.6 | — | — | — |
| | B.8 ptproc per-CPU | §1.2 | §2.1 | §3.1 | §4.3 | §5.2 |
| C direct_map | C.0 Minix3无direct_map | §1.3 | — | §3.1 | — | — |
| | C.1 Rust全新引入 | §1.3 | — | §3.1 | — | — |
| | C.2 DirectMapArch trait | §1.3 | — | §3.3 | §4.1 | §5.3 |
| | C.3 VM direct map | §1.3 | — | §3.2 | — | — |
| | C.4 Kernel direct map | §1.3 | — | §3.2 | — | — |
| | C.5 map_kernel演进 | — | — | §3.4 | — | — |
| | C.6 初始页表kernel建 | §1.4 | — | §3.5 | §4.2 | — |
| | C.7 4页结构 | — | — | §3.2 | — | — |
| D Rust现状 | D.0 PostInitArch | — | — | §3.3 | §4.3 | §5.2 |
| | D.1 MemoryInitArch | — | — | §3.3 | §4.3 | §5.2 |
| | D.2 VmPageTableInfo | — | — | §3.3 | §4.3 | §5.2 |
| | D.3 CrossSpaceInit | — | — | §3.3 | §4.3 | §5.2 |
| | D.4 FreePdeSlots | — | — | §3.1 | §4.3 | §5.2 |
| | D.5 FREE_PDE_SLOTS | — | — | §3.1 | §4.3 | §5.2 |
| | D.6 FREE_UPPER_IDX | — | — | §3.1 | §4.3 | §5.2 |
| | D.7 init_post_and_memory | §1.4 | — | §3.5 | §4.2 | §5.1 |
| E 架构演进 | E.0 32→64位页表 | §1.2 | — | §3.1 | — | — |
| | E.1 三架构BASE | §1.3 | — | §3.3 | §4.1 | §5.3 |
| | E.2 1GB huge page | — | — | §3.3 | §4.1 | §5.3 |
| | E.3 G=1 TLB优化 | — | — | §3.2 | — | — |

> 覆盖率：35/35 知识点全部映射到章节，无遗漏。

### 断裂修复验证

| # | 断裂点（来自 structure.md） | 修复章节 | 状态 |
|---|---------------------------|---------|------|
| 1 | 根叙事断裂（freepdes→direct_map） | §1.0/§1.3/§3.1 重写根叙事 | ✅ |
| 2 | pg_mapkernel 遗漏 | §2.3 新增 | ✅ |
| 3 | direct_map 双视图未覆盖 | §1.3/§3.2 新增 | ✅ |
| 4 | map_kernel 职责演进未对接 | §3.4 新增 | ✅ |
| 5 | DirectMapArch trait 未对接 | §3.3/§4.1 新增 | ✅ |
| 6 | 初始页表建立者未讲 | §1.4/§3.5/§4.2 新增 | ✅ |
| 7 | 08 文档反向引用断裂 | 待 07 重写后同步修 08 §1.3 | ⏳ 正文阶段 |
| 8 | 阶段 D 定位断裂 | §1.4/§3.5/§4.2 重新定位 | ✅ |

### 开发文档味修复验证

| # | 旧叙事 | 新叙事 | 状态 |
|---|--------|--------|------|
| 1 | "Rust 实现状态（2026-06-21）" + emoji | 假设性推理（§3.1/§3.5） | ✅ |
| 2 | "ptproc 未实现" | "如果用 ptproc 会泄漏 32 位模型"（§3.1） | ✅ |
| 3 | "当前状态"表 + ✅🚧 | 机制本质讲解（§4.2/§4.3） | ✅ |
| 4 | "为何不聚合 KernelState" | 移除（direct_map 下无此问题） | ✅ |
| 5 | 测试表"状态"列 ✅/DEFERRED | 测试覆盖什么（§5.1-5.3） | ✅ |
| 6 | "设计决策替代方案"实现选型 | 假设性推理（§3.1-3.6） | ✅ |
