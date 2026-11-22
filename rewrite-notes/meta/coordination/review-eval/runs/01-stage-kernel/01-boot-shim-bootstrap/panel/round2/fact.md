# P-FACT round2 —— 事实与合同复核

> 角色：独立事实审查者（P-FACT）。只读，未修改仓库任何文件。
> 输入：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（2973 行）、`runs/01-stage-kernel/01-boot-shim-bootstrap/related-code.txt`（48 文件）。
> 未读：历史 panel/round1、defect-ledger、archive、expected/。范围按协调者指示收窄到机械门 + R1 四项修复 + 高价值事实抽查。

## 一、机械门读数（L1：工具输出）

| 门 | 命令 | 输出摘要 | 判定 |
|---|---|---|---|
| 锚点 | `tools/anchor-resolve.sh --check <doc>` | `anchors=90 resolved=90 zero-def=0 multi-def=0` | PASS |
| 关联代码 | `tools/doc-code-map.sh <doc> --check` | `（共 48 个文件；存在性 48/48 通过）`；与 `related-code.txt` 的 48 行一致 | PASS |
| 结构 | `tools/doc-structure-check.py <doc>` | `⛔ ...:2639: 表格块第 2 行不是分隔行——表可能被空行劈成两半，或表头缺失`；`1 处问题（1 个文件）` | **FAIL** → FACT-01 |
| 内部引用 | `tools/doc-ref-check.sh <doc>` | `✅ 内部章节引用均可解析` | PASS |
| 文风 | `tools/doc-style-lint.sh <doc>` | `命中汇总：\| error级=0` | PASS |
| 引文 | `tools/doc-citation-check.py <doc>` | `blocks=54 checked=21 failed=0 skipped=29 warned=4` | PASS（0 失败；4 条 warning 未逐条展开，见 SUGGESTION-03） |
| 承载门 | `tools/doc-code-density-check.sh <doc>` | `✅ 承载门阻断项通过（4 个可判章均有码/命令）` | PASS |

## 二、R1 修复项的独立复核

### 2.1 §3.4.2 `pt_alloc` 引文块 —— 已修，与现行源码逐字一致

- 文档 1405–1435 行。逐行对照 `os/arch/src/arch/pt_alloc.rs`（L39、L45–48、L55–67、L70–72、L80–92）：
  - `type PtAllocFn = fn() -> Result<(PhysBytes, VirBytes), PageTableError>;` ✓
  - `static PT_ALLOC: AtomicUsize = AtomicUsize::new(0);` / `static PT_FREE: AtomicUsize = AtomicUsize::new(0);` ✓
  - `register` 用 `compare_exchange(0, alloc_fn as usize, Ordering::AcqRel, Ordering::Acquire)` + `panic!("pt_alloc::register called twice")` ✓
  - `is_registered` / `alloc_pt_page`（含 `assert!(raw != 0, "pt_alloc::alloc_pt_page called before register")` 与 transmute SAFETY 注释）✓
  - 块首标注 `// 代码引用（块内中文注释为译述；... source-symbol: register/alloc_pt_page）`，唯一被译述的是 `register` 内三行注释；英文 SAFETY 注释逐字。符合省略标注纪律。
- 阴性声明：`grep -n "PT_CLAIMED\|PT_REGISTERED\|PtAllocSlot" <doc>` → 0 命中（exit 1）。旧稿的三段式槽位模型在正文、引文块与推演段均已不存在。
- 结论：本项无发现。

### 2.2 §4.6 注册推演 —— 与源码一致

- 文档 1462–1470 行"一次初始化、三次注册"与 2496–2539 行 `init`/`resume` 引文对照：
  - `os/kernel/src/boot_alloc.rs` L54–64 `init`、L78–89 `resume` 与文档 2500–2525 行逐字一致（断言消息、Ordering 均一致）✓
  - `os/boot-shim/src/uboot_main.rs` L476 `prepare_boot(...)` → L482 `init_boot_pt_alloc(result.bump.start().0, result.bump.end().0)`，与文档 1466 行"U-Boot 腿写成 `result.bump.start()`/`result.bump.end()`"一致 ✓
  - `boot_validate_and_prepare` 内 `is_registered()` 守卫 + `register`：文档 1467/2539 行；源码 `os/kernel/src/lib.rs` 的 `boot_validate_and_prepare` 段落与 `pt_alloc` 的 `compare_exchange` 语义一致 ✓
- 结论：本项无发现。

### 2.3 §4.4 authority（`RamAuthority::DTB`/`UEFI`）—— 主体成立，一处符号级不准确

- 直核面：`os/kernel-image/src/bootface_common.rs:558` `build_kernel_info(&report, minix_boot::RamAuthority::DTB);` → 文档 2313 行"两个直核面传 `RamAuthority::DTB`" ✓
- 类型定义：`os/libs/minix-boot/src/memmap_source.rs:606` `pub struct RamAuthority(InputObjectId);`，常量 `DTB`/`UEFI` 在 L608–611 附近 → 文档 1179 行"`RamAuthority` 内部包一个 `InputObjectId`；`InputObjectId` 现在只有 DTB/UEFI 两个变体" ✓（`InputObjectId` 变体名 `DtbBootInput`/`UefiMemoryMap`，tag 0/1，与文档 1284 行编注"0 = boot DTB、1 = UEFI 内存图"一致）
- **但**：`Serializer::new` 的第二个参数类型是 `InputObjectId`（`os/libs/minix-boot/src/handoff_v1_write.rs:112-115`），UEFI 生产调用点传 `InputObjectId::UefiMemoryMap`（`os/boot-shim/src/uefi_helpers.rs` 的 `Serializer::new(arena, minix_boot::InputObjectId::UefiMemoryMap)`），U-Boot 传 `InputObjectId::DtbBootInput`（`uboot_main.rs:491`）。`RamAuthority::UEFI` 出现在 `ebs_pair.rs:427` 的测试辅助 `write_min_envelope` 里，不是发送侧 `Serializer` 调用。→ 见 FINDING FACT-02（文档 2313 行）。
- 结论：机制描述成立，单点符号不准确。

### 2.4 `minix-boot` 188/0 —— 数量吻合

- `cd os && cargo test -p minix-boot --lib -- --list` → `188 tests, 0 benchmarks`。文档 §3.0 最小示例、§5.0 最小示例、§5.2 均称 `188/0`；数量 188 吻合。通过数 0 失败未实跑（见阴性声明）。
- 顺带核到：`cargo test -p boot-shim --lib -- --list` → `136 tests`，与 §5.2 的 `136/0` 数量吻合；`os/libs/minix-elf/src` 的 `#[test]` 静态计数 27，与 §5.2 的 `27/0` 吻合。

### 2.5 编号 `2.4.2.5` 与 `3.2.2.4` 唯一性 —— 成立

- `grep -c "2\.4\.2\.5" <doc>` → `1`；`grep -c "3\.2\.2\.4" <doc>` → `1`。两者各只出现在各自标题行（L727、L1257），正文无第二处引用，不存在重号。

## FINDING FACT-01（P1）

- 位置：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §5.1（L2629–2640），断点在 L2636。
- 严重度：P1（结构门直接 FAIL；表格在 Markdown 渲染下被劈成两张，I2–I4 三行失去表头）。
- 问题：L2635 的 I1 行之后、L2638 的 I2 行之前，L2636 插入了一行无 `|` 的正文"门覆盖不等于成功路径覆盖；逐行读"不证明什么"列，尤其 aarch64 直核只到 SMP 交付。"，把同一张四列证据矩阵截断；L2638 起的三行成为"无表头表"。
- 影响：`doc-structure-check.py` 报 FAIL（见上表）；读者在 §5.1 看到 I2–I4 三行时列语义错位（第一列被当成表头），证据矩阵的可读性受损。这条正文本身内容有价值，不该删。
- 证据或推理：E1（工具输出）：`⛔ ...:2639: 表格块第 2 行不是分隔行`，且 `awk 'NR>=2633&&NR<=2640'` 显示 L2633–2635 是表头+分隔+I1、L2636 无竖线、L2637 空行、L2638–2640 是 I2–I4。
- 最小修复：把 L2636 整行移到表格**之后**（L2640 之后、L2642 `### 5.2` 之前）作为表后说明；或改为 `> ` 引用块并仍放在表后。改完重跑 `tools/doc-structure-check.py <doc>` 至 0 问题。
- 置信度：高。
- 是否需源码核实：否。
- 若我错了：若该行本意是 I1 行内的续行（表格内换行需 `<br>`），那目标仍是同一修法；无论意图如何，当前形态都过不了结构门。

## FINDING FACT-02（P2）

- 位置：文档 §4.4 L2313，`authority` 行"值从哪来"列末句："UEFI 腿的发送侧在 `Serializer` 里声明 `RamAuthority::UEFI`（§3.2 的绑定说明）"。
- 严重度：P2（字段表把调用点的实参符号写错；不影响设计结论，但读者按此名去 grep 找不到）。
- 问题：`Serializer::new` 的形参类型是 `InputObjectId`（`os/libs/minix-boot/src/handoff_v1_write.rs:112-115`），生产 UEFI 腿调用是 `Serializer::new(arena, minix_boot::InputObjectId::UefiMemoryMap)`；`RamAuthority::UEFI` 是读侧 `handoff_v1_read.rs:160-189` 从 `ram_authority` u32 恢复出的值（以及 `ebs_pair.rs:427` 测试辅助里的 `authority:` 字段），不是发送侧 `Serializer` 的实参。
- 影响：读者若按 `RamAuthority::UEFI` 去 `Serializer::new` 调用点核对，会以为文档符号写错整段机制；实际机制（wire 存 tag，读侧还原 `RamAuthority`）与 §3.2 的绑定说明一致，仅这一句的符号名错位。
- 证据或推理：E1（源码三处）：`grep -n "pub fn new" -A4 handoff_v1_write.rs` 显示形参 `authority: crate::memmap_source::InputObjectId`；`grep -rn "Serializer::new" os/` 的 UEFI/U-Boot 调用点为 `InputObjectId::UefiMemoryMap`/`InputObjectId::DtbBootInput`；`RamAuthority::UEFI` 命中 `ebs_pair.rs:427`（`write_min_envelope`，测试辅助）。
- 最小修复：改为"UEFI 腿的发送侧在 `Serializer::new` 里传 `InputObjectId::UefiMemoryMap`，接收侧解码为 `RamAuthority::UEFI`"（与 §3.2 L1179 的 U-Boot 表述同形：那里只写"声明 DTB 权威"，未错指类型）。
- 置信度：高。
- 是否需源码核实：是（已核）。
- 若我错了：若 `Serializer` 有第二个接收 `RamAuthority` 的构造入口（例如 `write_envelope` 的可选参数），则这句话可能指那条路径；但 `write_min_envelope` 是在测试里手工把 `authority` 放进 `KernelInfo` 载体，仍不是 `Serializer::new` 的实参。当前树 `grep -rn "RamAuthority::UEFI" os/boot-shim/src/` 仅 `ebs_pair.rs:427,848`（均在测试辅助段）。

## SUGGESTION

- SUGGESTION-01（P3）：§5.2 `minix-kernel --features mock --lib` 的读数写作 `866/0/8`，`AGENTS.md` 引用的 CI 基线（2026-10-12）是 `831 通过 / 0 失败 / 8 忽略`。两者忽略数一致、通过数差 35，最可能是文档读数取自更晚的一次本地运行或 AGENTS 基线过旧。建议：重跑一次并同时更新引用日期（或改写成"以 CI 当期读数为准"）。证据强度 E2（AGENTS 文本 + 文档文本），未实跑内核 mock 测试（成本高、且与 R1 复核项无关）。
- SUGGESTION-02（P3）：§2.1 L228 "实际模块数最多 19（最后一个槽，下标 19，要留给内核）"在模块数为 19 时确实只余下标 19 给内核，但句子里"最后一个槽"与"下标 19"并列容易被读成"模块自己能用到 19 号槽"。C 侧 `assert(mbi->mi_mods_count < MULTIBOOT_MAX_MODS)` 允许 19 个模块（下标 0..18），内核占 `k = mi_mods_count`。建议改为"模块下标最大 18，`k = mi_mods_count ≤ 19` 留给内核"。证据 E1（`pre_init.c:168,195`）。
- SUGGESTION-03（P3）：`doc-citation-check.py` 报 `warned=4` 而未在本次读数中展开。建议主 agent 复跑时把 4 条 warning 的具体命中贴入 scan.md，避免"0 失败"掩盖"节选标注不完整"类问题。

## 二、阴性声明（"扫了什么、发现数、N=0 不覆盖什么"）

| 检查 | 扫了什么 | 发现数 | N=0 时不覆盖 |
|---|---|---|---|
| 机械门 | 上述 7 个工具，整篇 | 1（FACT-01） | 工具不能判定的语义/引文细节 |
| R1 pt_alloc | 文档 1405–1435 与 `pt_alloc.rs` 全文逐行；旧符号 3 个 grep | 0 | 未核 `paging_ext.rs` 等调用方 |
| R1 注册推演 | `boot_alloc.rs` init/resume/next/used_bytes；`uboot_main.rs` 调用序 | 0 | 未核 VM 侧 `vm_pt_alloc`/`vm_pt_free` 实现体 |
| R1 authority | `build_kernel_info`、`buildface_common.rs:558`、`memmap_source.rs:591-630`、`handoff_v1_write.rs:112`、`handoff_v1_read.rs:160-189`、`Serializer::new` 全部调用点 | 1（FACT-02） | 未核 `validate_requiring_sources` 的完整实现 |
| 测试计数 | `--list`（minix-boot/boot-shim）、`#[test]` 静态计数（minix-elf） | 0 | **未实跑**：188/0 的"0 失败"、136/0、27/0、kernel mock 866/0/8 的通过数均未复跑 |
| 编号唯一性 | `2.4.2.5`、`3.2.2.4` 全文计数 | 0 | 未核其他 `x.y.z` 编号是否都唯一 |
| 常量抽查 | `BOOT_PT_BUMP_PAGES=1024`、`MEMMAP_REGION_STORE_LEN=128`、`RESERVED_REGION_STORE_LEN=256`、`BOOT_MODULE_NAME_LEN=16`、`PLATFORM_SOURCE_STORE_LEN=4`、`PARAM_BUF_STORE_LEN=1024`、`WIRE_SOURCES_END=29184`、`BOOT_HANDOFF_V1_VERSION=10`、`MULTIBOOT_MAX_MODS=20`、`MULTIBOOT_PARAM_BUF_SIZE=1024`、两个魔术值 | 0 | 容量表的 47/64/296 与 VM 侧 `VmBootHandoff` 字段未逐项核 |
| 引文抽查 | `head.S` multiboot 识别头与 `multiboot_init`、`pre_init.c` 内存收集段/断言区、`overlaps` 段、`kernel.lds` 锚点 | 0 | 全文 54 个引文块只抽了 C 侧 5 处 + Rust 侧 4 处，其余未逐字对 |

## VERDICT

NOT PASS（门级 FAIL 1 项：FACT-01；事实级 P2 1 项：FACT-02）。R1 抽查的四项（pt_alloc 引文块、§4.6 注册推演、authority 主体、188/0 与编号唯一性）全部成立，无回归；剩余问题是一处 Markdown 表格断裂与一处调用点符号名错位，均可小改收敛。SUGGESTION-01 的 866 vs 831 需一次实跑定谳，不阻断。
