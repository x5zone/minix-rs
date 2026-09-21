# 01-stage-kernel 文档重建蓝图（deepseek）

> 本文件是 R 相（重建蓝图）产物。它只定义新目录、每篇契约与迁移路径，不修改任何正文。
> 阅读本文件的人应当能够直接执行 B 相重建，不需要再读旧文档做取舍。

## 0. 元数据

| 项目 | 值 |
|------|-----|
| 执行者 | deepseek |
| 日期 | 2026-09-19 |
| 目标目录 | `notes/rewrite/fork-syscall-rewrite/01-stage-kernel/` |
| 仓库根目录 | `/home/xzhao/github/minix-rs` |
| 当前提交号 | `e5b9cd70061bba3ee512f1b6ca44498090f0b0eb`（2026-09-19） |
| 交付物 | 本文件（唯一落盘产物） |
| 本轮修订 | 2026-09-19（补做轮）：补记蓝图交付后新增的 `33-syscall-caller-api.md`——纳入 §0.1 文档清单（34 篇→35 篇）、§2.16 新增知识点 K-506、§5 契约 15 的知识点清单、§8.1 锚点迁移表与旧→新映射表各一行；§9.3 结论的篇数与知识点数同步订正（505→506） |

### 0.1 审查范围

**算文档（进重建范围）**：`01-stage-kernel/` 下 35 篇编号文档。

```text
00-kernel-overview.md, 01-boot-shim-bootstrap.md, 02-higher-half-kernel.md,
03-kmain-cstart.md, 04-platform-discovery.md, 05-clock-interrupt-init.md,
06-proc-init-boot-proc.md, 07-cross-space-init.md, 08-system-init-boot-finish.md,
09-vm-boot-protocol.md, 10-switch-to-user.md, 11-scheduling-primitives.md,
12-ipc-core.md, 13-syscall-dispatch.md, 14-exception-interrupt.md,
15-clock-timer.md, 16-smp.md, 17-syscall-process.md, 18-syscall-copy.md,
19-syscall-signal.md, 20-syscall-device.md, 21-syscall-clock.md, 22-privilege.md,
23-ipc-filter.md, 24-cross-space-runtime.md, 25-misc-unported.md,
26-watchdog.md, 27-kernel-utility.md, 28-usermapped-data.md, 29-kernel-debug.md,
30-kernel-profile.md, 31-fpu-context-switching.md, 32-stack-tracing.md,
33-syscall-caller-api.md, 99-global-concepts.md
```

> `33-syscall-caller-api.md`（222 行）创建于 2026-09-19 16:39，晚于本蓝图（05:09），属蓝图交付后新增；本轮补记其归属（新 15-syscall-dispatch）与知识点 K-506，详见 §2.16 与 §8.1。

**算参考材料（B 相归档，不进入新编号）**：`06-todo.md`（06 重构需求与执行记录）、`checklist.md`（早期覆盖度与 P0/P1 账本）、`endpoint_todo.md`（endpoint 重设计讨论稿）、`panic-in-drop.md`（KProcess Drop 纪律设计备忘）、`smp_gpt.md`/`smp_gpt_v2.md`/`smp_gpt_v3.md`（三轮外部评审输入）、`smp_todo.md`（SMP 设计+施工+完成记录）、`todo.md`（stage TODO 权威清单）、`07-paging_init_gpt.md`（页表设计冻结评审备忘）、`18-trap-bridge-design.md`（E1 trap 桥设计稿）。

**范围外**：`00-master-plan/`（旧主线规划，按自身 README 声明 defer）、`02-stage-vm/` 及更后 stage 的文档、`.review/` 与 `.design/` 与 `tmp_design_and_todo/`（规范禁止引用）。

### 0.2 读取清单

**文档**：上列 35 篇编号文档全文（含头部声明与边界声明）；9 份参考材料全文。

**C 源码（ground truth，路径相对 `minix3/minix/`）**：`kernel/` 顶层 13 个 `.c`（main.c、proc.c、system.c、clock.c、smp.c、interrupt.c、debug.c、profile.c、watchdog.c、utility.c、table.c、usermapped_data.c、cpulocals.c）+ 20 个头文件；`kernel/system/` 38 个 `do_*.c`；`kernel/arch/i386/` 21 个 `.c`（pre_init.c、pg_utils.c、protect.c、memory.c、arch_system.c、arch_clock.c、exception.c、i8259.c、apic.c、acpi.c、arch_smp.c、arch_do_vmctl.c、arch_reset.c、do_*.c 等）+ 汇编与链接脚本（head.S、mpx.S、klib.S、trampoline.S、apic_asm.S、debugreg.S、io_*.S、usermapped_glo_ipc.S、kernel.lds）+ 构建文件（Makefile、arch/i386/Makefile.inc）。头文件常量源：`include/minix/{com.h,endpoint.h,param.h,ipcconst.h,ipc.h,type.h,sys_config.h,config.h}`。

**非 C 制品**：`minix3/etc/boot.cfg.default`、`minix3/sys/arch/i386/include/multiboot.h`、`minix3/minix/include/arch/i386/include/{stackframe.h,fpu.h}`；Rust 侧 `os/boot-shim/`、`os/libs/minix-boot/`、`os/libs/minix-platform/`、`os/libs/minix-types/`、`os/libs/minix-sys/`、`os/arch/`、`os/kernel/`、`os/plat/`、`os/xtask/`、`os/qemu-tests/`、`os/kernel/tests/boot_integration.rs`、`os/.cargo/config.toml`。

**边界材料**：`00-master-plan/README.md`（新主线阶段划分与启动因果链）、`edge_todo.md`（跨阶段条目）、`01-stage-kernel/todo.md`（stage 权威清单）、`smp_todo.md`（SMP 施工与完成记录）。

### 0.3 使用的命令与关键输出（证据摘录）

```text
wc -l minix3/minix/kernel/*.c ...        → 顶层 13 文件；arch/i386 21 文件；system/ 38 文件
rg -n "vm_running"                       → 全树只有 main.c:47 一处写 0（无 =1）
rg -n "NR_BOOT_PROCS"                    → param.h:9 = NR_TASKS + LAST_SPECIAL_PROC_NR + 1
sed -n '142,166p' proc.h                 → RTS 16 位定义（0x01..0x10000，0x2000 空缺）
sed -n '44,65p' table.c                  → image[] 17 条目顺序 ASYNCM..INIT
sed -n '52p' system.c                    → call_vec[NR_SYS_CALLS]，58 槽
grep -rhoE '\b[0-9]{2}-[a-z0-9-]+\.md\b' *.md | wc -l
                                         → 文档间引用 760 处；20 个旧文件名已成悬空引用
grep -rn "(01|03)-stage-kernel/" os/     → Rust 代码引用 10 处（含 5 处旧路径 03-stage-kernel）
grep -rn "01-stage-kernel|03-stage-kernel" 其他 stage
                                         → 跨目录引用 389 处（含 master-plan/02-stage-vm/03-stage-rs）
```

C 真序抽样核对（15 条锚点逐条 `sed -n` 验证，全部命中）：`pre_init.c:217`、`pg_utils.c:162`、`protect.c:321`、`protect.c:388`、`main.c:115`、`main.c:38`、`proc.c:119`、`proc.c:299`、`system.c:168`、`clock.c:70`、`table.c:44`、`system.c:52`、`head.S:36`、`kernel.lds:4`。

### 0.4 代码真相源优先级与旧文档失真

> 本节回应用户的第三条要求：本任务的出现是为了修正已有文档的问题，旧文档可能含错误或未经人工审阅，**真相源是 `minix3/` C 源码与 `os/` Rust 实现**，不是旧文档。

**优先级链（B 相取料与断言的一级依据，从高到低）**：

| 级别 | 来源 | 用途 | 备注 |
|------|------|------|------|
| 1 | `minix3/` C 源码 | 行为底线：对外行为、协议、生命周期、错误码 | 与旧文档冲突时以此为准 |
| 2 | `os/` Rust 实现 | 机制是否存在、如何表达 | 不是顺序权威（顺序权威是 C） |
| 3 | 非 C 制品（链接脚本、汇编、构建文件、线格式头文件） | 协议与工程事实 | 如 `ipc.h`、`kernel.lds`、`Makefile.inc` |
| 4 | 旧文档（本目录 00-32/99 与参考材料） | 只作知识清单与线索 | **不作断言依据**；可能含错误、过期状态与失真锚点 |

**三条硬规则（写入 §5 契约前言，B 相强制执行）**：

- **R-GT1**：写任何一条存量知识点前先按锚点读代码；旧文档与代码冲突时以代码为准，并在正文用一句话记录差异，不展开旧说法。
- **R-GT2**：旧文档里 `（Lxxx，工具生成）` 形式的行号一概不信；一律按符号名（函数/结构体/文件）定位。
- **R-GT3**：代码与制品都无法确认的旧文档断言，降级为"文档主张"或直接删除（并入 §3.5 删除表）。

**本轮已确认的旧文档失真清单（样例，不完整；B 相逐条复核）**：

| 旧位置 | 旧说法 | 代码真相 | 证据 |
|--------|--------|---------|------|
| 06 §1.4.1 | `NR_BOOT_PROCS = NR_TASKS + NR_BOOT_MODULES` | `= NR_TASKS + LAST_SPECIAL_PROC_NR + 1 = 17` | `minix3/minix/include/minix/param.h:9` |
| 08 §1.2/§4.6、06 §4.8 | Step 8.5 用 `mem::forget(bkl_lock())` 取锁 | 现为 `debug_assert!(bkl_is_locked)`，重复取锁已删（S-5） | `os/kernel/src/lib.rs`（bsp_finish_booting Step 8.5） |
| 09 §1.3 | Kernel DM 基址 `0xFFFF_8000_0000_0000` | 三架构均为 `0xFFFF_8080_0000_0000` | `os/arch/src/arch/direct_map.rs` |
| 09 §1.3 与 07 §1.3.2 | VM DM 由内核 `arch_boot_proc` 建、Kernel DM 由 VM `map_kernel` 建 | 双窗口由 `establish_boot_dm` 在 `arch_boot` 一次建立 | `os/kernel/src/dm_coverage.rs`、`os/kernel/src/lib.rs:291`（冲突未定论，B 相按代码定稿） |
| 08 §2.3 / §4.5 / 09 §6.1 | kinfo 的 `mmap_size`/`mem_high_phys`：no-op / 已知缺口 / DEFERRED 三种说法 | 以 `add_memmap` 实现为准 | `arch/i386/pg_utils.c:110-115`、`os/kernel/src/memmap.rs` |
| 02 §4.5 | 引用 `07-kmain-entry-protection.md` | 文件不存在 | 悬空引用，改指 03 |
| 32 §6 | 引用 `16-signal-handling.md` | 文件不存在 | 悬空引用，改指 25 |
| 99 §1.1 | 来源注 `tmp-08-endpoint.md` | 中间产物，规范禁止引用 | 删除；endpoint 论述迁 14 |
| 99 §1.2/§1.4 | `_ENDPOINT_MAX_GENERATION ≈65535`；"generation 0 用于硬编码内核任务" | 实际 65534；内核任务为负 endpoint | `include/minix/endpoint.h:50`、`com.h:47-52` |
| 05 §3.2/§5.1 | DEFAULT_HZ 锚点自相矛盾（`clock.rs:43` vs `:49`） | 权威定义在 `arch/clock.rs` | `os/arch/src/arch/clock.rs` |
| 14 §4.6 注 | dispatcher 无生产调用方 | `trap_dispatch.rs` 已是生产调用方 | `os/kernel/src/trap_dispatch.rs` |
| 31 §4.5 | "FpuTrap 无 dead code" | FpuTrap 当前无消费者（交付路径未接线） | `os/arch/src/arch/exception_dispatcher.rs`（B 相复核） |
| 27 §5.3/§6.3 | panic handler 不打印消息；`minix_shutdown` 仍 DEFERRED | 已经 `format_panic_report` 格式化；`minix_shutdown` 已存在 | `os/libs/minix-rt/src/lib.rs`、`os/kernel/src/lib.rs` |
| 28 §3.5 | KernelInfo 精简为 12 字段 | 实际 13 字段（含 `param_buf`） | `os/libs/minix-boot/src/kernel_info.rs` |
| 28 §4.1 | `CLOCK_UPTIME` 等镜像"已删除" | `globals.rs` 仍定义 | `os/kernel/src/globals.rs` |
| 16 §5.3 | x86 `init_ap` 仍是 `panic!` 占位 | 已实现 | `os/arch/src/x86_64/protection.rs` |
| 12 §4.1 | `IpcCall` 只有 6 变体 | 含 `KernInfo=6` | `os/kernel/src/ipc.rs`、`os/kernel/src/syscall.rs` |
| 17/18/19/20/21/22/23/25 多表 | DEFERRED 项 | 多数已实现（T-1/T-6/T-7/T-12/D-13/D-16/D-43/D-45/D-49/U-1/U-2 等） | 对应 Rust 文件 |
| 03 头部 | `main.c:kmain,403-481` | `kmain` 在 115，403 是 `cstart` | `minix3/minix/kernel/main.c` |
| 04 §2.1 | `acpi_init` 在 `arch_system.c:246-287` | 246-287 是 `arch_init`；`acpi_init` 在 `acpi.c:310` | 两文件实读 |
| 13 §1.3/§4.8 | `dispatch_ipc_entry` 行号 523/541 | 实际 651 | `os/kernel/src/syscall.rs` |
| 23 §5/Ch6 | L2 过滤链未覆盖、memreq 过滤未实现 | `chain_allowed`/`dequeue_filtered` 已实现 | `os/kernel/src/ipc_filter.rs` |
| 30/32 | 对 panic 回溯"未实现/已实现"两说 | 以 `stacktrace.rs` 接线为准 | `os/kernel/src/stacktrace.rs` |
| 各篇头部 | 测试数与文件行数快照（"24 个测试"、"641 行"等） | 系统性漂移 | 不作为验收依据（DEL-05） |

> 说明：上表只收本轮读取中证据充分、可复现的样例；不排除还有更多失真。B 相必须按 R-GT1~R-GT3 重新逐条核对，而不是相信旧文档。

---

## 1. C 真序

> 本节直接从 C 源码重建运行时真序，不转述旧文档。所有锚点相对 `minix3/minix/kernel/`。

### 1.1 阶段类型判定

本 stage 同时具备三种特征，按以下口径处理：

| 特征 | 部分 | 处理方式 |
|------|------|---------|
| 启动链型 | 上电到第一次 `switch_to_user()`（§1.2 A 段） | 以启动时序为骨架，逐条锚点 |
| 事件循环型（非服务） | 运行期 trap 生命周期（§1.2 B 段） | 以"一次进入内核的完整生命周期"为主线：进入 → 分派 → 处理 → 调度 → 返回 |
| 集合型 | 58 个系统调用槽（§1.2 C 段） | 先讲统一分派框架与线格式，再按族分组（进程/拷贝/信号/设备/时钟/信息），族内代表讲透、其余差异表收束 |

**不是服务事件循环型**：内核没有"接收请求→处理→回复"的主循环。它的运行期由三类入口驱动：系统调用（trap）、中断、异常；所有路径的出口是唯一的 `switch_to_user()`（`proc.c:299-474`）。

### 1.2 真序表

#### A 段：启动链（上电 → 第一次调度）

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| A1 | GRUB 按 multiboot 协议加载内核与 boot 模块，跳 `MINIX` | `head.S:36-39`（入口）、`head.S:43-66`（multiboot header） | 内核前 8KB 的 multiboot 名片；magic 0x1BADB002 |
| A2 | 设临时栈、压 magic 与 mbi 指针、调 `pre_init` | `head.S:68-77` | 低地址恒等执行 |
| A3 | `pre_init` 校验魔数、调 `get_parameters` | `pre_init.c:217`、`:94` | 拷贝 mbi（:108）、解析命令行（:124-146）、拷贝模块列表（:170-171）、构建 memmap（:173-188） |
| A4 | 把内核自身当模块登记、切除内核与模块占用区 | `pre_init.c:194-199`、`:201-214`；`pg_utils.c:32`（cut_memmap）、`:86`（add_memmap） | 内存地图记账：模块内核占用从可用区扣除，可能拆分区间 |
| A5 | 建恒等映射（1024 个 4MB 页）、映射内核到高地址、装 CR3、开分页 | `pg_utils.c:254`（pg_clear）、`:162`（pg_identity）、`:186`（pg_mapkernel）、`:247`（pg_load）、`:204`（vm_enable_paging）；调用点 `pre_init.c:230-234` | 静止页目录 `pagedir[1024]`；`_kern_vir_base=0xF0400000`，`_kern_phys_base=0x400000` |
| A6 | 切高地址栈、把 `kinfo` 指针压栈、调 `kmain` | `head.S:84-87`；`main.c:115` | `kmain` 入口无返回 |
| A7 | `kmain` 自检 BSS、拷贝 kinfo 与 kmess、登记启动镜像、打开分配窗口 | `main.c:124-145` | `kernel_may_alloc=1`（:142）；`image[]` 拷入 `kinfo.boot_procs`（:144-145） |
| A8 | `prot_init`：清 GDT/IDT、建 TSS、填 GDT 段、载选择子、装 IDT 门、重建启动页表 | `protect.c:321-367`；`tss_init:154`；`idt_init:260`；`gate_table_pic:107-125`、`gate_table_exceptions:127-152`；`prot_load_selectors:298`；页表重建 `:360-363` | 保护结构就绪；第二次 `pg_clear/identity/mapkernel/load` 覆盖同一静止页目录 |
| A9 | `init_clock`：清零时钟与负载结构、从 env 取 hz | `main.c:418`；`clock.c:47-64` | 只初始化软件状态，不编程硬件 |
| A10 | `intr_init(0)`：编程 8259A、全部 IRQ 屏蔽 | `main.c:472`；`i8259.c:28-80` | 中断控制器就绪但未开中断 |
| A11 | `arch_init`：内核栈、串口、ACPI、单核 APIC、内存裁剪 | `main.c:474`；`arch_system.c:246-281` | x86 路径 |
| A12 | 取 BKL | `main.c:149` | 启动早期代码无锁运行，此后内核态串行化 |
| A13 | `proc_init`：清 261 个进程槽、每槽架构复位、特权表置空闲、为每 CPU 建 IDLE | `main.c:157`；`proc.c:119-159` | 进程表初始为全空闲 |
| A14 | IPC 过滤池初始化、boot 模块数量校验 | `main.c:158`；`main.c:160-162` | 模块数必须等于 NR_BOOT_MODULES=12，否则 panic |
| A15 | 遍历 `image[]`：定位模块、分配特权、判定可调度性；仅 VM 执行 `arch_boot_proc` 加载 ELF | `main.c:165-272`；`system.c:274`（get_priv）；`protect.c:388-455`（arch_boot_proc）；`memory.c:722`（arch_proc_init） | VM 的 bootstrap 页表由内核手工建立；VM ELF 加载后释放其内存 blob |
| A16 | 非 VM 用户进程挂 `RTS_VMINHIBIT|RTS_BOOTINHIBIT`；所有 boot 进程挂 `RTS_PROC_STOP`；清 `RTS_SLOT_FREE` | `main.c:264-270` | `RTS_SET(PROC_STOP)` 因 SLOT_FREE 未清而不出队；之后 `RTS_UNSET` 才真正入队 |
| A17 | `arch_post_init` 记录 `ptproc=VM` 与 bootstrap 页目录；登记 IPC 调用名；`memory_init` 领 2 个空闲 PDE；`system_init` 注册 call_vec；回收 bootstrap 内存 | `main.c:275-301`；`protect.c:370-377`；`memory.c:707-717`；`system.c:168-270` | 阶段 C/D/E 的收口 |
| A18 | `smp_init`：成功路径进 AP 启动并调 `bsp_finish_booting`；失败/单核走 fallback | `main.c:304-316`；`arch_smp.c:289-345`、`:100-170` | `smp_init` 正常不返回（`arch_smp.c:168` 内调 `bsp_finish_booting`） |
| A19 | `bsp_finish_booting`：CPU 身份、`vm_running=0`、计费指针、banner、解除 PROC_STOP（范围 `i < NR_BOOT_PROCS-NR_TASKS`，即 12 个）、TSC 基线、时钟启动、FPU、关分配窗口 | `main.c:38-107` | 最后一件：`kernel_may_alloc=0`（:105），VM 正式接管内存 |
| A20 | 第一次 `switch_to_user()`，选 VM 运行 | `main.c:107`；`proc.c:299` | 启动链结束，进入 B 段 |
| A21 | AP 启动：trampoline → `startup_ap_32` → `smp_ap_boot` → `ap_finish_booting` → 各自 `switch_to_user` | `trampoline.S:8-43`；`mpx.S:600-621`；`arch_smp.c:214-251` | AP 的 LAPIC 定时器与 idle 进程在此建立 |

#### B 段：运行期 trap 生命周期（一次进入内核的完整路径）

| 步 | 动作 | C 锚点 | 说明 |
|----|------|--------|------|
| B1 | 用户执行 `int 33`（IPC）/`int 32` 或 `syscall`（系统调用） | `mpx.S:202-337`；`protect.c:147-150`（门表）；`archconst.h:KTS_*` | int 33 可阻塞；syscall 快速入口只保存瘦帧 |
| B2 | 入口保存上下文、`context_stop`（获取 BKL、记账） | `mpx.S:273-300`（IPC）、`:306-337`（kernel call）；`arch_clock.c:208-349` | 用户态 → 内核态的边界 |
| B3 | `do_ipc` 或 `kernel_call` 分派 | `proc.c:599-698`（do_ipc）、`system.c:136-163`（kernel_call）、`system.c:95-127`（kernel_call_dispatch） | IPC 与系统调用入口分离，语义正交 |
| B4a | IPC：权限检查 → 六原语之一 → 阻塞或直接投递 | `proc.c:479-597`（do_sync_ipc）、`:870-962`（mini_send）、`:967-1117`（mini_receive）、`:1122-1167`（mini_notify）、`:1200-1326`（senda） | 消息拷贝到内核缓冲，接收方延迟投递 |
| B4b | 系统调用：权限掩码 → `call_vec[call_nr]` → handler；可能 `VMSUSPEND` | `system.c:103-123`、`:59-93`（finish）；`vm.h:6`（VMSUSPEND=-996） | handler 不阻塞；挂起经由 VM 恢复 |
| B5 | 中断/异常入口 | `mpx.S:64-190`（PIC IRQ）、`apic_asm.S`（APIC）、`mpx.S:340-585`（异常 stub）、`exception.c:180-283`（exception_handler） | 异常用户态转信号、内核态 panic；缺页转发 VM |
| B6 | 处理完毕进入调度循环：可运行性检查 → `pick_proc` → 杂项标志（恢复内核调用、投递延迟消息、延迟系统调用）→ 量子检查 → 终局分派 | `proc.c:309-474`；`:176-229`（idle） | `delivermsg` 在 `proc.c:363-367`；量子检查在 `:421-422` |
| B7 | 终局：`context_stop` 记账、释放 BKL、FPU 所有权、`restore_user_context` 返回用户态 | `proc.c:437-474`；`arch_system.c:495-610` | 三条入口路径共用同一出口 |
| B8 | 时钟中断：每 tick 记账、递减虚拟/性能定时器、扫描同步闹钟、更新负载 | `clock.c:70-173`；`arch_clock.c:72-74`（i386 空实现） | quantum 递减不在这里，在 `context_stop`（`arch_clock.c:326-330`） |
| B9 | VM 启动后续协商：切 CR3、物理映射声明、缺页请求应答、解除 VMINHIBIT/BOOTINHIBIT | `system/do_vmctl.c:17-173`；`arch_do_vmctl.c:19-67` | `vm_running` 在 C 中永远为 0（`main.c:47` 只写 0，全树无写 1），Rust 在 SetAddrSpace 成功时修正 |

#### C 段：系统调用集合（58 槽，i386 映射 46 个）

| 族 | 调用（槽号 = SYS 号 − 0x600） | 主要 C 文件 | 下界/上界约束 |
|----|------|------|------|
| 进程 | FORK 0、EXEC 1、CLEAR 2、SCHEDULE 3、PRIVCTL 4、TRACE 5、RUNCTL 46、EXIT 53、SCHEDCTL 54、STATECTL 55 | `system/do_fork.c` … `do_statectl.c` | 系统进程/SYS_PROC 权限 |
| 信号 | KILL 6、GETKSIG 7、ENDKSIG 8、SIGSEND 9、SIGRETURN 10 | `system/do_kill.c` 等 | 信号管理器协议 |
| 内存/拷贝 | MEMSET 13、UMAP 14、VIRCOPY 15、PHYSCOPY 16、UMAP_REMOTE 17、VUMAP 18、SAFECOPYFROM 31、SAFECOPYTO 32、VSAFECOPY 33、SETGRANT 34、SAFEMEMSET 56 | `system/do_copy.c`、`do_safecopy.c`、`do_umap*.c`、`do_vumap.c`、`do_memset.c`、`do_safememset.c`、`do_setgrant.c` | 信任调用者/grant 授权两层 |
| 设备 | IRQCTL 19、DEVIO 21、SDEVIO 22、VDEVIO 23、IOPENABLE 28、READBIOS 35 | `system/do_irqctl.c`、`do_devio.c`、`do_vdevio.c`、`arch/i386/do_*.c` | x86 特有部分在非 x86 上返回 BadCall |
| 时钟 | SETALARM 24、TIMES 25、VTIMER 45、STIME 39、SETTIME 40 | `system/do_setalarm.c`、`do_times.c`、`do_vtimer.c`、`do_stime.c`、`do_settime.c` | SYS_PROC/信任进程 |
| 信息/杂项 | GETINFO 26、ABORT 27、SPROF 36、VMCTL 43、DIAGCTL 44、GETMCONTEXT 50、SETMCONTEXT 51、UPDATE 52 | `system/do_getinfo.c`、`do_abort.c`、`do_sprofile.c`、`do_vmctl.c`、`do_diagctl.c`、`do_mcontext.c`、`do_update.c` | GETINFO 19 子请求；UPDATE 仅 RS |
| 空缺 | 11、12、20、29、30、37、38、41、42、47、48、49（i386 另加 57 PADCONF 为 ARM 专用） | `system.c:193-268` 未 `map()` | `call_vec[call_nr]==NULL` → EBADREQUEST |

**分组依据**：按"谁在什么时候发起"（`minix3/minix/include/minix/com.h:205-270` 的编号顺序 + `system.c:193-268` 的注册分组）而非代码目录顺序。并行族的组织规则见 §4.3。

> 序差说明：真序是"启动链 + 循环"，而新目录的教学顺序把运行期骨架（入口/并发/调度/IPC/分派/调度循环/异常）排在启动链之后、系统调用族之前。凡是教学序与运行时序不一致的地方，逐条记录在 §4.4《序差表》。

---

## 2. 知识点全集

> 编号 `K-nnn` 为 stage 内唯一编号，按"去向篇章"分组连续编号。类型取值：概念 / 机制 / 数据结构 / 接口 / 约束 / 演进 / 工具 / 测试 / 决策。
> 来源类型两种：**存量**（现有文档承载，要回答"搬到哪里去"）；**新增**（现有文档未覆盖，由 C 源码、非 C 制品或操作系统理论承载，要回答"证据锚点在哪里"）。
> "现有位置"中的 `§` 指旧文档小节，**加粗**为该知识点的主讲述点；锚点路径：C 相对 `minix3/minix/`，其余相对仓库根。
> 每行必须落在"去向"分组里；被明确删除的条目单列在 §3.5《明确删除表》。
>
> **归属与顺序一句话**：分组标题就是归属（唯一 owner），分组顺序就是新文档顺序（即推荐阅读顺序），组内行序就是该篇的讲授顺序；归属规则见 §2.0，跨篇争议裁决见 §2.38。

### 2.0 归属规则与排序约定

**归属规则（每条知识点恰好一个 owner 篇章）**：

- **R1 单一 owner**：分组即归属，不设第二归属；其他篇章只允许"引用锚点"，不允许重讲同一知识点。
- **R2 判据优先级**：① 首次完整语义出现的位置；② 定义者优先于使用者（定义、状态载体、不变量归定义者；调用点留在调用者）；③ 生命周期阶段一致（启动链概念归对应启动阶段，运行期机制归运行骨架）；④ 仍冲突时，归给"不读它就无法读懂其余篇章"的那一篇。
- **R3 跨切概念**允许按"定义 / 操作 / 使用点"拆开归篇。示例：RTS 位定义归 07，队列联动归 13，量子动作归 13，量子检查调用点归 16，quantum 归属决策归 19。
- **R4 集合型主题**：分派框架归 15、权限定义归 21、过滤归 22、各族接口与差异归 23-28、线格式归 36。
- **R5 重名冲突**：同一机制若在两处出现，只保留一处完整讲述，另一处改写为不同的语义侧面；本轮修正 5 组（RTS 宏/联动、priv 布局/语义、generation 语义/实现、cause_sig 触发/机制、SYS 号归一化/权威表），裁决记录见 §2.38。

**排序约定**：

- §2 的分组顺序 = 新文档编号顺序（2.1=00 … 2.37=36），即全 stage 的推荐阅读顺序。
- 每个分组内的行序 = 该篇的推荐讲授顺序；§5 契约的知识点清单沿用同一顺序。
- B 相写作时不得在篇内随意重排；确需调整需更新契约并记录理由。
- 篇与篇之间的顺序由 §4.2 与 §4.6 定义；备选序与否决理由见 §4.6。

### 2.1 去向 = 00-kernel-overview

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-001 | 内核的职责边界 | 概念 | 存量 | **00 §1.2** | `kernel/table.c`、`main.c:115` | 能说出内核做什么、不做什么 |
| K-002 | 三维运行态实体（IPC 身份 / 代码+状态 / 执行上下文） | 概念 | 存量 | **00 §1.4.1**、06 §1.1.4 | `table.c:44-51`、`com.h:47-56` | 能区分 user/server 与 kernel task |
| K-003 | kernel task 无独立执行流（RTS_PROC_STOP 恒置 + 无恢复点） | 机制 | 存量 | **00 §1.4.1**、06 §1.1.4 | `main.c:64-66,269`、`protect.c:396-402`、`system.c:136-163`、`clock.c:140-173` | 不再把 CLOCK/SYSTEM 当线程 |
| K-004 | 内核无主循环：三类入口一出口 | 概念 | 存量 | **00 §1.4/§4.1**、10 §1.2、14 §1.2 | `mpx.S:273-337`、`proc.c:299-474` | 建立事件驱动心智模型 |
| K-005 | 内核与 VM 的策略/机制分工 | 概念 | 存量 | **00 §1.3/§5.2**、09 全文 | `system/do_vmctl.c`、`arch_do_vmctl.c` | 理解页表跨两个主体管理 |
| K-006 | Rust 表达约束（BKL / no_std / trait 硬件抽象 / 错误码对齐） | 约束 | 存量 | **00 §1.5** | `os/kernel/src/lib.rs`、`os/arch/src/arch/*` | 写内核代码前知道硬规则 |
| K-007 | execution context 与 execution flow 的区分 | 概念 | 存量 | **00 §1.4.1** | `proc.h:23-24`、`proc.c:176-229` | 能解释 IDLE 为什么特殊 |
| K-008 | 内核是"最先运行、最后停止"的程序 | 概念 | 存量 | **00 §1.1** | `main.c:115`、`utility.c:22` | 定位内核在系统中的位置 |

### 2.2 去向 = 01-boot-shim-and-firmware

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-009 | Multiboot 协议（header 魔数 / 信息魔数 / 模块列表） | 接口 | 存量 | **01 §2.0.1** | `arch/i386/head.S:43-66`、`sys/arch/i386/include/multiboot.h:40-259` | 看懂 GRUB 与内核的契约 |
| K-010 | head.S 第一跳板（临时栈、压参、call pre_init） | 机制 | 存量 | **01 §2.0.2**、02 §2.2 | `head.S:68-77` | 知道内核第一条指令在哪 |
| K-011 | boot.cfg 与 load_mods / multiboot 指令 | 工具 | 存量 | **01 §2.0.3** | `minix3/etc/boot.cfg.default:1-8`、`man/man5/boot.cfg.5` | 能配置一次启动 |
| K-012 | get_parameters：从 mbi 收集启动参数 | 机制 | 存量 | **01 §2.3.2** | `pre_init.c:94-171` | 知道 kinfo 从哪来 |
| K-013 | memmap 记账（add_memmap / cut_memmap / alloc_lowest / 4GB 截断） | 机制 | 存量 | **01 §2.3.3-2.3.4** | `pg_utils.c:32,65,86-121`、`pre_init.c:173-214` | 理解可用物理内存怎么算出来 |
| K-014 | 内核与模块的重叠检查（overlaps） | 机制 | 存量 | **01 §3.5** | `pre_init.c:77-92,194-214` | 知道加载器不保证什么 |
| K-015 | KernelInfo 契约（boot-shim → kernel 唯一交接面） | 接口 | 存量 | **01 §3.5/§4.1** | `os/libs/minix-boot/src/kernel_info.rs:13-130` | 改 boot 协议时知道动哪里 |
| K-016 | BootShim trait 与 BootPrepareResult | 接口 | 存量 | **01 §4.4** | `os/libs/minix-boot/src/boot_shim.rs:20-58` | 看懂两种固件路径的共同抽象 |
| K-017 | UEFI 替代 Multiboot（跨架构统一） | 演进 | 存量 | **01 §3.1** | `os/boot-shim/src/uefi_helpers.rs`、`os/boot-shim/src/main.rs:34-60` | 理解引导代际迁移 |
| K-018 | OpenSBI + BootFileTable 协议（riscv64） | 接口 | 存量 | **01 §4.5.2** | `os/boot-shim/src/opensbi_helpers.rs:193-434` | 看懂 riscv64 无 UEFI 的替代 |
| K-019 | pt_alloc 页表页分配器与 BootAlloc bump 分配器 | 机制 | 存量 | **01 §4.3.1** | `os/arch/src/arch/pt_alloc.rs`、`os/kernel/src/boot_alloc.rs` | 理解 boot 期无堆如何分配页表页 |
| K-020 | 三层 crate 依赖方向（boot-shim → minix-boot/minix-arch → kernel） | 工具 | 存量 | **01 §3.8** | `os/*/Cargo.toml`、`os/README.md` | 知道改动不能反向依赖 |
| K-021 | bootstrap 区间语义（C 可回收 vs Rust no-op） | 决策 | 存量 | **01 §3.5.1** | `pre_init.c:230-234`、`os/boot-shim/src/*_helpers.rs` | 理解 `add_memmap` 守卫 |
| K-022 | 模块数校验 NR_BOOT_MODULES=12 | 约束 | 新增 | 无 | `main.c:160-162`、`com.h:74` | 知道启动镜像与模块必须一一对应 |

### 2.3 去向 = 02-kernel-image-and-higher-half

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-023 | VMA/LMA 与 AT()（物理装载地址 vs 虚拟运行地址） | 接口 | 存量 | **02 §2.1** | `kernel.lds:4-6,29-34` | 理解代码"没搬但换窗口" |
| K-024 | unpaged 段存在原因（分页前代码必须恒等低地址） | 决策 | 存量 | **02 §2.1** | `kernel.lds:14-20`、`arch/i386/Makefile.inc:51-71` | 理解 `__k_unpaged_` 前缀 |
| K-025 | usermapped 段的镜像位置（语义见 34） | 数据结构 | 存量 | **02 §2.1**、28 §1.2 | `kernel.lds:24-28` | 知道段在镜像里的位置 |
| K-026 | 高半核中间态与切栈跳转成对性 | 约束 | 存量 | **02 §1.1/§2.3** | `head.S:78-87`、`earm/head.S:41-47` | 能解释为什么必须成对 |
| K-027 | 物理代码从未移动：两个映射窗口 | 概念 | 存量 | **02 §1.3** | `pg_utils.c:162,186` | 建立地址翻译直觉 |
| K-028 | HigherHalf trait 与三架构跳转指令 | 接口 | 存量 | **02 §3.4/§4.3** | `os/kernel/src/boot/higher_half.rs:30`、`os/kernel/src/arch/*/higher_half.rs` | 看懂 Rust 如何表达跳转 |
| K-029 | 三架构 link.ld 与初始栈 | 工具 | 存量 | **02 §4.1** | `os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld` | 能读懂镜像布局 |
| K-030 | 自研 minix-elf（零分配段迭代、错误变体） | 机制 | 存量 | **02 §4.2** | `os/libs/minix-elf`、`os/boot-shim/src/loader.rs:58-253` | 知道 ELF 加载有多小 |
| K-031 | arch_boot_impl Step 0-4（校验→恒等映射→内核映射→开分页→记根→建 DM） | 机制 | 存量 | **02 §4.4**、07 §1.3.2/§4.4 | `os/kernel/src/lib.rs:291`、`os/arch/src/arch/paging.rs` | 看懂 Rust 版启动入口 |
| K-032 | kern_virt ≠ kern_phys 守卫 | 约束 | 存量 | **02 §4.4、附录 A** | `os/kernel/src/lib.rs`（arch_boot_impl 校验） | 知道两个历史 bug 的教训 |
| K-033 | 独立 kernel ELF 构建与 code model 约束 | 演进 | 存量 | **02 §3.2** | `os/kernel/Cargo.toml`、`os/.cargo/config.toml:33-34` | 理解 kernel 为什么单独链接 |
| K-034 | riscv64 Sv39 L2 覆盖 Bug 案例 | 演进 | 存量 | **02 附录 A** | `os/kernel/src/arch/riscv64/link.ld`、`os/arch/src/riscv64/paging.rs` | 学一个真实地址空间 bug |
| K-035 | PROVIDE 符号只服务内核自用 | 决策 | 存量 | **02 §4.1** | `os/kernel/src/arch/*/link.ld` | 知道 boot-shim 不读它们 |
| K-036 | 零分配 ELF 解析的边界条件与错误分类 | 接口 | 存量 | **02 §4.2.4/§4.2.6** | `os/libs/minix-elf`（ElfError 8 变体） | 能诊断加载失败 |

### 2.4 去向 = 03-protection-init

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-037 | 保护 = 特权级 + 内存权限 + 异常处理 | 概念 | 存量 | **03 §1.1** | `protect.c:321-367` | 建立"保护"三要素 |
| K-038 | 三架构特权级对照 | 概念 | 存量 | **03 §1.2** | `protect.c`、`archconst.h`、RISC-V Priv Spec | 跨架构统一理解 |
| K-039 | CPU 三问（特权级 / 异常入口 / 内核栈） | 概念 | 存量 | **03 §1.4** | `protect.c:154-167,260-268` | 全 stage 复用的分析框架 |
| K-040 | 跨特权级统一流程与返回指令 | 机制 | 存量 | **03 §1.3** | `mpx.S:391-459` | 理解 trap 往返 |
| K-041 | 硬件切栈 vs 软件切栈 | 机制 | 存量 | **03 §1.3/§1.4c** | `protect.c:154`（TSS.sp0）、`earm/mpx.S` | 知道栈何时换 |
| K-042 | kmain 入口三件事（BSS 自检 / 拷 kinfo / 开分配窗口） | 机制 | 存量 | **03 §2.1** | `main.c:115-147` | 定位启动入口 |
| K-043 | cstart 四调用顺序（prot_init→init_clock→intr_init→arch_init） | 机制 | 存量 | **03 §2.2** | `main.c:403-481` | 知道初始化固定次序 |
| K-044 | prot_init 全流程（清表→TSS→GDT→选择子→IDT→重建页表） | 机制 | 存量 | **03 §2.3** | `protect.c:321-367` | 能复述保护结构建立 |
| K-045 | tss_init 两件事（GDT TSS 描述符 + ss0/sp0） | 机制 | 存量 | **03 §2.3.1** | `protect.c:154-167` | 理解内核栈入口 |
| K-046 | idt_init 与 gate table（异常门 + PIC 门） | 机制 | 存量 | **03 §2.3.2** | `protect.c:107-152,260-268` | 知道向量表内容 |
| K-047 | 为什么保留 GDT（TSS 必须挂 GDT；64 位 flat segment） | 决策 | 存量 | **03 §1.7** | `protect.c:341-348` | 不再疑惑 64 位为何还有 GDT |
| K-048 | 不复用固件保护结构 | 决策 | 存量 | **03 §1.6** | `protect.c:330-331` | 理解生命周期与语义原因 |
| K-049 | ProtectionArch trait | 接口 | 存量 | **03 §3.1/§4.1** | `os/arch/src/arch/protection.rs:105` | 看懂 Rust 保护抽象 |
| K-050 | TrapEntryArch trait | 接口 | 存量 | **03 §3.1/§4.2** | `os/arch/src/arch/trap_entry.rs:116` | 看懂向量表抽象 |
| K-051 | 类型状态替代 prot_init_done | 决策 | 存量 | **03 §3.2** | `os/kernel/src/lib.rs:827` | 学 Rust 表达顺序约束 |
| K-052 | init/load 顺序不变量（handler=0 窗口期受控） | 约束 | 存量 | **03 §4.3/§6** | `os/kernel/src/lib.rs:827-990` | 知道启动期关中断的理由 |
| K-053 | 内核栈威胁模型（四攻击向量） | 概念 | 存量 | **03 附录 A** | `mpx.S`、`klib.S:812`（switch_k_stack） | 理解 TSS/SP_EL1/sscratch 的必要性 |
| K-054 | EarlyConsole trait 与 boot 期输出 | 接口 | 存量 | **03 §4.3**、01 §6.1、05 §3.4/§4.14 | `os/plat/src/early_console.rs:14` | 知道 panic 前怎么打印 |

### 2.5 去向 = 04-platform-discovery

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-055 | 内核需要的四类硬件参数 | 概念 | 存量 | **04 §1.1** | `arch_system.c:246-281`、`acpi.c:310` | 知道"发现"要发现什么 |
| K-056 | 硬编码地址的三类问题 | 决策 | 存量 | **04 §1.1** | `acpi.c:204-228`、`earm/arch_system.c:98` | 理解为什么必须发现 |
| K-057 | CPU 三问 + 隐含第四问（时间） | 概念 | 存量 | **04 §1.2** | 与 03 §1.4 衔接 | 复用三问框架 |
| K-058 | C 端 ACPI 发现链（RSDP→MADT→LAPIC/IOAPIC/CPU 拓扑） | 机制 | 存量 | **04 §2.1** | `acpi.c:204-228,310` | 对照 Rust 解析 |
| K-059 | C 端 ARM 板级常量（GIC/PMU/660MHz） | 机制 | 存量 | **04 §2.2** | `earm/arch_system.c:98-132`、`earm/bsp/ti/omap_intr.c` | 理解硬编码的历史 |
| K-060 | PlatformDesc trait 与子描述符 trait | 接口 | 存量 | **04 §3.2/§3.3/§4.1** | `os/libs/minix-boot/src/platform.rs:247-322` | 看懂平台抽象 |
| K-061 | QemuVirtDesc 兜底策略 | 机制 | 存量 | **04 §3.6/§4.2.1** | `os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs` | 知道 fallback 何时触发 |
| K-062 | KernelInfo.platform_sources 多源有序列表 | 接口 | 存量 | **04 §3.7/§4.4** | `os/libs/minix-boot/src/kernel_info.rs:103` | 知道 ACPI/DTB 如何交接 |
| K-063 | PlatformContext 与 init_from_kinfo | 机制 | 存量 | **04 §4.3** | `os/libs/minix-platform/src/global.rs:219-273` | 看懂发现入口 |
| K-064 | DeviceTreeDesc / AcpiDesc 解析 | 机制 | 存量 | **04 §4.2.2/§4.2.3** | `os/libs/minix-platform/src/device_tree.rs`、`acpi.rs` | 看懂两类描述源 |
| K-065 | 硬件描述 → 硬件实例的构造模式（new(desc)） | 决策 | 存量 | **04 §3.4/§4.5** | `os/arch/src/*/clock.rs`、`os/plat/src/*/interrupt.rs` | 理解热路径零 downcast |
| K-066 | T2.5 时序：发现必须早于时钟/中断初始化 | 约束 | 存量 | **04 §4.6** | `os/kernel/src/lib.rs`（kmain Phase A.5/B） | 知道对 C 的有意偏离 |
| K-067 | 迁移判别规则（ISA 字面量保留 vs 协议发现） | 决策 | 存量 | **04 §4.7** | `os/plat/src/x86_64/*` | 决定新常量放哪里 |
| K-068 | AssumeSyncCell 与 SMP 后替换条件 | 数据结构 | 存量 | **04 §3.5** | `os/libs/minix-types/src/types/cell.rs` | 知道该原语的生命周期 |

### 2.6 去向 = 05-clock-init

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-069 | init_clock 只初始化软件状态 | 机制 | 存量 | **05 §2.1** | `clock.c:47-64` | 知道硬件时钟何时才启动 |
| K-070 | ClockState 结构（kclockinfo + kloadinfo + 定时器队列） | 数据结构 | 存量 | **05 §3.1/§4.1** | `os/kernel/src/clock.rs:971` | 理解时钟状态集中管理 |
| K-071 | ClockArch trait 与 new(desc) | 接口 | 存量 | **05 §3.1/§4.2** | `os/arch/src/arch/clock.rs:87` | 看懂时钟硬件抽象 |
| K-072 | DEFAULT_HZ=100 的选择（C 无唯一值） | 决策 | 存量 | **05 §3.2** | `os/arch/src/arch/clock.rs:43`、`clock.c:56-60` | 理解行为差异与标注 |
| K-073 | 三架构时钟源（PIT / ARM Generic Timer / CLINT） | 机制 | 存量 | **05 §4.3-§4.5** | `os/arch/src/*/clock.rs`、`arch_clock.c:131-194` | 知道 tick 从哪来 |
| K-074 | hz 的边界校验（2..50000）与默认值 | 约束 | 存量 | **05 §2.1** | `clock.c:50-60` | 复刻解析行为 |
| K-075 | 定时器编程与使能的分离（Step 6 唯一硬件触点） | 决策 | 存量 | **05 §3.7 D-59** | `clock.c:294-304`、`interrupt.c:29-69` | 知道顺序为何这样排 |
| K-076 | Send+Sync 语义辨析（引用可传递 ≠ 线程安全） | 概念 | 存量 | **05 §3.1** | `os/kernel/src/clock.rs` | 避免错误依赖 Rust 类型 |
| K-077 | ClockState 的 BSP/AP 区分 | 数据结构 | 存量 | **05 §4.1.1** | `os/kernel/src/clock.rs`（is_bsp/cpu_id） | 理解时钟全局唯一性 |

### 2.7 去向 = 06-interrupt-controller-init

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-078 | 同步异常 vs 异步中断 | 概念 | 存量 | **05 §1.0**、14 §1.1 | `exception.c`、`interrupt.c` | 建立两类意外事件分类 |
| K-079 | 中断控制器 = 转发器 + 优先级仲裁器 | 概念 | 存量 | **05 §1.0** | `i8259.c:28-80`、`apic.c` | 理解单 INT 引脚如何扩多设备 |
| K-080 | 8259A ICW 初始化序列 | 机制 | 存量 | **05 §2.2** | `i8259.c:28-80` | 看懂 PIC 初始化 |
| K-081 | LAPIC/IOAPIC 代际迁移 | 演进 | 存量 | **05 §2.2** | `apic.c:241-296,868-1002` | 知道 x86-64 为什么换 |
| K-082 | ARM OMAP INTC → GICv3 / RISC-V PLIC | 演进 | 存量 | **05 §2.3/§3.8** | `earm/bsp/ti/omap_intr.c:22-44`、`os/plat/src/{arm64,riscv64}/interrupt.rs` | 跨架构对照 |
| K-083 | InterruptController trait（claim→mask→handler→unmask→complete） | 接口 | 存量 | **05 §3.3/§4.6/§4.8** | `os/plat/src/interrupt.rs:142` | 看懂中断控制器抽象 |
| K-084 | 全局动作 vs per-CPU ack 的非对称性 | 演进 | 存量 | **05 §3.3 note** | `os/plat/src/interrupt.rs` | 知道未来拆分方向 |
| K-085 | ArchInit trait（杂项架构初始化的收留所与反模式） | 接口 | 存量 | **05 §3.5/§4.9** | `os/arch/src/arch/arch_init.rs:46` | 知道什么能放这里 |
| K-086 | arch_init 六件事（栈/TSS/串口/ACPI/APIC/内存裁剪） | 机制 | 存量 | **05 §2.4** | `arch_system.c:246-281` | 对照 C 与 Rust 分工 |
| K-087 | 中断控制器放 minix-plat 的理由（板级外设维度） | 决策 | 存量 | **05 §3.3** | `os/plat/src/interrupt.rs` | 理解 crate 边界 |

### 2.8 去向 = 07-proc-table-init

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-088 | 阶段 A-F 启动主线 | 概念 | 存量 | **06 §1.0**、03 §1.8、07 §1.0、08 §1.1 | 全 stage 导览 | 知道本 stage 的地图 |
| K-089 | 进程 = CPU 时间的断点续传 | 概念 | 存量 | **06 §1.1.1** | `proc.h:23-24` | 理解进程本质 |
| K-090 | 进程五组成（槽/寄存器/调度/IPC/VM 状态） | 概念 | 存量 | **06 §1.1.3** | `proc.h:22-140` | 结构化认识 proc |
| K-091 | CPU 要回答的四个问题（初值/入口/栈参数/地址空间） | 概念 | 存量 | **06 §1.2** | `arch_system.c:495-610`、`memory.c:722-731` | 分析新进程创建 |
| K-092 | 三件套：进程表 / 特权表 / RTS 位图 | 概念 | 存量 | **06 §1.3** | `table.c`、`priv.h:94`、`proc.h:142-166` | 抓住阶段 C 的核心 |
| K-093 | 身份即索引与 proc_addr | 数据结构 | 存量 | **06 §2.1.0** | `proc.h:265-279` | 理解 O(1) 进程定位 |
| K-094 | proc 字段 6 组与存储模型 | 数据结构 | 存量 | **06 §2.1.1-2.1.6** | `proc.h:22-140` | 能按组理解 261 个槽 |
| K-095 | RTS 16 位全集与不变量（p_rts_flags==0 ⟺ 可运行） | 数据结构 | 存量 | **06 §2.1.6**、11 §1.1、29、99 | `proc.h:142-166,168-231` | 调度的唯一状态源 |
| K-096 | RTS_SET/UNSET 宏定义与置位/清零语义 | 机制 | 存量 | **06 §2.1.6**、11 §2.7 | `proc.h:206-231` | 理解标志即队列操作 |
| K-097 | 特权表分治与 USER_PRIV 共享 | 数据结构 | 存量 | **06 §2.2.0**、22 §1 | `priv.h:94-95`、`com.h:78` | 理解 64 槽怎么分 |
| K-098 | sys_id 与 proc_nr 两个独立索引空间 | 概念 | 存量 | **06 §2.2.0** | `type.h`、`endpoint.h:65-69` | 避免混用两个编号 |
| K-099 | ppriv_addr 历史桥接表（已弃） | 演进 | 存量 | **06 §2.2.0** | `priv.h:94-95` | 知道 Rust 直接索引的理由 |
| K-100 | 静态/动态 priv 段与 get_priv 分配 | 机制 | 存量 | **06 §2.2.0/§2.2.1**、22 §4.5 | `system.c:274-302` | 理解特权槽分配 |
| K-101 | priv 字段的存储布局（6 组导航） | 数据结构 | 存量 | **06 §2.2.0**、22 §1.1 | `priv.h:21-66` | 字段字典 |
| K-102 | IPC 对称性不变量（set_sendto_bit 反向设位） | 约束 | 存量 | **06 §2.2.3**、22 §4.6.1 | `system.c:307-330,349-359` | 理解授权必须成对 |
| K-103 | image[] 编译时清单与编号映射表 | 数据结构 | 存量 | **06 §1.4.1/§2.3** | `table.c:44-65`、`param.h:9`、`com.h:74` | 知道 17 个 boot 项 |
| K-104 | VM bootstrap 鸡生蛋（内核手工建页表） | 决策 | 存量 | **06 §1.4.2/§2.3** | `protect.c:388-455` | 理解第一个用户进程 |
| K-105 | 清空→填充节拍与 12 步 | 机制 | 存量 | **06 §1.5**、06-todo §7 | `proc.c:119-159`、`main.c:164-271` | 记住阶段 C 顺序 |
| K-106 | arch_proc_reset / arch_proc_init | 机制 | 存量 | **06 §2.1.4/§4.1** | `arch_system.c:146-186`、`memory.c:722-731` | 知道新进程如何可执行 |
| K-107 | CpuContextArch trait | 接口 | 存量 | **06 §3.5/§4.1** | `os/arch/src/arch/boot.rs:144` | 看懂三架构上下文 |
| K-108 | load_vm_elf 共享实现与栈布局 | 机制 | 存量 | **06 §3.7/§4.2** | `os/arch/src/arch/boot.rs:296` | 看懂 VM ELF 装载 |
| K-109 | 零堆三层因果链（编译期定容→boot 无堆→`[T;N]`） | 决策 | 存量 | **06 §2.0**、06-todo §10.5 | `sys_config.h:8-9`、`param.h:9`、`priv.h:101-102` | 理解 no_std 的真实约束 |
| K-110 | KProcess Drop 只报警（生命周期纪律） | 约束 | 存量 | **06 §3.1.1**、panic-in-drop 全文 | `os/kernel/src/proc.rs`（KProcess Drop） | 知道内核对象不可静默消失 |
| K-111 | CapabilityTemplate（correct-by-construction 特权模板） | 接口 | 存量 | **06 §3.2/§4.5** | `os/kernel/src/capability.rs` | 看懂 5 类 boot 进程权限 |
| K-112 | 三架构进程初始化对照（PSW/入口/栈/ps_strings） | 接口 | 存量 | **06 §1.2.3/§4.1** | `os/arch/src/x86_64/boot.rs:29-34`、`archconst.h:118-119` | 跨架构核对 |
| K-113 | FPU 保存区内嵌（架构相关尺寸） | 数据结构 | 存量 | **06 §3.6/§4.9**、31 | `arch_system.c:134`、`os/arch/src/*/fpu.rs` | 知道 KProcess 大小差异 |
| K-114 | kernel_may_alloc 窗口（关闭 = VM 接管内存） | 约束 | 存量 | **06 §1.6.2/§3.8**、08 §1.4 | `main.c:105`、`glo.h:76` | 理解分配纪律 |
| K-115 | Drop 三分类法与测试夹具豁免 | 测试 | 新增 | panic-in-drop §1/§4 | `os/kernel/src/proc.rs`、`os/kernel/src/test_helpers.rs` | 写测试时知道如何豁免 |

### 2.9 去向 = 08-cross-space-init

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-116 | 跨地址空间访问的核心矛盾 | 概念 | 存量 | **07 §1.1** | `memory.c:592-666` | 理解"借页表"问题 |
| K-117 | 32 位临时窗口（freepdes + createpde + mem_clear_mapcache） | 机制 | 存量 | **07 §1.2/§2.4/§2.5** | `memory.c:69,707-717`、`protect.c:370-377` | 知道 C 的历史方案 |
| K-118 | ptproc 双用途（临时映射用途废弃；setcr3 重载决策保留） | 机制 | 存量 | **07 §2.1**、09 §4.8 | `cpulocals.h:55`、`arch_do_vmctl.c:25` | 理解 ptproc 为何还在 |
| K-119 | direct_map 公式（va = pa + BASE）与跨进程四步 | 机制 | 存量 | **07 §1.3** | `os/arch/src/arch/direct_map.rs:28` | 理解 64 位的替代解 |
| K-120 | 双视图地址空间的 ISA 必然性（U/S 不能同时 0/1） | 约束 | 存量 | **07 §1.3.1/§3.2** | `os/arch/src/arch/direct_map.rs` | 理解为什么两个窗口 |
| K-121 | G=1 TLB 行为与 CR4.PGE | 机制 | 存量 | **07 §3.2** | `pg_utils.c:204-230`、`os/arch/src/x86_64/paging.rs` | 理解跨 CR3 不刷 TLB |
| K-122 | establish_boot_dm 双窗口建立（Kernel DM 先行） | 机制 | 存量 | **07 §1.3.2/§4.4** | `os/kernel/src/dm_coverage.rs`、`os/kernel/src/lib.rs:291` | 看懂 DM 覆盖建立 |
| K-123 | 两源候选并集与资源包含性选页粒度 | 数据结构 | 存量 | **07 §4.4** | `os/arch/src/arch/dm_coverage.rs` | 理解 hole 不可吞 |
| K-124 | DirectMapArch trait 与三架构 BASE/SIZE | 接口 | 存量 | **07 §4.1** | `os/arch/src/arch/direct_map.rs:28` | 看懂 DM 抽象 |
| K-125 | map_kernel 职责演进（去登记册与 freepdes） | 演进 | 存量 | **07 §3.4** | `os/servers/vm/src/vmproc/vmproc_handle.rs` | 知道 VM 侧配合 |
| K-126 | 阶段 D 降为"确认就绪"断言 | 决策 | 存量 | **07 §3.5/§4.2** | `os/kernel/src/lib.rs`（init_post_and_memory） | 理解简化边界 |
| K-127 | 废弃清单 9 项与保留项 | 演进 | 存量 | **07 §3.6/§4.3** | `07 §3.6` 表、`09 §4.8` | 知道 C 机制去向 |
| K-128 | free_upper_idx 契约字段保留 | 接口 | 存量 | **07 §4.3** | `os/boot-shim/src/uefi_helpers.rs` | 知道 kinfo 镜像用途 |
| K-129 | paging_init 语义分解与"无 replacement function" | 决策 | 新增 | 07-paging_init_gpt 全文 | `os/kernel/src/lib.rs`、`02-stage-vm/08-pagetable-ops.md` | 理解页表初始化职责边界 |

### 2.10 去向 = 09-system-init-and-boot-finish

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-130 | 启动收口的 Phase A-F 时序 | 机制 | 存量 | **08 §1.1** | `os/kernel/src/lib.rs`（kmain Phase A-F） | 知道第几阶段做什么 |
| K-131 | system_init 三段（IRQ 清表 / alarm timer 初始化 / call_vec 注册） | 机制 | 存量 | **08 §2.3** | `system.c:168-270` | 理解运行前准备 |
| K-132 | call_vec 与 map() 宏 | 机制 | 存量 | **08 §2.1/§2.2**、13 §2.2 | `system.c:52-57,188-268` | 理解函数指针分发 |
| K-133 | RTS_PROC_STOP 解除范围（排除 kernel task） | 约束 | 存量 | **08 §1.2/§1.4**、00 §1.4.1 | `main.c:64-66` | kernel task 永不调度的证据 |
| K-134 | add_memmap（4GB 截断/页对齐/空槽） | 机制 | 存量 | **08 §2.3/§4.5** | `pg_utils.c:86-121` | 理解内存回收 |
| K-135 | vm_running 从不置 1 的 C 缺陷与 Rust 修正 | 决策 | 存量 | **08 §2.2/§3 D6**、09 §1.2 | `main.c:47`（唯一写点） | 知道行为偏离 |
| K-136 | bsp_finish_booting 12 步 | 机制 | 存量 | **08 §2.3/§4.6** | `main.c:38-109` | 启动链最后一站 |
| K-137 | smp_init 不返回（成功路径内调 bsp_finish_booting） | 机制 | 存量 | **08 附录 A** | `main.c:304-316`、`arch_smp.c:168,339-345` | 避免读错调用树 |
| K-138 | Syscall enum 与编译期断言替代 call_vec | 演进 | 存量 | **08 §3.1/§4.1** | `os/kernel/src/syscall.rs:65` | 理解 Rust 的分发表达 |
| K-139 | CPU 身份探测与 C 的 model 截断缺陷 | 机制 | 存量 | **08 §4.6/§5.4** | `os/arch/src/arch/cpu_identity.rs`、`arch_system.c:212-244` | 学一个真实 C bug |
| K-140 | irq_hooks / s_alarm_timer 的初始化 | 数据结构 | 存量 | **08 §2.2** | `glo.h`（irq_handlers）、`priv.h:48` | 知道运行期表在哪初始化 |
| K-141 | add_memmap 的 mmap_size/mem_high_phys 缺口 | 工具 | 存量 | **08 §4.5/§6.1**、09 §6.1 | `pg_utils.c:110-115` | 知道 kinfo 遗留缺口 |

### 2.11 去向 = 10-vm-boot-protocol

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-142 | VM 与内核的启动协商 8 步 | 接口 | 存量 | **09 §1.2** | `system/do_vmctl.c:17-173` | 理解第一个用户进程如何接管 |
| K-143 | 双视图模型表（基址/权限/G/建立者/时机） | 数据结构 | 存量 | **09 §1.3** | `arch_do_vmctl.c`、`os/arch/src/arch/direct_map.rs` | 对照两窗口属性 |
| K-144 | do_vmctl 9 子命令 | 接口 | 存量 | **09 §2.1** | `system/do_vmctl.c:33-168` | 知道 VMCTL 全貌 |
| K-145 | arch_do_vmctl 4 子命令 | 接口 | 存量 | **09 §2.2** | `arch_do_vmctl.c:38-66` | 知道 x86 特有条目 |
| K-146 | setcr3 五步（存/写/使能分页/清 VMINHIBIT） | 机制 | 存量 | **09 §2.2/§4.5** | `arch_do_vmctl.c:19-33` | 理解地址空间切换 |
| K-147 | MEMREQ_GET / MEMREQ_REPLY 与 VMSTYPE | 机制 | 存量 | **09 §2.3** | `system/do_vmctl.c:37-110`、`proc.h:98-101` | 理解挂起请求链路 |
| K-148 | VMINHIBIT SET/CLEAR 的 SMP 语义（IPI / stale TLB / SENDA 重投） | 机制 | 存量 | **09 §2.4** | `system/do_vmctl.c:125-161`、`smp.c:75-140` | 理解启动期跨 CPU 协同 |
| K-149 | VmCtlParam / VmCtlResult / VmCtlError | 接口 | 存量 | **09 §4.1/§4.3** | `os/kernel/src/vm.rs:758-820` | 看懂 Rust 协议类型 |
| K-150 | TlbArch 与 set_active_root | 接口 | 存量 | **09 §4.7/§4.8.2** | `os/arch/src/arch/tlb_arch.rs:78` | 看懂 CR3/TTBR0/satp 切换 |
| K-151 | CURRENT_PTPROC_NR / CURRENT_ROOT_PHYS | 数据结构 | 存量 | **09 §4.8.1/§4.9.1** | `os/kernel/src/lib.rs` | 知道 boot 根传递 |
| K-152 | Paging::from_active_root | 接口 | 存量 | **09 §4.9.2** | `os/arch/src/arch/paging.rs` | 理解不清零包装 |
| K-153 | boot 期真实 VM ELF 加载（P9-5） | 机制 | 存量 | **09 §4.9.3** | `os/kernel/src/lib.rs`（init_clock_and_interrupts 内） | 知道 VM 镜像何时进内存 |
| K-154 | ClearMapCache WONTFIX | 决策 | 存量 | **09 §4.7** | `system/do_vmctl.c:162` | 知道 64 位无此概念 |

### 2.12 去向 = 11-trap-entry

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-155 | 三类进入路径与向量表布局 | 接口 | 存量 | **14 §1.2/附录 A**、13 §1.2 | `protect.c:147-153`、`earm/mpx.S:181-184`、`archconst.h:31-35` | 知道 CPU 怎么进内核 |
| K-156 | 异常帧（CPU 压栈字段） | 数据结构 | 存量 | **14 §1.3** | `arch_proto.h:72-80`、`stackframe.h:17-36` | 看懂 frame 指针 |
| K-157 | 入口汇编保存/恢复上下文（SAVE_PROCESS_CTX） | 机制 | 存量 | **14 §1.2/§2.1** | `mpx.S:202-337,340-389`、`sconst.h:75-89` | 理解寄存器快照 |
| K-158 | 返回指令三形态（iretq / sysret / sysexit；eret；sret） | 机制 | 存量 | **14 §4.8**、10 §4.2 | `mpx.S:391-459`、`os/arch/src/arch/trap_return.rs:72` | 理解出口选择 |
| K-159 | IPC/SYSCALL 寄存器约定 | 接口 | 存量 | **13 §1.2**、18-trap-bridge 决策二 | `usermapped_glo_ipc.S:80-104`、`os/kernel/src/trap_dispatch.rs` | 能写用户侧调用 |
| K-160 | usermapped trampoline（C）与 syscall 指令替代 | 演进 | 存量 | **28 §1.3**、13 §6.3 | `usermapped_glo_ipc.S:10-108` | 理解入口代际变化 |
| K-161 | KTS trap_style（入口方式记录与消费） | 数据结构 | 存量 | **19 §4.6.1**、32 §2.1 | `archconst.h:167-172`、`arch_system.c:577-610` | 理解返回路径分流 |
| K-162 | 二次返回通道（保存上下文 RBX） | 机制 | 新增 | 18-trap-bridge 决策二 | `os/arch/src/x86_64/boot.rs`（set_secondary_ipc_return） | 理解 MINIX_KERNINFO 返回 |
| K-163 | 统一 CpuContext 保存模型 | 决策 | 存量 | **18-trap-bridge 决策三** | `os/arch/src/arch/boot.rs:144`、`os/kernel/src/trap_dispatch.rs` | 理解阻塞如何平凡化 |
| K-164 | 同环/半环返回分流 | 机制 | 存量 | **14 §4.8** | `mpx.S:461-585`、`os/arch/src/x86_64/trap_stub.rs` | 理解内核态中断返回 |
| K-165 | E1 trap 桥双轨收敛开放问题 | 决策 | 新增 | 18-trap-bridge §五 | `18-trap-bridge-design.md:84-87` | 知道遗留风险 |
| K-166 | SYSENTER/SYSCALL 快速入口的瘦帧与限制 | 机制 | 存量 | **14 附录 A/附录 C** | `mpx.S:202-260`、`archconst.h:KTS_*` | 理解快速入口代价 |

### 2.13 去向 = 12-concurrency-model

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-167 | BKL：用串行化换简单性 | 概念 | 存量 | **16 §1.1** | `smp.c:27-28`、`spinlock.h:40-41` | 理解内核并发策略 |
| K-168 | BKL 三条约束（禁睡眠/非递归/阻塞前释放） | 约束 | 存量 | **16 §1.1** | `smp.c:80,86-111` | 写临界区前知道红线 |
| K-169 | BKL 获取/释放点（启动一次 + context_stop） | 机制 | 存量 | **16 §2.2/§4.13**、10 §2.1 | `main.c:149`、`arch_clock.c:242,346` | 知道临界区边界 |
| K-170 | per-CPU 数据免锁（cpulocals 14 项） | 数据结构 | 存量 | **16 §1.2/§2.3** | `cpulocals.h:37-75` | 理解哪些状态不用锁 |
| K-171 | 中断上下文 vs 进程上下文约束 | 概念 | 存量 | **00 §5.3**、16 §1.1 | 全 stage 约定 | 知道 handler 里不能做什么 |
| K-172 | spinlock 实现与单核退化 | 机制 | 存量 | **16 §2.2** | `klib.S:733-771`、`spinlock.h:27-35` | 理解锁的硬件实现 |
| K-173 | 内存序基础与"IPI 是通知、Release/Acquire 是发布" | 约束 | 存量 | **16 §3 D9/§4.5**、smp_todo §3.9 | `os/kernel/src/smp.rs:271-331` | 理解跨 CPU 可见性 |
| K-174 | BklGuard / BklSection witness | 演进 | 存量 | **16 D3/D4/§4.12** | `os/kernel/src/smp.rs:1223-1350` | 看懂 Rust 如何强制持锁 |
| K-175 | SMP 关闭时的退化路径 | 机制 | 存量 | **16 附录 A**、08 附录 A | `arch/i386/include/arch_smp.h:18-23` | 理解单核构建 |
| K-176 | 临界区内禁止的操作清单（sleep/schedule/IPC/等待锁） | 约束 | 存量 | **00 §1.5.1**、16 §1.1 | `AGENTS.md` 约束 + `smp.c` 注释 | 避免写坏内核 |

### 2.14 去向 = 13-scheduling-primitives

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-177 | INV-1：在队列 ⟺ 可运行 | 约束 | 存量 | **11 §1.1** | `proc.c:1595-1813` | 调度代码的验收不变量 |
| K-178 | 16 级优先级队列与 nice 反直觉 | 概念 | 存量 | **11 §1.2** | `config.h:66-74` | 读懂优先级数值 |
| K-179 | per-CPU 队列结构（head/tail + p_nextready） | 数据结构 | 存量 | **11 §2.1** | `cpulocals.h:57-60`、`proc.h:38-45` | 理解 O(1) 入队 |
| K-180 | enqueue 三职责与抢占门 | 机制 | 存量 | **11 §2.2/§3.5** | `proc.c:1595-1659` | 理解入队何时触发抢占 |
| K-181 | enqueue_head / dequeue（pointer-pointer 惯用法） | 机制 | 存量 | **11 §2.3/§2.4** | `proc.c:1670-1780` | 看懂队列操作 |
| K-182 | pick_proc 与 bill_ptr | 机制 | 存量 | **11 §2.5** | `proc.c:1785-1813` | 理解计费对象选择 |
| K-183 | proc_no_time 两分支 | 机制 | 存量 | **11 §2.6** | `proc.c:1893-1910` | 理解量子耗尽处理 |
| K-184 | RTS 位与就绪队列的联动（enqueue/dequeue 时机） | 机制 | 存量 | **11 §2.7**、06 §2.1.6 | `proc.h:206-231` | 标志与队列合一 |
| K-185 | enqueue C bug 与 enter_queue 修复 | 决策 | 存量 | **11 §2.2/§3.6** | `proc.c:1653,1701` | 知道 Rust 修正了什么 |
| K-186 | Scheduler / ProcessTable 拆分（EnqueueInfo） | 决策 | 存量 | **11 §3.2** | `os/kernel/src/sched.rs` | 学借用拆分手法 |
| K-187 | Priority newtype（u8, 0..=15） | 数据结构 | 存量 | **11 §3.3** | `os/kernel/src/proc.rs` | 类型安全表达优先级 |
| K-188 | SchedParams 与 sched_proc 9 步 | 接口 | 存量 | **11 §3.8/§4.6** | `system.c:642-723`、`os/kernel/src/sched.rs` | 理解用户态调度器接口 |
| K-189 | IDLE 永不入队（RTS_PROC_STOP） | 约束 | 存量 | **11 §3.7** | `proc.c:149-158`、`main.c:64` | 避免误调度 IDLE |
| K-190 | 通知调度器消息（SCHEDULING_NO_QUANTUM） | 机制 | 存量 | **11 §4.5** | `proc.c:1860-1891`、`com.h:803` | 理解协作调度协议 |
| K-191 | 阶段 C RTS 履历表 | 数据结构 | 存量 | **11 附录 A**（自 06 迁入） | `proc.c:119-159`、`main.c:165-270` | 每个位何时置位 |
| K-192 | sched_proc 参数验证（cpu/priority/quantum/niced） | 约束 | 存量 | **11 §4.6** | `system.c:642-723` | 知道入参边界 |

### 2.15 去向 = 14-ipc-core

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-193 | endpoint 是通信标识符 | 概念 | 存量 | **99 §1.1**、17 §D2 | `endpoint.h`、`notes/rewrite/concepts/endpoint.md` | 理解 IPC 寻址 |
| K-194 | endpoint 编码布局（generation 高 15 位 + slot 低 15 位 + 负任务区） | 数据结构 | 存量 | **99 §1.2** | `endpoint.h:45-69`、`os/libs/minix-types/src/types/endpoint.rs` | 能手工编码端点 |
| K-195 | generation 的防陈旧语义与验证条件 | 机制 | 存量 | **99 §1.3-1.4**、17 §1.2 | `system/do_fork.c:59,69-72`、`proc.c:133` | 理解槽位复用的安全 |
| K-196 | 特殊 endpoint（ANY/NONE/SELF/ANY_USR/ANY_SYS/ANY_TSK） | 数据结构 | 存量 | **99 §1.3**、23 §2.7 | `endpoint.h:54-57`、`ipc_filter.h:24-28` | 看懂通配语义 |
| K-197 | 端点验证三重条件（isokendpt_f） | 机制 | 存量 | **99 §1.4**、12 §2.3 | `proc.c:1830-1858` | 理解 EDEADSRCDST 来源 |
| K-198 | 六原语语义模型（按阻塞行为分类） | 概念 | 存量 | **12 §1.2** | `ipcconst.h:7-14`、`proc.c:599-698` | IPC 全景 |
| K-199 | do_sync_ipc 四层检查（端点/ANY/白名单/陷阱掩码/内核任务） | 机制 | 存量 | **12 §2.3** | `proc.c:479-597` | 知道谁被拒绝 |
| K-200 | mini_send 两路径（直接投递 / 阻塞入 caller_q） | 机制 | 存量 | **12 §2.4** | `proc.c:870-962` | 理解发送语义 |
| K-201 | mini_receive 来源优先级（notify→async→caller_q→阻塞） | 机制 | 存量 | **12 §1.4/§2.5** | `proc.c:967-1117` | 理解接收顺序 |
| K-202 | mini_notify 永不阻塞与位图索引（priv_id） | 机制 | 存量 | **12 §2.6** | `proc.c:1122-1167` | 理解通知语义 |
| K-203 | deadlock 检测与 2-cycle 特例 | 机制 | 存量 | **12 §1.5/§2.7** | `proc.c:703-768` | 理解 ELOCKED |
| K-204 | SENDREC 原子性与 MF_REPLY_PEND | 机制 | 存量 | **12 §1.2** | `proc.c:1000-1005`、`proc.h:234-235` | 理解请求-回复 |
| K-205 | SENDA 表扫描与 pending 重投 | 机制 | 存量 | **12 §2.9** | `proc.c:1200-1326,1348-1505` | 理解批量异步 |
| K-206 | delivermsg 延迟投递与两次失败处理 | 机制 | 存量 | **12 §2.8** | `proc.c:263-294` | 理解缺页重投 |
| K-207 | WILLRECEIVE / CANRECEIVE / P_BLOCKEDON 宏 | 接口 | 存量 | **12 §2.10** | `ipc.h:14-22`、`proc.h:187-198` | 看懂匹配条件 |
| K-208 | caller_q 侵入式 FIFO（零堆） | 数据结构 | 存量 | **12 §3.2/§4.9** | `os/kernel/src/ipc.rs` | 学 no_std 数据结构 |
| K-209 | IpcOutcome / IpcEngine / IpcError 分层 | 演进 | 存量 | **12 §3.1/§3.3/§3.5** | `os/kernel/src/ipc.rs` | 看懂 Rust IPC 表达 |
| K-210 | MF_SIG_DELAY 与信号延迟投递 | 机制 | 存量 | **12 §3.11/§4.4** | `proc.c:1082-1083`、`system.c:454-464` | 理解信号与 IPC 交互 |
| K-211 | SENDA 尺寸上限（>16×(NR_TASKS+NR_PROCS) → EDOM） | 约束 | 存量 | **12 §4.8** | `proc.c:681-682` | 知道批量上限 |
| K-212 | 异步消息线格式（asynmsg_t / AMF_* 标志） | 接口 | 存量 | **12 §3.10** | `ipc.h:2745-2761` | 理解异步协议字段 |

### 2.16 去向 = 15-syscall-dispatch

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-213 | KERNEL_CALL=0x600 归一化（分派视角） | 接口 | 存量 | **13 §1.3/§2.1** | `com.h:205-270` | 能定位任意调用号 |
| K-214 | 为什么用独立 trap 入口而不是 m_type 区间 | 决策 | 存量 | **13 §1.2** | `protect.c:147-150`、`ipcconst.h` | 理解入口分离 |
| K-215 | kernel_call_dispatch 三段式（边界→权限→分派） | 机制 | 存量 | **13 §2.4.1** | `system.c:95-127` | 理解请求路由 |
| K-216 | 权限位图 s_k_call_mask 门 | 机制 | 存量 | **13 §2.4.1**、22 §1.3 | `system.c:111-114`、`priv.h:38` | 理解 ECALLDENIED |
| K-217 | 46 有效槽与 12 空缺槽 | 数据结构 | 存量 | **13 §2.3** | `system.c:193-268` | 知道哪些调用不存在 |
| K-218 | kernel_call 的 TOCTOU 防护（先拷内核副本） | 机制 | 存量 | **13 §2.4.3** | `system.c:136-163` | 理解参数拷贝 |
| K-219 | kernel_call_finish 与 EDONTREPLY | 机制 | 存量 | **13 §2.4.2** | `system.c:59-93` | 理解谁负责回复 |
| K-220 | 内核调用挂起的 VMSUSPEND 返回约定 | 接口 | 存量 | **13 §1.4** | `vm.h:6`、`system.c:61-69,612-637` | 理解内核如何等 VM |
| K-221 | MF_KCALL_RESUME 时序与重放 | 约束 | 存量 | **13 §1.4/§2.4.4** | `proc.c:355-361`、`system.c:612-637` | 理解恢复点 |
| K-222 | BKL 持有区间五阶段 | 约束 | 存量 | **13 §1.5** | `system.c`、`arch_clock.c:242,346` | 知道锁在谁手里 |
| K-223 | Syscall enum + TryFrom | 演进 | 存量 | **13 §4.1** | `os/kernel/src/syscall.rs:65` | 看懂 Rust 分发表 |
| K-224 | KcallResult 五变体与 reply_code | 接口 | 存量 | **13 §4.2** | `os/kernel/src/syscall.rs:198-212` | 看懂返回值约定 |
| K-225 | ArchSyscall 默认 BadCall | 接口 | 存量 | **13 §3.4** | `os/kernel/src/syscall.rs:245-366` | 理解跨架构退化 |
| K-226 | dispatch_ipc_entry 与 kernel_call 共享入口的分工 | 机制 | 存量 | **13 §4.8** | `os/kernel/src/syscall.rs:651+` | 理解双入口编排 |
| K-227 | retry/重派发语义（arch_do_syscall 延迟调用） | 机制 | 存量 | **13 §6.2**、10 §4.5 | `arch_system.c:485-493`、`proc.c:368-381` | 知道追踪如何插入 |
| K-228 | kbill_kcall 计费 | 机制 | 存量 | **13 §6.4** | `system.c:160`、`arch_clock.c:279-281` | 理解服务时间记账 |
| K-506 | caller-by-nr：用槽位号取代 `&Proc` 指针的调用者接口 | 演进 | 新增 | **33 §Ch1-§Ch2**（蓝图交付后新增篇） | `os/kernel/src/syscall.rs:479`（`kernel_call`）、`minix3/minix/kernel/system/do_schedule.c:8`（C 侧为指针签名）、`system.c:141`（`p_delivermsg_vir` 保存） | 理解 Rust 借用冲突的出路与统一参数次序 |

### 2.17 去向 = 16-switch-to-user

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-229 | switch_to_user 是汇聚点且永不返回 | 概念 | 存量 | **10 §1.1** | `proc.c:299-474` | 理解内核出口唯一 |
| K-230 | 五阶段控制流 | 机制 | 存量 | **10 §1.3/§2.1** | `proc.c:309-474` | 能背出调度循环骨架 |
| K-231 | PREEMPTED 队头/队尾规则 | 机制 | 存量 | **10 §2.1** | `proc.c:322-330` | 理解抢占公平性 |
| K-232 | switch_address_space 三情形 | 机制 | 存量 | **10 §2.1/§2.3** | `proc.c:349`、`klib.S:605-626` | 理解 CR3 何时切换 |
| K-233 | check_misc_flags 五标志优先级 | 机制 | 存量 | **10 §2.1** | `proc.c:351-415` | 理解延迟工作清单 |
| K-234 | 量子检查位置（唯一不与 IPC 竞争的点） | 决策 | 存量 | **10 §2.1** | `proc.c:416-428` | 理解 proc_no_time 调用点 |
| K-235 | finish_and_restore 终局分派 | 机制 | 存量 | **10 §4.2** | `proc.c:437-474` | 理解 BKL/FPU/恢复顺序 |
| K-236 | idle 六步 | 机制 | 存量 | **10 §2.2/§4.3** | `proc.c:176-229` | 理解 CPU 空转 |
| K-237 | 与 C 的差异总表（17 项，含已知缺口） | 决策 | 存量 | **10 §4.5** | `os/kernel/src/lib.rs`、`os/kernel/src/proc_table.rs` | 知道 Rust 现状边界 |
| K-238 | BKL 与中断协作全景 | 机制 | 存量 | **10 §4.6** | `arch_clock.c:242-349` | 理解锁与陷入的交互 |
| K-239 | 信号在杂项循环"就地"投递 | 决策 | 存量 | **10 §3.3** | `os/kernel/src/proc_table.rs`（process_misc_flags） | 理解信号时机 |

### 2.18 去向 = 17-exception-interrupt

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-240 | 异常帧嵌套与来源判定（is_nested / 用户态 / 内核态） | 机制 | 存量 | **14 §1.1/§2.1** | `exception.c:180-283` | 理解异常分流 |
| K-241 | exception_handler 五分支 | 机制 | 存量 | **14 §2.1** | `exception.c:180-283` | 能说出每条分支去向 |
| K-242 | ex_data 向量→信号→最低处理器表 | 数据结构 | 存量 | **14 §2.4** | `exception.c:19-40` | 知道异常→信号映射 |
| K-243 | 嵌套恢复点四类（copy_msg / fxrstor / phys_copy / phys_memset） | 机制 | 存量 | **14 §2.2** | `exception.c:66-70,200-230` | 理解故障恢复点 |
| K-244 | 页错误转发 VM（RTS_PAGEFAULT + VM_PAGEFAULT 消息） | 机制 | 存量 | **14 §2.3** | `exception.c:49-130` | 理解缺页跨主体处理 |
| K-245 | 内核态/VM 自身缺页 = panic（禁止循环依赖） | 约束 | 存量 | **14 §1.5/§2.3** | `exception.c:100-113,167` | 知道红线 |
| K-246 | 异常路径触发信号（cause_sig 调用点） | 机制 | 存量 | **14 §2.1**、23 §1.4 | `system.c:389-449` | 理解信号产生 |
| K-247 | IRQ hook 链（put/rm/irq_handle/actid） | 机制 | 存量 | **14 §2.5** | `interrupt.c:23-176` | 理解共享中断 |
| K-248 | generic_handler 副作用链（随机数→位图→通知→重启用） | 机制 | 存量 | **14 §4.4**、24 §1.1 | `system/do_irqctl.c:143-172` | 理解中断变通知 |
| K-249 | ExceptionArch / ExceptionDispatcher / FaultContext | 接口 | 存量 | **14 §3.2/§4.2/§4.3** | `os/arch/src/arch/exception.rs:40`、`exception_dispatcher.rs:79` | 看懂 Rust 异常分流 |
| K-250 | IrqManager / IrqNotify / KernelNotifier | 接口 | 存量 | **14 §3.6/§4.4** | `os/kernel/src/irq_manager.rs` | 看懂 Rust IRQ 管理 |
| K-251 | classify_signal 与 #NM 特判 | 机制 | 存量 | **14 §4.5**、31 §4.4 | `exception_dispatcher.rs:124,220` | 理解向量到动作映射 |
| K-252 | page_fault.rs helpers 拆分 | 机制 | 存量 | **14 §4.6** | `os/kernel/src/page_fault.rs` | 看懂可测的缺页构造 |
| K-253 | 三架构 IRQ 抽象与共享中断 | 演进 | 存量 | **14 §2.7/附录 A** | `os/plat/src/*/interrupt.rs`、`hw_intr.h:19-52` | 跨架构对照 |
| K-254 | 异常处理与信号/VM/panic 的边界表 | 接口 | 存量 | **14 附录 B** | `14 附录 B` | 知道该查哪篇 |
| K-255 | 嵌套调试异常与内核态异常容忍策略 | 机制 | 存量 | **14 §2.1** | `exception.c:232-250` | 理解 trace 特例 |

### 2.19 去向 = 18-cross-space-runtime

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-256 | 内核代行缺页问题 | 概念 | 存量 | **24 §1** | `memory.c:592-705` | 理解为什么需要挂起协议 |
| K-257 | 三阶段抽象（解析→拷贝→回报） | 概念 | 存量 | **24 §1.1** | `memory.c:592-666` | 结构化理解跨空间拷贝 |
| K-258 | VMREQUEST 协议（挂起→SIGKMEM→REPLY→恢复重试） | 机制 | 存量 | **24 §1.2** | `proc.c:234-258`、`system/do_vmctl.c:37-110` | 理解内核与 VM 的恢复协作 |
| K-259 | VMSUSPEND 伪错误码与 Rust 类型分离 | 接口 | 存量 | **24 §1.3** | `vm.h:6-8`、`os/kernel/src/vm.rs` | 区分挂起与非法 |
| K-260 | 三种挂起类型（KERNELCALL/DELIVERMSG/MAP） | 约束 | 存量 | **24 §1.4** | `proc.h:98-101` | 知道挂起来源 |
| K-261 | VMREQUEST vs PAGEFAULT | 决策 | 存量 | **24 §1.5** | `proc.h:142-166`（0x800/0x400） | 区分两个缺页场景 |
| K-262 | sys_datacopy 宏澄清（无 SYS_DATACOPY） | 接口 | 存量 | **24 §2.1** | `syslib.h:129` | 避免找不存在的文件 |
| K-263 | do_copy 算法与 CP_FLAG_TRY | 机制 | 存量 | **24 §2.4**、18 §1.1 | `system/do_copy.c:22-89` | 理解拷贝入口 |
| K-264 | data_copy_vmcheck（C/Rust） | 机制 | 存量 | **24 §2.5/§4.2** | `memory.c:690-705`、`os/kernel/src/cross_space.rs` | 理解检查与挂起点 |
| K-265 | createpde/lin_lin_copy → Direct Map | 演进 | 存量 | **24 §2.6/§3 D1** | `memory.c:69-147` | 理解 C/Rust 机制差异 |
| K-266 | vm_suspend 实现（链头插 + 空链通知） | 机制 | 存量 | **24 §2.8** | `proc.c:234-258` | 理解请求入链 |
| K-267 | kernel_call_resume 重放 | 机制 | 存量 | **24 §2.8/§4.6** | `system.c:612-637` | 理解恢复执行 |
| K-268 | VmRequestQueue（Option 化队列） | 数据结构 | 存量 | **24 §3 D6/§4.4** | `os/kernel/src/vm.rs` | 看懂 Rust 表达 |
| K-269 | 挂起未入队缺口（P1） | 工具 | 存量 | **24 §4.6** | `os/kernel/src/cross_space.rs` | 知道当前缺口 |
| K-270 | 部分拷贝进度 WONTFIX | 决策 | 存量 | **24 §4.6** | `24 §4.6` 表 | 理解为何不做 |

### 2.20 去向 = 19-clock-timer

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-271 | tick 四类工作（记账/vtimer/闹钟/负载） | 机制 | 存量 | **15 §1.1** | `clock.c:70-173` | 知道 tick 做什么 |
| K-272 | BSP 独占时间维护 | 约束 | 存量 | **15 §1.4** | `clock.c:91-104` | 理解多核下时钟唯一性 |
| K-273 | 双重记账规则（BILLABLE / 非 BILLABLE 记到 billp） | 机制 | 存量 | **15 §1.4/§2.3** | `clock.c:113-120` | 理解计费语义 |
| K-274 | prof 定时器双重递减 | 机制 | 存量 | **15 §1.4/§2.3** | `clock.c:134-138` | 理解性能采样 |
| K-275 | adjtime 渐变调整 | 机制 | 存量 | **15 §1.3/§3.4** | `clock.c:42,97-100,195-198` | 理解实时钟微调 |
| K-276 | clock_timers 排序链与前缀到期 | 数据结构 | 存量 | **15 §1.3/§4.1.2** | `clock.c:37,159-161`、`lib/libtimers/tmrs_exp.c` | 理解定时器队列 |
| K-277 | cause_alarm（CLOCK notify） | 机制 | 存量 | **15 §2.3** | `system/do_setalarm.c:69-76` | 理解闹钟唤醒 |
| K-278 | vtimer_check（到期发信号） | 机制 | 存量 | **15 §2.3/§3.11** | `system/do_vtimer.c:81-103` | 理解虚拟定时器 |
| K-279 | load_update（6 秒槽 × 150） | 机制 | 存量 | **15 §2.3/§4.10** | `clock.c:260-292` | 理解负载平均 |
| K-280 | tick_with 零分配热路径 | 机制 | 存量 | **15 §4.5** | `os/kernel/src/clock.rs` | 学 no_std 热路径写法 |
| K-281 | TimerId/TimerEntry/TimerAction/TimerQueue | 数据结构 | 存量 | **15 §4.1.2** | `os/kernel/src/clock.rs`（双索引） | 看懂 Rust 定时器 |
| K-282 | 全局时间镜像 AtomicU64 | 数据结构 | 存量 | **15 §4.1.1** | `os/kernel/src/globals.rs` | 理解任意上下文读时间 |
| K-283 | quantum 递减归 context_stop | 决策 | 存量 | **15 §1.4 规则 3** | `arch_clock.c:208-349`（326-330） | 纠正 tick 的职责边界 |
| K-284 | arch_timer_int_handler i386 为空 | 机制 | 存量 | **15 §2.3** | `arch_clock.c:72-74` | 知道架构钩子位置 |
| K-285 | TimerTickResult/VtimerExpired 类型 | 接口 | 存量 | **15 §4.1.4/§4.6** | `os/kernel/src/clock.rs` | 看懂 tick 返回值 |
| K-286 | load 溢出的 wrapping 语义 | 约束 | 存量 | **15 §4.1.3** | `os/kernel/src/clock.rs` | 复刻 u16 回绕 |



### 2.21 去向 = 20-smp-bringup

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-287 | AP 启动三架构硬件契约（INIT-SIPI / PSCI / SBI HSM） | 接口 | 存量 | **16 §1.5/§2.6**、smp_todo §3.1 | `arch_smp.c:100-170`、`trampoline.S:8-43` | 知道 AP 怎么醒 |
| K-288 | ApBootstrap ABI 与 AP early entry 梯子 | 数据结构 | 新增 | smp_todo §3.2/§9（旧文档仅摘要） | `os/arch/src/x86_64/ap_early_entry.rs`、`os/kernel/src/smp.rs` | 理解 16 位到长模式的约束 |
| K-289 | AP 自举内存序（publisher fence + 消费侧屏障） | 约束 | 存量 | **16 §3 D9**、smp_todo §3.9 | `os/kernel/src/smp.rs:271-331` | 避免启动竞态 |
| K-290 | 双位图（boot_ack_mask / online_mask） | 数据结构 | 新增 | smp_todo §3.4（旧文未展开） | `os/kernel/src/smp.rs` | 区分"活着"与"初始化完" |
| K-291 | IPI 两类（异步/同步）与向量 | 接口 | 存量 | **16 §1.3/§2.4** | `smp.c:14-23,63-66,114-154`、`apic.h:90-93` | 理解跨 CPU 通知 |
| K-292 | smp_schedule_sync 释放 BKL + 重入处理 | 机制 | 存量 | **16 §1.3/§4.8** | `smp.c:75-112` | 理解同步 IPI 协议 |
| K-293 | sched_handler 三任务（STOP / SAVE_CTX+FPU / VMINHIBIT） | 机制 | 存量 | **16 §1.3/§4.7** | `smp.c:156-187` | 理解远端操作 |
| K-294 | CPU 亲和性与迁移 | 机制 | 存量 | **16 §1.4/§2.6** | `smp.c:142-154`、`proc.h`（p_cpu） | 理解进程绑核 |
| K-295 | BSP/AP 握手与 boot_lock | 机制 | 存量 | **16 §2.6/§4.11** | `smp.c:28-54`、`arch_smp.c:214-251` | 理解启动屏障 |
| K-296 | halt/shutdown（IPI + 停止清零） | 机制 | 存量 | **16 §1.5** | `smp.c:56-61`、`arch_smp.c:177-212` | 理解关机路径 |
| K-297 | 0x467 warm-reset 取舍（不移植） | 决策 | 存量 | **16 §9.4** | `16 §9.4` | 知道历史兼容被放弃 |
| K-298 | AP 本地定时器初始化（app_cpu_init_timer） | 机制 | 存量 | **16 §1.5**、15 §1.0 | `clock.c:306-312`、`arch_smp.c:237` | 理解每 CPU tick |
| K-299 | 调度 IPI 触发与 PREEMPTED 联动 | 机制 | 存量 | **16 §4.10** | `smp.c:194-204` | 理解抢占跨 CPU |
| K-300 | 三架构 early stub 与固件 ABI 修复 | 演进 | 新增 | smp_todo §10（旧文档未收录） | `os/arch/src/*/ap_early_entry.rs` | 知道三个真实 ABI bug |


### 2.22 去向 = 21-privilege

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-394 | priv 分治模型（系统进程独立 / 用户共享 USER_PRIV） | 概念 | 存量 | **22 §1** | `priv.h:94-95`、`com.h:78` | 理解权限隔离 |
| K-395 | priv 字段语义与权限模型（逐组） | 数据结构 | 存量 | **22 §1.1**、06 §2.2.0 | `priv.h:21-66` | 字段字典 |
| K-396 | s_flags 位表（PREEMPTIBLE/BILLABLE/DYN_PRIV_ID 等） | 数据结构 | 存量 | **22 §1.2** | `const.h:143-154` | 理解权限标志 |
| K-397 | 三类掩码（s_ipc_to / s_k_call_mask / s_trap_mask） | 数据结构 | 存量 | **22 §1.3** | `priv.h:35-38` | 三种位图别混用 |
| K-398 | may_send_to / may_asynsend_to 不对称 | 接口 | 存量 | **22 §1.3** | `priv.h:86-87` | 理解异步 self-send |
| K-399 | 静态/动态 priv 分区与 ID 分配 | 数据结构 | 存量 | **22 §1.4** | `priv.h:10-21` | 理解 64 槽布局 |
| K-400 | get_priv 分配与错误码（EBUSY/ENOSPC/EINVAL） | 机制 | 存量 | **22 §1.4/§4.5** | `system.c:274-302` | 理解特权槽生命周期 |
| K-401 | fork 特权降级（SYS_PROC 子 → USER_PRIV） | 约束 | 存量 | **22 §1.4** | `system/do_fork.c:104` | 理解权限降级的安全动机 |
| K-402 | I/O/IRQ/MEM 范围表与线性扫描 | 数据结构 | 存量 | **22 §1.5** | `priv.h`（s_io_tab/s_irq_tab/s_mem_tab） | 理解权限检查成本 |
| K-403 | KPriv 8 子结构 | 演进 | 存量 | **22 §3 D1/§4.2** | `os/kernel/src/kpriv.rs` | 看懂 Rust 权限结构 |
| K-404 | ProcessCapability bitflags 与 wire 编解码 | 演进 | 存量 | **22 §4.1** | `os/kernel/src/capability.rs` | 理解位兼容 |
| K-405 | CapabilityTemplate（boot 权限构造） | 接口 | 存量 | **22 §3 D3/§4.4** | `os/kernel/src/capability.rs` | 看懂 boot 权限构造 |
| K-406 | Newtype 掩码（TrapMask/IpcMask/KCallMask） | 演进 | 存量 | **22 §3 D5/§4.4** | `os/kernel/src/capability.rs` | 类型防混用 |
| K-407 | PrivTable 固定数组与 Option 化 sentinel | 数据结构 | 存量 | **22 §3 D6-D8/§4.3** | `os/kernel/src/kpriv.rs` | 学 no_std 表设计 |
| K-408 | set_sendto_bit 对称不变量与 PrivTable::update_priv | 约束 | 存量 | **22 §4.6.1** | `system.c:307-330`、`os/kernel/src/kpriv.rs` | 第三主讲述点 |
| K-409 | SYS_PRIVCTL 11 子命令 | 接口 | 存量 | **22 §4.7** | `system/do_privctl.c:26-371` | 知道权限管理接口 |
| K-410 | clear_ipc_refs 的 retreg 设计缺口 | 决策 | 存量 | **22 §4.7** | `os/kernel/src/syscall.rs:893-942` | 知道未建模项 |
| K-411 | s_ipcf / s_stack_guard 裸表示限制（P2） | 约束 | 存量 | **22 §3 D9/D10** | `os/kernel/src/kpriv.rs` | 知道类型债 |


### 2.23 去向 = 22-ipc-filter

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-412 | 三层过滤模型（L1 位图 / L2 过滤链 / L3 状态报告） | 概念 | 存量 | **23 §1.1** | `priv.h:35-38,86-87`、`ipc.h:14-25` | 理解过滤全貌 |
| K-413 | 检查时机不对称（send 查 L1 / receive 查 L2） | 约束 | 存量 | **23 §1.2** | `system.c:111`、`ipc.h:19-22` | 理解为什么异步 |
| K-414 | 无 CHECK_IPC（过滤无条件执行） | 决策 | 存量 | **23 §1.4** | `priv.h:86` | 知道不可关闭 |
| K-415 | 位图原语（get/set/unset_sys_bit） | 机制 | 存量 | **23 §2.2/§4.1** | `const.h:20-27` | 看懂掩码操作 |
| K-416 | s_k_call_mask 检查顺序（EBADREQUEST → ECALLDENIED） | 机制 | 存量 | **23 §2.4/§4.2** | `system.c:103-123` | 复刻拒绝顺序 |
| K-417 | CANRECEIVE / WILLRECEIVE（receive 反向过滤） | 接口 | 存量 | **23 §2.5** | `ipc.h:14-22` | 理解接收方偏好 |
| K-418 | allow_ipc_filtered_msg（blacklist 默认放行 + 延迟读 m_type） | 机制 | 存量 | **23 §2.6** | `system.c:803-874` | 理解过滤执行 |
| K-419 | 过滤链组合语义（后过滤可覆盖前决策） | 约束 | 存量 | **23 §2.6** | `system.c:853-874` | 理解链式语义 |
| K-420 | IPCF_EL_MATCH 与端点类别宏 | 机制 | 存量 | **23 §2.7** | `ipc_filter.h:19-41` | 能写过滤规则 |
| K-421 | IPC_STATUS_* 状态报告 | 机制 | 存量 | **23 §2.8/§3 D9** | `ipc.h:25-48` | 理解过滤结果回传 |
| K-422 | Option 化过滤池与链（无悬垂） | 演进 | 存量 | **23 §3 D5/D6/§4.3** | `os/kernel/src/ipc_filter.rs` | 学安全表达 |
| K-423 | IpcFilterType 与 kcall_filter_check | 接口 | 存量 | **23 §3 D8/§4.2** | `os/kernel/src/ipc_filter.rs` | 看懂 Rust 过滤 |
| K-424 | memreq 过滤（dequeue_filtered） | 机制 | 存量 | **23 §4.5/Ch6** | `os/kernel/src/ipc_filter.rs` | 理解 VM 请求过滤 |


### 2.24 去向 = 23-syscall-process

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-301 | 进程生命周期四弧（fork→exec→exit→clear） | 概念 | 存量 | **17 §1.1** | `system/do_{fork,exec,exit,clear}.c` | 理解进程状态机 |
| K-302 | exec 是替换非新建 | 概念 | 存量 | **17 §1.1** | `system/do_exec.c:20-60` | 知道 endpoint 不变 |
| K-303 | exit 委托信号管理器（cause_sig(SIGABRT)） | 机制 | 存量 | **17 §1.1/§2.3** | `system/do_exit.c:14-27` | 理解死亡决策权 |
| K-304 | clear 幂等（isemptyp → OK） | 约束 | 存量 | **17 §1.1/§4.4** | `system/do_clear.c:17-80` | 防二次释放 |
| K-305 | 同步 fork：父必须 RTS_RECEIVING | 约束 | 存量 | **17 §1.2** | `system/do_fork.c:51` | 理解 fork 前提 |
| K-306 | fork 时代际递增与回绕实现 | 机制 | 存量 | **17 §1.2** | `system/do_fork.c:59,69-72` | 防旧引用复活 |
| K-307 | 信号不继承与继承清单 | 约束 | 存量 | **17 §1.2** | `system/do_fork.c:FORKSTR` | 知道子进程初值 |
| K-308 | SYS_PROC 子进程降级 | 机制 | 存量 | **17 §1.3** | `system/do_fork.c:105-122` | 防 confusable deputy |
| K-309 | VMINHIBIT（等 VM 建页表） | 机制 | 存量 | **17 §1.3** | `system/do_fork.c:115` | 理解新进程何时能跑 |
| K-310 | runctl SMP IPI 与本地路径 | 机制 | 存量 | **17 §1.4/§4.5** | `system/do_runctl.c:18-76`、`smp.c:114-121` | 理解停止/恢复 |
| K-311 | RC_DELAY → EBUSY → SIGSNDELAY 重试 | 机制 | 存量 | **17 §1.4/§4.5** | `system/do_runctl.c:44` | 理解延迟停止 |
| K-312 | schedctl 双模式与 sched_proc | 机制 | 存量 | **17 §2.6/§4.6** | `system/do_schedctl.c:7-46`、`system.c:642-723` | 理解调度权移交 |
| K-313 | do_schedule（SYS_SCHEDULE）语义 | 接口 | 新增 | 旧文仅 08/13 表行 | `system/do_schedule.c:8-30` | 知道 sched server 如何让位 |
| K-314 | statectl 5 请求（CLEAR_IPC_REFS/SET_STATE_TABLE/ADD_BL/ADD_WL/CLEAR_FILTERS） | 接口 | 存量 | **17 §2.7/§4.7** | `system/do_statectl.c:15-53` | 理解进程 IPC 状态控制 |
| K-315 | KProcess::fork_from 与 dispatch_clear 9 步 | 演进 | 存量 | **17 §3 D1/§4.4** | `os/kernel/src/proc.rs`、`syscall_process.rs` | 看懂 Rust 生命周期实现 |
| K-316 | clear 幂等修正案例（endpoint_to_nr 跳槽） | 测试 | 存量 | **17 §5.2** | `os/kernel/src/syscall_process.rs` | 学一个测试暴露的 bug |


### 2.25 去向 = 24-syscall-copy

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-317 | 按信任级别分层的拷贝原语 | 概念 | 存量 | **18 §1** | `system/do_copy.c`、`do_safecopy.c` | 理解五种原语的分工 |
| K-318 | SELF 替换与共用 handler | 机制 | 存量 | **18 §1.1/§2.1** | `system/do_copy.c:22-91` | 理解调用者自身语义 |
| K-319 | CP_FLAG_TRY（VFS 专用，防内存映射文件死锁） | 约束 | 存量 | **18 §1.1/§4.2** | `system/do_copy.c:80` | 理解为什么有 EFAULT 直返 |
| K-320 | grant 表与三种 grant（DIRECT/MAGIC/INDIRECT） | 数据结构 | 存量 | **18 §1.2** | `system/do_safecopy.c`（verify_grant）、`priv.h`（s_grant_*） | 理解授权模型 |
| K-321 | verify_grant 11 步 | 机制 | 存量 | **18 §1.2** | `system/do_safecopy.c:60-215` | 能审计每一步拒绝条件 |
| K-322 | 间接链深度上限 MAX_INDIRECT_DEPTH=5 | 约束 | 存量 | **18 §1.2/§3 D4** | `system/do_safecopy.c:148` | 防循环链 |
| K-323 | CPF_TRY 软故障与 ABA 防护 | 机制 | 存量 | **18 §1.2** | `system/do_safecopy.c:258` | 理解软失败语义 |
| K-324 | umap 与 umap_remote 权限差异 | 接口 | 存量 | **18 §1.3** | `system/do_umap.c:25`、`do_umap_remote.c:26` | 知道 DMA 地址解析限制 |
| K-325 | vumap 批量映射与 MAPVEC_NR=64 | 机制 | 存量 | **18 §1.3/§2.4** | `system/do_vumap.c`、`const.h` | 理解 DMA 描述符批量 |
| K-326 | memset/safememset 与 pattern 截断 | 机制 | 存量 | **18 §1.4** | `system/do_memset.c`、`do_safememset.c:56` | 复刻填充语义 |
| K-327 | do_setgrant（SYS_SETGRANT）语义 | 接口 | 新增 | 旧文仅 08/13 表行 | `system/do_setgrant.c:15-30` | 知道 grant 表如何登记 |
| K-328 | GrantVerifyResult / SoftFaultInfo / SafecopyAccess | 演进 | 存量 | **18 §3 D3/D9/§4.1** | `os/kernel/src/grant.rs`、`syscall_copy.rs` | 看懂 Rust 类型表达 |
| K-329 | VmCopyError / CrossSpaceResult / PteWalkArch | 接口 | 存量 | **18 §4.1/§4.7** | `os/kernel/src/vm.rs:98`、`os/arch/src/arch/pte_walk_arch.rs:60` | 看懂底层翻译抽象 |
| K-330 | 7 个 dispatch 全接入状态与 QEMU 端到端缺口 | 测试 | 存量 | **18 §4.8/§5.2** | `os/kernel/src/syscall_copy.rs` | 知道哪些路径未验证 |


### 2.26 去向 = 25-syscall-signal

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-331 | 双路径分层 + 推拉结合 | 概念 | 存量 | **19 §1** | `system.c:389-449` | 理解信号为何委托用户态 |
| K-332 | 三步闭环（挂起→管理器拉取→确认） | 机制 | 存量 | **19 §1.1** | `system/do_getksig.c:18-43`、`do_endksig.c:15-41` | 理解内核信号路径 |
| K-333 | 状态三件套（RTS_SIGNALED / RTS_SIG_PENDING / p_pending） | 数据结构 | 存量 | **19 §1.1** | `proc.h:142-166`、`os/kernel/src/proc.rs`（SigSet） | 理解信号状态机 |
| K-334 | SELF 路径（自管理进程自通知 SIGKSIGSM） | 机制 | 存量 | **19 §1.1/§4.3** | `system.c:416-435` | 理解 VM 等自管理进程 |
| K-335 | SIGSEND 时序约束（寄存器修改必须最后） | 约束 | 存量 | **19 §1.3** | `system/do_sigsend.c:126` | 避免不可逆破坏 |
| K-336 | s_sig_mgr / s_bak_sig_mgr 与致命信号规则 | 数据结构 | 存量 | **19 §1.4** | `system.c:412-432`、`priv.h` | 理解信号管理器提升 |
| K-337 | stop-delay（SENDING/SC_DEFER → EBUSY） | 机制 | 存量 | **19 §1.5/§4.8** | `system/do_runctl.c:44`、`system.c:454-464` | 理解安静点投递 |
| K-338 | cause_sig 三路径完整机制（外部/SELF 非致命/SELF 致命） | 机制 | 存量 | **19 §4.3** | `os/kernel/src/syscall_signal.rs` | 看懂 Rust 信号产生 |
| K-339 | is_lethal 六信号 | 约束 | 存量 | **19 §4.2** | `os/kernel/src/syscall_signal.rs` | 知道哪些信号致命 |
| K-340 | do_kill / do_getksig / do_endksig 校验 | 机制 | 存量 | **19 §2.3** | `system/do_kill.c:17-41` 等 | 复刻拒绝条件 |
| K-341 | SigSet(u64) 与 128 位缺口 | 演进 | 存量 | **19 §3 D1/§4.8** | `os/kernel/src/proc.rs` | 知道信号位图边界 |
| K-342 | SignalContext trait（sigframe/sigcontext 架构相关） | 接口 | 存量 | **19 §3 D2/D6/§4.6** | `os/arch/src/arch/signal_context.rs:159` | 看懂跨架构信号帧 |
| K-343 | trap_style 前置校验与 MINIX3 BUG 修复 | 决策 | 存量 | **19 §1.2/§4.6.1** | `system/do_sigsend.c:79`、`os/kernel/src/syscall_signal.rs` | 学一个用户可控 panic 修复 |
| K-344 | SC_TRACE→SIGTRAP 与 delivermsg Segfault→SIGSEGV | 机制 | 存量 | **19 §4.7** | `proc.c:392-398`、`proc.c:271-278` | 理解调试与投递失败 |
| K-345 | DIAGCTL SIGKMESS 通知为 no-op 的设计结论 | 决策 | 存量 | **19 §4.3/§4.7** | `19 §4.7` 表 | 知道 kmess 移除后果 |


### 2.27 去向 = 26-syscall-device

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-346 | IRQ 钩子三元组（endpoint / notify_id / policy） | 数据结构 | 存量 | **20 §1.1** | `system/do_irqctl.c:23-120` | 理解驱动挂中断 |
| K-347 | 4 子请求（SETPOLICY/RMPOLICY/ENABLE/DISABLE） | 接口 | 存量 | **20 §1.1** | `system/do_irqctl.c` | 管理钩子生命周期 |
| K-348 | notify_id ≤ 31（u32 位图） | 约束 | 存量 | **20 §1.1** | `system/do_irqctl.c:88` | 知道上限 |
| K-349 | 钩子 owner 校验 | 约束 | 存量 | **20 §1.1** | `system/do_irqctl.c:46,126` | 防他人摘钩 |
| K-350 | 进程退出摘钩不变量 | 约束 | 存量 | **20 §1.1** | `system/do_irqctl.c:160` | 防已死进程收中断 |
| K-351 | 三层端口 I/O（devio/vdevio/sdevio） | 概念 | 存量 | **20 §1.2** | `system/do_devio.c`、`do_vdevio.c`、`arch/i386/do_sdevio.c` | 理解 I/O 原语分层 |
| K-352 | type/dir 解码与 _DIO_* 掩码 | 接口 | 存量 | **20 §1.2** | `com.h`（DEVIO 字段） | 能解析请求 |
| K-353 | CHECK_IO_PORT 与对齐检查 | 约束 | 存量 | **20 §1.2** | `system/do_devio.c:44,62` | 理解权限与 EPERM |
| K-354 | VDEVIO 批量与栈缓冲（VDEVIO_BUF_SIZE） | 机制 | 存量 | **20 §2.3/§3 D5** | `system/do_vdevio.c:67-156`、`syscall_device.rs` | 理解 SMP 安全缓冲 |
| K-355 | SDEVIO 跨进程与 grant/unsafe 双路径 | 机制 | 存量 | **20 §2.4** | `arch/i386/do_sdevio.c:40,96` | 理解安全变体 |
| K-356 | IOPENABLE 提权（IOPL） | 机制 | 存量 | **20 §1.3** | `arch/i386/do_iopenable.c:19-35` | 理解 x86 特权位 |
| K-357 | READBIOS 两段可读范围 | 约束 | 存量 | **20 §1.3** | `arch/i386/do_readbios.c:15-37` | 知道 BIOS 区可读边界 |
| K-358 | PortIo trait 与 MockPortIo | 接口 | 存量 | **20 §3 D2/§4.7** | `os/plat/src/port_io.rs:26` | 看懂跨架构端口 I/O |
| K-359 | 6 dispatcher 接线与非 x86 BadCall | 工具 | 存量 | **20 §4.8/§4.9** | `os/kernel/src/syscall.rs`（X86_64Syscall） | 知道哪些是 x86 特有 |
| K-360 | 语义偏移三处（unknown type / VDEVIO E2BIG / 对齐 EPERM vs panic） | 决策 | 存量 | **20 §4.9** | `20 §4.9` 表 | 知道行为差异 |


### 2.28 去向 = 27-syscall-clock

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-361 | 5 个时钟系统调用总览与权限矩阵 | 概念 | 存量 | **21 §1** | `system/do_{times,setalarm,stime,settime,vtimer}.c` | 知道时间服务接口 |
| K-362 | 三时钟源（monotonic / realtime / boottime） | 概念 | 存量 | **21 §1.1** | `system/do_times.c:40-42` | 不混淆时间语义 |
| K-363 | TIMES 的 SELF/NONE 语义 | 机制 | 存量 | **21 §1.1** | `system/do_times.c:33-35` | 理解查询目标 |
| K-364 | s_alarm_timer 与同步闹钟 | 数据结构 | 存量 | **21 §1.2** | `system/do_setalarm.c:22-64`、`priv.h:48` | 理解 per-priv 闹钟 |
| K-365 | 绝对/相对时间与 time_left 三分支 | 机制 | 存量 | **21 §1.2** | `system/do_setalarm.c:36-62` | 复刻时间参数语义 |
| K-366 | STIME 与 SETTIME 双模式（adjtime） | 机制 | 存量 | **21 §1.3** | `system/do_stime.c:15-19`、`do_settime.c:18-58` | 理解墙钟修改 |
| K-367 | CLOCK_REALTIME 限制与负值保护 | 约束 | 存量 | **21 §1.3** | `system/do_settime.c:25,43` | 知道不可设 monotonic |
| K-368 | VT_VIRTUAL / VT_PROF 两类定时器 | 接口 | 存量 | **21 §1.4** | `system/do_vtimer.c:21-74`、`com.h:420-421` | 理解用户态计时器 |
| K-369 | VtimerType 与 TimerAction::NotifyAlarm | 演进 | 存量 | **21 §3 D2/D3** | `os/kernel/src/syscall_clock.rs` | 看懂 Rust 定时器接口 |
| K-370 | ClockState 参数传递（可测试性） | 决策 | 存量 | **21 §3 D5** | `os/kernel/src/syscall_clock.rs` | 学可测设计 |
| K-371 | TimeStats（virt_left/prof_left 原子递减） | 数据结构 | 存量 | **21 §3 D6/§4.5** | `os/kernel/src/proc.rs` | 理解原子定时器字段 |
| K-372 | caller_has_sys_proc_with_table（fail-closed） | 约束 | 存量 | **21 §4.6** | `os/kernel/src/syscall_clock.rs` | 理解权限检查修正 |
| K-373 | adjtime 保留（POSIX 渐变语义） | 决策 | 存量 | **21 §3 D4** | `system/do_settime.c:29` | 理解为什么不能删 |


### 2.29 去向 = 28-syscall-info-and-misc

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-374 | "未移植"的三种语义（完全/部分/未识别） | 概念 | 存量 | **25 §1.2** | `25 §1.2` 表 | 区分缺口与 stub |
| K-375 | 部分实现 = 前置校验真执行、数据搬运延迟 | 决策 | 存量 | **25 §1.2 注** | `os/kernel/src/misc.rs` | 理解渐进策略 |
| K-376 | 系统调用四类分组（GETINFO/TRACE/UPDATE/SPROF） | 概念 | 存量 | **25 §1.3** | `system/do_{getinfo,trace,update,sprofile}.c` | 杂项归类 |
| K-377 | GETINFO 19 子请求 | 接口 | 存量 | **25 §2.2** | `system/do_getinfo.c:47-228` | 知道内核信息出口 |
| K-378 | GET_WHOAMI 特殊路径 | 机制 | 存量 | **25 §2.2** | `system/do_getinfo.c` | 理解直接写回复 |
| K-379 | 未处理 GET_*（KENV/KADDRESSES/SCHEDINFO/LOCKTIMING/BIOSBUFFER） | 约束 | 存量 | **25 §2.2/§3 D4** | `com.h:316-339` | 知道 EINVAL 来源 |
| K-380 | 长度检查与 E2BIG | 约束 | 存量 | **25 §2.2** | `system/do_getinfo.c` | 复刻长度语义 |
| K-381 | TRACE 13 子请求与 T_SETUSER 保护 | 接口 | 存量 | **25 §2.3** | `system/do_trace.c:20-208` | 理解调试寄存器控制 |
| K-382 | COPYFROMPROC/COPYTOPROC 与段寄存器禁写 | 约束 | 存量 | **25 §2.3/§4.3** | `system/do_trace.c:95-141` | 知道调试能力边界 |
| K-383 | UPDATE 7 步与 proc_is_updatable | 机制 | 存量 | **25 §2.4/§4.4** | `system/do_update.c:37-340` | 理解 RS 热更新 |
| K-384 | inherit_priv_irq/io/mem 与 set_sendto_bit 继承 | 机制 | 存量 | **25 §4.4** | `system/do_update.c:94-107` | 理解资源继承 |
| K-385 | swap_memreq / adjust_priv_slot 七字段 | 机制 | 存量 | **25 §4.4** | `system/do_update.c:292` | 理解槽交换修正 |
| K-386 | SPROF 状态机（START/STOP 与数据搬运） | 机制 | 存量 | **25 §2.5/§4.5** | `system/do_sprofile.c:36-132` | 理解采样控制面 |
| K-387 | BadCall vs ENOSYS 的边界 | 接口 | 存量 | **25 §3.2/§4.6** | `os/kernel/src/syscall.rs:444` | 知道错误码约定 |
| K-388 | krandom 子系统与 GET_RANDOMNESS | 数据结构 | 存量 | **25 §4.7** | `os/kernel/src/krandom.rs` | 理解熵源接口 |
| K-389 | get_randomness 的 read_tsc 架构偏离 | 演进 | 存量 | **25 §4.7 D3** | `os/kernel/src/krandom.rs` | 知道与 C 空体差异 |
| K-390 | GET_MONPARAMS 的 EINVAL 现状 | 工具 | 存量 | **25 §3.3/§4.1** | `system/do_getinfo.c:143` | 知道启动参数缺口 |
| K-391 | do_mcontext / do_setmcontext 语义 | 接口 | 新增 | 旧文仅 08/13/19 片段 | `system/do_mcontext.c:23,74`、`os/arch/src/arch/signal_context.rs`（Mcontext） | 知道机器上下文读写 |
| K-392 | do_diagctl 子命令（STACKTRACE / KPUTS / DIAG / DEBUG） | 接口 | 新增 | 旧文分散在 25/27/32 | `system/do_diagctl.c:18-68` | 知道诊断调用全貌 |
| K-393 | dispatch_unused 死代码清理 | 工具 | 存量 | **25 §4.6** | `os/kernel/src/misc.rs` | 知道待清理项 |


### 2.30 去向 = 29-fpu-context-switching

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-425 | FPU 状态组成（x87 / SSE / 控制状态 / 保存区尺寸） | 数据结构 | 存量 | **31 §1.1** | `include/arch/i386/include/fpu.h:6-48` | 知道要保存什么 |
| K-426 | eager vs lazy 策略取舍 | 决策 | 存量 | **31 §1.2** | `arch_system.c:51-215` | 理解成本转移 |
| K-427 | CR0.TS 与 #NM 机制 | 机制 | 存量 | **31 §1.3** | `exception.c:375-383`、`mpx.S:537-552` | 理解硬件所有权信号 |
| K-428 | fpu_owner 协议（每 CPU 唯一所有者） | 约束 | 存量 | **31 §1.4** | `cpulocals.h`（fpu_owner）、`os/kernel/src/smp.rs:163` | 理解状态不串扰 |
| K-429 | save_local_fpu / save_fpu / restore_fpu | 机制 | 存量 | **31 §2.3-§2.5** | `arch_system.c:88-203` | 理解保存恢复 |
| K-430 | release_fpu 与所有权释放 | 机制 | 存量 | **31 §2.6** | `proc.c:1961-1968` | 理解切换点 |
| K-431 | copr_not_available_handler（lazy 恢复主体） | 机制 | 存量 | **31 §2.7** | `proc.c:1922-1959` | 理解首次使用恢复 |
| K-432 | FpuArch trait 与三架构 State | 接口 | 存量 | **31 §3 D1/§4.1** | `os/arch/src/arch/fpu_arch.rs:59` | 看懂跨架构 FPU |
| K-433 | FpuTrap 分发与恢复失败点 | 接口 | 存量 | **31 §4.4** | `os/arch/src/arch/exception_dispatcher.rs:61,124` | 理解 #NM 接线 |
| K-434 | do_sigsend FPU 保存与 fpu_sigcontext（64 位不恢复） | 约束 | 存量 | **31 §1.6/§4.5** | `system/do_sigsend.c:86`、`do_sigreturn.c:85` | 理解信号帧 FPU 边界 |
| K-435 | D5/D6 缺口（lazy 恢复主体与信号路径） | 工具 | 存量 | **31 §4.5** | `31 §4.5` 表 | 知道当前缺口 |

### 2.31 去向 = 30-stack-tracing

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-436 | 栈回溯三场景（崩溃/诊断/p panic） | 概念 | 存量 | **32 §1.1** | `exception.c:155-170`、`utility.c`（util_stacktrace） | 知道何时用 |
| K-437 | frame-pointer 链（[saved_fp][return_addr] 8 字节对） | 机制 | 存量 | **32 §1.2** | `exception.c:287-373` | 能手工回溯 |
| K-438 | 内核栈 vs 用户栈回溯的读取权限 | 约束 | 存量 | **32 §1.3** | `exception.c`（PRCOPY） | 理解跨空间读 |
| K-439 | KTS 分流与 sp+16 依赖（C） | 机制 | 存量 | **32 §2.1/§1.4** | `exception.c:333-341`、`usermapped_glo_ipc.S` | 理解栈起点差异 |
| K-440 | 循环/截断/故障硬边界 | 约束 | 存量 | **32 §1.5** | `exception.c:287-330` | 防死循环 |
| K-441 | StacktraceArch trait 与 MAX_STACK_FRAMES=32 | 接口 | 存量 | **32 §3 D1/§4.1** | `os/arch/src/arch/stacktrace.rs:41` | 看懂 Rust 回溯 |
| K-442 | proc_stacktrace 公共助手与 cause_signal 接线 | 机制 | 存量 | **32 §4.6** | `os/kernel/src/stacktrace.rs` | 理解统一出口 |
| K-443 | DIAGCTL STACKTRACE 路径 | 接口 | 存量 | **32 §4.5** | `os/kernel/src/syscall.rs`（dispatch_diagctl） | 理解用户请求回溯 |

### 2.32 去向 = 31-kernel-observability

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-444 | runqueues_ok 系列一致性验证 | 机制 | 存量 | **29 §1.1/§4.1** | `debug.c`（runqueues_ok_cpu:14-110）、`os/kernel/src/debug.rs:47,188` | 理解调度队列自检 |
| K-445 | rtsflagstr / miscflagstr | 工具 | 存量 | **29 §2.2/§4.1** | `debug.rs:206,236` | 调试时读位 |
| K-446 | print_proc 与 print_proc_depends（部分 WONTFIX） | 机制 | 存量 | **29 §1.1/§4.2** | `debug.rs:256` | 打印进程详情 |
| K-447 | IPC 消息跟踪与统计 WONTFIX | 决策 | 存量 | **29 §1.2/§3 D3/D4** | `debug.c`（DEBUG_DUMPIPC/DEBUG_IPCSTATS） | 知道为什么不做 |
| K-448 | 条件编译模型 vs Rust 编译期/外部工具替代 | 演进 | 存量 | **29 §1.2/§4.3** | `debug.h:1-93` | 理解调试代码去处 |
| K-449 | 统计 profiling 四组件 | 机制 | 存量 | **30 §1.1** | `profile.c`（init/stop/handler/sample） | 理解采样原理 |
| K-450 | profile_sample 分类（idle/system/user） | 机制 | 存量 | **30 §1.1/§4.2** | `profile.c:75-110`、`os/kernel/src/misc.rs` | 理解样本分类 |
| K-451 | SPROF_SAMPLE_BUFFER（64MB→256KB） | 数据结构 | 存量 | **30 §3 D4/§4.2** | `os/kernel/src/misc.rs` | 知道缓冲边界 |
| K-452 | NMI profiling WONTFIX 与 timer IRQ 替代 | 决策 | 存量 | **30 §3 D3/§4.4** | `profile.c:nmi_sprofile_handler`、`os/kernel/src/clock.rs` | 理解替代方案 |
| K-453 | profile_clock 接口与 hook 链注册 | 接口 | 存量 | **30 §4.1/§4.3** | `os/kernel/src/clock.rs:289-375` | 看懂采样接线 |
| K-454 | C typo 复刻（sprof_proc 容量检查） | 决策 | 存量 | **30 §4.2** | `profile.c:84-89` | 学忠实重写的边界 |
| K-455 | 硬件断点与 debugreg（USE_DEBUGREG） | 工具 | 新增 | 旧文仅 25 §2.3 提及 | `arch/i386/breakpoints.c`、`debugreg.S`、`debugreg.h` | 知道 SYS_TRACE 硬件支持 |
| K-456 | MF_SPROF_SEEN 去重 | 机制 | 存量 | **30 §4.2** | `proc.h:258`、`os/kernel/src/proc.rs` | 理解进程名只存一次 |

### 2.33 去向 = 32-kernel-error-and-shutdown

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-457 | 错误路径三分类（可恢复/挂起/致命） | 概念 | 新增 | 旧文分散于 13/18/24/27 | `os/kernel/src/syscall.rs`（KcallResult）、`errno.rs` | 知道错误该走哪条路 |
| K-458 | errno 对齐原则与 Errno newtype | 约束 | 新增 | 旧文 13 §6.5 片段 | `minix3/minix/include/minix/errno.h`、`os/libs/minix-types/src/types/errno.rs` | 禁止自创错误码 |
| K-459 | panic 三件套（格式化 + 回溯 + 关机） | 机制 | 存量 | **27 §2.2/§4.1** | `utility.c:22-50`、`os/kernel/src/lib.rs`（panic handler） | 理解内核最后防线 |
| K-460 | ARE_PANICING 重入保护 | 机制 | 存量 | **27 §2.2** | `utility.c:17` | 防递归 panic |
| K-461 | kputc 与 kmess 双缓冲 | 数据结构 | 存量 | **27 §2.3** | `utility.c:55-83`、`type.h:173-174` | 理解内核日志历史方案 |
| K-462 | SIGKMESS → log 框架架构演进 | 演进 | 存量 | **27 §3.2/§6.2** | `os/kernel/src/kmess.rs`、`os/plat/src/early_console.rs` | 知道消息去哪 |
| K-463 | _exit 契约守卫（内核不能退出） | 约束 | 存量 | **27 §2.4** | `utility.c:88-93` | 理解 no_std 表达 |
| K-464 | SYS_ABORT / do_abort 与 prepare_shutdown | 机制 | 新增 | 旧文仅 27 §4.1 一行 | `system/do_abort.c:16-29`、`main.c:353-403` | 理解有序关机入口 |
| K-465 | reset/halt/poweroff 与串口输出 | 机制 | 新增 | 无 | `arch/i386/arch_reset.c` | 理解关机硬件路径 |
| K-466 | minix_shutdown 与 halt-loop 现状 | 工具 | 存量 | **27 §6.3/§6.4** | `os/kernel/src/lib.rs`（minix_shutdown） | 知道关机实现状态 |
| K-467 | panic 诊断 hook（D-48） | 演进 | 存量 | **27 §4.1** | `os/kernel/src/lib.rs`（register_panic_diagnostic） | 理解内核如何渲染 panic |
| K-468 | DIAGCTL 诊断子命令（与 26 的分工） | 接口 | 存量 | **27 §2.4/§4.2**、32 §4.5 | `system/do_diagctl.c` | 知道诊断入口 |

### 2.34 去向 = 33-watchdog

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-469 | NMI 不可屏蔽与 lockup 检测原理 | 概念 | 存量 | **26 §1.1** | `watchdog.c`、`arch_watchdog.c` | 理解最后的探针 |
| K-470 | lockup_check（连续 10 次无 tick） | 机制 | 存量 | **26 §2.2** | `watchdog.c:lockup_check` | 理解判定条件 |
| K-471 | NMI handler 双角色（watchdog + profiling） | 机制 | 存量 | **26 §2.3** | `watchdog.c:nmi_watchdog_handler` | 理解入口复用 |
| K-472 | struct arch_watchdog 操作表 | 接口 | 存量 | **26 §2.5** | `watchdog.h:arch_watchdog` | 看懂架构接入 |
| K-473 | USE_WATCHDOG + env 双重门控 | 约束 | 存量 | **26 §1.2** | `main.c:455-456` | 知道默认关闭 |
| K-474 | WONTFIX 五理由（架构覆盖/默认关/PMU 驱动/替代方案/非必需） | 决策 | 存量 | **26 §1.2/§3.1** | `26 §1.2` 表 | 理解决策依据 |
| K-475 | SMP static 变量缺陷（C） | 决策 | 存量 | **26 §6.3** | `watchdog.c:10` | 知道 C 的已知问题 |

### 2.35 去向 = 34-usermapped-data

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-476 | 两种信息暴露策略（共享映射 vs 系统调用） | 决策 | 存量 | **28 §1.1** | `kernel.lds:24-28`、`system/do_getinfo.c` | 理解权衡 |
| K-477 | .usermapped / .usermapped_glo 段 | 数据结构 | 存量 | **28 §1.2** | `kernel.lds:24-28`、`usermapped_data.c` | 知道段里放什么 |
| K-478 | 8 个内核结构体（kinfo/machine/kmessages/loadinfo/kuserinfo/kclockinfo/...） | 数据结构 | 存量 | **28 §2.1/附录 A** | `usermapped_data.c`、`type.h:98-121` | 字段清单 |
| K-479 | 三套 IPC trampoline（softint/sysenter/syscall） | 机制 | 存量 | **28 §1.3/§2.3** | `usermapped_glo_ipc.S:17-104` | 理解用户侧入口 |
| K-480 | arch_phys_map 与 usermapped 分支 | 机制 | 存量 | **28 §2.4** | `memory.c:746-847` | 理解映射建立 |
| K-481 | kuserinfo ABI 与 user_sp | 数据结构 | 存量 | **28 附录 A.1** | `usermapped_data.c`、`kernel_info.rs:43` | 理解用户栈约定 |
| K-482 | MINIX_KERNINFO 半保留（kerninfo page） | 演进 | 存量 | **28 §3.7/§1.1 增补** | `ipcconst.h:12`、`proc.c:685-694`、`os/kernel/src/kerninfo.rs` | 理解 64 位保留部分 |
| K-483 | KERNINFO_USER_VA 与固定映射 | 机制 | 存量 | **28 §3.7/§4.1** | `os/kernel/src/kerninfo.rs:54` | 理解用户可见页 |
| K-484 | ClockState/loadinfo 内部化 | 演进 | 存量 | **28 §3.3/§4.1** | `os/kernel/src/clock.rs` | 知道替代路径 |
| K-485 | kmessages 暴露缺口（future work） | 工具 | 存量 | **28 §4.2** | `28 §4.2` 表 | 知道未闭环项 |

### 2.36 去向 = 35-build-and-test-infra

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-486 | C 内核构建与 unpaged 对象重命名 | 工具 | 新增 | 旧文 02 §2.1 仅提一句 | `kernel/Makefile:15-26`、`arch/i386/Makefile.inc:51-83` | 理解 `__k_unpaged_` 从哪来 |
| K-487 | procoffsets.h 生成（genassym） | 工具 | 新增 | 无 | `arch/i386/Makefile.inc:108-125`、`procoffsets.cf`、`sconst.h:5` | 理解汇编偏移从哪来 |
| K-488 | 链接顺序与 -T（unpaged 段收集规则） | 约束 | 新增 | 无 | `kernel.lds:14-20`、`Makefile:25` | 知道改动为什么会链接失败 |
| K-489 | Rust workspace/crate 分层与 feature（mock/qemu_test） | 工具 | 存量 | **01 §3.8**、06 §5.0 | `os/Cargo.toml`、`os/kernel/Cargo.toml:24-27` | 能定位 crate |
| K-490 | .cargo/config.toml（relocation model / 栈大小） | 约束 | 存量 | **02 §3.2** | `os/.cargo/config.toml:21-34` | 避免构建踩坑 |
| K-491 | xtask 命令面 | 工具 | 新增 | 无 | `os/xtask/src/main.rs:22-144` | 知道构建入口 |
| K-492 | QEMU 测试阶梯与 run_all 脚本 | 测试 | 存量 | **01 §5.2**、05 §5.2、16 §5.1 | `os/qemu-tests/run_all.sh`、`run_qemu.sh`、`test-kernels/` | 能跑一次端到端 |
| K-493 | boot_integration 集成测试 | 测试 | 存量 | **02 §5.4**、29 §5.3 | `os/kernel/tests/boot_integration.rs:14-308` | 理解主机侧集成覆盖 |
| K-494 | mock 平台与测试夹具约定 | 测试 | 存量 | **06 §5.0**、panic-in-drop §4 | `os/plat/src/mock.rs`、`os/kernel/src/test_helpers.rs` | 会写内核单测 |
| K-495 | CI 状态与已知 flake（并行注册表） | 工具 | 存量 | **todo.md §23.3/§25** | `smp_todo.md`、`todo.md` | 知道测试可信度 |

### 2.37 去向 = 36-wire-formats-and-abi

| 编号 | 名称 | 类型 | 来源 | 现有位置 | 锚点 | 读者收益 |
|------|------|------|------|---------|------|---------|
| K-496 | message 64 字节布局与 union | 接口 | 新增 | 旧文散见 12/13/24 | `ipc.h:2403-2675`、`os/libs/minix-types/src/ipc/message.rs:26-62` | 能解析任意消息 |
| K-497 | IPC status 字编码与 MF_REPLY_PEND 门控 | 接口 | 存量 | **23 §2.8**、13 | `ipcconst.h:21-32`、`ipc.h:42-48` | 理解用户态状态回传 |
| K-498 | SYS 号表与 s_k_call_mask 位序（权威对照） | 接口 | 存量 | **13 §2.1/§2.3** | `com.h:205-278`、`os/libs/minix-types/src/ipc/kernel_call.rs:18-177` | 能对照 C/Rust 编号 |
| K-499 | kinfo_t 与 KernelInfo 字段对照 | 接口 | 存量 | **28 附录 A.2**、01 §4.1 | `param.h:14-47`、`kernel_info.rs:13-130` | 改协议时逐字段核对 |
| K-500 | boot_image / boot_module 条目描述 | 接口 | 存量 | **06 §2.3** | `type.h:148-154`、`table.c:44-65`、`kernel_info.rs:285-289` | 理解启动清单线格式 |
| K-501 | grant / umap / vumap 参数线结构 | 接口 | 存量 | **18 §2.1-§2.5** | `ipc.h`（cp_grant_t/vumap 结构）、`os/libs/minix-types/src/ipc/message.rs` | 能构造拷贝请求 |
| K-502 | mcontext 三架构尺寸（824/816/544 字节） | 接口 | 新增 | checklist F-43/F-44 | `sys/arch/x86/include/*`、`os/arch/src/*/signal.rs` | 知道信号帧大小 |
| K-503 | async msg / state table / IPC filter 线格式 | 接口 | 存量 | **12 §3.10**、23 §2.7 | `ipc.h:2745-2761`、`ipc_filter.h:1-73` | 理解扩展协议 |
| K-504 | 对齐与字节序约定（__aligned(16) / little-endian） | 约束 | 新增 | 无 | `ipc.h:2672`、`os/libs/minix-types/src/ipc/message.rs:31` | 避免序列化错误 |
| K-505 | errno 值表与 Rust 映射 | 接口 | 新增 | 散见各 do_*.c | `include/minix/errno.h`、`os/libs/minix-types/src/types/errno.rs`、`os/kernel/src/errno.rs` | 能对照错误码 |

### 2.38 争议归属裁决表（跨篇知识点唯一 owner）

> 本表回答"同一个机制出现在多篇时，完整讲述权归谁"。裁决判据是 §2.0 的 R1-R5；未被列出的知识点不存在归属争议（分组即唯一 owner）。
> 本轮同时取消了旧文档惯用的"第二主讲述点"表述——唯一 owner 之外的篇章只能引用，不能重讲。

| 知识点 | 竞争篇章 | 裁决 owner | 判据 | 落选方案与否决理由 |
|--------|---------|-----------|------|------------------|
| K-093 RTS 16 位全集与不变量 | 07 / 13 / 31 | **07**（定义与不变量） | R2-② | 落选 13：若 13 拥有全集定义，07 无法自足解释进程状态 |
| K-096 与 K-184（RTS 宏与队列联动） | 07 / 13 | **K-096 定义归 07；K-184 联动归 13** | R3 | 落选合并：合并会迫使 13 重复粘贴宏定义 |
| K-101 与 K-395（priv 字段） | 07 / 21 | **K-101 存储布局归 07；K-395 字段语义归 21** | R3 | 落选单归属：07 讲存储不放语义会失真，21 讲语义不带布局会架空 |
| K-195 与 K-306（generation） | 14 / 23 | **K-195 防陈旧语义归 14；K-306 fork 实现归 23** | R3 | 落选 23 全有：寻址语义是 IPC 的前提 |
| K-193~197 endpoint 寻址 | 14 / 07 / 21 / 23 | **14** | R2-①④ | 落选 07：07 只讲 proc_init 如何给出初值，不展开通信语义 |
| K-213/K-217 与 K-498（SYS 号） | 15 / 36 | **15 归一化与空缺槽；36 号表与掩码位序** | R4 | 落选 36 全有：归一化是分派行为，不是线格式 |
| K-216 与 K-397（s_k_call_mask） | 15 / 21 | **15 检查点；21 定义与赋值** | R3 | — |
| K-220 与 K-258/K-259（挂起约定） | 15 / 18 | **15 内核调用挂起；18 跨空间拷贝挂起协议** | R3 | 落选 15 全有：VMREQUEST 是跨空间机制的组成部分 |
| K-233 与 K-258（缺页） | 17 / 18 | **17 触发与转发；18 挂起-恢复协议** | R3 | — |
| K-237/K-248 与 K-346~350（IRQ） | 17 / 26 | **17 机制；26 驱动接口** | R3 | 落选 26 全有：hook 链与控制器语义属异常/中断篇 |
| K-246 与 K-338（cause_sig） | 17 / 25 | **17 触发点；25 完整机制** | R3 | 落选 17 全有：信号路径归属信号篇 |
| K-161 与 K-343（trap_style） | 11 / 25 | **11 机制；25 使用点** | R3 | — |
| K-070 与 K-281（ClockState 与定时器） | 05 / 19 | **05 状态创建；19 运行操作** | R3 | 落选 05 全有：tick 处理属运行期 |
| K-183/K-234 与 K-283（quantum） | 13 / 16 / 19 | **19 归属决策；13 动作；16 调用点** | R3 | 落选单篇：三者分属不同生命周期 |
| K-137/K-140（运行期表初始化） | 09 / 21 / 26 | **09 初始化；使用点留各篇** | R2-② | — |
| K-111 与 K-404/K-405（权限构造） | 07 / 21 | **07 boot 构造；21 运行时模型** | R3 | — |
| K-113 与 K-425（FPU 布局） | 07 / 29 | **07 对象内嵌与尺寸；29 状态语义与切换** | R3 | — |
| K-015/K-128 与 K-499（KernelInfo） | 01 / 08 / 36 | **01 契约；08 单字段语义；36 字段对照** | R3 | — |
| K-103 与 K-500（boot image） | 07 / 36 | **07 清单与流程；36 线格式** | R3 | — |
| K-119/K-122 与 K-143（direct map） | 08 / 10 | **08 建立与 trait；10 协商期属性** | R3 | 落选合并：属性表是 VMCTL 协议的一部分 |
| K-386 与 K-449（SPROF） | 28 / 31 | **28 接口与状态机；31 采样实现** | R3 | — |
| K-212 与 K-496（message） | 14 / 36 | **36 基础布局；14 异步扩展（SENDA）** | R4 | — |
| K-402 与 K-353（I/O 范围） | 21 / 26 | **21 表结构与分配；26 检查点** | R3 | — |
| K-459 与 K-436（panic 与回溯） | 32 / 30 | **30 通用回溯；32 panic 编排** | R3 | — |
| K-052 与 K-462（console 与日志） | 03 / 32 | **03 boot 输出；32 日志演进** | R3 | — |
| K-476~479 与 K-025（usermapped） | 34 / 02 | **34 语义；02 镜像位置** | R2-② | — |

> 自查口径：B 相写每篇前，先确认本篇 K 清单与 §2.38 的 owner 列一致；若发现新的跨篇重名，先登记裁决再写作。

### 2.39 统计摘要

| 指标 | 数值 |
|------|------|
| 知识点总条数 | 505（K-001 ~ K-505，连续无重复） |
| 存量 | 480 |
| 新增 | 25 |
| 新增编号清单 | K-022、K-115、K-129、K-162、K-165、K-288、K-290、K-300、K-313、K-327、K-391、K-392、K-455、K-457、K-458、K-464、K-465、K-486、K-487、K-488、K-491、K-496、K-502、K-504、K-505 |
| 按类型分布 | 机制 172、接口 79、约束 58、数据结构 51、概念 45、决策 45、演进 31、工具 18、测试 6（合计 505） |
| 按去向篇章分布（条目数） | 00:8、01:14、02:14、03:18、04:14、05:9、06:10、07:28、08:14、09:12、10:13、11:12、12:10、13:16、14:20、15:16、16:11、17:16、18:15、19:16、20:14、21:18、22:13、23:16、24:14、25:15、26:15、27:13、28:20、29:11、30:8、31:13、32:12、33:7、34:10、35:10、36:10（合计 505） |

> 复核对账命令（B 相开工前可重跑）：
>
> ```bash
> # 池内条目数（只在 §2 分组表内计数，排除 §2.38 裁决表与 §5 契约表）
> sed -n '/^## 2\./,/^## 3\./p' doc_rerank_deepseek.md | grep -cE '^\| K-[0-9]{3} \|'   # 应为 505
> # 新增条目数（来源列恰为"新增"）
> sed -n '/^## 2\./,/^## 3\./p' doc_rerank_deepseek.md | grep -E '^\| K-[0-9]{3} \|' | grep -c '| 新增 |'  # 应为 25
> # 编号唯一性：每条恰好出现两次（池一次、契约一次）
> grep -oE '^\| K-[0-9]{3} \|' doc_rerank_deepseek.md | sort | uniq -c | awk '$1 != 2'  # 应为空
> ```

---

## 3. 覆盖审计

### 3.0 主题全集与来源

主题全集按四路收集，用来与知识点池、现有文档对账：

1. **C 源码符号**：`kernel/` 顶层 13 个 `.c` + `system/` 38 个 `.c` + `arch/i386/` 21 个 `.c` 中的全部非 static 函数、结构体、宏、状态机、错误路径与架构分支；`include/minix/` 中的常量与线格式头文件。
2. **操作系统通用概念**：进程状态机、地址空间、权限模型、IPC 模型、中断/异常模型、SMP 并发模型、定时器模型、缓存与 TLB 行为、信号模型。
3. **非 C 制品承载的主题**：链接脚本、汇编入口、引导协议、构建系统、测试基建、跨模块线格式（逐项见 §3.4）。
4. **阶段边界契约里属于本 stage 的主题**：`00-master-plan/README.md` 的启动因果链（kernel → VM → RS → …）与 `edge_todo.md` 中 kernel-owned 条目。

### 3.1 覆盖缺口表（主题在全集里但没有任何一篇讲清楚）

| 编号 | 缺口主题 | 证据 | 建议 | 新增知识点 |
|------|---------|------|------|-----------|
| GAP-01 | 内核构建系统（Makefile / unpaged 对象 objcopy 重命名 / 链接顺序 / procoffsets 生成） | `arch/i386/Makefile.inc:48-71,108-125` 不在任何文档的源码行 | **新建 35-build-and-test-infra** | K-486、K-487、K-488 |
| GAP-02 | 跨模块线格式集中说明（message / kinfo / syscall 号 / IPC status / grant / mcontext / errno） | 分散在 12/13/18/22/23/24/25/28，无统一对照 | **新建 36-wire-formats-and-abi** | K-496、K-502、K-504、K-505 |
| GAP-03 | SYS_SCHEDULE（do_schedule）语义 | 仅在 08 §2.1、13 §2.3 的表行出现 | 并入 23-syscall-process §调度控制族 | K-313 |
| GAP-04 | SYS_SETGRANT（do_setgrant）语义 | 仅在 08/13 表行出现 | 并入 24-syscall-copy §授权表登记 | K-327 |
| GAP-05 | SYS_GETMCONTEXT / SYS_SETMCONTEXT（do_mcontext） | 仅在 19 §4.6 的 `Mcontext` 关联类型片段出现 | 并入 28-syscall-info-and-misc §机器上下文 | K-391 |
| GAP-06 | SYS_ABORT / do_abort / reset / halt 的完整关机路径 | 27 §4.1 一行；`arch_reset.c` 零文档 | 并入 32-kernel-error-and-shutdown §关机 | K-464、K-465 |
| GAP-07 | SYS_DIAGCTL 子命令全貌 | 27（kputc 路径）、32（STACKTRACE）各讲一半 | 并入 28-syscall-info-and-misc §诊断调用 | K-392、K-468 |
| GAP-08 | trap 入口汇编与寄存器约定的集中讲述（含 usermapped trampoline、KTS、快速入口瘦帧） | 14 §1.2/附录、13 §1.2、28 §1.3 三处碎片 | **新建 11-trap-entry** | K-155~K-166 |
| GAP-09 | BKL 与 per-CPU 的独立概念篇（旧 16 把并发模型与 AP 启动混在一篇） | 16 全文 1298 行同时承载两主题 | **拆分**：12-concurrency-model + 20-smp-bringup | K-167~K-176、K-287~K-300 |
| GAP-10 | AP early entry 与 ApBootstrap ABI 的完整正文 | 16 §9-§10 仅摘要，详版只在 smp_todo（参考材料） | 并入 20-smp-bringup（新增知识点承载） | K-288、K-290、K-300 |
| GAP-11 | paging_init 语义分解与职责边界 | 仅 07-paging_init_gpt（设计备忘） | 并入 08-cross-space-init §职责边界 | K-129 |
| GAP-12 | Drop 三分类法与测试夹具豁免 | 仅 panic-in-drop（参考材料） | 并入 07-proc-table-init §生命周期纪律 | K-115 |
| GAP-13 | 硬件断点 / debugreg 子系统 | checklist F-46 提及；无正文 | 并入 31-kernel-observability §调试寄存器 | K-455 |
| GAP-14 | NR_BOOT_MODULES 模块数校验 | `main.c:160-162` 无文档 | 并入 01-boot-shim-and-firmware §模块协议 | K-022 |
| GAP-15 | bootstrap 区域释放语义与 Rust no-op 的完整因果 | 01 §3.5.1 有，但缺 C 侧 `cut_memmap` 与模块回收的全链 | 并入 01（与 K-013/K-021 合并讲述） | —（合并） |
| GAP-16 | `do_sdevio` / `do_readbios` / `do_iopenable` 的完整 C 语义 | 20 §1.3 只给范围，未逐步讲 | 并入 26-syscall-device（已有 K-355~K-357） | —（补充展开，不新增条目） |
| GAP-17 | `arch/i386/Makefile.inc` 条件编译（USE_ACPI/USE_APIC/USE_DEBUGREG/USE_WATCHDOG） | 无文档 | 并入 35-build-and-test-infra | —（归入 K-486） |
| GAP-18 | `sys_config.h` 的 NR_PROCS=256 / NR_SYS_PROCS=64 与 `endpoint_t` 上限（31742）的关系 | 06 §2.0/99 各讲一半 | 并入 14-ipc-core §寻址空间与 36 §线格式 | —（已有 K-194/K-098） |

**缺口处理原则**：凡"并入"类缺口，B 相在对应契约的"讲什么"里以知识点编号列出；凡"新建"类缺口，见 §7《缺漏新篇》逐项落实。

### 3.2 重复主题表（同一主题在多篇展开，需指定唯一主讲述点）

| 主题 | 重复位置 | 主讲述点（新） | 其余处理 |
|------|---------|--------------|---------|
| RTS 16 位全集与语义 | 06 §2.1.6、11 §1.1/§2.7、29 §4.1、99 §1 | **13-scheduling-primitives** | 07 只列字段组并指向 13；31 只讲打印；99 撤并为 14 的寻址节 |
| RTS_SET/UNSET 队列联动 | 06 §2.1.6、11 §2.7 | **13 §RTS 联动** | 07 留一句"队列联动见 13" |
| 内核 task 无执行流 | 00 §1.4.1、06 §1.1.4、13 §1.1 | **00-kernel-overview** | 06 保留"阶段 C 如何设置它的 PROC_STOP"；13 只讲权限收紧 |
| 页错误转发 VM | 14 §2.3、24 §1.5 | **17-exception-interrupt**（触发侧） | 18-cross-space-runtime 讲恢复协议侧，引用 17 |
| VMSUSPEND 挂起-恢复 | 13 §1.4、24 §1.2 | **15-syscall-dispatch**（内核调用挂起） | 18 讲跨空间拷贝触发的挂起；13 讲内核调用挂起，双方各自完整、互不复制 |
| clock 初始化与 tick 处理 | 05、15 | **05-clock-init**（初始化）/ **19-clock-timer**（运行期） | 05 不讲 tick 处理；19 不讲 `init_clock` 细节 |
| quantum 递减归属 | 05 §3.7 D9、10 §2.1、15 §1.4/§3 | **19-clock-timer §归属决策** | 05/16 只引用结论，不重复论证 |
| direct map 双窗口 | 07 §1.3/§3.2、09 §1.3、VM 侧 07/08 | **08-cross-space-init** | 10 只讲协商时序中的两个 setcr3 相关点 |
| endpoint 编码/generation | 99、06 §2.1.1、17 §1.2/D2、22 §D2 | **14-ipc-core §寻址** | 07 讲"proc_init 如何初始化 endpoint"；23 讲 fork 如何递增（引用 14） |
| 三层过滤模型 | 22 §1.3、23 全文 | **22-ipc-filter** | 21-privilege 只讲字段定义与掩码赋值 |
| priv 31 字段 | 06 §2.2.0、22 §1.1 | **21-privilege** | 07 只讲存储模型与初始化；B 相以 21 为准 |
| 异常 vs 中断分类 | 05 §1.0、14 §1.1 | **06-interrupt-controller-init**（首次完整） | 17 开篇引用，不重新解释 |
| 内核与 VM 的双窗口/VMCTL | 00 §1.3、09、VM stage | **10-vm-boot-protocol** | 00 只给定位；10 完整展开 |
| C/Rust 差异总表 | 10 §4.5、13 §6、16 §4.14、18 §4.9、20 §4.9、22 §7 | 各篇自留"本篇差异表" | 36 只集中线格式，不集中所有差异 |
| 测试覆盖与计数 | 每篇 Ch5 | 各篇自留测试节 | 35 只讲测试基建与运行方式，不重复各篇清单 |
| DEFERRED 清单 | 17/18/19/20/21/22/23/25/31/32 的各表 | 各篇自留"现状边界"节 | 35 §现状与 §7 汇总跨篇缺口；过期条目按 §3.5 删除 |

### 3.3 越界主题表（某篇讲了声明边界之外的主题）

| 旧位置 | 越界内容 | 正确归属 |
|--------|---------|---------|
| 03 §1.8/§4.3 | TrapReturnArch 设计评估（属 trap 返回） | 11-trap-entry |
| 05 §4.1/§4.3-§4.12 | ClockState 定时器队列/tick 语义/GIC/PLIC 编程 | 19-clock-timer（tick）、06-interrupt-controller-init（GIC/PLIC） |
| 06 §2.2.3/§4.6 | priv 运行时字段语义与 set_sendto_bit | 21-privilege |
| 07 §2.4-§2.5 | VM 侧页目录槽位发号线与 createpde 使用 | 08-cross-space-init（保留为历史对照）；VM 细节归 02-stage-vm |
| 08 §4.1-§4.5 | SYS_* 常量全表与 Syscall enum | 15-syscall-dispatch（常量表）、36（线格式） |
| 09 §4.8/§4.9 | ptproc/root phys 运行时跟踪细节 | 20-smp-bringup（per-CPU 部分）与 08（boot 根） |
| 12 §3-§4 | Rust IPC 实现的完整 API（超过"核心机制"） | 14-ipc-core 保留一份精简的"Rust 表达"节，详细 API 由代码承担 |
| 14 §3.2/§4.3 | FaultContext/RecoveryPoint 的完整设计推演 | 17-exception-interrupt 保留结论，过程归 20-redesign |
| 15 §3-§4 | syscall_clock 的适配细节 | 27-syscall-clock |
| 16 §9-§10 | AP early entry/固件 ABI | 20-smp-bringup |
| 19 §4.6.1 | trap_style 机制全述 | 11-trap-entry |
| 20 §4.7-§4.9 | PortIo/anti-translate 汇总 | 26-syscall-device 保留本篇差异表；通用表达规则归 35 |
| 22 §4.6.1 | seL4/Redox/Linux 对照与 boot 路径 D-58 | 21-privilege 保留一段对照（教学价值），过程记录归档 |
| 25 §4.7 | krandom 的完整实现 | 28-syscall-info-and-misc 保留；熵源理论归 20-redesign |
| 28 §3.7/§4.1 | MINIX_KERNINFO 页与 kerninfo.rs | 34-usermapped-data 保留（已是主讲述点） |
| 29 §3-§4 | profile 采样实现（属性能工具） | 31-kernel-observability（合并后不再越界） |
| 31 §2 | FPU 信号保存与 sigcontext 布局 | 29-fpu-context-switching 保留约束，布局归 36 §线格式 |
| 32 §2.5 | panic 路径回溯的完整实现 | 30-stack-tracing 保留机制，panic 编排归 32 |

### 3.4 非 C 主题逐项回答（固定清单，不许留空）

| 主题 | 在哪里讲（新篇章） | 理由/边界 |
|------|------------------|----------|
| 链接与加载 | **02-kernel-image-and-higher-half**（kernel.lds、ELF 加载、HigherHalf）；**35**（链接顺序与构建） | 02 讲语义，35 讲怎么构建 |
| 镜像与内存布局 | **02**（VMA/LMA、unpaged、usermapped、初始栈）；**01**（物理 memmap）；**08**（direct map 窗口） | 物理布局与虚拟布局分开讲 |
| 汇编入口与陷阱进入 | **11-trap-entry**（head.S 之后的全部进入路径、trap frame、返回指令、寄存器约定、trampoline） | 集中一处，避免 14/13/28 三处碎片 |
| 启动装配 | **01-10 全链**（固件→boot-shim→ELF→保护结构→平台发现→时钟/中断→进程表→跨空间→系统初始化→VM 协商） | 每篇一个装配阶段 |
| 构建与工具链 | **35-build-and-test-infra**（C Makefile / objcopy / genassym；Rust Cargo / xtask / .cargo 配置） | 新建篇章 |
| 跨模块接口与线格式 | **36-wire-formats-and-abi**（message/kinfo/syscall 号/IPC status/grant/mcontext/errno/对齐） | 新建篇章；endpoint 见 14，VMCTL 线格式见 10 |
| 错误路径 | **32-kernel-error-and-shutdown**（分类原则、errno、panic、关机）；各篇自带错误表 | 原则集中，细节就近 |
| 关闭与退出 | **32**（panic/abort/reset/halt）；**23-syscall-process**（进程 exit/clear） | 内核终止与进程终止分开 |
| 并发与同步 | **12-concurrency-model**（BKL/per-CPU/上下文约束）；**20-smp-bringup**（跨 CPU 机制） | 抽象与启动分开 |
| 测试基建 | **35**（QEMU 阶梯、boot_integration、mock、CI）；各篇自带验收用例 | 基建集中，用例就近 |

### 3.5 明确删除表（不入新目录，B 相归档时整体保留旧文，不迁移内容）

| 编号 | 删除内容 | 现有位置 | 理由 |
|------|---------|---------|------|
| DEL-01 | 32 位启动路径对比（BIOS/GRUB legacy/earm head.S 附录） | 01 附录 A.0、02 §2.2 对比 | 三架构目标下无教学与实现价值；riscv/arm 对照保留在 02 正文 |
| DEL-02 | T0-T6 私有阶段编号 | 08 §1.1 及多处 | 与代码里的 Phase A-F 冲突；统一用 A-F |
| DEL-03 | 虚构机制（trait PageFaultHandler、do_unused、"timer_tick 递减 quantum"旧说法、虚构的 `lib.rs:2583` 占位循环描述） | 14 §4.6 注、25 §4.6、15 §3 旧版 | 已被代码事实证伪；删除并在对应篇加"常见误解"一句（可选） |
| DEL-04 | 已解除的 DEFERRED 表（17 §4.8、18 §4.8、19 §4.7、20 §4.8、21 附录 B、23 Ch5/Ch6、25 附录） | 各篇 | 状态与代码矛盾；B 相以代码为准重写"现状边界"节 |
| DEL-05 | 测试计数与文件行数快照（"24 个测试"、"641 行"等） | 17/18/19/20/21/22/25/29/32 头部 | 系统性漂移；新契约要求不放计数，改为可复跑的验收命令 |
| DEL-06 | 06-todo 的执行记录、对话摘要、逐项行号漂移清单 | 06-todo §4-§附 | 过程材料；归档，只保留职责边界矩阵结论迁入 07 契约 |
| DEL-07 | smp_gpt / v2 / v3 的三轮评审问答 | 三份参考材料 | 输入历史；结论已并入 smp_todo 与 20 契约 |
| DEL-08 | checklist 的 P0/P1/P2 历史账本与验证命令旧版 | checklist §7-§9 | 历史态；机制类结论（CHECK_IPC 幻影、mcontext 尺寸）已入池 |
| DEL-09 | endpoint_todo 的方案 A/B/C 成本对比 | endpoint_todo §5-§8 | 未采纳的设计讨论；如需立项属 20-redesign，不进本 stage |
| DEL-10 | "工具生成"锚点标记（`（Lxxx，工具生成）`） | 全部文档 | 锚点已失真且误导；新契约要求锚点写符号名，行号可选 |
| DEL-11 | 各篇的"本章不讲什么"清单在重建后重复展开的旧叙述 | 全部文档 | 新目录按单一语义重划边界，旧边界声明作为契约输入（§5），不逐句迁移 |
| DEL-12 | 与 02-stage-vm 的重复实现叙述（VM 侧 page table 实现细节） | 07 §2.4-§2.5、09 §4.9 | 归 02-stage-vm；kernel 侧只留接口与协作点 |

> 删除不等于丢弃：旧文档整体归档（B 相不删文件），上表只界定"哪些旧内容不进入新正文"。

---

## 4. 新目录

### 4.1 设计取舍（为什么是这个形状）

1. **单一语义优先于继承旧编号**。旧目录的编号是历史产物：01/02 拆得动、05/16 混得重、10-14 顺序反直觉、26-32 是补丁式追加。新目录按"读者学会一个概念所需的完整语境"重划，允许与旧编号大面积错位。
2. **启动链保持线性，运行期按"一次陷入的生命周期"组织**。内核没有服务式主循环，但每一次被进入都走同一条路：进入（11）→ 并发纪律（12）→ 调度状态机（13）→ 通信（14）→ 分派（15）→ 退出与再调度（16）→ 异常/中断（17）→ 跨空间恢复（18）→ 时钟 tick（19）→ 多核（20）。这条顺序让每一篇都能只依赖更早的篇章。
3. **系统调用族独立成部**。58 个调用是并行集合，按"框架 → 族 → 差异表"组织，避免把 handler 细节塞进核心机制篇。
4. **基础设施与工具成支线**。FPU、回溯、观测、错误/关机、watchdog、usermapped、构建、线格式互不依赖，集中成组，允许跳读。
5. **为 C 之外的主题新建两篇**（35 构建与测试、36 线格式），并把散落的 trap 入口集中成 11。

### 4.2 新篇章总表

| 新编号 | 标题 | 一句话定位 | 分组 |
|--------|------|-----------|------|
| 00 | kernel-overview | 内核是什么、怎么运行、怎么读这套文档 | 总览 |
| 01 | boot-shim-and-firmware | 固件把机器交给 boot-shim，直到 `arch_boot()` 被调用 | 启动链 |
| 02 | kernel-image-and-higher-half | 内核镜像如何被装载并切到高地址运行 | 启动链 |
| 03 | protection-init | 建立 GDT/IDT/TSS 与内核栈，让 CPU 有可信上下文 | 启动链 |
| 04 | platform-discovery | 不硬编码地址，从 ACPI/DTB/兜底描述中拿到硬件参数 | 启动链 |
| 05 | clock-init | 时钟软件状态与三架构时钟源就绪 | 启动链 |
| 06 | interrupt-controller-init | 中断控制器就绪，内核准备接收异步事件 | 启动链 |
| 07 | proc-table-init | 把 boot image 实例化为进程表（阶段 C） | 启动链 |
| 08 | cross-space-init | 内核获得访问任意物理页的能力（阶段 D） | 启动链 |
| 09 | system-init-and-boot-finish | 注册系统调用、回收 bootstrap、解除停止、进入调度 | 启动链 |
| 10 | vm-boot-protocol | VM 接手页表，系统进入运行态 | 启动链 |
| 11 | trap-entry | CPU 如何进入内核、上下文如何保存与返回 | 运行骨架 |
| 12 | concurrency-model | BKL 与 per-CPU：内核的并发纪律 | 运行骨架 |
| 13 | scheduling-primitives | RTS 状态机与就绪队列如何保持一致 | 运行骨架 |
| 14 | ipc-core | endpoint 寻址与六原语：消息如何中转、阻塞、唤醒 | 运行骨架 |
| 15 | syscall-dispatch | 系统调用如何被路由、挂起与恢复 | 运行骨架 |
| 16 | switch-to-user | 调度循环：内核唯一的出口 | 运行骨架 |
| 17 | exception-interrupt | 异常与中断进入内核后发生什么 | 运行骨架 |
| 18 | cross-space-runtime | 内核代行缺页时的挂起-通知-恢复协议 | 运行骨架 |
| 19 | clock-timer | 每个 tick 内核做什么，quantum 为什么不在 tick | 运行骨架 |
| 20 | smp-bringup | AP 如何启动、IPI 如何协同、跨 CPU 调度如何工作 | 运行骨架 |
| 21 | privilege | priv 结构与权限模型，SYS_PRIVCTL 如何改权限 | 权限与系统调用 |
| 22 | ipc-filter | 三层过滤：谁能发给谁、接收方收不收 | 权限与系统调用 |
| 23 | syscall-process | 进程生命周期与控制类调用（fork/exec/exit/clear/runctl/schedctl/statectl/schedule） | 系统调用 |
| 24 | syscall-copy | 跨空间拷贝与授权（vircopy/safecopy/grant/umap/vumap/memset） | 系统调用 |
| 25 | syscall-signal | 信号中介：内核挂起、管理器拉取、用户态处理器往返 | 系统调用 |
| 26 | syscall-device | 驱动如何收中断、读写端口（irqctl/devio/sdevio/vdevio/iopenable/readbios） | 系统调用 |
| 27 | syscall-clock | 时间查询、闹钟、墙钟、虚拟/性能定时器 | 系统调用 |
| 28 | syscall-info-and-misc | 信息查询、追踪、热更新、采样、机器上下文、诊断与未移植语义 | 系统调用 |
| 29 | fpu-context-switching | FPU 状态的所有权与 #NM 惰式切换 | 基础设施 |
| 30 | stack-tracing | 帧指针链回溯与跨空间读取 | 基础设施 |
| 31 | kernel-observability | 调度队列自检、进程打印、统计采样、硬件断点 | 基础设施 |
| 32 | kernel-error-and-shutdown | 错误分类、panic、日志与有序关机 | 基础设施 |
| 33 | watchdog | NMI lockup 检测（WONTFIX 文档化） | 基础设施 |
| 34 | usermapped-data | 用户可见内核数据的 .usermapped 段与 64 位替代 | 基础设施 |
| 35 | build-and-test-infra | 内核如何构建、如何被测试 | 基础设施 |
| 36 | wire-formats-and-abi | 跨模块线格式与 ABI 集中对照 | 基础设施 |

共 37 篇（00-36）。篇幅按内容需要定量：单篇目标 600-1500 行；07（进程表全集）、36（线格式对照）允许更长；旧 01（2019 行）拆走 ELF 后目标 ≤1200 行。

### 4.3 阅读路径

**主线（第一次读，按序）**：00 → 01 → 02 → 03 → 04 → 05 → 06 → 07 → 08 → 09 → 10 → 11 → 12 → 13 → 14 → 15 → 16 → 17 → 18 → 19 → 20 → 21（权限）→ 22（过滤）。

**系统调用支线（按需要读）**：21-privilege 与 22-ipc-filter 定义所有 handler 的权限与过滤语境，应在进入族篇前读；之后 23-28 任意族可独立进入，每族只依赖 00-22 的核心篇。

**基础设施支线（可跳读）**：29-36。其中 35（构建与测试）适合开读之前先看；32（错误与关机）建议在 11 之后随时读；34（usermapped）与 36（线格式）可在读 10/14 时对照。

**二次阅读（按机制串联）**：
- 追一次 IPC 请求：11 → 14 → 15 → 13 → 16 → 10。
- 追一次缺页：17 → 18 → 10 → 24。
- 追一次时钟：05 → 06 → 19 → 27 → 13。
- 追一次进程出生：07 → 10 → 23 → 13 → 16。
- 追一次多核事件：12 → 20 → 13 → 16。

**已声明不做（WONTFIX）**：33（NMI watchdog 整体）、31（IPC dump/统计）、34（.usermapped 大部分）、29（lazy-restore 主体与信号保存两处缺口）、36（无）。这些篇章显式标注"文档化但不实现"，不阻塞主线。

### 4.4 序差表（教学序 vs 运行时序）

| 序差编号 | 运行时序事实（带锚点） | 教学序选择 | 理由 | 回指补偿 |
|---------|----------------------|-----------|------|---------|
| SEQ-01 | 时钟中断在 `boot_cpu_init_timer`（`main.c:73`）之后就可能到来，而第一次 `switch_to_user` 在同一函数末尾 | 19-clock-timer 排在 16-switch-to-user 之后 | 先用 11-16 建好"进入-调度-退出"骨架，19 才能讲"tick 触发其中的 quantum 检查" | 09 明确"时钟启动后、调度前可能已有 tick；此阶段 tick 只记账" |
| SEQ-02 | 异常/缺页在启动期就可能发生（`exception.c:180` 对内核态直接 panic） | 17-exception-interrupt 排在启动链之后 | 启动期异常路径是 panic 直通，无需教程；运行期异常才需要分流知识 | 03/11 各留一句"启动期异常 = panic，运行期分流见 17" |
| SEQ-03 | IRQ 在 06 初始化后即可到达，`put_irq_handler` 在 09/19/20 才注册 | 06 只讲控制器与 mask 语义，handler 链在 17 讲，注册使用在 19/20 | 先"能收"，后"谁来处理" | 06 结尾给出"中断入口之后的路径见 17" |
| SEQ-04 | `system_init`（`main.c:295`）在 `bsp_finish_booting`（`main.c` 38）之前，而 `switch_to_user` 在最后 | 09 一篇内按真实顺序讲完三者 | 同一阶段不收尾无法理解 | 09 §时序图给出精确节拍 |
| SEQ-05 | VMCTL 协商由运行后的 VM 发起（`do_vmctl` 在调度循环内被调用） | 10-vm-boot-protocol 排在 16-switch-to-user 之前 | 协商是"启动的完成条件"（VMINHIBIT 解除），放启动链末尾才闭合因果 | 10 开头声明"VM 被首次调度后才发起，本协议时序跨 09 与 16"；16 引用 10 |
| SEQ-06 | `delivermsg` 的调用点在 `switch_to_user`（`proc.c:363-367`），但投递语义在 IPC | 14-ipc-core 讲投递协议，16 讲调用点 | IPC 完整性优先，调用点属于调度循环 | 14 结尾标注"调用点在 16 §misc flags"；16 只引用不重讲 |
| SEQ-07 | BKL 在启动早期已取（`main.c:149`） | 12-concurrency-model 排在 11 之后 | 先理解"进入内核"，再理解"进入后的互斥纪律" | 12 开头声明"BKL 实际在 09 阶段已取，概念在此首次完整" |
| SEQ-08 | per-CPU 数据在 SMP 启动（20）才出现第二个副本 | 12 讲 per-CPU 模型，20 讲多副本启动 | 概念与启动分离 | 12 结尾给出"SMP 关闭时退化为单副本；AP 启动见 20" |
| SEQ-09 | 调度原语必须先于 IPC 阻塞（RTS 位语义） | 13 在 14 之前 | 阻塞/唤醒是状态机的操作 | 14 引用 13，不重述队列机制 |
| SEQ-10 | 异常处理实际先于系统调用被使用（启动期 trap） | 17 排在 14/15/16 之后 | 处理逻辑依赖调度与 IPC（cause_sig 通知、页错误 send VM） | 17 开头依赖声明；11 只讲进入不讲处理 |

### 4.5 并行主题的组织（集合型部分）

系统调用族按"汇聚点 + 触发时机"组织：

- **统一框架**（23 之前已就绪）：15-syscall-dispatch（分派）、21-privilege（权限）、22-ipc-filter（过滤）、36（线格式）。
- **族分组**：进程与生命周期（23）、内存与授权（24）、信号（25）、设备与 I/O（26）、时间（27）、信息与杂项（28）。
- **代表讲透、其余差异表**：
  - 23 以 fork 与 clear 为代表（同步点 + 幂等回收两种典型），exec/exit/runctl/schedctl/statectl/schedule 用差异表收束。
  - 24 以 safecopy + grant 验证链为代表（最复杂的权限路径），vircopy/umap/memset 用差异表收束；vumap 单列批量 DMA 一节。
  - 26 以 irqctl + devio 为代表，sdevio/vdevio/iopenable/readbios 用差异表收束。
  - 28 以 GETINFO 为代表（子请求最多的接口），trace/update/sprofile/mcontext/diagctl 分组。
- **主线与支线**：21/22 是"每个 handler 都要懂"的主线前置；28 属杂项支线；33/34/31 属可跳读支线。

### 4.6 顺序推导：被选中的顺序与备选序否决

**被选中的主轴**（编号序）：启动链 01-10 → 运行骨架 11-20 → 权限与过滤 21-22 → 系统调用族 23-28 → 基础设施 29-36。

**运行骨架内部的因果链**（11 → 20，箭头即依赖）：

```text
11 入口（CPU 如何进来、上下文在哪里）
  → 12 并发纪律（进来之后，改共享状态的红线）
  → 13 调度状态机（RTS 位与就绪队列）
  → 14 通信（endpoint 寻址与六原语；阻塞/唤醒就是 13 的状态操作）
  → 15 分派（系统调用如何被路由、挂起、恢复）
  → 16 退出（调度循环：misc flags、延迟投递、终局恢复）
  → 17 异常/中断（另一类入口的处理：缺页转发、IRQ 唤醒、信号产生）
  → 18 跨空间恢复（内核代行缺页时的挂起-通知-恢复）
  → 19 时钟（tick 四类工作；quantum 递减其实在 16 的出口）
  → 20 多核（AP 启动与跨 CPU 协同）
```

**备选序与否决理由**：

| 备选序 | 形状 | 否决理由 |
|--------|------|---------|
| A 入口优先 | 11 入口 → 17 异常/中断 → 19 时钟 → … | 异常处理依赖调度与 IPC（cause_sig 要通知管理器、缺页要 send VM），会把前向引用搬到更前面；启动期异常是 panic 直通，教学收益低 |
| B 出口优先（旧目录 10-14 的形态） | switch_to_user → 调度 → IPC → 分派 → 异常 | switch_to_user 是汇聚点，先讲它必须提前解释 pick_proc / delivermsg / proc_no_time / 五阶段全部构件，等于把 13-15 的结论悬空使用 |
| C 服务式请求生命周期 | 进入 → 分派 → 处理 → 回复 一条主线 | 内核不是服务：回复不经"服务循环"而经调度；已部分采纳为 §1.2 B 段真序表，但不作为篇章主轴 |
| D 集合优先 | 先讲系统调用族，再讲核心机制 | 全部 handler 依赖 BKL / RTS / IPC / 分派，前向引用爆炸 |
| E 逐 C 文件顺序 | 按 `kernel/` 目录文件逐个讲 | 与运行时序、认知序都无关（`main.c` 的函数定义顺序本身就是颠倒的） |
| F 并行体线性化 | 把 58 个系统调用按编号 0-57 顺序讲 | 违反集合型组织规则（需先框架、再分组、组内代表加差异表） |

**排序三判据**：① 依赖闭包最小（当前编号序 = 拓扑序，见 §9.1 检查二）；② 运行时因果优先（启动链严格按 §1.2 A 段真序）；③ 认知递进（先具体后抽象、先现象后机制）。

**与运行时序不一致的十处**：见 §4.4《序差表》，每条都有运行时事实、教学序理由与回指补偿。

### 4.7 篇幅与拆篇规则

- **篇幅目标**：单篇正文 500-1500 行；单概念极复杂可到 3000 行（soft 上限）；低于 300 行的短篇允许存在（不为凑篇幅合并）。
- **拆篇触发器**（B 相执行；触发时更新编号并在 STATE 记录）：
  1. 单篇出现两个可独立阅读的语义单元，且各自预估超过 800 行（候选：28 的 GETINFO 与 trace/update/sprofile 两半；36 的线格式若按架构膨胀）；
  2. 单篇预计超过 3000 行且没有自然小节边界；
  3. 单篇不足 300 行且与相邻篇属于同一阶段（候选：05 与 06 可并为"早期硬件初始化"，编号合并后顺延）。
- **拆/合后的编号规则**：新篇插在被拆篇之后，后续编号顺延；引用迁移按 §8.2 批量替换；本蓝图不预拆，只给出触发器。
- **B 相预算提示**：07（28 知识点，最大）、14（20）、21（18）、03（18）、28（20）是重篇，写作前先列内部小节预算；30（8）、33（7）、05（9）、12（10）、06（10）是短篇，可快速通过。

---

## 5. 每篇契约

> 契约格式七要素：一句话定位 / 讲什么 / 不讲什么（含去向）/ 前置 / 后置 / 事实底线 / 知识点清单 + 验收标准。
> "前置"只允许指向更小新编号；跨 stage 依赖显式标注 stage 名。
> 知识点清单的"来源"列：存量填旧文档位置，新增填证据锚点（C 源码、非 C 制品或操作系统理论）。
> 取料规则（R-GT1~R-GT3，§0.4）：先按锚点读代码，再写正文；旧文档只提供知识点清单与线索，不作为断言依据；冲突时以 `minix3/` C 源码为准。

### 00-kernel-overview

- 一句话定位：让第一次读内核的人建立"内核是什么、怎么被进入、这套文档怎么读"的完整心智模型。
- 讲什么：K-001~K-008；全局术语表（endpoint、RTS、BKL、kernel task、IDLE、boot image、VMINHIBIT、direct map）；阅读路径（主线/支线/二次阅读）；Rust 表达总则（no_std、BKL、trait 硬件抽象、错误码对齐）。
- 不讲什么：任何具体机制的实现细节，交给 01-36；VM/PM/VFS 的内部工作，交给各自 stage；引导加载器与固件规范，见 01 的边界声明。
- 前置：无。
- 后置：全部篇章引用本篇的术语与阅读路径。
- 事实底线：
  - C：`table.c:44-51`（boot_image 五项内核任务）、`com.h:47-56`（负 endpoint 与 NR_TASKS=5）、`main.c:64-66,269`（kernel task 的 PROC_STOP 恒置）、`proc.c:176-229`（idle）、`proc.c:299-474`（switch_to_user）、`mpx.S:273-337`（三类入口）。
  - Rust：`os/kernel/src/lib.rs`（模块表）、`os/arch/src/arch/mod.rs:5-31`（trait 清单）、`os/libs/minix-types/src/types/endpoint.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-001 | 内核职责边界 | 概念 | `table.c`、`main.c:115` | 开篇必须回答"内核做什么" | 存量 00 §1.2 |
| K-002 | 三维运行态实体 | 概念 | `table.c:44-51`、`com.h:47-56` | 全 stage 最核心的心智模型 | 存量 00 §1.4.1 |
| K-003 | kernel task 无执行流 | 机制 | `main.c:64-66,269`、`protect.c:396-402` | 纠正常见误解 | 存量 00 §1.4.1 |
| K-004 | 无主循环：三类入口一出口 | 概念 | `mpx.S:273-337`、`proc.c:299-474` | 决定整套文档的组织方式 | 存量 00 §1.4 |
| K-005 | 与 VM 的策略/机制分工 | 概念 | `system/do_vmctl.c` | 理解跨主体页表管理 | 存量 00 §1.3 |
| K-006 | Rust 表达约束 | 约束 | `os/kernel/src/lib.rs` | 阅读时知道哪些是硬约束 | 存量 00 §1.5 |
| K-007 | context 与 flow 的区分 | 概念 | `proc.h:23-24`、`proc.c:176` | 解释 IDLE 特殊性 | 存量 00 §1.4.1 |
| K-008 | 最先运行、最后停止 | 概念 | `main.c:115`、`utility.c:22` | 定位内核 | 存量 00 §1.1 |

- 验收标准：读者合上本篇能回答——内核和用户态服务的执行模型差别是什么；CLOCK/SYSTEM 为什么不是线程；内核被进入的三种方式各是什么；全文 12 个术语表条目都能用一句话解释。必须出现的图：三维实体对照表、三类入口一出口图、阅读路径图。

### 01-boot-shim-and-firmware

- 一句话定位：讲清上电之后到内核 `kmain` 之前，机器状态如何被收集成一份可信的启动信息。
- 讲什么：K-009~K-022。Multiboot 与 boot.cfg；`get_parameters` 与 memmap 记账；KernelInfo 契约与字段；boot-shim 的 UEFI 与 OpenSBI 两条路径；boot 模块校验；balloc/pt_alloc；三层 crate 依赖；bootstrap 区间语义。
- 不讲什么：内核 ELF 的段拷贝与高半跳转（交给 02）；页表与 CR3（02/08）；保护结构（03）；kinfo 字段在进程表初始化中的消费（07）；UEFI 协议规范细节（只给"够用"的最小概念，不展开规范）。
- 前置：00。
- 后置：02（消费 KernelInfo 继续启动）、07（消费 boot_procs 与 memmap）、35（构建 boot-shim 与 kernel）。
- 事实底线：
  - C：`arch/i386/pre_init.c:94-171,194-214,217-238`；`arch/i386/pg_utils.c:32,65,86-121,162,186,204,247`；`arch/i386/head.S:36-77`；`include/minix/param.h:9,14-47`；`sys/arch/i386/include/multiboot.h:40-259`；`minix3/etc/boot.cfg.default`；`com.h:74`（NR_BOOT_MODULES=12）。
  - 非 C 制品：`os/boot-shim/src/{main.rs,loader.rs,uefi_helpers.rs,opensbi_helpers.rs}`；`os/libs/minix-boot/src/{kernel_info.rs,boot_shim.rs,platform.rs}`；`os/arch/src/arch/{paging.rs,paging_ext.rs,pt_alloc.rs}`；`os/kernel/src/boot_alloc.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-009 | Multiboot 协议 | 接口 | `head.S:43-66`、`multiboot.h:40-259` | 内核的第一份外部契约 | 存量 01 §2.0.1 |
| K-010 | head.S 第一跳板 | 机制 | `head.S:68-77` | 内核第一条指令 | 存量 01 §2.0.2 |
| K-011 | boot.cfg 与模块指令 | 工具 | `boot.cfg.default:1-8` | 能配置启动 | 存量 01 §2.0.3 |
| K-012 | get_parameters | 机制 | `pre_init.c:94-171` | 启动参数来源 | 存量 01 §2.3.2 |
| K-013 | memmap 记账 | 机制 | `pg_utils.c:32,65,86-121` | 可用内存的由来 | 存量 01 §2.3.3 |
| K-014 | 重叠检查 | 机制 | `pre_init.c:77-92` | 加载器不保证不重叠 | 存量 01 §3.5 |
| K-015 | KernelInfo 契约 | 接口 | `kernel_info.rs:13-130` | 唯一交接面 | 存量 01 §4.1 |
| K-016 | BootShim trait | 接口 | `boot_shim.rs:20-58` | 两种固件的抽象 | 存量 01 §4.4 |
| K-017 | UEFI 替代 Multiboot | 演进 | `uefi_helpers.rs` | 理解代际迁移 | 存量 01 §3.1 |
| K-018 | OpenSBI BootFileTable | 接口 | `opensbi_helpers.rs:193-434` | riscv64 路径 | 存量 01 §4.5.2 |
| K-019 | pt_alloc / BootAlloc | 机制 | `pt_alloc.rs`、`boot_alloc.rs` | boot 期分配 | 存量 01 §4.3.1 |
| K-020 | 三层 crate 依赖 | 工具 | `os/*/Cargo.toml` | 改动的边界 | 存量 01 §3.8 |
| K-021 | bootstrap 区间语义 | 决策 | `pre_init.c:230-234` | 回收守卫 | 存量 01 §3.5.1 |
| K-022 | 模块数校验 | 约束 | `main.c:160-162`、`com.h:74` | 启动完整性 | 新增 |

- 验收标准：读者能画出 mbi → kinfo → KernelInfo 的字段传递图；能解释 memmap 切除为什么要拆区间；能说出 UEFI 与 OpenSBI 两条路径唯一必须分叉的两个点（读文件方式、引导协议魔数）；能对 NR_BOOT_MODULES 校验与 overlaps 检查各给一个反例。必须出现的对比：C `pre_init` 与 Rust `UefiBootShim/OpenSbiBootShim` 的职责对照表。

### 02-kernel-image-and-higher-half

- 一句话定位：内核 ELF 怎么被装进内存、怎么在不搬代码的前提下开始在高地址运行。
- 讲什么：K-023~K-036。kernel.lds 的 VMA/LMA 与 AT()；unpaged 段与 usermapped 段的位置；head.S 的栈切换与跳转；Rust 的 link.ld、minix-elf、FileLoader、HigherHalf trait；`arch_boot_impl` 的五个步骤；kern_virt≠kern_phys 守卫；riscv64 L2 覆盖 bug 复盘。
- 不讲什么：memmap 与模块加载（01）；保护结构初始化（03）；direct map 覆盖的建立细节（08，本篇只给建立点）；VM 的 `map_kernel`（10 与 VM stage）。
- 前置：01。
- 后置：03（高地址下继续初始化）、08（arch_boot Step 4 的 DM 建立在此铺垫）、35（链接与构建）。
- 事实底线：
  - C：`arch/i386/kernel.lds:2-36`；`arch/i386/head.S:36-96`；`arch/i386/pg_utils.c:162,186,204`；`arch/i386/protect.c:360-363`（第二次建表）。
  - 非 C 制品：`minix3/minix/kernel/arch/i386/kernel.lds`；`os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld`；`os/kernel/src/boot/{mod.rs,higher_half.rs}`；`os/boot-shim/src/loader.rs:58-253`；`os/libs/minix-elf`；`os/kernel/src/lib.rs:291`（arch_boot_impl）；`os/.cargo/config.toml:33-34`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-023 | VMA/LMA 与 AT() | 接口 | `kernel.lds:4-6,29-34` | 镜像布局的地基 | 存量 02 §2.1 |
| K-024 | unpaged 段 | 决策 | `kernel.lds:14-20` | 解释前缀与恒等要求 | 存量 02 §2.1 |
| K-025 | usermapped 段位置 | 数据结构 | `kernel.lds:24-28` | 段布局完整 | 存量 02 §2.1 |
| K-026 | 中间态与成对性 | 约束 | `head.S:78-87` | 高半跳转的核心约束 | 存量 02 §1.1 |
| K-027 | 两个映射窗口 | 概念 | `pg_utils.c:162,186` | 地址翻译直觉 | 存量 02 §1.3 |
| K-028 | HigherHalf trait | 接口 | `higher_half.rs:30` | Rust 表达 | 存量 02 §3.4 |
| K-029 | 三架构 link.ld | 工具 | `os/kernel/src/arch/*/link.ld` | 布局可读 | 存量 02 §4.1 |
| K-030 | minix-elf | 机制 | `os/libs/minix-elf` | ELF 加载实现 | 存量 02 §4.2 |
| K-031 | arch_boot_impl Step 0-4 | 机制 | `os/kernel/src/lib.rs:291` | Rust 启动入口 | 存量 02 §4.4 |
| K-032 | kern_virt≠kern_phys 守卫 | 约束 | `os/kernel/src/lib.rs` | 历史 bug 教训 | 存量 02 §4.4 |
| K-033 | 独立 kernel ELF | 演进 | `os/kernel/Cargo.toml` | 构建形态 | 存量 02 §3.2 |
| K-034 | Sv39 L2 bug | 演进 | `os/arch/src/riscv64/paging.rs` | 真实案例 | 存量 02 附录 A |
| K-035 | PROVIDE 符号自用 | 决策 | `os/kernel/src/arch/*/link.ld` | 边界清晰 | 存量 02 §4.1 |
| K-036 | ELF 错误分类 | 接口 | `os/libs/minix-elf` | 诊断能力 | 存量 02 §4.2.6 |

- 验收标准：读者能解释"为什么切栈与跳转必须成对"并给出四种组合的后果；能画出低地址窗口与高地址窗口指向同一物理页的图；能列出 arch_boot_impl 五步并说明第 3、4 步为什么不能交换。必须出现的对比：x86/aarch64/riscv64 的跳转与取栈指令对照表。

### 03-protection-init

- 一句话定位：内核如何让 CPU 建立一套"只有内核能改"的运行上下文（GDT/IDT/TSS/内核栈）。
- 讲什么：K-037~K-054。保护的三要素；三架构特权级与返回指令；CPU 三问；`prot_init` 全流程；`tss_init` 与内核栈切换；`idt_init` 门表；类型状态；early console。
- 不讲什么：页表层级与地址翻译（08）；异常/中断处理逻辑（17）；trap frame 字段与入口汇编（11）；syscall 入口选择（11）；GDT 描述符位级格式与 IDT Gate 位级格式（只在附录级给一次表，不展开）。
- 前置：02。
- 后置：04（平台发现接续 cstart）、05、06、11（门表被入口使用）、17。
- 事实底线：
  - C：`main.c:115-147,403-481`；`protect.c:107-152,154-167,260-268,298-315,321-367`；`archconst.h`；`sched.h`（Linux 对照仅作课外引用）。
  - 非 C 制品：`os/arch/src/arch/{protection.rs:105,trap_entry.rs:116}`；`os/plat/src/early_console.rs:14`；`os/kernel/src/lib.rs:827`（init_protection）。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-037 | 保护三要素 | 概念 | `protect.c:321-367` | 概念地基 | 存量 03 §1.1 |
| K-038 | 三架构特权级 | 概念 | `protect.c`、`archconst.h` | 跨架构统一 | 存量 03 §1.2 |
| K-039 | CPU 三问 | 概念 | `protect.c:154,260` | 全 stage 框架 | 存量 03 §1.4 |
| K-040 | 跨特权级流程与返回指令 | 机制 | `mpx.S:391-459` | trap 往返前提 | 存量 03 §1.3 |
| K-041 | 硬件/软件切栈 | 机制 | `protect.c:154` | 内核栈起点 | 存量 03 §1.3 |
| K-042 | kmain 三件事 | 机制 | `main.c:115-147` | 定位入口 | 存量 03 §2.1 |
| K-043 | cstart 四调用 | 机制 | `main.c:403-481` | 固定次序 | 存量 03 §2.2 |
| K-044 | prot_init 全流程 | 机制 | `protect.c:321-367` | 主体 | 存量 03 §2.3 |
| K-045 | tss_init | 机制 | `protect.c:154-167` | 栈入口 | 存量 03 §2.3.1 |
| K-046 | idt_init 门表 | 机制 | `protect.c:107-152` | 向量表 | 存量 03 §2.3.2 |
| K-047 | 保留 GDT 的理由 | 决策 | `protect.c:341-348` | 消除疑惑 | 存量 03 §1.7 |
| K-048 | 不复用固件结构 | 决策 | `protect.c:330-331` | 生命周期理由 | 存量 03 §1.6 |
| K-049 | ProtectionArch | 接口 | `protection.rs:105` | Rust 表达 | 存量 03 §3.1 |
| K-050 | TrapEntryArch | 接口 | `trap_entry.rs:116` | Rust 表达 | 存量 03 §3.1 |
| K-051 | 类型状态替代标志 | 决策 | `os/kernel/src/lib.rs:827` | 顺序约束表达 | 存量 03 §3.2 |
| K-052 | init/load 顺序不变量 | 约束 | `os/kernel/src/lib.rs:827-990` | 安全窗口 | 存量 03 §4.3 |
| K-053 | 内核栈威胁模型 | 概念 | `mpx.S`、`klib.S:812` | 为什么必须换栈 | 存量 03 附录 A |
| K-054 | EarlyConsole | 接口 | `early_console.rs:14` | boot 输出 | 存量 03 §4.3 |

- 验收标准：能解释 `*((int*)0)=0` 为什么杀不掉内核（三要素各起什么作用）；能按序说出 prot_init 的六个里程碑；能说明 aarch64 写 SP_EL1 为什么必须保存/恢复 SP；能对照三架构回答"内核栈从哪里来"。必须出现的图：特权级切换往返图、IDT 门表。

### 04-platform-discovery

- 一句话定位：内核不写死地址，如何从固件给的描述里发现中断控制器、定时器、CPU 拓扑与控制台。
- 讲什么：K-055~K-068。四类硬件参数；硬编码三问题；C 的 ACPI/板级常量做法；PlatformDesc 与子描述符 trait；DTB/ACPI 两种解析器；QemuVirt 兜底；多源列表；T2.5 时序；迁移判别规则。
- 不讲什么：GDT/IDT/TSS（03）；时钟与中断控制器寄存器编程（05/06）；FDT 与 ACPI 规范细节（只讲"作为描述来源"的定位）；多核拓扑的实际使用（20）。
- 前置：03。
- 后置：05、06（消费描述）、20（CPU 拓扑）、35（测试兜底行为）。
- 事实底线：
  - C：`arch/i386/arch_system.c:246-281`（arch_init 内 acpi_init 调用）、`arch/i386/acpi.c:204-228,310`；`arch/earm/arch_system.c:98-132`。
  - 非 C 制品：`os/libs/minix-boot/src/platform.rs:55-322`；`os/libs/minix-platform/src/{desc.rs,kind.rs,global.rs:219-273,device_tree.rs,acpi.rs}`；`os/libs/minix-platform/src/arch/{x86_64,aarch64,riscv64}.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-055 | 四类硬件参数 | 概念 | `arch_system.c:246-281` | 发现的对象 | 存量 04 §1.1 |
| K-056 | 硬编码三问题 | 决策 | `acpi.c:204-228` | 动机 | 存量 04 §1.1 |
| K-057 | 三问 + 第四问 | 概念 | 与 03 衔接 | 框架复用 | 存量 04 §1.2 |
| K-058 | C ACPI 链 | 机制 | `acpi.c:310` | 对照基线 | 存量 04 §2.1 |
| K-059 | ARM 板级常量 | 机制 | `earm/arch_system.c:98-132` | 历史对照 | 存量 04 §2.2 |
| K-060 | PlatformDesc | 接口 | `platform.rs:247-322` | 抽象核心 | 存量 04 §3.2 |
| K-061 | QemuVirt 兜底 | 机制 | `minix-platform/src/arch/*.rs` | 无描述时行为 | 存量 04 §3.6 |
| K-062 | platform_sources | 接口 | `kernel_info.rs:103` | 多源交接 | 存量 04 §3.7 |
| K-063 | PlatformContext | 机制 | `global.rs:219-273` | 解析入口 | 存量 04 §4.3 |
| K-064 | DTB/ACPI 解析 | 机制 | `device_tree.rs`、`acpi.rs` | 两条实现 | 存量 04 §4.2 |
| K-065 | new(desc) 构造模式 | 决策 | `os/arch/src/*/clock.rs` | 热路径零 downcast | 存量 04 §3.4 |
| K-066 | T2.5 时序 | 约束 | `os/kernel/src/lib.rs` | 对 C 的偏离 | 存量 04 §4.6 |
| K-067 | 迁移判别规则 | 决策 | `os/plat/src/x86_64/*` | 常量归属 | 存量 04 §4.7 |
| K-068 | AssumeSyncCell | 数据结构 | `cell.rs` | 单写多读原语 | 存量 04 §3.5 |

- 验收标准：给出一张"参数 → 描述源 → 解析器 → 硬件实例"的传递图；能解释为什么发现要早于时钟/中断初始化（对比 C 顺序）；能说明 QemuVirt 兜底在生产构建与测试构建下的不同行为；能对 PIT 端口常量与 LAPIC 基址各判一次"保留还是发现"。

### 05-clock-init

- 一句话定位：内核软件时钟状态如何建立、三架构的 tick 源各是什么、为什么硬件定时器到 `bsp_finish_booting` 才启动。
- 讲什么：K-069~K-077。`init_clock`；ClockState 与构造；ClockArch trait；DEFAULT_HZ=100 的决策与 [ARCH: K-3] 标注；PIT/CNTP/CLINT 三种时钟源；hz 解析；BSP/AP 区分。
- 不讲什么：tick 处理四类工作（19）；中断控制器初始化（06）；AP 定时器注册（20）；TSC 校准细节（04）；时钟系统调用（27）。
- 前置：04。
- 后置：06（同一 cstart 段）、09（启动定时器）、19（tick 处理）、20（AP 定时器）。
- 事实底线：
  - C：`clock.c:47-64,294-304`；`arch/i386/arch_clock.c:131-194`；`archconst.h`（各架构 hz）。
  - 非 C 制品：`os/kernel/src/clock.rs`（ClockState）、`os/arch/src/arch/clock.rs:43,87`、`os/arch/src/*/clock.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-069 | init_clock 只初始化软件状态 | 机制 | `clock.c:47-64` | 时序关键 | 存量 05 §2.1 |
| K-070 | ClockState | 数据结构 | `os/kernel/src/clock.rs:971` | 状态载体 | 存量 05 §3.1 |
| K-071 | ClockArch | 接口 | `os/arch/src/arch/clock.rs:87` | 硬件抽象 | 存量 05 §3.1 |
| K-072 | DEFAULT_HZ=100 | 决策 | `os/arch/src/arch/clock.rs:43` | 行为差异 | 存量 05 §3.2 |
| K-073 | 三种时钟源 | 机制 | `arch_clock.c:131`、`os/arch/src/*/clock.rs` | tick 来源 | 存量 05 §4.3-4.5 |
| K-074 | hz 校验 | 约束 | `clock.c:50-60` | 解析复刻 | 存量 05 §2.1 |
| K-075 | 编程/使能分离 | 决策 | `clock.c:294-304` | 顺序理由 | 存量 05 §3.7 |
| K-076 | Send+Sync 辨析 | 概念 | `os/kernel/src/clock.rs` | 避免误解 | 存量 05 §3.1 |
| K-077 | BSP/AP 区分 | 数据结构 | `os/kernel/src/clock.rs` | 唯一性前提 | 存量 05 §4.1.1 |

- 验收标准：能说出 init_clock 具体改动了哪些字段；能画出三架构"tick 从哪个硬件来、比较值写在哪里"的对照表；能解释 DEFAULT_HZ 为什么必须标 [ARCH: K-3]（doc/design/code 三处一致）；能说明 `boot_cpu_init_timer` 为什么不在 cstart 里调用。

### 06-interrupt-controller-init

- 一句话定位：让 CPU 的异步事件有一个可管理、可屏蔽、可仲裁的入口。
- 讲什么：K-078~K-087。同步异常 vs 异步中断；中断控制器原理；8259A/LAPIC/IOAPIC/GICv3/PLIC 的初始化；InterruptController trait 与 claim→complete 协议；ArchInit trait 与 `arch_init` 六件事。
- 不讲什么：中断 handler 与钩子链（17）；驱动的 IRQ 注册与通知（26）；时钟源编程（05）；异常处理（17）。
- 前置：05（或与 05 并行阅读；两篇共同依赖 04）。
- 后置：09（时钟启动注册中断线）、17、20、24。
- 事实底线：
  - C：`i8259.c:28-80`；`apic.c:241-296,868-1002`；`arch_system.c:246-281`；`earm/bsp/ti/omap_intr.c:22-44`；`hw_intr.h:19-52`。
  - 非 C 制品：`os/plat/src/interrupt.rs:142,179`；`os/plat/src/{x86_64,arm64,riscv64}/interrupt.rs`；`os/arch/src/arch/arch_init.rs:46`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-078 | 同步 vs 异步 | 概念 | `exception.c`、`interrupt.c` | 首次完整 | 存量 05 §1.0 |
| K-079 | 控制器 = 转发 + 仲裁 | 概念 | `i8259.c:28-80` | 原理 | 存量 05 §1.0 |
| K-080 | 8259A ICW | 机制 | `i8259.c:28-80` | 初始化 | 存量 05 §2.2 |
| K-081 | LAPIC/IOAPIC | 演进 | `apic.c:241-296` | 代际 | 存量 05 §2.2 |
| K-082 | GICv3/PLIC | 演进 | `os/plat/src/*/interrupt.rs` | 跨架构 | 存量 05 §3.8 |
| K-083 | InterruptController | 接口 | `interrupt.rs:142` | 抽象核心 | 存量 05 §3.3 |
| K-084 | 全局 vs per-CPU 非对称 | 演进 | `os/plat/src/interrupt.rs` | 未来拆分 | 存量 05 §3.3 |
| K-085 | ArchInit | 接口 | `arch_init.rs:46` | 收留所边界 | 存量 05 §3.5 |
| K-086 | arch_init 六件事 | 机制 | `arch_system.c:246-281` | C 对照 | 存量 05 §2.4 |
| K-087 | 放 minix-plat 的理由 | 决策 | `os/plat/src/interrupt.rs` | crate 边界 | 存量 05 §3.3 |

- 验收标准：能画出 IRQ 线 → 控制器 → CPU → 内核入口的完整链路（入口处理只指到 17）；能说明 IDT 门安装与控制器初始化的先后顺序和理由；能对共享 IRQ 的 mask/actid 语义给出至少两个边界场景。

### 07-proc-table-init

- 一句话定位：把编译期写死的 17 个 boot 项，变成进程表里真实存在、特权就位、等待被调度的进程（阶段 C）。
- 讲什么：K-088~K-115。进程本质与四问；三件套；proc/priv 字段全表与存储模型；RTS 位表；boot image 与编号映射；VM ELF 手工装载；清空→填充 12 步；CpuContextArch；零堆因果链；Drop 纪律与测试夹具。
- 不讲什么：运行时 IPC 协议（14）；调度队列操作（13）；priv 的运行时语义与 privctl（21）；信号状态机（25）；运行期记账（19）；页表与跨空间（08）；启动收尾（09）；VM 协商（10）。
- 前置：03、05、06。
- 后置：08、09、10、13、14、21、23。
- 事实底线：
  - C：`proc.c:119-159`；`main.c:164-271`；`arch/i386/protect.c:388-455`；`arch/i386/memory.c:722-731`；`arch/i386/arch_system.c:146-186,495-610`；`proc.h:22-231,265-279`；`priv.h:10-66,94-95`；`table.c:44-65`；`include/minix/param.h:9`；`include/minix/sys_config.h:8-9`；`com.h:47-56,70-78`。
  - 非 C 制品：`os/kernel/src/{proc.rs,proc_table.rs,kpriv.rs,capability.rs,lib.rs}`；`os/arch/src/arch/boot.rs:144,296`；`os/arch/src/{x86_64,arm64,riscv64}/boot.rs`；`os/kernel/src/test_helpers.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-088 | A-F 主线 | 概念 | 全 stage 导览 | 阶段定位 | 存量 06 §1.0 |
| K-089 | 进程 = 断点续传 | 概念 | `proc.h:23-24` | 本质 | 存量 06 §1.1.1 |
| K-090 | 进程五组成 | 概念 | `proc.h:22-140` | 结构化 | 存量 06 §1.1.3 |
| K-091 | CPU 四问 | 概念 | `arch_system.c:495` | 新进程分析 | 存量 06 §1.2 |
| K-092 | 三件套 | 概念 | `table.c`、`priv.h:94` | 核心 | 存量 06 §1.3 |
| K-093 | proc_addr | 数据结构 | `proc.h:265-279` | O(1) 定位 | 存量 06 §2.1.0 |
| K-094 | proc 字段 6 组 | 数据结构 | `proc.h:22-140` | 字段字典 | 存量 06 §2.1 |
| K-095 | RTS 16 位全集 | 数据结构 | `proc.h:142-166` | 唯一状态源 | 存量 06 §2.1.6 |
| K-096 | RTS_SET/UNSET 宏定义 | 机制 | `proc.h:206-231` | 标志即队列 | 存量 06 §2.1.6 |
| K-097 | 特权表分治 | 数据结构 | `priv.h:94-95`、`com.h:78` | 64 槽布局 | 存量 06 §2.2.0 |
| K-098 | sys_id vs proc_nr | 概念 | `type.h`、`endpoint.h` | 两套编号 | 存量 06 §2.2.0 |
| K-099 | ppriv_addr 已弃 | 演进 | `priv.h:94-95` | 历史 | 存量 06 §2.2.0 |
| K-100 | priv 静态/动态段 | 机制 | `system.c:274-302` | 分配 | 存量 06 §2.2.0 |
| K-101 | priv 字段存储布局 | 数据结构 | `priv.h:21-66` | 字段字典 | 存量 06 §2.2.0 |
| K-102 | IPC 对称性 | 约束 | `system.c:307-330` | 不变量 | 存量 06 §2.2.3 |
| K-103 | image[] 与编号映射 | 数据结构 | `table.c:44-65` | boot 清单 | 存量 06 §1.4.1 |
| K-104 | VM bootstrap | 决策 | `protect.c:388-455` | 第一个用户进程 | 存量 06 §1.4.2 |
| K-105 | 清空→填充 12 步 | 机制 | `proc.c:119`、`main.c:164-271` | 主流程 | 存量 06 §1.5 |
| K-106 | arch_proc_reset/init | 机制 | `arch_system.c:146`、`memory.c:722` | 可执行起点 | 存量 06 §2.1.4 |
| K-107 | CpuContextArch | 接口 | `boot.rs:144` | Rust 表达 | 存量 06 §3.5 |
| K-108 | load_vm_elf | 机制 | `boot.rs:296` | ELF 装载 | 存量 06 §3.7 |
| K-109 | 零堆三层因果链 | 决策 | `sys_config.h:8-9` | no_std 约束 | 存量 06 §2.0 |
| K-110 | Drop 只报警 | 约束 | `os/kernel/src/proc.rs` | 生命周期 | 存量 06 §3.1.1 |
| K-111 | CapabilityTemplate | 接口 | `capability.rs` | boot 权限构造 | 存量 06 §3.2 |
| K-112 | 三架构初始化对照 | 接口 | `os/arch/src/*/boot.rs` | 跨架构 | 存量 06 §1.2.3 |
| K-113 | FPU 保存区内嵌 | 数据结构 | `arch_system.c:134` | 对象大小 | 存量 06 §3.6 |
| K-114 | kernel_may_alloc 窗口 | 约束 | `main.c:105` | 分配纪律 | 存量 06 §1.6.2 |
| K-115 | Drop 三分类与夹具豁免 | 测试 | `test_helpers.rs` | 测试纪律 | 新增 panic-in-drop |

- 验收标准：能按序复述阶段 C 的 12 步并指出每一步在哪设置 RTS 位；能从 `proc.h` 任取一个字段说出它属于哪组、在阶段 C 是清空还是填充；能解释 `RTS_SET(PROC_STOP)` 与 `~RTS_SLOT_FREE` 的先后为什么影响入队；能说明 `KProcess::drop` 为什么必须 panic 以及测试如何豁免。必须出现的表：proc 六组字段导航表、priv 六组字段导航表、RTS 16 位表、三架构 CPU 四问对照表。

### 08-cross-space-init

- 一句话定位：内核既没有自己的页表、又要访问别人的内存，64 位系统如何一次性解决这个问题（阶段 D）。
- 讲什么：K-116~K-129。矛盾定义；32 位临时窗口方案与它的三个代价；direct map 公式与跨进程四步；双视图必然性；Kernel DM 的建立（`establish_boot_dm`）；两源候选与页粒度；DirectMapArch；阶段 D 简化为断言；废弃清单；paging_init 语义分解。
- 不讲什么：VM 侧 `init_page_table`/`map_kernel` 的完整实现（归 02-stage-vm）；`createpde` 的逐行实现（只在历史对照里给引用，不展开）；运行时跨空间拷贝协议（18）；CR3 切换的 VMCTL 时序（10）。
- 前置：07。
- 后置：09（内存回收）、10（setcr3 使用根）、18（DM 被拷贝使用）、24。
- 事实底线：
  - C：`arch/i386/protect.c:370-377`（arch_post_init）；`arch/i386/pg_utils.c:162,186,247`；`arch/i386/memory.c:69-147,707-717`；`arch/i386/cpulocals.h:55`。
  - 非 C 制品：`os/kernel/src/dm_coverage.rs`；`os/arch/src/arch/{direct_map.rs:28,dm_coverage.rs}`；`os/kernel/src/lib.rs:291`（arch_boot Step 4）；`07-paging_init_gpt.md`（设计备忘，作为 K-129 证据）。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-116 | 跨空间矛盾 | 概念 | `memory.c:592-666` | 问题定义 | 存量 07 §1.1 |
| K-117 | 32 位临时窗口 | 机制 | `memory.c:69,707` | 历史方案 | 存量 07 §1.2 |
| K-118 | ptproc 双用途 | 机制 | `cpulocals.h:55`、`arch_do_vmctl.c:25` | 保留项 | 存量 07 §2.1 |
| K-119 | direct map 公式 | 机制 | `direct_map.rs:28` | 核心解法 | 存量 07 §1.3 |
| K-120 | 双视图必然性 | 约束 | `direct_map.rs` | 为什么两个窗口 | 存量 07 §1.3.1 |
| K-121 | G=1 TLB | 机制 | `pg_utils.c:204-230` | 性能关键 | 存量 07 §3.2 |
| K-122 | establish_boot_dm | 机制 | `os/kernel/src/dm_coverage.rs` | 建立点 | 存量 07 §4.4 |
| K-123 | 两源候选与页粒度 | 数据结构 | `os/arch/src/arch/dm_coverage.rs` | hole 语义 | 存量 07 §4.4 |
| K-124 | DirectMapArch | 接口 | `direct_map.rs:28` | 抽象 | 存量 07 §4.1 |
| K-125 | map_kernel 演进 | 演进 | VM 侧 `init_page_table` | 协作边界 | 存量 07 §3.4 |
| K-126 | 阶段 D 降为断言 | 决策 | `os/kernel/src/lib.rs` | 简化结果 | 存量 07 §3.5 |
| K-127 | 废弃清单 9 项 | 演进 | `07 §3.6` 表 | 去留清楚 | 存量 07 §3.6 |
| K-128 | free_upper_idx 保留 | 接口 | `uefi_helpers.rs` | 镜像字段 | 存量 07 §4.3 |
| K-129 | paging_init 语义分解 | 决策 | `07-paging_init_gpt.md`、`os/kernel/src/lib.rs` | 职责边界 | 新增 |

- 验收标准：能解释"内核没有自己的页表"这句话的准确含义；能画出一次跨进程拷贝从 VA 到 PA 到 kernel VA 的完整路径；能说明 Kernel DM 为什么必须先于 VM DM 建立；能列出被废弃的 9 项机制及各自替代。必须出现的图：双视图窗口图、direct map 建立时序图。

### 09-system-init-and-boot-finish

- 一句话定位：内核从"初始化态"切换到"运行态"的收口：注册调用、回收内存、解除停止、进入调度。
- 讲什么：K-130~K-141。Phase A-F 收口时序；`system_init` 三段；`add_memmap` 回收；`bsp_finish_booting` 12 步；`kernel_may_alloc` 关闭；`vm_running` 修正；smp fallback；Syscall enum 与编译期断言；CPU 身份探测。
- 不讲什么：各 handler 的实现（23-28）；VMCTL 协商（10）；SMP AP 启动（20，本篇只讲 fallback 与调用点）；调度循环内部（16）。
- 前置：07、08。
- 后置：10、13、16、20。
- 事实底线：
  - C：`system.c:168-270`；`main.c:38-109,149,160-162,275-316`；`arch/i386/pg_utils.c:86-121`；`arch_smp.c:168,289-345`；`include/i386/arch_smp.h:18-23`；`glo.h:74,76`。
  - 非 C 制品：`os/kernel/src/{lib.rs,syscall.rs,memmap.rs,cpu_identity?}`；`os/arch/src/arch/cpu_identity.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-130 | 收口 Phase 时序 | 机制 | `os/kernel/src/lib.rs` | 定位 | 存量 08 §1.1 |
| K-131 | system_init 三段 | 机制 | `system.c:168-270` | 收口主体 | 存量 08 §2.3 |
| K-132 | call_vec 与 map() | 机制 | `system.c:52-57` | 注册机制 | 存量 08 §2.1 |
| K-133 | PROC_STOP 解除范围 | 约束 | `main.c:64-66` | kernel task 证据 | 存量 08 §1.2 |
| K-134 | add_memmap | 机制 | `pg_utils.c:86-121` | 内存回收 | 存量 08 §2.3 |
| K-135 | vm_running 修正 | 决策 | `main.c:47` | 行为偏离 | 存量 08 §2.2 |
| K-136 | bsp_finish_booting 12 步 | 机制 | `main.c:38-109` | 启动最后一站 | 存量 08 §2.3 |
| K-137 | smp_init 不返回 | 机制 | `arch_smp.c:168` | 调用树 | 存量 08 附录 A |
| K-138 | Syscall enum | 演进 | `os/kernel/src/syscall.rs:65` | Rust 表达 | 存量 08 §3.1 |
| K-139 | CPU 身份探测 | 机制 | `cpu_identity.rs`、`arch_system.c:212` | C bug 修复 | 存量 08 §4.6 |
| K-140 | IRQ/alarm 表初始化 | 数据结构 | `glo.h`、`priv.h:48` | 运行期表起点 | 存量 08 §2.2 |
| K-141 | kinfo 字段缺口 | 工具 | `pg_utils.c:110-115` | 遗留 | 存量 08 §4.5 |

- 验收标准：能按序说出 `bsp_finish_booting` 每一步及其不可交换性（尤其是 TSC 基线、时钟启动、关窗口、进调度）；能解释为什么解除 PROC_STOP 的范围是 `NR_BOOT_PROCS - NR_TASKS`；能说明 Rust 把 C 的三件大事拆到哪三处。必须出现的表：C 12 步与 Rust 实现步骤对照表。

### 10-vm-boot-protocol

- 一句话定位：第一个被调度执行的用户进程（VM）如何从内核手里接过页表控制权，让系统真正可运行。
- 讲什么：K-142~K-154。协商 8 步；双视图属性；`do_vmctl` 与 `arch_do_vmctl` 子命令；`setcr3` 五步；MEMREQ 请求-应答；VMINHIBIT/BOOTINHIBIT 解除；VmCtlParam/TlbArch；boot 期 VM ELF 加载接线。
- 不讲什么：VM 侧 `init_page_table`/`map_kernel` 实现（02-stage-vm）；页错误处理（17）；跨空间拷贝挂起（18）；SYS_VMCTL 以外的内存系统调用（22/26）。
- 前置：07、09（16 的调度循环在运行时序上是本协议的执行环境，但本协议作为"启动完成条件"在此讲完；读者只需知道 VM 会被调度）。
- 后置：16（调度循环执行本协议）、17、18、24。
- 事实底线：
  - C：`system/do_vmctl.c:17-173`；`arch/i386/arch_do_vmctl.c:19-67`；`main.c:47`；`arch_do_vmctl.c:32`；`proc.h:98-101`。
  - 非 C 制品：`os/kernel/src/vm.rs:758-820`；`os/arch/src/arch/tlb_arch.rs:78`；`os/arch/src/arch/paging.rs`（from_active_root）；`os/kernel/src/lib.rs`（CURRENT_PTPROC_NR/CURRENT_ROOT_PHYS）。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-142 | 协商 8 步 | 接口 | `do_vmctl.c:17-173` | 主体 | 存量 09 §1.2 |
| K-143 | 双视图属性表 | 数据结构 | `direct_map.rs` | 属性对照 | 存量 09 §1.3 |
| K-144 | do_vmctl 9 子命令 | 接口 | `do_vmctl.c:33-168` | 全貌 | 存量 09 §2.1 |
| K-145 | arch 4 子命令 | 接口 | `arch_do_vmctl.c:38-66` | x86 特有 | 存量 09 §2.2 |
| K-146 | setcr3 五步 | 机制 | `arch_do_vmctl.c:19-33` | 切换实现 | 存量 09 §2.2 |
| K-147 | MEMREQ 与 VMSTYPE | 机制 | `do_vmctl.c:37-110` | 请求链 | 存量 09 §2.3 |
| K-148 | VMINHIBIT SMP 语义 | 机制 | `do_vmctl.c:125-161` | 跨 CPU 协同 | 存量 09 §2.4 |
| K-149 | VmCtl 类型 | 接口 | `os/kernel/src/vm.rs:758` | Rust 表达 | 存量 09 §4.1 |
| K-150 | TlbArch | 接口 | `tlb_arch.rs:78` | 根切换抽象 | 存量 09 §4.7 |
| K-151 | ptproc/root 跟踪 | 数据结构 | `os/kernel/src/lib.rs` | 状态传递 | 存量 09 §4.8 |
| K-152 | from_active_root | 接口 | `paging.rs` | 不清零包装 | 存量 09 §4.9 |
| K-153 | boot 期 VM ELF 加载 | 机制 | `os/kernel/src/lib.rs` | 接线现状 | 存量 09 §4.9.3 |
| K-154 | ClearMapCache WONTFIX | 决策 | `do_vmctl.c:162` | 64 位无此概念 | 存量 09 §4.7 |

- 验收标准：能按序复述协商 8 步并标注每一步影响哪些进程标志；能解释 `setcr3` 为什么先写 `cr3_v` 再条件写 CR3；能说明双视图为什么不是"两份映射"；能指出 C 的 `vm_running` 缺陷与 Rust 修正点。必须出现的图：Kernel/VM 双窗口与协商时序合并图。

### 11-trap-entry

- 一句话定位：CPU 从用户态（或内核态）进入内核的全部入口形态、寄存器约定与返回方式。
- 讲什么：K-155~K-166。三类入口与向量表；异常帧与 trap frame；入口汇编的保存/恢复；返回指令三形态与同环/半环分流；IPC/SYSCALL/SENDA 寄存器约定；KTS trap_style；二次返回通道；统一 CpuContext 模型；E1 trap 桥的决策与开放问题。
- 不讲什么：异常如何被分类处理（17）；系统调用如何被分派（15）；IPC 语义（14）；中断控制器初始化（06）；信号帧布局（25）。
- 前置：03（向量表）、07（上下文结构）。
- 后置：12（入口取 BKL）、15、16、17、25。
- 事实底线：
  - C：`arch/i386/mpx.S:202-459,461-585`；`arch/i386/usermapped_glo_ipc.S:10-108`；`arch/i386/include/{sconst.h:30-98,stackframe.h:17-36,arch_proto.h:50-80}`；`archconst.h:31-35,167-172`；`protect.c:147-153,243-268`；`arch_system.c:485-493,566-610`。
  - 非 C 制品：`os/arch/src/arch/{trap_entry.rs:116,trap_return.rs:72}`；`os/arch/src/x86_64/{trap_entry.rs,trap_stub.rs,boot.rs}`；`os/kernel/src/trap_dispatch.rs:179-236`；`18-trap-bridge-design.md`（E1 决策证据）。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-155 | 三类入口与向量表 | 接口 | `protect.c:147-153` | 开篇主体 | 存量 14 §1.2 |
| K-156 | 异常帧 | 数据结构 | `arch_proto.h:72-80` | 进入时的栈状态 | 存量 14 §1.3 |
| K-157 | 保存/恢复上下文 | 机制 | `mpx.S:202-337` | 入口汇编 | 存量 14 §1.2 |
| K-158 | 返回指令三形态 | 机制 | `mpx.S:391-459` | 出口选择 | 存量 14 §4.8 |
| K-159 | 寄存器约定 | 接口 | `usermapped_glo_ipc.S:80-104` | 用户侧契约 | 存量 13 §1.2 |
| K-160 | trampoline → syscall | 演进 | `usermapped_glo_ipc.S:10-108` | 入口代际 | 存量 28 §1.3 |
| K-161 | KTS trap_style | 数据结构 | `archconst.h:167-172` | 返回分流依据 | 存量 19 §4.6.1 |
| K-162 | 二次返回通道 | 机制 | `set_secondary_ipc_return` | KernInfo 返回 | 新增 18-trap-bridge |
| K-163 | 统一 CpuContext | 决策 | `boot.rs:144` | Rust 模型 | 存量 18-trap-bridge |
| K-164 | 同环/半环分流 | 机制 | `mpx.S:461-585` | 内核态返回 | 存量 14 §4.8 |
| K-165 | 双轨收敛 OQ | 决策 | `18-trap-bridge-design.md:84-87` | 风险记录 | 新增 |
| K-166 | 快速入口瘦帧 | 机制 | `mpx.S:202-260` | 性能代价 | 存量 14 附录 A/C |

- 验收标准：能画出一张表，行是三类入口（SYSCALL/SYSENTER/int）、列是"保存了什么/如何返回/谁消费返回值"；能说明 int-33 与 int-32 为什么必须是两个门；能解释 KTS 在 SIGSEND/SIGRETURN 往返中的作用；对 E1 的两条腿（SYSCALL 与 int-33）能各说一条当前限制。必须出现：寄存器约定完整表。

### 12-concurrency-model

- 一句话定位：在内核里修改共享状态前，读者必须先理解的互斥纪律与 per-CPU 模型。
- 讲什么：K-167~K-176。BKL 的取舍与代价；三条临界区约束；获取/释放点（入口、context_stop、switch_to_user）；per-CPU 数据为什么免锁；中断上下文与进程上下文的规则；spinlock 实现与单核退化；内存序基础。
- 不讲什么：调度队列操作（13）；IPC 阻塞（14）；AP 启动与 IPI（20）；具体锁的代码（只讲不变量，代码引用）。
- 前置：11（入口）、07（共享状态）。
- 后置：13、14、15、16、19、20、21-28。
- 事实底线：
  - C：`smp.c:27-28,75-112`；`spinlock.h:6-43`；`cpulocals.h:11-79`；`arch_clock.c:208-349`；`klib.S:723-771`；`main.c:149`。
  - 非 C 制品：`os/kernel/src/smp.rs:1223-1350`（BKL/Guard/Section）、`os/kernel/src/smp.rs:137-331`（CpuLocal/CpuState）；`AGENTS.md` 执行模型段。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-167 | BKL 取舍 | 概念 | `smp.c:27-28` | 开篇 | 存量 16 §1.1 |
| K-168 | BKL 三约束 | 约束 | `smp.c:80,86` | 红线 | 存量 16 §1.1 |
| K-169 | 获取/释放点 | 机制 | `main.c:149`、`arch_clock.c:242,346` | 边界 | 存量 16 §2.2 |
| K-170 | per-CPU 数据 | 数据结构 | `cpulocals.h:37-75` | 免锁原因 | 存量 16 §1.2 |
| K-171 | 上下文约束 | 概念 | `AGENTS.md`、`smp.c` | 行为规则 | 存量 00 §5.3 |
| K-172 | spinlock 实现 | 机制 | `klib.S:733-771` | 原子原语 | 存量 16 §2.2 |
| K-173 | 内存序基础 | 约束 | `os/kernel/src/smp.rs:271` | 可见性 | 存量 16 §4.5 |
| K-174 | BklGuard/Section | 演进 | `os/kernel/src/smp.rs:1223` | Rust 强制 | 存量 16 D3/D4 |
| K-175 | 单核退化 | 机制 | `arch_smp.h:18-23` | 构建差异 | 存量 16 附录 A |
| K-176 | 临界区禁做清单 | 约束 | 规范 + `smp.c` 注释 | 操作规则 | 存量 00 §1.5.1 |

- 验收标准：能回答"为什么自旋锁持有者不能睡眠"，并给出一个内核里的真实反例场景；能列出至少 6 项 per-CPU 数据并说明为什么它们不需要锁；能解释 BklSection 能在编译期阻止什么错误。必须出现：临界区边界图。

### 13-scheduling-primitives

- 一句话定位：RTS 标志与就绪队列如何保持"在队列 ⟺ 可运行"这一唯一不变量。
- 讲什么：K-177~K-192。INV-1；16 级优先级；队列结构；enqueue/enqueue_head/dequeue/pick_proc；proc_no_time；RTS_SET/UNSET 联动；enter_queue 修复；Scheduler/ProcessTable 拆分；Priority newtype；SchedParams/sched_proc；IDLE 不入队；通知调度器。
- 不讲什么：调度循环如何调用这些原语（16）；IPC 阻塞语义（14）；SMP 跨 CPU 入队细节（20 只引用）；用户态 SCHED 服务实现（06-stage-sched）。
- 前置：12（临界区）、07（proc/RTS）。
- 后置：14（阻塞唤醒）、16（调用方）、23（schedctl/schedule）、27（quantum）。
- 事实底线：
  - C：`proc.c:1595-1832,1860-1910`；`proc.h:38-45,187-231`；`cpulocals.h:57-60`；`config.h:66-74`；`system.c:642-723`。
  - 非 C 制品：`os/kernel/src/{sched.rs,proc_table.rs,proc.rs}`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-177 | INV-1 | 约束 | `proc.c:1595-1813` | 核心不变量 | 存量 11 §1.1 |
| K-178 | 16 级队列 | 概念 | `config.h:66-74` | 模型 | 存量 11 §1.2 |
| K-179 | 队列结构 | 数据结构 | `cpulocals.h:57-60` | 载体 | 存量 11 §2.1 |
| K-180 | enqueue 三职责 | 机制 | `proc.c:1595-1659` | 主操作 | 存量 11 §2.2 |
| K-181 | enqueue_head/dequeue | 机制 | `proc.c:1670-1780` | 主操作 | 存量 11 §2.3 |
| K-182 | pick_proc | 机制 | `proc.c:1785-1813` | 选择 | 存量 11 §2.5 |
| K-183 | proc_no_time | 机制 | `proc.c:1893-1910` | 量子耗尽 | 存量 11 §2.6 |
| K-184 | RTS 与队列联动 | 机制 | `proc.h:206-231` | 关键机制 | 存量 11 §2.7 |
| K-185 | enter_queue 修复 | 决策 | `proc.c:1653` | C bug | 存量 11 §3.6 |
| K-186 | Scheduler 拆分 | 决策 | `os/kernel/src/sched.rs` | Rust 设计 | 存量 11 §3.2 |
| K-187 | Priority newtype | 数据结构 | `os/kernel/src/proc.rs` | 类型安全 | 存量 11 §3.3 |
| K-188 | sched_proc 9 步 | 接口 | `system.c:642-723` | 调度器接口 | 存量 11 §4.6 |
| K-189 | IDLE 不入队 | 约束 | `proc.c:149`、`main.c:64` | 边界 | 存量 11 §3.7 |
| K-190 | SCHEDULING_NO_QUANTUM | 机制 | `proc.c:1860-1891` | 协作协议 | 存量 11 §4.5 |
| K-191 | RTS 履历表 | 数据结构 | `proc.c:119`、`main.c:165-270` | 时序台账 | 存量 11 附录 A |
| K-192 | 参数验证 | 约束 | `system.c:642-723` | 入参边界 | 存量 11 §4.6 |

- 验收标准：给定一个进程状态变化（如阻塞发送），能说出哪些队列操作必须同时发生、若遗漏会破坏哪条不变量；能解释 P_BLOCKEDON 这一类"动态字段选择"为什么让死锁检测统一；能复述 C 的两处 enqueue 写法错误及 Rust 如何避免。必须出现：队列操作与 RTS 位联动表。

### 14-ipc-core

- 一句话定位：endpoint 寻址与六个 IPC 原语——微内核里唯一的"通信总线"。
- 讲什么：K-193~K-212。endpoint 语义、编码与 generation；特殊端点；端点验证；六原语模型；发送/接收/通知/批量异步的实现语义；阻塞与唤醒；死锁检测；延迟投递；匹配宏；异步线格式。
- 不讲什么：权限与过滤（21/22）；trap 入口（11）；分派（15）；投递调用点（16）；信号交互（25）；VM 缺页恢复（18）。
- 前置：11、13。
- 后置：15（do_ipc 与 kernel_call 并列）、16、21-28、36（线格式）。
- 事实底线：
  - C：`proc.c:263-294,479-698,703-768,773-838,870-1167,1200-1326,1331-1384,1390-1590,1830-1858`；`ipc.h:14-22`；`ipcconst.h:7-14`；`endpoint.h:45-69`；`priv.h:28-43,86-87`。
  - 非 C 制品：`os/kernel/src/ipc.rs`；`os/libs/minix-types/src/types/endpoint.rs`；`os/libs/minix-types/src/ipc/message.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-193 | endpoint 标识符 | 概念 | `endpoint.h` | 寻址起点 | 存量 99 |
| K-194 | 编码布局 | 数据结构 | `endpoint.h:45-69` | 手工编码 | 存量 99 §1.2 |
| K-195 | generation 语义 | 机制 | `do_fork.c:59,69-72` | 安全复用 | 存量 99 §1.3 |
| K-196 | 特殊端点 | 数据结构 | `endpoint.h:54-57` | 通配 | 存量 99 §1.3 |
| K-197 | isokendpt_f | 机制 | `proc.c:1830-1858` | 验证 | 存量 99 §1.4 |
| K-198 | 六原语模型 | 概念 | `ipcconst.h:7-14` | 全景 | 存量 12 §1.2 |
| K-199 | do_sync_ipc 检查 | 机制 | `proc.c:479-597` | 入口 | 存量 12 §2.3 |
| K-200 | mini_send | 机制 | `proc.c:870-962` | 发送 | 存量 12 §2.4 |
| K-201 | mini_receive | 机制 | `proc.c:967-1117` | 接收优先级 | 存量 12 §1.4 |
| K-202 | mini_notify | 机制 | `proc.c:1122-1167` | 通知 | 存量 12 §2.6 |
| K-203 | deadlock | 机制 | `proc.c:703-768` | 死锁 | 存量 12 §1.5 |
| K-204 | SENDREC/PEND | 机制 | `proc.c:1000-1005` | 原子性 | 存量 12 §1.2 |
| K-205 | SENDA | 机制 | `proc.c:1200-1326` | 批量异步 | 存量 12 §2.9 |
| K-206 | delivermsg | 机制 | `proc.c:263-294` | 延迟投递 | 存量 12 §2.8 |
| K-207 | 匹配宏 | 接口 | `ipc.h:14-22` | 匹配条件 | 存量 12 §2.10 |
| K-208 | caller_q | 数据结构 | `os/kernel/src/ipc.rs` | 零堆队列 | 存量 12 §3.2 |
| K-209 | IpcOutcome/Engine/Error | 演进 | `os/kernel/src/ipc.rs` | Rust 表达 | 存量 12 §3 |
| K-210 | MF_SIG_DELAY | 机制 | `proc.c:1082`、`system.c:454` | 与信号交互 | 存量 12 §4.4 |
| K-211 | SENDA 上限 | 约束 | `proc.c:681-682` | 边界 | 存量 12 §4.8 |
| K-212 | asynmsg 线格式 | 接口 | `ipc.h:2745-2761` | 扩展协议 | 存量 12 §3.10 |

- 验收标准：能对六原语各给一个最小场景并说明 RTS 位如何变化；能解释"延迟投递"的动机与两次失败的不同后果；能说明死锁检测为什么在 SEND/RECEIVE 两条路径都要调用；能手工编码/解码一个带 generation 的 endpoint。必须出现：六原语阻塞矩阵、一次 sendrec 的完整生命周期图。

### 15-syscall-dispatch

- 一句话定位：用户态请求内核服务时，调用号如何被验证、路由、执行、回复或挂起。
- 讲什么：K-213~K-228。SYS 号空间与 KERNEL_CALL；独立入口的理由；三段式分派；权限门；有效/空缺槽；TOCTOU 防护；finish 与 EDONTREPLY；VMSUSPEND 协议；BKL 区间；Syscall enum/KcallResult/ArchSyscall；双入口编排；延迟系统调用。
- 不讲什么：各 handler 内部（23-28）；权限位图的构建（21）；过滤（22）；BKL 原理（12）；trap 入口汇编（11）。
- 前置：14、12。
- 后置：21-28（全部 handler 引用本篇的返回约定）。
- 事实底线：
  - C：`system.c:52-163,612-637`；`com.h:205-278`；`vm.h:6`；`errno.h`（EDONTREPLY/EBADREQUEST/ECALLDENIED）。
  - 非 C 制品：`os/kernel/src/{syscall.rs,errno.rs}`；`os/libs/minix-types/src/ipc/kernel_call.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-213 | KERNEL_CALL 归一化 | 接口 | `com.h:205` | 号空间 | 存量 13 §1.3 |
| K-214 | 独立入口 | 决策 | `protect.c:147` | 语义分离 | 存量 13 §1.2 |
| K-215 | 三段式分派 | 机制 | `system.c:95-127` | 主体 | 存量 13 §2.4.1 |
| K-216 | 权限门 | 机制 | `system.c:111` | 拒绝 | 存量 13 §2.4.1 |
| K-217 | 有效/空缺槽 | 数据结构 | `system.c:193-268` | 边界 | 存量 13 §2.3 |
| K-218 | TOCTOU 防护 | 机制 | `system.c:136-163` | 安全 | 存量 13 §2.4.3 |
| K-219 | finish/EDONTREPLY | 机制 | `system.c:59-93` | 回复 | 存量 13 §2.4.2 |
| K-220 | VMSUSPEND 返回约定 | 接口 | `vm.h:6`、`system.c:612` | 挂起 | 存量 13 §1.4 |
| K-221 | KCALL_RESUME | 约束 | `proc.c:355-361` | 恢复点 | 存量 13 §1.4 |
| K-222 | BKL 区间 | 约束 | `system.c`、`arch_clock.c:242` | 锁边界 | 存量 13 §1.5 |
| K-223 | Syscall enum | 演进 | `syscall.rs:65` | Rust 表达 | 存量 13 §4.1 |
| K-224 | KcallResult | 接口 | `syscall.rs:198` | 返回约定 | 存量 13 §4.2 |
| K-225 | ArchSyscall | 接口 | `syscall.rs:245` | 退化 | 存量 13 §3.4 |
| K-226 | 双入口编排 | 机制 | `syscall.rs:651` | IPC/SYS 汇合 | 存量 13 §4.8 |
| K-227 | 延迟系统调用 | 机制 | `arch_system.c:485` | 追踪插入 | 存量 13 §6.2 |
| K-228 | kbill_kcall | 机制 | `system.c:160` | 记账 | 存量 13 §6.4 |
| K-506 | caller-by-nr 接口（槽位号取代 `&Proc`） | 演进 | `os/kernel/src/syscall.rs:479`；`do_schedule.c:8` | Rust 侧调用者表示法的收敛点 | 新增（33-syscall-caller-api） |

- 验收标准：给定消息 `m_type`，能算出 call_nr 并判断会不会被权限门拒绝；能画出 VMSUSPEND 从挂起到 `kernel_call_resume` 重放的完整时序；能解释为什么 finish 对 `EDONTREPLY` 不回复。必须出现：分派时序图、返回约定表。

### 16-switch-to-user

- 一句话定位：内核唯一的出口：决定下一个运行谁、处理完延迟工作、恢复用户态。
- 讲什么：K-229~K-239。汇聚点与永不返回；五阶段；PREEMPTED 规则；地址空间切换；misc flags 优先级与延迟投递；量子检查点；终局分派；idle；与 C 的差异表；BKL 与中断协作。
- 不讲什么：调度原语实现（13）；IPC 投递语义（14）；量子怎么被扣减（19）；上下文切换的架构汇编细节（11）。
- 前置：11、13、14、15。
- 后置：17（异常/IRQ 出口）、19（tick 触发的检查）、20（AP 进入同一循环）、23-28（系统调用返回）。
- 事实底线：
  - C：`proc.c:176-229,299-474`；`klib.S:605-626`；`proc.c:437-474`；`arch_clock.c:208-349`。
  - 非 C 制品：`os/kernel/src/lib.rs`（switch_to_user/scheduler_loop）、`os/kernel/src/proc_table.rs`（misc flags）、`os/arch/src/arch/trap_return.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-229 | 汇聚点/永不返回 | 概念 | `proc.c:299-474` | 定位 | 存量 10 §1.1 |
| K-230 | 五阶段 | 机制 | `proc.c:309-474` | 主体 | 存量 10 §1.3 |
| K-231 | PREEMPTED 规则 | 机制 | `proc.c:322-330` | 公平性 | 存量 10 §2.1 |
| K-232 | 地址空间切换 | 机制 | `proc.c:349` | CR3 | 存量 10 §2.3 |
| K-233 | misc flags 优先级 | 机制 | `proc.c:351-415` | 延迟工作 | 存量 10 §2.1 |
| K-234 | 量子检查位置 | 决策 | `proc.c:416-428` | 唯一竞争点 | 存量 10 §2.1 |
| K-235 | finish_and_restore | 机制 | `proc.c:437-474` | 终局 | 存量 10 §4.2 |
| K-236 | idle | 机制 | `proc.c:176-229` | 空转 | 存量 10 §2.2 |
| K-237 | 差异总表 | 决策 | `os/kernel/src/lib.rs` | 现状边界 | 存量 10 §4.5 |
| K-238 | BKL 与中断协作 | 机制 | `arch_clock.c:242-349` | 锁交互 | 存量 10 §4.6 |
| K-239 | 信号就地投递 | 决策 | `proc_table.rs` | 信号时机 | 存量 10 §3.3 |

- 验收标准：能按五阶段复述一次调度并说明每阶段在什么条件下会改变结果；能解释 `delivermsg` 为什么放在调度循环而不是发送完成时；能说明 idle 与 IDLE 进程的关系（谁空转、谁登记）。必须出现：一次完整的陷入-处理-调度-返回贯线图。

### 17-exception-interrupt

- 一句话定位：异常与中断进入内核后，内核如何分流、转发与唤醒。
- 讲什么：K-240~K-255。异常帧嵌套与来源；五分支；信号映射表；嵌套恢复点；页错误转发 VM 与禁止循环；cause_sig；IRQ hook 链；generic_handler；IrqManager；分类函数；跨架构 IRQ。
- 不讲什么：入口汇编与返回（11）；信号系统调用协议（25）；驱动的 IRQCTL 接口（26）；时钟 tick 处理（19）。
- 前置：11、13、14、15。
- 后置：18（缺页恢复）、19（时钟中断）、25（信号消费）、26（IRQ 注册）。
- 事实底线：
  - C：`exception.c:19-40,49-130,180-283`；`interrupt.c:23-176`；`system/do_irqctl.c:143-172`；`hw_intr.h:19-52`；`system.c:389-449`。
  - 非 C 制品：`os/arch/src/arch/{exception.rs:40,exception_dispatcher.rs:55,79}`；`os/kernel/src/{irq_manager.rs,page_fault.rs}`；`os/arch/src/*/exception.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-240 | 嵌套与来源 | 机制 | `exception.c:180-283` | 分流前提 | 存量 14 §1.1 |
| K-241 | 五分支 | 机制 | `exception.c:180-283` | 主体 | 存量 14 §2.1 |
| K-242 | ex_data | 数据结构 | `exception.c:19-40` | 映射 | 存量 14 §2.4 |
| K-243 | 恢复点四类 | 机制 | `exception.c:66,200-230` | 嵌套安全 | 存量 14 §2.2 |
| K-244 | 页错误转发 | 机制 | `exception.c:49-130` | 核心路径 | 存量 14 §2.3 |
| K-245 | 缺页红线 | 约束 | `exception.c:100,167` | 循环禁止 | 存量 14 §1.5 |
| K-246 | 异常触发 cause_sig | 机制 | `system.c:389-449` | 信号产生 | 存量 14 §2.1 |
| K-247 | IRQ hook 链 | 机制 | `interrupt.c:23-176` | 中断处理 | 存量 14 §2.5 |
| K-248 | generic_handler | 机制 | `do_irqctl.c:143-172` | 驱动唤醒 | 存量 14 §4.4 |
| K-249 | ExceptionArch/Dispatcher | 接口 | `exception.rs:40` | Rust 表达 | 存量 14 §3.2 |
| K-250 | IrqManager | 接口 | `irq_manager.rs` | Rust 管理 | 存量 14 §3.6 |
| K-251 | classify_signal/#NM | 机制 | `exception_dispatcher.rs:124,220` | 映射 | 存量 14 §4.5 |
| K-252 | page_fault helpers | 机制 | `page_fault.rs` | 可测拆分 | 存量 14 §4.6 |
| K-253 | 跨架构 IRQ | 演进 | `os/plat/src/*/interrupt.rs` | 对照 | 存量 14 §2.7 |
| K-254 | 边界表 | 接口 | `14 附录 B` | 查阅 | 存量 14 附录 B |
| K-255 | 嵌套调试异常 | 机制 | `exception.c:232-250` | 特例 | 存量 14 §2.1 |

- 验收标准：给定一个异常向量与发生位置（用户/内核/嵌套），能说出进入哪条分支、会改哪些标志、最终回到哪里；能说明页错误为什么必须由 VM 处理，以及 VM 自身缺页为什么 panic；能画出 IRQ 从硬件到驱动 RECEIVE 的完整链路。

### 18-cross-space-runtime

- 一句话定位：内核代行拷贝/写内存时遇到未映射页，如何挂起调用方、请 VM 修页、再恢复执行。
- 讲什么：K-256~K-270。代行缺页问题；三阶段抽象；VMREQUEST 协议；VMSUSPEND 类型；三种挂起类型；与 PAGEFAULT 的区别；`data_copy_vmcheck`；vm_suspend/kernel_call_resume；VmRequestQueue；缺口与 WONTFIX。
- 不讲什么：vircopy/safecopy 系统调用接口与 grant 验证（22）；页错误进入路径（17）；VM 侧修页逻辑（02-stage-vm）；DM 建立（08）。
- 前置：17（页错误）、13（RTS_VMREQUEST 状态）、10（VMCTL_MEMREQ 接口）。
- 后置：24（拷贝系统调用依赖本协议）、14（delivermsg 复用同一挂起类型）。
- 事实底线：
  - C：`arch/i386/memory.c:592-705`；`proc.c:234-258`；`system.c:612-637`；`proto.h:182-185`；`vm.h:6-8`；`proc.h:95-124`；`system/do_vmctl.c:37-110`；`syslib.h:129`。
  - 非 C 制品：`os/kernel/src/{cross_space.rs,vm.rs}`；`os/kernel/src/proc.rs`（suspend_for_vm_with_copy）。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-256 | 代行缺页问题 | 概念 | `memory.c:592` | 开篇 | 存量 24 §1 |
| K-257 | 三阶段抽象 | 概念 | `memory.c:592-666` | 结构 | 存量 24 §1.1 |
| K-258 | VMREQUEST 协议 | 机制 | `proc.c:234-258` | 主体 | 存量 24 §1.2 |
| K-259 | VMSUSPEND 类型分离 | 接口 | `vm.h:6-8` | 返回值 | 存量 24 §1.3 |
| K-260 | 三类挂起 | 约束 | `proc.h:98-101` | 来源 | 存量 24 §1.4 |
| K-261 | VMREQUEST vs PAGEFAULT | 决策 | `proc.h:142-166` | 区分 | 存量 24 §1.5 |
| K-262 | sys_datacopy 澄清 | 接口 | `syslib.h:129` | 常见误解 | 存量 24 §2.1 |
| K-263 | do_copy 算法 | 机制 | `do_copy.c:22-89` | 入口 | 存量 24 §2.4 |
| K-264 | data_copy_vmcheck | 机制 | `memory.c:690-705` | 核心 | 存量 24 §2.5 |
| K-265 | createpde → DM | 演进 | `memory.c:69-147` | 机制差异 | 存量 24 §3 D1 |
| K-266 | vm_suspend | 机制 | `proc.c:234-258` | 挂起 | 存量 24 §2.8 |
| K-267 | kernel_call_resume | 机制 | `system.c:612-637` | 恢复 | 存量 24 §2.8 |
| K-268 | VmRequestQueue | 数据结构 | `os/kernel/src/vm.rs` | Rust 表达 | 存量 24 §3 D6 |
| K-269 | 挂起未入队缺口 | 工具 | `os/kernel/src/cross_space.rs` | 现状 | 存量 24 §4.6 |
| K-270 | 进度报告 WONTFIX | 决策 | `24 §4.6` | 决策 | 存量 24 §4.6 |

- 验收标准：能画出"内核拷贝 → 缺页 → 挂起 → 通知 VM → VM 回复 → 重放"的完整时序，并标出每一步改动的标志；能解释为什么第一次失败要挂起而第二次失败直接 SIGSEGV；能说明 VMREQUEST 与 PAGEFAULT 各自的触发者与恢复者。

### 19-clock-timer

- 一句话定位：每个 tick 内核做什么、定时器队列怎么工作、quantum 为什么不在 tick 里扣。
- 讲什么：K-271~K-286。tick 四类工作；BSP 独占；双重记账；vtimer/闹钟；adjtime；负载平均；定时器队列与到期扫描；`tick_with` 零分配；全局时间镜像；quantum 归属。
- 不讲什么：硬件时钟源初始化（05）；中断入口（11/17）；时钟系统调用接口（27）；AP 定时器注册（20）；调度决策（13/16）。
- 前置：17（时钟中断路径）、13（quantum 与 proc_no_time）、05（ClockState 创建）。
- 后置：27（系统调用消费）、16（quantum 检查）、31（profile 采样）。
- 事实底线：
  - C：`clock.c:70-173,178-292,294-312`；`arch/i386/arch_clock.c:72-74,208-349`；`system/do_setalarm.c:69-76`；`system/do_vtimer.c:81-103`；`lib/libtimers/{tmrs_exp.c,tmrs_set.c}`；`include/minix/timers.h:32-64`；`type.h:98-121`。
  - 非 C 制品：`os/kernel/src/clock.rs`；`os/kernel/src/globals.rs`；`os/arch/src/arch/clock.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-271 | tick 四类工作 | 机制 | `clock.c:70-173` | 主体 | 存量 15 §1.1 |
| K-272 | BSP 独占 | 约束 | `clock.c:91` | 唯一性 | 存量 15 §1.4 |
| K-273 | 双重记账 | 机制 | `clock.c:113-120` | 计费 | 存量 15 §1.4 |
| K-274 | prof 双重递减 | 机制 | `clock.c:134-138` | 采样 | 存量 15 §1.4 |
| K-275 | adjtime | 机制 | `clock.c:97-100,195` | 时间微调 | 存量 15 §1.3 |
| K-276 | 定时器队列 | 数据结构 | `clock.c:37,159` | 载体 | 存量 15 §4.1.2 |
| K-277 | cause_alarm | 机制 | `do_setalarm.c:69-76` | 唤醒 | 存量 15 §2.3 |
| K-278 | vtimer_check | 机制 | `do_vtimer.c:81-103` | 发信号 | 存量 15 §2.3 |
| K-279 | load_update | 机制 | `clock.c:260-292` | 负载 | 存量 15 §4.10 |
| K-280 | tick_with 零分配 | 机制 | `os/kernel/src/clock.rs` | Rust 热路径 | 存量 15 §4.5 |
| K-281 | Timer 类型族 | 数据结构 | `os/kernel/src/clock.rs` | Rust 表达 | 存量 15 §4.1.2 |
| K-282 | 全局时间镜像 | 数据结构 | `os/kernel/src/globals.rs` | 读时间 | 存量 15 §4.1.1 |
| K-283 | quantum 归属 | 决策 | `arch_clock.c:326-330` | 关键纠正 | 存量 15 §1.4 |
| K-284 | arch_timer_int_handler 空 | 机制 | `arch_clock.c:72-74` | 架构钩子 | 存量 15 §2.3 |
| K-285 | tick 返回类型 | 接口 | `os/kernel/src/clock.rs` | Rust 表达 | 存量 15 §4.6 |
| K-286 | load 回绕 | 约束 | `os/kernel/src/clock.rs` | 复刻 | 存量 15 §4.1.3 |

- 验收标准：能按序列出 tick 的六步并与 C 逐行对应；能解释为什么 `timer_int_handler` 不递减 quantum；能说明 BSP 独占与 AP 定时器停止/恢复的关系（引用 20）；能对一个"虚拟定时器与性能定时器同时到期"的场景说明发几个信号。

### 20-smp-bringup

- 一句话定位：第二颗及以后的 CPU 如何醒来、如何进入同一个内核、如何与 BSP 协同调度。
- 讲什么：K-287~K-300。三架构 AP 启动契约；ApBootstrap ABI 与 early entry 梯子；自举内存序；双位图；IPI 两类；smp_schedule_sync；远端 handler；亲和性与迁移；boot_lock 握手；halt/shutdown；AP 定时器；固件 ABI 修复。
- 不讲什么：BKL 与 per-CPU 模型（12）；调度队列本身（13）；VIRQ/中断控制器细节（06）；SMP 下各机制的行为差异（各篇自己标注）。
- 前置：12、06、05、09（fallback 调用点）。
- 后置：23（runctl 的 IPI 使用）、31（测试阶梯）、35（QEMU 多核启动）。
- 事实底线：
  - C：`smp.c:7-23,30-66,75-205`；`arch/i386/arch_smp.c:100-360`；`arch/i386/{trampoline.S:8-57,mpx.S:599-622,apic.c:965-1123,apic_asm.S:7-83}`；`smp.h:31-52`。
  - 非 C 制品：`os/kernel/src/smp.rs:61-331,555-911,1223-1350`；`os/arch/src/arch/smp.rs:46`；`os/arch/src/*/ap_early_entry.rs`；`smp_todo.md`（设计证据）。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-287 | 三架构启动契约 | 接口 | `arch_smp.c:100-170` | 开篇主体 | 存量 16 §1.5 |
| K-288 | ApBootstrap ABI | 数据结构 | `ap_early_entry.rs` | 关键约束 | 新增 smp_todo |
| K-289 | 自举内存序 | 约束 | `os/kernel/src/smp.rs:271` | 正确性 | 存量 16 §3 D9 |
| K-290 | 双位图 | 数据结构 | `os/kernel/src/smp.rs` | 语义区分 | 新增 smp_todo |
| K-291 | IPI 两类 | 接口 | `smp.c:63,75` | 通知 | 存量 16 §1.3 |
| K-292 | smp_schedule_sync | 机制 | `smp.c:75-112` | 同步协议 | 存量 16 §4.8 |
| K-293 | sched_handler | 机制 | `smp.c:156-187` | 远端操作 | 存量 16 §4.7 |
| K-294 | 亲和性/迁移 | 机制 | `smp.c:142-154` | 绑核 | 存量 16 §1.4 |
| K-295 | boot_lock 握手 | 机制 | `smp.c:28-54` | 启动屏障 | 存量 16 §4.11 |
| K-296 | halt/shutdown | 机制 | `smp.c:56-61` | 关机 | 存量 16 §1.5 |
| K-297 | 0x467 取舍 | 决策 | `16 §9.4` | 历史兼容 | 存量 16 §9.4 |
| K-298 | AP 定时器 | 机制 | `clock.c:306-312` | 每 CPU tick | 存量 16 §1.5 |
| K-299 | 调度 IPI | 机制 | `smp.c:194-204` | 抢占 | 存量 16 §4.10 |
| K-300 | 固件 ABI 修复 | 演进 | `os/arch/src/*/ap_early_entry.rs` | 真实 bug | 新增 smp_todo |

- 验收标准：能画出 AP 从 INIT/SIPI（或 PSCI/SBI）到 `switch_to_user` 的完整梯子，并标出每一步所在特权级；能解释 boot_ack 与 online 两个位图各解决什么问题；能说明为什么 `smp_schedule_sync` 必须暂时释放 BKL。必须出现：三架构 AP 启动对照表。

### 21-privilege

- 一句话定位：内核权限模型：priv 结构、掩码、范围表与 SYS_PRIVCTL。
- 讲什么：K-394~K-411。priv 分治；31 字段六组；s_flags；三类掩码与不对称；静态/动态分区与 get_priv；范围表；KPriv 8 子结构；capability 与模板；掩码 newtype；PrivTable；set_sendto_bit 不变量；PRIVCTL 11 子命令；缺口与类型债。
- 不讲什么：过滤执行（22）；fork 降级的具体调用点（23 引用本篇）；与 priv 无关的用户态服务权限（各自 stage）。
- 前置：07（存储模型）、13（进程状态）。
- 后置：22-28 全部依赖。
- 事实底线：
  - C：`priv.h:10-105`；`include/minix/priv.h`；`system.c:274-359,540-660,918-989`；`do_privctl.c:26-371`；`const.h:143-154`。
  - 非 C 制品：`os/kernel/src/{kpriv.rs,capability.rs}`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-394 | priv 分治 | 概念 | `priv.h:94` | 开篇 | 存量 22 §1 |
| K-395 | priv 字段语义 | 数据结构 | `priv.h:21-66` | 字段字典 | 存量 22 §1.1 |
| K-396 | s_flags | 数据结构 | `const.h:143-154` | 标志 | 存量 22 §1.2 |
| K-397 | 三类掩码 | 数据结构 | `priv.h:35-38` | 位图 | 存量 22 §1.3 |
| K-398 | 白名单不对称 | 接口 | `priv.h:86-87` | 异步 self | 存量 22 §1.3 |
| K-399 | 静态/动态分区 | 数据结构 | `priv.h:10-21` | 布局 | 存量 22 §1.4 |
| K-400 | get_priv | 机制 | `system.c:274-302` | 分配 | 存量 22 §1.4 |
| K-401 | fork 降级 | 约束 | `do_fork.c:104` | 安全 | 存量 22 §1.4 |
| K-402 | 范围表 | 数据结构 | `priv.h`（s_io_tab 等） | I/O 权限 | 存量 22 §1.5 |
| K-403 | KPriv 8 子结构 | 演进 | `os/kernel/src/kpriv.rs` | Rust 表达 | 存量 22 §3 D1 |
| K-404 | Capability 编解码 | 演进 | `capability.rs` | wire | 存量 22 §4.1 |
| K-405 | CapabilityTemplate | 接口 | `capability.rs` | 构造 | 存量 22 §3 D3 |
| K-406 | 掩码 newtype | 演进 | `capability.rs` | 类型安全 | 存量 22 §3 D5 |
| K-407 | PrivTable | 数据结构 | `os/kernel/src/kpriv.rs` | 零堆表 | 存量 22 §3 D6 |
| K-408 | 对称性不变量 | 约束 | `system.c:307-330` | 授权成对 | 存量 22 §4.6.1 |
| K-409 | PRIVCTL 11 子命令 | 接口 | `do_privctl.c` | 接口 | 存量 22 §4.7 |
| K-410 | clear_ipc_refs 缺口 | 决策 | `syscall.rs:893` | 未建模 | 存量 22 §4.7 |
| K-411 | 类型债 | 约束 | `os/kernel/src/kpriv.rs` | P2 | 存量 22 §3 D9 |

- 验收标准：能解释"系统进程独立 priv、用户共享 USER_PRIV"的空间/隔离权衡；能说出三类掩码各自检查的时机与消费方（指向 22）；能复述 set_sendto_bit 的双向操作并指出漏掉会坏什么；能对 PRIVCTL 11 个子命令分类（身份/IPC/资源/信号）。


### 22-ipc-filter

- 一句话定位：在消息送达前，基于发送方权限与接收方偏好做放行/拒绝。
- 讲什么：K-412~K-424。三层模型；时机不对称；无 CHECK_IPC；位图原语；k_call 检查；接收反向过滤；过滤链与组合；端点类别；状态报告；Rust 池与链；memreq 过滤。
- 不讲什么：权限结构定义（21）；IPC 原语语义（14）；分派主体（15）；过滤规则的业务含义（各服务）。
- 前置：21、14、15。
- 后置：23-28（每个 handler 的权限检查语境）、18（memreq 过滤）。
- 事实底线：
  - C：`ipc_filter.h:1-73`；`include/minix/ipc_filter.h`；`priv.h:35-38,86-87`；`ipc.h:14-48`；`system.c:103-123,803-874`。
  - 非 C 制品：`os/kernel/src/{ipc_filter.rs,kpriv.rs,syscall.rs}`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-412 | 三层模型 | 概念 | `priv.h`、`ipc.h:14` | 开篇 | 存量 23 §1.1 |
| K-413 | 时机不对称 | 约束 | `system.c:111`、`ipc.h:19` | 关键 | 存量 23 §1.2 |
| K-414 | 无 CHECK_IPC | 决策 | `priv.h:86` | 不可关 | 存量 23 §1.4 |
| K-415 | 位图原语 | 机制 | `const.h:20-27` | 操作 | 存量 23 §2.2 |
| K-416 | k_call 检查顺序 | 机制 | `system.c:103-123` | 拒绝顺序 | 存量 23 §2.4 |
| K-417 | 接收反向过滤 | 接口 | `ipc.h:14-22` | 偏好 | 存量 23 §2.5 |
| K-418 | allow_ipc_filtered_msg | 机制 | `system.c:803-874` | 执行 | 存量 23 §2.6 |
| K-419 | 链式组合 | 约束 | `system.c:853-874` | 覆盖语义 | 存量 23 §2.6 |
| K-420 | EL_MATCH/类别 | 机制 | `ipc_filter.h:19-41` | 规则 | 存量 23 §2.7 |
| K-421 | IPC_STATUS | 机制 | `ipc.h:25-48` | 回传 | 存量 23 §2.8 |
| K-422 | Option 化池/链 | 演进 | `os/kernel/src/ipc_filter.rs` | Rust 表达 | 存量 23 §3 D5 |
| K-423 | IpcFilterType | 接口 | `os/kernel/src/ipc_filter.rs` | 类型 | 存量 23 §3 D8 |
| K-424 | memreq 过滤 | 机制 | `os/kernel/src/ipc_filter.rs` | VM 请求 | 存量 23 §4.5 |

- 验收标准：能给一个 `(src, dst, m_type)` 组合，按三层顺序判断是否送达并给出拒绝错误码；能解释为什么细粒度过滤不放在 send 路径；能说明过滤链后项覆盖前项的一个真实用途。


### 23-syscall-process

- 一句话定位：驱动进程在"未出生 → 就绪 → 运行 → 停止 → 死亡 → 槽位回收"之间转换的七条调用弧。
- 讲什么：K-301~K-316。生命周期四弧；exec 替换、exit 委托、clear 幂等；同步 fork；endpoint 代际；降级与 VMINHIBIT；runctl 与 SMP IPI；stop-delay；schedctl 双模式；do_schedule；statectl 五请求；Rust 实现要点。
- 不讲什么：调度原语（13）；信号处理协议（25）；权限结构（21）；页表与地址空间释放细节（08 与 VM stage）；fork 在 VM/PM 侧的配合（对应 stage）。
- 前置：13、15、18（clear 可能触发 VM 请求）。
- 后置：21（privctl 协作）、25（exit 触发信号）。
- 事实底线：
  - C：`system/{do_fork.c:26-136,do_exec.c:20-60,do_exit.c:14-27,do_clear.c:17-80,do_runctl.c:18-76,do_schedctl.c:7-46,do_statectl.c:15-53,do_schedule.c:8-30}`；`system.c:642-723`；`smp.c:114-140`。
  - 非 C 制品：`os/kernel/src/{syscall_process.rs,proc.rs}`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-301 | 生命周期四弧 | 概念 | `do_fork.c` 等 | 开篇 | 存量 17 §1.1 |
| K-302 | exec 替换非新建 | 概念 | `do_exec.c:20-60` | 语义关键 | 存量 17 §1.1 |
| K-303 | exit 委托信号 | 机制 | `do_exit.c:14-27` | 三段式 | 存量 17 §1.1 |
| K-304 | clear 幂等 | 约束 | `do_clear.c:17-80` | 防二次释放 | 存量 17 §1.1 |
| K-305 | 同步 fork 前提 | 约束 | `do_fork.c:51` | 前提 | 存量 17 §1.2 |
| K-306 | fork 代际递增 | 机制 | `do_fork.c:59,69-72` | 安全 | 存量 17 §1.2 |
| K-307 | 继承清单 | 约束 | `do_fork.c:FORKSTR` | 初值 | 存量 17 §1.2 |
| K-308 | 特权降级 | 机制 | `do_fork.c:105-122` | 安全 | 存量 17 §1.3 |
| K-309 | VMINHIBIT | 机制 | `do_fork.c:115` | 可运行条件 | 存量 17 §1.3 |
| K-310 | runctl/IPI | 机制 | `do_runctl.c`、`smp.c:114` | SMP 停止 | 存量 17 §1.4 |
| K-311 | stop-delay | 机制 | `do_runctl.c:44` | 延迟 | 存量 17 §1.4 |
| K-312 | schedctl | 机制 | `do_schedctl.c`、`system.c:642` | 调度权 | 存量 17 §2.6 |
| K-313 | do_schedule | 接口 | `do_schedule.c:8-30` | 缺口补齐 | 新增 |
| K-314 | statectl 5 请求 | 接口 | `do_statectl.c:15-53` | IPC 状态控制 | 存量 17 §2.7 |
| K-315 | fork_from/clear 9 步 | 演进 | `os/kernel/src/proc.rs` | Rust 表达 | 存量 17 §3/§4 |
| K-316 | clear 幂等修正 | 测试 | `syscall_process.rs` | 案例 | 存量 17 §5.2 |

- 验收标准：对每条弧能说出输入校验、改动的字段、失败错误码；能解释 fork 为什么要求父进程 RTS_RECEIVING；能说明 clear 幂等对 PM 重试的意义；能画出 exit→cause_sig→clear 的三段式时序。


### 24-syscall-copy

- 一句话定位：五种跨地址空间拷贝原语与 grant 授权模型。
- 讲什么：K-317~K-330。信任分层；vircopy/physcopy 与 SELF；CP_FLAG_TRY；grant 表与三类型；verify_grant 11 步；间接链；软故障；umap/vumap；memset；do_setgrant；Rust 类型与 PteWalk；端到端缺口。
- 不讲什么：VMREQUEST 挂起协议（18）；Direct Map 建立（08）；权限范围表（21）；safecopy 的调用者身份细节（21/22）。
- 前置：18、15、08。
- 后置：23（fork 用 vircopy）、25（SIGSEND 用 grant）、26（SDEVIO 用 grant）。
- 事实底线：
  - C：`system/{do_copy.c:22-91,do_safecopy.c:60-448,do_umap.c:25-39,do_umap_remote.c:26-122,do_vumap.c:22-131,do_memset.c:17-28,do_safememset.c:20-57,do_setgrant.c:15-30}`；`const.h`（MAPVEC_NR）；`priv.h`（s_grant_*）。
  - 非 C 制品：`os/kernel/src/{syscall_copy.rs,grant.rs,cross_space.rs}`；`os/arch/src/arch/pte_walk_arch.rs:60`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-317 | 信任分层 | 概念 | `do_copy.c`、`do_safecopy.c` | 开篇 | 存量 18 §1 |
| K-318 | SELF 与共用 handler | 机制 | `do_copy.c:64-66` | 入口 | 存量 18 §1.1 |
| K-319 | CP_FLAG_TRY | 约束 | `do_copy.c:80` | VFS 死锁 | 存量 18 §1.1 |
| K-320 | grant 表/三类型 | 数据结构 | `do_safecopy.c`、`priv.h` | 授权模型 | 存量 18 §1.2 |
| K-321 | verify_grant 11 步 | 机制 | `do_safecopy.c:60-215` | 核心 | 存量 18 §1.2 |
| K-322 | 间接链上限 | 约束 | `do_safecopy.c:148` | 防循环 | 存量 18 §1.2 |
| K-323 | CPF_TRY 软故障 | 机制 | `do_safecopy.c:258` | ABA | 存量 18 §1.2 |
| K-324 | umap 双态 | 接口 | `do_umap.c:25`、`do_umap_remote.c:26` | DMA | 存量 18 §1.3 |
| K-325 | vumap 批量 | 机制 | `do_vumap.c`、`const.h` | DMA 批量 | 存量 18 §1.3 |
| K-326 | memset 语义 | 机制 | `do_memset.c`、`do_safememset.c:56` | 填充 | 存量 18 §1.4 |
| K-327 | do_setgrant | 接口 | `do_setgrant.c:15-30` | 缺口补齐 | 新增 |
| K-328 | GrantVerifyResult 等 | 演进 | `grant.rs` | Rust 表达 | 存量 18 §3 |
| K-329 | VmCopyError/PteWalk | 接口 | `vm.rs:98`、`pte_walk_arch.rs:60` | 底层 | 存量 18 §4.1 |
| K-330 | 接线与缺口 | 测试 | `syscall_copy.rs` | 现状 | 存量 18 §4.8 |

- 验收标准：能回答"vircopy 为什么不需要验证权限"与"safecopy 的 11 步里哪一步防重放"；能画出 grant 间接链的解析路径；能说明 `CPF_TRY` 与 `CP_FLAG_TRY` 的区别与各自触发条件。必须出现：11 步验证表、grant 三类型对照表。


### 25-syscall-signal

- 一句话定位：内核作为信号中介：产生、挂起、通知管理器、支持用户态处理器往返。
- 讲什么：K-331~K-345。双路径与推拉模型；三步闭环；状态三件套；SELF 路径；SIGSEND 时序；信号管理器与致命规则；stop-delay；cause_signal 实现；do_getksig/endksig；SigSet 边界；SignalContext；调试/投递失败信号；DIAGCTL 结论。
- 不讲什么：异常到 cause_sig 的入口（17）；trap_style 机制（11）；FPU 信号保存（29）；PM 侧信号处理实现（04-stage-pm）。
- 前置：17、13、15（trap_style 由 11 引入）。
- 后置：28（sigsend 相关诊断）、29（信号帧 FPU 边界）。
- 事实底线：
  - C：`system/{do_kill.c:17-41,do_getksig.c:18-43,do_endksig.c:15-41,do_sigsend.c:19-166,do_sigreturn.c:19-98}`；`system.c:364-464`；`signal.h`（SIGKSIG 等）；`archconst.h`（SC_MAGIC）。
  - 非 C 制品：`os/kernel/src/{syscall_signal.rs,proc.rs}`；`os/arch/src/arch/signal_context.rs:159`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-331 | 双路径推拉 | 概念 | `system.c:389` | 开篇 | 存量 19 §1 |
| K-332 | 三步闭环 | 机制 | `do_getksig.c`、`do_endksig.c` | 内核路径 | 存量 19 §1.1 |
| K-333 | 状态三件套 | 数据结构 | `proc.h:142`、`SigSet` | 状态机 | 存量 19 §1.1 |
| K-334 | SELF 路径 | 机制 | `system.c:416-435` | 自管理 | 存量 19 §1.1 |
| K-335 | SIGSEND 时序 | 约束 | `do_sigsend.c:126` | 不可逆 | 存量 19 §1.3 |
| K-336 | 管理器与致命 | 数据结构 | `system.c:412-432` | 提升规则 | 存量 19 §1.4 |
| K-337 | stop-delay | 机制 | `do_runctl.c:44`、`system.c:454` | 延迟 | 存量 19 §1.5 |
| K-338 | cause_sig 完整机制 | 机制 | `syscall_signal.rs` | Rust 实现 | 存量 19 §4.3 |
| K-339 | is_lethal | 约束 | `syscall_signal.rs` | 六信号 | 存量 19 §4.2 |
| K-340 | 三函数校验 | 机制 | `do_kill.c` 等 | 拒绝条件 | 存量 19 §2.3 |
| K-341 | SigSet 缺口 | 演进 | `os/kernel/src/proc.rs` | 位图边界 | 存量 19 §4.8 |
| K-342 | SignalContext | 接口 | `signal_context.rs:159` | 跨架构 | 存量 19 §4.6 |
| K-343 | trap_style 校验/BUG | 决策 | `do_sigsend.c:79` | 安全修复 | 存量 19 §4.6.1 |
| K-344 | SC_TRACE/Segfault | 机制 | `proc.c:392,271` | 附加信号 | 存量 19 §4.7 |
| K-345 | DIAGCTL SIGKMESS no-op | 决策 | `19 §4.7` | 结论 | 存量 19 §4.7 |

- 验收标准：能画出一条内核信号从产生到管理器确认的完整链，并标出哪些进程在何时不可调度；能解释 SIGSEND 把 trap_style 盖进信号帧、SIGRETURN 写回的意义；能说明普通信号与致命信号的路径分叉点。


### 26-syscall-device

- 一句话定位：用户态驱动如何安全地收到中断并访问 I/O 端口。
- 讲什么：K-346~K-360。IRQ 钩子与四子请求；owner 校验与进程退出摘钩；三层端口 I/O；type/dir 解码；权限与对齐；VDEVIO 批量；SDEVIO 跨进程；IOPENABLE；READBIOS；PortIo trait；接线与语义偏移。
- 不讲什么：IRQ hook 链与 generic_handler 的通用机制（17 已完整讲，本篇只讲驱动接口）；中断控制器初始化（06）；设备驱动本身（16-stage-drivers）；VMCTL（10）。
- 前置：15、17（hook 机制）、21（CHECK_IO_PORT / CHECK_IRQ 字段定义）。
- 后置：16-stage-drivers（驱动侧使用）。
- 事实底线：
  - C：`system/{do_irqctl.c:23-174,do_devio.c:19-107,do_vdevio.c:25-165}`；`arch/i386/{do_sdevio.c:24-163,do_iopenable.c:19-35,do_readbios.c:15-37}`；`priv.h`（s_io_tab/s_irq_tab）；`com.h`（DEVIO 字段）。
  - 非 C 制品：`os/kernel/src/{syscall_device.rs,irq_manager.rs}`；`os/plat/src/port_io.rs:26`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-346 | 钩子三元组 | 数据结构 | `do_irqctl.c:23` | 接口核心 | 存量 20 §1.1 |
| K-347 | 四子请求 | 接口 | `do_irqctl.c` | 生命周期 | 存量 20 §1.1 |
| K-348 | notify_id 上限 | 约束 | `do_irqctl.c:88` | 边界 | 存量 20 §1.1 |
| K-349 | owner 校验 | 约束 | `do_irqctl.c:46,126` | 安全 | 存量 20 §1.1 |
| K-350 | 退出摘钩不变量 | 约束 | `do_irqctl.c:160` | 安全 | 存量 20 §1.1 |
| K-351 | 三层端口 I/O | 概念 | `do_devio.c` 等 | 分类 | 存量 20 §1.2 |
| K-352 | type/dir 解码 | 接口 | `com.h` | 请求解析 | 存量 20 §1.2 |
| K-353 | 权限与对齐 | 约束 | `do_devio.c:44,62` | 拒绝 | 存量 20 §1.2 |
| K-354 | VDEVIO 批量 | 机制 | `do_vdevio.c:67-156` | 批量 | 存量 20 §2.3 |
| K-355 | SDEVIO 跨进程 | 机制 | `do_sdevio.c:40,96` | 安全变体 | 存量 20 §2.4 |
| K-356 | IOPENABLE | 机制 | `do_iopenable.c` | 提权 | 存量 20 §1.3 |
| K-357 | READBIOS 范围 | 约束 | `do_readbios.c:15` | 边界 | 存量 20 §1.3 |
| K-358 | PortIo trait | 接口 | `port_io.rs:26` | 跨架构 | 存量 20 §3 D2 |
| K-359 | 接线与 BadCall | 工具 | `syscall.rs` | 现状 | 存量 20 §4.8 |
| K-360 | 语义偏移 | 决策 | `20 §4.9` | 差异 | 存量 20 §4.9 |

- 验收标准：对一个 IRQ 注册请求逐字段说明校验与失败码；能画出硬件中断到驱动 `RECEIVE` 的通知路径（与 17 分工明确）；能说出三种端口 I/O 的适用场景；能列出 x86 特有的五个调用及非 x86 行为。


### 27-syscall-clock

- 一句话定位：用户态可见的时间服务：查询、闹钟、墙钟设置与虚拟/性能定时器。
- 讲什么：K-361~K-373。五个调用与权限矩阵；三时钟源；TIMES 的目标语义；闹钟相对/绝对与 time_left；STIME/SETTIME；vtimer 两类；Rust 类型与参数传递；原子定时器字段；权限检查。
- 不讲什么：tick 处理与队列实现（19）；时钟初始化（05）；系统时间记账（19）；SCHED 服务如何使用定时器（06-stage-sched）。
- 前置：19、15。
- 后置：06-stage-sched（用户态调度器使用闹钟/vtimer）。
- 事实底线：
  - C：`system/{do_times.c:22-46,do_setalarm.c:22-78,do_stime.c:15-19,do_settime.c:18-58,do_vtimer.c:21-103}`；`com.h:420-421`。
  - 非 C 制品：`os/kernel/src/{syscall_clock.rs,clock.rs,proc.rs}`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-361 | 五调用与权限 | 概念 | `do_times.c` 等 | 开篇 | 存量 21 §1 |
| K-362 | 三时钟源 | 概念 | `do_times.c:40-42` | 语义核心 | 存量 21 §1.1 |
| K-363 | TIMES 目标 | 机制 | `do_times.c:33-35` | 查询语义 | 存量 21 §1.1 |
| K-364 | s_alarm_timer | 数据结构 | `do_setalarm.c:36` | 载体 | 存量 21 §1.2 |
| K-365 | 时间参数三分支 | 机制 | `do_setalarm.c:36-62` | 语义 | 存量 21 §1.2 |
| K-366 | STIME/SETTIME | 机制 | `do_stime.c`、`do_settime.c` | 设置 | 存量 21 §1.3 |
| K-367 | REALTIME 限制 | 约束 | `do_settime.c:25,43` | 边界 | 存量 21 §1.3 |
| K-368 | vtimer 两类 | 接口 | `do_vtimer.c`、`com.h:420` | 接口 | 存量 21 §1.4 |
| K-369 | VtimerType/NotifyAlarm | 演进 | `syscall_clock.rs` | Rust 表达 | 存量 21 §3 |
| K-370 | ClockState 传参 | 决策 | `syscall_clock.rs` | 可测性 | 存量 21 §3 D5 |
| K-371 | TimeStats | 数据结构 | `os/kernel/src/proc.rs` | 原子字段 | 存量 21 §3 D6 |
| K-372 | 权限检查修正 | 约束 | `syscall_clock.rs` | fail-closed | 存量 21 §4.6 |
| K-373 | adjtime 保留 | 决策 | `do_settime.c:29` | POSIX | 存量 21 §3 D4 |

- 验收标准：能对每个调用说出成功的输入范围与失败码；能解释 monotonic/realtime/boottime 三者的可设置性与用途；能说明 `time_left` 计算的溢出安全策略。必须出现：五调用权限矩阵、三时钟源对照表。


### 28-syscall-info-and-misc

- 一句话定位：内核信息出口、追踪、热更新、采样、机器上下文、诊断，以及"未移植"的诚实边界。
- 讲什么：K-374~K-393。未移植三语义；GETINFO 19 子请求；TRACE 13 子请求与段寄存器保护；UPDATE 7 步与资源继承；SPROF 状态机；BadCall/ENOSYS 边界；krandom；GET_MONPARAMS 现状；mcontext；diagctl；死代码。
- 不讲什么：各子请求消费方（用户态服务）的实现；profile 采样内核侧（31）；stacktrace 机制（30）；panic 编排（32）；错误码总表（36）。
- 前置：15、18、21、24（拷贝）。
- 后置：31（采样实现细节）、32（诊断与关机）、各用户态 stage。
- 事实底线：
  - C：`system/{do_getinfo.c:47-228,do_trace.c:20-208,do_update.c:37-340,do_sprofile.c:36-132,do_mcontext.c:23-106,do_diagctl.c:18-68}`；`profile.c:75-110`；`com.h:316-339`。
  - 非 C 制品：`os/kernel/src/{misc.rs,krandom.rs,syscall.rs}`；`os/arch/src/arch/signal_context.rs`（Mcontext）。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-374 | 未移植三语义 | 概念 | `25 §1.2` | 开篇 | 存量 25 §1.2 |
| K-375 | 部分实现原则 | 决策 | `os/kernel/src/misc.rs` | 策略 | 存量 25 §1.2 |
| K-376 | 四类分组 | 概念 | `do_getinfo.c` 等 | 结构 | 存量 25 §1.3 |
| K-377 | GETINFO 19 子请求 | 接口 | `do_getinfo.c:47-228` | 主体 | 存量 25 §2.2 |
| K-378 | GET_WHOAMI | 机制 | `do_getinfo.c` | 特例 | 存量 25 §2.2 |
| K-379 | 未处理 GET_* | 约束 | `com.h:316-339` | 边界 | 存量 25 §2.2 |
| K-380 | 长度/E2BIG | 约束 | `do_getinfo.c` | 校验 | 存量 25 §2.2 |
| K-381 | TRACE 13 子请求 | 接口 | `do_trace.c:20-208` | 追踪 | 存量 25 §2.3 |
| K-382 | 段寄存器禁写 | 约束 | `do_trace.c:95-141` | 安全 | 存量 25 §2.3 |
| K-383 | UPDATE 7 步 | 机制 | `do_update.c:37-340` | 热更新 | 存量 25 §2.4 |
| K-384 | 资源继承 | 机制 | `do_update.c:94-107` | 继承 | 存量 25 §4.4 |
| K-385 | 槽交换修正 | 机制 | `do_update.c:292` | 修正 | 存量 25 §4.4 |
| K-386 | SPROF 状态机 | 机制 | `do_sprofile.c:36-132` | 采样接口 | 存量 25 §2.5 |
| K-387 | BadCall vs ENOSYS | 接口 | `syscall.rs:444` | 错误约定 | 存量 25 §3.2 |
| K-388 | krandom | 数据结构 | `os/kernel/src/krandom.rs` | 熵源 | 存量 25 §4.7 |
| K-389 | read_tsc 偏离 | 演进 | `os/kernel/src/krandom.rs` | 行为差异 | 存量 25 §4.7 |
| K-390 | GET_MONPARAMS | 工具 | `do_getinfo.c:143` | 缺口 | 存量 25 §3.3 |
| K-391 | mcontext | 接口 | `do_mcontext.c` | 缺口补齐 | 新增 |
| K-392 | diagctl 子命令 | 接口 | `do_diagctl.c` | 缺口补齐 | 新增 |
| K-393 | dispatch_unused 清理 | 工具 | `os/kernel/src/misc.rs` | 死代码 | 存量 25 §4.6 |

- 验收标准：能对 GETINFO 的 19 个子请求各说一句用途与数据源；能说明"部分实现"与 stub 的区别并举一例；能画出 UPDATE 的资源继承与槽交换步骤；能说出哪些调用当前返回 EINVAL/ENOSYS 及原因。

### 29-fpu-context-switching

- 一句话定位：FPU 寄存器组的所有权如何在进程之间转移，以及"惰式切换"如何把成本与真实使用挂钩。
- 讲什么：K-425~K-435。FPU 状态组成；eager/lazy；CR0.TS 与 #NM；fpu_owner 协议；保存/恢复/释放；`copr_not_available_handler`；FpuArch trait 与三架构 State；FpuTrap 分发；信号路径缺口。
- 不讲什么：通用寄存器上下文切换（11/16）；异常分发框架（17）；SMP 远程保存的 IPI 机制（20）；sigframe 布局（25/36）。
- 前置：16（切换点）、17（#NM 分发）、12（per-CPU）。
- 后置：25（信号保存边界）、30（嵌套 fxrstor 恢复点）、31（SAVE_CTX）。
- 事实底线：
  - C：`arch/i386/arch_system.c:51-215,614`；`proc.c:1922-1968`；`arch/i386/exception.c:375-383`；`arch/i386/mpx.S:537-552`；`include/arch/i386/include/fpu.h:6-48`；`system/do_sigsend.c:86`。
  - 非 C 制品：`os/arch/src/arch/fpu_arch.rs:59`；`os/arch/src/{x86_64,arm64,riscv64}/fpu.rs`；`os/kernel/src/smp.rs`（fpu_owner/SAVE_CTX）；`os/arch/src/arch/exception_dispatcher.rs:61,124`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-425 | FPU 状态组成 | 数据结构 | `fpu.h:6-48` | 对象 | 存量 31 §1.1 |
| K-426 | eager vs lazy | 决策 | `arch_system.c:51-215` | 策略 | 存量 31 §1.2 |
| K-427 | TS/#NM | 机制 | `exception.c:375`、`mpx.S:537` | 硬件信号 | 存量 31 §1.3 |
| K-428 | fpu_owner | 约束 | `cpulocals.h`、`os/kernel/src/smp.rs:163` | 不变量 | 存量 31 §1.4 |
| K-429 | 保存/恢复 | 机制 | `arch_system.c:88-203` | 实现 | 存量 31 §2.3 |
| K-430 | release_fpu | 机制 | `proc.c:1961` | 释放 | 存量 31 §2.6 |
| K-431 | #NM handler | 机制 | `proc.c:1922` | 惰式主体 | 存量 31 §2.7 |
| K-432 | FpuArch/State | 接口 | `fpu_arch.rs:59` | Rust 表达 | 存量 31 §3 D1 |
| K-433 | FpuTrap 分发 | 接口 | `exception_dispatcher.rs:61` | 接线 | 存量 31 §4.4 |
| K-434 | 信号路径边界 | 约束 | `do_sigsend.c:86`、`do_sigreturn.c:85` | 64 位边界 | 存量 31 §1.6 |
| K-435 | D5/D6 缺口 | 工具 | `31 §4.5` | 现状 | 存量 31 §4.5 |

- 验收标准：能解释 lazy 策略下"每个进程看到的 FPU 状态始终是自己上次离开时的状态"这一等价性；能说明 TS 位、fpu_owner、保存区三者的关系；能给出一张"切换 / 首次使用 / 信号 / 进程死亡"四个时机的动作表。

### 30-stack-tracing

- 一句话定位：沿帧指针链还原调用栈，并能跨地址空间读取目标进程的栈。
- 讲什么：K-436~K-443。三场景；帧指针对布局；内核栈与用户栈读取权限；KTS 分流；循环/截断/故障边界；StacktraceArch；统一 helper；DIAGCTL STACKTRACE。
- 不讲什么：panic 编排（32）；trap frame 布局（11）；DIAGCTL 其他子命令（28）；符号解析（外部工具）。
- 前置：11（上下文）、17（崩溃路径）。
- 后置：28（诊断调用）、32（panic 回溯）。
- 事实底线：
  - C：`arch/i386/exception.c:287-373`（proc_stacktrace 家族）；`system/do_diagctl.c`（STACKTRACE）；`utility.c`（util_stacktrace）；`libsys/stacktrace.c:17`。
  - 非 C 制品：`os/arch/src/arch/stacktrace.rs:41`；`os/arch/src/{x86_64,arm64,riscv64}/boot.rs`（StacktraceArch impl）；`os/kernel/src/stacktrace.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-436 | 三场景 | 概念 | `exception.c:155`、`utility.c` | 开篇 | 存量 32 §1.1 |
| K-437 | 帧指针链 | 机制 | `exception.c:287-330` | 主体 | 存量 32 §1.2 |
| K-438 | 读取权限 | 约束 | `exception.c`（PRCOPY） | 跨空间 | 存量 32 §1.3 |
| K-439 | KTS 分流 | 机制 | `exception.c:333-341` | 起点差异 | 存量 32 §2.1 |
| K-440 | 硬边界 | 约束 | `exception.c:287-330` | 防死循环 | 存量 32 §1.5 |
| K-441 | StacktraceArch | 接口 | `stacktrace.rs:41` | Rust 表达 | 存量 32 §3 D1 |
| K-442 | 统一 helper | 机制 | `os/kernel/src/stacktrace.rs` | 出口 | 存量 32 §4.6 |
| K-443 | DIAGCTL STACKTRACE | 接口 | `syscall.rs`（dispatch_diagctl） | 用户路径 | 存量 32 §4.5 |

- 验收标准：给出一个内存布局图，说明 `fp`、`[fp]`、`[fp+8]` 分别是什么；能说明用户栈回溯为什么要经过跨空间读取而不是直接解引用；能列出三个会导致回溯提前终止的条件。

### 31-kernel-observability

- 一句话定位：内核观测自身的三类手段：调度队列自检、统计采样、调试打印与硬件断点。
- 讲什么：K-444~K-456。runqueues_ok；标志字符串化；print_proc；IPC dump/统计 WONTFIX；条件编译模型替代；profile 四组件与分类；采样缓冲；NMI 替代；profile 接口；C typo 复刻；硬件断点；MF_SPROF_SEEN。
- 不讲什么：调度队列正常逻辑（13）；IPC 正常逻辑（14）；采样系统调用接口（28）；panic 输出（32）。
- 前置：13、14、20（SMP 队列检查范围）。
- 后置：28（SPROF 接口）、32（诊断输出）、35（测试使用 runqueues_ok）。
- 事实底线：
  - C：`debug.c:14,110,136-174,431-563`；`profile.c:75-110`；`arch/i386/breakpoints.c`；`arch/i386/debugreg.S`；`debugreg.h`；`proc.h:258`（MF_SPROF_SEEN）。
  - 非 C 制品：`os/kernel/src/{debug.rs,clock.rs,misc.rs,globals.rs}`；`os/plat/src/mock.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-444 | runqueues_ok | 机制 | `debug.c:14-110`、`debug.rs:47,188` | 自检 | 存量 29 §1.1 |
| K-445 | 标志字符串化 | 工具 | `debug.rs:206,236` | 调试 | 存量 29 §2.2 |
| K-446 | print_proc | 机制 | `debug.rs:256` | 打印 | 存量 29 §1.1 |
| K-447 | IPC dump/统计 WONTFIX | 决策 | `debug.c`（DEBUG_*） | 边界 | 存量 29 §3 |
| K-448 | 条件编译替代 | 演进 | `debug.h` | 演进 | 存量 29 §1.2 |
| K-449 | profile 四组件 | 机制 | `profile.c` | 采样 | 存量 30 §1.1 |
| K-450 | 样本分类 | 机制 | `profile.c:75-110` | 语义 | 存量 30 §1.1 |
| K-451 | 采样缓冲 | 数据结构 | `os/kernel/src/misc.rs` | 边界 | 存量 30 §3 D4 |
| K-452 | NMI 替代 | 决策 | `profile.c:nmi_sprofile_handler` | 方案 | 存量 30 §3 D3 |
| K-453 | profile 接口 | 接口 | `os/kernel/src/clock.rs:289` | 接线 | 存量 30 §4.1 |
| K-454 | C typo 复刻 | 决策 | `profile.c:84-89` | 忠实边界 | 存量 30 §4.2 |
| K-455 | 硬件断点 | 工具 | `breakpoints.c`、`debugreg.S` | 缺口补齐 | 新增 |
| K-456 | MF_SPROF_SEEN | 机制 | `proc.h:258` | 去重 | 存量 30 §4.2 |

- 验收标准：能说明 `runqueues_ok` 验证哪些字段、多久跑一次；能解释采样 profiling 如何用统计推断 CPU 分布；能说出 NMI 被哪种机制替代以及代价；能说明硬件断点与 SYS_TRACE 的关系。

### 32-kernel-error-and-shutdown

- 一句话定位：内核失败时的三条出路（返回错误、挂起、panic）与最终关机的完整路径。
- 讲什么：K-457~K-468。错误路径三分类与 errno 原则；panic 三件套与重入保护；kputc/kmess 与 log 演进；_exit 守卫；SYS_ABORT 与关机序列；reset/halt；panic 诊断 hook；与 DIAGCTL 的分工。
- 不讲什么：具体 handler 的错误码（各篇自带错误表）；信号终止进程（25）；关机对用户态服务的通知（PM 侧）；异常分发框架（17）。
- 前置：15（返回约定）、11（panic 时的上下文）。
- 后置：26（diagctl）、30（回溯被 panic 使用）。
- 事实底线：
  - C：`utility.c:17-93`；`system/do_abort.c:16-29`；`main.c:333-403`（shutdown 路径）；`arch/i386/arch_reset.c`；`type.h:173-174`；`const.h:30`（END_OF_KMESS）；`include/minix/errno.h`。
  - 非 C 制品：`os/kernel/src/{errno.rs,kmess.rs,lib.rs}`；`os/plat/src/early_console.rs`；`os/libs/minix-types/src/types/errno.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-457 | 错误三分类 | 概念 | `syscall.rs`（KcallResult） | 开篇框架 | 新增 |
| K-458 | errno 原则 | 约束 | `errno.h`、`errno.rs` | 硬规则 | 新增 |
| K-459 | panic 三件套 | 机制 | `utility.c:22-50` | 主体 | 存量 27 §2.2 |
| K-460 | ARE_PANICING | 机制 | `utility.c:17` | 重入 | 存量 27 §2.2 |
| K-461 | kputc/kmess | 数据结构 | `utility.c:55-83` | 历史方案 | 存量 27 §2.3 |
| K-462 | SIGKMESS → log | 演进 | `os/kernel/src/kmess.rs` | 演进 | 存量 27 §3.2 |
| K-463 | _exit 守卫 | 约束 | `utility.c:88-93` | 契约 | 存量 27 §2.4 |
| K-464 | SYS_ABORT | 机制 | `do_abort.c:16-29` | 缺口补齐 | 新增 |
| K-465 | reset/halt | 机制 | `arch_reset.c` | 缺口补齐 | 新增 |
| K-466 | minix_shutdown 现状 | 工具 | `os/kernel/src/lib.rs` | 现状 | 存量 27 §6.3 |
| K-467 | panic 诊断 hook | 演进 | `os/kernel/src/lib.rs` | 演进 | 存量 27 §4.1 |
| K-468 | DIAGCTL 分工 | 接口 | `do_diagctl.c` | 边界 | 存量 27/32 |

- 验收标准：能对一个失败的 handler 判断该返回错误、挂起还是 panic；能复述 `minix_shutdown` 的调用条件与硬件动作；能说明 kmess 被移除后内核消息走哪条路；能区分 `EDONTREPLY`、`VMSUSPEND`、`EINVAL` 三类返回的含义。

### 33-watchdog

- 一句话定位：用 NMI 检测内核自身是否还活着——以及为什么本重写不做它。
- 讲什么：K-469~K-475。NMI 与 lockup 检测原理；lockup_check 判定；handler 双角色；arch_watchdog 操作表；双重门控；WONTFIX 五理由；SMP static 缺陷。
- 不讲什么：NMI profiling（31/28 的 SPROF PROF_NMI）；中断控制器（06）；调度器活性（13）。
- 前置：19（tick 计数）、17（NMI 入口）。
- 后置：无（支线终点）。
- 事实底线：
  - C：`watchdog.c:10-112`；`watchdog.h:1-50`；`arch/i386/arch_watchdog.c:53-105`；`main.c:455-456`。
  - 非 C 制品：`os/kernel/src/misc.rs`（PROF_NMI→ENOSYS）。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-469 | NMI 原理 | 概念 | `watchdog.c` | 开篇 | 存量 26 §1.1 |
| K-470 | lockup_check | 机制 | `watchdog.c:14` | 判定 | 存量 26 §2.2 |
| K-471 | handler 双角色 | 机制 | `watchdog.c` | 复用 | 存量 26 §2.3 |
| K-472 | 操作表 | 接口 | `watchdog.h` | 架构接入 | 存量 26 §2.5 |
| K-473 | 双重门控 | 约束 | `main.c:455-456` | 默认关 | 存量 26 §1.2 |
| K-474 | WONTFIX 五理由 | 决策 | `26 §1.2` | 决策 | 存量 26 §1.2 |
| K-475 | SMP static 缺陷 | 决策 | `watchdog.c:10` | C 问题 | 存量 26 §6.3 |

- 验收标准：能解释为什么 NMI 是唯一能在 `cli` 下到达的中断；能说明判定 lockup 的阈值与防误报措施；能对"未来若要实现"给出一条基于 ClockArch 模式的接入路径。

### 34-usermapped-data

- 一句话定位：内核数据如何曾被直接映射给用户态，以及 64 位重写为何改为系统调用获取。
- 讲什么：K-476~K-485。两种暴露策略；.usermapped 两段；八个结构体；三套 IPC trampoline；arch_phys_map；kuserinfo ABI；MINIX_KERNINFO 半保留；KERNINFO 页；ClockState/loadinfo 内部化；kmessages 缺口。
- 不讲什么：系统调用获取路径的实现（28）；时钟内部状态（19）；trap 入口（11）。
- 前置：02（段位置）、07（kinfo 消费）。
- 后置：28（GETINFO）、36（线格式）。
- 事实底线：
  - C：`usermapped_data.c:1-15`；`arch/i386/usermapped_data_arch.c:1-33`；`arch/i386/usermapped_glo_ipc.S:1-108`；`kernel.lds:24-28`；`arch/i386/memory.c:746-847`；`include/minix/type.h:98-121`；`ipcconst.h:12`；`proc.c:685-694`。
  - 非 C 制品：`os/kernel/src/kerninfo.rs:54,164`；`os/libs/minix-boot/src/kernel_info.rs`；`os/kernel/src/clock.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-476 | 两种暴露策略 | 决策 | `kernel.lds:24`、`do_getinfo.c` | 开篇权衡 | 存量 28 §1.1 |
| K-477 | .usermapped 段 | 数据结构 | `kernel.lds:24-28` | 段机制 | 存量 28 §1.2 |
| K-478 | 八结构体 | 数据结构 | `usermapped_data.c`、`type.h:98` | 清单 | 存量 28 §2.1 |
| K-479 | 三套 trampoline | 机制 | `usermapped_glo_ipc.S:17-104` | 入口 | 存量 28 §1.3 |
| K-480 | arch_phys_map | 机制 | `memory.c:746-847` | 映射 | 存量 28 §2.4 |
| K-481 | kuserinfo ABI | 数据结构 | `usermapped_data.c` | 用户栈 | 存量 28 附录 A |
| K-482 | MINIX_KERNINFO 半保留 | 演进 | `ipcconst.h:12`、`proc.c:685` | 演进 | 存量 28 §3.7 |
| K-483 | KERNINFO 页 | 机制 | `kerninfo.rs:54` | 替代 | 存量 28 §4.1 |
| K-484 | ClockState 内部化 | 演进 | `os/kernel/src/clock.rs` | 替代 | 存量 28 §3.3 |
| K-485 | kmessages 缺口 | 工具 | `28 §4.2` | 现状 | 存量 28 §4.2 |

- 验收标准：能解释 32 位为什么用共享映射、64 位为什么改用系统调用；能画出 `.usermapped` 段在镜像里的位置与映射权限；能说清 MINIX_KERNINFO 调用与 GETINFO 的分工。

### 35-build-and-test-infra

- 一句话定位：内核镜像如何被构建出来、如何被验证。
- 讲什么：K-486~K-495。C 构建与 unpaged 重命名；genassym/procoffsets；链接顺序；Rust workspace 与 feature；.cargo 约束；xtask；QEMU 测试阶梯；boot_integration；mock 夹具；CI 现状。
- 不讲什么：链接脚本语义（02）；启动流程（01-10）；各机制的单元测试清单（各篇 Ch5）。
- 前置：00（想跑一次内核的人先读本篇）。
- 后置：各篇测试节的运行方式引用本篇。
- 事实底线：
  - 非 C 制品：`minix3/minix/kernel/Makefile:15-26`；`arch/i386/Makefile.inc:48-125`；`procoffsets.cf`；`os/Cargo.toml`；`os/kernel/Cargo.toml`；`os/.cargo/config.toml`；`os/xtask/src/main.rs`；`os/qemu-tests/{run_all.sh,run_qemu.sh,README.md}`；`os/kernel/tests/boot_integration.rs`；`os/plat/src/mock.rs`；`os/kernel/src/test_helpers.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-486 | C 构建与 unpaged 重命名 | 工具 | `Makefile.inc:51-83` | 缺口补齐 | 新增 |
| K-487 | procoffsets 生成 | 工具 | `Makefile.inc:108-125` | 缺口补齐 | 新增 |
| K-488 | 链接顺序 | 约束 | `kernel.lds:14-20`、`Makefile:25` | 缺口补齐 | 新增 |
| K-489 | Rust crate 分层/feature | 工具 | `os/kernel/Cargo.toml:24-27` | 现状 | 存量 01 §3.8 |
| K-490 | .cargo 约束 | 约束 | `.cargo/config.toml:21-34` | 构建 | 存量 02 §3.2 |
| K-491 | xtask | 工具 | `os/xtask/src/main.rs:22-144` | 入口 | 新增 |
| K-492 | QEMU 测试阶梯 | 测试 | `qemu-tests/run_all.sh` | 端到端 | 存量 01 §5.2 |
| K-493 | boot_integration | 测试 | `boot_integration.rs:14-308` | 集成 | 存量 02 §5.4 |
| K-494 | mock 与夹具 | 测试 | `os/plat/src/mock.rs`、`test_helpers.rs` | 单测 | 存量 06 §5.0 |
| K-495 | CI 与 flake | 工具 | `todo.md`、`smp_todo.md` | 可信度 | 存量 todo §23 |

- 验收标准：能照着本篇从零构建并在 QEMU 启动三架构测试内核；能解释 `__k_unpaged_` 重命名的目的与实现手段（objcopy）；能说明哪些测试在主机跑、哪些必须 QEMU；能列出当前已知的测试环境限制（并行 flake、WSL、跳过项）。

### 36-wire-formats-and-abi

- 一句话定位：跨模块的所有线格式与 ABI 集中对照，改协议前只查这一篇。
- 讲什么：K-496~K-505。message 64B 与 union；IPC status；SYS 号空间与掩码位序；kinfo/KernelInfo 字段对照；boot image/module 描述；grant/umap/vumap 参数；mcontext 尺寸；async/state/ipc filter 结构；对齐与字节序；errno 值表。
- 不讲什么：协议的语义与生命周期（14/15/18/24/25 各自讲）；endpoint 编码（14）；VMCTL 子命令语义（10）；usermapped 段机制（34）。
- 前置：14、15（语义读者应先读）。
- 后置：所有需要改跨模块结构的篇章。
- 事实底线：
  - C：`include/minix/ipc.h:2403-2675,2745-2761`；`ipcconst.h:7-32`；`com.h:205-278`；`param.h:9-47`；`type.h:148-154`；`ipc_filter.h:1-73`；`include/minix/errno.h`；`include/arch/i386/include/stackframe.h`、`fpu.h`。
  - 非 C 制品：`os/libs/minix-types/src/ipc/{message.rs:26-62,kernel_call.rs:18-177}`；`os/libs/minix-types/src/types/errno.rs`；`os/libs/minix-boot/src/kernel_info.rs`；`os/arch/src/*/signal.rs`。
- 知识点清单：

| 知识点 | 名称 | 类型 | 锚点 | 为什么归本篇 | 来源 |
|--------|------|------|------|-------------|------|
| K-496 | message 布局 | 接口 | `ipc.h:2403-2675` | 缺口补齐 | 新增 |
| K-497 | IPC status 编码 | 接口 | `ipcconst.h:21-32` | 状态回传 | 存量 23 §2.8 |
| K-498 | SYS 号表/掩码位序 | 接口 | `com.h:205-278` | 编号对照 | 存量 13 §2.1 |
| K-499 | kinfo 字段对照 | 接口 | `param.h:14-47` | 逐字段 | 存量 28 附录 |
| K-500 | boot image/module | 接口 | `type.h:148-154` | 启动清单 | 存量 06 §2.3 |
| K-501 | grant/umap/vumap 结构 | 接口 | `ipc.h` | 调用参数 | 存量 18 §2 |
| K-502 | mcontext 尺寸 | 接口 | `os/arch/src/*/signal.rs` | 缺口补齐 | 新增 |
| K-503 | async/state/filter 结构 | 接口 | `ipc.h:2745`、`ipc_filter.h` | 扩展协议 | 存量 12/23 |
| K-504 | 对齐与字节序 | 约束 | `ipc.h:2672` | 缺口补齐 | 新增 |
| K-505 | errno 值表 | 接口 | `errno.h`、`errno.rs` | 缺口补齐 | 新增 |

- 验收标准：能画出 message 的 64 字节布局（source/type/union 起止）；能对任一系统调用写出它的消息 arm 与参数结构；能对照 C 结构体与 Rust `minix-types` 逐字段检查（至少演示一个）；能说出三处最容易踩的对齐/宽度陷阱。

---

## 6. 变更表

> 操作类型：重排 / 拆分 / 合并 / 新建 / 归档。旧位置为旧文档编号；新位置为新编号。涉及知识点以 K 编号列出；"存量去向"指该操作经手知识点的最终落点（不逐条展开的见 §2 的按篇分组）。
> 归档 = 旧文整体退出正式目录，内容被吸收或判定为过程材料；B 相不删除文件。

### 6.1 整篇映射总表

| 操作 | 旧位置 | 新位置 | 类型 | 理由 | 涉及知识点 | 存量去向 |
|------|--------|--------|------|------|-----------|---------|
| 重排+重写 | 00 | 00 | 重排 | 保留总览定位，压缩为心智模型与阅读地图 | K-001~008 | 00 全篇 |
| 拆分 | 01 | 01 + 02 | 拆分 | 01 原含"引导准备"与"ELF 加载/高半"两语义（2019 行） | K-009~022 → 01；K-023~036 → 02 | 01/02 |
| 重排 | 02 | 02 | 重排 | 去重复叙述，接收 01 拆出的 ELF/高半内容 | K-023~036 | 02 |
| 重写 | 03 | 03 | 重写 | 去 TrapReturnArch 越界（→11），保留保护结构与 early console | K-037~054 | 03 |
| 重写 | 04 | 04 | 重写 | 保留平台发现主线，去锚点失真 | K-055~068 | 04 |
| 拆分 | 05 | 05 + 06 + 03（EarlyConsole） | 拆分 | 原 2263 行含时钟初始化、中断控制器、ArchInit、控制台多主题 | K-069~077 → 05；K-078~087 → 06；K-054 → 03 | 05/06/03 |
| 重写+下放 | 06 | 07 | 重写 | 保留阶段 C 主线与存储模型；运行时字段语义下放 13/14/19/21/23/25 | K-088~115 → 07；K-095/096/184/191 → 13；K-193~197 → 14；K-273~283 → 19；K-301~316 → 23；K-331~345 → 25；K-394~411 → 21 | 07/13/14/19/21/23/25 |
| 重写 | 07 | 08 | 重写 | 保留阶段 D；删除 createpde 逐行、下放 VM 侧细节；吸收 07-paging 备忘 | K-116~129 | 08 |
| 重写 | 08 | 09 | 重写 | 保留收口时序；SYS 常量表下放 15/36 | K-130~141 → 09；K-213~217 → 15 | 09/15/36 |
| 重排 | 09 | 10 | 重排 | 保留 VMCTL 协议，作为启动链末尾 | K-142~154 | 10 |
| 重排 | 10 | 16 | 重排 | 调度循环移到运行骨架末位（因果序：先构件后出口） | K-229~239 | 16 |
| 重写 | 11 | 13 | 重写 | 保留调度原语；RTS 履历表随迁；去 16 的 SMP 混叙 | K-177~192 | 13 |
| 重写 | 12 | 14 | 重写 | 保留 IPC 核心；并入 endpoint（99）；delivermsg 调用点归 16 | K-193~212；K-193~197（自 99） | 14 |
| 重写 | 13 | 15 | 重写 | 保留分派机制；trap 入口归 11；常量表与线格式归 36 | K-213~228；K-155/159 → 11；K-498 → 36 | 15/11/36 |
| 拆分 | 14 | 11 + 17 | 拆分 | 入口机制与异常处理是两件事 | K-155~166 → 11；K-240~255 → 17 | 11/17 |
| 拆分 | 15 | 19 + 27 | 拆分 | tick 处理（运行期）与时钟系统调用（接口）分篇；初始化归 05 | K-271~286 → 19；K-361~373 → 27；K-069~077 → 05 | 19/27/05 |
| 拆分 | 16 | 12 + 20 | 拆分 | BKL/per-CPU 模型与 AP 启动/IPI 是两种语义（1298 行混叙） | K-167~176 → 12；K-287~300 → 20 | 12/20 |
| 重写 | 17 | 23 | 重写 | 保留进程生命周期；do_schedule 补缺 | K-301~316 | 23 |
| 重写 | 18 | 24 | 重写 | 保留拷贝原语；do_setgrant 补缺；VMSUSPEND 协议归 18 | K-317~330 | 24 |
| 重写 | 19 | 25 | 重写 | 保留信号中介；trap_style 归 11 | K-331~345；K-161 → 11 | 25/11 |
| 重写 | 20 | 26 | 重写 | 保留设备接口；anti-translate 收敛 | K-346~360 | 26 |
| 重写 | 21 | 27 | 重写 | 保留时钟系统调用；tick 实现归 19 | K-361~373 | 27 |
| 重写 | 22 | 21 | 重写 | 保留权限模型；过滤执行归 22；KPriv 计数修正 | K-394~411 | 21 |
| 重写 | 23 | 22 | 重写 | 保留过滤；权限字段定义归 21 | K-412~424 | 22 |
| 重排 | 24 | 18 | 重排 | 跨空间恢复协议前移到运行骨架（拷贝篇的前置） | K-256~270 | 18 |
| 重写 | 25 | 28 | 重写 | 保留杂项；mcontext/diagctl 补缺；krandom 保留 | K-374~393 | 28 |
| 重写 | 26 | 33 | 重写 | 保留 WONTFIX 文档化 | K-469~475 | 33 |
| 重写 | 27 | 32 | 重写 | 保留错误/日志/关机；do_abort/reset 补缺；errno 原则新增 | K-457~468 | 32 |
| 重写 | 28 | 34 | 重写 | 保留 usermapped 演进 | K-476~485 | 34 |
| 合并 | 29 + 30 | 31 | 合并 | 两者同属"内核观测自身"支线，各约 370 行，合并后单篇完整 | K-444~456 | 31 |
| 重排 | 31 | 29 | 重排 | 独立 FPU 篇保持 | K-425~435 | 29 |
| 重排 | 32 | 30 | 重排 | 独立回溯篇保持；修悬空引用 | K-436~443 | 30 |
| 重写 | 99 | 14（§寻址）+ 36 | 拆分+归档 | 99 是只有 Endpoint 一节的残篇；RTS/常量承诺未交付 | K-193~197 → 14；K-194/499 → 36 | 14/36 |
| 新建 | — | 11 | 新建 | 入口机制集中成篇（缺口 GAP-08） | K-155~166 | 新建 |
| 新建 | — | 35 | 新建 | 构建与测试基建（缺口 GAP-01） | K-486~495 | 新建 |
| 新建 | — | 36 | 新建 | 线格式与 ABI 集中（缺口 GAP-02） | K-496~505 | 新建 |
| 归档 | 06-todo | — | 归档 | 06 重构的过程材料；职责边界矩阵已入 07 契约 | — | 07 契约 |
| 归档 | 07-paging_init_gpt | — | 归档 | 设计冻结备忘；K-129 已入 08 | K-129 | 08 |
| 归档 | 18-trap-bridge-design | — | 归档 | E1 设计稿；K-158/161~165 已入 11 | K-158/161~165 | 11 |
| 归档 | checklist | — | 归档 | 历史账本；K-455/K-502 等机制项已入池 | K-455、K-502 | 31/36 |
| 归档 | endpoint_todo | — | 归档 | 未采纳设计讨论；核心事实已入 14 | K-194 | 14 |
| 归档 | panic-in-drop | — | 归档 | 设计备忘；K-108/115 已入 07 | K-108、K-115 | 07 |
| 归档 | smp_todo | — | 归档 | 施工记录；K-288/290/300 已入 20 | K-288、K-290、K-300 | 20 |
| 归档 | smp_gpt / v2 / v3 | — | 归档 | 外部评审输入，结论已并 | — | 20/31 |
| 归档 | todo | — | 归档 | stage 权威清单，open 项转 edge_todo 后归档 | K-495 | 35 |

### 6.2 拆分与合并的存量去向（逐条）

**拆分 01 → 01 + 02**：boot-shim/固件/KernelInfo（K-009~022）留 01；ELF 加载/link.lds/高半/HigherHalf（K-023~036）迁 02。01 原 §3.3（2→4 级页表）与 §4.3（pt_alloc）中"页表页分配"留 01，"映射建立"迁 02。无知识点丢弃。

**拆分 05 → 05 + 06 + 03**：时钟软件状态与时钟源（K-069~077）留 05；中断控制器与 ArchInit（K-078~087）迁 06；EarlyConsole（K-054）迁 03。05 原 GICv3/PLIC 两节归 06，避免"时钟篇讲中断控制器"。

**拆分 14 → 11 + 17**：入口汇编、trap frame、返回指令、KTS、寄存器约定（K-155~166）迁 11；异常五分支、页错误、cause_sig、IRQ 链（K-240~255）留 17。14 原 §4.8（返回路径）整节迁 11。

**拆分 15 → 19 + 27 + 05**：tick 处理/定时器队列/负载（K-271~286）留 19；系统调用（K-361~373）迁 27；ClockState 构造与 hz（K-069~077）已在 05。

**拆分 16 → 12 + 20**：BKL/per-CPU/内存序/上下文约束（K-167~176）迁 12；AP 握手/双位图/IPI/schedule_sync/halt（K-287~300）迁 20。16 原 §9/§10（early entry）全文并 20。

**合并 29 + 30 → 31**：队列自检/打印/IPC 统计 WONTFIX（K-444~448）与 profile 采样（K-449~454、456）合成"观测"篇；各自保留独立小节，不互相前置。新增 K-455（硬件断点）。

### 6.3 新建篇章的原料来源

| 新篇 | 从旧文档取 | 从 C 源码取 | 从非 C 制品取 |
|------|-----------|------------|--------------|
| 11-trap-entry | 14 §1.2/§1.3/附录 A、13 §1.2、19 §4.6.1、28 §1.3、18-trap-bridge 全文 | `mpx.S`、`usermapped_glo_ipc.S`、`sconst.h`、`stackframe.h`、`arch_proto.h`、`protect.c:147-153`、`arch_system.c:485-610` | `os/arch/src/arch/{trap_entry,trap_return}.rs`、`os/arch/src/x86_64/{trap_entry,trap_stub,boot}.rs`、`os/kernel/src/trap_dispatch.rs` |
| 35-build-and-test-infra | 01 §5、02 §5、05 §5、06 §5、16 §5（只取运行方式） | `kernel/Makefile`、`arch/i386/Makefile.inc`、`procoffsets.cf` | `os/Cargo.toml`、`os/xtask`、`os/qemu-tests`、`os/.cargo/config.toml`、`boot_integration.rs`、`test_helpers.rs` |
| 36-wire-formats-and-abi | 12 §3.10、13 §2.1/§2.3、18 §2、22 §4.1、23 §2.7/§2.8、28 附录 A | `ipc.h`、`ipcconst.h`、`com.h`、`param.h`、`type.h`、`ipc_filter.h`、`errno.h`、`stackframe.h`、`fpu.h` | `os/libs/minix-types/src/ipc/*`、`os/libs/minix-types/src/types/errno.rs`、`os/libs/minix-boot/src/kernel_info.rs`、`os/arch/src/*/signal.rs` |

---

## 7. 缺漏新篇

> 本节逐项落实 §3.1 的缺口与 §3.4 的非 C 清单，不允许留空、不允许"待定"。
> 每项回答：主题是什么、为什么重要、原料在哪里、归哪一篇、验收标准。

### 7.1 新建篇章（2 篇）

**NEW-01 构建与测试基建（35-build-and-test-infra）**
- 主题：C 内核与 Rust 内核分别如何构建、如何被单元/集成/QEMU 测试。
- 为什么重要：缺此篇时读者无法复现任何验收；`__k_unpaged_` 前缀、procoffsets、链接顺序这些"工程性前提"完全无文档。
- 原料：§6.3 第三行。
- 归属：35。
- 验收：能按篇内命令从零构建三架构测试内核并在 QEMU 跑通；能解释 unpaged 重命名与 genassym 的目的。

**NEW-02 跨模块线格式与 ABI（36-wire-formats-and-abi）**
- 主题：message、kinfo、syscall 号、IPC status、grant/umap/vumap、mcontext、errno、对齐。
- 为什么重要：这是 kernel 与所有其他 stage、以及用户态运行时的契约；B 相重写任何协议前必须有单一查阅点。
- 原料：§6.3 第四行。
- 归属：36。
- 验收：能对任一系统调用给出"消息 arm + 参数结构 + 返回编码"三件套；能对照 C/Rust 逐字段检查一个结构体。

### 7.2 并入既有篇章的缺口（14 项）

| 缺口 | 归篇 | 承载知识点 | 验收标准（可验证） |
|------|------|-----------|------------------|
| SYS_SCHEDULE | 23 | K-313 | 能说明 sched server 调用它的时机与对调用者自己的影响 |
| SYS_SETGRANT | 24 | K-327 | 能说明 grant 表地址/条目如何被校验与登记 |
| GETMCONTEXT/SETMCONTEXT | 28 | K-391 | 能列出 mcontext 读写的字段与架构差异入口 |
| SYS_ABORT / reset / halt | 32 | K-464、K-465 | 能复述从调用到硬件复位的完整路径 |
| DIAGCTL 子命令 | 28/32 | K-392、K-468 | 能列出四个子命令各自触发路径 |
| AP early entry ABI | 20 | K-288、K-300 | 能绘制 16 位到长模式梯子与固件 ABI 差异 |
| 双位图语义 | 20 | K-290 | 能说明 boot_ack 与 online 的转换条件 |
| paging_init 职责边界 | 08 | K-129 | 能说明六项职责各归谁 |
| Drop 三分类与夹具 | 07 | K-115 | 能对任一类型判断是否需要 Drop 检查 |
| 硬件断点 | 31 | K-455 | 能说明 debugreg 保存/恢复与 SYS_TRACE 的关系 |
| 模块数校验 | 01 | K-022 | 能说明校验失败的系统行为 |
| bootstrap 释放链 | 01/09 | K-013、K-021、K-134 | 能画出内核镜像被回收与不被回收的两种路径 |
| 端口 I/O 完整语义 | 26 | K-351~357 | 能对 devio/sdevio/vdevio 各给一个请求例 |
| 权限范围表查询路径 | 21 | K-402 | 能说明一次 CHECK_IO_PORT 的扫描过程 |

### 7.3 非 C 主题落实对照（与 §3.4 一一对应）

| 非 C 主题 | 落实 |
|----------|------|
| 链接与加载 | 02（语义）+ 35（构建） |
| 镜像与内存布局 | 02（虚拟/段）+ 01（物理 memmap）+ 08（DM 窗口） |
| 汇编入口与陷阱进入 | 11 |
| 启动装配 | 01-10 |
| 构建与工具链 | 35 |
| 跨模块接口与线格式 | 36 |
| 错误路径 | 32 + 各篇错误表 |
| 关闭与退出 | 32（内核）+ 21（进程） |
| 并发与同步 | 12 + 20 |
| 测试基建 | 35 + 各篇验收 |

### 7.4 参考材料归档去向与范围外发现

**归档材料中必须迁移的耐久知识**（已在 §2 入池）：

| 材料 | 迁移内容 | 去向 |
|------|---------|------|
| 06-todo | 职责边界矩阵、A1-A15 验收思想、零堆因果链、KPriv 计数校正 | 07 契约、35 |
| 07-paging_init_gpt | 两源资格规则、AArch64 窗口前提、paging_init 语义分解 | 08（K-126、K-129） |
| 18-trap-bridge-design | int-33 决策、RAX/RBX/RCX 约定、统一 CpuContext、OQ | 11（K-158~165） |
| checklist | CHECK_IPC 幻影、mcontext 尺寸、P0/P1 根因（BKL 泄漏、process_misc_flags 死循环） | 27/36/12/16 |
| endpoint_todo | SHIFT 与 ABI 论证、上限 31742 | 14（K-194） |
| panic-in-drop | 三分类法、夹具豁免、与 BklSection 正交 | 07（K-108、K-115） |
| smp_todo | ApBootstrap、双位图、内存序表、S-3d 根因、K6 设计 | 20（K-288~300） |
| smp_gpt×3 | 三轮问题作为 20 的"常见误解"素材（可选） | 20 |
| todo | 未闭环实施项（I-1 独立 ELF、I-5 RSDP、T-10 riscv U-Boot 链） | §7.5 范围外/转 edge_todo |

**范围外发现**（本 stage 目录内、但不属 kernel 正文的主题）：

| 发现 | 正确归属 | 处理 |
|------|---------|------|
| E1 的用户态半边（minix-sys `arch_trap.rs` 用户 trap 体） | 14-stage-runtime | 本 blueprint 在 11 契约中只定义内核侧契约；用户侧归 14-stage-runtime |
| E5-SMP 冒烟用例（fork + CoW 并发） | edge_todo（edge1 K3 / edge4 E5） | 归 35 测试基建引用，不新立 kernel 篇章 |
| K6 AP LAPIC 定时器设计 | 20（C 有对应实现，属真实缺口） | 已入 20 契约（K-298） |
| E-VMTLB（SMP 陈旧 TLB） | edge_todo + 02-stage-vm | 20 契约声明"跨 CPU TLB 刷新"为已知缺口，引用 edge |
| endpoint 重设计（方案 A/B/C） | 20-redesign | 不在本 stage；14 只保留现行编码事实 |
| do_padconf（ARM 平台引脚复用） | 16-stage-drivers / earm 平台 | 范围外：x86-64 主线不展开，36 记录调用号存在 |
| VM 侧 `map_kernel` / `init_page_table` | 02-stage-vm | 10/08 只讲接口与协作 |
| 用户态 SCHED 如何使用 vtimer/闹钟 | 06-stage-sched | 25 只讲接口语义 |
| PM 如何消费 SIGKSIG/ENDKSIG | 04-stage-pm | 23 只讲内核侧协议 |

---

## 8. 锚点迁移与断链成本

### 8.1 锚点迁移表（覆盖所有变化文档的每一节）

> 迁移类型：原样搬移 / 改写 / 合并 / 拆分 / 删除。风险列只标"旧编号被外部引用"或"内容跨篇"的高风险项；未标者按改写处理。

| 旧位置 | 旧内容（一句话） | 新位置 | 迁移类型 | 备注（断链风险） |
|--------|----------------|--------|---------|----------------|
| 00 §1.1 | 舞台导演类比 | 00 §内核是什么 | 原样搬移 | — |
| 00 §1.2 | 职责表 | 00 §职责 | 改写 | — |
| 00 §1.3 | 与 VM 关系 | 00 §与 VM 分工 | 改写 | 被 02-stage-vm 引用 |
| 00 §1.4/§1.4.1 | 三维实体/无主循环 | 00 §执行模型 | 原样搬移 | 高：被 06/10/14 引用 |
| 00 §1.5 | Rust 约束 | 00 §Rust 表达约束 | 改写 | 高：全 stage 共享 |
| 00 §2 | 启动线五阶段 | 00 §阅读地图 + 01-10 导览 | 拆分 | — |
| 00 §3 | 文档导航 | 00 §阅读路径 | 改写 | 高：被 vm plan 引用 |
| 00 §4 | 与 VM/PM/VFS 差异 | 00 §执行模型 | 合并 | — |
| 00 §5 | 设计原则 | 00 §Rust 表达约束、35 | 拆分 | — |
| 00 §6/§7 | 交叉参照/参见 | 00 §阅读路径 | 改写 | — |
| 01 §1.1-§1.6 | 内核第一段代码/启动链路 | 01 §引导准备 | 改写 | — |
| 01 §2.0.1-2.0.3 | Multiboot/head.S/boot.cfg | 01 §协议与配置 | 改写 | — |
| 01 §2.1-2.2 | 常量与数据结构 | 01 §memmap 与 kinfo | 改写 | — |
| 01 §2.3 | pre_init 关键函数 | 01 §启动数据收集 | 改写 | — |
| 01 §2.4-2.5 | 调用关系/设计要点 | 01 §责任链 | 合并 | — |
| 01 §3.1-3.8 | Rust 设计决策 | 01 §boot-shim 设计 | 改写 | 高：KernelInfo 契约被多篇引用 |
| 01 §4.1-4.7 | 实现详解 | 01 §实现 | 改写 | ELF/HigherHalf 部分迁 02 §加载 |
| 01 §5 | 测试要点 | 35 §测试阶梯 | 合并 | — |
| 01 §6 | 启动失败与诊断 | 01 §诊断、32 | 拆分 | — |
| 01 §7 | 参见 | 删除（并入 00 阅读路径） | 删除 | DEL-11 |
| 01 附录 A | UEFI 指南/32 位历史 | 01 §UEFI 最小概念；A.0 删除 | 拆分 | DEL-01 |
| 02 §1 | 中间态/高半理由 | 02 §高半问题 | 改写 | — |
| 02 §2 | C 链接/head.S/切栈 | 02 §C 镜像与跳转 | 改写 | — |
| 02 §3 | Rust 设计决策 | 02 §构建与跳转设计 | 改写 | — |
| 02 §4.1 | link.ld | 02 §链接脚本 | 原样搬移 | — |
| 02 §4.2 | ELF 加载 | 02 §内核 ELF 加载 | 原样搬移 | — |
| 02 §4.3 | HigherHalf | 02 §高半跳转 | 原样搬移 | — |
| 02 §4.4 | arch_boot_impl | 02 §arch_boot | 改写 | 高：被 07 引用 |
| 02 §4.5 | 完整流程图 | 02 §启动流程图 | 原样搬移 | — |
| 02 §5 | 测试 | 02 §验证、35 | 拆分 | — |
| 02 §6 | 过渡 | 删除（并入 03 前置） | 删除 | DEL-11 |
| 02 附录 A | riscv L2 bug | 02 §案例 | 原样搬移 | — |
| 03 §1.1-1.9 | 保护结构概念 | 03 §概念 | 改写 | — |
| 03 §1.10 | 小结 | 03 §小结 | 原样搬移 | — |
| 03 §2 | prot_init 三架构 | 03 §C 实现 | 改写 | — |
| 03 §3 | Rust 设计 | 03 §设计 | 改写 | — |
| 03 §4.1-4.2 | trait | 03 §trait | 原样搬移 | — |
| 03 §4.3 | init_protection + TrapReturnArch 评估 | 03 §init_protection；评估迁 11 | 拆分 | 高：trap return 引用 |
| 03 §4.4-4.6 | 映射/别名/差异 | 03 §实现 | 合并 | — |
| 03 §5 | 测试 | 03 §验证、35 | 拆分 | — |
| 03 §6 | 过渡 | 04 前置 | 删除 | DEL-11 |
| 03 附录 A | 内核栈威胁模型 | 03 §为什么换栈 | 原样搬移 | — |
| 04 §1 | 硬件发现概念 | 04 §概念 | 改写 | — |
| 04 §2 | C 实现 | 04 §C 基线 | 改写 | — |
| 04 §3.1-3.6（含 §11） | Rust 设计 | 04 §设计 | 改写 | §11 内嵌节改为正常小节 |
| 04 §4 | 实现 | 04 §实现 | 改写 | — |
| 04 §5 | 测试 | 04 §验证 | 改写 | — |
| 04 §6 | 术语索引 | 00 术语表 | 合并 | — |
| 04 §7 | 参见 | 删除 | 删除 | DEL-11 |
| 05 §1.0 | 同步/异步 + 控制器 | 06 §概念 | 拆分 | 高：被 14/15 引用 |
| 05 §1.1-1.4 | 启动位置/职责/三架构 | 05 §定位、06 §定位 | 拆分 | — |
| 05 §2.1 | init_clock | 05 §init_clock | 原样搬移 | — |
| 05 §2.2-2.3 | 8259A/OMAP | 06 §控制器初始化 | 拆分 | — |
| 05 §2.4-2.5 | arch_init | 06 §arch_init | 拆分 | — |
| 05 §2.6 | env 取舍 | 05 §hz 解析 | 合并 | — |
| 05 §3.1-3.2 | ClockArch/频率 | 05 §设计 | 改写 | — |
| 05 §3.3 | InterruptController | 06 §InterruptController | 原样搬移 | — |
| 05 §3.4 | EarlyConsole | 03 §early console | 原样搬移 | 高：被 01 引用 |
| 05 §3.5 | ArchInit | 06 §ArchInit | 原样搬移 | — |
| 05 §3.6 | 内存裁剪 | 06 §arch_init | 合并 | — |
| 05 §3.7 | D-59/不实现清单 | 05 §硬件触点、19 §tick | 拆分 | — |
| 05 §4.1 | ClockState | 05 §ClockState、19 §类型族 | 拆分 | 高：被 21 引用 |
| 05 §4.2-4.5 | 三架构时钟源 | 05 §时钟源 | 改写 | — |
| 05 §4.6-4.8 | GICv3/PLIC | 06 §控制器实现 | 原样搬移 | — |
| 05 §4.9 | ArchInit 实现 | 06 §ArchInit 实现 | 原样搬移 | — |
| 05 §4.10-4.12 | arch 实现 | 06 §arch 实现 | 原样搬移 | — |
| 05 §4.13 | init_clock_and_interrupts | 05 §初始化序列 | 改写 | — |
| 05 §4.14 | EarlyConsole 实现 | 03 §early console | 原样搬移 | — |
| 05 §5 | 测试 | 05/06 §验证、35 | 拆分 | — |
| 06 Ch1 | 概念（进程/四问/三件套/节拍） | 07 §概念 | 改写 | 高：被 03/08/09/10 引用 |
| 06 Ch2 | C 存储模型 | 07 §存储模型 | 原样搬移 | 高：字段全集导航表最常被引用 |
| 06 Ch3 | Rust 设计 | 07 §设计 | 改写 | — |
| 06 Ch4 | 实现 | 07 §实现 | 改写 | — |
| 06 Ch5 | 测试 | 07 §验证、35 | 拆分 | — |
| 06 Ch6/§6.1 | 边界矩阵 | 07 契约"不讲什么" | 合并 | — |
| 06 §1.4.1 | 编号映射 | 07 §boot image | 原样搬移 | — |
| 06 §2.1.6/§2.2.3 | RTS/IPC 不变量 | 13 §RTS、27 §对称性 | 拆分 | — |
| 07 §1 | 概念与两解 | 08 §概念 | 改写 | — |
| 07 §2 | C 实现 | 08 §C 基线 | 改写 | — |
| 07 §3 | 设计决策 | 08 §设计 | 改写 | — |
| 07 §4 | 实现 | 08 §实现 | 改写 | — |
| 07 §5 | 测试 | 08 §验证 | 改写 | — |
| 07 附录 A/B | 时序图/对照 | 08 §时序图 | 合并 | — |
| 07-paging_init_gpt 全文 | 设计评审备忘 | 08 §K-129 | 归档 | 参考材料 |
| 08 §1 | T0-T6 与拆散 | 09 §收口 | 改写 | DEL-02 |
| 08 §2.1-2.2 | 常量/数据结构 | 15 §常量、36 §线格式 | 拆分 | 高：SYS 常量表 |
| 08 §2.3 | 关键函数 | 09 §system_init/bsp | 改写 | — |
| 08 §2.4-2.5 | 调用关系/设计要点 | 09 §嵌套调用 | 合并 | — |
| 08 §3 | D1-D9 | 09 §设计、15 §设计 | 拆分 | — |
| 08 §4 | 实现 | 09 §实现 | 改写 | — |
| 08 §5 | 测试 | 09 §验证 | 改写 | — |
| 08 附录 A | smp_init 位次 | 09 §SMP 调用点、12 §BKL | 拆分 | — |
| 09 §1 | 协商时序 | 10 §协议 | 改写 | 高：被 02-stage-vm 引用 |
| 09 §2 | C 实现 | 10 §C 基线 | 改写 | — |
| 09 §3 | 设计决策 | 10 §设计 | 改写 | — |
| 09 §4.1-4.7 | 类型/切换/TLB | 10 §实现 | 改写 | — |
| 09 §4.8 | ptproc 跟踪 | 10 §跟踪、20 §per-CPU | 拆分 | — |
| 09 §4.9 | root/ELF 加载 | 10 §boot 根 | 改写 | — |
| 09 §5 | 测试 | 10 §验证 | 改写 | — |
| 09 §6.1 | kinfo 字段缺口 | 09 §K-141 | 合并 | — |
| 10 §1 | 汇聚点 | 16 §概念 | 改写 | 高：被 12/14/31 引用 |
| 10 §2 | C 主循环/idle | 16 §C 基线 | 改写 | — |
| 10 §3 | Rust 设计 | 16 §设计 | 改写 | — |
| 10 §4 | 实现/差异 | 16 §实现 | 改写 | — |
| 10 §5-6 | 测试/参见 | 16 §验证 | 合并 | — |
| 11 §1-2 | 概念/C 原语 | 13 §概念/C 基线 | 改写 | — |
| 11 §3 | Rust 设计 | 13 §设计 | 改写 | — |
| 11 §4 | 实现 | 13 §实现 | 改写 | — |
| 11 §5 | 测试 | 13 §验证 | 改写 | — |
| 11 附录 A | RTS 履历表 | 13 附录 A | 原样搬移 | — |
| 12 §1-2 | IPC 概念/C 实现 | 14 §概念/C 基线 | 改写 | 高：SENDREC/NOTIFY 语义引用 | 
| 12 §3-4 | Rust 设计与实现 | 14 §设计/实现 | 改写 | — |
| 12 §5-6 | 测试/参见 | 14 §验证 | 合并 | — |
| 13 §1 | 分派概念 | 15 §概念 | 改写 | — |
| 13 §2 | C 实现（含 58 号表） | 15 §C 基线、36 §号空间 | 拆分 | 高：号表被多篇引用 |
| 13 §3-4 | Rust 设计与实现 | 15 §设计/实现 | 改写 | — |
| 13 §5-7 | 测试/缺口/参见 | 15 §验证/现状 | 合并 | — |
| 14 §1.1-1.5 | 异常概念 | 06 §同步异步、17 §异常概念 | 拆分 | — |
| 14 §1.2-1.3 | 入口路径/异常帧 | 11 §入口 | 原样搬移 | — |
| 14 §2 | C 异常/中断实现 | 17 §C 基线 | 改写 | — |
| 14 §3-4 | Rust 设计与实现 | 17 §设计/实现 | 改写 | — |
| 14 §5 | 测试 | 17 §验证 | 改写 | — |
| 14 附录 A/B/C | 三架构入口/边界/保护模式 | 11 §入口、17 §边界 | 拆分 | — |
| 15 §1 | tick 概念 | 19 §概念 | 改写 | — |
| 15 §2 | C 实现 | 19 §C 基线 | 改写 | — |
| 15 §3 | Rust 设计 | 19 §设计、25 §设计 | 拆分 | — |
| 15 §4 | 实现 | 19 §实现、25 §实现 | 拆分 | — |
| 15 §5-6 | 测试/参见 | 19 §验证 | 合并 | — |
| 16 §1.1-1.4 | BKL/per-CPU/IPI/亲和 | 12 §并发模型、20 §SMP | 拆分 | 高：BKL 被全部运行期篇引用 |
| 16 §1.5-1.6 | AP 握手/跨架构 | 20 §SMP | 原样搬移 | — |
| 16 §2 | C 实现 | 12/20 §C 基线 | 拆分 | — |
| 16 §3 D1-D10 | Rust 设计 | 12 §设计、20 §设计 | 拆分 | — |
| 16 §4.1-4.6 | 常量/CpuLocal/IPI | 12/20 §实现 | 拆分 | — |
| 16 §4.7-4.14 | handler/sync/BKL/接入 | 12/20 §实现 | 拆分 | — |
| 16 §5 | 测试 | 20 §验证、35 | 拆分 | — |
| 16 §9-§10 | AP early entry | 20 §AP 梯子 | 原样搬移 | — |
| 16 附录 A | SMP 预留设计 | 20 §就绪链 | 原样搬移 | — |
| 17 §1 | 生命周期概念 | 23 §概念 | 改写 | — |
| 17 §2 | C 实现 | 23 §C 基线 | 改写 | — |
| 17 §3-4 | Rust 设计与实现 | 23 §设计/实现 | 改写 | — |
| 17 §4.8 | DEFERRED 表 | 23 §现状（按代码重写） | 删除+改写 | DEL-04 |
| 17 §5-6 | 测试/参见 | 23 §验证 | 合并 | — |
| 18 §1 | 信任分层 | 24 §概念 | 改写 | — |
| 18 §2 | C 实现 | 24 §C 基线 | 改写 | — |
| 18 §3-4 | Rust 设计与实现 | 24 §设计/实现 | 改写 | — |
| 18 §5-6 | 测试/参见 | 24 §验证 | 合并 | — |
| 18-trap-bridge | 设计稿 | 11 §K-158~165 | 归档 | 参考材料 |
| 19 §1 | 信号概念 | 25 §概念 | 改写 | — |
| 19 §2 | C 实现 | 25 §C 基线 | 改写 | — |
| 19 §3-4 | Rust 设计与实现 | 25 §设计/实现 | 改写 | — |
| 19 §4.6.1 | trap_style | 11 §KTS | 原样搬移 | 高：被 32 引用 |
| 19 §4.7-4.10 | DEFERRED/redox/BKL | 25 §现状 | 删除+改写 | DEL-04 |
| 19 §5-6 | 测试/参见 | 25 §验证 | 合并 | — |
| 20 §1 | 设备概念 | 26 §概念 | 改写 | — |
| 20 §2 | C 实现 | 26 §C 基线 | 改写 | — |
| 20 §3-4 | Rust 设计与实现 | 26 §设计/实现 | 改写 | — |
| 20 §4.8-4.9 | DEFERRED/anti-translate | 26 §现状/差异 | 删除+改写 | DEL-04 |
| 20 §5-6 | 测试/参见 | 26 §验证 | 合并 | — |
| 21 §1 | 五调用概念 | 27 §概念 | 改写 | — |
| 21 §2 | C 实现 | 27 §C 基线 | 改写 | — |
| 21 §3-4 | Rust 设计与实现 | 27 §设计/实现 | 改写 | — |
| 21 §5-6 | 测试/参见 | 27 §验证 | 合并 | — |
| 21 附录 A/B | 差异矩阵/DEFERRED | 27 §差异/现状 | 删除+改写 | DEL-04 |
| 22 Ch1 | 权限概念 | 21 §概念 | 改写 | 高：priv 字段被 07/22 引用 |
| 22 Ch2 | C 实现 | 21 §C 基线 | 改写 | — |
| 22 Ch3-4 | Rust 设计与实现 | 21 §设计/实现 | 改写 | — |
| 22 §4.6.1 | sendto 对称 | 21 §不变量 | 原样搬移 | — |
| 22 Ch5 | 测试 | 21 §验证 | 改写 | — |
| 23 Ch1 | 三层过滤概念 | 22 §概念 | 改写 | — |
| 23 Ch2 | C 实现（含契约表） | 22 §C 基线 | 改写 | — |
| 23 Ch3-4 | Rust 设计与实现 | 22 §设计/实现 | 改写 | — |
| 23 Ch5-7 | 测试/缺口/参见 | 22 §验证/现状 | 合并 | — |
| 24 Ch1 | 挂起协议概念 | 18 §概念 | 改写 | — |
| 24 Ch2 | C 实现 | 18 §C 基线 | 改写 | — |
| 24 Ch3-4 | Rust 设计与实现 | 18 §设计/实现 | 改写 | — |
| 24 Ch5-6 | 测试/参见 | 18 §验证 | 合并 | — |
| 24 附录 A | redox 对照 | 18 §对照 | 原样搬移 | — |
| 25 Ch1 | 未移植语义 | 28 §概念 | 改写 | — |
| 25 Ch2 | C 实现 | 28 §C 基线 | 改写 | — |
| 25 Ch3-4 | Rust 设计与实现 | 28 §设计/实现 | 改写 | — |
| 25 Ch5-6 | 测试/参见 | 28 §验证 | 合并 | — |
| 25 附录 | DEFERRED 清单 | 28 §现状 | 删除+改写 | DEL-04 |
| 26 Ch1-2 | watchdog 概念/C | 33 §概念/C 基线 | 改写 | — |
| 26 Ch3-6 | 设计/无实现/WONTFIX | 33 §设计/现状 | 改写 | — |
| 27 Ch1-2 | panic/kputc/_exit 概念/C | 32 §概念/C 基线 | 改写 | — |
| 27 Ch3-4 | Rust 设计/实现 | 32 §设计/实现 | 改写 | — |
| 27 Ch5-6 | 测试/缺口 | 32 §验证/现状 | 合并 | — |
| 28 Ch1-2 | usermapped 概念/C | 34 §概念/C 基线 | 改写 | — |
| 28 Ch3-4 | 设计/实现 | 34 §设计/实现 | 改写 | — |
| 28 Ch5-6/附录 | 测试/引用/字段表 | 34 §验证、36 §字段 | 拆分 | — |
| 29 Ch1-2 | debug 概念/C | 31 §自检 | 改写 | — |
| 29 Ch3-4 | 设计/实现 | 31 §自检实现 | 改写 | — |
| 29 Ch5-6 | 测试/参见 | 31 §验证 | 合并 | — |
| 30 Ch1-2 | profile 概念/C | 31 §采样 | 改写 | — |
| 30 Ch3-4 | 设计/实现 | 31 §采样实现 | 改写 | — |
| 30 Ch5-6 | 测试/参见 | 31 §验证 | 合并 | — |
| 31 Ch1-3 | FPU 概念/设计 | 29 §概念/设计 | 改写 | — |
| 31 Ch4-6 | 实现/缺口/参见 | 29 §实现/现状 | 改写 | — |
| 32 Ch1-3 | 回溯概念/设计 | 30 §概念/设计 | 改写 | — |
| 32 Ch4-6 | 实现/测试/参见 | 30 §实现/验证 | 改写 | — |
| 99 §1.1-1.5 | Endpoint | 14 §寻址 | 原样搬移 | 高：被 17/22/23 引用 |
| 33 §Ch1-§Ch2 | caller-by-nr 的动机、接口形态与调用图 | 15 §调用者句柄（K-506） | 改写 | 低：该篇为蓝图交付后新增，无既有互引 |

### 8.2 引用迁移表

> 检索范围：本目录全部 `.md`（760 处 `NN-*.md` 引用）与 `os/` 下 `.rs`（10 处指向 kernel 文档的引用）。
> 验证方式：B 相重建完成后，用脚本对"旧文件名 → 新文件名"做一次全局替换，再跑一次悬空引用扫描（`for r in $(rg -o '\b[0-9]{2}-[a-z0-9-]+\.md\b' ...)` 与文件清单比对）。

**A. 文档间引用（按旧目标聚合，迁移到新编号）**

| 旧目标 | 引用次数 | 新目标 | 备注 |
|--------|---------|--------|------|
| 16-smp.md | 77 | 12 + 20 | 引用需按语境分流（BKL/per-CPU → 12；AP/IPI → 20） |
| 06-proc-init-boot-proc.md | 77 | 07 | 高频；字段导航表锚点需一并更新 |
| 14-exception-interrupt.md | 67 | 11 + 17 | 入口相关 → 11；处理相关 → 17 |
| 11-scheduling-primitives.md | 60 | 13 | — |
| 25-misc-unported.md | 59 | 28 | — |
| 07-cross-space-init.md | 54 | 08 | 被 02-stage-vm 多处引用（跨目录） |
| 22-privilege.md | 53 | 21 | — |
| 02-higher-half-kernel.md | 52 | 02 | 编号不变，内容重划 |
| 17-syscall-process.md | 44 | 23 | — |
| 12-ipc-core.md | 43 | 14 | — |
| 04-platform-discovery.md | 41 | 04 | — |
| 03-kmain-cstart.md | 40 | 03 | Rust 代码里旧路径 `03-stage-kernel/03-kmain-cstart.md` 需整体改指 |
| 09-vm-boot-protocol.md | 39 | 10 | 被 02-stage-vm/01-vm-init-main 引用 |
| 15-clock-timer.md | 39 | 19 + 27 | 按语境分流 |
| 13-syscall-dispatch.md | 39 | 15 | — |
| 01-boot-shim-bootstrap.md | 35 | 01 | — |
| 08-system-init-boot-finish.md | 35 | 09 | — |
| 10-switch-to-user.md | 35 | 16 | — |
| 05-clock-interrupt-init.md | 34 | 05 + 06 | 按语境分流 |
| 19-syscall-signal.md | 29 | 25 | — |
| 24-cross-space-runtime.md | 23 | 18 | — |
| 18-syscall-copy.md | 22 | 24 | — |
| 26-watchdog.md | 19 | 33 | — |
| 31-fpu-context-switching.md | 19 | 29 | — |
| 00-kernel-overview.md | 18 | 00 | 被 02-stage-vm plan 引用（跨目录） |
| 23-ipc-filter.md | 16 | 22 | 被 `os/servers/rs/src/ipc_mask.rs:148` 引用 |
| 20-syscall-device.md | 12 | 26 | — |
| 21-syscall-clock.md | 10 | 27 | — |
| 30-kernel-profile.md | 6 | 31 | — |
| 99-global-concepts.md | 6 | 14 + 36 | endpoint → 14；常量 → 36 |
| 28-usermapped-data.md | 3 | 34 | 被 `os/kernel/src/kerninfo.rs:14` 引用 |
| 32-stack-tracing.md | 3 | 30 | — |
| 33-syscall-caller-api.md | 1 | 15 | 蓝图交付后新增（2026-09-19 16:39）；caller-by-nr 议题收编为 K-506 |
| 27-kernel-utility.md | 2 | 32 | — |
| 29-kernel-debug.md | 2 | 31 | — |

**B. 跨目录引用（其他 stage 指向本目录）**

| 引用源 | 引用目标 | 数量 | 迁移动作 |
|--------|---------|------|---------|
| `02-stage-vm/07-pagetable-struct.md` | 01-stage-kernel/07-cross-space-init.md | 2 | 改指 08，并更新小节锚点 |
| `02-stage-vm/08-pagetable-ops.md` | 01-stage-kernel/07-cross-space-init.md | 1 | 改指 08 |
| `02-stage-vm/01-vm-init-main.md` | 01-stage-kernel/09-vm-boot-protocol.md | 2 | 改指 10 |
| `02-stage-vm/16-pagefault.md` | 01-stage-kernel（整目录） | 1 | 保持目录引用；补新编号 |
| `02-stage-vm/plan.md` / `todo.md` | 01-stage-kernel/00、07 | 3 | 改指 00/08 |
| `02-stage-vm/00-vm-overview.md` | 01-stage-kernel（整目录） | 1 | 保持 |
| `03-stage-rs/00-rs-overview.md` | 01-stage-kernel/09-vm-boot-protocol.md | 1 | 改指 10 |
| `00-master-plan/README.md` | 09、06 | 4 | 改指 10、07；旧映射表本身需 master-plan 自行维护 |
| 合计 | — | 389（含 master-plan 与 plan 的过程引用） | B 相按上表批量替换 |

**C. Rust 代码注释引用（逐条）**

| 代码位置 | 旧引用 | 迁移动作 |
|---------|--------|---------|
| `os/arch/src/x86_64/trap_entry.rs:21` | `03-stage-kernel/03-kmain-cstart.md` | 改指 `01-stage-kernel/03-protection-init.md` + `11-trap-entry.md` |
| `os/arch/src/x86_64/protection.rs:8` | 同上 | 改指 03 |
| `os/arch/src/arch/trap_entry.rs:52` | 同上 | 改指 03/11 |
| `os/arch/src/arch/protection.rs:42` | 同上 | 改指 03 |
| `os/arch/src/riscv64/trap_entry.rs:45` | 同上 | 改指 03/11 |
| `os/kernel/src/globals.rs:45` | `03-stage-kernel/06-proc-init-boot-proc.md` | 改指 07 |
| `os/servers/vm/src/boot.rs:14` | `01-stage-kernel/09-vm-boot-protocol.md` | 改指 10 |
| `os/servers/rs/src/ipc_mask.rs:148` | `01-stage-kernel/23-ipc-filter.md` | 改指 22 |
| `os/kernel/src/kerninfo.rs:14` | `01-stage-kernel/28-usermapped-data.md` | 改指 34 |
| `os/libs/minix-sys/src/arch_trap.rs:3` | `01-stage-kernel/18-trap-bridge-design.md` | 改指 11（设计稿归档后仍可保留归档路径，但正文引用改 11） |

**D. 悬空引用清单（重建前就已失效，迁移时一并修正或删除）**

| 悬空引用 | 出现位置（示例） | 处理 |
|---------|----------------|------|
| `07-kmain-entry-protection.md` | 02 §4.5 流程图附近 | 删除或改指 03 |
| `16-signal-handling.md` | 32 §6 参见 | 改指 25 |
| `tmp-08-endpoint.md` | 99 §1.1 来源注 | 删除，改指 14 |
| `00-vm-overview.md`、`26-vm-init-main.md`、`24-vm-ipc-dispatch.md`、`15-pagefault.md` | 多处指向 02-stage-vm/draft/ | 改指 02-stage-vm 顶层新编号（跨目录协调） |
| `02-page-table-kernel.md` | 24 §1.6 | 改指 08 或 02-stage-vm 对应篇 |
| `01-vmproc-struct.md`、`02-vmproc-struct.md`、`03-vmproc-table.md`、`04-acl.md`、`04-protection.md`、`05-exception-interrupt.md`、`06-pagetable-struct.md`、`07-pagetable-struct.md`、`08-pagetable-ops.md`、`03-vm-request.md`、`08-endpoint.md` | 旧命名残留 | 按 02-stage-vm 现行编号统一替换 |
| `01-04-test-audit.md` | 03/04 的 review 引用 | 删除（指向 .review 产物，按规范不得引用） |
| `07-rs-period-heartbeat.md` | 06-todo 引用 | 归档材料内部引用，随材料归档 |

### 8.3 断链成本摘要

| 指标 | 数值 | 说明 |
|------|------|------|
| 文档间引用总数（本目录内） | 760 | 按 §8.2A 聚合；平均每篇被引 22 次 |
| 跨目录引用总数 | 389 | 含 master-plan/02-stage-vm/03-stage-rs；多数是目录名或旧编号 |
| Rust 代码注释引用 | 10 | 逐条列在 §8.2C |
| 受影响目标文档 | 33/34 | 除 00 外全部编号文档的编号或文件名发生变化 |
| 热点目标（前五） | 16-smp（77）、06-proc-init（77）、14-exception（67）、11-scheduling（60）、25-misc（59） | 前五合计占 44% |
| 热点引用源（前五） | 06-proc-init（147）、04-platform（65）、07-cross-space（57）、00-overview（53）、03-kmain（51） | 前五合计占 49% |
| 悬空引用（重建前已有） | 20 个不同目标 | §8.2D |
| 建议批量修改方式 | 1. 先跑"旧文件名 → 新文件名"映射替换（含 `03-stage-kernel/` → `01-stage-kernel/`）；2. 按语境分流的 4 个目标（05/11/14/15/16）改用小节锚点（如 `16-smp.md#bkl` → `12-concurrency-model.md#bkl`）；3. 跨目录引用由各 stage 自行替换；4. 最后跑悬空扫描与 `tools/check-review-rules.sh` 类的规则检查 | 单次脚本可完成 80%，剩余 20% 需人工按语境判定 |

**风险判定**：断链成本高的原因不是引用数量（760 处可由脚本处理），而是"按语境分流"的四组旧编号（05/14/15/16）。缓解措施：这四组在新目录采用**小节级锚点命名**（`#tick`、`#entry`、`#bkl`、`#ap-boot`），迁移脚本可用锚点关键词自动分流；无法自动分流的引用人工处理，预计 40-60 处。

---

## 9. 验证与自检门

### 9.1 四种机械检查结果

#### 检查一：前向引用扫描（逐篇检查"前置"）

**判定口径（本蓝图采用，供共识确认）**：把"前向引用"限定为**定义依赖**——即读者若不先读后篇就无法读懂本篇的术语或机制。以下三类**不算**违规：
1. `后置` 字段（契约规定的反向指针）；
2. "不讲什么"里的去向指引（边界声明，不携带知识）；
3. 正文中"详见 NN"式的实现位置指引（当前篇对所指机制已完成自足说明，指引只用于后续深读）。

按此口径逐篇扫描 37 篇契约的"前置"，结果如下（全部只指向更小新编号）：

| 新篇 | 前置 | 是否更早 |
|------|------|---------|
| 00 | 无 | — |
| 01 | 00 | ✅ |
| 02 | 01 | ✅ |
| 03 | 02 | ✅ |
| 04 | 03 | ✅ |
| 05 | 04 | ✅ |
| 06 | 05 | ✅（并列读亦可） |
| 07 | 03、05、06 | ✅ |
| 08 | 07 | ✅ |
| 09 | 07、08 | ✅ |
| 10 | 07、09 | ✅ |
| 11 | 03、07 | ✅ |
| 12 | 11、07 | ✅ |
| 13 | 12、07 | ✅ |
| 14 | 11、13 | ✅ |
| 15 | 14、12 | ✅ |
| 16 | 11、13、14、15 | ✅ |
| 17 | 11、13、14、15 | ✅ |
| 18 | 17、13、10 | ✅ |
| 19 | 17、13、05 | ✅ |
| 20 | 12、06、05、09 | ✅ |
| 21（privilege） | 07、13 | ✅ |
| 22（filter） | 21、14、15 | ✅ |
| 23（process） | 13、15、18 | ✅ |
| 24（copy） | 18、15、08 | ✅ |
| 25（signal） | 17、13、15 | ✅ |
| 26（device） | 15、17、21 | ✅ |
| 27（clock） | 19、15 | ✅ |
| 28（info） | 15、18、21、24 | ✅ |
| 29 | 16、17、12 | ✅ |
| 30 | 11、17 | ✅ |
| 31 | 13、14、20 | ✅ |
| 32 | 15、11 | ✅ |
| 33 | 19、17 | ✅ |
| 34 | 02、07 | ✅ |
| 35 | 00 | ✅ |
| 36 | 14、15 | ✅ |

**正文跨篇指引清单（允许类，重编号后共 6 处，B 相写作时保留）**：14→16（投递调用点）、18→24（拷贝接口）、10→16（调度环境）、29→25（信号帧边界）、31→28（SPROF 接口）、32→28（DIAGCTL 分工）。每处均为"当前篇已自足、指引仅供深读"；原清单中指向权限/过滤/trap_style 的 3 处指引在重编号后已变为向后引用，不再是跨篇指引。

**结论**：0 处定义级前向引用；重编号后全部前置严格更小，无需修正项。B 相扫描脚本建议：

```bash
# 对每篇，抽取"前置："行，检查编号是否严格小于本篇编号
for f in 0[0-9]-*.md 1[0-9]-*.md 2[0-9]-*.md 3[0-6]-*.md; do
  echo "$f: $(grep -m1 '^- 前置：' $f)"
done
```

#### 检查二：依赖关系图检查（无环）

依赖图节点 = 37 篇；边 = 前置关系。因为每条约都指向更小新编号（检查一），图必然是 DAG，拓扑序即数字序（备选序与否决见 §4.6）。关键汇聚点（被引用最多的篇）：

| 新篇 | 被后置引用次数（按契约字段估计） | 角色 |
|------|-------------------------------|------|
| 00 | 37（全部） | 术语与阅读路径根 |
| 07 | 8（08/09/10/11/12/14/27/34） | 进程与存储模型根 |
| 17 | 6（18/19/20/23/24/30/31/33） | 异常/中断机制根 |
| 13 | 8（14/16/18/19/20/21/23/27/31） | 调度状态机根 |
| 15 | 6（21/22/24/25/28/32） | 分派与返回约定根 |
| 12 | 3（13/20/29） | 并发模型根 |
| 11 | 6（12/14/16/17/30/32） | 入口机制根 |

无环；无需拆解。

#### 检查三：覆盖率检查（知识点池 → 新篇章）

| 检查项 | 结果 | 证据 |
|--------|------|------|
| 知识点总数 | 505 | §2.39 对账命令 |
| 归属唯一性 | 505/505 单 owner，0 条双归属 | §2.0 规则 + §2.38 争议裁决表 |
| 有条目无去向 | 0 | 池按"去向篇章"分组（§2.1-§2.37），每组即去向 |
| 明确删除且给理由 | 12 项 | §3.5 DEL-01~DEL-12 |
| 新增条目有证据锚点 | 25/25 | §2 各新增行的锚点列（C 源码 / 非 C 制品 / 参考材料） |
| 旧文档逐节有去向 | 34/34 篇覆盖 | §8.1 锚点迁移表 |
| C 文件有归属或明确排除 | 72/72（13+38+21） | §9.2 G2 列表 |
| 非 C 制品有归属 | 全部（见 G2） | §9.2 G2 列表 |
| 非 C 十项主题回答 | 10/10 | §3.4 |
| 缺口逐项落实 | 18/18 | §3.1 + §7.2 |
| 契约覆盖本组全部 K | 37/37 组 | 各契约"讲什么"的 K 区间与该组池条目数一一对应 |

#### 检查四：断链成本统计

见 §8.3。摘要：本目录内 760 处引用、跨目录 389 处、代码注释 10 处、历史悬空 20 处；热点目标前五占 44%；四组需按语境分流的旧编号（05/14/15/16）预计需要 40-60 处人工迁移。

### 9.2 自检门 G1-G9

| 门 | 检查内容 | 结果 | 证据 |
|----|---------|------|------|
| **G1** | C 真序逐条可核对（随机抽十条核对锚点） | **PASS** | 抽样核对命令与输出（`sed -n` 验证，2026-09-19）：`pre_init.c:217`→`kinfo_t *pre_init(...)`；`pg_utils.c:162`→`void pg_identity(...)`；`protect.c:321`→`void prot_init(void)`；`protect.c:388`→`void arch_boot_proc(...)`；`main.c:115`→`void kmain(...)`；`main.c:38`→`void bsp_finish_booting(void)`；`proc.c:119`→`void proc_init(void)`；`proc.c:299`→`void switch_to_user(void)`；`system.c:168`→`void system_init(void)`；`clock.c:70`→`int timer_int_handler(void)`；另核 `table.c:44`（image[] 起点）、`system.c:52`（call_vec）、`head.S:36`（MINIX 入口）、`kernel.lds:4`（_kern_phys_base）与 `proc.h:142-166`（RTS 16 位）全部命中 |
| **G2** | 知识点池完整：每个 C 文件、每个非 C 制品都有归属或"明确排除加理由" | **PASS** | C 顶层 13：clock.c→05/19、cpulocals.c→12、debug.c→31、interrupt.c→17、main.c→03/07/09/32、proc.c→13/14/16、profile.c→31、smp.c→12/20、system.c→09/15、table.c→00/07、usermapped_data.c→34、utility.c→32、watchdog.c→33。system/ 38 个 do_*.c 全数映射（23-28 契约的知识点清单逐文件对应；do_vmctl→10；do_abort→32）。arch/i386 21 个：acpi.c→04、apic.c→06、arch_clock.c→05/19、arch_do_vmctl.c→10、arch_reset.c→32、arch_smp.c→20、arch_system.c→03/05/06/17/29、arch_watchdog.c→33、breakpoints.c→31、direct_tty_utils.c→03/35、do_iopenable/readbios/sdevio.c→26、exception.c→17/29/30、i8259.c→06、memory.c→02/08/18/24、pg_utils.c→01/02/08、pre_init.c→01、protect.c→02/03/07、usermapped_data_arch.c→34；`oxpcie.c` 明确排除（`#if CONFIG_OXPCIE` 且 `kernel.h:12` 硬编码 0，DEAD code，不文档化）；`arch/earm/**` 明确排除（非 x86 目标，本 stage 只作对照引用）。非 C 制品：kernel.lds→02、head.S→01/02、mpx.S→11、klib.S→12/16、trampoline.S→20、apic_asm.S→06/20、debugreg.S→31、io_*.S→26、usermapped_glo_ipc.S→11/34、boot.cfg.default→01、multiboot.h→01、stackframe.h→11/36、fpu.h→29/36、Makefile/Makefile.inc→35、procoffsets.cf→35、Rust 各 crate→对应篇（§5 事实底线逐篇列出） |
| **G3** | 新目录前向引用为零（逐篇扫描"前置"） | **PASS（附 1 处修正）** | §9.1 检查一表；唯一修正项为 24 的前置字段（去掉对 27 的前置，改为正文指引） |
| **G4** | 依赖关系图无环；有环给拆解 | **PASS** | 全部前置指向更小编号；拓扑序=数字序；无需拆解；六种备选序与否决理由见 §4.6 |
| **G5** | 覆盖率 100%：池每条有去向或删除理由；新增都有证据锚点 | **PASS** | 505 条全部分组且单 owner（§2.38）；12 项删除（§3.5）；25 条新增全部带锚点（K-022 `main.c:160-162`、K-115 panic-in-drop、K-129 07-paging memo、K-162 18-trap-bridge、K-165 同、K-288/290/300 smp_todo、K-313 `do_schedule.c`、K-327 `do_setgrant.c`、K-391 `do_mcontext.c`、K-392 `do_diagctl.c`、K-455 `breakpoints.c`、K-457/458 `KcallResult`/`errno.h`、K-464 `do_abort.c`、K-465 `arch_reset.c`、K-486/487/488/491 Makefile/xtask、K-496 `ipc.h`、K-502 mcontext、K-504 对齐、K-505 errno 表） |
| **G6** | 拆分/合并写清存量去向；新建写清来源（抽查十处） | **PASS** | 抽查：① 01→01+02（K-023~036 迁 02）；② 05→05+06+03（K-078~087 迁 06、K-054 迁 03）；③ 14→11+17（K-155~166 迁 11）；④ 15→19+25（K-361~373 迁 25）；⑤ 16→12+20（K-287~300 迁 20）；⑥ 29+30→31（K-444~456 合并）；⑦ 新建 11 来源（C mpx.S/usermapped_glo_ipc.S + 18-trap-bridge）；⑧ 新建 35 来源（Makefile/Makefile.inc + os 构建制品）；⑨ 新建 36 来源（ipc.h/com.h/param.h + minix-types）；⑩ 99→14+36（K-193~197 与常量迁移） |
| **G7** | 每篇契约七要素齐全（定位/讲什么/不讲什么/前置/后置/事实底线/知识点清单+验收） | **PASS** | 37/37；每篇均含"一句话定位、讲什么、不讲什么（含去向）、前置、后置、事实底线、知识点清单、验收标准"八项（七要素中的"知识点清单+验收标准"合并为末两项） |
| **G8** | 锚点迁移表覆盖所有变化文档的每一节；引用迁移覆盖文档与代码注释 | **PASS** | §8.1 覆盖 34 篇旧文档全部小节（含附录），共 180+ 行；§8.2A 覆盖本目录 760 处引用按目标聚合；§8.2B 覆盖跨目录 389 处；§8.2C 逐条列出 10 处代码注释引用；§8.2D 列出 20 处历史悬空引用 |
| **G9** | 事实断言都有锚点（随机抽十条核对；推测项标注） | **PASS（附待验证项）** | 随机抽十条核对：`image[]` 17 条目顺序（`table.c:44-65`，sed 实读命中）；RTS 16 位值（`proc.h:142-166`，实读命中）；`NR_BOOT_PROCS = NR_TASKS + LAST_SPECIAL_PROC_NR + 1`（`param.h:9`，rg 命中）；`call_vec[NR_SYS_CALLS]`（`system.c:52`，实读命中）；`vm_running` 唯一写点在 `main.c:47`（全树 rg 无 `=1`）；`smp_init` 成功路径内调 `bsp_finish_booting`（`arch_smp.c:168`）；`pg_identity` 1024 个 4MB 页（`pg_utils.c:162`）；`NR_SYS_CALLS=58`、12 个空缺（`com.h:270` + `system.c:193-268`）；`vm_enable_paging` 顺序（`pg_utils.c:204-230`）；`kernel_may_alloc=0` 在 `main.c:105`。**待验证项（已标注，B 相以符号核对）**：a) 蓝图中的 Rust `file:line` 行号多来自旧文档快照，可能漂移（如 `clock.rs:971`、`smp.rs:163`、`syscall.rs:651`）——B 相一律按符号名定位；b) 旧文档小节号（§8.1 左列）为快照，个别小节名可能不完全一致；c) §8.3 的引用计数为 2026-09-19 grep 快照；d) `os/arch/src/*/signal.rs` 的 mcontext 尺寸（824/816/544）来自 checklist F-43/F-44，未逐架构复核 |

### 9.3 结论与待用户裁决的问题

**结论**：本蓝图完成。新目录 37 篇，覆盖旧 35 篇编号文档（含蓝图交付后新增的 `33-syscall-caller-api.md`）与 9 份参考材料的全部耐久知识（506 个知识点：480 存量 + 26 新增），显式删除 12 类历史残留，缺口 18 项全部落实，锚点迁移与断链成本已量化。B 相可以按本蓝图直接开工；开工前建议先做一次共识收敛，确认下列问题。

**待用户/共识裁决的问题**：

1. **是否采用 37 篇新编号方案**（00 + 01-36）？相对旧 34 篇，主要变化是：05/14/15/16 四处拆分、29+30 合并、新增 11/35/36、endpoint 从 99 并入 14、权限篇与过滤篇前移到 21/22 以保证系统调用族无前向引用。
2. **是否接受 12 项明确删除**，尤其是 DEL-01（32 位启动历史）、DEL-03（虚构机制段落）、DEL-05（测试计数快照）？这些内容在旧文档归档中仍可回溯，但不进入新正文。
3. **参考材料（06-todo/smp_todo/todo/checklist/endpoint_todo/panic-in-drop/smp_gpt×3/07-paging/18-trap-bridge）的归档方式**：随 B 相移入本目录 `archive/`，还是保留原位并由目标目录 README 声明"参考材料，不属正式目录"？
4. **跨目录引用迁移的责任划分**：389 处跨目录引用中，kernel 侧的目标编号变更由谁批量替换（kernel B 相统一改 vs 各 stage 自行改）？建议 kernel B 相随重建一次性替换，其他 stage 只核对。
5. **"允许的跨篇指引"口径**（§9.1 检查一）是否被共识接受为 G3 的判定标准？若不接受，B 相需为 6 处指引在各自篇内补充最小定义。
6. **是否需要保留 99-global-concepts 这一体例**：本蓝图把其唯一内容（endpoint）并入 14，未设计新的 99。若项目希望保留全局概念篇，可在 14 之外单立 99 并迁移 endpoint 常量表。
7. **归属规则与争议裁决是否接受**（§2.0 与 §2.38）：本轮取消了「第二主讲述点」写法，并把 5 组重名知识点（RTS 宏/联动、priv 布局/语义、generation 语义/实现、cause_sig 触发/机制、SYS 号归一化/权威表）拆给不同 owner。请确认这些裁决，尤其是 K-093（RTS 全集归 07）与 K-193~197（endpoint 归 14）两条。
8. **代码真相源规则的执行方式**（§0.4）：R-GT1~R-GT3 要求 B 相逐条按代码重核，并把旧文档失真记入正文差异说明；§0.4 的失真清单是否作为 B 相开工前的对账基线？

