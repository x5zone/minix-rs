# D01 R1（V3.12）基线

- 文档：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- 行数：2,983
- sha256：`82271968c81aae8630a70a886e80fb031546d79e80f84cf73df9a88c1842fe90`
- 基线 commit：`64f76309fe3393f9884f797335e582412f18480f`
- edition：V3.12（从 V3.11 重开，旧计数与旧冻结证据归档在父目录）
- 轮型：full（整篇 survey）
- 单元：48（6 章 + 42 节），全部未冻结
- run 目录：`rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312`
- 关联代码：`related-code.txt` 48 个文件，存在性 48/48
- 学习范围：`v312/SCOPE.md` 与 `meta/review/scope/01-stage-kernel/01-boot-shim-bootstrap.yaml`（已由 me-agent 代确认，`confirmed_by=me-agent-deputy`，代裁待追认）
- L0.9 读数（2026-11-23）：
  - `doc-publication-check.py`：无新增违规
  - `code-publication-check.py`：无新增违规
  - `todo-ledger.py check --doc ... --fail-on-open`：open=0，blocked=0
  - `tutorial-rubric-check.py`：6 章，0 章缺项（原先 5 章缺 TR-0，已补 5 段问题入口）
- 机械门读数：
  - `anchor-resolve.sh --check`：anchors=90 resolved=90 zero-def=0 multi-def=0
  - `doc-ref-check.sh --check`：内部章节引用均可解析
  - `doc-code-density-check.sh --check`：4 个可判章均有码/命令
  - `doc-citation-check.py`：blocks=54 checked=21 failed=0 skipped=29 warned=4
  - `doc-code-map.sh --check`：48/48 存在
- 说明：V3.12 早期未完成 R1 的 `baseline.md` 与 `reader-a-r1.md` 保存在 `v312/pre-r1/`，
  不作为本轮证据；旧 READER-A 产生于 L0.9 清零之前，本轮作废并重派。
