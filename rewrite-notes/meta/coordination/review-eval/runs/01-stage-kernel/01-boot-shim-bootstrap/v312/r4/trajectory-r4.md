TRAJECTORY-REVIEW R4
DOC 01-boot-shim-bootstrap.md
VERDICT: PASS

# R4 确认轮轨迹复核

- 复核方式：fresh subagent，只读；除本报告外未修改仓库文件。
- 复核对象：`v312/r4/`（`G18-COLD-READ.md`、`confirmation.md`、`unit-confirm/` 48 份）、`meta/review/state/01-stage-kernel/01-boot-shim-bootstrap.yaml`、`meta/review/events.csv`、`meta/review/scope/01-stage-kernel/01-boot-shim-bootstrap.yaml`，以及 R3 关账材料 `v312/r3/`。
- 快照：正文 `67440cd885aad0e808cd5d76905323872b08b1c35849566b976c123d84dff6e8`，3,085 行；HEAD `a5bf73249`（`chore(review): R3 单元证据补全与 G18 事件预登记`）。
- 总体判断：确认轮的证据链成立，可以判 R4 轨迹 PASS。`CONVERGED` 尚不能宣告，卡点不在本轮轨迹，而在 T3、R4 轨迹登记与 R4 包入库这三步（见第 6 节）。

## 1. 七项检查

| # | 检查项 | 判定 | 实测证据 |
|---|---|---|---|
| 1 | G18 首三行、48 个单元逐行“可跟读”、S1–S6 全“能完成” | PASS | `head -3` 为 `DOC 01-boot-shim-bootstrap.md` / `HARD-REJECTIONS: 0` / `VERDICT: READY`；`rg -n '^\| [0-9]' G18-COLD-READ.md \| wc -l` = 48；48 行无一行非“可跟读”；`sed -n '72,81p'` 的 S1–S6 六行全部“能完成” |
| 2 | 48 份 unit-confirm 每份只点名一个 UNIT、指向 G18 对应行；state 每单元含 R4；两计数为 0 | PASS | 48 个文件与 48 个单元一一对应；`UNIT` 与文件名不符 0 份；`EVIDENCE` 行号与 G18 表行号不符 0 份；48 个单元 `review_rounds` 与 `post_freeze_rounds` 均含 4；`stats` 输出 `units_review_lt2=0 units_post_freeze_0=0` |
| 3 | R4 的 student-cold-read 与 confirmation 事件 completed、覆盖 48 单元、`event verify --fresh` 退出 0 | PASS | 两个事件均为 `completed-pass`，`unit_ids` 各 48 个且无重复；`event verify --fresh` 输出 `events=39 completed=39 fresh_only=1` 与 `event verify: OK`，退出 0 |
| 4 | R1–R4 每轮都有 PASS 的 trajectory review | PASS | R1 `y01b-trajectory.md`、R2 `y03-r2-trajectory.md`、R3 `trajectory-r3.md` 均在 state 记 `verdict: PASS`，三份 `evidence_sha` 与文件实算一致；R4 由本报告背书 |
| 5 | `check`、`stats`、`v312-closure-check --round 3` 与 R4 状态自洽；T3 未 PASS 是 gate 前正常状态 | PASS | `check: OK`；`stats` 为 `round=4 full_rounds=3 trajectory_reviews=3 units_review_lt2=0 units_post_freeze_0=0 t3=FAIL`；R3 关账带 `--run-dir …/v312/r3` 输出 `v312-closure-check: PASS`；`t3.passed=false`、`t3.evidence=''` 与“gate 前”一致 |
| 6 | 是否存在“只有形式通过”：冷读真逐单元、确认报告是否只是摘要、`report_sha` 唯一、提交锚点含文档 sha | PASS（带非阻断残留） | 冷读报告 48 行逐单元，30 余处行级断言与正文逐条命中（见第 2 节）；确认报告是摘要，但逐单元实体在 G18 第二节与 48 份 unit-confirm 的 `EVIDENCE` 行；247 行事件中 completed 行的 `report_sha` 无重复；两个 R4 事件的 `target_commit` 内正文 sha 均为 `67440cd8…`。残留见第 6 节 |
| 7 | 学习范围表是否仍为 me-agent 代确认 | PASS（待作者追认） | `rg -n 'confirmed_by' scope.yaml` = `confirmed_by: me-agent-deputy`；`confirmed: true`、`confirmed_at: '2026-11-23T02:39:15'`。这是待作者追认项，不是轨迹失败项 |

## 2. G18 冷读的实质检查

冷读报告不是模板摘要。它的 48 行逐单元表之外，正文的判定说明与可选改进清单给出了可核对的正文行号；我抽查了 30 余处，全部与当前正文一致：

| G18 断言 | 正文实算 |
|---|---|
| §3.6 的 `black_box` 在 1846 行且未解释 | `1846: let entry = VirBytes(core::hint::black_box(report.kern_virt_base.0));` |
| §4.4 字段表 2441–2453 混入 `user_sp`/`kern_stack_top`/`syscall_entry` 三行已退场字段 | 三行均在表中，且逐行标“v10 起不再是字段” |
| §4.4.1 的 2513–2515 有两句几乎重复的引导句 | 2513 与 2515 确为两句同义引导 |
| §3.2.2 的 1258 是“先分清三套数字”段 | 原文给出 22 / 16→13→14→10 / 16→13 并点明交集 |
| §3.7 的 1935 是一整段所有权长句 | 原文为单段，含 U-Boot shim、UEFI、直核、`BootReport` 四块 |
| §4.5 的 2542–2563 节选里 `mstore`/`msrc` 未在节选内定义 | 原文只留 `// …六个切片依次 land_slice 到 .bss store…` |
| `EBS` 首次出现在 1788 与 2188 | 两行均出现 `ExitBootServices`/`EBS` 的首次使用 |
| S1 的 76、144–147 | 接缝声明与 `pre_init`/`head.S` 分工逐条命中 |
| S2 的 260–268、272–274、1047–1050、149、1049、1051 | `boot.cfg` 指令表、`load_mods` 次序段、D1–D5 定义与 D3/D5 归属逐条命中 |
| S3 的 1114–1116、1191–1193、1405–1411、1745–1749、1931–1933 | 三否一采、`KernelInfo` 结构、两张表与 `SourceRecord`、三条调用链、所有权段逐条命中 |
| S4 的 2168–2179、2500–2502、2538–2540 | 九动作时间拍表、腿间执行序、`land_and_store` 节选逐条命中 |
| S5 的 2742–2747、2769–2771 | 证据矩阵与真机门链表逐条命中 |
| S6 的 2837、2857–2858、3006–3012 | EBS 分界、里程碑表头、三层排查逐条命中 |

48 行逐单元表的判定只有“可跟读”与“有卡点”两档，无“有卡点”行。章单元（1–6）只含章首导语，与表头声明的口径一致；节单元的“回读一次以上”位置（§3.2.2 的 1258、§3.2.1 的 1217、§3.7 的 1935、§4.4 的 2441–2453、§4.5 的 2542–2563、§3.6 的 1846、§4.3 的 2407、EBS 首次出现）在正文中都能就地解决，不构成硬阻断，与 `HARD-REJECTIONS: 0` 一致。

有一处表述口径需要说明，但不影响逐单元判定：3.4 与 3.5 两行写 1413–1414、1619–1620，只覆盖容器标题，正文在 3.4.1–3.4.3 与 3.5.1–3.5.2；同类含 `####` 子节的 2.4、3.2、4.2、4.4 则把子节正文包含在范围内。3.4/3.5 两行的备注已明确写“正文在 3.4.1–3.4.3 / 3.5.1–3.5.2；不阻断”，冷读的学习路径也明确把这两节的内容列入了阅读任务，所以“可跟读”不是只看标题得出的；但范围列的横向口径不统一，见第 6 节残留 2。

## 3. 48 份 unit-confirm 的绑定

- `ls unit-confirm | wc -l` = 48；文件名集合与 state 的 48 个单元集合完全相同，无缺无多。
- 每份首行 `UNIT <id>` 恰好出现一次；`UNIT` 与文件名不符 0 份。
- 每份 `EVIDENCE:` 指向 `G18-COLD-READ.md:<line>`，48 份的行号与 G18 表的对应行号完全一致，0 份偏差。
- 每份末行的可重放命令 `rg -n -F '| <id> |' …/G18-COLD-READ.md` 逐条实跑：48 条全部只返回 1 行，且返回行号等于该份 `EVIDENCE` 行号；没有一条命令返回空或多行。
- 每份都含 `DOC 01-boot-shim-bootstrap.md`，账户绑定没有被别篇证据复用。

这些文件是账户绑定用的指针，不是第二份独立冷读；逐单元判断的实体在 G18 第二节。就“每单元留下 `UNIT <id>` 证据”的规则而言，48 份全部合格。

## 4. 事件与提交锚点

R4 两个事件在 `events.csv` 中的 completed 行：

| 事件 | review_id | 类型 | 状态 | 单元数 | target_commit | doc_sha | report_sha |
|---|---|---|---|---|---|---|---|
| 冷读 | `REV-20261123-389A201F` | `student-cold-read` | `completed-pass` | 48 | `3b8ac0c63317e6ef042b9e169cec3369810515f3` | `67440cd8…` | `63fdbb299013cb175d5ed6fdd8dd81306fefe63a4caee29bbd8b03f19ac93499` |
| 确认 | `REV-20261123-E794927E` | `confirmation` | `completed-pass` | 48 | `a5bf73249b34a05e37bebdaddd311523e5e651f6` | `67440cd8…` | `334d2419c4a5a0bfa21ffbb30b2dc2f81d0e0f6f59b35fd107dc1d809b6d9d2c` |

核对结果：

- 两份报告的实算 sha 与事件记录逐字节一致：`G18-COLD-READ.md` = `63fdbb29…`，`confirmation.md` = `334d2419…`。
- 两个 `target_commit` 都存在，且 `git show <commit>:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md | sha256sum` 均输出 `67440cd8…`；提交锚点确实包含被审文档的 sha。
- 全部 247 行事件中，completed 行的 `report_sha` 无重复；两个 R4 事件的 `report_sha` 也不与历史轮次重复。
- `event verify --fresh` 退出 0，R4 两个事件的 `unit_sha` 与当前 48 个单元的指纹一致（`fresh_only=1` 是该命令的 `--fresh` 标志读数，不是“只有 1 个新鲜事件”）。
- `confirmation` 事件是在确认轮 active 期间完成的；`end-confirmation` 的前置条件（每个单元 `len(review_rounds) >= 2`、`post_freeze_rounds` 含当前轮）在 state 中全部满足。

## 5. 历史轮次与状态自洽

- R1、R2、R3 的 `trajectory_reviews` 均为 PASS，三份证据文件存在且 `evidence_sha` 与实算一致；R1 文件首部为 `TRAJECTORY-REVIEW R1 PASS`，R2 为 `TRAJECTORY-REVIEW R2 PASS`，R3 为 `TRAJECTORY-REVIEW R3` + `VERDICT: PASS`。
- R1、R2、R3 的 `full_rounds` 证据文件存在且 sha 一致；R3 的 `fix-verify.md` 为 `VERDICT: PASS` + `RECHECK: PASS`。
- R3 关账：`v312-closure-check --round 3 --run-dir …/v312/r3` 输出 `v312-closure-check: PASS`，退出 0；`review-units.py audit --doc … --ledger …/r3/defect-ledger.md` 对 48 个单元全部 `open=0`，退出 0。
- `review-units.py check` 输出 `units=48 frozen=48 chapters=6`、`review-units check: OK`，退出 0；仅提示 `T3 尚未通过`，这是确认轮结束、gate 之前的预期读数。
- `review-units.py stats` 输出 `round=4 full_rounds=3 trajectory_reviews=3 units_review_lt2=0 units_post_freeze_0=0 t3=FAIL`，与 R4 状态自洽。
- `t3.passed=false` 且 `t3.evidence=''` 不是 R4 轨迹的缺陷：T3 是确认轮之后的出版/代码同步门，gate 前本就不应 PASS。

## 6. 非阻断残留与提交前必做

以下都不是“证据不存在”或“数字对不上”的硬阻断，按影响排序：

1. **`confirmation.md` 的第一条重放命令写成了双反斜杠。** 原文第 18 行的代码块内容是 `rg -n '^\\| [0-9]' G18-COLD-READ.md`（`^` 之后是两个反斜杠）。实跑该命令得 70 行，不是 48；正确写法是 `rg -n '^\| [0-9]' G18-COLD-READ.md`（48 行）。内容本身我已用正确命令穷举核对，48 行齐全；这是报告里的命令笔误，不是覆盖缺失。最小修复：把 `^\\|` 改为 `^\|`；若改，`confirmation.md` 的 sha 会变，`confirmation` 事件的 `report_sha` 需重新完成或补一条事件，建议与提交一起处理。
2. **G18 表 3.4/3.5 的行范围只到容器标题。** 3.4 写 1413–1414、3.5 写 1619–1620，正文在 3.4.1–3.4.3（1415–1618）与 3.5.1–3.5.2（1621–1738）；2.4、3.2、4.2、4.4 等含 `####` 子节的节则把子节正文包在范围内。两行的备注已说明正文位置，学习路径也覆盖了这两节内容，所以不影响“可跟读”结论；但范围列的横向口径不统一。最小修复：把这两行改成 1413–1618、1619–1738，或在表头写明“容器节只报标题行”。
3. **R4 包与状态尚未入库。** `git status` 显示 `?? rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r4/`、`M rewrite-notes/meta/review/state/01-stage-kernel/01-boot-shim-bootstrap.yaml`、`M rewrite-notes/meta/review/events.csv`。提交前必须把 G18、confirmation、48 份 unit-confirm、本报告、state 与 events 一并入库；否则干净 checkout 上 R4 的证据链与 `confirmation.completed=true` 都不可复现。这与 R3 轨迹复核中的“unit-evidence 入库”是同一类提交前项，不是当前轨迹失败。
4. **R4 轨迹尚未写进 state 的 `trajectory_reviews`。** 当前 state 只有 R1–R3 三条；本报告通过后，需用 `review-units.py trajectory-review`（或对应命令）把 R4 PASS 登记进去，gate 才看得到 R4 的轨迹。登记前 `stats` 显示 `trajectory_reviews=3` 是正常的。
5. **学习范围表是 me-agent 代确认。** `confirmed_by: me-agent-deputy`，不是作者本人；`scope_errors` 只检查 `confirmed: true`，所以工具门不拦。这是待作者追认项；在作者追认前，R4 可以作为流程上的确认轮通过，但不应把“作者已批准学习范围”当成既成事实写进对外结论。
6. **裸跑 `v312-closure-check --round 3` 会打到旧账户目录。** 不带 `--run-dir` 时工具默认解析到 `rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap`（V3.11 旧目录），会报 13 项 FAIL；带 `--run-dir …/v312/r3` 才是本次 R3 关账目录并 PASS。重放时必须带 `--run-dir`；这不是 R3 关账失败，是调用路径问题。
7. **T3 未 PASS。** `t3.passed=false` 是 gate 前的正常状态；它不阻塞“R4 确认轮通过”的轨迹结论，但阻塞 `CONVERGED`。

## 7. 重放命令与读数

```bash
# 快照
sha256sum rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
# 67440cd885aad0e808cd5d76905323872b08b1c35849566b976c123d84dff6e8
wc -l rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
# 3085

# G18 首三行、48 行逐单元、S1–S6
head -3 rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r4/G18-COLD-READ.md
# DOC 01-boot-shim-bootstrap.md
# HARD-REJECTIONS: 0
# VERDICT: READY
rg -n '^\| [0-9]' rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r4/G18-COLD-READ.md | wc -l
# 48
rg -n '^\| [0-9]' rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r4/G18-COLD-READ.md | rg -v '可跟读' || true
# 无输出
sed -n '72,81p' rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r4/G18-COLD-READ.md | rg -c '能完成'
# 6

# 48 份 unit-confirm 的数量与逐条绑定
ls rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r4/unit-confirm | wc -l
# 48
python3 -I - <<'PY'
import re, pathlib, subprocess, yaml
root = pathlib.Path('/home/xzhao/github/minix-rs')
r4 = root/'rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r4'
rep = (r4/'G18-COLD-READ.md').read_text().splitlines()
row_line = {}
for i, l in enumerate(rep, 1):
    m = re.match(r'^\| ([0-9]+(?:\.[0-9]+)?) \|', l)
    if m:
        row_line[m.group(1)] = i
bad = []
for f in sorted((r4/'unit-confirm').glob('*.md')):
    t = f.read_text().splitlines()
    unit = [l for l in t if l.startswith('UNIT ')][0].split()[1]
    ev = int([l for l in t if l.startswith('EVIDENCE:')][0].rsplit(':', 1)[1])
    cmd = t[-1]
    out = subprocess.run(cmd, shell=True, cwd=root, capture_output=True, text=True).stdout.splitlines()
    if unit != f.stem or ev != row_line.get(unit) or len(out) != 1 or int(out[0].split(':', 1)[0]) != ev:
        bad.append(f.name)
print('unit-confirm mismatches:', len(bad), bad)
st = yaml.safe_load((root/'rewrite-notes/meta/review/state/01-stage-kernel/01-boot-shim-bootstrap.yaml').read_text())
print('units:', len(st['units']),
      'no-R4-review:', [u['id'] for u in st['units'] if 4 not in u.get('review_rounds', [])],
      'no-R4-post-freeze:', [u['id'] for u in st['units'] if 4 not in u.get('post_freeze_rounds', [])],
      'lt2:', sum(1 for u in st['units'] if len(u.get('review_rounds', [])) < 2),
      'pf0:', sum(1 for u in st['units'] if not u.get('post_freeze_rounds')))
PY
# unit-confirm mismatches: 0 []
# units: 48 no-R4-review: [] no-R4-post-freeze: [] lt2: 0 pf0: 0

# 状态、事件、关账
python3 tools/review-units.py check --doc rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
# units=48 frozen=48 chapters=6
# [WARN] 全部章已冻结；T3 尚未通过
# review-units check: OK
python3 tools/review-units.py stats rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
# chapters=6 sections=42 frozen=48/48
# t1_pending=0 t2_pending=0 reopened=0 stale=0 t3=FAIL
# round=4 full_rounds=3 trajectory_reviews=3 units_review_lt2=0 units_post_freeze_0=0
python3 tools/review-units.py event verify rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --fresh
# events=39 completed=39 fresh_only=1
# event verify: OK
python3 tools/review-units.py audit --doc rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md \
  --ledger rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3/defect-ledger.md
# review-units audit: OK（全部单元 open=0）
python3 tools/v312-closure-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md \
  --round 3 --run-dir rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3
# v312-closure-check: PASS

# 报告 sha、事件 sha 与提交锚点
sha256sum rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r4/G18-COLD-READ.md \
          rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r4/confirmation.md
# 63fdbb29…  G18-COLD-READ.md
# 334d2419…  confirmation.md
git show 3b8ac0c63317e6ef042b9e169cec3369810515f3:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md | sha256sum
git show a5bf73249b34a05e37bebdaddd311523e5e651f6:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md | sha256sum
# 两行均为 67440cd8…
python3 -I - <<'PY'
import csv, collections
rows = list(csv.DictReader(open('rewrite-notes/meta/review/events.csv', encoding='utf-8')))
comp = [r for r in rows if r['status'].startswith('completed') and r['report_sha']]
dup = {k: v for k, v in collections.Counter(r['report_sha'] for r in comp).items() if v > 1}
print('rows:', len(rows), 'completed with sha:', len(comp), 'duplicate report_sha:', dup)
PY
# rows: 247 completed with sha: ... duplicate report_sha: {}

# 学习范围表
rg -n 'confirmed:|confirmed_by|confirmed_at' rewrite-notes/meta/review/scope/01-stage-kernel/01-boot-shim-bootstrap.yaml
# confirmed: true
# confirmed_by: me-agent-deputy
# confirmed_at: '2026-11-23T02:39:15'

# 工作区状态（R4 包未入库）
git status --porcelain | rg 'v312/r4|events.csv|01-boot-shim-bootstrap.yaml'
```

## 8. 结论

R4 确认轮的证据链成立：G18 冷读是逐单元的真实阅读，48 份 unit-confirm 全部正确绑定到对应行，两个 R4 事件 completed、覆盖 48 单元、指纹新鲜，报告 sha 唯一且提交锚点包含正文 sha，R1–R3 轨迹均为 PASS，R3 关账与 audit 均通过。可以判 `TRAJECTORY-REVIEW R4 PASS`。

`CONVERGED` 仍不能宣告，原因不在本轮轨迹，而在三个后续动作：

1. 把 R4 目录、本报告、state 与 events 一并入库（第 6 节残留 3）；
2. 把 R4 PASS 登记进 state 的 `trajectory_reviews`（第 6 节残留 4）；
3. 通过 T3 出版/代码同步门（`t3.passed=false` 是 gate 前正常状态）。

第 6 节残留 1（confirmation 重放命令的 `^\\|` 笔误）和第 2（3.4/3.5 范围列口径）不影响本轮 PASS；若在提交前顺手修掉，需同步更新 `confirmation.md` 的 `report_sha` 或补一条事件。
