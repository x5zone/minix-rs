# D01 y04 简报：R2 ROUND-CLOSED

## y04 — R2 ROUND-CLOSED：D-120/D-121 落地，冷读 NOT-READY 交 R3

- R2 25 个 FINDING 全部 `①`：23 个文档/代码修复 + DESIGN-01/02 按作者裁决 A 落地
  （D-120/D-121）。
- D-120：`BOOT_MODULE_IDENTITIES` 唯一权威表；`MODULE_NAMES`/`BOOT_MODULE_PROC_NRS`
  const 派生视图；uboot 第三份抄本删除；xtask order lock typed 读取。独立抄本 2→1。
- D-121：`kmain` 改 `validate_requiring_sources()`；双 `fw_end` 宿主 DTB fixture；
  x86 UEFI 与 riscv64 U-Boot shim 启动门 PASS；aarch64 直核 kmain 门前通过、后遇已登记 A2。
- 测试：minix-boot 189/0、boot-shim 136/0、minix-kernel mock 866/0/8、xtask manifest 4/4。
- 独立设计 verifier：PASS / `RECHECK: PASS`（doc `cbc105c7…`）；R2 trajectory：PASS；
  `v312-closure-check --round 2` PASS；台账 `open=0`；`frozen=48/48`；`event verify --fresh` OK。
- R2 冷读仍 `HARD-REJECTIONS: 4 / NOT-READY`：实现节选不可跟随、版本历史混入正文、
  多套编号/计数、权威表式交叉引用。这是 **R3 结构轮**的输入，不是 R2 未闭环项。
- R2 后 `units_review_lt2=31`：R3 必须覆盖这 31 个改过的单元，确认轮前达到 ≥2 轮。
- 提交：`7d6835aa5`（D-120/D-121）、`5ce3f9144`/`b079fa0e3`（文档同步）、
  `bc60fb26d`（R2 关账）、后续 trajectory/交接 commit。
- 下一步：R3 结构轮（现役字段表、wire 布局表、历史移附录、端到端图/门×腿矩阵、计数术语表）；
  14:00 峰值暂停，18:00 后从 `HANDOFF.md` 重启。
