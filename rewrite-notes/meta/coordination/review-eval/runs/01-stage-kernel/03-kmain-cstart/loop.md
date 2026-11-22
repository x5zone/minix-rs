# D03 kmain 与 cstart —— V3.11 独立账户

## R0 账户初始化（2026-11-22）

- 文档：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
- 规则版本：`V3.11`
- 轮次：`0`（无 full、无 confirmation、无 trajectory）
- 旧 V3.10 轨迹：`rewrite-notes/meta/coordination/review-eval/archive/V3.10/01-stage-kernel-runs/03-kmain-cstart/`
- 本账户从 R1 开始；上一 edition 的任何轮次、发现数、冻结状态都不继承。

## R1 full（V3.11，2026-11-22）

### §0 基线

- commit: `e3a96c6ef302bb2c6811a07c127bb7af71056a9a`
- 文档 sha256: `5b142de24f009880fffc78818af6913be8150af31374632e80d57187fd6354f7`
- 行数: `1688`
- 单元: 6 章 + 38 节 = 44
- 关联代码: `related-code.txt` 22 个文件，`doc-code-map --check` 22/22
- L0.9: doc-publication 0 新增；tutorial 6/6；TODO `--fail-on-open` 0 行；code-publication 22 文件 0 新增
- 轮型: `full`；schema/edition: `V3.11`
- 目标：第 1 个 full 轮，覆盖 44/44；D01/D02 的轮次/证据不继承

### §1 面板

- 角色：P-FACT、P-DESIGN D1/D2、P-READER A/B/C、P-REFUTE；全部冷启动、未参与修复。
- 制品：`panel/round1/{fact,design-d1,design-d2,reader-a,reader-b,reader-c,refute}.md`。

### §2 发现

- P-FACT 5 P1/P2（IST 两段、载体陈述、39/41 门、省略号、顺序钉 3/4）+1 P3。
- P-REFUTE 13 CONFIRM / 2 REFUTE（同上两条）。
- READER-A 6（IST、bss_test、别名成因、_TABLE、39/41、§4.7 指针）。
- READER-B 2（同 IST/载体）。
- READER-C 3（IST、lgdt 取指/tss_init、§2.5 异常窗口）。
- D2 1 P0（LDT 陈旧引文）+2 P1（IST ARCH 标记、重复段）+1 P2 FP。
- 合并 15 个根因；其中 P0 1 条（LDT 引文）。

### §3 归因

- 15 根因：事实/计数 6、时序/因果 3、术语/引用 3、代码 ARCH 标注 1、设计取舍 1（FP）、其他 1。
- 跨角色同根：IST（FACT/REFUTE/A/B/C/D2）、载体（FACT/REFUTE/B）、39/41（FACT/A）。

### §4 修复

- 删除 set_gate_ist 旧段；载体陈述改为实际调用；C 39/Rust 41 拆开；省略号标注、顺序钉 3→4、五处命中；bss_test 限定；_TABLE 定义；§2.8 指针；别名覆写限定；lgdt/tss_init 时序；§2.5 异常轴；LDT 陈旧引文；protection.rs IST 注释补 [ARCH]。
- 代码只改注释，`cargo check -p minix-arch` 通过。

### §5 复验

- 文档门全绿；单元 audit open=0；R1 full 44/44；31 pass + 13 fix。
- cargo check minix-arch 通过。

### §6 指标

见 `metrics.md` 的 `## R1 full（V3.11）`：15 根因、14 ① + 1 FP、0 未闭环。

### §7 状态

`ROUND-CLOSED`，进入 `CONVERGENCE-CANDIDATE`；R2 整篇复审，R3 逐单元确认。

### §8 给 meta-review 的不确定点

1. R1 面板在修复前读文本；修复后只跑机械门，R2 必须重读。
2. D2 的 refresh no-op 是设计候选，未实施。
3. 多条 READER SUGGESTION 未逐条实施。

## R2 full（V3.11，2026-11-22）

### §0 基线

- commit: `e3a96c6ef302bb2c6811a07c127bb7af71056a9a`
- 文档 sha256: `$(sha256sum rewrite-notes/01-stage-kernel/03-kmain-cstart.md | cut -d' ' -f1)`
- 行数: 1688
- 单元: 44；R1 的 13 个 fix 单元未冻结
- 轮型: full；schema/edition: V3.11；R1 轨迹 PASS

## R2 full（BLOCKED，2026-11-22）

- 派发六角色后，`panel/round2/` 在等待窗口内没有任何角色报告落盘。
- R1 已 `ROUND-CLOSED`、44/44 覆盖、trajectory R1 PASS；R2 不能拿 R1 报告替代。
- 解除条件见 `STUCK-REPORT.md`。
- 状态：`BLOCKED`；不得 `CONVERGED`，不得进入 D04。

### §1 面板

- 六角色全部落盘（D1/D2 之外的四份曾在等待窗口后到达；STUCK-REPORT 已标 UNBLOCKED）。
- R2 是独立 full 轮：面板重读整篇，重点复核 R1 的 13 个 fix 单元。

### §2 发现

- D2：1 P0（虚构 `KernelInfo.syscall_entry`）+2 P1（gate_present 引文、重复段）+2 P2。
- READER-A：5（重复段、GsArea、内部编号、aarch64 栈链、§4.6 句子）。
- READER-B：0 FINDING，12 SUGG。
- READER-C：8（IST 表、LDT、tss_init busy、重复、暴露面、向量 33、GP 0x28、代价列）。
- P-REFUTE：12 CONFIRM / 3 REFUTE（重复段、arch tests 87→86、SP_EL1/TSP）。
- P-FACT：1 P1（重复段）+2 P2（§2.8 指针、链接文字）。
- 合并 16 个根因；P0 1 条。

### §3 归因

- 16 根因：事实/引文 4、IST/门 2、LDT/TSS 2、重复/编号 2、安全/向量/数值 3、栈链/句子 2、计数/TSP 2、ZST 1。
- 跨角色同根：IST（C/D2）、重复段（A/C/D2/FACT）、LDT（C/D2）、SP_EL1（C/REFUTE）。

### §4 修复

- 删除 syscall_entry 行；gate_present 引文改实义；IST 表行改单写者；LDT 空选择子；tss_init busy 补段；删除重复 IST 段；暴露面改“不新增”；向量 32/33 分开；#GP 0x2B；GsArea 字段归属；内部编号；aarch64 栈链；§4.6 句子；arch 86；TSP 分配置；LDT/ZST 措辞。
- 代码只改注释（trap_entry.rs），`cargo check -p minix-arch` 通过。

### §5 复验

- 文档门全绿；单元 44/44 frozen；audit open=0；cargo check minix-arch 通过。

### §6 指标

见 `metrics.md` 的 `## R2 full（V3.11）`：16 根因、16 ①、0 未闭环、3 自引入已修。

### §7 状态

`ROUND-CLOSED`，进入 `CONVERGENCE-CANDIDATE`；R3 逐单元确认。

### §8 给 meta-review 的不确定点

1. R2 面板在修复前读文本；修复后只跑机械门/cargo check，未重派全量读者。
2. D2 的 refresh 无条件化是设计候选，未实施。
3. STUCK-REPORT 的“面板缺失”在等待后失效，已追加 UNBLOCKED。

## R3 confirmation（V3.11，2026-11-22）

### §0 基线

- 文档 sha256: `9ad016031f4dac20d15067a085c95601fec6fa7d165568558a6d77abbd647975`，1689 行
- 单元：44/44 frozen；R2 ROUND-CLOSED；trajectory R2 PASS

### §1 面板

- 逐单元 `--confirm-review`；证据 `unit-r3/<id>.md` 引用 `confirmation-r3.md` 与 R2 面板。

### §2 发现

- 0 新根因；R2 16 条、R1 15 条全部 CLOSED。

### §3 归因

- 0 新根因；0 未闭环。

### §4 修复

- 无文档修改；只记录确认。

### §5 复验

- 44/44 单元 post_freeze_rounds 含 R3；review_rounds ≥2；T3 PASS。
- 文档/代码门见 `certify/round3-gates.md`。

### §6 指标

- 发现 0；根因 0；未闭环 0；轮型 confirmation；结果 CONVERGED。

### §7 状态

`CONVERGED`（V3.11）。限制见 `FREEZE.md`。

### §8 给 meta-review 的不确定点

1. R3 逐单元确认仍引用 R2 面板，不是 44 个独立冷读会话。
2. R2 六角色晚到后已解除 STUCK；STUCK-REPORT 追加 UNBLOCKED 记录。
