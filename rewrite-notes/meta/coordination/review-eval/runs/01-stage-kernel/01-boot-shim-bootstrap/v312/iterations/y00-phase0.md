# y00 — Phase 0 基线（D01 V3.12 R1）

- 时间：2026-11-23
- 本轮不是 review 轮；目标是把执行现场、D01 账户、学习范围与 L0.9 预检准备好。
- 分支 `rewrite`；HEAD `64f76309fe3393f9884f797335e582412f18480f`。
- D01 基线：2,983 行；sha256 `82271968c81aae8630a70a886e80fb031546d79e80f84cf73df9a88c1842fe90`。
- 工作树：两个未跟踪文件（`01-design-learn.md`、`01-boot-shim-bootstrap-r26.md`）不纳入提交。

## 计划对抗审查

- 已启动独立 subagent，对 `PLAN.md` 与 `review-units.md`/`review-evals.md`/`review-loop.md`/`review-panel.md` 做一致性审查。
- 本简报落盘时审查尚未返回；返回后在本节追加结论，若发现 P0/P1 计划缺陷，先修计划再派面板。

## 运行时与技能

- Skill 工具实际调用：`review-loop`、`full-review`、`style-bible`。
- VARIANCE：ZCode 运行时没有 `review-process-skill`/`review-doc-skill` 细粒度技能；用 `full-review` 伞形技能 + 规则源文本过渡，面板子 agent 只拿 `review-panel.md` 第四节 brief。
- 工具自检：`review-units.py --self-test` PASS；`v312-closure-check.py --self-test` PASS；`eval-trace-check.py --self-test` PASS。
- 一致性门：`generate-derived-skills.sh --check` PASS；`check-review-rules.sh` PASS；`lint-review-rules.sh` 0 失败。

## D01 账户与 run 目录

- V3.12 run 目录：`runs/01-stage-kernel/01-boot-shim-bootstrap/v312/`；V3.11 父目录原样保留。
- `review-units.py init --force --edition V3.12`：48 单元、0 冻结、round=0；随后 `baseline --evidence r1-v312/full-r1.md` 记录 R1（round=1，full_rounds=1）。
- 旧 V3.12 未完成 R1 的 `baseline.md`/`reader-a-r1.md` 存 `v312/pre-r1/`，不作本轮证据。
- 旧 `.review/zcode/01-stage-kernel/STATE.md` 预检 FAIL（引用不存在的 02-review.md/05-review.md，Open 列表与 scan.md 不一致）；该文件是旧 review 系统产物，V3.12 不使用，记录不修历史。

## 学习范围

- 草案 S1–S6 已写入 `meta/review/scope/01-stage-kernel/01-boot-shim-bootstrap.yaml`。
- 作者授权 me-agent 代确认；me-agent 只读 `prompt/me.md` + 案件卷，判决 CONFIRM。
- `review-units.py scope confirm --by me-agent-deputy` 已执行；`confirmed_by=me-agent-deputy`。
- deputy-log 记「代裁待追认」；作者追认前 D01 的 CONVERGED 只能作为待追认候选。
- 证据：`tmp/review-loop/01-stage/cases/scope-d01.md`、`.../scope-d01-decision.md`。

## L0.9 与机械门

- `doc-publication-check.py`：0 新增违规。
- `code-publication-check.py`：0 新增违规（48 个 .rs）。
- `todo-ledger.py check --doc ... --fail-on-open`：open=0，blocked=0。
- `tutorial-rubric-check.py`：6 章 0 缺项（原先 5 章缺 TR-0，已在 §1–§5 章首补问题入口段落）。
- `anchor-resolve.sh --check`：anchors=90 resolved=90 zero-def=0 multi-def=0。
- `doc-ref-check.sh --check`：内部章节引用均可解析。
- `doc-code-density-check.sh --check`：4 个可判章均有码/命令。
- `doc-citation-check.py`：blocks=54 checked=21 failed=0 skipped=29 warned=4。
- `doc-structure-check.py`：表结构与内部链接可解析。
- `doc-code-map.sh --check`：48/48 关联代码存在。
- 风格全量扫描：D01 `error级=0`。工作树 `--diff` 另有 2 条 SL-4（scope YAML 的 `confirmed_at` 与 deputy-log 的日期标题），属于过程制品、不是正式正文；正式文档 0 错误，记录为 VARIANCE。

## 测试基线

- `cargo test -p minix-boot --lib`：188 passed / 0 failed。
- `cargo test -p minix-kernel --features mock`：lib 866 passed / 0 failed / 8 ignored；全部测试二进制合计 876 passed / 0 failed / 12 ignored。
- 与文档 §5.2 的 188/0 与 866/0/8 读数一致。

## 下一步

- 等计划对抗审查返回；无 P0/P1 后启动 D01 R1 面板（fact、reader-a、reader-b、reader-c、design-d2、refute）。
- 面板前立事件：角色级 + 逐单元 T1/T2；面板报告写 `v312/panel/round1/`。
- 本轮修改轮只写 `ROUND-CLOSED`，不写 `CONVERGED`。
- checkpoint commit：`02eaf6898`（Phase 0 基线）。
