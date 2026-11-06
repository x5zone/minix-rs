# D03 缺陷与处置台账

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| DESIGN-01 | design-d2 | P1 | 设计/NMI IST 语义 | ② | 转出：follow-ups.md D03-F01（NMI 配 IST1 改变嵌套语义；需 [ARCH] 标注或改为 C 的嵌套行为） | panel/design-d2.md DESIGN-01 | 不适用（转出） |
| DESIGN-02 | design-d2 | P1 | 代码/CPU 容量不一致 | ② | 转出：follow-ups.md D03-F02（kernel MAX_CPUS=32 vs x86 TSS/GS/AP 栈=8；补编译期关联或统一容量） | panel/design-d2.md DESIGN-02 | 不适用（转出） |
| DESIGN-03 | design-d2 | P2 | 设计/DPL 双真相源 | ② | 转出：follow-ups.md D03-F03（`init` 的 DPL 与 `user_accessible` 两处来源，安装后只后者生效） | panel/design-d2.md DESIGN-03 | 不适用（转出） |
| DESIGN-04 | design-d2 | P1 | 设计/load 可重入 | ② | 转出：follow-ups.md D03-F04（`load()` 安全 `&self` 可重入，第二次 `ltr` 会 #GP；类型上标一次或加断言） | panel/design-d2.md DESIGN-04 | 不适用（转出） |
| DESIGN-05 | design-d2 | P2 | 设计/IST 字段不一致 | ② | 转出：follow-ups.md D03-F05（门的 IST 字段与 TSS.ist 槽的对应只对 2/8 恢复，其余向量可能失配） | panel/design-d2.md DESIGN-05 | 不适用（转出） |
| DESIGN-06 | design-d2 | P2 | 设计/哨兵零值 | ② | 转出：follow-ups.md D03-F06（`kernel_stack_top == 0` 同时表示“无”和“合法零值”，改为 Option） | panel/design-d2.md DESIGN-06 | 不适用（转出） |
| DESIGN-07 | design-d2 | P2 | 设计/cpu_count 只写不读 | ② | 转出：follow-ups.md D03-F07（`cpu_count`、`boot_cpu_id` 与 BSP 选择面的职责重叠） | panel/design-d2.md DESIGN-07 | 不适用（转出） |
| DESIGN-08 | design-d2 | P2 | 设计/共享栈可表示 | ② | 转出：follow-ups.md D03-F07（多个 CPU 共用同一内核栈的可表示状态） | panel/design-d2.md DESIGN-08 | 不适用（转出） |
| DESIGN-09 | design-d2 | P2 | 代码/IST 栈对齐 | ② | 转出：follow-ups.md D03-F08（`[[u8; 0x2000]; 8]; 2]` 的对齐只有 1，交给硬件的 RSP 不保证 16 对齐） | panel/design-d2.md DESIGN-09 | 不适用（转出） |
| DESIGN-10 | design-d2 | P0 | 文档/旧 memmap 链 | ① | 已修：§4.1 改为 Phase A.2 为空、无 FREE_MEMMAP、扣除在 `vm_handoff::classify` | panel/design-d2.md DESIGN-10 | L4 文档五门重跑 |
| DESIGN-11 | design-d2 | P1 | 文档/STUB_VECTORS 作用 | ① | 已修：§4.4 明确布尔值只被宿主测试使用，汇编由字面量实例化 | panel/design-d2.md DESIGN-11 | L4 文档五门重跑 |
| DESIGN-12 | design-d2 | P2 | 设计/is_trap 布尔 | ② | 转出：follow-ups.md D03-F09（门类型 is_trap 的双来源） | panel/design-d2.md DESIGN-12 | 不适用（转出） |
| DESIGN-13 | design-d2 | P2 | 设计/特权级死轴 | ② | 转出：follow-ups.md D03-F10（`Privilege`/`PrivilegeLevel` 无生产读者） | panel/design-d2.md DESIGN-13 | 不适用（转出） |
| DESIGN-14 | design-d2 | P2 | 设计/load_ap 三实现同体 | ② | 转出：follow-ups.md D03-F10（`TrapEntryArch::load_ap` 三个实现全是 `self.load()`） | panel/design-d2.md DESIGN-14 | 不适用（转出） |
| DESIGN-15 | design-d2 | P2 | 设计/cpu_count 冗余 | ② | 转出：follow-ups.md D03-F07（保护结构的 `cpu_count` 只写不读） | panel/design-d2.md DESIGN-15 | 不适用（转出） |
| DESIGN-16 | design-d2 | P3 | 代码/冗余 unsafe impl | ② | 转出：follow-ups.md D03-F10（`X86_64Protection` 的 `unsafe impl Send/Sync` 可删） | panel/design-d2.md DESIGN-16 | 不适用（转出） |
| DESIGN-17 | design-d2 | P3 | 设计/入口基址契约 | ② | 转出：follow-ups.md D03-F10（`program_entry_base` 的 doc 与默认实现不一致） | panel/design-d2.md DESIGN-17 | 不适用（转出） |
| DESIGN-18 | design-d2 | P3 | 设计/文档契约 | ② | 转出：follow-ups.md D03-F10（HigherHalf 统一调用点理由、DIVIDE 等三处低危契约项） | panel/design-d2.md DESIGN-18 | 不适用（转出） |
| DESIGN-19 | design-d2 | P1 | 测试/装载次序无断言 | ② | 转出：follow-ups.md D03-F11（lgdt→ltr、GS 搬迁、load 次序无宿主/真机断言） | panel/design-d2.md DESIGN-19 | 不适用（转出） |
| FACT-01 | fact | P0 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-01 | L4 文档五门重跑 |
| FACT-02 | fact | P0 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-02 | L4 文档五门重跑 |
| FACT-03 | fact | P0 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-03 | L4 文档五门重跑 |
| FACT-04 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-04 | L4 文档五门重跑 |
| FACT-05 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-05 | L4 文档五门重跑 |
| FACT-06 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-06 | L4 文档五门重跑 |
| FACT-07 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-07 | L4 文档五门重跑 |
| FACT-08 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-08 | L4 文档五门重跑 |
| FACT-09 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-09 | L4 文档五门重跑 |
| FACT-10 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-10 | L4 文档五门重跑 |
| FACT-11 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-11 | L4 文档五门重跑 |
| FACT-12 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-12 | L4 文档五门重跑 |
| FACT-13 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-13 | L4 文档五门重跑 |
| FACT-14 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-14 | L4 文档五门重跑 |
| FACT-15 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-15 | L4 文档五门重跑 |
| FACT-16 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（见 loop.md §4） | panel/fact.md FACT-16 | L4 文档五门重跑 |
| READER-A-01 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12a（reader A-01） | panel/reader-a.md READER-A-01 | 不适用（转出） |
| READER-A-02 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12a（reader A-02） | panel/reader-a.md READER-A-02 | 不适用（转出） |
| READER-A-03 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12a（reader A-03） | panel/reader-a.md READER-A-03 | 不适用（转出） |
| READER-A-04 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12a（reader A-04） | panel/reader-a.md READER-A-04 | 不适用（转出） |
| READER-A-05 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12a（reader A-05） | panel/reader-a.md READER-A-05 | 不适用（转出） |
| READER-A-06 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12a（reader A-06） | panel/reader-a.md READER-A-06 | 不适用（转出） |
| READER-A-07 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12a（reader A-07） | panel/reader-a.md READER-A-07 | 不适用（转出） |
| READER-A-08 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12a（reader A-08） | panel/reader-a.md READER-A-08 | 不适用（转出） |
| READER-A-09 | reader-a | P1 | 文档/跨节引用 | ① | 已修：同 FACT-14（§5.3 记录已同步，无“出入”） | panel/reader-a.md READER-A-09 | L4 文档五门重跑 |
| READER-A-10 | reader-a | P1 | 文档/反事实前提 | ① | 已修：同 FACT-15（EFER.SCE 窗口内无 CPL3 用户态） | panel/reader-a.md READER-A-10 | L4 文档五门重跑 |
| READER-A-11 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12a（reader A-11） | panel/reader-a.md READER-A-11 | 不适用（转出） |
| READER-A-12 | reader-a | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12a（reader A-12） | panel/reader-a.md READER-A-12 | 不适用（转出） |
| READER-B-01 | reader-b | P1 | 文档/因果 | ① | 已修：同 FACT-03（cstart 返回后 kmain 继续） | panel/reader-b.md READER-B-01 | L4 文档五门重跑 |
| READER-B-02 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-02） | panel/reader-b.md READER-B-02 | 不适用（转出） |
| READER-B-03 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-03） | panel/reader-b.md READER-B-03 | 不适用（转出） |
| READER-B-04 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-04） | panel/reader-b.md READER-B-04 | 不适用（转出） |
| READER-B-05 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-05） | panel/reader-b.md READER-B-05 | 不适用（转出） |
| READER-B-06 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-06） | panel/reader-b.md READER-B-06 | 不适用（转出） |
| READER-B-07 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-07） | panel/reader-b.md READER-B-07 | 不适用（转出） |
| READER-B-08 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-08） | panel/reader-b.md READER-B-08 | 不适用（转出） |
| READER-B-09 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-09） | panel/reader-b.md READER-B-09 | 不适用（转出） |
| READER-B-10 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-10） | panel/reader-b.md READER-B-10 | 不适用（转出） |
| READER-B-11 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-11） | panel/reader-b.md READER-B-11 | 不适用（转出） |
| READER-B-12 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-12） | panel/reader-b.md READER-B-12 | 不适用（转出） |
| READER-B-13 | reader-b | P1 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（§2.2 重复论证，未修） | panel/reader-b.md READER-B-13 | 不适用（转出） |
| READER-B-14 | reader-b | P2 | 文档/跨节引用 | ① | 已修：同 FACT-14（§3.6 的“§5.3 有披露”改为已同步） | panel/reader-b.md READER-B-14 | L4 文档五门重跑 |
| READER-B-15 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-15） | panel/reader-b.md READER-B-15 | 不适用（转出） |
| READER-B-16 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-16） | panel/reader-b.md READER-B-16 | 不适用（转出） |
| READER-B-17 | reader-b | P2 | 读者/结构 | ② | 转出：follow-ups.md D03-F12b（reader B-17） | panel/reader-b.md READER-B-17 | 不适用（转出） |
| READER-C-01 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-01） | panel/reader-c.md READER-C-01 | 不适用（转出） |
| READER-C-02 | reader-c | P1 | 文档/跨节引用 | ① | 已修：同 FACT-14 | panel/reader-c.md READER-C-02 | L4 文档五门重跑 |
| READER-C-03 | reader-c | P1 | 文档/控制器状态 | ① | 已修：同 FACT-06/07（Phase B 的 intr.init 编程控制器；Phase F 只放行时钟门） | panel/reader-c.md READER-C-03 | L4 文档五门重跑 |
| READER-C-04 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-04） | panel/reader-c.md READER-C-04 | 不适用（转出） |
| READER-C-05 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-05） | panel/reader-c.md READER-C-05 | 不适用（转出） |
| READER-C-06 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-06） | panel/reader-c.md READER-C-06 | 不适用（转出） |
| READER-C-07 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-07） | panel/reader-c.md READER-C-07 | 不适用（转出） |
| READER-C-08 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-08） | panel/reader-c.md READER-C-08 | 不适用（转出） |
| READER-C-09 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-09） | panel/reader-c.md READER-C-09 | 不适用（转出） |
| READER-C-10 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-10） | panel/reader-c.md READER-C-10 | 不适用（转出） |
| READER-C-11 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-11） | panel/reader-c.md READER-C-11 | 不适用（转出） |
| READER-C-12 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-12） | panel/reader-c.md READER-C-12 | 不适用（转出） |
| READER-C-13 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-13） | panel/reader-c.md READER-C-13 | 不适用（转出） |
| READER-C-14 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-14） | panel/reader-c.md READER-C-14 | 不适用（转出） |
| READER-C-15 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-15） | panel/reader-c.md READER-C-15 | 不适用（转出） |
| READER-C-16 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-16） | panel/reader-c.md READER-C-16 | 不适用（转出） |
| READER-C-17 | reader-c | P2 | 读者/一致性 | ② | 转出：follow-ups.md D03-F12c（reader C-17） | panel/reader-c.md READER-C-17 | 不适用（转出） |

## P-REFUTE 附加（不属于 `## FINDING` 台账）

| 主张 | 结果 | 处置 |
|---|---|---|
| CLAIM 01 | 反证失败，主张成立；附 P2 ×2（LDT 描述符注释过期、DS 访问字节少 ACCESSED 位） | 未改，转 D03-F13 |
| CLAIM 02 | 反证成功：FREE_MEMMAP/Phase A.2 旧实现（P0）、`arch_init` APIC 分支（P1） | 已修 FACT-01/07 |
| CLAIM 03 | 反证失败；1 条 P2 措辞 | 无动作 |
| CLAIM 04 | 反证失败；TSS/IST 布局核实 | 无动作 |
| CLAIM 05 | 机制成立；P1 GsArea 被入口汇编写、P2 APIC 门数与向量号 | P1 转 D03-F13；P2 转 D03-F14 |
| CLAIM 06 | 反证失败；P2 ×2（test-protection 自述矛盾、aarch64 载体注释） | 转 D03-F14 |
