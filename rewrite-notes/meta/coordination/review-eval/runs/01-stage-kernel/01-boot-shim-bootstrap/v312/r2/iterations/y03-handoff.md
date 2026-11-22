# D01 交接（2026-11-23 06:53 CST）

> 先读这个文件，再看 `BRIEFING.md` 的 y03 节和 `STATUS.md`。

## 一句话状态

D01 的 R1 已 `ROUND-CLOSED`（48/48 冻结、独立 doc/code verifier PASS、R1 冷读 READY）；
R2 full 已完成面板与 23/25 修复，但**卡在两个设计裁决**，且 R2 新人冷读判 `NOT-READY`
（4 个结构性硬阻断）。因此 D01 现在**不能宣告 `CONVERGED`**，R2 保持 `BLOCKED`。

## 需要你做的两个裁决

案件卷：`rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/v312/r2/R2-DESIGN-CASE.md`

1. **DESIGN-01**：是否把 `minix-boot::MODULE_NAMES` 与 `kernel::BOOT_MODULE_PROC_NRS` 合并成
   一张 `{name, proc_nr}` 表？
   - A：合并（净减一张表，关掉两表错位面；需改常量形状与引用）。
   - B：保留两张表，加一条宿主测试钉住 12 对值。
2. **DESIGN-02**：是否把 `kmain` 的 `validate()` 改成 `validate_requiring_sources()`？
   - A：改（1 行，终门按 `KernelInfo.authority` 复核；QEMU 测试内核不编译 `kmain`）。
   - B：保持非 strict，只在文档写明终门不做权威审计。

两项都属 R2 P2；裁决前不写 `ROUND-CLOSED`/`CONVERGED`。

## 第二件事：R3 必须是结构轮

R2 冷读报告：`tmp/review-loop/01-stage/cold-read/y02-d01-r2.md`（`HARD-REJECTIONS: 4`、
`NOT-READY`）。四个硬阻断：

1. 实现章代码是“语义重排、非逐字”的节选，未展开分支要求看源文件；不读源码的读者无法沿一条腿走完。
2. 当前设计与版本历史混写（16→13→14→10、7→8、184→160→128、startup 1440 B 等）。
3. 同一组路径有多套编号/计数（形态/腿/链/产线/面/动作 ID/时间拍/Step/判定五步）。
4. “以 §X 为权威 / 本篇不复制”把 wire 偏移等关键事实推到源码或别节。

R3 建议：现役 `KernelInfo` 字段表、wire 字节布局表、历史移附录、端到端图 + 门×腿矩阵、
计数术语表、`prepare_boot` 签名补 `<'a>`。详见
`v312/r2/R2-COLD-READ-CASE.md`。R3 是第 3 个修改轮，按 V3.8 必须整节结构改；不再开第 4 轮。

## 已完成与证据

- R1（doc `19c0ad9a`，2,951 行）：36 个面板 FINDING（P1 3/P2 18/P3 15）→ 40 根因；
  修复、40/40 单元冻结、`v312-closure-check --round 1` PASS、doc/code verifier PASS、
  冷读 `HARD-REJECTIONS: 0 / READY`、trajectory `RECHECK: PASS`。
- R2（doc `34bee52f`，2,951 行）：25 个 FINDING（P2 11/P3 14）；23 个 `①` 已修；
  `DESIGN-01/02` 两个 `⑤` 待裁；R2 冷读 4 个硬阻断。
- 测试：minix-boot 188/0；boot-shim 136/0；minix-kernel mock 866/0/8；
  `cargo check -p minix-kernel` 通过；R1 三支 `test-higher-half*` QEMU PASS。
- 8 个文档门在 R2 修复后全绿（anchor 86/86、code-map 49/49、ref/style/pub/rubric/density OK、
  citation failed=0）。
- 关键 commit：`76ac760b4`（R1 收口/工具修复）、`5b8325437`（R2 开轮）、
  `11a33354c`（R2 文档/代码批）。
- 两个未跟踪文件仍未纳入：`rewrite-notes/01-stage-kernel/01-design-learn.md`、
  `rewrite-notes/meta/coordination/review-eval/meta/01-boot-shim-bootstrap-r26.md`。

## 流程改进（本轮已落地）

- `panel-index.py`：FINDING 正则改为 `##/### + ASCII ID`，不再把分组标题算一条；
  本轮 INDEX reader-b=4、reader-c=6。
- `v312-closure-check.py`：只认 `- 轮结果: ROUND-CLOSED` 行，注释含 token 不再假通过。
- `review-units.py`：DOC 绑定正则要求 `DOC` 后空白/冒号，`DOC-SHA:` 不再误匹配。
- 三者 `--self-test` 均 PASS，已入 `76ac760b4`。
- 新记 `WF-13`：旧轮 `panel/round2/` 残留；本轮已把 2026-11-08 文件移到 `archive-nov8/`，
  R2 只接受 2026-11-23 写入的报告。

## 建议的下一步

1. 你裁决 DESIGN-01/02（A/B）。
2. 批准 R3 结构轮（或先只批结构、设计项另排）。
3. R3 收口后做确认轮 + G18 全篇冷读；G18 要求 `HARD-REJECTIONS: 0` 才能 `CONVERGED`。
4. 09:00 后进入峰值计费，任务已按要求停在这个自然边界。
