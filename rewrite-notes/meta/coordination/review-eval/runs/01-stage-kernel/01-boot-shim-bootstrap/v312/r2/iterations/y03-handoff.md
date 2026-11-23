# D01 交接（2026-11-23 06:53 CST）

> 先读这个文件，再看 `BRIEFING.md` 的 y03 节和 `STATUS.md`。

## 一句话状态

D01 的 R1 已 `ROUND-CLOSED`（48/48 冻结、独立 doc/code verifier PASS、R1 冷读 READY）；
R2 full 也已 `ROUND-CLOSED`：25/25 面板发现与设计裁决 D-120/D-121（均选 A）已落地，
台账 `open=0`、48/48 冻结、`v312-closure-check --round 2` PASS、R2 trajectory PASS。
但 R2 新人冷读判 `NOT-READY`（4 个结构性硬阻断），R3 结构轮尚未开始。
因此 D01 现在**不能宣告 `CONVERGED`**；R3 是下一轮。

## 设计裁决：已完成（A/A）

- `DECISIONS.md` D-120：DESIGN-01 选 A。`minix-boot::BOOT_MODULE_IDENTITIES` 为唯一权威表；
  `MODULE_NAMES`/`BOOT_MODULE_PROC_NRS` 保留 const 派生视图；`uboot_main` 第三份抄本删除；
  `xtask` order lock 改 typed 读取。独立抄本 2→1，不宣称常量个数字面 -1。
- `DECISIONS.md` D-121：DESIGN-02 选 A。`kmain` 改 `validate_requiring_sources()`；
  宿主 DTB fixture 覆盖 `fw_end=0` 与 `0x8020_0000`；x86 UEFI 与 riscv64 U-Boot shim 启动门 PASS。
- 实现 commit：`7d6835aa5`；文档同步 `5ce3f9144`/`b079fa0e3`；R2 关账 `bc60fb26d`，终态 `166366a89`。
- 设计批独立 verifier：`certify/y03-design-verify.md` PASS，`RECHECK: PASS`（doc `cbc105c7…`）。

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
- 测试：minix-boot 189/0；boot-shim 136/0；minix-kernel mock 866/0/8；xtask manifest 4/4；
  `cargo check -p minix-kernel` 通过；R1 三支 `test-higher-half*` QEMU PASS。
- 8 个文档门在 R2 修复后全绿（anchor 86/86、code-map 49/49、ref/style/pub/rubric/density OK、
  citation failed=0）。
- R2 后 `units_review_lt2=31`：R3 必须覆盖这 31 个改过的单元，确认轮前达到 ≥2 轮。
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

1. R3 结构轮（已获批准）：现役字段表、wire 布局表、历史移附录、端到端图/门×腿矩阵、计数术语表。
2. R3 收口后做确认轮 + G18 全篇冷读；G18 要求 `HARD-REJECTIONS: 0` 才能 `CONVERGED`。
3. 14:00 进入峰值计费，任务停在这个自然边界；18:00 后重启。R2 已关账，R3 从结构改写开始。

### R3 开轮前要做的账务

1. 为结构轮建立新的 edition/round 账户（按 `review-units.md` 的 structural round 规则），
   不要直接把 R2 的 `ROUND-CLOSED` 改成 `CONVERGED`。
2. R3 的输入是 `R2-COLD-READ-CASE.md` 的四个硬阻断；完成后必须再跑一次独立冷读。
3. R3 覆盖 31 个 `units_review_lt2=1` 的单元；确认轮前每个单元 ≥2 轮有效 review。
