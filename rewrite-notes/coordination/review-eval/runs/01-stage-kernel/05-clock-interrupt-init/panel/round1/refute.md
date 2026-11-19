# 反证报告 · 05-clock-interrupt-init（round1 / refuter）

反证对象：`rewrite-notes/01-stage-kernel/05-clock-interrupt-init.md`
证据范围：文档全文 + `minix3/minix/` 与 `os/` 针对性 grep（相关清单 `related-code.txt` 的 33 个文件抽查了其中核心 12 个）。C 侧逐条核过锚点；Rust 侧核过 boot_init_timer / init_clock_and_interrupts / 三架构 clock・timer_irq_gate・interrupt・arch_init / irq_manager；§5.1 的 15 个计数与 §6.2 的命令全部原样重放。

---

## CLAIM RD05-CLAIM-01

**主张**（文档 §1.2、§2.3）：8259A `intr_init` 是一串定长端口写——执行路径十次 `outb`、源码 12 行；`auto_eoi=0` 时主片写 `ICW4_AT_MASTER=0x05`、从片写 `ICW4_AT_SLAVE=0x01`；结束状态是主片 `0xFB`（仅级联线 IRQ2 放行）、从片 `0xFF`（全屏蔽）；端口 0x20/0x21/0xA0/0xA1，向量基 `IRQ0_VECTOR=0x50`、`IRQ8_VECTOR=0x70`，`CASCADE_IRQ=2`。

**反证尝试**：
- 数 outb 行数：实跑 §6.2 命令 2（`awk '/^int intr_init/,/^}/' … | grep -c 'outb('`）→ 输出 **12**，与文档口径一致；`auto_eoi=0` 走两处 else 臂，执行 10 次，无出入。
- 找相反常量：`i8259.c:16-17` 两行尾注逐字相同（`not SFNM, not buffered, normal EOI, 8086`）、值 0x01/0x05 只差 bit 2——文档第三条结论如实登记"注释未解释该位"，未虚构裁决。
- 找反例端口/向量：`interrupt.h:9-12`（0x20/0x21/0xA0/0xA1）、`:17-18`（0x50/0x70）、`:39-41`（CLOCK_IRQ=0、CASCADE_IRQ=2）全部吻合；`cstart` 实参 `intr_init(0)` 在 `main.c:472` 逐字存在。
- 攻击"第 5 次写起 0x21 才变成屏蔽寄存器"：该口径来自 8259A 数据手册语义，源码内无法直接证伪，但与 ICW1=0x11（need ICW4）→ ICW4 之后再写数据口的解释序列自洽，无内部矛盾。

**结果**：反证失败（主张成立）。

**证据**：`minix3/minix/kernel/arch/i386/i8259.c:13-49`；`minix3/minix/include/arch/i386/include/interrupt.h:9-18,39-41`；`minix3/minix/kernel/main.c:472`；§6.2 命令 2 重放输出 12、命令 3 重放输出 9。

**若我错了**：只有一种可能——8259A 硬件语义（ICW 计数区分、OCW1 切换）与文档的"计数区分"叙述不符，但那在本仓之外，源码常量互证无矛盾。

---

## CLAIM RD05-CLAIM-02

**主张**（文档 §1.1、§2.1、§2.2、§2.7）：cstart 三行次序中 `init_clock` 与 `intr_init` 无共享状态可交换；`arch_init` 不能前置——它经 APIC 分支读 `system_hz`（唯一赋值点是 `init_clock`），且 `system_hz == 0` 会"同时污染三处"（`load_update` 槽位算式、`init_local_timer` legacy 分支除法、校准链两式与 `estimate_cpu_freq` 采样循环）。

**反证尝试**：
- 找其他 `kclockinfo.hz` 赋值点：全仓 grep 只有 `clock.c:58-60` 三行——"唯一赋值点"成立。
- 找次序反例：`main.c:411/418/472/474`（prot_init→init_clock→intr_init(0)→arch_init）与文档一致；`init_clock`/`intr_init` 函数体互不读对方产物，"可交换"成立。
- 逐条验证污染清单里的除法：`clock.c:273`（load_update 除 `system_hz`）、`arch_clock.c:149`（legacy 分支 `cpu_get_freq(0)/system_hz`）、`apic.c:169`（`PROBE_TICKS = system_hz/10`）、`apic.c:517/520`（两式）、`apic.c:491`（`init_8253A_timer(system_hz)`）、`arch_clock.c:112-125`（estimate 循环与算式）、`lapic_enable→apic_calibrate_clocks` 调用点在 `apic.c:739`——全部存在。
- **构造反例（反证得手一半）**：把"污染三处"放回 §1.1 的具体场景"仅把 `arch_init` 提到 `init_clock` 之前"检验执行路径：`load_update` 只在运行期由 tick 记账调用（远在 `init_clock` 恢复 hz 之后）；`init_local_timer` 的 legacy 分支与 `estimate_cpu_freq` 只在启动末端 `boot_cpu_init_timer` 里执行（同样在 `init_clock` 之后）。纯换序下这三处读到的已是恢复后的 hz，**不被该换序污染**。反过来，真正在 `arch_init` 内立即触发的污染——`init_8253A_timer(0)` 展开为 `TIMER_COUNT(freq) = TIMER_FREQ/(freq)`（`arch_clock.c:33,55-56`），freq=0 是一次整数除零（x86 上 #DE）——文档清单反而未列。即：清单是"hz 恒为 0"世界（§2.2 删范围守卫场景，在那里逐条成立）的后果，被挪用成了换序场景的后果。次序结论本身（arch_init 自己在读 `system_hz`，不能前置）不受影响。

**结果**：非阻断（P2）。核心主张（次序约束、可交换对、唯一赋值点、每个除法锚点）全部成立；仅 §1.1"污染三处"的枚举把两个场景的执行路径混在一起——换序场景下三处中有两处实际读不到 0，且漏了立即触发的 `TIMER_FREQ/0`。不翻转任何结论，按观察登记。

**证据**：`minix3/minix/kernel/main.c:411-474`；`minix3/minix/kernel/clock.c:273,58-60`；`minix3/minix/kernel/arch/i386/apic.c:169,491,517,520,739`；`minix3/minix/kernel/arch/i386/arch_clock.c:33,48-58,112-125,145-149`；`minix3/minix/kernel/glo.h:38`。

**若我错了**：若能给出"纯换序后 load_update/init_local_timer 仍在 `init_clock` 之前执行"的调用路径，则升级为阻断；我已核过两处的全部调用点（tick 记账与 `boot_cpu_init_timer`），不存在这样的路径。

---

## CLAIM RD05-CLAIM-03

**主张**（文档 §2.5、§2.6）：持久 tick 源的编程不在 `cstart` 里——C 收在 `boot_cpu_init_timer`（`bsp_finish_booting` 末端调用，前置 `cycles_accounting_init`）；三段结构（编程/注册/放行），x86 PIC 支路的首次放行藏在 `put_irq_handler` 末尾（`actids==0` 时 `hw_intr_unmask`）；`irq_handle` 空链分支打印伪中断并**保持屏蔽**；APIC 形态在 cstart 内有一段用临时 PIT 的校准窗口；ARM 侧先挂链后 `bsp_irq_unmask`；APIC 支路的 LVT 门在编程段就由 `lapic_set_timer_one_shot` 打开。

**反证尝试**：
- 找 `boot_cpu_init_timer` 的其他调用点：全仓只有 `main.c:73`（`bsp_finish_booting` 内，`cycles_accounting_init()` 在 71 行）——"不在 cstart"成立。
- 找放行时序反例：`interrupt.c:65-68` 确为挂链后 `hw_intr_unmask(hook->irq)`；`omap_timer.c` 的 `bsp_register_timer_handler` 先 `put_irq_handler` 再 `bsp_irq_unmask`，注释逐字一致。
- 攻击"空链分支保持屏蔽"：`irq_handle` 入口先 `hw_intr_mask(irq)`，空链分支计数、打印 `…keeping masked` 后 `return`，无 unmask——主张成立。
- 攻击校准窗口归属：`apic.c:489-521` 完整呈现"挂临时钩子→`init_8253A_timer(system_hz)`→开中断→等 `PROBE_TICKS`→关中断→`rm_irq_handler`"，确为 cstart 内（经 `lapic_enable`）的临时窗口；`apic.c:541-542` + `apic.h:90`（`APIC_TIMER_INT_VECTOR = 0xf0`，无 mask 位）证实 `lapic_set_timer_one_shot` 写 LVTTR 即开门。
- **发现一处锚点错位（P2 观察，不构成反例）**：文档 §2.6 限定段引用"（`apic.c:139`、`:541-542`）"——`apic.c:139` 实为 `#define is_boot_apic(apicid)`，与主张无关；真正的调用点在 `arch_clock.c:139`（`lapic_set_timer_one_shot(1000000 / system_hz);`）。实质主张仍成立，仅文件前缀写错。

**结果**：反证失败（主张成立）。附一条 P2 引用锚点勘误（`apic.c:139` 应为 `arch_clock.c:139`）。

**证据**：`minix3/minix/kernel/clock.c:294-304`；`minix3/minix/kernel/main.c:71-76`；`minix3/minix/kernel/interrupt.c:23-37,65-68,143-166`；`minix3/minix/kernel/arch/i386/apic.c:489-521,541-542,739`、`apic.h:90`；`minix3/minix/kernel/arch/i386/arch_clock.c:136-140`；`minix3/minix/kernel/arch/earm/bsp/ti/omap_timer.c`（put→unmask 两行）。

**若我错了**：若 `apic.c:139` 在其他 git 历史版本确是 LVT 相关定义，则该勘误降级为行号漂移；当前工作树内容如上。

---

## CLAIM RD05-CLAIM-04

**主张**（文档 §3.4、§4.1、§4.7、§4.8、§4.10）：Rust x86-64 端到端——`boot_init_timer` 四件事（`init_timer(DEFAULT_HZ)`→`register_hook(TIMER_IRQ=0, clock_irq_handler, …, REENABLE)`→`enable_timer_irq`→x86_64 尾调 `pic_init`）；first-handler 规则在 `register_hook_at_slot` 里 `actids==0` 时 `controller.unmask`，经 `isa_irq_to_pin`（0→2）落到 IOAPIC 引脚 2；`pic_init` 重映射 0x50/0x70、IMR 全 0xFF 后 `unmask_irq(0)+(2)`；BSP 启动路径无 `sti`，IF 在 `trap_return` 的恢复路径才置位（`IF_MASK = 1<<9`）；`init_clock_and_interrupts` 的 Step 1→4 次序与两处全局写入时机。

**反证尝试**：
- 找 `init_timer` 的其他调用者：`rg 'clock_arch.init_timer(' `→ 仅 `lib.rs:3755` 一处，实参 `crate::clock::DEFAULT_HZ`——"唯一调用点 + 编译期常量实参"成立。
- 攻击"unmask 落在引脚 2"：`ioapic_set_mask` 首行 `let pin = Self::isa_irq_to_pin(irq)`，映射 0→2——若不过映射则会解错引脚，实码过了。
- 攻击 `pic_init` 位置：`#[cfg(target_arch = "x86_64")] minix_plat::pic_init();` 在函数体最后一句（`lib.rs:3802-3803`），内容与文档引用逐字一致（ICW4_8086 两片同写、IMR 0xFF、`unmask_irq(0)`+`unmask_irq(2)`）。
- 找启动期 `sti`：`os/` 非 test 代码里仅 `arch/src/x86_64/smp.rs:275`（AP bring-up 路径）一处；BSP 主启动路径无 `sti`，`trap_return.rs:42-43` `IF_MASK: u64 = 1 << 9` 且注释明写 OR 进保存的 PSW——"IF 在切用户态恢复路径才置位"成立。
- 攻击 Step 次序：`init_clock_and_interrupts` 内 Step1 `ClockState::new` → Step2 `intr.init()`（后入 `IRQ_MANAGER`）→ Step3 `arch_init.init()` → 末尾 `CLOCK_STATE.get() = Some(clock)`；`bsp_finish_booting` 在 `lib.rs:3890` 调 `boot_init_timer()`。

**结果**：反证失败（主张成立）。

**证据**：`os/kernel/src/lib.rs:3755-3803,3890,2440-2463`；`os/kernel/src/irq_manager.rs:576-578`；`os/plat/src/x86_64/interrupt.rs:45,376-383,ioapic_set_mask,pic_init`；`os/arch/src/x86_64/trap_return.rs:42-43`；`os/arch/src/x86_64/smp.rs:275`（唯一非测试 `sti`）。

**若我错了**：若某条 BSP 启动路径存在被我 grep 漏掉的 `sti`（例如以宏/拼接字符串形式），则"此刻 CPU 不取中断"的表述要加限定；但 `grep '"sti"'` 全仓只有上述一处。

---

## CLAIM RD05-CLAIM-05

**主张**（文档 §3.7、§4.3、§4.4、§4.5、§4.9）：aarch64——`TIMER_IRQ=30`（PPI INTID 30，走 Redistributor）；`init_timer` 写 `CNTP_CVAL_EL0=当前计数+间隔`、`CNTP_CTL_EL0=0x2`（门保持关）；`enable_timer_irq` 写 `CNTP_CTL_EL0=1 + isb`；`GICR_SGI_FRAME=0x1_0000`、`GICR_ICENABLER0/GICR_IGROUPR0` 写 `0xFFFFFFFF`；claim 读 `ICC_IAR1_EL1` 低 24 位、1023 映射 `None`、complete 原样写回 `ICC_EOIR1_EL1`。riscv64——`TIMER_IRQ=0` 且 `mask/unmask` 对 0 提前返回（PLIC 无此线）；claim/complete 共用同一偏移；编程走 SBI TIME ecall、读计数走 `time` CSR；`sie.STIE` 由 `TimerIrqGate` 的 `csrs sie, 0x20` 打开。

**反证尝试**：
- 找 arm64 门未关的反例：`arm64/clock.rs:125`（init_timer 写 `0x2`）、`:149`（stop 写 0x2）、`:159`（restart 写 1——是 stop 的逆操作，不在启动三段内）；`timer_irq_gate.rs` 的 enable 写 `1 + isb`——三段语义与文档表逐格吻合，无一处启动路径提前开门。
- 攻击 GICR 帧偏移：`GICR_SGI_FRAME = 0x1_0000`（`arm64/interrupt.rs:65`），`IGROUPR0/ISENABLER0/ICENABLER0` 三个常量都带该偏移；`init_redistributor` 确写 `ICENABLER0=0xFFFFFFFF` 与 `IGROUPR0=0xFFFFFFFF`（`:207,216`），注释含首次载体"PPI 沉默"现象——与文档叙述一致。
- 攻击 claim/complete 配对：`claim` 读 `icc_iar1_el1`、`& 0x00FF_FFFF`、`(intid != 1023).then_some(intid)`；`complete` 对 `None` 直接 return、对 `Some` 写回同一 INTID。x86 侧 `claim` 返回 `None`、`complete` 忽略参数恒写 `lapic_eoi()`——三架构对照表成立。
- 找 riscv64 假线穿帮：`riscv64/interrupt.rs:40`（TIMER_IRQ=0）、`:136-165`（mask/unmask 对 0 提前 return）、`:24,28`（`PLIC_COMPLETE = PLIC_CLAIM` 同偏移）、`claim_context` 读 claim 偏移、0 映射 None；`riscv64/timer_irq_gate.rs` 的 enable/disable 分别 `csrs/csrc sie, 0x20`——全部吻合。
- 攻击"读 CSR 写 SBI"的通道分工：`riscv64/clock.rs:95`（`read_time_csr`）、`:37,143`（`sbi_set_timer(now + interval)`）、`:153` 注释实测 `scause=5, stval=0x200bff8`——与文档 §3.8/§4.3 的理由一致。

**结果**：反证失败（主张成立）。

**证据**：`os/plat/src/arm64/interrupt.rs:32,65-74,149-150,169,207,216,409-430`；`os/arch/src/arm64/clock.rs:125,149,159`；`os/arch/src/arm64/timer_irq_gate.rs`（enable=1+isb）；`os/plat/src/riscv64/interrupt.rs:24,28,40,136-165,264,278`；`os/arch/src/riscv64/timer_irq_gate.rs`；`os/arch/src/riscv64/clock.rs:37-55,95,139-143,153-158`。

**若我错了**：唯一可攻击点是 arm64 `restart_local_timer` 也写 `CNTP_CTL_EL0=1`（`clock.rs:159`）——若启动路径在注册前调过它则构成"门先开"反例；已核其调用者（wake/idle 路径），启动三段内不可达。

---

## CLAIM RD05-CLAIM-06

**主张**（文档 §5.1、§5.2、§6.2）：测试覆盖口径——15 个文件的 `#[test]` 计数（kernel clock 47 / irq_manager 10 / arch clock 3 / timer_irq_gate 4 / plat interrupt 0 / x86_64 5 / arm64 3 / riscv64 4 / x86_64 arch_init 3 / arm64·riscv64 arch_init 0 / 四个 integration test 3/4/3/5）；kernel clock.rs 在 x86_64 宿主"46 跑 + 1 忽略"；§6.2 各命令的预期输出（命令 2 = 12、命令 3 = 9 行、命令 5 = 5 行（169/491/517/520/578）、命令 6 = 两行（67/164）、`self.controller.unmask` 五处）；三架构载体登记于 `run_all.sh`，riscv64 载体自带 FAIL 横幅打印。

**反证尝试**：
- 原样重放全部 15 个 `grep -c '#\[test\]'`：**15/15 与文档逐格相同**。
- 攻击"1 忽略"：`grep -c '#\[ignore'` 得 2——追查后一条（`:1692`）是 doc-comment 行（`/// #[ignore]: …`），真实属性只有 `:1697` 一条，47 = 46 跑 + 1 忽略自洽。（"46 跑"的实跑数未用 `cargo test --list` 复算——静态口径全部吻合，见"证据"。）
- 重放 §6.2 命令 2/3/5/6：输出 12、9、5 行（行号 169/491/517/520/578 逐行一致）、2 行（67/164）——四个预期全部命中；`grep -c 'self.controller.unmask' irq_manager.rs` = 5，与命令 9 预期"五处"一致。
- 攻击载体登记：`run_all.sh:96/102/108` 三条架构清单均含 test-timer-irq；riscv64 载体 `main.rs:363-365` 确有载体自打 `### TEST_RESULT: FAIL test-timer-irq-riscv64 ###`；x86_64 载体的 `fail()`（`:250`）只定义无调用点——文档"从未被调用"的表述也吻合。

**结果**：反证失败（主张成立）。唯一未复算项：第二口径"x86_64 宿主实跑数"需 `cargo test --list`（本次未执行），其静态前提（47 条源码测试、恰 1 条真实 `#[ignore]`、差值来自 cfg 门控）已逐项核过。

**证据**：15 个文件的 `grep -c '#\[test\]'` 输出（与 §5.1 表逐格相同）；`os/kernel/src/clock.rs:1692,1697`；`os/kernel/src/irq_manager.rs`（unmask 计数 5）；`minix3/minix/kernel/arch/i386/i8259.c,apic.c,interrupt.c`（命令 2/3/5/6 重放）；`os/qemu-tests/run_all.sh:96-108`；`os/qemu-tests/test-kernels/kernel/bootstrap/test-timer-irq-riscv64/src/main.rs:363-365`；`test-timer-irq/src/main.rs:250`。

**若我错了**：若宿主实跑数与"46 跑 + 1 忽略"不符，只会差在 cfg 门控的判定，第一口径 15/15 与四个命令重放不受影响。

---

## 跨节一致性核对

- `init_clock`↔`intr_init` 可交换：§1.1、§2.1、§2.7 三处复述一致，且都与源码（两函数互不读写对方产物）相符。
- 门的三段归属（编程关门 `0x2` / 注册解控制器门 / `enable_timer_irq` 开本地门）：§3.7 推演、§4.3、§4.7 三处复述一致，与 `arm64/clock.rs:125`、`irq_manager.rs:576-578`、`arm64/timer_irq_gate.rs` 一一对应，无互相冲突。
- 失败路径未发现被省略或写成"不会发生"的反例：hz∈[2,9] 使 `PROBE_TICKS=0` 的退化（§2.6 明写）、OVMF LINT0 遗留向量（§4.4 明写）、8259A 未初始化缺口由 `pic_init` 补（§4.7 明写）、riscv64 MMIO 读 fault 实测读数（§3.8/§4.3 明写）——文档反而主动披露了这些失败面。唯一措辞问题即 CLAIM-02 登记的 §1.1"污染三处"场景混用（属复述归属失准，非"写成不会发生"）。

## 结论

文档的全部可证伪核心主张在针对性反证下均未被打倒；发现两处非阻断问题：§1.1 污染清单把"hz 恒为 0"世界的后果挪用到"纯换序"场景（且漏列 `init_8253A_timer(0)` 的立即除零），§2.6 一处引用锚点 `apic.c:139` 应为 `arch_clock.c:139`。两者均不翻转任何次序或机制结论。

- 本次反证：6 条 CLAIM（阻断 0 / 非阻断 1 / 反证失败 5）。

## CLAIM 11

- 结果：反证失败（对应上文 RD05-CLAIM-01）

## CLAIM 12

- 结果：非阻断（对应上文 RD05-CLAIM-02）

## CLAIM 13

- 结果：反证失败（对应上文 RD05-CLAIM-03）

## CLAIM 14

- 结果：反证失败（对应上文 RD05-CLAIM-04）

## CLAIM 15

- 结果：反证失败（对应上文 RD05-CLAIM-05）

## CLAIM 16

- 结果：反证失败（对应上文 RD05-CLAIM-06）
