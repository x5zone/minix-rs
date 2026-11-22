# D01 R1 metrics（V3.12，post-DESIGN-02）

## R1（V3.12，full）

- 发现总数: 36
- 唯一根因数: 40
- 本轮新准入: 36
- certify 新准入: 5
- 重复合并: 4
- 自引入: 1
- 处置: ① 39 / ② 0 / ④ 0 / ⑤ 0 / ⑥ 0 / FP 1 / SUGG 26
- 未闭环: 0
- 轮型: full
- 轮结果: ROUND-CLOSED
- 代裁决: 0 / ARCH 提案: 0 / 用户否决: 0
- ARCH 提案: 0
- 用户否决: 0
- edition: V3.12
- re-litigation: 0
- adjacent 阻塞: 0
- 重构: 提出 0 / 接受 0 / 实施 0 / 登记 0
- 减法: 提案 4 / 采纳 6
- 成本: agent 8 / token 未统计 / 耗时 未统计

说明：`发现总数` 只数面板 FINDING（36）；certify/refute 的根因单列在 `certify 新准入`
与台账 `R1-*` 计数中，故 `唯一根因数` 40 大于 `发现总数` 不是三账不同源。

关账记录：2026-11-23 06:06 CST
`python3 tools/v312-closure-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md
--run-dir rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r1-post-design02
--round 1` → `v312-closure-check: PASS`；`event verify --fresh` → `OK`；
`review-units.py audit` → 48 单元 `open=0`；`review-units.py stats` → `frozen=48/48`、
`t1_pending=0`、`t2_pending=0`、`reopened=0`、`stale=0`。
