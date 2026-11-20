# D03 开放欠账

> 只有两个出口：实施（附证据）或 DECISIONS.md 判决账行（§4.0f）。未销账行计入未闭环。

| ID | 严重度 | 处置/档期 |
|---|---|---|
| D03-design-batch | P2 | **2026-11-21 用户裁决 D-099（V3.6 分层处置）后改写**：**待实施（Tier 1，纯减法/紧化）**——D2-15 删两冗余 unsafe impl、D2-06 删三架构死字段 cpu_count 及三条自证测试、D2-11 riscv `set_kernel_stack` 的 `debug_assert_ne!`→`assert_ne!`、非 x86 `with_protection_mut`（lib.rs:2502）零调用者死代码删除；**待实施（Tier 2，行为等价）**——A-2-lite：cpu_count 删除后两非 x86 类型 ZST 化，`with_protection` 改返回 `&'static` 实例，哑实例与 `base==0` 哨兵退役（前提句迁入 init doc）；**已裁决不采纳（触发器制）**——D2-16/RD03-A1 的 A-1 本体（init 改 `&mut` 签名 + 删 refresh_descriptors 全家），理由与重开触发器见判决行 D-099；**维持延期**——D2-10 四处陈旧注释、F03 `bsp_finish_booting` 过期注释、F04 非 0 BSP 的 TSS 槽 0 写错 |
| F01-IST-nmi-arch | P3 | **2026-11-21 用户裁决 D-100（保留 IST1/IST2，拆三件）后立行**：**待实施**——F01a 注释事实修正（`os/arch/src/x86_64/protection.rs:264-266` 的 "C gate_table_exceptions parity" 括号挂在 `ist[0]/ist[1]` 赋值上按字面为假：C 的 32 位门与 TSS 无 IST 字段，IST 是长模式新增；改写为「门元数据 parity 成立 + IST 落专用栈是 [ARCH: IST 取代 i386 同栈嵌套] 机制差异 + 消费方经帧内保存 RSP 定位 + C 默认 watchdog 的 push %esp 形状移植时必须改写」）；F01b `[ARCH]` 三处一致标注（文档 `03-kmain-cstart.md:757` 偏差表行补语义句 + 设计快照 + `x86_64/trap_entry.rs:141`/`:212` 短版）；F08 IST 栈 16 对齐（`IST_STACKS` 是 align-1 的 `u8` 数组（`protection.rs:330-331`），IST 交付的 RSP 无 16 对齐保证——`#[repr(align(16))]` 包装，独立 commit）；**不实施（触发器制）**——NMI 重入守卫（Linux 式 per-CPU flag 退回普通栈），触发器 = 出现第一个真实 NMI 消费方（watchdog/剖析），届时同步重核 IST 栈 8 KiB 容量 |
| R3-design-batch | P2 | **2026-11-21 用户裁决 D-099 后改写**：**已解决**——RD03-DESIGN-D2-04（非 x86 哑实例迫使 base==0 哨兵）的实质由 D-099 Tier 2 的 A-2-lite 承接（哑实例与哨兵一并退役）；**已裁决不采纳（触发器制）**——RD03-A1（与 D2-16 同主题，A-1 本体）与 RD03-A2（init_protection 三分体拆分）本体，见判决行 D-099 的理由与重开触发器；**维持延期**——RD03-DESIGN-D2-01（init() 的 8 个 trap gate 死写点）、D2-02（错误码知识三处编码）、D2-03（set_kernel_stack 的 trait 契约）、D2-05（Privilege 关联类型零消费者） |
