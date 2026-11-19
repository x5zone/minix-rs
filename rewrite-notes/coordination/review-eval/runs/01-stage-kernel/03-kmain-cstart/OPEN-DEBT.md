# D03 开放欠账

> 只有两个出口：实施（附证据）或 DECISIONS.md 判决账行（§4.0f）。未销账行计入未闭环。

| ID | 严重度 | 处置/档期 |
|---|---|---|
| D03-design-batch | P2 | **用户批准 2026-11-12（代理裁决 D-053）延期至下一 design batch**：D2-10 四处陈旧注释、D2-11 TSS 占位/riscv64 断言强度、D2-16 就地初始化 + 删 refresh_descriptors（ARCH 提案，待用户裁决）、R2 的 F03 `bsp_finish_booting` 过期注释与 F04 非 0 BSP 的 TSS 槽 0 写错 |
| R3-design-batch | P2 | **用户批准 2026-11-20（代理裁决 D-066）延期至下一 design batch**：RD03-DESIGN-D2-01..05 五项设计 P2（[DEL] init() 的 8 个 trap gate 死写点、[DEL] 错误码知识三处编码、[MODERN] set_kernel_stack 的 trait 契约在 riscv64/arm64 不可满足、[DEL] 非 x86 哑实例迫使 base==0 哨兵、[DEL] Privilege 关联类型零消费者）与 2 条 ARCH 提案（RD03-A1 保护实例就地构造、RD03-A2 init_protection 按 arch 拆分）。确认轮不实施 ⑥（§3.6），登记后销账 |
