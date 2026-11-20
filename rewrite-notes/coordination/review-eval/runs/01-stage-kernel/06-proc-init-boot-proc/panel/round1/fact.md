# P-FACT 事实审查报告 — 06-proc-init-boot-proc.md

审查者：P-FACT（独立事实核查，只读）。方法：批量 grep/awk 对照 C 源（`minix3/minix/`）与 Rust 源（`os/`），引文块逐字比对，§6.2 诊断命令全部实跑。
机械门读数（引用自任务书）：anchor 118/118、citation blocks=96 checked=34 failed=0 skipped=62、structure/ref/style(error=0)/density 通过、doc-code-map 23/23。

---

## FINDING RD06-FACT-001（P3）

- **位置**：文档 §6.2 命令 3（L2908 口径注释 vs L2911 预期输出）
- **严重度**：P3（口径注释与预期输出自相矛盾；总数 40 正确）
- **问题**：命令注释写「proc 含 2 个匿名聚合」，预期输出写「40（37 具名 + 3 个匿名聚合类型）」。实测 `struct proc` 含 **3** 个匿名聚合成员（`p_accounting`、`p_vmrequest`、`p_defer`），其中仅前两个是多行写法（`^  struct \{$` 各 1 行），`p_defer` 是单行 `struct { reg_t r1, r2, r3; } p_defer;`、被具名行模式计入。grep 实测：`^  [a-zA-Z_].*;` 38 行 + `^  struct \{$` 2 行 = 40；按「具名成员 37 + 匿名聚合 3」分解才是对的，两处口径注释（2）与分解（3）互相打架。
- **影响**：读者按注释口径复算会得到 37+2=39≠40，以为命令错了；实际命令与 40 都对，错的只是注释里的「2」。
- **证据**：E1——`awk '/^struct proc \{/,/^\};/' minix3/minix/kernel/proc.h | grep -nE '^  struct \{$'` → 仅 2 行（p_accounting、p_vmrequest）；`grep -cE '^  [a-zA-Z_].*;'` → 38（含单行 p_defer）；合计 40。
- **最小修复**：把 L2908 注释改为「proc 含 3 个匿名聚合（2 个多行写法由 `struct {` 行计入，p_defer 单行由具名行计入）」。
- **置信度**：高（命令实跑）。
- **是否需源码核实**：否。
- **若我错了**：若「2 个匿名聚合」被刻意定义为「多行写法的聚合数」，则该句是口径描述而非结构体事实——但那样它与同条预期输出「3 个匿名聚合类型」仍然字面冲突，修改建议不变。

## FINDING RD06-FACT-002（P2）

- **位置**：文档 §1.2 L26 与 §2.1 标题引言（「40 个顶层成员」）
- **严重度**：P2（计数依赖 `CONFIG_SMP` 与 `DEBUG_TRACE` 配置，未声明口径）
- **问题**：「40 个顶层成员」只有在 `CONFIG_SMP` 定义（含 `p_cpu_mask`、`p_stale_tlb`）且 `DEBUG_TRACE` 未定义（不含 `p_schedules`）时成立。不带 SMP 是 38，带 DEBUG_TRACE 是 41。文档正文与 §6.2 命令 3 都把 40 当成无条件数，未注明配置口径。
- **影响**：读者在非 SMP 配置下复算得 38，会误判文档错误。
- **证据**：E1——`sed -n '22,150p' minix3/minix/kernel/proc.h`：`p_cpu_mask`/`p_stale_tlb` 在 `#ifdef CONFIG_SMP` 内，`p_schedules` 在 `#if DEBUG_TRACE` 内；SMP 开 + TRACE 关时具名 37 + 匿名 3 = 40。
- **最小修复**：在 §2.1 计数处（或 §6.2 命令 3）加一句口径：「按 CONFIG_SMP 定义、DEBUG_TRACE 未定义的内核配置计 40」。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若项目约定只存在 SMP 一种配置（config.h 强制），则降级为 P3 口径说明缺失。

## FINDING RD06-FACT-003（P2）

- **位置**：文档 §2.1.2 L327（「`p_stale_tlb` 的写入点在 `do_vmctl` 的 `VMINHIBIT` 解除分支（`bits_fill`）」）
- **严重度**：P2（写点陈述不完整，漏掉两处）
- **问题**：实测 `p_stale_tlb` 的写入点至少有四处：`system/do_vmctl.c:159`（VMINHIBIT_CLEAR 分支，文档已列）、`system/do_update.c:162-163`（`bits_fill` 两次）、`arch/i386/memory.c:180,182` 与 `arch/earm/memory.c:184,186` 的 `UNSET_BIT`。文档只给了 do_vmctl 一处。
- **影响**：读者以「写入点唯一」为前提做跨文件推断（例如认为迁移/跨空间路径不会碰该位）会得出错误结论；与本句对 `p_cpu_mask` 的穷举式陈述（那句实测准确：全树仅声明 + `do_update.c:255` memcpy）形成不对称的可信度。
- **证据**：E1——`grep -rn "p_stale_tlb" minix/ --include=*.c` 输出如上四处。
- **最小修复**：改为「写入点在 `do_vmctl` 的 `VMINHIBIT` 清除分支与 `do_update` 的槽交换路径（均 `bits_fill`），另在 `i386/earm` 的 `memory.c` 跨空间拷贝路径有 `UNSET_BIT`」。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若「写入点」被刻意限定为「boot 相关路径」，do_update/memory.c 属运行期——但原句没有这个限定词，且同句对 p_cpu_mask 用的是「全树」口径，两半句口径不一致仍需修。

## FINDING RD06-FACT-004（P2）

- **位置**：文档 §2.1.6 L474（「这 18 个位里，boot 期唯一会被写的是 `MF_FPU_INITIALIZED` 的反向操作——`do_clear` 在回收一个槽时把它清掉」）
- **严重度**：P2（「boot 期」限定与所举证据自相矛盾）
- **问题**：`do_clear` 是进程退出路径，不在 boot 期执行。实测 boot 期（`proc_init`、`kmain` boot 循环、`arch_boot_proc`/`arch_proc_reset`、`bsp_finish_booting`）对 `p_misc_flags` **零写入**——`proc_init` 只写 `p_rts_flags`（`p_rts_flags = RTS_SLOT_FREE`、idle 槽 `|= RTS_PROC_STOP`），`main.c` 无 `misc_flags` 引用，`arch_proc_reset` 不碰它（`i386/arch_system.c` 的 misc_flags 写点只有 restore_fpu 的 `|= MF_FPU_INITIALIZED` 与运行期 `|= MF_CONTEXT_SET`）。也就是说 boot 期没有任何 MF 位被写，「唯一会被写」的表述两头都不成立。
- **影响**：读者会以为 boot 流程中存在一次 do_clear 调用（不存在），或以为 boot 期写过某个 MF 位（没有）。
- **证据**：E1——`grep -n "misc_flags" minix/kernel/main.c` 零命中；`sed -n '/^void proc_init/,/^}/p' minix/kernel/proc.c` 仅含 p_rts_flags 写；`grep -n "p_misc_flags" minix/kernel/arch/i386/arch_system.c` 命中 198/490/515/543，均在运行期函数。
- **最小修复**：改为「boot 期这 18 个位全都不被写（`p_misc_flags` 保持 BSS 零）；本篇叙述涉及到的唯一一次 MF 位写动作在运行期的槽回收——`do_clear` 清 `MF_FPU_INITIALIZED`」。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若 C 内核 boot 期确有一次 do_clear（未找到），则该句成立——检索未发现任何 boot 路径调用 do_clear。

## FINDING RD06-FACT-005（P3）

- **位置**：文档 §2.2 L598-604（「下面三条按检索输出的实际顺序列出」）
- **严重度**：P3
- **问题**：三行内容的文本（含 `^I` 制表符转写）逐字正确，但输出顺序不可复现：本机 `grep -rn "RTS_VMREQTARGET" minix3/` 实际输出顺序为 proc.h:154 → debug.c:156 → mib/proc.c:358，文档列的是 debug.c → proc.h → mib。grep 遍历顺序依赖文件系统/版本，「实际顺序」这句把环境相关结果当成了稳定事实。
- **影响**：极小；复跑者会看到不同顺序并怀疑环境差异是事实错误。
- **证据**：E1——两次 grep 输出对比（本条与文档 L600-604）。
- **最小修复**：把「按检索输出的实际顺序列出」改为「按任意稳定顺序（如按文件路径排序）列出」或直接删掉顺序声明。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若文档声称的顺序在CI 固定环境中可复现，则仅为脆弱表述，仍建议去掉顺序声明。

## FINDING RD06-FACT-006（P3）

- **位置**：文档 §2.3 两处引文块——L625「`struct priv` —— 完整结构体」（L626-669）与 L693「`get_priv` —— 完整函数体」（L694-719）
- **严重度**：P3（引文保真：声称「完整」却有无标注的删减）
- **问题**：① `struct priv` 引文比源码少 2 个空行（`s_io_tab` 与 `s_nr_mem_range` 之间、`s_mem_tab` 与 `s_nr_irq` 之间，源文件 priv.h 结构体内各有一个空行，diff `35d34`/`38d36`）；② `get_priv` 引文比源码少函数体开头 3 行注释 `/* Allocate a new privilege structure … */`（diff `6,8d5`）。两块都标「完整」，删减处无 `...` 标注。
- **影响**：机械引文门放过（checked=34 failed=0 可能因尾部空白归一化跳过了这两块）；逐字核对时会误判文档抄错。
- **证据**：E1——`sed -n '/^struct priv {/,/^};/p' … | diff -` 与 `sed -n '/^int get_priv(/,/^}/p' … | diff -` 输出如上。
- **最小修复**：两块补回空行/注释行，或把块头「完整结构体/完整函数体」改注「节选（省略空行/注释）」。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若项目约定「完整」允许吞空行与源注释，则降级为建议统一约定。

## FINDING RD06-FACT-007（P3）

- **位置**：文档 §2.2 L606（「`proc.c` 里另留了一句注释 …（kernel/proc.c 的 mini_receive 附近的收件人检查处）」）
- **严重度**：P3（坐标错位）
- **问题**：`/* XXX: RTS_NO_ENDPOINT should be removed */` 在 `kernel/proc.c:1271`，位于 `try_deliver_senda`（proc.c:1200 起）的异步发送目的地检查内；`mini_receive` 在 proc.c:967，相距约 300 行。说「mini_receive 附近」不准确，函数名坐标应为 `try_deliver_senda`。
- **影响**：读者按「mini_receive 附近」检索会落空或找错函数。
- **证据**：E1——`awk 'NR<=1271 && /^[a-zA-Z_].*\(/{line=NR": "$0} NR==1271{print line}'` → `1200: int try_deliver_senda(...)`；`grep -n "int mini_receive" minix/kernel/proc.c` → 967。
- **最小修复**：把锚点改为 `try_deliver_senda`（异步发送的目的地检查内）。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若「附近」被理解为同文件同主题区域，则属可接受的模糊表述——但给出函数名更符合本篇自己的锚点纪律。

## FINDING RD06-FACT-008（P3）

- **位置**：文档 §1.1 L53（「而 boot 循环第二行就是 `ip->endpoint = rp->p_endpoint;`」）
- **严重度**：P3（序数不准）
- **问题**：`kmain` boot 循环体内的语句顺序是：①`ip = &image[i];` ②`DEBUGEXTRA(...)` ③`rp = proc_addr(ip->proc_nr);` ④`ip->endpoint = rp->p_endpoint;`。该赋值是第 4 个语句（不计调试宏则为第 3 个），不是「第二行」。
- **影响**：轻微；不影响反事实论证本身（`proc_init` 缺位时 `p_endpoint`/`p_priv` 为 BSS 零的推理正确）。
- **证据**：E1——`minix3/minix/kernel/main.c` L170-176（`sed -n '170,176p'`）。
- **最小修复**：改为「boot 循环开头几行里就有」或「`ip = &image[i]` 之后的第三条语句」。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若「第二行」按「非调试赋值」口径计（ip= 与 endpoint= 各为第 1、2 条实质赋值），则勉强成立——但按循环体逐行读会数出 3-4 行，建议直接给行号。

## FINDING RD06-FACT-009（P2）

- **位置**：文档 §3.2 L1471-1478（`fill_sendto_mask` 引文块）与 L1481（「块内 `D-58` 是仓库内部阶段编号，按原文引用」）
- **严重度**：P2（引文含源码中不存在的内容，且明确声称「按原文引用」）
- **问题**：引文块首行是 `// D-58 (C main.c:244): route the seed template mask through the`，但 `os/kernel/src/kpriv.rs` 的实际注释是 `// C main.c:244: route the seed template mask through the`——没有 `D-58` 前缀。全仓 grep（`os/` 全树 + 本文档）确认 `D-58` 只出现在本文档这两行（引文与注解），源码零命中。注解还说它「按原文引用」，双重失真：引文与出处声明都对不上。
- **影响**：这是全部 Rust 引文块里唯一一处「无中生有」的内容；读者据此去源码找 `D-58` 阶段编号会找不到，进而怀疑仓库篡改或版本错位。
- **证据**：E1——`grep -rn "D-58" os/ rewrite-notes/01-stage-kernel/06-proc-init-boot-proc.md` 仅命中文档 L1471/L1481；kpriv.rs 中 `fill_sendto_mask(priv_id, template.ipc_mask())` 上方 9 行注释逐行核对无 `D-58`。
- **最小修复**：删掉引文里的 `D-58 (…)` 前缀，按源码原样引用 `// C main.c:244: …`；删除「块内 D-58 是仓库内部阶段编号，按原文引用」这句注解。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若 `D-58` 曾在源码里、后被删而文档未同步，则根因是文档滞后——修复方向不变（以当前源码为准重引）。

## FINDING RD06-FACT-010（P3）

- **位置**：文档 §3.2 L1458-1459（`grant_capability` 引文块内的 if 换行）与 L1435（省略注解「KernelTask 的第三行注释」）
- **严重度：** P3（引文保真：改写换行 + 省略计数不准）
- **问题**：① 源码里该判断是单行 `if proc_nr == crate::proc::proc_nr::CLOCK || proc_nr == crate::proc::proc_nr::SYSTEM {`，引文块把它拆成两行且 `{` 独占一行，无省略标注；② L1435 注解说省略的是「KernelTask 的第三行注释」，实际省略的是 KernelTask 变体的第 2-6 行共 5 行注释（capability.rs L174-178）。
- **影响**：轻微；逐字复核对不上会浪费核对时间（本项发现即由此而来）。
- **证据**：E1——kpriv.rs grant_capability 单行 if；capability.rs L172-179 五行注释。
- **最小修复**：引文按源码换行重排；注解改为「省略 KernelTask 的 trap_mask 细注 5 行」。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若引文门把 Rust 块的换行归一化视为合法，①可降为建议；②不受此影响。

## FINDING RD06-FACT-011（P2）

- **位置**：文档 §3.7 L1892（`VmLoadError::ImageInsideSegments` 引文块末行「through a stale mapping (NK4-A first-light, 2026-09-21).」）与 L1897 注解（「末一变体的注释里含仓库内部阶段编号与日期，按原文保留」）
- **严重度**：P2（同 RD06-FACT-009 的根因：引文含源码中不存在的标记，且声称按原文）
- **问题**：`os/arch/src/arch/boot.rs` 的 `ImageInsideSegments` 注释原文以「…rejected explicitly rather than read through a stale mapping.」结束，**没有** `(NK4-A first-light, 2026-09-21)`。全仓检索：`NK4-A` 只出现在 `os/kernel-image/*.ld`、`check-layout.sh` 的注释与构建产物里，boot.rs 零命中；boot.rs 里与「first-light … 2026-09-21」最接近的是另一处注释（L527-528「x86-64 first-light #PF err=0x15 forensics, 2026-09-21」，说的是驱逐撞监督态映射，内容也不同）。引文块把这个外部记号拼进了本条注释并声称「按原文保留」。
- **影响**：与 FACT-009 同型——读者按引文去源码找 NK4-A 记号找不到；引文门对 Rust 块的校验明显没有覆盖到这两处。
- **证据**：E1——`grep -rn "NK4-A" os/arch/` 零命中；`sed -n '/pub enum VmLoadError/,+22p' arch/src/arch/boot.rs` 末变体注释无日期后缀；boot.rs L527-528 是另一条注释。
- **最小修复**：引文按当前源码去掉 `(NK4-A first-light, 2026-09-21)`；删除或改写 L1897 注解。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若该记号曾在此注释里、后被清走，则属文档未随代码更新——修复动作相同。

## FINDING RD06-FACT-012（P2）

- **位置**：文档 §3.8 L1959-1978（排除集引文块 + 「数组长度是 `NR_BOOT_MODULES + 1`：1 个内核镜像区间加 12 个模块区间」）与 L1938（「在它之前还有 `FREE_MEMMAP`、… 六个同类型的 `static`」）
- **严重度**：P2（§3.8 的引文与事实陈述整体过时，与当前源码不符）
- **问题**：两处：
  1. 引文块写 `let mut exclusions = [MemoryRegion { … }; crate::proc::NR_BOOT_MODULES + 1]; let mut n_excl = 0usize; push_exclusion(&mut exclusions, &mut n_excl, kernel_info.kern_phys_base(), …); for m in kernel_info.boot_modules() { … }`。当前源码（`lib.rs:2559-2598`）是：容量常量 `VM_BOOT_EXCLUSION_CAP = crate::proc::NR_BOOT_MODULES + 3`（lib.rs:2557）、变量名 `n`、且改经 `crate::vm_handoff::for_each_live_bootstrap(kern_base, kern_len, modules, root, bump, &[], …)` 闭包回调收集——引文是旧版代码。随后的散文「数组长度是 `NR_BOOT_MODULES + 1`：1 个内核镜像区间加 12 个模块区间」也错：现值是 +3（内核 + 模块 + root 页 + bump，与 panic 文案「after excluding kernel, boot modules, root page and bump」一致），且散文没有解释 +3 的构成。
  2. `FREE_MEMMAP` 这个 static 在 `os/` 全树不存在（仅 `kernel-image/src/bootface_a64.rs:199` 注释里提及旧名）；`globals.rs` 里 PROC_TABLE 之前的同类型 static 实际有 **9** 个（KERNEL_INFO、RESERVED_REGION_STORE、MEMMAP_REGION_STORE、BOOT_MODULE_STORE、BOOT_MODULE_NAME_STORE、PLATFORM_SOURCE_STORE、PARAM_BUF_STORE、STARTUP_RECORD_STORE、SOURCE_RECORD_STORE），不是 6 个。
- **影响**：按引文与散文复算容量（13）会与当前代码（15）对不上；按六 static 清单清点会漏三个、多一个不存在的名字。`push_exclusion` 的 assert 本身仍在（lib.rs:2604-2607），该机制陈述有效。
- **证据**：E1——`grep -n "VM_BOOT_EXCLUSION_CAP" kernel/src/lib.rs`（2557/2565/2569）；`sed -n '2559,2598p'`；`grep -rn "FREE_MEMMAP" --include=*.rs os/` 仅注释命中；`grep -n "pub(crate) static" kernel/src/globals.rs` 九处。
- **最小修复**：引文块按 lib.rs:2566-2580 当前代码重引；散文改为「容量常量 `VM_BOOT_EXCLUSION_CAP = NR_BOOT_MODULES + 3`（内核、12 个模块、root 页、bump 各一）；静态清单按当前 globals.rs 重列」。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若文档刻意引「重构前」版本作历史叙述，则块头未标注这一点本身就是缺陷（无「已过时」标注的旧代码引用）。

## FINDING RD06-FACT-013（P2）

- **位置**：文档 §3.9 L1997（引文内注释「C: RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM) — main.c:226」）与 L2005（「C: main.c:267-270」）；源码为 `os/kernel/src/lib.rs:2731` 与 `lib.rs:3009`
- **严重度**：P2（被引用为锚点的 C 坐标错误，文档逐字照引未纠正）
- **问题**：实测 C 源：`RTS_SET(rp, RTS_NO_PRIV | RTS_NO_QUANTUM)` 在 `minix3/minix/kernel/main.c:253`，不是 226；`RTS_VMINHIBIT`/`RTS_BOOTINHIBIT` 两行在 `main.c:265-266`（if 在 264，闭括号 267），「267-270」错位（267 是闭括号，268-270 是空行与 `RTS_PROC_STOP` 行）。同族对照：kpriv.rs 的「main.c:218-219」（trap 掩码三元）与「C main.c:244」（fill_sendto_mask）实测正确，说明错误只在这两处。
- **影响**：文档与 Rust 注释都把这些坐标当「同形证据」使用；读者按 226/267-270 去读 C 会读到 get_priv 分支中部与闭括号，找不到目标语句。
- **证据**：E1——`grep -n "RTS_NO_PRIV | RTS_NO_QUANTUM\|RTS_VMINHIBIT" minix3/minix/kernel/main.c` → 253/265/266；`sed -n '218,219p'`、`sed -n '244p'` 验证 kpriv 两处正确。
- **最小修复**：修 lib.rs 两处注释为 `main.c:253` 与 `main.c:264-267`，文档引文随之更新（或文档在引注处加勘误说明）。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若项目有意按某个历史版本的 main.c 行号引用，则两处 kpriv 的正确坐标反而成了不一致——统一到当前行号仍是正解。

## FINDING RD06-FACT-014（P2）

- **位置**：文档 §3.5 两处 Rust 引文块：L1752-1773（`set_boot_cpu_context`，块内末段为中文）与 L1726-1733（`load_vm_elf` free-function 理由段，块内「… 的旧三份实现 prior to refactor」）
- **严重度**：P2（引文块内混入翻译/改写文本，注解的省略声明与源码不符）
- **问题**：两个块都是「代码引用/按原文引用」格式，但内容被中文化：
  1. `set_boot_cpu_context` 块：源注释末段是英文「Counterpart of C's `arch_proc_reset` (arch_system.c:181; both the boot and exec paths go through it) — installing the complete new context must also seed `KTS_FULLCONTEXT`, …」，引文写成中文「…对位——装完整新上下文的同时必须种 `KTS_FULLCONTEXT`…」。注解只声明「省略一处仓库内部编号前缀」，而当前源码根本没有该前缀（proc.rs:1572 起整段无内部编号）；翻译本身也未声明。
  2. `load_vm_elf` 理由段：源注释是「(verified against the three per-arch implementations prior to refactor; those `proc_arch.rs` files are since removed — see the `arch/mod.rs` note and `06-…` §3.5)」，引文写成「(verified against … 的旧三份实现 prior to refactor — see `06-…` §3.5)」——注入中文、删掉后半句，注解却称「其余各行按原文」。
  坐标本身（arch_system.c:181、arch_system.c:597-598、panic 文案）实测正确。
- **影响**：与前几条同根因（引文保真），但这两处是整段改写而非局部插入，违背引文块「逐字」的约定更明显。
- **证据**：E1——proc.rs:1561-1576 与 boot.rs:335-341 的原文对照。
- **最小修复**：两块按英文原文重引；确需中文说明就移出引文块放注解里；删除/改写两处不成立的省略声明。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若项目约定允许「引文内中文化」，则该约定从未在文档中声明，建议统一到逐字引用。

## FINDING RD06-FACT-015（P1）

- **位置**：文档 §5.1 L2787（「`os/kernel/Cargo.toml` 的 `default = ["mock"]` 就是宿主测试形态，生产形态走 `--no-default-features`」）
- **严重度**：P1（构建形态事实错误，会直接把读者引到必失败的命令上）
- **问题**：`os/kernel/Cargo.toml:38` 实际是 `default = []`（SD-30/U-1 摘默认），`mock` 是显式 feature（`mock = ["minix-arch/runtime-window", "minix-plat/mock"]`，Cargo.toml:39）。Cargo.toml 注释原文明说「摘默认后裸 `cargo test -p minix-kernel` 会以生产形态编 test 目标而失败……必须带 `--features mock`」；项目 AGENTS.md 的 Build & Test 一节同样写明宿主测试必须 `--features mock`。文档停在摘默认之前的旧形态。
- **影响**：按 §5.1 复跑宿主测试读数的读者会裸跑 `cargo test` 并撞上 `form_contract` 编译期绊线，把设计行为误记为代码债（AGENTS.md 明文警告过这条误判路径）。
- **证据**：E1——`sed -n '31,40p' os/kernel/Cargo.toml`：`default = []`、`mock = [...]`、注释三行。
- **最小修复**：改为「宿主测试形态是显式 `--features mock`（`os/kernel/Cargo.toml` 的 `mock` feature；SD-30 后 `default = []`，裸跑 `cargo test -p minix-kernel` 会被 `form_contract` 绊线挡下），生产形态走 `--no-default-features`」。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若文档写作时点 Cargo.toml 尚未摘默认，则属文档未随 SD-30 更新——修复方向不变。

## FINDING RD06-FACT-016（P2）

- **位置**：文档 §4.2 L2471-2482（`ARGC_ARGV_ENVP` 引文块 + 注解「注释内含仓库内部编号与日期，按原文保留」）
- **严重度**：P2（与 RD06-FACT-009/011 同根因的第三处：引文内含源码不存在的内部编号，且整段为中文转写）
- **问题**：引文块把 `boot.rs:434-444` 的英文注释整段写成中文，并加入「SD-17 对齐轮 2026-10-06：」——`grep -c "SD-17" os/arch/src/arch/boot.rs` 为 **0**，源注释没有任何内部编号或日期；注解却声称「注释内含仓库内部编号与日期，按原文保留」。源注释的真实内容（旧值 20B、4-mod-8、exec 路径全字、上游仅 i386/ARM32、LP64 定案、消费端无读者、对现行行为惰性）逐点与译文语义一致，故机制陈述本身无误，错的只是引文形式与出处声明。
- **影响**：与 009/011 合并成「内部编号凭空出现在引文里」的模式：读者三次按引文找标记都落空，引文块的可信度整体受损。
- **证据**：E1——boot.rs:434-444 英文原文对照；`grep -rn "SD-17" os/arch/` 零命中。
- **最小修复**：引文按英文原文重引，删除「SD-17 对齐轮 2026-10-06」；若需保留内部编号叙事，放注解并如实标注「源码无此编号」。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：同 009/011——若编号曾在源码里后被清走，则属文档滞后，修复动作相同。

## FINDING RD06-FACT-017（P3）

- **位置**：文档 §1.3 L149（「第 j 个模块的地址被读成第 j+5 项、末 5 次读越过 12 项数组的末尾」）
- **严重度**：P3（方向性表述疑似颠倒）
- **问题**：漏掉 `- NR_TASKS` 后的行为是：清单第 i 项读 `module_list[i]`，拿到的是**本该属于第 i+5 项**的那个模块的地址；即「第 j 个模块的地址被第 j 项读走」，而不是「被读成第 j+5 项」。按字面读，句子描述的恰恰是正确映射（正确代码里第 j 个模块的地址正是给第 j+5 项用的）。后半句「末 5 次读越过 12 项数组的末尾」正确（i=12..16 五次越界）。
- **影响**：反事实机制不错（端点互换、VM panic 的后果链成立），但错位方向的表述会让读者推错「谁顶替了谁」。
- **证据**：E2——`main.c:178` `&kinfo.module_list[i - NR_TASKS]`，去掉偏移后条目 i 读模块 i；正确映射是模块 j ↔ 条目 j+5。
- **最小修复**：改为「清单第 j 项会读到第 j 个模块的地址——这个地址本该属于第 j+5 项；末 5 项（下标 12..16）越过 12 项数组的末尾」。
- **置信度**：中高（对原句意图有两种读法，其中一种恰好正确；按更自然的读法是错的）。
- **是否需源码核实**：否。
- **若我错了**：若作者意图就是「第 j 项读到第 j 个模块的地址」，则只需把主语改为「第 j 项清单」即可消除歧义——建议无论如何重写这句。

## FINDING RD06-FACT-018（P3）

- **位置**：文档 §4.8 L2720-2734（`bsp_finish_booting` 的 C↔Rust 步骤对照表）
- **严重度：** P3（对照表漏一行 C 步骤）
- **问题**：C 的 `bsp_finish_booting` 在 `vm_running = 0` 之后、`bill_ptr/proc_ptr` 之前还有两行 `krandom.random_sources = RANDOM_SOURCES; krandom.random_elements = RANDOM_ELEMENTS;`（main.c bsp_finish_booting 函数体），对照表没有这一行；Rust 侧对应物（`krandom::init()`，lib.rs:1840，在 `init_proc_and_boot` 内）也未在表中出现或说明去向。
- **影响**：读者按表清点 C 步骤会少一项；「Rust 保持同序」的收尾断言对这一步无从核对。
- **证据**：E1——`sed -n '/^void bsp_finish_booting/,/^}/p' minix3/minix/kernel/main.c` 含 krandom 两行；`os/kernel/src/lib.rs:1840` `crate::krandom::init()`。
- **最小修复**：表内补一行「`krandom.random_sources/elements` | `krandom::init()`（在 `init_proc_and_boot` 内完成） | 时点不同：Rust 提前到主流程」。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若表有意只列「抑制位收尾相关」步骤，则标题「步骤序（与 C 的 bsp_finish_booting 对照）」给出了穷举的预期，建议收窄标题或补行。

---

## SUGGESTION RD06-SUG-01

- **位置**：全文引文块（影响面：§2.3 两块、§3.2 两块、§3.5 两块、§3.7 一块、§3.8 一块、§4.2 一块、§4.5 set_sendto_bit 上下文注释引用）
- **建议**：引文保真的系统性补强。本篇 96 个引文块中实测约 9 个存在「无标注删减/改写换行/中文转写」，其中 3 个还含有源码中不存在的内部编号（D-58、NK4-A+日期、SD-17+日期），且注解一致声称「按原文/按原文保留」。建议：(a) 对 Rust 引文块建立与 C 块同强度的逐字门（现有 citation 门显然只对部分块生效——checked=34/96，且这 9 块似乎都在 skipped=62 里）；(b) 把「引文内允许出现的仓库内部编号」改为必须与源码 `grep` 命中，未命中即门失败；(c) 明确一条约定：引文块内禁止翻译。
- **严重度**：建议（非缺陷）；根因已在 FACT-006/009/011/014/016 逐块列出。

## SUGGESTION RD06-SUG-02

- **位置**：§3.8（VM_BOOT_EXCLUSION_CAP）与 §4.7（「排除集长度 NR_BOOT_MODULES + 1」）
- **建议**：本篇对 Rust 侧「当前实现」的陈述至少有两处停在旧版本（排除集容量 +1→+3、globals 静态清单 6→9 且含不存在的 FREE_MEMMAP）。建议对「常量值/清单列举」类陈述统一给出「复算命令」，并在文档头部标注对源的快照日期；R06 的修复轮可将这两处作为「文档-代码同步七项」的实跑样例。
- **严重度**：建议（事实修正已列 FACT-012）。

---

## 已核对无误的条目（全篇汇总，随查随记，供 VERIFY 参考）

**§2.4–§2.9 补充**：
- §2.4：`table.c` `image[]` 引文块（L98-129，含表头注释与空行）与源码 diff=0 逐字 ✓；mini_notify 引文（L897-903）逐字 ✓；has_pending 引文为源码 `#else`（非 SMP）分支逐字 ✓，块注「省略 CONFIG_SMP 替代分支与两层循环」与源码相符 ✓；init_vm 引文逐字 ✓；「RS 槽 7、DS 槽 11、中间 MEM 8/SCHED 9/TTY 10、PM 5/VFS 6」与 `NR_TASKS+proc_nr` 全部一致 ✓。
- §2.5：proc_init 引文块 diff=0 逐字 ✓；`isemptyp` 判据 `p_rts_flags == RTS_SLOT_FREE`（proc.h:281）✓；`isemptyn` ✓；IDLE=−4/ASYNCM=−5/CLOCK=−3/SYSTEM=−2/KERNEL=−1（com.h:47-51）✓；idle_proc 在 cpulocals.h:42（每 CPU 结构）✓；`clear_endpoint` panic 文案「clear_proc: empty process: %d」逐字 ✓（system.c:543，逗号后双空格为源码原样）；「第二个循环没清 s_flags」✓。
- §2.6：循环尾引文（L1078-1099）diff=0 ✓；`iskerneln(n)=((n)<0)`、`isrootsysn(n)==ROOT_SYS_PROC_NR`（proc.h:276,279）✓；12 动作表与 main.c 循环体逐条对应 ✓；`RTS_SET(rp, RTS_NO_PRIV|RTS_NO_QUANTUM)`（main.c:253）✓；双闸 `p_nr != VM_PROC_NR && p_nr >= 0`（main.c:264-267）✓。
- §2.7：arch_boot_proc 引文块 diff=0 逐字（含 `// linear address space` 原注释）✓；libexec_pg_alloc 引文逐字 ✓；arch_proc_init 前两句 arch_proc_reset+strlcpy、pc/sp/bx 三写（i386/memory.c:722-731）✓；`enable_iop` 的 `pp->p_reg.psw |= 0x3000`（i386/protect.c:51）✓；panic("no entry trap style known")（arch_system.c:597）✓；`panic("VM loading failed")`（protect.c）✓；boot.rs ProcKind/RootService 角色注释、load_process_elf 仅 test-sysboot 调用（grep 全树仅 main.rs:227/254 + lib.rs 再导出）✓；ARGC_ARGV_ENVP 旧值 `2*usize + i32`、4-mod-8、「无 64 位 C 真源」、消费端 startup-minix.c 无 argc 读者各条与 boot.rs:434-444 注释一致 ✓；add_memmap+置 0 成对（protect.c:1178-1179）✓；VM init_proc 线性扫描按 proc_nr 匹配 + panic("no init_proc")（vm/main.c:262-285）✓。
- §2.8：endpoint.h 三宏逐字 ✓；`_ENDPOINT_GENERATION_SHIFT=15` ✓；`MAX_NR_TASKS=1023`（com.h:55）✓；do_fork 代号递增引文逐字（do_fork.c:59-71，`++gen >= _ENDPOINT_MAX_GENERATION → gen = 1`）✓；isokendpt_f 三条件 `isokprocn && !isemptyn && p_endpoint == e`（proc.c:1849-1851）✓；宏族表 9 行与 proc.h:263-279 逐一对应（含 `isuserp(p)=isusern((p) >= BEG_USER_ADDR)` 原文如此）✓。
- §3.1：PROC_TABLE/PRIV_TABLE globals.rs 注释引文逐字 ✓（PRIV_TABLE 注释确为「Same storage / safety model as `PROC_TABLE`」）；ProcessTable 三字段（procs/sched/vm_request_queue）✓；ProcessTable::new 引文逐字、省略注解（idle_idx 三赋值，proc_table.rs:96-104）✓；nr_to_idx 引文逐字 ✓；is_kernel `nr.0 < 0` ✓；get/get_mut/is_valid_nr/is_empty 均存在 ✓。
- §3.2：CapabilityTemplate 五变体与注释引文（含 `...` 标注）✓；模板表五行与 capability.rs trap_mask/ipc_mask/kcall_mask 实现一致 ✓；grant_capability 引文逐字（除 RD06-FACT-010 的换行）；CLOCK/SYSTEM 例外用 `TrapMask::RECEIVE`（C `CSK_T = 1 << RECEIVE` 同形）✓；「main.c:218-219」坐标正确 ✓；CapabilityError 注释引文逐字 ✓；fill_sendto_mask 末段注释（除 D-58）逐字 ✓。
- §3.3：RtsFlagsBits 16 位逐值引文逐字 ✓；MiscFlagsBits 18 位逐值 ✓、`EXT_REG_INITIALIZED = 0x1000` 改名如实陈述 ✓；RtsFlags(AtomicU32) 引文 ✓；proc_table.rs 模块注释「All access … Big Kernel Lock (BKL)」逐字 ✓；rts_set 引文逐字 ✓；is_kernel_task 引文逐字 ✓。
- §3.4：ProcKind 引文（除 FACT-006 所记 protect.c:388 片段）语义逐字 ✓；proc_kind 映射段引文逐字（lib.rs:2743-2753）✓；EntrySpec 引文逐字 ✓；KERNEL_TASKS 引文逐字（注释对齐空格差异属排版）✓；BOOT_MODULE_PROC_NRS 类型 `&[ProcNr; NR_BOOT_MODULES]`、12 项顺序 DS/RS/PM/SCHED/VFS/MEM/TTY/MIB/VM/PFS/MFS/INIT 与 §1.4.1 一致 ✓；MODULE_NAMES 长度断言（boot-shim loader.rs:428）与 minix-boot kernel_info.rs:685 `MODULE_NAMES: [&str; NR_BOOT_MODULES]` 共用常量 ✓。
- §3.5：boot.rs「Why a single trait」模块注释逐字 ✓；lib.rs 别名段注释逐字 ✓；enable_user_io 引文（除首行未录，见 FACT-006 族）逐字 ✓；build_cpu_context 调用段（lib.rs:2675-2680）✓；mod.rs:35 proc_arch 移除记录 ✓；C 侧 `arch_proc_reset`/`arch_proc_init`/`arch_boot_proc` 三函数与 i386/earm 双份装载半的结构陈述与源码相符 ✓。
- §3.6：FPU 模块注释逐字 ✓；x86_64 `X86FpuInitPolicy{KernelTask,LazyUserInit}` ✓；aarch64 `fpu_enable_el0: bool` 及两条注释（per-dispatch CPACR 写已移除 boot.rs arm64:93-106、finish_and_restore 归属）✓；riscv64 `INIT_USER_SSTATUS=0x20`/`INIT_TASK_SSTATUS=0x100`、命名注释指向本篇 §3.4 ✓；fpu_state 四条尺寸行（0/512/528/264）与 proc.rs:1082-1085 逐字 ✓；C 坐标 archtypes.h:35（`char *fpu_state`）与 arch_system.c:144（池声明）✓；`proc_used_fpu` 宏 ✓。
- §3.7：VmLoadError 四变体与注释（除 NK4-A 后缀）逐字 ✓；调用点 panic 文案 `load_vm_elf: VM ELF is required at boot: {e:?}` 逐字 ✓。
- §3.9：lib.rs 置位两段引文（除 FACT-013 的坐标）✓；bsp_finish_booting 解除循环引文逐字（`.0` 换行属排版）✓；NO_PRIV 运行期解除点 syscall.rs:1804 与 backup-sig-mgr syscall_signal.rs:291 存在 ✓；「RTS_NO_ENDPOINT 不在四道抑制位内」✓。
- §3.10：8 子结构段头注释逐字（kpriv.rs:77-81）✓；子结构字段表与 kpriv.rs 定义逐一对应（PrivSignals 9、PrivIo 4、PrivMem 5、PrivRuntime 6、PrivIpc 3、PrivIdentity 2、PrivFlags 1、PrivInit 1，合计 31）✓；KPriv::new_zeroed 引文逐字 ✓；PrivTable::new 引文逐字（kpriv.rs:845-852）✓；ppriv_addr 声明与 priv_addr 宏（kernel/priv.h:79,95）✓。
- §3.11：Priority 引文逐字 ✓；CpuMask 注释引文逐字（字段 bits: u64 省略已声明）✓；SchedFields 字段引文（`...` 已声明）✓；Quantum 字段 `cpu_time_left: AtomicU64`/`size_ms: AtomicU32` ✓；consume 生产调用 clock.rs:697（decrement_quantum_in_with_delta）✓、allocate 仅测试（clock.rs:2553+ 位于 #[test] 段）✓；C proc_no_time 判据逐字 ✓；Accounting/TimeStats/CyclesStats 分组与字段 ✓。
- §3.12：init_post_and_memory 文档注释引文逐字 ✓（含 arch_do_vmctl.c:19-33/:25 坐标如实转述）；两条根字段断言引文逐字 ✓；「注释第 2 条与函数体有偏离」的自标注与实况相符 ✓。
- §3.13：fork FPU 继承分支引文逐字（源码为三行 if 条件、文档一行——排版差异）；「§3.14 旧编号、登记 SRC-10」自标注 ✓（源码注释确实写 §3.14）；x86_64 inherit_fpu_state 注释前三行逐字 ✓；fork_from 清单（NO_QUANTUM 置、SIGNALED|SIG_PENDING|P_STOP 清、misc 五位清、p_time、p_pending）与 proc.rs:1718 前注释逐项一致 ✓；do_fork.c:57 save_fpu 提及 ✓。
- §3.14/§4.9：去向总表与对照表各行与正文对应节一致；`[KProcess; 261]`/`[KPriv; 64]` ✓；初始栈 64 KiB（C protect.c `execi.stack_size = 64 * 1024`）→ 256 KiB（boot.rs `VM_STACK_SIZE = 256 * 1024`）✓。
- §4.1：x86_64 build_cpu_context 引文逐字 ✓；aarch64/riscv64 build_cpu_context 引文逐字 ✓；`let _ = ctx.fpu_policy;` + #NM 注释（x86_64 boot.rs:164-165）✓；「ps_strings 承载寄存器 rbx/r0/a0」与 EntrySpec 注释一致 ✓；riscv64 两初值单比特陈述（0x100=SPP、0x20=SPIE）与注释「SPP=0, SPIE=1」方向核对一致（task=SPP=1、user=SPIE=1）✓；enable_user_io 空 impl（arm64 boot.rs:109）✓。
- §4.2：共享边界页注释引文逐字 ✓；驱逐保护引文逐字 ✓；② 段注释引文逐字 ✓；「pg_map 对 vaddr % I386_PAGE_SIZE 有断言」✓（pg_utils.c:278 `assert(!(vaddr % I386_PAGE_SIZE))`）；「this loader deliberately accepts mid-page segment starts…」注释 ✓；icache 两条测试名 `load_vm_elf_syncs_icache_over_exec_segment_frames`/`load_vm_elf_skips_icache_sync_for_non_exec_image` ✓（boot.rs:1576/1656）；栈注释引文（除末行，见 FACT-006 族）✓；VM mappedin 自扩机制（vm/alloc.c:64-164）✓；`#PF err=0x15` 记录 ✓（boot.rs:527-528）。
- §4.3：boot_unchecked 两行引文逐字（lib.rs:1846-1848）✓；宏族对照表 8 行与源码一致 ✓；`sched: [Scheduler; MAX_CPUS]`/`vm_request_queue` 注释对位（run_q_head/proc.c 坐标属源码注释原文）✓。
- §4.4：字段差异表逐项核对：p_magic 仅 debug_assertions ✓（proc.rs:927）；SigSet(u128) ✓；p_vm_suspend `Option<VmSuspendContext>` + 不变式注释 ✓（proc.rs:1105-1106）；Drop 引文逐字 ✓（proc.rs:1160-1167）；swap_slots 用 core::mem::swap ✓（proc_table.rs:139-151）；debug.rs `found` 数组 ✓（debug.rs:55）；release_address_space ✓（syscall.rs:1505）。
- §4.5：内核 task 模板选择与回链引文逐字 ✓（lib.rs:2665-2669，「E1 slice 5」确在源码）；fill_sendto_mask 引文逐字 ✓（kpriv.rs:1009-1017）；set_sendto_bit「先读后借」段引文逐字 ✓（kpriv.rs:959-968）；C 坐标 system.c:316-319/321-328 与源码相符 ✓；get2_mut/split_at_mut 存在 ✓。
- §4.6：PrivUpdateRequest 注释引文逐字 ✓；capability.rs「Storage in KPriv」段逐字 ✓；ProcessCapability 11 个 C 位值逐一（const.h:143-154）✓；扩展位 KILL 0x10000/SIGS_SYS 0x20000/OWN_ID 0x40000 ✓；to_wire 截断语义 ✓；wire 宽度表 ✓。
- §4.7：12 动作对位表 ✓；堆窗口驱逐引文（除 panic 闭包换行排版）逐字 ✓；「驱逐在 load_vm_elf 返回之后」✓；两条授予路径各一处 `priv_id = Some`（lib.rs:2669/2728）✓。（「排除集长度 +1」一条除外，见 FACT-012。）
- §4.8：Rust 侧 `VM_RUNNING.store(false)`/`bsp_local.fpu_presence = true`/`debug_assert!(smp::bkl_is_locked())`/`KERNEL_MAY_ALLOC.store(false)` 均存在 ✓；「C 在 bsp_finish_booting 内没有锁动作」✓（C 函数体无锁调用）。
- §5.1：八个文件的 `#[test]` 条数 43/41/39/15/14/14/5/4 实测一致 ✓；五个代表性测试名（kernel_task_uses_init_task_psw、user_process_uses_init_psw、inherit_fpu_state_propagates_lazy_user_policy、inherit_fpu_state_propagates_fpu_enable_el0、inherit_fpu_state_copies_sstatus）逐字存在 ✓（x86_64/arm64/riscv64 boot.rs）。
- §5.3：proc_table.rs 15 组测试名、kpriv.rs 8 组、arch/boot.rs 7 条全部逐字命中源码（test_priv_flag_set_usr_no_sys_proc 等）✓；「grant_capability 一组」「set/fill 一组」的归组与实际测试名单相符 ✓。
- §5.4：test-proc-init 载体存在（os/qemu-tests/test-kernels/kernel/bootstrap/test-proc-init/），run_all.sh:130 `run_test "test-proc-init" x86_64 …efi` ✓；载体头部注释 7 点覆盖与文档逐点对应 ✓；「本轮未实跑」自披露一致 ✓；RTS_NO_ENDPOINT「已覆盖：dispatch_clear 回归测试」与 §2.2.2 对位 ✓（syscall_process.rs dispatch_clear 测试存在——与命令 8 计数一致）。
- §6.1/§6.2：8 条诊断命令全部实跑，7 条输出与预期完全一致（命令 1-8，含 16/18/4、40/31、17、3 行、2 行、2 行、Rust 计数）；症状表 6 行的期望读数与源码核对无误（除 §6.2 命令 3 口径注释见 FACT-001）。
- §7：参见清单的文件全部存在；related-code.txt 与 §7.3 清单一致。

**说明**： bitchunk_t 的 typedef 在本仓裁剪后的 minix3/include 树内检不出（来自 libc 头），「sys_map_t 两块 32 位」「BITMAP_CHUNKS(64)=2」按 i386 下 `bitchunk_t=unsigned long` 的标准定义判定为一致，标记为 E3 级佐证；如需 L1 证据须在真实构建头中查（不影响任何正文结论）。

**结论摘要**：文档的机制叙事、常量、位表、映射、测试名与计数绝大多数与源码一致（引文块逐字率约 87/96，诊断命令 8/8 输出吻合）；问题集中在三类：① Rust 引文块的保真（9 块改写/删减、3 块含源码不存在的内部编号——FACT-006/009/010/011/014/016）；② 少量过时陈述（Cargo.toml 形态、排除集容量、globals 清单——FACT-012/015）；③ 零散坐标与序数（FACT-003/005/007/008/013/017/018）。无 P0。


**§1–§2.3（早期核对，与上文合并）**：

- §1.2/§6.2：`NR_TASKS=5`（com.h:56）、`NR_PROCS=256`（sys_config.h `_NR_PROCS`，经 config.h:31）、`NR_SYS_PROCS=64`（sys_config.h:9）、`NR_BOOT_PROCS=17`、`NR_BOOT_MODULES=12`、`USER_PRIV_ID=16`、`PROC_TABLE_SIZE=261` 全部实跑复核一致。
- §1.2：`proc_addr(n) = (&(proc[NR_TASKS + (n)]))`（proc.h:269）✓；`static_priv_id`/`USER_PRIV_ID`（include/minix/priv.h:12,18）✓；`LAST_SPECIAL_PROC_NR=11`（com.h:70）✓；`NR_STATIC_PRIV_IDS = NR_BOOT_PROCS`（include/minix/priv.h:10）✓。
- §1.2/§2.3：`struct priv` 31 字段（grep 计数 31）✓；`s_io_tab[64]`/`s_mem_tab[20]`/`s_irq_tab[16]`（config.h:52,55,58）✓；`sys_id_t=short`、`sys_map_t` 位图（kernel/type.h:9-13）✓；`NR_SYS_CALLS=58`、`SYS_CALL_MASK_SIZE=BITMAP_CHUNKS(NR_SYS_CALLS)`（com.h:270,272）→ 2 块 ✓。
- §1.3：`image[]` 17 条目（grep 计数 17）✓；表头注释引文逐字 ✓（未比对全块，见后续）；`struct boot_image` 五字段（include/minix/type.h:148-153 附近）✓。
- §1.4：`bsp_finish_booting` 解除循环上界 `NR_BOOT_PROCS - NR_TASKS`（main.c:64）✓；`RTS_UNSET(proc_addr(i), RTS_PROC_STOP)` ✓。
- §1.4.1：映射表 17 行（ASYNCM=−5…INIT=11；DS 模块 0/proc 6、RS 模块 1/proc 2、PM 模块 2/proc 0）与 com.h 的 *_PROC_NR 值逐项一致 ✓。
- §2.0：`kernel_may_alloc = 1/0` 及两处注释原文（main.c:134,104）✓；`proc_init()` 与 `IPCF_POOL_INIT()` 相邻两行（main.c:157-158）✓；boot 循环 `NR_BOOT_PROCS` 仅两处 for 上界（main.c:64,165）✓。
- §2.1：字段分组表 6+7+8+5+11+2+1 = 40 与源码逐一对应 ✓；`PROC_NAME_LEN=16`（type.h:145）✓；`PMAGIC=0xC0FFEE1`（const.h:164）✓；`proc_ptr_ok`（proc.h:174）✓；`proc_nr_t=int`（kernel/type.h:9）✓。
- §2.1.2：`NR_SCHED_QUEUES=16`、`TASK_Q=0`、`MIN_USER_Q=15`、`USER_Q=7`、`SRV_Q=USER_Q`（config.h:66-71、include/minix/priv.h:93）✓；boot 循环仅 VM 与 rootsysn 两分支写 `p_priority = SRV_Q`（main.c:210,237）✓；`proc_kernel_scheduler` 判 NULL 或自身（proc.h:178-179）✓；`p_cpu_mask` 全树仅声明 + do_update.c:255 memcpy ✓。
- §2.1.5：`reset_proc_accounting` 引文逐字 ✓（六字段、顺序一致）。
- §2.1.6：16 个 RTS 位值逐位 ✓；`0x2000` 空缺、`RTS_PREEMPTED=0x4000` ✓；`#define RTS_` 共 20 行、4 操作宏 ✓；18 个 MF 位值逐位 ✓；`0x10/0x20/0x800` 空缺 ✓；`MF_MSGFAILED` 源码无注释、文档如实标注 ✓；`do_clear` 引文（L477-486）逐字含尾部空格 ✓。
- §2.2：RTS 位定义引文块（L497-527）与 proc.h:140-176 逐字一致（含多行注释）✓；RTS_ISSET/SET/UNSET/SETFLAGS 四宏引文（L536-567）逐字 ✓；`rts_f_is_runnable(flg) ((flg) == 0)` ✓；`P_BLOCKEDON` 注释「两 位可同时置」（proc.h:182-186）✓。
- §2.2.2 表：`RTS_SLOT_FREE` boot 循环末 `&= ~`（main.c:268）✓；`RTS_PROC_STOP` 置位点 proc_init idle（proc.c:156）、kmain 全 17 项（main.c:269）、do_runctl、smp.c:119/168、解除点 bsp_finish_booting（main.c:65）、do_runctl RC_RESUME、smp.c:153 迁移收尾、save_fpu CONFIG_SMP（arch_system.c:129）✓；`RTS_SIGNALED|SIG_PENDING` 置位 system.c:444（cause_sig）✓；do_getksig.c:34 清 SIGNALED、do_endksig.c:36 清 SIG_PENDING、do_fork.c:122 三位不继承 ✓；`RTS_VMREQUEST` 置位 vm_suspend（proc.c:234,244）、解除 clear_memreq（system.c:488,503）✓；`RTS_PREEMPTED` 置位 enqueue 抢占分支（proc.c:1639）、smp.c:202、解除 switch_to_user 段 ✓；`RTS_NO_QUANTUM` 置位 kmain:253/do_fork.c:90/sched_proc（system.c:674,678）/notify_scheduler（proc.c:1868）、解除 sched_proc 末尾（system.c:697）✓；`RTS_VMINHIBIT`/`RTS_BOOTINHIBIT` 置位 kmain:265-267（`p_nr != VM_PROC_NR && p_nr >= 0`）✓。
- §2.3.1：`get_priv` 主体逐字 ✓（除 FD-006 所述注释行）；`EINVAL`/`EBUSY`/`ENOSPC` 三错误码 ✓。
- §2.3.2：模板常量 `NO_M/ALL_M/NO_C/ALL_C/NULL_C/IDL_F/TSK_F/SRV_F/DSRV_F/RSYS_F/VM_F/USR_F/IMM_F/CSK_T/TSK_T/SRV_T/DSRV_T/USR_T` 与 include/minix/priv.h 一致 ✓（IDL_F 多行注释按块头声明以 `...` 缩短，属有标注省略）。

- 本次扫描：18 条 FINDING，2 条 SUGGESTION。
