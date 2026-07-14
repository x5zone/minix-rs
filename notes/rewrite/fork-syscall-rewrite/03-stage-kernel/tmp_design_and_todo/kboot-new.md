# kboot-new: 03-stage-kernel 启动叙事最终文档规划

> **创建**: 2026-06-11
> **触发**: 01-06 已实现，需确认 05/06 是否合适，并规划剩余文档
> **方法**: 逐行追踪 Minix3 C 源码 `main.c:96-324`，以时间线重新评估覆盖
> **决策原则**: 一本账原则 —— 每个 C 函数调用只属于一篇文档，每篇文档覆盖一个"语义独立"的阶段

---

## 0. 总览：Minix3 kmain 完整时间线与文档覆盖

### 0.0 两个世界：C 的 32 位 vs Rust 的 64 位 + Direct Map

在阅读本规划之前，需要理解一个关键区别：**Minix3 C 源码是 32 位 x86 的，Rust 目标是 64 位**。位宽差异带来了根本性的架构变化：

| 方面 | Minix3 C（32 位 x86） | minix-rs（64 位 + Direct Map） |
|------|----------------------|-------------------------------|
| 地址空间 | 4GB 虚拟地址空间 | 48 位（256TB）虚拟地址空间 |
| 内核访问物理内存 | 无直接映射，需要 `createpde` 临时窗口 | `kernel_phys_to_virt(pa) = KERNEL_DIRECT_MAP_BASE + pa` |
| 跨进程内存拷贝 | `lin_lin_copy` → `freepdes` 临时 PDE 映射 | 翻译 VA→PA → `kernel_phys_to_virt()` → 直接 memcpy |
| 访问进程页目录 | `pagedir_mappings` 登记册 + `p_cr3_v` 窗口 | `kernel_phys_to_virt(cr3_phys)` 一行加法 |
| 页目录项大小 | 4 字节 PDE | 8 字节 PDE |
| 大页粒度 | 4MB（PDE 映射） | 1GB（PDPT 映射）或 2MB（PD 映射） |

**Direct Map 的核心影响**：64 位地址空间足够大，可以把全部物理内存线性映射到内核虚拟地址空间的高位。`kernel_phys_to_virt()` 就是一行加法——Minix3 C 的 `createpde`/`freepdes`/`pagedir_mappings` 全部被消除。

**本文档的立场**：01-07 的 Ch1-2 必须忠实记录 Minix3 C 的行为（Ground Truth），Ch3-4 可以进行 Direct Map 演进设计。但文档内容本身不因 Direct Map 而消失——freepdes 分配了 2 个槽位，这是 C 事实；Direct Map 不需要它们，这是 Rust 设计决策。两者不矛盾。

> **Kernel Direct Map 由谁建立？** 这是一个关键的设计决策。VM 是页表所有者，从语义哲学上，kernel direct map 应该由 VM 通过 `map_kernel()` 建立。详细论证见 §5.1。

### 0.1 时间线表（以 main.c 行号为序）

| 阶段 | 标记 | main.c 行号 | C 源码操作 | 现有文档 | 状态 |
|------|------|-----------|-----------|---------|------|
| **T0** | boot-shim | — | UEFI → boot-shim 加载 kernel ELF + boot modules → ExitBootServices | 01 | ✅ 已覆盖 |
| **T1** | HigherHalf | — | head.S 切栈跳高地址、ELF 加载器、trampoline | 02 | ✅ 已覆盖 |
| **T2** | kmain 入口 | L115-147 | memcpy(&kinfo)、BSS 检查、`kernel_may_alloc=1`、memcpy(boot_procs)、**cstart()** → prot_init | 03 | ✅ 已覆盖 |
| **T3** | cstart 後半 | L403-481 | init_clock()、intr_init()、arch_init() | 04 | ✅ 已覆盖 |
| **T4** | BKL + 进程表 | L149-163 | BKL_LOCK()、proc_init()、IPCF_POOL_INIT()、NR_BOOT_MODULES 检查 | 05 | ✅ 已覆盖 |
| **T5** | boot 进程加载 | L157-282 | for loop: 特权分配 → arch_boot_proc → VM ELF 解析+页表映射 → VMINHIBIT 设置 | 05 | ✅ 已覆盖 |
| **T6** | post-init | L283-290 | arch_post_init()（ptproc=VM, pg_info）、IPCNAME 宏 | 06 | ✅ 已覆盖 |
| **T7** | freepdes | L293 | memory_init()（分配 2 个 freepde 临时映射槽位）⚠️ Direct Map 消除 | 06 | ✅ 已覆盖 |
| **T8** | system_init | L294-295 | system_init()：系统调用向量注册、IRQ hook 初始化、alarm timer 初始化 | — | ❌ **P0 缺失** |
| **T9** | bootstrap 回收 | L301 | add_memmap(&kinfo, bootstrap_start, bootstrap_len) | — | ❌ **P0 缺失** |
| **T9.5** | SMP 初始化 | L299-323 | CONFIG_SMP: smp_init() 或 smp_single_cpu_fallback()；非 SMP: 直接 bsp_finish_booting() | — | ❌ **P0 缺失** |
| **T10** | 启动完成 | L316/324 | bsp_finish_booting()：cpu_identify、announce、解除 RTS_PROC_STOP、timer 初始化、fpu_init、kernel_may_alloc=0、**switch_to_user()** | — | ❌ **P0 缺失** |
| **T11** | VM 启动 | **switch_to_user 后** | VM 第一个运行 → init_page_table() → **map_kernel() 建立 kernel direct map** → VMCTL_SETADDRSPACE（kernel 切换 CR3）→ VMCTL_KERN_PHYSMAP → VMCTL_KERN_MAP_REPLY → VMCTL_VMINHIBIT_CLEAR | 02-stage-vm 的 26-vm-init-main | ⚠️ **跨目录引用** |
| **T12** | 运行时跨空间 | VM running 后 | createpde() / lin_lin_copy() / vm_memset() / vm_lookup() / VMSUSPEND ⚠️ Direct Map 消除前三个 | tmp-02 | ❌ **P1 需重定位** |

### 0.2 现有 01-06 的判断

#### 01-boot-shim-bootstrap.md ✅ 保持

- **覆盖**: boot-shim 引导准备。从 UEFI/OpenSBI 获取内存映射、加载内核 ELF 与 boot 模块、构造 KernelInfo、ExitBootServices、调用 arch_boot
- **Minix3 对应**: GRUB → pre_init() → pg_identity → pg_mapkernel → pg_load → vm_enable_paging → return &kinfo
- **判断**: 完好。01 文档结束点（分页开启）是自然叙事边界。01 结尾已经提到了 HigherHalf 跳转到 02。

#### 02-higher-half-kernel.md ✅ 保持

- **覆盖**: 链接脚本、内核 ELF 加载（ELF 解析、段拷贝、BSS 清零）、arch_boot_impl 页表映射、HigherHalf 切栈跳转
- **Minix3 对应**: head.S 三行（切栈 + push kinfo + call kmain）
- **判断**: 完好。这是 01 到 kmain 之间的桥梁，独立文档正确。

#### 03-kmain-cstart.md ✅ 保持

- **覆盖**: kmain 入口（memcpy kinfo、BSS 检查、kernel_may_alloc=1）+ cstart 前半（prot_init：GDT/IDT/TSS）
- **Minix3 对应**: main.c:115-143（入口）+ protect.c:321-367（prot_init）
- **判断**: 完好。将"入口"和"保护结构初始化"合并是合理的——prot_init 是 cstart 的第一步且是保护结构的核心。

#### 04-clock-interrupt-init.md ✅ 保持

- **覆盖**: cstart 後半（init_clock、intr_init、arch_init）
- **Minix3 对应**: clock.c:48-74 + i8259.c:28-63 + arch_system.c:246-288
- **判断**: 完好。时钟和中断是 cstart 的两大子阶段，独立文档合理。

#### 05-proc-init-boot-proc.md ✅ 保持（内容充实，无需拆分）

- **覆盖**: proc_init() + boot image 循环（特权分配 + arch_boot_proc + VM ELF 加载 + VMINHIBIT）
- **Minix3 对应**: proc.c:119-160 + main.c:157-282 + protect.c:379-456
- **行数**: ~400 行（Ch1-4）
- **判断**: **内容充实，无需拆分。** proc_init 和 arch_boot_proc 是一个不可分割的语义单元——proc_init 创建空容器，arch_boot_proc 填充内容。分开反而不自然。

#### 06-cross-space-init.md ✅ 保持（虽薄但概念独立）

- **覆盖**: arch_post_init()（ptproc=VM, pg_info）+ memory_init()（freepdes 分配）
- **Minix3 对应**: protect.c:370-377 + memory.c:707-717
- **行数**: ~300 行（Ch1-4，含完整的 createpde 导览和设计决策）
- **C 源码行数**: arch_post_init 5 行 + memory_init 7 行 + mem_clear_mapcache 13 行 = 25 行
- **判断**: **虽 C 源码薄，但概念独立，保持独立文档。** 理由：
  1. ptproc + freepdes 是 Minix3 C 中"跨地址空间访问"的基础设施，语义边界清晰
  2. 它们处于"进程就绪 → 运行时可用"的枢纽位置
  3. 文档内容已经充分展开（createpde 导览、64 位适配、BKL 安全性、Rust trait 设计）
  4. 如果合并到 05 或 07，会导致合并后的文档语义混杂（进程初始化 + 页表机制 + 系统调用初始化）
- **Direct Map 演进**：freepdes 在 64 位 Direct Map 方案下被消除（`kernel_phys_to_virt()` 替代），但 06 的 Ch1-2 忠实记录 C 行为仍然有价值。Ch3-4 应标注 Direct Map 对 freepdes/ptproc 的替代关系。详细论证见 §5.1。

---

## 1. 必须新增的文档（P0）

### 07-system-init-boot-finish: 系统调用初始化与启动完成

> **分类**: 全局基建
> **源码**: `minix3/minix/kernel/system.c:168-278`, `minix3/minix/kernel/arch/i386/pg_utils.c:86-125`, `minix3/minix/kernel/main.c:38-117`
> **前置**: [06-cross-space-init.md](06-cross-space-init.md) — ptproc 已设置、freepdes 已分配

#### 覆盖范围

| 子阶段 | C 函数 | C 源码位置 | 行数 |
|--------|--------|----------|------|
| T8: 系统调用初始化 | `system_init()` | system.c:168-278 | ~110 行 |
| T9: bootstrap 回收 | `add_memmap()` | pg_utils.c:86-125 | ~40 行 |
| T10: 启动完成 | `bsp_finish_booting()` | main.c:38-117 | ~80 行 |

**为什么合并为一个文档？** 这三步是 kmain 的最后三行调用，总 C 源码 ~200 行。system_init 和 add_memmap 各自过薄，bsp_finish_booting 是终点。合并后形成"kmain 的最后阶段：从系统调用就绪到第一个用户进程"的完整叙事。

#### 各子阶段内容

**T8: system_init()** — system.c:168-278

```
1. IRQ hook 初始化（NR_IRQ_HOOKS 个槽位标记为 NONE）
2. Alarm timer 初始化（遍历 BEG_PRIV_ADDR→END_PRIV_ADDR 所有 priv 结构的 s_alarm_timer）
3. 系统调用向量注册（call_vec[] 映射，先全部置 NULL 再逐个 map）：
   - 进程管理：SYS_FORK, SYS_EXEC, SYS_CLEAR, SYS_EXIT, SYS_PRIVCTL, SYS_TRACE, SYS_SETGRANT, SYS_RUNCTL, SYS_UPDATE, SYS_STATECTL
   - 信号：SYS_KILL, SYS_GETKSIG, SYS_ENDKSIG, SYS_SIGSEND, SYS_SIGRETURN
   - 设备 I/O：SYS_IRQCTL, SYS_DEVIO（x86 only）, SYS_VDEVIO
   - 内存管理：SYS_MEMSET, SYS_VMCTL
   - 拷贝：SYS_UMAP, SYS_UMAP_REMOTE, SYS_VUMAP, SYS_VIRCOPY, SYS_PHYSCOPY, SYS_SAFECOPYFROM, SYS_SAFECOPYTO, SYS_VSAFECOPY
   - 安全写入：SYS_SAFEMEMSET
   - 时钟：SYS_TIMES, SYS_SETALARM, SYS_STIME, SYS_SETTIME, SYS_VTIMER
   - 系统控制：SYS_ABORT, SYS_GETINFO, SYS_DIAGCTL
   - 性能分析：SYS_SPROF
   - 调度：SYS_SCHEDULE, SYS_SCHEDCTL
   - 机器状态：SYS_SETMCONTEXT, SYS_GETMCONTEXT
   - 架构特定：SYS_READBIOS, SYS_IOPENABLE, SYS_SDEVIO（x86）, SYS_PADCONF（ARM）
```

**T9: add_memmap(bootstrap)** — pg_utils.c:86-125

```
1. 将 bootstrap_start→bootstrap_end 范围加入 memmap，标记为 MULTIBOOT_MEMORY_AVAILABLE
2. 4GB 截断（Minix3 32 位地址空间限制遗留，Rust 64 位版不需要；源码注释 "rest of minix can't deal with any bigger"）
3. assert(kernel_may_alloc) — 确保回收发生在 VM 启动前
4. 更新 mem_high_phys
```

**T10: bsp_finish_booting()** — main.c:38-117

```
1. cpu_identify() — CPU 型号检测
2. vm_running = 0 — VM 还没开始跑
3. krandom 初始化
4. bill_ptr = idle_proc, proc_ptr = idle_proc
5. announce() — 打印 MINIX 启动 banner
6. 解除 boot_proc（不含 kernel tasks）的 RTS_PROC_STOP：`for (i=0; i < NR_BOOT_PROCS - NR_TASKS; i++) RTS_UNSET(proc_addr(i), RTS_PROC_STOP)`
7. cycles_accounting_init() — CPU 记账
8. boot_cpu_init_timer(system_hz) — 启动 BSP 的时钟中断（100Hz）
9. fpu_init() — FPU 初始化
10. CPU 计数设置（machine.processors_count, machine.bsp_id）
11. kernel_may_alloc = 0 — 禁止内核分配内存
12. switch_to_user() — ★ 进入调度循环，永不返回
```

#### 关键设计决策

| 决策 | 选项 | 结论 |
|------|------|------|
| system_init 的 Rust 表达 | 全局 call_vec 数组 vs 分派表结构体 | 分派表结构体 + trait 方法 |
| add_memmap 的 4GB 截断 | 保留 vs 删除 | **删除**（64 位不需要，这是 32 位 Minix3 的 PAE 限制遗留） |
| bsp_finish_booting 中的 timer 初始化 | 在 switch_to_user 之前 vs 之后 | 保持之前（需要时钟中断驱动调度） |
| switch_to_user 是否展开 | 概要提及 vs 详细展开 | 概要提及（调度循环是 tmp-07-scheduling 的主题） |

#### 与 tmp- 文件的关系

- `tmp-17-main-init.md` 覆盖了 system_init → bsp_finish_booting 但**叙事是非时间线（参考资料式）**。07 以时间线重新组织。
- `tmp-07-scheduling.md` 覆盖了 switch_to_user 之后的调度循环。07 只需要概要提及 switch_to_user 的入口语义。

---

### 可能新增的文档（P1）

### 08-vm-boot-protocol: VM 启动后的内核-VM 协商协议

> **分类**: Kernel IPC 协议
> **源码**: `minix3/minix/kernel/arch/i386/do_vmctl.c`, `minix3/minix/servers/vm/`（相关 VMCTL handler）
> **前置**: [07-system-init-boot-finish](07-system-init-boot-finish.md) — bsp_finish_booting 已完成，VM 已开始运行

#### 覆盖范围

switch_to_user() 之后的时序（见 02-stage-vm 的 [26-vm-init-main.md](../02-stage-vm/26-vm-init-main.md)）：

```
1. 调度器选择 VM（唯一没有 VMINHIBIT 的进程）
2. VM 运行 → 初始化自身 → 丢弃 bootstrap 页表
3. VM 调用 init_page_table() → map_kernel()
   → ★ 建立 kernel direct map（KERNEL_DIRECT_MAP_BASE, U/S=0, G=1）
   → 这是 VM 作为页表所有者对内核页表映射的"授权"
4. VM → SYS_VMCTL(VMCTL_SETADDRSPACE):
   → arch_do_vmctl → write_cr3(vm->p_seg.p_cr3)  ← kernel 切换到 VM 的真实页表
   → arch_enable_paging → switch_address_space
   → video_mem = video_mem_vaddr
5. VM → SYS_VMCTL(VMCTL_KERN_PHYSMAP):
   → arch_phys_map() — 内核声明需要映射的物理区域
6. VM → SYS_VMCTL(VMCTL_KERN_MAP_REPLY):
   → arch_phys_map_reply() — 内核获得虚拟地址
7. VM 为 PM/VFS/RS 等创建页表
8. VM → SYS_VMCTL(VMCTL_VMINHIBIT_CLEAR):
   → 解除 PM/VFS/RS 的 RTS_VMINHIBIT
   → 其他 boot 进程开始调度
9. vm_running = 1
```

#### 为什么可能不需要独立文档？

VMCTL 协议是**运行时 IPC 协议**，不是 boot 阶段独有的。08 的内容其实是 `SYS_VMCTL` 这个内核调用的完整语义——它应该在系统调用文档（tmp-13-syscall-memory.md 或 tmp-12-syscall-dispatch.md）中覆盖。

**决策**：**暂不独立成篇**，将 VMCTL 的 4 个子命令分散到对应文档中：
- `VMCTL_SETADDRSPACE` → 07-system-init-boot-finish 的 "VM 的第一个动作" 小节
- `VMCTL_KERN_PHYSMAP` / `VMCTL_KERN_MAP_REPLY` → 02-stage-vm 的 [07-pagetable-ops.md](../02-stage-vm/07-pagetable-ops.md)（kernel direct map 的前置条件）
- `VMCTL_VMINHIBIT_CLEAR` → 07-system-init-boot-finish 的 "启动完成后的调度" 小节
- `map_kernel()` → 02-stage-vm 的 [07-pagetable-ops.md](../02-stage-vm/07-pagetable-ops.md)（kernel direct map 的建立）

---

### 09-runtime-cross-space: 运行时跨地址空间访问机制

> **分类**: Kernel 内存管理
> **源码**: `minix3/minix/kernel/arch/i386/do_vmctl.c`, `minix3/minix/kernel/arch/i386/do_vcopyf.c`
> **前置**: [06-cross-space-init](06-cross-space-init.md) — ptproc + freepdes 已就绪；[07-system-init-boot-finish](07-system-init-boot-finish.md) — VM 已运行

#### 覆盖范围

- createpde() — 临时 PDE 映射（Minix3 C 跨地址空间访问的核心）
- lin_lin_copy() / virtual_copy_f() — 跨进程内存拷贝
- vm_memset() — 跨进程内存清零
- vm_lookup() — 跨进程页表遍历
- VMSUSPEND 机制 — 缺页挂起协议
- mem_clear_mapcache() — 临时映射清理

**Direct Map 重新定义"跨进程拷贝"**：以上 `createpde`/`lin_lin_copy`/`vm_memset` 在 64 位 Direct Map 方案下全部被 `kernel_phys_to_virt()` 替代——"跨进程拷贝"退化为"普通 memcpy"。09 的 Ch1-2 必须忠实记录 C 源码行为，Ch3-4 应标注 Direct Map 如何消除这些机制。详见 [02-stage-vm/07-pagetable-ops.md §3.0.4-3.0.6](../02-stage-vm/07-pagetable-ops.md)。

`vm_lookup` 和 VMSUSPEND 在 Direct Map 下仍然保留——vm_lookup 用于页表遍历（检查映射是否存在），VMSUSPEND 是缺页挂起协议（与 Direct Map 无关）。

#### 来源

即 `tmp-02-page-table-kernel.md` 的内容重新定位。当前 tmp-02 在 03-stage-kernel 目录中编号错位（旧的 02，已被 02-higher-half-kernel 取代），应在 09 位置重新编号并增加前置条件说明。

---

## 2. 最终文档编号体系

| 编号 | 文件名 | 覆盖 | 状态 |
|------|--------|------|------|
| 01 | `01-boot-shim-bootstrap.md` | boot-shim 引导准备：GRUB/pre_init → paging enable | ✅ 已实现 |
| 02 | `02-higher-half-kernel.md` | HigherHalf：链接脚本、ELF 加载、trampoline、切栈跳高地址 | ✅ 已实现 |
| 03 | `03-kmain-cstart.md` | kmain 入口 + cstart 前半：prot_init（GDT/IDT/TSS） | ✅ 已实现 |
| 04 | `04-clock-interrupt-init.md` | cstart 後半：init_clock、intr_init、arch_init | ✅ 已实现 |
| 05 | `05-proc-init-boot-proc.md` | proc_init + arch_boot_proc：进程表、特权分配、VM ELF 加载 | ✅ 已实现 |
| 06 | `06-cross-space-init.md` | arch_post_init + memory_init：ptproc=VM、freepdes 分配 | ✅ 已实现 |
| **07** | **`07-system-init-boot-finish.md`** | **system_init + add_memmap + bsp_finish_booting → switch_to_user** | ❌ **需新建** |
| — | (08 暂不独立) | VMCTL 协议分散到 07 和 09 | ⏸️ 合并 |
| 09 | `09-runtime-cross-space.md` | createpde / lin_lin_copy / vm_memset / vm_lookup / VMSUSPEND | ❌ 需重定位 |

### 2.1 编号跳跃说明

08 号空缺是故意的：VMCTL 协议作为运行时 IPC 协议，更适合放在系统调用文档中，而不是作为 boot 线的一部分。如果后续发现需要独立文档，可以填入 08。

### 2.2 tmp- 文件的最终归属

| tmp- 文件 | 内容 | 归属 |
|-----------|------|------|
| `tmp-02-page-table-kernel.md` | createpde/lin_lin_copy/vm_memset/vm_lookup | → 重命名为 09-runtime-cross-space.md |
| `tmp-04-protection.md` | prot_init (GDT/IDT/TSS) | → 已被 03 覆盖，改为 `ref-04-protection.md` 移入 reference/ |
| `tmp-05-exception-interrupt.md` | 异常/中断处理 | → 已被 04 覆盖，改为 `ref-05-exception-interrupt.md` |
| `tmp-07-scheduling.md` | pick_proc/enqueue/dequeue/switch_to_user | → 调度循环是独立的子系统文档（不属于 boot 线），保留 tmp- 前缀，待后续纳入调度子系统文档体系 |
| `tmp-17-main-init.md` | kmain 全流程（参考资料式） | → 已被 03-07 拆分覆盖，改为 `ref-17-main-init.md` |

---

## 3. 与 kboot-design.md / kboot-todo.md 的区别

| 方面 | kboot-design / kboot-todo | kboot-new（本文） |
|------|--------------------------|-------------------|
| 文档编号 | 提出 02=boot-bridge, 03=kmain-boot, 04=vm-boot, 05=runtime | 发现 01-06 已经实现了更细粒度的拆分，沿用现有编号 |
| 05/06 的处理 | 未讨论（那时还没有 05/06） | 评估后判断：05 内容充实保留，06 虽薄但概念独立保留 |
| VMCTL 协议 | 独立 04-vm-boot-protocol | 判断为运行时 IPC 协议，分散到 07/09 |
| 新文档数 | 5 篇（02-05 + 01 修改） | 1 篇必须新增（07），1 篇需重定位（09），其余已完成 |

---

## 4. 实现优先级

| 优先级 | 任务 | 类型 | 依赖 |
|--------|------|------|------|
| **P0** | 撰写 `07-system-init-boot-finish.md` | 文档新建 | 06 完成 |
| **P1** | 将 `tmp-02-page-table-kernel.md` 重定位为 `09-runtime-cross-space.md` | 文档重命名+内容修订 | 07 完成 |
| **P2** | 清理 tmp- 文件，移入 `reference/` 子目录 | 文档清理 | 07+09 完成 |
| **P2** | 更新 `00-kernel-overview.md` 的文档索引 | 文档维护 | 全部完成 |

---

## 5. 设计决策记录

### 5.1 Kernel Direct Map 由谁建立？Kernel 还是 VM？

这是整个 Direct Map 架构中最重要的设计决策——它决定了 kernel 和 VM 的职责边界。

#### 5.1.1 双视图地址空间模型

从 02-stage-vm 的 [06-pagetable-struct.md §3.6.0](../02-stage-vm/06-pagetable-struct.md)，VM 进程的地址空间包含两个 direct map：

| 属性 | VM direct map | Kernel direct map |
|------|--------------|-------------------|
| 虚拟地址基址 | `0x0000_0000_8000_0000` | `0xFFFF_8000_0000_0000` |
| U/S 位 | 1（用户态可访问） | 0（仅内核态可访问） |
| G 位 | 0 | 1（CR3 切换不刷新 TLB） |
| 建立者 | **Kernel**（arch_boot_proc 初始页表） | **VM**（`map_kernel()`） |
| 建立时机 | VM 启动前（T5 阶段） | VM 启动后（T11 阶段） |
| 修改者 | VM（Phase 2 扩展） | 无人（只读不变量） |

**这不是"两份映射"，而是"同一物理内存在不同特权级下的两个必要窗口"**。x86-64 的特权级硬件要求：
- VM（ring 3）通过 U/S=1 的 PTE 访问物理内存
- Kernel（ring 0）通过 U/S=0 的 PTE 访问物理内存

一个 PTE 的 U/S 位不可能同时为 0 和 1，因此两个窗口是硬件的必然要求。

#### 5.1.2 选项分析

| 选项 | 描述 | 优点 | 缺点 |
|------|------|------|------|
| **A: Kernel 建立** | Kernel 在 T5 的初始页表中同时建立 VM direct map 和 kernel direct map | 简单，kernel direct map 从一开始就可用 | 违反语义——kernel 不应单方面决定 VM 地址空间的映射内容；即使布局已知，kernel direct map 是"VM 授权 kernel 以 ring 0 特权访问物理内存"的映射，授权者应是 VM 而非被授权者自己 |
| **B: VM 建立** | Kernel 仅建立 VM direct map（VM 自举需要），kernel direct map 由 VM 在 `map_kernel()` 中建立 | 语义正确：VM 是页表所有者，kernel direct map 是 VM 对 kernel 的"授权"；VM 可以精确控制 kernel 看到什么物理内存；与 Minix3 的 `map_kernel` 职责一致 | kernel direct map 在 VM 启动前不可用；但 kernel 在 boot 阶段不需要 direct map（只有单一地址空间，不需要跨空间访问） |

#### 5.1.3 结论：VM 建立（选项 B）

**决策**：kernel direct map 由 VM 通过 `map_kernel()` 建立。理由：

1. **语义哲学**：VM 是页表所有者。kernel direct map 是"VM 授权 kernel 以 ring 0 特权访问所有物理内存"的页表映射。从语义上，授权者（VM）应该主动建立映射，而不是被授权者（kernel）自己建立。

2. **Boot 时序不冲突**：boot 阶段 kernel 不需要 kernel direct map——只有单一地址空间，不需要跨空间访问。kernel direct map 在 VMCTL_SETADDRSPACE 之前建立即可（VM 在 `init_page_table()` 中调用 `map_kernel()`，然后才 `VMCTL_SETADDRSPACE`）。

3. **与 Minix3 一致**：Minix3 的 `pt_mapkernel()` 本来就是 VM 调用的——VM 在 `pt_init()` 中调用 `pt_mapkernel()`，将内核映射写入页表。Direct Map 方案只是扩展了 `map_kernel()` 的职责（增加了 kernel direct map），没有改变建立者。

4. **安全边界**：VM 作为内存管理器，应该控制 kernel 能看到什么物理内存。kernel direct map 是"一次性建立、不再修改"的只读不变量，由 VM 在初始化时精确建立，符合最小权限原则。

#### 5.1.4 对文档规划的影响

- **06-cross-space-init.md**：freepdes 是 Minix3 C 的机制，在 Direct Map 下被消除。Ch3-4 应标注此演进。
- **09-runtime-cross-space.md**：createpde/lin_lin_copy 是 Minix3 C 的机制，在 Direct Map 下被消除。Ch3-4 应标注替代关系。
- **T11 (VM 启动)**：`map_kernel()` 建立 kernel direct map 是 VM 启动的关键步骤，不在 03-stage-kernel 中独立成篇，而是跨目录引用 [02-stage-vm/26-vm-init-main.md](../02-stage-vm/26-vm-init-main.md) 和 [02-stage-vm/07-pagetable-ops.md](../02-stage-vm/07-pagetable-ops.md)。

### 5.2 其他设计决策

| 决策 | 选项 A | 选项 B | 选择 | 理由 |
|------|--------|--------|------|------|
| 05 是否保留 | 保留 | 拆分为 proc_init + arch_boot_proc | **保留** | proc_init 是"创建容器"，arch_boot_proc 是"填充内容"，属同一语义单元 |
| 06 是否合并 | 保留独立 | 合并到 05 或 07 | **保留独立** | C 源码虽薄（25行），但 ptproc+freepdes 概念独立，文档已充分展开 |
| 07 覆盖范围 | system_init 独立一篇 + bsp_finish 独立一篇 | 合并三者为 07 | **合并** | 三者是 kmain 最后三行调用，各自过薄，合并后形成完整叙事 |
| 08 是否独立 | VMCTL 协议独立文档 | 分散到 07/09 | **分散** | VMCTL 是运行时 IPC，不是 boot 独有，独立文档会与系统调用文档重叠 |
| add_memmap 的 4GB 截断 | 保留 | 删除 | **删除** | Minix3 32 位地址空间限制遗留（源码注释 "rest of minix can't deal with any bigger"），64 位不需要 |

---

## 6. 参考资料

- [00-kernel-overview.md](00-kernel-overview.md) — 内核概览与"严格线性 boot 过程"的承诺
- [01-boot-shim-bootstrap.md](01-boot-shim-bootstrap.md) — boot-shim 引导准备
- [02-higher-half-kernel.md](02-higher-half-kernel.md) — HigherHalf 内核
- [03-kmain-cstart.md](03-kmain-cstart.md) — kmain 入口与保护结构
- [04-clock-interrupt-init.md](04-clock-interrupt-init.md) — 时钟与中断初始化
- [05-proc-init-boot-proc.md](05-proc-init-boot-proc.md) — 进程表初始化与 VM ELF 加载
- [06-cross-space-init.md](06-cross-space-init.md) — 架构后初始化与临时页表槽位
- [kboot-design.md](kboot-design.md) — 架构设计方案（链接脚本、trampoline、ELF 加载器）
- [kboot-todo.md](kboot-todo.md) — TODO 列表与决策记录
- [kboot-problem.md](kboot-problem.md) — 问题发现与完整 kmain 序列
- 02-stage-vm 参考资料:
  - [02-stage-vm/06-pagetable-struct.md](../02-stage-vm/06-pagetable-struct.md) — 双视图 Direct Map 地址空间布局
  - [02-stage-vm/07-pagetable-ops.md](../02-stage-vm/07-pagetable-ops.md) — `map_kernel()` 与 kernel direct map 建立
  - [02-stage-vm/26-vm-init-main.md](../02-stage-vm/26-vm-init-main.md) — VM 启动流程与分阶段初始化
  - [02-stage-vm/04-physical-memory.md](../02-stage-vm/04-physical-memory.md) — 3 阶段启动与 Direct Map 自举
  - [02-stage-vm/05-vm-allocpage.md](../02-stage-vm/05-vm-allocpage.md) — Direct Map 消除递归的设计论证
- Minix3 C 源码:
  - `minix3/minix/kernel/main.c:96-324` — kmain 完整实现
  - `minix3/minix/kernel/system.c:168-278` — system_init()
  - `minix3/minix/kernel/arch/i386/pg_utils.c:86-125` — add_memmap()
  - `minix3/minix/kernel/main.c:38-117` — bsp_finish_booting()
  - `minix3/minix/kernel/proc.c:119-160` — proc_init()
  - `minix3/minix/kernel/arch/i386/protect.c:370-456` — arch_post_init + arch_boot_proc
  - `minix3/minix/kernel/arch/i386/memory.c:707-722` — memory_init()