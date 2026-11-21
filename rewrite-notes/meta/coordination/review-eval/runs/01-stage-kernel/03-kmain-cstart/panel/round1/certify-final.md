# D03 R1 最终认证（闭集）

- 文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
- 规模：1384 行
- 身份：`sha256 = 8b89945f9596717638adbea1e696fdf7d4c7867487ca5b3ee74705987ed3503f`（与任务给的 `8b89945f` 前缀一致；git blob 为 `06f018d7900b82d049507c2b40604a8c1f134640`）
- 范围：只核对下列 7 项改动句及邻句；未重扫全文，未读历史评审、其他面板与 expected。
- 日期：2026-11-12

## 一、逐条核对

| # | 核对项 | 结论 | 证据摘要 |
|---|---|---|---|
| 1 | §2.1 `local_cbi` 指向 `pre_init` 的 `.unpaged_bss` 暂存副本；代码块注释“副本身份关系” | **CLOSED** | 文档 L141、L144；C 侧 `pre_init.c` 定义 `kinfo`/`kmessages` 并 `return &kinfo;`（L20-21、L237）；`Makefile.inc` 对 `pre_init.o` 做 `--prefix-symbols=__k_unpaged_`（L51、L64、L67）；`kernel.lds:19` 把 `unpaged_*.o(.bss COMMON)` 收进 `.unpaged_bss`；`head.S` 把 `pre_init` 返回值压栈后 `call kmain`（L77、L87）；`usermapped_data.c:7,9` 定义 `.usermapped` 的 `kinfo`/`kmessages`。 |
| 2 | §2.1 删 `memcpy` 的失败点 = `pg_identity` 的 `assert(cbi->mem_high_phys)`（`pg_utils.c:170`） | **CLOSED** | 文档 L179；`minix3/minix/kernel/arch/i386/pg_utils.c:170` 原文即 `assert(cbi->mem_high_phys);`；`main.c:403` `cstart` → `main.c:411` `prot_init()` → `protect.c:361` `pg_identity(&kinfo)`。删两行后普通全局 `kinfo` 为零，`mem_high_phys == 0` 在该断言处触发（kernel 构建未见 `-DNDEBUG`；`minix3/minix/lib/libsys/assert.c` 的 `__assert13` 走 `panic`）。 |
| 3 | §5.4 M9 = 普通副本建第二张表并切 CR3（无“原地重填/TLB”） | **CLOSED** | 文档 L1275 已改为“普通副本建第二张表并切 CR3”；全文无 `TLB`；`原地重填` 只以否定形式出现在 L615（“不是同一数组的原地重填”）。C 侧：`Makefile.inc:51` 把 `pg_utils.o` 放进未分页改名名单；`kernel.lds:19` 的 `.unpaged_bss`；`pg_utils.c:254-256` 的 `memset(pagedir, …)`，`pg_utils.c:249-250` 以 `vir2phys(pagedir)` 写 CR3；`protect.c` 的普通 `pg_utils.o` 副本用 `.bss` 的另一个 `pagedir`。 |
| 4 | §3.7 3/5 差 = `CONFIG_SMP` 的 0xF1/0xF2 | **CLOSED** | 文档 L947 算式为 `39 − 16 + 64 + 3/5 = 90/92`；C 侧 `apic.h:90-92`：`APIC_TIMER_INT_VECTOR=0xf0`、`APIC_SMP_SCHED_PROC_VECTOR=0xf1`、`APIC_SMP_CPU_HALT_VECTOR=0xf2`；`apic.c:764` 的 `gate_table_ioapic` 为 64 条 0x50–0x8F，另含 0xFE/0xFF；`apic.c:843` 的 `gate_table_smp` 在 `#ifdef CONFIG_SMP`（L842）内；`apic.c:899` 仅在 SMP 下追加。差额 2 = 0xF1/0xF2，结论成立。附注：`+3` 的第三项是 0xF0 定时器门（`apic.c:913` 的 `int_gate_idt(APIC_TIMER_INT_VECTOR, …)`），句中只点名了 spurious/错误两项，计数本身不受影响。 |
| 5 | §3.5 `use ... as` 转出（无“重导出”） | **CLOSED** | 文档 L891 写“`IDENTITY_MAP_END` 通过 `use ... as` 转出 `BOOT_IDENTITY_MAP_END`”；`os/arch/src/lib.rs:217` 定义 `pub const BOOT_IDENTITY_MAP_END`；`os/kernel/src/lib.rs:79` 原文 `BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END`；`os/kernel/src/lib.rs:1224-1225` 的 Step 1 注释与文档引文一致（“C maps 4 GiB with `pg_identity`…the Rust cross-image leg only needs…”）。 |
| 6 | §4.1 “随后一步”、§5 `src/**/*.rs` | **CLOSED**（措辞）；同句计数见 RD3-CERTF-02 | 文档 L965 为“随后一步有 C 侧对应”，`minix_platform::init_from_kinfo` 在 `os/kernel/src/lib.rs:1758` 紧随 `store_kernel_info` 之后；文档 L1223 原文含 `` `src/**/*.rs` ``。计数问题单列为 P2。 |
| 7 | §1.2/§4.6/§3.8 “四类入口”，无“三张表与三个问题” | **REOPEN** | §1.2 标题（L38）与四行表（L44–L47）已用“四类入口”；§4.6 表头（L1176）与 syscall 行（L1180）已改。但 §1.2 正文仍有 L40“三份答案落在三张表上”、L55“它不回答上面三问中的任何一问”、L57“三张表与它们回答的三个问题”，与同节标题、四行表及 §1.4 L90“保护结构 = 三张表与四类入口”直接矛盾。另：本文档只有 §3.1–§3.7（L738–L928），**不存在 §3.8**，也无任何 §3.8 引用，该项无法按字面核对。 |

## FINDING RD3-CERTF-01（P1）

**位置**：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:40,55,57`（§1.2 正文）。

**事实**：§1.2 标题已改成“三张表与四类入口”，表体也补了“系统调用入口”一行（L46），但正文仍按旧的“三问/三个问题”口径叙述：

- L40：“三份答案落在三张表上，每张表的装填方式由它回答的问题决定：”
- L55：“它不回答上面三问中的任何一问”
- L57：“本篇的‘保护结构’一词指上面三张表与它们回答的三个问题。”

同节表有四行（特权级、异常/中断入口、系统调用入口、陷入内核用哪根栈），§1.4 L90 也写成“保护结构 = 三张表与四类入口”。L57 是本章核心定义句，仍说“三个问题”，读者在同一节会读到两种口径；这是本次 §1.2 改名未收尾的残留，不是新事实主张。

**影响**：第 7 项不能判 CLOSED；属于 P1 章节内部一致性/定义矛盾，不涉及 C 源码事实。

**建议修法**：把 L57 改为“三张表与四类入口”（与 §1.4 同词）或“三张表与它们回答的四类入口”；L55 的“三问”改为“四类入口中的任何一类”；L40 的“三份答案”改为“四份答案落在三张表与一条 syscall MSR 上”一类表述，或明确声明“其中三份由表承载、系统调用入口由 MSR 承载”。若“四类入口”是全文口径，§1.1/intro 的 L10、L16、L22 的“三份答案/三件事”也需一并收拢；若只在 §1.2/§4.6 使用四类口径，则至少把 §1.2 正文与 §1.4 定义句对齐。

## FINDING RD3-CERTF-02（P2）

**位置**：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:1223`（§5.1 计数句），并波及同节 L1223 前的 9 文件循环与“合计 56”。

**事实**：L1223 的“374 个测试/54 文件”“95 个/21 文件”是 `grep '#\[test\]'` 的**文本命中数**，不是 `#[test]` 测试函数数。复算：

```text
$ grep -r '#\[test\]' os/arch/src --include='*.rs' | wc -l
374
$ grep -rE '^\s*#\[test\]\s*$' os/arch/src --include='*.rs' | wc -l
373
$ grep -r '#\[test\]' os/arch/tests --include='*.rs' | wc -l
95
$ grep -rE '^\s*#\[test\]\s*$' os/arch/tests --include='*.rs' | wc -l
87
```

差额来源是注释/字符串中的文本：`os/arch/src/riscv64/trap_stub.rs:906` 的注释写着“a `#[test]` written here…”（恰好说明这里**没有**测试）；`os/arch/tests/arch_gated_audit.rs:1,7,207,208,210` 等处的文档注释与 `split("#[test]")` 字符串。文件数 54/21 正确，测试数分别多计 1 与 8。9 文件子集同理：文档循环输出“riscv64/trap_stub.rs: 7”，实际锚定 `#[test]` 为 6，子集合计应为 55 而非 56。

**影响**：§5.1 的“测试计数与分布”把注释提及当成测试，属于测试清单口径偏差；§5.1 点名的具体测试名与 §5.2/§5.4 的机制结论不受影响。按 `tools/arch-gated-test-count.py` 的锚定判据（`TEST = ^\s*#\[test\]\s*$`）复算才是测试数。

**建议修法**：二选一——(a) 保留 374/95 但把措辞改成“`#[test]` 标记命中数”（并说明是 grep 口径）；(b) 改成锚定计数 373/87、子集 55，并在 §5.1 的循环命令里使用 `grep -cE '^\s*#\[test\]\s*$'`。后者与项目现有 `arch-gated-test-count.py` 的口径一致。

## 二、四门证据

```text
$ python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/03-kmain-cstart.md
rewrite-notes/01-stage-kernel/03-kmain-cstart.md: blocks=42 checked=0 failed=0 skipped=42 warned=0
（verbose 逐块原因均为 SKIP: 无引文标注；exit 0）

$ bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md
----
anchors=75 resolved=75 zero-def=0 multi-def=0
（exit 0）

$ python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/03-kmain-cstart.md
✅ doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析
（exit 0）

$ bash tools/doc-ref-check.sh rewrite-notes/01-stage-kernel/03-kmain-cstart.md
✅ doc-ref-check: rewrite-notes/01-stage-kernel/03-kmain-cstart.md 内部章节引用均可解析
（exit 0）
```

**证据强度说明（不另立 FINDING）**：`doc-citation-check` 本次 `checked=0`，42 个代码块全部因“无引文标注”被 SKIP。该工具的标注识别式只接受 `// 代码引用/节选/…`，而本文档 C 代码块使用 `/* 代码引用，… */`（如 L101、L165、L284）。因此这道门对本文档**没有产生逐行对账证据**，不能把 `failed=0` 当作引文正确性的证明；本认证的 7 项改以人工读 C/Rust 源与 `grep` 取证。此为工具标注格式与文档写法的既存错位，不是本次 7 项改动新增的事实错误，故不纳入新准入。

## 三、结论

- 第 1–6 项：CLOSED（第 6 项附 P2 计数口径问题）。
- 第 7 项：REOPEN（§1.2 正文三处旧口径；§3.8 不存在）。
- 新准入：P1 × 1（§1.2 定义句仍写“三个问题”），P2 × 1（§5.1 测试计数把注释中的 `#[test]` 计入）。

最终认证新准入 = 2

## 四、复认证（doc sha 前缀 3fc8dd7d）

- RD3-CERTF-01：CLOSED。§1.2 L40 已改为“四类入口落在三张表与系统调用路径上”，L55 改为“上面四类入口中的任何一类”，L57 改为“上面三张表/系统调用路径与它们回答的四类入口”；§1.2 内已无“三份答案/三问/三个问题”。
- RD3-CERTF-02：CLOSED。§5.1 L1223 已改为锚定 `#[test]` 的 373/87，9 文件子集 55；复算一致（`os/arch/src` 373/54，`os/arch/tests` 87/21，子集 55）。
- 本次按要求只重读上述改动句及邻句，未重扫全文；未发现新 P0/P1/P2。改动不涉及锚点、表格结构或章节引用。

最终认证新准入 = 0
