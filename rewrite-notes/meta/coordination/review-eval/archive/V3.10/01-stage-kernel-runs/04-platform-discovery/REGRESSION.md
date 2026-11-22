# D04 回归审查 —— 04-platform-discovery（FREEZE 后）

- 审查日期：2026-11-13
- 当前 HEAD：`d6c37bce0`
- 冻结提交：`f9582d655`（R2 确认轮收口；`7073926cb` 为 FREEZE 提交）
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
| 冻结后代码改动 | 无 | `git diff --stat f9582d655..HEAD` 只含 `rewrite-notes/` 的文档、FREEZE、台账、metrics；无 `os/` 改动 | 一致 |
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

---

# D04 回归审查（R3 确认轮后）

- 审查日期：2026-11-20
- 基线提交：`f1cbe6a67db4d3e3f38e253bb1fd3237bcc68cda`（2026-11-20 04:13:55 +0800，`fix(04): R3 V3.4 确认轮——D04 在 V3.5 制品链下重新收敛`；工作树除 3 个与本篇无关的未跟踪文件外干净）
- 审查对象：`FREEZE.md` R3 节及其引用的全部制品（`loop.md`、`metrics.md`、`defect-ledger.md`、`OPEN-DEBT.md`、`certify/round3-certify.md`、`panel/round3/`、`suggestions.md`、`DECISIONS.md` D-070）与最终工作树
- **结论：FAIL**（两处记录层不一致：FAIL-1 严重度分布、FAIL-2 台账复验列与 suggestions.md 脱节。其余全部核对项——文档哈希/行数、三轮轨迹门、panel-index、六门、metrics 全部数值、D-070 判决链、抽查 5 行——均与 FREEZE 记录一致。两处 FAIL 均不动摇六门与制品链读数，属冻结记录文字/登记缺陷）

## R3-1. FREEZE 记录复核表

| 项 | FREEZE 记录 | 实测命令与输出 | 结果 |
|---|---|---|---|
| 文档 sha256 | `90179b7b…ea3545d`（R3 复核后） | `sha256sum rewrite-notes/01-stage-kernel/04-platform-discovery.md` → `90179b7b4d7a4428fe2a1591502aaf64b3ec4cd98947617d97f126529ea3545d` | 一致 |
| 文档行数 | 2737（R3 时点） | `wc -l` → `2737` | 一致 |
| 冻结头字段 | 头部保留 R2 口径（冻结日期 2026-11-12、edition V3.3、确认轮 R2） | R3 节为 append-only 追加（FREEZE.md 第 65–74 行），含 edition V3.4、`schema: V3.4 from R3`、D-070 引用 | 符合 §4h append 语义 |

## R3-2. 轨迹门与制品链

| 项 | FREEZE/loop 记录 | 实测命令与输出 | 结果 |
|---|---|---|---|
| `eval-trace-check --round R3` | PASS（11/2/0） | `python3 -I tools/eval-trace-check.py <run-dir> --round R3` → `汇总: PASS 11 / WARN 2 / FAIL 0`、`结论: PASS` | 一致 |
| `--round R1` | 仍 PASS | 同命令 → `汇总: PASS 11 / WARN 3 / FAIL 0`、`结论: PASS` | 一致 |
| `--round R2` | 仍 PASS | 同命令 → `汇总: PASS 11 / WARN 3 / FAIL 0`、`结论: PASS` | 一致 |
| `panel-index --check` | 6 文件哈希一致 | `python3 -I tools/panel-index.py <run-dir>/panel/round3 --check` → `panel-index: 6 个文件哈希与计数一致` | 一致 |
| panel INDEX 计数 | FINDING 27 / SUGGESTION 37 / CLAIM 6 | INDEX.md 实表逐列求和：FINDING 9+6+1+7+4+0=27、SUGGESTION 0+3+10+12+12=37、CLAIM 6 | 一致 |
| certify 回执 | 12/12 CLOSED，首轮即 PASS | `certify/round3-certify.md`：`结论：PASS`、`核验条数：12`、`CLOSED：12；STILL-BROKEN：0` | 一致 |
| 制品存在性 | FREEZE R3 全部引用 | `panel/round3/{fact,design-d2,reader-a,reader-b,reader-c,refute}.md`+`INDEX.md`、`certify/round3-certify.md`、`loop.md` R3 §0–§8+L6/L6.5、`metrics.md` R3 块、`defect-ledger.md` R3 节（29 行）、`OPEN-DEBT.md` R3-design-batch 行、`suggestions.md` R3 段、`DECISIONS.md:151` D-070、`prompt/EXECUTE-D01-D06-CONVERGENCE.md`（`git log --oneline -1 7b3c82e0b` → 确为该文件新增提交）、`design-d2.md:165` ARCH-PROPOSAL RD04-A1 —— 全部存在 | 一致 |

## R3-3. 六门读数（任务列 7 条命令，全部实测）

| 门 | FREEZE 记录 | 实测命令与输出 | 结果 |
|---|---|---|---|
| `anchor-resolve.sh --check` | 77/77 | `anchors=77 resolved=77 zero-def=0 multi-def=0` | 一致 |
| `doc-citation-check.py` | checked=68 failed=0 warned=7 | `blocks=87 checked=68 failed=0 skipped=12 warned=7` | 一致 |
| `doc-structure-check.py` | 通过 | `✅ doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析` | 一致 |
| `doc-ref-check.sh --check` | 通过 | `✅ doc-ref-check: … 内部章节引用均可解析` | 一致 |
| `doc-style-lint.sh` | error=0 | `命中汇总：\| error级=0` | 一致 |
| `doc-code-density-check.sh --check` | 通过 | `✅ 承载门阻断项通过（4 个可判章均有码/命令）` | 一致 |
| `doc-code-map.sh --check` | 54/54 | `（共 54 个文件；存在性 54/54 通过）` | 一致 |

## R3-4. metrics R3 块 vs 台账逐 ID 聚合

| 指标 | metrics R3 | 台账聚合（29 行逐行求和） | 结果 |
|---|---|---|---|
| 处置 ① | 10 | RD-R3-01(1)+02(3)+03(3)+04(2)+29(1)=10 | 一致 |
| 处置 ② | 3 | RD-R3-05(3)=3 | 一致 |
| 处置 ④ | 7 | RD-R3-08(1)+13(3)+20(1)+21(2)=7 | 一致 |
| FP | 5 | RD-R3-27(3)+28(2)=5（CLAIM-11..15 反证失败/主张成立） | 一致 |
| SUGG | 45 | 17 行 45 成员 = 读者 SUGGESTION 34（RA-S 10+RB-S 12+RC-S 12）+ DESIGN-D2-04..09（6）+ finding 形态 P3 5（READER-B-04..07、READER-C-04） | 一致（登记缺口见 FAIL-2） |
| 发现总数 | 70 | 70 个唯一成员 ID、零重复；形态分布 FACT 6+SUG 3 / DESIGN-D2 9 / READER-A 1+RA-S 10 / READER-B 7+RB-S 12 / READER-C 4+RC-S 12 / CLAIM 6 —— 与 INDEX 三类计数逐类吻合 | 一致 |
| 唯一根因数 | 29 | 29 行 RD-R3-01..29 | 一致 |
| 本轮新准入 | 29 | 29 行 origin 列全部 = `R3` | 一致 |
| certify 新准入 | 0 | 回执 12/12 CLOSED、0 STILL-BROKEN、无新增发现 | 一致 |
| 未闭环 | 0 | 29 行处置均终态（①已修 / ②D-070 批准延期 / SUGG 登记 / FP / CLAIM①已修） | 一致 |

### FAIL-1：严重度分布记录不一致

- FREEZE.md R3：「无 P0，1 条 P1（READER-C-01…），**15 条 P2**」；loop.md R3 §2：「P0 = 0；P1 = 1；**P2 = 15；P3 = 54**」。
- 实测 A（`defect-ledger.md` 严重度列按成员加权）：`grep "^| RD-R3-" … | awk -F'|' …$6` → P1=1、**P2=11**（RD-R3-01/05/08/13/21/29，成员 1+3+1+3+2+1）、**P3=58**。
- 实测 B（panel 六文件显式严重度标注，Python 逐节解析）：P1=1（READER-C-01）、**P2=11**（fact 1、design-d2 3、reader-a 1、reader-b 3、reader-c 2、refute CLAIM-06 1）、P3 显式 26，其余为「建议/S」或未标注。
- 两套独立真值互相一致（P2=11 / P3=58），FREEZE 与 loop 的 15/54 在任一制品中都数不出来；P0=0、P1=1 两条正确。`metrics.md` R3 块无严重度行，不受影响；`eval-trace-check` 不依赖该数字。
- 判：**FAIL（记录不一致）**。需把 FREEZE.md R3 节与 loop.md R3 §2 的严重度行改为 P2=11/P3=58（或给出 15/54 的可复算口径）。

### FAIL-2：RD-R3-14/15/22 复验列声称已登记 suggestions.md，实际缺 5 条

- 台账复验列实文：RD-R3-14「RD04-READER-B-04 等 3 条已登记 suggestions.md 的 R3 段」、RD-R3-15「RD04-READER-B-07 等 3 条已登记 suggestions.md 的 R3 段」、RD-R3-22「RD04-READER-C-04 等 3 条已登记 suggestions.md 的 R3 段」。
- `suggestions.md` R3 段实际仅 4 行：RA-S01..S10（10）+ RB-S01..S12（12）+ RC-S01..S12（12）+ DESIGN-D2-04..09（6）= **40 条**。
- READER-B-04/05/06/07、READER-C-04（finding 形态 P3、处置 SUGG）不在其中；SUGG 处置合计 45 vs suggestions.md 40，差 5 恰为这 5 条。
- 判：**FAIL（复验证据与登记制品不一致）**。P3→SUGG 降档不实施的处置本身合规（§4.0c），缺的是登记动作；需把 5 条补进 suggestions.md R3 段或更正台账复验列。

## R3-5. 未闭环与债务

- 未闭环 = 0（聚合见 R3-4；无任何行处于 open 态）。
- `OPEN-DEBT.md` R3-design-batch 行存在：「用户批准 2026-11-20（代理裁决 D-070）延期至下一 design batch」，列 RD04-DESIGN-D2-01/02/03 三项设计 P2 + 6 条设计 P3 + ARCH 提案 RD04-A1 —— 与 FREEZE R3「有 D-070 判决行与 2026-11-20 日期」一致。
- `DECISIONS.md:151` D-070（2026-11-20）：重开 D04 升 edition V3.4、12 处文档修改点本轮修、3 条设计 P2 记 `R3-design-batch`、RD04-A1 待用户亲裁；状态「已实施」。
- 旧账未动（FREEZE 亦未声称销账）：`D04-design-batch`（D-054）与 `R04-T13`（含 r7「判决行缺失」注记）仍在 OPEN-DEBT —— 一致。

## R3-6. 一致性抽查（5 行，①/②/④/SUGG/FP 五类各 ≥1）

| 台账行（类） | 台账声称 | 重放证据（实际命令与输出） | 结果 |
|---|---|---|---|
| RD-R3-01（①，RD04-FACT-01，P2） | 文档该变体解析路径已改为 `new(DTB, PhysBytes(dtb_phys))` | `sed -n '2472p' <doc>` → 「`PlatformDescSource::new(DTB, PhysBytes(dtb_phys))`（len=0，不带长度；与 boot-shim 生产腿的 `with_optional_len` 不同）」；`sed -n '110p' os/qemu-tests/test-kernels/kernel/bootstrap/test-smp-topo-riscv64/src/main.rs` → `let source = PlatformDescSource::new(DTB, PhysBytes(dtb_phys));` | PASS |
| RD-R3-05（②，DESIGN-D2-01..03，P2） | early-console 描述链死抽象、无活消费者；延期 R3-design-batch（D-070） | `grep -rn "early_console\|ConsoleDesc" os/libs/minix-platform/src os/libs/minix-boot/src` → 声明/转发集中于 `global.rs:69-70/151-157`、`platform.rs:356/448`；唯一其他引用 `global.rs:319` 位于 `#[cfg(test)]` 的 `test_qemu_virt_dispatch_via_enum` —— 生产代码零活消费者成立 | PASS |
| RD-R3-08（④，READER-A-01，P2） | §4.2 authority 过滤句已改为可解析机制描述 | `<doc>:1644` 含「`audit_source_coverage_for`（`memmap_source.rs:361`）在 UEFI 腿只以 `UefiMemoryMap` …机制见 `memmap_source.rs:354-360` 的注释」；`sed -n '354,361p' os/libs/minix-boot/src/memmap_source.rs` → 注释原文（"Only the memory-map records are authoritative…"）与 `pub fn audit_source_coverage_for(` 均在 | PASS |
| RD-R3-06（SUGG，DESIGN-D2-04..06，P3） | `Frozen::freeze` 防重闸是 `debug_assert!`；已登记 suggestions | `sed -n '227,228p' os/libs/minix-types/src/types/cell.rs` → `pub fn freeze(&self, value: T)` + `debug_assert!(`；suggestions.md R3 段含 DESIGN-D2-04..09（随 R3-design-batch 评估） | PASS |
| RD-R3-27（FP，CLAIM-11..13，P3） | 反证失败（主张成立）：aarch64 两段独立循环、DTB GUID 在前 | `sed -n '234,253p' os/boot-shim/src/uefi_helpers.rs` → :234 `DEVICE_TREE_GUID \|\| LEGACY_DEVICE_TREE_GUID`、:242 `break`、:246 第二段 `for`、:247 `ACPI2_GUID \|\| ACPI_GUID` —— 两段独立循环、DTB 在前，与文档一致 | PASS |

抽查 5/5 PASS；两处 FAIL 均为记录层（R3-4），不在抽查行的事实内容里。

## R3-7. 非阻断观察（不计入 FAIL）

1. 「12 处文档修改点」的枚举列出 13 个点（两个 §5.2 点是否合并计一处未写明）；certify 恰 12 条且无「三变体构造器差异说明」专条——loop.md §8.3 已自报 certify 覆盖依赖其第 9 条。
2. metrics R3「重构: 提出 3 / 接受 3 / 实施 0 / 登记 3」无逐条映射；数量与 ② 3 条（D2-01/02/03 延期登记）吻合，属推断，未独立验证。
3. FREEZE R3「四篇中唯一首轮即 PASS」：`runs/01-stage-kernel/` 下仅 02/03/04 三个 run 目录有 `round3-certify.md`；第四篇的制品不在本篇允许读取范围内，未验证。

## R3-8. 局限声明

- **同 agent 审计**：本审查与 FREEZE/面板/修复同模型；全部命令与输出已列出，可第三方复算。
- **只读**：除本文件追加节外未修改任何文件。
- **允许清单约束**：未读 `prompt/` 规范全文与兄弟 run 的 loop/ledger 正文（仅目录清单），FAIL-1 的「15」来源与「四篇」计数只能列出不一致、不能定位成因。
- **测试未重跑**：R3 无代码改动（loop.md §5 声明），本审计未重跑 cargo 测试与 QEMU；六门 + 三轮轨迹门 + panel-index 哈希门全部机械重放。
- 「自引入 0」「重复合并 0」按 70 唯一 ID 与面板形态对账验证；语义级自引入（同根因换皮）无法机械排除。
- certify 12 条未逐字重放（其读数与六门独立成立）；抽查覆盖五类处置各 1 行，非全量复验。

## R3-9. 收口条件

维持冻结记录一致性需完成两件小事：① 把 FREEZE.md R3 节与 loop.md R3 §2 的严重度行改为 P2=11/P3=58（或给出 15/54 的可复算口径）；② 把 READER-B-04/05/06/07、READER-C-04 五条补进 suggestions.md R3 段，或更正 defect-ledger.md RD-R3-14/15/22 的复验列。完成后本审计的全部读数即与 FREEZE 记录一致，可改判 PASS。

## R3 回归审计的补修记录（主 agent，append-only）

上面是独立审计者的原始结论（FAIL，2 条记录层不一致）。按 §4h 已全部处置：

| # | 审计发现 | 处置 | 复验 |
|---|---|---|---|
| 1 | FREEZE/loop 写 P2=15/P3=54，台账实为 P2=11/P3=58 | 两处改为 11/58 | 台账逐 ID 聚合即审计者实测 |
| 2 | RD-R3-14/15/22 的复验列声称已登记 suggestions.md，实际 5 条 finding 形态 P3（READER-B-04..07、READER-C-04）漏登 | 补登 suggestions.md 的 R3 段（4+1 条） | suggestions.md R3 段现含 45 条（34+4+1+6） |
| 附 | 「12 处修改点」实为 13 | FREEZE/loop 改 13 | 枚举清单 13 项 |

补修后实测：`--round R1/R2/R3` 三轮 PASS；anchor 77/77、citation 68/0、panel-index 6 文件哈希一致。
