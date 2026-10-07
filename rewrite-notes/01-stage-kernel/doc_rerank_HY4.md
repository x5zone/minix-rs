# 01-stage-kernel 文档重建蓝图（HY4）

## 执行头部

```text
your_name(AI agent name) = HY4
target_dir(关注的工作目录) = 01-stage-kernel
repo_root(仓库根目录) = /home/xzhao/github/minix-rs

任务 = R 相·重建蓝图：输出 target_dir/doc_rerank_HY4.md，不改任何正文。
       只读 C 代码不够，还必须读：本目录全部文档的头部声明、os/ 目录对应入口、
       00-*-overview.md 的导航表。
约束 = 不得引用 .design/ 与 tmp_design_and_todo/；任何落盘产物必须带
       _HY4 后缀；不得读取或照抄其它 AI 的 doc_rerank_* 产物。
```

---

## 0. 元数据

### 0.1 执行信息

| 项 | 值 |
|----|-----|
| 执行者 | HY4 |
| 日期 | 2026-09-19 |
| 目标目录 | `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/` |
| 仓库根 | `/home/xzhao/github/minix-rs` |
| 当前提交 | `2d9d1f0aa32da37b0b0761a2d79d32010ada5d5c` |
| 阶段判定 | **启动链型为主 + 运行期三入口汇聚型为辅**（判定理由见 §1.0） |
| 交付物 | 本文件（唯一落盘产物） |

### 0.2 审查范围

**算作文档（编号文档，进入知识点池与迁移表）**——共 35 篇：

`00-kernel-overview` `01-boot-shim-bootstrap` `02-higher-half-kernel` `03-kmain-cstart`
`04-platform-discovery` `05-clock-interrupt-init` `06-proc-init-boot-proc` `07-cross-space-init`
`07-paging_init_gpt` `08-system-init-boot-finish` `09-vm-boot-protocol` `10-switch-to-user`
`11-scheduling-primitives` `12-ipc-core` `13-syscall-dispatch` `14-exception-interrupt`
`15-clock-timer` `16-smp` `17-syscall-process` `18-syscall-copy` `18-trap-bridge-design`
`19-syscall-signal` `20-syscall-device` `21-syscall-clock` `22-privilege` `23-ipc-filter`
`24-cross-space-runtime` `25-misc-unported` `26-watchdog` `27-kernel-utility`
`28-usermapped-data` `29-kernel-debug` `30-kernel-profile` `31-fpu-context-switching`
`32-stack-tracing` `33-syscall-caller-api` `99-global-concepts`

**算作参考材料**（不进知识点池，只在变更表里给出归档处置）：
`todo.md`、`06-todo.md`、`checklist.md`、`endpoint_todo.md`、`smp_todo.md`、
`smp_gpt.md` / `smp_gpt_v2.md` / `smp_gpt_v3.md`、`panic-in-drop.md`、
`doc_rerank_deepseek.md` / `doc_rerank_glm.md` / `doc_rerank_qwen.md`（他人产物，未读）。

**范围外**：`02-stage-vm/` 及以后各 stage；`minix3/minix/servers/*`、`lib/*`、`fs/*`（非内核）。

### 0.3 读取清单

| 类别 | 实际读取 |
|------|---------|
| 文档头部声明 | 上述 35 篇的头部 `>` 声明块全部读完；其中 00–14、16–25、27–33 的**全文标题树**（`##`/`###`）已抽取 |
| 文档全文精读 | `00-kernel-overview.md`（388 行）、`06-todo.md`（818 行，写法范例 + 体检结论来源） |
| C 源码 | `minix3/minix/kernel/` 全量文件树；`main.c` 全文（522 行）；`proc.c`/`system.c`/`clock.c`/`smp.c`/`interrupt.c`/`exception.c`/`pre_init.c`/`pg_utils.c`/`protect.c`/`table.c` 符号级 grep；`minix/include/minix/com.h`（58 个 syscall 号全量） |
| 非 C 制品 | `os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld`、`os/boot-shim/src/*`、`os/libs/minix-boot/src/*`、`os/libs/minix-platform/src/*`、`os/xtask/`、`os/qemu-tests/`、`tools/*.sh` |
| Rust 入口 | `os/kernel/src/lib.rs` 的 `arch_boot*`/`kmain`/`init_*`/`bsp_finish_booting`/`switch_to_user` 符号表；`os/kernel/src/` 全部模块文件名；`os/arch/src/`、`os/libs/` crate 清单 |
| 阶段边界 | `00-master-plan/README.md`（阶段划分与启动因果链，全文）、`edge_todo.md`（头部 + E1/E8 切片进度）、`01-stage-kernel/todo.md`（§0 基线 + §0.5/§0.6 backlog） |

### 0.4 使用过的命令与关键输出（证据摘录）

```text
# 1. 文档规模（wc -l 01-stage-kernel/*.md | sort -k2，节选）
  2263 05-clock-interrupt-init.md      <- 最长
  2019 01-boot-shim-bootstrap.md
  1375 06-proc-init-boot-proc.md
  1329 02-higher-half-kernel.md
  1298 16-smp.md
  1255 04-platform-discovery.md
  1209 12-ipc-core.md
  1197 03-kmain-cstart.md
  1019 08-system-init-boot-finish.md
   949 13-syscall-dispatch.md
   924 17-syscall-process.md
   829 21-syscall-clock.md / 796 20-syscall-device.md
   661 14-exception-interrupt.md / 622 22-privilege.md / 594 09-vm-boot-protocol.md
   506 10-switch-to-user.md / 523 23-ipc-filter.md / 468 24-cross-space-runtime.md
   448 28-usermapped-data.md / 392 27-kernel-utility.md / 388 00-kernel-overview.md
   369 29-kernel-debug.md / 368 30-kernel-profile.md / 293 26-watchdog.md
   222 33-syscall-caller-api.md /  87 18-trap-bridge-design.md
    55 99-global-concepts.md           <- 最短，但被引用最多

# 2. C 源码规模（minix3/minix/kernel/，wc -l *.c *.h | sort -n，节选）
  1980 proc.c / 997 system.c / 563 debug.c / 522 main.c / 312 clock.c
   290 proc.h / 251 proto.h / 210 system.h / 205 smp.c / 177 interrupt.c
   157 profile.c / 112 watchdog.c / 105 priv.h /  94 glo.h /  93 utility.c
  6908 total（另有 arch/i386/ 与 arch/earm/ 两棵架构子树 + system/ 38 个 .c）

# 3. syscall 总数
$ grep -n "NR_SYS_CALLS" minix/include/minix/com.h
  270:#define NR_SYS_CALLS   58      /* number of kernel calls */

# 4. 引用热度（排除 doc_rerank_*，统计 fork-syscall-rewrite 全树 .md）
  99-global-concepts        114 文件 / 273 次   <- 最大热点（却排在目录末尾）
  16-smp                     45 文件 / 186 次
  06-proc-init-boot-proc     50 文件 / 132 次
  11-scheduling-primitives   40 文件 / 132 次
  12-ipc-core                38 文件 / 126 次
  09-vm-boot-protocol        37 文件 / 123 次
  15-clock-timer             37 文件 / 106 次
  14-exception-interrupt     33 文件 / 105 次
  22-privilege               31 文件 /  98 次
  10-switch-to-user          31 文件 /  91 次
  03-kmain-cstart            29 文件 /  87 次
  00-kernel-overview         31 文件 /  80 次
  25-misc-unported           17 文件 /  77 次
  13-syscall-dispatch        32 文件 /  75 次
  18-syscall-copy            38 文件 /  75 次
  17-syscall-process         26 文件 /  72 次
  02-higher-half-kernel      13 文件 /  68 次
  19-syscall-signal          30 文件 /  67 次
  07-cross-space-init        18 文件 /  66 次
  04-platform-discovery      21 文件 /  66 次
  05-clock-interrupt-init    16 文件 /  64 次
  08-system-init-boot-finish 17 文件 /  45 次
  24-cross-space-runtime     16 文件 /  40 次
  31-fpu-context-switching   13 文件 /  35 次
  21-syscall-clock           12 文件 /  30 次
  28-usermapped-data         12 文件 /  23 次
  20-syscall-device          10 文件 /  22 次
  32-stack-tracing            7 文件 /  21 次
  30-kernel-profile           7 文件 /  15 次
  29-kernel-debug             6 文件 /  11 次
  27-kernel-utility           5 文件 /  10 次
  33-syscall-caller-api       5 文件 /   9 次

# 5. 代码注释里的文档引用（跨模块，重建必须同步迁移）
$ grep -rn "01-stage-kernel" --include=*.rs os/
  os/servers/rs/src/ipc_mask.rs:148    -> 01-stage-kernel/23-ipc-filter.md
  os/servers/vm/src/boot.rs:14         -> 01-stage-kernel/09-vm-boot-protocol.md
  os/kernel/src/test_helpers.rs:4      -> 01-stage-kernel/panic-in-drop.md §1
  os/kernel/src/kerninfo.rs:14         -> 01-stage-kernel/28-usermapped-data.md D1
  os/libs/minix-sys/src/arch_trap.rs:3 -> 01-stage-kernel/18-trap-bridge-design.md

# 6. 覆盖缺口探测（正式文档命中，排除 doc_rerank_*）
  do_mcontext   -> 仅 19-syscall-signal.md 一处提及
  do_setgrant   -> 仅 08 / 13 的表格中出现
  do_abort      -> 仅 08 / 13 / 27 的表格中出现
  do_schedule   -> 仅 00 / 08 / 13 / 33 提及
  breakpoints.c-> 正式文档 0 命中
  oxpcie.c     -> 仅 08 / 09 提及
  xtask        -> 正式文档 0 命中（仅 todo.md）
  qemu-tests   -> 散在各篇 §5，无统一篇
  prepare_shutdown / minix_shutdown -> 仅 27 一处提及，无正式篇
```

---

## 1. C 真序（运行时真序重建）

### 1.0 阶段类型判定

**判定：启动链型为主 + 运行期"三入口汇聚"型为辅。**

三条证据：

1. **存在唯一线性启动链且可逐锚点核对**：`pre_init()`（`pre_init.c:217`）→ `kmain()`（`main.c:115`）
   → `cstart()`（`main.c:403`）→ `proc_init()`（`proc.c:119`）→ `system_init()`（`system.c:168`）
   → `bsp_finish_booting()`（`main.c:38`）→ `switch_to_user()`（`proc.c:299`）。每一步有唯一前驱
   与唯一后继。
2. **启动完成后内核没有主循环**：`switch_to_user()` 末尾是 `NOT_REACHABLE`（`proc.c:473`），
   内核代码只以三种方式被调用（异常/中断、系统调用、进程切换）。因此"运行期"不是线性序列，
   而是**多个并行入口汇聚到同一出口**，必须按"汇聚点 + 触发时机"组织。
3. **58 个系统调用是典型并行体**：`com.h:270` `NR_SYS_CALLS 58`，天然分成
   进程生命周期 / 跨空间数据 / 信号 / 设备 / 时间 / 地址空间 / 杂项 七组（见 §4.3），
   组内成员之间无顺序依赖。现状 17→18→19→20→21 的排法是伪线性。

### 1.1 真序表（启动段）

> 每条可 `sed -n` 核对。路径相对于 `minix3/minix/kernel/`。

| # | 动作 | C 函数与文件锚点 | 说明 |
|---|------|-----------------|------|
| S01 | 固件加载并跳到内核入口 | `arch/i386/pre_init.c:217 pre_init()` | x86 为 multiboot/GRUB，ARM 为 U-Boot；Rust 侧改 UEFI/OpenSBI |
| S02 | 解析启动参数与内存布局 | `pre_init.c:94 get_parameters()`；`pre_init.c:35 mb_set_param()` | 填 `kinfo.memmap[]` / `kinfo.mbi` / `kinfo.param_buf` |
| S03 | 扣除内核自身与模块占用 | `pg_utils.c:32 cut_memmap()` | 从可用内存挖掉 kernel image 与 modules |
| S04 | 建立恒等映射 | `pg_utils.c:162 pg_identity()` | 低地址 1:1，保证开分页瞬间取指不断流 |
| S05 | 映射内核到高地址 | `pg_utils.c:186 pg_mapkernel()` | 高半内核的页表侧准备 |
| S06 | 装载页目录并开分页 | `pg_utils.c:247 pg_load()` → `pg_utils.c:204 vm_enable_paging()` | CR3 + CR0.PG |
| S07 | 切栈 + 跳高地址 | `arch/i386/head.S`（`k_initial_stktop`） | 有去无回；Rust 侧 `HigherHalf::jump_to_kmain()` |
| S08 | `kmain()` 入口 | `main.c:115 kmain`；`:128 memcpy(&kinfo,…)`；`:124 assert(bss_test==0)`；`:142 kernel_may_alloc=1` | |
| S09 | `cstart()` 第一步：保护结构 | `main.c:411 prot_init()` → `arch/i386/protect.c:321 prot_init()`，内含 `protect.c:260 idt_init()` | GDT/IDT/TSS |
| S10 | `cstart()` 第二步：时钟变量初始化 | `main.c:418 init_clock()` → `clock.c:48 init_clock()` | 只初始化变量与 `system_hz`，**不启动定时器** |
| S11 | `cstart()` 第三步：环境变量解析 | `main.c:414/421/443/455/464 env_get()` | `verboseboot`/`ac_layout`/`no_apic`/`watchdog`/`no_smp` |
| S12 | `cstart()` 第四步：中断控制器 | `main.c:472 intr_init(0)` → `arch/i386/i8259.c` | PIC 编程；ARM 侧 GICv3，RISC-V 侧 PLIC |
| S13 | `cstart()` 第五步：架构特定初始化 | `main.c:474 arch_init()` → `arch/i386/arch_system.c:246 arch_init()`（含 `acpi_init()`）；ARM 侧调 `bsp_init()` | |
| S14 | 取 BKL | `main.c:149 BKL_LOCK()` | **自此到 `switch_to_user()` 全程持锁** |
| S15 | 清空进程表 + IPC 过滤池 | `main.c:157 proc_init()` → `proc.c:119 proc_init()`；`main.c:158 IPCF_POOL_INIT()` | |
| S16 | 遍历 boot image，逐槽填充 | `main.c:165-272`；`table.c:44-64 image[NR_BOOT_PROCS]` | 17 项：5 内核 task + 12 模块 |
| S16a | 分配特权结构（仅可立即调度者） | `main.c:196-200` 判定 + `get_priv()`（`system.c:274`） | 仅 kernel task / RS / VM |
| S16b | 按角色授予能力 | `main.c:203-234`（`VM_F`/`TSK_F`/`RSYS_F`，`SRV_T`/`CSK_T`，`SRV_M`/`TSK_M`，`SRV_KC`/`TSK_KC`）；`main.c:244 fill_sendto_mask()`；`main.c:247-249 s_k_call_mask` | |
| S16c | 不可立即调度者禁止运行 | `main.c:253 RTS_SET(rp, RTS_NO_PRIV \| RTS_NO_QUANTUM)` | 其余服务等 RS 授权 |
| S16d | 架构态初始化（含 VM ELF 加载） | `main.c:257 arch_boot_proc()` → `protect.c:388 arch_boot_proc()` | **只有 VM 真正加载 ELF** |
| S16e | 非 VM 进程挂起等页表 | `main.c:264-267` `RTS_VMINHIBIT` + `RTS_BOOTINHIBIT` | |
| S16f | 全体挂 `RTS_PROC_STOP`、清 `RTS_SLOT_FREE` | `main.c:269-270` | "就位但暂停" |
| S17 | 架构后初始化（跨空间能力） | `main.c:283 arch_post_init()` | 记录 bootstrap 页表地址、设置 `ptproc`/`freepdes`（32 位）；Rust 侧简化为确认 direct_map 就绪 |
| S18 | 内存子系统初始化 | `main.c:293 memory_init()` | |
| S19 | 系统调用表初始化 | `main.c:295 system_init()` → `system.c:168 system_init()`；`system.c:52 call_vec[NR_SYS_CALLS]`；`system.c:189` 清零 | |
| S20 | 回收 bootstrap 内存 | `main.c:301 add_memmap(&kinfo, kinfo.bootstrap_start, kinfo.bootstrap_len)` | |
| S21 | SMP 初始化（条件分支） | `main.c:303-317`：`smp_single_cpu_fallback()` 或 `smp_init()`；`smp.c:27 big_kernel_lock`；AP 握手 `smp.c:30 wait_for_APs_to_finish_booting()` | 由 `config_no_apic` / `config_no_smp` 决定 |
| S22 | 完成启动（BSP） | `main.c:324 bsp_finish_booting()` → `main.c:38`；`:56-57` bill_ptr/proc_ptr 指向 idle；`:58 announce()`；`:64-66` 只清非内核 task 的 `RTS_PROC_STOP`；`:71 cycles_accounting_init()`；`:73 boot_cpu_init_timer(system_hz)`（`clock.c:294`）；`:78 fpu_init()`；`:105 kernel_may_alloc=0` | |
| S23 | 进入调度循环（永不返回） | `main.c:107 switch_to_user()` → `proc.c:299`；`proc.c:338 while(!(p=pick_proc()))`；`proc.c:176 idle()` | **汇聚点** |

### 1.2 真序表（运行段：三条入口 → 一个出口）

| # | 入口 | C 函数与文件锚点 | 出口 |
|---|------|-----------------|------|
| R01 | CPU 异常 / 硬件中断 | `arch/i386/exception.c:180 exception_handler()`；`interrupt.c:116 irq_handle()`；`interrupt.c:29 put_irq_handler()` 注册的 hook 链 | 回 `switch_to_user()` |
| R02 | 系统调用（kernel call） | `system.c:136 kernel_call()` → `system.c:95 kernel_call_dispatch()` → `system.c:117-118 (*call_vec[call_nr])(caller,msg)` → `system.c:59 kernel_call_finish()` | 同上 |
| R03 | IPC 原语 | `proc.c:599 do_ipc()` → `mini_send()`（`proc.c:870`）/ `mini_receive()` / `mini_notify()`（`proc.c:1122`）/ `mini_senda()` → `proc.c:263 delivermsg()` | 同上 |
| R04 | 时钟中断（R01 子类，调度心跳） | `clock.c:70 timer_int_handler()` → 会计/定时器/quantum → `proc.c:1893 proc_no_time()` | 同上 |
| R05 | VMSUSPEND 恢复（R02 续体） | `proc.c:234 vm_suspend()` → VM 处理 → `system.c:612 kernel_call_resume()` → `system.c:630 kernel_call_dispatch()` 重放 | 同上 |
| R06 | 调度原语（被以上所有路径调用） | `proc.c:1595 enqueue()` / `proc.c:1670 enqueue_head()` / `proc.c:1716 dequeue()` / `proc.c:1785 pick_proc()` | — |
| R07 | 跨 CPU 协同 | `smp.c:63 smp_schedule()` → `smp.c:75 smp_schedule_sync()` → `smp.c:156 smp_sched_handler()` | — |
| R08 | 信号注入（内核内部） | `system.c:389 cause_sig()` | 走 R01/R02 |
| R09 | 终态 | `main.c:351 prepare_shutdown()` → `main.c:368 minix_shutdown()`；`utility.c` panic 路径 | 不返回 |

### 1.3 序差表（教学序 vs 运行时序）

| 编号 | 主题 | 运行时序事实（锚点） | 教学序选择 | 理由 | 回指补偿位置 |
|------|------|-------------------|-----------|------|-------------|
| D-1 | VM 启动协议 `SYS_VMCTL` | VM 必须先被调度到用户态才能发请求，即 `switch_to_user()`（`main.c:107`）**之后** | 新目录 **19-syscall-vmctl**，位于 10 之后 | 旧目录放 09（08 后、10 前），其头部自写"前置: 08 + 10"，是显式前向引用 | 09 末尾一句"VM 协商触发于调度循环启动后，见 19" |
| D-2 | 特权结构 `struct priv` | boot 循环里就分配（`main.c:196-250`） | 启动侧 07 讲"boot 期如何分配"；语义侧 **16-privilege-model**（syscall 组之前） | 旧 22 排在 17/19/20/21 之后，被这四篇前向引用 | 07 能力授予段末尾指向 16 |
| D-3 | SMP / BKL | `smp_init()` 在 `system_init()` 后、`bsp_finish_booting()` 前（`main.c:311`）；IPI 与 BKL 释放属运行期 | 拆两处：启动侧 09；运行侧 **11-kernel-execution-model** | 旧 16 排在 15 之后，而 17 依赖它；拆开后两边都不跨阶段前向引用 | 11 讲"释放窗口"时脚注指向 13；13 反向回指 11 |
| D-4 | `ClockState` / 定时器 | `init_clock()`（`clock.c:48`）在 `cstart()` 内（启动链第 10 步）；定时器运行属 R04 | 启动侧 **06** 只讲 `init_clock`/`intr_init`/`arch_init`；运行侧 **13-clock-and-timers** | 旧目录把 ~500 行 `ClockState` 塞进 05（2263 行，全目录最长） | 06 末尾一句"数据结构与运行语义见 13" |
| D-5 | 跨空间 direct_map | 32 位在 `pg_utils.c`/`memory.c` 建临时窗口；Rust 在 `arch_boot_impl` 建立、阶段 D 确认 | 启动侧 **08**；运行侧消费 **18** | 旧 07 与 24 两处讲，18 又引用 24（前向引用） | 08 末尾指向 18 |
| D-6 | 调度原语 | 启动期不调用；首次调用在 `switch_to_user()` 内（`proc.c:338`） | 归入 **10-switch-to-user** 的协作段（旧 11 并入） | 与真序一致；调度原语的唯一调用者就是调度循环 | — |
| D-7 | 异常 / 中断分派 | 硬件设施在 `cstart()` 就绪（`prot_init`/`intr_init`），运行期才发生 | 启动侧 04/06；运行侧 **12** | 与真序一致 | — |
| D-8 | 关闭与重启 | 运行期最后一步（`main.c:351/368`），旧目录**无文档** | 新建 **25-kernel-utility-and-terminal §3**（支线，与 panic/kputc 同属"内核终态"） | 属"关闭与退出"固定清单项 | 25 §3 内部与 panic 一节并列 |

---

## 2. 知识点全集（存量池 + 新增）

> 编号 `K-nnn`，stage 内唯一；多 AI 汇总时对齐键 = **名称 + 锚点**。
> 来源 `S` = 存量（现有文档承载）；`N` = 新增（C 源码 / 非 C 制品 / OS 理论承载，现有文档没讲或只一句话带过）。
> "主讲述点"列：重复出现时，新目录中保留哪一篇做主讲述。

### 2.A 概念与心智模型（K-001 ~ K-022）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 新归属 | 主讲述点 |
|------|------|------|------|---------|------|---------|--------|---------|
| K-001 | 微内核内核 vs 用户态服务 | 概念 | S | 00 §1.1/§4.1 | `00:13-44,311-322` | 内核为什么没有事件循环 | 00 | ✅ 00 |
| K-002 | 运行态实体三维度（IPC identity / code+state / execution context） | 概念 | S | 00 §1.4.1 | `00:60-101`；`main.c:64-66` | kernel task 有没有自己的执行流 | 01 | ✅ 01 |
| K-003 | 内核 task（5 个）vs 系统服务器 | 概念 | S | 00 §1.4.1；06 §1.1.4 | `com.h:47-56`；`table.c:44-51` | CLOCK/SYSTEM 为什么不被调度器恢复 | 01 | ✅ 01（06 证据链并入） |
| K-004 | IDLE 是唯一自持 execution context 的内核实体 | 概念 | S | 00 §1.4.1 | `proc.c:176-193` | idle 空转不是"没进程" | 01/10 | 01 概念，10 实现 |
| K-005 | HARDWARE / ASYNCM 是纯 IPC 身份 | 概念 | S | 00 §1.4.1 | `com.h:47,52`（`#define HARDWARE KERNEL`） | 通知来源如何被寻址 | 01 | ✅ 01 |
| K-006 | 三条进入路径、一个出口 | 概念 | S | 00 §1.4/§4.1；10 §1.2 | `proc.c:299`；`main.c:107-108` | 内核代码为什么总是"被调用" | 00/11 | ✅ 11（00 只给全景） |
| K-007 | 进程的本质：断点续传 | 概念 | S | 06 §1.1.1 | `proc.h` | 进程在内核里是什么 | 01 | ✅ 01 |
| K-008 | 进程五组成部分 | 概念 | S | 06 §1.1.3 | `proc.h` | 进程表字段为什么分成那些组 | 01 | ✅ 01 |
| K-009 | CPU 四问（寄存器初值/入口/栈与参数/地址空间） | 概念 | S | 06 §1.2；03 §1.4 | `protect.c:388`；`proc.h stackframe_s` | 让进程"第一次跑起来"需要什么 | 07 | ✅ 07 |
| K-010 | 特权级与信任边界（ring/EL/privilege mode） | 概念 | S | 03 §1.1/§1.2 + 附录 A | `03:24-59,1079-1193` | 用户程序为什么写不了内核内存 | 01/04 | 04 |
| K-011 | 异常 vs 中断（同步 vs 异步） | 概念 | S | 05 §1.0；14 §1.1 | `05:12-53`；`14:12-24` | 两类"意外事件"为什么分流 | 12 | ✅ 12（05 改引用） |
| K-012 | 中断上下文 vs 进程上下文 | 概念 | S | 00 §5.3 | `00:357-364` | 什么时候不能睡眠 | 11 | ✅ 11 |
| K-013 | 地址空间与 CR3 / 页表所有权 | 概念 | S | 00 §1.3/§5.2；09 §1.3 | `system/do_vmctl.c` | 内核为什么不自己管页表 | 01/19 | 19 |
| K-014 | 无共享内存的进程间通信模型 | 概念 | S | 12 §1.1 | `12:12-24` | 微内核为什么只提供"通信" | 15 | ✅ 15 |
| K-015 | 策略（VM）与机制（Kernel）分离 | 概念 | S | 00 §5.2 | `00:353-356` | 页表决策为什么跨两个执行主体 | 00 | ✅ 00 |
| K-016 | 微内核自举：VM 鸡生蛋 | 概念 | S | 06 §1.4.2 | `06:301-366`；`main.c:203-211` | VM 为什么是第一个被加载的用户程序 | 07 | ✅ 07 |
| K-017 | 编译期定容：为什么内核表不能 `Vec::new()` | 概念+约束 | S | 06 §2.0 | `06:453-491`；`config.h` | 内核存储形态的因果链 | 07 | ✅ 07 |
| K-018 | 内核直接操作硬件 vs 服务通过 IPC 请求 | 概念 | S | 00 §1.1/§4 | `00:19-21` | 权限边界在哪 | 00 | ✅ 00 |
| K-019 | 时间基准三分：monotonic / realtime / boottime | 概念 | **N** | 15/21 隐含未明写 | `clock.c:178/187/195/203/212/220` | C 有三种"现在"，各自被谁用 | 13 | ✅ 13 |
| K-020 | 能力（capability）而非 uid | 概念 | S | 22 §1.1/§1.3；06 §1.3.2 | `priv.h` | Minix3 怎么表达"谁能干什么" | 16 | ✅ 16 |
| K-021 | 通知（notify）永不阻塞、永不丢失 | 概念 | S | 12 §1.2/§2.6 | `proc.c:1122` | 异步与同步消息语义差 | 15 | ✅ 15 |
| K-022 | 驱动即进程与 IRQ 解耦 | 概念 | **N** | 20 隐含未明写 | `interrupt.c:29`；`do_irqctl.c` | 驱动为什么不能自己装 handler | 22 | ✅ 22 |

### 2.B 启动链机制（K-023 ~ K-048）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 新归属 | 主讲述点 |
|------|------|------|------|---------|------|---------|--------|---------|
| K-023 | multiboot 协议与启动参数传递 | 接口 | S | 01 §2.0/§2.1 | `pre_init.c:94/35` | 内核怎么知道内存多大 | 02 | ✅ 02 |
| K-024 | 内存映射的构造与裁剪 | 机制 | S | 01 §2.3；08 §4.5 | `pg_utils.c:32/86`；`main.c:301` | 可用内存清单怎么算出来 | 02/09 | 02 |
| K-025 | 恒等映射的作用与生命周期 | 机制 | S | 01 §2.3/§3.4；02 §1.1 | `pg_utils.c:162` | 开分页那一瞬间为什么不崩 | 03 | ✅ 03 |
| K-026 | 高半内核：为什么要切到高地址 | 机制 | S | 02 §1.2 | `02:27-36` | 用户低地址与内核高地址如何共存 | 03 | ✅ 03 |
| K-027 | 切栈与跳指令必须成对 | 机制+不变量 | S | 02 §1.1/§2.3 | `02:12-26,155-165` | 只跳不切会怎样 | 03 | ✅ 03 |
| K-028 | 链接脚本：VMA/LMA 与 `AT()` | 工具 | S | 02 §2.1/§3.1 | `arch/i386/kernel.lds`；`os/kernel/src/arch/*/link.ld` | 链接地址与加载地址为何不同 | 03 | ✅ 03 |
| K-029 | 内核 ELF 加载（段拷贝 + BSS 清零） | 机制 | S | 02 §4.2；06 §4.2 | `os/kernel/src/lib.rs:297 arch_boot_impl` | 镜像怎么变可执行 | 03 | ✅ 03 |
| K-030 | `KernelInfo` / `kinfo_t`：boot→kernel 唯一契约 | 接口 | S | 01 §3.5/§4.1；28 §3.2 | `os/libs/minix-boot/src/kernel_info.rs` | 启动侧与内核侧的边界 | 02/32 | 02 |
| K-031 | BSS 清零断言 | 约束 | S | 03 §2.1 | `main.c:121-125 bss_test` | 内核为什么要自己验 BSS | 04 | ✅ 04 |
| K-032 | `kernel_may_alloc` 窗口 | 约束 | S | 03 §2.1；06 §3.8；08 §1.6 | `main.c:142`（开）/ `main.c:105`（关） | 内核什么时候可以动内存 | 04/09 | 04 开、09 关 |
| K-033 | `prot_init()`：GDT/IDT/TSS | 机制 | S | 03 §2.3–§2.6 | `protect.c:321/260` | 内核怎么给 CPU 装保护设施 | 04 | ✅ 04 |
| K-034 | 为什么不能用固件留下的保护结构 | 概念 | S | 03 §1.6 | `03:228-237` | 为什么必须重装 GDT | 04 | ✅ 04 |
| K-035 | 三架构保护结构对照 | 架构演进 | S | 03 §1.5/§4.4 | `03:211-227,855-869` | 三架构怎么表达同一件事 | 04 | ✅ 04 |
| K-036 | 为什么必须切到内核栈（威胁模型） | 概念 | S | 03 附录 A | `03:1079-1193` | 用户栈为什么不能用于内核 | 04 | ✅ 04 |
| K-037 | 硬件发现：硬编码地址的不可持续性 | 概念 | S | 04 §1.1 | `04:26-44` | 为什么需要 ACPI/DTB | 05 | ✅ 05 |
| K-038 | ACPI 作为 x86 硬件描述来源 | 机制 | S | 04 §2.1 | `arch/i386/acpi.c:acpi_init()`；`arch_system.c:246` | x86 硬件参数从哪来 | 05 | ✅ 05 |
| K-039 | DTB / FDT 作为 ARM/RV 硬件描述来源 | 机制 | S | 04 §2.2 | `arch/earm/arch_system.c`（`bsp_init()`） | ARM/RV 硬件参数从哪来 | 05 | ✅ 05 |
| K-040 | `PlatformDesc` trait 与三架构统一抽象 | 机制 | S | 04 §3.1–§3.6 | `os/libs/minix-platform/src/{desc,kind,global}.rs` | Rust 侧怎么不硬编码 | 05 | ✅ 05 |
| K-041 | QEMU `virt` 兜底描述符 | 机制 | S | 04 §3.6 | `.../arch/*/QemuVirtDesc` | 没有固件表时怎么跑起来 | 05 | ✅ 05 |
| K-042 | 解析职责切分：boot-shim 定位、kernel 解析 | 机制 | S | 04 §3.1 | `04:216-252` | 为什么不让 kernel 直接读 UEFI | 05 | ✅ 05 |
| K-043 | `init_clock()`：变量初始化而非启动定时器 | 机制 | S | 05 §2.1 | `clock.c:48` | 时钟在启动链哪一步"活" | 06 | ✅ 06 |
| K-044 | `intr_init()`：中断控制器编程 | 机制 | S | 05 §2.2/§2.3 | `i8259.c`；`apic.c`；GICv3/PLIC | 中断怎么从设备到 CPU | 06 | ✅ 06 |
| K-045 | `arch_init()`：架构特定收尾 | 机制 | S | 05 §2.4/§2.5 | `arch/i386/arch_system.c:246` | 还有哪些硬件必须这步就绪 | 06 | ✅ 06 |
| K-046 | 早期控制台（EarlyConsole） | 机制 | S | 01 §6.1；05 §3.4/§4.14；27 §3.2 | `os/arch/.../EarlyConsole` | 串口就绪前内核怎么说话 | 06/25 | 06 初始化，25 panic 路径 |
| K-047 | 环境变量驱动的配置 | 机制 | S | 05 §2.6 | `main.c:414-469` | 启动参数怎么变开关 | 06 | ✅ 06 |
| K-048 | `proc_init()` 清表 | 机制 | S | 06 §1.5.1/§4.7 | `proc.c:119`；`main.c:157` | 进程表白板长什么样 | 07 | ✅ 07 |

### 2.C 进程、特权与状态（K-049 ~ K-066）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 新归属 | 主讲述点 |
|------|------|------|------|---------|------|---------|--------|---------|
| K-049 | `proc[NR_TASKS+NR_PROCS]` 静态数组 + `proc_addr()` | 数据结构 | S | 06 §2.1.0 | `proc.h`；`proc.h:269 proc_addr` | 身份即索引 | 07 | ✅ 07 |
| K-050 | `p_nr` 与 `p_endpoint`：槽位索引 vs 通信身份 | 数据结构 | S | 99 §1；06 §2.1.1；17 §1.2 | `proc.h`；`include/minix/endpoint.h` | 为什么要两个标识 | 01 | ✅ 01 |
| K-051 | endpoint 编码布局（generation << 15 \| slot） | 数据结构 | S | 99 §1.2 | `99:17-29` | 旧 endpoint 为什么不会误命中 | 01 | ✅ 01 |
| K-052 | 特殊 endpoint `ANY`/`NONE`/`SELF` | 数据结构 | S | 99 §1.3 | `99:30-39` | 通配与自我引用怎么表达 | 01 | ✅ 01 |
| K-053 | 进程表字段分组（标识/调度/IPC/上下文/记账/生命周期） | 数据结构 | S | 06 §2.1.1–§2.1.6 | `proc.h` | 60+ 字段按 OS 语义怎么归类 | 07（导航，纯 C 侧） | ✅ 07 |
| K-054 | `priv[NR_SYS_PROCS=64]` + `ppriv_addr[]` 双表桥接 | 数据结构 | S | 06 §2.2.0；22 §1.4 | `priv.h`；`system.c:274` | 特权槽为什么比进程槽少 | 07/16 | 07 存储，16 语义 |
| K-055 | `USER_PRIV` 共享：普通进程不占独立特权槽 | 数据结构 | S | 06 §2.2.0 | `priv.h`；`system.c:918-989` | 分治策略省了什么 | 07 | ✅ 07 |
| K-056 | 静态特权段 vs 动态特权段 | 数据结构 | S | 06 §2.2.0 | `system.c:274`；`main.c:200 static_priv_id` | boot 期与运行期分配差别 | 07/16 | 16 |
| K-057 | `p_rts_flags` 16 位全集 | 数据结构 | S | 06 §2.1.6/§3.3；11 §1.1；99 | `proc.h` | "不能跑"的原因有几种 | 01/07 | 01 位表，07 boot 期 |
| K-058 | 不变量 `p_rts_flags == 0 ⟺ runnable` | 约束 | S | 06 §1.3.3；11 §1.1 | `06:191-299`；`11:12-26` | 调度正确性依赖哪条不变量 | 01/10 | 01 声明，10 维护 |
| K-059 | `RTS_SET/UNSET` 与 enqueue/dequeue 的隐藏联动 | 机制 | S | 11 §2.7/§3.5 | `11:212-236`；`proc.c:1595/1716` | 改状态位为什么会动队列 | 10 | ✅ 10 |
| K-060 | 特权位 `s_flags`（PREEMPTIBLE/BILLABLE/SYS_PROC…） | 数据结构 | S | 22 §1.2；06 §2.2.1 | `priv.h` | 进程"角色"怎么表达 | 16 | ✅ 16 |
| K-061 | 三类掩码 `s_trap_mask`/`s_ipc_to`/`s_k_call_mask` | 数据结构 | S | 22 §1.3；23 §2.4 | `priv.h:86-87`；`system.c:111`；`com.h:272` | 三类能力检查各管什么 | 16 | ✅ 16 |
| K-062 | I/O 端口 / IRQ / 内存范围授权表 | 数据结构 | S | 22 §1.5；20 §1.2 | `priv.h`；`do_devio.c`；`do_irqctl.c` | 驱动能碰哪些硬件 | 16/22 | 16 结构，22 检查 |
| K-063 | 五能力模板（`VM_F`/`TSK_F`/`RSYS_F`/`SRV_*`/`TSK_*`） | 机制 | S | 06 §4.5；22 §4.4 | `main.c:203-234`；`os/kernel/src/capability.rs` | boot 期怎么一次性发完权限 | 07/16 | 16 定义，07 授予 |
| K-064 | boot image 表 `image[NR_BOOT_PROCS]` 17 项 | 数据结构 | S | 06 §2.3/§1.4.1 | `table.c:44-64` | 系统一开始有哪些进程、顺序如何 | 07 | ✅ 07 |
| K-065 | boot image 顺序 ≠ proc 号顺序 | 约束 | S | 06 §1.4.1 | `table.c:44-64` vs `com.h` 的 `*_PROC_NR` | ds 排第一但编号 6 | 07 | ✅ 07 |
| K-066 | multiboot module list ↔ boot image 的 `i - NR_TASKS` 对应 | 接口 | S | 06 §1.4.1/§2.3 | `main.c:179-184` | ELF 物理地址从哪来 | 07 | ✅ 07 |

### 2.D 运行期机制（K-067 ~ K-111）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 新归属 | 主讲述点 |
|------|------|------|------|---------|------|---------|--------|---------|
| K-067 | `switch_to_user()` 主循环 | 机制 | S | 10 §2.1/§4.1 | `proc.c:299-477`；`lib.rs:2913` | "接下来跑谁"在哪决定 | 10 | ✅ 10 |
| K-068 | `idle()`：CPU 的休眠姿态 | 机制 | S | 10 §2.2/§4.3 | `proc.c:176-193` | 没有就绪进程时 CPU 干嘛 | 10 | ✅ 10 |
| K-069 | `pick_proc()` 与 16 级优先级队列 | 机制 | S | 11 §2.5/§1.2 | `proc.c:1785`；`sched.rs` | 谁先跑 | 10 | ✅ 10（从 11 移入） |
| K-070 | `enqueue`/`enqueue_head`/`dequeue` | 机制 | S | 11 §2.2–§2.4 | `proc.c:1595/1670/1716` | 队列怎么维护 | 10 | ✅ 10（从 11 移入） |
| K-071 | `proc_no_time()` 与 quantum 耗尽 | 机制 | S | 11 §2.6 | `proc.c:1893` | 时间片谁扣 | 13 | ✅ 13（时钟驱动） |
| K-072 | 抢占与 `RTS_PREEMPTED` | 机制 | S | 11 §2.2/§3.5 | `proc.c:1639` | 高优先级入队会怎样 | 10 | ✅ 10（从 11 移入） |
| K-073 | BKL 与释放窗口 | 机制 | S | 00 §1.5.1；13 §1.5；16 | `smp.c:27`；`arch_clock.c:92,107,118`；`smp.c:75` | 内核什么时候真的并发 | 11 | ✅ 11（三处合并） |
| K-074 | per-CPU 数据（`cpulocals.h`） | 数据结构 | S | 16；07 §2.1 | `cpulocals.h`；`main.c:56-57` | 哪些状态每 CPU 一份 | 11 | ✅ 11 |
| K-075 | IPI 与跨 CPU 调度 | 机制 | S | 16 | `smp.c:63/75/114/123/132/142` | 怎么让别的 CPU 重新调度 | 11 | ✅ 11 |
| K-076 | AP 启动握手与 trampoline | 机制 | S | 16；01/02 提及 | `smp.c:30`；`arch/i386/trampoline.S` | 第二个 CPU 怎么起来 | 09/11 | 09 启动侧，11 协议 |
| K-077 | `SyncUnsafeCell` + `BklProtected` + `BklSection` | 机制 | S | todo.md §0；06 §4.3 | `os/kernel/src/globals.rs`；`smp.rs` | Rust 侧全局状态怎么可证安全 | 11 | ✅ 11 |
| K-078 | 全局状态获取约定 `*_with(section)` | 接口 | S | lib.rs 符号表 | `lib.rs:1599/1617/1737/1769/1811/1918` | 拿进程表为什么要传锁凭证 | 11 | ✅ 11 |
| K-079 | 异常帧：CPU 压了什么 | 数据结构 | S | 14 §1.3 | `14:41-56` | trap 进来时内核拿到什么 | 12 | ✅ 12 |
| K-080 | 异常分流主干与四种嵌套恢复点 | 机制 | S | 14 §2.1/§2.2/§3.4 | `exception.c:180/132` | 内核里再出错怎么办 | 12 | ✅ 12 |
| K-081 | 页错误转发 VM | 机制 | S | 14 §2.3/§4.6；00 §1.3 | `exception.c:49`；`proc.c:234` | 缺页为什么不是 SIGSEGV | 12/19 | 12 分流 |
| K-082 | 异常→信号映射 `ex_data[]` | 数据结构 | S | 14 §2.4/§4.5 | `arch/i386/exception.c` | 非法指令为什么变 SIGILL | 12/21 | 12 映射 |
| K-083 | IRQ hook 链与 `irq_handle` | 机制 | S | 14 §2.5；20 §1.1 | `interrupt.c:29/116/161/169` | 中断怎么变驱动的通知 | 12/22 | 12 分发 |
| K-084 | 中断控制器抽象（hw_intr → `InterruptController`） | 机制 | S | 14 §2.6；05 §3.3 | `include/hw_intr.h`；`minix-platform` | 三架构中断控制器怎么统一 | 06/12 | 12 运行侧 |
| K-085 | `ClockState`/`TimerAction`/`TimerEntry`/`TimerId` | 数据结构 | S | 05 §4.1（~500 行）；15；21 | `os/kernel/src/clock.rs` | 时间设施由哪些结构组成 | 13 | ✅ 13（**从 05 移出**） |
| K-086 | `timer_int_handler()`：100Hz 心跳 | 机制 | S | 15 | `clock.c:70` | 一次时钟中断干几件事 | 13 | ✅ 13 |
| K-087 | 虚拟/性能定时器与同步闹钟 | 机制 | S | 15；21 §1.2/§1.4 | `do_setalarm.c`；`do_vtimer.c`；`clock.c:229` | 三种定时器差别 | 13/23 | 13 机制 |
| K-088 | 负载平均 `load_update()` | 机制 | S | 15 | `clock.c:260` | 系统忙不忙怎么算 | 13 | ✅ 13 |
| K-089 | `kernel_call_dispatch` 与 `call_vec` | 机制 | S | 13 §2.3/§4.3/§4.4；08 §4.2 | `system.c:52/95/117-118/136` | 58 个调用怎么路由 | 14 | ✅ 14 |
| K-090 | syscall vs IPC：独立 trap 入口的两种语义 | 概念 | S | 13 §1.2 | `13:26-49` | syscall 为什么不走 IPC 原语 | 14 | ✅ 14 |
| K-091 | `KERNEL_CALL` 归一化 | 接口 | S | 13 §1.3/§3.3 | `13:50-88` | 调用号怎么统一 | 14/32 | 14 |
| K-092 | `kernel_call_finish` 与回复路径 | 机制 | S | 13 §4.5 | `system.c:59` | 结果怎么回用户态 | 14 | ✅ 14 |
| K-093 | VMSUSPEND 挂起-恢复协议 | 机制 | S | 13 §1.4；24 §1.3/§2.8 | `proc.c:234`；`system.c:612` | 等 VM 时怎么不丢状态 | 14/18 | 14 框架 |
| K-094 | 调用者身份：`struct proc *` → `ProcNr` 句柄 | 接口 | S | 33 全文 | `33:11-46`；`proc_table.rs:caller_slot_mut`；`do_schedule.c` | 别名契约为什么必须降级为值 | 14 | ✅ 14（33 并入） |
| K-095 | BadCall 与 errno 通道 | 接口 | S | 13 §3.4；25 §3.2 | `system.c`；`minix-types` `Errno` | 非法调用怎么报错 | 14/32 | 14 |
| K-096 | IPC 六原语与阻塞行为分类 | 机制 | S | 12 §1.2 | `12:25-41` | 六个原语差别 | 15 | ✅ 15 |
| K-097 | `mini_send`/`mini_receive` 三级检查 | 机制 | S | 12 §2.4/§2.5/§4.3/§4.4 | `proc.c:870` | 一次同步 IPC 走几步 | 15 | ✅ 15 |
| K-098 | `mini_notify` 与 pending 位图 | 机制 | S | 12 §2.6 | `proc.c:1122/843/852` | 通知为什么不丢 | 15 | ✅ 15 |
| K-099 | 死锁检测：跟随 `P_BLOCKEDON` 动态链 | 机制 | S | 12 §1.5/§2.7/§4.5 | `proc.c` deadlock | 循环等待怎么提前发现 | 15 | ✅ 15 |
| K-100 | `delivermsg` 延迟拷贝 | 机制 | S | 12 §1.3/§2.8/§3.8；13 §3.2 | `proc.c:263`；`system.c:141` | 消息为什么分两段拷 | 15 | ✅ 15 |
| K-101 | `mini_senda`/`try_deliver_senda` 批量异步 | 机制 | S | 12 §2.9/§3.10 | `proc.c:1200/1510` | 一次发多条怎么做 | 15 | ✅ 15 |
| K-102 | 发送者队列 `p_caller_q` | 数据结构 | S | 12 §3.2/§4.9 | `proc.h` | 谁在等我 | 15 | ✅ 15 |
| K-103 | IPC 权限四层检查 | 机制 | S | 12 §4.7；23 §2.3–§2.8 | `system.c:111`；`system.c:803-874` | 一条消息要过几道关 | 17 | ✅ 17（**从 12 抽出**） |
| K-104 | IPC 过滤三层模型（掩码/规则/状态报告） | 机制 | S | 23 §1.1/§2.x | `ipc.h:14-48`；`ipc_filter.h:19-41` | 黑白名单怎么生效 | 17 | ✅ 17 |
| K-105 | 过滤池与 `IPCF_POOL_INIT` | 数据结构 | S | 23 §3.5；08 | `main.c:158` | 过滤规则存在哪 | 17 | ✅ 17 |
| K-106 | 跨空间拷贝三阶段抽象 | 机制 | S | 24 §1.1；18 §1.x | `include/minix/syslib.h`；`do_copy.c` | 内核怎么读别的进程内存 | 18 | ✅ 18（18+24 合并） |
| K-107 | grant 授权拷贝（safecopy 家族） | 机制 | S | 18 §2.2/§4.3 | `do_safecopy.c`；`grant.rs` | 授权凭证怎么验 | 18 | ✅ 18 |
| K-108 | `umap`/`umap_remote`/`vumap` 地址翻译 | 机制 | S | 18 §2.3/§2.4/§4.4/§4.5 | 三个 do_*.c | 虚拟地址怎么变物理地址 | 18 | ✅ 18 |
| K-109 | Direct Map：一行加法替代 createpde 临时映射 | 机制+演进 | S | 07 §1.3；18 §1.5；24 | `pg_utils.c:267`；`os/kernel/src/pte_walk.rs` | 64 位下为什么不需要临时窗口 | 08/18 | 08 建立 |
| K-110 | `SYS_VMCTL` 协商协议与子命令表 | 接口 | S | 09 全文 | `do_vmctl.c`；`arch_do_vmctl.c` | VM 与内核怎么谈页表 | 19 | ✅ 19 |
| K-111 | ptproc 跟踪与 `set_active_root` | 机制 | S | 09 §4.8；07 §2.1 | `os/kernel/src/vm.rs`；`lib.rs:2251/2324` | 内核怎么记住当前地址空间 | 19 | ✅ 19 |

### 2.E 系统调用集合（K-112 ~ K-130）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 新归属 |
|------|------|------|------|---------|------|---------|--------|
| K-112 | 58 个 syscall 全量清单与角色分组 | 数据结构 | S | 13 §2.3 | `com.h:207-267`；`com.h:270` | 内核一共提供哪些服务 | 14（总表）/ 32（ABI） |
| K-113 | fork 内核侧：槽位 + 寄存器 + endpoint | 机制 | S | 17 §1.1/§1.2 | `do_fork.c` | fork 在内核里只做多少事 | 20 |
| K-114 | exec 映像替换 | 机制 | S | 17 | `do_exec.c`；`arch_system.c:722-732` | 换程序时内核改什么 | 20 |
| K-115 | exit 与自杀信号委托 | 机制 | S | 17 | `do_exit.c` | 进程退出时内核做什么 | 20 |
| K-116 | clear 的幂等回收 | 机制 | S | 17 | `do_clear.c` | 槽位回收为什么必须幂等 | 20 |
| K-117 | runctl 停止/恢复（含 SMP IPI） | 机制 | S | 17 | `do_runctl.c`；`smp.c:132` | 怎么让别的 CPU 上的进程停下 | 20 |
| K-118 | schedctl / statectl | 机制 | S | 17 | `do_schedctl.c`；`do_statectl.c`；`com.h:442-446` | 调度参数与 IPC 状态怎么改 | 20 |
| K-119 | privctl：运行期权限变更 | 机制 | S（散） | 22 §4.7；08/13 表格 | `do_privctl.c`；`com.h:342-353` | RS 怎么给新服务发权限 | 20（**独立承载**） |
| K-120 | schedule：用户态调度器接口 | 机制 | **N** | 33 提及 | `do_schedule.c`；`system.c:642-723` | 用户态 sched 怎么改优先级 | 20（**新增**） |
| K-121 | 信号双路径：内核信号 vs POSIX 信号 | 概念 | S | 19 §1.1/§1.2 | `do_kill.c`/`do_getksig.c`/`do_endksig.c`/`do_sigsend.c`/`do_sigreturn.c` | 两套信号机制差别 | 21 |
| K-122 | `cause_sig` 与信号管理器 | 机制 | S | 19 §1.4/§4.3；13 | `system.c:389`；`priv.h` | 内核怎么给进程发信号 | 21 |
| K-123 | 停止延迟与 `SIGSNDELAY` | 机制 | S | 19 §1.5/§4.8 | `system.c:463`；`signal.h:264` | PM 怎么等一个进程停下来 | 21 |
| K-124 | sigcontext 保存/恢复三架构抽象 | 机制 | S | 19 §4.6 | `os/arch/src/arch/signal*.rs` | 信号处理器怎么回到原处 | 21 |
| K-125 | mcontext：机器上下文读写 | 机制 | **N** | 19 一处提及 | `do_mcontext.c`；`com.h:256-257` | 调试器/协程怎么读写寄存器 | 21（**新增**） |
| K-126 | IRQ 钩子注册（驱动-中断解耦） | 机制 | S | 20 §1.1/§4.1 | `do_irqctl.c`；`interrupt.c:29` | 驱动怎么收硬件中断 | 22 |
| K-127 | 端口 I/O 三种语义（DEVIO/VDEVIO/SDEVIO） | 机制 | S | 20 §1.2/§4.2–§4.4 | `do_devio.c`；`do_vdevio.c`；`do_sdevio.c` | 单次/批量/跨进程差别 | 22 |
| K-128 | x86-only：IOPL 提权与 BIOS 读取 | 机制 | S | 20 §1.3/§4.5/§4.6 | `do_iopenable.c`；`do_readbios.c` | 非 x86 上怎么退化 | 22 |
| K-129 | TIMES/SETALARM/STIME/SETTIME/VTIMER | 机制 | S | 21 全文 | 五个 do_*.c | 时间服务的用户态接口 | 23 |
| K-130 | setgrant：grant 表建立 | 机制 | **N** | 08/13 表格 | `do_setgrant.c`；`grant.rs` | 授权表是谁建的 | 18（**新增**） |

### 2.F 诊断、工程与支线（K-131 ~ K-152）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 新归属 |
|------|------|------|------|---------|------|---------|--------|
| K-131 | getinfo 分派 | 机制 | S | 25 §2.2/§4.2 | `do_getinfo.c` | 用户态怎么问内核要信息 | 24 |
| K-132 | trace 分派 | 机制 | S | 25 §2.3/§4.3 | `do_trace.c` | ptrace 类功能的内核侧 | 24 |
| K-133 | update：进程槽位交换 | 机制 | S | 25 §2.4/§4.4 | `do_update.c`；`com.h:435-438` | 热更新进程表槽位 | 24 |
| K-134 | sprofile 状态机 | 机制 | S | 25 §2.5；30 | `do_sprofile.c` | 采样开关怎么切 | 24/27 |
| K-135 | abort：内核主动中止 | 机制 | **N** | 08/13/27 表格 | `do_abort.c`；`utility.c` | Minix3 的"内核自杀"路径 | 24（**新增**） |
| K-136 | diagctl 与栈回溯请求 | 机制 | S | 32 §2.4；25 | `do_diagctl.c` | 用户态怎么要一个栈回溯 | 26 |
| K-137 | panic 路径与 `#[panic_handler]` | 机制 | S | 27 §2.2/§3.1/§4.1；01 §6.2 | `utility.c`；`lib.rs:1687/1711` | 不可恢复错误怎么处理 | 25 |
| K-138 | `kputc` 与内核消息缓冲（kmess） | 机制 | S | 27 §2.3/§3.2；28 | `utility.c`；`kmess.rs` | 内核日志怎么出去 | 25 |
| K-139 | `_exit` 在内核被禁止 | 约束 | S | 27 §2.4/§3.3 | `utility.c` | 为什么内核不许调 exit | 25 |
| K-140 | shutdown 与 reboot：有序关闭 | 机制 | **N** | 27 一处提及 | `main.c:351/368`；`arch_reset.c` | 系统怎么停下来 | 25 §3（**新增**） |
| K-141 | 调度队列 sanity check（`runqueues_ok`） | 工具 | S | 29 §2.2/§3.1 | `debug.c`；`debug.rs` | 怎么自检调度不变量 | 26 |
| K-142 | 进程信息打印（`print_proc`/`ser_dump_proc`） | 工具 | S | 29 §2.3/§3.2 | `debug.c`；`proc.c:1970` | 调试时看什么 | 26 |
| K-143 | 栈回溯：frame-pointer 链遍历 | 机制 | S | 32 全文 | `exception.c:289/333`；`stacktrace.rs` | 崩溃时怎么还原调用链 | 26 |
| K-144 | 内核栈 vs 用户栈回溯的权限差异 | 约束 | S | 32 §1.3 | `32:42-58` | 回溯用户栈要小心什么 | 26 |
| K-145 | 采样 profiling 与 `SPROFILE` | 机制 | S | 30 §1.1/§1.2/§2.x | `profile.c`（157 行全在 `#if SPROFILE`） | 性能数据怎么采 | 27 |
| K-146 | NMI profiling（WONTFIX） | 架构演进 | S | 30 §3.3；26 §2.4 | `profile.c` NMI handler | 为什么不实现 | 27 |
| K-147 | FPU lazy 上下文切换 | 机制 | S | 31 全文；06 §3.6/§4.9 | `arch_system.c:51-215`；`proc.c:1922` | FPU 寄存器为什么拖到用时才存 | 28 |
| K-148 | `#NM` 陷阱与 `fpu_owner` 协议 | 机制 | S | 31 | `mpx.S:537-552`；`smp.rs` | 谁"拥有" FPU | 28 |
| K-149 | `.usermapped` 段：用户可见内核数据 | 机制+演进 | S | 28 全文 | `usermapped_data.c`；`kernel.lds` | 内核数据怎么零 syscall 暴露 | 29 |
| K-150 | NMI watchdog：lockup 检测 | 机制+WONTFIX | S | 26 全文 | `watchdog.c`（112 行） | 内核卡死怎么被发现 | 30 |
| K-151 | 构建、镜像与工具链 | 工具 | **N** | 正式文档 0 命中 | `os/Cargo.toml`；`os/xtask/`；`link.ld`；`tools/*.sh` | 这套代码怎么变可启动镜像 | 31（**新建**） |
| K-152 | 测试基建：qemu-tests 与 test kernels | 工具 | **N** | 散在各篇 §5 | `os/qemu-tests/test-kernels/`（20+） | 真机验证怎么做 | 31（**新建**） |

### 2.G 跨模块契约与线格式（K-153 ~ K-160）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 新归属 |
|------|------|------|------|---------|------|---------|--------|
| K-153 | `KernelInfo` 线格式与 `validate()` | 接口 | S | 01 §3.5/§4.1；28 §3.2 | `minix-boot/src/kernel_info.rs` | 两侧共享的二进制契约 | 32 |
| K-154 | syscall ABI：调用号、寄存器约定、errno 通道 | 接口 | S | 13 §1.3；33 | `com.h:207-267`；`minix-types` | 用户态与内核的二进制接口 | 32 |
| K-155 | 用户态 trap 桥（int-33 腿）设计 | 接口 | S | 18-trap-bridge-design（87 行） | `18-trap-bridge-design.md`；`minix-sys/src/arch_trap.rs` | 用户态怎么发 syscall | 32 |
| K-156 | UEFI / OpenSBI 引导协议 | 接口 | S | 01 附录 A（~130 行） | `01:1867-2019`；`boot-shim/src/{uefi,opensbi}_helpers.rs` | 固件接口怎么被抽象 | 32（**从 01 移出**） |
| K-157 | 消息结构（message 72B）与字段访问约定 | 接口 | S | 25 §3.9；todo.md V12-B3 | `include/minix/ipc.h`；`minix-types` | 消息怎么解析 | 32 |
| K-158 | grant 表线格式 | 接口 | S | 18 §4.3 | `grant.rs` | 授权表在内存里长什么样 | 32 |
| K-159 | `Errno` newtype 与 `ToErrno` trait | 接口 | S | todo.md §0；13 §6.5 | `minix-types/src/errno.rs` | 错误码为什么要有 newtype | 32 |
| K-160 | 跨 crate 依赖方向 `kernel→arch→plat/platform→types/boot/elf` | 约束 | S | todo.md §0 | `os/Cargo.toml`；`os/README.md` | 为什么依赖是单向的 | 31 |

### 2.H 架构演进与 Rust 设计（K-161 ~ K-172）

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 | 新归属 |
|------|------|------|------|---------|------|---------|--------|
| K-161 | 四条架构演进（UEFI 替代 multiboot / 4 级页表 / 4GB 截断删除 / 独立 kernel ELF） | 演进 | S | 01 §3.1/§3.3/§3.6；02 §3.1/§3.2；08 §3.3 | `01:537-819`；`02:194-227` | 哪些地方**不是**翻译 | 02/03/32 |
| K-162 | 硬件抽象：16+ trait + 每 arch Mock | 演进 | S | todo.md §0；04 §3.x；03 §3.1 | `os/arch/src/arch/*` | 架构差异怎么被隔离 | 05/32 |
| K-163 | `#[cfg(target_arch)]` 使用纪律 | 约束 | S | todo.md §0.6 | `CLAUDE.md`；`lib.rs` | 什么情况下允许 cfg 选行为 | 32 |
| K-164 | `RtsFlags` bitflags + `AtomicU32` | 演进 | S | 06 §3.3；11 §4.1 | `proc_table.rs`；`sched.rs` | 状态位怎么类型化 | 01/11 |
| K-165 | `Endpoint`/`ProcNr` newtype 防混淆 | 演进 | S | 06 §3.1；99 | `minix-types`；`proc.rs` | 两个"进程号"怎么不混 | 01 |
| K-166 | `Syscall` 枚举 + `TryFrom` 替代 `call_vec[]` | 演进 | S | 08 §3.1/§4.1；13 §4.1 | `syscall.rs` | 分发表怎么类型化 | 14 |
| K-167 | `IpcOutcome` 枚举替代 `Err(NotReady)` | 演进 | S | 12 §3.1 | `ipc.rs` | IPC 结果怎么表达 | 15 |
| K-168 | `CapabilityTemplate` 枚举替代 6 裸参数 | 演进 | S | 06 §3.2；22 §4.4 | `capability.rs` | 能力授予怎么类型化 | 16 |
| K-169 | `GrantVerifyResult` 结构体替代多出参 | 演进 | S | 18 §D3 | `syscall_copy.rs` | Rust 侧怎么修 C 的出参风格 | 18 |
| K-170 | 零堆启动：`const fn` + 静态数组 + `boot_alloc` | 约束 | S | 06 §3.8；todo.md | `boot_alloc.rs`；`lib.rs` | 无 allocator 时怎么建表 | 07 |
| K-171 | `no_std` 与测试：`#[cfg(test)]` + mock trait | 约束 | S | 00 §1.5.2/§1.5.4；各篇 §5 | `test_helpers.rs` | 内核怎么单测 | 31 |
| K-172 | redox 对照 | 演进 | S | 19/20/21/23/26/27/28/29/30 尾部 | 各篇尾部 | 别的微内核怎么做 | 各篇保留 |

### 2.I 统计摘要

| 维度 | 数量 |
|------|------|
| 知识点总条数 | **172** |
| 存量（S） | 165（96%） |
| 新增（N） | 7 条单列（K-019、K-022、K-120、K-125、K-130、K-135、K-140）+ 5 条工程类（K-151、K-152 及 §3.2 的 G-12/G-16 并入）= **12 条（7%）** |
| 按类型分布 | 概念 22 / 机制 71 / 数据结构 24 / 接口与协议 22 / 约束与不变量 12 / 架构演进 12 / 工具与工程 7 / 测试性质 2 |
| 重复出现 ≥2 处的条目 | 43 条（已在"主讲述点"列标注唯一归属） |
| 现有文档承载量 Top5 | 06（23 条）/ 05（12 条）/ 00（11 条）/ 12（11 条）/ 16（9 条） |
| 承载 0 个知识点的编号文档 | `07-paging_init_gpt.md`（分析记录，非概念文档）、`18-trap-bridge-design.md`（设计稿，其知识点已单列为 K-155） |

---

## 3. 覆盖审计

### 3.1 主题全集来源（四路）

1. **C 源码符号**：`minix3/minix/kernel/` 顶层 24 个 .c/.h（6908 行）+ `system/` 38 个 .c +
   `arch/i386/` 与 `arch/earm/` 两棵子树。函数 / 结构体 / 宏 / 常量 / 状态机 / 错误路径 /
   `#if SPROFILE`、`#ifdef CONFIG_SMP`、`#ifdef USE_APIC` 等条件分支已逐项 grep。
2. **OS 通用概念**：进程状态机、地址空间、权限模型、IPC 语义、定时器模型、中断模型、
   FPU lazy 语义、profiling 语义、栈回溯语义。
3. **非 C 制品**：`os/kernel/src/arch/*/link.ld`、`os/boot-shim/src/*`、`os/libs/minix-boot`、
   `os/libs/minix-platform`、`os/xtask/`、`os/qemu-tests/`、`tools/*.sh`、`os/Cargo.toml`、
   `scripts/update_minix.sh`、UEFI/OpenSBI 协议。
4. **阶段边界契约**：`00-master-plan/README.md` 启动因果链；`edge_todo.md`（E1 trap 桥、
   E8 SCHED 通电、E-RSWIRE 等）；`01-stage-kernel/todo.md` §0 审查基线。

### 3.2 覆盖缺口表

| # | 缺口主题 | 证据锚点 | 现状 | 建议 | 落实 |
|---|---------|---------|------|------|------|
| G-01 | 关闭与重启路径 | `main.c:351 prepare_shutdown`；`main.c:368 minix_shutdown`；`arch/i386/arch_reset.c` | 正式文档仅 27 一处提及 | **并入新建的 25-kernel-utility-and-terminal §3**（与 panic/kputc 同属"内核终态"） | K-140 |
| G-02 | 构建、镜像布局与工具链 | `os/Cargo.toml`；`os/xtask/`；`os/kernel/src/arch/*/link.ld`；`tools/gen-test-kernel.sh`；`scripts/update_minix.sh` | 正式文档 **0 命中** `xtask` | **新建 31-build-image-and-test-infra** | K-151 |
| G-03 | 测试基建（qemu-tests 与 test kernel 体系） | `os/qemu-tests/test-kernels/`（`bootstrap/`、`user/`、`smp/` 三组 20+ 测试内核 + `.sh` 脚本） | 散落在 01/02/03/04/06/07/27/30 各篇 §5 | **并入新建 31** | K-152 |
| G-04 | `SYS_SETGRANT` 与 grant 表建立 | `system/do_setgrant.c`；`com.h:242`；`os/kernel/src/grant.rs` | 仅 08/13 表格出现 | **并入 18-syscall-cross-space**（grant 是跨空间拷贝的授权凭证） | K-130 |
| G-05 | `SYS_GETMCONTEXT` / `SYS_SETMCONTEXT` | `system/do_mcontext.c`；`com.h:256-257` | 仅 19 一处提及 | **并入 21-syscall-signal**（机器上下文是信号上下文的泛化） | K-125 |
| G-06 | `SYS_SCHEDULE`（用户态调度器接口） | `system/do_schedule.c`；`system.c:642-723 sched_proc` | 00/08/13/33 提及，无正式篇 | **并入 20-syscall-process** | K-120 |
| G-07 | `SYS_ABORT` | `system/do_abort.c`；`com.h:237` | 仅 08/13/27 表格 | **并入 24-syscall-misc** | K-135 |
| G-08 | 三种时间基准的区别 | `clock.c:178/187/195/203/212/220` | 15/21 隐含未明写 | **并入 13-clock-and-timers** | K-019 |
| G-09 | "驱动即进程"与 IRQ 解耦 | `interrupt.c:29`；`system/do_irqctl.c` | 20 隐含未明写 | **并入 22-syscall-device §Ch1** | K-022 |
| G-10 | 硬件断点 `breakpoints.c` / `debugreg.S` | `arch/i386/breakpoints.c`、`debugreg.S`、`debugreg.h` | 正式文档 **0 命中** | **明确不做**：C 侧为 kdb 内核调试器遗留，minix-rs 无内核调试器计划，`todo.md` 亦无登记 | 见 §3.6 |
| G-11 | `oxpcie.c`（PCIe 配置空间） | `arch/i386/oxpcie.c` | 08/09 提及 | **明确不做**：属设备枚举，归 `16-stage-drivers` | 见 §3.6 |
| G-12 | LAPIC/IOAPIC 编程 | `arch/i386/apic.c`、`apic_asm.S` | 05 仅一次提及文件名 | **并入 06-clock-and-interrupt-bringup** | 并入 K-044 |
| G-13 | 跨模块契约与线格式的统一视图 | 分散在 01/13/28/33/18-trap-bridge-design | 无统一篇 | **新建 32-cross-module-contracts** | K-153~K-160 |
| G-14 | 内核执行模型的统一讲述 | 00 §1.4/§1.5 与 16 全文各讲一半 | 分散且互相引用 | **新建 11-kernel-execution-model** | K-006/012/073~078 |
| G-15 | IPC 权限检查与过滤的集中讲述 | 12 §4.7 与 23 重叠 | 两处重复 | **12 只讲原语；检查与过滤归 17** | K-103~K-105 |
| G-16 | `SYS_PADCONF` | `com.h:267`；`arch/earm/do_padconf.c` | 08/09 提及 | **并入 22-syscall-device**（架构特定设备调用，与 READBIOS 同类） | 并入 K-128 |

### 3.3 重复主题表

| # | 主题 | 重复位置 | 主讲述点 | 其余改为 |
|---|------|---------|---------|---------|
| R-01 | 异常 vs 中断（同步/异步） | 05 §1.0、14 §1.1 | **12** | 05 改为一句 + 引用 |
| R-02 | BKL 与释放窗口 | 00 §1.5.1、13 §1.5、16 | **11** | 00 删；13 改引用 |
| R-03 | 运行态实体三维度 / kernel task 无执行流 | 00 §1.4.1、06 §1.1.4 | **01** | 06 删证据链，只留三句结论 |
| R-04 | `ClockState` / `TimerAction` 定义 | 05 §4.1（~500 行）、15、21 | **13** | 05 移出；21 改引用 |
| R-05 | RTS 16 位全集与不变量 | 06 §2.1.6、11 §1.1、99 | **01** | 06/11 只讲各自场景的设置/联动 |
| R-06 | Direct Map 替代 createpde | 07 §1.3、18 §1.5、24 §1.1 | **08** | 18 只讲"怎么用"；24 合并进 18 |
| R-07 | VMSUSPEND 协议 | 13 §1.4、24 §1.3/§2.8 | **14** | 24 合并进 18 后只引用 |
| R-08 | `struct priv` 字段语义 | 22 §1.1、06 §2.2.1–§2.2.6 | **16** | 06 只讲存储形态 |
| R-09 | KPriv 8 子结构 | 06 §3.10、06 §4.6、22 §4.2 | **16** | 06 删除（`06-todo.md` §11 记录三方计数 6/8/9 不一致） |
| R-10 | SMP 就绪队列与并发模型 | 06 §3.9、16 | **11** | 06 删除 |
| R-11 | FPU 上下文 | 06 §3.6/§4.9、31 | **28** | 06 只留一句指针 |
| R-12 | 栈回溯 | 32、27（util_stacktrace）、14（崩溃路径） | **26** | 27/14 只留调用点一句 |
| R-13 | 早期控制台 | 01 §6.1、05 §3.4/§4.14、27 §3.2 | **06** | 01/27 只留调用点 |
| R-14 | `kernel_call_finish` / `kernel_call_resume` | 13 §4.5/§4.6、24 §2.8 | **14** | 24 合并后只引用 |
| R-15 | Endpoint generation | 99 §1、06 §2.1.1、17 §1.2 | **01** | 06/17 改引用 |
| R-16 | 跨空间拷贝的 syscall 与运行时库宏 | 18（syscall）、24（`sys_datacopy`） | **18**（合并） | 24 归档 |
| R-17 | 调度原语（enqueue/pick_proc） | 11、10 §2.3 | **10** | 11 并入 10 |
| R-18 | 能力模板 | 06 §4.5、22 §4.4 | **16**（定义） | 06 只讲 boot 期授予动作 |

### 3.4 越界主题表

| # | 位置 | 越界内容 | 正确归属 | 处置 |
|---|------|---------|---------|------|
| O-01 | 05 §4.1（~500 行） | `ClockState` 完整实现详解 | 13-clock-and-timers | 整节移出（05：2263 → ~1400 行） |
| O-02 | 01 附录 A（~130 行） | UEFI 极简指南 | 32-cross-module-contracts | 移出 |
| O-03 | 06 §3.6 | FPU 架构演进 | 28-fpu-context | 压为一句指针 |
| O-04 | 06 §3.9 | SMP 预留设计 | 11-kernel-execution-model | 删除 |
| O-05 | 06 §3.10 / §4.6 | KPriv 8 子结构 | 16-privilege-model | 删除，留一句索引 |
| O-06 | 06 §3.11 | 调度字段与统计 | 10 / 13 | 删除 |
| O-07 | 06 §3.12 | boot→running 9 步代码 | 09-system-init | 删除 |
| O-08 | 06 §4.8 | `bsp_finish_booting` 9 步 | 09-system-init | 压为一句 + 引用 |
| O-09 | 06 §4.2 | `load_vm_elf` 栈布局细节 | 03 / 07 | 压为骨架 |
| O-10 | 00 §1.5 | 内核执行模型约束（Rust 开发必读） | 11（并发）/ 32（硬件抽象） | 移出，00 只留一句指针 |
| O-11 | 12 §4.7 | IPC 权限四层检查 | 17-ipc-filtering | 移出 |
| O-12 | 08 §4.1/§4.2 | Syscall 枚举与分派实现 | 14-syscall-dispatch | 移出 |
| O-13 | 09 §4.9 | VM ELF 加载 at boot | 07（boot 期加载） | 移出 |
| O-14 | 27 §6 | 已知缺口（进度式表述） | 参考材料 | 移出正文 |
| O-15 | 各篇 §5 | 长测试名清单 | 31-build-image-and-test-infra | 各篇只留"代表性 10 + `cargo test` 可重放" |
| O-16 | 06 §1.4.1 | boot image 排序的三维度辩护 | 07（压为 3 句） | 压缩 |

### 3.5 非 C 主题逐项回答（固定清单）

| # | 主题 | 讲什么 | 承载篇章 |
|---|------|-------|---------|
| 1 | **链接与加载** | 链接脚本 VMA/LMA、ELF 段拷贝、BSS 清零、入口符号 | **03-kernel-image-and-entry** |
| 2 | **镜像与内存布局** | memmap 构造（02）、链接布局（03）、镜像组装与 xtask（31） | 02 / 03 / **31** |
| 3 | **汇编入口与陷阱进入** | 内核入口 asm 与切栈（03）、异常入口与 trap frame（12）、用户态 trap 桥（32） | 03 / 12 / **32** |
| 4 | **启动装配** | 固件协议与 boot-shim（02）、AP 启动与 trampoline（09/11） | 02 / 09 / 11 |
| 5 | **构建与工具链** | Cargo workspace、三架构 target、xtask、链接脚本、gen-test-kernel | **31** |
| 6 | **跨模块接口与线格式** | `KernelInfo`、syscall ABI、errno 通道、grant 线格式、message 布局、依赖方向 | **32** |
| 7 | **错误路径** | panic 与 `#[panic_handler]`（25）、BadCall/errno（14）、未移植调用兜底（24） | 25 / 14 / 24 |
| 8 | **关闭与退出** | `prepare_shutdown`/`minix_shutdown`/`arch_shutdown`，与 panic 的关系 | **25 §3** |
| 9 | **并发与同步** | BKL、per-CPU、IPI、`BklSection` witness、全局状态白名单 | **11** |
| 10 | **测试基建** | 单测（`#[cfg(test)]` + mock trait）、QEMU 集成测试内核、CI | **31** |

### 3.6 明确不做（本 stage 范围外，附理由）

| 主题 | 理由 | 去向 |
|------|------|------|
| 硬件断点 `breakpoints.c` / `debugreg.S` / `debugreg.h` | C 侧为 kdb 内核调试器遗留；minix-rs 无内核调试器计划，`todo.md` 亦无登记 | 不进本 stage；若将来需要，归 26-kernel-diagnostics |
| PCIe 配置空间 `oxpcie.c` | 属设备枚举与总线驱动，非内核机制 | `16-stage-drivers` |
| `do_padconf.c` 的引脚复用业务语义 | ARM 板级引脚配置 | 22 只讲"这是架构特定设备调用"，语义归 `12-stage-input`/`16-stage-drivers` |
| ACPI / DTB 的完整规范 | 规范是外部标准 | 05 只讲"它是硬件描述来源"，规范引用外链 |
| 各用户态服务的业务逻辑（RS/PM/VFS/SCHED） | 属其它 stage | 见 `00-master-plan/README.md` 启动因果链 |
| `arch/earm/bsp/ti/` 具体板级寄存器编程 | 目标平台是 QEMU virt，不是 OMAP 实板 | 05 只保留"板级常量作为硬件描述来源之一"这一层 |

---

## 4. 新目录

### 4.1 设计原则（五条硬规则）

1. **分段而非单线**。目录分五段：Part 0 地基 / Part I 启动链 / Part II 运行期核心 /
   Part III 系统调用集合 / Part IV 支线。只有 Part I 严格线性；Part II/III 是并行体；Part IV 可跳读。
2. **横切概念前置**。被全目录依赖的横切概念（进程标识、能力、状态位、调用号、地址空间、时间）
   集中到 `01-kernel-foundations`，**不再排在 99**。理由：现状 `99-global-concepts` 被
   114 个文件引用 273 次（全树最高热点），却排在目录末尾——这是结构性矛盾。
3. **权限与过滤必须在 syscall 集合之前**。现状 `17/19/20/21` 四篇的前置声明都指向 `22-privilege`，
   是四处显式前向引用。新目录把权限放 `16`、过滤放 `17`，syscall 组从 `18` 起。
4. **启动侧与运行侧分离**。凡"boot 期做一次"与"运行期反复发生"同名的主题（时钟、跨空间、SMP、
   特权分配），拆到启动链篇与运行期篇各讲一半，两边互相引用，不在一篇里从头讲到尾。
5. **支线集中**。诊断、profiling、FPU、用户可见数据、watchdog、关闭路径、工程与契约全部收进
   Part IV，主线读者可以完全跳过。

### 4.2 新篇章总表

| 段 | 编号 | 标题 | 一句话定位 | 旧来源 |
|----|------|------|-----------|--------|
| 0 | 00 | `kernel-overview` | 内核是什么、启动线长什么样、本目录怎么读 | 00（重写瘦身） |
| 0 | 01 | `kernel-foundations` | 全目录共享的横切概念：进程与 endpoint、能力、状态位、调用号、地址空间、时间 | **99 + 00 §1.4 + 06 §1.3**（新建） |
| I | 02 | `firmware-handoff` | 固件把系统状态交给内核：`KernelInfo` 的构造与 `arch_boot` 交接 | 01（瘦身） |
| I | 03 | `kernel-image-and-entry` | 内核镜像怎么摆、怎么加载、怎么跳进高地址 | 02 |
| I | 04 | `kmain-and-protection` | `kmain` 入口与保护结构的建立 | 03 |
| I | 05 | `platform-discovery` | 内核怎么在不硬编码的前提下认识硬件 | 04 |
| I | 06 | `clock-and-interrupt-bringup` | 让内核能响应硬件事件：时钟变量、中断控制器、架构收尾 | 05（移出 `ClockState`） |
| I | 07 | `process-table-and-boot-image` | 阶段 C：建表、按 boot image 落槽、发权限、加载 VM | 06 |
| I | 08 | `cross-space-capability` | 内核怎么获得"看别人地址空间"的能力 | 07 |
| I | 09 | `system-init-and-boot-finish` | 把内核从初始化态带入运行态：syscall 注册、内存回收、SMP 启动、完成启动 | 08 + `smp_init` |
| I | 10 | `switch-to-user` | 汇聚点：内核把 CPU 交给下一个进程 | 10（+ 旧 11 的调度原语） |
| II | 11 | `kernel-execution-model` | 三条入口、一个出口；BKL、per-CPU、IPI 与全局状态安全 | **新建**（16 + 00 §1.4/§1.5） |
| II | 12 | `exception-and-interrupt` | CPU 出事时内核怎么分流、恢复、转发、通知 | 14（+ 05 §1.0） |
| II | 13 | `clock-and-timers` | 时间从哪来、怎么记账、定时器怎么到期 | 15 + 05 §4.1 |
| II | 14 | `syscall-dispatch` | 58 个内核调用怎么路由、怎么挂起、怎么回复 | 13 + 33 + 08 §4.1/§4.2 |
| II | 15 | `ipc-core` | 无共享内存下进程怎么通信、阻塞、唤醒、防死锁 | 12（移出权限检查） |
| II | 16 | `privilege-model` | 一个进程"能干什么"由什么决定 | 22（+ 06 的 KPriv） |
| II | 17 | `ipc-filtering` | 一条消息要过哪几道关才能送达 | 23 + 12 §4.7 |
| III | 18 | `syscall-cross-space` | 跨地址空间的数据搬运：拷贝、授权、地址翻译、填充 | 18 + 24 + K-130 |
| III | 19 | `syscall-vmctl` | 内核与 VM 的页表协商协议 | 09 |
| III | 20 | `syscall-process` | 进程生命周期类内核调用 | 17 + privctl + schedule |
| III | 21 | `syscall-signal` | 信号类内核调用与内核信号路径 | 19 + mcontext |
| III | 22 | `syscall-device` | 设备类内核调用：IRQ 与端口 I/O | 20 + padconf |
| III | 23 | `syscall-clock` | 时间类内核调用 | 21 |
| III | 24 | `syscall-misc` | 杂项与信息类内核调用 | 25 + abort |
| IV | 25 | `kernel-utility-and-terminal` | 内核怎么说话、怎么死、怎么停下来 | 27 + K-140 |
| IV | 26 | `kernel-diagnostics` | 内核怎么自检与向人类报告状态 | 29 + 32 |
| IV | 27 | `profiling` | 统计采样：机制、开关与 WONTFIX 部分 | 30 |
| IV | 28 | `fpu-context` | FPU 状态的 lazy 保存与恢复 | 31 |
| IV | 29 | `usermapped-data` | 用户可见内核数据（含为什么 64 位不保留） | 28 |
| IV | 30 | `watchdog` | NMI watchdog 与 lockup 检测（WONTFIX） | 26 |
| IV | 31 | `build-image-and-test-infra` | 这套代码怎么变成可启动镜像、怎么被验证 | **新建** |
| IV | 32 | `cross-module-contracts` | 跨模块与跨特权级的二进制契约 | **新建** |

**共 33 篇**（旧 35 篇编号文档 → 33 篇；新建 4、归档 4、合并 3、净减 2）。

### 4.3 并行体的分组（58 个 syscall）

按 `com.h:207-267` 分组。**主线** = 每组挑 1 个代表讲透，其余按差异表收束：

| 组 | 篇章 | 调用号偏移（com.h） | 代表成员 | 其余成员（差异表收束） |
|----|------|-------------------|---------|---------------------|
| 跨空间数据 | **18** | 13,14,15,16,17,18,31,32,33,56,34 | `SYS_VIRCOPY`(15) | MEMSET(13)/UMAP(14)/PHYSCOPY(16)/UMAP_REMOTE(17)/VUMAP(18)/SAFECOPYFROM(31)/SAFECOPYTO(32)/VSAFECOPY(33)/SAFEMEMSET(56)/SETGRANT(34) |
| 地址空间 | **19** | 43 | `SYS_VMCTL`(43) | 单成员组，但子命令多 |
| 进程生命周期 | **20** | 0,1,2,3,4,46,52,53,54,55 | `SYS_FORK`(0) | EXEC(1)/CLEAR(2)/SCHEDULE(3)/PRIVCTL(4)/RUNCTL(46)/UPDATE(52)/EXIT(53)/SCHEDCTL(54)/STATECTL(55) |
| 信号 | **21** | 6,7,8,9,10,50,51 | `SYS_KILL`(6) | GETKSIG(7)/ENDKSIG(8)/SIGSEND(9)/SIGRETURN(10)/GETMCONTEXT(50)/SETMCONTEXT(51) |
| 设备 | **22** | 19,21,22,23,28,35,57 | `SYS_IRQCTL`(19) | DEVIO(21)/SDEVIO(22)/VDEVIO(23)/IOPENABLE(28)/READBIOS(35)/PADCONF(57) |
| 时间 | **23** | 24,25,39,40,45 | `SYS_SETALARM`(24) | TIMES(25)/STIME(39)/SETTIME(40)/VTIMER(45) |
| 杂项与信息 | **24** | 5,26,27,36,44 | `SYS_GETINFO`(26) | TRACE(5)/ABORT(27)/SPROF(36)/DIAGCTL(44) |

未使用调用号（11,12,20,29,30,37,38,41,42,47,48,49）在 14 的总表标注为保留位。

### 4.4 阅读路径

- **主线（必读）**：`00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 16 → 17`
  - 读完后读者能回答："这台机器从通电到能跑用户进程，内核做了什么；用户进程进来之后，内核靠什么机制继续运转。"
- **支线一（syscall 实现，按需跳读）**：`18 → 19 → 20 → 21 → 22 → 23 → 24`（每组独立）
- **支线二（可观测与工程）**：`25 → 26 → 27 → 28 → 29 → 30 → 31 → 32`
- **最短路径（只关心启动）**：`00 → 01 → 02 → 03 → 04 → 06 → 07 → 09 → 10`
- **最短路径（只关心 IPC）**：`01 → 10 → 11 → 15 → 16 → 17`

---

## 5. 每篇契约

> 七要素齐全。**前置只允许指向更早编号**；已在 §1.3 序差表登记的后向引用以 `（回指补偿 D-n）` 标注。

### 00-kernel-overview

- **一句话定位**：让读者在 20 分钟内建立"Minix3 内核是什么、启动线长什么样、本目录怎么读"的心智地图。
- **讲什么**：K-001、K-006（全景版）、K-015、K-018；启动线全景图；五段目录导航表；与 02-stage-vm / 03-stage-rs 的边界。
- **不讲什么**：
  - 运行态实体三维度的证据链 → 01
  - BKL/并发约束 → 11
  - 硬件抽象纪律 → 32
  - 任何单个机制的细节 → 对应篇章
- **前置**：无。假定读者懂 OS 基础概念与 Rust 基础语法。
- **后置**：全部 32 篇。
- **事实底线**：
  - C：`main.c:115 kmain`、`main.c:403 cstart`、`main.c:38 bsp_finish_booting`、`main.c:107 switch_to_user`、`proc.c:299`、`com.h:270 NR_SYS_CALLS 58`、`table.c:44-64`
  - 非 C：`os/Cargo.toml`、`os/README.md`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-001 | 微内核内核 vs 服务 | 概念 | `00:13-44,311-322` | 本篇定义内核身份 | S：00 §1.1/§4.1 |
  | K-006 | 三条路径一个出口（全景） | 概念 | `proc.c:299`；`main.c:107-108` | 本篇给全景，11 给机制 | S：00 §1.4/§4.1 |
  | K-015 | 策略与机制分离 | 概念 | `00:353-356` | 解释内核-VM 关系 | S：00 §5.2 |
  | K-018 | 硬件操作权限边界 | 概念 | `00:19-21` | 本篇给信任边界 | S：00 §1.1/§4 |

- **验收标准**：
  1. 读者能不看后续篇章画出"固件 → kmain → switch_to_user → 用户进程"的 6 段链。
  2. 全景图每一步带 C 锚点，随机抽 3 条 `sed -n` 可核对。
  3. 导航表覆盖全部 32 篇，无遗漏、无指向不存在的编号。
  4. 全篇不含机制实现细节（grep 无 `enqueue`/`mini_send`/`kernel_call_dispatch` 的函数级展开）。

### 01-kernel-foundations

- **一句话定位**：把全目录反复引用的横切概念（进程标识、能力、状态位、调用号、地址空间、时间）一次讲清，使后续每一篇都不必重复解释术语。
- **讲什么**：K-002、K-003、K-004、K-005、K-007、K-008、K-010（术语层）、K-013（术语层）、K-019、K-050、K-051、K-052、K-057、K-058、K-165
- **不讲什么**：
  - 进程表的存储形态与 boot 期填充 → 07
  - `struct priv` 的字段语义与掩码构建 → 16
  - RTS 各位的设置时机与队列联动 → 07 / 10
  - IPC 原语的运行时行为 → 15
  - 定时器的到期处理 → 13
- **前置**：00
- **后置**：除 00 外全部 31 篇
- **事实底线**：
  - C：`proc.h`（`p_nr`/`p_endpoint`/`p_rts_flags`）、`include/minix/endpoint.h`、`com.h:47-56`、`com.h:207-270`、`const.h`、`config.h`、`type.h`、`glo.h`、`kernel.h`、`clock.c:178/203/220`
  - 非 C：`os/libs/minix-types/src/*`（`Endpoint`/`ProcNr`/`Errno`）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-002 | 运行态实体三维度 | 概念 | `00:60-101`；`main.c:64-66` | 全目录共用分类框架 | S：00 §1.4.1 |
  | K-003 | 内核 task vs 系统服务器 | 概念 | `com.h:47-56`；`table.c:44-51` | 术语澄清，多处误用 | S：00 §1.4.1 + 06 §1.1.4 |
  | K-004 | IDLE 的特殊性 | 概念 | `proc.c:176-193` | 三维度模型的反例 | S：00 §1.4.1 |
  | K-005 | HARDWARE / ASYNCM | 概念 | `com.h:47,52` | 纯 IPC 身份 | S：00 §1.4.1 |
  | K-007 | 进程的本质：断点续传 | 概念 | `proc.h` | 后续所有进程话题的地基 | S：06 §1.1.1 |
  | K-008 | 进程五组成部分 | 概念 | `proc.h` | 字段分组的依据 | S：06 §1.1.3 |
  | K-010 | 特权级与信任边界（术语层） | 概念 | `03:24-59` | 名词与层级，实现归 04 | S：03 §1.1/§1.2 |
  | K-013 | 地址空间与页表所有权（术语层） | 概念 | `do_vmctl.c` | 名词，协议归 19 | S：09 §1.3 |
  | K-019 | 三种时间基准 | 概念 | `clock.c:178/203/220` | **新增**：C 有三种"现在" | N：C 源码 |
  | K-050 | `p_nr` vs `p_endpoint` | 数据结构 | `proc.h`；`endpoint.h` | 两个标识的区分全目录要用 | S：99 §1 + 06 §2.1.1 |
  | K-051 | endpoint 编码布局 | 数据结构 | `99:17-29` | 位布局只讲一次 | S：99 §1.2 |
  | K-052 | `ANY`/`NONE`/`SELF` | 数据结构 | `99:30-39` | 常量表 | S：99 §1.3 |
  | K-057 | RTS 16 位全集 | 数据结构 | `proc.h` | 位表是字典，不属于单一场景 | S：06 §2.1.6 / 11 §1.1 |
  | K-058 | 不变量 `rts==0 ⟺ runnable` | 约束 | `06:191-299`；`11:12-26` | 全目录最强不变量 | S：06 §1.3.3 / 11 §1.1 |
  | K-165 | `Endpoint`/`ProcNr` newtype | 演进 | `minix-types` | 类型安全决策与标识概念强绑定 | S：06 §3.1 |

- **验收标准**：
  1. RTS 16 个位名可 grep 到全部 16 个。
  2. `p_nr` 与 `p_endpoint` 的区别有一个"拿错会怎样"的具体反例。
  3. 全篇不含任何 boot 期动作（grep 无 `proc_init`/`arch_boot_proc`）。
  4. 后续 31 篇凡涉及本篇概念的，都写成 `01 §X` 引用而非重复解释。

### 02-firmware-handoff

- **一句话定位**：固件手里有什么、怎么交给内核、`KernelInfo` 是怎么造出来的。
- **讲什么**：K-023、K-024（构造侧）、K-030；启动前系统状态；自举四步；boot-shim 与固件的职责切分
- **不讲什么**：
  - 链接脚本与高半跳转 → 03
  - UEFI/OpenSBI 协议细节 → 32
  - 页表多级结构与 Paging trait → 03 / 08
  - 早期控制台实现 → 06
- **前置**：00、01
- **后置**：03、04、05、06、07、09、31、32
- **事实底线**：
  - C：`arch/i386/pre_init.c:217 pre_init`、`pre_init.c:94 get_parameters`、`pre_init.c:35 mb_set_param`、`pg_utils.c:32 cut_memmap`、`pg_utils.c:86 add_memmap`
  - 非 C：`os/boot-shim/src/{lib,loader,main,uefi_helpers,opensbi_helpers}.rs`、`os/libs/minix-boot/src/kernel_info.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-023 | multiboot 与启动参数 | 接口 | `pre_init.c:94/35` | 固件→内核的第一手资料 | S：01 §2.0/§2.1 |
  | K-024 | memmap 构造与裁剪 | 机制 | `pg_utils.c:32/86` | boot-shim 的核心产出 | S：01 §2.3 |
  | K-030 | `KernelInfo` 契约（构造侧） | 接口 | `minix-boot/src/kernel_info.rs` | 两侧唯一契约由本篇造出 | S：01 §3.5/§4.1 |
  | K-156 | UEFI / OpenSBI 引导协议 | 接口 | `01:1867-2019` | **本篇只讲"协议被谁用"，规范细节归 32** | S：01 附录 A（移出） |

- **验收标准**：
  1. 读者能列出 `KernelInfo` 的字段分组并说明每个字段是谁填的。
  2. `pre_init()` 四步中本篇负责前两步，与 03 的分界有一句显式声明。
  3. 全篇不再有独立的"UEFI 极简指南"附录（grep `附录 A` 应指向 32）。
  4. 目标行数 ≤ 900（旧 2019 行；附录 A 与页表多级结构论证已移出）。

### 03-kernel-image-and-entry

- **一句话定位**：内核二进制怎么摆在内存里、怎么被加载、怎么跳进高地址开始执行 C 代码。
- **讲什么**：K-025、K-026、K-027、K-028、K-029、K-161 的"独立 kernel ELF"与"4 级页表"两条
- **不讲什么**：
  - `KernelInfo` 字段设计 → 02
  - 平台/硬件描述解析 → 05
  - direct_map 的运行时使用 → 08 / 18
  - boot 期 VM ELF 加载（那是加载用户程序）→ 07
- **前置**：02
- **后置**：04、08、31、32
- **事实底线**：
  - C：`arch/i386/kernel.lds`、`arch/i386/head.S`、`pg_utils.c:162 pg_identity`、`pg_utils.c:186 pg_mapkernel`、`pg_utils.c:247 pg_load`、`pg_utils.c:204 vm_enable_paging`
  - 非 C：`os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld`、`os/kernel/src/lib.rs:297 arch_boot_impl`、`lib.rs:158/170/182 arch_boot`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-025 | 恒等映射的作用 | 机制 | `pg_utils.c:162` | 开分页瞬间的连续性 | S：01 §2.3 / 02 §1.1 |
  | K-026 | 为什么是高半内核 | 机制 | `02:27-36` | 地址空间划分 | S：02 §1.2 |
  | K-027 | 切栈与跳指令成对 | 机制+不变量 | `02:12-26,155-165` | 强不变量，一次讲透 | S：02 §1.1/§2.3 |
  | K-028 | VMA/LMA 与 `AT()` | 工具 | `kernel.lds`；`link.ld` | 本篇的核心非 C 制品 | S：02 §2.1 |
  | K-029 | 内核 ELF 加载 | 机制 | `lib.rs:297` | 段拷贝 + BSS 清零 | S：02 §4.2 |
  | K-161a | 独立 kernel ELF（演进） | 演进 | `02:194-227` | 演进决策就近讲 | S：02 §3.2 |
  | K-161c | 4 级页表（演进） | 演进 | `01:621-637` | 页表级数变化在此落地 | S：01 §3.3 |

- **验收标准**：
  1. 三架构 `link.ld` 的约束各有一张对照表。
  2. "只跳不切"与"只切不跳"两种错误各有明确后果描述。
  3. riscv64 L2 条目覆盖 bug 的完整调试记录压为一句 + `git log` 指针（原附录 A ~270 行）。
  4. 目标行数 ≤ 800（旧 1329 行）。

### 04-kmain-and-protection

- **一句话定位**：`kmain()` 入口到保护结构就绪——内核怎么给 CPU 装上"我能保护自己"的设施。
- **讲什么**：K-031、K-032（开窗侧）、K-033、K-034、K-035、K-036、K-010（实现层）
- **不讲什么**：时钟与中断控制器 → 06；平台/硬件描述 → 05；进程表与 boot image → 07；
  trap frame 布局与 syscall 入口 → 12 / 14
- **前置**：03
- **后置**：05、06、07、12
- **事实底线**：
  - C：`main.c:115 kmain`、`main.c:121-125 bss_test`、`main.c:142 kernel_may_alloc=1`、`main.c:411 prot_init`、`arch/i386/protect.c:321 prot_init`、`protect.c:260 idt_init`、`arch/earm/protect.c:prot_init`
  - 非 C：`os/kernel/src/lib.rs:833 init_protection`、`os/arch/src/arch/*`（`ProtectionArch`/`TrapEntryArch`）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-031 | BSS 清零断言 | 约束 | `main.c:121-125` | kmain 的第一件事 | S：03 §2.1 |
  | K-032 | `kernel_may_alloc` 开窗 | 约束 | `main.c:142` | 与 09 的关窗配对 | S：03 §2.1 |
  | K-033 | `prot_init()` GDT/IDT/TSS | 机制 | `protect.c:321/260` | 本篇主机制 | S：03 §2.3–§2.6 |
  | K-034 | 为什么重装保护结构 | 概念 | `03:228-237` | 反直觉点 | S：03 §1.6 |
  | K-035 | 三架构保护结构对照 | 演进 | `03:211-227,855-869` | 抽象结论 | S：03 §1.5/§4.4 |
  | K-036 | 为什么必须切内核栈 | 概念 | `03:1079-1193` | 威胁模型 | S：03 附录 A |
  | K-010 | 特权级与信任边界（实现层） | 概念 | `03:24-59` | 保护语义的地基 | S：03 §1.1/§1.2 |

- **验收标准**：
  1. 三架构"特权级"名词（ring / EL / privilege mode）各有一行对照。
  2. `prot_init()` 的步骤数与 Rust 侧 `init_protection()` 的步骤数有对照表。
  3. 附录 A 威胁模型压到 ≤ 60 行。
  4. 目标行数 ≤ 750（旧 1197 行）。

### 05-platform-discovery

- **一句话定位**：内核怎么在不硬编码 MMIO 地址的前提下，认识自己跑在什么硬件上。
- **讲什么**：K-037、K-038、K-039、K-040、K-041、K-042
- **不讲什么**：时钟/中断控制器的寄存器级编程 → 06；GDT/IDT 初始化 → 04；
  ACPI/DTB 的二进制规范 → 外链；具体板级寄存器（OMAP）→ §3.6 明确不做
- **前置**：04
- **后置**：06、09、11、22、32
- **事实底线**：
  - C：`arch/i386/arch_system.c:246 arch_init`（含 `acpi_init()`）、`arch/i386/acpi.c:acpi_init`、`arch/earm/arch_system.c`（`bsp_init()`）、`arch/earm/bsp/include/bsp_init.h`
  - 非 C：`os/libs/minix-platform/src/{desc,kind,global,device_tree,acpi}.rs`、`arch/{x86_64,aarch64,riscv64}.rs`、`os/libs/minix-boot/src/platform.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-037 | 硬编码地址的不可持续性 | 概念 | `04:26-44` | 问题定义 | S：04 §1.1 |
  | K-038 | ACPI 作为 x86 来源 | 机制 | `acpi.c:acpi_init` | x86 路径 | S：04 §2.1 |
  | K-039 | DTB 作为 ARM/RV 来源 | 机制 | `arch/earm/arch_system.c` | ARM/RV 路径 | S：04 §2.2 |
  | K-040 | `PlatformDesc` trait | 机制 | `minix-platform/src/desc.rs` | Rust 侧核心抽象 | S：04 §3.2–§3.4 |
  | K-041 | QEMU virt 兜底 | 机制 | `.../arch/*/QemuVirtDesc` | 无固件表时的路径 | S：04 §3.6 |
  | K-042 | 解析职责切分 | 机制 | `04:216-252` | boot-shim vs kernel | S：04 §3.1 |

- **验收标准**：
  1. 有一张"硬件参数 → 来源（ACPI/DTB/兜底）→ 消费方（时钟/中断/SMP）"对照表。
  2. 三架构各自的 `QemuVirtDesc` 有一行说明。
  3. 全篇不出现 ACPI 表头字段级解析（grep 无 `RSDP` 逐字节展开）。
  4. 目标行数 ≤ 900（旧 1255 行）。

### 06-clock-and-interrupt-bringup

- **一句话定位**：`cstart()` 的后半段——让内核"听得见"硬件：时钟变量、中断控制器、架构收尾、早期控制台。
- **讲什么**：K-043、K-044、K-045、K-046（初始化侧）、K-047、G-12（LAPIC/IOAPIC）
- **不讲什么**：
  - `ClockState` / `TimerAction` 数据结构 → **13**（本篇只说"变量在此初始化，结构见 13"）
  - 时钟中断的运行期处理 → 13
  - 异常/中断的运行期分派 → 12（K-011 只留一句引用）
  - panic 路径下的控制台使用 → 25
- **前置**：04、05
- **后置**：09、12、13、25
- **事实底线**：
  - C：`main.c:403 cstart`、`main.c:418 init_clock`、`clock.c:48 init_clock`、`main.c:472 intr_init(0)`、`arch/i386/i8259.c`、`arch/i386/apic.c`、`arch/i386/arch_system.c:246 arch_init`、`main.c:414-469`
  - 非 C：`os/kernel/src/lib.rs:1020 init_clock_and_interrupts`、`os/arch/src/arch/clock.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-043 | `init_clock()` 只初始化变量 | 机制 | `clock.c:48` | 区分"初始化"与"启动" | S：05 §2.1 |
  | K-044 | `intr_init()` 中断控制器编程 | 机制 | `i8259.c`；`apic.c`；GICv3/PLIC | 本篇主机制 | S：05 §2.2/§2.3 + G-12 |
  | K-045 | `arch_init()` 架构收尾 | 机制 | `arch_system.c:246` | cstart 最后一步 | S：05 §2.4/§2.5 |
  | K-046 | 早期控制台（初始化侧） | 机制 | `05:471-540,2014-2091` | 初始化归本篇 | S：05 §3.4/§4.14 |
  | K-047 | 环境变量驱动配置 | 机制 | `main.c:414-469` | 启动参数→开关 | S：05 §2.6 |
  | K-011 | 异常 vs 中断 | 概念 | `05:12-53` | **引用 12**，本篇只留一句 | S：05 §1.0（移出） |

- **验收标准**：
  1. 有一张"`cstart()` 调用序"表，五步各带行号。
  2. `init_clock`（`clock.c:48`）与 `boot_cpu_init_timer`（`clock.c:294`）的时序差有明确说明。
  3. `ClockState` 在本篇出现 ≤ 3 次且每次都是"见 13"。
  4. **目标行数 ≤ 1000（旧 2263 行，本次重建最大的单篇瘦身）**。

### 07-process-table-and-boot-image

- **一句话定位**：阶段 C——清空进程表、按 boot image 落槽、发权限、加载 VM，让系统出现第一批"就位但暂停"的进程。
- **讲什么**：K-009、K-016、K-017、K-048、K-049、K-053、K-054（存储侧）、K-055、K-064、K-065、K-066、K-170
- **不讲什么**：
  - `struct priv` 字段语义与掩码 → 16（本篇只讲"槽怎么分、怎么桥接"）
  - RTS 各位的运行期联动 → 10
  - FPU / SMP / KPriv 8 子结构 → 28 / 11 / 16（**删除**）
  - fork/exec 运行时如何改字段 → 20
  - ELF 加载机制的完整细节 → 03
- **前置**：01、04、06
- **后置**：08、09、10、13、15、16、20
- **事实底线**：
  - C：`main.c:157 proc_init`、`main.c:165-272`、`proc.c:119 proc_init`、`main.c:196-250`、`main.c:253`、`main.c:257 arch_boot_proc`、`protect.c:388 arch_boot_proc`、`main.c:264-267`、`main.c:269-270`、`table.c:44-64`、`system.c:274 get_priv`、`main.c:179-184`
  - 非 C：`os/kernel/src/lib.rs:1112 init_proc_and_boot`、`{proc,proc_table,kpriv,capability}.rs`、`boot_alloc.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-009 | CPU 四问 | 概念 | `protect.c:388`；`proc.h stackframe_s` | 让进程"第一次能跑"需要什么 | S：06 §1.2 / 03 §1.4 |
  | K-016 | VM 鸡生蛋 | 概念 | `main.c:203-211` | 解释为什么只有 VM 被加载 | S：06 §1.4.2 |
  | K-017 | 编译期定容三层因果链 | 概念+约束 | `06:453-491`；`config.h` | **本篇的存在理由** | S：06 §2.0 |
  | K-048 | `proc_init()` 清表 | 机制 | `proc.c:119` | 阶段 C 第一步 | S：06 §1.5.1/§4.7 |
  | K-049 | `proc[]` + `proc_addr()` | 数据结构 | `proc.h`；`proc.h:269` | 存储形态 | S：06 §2.1.0 |
  | K-053 | 进程表字段分组（纯 C 侧） | 数据结构 | `proc.h` | 导航，不展开运行语义 | S：06 §2.1.1–§2.1.6 |
  | K-054 | `priv[64]` + `ppriv_addr[]` | 数据结构 | `priv.h`；`system.c:274` | 存储形态（双表桥接） | S：06 §2.2.0 |
  | K-055 | `USER_PRIV` 共享 | 数据结构 | `priv.h` | 分治策略 | S：06 §2.2.0 |
  | K-064 | boot image 17 项 | 数据结构 | `table.c:44-64` | 系统第一批进程 | S：06 §2.3/§1.4.1 |
  | K-065 | 顺序 ≠ proc 号 | 约束 | `table.c:44-64` vs `com.h` | 常见误解 | S：06 §1.4.1 |
  | K-066 | module list ↔ image 对应 | 接口 | `main.c:179-184` | ELF 地址来源 | S：06 §2.3 |
  | K-170 | 零堆启动存储形态 | 约束 | `boot_alloc.rs` | 三层因果链的 Rust 落地 | S：06 §3.8 |

- **验收标准**：
  1. RTS 的 boot 期 6 位（`SLOT_FREE`/`NO_PRIV`/`NO_QUANTUM`/`VMINHIBIT`/`BOOTINHIBIT`/`PROC_STOP`）各有"谁在什么时候置位"一行。
  2. boot image 17 项编号表可 grep 且与 `table.c:44-64` 逐行对得上。
  3. Ch2 组内**不出现 Rust 类型**（grep 无 `AtomicU32`/`VecDeque`/`newtype`）——沿用 `06-todo.md` §6.1 的 A3。
  4. 全篇不含 FPU/SMP/KPriv 独立小节。
  5. **目标行数 ≤ 1100（旧 1375 行；`06-todo.md` 体检建议收口方向）**。

### 08-cross-space-capability

- **一句话定位**：内核怎么获得"看别的进程地址空间"的能力——32 位的临时窗口为什么被 64 位的 direct map 取代。
- **讲什么**：K-109（原理与建立侧）；`arch_post_init` / `memory_init` 的 C 侧职责；DM 双窗口建立；阶段 D 简化
- **不讲什么**：direct map 在 syscall 里的消费 → 18；页表多级结构与 PTE walk → 03 / 18；VM 侧页表管理 → 02-stage-vm
- **前置**：03、07
- **后置**：09、18、19
- **事实底线**：
  - C：`main.c:283 arch_post_init`、`main.c:293 memory_init`、`arch/i386/memory.c`、`pg_utils.c:267 pg_map`、`pg_utils.c:312 pg_info`
  - 非 C：`os/kernel/src/pte_walk.rs`、`os/arch/src/arch/*`（`DirectMapArch`）、`lib.rs:1514 init_post_and_memory`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-109 | Direct Map 替代 createpde | 机制+演进 | `pg_utils.c:267`；`pte_walk.rs` | 原理与建立归本篇，使用归 18 | S：07 §1.3 + 18 §1.5（去重合并） |

- **验收标准**：
  1. "32 位临时窗口"与"64 位 direct map"有一张成本对照表（映射次数/TLB 刷新/代码复杂度）。
  2. 有一句显式声明：Rust 侧 direct map 的建立点在 `arch_boot_impl`（03），阶段 D 只做**确认**。
  3. 全篇不出现 `SYS_VIRCOPY` 的实现细节。
  4. 目标行数 ≤ 500（旧 691 行）。

### 09-system-init-and-boot-finish

- **一句话定位**：把内核从"初始化态"带入"运行态"——注册 syscall 表、回收 bootstrap 内存、启动其它 CPU、放行进程、交出 CPU。
- **讲什么**：K-032（关窗侧）、S18–S22 全流程；`system_init`、call_vec 注册、`add_memmap`、`smp_init` 与 AP 握手、`bsp_finish_booting` 九步；K-076（启动侧）
- **不讲什么**：syscall 分派机制细节 → 14；BKL/IPI 运行期语义 → 11；`switch_to_user` 主循环 → 10；VM 协商 → 19（D-1）
- **前置**：07、08
- **后置**：10、11、14、19、20
- **事实底线**：
  - C：`main.c:293/295/301`、`system.c:168/52/189`、`main.c:303-317`、`smp.c:30`、`main.c:324 bsp_finish_booting`、`main.c:38-109`、`main.c:64-66`、`main.c:73 boot_cpu_init_timer`、`clock.c:294`、`main.c:78 fpu_init`、`main.c:105`
  - 非 C：`lib.rs:1514 init_post_and_memory`、`lib.rs:2073 bsp_finish_booting`、`lib.rs:2015 boot_init_timer`、`lib.rs:655 init_smp_state`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-032 | `kernel_may_alloc` 关窗 | 约束 | `main.c:105` | 与 04 开窗配对 | S：08 §1.6 |
  | K-076 | AP 启动握手与 trampoline（启动侧） | 机制 | `smp.c:30`；`trampoline.S` | 启动侧归本篇，协议归 11 | S：16（拆分） |
  | S19 | `system_init` 与 call_vec 注册 | 机制 | `system.c:168/52/189` | 本篇主机制 | S：08 §2.3/§4.4 |
  | S20 | bootstrap 内存回收 | 机制 | `main.c:301 add_memmap` | 本篇主机制 | S：08 §4.5 |
  | S22 | `bsp_finish_booting` 九步 | 机制 | `main.c:38-109` | 本篇主机制 | S：08 §4.6 + 06 §4.8（去重） |

- **验收标准**：
  1. `bsp_finish_booting` 九步表与 `main.c:38-109` 逐行对得上，并说明"为什么只清非内核 task 的 `RTS_PROC_STOP`"。
  2. SMP 三条分支（`config_no_apic` / `config_no_smp` / 正常）有一张判定表。
  3. 有一句前向说明："VM 协商由 VM 被调度后发起，见 19"（D-1 回指补偿）。
  4. 目标行数 ≤ 800（旧 1019 行）。

### 10-switch-to-user

- **一句话定位**：所有内核工作流的汇聚点——内核在这里决定"接下来跑谁"，并把 CPU 交出去。
- **讲什么**：K-067、K-068、K-069、K-070、K-072、K-059（联动侧）；三条入口一个出口；BKL 持有到记账点
- **不讲什么**：时钟如何驱动 quantum → 13（回指补偿 D-3）；IPC 阻塞如何改变可运行性 → 15；
  特权与过滤 → 16/17；用户态 trap 桥入口 → 32
- **前置**：01、07、09
- **后置**：12、13、14、15、16
- **事实底线**：
  - C：`proc.c:299 switch_to_user`、`proc.c:338 while(!(p=pick_proc()))`、`proc.c:473 NOT_REACHABLE`、`proc.c:176 idle`、`proc.c:1595/1670/1716/1785/1893`
  - 非 C：`lib.rs:2913 switch_to_user`、`os/kernel/src/sched.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-067 | `switch_to_user()` 主循环 | 机制 | `proc.c:299-477` | 汇聚点本体 | S：10 §2.1/§4.1 |
  | K-068 | `idle()` | 机制 | `proc.c:176-193` | 无就绪进程时的姿态 | S：10 §2.2/§4.3 |
  | K-069 | `pick_proc()` 与 16 级队列 | 机制 | `proc.c:1785`；`sched.rs` | **从 11 移入**：调度主循环的一部分 | S：11 §2.5/§1.2 |
  | K-070 | enqueue/dequeue/enqueue_head | 机制 | `proc.c:1595/1670/1716` | **从 11 移入** | S：11 §2.2–§2.4 |
  | K-072 | 抢占与 `RTS_PREEMPTED` | 机制 | `proc.c:1639` | **从 11 移入** | S：11 §2.2/§3.5 |
  | K-059 | RTS 与队列的隐藏联动 | 机制 | `11:212-236` | 在循环里讲联动才自然 | S：11 §2.7 |

- **验收标准**：
  1. 读者能回答"一条内核工作流（异常/IPC/syscall）结束后，控制流如何回到用户态"。
  2. 16 级队列与 `pick_proc` 的扫描顺序有一张图。
  3. `idle()` 被 `switch_to_user()` 调用的关系明确无误。
  4. 有一句声明："quantum 递减与时钟中断的耦合见 13"（D-3 回指补偿）。
  5. 目标行数 ≤ 600（旧 506 行，因接收旧 11 的调度原语允许增长）。

> **注**：旧 `11-scheduling-primitives` 的调度原语主体（K-069/070/072）并入本篇，因为它们的唯一调用者是
> `switch_to_user` 循环。旧 11 的设计决策（数组索引替代指针链表、`Priority` newtype、`Scheduler` 拆分）
> 随知识点移入。旧编号 11 释放给 `kernel-execution-model`。

### 11-kernel-execution-model

- **一句话定位**：内核代码是怎么被调用的，以及多核下怎么保证不出错——三条入口、BKL、per-CPU、IPI、全局状态安全。
- **讲什么**：K-006（机制版）、K-012、K-073、K-074、K-075、K-076（协议侧）、K-077、K-078、K-164
- **不讲什么**：异常/中断的具体分流 → 12；时钟中断做什么 → 13（回指补偿 D-3）；每个 syscall 做什么 → 14/18–24
- **前置**：01、09、10
- **后置**：12、13、14、15、17、20
- **事实底线**：
  - C：`smp.c:27 SPINLOCK_DEFINE(big_kernel_lock)`、`smp.c:30/63/75/114/123/132/142/156/194`、`cpulocals.h`、`main.c:56-57`、`arch/i386/arch_clock.c:92,107,118`
  - 非 C：`os/kernel/src/smp.rs`、`globals.rs`、`lib.rs:1737/1769/1811/1918`（`*_with` 访问器）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-006 | 三条路径一个出口（机制版） | 概念 | `proc.c:299` | 执行模型骨架 | S：00 §1.4/§4.1 |
  | K-012 | 中断上下文 vs 进程上下文 | 概念 | `00:357-364` | 并发安全的前提 | S：00 §5.3 |
  | K-073 | BKL 与释放窗口 | 机制 | `smp.c:27`；`arch_clock.c:92/107/118` | **00 §1.5.1 + 13 §1.5 + 16 三处合并** | S：00/13/16 |
  | K-074 | per-CPU 数据 | 数据结构 | `cpulocals.h`；`main.c:56-57` | 多核状态划分 | S：16 |
  | K-075 | IPI 与跨 CPU 调度 | 机制 | `smp.c:63/75/114/123/132/142` | 跨核协同 | S：16 |
  | K-076 | AP 启动协议（运行侧） | 机制 | `smp.c:30`；`trampoline.S` | 与 09 的启动侧分工 | S：16（拆分） |
  | K-077 | `SyncUnsafeCell` + `BklProtected` + `BklSection` | 机制 | `globals.rs`；`smp.rs` | Rust 侧全局状态安全 | S：todo.md §0 + 06 §4.3 |
  | K-078 | `*_with(section)` 访问约定 | 接口 | `lib.rs:1737/1769/1811/1918` | 依赖注入形态 | S：lib.rs 符号表 |
  | K-164 | `RtsFlags` bitflags + `AtomicU32` | 演进 | `proc_table.rs`；`sched.rs` | 并发协议落到具体字段 | S：06 §3.3 |

- **验收标准**：
  1. 有一张"BKL 在哪里被释放"完整清单（`arch_clock.c:92/107/118`、`smp.c:75`、`smp.c:30` 等待路径），每条带锚点。
  2. 有一张"哪些状态是 per-CPU、哪些是全局"的对照表。
  3. 有一句声明："BKL 释放窗口内可能并发访问共享数据；具体到时钟中断的路径见 13"（D-3 回指补偿）。
  4. 全篇不出现 APIC 寄存器级编程（那是 06）。
  5. 目标行数 ≤ 900（旧 16 为 1298 行）。

### 12-exception-and-interrupt

- **一句话定位**：CPU 出事时，内核怎么分流、怎么恢复、什么时候转给别人、什么时候通知驱动。
- **讲什么**：K-011、K-079、K-080、K-081、K-082、K-083、K-084
- **不讲什么**：时钟中断的业务内容 → 13；信号投递路径 → 21；页错误的 VM 侧处理 → 02-stage-vm；
  syscall trap 入口 → 14
- **前置**：04、06、10、11
- **后置**：13、21、22
- **事实底线**：
  - C：`exception.c:180/132/49/289/333`、`interrupt.c:29/116/161/169`、`include/hw_intr.h`
  - 非 C：`os/arch/src/arch/exception.rs`、`exception_dispatcher.rs`、`os/kernel/src/irq_manager.rs`、`page_fault.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-011 | 异常 vs 中断 | 概念 | `05:12-53`；`14:12-24` | **两处合并** | S：05 §1.0 + 14 §1.1 |
  | K-079 | 异常帧 | 数据结构 | `14:41-56` | trap 进入的第一手数据 | S：14 §1.3 |
  | K-080 | 分流主干与四种嵌套恢复点 | 机制 | `exception.c:180/132` | 本篇主机制 | S：14 §2.1/§2.2/§3.4 |
  | K-081 | 页错误转发 VM | 机制 | `exception.c:49`；`proc.c:234` | 分流的一种结果 | S：14 §2.3/§4.6 |
  | K-082 | 异常→信号映射 | 数据结构 | `exception.c` `ex_data[]` | 映射表归本篇，投递归 21 | S：14 §2.4/§4.5 |
  | K-083 | IRQ hook 链 | 机制 | `interrupt.c:29/116/161/169` | 中断→驱动通知 | S：14 §2.5 + 20 §1.1（合并） |
  | K-084 | 中断控制器抽象 | 机制 | `hw_intr.h`；`InterruptController` | 运行侧抽象 | S：14 §2.6 + 05 §3.3 |

- **验收标准**：
  1. 有一张"异常号 → 内核动作（转发 VM / 发信号 / panic）"三分表。
  2. 四种嵌套恢复点各有"什么情况下走到它"的一句。
  3. 有一句声明："时钟中断是本篇分流的一个子类，其业务内容见 13"。
  4. 目标行数 ≤ 700（旧 661 行）。

### 13-clock-and-timers

- **一句话定位**：时间从哪来、怎么记账、定时器怎么到期——内核的时间设施。
- **讲什么**：K-019、K-085、K-086、K-087、K-088、K-071（quantum 侧）
- **不讲什么**：`init_clock` 的启动链位置 → 06；时间类 syscall 的参数与返回 → 23；
  负载平均的调度用途细节 → 10
- **前置**：06、10、11
- **后置**：20、23
- **事实底线**：
  - C：`clock.c:48/70/178/187/195/203/212/220/229/245/260/294`、`proc.c:1893`、`do_setalarm.c`、`do_vtimer.c`
  - 非 C：`os/kernel/src/clock.rs`、`os/arch/src/arch/clock.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-019 | 三种时间基准 | 概念 | `clock.c:178/203/220` | **新增**：C 有三种"现在" | N：C 源码 |
  | K-085 | `ClockState`/`TimerAction`/`TimerEntry`/`TimerId` | 数据结构 | `os/kernel/src/clock.rs` | **从 05 §4.1 移入**（~500 行） | S：05 §4.1 |
  | K-086 | `timer_int_handler()` | 机制 | `clock.c:70` | 心跳本体 | S：15 |
  | K-087 | 三类定时器 | 机制 | `do_setalarm.c`；`do_vtimer.c` | 机制归本篇，syscall 归 23 | S：15 + 21 §1.2/§1.4 |
  | K-088 | 负载平均 | 机制 | `clock.c:260` | 时钟副产品 | S：15 |
  | K-071 | quantum 递减与 `proc_no_time` | 机制 | `proc.c:1893` | 时钟驱动调度 | S：11 §2.6 |

- **验收标准**：
  1. 有一张"realtime / monotonic / boottime 各自被谁读写"对照表（`SYS_STIME`/`SYS_SETTIME` 分别改哪个）。
  2. `ClockState` 字段分组表与 `clock.rs` 逐字段对得上。
  3. 有一句反向回指："BKL 在时钟中断处理定时器时被临时释放，见 11"（D-3 补偿）。
  4. 目标行数 ≤ 1000（旧 15 为 1162 行 + 05 移入 ~500 行，需压缩重复段落）。

### 14-syscall-dispatch

- **一句话定位**：58 个内核调用怎么路由、怎么在等 VM 时挂起、怎么把结果送回用户态。
- **讲什么**：K-089、K-090、K-091、K-092、K-093、K-094、K-095、K-112（总表）、K-166
- **不讲什么**：各 `dispatch_*` 实现 → 18–24；`s_k_call_mask` 构建与权限分配 → 16；
  trap 入口汇编与用户态 trap 体 → 32；BKL 本身 → 11
- **前置**：01、10、11、15、16
- **后置**：18、19、20、21、22、23、24
- **事实底线**：
  - C：`system.c:52/95/117-118/136/59/612/141`、`proc.c:234 vm_suspend`、`com.h:207-270`
  - 非 C：`os/kernel/src/syscall.rs`、`trap_dispatch.rs`、`proc_table.rs:caller_slot_mut`、`minix-types/src/errno.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-089 | `kernel_call_dispatch` 与 call_vec | 机制 | `system.c:52/95/117-118/136` | 本篇主机制 | S：13 §2.3/§4.3/§4.4 + 08 §4.2 |
  | K-090 | syscall vs IPC 双语义 | 概念 | `13:26-49` | 入口区分 | S：13 §1.2 |
  | K-091 | `KERNEL_CALL` 归一化 | 接口 | `13:50-88` | 调用号约定 | S：13 §1.3/§3.3 |
  | K-092 | `kernel_call_finish` | 机制 | `system.c:59` | 回复路径 | S：13 §4.5 |
  | K-093 | VMSUSPEND 挂起-恢复 | 机制 | `proc.c:234`；`system.c:612` | **合并 13 §1.4 与 24 §1.3** | S：13 §1.4 + 24 §2.8 |
  | K-094 | 调用者身份：`*proc` → `ProcNr` | 接口 | `33:11-46`；`caller_slot_mut` | **33 全文并入** | S：33 |
  | K-095 | BadCall 与 errno 通道 | 接口 | `system.c`；`minix-types` `Errno` | 错误返回通道 | S：13 §3.4 + 25 §3.2 |
  | K-112 | 58 个 syscall 总表 | 数据结构 | `com.h:207-267` | **本篇只做索引**，实现归 18–24 | S：13 §2.3 |
  | K-166 | `Syscall` enum + `TryFrom` | 演进 | `syscall.rs` | 分发表类型化 | S：08 §3.1/§4.1 |

- **验收标准**：
  1. 58 个调用号的表格里，每一个都有"归哪一篇（18–24 或保留位）"的归属列。
  2. `kernel_call` → `dispatch` → `finish` 三步各带行号，并说明 `kernel_call_resume` 在哪一步插入。
  3. 有一节专门讲"调用者是进程表里的一行"如何降级为 `ProcNr`（吸收旧 33 的 Ch1–Ch3），给出两条链顶。
  4. 目标行数 ≤ 900（旧 13 为 949 行 + 旧 33 的 222 行并入，需压缩备选方案章节）。

### 15-ipc-core

- **一句话定位**：无共享内存的前提下，进程之间怎么通信、怎么阻塞、怎么被唤醒、怎么避免互相等死。
- **讲什么**：K-014、K-021、K-096、K-097、K-098、K-099、K-100、K-101、K-102、K-167
- **不讲什么**：IPC 权限检查与过滤 → **17**（本篇只说"检查发生在入口"）；endpoint 编码规则 → 01；
  各 syscall 的具体参数 → 18–24
- **前置**：01、10、11
- **后置**：14、17、19、20
- **事实底线**：
  - C：`proc.c:599/870/1122/1200/1510/263/843/852`、`system.c:141`
  - 非 C：`os/kernel/src/ipc.rs`（`IpcCall`/`IpcError`/`SendFlags`/`IpcOutcome`/`IpcEngine`）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-014 | 无共享内存通信模型 | 概念 | `12:12-24` | 问题定义 | S：12 §1.1 |
  | K-021 | notify 不阻塞不丢失 | 概念 | `proc.c:1122` | 异步语义 | S：12 §1.2/§2.6 |
  | K-096 | 六原语与阻塞分类 | 机制 | `12:25-41` | 语义模型 | S：12 §1.2 |
  | K-097 | send/receive 三级检查 | 机制 | `proc.c:870` | 主机制 | S：12 §2.4/§2.5/§4.3/§4.4 |
  | K-098 | notify 与 pending 位图 | 机制 | `proc.c:1122/843/852` | 位图存储 | S：12 §2.6 |
  | K-099 | 死锁检测 | 机制 | `proc.c` deadlock | 安全网 | S：12 §1.5/§2.7/§4.5 |
  | K-100 | `delivermsg` 延迟拷贝 | 机制 | `proc.c:263`；`system.c:141` | 两段拷贝的原因 | S：12 §1.3/§2.8 |
  | K-101 | `mini_senda` 批量异步 | 机制 | `proc.c:1200/1510` | 批量路径 | S：12 §2.9/§3.10 |
  | K-102 | `p_caller_q` 发送者队列 | 数据结构 | `proc.h` | 队列形态 | S：12 §3.2/§4.9 |
  | K-167 | `IpcOutcome` enum | 演进 | `ipc.rs` | 结果建模 | S：12 §3.1 |

- **验收标准**：
  1. 六原语有一张"阻塞行为 / 是否需要回复 / 是否可被 notify 打断"对照表。
  2. 死锁检测算法有一句"最坏情况复杂度"与一个具体的 A↔B 例子。
  3. 有一句声明："消息能否送达由 17 决定，本篇只讲送达动作本身"。
  4. 目标行数 ≤ 900（旧 1209 行；§4.7 权限检查四层已移出）。

### 16-privilege-model

- **一句话定位**：一个进程"能干什么"由什么决定——`struct priv` 的字段、三类掩码、能力模板与 privctl。
- **讲什么**：K-020、K-054（语义侧）、K-056、K-060、K-061、K-062、K-063、K-168
- **不讲什么**：特权表的存储形态（分治/双表/零堆）→ 07；IPC 过滤规则的匹配算法 → 17；
  每个 syscall 怎么用权限 → 18–24
- **前置**：01、07
- **后置**：17、18、20、21、22、23
- **事实底线**：
  - C：`priv.h`、`include/minix/priv.h`、`system.c:274 get_priv`、`system.c:918-989`、`do_privctl.c`、`com.h:272`、`com.h:342-353`
  - 非 C：`os/kernel/src/kpriv.rs`（**8** 个子结构：`PrivIdentity`/`PrivInit`/`PrivFlags`/`PrivSignals`/`PrivIpc`/`PrivIo`/`PrivMem`/`PrivRuntime`）、`capability.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-020 | 能力（capability）而非 uid | 概念 | `priv.h` | 权限模型定性 | S：22 §1.1/§1.3 |
  | K-054 | `priv[64]` 双表语义（谁占槽、怎么桥接） | 数据结构 | `priv.h`；`system.c:274` | 与 07 的存储侧分工 | S：06 §2.2.0 + 22 §1.4 |
  | K-056 | 静态段 vs 动态段 | 数据结构 | `system.c:274`；`main.c:200` | 运行期分配 | S：06 §2.2.0 |
  | K-060 | `s_flags` 角色位 | 数据结构 | `priv.h` | 角色表达 | S：22 §1.2 |
  | K-061 | 三类掩码 | 数据结构 | `priv.h:86-87`；`com.h:272` | 三类能力检查 | S：22 §1.3 |
  | K-062 | I/O / IRQ / 内存范围授权表 | 数据结构 | `priv.h`；`do_devio.c`；`do_irqctl.c` | 硬件授权 | S：22 §1.5 + 20 §1.2 |
  | K-063 | 五能力模板 | 机制 | `main.c:203-234`；`capability.rs` | 模板定义归本篇，boot 授予归 07 | S：06 §4.5 + 22 §4.4 |
  | K-168 | `CapabilityTemplate` enum | 演进 | `capability.rs` | 类型化决策 | S：06 §3.2 + 22 §4.4 |

- **验收标准**：
  1. `struct priv` 的 31 个字段全部可 grep 且与 `priv.h` 对得上；按 6 组分组。
  2. KPriv 子结构计数明确写 **8**（以 `kpriv.rs` 为准；修复 `06-todo.md` §11 记录的 6/8/9 三方不一致）。
  3. 三类掩码各有一句"检查发生在哪一行"。
  4. 目标行数 ≤ 700（旧 22 为 622 行 + 06 的 KPriv 内容并入）。

### 17-ipc-filtering

- **一句话定位**：一条消息从"想发"到"能发"，中间要过哪几道关。
- **讲什么**：K-103、K-104、K-105
- **不讲什么**：IPC 原语的阻塞与唤醒 → 15；特权结构的字段定义 → 16；异常/页错误路径 → 12
- **前置**：15、16
- **后置**：20（statectl 的过滤子命令）
- **事实底线**：
  - C：`const.h:20-27`、`priv.h:86-87`、`system.c:111`、`ipc.h:14-22`、`system.c:803-874`、`ipc_filter.h:19-41`、`ipc.h:25-48`、`main.c:158`
  - 非 C：`os/kernel/src/ipc_filter.rs`、`kpriv.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-103 | IPC 权限四层检查 | 机制 | `system.c:111`；`priv.h:86-87` | **从 12 §4.7 移入** | S：12 §4.7 |
  | K-104 | 三层过滤模型 | 机制 | `ipc.h:14-48`；`ipc_filter.h:19-41` | 本篇主机制 | S：23 §1.1/§2.x |
  | K-105 | 过滤池与 `IPCF_POOL_INIT` | 数据结构 | `main.c:158` | 存储 | S：23 §3.5 |

- **验收标准**：
  1. 有一张"一条 SEND 消息的完整检查序列"表（每一步：检查什么 / 失败返回什么 / 在哪一行）。
  2. `may_send_to` 与 `may_asynsend_to` 的不对称性有明确解释（为什么没有 `CHECK_IPC` 标志）。
  3. 目标行数 ≤ 550（旧 23 为 523 行 + 12 §4.7 并入）。

### 18-syscall-cross-space

- **一句话定位**：跨地址空间的数据搬运——直接拷贝、授权拷贝、地址翻译、跨空间填充，以及搬不动时的挂起协议。
- **讲什么**：K-106、K-107、K-108、K-130、K-169、K-093（跨空间触发侧）；消费 08 的 direct map
- **不讲什么**：direct map 的建立原理 → 08；VMSUSPEND 框架 → 14；VM 的页表决策 → 19 / 02-stage-vm
- **前置**：08、14、16
- **后置**：19、20
- **事实底线**：
  - C：`do_copy.c`、`do_safecopy.c`、`do_umap.c`、`do_umap_remote.c`、`do_vumap.c`、`do_memset.c`、`do_safememset.c`、`do_setgrant.c`、`include/minix/syslib.h`（`sys_datacopy` 宏）
  - 非 C：`os/kernel/src/syscall_copy.rs`、`cross_space.rs`、`grant.rs`、`pte_walk.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-106 | 跨空间拷贝三阶段 | 机制 | `syslib.h`；`do_copy.c` | **合并 24 §1.1 + 18 §1.x** | S：24 §1.1 + 18 |
  | K-107 | grant 授权拷贝 | 机制 | `do_safecopy.c`；`grant.rs` | safecopy 家族 | S：18 §2.2/§4.3 |
  | K-108 | umap/vumap 地址翻译 | 机制 | `do_umap.c` 等 | 翻译类调用 | S：18 §2.3/§2.4 |
  | K-130 | `setgrant` 授权表建立 | 机制 | `do_setgrant.c`；`grant.rs` | **新增**：授权表谁建 | N：C 源码 |
  | K-169 | `GrantVerifyResult` | 演进 | `syscall_copy.rs` | 出参类型化 | S：18 §D3 |
  | K-093 | 跨空间拷贝触发的 VMSUSPEND | 机制 | `proc.c:234`；`system.c:612` | 消费 14 的框架 | S：24 §1.3/§2.8 |

- **验收标准**：
  1. 有一张"7 个拷贝类调用 × 是否检查 grant × 是否可能挂起"对照表。
  2. `sys_datacopy` 宏与 `SYS_VIRCOPY` syscall 的关系有一张图（用户态宏展开为 syscall）。
  3. direct map 在每一类拷贝中怎么用，有一句显式引用 08。
  4. 目标行数 ≤ 1100（旧 18 为 809 行 + 24 为 468 行，合并去重后收敛）。

### 19-syscall-vmctl

- **一句话定位**：内核与 VM 的页表协商——VM 是怎么一步步把地址空间接管过去的。
- **讲什么**：K-013（协议侧）、K-110、K-111
- **不讲什么**：VM 内部的页表管理 → 02-stage-vm；boot 期 VM ELF 加载 → 07；direct map 建立 → 08
- **前置**：10、14、16、18
- **后置**：20
- **事实底线**：
  - C：`system/do_vmctl.c`、`arch/i386/arch_do_vmctl.c`、`arch/earm/arch_do_vmctl.c`
  - 非 C：`os/kernel/src/vm.rs`、`vm_handoff.rs`、`lib.rs:2251 current_ptproc_nr`、`lib.rs:2324 current_root_phys`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-013 | 页表所有权在 VM（协议侧） | 概念 | `do_vmctl.c` | 协议前提 | S：09 §1.3 |
  | K-110 | `SYS_VMCTL` 子命令表 | 接口 | `do_vmctl.c` | 本篇主体 | S：09 §2.1–§2.4 |
  | K-111 | ptproc 跟踪与 `set_active_root` | 机制 | `vm.rs`；`lib.rs:2251/2324` | 状态跟踪 | S：09 §4.8 |

- **验收标准**：
  1. 有一张"VM 启动后依次发哪些 `VMCTL_*`"的时序表，每步说明"发之前 / 发之后"。
  2. 有一句声明："本协议由 VM 被调度到用户态后发起，即 10 之后"（D-1）。
  3. `VMCTL_VMINHIBIT_CLEAR` 与 `main.c:265-267` 设置的 `RTS_VMINHIBIT` 一一对应。
  4. 目标行数 ≤ 500（旧 594 行）。

### 20-syscall-process

- **一句话定位**：进程生命周期类内核调用——从 fork 到 exit 到权限变更到调度参数。
- **讲什么**：K-113、K-114、K-115、K-116、K-117、K-118、K-119、K-120
- **不讲什么**：进程表的存储与 boot 期填充 → 07；调度队列的运作 → 10；信号投递 → 21；
  endpoint generation 编码 → 01
- **前置**：01、07、14、16
- **后置**：21、23
- **事实底线**：
  - C：`do_fork.c`、`do_exec.c`、`do_exit.c`、`do_clear.c`、`do_runctl.c`、`do_schedctl.c`、`do_statectl.c`、`do_privctl.c`、`do_schedule.c`、`arch_system.c:722-732`、`com.h:342-353`、`com.h:442-446`
  - 非 C：`os/kernel/src/syscall_process.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-113 | fork 内核侧 | 机制 | `do_fork.c` | 代表成员 | S：17 §1.1/§1.2 |
  | K-114 | exec 映像替换 | 机制 | `do_exec.c`；`arch_system.c:722-732` | 生命周期 | S：17 |
  | K-115 | exit 与自杀信号 | 机制 | `do_exit.c` | 生命周期终点 | S：17 |
  | K-116 | clear 幂等回收 | 机制 | `do_clear.c` | 槽位回收 | S：17 |
  | K-117 | runctl 停止/恢复 | 机制 | `do_runctl.c`；`smp.c:132` | 含 SMP IPI | S：17 |
  | K-118 | schedctl / statectl | 机制 | `do_schedctl.c`；`do_statectl.c` | 参数与状态 | S：17 |
  | K-119 | privctl 运行期权限变更 | 机制 | `do_privctl.c`；`com.h:342-353` | **从 22 §4.7 移入并独立承载** | S：22 §4.7 |
  | K-120 | schedule 用户态调度接口 | 机制 | `do_schedule.c`；`system.c:642-723` | **新增** | N：C 源码 |

- **验收标准**：
  1. 有一张"10 个调用 × 谁有权发起 × 改变哪些 proc/priv 字段"对照表。
  2. fork 一节必须回答"内核侧 fork 占整个 fork 工作量的百分之多少，其余在哪"。
  3. `SYS_PRIVCTL` 的 11 个子命令（`com.h:342-353`）有完整表，说明每个改哪个掩码。
  4. 目标行数 ≤ 1000（旧 17 为 924 行 + privctl/schedule 新增）。

### 21-syscall-signal

- **一句话定位**：信号类内核调用与内核内部的信号路径——从 `cause_sig` 到 PM 取走到处理器返回。
- **讲什么**：K-121、K-122、K-123、K-124、K-125
- **不讲什么**：异常→信号的映射表 → 12；进程停止/恢复的调度动作 → 20；POSIX 信号的用户态语义 → 04-stage-pm
- **前置**：12、14、16
- **后置**：无（终端篇）
- **事实底线**：
  - C：`do_kill.c`、`do_getksig.c`、`do_endksig.c`、`do_sigsend.c`、`do_sigreturn.c`、`do_mcontext.c`、`system.c:389 cause_sig`、`system.c:463`、`signal.h:264/273/274/280-282`、`arch/i386/signal.h:115 SC_MAGIC`
  - 非 C：`os/kernel/src/syscall_signal.rs`、`proc.rs`（`SigSet`/`p_pending`）、`kpriv.rs`（`PrivSignals`）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-121 | 信号双路径 | 概念 | 五个 do_*.c | 本篇主概念 | S：19 §1.1/§1.2 |
  | K-122 | `cause_sig` 与信号管理器 | 机制 | `system.c:389`；`priv.h` | 注入路径 | S：19 §1.4/§4.3 |
  | K-123 | 停止延迟与 `SIGSNDELAY` | 机制 | `system.c:463`；`signal.h:264` | 时序约束 | S：19 §1.5/§4.8 |
  | K-124 | sigcontext 三架构抽象 | 机制 | `os/arch/src/arch/signal*.rs` | 上下文保存 | S：19 §4.6 |
  | K-125 | mcontext 机器上下文 | 机制 | `do_mcontext.c`；`com.h:256-257` | **新增** | N：C 源码 |

- **验收标准**：
  1. 有一张"内核信号路径（cause_sig → GETKSIG → ENDKSIG）"与"POSIX 路径（SIGSEND → SIGRETURN）"并排时序图。
  2. `SIGS_IS_LETHAL` 的 backup 切换与 panic 子路径有明确说明。
  3. 有一句声明："哪些 CPU 异常变成哪些信号，见 12 的映射表"。
  4. 目标行数 ≤ 900（旧 931 行 + mcontext 新增）。

### 22-syscall-device

- **一句话定位**：设备类内核调用——驱动怎么注册中断、怎么访问端口，以及 x86-only 特性怎么在非 x86 上退化。
- **讲什么**：K-022、K-126、K-127、K-128、G-16（padconf）
- **不讲什么**：IRQ 分发的运行期机制 → 12；端口/IRQ 权限表结构 → 16；具体设备的驱动逻辑 → 16-stage-drivers
- **前置**：12、14、16、18
- **后置**：无（终端篇）
- **事实底线**：
  - C：`do_irqctl.c`、`do_devio.c`、`do_vdevio.c`、`arch/i386/do_sdevio.c`、`do_iopenable.c`、`do_readbios.c`、`arch/earm/do_padconf.c`、`interrupt.c:29/116/161/169`
  - 非 C：`os/kernel/src/syscall_device.rs`、`irq_manager.rs`、`os/arch/src/arch/*`（`PortIo`）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-022 | 驱动即进程与 IRQ 解耦 | 概念 | `interrupt.c:29`；`do_irqctl.c` | **新增**：本篇的心智前提 | N：OS 理论 + C 源码 |
  | K-126 | IRQ 钩子注册 | 机制 | `do_irqctl.c`；`interrupt.c:29` | 代表成员 | S：20 §1.1/§4.1 |
  | K-127 | 三类端口 I/O | 机制 | `do_devio.c`；`do_vdevio.c`；`do_sdevio.c` | 语义分层 | S：20 §1.2/§4.2–§4.4 |
  | K-128 | IOPL 提权与 BIOS 读取 | 机制 | `do_iopenable.c`；`do_readbios.c` | x86-only 退化 | S：20 §1.3/§4.5/§4.6 |
  | G-16 | `SYS_PADCONF` | 机制 | `com.h:267`；`do_padconf.c` | **新增**：架构特定设备调用 | N：C 源码 |

- **验收标准**：
  1. 有一张"7 个调用 × 架构可用性 × 需要的权限项"对照表。
  2. x86-only 的三个调用（SDEVIO/IOPENABLE/READBIOS）在非 x86 上的 BadCall 退化有明确说明。
  3. 有一句声明："IRQ 到达后内核怎么通知驱动，见 12"。
  4. 目标行数 ≤ 800（旧 829 行 + padconf 新增）。

### 23-syscall-clock

- **一句话定位**：时间类内核调用——查时间、设闹钟、改时钟、管虚拟定时器。
- **讲什么**：K-129；消费 13 的 `ClockState`/`TimerAction`
- **不讲什么**：时钟中断与定时器机制 → 13；`ClockState` 字段定义 → 13
- **前置**：13、14、16
- **后置**：无（终端篇）
- **事实底线**：
  - C：`do_times.c`、`do_setalarm.c`、`do_stime.c`、`do_settime.c`、`do_vtimer.c`
  - 非 C：`os/kernel/src/syscall_clock.rs`、`clock.rs`、`proc.rs`（`TimeStats`）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-129 | 5 个时间类调用 | 机制 | 上述五个 do_*.c | 本篇主体 | S：21 全文 |

- **验收标准**：
  1. 有一张"5 个调用 × 改哪个时间基准 × 谁有权调用"对照表。
  2. `ClockState` 以参数传入而非全局访问的测试收益有一节说明（保留旧 21 的 D5 论点）。
  3. 有一句声明："定时器如何到期、`vtimer_check` 在哪里被调，见 13"。
  4. 目标行数 ≤ 700（旧 829 行）。

### 24-syscall-misc

- **一句话定位**：杂项与信息类内核调用——查信息、追踪、槽位交换、采样开关、中止。
- **讲什么**：K-131、K-132、K-133、K-134、K-135；`call_vec` 未注册调用的兜底
- **不讲什么**：采样数据怎么用 → 27；栈回溯请求的实现 → 26
- **前置**：14、16
- **后置**：26、27
- **事实底线**：
  - C：`do_getinfo.c`、`do_trace.c`、`do_update.c`、`do_sprofile.c`、`do_abort.c`、`com.h:435-438`
  - 非 C：`os/kernel/src/misc.rs`、`grant.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-131 | getinfo 分派 | 机制 | `do_getinfo.c` | 信息查询 | S：25 §2.2/§4.2 |
  | K-132 | trace 分派 | 机制 | `do_trace.c` | 进程追踪 | S：25 §2.3/§4.3 |
  | K-133 | update 槽位交换 | 机制 | `do_update.c`；`com.h:435-438` | 槽位交换 | S：25 §2.4/§4.4 |
  | K-134 | sprofile 状态机 | 机制 | `do_sprofile.c` | 采样开关（机制归 27） | S：25 §2.5 |
  | K-135 | abort 内核中止 | 机制 | `do_abort.c`；`utility.c` | **新增** | N：C 源码 |

- **验收标准**：
  1. 有一张"5 个调用 × 谁有权发起 × 返回什么"对照表。
  2. 未使用调用号（11,12,20,29,30,37,38,41,42,47,48,49）有一行说明是保留位。
  3. 有一句声明："DIAGCTL STACKTRACE 的实际回溯实现见 26"。
  4. 目标行数 ≤ 700（旧 819 行）。

### 25-kernel-utility-and-terminal

- **一句话定位**：内核怎么说话、怎么死、怎么停下来——输出通道、panic 路径与有序关闭。
- **讲什么**：K-137、K-138、K-139、K-140、K-046（panic 路径侧）
- **不讲什么**：早期控制台的初始化 → 06；栈回溯的实现 → 26；采样与统计 → 27
- **前置**：06、12
- **后置**：26、30
- **事实底线**：
  - C：`utility.c`（93 行）、`main.c:351 prepare_shutdown`、`main.c:368 minix_shutdown`、`arch/i386/arch_reset.c`
  - 非 C：`lib.rs:1687 kernel_panic_diagnostic`、`lib.rs:1711 register_panic_diagnostic`、`kmess.rs`、`os/arch/src/arch/*`（`EarlyConsole`）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-137 | panic 路径与 panic handler | 机制 | `utility.c`；`lib.rs:1687/1711` | 终态之一 | S：27 §2.2/§3.1/§4.1 |
  | K-138 | `kputc` 与 kmess 缓冲 | 机制 | `utility.c`；`kmess.rs` | 输出通道 | S：27 §2.3/§3.2 |
  | K-139 | `_exit` 在内核被禁止 | 约束 | `utility.c` | 反直觉约束 | S：27 §2.4/§3.3 |
  | K-140 | shutdown 与 reboot | 机制 | `main.c:351/368`；`arch_reset.c` | **新增**：关闭与退出 | N：C 源码 |
  | K-046 | panic 路径下的控制台使用 | 机制 | `05:2014-2091` | 消费 06 的初始化 | S：01 §6.2 + 27 §3.2 |

- **验收标准**：
  1. 有三条终态路径的对照：panic（不可恢复）/ `minix_shutdown`（有序）/ `idle`（永不终止）。
  2. `prepare_shutdown` 为什么要用 1 秒定时器（`main.c:356-362`）有解释。
  3. 目标行数 ≤ 550（旧 27 为 392 行 + shutdown 新增）。

### 26-kernel-diagnostics

- **一句话定位**：内核怎么自检、怎么向人类报告自己的状态。
- **讲什么**：K-141、K-142、K-143、K-144、K-136（diagctl 消费侧）
- **不讲什么**：panic 与关闭 → 25；采样 profiling → 27；FPU 状态 → 28
- **前置**：10、12、15
- **后置**：无（终端篇）
- **事实底线**：
  - C：`debug.c`（563 行）、`exception.c:289/333`、`utility.c:39 util_stacktrace`、`do_diagctl.c`、`proc.c:1970 ser_dump_proc`
  - 非 C：`os/kernel/src/debug.rs`、`stacktrace.rs`、`os/arch/src/arch/stacktrace.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-141 | 调度队列 sanity check | 工具 | `debug.c`；`debug.rs` | 自检 | S：29 §2.2/§3.1 |
  | K-142 | 进程信息打印 | 工具 | `debug.c`；`proc.c:1970` | 状态导出 | S：29 §2.3/§3.2 |
  | K-143 | 栈回溯：FP 链遍历 | 机制 | `exception.c:289/333`；`stacktrace.rs` | **合并旧 32 全文** | S：32 |
  | K-144 | 内核栈 vs 用户栈回溯 | 约束 | `32:42-58` | 权限差异 | S：32 §1.3 |
  | K-136 | diagctl 与栈回溯请求 | 机制 | `do_diagctl.c` | 用户态入口 | S：32 §2.4 |

- **验收标准**：
  1. 有一张"三架构 FP 寄存器与栈帧布局"对照表。
  2. 循环检测、截断上限、读取失败容错三条健壮性措施各有说明。
  3. 有一句声明："panic 时会调用本设施，见 25"。
  4. 目标行数 ≤ 800（旧 29 为 369 行 + 旧 32 的 575 行，合并去重后收敛）。

### 27-profiling

- **一句话定位**：统计采样——机制、开关，以及为什么不实现 NMI profiling。
- **讲什么**：K-145、K-146；消费 24 的 `SYS_SPROF`
- **不讲什么**：watchdog 的 NMI 路径 → 30；时钟中断本身 → 13
- **前置**：13、24
- **后置**：无（终端篇）
- **事实底线**：
  - C：`profile.c`（157 行，全在 `#if SPROFILE`）、`profile.h`、`do_sprofile.c`
  - 非 C：`os/kernel/src/misc.rs`、`clock.rs`（init/stop/ack 接口）、`os/arch/src/*/clock.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-145 | 采样 profiling 与 `SPROFILE` | 机制 | `profile.c` | 本篇主体 | S：30 §1.1/§1.2/§2.x |
  | K-146 | NMI profiling（WONTFIX） | 演进 | `profile.c` NMI handler | 明确不做 | S：30 §3.3 |

- **验收标准**：
  1. 有一张"采样时钟 → 样本收集 → 用户态读取"的数据流图。
  2. NMI profiling 的 WONTFIX 理由独立成节，不写成"未实现"。
  3. 目标行数 ≤ 400（旧 368 行）。

### 28-fpu-context

- **一句话定位**：FPU 状态的 lazy 保存与恢复——为什么拖到真正用时才存。
- **讲什么**：K-147、K-148
- **不讲什么**：上下文切换的主流程 → 10；信号的上下文保存 → 21
- **前置**：10、12
- **后置**：无（终端篇）
- **事实底线**：
  - C：`arch_system.c:51-215`（fpu 函数族）、`proc.c:1922 copr_not_available_handler`、`proc.c:1961 release_fpu`、`exception.c:375/382`、`mpx.S:537-552`、`do_sigsend.c:86`、`include/arch/i386/include/fpu.h`、`cpulocals.h`
  - 非 C：`os/arch/src/arch/fpu_arch.rs`、`os/arch/src/*/fpu.rs`、`os/kernel/src/smp.rs`、`exception_dispatcher.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-147 | FPU lazy 上下文切换 | 机制 | `arch_system.c:51-215`；`proc.c:1922` | 本篇主体 | S：31 + 06 §3.6/§4.9（合并） |
  | K-148 | `#NM` 陷阱与 `fpu_owner` | 机制 | `mpx.S:537-552`；`smp.rs` | 所有权协议 | S：31 |

- **验收标准**：
  1. 有一张"CR0.TS 置位/清除的时机"表。
  2. 有一句声明"为什么不翻译 C 的 `fnsave`"并标注 `[ARCH]`。
  3. 目标行数 ≤ 500（旧 602 行 + 06 的 FPU 内容并入）。

### 29-usermapped-data

- **一句话定位**：内核数据怎么零 syscall 暴露给用户态，以及为什么 64 位重写不保留它。
- **讲什么**：K-149
- **不讲什么**：`KernelInfo` 的完整线格式 → 32；时钟状态的数据结构 → 13
- **前置**：01、13
- **后置**：32
- **事实底线**：
  - C：`usermapped_data.c`（15 行）、`arch/i386/usermapped_data_arch.c`、`usermapped_glo_ipc.S`、`kernel.lds`（`.usermapped` 段）
  - 非 C：`os/kernel/src/kerninfo.rs`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-149 | `.usermapped` 段机制 | 机制+演进 | `usermapped_data.c`；`kernel.lds` | 本篇主体 | S：28 全文 |

- **验收标准**：
  1. 有一张"8 个用户可见数据结构 × 保留/不保留 × 替代方案"的表。
  2. WONTFIX 决策（D1–D6）各有独立理由，不写成"未实现"。
  3. 目标行数 ≤ 400（旧 448 行）。

### 30-watchdog

- **一句话定位**：NMI watchdog 与 lockup 检测，以及为什么 64 位重写不实现。
- **讲什么**：K-150
- **不讲什么**：NMI profiling → 27；时钟中断 → 13
- **前置**：11、13
- **后置**：无（终端篇）
- **事实底线**：
  - C：`watchdog.c`（112 行）、`watchdog.h`（50 行）、`arch/i386/arch_watchdog.c`、`arch/earm/include/arch_watchdog.h`
  - 非 C：`os/kernel/src/misc.rs`（未实现的接入点）
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-150 | NMI watchdog 与 lockup 检测 | 机制+WONTFIX | `watchdog.c` | 本篇主体 | S：26 全文 |

- **验收标准**：
  1. lockup 检测的三条判据各有说明。
  2. WONTFIX 理由独立成节，并给出"若将来要接入，接入点在哪"。
  3. 目标行数 ≤ 300（旧 293 行）。

### 31-build-image-and-test-infra

- **一句话定位**：这套代码怎么变成可启动镜像，又怎么被验证。
- **讲什么**：K-151、K-152、K-171、K-160（依赖方向）
- **不讲什么**：链接脚本的内容细节 → 03；单个测试内核的逻辑 → 对应篇章（本篇只给索引）；跨模块线格式 → 32
- **前置**：00（可独立于主线阅读）
- **后置**：无（终端篇）
- **事实底线**：
  - 非 C：`os/Cargo.toml`（workspace 成员）、`os/README.md`、`os/xtask/src/*`、`os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld`、`os/qemu-tests/`（`test-kernels/kernel/bootstrap/`、`test-kernels/user/` 等 20+ 测试内核 + `.sh` 运行脚本）、`tools/gen-test-kernel.sh`、`tools/*.sh`、`scripts/update_minix.sh`、`os/tests/`、`os/benches/`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-151 | 构建、镜像与工具链 | 工具 | `os/Cargo.toml`；`os/xtask/`；`link.ld` | **新增**：正式文档 0 覆盖 | N：非 C 制品 |
  | K-152 | 测试基建 | 工具 | `os/qemu-tests/`；`tools/gen-test-kernel.sh` | **新增**：散落各篇 §5 | N：非 C 制品 |
  | K-171 | `no_std` 与测试策略 | 约束 | `test_helpers.rs` | 测试形态统一讲 | S：00 §1.5.2/§1.5.4 + 各篇 §5 |
  | K-160 | 跨 crate 依赖方向 | 约束 | `os/Cargo.toml`；`os/README.md` | 构建层约束 | S：todo.md §0 |

- **验收标准**：
  1. 有一张"三架构 target → 构建命令 → 产物"的对照表。
  2. 有一张"测试金字塔（单测 / QEMU 集成 / 端到端）"的图，并给出各层的运行命令。
  3. 各篇 §5 的长测试名清单在本篇有索引，正文只留"代表性 10 + `cargo test` 可重放"。
  4. 目标行数 ≤ 700（**新建**）。

### 32-cross-module-contracts

- **一句话定位**：跨模块与跨特权级的二进制契约——哪些东西改了就会两边同时坏。
- **讲什么**：K-153、K-154、K-155、K-156、K-157、K-158、K-159、K-163、K-161（协议相关两条）
- **不讲什么**：`KernelInfo` 的字段业务语义 → 02；各 syscall 的参数语义 → 18–24；
  `PlatformDesc` 的设计 → 05
- **前置**：01、14
- **后置**：无（终端篇）
- **事实底线**：
  - 非 C：`os/libs/minix-boot/src/kernel_info.rs`、`os/libs/minix-types/src/*`、`os/libs/minix-sys/src/arch_trap.rs`、`os/boot-shim/src/{uefi_helpers,opensbi_helpers}.rs`、`os/kernel/src/grant.rs`、`os/Cargo.toml`
  - C：`com.h:207-270`、`include/minix/ipc.h`、`include/arch/i386/include/fpu.h`
- **知识点清单**：

  | 编号 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
  |------|------|------|------|-------------|------|
  | K-153 | `KernelInfo` 线格式与 `validate()` | 接口 | `minix-boot/src/kernel_info.rs` | boot↔kernel 二进制契约 | S：01 §3.5/§4.1 + 28 §3.2 |
  | K-154 | syscall ABI | 接口 | `com.h:207-267`；`minix-types` | 用户态↔内核二进制接口 | S：13 §1.3 + 33 |
  | K-155 | 用户态 trap 桥（int-33 腿） | 接口 | `18-trap-bridge-design.md`；`arch_trap.rs` | **从设计稿升为正式篇** | S：18-trap-bridge-design |
  | K-156 | UEFI / OpenSBI 引导协议 | 接口 | `01:1867-2019`（旧附录 A） | **从 01 移入** | S：01 附录 A |
  | K-157 | 消息结构与字段访问约定 | 接口 | `include/minix/ipc.h`；`minix-types` | 跨进程线格式 | S：25 §3.9 |
  | K-158 | grant 表线格式 | 接口 | `grant.rs` | 授权表内存布局 | S：18 §4.3 |
  | K-159 | `Errno` newtype 与 `ToErrno` | 接口 | `minix-types/src/errno.rs` | 单一错误通道 | S：todo.md §0 |
  | K-163 | `#[cfg(target_arch)]` 使用纪律 | 约束 | `CLAUDE.md`；`lib.rs` | 硬件抽象纪律 | S：todo.md §0.6 |
  | K-162 | 硬件抽象 trait 清单 | 演进 | `os/arch/src/arch/*` | trait 全景 | S：todo.md §0 + 04 §3.x |

- **验收标准**：
  1. 有一张"契约 → 定义方 crate → 消费方 crate → 破坏后果"的对照表。
  2. trap 桥一节给出"用户态怎么发一个 syscall"的完整字节级路径（吸收旧 18-trap-bridge-design 的四项裁决）。
  3. `Errno` 一节说明为什么错误码必须有单一权威。
  4. 目标行数 ≤ 800（**新建**）。

---

## 6. 变更表

### 6.1 总表

> `旧 → 新` 逐条列出；"去向"列对存量知识点给出新位置（新篇章 + 小节）。

| 操作 | 类型 | 旧位置 | 新位置 | 理由 | 涉及知识点 | 去向 |
|------|------|--------|--------|------|-----------|------|
| C-01 | 重排 + 瘦身 | `00-kernel-overview.md` | `00-kernel-overview.md` | 去掉执行模型与 Rust 约束（越界），只留心智地图与导航 | K-001/006/015/018 | 00 §1/§2/§3；K-002/003/004/005 → 01 §1；K-073/012 → 11；K-163 → 32 |
| C-02 | **新建** | `99-global-concepts.md`（55 行） | `01-kernel-foundations.md` | 99 被 114 文件引用 273 次却排在末尾，是结构性矛盾；前置到 01 | K-002~005/007/008/010/013/019/050/051/052/057/058/165 | 01 §1–§7 |
| C-03 | 归档 | `99-global-concepts.md` | 归档（内容并入 01） | 内容被完全吸收 | 同上 | — |
| C-04 | 重排 + 瘦身 | `01-boot-shim-bootstrap.md`（2019） | `02-firmware-handoff.md` | 附录 A（UEFI 指南 130 行）与页表多级结构论证移出 | K-023/024/030/156 | 02 §2–§4；K-161c → 03；K-156 → 32；K-025 → 03 |
| C-05 | 重排 | `02-higher-half-kernel.md`（1329） | `03-kernel-image-and-entry.md` | 内容基本对应，压缩 riscv64 bug 附录 | K-025~029/161a/161c | 03 §2–§4 |
| C-06 | 重排 | `03-kmain-cstart.md`（1197） | `04-kmain-and-protection.md` | 内容基本对应，压缩威胁模型附录 | K-031~036/010 | 04 §2–§4 + 附录 |
| C-07 | 保留 | `04-platform-discovery.md`（1255） | `05-platform-discovery.md` | 内容与位置均合理，只做瘦身 | K-037~042 | 05 §2–§4 |
| C-08 | **拆分** | `05-clock-interrupt-init.md`（2263） | `06-clock-and-interrupt-bringup.md` + `13-clock-and-timers.md` | `ClockState` 实现详解（~500 行）是运行期内容，塞在"初始化"篇是 05 膨胀主因 | K-043~047 留 06；K-085/086/087/088/071/019 移 13 | 06 §2–§4（保留）；13 §2–§4（移入）；K-011 → 12 |
| C-09 | 重排 | `06-proc-init-boot-proc.md`（1375） | `07-process-table-and-boot-image.md` | 保留阶段 C 主线；删除 FPU/SMP/KPriv/fork 对照等越界内容 | K-009/016/017/048/049/053/054/055/064/065/066/170 | 07 §2–§4；K-003 → 01；K-147 → 28；K-073/074 → 11；K-054/056/060~063 → 16；K-113 → 20 |
| C-10 | 重排 | `07-cross-space-init.md`（691） | `08-cross-space-capability.md` | 内容对应，压缩 | K-109 | 08 §1–§4 |
| C-11 | 归档 | `07-paging_init_gpt.md`（358） | 归档（GPT 分析记录，非概念文档） | 是设计讨论记录，不是教学文档；其结论若有效并入 08 | 无知识点 | 08 §3 |
| C-12 | 重排 + 拆分 | `08-system-init-boot-finish.md`（1019） | `09-system-init-and-boot-finish.md` | §4.1/§4.2（Syscall 枚举与分派）移 14 | S18–S22、K-032/076 | 09 §2–§4；K-166 → 14 |
| C-13 | 重排 | `09-vm-boot-protocol.md`（594） | `19-syscall-vmctl.md` | **位置从 09 移到 19**：触发时机在 `switch_to_user` 之后（D-1） | K-013/110/111 | 19 §2–§4 |
| C-14 | 保留 + 合并 | `10-switch-to-user.md`（506） + `11-scheduling-primitives.md`（716） | `10-switch-to-user.md` | 调度原语的唯一调用者是调度循环，合并后消除"先讲循环再讲队列"的倒置 | K-067/068/059 + K-069/070/071/072 | 10 §2–§4；K-071 → 13 |
| C-15 | 归档 | `11-scheduling-primitives.md` | 归档（内容并入 10 / 13） | 见 C-14 | 同上 | — |
| C-16 | 重排 | `12-ipc-core.md`（1209） | `15-ipc-core.md` | §4.7 权限四层检查移 17；位置后移以让权限篇前置 | K-014/021/096~102/167 | 15 §2–§4；K-103 → 17 |
| C-17 | 重排 | `13-syscall-dispatch.md`（949） + `33-syscall-caller-api.md`（222） | `14-syscall-dispatch.md` | 33 的"调用者身份降级"是分派框架的一部分，合并消除一篇过短的设计记录 | K-089~095/112/166 + K-094 | 14 §2–§4 |
| C-18 | 归档 | `33-syscall-caller-api.md` | 归档（并入 14） | 见 C-17 | K-094 | 14 §2 |
| C-19 | 重排 | `14-exception-interrupt.md`（661） | `12-exception-and-interrupt.md` | 位置前移（在时钟之前），并吸收 05 §1.0 的异常/中断概念 | K-011/079~084 | 12 §2–§4 |
| C-20 | 重排 + 合并 | `15-clock-timer.md`（1162） + 05 §4.1 | `13-clock-and-timers.md` | 见 C-08 | K-019/085/086/087/088/071 | 13 §2–§4 |
| C-21 | **拆分** | `16-smp.md`（1298） | `09`（启动侧）+ `11-kernel-execution-model.md`（运行侧） | SMP 横跨启动链与运行期；拆开后消除 17 对 16 的时序错位 | K-076 → 09 + 11；K-073/074/075/077/078/164 → 11 | 09 §4；11 §2–§4 |
| C-22 | 重排 | `17-syscall-process.md`（924） | `20-syscall-process.md` | 补入 privctl 与 schedule 两个缺口 | K-113~118 + K-119/120 | 20 §2–§4 |
| C-23 | 重排 + 合并 | `18-syscall-copy.md`（809） + `24-cross-space-runtime.md`（468） | `18-syscall-cross-space.md` | 两篇讲同一件事（syscall 侧 vs 库宏侧），合并后消除 18 对 24 的前向引用 | K-106/107/108/169/093 + K-130 | 18 §2–§4 |
| C-24 | 归档 | `24-cross-space-runtime.md` | 归档（并入 18） | 见 C-23 | 同上 | — |
| C-25 | 重排 | `19-syscall-signal.md`（931） | `21-syscall-signal.md` | 补入 mcontext | K-121~124 + K-125 | 21 §2–§4 |
| C-26 | 重排 | `20-syscall-device.md`（829） | `22-syscall-device.md` | 补入 padconf 与"驱动即进程"概念 | K-126/127/128 + K-022/G-16 | 22 §2–§4 |
| C-27 | 重排 | `21-syscall-clock.md`（829） | `23-syscall-clock.md` | 内容对应 | K-129 | 23 §2–§4 |
| C-28 | 重排 | `22-privilege.md`（622） | `16-privilege-model.md` | **位置前移**到 syscall 组之前，消除 17/19/20/21 四处前向引用 | K-020/054/056/060~063/168 | 16 §2–§4 |
| C-29 | 重排 + 合并 | `23-ipc-filter.md`（523） + 12 §4.7 | `17-ipc-filtering.md` | 紧邻权限篇 | K-103/104/105 | 17 §2–§4 |
| C-30 | 重排 | `25-misc-unported.md`（819） | `24-syscall-misc.md` | 补入 abort；diagctl 的回溯实现移 26 | K-131~134 + K-135 | 24 §2–§4；K-136 → 26 |
| C-31 | 重排 | `26-watchdog.md`（293） | `30-watchdog.md` | 移入支线 | K-150 | 30 §2–§4 |
| C-32 | 重排 + 合并 | `27-kernel-utility.md`（392） | `25-kernel-utility-and-terminal.md` | 补入 shutdown/reboot | K-137/138/139/046 + K-140 | 25 §2–§4 |
| C-33 | 重排 | `28-usermapped-data.md`（448） | `29-usermapped-data.md` | 移入支线 | K-149 | 29 §2–§4 |
| C-34 | 重排 + 合并 | `29-kernel-debug.md`（369） + `32-stack-tracing.md`（575） | `26-kernel-diagnostics.md` | 两者都是"内核如何报告自己的状态" | K-141/142/143/144/136 | 26 §2–§4 |
| C-35 | 归档 | `32-stack-tracing.md` | 归档（并入 26） | 见 C-34 | 同上 | — |
| C-36 | 重排 | `30-kernel-profile.md`（368） | `27-profiling.md` | 移入支线 | K-145/146 | 27 §2–§4 |
| C-37 | 重排 | `31-fpu-context-switching.md`（602） | `28-fpu-context.md` | 移入支线，吸收 06 的 FPU 内容 | K-147/148 | 28 §2–§4 |
| C-38 | **新建内容** | —（缺口 G-01，无旧文档） | `25-kernel-utility-and-terminal.md §3` | shutdown/reboot 是"关闭与退出"固定清单项，旧目录无正式篇；与 panic/kputc 同属"内核终态"语义，不单独占一个编号 | K-140 | 25 §3 |
| C-39 | **新建** | —（缺口 G-02/G-03） | `31-build-image-and-test-infra.md` | 构建与测试基建正式文档 0 覆盖 | K-151/152/171/160 | 31 §1–§5 |
| C-40 | **新建** | —（缺口 G-13） | `32-cross-module-contracts.md` | 线格式与契约分散在 5 处，无统一篇；吸收 01 附录 A 与旧 18-trap-bridge-design | K-153~159/163/162 | 32 §1–§5 |
| C-41 | 归档 | `18-trap-bridge-design.md`（87） | 归档（内容升入 32 §2） | 是设计稿（编号还与 18-syscall-copy 冲突），结论升为正式篇 | K-155 | 32 §2 |
| C-42 | 保留（不动） | `todo.md` / `06-todo.md` / `checklist.md` / `endpoint_todo.md` / `smp_todo.md` / `smp_gpt*.md` / `panic-in-drop.md` | 参考材料，原样保留 | 这些是流程与记录产物，不是教学文档；`panic-in-drop.md` 被代码注释引用，不动 | — | — |

### 6.2 按操作类型汇总

| 类型 | 数量 | 条目 |
|------|------|------|
| 重排（含改名） | 21 | C-01/04/05/06/07/09/10/12/13/16/17/19/25/26/27/28/31/33/36/37 + C-30（25→24，含 abort 新增） |
| 拆分 | 2 | C-08（05 拆为 06+13）、C-21（16 拆为 09+11） |
| 合并 | 4 | C-14（10+11）、C-17（13+33）、C-23（18+24）、C-34（29+32） |
| 新建（篇章） | 3 | C-02（01-kernel-foundations）、C-39（31-build-image-and-test-infra）、C-40（32-cross-module-contracts） |
| 新建（内容，不占编号） | 1 | C-38（shutdown/reboot 内容并入 25 §3） |
| 归档 | 7 | C-03（99）、C-11（07-paging_init_gpt）、C-15（11）、C-18（33）、C-24（24）、C-35（32）、C-41（18-trap-bridge-design） |

> **C-38 说明**：关闭与重启最初考虑独立为 `30-shutdown-and-reboot`，最终决定**与 `27-kernel-utility`
> 合并为 `25-kernel-utility-and-terminal`**——两者同属"内核终态"语义（怎么说话/怎么死/怎么停），
> 独立成篇会只有 ~200 行且语义单薄。§4.2 表中编号 30 为 `watchdog`，25 为终态篇，与此处一致。

---

## 7. 缺漏新篇

> §3.2 的缺口逐项落实；不允许"待定"。

| 缺口 | 主题 | 为什么重要 | 原料在哪 | 归哪一篇 | 验收标准 |
|------|------|-----------|---------|---------|---------|
| G-01 | 关闭与重启 | 没有它，"内核怎么结束"这条路径完全没文档；`main.c:351/368` 是真实存在的 C 代码；属提示词固定清单"关闭与退出"项 | `main.c:351-398`；`arch/i386/arch_reset.c`；`utility.c` panic 对照 | **25-kernel-utility-and-terminal §3** | 三条终态（panic / shutdown / idle）对照表可 grep；`prepare_shutdown` 的 1 秒定时器理由有解释 |
| G-02 | 构建与工具链 | `xtask` 与三架构 target 是读者复现系统的第一道门；正式文档 0 命中；属"构建与工具链"清单项 | `os/Cargo.toml`；`os/xtask/src/*`；`os/README.md`；`scripts/update_minix.sh` | **31-build-image-and-test-infra §1–§2** | "三架构 target → 构建命令 → 产物"对照表；依赖方向图 |
| G-03 | 测试基建 | 各篇 §5 的测试清单是开发文档味道重灾区；统一到一篇后各篇可只留"代表性 10" | `os/qemu-tests/test-kernels/`（20+ 内核）；`tools/gen-test-kernel.sh`；`os/tests/` | **31-build-image-and-test-infra §3–§4** | 测试金字塔图 + 各层运行命令；各篇 §5 有索引 |
| G-04 | `SYS_SETGRANT` | grant 是 safecopy 的前提，却没人讲"授权表是谁建的" | `system/do_setgrant.c`；`os/kernel/src/grant.rs`；`com.h:242` | **18-syscall-cross-space §2** | 7 个拷贝类调用对照表含 SETGRANT 行；grant 建立→验证→消费链路完整 |
| G-05 | `SYS_GET/SETMCONTEXT` | 机器上下文是调试器与协程的基础；现有只一处提及 | `system/do_mcontext.c`；`com.h:256-257` | **21-syscall-signal §2** | 信号组对照表含两个 mcontext 调用；与 sigcontext 的关系有说明 |
| G-06 | `SYS_SCHEDULE` | 用户态 sched 服务的内核入口；`06-stage-sched` 依赖它 | `system/do_schedule.c`；`system.c:642-723 sched_proc` | **20-syscall-process §2** | 10 个调用对照表含 SCHEDULE；能发起者与权限有说明 |
| G-07 | `SYS_ABORT` | Minix3 的"内核主动中止"路径，与 panic/shutdown 三足鼎立 | `system/do_abort.c`；`com.h:237` | **24-syscall-misc §2** | 5 个调用对照表含 ABORT；与 `utility.c` 的关系有说明 |
| G-08 | 三种时间基准 | C 有三种"现在"，13/23 都隐含使用却从没讲清区别 | `clock.c:178/187/195/203/212/220` | **13-clock-and-timers §1** | "realtime/monotonic/boottime 各被谁读写"对照表；`SYS_STIME`/`SYS_SETTIME` 分别改哪个有答案 |
| G-09 | 驱动即进程与 IRQ 解耦 | 这是理解"驱动为什么不能自己装中断 handler"的前提，读者缺它就会把 22 读成 API 手册 | `interrupt.c:29`；`system/do_irqctl.c` | **22-syscall-device §Ch1** | §Ch1 有一节专门讲"为什么中断要经内核转一道" |
| G-12 | LAPIC/IOAPIC 编程 | x86 中断控制器初始化的架构侧，05 只提了文件名 | `arch/i386/apic.c`；`apic_asm.S` | **06-clock-and-interrupt-bringup §2.2（并入 K-044）** | x86 中断控制器一节含 PIC 与 APIC 两条路径 |
| G-13 | 跨模块契约统一视图 | 五处分散的契约没有统一篇，改一个字段不知道会破坏谁 | 见 32 的事实底线 | **32-cross-module-contracts** | "契约 → 定义方 → 消费方 → 破坏后果"对照表 |
| G-14 | 内核执行模型统一讲述 | 00 讲概念、16 讲 SMP，两处各讲一半且互相引用 | 见 11 的事实底线 | **11-kernel-execution-model** | BKL 释放清单 + per-CPU/全局对照表 |
| G-15 | IPC 权限与过滤集中 | 12 与 23 重叠 | 见 17 的事实底线 | **17-ipc-filtering** | "一条 SEND 的完整检查序列"表 |
| G-16 | `SYS_PADCONF` | ARM 板级引脚配置调用，属设备类；属"架构特定设备调用" | `com.h:267`；`arch/earm/do_padconf.c` | **22-syscall-device §2** | 7 个调用对照表含 PADCONF；非 ARM 上的退化有说明 |
| G-10 | 硬件断点 | — | `arch/i386/breakpoints.c` | **明确不做**（见 §3.6） | 在 26 留一句"本设施不保留"并给理由 |
| G-11 | PCIe 配置空间 | — | `arch/i386/oxpcie.c` | **明确不做**（归 `16-stage-drivers`） | 在 06 的 `arch_init` 一节留一句"设备枚举不在本 stage" |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（逐节）

> 覆盖所有发生变化文档的每一节。类型：原样搬移 / 改写 / 合并 / 拆分 / 删除。

| 旧位置 | 旧内容（一句话） | 新位置 | 类型 | 备注（断链风险） |
|--------|-----------------|-------|------|-----------------|
| 00 §1.1 内核是什么 | 内核 vs 服务的类比 | 00 §1 | 原样搬移 | 低：00 保留编号 |
| 00 §1.2 核心职责表 | 七项职责 | 00 §1 | 原样搬移 | 低 |
| 00 §1.3 内核与 VM 关系 | 协议图 | 00 §1 | 原样搬移 | 低 |
| 00 §1.4 执行模型（含 1.4.1 三维度） | 运行态实体、kernel task、IDLE | **01 §1–§2** | 拆分 | **中**：`06 §1.1.4` 与本文互相引用，两处都要同步 |
| 00 §1.5 内核执行模型约束（Rust 必读） | BKL/类型/硬件/差异速查 | **11 §2 + 32 §1** | 拆分 | **高**：这是很多下游文档引用的"内核开发约束"，迁移后所有引用需重定向 |
| 00 §2 启动线图 | 五阶段 ASCII | 00 §2 | 原样搬移 | 低 |
| 00 §3 文档导航表 | 01~30 的叙事逻辑 | 00 §3（重写为 33 篇） | 改写 | **高**：整张表重建，是 B 相必改项 |
| 00 §4 与 VM/PM/VFS 差异 | fork 工作量分配 | 00 §4 / 20 §2 | 拆分 | 低 |
| 00 §5 设计原则 | 内核态/VM 协议/上下文 | 00 §5 | 原样搬移 | 低 |
| 00 §6 与 02-stage-vm 关系 | 交叉参照表 | 00 §6 | 改写（编号更新） | 中：含 `02-stage-vm/draft/` 路径，需确认仍存在 |
| 01 §1 概述（含 1.3 启动前状态） | boot-shim 定位 | 02 §1 | 原样搬移 | 低 |
| 01 §2 C 源码分析（2.0–2.5） | multiboot、memmap | 02 §2 | 原样搬移 | 中：`01 §2.5` 被 `06-todo.md` Step 0 记录为下游锚点，需同步 |
| 01 §3 Rust 设计决策（3.1–3.8） | UEFI/页表/KernelInfo | 02 §3（+ 03 / 32） | 拆分 | 中：3.3（4 级页表）→ 03；3.1/附录 → 32 |
| 01 §4 实现详解（4.1–4.7） | KernelInfo、Paging、boot-shim | 02 §4 | 原样搬移 | 低 |
| 01 §5 测试要点 | 单测 + qemu-tests | **31 §4** | 合并 | 中：qemu-tests 描述移出 |
| 01 §6 启动失败与诊断（6.1–6.5） | EarlyConsole、panic 路径 | **06 §4 / 25 §3** | 拆分 | 中 |
| 01 附录 A UEFI 极简指南 | 固件协议教学 | **32 §2** | 拆分 | **高**：130 行整体搬家 |
| 02 §1 概述 | 中间态问题 | 03 §1 | 原样搬移 | 低 |
| 02 §2 C 源码分析（2.0–2.4） | lds、head.S、切栈 | 03 §2 | 原样搬移 | 低 |
| 02 §3 Rust 设计决策（3.1–3.5） | AT() 不保留、独立 ELF、HigherHalf | 03 §3 | 原样搬移 | 低 |
| 02 §4 实现详解（4.1–4.5） | link.ld、ELF 加载、三架构 | 03 §4 | 原样搬移 | 低 |
| 02 附录 A riscv64 L2 bug | 调试记录 | **压为一句 + git 指针** | 删除（压缩） | 中：有人可能引用 `02 附录 A` |
| 03 §1 概念（1.0–1.10） | 保护结构、特权级、三架构 | 04 §1 | 原样搬移 | 低 |
| 03 §2 C 源码分析（2.1–2.6） | kmain 入口、cstart、prot_init | 04 §2 | 原样搬移 | 低 |
| 03 §3 Rust 设计（3.1–3.3） | ProtectionArch/TrapEntryArch | 04 §3 | 原样搬移 | 低 |
| 03 §4 实现详解（4.1–4.6） | trait、init_protection | 04 §4 | 原样搬移 | 低 |
| 03 附录 A 为什么切内核栈 | 威胁模型 | 04 附录（压缩到 ≤60 行） | 改写 | 低 |
| 04 §1/§2/§3/§4 | 硬编码问题、ACPI、DTB、PlatformDesc | 05 §1–§4 | 原样搬移 | 低 |
| 05 §1.0 中断模型 | 同步 vs 异步 | **12 §1.1** | 合并 | 中：05 与 14 两处都有，需一并处理 |
| 05 §2.1–§2.6 C 源码分析 | init_clock、intr_init、arch_init | 06 §2 | 原样搬移 | 低 |
| 05 §3.1–§3.8 Rust 设计决策 | ClockArch、InterruptController、EarlyConsole | 06 §3 | 原样搬移 | 低 |
| 05 §4.1 ClockState（~500 行） | 时钟状态实现详解 | **13 §2** | **拆分** | **高**：2263 行里最大的一块搬家 |
| 05 §4.2–§4.14 实现详解 | ClockArch 三架构、GICv3、PLIC、ArchInit | 06 §4（ClockState 相关移 13） | 拆分 | **高** |
| 05 §5 测试要点 | 单测 + 覆盖分析 | **31 §4** | 合并 | 中 |
| 06 §1.1.4 三类实体（含证据链） | kernel task 无执行流 | **01 §2** | 合并 | **高**：`00-kernel-overview` §1.4.1 引用本文 |
| 06 §1.2 CPU 四问 | 寄存器/入口/栈/地址空间 | 07 §1 | 原样搬移 | 低 |
| 06 §1.3 三件套 | 进程表/特权表/RTS | 01 §3–§5（概念）+ 07 §2（存储） | 拆分 | 高 |
| 06 §1.4 boot image / VM 鸡生蛋 | 清单与自举 | 07 §1 | 改写（压缩） | 中 |
| 06 §1.5 阶段 C 执行节拍 | 清空→填充→唤醒 | 07 §1 + §4 | 拆分 | 中 |
| 06 §2.0 存储约束总述 | 三层因果链 | 07 §2.0 | 原样搬移 | 低 |
| 06 §2.1 进程表存储 | 数组 + 6 组字段 | 07 §2.1 | 原样搬移 | 低 |
| 06 §2.2 特权表存储 | 分治/双表/零堆 | 07 §2.2 | 原样搬移 | 中：与 16 分担 |
| 06 §2.3 boot image 存储 | image[] 与链路 | 07 §2.3 | 原样搬移 | 低 |
| 06 §3.1–§3.13 Rust 设计决策 | 14 个小节 | 07 §3（保留有独立决策的）+ 删（FPU/SMP/KPriv/调度/boot→running） | 拆分 + 删除 | **高**：`06-todo.md` §9 的 A1–A15 验收引用了具体小节号 |
| 06 §4.0–§4.9 实现详解 | 10 个小节 | 07 §4（主流程保留）+ 删（4.6/4.8/4.9） | 拆分 + 删除 | **高** |
| 06 §5 测试要点 | 68 测试名清单 | **31 §4**（只留代表性 10） | 合并 + 删除 | 中 |
| 06 §6.1 职责边界矩阵 | 12 行转交表 | 07 §6 | 改写（编号更新） | 中 |
| 07 §1–§4（cross-space-init） | 临时窗口 vs direct_map | 08 §1–§4 | 改写（压缩） | 低 |
| 07 附录 A/B | 时序图与对照 | 08 附录 | 原样搬移 | 低 |
| 08 §1–§4 | system_init、bsp_finish_booting | 09 §1–§4 | 原样搬移 | 中 |
| 08 §4.1/§4.2 Syscall 枚举与分派 | `Syscall` enum、dispatch | **14 §4** | 拆分 | **高** |
| 08 附录 A smp_init 启动位次 | SMP 时序 | **09 §4** | 合并 | 中 |
| 09 §1–§4（vm-boot-protocol） | VMCTL 协商 | **19 §1–§4** | 原样搬移（**编号 09 → 19**） | **高**：09 被 37 文件引用 123 次 |
| 10 §1–§4 | switch_to_user、idle | 10 §1–§4 | 原样搬移 | 低（编号不变） |
| 11 §1–§4 + 附录 A | 调度原语、RTS 履历 | **10 §2–§4**（原语）+ **13 §2**（quantum）+ 附录履历保留在 10 | 拆分 | **高**：11 被 40 文件引用 132 次 |
| 12 §1–§4 | IPC 六原语、delivermsg、senda | 15 §1–§4 | 原样搬移（**12 → 15**） | **高**：12 被 38 文件引用 126 次 |
| 12 §4.7 check_ipc_permission 四层 | 权限检查 | **17 §2** | 拆分 | 中 |
| 13 §1–§4 | syscall 分派、VMSUSPEND、finish/resume | 14 §1–§4 | 原样搬移（**13 → 14**） | **高**：13 被 32 文件引用 75 次 |
| 13 §6 已知缺口与限制 | 进度式表述 | 参考材料 | 删除 | 低 |
| 14 §1–§4 + 附录 | 异常分流、页错误、IRQ | 12 §1–§4 + 附录 | 原样搬移（**14 → 12**） | **高**：14 被 33 文件引用 105 次 |
| 15 §1–§4 | 时钟、定时器、闹钟 | 13 §1–§4 | 原样搬移（**15 → 13**） | **高**：15 被 37 文件引用 106 次 |
| 16 §1–§4 | BKL、per-CPU、IPI、AP 启动 | **11 §1–§4**（运行侧）+ **09 §4**（启动侧） | 拆分 | **最高**：16 被 45 文件引用 186 次 |
| 17 §1–§4 | fork/exec/exit/clear/runctl/schedctl/statectl | 20 §1–§4 | 原样搬移 + 新增 privctl/schedule | 中 |
| 18 §1–§4 | vircopy/safecopy/umap/vumap/memset | 18 §1–§4 | 原样搬移（编号不变） | 低 |
| 18-trap-bridge-design §一~§五 | trap 桥四项裁决 | **32 §2** | 改写（升为正式篇） | 中：被 `os/libs/minix-sys/src/arch_trap.rs:3` 引用 |
| 19 §1–§4 | 信号双路径、cause_sig、SIGSEND | 21 §1–§4 | 原样搬移（**19 → 21**） | 中：19 被 30 文件引用 67 次 |
| 20 §1–§4 | IRQ、端口 I/O、x86-only | 22 §1–§4 | 原样搬移（**20 → 22**） | 低：20 仅 10 文件 22 次 |
| 21 §1–§4 | 5 个时间类调用 | 23 §1–§4 | 原样搬移（**21 → 23**） | 低：21 仅 12 文件 30 次 |
| 22 §1–§4 | struct priv、三类掩码、KPriv | 16 §1–§4 | 原样搬移（**22 → 16**） | **高**：22 被 31 文件引用 98 次 |
| 23 §1–§4 | IPC 三层过滤 | 17 §1–§4 | 原样搬移（**23 → 17**） | 中 |
| 24 §1–§4 | 跨空间拷贝运行时、VMSUSPEND | **18 §1–§4** | 合并（**24 → 18**） | 中：24 被 16 文件 40 次 |
| 25 §1–§4 | getinfo/trace/update/sprofile | 24 §1–§4 | 原样搬移（**25 → 24**） | 中 |
| 26 §1–§4 | NMI watchdog | 30 §1–§4 | 原样搬移（**26 → 30**） | 低 |
| 27 §1–§4 | panic/kputc/_exit | 25 §1–§4 | 原样搬移 + 新增 shutdown（**27 → 25**） | 低：27 仅 5 文件 10 次 |
| 28 §1–§4 + 附录 A | `.usermapped` 段 | 29 §1–§4 | 原样搬移（**28 → 29**） | 低 |
| 29 §1–§4 | debug 设施 | **26 §1–§4** | 合并（**29 → 26**） | 低 |
| 30 §1–§4 | profiling | 27 §1–§4 | 原样搬移（**30 → 27**） | 低 |
| 31 §1–§4 | FPU lazy | 28 §1–§4 | 原样搬移（**31 → 28**） | 低 |
| 32 §1–§4 | 栈回溯 | **26 §2–§4** | 合并（**32 → 26**） | 低 |
| 33 §1–§7 | 调用者句柄化 | **14 §2** | 合并（**33 → 14**） | 低：33 仅 5 文件 9 次 |
| 99 §1 Endpoint 与 generation | 标识编码 | **01 §3** | 合并（**99 → 01**） | **最高**：99 被 114 文件引用 273 次 |

### 8.2 引用迁移表（文档之间 + 代码注释）

| # | 旧引用 | 旧目标 | 新目标 | 验证方式 |
|---|--------|--------|--------|---------|
| R-1 | `99-global-concepts.md`（273 次） | 99 | `01-kernel-foundations.md` | `rg "99-global-concepts" notes/ os/` 全量替换后重跑，确认 0 残留 |
| R-2 | `os/servers/vm/src/boot.rs:14` 注释引用 `01-stage-kernel/09-vm-boot-protocol.md` | 09-vm-boot-protocol | `19-syscall-vmctl.md` | 改注释；`cargo check -p minix-vm` |
| R-3 | `os/servers/rs/src/ipc_mask.rs:148` 注释引用 `01-stage-kernel/23-ipc-filter.md` | 23-ipc-filter | `17-ipc-filtering.md` | 改注释；`cargo check -p minix-rs` |
| R-4 | `os/kernel/src/kerninfo.rs:14` 注释引用 `01-stage-kernel/28-usermapped-data.md` D1 | 28-usermapped-data | `29-usermapped-data.md` D1 | 改注释；`cargo check -p minix-kernel` |
| R-5 | `os/libs/minix-sys/src/arch_trap.rs:3` 注释引用 `01-stage-kernel/18-trap-bridge-design.md` | 18-trap-bridge-design | `32-cross-module-contracts.md` §2 | 改注释；`cargo check -p minix-sys` |
| R-6 | `os/kernel/src/test_helpers.rs:4` 注释引用 `panic-in-drop.md` §1 | 不变（参考材料保留） | 不变 | 无需迁移 |
| R-7 | 各文档内 `详见 NN-xxx.md §Y` 的编号引用 | 旧编号 | 新编号 | 按 §8.1 逐条替换；`rg "\]\(\./?[0-9]{2}-" ` 全量核对 |
| R-8 | `03-stage-rs/07-rs-period-heartbeat.md` 引用 `06 §2.1/§2.2/§2.6` | 06 旧小节号 | 需按新 07 的小节号重定 | `rg "06 §\|proc-init-boot-proc\.md §" notes/` 后逐条核对 |
| R-9 | `00-kernel-overview.md` 引用 `06 §1.1.4` | 06 §1.1.4 | `01 §2` | 见 R-10 |
| R-10 | `12-ipc-core.md` / `13-syscall-dispatch.md` 把 06 当"struct proc 字段语义"权威 | 06 | **语义归属变更**：字段导航权威仍是 `07 §2.1`，字段语义权威是 `16` | 重定向，不是编号替换 |
| R-11 | `06-todo.md` §9 A1–A15 引用的 `06 §3.x/§4.x` 小节号 | 06 旧小节 | 旧 06 归档，todo 作为历史记录保留但加一行"v3 后章节号已变，见 07" | 在 `06-todo.md` 顶部加注记 |
| R-12 | 各 stage 的 `plan.md` 引用 `01-stage-kernel/NN-*.md` | 旧编号 | 新编号 | `rg "01-stage-kernel/" notes/` 列出全部（约 200+ 处），按映射表批量 sed |

### 8.3 断链成本摘要

| 指标 | 值 |
|------|-----|
| 受影响引用总数（fork-syscall-rewrite 全树 .md，排除 doc_rerank_*） | **≈ 2 400 处**（35 篇文档的命中数求和） |
| 编号发生变化的文档 | **27 篇**（仅 00/10/18 三篇编号不变；01→02、02→03、03→04、04→05、05→06、06→07、07→08、08→09、09→19、11→10(合并)、12→15、13→14、14→12、15→13、16→11、17→20、19→21、20→22、21→23、22→16、23→17、24→18、25→24、26→30、27→25、28→29、29→26、30→27、31→28、32→26、33→14、99→01） |
| 热点文件（>100 次引用） | `99-global-concepts`(273) / `16-smp`(186) / `06-proc-init-boot-proc`(132) / `11-scheduling-primitives`(132) / `12-ipc-core`(126) / `09-vm-boot-protocol`(123) / `15-clock-timer`(106) / `14-exception-interrupt`(105) |
| 跨 stage 引用 | ≈ 200+ 处（`rg "01-stage-kernel/" notes/` 命中，主要来自各 stage 的 `plan.md` 与 `edge_todo.md`） |
| 代码注释引用 | **5 处**（见 §8.2 R-2~R-6），其中 4 处需改 |
| 建议的批量修改方式 | ① 建立旧→新编号映射表（本表 §8.3 第二行）；② 对 `notes/` 全树执行 `sed` 批量替换文件名与 `NN-name.md` 形态的引用；③ 对锚点级引用（`NN §X.Y`）单独人工核对，因为小节号会随正文重写而漂移；④ 代码注释的 4 处逐个 `cargo check` 验证；⑤ B 相每写完一篇，立即跑一次 `rg "<旧编号>"` 确认清零 |

---

## 9. 验证与自检门

### 9.1 四种机械检查

| 检查 | 方法 | 结果 |
|------|------|------|
| **1. 前向引用扫描** | 逐篇检查 §5 契约的"前置"字段，确认只指向更早编号 | **通过**。33 篇的前置字段全部只指向更小编号。例外：11 讲 BKL 释放窗口时指向 13（时钟），13 讲 quantum 时指向 11 的 BKL——这两处已在 §1.3 序差表 D-3 登记并给出双向回指补偿。其余 D-1/D-2/D-4/D-5/D-8 均为"旧目录的前向引用被新目录修正"，不再是例外 |
| **2. 依赖关系图无环** | 由契约的"前置"关系构图 | **通过**。序差表 D-3 形成的 11↔13 环已在 D-3 行拆解为"11 只声明结论 + 13 回指"，不构成知识依赖环（11 不依赖 13 的内容即可读懂） |
| **3. 覆盖率 100%** | 知识点池 172 条逐条查"新归属"列 | **通过**。172 条全部有归属篇章；无"待定"。明确不做的 3 项（G-10 硬件断点、G-11 PCIe、§3.6 其余项）单独列在 §3.6 并给理由，不进池 |
| **4. 断链成本统计** | `rg` 统计 | 见 §8.3：≈2 400 处引用，27 篇编号变化，热点 8 篇 |

### 9.2 自检门

| 门 | 检查内容 | 结果 | 证据 |
|----|---------|------|------|
| **G1** | C 真序是否逐条可核对（随机抽十条） | **通过** | 抽 S04(`pg_utils.c:162`)、S06(`pg_utils.c:204`)、S09(`protect.c:321`)、S12(`main.c:472`)、S15(`proc.c:119`)、S16d(`protect.c:388`)、S19(`system.c:168`)、S20(`main.c:301`)、S22(`main.c:38`)、S23(`proc.c:299`)——十条锚点均由 grep 命中（见 §0.4 命令输出第 2/6 项与 §3.1） |
| **G2** | 知识点池完整性：每个 C 文件、每个非 C 制品都有归属或"明确排除 + 理由" | **通过（有 2 项待 B 相复核）** | C 侧：`system/` 38 个 .c 全部在 §4.3 分组或 §3.6 排除项里；非 C 侧：`link.ld`→03、`boot-shim`→02/32、`xtask`→31、`qemu-tests`→31、`Cargo.toml`→31/32。**待复核**：`arch/i386/oxpcie.c` 与 `arch/earm/bsp/ti/*` 归 §3.6 明确不做，B 相应确认无遗漏知识点 |
| **G3** | 新目录前向引用为零 | **通过** | 见 §9.1 检查 1 |
| **G4** | 依赖关系图无环；有环是否给出拆解方案 | **通过** | 11↔13 已在 D-3 拆解 |
| **G5** | 覆盖率 100%；新增条目是否都有证据锚点；明确删除项单独列出 | **通过** | 12 条新增（K-019/022/120/125/130/135/140/151/152 + G-12/G-16 并入项）全部有 C 源码或非 C 制品路径锚点；明确删除项见 §3.6 |
| **G6** | 每处拆分/合并是否写清存量知识点去向；每处新建是否写清新增知识点来源（抽查十处） | **通过** | 抽查：C-08(05 拆)→K-085 去向 13 §2 ✅；C-21(16 拆)→K-076 去向 09 §4+11 ✅；C-14(10+11 合并)→K-069/070/072 去向 10 ✅；C-17(13+33)→K-094 去向 14 §2 ✅；C-23(18+24)→K-106 去向 18 §2 ✅；C-34(29+32)→K-143 去向 26 §3 ✅；C-02(新建 01)→来源 99+00 §1.4+06 §1.3 ✅；C-39(新建 31)→来源 `os/xtask/`+`qemu-tests/` ✅；C-40(新建 32)→来源 01 附录 A+18-trap-bridge-design+13 ABI ✅；C-38(shutdown)→来源 `main.c:351/368` ✅ |
| **G7** | 每篇契约七要素齐全 | **通过** | 33 篇全部含：一句话定位 / 讲什么 / 不讲什么 / 前置 / 后置 / 事实底线 / 知识点清单 + 验收标准 |
| **G8** | 锚点迁移表是否覆盖所有变化文档的每一节；引用迁移表是否覆盖文档与代码注释 | **通过** | §8.1 覆盖 33 篇中 27 篇变化文档的节级条目（含 4 篇归档文档）；§8.2 覆盖 5 处代码注释引用 + 6 类文档间引用 |
| **G9** | 事实断言是否都有锚点；推测项是否已标注 | **通过（有 2 处标注为推测）** | ① §2.I 统计里"`06-todo.md` 记录 06 重构后为 2028 行"来自 todo 文档自述，正文实测 1375 行——**推测**：06 在 2026-09 又有一次瘦身，B 相应以 `wc -l` 实测为准。② §8.3 的"≈2 400 处"是对 35 篇命中数求和的估算（含单个文件内多次命中），**推测**，B 相批量替换前应以 `rg -o \| wc -l` 精确统计 |

### 9.3 结论

**结论：完成（可进入 B 相）。**

本蓝图满足四条硬标准与九个自检门：

- 无前向引用（唯一潜在环已在序差表 D-3 登记并给出双向回指补偿）；
- 执行与因果序优先（启动链严格按 C 真序 S01–S23；运行期按"三入口汇聚 + 并行体分组"组织；
  8 处序差全部记录事实锚点、选择理由与补偿位置）；
- 首次出现即完整（横切概念集中在 `01-kernel-foundations`，后续 31 篇只引用）；
- 单篇单语义（33 篇各自承载一个可独立阅读的语义单元；05/06 两篇的百科化问题通过拆分 + 下放解决）。

### 9.4 待用户裁决的问题

| # | 问题 | 备选 | 我的倾向 |
|---|------|------|---------|
| Q-1 | `11-scheduling-primitives` 的调度原语（enqueue/pick_proc）并入 `10-switch-to-user`，还是保留独立编号？ | A：并入 10（本方案）；B：保留为 `11-scheduling-primitives`，位置不动 | **A**。理由：调度原语的唯一调用者是调度循环，先讲循环再讲队列是因果倒置；且旧 11 的 716 行里有大量与 10 重叠的"队列一致性"论述 |
| Q-2 | `07-paging_init_gpt.md`（358 行 GPT 分析记录）是归档还是升为正式篇？ | A：归档（本方案）；B：升为正式篇 | **A**。理由：它的形态是对话式分析记录，不是教学文档，且主题（分页初始化）已被 03/08 覆盖 |
| Q-3 | 关闭与重启是独立成篇（约 200 行）还是并入 `25-kernel-utility-and-terminal`？ | A：并入 25（本方案）；B：独立为 `30-shutdown-and-reboot`，watchdog 顺延 | **A**。理由："怎么说话 / 怎么死 / 怎么停"是同一个语义单元（内核终态）；独立成篇会语义单薄 |
| Q-4 | `99-global-concepts` 的常量速查（RTS 位表、特权位、errno）是放进 `01-kernel-foundations` 正文还是作为附录？ | A：放 01 正文 §3–§5；B：作为 01 的附录 | **A**。理由：这些常量本身就是横切概念的一部分（"RTS 有哪几位"是概念不是速查）；但 B 相若发现 01 超过 800 行，可把纯数值表下沉为附录 |
| Q-5 | 三篇"参考材料"（`06-todo.md` / `todo.md` / `checklist.md`）在新建目录里要不要保留？ | A：原样保留不动（本方案）；B：移入 `archive/` 子目录 | **A**。理由：它们是流程记录，不是教学文档；但 `06-todo.md` 顶部应加一行"v3 后 06 的章节号已变，见 07"的注记 |

---

*本文件是 R 相（重建蓝图）产物，未修改任何正文。B 相按 §5 的契约逐篇取料重写，
按 §8 的迁移表批量迁移引用。*






