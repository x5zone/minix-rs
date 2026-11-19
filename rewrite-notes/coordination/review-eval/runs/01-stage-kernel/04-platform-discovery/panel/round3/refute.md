# 04-platform-discovery 反证（round3 · 反证者）

方法：通读文档一遍（2736 行）后，对 6 条核心主张逐条构造反例——反向执行路径、失败路径、架构分支、时序条件——用针对性 grep / sed 对照 C 真值源（`minix3/`）与 Rust 实现（`os/`）。只读文档与源码，未读作者解释与历史评审。证据一律给 path:line 或命令与输出。

---

## CLAIM RD04-CLAIM-01

**主张**：aarch64 的固件来源扫描是两段独立循环——先扫 DTB 两个 GUID（spec/legacy）、命中即停，再扫 ACPI 两个 GUID（§4.2 的表与反例段）；AAVMF 实测配置表里两个 DTB GUID 都不在，aarch64 的发现链因此实际走 ACPI（§1.2、§4.3、§5.2）。

**反证尝试**：找"单循环单判断"或"ACPI 先扫"的相反实现路径；找 U-Boot/直核腿把次序颠倒的分支。

**结果**：反证失败（主张成立）。

**证据**：
- `os/boot-shim/src/uefi_helpers.rs:234-246`（aarch64 第一段循环：`DEVICE_TREE_GUID || LEGACY_DEVICE_TREE_GUID`，命中 `break`）与 `:247-253`（第二段循环：`ACPI2_GUID || ACPI_GUID`）——两段独立 for、DTB 在前，与文档完全一致；x86-64 是 `:217-227` 的单循环 `||` + `break`，文档 §4.2 自己指出"prefer ACPI 2.0+"意图大于实现，代码核实为同一形状。
- AAVMF 无 DTB 的实测主张有代码侧佐证：`os/libs/minix-platform/src/kind.rs:64-66` 注释 "empirically verified: 8 entries, ACPI2 present, both DTB GUIDs absent"；`os/qemu-tests/test-kernels/kernel/bootstrap/test-smp-topo-aarch64/src/main.rs` 头注写明 "the discovery path is ACPI, not DTB on AAVMF"。
- riscv64 无 UEFI 扫描分支：`:259-263` 的 `#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]` 返回空，与 §4.2 表第三行一致。

**若我错了**：若某腿（如 `bootface_a64.rs` 直核）把 ACPI 放在 DTB 之前，次序主张不成立——查证该文件只装 DTB 单源（`PlatformDescSource::with_optional_len(DTB, …)`），无 ACPI 臂，反例不存在。

---

## CLAIM RD04-CLAIM-02

**主张**：C 侧 LAPIC 的物理基址唯一来源是常量 `LOCAL_APIC_DEF_ADDR`（0xfee00000），MADT 表头 `local_apic_address` 全仓只有声明、没有读者（§1.1、§2.5）；Rust 侧读进描述符并在消费端 check-and-reject（§2.5、§4.8、§6.1）。

**反证尝试**：(a) 把检索面从文档声明的 `arch/i386/` 扩到整个 `minix3/minix/kernel/`，找其他 arch 或 SMP 路径的读者；(b) 找不经字段名的旁路读取（指针算术读 MADT+36）；(c) 查 Rust 消费端是否真有断言。

**结果**：反证失败（主张成立）。

**证据**：
- `rg -n "local_apic" minix3/minix/kernel/` 全目录唯一命中 `minix3/minix/kernel/arch/i386/acpi.h:99`（`u32_t local_apic_address;` 声明），无读取点——比文档声称的范围更宽仍成立。
- `apic.c` 内活路径基址赋值仅 `lapic_addr = LOCAL_APIC_DEF_ADDR` 两处（单核 `apic_single_cpu_init`、SMP `smp_init`）；MSR 读地址段确在 `#if 0` 死代码里。
- Rust 消费端断言实存：`os/plat/src/x86_64/interrupt.rs:163`（"APIC base … != device-window LAPIC page — refusing"）与 `:318`（"!= descriptor LAPIC base … (relocated LAPIC unsupported; …)"）。

**若我错了**：`rg` 只能抓字段名，抓不到 `((u32_t*)madt)[9]` 式裸偏移读；通读 `acpi.c`/`apic.c` 未见此类写法，但这是检索方法的固有盲区，不是逻辑证明。

---

## CLAIM RD04-CLAIM-03

**主张**：`acpi_init()` 有且仅有三个调用点；`apic_single_cpu_init` 自己先调 `acpi_init()` 再 `detect_ioapics()`，因此对调 `arch_init` 里两步不产生"字段还是 0"的状态（§2.1、§2.7）；`config_no_apic` 缺省 1 使默认配置下 `acpi_init` 只跑 `arch_init` 一处；`acpi_init` 失败路径不幂等（清单旧、开关关）（§2.7）。

**反证尝试**：构造时序反例——若 `apic_single_cpu_init` 里两行次序相反，或 `config_no_apic` 缺省不是 1，或存在第四个调用点，则文档的"次序无关"论证塌掉。

**结果**：反证失败（主张成立）。

**证据**：
- `rg -n "acpi_init\(\)" minix3/minix/kernel/` → 恰好三点：`arch_system.c:264` / `arch_smp.c:315` / `apic.c:1141`；`apic.c:1141`（acpi_init）在 `:1143`（`if (!detect_ioapics())`）之前，次序与文档一致。
- `minix3/minix/kernel/main.c:445-447`：`config_no_apic = atoi(value); … else config_no_apic = 1;`——缺省确为 1；`main.c:304-307`：`if (config_no_apic) { smp_single_cpu_fallback(); } else if (config_no_smp) { … } else { smp_init(); }`——与 §2.7 表逐行吻合。
- 失败不幂等的物质基础：`acpi.c:68-75` `acpi_phys_copy` 在 `!vm_running` 下无条件 `memcpy` 返回 0，`acpi_read_sdt_at` 的读失败分支启动期确实走不到，与 §2.3 的可达性分析一致。

**若我错了**：若非 SMP 构建下 `arch_init` 的 APIC 段守卫不是 `#if defined(USE_APIC) && !defined(CONFIG_SMP)`，则"非 SMP 下 no_apic 未给 ⇒ 跳过"一行不成立——核实 `arch_system.c:267` 正是该守卫。

---

## CLAIM RD04-CLAIM-04

**主张**：Rust 侧 `init_from_kinfo` 按序试每个来源、取第一个解析成功者；全失败时 dev 构建静默落 `QemuVirtDesc`、release panic，兜底路径什么都不打印（§3.5、§4.7）；`parse_by_kind` 对未知 kind 报 `UnknownKind` 而非送进解析器（§4.3）；MADT 遍历有两道闸，零长/越界记录被拒（§2.4、§4.4）；`check_gic_madt` 决策表存在（§4.4）。

**反证尝试**：找兜底分支里实际打印日志的证据（文档断言"什么都不打印"）；找 release 下兜底仍可用的路径；构造零长记录挂死反例。

**结果**：反证失败（主张成立）。

**证据**：
- `os/libs/minix-platform/src/global.rs:275-290` `qemu_fallback_or_panic` 逐字吻合：dev 分支体仅 `let _ = reason;` + 构造，release 分支 panic 文本 `"platform::init_from_kinfo: {} and not a dev build (no QemuVirt fallback in release)"`；`:2073-2099` `init_from_kinfo` 的 for 循环 Err 续源、None 落兜底，注释与文档引文一致。
- `os/libs/minix-platform/src/kind.rs`：`k => Err(PlatformParseError::UnknownKind(k.raw()))` 兜底臂存在；`Ok(PlatformDescEnum::` 恰 2 处（:57 DeviceTree、:69 Acpi），另 2 处匹配在文档注释——与 §4.3 的口径复算一致。
- `os/libs/minix-platform/src/acpi.rs:498`：`if entry_len < core::mem::size_of::<MadtEntryHeader>() || offset + entry_len > madt_len { break; }`——零长记录（`entry_len == 0 < 2`）在第一道闸即被拒，不会复刻 C 侧 `acpi_madt_get_typed_item` 的死循环；`:736-749` `check_gic_madt` 三查齐备。

**若我错了**：若 dev 分支在某处（如 `log` 可用时）真的输出，则"什么都不打印"过强——通读该函数体无任何输出语句，反例不存在。

---

## CLAIM RD04-CLAIM-05

**主张**：CPU 拓扑链路：MADT LAPIC/X2APIC/GICC 按 `flags & 1` 过滤（§2.4、§4.4）；`CpuTopology.bsp_id` 的语义是"第一条 enabled 记录的硬件 ID"、不是逻辑槽位（§4.9）；`elected_bsp_hw_id` 用 CPUID/election 解析真实引导 CPU、失败回退 `topo.bsp_id`；`init_smp_state` 与 `init_protection` 对同一次选举用不同回退（`unwrap_or(topo.bsp_id)` vs `unwrap_or(0)`），且 `CpuId::new_unchecked` 不查界——文档把这条登记为 R04-T12 而非宣称已防护。

**反证尝试**：找"文档宣称有防护但代码没有"的矛盾点；验证过滤与两种回退是否如文档描述；验证 hw_id/槽位混用风险是否真实存在。

**结果**：反证失败（主张成立）。

**证据**：
- `os/libs/minix-platform/src/acpi.rs:534/562/595`：三处 `let enabled = (flags & 1) != 0;`——过滤实存。
- `os/kernel/src/lib.rs:1923-1950` `elected_bsp_hw_id`：三架构分支（riscv64/aarch64 选举 cell、x86_64 CPUID leaf 1），末行回退 `topo.bsp_id as u64`；`lib.rs:1974` `init_smp_state` 的 `.unwrap_or(topo.bsp_id as usize)` 与 `lib.rs:2251-2254` `init_protection` 的 `unwrap_or(0)`——两种回退并存，文档如实陈述并登记 R04-T12。
- `os/kernel/src/proc.rs:511-513`：`pub const fn new_unchecked(id: u32) -> Self { CpuId(id) }`——确无边界检查，越界风险真实；文档未把它写成"不会发生"。
- `os/libs/minix-boot/src/platform.rs:279` `pub bsp_id: u32` 及其字段注释与 §4.9 的"hw_id 同域、非逻辑下标"表述一致。

**若我错了**：若 MADT/DTB 解析器保证 `bsp_id == 槽位 0`（即不变量其实成立），R04-T12 的登记就多余——但 `elected_bsp_hw_id` 自己的注释明说 "ACPI does not require to be the boot CPU"，不变量不成立的立场有代码内证。

---

## CLAIM RD04-CLAIM-06

**主张**：文档 §5/§6 的统计与引用口径：minix-platform 26 条 `#[test]`（x86_64 宿主实跑 18 条）；54 处 `platform_sources: &[]`（含 21 个测试内核）；`global.rs` 25 条 `Self::` 转发；`acpi.c` 410 行；`ACPI_MADT_TYPE` 11 个；`run_all.sh` 各行号；§4.2 乙段的 "cross_validate_config_dtb（uefi_helpers.rs:1188，5 条 panic 停机路径）" 与 "push_dtb_sources 写进 wire（handoff_v1_write.rs:327-334）"。

**反证尝试**：逐条复跑文档给出的命令；对 18 条的 cfg 算术做独立核对；精确圈定 `cross_validate_config_dtb` 的函数体范围数停机路径。

**结果**：非阻断。

**严重度**：P2

**证据**：
- 复跑全部命中：`#[test]` 总数 26；`platform_sources:\s*&\[\]` 54；`Self::(DeviceTree|Acpi|QemuVirt)` 25；`rg -l … os/qemu-tests/test-kernels` 21；`wc -l acpi.c` 410；`grep -c 'define ACPI_MADT_TYPE' acpi.h` 11；`run_all.sh:96/102/108/131/211/246` 逐行吻合。
- 18 的算术核对独立成立：26 − (riscv64 4 + aarch64 3 + `device_tree.rs:627` riscv64-only 1) = 18，且 `device_tree.rs:661` 与 `acpi.rs:868/962` 的 cfg 门控与之自洽。
- **反证命中两处瑕疵**：
  1. `cross_validate_config_dtb` 定义在 `os/boot-shim/src/uefi_helpers.rs:855-895`（下一函数 `load_dumpdtb_blob` 始于 :896），函数体内停机路径是 **4** 条——:866 panic（no owned copy）、:874 panic（unparseable）、:880 `.expect`（memmap rejected）、:885 panic（mismatch）；:899/:902 两条 panic 属于 `load_dumpdtb_blob`，不在该函数内。文档写"5 条"且行号 `:1188` 实为 UEFI 腿的**调用点**（`sed -n '1185,1192p'` 可见 `cross_validate_config_dtb(report.platform_sources, …)`），不是定义。
  2. `push_dtb_sources` 定义在 `os/boot-shim/src/ebs_pair.rs:237`；`handoff_v1_write.rs:326-334` 是它逐条调用的 `push_source`（wire 追加处）。函数名与文件的归属不精确，但"DTB 派生 SourceRecord 跨 ABI 写进 wire"的行为属实（`ebs_pair.rs:194` 调用点核实）。
- 两处瑕疵都不动摇任何设计结论（机制存在、行为正确），故 P2 非阻断；若按最严口径归类，(1) 属可判 P0-fact 的计数失真，因无行为后果降为 P2。

**若我错了**：若"5 条"的口径把被调 helper 内的停机路径（如 memmap 拒绝链）也计入，则计数成立——但该口径文中未声明，且"1188 是调用点"这一点独立于口径成立。

---

## 跨节复述与失败路径检查（任务第 4 项）

- **复述一致性**：aarch64"先 DTB 后 ACPI、AAVMF 实测走 ACPI"在 §1.2 / §4.2 / §4.3 / §5.2 四处复述互相一致，且与代码（两段循环）与 kind.rs 注释三方对齐；"两处例外（甲 memmap DTB 子集 / 乙 dtb_copy 两段式）"在 §1.3、§3.1、§4.2 三处编号与内容一致；§2.7 的"默认只跑一次"与 §2.10 归纳一致，均与 `main.c` 吻合。
- **失败路径**：未发现把失败路径写成"不会发生"的情况。文档对以下失败面全部显式给出：`acpi_init` 二次失败不幂等（§2.7）、RSDP 20 字节校验不覆盖扩展部分（§2.2）、C MADT 零长记录死循环（§2.4）、Rust 兜底静默与 release panic（§3.5/§4.7）、`check_gic_madt` 拒绝表（§4.4）、ACPI 表链窗口缺口（§4.5，R04-T13）、BSP 回退不一致（§4.9，R04-T12）。唯一发现的失真是 CLAIM-06 的"5 条 panic"计数与行号引用精度。

---

## CLAIM 11

- 结果：反证失败（对应上文 RD04-CLAIM-01）

## CLAIM 12

- 结果：反证失败（对应上文 RD04-CLAIM-02）

## CLAIM 13

- 结果：反证失败（对应上文 RD04-CLAIM-03）

## CLAIM 14

- 结果：反证失败（对应上文 RD04-CLAIM-04）

## CLAIM 15

- 结果：反证失败（对应上文 RD04-CLAIM-05）

## CLAIM 16

- 结果：非阻断（对应上文 RD04-CLAIM-06）

- 本次反证：6 条 CLAIM（阻断 0 / 非阻断 1 / 反证失败 5）。
