# RD05R2 · P-FACT 确认轮（事实审查）

- 审查对象：`rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md`（2207 行，修复轮之后全新事实核对）
- 口径：逐字核对引文块与常量/签名/字段/错误码/测试名/行号，全部证据为本次直接读取源码所得（E1 = 直接读数对账，E2 = 手工 grep/行号对账，E3 = 推断与算术）。
- 与机械门读数的关系：citation blocks failed=0 与本报告不矛盾——下述引文问题属于「被省略行未标注 / 语句重排 / 跨段拼接 / 转述标注错误」，机械门按"引文中的行逐字存在于源码"对账，查不出这几类失真；它们大概率落在 skipped=14 / warned=5 之内。
- 已核对为真且无发现的代表性项（抽样登记，证明覆盖面）：C 侧 i8259.c 全部 ICW 常量与注释、interrupt.h 四端口与 0x50/0x70/CASCADE_IRQ=2/CLOCK_IRQ=0、intr_init 十次写的实数表、DEFAULT_HZ 60/1000（两份 archconst.h:4）、init_clock 唯一赋值点、load_update 槽位算式、apic.c 行号 169/443/491/517/520/578/674/739/1125、arch_clock.c 131-153/177-194、main.c 403/472/481/505/38、bsp_finish_booting 引文、interrupt.c 65/67/104/155/164、earm 三文件引文、hw_intr.h 映射、usermapped_data.c `.usermapped`、Rust 侧 ClockArch 十二方法签名、ClockState 字段、IrqManager 五字段、plat 三 trait、三架构 init/claim/complete/pic_init/isa_irq_to_pin/ioapic_route_and_mask/mask_all、TIMER_IRQ 0/30/0 及其文档注释、arm64 GICD_CTLR 注释位宽差异（文档已登记）、timer_irq_gate 三实现、x86 hz>=19 断言、PIT 常量 0x40/0x43/0x36/1_193_182、SBI 0x5449_4D45、scause=5/stval=0x200bff8、scause=2/stval=0x3b051073、FPEN 0x0030_0000、PMU 三写值、§5.1 测试计数表全表（13 行逐一 grep 对账）、§6.2 命令 2/3/6/7/8/9 的预期读数逐条复算、载体 PASS_TICKS=5 与三架构判定文本、run_qemu.sh "no PASS marker found"、aarch64/riscv64 脚本补横幅、globals/expect 文案、Phase C/D/F 注释与 init_smp_state "自称 Phase D、调用在 Phase F" 的事实。

---

## FINDING RD05R2-FACT-01（P1）

- **位置**：文档 L752-780（§3.2，`DEFAULT_HZ` 引文块 L755-778 与其后正文 L780）；源码 `os/arch/src/arch/clock.rs` L36-43。
- **严重度**：P1。
- **问题**：引文块（标注"代码引用（文档注释与常量定义行）"）的最后一段不是原文。文档写成 "**Authoritative definition**: this is the arch-level constant. The kernel crate re-exports it（`os/kernel/src/clock.rs` 的 `pub use`）——早先 kernel 有一份独立副本……已改为单一权威定义。"，而源码实际注释是 "The kernel crate (`os/kernel/src/clock.rs`) has an independent copy to avoid a cross-crate dependency; modify here first, then sync."。即：把"合并完成后应有的注释"当作现行源码注释逐字引用。同时该源注释现在描述的状态（kernel 持独立副本）与代码事实相反——`os/kernel/src/clock.rs:710` 已是 `pub use minix_arch::arch::clock::DEFAULT_HZ;`，kernel 侧注释（L700-709）也已改为 re-export 口径，唯独 arch 侧这份注释没同步。
- **影响**：读者会以为源码注释已记录 D2-05 合并；实际 arch 侧注释仍是"双副本"旧口径，文档引用与源码脱节，且该注释是 §3.2 的核心证据之一。文档的结论本身（合并、单一权威定义）经核实为真——失真在证据文本而不在结论。
- **证据或推理**：E1+E2。直接读取 `os/arch/src/arch/clock.rs` L36-43 与文档 L755-778 逐行对比，尾段四行逐字不同；`os/kernel/src/clock.rs:710` 的 `pub use` 与 L700-709 注释证实合并已落地。
- **最小修复**：两步。(a) 文档引文块尾段改回源码逐字（"has an independent copy … modify here first, then sync"），并把"已合并"的陈述改为指向 kernel 侧注释（`os/kernel/src/clock.rs` L700-710）而非 arch 侧注释；(b) 按 fix-guard 把 arch 侧过时注释同步成合并后口径（代码侧一条注释改动，非文档可独自完成）。
- **置信度**：高。
- **是否需源码核实**：否（本轮已逐字核实）。
- **若我错了**：若该引文块被有意标注为"改后示意"而非"代码引用"，则失真降级为标注问题——但块头明写"代码引用（文档注释与常量定义行）"，故按现状判 P1。

## FINDING RD05R2-FACT-02（P2）

- **位置**：文档 L780（§3.2 正文）。
- **严重度**：P2。
- **问题**："kernel 层的副本在……注释里额外写了一句「需与 C 完全一致时可在 boot 阶段用 `with_hz(60)` 指定」"——这句在 `os/kernel/src/clock.rs` 与 `os/arch/src/arch/clock.rs` 的任何注释里都不存在（两文件全文 grep `with_hz(60)`/`需与 C` 均无命中；kernel 侧 DEFAULT_HZ 注释 L700-709 全文已读，无此句）。
- **影响**：把不存在的注释文本当作存在的事实陈述（与 FACT-01 同根：按理想状态描写注释），读者据此去找"那句话"会落空。
- **证据或推理**：E1+E2。`grep -rn 'with_hz(60)' os/kernel/src/clock.rs os/arch/src/arch/clock.rs` 零命中；`os/kernel/src/clock.rs:700-709` 注释全文核对无该句。
- **最小修复**：删掉这句，或改为"范围守卫保留在 `with_hz`（`os/kernel/src/clock.rs`），需要与 C 逐值对齐时可显式调用"这样的非引语陈述。
- **置信度**：高。
- **是否需源码核实**：否。
- **若我错了**：若该句位于本清单之外的其他注释（如 01 篇），则降级为锚点错误——本轮在两处 DEFAULT_HZ 相关注释内均未找到。

## FINDING RD05R2-FACT-03（P2）

- **位置**：四处引文块（同一根因：引文省略/拼接/重排/转述未按本篇自己的约定标注）：
  1. 文档 L140-181（§2.1 `cstart` 引文）：块头声明"块内其余行逐字保留，原注释保留"、只登记两处省略号，但源码 `minix3/minix/kernel/main.c` 中 `cstart` 函数体 `{` 之后的 4 行函数级注释（`/* Perform system initializations prior to calling main()... */`）被静默省略，无省略号登记。
  2. 文档 L1094-1115（§4.1 x86_64 Step 2 条件子步引文）：语句次序重排。源码 `os/kernel/src/lib.rs` 的顺序是 `assert!(lapic_region…)` → 两条 `assert_eq!`（L2447-2462）→ `device_va_for`/`set_lapic_va` → `device_va_for`/`set_ioapic_va` → `set_x86_window_live`；引文把两段 VA 注入放在前面，把"此处原文还有两条 `assert_eq!`（lib.rs:2447-2462）"夹在 ioapic_va 计算与 `set_ioapic_va` 之间——行号本身正确，但在引文中的位置使读者误以为断言发生在注入之后。正文 §4.1"2a"条目（L1141"断言通过后把 LAPIC 与 IOAPIC 的高半 VA 注入驱动"）的次序是对的，失真只在引文块。
  3. 文档 L1862-1872（§4.9 `TIMER_IRQ` 转发引文）：四条 cfg 转发 + mock 兜底显示为连续行；实际 `os/plat/src/lib.rs` 中 x86_64 转发在 L90-91，aarch64/riscv64/mock 在 L109-114，中间隔着约 18 行其他 re-export（`pic_init`/`pic_eoi`/`lapic_eoi`/`set_lapic_va`/`set_ioapic_va`），无省略号标注。块头"节选"承认了选摘，但本篇其他引文（如 L1049、L1689）对同类跳段均显式写省略，此处不一致。
  4. 文档 L1749-1774（§4.7 `pic_init` 引文）：块头"注释按原文"，但注释实为中文转述（源注释英文，如 `// IMR: mask everything (IRQ0/IRQ2 are opened on demand by unmask_irq).` 被写成 `// IMR：全 mask（IRQ0/IRQ2 由 unmask_irq 按需打开）。`）。同节上一块（L1736）对同类情形明确声明"注释为中文转述、非源码逐字"，此块的声明与之矛盾。
- **严重度**：P2。
- **问题**（合并根因）：引文块的省略/拼接/重排/转述没有完整标注，与"引文块逐字"的硬约束冲突；机械门逐行对账查不出这类失真（被省略的行不在引文里，故不失败）。
- **影响**：读者按引文重建源码时会得到错误的相邻关系（第 2、3 项）或高估引文的完整性（第 1、4 项）；对一篇以"引文即证据"为纪律的文档，这是纪律执行缺口而非事实错误——四处引文中的每一行本身都逐字对得上源码。
- **证据或推理**：E1。`sed -n '403,476p' minix3/minix/kernel/main.c`、`sed -n '2436,2516p' os/kernel/src/lib.rs`（行号 2447-2462 复核）、`sed -n '85,120p' os/plat/src/lib.rs`、`sed -n '79,103p' os/plat/src/x86_64/interrupt.rs` 四处直接读取对账。
- **最小修复**：逐处补登记——(1) cstart 块头加"另省略 `{` 后的函数级注释 X 行"或补入第三处省略号；(2) §4.1 引文按源码次序重排（assert_eq! 前置），省略号说明同步改写；(3) §4.9 引文在 x86_64 转发与 aarch64 转发之间加 `...` 并注明"中间为 x86_64 专属 re-export"；(4) pic_init 块头改为与 L1736 相同的"注释为中文转述"声明（或换回英文原注释）。
- **置信度**：高（四处均为直接读取对账）。
- **是否需源码核实**：否。
- **若我错了**：若机械门的 skipped=14 已把这四处判为"结构性跳过"并另行登记，则本条降为复述；但文档文本本身仍未登记这些省略，读者视角的问题不随门的分类消失。

## FINDING RD05R2-FACT-04（P2）

- **位置**：文档 L1657（§4.6 FPEN 段）；源码 `os/arch/src/arm64/arch_init.rs` L41-49。
- **严重度**：P2。
- **问题**：文档写"kmain 之后第一次 4 字节 memset 就会触发未定义指令类异常"。源注释原文是 "takes an undef on the first 4-byte memset after kmain (observed on run smpd5: elr=memset+0xe0, EC=0x7)"。按 ARMv8 ESR 编码，EC=0x7（0b000111）是 Trapped SIMD/FP 异常类，不是 Undefined instruction（EC=0b000000）；源注释的口语 "undef" 被文档强化成了"未定义指令类"，与它自己引用的 EC 读数矛盾。
- **影响**：低——推理链（FPEN=00 → FP 指令被陷阱 → memset 残差触发）不受影响，但异常分类名与给出的 EC 证据不自洽；按本项目对源注释与规范冲突"登记差异"的惯例（本篇对 GICD_CTLR ARE 位就如此处理），此处应同样处理而不是沿用口语。
- **证据或推理**：E3。源注释 EC=0x7 直接可读（E1）；EC 分类对照 ARM ARM（E3）。源注释自身说 "undef" 属口语化，文档照抄并加重。
- **最小修复**：改为"触发 FP/SIMD 陷阱类异常（源注释记 EC=0x7；ARMv8 里该 EC 是 Trapped SIMD/FP，注释原文的 undef 是口语说法）"，或按篇内 GICD_CTLR 先例登记"注释措辞与规范分类的差异"。
- **置信度**：中高（EC 编码为公开规范；"undef"一词的解释有一定主观空间）。
- **是否需源码核实**：否（EC 读数已直接核实）。
- **若我错了**：若按某种口径 EC=0x7 可归入"未定义类"，则只剩措辞问题，降为 SUGGESTION 级。

## FINDING RD05R2-FACT-05（P2）

- **位置**：文档 L2030（§5.2）、L2076（§5.4）："三架构载体已实跑：x86_64、aarch64、riscv64 均输出 PASS 横幅"。
- **严重度**：P2。
- **问题**：这是全篇端到端证明（M13、§5.4 收束段）的关键事实断言，但证据（三份串口日志）不在文档内，也未给出日志路径；本清单允许范围内无法核验。载体源码侧的判定文本（PASS 横幅、riscv64 的 `matched > 0 && now >= target`、`target + 10` 窗口、`main.rs:357-363` 的 FAIL 分支、run_qemu.sh 的 `no PASS marker found`）已逐字核实为真，但"已实跑且均 PASS"本身在此无证据。
- **影响**：若该断言的日志不可追溯，端到端链（M13"已覆盖"）的证据强度回落到"源码形态正确 + 判定逻辑正确"，尚差一环；排查时也无法复核是哪次构建的读数。
- **证据或推理**：证据不足（按约定如实标注）。允许范围内可核实的是载体源码与脚本逻辑（E1），不可核实的是运行结果本身。
- **最小修复**：给三份串口日志各挂一个绝对路径锚点（会话产物按约定在 `tmp/` 的功能子目录），或把该句改为"判定逻辑见载体源码与 run_qemu.sh；实跑读数以 `<日志路径>` 为准"。
- **置信度**：高（"不可核验"这一判断本身可靠；不预设实跑为假）。
- **是否需源码核实**：需仓库外证据（日志），非源码核实。
- **若我错了**：若日志路径存在于 `os/qemu-tests/README.md` 或相关文档且我漏读，则降为"补锚点"级建议；本轮已查 README 目录清单（related-code.txt 含 run_all.sh/README.md 引用，未列日志）。

---

## SUGGESTION RD05R2-SUG-01

- **位置**：文档 L537（§2.6 限定段）。
- **建议**：'随后停 PIT、摘临时钩子、关中断（`apic.c:491`、`:499-512`）'中，"停 PIT"实际发生在校准钩子内 `stop_8253A_timer()`（`apic.c:427`，`probe_ticks == PROBE_TICKS` 分支），不在所引的 499-512 区间内（该区间覆盖的是开中断、采样循环、关中断、摘钩子）。补一个 `apic.c:427` 锚点，使三个动作各有出处。
- **理由**：E1（`sed -n '417,431p' apic.c` 直接读数）；事实断言本身为真，只是行号引用未全覆盖。
- **收益**：低成本消除一个"引用区间覆盖不全"的口径缺口。

## SUGGESTION RD05R2-SUG-02

- **位置**：文档 L1049（§4.1 引文头"开头两条 `use` 的顺序按源码原样"）与 L1689（§4.7 引文头"两条 `use` 的顺序按源码原样"）。
- **建议**：两处引文实际分别含 4 条与 3 条 `use` 行（L1050-1053；L1692-1694），头注的"两条"与所示条数不符。改为"各条 `use` 的顺序按源码原样"或写明实际条数。
- **理由**：E1（引文与源码逐行对账）；纯标注计数问题，不影响事实。
- **收益**：引文头注与引文自身一致，避免读者按注去数行。

## SUGGESTION RD05R2-SUG-03

- **位置**：文档 L794-801（§3.2 `with_hz` 引文）。
- **建议**：该块把源码三行的 `let hz = if !(2..=50000).contains(&hz) { … } else { … };` 折为一行，块头"代码引用（函数体四行）"未注明折行；§4.7 对同类情形（L1689"`init_timer` 的两行实参按源码折为三行"）有现成的标注惯例，建议对齐（补一句"折行未改语义"或展开回源码换行）。
- **理由**：E1（`os/kernel/src/clock.rs:1144-1152` 直接读数）；语义逐字一致，仅版面折行未登记。
- **收益**：引文保真惯例全篇统一，机械门的 warned 口径也更干净。

---

- 本次扫描：5 条 FINDING，3 条 SUGGESTION。
