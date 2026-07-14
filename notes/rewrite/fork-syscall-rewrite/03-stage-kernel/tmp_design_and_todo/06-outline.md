# 06-outline.md — 06-proc-init-boot-proc.md 重写细化大纲

> **目的**：在重写 06 文档前，先列出完善的、细化的、每小节明确知识点的大纲。确保教学性、本质理论深度、读者友好性、知识点零遗漏。
>
> **组织原则**：
> 1. **概念导向**：Ch1 从 CPU/OS 视角建立心智模型，不讲函数名/结构名
> 2. **本质机制**：Ch3 讲 Rust rewrite 捕获的**本质**（非 translate 表面），约束驱动设计
> 3. **纵向链路完整**：Ch1 概念 → Ch2 C 源码 → Ch3 Rust 设计 → Ch4 实现 → Ch5 测试，每概念组全链路覆盖
> 4. **去开发文档味**：假设性推理（"如果 X 设计，会有 Y 问题"）替代迭代历史（"旧版/最初/后来"）
> 5. **架构范围标注**：x86-only / aarch64 / riscv64 / 三架构统一，每处明确标注
>
> **知识点编号**：A.0-O.5，对应 `06-structure.md` 概念组。每节标注覆盖的知识点。
>
> **缺口修复标记**：[NEW] 表示当前文档缺失、本大纲新增的章节。

---

## Ch1. 概述（概念导向，建立心智模型）

> **本章目标**：从 CPU/OS 视角回答"阶段 C 要解决什么问题"，建立读者心智模型。读者读完本章应理解：进程是什么、为什么需要进程表/特权表/RTS 标志、VM 鸡生蛋问题、CPU 要回答的四个问题、boot 流程的起点与终点。**不讲函数名、不讲 Rust 类型**。

### 1.0 本章讲什么
- **讲什么**：6 阶段启动全景（A-F），本章聚焦阶段 C；阶段 C 的任务定义（清空进程表/特权表 → 遍历 boot image → 加载 VM ELF）；本章不讲什么（阶段 A/B/D/E/F 详见其他文档）
- **知识点**：D.5（阶段 C 在 boot 流程中的位置）
- **教学要点**：用全景表定位阶段 C，让读者知道"自己在哪"

### 1.1 进程的本质 [NEW，补 A.0 断裂]
- **讲什么**：
  - 进程的本质：**CPU 时间的"断点续传"**
    - CPU 只有单执行流，OS 通过保存/恢复完整状态，让多个执行流轮流占用 CPU，营造出"同时运行"的幻象
    - 进程表就是保存每个"断点"的存档数据结构
  - 核心矛盾：单 CPU 单执行流 vs OS 多进程抽象
  - 矛盾的桥梁：进程表保存每个进程被暂停时的完整状态——寄存器、调度属性、IPC 状态、VM 状态
  - 进程在内核眼中的具体组成：slot + 寄存器状态 + 调度属性 + IPC 状态 + VM 状态
- **知识点**：A.0（进程概念抽象）
- **教学要点**：用"断点续传"建立直觉，再落到数据结构；先建立"进程是什么"，再讲"为什么需要进程表"

### 1.2 CPU 要回答的四个问题（三架构统一框架）
- **讲什么**：
  - boot 一个进程，CPU 必须回答四个问题：
    1. 寄存器初值是什么？（通用寄存器 + PC + SP + 状态寄存器）
    2. 用户态入口在哪？（ELF entry point）
    3. 栈和参数怎么传？（SP + ps_strings）
    4. 第一个进程的地址空间谁建？（VM 的 bootstrap 页表）
  - 三架构对照表（x86-64 / aarch64 / riscv64）回答四问
  - 状态寄存器（PSW/PSR/sstatus）是什么：保存条件标志、中断使能、当前特权级
  - ps_strings 是什么：放在栈顶的小型结构体，启动代码用它定位 argv/envp
    - **栈布局示意**：栈顶依次存放 argc、argv 指针、envp 指针、ps_strings 结构体，SP 最终指向 argc
  - **架构范围标注**：x86 的"段选择子"是 x86 段机制遗留，aarch64/riscv64 无此概念
- **知识点**：E.1（CPU 四问）、E.5（三架构状态寄存器初值）、E.6（ps_strings 机制）、K.1（三架构差异点）
- **教学要点**：从 CPU 视角提取统一框架，再对照三架构差异——这是"概念导向"的典范；ps_strings 用最小栈图建立空间直觉

### 1.3 三件套：进程表、特权表、RTS 位图
- **讲什么**：
  - 用 CPU 四问框架理解三件套：
    - **进程表**回答"进程是谁"：slot 布局、关键标识（p_nr / p_endpoint）、slot 生命周期
    - **特权表**回答"进程被允许做什么"：静态能力（类型标志 / trap_mask / ipc_to / k_call_mask / io_tab / irq_tab / sig_mgr）
    - **RTS 位图**回答"进程现在能不能跑"：动态状态，非零 = 不可运行
  - 进程表细节：
    - slot 布局：前 NR_TASKS 项是内核 task（p_nr<0），后 NR_PROCS 项是用户进程（p_nr≥0）
    - `p_nr`：进程号 = slot 索引，负数是内核 task，非负是用户进程
    - `p_endpoint`：端点号 = (generation, p_nr)，防陈旧引用（进程退出复用 slot 时 generation 递增）
    - slot 生命周期：SLOT_FREE → 分配 → 运行/阻塞/停止 → 退出 → 回收回 SLOT_FREE
  - 特权表细节：
    - **为什么特权独立于进程表**：特权是稀缺资源（NR_SYS_PROCS << NR_PROCS），只有系统进程需要独立 priv，普通用户进程共享默认特权。塞进每个 proc 会浪费内存
    - 分配方式：静态分配（boot 期系统进程）vs 动态分配（运行时普通进程）
    - 阶段 C 的 boot 循环：内核 task/RS/VM 立即获静态特权；其他用户进程标"无特权"等 RS 运行时配置
  - RTS 位图细节：
    - **为什么用位图而非枚举**：进程可同时因多个原因不可运行（如无特权 + 等 VM 建页表），位图支持多原因叠加，枚举不行
    - "A process is runnable iff p_rts_flags == 0" 不变量
    - 阶段 C 涉及的关键位：SLOT_FREE / NO_PRIV|NO_QUANTUM / VMINHIBIT|BOOTINHIBIT / PROC_STOP
    - RTS_SET/UNSET 自动维护调度队列一致性（设标志→dequeue，清标志→enqueue）
    - misc_flags 与 RTS 的区别：RTS 决定可运行性（影响调度队列），misc_flags 记录次要运行时状态（不影响调度）
- **知识点**：A.1-A.6（进程表布局/标识符/生命周期）、B.1-B.5（特权表）、B.12（boot 期授予）、C.1-C.4（RTS 位图）、C.6-C.8（misc_flags/P_BLOCKEDON/proc_kernel_scheduler）
- **教学要点**：用"静态能力 vs 动态状态"切分特权表与 RTS；p_endpoint 的 generation 机制是分布式系统"陈旧引用"防御；位图 vs 枚举讲清"多原因叠加"本质

### 1.4 boot image 与 VM 的"开天辟地"问题
- **讲什么**：
  - boot image = 编译时硬编码的进程清单
    - **为什么编译时硬编码**：boot 期无文件系统，清单必须编译进内核镜像（bootstrapping 的另一面）
    - 三类进程：内核 task（CLOCK/SYSTEM/IDLE，负 p_nr）/ 系统进程（VM/PM/VFS/RS，multiboot 提供 ELF）/ 普通用户进程（INIT，RS 运行时加载）
    - NR_BOOT_PROCS vs NR_BOOT_MODULES 的区别（前者含内核 task，后者只含用户态模块）
    - schedulable 判定：内核 task + RS + VM 立即可调度；其他用户进程需等 RS 运行时设特权
  - VM（Memory Manager）负责管理地址空间，但 VM 自己也需要地址空间才能运行——鸡生蛋问题
    - 解决：内核手工用 bootstrap 页表"抱"VM 起来
    - **bootstrap 页表的三部分映射**（跨三阶段生长）：
      - 恒等映射（VA=PA，pre_init/prot_init 建立，内核访问 LAPIC/显存）
      - 内核高半区映射（内核代码运行于此）
      - VM 用户态映射（阶段 C 添加，VM 代码运行于此）
    - 通用 OS 设计模式：**第一个进程必须由内核手工创建，然后它才能创建更多进程**
- **知识点**：D.1-D.4（boot image）、D.8（schedulable）、F.1（鸡生蛋）、F.4（bootstrap 页表三部分）
- **教学要点**：bootstrapping 的"自举"本质——最早的东西必须"自带"；VM 鸡生蛋是 OS 设计的普遍模式，不是 Minix3 特例

### 1.5 阶段 C 的执行节拍：从白板到就位但暂停
- **讲什么**：
  - 把前面概念串成执行顺序：先清空，再填充
  - **第一步：清空**（C 中对应 `proc_init()`）
    - 清空进程表：所有 slot 设 SLOT_FREE，设 p_nr 与初始 p_endpoint
    - 清空特权表：建立索引→槽映射
    - 初始化 IDLE 进程（每 CPU 一个，永远不可调度）
    - 调用架构相关逻辑清零寄存器状态
  - **第二步：填充**（C 中对应 main.c 的 boot 循环 + `arch_boot_proc()`）
    - 遍历 boot image，为每个 entry 填充 slot
    - 判断 schedulable：内核 task + RS + VM 立即分配静态特权；其他用户进程标记为无特权/无时间片
    - 普通进程：初始化 CPU 上下文（PC/SP/ps_strings）
    - VM：在 bootstrap 页表中加载 ELF，再初始化 CPU 上下文
    - 所有进程最终设停止标志，清除 SLOT_FREE
  - 为什么必须先清空再填充：填充时依赖 slot 编号与 endpoint 已就绪
  - 阶段 C 结束状态：所有 boot 进程"就位但等待唤醒"
- **知识点**：D.5（boot 循环逻辑）、A.7（proc_init 角色）
- **教学要点**：用"清空→填充"两节拍建立直觉；函数名仅作为 C 源码对应脚注，不作为主语

### 1.6 boot 流程的终点：唤醒与首次切换 [NEW，补 N 断裂]
- **讲什么**：
  - 阶段 C 结束 ≠ 进程可运行：所有 boot 进程带停止标志，被刻意按住不让跑
  - boot 流程的真正终点：后续阶段（C 中对应 `bsp_finish_booting()`）清除停止标志，唤醒 boot 进程并入调度队列
  - 唤醒后关键动作：初始化当前运行进程/计费进程指针、CPU 级 FPU 使能、关闭 boot 期内存分配窗口、切换到第一个用户态进程
  - **boot 期内存分配窗口**：proc_init → boot 循环 → post-init → memory_init 期间允许内核直接分配物理内存（VM 还没启动），boot 完成后关闭
- **知识点**：N.1-N.5（boot→running 转换）、I.4（boot 期内存分配窗口）
- **教学要点**：这是 06 文档最大的覆盖缺口——读者必须知道"阶段 C 之后进程怎么开始跑"

### 1.7 本章小结
- **讲什么**：进程本质（断点续传）+ CPU 四问框架 + 三件套（进程表/特权表/RTS）+ boot image 与 VM 鸡生蛋 + 阶段 C 执行节拍 + boot 流程终点
- **教学要点**：收束心智模型，为 Ch2 源码分析做铺垫

---

## Ch2. C 源码分析（Ground Truth，Minix3 实际做了什么）

> **本章目标**：以 Minix3 源码为 ground truth，分析阶段 C 的实际实现。每节标注 `file:line` 锚点。**补全 bsp_finish_booting 和 post-init 缺口**。

### 2.0 源码地图：函数分布在哪些文件
- **讲什么**：5 文件 3 层（数据结构层 proc.c / 流程层 main.c / 架构层 arch/*/）；函数→文件→行号→职责→调用者表
- **知识点**：源码分层概览
- **教学要点**：让读者先有"地图"再深入

### 2.1 proc_init()：清空进程表和特权表
- **讲什么**：
  - 三遍循环：进程表白板化（SLOT_FREE/p_nr/p_endpoint/arch_proc_reset）→ 特权表白板化（s_proc_nr=NONE/s_id/ppriv_addr 映射）→ IDLE 进程初始化（每 CPU 一个，共享 idle_priv，RTS_PROC_STOP）
  - p_nr 与 slot 索引一一对应的设计（proc_addr 用数组下标直接定位）
  - p_endpoint 初始 generation=0
  - IDLE 永远不可调度（RTS_PROC_STOP 永不清除）
- **知识点**：A.7（proc_init 实现）、A.6（slot 生命周期）
- **教学要点**：三遍循环的职责分离

### 2.2 arch_proc_reset()：架构特定寄存器初始化
- **讲什么**：
  - x86：分配 FPU 状态区（FPUALIGN 对齐）+ memset 清零 + 设 PSW（INIT_TASK_PSW/INIT_PSW）+ 设段选择子（USER_CS/DS_SELECTOR）+ arch_proc_setcontext(KTS_FULLCONTEXT)
  - ARM：memset 清零 p_reg + 设 PSR
  - **架构差异**：x86 有 FPU 状态区 + 段选择子，ARM 无
  - 三架构状态寄存器初值表：INIT_PSW=0x0202 / INIT_TASK_PSW=0x1202 / INIT_PSR / INIT_TASK_PSR / INIT_USER_SSTATUS / INIT_TASK_SSTATUS
- **知识点**：E.2（arch_proc_reset）、E.5（状态寄存器初值）、K.1（三架构差异）
- **教学要点**：状态寄存器初值决定进程首次运行的特权级和中断状态

### 2.3 boot image 循环：main.c 的核心
- **讲什么**：
  - 遍历 image[0..NR_BOOT_PROCS]：取 proc_addr、同步 endpoint、复制名字、取 boot module
  - reset_proc_accounting(rp) 重置统计
  - **schedulable 判定**：iskerneln || isrootsysn || VM_PROC_NR
  - schedulable 进程：get_priv(static_priv_id) + 按类型设特权（VM→VM_F/SRV_T/SRV_M/SRV_KC；内核 task→TSK_F/TSK_T/NO_M/NO_C；RS→RSYS_F/SRV_T/SRV_M/SRV_KC）
  - **p_priority/p_quantum_size_ms 覆写** [NEW，补 M.2 断裂]：VM/RS→SRV_Q/SRV_QT；内核 task 不覆写（保持 0）；非 schedulable 用户进程保持 0（等 RS 运行时设）
  - 非 schedulable：RTS_NO_PRIV|RTS_NO_QUANTUM
  - arch_boot_proc(ip, rp)（VM 加载 ELF）
  - 非 VM 用户进程：RTS_VMINHIBIT|RTS_BOOTINHIBIT
  - 所有进程：RTS_PROC_STOP，清 RTS_SLOT_FREE
- **知识点**：D.5（boot 循环逻辑）、D.8（schedulable）、B.12（特权授予）、M.2（priority/quantum 覆写）、C.4（阶段 C RTS 位）
- **教学要点**：这是 main.c 的核心循环，讲清每类进程的差异化处理

### 2.4 get_priv() 和 fill_sendto_mask()：特权与 IPC 掩码
- **讲什么**：
  - get_priv()：静态分配（is_static_priv_id 校验 + 槽未占用）vs 动态分配（遍历 BEG_DYN_PRIV_ADDR..END_DYN_PRIV_ADDR 找空闲）；错误码 ENOSPC/EINVAL/EBUSY；关联 p_priv=sp, sp->s_proc_nr=proc_nr
  - set_sendto_bit/fill_sendto_mask：设 IPC 目标位，**保持对称性**（A 能发到 B ⇔ B 能发到 A，除非 B 只支持 RECEIVE）
- **知识点**：B.6（get_priv）、B.7（set_sendto_bit/fill_sendto_mask）
- **教学要点**：IPC 对称性是微内核安全模型的关键不变量

### 2.5 arch_boot_proc()：VM ELF 加载
- **讲什么**：
  - 内核 task（p_nr<0）直接 return
  - bootmod(p_nr) 查 boot module（遍历 kinfo.module_list 匹配 proc_nr）
  - **仅 VM_PROC_NR 特殊处理**：
    - 构造 exec_info execi（stack_high=kinfo.user_sp / stack_size=64KB / hdr=mod->mod_start / filesize=mod_end-mod_start / frame_len=0）
    - 设 6 回调：copymem/clearmem/allocmem_prealloc_junk/allocmem_prealloc_cleared/allocmem_ondemand/clearproc=NULL
    - libexec_load_elf(&execi) 解析 ELF + 映射到 bootstrap 页表
    - libexec_pg_alloc 回调：pg_map(PG_ALLOCATEME) 让分配器选物理帧 + pg_load() 加载页表 + memset 清零 + alloc_for_vm += len 累计
    - 设置 ps_strings 结构（argv/envp），调整 SP（下移 3 字：argc/argv/envp）
    - arch_proc_init(rp, execi.pc, sp, ps_str, name)
    - add_memmap 记录 VM blob 内存区域
    - 清 mod_start/mod_end=0（标记 module 已消费，防重复加载）
    - kinfo.vm_allocated_bytes = alloc_for_vm
- **知识点**：E.4（arch_boot_proc）、F.1（鸡生蛋）、F.3（pg_alloc 回调）、F.10（exec_info）、F.11（6 回调）、F.12（alloc_for_vm）、F.13（bootmod）、F.14（mod 清零）
- **教学要点**：这是阶段 C 最复杂的函数，讲清 libexec 框架的回调机制和 VM 特殊处理

### 2.6 arch_proc_init()：设置进程初始 PC/SP
- **讲什么**：
  - 调 arch_proc_reset(pr) 重新清零
  - strlcpy 名字
  - 设 p_reg.pc=ip、p_reg.sp=sp、p_reg.bx=ps_str（x86 约定 bx 存 ps_strings 地址）
  - 三架构参数传递寄存器：x86 bx / ARM r0 / RISC-V a0
- **知识点**：E.3（arch_proc_init）、E.6（ps_strings 机制）
- **教学要点**：ps_strings 的跨架构传递约定

### 2.7 libexec_load_elf 框架
- **讲什么**：
  - 输入 exec_info（hdr/hdr_len/stack_high/stack_size/proc_e/filesize/progname/frame_len + 回调）
  - 流程：elf_unpack → elf_has_interpreter → 遍历 PT_LOAD → 映射+复制+清零 → 分配栈 → 返回 pc/load_base
  - 错误码：ENOEXEC（ELF 无效）/ ENOMEM（映射失败）
  - 按段类型调用不同回调（copymem 复制段字节 / clearmem 清零 BSS / allocmem_* 分配页）
- **知识点**：F.2（libexec_load_elf 框架）、F.10（exec_info）、F.11（6 回调）
- **教学要点**：libexec 是通用 ELF 加载框架，boot 期只是它的一个特化使用

### 2.8 bsp_finish_booting()：boot 流程的终点 [NEW，补 N 断裂]
- **讲什么**：
  - boot 流程最后一步：proc_init → boot 循环 → arch_post_init → memory_init → system_init → **bsp_finish_booting**
  - 清除 RTS_PROC_STOP 唤醒 boot 进程：`for(i=0; i<NR_BOOT_PROCS-NR_TASKS; i++) RTS_UNSET(proc_addr(i), RTS_PROC_STOP)`（遍历用户态 boot 进程，不含内核 task；RTS_UNSET 自动 enqueue）
  - proc_ptr/bill_ptr 初始化（当前运行进程/计费进程）
  - fpu_init CPU 级 FPU 使能
  - kernel_may_alloc=0（VM 即将接管内存管理，内核不再直接分配）
  - switch_to_user 切换到第一个用户态进程（引用 10-switch-to-user.md）
- **知识点**：N.1-N.9（bsp_finish_booting 全流程）、I.4（kernel_may_alloc 窗口关闭）
- **教学要点**：**这是 06 文档最大的覆盖缺口**——读者必须理解"阶段 C 之后进程怎么开始跑"

### 2.9 boot 期架构后初始化与内存映射 [NEW，补 O 断裂]
- **讲什么**：
  - arch_post_init()：设 ptproc=VM（VM 的页表成为内核切换目标）
  - memory_init()：分配 freepdes 空闲页目录
  - system_init()：初始化特权表运行时结构
  - add_memmap()：向 VM 传递内存映射（记录内存区域）
  - IPCF_POOL_INIT()：IPC filter pool 初始化
  - **范围边界声明**：这些步骤的详细实现见 07/08 文档，本节只讲它们在 boot 流程中的位置和职责
- **知识点**：O.1-O.5（post-init 全流程）
- **教学要点**：明确范围边界，避免与 07/08 文档重复

---

## Ch3. Rust 设计决策（本质机制，约束驱动）

> **本章目标**：讲 Rust rewrite 捕获的**本质机制**（非 translate 表面）。每节用"假设性推理"（如果 X 设计，会有 Y 问题，所以用 Z）替代"迭代历史"（旧版/最初/后来）。**讲 WHY，不讲 HOW**（HOW 在 Ch4）。
>
> **核心叙事**：Rust 不是翻译 C 的函数，而是重新表达 C 背后的机制本质，用 Rust 类型系统强制不变量。

### 3.0 核心问题与设计原则
- **讲什么**：
  - 阶段 C 的 Rust rewrite 要捕获的本质：进程表/特权表/RTS/boot image/CPU 上下文/VM ELF 加载
  - 强制设计原则：
    - P1 零堆启动（boot 期无堆分配器，固定数组 + const fn）
    - P2 no_std（不链接 std，除 #[cfg(test)]）
    - P3 SMP 安全（BKL 下 Rc/RefCell 跨 CPU 不安全，用 Atomic + BKL）
    - P6 FPU 是 arch 演进问题（不翻译 Minix3 的 fnsave/fxrstor，用现代 XSAVE/CPACR_EL1/sstatus.FS）
    - 硬件抽象为 trait（上层不读 arch 私有字段，不用 #[cfg(target_arch)] 选行为）
  - rewrite vs translate 的区别：rewrite 捕获本质机制，translate 复制表面结构
- **知识点**：I.1（零堆）、I.3（const fn 约束）、G.3（BKL）、E.9（FPU arch 演进）、K.2（统一抽象原则）、L.3（panic vs Result）
- **教学要点**：先讲约束（零堆/no_std/SMP/BKL），再讲约束如何驱动设计——这是"本质机制"的核心

### 3.1 进程表设计 [NEW，补 A.8-A.10 断裂]
- **讲什么**：
  - **本质**：进程表是"索引→进程状态"的映射，C 用全局数组 + 裸指针
  - **约束驱动**：零堆 → 固定数组 `[KProcess; PROC_TABLE_SIZE]`；const fn → 编译期初始化；SMP → BKL 下安全
  - **指针→索引的 rewrite**：C 的 `struct proc *p_nextready`/`p_scheduler` 裸指针 → Rust 的 `AtomicI32`/`Option<ProcNr>` 索引。本质：避免裸指针跨 CPU 共享、便于边界检查、便于 const fn 构造
  - **边界检查强制**：C 的 `proc_addr(n)` 无检查 → Rust 的 `get(nr)`/`get_mut(nr)` 返回 `Option`。本质：把"越界是 UB"变为"越界是编译期/运行期可捕获"
  - **假设性推理**：如果在 boot 期用 `Box<[KProcess]>` 堆分配，会破坏零堆约束（boot 期无堆分配器）；如果用裸指针跨 CPU 共享，SMP 下无法证明安全
- **知识点**：A.8（ProcessTable 类型设计）、A.10（指针→索引）、I.2（零堆实现）、I.3（const fn）
- **教学要点**：讲清"指针→索引"不是风格选择，是 SMP 安全 + const fn 的必然

### 3.2 特权表设计 [NEW，补 B.8-B.11 断裂]
- **讲什么**：
  - **本质**：特权表是"特权 ID → 能力集合"的映射，C 用全局数组 + ppriv_addr 加速指针
  - **静态区/动态区分区**：静态区前 NR_STATIC_PRIV_IDS 个 boot 期分配（static_priv_id(n)=NR_TASKS+n）；动态区后续运行时分配。本质：boot 期确定性分配 vs 运行时按需分配
  - **grant_capability API**：C 的 assign_static + configure_boot_priv 两步 → Rust 的 `grant_capability(nr, template)` 单一调用。本质：封装"分配 + 配置"原子操作，避免中间不一致状态
  - **CapabilityTemplate 5 变体**：Idle/KernelTask/Vm/RootService/Deferred，对应 5 类进程角色。本质：用类型系统替代 C 的 if-else 散落判定，编译期保证角色→能力映射完整
  - **Newtype 类型安全**：TrapMask(u32)/IpcMask(u64)/KCallMask(u64)/PrivId(u32)。本质：类型系统强制"不能把 IpcMask 传给 KCallMask 参数"，C 的裸 int 无此保护
  - **假设性推理**：如果让调用方手动传 flags/init_flags/trap_mask/ipc_to/k_call_mask/sig_mgr 6 个参数，会有三个问题（易传错顺序/无法编译期校验/中间状态不一致）
- **知识点**：B.8（PrivTable）、B.10（CapabilityTemplate）、B.11（Newtype）、B.4（静态/动态分区）
- **教学要点**：CapabilityTemplate 是"类型驱动设计"的典范——用枚举替代 if-else

### 3.3 进程状态表达 [NEW，补 C.5-C.8 断裂]
- **讲什么**：
  - **本质**：RTS 标志是"多原因不可运行"的位图状态机，C 用裸 int + 宏
  - **RtsFlags bitflags 设计**：17 位全保留，`is_runnable()` 方法表达"非零=不可运行"不变量
  - **rts_set/rts_unset 调度队列维护**：C 的 RTS_SET/UNSET 宏自动 dequeue/enqueue → Rust 的 rts_set/rts_unset 方法。本质：把"标志与队列一致性"这个隐藏不变量变为方法契约
  - **misc_flags 全集**（MF_* 18 位）：MF_REPLY_PEND/MF_VIRT_TIMER/MF_PROF_TIMER/MF_KCALL_RESUME/MF_DELIVERMSG/MF_SIG_DELAY/MF_SC_ACTIVE/DEFER/TRACE/MF_FPU_INITIALIZED/MF_SENDING_FROM_KERNEL/MF_CONTEXT_SET/MF_FLUSH_TLB/MF_STEP/MF_NICED 等
  - **misc_flags 与 RTS 的区别**：RTS 决定可运行性（影响调度队列），misc_flags 记录次要运行时状态（不影响调度）
  - **P_BLOCKEDON/blocked_on()**：返回进程阻塞在哪个端点上（SENDING→p_sendto_e / RECEIVING→p_getfrom_e / 否则 NONE）
  - **proc_kernel_scheduler/is_kernel_scheduled()**：判断进程是否由内核默认调度（scheduler==None || scheduler==self）
- **知识点**：C.5（RtsFlags 设计）、C.6（misc_flags 全集）、C.7（P_BLOCKEDON）、C.8（proc_kernel_scheduler）
- **教学要点**：RTS_SET/UNSET 的队列一致性是隐藏不变量，Rust 用方法契约显式化

### 3.4 boot image 类型设计 [NEW，补 D.6-D.8 断裂]
- **讲什么**：
  - **本质**：boot image 是"编译期固定的进程清单"，C 用 `struct boot_image image[]` 数组
  - **KERNEL_TASKS/BOOT_MODULE_PROC_NRS 常量数组**：编译期固定列表，替代 C 的 image[]。本质：用 const 常量表达"编译时硬编码"
  - **BootModule 类型衔接**：from minix_boot crate，含 name/start/len，替代 C 的 multiboot_module_t
  - **NR_BOOT_PROCS vs NR_BOOT_MODULES**：前者含内核 task（17），后者只含用户态模块（12）。本质：两类进程的来源不同（内核 task 内置 / 用户态模块来自 multiboot）
  - **schedulable 判定**：is_root_sys || is_vm，替代 C 的 iskerneln||isrootsysn||VM_PROC_NR
- **知识点**：D.6（boot image 类型设计）、D.7（init_proc_and_boot 主流程）、D.8（schedulable）
- **教学要点**：用 const 常量表达"编译时硬编码"是 Rust 的自然方式

### 3.5 CPU 上下文抽象：从 3 函数到 1 trait
- **讲什么**：
  - **本质**：C 用 3 个 arch 函数（arch_proc_reset + arch_proc_init + arch_boot_proc）回答 CPU 四问；Rust 用 1 个 trait（CpuContextArch）统一
  - **为什么合并 3 trait 为 1**（假设性推理，非"旧设计"）：如果用 3 个 trait 镜像 C 的 3 函数，会暴露三个问题：①trait 间状态共享困难（CpuContext 要在 trait 间传递）；②调用方需协调 3 trait 调用顺序；③arch 实现重复（三架构的 reset/init/boot_proc 都操作同一 CpuContext）
  - **CpuContextArch trait 设计**：`build_cpu_context(kind, proc_nr, entry)` / `apply_to_trap_frame` / `enable_user_io` / `inherit_fpu_state`；CpuContext 是关联类型，arch 私有，kernel 层不检视字段
  - **ProcKind 枚举**：KernelTask/Vm/RootService/UserService/UserProcess，驱动 build_cpu_context 选择 PSW/PSR/sstatus + FPU 策略。本质：用类型替代 C 的 iskerneln/isrootsysn 散落判定
  - **EntrySpec 结构**：pc/sp/ps_strings 均为 Option，明确"未知/延迟/已加载"三态。本质：替代 C 的"零值表示未知"隐式约定
  - **CurrentCpuContextArch 类型别名**：编译期单架构选择，上层用 `<CurrentCpuContextArch as CpuContextArch>::build_cpu_context(...)` 调用
- **知识点**：E.7（CpuContextArch trait）、E.10（ProcKind）、E.11（EntrySpec）、K.2-K.4（统一抽象）、K.3（CurrentCpuContextArch）
- **教学要点**：3→1 合并是"本质抽象"的典范——C 的 3 函数操作同一 CpuContext，本质是同一职责

### 3.6 FPU 架构演进：不翻译 Minix3 的 fnsave（KEY 知识点）
- **讲什么**：
  - **灵魂本质**：FPU 不是"进程状态的一部分"，而是"硬件上下文策略"——不同架构对浮点的理解完全不同，rewrite 要捕获的是这种差异，而不是把 Minix3 的 x86-32 模型硬搬到三架构
  - **第一层：Minix3 的历史包袱——fnsave/fxrstor 模型**
    - x86-32 时代：每个用户进程有一块固定大小的 FPU save area（`p_seg.fpu_state[FPU_XFP_SIZE]`），启动时 memset 清零
    - 这是"每个进程都有一份 FPU 状态"的朴素模型，只适合 x86 fnsave/fxrstor
    - aarch64/riscv64 没有对应概念——硬翻译会得到"所有进程都有 576B FPU 区"的虚假统一
  - **第二层：现代硬件的 FPU 模型已经分化**
    - **x86_64 XSAVE 模型**：lazy 初始化（首次 FP 指令 #NM trap），XSAVE area 按需分配；`X86FpuInitPolicy { KernelTask, LazyUserInit }`；不是 memset 清零
    - **aarch64 CPACR_EL1.FPEN**：系统级/进程级混合配置；per-process 只有 FPCR/FPSR；`fpu_enable_el0` 字段表达"该用户进程是否允许 EL0 发 FP 指令"
    - **riscv64 sstatus.FS 状态机**：Off=0/Initial=1/Clean=2/Dirty=3，第一次 FP 指令 trap 进 kernel；设 Initial 触发首次陷阱
  - **第三层：rewrite 的本质——把 FPU 策略下沉到 arch 层**
    - FPU 策略是 `CpuContext` 的 arch 私有字段，kernel 层从不读取
    - x86_64 的 `fpu_policy` / aarch64 的 `fpu_enable_el0` / riscv64 的 `sstatus`（含 FS 字段）都封装在 arch 内部
    - `inherit_fpu_state`：fork 路径继承父进程 FPU 策略（x86_64 复制 fpu_policy / aarch64 复制 fpu_enable_el0 / riscv64 复制 sstatus）
  - **C vs Rust 对照表**：
    | 架构 | Minix3 C 做法 | 现代硬件实际 | Rust rewrite 做法 |
    |---|---|---|---|
    | x86-64 | `memset(fpu_state[n])` | XSAVE lazy init | `X86FpuInitPolicy` 字段在 `CpuContext` |
    | aarch64 | 无 FPU init | CPACR_EL1.FPEN 系统/进程级 | `fpu_enable_el0` 字段 |
    | riscv64 | 无对应源码 | `sstatus.FS` 状态机 | `sstatus` 含 FS 字段 |
  - **假设性推理**：如果翻译 Minix3 的 `fnsave`/`fxrstor` + `p_seg.fpu_state[FPU_XFP_SIZE]`，会泄漏 32 位遗留模型到 OS 层，且 aarch64/riscv64 无对应概念
- **知识点**：E.9（FPU 现代模型）、H.2（FPU 继承）、K.1（三架构差异）
- **教学要点**：**这是用户强调的关键知识点**——FPU 架构演进是"rewrite not translate"的典范。讲清：历史包袱是什么、现代硬件怎么想、Rust 如何重新表达这种差异

### 3.7 能力模型：CapabilityTemplate 替代 6 裸参数
- **讲什么**：
  - **本质**：C 的特权授予是"按 proc_nr 分支 + 6 裸参数逐字段设"，Rust 用 CapabilityTemplate 枚举封装
  - **5 变体对应 5 类进程角色**：Idle（IDL_F）/ KernelTask（TSK_F）/ Vm（VM_F）/ RootService（RSYS_F）/ Deferred（运行时设）
  - **每个变体的 capabilities()/trap_mask()/ipc_mask()/kcall_mask()**：返回类型安全的 Newtype，替代 C 的裸 int
  - **假设性推理**：如果让调用方手动传 6 参数（flags/init_flags/trap_mask/ipc_to/k_call_mask/sig_mgr），会有三个问题（易传错顺序/无法编译期校验角色→能力映射/中间状态不一致）
  - **错误处理**：CapabilityError { InvalidProcNr, SlotOccupied, NoFreeSlots }，替代 C 的 errno
- **知识点**：B.10（CapabilityTemplate）、B.11（Newtype）、B.12（boot 期授予）、L.1（errno→Result）
- **教学要点**：CapabilityTemplate 是"类型驱动设计"——用枚举变体替代 if-else 分支

### 3.8 零堆启动：固定数组 + const fn
- **讲什么**：
  - **本质约束**：boot 期无堆分配器（allocator 在 VM 启动后才可用），所有数据结构必须编译期或 BSS 静态分配
  - **固定数组**：ProcessTable { procs: [KProcess; PROC_TABLE_SIZE] } / PrivTable { privs: [KPriv; NR_SYS_PROCS] }
  - **const fn 初始化**：编译期构造，`static mut PROC_TABLE`/`static mut PRIV_TABLE` BSS 全局
  - **const fn 约束**：不能用 for 循环（用 while）/不能调用非 const fn/不能用 String/Vec；KProcess/KPriv 所有字段必须 const-constructible；这是为什么用 AtomicI32 而非 Mutex
  - **kernel_may_alloc 标志**：boot 期 =1（内核可临时分配物理内存，VM 还没启动），bsp_finish_booting 后 =0（VM 接管）。**boot 期内存分配窗口**
  - **假设性推理**：如果在 boot 期用 Box<[KProcess]> 堆分配，会破坏零堆约束
- **知识点**：I.1（为什么零堆）、I.2（零堆实现）、I.3（const fn 约束）、I.4（kernel_may_alloc）
- **教学要点**：零堆是 boot 期的硬约束，驱动固定数组 + const fn 设计

### 3.9 SMP 预留：阶段 C 只埋桩（范围边界）
- **讲什么**：
  - 阶段 C 的进程数据结构只**预留** SMP 相关字段（`cpu_mask`、`scheduler`、`cpu` 等），但本身不涉及多 CPU 调度
  - boot 期所有内核代码在 BKL 下串行执行，这些字段此时只是占位
  - 真正的 per-CPU 调度状态（`proc_ptr`/`bill_ptr`/`run_q_head` 等）在阶段 F 的 `SmpState` 中展开
  - 本节不深入 CpuMask/BKL/Atomic 细节，避免提前剧透阶段 F
- **知识点**：G.1-G.5（仅作范围标注）
- **教学要点**：明确告诉读者"这里只是埋桩，阶段 F 再讲"，防止范围漂移

### 3.10 KPriv 字段重组：6 子结构
- **讲什么**：
  - **本质**：C 的 priv 是 30+ 字段单一大结构体，访问语义混乱；Rust 按语义重组为 6 子结构
  - **6 子结构**：PrivCapability（s_proc_nr/s_id/s_flags/s_init_flags）/ PrivSignals（信号相关）/ PrivIpc（s_trap_mask/s_ipc_to/s_k_call_mask）/ PrivIo（s_io_tab/s_irq_tab）/ PrivMem（s_mem_tab/s_ipcf/s_stack_guard）/ PrivRuntime（s_alarm_timer/s_grant_table/s_state_table）
  - **每子结构 const fn new()**：可编译期构造
  - **假设性推理**：如果把 priv 的 30+ 字段平铺在一个结构体里，访问语义会混乱（IPC 字段与 IO 字段与信号字段混在一起），且无法按语义分组初始化
- **知识点**：B.9（KPriv 6 子结构）
- **教学要点**：6 子结构是"语义分组"设计——按职责拆分，非按字段类型

### 3.11 调度字段与统计设计 [NEW，补 M 断裂]
- **讲什么**：
  - **本质**：进程的调度属性（优先级/时间片/CPU 亲和性/调度器）和统计信息（accounting/time/cycles）需要初始化
  - **SchedFields 结构体**：priority: AtomicI8 / quantum: Quantum / cpu: AtomicU32 / cpu_mask: CpuMask / scheduler: Option<ProcNr>；const fn new() 默认 USER_Q/200ms/cpu=0/all/None
  - **p_priority/p_quantum 覆写策略**：VM/RS→SRV_Q/SRV_QT（boot 循环覆写）；内核 task 保持 0；非 schedulable 用户进程保持 0（等 RS 运行时设）
  - **Accounting/TimeStats/CyclesStats 统计结构体**：reset_proc_accounting() 语义（重置统计，boot 期每进程调一次）
  - **调度优先级常量**：SRV_Q/SRV_QT/TSK_Q/TSK_QT/USER_Q/MIN_USER_Q/MAX_USER_Q/NONE_Q
- **知识点**：M.1（优先级常量）、M.2（覆写策略）、M.5（SchedFields）、M.7（reset_proc_accounting）
- **教学要点**：讲清"哪些进程在 boot 期设调度字段，哪些等运行时"

### 3.12 boot→running 转换设计 [NEW，补 N 断裂]
- **讲什么**：
  - **本质**：阶段 C 结束时所有 boot 进程带 RTS_PROC_STOP，bsp_finish_booting 唤醒它们
  - **finish_booting 设计**：清除 RTS_PROC_STOP + enqueue（遍历用户态 boot 进程，不含内核 task）
  - **proc_ptr/bill_ptr 初始化**：当前运行进程/计费进程指针
  - **fpu_init CPU 级 FPU 使能**：x86 CR0.MP/EM/TS / aarch64 CPACR_EL1.FPEN / riscv64 sstatus.FS
  - **kernel_may_alloc=0**：关闭 boot 分配窗口，VM 即将接管
  - **switch_to_user**：切换到第一个用户态进程（引用 10-switch-to-user.md）
- **知识点**：N.1-N.9（boot→running 全流程）
- **教学要点**：这是 06 文档最大的覆盖缺口——讲清"阶段 C 之后进程怎么开始跑"

### 3.13 错误处理与不变量 [NEW，补 L 断裂]
- **讲什么**：
  - **errno → Result 转换**：C 的 get_priv 返回 EINVAL/ENOSPC/EBUSY → Rust 的 `Result<PrivId, CapabilityError>`；C 的 libexec_load_elf 返回 ENOEXEC/ENOMEM → Rust 的 `Result<VmLoadResult, VmLoadError>`
  - **不变量表达**：C 的 `p_rts_flags == RTS_SLOT_FREE` 表示空槽（注释约定）→ Rust 的 `is_empty()` 方法 + debug_assert；C 的 p_vmrequest 始终嵌入 + RTS_VMREQUEST 控制有效性 → Rust 的 `p_vm_suspend: Option<VmSuspendContext>` + 不变量注释 `RTS_VMREQUEST <==> p_vm_suspend.is_some()`
  - **panic vs Result 策略**：boot 期错误（如 grant_capability 槽占用）panic（boot 不可恢复）；运行时错误（如 fork 失败）返回 Result；VM ELF 加载 DEFERRED 不 panic（诚实标记未实现）
- **知识点**：L.1（errno→Result）、L.2（不变量表达）、L.3（panic vs Result）
- **教学要点**：Option 替代"always-embedded + flag"是 Rust 不变量表达的核心

### 3.14 对照：fork 运行时路径（非 boot 路径）
- **讲什么**：
  - 06 文档聚焦 boot 路径；fork 是运行时路径，仅作为对照提及
  - fork_from 设计要点：继承调度属性（priority/quantum/cpu/cpu_mask）与 IPC 端点；重置 accounting/cpuavg；队列指针独立
  - **FPU 状态继承**是 fork 与 boot 路径的交汇点：`inherit_fpu_state` 把父进程 FPU 策略复制给子进程（x86_64 复制 fpu_policy / aarch64 复制 fpu_enable_el0 / riscv64 复制 sstatus）
  - RTS 继承策略：子进程不继承 SENDING/RECEIVING/PREEMPTED；初始带 PROC_STOP；清 SLOT_FREE
- **知识点**：H.1-H.4
- **教学要点**：明确标注"运行时路径，非 boot 路径"，避免稀释阶段 C 主题

### 3.15 设计决策汇总表
- **讲什么**：所有设计决策的汇总表（决策 / 约束 / 替代方案 / 理由）
- **教学要点**：一表收束 Ch3，便于读者快速回顾

---

## Ch4. 实现详解（HOW，具体代码）

> **本章目标**：讲 HOW——具体 trait 实现、结构体字段、主流程代码。**与 Ch3 边界**：Ch3=WHY（约束驱动设计），Ch4=HOW（具体实现）。

### 4.0 实现地图：init_proc_and_boot() 主流程
- **讲什么**：Rust 主流程 Step 1-8 概览（获取全局表 → 校验 boot_modules → Step 3a 内核 task → Step 3b 用户进程 → 更新 boot_procs → finish_booting）
- **知识点**：D.7（init_proc_and_boot 主流程）
- **教学要点**：先有地图再深入

### 4.1 arch 层：CpuContextArch trait 实现
- **讲什么**：
  - x86_64：X86_64CpuContext { psw, cs, ds, ss, es, fs, gs, pc, sp, bx, fpu_policy } + X86FpuInitPolicy { KernelTask, LazyUserInit }
  - aarch64：AArch64CpuContext { psr, pc, sp, r0, fpu_enable_el0 }
  - riscv64：Riscv64CpuContext { sstatus, sepc, sp, a0 }
  - build_cpu_context 各架构实现（PSW/PSR/sstatus 初值 + 段选择子 + FPU 策略）
  - apply_to_trap_frame：CpuContext → TrapFrame 转换
  - enable_user_io（x86 IOPL 提升，[NEW] 补 E.12 断裂）：设 PSW.IOPL=3，某些驱动需直接 IN/OUT；ARM/RISC-V 无对应概念（用 MMIO 映射代替）
  - inherit_fpu_state：fork 路径 FPU 继承
- **知识点**：E.8（三架构 CpuContext 实现）、E.12（enable_user_io）、H.2（inherit_fpu_state）
- **教学要点**：三架构实现对照，突出差异点

### 4.2 arch 层：load_vm_elf 共享实现
- **讲什么**：
  - `fn load_vm_elf<P: Paging>(module, kernel_info, paging) -> Result<VmLoadResult, VmLoadError>`
  - free function 而非 trait 方法（三架构实现字节相同，false polymorphism）
  - 用 minix_elf crate 解析 ELF（替代 C 的 libexec）
  - 逐页映射 PT_LOAD 段（identity mapping，paddr=vaddr）
  - 复制段字节到映射页 / 清零 BSS
  - 分配用户栈（VM_STACK_SIZE=64KB）
  - 返回 VmLoadResult { pc, sp, ps_strings, allocated_bytes }
  - ELF 段标志到页标志映射：PF_R|PF_W|PF_X → PageFlags
  - VmLoadError { InvalidElf, MappingFailed }（Result，无静默失败）
  - DEFERRED 路径（当前 Rust 实现）：真实 VM ELF 加载需 bootstrap 页表，当前标 DEFERRED，VM 启动时 PC=0，RS 在用户态启动期加载真实 VM ELF。诚实标记未实现，非静默失败
- **知识点**：F.5（load_vm_elf 设计）、F.6（VmLoadResult 与 EntrySpec 衔接）、F.7（段标志映射）、F.8（VM 占用字节统计）、F.9（DEFERRED 路径）、L.1（VmLoadError）
- **教学要点**：free function vs trait 方法的选择——false polymorphism 不该用 trait

### 4.3 kernel 层：ProcessTable 与 PrivTable
- **讲什么**：
  - ProcessTable { procs: [KProcess; PROC_TABLE_SIZE], sched, vm_request_queue }；const fn new()；get/get_mut 返回 Option；iter/iter_mut；endpoint_to_nr
  - PrivTable { privs: [KPriv; NR_SYS_PROCS] }；const fn new()；get/get_mut；assign_static/assign_dynamic；grant_capability
  - static mut PROC_TABLE/PRIV_TABLE BSS 全局
  - SMP 安全：所有方法要求持有 BKL
- **知识点**：A.8（ProcessTable）、B.8（PrivTable）、I.2（零堆实现）
- **教学要点**：两个表的对称设计

### 4.4 kernel 层：KProcess 结构
- **讲什么**：
  - KProcess 字段全集：p_nr/p_endpoint/p_seg/priv_id: Option<PrivId>/p_rts_flags/p_misc_flags/p_sched/p_accounting/p_time/p_cycles/p_cpuavg/p_nextready: AtomicI32/p_caller_q/p_q_link/p_getfrom_e/p_sendto_e/p_pending/p_name/p_sendmsg/p_delivermsg/cpu_context: CurrentCpuContext/p_next_restart/p_next_requestor/p_vm_suspend: Option<VmSuspendContext>
  - RtsFlags bitflags 实现（17 位）
  - MiscFlags bitflags 实现（18 位 MF_*）
  - SchedFields { priority: AtomicI8, quantum: Quantum, cpu: AtomicU32, cpu_mask: CpuMask, scheduler: Option<ProcNr> }
  - Accounting/TimeStats/CyclesStats 统计结构体
  - 不变量注释：RTS_VMREQUEST <==> p_vm_suspend.is_some()
  - **fork_from 方法（运行时路径，非 boot 路径）**：继承调度属性与 IPC 端点；重置 accounting/cpuavg；队列指针独立；调用 `cpu_context.inherit_fpu_state(&parent.cpu_context)` 继承 FPU 策略；RTS 修正（不继承 SENDING/RECEIVING/PREEMPTED，带 PROC_STOP，清 SLOT_FREE）
- **知识点**：A.9（KProcess 结构体）、A.10（指针→索引）、C.5（RtsFlags）、C.6（MiscFlags）、M.5（SchedFields）、L.2（不变量表达）、H.2-H.4（fork 继承策略）
- **教学要点**：字段按语义分组讲解（标识/状态/调度/统计/IPC/VM）；fork 作为运行时对照点，不展开细节

### 4.5 kernel 层：能力授予
- **讲什么**：
  - grant_capability(nr, template) -> Result<PrivId, CapabilityError>
  - 内部：assign_static + configure_boot_priv 两步合一
  - 5 模板的 capabilities()/trap_mask()/ipc_mask()/kcall_mask() 返回值
  - 重复分配失败测试（SlotOccupied 错误）
- **知识点**：B.10（CapabilityTemplate）、B.12（boot 期授予）、L.1（CapabilityError）
- **教学要点**：grant_capability 是"分配+配置"原子操作

### 4.6 kernel 层：KPriv 6 子结构
- **讲什么**：
  - 6 子结构字段全集（PrivCapability/PrivSignals/PrivIpc/PrivIo/PrivMem/PrivRuntime）
  - 每子结构 const fn new()
  - 与 C priv.h 字段对照表
- **知识点**：B.9（KPriv 6 子结构）、B.2（priv 字段全集）
- **教学要点**：6 子结构的语义分组

### 4.7 主流程：init_proc_and_boot()
- **讲什么**：
  - Step 1+2：获取全局 proc_table/priv_table；校验 boot_modules.len()==NR_BOOT_MODULES
  - Step 3a：遍历 KERNEL_TASKS，设名字 + grant_capability + build_cpu_context + RTS_PROC_STOP
  - Step 3b：遍历 boot_modules，设名字 + 判断 schedulable + grant_capability 或 RTS_NO_PRIV + build_cpu_context + VMINHIBIT/BOOTINHIBIT + RTS_PROC_STOP
  - Step 4：更新 boot_procs 信息
  - schedulable 判定实现：is_root_sys || is_vm
  - p_priority/p_quantum 覆写实现（VM/RS→SRV_Q/SRV_QT）
- **知识点**：D.7（init_proc_and_boot 主流程）、D.8（schedulable）、M.2（覆写实现）
- **教学要点**：Step 3a/3b 的对称结构

### 4.8 boot→running：finish_booting [NEW，补 N 断裂]
- **讲什么**：
  - finish_booting 实现：清除 RTS_PROC_STOP + enqueue 循环
  - proc_ptr/bill_ptr 初始化
  - fpu_init CPU 级 FPU 使能
  - kernel_may_alloc 设 false（AtomicBool）
  - switch_to_user 引用
- **知识点**：N.9（finish_booting 实现）
- **教学要点**：与 Ch3.12 的 WHY 对应

---

## Ch5. 测试要点

> **本章目标**：验证 Rust 实现的正确性。分 arch/kernel/集成三层 + L3 grep 证据。

### 5.0 测试策略
- **讲什么**：三层测试（arch/kernel/集成）+ L3 grep 证据；boot 期 panic vs 运行时 Result 的测试策略
- **知识点**：J.1-J.4 概览
- **教学要点**：测试分层与设计原则

### 5.1 arch 层测试
- **讲什么**：
  - x86_64：PSW 初值/段选择子/apply_to_trap_frame/inherit_fpu_state/enable_user_io（[NEW] 补 E.12）
  - aarch64：PSR 初值/fpu_enable_el0/inherit_fpu_state
  - riscv64：sstatus 初值/SPP/SPIE 位/inherit_fpu_state
  - load_vm_elf：invalid_elf_returns_err / mock paging 映射验证
- **知识点**：J.1（arch 层测试）、E.12（enable_user_io 测试）
- **教学要点**：三架构测试对照，每架构覆盖 FPU 策略差异

### 5.2 kernel 层测试
- **讲什么**：
  - ProcessTable：const fn 可编译 / per-slot p_nr/p_endpoint / IDLE slot 特殊处理
  - PrivTable：const fn 可编译 / grant_capability 各模板 / 重复分配失败（SlotOccupied）
  - KProcess：fork_from 继承 / accounting 重置 / 队列指针独立
  - RtsFlags：rts_set/rts_unset 调度队列维护（[NEW] 补 C.5 断裂）
  - SchedFields/Accounting：初始化测试（[NEW] 补 M 断裂）
- **知识点**：J.2（kernel 层测试）、C.5（rts_set/rts_unset 测试）、M.5（SchedFields 测试）
- **教学要点**：rts_set/rts_unset 的队列一致性是隐藏不变量，必须有测试

### 5.3 集成测试
- **讲什么**：
  - init_proc_and_boot() 主流程：boot_modules 数量校验 / Step 3a+3b 完整流程 / RTS 标志最终状态
  - VM ELF 加载（mock paging）：load_vm_elf 集成
  - finish_booting 唤醒测试（[NEW] 补 N 断裂）：RTS_PROC_STOP 清除 + enqueue 验证
- **知识点**：J.3（集成测试）、N.9（finish_booting 测试）
- **教学要点**：集成测试验证主流程端到端

### 5.4 L3 grep 证据：旧 API 0 残留
- **讲什么**：
  - 旧 API（assign_static + configure_boot_priv）0 残留 grep 证据
  - initial_pc/initial_sp/initial_ps_strings_reg/initial_status 旧字段 0 残留 grep 证据
  - 新 API 命名空间导入一致性 grep 证据
- **知识点**：J.4（L3 grep 证据）
- **教学要点**：grep 证据是"旧 API 已清除"的机器可验证明

### 5.5 测试覆盖矩阵
- **讲什么**：知识点→测试用例映射矩阵；覆盖完整性自检
- **知识点**：J.1-J.4 汇总
- **教学要点**：一表收束 Ch5，便于查漏补缺

---

## 附录 A. libexec_load_elf 框架详解

> **附录目标**：深入讲 libexec_load_elf 的通用框架，作为 §2.7 的补充。

### A.1 exec_info 结构体字段全集
- **讲什么**：stack_high/stack_size/proc_e/hdr/hdr_len/filesize/progname/frame_len/pc/load_offset + 6 回调字段
- **知识点**：F.10（exec_info）

### A.2 6 回调机制详解
- **讲什么**：copymem/clearmem/allocmem_prealloc_junk/allocmem_prealloc_cleared/allocmem_ondemand/clearproc；按 ELF 段类型调用不同回调
- **知识点**：F.11（6 回调）

### A.3 ELF 段处理流程
- **讲什么**：elf_unpack → elf_has_interpreter → 遍历 PT_LOAD → 映射+复制+清零 → 分配栈 → 返回 pc/load_base
- **知识点**：F.2（libexec_load_elf 流程）

---

## Ch6. 参见

- **讲什么**：前置文档（03-kmain-cstart / 05-clock-interrupt-init）/ 后续文档（07-cross-space-init / 08-system-init-boot-finish / 10-switch-to-user）/ 相关文档（00-kernel-overview）
- **教学要点**：明确文档间关系，便于读者导航

---

## 大纲完整性自检

### 知识点覆盖矩阵（A.0-O.5 → 章节）

| 概念组 | Ch1 概念 | Ch2 C 源码 | Ch3 Rust 设计 | Ch4 实现 | Ch5 测试 | 状态 |
|--------|---------|-----------|--------------|---------|---------|------|
| A.0 进程概念 | §1.1 [NEW] | — | — | — | — | ✅ |
| A.1-A.6 进程表布局/标识符/slot | §1.3 | §2.1 | §3.1 [NEW] | §4.3 | §5.2 | ✅ |
| A.7 proc_init() | §1.5 | §2.1 | §3.1 | §4.3 | §5.2 | ✅ |
| A.8-A.10 ProcessTable/KProcess/指针→索引 | — | — | §3.1 [NEW] | §4.3/§4.4 | §5.2 | ✅ |
| B.1-B.5 特权表/priv/默认设置 | §1.3 | §2.4 | §3.2 [NEW] | §4.5/§4.6 | §5.2 | ✅ |
| B.6-B.7 get_priv/fill_sendto_mask | — | §2.4 | §3.2 | §4.5 | §5.2 | ✅ |
| B.8-B.11 PrivTable/CapabilityTemplate/Newtype | — | — | §3.2 [NEW] | §4.5 | §5.2 | ✅ |
| B.12 boot 期特权授予 | §1.3 | §2.3 | §3.7 | §4.5/§4.7 | §5.3 | ✅ |
| C.1-C.4 RTS 标志/位图/宏/阶段 C 位 | §1.3 | §2.1/§2.3 | §3.3 [NEW] | §4.4/§4.7 | §5.2 | ✅ |
| C.5-C.8 RtsFlags/misc_flags/P_BLOCKEDON/proc_kernel_scheduler | — | — | §3.3 [NEW] | §4.4 | §5.2 | ✅ |
| D.1-D.5 boot image/三类进程/循环 | §1.4 | §2.3 | §3.4 [NEW] | §4.7 | §5.3 | ✅ |
| D.6-D.8 boot image 类型/init_proc_and_boot/schedulable | — | — | §3.4 [NEW] | §4.7 | §5.3 | ✅ |
| E.1 CPU 四问 | §1.2 | §2.2/§2.5/§2.6 | §3.5 | §4.1 | §5.1 | ✅ |
| E.2-E.4 arch_proc_reset/init/boot_proc | §1.2 | §2.2/§2.5/§2.6 | §3.5 | §4.1/§4.2 | §5.1 | ✅ |
| E.5 三架构状态寄存器 | §1.2 | §2.2 | §3.5 | §4.1 | §5.1 | ✅ |
| E.6 ps_strings | §1.2 | §2.6 | §3.5 | §4.2 | §5.1 | ✅ |
| E.7-E.8 CpuContextArch trait + 三架构实现 | — | — | §3.5 | §4.1 | §5.1 | ✅ |
| E.9 FPU 现代模型 | — | — | §3.6 (KEY) | §4.1/§4.4 | §5.1 | ✅ |
| E.10-E.11 ProcKind/EntrySpec | — | — | §3.5 | §4.1 | §5.1 | ✅ |
| E.12 enable_user_io | — | §2.5 | §3.5 | §4.1 [NEW] | §5.1 [NEW] | ✅ |
| F.1 VM 鸡生蛋 | §1.4 | §2.5 | §3.5 | §4.2 | §5.3 | ✅ |
| F.2-F.3 libexec_load_elf/pg_alloc 回调 | §1.4 | §2.5/§2.7/附录A | §3.5 | §4.2 | §5.3 | ✅ |
| F.4 bootstrap 页表三部分 | §1.4 | §2.5 | §3.5 | §4.2 | §5.3 | ✅ |
| F.5-F.9 load_vm_elf/VmLoadResult/DEFERRED | — | — | §3.5 | §4.2 | §5.3 | ✅ |
| F.10-F.14 exec_info/回调/alloc_for_vm/bootmod/mod清零 | — | §2.5/附录A | §3.5 | §4.2 | §5.3 | ✅ |
| G.1-G.5 SMP 预留 | — | — | §3.9 | — | — | ✅（标注阶段 F） |
| H.1-H.4 fork 路径 | — | — | §3.14 | §4.4 | §5.2 | ✅（标注运行时） |
| I.1-I.3 零堆启动 | — | — | §3.8 | §4.3 | §5.2 | ✅ |
| I.4 kernel_may_alloc | §1.6 [NEW] | §2.8 [NEW] | §3.8 | §4.8 [NEW] | — | ✅ |
| J.1-J.4 测试覆盖 | — | — | — | — | §5.1-§5.5 | ✅ |
| K.1-K.4 跨架构统一抽象 | §1.2 | §2.2 | §3.5 | §4.1 | §5.1 | ✅ |
| L.1-L.3 错误处理/不变量/panic | — | — | §3.13 [NEW] | §4.5 | — | ✅ |
| M.1-M.4 调度优先级/p_priority/p_quantum/p_scheduler | — | §2.3 [NEW] | §3.11 [NEW] | §4.7 | — | ✅ |
| M.5-M.7 SchedFields/Accounting/reset_proc_accounting | — | — | §3.11 [NEW] | §4.4 | §5.2 [NEW] | ✅ |
| N.1-N.9 boot→running 转换 | §1.6 [NEW] | §2.8 [NEW] | §3.12 [NEW] | §4.8 [NEW] | §5.3 [NEW] | ✅ |
| O.1-O.5 arch_post_init/memory_init/add_memmap/IPCF | — | §2.9 [NEW] | — | — | — | ✅（范围边界） |

### 断裂修复验证（对照 06-structure.md §三）

| 断裂点 | 修复章节 | 状态 |
|--------|---------|------|
| 1. Ch1.3 讲进程表，Ch3 无 ProcessTable 设计 | §3.1 [NEW] | ✅ |
| 2. Ch1.3 讲特权表，Ch3 无 PrivTable 设计 | §3.2 [NEW] | ✅ |
| 3. Ch1.3 讲 RTS，Ch3/Ch4 无 RtsFlags 设计 | §3.3 [NEW] + §4.4 | ✅ |
| 4. Ch1.4 讲 boot image，Ch3 无类型设计 | §3.4 [NEW] | ✅ |
| 5. Ch1 缺进程概念抽象 | §1.1 [NEW] | ✅ |
| 6. enable_user_io 缺失 | §4.1 [NEW] + §5.1 [NEW] | ✅ |
| 7. 错误处理与不变量缺失 | §3.13 [NEW] | ✅ |
| 8. 调度字段与统计缺失 | §2.3 [NEW] + §3.11 [NEW] + §4.7 + §5.2 [NEW] | ✅ |
| 9. boot→running 转换缺失 | §1.6 + §2.8 + §3.12 + §4.8 + §5.3 [全 NEW] | ✅ |
| 10. post-init 缺失 | §2.9 [NEW]（范围边界） | ✅ |

### 开发文档味修复验证（对照 06-structure.md §四）

| 位置 | 旧叙事（删除） | 新叙事（假设性推理） | 状态 |
|------|--------------|-------------------|------|
| §3.1 | "最初的 Rust 设计用 3 trait..." | "如果用 3 trait 镜像 C 的 3 函数，会暴露三个问题..." | ✅ |
| §3.1 | "旧 Rust 设计用 Box<[KProcess]>..." | "如果在 boot 期用 Box<[KProcess]> 堆分配，会破坏零堆约束..." | ✅ |
| §3.7 | "最初的实现需要调用方手动传 6 参数..." | "如果让调用方手动传 6 参数，会有三个问题..." | ✅ |
| §3.8 | "我们最初用 Box，后来改成固定数组..." | "boot 期没有堆分配器，所以 ProcessTable 必须用固定数组..." | ✅ |
| §3.10 | "旧版 KPriv 是单一大结构体..." | "如果把 priv 的 30+ 字段平铺在一个结构体里..." | ✅ |
| §4.4 | "我们后来加了 fork_from..." | "fork 运行时路径需要继承父进程的 FPU 策略..." | ✅ |

---

## 大纲设计说明

### 教学性设计
1. **概念先行**：Ch1 从 CPU/OS 视角建立心智模型，不讲函数名；§1.1 用"断点续传"讲清进程本质，§1.2 用 CPU 四问建立统一框架
2. **本质机制**：Ch3 每节先讲"本质是什么"，再讲"约束如何驱动设计"，最后讲"假设性推理"
3. **纵向链路**：每概念组 Ch1→Ch2→Ch3→Ch4→Ch5 全链路覆盖，读者可按概念追踪
4. **架构范围标注**：x86-only / aarch64 / riscv64 / 三架构统一，每处明确标注
5. **KEY 知识点突出**：§3.6 FPU 架构演进是"rewrite not translate"的典范，单独成节深入讲

### 读者友好性设计
1. **Ch1.0 全景表**：让读者知道"自己在哪"
2. **Ch2.0 源码地图**：让读者先有"地图"再深入
3. **Ch4.0 实现地图**：先有地图再深入
4. **§3.15 设计决策汇总表**：一表收束 Ch3
5. **§5.5 测试覆盖矩阵**：一表收束 Ch5
6. **附录 A**：libexec 框架详解作为 §2.7 的补充，避免主文档过载

### 理论深度设计
1. **§1.1 进程本质**：CPU 时间的"断点续传"
2. **§1.3 位图 vs 枚举**：多原因叠加的本质
3. **§1.4 鸡生蛋问题**：OS 设计普遍模式
4. **§1.2 CPU 四问**：从 CPU 视角提取统一框架
5. **§3.6 FPU 架构演进**：三架构现代模型的本质差异
6. **§3.13 不变量表达**：Option 替代"always-embedded + flag"的类型系统力量

### 文档规模预估
- Ch1 概述：~350 行
- Ch2 C 源码分析：~450 行（含 §2.8/§2.9 新增）
- Ch3 Rust 设计决策：~600 行（含 §3.1-§3.4/§3.11-§3.13 新增）
- Ch4 实现详解：~400 行（含 §4.8 新增）
- Ch5 测试要点：~150 行
- 附录 A：~100 行
- Ch6 参见：~20 行
- **总计**：~2070 行（与 06-structure.md §五 方案 A 预估 ~2200 行一致）