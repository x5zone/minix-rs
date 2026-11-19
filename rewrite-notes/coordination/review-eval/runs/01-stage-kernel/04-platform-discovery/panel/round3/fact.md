# P-FACT 独立事实审查 — 04-platform-discovery.md（round3）

审查者：P-FACT。方法：对文档 2735 行逐节抽取可核对断言（常量、签名、结构体字段、错误码、引文、测试计数、行号锚点、反事实前提），批量 grep/sed 对照 `minix3/`（C 真值）与 `os/`（Rust 实现）后逐条核对。全部为只读操作。

**核对面覆盖（均无误，抽样列举）**：
- C 侧：`acpi_phys2vir`/`get_acpi_rsdp`/`acpi_rsdp_test`/`acpi_init`/`acpi_read_sdt_at`/`acpi_madt_get_typed_item`/`acpi_get_lapic_next`/`acpi_get_ioapics`/`detect_ioapics`/`apic_single_cpu_init`/`arch_init`/`init_clock`/`intr_init`/`discover_cpus`/`smp_init`/`lapic_enable_in_msr` 的 `#if 0` 块 全部逐字吻合；`struct acpi_rsdp`/`struct machine`（type.h:122-131）/`acpi_madt_hdr`/`board.h` 位域注释逐字吻合；`LOCAL_APIC_DEF_ADDR 0xfee00000`（apic.h:59）、`TIMER_FREQ 1193182`（i386/arch_clock.c:32）、SDT 头 36 字节与 176 容量、`MAX_RSDT 35`、`(-1-36)/4 = 1073741814` 的无符号提升演算、`acpi_read_sdt_at` 六处 `return -1`、MADT 类型 11 个 define、earm 的 `cpu_identify`（freq=660）/`bsp_init`/`intr_init`/`tsc_per_ms` 16250/15000/panic 文本/短名表 `BBXM,A335BONE,A335BNLT`/`POORMANS_FAILURE_NOTIFICATION = svc #00`/omap_timer `0xffffffff-(32768/freq)+1` 全部吻合；行号锚点 acpi.c:325、apic.c:1141、arch_system.c:264、arch_smp.c:315、memory.c:819/852/954、pci.c:917、main.c:443-468/304-307、acpi.h:99（`rg local_apic` 仅此一行）逐一复现。
- Rust 侧：`PlatformDesc` 8 方法（2 个默认 `None`）、`PlatformDescKind`/DTB=1/RSDP=2、`PlatformDescSource`（u32 kind/len + PhysBytes u64，`#[repr(transparent)]`）、`Frozen` 结构与两条断言文本（`"Frozen::freeze called twice"` debug_assert；运行时 panic `"Frozen::get before freeze"`，Panics 注释原文 `"frozen before freeze"`）、`Self::…` 25 条（含兜底臂 26）、`kind.rs:57/69`、`rg -c` global.rs:2/kind.rs:4、`check_gic_madt` 逐字＋决策表 8 断言、`AcpiParseError` 变体清单、MADT 常量 0/1/9/11/12/14、`MAX_ROOT_ENTRIES=35`/`MAX_TABLE_LEN=64KiB`/44 下限、GICR 推导 `gicr_base+hw_id*0x2_0000`（acpi.rs:688）、三份 QemuVirtDesc 全部数值、riscv64 `Default` 逐字、`CpuInfo::zero` 注释、`MAX_CPUS=64`、`from_fdt`/模块文档两段逐字、CLINT 偏移 0xBFF8/0x4000、`parse_clint().or_else(parse_aclint_mtimer)`、`parse_plic` ndev 缺省 64、`context=1` 硬编码、`parse_by_kind` 逐字、`init_from_kinfo`/`qemu_fallback_or_panic`/`PLATFORM`+`init`+`platform_desc` 逐字、psci 转发仅 DeviceTree 臂（global.rs）、`init_from_kinfo` Phase A.5 逐字（lib.rs:1752-1761）、`init_protection` 读拓扑＋`unwrap_or(0)`、`init_smp_state` 代码行、`MachineStruct`/`machine_record`（文档已自注"示意"）、sched `MachineInfo` 逐字、`TIMER_IRQ` 逐字、`IOAPIC_NUM_PINS=24`、`NR_IRQ_VECTORS=64`（plat:250；C interrupt.h:35/37 条件 16/64）、pic_init 0x50/0x70（boot_init_timer 末尾 lib.rs:3802）、`mmio_translate` 断言文本与窗口 `0x0800_0000..0x0A00_0000`（MMIO_PA_BASE/WINDOW_LEN=0x0200_0000）、aarch64 `new()` 三断言、`"Riscv64ClockArch::new: expected ClintDesc"`、`elected_bsp_hw_id` 回退 `topo.bsp_id`、GUID 两值逐字、`ebs_pair.rs:193/194`、`uefi_helpers.rs:1188`（5 条 panic）、`handoff_v1_write.rs:327-334`、`lib.rs:711/725`、`push_platform_windows` len=0→2 MiB、run_all.sh 六行、三变体横幅/标签/TEST_RESULT、`write_hex` 固定 16 位、`#[test]` 26 条逐文件 9/4/4/3/4/1/1、x86_64 宿主 18 条（cfg 门逐条核实）、13 个具名测试全部存在、`&[]` 54 条及分布（10/9/6/3/…/21）、uefi_helpers 12 条测试 1+5+6 分解、10 个调用 `find_platform_sources_into` 的测试内核、`collect_arrays`（minix-boot/src/memmap_source.rs:980）、`resv_claims.rs:97`。反事实前提（去签名闸→任意 20 字节零和位置当 RSDP；零长记录死循环；GICv2 GICV 帧误当 GICR；挪 `init_from_kinfo` 撞 `init_protection` 的 Frozen 断言；`Frozen` 二次 freeze debug 断言/发布覆盖）逐条对到源码成立。

## FINDING RD04-FACT-01（P2）
- 位置：rewrite-notes/01-stage-kernel/04-platform-discovery.md §5.2 表格 riscv64 行（约 2472 行）；os/qemu-tests/test-kernels/kernel/bootstrap/test-smp-topo-riscv64/src/main.rs:110
- 严重度：P2（测试描述与源码不符，属 P0-test 描述/代码对账类的事实错）
- 问题：文档写该变体的解析路径为 `PlatformDescSource::with_optional_len(DTB, ...)`；源码实际是 `let source = PlatformDescSource::new(DTB, PhysBytes(dtb_phys));`（不带 len，len=0）。
- 影响：读者按文档复述该测试的交接语义会得出"riscv64 变体记录了 DTB 长度、窗口按精确长度建"的错误结论；实际该测试走 `new()`（len=0 → 窗口回退 2 MiB 片），与 §4.6/§4.5 的 len=0 口径正好构成它自己描述的反例。注意 boot-shim 生产腿（os/boot-shim/src/opensbi_helpers.rs:421 与 os/kernel-image/src/bootface_a64.rs:300）确实用 `with_optional_len`，故 §4.11 riscv64 ② 行无误——错误仅限 §5.2 这一行。
- 证据或推理：E1（sed 直读 main.rs:105-113；rg `with_optional_len` 全仓 7 处命中清单中不含该测试内核）。
- 最小修复：该行改为 `PlatformDescSource::new(DTB, ...)`，并可加半句"`new` 即 len=0，窗口按 2 MiB 片覆盖；boot-shim 生产腿才带 len"。
- 置信度：高。
- 是否需源码核实：已完成（本条即源码核实结论）。
- 若我错了：若仓库另有 riscv64 topo 变体使用 `with_optional_len` 的版本（rg 未发现），则以那一处为准改回；需重跑 `rg -n 'with_optional_len' os/qemu-tests/` 确认为空。

## FINDING RD04-FACT-02（P3）
- 位置：文档 §3.1 引文块（约 1077-1079 行）；os/libs/minix-boot/src/kernel_info.rs（`platform_sources` 字段的 Cross-binary safety 段）
- 严重度：P3（引文保真：标注为"节选"，但做了未标注的改序拼接）
- 问题：源注释语序是"Each `PlatformDescSource` is pure data `(u32 kind, u32 len, u64 phys_addr)` — safe to pass from boot-shim to kernel even when they are separate ELF binaries. No function pointers cross the boundary. `len = 0` means the producer did not record the blob extent; the window builder then falls back to one 2 MiB piece."；文档引文把 `len = 0 …2 MiB piece` 前移、用破折号拼进第一句，并把 "No function pointers cross the boundary" 移到段尾。
- 影响：三句内容无一字篡改，但拼接后"safe to pass"的挂靠对象从"pure data"变成了"len=0 回退"子句，引用核查时对不上行序；该块标注只写"省略号处为其余字段"，未声明改序。
- 证据或推理：E2（Read 源文件该字段注释并与文档引文逐句比对）。
- 最小修复：恢复源注释语序，或在节选标注中加"句序有重排"。
- 置信度：高。
- 是否需源码核实：已完成。
- 若我错了：若项目文风约定允许节选块内重排（未见于 review 规则），降为 SUG 级即可。

## FINDING RD04-FACT-03（P3）
- 位置：文档 §4.9 `init_smp_state` 引文块（约 2232-2234 行）；os/kernel/src/lib.rs:1966-1970
- 严重度：P3（引文内注释被改写，未用省略号标出）
- 问题：文档引文写注释 "// The DTB/ACPI `bsp_id` is a hint, not the truth: election/CPUID resolves the CPU actually executing this code."；源注释实为 "…is a hint, not the truth: OpenSBI may hand control to any hart (QEMU -smp 4 has booted hart 1 and hart 2), and the entry election cell records the real one. Resolve it here, before per-CPU state is seeded, so the BSP's READY flag, ptproc and clock state all land on the CPU that is actually running."（首句前半逐字，其后整段被压缩改写）。代码行本身逐字一致。
- 影响：同 FACT-02——节选标注（"第一个 unsafe 块"）未覆盖注释改写；读者 grep 引文原句会落空。
- 证据或推理：E2（lib.rs:1961-1985 与引文逐行比对）。
- 最小修复：改用省略号保留首句原文（"…not the truth: OpenSBI may hand control…"），或标注"注释为中文转述式压缩"（同 §4.5 对 `parse_cpu_topology` 的先例标注）。
- 置信度：高。
- 是否需源码核实：已完成。
- 若我错了：无——引文与源文件的差异是可复核的文本事实。

## FINDING RD04-FACT-04（P3）
- 位置：文档 §4.8 aarch64 `new()` 引文块（约 2148-2184 行）；os/plat/src/arm64/interrupt.rs `fn new`
- 严重度：P3（引文注释被整段译成中文，标注口径偏窄）
- 问题：引文标注写"注释有删改：行首的过程编号未保留"，但实际修改远不止删行首编号：源文件两段英文注释（"Check for zero before translating…"、"Route to the kernel higher-half (TTBR1) MMIO alias…"）在引文里被整体译写为中文。代码（三断言、`Self{}` 七行）逐字一致。
- 影响：同 FACT-02/03；"删改"的范围被低估，逐字核查者会误判引文不可信或可信边界。
- 证据或推理：E2（plat/src/arm64/interrupt.rs `fn new` 全文比对）。
- 最小修复：标注改为"注释为中文转述，非源码逐字；代码逐字"。
- 置信度：高。
- 是否需源码核实：已完成。
- 若我错了：无。

## FINDING RD04-FACT-05（P3）
- 位置：文档 §3.2（约 1154 行）；os/libs/minix-platform/src/global.rs（`PlatformDescEnum` 文档 "# Why not `Box<dyn PlatformDesc>`?" 段）
- 严重度：P3（转述被当作源注释的原文口径）
- 问题：文档括注称枚举文档把理由写成"（no_std——避免引导期依赖分配器；**enum 分派不需要为每个描述符做一次堆分配**）"；源注释第二条实为 "- Enum dispatch is zero-cost (no vtable indirection)."——源注释谈的是零开销/无 vtable 间接，未出现"每描述符堆分配"表述（那是文档自己在同节 vtable 段落的推论）。
- 影响：把文档推论伪装成源注释口径，削弱该段"理由写在注释里"的可核查性。
- 证据或推理：E2（global.rs 该段逐行比对）。
- 最小修复：括注后半改为"enum 分派零开销（不经 vtable）"，并把"省掉每描述符一次堆分配"明确归为正文推论。
- 置信度：高。
- 是否需源码核实：已完成。
- 若我错了：若视为概括而非引文，降为文风问题（但该括注紧跟"文档把这条理由写在…"的归属句式，读者会当原文）。

## FINDING RD04-FACT-06（P3）
- 位置：文档 §4.2（约 1644 行）："（MRB，记为 `MrbEntry`、映射到 `EfiReservedMemoryType`，`memmap_source.rs:1152`、`resv_claims.rs:97`）"
- 严重度：P3（锚点不可直接解析：两个裸文件名分属两个 crate，且都不在 related-code.txt 清单内）
- 问题：`memmap_source.rs` 只存在于 `os/libs/minix-boot/src/`（`collect_arrays` 在其 :980，MRB 循环在 :1152）；`resv_claims.rs` 只存在于 `os/boot-shim/src/`（:97 恰为 `SourceKind::ReservedNoMap | SourceKind::MrbEntry => ExpectedEfiType::Reserved`）。两处裸文件名均无路径前缀，`os/boot-shim/src/` 下没有 memmap_source.rs，按邻近 crate 猜会落空；也不满足 AGENTS.md"正式文档引用指向绝对路径"的约定。
- 影响：事实本身（MrbEntry→Reserved 映射、MRB 子集解析）经核对为真；只是复核命令/跳转成本被无谓抬高，机械门的 citation 检查可能按文件名短路径失配。
- 证据或推理：E2（ls 两 crate 的 src/、sed 两处行号、rg MrbEntry）。
- 最小修复：补全为 `os/libs/minix-boot/src/memmap_source.rs:1152` 与 `os/boot-shim/src/resv_claims.rs:97`。
- 置信度：高。
- 是否需源码核实：已完成。
- 若我错了：若项目允许同目录内裸文件名（此处恰跨 crate，不适用），则仅为风格项。

## SUGGESTION RD04-SUG-01
- 位置：文档 §5.2 riscv64 行 / §5.1 计数段
- 建议：随 RD04-FACT-01 修正该行构造器为 `PlatformDescSource::new(DTB, …)`，并在 §5.2 表下补一句三个变体构造器的差异说明（x86_64 `new(RSDP,…)`、aarch64 同、riscv64 `new(DTB,…)`，均 len=0），与 boot-shim 生产腿的 `with_optional_len` 区分开——目前 §4.11 ②列（with_optional_len）与 §5.2（实为 new）的矛盾正是本条要消除的。
- 依据：E1/E2，见 FACT-01。

## SUGGESTION RD04-SUG-02
- 位置：文档 §3.1 / §4.9 / §4.8 三处引文块
- 建议：统一"引文保真标注"口径：凡注释被转述/压缩/译写/改序的引文块，沿用 §4.5 `parse_cpu_topology` 的先例格式（"注释是中文转述，不是源码逐字"）逐一标注；纯省略用 `…`。三个待改点见 FACT-02/03/04。
- 依据：E2，三处源文件比对。

## SUGGESTION RD04-SUG-03
- 位置：文档 §3.2 括注与 §4.2 锚点
- 建议：§3.2 括注改回源注释原文（zero-cost / no vtable）；§4.2 两个裸文件名补全 crate 路径（FACT-05/06 的最小修复），便于机械门 citation 全绿。

- 本次扫描：6 条 FINDING，3 条 SUGGESTION。
