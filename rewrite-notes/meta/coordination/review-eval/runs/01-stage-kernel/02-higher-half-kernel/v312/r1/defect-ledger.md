# D02 R1 缺陷台账（V3.12，BLOCKED）

- 轮次: R1 full；轮结果: ROUND-OPEN（P0 已清；P1 8 / P2 33 / P3 7 待修）
- 面板 FINDING 53：见 `panel/round1/`；P-REFUTE 6 CLAIM。
- 本表在阻断点先登记全部面板 ID；处置列 `OPEN` 表示尚未修，`⑤` 表示待作者裁决。

| 发现 ID | 严重度 | 来源 | 处置类 | 说明 |
|---|---|---|---|---|
| FACT-01 | P0 | P-FACT | ① | ① 已修：发送侧/载体统一为 `BootReport`，`KERNEL_ENTRY_ANCHOR` 签名同步 |
| FACT-02 | P0 | P-FACT | ① | `[ARCH: boot 最小恒等窗口；跨镜像腿不再铺低 4 GiB]` 三处补齐：文档 §1.2/§4.3、`os/arch/src/lib.rs:BOOT_IDENTITY_MAP_END`、`os/kernel/src/lib.rs` Step 1–3 注释与 `02-design.v4.md` |
| FACT-03 | P2 | P-FACT | OPEN | 待修；见面板报告 |
| FACT-04 | P2 | P-FACT | OPEN | 待修；见面板报告 |
| FACT-05 | P2 | P-FACT | OPEN | 待修；见面板报告 |
| FACT-06 | P2 | P-FACT | OPEN | 待修；见面板报告 |
| FACT-07 | P3 | P-FACT | OPEN | 待修；见面板报告 |
| FACT-08 | P3 | P-FACT | OPEN | 待修；见面板报告 |
| FACT-09 | P3 | P-FACT | OPEN | 待修；见面板报告 |
| READER-A-01 | P1 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-02 | P2 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-03 | P2 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-04 | P1 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-05 | P2 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-06 | P2 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-07 | P2 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-08 | P2 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-09 | P2 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-10 | P1 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-11 | P2 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-12 | P2 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-13 | P2 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-14 | P3 | READER-A | OPEN | 待修；见面板报告 |
| READER-A-15 | P3 | READER-A | OPEN | 待修；见面板报告 |
| READER-B-01 | P1 | READER-B | OPEN | 待修；见面板报告 |
| READER-B-02 | P1 | READER-B | OPEN | 待修；见面板报告 |
| READER-B-03 | P1 | READER-B | OPEN | 待修；见面板报告 |
| READER-B-04 | P2 | READER-B | OPEN | 待修；见面板报告 |
| READER-B-05 | P2 | READER-B | OPEN | 待修；见面板报告 |
| READER-B-06 | P2 | READER-B | OPEN | 待修；见面板报告 |
| READER-B-07 | P2 | READER-B | OPEN | 待修；见面板报告 |
| READER-B-08 | P2 | READER-B | OPEN | 待修；见面板报告 |
| READER-B-09 | P2 | READER-B | OPEN | 待修；见面板报告 |
| READER-B-10 | P2 | READER-B | OPEN | 待修；见面板报告 |
| READER-B-11 | P2 | READER-B | OPEN | 待修；见面板报告 |
| READER-B-12 | P2 | READER-B | OPEN | 待修；见面板报告 |
| READER-B-13 | P2 | READER-B | OPEN | 待修；见面板报告 |
| READER-C-01 | P1 | READER-C | OPEN | 待修；见面板报告 |
| READER-C-02 | P2 | READER-C | OPEN | 待修；见面板报告 |
| READER-C-03 | P2 | READER-C | OPEN | 待修；见面板报告 |
| READER-C-04 | P2 | READER-C | OPEN | 待修；见面板报告 |
| READER-C-05 | P2 | READER-C | OPEN | 待修；见面板报告 |
| READER-C-06 | P2 | READER-C | OPEN | 待修；见面板报告 |
| READER-C-07 | P2 | READER-C | OPEN | 待修；见面板报告 |
| READER-C-08 | P2 | READER-C | OPEN | 待修；见面板报告 |
| DESIGN-01 | P0 | DESIGN-D2 | ① | ① 已修：`kmain` 断言接受高半/物理两种别名（commit 7ff12bca4） |
| DESIGN-02 | P0 | DESIGN-D2 | ① | ① 已修：路径图发送侧改为 `BootReport` |
| DESIGN-03 | P0 | DESIGN-D2 | ① | ① 已修：`KERNEL_ENTRY_ANCHOR` 签名 |
| DESIGN-04 | P1 | DESIGN-D2 | OPEN | 待修；见面板报告 |
| DESIGN-05 | P2 | DESIGN-D2 | OPEN | 待修；见面板报告 |
| DESIGN-06 | P2 | DESIGN-D2 | OPEN | 待修；见面板报告 |
| DESIGN-07 | P3 | DESIGN-D2 | OPEN | 待修；见面板报告 |
| DESIGN-08 | P3 | DESIGN-D2 | OPEN | 待修；见面板报告 |
