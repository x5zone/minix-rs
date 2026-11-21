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

- **READER-A-S01–S11**：未定义 ID/悬空 follow-ups、`push 0` 终止槽解释、§3.1 第 5 行指针、§3.5 两档条件重叠、开头节号、§5.2 重复 kern_size、§4.4 init_protection 引文张力、§2.4 证据表、§2.5 射程、§4.1 AlreadyMapped 指代、自引导 p_paddr 读者。
- **READER-B-S01–S15**：职责重叠、章节边界、计数口径、单形态概括、证据缺口、指针自指与术语未定义等结构建议。
- **READER-C-S01–S11**：指针、过程词、路径缺失、§7.1 自指、重复事实、`.lrodata` 叙述、术语等。
- **FACT-S1–S5**：kern_stack_top「两个派生标量」措辞、§7.2 pagetable.c 背景、§3.1 检索面排除 target、§5.1 每环节措辞、SegmentIter::new 构建形态。
- **DESIGN-D2-07（P3）**：删 `jump_to_kmain` 的 `stack_top` 形参（从 kinfo 现算）。
- **DESIGN-D2-08（P3）**：删零读者的 `p_align`/`LoadSegment.align`、`HIGHER_HALF_CALLED`、`virtual_end`。
- **DESIGN-D2-09（P3）**：删 `KernelInfo`/`MemoryRegion`/`BootModule` 的 `repr(C)`（无 ABI 读者）。
