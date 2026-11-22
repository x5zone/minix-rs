# D01 R1 轨迹复核

TRAJECTORY-REVIEW R1
DOC 01-boot-shim-bootstrap.md

## 结论

PASS（带限制）。

## 复核

- 覆盖：R1 full 的 `metrics-full-r1.md` 列出 49/49 个 `UNIT`；`review-units.py stats` 显示 round=1、full_rounds=1。
- 面板：7 份报告（fact/design-d1/design-d2/reader-a/b/c/refute），P-FACT 检出 P0（pt_alloc 引文与当前源码脱节），结构/因果/读者角色各抓到独立问题。
- 账本：46 条处置，0 未闭环；`eval-trace-check --round R1` PASS 12 / WARN 1 / FAIL 0。
- 机械门：publication 0 新增、tutorial 6/6、citation failed=0、anchor 90/90、doc-code-map 48/48。
- 限制：R1 的单元证据是 full 覆盖行 + 面板制品，不是逐单元独立回执；R1 的 22 个 fix 单元未冻结。R2 必须全篇复审并逐单元 pass/freeze，R3 再逐单元 confirm-review。

## 可重放

rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/metrics-full-r1.md:1
