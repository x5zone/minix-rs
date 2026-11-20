# RD06 反证报告（round1 · 反证者）

对象：`rewrite-notes/01-stage-kernel/06-proc-init-boot-proc.md`（2977 行）。
方法：以 `minix3/` 原始 C 源为真值优先序的第一位，逐条核对该文档的核心可证伪主张；Rust 侧以 `os/` 当前源核对。每条给出实际执行的命令与关键输出。允许的源码范围：`minix3/`、`os/`（清单 `related-code.txt` 内文件）。

## CLAIM RD06-CLAIM-01

**主张**（§1.2、§2.0、§2.3.1）：三张表的容量常量体系——进程表 261 槽（`NR_TASKS=5` + `NR_PROCS=256`）、特权表 64（`NR_SYS_PROCS`）、boot 清单 17（`NR_BOOT_PROCS = NR_TASKS + LAST_SPECIAL_PROC_NR + 1`）、模块 12（`NR_BOOT_MODULES = INIT_PROC_NR + 1`）、`USER_PRIV_ID = NR_TASKS + 11 = 16` 为静态段末槽、静态段长 `NR_STATIC_PRIV_IDS = NR_BOOT_PROCS = 17`、动态段 `[17, 64)`，以及 `kernel/priv.h` 末尾的 `#if (NR_BOOT_PROCS > NR_SYS_PROCS) #error` 编译期门。

**反证尝试**：逐常量对照头文件原始定义。

- `grep -n` `minix3/minix/include/minix/com.h`：`MAX_NR_TASKS 1023`（:55）、`NR_TASKS 5`（:56）、`NR_BOOT_MODULES (INIT_PROC_NR+1)`（:74）、`LAST_SPECIAL_PROC_NR 11`（:70）、`ROOT_USR_PROC_NR INIT_PROC_NR`（:78）。
- `minix3/minix/include/minix/sys_config.h`：`_NR_PROCS 256`（:8）、`_NR_SYS_PROCS 64`（:9）。
- `minix3/minix/include/minix/param.h:9`：`#define NR_BOOT_PROCS (NR_TASKS + LAST_SPECIAL_PROC_NR + 1)`。
- `minix3/minix/include/minix/priv.h`：`static_priv_id(n) (NR_TASKS + (n))`（:12）、`USER_PRIV_ID static_priv_id(ROOT_USR_PROC_NR)`（:18）、`NR_STATIC_PRIV_IDS NR_BOOT_PROCS`（:10）。
- `minix3/minix/kernel/priv.h:101-102`：`#if (NR_BOOT_PROCS > NR_SYS_PROCS)` / `#error NR_SYS_PROCS must be larger than NR_BOOT_PROCS`。
- `minix3/minix/kernel/proc.h:283`：`EXTERN struct proc proc[NR_TASKS + NR_PROCS];`。

代数复算：17 = 5+11+1；12 = 11+1；16 = 5+11；`[17,64)` 动态段与 `BEG_DYN_PRIV_ADDR`/`END_DYN_PRIV_ADDR`（`get_priv` 动态路径扫描范围，system.c 实测）一致。

**结果**：每个常量、每条算式、编译期门均与 C 源逐字对上，261/64/17/12/16 无一处可推翻。反证失败。

## CLAIM RD06-CLAIM-02

**主张**（§1.3、§1.4.1、§2.4）：`table.c` 的 `image[]` 恰 17 项、顺序与文档 §1.4.1 映射表逐行一致（asyncm/idle/clock/system/kernel 为负号 task，ds=6、rs=2、pm=0、sched=4、vfs=1、memory=3、tty=5、mib=7、vm=8、pfs=9、mfs=10、init=11）；模块下标与 proc 号无算术关系；表头三条顺序契约（顺序必须与 boot image 一致、内核 task 在前、DS 第一个系统服务且 RS 紧随）逐字存在；VM 的 `init_proc` 按 `proc_nr` 线性扫描 `boot_procs[]`，找不到即 `panic("no init_proc")`。

**反证尝试**：

- `awk '/^struct boot_image image/,/^};/' minix3/minix/kernel/table.c` 输出 17 行条目，名字与顺序与文档 §1.3 引文逐字一致（asyncm→init）。
- `grep -n` `minix3/minix/include/minix/com.h:47-72`：`ASYNCM -5`、`IDLE -4`、`CLOCK -3`、`SYSTEM -2`、`KERNEL -1`、`HARDWARE = KERNEL`、PM 0 / VFS 1 / RS 2 / MEM 3 / SCHED 4 / TTY 5 / DS 6 / MIB 7 / VM 8 / PFS 9 / MFS 10 / INIT(LAST_SPECIAL) 11——与 §1.4.1 表的 17 行映射逐格对上；模块 0=ds(6)、1=rs(2)、2=pm(0) 证实"无算术关系"。
- `minix3/minix/kernel/table.c:35-40`：三条表头契约句逐字存在（"MUST agree" / "all kernel tasks must come first" / "NOTIFY … DS must be the first system process … RS comes right after"）。
- `minix3/minix/servers/vm/main.c:262-286`：`init_proc` 以 `for (ip = &kernel_boot_info.boot_procs[0]; …)` 按 `ip->proc_nr != ep_nr` 线性扫描，扫描失败落到 `panic("no init_proc")`（:285）——与 §1.4.1、§2.4 的两处复述一致。
- §2.4 引文的 `init_vm` 循环（跳负号、跳 VM、`assert(ip->start_addr)`、`exec_bootproc`、`free_mem`）在 vm/main.c 逐行对上。

**结果**：反证失败。17 项、顺序、映射、契约注释、VM 扫描与 panic 消息全部成立。

## CLAIM RD06-CLAIM-03

**主张**（§2.1.6、§2.2、§3.3）：`p_rts_flags` 恰 16 个位（`0x01`…`0x10000`，`0x2000` 空缺）、`p_misc_flags` 恰 18 个位（`0x001`…`0x100000`，`0x010`/`0x020`/`0x800` 空缺）；口径"20 行 `#define RTS_` 中 4 行是操作宏"；Rust `RtsFlagsBits` 16 值与 C 逐一相同、`MF_FPU_INITIALIZED` 改名 `EXT_REG_INITIALIZED` 值仍 `0x1000`。

**反证尝试**：

- `grep -n '^#define RTS_' minix3/minix/kernel/proc.h`：142-154、162-166 恰 16 条位定义，值与文档 §2.2 代码块逐字一致（含 `RTS_PREEMPTED 0x4000`、`RTS_BOOTINHIBIT 0x10000`，`0x2000` 无定义）；202/206/216/227 四条操作宏（`RTS_ISSET/RTS_SET/RTS_UNSET/RTS_SETFLAGS`）。
- `grep -c '^#define RTS_[A-Z_]*[[:space:]]*0x'` → 16；`grep -c '^#define MF_[A-Z_]*[[:space:]]*0x'` → 18；`grep -c '^#define RTS_[A-Z]*('` → 4——与文档 §6.2 命令 2 的预期完全一致。
- MF 位 234-262：`MF_REPLY_PEND 0x001` … `MF_NICED 0x100000`，18 条、值与文档表逐格一致，空缺位 `0x010/0x020/0x800` 成立。
- `os/kernel/src/proc.rs:129-144`：`RtsFlagsBits` 16 个 const 值与 C 一致；`:196` `EXT_REG_INITIALIZED = 0x1000`。
- 三个操作宏的宏体（§2.2.1 引文）与 proc.h:202-227 逐字一致；"跃迁才动队列"的语义描述与宏体相符。

**结果**：反证失败。位值、空缺、口径、Rust 对位全部成立。

## CLAIM RD06-CLAIM-04

**主张**（§2.1、§2.1.2、§2.3）：`struct proc` 40 个顶层成员（37 具名 + 3 匿名聚合 `p_accounting`/`p_vmrequest`/`p_defer`）；`struct priv` 31 字段、七组算术 4+3+3+6+4+5+6=31；§2.1.2 的字段级使用口径——`p_cpu_mask` "全树只有声明与 do_update 槽交换时的一次整块拷贝"，`p_stale_tlb` "写入点在 do_vmctl 的 VMINHIBIT 解除分支（bits_fill）"。

**反证尝试**：

- `awk '/^struct proc \{/,/^\};/' minix3/minix/kernel/proc.h | grep -cE '^  [a-zA-Z_].*;|^  struct \{$'` → **40**；匿名聚合三处在 proc.h:48-55（`p_accounting`）、95-124（`p_vmrequest`）、132（`p_defer`）。
- `awk '/^struct priv \{/,/^\};/' …/priv.h | grep -cE '^  [a-zA-Z_].*;'` → **31**；逐字段清单与文档 §2.3 引文一致，七组算术与字段清单可复算对上。
- `p_cpu_mask` 全树 grep：仅 `proc.h:37`（声明）+ `system/do_update.c:255`（memcpy）——"全树只有声明与一次整块拷贝"**成立**。
- `p_stale_tlb` 全树 grep：**do_update.c:162-163 也写它**（`bits_fill(src_rp->p_stale_tlb, …)` 与 `bits_fill(dst_rp->p_stale_tlb, …)`，槽交换路径），另有 i386/earm `memory.c:178-189` 的 `UNSET_BIT` 写入；文档只点名 `do_vmctl.c:159` 一处，枚举不完整。

**结果**：非阻断。计数与分组主张反证失败（成立）；但 §2.1.2 中 `p_stale_tlb` "写入点"的枚举被反证成立一处遗漏（`do_update.c:162-163`，运行期槽交换路径），该遗漏在 §4.4 的 `p_stale_tlb` 行原样复述。属事实不完整而非机制错误，不动摇本篇任何启动期结论，故非阻断；两处（§2.1.2、§4.4）应补齐写点。

## CLAIM RD06-CLAIM-05

**主张**（§1.4、§2.5、§2.6、§2.7、§2.9、§3.9、§4.8）：`proc_init` 全文（7 字段清表 + 特权表 5 项 + 每 CPU idle 置 `PROC_STOP`）；boot 循环 12 动作及其顺序；可调度三条件与 `(void) get_priv`；优先级只给 VM 与 RS（`SRV_Q = USER_Q = 7`）；双闸条件 `p_nr != VM_PROC_NR && p_nr >= 0` 且 VM 例外；`bsp_finish_booting` 解除循环上界 `NR_BOOT_PROCS - NR_TASKS = 12`、按用户进程号 0..11、内核 task 永不解除；`arch_boot_proc` 三类划分与 VM ELF 装载五段；Rust 侧对应循环与位操作逐点对位。

**反证尝试**：

- `minix3/minix/kernel/proc.c` 的 `proc_init` 全文与文档 §2.5 引文逐字一致（含 `idle_priv.s_flags = IDL_F` 与 `ip->p_rts_flags |= RTS_PROC_STOP`）。
- `minix3/minix/kernel/main.c:165-272`：12 动作按文档 §2.6 表的顺序逐条命中——`ip=&image[i]`/`rp=proc_addr`（171-173）、`ip->endpoint = rp->p_endpoint`（174）、`p_cpu_time_left=0`（175）、task 写名（176-177）、模块回填 `kinfo.module_list[i - NR_TASKS]`（179-184）、`reset_proc_accounting`（186）、三条件 `schedulable_proc`（195-197）、`(void) get_priv`（200）、VM/task/RS 三分支（203-234，含 `s_sig_mgr = SELF` 仅 VM 一处 :208、RS 用 `SRV_SM` :231）、掩码填充（236-249）、else `RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM)`（251-254）、`arch_boot_proc`（257）、`proc_ptr` 兜底（259-261）、双闸条件（264-267）、尾部裸 `|= RTS_PROC_STOP` 与 `&= ~RTS_SLOT_FREE`（269-270）。
- 优先级：`rp->p_priority = SRV_Q` 仅出现在 VM 支（:209）与 RS 支（:232）；`config.h:66-71` `NR_SCHED_QUEUES=16`/`TASK_Q=0`/`MIN_USER_Q=15`，`USER_Q = (15-0)/2+0 = 7`，`priv.h:93` `SRV_Q USER_Q`——"只给 VM 与 RS、值 7"成立。
- `main.c:64-66`：`for (i=0; i < NR_BOOT_PROCS - NR_TASKS; i++) RTS_UNSET(proc_addr(i), RTS_PROC_STOP);`——上界与按进程号迭代成立；内核 task 不在 0..11 内，`proc_init` 的每 CPU idle 确置该位，§1.4 的"永不解除 + 惰性调用"推理成立。
- `i386/protect.c` 的 `arch_boot_proc` 全文与 §2.7 引文逐字一致（提前返回、VM 分支、六个回调、`64 * 1024` 栈、三字减法、`add_memmap`+置零成对、`kinfo.vm_allocated_bytes`）；`libexec_pg_alloc` 与 `pg_utils.c:143 assert(kernel_may_alloc)` 成立。
- Rust 对位：`os/kernel/src/lib.rs` 内核 task 循环尾置 `PROC_STOP`+清 `SLOT_FREE`（:2682-2685 "Kernel tasks start stopped."），用户模块轮 else 分支 `NO_PRIV|NO_QUANTUM`、双闸 `if nr != proc_nr::VM_PROC_NR`（:3010-3013，在用户模块轮内故省略 `>= 0` 安全）、尾部 `PROC_STOP`+`SLOT_FREE`；`bsp_finish_booting` 的 `for nr in 0..(NR_BOOT_PROCS - NR_TASKS)`（:3860-3865）与 C 同界。
- `bsp_finish_booting` 的 C 步序（§4.8 对照表）与 main.c:45-107 逐行一致（cpu_identify→vm_running=0→bill_ptr/proc_ptr=idle→announce→解除循环→cycles_accounting_init→boot_cpu_init_timer→fpu_init→processors_count→kernel_may_alloc=0→switch_to_user）。

**附带核对（非阻断级口误与复述不一致，不推翻主张本体）**：

1. §4.7 行 12 把 `PROC_STOP`+清 `SLOT_FREE` 的 Rust 位置写成"用户模块轮尾部"，漏了内核 task 轮尾部（lib.rs:2682-2685）也做同样两步——与 §3.9 表"每个成员各置一次"复述不一致（§3.9 与源码才是对的）。
2. §1.3"另外两个字段是空的"：`struct boot_image` 五字段中静态只写 `proc_nr`/`proc_name` 两样，静态为空的是 **三个**（endpoint/start_addr/len），且文档下一句自己说 endpoint 也由 boot 循环回填——计数口误。
3. §2.6"尾部三行都用裸位运算"：实为四条裸位运算语句（两条 inhibit + `PROC_STOP` + 清 `SLOT_FREE`）。
4. §1.1"boot 循环第二行就是 `ip->endpoint = …`"：实为循环体第 4 条语句（其后有 DEBUGEXTRA 与 `rp = proc_addr`）。
5. §2.1.6"boot 期唯一会被写的是 `MF_FPU_INITIALIZED` 的反向操作——do_clear…"：`do_clear` 是运行期槽回收路径，boot 期（kmain 内）没有任何 MF 位写点——"boot 期"的措辞与所引路径的时期不符。

**结果**：反证失败。主张的全部机制性内容（清表、12 动作、条件、上界、装载、Rust 对位）逐点成立；附带 5 处计数/措辞口误与 1 处跨节复述不一致，均非阻断。

## CLAIM RD06-CLAIM-06

**主张**（§5.1、§5.3、§5.4、§6.2）：宿主测试条数 43（proc.rs）/41（proc_table.rs）/39（kpriv.rs）/15（capability.rs）/14（arch/boot.rs）/14（x86_64）/5（arm64）/4（riscv64）；§5.3 所列测试名逐字取自源码；§5.4 真机载体 `test-proc-init` 存在且头部注释覆盖 7 点；§6.2 的命令"每条都已实跑"且预期输出正确；读数跑在 `feature = "mock"` 构建下，且 **"os/kernel/Cargo.toml 的 `default = ["mock"]` 就是宿主测试形态，生产形态走 `--no-default-features`"**。

**反证尝试**：

- `grep -c '#\[test\]'` 逐文件：43 / 41 / 39 / 15 与 14 / 14 / 5 / 4——与 §5.1 表完全一致。
- §5.3/§5.1 点名的测试函数逐个 `grep -rl "fn <名>"`：`kernel_task_uses_init_task_psw`、`user_process_uses_init_psw`、`inherit_fpu_state_propagates_lazy_user_policy`（x86_64/boot.rs）、`inherit_fpu_state_propagates_fpu_enable_el0`（arm64）、`inherit_fpu_state_copies_sstatus`（riscv64）、`load_vm_elf_syncs_icache_over_exec_segment_frames`、`load_vm_elf_skips_icache_sync_for_non_exec_image`（arch/boot.rs）、`test_process_table_const_init_per_slot_nr`、`test_iter_counts`、`test_is_valid_nr_boundaries`、`test_set_bill_to_idle`（proc_table.rs）、`test_priv_table_const_init_sets_per_slot_s_id`（kpriv.rs）——全部存在，无虚构测试。
- 载体：`os/qemu-tests/test-kernels/kernel/bootstrap/test-proc-init/` 存在；其 `src/main.rs` 头部注释恰 7 点，与 §5.4 的逐点转述一致；`os/qemu-tests/run_all.sh:130` `run_test "test-proc-init" x86_64 …`。
- §6.2 命令 2/4/6/7/8 在本机重放：16/18/4、17、main.c 两处 `NR_BOOT_PROCS`（:64/:165）、`proc_init()`/`IPCF_POOL_INIT()` 相邻（:157-158）、Rust 常量 256/64/261 与测试条数——全部复现。
- **构建形态子主张被推翻**：`os/kernel/Cargo.toml:36` 现为 **`default = []`**，且 ：31-35 的注释明写"形态开关（SD-30/U-1 已摘默认……）：`default = []`——不再有隐式形态。宿主测试腿走显式 `--features mock`……裸 `cargo test -p minix-kernel` 会以生产形态编 test 目标而失败"。仓库指令（AGENTS.md Build & Test 节）同一口径（SD-30 已清空三包默认形态）。文档 §5.1 的 `default = ["mock"]` 与现状相反，"生产形态走 `--no-default-features`"在 `default` 已空的前提下也不再构成形态区分。

**结果**：阻断。覆盖计数、测试名、载体、命令复现全部反证失败（成立）；唯一被推翻的是构建形态句——它是可当场复现的伪事实（P0-fact 类：`os/kernel/Cargo.toml:31-36` 为准），且位于文档自称"证据总账"的 §5，直接误导读数的复现方式（按文档裸跑会撞形态绊线）。修复面很小（改 §5.1 一句括注为"`default = []`，宿主测试显式 `--features mock`"），但按"文档与代码保持同步"的硬门，未修前不得收敛。

## 跨节复述冲突与失败路径省略（汇总）

跨节复述冲突（均已在上文各 CLAIM 附带核对，此处汇总）：

1. §4.7 行 12（"用户模块轮尾部"）vs §3.9 表（"每个成员各置一次"）vs lib.rs 实际（两轮循环尾各置一次）——§4.7 漏内核 task 轮，非阻断（RD06-CLAIM-05 附 1）。
2. §2.9"其余用户模块（10 项）名字与模块地址已登记"vs §2.1.1"其余 11 个用户模块在 boot 期没有名字"——"登记"只能读作清单（`kinfo.boot_procs` 副本）登记而非 `p_name` 写入，两处可调和但措辞易误读，非阻断。
3. §1.4 的四位"由谁解除"表与 §2.2.2 的全量置/解位表无冲突（前者显式限定 boot 期，后者声明"非穷举"口径）。

失败路径省略（抽查，均非阻断；文档自己列出的失败路径——`(void) get_priv` 不检查、VM 装载 panic、模块数 panic、`push_exclusion` assert、`ImageInsideSegments`、`phys_root` 断言——经查均如实覆盖）：

1. C `bsp_finish_booting` 的定时器初始化失败路径 `panic("FATAL : failed to initialize timer interrupts…")`（main.c:73-76）未出现在 §4.8 对照表。
2. Rust 交接页分配失败 `.expect("init_proc_and_boot: no frame for VM boot handoff page")`（lib.rs，§4.7 只讲写入不讲分配失败）未提及。
3. `kmain` 开头的 `assert(sizeof(kinfo.boot_procs) == sizeof(image))`（main.c:144）未提及。

## CLAIM 11

- 结果：反证失败——容量常量体系（261/64/17/12/16、静态段 17、`#error` 门）与 C 头文件逐字对上（对应上文 RD06-CLAIM-01）。

## CLAIM 12

- 结果：反证失败——17 项 boot 清单、三套编号映射、表头三条契约、VM `init_proc` 线性扫描与 `panic("no init_proc")` 全部成立（对应上文 RD06-CLAIM-02）。

## CLAIM 13

- 结果：反证失败——RTS 16 位与 MF 18 位的位值、空缺位、"20 行中 4 行操作宏"口径及 Rust bitflags 对位全部成立（对应上文 RD06-CLAIM-03）。

## CLAIM 14

- 结果：非阻断——40 成员/31 字段与分组算术成立，但 §2.1.2（及 §4.4 复述）的 `p_stale_tlb` 写入点枚举遗漏 `do_update.c:162-163`（对应上文 RD06-CLAIM-04）。

## CLAIM 15

- 结果：反证失败——`proc_init`、boot 循环 12 动作、四道抑制位的置位/解除、解除循环上界与 VM ELF 装载逐点成立；附 5 处计数/措辞口误与 §4.7 一处复述不一致，均非阻断（对应上文 RD06-CLAIM-05）。

## CLAIM 16

- 结果：阻断——测试条数、测试名、载体 7 点与 §6.2 命令复现全部成立，但 §5.1"Cargo.toml 的 `default = ["mock"]`"与现状 `default = []`（SD-30 摘默认，os/kernel/Cargo.toml:31-36）相反，属 P0-fact，未修不得收敛（对应上文 RD06-CLAIM-06）。

- 本次反证：6 条 CLAIM（阻断 1 / 非阻断 1 / 反证失败 4）。
