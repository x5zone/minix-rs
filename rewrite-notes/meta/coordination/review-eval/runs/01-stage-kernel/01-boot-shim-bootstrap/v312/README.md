# D01 V3.12 run 目录说明

本目录是 `01-boot-shim-bootstrap` 的 V3.12 review 运行现场。

## 为什么单独一层

同一篇文档的 canonical run 目录（父目录）已经保存了 V3.11 的完整证据：
`panel/round1/`、`panel/round2/`、`defect-ledger.md`、`loop.md`、`metrics.md`、`FREEZE.md` 等。
V3.12 的 `v312-closure-check.py` 固定读取 `<run-dir>/panel/round<N>/`、`loop.md`、`metrics.md`、
`defect-ledger.md`；如果直接把 V3.12 写在父目录，会覆盖 V3.11 历史证据。

因此 V3.12 全部写入本目录 `v312/`，父目录保持只读历史。工具调用统一带
`--run-dir rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312`。

## 目录内容

- `SCOPE.md`：本轮内容范围裁决。
- `related-code.txt`：48 个关联代码文件的清单（从父目录复制）。
- `pre-r1/`：V3.12 早期未完成 R1 的输入（`baseline.md`、`reader-a-r1.md`）。它们不是本轮证据；
  旧 READER-A 报告产生于 L0.9 清零之前，本轮作废并重派。
- `r1-v312/`：R1 的基线、full-round 证据、轨迹。
- `panel/round<N>/`：每轮面板原始报告（逐字保留）。
- `unit-r1/` 等：逐单元证据。
- `iterations/`：每轮简报入库副本，与 checkpoint commit 一一对应。

## 历史边界

- 父目录的 V3.11 `FREEZE.md` 记录该 edition 曾被机械判定 CONVERGED，但已被用户 D-116 撤销
  （理由：人工审阅“我根本读不进去”）。V3.11 的轮次、冻结、trajectory 是历史事实，不作为
  V3.12 的起点或证据。
- `runs/01-stage-kernel/USER-REVIEW-REVOKE-2026-11-22.md` 是撤销记录。
- 本目录的 V3.12 账户从 R0 重新开始，48 个单元全部未冻结。
