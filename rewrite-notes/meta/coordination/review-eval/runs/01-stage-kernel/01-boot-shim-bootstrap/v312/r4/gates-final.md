DOC 01-boot-shim-bootstrap.md

# D01 收敛门证据（R4，T3）

- 文档：3,085 行，sha256 `67440cd885aad0e808cd5d76905323872b08b1c35849566b976c123d84dff6e8`
- G18 学生冷读：`HARD-REJECTIONS: 0`、`VERDICT: READY`；48 个单元逐行“可跟读”；S1–S6 全“能完成”。
- 确认轮 R4：48/48 单元完成 `--confirm-review`；`stats` 为 `frozen=48/48`、
  `t1_pending=t2_pending=reopened=stale=0`、`units_review_lt2=0`、`units_post_freeze_0=0`。
- 轨迹：R1、R2、R3、R4 均有 PASS 的 `trajectory_reviews`。
- R3 关账：`v312-closure-check --round 3 --run-dir …/v312/r3` PASS；台账 28 行、根因 20、`open=0`。
- 文档门：anchor 87/87、code-map 49/49、ref/style/pub/rubric/citation/density/structure 全绿。
- 宿主：minix-boot 190/0、boot-shim 136/0、minix-kernel mock 866/0/8、xtask 1/1。
- QEMU：x86_64 UEFI、aarch64 UEFI、riscv64 U-Boot shim、riscv64 `bootface` 四门 PASS。
- 学习范围：`confirmed_by=me-agent-deputy`（待作者追认；工具门已按 confirmed=true 通过）。

## 重放

rg -n 'HARD-REJECTIONS: 0|VERDICT: READY' rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r4/G18-COLD-READ.md
rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r4/G18-COLD-READ.md:2
- `sed -n '1,3p' v312/r4/G18-COLD-READ.md` → DOC / HARD-REJECTIONS: 0 / VERDICT: READY。
- `python3 tools/review-units.py stats rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` → 上面的 48/48 与 0/0 读数。
- `python3 tools/review-units.py event verify rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --fresh` → OK。
- `python3 tools/v312-closure-check.py rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --round 3 --run-dir rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r3` → PASS。
- `tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` → anchors=87 resolved=87。
