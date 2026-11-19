# D04 R2 确认轮 · P-READER（A+B+C 合并）

VARIANCE: A+B+C 合并一个 agent

- 文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 基线：2708 行；sha256 前缀 `773c10f1`。本次复核：`wc -l` = 2708，`sha256sum` 前 16 位 = `773c10f1bf69de6f`。
- 读者画像：`tmp/review-eval/01-stage-kernel/04-platform-discovery/reader-profile.md`。
- 读取范围：以上两份。未读源码、其他文档、diff、历史评审、其他面板、`expected/`。
- 面板口径（本 agent 自定；因只允许读上述两份，无法读取 `review-panel.md` 的原始定义，故在文内显式声明）：
  - A = 论证链与内部一致性：定义、枚举、计数、交叉引用、结论与证据是否对得上。
  - B = 事实与证据：摘录是否支持断言、算术与单位、命令与输出口径、事实/推断分界。
  - C = 读者可达性：目标读者能否按 §教学目标 独立复述；前置知识缺口、术语、指代、导航。
- 命中清单图例：A1 论证链完整；A2 定义/术语一致；A3 枚举/计数一致；A4 交叉引用；A5 结论-证据匹配。B1 摘录忠实；B2 算术/单位；B3 命令/口径；B4 事实/推断分界。C1 前置知识可达；C2 术语/指代；C3 可复现；C4 教学目标覆盖。G1–G6 对应读者画像的六条教学目标。
- 严重度：P1 会改变设计判断；P2 会让读者形成错误结论或无法完成主线；P3 局部精度/导航问题。

## A 面板：论证链与内部一致性

## FINDING RD4B-READER-A-01（P2）

**位置**：§1.4 ⑤（行 112–116）与 §4.1（行 1541）。

**摘录**：§1.4 ⑤：「C 侧：`apic_single_cpu_init` / `discover_cpus` / `acpi_init_poweroff`（FACP 表）三处内核内消费者，外加用户态 ACPI 驱动经 `machine.acpi_rsdp` 的拷贝」；§4.1：「C 侧消费 ACPI 结果的入口只有少数几处——APIC 初始化（`apic_single_cpu_init`，非 SMP 构建；SMP 构建下并入 `smp_init` 路径，§2.1）与 CPU 拓扑发现（`discover_cpus`，§2.4），外加用户态 ACPI 驱动经 `machine.acpi_rsdp` 的一处（§2.8）」。

**严重度**：P2。

**问题**：同一件事在前文列三处内核内消费者（含 `acpi_init_poweroff`），在后文用「只有少数几处——…与…，外加…」的封闭枚举列两处，FACP/poweroff 被丢掉。§4.1 正是用这个枚举支撑「消费者变多了，所以发现必须前移」的论证；枚举口径不一致会让论证的 C 侧基线漂移。

**影响**：读者无法判断 FACP/poweroff 是否属于 Rust 侧必须复刻的消费者；若属于，§4.1 的比较基数少一个，「C 侧只有少数几处」与「Rust 侧消费者变多」的对照就不再是同一口径。

**证据**：§1.4 行 113–116 明写「三处内核内消费者」；§4.1 行 1541 用「只有少数几处——…与…，外加…」作封闭枚举，未提 `acpi_init_poweroff`。两段都在本篇内，无需外部证据。

**最小修复**：在 §4.1 的枚举里补上 `acpi_init_poweroff`（FACP），或加一句「此处只列与本篇四组参数直接相关的消费者；FACP/poweroff 见 §1.4」。

**置信度**：高（纯文档对读）。若作者有意将 FACP 排除在「四组问题」之外，则属于「需显式限定」而非漏列。

**是否需源码核实**：否（文档内部即可判定）。若要论证 FACP 属于「消费 ACPI 结果」，需源码核实 `acpi_init_poweroff` 的输入确实取自 ACPI 表（§1.4 自己已如此断言，但未展开）。

**若我错了**：如果 §4.1 的「消费 ACPI 结果的入口」有意排除 FACP/poweroff，那么应把「只有少数几处」改成「与本篇四组参数相关的入口有…」；否则就是漏列。

**命中清单**：A2 定义一致；A3 枚举一致；A5 结论-证据匹配；G1、G4。

## FINDING RD4B-READER-A-02（P3）

**位置**：§4.9（行 2208–2215、行 2243）与 §4.10 `bsp_id` 行（行 2325）。

**摘录**：`init_smp_state`：「The DTB/ACPI `bsp_id` is a hint, not the truth: election/CPUID resolves the CPU actually executing this code.」；随后 `let bsp_logical = (0..topo.nr_cpus as usize).find(|&i| topo.cpus[i].hw_id == bsp_hw).unwrap_or(topo.bsp_id as usize) as u32;`；§4.9：「`elected_bsp_hw_id` 本身的回退是 `topo.bsp_id`，同样不保证返回值在 `topo.cpus[0..nr_cpus]` 内。」；§4.10：「改型：来源从常量改为 arch election 的逻辑 id（`elected_bsp_hw_id`）…；`cpu_topology().bsp_id` 只是"第一条 enabled 记录"的 hint」。

**严重度**：P2。

**问题**：`CpuTopology.bsp_id` 同时被当作两种量使用。`elected_bsp_hw_id` 的回退把它当 **hw_id**（与 `topo.cpus[i].hw_id` 比较），`init_smp_state` 的 `unwrap_or(topo.bsp_id as usize)` 又把它当 **逻辑槽位下标**。§4.10 一边叫它「第一条 enabled 记录」（听起来像记录序号/逻辑量），一边把 `elected_bsp_hw_id` 的返回值叫「逻辑 id」（函数名却是 hw id）。文档自始至终没有给出这个字段的单位与取值范围。

**影响**：两种解释各引出一个具体故障。若 `bsp_id` 是 hw_id，`unwrap_or(bsp_id as usize)` 可能越界索引 `CpuId::new_unchecked`；若它是逻辑下标，把它当 hw_id 回退就是范畴错误，选举失败时找不到正确的执行 CPU。两者都会落到 §4.9 自己描述的 `gs:0x10` 错槽位形态，而系统不报错。

**证据**：§4.9 行 2210–2215 的注释与 `unwrap_or(topo.bsp_id as usize)`；行 2243 明说「不保证返回值在 `topo.cpus[0..nr_cpus]` 内」，即承认它被当作 hw_id 用；§4.10 行 2325 又称其为「逻辑 id」。三处互相矛盾，且 §5.4 的 TODO 表没有对应条目。

**最小修复**：在 §4.9 或 §4.10 明写 `CpuTopology.bsp_id` 的单位（hw_id 还是逻辑下标）、合法范围，以及 `unwrap_or` 两边各自的语义；若确实存在越界风险，补一条 TODO（当前 R04 表没有覆盖）。

**置信度**：高（文档内三处自相矛盾）；对「是否真的会越界」为中等（取决于解析器保证的 `bsp_id < nr_cpus` 与 `hw_id` 的取值范围）。

**是否需源码核实**：是。需要看 `os/libs/minix-boot/src/platform.rs` 的 `CpuTopology.bsp_id` 定义、`elected_bsp_hw_id` 的实现与两个解析器填 `bsp_id` 的位置，才能判断是笔误、文档缺口还是活 bug。

**若我错了**：如果 `bsp_id` 的定义就是「第一条 enabled 记录的 hw_id」，且解析器保证它与 `cpus[].hw_id` 同域，那么 `unwrap_or(bsp_id as usize)` 仍是把 hw_id 当下标——除非所有机器上 hw_id 恰等于下标。文档需要给出这条不变量；否则不能算我错。

**命中清单**：A1 论证链完整；A2 定义/术语一致；C2 术语/指代；G4、G5。

## FINDING RD4B-READER-A-03（P2）

**位置**：§2.7 启动参数映射表第一行（行 807）与随后的正文（行 813）。

**摘录**：表格第一行：「`no_apic` 未给出 | 1（缺省） | 先按 `config_no_apic` 置 1、再被缺省的 `no_smp` 覆盖为 0 | `kmain` 因 `config_no_apic` 走 `smp_single_cpu_fallback`，`smp_init` 不跑；`acpi_init` 只有 `arch_init` 里那一处」；正文：「SMP 构建的 `cstart` 先按 `config_no_apic` 置 `config_no_smp`，再用 `no_smp` 覆盖（缺省 0），而 `kmain` 只读取这两个开关（`main.c:304-307`）。」

**严重度**：P2。

**问题**：表格第三列说 `config_no_smp` 的最终值是 **0**，第四列却把「走 `smp_single_cpu_fallback`」归因于 `config_no_apic=1`。这只有在 `kmain` 的条件是 `config_no_apic || config_no_smp` 时才成立；文档只说「`kmain` 只读取这两个开关」，从未引出这个条件。若 `kmain` 只看 `config_no_smp`，第四列就是错的。此外「先按 `config_no_apic` 置 1、再被缺省的 `no_smp` 覆盖为 0」使第一次赋值完全无效，形状可疑。

**影响**：§2.7 的核心结论——「默认配置下 `acpi_init` 只跑 `arch_init` 里那一处」——依赖这条分支；读者无法用文中给出的信息复算这张表，也无法判断 `no_apic=0` 且 `no_smp` 缺省时到底走 `smp_init` 还是 fallback。

**证据**：§2.7 行 807 的第三列（最终值 0）与第四列（fallback 归因 `config_no_apic`）；行 813 只给出「`kmain` 只读取这两个开关」而未给条件表达式。所需的条件式在文档内缺失。

**最小修复**：在表格上方或注脚引出 `main.c:304-307` 的实际条件（例如 `if (config_no_apic || config_no_smp) …`），或把第三列改成「最终值 + 让 kmain 走 fallback 的那个开关」两列，避免把赋值过程与判定依据混在一格。

**置信度**：高（表格内部逻辑无法闭合）；对「实际代码究竟怎么判」为中等。

**是否需源码核实**：是。需要看 `minix3/minix/kernel/main.c` 的 `cstart` 中 `config_no_apic`/`config_no_smp` 赋值段与 `kmain` 的分支条件（文档已给 `main.c:304-307`、`main.c:443-468` 两个锚点，但未引原文）。

**若我错了**：如果 `kmain` 的条件确实是 `config_no_apic || config_no_smp`，那么表格结论成立，但正文必须补出这个条件；否则读者仍然无法复算。这种情况下本条降级为「缺一句条件引用」的 P3。

**命中清单**：A1 论证链完整；A5 结论-证据匹配；B3 命令/口径；G4。

## FINDING RD4B-READER-A-04（P2）

**位置**：§3.2（行 1111）、§4.7（行 2108）、§1.2（行 56）与 §5.2（行 2479）。

**摘录**：§3.2：「`psci_conduit()` 的消费者在 SMP 的 AP 拉起路径上：aarch64 用它在 `hvc` 与 `smc` 之间选 PSCI 调用通道（`os/kernel/src/smp.rs` 读值后传给 `os/arch/src/arm64/smp.rs:fn set_psci_conduit_smc`）。」；§4.7：「它只有 `DeviceTree` 一臂是真的，其余走 `_ => None`。」；§5.2：「aarch64 变体的头部注释直接写明它走的是 ACPI 而不是 DTB（"the discovery path is ACPI, not DTB on AAVMF"）。」

**严重度**：P2。

**问题**：aarch64 的主导航路径（AAVMF/ACPI）不产生 `psci_conduit()` 的值——只有 `DeviceTree` 变体是真的，ACPI 走 `_ => None`。但 §3.2 又把该方法的消费者定位在 SMP 的 AP 拉起路径上。文档没有说消费者拿到 `None` 时做什么：是默认 `hvc`、默认 `smc`、还是拒绝拉起 AP。§3.2 的解释「回答不了的来源用默认值表示"没有这个能力"」在 AArch64 上是误导——AArch64 SMP 不可能「没有 PSCI」这个能力。

**影响**：读者无法判断 AAVMF/ACPI 这条实际启用的 aarch64 路径上 AP 能不能起来。若消费者没有对 `None` 的默认处理，这就是发现路径与 SMP 启动路径的接口断裂；若有默认值，文档应写明，否则读者会把 `None` 当作「不支持」。

**证据**：§4.7 行 2108 的「只有 `DeviceTree` 一臂是真的，其余走 `_ => None`」；§1.2 行 56 与 §5.2 行 2479 都确认 aarch64 实测走 ACPI；§3.2 行 1111 说消费者在 AP 拉起路径上。三者合起来暴露出 `None` 的处理缺口，文档内没有补句。

**最小修复**：在 §3.2 或 §4.7 加一句「ACPI 来源返回 `None`；`os/kernel/src/smp.rs` 将 `None` 解释为 `hvc`（或 `smc`/不支持）」并给出代码锚点；若实际是默认 `hvc`，同时说明真实硬件要求 `smc` 时怎么办（可登记 TODO）。

**置信度**：高（缺口在文档内可证）；对「是否构成活 bug」为中等（取决于消费端对 `None` 的默认）。

**是否需源码核实**：是。需要看 `os/kernel/src/smp.rs` 读取 `psci_conduit()` 后的分支，以及 `os/arch/src/arm64/smp.rs:fn set_psci_conduit_smc` 的默认通道。

**若我错了**：如果 ACPI 路径还有另一个 PSCI 来源（例如 GTDT/FADT 的 ARM 启动架构字段），那么 §4.7 的「只有 `DeviceTree` 一臂」需要修正；无论哪种情况，文档都得补出 ACPI 路径的答案。

**命中清单**：A1 论证链完整；A5 结论-证据匹配；C1 前置知识可达；G2、G4。

## FINDING RD4B-READER-A-05（P3）

**位置**：§4.1（行 1567–1569）。

**摘录**：「"必须早于谁"的确切名单就是这三个构造点：`CurrentInterruptController::new` 与 `CurrentArchInit::new`（`init_clock_and_interrupts` 内）、`CurrentClockArch::new`（后续 `boot_init_timer` 内）。」；紧接着下一段：「Phase B 的第一个动作 `init_protection` 已经无条件读 `platform_desc()`（用于 BSP logical id），所以无论把调用点挪到 `init_clock_and_interrupts` 之后还是 `boot_init_timer` 之后，都会先在 `init_protection` 触发同一句断言。」

**严重度**：P3。

**问题**：「必须早于谁」被写成「确切名单就是这三个构造点」，但紧接着的段落自己给出了第四个读取点：`init_protection` 无条件调用 `platform_desc().cpu_topology()`。`init_protection` 不是 `new(desc)` 构造点，却是发现必须先于它的真实理由之一。名单与下一段互相拆台。

**影响**：读者若把「三个构造点」当作完整的移动安全检查表，会漏掉 `init_protection`；而下一段又告诉他真正先撞上断言的就是 `init_protection`。这正是 §4.1 想教的「前移理由」，名单的封闭措辞反而削弱了它。

**证据**：§4.1 行 1567 的「确切名单就是这三个构造点」；行 1569 的「`init_protection` 已经无条件读 `platform_desc()`」。两段相隔两句，文档内部即可判定。

**最小修复**：把「确切名单」改成「构造点名单是这三个；读取点还包括 `init_protection` 的 `cpu_topology()`」，或在列表末尾补第四项。

**置信度**：高。

**是否需源码核实**：否（文档内部矛盾）；若要确认 `init_protection` 的读取路径，可看 `os/kernel/src/lib.rs:fn init_protection`，但 §4.1 自己已经断言。

**若我错了**：如果作者把「名单」限定为「`new(desc)` 构造点」而非「所有必须晚于发现的读取点」，那么措辞应改为「构造点名单」；当前「必须早于谁的确切名单」读起来是全集。

**命中清单**：A3 枚举一致；A5 结论-证据匹配；C2 术语/指代；G4。

## FINDING RD4B-READER-A-06（P3）

**位置**：§4.11 六段表「⑥ 消费」aarch64 格（行 2355）与 §4.8 三个构造点表（行 2118）。

**摘录**：§4.11 aarch64 格：「`AArch64InterruptController` + `ArchInit` + `init_smp_state`」；同表 x86-64 格：「`X86_64InterruptController` + `X86_64ClockArch` + `init_smp_state`」；riscv64 格：「`Riscv64InterruptController` + `Riscv64ClockArch` + `init_smp_state`」。§4.8：「`boot_init_timer` | `ClockArch` | x86-64 `PitDesc` / aarch64 `ArmGenericTimerDesc` / riscv64 `ClintDesc`」。

**严重度**：P3。

**问题**：§4.8 明确列出 aarch64 也有 `ClockArch` 构造点（期望 `ArmGenericTimerDesc`），但 §4.11 的 aarch64「消费」格用 `ArchInit` 顶替了时钟，而 `ArchInit` 的构造参数是 `&ArchMiscDesc`、不是 `TimerDesc`，两者不能互换。表格因此把「三架构同构」写成了一个不对称的形状。

**影响**：读者按 §4.11 复述「实现链」时，会得出 aarch64 没有时钟消费者、或时钟由 `ArchInit` 负责的错误结论；aarch64 的定时器频率（`CNTFRQ_EL0`，§4.5 表）究竟由谁读，在六段表里找不到落点。

**证据**：§4.8 行 2118 的 `ClockArch` 行含 aarch64；§4.11 行 2355 的 aarch64 格没有 `AArch64ClockArch`（或实际类型名）。两处都在本篇内。

**最小修复**：在 §4.11 的 aarch64 格补上时钟消费者（实际类型名，可能是 `AArch64ClockArch`），或加一句「aarch64 的时钟初始化由 `ArchInit` 承担」并说明为何 §4.8 仍列 `ArmGenericTimerDesc`。

**置信度**：高（文档内部即可判定不对称）；对实际类型名需源码核实（文档没有给出 aarch64 的 `ClockArch` 实现名）。

**是否需源码核实**：是（仅需确认 aarch64 的 `ClockArch` 实现名；不对称本身不需要源码）。

**若我错了**：如果 aarch64 的 `ClockArch` 确实由 `ArchInit` 路径一并构造（例如 `ArchInit` 内部再 `new` 一次 `ClockArch`），那么 §4.8 的「三个构造点」与 §4.11 的消费格需要统一措辞；当前两份表不能同时正确。

**命中清单**：A2 定义一致；A5 结论-证据匹配；G4、G5。

## FINDING RD4B-READER-A-07（P2）

**位置**：§3.5 兜底场景表（行 1320–1328）、§5.3 `platform_sources: &[]` 分布表（行 2528）与 §5.3 判据句（行 2530）。

**摘录**：§3.5：「| QEMU 集成测试 | 否 | QEMU virt 提供 ACPI 或设备树，解析走真实路径 |」；§5.3：「21 个极简测试内核（… `test-protection` 各架构变体 …）| 各 1 | 这些测试内核在 `main.rs` 里直接构造 `KernelInfo`，不经过 boot-shim 的固件发现阶段」；同节判据：「上述 54 处的被测单元（页表映射、ELF 装载、进程初始化、内核映像布局）都不读平台描述符」。

**严重度**：P2。

**问题**：§3.5 把「QEMU 集成测试」整体判为「不触发兜底」，但 §5.3 自己列出 21 个 QEMU 测试内核的 `platform_sources: &[]`；按 §3.5 同一张表的规则「`platform_sources` 为空 → 触发兜底」，这 21 个测试恰恰会走 `QemuVirtDesc` 兜底。同一节还说这些被测单元「都不读平台描述符」，但 `kmain` 的 Phase A.5 对 `KernelInfo` 无条件调用 `init_from_kinfo`，空切片也会冻结一份兜底描述符；凡进入 Phase B 的测试都会经 `init_protection`/中断初始化读到它。

**影响**：读者在 QEMU 测试里看到 QEMU 地址时，会按 §3.5 判断「这是真实解析结果」，实际可能是兜底；§5.3 用「不读平台描述符」论证 54 处 `&[]` 是安全夹具，前提也不准确——它们不依赖固件来源，但确实消费兜底描述符。这会影响 R04-T05/T06 的缺口判断与诊断路径选择。

**证据**：§3.5 行 1324–1326 的「QEMU 集成测试 否」；§3.5 行 1326 的「`platform_sources` 为空 → 是」；§5.3 行 2528 的 21 个 `&[]` 测试内核与行 2530 的「都不读平台描述符」。三处都在本篇内，规则与实例直接冲突。

**最小修复**：把 §3.5 的「QEMU 集成测试」一行拆成两类：「走 boot-shim 的真机门测试（否）」与「直接构造 `KernelInfo`、`platform_sources: &[]` 的极简测试（是，走 `QemuVirtDesc` 兜底）」；同时把 §5.3 的「都不读平台描述符」改成「都不依赖固件来源，消费的是兜底描述符」。

**置信度**：高（文档内部规则与实例直接冲突）；对「21 个测试是否都进入 Phase B」为中等（若某些测试根本不调 `kmain`，则只有其中一部分触发兜底，结论方向不变）。

**是否需源码核实**：否（文档内即可判定冲突）；若要精确列出哪些测试进入 Phase B，可看 `os/qemu-tests/test-kernels/.../src/main.rs`，但 §5.3 已给出「直接构造 `KernelInfo`」的定性。

**若我错了**：如果这 21 个测试虽然写 `platform_sources: &[]`，但根本不调用 `init_from_kinfo`（例如只测页表、在 Phase A.5 之前返回），那么「触发兜底」不成立；但 §5.3 说它们构造 `KernelInfo`，且 §5.4 M8 把「全失败兜底」列为未覆盖，暗示正常路径会走 `init_from_kinfo`。文档需要补出这条区分。

**命中清单**：A1 论证链完整；A3 枚举一致；A5 结论-证据匹配；B4 事实/推断分界；G6。

## B 面板：事实与证据

## FINDING RD4B-READER-B-01（P3）

**位置**：§4.2 x86-64 GUID 扫描摘录（行 1575–1585）、扫描顺序表（行 1605）与 aarch64 单次判断的反例段（行 1625）。

**摘录**：代码注释：「// On x86-64, look for ACPI RSDP (prefer ACPI 2.0+).」；循环体：

```text
for e in entries {
    if e.guid == ACPI2_GUID || e.guid == ACPI_GUID {
        let pa = PhysBytes(e.address as u64);
        ...
        break;
    }
}
```

表：「x86-64 | `ACPI2_GUID` 或 `ACPI_GUID`（同一次判断） | `break`」。反例段：「循环按配置表项顺序扫描，先遇到哪个 GUID 就取哪个——来源由固件给出的表项顺序决定，不由设计意图决定。」

**严重度**：P2。

**问题**：aarch64 的反例段批评「单次 GUID 判断 + 先遇到先取」会让固件表项顺序决定来源；x86-64 的摘录正是同一形状：一个 `||` 判断加 `break`。若配置表里 ACPI 1.0 的 `ACPI_GUID` 项排在 ACPI 2.0 的 `ACPI2_GUID` 项之前，x86-64 会取 1.0 RSDP 并 `break`，永远看不到 2.0 项。注释「prefer ACPI 2.0+」在这个形状下不成立。

**影响**：一台同时提供两个表项的机器可能被降级到 rev 1 RSDP，随后按 §4.4 的规则回落 RSDT，XSDT（64 位表项）不可达；这与文档自己主张的「ACPI 2.0+ 优先」相反，也与它对 aarch64 的批评标准不一致。

**证据**：§4.2 行 1575–1585 的 `||` + `break`；行 1605 的表把两者写成「同一次判断」；行 1625 自己论证「先遇到哪个就取哪个」是缺陷。三处合起来即可判定，无需外部证据。

**最小修复**：改为两段独立循环（先 `ACPI2_GUID`，再 `ACPI_GUID`），或把注释/表改成「接受任一版本、由固件表项顺序决定」并说明 rev 1 的后果。

**置信度**：中高。摘录标注了「示意…非逐字」，实际代码可能是两段循环而被摘录合并；但文档呈现给读者的就是单次 OR + break，读者无法据此复现「prefer ACPI 2.0+」。

**是否需源码核实**：是。需要看 `os/boot-shim/src/uefi_helpers.rs:fn find_platform_sources_into` 的 x86-64 段是否真的是单循环单条件。

**若我错了**：如果实际代码是两段独立循环，那么本条降级为「摘录失真」的 P3，修复是补出两段循环或注明摘录省略了第一段。

**命中清单**：B1 摘录忠实；B4 事实/推断分界；A1 论证链完整；G2。

## FINDING RD4B-READER-B-02（P2）

**位置**：§4.2 x86-64 摘录（行 1581）、§1.3 的「只搬地址」原则（行 87）、§1.4 的例外注记（行 98–100）与 §4.2 的例外展开（行 1627）。

**摘录**：摘录：「`let len = unsafe { minix_boot::rsdp_total_size(pa) };`」与「`PlatformDescSource::with_optional_len(RSDP, pa, len)`」。原则：「当前设计把窗口内的平台发现动作压到最小——只搬地址（x86-64 一个、aarch64 至多两个，§4.2）」。例外：「引导阶段另有两处例外：memmap 装配所需的 `/memory` 子集，以及 aarch64 UEFI 腿的 DTB 预校验」。

**严重度**：P2。

**问题**：`rsdp_total_size(pa)` 以 RSDP 物理地址为参数，要得到 `len` 必须读 RSDP 的 revision/length 字段（或至少读它的长度字段），这不是「只搬地址」，而是对固件描述的读取/预解析。§1.4 与 §4.2 的例外清单只有 `/memory` 与 aarch64 DTB 预校验两项，没有 `rsdp_total_size`。

**影响**：这是本篇的第一原则（窗口内动作最小化、职责切分）与实现摘录的直接冲突。读者无法判断 boot-shim 到底对 RSDP 做了多少校验、`len` 是可信值还是提示值，也无法据此评估跨二进制交接面的大小。

**证据**：§4.2 行 1581 的调用与参数 `pa`；§1.3 行 87 的「只搬地址」；§1.4 行 98–100 与 §4.2 行 1627 的两项例外清单。文档内没有任何一处把 `rsdp_total_size` 列入例外或解释它只读几个字节。

**最小修复**：把 `rsdp_total_size` 加入 §1.4/§4.2 的例外清单，写明它读 RSDP 的哪几个字段（revision、length）与为什么必须读；或者改为把 `len` 置 0（unknown/legacy）交给内核解析器计算，保持「只搬地址」的字面成立。

**置信度**：高（函数名、参数与用途都指向读表）；若 `rsdp_total_size` 只做常数上界（不读 `pa`），则参数无意义，仍需说明。

**是否需源码核实**：是。需要看 `minix_boot::rsdp_total_size` 的实现，确认它读了哪些偏移，以及 `len` 在交接线格式里是否被信任。

**若我错了**：如果 `rsdp_total_size` 是编译期常量或只返回固定上限，那么文档应改名/说明；否则本条成立。若作者认为「读长度字段」不算解析，那么「只搬地址」应改成「只取地址与长度」，并同步 §1.3/§1.4。

**命中清单**：B1 摘录忠实；B4 事实/推断分界；A1 论证链完整；G3。

## FINDING RD4B-READER-B-03（P3）

**位置**：§5.3 第一行「测试缺口」（行 2505）。

**摘录**：「`os/boot-shim/src/uefi_helpers.rs:fn find_platform_sources_into` 没有单测（该文件有 12 条 `#[test]`（1 条测 `KernelInfo` 字段、5 条测 bootstrap memmap 边界），均不覆盖 `find_platform_sources_into`）」。

**严重度**：P3。

**问题**：括号里的分类只解释了 6 条（1 + 5），而总数写的是 12 条；剩下 6 条既未归类，也未说明是否覆盖 `find_platform_sources_into`。括号的写法读起来像对 12 条的全量分解，实际只覆盖一半。

**影响**：这条是「`find_platform_sources_into` 无单测」的证据；证据本身算术不闭合，读者无法确认剩余 6 条测试是什么、是否可能间接覆盖 GUID 扫描。测试缺口的可信度被削弱。

**证据**：§5.3 行 2505 的「12 条」与「1 条…、5 条…」；文档内没有第三类。用 `1+5` 与 `12` 对读即可判定。

**最小修复**：补出剩余 6 条的类别，或改成「12 条中有 1 条测…、5 条测…，其余 6 条为…；均不覆盖 `find_platform_sources_into`」。

**置信度**：高（纯算术与文档内证）。

**是否需源码核实**：是。需要数 `os/boot-shim/src/uefi_helpers.rs` 的 `#[test]` 条数与各自主题，确认「12」与「1+5」的口径（是否按函数/按断言/按 `#[test]` 属性）。

**若我错了**：如果「12 条」是文件级 `#[test]` 总数而括号只列与 `KernelInfo`/memmap 相关的子集（其余 6 条属其他主题），那么只需把「（1 条…、5 条…）」改成「其中 1 条…、5 条…，其余 6 条…」；结论（均不覆盖）可能仍成立，但证据链要补全。

**命中清单**：B2 算术/单位；B3 命令/口径；A5 结论-证据匹配；G6。

## FINDING RD4B-READER-B-04（P3）

**位置**：§4.8 地址语义表 riscv64 行（行 2170）、§3.4 `ClockArch` 表（行 1258–1269）与 §4.5 per-CPU 地址段（行 1937）。

**摘录**：§4.8：「riscv64 | 物理地址（PLIC/CLINT） | PLIC 实例存 `plic_base` 与 `plic_va_base`；内核 check-and-reject 后注入高半 VA | PLIC 走高半 VA；CLINT 定时器走 SBI `TIME` ecall」。§3.4：「`Riscv64ClockArch::new` … `Self { mtime_addr: clint.mtime_addr, mtimecmp_base: clint.mtimecmp_base, mtimecmp_stride: clint.mtimecmp_stride, freq: clint.freq }`」。§4.5：「riscv64 是 `mtimecmp_base + hw_id * mtimecmp_stride`」。

**严重度**：P2。

**问题**：如果定时器路径完全走 SBI `TIME` ecall，那么 `mtime_addr`/`mtimecmp_base`/`mtimecmp_stride` 搬进 `ClockArch` 实例后没有被 MMIO 消费；如果它们仍被消费（例如读 `mtime` 或写 `mtimecmp`），那么 §4.8 的「MMIO 实际访问」列就漏了这条路径。文档没有把「哪些操作走 CLINT MMIO、哪些走 SBI」拆开。

**影响**：读者无法判断 CLINT 地址是活消费者还是「描述符里有、活路径不读」的另一例（与早期控制台同类）。这直接影响 §3.8 的判别规则（有报告协议 ⇒ 进描述符）与 R04-T03/T11 的定位：若 CLINT 地址是死字段，它不该被当作 §3.4 实例化的样本；若读 `mtime` 走 MMIO，则 §4.8 的表不完整。

**证据**：§4.8 行 2170 的「CLINT 定时器走 SBI `TIME` ecall」；§3.4 行 1258–1269 的实例字段；§4.5 行 1937 的 per-CPU `mtimecmp` 计算。三处都在本篇内，且没有一句把二者接起来。

**最小修复**：在 §4.8 的 riscv64 行或 §3.4 的 `ClockArch` 表下加一句：「`read_ticks` 经 CLINT `mtime` MMIO（或 `rdtime`）读取；设置比较值经 SBI `TIME` ecall，`mtimecmp_*` 字段当前不用于 MMIO」——按实际实现二选一，并标明未用字段。

**置信度**：高（缺口在文档内可证）；对「实际哪条路径用哪个字段」为中等。

**是否需源码核实**：是。需要看 `os/arch/src/riscv64/clock.rs` 的 `read_ticks`/`init_timer` 是否解引用 `mtime_addr`/`mtimecmp_base`，以及 SBI `TIME` 调用的位置。

**若我错了**：如果 SBI `TIME` 只是「设置下一次中断」而读计数仍走 CLINT MMIO，那么本条降级为「§4.8 表格缺一行说明」的 P3，修复仍是补一句分工；反之若两者都走 SBI，则 CLINT 字段应标为未用。

**命中清单**：B1 摘录忠实；B4 事实/推断分界；A1 论证链完整；G1、G5。

## FINDING RD4B-READER-B-05（P3）

**位置**：§4.11 六段表「真机证据」行 x86-64 格（行 2356）与 §5.3 `platform_sources: &[]` 分布表（行 2528）。

**摘录**：§4.11：「真机证据 | `test-smp-topo`、`test-timer-irq`、`test-protection` | … | 各自有交接路径的载体」。§5.3：「21 个极简测试内核（… 如 `hello-boot` / `test-paging-enable` / `test-protection` 各架构变体 …）| 各 1 | 这些测试内核在 `main.rs` 里直接构造 `KernelInfo`，不经过 boot-shim 的固件发现阶段」。

**严重度**：P3。

**问题**：`test-protection` 被列为六段发现链的真机证据，但 §5.3 明确把它归入「用 `platform_sources: &[]`、不经过 boot-shim 固件发现阶段」的 21 个测试内核。它验证的是兜底描述符（`QemuVirtDesc`）的消费，不是固件描述 → 取址 → 分派 → 解析这条链。

**影响**：证据行夸大了覆盖。读者可能据此认为 `test-protection` 验证了 RSDP/MADT 解析或 GUID 扫描；实际上它连 `platform_sources` 都是空的。§5.4 M1/M11 的覆盖结论也会被错误引用。

**证据**：§4.11 行 2356 的 x86-64 证据格；§5.3 行 2528 的 21 个测试内核清单与「不经过 boot-shim 的固件发现阶段」的说明。两处都在本篇内。

**最小修复**：把 `test-protection` 从「真机证据」格移除，或标注「仅覆盖 ⑤/⑥ 的兜底描述符消费，不覆盖 ①–④」。

**置信度**：高（文档内部即可判定）。

**是否需源码核实**：否（§5.3 已给出定性）；若要确认 `test-protection` 的 `platform_sources` 确为 `&[]`，可看其 `src/main.rs`，但文档自己已列入清单。

**若我错了**：如果 `test-protection` 另有变体不经 `&[]` 而走 boot-shim，那么 §5.3 的 21 个清单需要修正；无论哪边错，两表必须一致。

**命中清单**：B1 摘录忠实；A5 结论-证据匹配；G6。

## C 面板：读者可达性

## FINDING RD4B-READER-C-01（P3）

**位置**：§4.6 `QemuVirtDesc::new()` 与 `Default`（行 1955–1993、行 2008–2030）及 §3.9 的 `const fn new()` 决策行（行 1490）。

**摘录**：`new()` 里 `cpu_topology: CpuTopology { nr_cpus: 4, bsp_id: 0, cpus: [CpuInfo::zero(); MAX_CPUS] }`；`Default` 里「`// cpus needs runtime initialization for per-CPU mtimecmp_addr fields.`」并逐颗填 `mtimecmp_addr: Some(MTIMECMP_BASE + i * MTIMECMP_STRIDE)`。§3.9：「`const fn new()` 与 `Default` 分工 | `#[derive(Default)]` 不是 `const fn`；per-CPU 地址需循环 | 全 `const` → 不能在 `new()` 填 per-CPU 字段，否决 | 多一个 `CpuInfo::zero()`」。

**严重度**：P2。

**问题**：文档解释了「为什么要有 `new()` 与 `Default` 两套」，但没有说「什么时候只能用 `default()`」。在 riscv64 上，`new()` 产生的实例四颗 CPU 全是 `CpuInfo::zero()`（`hw_id: 0`、`mtimecmp_addr: None`），而 `default()` 才填 per-CPU 比较器地址。读者无法从类型或文档判断 `new()` 是否是可用的运行时构造器。

**影响**：若读者（或测试内核）在 riscv64 上调用 `QemuVirtDesc::new()` 作为兜底描述符，per-CPU `mtimecmp` 地址全部缺失，定时器/IPI 的 per-CPU 初始化会落到错误路径，且没有编译期或运行期提示。§4.3 提到「载体里的直接安装」时没有说明它用的是 `new()` 还是 `default()`，正好踩在这个缺口上。

**证据**：§4.6 行 1955–1993 的 `new()` 与行 2008–2030 的 `Default` 对照；§3.9 行 1490 只讨论实现分工，未给调用点规则；§4.3 行 1670 只说测试内核「直接安装 `QemuVirtDesc`」，未说构造方式。

**最小修复**：在 §4.6 的 `new()` 上方或 §3.9 行补一句调用点规则（例如「运行时兜底必须用 `default()`；`new()` 仅供 `const` 上下文与不读 per-CPU 字段的测试」），或在 `new()` 的文档注释写明 riscv64 的 per-CPU 字段为零、需再调 `Default`。

**置信度**：高（文档缺口可证）；对「是否已有调用点误用」为中等（未读源码）。

**是否需源码核实**：是。需要看 `QemuVirtDesc::new()`/`default()` 的全部调用点，尤其是 riscv64 测试内核与 `qemu_fallback_or_panic`。

**若我错了**：如果 `new()` 在 riscv64 上根本不允许作为运行时描述符（例如只被 `const` 静态使用），那么补一句「禁止运行时使用」即可；缺口仍在，只是严重度降为 P3。

**命中清单**：C1 前置知识可达；C3 可复现；C4 教学目标覆盖；G4、G5。

## SUGGESTION RD4B-READER-C-02

**位置**：§4.6 `impl Default for QemuVirtDesc`（行 2010–2027）。

**摘录**：

```text
let mut d = Self::new();
const MTIMECMP_BASE: usize = 0x200_4000;
const MTIMECMP_STRIDE: usize = 8;
for i in 0..d.cpu_topology.nr_cpus as usize {
    d.cpu_topology.cpus[i] = CpuInfo {
        ...
        mtimecmp_addr: Some(MTIMECMP_BASE + i * MTIMECMP_STRIDE),
    };
}
```

**严重度**：P3（措辞/可维护性偏好，不阻断阅读）。

**问题**：`Self::new()` 已经把 `timer.mtimecmp_base` 与 `timer.mtimecmp_stride` 填成同样的 `0x200_4000`/`8`；`Default` 却用两个局部常量重复一遍，而不是读 `d.timer.mtimecmp_base`/`d.timer.mtimecmp_stride`。文档把这个实现当作「`const fn new()` 与 `Default` 分工」的解法展示，没有解释重复的理由。

**影响**：`new()` 与 `default()` 可能漂移：改一处常量而忘另一处，per-CPU 地址与描述符的 `timer` 字段就会不一致。读者可能把这个写法当作可复制模式。

**证据**：§4.6 行 1968–1980 的 `new()` 里 `ClintDesc { mtimecmp_base: 0x200_4000, mtimecmp_stride: 8, ... }`；行 2013–2024 的 `Default` 局部常量。同一节内对读即可。

**最小修复**：`Default` 改用 `d.timer.mtimecmp_base`/`d.timer.mtimecmp_stride`；若因借用/`const` 限制不能，加一句「这里重复常量是因为…」。

**置信度**：高（重复在摘录内可见）。

**是否需源码核实**：否（摘录已足够）；若要确认 `ClintDesc` 字段可见性，可看源码，但 `Default` 在同一模块内。

**若我错了**：如果作者故意让 `Default` 与 `new()` 解耦（例如 `new()` 的 timer 值将来可变），那么应写明这条设计意图；否则重复仍是风险。

**命中清单**：C2 术语/指代；C4 教学目标覆盖；B2 算术/单位。

## SUGGESTION RD4B-READER-C-03

**位置**：§5.1 标题与正文（行 2364–2415）。

**摘录**：「### 5.1 宿主：三个解析器的输入从哪来」；正文只展开 ACPI（`test_parse_synthetic_acpi`）与设备树（`TEST_DTB` / `test_parse_riscv64_qemu_virt_dtb`）两条。

**严重度**：P3（措辞偏好）。

**问题**：小节标题说「三个解析器」，但 `minix-platform` 里能被称作解析器的只有 `AcpiDesc::parse` 与 `DeviceTreeDesc::parse`；`QemuVirtDesc` 是常量表，不是解析器，且 §5.1 没有它的测试输入讨论。第三个「解析器」没有指代对象。

**影响**：读者会先找第三个解析器，找不到后再怀疑自己漏读；测试覆盖图（§5.1 的 26 条与 18 条）与标题的「三个」对不上，增加不必要的回读。

**证据**：§5.1 行 2364 的标题；行 2366–2415 只出现 ACPI 与设备树两类输入；§4.6 明说 `QemuVirtDesc` 是三份 per-arch 常量表、不是解析器。

**最小修复**：标题改为「两个解析器与 QemuVirt 常量：输入从哪来」，或补出第三个解析器的实际对象。

**置信度**：高。

**是否需源码核实**：否。

**若我错了**：如果作者把 `QemuVirtDesc::new()` 也算作「解析器」（把常量当输入），那么应在 §5.1 补一段 QemuVirt 的测试输入与断言；标题才成立。

**命中清单**：C2 术语/指代；C1 前置知识可达；G6。

## FINDING RD4B-READER-C-04（P3）

**位置**：§3.8 判别表「早期控制台基址」行（行 1472）、§3.3 per-arch 字段表 x86-64 行（行 1189）与 §4.4 ACPI 解析器全节（行 1690–1764）。

**摘录**：§3.8：「早期控制台基址 | 有（ACPI 的 SPCR 表；设备树里的串口节点） | 描述符里有这一项（`ConsoleDesc`），但活路径不读它——早期控制台走各架构自己的模块（§1.5 的边界事实）」。§3.3：「x86-64 | … | `IsaSerialDesc { port_base: u16 }`」。§4.4 全节只讲 RSDP、根表、MADT，未出现 SPCR。

**严重度**：P3。

**问题**：判别表把 SPCR 当作「早期控制台基址有报告协议」的证据，但 ACPI 解析器一节从未说 `AcpiDesc` 读 SPCR；x86-64 的 `IsaSerialDesc { port_base }` 在 ACPI 路径上究竟取自 COM1 常量、SPCR，还是留零，文档没有交代。读者只能从 QemuVirt 的 `0x3F8` 猜。

**影响**：§3.8 的「有协议 ⇒ 进描述符」在控制台一行上成了举例而非实际实现说明。排查「x86 描述符里串口基址不对」时，读者不知道应该去查 SPCR 解析还是常量；§1.5 的「活路径不读它」也没有说明写入侧的值从哪来。

**证据**：§3.8 行 1472 的 SPCR 引用；§4.4 行 1690–1764 没有 SPCR；§3.3 行 1189 只给字段类型不给来源。三处合起来，写入侧缺失。

**最小修复**：在 §3.8 行或 §4.4 末尾加一句：「x86-64 的 `AcpiDesc` 不解析 SPCR；`IsaSerialDesc.port_base` 取 COM1 常量 `0x3F8`（与 QemuVirt 相同）」——按实际实现写；若确实解析 SPCR，则在 §4.4 补出那段。

**置信度**：中高（缺口可证；具体实现可能如我所猜）。

**是否需源码核实**：是。需要看 `os/libs/minix-platform/src/acpi.rs` 是否构造 `IsaSerialDesc`、端口值从哪来。

**若我错了**：如果 `AcpiDesc` 根本不实现 `early_console()`（返回 `None`），那么 §3.8 的「描述符里有这一项」对 ACPI 来源不成立，判别表应分来源写；这仍是需要修的文档问题。

**命中清单**：C1 前置知识可达；C2 术语/指代；B4 事实/推断分界；G1、G4。

## SUGGESTION RD4B-READER-C-05

**位置**：§3.1 引用的 `KernelInfo` 文档注释（行 1069）。

**摘录**：「Empty slice means boot-shim did not provide any — kernel falls back to `QemuVirtDesc` (dev) or panics (release). See 04-platform-discovery.md §4.6.」

**严重度**：P3（导航/措辞偏好）。

**问题**：注释把兜底语义指向 `§4.6`，但 §4.6 是「`QemuVirtDesc`：三份 per-arch 常量表」，讲的是常量值与 `new()`/`Default` 分工；兜底分支的判定在 §3.5（`qemu_fallback_or_panic`）与 §4.7（`init_from_kinfo`）。读者顺着指针会落到常量表，找不到「dev 兜底 / release panic」的代码。

**影响**：一次无效跳转；读者可能以为 §4.6 里有兜底逻辑，读完后仍需回退到 §3.5/§4.7。

**证据**：§3.1 行 1069 的 `See … §4.6`；§4.6 行 1939–2041 的内容范围；§3.5 行 1288–1331 与 §4.7 行 2043–2079 才是兜底实现。§3.1 的代码块是逐字引用源码注释，因此不能直接改注释，但可在文档里加注。

**最小修复**：在引用块后加一句「（源注释指向 §4.6；兜底判定见 §3.5、§4.7）」，或同步修正源码注释的节号。

**置信度**：高。

**是否需源码核实**：否（文档结构即可判定）；若要改源码注释才需要。

**若我错了**：如果 §4.6 在作者的分节意图里包含兜底（例如「QemuVirtDesc 即兜底实现」），那么至少应在 §4.6 开头补一句「兜底判定见 §3.5」，让指针可达。

**命中清单**：C2 术语/指代；A4 交叉引用；G4。

## SUGGESTION RD4B-READER-C-06

**位置**：§4.1 关于 `Frozen::get` 断言的括号注（行 1569）。

**摘录**：「运行时断言文本是 `"Frozen::get before freeze"`（`Frozen::get` 的断言；文档注释里写作 "frozen before freeze"）」。

**严重度**：P3（措辞/证据偏好）。

**问题**：括号说「文档注释里写作 'frozen before freeze'」，但 §3.7 引用的 `Frozen` 文档注释（行 1406–1414）只有 Safety contract 两条，没有这句话；读者无法定位这个短语，也不清楚它是要说明断言文本与注释措辞的差异，还是笔误。

**影响**：在一条诊断关键句（panic 文本）上留下无法复核的引用；读者可能怀疑断言文本本身记错，降低对 §6.1 诊断表的信任。

**证据**：§4.1 行 1569 的括号；§3.7 行 1406–1414 的引文不含该短语；§6.1 行 2583 只重复断言文本。文档内找不到「frozen before freeze」的出处。

**最小修复**：把括号改成直接引用注释原句（若存在），或删掉「文档注释里写作…」；若确实存在这句话，在 §3.7 的引文里补出。

**置信度**：高（当前引文内不存在）。

**是否需源码核实**：是。需要看 `os/libs/minix-types/src/types/cell.rs:fn get` 的文档注释，确认是否有 "frozen before freeze" 及其上下文。

**若我错了**：如果文档注释确实写了这句话而 §3.7 只是节选漏掉，那么补进 §3.7 的引文即可；本条降级为「引文不全」。

**命中清单**：B1 摘录忠实；C2 术语/指代；G6。

---

## 汇总

- 条目：A 面板 7 条（A-01–A-07），B 面板 5 条（B-01–B-05），C 面板 6 条（C-01–C-06）；共 18 条，其中 `FINDING` 14 条、`SUGGESTION` 4 条（C-02、C-03、C-05、C-06）。
- 严重度：P2 共 9 条（A-01、A-02、A-03、A-04、A-07、B-01、B-02、B-04、C-01），P3 共 9 条；无 P1/P0。
- 最集中的三类问题：①「口径/枚举不闭合」（A-01、A-03、A-07、B-03）；②「文档原则与自己的摘录冲突」（B-01、B-02、B-04）；③「已知差异缺少单位/默认值说明」（A-02、A-04、C-01）。
- 证据边界：按任务要求只读本篇与读者画像；所有证据均只引本篇行号与摘录。标「需源码核实」的 11 条不构成源码结论，只表示文档内无法闭合。
- 同 agent 局限：A/B/C 由同一 agent 顺序走完，未做跨面板独立复核；置信度中的「中高/中等」已按此折扣。读取范围为只读，未修改文档。

