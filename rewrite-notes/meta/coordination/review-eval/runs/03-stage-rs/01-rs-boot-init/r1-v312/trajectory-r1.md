TRAJECTORY-REVIEW R1
DOC 01-rs-boot-init.md
复核对象：R1 full（P-FACT / READER-A/B/C / DESIGN-D2 窄复核 + 本轮修复）
复核点：
1. 面板覆盖：READER-A/B/C 独立冷读；P-FACT 定点 7 项（4 PASS / 3 FAIL）；DESIGN-D2 窄复核 3 项（2 PASS / 1 FAIL）。
2. 归因：四份报告 + 设计复核归并为 24 个根因行，defect-ledger open=0。
3. 修复：Step 1 内核提交、SEF do_sef_rs_init、生产 acquire_from/TrapKernelApi、BootInit RProcTable、run 签名、测试计数、grant/锚点、导航残留。
4. 单元账：39/39 frozen，check OK，每单元独立 UNIT 证据。
5. 已知限制：DESIGN-D2 首个 agent 超时；窄复核补 3 项。原始大范围 P-FACT 也超时，窄口径补 7 项。
结论：R1 轨迹自洽，可 ROUND-CLOSED；不宣告 CONVERGED（还需第 2 个 full 轮、确认轮/G18）。
rg -n 'R1-' rewrite-notes/meta/coordination/review-eval/runs/03-stage-rs/01-rs-boot-init/r1-v312/defect-ledger.md
