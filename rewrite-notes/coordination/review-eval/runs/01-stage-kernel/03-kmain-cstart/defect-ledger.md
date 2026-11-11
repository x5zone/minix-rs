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

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
## P-REFUTE 附加（不属于 `## FINDING` 台账）

| 主张 | 结果 | 处置 |
|---|---|---|
| CLAIM 01 | 反证失败，主张成立；附 P2 ×2（LDT 描述符注释过期、DS 访问字节少 ACCESSED 位） | 未改，转 D03-F13 |
| CLAIM 02 | 反证成功：FREE_MEMMAP/Phase A.2 旧实现（P0）、`arch_init` APIC 分支（P1） | 已修 FACT-01/07 |
| CLAIM 03 | 反证失败；1 条 P2 措辞 | 无动作 |
| CLAIM 04 | 反证失败；TSS/IST 布局核实 | 无动作 |
| CLAIM 05 | 机制成立；P1 GsArea 被入口汇编写、P2 APIC 门数与向量号 | P1 转 D03-F13；P2 转 D03-F14 |
| CLAIM 06 | 反证失败；P2 ×2（test-protection 自述矛盾、aarch64 载体注释） | 转 D03-F14 |

## R1 完整 survey（V3.3）

> 4 角色 29 条 FINDING（FACT 8、DESIGN-D2 16、READER 4、REFUTE 1 条 P1 附加；另有 4 条 CLAIM 反证成功，2 条失败）。
> P0 = 6（memcpy 语义、kernel_may_alloc、KERNEL_MAY_ALLOC 已删、Step 1 旧引文、pg_utils 双副本）；P1 = 3；P2 = 12；P3 = 8。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD3-01 | RD3-FACT-01 | R1 | fact | P0 | 本轮发现 | ① | 已修：§2.1 改为“pre_init 的 .unpaged_bss 暂存副本 → main.c 的 .usermapped 最终全局”，删自拷贝措辞；反事实改为最终全局全零、boot module 计数失败 | panel/round1/fact.md；panel/round1/refute.md CLAIM01 | `grep -n "全局自拷贝" doc` → 0；`grep -n "暂存副本 → 最终全局" doc` → 1；冷读 CLOSED（certify） |
| RD3-02 | RD3-FACT-02 | R1 | fact | P0 | 本轮发现 | ① | 已修：§2.1 区分未分页暂存副本（初值 1）与普通 table.c 零初始化副本；`kmain` 置 1 是承重赋值 | panel/round1/fact.md；refute CLAIM02 | `grep -n "把同一句话再说一遍" doc` → 0；`grep -n "承重赋值" doc` → 1；冷读 CLOSED |
| RD3-03 | RD3-FACT-03 | R1 | fact | P0 | 本轮发现 | ① | 已修：§4.1/§4.7 删除 `KERNEL_MAY_ALLOC`/getter，改为“D01-F08 淘汰，引导期写权限由调用图与 FreeRangeSet 保证” | panel/round1/fact.md；panel/round1/design-d2.md D2-01；lib.rs:3916-3919 | `grep -n "KERNEL_MAY_ALLOC" doc` → 0；`grep -n "D01-F08 淘汰" doc` → 1；冷读 CLOSED |
| RD3-04 | RD3-FACT-04 | R1 | fact | P1 | 本轮发现 | ① | 已修：§3.5 Step 1 引文改为现行注释口径，`IDENTITY_MAP_END` 改为 `BOOT_IDENTITY_MAP_END` 重导出 | panel/round1/fact.md；panel/round1/design-d2.md D2-02；lib.rs:1221-1229 | `grep -n "Maps 1024" doc` → 0；`grep -n "重导出" doc` → 1；冷读 CLOSED |
| RD3-05 | RD3-FACT-05 | R1 | fact | P2 | 本轮发现 | ① | 已修：§4.4 诊断读数四个→三个（rdi、全局槽指针、rsp） | panel/round1/fact.md | `grep -n "四个读数" doc` → 0；`grep -n "三个读数" doc` → 1；冷读 CLOSED |
| RD3-06 | RD3-FACT-06 | R1 | fact | P2 | 本轮发现 | ① | 已修：§5 计数 375→374、94/20→95/21；逐文件 56 不动 | panel/round1/fact.md | `grep -n "375 个测试" doc` → 0；`grep -n "374 个测试" doc` → 1；冷读 CLOSED |
| RD3-07 | RD3-FACT-07 | R1 | fact | P3 | 本轮发现 | ① | 已修：§2.6 PWT/PCD 的“最后一块”改为分对齐/未对齐两种情形 | panel/round1/fact.md | `grep -n "最后一个仍含真实内存" doc` → 0；`grep -n "未 4MB 对齐时" doc` → 1；冷读 CLOSED |
| RD3-08 | RD3-FACT-08 | R1 | fact | P0 | 本轮发现 | ① | 已修：§2.6 改为 `pg_utils.c` 两份对象/两份 pagedir，`prot_init` 在普通副本建第二张表并切 CR3；删除原地重填/TLB 旧条目的机制与“冷启动照常跑”反事实 | panel/round1/fact.md；refute CLAIM02 | `grep -n "同一个静态页目录数组" doc` → 0；`grep -n "不是同一数组的原地重填" doc` → 1；冷读 CLOSED |
| RD3-09 | RD3-DESIGN-D2-01 | R1 | design-d2 | P0 | 本轮发现 | ① | 同根因：KERNEL_MAY_ALLOC 已删；并入 RD3-03 | panel/round1/design-d2.md:01 | 冷读 CLOSED（certify 回读）；同 RD3-03 |
| RD3-10 | RD3-DESIGN-D2-02 | R1 | design-d2 | P0 | 本轮发现 | ① | 同根因：Step 1 旧注释；并入 RD3-04 | panel/round1/design-d2.md:02 | 冷读 CLOSED（certify 回读）；同 RD3-04 |
| RD3-11 | RD3-DESIGN-D2-03 | R1 | design-d2 | P1 | 本轮发现 | ① | 已修：§3.7/§4.4 的 init/installer 分工改为“set_handler 整条重写门，元数据被覆盖；DPL 双源以 set_handler 为准” | panel/round1/design-d2.md:03；x86_64/trap_stub.rs | `grep -n "TrapEntryArch::init()\` 写元数据" doc` → 0；`grep -n "整条重写门" doc` → 1；冷读 CLOSED |
| RD3-12 | RD3-DESIGN-D2-04 | R1 | design-d2 | P2 | 本轮发现 | ① | 已修文档：§3.3 加“只适用 x86-64”限定；§4.7 `prot_init_done` 行标明 non-x86 无状态实现 | panel/round1/design-d2.md:04 | `grep -n "以下讨论只适用 x86-64" doc` → 1；`grep -n "non-x86 不进入全局" doc` → 1；冷读 CLOSED |
| RD3-13 | RD3-DESIGN-D2-05 | R1 | design-d2 | P2 | 本轮发现 | ① | 已修文档：§3.2 跨架构共性表改为“接口保留形状；当前无运行期消费者，Privilege 仅测试/预留” | panel/round1/design-d2.md:05 | `grep -n "仅测试/接口预留" doc` → 1；冷读 CLOSED |
| RD3-14 | RD3-DESIGN-D2-10 | R1 | design-d2 | P2 | 本轮发现 | ② | ② 相邻：目标 `D03-design-batch`（下一 design batch）；修于下一 design batch；用户批准 2026-11-12（代理裁决 D-053）；理由 四处代码注释残留，属代码批 | panel/round1/design-d2.md:10 | OPEN-DEBT `D03-design-batch`；D-053；冷读 CLOSED（登记核对） |
| RD3-15 | RD3-DESIGN-D2-11 | R1 | design-d2 | P2 | 本轮发现 | ② | ② 相邻：目标 `D03-design-batch`；修于下一 design batch；用户批准 2026-11-12（代理裁决 D-053）；理由 TSS 占位/riscv64 断言强度属代码修改 | panel/round1/design-d2.md:11 | OPEN-DEBT `D03-design-batch`；D-053；冷读 CLOSED（登记核对） |
| RD3-16 | RD3-DESIGN-D2-16 | R1 | design-d2 | P2 | 本轮发现 | ② | ② 相邻：目标 `D03-design-batch`（ARCH 提案：就地初始化 + 删 refresh_descriptors）；修于下一 design batch；用户批准 2026-11-12（代理裁决 D-053）；理由 ARCH 契约改动不在本轮 | panel/round1/design-d2.md:16 | OPEN-DEBT `D03-design-batch`；D-053；冷读 CLOSED（登记核对） |
| RD3-17 | RD3-READER-A-01 | R1 | reader | P2 | 本轮发现 | ① | 已修：§3.2 职责句改为“§2.4 的 TSS/段描述符/内核栈与系统调用 MSR，§2.5 的 IDT 与门” | panel/round1/reader-abc.md:19 | `grep -n "§2.5 的\"IDT + 门 + 系统调用入口\"" doc` → 0；冷读 CLOSED |
| RD3-18 | RD3-READER-A-02 | R1 | reader | P2 | 本轮发现 | ① | 已修：§5.4 末注改为“T03/T06/T07 在 §5.4 就地定义，其余见 §5.3” | panel/round1/reader-abc.md:38 | `grep -n "缺口 ID 与分流见 §5.3，本表" doc` → 0；`grep -n "T03/T06/T07 在 §5.4 就地定义" doc` → 1；冷读 CLOSED |
| RD3-19 | RD3-READER-C-01 | R1 | reader | P2 | 本轮发现 | ① | 已修：§1.2 与 §4.6 的入口表拆成“异常/中断入口”与“系统调用入口”两行，x86 格注明不是 IDT 门 | panel/round1/reader-abc.md:259 | `grep -n "系统调用入口 | SYSENTER/SYSCALL MSR" doc` → 1；冷读 CLOSED |
| RD3-20 | RD3-READER-C-02 | R1 | reader | P2 | 本轮发现 | ① | 已修：§3.7 的 90/92 改为“替换 16 条 PIC 门：39−16+64+3/5” | panel/round1/reader-abc.md:279 | `grep -n "39 − 16 + 64 + 3/5" doc` → 1；冷读 CLOSED |
| RD3-27 | RD3-DESIGN-D2-06 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：RD3-DESIGN-D2-06；已登记 suggestions.md | panel/round1/design-d2.md | 不适用（建议级） |
| RD3-28 | RD3-DESIGN-D2-07 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：RD3-DESIGN-D2-07；已登记 suggestions.md | panel/round1/design-d2.md | 不适用（建议级） |
| RD3-29 | RD3-DESIGN-D2-08 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：RD3-DESIGN-D2-08；已登记 suggestions.md | panel/round1/design-d2.md | 不适用（建议级） |
| RD3-30 | RD3-DESIGN-D2-09 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：RD3-DESIGN-D2-09；已登记 suggestions.md | panel/round1/design-d2.md | 不适用（建议级） |
| RD3-33 | RD3-DESIGN-D2-12 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：RD3-DESIGN-D2-12；已登记 suggestions.md | panel/round1/design-d2.md | 不适用（建议级） |
| RD3-34 | RD3-DESIGN-D2-13 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：RD3-DESIGN-D2-13；已登记 suggestions.md | panel/round1/design-d2.md | 不适用（建议级） |
| RD3-35 | RD3-DESIGN-D2-14 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：RD3-DESIGN-D2-14；已登记 suggestions.md | panel/round1/design-d2.md | 不适用（建议级） |
| RD3-36 | RD3-DESIGN-D2-15 | R1 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：RD3-DESIGN-D2-15；已登记 suggestions.md | panel/round1/design-d2.md | 不适用（建议级） |

| RD3-37 | CLAIM-01 | R1 | refute | P0 | 本轮发现 | ① | 同根因：memcpy 非自拷贝，并入 RD3-01 | panel/round1/refute.md | 冷读 CLOSED（certify 回读）；同 RD3-01 |
| RD3-38 | CLAIM-02 | R1 | refute | P0 | 本轮发现 | ① | 同根因：pg_utils 双副本，并入 RD3-08 | panel/round1/refute.md | 冷读 CLOSED（certify 回读）；同 RD3-08 |
| RD3-39 | CLAIM-03 | R1 | refute | P0 | 本轮发现 | ① | 同根因：KERNEL_MAY_ALLOC 已删，并入 RD3-03 | panel/round1/refute.md | 冷读 CLOSED（certify 回读）；同 RD3-03 |
| RD3-40 | CLAIM-04 | R1 | refute | P1 | 本轮发现 | ① | 同根因：IOPL 可授予，并入 RD3-21 | panel/round1/refute.md | 冷读 CLOSED（certify 回读）；同 RD3-21 |
| RD3-41 | CLAIM-05 | R1 | refute | P2 | 本轮发现 | FP | 反证失败（主张成立，不是缺陷）：IF=0 到 timer 打开、prot_init_done 读者唯一 | panel/round1/refute.md | `grep -n "prot_init_done" doc` → 命中 |
| RD3-42 | CLAIM-06 | R1 | refute | P2 | 本轮发现 | FP | 反证失败（主张成立，不是缺陷）：STUB_VECTORS 41 与门集合一致、39=16+23 算术成立 | panel/round1/refute.md | `grep -n "STUB_VECTORS" doc` → 命中 |
| RD3-43 | RD3-CERT-FACT-01 | R1 | fact-cert | P0 | 本轮发现 | ① | 已修：§2.1 的 `local_cbi` 改为指向 `pre_init` 的 `.unpaged_bss` 暂存副本，memcpy 搬进最终全局；代码块注释改“副本身份关系” | panel/round1/certify-fact.md；certify-reader.md | `grep -n "local_cbi\` 指向的也是这份最终全局" doc` → 0；冷读 CLOSED（二次回读） |
| RD3-44 | RD3-CERT-FACT-02 | R1 | fact-cert | P0 | 本轮发现 | ① | 已修：§2.1 删 memcpy 的失败点改为 `pg_identity` 的 `assert(cbi->mem_high_phys)`（pg_utils.c:170） | panel/round1/certify-fact.md | `grep -n "boot module 计数检查处失败" doc` → 0；冷读 CLOSED |
| RD3-45 | RD3-CERT-FACT-03 | R1 | fact-cert | P0 | 本轮发现 | ① | 已修：§5.4 M9 改为“普通副本建第二张表并切 CR3” | panel/round1/certify-fact.md；certify-reader.md | `grep -n "原地重填" doc` → 0；冷读 CLOSED |
| RD3-46 | RD3-CERT-FACT-04 | R1 | fact-cert | P1 | 本轮发现 | ① | 已修：§3.7 的 3/5 差改为 `CONFIG_SMP` 的 0xF1/0xF2 | panel/round1/certify-fact.md | `grep -n "取决于是否计入两条错误向量" doc` → 0；冷读 CLOSED |
| RD3-47 | RD3-CERT-FACT-05 | R1 | fact-cert | P2 | 本轮发现 | ② | ② 相邻：目标 `D03-citation-tool-batch`；修于下一 workflow 边界；用户批准 2026-11-12（代理裁决 D-053）；理由 citation 工具只认 `//` 标注，本文 42 个 `/* */` 块全跳过 | panel/round1/certify-fact.md | loop §8 已登记；D-053；冷读 CLOSED（登记） |
| RD3-48 | RD3-CERT-FACT-06 | R1 | fact-cert | P2 | 本轮发现 | ① | 已修：§3.5 `IDENTITY_MAP_END` 改为“通过 `use ... as` 转出” | panel/round1/certify-fact.md | `grep -n "的重导出" doc` → 0；冷读 CLOSED |
| RD3-49 | RD3-CERT-FACT-07 | R1 | fact-cert | P2 | 本轮发现 | ① | 已修：§2.1 代码块“自拷贝论证”改“副本身份关系”；§4.1 “随后两步”改“随后一步” | panel/round1/certify-fact.md | `grep -n "自拷贝论证" doc` → 0；`grep -n "随后两步" doc` → 0；冷读 CLOSED |
| RD3-50 | RD3-CERT-FACT-08 | R1 | fact-cert | P2 | 本轮发现 | ① | 已修：§5 的 `src/*.rs` 改 `src/**/*.rs` | panel/round1/certify-fact.md | `grep -n "src/\*.rs" doc` → 0；冷读 CLOSED |
| RD3-51 | RD3-CERT-READER-03 | R1 | reader-cert | P2 | 本轮发现 | ① | 已修：§1.2 标题改“三张表与四类入口”；§4.6 表头/§3.8 的“三问”同步改“四类入口” | panel/round1/certify-reader.md | `grep -n "三张表与三个问题" doc` → 0；冷读 CLOSED |
| RD3-52 | RD3-CERT-READER-01 | R1 | reader-cert | P1 | 本轮发现 | ① | 同根因：`local_cbi` 归属错误，并入 RD3-43 | panel/round1/certify-reader.md | 冷读 CLOSED（二次回读）；同 RD3-43 |
| RD3-53 | RD3-CERT-READER-02 | R1 | reader-cert | P1 | 本轮发现 | ① | 同根因：§5.4 M9 旧机制，并入 RD3-45 | panel/round1/certify-reader.md | 冷读 CLOSED（二次回读）；同 RD3-45 |
| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD3-54 | RD3-CERTF-01 | R1 | fact-cert | P1 | 本轮发现 | ① | 已修：§1.2 残留“三份答案/三问/三个问题”统一改为“四类入口”；§3 标题口径同步 | panel/round1/certify-final.md | `grep -n "三份答案落在三张表上" doc` → 0；`grep -n "三个问题" doc` → 0；冷读 CLOSED |
| RD3-55 | RD3-CERTF-02 | R1 | fact-cert | P2 | 本轮发现 | ① | 已修：§5.1 计数改为锚定 `#[test]` 口径 373/87、9 文件子集 55（剔除 riscv64/trap_stub.rs 注释命中） | panel/round1/certify-final.md | `grep -n "373 个测试" doc` → 1；`grep -n "87 个" doc` → 1；冷读 CLOSED |
| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD3-56 | RD3-CERTF2-01 | R1 | fact-cert | P2 | 本轮发现 | ① | 已修：§1.2 系统调用行补 `int $32/$33` 软中断门（libc 主路径），Rust 行补 32–35 软中断门 | panel/round1/certify-final2.md | `grep -n "int $32/$33" doc` → 1；冷读 CLOSED |
| RD3-57 | RD3-CERTF2-02 | R1 | fact-cert | P2 | 本轮发现 | ① | 已修：§1.2 的 ①②③ 加“§1.1 的”限定，与四行表序解耦 | panel/round1/certify-final2.md | `grep -n "§1.1 的 ②" doc` → 1；冷读 CLOSED |
## P-REFUTE 附加

| claim ID | 结果 | 处置 |
|---|---|---|
| CLAIM-01 | 反证成功（P0-fact，memcpy 非自拷贝） | 并入 RD3-01 |
| CLAIM-02 | 反证成功（P0-fact，pg_utils 双副本） | 并入 RD3-08 |
| CLAIM-03 | 反证成功（P0-fact，KERNEL_MAY_ALLOC 已删） | 并入 RD3-03 |
| CLAIM-04 | 反证成功（P1-fact，IOPL 可授予） | 并入 RD3-21 |
| CLAIM-05 | 反证失败（主张成立） | 无缺陷 |
| CLAIM-06 | 反证失败（主张成立） | 无缺陷 |
| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
