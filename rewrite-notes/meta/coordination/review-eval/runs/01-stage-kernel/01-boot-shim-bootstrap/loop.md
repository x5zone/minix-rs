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
