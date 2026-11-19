# D04 R1 reader-abc 修复日志

- 目标文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 基线状态：报告基线 2688 行 / sha `a8cb91b1`；开工时文档已被前序提交改为 2682 行（`wip(04): R1 P0/P1` 已部分处理 C-01、C-02、PlatformDescSource 槽位与 54 处计数），因此逐条按当前正文核对，不按报告行号。
- 范围：24 条 FINDING（A 7 / B 5 / C 12）；SUGGESTION（A-08/A-09/B-06..B-09/C-13..C-15）未处理。
- 结论：24 条全部落地；其中 C-05、C-06、B-01 为部分落地（见各行备注），无 SKIP-NEED-SOURCE。

| ID | 改了哪句 | 旧串 → 新串摘要 |
|---|---|---|
| RD4-READER-A-01 | §1.1 表 1 第 2 行 | 「…TIMER_FREQ（8254 的输入时钟，常量）；CPU 主频由 cpu_identify 写进 cpu_info[cpu].freq」→「…；「计数器在哪」见 §3.8 的 PIT 端口 `0x40`/`0x43`；CPU 主频（…）另见 §2.9，不属定时器问题」 |
| RD4-READER-A-02 | §1.2 第 2 段 | 「ARM 这一侧连解析器都没有」→「ARM 这一侧（`earm`，32 位 ARM）连解析器都没有」 |
| RD4-READER-A-03 | §1.4 步骤 ② 例外注记 | 9 行标识符清单（bootface / DtbMemoryMap / dtb_copy::preflight / authorize_and_copy / cross_validate_config_dtb / 5 条 panic）→ 2 行摘要「引导阶段另有两处例外…详见 §4.2 与 01 篇」；原细节整段移入 §4.2 末尾新增一段（事实不丢） |
| RD4-READER-A-04 | §2.1 代码节选与导语 | 「apic_single_cpu_init 的第一步就是常量赋值」→「第一段是 APIC 存在性检查，随后才是常量赋值」；节选标题「函数开头的三段」→「函数开头到 acpi_init / detect_ioapics 两行」；节选补 `acpi_init();` 与 `if (!detect_ioapics()) { lapic_disable(); lapic_addr = 0x0; return 0; }`。源码核实：`minix3/minix/kernel/arch/i386/apic.c:1125-1150` |
| RD4-READER-A-05 | §2.3 `acpi_read_sdt_at` 节选 | 省略号 `...` → `if (read_func(addr, tb, sizeof(struct acpi_sdt_header))) { printf("ERROR acpi cannot read %s header\n", name); return -1; }`；标题「节选（只保留签名/容量/校验和三道闸）」→「逐字引用」。源码核实：`minix3/minix/kernel/arch/i386/acpi.c:94-97` |
| RD4-READER-A-06 | §2.7 rg 输出之后 | 新增「启动参数 → config_no_apic → config_no_smp → 实际分支」三行表（SMP 构建）＋非 SMP 一句；「默认 `no_apic` 缺省为 1，`kmain` 走…」→「`no_apic` 未给出 ⇒ `config_no_apic = 1` ⇒ `arch_init` 跳过 `apic_single_cpu_init`；SMP 构建下…」。源码核实：`minix3/minix/kernel/main.c:443-468`、`304-307` |
| RD4-READER-A-07 | §3.2 `PlatformSource` 段；§7 Rust 落点 | 新增一句「两个名字只差一个 `Desc`，但不是同一层：`PlatformDescSource` 是交接项…；`PlatformSource` 是解析结果的格式标签」；§7 `minix-boot/src/platform.rs` 条目补 `PlatformSource` |
| RD4-READER-B-01 | 头部教学目标 ④；§3 导语 | 「④Rust 八项设计决策（§3）」→「④Rust 八项核心设计决策与七项实现层决策索引（§3）」；「本章…给出八项设计决策」→「…八项核心设计决策（§3.1–§3.8）与七项实现层决策索引（§3.9）」。备注：报告建议的「决策→§2.10 三问」映射表未加（属新增编排，超出最小改动；口径统一已消解三处矛盾） |
| RD4-READER-B-02 | §2.2 签名闸反事实；§2.3「第二层」 | §2.2 删「整数提升 + i=35 越界 + read_func 不退出」整段，改为「把任意地址带进 `sdt_count` 的算式；整数提升与越界的完整推演见 §2.3」；该段原文移入 §2.3 `sdt_count = (s - 36)/4` 之后 |
| RD4-READER-B-03 | §2.1 论证段 | 删「把这两步对调会坏在哪，可以逐跳数出来」设问，结论前置为「次序不影响正确性，因为 `apic_single_cpu_init` 自己会调一次 `acpi_init()`；真正的依赖是 `machine.acpi_rsdp` 必须先被填…」；删段尾重复的「真正的依赖是…谁去填它」 |
| RD4-READER-B-04 | §6.2 #4 命令与输出 | `head -3` → `head -5`；删掉 `head -3` 不可能产生的 `handoff_v1_write.rs:1`、`kerninfo.rs:1` 两行，换成实测第 4/5 行 `os/kernel/src/vm_handoff.rs:3`、`os/qemu-tests/.../test-timer-irq-riscv64/src/main.rs:1`（已重跑命令核对） |
| RD4-READER-B-05 | §5.3 分布表最后一行 | 「全量即 §6.2 #4 输出里的测试内核条目」→「清单：`rg -l 'platform_sources:\s*&\[\]' os/qemu-tests/test-kernels`，实测 21 个文件」；实测 `rg -l` = 21 |
| RD4-READER-C-01 | §2.4 反事实结尾 | 补「`flags & 1` 的作用不是"少算"，而是每次调用只返回一条启用记录、把后续记录留给下一次调用」；「症状是"少了 CPU"或"一颗都没有"」→「症状恒为"一颗都没有"，不会是"少了 CPU"」（前半段前序提交已改，本次补完） |
| RD4-READER-C-02 | §5.3 判据段 | 「上述 52 处」→「上述 54 处」；命令输出与分布表已为 54，逐行求和 10+9+6+2+1+1+3+1+21 = 54，`rg -o … \| wc -l` 实测 54 |
| RD4-READER-C-03 | §4.1 末句；§6.1 行 2 | 「区别只是原本要验证的时钟/中断消费者更晚才被到达」→「原本要验证的时钟/中断消费者永远到不了，所以症状只会停在 `init_protection`」；§6.1 预期输出「栈里先出现 init_protection，再出现 init_clock_and_interrupts 或 boot_init_timer…」→「栈顶是 Frozen::get，其下是 platform_desc、init_protection、kmain…；时钟/中断消费者在这一症状里不会出现」 |
| RD4-READER-C-04 | §1.4 五段链表 ② 行 | 「引导器（loader / boot-shim）」→「C：内核自己扫描 EBDA/BIOS 区并填 `machine.acpi_rsdp`（§2.2）；Rust：boot-shim 从配置表/`a1` 取地址装进 `KernelInfo.platform_sources`（§4.2）」 |
| RD4-READER-C-05 | §3.3 分工表与表后 | 表行「"有多少根 IRQ 线"」→「"IRQ 向量空间有多大"」；表后加「`nr_irqs` 报的是 IRQ 向量空间的容量（…§4.8 的 `min(NR_IRQ_VECTORS)`），不是控制器引脚数；x86 的 IOAPIC 引脚数是 `IOAPIC_NUM_PINS = 24`…（§2.6、R04-T10）」。备注：§3.2 的源码引文「Number of IRQ sources supported.」保留原样（引文不可改），统一解释放在 §3.3 |
| RD4-READER-C-06 | §4.9 `init_protection` 节选之后 | 新增一段：两处 `unwrap_or` 回退不同（`bsp_id` vs 逻辑 0）、Phase B 只能用保守值、`elected_bsp_hw_id` 的回退是 `topo.bsp_id` 且不保证在 `cpus[0..nr_cpus]` 内、选举失败会把 BSP 逻辑号定成不同 CPU。备注：未统一回退、未删分支（属代码/设计决定，文档侧只记录差异）；源码核实 `os/kernel/src/lib.rs:1923-1952`、`2217-2266` |
| RD4-READER-C-07 | 头部「三档术语」 | 补一句「aarch64 的发现链在 C 树里同样没有先例（§1.6），对应的架构替换（GICv3/Generic Timer 取代 32 位 ARM 的 OMAP INTC/BSP timer）已在 05-clock-interrupt-init.md 的头部术语档归入 `[ARCH: K-1]`」 |
| RD4-READER-C-08 | §3.2 ACLINT SSWI 句；§4.11 表后 | 「ACLINT SSWI 是 RISC-V 的能力（…）」→「（`[ARCH: K-2]`；…）」；§4.11 表后加「riscv64 这一列整列属 `[ARCH: K-2]`：Minix3 没有 RISC-V 版本，这一列没有 C 对照物（§1.6）」。备注：`[ARCH: mmio-device]` 的解码处已由篇首第 3 行指向 README「全局阅读约定」，未重复 |
| RD4-READER-C-09 | §4.4 错误清单句 | 「失败分别报 [六项]」→「根表/MADT 的闸分别报 [六项]；`AcpiParseError` 的其余变体是 `BadRsdpSignature` / `NoXsdtPointer` / `MadtNotFound` / `MadtTooShort` 与 GIC 决策表的 `GicdNotFound` / `GicrNotFound` / `GicVersionUnsupported`」。源码核实 `os/libs/minix-platform/src/acpi.rs:781-814` 共 13 个变体，全部覆盖 |
| RD4-READER-C-10 | §3.8 ISA 字面量段 | 「三个来源的每一个都要填…三个值必然相同」→「x86-64 上能构造 `PitDesc` 的两个来源（ACPI 与 QemuVirt）的每一个都要填…两个值必然相同」 |
| RD4-READER-C-11 | §4.10 逐字段去向表 `board_id` 行 | 「**淘汰**：Rust 侧无板卡标识…」→「**淘汰语义、保留 ABI 位置**（恒 0，同 `padding`）：Rust 侧无板卡标识…」 |
| RD4-READER-C-12 | §4.8 消费点表 `ArchInit` 行 | 「…字段标 `#[allow(dead_code)]`，当前无消费者」→「…当前无消费者——这里的「无消费者」指 `X86_64ArchInit` 的实例字段；`ArchMiscDesc.acpi_tables` 由 §4.10 的 `getinfo_machine` 消费」 |

## 附：未处理项

- 9 条 SUGGESTION（A-08、A-09、B-06、B-07、B-08、B-09、C-13、C-14、C-15）按任务口径不处理。
- 无 `SKIP-NEED-SOURCE`：A-04 / A-05 / A-06 / B-05 / C-02 / C-09 / C-06 的源码或命令核实均已完成并写在对应行。
