# D04 回归审查 —— 04-platform-discovery（FREEZE 后）

- 审查日期：2026-11-13
- 当前 HEAD：`a4afb4927`
- 冻结提交：`bc232e627`（R2 确认轮收口；`da04c7486` 为 FREEZE 提交）
- 文档：`rewrite-notes/01-stage-kernel/04-platform-discovery.md`
- 台账：`rewrite-notes/coordination/review-eval/runs/01-stage-kernel/04-platform-discovery/defect-ledger.md`
- 范围：文档全文、FREEZE、R2 台账、`tools/doc-code-map.sh` 列出的 52 个文件与 `minix3/` 下的 C 源
- **结论：PASS**（未发现 P0/P1 回归；1 条 P2 残留见 `FINDING REG-D04-01`，建议登记后随 `D04-design-batch` 收口）

## 1. FREEZE 复核

| 项 | FREEZE 记录 | 本次实测 | 结果 |
|---|---|---|---|
| 文档 sha256 | `719b75c67d9d4e131a30047ef15030e9676da78baba53d4a3612ca36f66a41dc` | `sha256sum rewrite-notes/01-stage-kernel/04-platform-discovery.md` 输出同值 | 一致 |
| 文档行数 | 2729 | `wc -l` → 2729 | 一致 |
| 关联代码存在性 | `doc-code-map.sh --check` 52/52 | `bash tools/doc-code-map.sh rewrite-notes/01-stage-kernel/04-platform-discovery.md --check` → 共 52 个文件；存在性 52/52 通过 | 一致 |
| anchor | 77/77 | `bash tools/anchor-resolve.sh --check <doc>` → `anchors=77 resolved=77 zero-def=0 multi-def=0` | 一致 |
| 内部引用 | structure/ref 通过 | `bash tools/doc-ref-check.sh --check <doc>` → 内部章节引用均可解析 | 一致 |
| R2 `--round` 门 | PASS（12 PASS / 2 WARN / 0 FAIL） | `python3 -I tools/eval-trace-check.py <run-dir> --round R2` → 同一分布，`结论: PASS`，exit 0 | 一致 |
| 冻结后代码改动 | 无 | `git diff --stat bc232e627..HEAD` 只含 `rewrite-notes/` 的文档、FREEZE、台账、metrics；无 `os/` 改动 | 一致 |
| 测试门 | `cargo test -p minix-platform` 18 passed | 本次重跑：`18 passed; 0 failed`（含 `test_parse_synthetic_acpi`、`test_check_gic_madt_decision_table` 等） | 一致 |

两个 WARN 分别是 `reader-fix-log.md` 无 `## FINDING` 行（loop 已豁免）与 `D04-design-batch` 的已批准延期（D-054）。`DECISIONS.md` 第 136 行有判决账行，`OPEN-DEBT.md` 有对应条目。WARN 不是回归。

## 2. 抽样复验（10 条台账已修项）

| 发现 ID | 原严重度 | 复验命令/证据 | 结果 |
|---|---|---|---|
| RD4-R1-01（RD4-FACT-01） | P0 | `grep -c "find_platform_sources(" doc` → 0；`grep -c "find_platform_sources_into" doc` → 9；`os/boot-shim/src/uefi_helpers.rs:208` 为 `pub fn find_platform_sources_into(out: &mut [PlatformDescSource]) -> usize`；`os/boot-shim/src/lib.rs:250` 重导出 | PASS |
| RD4-R1-02（RD4-FACT-02） | P0 | `os/libs/minix-boot/src/platform.rs:125-130` 为三字段 `kind: PlatformDescKind` / `len: u32` / `phys_addr: PhysBytes`；`os/libs/minix-boot/src/boot_storage.rs:224` 有 `platform_slots_mut`；`uefi_helpers.rs:208-274` 写槽位并 `copy_from_slice`；`grep -c "Box::leak" os/boot-shim/src/uefi_helpers.rs` → 0；doc:1076/1385/1625 的三字段与槽位描述一致 | PASS |
| RD4-R1-03（RD4-FACT-03） | P0 | `grep -c '#\[test\]' os/boot-shim/src/uefi_helpers.rs` → 12；逐名为 1 条 `KernelInfo`（`test_build_kernel_info_fields`）+ 5 条 bootstrap memmap（`test_bootstrap_outside_memmap_*`）+ 6 条 kernel-image/区间断言；与 doc:2525 的分解一致 | PASS |
| RD4-R1-04（RD4-FACT-04） | P1 | `rg -o 'platform_sources:\s*&\[\]' os/ --type rust -g '*.rs' \| wc -l` → 54；逐文件 `dm_coverage.rs` 10、`arch/boot.rs` 9、`kernel/lib.rs` 6、`vm_handoff.rs` 3；`rg -l ... os/qemu-tests/test-kernels \| wc -l` → 21；与 doc:2532-2548 一致 | PASS |
| RD4-R1-06（RD4-FACT-06） | P2 | doc:2196 写“Step 2b 的一处编译期断言（Step 1 用运行期 `assert!`）”；`os/kernel/src/lib.rs:1334` 为 Step 2b 的 `const _: () = assert!(...)`，`:1249` 为 Step 1 的运行期 `assert!` | PASS |
| RD4-R2-41（RD4-R2-CERT-01） | P1 | `grep -c "freeze 之后、\`intr.init()\`" doc` → 1；源：`os/kernel/src/lib.rs:2493` 调 `intr.set_plic_va_base(plic_va)`，`:2498` 才 `intr.init()`；方法体在 `os/plat/src/riscv64/interrupt.rs:286` | PASS |
| RD4-R2-42（RD4-R2-CERT-02） | P1 | `grep -c "owned copy 只用于交叉校验" doc` → 1；源：`os/boot-shim/src/uefi_helpers.rs:865` 的 `dtb_copy::owned()` 只在 `cross_validate_config_dtb` 内使用；`platform_sources` 仍由 `:1035` 的配置表扫描写入（`:1072` 交给 KernelInfo），DTB 地址未被 owned copy 替换 | PASS |
| RD4-R2-43（RD4-R2-CERT-03） | P2 | `grep -c "原文写 low 4 GiB" doc` → 1；doc:1905 的块头把 Safety 段标成“旧前提，现行实现见正文”；doc:1902 的正文说明 Step 1 只建最小窗口、`push_platform_windows` 按 source 长度建（`os/kernel/src/lib.rs:1123-1141`） | PASS（另见 §5 的 P3 观察） |
| RD4-R1-21（RD4-READER-A-01） | P2 | doc:26 的第二行已把“计数器在哪”指到 §3.8 的 PIT 端口 `0x40`/`0x43`，并把 CPU 主频另归 §2.9；与 `panel/round1/reader-fix-log.md` 的 A-01 记录一致 | PASS |
| RD4-R2-27（RD4B-READER-A-01） | P2 | doc:1555 的 C 侧消费者枚举已补 FACP 电源表读取（`acpi_init_poweroff`，§1.4 ⑤）；源：`minix3/minix/kernel/arch/i386/acpi.c:230` 定义、`:242` 读 `acpi_get_table_base("FACP")`；与 `panel/round2/reader-fix-log.md` 的 A-01 记录一致 | PASS |

复验方式说明：台账中标记为“冷读 CLOSED”的条目（R1 的 fact/design、R2 的 fact/certify 与 reader 项）没有可重放的 grep 串；本次按“正文 + 源码位置 + 修复日志”三方核对，能在源码定位的以源码为准。

## 3. 关键事实源码再核对（3 条）

### KF-1 §2.3 `acpi_init` 的整数提升与 176 字节容量闸（doc:538-544）

为什么关键：这是 C 章最承重的一处失败推演——它同时决定“容量闸缺失时受害对象是越界读还是越界写”“`sdt_count` 为什么是正数”“越界从哪个下标开始”。R1/R2 的 P0 修正多次围绕这段改写（RD4-FACT-07、RD4-READER-B-02）。

证据：

- `minix3/minix/kernel/arch/i386/acpi.c:473`：`#define MAX_RSDT 35`；`acpi_read_sdt_at` 的 `size` 参数是 `sizeof(struct acpi_rsdt)`；SDT 头字段和 = 4+4+1+1+6+8+4+4+4 = 36 字节，缓冲区 = 36 + 35×4 = 176 字节。
- `acpi.c:68-75`：`acpi_phys_copy` 在 `!vm_running` 时 `memcpy` 后 `return 0`；VM 起来才 panic。
- `acpi.c:323`：`sdt_count = (s - sizeof(struct acpi_sdt_header)) / sizeof(u32_t);`。`s` 为 `int`，`sizeof` 为无符号类型；i386 构建的 32 位 `size_t` 下 `(-1 - 36)` 提升为 `4294967259`，除 4 得 `1073741814`（本次复算：`python3 -c "print((2**32-1-36)//4)"` → `1073741814`）。
- 循环 `for (i = 0; i < sdt_count; i++)` 写 `sdt_trans[i].signature`，`sdt_trans[MAX_RSDT]` 只有 35 项，界内下标为 0..34，越界从 35 开始。

结论：文档的“越界读在前、越界写在后”“`-1` 被提升成正数 1073741814”“越界从 i=35 开始”“ERROR 不可达”均与源码一致（前提是 i386 目标的 `size_t` 为 32 位；Minix3 的 i386 构建即此）。

### KF-2 §4.2 boot-shim 的来源扫描顺序（doc:1617-1623）

为什么关键：这是“DTB 优先、ACPI 兜底”这条设计意图的直接载体，也是 R2 B-01 自省“注释 prefer ACPI 2.0+ 的意图大于实现”的依据；顺序反了会让来源由固件表项顺序决定而不是设计决定。

证据（`os/boot-shim/src/uefi_helpers.rs:208-266`）：

- x86-64：单个 `for e in entries`，条件 `e.guid == ACPI2_GUID || e.guid == ACPI_GUID`，命中即 `break`；最多 1 个槽位。
- aarch64：第一段 `for` 只匹配 `DEVICE_TREE_GUID || LEGACY_DEVICE_TREE_GUID` 并 `break`；第二段独立 `for` 才匹配 `ACPI2_GUID || ACPI_GUID` 并 `break`；最多 2 个槽位，DTB 在前。
- 其余架构：`let _ = entries;`，返回 0。

结论：文档的“两段独立循环”与“x86 单循环 + `||` 由表项顺序决定”与源码一致。

### KF-3 §2.5 LAPIC 基址的“半格发现”（doc:658-737）

为什么关键：这是本篇的标题级论点——“C 只发现了半格”；它同时是 §3.8 描述符判据（有无报告协议）与 §2.5 反事实（重定位 LAPIC 时 IOAPIC 正常、LAPIC 不通）的基础。

证据：

- `rg -n 'local_apic' minix3/minix/kernel/arch/i386/` 只有 `acpi.h:99` 的一行声明，无读取点。
- 两条启动路径都把 `lapic_addr` 赋成常量：`apic.c:1130` 与 `arch_smp.c:297` 的 `lapic_addr = LOCAL_APIC_DEF_ADDR;`；宏在 `apic.h:59`（`0xfee00000`）。
- 从 `IA32_APIC_BASE` 读地址位的那段在 `apic.c:651` 的 `#if 0` 块内，块外的 `msr_lo |= (1 << IA32_APIC_BASE_ENABLE_BIT)` 只置使能位。
- 进入分页后 `lapic_addr` 改存 VM 返回的虚拟地址：`memory.c:954` 的 `lapic_addr = lapic_addr_vaddr;`，`memory.c:852` 的 `lapic_addr_vaddr = addr;`；映射请求在 `memory.c:819` 的 `*addr = lapic_addr;`。

结论：文档关于“MADT 表头字段无读者、活路径只有常量、MSR 地址读取是死代码”的陈述与源码一致。

## 4. 新发现

### FINDING REG-D04-01（P2）

- 位置：`rewrite-notes/01-stage-kernel/04-platform-discovery.md:2371`（§4.11 第 ② 段 riscv64 格）与 `:2568`（§5.4 M7 证据格）。
- 证据：
  - doc:2371 写 `` `dtb_ptr()` → `PlatformDescSource::new(DTB, ...)` ``；实际 `os/boot-shim/src/opensbi_helpers.rs:419-422` 为 `let len = unsafe { minix_boot::dtb_total_size(PhysBytes(pa)) }; storage.push_platform_source(PlatformDescSource::with_optional_len(DTB, PhysBytes(pa), len))`。
  - doc:2568 写 `` `os/kernel-image/src/bootface_a64.rs:fn bootface_a64`（`PlatformDescSource::new(DTB, DTB_PA)` → `parse_gic`） ``；实际 `os/kernel-image/src/bootface_a64.rs:298-300` 为 `let dtb_len = unsafe { minix_boot::dtb_total_size(PhysBytes(DTB_PA)) }; storage.platform_slots_mut()[0] = PlatformDescSource::with_optional_len(DTB, PhysBytes(DTB_PA), dtb_len);`。
  - `PlatformDescSource::new` 的语义是 `len = 0`（`os/libs/minix-boot/src/platform.rs:136-142`），窗口构建会退回 2 MiB 片（`os/kernel/src/lib.rs:1136-1139`）；`with_optional_len` 才携带表头读出的长度。两处写法把“带长度的来源”写成了“无长度的兼容构造”。
  - 这两处正是原 P0 发现 `RD4-FACT-02` 最小修复里点名的落点（“§4.11 ② 与 §5.4 M7 的 `PlatformDescSource::new` 改为 `with_optional_len`（或注明 `new` 是 len=0 的兼容构造）”）；R1 修复只落地了 §3.1/§3.6/§4.2/§5.3，未触及这两格。
- 影响：读者按 §5.4 M7 去 `bootface_a64` 找 `new` 会找不到，且会误以为该路径不携带 DTB 长度、窗口退回 2 MiB——与 §4.2/§4.5 的“按 blob 长度建窗”设计相反。主模型（DTB 单源 → `parse_gic`）仍正确，故按局部符号/事实残留定为 P2，而非原发现的 P0。
- 最小修复：两处改为 `PlatformDescSource::with_optional_len(DTB, PhysBytes(...), dtb_len)`（或等价的文字描述“`with_optional_len(DTB, …)`”）；若刻意保留 `new` 作为简化，需在该格注明“`new` 是 len=0 的兼容构造，长度随后由 `dtb_total_size` 补”。

## 5. 非阻断观察（P3，不计入 P0/P1/P2 扫描结论）

- doc:1905 的引文块头写“原文写 low 4 GiB”，但 `os/libs/minix-platform/src/device_tree.rs:34-35` 的原文是 `low 4 GB`（不是 `GiB`）。引文块内也写作 `low 4 GiB`，与源文相差一个单位名。属引文漂移，不影响“旧前提、现行实现见正文”的结论；建议下次触碰该段时改为 `4 GB` 或标注“单位名已规范化”。

## 6. 局限声明

- **同 agent 复核**：本次审查与此前面板、certify、修复使用同一模型；没有跨模型独立复核。命令与源码行号已列出，便于第三方复算。
- **QEMU 未跑**：三架构真机门（`test-smp-topo*`、`test-smp-aps-aarch64`、`test-smp-ipi-riscv64` 等）未重跑；`test-smp-topo` 的打印格式、`-dtb`/`aclint=on` 点火行只按脚本与源码核对。
- **测试只重跑一个包**：`cargo test -p minix-platform` 重跑通过（18 passed）；`uefi_helpers`、`minix-boot` 与内核宿主测试未重跑，12 条 uefi 测试只按测试名与计数核对。
- **未重跑全部文档门**：只重跑了 FREEZE 列出的 sha/行数/`doc-code-map`/`anchor`/`doc-ref`/`eval-trace --round R2` 与 `cargo test -p minix-platform`；citation（86 块 `/* */` 的 MANUAL_FALLBACK）、style-lint、structure 与承载门未重跑，引用 FREEZE 与 R2 修复日志的结论。
- **设计批未重裁**：`D04-design-batch`（D-054）的 12 条代码/架构项与 P3/SUGGESTION 仍按登记状态存在，本次未重开；`R04-T01`..`R04-T12` 的测试/代码 TODO 也未实施。
- **抽样不是穷举**：10 条样本覆盖 P0/P1/P2 与事实/代码/读者三类，但不等于 43 条全部可重放；未抽到的“冷读 CLOSED”行仍可能有同类误记（REG-D04-01 即由关键事实核对而非抽样发现）。
