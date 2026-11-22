# D04 R2 certify（闭集）

- 目标：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 工作树基线：2729 行，`sha256 = aa0bd83ea99bbc89c39cc1f0d24189269f52109e7832404364e7cb5b087f64c1`
- 基线口径：`panel/round2/reader-fix-log.md` 记录的修复后哈希是 `5050072e3260907f2e65cd7defcbe581af1923ce3e5a60baec13c435636edb24`，与工作树不一致；行数同为 2729。说明读者修复之后工作树又落了 fact/design 的 R2 最小修复（§4.5、§3.4、§3.1 等）。本 certify 按工作树校验，不按 5050072 校验。
- 范围：R2 改动段及邻句；未重扫全文，未读其他面板/expected。
- 日期：2026-11-12

## 一、逐条核对

### 1. §4.5 恒等窗口（`最小启动窗口 + push_platform_windows + len=0 回退 2 MiB`，aarch64 UEFI 为 owned copy）— **REOPEN**

**已落实且正确的部分**：文档 1902 行已把旧句「低 4GB 铺 VA == PA」改为「`arch_boot_impl` Step 1 的最小启动恒等窗口内（不是低 4GB 全铺；窗口由 `push_platform_windows` 按每个 source 的长度建，`len=0` 才退回 2 MiB 片）」。源码对得上：

- `os/kernel/src/lib.rs:1221-1229` Step 1 注释「Minimal boot identity windows … the minimal windows turn those into page faults」；`1241-1278` 按窗口逐片 `map_huge(MIB_2)`。
- `os/kernel/src/lib.rs:1123-1141` `push_platform_windows`：`source.has_len()` 时 push `[start, start+len)`，否则 `push_2m`；`push_2m` 在 `1097-1103`。
- `os/libs/minix-boot/src/platform.rs:179-181` `has_len()` 就是 `len != 0`。

**不成立的部分**：同句「DTB 在直核腿是 boot-shim 留在低地址的静态固件内存，在 aarch64 UEFI 腿则是 `dtb_copy` 的 owned copy，两者都落在已注册窗口内」把内核 `parse` 的输入说错了。内核 `parse` 读的是 `KernelInfo.platform_sources` 里的地址；aarch64 UEFI 腿写入该字段的是配置表里的 DTB 地址，不是 `dtb_copy` 的副本：

- `os/boot-shim/src/uefi_helpers.rs:239-241`：`let pa = PhysBytes(e.address as u64); … PlatformDescSource::with_optional_len(DTB, pa, len)`，`pa` 是配置表项地址。
- `os/libs/minix-boot/src/handoff_v1_write.rs:250-266`：`write_platform_sources` 原样序列化 `(kind, len, phys_addr)`，没有把 DTB 地址改写成副本地址。
- `os/boot-shim/src/dtb_copy.rs:169-208`：`authorize_and_copy` 分配独立 `Box` 缓冲（`dst`），并断言 `dst` 不与源重叠；副本只由 `os/boot-shim/src/uefi_helpers.rs:865-869` 的 `cross_validate_config_dtb` 与 `os/boot-shim/src/ebs_pair.rs:190-194` 的 `resv_claims::capture` 消费。
- `os/kernel/src/lib.rs:1744-1747` 的内核注释直接写明「the only firmware blob read after this point (the DTB, Phase A.5) is consumed before the first allocator can reach it」——内核读的是固件 blob，不是 boot-shim 的副本。

因此「aarch64 UEFI 腿则是 `dtb_copy` 的 owned copy」应改为「aarch64 UEFI 腿在 boot-shim 侧另有一份 `dtb_copy` 的 owned copy（EBS 后交叉校验与 resv claims 用）；内核 `parse` 读的仍是 `platform_sources` 里的配置表地址，其窗口由 `push_platform_windows` 按 `dtb_total_size` 建」。详见 FINDING RD4-R2-CERT-02。

**邻句（同段 Safety 引文）**：紧接着的「逐字引用」仍写「an identity mapping over the low 4 GB」（源码 `os/libs/minix-platform/src/device_tree.rs:34` 确实还是这句），与新写的「不是低 4GB 全铺」直接矛盾。引文本身是源码原文，但源码注释已落后于 Step 1 的最小窗口实现；文档没有加注说明这一点，读者会得到两个互斥的前提。详见 FINDING RD4-R2-CERT-03。

### 2. §3.4 freeze 前 `set_plic_va_base` 例外 — **REOPEN**

文档 1279 行改成了：「实例字段在 freeze 后不再被 `&mut self` 方法重写；freeze 前有一个显式例外——riscv64 的 `set_plic_va_base` 会在 `init` 之后、freeze 之前重写 PLIC 基址。」

源码的实际顺序与该句的两个时间锚点都不符：

- freeze 先发生：`os/kernel/src/lib.rs:1758` 的 `minix_platform::init_from_kinfo(kernel_info)` → `os/libs/minix-platform/src/global.rs:218` 的 `PLATFORM.freeze(PlatformContext { desc })`。
- `init_clock_and_interrupts` 后发生：`os/kernel/src/lib.rs:1785`。
- 在该函数内部：`CurrentInterruptController::new` 在 `2429`，`intr.set_plic_va_base(plic_va)` 在 `2493`，`intr.init()` 在 `2498`。

即 `set_plic_va_base` 在 **freeze 之后、`intr.init()` 之前** 重写 `plic_va_base`（PLIC 的高半 VA，不是 `plic_base` 物理基址）。原句的「freeze 前」与「`init` 之后」都反了。详见 FINDING RD4-R2-CERT-01。

### 3. §3.1 KernelInfo 注释恢复 `len=0` 回退句 — **CLOSED**

文档 1076 行的引文已恢复为：

```text
/// Each `PlatformDescSource` is pure data `(u32 kind, u32 len, u64 phys_addr)` — `len = 0` means the producer did not record the blob extent; the window builder then falls back to one 2 MiB piece —
```

与源码逐字对得上：`os/libs/minix-boot/src/kernel_info.rs:134-136` 为

```text
/// ELF binaries. No function pointers cross the boundary. `len = 0`
/// means the producer did not record the blob extent; the window builder
/// then falls back to one 2 MiB piece.
```

文档只在行内把三行并成一行、用破折号嵌入原句，语义与文字一致；没有新增事实偏差。

### 4. reader R2 的 P2（A-01/A-02/A-04/A-07/B-01/B-02/B-03/B-04）— **CLOSED（抽查 5/8）**

按 `reader-fix-log.md` 抽查 5 条，5 条都已在工作树落实且与源码一致：

| 条目 | 文档位置与内容 | 源码复验 | 结论 |
|---|---|---|---|
| A-01 | §4.1 的 1555 行补「FACP 电源表读取（`acpi_init_poweroff`，§1.4 ⑤ 已列）」 | `minix3/minix/kernel/arch/i386/acpi.c:230` 有 `acpi_init_poweroff`，`242` 行读 `acpi_get_table_base("FACP")` | CLOSED |
| A-02 | §4.9 的 2263 行新增 `CpuTopology.bsp_id` 单位段；§4.10 的 2345 行 hint 补「其值是 `hw_id`」；§5.4 的 2591 行新增 R04-T12 | `os/libs/minix-boot/src/platform.rs:270` 注释「hardware ID of the first enabled CPU record」；`os/libs/minix-platform/src/device_tree.rs:475` 与 `os/libs/minix-platform/src/acpi.rs:538` 都在首条 enabled 记录处填 `bsp_id`；`os/kernel/src/proc.rs:511` 的 `CpuId::new_unchecked` 不查上界 | CLOSED |
| A-07 | §3.5 的 1330 行 QEMU 行补「走 boot-shim 的测试…；`platform_sources: &[]` 的极简测试内核不调用 `init_from_kinfo`，发现阶段不运行」；§5.3 的 2550 行判据句补「也不调用 `init_from_kinfo`」 | `rg -l 'platform_sources:\s*&\[\]' os/qemu-tests/test-kernels \| wc -l` = 21；与 `rg -l init_from_kinfo os/qemu-tests/test-kernels` 求交 = 0；21 个文件里没有 `test-smp-topo*` | CLOSED |
| B-01 | §4.2 的 1623 行新增「单循环 + `\|\|` + `break` 只保证接受任一版本；rev 1 项在前时会取到 rev 1 RSDP，按 §4.4 回落 RSDT，XSDT 不可达」 | `os/boot-shim/src/uefi_helpers.rs:217` 为 `if e.guid == ACPI2_GUID \|\| e.guid == ACPI_GUID … break`；aarch64 的 DTB 与 ACPI 是两段独立循环（`232-257`） | CLOSED |
| B-03 | §5.3 的 2525 行把 12 条 `#[test]` 拆成「1 条 `KernelInfo` 字段 + 5 条 bootstrap memmap 边界 + 6 条 kernel-image 保留区对齐/模块/区间重叠」 | `rg -n '^\s*#\[test\]' os/boot-shim/src/uefi_helpers.rs \| wc -l` = 12；名单为 `test_build_kernel_info_fields` 1、`test_bootstrap_outside_memmap_*` 5、`test_reserve_kernel_span_rejects_unaligned_span` 1、`test_assert_range_clear_of_kernel_*` 4、`test_assert_modules_clear_of_kernel_rejects_byte_overlap` 1 | CLOSED |

未抽查的 A-04、B-02、B-04 按任务要求（抽查 5 条）不展开；本次没有证据表明它们被破坏。

### 5. design 6 条 P2 是否登记 `D04-design-batch`（D-054）— **CLOSED**

运行台账里 R2 design-d2 的 P2 恰好 6 条，且全部指向 `D04-design-batch` 并引用 D-054：

- `defect-ledger.md:186`（RD4-R2-11，RD4B-DESIGN-D2-01）
- `defect-ledger.md:189`（RD4-R2-14，D2-04）
- `defect-ledger.md:190`（RD4-R2-15，D2-05）
- `defect-ledger.md:191`（RD4-R2-16，D2-06）
- `defect-ledger.md:192`（RD4-R2-17，D2-07）
- `defect-ledger.md:201`（RD4-R2-26，D2-16）

每行的处置都是「② 相邻：目标 D04-design-batch；修于下一 batch；用户批准 2026-11-12（代理裁决 D-054）」。`OPEN-DEBT.md:5` 的 `D04-design-batch` 行明确写了 12 条（R1 六条 + R2 六条），R2 六条为 `DeviceTreeDesc` 缺 `psci_conduit`、`platform_sources` 无入口校验、ACPI 默认值隐藏、`PlicDesc.context` 硬编码、x86 MMIO 全局原子、ARCH `DeviceWindow`；`DECISIONS.md:136` 有 D-054 的八字段判决行。登记完整。可补一句文字说明：D-054 的枚举文字只列了 R1 六条，R2 六条是通过 `OPEN-DEBT` 同一批条目与台账行挂到 D-054 的；不影响「已登记」的结论。

## 二、机械门（L1 工具输出）

在仓库根执行，全部 exit 0：

```text
$ tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/04-platform-discovery.md
anchors=77 resolved=77 zero-def=0 multi-def=0

$ tools/doc-code-map.sh rewrite-notes/01-stage-kernel/04-platform-discovery.md --check
（共 52 个文件；存在性 52/52 通过）

$ python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/04-platform-discovery.md
✅ doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析

$ AWK=gawk tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/04-platform-discovery.md
✅ doc-ref-check: rewrite-notes/01-stage-kernel/04-platform-discovery.md 内部章节引用均可解析
```

四道门均未命中 F2 新增段；门通过不等于本轮两条 REOPEN 不成立——它们是事实/顺序错误，四道门不检查这类语义。

## 三、新发现

## FINDING RD4-R2-CERT-01（P1）

**位置**：`rewrite-notes/01-stage-kernel/04-platform-discovery.md:1279`（§3.4）。

**问题**：`set_plic_va_base` 例外的时间锚点写反了。文档说「实例字段在 freeze 后不再被 `&mut self` 方法重写；freeze 前有一个显式例外——riscv64 的 `set_plic_va_base` 会在 `init` 之后、freeze 之前重写 PLIC 基址」。源码顺序是：`init_from_kinfo` 在 `os/kernel/src/lib.rs:1758` 调用，`PLATFORM.freeze` 在 `os/libs/minix-platform/src/global.rs:218`；`init_clock_and_interrupts` 在 `1785`；函数内 `CurrentInterruptController::new` 在 `2429`，`set_plic_va_base` 在 `2493`，`intr.init()` 在 `2498`。所以它是 **freeze 之后、`intr.init()` 之前** 重写 `plic_va_base`（PLIC 高半 VA），不是「`init` 之后、freeze 之前」。

**影响**：该段是 §3.4「实例字段比全局变量可靠」的核心论据；顺序反了会让读者把「freeze 之后仍有一次实例字段改写」误读成「freeze 前的一次性初始化」，并和 §4.1「init_from_kinfo 必须先于 init_clock_and_interrupts」直接冲突。

**建议修法（文档侧）**：改为「实例字段在构造后不再被 `&mut self` 方法重写；构造之后、`intr.init()` 之前有一个显式例外——riscv64 的 `set_plic_va_base` 会重写 `plic_va_base`（PLIC 的高半 VA，不是 `plic_base`）」。不要用 freeze 作实例字段的时间锚点；freeze 只约束描述符。

## FINDING RD4-R2-CERT-02（P1）

**位置**：`rewrite-notes/01-stage-kernel/04-platform-discovery.md:1902`（§4.5）。

**问题**：文档断言 aarch64 UEFI 腿的 DTB 是 `dtb_copy` 的 owned copy，内核 `parse` 读的就是它。当前实现不是：`find_platform_sources_into` 把配置表里的 DTB 地址 `pa` 原样写进 `PlatformDescSource`（`os/boot-shim/src/uefi_helpers.rs:239-241`），`write_platform_sources` 原样序列化（`os/libs/minix-boot/src/handoff_v1_write.rs:250-266`）；`dtb_copy::authorize_and_copy` 另分配独立缓冲（`os/boot-shim/src/dtb_copy.rs:169-208`），只被 `cross_validate_config_dtb`（`os/boot-shim/src/uefi_helpers.rs:865-869`）和 `resv_claims::capture`（`os/boot-shim/src/ebs_pair.rs:190-194`）消费。内核注释也写明 Phase A.5 读的是固件 blob（`os/kernel/src/lib.rs:1744-1747`）。

**影响**：这是 §4.5「为什么 `parse` 能直接解引用物理地址」的落点之一。按现文，读者会以为内核解析的是 EBS 后仍存活的副本；实际是配置表地址，其窗口由 `push_platform_windows` 按 `dtb_total_size(pa)` 建。若未来有人真把 `platform_sources` 改成副本地址，窗口注册与 `dtb_total_size` 的读点都会跟着变，现文会掩盖这个耦合。

**建议修法（文档侧）**：改为「aarch64 UEFI 腿在 boot-shim 侧另有一份 `dtb_copy` 的 owned copy（EBS 后交叉校验与 resv claims 用）；内核 `parse` 读的仍是 `platform_sources` 里的配置表地址，窗口由 `push_platform_windows` 按 `dtb_total_size` 建」。若设计意图确实是让内核读副本，则需另开代码项（改 `platform_sources` 地址或窗口注册），不能只在文档里断言。

## FINDING RD4-R2-CERT-03（P2）

**位置**：`rewrite-notes/01-stage-kernel/04-platform-discovery.md:1904-1913`（§4.5，紧跟改动句的 Safety 引文）。

**问题**：新写的「不是低 4GB 全铺」与紧接着的「逐字引用」里的「an identity mapping over the low 4 GB」互斥。源码注释 `os/libs/minix-platform/src/device_tree.rs:34` 确实仍是旧句，但 `arch_boot_impl` Step 1 已改成最小窗口（`os/kernel/src/lib.rs:1221-1278`）。文档把旧注释当作「这条前提」的出处，等于把两个不同前提并置。

**影响**：读者无法判断哪句是当前事实；这是改动段直接引入的自相矛盾，不是全文存量问题。

**建议修法（文档侧）**：在引文前加一句「该注释的 low 4 GB 表述早于 BOOT-SCOPE 2a；当前实现以 Step 1 的最小窗口为准」，或把引文改成「示意（注释未随最小窗口更新）」。同时把「`device_tree.rs` 模块 Safety 注释更新」登记进 `D04-design-batch`（代码侧），避免下一轮再把它当逐字事实引用。

## 结论

- CLOSED：§3.1 KernelInfo `len=0` 回退句；reader 抽查 5/5（A-01/A-02/A-07/B-01/B-03）；design 6 条 P2 的 `D04-design-batch`（D-054）登记。
- REOPEN：§4.5（aarch64 UEFI owned copy 断言；邻句 low 4 GB 引文）；§3.4（`set_plic_va_base` 的 freeze/`init` 时间锚点）。
- 机械门：四道全绿（见 §二）。
- 新发现：3 条（P1 ×2、P2 ×1）。

certify 新准入 = 3
