# P-FACT 独立事实核对 — 05-clock-interrupt-init.md（round1）

审查者：P-FACT。对象：`rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md`（2198 行）当前版本；源码 `minix3/` 与 `os/`。只读核对，逐条给 path:line。证据分级：E1 = 命令/工具输出可重放，E2 = 人工读源判定，E3 = 推理。

citation 门实测读数：blocks=62 checked=40 failed=3 skipped=14 warned=5。3 条 failed 全部定位并判定完毕，见 FACT-01/02/03（均为文档侧漂移，代码未改）。

总口径：C 侧全部常量、行号、函数体引用逐一对上（cstart/init_clock/intr_init/arch_init 两份/omap 两文件/init_local_timer/estimate_cpu_freq/register_local_timer_handler/put_irq_handler/boot_cpu_init_timer/bsp_finish_booting）；Rust 侧 trait 签名、结构体字段、常量、错误路径、expect 文案、测试名与计数全部对上。以下 11 条 FINDING 是全部不一致项，按根因合并。

## FINDING RD05-FACT-01（P2）

- 位置：文档 1048–1086（§4.1 引文块，含 1049 行块注）；源码 `os/kernel/src/lib.rs:2407-2411`
- 严重度：P2（citation 门 failed #1；引文逐字漂移，无语义错误）
- 问题：三条逐字漂移。① 文档 1053 行 `use minix_plat::{InterruptRouter, CurrentInterruptController};`，源码 2410 行是 `use minix_plat::{CurrentInterruptController, InterruptRouter};`（花括号内名字序不同）；② 文档 1054 行 `{platform_desc, PlatformDesc}`，源码 2411 行是 `{PlatformDesc, platform_desc}`；③ 1049 行块注声称"开头 `use minix_arch` 的跨行写法已并为一行"，但源码 2409 行本来就是单行 `use minix_arch::{ArchInit, CurrentArchInit};`，不存在任何跨行 `use`。
- 影响：节选声称为"原文（节选）"，逐字对账失败；块注对源码形状的描述失实，读者会以为源码里有跨行 use。
- 证据或推理：E1（citation-check 输出 `[FAIL] block 1048-1086 …缺: use minix_plat::{...}`；`sed -n '2407,2411p' os/kernel/src/lib.rs`）。三条均在文档侧：源码的四个 use 各占一行、顺序与文档不同。
- 最小修复：引文块 1051–1054 改回源码逐字顺序（`use crate::clock::ClockState;` → `use minix_arch::{…};` → `use minix_plat::{CurrentInterruptController, InterruptRouter};` → `use minix_platform::{PlatformDesc, platform_desc};`），删除"跨行写法已并为一行"半句。
- 置信度：高
- 是否需源码核实：已核实
- 若我错了：若 repo 存在另一份多行 use 的历史形态且块注指的是它，则只需删该半句、保名字序修正——但当前工作区源码就是单行，判定不变。

## FINDING RD05-FACT-02（P2）

- 位置：文档 1682–1709（§4.7 引文块，含 1683 行块注）；源码 `os/kernel/src/lib.rs:3750-3758`
- 严重度：P2（citation 门 failed #2；引文逐字漂移）
- 问题：① 文档 1691 行把源码 3755–3758 的三行调用 `clock_arch.init_timer(\n crate::clock::DEFAULT_HZ,\n crate::clock::current_cpuid().raw(),\n );` 合并成一行，且 1683 行块注"省略号处依次为函数头文档注释、三段之间的解释性注释，以及仅 x86_64 的 `pic_init` 尾调用"没有覆盖这一改写；② 文档 1687–1688 行 `use minix_types::Endpoint;` 在 `use minix_platform::platform_desc;` 之前，源码 3751–3752 行顺序相反（platform_desc 在前）。
- 影响：同 FACT-01，节选非逐字；且块注声明了省略号的全部范围，漏报了行合并与 use 换序两类改写。
- 证据或推理：E1（citation-check 输出 `[FAIL] block 1682-1709 …缺: clock_arch.init_timer(…)`；`sed -n '3750,3758p'`）。
- 最小修复：1691 行改为源码的三行写法（或块注追加"init_timer 调用的跨行写法已并为一行、两条 use 已换序"），1687–1688 按源码顺序调换。
- 置信度：高
- 是否需源码核实：已核实
- 若我错了：无此风险——源码三行形态是唯一形态，checker 与人工读码一致。

## FINDING RD05-FACT-03（P2）

- 位置：文档 1979–2008（§5.2 引文块，1984 与 1990 两行）；源码 `os/qemu-tests/test-kernels/kernel/bootstrap/test-timer-irq/src/main.rs:155, 204, 211`
- 严重度：P2（citation 门 failed #3；引文含虚构的变量路径）
- 问题：文档写 `unsafe { minix_platform::init_from_kinfo(&result.kernel_info) };` 与 `minix_kernel::init_proc_and_boot(&result.kernel_info);`；源码是 `let kernel_info = KernelInfo { … }`（155 行）后直接传 `&kernel_info`（204、211 行）。整个 x86_64 载体（含 aarch64/riscv64 两份载体）不存在 `result` 变量或 `.kernel_info` 字段访问。
- 影响：引文里的表达式在源码中不存在，且 `result.` 前缀会让读者误以为 init_from_kinfo 的入参来自某个 bootstrap 结果结构体的字段——这是 API 形状层面的失实，不只是排版漂移。
- 证据或推理：E1（citation-check 输出 `[FAIL] block 1979-2008 …缺: unsafe { minix_platform::init_from_kinfo(&result.kernel_info) };`；`grep -n 'let result\|kernel_info =' …/test-timer-irq/src/main.rs` 仅命中 155 行的 `let kernel_info = KernelInfo {`；对三份载体 grep `result.kernel_info` 均零命中）。
- 最小修复：1984、1990 两行的 `&result.kernel_info` 改为 `&kernel_info`。
- 置信度：高
- 是否需源码核实：已核实
- 若我错了：若该块源自某个历史版本的载体（曾返回 result 结构体），则属旧版引文未随源码同步——结论不变，仍需按现行源码修正。

## FINDING RD05-FACT-04（P2）

- 位置：文档 1727–1735（§4.7 x86 注释节选，块注"注释有删节"）与 1740–1765（§4.7 `pic_init` 引文块，块注"注释按原文"）；源码 `os/kernel/src/lib.rs:3779-3785`、`os/plat/src/x86_64/interrupt.rs:81-101`
- 严重度：P2（引文块声称"按原文"，实际英文注释被整体中译，其中一处还插入了源码里不存在的标记）
- 问题：① 文档 1730 行"（s14n：150s 全程 tick 臂 <1000 次调用）"——源码 3782 行是 `(over 150 s the tick arm was called fewer than 1000 times)`，没有 `s14n` 这个记号；整段注释在文档里是中文、源码是英文。② `pic_init` 块内五条行内注释全部中译（如"master ICW2：向量基 0x50"，源码 87 行是 `// master ICW2: vector base 0x50`；"IMR：全 mask（IRQ0/IRQ2 由 unmask_irq 按需打开）"，源码 98 行是 `// IMR: mask everything (IRQ0/IRQ2 are opened on demand by unmask_irq).`），而块注写"注释按原文"。
- 影响：这两块是引文块且声明了原文保真，中译+插入记号违反引文纪律；机械 checker 对注释不敏感所以没报，属于 gate 盲区下的漂移。
- 证据或推理：E1（`sed -n '3779,3785p'`、`sed -n '79,101p'`；grep `s14n` 在 lib.rs 与 interrupt.rs 均零命中）。
- 最小修复：两块注释恢复英文原文；若确需中译注释，把块注改为"注释已中译"并在 s14n 处注明是文档侧标记（或删掉）。
- 置信度：高
- 是否需源码核实：已核实
- 若我错了：若"s14n"是仓库某规范文件定义的评审标记且约定可入引文，则只余中译问题；当前 lib.rs 注释内无此记号是事实。

## FINDING RD05-FACT-05（P2）

- 位置：文档 537（§2.6 限定段："`apic.c:139`、`:541-542`"）；源码 `minix3/minix/kernel/arch/i386/arch_clock.c:139`、`minix3/minix/kernel/arch/i386/apic.c:139, 526-543`
- 严重度：P2（锚点文件名错误；机制陈述本身正确）
- 问题：`init_local_timer` 的 APIC 支路里 `lapic_set_timer_one_shot(1000000 / system_hz)` 的调用在 `arch_clock.c:139`（init_local_timer 函数体内），不是 `apic.c:139`——apic.c:139 是 `#define lapic_write_icr2(val) …` 宏行。`:541-542`（apic.c 内 `lapic_set_timer_one_shot` 函数体 526–543 末尾写 `LAPIC_LVTTR`）锚点正确。
- 影响：读者按锚点去 apic.c:139 会落到一个 ICR2 宏上，找不到"编程段开门"的调用点。
- 证据或推理：E1（`grep -n 'lapic_set_timer_one_shot' arch_clock.c` → 139；`sed -n '135,142p' apic.c` → lapic_write_icr2 宏；`sed -n '526,543p' apic.c` → 函数体）。源码另一处自证：`os/arch/src/arch/clock.rs:55-56` 的注释也写 "arch_clock.c:139"。
- 最小修复：537 行 `apic.c:139` 改为 `arch_clock.c:139`。
- 置信度：高
- 是否需源码核实：已核实
- 若我错了：无——两个文件 139 行内容均已直接读出。

## FINDING RD05-FACT-06（P2）

- 位置：文档 960（§3.6 理由 1 后半句）；源码 `os/kernel/src/lib.rs:1741-1751, 2827-2828`
- 严重度：P2（文档描述已被移除的代码；与现行设计陈述矛盾）
- 问题：文档称"内核自己那一侧还有一组 `cut_memmap` 调用，跑在 Phase A.2，切的是 boot module 区间与内核镜像本身"。现行 lib.rs 1741 行写明 "Phase A.2 is intentionally empty: the kernel keeps no mutable copy of the boot memmap. C's `kinfo.memmap[]` bookkeeping (pre_init.c add/cut) is served by construction here…"，保留改由"A2 deduction record"承担；`grep -rn cut_memmap os/ --include=*.rs` 只命中注释（lib.rs:2827"undoes the cut_memmap from Phase A.2"是旧设计的残句）与 `os/target/fmtchk/` 下的陈旧构建副本，无任何实际调用。文档同段前半句"KernelInfo.memmap 的提供者已经把保留区域排除掉了"与现行代码一致——矛盾出在文档自己把新旧两套说法并置。
- 影响：§3.6 的三条理由之一建立在不存在的调用上；读者会去找 Phase A.2 的裁剪实现而找不到。同段引用的 `01-boot-shim-bootstrap.md §2.4` 是否同步未在本次范围内。
- 证据或推理：E1（sed 1741-1751；grep -rn cut_memmap os/ --include=*.rs）。
- 最小修复：960 行改为与 lib.rs:1741-1751 同步的陈述（Phase A.2 为空、保留由构造与 A2 deduction record 保证），或删去后半句只留"提供者排除"。
- 置信度：高
- 是否需源码核实：已核实
- 若我错了：若 Phase A.2 的 cut_memmap 属于另一条 kmain 路径（如 bootstrap 变体），需要给出该调用的 path:line——本次全仓 grep 未找到任何活代码调用。

## FINDING RD05-FACT-07（P3）

- 位置：文档 2010（§5.2 判定标准引文）；源码 `os/qemu-tests/test-kernels/kernel/bootstrap/test-timer-irq/src/main.rs:12-13`
- 严重度：P3（反引号引文与源文措辞不符）
- 问题：文档引 `PASS = uptime reaches 5 ticks with interrupts delivered through the S-8 stub path`；源文件顶部注释实际是 "…delivered through the trap stub path…"。"S-8" 在该文件出现于别处（124 行横幅、199 行进度行），不在这句判定标准里。
- 影响：轻微；引文措辞失真，读者 grep "S-8 stub path" 顶部注释会落空。
- 证据或推理：E1（sed 12-13；grep -n 'S-8' 仅 124/199）。
- 最小修复：改为 `…through the trap stub path`（或注明改写）。
- 置信度：高
- 是否需源码核实：已核实
- 若我错了：无。

## FINDING RD05-FACT-08（P3）

- 位置：文档 1405 与 1435–1444（§4.4 aarch64 引文块第三函数）、1796（§4.7.1 要点锚 `fn init_cpu_interface`）；源码 `os/plat/src/arm64/interrupt.rs:143-153, 220-222`
- 严重度：P3（引文把辅助函数的函数体安到了委派函数名下）
- 问题：文档块显示 `fn init_cpu_interface` 内含 SRE/PMR/IGRPEN1 三条 asm；实际 `init_cpu_interface`（220–222 行）只有一行 `Self::init_cpu_interface_current();`，三条 asm 在 `init_cpu_interface_current`（143–153 行）。块注"三段之间的注释块"未覆盖这一"函数体换名"。
- 影响：按文档锚点读 `fn init_cpu_interface` 会得到空函数；§4.7.1 的写者分析（"ICC_IGRPEN1_EL1 的写者只有控制器初始化一处"）不受影响，但落点函数名不精确。
- 证据或推理：E1（sed 143-153、220-222）。
- 最小修复：块内第三个函数改名 `init_cpu_interface_current`（或保留 init_cpu_interface 名并在块注声明"函数体位于其调用的 init_cpu_interface_current"）。
- 置信度：高
- 是否需源码核实：已核实
- 若我错了：无。

## FINDING RD05-FACT-09（P3）

- 位置：文档 2010（§5.2 行锚 "`test-timer-irq-riscv64/src/main.rs:365-369`"）；源码 `os/qemu-tests/test-kernels/kernel/bootstrap/test-timer-irq-riscv64/src/main.rs:360-364`
- 严重度：P3（行锚偏移约 5 行）
- 问题：`### uart-rx: no first-source within window …` 打印在 360–362 行，`### TEST_RESULT: FAIL test-timer-irq-riscv64 ###` 在 364 行；文档写 365–369。
- 影响：轻微；行号对不上。
- 证据或推理：E1（sed 360-372）。
- 最小修复：行锚改为 360–364。
- 置信度：高
- 是否需源码核实：已核实
- 若我错了：无。

## FINDING RD05-FACT-10（P3）

- 位置：文档 721（§3.1 ClockArch 签名示意中 `fn init_local_timer(…) -> u32;`）；源码 `os/arch/src/arch/clock.rs:54`
- 严重度：P3（签名示意的必选/默认标注与源码不符）
- 问题：示意块把 `init_local_timer` 画成无默认体的必选方法（分号结尾）；源码 54 行它有默认体 `fn init_local_timer(…) -> u32 {`。块注自称"默认实现以空体/零值占位"，示意内其他四个默认方法（write_local_timer_icr、local_timer_eoi、read_tsc、restart_local_timer）都画了体，唯独这个漏画。
- 影响：轻微；读者会把"AP 路径是否必须实现"判错一档。
- 证据或推理：E1（awk 提取 trait 方法表：54 行带 `{`）。
- 最小修复：示意行改为 `fn init_local_timer(…) -> u32 { … }` 或在块注补一句"init_local_timer 亦有默认体，示意从略"。
- 置信度：高
- 是否需源码核实：已核实
- 若我错了：无。

## FINDING RD05-FACT-11（P3）

- 位置：文档 1092–1109（§4.1 Step 2 x86 子步引文块）与 1135（2a 说明）；源码 `os/kernel/src/lib.rs:2437-2462`
- 严重度：P3（节选无省略号标记地删去了两条实质性检查；正文随之漏述）
- 问题：源码该段在 `assert!(lapic_region.phys_base == 0xFEE0_0000, …)` 之后还有两条 `assert_eq!`（2447–2454 校验 `intr.lapic_base()` 等于窗口页；2455–2462 校验 `intr.ioapic_base()`），文档引文从 `assert!` 直接跳到 `let lapic_va`，中间无 `…` 标记；2a 说明只讲了"IA32_APIC_BASE 给出的 LAPIC 物理页必须等于窗口里那页"这一条，lib.rs 侧的描述符 LAPIC/IOAPIC 基址 check-and-reject 未提（§4.4 只覆盖了 init_lapic 内的同型检查）。
- 影响：轻微；PD-22 检查在 lib.rs 侧的完整性被低估，读者会以为基址拒绝逻辑只在控制器 init_lapic 一处。
- 证据或推理：E1（sed 2435-2465）。
- 最小修复：引文两处 `assert_eq!` 之间补 `…` 并在 2a 补一句"描述符 LAPIC/IOAPIC 基址不等于窗口页同样当场拒绝（lib.rs 2447-2462）"。
- 置信度：高
- 是否需源码核实：已核实
- 若我错了：无。

## 已核对无误的要点（抽样列证，均 E1）

- C 常量与行号：`glo.h:38` system_hz 宏、`archconst.h:4` 两份 DEFAULT_HZ 60/1000、`interrupt.h:9-12,17-18,39,41` 端口与向量、`i8259.c:16-23` 十一条 ICW 常量与 12 行 outb、`clock.c:48/294`、`arch_clock.c:96/131/177`、`interrupt.c:67/104/155/164`、`apic.c:169/443/491/517/520/578/674/739/1125`、`main.c:403-475` cstart 全函数、`omap_intr.c:24`、`omap_timer.c:136/143-144`、`clock.h:12-17` 四声明、`hw_intr.h:22-24,45-47` 两组映射——全部与文档一致。
- Rust 侧：`ClockState` 十字段（clock.rs:1079-1106）、`IrqManager` 五字段（irq_manager.rs:453-458）、`with_hz`（clock.rs:1143-1148）、期望文案两条（lib.rs:3130/3500 等）、访问器五个（含 `try_smp_state` lib.rs:3653 确实存在）、`TIMER_IRQ` 0/30/0 与七处定义、`GICR_SGI_FRAME = 0x1_0000`、PIT 0x43/0x40/0x36 与 1_193_182、`hz >= 19` 断言、CNTP_CTL 0x2/1、`csrs sie, 0x20`、SBI 0x5449_4D45、LAPIC EOI 0x0B0、`isa_irq_to_pin` 0↔2 与引脚 2 向量 0x50、RTE 引脚 0 向量 0x52、`IF_MASK = 1<<9`（trap_return.rs:43）、`pmu_cycle_counter` 四处构造全 false、`with_hz` 无生产调用者——全部一致。
- 测试对账：kernel clock.rs 47 条 = clock_irq_handler_tests 2 条 + mod tests 45 条，其中 1 条 `#[ignore]`（1697 行）→"46 跑 + 1 忽略"成立；irq_manager.rs 10 条及四个点名测试全在；arch clock.rs 3 条、timer_irq_gate.rs 4 条（x86_64 实跑 2 的口径成立）、x86_64/interrupt.rs 5 条、arm64/riscv64/interrupt.rs 3/4 条经 plat/src/lib.rs:31-36 的模块级 cfg 门控（宿主 0）；四个 integration test 文件 3/4/3/5 条与形状钉描述（TIMER_IRQ 0/30/0 两条、FPU/trap/protection 三条）全对；载体 fail() 定义后零调用、PASS_TICKS=5、riscv `matched > 0 && now >= target` 与 `target + 10`、aarch64 载体仅 PASS 横幅、run_qemu.sh:191/206 与 test-timer-irq-aarch64.sh:91——全部一致。
- §6.2 命令预期：命令 2 = 12、命令 3 = 9、命令 5 = 5 行（169/491/517/520/578）、命令 6 = 67/164 与 104/155 分工、命令 8 = 7 行、命令 9 = 4 行 + 5 处 unmask（578/633/787/807/993）——全部复算吻合。
- 反事实前提抽查：PROBE_TICKS=system_hz/10（hz∈[2,9] 时为 0）、除数退化为 -1、`estimate_cpu_freq` 的 `while(probe_ticks < PROBE_TICKS)`（arch_clock.c:112）、ICW1 三常量做差 0x02/0x08、ICW4_AT_MASTER/SLAVE 差 bit 2 且注释逐字相同、APIC 校准两式（apic.c:517/520）、mtimecmp/CVAL 复位 0 的风暴推演、TIMER_IRQ=1 的 PLIC 位图推演——算术与源码均支持。

## SUGGESTION RD05-SUG-01

- 位置：FACT-01/02/03/04 涉及的五个引文块
- 建议：统一做一次"引文块逐字回填"：三块 failed 块按源码逐字重排；两个"注释按原文"块恢复英文注释。本篇 citation 门其余 37 块已过，说明回填工作量集中在 §4.1/§4.7/§5.2 三处。
- 收益：citation 门 failed 清零；"注释中译"这类 checker 盲区不再依赖人工发现。

## SUGGESTION RD05-SUG-02

- 位置：文档 960（§3.6）
- 建议：按 lib.rs:1741-1751 的现行设计重写理由 1 后半句（Phase A.2 为空、保留由构造与 A2 deduction record 保证），并顺带检查 `01-boot-shim-bootstrap.md` 引用的 §2.4 是否同样描述了已移除的 cut_memmap 链。
- 收益：消除文档内部新旧两套 memmap 叙述的并置矛盾。

## SUGGESTION RD05-SUG-03

- 位置：文档 1092–1109、1135（§4.1 2a）
- 建议：给被删的两条 `assert_eq!` 补省略号标记，并在 2a 补一句 lib.rs 侧描述符基址拒绝检查（与 §4.4 的 init_lapic 检查并列为两道同型闸）。
- 收益：PD-22 check-and-reject 的完整落点（lib.rs 与 interrupt.rs 各一道）在同一篇内可追溯。

- 本次扫描：11 条 FINDING，3 条 SUGGESTION。
