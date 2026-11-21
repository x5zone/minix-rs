# D03 循环轨迹 —— 03-kmain-cstart

## §0 基线

- 工作流版本：V1（`rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md`）
- schema: V3.4 from R3（V3.5 制品链起约束的首轮；重开依据 D-066。R1/R2 保持各自历史标记，不追溯）
- 基线提交：`859dd2216`
- 文档：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
- 文档基线 sha256：`56b81ad4fdea99fbb802721d2312d23282fcf92c67b7192413fc38d14933efb4`
- 基线行数：1375
- 工作树：除 `prompt/resources/` 未跟踪外干净
- 语料条目：D03（`CORPUS.md` 第二节）；盲区重点 B-01、B-02、B-03、B-04、B-08、B-12、B-13
- 设计快照：`03-outline.v4.md`、`03-outline-review.v4.md`、`03-design.v2.md`；`design-coverage-check.sh 01-stage-kernel` 全 PASS
- 关联代码：21 个文件、18,063 行，`tools/doc-code-map.sh --check` 21/21 通过；清单见 `related-code.txt`
- 本篇形态：保护结构初始化 / GDT-IDT-TSS / cstart 序列 / 三架构；高风险，按 `review-panel.md` 第七节启用全角色
- 范围声明：本篇范围 = 文档主题（kmain 第一段与 cstart 的保护结构初始化）+ `related-code.txt` 的 21 个文件 + 本轮修复触碰的文件。不在范围 = 相邻模块、未列入清单的文件、下一批次的结构性重构。

## §1 面板

- P-FACT：文档全文、21 个关联 Rust 文件、24 个 C 源、trait 读数、冻结决策编号；未读历史 scan 与其他面板输出。输出 `panel/fact.md`，16 条发现。
- P-DESIGN D1：C 语义、文档 Ch1/Ch2、项目约束；任务书禁止现有 Rust、Ch3/Ch4、`.design/`、`reader-profile.md`。报告声明全程未打开 `os/`、未读 §3 及以后、未读 `.design/` 与读者画像。输出 `panel/design-d1.md`，14 条 DECISION（不是 FINDING）。
- P-DESIGN D2：D1 报告、文档 Ch3/Ch4、26 个 Rust 文件、trait 读数；未读作者解释、历史评审、其他面板输出。D2 在 D1 落盘后启动，**本轮完成了 D1→D2 对照**。输出 `panel/design-d2.md`，19 条发现。
- P-READER A/B/C：只读文档全文与 `reader-profile.md`。输出 `panel/reader-a.md`（12）、`reader-b.md`（17）、`reader-c.md`（17）。
- P-REFUTE：文档、C 源、关联 Rust、设计快照；未读其他面板输出。输出 `panel/refute.md`，6 条主张：5 条反证失败，1 条反证成功（CLAIM 02：旧 memmap 链与 `arch_init` APIC 分支），另有 P1/P2。
- 缩编：无。
- 工作树完整性：面板前后 `git status --porcelain` 只有 `prompt/resources/` 与新增 `runs/`，无面板写入源码或文档。
- 无发现面板：`design-d1.md`（独立设计提案，14 条 DECISION，只作参考）；`refute.md`（6 条 CLAIM，反证结果见 `defect-ledger.md` 的 `## P-REFUTE 附加` 表）。

## §2 发现

- 面板原始输出：`tmp/review-eval/01-stage-kernel/03-kmain-cstart/panel/`。
- 总计 81 条 `## FINDING`：FACT 16、DESIGN-D2 19、READER-A 12、READER-B 17、READER-C 17；P-REFUTE 另有 6 条主张。
- 最高严重度：
  - P0：FACT-01（§4.1 旧 `FREE_MEMMAP`/Phase A.2/Phase C/F 链）、FACT-02（`store_kernel_info` 7 slice 与“拷贝结构体”）、FACT-03（`cstart` 不是 `kmain` 最后动作）。
  - P1：FACT-04（aarch64 `SP_EL1` UNDEFINED 说法）、FACT-05（39 门只对 `prot_init` 成立）、FACT-06/07（Phase B“只做软件态”、`arch_init` 不初始化控制器）、DESIGN-01（NMI IST1 改变嵌套语义）、DESIGN-02（MAX_CPUS 32 vs 8）、DESIGN-04（`load()` 可重入）、DESIGN-19（装载次序无断言）。
- 完整清单与逐条处置见 `defect-ledger.md`；未修项见 `follow-ups.md`。

## §3 归因

- 处置分布：① 24 条、② 57 条、③ 0 条、④ 0 条、⑤ 0 条。
- 冻结决策检查：0 条 `re-litigation`；D-001 四章骨架没有被任何角色重新提出。
- 范围门：0 条 adjacent 被当阻塞；所有 ② 项只进 `follow-ups.md`。
- 误报为 0 不等于精度完美；reader B/C 的转出项可能含少量措辞偏好，留待闭环轮。

## §4 修复

### 文档

- FACT-01/02/DESIGN-10：§4.1 改为“`store_kernel_info` 落七个 slice；Phase A.2 故意为空；`FREE_MEMMAP` 不存在；内核保留由 `vm_handoff::classify` 的 `cut_memmap` 扣除并记录；`init_proc_and_boot` 与 Phase F 没有 `add_memmap`”；删掉“三个变长载荷/五块落地/不是拷贝结构体”的旧表述。
- FACT-03/READER-B-01：§2.2 改为“`cstart` 是 `kmain` 的一个早期动作，返回后 `kmain` 继续执行 BKL 加锁、`proc_init`、boot image 循环等”。
- FACT-04/READER-C-03：§4.6 改为“`SP_EL1` 在 EL1 可访问；崩溃原因是写它换掉当前栈，不是 UNDEFINED；从 EL0 进 EL1 时硬件选中 banked `SP_EL1`”。
- FACT-05：§3.7 的 41/39 对照加“只对 `prot_init`/`idt_init` 完成时成立；APIC/SMP 路径还会加 64 条 IOAPIC 门与 spurious/错误向量，C 总门数变为 84/86”。
- FACT-06/07/READER-C-03：§2.2 的 `arch_init` 补 APIC 分支；§3.6 改为“Phase B 的 `intr.init()` 编程 LAPIC/IOAPIC 控制器侧，Phase F 只放行时钟门”；§4.7 的状态句改为“中断源尚未放行，控制器已编程”。
- FACT-08：`114:const IDT_ENTRIES`。
- FACT-09/DESIGN-11：§4.4 明确 `STUB_VECTORS` 的布尔值只被宿主测试使用，汇编由 `TRAPSTUB vec err` 字面量实例化。
- FACT-10：随 FACT-04 一并改。
- FACT-11：§4.6 的 `set_entry_stack_base` 引文补 `pub(crate)`。
- FACT-12：§5.1 补“9 个文件是子集（56）；arch crate 全量 375/54，tests 94/20”。
- FACT-13：§7.2 补 `arch_clock.c`、`earm/pre_init.c`、`servers/vm/main.c`；§7.3 补 `timer_irq_gate.rs`、`trap_dispatch.rs`。
- FACT-14/READER-A-09/READER-B-13/READER-C-02：§3.6 的“§5.3 有披露”改为“注释与函数体已同步，§5.3 记录同步后的状态，没有出入可披露”。
- FACT-15/READER-A-10：§4.2 的 `EFER.SCE` 反事实加“窗口内没有 CPL3 用户态，SYSCALL 不会到达”。
- FACT-16：§2.8 的 `prot_init_done` 反事实加“AP 不会在 `prot_load_selectors` 之前被放行”。

### 代码

- 本轮未改代码；19 条设计发现全部转出（见 `follow-ups.md`），D03 标 design-debt。

### 未修

- DESIGN-01..09、12..19 与 reader A/B/C 的 57 条转出项：见 `follow-ups.md`。

## §5 复验

- `tools/anchor-resolve.sh --check`：anchors=80 resolved=80 zero-def=0 multi-def=0，exit 0。
- `tools/doc-code-map.sh --check`：22/22 存在，exit 0。
- `tools/doc-ref-check.sh --check`：内部章节引用均可解析，exit 0。
- `tools/doc-code-density-check.sh --check`：4 个可判章均有码/命令，阻断项通过，exit 0。
- `tools/doc-style-lint.sh --diff`：1 个文件 / 46 个新增行，error=0，exit 0。
- `cargo check -p minix-kernel --features mock`：exit 0。
- `tools/eval-trace-check.py`：81 条发现全部有处置行、无孤儿行、metrics 自洽，exit 0。
- QEMU 真机回归：未跑；DESIGN-01/02/04/19 的运行时行为都需要 QEMU 或专门载体。

## §6 指标

- 见 `metrics.md`：发现 81、① 24、② 57、③ 0、④ 0、⑤ 0、`re-litigation` 0、`adjacent 阻塞` 0、重构提出 19/接受 6/实施 2/登记 17。
- 成本：7 个面板 agent，面板约 60 分钟；修复约 1.5 小时；token 未采集。

## §7 状态

**NOT CONVERGED（design-debt + teaching-debt + test-debt pending）**

- 设计债：DESIGN-01..09、12..19 共 17 条转出；其中 DESIGN-01（NMI IST1）、DESIGN-02（MAX_CPUS）、DESIGN-04（load 可重入）是 P1，需优先排队。
- 教学债：reader A/B/C 的 57 条转出项。
- 测试债：`cargo test -p minix-arch` 测试模块编译失败（D03-F15，沿用 D01-F01）。
- 已修 24 条；未达到 `CONVERGED` 的原因不是“还有小问题”，而是上述三类债都有明确去处且尚未关闭。
- meta-review 总体 `FLAT`（检测面 IMPROVED、处置面 REGRESSED），运行质量 `NOT PASS`（抽样 25/81 = 30.9%，一致性 68.0%，宽口径 88.0%）；7 条 ① 修复不完整已按复核补修，B-20/B-21/B-22 入账。

## §8 给 meta-review 的不确定点

1. DESIGN-02（MAX_CPUS 32 vs 8）是真实启动崩溃风险，却被转出；本轮按“代码重构上限”处理，请判是否应改为本轮修。
2. DESIGN-01（NMI IST1）改变了 C 的嵌套语义，没有 `[ARCH]` 标注；请判是“有意的架构演进”还是“未披露的行为变化”。
3. FACT-05 的 41/39 对照加了 APIC 口径，但文档其余处仍可能把 41 当“全部”；请抽样核对。
4. reader B/C 的 57 条转出项中，有多少是真正的教学债、多少是文风偏好；本轮未逐条判 ③。
5. QEMU 未跑；`load()` 可重入、MAX_CPUS、IST 对齐都只有静态证据。
6. D2 本轮看到了 D1 并做了对照，但 D1 的 14 条 DECISION 没有进台账；请判是否需要把设计提案也纳入轨迹门（B-17）。
7. **修复覆盖**：meta-review 判 24 条 ① 里只有 17 条全改到；补修后仍有 `cargo test -p minix-arch --no-run` 的 45 个错误，文档 §5.1 的 375 个测试只是静态计数。请把“静态计数”与“能跑”分开看。
8. **新增盲区**：B-20（修复语句自身事实未复算）、B-21（文档修正与被引用代码注释脱钩）、B-22（处置类按来源角色分配）；对应候选 C-25/C-26。

## R1 完整 survey（V3.3）

### R1 §0 基线

- 轮型: full
- edition: V3.3
- schema: V3.3 from R1
- 基线提交：`e5ca3bb55`（D02 FREEZE 之后）
- 文档：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
- 文档 sha256：`469727d74e1ec63c90720ffc81a0fb4804a50b108b22566791ff2b19bd8e11a2`
- 行数：1382
- 工作树：除 3 个既有未跟踪文件外干净
- 设计快照：03 组齐备；`design-coverage-check.sh 01-stage-kernel` ALL DOCS COMPLETE
- 关联代码：`tools/doc-code-map.sh --check` 21/21 通过；清单见 `related-code.txt`
- 范围声明：本篇范围 = 文档主题（C 入口 `kmain`/`cstart`、trap entry、三架构） + 21 个关联文件 + 本轮修复触碰的文件。不在范围 = 相邻模块、未列入清单的文件、下一批次的结构性重构。
- L0 预修：删除了对不存在文件 `os/kernel/src/memmap.rs` 的两处引用（正文与 §7.3），`doc-code-map` 由 1/22 缺失转为 21/21；该修改记入 R1 台账。
- 历史说明：本文件顶部的旧 §0–§8 是 V1 时代记录，保留为历史段、不追溯改账；其未批准 ② 不带入 R1。

### R1 §1 面板

- 4 角色：fact（C+Rust）、design-d2、reader-abc（A/B/C 合并，VARIANCE 已记）、refute（P0 触发）。
- fact 8 条（P0 4 / P1 1 / P2 2 / P3 1）；design-d2 16 条（P0 2 / P1 1 / P2 5 / P3 8）；reader-abc 4 条 FINDING + 21 SUGG；refute 6 条主张（4 条反证成功、2 条失败）。
- 材料隔离：读者未读源码；设计未读作者解释；事实未读历史评审。

### R1 §2 发现

- 43 条（survey 34 + certify 9）：P0 = 9（memcpy 自拷贝误判、kernel_may_alloc 复述误判、KERNEL_MAY_ALLOC 已删、Step 1 旧引文、pg_utils 双副本、kmain 全局副本语义）；P1 = 3；P2 = 14；P3 = 8；FP 2。
- 最关键的是文档把 `pre_init` 的 `.unpaged_bss` 暂存副本与 `main.c` 的 `.usermapped` 最终全局混为一谈，以及把 `pg_utils.c` 的两份对象当成同一份。

### R1 §3 归因

- 处置：① 29 / ② 4 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 2 / SUGG 8 = 43；唯一根因 43。
- 冻结决策：0 条 re-litigation；D2-16 按 ARCH 提案登记 D-053，不实施。
- 范围门：0 条 adjacent 阻塞；三条代码设计项进 `D03-design-batch`。

### R1 §4 修复

- 文档：§2.1 memcpy/kernel_may_alloc 两段重写；§2.6 pg_utils 双副本、vir2phys 绑定、反事实重写；§3.5 Step 1 引文；§4.1/§4.7 KERNEL_MAY_ALLOC 淘汰；§4.4 诊断读数；§5 计数；§2.6 PWT/PCD 条件；§3.2/§3.3/§4.7 的 x86 限定与特权级预留；§3.7 门数算式；§1.2/§4.6 入口表拆行；§2.4 IOPL 表述。
- 代码：无行为改动；三条设计项登记 `D03-design-batch`（D-053）。

### R1 §5 复验

- 文档六门：citation 42 块全部跳过（`/* */` 标注，工具只认 `//`，见 §8）；anchor 75/75；structure/ref 通过；style-lint error=0；doc-code-map 21/21。
- 旧串归零：`全局自拷贝`、`把同一句话再说一遍`、`KERNEL_MAY_ALLOC`、`Maps 1024`、`同一个静态页目录数组`、`39 + 64`、`异常/中断/syscall`、`因此全部被拒` 均为 0。
- certify：#1 见 `panel/round1/certify-*.md`；本轮无代码行为改动，QEMU 未跑。

### R1 §6 指标

- 见 `metrics.md` R1 块：发现 43、唯一根因 43、① 29 / ② 3 / FP 2 / SUGG 8、未闭环 0、ROUND-CLOSED、代裁决 1、ARCH 提案 1。

### R1 §7 状态

**ROUND-CLOSED**（修改轮出口；进入 CONVERGENCE-CANDIDATE，等待确认轮）。open = 0；设计代码项按 §4.0f/§4.1b 登记，不阻断；不宣告 CONVERGED。

### R1 §8 给 meta-review 的不确定点

1. citation 门对本文 42 个 `/* */` 引文块全部跳过（工具只认 `//` 标注），引文核对主要靠 fact 的人工逐块；下一 edition 应修工具或改标注。
2. 6 条 P0 全是“文档把历史机制/已删代码当成现行实现”，说明本篇自 2026-11 后未随代码同步；R1 是首次 V3.3 全量。
3. 三条代码设计项（注释、断言、ARCH 契约）延期到 design batch；是否削弱 ROUND-CLOSED 请 meta 判断。
4. QEMU 未跑；本轮无 arch/boot 行为改动。

### L6

- 同 agent 收口：29 条全部处置；P0 6 条全修；ROUND-CLOSED。
- BLIND-SPOT-DELTA：新增“文档未随 D01-F08/BOOT-SCOPE 删除回写”的系统性漂移；无新结构性盲区。

### L6.5

- 三问：① 本轮不变量是“C 的两份对象/两份页目录与 Rust 的现行实现必须逐符号对上”；② 前提是旧 V1 run 只作历史；③ 证据是 29 行台账、六门读数与 fact/refute 的源码直读。
- 资源重读：本轮盲区是“历史文档与删除提交脱钩”，检测模式已由 `doc-code-map` + fact 的符号存在性核对覆盖；citation 工具的 `/* */` 盲区另记。

## R2 确认轮（V3.3 四态模型）

### R2 §0 基线

- 轮型: confirmation
- edition: V3.3
- schema: V3.3 from R1
- 基线提交：`1b1ea902f`（R1 修复与轨迹提交之后）
- 文档 sha256：`093de2311fb54746c3841dfbe7f17dbb97719a60cf86f1f304ca88b6c861faa7`
- 行数：1384
- 设计快照与关联代码同 R1；`doc-code-map` 21/21
- 轮次语义：确认轮；修复权限仅限新准入 P0/P1/P2 事实/同步项；不实施 ⑥；`certify 新准入 = 0` 才可 CONVERGED。

### R2 §1 面板

- 3 角色：fact（C+Rust）、design-d2、reader-abc（A/B/C 合并，VARIANCE 已记）；无 refute（无 survey P0，确认轮按裁剪）。
- fact 6 条（P0 3 / P2 3）；design-d2 10 条（P1 1 / P2 4 / P3 5）；reader-abc 27 条（P1 12 / P2 15）。
- 材料隔离：读者未读源码；设计未读作者解释；事实未读历史评审。

### R2 §2 发现

- 48 条（survey 43 + certify 5）：P0 = 5（`bootstrap_start/len` 已删、`GsArea.user_rsp` 被入口写、Rust 无 IOAPIC 64 门表）；P1 = 13；P2 = 22；P3 = 5。
- 关键：R1 修复后仍有“已删字段/反方向时序/把 C 的 64 门表当成 Rust 扩展”三类残项；读者侧 27 条集中在对象图归属、向量记法、范围声明与术语。

### R2 §3 归因

- 处置：① 41 / ② 2 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 5 = 48；唯一根因 48。
- ② 两条：design F03（过期注释）与 F04（非 0 BSP 的 TSS 槽 0），登记 `D03-design-batch`（D-053）。
- 冻结决策：0 条 re-litigation；设计 P3 5 条进 suggestions。

### R2 §4 修复

- 文档：bootstrap 字段删除、`GsArea.user_rsp` 读写、Rust 41 门/IOAPIC 16 IRQ、Phase F 三步、sscratch 时序、3/6 计数、TSS `cs` 行、次序承重小结、五处寄存器读数、`丢弃后重建` 去重、slice 字段名、引导方/装载方定义。
- 代码：无行为改动；F03/F04 登记设计批。

### R2 §5 复验

- 文档六门：citation 42 块全跳过（`/* */` 盲区）；anchor 75/75；structure/ref 通过；style-lint error=0；doc-code-map 21/21。
- 旧串归零：`bootstrap_start/len 仍为`、`三个字段……只读`、`Rust 侧相应增加`、`装载后第一个异常`、`各 3 与 7`、`本篇出口状态` 均为 0。
- certify：见 `panel/round2/certify-*.md`；最终回读 0。

### R2 §6 指标

- 见 `metrics.md` R2 块：发现 48、唯一根因 48、① 41 / ② 2 / SUGG 5、未闭环 0、CONVERGED、代裁决 1。

### R2 §7 状态

**CONVERGED**（确认轮出口）。G1–G9：in-scope P0/P1/P2 = 0；读者清单待修 = 0；⑤ = 0；② 两条有 D-053 批准与日期；机械门全绿；台账 43 行全部有处置与可重放复验；`certify 新准入 = 0`。设计代码项进 `D03-design-batch`，不阻断。

### R2 §8 给 meta-review 的不确定点

1. citation 门 42 块 `/* */` 全跳过（工具只认 `//`），引文核对靠 fact 人工；已登记 D03-citation-tool-batch。
2. 27 条读者项里多数是结构/术语，R2 只做线级修复，未做整节重写；是否足够请 meta 判断。
3. F03/F04 是代码项，延期到设计批；R2 的 CONVERGED 只覆盖文档事实层。
4. QEMU 未跑；无 arch/boot 行为改动。

### L6

- 同 agent 收口：43 条全部处置；P0 3 条全修；CONVERGED 由确认轮出口给出。
- BLIND-SPOT-DELTA：确认轮在 R1 已修段仍抓到“已删字段/反方向时序/跨实现错配”三类残项，说明 R1 的 certify 覆盖不足；已记录。

### L6.5

- 三问：① 本轮不变量是“已删字段不得回流、C 的 APIC 表不等于 Rust 的 41 门、入口对 GsArea 是读写”；② 前提是 R1 ROUND-CLOSED；③ 证据是 43 行台账、六门读数与 certify 0。
- 资源重读：本轮盲区属“文档同步后的二次残项”，检测模式是确认轮的跨节复述核对；无需新增规则。


## REG2/FULL 补账索引（r6 R6-b，非编号轮）

- 2026-11-17 回归轮（D-055 重做，独立三角色 + certify 三轮 7/3/1 → BLOCKED → 结构性重写）：台账见 `defect-ledger.md` REG2 节；面板 `tmp/review-eval/01-stage-kernel/03-kmain-cstart/panel/regress/`。
- 2026-11-18 完整 review（五角色 33 条聚合口径 / 面板编号 22 条）：台账见 FULL 节；面板 `panel/full/`。
- 两轮均不改变 FREEZE 时点的收敛状态（出口在 FREEZE 回归/完整 review 节）。

## R3 确认轮（V3.4 四态模型；D-066 授权的重开）

### R3 §0 基线

- 轮型: confirmation
- edition: V3.4
- schema: V3.4 from R3
- 基线提交：`32d4fa164`（D02 收尾提交；本轮修复未提交时的工作树）
- 文档：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
- 文档基线 sha256：`e54cad6907da01f4bd455e4b63712bda4896a1c92fe89ce8d4d04e227362344d`（R2 FREEZE 值，与面板启动时一致）
- 行数：1396（修复后仍 1396）
- 设计快照：03 组齐备；`design-coverage-check.sh 01-stage-kernel` ALL DOCS COMPLETE
- 关联代码：`doc-code-map --check` 21/21
- 语料条目：D03（`CORPUS.md` 第二节）
- 范围声明：保护结构初始化与 cstart 序列 + 21 个关联文件 + 本轮修复触碰的文件；不在范围 = 相邻模块与下一批次结构性重构
- 重开依据：D-066

### R3 §1 面板

- 六角色全量、独立子 agent；读者 A/B/C 独立（文档 1396 行 ≥ 500）：
  - P-FACT：2 条 FINDING + 2 SUGGESTION（`fact.md`）——面板实测 25 处引文逐字一致、约 90 项常量现算一致、13 个点名测试全存在、21 组反事实前提成立，文档事实质量高
  - P-DESIGN-D2：15 条 FINDING + 2 ARCH-PROPOSAL（`design-d2.md`）；D1 停跑（D-024），基线「文档主张 ↔ 源码事实」，记 VARIANCE
  - P-READER-A：3 + 16（`reader-a.md`）；P-READER-B：0 + 21（`reader-b.md`）；P-READER-C：2 + 7（`reader-c.md`）
  - P-REFUTE：6 条 CLAIM（阻断 0 / 非阻断 1 / 反证失败 5），`refute.md`
- **VARIANCE 1（派发降档）**：平台并发上限实测为 3，六角色按 3+3 两波派发（第一波 fact/reader-a/design-d2，第二波 reader-b/reader-c/refute）；第一次派发尝试 6 并发被限流 3 个，已作废重派。按 L1.1 的降档协议记档；风险：两波之间无交叉阅读，材料隔离不变。
- **VARIANCE 2（面板标题层级规范化）**：design-d2 原用 `### FINDING <ID>` + 裸 `## FINDING` 标题、refute 的机器索引挤成单标题——都会让 INDEX 计数与覆盖率口径分叉。主 agent 把 15 条 `### FINDING` 升为 `##`、删裸标题、refute 索引重排为 `## CLAIM 11..16` 六条；正文内容未改动。面板文件 sha256 以规范化后的 INDEX 为准。
- 制品入库：`panel/round3/` + `INDEX.md`（6 文件：FINDING 22 / SUGGESTION 46 / CLAIM 6）。
- certify 回执：`certify/round3-certify.md`（首轮 6 CLOSED / 2 STILL-BROKEN；按证据改正 3 处后复核 8/0 PASS）。

### R3 §2 发现

- 74 条机器可计：FINDING 22（fact 2、design-d2 15、reader-a 3、reader-b 0、reader-c 2）+ SUGGESTION 46 + CLAIM 6。
- 严重度：P0 = 0；P1 = 0；P2 = 8（fact 0、design 5、reader-a 2、reader-c 1）；P3 = 66。
- 面板要点：文档的引文保真与常量对账几乎全绿（fact 实测 25 块引文逐字一致）；设计侧抓到的是「统一抽象为兼容三架构付出的隐性税」（死写点、三处错误码编码、哑实例哨兵、零消费者关联类型）；读者侧两条 P2 是真矛盾（tss_init 参数位次、user_accessible 覆盖口径）。

### R3 §3 归因

- 29 个根因（RD-R3-01..29）。
- 冻结决策检查：无命中；5 条设计 P2 按 §3.6 记 ② 延期（D-066）；2 条 ARCH 提案进判决账待裁；CLAIM 11..16 中 5 条反证失败记 FP、1 条非阻断已修记 ①。
- 范围：全部 in-scope。

### R3 §4 修复

- 8 处文档修改点：§2.8 `tss_init` 参数位次（第一→第二参）、§5.4 M1 的 user_accessible 口径与 §5.1 对齐、§3.3 E1 指针、§5.1 测试计数口径（行首 `#[test]` 属性 + 374/95 的文件数辨析）、§4.2 两处省略号内容区分（含方向与行数按 certify 证据改正）、§4.3 门类型两个写点（init 的 8 个 trap gate + set_handler 重写）、§2.5 lgdt→lidt 窗口补「异常递送轴不设防」、§2.1 反事实补「先炸排序没有取证」、§2.4/§3.2 两处引文精度（`1 * sizeof(reg_t)`、trap_entry 文件头覆盖半边）。
- 文档六门：anchor 75/75、citation `checked=32 failed=0`、structure/ref/style(error=0)/density 通过、doc-code-map 21/21。

### R3 §5 复验

- 独立回执 `certify/round3-certify.md`：首轮 8 条判 6 CLOSED / 2 STILL-BROKEN（第 4 条 374/95 口径串位、第 8 条 env_get 排序限定未写入），另指出第 5 条的方向/行数措辞偏差；三处按证据改正后复核 8/0 PASS。
- **回执抓到主 agent 两处实质问题**：①「374/95」把 tests 侧的 95 处命中写成 src 文件数；② §4.2 省略标注把「拓扑计算在调用之前」写成「其后」。均已按 path:line 改正。
- `certify 新准入 = 0` 口径：certify 输入闭集 = 8 个修改点；回执覆盖全部，复核后无新增修改点。
- 代码行为：本轮无代码改动；QEMU 未跑。

### R3 §6 指标

- 与 `metrics.md` 的 `## R3 确认轮` 块一致：发现总数 74、唯一根因数 29、本轮新准入 29、certify 新准入 0、处置 ① 5 / ② 5 / ④ 3 / FP 5 / SUGG 56、未闭环 0。
- 轮型 `confirmation`；轮结果 `CONVERGED`。

### R3 §7 状态

- `CONVERGED`。前提：R1 是已关闭的修改轮，R3 certify 新准入 0。
- 未闭环 0；`OPEN-DEBT` 的 `R3-design-batch` 有 D-066 判决行与日期。
- `CONVERGED` 的语义限制同 D01/D02。

### R3 §8 给 meta-review 的不确定点

1. 本轮 5 条设计 P2 全是「翻译税」类（死写点/三处编码/哑实例/零消费者），修正都要动代码；确认轮权限把它们推入 design batch。若 meta 认为其中 [DEL] 类应视为事实缺陷在轮内修，本轮不能以确认轮关闭。
2. refute 的机器索引由主 agent 追加并重排（VARIANCE 2），正文未改动；这个「面板后处理」的边界（允许动什么）规则没有明文。
3. 派发降档（VARIANCE 1）是平台并发限制，不是流程选择；两波之间的独立性没有机制保障。

### L6

- 同 agent 收口（另有独立 certify 回执，见 §5）。流程质量记录：fact 角色实测「25 块引文逐字一致、约 90 项常量现算一致」说明 D03 在 R2 后的文档质量已经很高（只报 2 条 P3 事实项）；设计侧的「翻译税」画像与 D01/D02 的发现同型（统一抽象的隐性代价）。
- 流程质量结论：`IMPROVED`（面板 74 条里 P2 仅 8 条、P0/P1 为 0，比 D01 R35 的密度显著下降；certify 抓到的两处主 agent 错误说明回执纪律持续有效）。

### L6.5

- 三问：① 不变量是「确认轮在最终工作树上 certify 新准入 = 0」；② 前提是 R1 已 ROUND-CLOSED；③ 证据是 certify 回执 8/0 与六门读数。
- 资源重读：盲区属「流程类」（面板后处理边界），查 `fresh-eyes-review`；无新检测模式，记 BLIND-SPOTS 候选。
