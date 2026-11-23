# D01 y03b 简报：D-120/D-121 落地

## y03b — D-120/D-121 设计裁决落地；R2 仍因冷读 BLOCKED

- 作者裁决：DESIGN-01=A、DESIGN-02=A，写入 `DECISIONS.md` D-120/D-121。
- DESIGN-01：`minix-boot::BOOT_MODULE_IDENTITIES` 成为唯一权威表（名称 + `ProcNr`），
  `MODULE_NAMES`/`BOOT_MODULE_PROC_NRS` 保留为 const 派生视图；`uboot_main` 第三份字面抄本
  删除；`xtask` 顺序锁改 typed 读取。精确口径：独立抄本 2→1，不宣称常量个数字面 -1。
- DESIGN-02：`kmain` 改 `validate_requiring_sources()`；新增宿主 DTB fixture 覆盖
  `fw_end=0` 与 `0x8020_0000`，断言 `sources()` 非空且覆盖审计通过。
- 验证：minix-boot 189/0、boot-shim 136/0、minix-kernel mock 866/0/8、xtask manifest 4/4；
  x86 UEFI 与 riscv64 U-Boot shim 启动门 PASS（`kmain A.1 validate ok` + scheduler）；
  aarch64 直核到 VM init 后仍遇已登记 A2 用户链异常，非 kmain 门失败。
- R2 台账 `open=0`，25 个 FINDING 全部 ①；但 **R2 冷读仍 `NOT-READY`（4 个结构阻断）**，
  R3 结构轮未开，所以 R2 保持 `BLOCKED`，不写 `ROUND-CLOSED`/`CONVERGED`。
- 提交：`7d6835aa5`（D-120/D-121 实现）、`5ce3f9144`、`b079fa0e3`（文档同步/测试计数）。
- 下一步：R3 结构轮（现役字段表、wire 布局、历史移附录、端到端图/门×腿矩阵、计数术语表），
  之后确认轮 + G18。14:00 峰值前写交接并暂停。
