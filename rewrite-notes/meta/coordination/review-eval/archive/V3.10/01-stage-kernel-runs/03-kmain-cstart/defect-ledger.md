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
| RD3-11 | RD3-DESIGN-D2-03 | R1 | design-d2 | P1 | 本轮发现 | ① | 已修：§3.7/§4.4 的 init/installer 分工改为“set_handler 整条重写门，元数据被覆盖；DPL 双源以 set_handler 为准” | panel/round1/design-d2.md:03；x86_64/trap_stub.rs | `grep -n "TrapEntryArch::init()` 写元数据" doc` → 0；`grep -n "整条重写门" doc` → 1；冷读 CLOSED |
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
| RD3-43 | RD3-CERT-FACT-01 | R1 | fact-cert | P0 | 本轮发现 | ① | 已修：§2.1 的 `local_cbi` 改为指向 `pre_init` 的 `.unpaged_bss` 暂存副本，memcpy 搬进最终全局；代码块注释改“副本身份关系” | panel/round1/certify-fact.md；certify-reader.md | `grep -n "local_cbi` 指向的也是这份最终全局" doc` → 0；冷读 CLOSED（二次回读） |
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
| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
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

## R2 确认轮（V3.3 四态模型）

> 3 角色 43 条 FINDING（FACT 6、DESIGN-D2 10、READER-ABC 27）。
> P0 = 3（bootstrap 字段已删、GsArea.user_rsp、Rust 无 IOAPIC 64 门）；P1 = 13；P2 = 22；P3 = 5。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD3-R2-01 | RD3B-FACT-01 | R2 | fact | P0 | 本轮发现 | ① | 已修：§4.1 `bootstrap_start/len` 改为已随 D01-F03 删除 | panel/round2/fact.md | 冷读 CLOSED（二次回读 RD3-R2-01） |
| RD3-R2-02 | RD3B-FACT-02 | R2 | fact | P0 | 本轮发现 | ① | 已修：§3.4 `GsArea.user_rsp` 改为由 SYSCALL 入口写入 | panel/round2/fact.md | 冷读 CLOSED（二次回读 RD3-R2-02） |
| RD3-R2-03 | RD3B-FACT-03 | R2 | fact | P0 | 本轮发现 | ① | 已修：§3.7/§4.7 改为 Rust IDT 41 门、IOAPIC 只路由 16 条 ISA IRQ | panel/round2/fact.md | 冷读 CLOSED（二次回读 RD3-R2-03） |
| RD3-R2-04 | RD3B-FACT-04 | R2 | fact | P2 | 本轮发现 | ① | 已修：§3.6 Phase F 改为 boot_init_timer 的 program→register→gate 三步 | panel/round2/fact.md | 冷读 CLOSED（二次回读 RD3-R2-04） |
| RD3-R2-05 | RD3B-FACT-05 | R2 | fact | P2 | 本轮发现 | ① | 已修：§4.6 sscratch 时序改为返回前重写、返回后存用户 sp | panel/round2/fact.md | 冷读 CLOSED（二次回读 RD3-R2-05） |
| RD3-R2-06 | RD3B-FACT-06 | R2 | fact | P2 | 本轮发现 | ① | 已修：§5.1 的 3/7 改为 3/6（grep 含一条注释命中） | panel/round2/fact.md | 冷读 CLOSED（二次回读 RD3-R2-06） |
| RD3-R2-07 | RD3B-DESIGN-D2-01 | R2 | design-d2 | P1 | 本轮发现 | ① | 已修：同 FACT-01，§4.1 bootstrap 字段已删 | panel/round2/design-d2.md | 冷读 CLOSED（二次回读 RD3-R2-07） |
| RD3-R2-08 | RD3B-DESIGN-D2-02 | R2 | design-d2 | P2 | 本轮发现 | ① | 已修：同 FACT-03，IOAPIC 64 门表不存在 | panel/round2/design-d2.md | 冷读 CLOSED（二次回读 RD3-R2-08） |
| RD3-R2-09 | RD3B-DESIGN-D2-03 | R2 | design-d2 | P2 | 本轮发现 | ② | ② 相邻：目标 `D03-design-batch`；修于下一 design batch；用户批准 2026-11-12（代理裁决 D-053）；理由 `bsp_finish_booting` 过期注释属代码批 | panel/round2/design-d2.md | 冷读 CLOSED（二次回读 RD3-R2-09） |
| RD3-R2-10 | RD3B-DESIGN-D2-04 | R2 | design-d2 | P2 | 本轮发现 | ② | ② 相邻：目标 `D03-design-batch`；修于下一 design batch；用户批准 2026-11-12（代理裁决 D-053）；理由 非 0 BSP 的 TSS 槽 0 写错属代码批 | panel/round2/design-d2.md | 冷读 CLOSED（二次回读 RD3-R2-10） |
| RD3-R2-11 | RD3B-DESIGN-D2-05 | R2 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3）：死抽象清理；已登记 suggestions.md | panel/round2/design-d2.md | 冷读 CLOSED（二次回读 RD3-R2-11） |
| RD3-R2-12 | RD3B-DESIGN-D2-06 | R2 | design-d2 | P2 | 本轮发现 | ① | 已修：§5.1/M1 改为 init 门表测试，post-install 覆盖无断言 | panel/round2/design-d2.md | 冷读 CLOSED（二次回读 RD3-R2-12） |
| RD3-R2-13 | RD3B-DESIGN-D2-07 | R2 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3）：cpu_count 写而不读；已登记 suggestions.md | panel/round2/design-d2.md | 冷读 CLOSED（二次回读 RD3-R2-13） |
| RD3-R2-14 | RD3B-DESIGN-D2-08 | R2 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3）：cut_memmap/add_memmap 命名残留；已登记 suggestions.md | panel/round2/design-d2.md | 冷读 CLOSED（二次回读 RD3-R2-14） |
| RD3-R2-15 | RD3B-DESIGN-D2-09 | R2 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3）：§4.1 kmain 节选行重排（D-045）；已登记 suggestions.md | panel/round2/design-d2.md | 冷读 CLOSED（二次回读 RD3-R2-15） |
| RD3-R2-16 | RD3B-DESIGN-D2-10 | R2 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3）：§4.6 TSP=0 与 aarch64 载体文件头冲突；已登记 suggestions.md | panel/round2/design-d2.md | 冷读 CLOSED（二次回读 RD3-R2-16） |
| RD3-R2-17 | RD3B-READER-A-01 | R2 | reader-abc | P1 | 本轮发现 | ① | 已修：§2.1 对象副本段与四行证据块不给归属，读者要自行反推 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-17） |
| RD3-R2-18 | RD3B-READER-A-02 | R2 | reader-abc | P1 | 本轮发现 | ① | 已修：`int $32/$33`、`32–35`、`int 0x21` 三种记法混用 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-18） |
| RD3-R2-19 | RD3B-READER-A-03 | R2 | reader-abc | P1 | 本轮发现 | ① | 已修：「software only」与「编程 LAPIC/IOAPIC」互相拆台 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-19） |
| RD3-R2-20 | RD3B-READER-A-04 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：TSS 字段表 `cs` 行先说「转场期望值」再说「没有读者」 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-20） |
| RD3-R2-21 | RD3B-READER-A-05 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：目标 ② 要求 `kmain` 的不可交换次序，正文只给五步清单 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-21） |
| RD3-R2-22 | RD3B-READER-A-06 | R2 | reader-abc | P1 | 本轮发现 | ① | 已修：步骤 5 第一次 `memcpy` 没有读者，读者无法判断是否死代码 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-22） |
| RD3-R2-23 | RD3B-READER-A-07 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：「第 4 步」在节选里没有编号，易对到 `ltr` 上 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-23） |
| RD3-R2-24 | RD3B-READER-A-08 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：「五处表寄存器」把 CS/DS 与 GDTR/IDTR 混为一类 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-24） |
| RD3-R2-25 | RD3B-READER-B-01 | R2 | reader-abc | P1 | 本轮发现 | ① | 已修：「本轮未跑」「D01-F08」「没有出入可披露」等评审残留 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-25） |
| RD3-R2-26 | RD3B-READER-B-02 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：「没有丢弃后重建」在三节重复，且引号风格不一致 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-26） |
| RD3-R2-27 | RD3B-READER-B-03 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：§1.4 逐字重述 §1.2 的「保护结构」定义 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-27） |
| RD3-R2-28 | RD3B-READER-B-04 | R2 | reader-abc | P1 | 本轮发现 | ① | 已修：`Step 1`/`Step 8` 编号从未定义 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-28） |
| RD3-R2-29 | RD3B-READER-B-05 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：§5.1 九数相加 56，正文写 55，减法过程未写清 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-29） |
| RD3-R2-30 | RD3B-READER-B-06 | R2 | reader-abc | P1 | 本轮发现 | ① | 已修：`finish` 写成「到函数返回前」，实际停在调用者 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-30） |
| RD3-R2-31 | RD3B-READER-B-07 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：同一组七个 slice 在相邻两段用了两套名字 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-31） |
| RD3-R2-32 | RD3B-READER-B-08 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：引导方/引导器/装载方三名并存，引号风格分叉 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-32） |
| RD3-R2-33 | RD3B-READER-B-09 | R2 | reader-abc | P1 | 本轮发现 | ① | 已修：§1.4 排除 `sscratch` 交换与返回路径，§4.6 又讲了一遍 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-33） |
| RD3-R2-34 | RD3B-READER-C-01 | R2 | reader-abc | P1 | 本轮发现 | ① | 已修：`sscratch` 的「返回之后不能用」时序反了 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-34） |
| RD3-R2-35 | RD3B-READER-C-02 | R2 | reader-abc | P1 | 本轮发现 | ① | 已修：前移 `init_clock_and_interrupts` 的反事实里「新旧表不匹配」前提不成立 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-35） |
| RD3-R2-36 | RD3B-READER-C-03 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：`intr_init` 的「防御性次序」是无证据的动机归因 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-36） |
| RD3-R2-37 | RD3B-READER-C-04 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：「两条理由」里第二条是约束，不是理由 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-37） |
| RD3-R2-38 | RD3B-READER-C-05 | R2 | reader-abc | P1 | 本轮发现 | ① | 已修：`kernel_may_alloc` 的读者在 §2.1 与 §4.1 相互矛盾 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-38） |
| RD3-R2-39 | RD3B-READER-C-06 | R2 | reader-abc | P1 | 本轮发现 | ① | 已修：`EXTERN`→`table.c` 的结论没有出现在 grep 证据里 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-39） |
| RD3-R2-40 | RD3B-READER-C-07 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：PWT/PCD 被写成「防止访问出事」，实际只影响缓存 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-40） |
| RD3-R2-41 | RD3B-READER-C-08 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：先 `lidt` 后 `lgdt` 的失败窗口没有界定 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-41） |
| RD3-R2-42 | RD3B-READER-C-09 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：收束段只写 x86-64 的出口状态，未限定范围 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-42） |
| RD3-R2-43 | RD3B-READER-C-10 | R2 | reader-abc | P2 | 本轮发现 | ① | 已修：§1.2「256 个门」与 §2.5「39 条门」冲突 | panel/round2/reader-abc.md | 冷读 CLOSED（二次回读 RD3-R2-43） |
| RD3-R2-44 | RD3-R2-CERT-01 | R2 | fact-cert | P0 | 本轮发现 | ① | 已修：§2.1 次序承重改为“步骤 3 依赖步骤 2；步骤 5 只需早于启动循环，proc_init 不读 boot_procs” | panel/round2/certify.md | `grep -n "步骤 3 依赖步骤 2" doc` → 1；冷读 CLOSED（RD3-R2-44） |
| RD3-R2-45 | RD3-R2-CERT-02 | R2 | fact-cert | P2 | 本轮发现 | ① | 已修：§4.6 sscratch 改为“用户陷阱进入内核后保存用户 sp；返回前重写” | panel/round2/certify.md | `grep -n "用户陷阱进入内核后它保存的是用户 sp" doc` → 1；冷读 CLOSED（RD3-R2-45） |
| RD3-R2-46 | RD3-R2-CERT-03 | R2 | fact-cert | P2 | 本轮发现 | ① | 已修：§4.1 七个 slice 恢复中文名并注明具体字段见 §4.2 | panel/round2/certify.md | `grep -n "具体字段名见 §4.2" doc` → 1；冷读 CLOSED（RD3-R2-46） |
| RD3-R2-47 | RD3-R2-CERT-04 | R2 | fact-cert | P2 | 本轮发现 | ① | 已修：§5.1 的 13 条 trap 测试拆为 9 init + 2 set_handler + 2 布局 | panel/round2/certify.md | `grep -n "9 条 `init` 门表元数据" doc` → 1；冷读 CLOSED（RD3-R2-47） |
| RD3-R2-48 | RD3-R2-CERT2-01 | R2 | fact-cert | P0 | 本轮发现 | ① | 已修：§4.1 七个 slice 直接写代码字段名（不再指向 §4.2）；L184 改为启动循环读 image[]、proc_init 不读 | panel/round2/certify2.md | `grep -n "reserved_regions`、`memmap`、`boot_modules" doc` → 1；冷读 CLOSED（RD3-R2-48） |


## REG2-2026-11-17 回归轮补账（D-055 重做；r6 R6-b；面板 tmp/…/panel/regress/）

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| REG-D03-READER-A | REG-D03-READER-A-01..08 | REG2 | P-READER-A | P1×1/P2/P3 | 读者 | ① | 已修（含 §4.7 出口矛盾、§3.6 未定义 Phase 名、§2.5 中间态论证——均为合并角色漏报） | `ff3ea1844`（03 部分） | FREEZE 11-17 回归节 |
| REG-D03-READER-B | REG-D03-READER-B-01..05 | REG2 | P-READER-B | P2/P3 | 读者 | ① | 已修 | `ff3ea1844` | 同上 |
| REG-D03-READER-C | REG-D03-READER-C-01、02、03、05、06（无 C-04） | REG2 | P-READER-C | P2×4/P3×1 | 读者 | ① | 已修 | `ff3ea1844` | 同上 |
| REG-D03-CERT | REG-D03-CERT-01..07（第 1 轮） | REG2 | certify | P2 | 同步 | ① | 独立冷读三轮 7/3/1 → 按 §4.0b 记 BLOCKED + STUCK-REPORT → 两个复发根因结构性重写（§2.5 拆两段、§3.6 Phase 表七行）→ rewrite-verify PASS | `ff3ea1844`；`STUCK-REPORT.md`；`panel/regress/rewrite-verify.md` | 重写验证 = PASS |
| REG-D03-CERT2 | REG-D03-CERT2-01..03（第 2 轮） | REG2 | certify | P2 | 同步 | ① | 同上（同批结构重写覆盖） | 同上 | 同上 |
| REG-D03-CERT3 | REG-D03-CERT3-01（第 3 轮） | REG2 | certify | P2 | 同步 | ① | 同上 | 同上 | 同上 |
| REG2-SUGG | READER-A S01..S05、B/C 聚合 SUGG 段（31 条口径含聚合） | REG2 | 各角色 | — | 建议 | SUGG | 入 suggestions，不阻断 | `panel/regress/*.md` | — |

## FULL-2026-11-18 完整 review 补账（r6 R6-b；面板 tmp/…/panel/full/）

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD5F-FACT | RD5F-FACT-01、02、03；FACT-04 为面板自注撤回 | FULL | P-FACT | P2 | 事实 | ① | 已修 | `ebcf673dc`（锚点偏移/测试覆盖口径/C 术语等 6 主题） | FREEZE 完整 review 节：anchor 75/75 |
| RD5F-READER-A | RD5F-READER-A-01 | FULL | P-READER-A | P3 | 读者 | ① | 已修 | `ebcf673dc` | 同上 |
| RD5F-READER-B | RD5F-READER-B-01..04 | FULL | P-READER-B | P2/P3 | 读者 | ① | 已修 | `ebcf673dc` | 同上 |
| RD5F-CERT | RD5F-CERT-01..04（certify 更细粒度残项） | FULL | certify | P2 | 同步 | ① | 已修；独立冷读 certify 最终 6/6 CLOSED | `c2ba58596` | FREEZE：certify 6/6 CLOSED |
| RD5F-DESIGN-D2-01 | [MODERN] §3.7 括注 is_trap 与重写路径不符 | FULL | P-DESIGN-D2 | P2/P3 | 设计 | ① | 已修（文档侧） | `ebcf673dc`（括注歧义主题） | FREEZE |
| RD5F-DESIGN-D2-02 | [DEL] arm64 模块注释 UNDEFINED 残留 | FULL | P-DESIGN-D2 | P2 | 设计 | ② | 登记 D03-design-batch（FREEZE 明载；D-053 顺延）——V3.2 下 ② 须相邻/批准，此处为登记既有批次的存量路径，r6 注记：严格口径应在 D-053 行追加该项 | `OPEN-DEBT.md`（D03-design-batch 行）；FREEZE 完整 review 节 | — |
| RD5F-DESIGN-D2-03/04 | [DEL] x86 LDT 尾注失实；trap_entry 15 个零消费者向量名 | FULL | P-DESIGN-D2 | P2 | 设计 | ①/SUGG | 03 已修（ebcf673dc「LDT 残留」主题）；04（死导出）批次级处理 | `ebcf673dc` | FREEZE |
| RD5F-DESIGN-D2-05、07、08、09 | [DEL] cpu_count 只写；kmain 临时诊断块含无消费者 MSR 读；errcode 布尔列；§3.2 锚点格式 | FULL | P-DESIGN-D2 | P3 | 设计 | SUGG | 入 suggestions，不阻断 | `panel/full/design-d2.md` | — |
| RD5F-DESIGN-D2-06 | [DEL] Privilege/PrivilegeLevel 接口族无运行期消费者，去留属契约裁决 | FULL | P-DESIGN-D2 | P3 | 设计 | SUGG | 待用户亲裁（挂账，与 D-050/051/052 同类）；不阻断 | `panel/full/design-d2.md`；DECISIONS.md | — |
| RD5F-SUGG | 各文件聚合 SUGG 段（3 条口径） | FULL | 各角色 | — | 建议 | SUGG | 入 suggestions | `panel/full/*.md` | — |

## R3 确认轮（V3.4；D-066 授权的重开确认）

> 面板：`panel/round3/`（fact 2、design-d2 15、reader-a 3、reader-b 0、reader-c 2 条 FINDING；SUGGESTION 46；refute 6 条 CLAIM；INDEX.md 由 `panel-index.py` 生成，FINDING 22 / SUGGESTION 46 / CLAIM 6）。
> 确认轮权限按 §3.6：P0/P1/P2 事实与同步项本轮修（本面板无 P0/P1，5 条设计 P2 记 ②）；设计项不实施 ⑥（D-066）。
> 身份键：本面板用 `RD03-` 前缀；refute 的机器索引用 `CLAIM 11..16`（避开历史轮的 01..06）。
> **VARIANCE（面板标题层级规范化）**：design-d2 原用 `### FINDING <ID>` 加一条裸 `## FINDING` 标题、refute 的机器索引挤成单标题，都会让 INDEX 计数与覆盖率口径分叉；主 agent 把 15 条 `### FINDING` 升为 `##`、删除裸标题、把 refute 索引重排为六个独立 `## CLAIM <数字>` 标题——**正文内容未改动**。
> 独立回执：`certify/round3-certify.md`（首轮 6/2，按证据改正 3 处后复核 8/0 PASS）。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R3-01 | RD03-FACT-01、RD03-FACT-02、RD03-SUG-01 | R3 | fact | P3 | finding | ① | 已修：全量两对数字只在「属性行计数」（剔除注释中 `#[test]` 字样）口径下成立；文档未对该口径作任何说明，而 §5.1 同段与 §6.2 给出的复算命令恰是 `grep -c '#\[test\]'`，按此原样复算会得到不同数字 - 影响：读者按文档自己的命令复算全量会得到 3 | panel/round3/ 的 finding | RD03-FACT-01 等 3 条的改动由 certify/round3-certify.md 对应条目回读确认 |
| RD-R3-02 | RD03-SUG-02 | R3 | fact | P3 | suggestion | ① | 已修：`os/arch/src/x86_64/trap_entry.rs:7-21` 的文件头只自述「IDT + SYSCALL MSR 两类入口机制对 OS 隐藏」，未提「访问控制」（DPL）这一半；现锚点只支撑切分轴的入口机制半边。建议锚点改到能同时体现两半的行段，或在正文注明该佐 | panel/round3/ 的 suggestion | RD03-SUG-02 等 1 条的改动由 certify/round3-certify.md 对应条目回读确认 |
| RD-R3-03 | RD03-DESIGN-D2-01、RD03-DESIGN-D2-02、RD03-DESIGN-D2-03 | R3 | design-d2 | P2 | finding | ② | ② 相邻：目标 `R3-design-batch`（下一 design batch）；用户批准 2026-11-20（D-066）；`init()` 给向量 0/1/3/4/32-35 共 8 个门写 trap-gate 类型（`is_trap=true`），但这些门随后全部被 `install_idt_handlers → set_handler` 用 `is_trap=false`（中断门）整条重写；C  | panel/round3/ 的 finding | RD03-DESIGN-D2-01 等 3 条的延期已落 D-066 与 OPEN-DEBT 的 R3-design-batch 行 |
| RD-R3-04 | RD03-DESIGN-D2-04、RD03-DESIGN-D2-05 | R3 | design-d2 | P2 | finding | ② | ② 相邻：目标 `R3-design-batch`（下一 design batch）；用户批准 2026-11-20（D-066）；为让 `init_protection` 一份函数体编译三架构，非 x86 的 `with_protection/with_protection_mut/with_trap_entry` 每次调用伪造 `CurrentProtection::init(0, VirBytes::n | panel/round3/ 的 finding | RD03-DESIGN-D2-04 等 2 条的延期已落 D-066 与 OPEN-DEBT 的 R3-design-batch 行 |
| RD-R3-05 | RD03-DESIGN-D2-06、RD03-DESIGN-D2-07、RD03-DESIGN-D2-08 | R3 | design-d2 | P3 | finding | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：三个架构的 `cpu_count` 字段注释均写 "Number of CPUs initialized (for SMP tracking)"，但没有任何运行期读者——唯一消费是各自文件里直接构字段的测试（`x86_64/protection.rs:729-732` 等）。x8 | panel/round3/ 的 finding | RD03-DESIGN-D2-06 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-06 | RD03-DESIGN-D2-09、RD03-DESIGN-D2-10、RD03-DESIGN-D2-11 | R3 | design-d2 | P3 | finding | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：kmain 生产体里留着自declared临时的 v3 证据采集：注释写 "Evidence capture (temporary, v3)" 与 "This shadow can be deleted once the diagnostics below confirm it" | panel/round3/ 的 finding | RD03-DESIGN-D2-09 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-07 | RD03-DESIGN-D2-12、RD03-DESIGN-D2-13、RD03-DESIGN-D2-14 | R3 | design-d2 | P3 | finding | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：syscall 入口 asm 给 `frame.vector` 压字面量 `32`（注释自注 "KERN_CALL_VECTOR"），而同一段 asm 的用户段选择子用 `const` 操作数绑定（`user_cs = const USER_CS_SELECTOR`）。同一"as | panel/round3/ 的 finding | RD03-DESIGN-D2-12 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-08 | RD03-DESIGN-D2-15 | R3 | design-d2 | P3 | finding | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：`KernelInfo.syscall_entry` 由 boot 侧传入、`store_kernel_info` 落地，但生产代码零读者——`configure_syscall` 用 `syscall_entry_va()`（内核自己的 asm 标签），注释自declared  | panel/round3/ 的 finding | RD03-DESIGN-D2-15 等 1 条已登记 suggestions.md 的 R3 段 |
| RD-R3-09 | RD03-READER-A-01、RD03-READER-A-02 | R3 | reader-a | P2 | finding | ④ | 已修：对照表说 `&k_boot_stktop` 是"第一个参数"，而 §2.4 引用的函数原文中它是第二个实参（第一个实参是 CPU 号 `0`）。 - 影响：两处无法同时为真。对照表是 §2.7 架构对照的核心证据，一处错位会让读者对该表其余各行与"参数"类描述整体产生怀疑；读者被 | panel/round3/ 的 finding | RD03-READER-A-01 等 2 条的改动由 certify/round3-certify.md 对应条目回读确认 |
| RD-R3-10 | RD03-READER-A-03、RD03-RA-S01、RD03-RA-S02 | R3 | reader-a | P3 | finding | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）："原文如下"指向的内容不存在——E1 注释原文已在上方代码块（行 823，标注「os/kernel/src/lib.rs:fn init_protection（E1 注释原文）」）给出，"如下"之后是候选比较表，不是 E1 原文。 - 影响：读者向下寻找"如下"的 E1 原文而落 | panel/round3/ 的 finding | RD03-READER-A-03 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-11 | RD03-RA-S03、RD03-RA-S04、RD03-RA-S05 | R3 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）："按下的"不是标准术语，三处复现；读者每处都要猜它指"SYSCALL 指令执行时 CPU 装载的内核 CS"。 - 影响：不阻塞（上下文可推），但同一处生造措辞反复出现，增加每次遇到的理解税。 - 证据或推理：三处均以"按下的"修饰内核 CS，无任何一处给出该措辞的定义。 - 最 | panel/round3/ 的 suggestion | RD03-RA-S03 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-12 | RD03-RA-S06、RD03-RA-S07、RD03-RA-S08 | R3 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：TrapReturnArch 首次出现且无介绍；§3.2 的叙事是"拆成两个 trait"，读者会疑惑何时出现第三个 trait、它归哪篇。 - 影响：名字与语境（返回路径）可推出大意，不卡死；但与 §3.2"两个 trait"的框架产生张力。 - 证据或推理：行 792 说「R | panel/round3/ 的 suggestion | RD03-RA-S06 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-13 | RD03-RA-S09、RD03-RA-S10、RD03-RA-S11 | R3 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：同一事实三处陈述；行 961 已带 §5.2 引用，§5.2 内部又陈述两遍。 - 影响：无误导，属重复；§5.2 两段相距十行，重复感明显。 - 证据或推理：三处语义完全相同（载体不装 stub、门占位 0）。 - 最小修复：行 1259 简化为指回（"如本节开头所述"），行  | panel/round3/ 的 suggestion | RD03-RA-S09 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-14 | RD03-RA-S12、RD03-RA-S13、RD03-RA-S14 | R3 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）："出错的可见性"列在该行只给名词短语，不成句；对比前两行（"写错还来得及改；但错误表内容会在装载后生效""执行完即改变 CPU 行为，不可回退"），读者不知道该行想断言什么。 - 影响：归纳表是 C 章收束处，第三类的出错可见性缺一句完整表述。 - 证据或推理：同列前两行均为完整 | panel/round3/ 的 suggestion | RD03-RA-S12 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-15 | RD03-RA-S15、RD03-RA-S16 | R3 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）："查表表"未定义计数范围：§1.2 的四类入口表、§1.3 的指令特权表也是供查对的表，读者无法确定"第一张/第二张"数的是哪一类，编号失去导航作用。 - 影响：不阻塞，但自造计数词让编号承诺落空（读者数不到"第一张"之前的零张）。 - 证据或推理：全文无"查表表"的定义；行 3 | panel/round3/ 的 suggestion | RD03-RA-S15 等 2 条已登记 suggestions.md 的 R3 段 |
| RD-R3-16 | RD03-RB-S01、RD03-RB-S02、RD03-RB-S03 | R3 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：前提倒置——"Phase B"（连同隐含的 A–F 六 Phase 框架）在 §1.1 作为既定框架使用，其定义在约 890 行之后的 §3.6；"该篇引用的"的"该篇"也需要回看前句才能确定指 02 篇。Phase 一词在 §3.6 之前还出现于 §1.4 行 85（"`kma | panel/round3/ 的 suggestion | RD03-RB-S01 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-17 | RD03-RB-S04、RD03-RB-S05、RD03-RB-S06 | R3 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：重复冗余——同一事实（post-install 后 41 门与 `set_gate_ist` 无断言，同一批号）在一个自然段内说两遍，中间只隔一个 `user_accessible` 的插入句。 - 影响：读者会以为第二处带来新信息（"仍无断言"暗示前文说的是别的事），实际只是复 | panel/round3/ 的 suggestion | RD03-RB-S04 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-18 | RD03-RB-S07、RD03-RB-S08、RD03-RB-S09 | R3 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：重复冗余——对照表本应是压缩 recap，却把 §3.1 的完整理由逐字复制。 - 影响：轻微；两处日后只改一处会产生口径分叉。 - 证据或推理：两处文本逐字一致。 - 最小修复：§4.7 该列改"淘汰（理由见 §3.1 LDT 行）"。 - 置信度：高。 - 是否需源码核实：否 | panel/round3/ 的 suggestion | RD03-RB-S07 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-19 | RD03-RB-S10、RD03-RB-S11、RD03-RB-S12 | R3 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：重复冗余——表行已给出结论，紧随的反事实/展开段把同一结论重讲一遍；真正的增量（head.S 的 IF=0 证据链、i8259 的 ICW 细节、`[2,50000]` 夹取）被复述淹没。 - 影响：轻微；读者可能跳读，跳读时会略过真正的增量证据。 - 证据或推理：两处结论句同义 | panel/round3/ 的 suggestion | RD03-RB-S10 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-20 | RD03-RB-S13、RD03-RB-S14、RD03-RB-S15 | R3 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：节内分类错位——五类清单把"建 TSS"列为第三类，走读却把 `tss_init` 的调用行放进第二段，第三段只剩"填 GDT 四个描述符"；"建 TSS/填 GDT"这一类在走读里被拆到两段。走读用括注提示了 tss_init 的位置，但未说明"类"与"段"两套切分的对应关系。 | panel/round3/ 的 suggestion | RD03-RB-S13 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-21 | RD03-RB-S16、RD03-RB-S17、RD03-RB-S18 | R3 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：章节边界混淆——§2.1 第五步为证明"谁消费拷贝"，深入到启动循环的读写行为与 VM 读者；这些按 §1.4 属 06 篇（Phase C 及之后）。§2.2 对 init_clock/intr_init 有显式边界声明（行 271"它们的顺序属于本篇"、行 284"是本篇与时 | panel/round3/ 的 suggestion | RD03-RB-S16 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-22 | RD03-RB-S19、RD03-RB-S20、RD03-RB-S21 | R3 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：重复冗余＋行文——"挂空帧"的机理解释在同段讲两遍；修订考古注（曾写 UNDEFINED→已修正）插在论证句中间，打断因果链。 - 影响：轻微；aarch64 段是三架构段里最密的一节。 - 证据或推理：两处括注几乎逐字相同。 - 最小修复：第二处改"机理同上（见 `init`  | panel/round3/ 的 suggestion | RD03-RB-S19 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-23 | RD03-READER-C-01 | R3 | reader-c | P2 | finding | ④ | 已修：安全论证只覆盖了取指轴。`prot_load_selectors` 的实际次序是 `x86_lgdt` → `idt_init`（填内存）→ `idt_reload`（lidt）：在 lgdt 与 idt_reload 之间，IDTR 仍指向引导方 IDT，而 GDTR 已指向新 | panel/round3/ 的 finding | RD03-READER-C-01 等 1 条的改动由 certify/round3-certify.md 对应条目回读确认 |
| RD-R3-24 | RD03-READER-C-02、RD03-RC-S1、RD03-RC-S2 | R3 | reader-c | P3 | finding | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：反事实把 `pg_identity` 的 assert 定为首个失败点，但文档自己的因果图里有更早的 kinfo 消费者：步骤 3（`get_board_id_by_name(env_get(BOARDVARNAME))`）按“次序承重”段依赖步骤 2，且 cstart 里 `e | panel/round3/ 的 finding | RD03-READER-C-02 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-25 | RD03-RC-S3、RD03-RC-S4、RD03-RC-S5 | R3 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：单方法结论写成唯一方法。前半句（64 位没有立即数形式的远跳）正确且恰好是准确的限定；后半句过度——间接远跳 `ljmp m16:64` 在长模式同样合法，重载 CS 不只 push+`retfq` 一条路。`retfq` 写法是惯用与简洁的选择，不是架构唯一性。 - 影响：读者 | panel/round3/ 的 suggestion | RD03-RC-S3 等 3 条已登记 suggestions.md 的 R3 段 |
| RD-R3-26 | RD03-RC-S6、RD03-RC-S7 | R3 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：覆盖口径复述不一致；教学目标⑤⑥。 | panel/round3/ 的 suggestion | RD03-RC-S6 等 2 条已登记 suggestions.md 的 R3 段 |
| RD-R3-27 | CLAIM-11、CLAIM-12、CLAIM-13 | R3 | refute | P3 | claim | FP | 反证失败（主张成立） | 源码原文核对：apic.h:90-91 的 0xF0=timer/0xF1=sched、archconst.h:45 的 COPROC_SEG_VECTOR=9、STUB_VECTORS 41 项现算逐行吻合（panel/round3/refute.md 的 RD03-CLAIM-11/12/13） | panel/round3/refute.md 的 RD03-CLAIM-11/12/13（源码原文核对：apic.h:90-91 的 0xF0/0xF1、archconst.h:45 的 COPROC_SEG_VECTOR=9、STUB_VECTORS 41 项现算逐行吻合） | CLAIM-11/12/13 反证失败即主张成立，逐条源码锚见同文件各 CLAIM 的「证据」行 
| RD-R3-28 | CLAIM-14、CLAIM-15 | R3 | refute | P3 | claim | FP | 反证失败（主张成立） | 源码原文核对：arch/protection.rs:99-101 与 arch/trap_entry.rs:109-111 的契约注释、STAR/LSTAR/SFMASK/EFER 写入次序、arm64 无 SP_EL1 写点、riscv64 trap_stub.rs:536 的 csrrw（panel/round3/refute.md 的 RD03-CLAIM-14/15） | panel/round3/refute.md 的 RD03-CLAIM-14/15（源码原文核对：arch/protection.rs:99-101 与 arch/trap_entry.rs:109-111 的契约注释、STAR/LSTAR/SFMASK/EFER 次序、arm64 无 SP_EL1 写点、riscv64 trap_stub.rs:536 的 csrrw） | CLAIM-14/15 反证失败即主张成立（E1），逐条锚见同文件 
| RD-R3-29 | CLAIM-16 | R3 | refute | P2 | claim | ① | 已修：非阻断（is_trap 双来源，文档半边已修） | panel/round3/ 的 claim | CLAIM-16 等 1 条的改动由 certify/round3-certify.md 对应条目回读确认 |

## R39 确认轮（2026-11-22）

> **INVALIDATED（2026-11-22）**：本文件中的 R39 是 D01 的轮次，不是本篇轮次；D-114 仅适用于 D01。原“CONVERGED”宣告作废，状态为 CONVERGENCE-CANDIDATE。纠正记录见 `rewrite-notes/meta/coordination/review-eval/runs/01-stage-kernel/ROUND-IDENTITY-CORRECTION.md`。

| ID | 来源 | 节 | 轮 | 角色 | 严重度 | 类别 | 处置 | 修复 | 证据 | 状态 | — | — | — | — |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| RD-R39-001 | CONFIRM-D1 | 6.2 | R39 | cold-confirm | P1 | 文档/计数 | ① | 测试计数改为 16/13/12/0/1/3/0/1/7 | 重放 `grep -c '#\[test\]'` 九文件 | CLOSED；`certify/round39-confirmation.md` | — | — | — | — |
| RD-R39-002 | CONFIRM-D2 | 6.2 | R39 | cold-confirm | P1 | 文档/可执行性 | ① | GDB 组改生产 ELF + `target remote` + `hbreak` | 符号 `0xffff800000206330`；`-S` 下软件断点不可写高半 | CLOSED；`certify/round39-confirmation.md` | — | — | — | — |
| RD-R39-003 | CONFIRM-D3 | 6.2 | R39 | cold-confirm | P1 | 文档/工件 | ① | `break init_protection` 改生产 ELF 的 `hbreak minix_kernel::init_protection` | `gdb -ex 'info functions init_protection'` 命中 | CLOSED；`certify/round39-confirmation.md` | — | — | — | — |
| RD-R39-004 | CONFIRM-D4 | 6.2 | R39 | cold-confirm | P1 | 文档/命令 | ① | `info registers gdtr idtr tr` 改 `monitor info registers` | GDB 无 gdtr/idtr 寄存器名 | CLOSED；`certify/round39-confirmation.md` | — | — | — | — |
| RD-R39-005 | CONFIRM-D5 | 6.2 | R39 | cold-confirm | P2 | 文档/工件 | ① | 预期 GDT 基址改为生产 ELF 高半；不再用 test-protection 栈上实例 | 生产 ELF GDT 在高半 | CLOSED；`certify/round39-confirmation.md` | — | — | — | — |
| RD-R39-006 | CONFIRM-D6 | 5.3 | R39 | cold-confirm | P1 | 文档/覆盖 | ① | 改为“无负向测试；正向顺序钉见 init_protection_order_pin.rs” | 文件存在，断言 57-90 行 | CLOSED；`certify/round39-confirmation.md` | — | — | — | — |
| RD-R39-007 | CONFIRM-D7 | 5.3 | R39 | cold-confirm | P2 | 文档/覆盖 | ① | `fn init` 改为 256 空占位；41 真门由 `install_trap_stubs` 装 | `trap_entry.rs:188-200` | CLOSED；`certify/round39-confirmation.md` | — | — | — | — |
| RD-R39-008 | CONFIRM-D8 | 3.5 | R39 | cold-confirm | P2 | 文档/跨篇指针 | ① | §3.9 改 §3.4.3；freepde 处置改为字段删除/报告位 0/DM 走查 | 01 §3.4.3 | CLOSED；`certify/round39-confirmation.md` | — | — | — | — |
| RD-R39-009 | CONFIRM-D9 | 4.0 | R39 | cold-confirm | P2 | 文档/语义 | ① | `ltr` 改为必须可用 1001；高半丢失潜伏到首次 CPL3 读 sp0 | §2.5/§3.1 对照 | CLOSED；`certify/round39-confirmation.md` | — | — | — | — |
| RD-R39-010 | CONFIRM-D10 | 1.0 | R39 | cold-confirm | P2 | 文档/术语 | ① | “第一次行使特权”统一为 lgdt/lidt/lldt/ltr 装载动作 | §1.3 对照 | CLOSED；`certify/round39-confirmation.md` | — | — | — | — |
| RD-R39-011 | CONFIRM-D11 | 3.5 | R39 | cold-confirm | P3 | 文档/行号 | ① | BOOT_IDENTITY_MAP_END 217→221 | `grep -n` 221 | CLOSED；`certify/round39-confirmation.md` | — | — | — | — |
| RD-R39-012 | CONFIRM-D12 | 3.5 | R39 | cold-confirm | P3 | 文档/跨篇指针 | ① | 02 篇 §4.4→§4.3 | 02 篇 4.3 标题 | CLOSED；`certify/round39-confirmation.md` | — | — | — | — |
| RD-R39-013 | CONFIRM-D13 | 5.3/5.4 | R39 | cold-confirm | P3 | 文档/过程编号 | ① | 去掉 R03- 前缀，改“缺口N” | `grep -n 'R03'` 无命中 | CLOSED；`certify/round39-confirmation.md` | — | — | — | — |
