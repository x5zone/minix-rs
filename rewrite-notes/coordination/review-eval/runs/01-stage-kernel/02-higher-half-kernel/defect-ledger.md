# D02 缺陷与处置台账

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| DESIGN-01 | design-d2 | P2 | 文档/引文数值 | ① | 已修：§4.3 riscv64 引文改为源码原值 `0xffffffbf7ffe0000`，并注明原文加数笔误与按视图差复算的结果 | panel/design-d2.md DESIGN-01 | L4 文档五门重跑 |
| DESIGN-02 | design-d2 | P1 | 文档/因果理由 | ① | 已修：§4.3 aarch64 理由改为“相对分支不能改变执行视图”，删掉不成立的 ±128MiB 范围论 | panel/design-d2.md DESIGN-02 | L4 文档五门重跑 |
| DESIGN-03 | design-d2 | P2 | 设计/类型约束 | ② | 转出：follow-ups.md D02-F01（`jump_to_kmain` 删 `stack_top` 参数或加 debug_assert） | panel/design-d2.md DESIGN-03 | 不适用（转出） |
| DESIGN-04 | design-d2 | P2 | 设计/可推导字段 | ② | 转出：follow-ups.md D02-F02（`kern_stack_top == kern_virt_base + kern_size` 的校验或类型化） | panel/design-d2.md DESIGN-04 | 不适用（转出，需用户裁决） |
| DESIGN-05 | design-d2 | P2 | 设计/死脚本 | ② | 转出：follow-ups.md D02-F03（arch link.ld 无生产读者且与生产脚本不等价；删或修，需用户确认仓外消费者） | panel/design-d2.md DESIGN-05 | 不适用（转出） |
| DESIGN-06 | design-d2 | P1 | 设计/孪生函数 | ② | 转出：follow-ups.md D02-F04（loader 缓冲区版与物理地址版抽成同一区间计算） | panel/design-d2.md DESIGN-06 | 不适用（转出） |
| DESIGN-07 | design-d2 | P1 | 设计/契约靠调用顺序 | ② | 转出：follow-ups.md D02-F05（解析产出 `KernelImage`，摆放只接受已验证值） | panel/design-d2.md DESIGN-07 | 不适用（转出） |
| DESIGN-08 | design-d2 | P2 | 设计/链接期断言 | ② | 转出：follow-ups.md D02-F06（三份生产脚本补 2MiB 收口 ASSERT） | panel/design-d2.md DESIGN-08 | 不适用（转出） |
| DESIGN-09 | design-d2 | P2 | 代码/注释 | ① | 已修：`higher_half.rs` trait 注释改为“启动尾段调用”与“只能验证签名” | os/kernel/src/boot/higher_half.rs 注释 | cargo check -p minix-kernel --features mock：exit 0 |
| DESIGN-10 | design-d2 | P1 | 设计/拒绝项 | ④ | 非缺陷：`Paging`/`HugePages`/`BootArch` 有真实语义职责，保留 | panel/design-d2.md DESIGN-10 | 不适用（拒绝） |
| DESIGN-11 | design-d2 | P1 | 设计/重复恢复序列 | ② | 转出：follow-ups.md D02-F07（三份 gain 恢复序列收成泛型函数） | panel/design-d2.md DESIGN-11 | 不适用（转出） |
| DESIGN-12 | design-d2 | P1 | 文档/覆盖状态 | ① | 已修文档：§3.1 限定 aarch64 无等值对账，§5.4 M10 改部分覆盖；代码侧 L9 扩 aarch64 转出 D02-F11 | panel/design-d2.md DESIGN-12 | L4 文档五门重跑 |
| DESIGN-13 | design-d2 | P2 | 设计/重复自引导面 | ② | 转出：follow-ups.md D02-F08（bootface 与 bootface_a64 流程去重，需用户裁决） | panel/design-d2.md DESIGN-13 | 不适用（转出） |
| DESIGN-14 | design-d2 | P2 | 文档/外部行为来源 | ① | 已修：§4.4 补 aarch64 direct-core“低地址进入”来自固件平移入口的说明与 QEMU `-d in_asm` 观测入口 | panel/design-d2.md DESIGN-14 | L4 文档五门重跑 |
| DESIGN-15 | design-d2 | P2 | 设计/分类项 | ④ | 非缺陷：翻译残留分类清单，供后续 code-excellence 参考 | panel/design-d2.md DESIGN-15 | 不适用（分类） |
| FACT-01 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-01 的 E1/E2 证据 | L4 文档五门重跑 |
| FACT-02 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-02 的 E1/E2 证据 | L4 文档五门重跑 |
| FACT-03 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-03 的 E1/E2 证据 | L4 文档五门重跑 |
| FACT-04 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-04 的 E1/E2 证据 | L4 文档五门重跑 |
| FACT-05 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-05 的 E1/E2 证据 | L4 文档五门重跑 |
| FACT-06 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-06 的 E1/E2 证据 | L4 文档五门重跑 |
| FACT-07 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-07 的 E1/E2 证据 | L4 文档五门重跑 |
| FACT-08 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-08 的 E1/E2 证据 | L4 文档五门重跑 |
| FACT-09 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-09 的 E1/E2 证据 | L4 文档五门重跑 |
| FACT-10 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-10 的 E1/E2 证据 | L4 文档五门重跑 |
| FACT-11 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-11 的 E1/E2 证据 | L4 文档五门重跑 |
| FACT-12 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-12 的 E1/E2 证据 | L4 文档五门重跑 |
| READER-A-01 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader A-01 的读者路径/反例项） | panel/reader-a.md READER-A-01 | 不适用（转出） |
| READER-A-02 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader A-02 的读者路径/反例项） | panel/reader-a.md READER-A-02 | 不适用（转出） |
| READER-A-03 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader A-03 的读者路径/反例项） | panel/reader-a.md READER-A-03 | 不适用（转出） |
| READER-A-04 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader A-04 的读者路径/反例项） | panel/reader-a.md READER-A-04 | 不适用（转出） |
| READER-A-05 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader A-05 的读者路径/反例项） | panel/reader-a.md READER-A-05 | 不适用（转出） |
| READER-A-06 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader A-06 的读者路径/反例项） | panel/reader-a.md READER-A-06 | 不适用（转出） |
| READER-A-07 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader A-07 的读者路径/反例项） | panel/reader-a.md READER-A-07 | 不适用（转出） |
| READER-A-08 | reader-a | P1 | 读者/结构 | ① | 已修：同 READER-C-03（§3.4 明确 x86_64 无运行时检查，靠链接布局） | panel/reader-a.md READER-A-08 | L4 文档五门重跑 |
| READER-A-09 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader A-09 的读者路径/反例项） | panel/reader-a.md READER-A-09 | 不适用（转出） |
| READER-A-10 | reader-a | P2 | 文档/前置知识 | ② | 部分应用（开头前置知识已限定两条路径的不同起点）；A-10 要求补的 01 篇小节转出 D02-F10 | panel/reader-a.md READER-A-10 | 不适用（转出） |
| READER-A-11 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader A-11 的读者路径/反例项） | panel/reader-a.md READER-A-11 | 不适用（转出） |
| READER-A-12 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader A-12 的读者路径/反例项） | panel/reader-a.md READER-A-12 | 不适用（转出） |
| READER-B-01 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader B-01 的结构/教学项） | panel/reader-b.md READER-B-01 | 不适用（转出） |
| READER-B-02 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader B-02 的结构/教学项） | panel/reader-b.md READER-B-02 | 不适用（转出） |
| READER-B-03 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader B-03 的结构/教学项） | panel/reader-b.md READER-B-03 | 不适用（转出） |
| READER-B-04 | reader-b | P2 | 读者/结构 | ① | 已修：同 FACT-09（§2.2 的“上一段”改为“§6.2 的两条 printf”） | panel/reader-b.md READER-B-04 | L4 文档五门重跑 |
| READER-B-05 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader B-05 的结构/教学项） | panel/reader-b.md READER-B-05 | 不适用（转出） |
| READER-B-06 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader B-06 的结构/教学项） | panel/reader-b.md READER-B-06 | 不适用（转出） |
| READER-B-07 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader B-07 的结构/教学项） | panel/reader-b.md READER-B-07 | 不适用（转出） |
| READER-B-08 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader B-08 的结构/教学项） | panel/reader-b.md READER-B-08 | 不适用（转出） |
| READER-B-09 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader B-09 的结构/教学项） | panel/reader-b.md READER-B-09 | 不适用（转出） |
| READER-B-10 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader B-10 的结构/教学项） | panel/reader-b.md READER-B-10 | 不适用（转出） |
| READER-B-11 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader B-11 的结构/教学项） | panel/reader-b.md READER-B-11 | 不适用（转出） |
| READER-B-12 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader B-12 的结构/教学项） | panel/reader-b.md READER-B-12 | 不适用（转出） |
| READER-B-13 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader B-13 的结构/教学项） | panel/reader-b.md READER-B-13 | 不适用（转出） |
| READER-B-14 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D02-F10（reader B-14 的结构/教学项） | panel/reader-b.md READER-B-14 | 不适用（转出） |
| READER-C-01 | reader-c | P1 | 文档/内部一致性 | ① | 已修：开头前置知识限定两条路径的不同起点 | panel/reader-c.md READER-C-01 | L4 文档五门重跑 |
| READER-C-02 | reader-c | P1 | 文档/内部一致性 | ① | 已修：§3.4 表第一行的保证者改为 direct 面 `arch_boot_finish` / v1 面 `resume_after_v1_decode` | panel/reader-c.md READER-C-02 | L4 文档五门重跑 |
| READER-C-03 | reader-c | P1 | 文档/内部一致性 | ① | 已修：§3.4 明确 x86_64 没有运行时检查，靠链接布局，riscv64 才有显式补偿 | panel/reader-c.md READER-C-03 | L4 文档五门重跑 |
| READER-C-04 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D02-F10（reader C-04 的一致性/指针项） | panel/reader-c.md READER-C-04 | 不适用（转出） |
| READER-C-05 | reader-c | P2 | 文档/内部一致性 | ① | 已修：§2.3 与 §4.3 区分 C 侧 `pre_init` 帧指针与 Rust 侧 0 终止 | panel/reader-c.md READER-C-05 | L4 文档五门重跑 |
| READER-C-06 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D02-F10（reader C-06 的一致性/指针项） | panel/reader-c.md READER-C-06 | 不适用（转出） |
| READER-C-07 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D02-F10（reader C-07 的一致性/指针项） | panel/reader-c.md READER-C-07 | 不适用（转出） |
| READER-C-08 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D02-F10（reader C-08 的一致性/指针项） | panel/reader-c.md READER-C-08 | 不适用（转出） |
| READER-C-09 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D02-F10（reader C-09 的一致性/指针项） | panel/reader-c.md READER-C-09 | 不适用（转出） |
| READER-C-10 | reader-c | P2 | 文档/内部一致性 | ① | 已修：§2.2 注明 ARM 物理基址 `0x80200000` 不是 4MiB 对齐，不能套 i386 判据 | panel/reader-c.md READER-C-10 | L4 文档五门重跑 |
| READER-C-11 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D02-F10（reader C-11 的一致性/指针项） | panel/reader-c.md READER-C-11 | 不适用（转出） |
| READER-C-12 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D02-F10（reader C-12 的一致性/指针项） | panel/reader-c.md READER-C-12 | 不适用（转出） |
| READER-C-13 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D02-F10（reader C-13 的一致性/指针项） | panel/reader-c.md READER-C-13 | 不适用（转出） |
| READER-C-14 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D02-F10（reader C-14 的一致性/指针项） | panel/reader-c.md READER-C-14 | 不适用（转出） |
| READER-C-15 | reader-c | P2 | 读者/一致性 | ③ | 误报：`_MINIX_MAGIC` 在 `sys.mk:24-28` 是有条件定义，文档说法正确 | panel/reader-c.md READER-C-15 | 不适用（误报） |
| READER-C-16 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D02-F10（reader C-16 的一致性/指针项） | panel/reader-c.md READER-C-16 | 不适用（转出） |


## P-REFUTE 附加（不属于 `## FINDING` 台账）

| 主张 | 结果 | 处置 |
|---|---|---|
| CLAIM 01–03 | 反证失败，主张成立 | 无动作 |
| CLAIM 04 | 反证成功：aarch64 去 `isb` 的失败推演不可复现 | 已修 §4.3 其二与 §4.3 `isb` 说明 |
| CLAIM 05 | 主张成立；附 P2 ×2（`FallbackUnsupported` 未入表、aarch64 `KERN_SIZE` 无 L9） | `FallbackUnsupported` 已补；`KERN_SIZE` 对账转 D02-F11 |
| CLAIM 06 | 主张成立；附 P1 ×1（测试计数 44→113）与 P2 ×3（摆放函数调用前提、重叠射程、`paddr` 措辞） | 已修；调用前提已写进 §4.2 |
