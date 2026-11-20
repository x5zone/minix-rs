# D03 开放欠账

> 只有两个出口：实施（附证据）或 DECISIONS.md 判决账行（§4.0f）。未销账行计入未闭环。

| ID | 严重度 | 处置/档期 |
|---|---|---|
| D03-design-batch | P2 | **2026-11-21 用户裁决 D-099（V3.6 分层处置）后改写**：**待实施（Tier 1，纯减法/紧化）**——D2-15 删两冗余 unsafe impl、D2-06 删三架构死字段 cpu_count 及三条自证测试、D2-11 riscv `set_kernel_stack` 的 `debug_assert_ne!`→`assert_ne!`、非 x86 `with_protection_mut`（lib.rs:2502）零调用者死代码删除；**待实施（Tier 2，行为等价）**——A-2-lite：cpu_count 删除后两非 x86 类型 ZST 化，`with_protection` 改返回 `&'static` 实例，哑实例与 `base==0` 哨兵退役（前提句迁入 init doc）；**已裁决不采纳（触发器制）**——D2-16/RD03-A1 的 A-1 本体（init 改 `&mut` 签名 + 删 refresh_descriptors 全家），理由与重开触发器见判决行 D-099；**维持延期**——D2-10 四处陈旧注释、F03 `bsp_finish_booting` 过期注释、F04 非 0 BSP 的 TSS 槽 0 写错 |
| R3-design-batch | P2 | **2026-11-21 用户裁决 D-099 后改写**：**已解决**——RD03-DESIGN-D2-04（非 x86 哑实例迫使 base==0 哨兵）的实质由 D-099 Tier 2 的 A-2-lite 承接（哑实例与哨兵一并退役）；**已裁决不采纳（触发器制）**——RD03-A1（与 D2-16 同主题，A-1 本体）与 RD03-A2（init_protection 三分体拆分）本体，见判决行 D-099 的理由与重开触发器；**维持延期**——RD03-DESIGN-D2-01（init() 的 8 个 trap gate 死写点）、D2-02（错误码知识三处编码）、D2-03（set_kernel_stack 的 trait 契约）、D2-05（Privilege 关联类型零消费者） |
