# y01b 文档独立复验（D01 post-DESIGN-02 R1）

VERDICT: PASS

- 时间：2026-11-23（本机 CST）
- 目标文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 文档 sha256：`07e3a18156f3a83574ea4a22ba2a57e0af8499d07277e4afad541d0bc2a13c18`
- 行数：2952
- 修复 commit：`bb3b6e56a0f71d51e11955cfc66b5f5be9d7c5e5`（`bb3b6e56a`，2026-11-23 05:42:20 +0800）
- 工作区状态：`git diff --stat HEAD -- <doc>` 为空（目标文档无未提交改动），复验前后文档 sha 不变。复验期间（05:46–05:51）工作区出现并发写入：`…/r1-post-design02/loop.md`、`…/r1-post-design02/metrics.md`、`rewrite-notes/meta/review/events.csv`、`rewrite-notes/meta/review/state/01-stage-kernel/01-boot-shim-bootstrap.yaml` 被修改，并新增未跟踪目录 `…/r1-post-design02/unit-evidence/`；另有两条与本篇无关的未跟踪文件 `rewrite-notes/01-stage-kernel/01-design-learn.md`、`rewrite-notes/meta/coordination/review-eval/meta/01-boot-shim-bootstrap-r26.md`。这些改动不是本 verifier 所做（本 verifier 只写本报告，其余输出均在 `/tmp` 与 `os/target/` 构建产物）。
- 台账：`rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r1-post-design02/defect-ledger.md`（88 行）
- 面板 ID 覆盖：面板 36 个 FINDING ID（DESIGN 9、FACT 1、READER-A 16、READER-B 4、READER-C 6）全部在台账中出现；`comm -23` 差集为空。

结论口径：主表 36 行全部按 `复验` 命令执行并命中；其中 R1-34 的命令命中但 §3.7 的 v1 链仍漏一次复核（见 N-1），因此记 PASS 并附新发现。修复未引入 P0/P1/P2 级新矛盾，存在 3 个 P3 级文字问题。

## 逐根因

下表逐行给出：台账行、复验命令的关键读数、判定。所有 `rg` 命令的工作目录均为仓库根，`DOC` = `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`。

| 行 | 复验命令与关键输出 | 判定 |
|---|---|---|
| R1-01 | `rg -n '接收侧统一走 \`store_boot_report\`\|五条腿的发送侧都填 \`BootReport\`' DOC` → exit 1（0 命中）；`rg -n '统一经 \`land_and_store\`\|四个 \`fill_\*` 调用' DOC` → 命中 1206（`接收侧统一经 land_and_store`）、1848（`四个 fill_* 调用`） | PASS |
| R1-02 | `rg -n '两个容易同名的事件\|包装调用次序相反' DOC` → 命中 1846，同段含两个事件定义与“只指包装调用”的限定 | PASS |
| R1-03 | `rg -n '住在发送入口自己的栈帧' DOC` → 命中 1967；`rg -n '静态区（\`.data\`' DOC` → exit 1（0 命中）。代码侧三处生产调用均为栈局部：`uefi_helpers.rs:1086`、`uboot_main.rs:475`、`bootface_common.rs:354` | PASS |
| R1-04 | `rg -n "KernelInfo<'static>" os DOC --glob '*.rs' --glob '*.md'` → exit 1（0 命中） | PASS |
| R1-05 | `rg -n 'validate_before_tables\(authority\)' os DOC` → exit 1（0 命中）；`cd os && cargo test -p minix-boot --lib` → `test result: ok. 188 passed; 0 failed; 0 ignored` | PASS |
| R1-06 | `rg -n 'fn validate_before_tables\|fn validate\(' os/libs/minix-boot/src/kernel_info.rs` → 仅 `260: pub fn validate(&self)`（KernelInfo 非 strict，保留）与 `618: pub fn validate_before_tables(&self)`（BootReport）；strict 入口 `KernelInfo::validate_requiring_sources`、`BootReport::validate_requiring_sources` 存在；`KernelInfo::validate_before_tables`、`BootReport::validate` 均不存在 | PASS |
| R1-07 | `rg -n 'param_ptr' os/kernel-image/src/bootface_common.rs` → exit 1（0 命中） | PASS |
| R1-08 | `rg -n 'test_kernel_info\(' os/boot-shim/src` → exit 1（0 命中）；`cd os && cargo test -p boot-shim --lib` → `136 passed; 0 failed` | PASS |
| R1-09 | FP 处置核对：`rg -n 'repr\(C\)' os/libs/minix-boot/src/platform.rs` → 命中 123；16 B 断言 `const _: () = assert!(size_of::<PlatformDescSource>() == 16);` 保留；文档 1288 的例外句与现状一致（“只为钉住 16 B 句柄形状”） | PASS（FP 维持） |
| R1-10 | `rg -n '新增的 \`GET_KMESSAGES\`/\`VmBootHandoff\`' DOC` → 命中 1256 | PASS |
| R1-11 | `rg -n '只含七个字段（五个有读者' DOC` → 命中 1256 | PASS |
| R1-12 | `rg -n 'UEFI 腿五类归 \`BootStorage\`，source 分区直写 wire' DOC` → 命中 82（§1.2 I3 行） | PASS |
| R1-13 | `rg -n '\`land_slice\` 的指针相等检查' DOC` → 命中 2462 | PASS |
| R1-14 | `rg -n '尚未生成' DOC` → exit 1（0 命中） | PASS |
| R1-15 | `rg -n '\*\*\`memmap\`\*\*：可分配候选区间表' DOC` → 命中 48（§1.0 术语），`reserved_regions` 术语同段在 49 | PASS |
| R1-16 | `rg -n '三形态 v1 交接' DOC` → exit 1（0 命中）；1152 用“三种形态”表述且区分 v1/直核来源 | PASS |
| R1-17 | `rg -n '本节的“发布”指把落地后的 \`KernelInfo\` 写进' DOC` → 命中 1846，同段说明 VM 自由页发布是另一件事 | PASS |
| R1-18 | `rg -n '在生产腿的 strict 门里' DOC` → 命中 1336，同段补“非 strict 只在 source_records 非空时复核” | PASS |
| R1-19 | `rg -n '直核面前置 pre-table 与 postconditions' DOC` → 命中 2341 | PASS |
| R1-20 | `rg -n '三次 \`PT_ALLOC\` 注册与一次 VM \`PT_FREE\`' DOC` → 命中 1469；下表三行口径自洽（初始化 0 次 + 两二进制各 1 次 + VM 1 次） | PASS |
| R1-21 | `rg -n '本篇出现的"7→8"、"8→9"与"9→10"' DOC` → 命中 1288 | PASS |
| R1-22 | `rg -n '实现侧增量见 §4.7 的 \`cut_memmap\` 行' DOC` → 命中 2464 | PASS |
| R1-23 | `rg -n '引导期页表页见 \`alloc_pagetable\`' DOC` → exit 1（0 命中）；§4.7 的 Rust 列为 `alloc_pt_page`（2535） | PASS |
| R1-24 | `rg -n '见 §2.4.2 的(”\|“)?还' DOC` → exit 1（0 命中） | PASS |
| R1-25 | `rg -n '### 3.4 bootstrap 区间、分配器换手与页目录槽位' DOC` → 命中 1338 | PASS |
| R1-26 | `rg -n '本节按四个读者任务分组' DOC` → 命中 1221 | PASS |
| R1-27 | `rg -n '取页方向与跳板生命周期为什么只算观测常态' DOC` → 命中 731 | PASS |
| R1-28 | `rg -n 'snprintf\(cmdline, len, "%s %s", file, args\)' DOC` → 命中 231 | PASS |
| R1-29 | `rg -n '仅 UEFI 与 U-Boot 两族' DOC` → 命中 95 | PASS |
| R1-30 | `rg -n '落盘之后再消费' DOC` → 命中 1152（v1 解码落盘 / 直核借用落盘已分述） | PASS |
| R1-31 | `rg -n '四个 \`fill_\*` 调用' DOC` → 命中 1848 | PASS |
| R1-32 | `rg -n '审计里的 \`kind = Memory\` 指 \`/memory\`' DOC` → 命中 1334 | PASS |
| R1-33 | `rg -n '物理地址是值，可以跨镜像' DOC` → 命中 85（同段限定 I1 禁止发送侧虚拟地址/进程局部句柄） | PASS |
| R1-34 | `rg -n '\`kmain\` 的 \`validate\` → \`check_boot_postconditions\` 复核' DOC` → 命中 1850（直核链）与 2380（v1 链）；QEMU 空模块豁免核对：2394 行写“空模块表由 `validate_before_tables` 放行”，代码 `ValidationLevel::requires_boot_modules()` 仅在非 `BeforeTables` 时为真（`os/libs/minix-boot/src/kernel_info.rs:169-172`）。残留：§3.7 的 v1 链未列该复核（N-1） | PASS（附 P3 N-1） |
| R1-35 | `rg -n '正常分配下反序时' DOC` → 命中 243、869；两处均补“模块表分配失败则停在 HAS_MODS 断言” | PASS |
| R1-36 | 三载体构建：`cargo build -p test-higher-half --target x86_64-unknown-uefi --features fw-x86-uefi` exit 0；`...-aarch64 --target aarch64-unknown-uefi --features fw-aarch64-uefi` exit 0；`...-riscv64 --target riscv64gc-unknown-none-elf --features fw-riscv64-none` exit 0。release 同三条命令亦 exit 0。三支 QEMU 门在 HEAD 重跑：`bash os/qemu-tests/run_qemu.sh x86_64 <release efi>` → exit 0，`### TEST_RESULT: PASS test-higher-half ###`；aarch64 → exit 0，同 PASS 行；riscv64 → exit 0，同 PASS 行。原 `tmp/review-loop/01-stage/evidence/y01b-qemu-*.log` 三份也均为 PASS | PASS |

### 任务点 3 的重点项复核

| 重点项 | 证据 | 判定 |
|---|---|---|
| §4.0/§4.4 不再说“接收侧统一走 `store_boot_report`”；v1 明写 `store_kernel_info`，直核明写 `store_boot_report`，统一到 `land_and_store` | §4.0 小结 1894；§4.4 开头 2290；反例检索 0 命中 | PASS |
| §3.7 区分“解码落盘”与“`store_kernel_info` 包装调用” | 1846 行完整定义两个事件，并把“store 与 boot_validate 次序相反”限定为包装调用 | PASS |
| §4.2 BootStorage 位置为发送入口栈帧 | 文档 1967；代码三处 `let mut storage = BootStorage::new()` 均为入口栈局部，全仓无 `static ... BootStorage` | PASS |
| §3.2 报告视图为九字段（无 authority） | 文档 1206；代码 `BootReport` 9 个字段（`boot_storage.rs:292-311`），`KernelInfo` 10 个字段，`KernelInfoView` 9 个方法（`kernel_info.rs:186-196`） | PASS |
| §4.4 的 `source_records` 行区分 UEFI 直写 wire | 文档 2330：直核与 U-Boot shim 填 `BootReport`；UEFI 两阶段直写 wire、storage 切片为空 | PASS |
| §3.7 `fill_*` 计数 = 4 次调用 / 5 类落点 | 1848；UEFI 腿 4 次调用（`fill_modules`/`fill_platform`/`fill_param`/`fill_map_tables`，其中 `fill_map_tables` 提交两表），`fill_source_records` 仅 U-Boot shim | PASS |
| 旧符号清零 | `KernelInfo<'static>` 0；`validate_impl_inner` 0（doc+os）；`尚未生成` 0；Rust 列 `alloc_pagetable` 0（仅 C 侧列/正文存在） | PASS |
| kmain 链第二次 `check_boot_postconditions` | §4.4.1 2380 有；代码 `os/kernel/src/lib.rs:1873` 在 `kmain` 的 `validate()` 后调用；§3.7 的 v1 链未列（N-1） | PASS（附 P3） |
| QEMU 空模块豁免挂回 `validate_before_tables` | 2394；代码 `requires_boot_modules()` 仅 `BeforeTables` 为假 | PASS |

## 机械门

命令均在仓库根执行，`DOC` 同上；输出完整日志留在 `/tmp/y01b-gate-*.log`。

| 门 | 命令 | 读数 / 结果 |
|---|---|---|
| anchor-resolve | `bash tools/anchor-resolve.sh --check DOC` | exit 0；`anchors=86 resolved=86 zero-def=0 multi-def=0` |
| doc-code-map | `bash tools/doc-code-map.sh DOC --check` | exit 0；`（共 49 个文件；存在性 49/49 通过）` |
| doc-ref-check | `bash tools/doc-ref-check.sh --check DOC` | exit 0；`✅ … 内部章节引用均可解析` |
| doc-style-lint | `bash tools/doc-style-lint.sh DOC` | exit 0；`命中汇总：\| error级=0` |
| doc-publication-check | `python3 tools/doc-publication-check.py DOC` | exit 0；`无新增违规` |
| tutorial-rubric-check | `python3 tools/tutorial-rubric-check.py DOC` | exit 0；`检查 6 章，0 章缺项` |
| doc-citation-check | `python3 tools/doc-citation-check.py DOC` | exit 0；`blocks=54 checked=20 failed=0 skipped=30 warned=4` |
| doc-code-density-check | `bash tools/doc-code-density-check.sh --check DOC` | exit 0；可判承载/诊断章 4 个均有码/命令，`✅ 承载门阻断项通过`；豁免 0 |

## 新发现

N-1（P3，与 R1-34 相关）：§3.7 的 v1 接收链仍漏第二次 `check_boot_postconditions`。
- 位置：`DOC:1850`。直核链写成 `… → kmain 的 validate → check_boot_postconditions 复核`；紧随其后的 v1 链止于 `kmain 的 validate`，没有复核。
- 证据：同篇 §4.4.1 的权威表（`DOC:2380`）明确给 v1 链加了 `→ check_boot_postconditions 复核`；代码 `os/kernel/src/lib.rs:1873` 在 `kmain` 里 `kernel_info.validate()` 之后无条件调 `check_boot_postconditions()`，适用全部五条启动形态。
- 最小修复：在 1850 行 v1 链末尾补 `→ check_boot_postconditions 复核`，或改为“v1 同直核面在 kmain 复核”。

N-2（P3）：§4.0 的“落地区”术语仍把深拷归给 `store_kernel_info`。
- 位置：`DOC:1900`（“`store_kernel_info` 把 wire 解码后的切片拷进去”）；`DOC:1894` 的“解码后由 `store_kernel_info` 落 `.bss`”同样偏松。
- 证据：同篇 §3.7（`DOC:1846`）与 §4.5（`DOC:2462`）已定为：v1 分区在 `arch_boot_resume_v1` 解码时先落 `.bss`；`store_kernel_info` 是之后对同址切片的包装调用，`land_slice` 因指针相等跳过复制。当前术语会让读者以为复制发生在包装调用里。
- 最小修复：改为“v1 接收侧由 `arch_boot_resume_v1` 解码写入；`store_kernel_info` 之后对同址切片作包装（跳过复制）”。

N-3（P3，存量、非本批引入）：`DOC:119` 的“编号约定”段仍写“`I-<nn>` 是评审发现号，`D-<nnn>` 是裁决号，`D2-<nn>` 是设计批编号，`R02-T<nn>` 是测试缺口编号；它们只用于回查台账”。这是正式文档中的过程编号与台账指称。
- 证据：`python3 tools/doc-publication-check.py DOC` 报“无新增违规”（该段在基线内），说明它是存量而非本批新增；`git show bb3b6e56a` 未改该行。
- 最小修复：整段移入 `rewrite-notes/meta/` 或删除；若保留“回查台账”能力，改为不出现过程编号的读者指引。

无 P0/P1/P2 级新发现。

## 未覆盖

- 面板 SUGGESTION（READER-A-S01..S05、READER-B-S01..S13、READER-C-S01..S07、P-FACT SUGGESTION）只按台账“处置”列做了落点抽读（§1.3 回指、§1.0 术语、§3.5.1 角色一/二、§5.5 标题、§4.5 删除记录、§4.6 无归档日志、§3.4.2 源码核对标注、mmap_size 措辞等），未逐条与原始 SUGGESTION 文本做双向对账。
- `doc-citation-check` 的 `skipped=30 / warned=4` 覆盖缺口：15 个 C/asm/ld 多行 `/* … 代码引用/节选，path。 */` 标注不被工具识别（FACT-01 已记录并称人工逐行对过）。本轮通读文档时未把这 15 块逐行与 C 源码重新 diff。
- R1-04..R1-08 的代码侧只跑了针对性 `rg`、`minix-boot`/`boot-shim` 两个测试套件与三支 QEMU 载体；未跑全 workspace `cargo test`/`cargo clippy`，也未复核 `os/README.md`、`Cargo.toml` 等附带改动。
- QEMU 原始证据日志头写的是 `01dcb5252`（修复提交前的工作区状态）。本轮已在 HEAD 重建 release 并重跑三架构门补上该缺口；原始日志与重跑日志的逐行输出未做 diff。
- 面板报告正文（reader-a/b/c、design-d2、fact、refute）只核对了 ID 覆盖、P1/P2 结论与关键代码锚点，未复算其全部 36 条 FINDING 的事实细节。

## 复检（f707a4e5）

RECHECK: PASS

- 时间：2026-11-23（本机 CST）
- 文档 sha256：`f707a4e5a38b5a4544592231ea724215a06bc1179fd4f25ee41db47afc1f5e3d`（原 `07e3a181…`）
- 行数：2951（原 2952；删除 1 行）
- HEAD 仍为 `bb3b6e56a`；文档相对 HEAD 为未提交修改：`git diff --stat HEAD -- <doc>` = 1 file changed, 3 insertions(+), 4 deletions(-)。
- 范围：只复核 3 处 P3 修复与受影响的前后文，不重核 36 个根因。

### 3 处修复逐项

| 修复 | 位置（新 sha 行号） | 证据 | 判定 |
|---|---|---|---|
| ① §3.7 的 v1 链补第二次 `check_boot_postconditions` 复核 | 1849 | 新文本：`… → store_kernel_info → kmain 的 validate → check_boot_postconditions 复核，不经过 validate_before_tables`；与 §4.4.1 权威表 2379 及代码 `os/kernel/src/lib.rs:1873`（`kmain` 的 `validate()` 后调 `check_boot_postconditions()`）一致 | PASS |
| ② §4.0 小结与“落地区”术语改为“v1 解码进 `.bss`，`store_kernel_info` 只做同址包装” | 1893、1899 | 1893：“三条 v1 腿经 `arch_boot_resume_v1` 解码进 `.bss`，随后由 `store_kernel_info` 包装调用（同址跳过复制）”；1899：“v1 腿由 `arch_boot_resume_v1` 解码写入，`store_kernel_info` 只做同址包装”。与 §3.7 的 1845、§4.5 的 2461（“源已是前一步解码落进 store 的切片（同址，跳过）”）一致 | PASS |
| ③ 删除 §1.4 的“编号约定”过程编号段 | 原 119 行处（新 119 行前为 D1–D5 段） | `git diff` 显示整段删除；`rg '编号约定\|R02-T\|D2-[0-9]\|I-[0-9]{2}\|D-[0-9]{3}' <doc>` 0 命中；D1–D5 的定义句仍在同节，未产生悬空编号。残留的“字段台账”均指 wire/字段清单这一实质表（1050、1054、1141、1222、2610），不是过程台账 | PASS |

### 受影响前后文检查

- §1.4 删除处：前一段 D1–D5 定义完整，后接 `---` 与第 2 章；无空段、无孤立引用。
- §3.7：1845 的两事件段、1847 的所有权段未受改动；1849 的直核链与 v1 链现在都含 `kmain` 后的复核，且都不与 2379 冲突。
- §4.0：小结 1890–1894 与术语 1897–1902 相邻自洽；1894“六个落地区把切片深拷进内核 `.bss`”是两族共性陈述，不与 1893/1899 的按腿分工冲突。
- 未改动段落中的相近表述核对：2289（`store_kernel_info` 包装）、2461（同址跳过）、1788（解码进六个 `.bss` 落地区）均与改后口径一致；1788 的“解码进 `store_kernel_info` 的六个落地区”措辞偏松，但不构成事实矛盾，未列入新发现。

### 机械门（重跑）

| 门 | 命令 | 读数 / 结果 |
|---|---|---|
| anchor-resolve | `bash tools/anchor-resolve.sh --check <doc>` | exit 0；`anchors=86 resolved=86 zero-def=0 multi-def=0` |
| doc-publication-check | `python3 tools/doc-publication-check.py <doc>` | exit 0；`无新增违规` |
| doc-ref-check | `bash tools/doc-ref-check.sh --check <doc>` | exit 0；内部章节引用均可解析 |
| doc-citation-check | `python3 tools/doc-citation-check.py <doc>` | exit 0；`blocks=54 checked=20 failed=0 skipped=30 warned=4`（与复检前同读数） |
| doc-style-lint | `bash tools/doc-style-lint.sh <doc>` | exit 0；`命中汇总：\| error级=0` |

### 复检结论

3 处 P3 修复全部落地，受影响前后文无新矛盾；5 个机械门读数与修复前一致（citation 的 `skipped=30` 是既有工具识别缺口，非本次改动引入）。本轮未产生新的 P0/P1/P2/P3 发现。

遗留提示（不改变 RECHECK 结论）：文档相对 HEAD 尚未提交；若后续以提交固化，需把新 sha 与 3 处改动一并纳入同一 commit，避免 certify 记录与工作区状态脱节。

## RECHECK2

RECHECK2: PASS

- 时间：2026-11-23（本机 CST）
- 文档 sha256：`19c0ad9a306cc1e950b657ad6c6d1c34f3a0aa18256f238b048e7dbd9efc83dc`；行数 2951（与 RECHECK 同，未增删行）。
- 变更范围：仅 `DOC:2379`（§4.4.1 首段）直核链在 `kmain validate` 后补 `→ check_boot_postconditions 复核`；`git diff HEAD` 对该文件只有这一行的一处替换。
- 对照结果：
  - §4.4.1 新直核链：`… → boot_validate（1，arch_boot_impl 开头）→ kmain validate → check_boot_postconditions 复核`；v1 链末尾也含 `… → kmain 的 validate → check_boot_postconditions 复核`，两条链现在对称。
  - §3.7（`DOC:1849`）的直核链与 v1 链均已含 `kmain` 后的复核，与 §4.4.1 改后文本一致。
  - 代码：`os/kernel/src/lib.rs:1864` 为 `kernel_info.validate();`，`:1873` 为 `kernel_info.check_boot_postconditions();`；`:1868-1872` 的注释明确说明 `kmain` 是五条启动形态的共同终点（“kmain is the single common end point of all five boot forms”），因此在 `kmain` 补复核对直核与 v1 都成立。
- §4.4.1 同节其余文字（编号说明“共十三行…”、下表按门列 bug 类）不描述链路顺序，未受影响；未发现新矛盾。
- 本轮按要求未重跑机械门。
