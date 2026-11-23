TRAJECTORY-REVIEW R2 PASS
DOC 01-boot-shim-bootstrap.md

# y03 R2 trajectory 独立复核

- 角色：fresh context 独立 R2 trajectory reviewer。只读 R2 轨迹与证据，唯一写入是本报告。
- 复核对象：`v312/r2/`（defect-ledger、metrics、loop、panel/round2、certify、iterations、unit-evidence）。
- 基线：HEAD `bc60fb26d`（`chore(review): R2 closed with D-120/D-121`）；文档 sha `cbc105c790057db8ca93a63aa2ba7b94b214425d8b151e8c6e7a4867eb53ac9f`，`wc -l` = 2953。
- 复核时工作区：1 个已跟踪文件改动（`v312/r2/loop.md` 的 `units_review_lt2` 由 0 修正为 31）与 2 个预期未跟踪文件；无 R3 内容评价。

## VERDICT: PASS

R2 可关账：面 25 个 FINDING 全部有台账行与处置，台账 open=0，48/48 冻结，事件校验通过，`v312-closure-check --round 2` 在显式 run 目录下 PASS，D-120/D-121 已裁决并落地，冷读 `NOT-READY` 被保留为 R3 输入且未写成 `CONVERGED`。

阻断项：0。非阻断项：5（见文末，其中 N-1 是关账文档的措辞不一致，应在本轮归档前修）。

---

## 1. 面板 25 个 FINDING 覆盖；P-REFUTE 6 CLAIM 在附加表

命令与输出：

```text
$ for f in panel/round2/*.md; do grep -nE '^#{2,3} (FINDING|CLAIM)' "$f"; done
design-d2.md: FINDING DESIGN-01..05           （5）
fact.md:      FINDING FACT-01                 （1）
reader-a.md:  FINDING READER-A-01..06         （6）
reader-b.md:  FINDING READER-B-01..03         （3）
reader-c.md:  FINDING READER-C-01..10         （10）
refute.md:    CLAIM 01..06                    （6，无 FINDING）
```

```text
$ sed -n '9,46p' defect-ledger.md | awk -F'|' ...   # 抽出主表「发现 ID」列
READER-A-02、READER-B-02 | READER-C-02 | READER-A-01、READER-B-03 | READER-A-03 |
READER-A-04 | READER-C-01 | READER-C-03 | READER-B-01 | DESIGN-01 | DESIGN-02 |
READER-A-05 | READER-A-06 | READER-C-04 | READER-C-05 | READER-C-06 | READER-C-07 |
READER-C-08 | READER-C-09 | READER-C-10 | DESIGN-03 | DESIGN-04 | DESIGN-05 |
FACT-01 | REFUTE-02 | REFUTE-06
```

- 主表 25 行覆盖面板全部 25 个 FINDING ID：A-01..06（6）、B-01..03（3）、C-01..10（10）、DESIGN-01..05（5）、FACT-01（1）。两个合并行（A-02+B-02 → R2-01；A-01+B-03 → R2-03）不丢 ID。
- 面板严重度与台账声明一致：P2 11（A×4、B×2、C×3、DESIGN×2）/ P3 14（A×2、B×1、C×7、DESIGN×3、FACT×1），合计 25。
- P-REFUTE 6 条 CLAIM 全部在 `## P-REFUTE 附加`：REFUTE-01/03/04/05 反证失败记 FP，REFUTE-02/06 两条 P2 观察各转 `①` 并进主表（R2-26/R2-27）。
- 结果：PASS。

## 2. metrics 与台账一致性

`metrics.md` 实读：

```text
- 发现总数: 25
- 唯一根因数: 25
- 本轮新准入: 25
- certify 新准入: 0
- 重复合并: 2
- 自引入: 3
- 处置: ① 25 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 0 / SUGG 26
- 未闭环: 0
- 轮型: full
- 轮结果: ROUND-CLOSED
```

```text
$ grep -oE 'R2-[0-9]{2}' defect-ledger.md | sort -u | wc -l
25
$ grep -n '⑤' defect-ledger.md
7:- 处置：① 25 / ⑤ 0 / FP 0（R2-23 的 Default 子项保留，不单列）；未闭环 0（台账层）。轮结果 `BLOCKED` …
```

- 发现总数 25 = 面板 25 个 FINDING ID；唯一根因数 25 = 台账 25 个不同 `R2-*` 根因 ID；处置 ①25/⑤0；未闭环 0；轮型 full；轮结果 ROUND-CLOSED。计数与台账逐项一致。
- 不阻断的编号瑕疵：根因 ID 存在空号 `R2-09`、`R2-10`（`R2-01..R2-08、R2-11..R2-27`），不同 ID 数仍为 25，不影响三个计数，但削弱逐号追溯（见 N-3）。
- 不一致：台账头第 7 行仍写“轮结果 `BLOCKED`”，`loop.md` §6（第 79 行）也复述“metrics.md：… 轮结果 `BLOCKED`（冷读/R3）”；而 `metrics.md` 第 14 行是 `- 轮结果: ROUND-CLOSED`，`loop.md` §7 是 `ROUND-CLOSED`。git 证据显示 `bc60fb26d` 只把 metrics 的 `BLOCKED` 改成 `ROUND-CLOSED`，未同步台账头与 loop §6：

```text
$ git show bc60fb26d -- .../metrics.md
-- 轮结果: BLOCKED
+- 轮结果: ROUND-CLOSED
```

  按 item 2 的字面“与台账一致”，这一子项不成立；但台账的实质计数（①25/⑤0/open 0）与 metrics 一致，冲突仅在轮结果措辞。按非阻断记（N-1），关账前应统一为正式状态 `ROUND-CLOSED`（或反之，若作者判定 `BLOCKED`，则 metrics/loop §7 要改回，且应写明对应的 BLOCKED 类型）。
- 结果：计数 PASS；轮结果措辞一致性 FAIL（非阻断，N-1）。

## 3. 单元状态、台账审计、事件校验

```text
$ python3 tools/review-units.py stats rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
chapters=6 sections=42 frozen=48/48
t1_pending=0 t2_pending=0 reopened=0 stale=0 t3=FAIL
round=2 full_rounds=2 trajectory_reviews=1 units_review_lt2=31 units_post_freeze_0=48
```

```text
$ python3 tools/review-units.py audit rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md \
    --ledger .../v312/r2/defect-ledger.md
（尾部）… 6.4: open=0
review-units audit: OK（全部单元 open=0）
```

```text
$ python3 tools/review-units.py event verify rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --fresh
events=30 completed=30 fresh_only=1
event verify: OK
```

- 必查三项全部命中：frozen=48/48、t1_pending=0 / t2_pending=0、reopened=0 / stale=0。
- `t3=FAIL` 是预期：T3 门要求事件链与 G18 冷读同时成立，而 R2 冷读仍是 `HARD-REJECTIONS: 4 / NOT-READY`（见第 6 项）；本次 trajectory 复核本身也不构成 G18 冷读通过。此项不按 R2 缺陷记。
- `units_review_lt2=31`：R2 改动后 31 个单元只有 R2 这一轮有效 review；调度方已裁定为已知状态、由 R3 补到 ≥2 轮，不作为 R2 FAIL。工作区 `loop.md` §7 已把提交里的 `units_review_lt2=0` 修正为 31（`git diff` 仅此一处，2 行）。
- 结果：PASS。

## 4. `v312-closure-check --round 2`

```text
$ python3 tools/v312-closure-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md \
    --round 2 \
    --run-dir rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r2 \
    --ledger rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r2/defect-ledger.md
v312-closure-check: PASS
EXIT=0
```

```text
$ python3 tools/v312-closure-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --round 2
[FAIL] loop.md 缺 `schema: V3.12 from R2`
[FAIL] metrics.md R2 缺 `- 轮结果:` …（共 12 项）
v312-closure-check: FAIL(12)
EXIT=1
```

- 显式 run 目录 + 台账路径时 PASS；不带 `--run-dir` 时工具按 `runs/<stage>/<doc-stem>` 定位，找不到 `v312/r2`，读到别处制品后报 12 项 FAIL。
- 结论：轮次关账检查本身 PASS，但关账记录必须写明 `--run-dir`/`--ledger`（见 N-2）。结果：PASS（附调用方式约束）。

## 5. D-120/D-121 裁决、实现 commit、独立设计 verifier

```text
$ rg -n 'D-120|D-121' rewrite-notes/meta/coordination/review-eval/DECISIONS.md
195:| D-120（2026-11-23 12:00，作者裁决：R2 DESIGN-01 选 A） … 已实施：`BOOT_MODULE_IDENTITIES` … |
196:| D-121（2026-11-23 12:00，作者裁决：R2 DESIGN-02 选 A） … 已实施：kmain strict … |
```

```text
$ git show 7d6835aa5 -- .../defect-ledger.md | grep -E '^[+-].*(DESIGN-01|DESIGN-02|⑤|①)'
-- 处置：① 23 / ⑤ 2（DESIGN-01/02 待作者） … 未闭环 2，轮结果 `BLOCKED`。
+- 处置：① 25 / ⑤ 0 … 未闭环 0（台账层）。…
-| DESIGN-01 | 3.6, 4.3 | ⑤ | 待作者裁决 … |
-| DESIGN-02 | 3.2, 4.4 | ⑤ | 待作者裁决 … |
+| DESIGN-01 | 3.6, 4.3 | ① | 按 D-120 选 A … |
+| DESIGN-02 | 3.2, 4.4 | ① | 按 D-121 选 A … |
```

```text
$ git show --stat --format='%h %s' 7d6835aa5
7d6835aa5 feat(boot): D-120 merged module table and D-121 strict kmain
 os/boot-shim/src/uboot_main.rs | 10 +---   os/kernel/src/lib.rs | 16 ++----
 os/kernel/src/proc.rs | 25 +----             os/libs/minix-boot/src/kernel_info.rs | 67 ++++--
 os/libs/minix-boot/src/memmap_source.rs | 24 ++ os/xtask/src/manifest.rs | 33 ++---
 plus rewrite-notes 文档/DECISIONS/台账/metrics

$ rg -n 'BOOT_MODULE_IDENTITIES' os/libs/minix-boot/src/kernel_info.rs | head
685:pub const BOOT_MODULE_IDENTITIES: [BootModuleIdentity; NR_BOOT_MODULES] = [
$ rg -n 'validate_requiring_sources\(\)' os/kernel/src/lib.rs | head
747:    kinfo.validate_requiring_sources();
1867:    kernel_info.validate_requiring_sources();
```

```text
$ grep -nE 'VERDICT|RECHECK' .../v312/r2/certify/y03-design-verify.md
6:- 结论：**VERDICT: PASS**
223:**RECHECK: PASS**
```

- 台账在 `7d6835aa5` 中从 `① 23 / ⑤ 2` 改为 `① 25 / ⑤ 0`，DESIGN-01/02 行由 `⑤` 改 `①`；作者裁决文本在 `DECISIONS.md` D-120/D-121；实现 commit 含六个 `os/` 文件；独立 design verifier 结论 PASS、复检 RECHECK PASS。
- 结果：PASS。

## 6. R2 冷读保留为 R3 输入，未写成 CONVERGED

```text
$ head -2 .../v312/r2/certify/y02-cold-read.md
HARD-REJECTIONS: 4
VERDICT: NOT-READY
$ cmp -s tmp/review-loop/01-stage/cold-read/y02-d01-r2.md .../v312/r2/certify/y02-cold-read.md && echo IDENTICAL
IDENTICAL
$ sed -n '81,87p' .../v312/r2/loop.md
### §7 状态
- `ROUND-CLOSED`：25 个 FINDING 全部 `①` 已修/裁决 … `units_review_lt2=31` … 48/48 冻结。
- **R2 冷读仍 `NOT-READY`（4 个结构阻断，`R2-COLD-READ-CASE.md`）**，这是 R3 的输入，
  不是 R2 未闭环的 FINDING；不写 `CONVERGED`，下一轮是 R3 结构轮。
```

- 冷读原文与归档副本逐字节一致；`HARD-REJECTIONS: 4 / NOT-READY` 未被隐藏，loop §7 明确不写 `CONVERGED`，案件卷 `R2-COLD-READ-CASE.md` 在 run 目录。
- 结果：PASS。

## 7. V3.11 混放与 git status

```text
$ rg -n 'V3\.11' v312/r2/ rewrite-notes/meta/review/state/01-stage-kernel/01-boot-shim-bootstrap.yaml rewrite-notes/meta/review/events.csv rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
v312/r2/SCOPE.md:22:5. 不使用 V3.11 的旧面板、旧台账、旧冻结证据、旧 CONVERGED 结论。
v312/r2/SCOPE.md:41:- V3.11 历史轮次的结论、面板、冻结状态。
$ rg -n 'V3\.11' os
（无输出）
```

- 仅 SCOPE 的“排除 V3.11 输入”声明命中，不是混放；状态 schema `review-units/v2`、edition V3.12；events.csv 各行 edition 为 `V3.12`、round 2；文档与 `os/` 无 V3.11 残留。
- git status（HEAD `bc60fb26d` 之后）：

```text
 M rewrite-notes/meta/coordination/review-eval/runs/.../v312/r2/loop.md
?? rewrite-notes/01-stage-kernel/01-design-learn.md
?? rewrite-notes/meta/coordination/review-eval/meta/01-boot-shim-bootstrap-r26.md
```

  两个未跟踪文件与交接记录（`iterations/y03-handoff.md`）的“两个预期未跟踪文件”一致。另有一处 loop.md 未提交改动（`units_review_lt2` 0→31 的据实修正），属已知状态修正，不是新制品。
- 结果：PASS（附 N-4：该 loop.md 修正尚未提交）。

## 8. 正式文档引用、metrics 键、loop 九节

```text
$ rg -n 'tmp/|rewrite-notes/meta|review-eval/runs|tmp_design_and_todo' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
（无匹配；rg 退出码 1）
$ grep -cE '^- [^:]+:' .../v312/r2/metrics.md
19
$ grep -nE '^### §[0-8]' .../v312/r2/loop.md
19:§0 基线  26:§1 面板  36:§2 发现  46:§3 归因  52:§4 修复
65:§5 复验  77:§6 指标  81:§7 状态  89:§8 给 meta-review 的不确定点
```

- 正式文档不引用 `tmp/` 或 `meta/` 中间产物；`metrics.md` 含 review-evals §3.4 要求的全部 19 个固定键（提示词所称“16 键”是下限，实读 19 键齐全；`ARCH 提案`、`用户否决` 各重复一行但不缺键）；`loop.md` §0–§8 九节齐全。
- 结果：PASS。

---

## 非阻断项

| 编号 | 位置 | 事实 | 处理建议 |
|---|---|---|---|
| N-1 | `defect-ledger.md:7`、`loop.md:79` | 仍写“轮结果 `BLOCKED`”，与 `metrics.md:14`、`loop.md:83` 的 `ROUND-CLOSED` 冲突；`bc60fb26d` 只改了 metrics。实质计数 ①25/⑤0/open 0 一致。 | 归档前统一为 `ROUND-CLOSED`（或按作者裁决统一为 `BLOCKED` 并写明 BLOCKED 类型）；否则 item 2 的“与台账一致”字面不成立。 |
| N-2 | 关账命令 | 裸 `v312-closure-check <doc> --round 2` 因默认 run 目录不含 `v312/r2` 而 FAIL(12)；显式 `--run-dir`+`--ledger` PASS。 | 报告与复跑命令固定写全 `--run-dir`、`--ledger`。 |
| N-3 | `defect-ledger.md` 根因列 | `R2-09`、`R2-10` 空号；不同根因 ID 仍为 25。 | 加一行空号说明（合并/重编号所致）或重排编号，便于逐号追溯。 |
| N-4 | `loop.md` §7（工作区） | 提交版写 `units_review_lt2=0`，实际 stats=31；工作区已改但未提交。 | 随 R2 归档一并提交该据实修正。 |
| N-5 | `loop.md` §5 | 写 doc `d035c692…` “2,951 行”，design verifier 对同一 sha 记 2,953 行；当前 doc `cbc105c7…` 为 2,953 行。 | 统一行数口径（`wc -l` 口径与编辑器/统计口径差异需注明）。 |

## 未覆盖

- 未重跑宿主测试、QEMU 门与 clippy；按 design verifier 的记录采信（其报告列了命令与读数）。
- 未逐条重放 25 个 FINDING 的复验命令（只核台账命令格式与 `open=0`）；doc/code verifier 的 PASS 按其报告采信。
- 未评价 R3 结构轮的设计与内容。
