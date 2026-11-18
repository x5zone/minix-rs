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
| CLAIM-04 | 反证成功：aarch64 去 `isb` 的失败推演不可复现 | 已修 §4.3 其二与 §4.3 `isb` 说明 |
| CLAIM-05 | 主张成立；附 P2 ×2（`FallbackUnsupported` 未入表、aarch64 `KERN_SIZE` 无 L9） | `FallbackUnsupported` 已补；`KERN_SIZE` 对账转 D02-F11 |
| CLAIM-06 | 主张成立；附 P1 ×1（测试计数 44→113）与 P2 ×3（摆放函数调用前提、重叠射程、`paddr` 措辞） | 已修；调用前提已写进 §4.2 |

## R1 完整 survey（V3.3）

> 4 角色 33 条原始发现（FACT 14、DESIGN-D2 15、READER-ABC 2、REFUTE 2 条 P2 附加；REFUTE 另有 4 条反证失败主张）。
> P0 = 2（stale 符号/签名）、P1 = 8（测试计数、引文、锚点、帧指针、第二份公式）。设计 P2 以文档同步收口，残余代码改进登记设计批。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD1-34 | RD1-CERT-FACT-01 | R1 | fact-cert | P2 | 本轮发现 | ① | 同根因： RD1-05；riscv64 节选残留的 `.kern_virt_base()` 已删，恢复为字段直读 | panel/round1/certify-fact.md:107 | `grep -n "kern_virt_base()" doc` → 0；冷读 CLOSED（certify 回读同根因） |
| RD1-35 | RD1-CERT-FACT-02 | R1 | fact-cert | P2 | 本轮发现 | ① | 同根因： RD1-08；`BootStorageCell`/`BUMP_PTR` 已移出 `.bss` 例子（二者初值非零、落 `.data`），`.bss` 只留 `IMAGE_HEAP` 与内核启动全局 | panel/round1/certify-fact.md:118 | `grep -n "BootStorageCell.*\.bss" doc` → 0；冷读 CLOSED（certify 回读同根因） |
| RD1-36 | RD1-CERT-FACT-03 | R1 | fact-cert | P2 | 本轮发现 | ① | 同根因： RD1-03；§5.4 M7 的“18 测试”改为“22 个中 13 个直接相关” | panel/round1/certify-fact.md:130 | `grep -n "loader 18" doc` → 0；`grep -n "22 个测试中 13 个" doc` → 1；冷读 CLOSED（certify 回读同根因） |
| RD1-37 | RD1-CERT-FACT-04 | R1 | fact-cert | P2 | 本轮发现 | ① | 同根因： RD1-12；`.usermapped` 供料方补 `kernel/usermapped_data.c` 的 8 个对象，§7.2 补该文件 | panel/round1/certify-fact.md:141 | `grep -n "usermapped_data.c" doc` → 1；冷读 CLOSED（certify 回读同根因） |
| RD1-38 | RD1-CERT-READER-01 | R1 | reader-cert | P2 | 本轮发现 | ① | 同根因： RD1-32；§2.2 的两种改名情形拆开写（前缀不匹配=链接期未定义符号；一起改名=产物符号值可检查） | panel/round1/certify-reader.md | `grep -n "链接期报未定义符号" doc` → 1；冷读 CLOSED（certify 回读同根因） |
| RD1-39 | RD1-CERT-READER-02 | R1 | reader-cert | P2 | 本轮发现 | ① | 同根因： RD1-09；§2.3 补边界“未启用帧指针时 Rust 不保证调用链深度可回溯” | panel/round1/certify-reader.md | `grep -n "不保证调用链深度可回溯" doc` → 1；冷读 CLOSED（certify 回读同根因） |
| RD1-40 | RD1-CERT-READER-03 | R1 | reader-cert | P2 | 本轮发现 | ① | 已修：§2.4 的指向 `§3.3、§4.1` 改为“决策见 §3.3，实现见 §4.2 与 §4.4” | panel/round1/certify-reader.md | `grep -n "实现见 §4.2 与 §4.4" doc` → 1；冷读 CLOSED（certify-fact/reader 改动段回读） |
| RD1-41 | RD1-CERT-READER-04 | R1 | reader-cert | P1 | 本轮发现 | ① | 同根因： RD1-15；§3.4 前置条件表把“传入指针”与“kmain 消费的副本”拆成两个对象，失败模式按对象写 | panel/round1/certify-reader.md | `grep -n "跳转传入的指针与" doc` → 1；冷读 CLOSED（certify 回读同根因） |
| RD1-42 | RD1-CERT-READER-05 | R1 | reader-cert | P2 | 本轮发现 | ① | 已修：三处节选头去掉“省略号处”措辞，改为“仅保留签名/asm 块，说明段/注释未摘录” | panel/round1/certify-reader.md | `grep -n "省略号处为" doc` → 0；`grep -n "未摘录" doc` → 3；冷读 CLOSED（certify-fact/reader 改动段回读） |
| RD1-43 | RD1-CERT-READER-06 | R1 | reader-cert | P2 | 本轮发现 | ① | 同根因： RD1-18；§3.4 表行改为“断言只覆盖 kinfo.kern_stack_top；形参由两个调用点保证” | panel/round1/certify-reader.md | `grep -n "断言只覆盖" doc` → 1；冷读 CLOSED（certify 回读同根因） |
| RD1-44 | RD1-CERT-READER-07 | R1 | reader-cert | P1 | 本轮发现 | ① | 已修：§4.4 `place_modules` 的语法断裂句改为“源字节直接拷进 bump 页；不做整页清零，只把文件字节之后的尾部余量清零” | panel/round1/certify-reader.md | `grep -n "不清零整页之外的余量" doc` → 0；`grep -n "不做整页清零" doc` → 1；冷读 CLOSED（certify-fact/reader 改动段回读） |
| RD1-45 | RD1-CERT-READER-08 | R1 | reader-cert | P2 | 本轮发现 | ① | 已修：两处“模块文档”改为“`bootface.rs` 的模块文档” | panel/round1/certify-reader.md | `grep -n "写在模块文档里" doc` → 0；冷读 CLOSED（certify-fact/reader 改动段回读） |
| RD1-46 | RD1-CERT-READER-09 | R1 | reader-cert | P1 | 本轮发现 | ① | 同根因： RD1-03；M7 的 18 已改为“22 个中 13 个直接相关”，与 §5.1 同口径 | panel/round1/certify-reader.md | `grep -n "loader 18" doc` → 0；冷读 CLOSED（certify 回读同根因） |
| RD1-47 | RD1-CERT-READER-10 | R1 | reader-cert | P2 | 本轮发现 | ① | 同根因： RD1-17；“严格门”点名 strict 档，“保护测试载体”改为 `test-protection-riscv64` | panel/round1/certify-reader.md | `grep -n "test-protection-riscv64" doc` → 1；冷读 CLOSED（certify 回读同根因） |
| RD1-48 | RD1-CERT-READER-11 | R1 | reader-cert | P2 | 本轮发现 | ① | 已修：§3.2 补 `NoLoadSegments` 的测试名 `test_compute_kernel_layout_rejects_image_without_load_segments` | panel/round1/certify-reader.md；os/boot-shim/src/loader.rs:605 | `grep -n "rejects_image_without_load_segments" doc` → 1；冷读 CLOSED（certify-fact/reader 改动段回读） |
| RD1-49 | RD1-CERT-READER-12 | R1 | reader-cert | P2 | 本轮发现 | ① | 已修：§4.3 的 aarch64 `ldr` 说明改为“literal 读取位置随视图，读出的值始终是链接期绝对值”，与 §4.4 低地址进入一致 | panel/round1/certify-reader.md | `grep -n "literal 的读取位置随视图" doc` → 1；冷读 CLOSED（certify-fact/reader 改动段回读） |

|---|---|---|---|---|---|---|---|---|---|
| RD1-01 | RD1-FACT-01 | R1 | fact | P0 | 本轮发现 | ① | 已修：两处 `load_boot_modules_with_loader` → `load_boot_modules_into`（§3.2、§5.1） | panel/round1/fact.md:27；os/boot-shim/src/loader.rs:422 | `grep -n "load_boot_modules_with_loader" doc` → 0；anchor `57/57`、zero-def=0 |
| RD1-02 | RD1-FACT-02 | R1 | fact | P0 | 本轮发现 | ① | 已修：§4.4 `build_kernel_info` 节选改为现行 11 实参（补 sources/startup/authority，删旧 `PhysBytes(0), 0`），头注同步 | panel/round1/fact.md:40；os/kernel-image/src/bootface.rs:427-439；os/boot-shim/src/lib.rs:101-115 | `grep -n "PhysBytes(0)" doc` → 0；citation failed=0 |
| RD1-03 | RD1-FACT-03 | R1 | fact | P1 | 本轮发现 | ① | 已修：loader 测试 18→22、boot-shim --lib 113→125、M7 同步；复算命令改 `grep -c '#\[test\]'` | panel/round1/fact.md:53；os/boot-shim/src/loader.rs；cargo test -- --list | `grep -n "18 个测试" doc` → 0；`grep -n "125 个测试" doc` → 1 |
| RD1-04 | RD1-FACT-04 | R1 | fact | P1 | 本轮发现 | ① | 已修：两处“五个 first_overlapping_pair_*”→“六个”，补 `ignores_zero_length`，相关计数 12→13 | panel/round1/fact.md:66；os/boot-shim/src/loader.rs:874-970 | `grep -n "五个 `first_overlapping_pair" doc` → 0；`grep -n "六个 `first_overlapping_pair" doc` → 2 |
| RD1-05 | RD1-FACT-05 | R1 | fact | P1 | 本轮发现 | ① | 已修：riscv64 节选 `kinfo.kern_virt_base()/kern_phys_base()` → 字段直读 `.0` | panel/round1/fact.md:79；os/kernel/src/arch/riscv64/higher_half.rs:107-110 | `grep -n "kern_virt_base()" doc` → 0；`grep -n "kern_virt_base.0" doc` → 1 |
| RD1-06 | RD1-FACT-06 | R1 | fact | P1 | 本轮发现 | ① | 已修：`load_segments_into_phys_memory` 块头改“节选”，补 `...` 省源/目标重叠段；正文“三件事”→“四件事”并补 `first_source_destination_overlap` 的 panic 不对称 | panel/round1/fact.md:94；os/boot-shim/src/loader.rs:286-316 | `grep -n "代码引用（保留函数体）" doc` → 0；`grep -n "四件事按顺序" doc` → 1 |
| RD1-07 | RD1-FACT-07 | R1 | fact | P1 | 本轮发现 | ① | 已修：`place_modules` 锚点改 `bootface_common.rs`，§7.3 清单补该文件 | panel/round1/fact.md:107；os/kernel-image/src/bootface_common.rs:177 | `grep -n "bootface.rs:fn place_modules" doc` → 0；doc-code-map 28/28 |
| RD1-08 | RD1-FACT-08 | R1 | fact | P1 | 本轮发现 | ① | 已修：删不存在的 `MODULE_TABLE`/`PLATFORM_SOURCE` 例子，改 `IMAGE_HEAP` + 两个 `BootStorageCell`；写明 `BUMP_PTR` 在 `.data` | panel/round1/fact.md:120；os/kernel-image/src/bootface*.rs；main.rs:521 | `grep -n "MODULE_TABLE" doc` → 0；`grep -n "BUMP_PTR" doc` → 1 |
| RD1-09 | RD1-FACT-09 | R1 | fact | P1 | 本轮发现 | ① | 已修：§2.3/§4.3/§4.3 其三三处帧指针叙述改为“C 的 EBP 由 callee-saved 恢复为 0；Rust 不保存 rbp，链从 0 起”；§5.4 M11 与 §4.4 同步 | panel/round1/fact.md:134；panel/round1/refute.md CLAIM05；os/target/.../kernel 反汇编 | `grep -n "pre_init` 的帧指针" doc` → 0；`grep -n "不保存 `%rbp`" doc` → 1 |
| RD1-10 | RD1-FACT-10 | R1 | fact | P1 | 本轮发现 | ① | 已修：§3.3 把“没有第二份实现”限定为两族 boot-shim 之间，补 v1 `BootStorage::report()` 的第二处公式并注明锁步 | panel/round1/fact.md:147；os/libs/minix-boot/src/boot_storage.rs:356-380 | `grep -n "没有第二份实现" doc` → 0；`grep -n "BootStorage::report()` 里另有同一公式" doc` → 1 |
| RD1-11 | RD1-FACT-11 | R1 | fact | P2 | 本轮发现 | ① | 已修：`_MINIX_MAGIC` 补定义条件（`MKMAGIC=yes`/`MKASR=yes`，默认 no） | panel/round1/fact.md:160；minix3/share/mk/sys.mk:25-29 | `grep -n "MKMAGIC=yes" doc` → 1 |
| RD1-12 | RD1-FACT-12 | R1 | fact | P2 | 本轮发现 | ① | 已修：`.usermapped` 供料方拆开——`usermapped_glo_ipc.S` 供 `.usermapped_glo`，`usermapped_data_arch.c` 三个 `minix_ipcvecs_*` 供 `.usermapped` | panel/round1/fact.md:173；usermapped_glo_ipc.S:10；usermapped_data_arch.c | `grep -n "供料方是.*与" doc` → 0；`grep -n "供 .usermapped_glo" doc` → 1 |
| RD1-13 | RD1-FACT-13 | R1 | fact | P2 | 本轮发现 | ① | 已修：aarch64 `isb` 注释按源码逐字改为 `defensive barrier; enable() already synchronized TTBR/SCTLR` | panel/round1/fact.md:186；os/kernel/src/arch/aarch64/higher_half.rs:47 | `grep -n "drain write buffer before br" doc` → 0；`grep -n "defensive barrier" doc` → 1 |
| RD1-14 | RD1-FACT-14 | R1 | fact | P2 | 本轮发现 | ① | 已修：两处载体形态格改为“载体单地址（工具链默认布局）”，并注明 x86/aarch64 载体没有 `link.ld` | panel/round1/fact.md:199；os/qemu-tests/test-kernels/kernel/bootstrap | `grep -n "载体单地址脚本" doc` → 0 |
| RD1-15 | RD1-DESIGN-D2-01 | R1 | design-d2 | P2 | 本轮发现 | ① | 已修文档：§4.4 改为“形参按 ABI 传入，但生产 kmain 第一条语句遮蔽为 KERNEL_INFO 副本，形参此后无读者”，并补第二次根因；移除 shadow 的代码改动登记设计批 | panel/round1/design-d2.md:53；os/kernel/src/lib.rs:1671-1739 | `grep -n "指针本身仍按 ABI" doc` → 0；`grep -n "形参此后无读者" doc` → 1；冷读 CLOSED（certify-fact/reader 改动段回读） |
| RD1-16 | RD1-DESIGN-D2-02 | R1 | design-d2 | P2 | 本轮发现 | ① | 同根因： RD1-10；同一根因（kern_stack_top 第二份实现）已在 RD1-10 处置，文档已限定公式来源 | panel/round1/design-d2.md:67 | dup-of RD1-10（不单独计根因）；冷读 CLOSED（certify 回读同根因） |
| RD1-17 | RD1-DESIGN-D2-03 | R1 | design-d2 | P2 | 本轮发现 | ① | 已修文档：§3.4 补 `syscall_entry` 的生产读者为零（LSTAR 用 `syscall_entry_va()`，仅 riscv64 保护测试载体读），线协议 v2 删除登记设计批 | panel/round1/design-d2.md:81；os/libs/minix-boot/src/kernel_info.rs:109-117 | `grep -n "syscall_entry.*reference-only\；冷读 CLOSED（certify-fact/reader 改动段回读） |生产路径没有读者" doc` → 1 |
| RD1-18 | RD1-DESIGN-D2-04 | R1 | design-d2 | P2 | 本轮发现 | ① | 已修文档：§3.4 前置条件表写“两个调用点都传 kinfo.kern_stack_top”，并注明参数是可表示冗余、并回 kinfo 属设计批 | panel/round1/design-d2.md:95；os/kernel/src/lib.rs:232/375 | `grep -n "两个生产调用点都传" doc` → 1；冷读 CLOSED（certify-fact/reader 改动段回读） |
| RD1-19 | RD1-DESIGN-D2-05 | R1 | design-d2 | P2 | 本轮发现 | ① | 已修文档：§3.2 在 `continue` 段落后标注“静默降级/已知设计缺口（D2-05），引导路径应 fail-closed” | panel/round1/design-d2.md:109；os/libs/minix-elf/src/lib.rs:330-390 | `grep -n "已知设计缺口（设计批 D2-05）" doc` → 1；冷读 CLOSED（certify-fact/reader 改动段回读） |
| RD1-20 | RD1-DESIGN-D2-06 | R1 | design-d2 | P2 | 本轮发现 | ① | 已修文档：§3.2 补 `InvalidSegment` 归一化丢现场、源/目标重叠走 panic，错误分层登记设计批 D2-06 | panel/round1/design-d2.md:123；os/boot-shim/src/loader.rs:175-341 | `grep -n "错误类型分层属于设计批 D2-06" doc` → 1；冷读 CLOSED（certify-fact/reader 改动段回读） |
| RD1-21 | RD1-DESIGN-D2-07 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：`jump_to_kmain` 一次性令牌；已登记 suggestions.md | panel/round1/design-d2.md:137 | 不适用（建议级） |
| RD1-22 | RD1-DESIGN-D2-08 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：HigherHalf trait 的 testability 声明与 Mock 测试不符；已登记 suggestions.md | panel/round1/design-d2.md:151 | 不适用（建议级） |
| RD1-23 | RD1-DESIGN-D2-09 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：`let _ = virtual_end` 与两处小残留；已登记 suggestions.md | panel/round1/design-d2.md:165 | 不适用（建议级） |
| RD1-24 | RD1-DESIGN-D2-10 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：三个 arch_boot 壳可合一；已登记 suggestions.md | panel/round1/design-d2.md:179 | 不适用（建议级） |
| RD1-25 | RD1-DESIGN-D2-11 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：三份 arch link.ld 的 lockstep 读者与文档“无生产读者”措辞；已登记 suggestions.md | panel/round1/design-d2.md:193 | 不适用（建议级） |
| RD1-26 | RD1-DESIGN-D2-12 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：模块身份位置+名字双表达；已登记 suggestions.md | panel/round1/design-d2.md:207 | 不适用（建议级） |
| RD1-27 | RD1-DESIGN-D2-15 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：ValidationLevel 由调用者选档；已登记 suggestions.md | panel/round1/design-d2.md:252 | 不适用（建议级） |
| RD1-28 | RD1-DESIGN-D2-13 | R1 | design-d2 | P2 | 本轮发现 | ① | 已修文档：§4.4 补 x86_64 两块临时取证打印（`arch_boot arg=`/`preA rdi=`），标注不属于正式路标；删除/门控登记设计批 | panel/round1/design-d2.md:221；os/kernel/src/lib.rs:246-276/1681-1715 | `grep -n "preA rdi=" doc` → 1；冷读 CLOSED（certify-fact/reader 改动段回读） |
| RD1-29 | RD1-DESIGN-D2-14 | R1 | design-d2 | P2 | 本轮发现 | ① | 同根因： RD1-02/RD1-05/RD1-06；三处引用已分别在同一批修复（build_kernel_info 11 参、字段直读、重叠 panic 段） | panel/round1/design-d2.md:235 | dup-of RD1-02/RD1-05/RD1-06（不单独计根因）；冷读 CLOSED（certify 回读同根因） |
| RD1-30 | RD1-READER-A-01 | R1 | reader-a | P2 | 本轮发现 | ① | 同根因： RD1-09；帧指针矛盾已在 RD1-09 修复，§2.3/§4.3 与 §5.4 M11 同口径 | panel/round1/reader-abc.md（A-01） | 冷读 CLOSED（certify-reader 改动段回读） |
| RD1-31 | RD1-READER-C-01 | R1 | reader-c | P2 | 本轮发现 | ① | 同根因： RD1-09；§5.4 M11 已改为“形参按 ABI 传入但被遮蔽为 .bss 副本”，与 §4.4 一致 | panel/round1/reader-abc.md（C-01） | 冷读 CLOSED（certify-reader 改动段回读） |

| RD1-50 | CLAIM-01 | R1 | refute | P2 | 本轮发现 | ① | 同根因：§2.2 反方向推演不成立，已删（见 RD1-32） | panel/round1/refute.md | 冷读 CLOSED（certify-fact 回读 §2.2 推演段） |
| RD1-51 | CLAIM-02 | R1 | refute | P2 | 本轮发现 | FP | 反证失败（主张成立，不是缺陷）：链接脚本形态与 p_paddr 判据经 `AT(` 计数复核成立 | panel/round1/refute.md:44 | `grep -n "AT(" doc` → 命中（主张成立，无缺陷） |
| RD1-52 | CLAIM-03 | R1 | refute | P2 | 本轮发现 | ① | 同根因：§3.3 与 §3.5 矛盾，已修（见 RD1-33） | panel/round1/refute.md | 冷读 CLOSED（certify-fact 回读 §3.3/§3.5 段） |
| RD1-53 | CLAIM-04 | R1 | refute | P2 | 本轮发现 | FP | 反证失败（主张成立，不是缺陷）：HigherHalf 契约与三架构跳转经源码复核成立 | panel/round1/refute.md:102 | `grep -n "HigherHalf" doc` → 命中（主张成立，无缺陷） |
| RD1-54 | CLAIM-05 | R1 | refute | P1 | 本轮发现 | ① | 同根因：帧指针事实错误，并入 RD1-09 | panel/round1/refute.md | 冷读 CLOSED（certify-fact 回读帧指针三段） |
| RD1-55 | CLAIM-06 | R1 | refute | P1 | 本轮发现 | ① | 同根因：文档-代码同步，并入 RD1-01..RD1-08、RD1-10 | panel/round1/refute.md | 冷读 CLOSED（certify-fact 回读同步项） |
## P-REFUTE 附加

| claim ID | 结果 | 处置 |
|---|---|---|
| CLAIM-01 | 反证失败（主张成立；附 §2.2 反方向推演不成立） | 推演已删，见 RD1-32 |
| CLAIM-02 | 反证失败（主张成立） | 无缺陷 |
| CLAIM-03 | 反证失败（主张成立；附 §3.3/§3.5 矛盾） | 矛盾已修，见 RD1-33 |
| CLAIM-04 | 反证失败（主张成立） | 无缺陷 |
| CLAIM-05 | 阻断（P1，帧指针事实错误） | 并入 RD1-09 |
| CLAIM-06 | 阻断（P1，文档-代码同步） | 并入 RD1-01..RD1-08、RD1-10、RD1-29 |
| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|

## R2 确认轮（V3.3 四态模型）

> 6 角色 21 条 FINDING（FACT-A 3、FACT-B 1、DESIGN-D2 9、READER-A 1、READER-B 1、READER-C 6）。
> P0 = 0；P1 = 3（C 装载者模型、LMA 重叠报错、D-045 注释改写）；P2 = 11；P3 = 7。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD2-01 | RD2-FACT-A-01 | R2 | fact-a | P1 | 本轮发现 | ① | 已修：§1.2/§4.2 补射程声明——树内 boot monitor 走 `LOADADDR(p_vaddr)` 低 27 位，`p_paddr` 只对外部 multiboot 装载器（如 GRUB）成立；§2.1 反事实与 §6.1 诊断按外部装载器限定 | panel/round2/fact-a.md:28；minix3/sys/arch/x86/include/loadfile_machdep.h:40；loadfile_elf32.c:336/363 | `grep -n "射程声明" doc` → 1；`grep -n "外部 multiboot 装载器" doc` → 2 |
| RD2-02 | RD2-FACT-A-02 | R2 | fact-a | P1 | 本轮发现 | ① | 已修：§2.1 改为“现行 GNU ld 对 LMA 重叠报错并拒绝产出；旧工具链才可能覆盖”，`.bss` 改为 `.data` 所在 RW 段的 NOBITS 尾段 | panel/round2/fact-a.md:46；GNU ld 等价脚本实测 | `grep -n "链接器不会为此报错" doc` → 0；`grep -n "现行 GNU ld 对 LMA 重叠报错" doc` → 1 |
| RD2-03 | RD2-FACT-A-03 | R2 | fact-a | P2 | 本轮发现 | ① | 已修：§2.2 的 `pg_utils.o` 反事实改为链接期未定义符号；删 `pre_init.o` 括注并说明需同时移调用方才可能落高半 | panel/round2/fact-a.md:62；objcopy 实测 | `grep -n "函数会落进高一侧的\\\.text" doc` → 0；`grep -n "链接期会得到" doc` → 1 |
| RD2-04 | RD2-FACT-B-01 | R2 | fact-b | P2 | 本轮发现 | ① | 已修：§3.4 的 `syscall_entry` 句收窄——生产不把它当入口消费者（入口用 `syscall_entry_va()`），但 validate/postconditions/write debug_assert 仍读取 | panel/round2/fact-b.md；os/kernel/src/lib.rs、handoff_v1_write.rs | `grep -n "生产路径没有把它当 SYSCALL 入口的消费者" doc` → 1 |
| RD2-05 | RD2-DESIGN-D2-01 | R2 | design-d2 | P2 | 本轮发现 | ① | 已修文档：§3.2 改为“next() 的过滤会 continue；new/parse_phdr 的未检查加法畸形输入会 panic”，代码修复登记设计批 D2-01 | panel/round2/design-d2.md:118；minix-elf/src/lib.rs:333/250 | `grep -n "畸形输入会 panic" doc` → 1；冷读 CLOSED（certify-fact 回读） |
| RD2-06 | RD2-DESIGN-D2-02 | R2 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3）：entry_point 无独立选择权；已登记 suggestions.md | panel/round2/design-d2.md:132 | 不适用（建议级） |
| RD2-07 | RD2-DESIGN-D2-03 | R2 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3）：字段去向表称“列全/四种”与实际两消费者不符；已登记 suggestions.md | panel/round2/design-d2.md:146 | 不适用（建议级） |
| RD2-08 | RD2-DESIGN-D2-04 | R2 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3）：bootface 节选省略说明漏 validate_requiring_sources；已登记 suggestions.md | panel/round2/design-d2.md:160 | 不适用（建议级） |
| RD2-09 | RD2-DESIGN-D2-05 | R2 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3）：三份 arch link.ld 无生产读者却带假同步；已登记 suggestions.md | panel/round2/design-d2.md:174 | 不适用（建议级） |
| RD2-10 | RD2-DESIGN-D2-06 | R2 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3）：push 0 + sub rsp,8 可合并；已登记 suggestions.md | panel/round2/design-d2.md:188 | 不适用（建议级） |
| RD2-11 | RD2-DESIGN-D2-07 | R2 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3）：MockHigherHalf 只写不读；已登记 suggestions.md | panel/round2/design-d2.md:202 | 不适用（建议级） |
| RD2-12 | RD2-DESIGN-D2-08 | R2 | design-d2 | P1 | 本轮发现 | ① | 已修文档：四处节选头的“注释有删改”改为“注释有改写”，显式声明改写（D-045 豁免形态） | panel/round2/design-d2.md:216 | `grep -n "注释有删改" doc` → 0；`grep -c "注释有改写" doc` → 4；冷读 CLOSED（certify-fact 回读） |
| RD2-13 | RD2-DESIGN-D2-09 | R2 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3）：状态表用 qemu_test 的 kmain_verify 背书；已登记 suggestions.md | panel/round2/design-d2.md:230 | 不适用（建议级） |
| RD2-14 | RD2-READER-A-01 | R2 | reader-a | P2 | 本轮发现 | ① | 已修：§4.1 的 “boot_validate_and_prepare 走进 panic” 改为规划层 plan_kernel_mapping 在 arch_boot 里 panic | panel/round2/reader-a.md | `grep -n "走进 panic 分支" doc` → 0；`grep -n "规划层 `plan_kernel_mapping` 在 `arch_boot` 里 panic" doc` → 1；冷读 CLOSED（certify-reader #2 回读） |
| RD2-15 | RD2-READER-B-01 | R2 | reader-b | P2 | 本轮发现 | ① | 已修：§前置知识改为按腿分列的起点（v1/riscv64 自引导/aarch64 direct-core/inline 载体），不再用单一句覆盖全部路径 | panel/round2/reader-b.md | `grep -n "Rust 生产路径的起点按腿不同" doc` → 1；冷读 CLOSED（certify-reader #2 回读） |
| RD2-16 | RD2-READER-C-01 | R2 | reader-c | P2 | 本轮发现 | ① | 已修：§2.5 的落地结论限定为 i386 已证、ARM 侧证据待补；§1.1/§2.5 的通则限定为带 AT() 的普通段 | panel/round2/reader-c.md | `grep -n "ARM 侧按同一判据、改名名单与" doc` → 1；冷读 CLOSED（certify-reader #2 回读） |
| RD2-17 | RD2-READER-C-02 | R2 | reader-c | P2 | 本轮发现 | ① | 已修：§2.3 “还没有到位”改为“不是这两条指令建立的；由 pre_init 五步建表在此之前完成” | panel/round2/reader-c.md | `grep -n "此时还没有到位" doc` → 0；冷读 CLOSED（certify-reader #2 回读） |
| RD2-18 | RD2-READER-C-03 | R2 | reader-c | P2 | 本轮发现 | ① | 已修：§2.3 “只切栈不跳”的反证改为停在 hang 循环、内核不前进；删除不可达的延迟页错误 | panel/round2/reader-c.md | `grep -n "坏点推迟到恒等映射被移除" doc` → 0；`grep -n "直接症状是停机" doc` → 1；冷读 CLOSED（certify-reader #2 回读） |
| RD2-19 | RD2-READER-C-04 | R2 | reader-c | P2 | 本轮发现 | ① | 已修：§4.2 重叠反证区分“段间覆盖=数据损坏”与“源/目标重叠=copy_from_nonoverlapping 的 UB” | panel/round2/reader-c.md | `grep -n "后一个段覆盖前一个段的字节是数据损坏" doc` → 1；冷读 CLOSED（certify-reader #2 回读） |
| RD2-20 | RD2-READER-C-05 | R2 | reader-c | P2 | 本轮发现 | ① | 已修：§5.3 “任一处单独改动”限定为 L9 覆盖的 riscv64 分支，aarch64 只有上界断言 | panel/round2/reader-c.md | `grep -n "在 L9 覆盖的 riscv64 分支上" doc` → 1；冷读 CLOSED（certify-reader #2 回读） |
| RD2-21 | RD2-READER-C-06 | R2 | reader-c | P2 | 本轮发现 | ① | 已修：§2.5 表格把 ARM32 结论限定为“按同一判据、证据待补” | panel/round2/reader-c.md | `grep -n "ARM 侧按同一判据" doc` → 1；冷读 CLOSED（certify-reader #2 回读） |
| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD2-22 | RD2-CERT2-FACT-01 | R2 | fact-cert | P2 | 本轮发现 | ① | 已修：§2.2 的 `__k_unpaged_*` 清单补 `vm_enable_paging`，与 `pre_init.c:230-234` 五调用一致 | panel/round2/certify2-fact.md:54 | `grep -n "vm_enable_paging" doc` → 命中；最终认证 CLOSED |
| RD2-23 | RD2-CERT2-FACT-02 | R2 | fact-cert | P2 | 本轮发现 | ① | 已修：§前置知识改“Rust 侧各启动腿与测试载体的起点按腿不同（测试载体不是生产路径）” | panel/round2/certify2-fact.md:64 | `grep -n "测试载体不是生产路径" doc` → 1；最终认证 CLOSED |
| RD2-24 | RD2-CERT-READER-01 | R2 | reader-cert | P1 | 本轮发现 | ① | 已修：§前置知识补 riscv64 测试载体（物理视图、PC 高半 VMA），并把“两条路径”改为“各腿” | panel/round2/certify-reader.md:73 | `grep -n "riscv64 测试载体物理视图" doc` → 1；`grep -n "两条路径的起点不同" doc` → 0；冷读 CLOSED（最终回读） |
| RD2-25 | RD2-CERT-READER-02 | R2 | reader-cert | P1 | 本轮发现 | ① | 已修：§2.3 “其二”的“上一处坏在取指”改为“上一处是停在 hang 停机”，与“其一”一致 | panel/round2/certify-reader.md:85 | `grep -n "上一处坏在取指" doc` → 0；`grep -n "上一处是停在 hang" doc` → 1；冷读 CLOSED（最终回读） |
| RD2-26 | RD2-CERT-READER-03 | R2 | reader-cert | P2 | 本轮发现 | ① | 已修：§2.1 `.bss` 清零起点改为“`.data` 文件字节之后；独立成段才从段首起” | panel/round2/certify-reader.md:95 | `grep -n "清零范围同样从" doc` → 0；`grep -n "若 `.bss` 独立成段才从段首" doc` → 1；冷读 CLOSED（最终回读） |
| RD2-27 | RD2-CERT-READER-04 | R2 | reader-cert | P2 | 本轮发现 | ① | 已修：§2.2 删除悬空的“括注”回指，改为“若想用 `pre_init.o` 做同型替换…” | panel/round2/certify-reader.md:105 | `grep -n "括注" doc` → 0；`grep -n "若想用 `pre_init.o`" doc` → 1；冷读 CLOSED（最终回读） |
| RD2-28 | RD2-CERT2-READER-01 | R2 | reader-cert | P2 | 本轮发现 | ① | 已修：同上，`pre_init.o` 半句补主语与完整因果 | panel/round2/certify2-reader.md:37 | `grep -n "若想用 `pre_init.o` 做同型替换" doc` → 1；冷读 CLOSED（最终回读） |
| RD2-29 | RD2-CERT3-01 | R2 | fact-cert | P2 | 本轮发现 | ① | 已修：§2.2 的未定义符号清单补 `add_memmap`/`cut_memmap`，并加“至少”限定 | panel/round2/certify3.md:89 | `grep -n "__k_unpaged_add_memmap" doc` → 1；最终认证 CLOSED |


## REG-2026-11-18 回归轮补账（r6 R6-b：回归轮入账；面板 tmp/review-eval/01-stage-kernel/02-higher-half-kernel/panel/regress/）

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD2R-FACT | RD2R-FACT-01、02、03 | REG-2026-11-18 | P-FACT | P3 | 事实 | ① | 已修（回归修复批） | `fe49cd804`+`afdd867b7`（批次级；逐条-提交映射未入库） | FREEZE 回归节：anchor 55/55、structure/ref/style 绿 |
| RD2R-READER-A | RD2R-READER-A-01、02（P1：前置块塞五腿细节、OpenSBI/shim 混淆） | REG-2026-11-18 | P-READER-A | P1 | 读者 | ① | 已修（合并读者漏报、独立 A 抓到——D-055 必要性判例） | `fe49cd804` | FREEZE 回归节 |
| RD2R-READER-B | RD2R-READER-B-01..06 | REG-2026-11-18 | P-READER-B | P2/P3 | 读者 | ① | 已修（批次级） | `fe49cd804` | 同上 |
| RD2R-READER-C | RD2R-READER-C-01 | REG-2026-11-18 | P-READER-C | P2 | 读者 | ① | 已修 | `afdd867b7`（PC 行枚举补 aarch64 direct-core） | `git show afdd867b7` |
| RD2R-CERT | RD2R-CERT-01、02（certify 抓到 3 REOPEN + 2 新 P2 的残项） | REG-2026-11-18 | certify | P2 | 同步 | ① | 补修完成；最终独立确认 3 PASS / 1 FAIL（FAIL 为 certify 误用 D03 格式，非缺陷） | `final-verify.md`；`afdd867b7` | 面板 final-verify.md |
| RD2R-DESIGN | design-d2.md 头 FINDING 1 条（machine_type 零读者）+「翻译残留六类」清单（FREEZE 口径 9 条） | REG-2026-11-18 | P-DESIGN-D2 | P2/P3 | 设计 | ① | P2 已修；P3/SUGG 部分入 suggestions（批次级证据） | `fe49cd804`；`panel/regress/design-d2.md` | FREEZE 回归节 |
| RD2R-DESIGN-ARCH | design-d2.md 的 ARCH-PROPOSAL 1 条 | REG-2026-11-18 | P-DESIGN-D2 | — | 设计提案 | SUGG | 待用户亲裁（挂账，与 D-050/051/052 同类）；不阻断 | `panel/regress/design-d2.md`；DECISIONS.md | — |
| RD2R-SUGG | FACT S1/S2、READER C-S1..S6、A/B/FACT 聚合 SUGG 段 | REG-2026-11-18 | 各角色 | — | 建议 | SUGG | 入 suggestions，不阻断 | `panel/regress/*.md` | — |
