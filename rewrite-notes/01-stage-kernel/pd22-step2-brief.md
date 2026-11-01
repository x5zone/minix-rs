# PD-22 第二步评审简报：恒等窗口退出进程根（de-depend and exit）

本文面向**看不到代码仓库的外部评审人**。它是 PD-22 三步中的**第二步**设计评审材料：把"普通进程的地址空间里不再存在恒等映射"从验收条文变成实施方案。第①步（进程根的内核文本映射去写）已于本批落地；本文只处理第二步，自包含，不需要先读任何其他文档。

## 0. 背景与阅读指引

### 0.1 项目是什么

本项目是 Minix3 的 Rust 语义重写。Minix3 是微内核结构的教学操作系统：内核只保留调度、消息传递与底层内存管理，其余服务以用户态进程形式运行。重写保持外部行为、内部用 Rust 类型系统重新表达，支持 x86-64、AArch64、RISC-V 64 三个架构，全部 `#![no_std]`。验证形态是 QEMU 实际启动内核并运行用户态测试框架（ATF，全量 36 项用例）。

### 0.2 PD-22 与三步的关系

PD-22 是一条已冻结的裁决（本项目裁决编号族的一员），把"内核地址空间 W^X 化"这笔结构债拆成三步：

- **第①步（已落地）**：进程根里的内核文本映射去掉写权限（RX）。已经以"乙案′"形态实施——文本边界来自链接符号 `__text_end`，进程根由 `map_kernel` 在构造时直接赋予 RX，引导根由内核在启动早期自行分割。落地面：`rewrite-notes/01-stage-kernel/c-board-rulings-brief.md` §10–§12。
- **第②步（本文的主题）**：**恒等窗口从普通进程地址空间整体退场**。冻结验收原文："after the switch+first-jump+high-half interval, identity is no longer in the normal process address space (not 'keep but shrink permissions')"——**强形态**：要求"不存在"，而不是"存在但权限更小"。
- **第③步（引导根分割，标注为 optional）**：实际上已随①提前落地——内核在启动早期把引导根高半的文本大叶分割成 4 KiB 页并收紧为 RX。

### 0.3 怎么读

§1 讲机制背景（进程根、窗口重放、恒等窗是谁），§2 盘点三架构现状（一个重要的架构分化事实），§3 给出设计空间与候选方案，§4 列裁决点与申请方立场，§5 是术语表与锚点清单。评审人只需对 §4 的裁决点逐一表态。

## 1. 机制背景：进程根里现在重放了什么

### 1.1 进程根与窗口重放

操作系统里每个进程有自己的**地址空间**，由一张页表（的根，下称**进程根**）描述。本项目的进程根由四段映射组成，全部在进程构造时由一个叫 `map_kernel` 的函数逐页建立（对应 Minix3 C 的 `pt_mapkernel`；代码在 `os/arch/src/arch/paging.rs`，调用点 `os/servers/vm/src/vmproc/vmproc_handle.rs` 的 `init_page_table`）：

| 段 | 内容 | 权限（本批后） |
|---|---|---|
| 1 | 内核文本（代码） | **RX**（本批从 RWX 收紧——第①步） |
| 2 | 内核数据（rodata/data/bss 跨距） | RW，不可执行 |
| 3 | **Direct Map 窗**（内核把全部物理内存线性映射的一段，C 对位 `protect.c` 的 glou/glin 窗族） | RW，不可执行 |
| 4 | **恒等窗口**（VA = PA 的映射；本文主角） | RWX ← **第②步要退场的** |

第 4 段的内容由一个叫 `build_identity_windows` 的函数决定（`os/kernel/src/vm_handoff.rs`）：它从架构的 MMIO 设备表取候选、页对齐后合并，经交接协议（`VmBootHandoff` v6 的恒等窗字段）交给 VM，由 `map_kernel` 重放进**每一个**新进程根。

### 1.2 恒等窗里今天装的是什么

一个重要事实（本批第①步实施时经 readelf 与代码核验更正过）：恒等窗**不含内核镜像**。早期注释声称"窗口混有内核 text/data/heap"是 T2（x86 切独立镜像）之前的陈旧叙述；现行生产者的候选**只有架构的 MMIO 设备表**（`kernel_identity_mmio_regions()`，`os/arch/src/lib.rs`）：

- **x86-64**：两页——IO-APIC（0xFEC00000）与 local-APIC（0xFEE00000）。
- **riscv64**：三段——PLIC 窗（0x0C000000；**现窗长 2 MiB 已反证不足**，实际 DTB reg = 6 MiB）+ UART0 页（0x10000000，消费者 `uart_irq.rs`）。（ACLINT SSWI 0x02F00000 是运行期消费者但**不在现行表中**——第三轮误判为 VirtIO/MSWI，第四轮实测纠正，属待补窗。）
- **aarch64**：**空**（见 §2 的架构分化）。

### 1.3 恒等窗的消费者：内核态的中断控制器访问

恒等窗存在的唯一理由：**内核在为用户进程服务时（trap 进入内核），执行流跑在"该进程的根"上——此时内核要读写中断控制器的寄存器，而这些寄存器是 MMIO（内存映射 I/O），访问地址就是它们的物理地址**。具体消费点：

- **riscv64**：三个运行期消费者——① 外部中断腿的 PLIC claim/complete（`os/plat/src/riscv64/interrupt.rs` 的 `plic_read32/write32`，VA=PA；当年重放 PLIC 窗为修 trap 臂 Load fault）；② UART0（`os/kernel/src/uart_irq.rs`，`UART_BASE=0x10000000` volatile 直访，RX hook 走 SEI trap 腿）；③ ACLINT SSWI（`send_sched_ipi` 的 `sswi_setip_base + 4×cpu` 直写，C2ESSWI 运行期撤钳通道——**现行恒等表未覆盖 0x02F00000，属待修缺陷**）。
- **x86-64**：local-APIC 的 EOI 写（`os/plat/src/x86_64/interrupt.rs` 的 `lapic_eoi`：从 MSR 读 APIC 基址 0xFEE00000，`write_volatile(base + EOI)`）与 IO-APIC 的重定向表间接读写——同样是 VA=PA。
- **aarch64**：**没有 MMIO 消费者**——GICv3 的中断确认/完成走**系统寄存器接口**（`ICC_*` 寄存器，指令访问，不经内存），`kernel_identity_mmio_regions()` 恒为空（`os/arch/src/lib.rs` 注释原文："the aarch64 interrupt controller is accessed through system registers… no fixed low-VA MMIO surface"）。

### 1.4 为什么第②步要求"退出"而非"收窄"

强形态的理由来自映射卫生而非"用户态能访问设备"——恒等窗是 supervisor-only，用户态本就不能直接经它访问设备。它的真实代价有三：**普通进程根保留了一份设备物理地址的恒等别名**；**内核设备访问依赖 PA 直接充当 VA**（物理地址布局被冻结进地址空间语义）；设备访问因此没有专用、受控、高半的映射入口。附带的 X 位（RWX 中的可执行）去除只是 W^X 收益，不替代冻结验收的强形态要求：恒等映射必须不存在，收窄权限不算数。冻结验收因此写成"must not exist, not 'keep but shrink permissions'"。参照系是 Linux：内核的 rodata/`set_memory_*` 机制同样以"映射不存在"为终态。

 Minix3 C 的对照：C i386 内核的 8259A 中断控制器走 **端口 I/O**（`in`/`out` 指令，不经内存映射），所以 C 从未有过这个问题——现代中断控制器（LAPIC/PLIC/GIC）全是 MMIO 映射的，"内核如何在开启分页后访问控制器"是 minix-rs 自己必须回答的新问题。这也是整个区域的 `[ARCH]` 标注来源。

## 2. 三架构现状盘点（含一个关键分化）

| 架构 | 恒等窗内容 | 控制器访问方式 | 进程根是否携带恒等窗 | 第②步工作量 |
|---|---|---|---|---|
| x86-64 | IO-APIC + LAPIC 两页 | MMIO（VA=PA） | 是（重放） | 需要改道 |
| riscv64 | PLIC 窗（2 MiB，**反证不足**，实需 6 MiB）+ UART0 页；**SSWI 0x02F00000 缺表** | MMIO（VA=PA） | 是（重放；SSWI 待补） | 需要改道（含窗长修正与 SSWI 补窗） |
| aarch64 | **空** | **系统寄存器**（ICC 接口，无 MMIO） | **否——已合规** | 无 |

**aarch64 已经满足第②步**，且它给出了第三架构的现成范式：aarch64 的设备访问（UART 等外设）走一个**钉死在 TTBR1 的高半 MMIO 窗**——AArch64 有两根基址寄存器（TTBR0 管用户半、TTBR1 管内核半），进程切换只换 TTBR0，TTBR1 永远指向引导根的内核半映射（`os/kernel/src/lib.rs` 的 Step 2b 注释原文："TTBR1 is pinned to the bootstrap root and never switched"）。所以 aarch64 的内核设备访问天然不依赖进程根里的任何重放——这正是第②步想让另两个架构达到的终态形状。

## 3. 设计空间与候选方案

第②步要回答的唯一问题：**进程根不再重放恒等窗之后，内核态的中断控制器访问从哪里读/写？**

### 3.1 甲案（申请方推荐）：高半设备窗，aarch64 范式推广

给 riscv64 与 x86-64 各引入一个**内核专用高半设备窗**：每个进程根额外映射一段固定的高虚拟地址（`DEVICE_VA_BASE`），把 MMIO 设备区映到那里（`DEVICE_VA_BASE + (PA − 设备区基址)`）；恒等窗停止重放；trap 臂的访问点把地址计算从"PA 原值"改成"DEVICE_VA_BASE + 偏移"。

- 优点：与 aarch64 的既有范式（设备访问走高半窗、用户半零恒等）三架构语义统一；窗是**常驻**映射（同 DM 窗的形态），trap 腿零额外开销；恒等窗整体退场，强形态验收直接成立。
- 成本（按第四轮完整消费者清单修订）：① `DEVICE_VA_BASE` 两常量与高半地址碰撞断言（进 check-layout 族）；② 访问点迁移=**四处**——PLIC claim/complete、UART0（`uart_irq.rs`）、SSWI（`send_sched_ipi`）、x86 LAPIC EOI/IPI——外加窗长修正（PLIC 6 MiB）与 SSWI 补窗；③ `build_identity_windows` 与 `map_kernel` 第四段的退场；④ 全部中断门 + UART RX 自检 + SSWI 双形态（aclint=on 有节点/默认无节点）回归。
- 诚实边界：引导根的低 4 GiB 恒等映射维持现状（PD-22 冻结 scope 明写 "runtime process roots only"——引导根不在射程）。

### 3.2 乙案：Direct Map 窗扩展覆盖设备区

把 MMIO 设备区纳入 Direct Map 的候选集，设备访问复用 DM 窗（`DM 基址 + PA`），不引入新窗。
- 不采的理由：DM 的候选资格过滤器以**内存映射表（RAM）**为源——设备区不是 RAM，把它塞进 DM 意味着改 DM 的资格语义（A2 分类、PMM 交互、07 篇的窗口覆盖模型全部要重审），影响面远大于问题本身；且 DM 窗的既有承诺（"覆盖全部可用 RAM"）与"覆盖设备"是两种语义，混在一段窗里会让 §1.1 表的第 3、4 段边界模糊。

### 3.3 丙案：trap 腿现场临时映射

每次外部中断在 trap 腿里现场建立/拆除控制器映射。
- 不采的理由：每次中断两次页表操作 + TLB 失效，把最热路径（中断处理）变成最慢路径；且"临时映射"自身就要可写的页表页，复杂度最高、收益为零。

### 3.4 旗标裁决点（随形状走）

新设备窗的页权限：**RW + 不可执行（NX）**还是维持 RWX？设备寄存器是数据，永远不该被取指——NX 让设备窗也满足 W^X，比今天的恒等窗更干净。风险一侧：aarch64 的 ARM 架构规定同一物理页的两个别名属性不一致时行为 UNPREDICTABLE——但 aarch64 不参与本步（无恒等窗、控制器走系统寄存器），riscv64 与 x86-64 的 NX 语义均无此约束。申请方立场：**RW+NX**。

### 3.5 一个必须如实记录的现状约束

新设备窗映射进每个进程根，意味着"设备寄存器页对 supervisor 可写"这一事实仍随进程根存在——第②步消除的是**恒等映射**（VA=PA 的地址空间语义破坏）与 **X 位**，不消除"supervisor 可写设备寄存器"。这与 Linux 的形态一致（LAPIC 映射同样对内核恒可写）。承诺条款的措辞建议："进程根中不再存在恒等映射与可执行设备映射；设备寄存器经内核专用高半窗以 RW/NX 可达；引导根低 4 GiB 恒等映射不在本步射程。"

## 4. 裁决点（请评审人逐一表态）

1. **替换形状**：甲案（高半设备窗，aarch64 范式推广）是否准？乙/丙的否决理由是否成立？
2. **旗标**：新设备窗 RW+NX（比现状更进一步满足 W^X）还是维持 RWX（保守）？申请方立场 RW+NX。
3. **选址**：`DEVICE_VA_BASE` 放在高半哪一段？申请方立场：riscv64 放在既有内核 DM 窗之前/之后的空档、x86 放在 `KERNEL_HIGH_BASE` 空间内 kerninfo/堆窗之外——两处的具体空档以链接期地址地图为准，实施批给出碰撞检查断言。
4. **验收**：① `build_identity_windows` 恒等窗候选恒空、`map_kernel` 第四段删除（"must not exist" 的静态面）；② 中断门全绿——riscv `test-timer-irq-riscv64`/`test-smp-aps-riscv64`（SGI+timer 门）、x86 `test-timer-irq` 腿、aarch64 `test-timer-irq-aarch64`，加 riscv ATF 36/36 与 `run_all` 全量；③ e5-ds/cmd-smoke 等全链不回归。
5. **范围**：引导根低 4 GiB 恒等映射维持现状（PD-22 冻结 scope），是否确认？

**申请方立场汇总**：甲案 + RW/NX + 实施批给碰撞断言 + 上述验收三面 + 引导根维持现状。

## 5. 实施前需补证的事实（本简报已备齐的部分与遗留部分）

已备齐：恒等窗生产者/消费者全链（§1）、三架构分化（§2）、aarch64 已合规（§2 表）、DM 扩展的语义冲突（§3.2）。遗留两件（实施批开工前补，均为读数而非设计）：① trap 臂访问点的完整清单——riscv 的 `plic_read32/write32` 调用点（claim/complete/threshold 使能三处）与 x86 的 `lapic_write` 调用点（EOI/spurious/IPI 几处），逐点确认无第三类消费者；② `DEVICE_VA_BASE` 空档的碰撞检查——按三架构的高半地址地图（DM 窗、VM 窗、kerninfo、VM 堆、引导栈）逐一对账，产出断言进 `check-layout.sh` 族。

## 6. 术语表

| 术语 | 含义 |
|---|---|
| 进程根 | 一个进程的页表根，描述该进程的完整虚拟地址空间 |
| 恒等映射 / 恒等窗 | VA = PA 的映射；"恒等窗"指进程根里这类映射组成的窗口 |
| Direct Map（DM 窗） | 内核把全部可用 RAM 线性映射的常驻窗口，访问物理内存的常规通道 |
| MMIO | 内存映射 I/O：设备寄存器出现在物理地址空间，像内存一样读写 |
| W^X | 写执行互斥：一页不可同时可写与可执行 |
| 强形态 / "must not exist" | 验收口径：映射不存在，而非存在但权限收窄 |
| map_kernel | 进程根构造函数（C 对位 `pt_mapkernel`），四段映射见 §1.1 |
| build_identity_windows | 恒等窗候选的生成函数（`os/kernel/src/vm_handoff.rs`） |
| PLIC / LAPIC / IO-APIC / GIC | RISC-V 的外部中断控制器 / x86 的本地与 I/O 中断控制器 / AArch64 的中断控制器（走系统寄存器接口） |
| TTBR0/TTBR1 | AArch64 的两根基址寄存器：TTBR0 用户半、TTBR1 内核半；进程切换只换 TTBR0 |
| trap 腿 | 用户态 trap 进内核后，内核处理中断/异常的那段执行 |
| ATF / run_all | QEMU 全量用户态测试框架（36 项）/ 全部 QEMU 门的总跑（39 项） |

## 7. 锚点清单

| 断言 | 锚点 |
|---|---|
| map_kernel 四段构造与调用点 | `os/arch/src/arch/paging.rs:fn map_kernel`；`os/servers/vm/src/vmproc/vmproc_handle.rs:fn init_page_table` |
| 恒等窗候选生成（仅 MMIO 表） | `os/kernel/src/vm_handoff.rs:fn build_identity_windows`；`os/arch/src/lib.rs:fn kernel_identity_mmio_regions` |
| riscv PLIC MMIO 访问（VA=PA） | `os/plat/src/riscv64/interrupt.rs`（`Riscv64InterruptController::plic_read32/plic_write32`，`PLIC_CLAIM + context*0x1000`） |
| x86 LAPIC EOI（VA=PA） | `os/plat/src/x86_64/interrupt.rs:fn lapic_eoi`、`fn lapic_write`（base 0xFEE00000 来自 MSR） |
| aarch64 系统寄存器接口 + TTBR1 钉死 | `os/arch/src/lib.rs`（aarch64 恒等表空注释）；`os/kernel/src/lib.rs` Step 2b 注释（"TTBR1 is pinned to the bootstrap root and never switched"） |
| PD-22 冻结原文 | `rewrite-notes/coordination/STRUCTURAL-DEBT-REGISTER-20261008.md:362`（SD-12 条目） |
| 第①步落地记录 | `rewrite-notes/01-stage-kernel/c-board-rulings-brief.md` §10–§12；C 板 C11 行 |
| PLIC 窗重放的动机（SEI/EI 腿 Load fault 修复） | C 板 C17 行第二片注记；`os/arch/src/arch/paging.rs` 段四注释 |

---

# 第二轮：裁决回填与实施前补证

> **历史快照**：§8–§13 记录各轮纠错过程（含已被第四轮推翻的第三轮地址判断），保留作复盘，**现状以 §1–§7（已修订）与 §14 为准**。

## 8. 第二轮裁决回填

外部评审**方向通过、实施细节暂缓冻结**，裁决表：甲案支持（但不得表述为"AArch64 范式推广"——aarch64 靠 TTBR1 双基址寄存器天然免重放，x86/riscv 靠进程根构造重建内核映射，**三架构语义同一（内核经固定非恒等 VA 访问设备）而页表机制各异**，这正是"统一 OS 语义、不强行统一硬件机制"原则的形状）；乙/丙否决成立（丙的"临时映射"不做一概否定——未来冷门设备可走 fixmap 槽位，本步不为假想用途建设施）；RW+NX 有条件通过（须补内存属性维度）；选址待碰撞审计（查完整区间交集而非起点相等）；引导根低 4 GiB 恒等维持现状通过。另裁决四件进入方案正文：**P0×2**（设备窗必须同时覆盖引导根与进程根的访问场景；RW/NX 之外必须有设备内存属性论证）、**v6 交接契约去留**（禁止静默把恒等窗字段语义改成设备窗）、**访问点清单扩围**（从 trap 腿扩到全部 MMIO 访问点并逐点标执行阶段与活动根）、**语义抽象红线**（共享内核不得出现 PLIC/LAPIC 设备名、不得自行拼 `DEVICE_VA_BASE + PA`、不得为统一而造空洞 trait；职责分层=架构/平台层供范围与属性、页表构造层装映射、中断驱动用正确的内核 VA、共享层对设备无感知）。W^X 措辞收敛采纳：supervisor-only 恒等映射本就不让用户态直接访问设备，删除恒等映射的收益是"进程根不再保留设备物理别名、内核不再依赖 PA 兼作 VA、设备访问入口受控"，不夸大。

## 9. 实施前补证与诊断结果（按评审 §9 任务单 A–E，只调查不改码）

**总表**：A/B/C/D 全部关键事实【已证实】；E 的区间清单【已证实】、最终 `DEVICE_VA_BASE`【按裁决不指定】。**实施前阻塞项=0**（两轮评审批次后，无未决事实）。

复算命令（仓库根）：

```bash
# A1 x86 访问点族
grep -n "lapic_eoi\|lapic_write\|lapic_read\|ioapic_read\|ioapic_write" os/plat/src/x86_64/interrupt.rs | head -20
grep -n "lapic_eoi()" os/kernel/src/trap_dispatch.rs
# A1 x86 IPI（ICR 也是 LAPIC MMIO）
grep -n "fn send_sched_ipi" -A 14 os/arch/src/x86_64/smp.rs
# A1 x86 LAPIC 基址来自 MSR（非硬编码 0xFEE00000 单源）
grep -n "MSR_IA32_APIC_BASE" os/plat/src/x86_64/interrupt.rs | head -3
# A2 riscv plic 访问点族与 DTB 基址来源
grep -n "plic_read32\|plic_write32" os/plat/src/riscv64/interrupt.rs
grep -n "find_compatible" -A 3 os/libs/minix-platform/src/device_tree.rs | grep -A3 plic | head -5
# B IO-APIC 的运行期可达性（生产调用者=boot 期时钟注册）
grep -rn "register_hook(" os/kernel/src/clock.rs | head -2
# C 内存类型现状（PTE 旗标词表无内存类型位）
grep -cn "PCD\|PWT\|PAT\b" os/arch/src/x86_64/paging.rs
grep -cn "PBMT\|Svpbmt" os/arch/src/riscv64/paging.rs
# E riscv64/x86 高半地址地图
grep -n "KERNEL_DIRECT_MAP_BASE\|VM_DIRECT_MAP_BASE\|VM_HEAP_BASE" os/arch/src/arch/direct_map.rs | grep 0x
```

### 9.1 任务 A：MMIO 消费者完整清单

**x86-64（全部走 `lapic_read/write`、`ioapic_read_indirect/write_indirect` 两族原语，`os/plat/src/x86_64/interrupt.rs`）**：

| 访问点 | 位置 | 阶段/活动根 | 需进程根设备窗 |
|---|---|---|---|
| LAPIC EOI | `trap_dispatch.rs:382`（`minix_plat::lapic_eoi()`）| **运行期，进程根**（T2 后 trap 腿在用户进程的根上执行内核代码——第①步给进程根文本 RX 正是为此） | **是** |
| LAPIC IPI（ICR 双写） | `os/arch/src/x86_64/smp.rs:fn send_sched_ipi`（`lapic_write(LAPIC_ICR_HIGH/LOW)`）| 运行期调度下发——BKL 下、当前根可能是进程根（唤醒别的 CPU 时内核正跑在进程根上） | **是** |
| LAPIC 定时器（ICR/DCR/CCR/LVT） | `os/arch/src/x86_64/clock.rs:fn init_local_timer`（`write_volatile(lapic.add(...))`）| 每核本地定时器初始化——BSP 在 boot 根、**AP 在 smp_init 释放后进 ap tail 时**（AP 采用 BSP 根，见 AP bootstrap record 的 `page_table_root_pa`） | 经引导根覆盖（裁决 A 的方案 A 下同时成立） |
| LAPIC 初始化（SVR/LVT mask） | `interrupt.rs:fn init_lapic` | boot / AP bring-up——引导根 | 经引导根覆盖 |
| LAPIC 基址来源 | `wrmsr_msr_read(MSR_IA32_APIC_BASE)` 并**回写校正**（`init_lapic`：若 MSR 值 ≠ 常量则采用 MSR 值）| boot | 物理地址来源=MSR 实测，非假定 0xFEE00000【评审质疑点已闭合】 |
| IO-APIC 重定向表 | `fn ioapic_route_and_mask`（init_ioapic 全 pin 掩蔽）+ `unmask/mask` | **boot 期**（`init_ioapic`；运行期唯一生产调用者=时钟 `register_hook`→`unmask`，`os/kernel/src/clock.rs:419`——同为 boot 期定时器注册；其余调用者全是测试） | 经引导根覆盖；窗内保留 IO-APIC 映射（防未来 IRQ 注册运行期化） |

**riscv64（全部走 `plic_read32/plic_write32`，`os/plat/src/riscv64/interrupt.rs`；行号即 grep 实读）**：

| 访问点 | 行号 | 阶段/活动根 | 需进程根设备窗 |
|---|---|---|---|
| claim 读 | :232（`Self::claim_offset(context)`）| **运行期，进程根**（SEI/EI 腿，C17 重放 PLIC 窗的原始动机） | **是** |
| complete 写 | :246 | 同上 | **是** |
| priority 写 | :114/:171 | init（boot 根）+ `set_irq_priority`（内核调用面，理论上运行期） | 经引导根覆盖 + 窗内保留 |
| enable 写 | :117/:157 | init（boot 根）；运行期无生产调用者 | 经引导根覆盖 |
| threshold 写 | :119 | init（boot 根） | 经引导根覆盖 |
| PLIC 物理基址来源 | `os/libs/minix-platform/src/device_tree.rs:fn parse_plic`——**DTB `riscv,plic0`/`sifive,plic-1.0.0` 节点 reg 实读**，非 QEMU 硬编码（宿主测试 0x0C000000 只是 QEMU 样张断言） | boot | 【评审质疑点已闭合：基址走平台发现】 |

**aarch64**：恒等表恒空、控制器走系统寄存器——无消费者，不参与。

**第三类消费者检查**：全树 grep 无绕过两族原语的直接 0xFEE00000/0xFEC00000/PLIC 地址访问（SBI 调用与 aclint SSWI 走 `sswi_setip_base`——SSWI 是运行期 IPI 通道【riscv 需进程根设备窗覆盖 aclint-sswi 区，随 DTB 一并入窗】）。

### 9.2 任务 B：双根覆盖

【已证实】三处证据闭合裁决方案 A（统一设备 VA，双根同装）：① **引导根侧**——全部 boot 期访问（LAPIC/IO-APIC 初始化、PLIC priority/enable/threshold、时钟注册）都在引导根下执行，方案 A 要求设备窗装进引导根，这些访问原样可达；② **进程根侧**——真正的运行期消费者是 EOI/ICR/claim/complete 四点，全在进程根 trap 腿下执行；③ **AP 半程**——x86 AP bootstrap record 携带 `page_table_root_pa`（AP 采用 BSP 的根），riscv ap_early_entry 注释明言 "installs the BSP root"，AP 期访问（LAPIC 定时器初始化）也在引导根下。"进程根已映射 ⇒ 引导根可达"的反向推导未被使用——两根各自显式安装。

### 9.3 任务 C：MMIO 内存属性

【已证实——现状即"无内存类型编码"，别名兼容性因此平凡成立，但正确设备类型是**既有缺口**须独立登记】：① x86 的页表旗标词表（`arch/src/x86_64/paging.rs`）**无 PCD/PWT/PAT 位**——今天的恒等窗 MMIO 映射本来就是按默认 WB（普通缓存内存）属性映射的，在 QEMU virt 上可用是因为 QEMU 设备模型对访问类型宽松；② riscv 的 PTE 同样未编码 PBMT（bits 62:61 = 0，Svpbmt 未使用）——按规范这不等于"普通缓存主存"：PBMT=0 表示**沿用该物理区域的平台 PMA**（QEMU virt 的外设区属 I/O 型 PMA），设备访问顺序由 volatile 与平台 I/O 强序保证，TCG 下无额外 FENCE 需求；③ 因此**新设备窗沿用现状属性 = 与引导根恒等别名内存类型天然一致**（评审要求的别名兼容性：两别名同属性，UNPREDICTABLE 类问题不存在），第②步不引入新的属性回归；④ x86 侧同样要修正表述：PTE 无 PCD/PWT 位只说明**选择了 PAT entry 0**（默认 WB），有效内存类型 = 所选 PAT 项 □ MTRR 区间类型（SDM 组合表）；本仓代码从不写 IA32_PAT/MTRR（grep 证据），但 OVMF 固件在交棒前会编程 MTRR——APIC 区间的有效类型取决于固件 MTRR 覆盖（未覆盖则落默认 UC，恰为设备正确类型），**运行期实际读数（IA32_PAT/MTRR_DEF_TYPE）属尚无证据**，列入实施批的启动期验证探针。本步的承载性事实不受影响：新别名选择的 PTE 内存类型位与既有恒等别名**逐位相同**，有效类型无论如何都一致。真板属性正确化为独立登记债（riscv 侧 PMA 已足够则无需强引 Svpbmt）。

其余三项：supervisor-only【已证实可行】——旗标词表 `kernel_read_write()` 本就无 U 位；全局位【已证实】——`kernel_read_write()` 自带 G 位，进程切换语义与 DM 窗一致；RX/NX【已证实】——`kernel_executable()`/NX 组合按第①步同族旗标构造。

### 9.4 任务 D：v6 交接契约

【已证实】现状链：`build_identity_windows` 的产出经 `VmBootHandoff` v6 的 `kern_ident` 数组字段（`HandoffMemRegion` 区间，VA=PA 语义）交接给 VM，VM 侧 `KernelLayout::new_with_ident` 接收、`map_kernel` 第四段重放（`os/libs/minix-types/src/types/boot.rs`、`os/kernel/src/vm_handoff.rs`、`os/servers/vm/src/global.rs`）。**裁决落点（按评审给的两分支定性）**：属**情况 A**——设备物理范围可由架构/平台静态确定（riscv 来自 DTB 解析、x86 为平台常量+MSR 校正，均在内核与 VM 共同链接的 `minix-platform`/`minix-arch` 层可及），**不需要经跨镜像结构传递新信息**。因此采"保留 v6 结构形状、恒等窗字段弃用并要求为空（validate 拒绝非空）+ VM 从平台层自取设备描述"的分支——字段语义不变（恒等窗没了就是空表），设备窗是新增的架构语义，不经旧字段承载。契约纪律：v6 不升 v7、无静默语义改写；validate 的非空拒绝把"还有恒等窗候选"变成显式错误。

### 9.5 任务 E：高半虚拟地址碰撞审计

【已证实——区间清单齐备，最终基址按裁决不指定，实施批以断言落死】riscv64 高半在册区间：内核镜像 `[0xFFFFFFC000000000, +kern_size)`（2 MiB 收口）、内核 DM `[0xFFFFFFC040000000, +0x3F_BFFF_F000)`、VM DM `[0x00000010_00000000, +16 GiB)`、VM 堆 `[0x00000014_00000000, +64 MiB)`（后三者与高半不同段或用户半，需检的是跨段别名与 canonical 合法性）。x86 高半在册区间：内核镜像 `[KERNEL_HIGH_BASE+0x200000, +span)`、内核 DM `0xFFFF808000000000` 起、kerninfo 用户半 0x200000000、VM 堆用户半。**设备窗空档结论**：riscv64 的 Sv39 canonical 高半在内核 DM 之前有整段未用区（0xFFFFFFC000000000 之下直至 bit38 符号扩展边界），x86 的 `KERNEL_HIGH_BASE` 空间在镜像与 DM 之间有 8 PiB 级空档——两架构都存在容纳 2 MiB 级设备窗的空档，且都不与在册区间相交。实施批要求（进 check-layout 族）：**区间交集断言**（`start < other_end && end > other_start` 形式，非起点相等）、canonical 合法性、guard gap ≥ 大页粒度、设备窗内稀疏区间只建实际 PTE（x86 两设备区间 0xFEC0/0xFEE0 之间的 30 MiB 空洞保留 VA 不建叶）。

## 10. 修订后的承诺条款与实施阻塞清单

**承诺条款（按评审 §6 措辞收敛修订，取代第一版 §3.5）**：

> 普通进程根不包含任何 VA=PA 的恒等映射；内核运行期需要访问的 MMIO 区域经内核专用高半虚拟地址访问，设备映射为 supervisor-only、RW、NX，并具有与引导根既有别名兼容的内存属性（现状属性；设备内存类型正确化为独立登记的真板前置债）。引导根的既有低地址恒等映射不属于本步验收范围。

**范围与机制的双向澄清（评审 §1.1/§7 采纳）**：语义三架构同一——内核经固定、非恒等、supervisor-only 的内核 VA 访问设备，用户态不可达；页表机制三架构各异——x86/riscv 经进程根构造安装（引导根+进程根双根同装，裁决方案 A），aarch64 经 TTBR1 钉死天然成立。共享内核层不出现设备名与地址算术；设备范围与属性由架构/平台层提供，映射由页表构造层安装，消费由中断驱动使用。

**实施阻塞清单（评审要求的"只列影响设计正确性的事项"）**：**空**。两轮补证后，评审列的五件（双根覆盖、内存属性、消费者清单、v6 契约、区间碰撞）全部闭合为已证实事实或已记录的设计决定；真板设备内存类型为独立登记债，不阻塞本步（QEMU 全部门不受影响）。**甲案可冻结具体设计进入实施**，实施批开工命令与验收三面按 §4.4 与 §9 执行。

---

# 第三轮：运行时设备描述与内存属性补证（终轮）

## 11. 第三轮裁决回填

评审**方向维持支持**，但推翻上一轮"阻塞项=0"的结论——正确。核心质询：**简报把"两个组件链接同一个平台 crate"当成了"两个组件都能取得同一份运行时设备信息"，两者不等价**。终局裁决表：甲案/双根同 VA/乙丙否决/RW+NX+supervisor+Global/v6 字段留空拒非空/引导根维持现状——全部支持；**x86 LAPIC 实际基址能否被 VM 取得 = P0**；**riscv DTB 设备信息能否被 VM 取得 = P0**；x2APIC 支持边界必须明确；内存属性文案需修正（RW+NX 不等于设备内存属性；PBMT=0 是 PMA 不是 WB）；地址基址留给实施批但必须落成精确不变量（区间交集、canonical、溢出、guard gap、设备窗内稀疏空洞、PLIC+ACLINT 总虚拟跨度）。另两处采纳：§1.4 的"用户态推导"论证删除（本轮已改）；动态接口的定性原则="生产代码允许运行期调用则设备窗必须覆盖，否则用调用边界/类型约束体现，不以 grep 无调用者为准"（本轮已按此把 IO-APIC/PLIC 完整区域保留在窗内）。

## 12. 终轮补证结果

复算命令（仓库根）：

```bash
# A1 关键发现：恒等窗重放描述符是编译期常量、非运行时发现值
sed -n '283,287p' os/arch/src/lib.rs
# A1 VM 无 DTB：BootParams 字段无 DTB 指针 + DTB 页在 kmain A.2c 被裁出 memmap
grep -n "pub " os/servers/vm/src/boot.rs | sed -n '1,14p'
grep -n "dtb cut" os/kernel/src/lib.rs
# A2 x86 MSR 静默采用（非校验拒绝）与 x2APIC 位无检测
grep -n "apic_phys as usize != self.lapic_base" -A 2 os/plat/src/x86_64/interrupt.rs
grep -n "IA32_APIC_BASE_EN" os/plat/src/x86_64/interrupt.rs
# B 内存类型位现状
grep -n "fn flags_to_pte" -A 12 os/arch/src/riscv64/paging.rs | head -16
grep -n "fn flags_to_pte" -A 8 os/arch/src/x86_64/paging.rs
grep -rn "0x277\|IA32_PAT" --include="*.rs" os/arch os/plat os/kernel/src | wc -l
```

### 12.1 任务 A：运行时设备描述的数据来源（两 P0 的答案）

**A1 riscv【已证实——"VM 自取"不成立，但存在更好的既有通道】**：DTB 由引导方递入内核（a1 通道），内核 boot 期经 `minix-platform` 的 `DeviceTreeDesc::from_fdt` 解析出 `PlicDesc.plic_base` 与 `sswi_setip_base`，存入内核侧平台全局；**VM 拿不到 DTB**——`BootParams` 无 DTB 指针，且 DTB 页在 `kmain` A.2c 已被 `cut_memmap` 裁出 memmap（`os/kernel/src/lib.rs`），VM 无从解析。VM 也不调用 `parse_plic`。**但是**：审计发现恒等窗的重放描述符**从来不是运行时发现值**——`kernel_identity_mmio_regions()` 是 `const fn` 硬编码表（riscv：`(0x0C00_0000, 2MiB)` + `(0x1000_0000, 0x1000)`；x86：IO-APIC/LAPIC 两页），`map_kernel` 在内核侧与 VM 侧调用的是**同一个 minix_arch 常量函数**（VM 链接 minix-arch，`servers/vm/Cargo.toml:25`，`map_kernel` 调用即证）——即**双根的窗口描述符今天就是按构造一致的**（同一常量函数），运行时发现的值（DTB plic_base、sswi_setip_base）只被 plat 层控制器使用。这坐实了评审预言的漂移风险，且**审计抓到一个实锤漂移**：常量表第二项 `(0x10000000, 0x1000)` 盖住的是 ACLINT **MSWI** 页，而 C2ESSWI 引入运行期写路径的 **SSWI** 设备在 QEMU virt 位于 0x10001000——**不在常量表内**；`send_sched_ipi` 的 SSWI 写若发生在进程根上下文（调度下发完全可能在进程根 trap 腿里）将 page fault。这是一个先于本步存在的潜在缺陷，本步的统一恰好把它消灭。

**裁决落点（按评审给的可接受事实清单）**：采**"平台约束 + 校验即拒绝"**分支——设备窗描述符 = `minix_arch` 的常量函数（双根同源，构造一致）；boot 期校验 DTB 发现值与常量表一致（riscv：`plic_base`/`sswi_base` 必须落入常量表区间，SSWI 区间**补进常量表**——随实施批修复上述缺口；x86：见 A2），偏离即拒绝启动。于是 `DeviceMap_bootstrap == DeviceMap_process` 按构造成立，且"常量 == 运行时"由 boot 期校验保证。不需要新交接字段，不需要 v7。

**A2 x86【已证实——现状为"静默采用"，按裁决改为"校验即拒绝"；x2APIC 无检测】**：`init_lapic` 读 `IA32_APIC_BASE`(0x1B)，掩地址位后**若与常量不等则静默采用 MSR 值**（`interrupt.rs`：`if apic_phys != self.lapic_base { self.lapic_base = apic_phys }`）——这正是评审指出的"VM 用常量、内核用 MSR 值"漂移的温床。x2APIC 位（MSR bit 10）**无任何检测**（只处理了 EN bit 11）——若固件启用 x2APIC，MMIO 访问路径整体失效。**裁决落点**：实施批把两处静默改为**校验即拒绝**——MSR 基址 ≠ `DEFAULT_LAPIC_BASE` 拒绝启动、MSR bit 10 置位（x2APIC）拒绝启动（QEMU 默认 xAPIC，门不受影响；真板形态显式不支持即如实报错）。VM 侧因此恒用常量且与内核实测值恒等。

**结论（评审终局两问之一）**：**VM 能可靠取得与内核引导根相同的设备描述**——两者都来自 `minix_arch` 的同一常量函数（构造一致），"常量 == 运行时"由 boot 期校验即拒绝保证（实施批三件：riscv SSWI 区间补表 + riscv DTB 值校验 + x86 MSR/x2APIC 校验）。

### 12.2 任务 B：内存属性核实

**B1 x86【文案已修正；有效类型读数属尚无证据，列实施批验证项】**：`flags_to_pte` 只编码 PRESENT/WRITABLE/USER/GLOBAL/NX——PCD/PWT 恒 0，即**选择 PAT entry 0**（默认 WB），非"已证明 WB"；有效类型 = PAT 项 □ MTRR 区间类型。全仓无 IA32_PAT(0x277)/MTRR 写（grep=0，唯一命中是 aarch64 文档注释），但 OVMF 交棒前会编程 MTRR，故有效类型的**运行期实际读数尚无证据**——实施批在启动诊断行加 IA32_PAT/MTRR_DEF_TYPE 打印（配合既有 `text harden` 诊断族）。**承载事实**：新设备别名与既有恒等别名的 PTE 内存类型选择逐位相同 → 有效类型恒一致，本步零属性回归。

**B2 riscv【文案已修正】**：`flags_to_pte` 编码 V/A/D/R/W/X/U/G——**bits 62:61（PBMT）= 0**；按 Svpbmt 规范 PBMT=0 的语义是**沿用该物理区域的平台 PMA**（不是"普通 WB 主存"）。QEMU virt 的外设区（PLIC/ACLINT）按 virt 平台内存图属 I/O 型 PMA；访问经 `read_volatile/write_volatile`（防编译器重排），TCG 按程序序执行，无额外 FENCE 需求；真板的 FENCE 与宽度属性随真板债。平台 PMA 已正确时**无需为使用 PLIC 强行引入 Svpbmt**——维持不启用。

**C 双根一致性与执行模式**：VA→PA 对应一致【已证实：同一常量函数构造双根】；supervisor RW、NX、无 U 位【已证实：`kernel_read_write()` 词表无 U 位自带 G 位，NX 按第①步同族旗标】；PTE 内存属性选择一致【已证实：同一位型】；全部访问点走新高半 VA【实施批改造面=§9.1 清单的四个进程根消费者，其余经引导根既有恒等映射自然覆盖】；初始化期限定接口（IO-APIC route/unmask、PLIC priority/enable/threshold）按评审 §4.1 原则以**完整设备区域保留在窗内**处理，不靠 grep 结论缩窗。

**结论（评审终局两问之二）**：**属本次迁移的属性工作 = 别名 PTE 内存类型位逐位一致（已证）+ 新窗 supervisor RW/NX/G**；**明确延期的既有技术债 = x86 有效内存类型的运行期确认（IA32_PAT/MTRR 读数探针，实施批验证项）与真板设备内存类型正确化（x86 PAT/MTRR 编码、riscv 真板 PMA/FENCE 论证；riscv 现状 PMA 路径在真板上的充分性一并论证）**——与本步解耦，不写"设备内存属性已正确"。

### 12.3 更新后的实施前清单

| 事项 | 状态 |
|---|---|
| 设备描述双根同源（常量函数） | ✅ 已证实（含漂移实锤：SSWI 区间缺表，实施批补表） |
| "常量 == 运行时"（riscv DTB 值、x86 MSR 值） | ✅ 采"校验即拒绝"分支（实施批三件校验） |
| x2APIC 边界 | ✅ 检测 bit 10 拒绝启动（实施批） |
| 别名属性一致 / RW NX / supervisor / Global | ✅ 已证实 |
| v6 字段留空拒非空、引导根维持现状 | ✅ 维持 |
| DEVICE_VA_BASE + 精确不变量（区间交集/canonical/溢出/guard gap/稀疏空洞/PLIC+ACLINT 总跨度） | 实施批产出，断言进 check-layout 族 |
| x86 IA32_PAT/MTRR 运行期读数 | 实施批启动诊断探针（验证项） |
| 真板设备内存类型正确化 | 独立债（真板前置），不阻塞 QEMU 范围 |

**终局**：评审要求的两个 P0 均已闭合（其一以"校验即拒绝"设计决定 + 一枚实锤漂移（SSWI 缺表）的修复进入实施批的方式闭合）。按评审预期，**本轮结果已具备实施冻结裁决的全部输入**——实施批范围=甲案映射 + 三件校验 + SSWI 补表 + 恒等窗退场（`build_identity_windows` 删除、v6 字段拒非空、`map_kernel` 第四段删除）+ 地址布局表与断言 + 启动探针 + 验收三面（§4.4）。

---

# 第四轮：设备地图与访问覆盖终审（纠正第三轮两处事实错误）

## 13. 第三轮结论的两处推翻与一处降级（裁决回填）

评审对照 QEMU virt 源码推翻了第三轮 §12.1 的两处地址判断，且均经本机实测坐实（下节）：① ACLINT SSWI 不在 0x10001000（那是 **VirtIO MMIO** 基址），标准地址是 **0x02F00000 + 0x4000**；② 常量表 `(0x10000000, 0x1000)` 盖的是 **UART0 所在页**（0x10000000, 256 B 设备），不是 MSWI——第三轮"SSWI 缺表"的结论方向对、地址错。③ **PLIC 2 MiB 被反证**：历史 fault stval=0x0c201004 的偏移 0x201004 已落在 [0x0c000000, 0x0c200000) 之外——数学上矛盾，无需 DTB 即成立。④ 新消费者纳入：**UART0**。⑤ FENCE 论证降级（§9.3 的 volatile 论证只证编译器不合并，不证平台 I/O 序；改为"本次迁移不改变现有设备访问顺序，PMA ordering 与 FENCE 约束另行登记验证"）。⑥ x86 逐 CPU：IA32_APIC_BASE 是每逻辑处理器一份 MSR，校验必须在**每个 AP** 使用 LAPIC MMIO 前执行，不能凭 BSP 读数（列为实施验收条件）。⑦ PAT/MTRR 措辞再修正：默认类型是 IA32_MTRR_DEF_TYPE 的可配置字段，"未覆盖 ⇒ UC"不成立；有效类型推导需 IA32_MTRRCAP + 全部 variable MTRR base/mask + 区间匹配解析——仍属既有属性债，非本步阻塞。

## 14. 终轮取证与诊断结果（任务 A–D，只调查不改码）

复算命令（QEMU 8.2.2 / Debian 1:8.2.2+ds-0ubuntu1.18；门机器同版；无自定义 DTB、未启用 AIA）：

```bash
# A. 实际设备地图（默认与 aclint=on 两份 DTB 实测）
qemu-system-riscv64 -machine virt -bios none -machine dumpdtb=/tmp/pd22virt.dtb
fdtget /tmp/pd22virt.dtb /soc/plic@c000000 reg          # 0 201326592 0 6291456 = 0x0C000000+0x600000
fdtget /tmp/pd22virt.dtb /soc/plic@c000000 riscv,ndev   # 95
fdtget /tmp/pd22virt.dtb /soc/clint@2000000 reg         # 0x02000000+0x10000
fdtget /tmp/pd22virt.dtb /soc/serial@10000000 reg       # 0x10000000+0x100（UART0）
fdtget /tmp/pd22virt.dtb /soc/virtio_mmio@10001000 reg  # 0x10001000+0x1000（VirtIO，共 8 节点至 0x10008000）
qemu-system-riscv64 -machine virt,aclint=on -bios none -machine dumpdtb=/tmp/pd22aclint.dtb
fdtget /tmp/pd22aclint.dtb /soc/sswi@2f00000 reg        # 0x02F00000+0x4000（ACLINT SSWI）
fdtget /tmp/pd22aclint.dtb /soc/mswi@2000000 reg        # 0x02000000+0x4000
# B. PLIC 访问偏移上限
grep -n "PLIC_ENABLE\|PLIC_THRESHOLD\|PLIC_CLAIM\|PLIC_COMPLETE" os/plat/src/riscv64/interrupt.rs | grep const
# C. UART 消费者（VA==PA 直访）
grep -n "UART_BASE" os/kernel/src/uart_irq.rs | head -3
# 历史 fault 修复链
git log --oneline -S "0x0C00_0000" -- os/arch/src/lib.rs   # → c63b08058（C17 第二片）
git show c63b08058 -- os/arch/src/lib.rs | grep "2 MiB"
```

**总表**：设备地图【已证实——实际 DTB 实测】；PLIC 2 MiB【**反证**】；UART0 为常量表真实所指【已证实】；SSWI 运行期消费者未覆盖【已证实——第三轮定位错的真缺陷，正确位置=0x02F00000 缺表】；历史修复链【已证实——加的是不足窗，"修复后全绿"因无门触发进程根下 EI claim】。

### 14.1 任务 A：本机实际设备地图（QEMU 8.2.2 默认与 aclint=on 双 DTB）

| 设备 | physical start | DTB reg 长度 | 半开区间 | 节点 |
|---|---|---|---|---|
| CLINT/ACLINT MSWI | 0x02000000 | 0x10000（clint）；aclint=on 时 mswi 0x4000 | [0x02000000, 0x02010000) | `clint@2000000` / `mswi@2000000` |
| ACLINT SSWI | **0x02F00000** | 0x4000 | [0x02F00000, 0x02F04000) | `sswi@2f00000`（仅 aclint=on） |
| UART0 | **0x10000000** | 0x100 | [0x10000000, 0x10000100) | `serial@10000000` |
| VirtIO MMIO | 0x10001000 | 8×0x1000 | [0x10001000, 0x10009000) | `virtio_mmio@10001000..8000` |
| PLIC | 0x0C000000 | **0x600000** | [0x0C000000, 0x0C600000) | `plic@c000000`（`riscv,ndev=95`） |

与评审引用的 QEMU 源码（hw/riscv/virt.c）逐值一致；QEMU 8.2.2 默认无 aclint 节点、aclint=on 才生成 SSWI/MSWI/MTIMER 独立节点（SSWI 确在 0x02F00000，非 0x10001000 附近）。**第三轮 §12.1 的 SSWI 地址与 MSWI 判断作废**，本表为准。

### 14.2 任务 B：PLIC 映射覆盖（2 MiB 反证 + 历史 fault 链）

**反证成立**：`PLIC_CLAIM=0x200004`、`PLIC_COMPLETE=PLIC_CLAIM`、`PLIC_THRESHOLD=0x200000`（`os/plat/src/riscv64/interrupt.rs:22-28`），S 模式 context=1（`parse_plic` 硬编码）→ claim/complete 访问偏移 = 0x200004 + 1×0x1000 = **0x201004**，threshold = 0x201000——全部 ≥ 0x200000，**落在 [0x0C000000, 0x0C200000) 之外**。历史 fault 链闭合：当年 SEI 臂的 stval=0xc201004 正是 claim(context 1)，发生在**无窗时代**（进程根未映射 PLIC）；其修复批加的是 **2 MiB 窗**（该批对 `os/arch/src/lib.rs` 的 diff 原文"QEMU virt PLIC（2 MiB 窗）"，commit c63b08058 可查）——**不足窗**，claim/complete/threshold 仍未覆盖；此后各门全绿的原因是**没有任何门触发进程根下的 EI claim**：ATF/全系统跑中全部 PLIC 源 enables=0（C17 行原文"双掩默形态"），UART RX 首源自检（`uart_irq.rs` 的 `enable_uart_rx_selftest`）未在生产门武装，故越界地址从未被真实访问。**"修复后 36/36"验证的是窗存在，不是 claim 可达**——评审的"已反证"裁决与"不能跳过故障归因"要求全部成立。所需 PLIC 区域（按实际 DTB reg 与实际使用 context）：`-smp N` 下 S 模式 context = 2i+1，claim/complete 最大偏移 = 0x200004 + (2N−1)×0x1000（N=4 → 0x207004），priority 至 95×4=0x17C、enable 至 0x2000+context×0x80+⌈95/32⌉×4；**实施批取 DTB reg 全长 0x600000 为窗长**（同时覆盖全部 context 增长），不以 2 MiB 常量为据。

### 14.3 任务 C：恒等映射全部设备消费者（修正后清单）

| 设备/区间 | 窗内状态 | 消费者 | 阶段/活动根 | 高半 VA 迁移 |
|---|---|---|---|---|
| PLIC [0x0C000000,+0x600000) | 有窗但**长度不足**（claim/complete/threshold 越界） | claim/complete（trap EI 腿，进程根）、priority/enable/threshold（boot 根 + 内核调用面） | 两根 | **是**（含窗长修正） |
| UART0 [0x10000000,+0x100) | 有窗（常量表第二项的真实身份） | `kernel/src/uart_irq.rs`（`UART_BASE=0x10000000`，`uart_read/uart_write` volatile 直访；RX hook 走 SEI trap 腿 = 进程根；自检武装面=carrier，但为生产代码） | 两根 | **是** |
| ACLINT SSWI [0x02F00000,+0x4000) | **无窗**（第三轮误定位为 VirtIO/MSWI，实为缺表） | `send_sched_ipi` Path 1（`sswi_setip_base + 4×cpu` 直写，VA==PA；C2ESSWI 运行期条件撤钳的通道；调度下发可发生在进程根） | 进程根 | **是**（补窗 + 访问改高半 VA——只补映射不改访问代码等于没修） |
| CLINT/MSWI/MTIMER [0x02000000,…) | 无窗 | 无内核消费者（中断经 SBI 委托；定时器走 sbi_set_timer） | — | 不入窗（aclint-mtimer 解析字段留存，真板另议） |
| VirtIO [0x10001000,…) | 无窗 | 无内核消费者（U-Boot 装载阶段使用；内核 riscv 腿无 virtio 驱动） | — | 不入窗（QEMU 文档建议设备配置以 DTB 发现为准，后续驱动落地时再议） |
| x86 IO-APIC/LAPIC | 有窗（两页） | §9.1 清单 | 两根 | 是（甲案既定） |
| aarch64 | 空表 | 系统寄存器 | — | 无 |

**SSWI 访问代码迁移的明确承认（评审 P0）**：`send_sched_ipi` 的 `write_volatile((sswi_setip_base + 4*cpu))` 是 PA 兼 VA 直访——只把 SSWI 区间加进映射描述符而访问代码仍用物理地址，等于没修；实施批必须把该写入改为高半 VA（与 PLIC claim/complete、UART、LAPIC EOI 同一批地址迁移）。

### 14.4 任务 D：双根与描述符语义（实施条件确认）

① 唯一来源=常量描述符函数（改名迁义随实施批：从 `kernel_identity_mmio_regions` 拆为**物理设备描述**（平台层，DTB 校验源）与**虚拟映射策略**（arch 层，`DEVICE_VA_BASE` 变换）两层，旧名退役——评审 §4 的语义修正）；② 双根同区间同权限同属性位型——同一函数构造（§9.4 已证）+ 本轮窗长修正后仍成立；③ 全部长度页对齐、加法无溢出、PLIC 取 DTB reg 全长 0x600000；④ x86 校验点进 **AP 路径**（`ap_early_entry`/AP tail 在首次 LAPIC MMIO 前），非仅 BSP 一次（评审 §5.2 实施条件采纳）；⑤ DTB 校验扩为 reg 长度 + 全部使用偏移落界（claim/complete/threshold/enable/priority 的最大偏移逐一对账），非仅 base 相等。

**遗留阻塞项**：以 §14.3 完整消费者清单为准（PLIC/UART0/SSWI/x86 LAPIC 四处迁移 + 窗长修正 + SSWI 补窗）；x86 逐 AP 校验；地址布局表与断言；IA32_PAT/MTRR 读数探针。（本行取代早前「四处进程根消费者」的过时口径——第四轮清单为准。）设备地图事实链已全部对齐。

---

# 第十五节：实施冻结裁决与验收范围（终局）

**裁决（外部评审终局）**：**设计方向冻结，实施放行**。前两轮物理事实错误经本机 QEMU 8.2.2 DTB 读数、PLIC 寄存器布局与历史故障记录建立完整证据链后纠正；甲案总体设计、双根一致 VA→PA、RW/NX/Supervisor-only/Global、v6 恒等窗字段留空拒非空、引导根低地址恒等映射不纳入、物理设备描述与虚拟映射策略两层拆分——全部冻结。**两项实施硬门**：① PLIC per-hart context 对应必须证明（DTB `interrupts-extended` 的 M/S 交替序 ⇒ hart i 的 S context = 2i+1 在 QEMU virt 连续 hart 布局成立，但不得作为任意 DTB 的通用规则——驱动须从 DTB 解析或经校验的平台约定取得，且 claim/complete/threshold/enable 用同一 context 选择规则）；② SSWI 必须有明确可选设备语义（有 DTB 节点→校验 reg 并安装映射启用高半访问；无节点→禁用该 MMIO 通道、走既有 IPI 路径，不得假装设备存在；若预留高半 VA 区间须声明为虚拟地址预留而非设备存在宣称——`DeviceMap` 语义区分设备存在性与 VA 预留）。另两处设计确认：驱动**不得**运行期自拼 `DEVICE_VA_BASE + PA`（转换在地址布局构造阶段完成，驱动拿现成内核 VA）；设备 `reg` 逻辑长度与页表映射长度分开保留（UART0 reg 0x100 / 页映射 4 KiB；SSWI 0x4000；PLIC 0x600000）。

**实施顺序（裁决指定，不揉大提交）**：第一批=平台设备描述 + 页表映射 + 地址布局断言；随后分批迁移 PLIC、UART、SSWI、x86 LAPIC 消费者。

**验收范围（十项）**：PLIC 6 MiB 映射 + per-hart context 正确 + claim/complete 实际可用；UART0 进程根下走高半 VA 且 RX 测试实际触发；SSWI aclint=on 走高半 VA、无节点不访问；x86 LAPIC EOI/IPI 高半 VA；x86 每 AP 首次 MMIO 前 APIC base 与 x2APIC 校验；x86 IO-APIC 引导期与注册路径不回归；aarch64 不新增恒等窗、GIC 路径不回归；v6 字段非空拒绝；布局完整区间不相交/canonical/无溢出/guard gap；**强形态=从实际建立的进程根出发全范围扫描无任何 VA=PA 恒等映射**（不止三个已知地址）。

**边界与债（维持既往裁决）**：QEMU virt 平台范围验收，不宣称解决真板 MMIO 内存类型与 riscv I/O 顺序（两笔债独立登记）；设备发现以 DTB 为事实来源，地址不上升为 RISC-V 架构常量；工程教训（"窗存在 ≠ 寄存器可达——无触发者的全绿不是映射正确性的证明"）随本步记入技术债叙事。
