TRAJECTORY-REVIEW R3
DOC 01-boot-shim-bootstrap.md
VERDICT: FAIL

# R3 轨迹复核（第 3 个修改轮）

- 复核者：fresh subagent，只读；未修改目标正文、状态文件与事件表。
- 复核对象：`rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3/` 的九节账、面板、复验与单元证据。
- 结论：第 1、3、4、5、6、7 项通过（第 5、6 项带证据缺口）；第 2、8 项失败。正文本身在机械门与独立复验上通过，但轨迹账目有数字矛盾、复验串失效、提交锚点不符与回执缺失，当前不能按 `ROUND-CLOSED` 关账。

## 1. 当前快照

| 项 | 实测 | 命令或来源 |
|---|---|---|
| 当前正文 sha256 | `67440cd885aad0e808cd5d76905323872b08b1c35849566b976c123d84dff6e8` | `sha256sum rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` |
| 当前行数 | 3085 | `wc -l rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` |
| loop 记录的修复后 sha/行数 | `67440cd8…` / 3085 | `r3/loop.md:10-11` |
| loop 声明的 R3 输入 | `98b7cdaf…` / 3049 行，commit `a6a94b942` | `r3/loop.md:7`、`r3/loop.md:22` |
| 输入 sha 的真实 commit | `9f1327187` 的正文是 `98b7cdaf…` / 3049 行 | `git show 9f1327187:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` |
| `a6a94b942` 的真实正文 | `cbc105c7…` / 2953 行（R2 终态） | `git show a6a94b942:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` |
| HEAD `2914a7e18` 的真实正文 | `98b7cdaf…` / 3049 行，不是 `67440cd8…` | `git show HEAD:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` |
| 工作区状态 | 正文、7 个 `os/` 文件、state、events、loop、metrics 均未提交；R3 的 ledger、panel、unit-evidence、fix-verify 为未跟踪文件 | `git status --short` |

## 2. 逐项检查

| # | 检查项 | 判定 | 关键证据 |
|---|---|---|---|
| 1 | loop 九节齐全、schema、sha/行数、§7 状态 | PASS | 九节 §0–§8 全在；`schema: V3.12 from R3`；修复后 sha/行数等于当前正文；§7 写 `ROUND-CLOSED` 且明确“不写 CONVERGED” |
| 2 | metrics 16 键、22/20/28/①28 自洽 | FAIL | metrics 自身 16 键齐全且写 `① 28`；ledger 头写 `① 25`，与 metrics 及台账 28 行矛盾；`22+3+3` 的说明漏掉 READER-C 的 2 条附带发现 |
| 3 | ledger 覆盖全部面板 FINDING；P-REFUTE 六条有结果；open=0 | PASS | 22 个主 FINDING 与 2 个附带发现全部有行；六条 CLAIM 有结果；`review-units.py audit` 全部单元 `open=0` |
| 4 | 48/48 冻结；四计数为零；R3 事件覆盖且新鲜 | PASS | `frozen=48/48`、`t1_pending=0 t2_pending=0 reopened=0 stale=0`；`event verify --fresh` 退出 0 |
| 5 | 29 个解冻单元有 R3 pass/freeze 与 unit-evidence；19 个保持冻结 | PASS（措辞有偏差） | 29 个单元 `review_rounds` 含 R3，29 份 unit-evidence 均首行 `UNIT <id>` 且含 PASS；19 个单元保留 R2 证据；但“29 个因正文变化”实际是 23 个正文变化节 + 5 个父章 + 4.7 的锚点级联 |
| 6 | 两份独立复验 VERDICT/RECHECK PASS；QEMU 四门 | PASS（无 EXIT=0 记录） | `r3-doc-verify.md` 前两行 `RECHECK: PASS` / `VERDICT: PASS`，`r3-code-verify.md:1` 与 `:108` 同；四份日志各有 PASS 判据行，但没有字面 `EXIT=0` |
| 7 | `v312-closure-check.py --round 3` | PASS（需带 `--run-dir`） | 带正确 run 目录输出 `v312-closure-check: PASS`、退出 0；裸跑会落到无版本的默认路径并误报 13 项失败 |
| 8 | 只看形式通过、数字对不上、未闭环写成关闭 | FAIL | 见第 3 节 B1–B4：提交锚点与 doc_sha 不符；`①25` 与 `①28` 矛盾；三条复验串零命中或覆盖不足；`certify/` 为空 |

## 3. 阻断项与最小修复

### B1 提交锚点与 doc_sha 不符（item 8，影响 1/3/4）

`loop.md:7`、`loop.md:22` 与 `defect-ledger.md:5` 都把输入 `98b7cdaf…`（3049 行）挂在 `a6a94b942` 下；但该 commit 的正文是 `cbc105c7…`（2953 行）。`98b7cdaf…` 实际在 `9f1327187`。同样，R3 修复后的 `67440cd8…`（3085 行）不在 HEAD `2914a7e18` 里，HEAD 的正文仍是 `98b7cdaf…`。

受影响的不只是文字：

- `state/01-stage-kernel/01-boot-shim-bootstrap.yaml:21` 的 R3 full-round 记录 `commit: a6a94b942…`，而 `full-r3.md` 对应的输入是 `98b7cdaf…`。
- `events.csv:231-242` 的六个 R3 面板事件全部写 `target_commit=a6a94b942…`、`doc_sha=98b7cdaf…`；该组合在任何 commit 上都不成立。
- `events.csv:243-244` 的 R3 full 事件写 `target_commit=2914a7e18…`、`doc_sha=67440cd8…`；`2914a7e18` 内没有 `67440cd8…`。
- 29 个 R3 单元的 `frozen_at` 是 `2914a7e18…`，但这些单元的冻结正文只存在于未提交的工作区；另 19 个单元的 `frozen_at` 仍是 R2 的 `b079fa0e…`。

`event verify --fresh` 对带 `unit_sha` 的新事件只比较单元指纹与 `target_commit` 的祖先关系，不比较 commit 内容与 `doc_sha`，所以这些不一致不会让门变红；这是“形式通过”的直接来源。

最小修复：先提交 R3 的正文、代码与 run 目录制品；然后对六个面板事件与 R3 full-round 重新登记 `target_commit=9f1327187`（输入）或提交后的新 commit（修复）；对 29 个单元重新冻结到含 `67440cd8…` 的 commit。若工作流确实允许“工作区冻结”，则必须在 ledger/loop 明写 `input_commit=9f1327187`、`fix_worktree_sha=67440cd8…`，不能继续把 `a6a94b942` 当成 `98b7cdaf…` 的 commit。

重放：

```bash
git show a6a94b942:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md | sha256sum
git show 9f1327187:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md | sha256sum
git show 2914a7e18:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md | sha256sum
```

输出分别为 `cbc105c7…`、`98b7cdaf…`、`98b7cdaf…`。

### B2 台账数字与说明不一致（item 2）

`metrics.md` 写 `发现总数 22`、`唯一根因数 20`、`本轮新准入 28`、`处置 ① 28 / FP 3 / SUGG 37`；`defect-ledger.md` 主表实际有 28 行，且 28 行的处置类全是 `①`。但 `defect-ledger.md:7` 写 `处置：① 25`。同一轮的两份账一个 25、一个 28，不能同时成立。

`22+3+3=28` 这个说明也不完整。面板实际有两类发现：

- 22 个主 `## FINDING`：READER-A 8、READER-B 5、READER-C 2、FACT 3、DESIGN 4。
- 2 个 `### 附带发现`：READER-C-A01、READER-C-A02，台账也已按 `①` 修复。

再加上 P-REFUTE 的 3 条观察与独立复验的 3 条 NEW，发现 ID 总数是 `22+2+3+3=30`。台账因 A-01/B-01、A-04/B-02 两组共用一行，记成 28 行。也就是说：

- 28 作为“台账行数”成立，`①28` 成立；
- 28 作为“22+3+3”的算式不成立，因为它漏掉了 2 条附带发现，也没有说明 30 个 ID 如何变成 28 行。

最小修复：

1. `defect-ledger.md:7` 改为 `处置：① 28 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 3（…）；未闭环 0`。
2. `metrics.md` 的说明与 `loop.md §6` 改成：“22 个主 FINDING + 2 条附带发现 + 3 条 P-REFUTE 观察 + 3 条复验新发现 = 30 个发现 ID；A-01/B-01、A-04/B-02 两组合并，台账 28 行，全部 ①。”
3. 若坚持 `本轮新准入` 表示“新根因数”（`review-evals.md` §3.4 的字面定义），则它应是 20，不是 28；请在 metrics 说明里明确本试点把该键当作“台账行数”使用，避免与规则定义混淆。

重放：

```bash
rg -n '^- (发现总数|唯一根因数|本轮新准入|处置|未闭环):' \
  rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3/metrics.md
rg -n '^- 处置：' \
  rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3/defect-ledger.md
```

### B3 三条台账复验串失效（item 8）

`defect-ledger.md` 的“复验”列是 L1 证据，但以下三条与实际正文不符：

| 行 | 台账期望 | 实测 | 说明 |
|---|---|---|---|
| READER-A-04、READER-B-02 | `IS（information service|RS（reincarnation|PM（process`，期望 ≥3 | 三条分支各 0 命中 | 正文写的是 `PM 是进程管理器（process manager）`、`RS 是重生服务器（reincarnation server）`、`IS 是信息服务（information service）` |
| READER-A-08、READER-C-S03 | `四条腿（x86 UEFI、aarch64 UEFI、riscv64 U-Boot shim、riscv64 \`bootface\`）`，期望 1 | 0 命中 | 正文写的是 `**有启动门链的四条腿**是 x86 UEFI、…` |
| READER-C-A02 | `两个直核腿的六类载荷`，期望 1 | 1 命中（只在 I3 行 L114） | 本轮新增的 §3.7 L1935 用的是 `两个直核腿各拥有一份栈上的`；只查旧串覆盖不到本轮修复点 |

`r3-doc-verify.md` §五已经记录了这三处覆盖不足，但 ledger 仍按 PASS 记账。最小修复：把三条检查串改成当前正文的实际措辞，并在 ledger 的复验格写入重放输出；`READER-C-A02` 再加一条 `两个直核腿各拥有一份栈上的`。

重放：

```bash
rg -c 'IS（information service|RS（reincarnation|PM（process' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
rg -c '四条腿（x86 UEFI' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
rg -n 'PM 是进程管理器|RS 是重生服务器|IS 是信息服务' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
rg -n '有启动门链的四条腿' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
rg -n '两个直核腿各拥有一份栈上的' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
```

### B4 `certify/` 为空，loop 指针落空（item 6/8）

`r3/certify/` 目前是空目录；`loop.md:77` 写“见 §8 与 certify 目录”，但 §8 是“给 meta-review 的不确定点”，不含复验内容，目录里也没有 `round3-*.md`。V3.5 制品链要求独立回执落在 `runs/<stage>/<doc-stem>/certify/round<N>-*.md`；R1、R2 都有对应回执，R3 没有。两份复验原文只存在于 `tmp/review-loop/01-stage/`，与“必须入库”的制品链不一致。

最小修复：把

- `tmp/review-loop/01-stage/r3-doc-verify.md` 复制为 `r3/certify/round3-doc-verify.md`；
- `tmp/review-loop/01-stage/r3-code-verify.md` 复制为 `r3/certify/round3-code-verify.md`；

并把 `loop.md:77` 的“见 §8 与 certify 目录”改成这两个实际路径（或删除“§8”）。复制后重跑 `panel-index.py --check`（面板不受影响）与 `v312-closure-check.py --run-dir … --round 3`。

## 4. 非阻断观察

1. **QEMU 日志无字面 EXIT=0。** 四份日志各有判据行：`### TEST_RESULT: PASS test-shim-bootmarks-x86`、`### TEST_RESULT: PASS test-shim-bootmarks-aarch64`、`RESULT: PASS (D-73 … 6/6 markers)`、`### TEST_RESULT: PASS test-kernel-image-riscv64-bootface`。这是“判据 PASS”，不是“退出码 0”的原始记录。若关账要求字面 `EXIT=0`，应重跑四条命令并追加 `; echo EXIT=$?`；否则 loop §5 应写明“以日志判据行为准，未单独记录退出码”。
2. **`29 个因正文变化` 措辞不准。** 以 R2 终态 `cbc105c7…` 为参照，实际正文变化的是 23 个节；另外 5 个父章因级联解冻，4.7 因锚点引用变化解冻。覆盖结果不受影响（29/29 有证据），但 loop 的因果描述应改为“29 个解冻单元（23 个正文变化节 + 5 个父章 + 4.7 锚点级联）”。
3. **tmp 面板与入库面板不是同一份字节。** `r3/panel/round3/` 的六份报告带 `DOC` 头，INDEX 与事件 `report_sha` 也按入库副本计算；`tmp/review-eval/.../panel/round3/` 的 `design-d2.md`、`fact.md`、`reader-a.md`、`reader-b.md`、`reader-c.md` 在入库后又改过（例如 design-d2 把“五个长度”改成“六个长度”）。FINDING 数量与 ID 未变，不影响本轮关闭结论；但“当前面板原文”应以哪一份为准需要写清楚，否则 INDEX 的哈希与 tmp 报告对不上。
4. **台账是逐发现行，不是逐根因行。** R3-02 出现在 3 行、R3-04 出现在 3 行、R3-07 出现在 2 行；`review-evals.md` §4.0c 要求“一行一个根因”，同一根因的多个发现应合并或用 `dup-of` 标注。当前 `唯一根因数 20` 可数出，但行级形态与规则不一致。若保留逐发现行，请在 ledger 头注明“本试点按发现行记账，根因列仅作归因”。
5. **确认阶段前置状态。** `stats` 仍显示 `t3=FAIL`、`units_review_lt2=38`、`units_post_freeze_0=48`；这是确认轮尚未开始、29 个单元刚经历一次 R3 review 的正常状态，不是本轮 `ROUND-CLOSED` 的阻断项。`loop.md §8` 已说明确认轮会写 `post_freeze_rounds`。
6. **`轮型` 标签。** `metrics.md:13` 写 `轮型: full`，`loop.md:5` 写“full（结构轮；第 3 个修改轮）”。按 `review-evals.md` §4.0b，`structural` 是“不重开面板”的轮种；R3 重开了六角色面板，因此 `full` 是更准确的轮型。当前只是术语混用，不影响关账；若按 `structural` 记录，则应说明它为何重开了面板。

## 5. 已重放命令与读数

```bash
sha256sum rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
wc -l rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
rg -n '^(schema:|修复后 sha256:|修复后行数:|### §)' \
  rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3/loop.md
rg -n '^- ' \
  rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3/metrics.md
python3 tools/review-units.py stats rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
python3 tools/review-units.py event verify rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --fresh
python3 tools/review-units.py audit --doc rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md \
  --ledger rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3/defect-ledger.md
python3 tools/review-units.py check --doc rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
python3 tools/v312-closure-check.py --round 3 \
  --run-dir rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3 \
  rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
python3 tools/panel-index.py --check \
  rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3/panel/round3
tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
tools/doc-code-map.sh rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --check
python3 tools/doc-publication-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
python3 tools/tutorial-rubric-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
```

关键读数：

- 正文 `67440cd8…`、3085 行；loop 的修复后读数一致。
- loop 九节齐全，schema 为 `V3.12 from R3`。
- `stats`：`frozen=48/48`、`t1_pending=0 t2_pending=0 reopened=0 stale=0`、`t3=FAIL`（预期，待本报告）。
- `event verify --fresh`：`events=37 completed=37 fresh_only=1`、`event verify: OK`、退出 0。
- `audit`：全部单元 `open=0`。
- `v312-closure-check.py --round 3 --run-dir …`：`v312-closure-check: PASS`、退出 0。
- 裸跑 `python3 tools/v312-closure-check.py --round 3 rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`：默认路径没有版本子目录，输出 13 项 FAIL、退出 1；这是工具默认路径问题，不是本 run 的读数。
- `panel-index.py --check`：`panel-index: 6 个文件哈希与计数一致`。
- `anchor-resolve`：`anchors=87 resolved=87 zero-def=0 multi-def=0`。
- `doc-code-map`：49 个文件，存在性 49/49。
- `doc-publication-check`：`无新增违规`；`tutorial-rubric-check`：`检查 6 章，0 章缺项`。
- 面板计数：22 个主 FINDING；2 个附带发现；台账覆盖 22+2 个 ID，无缺。
- 独立复验：doc 首两行 `RECHECK: PASS` / `VERDICT: PASS`；code `VERDICT: PASS` 与第 108 行 `RECHECK: PASS`。
- QEMU 四门：四份日志均有 PASS 判据行；没有字面 `EXIT=0`。
- 提交：`a6a94b942` 的正文是 `cbc105c7…`（2953 行）；`98b7cdaf…`（3049 行）在 `9f1327187`；`2914a7e18` 仍是 `98b7cdaf…`。

## 6. 结论

R3 的正文内容没有在本轮复核中发现新的阻断缺陷；机械门与独立复验的结论是正向的。失败的是轨迹账目本身：`①25` 与 `①28` 互相矛盾，`22+3+3` 漏掉两条附带发现，三条复验串无法重放，`certify/` 回执缺失，且所有 R3 事件与冻结记录的 `target_commit` 都不包含它们声称的 `doc_sha`。

因此本报告的判定是 `VERDICT: FAIL`。完成第 3 节的 B1–B4 后，按第 5 节的命令重跑；`v312-closure-check` 仍会 PASS，但那只是必要条件，B1 的提交锚点与 B3 的复验串必须同时修好，R3 才有资格进入确认轮。
