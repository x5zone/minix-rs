# 01-stage-kernel 文档重建蓝图（mimo）

```text
your_name(AI agent name) = mimo
target_dir(关注的工作目录) = 01-stage-kernel
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 target_dir/doc_rerank_mimo.md，不改任何正文。
       只读 C 代码不够，还必须读：本目录全部文档的头部声明、os/ 目录对应入口、
       00-*-overview.md 的导航表。
约束 = 不得引用 .design/ 与 tmp_design_and_todo/；任何落盘产物必须带
       _<your_name> 后缀；不得读取或照抄其它 AI 的 doc_rerank_* 产物。
```

> 执行者声明：本报告未读取 `doc_rerank_HY4.md` / `doc_rerank_deepseek.md` /
> `doc_rerank_glm.md` / `doc_rerank_muse.md` / `doc_rerank_qwen.md` 的任何内容；
> 未引用 `.design/` 与 `tmp_design_and_todo/` 下任何内容。

---

## 0. 元数据

- 执行者：mimo；日期：2026-09-23；目标目录：`rewrite-notes/01-stage-kernel/`
- 仓库根：`/home/xzhao/github/minix-rs`；当前提交：`28a916f86f1231c631aed91ef93bc3e14fcfc8fa`（后续复跑 `git log -1` 时以 `45c0682f92569d65e26721edcdfdc409a1d0977d` 亦可见——工作区在取证期间有新提交，本报告锚点均为文件:符号级，不依赖行号快照）

### 0.1 审查范围

**算文档（编号文档 + 99 总概念篇）**：`00-kernel-overview.md` ~ `33-syscall-caller-api.md` 中的正式编号篇（共 35 个文件，含 `99-global-concepts.md`），以及与编号冲突的 `06-todo.md`、`07-paging_init_gpt.md`、`18-trap-bridge-design.md`（重建时分别归档/吸收/并入）。

**算参考材料（不进入新目录编号，但内容可吸收）**：`plan.md`（本目录不存在——以 `todo.md` + `00-kernel-overview.md` 导航表替代）、`todo.md`、`06-todo.md`、`checklist.md`、`endpoint_todo.md`、`panic-in-drop.md`、`smp_todo.md`、`smp_gpt.md` / `smp_gpt_v2.md` / `smp_gpt_v3.md`、`07-paging_init_gpt.md`。

**范围外**：`.design/`、`tmp_design_and_todo/`、`.review/` 中间产物、其它 AI 的 `doc_rerank_*`、`book/`、`AI-chats/` 对话原文（仅允许把 `panic-in-drop.md` 指向的已实施结论当事实线索，锚点以代码为准）。

**范围外发现（单列，不计入本蓝图交付）**：
1. 用户侧 trap 体（`os/libs/minix-sys/src/arch_trap.rs`）的文档归属地是 `14-stage-runtime`，本 stage 只拥有内核半边（入口 stub + 分派体）——见 edge E1 条目原文；重建后 `16-exception-interrupt` 契约须写明这条边界。
2. `minix3/sys/arch/i386/stand/`（bootxx/MBR 引导器链）位于内核源码树之外的引导器目录，本 stage 仅在 `01-boot-shim-bootstrap` 的启动链上游边界提及，不展开。
3. `minix-types` 的 `SYS_*` 调用号常量缺位（edge `E-MINTYPES-SYS`）——内核侧线格式讲在 `13-syscall-dispatch`，共享镜像归属跨 stage edge，不在本蓝图新建篇。

### 0.2 读取清单

| 类别 | 读取内容 |
|------|---------|
| 文档 | 目录全部 47 个 `.md` 的头部声明 + 编号文档正文按需精读（导航表、前置/边界、章节骨架全量 `rg '^#{1,3} '`）；`00-kernel-overview.md` §3 导航表全读 |
| C 源码 | `minix3/minix/kernel/*.c/*.h`（20 个 .c 共 6908 行）、`system/do_*.c`（38 个共 4248 行）、`arch/i386/`（40 文件 9028 行）、`arch/earm/`、`include/minix/{callnr,com,priv,ipc,const}.h`；关键函数体实读：`main.c`（kmain/cstart/bsp_finish/prepare_shutdown）、`proc.c`（proc_init/switch_to_user/do_ipc）、`system.c`（system_init/kernel_call_dispatch）、`clock.c`、`interrupt.c`、`table.c`、`protect.c`、`pre_init.c`、`pg_utils.c`、`memory.c`、`arch_system.c`、`arch_smp.c`、`do_schedule.c`、`do_setgrant.c`、`do_abort.c`、`do_mcontext.c`、`breakpoints.c`、`direct_tty_utils.c`、`arch_reset.c` |
| 非 C 制品 | 链接脚本 `kernel.lds`/`earm/kernel.lds`/`os/kernel/src/arch/*/link.ld`/`os/kernel-image/*.ld`；汇编 `head.S`/`mpx.S`/`usermapped_glo_ipc.S`/`trampoline.S`/`apic_asm.S`/`io_*.S`/`klib.S`/`debugreg.S`；引导 `minix3/etc/boot.cfg.default`、`multiboot.h`、bootxx 链；libc 用户侧 `arch/{arm,i386}/sys/_do_kernel_call_intr.S`；构建 `os/xtask/src/{image,qemu,manifest}.rs`、`os/kernel-image/{build.rs,check-layout.sh}`、`minix3/minix/kernel/{Makefile.inc,extract-errno.sh,extract-mtype.sh,extract-mfield.sh}`、`procoffsets.cf`；测试 `os/qemu-tests/`（run_all/test-user-trap/test-rt-birth 等 18 个脚本 + `test-kernels/`）、`minix3/tests/kernel/` |
| 边界材料 | `00-master-plan/README.md`（阶段划分与启动因果链）、`edge_todo.md`（E1/E2/E8/E-KERNINFO/E-MINTYPES-SYS/E-SCHEDSMP 等 30+ 条）、`01-stage-kernel/todo.md`（V13 权威清单，含 I-14「C 层重编号不做——风险>收益」历史裁决）、`06-todo.md`（写法范例：新文档契约体例） |
| 前一 stage | 无——本目录是 stage 链第一站（`00-master-plan/README.md` 表中 01 位于最前）。起点声明 = `00-kernel-overview` 面向新读者，不假设任何前 stage 知识 |
| Rust 入口 | `os/kernel/src/`（lib.rs 的 arch_boot/kmain/T0-T6/init_*、syscall*.rs、proc/proc_table/sched/ipc/smp/clock/cross_space/vm/grant/irq_manager/misc/stacktrace/kerninfo 等 38 文件）、`os/arch/src/`（arch/ + x86_64/arm64/riscv64，trap_entry/trap_return/fpu/paging 等）、`os/boot-shim/src/`（uefi_helpers/opensbi_helpers/loader/main）、`os/plat/`、`os/libs/minix-boot/kernel_info.rs`、`os/libs/minix-platform/` |
| 写法范例 | `06-todo.md`：学「讲什么/不讲什么/下放给谁/验收」契约体例，不搬其结论 |

### 0.3 使用的命令与关键输出（证据摘录）

```text
$ wc -l 01-stage-kernel/*.md | sort -n          # 编号文档 55~2263 行，05 最大 2263
$ rg -n "^#{1,3} " <每篇编号文档>               # 全量章节骨架（用于知识点池）
$ rg -o '\[[0-9]{2}-[a-z0-9_-]+\.md\]' -g '!doc_rerank*'   # 339 处文档间引用
$ rg -o '[0-9]{2}-[a-z0-9_-]+\.md' os/          # 865 处命中，其中本 stage 文档 226 处 / 分布于 os 多文件（热点 proc_table.rs:11, proc.rs:11, kpriv.rs:10, vm.rs:5+4, smp.rs:5）
$ rg -o '01-stage-kernel/[0-9]{2}-.*\.md' notes/ --glob '!**/doc_rerank*'  # 219 处跨 stage 引用
$ rg -c '03-stage-kernel' os/ notes/ prompt/ .claude/   # 23 个文件残留旧目录名路径
$ rg '21-clock-device|01-04-test-audit' notes/          # 既有断链 5+3 处（重建前就坏）
$ rg -n '02-stage-vm/draft' 00-kernel-overview.md        # 00 的 §6/§7 指向已不存在的 draft/ 子目录（断链）
$ sed -n '115,340p' minix3/minix/kernel/main.c           # kmain 真序实读
$ sed -n '299,477p' proc.c                               # switch_to_user 真序实读
$ sed -n '168,270p' system.c                             # system_init call_vec 注册全表
$ wc -l minix3/minix/kernel/*.c system/do_*.c arch/i386/*  # C 清单与行数
$ 前向引用扫描（旧目录）                                  # 旧目录存在前向引用边：09→10、11→10、13→19、14→31、15→16/21、16→17/22、17→19/22、18→20/24、19→22/32、20→22、21→22、22→23/25、23→24 等约 30 条
```

---

## 1. C 真序

### 1.1 阶段类型判定

本 stage 同时具备三种特征，按以下分工处理（依据第九部分）：

- **Part I（01–08）按启动链型**：从固件入口到 `bsp_finish_booting` 是线性调用链，讲述顺序与运行时序一致（序差极小，见 1.3）。
- **Part II（09–19）按汇聚点型**：所有运行时机制都汇聚于 `switch_to_user()` 循环；按「统一框架 + 汇聚点 + 触发时机」组织，不伪造调用线性序。
- **Part III（20–25）按集合型（并行体）**：几十个系统调用按主题分组，框架篇（13-syscall-dispatch）先讲分派/线格式/权限接入，组内代表性成员精讲 + 差异表收束。
- **Part IV（26–34）支线，可跳读**：调试、剖析、WONTFIX、构建测试基建。

判定理由：`minix3/minix/kernel/main.c:kmain`（L115）到 `bsp_finish_booting`（L38）是唯一入口链；`switch_to_user`（`proc.c:299`）函数体为无限循环且 `NOT_REACHABLE`，三条进入路径一个出口（10-switch-to-user §1.2 旧文所述，经 `proc.c:360-430` misc 标志循环复核成立）；`system_init` 的 `call_vec[]` 注册表（`system.c:168-270`）与 38 个 `do_*.c` 构成典型系统调用集合。

### 1.2 真序表（启动链段）

| # | 动作 | C 函数与文件锚点 | 说明 |
|---|------|------------------|------|
| 1 | GRUB 跳入内核入口，建栈，传 magic/mbi | `minix3/minix/kernel/arch/i386/head.S:multiboot_init（L68，工具生成）`，push eax/ebx :75-76 | 无分页；`ENTRY(__k_unpaged_MINIX)`（`kernel.lds:L3`） |
| 2 | 解析 multiboot 参数 → `kinfo`；建内核映射 | `pre_init.c:pre_init（L217，工具生成）` → `get_parameters`（:94）→ `pg_mapkernel`（:232 调用，定义 `pg_utils.c:186`） | 内存 map、module 表拷入 `kinfo`；`freepde_start` 记录 |
| 3 | 开分页（恒等 + 高半） | `pg_utils.c:vm_enable_paging（L204，工具生成）` | 调用点在 head.S→kmain 过渡（具体 call site 于 B 相写作时以 `head.S` 全文核定——本报告标**待验证**） |
| 4 | 进入 C 主入口 | `main.c:kmain（L115，工具生成）`：bss 自检、`memcpy(&kinfo,…)`、`kernel_may_alloc=1`、拷 `boot_procs`、`cstart()`（:147） | |
| 5 | 平台/保护/时钟/中断初始化 | `main.c:cstart（L403，工具生成）`：`prot_init` → `env_get` 解析 → `init_clock` → `intr_init(0)`（:475 区）→ `arch_init`（:477） | `prot_init`：`protect.c:321`（`init_codeseg` :344 起，GDT/IDT/TSS）；`arch_init`：`arch_system.c:246`（tss/ser/acpi/apic/`cut_memmap`） |
| 6 | 拿 BKL，初始化进程/权限/过滤池 | `main.c` cstart 返回后 `BKL_LOCK`（:148）→ `proc_init`（`proc.c:119`）→ `IPCF_POOL_INIT` | `proc_init` 清表、`_ENDPOINT(0,p_nr)`、idle 结构；BKL 概念首现于 00，实现于 18 |
| 7 | 遍历 boot_image 装配可调度进程 | `main.c` boot 循环（:157 起）：`get_priv`、`fill_sendto_mask`、`s_k_call_mask`、`arch_boot_proc`（VM ELF 加载经 `memory.c:arch_proc_init（L722，工具生成）`）、非 VM 挂 `RTS_VMINHIBIT|RTS_BOOTINHIBIT`，全体挂 `RTS_PROC_STOP`、清 `RTS_SLOT_FREE` | 仅 kernel task + RS + VM 立即可调度（`iskerneln‖isrootsysn‖VM`，:186）；`boot_image` 表：`table.c:44-64` |
| 8 | 跨空间初始化 | `protect.c:arch_post_init（L370，工具生成）`（ptproc/freepdes 记录）+ `memory.c:memory_init（L707，工具生成）` | 32 位临时窗口；Rust 对应 direct_map（07 篇） |
| 9 | 注册系统调用表 + 回收 bootstrap 内存 | `system.c:system_init（L168，工具生成）`：IRQ hooks 清、alarm timer init、`call_vec[]` 全注册（L191-270 map 宏 58 项）→ `add_memmap`（`pg_utils.c:86`） | Rust 对应 `enum Syscall`（细节归 13 篇，08 只留注册事实） |
| 10 | SMP 初始化或单核回退 | `arch_smp.c:smp_init（L289，工具生成）`；失败/禁用 → `bsp_finish_booting` | `main.c:305-324` 分支；AP 握手 `smp.c:wait_for_APs_to_finish_booting` |
| 11 | 启动收尾 | `main.c:bsp_finish_booting（L38，工具生成）`：`cpu_identify`、`vm_running=0`、`bill_ptr/proc_ptr=idle`、`announce`、`RTS_UNSET(PROC_STOP)`、`boot_cpu_init_timer`、`fpu_init`、`kernel_may_alloc=0`、`switch_to_user()`（:110） | 就绪队列解禁、时钟门打开、FPU 初始化 |
| 12 | 进入永不返回的调度循环 | `proc.c:switch_to_user（L299，工具生成）`：`pick_proc`/`idle`/misc 标志循环（MF_KCALL_RESUME→`kernel_call_resume`、MF_DELIVERMSG→`delivermsg`、MF_SC_DEFER→`arch_do_syscall`、MF_SC_TRACE→`cause_sig(SIGTRAP)`）→ 恢复上下文交还用户 | 循环体即 Part II 各机制的汇聚点；教学序置于 15（序差 #7） |

### 1.3 真序表（运行时循环段，分四路）

| # | 动作 | 锚点 | 说明 |
|---|------|------|------|
| R1 | VM 被首次调度 → 发起启动协商 | `system/do_vmctl.c:do_vmctl` + `arch/i386/arch_do_vmctl.c`；`os/servers/vm/src/boot.rs` 引用 09 篇旧文 | VMCTL_SETADDRSPACE/KERN_PHYSMAP/MEMREQ 时序；教学序放 19（需先有循环 + 分派，序差 #2） |
| R2 | 用户态 trap 进内核 | IDT 门配置：`protect.c:L147-150`（IPC/KERN_CALL 向量 USER_PRIVILEGE）；trap 体：`usermapped_glo_ipc.S:IPCFUNC（L17，工具生成）`（softint/sysenter/syscall 三套）；异常总入口：`exception.c:exception_handler（L180，工具生成）`；硬件中断：`mpx.S:hwint00（L98，工具生成）` → `interrupt.c:irq_handle（L116，工具生成）` | 用户半边寄存器约定含 E1 trap 桥裁决（`18-trap-bridge-design.md` 已批准，吸收进 16 篇） |
| R3 | 系统调用分派 | `system.c:kernel_call_dispatch（L52，工具生成）`：`s_k_call_mask` 检查（:111）→ `call_vec[call_nr]` → handler → `kernel_call_finish`/`kernel_call_resume`；IPC 腿 `proc.c:do_ipc（L599，工具生成）` | BadCall=`EBADREQUEST`；VMSUSPEND 挂起-恢复协议 |
| R4 | 时钟 tick | `clock.c:timer_int_handler（L70，工具生成）`：记账、quantum 递减、vtimer/sprofile 检查、闹钟到期 → 可能置 `RTS_NO_QUANTUM`/发信号 | 到期动作最终仍经 R6 出口 |
| R5 | IPC 六原语 | `proc.c`：`mini_send`/`mini_receive`/`mini_notify`/`mini_senda`/`delivermsg`/`deadlock`（旧 12 篇锚点 `proc.c:599-1590` 经复核沿用） | 阻塞/唤醒改 RTS + 队列联动 |
| R6 | 一切路径汇回 `switch_to_user` 出口 | `proc.c:299`（见真序 #12） | 三入口一出口 |

**序差表（运行时序事实 → 教学序选择 → 回指补偿）**

| # | 运行时序事实（锚点） | 教学序位置 | 理由 | 回指补偿 |
|---|---------------------|-----------|------|---------|
| 1 | BKL 在 `main.c:148` cstart 后即持有 | 概念首现 `00` §1.4-1.5（完整：是什么/为什么/解决什么），实现 `18-smp` | boot 段 06/08 全程在锁下，必须早知概念 | 18 篇开头回指 00 的概念，只讲实现（RAII/witness/IPI），不重讲概念 |
| 2 | VM 协议是第一个用户态动作（`do_vmctl`，R1） | `19-vm-boot-protocol`（需循环 15 + 分派 13 才能读懂） | 依赖环：读懂 VMCTL 需先有汇聚点与分派框架 | 08 篇结尾一句完成式提及「启动完成后 VM 将经 SYS_VMCTL 协商（机制见 19）」；19 开头回指 08/15 |
| 3 | 中断在 `intr_init(0)`（05 篇阶段）后即可能触发 | 结构配置在 `03`/`05`，handler 行为在 `16-exception` | 先结构后行为，认知递进 | 05 的 `intr_init` 节以完成式写「handler 收到后做什么（概述）」，细节见 16 |
| 4 | `fpu_init` 在 `bsp_finish_booting`（08 篇阶段）调用 | 机制 `29-fpu-context-switching` | lazy/#NM 是独立语义单元 | 08 只留调用事实一句 + 完成式概述 |
| 5 | `smp_init`/AP 握手在 boot（`arch_smp.c:289`） | `18-smp`（Part II 后段） | IPI/亲和性需异常、时钟互操作知识 | 08 §附录保留启动位次事实表（旧 08 附录 A 原样迁） |
| 6 | `cause_sig` 由异常（`ex_data`）、exit、vtimer 多处调用 | 语义核心 `14-signal-core`（早于 15/16/17） | 15 的 MF_SC_TRACE、16 的 ex_data、17 的 vtimer 都依赖它，必须前置 | 各消费篇只引用 14 的行为契约表 |
| 7 | `switch_to_user` 是 08 的最后一个动作（`main.c:110`） | 教学置 `15`（Part II 中段，前置 08/10/11/13/14 齐备后） | 直接在 08 展开循环体将前向引用 10/11/13/14 | 08 结尾以完成式陈述调用事实（是什么/为什么不返回）；15 开头回指「08 的最后一步」并展开循环体 |

---

## 2. 知识点全集

编号规则：`K-NNN` stage 内唯一；对齐键 = 名称 + 锚点。类型：概念(C)/机制(M)/数据结构(D)/接口与协议(I)/约束与不变量(V)/架构演进(A)/工具与工程(T)/测试性质(X)。来源：存量=旧文档（给去向）；新增=覆盖审计发现（给锚点）。

### 2.1 知识点池总表

| 编号 | 名称 | 类型 | 来源 | 现有位置（主讲述点） | 锚点 | 读者收益 | 去向（新目录） |
|------|------|------|------|---------------------|------|---------|---------------|
| K-001 | Kernel 是什么：与 VM/PM/VFS 的执行模型差异 | C | 存量 | 00 §1.1-1.5 | `main.c:kmain`；`table.c:44-51` | 回答「内核为何没有事件循环」 | 00 §1 |
| K-002 | BKL 执行模型：串行化换简单性 + 释放窗口 | M | 存量 | 00 §1.4-1.5、16 §1.1 | `smp.h:SPINLOCK_*`；`main.c:148` BKL_LOCK | 理解一切并发窗口的根源 | 00 §1.4（概念首现）+ 16 §1.1/§4 |
| K-003 | 启动六阶段 A–F 心智模型 | C | 存量 | 00 §2、06 §1.0、07 §1.0 | `main.c:kmain` | 建立全 stage 骨架 | 00 §2 |
| K-004 | 文档导航：阶段分组叙事 | T | 存量 | 00 §3 | — | 知道按什么顺序读 | 00 §3（按新目录重写） |
| K-005 | Kernel vs 用户服务：内核态/用户态、VM 间接操作、中断/进程上下文 | C | 存量 | 00 §4-5 | `glo.h:vm_running` | 设计原则地基 | 00 §4-5 |
| K-006 | boot-shim 引导准备全链路（固件→内存 map→加载→KernelInfo→ExitBootServices→arch_boot） | M | 存量 | 01 §1/§4 | `pre_init.c:pre_init（L217）`；`os/boot-shim/src/uefi_helpers.rs:prepare_boot` | 内核第一段代码的来龙去脉 | 01 |
| K-007 | Multiboot 协议与 kinfo_t：GRUB 如何传参 | I | 存量 | 01 §2.0-2.2 | `multiboot.h`；`pre_init.c:get_parameters（L94）` | 理解启动参数线格式 | 01 §2 |
| K-008 | UEFI/OpenSBI 替代 Multiboot 的架构演进 | A | 存量 | 01 §3.1-3.8 | `os/boot-shim/src/*`；`minix-boot/kernel_info.rs:KernelInfo` | 理解 Rust 侧为何不用 GRUB | 01 §3 |
| K-009 | 页表结构 2 级→4 级、4GB 截断删除等启动期演进 | A | 存量 | 01 §3.3/§3.6 | `os/kernel/src/arch/*/link.ld` | 理解 64 位布局决策 | 01 §3 |
| K-010 | BootShim trait / pt_alloc / 三层 crate 依赖 | D | 存量 | 01 §3.2/§3.7/§4.3-4.5 | `os/boot-shim/src/loader.rs` | 理解 Rust 引导分层 | 01 §4 |
| K-011 | 启动失败诊断：EarlyConsole、panic 路径、QEMU/GDB 调试 | T | 存量 | 01 §6 | `os/plat/src/early_console.rs`；qemu-tests | 会调启动失败 | 01 §6 |
| K-012 | 链接脚本 VMA/LMA/AT()、.usermapped 段、分区布局 | I | 存量 | 02 §2.1、28 §2.2 | `arch/i386/kernel.lds`；`os/kernel/src/arch/*/link.ld` | 理解镜像内存布局 | 02 §2.1 + 26 §2.2 |
| K-013 | head.S 入口：pre_init→kmain 的关键跳转、k_initial_stktop | M | 存量 | 02 §2.2/§2.4 | `head.S:multiboot_init（L68）` | 理解 C 入口前的汇编职责 | 02 §2 |
| K-014 | 高半核原理：切栈+跳高必须成对、物理代码不移动 | V | 存量 | 02 §1 | `HigherHalf::jump_to_kmain`（`os/kernel/src/boot/higher_half.rs`） | 理解为何不能直接 call kmain | 02 §1 |
| K-015 | ELF 加载：段拷贝、BSS 清零、独立 kernel ELF 决策 | M | 存量 | 02 §4.2/§3.2 | `os/boot-shim/src/loader.rs` | 理解内核镜像如何就位 | 02 §4.2 |
| K-016 | HigherHalf trait 三架构实现 | I | 存量 | 02 §4.3 | `os/kernel/src/arch/{x86_64,aarch64,riscv64}/higher_half.rs` | 理解跨架构抽象 | 02 §4.3 |
| K-017 | riscv64 L2 条目覆盖 bug 案例（附录 A） | X | 存量 | 02 附录 A | `os/kernel/src/arch/riscv64/` | 页表覆盖的真实教训 | 02 附录 A |
| K-018 | kmain 入口职责：bss 自检、kinfo 全局拷贝、kernel_may_alloc | M | 存量 | 03 §2.1 | `main.c:kmain（L115）` | 内核 C 第一函数的职责清单 | 03 §2.1 |
| K-019 | 保护结构概念：特权级、跨特权级统一流程、CPU 三问 | C | 存量 | 03 §1 | `protect.c:prot_init（L321）` | 读懂 GDT/IDT/TSS 的地基 | 03 §1 |
| K-020 | prot_init 三架构详解（GDT/IDT/TSS/段寄存器/MSR） | M | 存量 | 03 §2.3-2.6 | `protect.c:init_codeseg（L98）`；`os/arch/src/*/protection.rs` | 保护模式初始化全流程 | 03 §2 |
| K-021 | ProtectionArch/TrapEntryArch 类型系统建模 | A | 存量 | 03 §3-4 | `os/arch/src/arch/protection.rs` | Rust 如何用类型替代运行时标志 | 03 §3-4 |
| K-022 | 内核栈切换威胁模型（附录 A） | V | 存量 | 03 附录 A | `protect.c:tss_init` | 理解为何必须切内核栈 | 03 附录 A |
| K-023 | 硬件发现：硬编码地址不可持续、三架构描述来源 | C | 存量 | 04 §1 | `arch_system.c:arch_init（L246）` acpi_init | 回答「硬件参数从哪来」 | 04 §1 |
| K-024 | PlatformDesc trait 家族 + 实例状态 + AssumeSyncCell 全局存储 | D | 存量 | 04 §3-4 | `os/libs/minix-platform/src/desc.rs`/`global.rs` | 理解三架构统一抽象 | 04 §3-4 |
| K-025 | ACPI/DTB/QemuVirt 兜底三种数据源 | I | 存量 | 04 §2/§4.2 | `os/libs/minix-platform/src/{acpi,device_tree,qemu_virt}.rs` | 理解发现机制分派 | 04 §2/§4 |
| K-026 | KernelInfo 扩展字段与启动时序 T2.5 | I | 存量 | 04 §4.4/§4.6 | `minix-boot/src/kernel_info.rs:KernelInfo` | boot→kernel 契约 | 04 §4 + 01 §4.1（去重：契约在 01，扩展在 04） |
| K-027 | 时钟初始化 init_clock：变量置零、频率来源 | M | 存量 | 05 §2.1 | `clock.c:init_clock（L48）` | 时钟子系统起点 | 05 §2.1 |
| K-028 | 中断控制器初始化 intr_init：8259/GIC/PLIC 三架构 | M | 存量 | 05 §2.2-2.3 | `i8259.c:intr_init（L30）`；`os/plat/src/*/interrupt.rs` | 设备中断如何接入 CPU | 05 §2 |
| K-029 | arch_init：tss/串口/acpi/apic/memmap 裁剪 | M | 存量 | 05 §2.4-2.5 | `arch_system.c:arch_init（L246）` | 架构收尾初始化清单 | 05 §2.4 |
| K-030 | 同步 vs 异步中断模型、为何时钟必须先于 proc_init | C | 存量 | 05 §1.0-1.1 | — | 中断概念地基 | 05 §1 |
| K-031 | ClockArch/InterruptController/TimerIrqGate/ArchInit/EarlyConsole 五 trait 决策族 | A | 存量 | 05 §3 | `os/arch/src/arch/{clock,timer_irq_gate}.rs` | Rust 硬件抽象五件套 | 05 §3 |
| K-032 | ClockState 初始化与 EarlyConsole 实现（05 §4.1/§4.14 部分） | D | 存量 | 05 §4.1/§4.14 | `os/kernel/src/clock.rs:ClockState` | ⚠ 与 15-clock-timer §4.1 重复 | → 15（主讲述点），05 只留 init_clock 调用点 |
| K-033 | 进程本质、CPU 四问、进程表/特权表/RTS 三件套概念 | C | 存量 | 06 §1 | `proc.c:proc_init（L119）` | 进程数据模型地基 | 06 §1 |
| K-034 | 静态定容表的存储约束（编译期数组、EXTERN 模式） | V | 存量 | 06 §2.0 | `table.c` 头注释；`glo.h` | 理解内核为何不用堆 | 06 §2.0 + 99 |
| K-035 | struct proc 字段分组与语义全集 | D | 存量 | 06 §2.1 | `proc.h:struct proc` | 一切机制的数据载体 | 06 §2.1（字段全集表保留） |
| K-036 | struct priv 存储与 KPriv 分组 | D | 存量 | 06 §2.2、22 §1.1/§4 | `priv.h:priv`；`system.c:get_priv（L918）` | ⚠ 06 与 22 重复 | → 09-privilege 主讲；06 只留 boot 期填充动作 |
| K-037 | boot_image 清单与 VM 开天辟地（鸡生蛋破法） | M | 存量 | 06 §1.4/§2.3 | `table.c:44-64`；`main.c:196` | 理解 boot image 两层语义 | 06 §2.3 |
| K-038 | endpoint + generation 机制（防槽位复用混淆） | D | 存量 | 99 §1（仅 55 行）、06 §1.3 | `do_fork.c` generation++；`proc.c:isokendpt_f（L1830）` | IPC 寻址正确性根基 | 99 §1（扩写）+ 06 首现一句 |
| K-039 | RTS/MF 位图完整表与语义 | D | 存量 | 06 §2.1.6/§3.3、99（缺）、11 §4.1 | `proc.h:L142-262` | 状态机的位级真相 | 99（完整表权威）+ 06（存储设计）+ 11（联动） |
| K-040 | 12 步填充节拍：boot 循环逐字段装配 | M | 存量 | 06 §1.5/§4.7 | `main.c` boot 循环 :157-324 | 进程从白板到就位 | 06 §4.7 |
| K-041 | VM ELF 加载 load_vm_elf + 零堆启动工程 | M | 存量 | 06 §3.7-3.8/§4.2 | `os/arch/src/*/boot.rs:load_vm_elf` | boot 进程映像从哪来 | 06 §4.2 |
| K-042 | boot→running 转换、RTS_PROC_STOP 解禁 | M | 存量 | 06 §3.12/§4.8 | `main.c:bsp_finish_booting` RTS_UNSET | 启动何时算「跑起来」 | 06 §4.8 |
| K-043 | FPU 架构演进（fnsave→trait）、CpuContextArch | A | 存量 | 06 §3.5-3.6/§4.1/§4.9 | `os/arch/src/arch/fpu_arch.rs` | ⚠ 与 31-fpu 重叠 | → 29-fpu 主讲；06 只留 arch_boot_proc 调用点 |
| K-044 | Panic-in-Drop：KProcess/KPriv slot ownership invariant | V | 存量(参考) | panic-in-drop.md（已实施） | `os/kernel/src/proc_table.rs` Drop | Rust 侧槽位安全底线 | 06 §3（吸收为不变量一节） |
| K-045 | 跨空间访问核心矛盾：内核借页表却要看别人的内存 | C | 存量 | 07 §1.1 | — | 理解 freepdes/direct_map 的动机 | 07 §1 |
| K-046 | 32 位临时窗口（freepdes/createpde）vs 64 位 direct_map | A | 存量 | 07 §1.2-1.3/§2.4-2.5 | `memory.c:createpde（L69）` | 历史包袱与现代方案对照 | 07 §1-2 |
| K-047 | Dual-view 地址空间：VM DM + Kernel DM、DirectMapArch trait | D | 存量 | 07 §3.2-3.3/§4 | `os/arch/src/arch/direct_map.rs` | 双直映视图模型 | 07 §3-4 |
| K-048 | establish_boot_dm：DM 覆盖建立 | M | 存量 | 07 §4.4 | `os/kernel/src/dm_coverage.rs` | bootstrap 页何时进窗口 | 07 §4.4 |
| K-049 | system_init：call_vec 注册 + alarm/IRQ hook 初始化 | M | 存量 | 08 §2.3/§4.4 | `system.c:system_init（L168）` | 系统调用表如何成型 | 08 §2.3（注册事实）+ 13（enum 细节，见变更表 C-3） |
| K-050 | add_memmap / bootstrap 内存回收 | M | 存量 | 08 §4.5 | `pg_utils.c:add_memmap（L86）` | 引导内存何时归还 | 08 §4.5 |
| K-051 | bsp_finish_booting 节拍：cpu_identify→解禁→timer→fpu→switch | M | 存量 | 08 §1.2/§4.6 | `main.c:bsp_finish_booting（L38）` | 启动收尾的完整因果链 | 08 §4.6 |
| K-052 | Rust kmain T0–T6 阶段拆分与 C 对照 | A | 存量 | 08 §1.1 | `os/kernel/src/lib.rs:kmain（L529）` | Rust 侧启动时序真相 | 08 §1.1 |
| K-053 | Syscall enum + TryFrom + 58 项调用表 | D | 存量 | 08 §4.1-4.3、13 §2.3/§4.1 | `os/kernel/src/syscall.rs:Syscall` | 调用号→handler 映射 | ⚠ 重复 → 13 主讲；08 删（见变更表 C-3） |
| K-054 | SYS_VMCTL 子命令协议（SETADDRSPACE/KERN_PHYSMAP/MEMREQ/VMINHIBIT…） | I | 存量 | 09 §2/§4 | `system/do_vmctl.c:do_vmctl`；`arch/i386/arch_do_vmctl.c` | 内核-VM 协商线格式 | 19 |
| K-055 | VM 启动协商时序（switch→pick VM→init_page_table→VMCTL 五步） | M | 存量 | 09 §1.2 | `os/servers/vm/src/boot.rs` | 系统如何跨过鸡生蛋 | 19 §1.2 |
| K-056 | 双视图地址空间模型（bootstrap 视图→真实页表切换） | D | 存量 | 09 §1.3 | VMCTL_SETADDRSPACE | 理解 CR3 切换时机 | 19 §1.3 |
| K-057 | ptproc 跟踪/set_active_root/VM ELF at boot（09 §4.8-4.9） | M | 存量 | 09 §4.8-4.9 | `os/kernel/src/vm.rs` | ⚠ 与 06/07 部分重叠 | 19 §4（VM 协议侧），boot 加载事实留 06 |
| K-058 | switch_to_user：三入口一出口汇聚点、永不返回 | M | 存量 | 10 §1/§2.1 | `proc.c:switch_to_user（L299）` | 内核控制流总枢纽 | 15 |
| K-059 | idle：CPU 休眠姿态与唤醒 | M | 存量 | 10 §2.2 | `proc.c:idle（L176）` | 无可运行进程时干什么 | 15 |
| K-060 | misc 标志循环：KCALL_RESUME/DELIVERMSG/SC_DEFER/SC_TRACE/SC_ACTIVE | M | 存量 | 10 §2.1/§4.1 | `proc.c:L360-430` | 恢复前的五类遗留工作 | 15 §4.1 |
| K-061 | BKL 持有到记账点才释放、finish_and_restore | V | 存量 | 10 §3.1/§4.2/§4.6 | `os/kernel/src/lib.rs:finish_and_restore` | 锁生命周期与中断协作 | 15 §3-4 |
| K-062 | 队列-状态不变量 INV-1：在队 ⇔ 可运行 | V | 存量 | 11 §1.1 | `proc.c:enqueue（L1595）` | 调度正确性核心不变量 | 10（新号）§1 |
| K-063 | 16 级优先级队列模型 + enqueue/dequeue/pick_proc/proc_no_time | M | 存量 | 11 §1.2/§2 | `proc.c:pick_proc（L1689 区）`；`system.c:sched_proc（L642）` | 调度原语全集 | 10 §1-2 |
| K-064 | RTS_SET/UNSET 隐藏联动（宏→队列副作用） | V | 存量 | 11 §2.7/§4.4 | `proc.h:RTS_SET（L206）` | 状态位与队列的原子联动 | 10 §2.7 |
| K-065 | 数组索引替代指针链、Priority newtype、Scheduler 拆分等 Rust 决策族 | A | 存量 | 11 §3 | `os/kernel/src/sched.rs` | Rust 调度器设计取舍 | 10 §3 |
| K-066 | PREEMPTIBLE 标志 vs priority≠0 近似（edge E-PREEMPTFLAG 断环） | V | 存量 | 11 §3.5 部分 | `proc.c:1895`；`os/kernel/src/proc_table.rs:766` | 抢占判定的 C 语义真相 | 10 §3.8（缺陷现状如实标注） |
| K-067 | 六原语语义模型（按阻塞行为分类：send/receive/sendrec/notify/sendnb/senda） | C | 存量 | 12 §1.2 | `ipcconst.h` IPCNO_*；`proc.c:do_ipc（L599）` | IPC 语义总纲 | 11 |
| K-068 | 延迟拷贝 delivermsg：阻塞时只记账、恢复时才投递 | M | 存量 | 12 §1.3/§2.8 | `proc.c:delivermsg（L479）` | 消息何时真正搬移 | 11 §2.8 |
| K-069 | RECEIVE 三级检查与来源优先级 | M | 存量 | 12 §1.4/§2.5 | `proc.c:mini_receive` | 接收方如何选消息 | 11 §2.5 |
| K-070 | 死锁检测：P_BLOCKEDON 动态链 | M | 存量 | 12 §1.5/§2.7 | `proc.c:deadlock（L350 区）` | 循环等待如何被发现 | 11 §2.7 |
| K-071 | mini_notify 单边通知：不阻塞不丢失 | M | 存量 | 12 §2.6 | `proc.c:mini_notify` | 轻量通知机制 | 11 §2.6 |
| K-072 | mini_senda 批量异步 + SENDA 用户表直读 | M | 存量 | 12 §2.9/§3.10 | `proc.c:mini_senda` | 批量通知优化 | 11 §2.9 |
| K-073 | IpcOutcome/IpcEngine/caller_q 索引式 FIFO 等 Rust 决策族 | A | 存量 | 12 §3-4 | `os/kernel/src/ipc.rs` | Rust IPC 引擎设计 | 11 §3-4 |
| K-074 | IPC_STATUS 寄存器语义（L3 状态报告） | I | 存量 | 23 §2.8 | `ipc.h:IPC_STATUS_ADD（L42）` | 过滤结果如何回用户态 | 12（新号）§2.8 |
| K-075 | 三层过滤模型 L1 位图/L2 过滤链/L3 状态 | M | 存量 | 23 §1.1 | `priv.h:35,86`；`system.c:111,803` | 最小特权在 IPC 路径的执行 | 12 §1.1 |
| K-076 | may_send_to 与 may_asynsend_to 不对称、无 CHECK_IPC 的原因 | V | 存量 | 23 §1.3-1.4 | `priv.h:86-87` | 过滤时机的边界 | 12 §1.3-1.4 |
| K-077 | IPCF 过滤链元素匹配（IPCF_EL_MATCH 宏族） | D | 存量 | 23 §2.7 | `ipc_filter.h:19-41` | 细粒度过滤怎么匹配 | 12 §2.7 |
| K-078 | cause_sig → GETKSIG 轮询 → ENDKSIG 三步闭环 | M | 存量 | 19 §1.1/§2.3 | `system.c:cause_sig（L389）`；`do_getksig.c` | 微内核信号推拉模型 | 14-signal（新号，见变更 C-5） |
| K-079 | POSIX 路径 SIGSEND/SIGRETURN + sigframe 时序约束 | M | 存量 | 19 §1.2-1.3 | `do_sigsend.c`；`do_sigreturn.c` | 用户态处理器如何被调用 | 20（并入 syscall-signal） |
| K-080 | 信号管理器 s_sig_mgr/s_bak_sig_mgr、致命自信号 panic | V | 存量 | 19 §1.4 | `do_exit.c`；`system.c:cause_sig` | 自管理进程退出的边界 | 09-privilege（字段）+ 14（路径） |
| K-081 | stop-delay（MF_SIG_DELAY/sig_delay_done） | M | 存量 | 19 §1.5/§4.8 | `system.c:sig_delay_done` | PM 如何等进程停下 | 14 |
| K-082 | ex_data[] 异常→信号映射 | M | 存量 | 14 §2.4 | `exception.c:ex_data`；`arch_system.c` | 缺页/GP 如何变信号 | 16-exception |
| K-083 | SYS_KILL/GETKSIG/ENDKSIG/SIGSEND/SIGRETURN 五调用行为契约 | I | 存量 | 19 §2.3 | `system/do_{kill,getksig,endksig,sigsend,sigreturn}.c` | 信号调用面 | 14（内核路径调用）+20（POSIX 调用）——按双路径拆去向 |
| K-084 | SignalContext trait 三架构 sigcontext | A | 存量 | 19 §4.6 | `os/arch/src/arch/signal_context.rs` | 架构相关信号帧 | 20 §4 |
| K-085 | IRQ hook 生命周期（SETPOLICY/RMPOLICY/ENABLE/DISABLE 四子请求） | I | 存量 | 20 §1.1/§2.1 | `do_irqctl.c:do_irqctl` | 驱动如何收中断 | 22-device |
| K-086 | generic_handler 中断→通知副作用链（熵/位图/notify） | M | 存量 | 20 §1.1 | `do_irqctl.c:generic_handler` | 中断上下文不阻塞的翻译层 | 22 |
| K-087 | DEVIO/VDEVIO/SDEVIO 端口 I/O 语义分层 + IOPL/READBIOS 架构边界 | I | 存量 | 20 §1.2-1.3/§2 | `do_devio.c`/`do_vdevio.c`/`arch/i386/do_sdevio.c` | 驱动 I/O 面 | 22 §2 |
| K-088 | PortIo trait + BadCall 替代 cfg 的架构抽象 | A | 存量 | 20 §1.4/§4.7 | `os/plat/src/port_io.rs` | x86-only 调用的三架构退化 | 22 §4 |
| K-089 | TIMES 五值三时钟源（monotonic/realtime/boottime 不可混淆） | V | 存量 | 21 §1.1 | `do_times.c:do_times` | 时间查询语义 | 24-clock-sys |
| K-090 | SETALARM 同步闹钟链（minix_timer_t） | M | 存量 | 21 §1.2/§4.3 | `do_setalarm.c`；`clock.c:set_kernel_timer（L229）` | 进程级闹钟如何挂 | 24 |
| K-091 | STIME/SETTIME/adjtime 渐变 | M | 存量 | 21 §1.3/§4.4 | `do_stime.c`；`do_settime.c` | 系统时钟设置 | 24 |
| K-092 | VTIMER 虚拟/性能定时器 + vtimer_check 递减 | M | 存量 | 21 §1.4；15 §4.6-4.7 | `do_vtimer.c`；`clock.c` | 进程虚拟时间 | 24（syscall 面）+15（tick 内检查，已在 15） |
| K-093 | ClockState/TimerAction/AlarmTimerNode/TimerId 数据结构族 | D | 存量 | 15 §4.1-4.4 | `os/kernel/src/clock.rs` | Rust 定时器基建 | 17-clock-timer |
| K-094 | tick_with 全流程：记账/quantum/vtimer/闹钟/负载 | M | 存量 | 15 §4.5 | `clock.c:timer_int_handler（L70）` | 100Hz 上内核做什么 | 17 |
| K-095 | 三时钟源语义 + adjtime | D | 存量 | 15 §4.1；21 §1.1 | `clock.c:get_monotonic/get_realtime/get_boottime` | 时钟状态机 | 17（权威） |
| K-096 | 量子递减归架构层（decrement_quantum） | M | 存量 | 15 §4.9.1 | `os/arch/src/arch/clock.rs` | quantum 在哪减 | 17 §4.9 |
| K-097 | 负载平均 load_update | M | 存量 | 15 §4.10 | `clock.c:load_update（L260）` | 负载统计 | 17 |
| K-098 | BKL 完整机制：AtomicBool+CAS、RAII guard、BklSection witness、单 CPU 退化 | M | 存量 | 16 §1.1/§3/§4.12 | `os/kernel/src/smp.rs`；`globals.rs:BklProtected` | 多核同步实现 | 16-smp |
| K-099 | per-CPU 数据（cpulocals/CpuLocal）、免锁理由 | D | 存量 | 16 §1.2/§2.3/§4.3 | `cpulocals.h`；`os/kernel/src/smp.rs:CpuLocal` | CPU 私有状态 | 16 |
| K-100 | IPI 调度：smp_schedule_sync/handler、四函数封装 | M | 存量 | 16 §1.3/§4.7-4.10 | `smp.c:smp_schedule_sync（L75）`；`arch_smp.c:arch_send_smp_schedule_ipi（L357）` | 跨 CPU 唤醒 | 16 |
| K-101 | AP 启动握手与 early entry 梯子 | M | 存量 | 16 §1.5/§9 | `arch_smp.c:smp_init（L289）`；`ap_early_entry.rs` | 多核如何上线 | 16 |
| K-102 | CPU 亲和性与迁移（migrate_proc） | M | 存量 | 16 §1.4/§4.9 | `smp.c:smp_schedule_migrate_proc（L142）` | 进程绑核语义 | 16 |
| K-103 | SmpArch trait 三架构 IPI 抽象 | A | 存量 | 16 §3.D7/§4.2 | `os/arch/src/arch/smp.rs` | IPI 跨架构 | 16 |
| K-104 | kernel_call 分派循环：权限→路由→finish/resume | M | 存量 | 13 §1.6/§2.4/§4 | `system.c:kernel_call_dispatch（L52）` | 系统调用如何路由 | 13-syscall-dispatch |
| K-105 | KERNEL_CALL 归一化、独立 trap 入口、BadCall 决策 | V | 存量 | 13 §1.2-1.3/§3.3-3.4 | `mpx.S` KERNEL_CALL 向量；`protect.c:L148` | IPC vs syscall 入口分流 | 13 |
| K-106 | VMSUSPEND 挂起-恢复协议（dispatch 侧） | I | 存量 | 13 §1.4；24 §1.2-1.3 | `vm.h:VMSUSPEND`；`proc.c:vm_suspend` | 缺页时系统调用如何不丢 | 13 §1.4（协议定义）+ 21（新号 copy 篇消费） |
| K-107 | 58 项 syscall 列表与 call_vec 注册 | D | 存量 | 13 §2.3 | `system.c:L191-270` | 调用面全景 | 13 |
| K-108 | copy_msg_to_user 复用 delivermsg（D7 决策） | A | 存量 | 13 §3.2/§4.7 | `os/kernel/src/syscall.rs` | 应答如何回用户 | 13 |
| K-109 | dispatch_ipc_entry：IPC trap 入口臂 | M | 存量 | 13 §4.8；18-trap-bridge（设计已批准） | `os/kernel/src/syscall.rs:dispatch_ipc_entry`；`x86_ipc_dispatch_body` | int-33 腿的内核半 | 13 §4.8 + 16 §（入口汇编归 exception 篇） |
| K-110 | trap 桥：int-33 门、寄存器约定（RAX/RBX/RCX/SENDA=RDX）、RBX 二次返回通道 | I | 存量 | 18-trap-bridge-design（整篇，已批准已落地） | `trap_entry.rs:configure_ipc_entry`；`arch_trap.rs` | 用户如何进内核再回来 | 吸收→16-exception（入口约定）+ 13（分派衔接）；旧文归档 |
| K-111 | 异常帧 CPU 压了什么、三架构异常入口 | D | 存量 | 14 §1.3/附录 A | `mpx.S`；`os/arch/src/arch/exception.rs` | 读异常处理的前提 | 16 |
| K-112 | exception_handler 分流主干 + 嵌套四恢复点 | M | 存量 | 14 §2.1-2.2/§3.3 | `exception.c:exception_handler（L180）` | 异常如何被分派 | 16 |
| K-113 | 页错误转发 VM（RTS_PAGEFAULT/ForwardToVm） | M | 存量 | 14 §2.3/§4.6 | `exception.c:pagefault（L49）`；`os/kernel/src/page_fault.rs` | 缺页如何到 VM | 16 §2.3 |
| K-114 | IRQ hook 链管理 + hw_intr 抽象 | M | 存量 | 14 §2.5-2.6/§4.4 | `interrupt.c:put_irq_handler（L29）`；`hw_intr.h` | 中断分发实现 | 16 §2.5-2.6 + 22 §（do_irqctl 面） |
| K-115 | ExceptionArch/ExceptionDispatcher/FaultContext 决策族 | A | 存量 | 14 §3-4 | `os/arch/src/arch/exception_dispatcher.rs` | Rust 异常框架 | 16 §3-4 |
| K-116 | kernel_call 入口细节（SYSCALL LSTAR 腿、x86_syscall_dispatch_body） | M | 存量 | 18-trap-bridge R2；13 §4.3 | `trap_dispatch.rs:L179-236` | SYSCALL 指令腿 | 16 §（与 int-33 并列两腿） |
| K-117 | 进程生命周期弧 fork→exec→exit→clear + 三控制弧 | M | 存量 | 17 §1.1 | `do_fork.c/do_exec.c/do_exit.c/do_clear.c` | 进程状态机引擎 | 20-process（新号） |
| K-118 | 同步 fork：父必须 RECEIVING + 权限降级 SYS_PROC→USER | V | 存量 | 17 §1.2-1.3 | `do_fork.c:L63` | fork 的两条硬约束 | 20 |
| K-119 | runctl/schedctl/statectl 控制弧（含 SMP IPI 停止） | M | 存量 | 17 §1.4/§2.5-2.7 | `do_runctl.c/do_schedctl.c/do_statectl.c` | 运行时控制面 | 20 |
| K-120 | dispatch_fork/exec/exit/clear/runctl/schedctl/statectl 实现 + DEFERRED 标注 | X | 存量 | 17 §4 | `os/kernel/src/syscall_process.rs`（7 dispatch + 24 测试） | Rust 实现现状 | 20 §4 |
| K-121 | do_schedule（SYS_SCHEDULE）：EPERM 调度者检查 + sched_proc | I | 新增 | 仅 00 §3.10/33 §1.1/08/13 表格一行 | `system/do_schedule.c:do_schedule`（30 行实读） | 调度控制调用无正文——**覆盖缺口** | 20 §（并入 process 组：schedule+runctl 同族） |
| K-122 | vircopy/physcopy 信任调用者直接拷贝 | M | 存量 | 18 §1.1 | `do_copy.c:do_copy` | 分层拷贝第一层 | 21-copy（新号=旧18+24 合并，见 C-6） |
| K-123 | safecopy grant 授权拷贝（verify_grant 五步） | M | 存量 | 18 §1.2/§2.2/§4.3 | `do_safecopy.c`（448 行）；`os/kernel/src/grant.rs` | 不信任调用者的拷贝 | 21 |
| K-124 | umap/vumap/memset/safememset 地址映射与填充族 | I | 存量 | 18 §1.3-1.4 | `do_umap.c/do_vumap.c/do_memset.c/do_safememset.c` | DMA/映射查询 | 21 |
| K-125 | Direct Map 一行加法替代 createpde（64 位演进） | A | 存量 | 18 §1.5/§4.7；24 §4.1 | `os/kernel/src/cross_space.rs` | 拷贝基建演进 | 21（合并后单点） |
| K-126 | VMREQUEST 挂起-通知-恢复协议（kernel 代行缺页场景） | M | 存量 | 24 §1.2/§2.8 | `proc.h:p_vmrequest`；`proc.c:vm_suspend` | 内核代行撞缺页怎么办 | 21（与 106 同篇闭环） |
| K-127 | 三阶段拷贝抽象（地址解析/物理拷贝/结果回报） | C | 存量 | 24 §1.1 | — | 拷贝的统一心智模型 | 21 §1.1 |
| K-128 | 三种挂起类型 + 与 PAGEFAULT 区别 | V | 存量 | 24 §1.4-1.5 | `vm.h:VMSUSPEND/EFAULT_*` | 挂起语义边界 | 21 |
| K-129 | cross_space_copy/data_copy_vmcheck/suspend_for_vm 实现族 | X | 存量 | 24 §4 | `os/kernel/src/{cross_space,vm}.rs`（T-7 已补齐） | Rust 侧现状 | 21 §4 |
| K-130 | do_getinfo 多子请求分派 | I | 存量 | 25 §2.2/§4.1-4.2 | `do_getinfo.c`（228 行） | 信息查询面 | 25-misc |
| K-131 | do_trace 进程追踪（T_STEP/MF_STEP/SC_TRACE） | M | 存量 | 25 §2.3/§4.3 | `do_trace.c`（208 行） | 单步/追踪语义 | 25 |
| K-132 | do_update 进程槽位交换 | M | 存量 | 25 §2.4/§4.4 | `do_update.c`（340 行） | 槽位交换协议 | 25 |
| K-133 | 未移植三语义（完全/部分/未识别）与 BadCall 对齐 | V | 存量 | 25 §1.2 | `system.c:call_vec==NULL` | 诚实标注方法论 | 25 §1.2 |
| K-134 | krandom 随机数子系统接入 | M | 存量 | 25 §4.7 | `os/kernel/src/krandom.rs`；`do_irqctl` 熵源 | 随机数从哪来 | 25 |
| K-135 | do_setgrant：grant 表注册到 priv | I | 新增 | 仅 22 §4 提及 grant 表字段，`do_setgrant.c` 无专节 | `system/do_setgrant.c:do_setgrant`（30 行实读：RTS_NO_PRIV→EPERM，否则 _K_SET_GRANT_TABLE） | grant 表如何建立——**覆盖缺口** | 09-privilege §（与 privctl 同篇：priv 写入面） |
| K-136 | do_mcontext/getmcontext 机器上下文读写 | I | 新增 | 仅 08 §4.1 表格一行 + 19 §4.6 一句 | `system/do_mcontext.c:do_getmcontext`（106 行实读：isokendpt→EPERM 内核进程→FPU 状态检查） | 信号/调试相关上下文面——**覆盖缺口** | 20（信号上下文同族，do_sigsend 消费 mcontext） |
| K-137 | do_abort + prepare_shutdown + minix_shutdown 关停链 | M | 存量(部分) | 27 §4.275 表格一行有链，正文仅 panic 侧 minix_shutdown | `system/do_abort.c:do_abort`；`main.c:prepare_shutdown（L351）`→`minix_shutdown（L368）`→`arch_shutdown`（实读：1 秒 timer 后停中断/停 timer/清屏/打印/arch_shutdown） | 系统如何关机——非 C 固定清单「关闭与退出」——**半缺口** | 27-kernel-utility 扩节 §「关停链」 |
| K-138 | panic/kputc/_exit 三工具函数 | M | 存量 | 27 §2.2-2.4 | `utility.c`（93 行） | 内核错误报告 | 27 |
| K-139 | kmess_buf→Rust log 架构演进 | A | 存量 | 27 §3.2 | `os/kernel/src/kmess.rs`；`EarlyConsole` | 日志机制演进 | 27 |
| K-140 | .usermapped 段机制 + 两种信息暴露策略权衡 | C | 存量 | 28 §1/§2 | `usermapped_data.c`；`kernel.lds:L24-28` | 共享内存 vs 系统调用取舍 | 26-usermapped |
| K-141 | 8 个用户可见数据结构 + IPC trampoline 三套机制 | D | 存量 | 28 §2.2-2.3 | `usermapped_glo_ipc.S:IPCFUNC（L17）` | C 侧用户可见面 | 26 |
| K-142 | 64 位不保留 .usermapped 的六决策 + KERNINFO 共享页半保留 | A | 存量 | 28 §3 | `os/kernel/src/kerninfo.rs`（KerninfoPage + KERNINFO_USER_VA） | 演进决策与 edge E-KERNINFO 落地 | 26 §3 |
| K-143 | runqueues_ok 调度队列一致性检查 | M | 存量 | 29 §2.2/§4.1 | `debug.c:runqueues_ok`；`os/kernel/src/debug.rs` | 内核自诊断 | 29-kernel-debug |
| K-144 | print_proc 进程打印 + 条件编译调试模型 | M | 存量 | 29 §1.2/§2.3 | `debug.c` | 调试基建四类功能 | 29 |
| K-145 | IPC hook/统计（DEBUG_IPC_*）不实现的决策 | A | 存量 | 29 §3.3-3.4 | `debug.c:hook_ipc_msgsend` | WONTFIX 理由 | 29 |
| K-146 | 统计 profiling：采样四组件 + 三类样本分类 | M | 存量 | 30 §1.1/§2 | `profile.c:profile_sample`（157 行） | 内核自省采样 | 30-kernel-profile |
| K-147 | SPROFILE 条件编译 + NMI 采样 WONTFIX | V | 存量 | 30 §1.2/§3.3；26 §1.2 | `profile.c:#if SPROFILE`；`nmi_sprofile_handler` | 特性开关与替代方案 | 30 + 31（watchdog NMI 侧） |
| K-148 | lazy FPU + #NM 陷阱 + fpu_owner 归属协议 | M | 存量 | 31 §1/§2 | `arch_system.c` fpu 族 :51-215；`proc.c:copr_not_available_handler（L1922）` | 上下文切换成本优化 | 29-fpu（新号） |
| K-149 | FpuArch trait 三架构 State + SAVE_CTX IPI | A | 存量 | 31 §4 | `os/arch/src/arch/fpu_arch.rs` | Rust FPU 抽象 | 29 |
| K-150 | frame-pointer 链回溯 + 跨地址空间读取 | M | 存量 | 32 §1/§2 | `exception.c:proc_stacktrace（L333）`；`utility.c:util_stacktrace` | 崩溃诊断 | 32-stack-tracing |
| K-151 | StacktraceArch trait 三架构 + DIAGCTL STACKTRACE 接线 | A | 存量 | 32 §4 | `os/kernel/src/stacktrace.rs` | Rust 回溯实现 | 32 |
| K-152 | caller-by-nr 接口：句柄降级为值参数、两条链顶、残余 laundering | A | 存量 | 33 整篇 | `do_schedule.c:8` 签名；`proc.c:do_ipc（L601）` per-CPU 取 | Rust 借用模型下的调用者传递 | 33-syscall-caller-api |
| K-153 | NMI watchdog lockup 检测 + 64 位 WONTFIX 五理由 | V | 存量 | 26 整篇 | `watchdog.c`（112 行）；`main.c:cstart` USE_WATCHDOG 双门控 | 不实现的完整论证 | 31-watchdog |
| K-154 | Kernel 整体层级与 5 内核 task vs 系统服务器辨析 | C | 存量 | 00 §1.2-1.3 | `table.c:44-51`；`com.h:52` | 术语澄清（HARDWARE/ASYNCM 是 IPC 身份） | 00 §1.2 |
| K-155 | env_get/get_value boot 参数解析 | M | 存量 | 05 §2.6 部分、03 | `main.c:env_get（L505）` | boot 参数如何读 | 03 §2（cstart 内） |
| K-156 | cpu_identify/GET_CPUINFO（D-53） | M | 存量 | 08 §5.4 | `arch_system.c` cpuid 段；`os/arch/src/x86_64/cpu_identity.rs` | CPU 信息上报 | 08 §4.6 内 |
| K-157 | 非 C：构建工具链（cargo workspace/xtask/link.ld/check-layout） | T | 新增 | 散见 01 §5.2、02 §4.1，无专篇 | `os/xtask/src/{image,qemu}.rs`；`os/kernel-image/{build.rs,check-layout.sh}`；`os/Cargo.toml` | 内核镜像怎么构建出来——**固定清单「构建与工具链」** | **新建 34-build-test-infra** |
| K-158 | 非 C：测试基建（qemu-tests 脚本族/test-kernels/hosted 单测/CI） | T | 新增 | 散见各篇 §5，无总览 | `os/qemu-tests/*.sh`（18 个）；`test-kernels/`；`.github`（T-5 QEMU CI） | 测试怎么跑、分几层——**固定清单「测试基建」** | **新建 34-build-test-infra** |
| K-159 | 非 C：镜像与内存布局断言（check-layout L0-L4 契约） | I | 新增 | 02 部分涉及，check-layout.sh 无文档 | `os/kernel-image/check-layout.sh`（实读头注：ELF EXEC/VMA-LMA 平移量断言） | 布局契约的可执行验证 | 34 §布局断言 + 02 交叉引用 |
| K-160 | 非 C：跨模块线格式（KernelInfo validate、message wire、minix-types SYS_* 缺位） | I | 新增 | 01 §4.1 KernelInfo 有；SYS_* 常量无 | `minix-boot/src/kernel_info.rs:validate`；`minix-types/src/ipc/*`；edge E-MINTYPES-SYS | 跨 crate 契约盘点 | 01 §4.1 + 13 §（调用号线格式）+ 范围外发现 3 |
| K-161 | 非 C：错误路径总账（panic/shutdown/errno 映射/EDONTREPLY） | V | 新增 | 分散 14/19/27 | `do_exit.c:EDONTREPLY`；`errno.rs:ToErrno`（D1/D2） | 错误如何贯穿 | 27 §（统一错误路径表）+ 各篇契约内 errno 底线 |
| K-162 | 非 C：并发与同步固定项 → 00+16（K-002/K-098 已覆盖） | M | 存量 | 00/16 | 同 K-002/098 | — | 已有去向，列此仅为固定清单逐项对账 |
| K-163 | 非 C：汇编入口与陷阱进入固定项 → 02+16（K-013/K-110/K-111 已覆盖） | M | 存量 | 02/16/18-trap-bridge | 同 K-013/110/111 | — | 已有去向 |
| K-164 | 非 C：启动装配固定项 → 01+06（K-006/K-037 已覆盖） | M | 存量 | 01/06 | 同 K-006/037 | — | 已有去向 |
| K-165 | 非 C：链接与加载固定项 → 02（K-012/K-015 已覆盖） | M | 存量 | 02 | 同 K-012/015 | — | 已有去向 |
| K-166 | 非 C：镜像与内存布局固定项 → 02+07+26（K-012/K-047/K-140 已覆盖） | M | 存量 | 02/07/28 | 同 K-012/047/140 | — | 已有去向 |
| K-167 | C 装配辅助：extract-*.sh 脚本生成 errno/mtype 常量 | T | 新增 | 无文档 | `minix3/minix/kernel/extract-{errno,mtype,mfield}.sh` | 常量单一来源机制 | 34 §（C 构建辅助）或 13 §（常量来源注记）→ 归 34 |
| K-168 | procoffsets.cf：asm 与 C 结构偏移同步 | T | 新增 | 无文档 | `arch/i386/procoffsets.cf` | 汇编访问 struct 字段的契约 | 34 § + 02 交叉引用 |
| K-169 | boot.cfg 引导配置（模板→用户文件） | I | 存量(部分) | 01 §2.0.3 提及 | `minix3/etc/boot.cfg.default` | 引导配置面 | 01 §2 |
| K-170 | libc 用户侧 trap 体（_do_kernel_call_intr.S 双架构） | I | 新增 | 18-trap-bridge C2 提及无专节 | `minix3/minix/lib/libc/arch/{arm,i386}/sys/_do_kernel_call_intr.S` | 用户侧调用内核的汇编真相 | 16 §（用户半边，与 E1 Rust 半边对照） |

### 2.2 统计摘要

- **总条数**：170（存量 144 + 新增 26）
- **按类型**：概念 12、机制 68、数据结构 17、接口与协议 22、约束与不变量 14、架构演进 17、工具与工程 9、测试性质 3、混合 8（按主类型计）
- **按现有文档分布（主讲述点）**：00×5, 01×6, 02×6, 03×5, 04×4, 05×5, 06×11, 07×4, 08×5, 09×4, 10×4, 11×4, 12×7, 13×6, 14×4, 15×5, 16×6, 17×4, 18×4, 19×6, 20×4, 21×4, 22×1, 23×2, 24×4, 25×5, 26×2, 27×3, 28×1, 29×3, 30×2, 31×1, 32×2, 33×1, 99×2, 参考材料×2（K-044/K-137），纯新增无旧位×12
- **重复主讲述点标记**：K-032（05↔15）、K-036（06↔22）、K-043（06↔31）、K-053（08↔13）、K-057（06↔07↔09）、K-092（15↔21）、K-114（14↔20）——去向均已在表中裁决
- **覆盖缺口（新增来源）**：K-121 do_schedule、K-135 do_setgrant、K-136 do_mcontext、K-137 关停链、K-157 构建、K-158 测试基建、K-159 布局断言、K-160 线格式盘点、K-161 错误路径总账、K-167/K-168 构建辅助、K-170 libc trap 体

---

## 3. 覆盖审计

### 3.1 主题全集与来源

1. **C 源码符号**：20 个核心 .c（6908 行）+ 38 个 do_*.c（4248 行）+ arch/i386 40 文件（9028 行）+ arch/earm + 头文件族（priv/ipc/const/proc/system/config/glo/type/kernel）——逐文件与现有文档对账见 3.4 表与 2.1 池。
2. **OS 通用概念**：进程状态机、地址空间、权限/能力模型、中断与异常、时钟与调度、IPC 同步、SMP 并发、信号、上下文切换、页表——均已有主讲述点。
3. **非 C 制品**：见 3.5 固定清单十项逐项回答。
4. **阶段边界契约**（`00-master-plan/README.md` + `edge_todo.md` + `todo.md`）：01-stage-kernel = boot-shim/kmain/proc init/IPC/syscall 等（master-plan 表第 1 行）；edge 中属于本 stage 的生产面：E1 trap 桥（已落地，设计文=18-trap-bridge）、E-KERNINFO（→26）、E-SCHEDSMP/E-PREEMPTFLAG/E-SCHEDNICED（→10-scheduling/16-smp）、E-MINTYPES-SYS（范围外发现 3）、I-1/I-5/I-6 todo 项（→25/34 如实标注现状）。

### 3.2 覆盖缺口表

| # | 缺口主题 | 证据 | 建议 | 落实为新增知识点 |
|---|---------|------|------|-----------------|
| G-1 | do_schedule（SYS_SCHEDULE）无正文 | 全目录仅表格行 4 处（00/08/13/33）；C 实读 30 行含 EPERM 调度者检查 | 并入 process 组新篇一节 | K-121 → 20 §schedule |
| G-2 | do_setgrant 无正文 | 22 §4 仅 grant 表字段，`do_setgrant.c` 独立 30 行 | 并入 privilege 篇 | K-135 → 09 §privctl/SETGRANT |
| G-3 | do_mcontext 仅表行 | `do_mcontext.c` 106 行实读有独立语义（FPU 状态分支） | 并入 signal 篇 mcontext 节 | K-136 → 20 §mcontext |
| G-4 | 关停链（do_abort→prepare_shutdown→minix_shutdown）断续 | 27 §仅有 panic 侧 + 表格一行；`main.c:351-396` 实读完整链含 SMP 停核 | utility 篇扩「关停链」专节 | K-137 → 27 §关停 |
| G-5 | 构建工具链无文档 | 01/02 散见；`xtask`/`check-layout.sh`/`kernel-image/build.rs` 零文档 | **新建篇**（固定清单必答） | K-157/K-159/K-167/K-168 → 34 |
| G-6 | 测试基建无总览 | 各篇 §5 局部；`qemu-tests/` 18 脚本 + test-kernels 无目录级文档 | **新建篇**（与 G-5 合一篇） | K-158 → 34 |
| G-7 | 镜像布局断言契约 | `check-layout.sh` L0-L4 头注实读，无文档承载 | 34 § + 02 交叉引用 | K-159 → 34 |
| G-8 | 跨模块线格式总账 | KernelInfo 在 01；SYS_* 常量缺位（edge）；message wire 分散 | 01/13 各持一段 + 范围外发现登记 | K-160 → 01+13 |
| G-9 | 错误路径跨篇无总账 | panic 在 27、errno 在 13、EDONTREPLY 在 19、EFAULT 在 24 | utility 篇加统一表，各篇契约给 errno 底线 | K-161 → 27 § |
| G-10 | breakpoints.c（57 行）+ debugreg.S | 全目录零提及（`rg breakpoint 01-stage-kernel` 仅 exception gate 间接） | 并入 29-debug 一节（arch 断点寄存器面），或明确不做给理由 | → 29 §硬件断点（B 相写作时判定 WONTFIX 与否，契约验收含此项） |
| G-11 | oxpcie.c/direct_tty_utils.c/arch_reset.c（83+130+173 行） | direct_tty 被 01 boot 输出引用；oxpcie 仅 05 CONFIG_OXPCIE 一句；arch_reset 关停链用 | direct_tty→27 关停/输出；oxpcie→05 一句 + 明确边缘不做；arch_reset→27 关停链 | K-137 扩 + 05 注记 |
| G-12 | do_padconf/do_sdevio（earm/i386 arch 侧） | 20 §2.4-2.5 已覆盖 sdevio；padconf 在 todo 奇偶闭合但无正文 | 22-device 补 padconf 一小节（arm-only BadCall 语义） | → 22 §arch-only |
| G-13 | 99 篇仅 55 行，承诺「RTS 完整表+全局变量清单」未兑现 | `wc -l`=55，实际只有 endpoint 一节 | 99 扩写为权威查表篇（K-038/K-039/K-034） | K-038/K-039 → 99 |
| G-14 | 00 §6/§7 引用 `02-stage-vm/draft/` 已断链 | `rg draft 00-kernel-overview` 5 处，draft 目录不存在（02-stage-vm 已是正式编号） | 重建 00 时改指向 `../02-stage-vm/00-vm-overview.md` 等正式文件 | → 00 §6-7 |
| G-15 | 既有断链：21-clock-device.md（5 处 04-stage-pm）、01-04-test-audit（3 处） | rg 实证，目标文件不存在 | 计入引用迁移表，重建时顺带修复 | 见 §8.2 |
| G-16 | 07-paging_init_gpt 非文档（GPT 对话片段 358 行） | 无标题无结构 | 内容为 paging 设计评审，知识已入 02/07 正文 → 归档不迁 | 归档（参考材料） |
| G-17 | boot 链非 C 制品：bootxx/multiboot.S 站外链 | `sys/arch/i386/stand/` 整链 | 判定**不在本 stage**：属引导器阶段（master-plan 因果链 kernel 之前），01 边界一句话声明 | 明确不做 + 理由 |
| G-18 | Minix3 C 侧 tests/kernel（kqueue/tty/umount 等） | `minix3/tests/kernel/` 无文档 | 判定**参考材料**：Rust 侧等价物是 qemu-tests，归 34 §测试映射一节 | K-158 扩 |

**固定清单十项逐项回答（非 C）**：

| 项 | 在哪里讲 / 为什么不在本 stage |
|----|------------------------------|
| 链接与加载 | **02**（K-012/015/016） |
| 镜像与内存布局 | **02**（链接布局）+ **07**（运行时地址空间）+ **26**（.usermapped）+ 34（断言） |
| 汇编入口与陷阱进入 | **02**（head.S）+ **16**（mpx.S/trap 两腿/trap 桥吸收）+ 16 §用户半边（libc S 文件） |
| 启动装配 | **01**（boot-shim/multiboot/KernelInfo/boot.cfg）+ **06**（boot_image）；bootxx 站外链=G-17 不在本 stage |
| 构建与工具链 | **34 新建**（G-5/G-7/G-67/G-168） |
| 跨模块接口与线格式 | **01**（KernelInfo）+ **13**（call 线格式）+ **19**（VMCTL）+ 范围外 3（minix-types SYS_* 归 edge） |
| 错误路径 | **27**（统一表 K-161）+ 各篇契约 errno 底线 + **16**（异常→信号） |
| 关闭与退出 | **27**（K-137 关停链 + panic + _exit 禁令） |
| 并发与同步 | **00**（概念）+ **16**（机制）+ **10**（锁下队列操作）——K-162 |
| 测试基建 | **34 新建**（G-6/G-18） |

### 3.3 重复主题表

| # | 重复主题 | 出现处 | 新目录主讲述点 | 其余处置 |
|---|---------|--------|---------------|---------|
| D-1 | ClockState/tick 实现 | 05 §4.1 ↔ 15 §4.5 | **17-clock-timer** | 05 删 §4.1，保留 init_clock 调用点 |
| D-2 | struct priv 字段与 KPriv | 06 §2.2 ↔ 22 §1/§4 | **09-privilege** | 06 只留 boot 填充动作（K-040 内） |
| D-3 | FPU | 06 §3.6/§4.9 ↔ 31 整篇 | **29-fpu** | 06 留 arch_boot_proc 调用点 |
| D-4 | Syscall enum/58 项表 | 08 §4.1 ↔ 13 §2.3/§4.1 | **13-syscall-dispatch** | 08 删表，留 system_init 注册事实 |
| D-5 | cause_sig 语义 | 19 §4.3 ↔ 14 §2.4（消费）↔ 17 exit（消费） | **14-signal-core**（内核路径） | 16/20 只引用行为契约表 |
| D-6 | vtimer_check | 15 §4.6 ↔ 21 §1.4 | **17**（tick 内检查）+ **24**（syscall 面）——按面拆分非重复 | 两篇契约写清面边界 |
| D-7 | data_copy_vmcheck/DirectMap 消费 | 18 §4.7 ↔ 24 §4.1-4.2 | **21（18+24 合并后）** | 合并即消重 |
| D-8 | VM ELF at boot | 06 §4.2 ↔ 09 §4.9 | **06**（加载动作）+ **19**（协议侧） | 09 §4.9 改引用 |
| D-9 | endpoint | 99 §1 ↔ 06 §1.3 | **99**（权威表）+ 06（首现一句） | 06 去细节留引用 |
| D-10 | KernelInfo | 01 §4.1 ↔ 04 §4.4 | **01**（契约定义）+ 04（PlatformDesc 扩展消费） | 04 只讲扩展字段 |
| D-11 | IRQ hook | 14 §2.5 ↔ 20 §1.1/§2.1 | **16**（hook 链机制）+ **22**（do_irqctl syscall 面） | 按机制/接口拆面 |
| D-12 | 调试条件编译模型 | 29 §1.2 ↔ 30 §1.2（SPROFILE 同型） | 各归其篇（29/30）但共用「条件编译」概念首现放 29 | 30 引用 |
| D-13 | redox 对照 | 17/19/20/26/27/28/29/30/24 附录多处 | 保留为各篇 §「对照」小节（卓越性素材，不独立成篇） | 不迁不删 |

### 3.4 越界主题表

| # | 篇 | 越界内容 | 正确归属 |
|---|----|---------|---------|
| O-1 | 20-syscall-device 头部前置链接写成 `../14-exception-interrupt.md`（相对路径多了 `../`） | 链接错误 | 重建时修正（链到新编号 16） |
| O-2 | 05 §4.1 ClockState 全实现（D-1 同源） | 时钟运行时实现超出「初始化篇」边界 | 17 |
| O-3 | 08 §4.1-4.3 Syscall enum 全量（D-4） | 分派实现超出「启动完成篇」 | 13 |
| O-4 | 06 含 FPU/调度字段/SMP 预留等 §3.5-3.9（06-todo 已裁「不拆、分组引用」） | **不判越界**——历史裁决保留：单概念承载+字段全集导航可接受 | 留 06，但 FPU 实现细节（§4.9）按 D-3 迁 29 |
| O-5 | 22-privilege 前置声明「16（进程管理）、17（syscall-process）」但 16/17 编号实际是 smp/process——前置名实不符 | 前置声明错误 | 重建 09 时前置写真实依赖（06/11/13） |
| O-6 | 25-misc 前置「17-24 全部前序文档」 | 过度声明 | 新 25 前置精确化（13/16/21/9/29） |
| O-7 | 30-profile 前置含 26-watchdog，但 30 号<26 号时序上 26 在后（旧目录 26<30 实际成立）——旧目录无矛盾，但 26 的 NMI 结论被 30 依赖 | 依赖链要在新目录保持 31(watchdog)→30(profile) 或反向 | 新目录：watchdog=31、profile=30，30 前置 15/25 + 31 的 NMI 结论改为「31 反向引用 30」——profile 是 NMI 的消费论证方，**31 前置 30**，30 不前置 31 |

### 3.5 C 源码 ↔ 现有文档映射核对（清单来源：overview §3 表 + 检索核对）

`system/do_*.c` 38 文件逐一对账：36 个有正文归属（按 K 表去向），`do_setgrant.c`/`do_mcontext.c` = G-2/G-3 缺口（有表无文）；`do_abort.c` = G-4 半缺口。核心 .c 20 文件：`table.c`→06/99、`glo.h`→99、`cpulocals.c`（3 行）→16、`spinlock.h`→16、`watchdog.c`→31、`profile.c`→30、`debug.c`→29、`utility.c`→27、`interrupt.c`→16、`ipc_filter.h`→12、`usermapped_data.c`→26、其余均已映射。arch/i386 40 文件：`breakpoints.c`=G-10、`oxpcie.c`/`direct_tty_utils.c`/`arch_reset.c`=G-11、`io_*.S`→22（端口 I/O 汇编）、`klib.S`→16/22（phys_copy/端口原语）、`trampoline.S`/`apic_asm.S`→16、`debugreg.S`→G-10、`usermapped_glo_ipc.S`→16+26、其余→01/02/03/05/07/14/15/19。

---

## 4. 新目录

### 4.1 新篇章总表（34 篇）

| 新编号 | 标题 | 一句话定位 | 分组 | 操作类型 |
|--------|------|-----------|------|---------|
| 00 | kernel-overview | 新读者入口：内核是什么、启动线、按新目录的导航 | Part 0 入口 | 重写（导航按新目录） |
| 01 | boot-shim-bootstrap | 固件→boot-shim→arch_boot 的引导准备与控制权交接 | Part I 启动链 | 保留（编号不变） |
| 02 | link-load-higher-half | 链接、加载与高地址跳转：进 kmain 之前发生的一切 | Part I | 保留+改名 |
| 03 | kmain-cstart | kmain 入口与 cstart 保护结构初始化 | Part I | 保留 |
| 04 | platform-discovery | 平台硬件发现：参数从哪来 | Part I | 保留 |
| 05 | clock-intr-init | 时钟与中断初始化（仅初始化；运行时实现移交 17） | Part I | 保留+瘦身 |
| 06 | proc-init-boot-proc | 进程表初始化与 boot 进程装载（boot 阶段 C 权威） | Part I | 保留+吸收 K-044、按 D-2/D-3 瘦身 |
| 07 | cross-space-init | 跨地址空间初始化：临时窗口→direct_map | Part I | 保留 |
| 08 | system-init-boot-finish | 系统调用注册与启动完成（按 D-4 瘦身） | Part I | 保留+瘦身 |
| 09 | privilege | 权限管理：struct priv、掩码、privctl/setgrant 写入面 | Part II 机制 | **重排**（旧 22）+吸收 G-2 |
| 10 | scheduling-primitives | 调度原语与队列-状态不变量 | Part II | **重排**（旧 11） |
| 11 | ipc-core | IPC 六原语核心机制 | Part II | **重排**（旧 12） |
| 12 | ipc-filter | IPC 三层过滤（紧随 09/11） | Part II | **重排**（旧 23） |
| 13 | syscall-dispatch | 系统调用分派框架（并行体统一框架篇）+ caller 链顶 | Part II | 保留编号+吸收 33 §调用图 |
| 14 | signal | 信号：内核路径（cause_sig/GETKSIG/ENDKSIG）+ POSIX 路径 | Part II | **重排**（旧 19）+吸收 G-3 |
| 15 | switch-to-user | 调度循环：三入口一出口汇聚点 | Part II | **重排**（旧 10，序差 #7） |
| 16 | exception-interrupt | 异常与中断处理 + trap 桥（吸收 18-trap-bridge-design） | Part II | 保留+吸收 |
| 17 | clock-timer | 时钟中断与定时器运行时（吸收 05 §4.1） | Part II | 保留+吸收 D-1 |
| 18 | smp | 多核：BKL/per-CPU/IPI/AP | Part II | 保留 |
| 19 | vm-boot-protocol | 内核-VM 启动协商协议 | Part II | **重排**（旧 09，序差 #2） |
| 20 | syscall-process | 进程管理类调用（fork 族+控制族+**schedule G-1**） | Part III 并行体 | **重排**（旧 17）+扩 |
| 21 | syscall-copy | 跨空间拷贝 + VMREQUEST 运行时（**旧 18+24 合并**） | Part III | **合并**（C-6） |
| 22 | syscall-device | 设备 I/O 调用（+padconf G-12；修 O-1 链接） | Part III | **重排**（旧 20）+扩 |
| 23 | syscall-clock | 时钟类调用（TIMES/SETALARM/STIME/SETTIME/VTIMER） | Part III | **重排**（旧 21） |
| 24 | syscall-misc | 杂项与未移植调用（GETINFO/TRACE/UPDATE/DIAGCTL/SPROF…） | Part III | **重排**（旧 25）+精确前置 |
| 25 | privilege-control | **并入 09**，不独立成篇 | — | 旧 22 合并去向（见变更 C-1） |
| 25 | kernel-utility | panic/kputc/_exit + **关停链（G-4）+ 错误路径总账（G-9）** | Part IV 支线 | 保留（旧 27）+扩 |
| 26 | usermapped-data | 用户可见内核数据与 .usermapped 演进 | Part IV | **重排**（旧 28） |
| 27 | stack-tracing | 栈回溯 | Part IV | **重排**（旧 32） |
| 28 | fpu-context-switching | FPU lazy 上下文切换 | Part IV | **重排**（旧 31） |
| 29 | kernel-debug | 内核调试基础设施（+G-10 断点判定） | Part IV | 保留（旧 29） |
| 30 | kernel-profile | 统计 profiling | Part IV | 保留（旧 30） |
| 31 | watchdog | NMI watchdog（WONTFIX 文档化） | Part IV | **重排**（旧 26；O-7 依赖反向） |
| 32 | build-and-test | **新建**：构建工具链+镜像断言+测试基建+跨模块线格式总账 | Part IV | **新建**（G-5/6/7/9/67/68/60） |
| 33 | syscall-caller-api | caller-by-nr 接口（Rust 借用模型专题） | Part IV | 保留（旧 33） |
| 99 | global-concepts | 权威查表：endpoint/RTS/MF/全局变量/常量（**扩写**） | 附录 | 保留+扩写（G-13） |

> 注：表中第 25 行「privilege-control」是合并说明行，不占编号——实际新目录 00–33 + 99 共 **35 个文件**（旧目录 35 个编号文件持平；净变化 = 合并旧 18+24→21、旧 22+23→09+12、吸收 18-trap-bridge、**新建 32**、旧 07-paging_init_gpt/06-todo 归档退出编号）。

### 4.2 阅读路径

- **主线（启动因果链，必读）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 15（循环枢纽）→ 11 → 13 → 16 → 17 → 19 → 14 → 20 → 21
  - 主线内部序差已在 §1.3 序差表登记：15 置于 08 后第一位（枢纽先行），11/13 支撑读懂 15 的 misc 标志，19 紧随 15+13（VM 是第一个用户态动作）。
- **机制支线（按需）**：09 → 10 → 12 → 18 → 22/23/24（系统调用族）
- **可跳读支线**：25/26/27/28/29/30/31/32/33（调试、剖析、WONTFIX、构建测试、Rust 专题）
- **查表**：99（随时回查，不计入顺序依赖）

### 4.3 并行主题的分组与代表成员（Part III）

| 组 | 篇 | 组内代表成员（精讲） | 其余成员（差异表收束） |
|----|----|---------------------|----------------------|
| 框架 | 13 | kernel_call_dispatch 全链 | —（本篇即框架） |
| 进程生命周期 | 20 | **fork**（同步前提+代际+降权讲透） | exec/exit/clear/runctl/schedctl/statectl/schedule |
| 拷贝 | 21 | **safecopy**（grant 五步验证讲透） | vircopy/physcopy/umap/vumap/memset/safememset + VMREQUEST |
| 信号 | 14 | **cause_sig→GETKSIG 闭环**（内核路径讲透） | POSIX 路径 SIGSEND/SIGRETURN 按差异表 |
| 设备 | 22 | **IRQCTL**（钩子生命周期讲透） | DEVIO/VDEVIO/SDEVIO/IOPENABLE/READBIOS/PADCONF |
| 时钟 | 23 | **SETALARM**（闹钟链讲透） | TIMES/STIME/SETTIME/VTIMER |
| 杂项 | 24 | **GETINFO**（子请求分派讲透） | TRACE/UPDATE/DIAGCTL/SPROF + 未移植三语义 |

- **主线路径**：4.2 主线 19 篇。
- **支线路径**：机制支线 + 系统调用族（服务开发者读法）。
- **可跳读路径**：Part IV 全部；读启动链只需要 Part 0+I+15 骨架（15 首次读可只读 §1-§2，misc 标志细节回读 13/14/16）。

---

## 5. 每篇契约

> 体例参照 `06-todo.md`：定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单/验收。前置只允许指向更早编号（G3 机械检查依据）。

### 00-kernel-overview

- 一句话定位：没读过任何一篇之前，这篇告诉你内核是什么、整个 stage 怎么读。
- 讲什么：K-001、K-002（BKL 概念首现，完整）、K-003、K-004（按新目录导航）、K-005、K-154、与 02-stage-vm 边界一句话。
- 不讲什么：任何机制细节（去向：各篇）；启动步骤展开（→01）；执行模型以外的并发（→18）。
- 前置：无。
- 后置：全部篇。
- 事实底线：`table.c:44-51`（5 内核 task）、`com.h:52`（HARDWARE=KERNEL）、`smp.h` BKL、`main.c:kmain`。
- 知识点清单：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-001 | Kernel 执行模型 | C | `main.c:kmain` | 入口概念 | 00 §1 |
  | K-002 | BKL 概念首现 | M | `smp.h`；`main.c:148` | 全 stage 并发前提 | 00 §1.4 |
  | K-003 | 启动六阶段 | C | `main.c` | 全景骨架 | 00 §2 |
  | K-004 | 新目录导航 | T | — | 阅读地图 | 重写 |
  | K-005 | 设计原则 | C | `glo.h:vm_running` | 原则先于细节 | 00 §4-5 |
  | K-154 | 5 task vs 服务器 | C | `table.c:44-51` | 术语澄清首现 | 00 §1.2 |
- 验收：新读者读完能复述「内核与 VM 的三点差异」+ 说出六阶段名称；导航表中每个链接指向真实存在的新文件（`rg` 验证 0 断链）；G-14 的 draft 断链清零。

### 01-boot-shim-bootstrap

- 一句话定位：内核之前的那段代码做了什么，怎么把控制权交给内核。
- 讲什么：K-006、K-007、K-008、K-009、K-010、K-011、K-169、K-160（KernelInfo 契约定义半）、bootxx 边界声明（G-17）。
- 不讲什么：链接脚本细节（→02）、PlatformDesc 运行时（→04）、KernelInfo 扩展字段消费（→04，D-10）。
- 前置：00。
- 后置：02、04、32。
- 事实底线：`pre_init.c:pre_init/get_parameters/pg_mapkernel`、`head.S:multiboot_init`、`os/boot-shim/src/*`、`minix-boot/kernel_info.rs:KernelInfo`、`etc/boot.cfg.default`、`sys/arch/i386/include/multiboot.h`。
- 知识点：K-006、K-007、K-008、K-009、K-010、K-011、K-169、K-160。
- 验收：能画「固件→shim→arch_boot」四步因果链并给每步锚点；KernelInfo 每字段有生产者；bootxx 边界声明存在；§5 测试要点与 `qemu-tests` 实际脚本对账（G-6 交叉引用 32）。

### 02-link-load-higher-half

- 一句话定位：镜像如何被摆进内存并跳到高地址的 kmain。
- 讲什么：K-012、K-013、K-014、K-015、K-016、K-017、K-165/K-166（固定清单去向）。
- 不讲什么：boot-shim 装载逻辑（→01）、保护结构（→03）、运行时页表（→07）。
- 前置：01。
- 后置：03、07、26、32。
- 事实底线：`arch/i386/kernel.lds`、`head.S`、`os/kernel/src/arch/*/link.ld`、`os/boot-shim/src/loader.rs`、`higher_half.rs` 三架构。
- 知识点：K-012、K-013、K-014、K-015、K-016、K-017。
- 验收：能解释「只切栈不跳高会怎样」；riscv64 附录 bug 案例保留且锚点可核；链接布局与 32 篇 check-layout L0-L4 断言表互链。

### 03-kmain-cstart

- 一句话定位：进入 C 世界的第一件事——把 CPU 的保护结构接到内核手里。
- 讲什么：K-018、K-019、K-020、K-021、K-022、K-155。
- 不讲什么：时钟/中断初始化（→05，cstart 后半段调用点保留一句+后置）、平台发现（→04）、trap handler 行为（→16）。
- 前置：02。
- 后置：05、16、25。
- 事实底线：`main.c:kmain/cstart`、`protect.c:prot_init/init_codeseg/arch_post_init`、`os/arch/src/arch/protection.rs`、`trap_entry.rs`（门配置事实）。
- 知识点：K-018、K-019、K-020、K-021、K-022、K-155。
- 验收：cstart 调用序列逐项带锚点且与 `main.c:403-477` 实读一致（G1 抽查点）；「本章不讲什么」四条各有一个去向编号。

### 04-platform-discovery

- 一句话定位：不硬编码地址，内核怎么知道自己跑在什么硬件上。
- 讲什么：K-023、K-024、K-025、K-026（扩展字段消费面，D-10 边界）。
- 不讲什么：KernelInfo 契约定义（→01）、时钟编程（→05/17）、DTB/ACPI 规范全文。
- 前置：03。
- 后置：05、32。
- 事实底线：`arch_system.c:arch_init（acpi_init 消费点）`、`os/libs/minix-platform/*`、`minix-boot/kernel_info.rs`。
- 知识点：K-023、K-024、K-025、K-026。
- 验收：三架构「参数来源」对照表齐全；PlatformDesc 三种实现各有可核锚点；与 01 的 KernelInfo 边界句存在（D-10 闭环）。

### 05-clock-intr-init

- 一句话定位：让内核能感知时间与硬件事件——只讲初始化，不讲运行时。
- 讲什么：K-027、K-028、K-029、K-030、K-031；oxpcie 一句注记（G-11）。
- 不讲什么：**ClockState 全实现（D-1 删，→17）**、timer_int_handler 行为（→17）、异常分发（→16）、APIC 细节（→18）。
- 前置：03、04。
- 后置：06、15、16、17、32。
- 事实底线：`clock.c:init_clock`、`i8259.c:intr_init`、`arch_system.c:arch_init`、`os/plat/src/*/interrupt.rs`、`arch_init.rs`、`timer_irq_gate.rs`。
- 知识点：K-027、K-028、K-029、K-030、K-031（K-032 移出）。
- 验收：删除 §4.1 后与 17 §4.5 无重叠段（D-1 对账）；D-59 时序约束（enable 晚于 register）保留且锚点在；行数从 2263 降到 ≤1600（瘦身可测）。

### 06-proc-init-boot-proc

- 一句话定位：进程表从白板到「就位但暂停」——boot 阶段 C 的权威篇章。
- 讲什么：K-033、K-034、K-035、K-037、K-040、K-041、K-042、K-044（吸收）；priv 字段**填充动作**（D-2 边界内）；endpoint 首现一句（D-9）。
- 不讲什么：struct priv 字段语义全集（→09）、调度队列联动（→10）、FPU 实现（→28，D-3）、RTS 权威表（→99）、boot→running 的循环侧（→15）。
- 前置：03、05。
- 后置：07、09、10、15、19、20、99。
- 事实底线：`proc.c:proc_init`、`main.c` boot 循环 :157-324、`table.c:44-64`、`memory.c:arch_proc_init`、`proc.h` RTS_MF 定义行、`os/kernel/src/{proc,proc_table,kpriv}.rs`（os 侧引用热点 58 处，见 §8）。
- 知识点：K-033、K-034、K-035、K-037、K-040、K-041、K-042、K-044。
- 验收：06-todo §9 的 A1-A15 验收在重建后仍逐条可核（字段全集表保留、12 步节拍保留、职责边界矩阵保留）；与 09/17 的 priv/FPU 去重后无双主讲述点；`proc_table.rs` 58 处代码引用的目标小节名在迁移表可查。

### 07-cross-space-init

- 一句话定位：内核如何获得「看别的地址空间」的能力。
- 讲什么：K-045、K-046、K-047、K-048。
- 不讲什么：拷贝 syscall（→21）、VM 协议（→19）、页表结构本身（→02-stage-vm，Redesign 依据头注保留）。
- 前置：02、06。
- 后置：19、21、26、32。
- 事实底线：`protect.c:arch_post_init`、`memory.c:memory_init/createpde`、`os/arch/src/arch/direct_map.rs`、`dm_coverage.rs`。
- 知识点：K-045、K-046、K-047、K-048。
- 验收：「32 位窗口 vs 64 位 DM」对照表保留；附录 A/B 时序图与对照表保留；`direct_map.rs` 2 处代码引用的 §4.1 地址表小节在迁移表命中。

### 08-system-init-boot-finish

- 一句话定位：从「初始化态」到「运行态」的临门一脚。
- 讲什么：K-049（注册事实半）、K-050、K-051、K-052、K-156；smp_init 调用点（→18 引用）；**switch_to_user 调用事实一句（序差 #7 回指锚）**。
- 不讲什么：Syscall enum/58 表（D-4 删→13）、分派行为（→13）、循环体（→15）、VM 协议（→19）。
- 前置：05、06、07。
- 后置：13、15、18、19、32。
- 事实底线：`system.c:system_init`、`main.c:bsp_finish_booting`、`pg_utils.c:add_memmap`、`os/kernel/src/lib.rs:kmain T0-T6`。
- 知识点：K-049、K-050、K-051、K-052、K-156。
- 验收：删除 §4.1-4.3 后与 13 §2.3/§4.1 无表格重复（D-4 对账）；附录 A（smp_init 位次）保留；`memmap.rs`/`cpu_identity.rs`/`lib.rs` 共 11 处代码引用小节可迁。

### 09-privilege

- 一句话定位：谁能发给谁、能调什么、能碰哪些 I/O——权限的结构与写入面。
- 讲什么：K-036（主讲述点）、K-080（字段半）、K-135（**新增 do_setgrant 正文**）、s_flags/三类掩码/priv 表分配/I/O 范围表、SYS_PRIVCTL 子命令。
- 不讲什么：过滤执行时机（→12）、fork 时的权限降级流程（→20，引用本篇字段）、IPC 原语（→11）。
- 前置：06、11。（注：11 在 09 后——**修正**：09 前置改为 06；L1 过滤检查点的叙述放 12，09 只给结构。最终前置 = 06。）
- 后置：11、12、13、14、20、21、22、23。
- 事实底线：`minix/include/minix/priv.h`、`kernel/priv.h`、`system.c:get_priv`、`do_privctl.c`（371 行）、`do_setgrant.c`、`const.h:143-154` s_flags、`os/kernel/src/{kpriv,capability}.rs`（capability.rs 4 处代码引用）。
- 知识点：K-036、K-080（字段）、K-135、privctl 4.7 节既有内容。
- 验收：`do_setgrant.c` 全 30 行语义有正文（G-2 关闭）；与 06 的 priv 去重（D-2）后 06 无字段表；O-5 旧前置名实不符已修正；redox 对照保留。

### 10-scheduling-primitives

- 一句话定位：可运行状态与就绪队列为什么必须始终一致，以及维护它的四个原语。
- 讲什么：K-062、K-063、K-064、K-065、K-066（E-PREEMPTFLAG 断环现状如实写）。
- 不讲什么：switch_to_user 循环（→15）、SCHED 服务器面（→06-stage-sched，边界句）、clock 的 quantum 递减（→17）。
- 前置：06。
- 后置：11、15、17、20、29。
- 事实底线：`proc.c:enqueue/enqueue_head/dequeue/pick_proc/proc_no_time`、`system.c:sched_proc（L642）`、`proc.h:RTS_SET（L206）`、`os/kernel/src/sched.rs`（代码引用 10 处）。
- 知识点：K-062、K-063、K-064、K-065、K-066。
- 验收：INV-1 有「违反后果」两例（旧 §1.1 保留）；E-PREEMPTFLAG 断环有 C 锚点 `proc.c:1895` 与 Rust 锚点 `proc_table.rs:766` 双向对账；`sched.rs:3` 处引用小节可迁。

### 11-ipc-core

- 一句话定位：没有共享内存的两进程如何可靠交换消息——六原语+延迟投递+死锁检测。
- 讲什么：K-067、K-068、K-069、K-070、K-071、K-072、K-073。
- 不讲什么：过滤（→12）、分派入口（→13）、IPC_STATUS（→12）、拷贝机制本身（→21）。
- 前置：06、09、10。
- 后置：12、13、15、16、29。
- 事实底线：`proc.c:do_ipc/mini_send/mini_receive/mini_notify/mini_senda/delivermsg/deadlock`、`ipc.h` 宏族、`os/kernel/src/ipc.rs`。
- 知识点：K-067~K-073。
- 验收：六原语按阻塞行为的分类表保留；RECEIVE 三级检查与来源优先级表保留；混合链死锁测试（旧 §5.2）保留；`ipc.rs` 2 处引用可迁。

### 12-ipc-filter

- 一句话定位：消息送达前的三道闸——发送方位图、接收方过滤链、状态回报。
- 讲什么：K-074、K-075、K-076、K-077；L1/L2/L3 全表、行为契约表。
- 不讲什么：priv 字段定义（→09）、六原语本体（→11）、`s_k_call_mask` 构建（→09，检查时机留 13+本篇交叉）。
- 前置：09、11、13？——**修正（G3）**：过滤的分派接入点叙述放 13 之后会前向引用。裁决：本篇前置 = 09、11；`kernel_call_dispatch` 接入点以「调用入口处的 L1 检查」在本篇 §2.4 给出 C 行号锚点（`system.c:111`），不依赖 13 的正文。
- 后置：13、22（rs 侧对称检查引用 23 旧文 → 迁移后指 12）。
- 事实底线：`const.h:20-27` 位图原语、`priv.h:86-87`、`system.c:111,803-874`、`ipc_filter.h`、`ipc.h:25-48`、`os/kernel/src/ipc_filter.rs`（代码引用 2 处）。
- 知识点：K-074、K-075、K-076、K-077。
- 验收：三层模型方向不对称（L1 发送方/L2 接收方）论证保留；「为何无 CHECK_IPC」一节保留；`rs/ipc_mask.rs:148` 代码引用改指新编号 12 后 `rg` 命中。

### 13-syscall-dispatch

- 一句话定位：CPU 进内核后，请求如何路由到 handler、如何挂起恢复——并行体的统一框架篇。
- 讲什么：K-104、K-105、K-106（协议定义面）、K-107、K-108、K-109（衔接面）、K-053（**从 08 吸收的 enum/58 表**）、58 项完整列表、BadCall 决策、BKL 持有区间。
- 不讲什么：各 handler 内部（→20-24）、trap 入口汇编（→16）、权限字段（→09）、VMSUSPEND 的 copy 侧实现（→21）。
- 前置：06、08、09、11、15。（注：15 在 13 后——**修正 G3**：15-sched=旧11 已是 10，15 新号是 switch-to-user。重新列：前置 = 06、08、09、11、**10**（RTS）、**12** 无需。最终前置 = 06、08、09、10、11。dispatch 不依赖 switch_to_user 正文——它被 15 消费，方向是 15→13。）
- 后置：14、15、16、19、20、21、22、23、24、33。
- 事实底线：`system.c:kernel_call_dispatch/kernel_call_finish/kernel_call_resume`、`system_init` 注册段、`os/kernel/src/syscall.rs`、`trap_dispatch.rs:179-236`、`callnr.h`/`com.h` 调用号、`mpx.S` KERNEL_CALL 向量。
- 知识点：K-104、K-105、K-106、K-107、K-108、K-109、K-053。
- 验收：58 项表只存在于本篇（D-4 对账 `rg 'SYS_FORK' 08 13` 分布）；VMSUSPEND 协议表与 21 篇行为契约互链不重复展开；`syscall.rs` 分派相关引用（33 篇调用图并入 §4 后）可迁；`memmap.rs:1` 处旧引用指向 08 的 system_init 小节——迁移表覆盖。

### 14-signal

- 一句话定位：内核只做特权三件事，何时怎么处理交给用户态信号管理器——双路径信号机制。
- 讲什么：K-078、K-079、K-080（路径半）、K-081、K-082（消费契约）、K-083、K-084、K-136（**新增 do_mcontext 正文**）。
- 不讲什么：RTS 位定义（→06/99）、priv 字段（→09）、SIGSEND 拷贝实现（→21，引用）、异常分流本体（→16）。
- 前置：06、09、11、16？——**修正 G3**：K-082 ex_data 在 16 定义。裁决：ex_data 映射的**消费契约**（异常发生→置信号）在本篇只以 C 锚点 `exception.c` ex_data 表引用行号，不依赖 16 正文；本篇前置 = 06、09、11、13、16（若 16>14 则不行）。**最终序调整**：在 4.1 总表中把 14-signal 放在 16-exception **之后**（新编号 14↔16 对调代价大，改为：本篇前置 = 06、09、11、13；ex_data 消费面标「详见 16 §2.4」为后置引用，本篇正文不展开 ex_data 表）。前置 = 06、09、11、13。
- 后置：15（SC_TRACE 消费）、16（ex_data 消费）、20（exit 委托）、28（sigsend FPU）。
- 事实底线：`system.c:cause_sig/sig_delay_done`、`do_{kill,getksig,endksig,sigsend,sigreturn,mcontext}.c`、`proc.h` RTS_SIGNALED/RTS_SIG_PENDING、`signal.h` 常量、`os/kernel/src/syscall_signal.rs`（1035 行+18 测试）、`os/arch/src/arch/signal_context.rs`（代码引用 2 处）。
- 知识点：K-078、K-079、K-080、K-081、K-083、K-084、K-136、K-082（契约引用形态）。
- 验收：三步闭环时序图保留；`do_mcontext.c` 有正文段（G-3 关闭）；「范围说明」（无 do_sigctl 版本声明）保留；`signal_context.rs` 2 处引用 §3 D6 小节可迁；与 09 的 s_sig_mgr 分工句存在（D-2 边界）。

### 15-switch-to-user

- 一句话定位：所有进内核的路都从这一个出口回到用户态——调度循环枢纽篇。
- 讲什么：K-058、K-059、K-060、K-061；三入口一出口图、misc 五标志循环、finish_and_restore、地址空间切换、**08 调用事实回指（序差 #7）**。
- 不讲什么：pick_proc 算法（→10）、delivermsg 实现（→11）、kernel_call_resume 实现（→13）、cause_sig（→14）、trap 入口（→16）、tick（→17）——本篇只以「调用 10/11/13/14 的入口点」形态组织，各 callee 的完整语义归其篇。
- 前置：06、08、10、11、13、14。
- 后置：16、17、19、20、21、24、28。
- 事实底线：`proc.c:switch_to_user（L299）/idle（L176）`、misc 标志 `proc.h:234-262`、`os/kernel/src/lib.rs:switch_to_user（L3466）/finish_and_restore/idle/pick_and_bill`（代码引用 5 处）。
- 知识点：K-058、K-059、K-060、K-061。
- 验收：「三条进入路径」每条有 16 篇锚点交叉；BKL 记账点释放有 18 篇交叉；C/Rust 差异总表保留；`trap_return.rs:2` 处 + `proc_table.rs:1` 处引用可迁；**序差 #7 回指句**（「08 的最后一步」）存在。

### 16-exception-interrupt

- 一句话定位：异常与中断进来之后发生什么，以及用户态 trap 桥的内核半边。
- 讲什么：K-111、K-112、K-113、K-114、K-115、K-116、K-110（**吸收 18-trap-bridge-design**：int-33 门/寄存器约定/RBX 二次返回）、K-170（libc 用户半边对照）、K-082（ex_data 主讲述点）。
- 不讲什么：门配置（→03）、控制器初始化（→05）、时钟 handler（→17）、分派循环（→13）、信号后续路径（→14）、用户侧 Rust trap 体（→14-stage-runtime，E1 边界声明）。
- 前置：03、05、06、11、13、15。
- 后置：14（ex_data 消费）、17、20、22、28。
- 事实底线：`exception.c:exception_handler/pagefault/proc_stacktrace`、`interrupt.c:irq_handle`、`mpx.S:hwint/ipc_entry`、`usermapped_glo_ipc.S:IPCFUNC`、`protect.c:L147-150`、`arch/src/arch/{exception,exception_dispatcher,trap_entry,trap_return}.rs`（代码引用 9 处）。
- 知识点：K-111、K-112、K-113、K-114、K-115、K-116、K-110、K-170、K-082。
- 验收：trap 桥四决策（int-33/RAX-RBX-RCX 寄存器/上下文保存/RBX 返回通道）在本篇有完整小节且锚点含 `trap_entry.rs:291-314` 与 `arch_trap.rs`；18-trap-bridge 原文 87 行每节去向在迁移表；E1 边界句（用户半边归 14-stage）存在；O-1 旧 20 篇链接错误在新 22 修正后本篇前置链接全有效。

### 17-clock-timer

- 一句话定位：100Hz 的每一次滴答里内核做什么——时间记账与定时器运行时。
- 讲什么：K-093、K-094、K-095、K-096、K-097、K-092（tick 内检查面）、**K-032（从 05 吸收的 ClockState 实现）**。
- 不讲什么：init_clock 初始化（→05）、时钟 syscall 解析（→23）、AP 定时器注册（→18）、中断入口（→16）。
- 前置：05、10、15、16。
- 后置：23、24、30、31、28。
- 事实底线：`clock.c:timer_int_handler/load_update/get_*/set_*`、`arch_clock.c`、`arch/src/arch/clock.rs`、`os/kernel/src/clock.rs`（代码引用 6+4 处）。
- 知识点：K-093、K-094、K-095、K-096、K-097、K-092、K-032。
- 验收：ClockState 全文只在本篇（D-1 对账）；三时钟源语义表与 23 篇 TIMES 表互链不重复；L1/L2 对偶测试保留；`clock.rs` 10 处引用小节在迁移表。

### 18-smp

- 一句话定位：多核下用串行化换简单性——BKL、per-CPU、IPI、AP 上线。
- 讲什么：K-002（实现面回指 00 概念）、K-098、K-099、K-100、K-101、K-102、K-103。
- 不讲什么：BKL 概念首现（00 已讲，只引用）、锁下队列语义（→10）、FPU SAVE_CTX 消费（→28）。
- 前置：00、10、15、16、17。
- 后置：20（runctl IPI）、21（safecopy 锁）、28、29。
- 事实底线：`smp.c`（205 行）、`smp.h`、`cpulocals.h`、`arch_smp.c:smp_init`、`trampoline.S`、`apic_asm.S`、`os/kernel/src/smp.rs`（1439 行，代码引用 15 处热点）。
- 知识点：K-098、K-099、K-100、K-101、K-102、K-103（+K-002 实现面）。
- 验收：D1-D10 十决策保留；AP early entry 梯子节保留；28 个既有测试清单保留；`smp.rs`/`arm64/smp.rs` 15 处引用 §3 D7 等小节在迁移表；单 CPU 退化（compiler fence）语义保留。

### 19-vm-boot-protocol

- 一句话定位：VM 起来后与内核怎么谈，把系统切到运行时可用状态。
- 讲什么：K-054、K-055、K-056、K-057（协议侧）、五步协商时序、VMCTL 子命令分发表。
- 不讲什么：循环如何调度到 VM（→15）、direct_map 建立（→07）、VM 服务器内部（→02-stage-vm）、dispatch 框架（→13）。
- 前置：07、08、13、15。
- 后置：21、26、32。
- 事实底线：`system/do_vmctl.c`（173 行）、`arch/i386/arch_do_vmctl.c`（67 行）、`earm/arch_do_vmctl.c`、`os/servers/vm/src/boot.rs`（引用旧 09 共 7 处代码引用）、`os/kernel/src/vm.rs`。
- 知识点：K-054、K-055、K-056、K-057。
- 验收：协商时序图五步每步带 C 锚点；`vm.rs`/`boot.rs`/`syscall.rs`/`globals.rs`/`cross_space.rs` 共 12 处代码引用迁移命中；**序差 #2 回指句**（「本协议是第一个用户态动作」）存在；旧 09 §1.2 内 TODO 注（「应讲 C 而非 Rust」）在重建时落实为 C 优先叙述。

### 20-syscall-process

- 一句话定位：进程生命周期与运行时控制的七+一族调用。
- 讲什么：K-117、K-118、K-119、K-120、**K-121（do_schedule 新增正文）**；fork 精讲 + 六成员差异表（4.3 代表成员规则）。
- 不讲什么：priv 分配结构（→09）、信号委托细节（→14，引用 cause_sig 契约）、调度队列（→10）、SCHED 服务器（→06-stage）。
- 前置：06、09、10、13、14、18。
- 后置：21（fork 用 vircopy）、33（do_schedule 为 caller 链顶例）。
- 事实底线：`system/do_{fork,exec,exit,clear,runctl,schedctl,statectl,schedule}.c`、`os/kernel/src/syscall_process.rs`（代码引用 2 处）。
- 知识点：K-117、K-118、K-119、K-120、K-121。
- 验收：`do_schedule.c` 全 30 行语义（EPERM 调度者检查+五参数）有正文（G-1 关闭）；生命周期状态弧图保留；DEFERRED 标注保留；24 个测试清单可 grep 对账。

### 21-syscall-copy（旧 18+24 合并）

- 一句话定位：跨地址空间搬字节的完整故事——从信任分层到缺页挂起恢复。
- 讲什么：K-122、K-123、K-124、K-125、K-126、K-127、K-128、K-129；VMSUSPEND 消费面（协议定义引 13）。
- 不讲什么：grant 字段结构（→09）、direct_map 建立（→07）、VM 协议全貌（→19，MEMREQ 交互点引用）、分派框架（→13）。
- 前置：09、13、16？——**修正**：合并篇前置 = 09（grant）、13（分派+VMSUSPEND 定义）、16（BKL 新号=18）→ 前置 = 09、13、18、19（MEMREQ 交互在 19 后）。**最终：09、13、18、19、20**（fork 消费 vircopy：20 在前，前置含 20）。为满足 G3，21 号在 20 后 ✓。
- 后置：14（SIGSEND 拷贝）、20（fork 消费，交叉）、22（data_copy_vmcheck）、24、27、33。
- 事实底线：`do_copy.c/do_safecopy.c/do_umap.c/do_umap_remote.c/do_vumap.c/do_memset.c/do_safememset.c`、`memory.c:data_copy/virtual_copy_f/createpde/vm_lookup`、`proc.h:p_vmrequest`、`proc.c:vm_suspend`、`system.c:kernel_call_resume`、`os/kernel/src/{syscall_copy,cross_space,vm,grant}.rs`（syscall_copy 代码引用 1 处、cross_space 1 处、vm 4 处）。
- 知识点：K-122、K-123、K-124、K-125、K-126、K-127、K-128、K-129。
- 验收：合并后三阶段抽象（解析/拷贝/回报）为 §1 主线，旧 18 的分层叙事与旧 24 的挂起叙事无重复段（D-7 对账：`rg 'VMSUSPEND' 新21` 只一处主展开）；「不存在 do_datacopy」纠误句保留；旧 18 §4.8「DEFERRED 全落地」与旧 24 §4.6 状态合并无矛盾；54+ 测试清单合账。

### 22-syscall-device

- 一句话定位：驱动安全碰硬件的两条路——中断变通知、端口受权限。
- 讲什么：K-085、K-086、K-087、K-088、**padconf 小节（G-12）**、sdevio 跨进程批量（旧已覆盖）。
- 不讲什么：hook 链机制（→16，D-11 拆面）、CHECK 权限字段（→09）、port I/O 汇编原语（→32 引用 klib.S/io_*.S 锚点即可）。
- 前置：09、13、16、18。
- 后置：24、29。
- 事实底线：`do_irqctl.c/do_devio.c/do_vdevio.c`、`arch/i386/{do_sdevio,do_iopenable,do_readbios}.c`、`earm/do_padconf.c`、`os/kernel/src/syscall_device.rs`（1967 行，代码引用 2 处）、`os/plat/src/port_io.rs`（1 处）。
- 知识点：K-085、K-086、K-087、K-088、K-012（sdevio 偏移另计）、padconf=G-12。
- 验收：**O-1 链接错误修正**（`../` 前缀删除）；padconf arm-only BadCall 语义有正文（与 todo「奇偶闭合」结论一致）；4 子请求表保留；D1-D7 决策保留。

### 23-syscall-clock

- 一句话定位：用户态如何查询与设置时间、挂闹钟、管虚拟定时器。
- 讲什么：K-089、K-090、K-091、K-092（syscall 面）、五调用行为契约。
- 不讲什么：ClockState/timer 链实现（→17，D-6 拆面）、clock 初始化（→05）、SCHED 记账（→10）。
- 前置：09、13、17、20。
- 后置：30（SPROF 时钟依赖对照）。
- 事实底线：`do_times.c/do_setalarm.c/do_stime.c/do_settime.c/do_vtimer.c`、`clock.c:set_kernel_timer`、`os/kernel/src/syscall_clock.rs`（641 行，代码引用 1 处）。
- 知识点：K-089、K-090、K-091、K-092。
- 验收：三时钟源语义表与 17 篇互链不双主（D-6）；D1-D7 决策保留；「旧文档误标为 0」勘误句（todo_plan 引用的 15:140 处）在迁移后仍有承载篇；10 个测试+T-6 补齐清单保留。

### 24-syscall-misc

- 一句话定位：查信息、做追踪、换槽位、收诊断——杂项与未移植语义的诚实账本。
- 讲什么：K-130、K-131、K-132、K-133、K-134；DO_SPROF 状态机、krandom、BadCall 兜底。
- 不讲什么：profile 采样路径（→30，旧 25 §2.7/§4.8 **迁 30**）、panic/kputc（→25）、trace 的硬件断点面（→29，G-10 判定）。
- 前置：09、13、16、21。
- 后置：25（kputc 背景）、30（SPROF 状态机消费）、33。
- 事实底线：`do_getinfo.c/do_trace.c/do_update.c/do_sprofile.c`、`os/kernel/src/misc.rs`（3645 行，代码引用 1 处）、`krandom.rs`。
- 知识点：K-130、K-131、K-132、K-133、K-134。
- 验收：**O-6 前置精确化落地**（不再写「17-24 全部」）；profile.c 路径内容迁 30 后本篇无采样实现正文（D 对账）；三语义表（完全/部分/未识别）保留；DEFERRED 清单保留且与 todo §7 对账。

### 25-kernel-utility（旧 27 + 扩）

- 一句话定位：内核自己出错或要关机时怎么办——panic、输出、关停链、错误路径总账。
- 讲什么：K-138、K-139、**K-137（关停链新正文：do_abort→prepare_shutdown→minix_shutdown→arch_shutdown，G-4）**、**K-161（错误路径总账表，G-9）**、_exit 禁令、direct_tty/arch_reset 消费点（G-11）。
- 不讲什么：diagctl 实现（→24）、普通进程 exit（→20）、boot console 初始化（→03/05）、栈回溯实现（→27，panic 只调用引用）。
- 前置：03、08、24、27？——**G3 修正**：栈回溯在旧 32→新 27，27>25 会前向。裁决：panic 的 stacktrace **调用点**在本篇一句引用（锚点 `utility.c:util_stacktrace`），回溯机制正文归 27——本篇不需要 27 正文。**前置 = 03、08、24**；27 反向后置引用本篇（panic 路径消费）。
- 后置：27（panic 消费）、30、31、33。
- 事实底线：`utility.c`（93 行全）、`main.c:prepare_shutdown/minix_shutdown（L351-396 实读）`、`system/do_abort.c`、`arch/i386/arch_reset.c`、`direct_tty_utils.c`、`os/kernel/src/kmess.rs`、`minix-rt/src/lib.rs` panic_handler（代码引用 1 处 lib.rs→27 旧）。
- 知识点：K-137、K-138、K-139、K-161。
- 验收：关停链每步有 C 锚点且含 SMP 停核分支（`smp_shutdown_aps`）（G-4 关闭）；错误路径总账表覆盖 panic/errno/EDONTREPLY/EFAULT/VMSUSPEND 五通道各一行+篇指针（G-9 关闭）；kmess→log 演进保留；minix_shutdown DEFERRED 状态如实。

### 26-usermapped-data

- 一句话定位：内核信息如何免系统调用地到达用户态，以及 64 位为何不再这么做。
- 讲什么：K-140、K-141、K-142（含 E-KERNINFO 落地现状）。
- 不讲什么：链接段布局细节（→02）、KernelInfo boot 契约（→01）、ClockState 内部（→17）。
- 前置：01、02、07、19。
- 后置：04（对照）、33。
- 事实底线：`usermapped_data.c`、`usermapped_data_arch.c`、`usermapped_glo_ipc.S`、`kernel.lds:L24-28`（usermapped 段）、`os/kernel/src/kerninfo.rs`（代码引用 1 处）、`minix-boot/kernel_info.rs`。
- 知识点：K-140、K-141、K-142。
- 验收：两策略权衡表保留；六决策+D7 增补（KERNINFO 半保留）保留；IPC trampoline 三套机制与 16 篇入口章交叉不重复展开；`kerninfo.rs:14` 代码引用改指新编号后命中。

### 27-stack-tracing

- 一句话定位：崩溃时把调用栈捞出来——frame-pointer 链的三场景三架构。
- 讲什么：K-150、K-151；三场景（崩溃/DIAGCTL/panic）。
- 不讲什么：DIAGCTL 分派（→24）、异常分流（→16）、跨空间读原语（→21，read_word 消费引用）。
- 前置：16、21、24。
- 后置：25（panic 调用反向）、33。
- 事实底线：`exception.c:proc_stacktrace/proc_stacktrace_execute（L287-373）`、`utility.c:util_stacktrace（L39）`、`do_diagctl.c` STACKTRACE 分支、`os/kernel/src/stacktrace.rs`（代码引用 1 处）、`os/arch/src/arch/stacktrace.rs`。
- 知识点：K-150、K-151。
- 验收：KTS 简化代价（D5）保留；循环检测/截断/容错三健壮性节保留；`is/acquire.rs:1` 代码引用（旧 32）迁移命中；与 25 的分工句存在（25 调用、27 实现）。

### 28-fpu-context-switching

- 一句话定位：让 FPU 保存成本跟着真实使用走，而不是跟着调度走。
- 讲什么：K-148、K-149；lazy 模型、#NM 路径、fpu_owner 协议、SMP 远程保存、信号路径保存。
- 不讲什么：arch_boot_proc 初始化调用点（→06）、SAVE_CTX IPI 机制（→18，本篇消费）、异常分流（→16）。
- 前置：06、16、17、18、20（SIGSEND 消费）。
- 后置：33。
- 事实底线：`arch_system.c:fpu 族 :51-215`、`proc.c:copr_not_available_handler（L1922）/release_fpu`、`exception.c:enable/disable_fpu_exception（L375-383）`、`do_sigsend.c:84-88`、`cpulocals.h`、`fpu_arch.rs` + 三架构 `fpu.rs`（代码引用 1 处 exception_dispatcher→31 旧）。
- 知识点：K-148、K-149。
- 验收：C 侧 9 小节 file:line 索引全保留；恢复失败→SIGFPE 契约保留；`exception_dispatcher.rs:121` 代码引用迁移命中；与 06 的 D-3 去重后 06 §4.9 只剩调用点。

### 29-kernel-debug

- 一句话定位：内核怎么给自己做体检——队列一致性、进程打印、IPC 跟踪决策。
- 讲什么：K-143、K-144、K-145、**G-10 断点判定正文**（breakpoints.c/debugreg.S：保留为 WONTFIX 或实现，契约要求给出判定+理由）。
- 不讲什么：runqueues 数据结构（→10）、IPC 本体（→11）、profile 统计（→30）。
- 前置：10、11、18。
- 后置：30。
- 事实底线：`debug.c`（563 行）、`breakpoints.c`、`debugreg.S`、`os/kernel/src/debug.rs`（代码引用 0 处直接，间接经 runqueues）。
- 知识点：K-143、K-144、K-145、G-10。
- 验收：四类功能×条件编译表保留；WONTFIX 两条有理由；breakpoints.c 覆盖判定落地（G-10 关闭）；`debug.rs` 测试清单保留。

### 30-kernel-profile

- 一句话定位：内核采样自己，回答「CPU 时间花在哪」。
- 讲什么：K-146、K-147；**从 24 迁入的 profile.c 消费段**（旧 25 §2.7/§4.8 对应物——SPROF 状态机在 24，采样路径在本篇，D 拆面）。
- 不讲什么：do_sprofile 状态机（→24）、NMI watchdog（→31，**反向：31 前置本篇**）、时钟基建（→17）。
- 前置：17、24、29。
- 后置：31。
- 事实底线：`profile.c`（157 行全 `#if SPROFILE`）、`profile.h`、`do_sprofile.c`（状态机引用）、`os/kernel/src/misc.rs` profile 部分、`clock.rs` init/stop/ack_profile_clock（代码引用 0 直接）。
- 知识点：K-146、K-147。
- 验收：三架构 profile clock 接线表保留；NMI WONTFIX 决策保留且**不前置 31**（O-7 闭环）；与 24 的拆面边界句存在。

### 31-watchdog

- 一句话定位：内核卡死时谁拉警报——以及为什么 64 位重写不做它。
- 讲什么：K-153；lockup 检测机制全解析 + WONTFIX 五理由 + redox 对照。
- 不讲什么：时钟基建（→17）、NMI 采样消费（→30，**本篇前置 30**——O-7 裁决：30 的 sprofile NMI 消费结论被本篇 WONTFIX 论证引用，故 31 前置 30）。
- 前置：17、24、30。
- 后置：无（叶子）。
- 事实底线：`watchdog.c`（112 行）、`watchdog.h`、`arch/i386/arch_watchdog.c`（235 行 MSR 细节）、`main.c:cstart` USE_WATCHDOG/env_get 双门控、`profile.c:nmi_sprofile_handler`。
- 知识点：K-153。
- 验收：五理由各带锚点（arch 只 i386/earm、双重门控、PMU/MSR、NMI 采样替代、非正确性必需）；WONTFIX 清单保留；未来接入路径节保留。

### 32-build-and-test（**新建**）

- 一句话定位：这套内核怎么构建出镜像、怎么验证布局、怎么跑测试——工程基建总账。
- 讲什么：**K-157**（cargo workspace/xtask image+qemu/三 link.ld/kernel-image build.rs）、**K-159**（check-layout L0-L4 断言契约全文）、**K-158**（qemu-tests 18 脚本分层表 + test-kernels 目录 + hosted 单测 + T-5 CI）、**K-167**（extract-*.sh 常量生成）、**K-168**（procoffsets.cf 偏移同步）、**K-160**（跨模块线格式总账：KernelInfo/message wire/SYS_* 缺位 edge 指针）、**G-18**（minix3 C tests/kernel→qemu-tests 映射一节）、各篇 §5 测试要点的**索引**（不重复内容）。
- 不讲什么：任何被测机制的语义（各篇 §5）、测试用例逐条展开（各篇）、14-stage-runtime 的用户态测试（edge 边界）。
- 前置：01、02（构建物语义）。
- 后置：00（导航可链）、全部篇（§5 索引反向）。
- 事实底线：`os/xtask/src/{image,qemu,manifest}.rs`、`os/kernel-image/{Cargo.toml,build.rs,check-layout.sh,*.ld}`、`os/qemu-tests/*.sh`（`run_all.sh/run_qemu.sh/test-user-trap.sh/test-rt-birth*.sh` 等 18 个）+ `test-kernels/{bootstrap,user}/`、`os/Cargo.toml` workspace、`minix3/minix/kernel/{Makefile.inc,extract-*.sh,procoffsets.cf}`、`minix3/tests/kernel/`。
- 知识点：K-157、K-158、K-159、K-160、K-167、K-168、G-18。
- 验收（新建篇验收标准）：固定清单「构建与工具链」「测试基建」两项在本篇有完整正文（G-5/G-6 关闭）；check-layout 五断言逐条有脚本行锚点；qemu-tests 每脚本一行「测什么/对应哪篇」；C tests→Rust 测试映射表 ≥10 行；`procoffsets.cf`/`extract-*.sh` 各有半页说明（G-67/G-68 关闭）；本篇是全 stage 唯一无机制语义的纯工程篇（单篇单语义自检）。

### 33-syscall-caller-api

- 一句话定位：Rust 借用模型下，「调用者是进程表一行」如何降级为进程号传递。
- 讲什么：K-152；值参数三形态、两条链顶、残余 laundering 清单、备选方案取舍。
- 不讲什么：分派行为（→13，本篇只消费其链顶形态）、do_schedule 语义（→20）。
- 前置：13、20。
- 后置：无（Rust 专题叶子）。
- 事实底线：`do_schedule.c:8`、`system.c:p_delivermsg_vir :141`、`proc.c:do_ipc :601`、`do_privctl.c:47`、`proc.h:proc_addr :269`、`os/kernel/src/proc_table.rs:caller_slot_mut`。
- 知识点：K-152。
- 验收：调用图与残余清单保留且行号锚点复核（Rust 行号漂移风险高——验收要求 `rg` 复核每个 `.rs:` 锚点）；§6 验证/§7 边界保留；与 13 的分工句存在（13=C 语义+路由，33=Rust API 形态）。

### 99-global-concepts（**扩写**）

- 一句话定位：全 stage 权威查表——endpoint/RTS/MF/全局变量/核心常量，读任何篇卡在术语时回这里。
- 讲什么：**K-038（endpoint 扩全：编码/特殊值/行为规则/函数表）**、**K-039（RTS+MF 完整位表——旧承诺未兑现的 G-13）**、**K-034（EXTERN/table.c 存储模式）**、`glo.h` 全局变量清单、`const.h/config.h/kernel.h` 常量族、`type.h` 核心 typedef、`kernel.h` 头文件包含序。
- 不讲什么：任何机制叙述（各篇）；字段存储设计（→06）；权限字段语义（→09）。
- 前置：06（RTS/endpoint 首现之后回查）——**修正**：查表篇被 06 引用会 06→99 后向 ✓（99>06 是前向！）。**G3 裁决：99 置于目录末尾作为附录，任何篇前置不得写 99；各篇引用 99 为「回查表」不构成前置依赖**（前置=读懂本篇必需的前文；查表是可选回查）。本篇前置：无（自包含查表）。
- 后置：全部（可选回查）。
- 事实底线：`proc.h:L142-262`（RTS/MF 全位）、`endpoint.h`、`glo.h`（94 行全）、`const.h`、`config.h`、`type.h`、`kernel.h`、`table.c` EXTERN 模式、`minix/include/minix/{const,callnr,com}.h`。
- 知识点：K-038、K-039、K-034、+新增表行（globals/constants/type 三表，来源类型=新增，锚点如上）。
- 验收（G-13 关闭标准）：RTS 16 位+MF 20 位逐位有表行（对 `proc.h` 行号全量核对）；endpoint 五特殊值+四行为规则+函数表齐全；`glo.h` 每 extern 变量有「谁生产谁消费」一行；文件从 55 行扩到 ≥250 行；`rg` 验证各篇无重复的完整位表（D-9/D-39 对账）。

---

## 6. 变更表

| 操作编号 | 操作类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向/来源 |
|---------|---------|--------|--------|------|-----------|----------|
| C-1 | 合并 | 22-privilege + 23-ipc-filter | 09-privilege + 12-ipc-filter（**不合并，重排为相邻两篇**） | 过滤是权限的执行层，但 L2/L3 有独立语义与 5 函数行为契约表，硬合会超单篇单语义；**改为重排**：22→09（结构+写入面）、23→12（执行时机），09/11/12 相邻成组 | K-036,K-074~077,K-135 | 存量去向：22→09 全节；23→12 全节；新增来源：K-135 `do_setgrant.c` |
| C-2 | 合并 | 18-syscall-copy + 24-cross-space-runtime | 21-syscall-copy | 两篇共享 do_copy/`data_copy_vmcheck`/VMSUSPEND 同一主题链，拆开导致 18↔24 互为前置死锁（旧目录 18 前置 24 却 18<24 是后向、24 前置 18——实为双向交叉）；合并后单叙事：分层→挂起→恢复 | K-122~129 | 存量：18 全节+24 全节→21 对应节；无删除 |
| C-3 | 重排+瘦身 | 08 §4.1-4.3（Syscall enum/58 表） | 13-syscall-dispatch | D-4 重复主讲述点裁决 | K-053 | 08 删除（标注迁移→13），13 吸收 |
| C-4 | 重排+瘦身 | 05 §4.1（ClockState 实现） | 17-clock-timer | D-1 重复裁决 | K-032 | 05 删除→17 吸收 |
| C-5 | 重排 | 19-syscall-signal | 14-signal | 信号依赖链（RTS/priv/notify/dispatch）在 Part II 后段闭合；放 19 使其前置 11/16/22/16 大量前向；移 14 后前置=06/09/11/13 全后向 | K-078~084,K-136 | 全节原样搬移+编号改；新增 K-136 并入 |
| C-6 | 重排 | 09-vm-boot-protocol | 19-vm-boot-protocol | 序差 #2：协议需 15 循环+13 分派读懂；旧 09 前置 10（前向）旧 10 前置 09（交叉）——重排消解 | K-054~057 | 全节搬移 |
| C-7 | 重排 | 10-switch-to-user | 15-switch-to-user | 序差 #7：循环体需 10/11/13/14 作 callee；旧 10 前置 09 与 09 前置 10 死锁——重排消解 | K-058~061 | 全节搬移 |
| C-8 | 重排 | 11-scheduling-primitives | 10-scheduling-primitives | 让 10/11/12/13（sched→ipc→filter→dispatch）构成 Part II 前导组，消除旧 11→10、12→10 的前向 | K-062~066 | 全节搬移 |
| C-9 | 重排 | 12-ipc-core | 11-ipc-core | 同 C-8 | K-067~073 | 全节搬移 |
| C-10 | 重排 | 13-syscall-dispatch | 13（**编号不变**，但前置改写） | 旧前置含 10/11/12（旧号，其中 10 前向）；新前置=06/08/09/10/11 全后向 | K-104~109 | 前置声明重写 |
| C-11 | 重排 | 14-exception-interrupt | 16-exception-interrupt | 16 需 15（循环出口）+13（入口衔接）；放 16 后全部后向 | K-111~116,K-110,K-170,K-082 | 全节搬移+吸收 trap-bridge |
| C-12 | 重排 | 15-clock-timer | 17-clock-timer | 17 需 16（中断入口）；旧 15 前置 14/10 后向、前置 16（旧 smp）前向——新号 17>16 消解 | K-093~097,K-032 | 全节搬移+吸收 05 §4.1 |
| C-13 | 重排 | 16-smp | 18-smp | 18 需 17（时钟互操作）；旧 16 前置 15 后向、被 17/20/21 前置——新号 18 高于全部消费者前置集之外（消费者 20/21/28 在后） | K-098~103 | 全节搬移 |
| C-14 | 重排 | 17-syscall-process | 20-syscall-process | Part III 并行体组首篇 | K-117~121 | 全节搬移+新增 K-121 |
| C-15 | 重排 | 20-syscall-device | 22-syscall-device | Part III 组序（20→22） | K-085~088 | 全节搬移+G-12 扩+O-1 修 |
| C-16 | 重排 | 21-syscall-clock | 23-syscall-clock | Part III 组序 | K-089~092 | 全节搬移 |
| C-17 | 重排+瘦身 | 25-misc-unported | 24-syscall-misc | 旧 25「前置 17-24 全部」过度声明；profile.c 段迁 30 | K-130~134 | 除 profile.c 对应节→30 外全迁；O-6 修正 |
| C-18 | 重排 | 26-watchdog | 31-watchdog | 旧 26 在 debug/profile 前但 NMI 结论依赖 profile——O-7 反向：31 前置 30 | K-153 | 全节搬移 |
| C-19 | 重排+扩 | 27-kernel-utility | 25-kernel-utility | 放 Part IV 首（错误总账）；吸收 G-4 关停链、G-9 错误总账 | K-137,K-138,K-139,K-161 | 27 全节+新增两节 |
| C-20 | 重排 | 28-usermapped-data | 26-usermapped-data | Part IV 序 | K-140~142 | 全节搬移 |
| C-21 | 重排 | 31-fpu-context-switching | 28-fpu-context-switching | 28 需 20（SIGSEND 消费）——旧 31 前置 19（前向！）新 28>20 消解 | K-148,K-149 | 全节搬移 |
| C-22 | 重排 | 32-stack-tracing | 27-stack-tracing | 27 需 24（diagctl 消费点在 24 后）；旧 32 无前置声明（缺失） | K-150,K-151 | 全节搬移+前置补全 |
| C-23 | 保留 | 00,01,03,04,06,07,29,30,33,99 | 同号 | 编号未变；00/06/99 内容修订见 C-24/25/26 | — | — |
| C-24 | 重写 | 00 §3 导航 + §6/§7 参见 | 00 同号 | 旧导航表指向旧编号、§6 断链 draft/（G-14） | K-004 | 按新目录重写导航；断链改指 `../02-stage-vm/00-vm-overview.md` 等实文件 |
| C-25 | 吸收+瘦身 | 06 + panic-in-drop.md + 22 字段节 + 31 FPU 节 | 06 同号 | K-044 吸收；D-2/D-3 去重 | K-036,K-043,K-044 | 06 增 §不变量；priv/FPU 细节迁 09/28 |
| C-26 | 扩写 | 99（55 行） | 99 同号 | G-13 兑现承诺 | K-038,K-039,K-034+新增表 | 新增 RTSMF/globals/constants 表 |
| C-27 | 归档 | 06-todo.md、07-paging_init_gpt.md、18-trap-bridge-design.md、smp_gpt*.md | 归档区（B 相执行时移 `archive/` 子目录，不删） | 06-todo=过程文档（验收已执行完，§9 验收标准并入 06 契约）；07-gpt=GPT 对话片段非文档（G-16）；trap-bridge=已批准设计（知识全吸收进 16，K-110）；smp_gpt=smp_todo 评审输入（结论已在 16/smp_todo） | K-110 | 逐节去向见 §8.1 |
| C-28 | 新建 | — | 32-build-and-test | G-5/G-6/G-7/G-60/G-67/G-68/G-18 | K-157~160,K-167,K-168 | 来源锚点见 §7 |
| C-29 | 归档不动 | checklist.md、endpoint_todo.md、todo.md、smp_todo.md、panic-in-drop.md | 留原地（参考材料，不进编号） | todo/checklist/endpoint_todo 是工作清单非教学文档；panic-in-drop 知识已吸收（C-25） | K-044 | 参考材料声明见 §0.1 |
| C-30 | 重排 | 02-higher-half-kernel | 02-link-load-higher-half（**改名**） | 旧名只说 higher-half，实际范围=链接+加载+跳转三件事；改名对齐契约边界 | K-012~016 | 文件改名，节结构不动 |

**分节统计**：重排 18 篇、拆分 0 篇（05/08/25 的瘦身是「移出重复节」而非拆篇）、合并 1 篇（18+24→21）、新建 1 篇（32）、归档 4 篇、改名 1 篇（02）、保留但内容修订 5 篇（00/06/08/13/99）。

---

## 7. 缺漏新篇（非 C 固定清单逐项落实）

| # | 主题 | 为什么重要 | 原料 | 归哪篇 | 验收标准 |
|---|------|-----------|------|--------|---------|
| N-1 | 构建与工具链 | 固定清单必答项；读者无法从任何篇得知「cargo build 产出什么、xtask 怎么用、三 link.ld 差异」 | `os/xtask/src/*.rs`、`os/kernel-image/{build.rs,*.ld,check-layout.sh}`、`os/Cargo.toml`、`minix3/minix/kernel/Makefile.inc` | **32 新建 §构建** | 覆盖 workspace 结构/xtask image+qemu 子命令/三架构 link.ld 差异/build.rs 注入 `-T` 的机制；每条带锚点；读者能复述「一条命令从源码到可引导镜像」 |
| N-2 | 测试基建 | 固定清单必答项；各篇 §5 有局部但无分层总览（unit/hosted/qemu/integration 四层各是什么、怎么跑） | `os/qemu-tests/*.sh`（18 个）+ `test-kernels/` + `os/tests/` + 各 crate `#[cfg(test)]` + T-5 QEMU CI（todo §0.1） | **32 新建 §测试** | 四层测试模型表（层名/命令/代表脚本/测什么）≥4 行；18 个 qemu 脚本逐个一行归类；与 `minix3/tests/kernel/` 的 C 侧对照表 ≥10 行（G-18） |
| N-3 | 镜像布局断言 | check-layout.sh 的 L0-L4 是 boot-shim loader 的前置契约，零文档则断言无读者 | `os/kernel-image/check-layout.sh`（头注实读） | **32 新建 §布局断言** + 02 交叉引用 | L0-L4 五断言每条：断言什么/防什么漂移/脚本行锚点；与 02 §4.1 链接布局互链 |
| N-4 | 错误路径总账 | panic/errno/EDONTREPLY/EFAULT/VMSUSPEND 五通道散落五篇，读者无处总查 | `errno.rs:ToErrno`（D1/D2）、`utility.c:panic`、`do_exit.c:EDONTREPLY`、`vm.h`、`system.c:EBADREQUEST` | **25-kernel-utility §错误总账** | 五行表（通道/语义/产生点/锚点/详见篇）齐全；`Errno newtype` 演进（todo D1/D2 结论）一句带锚 |
| N-5 | 跨模块线格式总账 | KernelInfo/wire/SYS_* 常量三处分散；E-MINTYPES-SYS 是活 edge | `minix-boot/kernel_info.rs:validate`、`minix-types/src/ipc/*`、edge E-MINTYPES-SYS 条目 | **32 新建 §线格式**（总账+edge 指针）；KernelInfo 细节留 01，call 线格式留 13 | 总账表 ≥3 行（契约名/所有者篇/状态）；E-MINTYPES-SYS 以「范围外-edge」声明且指 edge_todo |
| N-6 | extract 脚本与 procoffsets（构建辅助） | 常量与 asm 偏移的单一来源机制，防「文档手抄常量」类漂移 | `extract-{errno,mtype,mfield}.sh`、`procoffsets.cf` | **32 §构建辅助** | 两机制各半页：输入什么/生成什么/谁消费 |
| N-7 | libc 用户侧 trap 体 | C 侧用户半边（`_do_kernel_call_intr.S`）是 trap 桥完整回路的一半，旧 18-trap-bridge 仅引用未展开 | `minix3/minix/lib/libc/arch/{arm,i386}/sys/_do_kernel_call_intr.S` | **16-exception §用户半边**（与内核臂对照；Rust 半边声明归 14-stage） | 双架构 S 文件逐指令对照表（进入/返回/errno 位置）；E1 边界句存在 |
| N-8 | bootxx 站外引导链 | 启动因果链在 kernel 之前还有一段，读者会找 | `minix3/sys/arch/i386/stand/{bootxx,boot,cdboot}` | **明确不做**：属引导器阶段（master-plan 因果链 GRUB/boot 之前），01 §边界一句声明+指向 master-plan | 01 存在边界声明句；无展开正文 |

**缺口→新篇映射核对**：G-5→N-1、G-6→N-2、G-7→N-3、G-9→N-4、G-60→N-5、G-67/68→N-6、G-170→N-7、G-17→N-8、G-1~G-4（do_schedule/setgrant/mcontext/关停链）→既有篇扩节（20/09/14/25），不新建篇。固定清单十项在 §3.2 已逐项有归宿，无一项留空。

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（旧位置 → 新位置）

> 覆盖全部 35 个旧编号文件 + 3 个被吸收文件。迁移类型：原样搬移=仅编号变、节结构不变；改写=节内叙事调整；合并=多来源并一；拆分=一来源进多篇；删除=迁移后不保留（给理由）。

**（a）编号不变的 10 篇（00/01/03/04/06/07/29/30/33/99）——按节列出内容修订：**

| 旧位置 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|--------|--------|--------|---------|------|
| 00 §1.1-1.5 | 内核是什么/执行模型/BKL | 00 §1（同） | 原样 | BKL 升级为「概念首现完整」（18 回指） |
| 00 §2 | 启动线 | 00 §2（同） | 原样 | 六阶段表编号按新目录核对 |
| 00 §3.1-3.10 | 旧导航 9 段 | 00 §3 | **改写** | 表行全部换新编号（C-24） |
| 00 §4-7 | 差异/原则/VM 关系/参见 | 00 §4-7 | 改写 | §6-7 断链 draft/ 改指实文件（G-14） |
| 01 全篇 6 章 47 节 | 引导准备 | 01 全篇 | 原样 | +§边界声明 bootxx（N-8）；§5 与 32 互链 |
| 03 全篇 7 章+附录 A | kmain/保护结构 | 03 同 | 原样 | — |
| 04 全篇 7 章 | 平台发现 | 04 同 | 原样 | +与 01 的 KernelInfo 分工句（D-10） |
| 06 Ch1-Ch6 | boot 阶段 C | 06 同 | 吸收+瘦身 | +§不变量(K-044)；§2.2 priv 细节→09；§3.6/4.9 FPU→28；endpoint 细节→99（D-2/3/9） |
| 07 全篇 5 章+附录 A/B | 跨空间初始化 | 07 同 | 原样 | — |
| 29 全篇 6 章 | 调试 | 29 同 | 吸收 | +G-10 breakpoints 判定节 |
| 30 全篇 6 章 | profiling | 30 同 | 吸收 | +25 迁入的 profile.c 消费段；前置去 31（O-7） |
| 33 全篇 7 章 | caller-by-nr | 33 同 | 原样 | `.rs:` 行号锚点全量 `rg` 复核（验收已列） |
| 99 §1 | endpoint | 99 §1 | 扩写 | K-038 全扩 |
| 99（缺） | RTS/MF/全局/常量 | 99 §2-5 | 新增 | G-13；锚点 `proc.h/glo.h/const.h/config.h` |

**（b）重排篇（编号变、节结构基本不动）——逐文件：**

| 旧文件 | 新文件 | 节级迁移说明 | 断链风险 |
|--------|--------|-------------|---------|
| 05-clock-interrupt-init | 05（同名） | §1-§3、§4.2-§4.14、§5-§6 **原样**；**§4.1 → 17 §4.1（吸收）**；§2.6 环境变量与 03 重叠→03 主讲、05 留调用点 | 低：文件名不变，仅删一节 |
| 08-system-init-boot-finish | 08（同名） | §1-§3、§4.4-§4.6、§5-§6+附录A **原样**；**§4.1-4.3 → 13 §4.1-4.3（吸收）** | 低 |
| 09-vm-boot-protocol | **19** | 全部 6 章原样搬，编号改；§4.9 与 06 重叠改引用（D-8） | **中**：7 处代码引用+18 处跨 stage 引用（见 8.2） |
| 10-switch-to-user | **15** | 全部 6 章原样；开头加「08 最后一步回指」段（序差 #7） | **中**：5 处代码引用+1 处跨 stage |
| 11-scheduling-primitives | **10** | 全部 6 章+附录 A 原样 | **高**：10 处代码引用+17 处跨 stage |
| 12-ipc-core | **11** | 全部 6 章原样 | **中**：2 处代码+14 处跨 stage |
| 13-syscall-dispatch | 13（同号） | 全部 7 章；前置改写；§4 吸收 08 §4.1-4.3 与 33 §调用图交叉 | 低（同号） |
| 14-exception-interrupt | **16** | 全部 6 章+附录；**18-trap-bridge-design 全文吸收**（§一~决策五 → 16 新增 §trap 桥节） | **高**：9 处代码引用+28 处跨 stage（14 是跨 stage 热点第 4） |
| 15-clock-timer | **17** | 全部 6 章；**+05 §4.1 吸收**；§6 前置改写 | **中**：10 处代码+10 处跨 stage |
| 16-smp | **18** | 全部 6 章+§9 AP 梯子原样 | **高**：15 处代码+6 处跨 stage |
| 17-syscall-process | **20** | 全部 6 章；+§schedule 新节（G-1） | 中：2 处代码 |
| 18-syscall-copy | **21（合并左半）** | §1-§6 原样→21 对应节 | **高**：27 处跨 stage（跨 stage 热点第 3）+1 处代码 |
| 19-syscall-signal | **14** | 全部 6 章；+§mcontext（G-3）；ex_data 改契约引用 | 中：3 处代码+12 处跨 stage |
| 20-syscall-device | **22** | 全部 6 章；O-1 链接修正；+padconf 节 | 中：3 处代码 |
| 21-syscall-clock | **23** | 全部 6 章+附录 A/B | 中：1 代码+4 跨 stage（含既有断链 21-clock-device→23 修复） |
| 22-privilege | **09** | 全部 6 章；O-5 前置改写；+setgrant 节 | 中：4 代码+6 跨 stage |
| 23-ipc-filter | **12** | 全部 7 章 | 中：2 代码+9 跨 stage |
| 24-cross-space-runtime | **21（合并右半）** | §1-§6+附录 →21 对应节；与 18 来源节合并去重（C-2） | 中：7 代码+7 跨 stage |
| 25-misc-unported | **24** | 除 §2.7/§4.8（profile.c 路径）**→30** 外全迁；前置改写 | 中：1 代码 |
| 26-watchdog | **31** | 全部 6 章；前置 +=30（O-7） | 低：0 代码+2 跨 stage |
| 27-kernel-utility | **25** | 全部 6 章；+关停链节（G-4）+错误总账节（G-9） | 低：1 代码 |
| 28-usermapped-data | **26** | 全部 6 章+附录 A | 低：2 代码+3 跨 stage |
| 31-fpu-context-switching | **28** | 全部 6 章 | 低：1 代码+2 跨 stage |
| 32-stack-tracing | **27** | 全部 6 章；前置补全 | 低：2 代码+2 跨 stage |
| 02-higher-half-kernel | **02（改名 link-load-higher-half）** | 全部 7 章+附录 A 原样 | **高**：16 处代码引用（3 个 link.ld 文件直接引用文件名！）——**改名必须同步 `rg higher-half-kernel os/` 全改** |

**（c）被吸收/归档文件的节级去向：**

| 旧文件与节 | 旧内容 | 新位置 | 迁移类型 | 备注 |
|-----------|--------|--------|---------|------|
| 18-trap-bridge-design §一 | 问题定义（回路缺失） | 16 §trap 桥§背景 | 合并 | 状态行「已批准」保留为历史注记 |
| 18-trap-bridge §二 C1-C4 | C 侧约束穷举 | 16 §trap 桥§C 约束 | 合并 | 锚点全带 |
| 18-trap-bridge §二 R1-R7/U1-U2 | Rust 现状+用户面 | 16 §trap 桥§现状 | 合并 | E1 边界句（U 面归 14-stage） |
| 18-trap-bridge §三（隐含决策） | 四决策+5 切片 | 16 §trap 桥§决策 | 合并 | RBX 二次返回（K-110） |
| 18-trap-bridge §进度各段 | 通电验证记录 | **删除** | 删除 | 属 edge_todo/E1 过程记录，教学文档不承载进度日志；edge_todo.md 仍在（参考材料） |
| 06-todo §1-§11 | 重构需求+验收 | **归档**（C-27） | 归档 | 验收标准 A1-A15 摘入 06 契约；写法体例已被本蓝图借用 |
| 07-paging_init_gpt 全文 | GPT 冻结评审对话 | **归档**（C-27） | 归档 | 结论（pt_init FROZEN）属 02-stage-vm/07 范围；kernel 侧无独立去向 |
| smp_gpt/v2/v3 | smp_todo 评审输入 | **归档**（C-27） | 归档 | 结论已固化进 16（现 18）§3 决策 |
| panic-in-drop.md | Drop invariant 设计 | **06 §3 不变量**（C-25 吸收） | 合并 | 锚点 `proc_table.rs` Drop；状态「已实施」保留 |
| checklist.md | 覆盖检查表（607 行） | **归档**（C-29 参考） | 归档 | 工作清单非教学；其覆盖结论已被本蓝图 §2/§3 取代 |

### 8.2 引用迁移表

**检索命令**（B 相批量执行用）：

```bash
# 1) 文档间链接（339 处，旧目录内）
rg -n '\[[0-9]{2}-[a-z0-9_-]+\.md\]' rewrite-notes/01-stage-kernel/ -g '!doc_rerank*'
# 2) os/ 代码注释（226 处命中本 stage 文件名；全仓 NN-title 模式 865 处含他 stage）
rg -n '[0-9]{2}-(boot-shim|higher-half|kmain|platform|clock-interrupt|proc-init|cross-space|system-init|vm-boot|switch-to-user|scheduling|ipc-core|syscall-dispatch|exception|clock-timer|smp|syscall-process|syscall-copy|syscall-signal|syscall-device|syscall-clock|privilege|ipc-filter|misc-unported|watchdog|kernel-utility|usermapped|kernel-debug|kernel-profile|fpu|stack-tracing|syscall-caller|kernel-overview)[a-z-]*\.md' os/
# 3) 跨 stage notes 引用（219 处）
rg -n '01-stage-kernel/[0-9]{2}-[a-z0-9_-]+\.md' notes/ -g '!**/doc_rerank*'
# 4) 规则/计划引用（21 处）
rg -n '01-stage-kernel/[0-9]{2}-' prompt/ .claude/
# 5) 旧目录名残留（23 文件）
rg -n '03-stage-kernel' os/ notes/ prompt/ .claude/
```

**旧引用 → 新目标 对照表（批量 sed 主表）：**

| 旧引用（文件名模式） | 新目标 | 影响引用数（文档间/代码/跨 stage） | 验证方式 |
|---------------------|--------|-----------------------------------|---------|
| `09-vm-boot-protocol.md` | `19-vm-boot-protocol.md` | 10 / 7 / 18 | sed 后 `rg 09-vm-boot` 三路=0 |
| `10-switch-to-user.md` | `15-switch-to-user.md` | 9 / 5 / 1 | 同上 |
| `11-scheduling-primitives.md` | `10-scheduling-primitives.md` | 15 / 10 / 17 | **热点**：先 sed 11→10TMP 再落盘，防与真 10 互撞 |
| `12-ipc-core.md` | `11-ipc-core.md` | 15 / 2 / 14 | 同双步法 |
| `14-exception-interrupt.md` | `16-exception-interrupt.md` | 28 / 9 / 1 | 高热点，优先批次 |
| `15-clock-timer.md` | `17-clock-timer.md` | 14 / 10 / 10 | 同上 |
| `16-smp.md` | `18-smp.md` | 31 / 15 / 6 | **最高热点（31 处文档引用）** |
| `17-syscall-process.md` | `20-syscall-process.md` | 17 / 2 / 1 | 双步法（17→20 与 20→22 环） |
| `18-syscall-copy.md` | `21-syscall-copy.md` | 7 / 1 / 27 | **跨 stage 热点第 3** |
| `19-syscall-signal.md` | `14-syscall-signal.md`（新名 `14-signal.md`） | 10 / 3 / 12 | 改名+编号双变，sed 用新全名 |
| `20-syscall-device.md` | `22-syscall-device.md` | 4 / 3 / 1 | 双步法 |
| `21-syscall-clock.md` | `23-syscall-clock.md` | 6 / 1 / 4 | 含修复既有断链 `21-clock-device`→23（G-15，5 处在 04-stage-pm） |
| `22-privilege.md` | `09-privilege.md` | 13 / 4 / 6 | 双步法 |
| `23-ipc-filter.md` | `12-ipc-filter.md` | 7 / 2 / 9 | 双步法 |
| `24-cross-space-runtime.md` | `21-syscall-copy.md`（并入） | 9 / 7 / 2 | 指向内容小节时给 21 对应新节名 |
| `25-misc-unported.md` | `24-syscall-misc.md` | 9 / 1 / 1 | 低风险 |
| `26-watchdog.md` | `31-watchdog.md` | 2 / 0 / 0 | 低 |
| `27-kernel-utility.md` | `25-kernel-utility.md` | 2 / 1 / 0 | 低 |
| `28-usermapped-data.md` | `26-usermapped-data.md` | 3 / 2 / 3 | 低 |
| `31-fpu-context-switching.md` | `28-fpu-context-switching.md` | 6 / 1 / 2 | 双步法（31→28 与 31 新号冲突——**旧 26→31 先做**，顺序敏感） |
| `32-stack-tracing.md` | `27-stack-tracing.md` | 1 / 2 / 2 | 双步法 |
| `02-higher-half-kernel.md` | `02-link-load-higher-half.md` | 8 / 16 / 0 | **改名**：代码 16 处（含 3 个 link.ld 文件头注释）必改 |
| `18-trap-bridge-design.md` | `16-exception-interrupt.md`（trap 桥节） | 0 / 1 / 1 | arch_trap.rs:3 注释改指 16 |
| `06-todo.md` | 归档路径（如仍需引） | 2 / 0 / 3 | 引用点改 `06-todo（归档）` 或删 |
| `26-vm-init-main.md`/`15-pagefault.md`/`03-vm-request.md`（00 §6 旧引用） | `../02-stage-vm/01-vm-init-main.md` 等实文件 | 5 / 0 / 0 | G-14 断链修复，逐条手改 |
| `01-04-test-audit.md`（03/04 内 3 处） | `.review/...` 真实路径或删 | 3 / 0 / 0 | G-15 既有断链 |
| `03-stage-kernel/`（旧目录名 23 文件） | `01-stage-kernel/` | 0 / 23 文件 | `rg 03-stage-kernel` 清零（含 `globals.rs:45`、`trap_entry.rs` 2 处、prompt 历史案例注记——**历史案例注记可保留**，标「当时目录名」） |

**批量修改方式建议**：
1. **双步编号迁移**防互撞：所有 `旧号-` 先换成 `NEWTMP-`（按对照表），第二遍 `NEWTMP-` → `新号-`；改名文件（02、14-signal）单独第三遍。
2. **顺序**：先归档文件移动（C-27/29），再双步 sed（目录内 → os/ → notes 跨 stage → prompt），最后改名两篇。
3. **验证门**：每批后跑 `rg '09-vm-boot|10-switch|11-scheduling|12-ipc-core|14-exception|15-clock-timer|16-smp|17-syscall-process|18-syscall-copy|19-syscall-signal|20-syscall-device|21-syscall-clock|22-privilege|23-ipc-filter|24-cross-space|25-misc|26-watchdog|27-kernel|28-usermapped|31-fpu|32-stack' `（旧模式）三路应为 0（prompt 历史注记除外）。
4. **代码内 §小节锚点**：如 `06-proc-init-boot-proc.md §3.6`（os/arch/boot.rs 等 58 处）——文件名不变无需改；变号篇的 `NN-name.md §X.Y` 小节引用随文件名一起 sed，§号在「原样搬移」下不变，**吸收节**（05§4.1→17、08§4.1→13、25§2.7→30、18-trap→16）的引用需逐条人工改（rg 精确列：`rg '05.*§4\.1|08.*§4\.1|25.*§2\.7|trap-bridge' os/ notes/`）。

### 8.3 断链成本摘要

- **受影响引用总数**：约 **1,177 处** = 文档间 339 + os 代码 226 + 跨 stage 219 + 规则/计划 21 + 旧目录名残留 23 文件 + 既有断链 13（21-clock-device 5 + test-audit 3 + draft 5）+ 吸收文件引用 6（trap-bridge/todo）。
- **热点文件**（修改最密集）：
  1. `os/kernel/src/proc_table.rs`（11 处：06×11）+ `proc.rs`（11）+ `kpriv.rs`（10）——06 不变号，**零风险**；
  2. 变号热点：`os/kernel/src/{lib,vm,smp,clock}.rs`、`os/arch/src/**/{boot,clock,smp,timer_irq_gate}.rs`（合计 ~60 处落在变号篇引用）；
  3. 文档热点：`16-smp` 被引 31 次、`14-exception` 28、`06-proc` 20（不变）、`11-sched` 15、`13-dispatch` 14（不变）；
  4. 跨 stage 热点：`06-proc-init` 31、`18-syscall-copy` 27、`09-vm-boot` 18、`11-sched` 17、`12-ipc` 14——重排篇占比高，B 相必须先跑 §8.2 批量表再改正文。
- **风险分级**：高（14→16、16→18、11→10、02 改名、18/24→21 合并）共 5 组须双步+人工抽查；中 12 组；低 8 组。
- **历史注记豁免**：`prompt/review-rules/review-patterns.md` 等 10 处 `03-stage-kernel` 是「首次发现于旧目录名时期」的历史案例记录——**保留不改**（改了反而失真），在迁移表登记豁免。

---

## 9. 验证与自检门

### 9.1 四种机械检查

**① 前向引用扫描**（逐篇检查契约「前置」字段只指向更早编号）：

| 篇 | 契约前置集 | 是否全后向 |
|----|-----------|-----------|
| 00 | ∅ | ✅ |
| 01 | {00} | ✅ |
| 02 | {01} | ✅ |
| 03 | {02} | ✅ |
| 04 | {03} | ✅ |
| 05 | {03,04} | ✅ |
| 06 | {03,05} | ✅ |
| 07 | {02,06} | ✅ |
| 08 | {05,06,07} | ✅ |
| 09 | {06} | ✅ |
| 10 | {06} | ✅ |
| 11 | {06,09,10} | ✅ |
| 12 | {09,11} | ✅ |
| 13 | {06,08,09,10,11} | ✅ |
| 14 | {06,09,11,13} | ✅ |
| 15 | {06,08,10,11,13,14} | ✅ |
| 16 | {03,05,06,11,13,15} | ✅ |
| 17 | {05,10,15,16} | ✅ |
| 18 | {00,10,15,16,17} | ✅ |
| 19 | {07,08,13,15} | ✅ |
| 20 | {06,09,10,13,14,18} | ✅ |
| 21 | {09,13,18,19,20} | ✅ |
| 22 | {09,13,16,18} | ✅ |
| 23 | {09,13,17,20} | ✅ |
| 24 | {09,13,16,21} | ✅ |
| 25 | {03,08,24} | ✅ |
| 26 | {01,02,07,19} | ✅ |
| 27 | {16,21,24} | ✅ |
| 28 | {06,16,17,18,20} | ✅ |
| 29 | {10,11,18} | ✅ |
| 30 | {17,24,29} | ✅ |
| 31 | {17,24,30} | ✅ |
| 32 | {01,02} | ✅ |
| 33 | {13,20} | ✅ |
| 99 | ∅（查表附录，前置规则见契约） | ✅ |

结果：**前向引用 = 0**（契约字段层面）。契约正文中 3 处「后置引用未来篇」（09→11 检查点叙述、14→16 ex_data 契约引用、25→27 stacktrace 调用点）均已在对应契约的「不讲什么/去向」或锚点引用形态中声明为**不依赖前文可读**——09 的 L1 检查点自带 C 锚点自洽、14 的 ex_data 只引行号、25 只给调用点，符合「读者只用前置集即可完整读懂」。

**② 依赖关系图（无环验证）**：由 ① 的前置集构图。构造性证明：全部前置集的每个元素 < 篇号（逐行核验上表），故图中无任何环（全序子集）。**无环 ✅**，无需拆解方案。

**③ 覆盖率检查**：知识点池 170 条，逐条在 §5 契约清单或 §6 变更表有去向——**170/170 = 100%**。明确删除项仅一类：18-trap-bridge「进度日志段」（删除理由：edge 过程记录非教学内容，已在 §8.1(c) 单列）。新增条目（26 条）全部带证据锚点（见 §2.1 来源列与 §3.2 缺口表）。✅

**④ 断链成本统计**：总量 ≈1,177 处、热点文件与批量方案见 §8.3。✅（成本已算清，重建可行）

### 9.2 自检门逐门结果

| 门 | 检查内容 | 结果 | 证据 |
|----|---------|------|------|
| G1 | C 真序逐条可核（抽 10 条核锚点） | **PASS** | 抽验：①`head.S:multiboot_init L68` push :75-76 ✅；②`pre_init.c:L217`、get_parameters :94、pg_mapkernel :232 ✅；③`main.c:kmain L115`、cstart :147 ✅；④`cstart L403` prot_init→init_clock→intr_init→arch_init :477 ✅；⑤`proc_init L119`、boot 循环 RTS_VMINHIBIT :265 区 ✅；⑥`system_init L168` map 表 :191-270 ✅；⑦`bsp_finish L38`、switch_to_user 调用 :110、RTS_UNSET :66、fpu_init :83 ✅；⑧`switch_to_user proc.c L299`、misc 循环 KCALL_RESUME/DELIVERMSG/SC_* ✅；⑨`do_ipc L599`、`exception_handler L180`、`irq_handle L116`、`timer_int_handler L70` ✅；⑩`smp_init arch_smp.c L289`、`arch_init arch_system.c L246` ✅。唯一标**待验证**项：真序 #3 vm_enable_paging 的确切 call site（`head.S` 全文未逐步读）——已按规则显式标注，不入断言。 |
| G2 | 知识点池完整：每个 C 文件/非 C 制品有归属或明确排除加理由 | **PASS** | 20 核心 .c + 38 do_*.c + arch 40 文件逐一对账在 §3.5；排除项带理由：bootxx（G-17/N-8 站外链）、oxpcie（G-11 判定边缘不做+05 注记）、`cpulocals.c` 3 行（→18 已含）；非 C 十项固定清单 §3.2 全回答无空项。 |
| G3 | 新目录前向引用为零（逐篇扫前置） | **PASS** | §9.1① 35 行全表，0 前向。 |
| G4 | 依赖图无环 | **PASS** | §9.1② 构造性证明（前置元素全小于篇号）。 |
| G5 | 覆盖率 100%：池每条有去向或删除理由；新增有锚点；删除项单列 | **PASS** | §9.1③：170/170；删除=trap-bridge 进度段（单列）；新增 26 条锚点齐。 |
| G6 | 每处拆分/合并写清存量去向；每处新建写清新增来源（抽 10 处） | **PASS** | 抽验 10 处：C-1(22→09 全节+23→12 全节)✅；C-2(18+24→21 双向逐节)✅；C-3(08§4.1→13)✅；C-4(05§4.1→17)✅；C-5(19→14)✅；C-27(trap-bridge §一~四→16、§进度→删有理由)✅；C-30(02 改名节不动)✅；N-1(来源=xtask/kernel-image 路径级锚点)✅；N-2(来源=qemu-tests 18 脚本)✅；N-8(明确不做+理由)✅。10/10。 |
| G7 | 每篇契约七要素齐全 | **PASS** | §5 共 35 篇（00-33+99），逐篇含：定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单表/验收标准——8 要素（题面 7 项+清单与验收合并算 7 项亦满足）。**35/35 齐全**。 |
| G8 | 锚点迁移表覆盖所有变化文档的每一节；引用迁移覆盖文档与代码注释 | **PASS** | §8.1：编号不变 10 篇逐节列修订、重排 24 篇逐文件列节级迁移+「原样搬移=节结构不动」声明、吸收/归档 4 文件逐节列——**35+4 全覆盖**；§8.2 引用迁移含 5 路检索命令+27 行对照+批量方案，文档间/代码/跨 stage/规则四类全含。 |
| G9 | 事实断言有锚点（抽 10 条核；推测已标注） | **PASS** | 抽验：①5 内核 task→`table.c:44-51`✅；②BKL_LOCK 时机→`main.c:148`✅；③do_setgrant EPERM 语义→`do_setgrant.c:RTS_ISSET(RTS_NO_PRIV)`（30 行实读）✅；④do_schedule 调度者检查→`do_schedule.c:caller != p->p_scheduler → EPERM`✅；⑤do_mcontext FPU 分支→`do_getmcontext.c:proc_used_fpu→OK`✅；⑥关停链四步→`main.c:351/368 + do_abort.c`✅；⑦check-layout L0-L4→`check-layout.sh` 头注✅；⑧IPCFUNC 三套→`usermapped_glo_ipc.S:L17/29/55`✅；⑨IDT 门 USER_PRIVILEGE→`protect.c:L147-150`✅；⑩`system_hz` 别名→`glo.h:#define system_hz (kclockinfo.hz)`✅。推测项：真序 #3 paging call site（已标待验证）；33 篇 `.rs:` 行号时效性（验收要求 `rg` 复核）。10/10 锚点命中。 |

### 9.3 结论与待用户裁决的问题

**结论**：**未完成→已完成（蓝图交付）**。四机械检查与九自检门全 PASS；断链成本 ≈1,177 处已算清且给出双步批量方案；知识点池 170 条 100% 有去向；35 篇契约七要素齐全、前向引用为零、依赖无环。本蓝图可作为 B 相施工图的 mimo 分量输入（待与其它 AI 蓝图汇总收敛为共识蓝图后开工）。

**待用户裁决的问题**（不阻塞蓝图交付，共识阶段需拍板）：

1. **合并粒度**：C-2 将旧 18+24 合为 21（消双向交叉），但合并后预计 ~1400 行；若共识认为仍偏大，备选拆法=21a「分层拷贝与 grant」+21b「VMREQUEST 挂起恢复」（前置链 21b→21a 反向需再验证）。mimo 推荐**先合并**（单叙事更完整）。
2. **重排深度**：Part II 采用「sched→ipc→filter→dispatch→signal→switch→exception→clock→smp→vm」10 篇重排，消解旧目录全部约 30 条前向边；代价=16-smp/11-sched 等高热点引用大批量迁移。备选浅案=只动 09/10/11 三篇死锁（旧 09↔10、11→10）+22/19/31 三篇前向，其余不动——迁移量降 60% 但仍留 15+ 条前向。mimo 推荐**深度重排**（本任务前提=旧文档未审、以 C 真相为准，浅案留病灶）。
3. **02 改名**（higher-half→link-load-higher-half）：牵动 16 处代码注释；若共识要最小化断链，可保留旧文件名只改头部声明边界。mimo 推荐改名（边界更诚实），但**接受共识否决**。
4. **新建 32 与 06-todo 类过程文档的归档位置**：`archive/` 子目录 vs 留原地加 `archived-` 前缀——涉及 B 相归档命令形态，需与多 AI 产物共存策略（他 AI 的 `doc_rerank_*` 不归我们管）一并裁决。
5. **`prompt/todo_plan.md` 的 21 处引用**：属工作计划文件，是否随迁移批量改，还是等计划条目自然过期——建议裁决为**改**（防后续会话按旧编号定位失败）。
