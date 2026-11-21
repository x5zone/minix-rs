# D02 suggestions —— 读者/设计 P3 建议桶

> 按 `review-eval/review-evals.md` §4.0c：读者与设计角色的 P3 建议、以及准入判据不全的读者/设计项，
> 以 SUGG 行入台账、登记本文件，不进修复队列、不参与 G1–G9。

## R3 确认轮（V3.4）

| 来源 | 数量 | 摘要 | 处置 |
|---|---|---|---|
| RD3-READER-A-S* / reader-a.md 的 SUGGESTION | 19 | 措辞、指针、示例与术语类建议 | 登记不实施（读者 P3） |
| reader-b.md 的 SUGGESTION | 15 | 节边界、重复指针化、术语定义、过程词类 | 登记不实施（读者 P3） |
| reader-c.md 的 SUGGESTION | 10 | 复述一致性、GDB 输出多架构适配、覆盖声明 | 登记不实施（读者 P3） |
| RD3-DESIGN-D2 的 P3 | 5 | 设计 P3（含 `entry_point` 语义、arch link.ld 读者、自引导面 `&'static str` 旁的次要项） | 随 `R3-design-batch`（D-063）评估 |

> 原文件：`panel/round3/reader-*.md` 与 `design-d2.md`；逐条 ID 见各文件。

## R4 确认轮建议（V3.4）

> 来源：`panel/round4/` 的 37 条编号 SUGGESTION + 5 条 fact 未编号建议 + 3 条设计 P3（DESIGN-D2-07/08/09）。不阻断收敛。
>
> 三态：**已由发现解决**（目标文本已被别的 FINDING 修掉，标 `dup-of`）/ **只登记**（保留给下一 edition）/ **下一设计批**（有设计债务行）。

- **已由发现解决（5 条）**：
  - `READER-A-S03`：§3.1「第 5 行」指针已由 `READER-B-01`/`READER-C-02` 修成「第 3 行」。
  - `READER-A-S06`、`READER-B-S11`、`READER-C-S10(a)`：§5.2 重复 `kern_size` 已由 `FACT-07`/`DESIGN-D2-10` 的载体表达式修掉。
  - `READER-B-S14`：§2.5 ARM「待补」已由 `FACT-09` 的 ARM 证据修掉。
- **只登记（其余 32 条编号建议 + 5 条 FACT 未编号建议）**：
- **READER-A-S01–S11**：未定义 ID/悬空 follow-ups、`push 0` 终止槽解释、§3.1 第 5 行指针、§3.5 两档条件重叠、开头节号、§5.2 重复 kern_size、§4.4 init_protection 引文张力、§2.4 证据表、§2.5 射程、§4.1 AlreadyMapped 指代、自引导 p_paddr 读者。（其中 S03/S06 已解决，见上）
- **READER-B-S01–S15**：职责重叠、章节边界、计数口径、单形态概括、证据缺口、指针自指与术语未定义等结构建议。（其中 S11/S14 已解决，见上）
- **READER-C-S01–S11**：指针、过程词、路径缺失、§7.1 自指、重复事实、`.lrodata` 叙述、术语等。（其中 S10(a) 已解决，其余子项仍登记）
- **FACT-S1–S5**（事后编号，面板原文无 ASCII ID）：kern_stack_top「两个派生标量」措辞、§7.2 pagetable.c 背景、§3.1 检索面排除 target、§5.1 每环节措辞、SegmentIter::new 构建形态。`panel-index.py` 不计这 5 条（B-72）。
- **FACT-OBS-01**（certify 观察，非台账行）：§5.1 的「三类畸形」与迭代器四个 `ElfError` 变体不符；本轮已改为「四类畸形」并列出变体（post-certify P3）。
- **下一设计批（3 条）**：
- **DESIGN-D2-07（P3）**：删 `jump_to_kmain` 的 `stack_top` 形参（从 kinfo 现算）；`dup-of` V1 `DESIGN-03`/`D02-F01`。
- **DESIGN-D2-08（P3）**：删零读者的 `p_align`/`LoadSegment.align`、`HIGHER_HALF_CALLED`、`virtual_end`。
- **DESIGN-D2-09（P3）**：删 `KernelInfo`/`MemoryRegion`/`BootModule` 的 `repr(C)`（无 ABI 读者）。
