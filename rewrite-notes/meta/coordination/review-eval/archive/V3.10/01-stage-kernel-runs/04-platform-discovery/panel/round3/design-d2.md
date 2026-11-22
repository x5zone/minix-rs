# RD04 设计审查 · 第二遍（D2）— 04-platform-discovery

> 审查者：独立设计审查（P-DESIGN 第二遍；D1 停跑见 D-024，本遍不承接 D1 的任何未验证断言）。
> 基线：文档主张（§3/§4）↔ 源码事实。方法：批量 grep 关键类型与方法，文档给出的每条可数口径全部复算，C 侧引文抽样回读 `minix3/` 原文。
> 维度：状态表达 / 身份 / 所有权 / 错误 / 并发 / 抽象边界 / 可测试性；另查翻译残留、死抽象、非法状态、最小化机会。

## DIFF-TABLE（文档主张 ↔ 源码事实）

| # | 文档主张（位置） | 源码证据（path:line） | 判定 |
|---|---|---|---|
| 1 | §3.2 `PlatformDesc` 八方法（6 必需 + 2 默认 `sswi_setip_base`/`psci_conduit`） | `os/libs/minix-boot/src/platform.rs:350-385`，默认实现 :374/:382 | 一致 |
| 2 | §3.2 `global.rs` 变体转发 25 条（22 转发 + 3 Debug，不含 `_ => None` 兜底臂） | `rg -o 'Self::(DeviceTree\|Acpi\|QemuVirt)' os/libs/minix-platform/src/global.rs \| wc -l` = 25；`psci_conduit` 确非三分支（global.rs:196-202） | 一致 |
| 3 | §3.3 三个子 trait 带 `Any`；品牌 struct 在 `arch/`；`as_any` 无默认实现的理由 | platform.rs:415-424/434-438/448-451；arch/{x86_64,aarch64,riscv64}.rs 字段与 §3.3 表逐字段相符 | 一致 |
| 4 | §3.5 兜底三份 QemuVirt 常量表（0xFEE0_0000 / 0x0C00_0000 / 0x0800_0000 等） | arch/x86_64.rs:93-101、arch/riscv64.rs:140-166、arch/aarch64.rs 同形 | 一致 |
| 5 | §3.5 dev 兜底什么都不打印，`let _ = reason;` | global.rs:275-288 | 一致（问题见 D2-05） |
| 6 | §3.7 `Frozen` 契约：`debug_assert!(!frozen…, "Frozen::freeze called twice")`；`get` panic 文本 `"Frozen::get before freeze"`；`AssumeSyncCell` 同文件仍在 | minix-types/src/types/cell.rs:227-231、:247-256、:51-112 | 一致（检查是 dev-only，见 D2-04） |
| 7 | §4.1 Phase A.5 在 `init_protection` 之前；`init_protection` 是无条件读取点 | kernel/src/lib.rs:1752-1758（A.5）、:2246-2258（读 topo） | 一致 |
| 8 | §4.2 x86-64 单循环 `ACPI2_GUID \|\| ACPI_GUID` + break；aarch64 两段独立循环（DTB 先） | boot-shim/src/uefi_helpers.rs:216-227（单循环）、:233-257（两段） | 一致 |
| 9 | §4.3 `parse_by_kind` 臂与 cfg；kind.rs 4 行计数（2 注释 + 2 构造） | minix-platform/src/kind.rs:49-75、:39-40 | 一致 |
| 10 | §4.4 RSDP 20 字节校验和保持 C 口径、扩展校验和不校验；rev 2 走 XSDT；`MAX_ROOT_ENTRIES = 35`；MADT 44/64 KiB 双闸；GIC 决策表八条 | acpi.rs:192-199（20 字节，注释写明对齐 C）、:208-234、:61、:66、:736-750 | 一致 |
| 11 | §4.4 C 侧"声明十一个、处理两个"MADT 类型 | `grep -c 'define ACPI_MADT_TYPE' acpi.h` = 11；acpi.c 只出现 TYPE_IOAPIC(:358) 与 TYPE_LAPIC(:382) | 一致 |
| 12 | §4.5 eager parsing；`all_nodes` 过滤代替 `Fdt::cpus()`；CLINT `+0xBFF8/+0x4000`；先 CLINT 再 ACLINT | device_tree.rs:445-505（过滤注释同文）、:327-340、:295（or_else） | 一致 |
| 13 | §4.5 窗口缺口（ACPI 表链不在注册窗口，2 MiB 取整侥幸）如实写为 R04-T13 | 文档 §4.5 明言"不是契约"；未发现把它写成已解决的表述 | 一致 |
| 14 | §4.6 三份 per-arch `QemuVirtDesc`；`new()` 只被 `default()` 内部调用；`Default` 用局部常量 | arch/riscv64.rs:140-166（局部 `MTIMECMP_*` 同文）；`new()` 无其他调用点 | 一致 |
| 15 | §4.7 `init_from_kinfo` 逐源尝试、失败静默续源；`None`→hvc | global.rs:241-268；kernel/src/smp.rs:1289-1290 | 一致 |
| 16 | §4.8 三构造点；aarch64 先判零再翻译；`mmio_translate` 常量断言；x86 `init_lapic` check-and-reject 断言原文 | plat/arm64/interrupt.rs:241-262；arch/aarch64.rs:48-54；plat/x86_64/interrupt.rs init_lapic（`IA32_APIC_BASE ... != descriptor LAPIC base`） | 一致 |
| 17 | §4.9 `init_smp_state` 回退 `topo.bsp_id`、`init_protection` 回退 0 的已知差异（R04-T12） | lib.rs:2249 附近（`unwrap_or(topo.bsp_id as usize)`）与 :2246-2258（`unwrap_or(0)`） | 一致（已登记，维持） |
| 18 | §4.9/§4.10 GET_MACHINE 六字段填四、`padding`/`board_id` 恒 0 | kernel/src/misc.rs:1120-1141 + `MachineStruct` | 一致 |
| 19 | §1.5/§3.8 早期控制台"描述符有项、无活消费者" | `early_console()` 调用点仅 global.rs 转发与一处 `let _ =` 测试绑定 | 一致（但死链被保留，见 D2-01） |
| 20 | §3.4 riscv64 CLINT 字段"只搬入、无 MMIO 消费者"，走 `time` CSR 与 SBI TIME ecall | arch/riscv64/clock.rs:23-55；`mtimecmp_addr` 全仓无读者 | 一致（`gicr_base` 同样死，文档未提，见 D2-02） |
| 21 | §2.6/R04-T10 单 IOAPIC；`IOAPIC_NUM_PINS = 24`、`NR_IRQ_VECTORS = 64` | plat/x86_64/interrupt.rs:217、:250；ApicDesc 注释明言"single IOAPIC" | 一致 |
| 22 | §5.1/§5.3/§6.2 可数口径：26 测试、54 处 `&[]`、C 三调用点、410 行、`local_apic` 单行声明 | 全部按文档 §6.2 命令复算，逐条命中 | 一致 |
| 23 | （文档未提）`init_clock_and_interrupts` 的 BSP SGI 门用固件 hint 而非选举结果 | kernel/src/lib.rs:2513 vs :1923-1955 | 偏差 → D2-03 |
| 24 | （文档未提）`CpuInfo.gicr_base` 无读者 | 全仓 grep：仅 device_tree.rs:481、acpi.rs:688 写入 | 偏差 → D2-02 |
| 25 | §3.6 否决 enum 的理由"格式名进 handoff 公共 API" | `pub const DTB/RSDP` 就在同一公共 API（platform.rs:97/:104） | 理由不成立 → D2-07 |

对账结论：可数口径（25/26/54/410/11 等）与代码引文全部复算通过，C 侧行号抽查（acpi.c、memory.c:815-825/850-856/950-958、`platform_tbl_checksum_ok(buff, 20)`）全部命中。文档与代码的对账质量很高；问题集中在文档自己声明为"统一/干净"的边界上有四处没对齐（D2-01/02/07/08），外加一处 BSP 身份分叉（D2-03）与两处协议强度问题（D2-04/05）。

## FINDING RD04-DESIGN-D2-01（P2）

- 位置：`os/libs/minix-boot/src/platform.rs:356`（trait 方法）、:448-451（`ConsoleDesc`）、`os/libs/minix-platform/src/arch/x86_64.rs:59-69`（`IsaSerialDesc`）、`os/plat/src/x86_64/early_console.rs:18`、`os/plat/src/arm64/early_console.rs:11`
- 严重度：P2
- 分类：`[DEL]`
- 问题：整条早期控制台描述链是死抽象——`PlatformDesc::early_console()`、`ConsoleDesc`（唯一方法 `as_any`，零能力方法）、三个 per-arch console 结构体，全链无活消费者；描述符里的 `0x3F8`/`0x0900_0000` 与 plat 层自有常量（`COM1_BASE`、`PL011_PA_BASE`）重复维护。
- 影响：三份解析器各写一个永不读取的分支；文档 §3.8 判据表专门为它写了一条"有协议但不读"的例外说明，读者负担持续存在；两处 0x3F8/0x0900_0000 常量将来漂移无报警。
- 证据或推理：`grep -rn 'early_console()' os/` 的调用点只有 global.rs:69-70/154-157（转发）与 global.rs:319（测试里 `let _ =` 绑定）；`ConsoleDesc` 在 minix-platform/minix-boot 之外零引用；plat 两侧早期控制台各有独立常量（path 见位置栏）。
- 最小修复：删 `early_console()` trait 方法与三个 console 结构体/字段；若想保留"将来从 SPCR/DTB 接管早期控制台"的意图，登记 TODO 而不是保留死代码。
- 置信度：高
- 是否需源码核实：否（已 grep 全仓）
- 若我错了：若后续篇（05）马上要接上 `early_console()` 消费者，则保留可辩护——但现状是"文档承认无消费者仍保留"，至少应降级为显式 TODO。
- 处置建议：接受

## FINDING RD04-DESIGN-D2-02（P2）

- 位置：`os/libs/minix-boot/src/platform.rs:302-307`（`CpuInfo.gicr_base`/`mtimecmp_addr`）、`os/libs/minix-platform/src/device_tree.rs:481/:488-498`、`os/libs/minix-platform/src/acpi.rs:688`、`os/plat/src/arm64/interrupt.rs:127`
- 严重度：P2
- 分类：`[DEL]`
- 问题：`CpuInfo` 的两个 per-CPU 私有地址字段只写不读。消费者自行推导同一值：aarch64 用实例 `gicr_base + hw_id * gicr_stride`（interrupt.rs:127），riscv64 的 S 模式定时器走 SBI（mtimecmp 无 MMIO 消费者，文档 §3.4/§4.8 已如实登记一半）。且 `parse_cpu_topology` 在每个 CPU 节点内重跑 `parse_gic`/`parse_timer` 仅仅为了填这两个死字段。
- 影响：per-CPU 冗余数据 + 每 CPU 一次整树重解析（4 核 × 全树遍历）；更糟的是这两个字段是"错误温床"——任何人将来读它们会拿到与控制器实例推导值可能不一致的第二份真相。
- 证据或推理：全仓 grep `\.gicr_base` / `mtimecmp_addr`，除写入点与单测断言（arch/riscv64.rs:225-227、platform.rs:526）外无读者；`enable_sgi(hw_id)` 路径（irq_manager.rs:396-404）用的是控制器实例推导，不是 `CpuInfo.gicr_base`。
- 最小修复：删 `CpuInfo.gicr_base` 与 `CpuInfo.mtimecmp_addr`（连同 `parse_cpu_topology` 内的 per-CPU 重解析与 `QemuVirtDesc::default` 的填充循环），`CpuInfo` 只留 `hw_id`。
- 置信度：高
- 是否需源码核实：否（已 grep 全仓）
- 若我错了：若 16-smp 篇的 AP 拉起计划直接消费 `CpuInfo.mtimecmp_addr`，则它是"已交付的接缝"——但文档 §3.4 说 riscv64 定时器编程走 SBI，与该字段的存在自相矛盾。
- 处置建议：接受

## FINDING RD04-DESIGN-D2-03（P2）

- 位置：`os/kernel/src/lib.rs:2513`（对照 `os/kernel/src/lib.rs:1923-1955`、`os/kernel/src/irq_manager.rs:396-404`）
- 严重度：P2
- 分类：`[MODERN]`
- 问题：BSP 身份判定在同一份启动序列里分叉：`init_smp_state` 与 `init_protection` 用 `elected_bsp_hw_id`（选举真相），而 `init_clock_and_interrupts` 里 `enable_sched_sgi_for_cpu(pd.cpu_topology().bsp_id as usize)` 用固件第一条 enabled 记录的 hw_id。文档 §4.9 自己论证"固件 `bsp_id` 是 hint 不是真相（OpenSBI 曾从 hart 1/2 引导）"，这一处却把 hint 当真值。
- 影响：选举结果与首条记录不一致时，调度 SGI 的 set-enable 位落在错误 CPU 的 redistributor 帧（`enable_sgi(hw_id)` 按 hw_id 找帧，interrupt.rs:127），真实 BSP 收不到调度 IPI，系统不报错。测试环境（AAVMF 从 CPU 0 引导、MADT 首条即 CPU 0）探不到。
- 证据或推理：lib.rs:2513 直读 `cpu_topology().bsp_id`；同文件 :1923-1955 的选举函数在 Phase B 已可用（`init_protection` 就在用）；`elected_bsp_hw_id` 失败时回退值恰是 `topo.bsp_id`，改用它严格不劣于现状。
- 最小修复：lib.rs:2513 改为 `enable_sched_sgi_for_cpu(elected_bsp_hw_id(&topo) as usize)`。
- 置信度：高
- 是否需源码核实：部分——调用点与选举函数已核；aarch64 选举 cell 值与 `hw_id`（MPIDR.Aff0 域）的等价性未逐行核（`elected_bsp_hw_id` 的 `e - 1` 语义依赖该等价，若不等价则本条回退路径本就不对，问题更大而非更小）。
- 若我错了：若此处刻意用 hint（例如"SGI 门必须开在固件认定的 BSP 帧"有某条架构理由），需要把理由写进代码注释——现在注释只说"clamp 期间只有 BSP 能收"，没有解释为何不用选举值。
- 处置建议：接受

## FINDING RD04-DESIGN-D2-04（P3）

- 位置：`os/libs/minix-types/src/types/cell.rs:227-231`
- 严重度：P3
- 分类：`[MODERN]`
- 问题：`Frozen::freeze` 的防重闸是 `debug_assert!`——发布构建下二次冻结会静默覆盖 `MaybeUninit` 值，而已有读者持 `&T` 并发读，构成数据竞争 UB。该类型的存在理由（文档 §3.7、global.rs:32-37"把 C 的纯约定变成受检协议"）恰是这道检查，检查却是 dev-only。
- 影响：保护整个平台描述符全局的最后一道类型级防线在发布构建缺席；C 的"发现跑两次"事故形态（§2.7"清单是旧的、开关关了"）在 Rust 发布构建退化为 UB 而非 panic。
- 证据或推理：cell.rs:228-231 是 `debug_assert!`；文档 §3.7 明言"保证'只冻结一次'的最终依据是启动顺序"。
- 最小修复：改 `assert!`（启动期一次性分支，代价可忽略）；若要彻底，占位用 `compare_exchange` 使并发双冻结也只成功一次。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若发布构建对这条 assert 的体积/时间有硬约束（未见证据），至少应在 `get` 侧补一条 release 检查或在文档把"release 下重复冻结 = UB"写成显式契约。
- 处置建议：接受

## FINDING RD04-DESIGN-D2-05（P3）

- 位置：`os/libs/minix-platform/src/global.rs:253-258`（`Err(_e)` 丢弃）、`os/libs/minix-platform/src/kind.rs:58/:70`（`DtbParse`/`AcpiParse` 折叠底层细节）、global.rs:275-288（兜底丢 `reason`）
- 严重度：P3
- 分类：`[MODERN]`
- 问题：失败信息全链丢弃。C 侧 `acpi_init` 失败至少 `printf("WARNING : Cannot configure ACPI")`；Rust 侧 dev 兜底 `let _ = reason;`，逐源尝试把每个来源的具体解析错误（`DtbParse`/`AcpiParse` 及其底层 `AcpiParseError`）全部吞掉，`qemu_fallback_or_panic` 的 panic 文本只有"no platform source parsed successfully"。文档 §6.1 辩护"控制台未就绪"，成立的是"此刻不能打印"，不成立的是"永久丢弃"——存起来与打印是两件事。
- 影响：§6.1 诊断表第一行只能判"走了兜底"，无法判"为什么走"（空切片？DTB 坏？ACPI 坏？哪个来源坏？）；真机上排障少一半线索。
- 证据或推理：global.rs:253-258 注释自承"(In dev builds, log the failure…)"而实际无任何动作；`PlatformContext` 只有一个 `desc` 字段（global.rs:57-60），无处安放原因。
- 最小修复：`PlatformContext` 增加 `fallback_reason: Option<&'static str>`（及可选 `last_parse_error`），console 就绪后由诊断路径读取；panic 文本拼入最后一个来源错误。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若认为 `source() == QemuVirt` 已足够诊断，则至少 `init_from_kinfo` 的注释不该写"log the failure"这种与行为相反的话（文档 §4.7 已指出此矛盾，代码未改）。
- 处置建议：接受

## FINDING RD04-DESIGN-D2-06（P3）

- 位置：`os/libs/minix-boot/src/platform.rs:266-282`（`MAX_CPUS`/`CpuTopology`）
- 严重度：P3
- 分类：`[MODERN]`
- 问题：`CpuTopology` 的两条不变量——"`cpus[0..nr_cpus]` 才有效"与"`nr_cpus ≤ MAX_CPUS = 64`"——只是文档注释；struct 是 pub 字段，`nr_cpus: u32` 可以构造出 100 这样越界组合。消费侧 `smp.rs:1126-1128` 按 `0..topo.nr_cpus` 索引 `cpus[i]`，越界即 panic。当前解析器封顶（device_tree.rs 循环 `break at MAX_CPUS`、acpi.rs `copy_count = min(nr_cpus, MAX_CPUS)`），故现行不可达，属"非法状态可表示"而非可达缺陷。
- 影响：下一个直接构造 `CpuTopology` 的调用点（测试、新解析器）没有编译期/调试期防线。
- 证据或推理：platform.rs:270-282 无受检构造；`Default` 给 `nr_cpus: 1` 与解析器封顶并存，说明界内界外两种口径都靠约定。
- 最小修复：提供受检构造 `CpuTopology::new(&[CpuInfo]) -> Self`（内部 clamp 到 `MAX_CPUS` 并把字段私有化），或至少加一条 `debug_assert!(nr_cpus <= MAX_CPUS)`。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若三份解析器被认定是该类型的唯一生产者且测试覆盖足够，此条可降级为风格项。
- 处置建议：接受（低优先）

## FINDING RD04-DESIGN-D2-07（P3）

- 位置：文档 §3.6 否决表（"enum（直接列出 `Dtb`/`Rsdp` 变体）……把固件格式名写进 handoff 层公共 API"）对照 `os/libs/minix-boot/src/platform.rs:92-104`、:253-261
- 严重度：P3
- 分类：`[DEL]`
- 问题：否决理由与 API 事实不符：采用方案在同一公共 API 里就有 `pub const DTB`、`pub const RSDP` 与 `PlatformSource::{DeviceTree, Acpi}`——固件格式名同样可见。真正的差异是：boot-shim 不依赖 minix-platform 即可构造来源（platform.rs:80-84 注释）、分发集中在 `parse_by_kind` 一处（kind.rs:49-75）。同文件模块注释 "Brand names like Apic, Gicv3, Plic, DeviceTree, Acpi never appear here"（platform.rs:4-6）也因 `DeviceTree`/`Acpi` 字面量就在本文件而不成立，其真实射程只是控制器品牌名（Apic/Gicv3/Plic）。
- 影响：读者按文档理解会得出"handoff 层看不见格式名"的错误结论；§3.6 的方案对比表传递了错误的判据。
- 证据或推理：platform.rs:97/:104/:253-261；`PlatformSource` 枚举变体名即格式名。
- 最小修复：改写 §3.6 该格理由（改为"boot-shim 免依赖 + 分发单点"），并把模块注释的宣称收窄到"控制器/定时器品牌名"。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若"格式名不可见"指的是"上层不能对格式名做 match 分支"（语义不可见而非名字不可见），文档应把这句话写全——现在的表述按字面读为假。
- 处置建议：接受

## FINDING RD04-DESIGN-D2-08（P3）

- 位置：`os/libs/minix-boot/src/platform.rs:326-332`（`ArchMiscDesc`）、文档 §3.3 与 §4.6
- 严重度：P3
- 分类：`[DEL]`
- 问题：`ArchMiscDesc` 是文档自己反对的"三架构字段并集"形态：x86-64 专属 `acpi_tables` 与 ARM64 专属 `pmu_cycle_counter` 合在 handoff 层的一个 struct，经 `arch_misc()` 直接穿过统一接口——与 §4.6 否掉"单文件 + 方法体 `#[cfg]`"时给出的理由（"字段类型退化为三架构字段的并集"）同形，与 §3.3"品牌/架构细节退到 arch 模块"的原则相抵。三份 `QemuVirtDesc`/三个解析器都要为"别家的字段"填默认值（acpi.rs:245 等四处 `pmu_cycle_counter: false`）。
- 影响：同一条原则在 trait 化的子描述符上执行、在 `ArchMiscDesc` 上豁免，判据不明；x86_64 `X86_64ArchInit` 的实例 `acpi_tables` 字段标 `#[allow(dead_code)]`（文档 §4.8 已登记）是这条并集的直接产物。
- 证据或推理：platform.rs:326-332（注释明写 "x86-64:" / "ARM64:"）；arch/x86_64.rs `X86_64ArchInit` 收字段标 `#[allow(dead_code)]`；活消费者仅两处：getinfo_machine 读 `acpi_tables`（misc.rs:1131-1134）、arm64 arch_init 读 `pmu_cycle_counter`（arch/arm64/arch_init.rs:61）。
- 最小修复：二选一——(a) 文档承认例外并写明判据（"跨架构广播型杂项允许并集"）；(b) 按 §3.3 的 downcast 模式把 arch-misc 并进 per-arch 描述符。两向成本都不大。
- 置信度：高
- 是否需源码核实：否
- 若我错了：若 `ArchMiscDesc` 被视为"接口稳定优先于纯净"的有意取舍，那缺的是文档里的一句话，不是代码改动。
- 处置建议：需用户裁决

## FINDING RD04-DESIGN-D2-09（P3）

- 位置：`os/libs/minix-platform/src/device_tree.rs`（`parse_cpu_topology` 尾部 `if nr_cpus == 0 { return CpuTopology::default(); }`）
- 严重度：P3
- 分类：`[MODERN]`
- 问题：DTB 无 CPU 节点时静默伪造"1 核、hw_id = 0"的幻影拓扑，而不是报错。这与 §3.5"Rust 把 C 的静默降级改成显式分支"的自述相抵：来源层做了显式化，字段层没有。下游有防御（irq_manager.rs:287-297 的越界回落、`init_smp_state` 的 `max(1)`），但幻影 `hw_id = 0` 会让 BSP 匹配逻辑选中一个不存在的核。
- 影响：触发面是畸形 DTB（实测环境不可达）；一旦触发，后续 SMP 状态建在假拓扑上，诊断被引向"SMP 初始化"而非"DTB 缺 CPU 节点"。
- 证据或推理：函数尾部直接返回 `Default`（`nr_cpus: 1, bsp_id: 0, cpus` 全零——`hw_id: 0` 是编造值，不是读到的值）。
- 最小修复：把"零 CPU 节点"变成 `Err(DtParseError)`（与 `parse_plic` 对缺节点的处理一致）；或保持现状但在文档 §4.5 表里把"缺省"改写为"伪造单核（显式降级）"。
- 置信度：中高
- 是否需源码核实：否
- 若我错了：若"最小 DTB 也该能启动"是刻意目标（如 unit-test 夹具），现行为可辩护，但应由测试显式依赖它，而非解析器静默兜底。
- 处置建议：需用户裁决

## ARCH-PROPOSAL RD04-A1

- 位置：`os/kernel/src/smp.rs:1289-1290`（消费端）、`os/libs/minix-platform/src/acpi.rs`（`AcpiDesc` 不解析 FADT）、文档 §4.7
- 提案：aarch64 ACPI 腿的 PSCI conduit 发现缺口。`psci_conduit()` 只有 DTB 腿返回值，ACPI 腿恒 `None`，消费端把 `None`（连同 `Hvc`）一律按 hvc 处理。ACPI 规范本身在 FADT 的 `ARM_BOOT_ARCH` flags（bit 0 = PSCI compliant，bit 1 = PSCI use HVC）里报告通道；真机 SBBR 固件（TF-A 常驻 EL3）通常走 smc，此缺口在真 ACPI 硬件上会让 CPU_ON 走错通道。QEMU + AAVMF 的 conduit 恰是 hvc，现有真机门探不到。
- 为何归 ARCH：这改变"平台发现覆盖什么"——新增一个 ACPI 报告项（FADT 字段），属于对发现链覆盖面的扩展，不是现机制内的修复。
- 最小实现：`AcpiDesc::parse` 增加 FADT 定位（XSDT 表项签名 `"FACP"`）与 +offset flags 读取（一次偏移读，不解析 AML），映射到 `PsciConduit`；无 FADT 或 flags 未置 compliant 时维持 `None`。
- 备选（不写码）：在文档 §4.7 与 §6.1 把"aarch64 ACPI 腿 = hvc only"写成显式限制并登记 TODO，与 R04-T09/T11 同列。
- 置信度：中（FADT 字段偏移与 flags 语义需按 ACPI 规范核对；本仓现无真机 ACPI aarch64 载体）
- 是否需用户裁决：是——扩覆盖面还是先登记限制，取决于项目对真机 SBBR 的优先级。

## VERDICT

- 总体判定：**基本通过（有条件）**。文档主张与源码事实的对账质量很高：全部可数口径复算通过，代码引文与 C 侧行号抽查全部命中，已知缺口（R04-T09/T10/T11/T12/T13）的登记与定性准确。
- 无 P1。两条 P2 是死抽象（D2-01 控制台链、D2-02 per-CPU 私有地址字段），一条 P2 是 BSP 身份分叉（D2-03，测试环境不可达的真值不一致）。三条 P3 是协议强度与可诊断性（D2-04/05/06），两条 P3 是文档自述与代码形态的矛盾（D2-07/08），一条 P3 是静默伪造（D2-09）。
- 纪律自查：未把"trait 只为 downcast 服务"当问题（`InterruptControllerDesc`/`TimerDesc` 的 `nr_irqs`/`frequency` 是真实接口，三个实现非单实现）；未把 `pmu_cycle_counter: bool` 要求改 enum（它有活消费者且无非此 bool 的非法状态）；D2-01 的判定依据是零消费者而非 trait 形单薄。
- 建议的处置批次：D2-01/02（删死代码）与 D2-03（一行改选举值）可作一批低成本修复；D2-04/05/06 跟随其后；D2-07/08 是文档修订；D2-08 与 A1 需用户裁决方向。
- 与已登记 TODO 的关系：D2-02 与 R04-T09/T10 无冲突（多 IOAPIC 模型仍要 per-CPU 基址时可在控制器实例推导，不必回填 `CpuInfo`）；A1 建议与 R04-T11 并列登记。

- 本次扫描：9 条 FINDING，1 条 ARCH-PROPOSAL。
