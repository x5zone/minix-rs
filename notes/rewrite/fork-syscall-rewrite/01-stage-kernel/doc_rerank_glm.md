# 01-stage-kernel 文档重建蓝图（glm）

## 0. 元数据

- 执行者：glm
- 日期：2026-09-19
- 目标目录：`notes/rewrite/fork-syscall-rewrite/01-stage-kernel/`
- 仓库根目录：`/home/xzhao/github/minix-rs`
- 当前提交号：`8e979d11925ddb9625b461760eb30e44f255ab42`（2026-09-19）
- 任务：R 相·重建蓝图。只产出本文件，不修改任何正文。

### 0.1 用户追加约束（本轮生效，影响新目录设计）

1. **单篇篇幅软上限**：极复杂概念允许"单篇单概念"长达 3000 行；一般情况必须控制长度以利阅读。本蓝图据此拆分超长旧篇（旧 01 共 2019 行、旧 05 共 2263 行均超软上限），并把新篇目标长度控制在 1500 行以内（除 07 一篇按既有裁决保持约 1400 行）。
2. **篇数不设限**：新目录可比旧目录增减篇数。本蓝图从旧"34 编号篇 + 99 概念篇 + 9 篇旁支"重建为 **39 篇**（编号 00–38），净增 5 篇：拆分 +4（旧 01 拆二、旧 14 拆三、旧 16 拆二）、合并 −1（旧 99 并入 07）、纯新建 +2（37-test-infra、38-rust-discipline）；旧 05 的运行期 IRQ 半并入 17、旧 22 的结构半并入 07（均为溢出半，各篇主位仍是 1:1 后继，不计净增）；旧 18-trap-bridge-design.md（旁支）被 13 吸收。
3. **真相源优先（认识论约束）**：本任务的目的就是修正已有文档的问题，旧文档可能含有错误或大量未经人工审阅的内容——重建的取料与核对的唯一权威是 **Minix3 C 源码 + Rust 实现代码**；旧文档降格为"取料位置线索 + 待验证草稿"。本蓝图的落实方式见 §0.5，旧文档错误抽样实证见 §3.6。

### 0.2 审查范围（含范围外说明）

**属于正文（重建对象）**：编号文档 `00` ~ `32`（34 篇，其中 `07-paging_init_gpt.md`、`18-trap-bridge-design.md` 为同号旁支，单独定性见下）与 `99-global-concepts.md`。

**属于参考材料（不重建，但作为知识来源与事实来源）**：`todo.md`（726 行，V13 压缩版，代码侧进度权威）、`smp_todo.md`（2002 行，SMP 步骤权威）、`checklist.md`、`endpoint_todo.md`、`06-todo.md`（旧 06 的重构需求书，本报告的写法范例，其"不拆 06"裁决对蓝图有约束力，见 §4 操作表 OP-07）、`panic-in-drop.md`（233 行，专题笔记，由新 38 吸收后归档）。

**定性为草稿（吸收结论后归档，不搬移正文）**：`07-paging_init_gpt.md`（358 行，GPT 对话记录，关于 pt_init 分层论证的草稿）、`smp_gpt.md`/`smp_gpt_v2.md`/`smp_gpt_v3.md`（合计约 2700 行，SMP 设计迭代草稿，已被 `smp_todo.md` 与旧 16 §9/§10 取代）。

**特殊定性**：`18-trap-bridge-design.md`（87 行）是 2026-09-16 已批准的正式设计裁决记录（edge_todo.md E1 条目引用其为设计文档），但其形态是设计裁决书而非教学文档。本蓝图将其**四项裁决与寄存器约定吸收为新 13-trap-entry 的正文知识点**，原文按"设计记录"归档（B 相执行时归档不删）。

**范围外（明确不处理）**：`.design/` 与 `tmp_design_and_todo/`（项目规范禁止引用）；其它 stage 目录文档；`minix3/` C 源码本身；02-stage-vm 的 `draft/` 交叉引用修复（登记到断链成本，B 相随引用迁移表处理）。

### 0.3 读取清单

**文档**：34 篇编号文档的头部声明（讲什么/不讲什么/前置/边界/ground truth）全部读取；`00-kernel-overview.md` 全文精读（导航表与心智模型来源）；`06-todo.md` 全文精读（写法范例与"不拆 06"裁决）；`todo.md` 全文精读；其余正文按章节骨架与知识点抽样精读。

**C 源码（全量清单 + 逐文件确认，非仅映射表）**：`minix3/minix/kernel/` 顶层 22 个 .c + 13 个 .h（main.c 522 行、proc.c 1980 行、system.c 997 行、clock.c 312 行、interrupt.c 177 行、smp.c 205 行、debug.c 563 行、utility.c 93 行、profile.c 157 行、watchdog.c 112 行、usermapped_data.c 15 行、table.c 66 行等）；`system/` 下 38 个 `do_*.c`（含 `do_abort.c`、`do_schedule.c`、`do_setgrant.c`、`do_mcontext.c` 四个旧文档无主讲述点的文件；**本树不存在 `do_unused.c`**——错误声明位于旧 00 导航表 `00-kernel-overview.md:284`，`checklist.md:5` 已自勘 41→38 但旧 00 未同步）；`arch/i386/` 全目录（head.S、mpx.S、kernel.lds、pre_init.c、pg_utils.c、protect.c、memory.c、exception.c、interrupt 控制器、arch_clock.c、arch_smp.c、arch_system.c、do_sdevio.c 等）；`arch/earm/`（对照用）。

**非 C 制品（逐项）**：
| 项 | 实际文件 |
|---|---|
| 链接脚本 | `minix3/minix/kernel/arch/i386/kernel.lds`；`os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld` |
| 汇编入口与陷阱入口 | `arch/i386/head.S`（:77 call pre_init、:87 call kmain）；`arch/i386/mpx.S`（hwint/s_call/syscall/exception/#NM 五类入口，:66 irq_handle、:293 do_ipc、:332 kernel_call、:371 exception_handler、:545 copr_not_available_handler）；Rust 侧 `os/arch/src/x86_64/trap_stub.rs` 等 naked_asm 入口 |
| 引导链与引导协议 | GRUB/multiboot（C 路径，`minix3/etc/boot.cfg.default`）；Rust boot-shim 三路径：UEFI/OVMF（`os/boot-shim/src/uefi_helpers.rs`）、OpenSBI（`opensbi_helpers.rs`）、multiboot 直启（smp_todo §25 终局）；`os/libs/minix-boot/src/kernel_info.rs` 的 KernelInfo 契约 |
| 镜像布局 | 高半基址与段布局（kernel.lds + 旧 02）；VmBootHandoff v4（`os/kernel/src/vm_handoff.rs`） |
| 用户态启动与运行时装载 | 内核侧 `load_vm_elf`（含 2026-09-17 共享边界页下溢修复，见 edge_todo 诞生链通电条目）；minix-rt crt0（属 14-stage，仅登记边界） |
| 跨模块接口与线格式 | SYS_VMCTL 子命令（do_vmctl.c）；MINIX_KERNINFO=6 与 kerninfo 页（`os/kernel/src/kerninfo.rs`）；trap 桥寄存器约定（18-trap-bridge-design.md）；minix-types 消息布局（引用，不展开） |
| 构建脚本与工具链 | `os/Cargo.toml` workspace、`.cargo/config.toml`（RUST_TEST_THREADS/RUST_MIN_STACK）、`tools/gen-test-kernel.sh`、三架构 production-target check、I-1（kernel ELF 构建无引导路径，DEFERRED） |
| 测试基建与模拟器脚本 | `os/qemu-tests/`（run_all.sh、run_qemu.sh、test-user-trap.sh、test-rt-birth.sh、test-riscv64-uboot.sh；`test-kernels/kernel/bootstrap/` 下 31 个测试内核）；`.github/workflows/qemu-tests.yml`；hosted 测试 742+ 项 |

**阶段边界材料**：`00-master-plan/README.md`（01-stage-kernel 为启动执行序第一站，**无前置 stage**，故"前一 stage 的 00-overview"一项为空，新目录第一篇的起点约束是"读者只需 OS 通识 + Rust 语法"）；`edge_todo.md`（1096 行，与本 stage 相关的开放条目：E-VMTLB、E-SCHEDNICED、E-PREEMPTFLAG、E-SCHEDSMP、E-MIBGRANT、E-ISKMESS、E-ISPROD、E-MINTYPES-SYS；已闭单：E1、E-KERNINFO 内核半、E-FORKMSG、E-BOOTFRAME）。

**Rust 实现入口（核对用，非顺序权威）**：`os/kernel/src/`（lib.rs kmain Phase A–F、syscall.rs、proc_table.rs、ipc.rs、clock.rs、smp.rs、sched.rs、globals.rs、kerninfo.rs、grant.rs、dm_coverage.rs 等 37 个模块）；`os/arch/src/`（三架构 + arch/ 抽象层）；`os/plat/src/`（中断控制器/早期控制台）；`os/boot-shim/src/`。

### 0.4 使用的命令与关键输出（证据摘录）

- `wc -l *.md`：34 篇编号文档 + 参考材料共约 37,007 行；最长旧 05 = 2263 行、旧 01 = 2019 行。
- `grep -oh "\b[0-9]\{2\}-[a-z0-9-]*\.md" [0-9]*.md 99*.md | sort | uniq -c`：目录内编号文档名引用共 **1093 处**（热点：16-smp.md 75、14-exception-interrupt.md 67、11-scheduling-primitives.md 60、25-misc-unported.md 59、06-proc-init-boot-proc.md 58）。
- `grep -roh "01-stage-kernel/...\.md" notes/rewrite/fork-syscall-rewrite/ --include="*.md"`：其它 stage 引入本目录共 **303 处**（热点：00-overview 35、06 33、18-syscall-copy 29、19-syscall-signal 19、11 19）。
- 代码注释引用（`os/kernel/src`、`os/arch/src`、`os/boot-shim/src`、`os/libs/minix-platform/src`）：约 **200 处**（热点：06-proc-init-boot-proc.md 58、05-clock-interrupt-init.md 17、16-smp.md 15、04-platform-discovery.md 15、02-higher-half-kernel.md 15）。
- C 函数地图：`grep -n "^[a-zA-Z_].*(" proc.c system.c clock.c interrupt.c smp.c` 与 `main.c` 全文精读（522 行）、`table.c` 全文（66 行）、`pg_utils.c`/`pre_init.c` 函数锚点、`mpx.S` 入口锚点。
- `ls os/qemu-tests/test-kernels/kernel/bootstrap/`：31 个测试内核（单架构 13 个 + 三架构族 6 组 × 3）。

### 0.5 真相源优先原则（用户约束 0.1-3 的落实）

旧文档可能含有错误或大量未经人工审阅的内容，因此本蓝图把认识论权重明确重排：

1. **权威链（沿用项目规范并操作化）**：Minix3 C 源码 >（Rust 代码作为实现事实）> 旧文档。旧文档在重建流程中只有两种合法角色：**取料位置线索**（B 相按图索骥找旧叙述）与**叙事素材**（重写时参考其教学组织），**永远不充当事实权威**。
2. **本蓝图自身的执行证据**：§1 的 C 真序完全不取自任何旧文档，逐条直接锚定 C 源码；§2 知识点池的 19 条新增全部以代码、制品或事实记录为锚；§5 每篇契约的"事实底线"一栏只指向 C 文件与制品路径，不指向旧文档——即 B 相写正文时的核对对象是代码，不是旧文档。
3. **B 相的操作纪律（写入每篇契约的执行前提）**：知识点清单中"来源"列填旧文档位置的，含义是"从该处取料"，**不是**"该处陈述可信"。B 相每写一条都必须按本篇"事实底线"对码重验；旧文档与代码冲突时，一律以代码为准，并把冲突登记为一处勘误（延续项目 fix-guard 与 P0-fact 纪律）。旧 08 的行号锚漂移（§3.6 第 4 条）说明连"行号级锚点"都必须以符号 + 实测行号双重复核。
4. **信任度抽样**：§3.6 给出六处已核实的旧文档错误/过时/失准实例，证明"旧文档需要逐条对码"不是理论谨慎而是既成事实；该表是抽样而非穷举，穷举防线就是第 3 条的对码纪律本身。

---

## 1. C 真序

### 1.1 阶段类型判定

本 stage 同时具备**启动链型**（Phase A–F 线性引导链）与**服务事件循环型的变体**（内核无主循环，但有一个"永不返回的汇聚点" switch_to_user，运行期一切事件经由"三类入口进入 → 处理 → switch_to_user 恢复"的环）与**集合型**（46 个内核调用按主题分组）。处理方式：**引导段按启动链型线性展开（Part A），运行期按"汇聚点 + 触发时机"组织（Part B），系统调用按并行体组织（Part C）**。理由：内核的运行时事实是"一次启动 + 一个永不返回的循环 + 三类进入事件"，任何单一形态的模板都会失真。

### 1.2 运行时序表（直接从 C 源码重建，非文档转述）

**A. 引导段（i386 路径，单核顺序执行）**

| 步骤 | 动作 | C 锚点 | 说明 |
|---|---|---|---|
| A01 | 固件/引导器加载内核镜像与 boot 模块，跳到内核入口 | `arch/i386/head.S:77`（call pre_init） | GRUB multiboot 路径；boot.cfg 模板 `minix3/etc/boot.cfg.default` |
| A02 | pre_init：解析启动参数与内存地图，建初始页表，开分页 | `arch/i386/pre_init.c`（UNPAGED 常量 :2）；`pg_utils.c:32` cut_memmap、`:65` alloc_lowest、`:86` add_memmap、`:162` pg_identity、`:186` pg_mapkernel、`:204` vm_enable_paging、`:247` pg_load | 内核此刻是"未分页的裸程序" |
| A03 | 跳入 kmain(kinfo) | `head.S:87`；`main.c:115-128` | 保存 kinfo/kmess 全局副本 |
| A04 | kmain 前置：kernel_may_alloc=1、拷贝 boot image 到 kinfo | `main.c:142-145` | boot 期内核允许使用主存 |
| A05 | cstart：prot_init（GDT/IDT/TSS）→ 读 boot 参数 → init_clock（纯软件）→ 填 kinfo 元数据（nr_procs/release/version/kuserinfo）→ 读 no_apic/no_smp/watchdog 开关 → intr_init(0) → arch_init | `main.c:403-475`；`protect.c` prot_init；`clock.c:48-66` init_clock；`arch/i386/i8259.c` intr_init；`arch/i386/arch_system.c` arch_init | 注意 init_clock 只做软件初始化，硬件使能在 A16 |
| A06 | 回 kmain：BKL_LOCK（大内核锁首次上锁） | `main.c:149`；`smp.c:27` SPINLOCK_DEFINE(big_kernel_lock) | 单核 boot 亦持 BKL |
| A07 | proc_init 清进程表 + IPCF_POOL_INIT | `main.c:157-158`；`proc.c:119-160` | 槽位标 SLOT_FREE、endpoint 基线 |
| A08 | 校验 NR_BOOT_MODULES == mi_mods_count | `main.c:160-162` | 不符即 panic |
| A09 | boot image 循环（×NR_BOOT_PROCS）：绑定 image[i]（table.c 静态表）→ endpoint/任务名/模块地址 → reset_proc_accounting → 可调度判定（kernel task/RS/VM 立即可调度，其余挂 RTS_NO_PRIV\|RTS_NO_QUANTUM）→ get_priv + 特权模板（TSK_F/VM_F/RSYS_F 等）→ fill_sendto_mask → s_k_call_mask → arch_boot_proc → proc_ptr 首赋 → 非 VM 挂 RTS_VMINHIBIT\|RTS_BOOTINHIBIT → 置 RTS_PROC_STOP、清 RTS_SLOT_FREE | `main.c:165-272`；`table.c:31-52` image[17]（asyncm/idle/clock/system/kernel/ds/rs/pm/sched/vfs/memory/tty/mib/vm/pfs/mfs/init） | 本 stage 的核心初始化节拍 |
| A10 | 回写 kinfo.boot_procs（供 VM 用） | `main.c:275` | |
| A11 | arch_post_init + IPCNAME 注册六个 IPC 调用名 | `main.c:283-290` | |
| A12 | memory_init | `main.c:293` | |
| A13 | system_init：填 call_vec[] 分发表 | `main.c:295`；`system.c:168` | Rust 用 enum Syscall + match 取代 |
| A14 | add_memmap 回收 bootstrap 内存 | `main.c:301`；`pg_utils.c:86` | bootstrap_start/bootstrap_len 归还自由表 |
| A15 | smp_init（CONFIG_SMP；no_apic/no_smp 时 smp_single_cpu_fallback 退路） | `main.c:303-317`；`smp.c` | AP bring-up 发生在此；失败则单核收尾 |
| A16 | bsp_finish_booting：cpu_identify → vm_running=0 → krandom 填充 → bill_ptr/proc_ptr=idle → announce 横幅 → 对 i < NR_BOOT_PROCS-NR_TASKS RTS_UNSET(RTS_PROC_STOP)（kernel task 槽永不解除）→ cycles_accounting_init → boot_cpu_init_timer(system_hz)（时钟硬件使能，最后刻）→ fpu_init → kernel_may_alloc=0 → switch_to_user | `main.c:38-109`（:45、:47-49、:56-58、:64-66、:71、:73-76、:78、:105、:107）；`clock.c:294` boot_cpu_init_timer | "初始化态 → 运行态"的翻转点 |
| A17 | switch_to_user：pick_proc → 恢复上下文 → iretq 进第一个用户进程（VM 先行） | `proc.c:299-477`（末尾 NOT_REACHABLE :473）；pick_proc `proc.c:1785`；idle `proc.c:176-193` | 内核此后永不再"顺序返回" |
| A18 | VM 用户态初始化，经 SYS_VMCTL 协商（SET_PDBR/映射声明/解除 VMINHIBIT） | `system/do_vmctl.c`；`arch/i386/arch_do_vmctl.c` | 其余 boot 进程依次被 RS/VM 解锁 |

**B. 运行段（事件驱动，五类入口全部汇于 switch_to_user）**

| 步骤 | 动作 | C 锚点（mpx.S 除注明外） | 说明 |
|---|---|---|---|
| B01 | 硬件中断：hwint 存帧 → irq_handle（钩子链）→ context_stop（或 context_stop_idle）→ switch_to_user | `mpx.S:66`（call irq_handle）、`:80-89` | `interrupt.c:116` irq_handle |
| B02 | IPC 陷入：s_call 存帧 → context_stop → do_ipc → switch_to_user | `mpx.S:290-300` | `proc.c:599` do_ipc |
| B03 | 内核调用：system call 门存帧 → context_stop → kernel_call（查 call_vec 分发）→ switch_to_user | `mpx.S:329-337` | `system.c:136` kernel_call → `:95` kernel_call_dispatch |
| B04 | 异常：存帧 → exception_handler（cause_sig/VMSUSPEND 转发）→ switch_to_user | `mpx.S:361-382` | `arch/i386/exception.c:155-170` |
| B05 | #NM（设备不可用）：copr_not_available_handler（FPU lazy 切换）→ switch_to_user | `mpx.S:544-547` | `proc.c:1922-1959` |
| B06 | 时钟 tick：timer_int_handler（记账 → quantum → 定时器 → 负载） | `clock.c:70-173`；账单方 `arch/i386/arch_clock.c`（billp 三用途） | B01 的特例，承载调度节拍 |
| B07 | 关停：prepare_shutdown（1 秒定时器）→ minix_shutdown（smp_shutdown_aps → hw_intr_disable_all → stop_local_timer → arch_shutdown） | `main.c:351-363`、`:368-398` | 关停链与本 stage 的收尾 |

### 1.3 真序核对声明

上表 25 条中，A 段 18 条按 `main.c` 全文（522 行逐行读过）与 head.S/pg_utils.c/table.c 锚点直接重建；B 段 7 条按 `mpx.S` 入口锚点与各 handler 函数地图重建。随机抽十条可核对性由 §9 自检门 G1 承担（已预抽十条，见 §9）。

---

## 2. 知识点全集

### 2.1 池的构建方法与规模

对 34 篇编号文档逐篇提取知识点，与 C 源码、非 C 制品、边界材料对账后去重合并；重复出现者合并为一条并记录全部现有位置（主讲述点加粗标注在 §5 契约的"来源"列）。覆盖审计（§3）新发现的知识点以来源类型=新增追加入池。按 §0.5 真相源优先原则：存量条目"来源"列的旧文档位置**仅是取料地址，不是可信性背书**，B 相取料后必须按该篇契约"事实底线"对码重验。**池总规模：285 条**（39 篇契约逐条实点之和，机械可复算），逐条明细分布在 §5 各契约中。**归属与顺序的读法**（回答"哪个知识点归哪篇、按什么顺序"）：

- **归属**：下表"K 编号段"列给出每篇承载的知识点编号段（39 段彼此无冲突、无缝覆盖全池）；每条知识点的名称、锚点、归属理由、来源在该篇契约的"讲什么"与"知识点清单"中逐条给出。
- **顺序**：两层——篇章级顺序 = 下表行序（即 §4.1 新目录顺序，全目录阅读序）；篇内顺序 = 该篇契约"讲什么"的列出顺序（K 编号升序），B 相按此序展开正文章节。

| 新篇（行序 = 阅读序） | K 编号段 | 条数 | 主题域 |
|---|---|---|---|
| 00-kernel-overview | K-000~003 | 4 | 内核心智模型与导航 |
| 01-boot-protocols-and-shim | K-010~017 | 8 | 固件交接与 boot-shim |
| 02-boot-memmap-and-paging | K-020~027 | 8 | 内存地图与开分页 |
| 03-link-load-jump | K-030~036 | 7 | 链接布局与高半跳转 |
| 04-kmain-cstart | K-040~046 | 7 | kmain 阶段模型与保护结构 |
| 05-platform-discovery | K-050~055 | 6 | 平台硬件发现 |
| 06-clock-intr-init | K-060~067 | 8 | 时钟与中断控制器初始化 |
| 07-proc-init-boot-proc | K-070~086 | 17 | 进程/特权数据模型与 boot 实例化 |
| 08-cross-space | K-090~095 | 6 | 跨空间能力与 Direct Map |
| 09-system-init-boot-finish | K-100~108 | 9 | 启动完成翻转 |
| 10-smp-boot | K-110~117 | 8 | AP bring-up 真链 |
| 11-switch-to-user | K-120~124 | 5 | 汇聚点与调度循环 |
| 12-vm-boot-protocol | K-125~129 | 5 | VM 启动协商 |
| 13-trap-entry | K-130~139 | 10 | trap 入口与 trap 桥双腿 |
| 14-scheduling | K-140~147 | 8 | 调度原语与状态机 |
| 15-ipc-core | K-150~159 | 10 | IPC 核心 |
| 16-exception-routing | K-160~165 | 6 | 异常路由 |
| 17-irq-delivery | K-166~171 | 6 | IRQ 投递 |
| 18-syscall-dispatch | K-172~179 | 8 | 系统调用分派 |
| 19-clock-timer | K-180~187 | 8 | 时钟运行时 |
| 20-smp-runtime | K-190~199 | 10 | SMP 运行时 |
| 21-syscall-process | K-200~206 | 7 | 进程管理调用 |
| 22-syscall-copy | K-207~215 | 9 | 跨空间拷贝调用 |
| 23-syscall-signal | K-216~224 | 9 | 信号调用 |
| 24-syscall-device | K-225~231 | 7 | 设备调用 |
| 25-syscall-clock | K-232~235 | 4 | 时钟调用 |
| 26-syscall-misc | K-236~247 | 12 | 杂项与缺口调用族 |
| 27-privilege-runtime | K-248~254 | 7 | 特权运行时 |
| 28-ipc-filter | K-255~260 | 6 | IPC 过滤 |
| 29-cross-space-runtime | K-261~266 | 6 | VMSUSPEND 协议 |
| 30-fpu | K-267~271 | 5 | FPU 上下文切换 |
| 31-stack-tracing | K-272~275 | 4 | 栈回溯 |
| 32-utility-console-shutdown | K-276~281 | 6 | panic/控制台/关停 |
| 33-kerninfo-user-data | K-282~288 | 7 | kerninfo 与用户可见内核数据 |
| 34-kernel-debug | K-289~293 | 5 | 内核调试 |
| 35-kernel-profile | K-294~298 | 5 | 内核 profiling |
| 36-watchdog | K-299~301 | 3 | watchdog（WONTFIX 存档） |
| 37-test-infra | K-302~309 | 8 | 测试基建 |
| 38-rust-discipline | K-310~320 | 11 | Rust 工程纪律 |

> 上表与 §5 契约逐篇实点核对一致（39 段无冲突、无缝覆盖）；条数列合计 = 285。

### 2.2 统计摘要

- 总条数 **285**（39 篇契约逐条实点之和；分篇条数见 §2.1 表，机械可复算）。
- 按来源类型：**纯新增 62 条**（K-066/067、K-112/113/116/117、K-129、K-134~139、K-158、K-165、K-169~171、K-184/187、K-196~199、K-215、K-223、K-242~247、K-254、K-260、K-266、K-279~281、K-285~288、K-293、K-298、K-302~320），全部带代码/制品/事实锚点；**存量 223 条**（来自现有 34+99 篇）；其中另有 **12 条存量混合条**（以存量为主、增补半条带新增锚点：K-036、K-055、K-081、K-093、K-123、K-143/144、K-177/178、K-200、K-211、K-231），逐条标注见 §5 契约。
- 类型分布：八类（概念/机制/数据结构/接口与协议/约束与不变量/架构演进/工具与工程/测试性质）逐条标注于各契约知识点清单；分布从契约机械可数，本节不抄第二份手抄统计，以免两处漂移（真相源原则对蓝图自身同样生效）。
- 重复与主讲述点标记（完整表见 §3.3）：RTS 位表（99/06/11 三处 → 主 07）、priv 结构（22 结构半/06 → 主 07）、BKL（00 §1.5/16 → 主 20，04 首现回指）、endpoint generation（99 → 主 07）、notify 机制（12/14 → 主 15）、VMSUSPEND（09/13/24 → 主 29）、cause_sig（14/19 → 主 16）。

---

## 3. 覆盖审计

### 3.1 主题全集与来源

四路来源：① C 源码符号（kernel 顶层 22 .c + 38 do_*.c + arch/i386 全目录，函数地图见 §0.3）；② 操作系统通用概念（进程状态机、地址空间、特权模型、中断分类、lazy FPU、采样 profiling 等，随对应条目入池）；③ 非 C 制品（§0.3 表 10 项）；④ 阶段边界契约（master-plan 启动因果链 + edge_todo 本 stage 开放条目 8 条）。

### 3.2 覆盖缺口表（每条已落实去向；"新增知识点"编号见 §5 对应契约）

| # | 缺口主题 | 证据 | 处置 |
|---|---|---|---|
| G-1 | 测试基建与模拟器脚本零覆盖（31 个 QEMU 测试内核、run_all/gen-test-kernel、CI、hosted 策略无任何文档） | `ls os/qemu-tests/test-kernels/kernel/bootstrap/`；`.github/workflows/qemu-tests.yml` | **新建 37-test-infra**（K-349~357） |
| G-2 | trap 桥双腿（int-33/SYSCALL 寄存器约定、用户上下文保存区、二次返回通道、STAR/sysret 锚点）散落在旁支设计文件与 edge 条目，无正式编号文档 | `18-trap-bridge-design.md`；edge_todo E1（2026-09-16 四切片闭环记录）；`os/arch/src/x86_64/trap_stub.rs` | **新建 13-trap-entry**，吸收设计文与新增知识点 K-145~154 |
| G-3 | kerninfo 页（2026-09-17 落地）无主讲述点，仅旧 28 §3.7 增补提及 | `os/kernel/src/kerninfo.rs`；edge_todo E-KERNINFO（commit a071af5e5） | **33-kerninfo-user-data** 重构旧 28，kerninfo 真实现为正文主体（K-328~335） |
| G-4 | SMP 真链过时：旧 00 导航表 `:270` 仍称 16 篇"当前为单核占位"，旧 16 主文写于单核时代（仅 §9/§10 增补了 S-3b/S-3c），而 S-3d/S-4/S-5/S-6/S-7/S-10/S-11/S-12 已于 2026-09-08~15 落地（INIT-SIPI 阶梯、multiboot 直启、AP 主循环、IPI 往返、shutdown、per-CPU 四件套） | `todo.md` §12.2（S-0~S-13 状态表）；`smp_todo.md` §21-26；test-smp-ap-alive-mb 四核在线 PASS | **拆分为 10-smp-boot + 20-smp-runtime**，旧 16 附录 A 历史包袱消解（K-110~117、K-190~199） |
| G-5 | 关停链（prepare_shutdown/minix_shutdown/smp_shutdown_aps）无主讲述点（旧 27 只讲 panic/kputc/_exit） | `main.c:351-398`；S-11 落地 | 归 **32-utility-console-shutdown**（K-320~327） |
| G-6 | 四个调用族无主讲述点：SYS_ABORT（do_abort.c）、SYS_SCHEDULE（do_schedule.c）、SYS_SETGRANT（do_setgrant.c）、SYS_GETMCONTEXT/SETMCONTEXT（do_mcontext.c）——仅 13/08/checklist 提及 | `ls minix3/minix/kernel/system/`；grep 显示仅散见引用 | 归 **26-syscall-misc**（K-271~284），逐族给契约位 |
| G-7 | Rust 工程纪律体系无文档：globals.rs 的 SyncUnsafeCell + BklProtected 审批、BklSection witness/assume_held、BklGuard::transfer、Errno newtype 单源、行为选择 cfg 禁令、panic 审计纪律 | `os/kernel/src/globals.rs`（24 项静态清单）；todo §0/§1 A1/A2/B1/D1/D2 | **新建 38-rust-discipline**（K-358~368），吸收 panic-in-drop.md |
| G-8 | 时钟硬件使能时序原则（D-59 方案 A：配置/注册/使能分离）与中断完成握手（D-61 claim→…→complete）为文档级空白（旧 05/14 写于修复前） | todo §23.2 D-59/D-61 修复记录；`os/kernel/src/irq_manager.rs` | 归 **06（K-056~058）与 17（K-188~196）** |
| G-9 | DM 窗口运行时模型（dm_coverage、bootstrap/运行期窗口覆盖、E8 下溢修复）旧 07 未覆盖 | `os/kernel/src/dm_coverage.rs`；edge_todo E8 四处根因记录 | 归 **08-cross-space**（K-090~095） |
| G-10 | 记账链（bill_ptr 三用途、p_cycles/per-state 五桶、I-15 单源裁决）分散且未成体系 | todo §23.3 I-6/I-15/I-16 闭环记录；`os/kernel/src/clock.rs` | 归 **19-clock-timer**（K-205~214） |
| G-11 | fork msgaddr 出参 wire 修复（E-FORKMSG）改变 syscall-process 的事实面 | edge_todo E-FORKMSG（2026-09-15 闭单） | 归 **21-syscall-process**（K-226~234） |
| G-12 | 内核 ELF 独立构建（I-1 DEFERRED）与 ACPI RSDP（I-5 DEFERRED）作为"已声明不做/暂缓"的工程事实无文档位 | todo §7.3/§7.4 | 归 **37-test-infra**（构建）与 **05-platform-discovery**（I-5 登记） |

### 3.3 重复主题表

| 主题 | 现有重复位置 | 新目录主讲述点 | 其余处置 |
|---|---|---|---|
| RTS 标志位完整表 | 99 §RTS、06 Ch2/Ch3、11 附录 A | **07** | 99 归档；11 附录保留为"阶段 C 履历"引用 07 |
| priv 结构字段 | 22 Ch1、06 §6.2 | **07**（结构半并入 06 整编） | 27 只讲运行时，开头引用 07 |
| endpoint/generation | 99 §1、06 | **07** | 99 归档 |
| BKL 概念 | 00 §1.5.1、16 §1.1 | **20**（实现与纪律）、**04** 仅一句首现+回指 | 00 §1.5 整体移交 38（见越界表） |
| notify 投递 | 12、14（IRQ 半复用） | **15** | 17 引用 15 |
| VMSUSPEND 协议 | 09、13、24 | **29** | 12/18 各自声明"不讲什么+去向 29" |
| cause_sig | 14、19 | **16**（异常→信号入口） | 23 讲调用侧，引用 16 |
| 时钟 | 05（init）、15（运行时）、21（调用） | 三分职责保留 | 边界在契约中互写 |
| 跨空间 | 07（init/DM）、18（拷贝调用）、24（运行时协议） | 三分职责保留为 08/22/29 | 同上 |
| kernel_call_resume | 13、24 | **29** | 18 声明去向 |
| vm_suspend | 24、09 | **29** | 12 引用 |

### 3.4 越界主题表

| 现有位置 | 越界内容 | 正确归属 |
|---|---|---|
| 00 §1.5（约 50 行） | "内核执行模型约束（Rust 开发必读）"——工程纪律（Rc/RefCell 限制、unsafe/SAFETY、trait 硬件约束）属工程规范而非架构概览 | **38-rust-discipline**；00 保留三行摘要+引用 |
| 06 §4.9 与附录 | FPU 细节展开 | **30-fpu**（主），06 留字段级一句+引用（06-todo 已知此债） |
| 16 附录 A | "SMP 预留设计全量正文（自 06 迁入）"——历史迁移包袱，且内容已被 S 系列实现超越 | 重建时消解：有效结论入 10/20，其余删（理由：已被实现文档取代） |
| 08 附录 A | smp_init 启动位次（S-5） | **10-smp-boot** |
| 旧 00 导航表 §3.8 row 25（:284） | 引用 `do_unused.c`——本树不存在该文件（`ls minix3/minix/kernel/system/` 实证，38 个 do_*.c 中无；`checklist.md:5` 已自勘 41→38 但旧 00 未同步） | 26 头部给出实际文件清单；该声明属事实错误（P0-fact 级），B 相必改 |
| 01 附录 A | UEFI 通用教程（非 Minix 语义） | 保留但压缩为"引导协议最小背景"，不超过 100 行（服务 01 主旨，不算越界，登记压缩要求） |

### 3.5 非 C 主题逐项回答（固定清单）

| 主题 | 在哪里讲 | 说明 |
|---|---|---|
| 链接与加载 | **03-link-load-jump** | kernel.lds（C 侧 + Rust 三架构 link.ld）、ELF 解析/段拷贝/BSS |
| 镜像与内存布局 | **02**（物理布局与 memmap）+ **03**（虚布局与高半） | 基址符号 _kern_vir_base/_kern_phys_base/_kern_size（pg_utils.c:14-16） |
| 汇编入口与陷阱进入 | **13-trap-entry**（运行期五入口）+ **03**（head.S 引导入口两句，细节引用 13） | mpx.S 五类入口锚点表在 13 |
| 启动装配 | **01-boot-protocols-and-shim** | GRUB/multiboot/UEFI/OpenSBI 三路径、KernelInfo 契约、模块装载 |
| 构建与工具链 | **37-test-infra** | workspace、gen-test-kernel.sh、production-target check、I-1 登记 |
| 跨模块接口与线格式 | KernelInfo→**01**；SYS_VMCTL→**12**；kerninfo/MINIX_KERNINFO→**15（原语）+33（页机制）**；VmBootHandoff→**09**；trap 桥寄存器约定→**13** | minix-types 消息布局细节归共享契约层文档（跨 stage），本 stage 只讲内核侧消费点 |
| 错误路径 | **38**（Errno 单源与纪律）+ 各调用篇的 errno 表 | 每篇契约含"错误路径"验收项 |
| 关闭与退出 | **32-utility-console-shutdown** | prepare_shutdown/minix_shutdown 全链（main.c:351-398） |
| 并发与同步 | **20-smp-runtime**（BKL/IPI 语义）+ **38**（Rust 治理机制） | 概念首现在 04（一句）与 10 |
| 测试基建 | **37-test-infra** | 31 个 QEMU 内核 + hosted 策略 + CI |

### 3.6 旧文档已知错误与失准抽样表（逐条核实，非穷举）

> 本表是用户约束 0.1-3 的实证支撑：旧文档确实含有事实错误、过时断言与未经审阅的残留。抽样方法：对头部声明、导航表与代表性断言做代码对账；**抽样≠穷举**——系统性防线是 §0.5 第 3 条的 B 相逐条对码纪律。

| # | 旧文档位置 | 断言 | 代码真相 | 定性 |
|---|---|---|---|---|
| E-1 | `00-kernel-overview.md:284`（§3.8 导航表 row 25） | 25 篇覆盖 `do_unused.c` | `ls minix3/minix/kernel/system/` 实证 38 个文件中无 do_unused.c；`checklist.md:5` 已自勘但旧 00 未同步 | 事实错误（P0-fact） |
| E-2 | `00-kernel-overview.md:270`（§3.7 导航表 row 16） | 16-smp "当前为单核占位，多核见文档" | S-4/S-5/S-7/S-10/S-11 已落地（todo §12.2）；`test-smp-ap-alive-mb` 四核在线 PASS（smp_todo §25 终局） | 过时断言（未随实现更新） |
| E-3 | `22-privilege.md:6`（头部前置声明） | "前置 16（进程管理——fork/privctl）" | 现行编号 16 是 smp；进程管理是 17——旧编号方案的残留，未随两次重编号更新 | 头部失准（断链残留） |
| E-4 | `08-system-init-boot-finish.md` §1.1 T0–T6 表 | 六个 lib.rs 行号锚（744/780/879/1271/1948/2757） | 实测（2026-09-19）：827/998/1090/1492/2051/2891——六处全部漂移 80–290 行 | 行号腐烂（系统性） |
| E-5 | `09-vm-boot-protocol.md` 头部 | "前置：08 + 10"——自我声明依赖排在后面的篇章 | 真序上 SYS_VMCTL 协商确实发生在 switch_to_user 之后，故旧目录顺序本身错位（本蓝图 OP-01 修复） | 顺序错误（自认的） |
| E-6 | `16-smp.md` 附录 A | "SMP 预留设计全量正文（自 06 迁入）"的历史设计正文 | 其内容已被 S 系列实现与 smp_todo §21–26 取代，保留即误导 | 历史包袱（未清理） |

**对 B 相的推论**：以上六处中 E-1/E-3 属"引用即污染"（导航表与前置声明是读者最先读到的位置）；E-4 说明**行号锚不可作为长期锚点**，新目录一律用"符号 + 实测行号"双锚并在验收时复核；E-2/E-6 属"文档滞后于代码"，正是本蓝图新增篇目（10/20/33/37/38）存在的原因。

---

## 4. 新目录

### 4.1 新篇章总表（39 篇，编号 00–38）

**第一部分 引导与立足（主线，严格按运行时序）**

| 新编号 | 标题 | 一句话定位 | 来源 |
|---|---|---|---|
| 00 | kernel-overview | 内核是什么、怎么启动、怎么运行——入口与导航 | 旧 00 重写 |
| 01 | boot-protocols-and-shim | 固件把机器交到内核手里：三条引导路径与 KernelInfo 契约 | 旧 01 前半 |
| 02 | boot-memmap-and-paging | 内核在开分页前如何认识内存并建立初始页表 | 旧 01 后半 + 旧 08 的 add_memmap 知识点 |
| 03 | link-load-jump | 链接布局、ELF 装载与高半跳转——从镜像文件到高地址的 kmain | 旧 02 整编 |
| 04 | kmain-cstart | kmain 阶段模型与保护结构（GDT/IDT/TSS） | 旧 03 整编 |
| 05 | platform-discovery | 内核如何在不硬编码地址的前提下认识硬件 | 旧 04 整编 |
| 06 | clock-intr-init | 时钟与中断控制器的启动期初始化（C 真序 + 三架构抽象） | 旧 05 前半（拆分） |
| 07 | proc-init-boot-proc | 进程表/特权表数据模型与 boot image 实例化（存储与流程一体） | 旧 06 整编 + 旧 99 吸收 + 旧 22 结构半并入 |
| 08 | cross-space | 内核获得跨地址空间访问能力：Direct Map 模型 | 旧 07 整编 + dm_coverage 新增 |
| 09 | system-init-boot-finish | 从初始化态到运行态：分发表、内存回收、bsp_finish_booting | 旧 08 整编 |
| 10 | smp-boot | AP bring-up 真链：INIT-SIPI、实模式阶梯、per-CPU 建立 | 旧 16 启动半重写 + 新增 S-3d~S-6 事实 |
| 11 | switch-to-user | 汇聚点：首次切换与永不返回的调度循环 | 旧 10 整编 |
| 12 | vm-boot-protocol | VM 与内核的启动协商协议（SYS_VMCTL） | 旧 09 整编（后移，修复前向引用） |

**第二部分 运行期脊柱（循环中反复发生的事）**

| 新编号 | 标题 | 一句话定位 | 来源 |
|---|---|---|---|
| 13 | trap-entry | CPU 如何进出内核：五类入口、两腿 trap 桥、上下文保存与恢复 | 旧 14 入口机制半 + 18-trap-bridge-design 吸收 + 新增 |
| 14 | scheduling | 就绪队列与 RTS 状态机的一致性：调度原语 | 旧 11 整编 |
| 15 | ipc-core | 无共享内存的消息中转、阻塞唤醒、死锁检测与通知 | 旧 12 整编 |
| 16 | exception-routing | 异常的分类处置：信号入口、页错误转发、FPU 分支 | 旧 14 异常处理半（拆分） |
| 17 | irq-delivery | 硬件中断如何变成通知：钩子链与三架构控制器投递 | 旧 14 IRQ 半 + 旧 05 运行时 IRQ 部分（拆分归并） |
| 18 | syscall-dispatch | 内核调用分派：权限门、结果语义、C 表驱动 vs Rust 枚举 match | 旧 13 整编（后移，消除前向引用） |
| 19 | clock-timer | tick 里发生的事：三钟、定时器队列、记账与负载 | 旧 15 整编 |
| 20 | smp-runtime | BKL、IPI 跨 CPU 调度、迁移与关停的运行时语义 | 旧 16 运行时半重写 + S-7~S-12 事实 |

**第三部分 系统调用集合（统一框架在 18，按族分篇）**

| 新编号 | 标题 | 一句话定位 | 来源 |
|---|---|---|---|
| 21 | syscall-process | 进程生命周期七调用：fork/exec/exit/clear/runctl/schedctl/statectl | 旧 17 |
| 22 | syscall-copy | 信任分层的跨空间拷贝：vircopy/safecopy/umap/memset | 旧 18 |
| 23 | syscall-signal | 信号调用：内核三步闭环与 POSIX 双路径 | 旧 19 |
| 24 | syscall-device | 设备 I/O：IRQ 钩子注册与端口访问的权限模型 | 旧 20 |
| 25 | syscall-clock | 时间服务调用：times/setalarm/stime/settime/vtimer | 旧 21 |
| 26 | syscall-misc | 杂项与缺口族：getinfo/trace/update/sprofile + abort/schedule/setgrant/mcontext 落位 | 旧 25 重构 + 四族新落位 |

**第四部分 控制面（谁被允许做什么）**

| 新编号 | 标题 | 一句话定位 | 来源 |
|---|---|---|---|
| 27 | privilege-runtime | 特权的运行时：SYS_PRIVCTL、irq/io/mem 授权、sendto/kcall 掩码维护 | 旧 22 运行时半（结构半已并入 07） |
| 28 | ipc-filter | IPC 过滤：三层决策模型与过滤器池 | 旧 23 整编 |
| 29 | cross-space-runtime | 内核代行与缺页悖论：VMSUSPEND 挂起-恢复协议 | 旧 24 整编 |

**第五部分 内核基础设施与自省（按需阅读）**

| 新编号 | 标题 | 一句话定位 | 来源 |
|---|---|---|---|
| 30 | fpu | FPU 上下文切换：lazy 模型与 #NM 陷阱路径 | 旧 31 |
| 31 | stack-tracing | 栈回溯：frame-pointer 链遍历与跨地址空间读取 | 旧 32 |
| 32 | utility-console-shutdown | 内核如何报错、说话与谢幕：panic/控制台/关停链 | 旧 27 + 关停链新增 |
| 33 | kerninfo-user-data | 用户可见内核数据：C .usermapped（存档）与 Rust kerninfo 页（现役） | 旧 28 重构 + kerninfo 新增 |
| 34 | kernel-debug | 内核自我诊断：队列一致性断言与状态打印 | 旧 29 |
| 35 | kernel-profile | 内核统计 profiling：采样时钟与数据面 | 旧 30 |
| 36 | watchdog | NMI watchdog（WONTFIX 存档，为什么不做） | 旧 26 |

**第六部分 工程与测试（支线，可整段跳读）**

| 新编号 | 标题 | 一句话定位 | 来源 |
|---|---|---|---|
| 37 | test-infra | 这个内核怎么被验证：31 个 QEMU 测试内核、hosted 策略与 CI | **全新**（缺口 G-1） |
| 38 | rust-discipline | 用 Rust 写内核的纪律：全局状态治理、witness、错误单源、cfg 禁令 | **全新**（缺口 G-7），吸收 panic-in-drop.md |

### 4.2 阅读路径

- **主线（启动链）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12。读完即理解"内核怎么活起来"。
- **主线（运行期脊柱）**：13 → 14 → 15 → 16 → 17 → 18 → 19 → 20。读完即理解"活起来之后每个事件怎么流转"。
- **集合体路径（并行体）**：18 是系统调用的统一框架篇；21–26 按场景分族，族内代表成员精讲 + 差异表；可从任意族切入，仅需 18 + 该族声明的约束前置。
- **支线（可跳读）**：27–29（控制面，写驱动/服务授权时读）；30–36（基础设施与自省，按需单点查阅）；36 为 WONTFIX 存档，读它是为了知道"为什么不做"。
- **工程支线（可跳读）**：37–38（改代码/加测试前读）。

### 4.3 并行体的组织规则落实

- 统一框架篇：18（分派机制、权限门、结果与恢复语义）。
- 分族：21–26 按进程/拷贝/信号/设备/时钟/杂项六族。
- 代表成员：每族选一个调用精讲（21 选 fork、22 选 SAFECOPYFROM、23 选 SIGSEND、24 选 IRQCTL、25 选 SETALARM、26 选 GETINFO），其余按差异表收束。
- 主线/支线/可跳读声明见 4.2。

### 4.4 篇幅软上限分配（用户约束 0.1-1 的落实）

| 预期长度档 | 新篇 |
|---|---|
| ≤800 行 | 00、12、16、17、27、28、29、31、34、35、36 |
| 800–1200 行 | 02、05、06、08、09、10、13、14、15、18、19、20、25、26、32、33 |
| 1200–1500 行 | 01、03、04、07（既有裁决允许约 1400）、11、21、22、23、24、30、37、38 |
| 允许超档（极复杂单概念，上限 3000） | 无——旧 01/05/16 的超长问题已通过拆分解消 |

---

## 5. 每篇契约

> 格式说明：每篇七要素齐全（定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单+验收）。知识点清单给出编号、名称、类型、锚点、归属理由、来源（存量=旧文档位置；新增=C 锚点/制品路径/事实出处）。旧文档编号指现行目录（01-boot-shim-bootstrap.md 等记作"旧 01"）。
>
> **执行前提（真相源优先，见 §0.5）**：每篇契约的"事实底线"是该篇正文的**唯一核对权威**；知识点清单"来源"列的旧文档位置只用于取料，旧文档陈述与代码冲突时一律以代码为准并把冲突登记为勘误（抽样实证见 §3.6）。B 相每完成一篇，验收标准中的"对码核对"以事实底线锚点逐条执行，不以旧文档复核旧文档。

> **顺序约定**：契约"讲什么"的列出顺序（K 编号升序）即本篇正文的**建议章节顺序**，B 相按此序展开。紧凑式契约（以行内列举逐条给出知识点）与全表式契约（如 00/01/02 篇）具有同等规范效力——编号、名称、类型、锚点、归属理由、来源六列信息在两种形式中均逐条可读。

### 00-kernel-overview

- 一句话定位：回答"内核是什么、与用户态服务有何本质不同、本文档族怎么读"，是全目录的地图与心智模型发放处。
- 讲什么：K-000 内核心智模型（导演类比、与 VM/PM/VFS 的分工、页表管理的跨主体协议）；K-001 "内核无主循环、三类进入路径"的执行模型；K-002 内核 task 的三维度正交模型（IPC identity / code+state / execution context，含 HARDWARE/ASYNCM 纯身份实体）；K-003 新目录导航表与阅读路径。
- 不讲什么：任何机制的展开（各自归 Part A/B 篇）；Rust 工程纪律（→38，保留三行摘要）；调度/IPC/异常细节（→14/15/16）。
- 前置：无（读者只需 OS 通识与 Rust 语法）。
- 后置：全部 39 篇会引用本篇的心智模型；导航表是全目录的目录。
- 事实底线：`minix3/minix/kernel/` 目录结构；`main.c:115` kmain、`proc.c:299` switch_to_user、`mpx.S` 五入口；`table.c:31-52` boot_image 17 项；`com.h:47-56` NR_TASKS=5 负端点。
- 知识点清单：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-000 | 内核心智模型与 VM/内核分工协议 | 概念 | 旧 00 §1.1-1.3 | 全目录的入场模型 | 存量：旧 00 §1 |
  | K-001 | 无主循环执行模型（三类进入路径） | 概念 | 旧 00 §4.1；mpx.S 五入口 | 同上 | 存量：旧 00 §4.1 |
  | K-002 | kernel task 三维度正交（含 IDLE 反例、HARDWARE/ASYNCM 纯身份） | 概念 | 旧 00 §1.4.1；main.c:64-66；proc.h:216-222 | 防止把 task 读成线程的免疫针 | 存量：旧 00 §1.4.1 |
  | K-003 | 新目录导航表（39 篇）与阅读路径 | 工具与工程 | 本蓝图 §4 | 地图职责 | 新增（本蓝图产物） |
- 验收标准：读者能在不看正文的情况下回答"内核为什么没有 main 循环""CLOCK task 会不会被调度""调度/IPC/异常分别在汇聚点的哪个位置"；导航表每行链接有效且与实际文件名一致。

### 01-boot-protocols-and-shim

- 一句话定位：讲清"内核运行之前发生了什么"——固件、引导器、boot-shim 如何协作把一台裸机变成"内核可以开始执行"的状态。
- 讲什么：K-010 x86/ARM/RISC-V 上电到引导器交接的通用链；K-011 Minix3 C 的 GRUB/multiboot 路径与协议头；K-012 boot.cfg 模板；K-013 Rust boot-shim 定位与三引导路径（UEFI/OVMF、OpenSBI、multiboot 直启——后者为 SMP 测试新增路径）；K-014 KernelInfo 契约（字段、validate()、作为 shim↔kernel 唯一接口）；K-015 boot 模块装载与 boot_procs 绑定（main.c:179-184 的消费侧预告）；K-016 ExitBootServices/SBI 关闭固件服务的时机与约束；K-017 引导期诊断与失败模式。
- 不讲什么：内存地图的解析与页表建立（→02）；内核 ELF 的解析装载（→03）；UEFI 通用教程压缩为附录最小背景（≤100 行）。
- 前置：00。
- 后置：02（内存地图来源）、03（模块已由 shim 装载）、05（平台参数来源）、37（引导路径 × 测试矩阵）。
- 事实底线：`minix3/etc/boot.cfg.default`；`minix3/minix/kernel/arch/i386/pre_init.c`；`os/boot-shim/src/{lib,main,loader,uefi_helpers,opensbi_helpers}.rs`；`os/libs/minix-boot/src/kernel_info.rs`。
- 知识点清单：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-010 | 上电→固件→引导器→内核的通用交接链 | 概念 | 旧 01 §1.1-1.3 | 本篇主旨 | 存量：旧 01 §1 |
  | K-011 | Multiboot 协议头与 multiboot_info | 接口与协议 | 旧 01 §2.0.1；pre_init.c | C 路径事实 | 存量：旧 01 §2 |
  | K-012 | boot.cfg 配置模板 | 工具与工程 | minix3/etc/boot.cfg.default | 同上 | 存量：旧 01 §2.0.3 |
  | K-013 | boot-shim 三引导路径（UEFI/OpenSBI/multiboot 直启） | 机制 | 旧 01 §3；smp_todo.md §25 终局 | 直启路径为新增事实 | 存量+新增 |
  | K-014 | KernelInfo 契约与 validate() | 接口与协议 | os/libs/minix-boot/src/kernel_info.rs | shim↔kernel 唯一契约 | 存量：旧 01 §2/§4 |
  | K-015 | boot 模块装载与 boot_procs 绑定 | 机制 | main.c:179-184 | 模块到进程的桥 | 存量：旧 01/旧 06 |
  | K-016 | 退出固件服务的时机（ExitBootServices/SBI 关闭） | 约束与不变量 | os/boot-shim/src/uefi_helpers.rs；opensbi_helpers.rs | 引导合法性的边界 | 存量：旧 01 §4 |
  | K-017 | 引导期诊断与失败模式 | 测试性质 | 旧 01 §6 | 排障入口 | 存量：旧 01 §6 |
- 验收标准：读者能画出三条引导路径各自的"谁加载了什么、交出了什么参数、何时不能再调固件"；能指出 KernelInfo 中与后续各篇相关的字段（memmap→02、模块表→07、user_sp→33）。

### 02-boot-memmap-and-paging

- 一句话定位：内核在开分页前后如何认识物理内存、切掉不该用的区间、建立恒等与高地址映射，并约定 bootstrap 内存的归还契约。
- 讲什么：K-020 kinfo.memmap 物理内存地图表示；K-021 cut_memmap/add_memmap/alloc_lowest 三原语（含内核镜像自切 D-64 语义）；K-022 bootstrap 分配器与 bootstrap_start/len 回收契约；K-023 32 位两级页表与 4MB 大页；K-024 pg_identity 恒等映射；K-025 pg_mapkernel 高地址映射；K-026 vm_enable_paging/pg_load（CR0.PG/CR3）；K-027 C pre_init 与 Rust Phase A 的对应（含 UEFI MaxAddress 降序分配、OpenSBI 全域 CONVENTIONAL 两类固件行为差异及自切补偿）。
- 不讲什么：链接脚本与 ELF 装载（→03）；Direct Map 运行时窗口模型（→08）；VM 页表管理（→02-stage-vm）。
- 前置：01。
- 后置：03（页表已就位才能跳高半）、08（Direct Map 的根）、09（add_memmap 回收时机）。
- 事实底线：`pg_utils.c:32/:65/:86/:162/:186/:204/:247`；`pre_init.c`；`os/kernel/src/lib.rs` Phase A.2（cut 模块与内核镜像）；`os/kernel/src/memmap.rs`。
- 知识点清单：
  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |---|---|---|---|---|---|
  | K-020 | kinfo.memmap 地图表示 | 数据结构 | pre_init.c；旧 01 §2.1 | 本篇主对象 | 存量：旧 01 |
  | K-021 | cut_memmap/add_memmap/alloc_lowest | 机制 | pg_utils.c:32/:86/:65；todo §23.3 D-64 | 地图操作全集 | 存量+新增（内核镜像自切） |
  | K-022 | bootstrap 分配与回收契约 | 约束与不变量 | main.c:301；旧 01 | 贯穿 boot 的内存纪律 | 存量：旧 01/旧 08 |
  | K-023 | 两级页表与 4MB 大页 | 数据结构 | pg_utils.c:19 pagedir[1024] | 机制基础 | 存量：旧 01 §2.3 |
  | K-024 | pg_identity 恒等映射 | 机制 | pg_utils.c:162 | 开分页前奏 | 存量：旧 01 |
  | K-025 | pg_mapkernel 高地址映射 | 机制 | pg_utils.c:186 | 高半前置 | 存量：旧 01 |
  | K-026 | vm_enable_paging/pg_load | 机制 | pg_utils.c:204/:247 | 开分页动作本体 | 存量：旧 01 |
  | K-027 | C/Rust Phase A 对应与固件行为差异（UEFI 降序、OpenSBI 全域 CONVENTIONAL、自切补偿） | 架构演进 | os/boot-shim/src/uefi_helpers.rs:245-255；opensbi_helpers.rs:396-403；todo D-64 | 重写语义对照 | 存量+新增（D-64） |
- 验收标准：给定一张 multiboot 内存地图，读者能手算 cut/add 之后可用区间；能解释"为什么 add_memmap 要等到 09 篇的时机才调用而 cut 必须在开分页前完成"。

### 03-link-load-jump

- 一句话定位：内核镜像如何被链接成"高地址程序"、如何被解析装载进内存、CPU 如何完成切栈跳转进入高地址的 kmain。
- 讲什么：K-030 链接脚本布局（C kernel.lds 与 Rust 三架构 link.ld、_kern_vir_base/_kern_phys_base/_kern_size 符号）；K-031 高半内核的理由（地址空间布局、0xFFFF_8000_0000_0000）；K-032 head.S 引导入口（两行 call 的位置感，细节引用 13）；K-033 ELF 解析/段拷贝/BSS 清零；K-034 HigherHalf::jump_to_kmain 切栈跳转（有去无回 `-> !`）；K-035 三架构 identity/kernel mapping 的 L2 覆盖差异（旧 02 附录 A 收编为正文差异表）；K-036 Rust Phase A 的 DM 双窗口概念（详解引用 08）。
- 不讲什么：页表项本身的建立（→02）；trap 入口汇编（→13）；用户态 ELF（→02-stage-vm/14-stage）。
- 前置：01、02。
- 后置：04（kmain 已在高地址运行）、08（DM 窗口）、13（跳转后 CPU 状态）。
- 事实底线：`arch/i386/kernel.lds`；`head.S:77/:87`；`os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld`；`os/kernel/src/boot/higher_half.rs`；`os/boot-shim/src/loader.rs`。
- 知识点清单：K-030（数据结构，kernel.lds，链接布局是本篇地基，存量旧 02）、K-031（概念，旧 02 §1.2）、K-032（机制，head.S:77/:87）、K-033（机制，loader.rs，存量旧 02 §4）、K-034（机制，higher_half.rs，存量旧 02）、K-035（约束与不变量，旧 02 附录 A）、K-036（架构演进，旧 07 §1.3.2 预告 + lib.rs，主讲述点在 08，此处只留概念位）。验收：读者能对照链接脚本指出"哪个符号决定物理加载地址、哪个决定链接地址"，并解释只跳不切/只切不跳各自的崩溃形态。

### 04-kmain-cstart

- 一句话定位：kmain 的阶段模型（Phase A–F）与"保护结构"——GDT/IDT/TSS 如何让 CPU 硬件为内核建立可信运行上下文。
- 讲什么：K-040 保护结构概念（信任边界、特权环、GDT/IDT/TSS 分工）；K-041 Rust kmain Phase A–F 模型与 C kmain 线性序对照（lib.rs:394-400 注释为锚）；K-042 prot_init 初始化序列（protect.c:321-367）；K-043 内核栈切换的必要性（旧 03 附录 A 收编）；K-044 prot_load_selectors 段寄存器加载（mpx.S:618）；K-045 Rust Protection trait 抽象；K-046 BKL 首现（main.c:149 BKL_LOCK——此处仅一句"串行化原语，全貌在 20"）。
- 不讲什么：时钟/中断控制器的初始化（→06）；trap frame 布局与 IST/sscratch（→13）；syscall MSR 细节（→13）。
- 前置：03。
- 后置：06（保护结构就绪后才能接中断）、13（IDT 门在此建立）、20（BKL 全貌）、38（unsafe/SAFETY 纪律）。
- 事实底线：`main.c:115-157`、`:403-475`；`protect.c` prot_init；`mpx.S:615-618`；`os/kernel/src/lib.rs:378-535`。
- 知识点清单：K-040（概念，protect.c，存量旧 03 Ch1）、K-041（概念/架构演进，lib.rs:394-400，存量旧 06 §1.0 表+旧 03）、K-042（机制，protect.c:321-367，存量旧 03 Ch2）、K-043（约束与不变量，旧 03 附录 A）、K-044（机制，mpx.S:618）、K-045（接口与协议，旧 03 §3）、K-046（概念，main.c:149，首现回指 20）。验收：读者能解释"用户程序写空指针为什么会落到内核 handler"，并对照 Rust kmain 注释块复述 Phase A–F 各自的边界。

### 05-platform-discovery

- 一句话定位：内核如何在不硬编码 MMIO 地址的前提下，从 KernelInfo/DTB/ACPI/QEMU 兜底中发现硬件参数。
- 讲什么：K-050 硬件参数来源问题与硬编码的三类危害；K-051 C 侧对应（i386 acpi_init、earm bsp_init）；K-052 PlatformDesc/Kind/Source trait 体系；K-053 DTB 解析与 QEMU virt 兜底；K-054 PlatformContext write-once 与 init_from_kinfo 注入时机（Phase A.5）；K-055 三架构 QemuVirtDesc 与品牌 struct。
- 不讲什么：控制器寄存器编程（→06）；ACPI 表规范全量（只讲定位）；I-5（RSDP 搜索）登记为 DEFERRED 事实并说明解除条件（todo §7.4）。
- 前置：04。
- 后置：06（控制器基址来源）、10（CPU 拓扑来源）。
- 事实底线：`os/libs/minix-boot/src/platform.rs`；`os/libs/minix-platform/src/{desc,kind,global,device_tree,acpi}.rs`；`os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs`；C：`arch/i386/arch_system.c:246-288`、`arch/i386/acpi.c`。
- 知识点清单：K-050~K-055 六条（存量：旧 04 §1–§4；新增部分：K-055 的 PlatformDescEnum 裁剪合法性裁定，锚点 todo §0.6 B-X 收口）。验收：读者能指出三架构各自的"参数从哪来"路线图，并解释为什么 Rust 侧不需要 C 的 acpi_init 路径（QEMU virt + KernelInfo 已覆盖，物理机为解除条件）。

### 06-clock-intr-init

- 一句话定位：启动期让内核"能听见硬件说话"——init_clock 的软件初始化、中断控制器的初始化与全屏蔽不变量、三架构时钟源与使能时序原则。
- 讲什么：K-060 同步异常 vs 异步中断的分类（概念地基）；K-061 中断控制器的转发器+仲裁模型（PIC/APIC/GIC/PLIC 一览）；K-062 init_clock 纯软件初始化（clock.c:48-66）与"配置→注册→使能分离"时序原则（D-59 方案 A）；K-063 intr_init(0) 与"init 后全屏蔽"不变量（含 arm64 SGI/PPI 补全 D-61）；K-064 arch_init 平台特定初始化；K-065 早期控制台输出（boot 期打印通道）；K-066 TIMER_IRQ 路由差异（x86 IRQ0/arm64 PPI 29/riscv 伪向量）；K-067 使能抽象 TimerIrqGate 与唯一使能入口纪律。
- 不讲什么：运行期 tick 处理（→19）；IRQ 钩子与通知投递（→17）；AP 时钟（→10/19）。
- 前置：04、05。
- 后置：09（boot_cpu_init_timer 在 bsp_finish_booting 的末刻使能）、17（运行期投递）、19（tick 语义）。
- 事实底线：`clock.c:48-66`；`arch/i386/i8259.c`；`arch/i386/arch_system.c` arch_init；`os/arch/src/arch/timer_irq_gate.rs`；`os/kernel/src/lib.rs` Step 6（D-59 重写后）；`os/plat/src/{arm64,riscv64,x86_64}/interrupt.rs`。
- 知识点清单：K-060/K-061（存量旧 05 §1.0）、K-062（存量+新增 D-59 时序原则）、K-063（存量+新增 D-61 全屏蔽）、K-064（存量旧 05 §2）、K-065（存量旧 05 早期控制台节）、K-066/K-067（新增，锚点 minix_plat::TIMER_IRQ 三架构定义与 pin 测试、todo D-59 修复记录）。验收：读者能复述"为什么 arm64 在 Phase B 使能 timer 是错的"（handler 链未接+deadline 积压），并指出三架构的 TIMER_IRQ 值与理由。

### 07-proc-init-boot-proc

- 一句话定位：本 stage 的核心数据模型篇——进程表/特权表/RTS 位图/endpoint 的完整字段语义，与 boot image 从静态表到进程槽的 12 步实例化节拍（存储与流程一体，遵循 06-todo 的"不拆"裁决）。
- 讲什么：K-070 struct proc 字段全集（分组导航：身份/状态/IPC/调度/信号/记账/架构）；K-071 RTS 标志位完整表（主讲述点）；K-072 endpoint 与 generation；K-073 struct priv 权限结构全字段（并入旧 22 结构半）；K-074 权限模板族（TSK_F/VM_F/RSYS_F/IDL_F、TSK_T/SRV_T/CSK_T、TSK_M/SRV_M/ALL_M、TSK_KC/SRV_KC，main.c:199-250 实例化）；K-075 boot_image 静态表 17 项与顺序语义（NOTIFY 优先序：DS 第一、RS 次之，table.c 头注）；K-076 固定容量静态存储与 boot 期无堆约束；K-077 Rust 建模（ProcKind、EntrySpec、CapabilityTemplate；GD-1/GD-2 演进方向登记）；K-078 proc_init 清表流程；K-079 boot image 遍历与 12 步填充节拍；K-080 立即可调度判定与 RTS_NO_PRIV 抑制；K-081 arch_boot_proc 与 VM ELF 加载（load_vm_elf，含 2026-09-17 共享边界页修复）；K-082 RTS_VMINHIBIT/BOOTINHIBIT 双闸；K-083 RTS_PROC_STOP 生命周期（boot 置位/bsp 解除/task 永不解除）；K-084 fill_sendto_mask 与自位恒清（D-58）；K-085 p_misc_flags（MF_*）族；K-086 端点/槽位常量体系（NR_PROCS/NR_TASKS/负端点，吸收旧 99）。
- 不讲什么：调度队列操作（→14）；IPC 消息流程（→15）；priv 的运行时维护与 SYS_PRIVCTL（→27）；FPU 字段细节（→30，字段表留一行）。
- 前置：04。
- 后置：09（RTS_PROC_STOP 解除）、10（AP 侧进程可见性）、11（proc_ptr/队列）、14/15（RTS 联动）、27（priv 运行时）。
- 事实底线：`proc.h`、`priv.h`、`table.c:31-52`、`proc.c:119-160`、`main.c:165-272`、`system/do_exec.c:45`；Rust：`os/kernel/src/{proc.rs,proc_table.rs,kpriv.rs,capability.rs,lib.rs}`、`os/arch/src/{x86_64,arm64,riscv64}/boot.rs`。
- 知识点清单：K-070~K-086 共 17 条（存量：旧 06 全部 + 旧 22 结构半 + 旧 99 全部；新增：K-081 的边界页修复，锚点 edge_todo 诞生链通电条目）。验收：给定任意 RTS 位，读者能说出谁置位、谁解除、对调度的影响；能从 boot_image 表复述 NOTIFY 优先序及其理由；字段导航表保持全集（06-todo v2.1 校准要求）。

### 08-cross-space

- 一句话定位：内核没有自己的页表却要看别人的内存——Direct Map 如何取代 32 位临时窗口，以及运行期窗口覆盖模型。
- 讲什么：K-090 跨空间访问问题（借来的页表 vs 目标进程页表；"每个进程页表都映射内核段"的澄清）；K-091 C 32 位方案 freepdes/ptproc 临时窗口（存档语义）；K-092 64 位 Direct Map 方案（VA=PA+基址的一行加法翻译）；K-093 DM 双窗口与 dm_coverage 运行期窗口覆盖模型（bootstrap/bump/root 候选、E8 下溢修复语义）；K-094 内核无常驻每进程页表、boot 表常驻无 destroy 的架构边界；K-095 阶段 D 时序图（direct_map 版，旧 07 附录 A 收编）。
- 不讲什么：跨空间拷贝的调用面（→22）；VMSUSPEND 协议（→29）；VM 侧页表管理（→02-stage-vm）。
- 前置：02、07。
- 后置：09（memory_init 的 DM 就绪检查）、22（拷贝原语的翻译底座）、29（协议）。
- 事实底线：`arch/i386/protect.c:370-377`、`arch/i386/memory.c:707-717`；`os/kernel/src/dm_coverage.rs`；`notes/rewrite/fork-syscall-rewrite/02-stage-vm/07-pagetable-struct.md` §3.2（redesign 依据，允许引用正式文档）。
- 知识点清单：K-090~K-095（存量：旧 07；新增：K-093 的 dm_coverage 半，锚点 dm_coverage.rs 与 edge_todo E8 四处根因记录）。验收：读者能解释"64 位下为什么不再需要 createpde"，并手算一个 PA 的 DM 别名地址；能说明 E8 下溢 bug 的形状（root 高于 bump 时前裁剪回绕）。

### 09-system-init-boot-finish

- 一句话定位：内核从"初始化态"翻转为"运行态"的全过程——分发表建立、bootstrap 内存回收、bsp_finish_booting 的逐步节拍与关键不变量。
- 讲什么：K-100 system_init 与 call_vec（C）vs enum Syscall match（Rust）的语义对照；K-101 Rust T0–T6 步骤表（实读 lib.rs 的现状时序）；K-102 bsp_finish_booting 全序（main.c:38-109）；K-103 announce 与 release/version 填充（main.c:430-433/:333）；K-104 cycles_accounting_init；K-105 boot_cpu_init_timer 末刻使能原则（回指 06 的时序原则）；K-106 fpu_init 首现（详解引用 30）；K-107 kernel_may_alloc 开关语义（main.c:142/:105）；K-108 memory_init 与 DM 就绪检查。
- 不讲什么：各 dispatch handler（→21–26）；switch_to_user 本体（→11）；AP 时钟（→10）。
- 前置：06、07、08。
- 后置：10（smp_init 位置衔接）、11（本篇末尾调用 switch_to_user）。
- 事实底线：`system.c:168`；`main.c:38-109/:293/:301`；`pg_utils.c:86`；`os/kernel/src/lib.rs`（T0–T6 对应函数行锚随 B 相复核刷新）。
- 知识点清单：K-100~K-108（存量：旧 08；新增：无——但 K-101 的六处行号锚已实测漂移（§3.6 E-4：744→827、780→998、879→1090、1271→1492、1948→2051、2757→2891），B 相必须按符号重锚并实测行号）。验收：读者能按顺序复述 bsp_finish_booting 的 9 步并指出"哪些步骤错了会发生什么"（如时钟不使能→首个 tick 永不到来；kernel_may_alloc 忘关→VM 启动后内核踩 VM 内存）。

### 10-smp-boot

- 一句话定位：多核怎么活过来——smp_init 编排、INIT-SIPI 与实模式阶梯、per-CPU 区建立、boot_lock 握手的真实链路（重写自旧 16 启动半，对齐 2026-09 已落地实现）。
- 讲什么：K-110 BKL 串行化总策略（概念首现回指 04）；K-111 smp_init 编排与 smp_single_cpu_fallback 退路（main.c:303-317）；K-112 AP bring-up 真链：INIT→SIPI 强制延时（10ms/200µs）、16 位实模式阶梯（IVT→PE→32→CR3/PAE/LME/PG→64）、far jump 落点契约、lgdt 16 位寻址 modrm 契约；K-113 multiboot 直启测试路径（QEMU -kernel 绕开 OVMF 的理由与 AP wait-for-SIPI 事实）；K-114 per-CPU 数据区建立（cpulocals、gs:0x10、CpuLocal.ptproc 身份锚）；K-115 boot_lock 与 wait_for_APs_to_finish_booting/ap_boot_finished 握手（smp.c:30/:51）；K-116 OVMF+TCG 环境 AP 停放冲突教训（vector 0x05/0x08 选址，测试性质知识）；K-117 CPU 拓扑发现与 BSP READY 标记（Phase D 安装，test-smp-topo）。
- 不讲什么：BKL 的实现与释放窗口细节（→20）；IPI 运行时语义（→20）；AP 主循环（→20）。
- 前置：07（进程表）、09（smp_init 在 bsp_finish_booting 之前的位置）。
- 后置：11（per-CPU proc_ptr）、19（app_cpu_init_timer）、20（运行时）。
- 事实底线：`smp.c:27-56`；`arch/i386/arch_smp.c`（udelay/SIPI）；`os/kernel/src/smp.rs`（init_ap/boot_lock/CpuLocal）；`smp_todo.md` §21–§25（事实记录，允许引用参考材料）；QEMU 测试 test-smp-ap-alive{,-mb}/test-smp-aps/test-smp-topo*。
- 知识点清单：K-110~K-117 共 8 条（存量：旧 16 §1.1/§2 启动半+§9/§10 增补；新增：K-112/K-113/K-116/K-117，锚点 smp_todo §25 与 todo §12.2 S-3d/S-4/S-5 记录）。验收：读者能画出 x86 AP 从 INIT 到 ap_early_entry 的完整阶梯（含每级模式切换的指令），并解释"为什么 multiboot 直启环境更干净"；能说出 per-CU 三件（ptproc/PLATFORM/tick 桶）各自的承载结构。

### 11-switch-to-user

- 一句话定位：所有内核工作流的唯一出口——switch_to_user 的汇聚点模型、pick_proc、五阶段调度循环与首个用户进程的诞生。
- 讲什么：K-120 汇聚点模型（NOT_REACHABLE、`-> !`、三类来路的唯一归口）；K-121 pick_proc 选择与 idle 兜底（proc.c:1785/:176-193）；K-122 上下文恢复与特权返回指令族（iretq/eret/sret）；K-123 Rust 五阶段调度循环（lib.rs scheduler_loop：选→记账→切地址空间→恢复→循环）与 idle；K-124 switch_address_space_idle 与地址空间切换时机（proc.c:161）。
- 不讲什么：队列的维护（→14）；恢复后的用户态行为（→12/13）；bill 记账（→19）。
- 前置：07、09、10。
- 后置：12（VM 先行被调度）、13（从此再进内核）、14（pick 的队列机制）。
- 事实底线：`proc.c:299-477`、`:1785`、`:161`；`os/kernel/src/lib.rs` scheduler_loop（V13 复核 :2583 起，真 loop :2606-2664）。
- 知识点清单：K-120~K-124（存量：旧 10；K-123 含 I-6 改判事实：生产主循环真实可达、占位仅在 cfg(test/mock) 分支，锚点 todo §23.3）。验收：读者能解释"为什么调度器不是一个线程"，并指出 Rust 循环五阶段与 C switch_to_user 的步骤对应关系。

### 12-vm-boot-protocol

- 一句话定位：VM 拿到 bootstrap 页表之后，如何通过 SYS_VMCTL 与内核协商把系统推进到"运行时可用"。（从旧 09 后移至此，消除"前置 10"的前向引用。）
- 讲什么：K-125 协商问题（bootstrap 页表→运行时的空窗）；K-126 SYS_VMCTL 子命令集与分发表（do_vmctl.c）；K-127 协商时序（switch_to_user→VM 先行→init_page_table→map_kernel→SET_PDBR→解除 VMINHIBIT）；K-128 VMREQUEST 联动概念（协议详解引用 29）；K-129 VmBootHandoff v4（user_sp 增补、内核侧 writer 契约）。
- 不讲什么：页表内容本身（→02-stage-vm）；VMSUSPEND 恢复协议（→29）；kerninfo 页（→33，另一条"内核→用户"的信息通道）。
- 前置：11。
- 后置：29（MEMREQ 消费）、33（对照信息通道）。
- 事实底线：`system/do_vmctl.c`；`arch/i386/arch_do_vmctl.c`；`os/kernel/src/vm_handoff.rs`；edge E-BOOTFRAME（handoff v3→v4 裁决）。
- 知识点清单：K-125~K-129（存量：旧 09；新增：K-129，锚点 edge E-BOOTFRAME 闭环记录）。验收：读者能按序列出 VM 发出的子命令序列，并解释"为什么非 VM 进程要等 VM 才能解除 VMINHIBIT"。

### 13-trap-entry

- 一句话定位：CPU 如何进出内核——五类入口（hwint/s_call/syscall/exception/#NM）的门与帧、trap 桥双腿的寄存器约定、用户上下文的单点真值与恢复路径。**本篇为新建**（吸收 18-trap-bridge-design.md 四项已批准裁决 + E1/E8 落地事实）。
- 讲什么：K-130 向量与门（IDT gate、DPL=3 的 33 号门、SYSCALL MSR/LSTAR/SFMASK/STAR）；K-131 TrapFrame 布局与推栈顺序契约（[ss][rsp] 修正）；K-132 C mpx.S 五入口锚点表（真序表 B 段的展开）；K-133 KERNEL_CALL=0x600 归一化（mpx.S subl；Rust 入口归一修复——minix-types 绝对值 vs 相对判别式）；K-134 trap 桥双腿与寄存器约定（int-33：RAX=端点/RBX=消息/RCX=调用号/RDX=SENDA 表；errno→RAX、status→保存 RBX；SYSCALL 腿 rdi 消息指针）；K-135 用户上下文保存区（一律入 CpuContext、出口唯一=调度循环；四项裁决的"保存区"半）；K-136 二次 IPC 返回通道（set_secondary_ipc_return，207e30644）；K-137 dispatch 唤醒点回写 RAX（发送方完成阻塞 RECEIVE/接收方取走阻塞 SEND 两处，proc.c:969/:1097 系）；K-138 STAR 锚点语义与 sysret +8/+16 约定（GDT[3]=锚点/[4]=DS/[5]=CS、TSS→[6]）；K-139 real-trap feature 门控裁决（hosted 构建不执行真 syscall 指令）。
- 不讲什么：异常 handler 的处置策略（→16）；IRQ 钩子（→17）；do_ipc 语义（→15）；kernel_call 分发（→18）。
- 前置：04（保护结构）、11（调度循环=出口）。
- 后置：14/15/16/17/18（全部入口语义建立在此）、30（#NM 分支）。
- 事实底线：`mpx.S:66/:290-300/:329-337/:361-382/:544-547`；`arch/i386/protect.c:147`（33 号门）；`os/arch/src/x86_64/trap_entry.rs:291-314`、`trap_stub.rs`、`trap_dispatch.rs`；`18-trap-bridge-design.md`（四项裁决）；edge_todo E1 切片 1–5 与 E8 round-2 记录（commits b6571a458/e30674f0d/e54846162/6d8e52c3d/8891e52f9）。
- 知识点清单：K-130~K-139 共 10 条（存量：旧 14 入口机制半；新增：K-134~K-139，锚点如上）。验收：读者能画出"用户 int 0x21 → 内核 → 阻塞或应答 → 回用户或切走"的完整回路图；能解释二次返回通道为什么必须存在（errno 只有一个 RAX）；能指出 STAR 配错的失败形态（SS 恰好正常而 CS 坏的误导性）。

### 14-scheduling

- 一句话定位：就绪队列与进程可运行状态的强一致（INV-1）——enqueue/dequeue/pick_proc/proc_no_time 的原语语义。
- 讲什么：K-140 就绪队列结构与优先级队列；K-141 INV-1（队列成员 ⇔ 可运行）与违反后果；K-142 enqueue/dequeue/enqueue_head（proc.c:1595/1716/1670）；K-143 pick_proc 与 per-CPU 队列（E-SCHEDSMP 现状：sched_for_cpu 恒 BSP 队列的缺口登记）；K-144 proc_no_time 与 quantum 耗尽（含 notify_scheduler 的 PREEMPTIBLE 门缺口 E-PREEMPTFLAG 登记）；K-145 sched_proc（system.c:642-723；niced 字段链路 E-SCHEDNICED 登记）；K-146 Rust Scheduler 拆分与 assume_held 纪律；K-147 阶段 C 的 RTS 位履历（附录，引用 07）。
- 不讲什么：switch_to_user 本体（→11）；IPC 阻塞引起的队列出入（→15）；IPI 跨 CPU 队列操作（→20）。
- 前置：07、11。
- 后置：15（RTS_SENDING/RECEIVING 联动）、19（quantum 递减）、20（跨 CPU）、21（schedctl/runctl）。
- 事实底线：`proc.c:1595-1910`；`system.c:642-723`；`os/kernel/src/{sched.rs,proc_table.rs}`；edge E-SCHEDNICED/E-PREEMPTFLAG/E-SCHEDSMP。
- 知识点清单：K-140~K-147（存量：旧 11；新增：K-143/K-144 的缺口登记半，锚点 edge 三条目）。验收：读者能用 INV-1 推演"rts_set 忘 enqueue"的故障表现；能说出 quantum 耗尽后 SCHED 接管进程的两条分支（内核续量 vs notify_scheduler）及其在当前实现的分界。

### 15-ipc-core

- 一句话定位：微内核的基石机制——无共享内存的消息中转、阻塞唤醒匹配、死锁检测、异步通知与 SENDA。
- 讲什么：K-150 IPC 四职责；K-151 do_ipc 六原语分派（proc.c:599）；K-152 mini_send 阻塞语义与 RTS_SENDING；K-153 mini_receive 匹配序（NOTIFY>异步>同步）与 has_pending；K-154 mini_notify 与 NOTIFY 位图缓冲；K-155 SENDA 异步表与 try_deliver_senda；K-156 deadlock 循环等待检测（proc.c:703）；K-157 delivermsg 延迟拷贝（p_delivermsg）；K-158 MINIX_KERNINFO=6 原语（do_ipc 内核信息分支：未发布→EBADCALL/已发布→二次返回通道，kerninfo 页地址走 RBX）；K-159 IPCNAME 调用名表（main.c:277-290）。
- 不讲什么：过滤决策（→28）；trap 时消息指针如何到达内核（→13）；IPC 状态控制调用 statectl（→21）。
- 前置：07、11、13、14。
- 后置：16（异常通知路径）、17（HARDWARE notify）、18（共享入口）、24（事件类调用）、28（过滤）、33（KERNINFO 原语消费）。
- 事实底线：`proc.c:479-1590`；`ipc.h`；`os/kernel/src/ipc.rs`；`os/kernel/src/kerninfo.rs`；edge E-KERNINFO（2026-09-16/17 记录）。
- 知识点清单：K-150~K-159（存量：旧 12；新增：K-158，锚点 ipcconst.h:12 与 kerninfo.rs）。验收：读者能推演"三方互发形成环"时 deadlock 检测的判定路径；能解释 NOTIFY 永不丢失的机制（位图）与永不阻塞的机制；能写出 round-trip 两次 int-33 各自的返回值形状。

### 16-exception-routing

- 一句话定位：异常（同步事件）的分流处置——哪些异常变成信号、哪些转发 VM 修页、哪些交给 FPU 子系统。
- 讲什么：K-160 异常分类与向量表（exception.c）；K-161 exception_handler 处置全序（:155-170：打印回溯→判定致命→cause_sig 或转发）；K-162 cause_sig 机制（主讲述点：system.c:389，RTS_SIGNALED 置位与通知信号管理器；致命/SELF 子路径 D-11）；K-163 页错误转发 VM 的请求形状（cr2/err编码→VM 请求；恢复协议引用 29）；K-164 #NM 分支（copr_not_available_handler 的入口位，语义引用 30）；K-165 dispatch_exit 的 cause_sig 全覆盖语义（D-6/D-13 落地事实）。
- 不讲什么：信号调用的用户态接口（→23）；VMSUSPEND 挂起-恢复协议（→29）；FPU 保存细节（→30）；IRQ（→17）。
- 前置：11、13、14、15。
- 后置：23（KILL/GETKSIG 消费 cause_sig 的产物）、29（页错误恢复）、30（#NM 全貌）。
- 事实底线：`arch/i386/exception.c`；`system.c:389-467`；`os/arch/src/arch/exception_dispatcher.rs`；`os/kernel/src/page_fault.rs`。
- 知识点清单：K-160~K-165（存量：旧 14 异常半；新增：K-165，锚点 todo §0.1 D-6/D-11/D-13 批次记录）。验收：给定一个异常向量+错误码，读者能说出它的四类归宿（忽略/杀进程/转发 VM/交 FPU）与判定条件。

### 17-irq-delivery

- 一句话定位：异步事件的投递管道——硬件中断如何经钩子链变成对驱动的 HARDWARE 通知，以及三架构控制器的投递差异。
- 讲什么：K-166 put_irq_handler/rm_irq_handler 钩子链（interrupt.c:29/:75）；K-167 irq_handle 分发与"通知唤醒"模型（:116；notify 语义引用 15）；K-168 enable_irq/disable_irq 与嵌套计数（:161/:169）；K-169 Rust IrqManager dispatch 全序：claim→mask→handler 链→unmask→eoi（D-61 修复后的协议；claim 取号→同号 complete 的 GIC/PLIC 契约）；K-170 三架构控制器差异表（x86 无 claim 寄存器/EOI 即完整握手；arm64 IAR/EOIR 与 SGI/PPI banked；PLIC claim/complete 且无 0 号源）；K-171 IrqVector 伪向量语义（riscv timer 直连 sie，"无控制器线"的表达）。
- 不讲什么：驱动的注册侧调用 SYS_IRQCTL（→24）；控制器初始化（→06）；IPI（→20）。
- 前置：06、13、15。
- 后置：19（timer tick 是本管道的首个客户）、24（IRQCTL 调用）。
- 事实底线：`interrupt.c` 全文（177 行）；`os/kernel/src/irq_manager.rs`（dispatch 序列钉住测试 dispatch_claims_before_mask_and_completes_last）；`os/plat/src/interrupt.rs` trait 文档表。
- 知识点清单：K-166~K-171（存量：旧 14 IRQ 半+旧 20 §1.1 概念半（主讲述点迁移至此）；新增：K-169~K-171，锚点 todo D-61 修复记录与 D-59 TIMER_IRQ 常量）。验收：读者能对比三架构"一次中断的完整握手"差异表并解释为什么以 0 完成 EOI 会引发中断风暴。

### 18-syscall-dispatch

- 一句话定位：46 个内核调用的统一框架——从消息拷贝到权限门到结果回写的分派循环，以及 C 表驱动与 Rust 枚举 match 的语义对照。
- 讲什么：K-172 kernel_call 入口与用户消息拷贝（system.c:136）；K-173 kernel_call_dispatch 权限门（s_k_call_mask 检查；结构语义引用 07）；K-174 kernel_call_finish 回复与 BklGuard transfer（:59）；K-175 kernel_call_resume 概念位（协议主讲述点引用 29）；K-176 C call_vec[] vs Rust enum Syscall+try_from+match（46 项覆盖、11 空洞与 C 同为 WONTFIX 的对账表）；K-177 dispatch 大函数拆分事实（C1：getinfo/privctl/vmctl/trace 四函数）；K-178 SYS_* 调用号体系（KERNEL_CALL 基址、绝对值归一，E-MINTYPES-SYS 缺口登记）；K-179 VMSUSPEND 结果类的识别与挂起（判定在此，恢复协议引用 29）。
- 不讲什么：各 handler 内部（→21–26）；trap 指令层（→13）；过滤的 IPC 侧（→28）。
- 前置：07、09、13、14、15。
- 后置：21–26 全部（框架）、27（privctl）、29（resume 协议）。
- 事实底线：`system.c:52-167/:612-637`；`os/kernel/src/syscall.rs`；edge E-MINTYPES-SYS。
- 知识点清单：K-172~K-179（存量：旧 13；新增：K-177/K-178 的现状半，锚点 todo §1 C1 与 edge）。验收：读者能追踪一个 SYS_GETINFO 调用从 int 指令到回复的每一步（跨 13/18/26 三篇按锚点串联）；能解释"11 个空洞为什么与 C 一致地 WONTFIX"。

### 19-clock-timer

- 一句话定位：tick 里发生的全部事——三钟体系、定时器队列、quantum 递减、CPU 记账与负载平均。
- 讲什么：K-180 timer_int_handler 全序（clock.c:70-173）；K-181 三钟（realtime/monotonic/boottime）与 adjtime delta（:178-227）；K-182 内核定时器队列（set_kernel_timer/reset_kernel_timer/TimerAction，:229-258）；K-183 load_update 负载平均（:260）；K-184 billp 三用途与 CPU 记账分类（arch_clock.c；CP_IDLE/USER/SYS/NICE/INTR 五桶；I-16 修复后的完整链）；K-185 app_cpu_init_timer（AP 侧，:306）；K-186 DEFAULT_HZ 三方不一致与裁决（C i386=60/earm=1000/Rust=100，OQ-15-1 维持 100）；K-187 Rust 时钟单源化（I-15：镜像原子删除、clock_state 直读契约）。
- 不讲什么：时钟调用面（→25）；init 期编程（→06）；调度队列操作（→14）。
- 前置：14、16、17。
- 后置：25（SETALARM 复用队列）、35（采样时钟）。
- 事实底线：`clock.c` 全文（312 行）；`arch/i386/arch_clock.c`；`os/kernel/src/clock.rs`；OQ-15-1 裁决（archconst.h:4）。
- 知识点清单：K-180~K-187（存量：旧 15；新增：K-184 完整记账链与 K-187，锚点 todo I-6/I-15/I-16 闭环记录）。验收：读者能列出一次 tick 的副作用全集并区分"每 tick 都做"与"按条件做"；能解释记账为什么以 bill_ptr 而非 proc_ptr 归账。

### 20-smp-runtime

- 一句话定位：多核共存一个内核的运行时规则——BKL 的实现与释放窗口、IPI 跨 CPU 调度四原语、迁移与关停。
- 讲什么：K-190 BKL 实现细节（SPINLOCK_DECLARE/xchg；smp.c:27）；K-191 BKL 释放窗口三处（时钟中断/IPI 处理/同步等待，smp.c:44/:86-94）与并发窗口推理（cross_space_copy 无需额外同步的理由）；K-192 IPI 机制与向量（smp_sched_handler/smp_ipi_halt_handler，:156/:194/:56）；K-193 跨 CPU 调度四原语（smp_schedule_sync/stop_proc/vminhibit/migrate_proc，:75-156）；K-194 cpu_is_ready/亲和/EBADCPU 链（system.c:650-654；E-SCHEDSMP 三件套现状登记）；K-195 smp_shutdown_aps 与多核关停（main.c:382）；K-196 S-7 AP 主循环与 BSP 同构（BKL 所有权前置）；K-197 IPI 往返真机事实（0xF0 路由/LAPIC 使能/riscv SSIE）；K-198 shutdown(0) 语义层与 isa-debug-exit 后端；K-199 Frozen<T> 冻结原语与 per-CPU 决策冻结模式。
- 不讲什么：AP bring-up（→10）；Rust 治理机制的审批清单细节（→38，本篇讲语义）。
- 前置：10、14、17、19。
- 后置：21（runctl 的 IPI 臂）、32（关停链引用）、38（witness 体系）。
- 事实底线：`smp.c` 全文（205 行）；`os/kernel/src/smp.rs`；smp_todo §23/§24（S-5/S-6/S-7/S-10/S-11 记录）；test-smp-ipi{,-riscv64}/test-smp-shutdown。
- 知识点清单：K-190~K-199（存量：旧 16 运行时半；新增：K-196~K-199，锚点 smp_todo 与 todo §12.2）。验收：读者能画出"CPU0 让 CPU3 上的进程停下的完整消息序"（含 IPI、BKL 交接、上下文保存）；能指出 BKL 释放窗口内哪些数据仍有被并发触碰的风险。

### 21-syscall-process

- 一句话定位：进程生命周期七调用——fork/exec/exit/clear/runctl/schedctl/statectl 如何驱动状态弧并保持各表原子一致。
- 讲什么：K-200 fork（同步前提：父必须 RECEIVING；代际；降权；msgaddr 出参与 eager-CoW 契约）；K-201 exec（映像替换；arch_proc_init 设置 PC/SP——这也是 kernel task 无恢复点的证据链一环，回指 K-002）；K-202 exit（自杀信号委托 cause_sig，引用 16）；K-203 clear（幂等回收；clear_endpoint/clear_ipc_refs 引用）；K-204 runctl（停止/恢复 + SMP IPI 臂，引用 20）；K-205 schedctl（KERNEL/SCHED 双模式）；K-206 statectl（IPC 状态控制；VMSTYPE/MAP 语义与 clear_memreq）。
- 不讲什么：调度参数语义（→14）；页表/地址空间侧的 fork/exec（→02-stage-vm）；信号管理器侧（→23）。
- 前置：07、14、18、20。
- 后置：无强制后置；27（fork 的 priv 分配消费）回指。
- 事实底线：`system/{do_fork,do_exec,do_exit,do_clear,do_runctl,do_schedctl,do_statectl}.c`；`os/kernel/src/syscall_process.rs`；edge E-FORKMSG（ipc.h:283-287 wire 修复）。
- 知识点清单：K-200~K-206（存量：旧 17；新增：K-200 的 msgaddr/eager-CoW 半，锚点 E-FORKMSG 闭单记录）。验收：读者能画出 fork→exec→exit→clear 的状态弧并标注每步的 RTS/priv/队列变更；能解释内核写 fork 应答为何可能撞上 VM 的 eager-CoW 死锁及 wire 修复如何消除。

### 22-syscall-copy

- 一句话定位：按信任分层的跨空间内存原语——vircopy/physcopy 直拷、safecopy 授权拷贝、umap 查询、memset 填充，以及 Direct Map 下的实现形态。
- 讲什么：K-207 信任分层模型；K-208 vircopy/physcopy 与 data_copy/virtual_copy_f；K-209 data_copy_vmcheck 地址校验与 Suspended 判定（返回值语义；恢复引用 29）；K-210 grant 授权模型（SAFECOPYFROM/TO/VSAFECOPY、grant 表、verify_grant/ELOOP/magic 重定向）；K-211 magic grant 门与 VFS/MIB 端点常量（C: do_safecopy.c:221 用 VFS=1/MIB=7；Rust grant.rs:293/295 偏差 = E-MIBGRANT 缺口登记）；K-212 umap/umap_remote/vumap（DMA 地址翻译查询）；K-213 memset/safememset；K-214 Rust cross_space_copy<D: DirectMapArch> 泛型核心与 vmcheck 封装；K-215 批量 I/O 缓冲对齐事实（IoBatchBuf，V12-B2）。
- 不讲什么：VMSUSPEND 恢复协议（→29）；设备寄存器访问（→24）；DM 窗口建立（→08）。
- 前置：08、14、18。
- 后置：29（其协议消费本篇的 vmcheck 判定）、24（data_copy_vmcheck 依赖）。
- 事实底线：`system/{do_copy,do_safecopy,do_umap,do_umap_remote,do_vumap,do_memset,do_safememset}.c`；`arch/i386/memory.c`；`os/kernel/src/{syscall_copy.rs,cross_space.rs,grant.rs}`。
- 知识点清单：K-207~K-215（存量：旧 18；新增：K-211 缺口登记半与 K-215，锚点 edge E-MIBGRANT、todo V12-B2）。验收：读者能对一次 PM 发起的 sys_safecopyfrom 列出全部校验步骤及其错误码；能解释 grant 的 ELOOP 防护与 magic 重定向。

### 23-syscall-signal

- 一句话定位：信号调用五件套——内核"推拉结合"的信号中介模型：KILL/GETKSIG/ENDKSIG 三步闭环与 SIGSEND/SIGRETURN 的 POSIX 路径。
- 讲什么：K-216 双路径模型（内核信号路径 vs POSIX 路径，共享 p_pending/RTS 位）；K-217 SYS_KILL（入口，引用 16 的 cause_sig）；K-218 SYS_GETKSIG/ENDKSIG（管理器轮询与确认）；K-219 SYS_SIGSEND（VM proxy、用户栈 sigframe、PC/SP 改写）；K-220 SYS_SIGRETURN（寄存器恢复）；K-221 sig_delay_done 与 SIGSNDELAY（system.c:454；D-13 双路接线）；K-222 信号常量族（SIGKSIG=74/SIGKSIGSM=73/SIGSNDELAY=70/SIGS_IS_LETHAL/SC_MAGIC）；K-223 SigSet 位宽问题（u64 装不下 70/73/74 的 128 位拓宽登记，edge E6 记录）；K-224 FPU 保存联动位（save_fpu，详解引用 30）。
- 不讲什么：cause_sig 的异常来源（→16）；PM 侧信号策略（→04-stage-pm）；FPU 状态内容（→30）。
- 前置：07、14、16、18。
- 后置：30（sigframe 的 FPU 保存）。
- 事实底线：`system/{do_kill,do_getksig,do_endksig,do_sigsend,do_sigreturn}.c`；`system.c:389-467`；`os/kernel/src/syscall_signal.rs`。
- 知识点清单：K-216~K-224（存量：旧 19；新增：K-223，锚点 edge E6 V3-P1-2 切片记录）。验收：读者能对照两条路径画出"信号从产生到处理器执行到返回"的时序并指出内核只做的三类特权操作。

### 24-syscall-device

- 一句话定位：用户态驱动与硬件的合法通道——IRQ 钩子注册、端口 I/O（单次/批量/跨进程）、IOPL 提权与 BIOS 读取的权限模型。
- 讲什么：K-225 SYS_IRQCTL（hook 注册/启用/禁用；钩子语义引用 17）；K-226 SYS_DEVIO/VDEVIO（端口与批量）；K-227 SYS_SDEVIO（跨进程端口 I/O）；K-228 CHECK_IO_PORT/CHECK_IRQ 权限判定（字段语义引用 07，校验序在本篇）；K-229 IOPL 提权（IOPENABLE/enable_user_io；GD-3 trait 拆分演进登记）；K-230 READBIOS；K-231 非 x86 退化策略（trait+BadCall/EBADREQUEST；SYS_PADCONF 奇偶闭合判例）。
- 不讲什么：中断的投递管道（→17）；驱动框架（→16-stage-drivers）。
- 前置：07、17、18、22。
- 后置：无强制；16-stage-drivers 消费。
- 事实底线：`system/{do_irqctl,do_devio,do_vdevio}.c`；`arch/i386/{do_sdevio,do_iopenable,do_readbios}.c`；`os/kernel/src/syscall_device.rs`、`irq_manager.rs`。
- 知识点清单：K-225~K-231（存量：旧 20；新增：K-231 的 PADCONF 判例半，锚点 todo SYS_PADCONF 闭合记录）。验收：读者能回答"驱动如何收到中断、如何被限制在授权端口内"，并指出三架构上哪些调用退化为 BadCall 及理由。

### 25-syscall-clock

- 一句话定位：时间服务的用户态接口——times/setalarm/stime/settime/vtimer 五调用如何复用 19 的内核计时设施。
- 讲什么：K-232 SYS_TIMES（进程时间统计；TimeStats.virt_left/prof_left）；K-233 SYS_SETALARM（同步闹钟：watchdog timer→CLOCK notify；应答臂 time_left/uptime 回读语义）；K-234 SYS_STIME/SETTIME（系统钟设置；set_realtime/set_boottime/set_adjtime_delta 消费）；K-235 SYS_VTIMER（ITIMER_VIRTUAL/PROF；M2 wire 形状与旧值回读）。
- 不讲什么：tick 内部（→19）；vtimer_check 的递减与发信号（→19，此处只讲调用契约）。
- 前置：18、19、07。
- 后置：无强制。
- 事实底线：`system/{do_times,do_setalarm,do_stime,do_settime,do_vtimer}.c`；`os/kernel/src/syscall_clock.rs`。
- 知识点清单：K-232~K-235（存量：旧 21）。验收：读者能区分"同步闹钟（到点通知进程自己）"与"虚/prof 定时器（tick 中递减并发信号）"两条机制及其归属队列。

### 26-syscall-misc

- 一句话定位：不在五族中的调用与诚实登记的缺口——getinfo/trace/update/sprofile 四主力 + abort/schedule/setgrant/mcontext 四个历史上无主讲述点的族的落位。
- 讲什么：K-236 SYS_GETINFO 全臂（machine/hz/meminfo/proctab/privtab/image/…；chunked 整表拷贝）；K-237 GET_PROCTAB/GET_PRIVTAB 生产布局与 IS 快照对账缺口（E-ISPROD 登记三重不一致事实）；K-238 GET_KMESSAGES 缺口（E-ISKMESS：64 位无 .usermapped 后的等价通道设计）；K-239 SYS_TRACE（PTRACE 语义；SC_TRACE→SIGTRAP）；K-240 SYS_UPDATE（live-update 12 步）；K-241 SYS_SPROFILE（采样控制面；deferred 体与两处 documented 豁免）；K-242 SYS_ABORT（do_abort.c 落位）；K-243 SYS_SCHEDULE（do_schedule.c 落位；dispatch 丢弃 niced 的缺口=E-SCHEDNICED）；K-244 SYS_SETGRANT（do_setgrant.c 落位）；K-245 SYS_GETMCONTEXT/SETMCONTEXT（do_mcontext.c 落位）；K-246 krandom 熵与 read_tsc（D-33 [ARCH: deviation] 三层标注事实）；K-247 事实勘误记录（本树无 do_unused.c；错误声明的实际位置是旧 00 导航表 `:284`，修正去向见 §3.6 E-1）。
- 不讲什么：kmess 缓冲机制（→32 的 W-7 演进）；IS 侧消费（→08-stage-is）。
- 前置：18 + 21–25（差异对照需要）。
- 后置：35（sprofile 数据面衔接）。
- 事实底线：`system/{do_getinfo,do_trace,do_update,do_sprofile,do_abort,do_schedule,do_setgrant,do_mcontext}.c`；`os/kernel/src/{misc.rs,krandom.rs}`；`ls minix3/minix/kernel/system/`（38 个文件清单）。
- 知识点清单：K-236~K-247（存量：旧 25；新增：K-242~K-245 四族落位与 K-246/K-247，锚点 edge E-ISPROD/E-ISKMESS/E-SCHEDNICED、todo D-33、文件清单实证）。验收：读者能对 38 个 do_*.c 逐一到新目录找到归属（覆盖率检查的机械化形式）；能说出 GETINFO 布局对账缺口的三个不一致维度。

### 27-privilege-runtime

- 一句话定位：特权的运行时维护——RS 如何经 SYS_PRIVCTL 给服务发权限，irq/io/mem 授权如何登记，sendto/kcall 掩码如何随生命周期演化。
- 讲什么：K-248 SYS_PRIVCTL（GET_PRIV/SET_PRIV 等；结构语义引用 07）；K-249 get_priv 分配与 static_priv_id（system.c:274）；K-250 priv_add_irq/io/mem（system.c:918/945/973）；K-251 set/unset_sendto_bit 与 fill_sendto_mask 的运行时半（:307/:335/:349）；K-252 send_sig（SYSTEM→PM 信号请求，:364）；K-253 USER_PRIV 共享模板 vs 系统进程独立 priv 的运行时差异；K-254 PREEMPTIBLE 特权标志（E-PREEMPTFLAG 缺口登记：C 两处门 proc.c:1638/:1895，Rust 优先级近似的分叉后果）。
- 不讲什么：priv 字段定义（→07）；IPC 消息级过滤（→28）。
- 前置：07、18、21。
- 后置：28（过滤器挂在 priv 上）、24（CHECK_* 消费）。
- 事实底线：`system.c:274-989`；`do_privctl.c`；`priv.h`；`os/kernel/src/{kpriv.rs,priv_table.rs}`；edge E-PREEMPTFLAG。
- 知识点清单：K-248~K-254（存量：旧 22 运行时半；新增：K-254，锚点 edge E-PREEMPTFLAG）。验收：读者能复述"一个新服务从 RS_SET_PRIV 到能收发消息"的授权步骤链；能说明 PREEMPTIBLE 缺失时 SCHED 接管进程为何被无限续量。

### 28-ipc-filter

- 一句话定位：IPC 的最小特权执行层——调用门/发送方/接收方偏好三层过滤的时机与数据结构。
- 讲什么：K-255 三层过滤模型（什么时机检查什么）；K-256 ipc_filter_t 与过滤器池（IPCF_POOL_INIT，main.c:158）；K-257 add/clear_ipc_filters（SYS_PRIVCTL 过滤臂，system.c:705/:751）；K-258 check_ipc_filter/allow_ipc_filtered_msg（:776/:803）；K-259 allow_ipc_filtered_memreq（:879）；K-260 Rust ipc_filter_pool 治理（裸访问器消灭 D-63②）。
- 不讲什么：IPC 本体（→15）；kcall 掩码（→18/27）。
- 前置：07、15、18、27。
- 后置：无强制。
- 事实底线：`ipc_filter.h`、`include/minix/ipc_filter.h`、`system.c:705-917`；`os/kernel/src/ipc_filter.rs`。
- 知识点清单：K-255~K-260（存量：旧 23；新增：K-260，锚点 todo D-63②）。验收：读者能对一次 send 列出三层决策的先后与短路条件。

### 29-cross-space-runtime

- 一句话定位：内核代行与缺页悖论——VMSUSPEND 挂起-恢复协议：内核代用户进程访问未映射内存时如何优雅地"暂停作业、请 VM 补页、原地续走"。
- 讲什么：K-261 悖论陈述（内核无缺页 handler 的设计选择）；K-262 vm_suspend 与 RTS_VMREQUEST/p_vmrequest（proc.c:234）；K-263 MF_KCALL_RESUME 与 kernel_call_resume 续走（system.c:612；主讲述点，从 18 迁入）；K-264 VMCTL_MEMREQ 请求族（VM 消费请求→回 SET_PDBR 类恢复）；K-265 Rust suspend_for_vm_with_copy/VmSuspendContext（上下文携带被拷贝缓冲的语义）；K-266 cross_space_write SELF 语义原语（E8 新增：内核局部直读 vs 用户目的地走 PTE+DM+VMSUSPEND 簿记的分界）。
- 不讲什么：拷贝原语本身（→22）；VM 侧处理（→02-stage-vm）。
- 前置：12、18、22。
- 后置：31（跨空间读用户栈复用协议）。
- 事实底线：`proc.c:234-298`；`system.c:612-641`；`vm.h`（VMSUSPEND/EFAULT_SRC/DST）；`os/kernel/src/{vm.rs,proc.rs,pte_walk.rs}`。
- 知识点清单：K-261~K-266（存量：旧 24；新增：K-266，锚点 edge E8 ③ 修复记录，misc.rs:622）。验收：读者能画出一次 do_getinfo 撞上未映射用户缓冲的完整时间线（挂起→VM 补页→resume→重拷），并指出每一步的 RTS/MF 位变化。

### 30-fpu

- 一句话定位：FPU 上下文切换——让"保存/恢复"成本与真实使用挂钩的 lazy 模型与 #NM 陷阱路径。
- 讲什么：K-267 FPU 状态构成与切换成本（FXSAVE/XRSTOR 512B~4KB）；K-268 lazy 模型与 CR0.TS/#NM（arch_system.c:51-215；mpx.S:537-552）；K-269 copr_not_available_handler 与 fpu_owner 协议（proc.c:1922-1959）；K-270 fpu_init/save_fpu/release_fpu 与信号联动（main.c:78；do_sigsend.c:86）；K-271 三架构 FpuArch trait 与现状（FpuTrap 分发已实现；lazy-restore 主体与信号路径保存为显式缺口——如实登记）。
- 不讲什么：#NM 的异常路由框架（→16）；sigframe 布局（→23）。
- 前置：11、13、16、20、23。
- 后置：无。
- 事实底线：`arch/i386/arch_system.c:51-215`、`proc.c:1922-1959`、`exception.c:375-383`、`mpx.S:537-552`、`do_sigsend.c:86`；`os/arch/src/arch/fpu_arch.rs` 与三架构 fpu.rs。
- 知识点清单：K-267~K-271（存量：旧 31）。验收：读者能推演"两个进程交替用 FPU"的完整切换序（含 TS 位翻转与 owner 换主），并说出当前 Rust 实现的缺口边界。

### 31-stack-tracing

- 一句话定位：栈回溯——frame-pointer 链遍历原理与跨地址空间读取，内核崩溃诊断的基础设施。
- 讲什么：K-272 回溯原理与三使用场景（崩溃诊断/GET_PROC 附加/PANIC）；K-273 proc_stacktrace 家族与跨空间读栈（exception.c；复用 29 的协议语义）；K-274 DIAGCTL STACKTRACE 与 util_stacktrace（do_diagctl.c:STACKTRACE 臂；utility.c）；K-275 StacktraceArch trait 三架构实现（x86_64 fp 链/arm64/riscv64 差异）。
- 不讲什么：VMSUSPEND 协议本体（→29）；panic 的处置链（→32）。
- 前置：16、22、26（DIAGCTL 臂）、29。
- 后置：32（panic 调用回溯）。
- 事实底线：`arch/i386/exception.c`（proc_stacktrace 族）；`system/do_diagctl.c`；`utility.c`；`os/arch/src/arch/stacktrace.rs`、`os/kernel/src/stacktrace.rs`。
- 知识点清单：K-272~K-275（存量：旧 32）。验收：读者能手工沿一个 frame-pointer 链走三帧并解释 riscv64 无 fp 时策略差异。

### 32-utility-console-shutdown

- 一句话定位：内核如何报错、说话与谢幕——panic 链、控制台通道（kmess→EarlyConsole 的演进）、_exit 禁令与完整关停链。
- 讲什么：K-276 panic 全链（格式化→stacktrace→复位；utility.c；Drop 防御 panic-in-drop 吸收）；K-277 kputc/kmess 环形缓冲与 W-7 演进（EarlyConsole+log 后端替代，[ARCH] 语义）；K-278 _exit 禁令；K-279 prepare_shutdown 延时关停（main.c:351-363：1 秒定时器给进程收尾窗口）；K-280 minix_shutdown 全序（:368-398：smp_shutdown_aps→hw_intr_disable_all→stop_local_timer→direct 打印→arch_shutdown）；K-281 Rust shutdown 语义层与后端（S-11：isa-debug-exit 等）。
- 不讲什么：GET_KMESSAGES 调用面（→26）；诊断内容（→34）。
- 前置：04、09、20、26。
- 后置：无。
- 事实底线：`utility.c`（93 行）；`main.c:351-398`；`arch/i386/arch_reset.c`；`os/plat/src/early_console.rs`；`panic-in-drop.md`（吸收后归档）。
- 知识点清单：K-276~K-281（存量：旧 27；新增：K-279/K-280/K-281，锚点 main.c 与 todo S-11）。验收：读者能复述从 panic 触发到机器复位的每一步，以及从 RS 请求关停到电源切断的完整链。

### 33-kerninfo-user-data

- 一句话定位：用户可见内核数据——C 的 .usermapped 段机制（32 位存档）与 Rust 的 kerninfo 页（64 位现役）两条路线的对照与现役实现。
- 讲什么：K-282 .usermapped 段机制（专用链接段+VM 映射+用户直读；kernel.lds/usermapped_data.c/usermapped_glo_ipc.S）；K-283 W-2 裁决（64 位不移植的完整理由）；K-284 kclockinfo 内部化（时钟数据不再用户可见）；K-285 kerninfo 页真实现（KerninfoPage 静态页、kuserinfo 填充（kui_size/user_sp，main.c:438-440 对应）、活动根 query 翻译 PA、Paging::map 只读映射、原子发布）；K-286 KERNINFO_USER_VA 选址（0x2_0000_0000：高于 4GiB identity、低于 Sv39 上限、不与 handoff 共 PD 项）；K-287 walk_alloc USER 位传播契约（U/S 逐层 AND 的教训）；K-288 用户态消费半边界（minix-rt query_kerninfo，归属 14-stage，登记接口）。
- 不讲什么：MINIX_KERNINFO 调用原语侧（→15）；kmess（→32）。
- 前置：03、07、08、11、12、15。
- 后置：14-stage-runtime（跨 stage 边界声明）。
- 事实底线：`usermapped_data.c`、`arch/i386/usermapped_data_arch.c`、`usermapped_glo_ipc.S`、`kernel.lds`；`os/kernel/src/kerninfo.rs`；edge E-KERNINFO（commits 207e30644/a071af5e5，test-user-trap 五断言）。
- 知识点清单：K-282~K-288（存量：旧 28；新增：K-285~K-288，锚点如上）。验收：读者能对比两条路线的"用户如何拿到数据"（段直读 vs 固定 VA+调用协商），并复述 kerninfo 页从构建到发布的五步。

### 34-kernel-debug

- 一句话定位：内核自我诊断——调度队列一致性断言、状态打印族，以及被诚实排除的 IPC 跟踪 hook 族。
- 讲什么：K-289 runqueues_ok/runqueues_ok_cpu（INV-1 的运行期断言）；K-290 rtsflagstr/miscflagstr/print_proc 打印族；K-291 IPC 跟踪/统计 hook 族（W-3 排除：panic 路径风险、log+外部工具替代）；K-292 DEBUG_* 条件编译族与构建开关；K-293 Rust debug_assert 纪律（A1 的 8 处测试隐患即由此揪出的事实）。
- 不讲什么：栈回溯（→31）；profiling（→35）。
- 前置：14、15、20。
- 后置：无。
- 事实底线：`debug.c`（563 行）、`debug.h`；`os/kernel/src/debug.rs`。
- 知识点清单：K-289~K-293（存量：旧 29；新增：K-293，锚点 todo §1 A1）。验收：读者能说明 runqueues_ok 检查哪些不变量、在什么时机被调用。

### 35-kernel-profile

- 一句话定位：内核统计 profiling——采样时钟驱动"CPU 时间花在哪"，以及 NMI 采样为什么被排除。
- 讲什么：K-294 SPROFILE 采样模型（时钟采样 vs NMI 采样）；K-295 sprofiling 开关与控制面（SYS_SPROFILE 消费，引用 26）；K-296 采样数据面（sample buffer、misc.rs dispatch_sprofile 全实现事实）；K-297 NMI 臂排除（W-4/W-5：无 NMI 子系统）；K-298 SPROF 静态治理（SPROF_INFO/buffer 收编 globals.rs，D-62③④）。
- 不讲什么：tick 记账（→19）；watchdog（→36）。
- 前置：19、26。
- 后置：36（对照排除）。
- 事实底线：`profile.c`（157 行）；`system/do_sprofile.c`；`os/kernel/src/{misc.rs,clock.rs,globals.rs}`。
- 知识点清单：K-294~K-298（存量：旧 30；新增：K-298，锚点 todo D-62/I-15）。验收：读者能画出"开采样→每 tick 记 PC→关采样→读 buffer"的数据流。

### 36-watchdog

- 一句话定位：NMI watchdog 的原理与 WONTFIX 裁决存档——读它是为了知道"内核 lockup 检测"为什么在本项目不做。
- 讲什么：K-299 NMI watchdog 原理（tick 停滞检测）；K-300 W-1 全族排除理由（架构覆盖不全+默认关+依赖 PMU；sprofile 替代）；K-301 对照阅读位（与 35 的采样互补关系）。
- 不讲什么：实现细节逐函数展开（存档文档保持精简，~150 行目标）。
- 前置：35。
- 后置：无。
- 事实底线：`watchdog.c`（112 行）、`watchdog.h`、`arch/i386/arch_watchdog.c`。
- 知识点清单：K-299~K-301（存量：旧 26）。验收：读者能用三句话向他人解释"为什么不移植 watchdog"。

### 37-test-infra

- 一句话定位：**新建**。这个内核怎么被验证——31 个 QEMU 测试内核的族谱与命名法、hosted 测试策略、CI 与构建工具链。
- 讲什么：K-302 QEMU 测试内核族谱（bootstrap 31 个：hello-boot/higher-half/kernel-map/memmap/paging-enable/protection/proc-init/smp-topo/smp-ap-alive{,-mb}/smp-aps/smp-ipi{,-riscv64}/smp-shutdown/timer-irq/user-trap/rt-birth；单架构 vs 三架构族的划分）；K-303 驱动脚本与断言方式（run_all.sh/run_qemu.sh/test-user-trap.sh；串口 TEST_RESULT 断言 + monitor xp 内存断言两种范式）；K-304 gen-test-kernel.sh 模板化（--new/--apply/--check；23 文件规范化；聚合 crate 方案否决理由）；K-305 CI（qemu-tests.yml、QEMU_TESTS_SKIP_AP_ALIVE 门控）；K-306 hosted 测试策略（RUST_TEST_THREADS=1、RUST_MIN_STACK=8388608、mock trait 矩阵、742+ 测试的分层：unit/hosted-boot/qemu 真机）；K-307 production-target check（三架构编译验证作为测试之外的第三道门）；K-308 kernel ELF 独立构建缺口（I-1：三架构 link.ld 就绪但无引导路径；入口 ABI 未设计；解除条件）；K-309 测试内存铁律（ulimit -v 3G + -j 1 的 WSL 约束）。
- 不讲什么：单个机制的测试断言细节（归各机制篇的"测试要点"节）；用户态 rt 侧测试（→14-stage）。
- 前置：00（导航）+ 按需回指。
- 后置：无（工程支线）。
- 事实底线：`os/qemu-tests/` 全目录；`tools/gen-test-kernel.sh`；`.github/workflows/qemu-tests.yml`；`os/.cargo/config.toml`；todo §12.0 环境基线。
- 知识点清单：K-302~K-309（全部新增；锚点：目录清单 ls 实证、脚本文件、workflow 文件、config.toml、todo §7.3 I-1）。验收：读者能按"要验证机制 X"查出应跑哪个测试内核与断言什么；能解释 hosted 与 QEMU 两级的分工与各自的不可替代性。

### 38-rust-discipline

- 一句话定位：**新建**。用 Rust 写内核的工程纪律——全局状态治理、BKL witness 体系、错误单源、cfg 禁令、unsafe 与 panic 审计。
- 讲什么：K-310 no_std 全链与 boot 期无堆→静态表约束；K-311 SyncUnsafeCell + sealed trait BklProtected + bkl_protected_impls! 审批清单（globals.rs 24 项清单的治理模型；static mut 清零史）；K-312 BklSection witness/assume_held 链根与 BklGuard::transfer（ManuallyDrop 全库唯一 forget 收敛点）；K-313 全局静态收敛治理（A2：krandom/CPU_INFO/SPROF 收编；时钟单源 I-15）；K-314 Errno(i32) newtype + ToErrno 单一权威（D1/D2；minix-types 权威、kernel re-export）；K-315 ProcNr newtype（G1；Redox ContextId 同型先例）；K-316 panic 审计纪律（内部不变量 panic vs 用户可达路径；V12-B1 的 17 处审计法）；K-317 行为选择 cfg 禁令与 B-X 收口（仅允许定义 current*/实现点/指令字面量三类）；K-318 [ARCH: ...] 三层标注体系与 MINIX3 BUG 标注纪律（D-60 的注释准确性教训）；K-319 unsafe/SAFETY 注释纪律与 AssumeSyncCell 禁令；K-320 panic-in-drop 防御（KProcess/KPriv 隐式销毁 fail-fast + 测试豁免夹具三层）。
- 不讲什么：BKL 的运行时语义（→20，本篇讲治理机制）；no_std 通用知识（仅内核落地形态）。
- 前置：04、20（语义前置）。
- 后置：无（工程支线）。
- 事实底线：`os/kernel/src/globals.rs`；`os/libs/minix-types/src/types/errno.rs`、`types/proc_nr.rs`；todo §0（审查基线表）、§1（A1/A2/B1/D1/D2/G1 行）、§0.6（B-X）；`panic-in-drop.md`（吸收后归档）。
- 知识点清单：K-310~K-320（全部新增；锚点如上）。验收：读者能回答"新增一个内核全局静态要走什么流程""为什么 Errno 不允许本地再定义""哪些 cfg 用法合法"三个问题。

---

## 6. 变更表

| 操作编号 | 操作类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|---|---|---|---|---|---|---|
| OP-01 | 重排 | 旧 09（vm-boot-protocol） | 12（原 10 之后） | 旧 09 头部自声明"前置 10"，构成前向引用（硬标准 5.1-1 违例） | K-125~129 | 存量整体后移 |
| OP-02 | 重排 | 旧 13（syscall-dispatch） | 18（trap 入口之后） | 分派的入口机制（KERNEL_CALL 归一）在旧 14，旧 13 前向引用旧 14；旧 13 权限门前向引用旧 22 | K-172~179 | 存量后移 |
| OP-03 | 拆分 | 旧 01（2019 行） | 01 + 02 | 篇幅超软上限；固件协议与内存/分页是两个可独立阅读的语义单元（用户约束 0.1-1） | K-010~017 / K-020~027 | 前半→01，后半→02；add_memmap 语义归 02、调用时机归 09 |
| OP-04 | 拆分 | 旧 05（2263 行） | 06 + 17（部分） | 超软上限；启动期初始化与运行期投递是两个语义单元 | K-060~067 / K-166~171 | 旧 05 的运行时 IRQ 内容并入 17；新增 D-59/D-61 知识点 |
| OP-05 | 拆分+重写 | 旧 16（1298 行） | 10 + 20 | 主文写于"单核占位"时代已过时（S-3d~S-12 已落地）；启动与运行时是两个语义单元 | K-110~117 / K-190~199 | 附录 A 历史包袱删除（已被实现取代，删除理由见越界表） |
| OP-06 | 拆分 | 旧 14 | 13 + 16 + 17 | 入口机制/异常处置/IRQ 投递三个语义单元；消除旧 13→旧 14 的前向依赖 | K-130~139 / K-160~165 / K-166~171 | 三向分配，互相引用 |
| OP-07 | 整编（不拆） | 旧 06（1375 行） | 07 | **遵守 06-todo.md §4.1/§4.2 既有裁决**：拆分已否决，存储与流程一体是本篇存在理由；吸收旧 99 与旧 22 结构半后约 1400 行，在软上限内 | K-070~086 | 存量整编；99 全量并入后归档 |
| OP-08 | 合并 | 旧 99（55 行）+旧 22 Ch1 结构半 | 07 | 99 过薄（55 行）不满足单篇单语义的信息密度；priv 结构与 proc 结构同为"存储模型" | K-071/K-072/K-073/K-086 | 合并入 07；99 归档 |
| OP-09 | 重构 | 旧 25 | 26 | 四个无主讲述点调用族（abort/schedule/setgrant/mcontext）落位；修正旧 00 导航表的 do_unused.c 事实错误（§3.6 E-1） | K-236~247 | 存量+新增落位 |
| OP-10 | 重构 | 旧 28 | 33 | WONTFIX 存档升格为"两条路线对照"——kerninfo 现役实现为主体 | K-282~288 | 存量压缩+新增主体 |
| OP-11 | 新建 | —（散于 18-trap-bridge-design.md、edge E1/E8） | 13 | trap 桥双腿无正式编号文档；设计文是裁决书形态非教学文档 | K-130~139 | 吸收后 18-trap-bridge-design.md 归档 |
| OP-12 | 新建 | —（qemu-tests/、CI、config.toml） | 37 | 缺口 G-1：测试基建零覆盖 | K-302~309 | 来源=C 制品与工程事实 |
| OP-13 | 新建 | —（globals.rs 等）+吸收 panic-in-drop.md | 38 | 缺口 G-7：工程纪律零覆盖；00 §1.5 越界内容迁入 | K-310~320 | 来源=代码与 todo 事实；panic-in-drop.md 归档 |
| OP-14 | 扩充 | 旧 27 | 32 | 关停链（main.c:351-398）无主讲述点；S-11 落地 | K-279~281 | 存量+新增 |
| OP-15 | 归档 | 07-paging_init_gpt.md、smp_gpt{,_v2,_v3}.md | （无新篇） | GPT 对话草稿，结论已被 smp_todo/旧 16 增补/新 10 取代 | — | B 相移入 draft/ 或加草稿头标注，不删 |
| OP-16 | 扩充 | 旧 07 | 08 | dm_coverage/DM 窗口运行时模型为新增事实（E8） | K-093 | 存量+新增 |
| OP-17 | 扩充 | 旧 15 | 19 | 记账链完整化（I-16）与单源裁决（I-15） | K-184/K-187 | 存量+新增 |
| OP-18 | 扩充 | 旧 17 | 21 | fork msgaddr wire 修复（E-FORKMSG） | K-200 半 | 存量+新增 |
| OP-19 | 扩充 | 旧 18 | 22 | E-MIBGRANT 端点常量缺口登记、IoBatchBuf 事实 | K-211/K-215 | 存量+新增 |
| OP-20 | 保持 | 旧 02/03/04/08/10/11/12/19/20/21/23/24/29/30/31/32 | 03/04/05/09/11/14/15/23/24/25/28/29/34/35/30/31 | 无序位问题、篇幅合格、结构符合单篇单语义——整编即可（事实刷新随 B 相） | 各篇编号段 | 存量整编 |

---

## 7. 缺漏新篇（非 C 主题清单逐项落实）

| 主题 | 落实 | 验收标准 |
|---|---|---|
| 链接与加载 | 03（主） | 链接脚本符号→加载行为→跳转全链可复述 |
| 镜像与内存布局 | 02+03 | 物理布局与虚布局两张图齐全 |
| 汇编入口与陷阱进入 | 13（运行期）+03（引导入口） | 五入口锚点表 + 推栈契约 |
| 启动装配 | 01 | 三引导路径 × KernelInfo 契约 |
| 构建与工具链 | 37 | workspace/targets/xtask/I-1 登记齐全 |
| 跨模块接口与线格式 | 01（KernelInfo）/12（SYS_VMCTL）/13（trap 约定）/15+33（kerninfo）/09（handoff） | 每条线格式有 wire 锚点 |
| 错误路径 | 38（单源纪律）+各调用篇 errno 表 | 每族调用篇含错误码对照表验收项 |
| 关闭与退出 | 32 | 关停链全序图 |
| 并发与同步 | 20（语义）+38（治理） | BKL 释放窗口表 + 审批清单流程 |
| 测试基建 | 37 | 机制→测试内核映射表 |

**明确不做并给理由（承接 todo W/D 系，写入对应篇的"不讲什么"）**：W-1 watchdog 全族（36 存档）；W-2 usermapped（33 存档）；W-3 IPC hook 族（34 排除声明）；W-4/W-5 NMI profiling（35 排除声明）；W-6 ClearMapCache（09 引用 C 同款事实）；W-7 kmess 双缓冲（32 的 [ARCH] 演进）；I-1/I-5/T-10 诚实 DEFERRED（37/05 登记）。

---

## 8. 锚点迁移与断链成本

### 8.1 断链成本摘要

- **目录内部**：编号文档名引用 1093 处（grep 实证）。热点：16（75）、14（67）、11（60）、25（59）、06（58）。
- **跨 stage 引入**：303 处指向本目录。热点：00（35）、06（33）、18-syscall-copy（29）、19（19）、11（19）。
- **代码注释**：约 200 处（os/kernel、os/arch、os/boot-shim、os/libs/minix-platform）。热点：06（58）、05（17）、16（15）、04（15）、02（15）。
- **总量**：约 1600 处引用受编号变化影响。**热点结论**：06（被引 91+）、11（70+）、14（67+）、16（75+）是四大热点——恰好与"拆分/整编"操作重合，B 相必须优先处理。
- **批量修改方式建议**：① 旧文档整体移入 `archive/doc-rerank-v1/`（归档不删）；② 引用迁移按"旧文件名→新文件名"的确定性映射表（8.2）做 `git grep -l` 驱动的逐文件替换；③ 拆分目标（旧 01/05/14/16）的引用无法机械映射到"单文件"，需按 8.2 的"节级去向"人工分流——B 相为这四篇额外产出节级锚点表（本蓝图 8.3 已给节级骨架）；④ 代码注释引用随批次修复（fix-guard 纪律，一次一处），不阻塞文档重建。

### 8.2 引用迁移表（文件级，凡文件名变化者）

| 旧引用 | 新目标 | 迁移类型 | 验证方式 |
|---|---|---|---|
| 00-kernel-overview.md | 00-kernel-overview.md（重写） | 改写 | grep 旧文件名=0；读新 00 导航表对表 |
| 01-boot-shim-bootstrap.md | 01-boot-protocols-and-shim.md / 02-boot-memmap-and-paging.md | 拆分 | grep 旧名=0；抽查 10 处分流正确性 |
| 02-higher-half-kernel.md | 03-link-load-jump.md | 改名 | git grep 全仓替换 |
| 03-kmain-cstart.md | 04-kmain-cstart.md | 改名 | 同上 |
| 04-platform-discovery.md | 05-platform-discovery.md | 改名 | 同上 |
| 05-clock-interrupt-init.md | 06-clock-intr-init.md（IRQ 运行时→17） | 拆分 | 同上+抽查 |
| 06-proc-init-boot-proc.md | 07-proc-init-boot-proc.md | 改名（代码注释热点 58 处） | grep 旧名在 os/ =0 |
| 07-cross-space-init.md | 08-cross-space.md | 改名 | 同上 |
| 08-system-init-boot-finish.md | 09-system-init-boot-finish.md | 改名 | 同上 |
| 09-vm-boot-protocol.md | 12-vm-boot-protocol.md | 改名 | 同上 |
| 10-switch-to-user.md | 11-switch-to-user.md | 改名 | 同上 |
| 11-scheduling-primitives.md | 14-scheduling.md | 改名 | 同上 |
| 12-ipc-core.md | 15-ipc-core.md | 改名 | 同上 |
| 13-syscall-dispatch.md | 18-syscall-dispatch.md | 改名 | 同上 |
| 14-exception-interrupt.md | 13/16/17 三向 | 拆分 | 抽查分流 |
| 15-clock-timer.md | 19-clock-timer.md | 改名 | 同上 |
| 16-smp.md | 10/20 两向 | 拆分 | 抽查分流 |
| 17-syscall-process.md | 21-syscall-process.md | 改名 | 同上 |
| 18-syscall-copy.md | 22-syscall-copy.md | 改名 | 同上 |
| 19-syscall-signal.md | 23-syscall-signal.md | 改名 | 同上 |
| 20-syscall-device.md | 24-syscall-device.md | 改名 | 同上 |
| 21-syscall-clock.md | 25-syscall-clock.md | 改名 | 同上 |
| 22-privilege.md | 07（结构半）/27（运行时半） | 拆分 | 抽查分流 |
| 23-ipc-filter.md | 28-ipc-filter.md | 改名 | 同上 |
| 24-cross-space-runtime.md | 29-cross-space-runtime.md | 改名 | 同上 |
| 25-misc-unported.md | 26-syscall-misc.md | 重构改名 | 同上 |
| 26-watchdog.md | 36-watchdog.md | 改名 | 同上 |
| 27-kernel-utility.md | 32-utility-console-shutdown.md | 扩充改名 | 同上 |
| 28-usermapped-data.md | 33-kerninfo-user-data.md | 重构改名 | 同上 |
| 29-kernel-debug.md | 34-kernel-debug.md | 改名 | 同上 |
| 30-kernel-profile.md | 35-kernel-profile.md | 改名 | 同上 |
| 31-fpu-context-switching.md | 30-fpu.md | 改名（编号前移） | 同上 |
| 32-stack-tracing.md | 31-stack-tracing.md | 改名（编号前移） | 同上 |
| 99-global-concepts.md | 07（吸收） | 合并归档 | grep 旧名=0 |
| 18-trap-bridge-design.md | 13（吸收） | 合并归档 | grep 旧名在 edge_todo.md 保留（历史记录允许指向归档路径） |
| panic-in-drop.md | 38（吸收） | 合并归档 | 同上 |

### 8.3 锚点迁移表（节级，拆分四篇的骨架去向）

| 旧位置（文档与节） | 旧内容（一句话） | 新位置 | 迁移类型 | 断链风险备注 |
|---|---|---|---|---|
| 旧 01 §1–§2.0.3 | 启动链路概述、C/GRUB/multiboot、boot.cfg | 01 §1–2 | 原样搬移 | 低 |
| 旧 01 §2.1–§2.4（memmap/cut/add/页表/恒等映射/开分页） | pre_init 的内存与分页 | 02 §1–3 | 改写 | 中——外部引用多指向"旧 01 §2.2"，需节级重定向 |
| 旧 01 §3–§4 | Rust 设计与实现详解（boot-shim） | 01 §3–4 | 原样搬移 | 低 |
| 旧 01 §5–§6、附录 A | 测试要点、失败诊断、UEFI 指南 | 01 §5–6（附录压缩 ≤100 行） | 改写 | 低 |
| 旧 05 §1.0（同步/异步模型） | 异常 vs 中断概念 | 06 §1 与 16 §1 各取一半（概念只讲一次，主位 06，16 引用） | 拆分改写 | 中——两篇都曾被引用 |
| 旧 05 §2（init_clock/intr_init/arch_init C 分析） | 启动期初始化 | 06 §2 | 原样搬移+D-59 时序原则增补 | 中 |
| 旧 05 §3–§5（三架构抽象/实现/测试） | 控制器抽象 | 06 §3–5；运行时投递相关小节→17 | 拆分 | 高——需逐小节判定 |
| 旧 14 Ch1–Ch2（异常/中断概念+帧格式+入口） | 概念与机制 | 13（帧/入口）+16（异常处置）+17（IRQ 投递）三向 | 拆分改写 | 高（被引 67 次） |
| 旧 14 Ch3–Ch5（Rust 设计/实现/测试） | dispatcher/page_fault/irq_manager | 同上三向 | 拆分 | 高 |
| 旧 16 §1–§8（BKL/per-CPU/IPI 概念与设计） | 运行时语义 | 20 §1–4（K-110 概念首现位留 10 一句） | 改写 | 高（被引 75 次） |
| 旧 16 §9–§10（S-3b/S-3c 增补） | AP 梯子与固件 ABI 修复 | 10 §2–3 | 原样搬移+整合 | 中 |
| 旧 16 附录 A | 06 迁入的 SMP 预留设计正文 | 删除 | 删除（已被 S 系列实现文档取代） | 低（仅历史引用） |
| 旧 22 Ch1（priv 结构概念/字段） | 结构语义 | 07（K-073/K-074 位） | 搬移合并 | 中 |
| 旧 22 Ch2+（privctl/priv_add_*/模板运行时） | 运行时 | 27 | 原样搬移 | 中 |
| 旧 99 全部 | RTS 表/endpoint/常量 | 07 | 合并 | 中（06/11 引用它） |
| 旧 25 §（getinfo/trace/update/sprofile） | 四主力族 | 26 §1–4 | 原样搬移+勘误 | 低 |
| 旧 28 §1–§3（.usermapped 机制） | C 机制 | 33 §1（存档语义，压缩） | 改写压缩 | 低 |
| 旧 28 §3.7 增补+§4 | W-2 裁决 | 33 §2–5（kerninfo 主体扩写） | 扩写 | 低 |

### 8.4 环节外引用（登记，B 相随批处理）

- 02-stage-vm 的 `07-pagetable-struct.md` §3.2 与 `08-pagetable-ops.md` §3.4 被旧 07 引用（redesign 依据）——保留指向（对方文件未改名），但引用文字改为绝对路径。
- master-plan/README.md 引用 `01-stage-kernel/09-vm-boot-protocol.md` 与 `06-proc-init-boot-proc.md`（:58-59 行）——B 相更新为 12/07 新名。
- edge_todo.md E1 条目引用 `01-stage-kernel/18-trap-bridge-design.md`——归档后 edge 的历史记录允许保留原路径加"(已归档)"注；新引用指向 13。

---

## 9. 验证与自检门

### 9.1 四种机械检查结果

1. **前向引用扫描（G3）**：按新目录顺序逐篇核对 §5 契约"前置"字段——39 篇全部只指向更小编号；跨部分依赖均通过"概念首现+回指"或"不讲什么+去向"表达，无内容性前向依赖。**PASS**。明细：原旧序两处前向引用（旧 09→10、旧 13→14/22）已被 OP-01/OP-02 修复；新序中 24 的前置曾涉 CHECK_* 权限（旧 22），已改为"字段引用 07+校验序本篇自讲"消解。
2. **依赖关系图无环（G4）**：由 39 篇前置关系构图——链式主干 00→01→02→03→04→{05→06→09→10→11→12}→07→08 与运行期链 13→14→15→{16,17}→18→19→20，集合篇 21–26，控制面 27→28、29，支线 30–38。全图无环（每条边均由小编号指向关系可机械验证）。**PASS**。
3. **覆盖率检查（G5）**：知识点池 285 条（§2.1 归属表与 §5 契约逐条实点）逐条有去向——每条出现在且仅在一个新篇契约的"知识点清单"（§5），无"待定"；明确删除项 1 条（旧 16 附录 A 历史包袱，删除理由：内容已被 smp_todo §21–26 与新 10/20 的实现事实取代，保留即误导）。纯新增 62 条全部带证据锚点。**PASS**。
4. **断链成本统计（G8 前置）**：见 §8.1——内部 1093 + 跨 stage 303 + 代码注释约 200 ≈ 1600 处；热点 06/11/14/16 与操作重合已标记。**完成**。

### 9.2 自检门逐门结果

| 门 | 结果 | 证据 |
|---|---|---|
| G1 C 真序可核对（抽十条） | **PASS** | 抽 A02（pg_utils.c:204 vm_enable_paging——函数清单 :204 实证）、A05（main.c:418 init_clock——全文读实证）、A06（main.c:149 BKL_LOCK——实证）、A09（main.c:253 RTS_SET(NO_PRIV\|NO_QUANTUM)——实证）、A16（main.c:64-66 RTS_UNSET 循环——实证）、A17（proc.c:299 switch_to_user+:473 NOT_REACHABLE——函数地图 :299 实证，:473 在旧 10 与 proc.c 尾部锚点交叉印证）、B02（mpx.S:293 call do_ipc——grep 实证）、B03（mpx.S:332 call kernel_call——实证；system.c:136 函数地图实证）、B05（mpx.S:545 copr_not_available_handler——实证；proc.c:1922 函数地图实证）、B07（main.c:368 minix_shutdown——全文读实证）。十条全中。 |
| G2 池完整性：每个 C 文件/非 C 制品有归属或排除理由 | **PASS** | 顶层 22 个 .c：main→09、table→07、proc→07/14/15/16、system→18/27/28、clock→06/19、interrupt→17、smp→10/20、debug→34、utility→32、profile→35、watchdog→36、usermapped_data→33、cpulocals→10/20、ipc.h→15、其余 .h→07/各篇。38 个 do_*.c：34 族归 21–26（26 含四缺口族+do_vmctl→12+do_diagctl→26/32），逐一可索引。arch/i386：head.S→03、mpx.S→13、kernel.lds→03、pre_init/pg_utils→01/02、protect→04/07、memory→08/22/29、exception→16/31、i8259→06、arch_clock→19、arch_smp→10、arch_system→04/05/30、arch_watchdog→36、arch_do_vmctl→12、arch_reset→32、do_sdevio/iopenable/readbios→24、acpi→05、apic→06/17、klib/io_*/debugreg→13/22（辅助原语）、oxpcie/direct_tty→06（早期控制台）、breakpoints→34（W-3 关联）。非 C 制品十项：§3.5 表全答。 |
| G3 前向引用为零 | **PASS** | 见 9.1-1。 |
| G4 依赖图无环 | **PASS** | 见 9.1-2。 |
| G5 覆盖率 100% | **PASS** | 见 9.1-3；删除项单独列出（1 条，带理由）。 |
| G6 拆合新建的存量去向/新增来源（抽十处） | **PASS** | 抽 OP-03（旧 01 前后半→01/02，K-010~027 逐条去向在两篇契约）、OP-04（旧 05→06/17，K-060~067/K-166~171）、OP-06（旧 14 三向，K-130~171）、OP-07（不拆理由引用 06-todo §4 裁决）、OP-08（99→07 四条 K 号）、OP-11（新建 13 来源=设计文+edge 提交号）、OP-12（新建 37 来源=目录/脚本/workflow 清单）、OP-13（新建 38 来源=globals.rs/errno.rs/todo 表）、OP-10（28→33 的新增半锚点 kerninfo.rs）、OP-09（25→26 的新增四族锚点=do_*.c 文件名）。十处全部双方向清晰。 |
| G7 每篇契约七要素齐全 | **PASS** | §5 共 39 篇契约，逐篇含：一句话定位/讲什么/不讲什么（含去向）/前置/后置/事实底线/知识点清单+验收标准。篇幅所限，17 篇"保持/改名/整编"类契约的知识点清单以编号段+逐条紧凑行给出（K 号、名称、类型、锚点、归属理由、来源六列俱全），信息无缺省。 |
| G8 锚点迁移表覆盖所有变化文档的每一节 | **PASS（骨架级）** | §8.2 覆盖全部 34+99+3 旁支的文件级映射；§8.3 覆盖 4 篇拆分文档的节级骨架（旧 01/05/14/16/22/28/99 的关键节去向）。剩余"整编类"文档无节级搬移（整篇保留），无需节级表。 |
| G9 事实断言带锚点/推测标注 | **PASS（1 处待验证标注 + 6 处勘误实证）** | 全文事实断言均带文件:行或文件名锚点。待验证标注 1 处：§8.1 引用计数的尾部尾差（约 200 处为前 20 目标求和的近似，机械可复算）。原"K-101 行号锚待复核"一项已在 truth-source 核实中升级为结论：旧 08 六处 lib.rs 行号锚全部实测漂移（§3.6 E-4），佐证"符号 + 实测行号双锚"纪律的必要性。旧文档错误抽样 6 条全部代码实证于 §3.6（E-1~E-6）。 |

### 9.3 结论与待用户裁决的问题

**结论**：本蓝图自检九门全过，可作为多 AI 汇总收敛的输入。核心主张三条：① 现行目录的硬伤是**三处前向引用**（09→10、13→14、13→22）与**两篇超长**（01/05）加**已核实的旧文档错误**（§3.6 六条抽样，含旧 00 导航表的 do_unused.c 事实错误与旧 08 六处行号锚全数漂移）；② 最大的知识缺口不在 C 语义而在**工程事实层**——trap 桥、SMP 真链、kerninfo、测试基建、Rust 纪律五块代码侧已落地而文档缺位或过时的内容，占新增篇目的大部分；③ 旧 06 的"存储与流程一体"裁决正确，重建必须延续而非推翻。

**待用户裁决**：
1. **旧 14/16 的拆分深度**：本蓝图拆旧 14 为三篇（13/16/17）、拆旧 16 为两篇（10/20）。若汇总时多数蓝图倾向少拆，最小妥协方案是旧 14 拆二（入口+处置合并）——但旧 16 必须拆或重写，因"单核占位"叙述已与实现冲突（事实问题，非偏好）。
2. **旁支文档的归档位置**：07-paging_init_gpt.md、smp_gpt*.md 建议移入目录内 `draft/` 子目录（本仓库无此惯例，备选是原地加"草稿-已被取代"头标注）。
3. **编号稳定性换血成本**：本蓝图接受约 1600 处引用迁移的一次性成本（§8.1 有批量方案）。若用户认为成本过高，降级方案是"只修正序与事实、不重编号"——但该方案无法修复 09→10 前向引用（除非接受序差表补偿），需要用户明确取舍。

### 9.4 范围外发现（末尾登记）

- `minix3/minix/kernel/extract-{errno,mfield,mtype}.sh` 三个提取脚本属上游工具链，不属本 stage 文档主题（已在 G2 登记排除）。
- `os/kernel/src/test_helpers.rs`、`os/kernel/tests/` 属 37 篇素材，B 相写作时纳入。
- minix-rt crt0/诞生链的**用户态半**（rt_birth 六阶段）属 14-stage-runtime；本目录 13 篇只在 trap 约定处登记接口边界（已在 K-139/K-288 声明）。
