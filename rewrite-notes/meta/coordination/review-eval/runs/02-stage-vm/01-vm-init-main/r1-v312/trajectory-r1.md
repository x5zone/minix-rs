TRAJECTORY-REVIEW R1
DOC 01-vm-init-main.md
复核对象：R1 full（五角色面板 + P-FACT 定点复核 + 本轮修复）
复核点：
1. 面板是否覆盖整篇：reader-a/b/c 为独立冷读，design-d2 与 fact 对照 C/Rust；材料隔离遵守。
2. 发现是否归因：79 条原始发现归并为 34 个根因行，defect-ledger 全部 ① 闭环。
3. 修复是否可重放：doc-publication 0、tutorial-rubric 0、style 0、ref 通过、density 通过；cargo test -p minix-vm --lib 549/0。
4. 单元账：43/43 frozen，check OK，audit open=0；每单元有独立 UNIT 证据。
5. 已知限制：P-FACT 窄口径后补，原始 P-FACT 大范围 agent 超时；fact-check 以同 agent MANUAL_FALLBACK 记录，后补 P-FACT 的 4 条已修复。
结论：R1 轨迹自洽，可 ROUND-CLOSED；不宣告 CONVERGED（还需第 2 个 full 轮与确认轮/G18）。
rg -n 'PASS|FAIL' rewrite-notes/meta/coordination/review-eval/runs/02-stage-vm/01-vm-init-main/r1-v312/defect-ledger.md
