# D01 启动交接：boot-shim 与 bootstrap —— V3.11 独立账户

## R0 账户初始化（2026-11-22）

- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 规则版本：`V3.11`
- schema: V3.11 from R1
- edition: V3.11
- 轮次：`0`（无 full、无 confirmation、无 trajectory）
- 旧 V3.10 轨迹：`rewrite-notes/meta/coordination/review-eval/archive/V3.10/01-stage-kernel-runs/01-boot-shim-bootstrap/`
- 本账户从 R1 开始；上一 edition 的任何轮次、发现数、冻结状态都不继承。

## R1 full（V3.11，2026-11-22）

### §0 基线

- commit: `3c9d1ef1f0709ad58a5b3424fb7eebd7387716f0`
- 文档 sha256: `572bb4c164e9ad66de001ab248b72255df9bf20a19779092b3f84bcf569947e0`
- 行数: `2968`
- 单元: 6 章 + 43 节 = 49
- 关联代码: `related-code.txt` 48 个文件，`doc-code-map --check` 48/48
- L0.9 门: doc-publication 0 新增；tutorial 6/6 0 缺项；TODO `--fail-on-open` OK；code-publication 48 文件 0 新增
- 范围：D01 文档 + 48 个关联文件；不含其它篇目
- 轮型：`full`；schema：`V3.11`；目标：第 1 个 full 轮，覆盖全部 49 个单元

### §1 面板

- 角色：P-FACT、P-DESIGN D1、P-DESIGN D2、P-READER A/B/C、P-REFUTE；均为未参与修复的冷启动 agent。
- 制品：`panel/round1/{fact,design-d1,design-d2,reader-a,reader-b,reader-c,refute}.md`。
- D1 先完成，D2 在 D1 落盘后启动；D2 为窄审查（wire/所有权/KernelInfo 三维），无 P0。
- 材料隔离：读者角色未读源码/历史评审；设计 D1 未读现有实现与 Ch3/Ch4；D2 未读作者解释。
- 缺口：P-FACT 报告 P0 1 条（pt_alloc 引文与当前源码脱节），D2 仅设计意见，无 P0。

### §2 发现

- P-FACT 8：P0=1（pt_alloc 旧实现）、P2=4（2.4.2 重号、3.2.2.4 重标题、authority 类型、minix-boot 187/188）、P3=3（引文标注、行号、user_sp 读者）。
- READER-A 5：P1=2（高半归零证据强度、§5.5 无缺口口径）、P2=2（前置概念、D1–D5 学习目标）、P1 编号重复并入 RD-R1-002。
- READER-B 7：P1=2（§2.4.2.3 结论前置、§4.2.1 动作编号）、P2=5（2.4.2、3.2.2.4、§2.6、§2.7、§3.2.2）。
- READER-C 5：P1=1（§5.4 DTB preflight 变体）、P2=4（统一消费次序、D1–D5 范畴、U-Boot arena 指代、分类结果双写）。
- P-REFUTE 15：13 CONFIRM、1 REFUTE（minix-boot 187→188）、1 UNCLEAR（§5.5 全称否定）。
- 根因：22 条，见 `defect-ledger.md` RD-R1-001..022；1 条 FP（D2 设计意见不进入文档修复）。

### §3 归因

- 22 个根因：文档-代码同步 1（P0）、编号/标题 2、事实/类型/计数 4、证据强度/范围 3、术语/范畴/指代/双写 4、结构/边界/执行序 5、引文/行号/读者口径 3。
- 跨角色同根合并：编号重复（FACT-2/RA-F3/RB-B01）、标题重复（FACT-3/RB-B02）、§5.5 口径（RA-F2/REFUTE-15）。
- 设计意见 RD-R1-022：D2 明确“无 P0”，F1 非行为缺陷，F2 是 D1 报告错靶，F3 是 [ARCH] 提案；不进入 D01 修复。

### §4 修复

- 批次 A（P0 + 机械）：pt_alloc 引文块按 `AtomicUsize` + `compare_exchange` 重写；四处旧标志引用同步；2.4.2 第二处改 2.4.2.5；删除重复 3.2.2.4；authority 改 `RamAuthority::DTB/UEFI`；187→188；KinfoStruct 读者口径；引文标注；ebs_pair 行号。
- 批次 B（结构/证据）：§1.0/§1.1 高半归零限定；§5.5 范围限定；§2.0 目标；§2.8 范畴；§3.6 arena 指代；§3.2/§4.1 source 例外；§5.4 DTB preflight 前提；§2.6 三层表；§2.7 边界收紧；§2.4.2.3 判决框；§3.2.2 标题；§4.2.1 执行序主表。
- 每批按 fix-guard：先 grep 现状，改后回读邻句；未改代码。

### §5 复验

- 文档门：publication 0 新增；tutorial 6/6；structure/ref/style 通过；citation `blocks=54 checked=21 failed=0`；anchor 90/90；density 4/4 可判章通过；doc-code-map 48/48。
- 单元账本：`check` OK，`audit` open=0；R1 full 49/49 覆盖；35 pass + 22 fix。
- 代码：R1 未改 `os/`；P0 是文档对既有代码的描述修正，代码计数按 `cargo test -p minix-boot --lib` 188/0 回填。
- 复验命令见 `defect-ledger.md` 每行；未闭环 0。

### §6 指标

见 `metrics.md` 的 `## R1 full（V3.11）`：发现 40、根因 22、新准入 22、处置 ①21/FP1、未闭环 0、full 49/49。

### §7 状态

`ROUND-CLOSED`，进入 `CONVERGENCE-CANDIDATE`。full 轮无 CONVERGED 声明权；R2 仍需完整全篇复审，R3 逐单元确认。

### §8 给 meta-review 的不确定点

1. P-FACT P0 的修复是文档引文重写，未改代码；需 R2 复核引文块与源码逐字一致。
2. §2.7 的边界收紧是否过度，需 R2 读者复读。
3. D2 的 F1/F3 是设计意见，未实施；是否作为独立设计候选另案，需作者/me-agent 裁决。
4. R1 的单元证据是覆盖行 + 面板制品，不是逐单元独立回执；R2 将逐单元 pass/freeze，R3 逐单元 confirm-review。

### L6

- 同轮独立面板 + 主 agent 归因。P-FACT 的 P0（pt_alloc 引文与当前源码脱节）是本轮最重要的检出：单元冻结/旧确认不能替代当前源码对账。
- 结构角色（B-05/B-07）与因果角色（C-1/C-4/C-5）互补：一个抓叙事顺序，一个抓限定语丢失；两者都指向“实现章说清、概述复述丢限定”的同类模式。

### L6.5

- 三问：① 本轮不变量 = 49 单元全覆盖 + P0 文档-代码同步 + 22 根因清零；② 基线 = commit `3c9d1ef1f` / 文档 sha `572bb4c1…`；③ 证据 = 7 份面板报告 + defect-ledger 24 行 + 文档门/单元账本。
- 结论：R1 可关闭为 ROUND-CLOSED；R2 仍需独立全篇复审，R3 逐单元确认。

## R2 full（V3.11，2026-11-22）

### §0 基线

- commit: `d683ee10c6c07b63c31f67dd79e2c49adf4e55ce`
- 文档 sha256: `7450c57c65cfc61e75898d115004babc791bbca9526d16aacdd704697bb348a3`
- 行数: `2973`
- 单元: 49；R1 已记录 full 轮，R1 的 22 个 fix 单元未冻结
- 范围：整篇 D01 + 48 个关联文件；R2 是独立 full 轮，必须覆盖全部 49 个单元
- 轮型：`full`；schema：`V3.11`；目标：复核 R1 修复并寻找新问题
- R1 轨迹：`trajectory-r1.md` PASS

### §1 面板

- 角色：P-FACT、P-DESIGN D2、P-READER A/B/C、P-REFUTE；全部冷启动、未参与 R1 修复。
- 制品：`panel/round2/{fact,design-d2,reader-a,reader-b,reader-c,refute}.md` + INDEX。
- R2 是独立 full 轮：面板重新读整篇；不复用 R1 报告作为结论。

### §2 发现

- P-FACT 2（P1 §5.1 断表、P2 Serializer 实参）。
- P-REFUTE 2 REFUTE（§5.4 aarch64 DTB 变体写反、§5.1 断表）+ 4 UNCLEAR（门覆盖计数/范围）。
- READER-A 3（断表、§4.4.1 漏 store、§5.5 门计数）。
- READER-B 4（2.4 编号层级、断表、漏 store、悬空引用）。
- READER-C 8（source 例外无条件、aarch64 kmain 证据、§4.8 v1 建根链、DTB_CFGTABLE 命名、断表、一条/两条依赖、same pool、wire 版本定义）。
- D2 3（source 两阶段、§3.2.1 authority、source_records 归属）。
- 合并为 14 个根因；其中 2 条是 R1 修复引入的回归（§5.1 断表、§5.4 变体写反），已在本轮修复。

### §3 归因

- 14 个根因：结构/表格 2、执行序/链 3、source/authority 归属 3、证据范围 2、编号层级 1、引用 1、术语/计数/版本 3、设计事实 1（并入 14）。
- 跨角色同根已合并；无新增 ARCH 实施。

### §4 修复

- 一次一条按 fix-guard：§5.1 表恢复、§4.4.1 补 store、§5.4 DTB 变体纠正、2.4 层级重排并 prune 旧单元、source 例外限定 UEFI、§4.8 分两链、DTB_CFGTABLE 定义、引用/计数/版本措辞、D2 三条事实改正。
- 文档从 2973 行变为 2980 行；单元 49→48（2.4.2 从 `###` 降为 `####`，并入 2.4）。

### §5 复验

- `doc-structure-check`、`doc-ref-check`、`doc-publication-check`、`tutorial-rubric-check`、`anchor-resolve`、`doc-code-density` 全绿。
- 单元：48/48 frozen；audit open=0；R2 full 覆盖 48/48。
- 宿主：minix-boot 188/0、boot-shim 136/0、minix-arch 266/0、minix-kernel mock 866/0/8。
- 未闭环 0。

### §6 指标

见 `metrics.md` 的 `## R2 full（V3.11）`：14 根因、14 处置、0 未闭环、2 自引入已修。

### §7 状态

`ROUND-CLOSED`，进入 `CONVERGENCE-CANDIDATE`；R3 confirmation 逐单元 `--confirm-review`。

### §8 给 meta-review 的不确定点

1. R2 的 5 条 SUGGESTION（§3.2.2 拆分、§2.4.2.5 范围、§7.5 指针等）未逐条实施；R3 若复现则转 P1。
2. R2 面板在修复前读文本；修复后只跑机械门，未再派全量读者。R3 必须逐单元确认。
3. `2.4.2` 单元从 `###` 降为 `####` 后 prune，历史 R1 证据仍按 49 单元记录；当前账户 48 单元。

## R3 confirmation（V3.11，2026-11-22）

### §0 基线

- 文档 sha256: `9ca2e153ecc0a9562c9062f4b5572f4b5a08cddf69b764fc3f3d8b9404affbf2`，2973 行
- 单元：48/48 frozen；R2 已 ROUND-CLOSED；trajectory R2 PASS

### §1 面板

- 确认轮逐单元 `--confirm-review`，证据 `unit-r3/<id>.md` 引用 `confirmation-r3.md` 与 R2 面板；未开新全量面板。

### §2 发现

- 无新增根因；R2 的 14 条全部 CLOSED；R1 的 46 条保持 CLOSED。

### §3 归因

- 0 新根因；0 未闭环。

### §4 修复

- 无文档修改；只记录确认。

### §5 复验

- 48/48 单元 post_freeze_rounds 含 R3；review_rounds ≥2；T3 PASS。
- 文档门与宿主测试见 `certify/round3-gates.md`。

### §6 指标

- 发现 0；根因 0；未闭环 0；轮型 confirmation；机械结果 CONVERGED，但 2026-11-22 被用户人工审阅否决，最终状态 NOT CONVERGED（D-116）。

### §7 状态

最终状态 `NOT CONVERGED`（D-116：用户人工审阅 D01 后判定读不进去，D01–D03 一并撤销）。限制见 `FREEZE.md`：R3 逐单元确认引用 R2 面板与 confirmation 记录，不是 48 个独立冷读会话；D2 设计候选未实施。

### §8 给 meta-review 的不确定点

1. R3 的独立冷读强度弱于 R1/R2；若要求每单元独立冷读，需重开确认轮。
2. SUGGESTION 中的 §3.2.2 拆分、§2.4.2.5 范围等未逐条实施；当前未计入 open。
