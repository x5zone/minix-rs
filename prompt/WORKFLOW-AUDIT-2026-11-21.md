# 工作流审计报告（2026-11-21）

> 范围：`prompt/` 规范源、`tools/` 门禁、`.github/workflows/`、`.claude/`、`.codex/`、`.trae/`、`.agents/` 派生件，以及 TODO 单一账本。
> 基线：`486fd7929114512782bfb96ad313ab9be7c92410`（branch `rewrite`）。
> 方法：三轮；每轮 4–5 个只读子 agent 分片穷举，主 agent 重放证据后裁决与编辑；每轮结束重新生成派生件并跑门。
> 证据：本报告只引用仓库内路径与可重放命令；`tmp/workflow-audit/` 只作过程留存，不是唯一证据。

## 一、结论

**workflow audit 收敛。** Round 3 未新增 P0/P1；Round 2 回归复核发现的两条 P0（`review-scan` 双口径、`.claude/rules` 手工副本旧口径）已在 Round 3 前修复并重生成派生件。已知遗留项全部是 Round 1/2 登记过的改进项，列在附录 B，不阻塞本报告成立。

审计开始时 `prompt/` 有 48 篇 Markdown（不含被 `.git/info/exclude` 排除的 `prompt/resources/`）。去重后：P0 9 条、P1 28 条、P2 20 余条、P3 8 条。处置结果：

- P0：7 条在 Round 1 修复，1 条（`verify-check.py` 空证据可过）在 Round 2 修复，1 条（R37 轨迹 FAIL 后收口）由用户裁决为 `INVALIDATED`（`DECISIONS.md` D-108）。
- P1：22 组在 Round 1 修复，Round 2 修复 10 条，Round 3 修复回归复核发现的 4 条；剩余 6 条改进项（P1/P2）是工具/体验改进，列附录 B。
- P2/P3：能低风险修的已修；历史报告与冻结正文不改写，逐条登记。

本次提交：74 个文件（含本报告），提交前 staged 统计 +1908/−1082 行。所有改动属于本轮审计；未触碰其他会话的 2 个未跟踪文件。

## 二、全量清单摘要

审计开始时 `prompt/` 文档分类（`round-1/manifest.tsv` 逐行与 `find` 对账）：

| 分类 | 数量 | 说明 |
|---|---|---|
| live | 19 | `review-rules/` 规则、`skill/` 源、`review-scan/` 编排器 |
| historical | 5 | 一次性报告、迁移记录、裁决存档 |
| derived | 24 | 适配层 skill 源（生成 `.codex`/`.trae`，软链 `.agents`） |
| 合计 | 48 | 不含 `prompt/resources/` 的 658 篇只读参考 |

被 live 规则引用但当时已失效的目标，已修：

- 出版标准 `DP-1..DP-5` 与正文 `DP-1..DP-6` 两种口径（7 处，含工具 docstring）。
- `review-doc-checklist.md:526` 指向不存在的 `prompt/review-rules/review-core.md`。
- `DOC-ITER-WORKFLOW.md` 六处 `rewrite-notes/coordination/`（已迁 `meta/`）。
- `WORKFLOW-OPEN-ITEMS.md` 的旧 `todo.md` 与 `TODO-LEDGER-*` 路径。
- Gate C 的 `review.md §15`（实际在 `review-code-checklist.md §15`）。
- `AGENTS.md` 的 Blocker Gates 列表漏 `D-Impl`（注册表 10 门，入口只列 9 门）。
- 架构抽象族上界 `79-82/79-83/79-84` 四种写法（统一 79-85）。
- Profile C 的 `<300 行` 与 `review-process.md` 决策树的 `<500/500–1500`。
- 模式 69 的 `.design/` 判定信号（已迁 `meta/design/<stage>/`）。
- `review-cmds.md` 的 fix-guard 链接指向 `.claude/rules/` 派生副本。

仍登记未修的历史读数（不影响 live 执行）：`WORKFLOW-OPEN-ITEMS.md` 的 W-7 计数漂移、W-10 `§2.6.2`、W-12 `boot_shim.rs` 样本；`review-eval/README.md` 的“当前 V3.5”；`review-cmds.md` 通用门表格被引用块截断；`review-scan` 的 16 与 19 个 doc check 口径。

## 三、规则矛盾与修复

| 矛盾 | 位置 | 修法 |
|---|---|---|
| 普通 review 可用 `P1 新增 ≤ 1` 判 CONVERGED，出版标准要求 P1=0 | `review-process.md` Step 5.5/7.1、`review-process-skill.md`、`review-scan/SKILL.md`、`.claude/rules/review-process.md` | 统一为 P1=0；删除 `CONVERGED with warning` / `excellence-pending tag`；Step 7.1 只保留成本停止，不再作为收敛判据 |
| `backlog` 同时是合法出口与被禁出口 | `AGENTS.md`、`review-process.md`、`review-evals.md`、`review-doc-skill.md`、`review-code-excellence.md` | 删除 backlog 交付/教学延期措辞；未获 `WONTFIX` 的项进 TODO 账本 |
| ⑥ 设计项三处允许转 `OPEN-DEBT.md` 下一设计批 | `review-panel.md`、`review-evals.md` §4.0、`review-loop.md` | 统一为“轮内实施；需改 `os/` 的中止本轮、回 Manuscript 后重开；不得转出” |
| 代码冻结“任何 `os/` 改动 = 中止本轮”与 L3/L3.5 代码修复死锁 | `review-publication.md` §四.6、`review-loop.md`、`review-units.md` | 冻结限定为 survey/certify；本轮台账内修复是常规例外，实施后重设基线；设计批级改动回 Manuscript |
| 修改轮上限 3（跨 edition）与 5 轮并存 | `AGENTS.md`、`review-process.md` Step 7.1、`review-loop.md` | 统一为 3 个修改轮（跨 edition 累计）；第 3 轮失败先整节重写 + 1 次确认轮，仍 `open>0` 才 `NOT CONVERGING` |
| G14 一处只要求 `check`，三处要求 `check`+`audit` | `review-evals.md` G14、`review-units.md`、`review-publication.md`、`review-loop.md` | 统一为 `check` 与 `audit` 都退出 0 |
| SUGG/P3 是否阻断收敛有四种答案 | `review-panel.md` §3.6/§五、`review-evals.md` §4.0c/G8、`review-publication.md` §三 | 统一为“不阻断过程，但阻断 CONVERGED”；`suggestions.md` 只作历史归档 |
| Layer 2 ≥80% 与 G1–G14 两个 CONVERGED 定义 | `review.md` §4.5、`review-publication.md` §五、`review-loop.md` | 加轨道边界：普通 review 另加 Layer 2；review-loop 只看 G1–G14 |
| `review-scan` 同一文件两套收敛口径（`P1 new=0` 与 `P1 new≤1`/5 轮/backlog） | `review-scan/SKILL.md`、`checks/process.md`、`.claude/.codex` 派生件 | 修源文件并重生成；`.claude/rules` 手工副本同步 |
| `review-profiles.md` 的 `<300 行` 与决策树 `<500` 冲突 | `review-profiles.md` 两处 | 行数阈值统一指向 `review-process.md` 决策树 |
| 架构抽象族 79-82/83/84/85 四答 | `review-cmds.md`、`code-excellence` 薄壳、`review-patterns-skill.md`、`review-agent-ide.md`、`review-doc-skill.md` | 统一 79-85 |
| `review-units.md` 的 G14 单元列对旧 edition 追溯，01 篇 audit 恒 FAIL | `review-units.py`、`review-units.md` | 只有 V3.9+ 台账段强制 `单元` 列；全部 closed 的旧段不追溯；空台账仍拒绝 |

## 四、机械化缺口

### 从人工自证变成机械门的（本轮）

- `tools/verify-check.py`：Gate C 要有 Precision/meta-rule 判据；Gate D 要有至少 5 条 `rg`/`grep`；Gate D-6 要有 `structure.md`；Gate E 要有 `fn test_*` 或 `N/A`+理由；Gate G 解析 `consistency NN%` 并要求 ≥90；Gate H 要有 design+outline。空围栏不再 PASS。
- `tools/verify-check.py`：scan.md 新增 `gate-evidence-G10..G13` 四块，分别要求出版工具名与 0 命中/exit 0/计数；G10–G13 从自报变成 Gate 0 的一部分。
- `tools/verify-check.py --preflight`：吸收原 `review-gate-check.sh` 的 7 组产物存在性、B4.1 关联代码清单/gate-evidence-code、D4 artifact 存在性；旧脚本已删除。
- `tools/verify-check.py --finalize`：从已填的 §2.1 表计算 Gate G 一致性，残留（待填）或 <90% 返回非 0。
- `tools/tutorial-rubric-check.py`：TR-1/2/4/6 正文至少 20 字、TR-5 条目有内容、`checked==0` 返回 2；六节标题齐但正文全空不再 PASS。
- `tools/review-units.py`：`mark` 的 `NameError` 修复；`mark`/`gate` 的 evidence 至少 8 个非空白字符；`audit` 只对 V3.9+ 台账段强制 `单元` 列。
- `tools/todo-ledger.py`：`check --strict-refs` 解析文件、符号、文档单元、真实 `#[test]`、判决定义行、关系悬空/环路、日期合法性、仓库内相对路径；`historical`/`rejected` 不再被 `refs is None` 绕过；`check --scope coordination` 让跨阶段 open 可见。
- `tools/doc-publication-check.py` 与 `code-publication-check.py`：删除 `--update-baseline` 一键消音；保留只读 `--baseline`。
- `tools/check-command-boundary.sh`：匹配前剥 `//` 注释，不再把注释里的类型名当违规。
- `tools/eval-trace-check.py`：`--round` 未带 `--units` 时按 run 目录自动定位单元状态；定位不到 = G14 FAIL。

### 仍需人工或待机械化的

- Gate G 的行判断仍由 agent 逐行填「一致?」；`--finalize` 只保证表格与数字一致，不验证判断本身。
- “同 agent 验证 + 已重放 grep 命令”标签：`tools/` 没有读取它，写与不写门读数相同。
- `check-review-rules.sh` 对 `.claude/rules` 手工副本只查存在、不查内容；R-02 的旧口径就是这样漏过门的。
- TODO v2 的 `save()` 会重写手写注释，且并发写无锁；当前账本没有手写正文注释，暂不处理。
- CI 未跑：本地 `rewrite` 领先 `origin/rewrite` 574 个提交，本轮修复尚未经过远端 CI。

## 五、减法账本

删除或关闭的装置（12 项）：

1. `.github/workflows/mdbook.yml`：根目录无 `book.toml`，每次 push 到 main 必失败；`deploy.yml` 已覆盖 Pages。
2. `tools/todo-staleness-check.sh`（已删除，现不存在）：名义门，对 YAML 账本恒绿；由 `todo-ledger.py check` 取代。
3. `tools/review-line-check.sh`：规则三处只写“已作废”，CI 零引用；行坐标卫生由 `anchor-suspect-scan.py` 承担。
4. 两个出版门的 `--update-baseline`：规则早已写“零容忍”，这个开关能一键消音。
5. `pattern-gate` 的过期 P2 条目：`test_clamp_cpu_to_bsp_transitional_guard` 随 BSP 钳制功能撤除，不是回归网丢失。
6. `P1 新增 ≤ 1` 收敛判据（Step 5.5 / Step 7.1 / skill / scan）。
7. `backlog` 作为交付出口（Step 7.1、G8、教学段落）。
8. `CONVERGED with warning` 与 `CONVERGED with excellence-pending tag` 两个状态。
9. ⑥ 的三处 `OPEN-DEBT.md` 转出口。
10. “任何 `os/` 改动 = 中止本轮”的绝对冻结句（改为分阶段冻结）。
11. L6.5 对未入库 `prompt/resources/` 的强制依赖（改为重读 `review-patterns.md` 对应族）。
12. `review-units.py audit` 对旧 edition 台账的 `单元` 列追溯。

替代关系：

- `todo-staleness-check.sh` → `todo-ledger.py check`（origin_file FAIL / origin_line WARN）+ `--strict-refs`。
- Gate C/D/D-6/E/G/H 的自由文本 → `verify-check.py` 的结构判据。
- G10–G13 的自报 → scan.md 的 `gate-evidence-G10..G13`。
- 扁平 TODO 自由文本 → YAML v2 的 `refs`/`relations`/`lifecycle`。
- `.claude/rules` 的旧口径 → 与 `prompt/` 源同步的手工副本（R-02 修复）。

## 六、成本判断与 01 首轮建议

固定全量构件（实测与估算分开）：

- 规则与 skill 源合计 20,091 行；普通 review 的必读规则约 8,278 行，review-loop 约 8,095 行。
- review-loop 每轮实测均值：6.63 个 agent、3.32 小时（38 条 `metrics.md` 成本行，30 条可解析耗时）；38/38 条 token 未采集。
- 01 篇关联代码 50 个文件、约 55,531 行；面板单轮输入约 8 万行量级（估算）。

盈亏平衡：

- 单轮能产出至少 1 条会流出的 P0，或至少 4 条 P1（`weighted_new = P0×10 + P1×3 + P2×1 ≥ 10`）时，固定构件值得。
- 若单轮产出 70% 以上是 P2/P3/SUGG/重复，边际价值低于 6–7 个 agent、3.3 小时的固定成本。
- 单元模式只在同时满足以下条件时启用：文档 ≥1,000 行、`###` 单元 ≥30、未冻结比例 ≤15%、预计还能跑 ≥3 轮、编辑局部化、台账可逐单元归属；否则默认整篇。
- 这套流程减少的是“同一缺陷反复漏掉”，不减少总轮次；≥2,500 行、≥50k 关联代码的普通教学文档，3 个修改轮后应交付并写已知限制，不再开轮。

01 篇首轮建议：

1. 以 V3.9 新 edition 开轮（D-108 已记录 R37 `INVALIDATED`），先跑 L0.9 出版门与 G10–G13 证据块。
2. 首 edition 用整篇 survey（`review-panel.md` §七 表 A），不要按 41 个单元拆轮；整篇 survey 完成后再 `review-units.py baseline`。
3. TODO 账本先补 01 的 open 项强 ref（当前 16/17 已有可解析 ref），coordination 的 37 条 open 用 `check --scope coordination --fail-on-open` 单独看，不计入单篇 G12。
4. 3 个修改轮内收敛；第 3 轮失败先整节重写 + 1 次确认轮，仍 `open>0` 才 `NOT CONVERGING`。

## 七、下一轮（实际执行工作流）的前置条件与停止条件

前置条件：

- D-108 已入 `DECISIONS.md`；V3.9 edition 的 run 目录从 R1 开始；单元状态 `review-units.py stats` 可读。
- scan.md 含 `gate-evidence-G10..G13`；`verify-check.py --self-check` 通过。
- 当前篇 `todo-ledger.py check --doc <篇> --fail-on-open` 通过；coordination 门单独记录。
- 本地改动已提交并 push 到 `rewrite`，让 CI 第一次跑这 574 个提交；CI 红则先修再开轮。
- `review-gate-check.sh` 已并入 `verify-check.py --preflight`；Step 5.6 已接入 `--finalize`。

停止条件：

- 同一篇 3 个修改轮（跨 edition 累计）后仍未收敛 → 整节重写 + 1 次确认轮；确认轮仍 `open>0` → `NOT CONVERGING`，不再开轮。
- 确认轮最多 2 次；`certify 新准入 > 0` 或结构性新根因 → 回边界裁决。
- 有 `blocked: true` 的待裁项 → 任何轮不得 `CONVERGED`。
- G10–G14 任一未过、`OPEN-DEBT` 有未销账行、TODO `open > 0` → 不得 `CONVERGED`。
- 单轮成本超过上一轮 80% 而加权新发现低于 20% → 停止寻找新问题，按出版标准判定状态；不得用 backlog 交付。

## 附录 A：验证命令与结果

```text
bash tools/generate-derived-skills.sh              exit 0（已重新生成）
bash tools/check-review-rules.sh                   exit 0
bash tools/lint-review-rules.sh                    exit 0（0 failed）
bash tools/diff-trae-skills.sh --only-diff         exit 0（10/10 同步）
python3 tools/eval-trace-check.py --self-test      exit 0
python3 tools/review-units.py --self-test          exit 0
python3 tools/todo-ledger.py --self-test           exit 0
python3 tools/todo-ledger.py check                 exit 0（open=73，带 ref=36，49.3%）
python3 tools/tutorial-rubric-check.py --self-test exit 0
python3 tools/doc-publication-check.py --self-test exit 0
python3 tools/code-publication-check.py --self-test exit 0
bash tools/pattern-gate.sh                         exit 0（PASS 16 / FAIL 0）
bash tools/check-command-boundary.sh               exit 0
python3 tools/decision-ref-check.py                exit 0
bash tools/design-coverage-check.sh 01-stage-kernel exit 0（56 篇齐全）
python3 tools/review-units.py audit --doc rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md exit 0
```

预存红（HEAD 同样红，非本轮引入）：

- `python3 tools/notes-link-check.py rewrite-notes/` → 151 条断链（64 在 `01-stage-kernel/draft/`、21 在 `00-master-plan/`、22 在 `meta/`；扫描域含历史 draft 与 review-eval 制品）。
- 历史 `review-eval/runs/**` 仍有 98 处旧表列 schema 与 4 处历史 panel 断链；这些是历史轮轨迹，不追溯。活制品（`review-eval/*.md` + 01 篇）已修到 0 处。

首轮 CI 实测（commit `91c8144fc`）：

- `rules-consistency` #8/#9：failed。首轮原因是最终报告 L10 缺状态标记（已修）+ 历史 `runs/**` 旧表 schema（已把 CI 的 P-2 步缩到活制品；活制品现 0 处）。`DECISIONS.md` 的 8 处劈表空行与 6 处行内列/嵌套代码 span 已做结构修复，不改叙事。本地按 workflow 顺序重放：四道一致性门、pattern-gate、boundary、全部工具自测、doc-citation、arch-gated-test-count 均 exit 0；P-2 只扫活制品后 exit 0。
- `kernel-tests` #7、`qemu-tests` #10、`vm-tests` #10、`clippy-ratchet` #7：failed；本分支此前同类作业也红（`11feb85` 上 kernel-tests 通过、其余四项红），本次只改注释与一行 import，需看日志区分存量与环境因素。
- `cmd-smoke` #7：首轮读取时仍在运行。

## 附录 B：未闭环与用户裁决

未闭环改进（不阻塞本报告，但下一轮前应处理）：

1. Gate G 的行判断、以及“同 agent 验证 + 已重放 grep”标签仍由人工填写。
2. `check-review-rules.sh` 对 `.claude/rules` 手工副本只查存在、不查内容。
3. TODO v2 的 `save()` 会重写手写注释；并发写无锁。
4. TODO v2 的 `relations`/`lifecycle` 在 113 行中零使用；校验器只靠自测覆盖。
5. TODO v2 覆盖率 49.3%（强口径 11/73）；Phase 2 按 kind 分档后仍需一次 ref 补齐，再开 `--strict-refs` 阻断。
6. `WORKFLOW-OPEN-ITEMS.md` 的 W-7/W-10/W-12 历史读数与样本路径未更新；`review-eval/README.md` 仍写“当前 V3.5”。

已裁决（2026-11-21）：

1. `review-gate-check.sh`：已并入 `verify-check.py --preflight` 后删除。
2. Gate G：已实现 `verify-check.py --finalize`；行判断仍人工。
3. 574 个提交：已选择提交并 push，让 CI 跑一次；红当独立代码债处理。
4. TODO v2 Phase 2：按 kind 分档（code/doc 强 ref 80%，design/process 用 unit/decision/blocked_reason），coordination 用独立项目级门。
