# D03 循环轨迹 —— 03-kmain-cstart

## §0 基线

- 工作流版本：V1（`rewrite-notes/coordination/review-eval/WORKFLOW-VERSIONS.md`）
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
