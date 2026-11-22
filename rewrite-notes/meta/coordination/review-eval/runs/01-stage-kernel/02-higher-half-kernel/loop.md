# D02 高半内核 —— V3.11 独立账户

## R0 账户初始化（2026-11-22）

- 文档：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 规则版本：`V3.11`
- 轮次：`0`（无 full、无 confirmation、无 trajectory）
- 旧 V3.10 轨迹：`rewrite-notes/meta/coordination/review-eval/archive/V3.10/01-stage-kernel-runs/02-higher-half-kernel/`
- 本账户从 R1 开始；上一 edition 的任何轮次、发现数、冻结状态都不继承。

## R1 full（V3.11，2026-11-22）

### §0 基线

- commit: `6ac2da259ccabd981453fb6701eea1bb4a81ac97`
- 文档 sha256: `b34ae2fd7d8b74145cb2e6b56e061cb0b10472862f1fa36fb7335cecb397d7b0`
- 行数: `2108`
- 单元: 6 章 + 35 节 = 41
- 关联代码: `related-code.txt` 30 个文件，`doc-code-map --check` 30/30
- L0.9: doc-publication 0 新增；tutorial 6/6；TODO `--fail-on-open` OK；code-publication 30 文件 0 新增
- 轮型: `full`；schema: `V3.11`；edition: `V3.11`
- 目标：第 1 个 full 轮，覆盖全部 41 个单元；D01 的轮次/证据不继承

### §1 面板

- 角色：P-FACT、P-DESIGN D1/D2、P-READER A/B/C、P-REFUTE；全部冷启动、未参与修复。
- 制品：`panel/round1/{fact,design-d1,design-d2,reader-a,reader-b,reader-c,refute}.md`。
- D2 为窄审查，结论 PARTIAL；未发现 R1 修复引入的 P0。

### §2 发现

- P-FACT 1 P2（`InvalidSegment` 返回点计数不完整）+ 2 SUGG。
- P-REFUTE 14 CONFIRM、0 REFUTE、1 UNCLEAR（check-layout 实跑属运行态）。
- READER-A：2 主冲突 + 5 FINDING；READER-B：3 FINDING；READER-C：3 FINDING。
- 合并 12 个根因；无 P0；P1 集中在 Step 2 条件、L3 子断言、四时刻主体、bootstrap 栈前提、症状表、§4.0 建根主语。

### §3 归因

- 12 根因：事实/计数 1、条件/断言 2、主体/因果 3、分类/范围 2、症状/引用 2、症状拆分 1、设计取舍 1（FP）。
- 跨角色同根：L3b/L3c（A-F1/B-F3）、栈症状（§2.3/§6.1）、bootstrap 前提（§2.3/§2.5）。

### §4 修复

- 逐条按 fix-guard：InvalidSegment 计数、§3.2 内核解析限定、§4.3 Step 2 守卫、§5.2 L3/L6 子断言、§1.1 映射主体、§6.1 引用与症状拆行、§3.1 三形态、§5.4 闭合定义、§2.5 假设/时间窗、§4.0 建根主语。
- 未改代码；D2 的根 typestate/EnterTarget/1GiB 删除不实施。

### §5 复验

- 文档门全绿：publication 0 新增、tutorial 6/6、structure/ref/style/citation/anchor/density 通过。
- 单元：audit open=0；R1 full 41/41 覆盖；32 pass + 9 fix。
- 代码未改；文档中的计数按当前源码回填。

### §6 指标

见 `metrics.md` 的 `## R1 full（V3.11）`：12 根因、11 ① + 1 FP、未闭环 0。

### §7 状态

`ROUND-CLOSED`，进入 `CONVERGENCE-CANDIDATE`；R2 必须整篇复审，R3 逐单元确认。

### §8 给 meta-review 的不确定点

1. R1 面板在修复前读文本；修复后只跑机械门，R2 必须重读。
2. D2 的根 typestate/EnterTarget 是设计候选，未实施；不进入文档修复。
3. READER 的 SUGGESTION 未逐条实施；R2 若复现再升级。
