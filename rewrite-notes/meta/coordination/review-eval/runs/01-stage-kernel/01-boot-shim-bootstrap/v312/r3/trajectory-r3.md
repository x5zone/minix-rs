TRAJECTORY-REVIEW R3
DOC 01-boot-shim-bootstrap.md
VERDICT: PASS

RECHECK: PASS

# R3 轨迹复核（第 3 个修改轮）— RECHECK

- 复核方式：fresh subagent，只读；除本报告外未修改仓库文件。
- 首轮结论为 FAIL；B1–B4 修复后逐项复核，当前工作树通过。
- 非阻断残留见第 4 节；提交前需把 48 份 unit-evidence 入库，否则 `frozen_at=ea18db20e` 的证据链在干净 checkout 上不完整。

## 1. 复核快照

| 项 | 实测 | 命令 |
|---|---|---|
| 当前正文 | `67440cd885aad0e808cd5d76905323872b08b1c35849566b976c123d84dff6e8`，3085 行 | `sha256sum …`、`wc -l …` |
| HEAD `20496779b` 的正文 | `67440cd8…`，3085 行 | `git show HEAD:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` |
| R3 输入 commit `9f1327187` | `98b7cdaf…`，3049 行 | `git show 9f1327187:…` |
| R2 终态 commit `a6a94b942` | `cbc105c7…`，2953 行 | `git show a6a94b942:…` |
| 修复批 commit `ea18db20e` | `67440cd8…`，3085 行 | `git show ea18db20e:…` |
| 事务状态 | `round=3`、`frozen=48/48`、`t1/t2/reopened/stale=0`、`t3=FAIL`（待本报告） | `review-units.py stats` |
| 工作区 | 29 份 unit-evidence 为 `M`、19 份为 `??`；本报告更新后为 `M` | `git status --short` |

## 2. B1–B4 复核

| 项 | 判定 | 复核实证 |
|---|---|---|
| B1 提交锚点 | PASS | `98b7cdaf…` 在 `9f1327187`；`67440cd8…` 在 `ea18db20e`；六个 R3 面板事件 `target_commit=9f1327187…`、`doc_sha=98b7cdaf…`，逐事件比对 commit 内正文 sha 全部 OK；R3 full 事件 `target_commit=ea18db20e…`、`doc_sha=67440cd8…`，OK；state `full_rounds[n=3].commit=9f1327187…`；48 个单元 `frozen_at=ea18db20e…`；loop/ledger 的基线 commit 均为 `9f1327187` |
| B2 数字 | PASS | ledger 头写 `① 28 行` 并给出“30 个发现 ID（22 主 + 2 附带 + 3 P-REFUTE + 3 NEW），两组合并 → 28 行”；metrics 写 `① 28` 且说明同一口径；loop §6 写“30 个发现 ID … 合并为 28 行台账”；`唯一根因数=20` 与 ledger 的 20 个不同根因一致 |
| B3 复验串 | PASS | A-04 的 `PM 是进程管理器|RS 是重生服务器|IS 是信息服务`、A-08 的 `有启动门链的四条腿`、C-A02 的 `两个直核腿的六类载荷|两个直核腿各拥有一份栈上的` 均已命中当前正文；A-08 实测 1 行，C-A02 实测 2 行，A-04 三项在同一句（见第 4 节计数注解） |
| B4 certify | PASS | `certify/round3-doc-verify.md` 与 `certify/round3-code-verify.md` 已入库；与 tmp 原件 sha256 分别一致（`da58efcd…`、`21c59ab0…`）；loop §5 已改为指向这两个路径 |

## 3. 八项检查更新

| # | 检查项 | 判定 | 关键证据 |
|---|---|---|---|
| 1 | loop 九节、schema、sha/行数、§7 | PASS | §0–§8 齐全；`schema: V3.12 from R3`；修复后 sha/行数等于当前正文；§7 写 `ROUND-CLOSED` 且不写 `CONVERGED` |
| 2 | metrics 16 键、22/20/28/①28 | PASS | metrics 16 个必需键全在；`发现总数 22`、`唯一根因数 20`、`本轮新准入 28`、`处置 ① 28`；ledger 28 行全 ①，头注 30 IDs/28 行；`本轮新准入` 已在 metrics 说明中明确为台账行数，不是新根因数 |
| 3 | ledger 覆盖全部面板 FINDING；六条 CLAIM；open=0 | PASS | 22 个主 FINDING + 2 个附带发现全部有行；P-REFUTE 六条 CLAIM 均有结果；`audit` 全部单元 `open=0` |
| 4 | 48/48 冻结；四计数为零；事件新鲜 | PASS | `frozen=48/48`、`t1_pending=0 t2_pending=0 reopened=0 stale=0`；`event verify --fresh` 输出 `events=37 completed=37 fresh_only=1`、`event verify: OK`、退出 0；六个面板事件在输入 sha 上，已被 fix-verify full 事件覆盖 |
| 5 | 解冻单元有 R3 pass/freeze 与 unit-evidence；其余保持冻结 | PASS | 48 个单元全部 `frozen_at=ea18db20e` 且 `review_rounds` 含 R3；48 份 unit-evidence 首行均为 `UNIT <id>` 且含 PASS；29 个解冻单元的组成经独立比对为 23 个正文变化节 + 5 个父章 + 4.7 锚点级联；其余 19 个也已重新冻结到 `ea18db20e` |
| 6 | 两份独立复验 PASS/RECHECK；QEMU 四门 | PASS | certify 两文件首部分别为 `RECHECK: PASS`/`VERDICT: PASS` 与 `VERDICT: PASS`（第 108 行 `RECHECK: PASS`）；四份 QEMU 日志判据行全 PASS；loop §5 记“命令退出码另行记录为 0” |
| 7 | `v312-closure-check --round 3` | PASS | `python3 tools/v312-closure-check.py --round 3 --run-dir …` 输出 `v312-closure-check: PASS`、退出 0 |
| 8 | 形式通过、数字、未闭环伪装 | PASS（带非阻断残留） | 首轮 B1–B4 的阻断项均已消除；剩余为单位证据入库、计数注解与摘要口径，见第 4 节，不构成新的数字矛盾或未闭环伪装 |

## 4. 非阻断残留

1. **unit-evidence 尚未全部入库。** `git status` 显示 29 份 unit-evidence 为 `M`（补了 `FIX: commit ea18db20e` 行），19 份为 `??`；state 的 `evidence` 指向它们，`frozen_at=ea18db20e` 已经落盘，但干净 checkout 只会看到 ea18db20e 提交里的旧 29 份、缺 19 份。提交前把这 48 份与本报告一起入库，冻结证据链才可复现。这是当前唯一的“提交前必做”项。
2. **A-04 的“期望 3”按 `rg -n` 是 1 行。** `PM/RS/IS` 三项在同一句（L57），`rg -n` 只输出 1 行；按出现次数则是 3。内容命中已修好。若要让“期望 3”字面可复算，把命令改成 `rg -o 'PM 是进程管理器|RS 是重生服务器|IS 是信息服务' … | wc -l`（实测 3），或把期望改成“1 行含三项”。同理 A-04 第一行的“各 1”应按出现次数读（`rg -o` 实测各 1）。
3. **`fix-verify.md` 的覆盖摘要仍按 22 + 3 + 3 计数。** 它写“面板 22 个 FINDING、P-REFUTE 1 个 P2 观察与 2 条非阻断观察全部 ① 关闭”，没有点出 READER-C 的 2 条附带发现；ledger/metrics/loop §6 已是 30 IDs/28 行口径。建议把该摘要改成同一口径，避免读者把 22+3+3 当成全部准入。
4. **QEMU 退出码是叙述记录，不是原始日志行。** loop §5 已写“命令退出码另行记录为 0”，四份日志有判据行但没有字面 `EXIT=0`；若门要求 L1 原始证据，补一份带 `EXIT=0` 的 sidecar 即可。当前按“判据 PASS + loop 记录”接受。
5. **29 与 48 的措辞需要一句过渡。** loop 说“29 个单元解冻”，state 现在 48 个单元都有 R3 review 并冻结在 `ea18db20e`；29 的组成正确，19 个未解冻单元也在新 commit 重新冻结。建议在 loop §0/§5 补一句“19 个未解冻单元同样在 ea18db20e 重新冻结”，否则 29 与 48 份证据并读会显得少算。
6. **`fix-verify.md` 仍引用 tmp 路径。** 它写文档/代码复验在 `tmp/review-loop/01-stage/…`，loop §5 已指向 `certify/round3-*.md`；两处内容相同，但正式账应引用入库回执。改成 certify 路径即可。
7. **tmp 面板与入库面板仍有字节差。** 入库副本的 `report_sha` 与 INDEX 一致，事件绑定的是入库副本；tmp 只作工作副本。这一点已在事件层解决，不需要再改，只要别再以 tmp 副本作为复算基准。
8. **预期中的确认轮前置状态。** `t3=FAIL`、`units_review_lt2=29`、`units_post_freeze_0=48` 是确认轮尚未开始、29 个单元只有一次 R3 review 的正常读数，不是本轮 `ROUND-CLOSED` 的阻断项。

## 5. 重放命令与读数

```bash
sha256sum rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
wc -l rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
git show 9f1327187:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md | sha256sum
git show ea18db20e:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md | sha256sum
git show a6a94b942:rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md | sha256sum
python3 tools/v312-closure-check.py --round 3 \
  --run-dir rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3 \
  rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
python3 tools/review-units.py event verify rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --fresh
python3 tools/review-units.py stats rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
python3 tools/review-units.py audit --doc rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md \
  --ledger rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3/defect-ledger.md
python3 tools/review-units.py check --doc rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
python3 tools/panel-index.py --check \
  rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3/panel/round3
rg -n 'PM 是进程管理器|RS 是重生服务器|IS 是信息服务' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
rg -n '有启动门链的四条腿' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
rg -n '两个直核腿的六类载荷|两个直核腿各拥有一份栈上的' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
```

关键读数：

- 正文与 HEAD 均为 `67440cd8…`/3085；`9f1327187` 为 `98b7cdaf…`/3049；`a6a94b942` 为 `cbc105c7…`/2953。
- `v312-closure-check`：`PASS`，退出 0。
- `stats`：`frozen=48/48`、`t1_pending=0 t2_pending=0 reopened=0 stale=0`、`t3=FAIL`（待本报告）；`units_review_lt2=29`、`units_post_freeze_0=48`（确认轮前置）。
- `event verify --fresh`：`events=37 completed=37 fresh_only=1`、`OK`、退出 0。
- `audit`：全部单元 `open=0`。
- `panel-index --check`：6 个文件哈希与计数一致。
- 六个面板事件的 commit 内正文 sha 逐条为 `98b7cdaf…`，与 `doc_sha` 一致；R3 full 事件的 commit 内正文 sha 为 `67440cd8…`，与 `doc_sha` 一致。
- state：`full_rounds[n=3].commit=9f1327187…`；48 个单元 `frozen_at=ea18db20e…`；48 个单元 `review_rounds` 含 R3；48 份 unit-evidence 全部有 `UNIT <id>` 与 PASS。
- QEMU 四门判据行：`PASS test-shim-bootmarks-x86`、`PASS test-shim-bootmarks-aarch64`、`PASS`（U-Boot 6/6 markers）、`PASS test-kernel-image-riscv64-bootface`；退出码按 loop §5 记录为 0。
- certify 两份文件与 tmp 原件 sha256 一致：`da58efcd…`（doc）、`21c59ab0…`（code）。

## 6. 结论

B1–B4 已按最小修复落地，首轮阻断项全部消除；本轮复核判定为 `RECHECK: PASS`。提交前只需把 48 份 unit-evidence 与本报告入库，并按第 4 节第 2、3、5 条做三处文字/计数注解修正；这些不改变任何文档事实、单元状态或事件结论。

本报告只更新 `trajectory-r3.md`；未修改正文、state 与 events。
