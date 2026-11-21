# D01 缺陷与处置台账

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| DESIGN-01 | design-d2 | P2 | 设计/C 形状 | ② | 转出：follow-ups.md D01-F03（删 bootstrap_start/len 与 free_upper_idx 交接，wire 版本 +1，需用户裁决） | panel/design-d2.md DESIGN-01 | 不适用（转出） |
| DESIGN-02 | design-d2 | P2 | 设计/死抽象 | ② | 转出：follow-ups.md D01-F04（删 BootShim trait，保留函数与返回类型） | panel/design-d2.md DESIGN-02 | 不适用（转出） |
| DESIGN-03 | design-d2 | P2 | 设计/死抽象 | ② | 转出：follow-ups.md D01-F05（删 PagingWithId 与两个死常量） | panel/design-d2.md DESIGN-03 | 不适用（转出） |
| DESIGN-04 | design-d2 | P2 | 设计/死抽象 | ② | 转出：follow-ups.md D01-F06（BootForm 删除或降级到 cfg(test)） | panel/design-d2.md DESIGN-04 | 不适用（转出） |
| DESIGN-05 | design-d2 | P1 | 设计/static mut 视图 | ② | 转出：follow-ups.md D01-F02（跨四个 boot face 加一次性守卫或 owned 快照；当前单次调用图安全，design-debt） | panel/design-d2.md DESIGN-05 | 不适用（转出）；D01 标 NOT CONVERGED |
| DESIGN-06 | design-d2 | P1 | 代码/深拷自包含 | ① | 已修：store_kernel_info 补到 7/7，新增 source/startup 溢出负控 | os/kernel/src/lib.rs 的 store_kernel_info 与两条新测试 | cargo test -p minix-kernel --features mock：865 通过 |
| DESIGN-07 | design-d2 | P2 | 设计/记录集合 | ② | 转出：follow-ups.md D01-F07（startup_objects 规范集合与容量，需用户裁决） | panel/design-d2.md DESIGN-07 | 不适用（转出） |
| DESIGN-08 | design-d2 | P2 | 设计/C 形状账本 | ② | 转出：follow-ups.md D01-F08（memmap 空洞数组与 KERNEL_MAY_ALLOC，需用户裁决） | panel/design-d2.md DESIGN-08 | 不适用（转出） |
| DESIGN-09 | design-d2 | P2 | 设计/非法状态 | ② | 转出：follow-ups.md D01-F09（MemoryRegion 非空/对齐约束） | panel/design-d2.md DESIGN-09 | 不适用（转出） |
| DESIGN-10 | design-d2 | P1 | 代码/fail-fast | ① | 已修：arch_boot_finish 入口前置 validate + check_boot_postconditions | os/kernel/src/lib.rs 的 arch_boot_finish | cargo test -p minix-kernel --features mock：865 通过 |
| DESIGN-11 | design-d2 | P1 | 设计/外部 ABI | ② | 转出：follow-ups.md D01-F10（GET_KINFO 载荷收窄需用户裁决：[ARCH] 标注或恢复全量布局） | panel/design-d2.md DESIGN-11 | 不适用（转出，需用户裁决） |
| DESIGN-12 | design-d2 | P1 | 代码/错误哨兵 | ① | 已修：alloc_pt_page 前置 is_registered 断言；wire_module_to_boot_module 改 Result | os/arch/src/arch/pt_alloc.rs、os/libs/minix-boot/src/handoff_v1_read.rs | cargo test -p minix-boot：175 通过 |
| DESIGN-13 | design-d2 | P2 | 设计/构造校验 | ② | 转出：follow-ups.md D01-F11（Serializer authority 必填、BootPrepareResult 校验构造） | panel/design-d2.md DESIGN-13 | 不适用（转出） |
| DESIGN-14 | design-d2 | P2 | 代码/注释误导 | ① | 已修：BootAlloc 并发注释改为单线程事实与 CAS 未来工作 | os/kernel/src/boot_alloc.rs 模块注释 | cargo check -p minix-arch -p minix-boot：exit 0 |
| DESIGN-15 | design-d2 | P1 | 文档/数字漂移 | ① | 已修：文档数字与注释同步（与 FACT-01/03/04/05/08 同批） | panel/design-d2.md DESIGN-15 | 文档五门在 L4 重跑 |
| FACT-01 | fact | P0 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-01 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-02 | fact | P0 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-02 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-03 | fact | P0 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-03 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-04 | fact | P0 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-04 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-05 | fact | P0 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-05 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-06 | fact | P0 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-06 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-07 | fact | P0 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-07 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-08 | fact | P0 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-08 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-09 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-09 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-10 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-10 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-11 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-11 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-12 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-12 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-13 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-13 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-14 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-14 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-15 | fact | P1 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-15 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-16 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-16 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| FACT-17 | fact | P2 | 文档事实 | ① | 已修：按 fact.md 的最小修复改写对应段（同批修订，见 loop.md §4） | panel/fact.md FACT-17 的 E1/E2 证据；L4 重跑文档五门 | 已重放 fact.md 中的 rg 命令 |
| READER-A-01 | reader-a | P0 | 结构/依赖清单 | ② | 转出：follow-ups.md D01-F12（§1.2 依赖边与逐条清单缺一条） | panel/reader-a.md READER-A-01 | 不适用（转出） |
| READER-A-02 | reader-a | P0 | 教学/链接顺序 | ② | 转出：follow-ups.md D01-F12（16 字节对齐论证依赖链接顺序，需链接脚本证据） | panel/reader-a.md READER-A-02 | 不适用（转出） |
| READER-A-03 | reader-a | P0 | 事实/NUL 语义 | ② | 转出：follow-ups.md D01-F12（mb_set_param 删除扫描的 NUL 语义需 C 源码复核） | panel/reader-a.md READER-A-03 | 不适用（转出） |
| READER-A-04 | reader-a | P0 | 事实/空槽顺序 | ② | 转出：follow-ups.md D01-F12（add_memmap 空槽填充顺序需 C 源码复核） | panel/reader-a.md READER-A-04 | 不适用（转出） |
| READER-A-05 | reader-a | P1 | 文档/分配接口 | ④ | 已修：§3.1 明确页分配 allocate_pages 与池分配 allocate_pool 两类 | rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md §3.1 | L4 文档五门重跑 |
| READER-A-06 | reader-a | P1 | 文档/未定义术语 | ④ | 已修：test-kernel-map 已定义；D1 闸改为描述性一致性检查 | 同篇 §3.4、§6.1 | L4 文档五门重跑 |
| READER-A-07 | reader-a | P1 | 文档/术语 | ④ | 已修：§3.1 补 [ARCH] 架构演进标注说明 | 同篇 §3.1 | L4 文档五门重跑 |
| READER-A-08 | reader-a | P1 | 文档/唯一落点 | ④ | 已修：read_required 收窄为内核 ELF 落点，模块另有 panic | 同篇 §3.2、§4.3 | L4 文档五门重跑 |
| READER-A-09 | reader-a | P0 | 文档/引文保真 | ④ | 已修：§3.8 register 引文补 release 守门（同 FACT-07） | 同篇 §3.8 | L4 文档五门重跑 |
| READER-A-10 | reader-a | P1 | 文档/计数矛盾 | ④ | 已修：四条→五条（同 FACT-16） | 同篇 §3.7 | L4 文档五门重跑 |
| READER-A-11 | reader-a | P2 | 文档/歧义 | ② | 转出：follow-ups.md D01-F12（BootFileTable 与 DTB 取模块的“或”歧义） | panel/reader-a.md READER-A-11 | 不适用（转出） |
| READER-A-12 | reader-a | P2 | 文档/空承诺 | ④ | 已修：§4.1 改为一段式入口加三段内部流程 | 同篇 §4.1 | L4 文档五门重跑 |
| READER-A-13 | reader-a | P2 | 文档/未定义术语 | ④ | 已修：§3.4 补 MapKey 定义 | 同篇 §3.4 | L4 文档五门重跑 |
| READER-A-14 | reader-a | P2 | 文档/悬空引用 | ④ | 已修：NR_BOOT_PROCS 回指改为 boot image 表 | 同篇 §4.3 | L4 文档五门重跑 |
| READER-A-15 | reader-a | P2 | 教学/证据不足 | ② | 转出：follow-ups.md D01-F12（kern_stack_top 与 L7 断言不等价） | panel/reader-a.md READER-A-15 | 不适用（转出） |
| READER-A-16 | reader-a | P2 | 文档/错误节号 | ④ | 已修：§7.5 三行错误节号已收窄 | 同篇 §7.5 | L4 文档五门重跑 |
| READER-B-01 | reader-b | P1 | 结构/边界声明 | ② | 转出：follow-ups.md D01-F13（§1.4 边界与第 3/4 章重叠） | panel/reader-b.md READER-B-01 | 不适用（转出） |
| READER-B-02 | reader-b | P2 | 结构/时序倒置 | ② | 转出：follow-ups.md D01-F13（§1.1 栈与 memmap 时序倒置） | panel/reader-b.md READER-B-02 | 不适用（转出） |
| READER-B-03 | reader-b | P1 | 教学/结论先于证据 | ② | 转出：follow-ups.md D01-F14（§1.2 结论先于证据、四次重复） | panel/reader-b.md READER-B-03 | 不适用（转出） |
| READER-B-04 | reader-b | P2 | 结构/括注歧义 | ② | 转出：follow-ups.md D01-F13（§1.2 箭头括注歧义） | panel/reader-b.md READER-B-04 | 不适用（转出） |
| READER-B-05 | reader-b | P3 | 结构/指代混乱 | ② | 转出：follow-ups.md D01-F13（§2.4.1 标题与首句指代混乱） | panel/reader-b.md READER-B-05 | 不适用（转出） |
| READER-B-06 | reader-b | P1 | 结构/判据矛盾 | ② | 转出：follow-ups.md D01-F13（§2.8 筛选判据自相矛盾） | panel/reader-b.md READER-B-06 | 不适用（转出） |
| READER-B-07 | reader-b | P2 | 结构/导航摘要 | ② | 转出：follow-ups.md D01-F13（§2 路线表只描述三分之一） | panel/reader-b.md READER-B-07 | 不适用（转出） |
| READER-B-08 | reader-b | P2 | 结构/引用未处理例外 | ② | 转出：follow-ups.md D01-F13（§3.1 引用 §2.7 未处理架构例外） | panel/reader-b.md READER-B-08 | 不适用（转出） |
| READER-B-09 | reader-b | P1 | 结构/标题与正文不一致 | ② | 转出：follow-ups.md D01-F13（§3.2 标题承诺与正文自述不一致） | panel/reader-b.md READER-B-09 | 不适用（转出） |
| READER-B-10 | reader-b | P1 | 教学/证据强于结论 | ② | 转出：follow-ups.md D01-F15（§3.4 探针结论引入候选集之外的第三因） | panel/reader-b.md READER-B-10 | 不适用（转出） |
| READER-B-11 | reader-b | P2 | 结构/重复表格 | ② | 转出：follow-ups.md D01-F13（§3.5 段落重复表格） | panel/reader-b.md READER-B-11 | 不适用（转出） |
| READER-B-12 | reader-b | P3 | 结构/偏离主题 | ② | 转出：follow-ups.md D01-F13（§3.6 第二个可核对点偏离主题） | panel/reader-b.md READER-B-12 | 不适用（转出） |
| READER-B-13 | reader-b | P2 | 结构/重述 | ② | 转出：follow-ups.md D01-F13（同一状态事实四五处重述） | panel/reader-b.md READER-B-13 | 不适用（转出） |
| READER-B-14 | reader-b | P2 | 结构/取舍未响应 | ② | 转出：follow-ups.md D01-F13（§2.6 取舍与 §4.6 未响应） | panel/reader-b.md READER-B-14 | 不适用（转出） |
| READER-B-15 | reader-b | P2 | 结构/错误回指 | ② | 转出：follow-ups.md D01-F13（§4.8 的 §4.1 引文回指不存在） | panel/reader-b.md READER-B-15 | 不适用（转出） |
| READER-B-16 | reader-b | P1 | 结构/计数矛盾 | ② | 转出：follow-ups.md D01-F16（三/五/四形态计数不一致） | panel/reader-b.md READER-B-16 | 不适用（转出） |
| READER-B-17 | reader-b | P1 | 结构/三表互不认 | ② | 转出：follow-ups.md D01-F17（§5.1/§5.3/§5.4 三表互不认） | panel/reader-b.md READER-B-17 | 不适用（转出） |
| READER-B-18 | reader-b | P2 | 结构/C 失败模式 | ② | 转出：follow-ups.md D01-F13（§6.3 前两行是 C 的失败模式） | panel/reader-b.md READER-B-18 | 不适用（转出） |
| READER-B-19 | reader-b | P2 | 教学/未标记合取 | ② | 转出：follow-ups.md D01-F13（§2.4.1 安全论证是未标记合取） | panel/reader-b.md READER-B-19 | 不适用（转出） |
| READER-B-20 | reader-b | P2 | 结构/从句藏约束 | ② | 转出：follow-ups.md D01-F13（§2.4.2 16 条截断只在从句出现） | panel/reader-b.md READER-B-20 | 不适用（转出） |
| READER-C-01 | reader-c | P1 | 文档/唯一落点 | ④ | 已修：同 READER-A-08 | panel/reader-c.md READER-C-01 | L4 文档五门重跑 |
| READER-C-02 | reader-c | P1 | 文档/引文保真 | ④ | 已修：同 FACT-07 | panel/reader-c.md READER-C-02 | L4 文档五门重跑 |
| READER-C-03 | reader-c | P1 | 文档/交接面矛盾 | ④ | 已修：§3.1 传递参数改为 arena 基址，KernelInfo 序列化进 arena | 同篇 §3.1 | L4 文档五门重跑 |
| READER-C-04 | reader-c | P1 | 教学/单形态结论 | ④ | 已修：§3.8 按已用部分扣除加自引导路径限定 | 同篇 §3.8 | L4 文档五门重跑 |
| READER-C-05 | reader-c | P1 | 教学/探针不同源 | ② | 转出：follow-ups.md D01-F15（探针形态与真机读数不同源） | panel/reader-c.md READER-C-05 | 不适用（转出） |
| READER-C-06 | reader-c | P1 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 结构/一致性项） | panel/reader-c.md READER-C-06 | 不适用（转出） |
| READER-C-07 | reader-c | P1 | 文档/归属矛盾 | ④ | 已修：§4.6 LAPIC/IO-APIC 归属改为 Step 2c | 同篇 §4.6 | L4 文档五门重跑 |
| READER-C-08 | reader-c | P1 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 一致性/指针项） | panel/reader-c.md READER-C-08 | 不适用（转出） |
| READER-C-09 | reader-c | P1 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 一致性/指针项） | panel/reader-c.md READER-C-09 | 不适用（转出） |
| READER-C-10 | reader-c | P2 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 一致性/指针项） | panel/reader-c.md READER-C-10 | 不适用（转出） |
| READER-C-11 | reader-c | P2 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 一致性/指针项） | panel/reader-c.md READER-C-11 | 不适用（转出） |
| READER-C-12 | reader-c | P2 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 一致性/指针项） | panel/reader-c.md READER-C-12 | 不适用（转出） |
| READER-C-13 | reader-c | P2 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 一致性/指针项） | panel/reader-c.md READER-C-13 | 不适用（转出） |
| READER-C-14 | reader-c | P2 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 一致性/指针项） | panel/reader-c.md READER-C-14 | 不适用（转出） |
| READER-C-15 | reader-c | P2 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 一致性/指针项） | panel/reader-c.md READER-C-15 | 不适用（转出） |
| READER-C-16 | reader-c | P2 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 一致性/指针项） | panel/reader-c.md READER-C-16 | 不适用（转出） |
| READER-C-17 | reader-c | P2 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 一致性/指针项） | panel/reader-c.md READER-C-17 | 不适用（转出） |

# Round 1（V2.1 全面板首轮；基线 7d638f774）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | fact | P1 | 文档/引文过期（F03 残留） | ① | 已修：§4.2 两处调用引文 10→8 参；同块 Serializer authority、pa_limit()、dumpdtb_blob 同步 | panel/round1/fact.md | L4 引文对账重跑 |
| FACT-02 | fact | P1 | 文档/字段表过期 | ① | 已修：§4.4 字段表 bootstrap 行改删除注记 | 同上 | 同上 |
| FACT-03 | fact | P1 | 文档/反事实前提失效 | ① | 已修：§4.4 校验表行删除；反事实段改"已无从谈起"时态并回指 §3.5 | 同上 | 同上 |
| FACT-04 | fact | P1 | 文档/不可复现错误消息 | ① | 已修：§6.3 行改历史注记（现行 grep 不到，排障勿按此找） | 同上 | 同上 |
| FACT-05 | fact | P2 | 文档/引文 expect 串 | ③ 误报 | 现行文档已是源码版本（"DM-admissible bound must sit above address zero"）；面板引自旧快照 | grep 零命中旧串 | — |
| FACT-06/07 | fact | P1 | 文档/测试计数漂移 | ① | 已修：125→124（opensbi 12→11）、175→173（handoff_v1 3→2、read 22→21），逐文件表与转录块同步，附成因注 | 实跑 cargo test 复测 | L4 重放 |
| FACT-08 | fact | P1 | 文档/已删测试仍列名 | ① | 已修：契约表行改删除注记 | 同上 | 同上 |
| FACT-09..11 | fact | P2 | 文档/章内计数矛盾 | ① | 已修：TODO-03 五枚→七枚、TODO-07 3→5 | 同上 | 同上 |
| FACT-12 | fact | P1 | 文档/引文门名不符 | ① | 已修：arch_boot_finish 引文 validate()→validate_before_tables()（含注释行） | lib.rs:224 | 同上 |
| FACT-13 | fact | P2 | 文档/计数维度 | ① | 已修：startup_objects 段"四条"改"五条腿四个写点"；kernel_info.rs 代码注释同步 | — | 同上 |
| FACT-14 | fact | P2 | 文档/里程碑串截断 | ① | 已修：补 `(high half)` 后缀 | lib.rs:358 | 同上 |
| FACT-15 | fact | P3 | 文档/节选归属 | ① | 已修：DEV_PIECE 改注释声明为文件级常量 | — | 同上 |
| READER-A-01/B-01/C-01 | readers | P0 | 文档/新旧叙事并存（F03 残留簇） | ④ | 已修：同 FACT-01..04 批（§4.2/§4.4/§5.2.1/§6.3 全部对齐删除态） | 三读者交叉同靶 | L4 回读 |
| READER-A-02 | reader-a | P0 | 文档/builder 缺字段 | ④ | 已修：§4.4 builder 引文补 `memmap_sources: &[]`/`startup_objects: &[]`（与源码一致——随 FACT-01 批核对） | lib.rs builder | 同上 |
| READER-A-03/C-02 | readers | P1 | 文档/§3.10 与 §3.9 矛盾 | ④ | 已修：§3.10 freepde 行改"先 Option 后退场" | — | 同上 |
| READER-A-04/C-06 | readers | P2 | 文档/回指落空 | ④ | 已修：§1.2"两个分支"回指改指 §2.4 LOAD_BSS 链 | — | 同上 |
| READER-A-05/B-05 | readers | P2 | 文档/编号无图例 | ④ | 已修：§3.4 前加编号图例块（D01-Fnn/D05/T2/Phase A.2） | — | 同上 |
| READER-B-03 | reader-b | P1 | 文档/计数维度冲突 | ④ | 已修：§4.8 开头改"五行并列+归并口径回指" | — | 同上 |
| READER-B-04 | reader-b | P1 | 文档/三条链 vs 五行 | ④ | 已修：§5.2.3 标题去计数、§3.10 收束句改"三类链+回指术语对账" | — | 同上 |
| READER-C-03 | reader-c | P1 | 文档/因果前提缺失 | ④ | 已修：§3.8 补 Rust 侧搬运时序前提（blob 留原地、VM 启动才搬；C 复制后才回收）；表注两种"段帧"说法统一 | 2.4.2 引文 | 同上 |
| READER-C-04 | reader-c | P1 | 文档/跨二进制钩子因果 | ④ | 已修：§6.2 第 4 条拆 shim/内核两腿（钩子每二进制一份，shim 侧恒未注册） | diagnostic.rs 静态量 | 同上 |
| READER-C-05 | reader-c | P1 | 文档/次序计数 | ④ | 已修：§2.8 括注改"步骤级五条+字段级一条"的完整清单 | — | 同上 |
| READER-C-07 | reader-c | P1 | 文档/隐含不变量 | ④ | 已修：§2.4.1 补"下标序=地址序"的成立条件与失效场景 | C 源回填语义 | 同上 |
| READER-C-08 | reader-c | P1 | 文档/守卫触发场景 | ④ | 已修：§4.8 点名 VMA=LMA 测试内核（link.ld 单行 `. = 0x80200000`）为相等场景 | link.ld | 同上 |
| READER-C-09 | reader-c | P1 | 文档/同段尺寸矛盾 | ④ | 已修：Step 2 反向推演改"按 Step 2 自己的尺寸，设备分片槽例外" | — | 同上 |
| READER-C-10/FACT-10/11 | 多源 | P2 | 文档/计数互斥 | ④ | 同 FACT-10/11 批 | — | 同上 |
| READER-C-11 | reader-c | P2 | 文档/覆盖声明过强 | ④ | 已修：M14 行补覆盖边界（路标出现≠内核存活，后者归下游门链） | — | 同上 |
| READER-C-20 | reader-c | P3 | 文档/测试名与省略声明表面冲突 | ④ | 已修：§3.4 补"测试传显式板名验证的是有来源分支；生产传 None" | boot_params.rs:209 | 同上 |
| READER-C-21 | reader-c | P3 | 文档/BIOS 链无着落 | ④ | 已修：改"固件未开分页形态（C 侧语义统一），本仓无 BIOS 产线" | — | 同上 |
| READER-C-22 | reader-c | P3 | 文档/同形掩盖差异 | ④ | 已修：param_buf 句改"仅就 ARCH 同形；BOARD 省略不再保证" | — | 同上 |
| READER-B-19 | reader-b | P3 | 文档/图面与文字 | ④ | 已修：§1.2 改"一条逆向箭头+一条文字标注依赖" | — | 同上 |
| READER-B-20 | reader-b | P3 | 文档/指涉不明 | ④ | 已修：§5.2.3 补三条路标的 EBS 前后归属 | — | 同上 |
| READER-A 术语组 | reader-a | P3 | 教学/首用未定义 | ④ | 已修：arena/信封/v1、RAMDS、medany、PML4 首用处就地展开 | — | 同上 |
| DESIGN-D2-01/02/03 | design-d2 | P1 | 设计/死抽象（F04/F05/F06 复核维持） | ② 带日期 | code-excellence 批次执行（用户既定裁决；D2 补充了执行面修正：build_memmaps 族被 QEMU 测试依赖不可删） | panel/round1/design-d2.md | 批次执行时复验 |
| DESIGN-D2-04 | design-d2 | P2 | 代码/非法状态（kern_stack_top 无上界） | ① | 本轮代码批修复 | design-d2.md | 测试 |
| DESIGN-D2-05 | design-d2 | P2 | 代码/非法状态（memmap 无序校验） | ① | 本轮代码批修复（FreeRangeSet::from_memmap 断言有序不相交） | 同上 | 测试 |
| DESIGN-D2-06 | design-d2 | P2 | 设计/store 收口时点 | ⑤ | 证据不足：需读 store_kernel_info 与 boot_validate_and_prepare 的完整时序，登记 follow-ups 带获取路径 | 同上 | 下轮 |
| DESIGN-D2-07 | design-d2 | P1 | 代码/三抄加性恢复体 | ① | 本轮代码批：合并 resume_additive<A, H> | 同上 | 测试+三架构编译 |
| DESIGN-D2-08 | design-d2 | P2 | 代码/clone 到能编译 | ① | 本轮代码批：删 Box::leak(to_string()) | loader.rs:344 | 测试 |
| 反证P2观察（resume 兜底） | refute | P3 | 代码/潜伏路径（boot_validate_and_prepare 兜底读发送方切片） | ⑤ | 证据不足：登记 follow-ups（当前被 resume_boot_pt_alloc 挡住）；BootAlloc 非线性化约束补进 §3.8 正文一句 | refute.md CLAIM 观察 | 下轮 |
| D1 方案组 | design-d1 | — | 设计提案 | ③/参考 | typestate/统一 BootError/BootMmu 抽取被 D2 依证据拒绝（多入口×三架构实况）；8 决策作 Round 2+ 重构参考素材，不直接实施 | panel/round1/design-d1.md | — |

# Round 2（收敛观察轮；基线 da984f70f）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-04 | fact | P0 | 代码/钉测试落后合并 | ① | 已修 `ac223745c`：store_before_alloc_pin 更新到 resume_additive 合并后结构 | 实跑 FAIL→PASS | 内核全量复绿 |
| FACT-05/06 | fact | P1 | 文档/归属与链形状 | ① | 已修 `ac223745c`：§3.7 引文头归 resume_additive；§4.8 收敛句改三委托壳 | lib.rs:328 | 五门重跑 |
| FACT-01/02/03/09 | fact | P1 | 文档/计数表第二张 | ① | 已修 `ac223745c`+本轮：逐文件转录块 opensbi 11/kernel_info 17/handoff 2/21；4→6 条之一 | grep -c 实测 | 同上 |
| FACT-07 | fact | P2 | 文档/引文无省略标记 | ① | 已修 `ac223745c`：§4.2 补约 35 行 startup 写入段的省略注记 | uefi_helpers:1059-1104 | 同上 |
| FACT-08 | fact | P1 | 文档/builder 引文缺两行 | ① | 已修 `ac223745c`：引文补 memmap_sources/startup_objects + 字段表补行 | lib.rs builder | 同上 |
| FACT-10/11/12 | fact | P3 | 文档/引文逐字与判据描述 | ① | 已修 `ac223745c`：vm/utility.c 逐字原文；alloc_lowest 判据三处命中；link.ld 措辞 | C 源 | 同上 |
| READER-A-01/B-19 | readers | P1 | 文档/GDB 清单查已删字段 | ④ | 已修：§6.4 第三层改现存字段+委托链，明示 bootstrap_len 非检查项 | — | 五门 |
| READER-A-02/B-18/C-04 | readers | P1 | 文档/三套计数互斥 | ④ | 同 FACT-01/02/03/09 批（转录块+算式行+契约表全部对齐 124/176） | 实测 | 同上 |
| REFUTE-③④ | refute | P2 | 文档/计数格过期+签名失真 | ① | 同上批 + bootstrap_alloc_limit() 无参签名修正 | uefi_helpers:635 | 同上 |
| REFUTE-① | refute | P2 | 文档/机制过强（TLB） | ① | 已修：改"冷取指"机制表述（三架构不因写表清 TLB） | refute.md | 五门 |
| REFUTE-② | refute | P2 | 设计/无守护前提（<4GiB） | ① | 已修：§6.3 补一行失败模式（boot-shim 镜像 >4GiB 无守护，OVMF 门链恒低于 4GiB） | refute.md | 同上 |
| REFUTE 焦点a | refute | P1 | 代码/resume 侧次序无钉 | ① | 已修：store_before_alloc_pin 增 validate < resume_boot_pt_alloc < delegate 序钉 | 实跑 PASS | 全量绿 |
| REFUTE 焦点b | refute | P2 | 设计/store 与 validate 收口序 | ③ | 反证失败：两序均安全（resume 体切片已落地、兜底死代码、validate 无页分配）；文档未主张同序，无冲突 | refute.md | — |
| D2-01/02/03 | design-d2 | P1 | 设计/死抽象稳定判定 | ② 带日期 | code-excellence 批次（F05 加强为"零生产实现"；F06 补 per-form 空转缺口；F04 拆分——UefiBootShim 零调用可删、trait 本体去留待裁决） | panel/round2/design-d2.md | 批次执行 |
| D2-10 | design-d2 | P3 | 工具/trait-audit 两类盲区 | ⑤（工作流记录项：入 meta/BLIND-SPOTS 候选，非文档缺陷） | 记入 BLIND-SPOTS 候选（宏展开 impl 假阳性、非 trait 抽象逃逸） | 同上 | meta |
| READER-C-01/02/03/05/06/08 + B 簇 + A 簇 | readers | P1-P3 | 文档/复述失真与导航 | ④ | 批二已修（本轮）：C-01 load_mods 表格行（exec_multiboot:759 实证）、C-02 直核家族限定、C-03 arena 四段布局注、C-05 双前缀+规划函数名统一、C-06 视角锚定句、C-08 aarch64 表址单通道、B-01 框图注、B-02 边界清单补 §2.3、B-03/C-09 三次出场、B-04/C-12 指向 §2.4.2、B-11 两道闸分工、B-15 链形状、A-03 可执行体/库消歧、A-07 跨镜像产线措辞、A-08 指针落点、A-18 升序机制修正（右余量落数组末尾，序不保证；alloc_lowest 按地址取不受影响）、A-04/A-05/A-14/A-17/A-19、B-06/B-13/B-14/B-17、C-07/C-10/C-11 | panel/round2/reader-*.md | 五门全绿+锚点 100/100 |
| B-05/07/08/09/16/20 | reader-b | P3 | 文档/冗余（七处） | ② 带日期 | 纯冗余收敛类，Round 3 与收敛判定合并处理（每处补权威指针或删一处复述；不动正确性） | panel/round2/reader-b.md | Round 3 |
| D2-04 | design-d2 | P2 | 代码/模块文档四处失真 | ① | 已修：boot-shim/src/lib.rs 与 boot_shim.rs 模块头按 §3.2 口径重写（trait 定义在 minix-boot；kernel 零引用；cfg 8 处实况；T2 注记更新；Why-a-trait 换四条收益带证据强度） | grep 实证 | 编译 0 错 |
| D2-05 | design-d2 | P1 | 设计/参数表 wire 分歧 | ① | 已修（比照 F10 先例）：doc §3.4 与 boot_params.rs 模块文档统一标 [ARCH: 参数表语义规约化]（重复键首条生效/内部键拒绝/BOARD 按源生成三处分歧；仓内消费者零命中已核） | panel/round2/design-d2.md | 五门 |

# Round 3（收敛判定轮；基线 f17c712b0）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| refute R3 四项 | refute | P2 | 文档/措辞与源注释 | ① | 已修 `f839e15b0`：§6.3 四段、§4.2 步骤4 alloc_arena、M2 措辞、handoff_v1 源注释 96 | refute.md | 五门 |
| REFUTE 终裁 a/b | refute | — | 遗留观察 | ③ | 反证失败（主张成立）：兜底可达性钉完整无第三路径；<4GiB 行措辞已闭合 | refute.md | — |
| FACT-01 | fact | P1 | 文档/契约表 kernel_info 行 9→17 | ① | 已修：rejects 8 + accepts 3 + requiring 4 + postcond 1 + getters 1 = 17；不变量表述与 §4.4 对齐 | grep 实测 | 五门 |
| FACT-04/05/07 | fact | P2 | 文档/引文保真 | ① | 已修：§4.2 自由变量来源注；§4.7 引文尾句换源码真句（validated not trusted）；§4.6 bullet1 旧注释改过去时指针 | grep 零命中 | 同上 |
| FACT-06/B-05 | fact+reader | P1 | 文档/MapKey 无定义 | ① | 已修：§4.2 补"MapKey 是什么"定义段（快照标识/失效/配对/拒绝重试） | UEFI 契约 | 同上 |
| FACT-08/09/11/12/13 | fact | P2-P3 | 文档/行号、cfg 口径、枚举、重复、编注 | ① | 已修：354→317 行；cfg 8 处口径改属性锚定表述；五条 panic 枚举两案 wrapping 补名；§3.9 wire 版本改指针；两条编注移出引文块 | 实测 | 同上 |
| 断锚簇 B-01/C-03/A-08、C-06/B-03、B-02、B-04、B-05 | readers | P1 | 文档/回指断裂 | ① | 已修：三类链定义搬进对账（§3.10 空锚消除）；boot_image 前 8 项改 §4.3；§3.4→§3.7 BootAlloc 引证；MapKey 见上；图例闭合（O-11/D01-R1/R2/D05/T2/Phase A.2 全覆盖+各自回指） | grep | 同上 |
| B-06/A-08 链形状 | readers | P1 | 文档/接收入口双名 | ① | 已修：§3.7/§4.7/§6.1 三处改"本架构委托壳（aarch64 resume_high_half / x86-riscv resume_after_v1_decode）"；§3.10 收敛句同步双收敛点 | lib.rs | 同上 |
| C-04/B-13 | reader-c | P1 | 文档/收敛主张被自括号推翻 | ① | 已修：§4.8 改双收敛点表述+发送侧终点 br {entry} | §3.7 引文 | 同上 |
| C-01/02、C-05、C-07 | reader-c | P2 | 文档/计数因果与成员集 | ① | 已修：minix-boot 因果账闭合（17 计法）；handoff 3→2；真机门链计法入对账+§6.4 同步；§3.3 riscv64 句对齐权威表 | 实测 | 同上 |
| C-10 | reader-c | P2 | 文档/兜底与保留表相抵 | ① | 已修：补 LOADER_DATA 落保留表一侧的限定 | vm_handoff 注释 | 同上 |
| B-14 | reader-b | P2 | 文档/三陈述组合互斥 | ① | 已修（源码核实后为表述缺口）：§4.6 补"分片在 Step 1 自循环决定、窗口部位只写一次 2MiB 叶"——无先 1GiB 后 2MiB 次序；Step 2c 另腿不共享槽位判定 | lib.rs:900-967 | 同上 |
| A-08 等读者残余（§4.5 四个容量、125→124/175→176 算式句、M14 路标名、§4.9 孤例、§1.2/§2.8 三层推演重叠、§4.2↔§6.1 双份诊断串、孤立符号 vmctl_set_addr_space、B-15/16 脚本指针、B-17 收口块、B-18 里程碑标注） | readers | P3 | 文档/残余打磨 | ② 带日期 | 登记随下篇或 D01 维护批（全部为局部打磨，不接触论证链；其中三处与 B-06 链形状修复重叠已顺带闭合） | panel/round3/reader-*.md | 维护批 |
| D2-01/02/05/06 | design-d2 | P2 | 文档引文同步/注释口径 | ① 待修-小 | 登记维护批（§4.3 Box::leak 引文块未跟、boot_shim.rs 模块头 3-5 行残留、输入假设显式化两处） | panel/round3/design-d2.md | 维护批 |
| D2-03 | design-d2 | P2 | 设计/DTB 乱序输入 | ⑤→用户 | 乱序 DTB /memory 会被有序断言拒启（fail-closed，非静默错）——声明子集约束或加排序，随用户裁决 | 同上 | 裁决 |
| D2-04 | design-d2 | P1 | 设计/PagingWithId 去留 | ② 带日期 | 维持 code-excellence 批次（R3 独立裁决加强：零生产实现、文档零提及） | 同上 | 批次 |

## 处置索引（逐 ID 补充，Round 3 收敛判定轮追加）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| DESIGN-D2-02 | design-d2(R2) | P1 | 设计/BootForm 删除建议 | ② 带日期 | code-excellence 批次；R3 d2 独立复核改判"接受保留"（记账职责真实、形态盲电池已自报）——两轮裁决并存，批次执行时以 R3 复核为准 | panel/round2/design-d2.md、panel/round3/design-d2.md | 批次执行 |
| DESIGN-D2-09 | design-d2(R2) | P3 | 设计/D1"错误消息逐字对齐 C"提案 | ③ | 接受现状：R2 d2 定性确认 D1 提案落选（boot 失败即停机、逐字对齐无收益）；与 refute R3 的 C 侧逐字命中互证 | panel/round2/design-d2.md | — |
| DESIGN-D2-01 | design-d2(R2) | P1 | 设计/PagingWithId 零生产实现（F05 加强） | ② 带日期 | code-excellence 批次（R3 d2 独立复核同判"建议删除"） | panel/round2/design-d2.md、panel/round3/design-d2.md | 批次执行 |
| DESIGN-D2-03 | design-d2(R2) | P1 | 设计/BootShim trait 文档三理由失实（F04 强度重定） | ② 带日期 | code-excellence 批次；模块头失实部分已在 R2 批二 D2-04 修复，trait 本体去留随 UefiBootShim impl 删除后再评估（R3 d2 判"接受保留"） | 同上 | 批次执行 |
| DESIGN-D2-10 | design-d2(R2) | P3 | 工具/trait-audit 两类盲区（宏 impl 假阳性、enum 逃逸） | ⑤（工作流记录项：入 meta/BLIND-SPOTS 候选，非文档缺陷） | 已记入 meta R3 报告与 BLIND-SPOTS 候选（R3 d2 独立复核同发现） | panel/round2/design-d2.md、panel/round3/design-d2.md | meta 已采集 |
| READER-A-17 | reader-a(R2) | P2 | 文档/wire 双版本轴未对表 | ④ | 已修：§3.4 补"协议身份 v1 vs 信封版本号（当前 8）"两层版本号段 | panel/round2/reader-a.md | 五门 |
| READER-A-18 | reader-a(R2) | P1 | 文档/升序机制表述错误 | ④ | 已修：§2.4.1 改"右余量落数组末尾空槽、序不保证；alloc_lowest 按地址取 min 不依赖序" | 同上 | 同上 |
| READER-A-19 | reader-a(R2) | P3 | 文档/仓库内库称"第三方" | ④ | 已修：§3.2 改"本仓库的共享库 crate minix-boot" | 同上 | 同上 |
| READER-A-20 | reader-a(R2) | P3 | 文档/同句自重复+孤立符号 vmctl_set_addr_space | ② 带日期 | 维护批（局部打磨；与 §4.1 括注重叠已部分收敛） | 同上 | 维护批 |
| READER-C-18 | reader-c(R1) | P2 | 文档/bootstrap_alloc_limit 签名不一致 | ④ | 已修：R3 refute 批把步骤 4 改为 alloc_arena() 经 UEFI allocate_pages(MaxAddress(bootstrap_alloc_limit()))——无参签名与引文一致 | panel/round1/reader-c.md | 五门重跑 |
| READER-C-19 | reader-c(R1) | P3 | 文档/24 字节归因不闭合 | ④ | 已修（信息在 §3.9 可连读；R3 批二把 §3.9 的 wire 版本句改指针后两节连读路径仍在）——补一句显式归因见维护批 | 同上 | 维护批补一句 |

# Round 4（清积压后首轮；基线 194e6c8c2）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | fact | P1 | 文档/入口寄存器三腿 | ① | 已修：§3.7 改 rbx→rdi / x0 / a0 三腿 + 段首三腿限定 | `fc1faecd5` | 五门 |
| FACT-02 | fact | P1 | 文档/计数归因 | ① | 已修：git 逐 commit 复算（F03 −3、F07 +1、D2 +3 → 176/124） | 同上 | 同上 |
| FACT-03 | fact | P1 | 文档/entering 行腿数 | ① | 已修：§6.1 标三条 v1 发送腿 | 同上 | 同上 |
| FACT-04 | fact | P1 | 文档/v0–v4 两次打印 | ① | 已修：§6.1 行注两次打印 + §6.3 最后一行推演改发送侧 v4 | 同上 | 同上 |
| FACT-05 | fact | P2 | 文档/arena 先后序 | ① | 已修：§4.1 准备链改 root/bump→装载→平台源→arena | 同上 | 同上 |
| FACT-06 | fact | P2 | 文档/dmp_kernel 字段数 | ① | 已修：四字段归 dmp_kernel、三字段归 VM | 同上 | 同上 |
| FACT-07 | fact | P2 | 文档/adjudicate 五步 | ① | 已修：§4.2 五步与源码 Step 1–5 对齐（含 RESERVED+WRITE_BACK 反例、RefuseToBoot） | 同上 | 同上 |
| FACT-08 | fact | P2 | 文档/DTB 解析事实 | ① | 已修：§3.5 行改"建内存表、读 bootargs、传 PlatformDescSource" | 同上 | 同上 |
| FACT-09 | fact | P2 | 文档/门链集合与脚本 | ① | 已修：五条=表格五行（含 uboot-shim），aarch64 直核注明不计入；§6.4 补脚本 | 同上 | 同上 |
| FACT-10 | fact | P3 | 文档/委托链精度 | ① | 已修：三架构直接壳均 resume_after_v1_decode，aarch64 再转 high_half | 同上 | 同上 |
| FACT-11 | fact | P2 | 源码注释/aarch64 only | ① | 已修：resume_additive 注释改三架构 | 同上 | 编译绿 |
| FACT-12 | fact | P3 | 文档/M14 路标字符串 | ① | 已修：handing control → [raw] entering kernel image v1 | 同上 | 同上 |
| DESIGN-D2-09 | design-d2 | P1 | 源码注释/入口顺序与寄存器 | ① | 已修：uefi_helpers 入口注释改 x86_64/aarch64 双腿 + 顺序 + rbx/x0 | `194e6c8c2`+`fc1faecd5` | 编译绿 |
| DESIGN-D2-10 | design-d2 | P1 | 源码注释/表来源与入口 | ① | 已修：opensbi_helpers 模块头改 bootelf-argv + shim 自建表 | 同上 | 同上 |
| DESIGN-D2-11 | design-d2 | P2 | 源码注释/硬编码内存图 | ① | 已修：lib.rs opensbi 描述改 DTB /memory | 同上 | 同上 |
| DESIGN-D2-12 | design-d2 | P1 | 源码注释/reserved 范围 | ① | 已修：build_memmap 文档改五类型映射口径 | 同上 | 同上 |
| DESIGN-D2-13 | design-d2 | P2 | 源码注释/五载荷 | ① | 已修：handoff_v1_read 改 seven payloads | 同上 | 同上 |
| DESIGN-D2-14 | design-d2 | P2 | 源码注释/快照时点 | ① | 已修：kernel_info 注释改分腿事实（UEFI 后拍/自引导腿含占用） | 同上 | 同上 |
| DESIGN-D2-15 | design-d2 | P2 | 文档/五步口径 | ① | 已修（与 FACT-07 同靶，采源码口径） | 同上 | 同上 |
| DESIGN-D2-16 | design-d2 | P2 | 源码注释/模块分配模型 | ① | 已修：Real-hardware 段改 LOADER_DATA 事实 | 同上 | 同上 |
| READER-A-01 | reader | P0 | 文档/§1.2 三层依赖密度 | ④ | 已修：压成一句+回指 §2.8（去重） | 同上 | 五门 |
| READER-A-02 | reader | P0 | 文档/16 字节对齐消费者 | ④ | 已修：补 ABI 消费者与 #GP 后果、编译选项限定 | 同上 | 同上 |
| READER-A-03 | reader | P0 | 文档/ARM overlaps 锚点 | ④ | 已修：补 earm/pre_init.c:overlaps 锚点 | 同上 | 锚点门 |
| READER-A-04 | reader | P0 | 文档/ram_authority 未定义 | ④ | 已修：wire 引文加编注定界 | 同上 | 五门 |
| READER-A-05 | reader | P2 | 文档/arena 分区计数 | ④ | 已修：七载荷分区列名 + 信封+四段相加口径 | 同上 | 同上 |
| READER-B-01 | reader | P3 | 文档/跨侧类型论证 | ④ | 已修（与 C-09 同靶）：KernelInfo 跨侧、接口类型引导侧 | 同上 | 同上 |
| READER-B-02 | reader | P3 | 文档/框图双向依赖 | ④ | 已修：箭头注拆"依赖 A/B"两行 | 同上 | 同上 |
| READER-B-03 | reader | P1 | 文档/§3.1 列名 OpenSBI | ④ | 已修：列名改 U-Boot shim 路径 + 直核指向 §3.7 | 同上 | 同上 |
| READER-B-04 | reader | P1 | 文档/三类链循环指认 | ④ | 已修：定义收进 §3.7 对账为唯一处，§4.1 只按两列 | 同上 | 同上 |
| READER-C-01 | reader | P1 | 文档/门链五条集合 | ④ | 已修（与 FACT-09 同靶） | 同上 | 同上 |
| READER-C-02 | reader | P1 | 文档/§2.8 证据数 | ④ | 已修：两条在此配证据、三条在 §1.2/§2.6/§2.6.1 | 同上 | 同上 |
| READER-C-03 | reader | P1 | 文档/§3.8 第四写点 | ④ | 已修：换手表 Step 0 行并入接收镜像 resume_additive 注册 | 同上 | 同上 |
| READER-C-04 | reader | P1 | 文档/riscv 反事实范围 | ④ | 已修：限定非 U-Boot 特性直核入口 | 同上 | 同上 |
| READER-C-05 | reader | P1 | 文档/计数轴 | ② 带日期 | R5 fact 复核（B-04/术语对账重写后需观察一轮） | panel/round4/reader.md | R5 |
| READER-C-06 | reader | P1 | 文档/M1 证据类型 | ④ | 已修：引文对账、删真机兜底、注明真机不覆盖 C head.S | 同上 | 同上 |
| READER-C-07 | reader | P1 | 文档/M3 覆盖强度 | ④ | 已修：改部分覆盖+源码序钉说明 | 同上 | 同上 |
| READER-C-08 | reader | P1 | 文档/pg_map USER | ④ | 已修：补用户态消费与 pg_identity 对照 | 同上 | 同上 |
| READER-C-09 | reader | P1 | 文档/跨侧类型论证 | ④ | 已修（与 B-01 同靶） | 同上 | 同上 |
| READER-C-10 | reader | P3 | 文档/24 字节归因 | ④ | 已修：§3.5 补 free_upper_idx 8 字节 | 同上 | 同上 |
| READER-C-12 | reader | P2 | 文档/§1.2 四条编号 | ④ | 已修：四条编号+单列一条=五条 | 同上 | 同上 |

# Round 5（基线 6fecc5a92）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| DESIGN-D2-09 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-10 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-11 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-12 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-13 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-14 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-15 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-16 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-17 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-18 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-19 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-20 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-21 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-22 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| DESIGN-D2-23 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-01 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-02 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-03 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-04 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-05 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-06 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-07 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-08 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-09 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-10 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-11 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-12 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-13 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-14 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| FACT-15 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-A-01 | panel-r5 | P0 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-A-02 | panel-r5 | P0 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-A-03 | panel-r5 | P0 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-A-04 | panel-r5 | P0 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-B-01 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-B-02 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-B-03 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-B-04 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-01 | panel-r5 | P1 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-02 | panel-r5 | P1 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-03 | panel-r5 | P1 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-04 | panel-r5 | P1 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-05 | panel-r5 | P1 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-06 | panel-r5 | P1 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-07 | panel-r5 | P1 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-08 | panel-r5 | P1 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-09 | panel-r5 | P1 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-10 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-11 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-12 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-13 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-14 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-15 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-16 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-17 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-18 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-19 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-20 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-21 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-22 | panel-r5 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |

# Round 6（基线 d4915ae1f）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| DESIGN-D2-01 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-02 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-03 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-04 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-05 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-06 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-07 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-08 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-09 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-10 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-11 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-12 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-13 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-14 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| DESIGN-D2-15 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| R6-01 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| R6-02 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| R6-03 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| R6-04 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| R6-05 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| R6-06 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| R6-07 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| R6-08 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| R6-09 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| R6-10 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| R6-11 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| R6-12 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| R6-13 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-A-01 | panel-r6 | P0 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-A-02 | panel-r6 | P0 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-A-03 | panel-r6 | P0 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-A-04 | panel-r6 | P0 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-A-05 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-A-06 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-01 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-02 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-03 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-04 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-05 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-06 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-07 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-08 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-09 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-10 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-11 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-01 | panel-r6 | P1 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-02 | panel-r6 | P1 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-03 | panel-r6 | P1 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-04 | panel-r6 | P1 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-05 | panel-r6 | P1 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-06 | panel-r6 | P1 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-07 | panel-r6 | P1 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-08 | panel-r6 | P1 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-09 | panel-r6 | P1 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-10 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-11 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-12 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-13 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-14 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-15 | panel-r6 | P2/P3 | 文档/源码注释同步 | ④ | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |

## Round 6 fact 明细（R6-NN 逐 ID）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| R6-01 | fact | P1 | 文档/§5.4 与 M6 解绑不同步（SELF-INFLICTED） | ① | 已修：§5.4 改"M5 与 M13 已登记；M6 未单独登记 TODO" | `6afb501c6` | 五门 |
| R6-02 | fact | P1 | 源码注释/reserved_regions 旧文案 | ① | 已修：访问器改"可用但不可分配、Direct Map 候选" | 同上 | 编译绿 |
| R6-03 | fact | P1 | 文档/fw_end=0 过头 | ① | 已修：两处改"仅关前缀裁剪，/reserved-memory 仍进表（典型为空）" | 同上 | 五门 |
| R6-04 | fact | P2 | 文档/接收链两处互斥 | ① | 已修：§4.8 统一"三架构先经 decode"；§6.4 箭头串联 | 同上 | 同上 |
| R6-05 | fact | P2 | 文档/TODO-07 重复标签（SELF-INFLICTED） | ① | 已修：删重复"5 条：" | 同上 | 同上 |
| R6-06 | fact | P2 | 文档/jalr→jr（SELF-INFLICTED） | ① | 已修：两处跳转助记符改 `jr {entry}` | 同上 | 同上 |
| R6-07 | fact | P2 | 文档/最后一行边界（SELF-INFLICTED） | ① | 已修：补 Step1/2 挂住情形 | 同上 | 同上 |
| R6-08 | fact | P2 | 文档/memmap 可分配与未过滤冲突 | ① | 已修：行内注明"未预过滤、先扣占用区" | 同上 | 同上 |
| R6-09 | fact | P2 | 源码注释/five payloads→seven | ① | 已修：lib.rs:518 | 同上 | 编译绿 |
| R6-10 | fact | P2 | 源码注释/ld staging 头过期 | ① | 已修：ld 头对齐 uboot_layout 常量 | 同上 | 同上 |
| R6-11 | fact | P2 | 文档/辅助八步漏 collect_param_buf | ① | 已修：八步补"归一化 param_buf" | 同上 | 五门 |
| R6-12 | fact | P2 | 文档/aarch64 第三形态标记 | ① | 已修：补 DTB_CFGTABLE=1 形态 | 同上 | 同上 |
| R6-13 | fact | P2 | 源码注释/锚点 161→162（SELF-INFLICTED） | ① | 已修：loader.rs NoLoadSegments 行号 | 同上 | 编译绿 |

# Round 7（基线 6a9afd065）

## R7 d2 明细（DESIGN-D2-NN 逐 ID）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| DESIGN-D2-01 | panel-r7 | P1 | 代码/aarch64 startup 记录借用 riscv bump 池常量 | ① | 已修：`bootface_a64.rs` 改 `BUMP_BASE`/`BUMP_BASE+BUMP_SIZE`，删 riscv 访问器导入 | `os/kernel-image/src/bootface_a64.rs`；`cargo check -p kernel-image` 0 错 | 编译绿 |
| DESIGN-D2-02 | panel-r7 | P1 | 代码/内核重叠防线只在无调用方函数、事后检查、未页取整 | ①（预防面转出） | 已修：`record_kernel_span` + `alloc_module_pages` 拷贝前拒绝；两条入口加页取整重查；同族自查补 root/bump 装载后复核（预防面"先装内核再分配"留 follow-up）；§4.2 步骤 2、§5.1 M16 同步 | `uefi_helpers.rs`；`cargo test -p boot-shim --features test-all` 124+5 | 测试+五门 |
| DESIGN-D2-03 | panel-r7 | P1 | BootForm 三处断言不成立 | ①（结构取舍待裁决） | 已修：三处断言改为"注册表不追踪真实入口/电池只查规范几何"；保留/删除/接线需用户裁决 | `os/libs/minix-boot/src/boot_form.rs` | 测试绿 |
| DESIGN-D2-04 | panel-r7 | P0 | 源码注释/reserved_regions 说两条腿传空表 | ① | 已修：字段文档按四腿写真值（UEFI 空；U-Boot/riscv 直核非空；aarch64 直核通常空） | `kernel_info.rs:45` | 编译绿 |
| DESIGN-D2-05 | panel-r7 | P1 | 源码注释/表归属 7 处仍归 U-Boot；a2 误标 | ① | 已修：opensbi_helpers 七处 + bootface.rs 两处 + 测试脚本头改"shim 自建表/QEMU 直载 a2" | 见源码注释 | 编译绿 |
| DESIGN-D2-06 | panel-r7 | P1 | 源码注释/两处"五个载荷" | ① | 已修：boot-shim main.rs 与 kernel-image main.rs 改"七载荷分区" | 同左 | 编译绿 |
| DESIGN-D2-07 | panel-r7 | P1 | 源码注释/build_identity_windows 三段互斥 + 不存在常量 | ① | 已修：并成一段退役说明，删 `USER_IDENTITY_FLOOR`；测试改名 `test_identity_windows_are_retired` | `vm_handoff.rs` | 测试绿 |
| DESIGN-D2-08 | panel-r7 | P1 | 源码注释/引用不存在的 `opensbi_helpers::DRAM_BASE` | ① | 已修：改指 `MODULE_REGION_BASE` 的同基推导 | `bootface.rs` | 编译绿 |
| DESIGN-D2-09 | panel-r7 | P2 | 源码注释/Real-hardware 段说覆盖 arena | ① | 已修：改为"本函数只覆盖 root/bump；arena 在另一入口" | `uefi_helpers.rs` | 编译绿 |
| DESIGN-D2-10 | panel-r7 | P2 | 源码注释/16 KiB arena 旧数 | ① | 已修：改 36 KiB（九页）+96 B spare | `handoff_v1.rs` | 测试绿 |
| DESIGN-D2-11 | panel-r7 | P1 | 源码注释/最高页拒绝归因到记录层 | ① | 已修：改为投影层 `MemoryRegion` 宽度限制，记录层为 128 位 | `memmap_source.rs` | 编译绿 |
| DESIGN-D2-12 | panel-r7 | P2 | 源码注释/序列化器分区清单漏 source/startup | ① | 已修：模块文档补两项独立分区 | `handoff_v1_write.rs` | 编译绿 |
| DESIGN-D2-13 | panel-r7 | P2 | 文档/§3.4"四段分区"与引文非原文 | ① | 已修：改五组相加明细并标注该注释为改述 | 文档 §3.4 | 五门 |

## R7 fact 明细（终版 FACT-NN 逐 ID）

> fact 面板在审查期间自我收窄：首版 16 条 + 2 溢出，终版（07:47）保留 11 条；首版的其余 7 条已随 07:43/07:45 修订解决（面板头 "并发修订复核" 逐项列明）。下表为终版 11 条。

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | panel-r7 | P2 | 文档/M16 "宿主无法构造"不成立 | ① | 已修：新增 5 条纯函数宿主测试（相离/相切/包含/部分重叠/页取整）；M16 改"纯函数宿主可测，仅 AllocatePages 分配面不可构造" | `uefi_helpers.rs` 新 5 测试；boot-shim 129+5 | 测试+五门 |
| FACT-02 | panel-r7 | P2 | 文档/§4.8 把 U-Boot 一族整体算进 v1 路径 | ① | 已修：改为"U-Boot 一族的 shim 生产腿；直载测试链不参与交接路径计数" | 文档 §4.8 | 五门 |
| FACT-03 | panel-r7 | P3 | 文档/§4.6 尾句重复（SELF-INFLICTED） | ① | 已修：删重复句 | 文档 §4.6 | 五门 |
| FACT-04 | panel-r7 | P3 | 文档/§4.2 越界界把 Side 排除 | ① | 已修：改"凡过 Step 3 的描述符（含 Side）都过 checked_len；上界是 pa_limit" | 文档 §4.2 | 五门 |
| FACT-05 | panel-r7 | P3 | 文档/§4.2 引文漏新断言且无省略标记 | ① | 已修：引文补省略标记并指步骤 2/M16 | 文档 §4.2 | 五门 |
| FACT-06 | panel-r7 | P3 | 文档/§5.2.1 转录顺序与 for 列表不一致 | ① | 已修：注明输出按分组重排、数字逐字来自实跑 | 文档 §5.2.1 | 五门 |
| FACT-07 | panel-r7 | P3 | 文档/§4.3 代码块中文译注未声明改述 | ① | 已修：块头补"块内中文注释为改述/译述" | 文档 §4.3 | 五门 |
| FACT-08 | panel-r7 | P3 | 文档/§5.2.3 "五条链都只到入口"过宽 | ① | 已修：限定三条 riscv64/aarch64 链；x86 与 U-Boot shim 含 entering scheduler | 文档 §5.2.3 | 五门 |
| FACT-09 | panel-r7 | P3 | 文档/测试名只给后缀 | ① | 已修：M13/TODO-07/§5.2.1 统一完整 `test_bootstrap_outside_memmap_*` 名 | 文档 §5.1/§5.2.1/§5.3 | 五门 |
| FACT-10 | panel-r7 | P3 | 文档/§3.4 memmap 指针错指 §4.5 | ① | 已修：改"§3.8 扣除表；扣减实现在 vm_handoff::classify" | 文档 §3.4 | 五门 |
| FACT-11 | panel-r7 | P3 | 文档/M16 "不走 alloc_module_pages"同名歧义 | ① | 已修：点名 U-Boot shim 的同名函数与直核 place_modules，均无内核重叠拒绝 | 文档 §5.1 M16 | 五门 |

## R7 reader 明细（终版 READER-C-NN 逐 ID）

> 面板实例在审查期间自我重读：初版 31 条（07:26）→ 中间版 11 条（07:42）→ 终版 2 条（07:45）。前两版的发现均随修订解决或并入终版，逐条修复见 `c85a93837` 与 R7 后续小批；下表只记终版 2 条。

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| READER-C-01 | panel-r7 | P1 | 文档/§4.4 表缺页大小能力/物理上限行 | ④ | 已修：表补 `detect_boot_page_support` 成功且 `phys_addr_limit() >= IDENTITY_MAP_END` 一行 | 文档 §4.4 | 五门 |
| READER-C-02 | panel-r7 | P1 | 文档/§4.4 表用未定义符号 `entry_start_pa` | ④ | 已修：改自然语言"镜像内入口/栈顶偏移写错，或用户栈上界越入内核半区" | 文档 §4.4 | 五门 |

# Round 8（基线 1ceaf539d）

## R8 面板明细（39 条）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-R8-01 | panel-r8 | P1 | 文档/§5.2.1 计数未随 5 条新测试回填 | ① | 已修：124→129、6→11、113→118、求和式与“该文件 11 条”同步；历史补 R7 D2-02 五条负控 | 文档 §5.2.1 | 五门+测试 |
| FACT-R8-02 | panel-r8 | P2 | 文档/§2.2 把入口 esp≡0 当 i386 SysV ABI 要求 | ① | 已修：改“入口 %esp+4 是 16 的倍数；本布局 esp≡0，差 4 字节”，保留编译选项免责 | 文档 §2.2 | 五门 |
| FACT-R8-03 | panel-r8 | P2 | 源码注释/main.rs 仍写 kerninfo 每绑定注入 | ① | 已修：改历史陈述 + D72VM 已移交 VM；删 per-bind 乘法与“修复未落地” | os/boot-shim/src/main.rs | 编译绿 |
| FACT-R8-04 | panel-r8 | P2 | 文档/§4.5 称 kmain 会再存一次 store_kernel_info | ① | 已修：改“守卫防未来同址调用”；lib.rs 两处 kmain 注释同步改写 | 文档 §4.5；os/kernel/src/lib.rs | 测试+五门 |
| FACT-R8-05 | panel-r8 | P3 | 源码注释/vm_server 行号锚点失效 | ① | 已修：改指 boot_validate_and_prepare，不绑行号 | os/servers/vm/src/vm_server.rs | 编译绿 |
| READER-R8-F01 | panel-r8 | P2 | 文档/§3.7 三条入口收敛点与 §4.8 矛盾 | ④ | 已修：分直核 arch_boot_finish 与接收 arch_boot_resume_v1→resume_additive；发送侧不收敛 | 文档 §3.7 | 五门 |
| READER-R8-F02 | panel-r8 | P2 | 文档/§2.8 N 同时指 mmap 条目数与模块数 | ④ | 已修：拆 R（mmap 条目数）与 M（模块数）；箭头改“交错” | 文档 §2.8 | 五门 |
| READER-R8-F03 | panel-r8 | P2 | 文档/§4.2 九步漏 startup 分区写入 | ④ | 已修：表后注明表外动作（EBS 后补写 canonical 五类） | 文档 §4.2 | 五门 |
| READER-R8-F04 | panel-r8 | P2 | 文档/Step 0-4 编号全文使用无定义 | ④ | 已修：§4.6 开头补五行阶段表（含 2b/2c 归属） | 文档 §4.6 | 五门 |
| READER-R8-F05 | panel-r8 | P2 | 文档/§4.6 AlreadyMapped 与 AllocationFailed 成因写串 | ④ | 已修：可达类=AllocationFailed；AlreadyMapped 结构性排除，各自理由分开 | 文档 §4.6 | 五门 |
| READER-R8-F06 | panel-r8 | P2 | 文档/流程代号密度与读者画像冲突；O-11 仍在用 | ④ | 已修：图例明确可跳过、删编号后句意完整；O-11 改“两快照纪律” | 文档 §3.4/§4.1 | 五门 |
| READER-R8-F07 | panel-r8 | P3 | 文档/§3.4 “删掉的不是信息”与退役三字段矛盾 | ④ | 已修：段首改“多数换载体，少数退役”，保留三类归位 | 文档 §3.4 | 五门 |
| READER-R8-F08 | panel-r8 | P3 | 文档/§2.4.2 结算锚点不含 16 条截断 | ④ | 已修：改指 §2.4.1 末“另一件必须单说的事”段 | 文档 §2.4/§3.4 | 五门 |
| READER-R8-F09 | panel-r8 | P3 | 文档/§1.4 kmain 校验归给 §3.5 | ④ | 已修：改指 §4.4 门链 | 文档 §1.4 | 五门 |
| READER-R8-F10 | panel-r8 | P3 | 文档/ram_authority 注释指 §4.2 无 source 审计 | ④ | 已修：改指 §3.4 规范集合与 §4.4 strict 门 | 文档 §3.4 | 五门 |
| READER-R8-F11 | panel-r8 | P3 | 文档/两处“§2.4 升序讨论”层级错 | ④ | 已修：两处改 §2.4.1 | 文档 §2.4.1 | 五门 |
| READER-R8-F12 | panel-r8 | P3 | 文档/§2.6.5 “三张表”后写“两张表” | ④ | 已修：改“CR0/CR4 表里” | 文档 §2.6.5 | 五门 |
| READER-R8-F13 | panel-r8 | P3 | 文档/逐页累加与整段累加自相矛盾 | ④ | 已修：前半句拆两种粒度，删末尾重复更正 | 文档 §2.4.1 | 五门 |
| READER-R8-F14 | panel-r8 | P3 | 文档/§4.1 “步骤 2 的 init_boot_pt_alloc”编号悬空 | ④ | 已修：改动作名“prepare_and_enter_kernel_image 开头那次” | 文档 §4.1 | 五门 |
| READER-R8-F15 | panel-r8 | P3 | 文档/§3.8 首行不写注册槽却计入“写点” | ④ | 已修：改“一次初始化 + 三次注册”；首行标“不写注册槽” | 文档 §3.8 | 五门 |
| READER-R8-F16 | panel-r8 | P3 | 文档/961..1023 与 961+k 边界差一格 | ④ | 已修：改“0–1023（含 1023）”与“961+k-1” | 文档 §2.6 | 五门 |
| READER-R8-F17 | panel-r8 | P2 | 文档/接收公共段 high_half vs resume_additive 两说 | ④ | 已修：次序句改“各架构委托壳”；§6.1 表注明两行仅 aarch64 壳打印 | 文档 §4.7/§6.1 | 五门 |
| READER-R8-F18 | panel-r8 | P3 | 文档/§3.3 依赖图漏 8 条边（含反向库依赖） | ④ | 已修：图下注明简化图、条件依赖见四表 | 文档 §3.3 | 五门 |
| READER-R8-F19 | panel-r8 | P3 | 文档/“跨镜像两腿”与发送腿术语冲突 | ④ | 已修：改“发送/接收两端” | 文档 §3.7 | 五门 |
| READER-R8-F20 | panel-r8 | P3 | 文档/§5.2.3 “这条 U-Boot 链”指代不明；脚本不在清单 | ④ | 已修：点名 riscv64 U-Boot 直载测试链；§7.4 补 test-riscv64-boot-full.sh | 文档 §5.2.3/§7.4 | 五门 |
| READER-R8-F21 | panel-r8 | P3 | 文档/ESP、ConOut、MIB 首现未展开 | ④ | 已修：三处补一句定义 | 文档 §6.1/§6.3/§2.3 | 五门 |
| READER-R8-F22 | panel-r8 | P3 | 画像目标④章节集合与正文不一致 | ④ | 已修：画像同步 §2.4/§2.4.2 | reader-profile.md | 五门 |
| READER-R8-F23 | panel-r8 | P3 | 文档/RESERVED 256 来由 127×2=254 差 2 | ④ | 已修：改“翻倍后向上取整到 2 的幂” | 文档 §4.5 | 五门 |
| READER-R8-F24 | panel-r8 | P3 | 文档/fast-fail “被削弱” vs “本来没有” | ④ | 已修：统一“相对稀疏方案被放弃；C 本来没有” | 文档 §2.6.1 | 五门 |
| READER-R8-F25 | panel-r8 | P3 | 文档/测试证据三处重复无单一权威 | ④ | 已修：M13 与 §5.2.1 改指针，§5.3 TODO-07 保留完整清单 | 文档 §5.1/§5.2.1/§5.3 | 五门 |
| DESIGN-R8-01 | panel-r8 | P0 | 源码/文档/a2 通道三种互斥说法且无生产者 | ①（待裁决） | 已修：统一为“备用 a2 通道，仓内无生产者；已跑通 DTB /chosen”（doc §3.7/§4.8、bootface.rs、main.rs asm、脚本注释）；是否补 a2 真机取证待用户 | 四文件注释 | 编译绿 |
| DESIGN-R8-02 | panel-r8 | P1 | 代码/pool 分配面仍可覆写内核（模块读入、param、DTB、ELF 源） | ①（部分转出） | 已修：loader 段拷贝加 src/dest 不重叠断言；M16 收窄为“仅 UEFI 模块页防线”；全局 allocator 守卫留 follow-up | os/boot-shim/src/loader.rs；M16 | 测试+五门 |
| DESIGN-R8-03 | panel-r8 | P2 | 测试/页取整案无判别力 | ① | 已修：换成模块字节越界案；M16 五档改“相离/相切、包含、部分重叠、内核在区间内、模块字节越界” | uefi_helpers.rs 测试 | 测试绿 |
| DESIGN-R8-04 | panel-r8 | P2 | 源码注释/vm_handoff 调用点仍写恒等窗重放 | ① | 已修：改“退休；生产者空表、VM 拒绝、MMIO 走设备窗” | os/kernel/src/vm_handoff.rs | 编译绿 |
| DESIGN-R8-05 | panel-r8 | P2 | 源码注释/handoff_v1 测试把 96B 挂到 SEMANTIC_END | ① | 已修：改“wire 镜像到 WIRE_STARTUP_END 后余 96B” | os/libs/minix-boot/src/handoff_v1.rs | 测试绿 |
| DESIGN-R8-06 | panel-r8 | P2 | 源码注释/BootFileEntry.path 零填充说法未被 builder 保证 | ① | 已修：注释改 .bss 零初始化 + builder 写出 NUL；builder 增路径长度断言 | os/boot-shim/src/uboot_main.rs / opensbi_helpers.rs | 编译绿 |
| DESIGN-R8-07 | panel-r8 | P3 | 文档/§4.2 把 arena 重查算进 alloc_arena 之前 | ① | 已修：括注拆“root/bump 与模块在 alloc_arena 前；arena 重查紧跟 alloc_arena” | 文档 §4.2 | 五门 |
| DESIGN-R8-08 | panel-r8 | P3 | 源码注释/BootForm (e) 仍称 park mailbox | ① | 已修：改“AP early-entry stub（次级核路径，非主启动形态）”；结构取舍随 R7-FU-01 | os/libs/minix-boot/src/boot_form.rs | 编译绿 |
| DESIGN-R8-09 | panel-r8 | P3 | 源码注释/magic 闸高估能抓的失败 | ① | 已修：改“只抓可读地址上的坏内容；野地址在首次读 magic 时 fault” | os/boot-shim/src/opensbi_helpers.rs | 编译绿 |
| FACT-R8-06 | panel-r8 | P2 | 源码注释/VmBootHandoff 版本历史缺 v8，v7 末句与返空生产者相反 | ① | 已修：补 v8 条目（deducted 32→47、vm_boot_exclusions、kern_ident 恒空），v7 末句改过去时 | os/libs/minix-types/src/types/boot.rs | 编译绿 |
| FACT-R8-07 | panel-r8 | P3 | 文档/§3.4 称内存账走 GETINFO 键 | ① | 已修：模块表/param/kmessages 各自键写清；内存账改走 VM boot handoff 页 | 文档 §3.4 | 五门 |
| FACT-R8-08 | panel-r8 | P3 | 文档/§4.4 把 reserved 来源写成“非 conventional” | ① | 已修：两处改 `ReservedMapped` 档（Excluded/DevicePath/Side 不进）；kernel_info 字段首句同步 | 文档 §4.4；kernel_info.rs | 五门+编译绿 |
| FACT-R8-09 | panel-r8 | P1 | 文档/§4.5 与 lib.rs 把 kmain 的同址再存误判为不存在（R8 修复引入；fact 自我纠错） | ① | 已修（R9 批）：文档改三个调用点、resume 与 kmain 均同址；lib.rs 注释恢复 kmain re-store | 文档 §4.5；os/kernel/src/lib.rs | 五门+测试 |

# Round 9（基线 6321ce600）

## R9 面板明细（33 条）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-R9-01 | panel-r9 | P1 | 文档/store_kernel_info 称只有两个调用点 | ① | 已修：改三个调用点（arch_boot_finish/resume_additive/kmain），resume 与 kmain 同址；lib.rs 注释恢复 kmain re-store | 文档 §4.5；lib.rs | 五门+测试 |
| FACT-R9-02 | panel-r9 | P2 | 文档/§2.8 改 R/M 后残留 N+1 | ① | 已修：末行改 M+1；R 定义补 AVAILABLE | 文档 §2.8 | 五门 |
| FACT-R9-03 | panel-r9 | P2 | 测试/boot-shim 并行下 preflight 全局态互污染 | ①（设计选择） | 已修：新增 `test_serial` 锁；`.cargo/config.toml` 固定 RUST_TEST_THREADS=1 并删“可并行覆盖”邀请；分配计数测试不承诺并行（R10-06 纠正） | os/boot-shim/src/{lib,dtb_copy,ebs_pair}.rs；os/.cargo/config.toml | 默认单线程 131 绿 |
| FACT-R9-04 | panel-r9 | P2 | 源码注释/kern_ident 只保留 v6/v7 与 v8 结构相反 | ① | 已修：vm_handoff 注释改"v8 仍占布局、恒空、v6/v7 精确拒绝"；boot.rs 字段注释同步 | vm_handoff.rs；types/boot.rs | 编译绿 |
| FACT-R9-05 | panel-r9 | P3 | 文档/§4.3 逐字段补零与 builder 新契约不符 | ① | 已修：改"路径 + 一个 NUL；其余 .bss 零初值；越界由 assert 拦下" | 文档 §4.3 | 五门 |
| READER-R9-F01 | panel-r9 | P2 | 文档/§1.2 .bss 装载证据指向 §2.8 | ④ | 已修：拆两指向（§2.8 推演、§2.4 装载证据） | 文档 §1.2 | 五门 |
| READER-R9-F02 | panel-r9 | P2 | 文档/kern_stack_top 门描述与构造边界矛盾 | ④ | 已修：门描述改"不越过上界，恰等于镜像尾合法" | 文档 §4.4 | 五门 |
| READER-R9-F03 | panel-r9 | P2 | 文档/落地区二义，接收侧深拷次数不清 | ④ | 已修：接收侧直接解码进七个 store，store 同址 no-op；DTB 落地区改暂存区 | 文档 §4.8/§4.5/§4.2 | 五门 |
| READER-R9-F04 | panel-r9 | P2 | 文档/U-Boot shim 腿写成调 arch_boot | ④ | 已修：改"序列化进 arena 交接收镜像 arch_boot_resume_v1" | 文档 §3.7 | 五门 |
| READER-R9-F05 | panel-r9 | P2 | 文档/§4.2 省略块同时承载循环内 preflight 与 EBS 后写入 | ④ | 已修：拆成"循环体内另一处省略"与"本处只含 EBS 后 startup" | 文档 §4.2 | 五门 |
| READER-R9-F06 | panel-r9 | P3 | 文档/§2.6.1 PSE/PGE 挂在 CR0.PG 后 | ④ | 已修：改 CR0.PG/CR0.WP/CR4.PSE/CR4.PGE | 文档 §2.6.1 | 五门 |
| READER-R9-F07 | panel-r9 | P3 | 文档/"两个可执行体"缺主线限定 | ④ | 已修：§1.2/§1.3 加主线限定；§1.4 补 resume 入口 | 文档 §1.2/§1.3/§1.4 | 五门 |
| READER-R9-F08 | panel-r9 | P3 | 文档/手算五例分组与定档返回不符 | ④ | 已修：改"前两例成功定档（例2 Step3 返回）；后三例报错/Side/报错" | 文档 §4.2 | 五门 |
| READER-R9-F09 | panel-r9 | P3 | 文档/§2.8 R 定义与 AVAILABLE 过滤不符 | ④ | 已修：R = 可用条目数；N+1→M+1 | 文档 §2.8 | 五门 |
| READER-R9-F10 | panel-r9 | P3 | 文档/"16 篇"不可达 | ④ | 已修：改指 `smp_gpt_v3.md`，§7.1 补一行 | 文档 §4.9/§7.1 | 五门 |
| READER-R9-F11 | panel-r9 | P3 | 文档/UEFI 术语承诺未兑现 | ④ | 已修：Configuration Table/LoadOptions/SimpleFileSystem 首现补定义 | 文档 §4.2/§4.3 | 五门 |
| READER-R9-F12 | panel-r9 | P3 | 文档/§2.4.2 口径句漏内核块 | ④ | 已修：切占用含内核镜像块 | 文档 §2.4.2 | 五门 |
| READER-R9-F13 | panel-r9 | P3 | 文档/syscall_entry 语义二义 | ④ | 已修：字段表注明引用性元数据、实际门地址由架构 asm 标签提供 | 文档 §4.4 | 五门 |
| READER-R9-F14 | panel-r9 | P3 | 文档/§6.4 同块"无脚本/有脚本"并存 | ④ | 已修：首处改"无专用启动门脚本，由 test-smp-aps-aarch64.sh 间接驱动" | 文档 §6.4 | 五门 |
| READER-R9-F15 | panel-r9 | P3 | 文档/§3.6 否决表方案与理由不对应 | ④ | 已修：方案扩为常量面三件 | 文档 §3.6 | 五门 |
| READER-R9-F16 | panel-r9 | P3 | 文档/§2.8 引用章内未出现的断言 | ④ | 已修：换成 §2.4 已引的 `assert(cbi->mem_high_phys)` | 文档 §2.8 | 五门 |
| READER-R9-F17 | panel-r9 | P3 | 文档/§2.6.4 把同一批 1024 项数两遍 | ④ | 已修：改"0..960 恒等 + 961 起被内核映射覆写" | 文档 §2.6.4 | 五门 |
| DESIGN-R9-01 | panel-r9 | P1 | 代码/loader 新断言只查同段，跨段源破坏仍可过 | ① | 已修：新增 `first_source_destination_overlap` 全源×全目标判定；文档 §4.3 补防线 | loader.rs；文档 §4.3 | 测试+五门 |
| DESIGN-R9-02 | panel-r9 | P0 | 文档/源码 kmain 同址再存被误判 | ① | 已修（与 FACT-R9-01 同批）：恢复三调用点与同址说明 | 文档 §4.5；lib.rs | 五门+测试 |
| DESIGN-R9-03 | panel-r9 | P0 | 源码注释/BootForm(e) "park until tail installed" 与永久 wfi 相反 | ① | 已修：改"无 tail 永久 wfi；tail 须在首个 hart_start 前注册" | boot_form.rs | 编译绿 |
| DESIGN-R9-04 | panel-r9 | P0 | 源码注释/v8 历史把内核侧 vm_boot_exclusions 写成 handoff 变更 | ① | 已修：v8 只留 deducted 32→47；注明 exclusions 不跨 handoff | types/boot.rs | 编译绿 |
| DESIGN-R9-05 | panel-r9 | P0 | 源码注释/"kept only for v6/v7" 与 v8 仍带字段相反 | ① | 已修：改"v8 仍占布局、恒空；v6/v7 精确拒绝" | vm_handoff.rs；types/boot.rs | 编译绿 |
| DESIGN-R9-06 | panel-r9 | P0 | 源码注释/KERNINFO_PAGE_PHYS 仍称 SetAddrSpace 消费者 | ① | 已修：改唯一读者 vm_handoff builder；VM 在 init_page_table 建每根映射 | kerninfo.rs | 编译绿 |
| DESIGN-R9-07 | panel-r9 | P1 | 源码注释/ld SHIM_HEAP 写 8 MiB，常量 16 MiB | ① | 已修：uboot_main 注释、ld .lbss 与镜像尺寸三处改 16 MiB | uboot_main.rs；riscv64-uboot.ld | 编译绿 |
| DESIGN-R9-08 | panel-r9 | P1 | 文档/§4.3 逐字段补零 | ① | 已修（与 FACT-R9-05 同批） | 文档 §4.3 | 五门 |
| DESIGN-R9-09 | panel-r9 | P1 | 源码注释/main.rs a2 主语仍绑 U-Boot | ① | 已修：改"本 riscv64 直核面另留 a2"；U-Boot 另入口 a0/a1 | kernel-image/src/main.rs | 编译绿 |
| DESIGN-R9-10 | panel-r9 | P1 | 代码/DRAM 边界闸只查起始，魔数读 8 字节可越界 | ① | 已修：两处直核面改 `in_memmap_range(..,8)`；删未用 in_memmap 导入 | bootface.rs；bootface_a64.rs | 编译绿 |
| DESIGN-R9-11 | panel-r9 | P2 | 源码注释/4 MiB 归因于扣除，UEFI 腿实际不在 free list | ① | 已修：注释拆因果（UEFI LOADER_DATA 不在 conventional；整段扣除只对自引导腿有效） | boot-shim/src/main.rs | 编译绿 |
| FACT-R9-06 | panel-r9 | P2 | 文档/"九步只覆盖 EBS 之前"与步骤 7–9 矛盾 | ① | 已修：改"编号覆盖到跳转为止（含 EBS 后 7–9）；startup 夹在 6 与 7 之间" | 文档 §4.2 | 五门 |
| FACT-R9-07 | panel-r9 | P2 | 文档/§3.5 引用已随 F03 删除的调用点注释 | ① | 已修：改"早期注释（f80ae358b^ 删除）；现仓仅 opensbi 测试模块一句结构说明" | 文档 §3.5 | 五门 |
| FACT-R9-08 | panel-r9 | P2 | 代码/loader 断言只查同段，跨段源破坏仍可过 | ① | 已修（与 DESIGN-R9-01 同批）：全源×全目标谓词 + 跨段宿主测试 | loader.rs | 测试绿 |
| FACT-R9-09 | panel-r9 | P3 | 源码注释/页取整理由与新测试注释互相矛盾 | ① | 已修：函数注释与调用点注释改"页对齐前提下取整不增加判别力，属防御性写法" | uefi_helpers.rs | 编译绿 |
| FACT-R9-10 | panel-r9 | P3 | 脚本头注仍把 a2 说成 QEMU 直载通道 | ① | 已修：改"直核面备用 a2，仓内无生产者；QEMU/OpenSBI 只约定 a0/a1" | test-kernel-image-riscv64-bootface.sh | 五门 |
| FACT-R9-11 | panel-r9 | P3 | 文档/§3.8 pt_alloc 引文漏段未标省略 | ① | 已修：register 与 alloc_pt_page 两处补显式省略标记，写明被省略的 write-once 与不变量说明 | 文档 §3.8 | 五门 |

# Round 10（基线 a327032d6）

## R10 面板明细（32 条）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-R10-01 | panel-r10 | P0 | 代码/riscv64 kernel-image 编译失败（bootface 对 u64 取 .0） | ① | 已修：删两处 `.0`；两个 riscv feature 构建均绿；CI 补 build 作业登记 follow-up | bootface.rs；cargo build riscv64 x2 | 构建绿 |
| FACT-R10-02 | panel-r10 | P3 | 文档/uboot_main 行号锚点 395→403 | ① | 已修：两处改 403，并用函数名锚点 | 文档 §3.1/§3.2 | 五门 |
| FACT-R10-03 | panel-r10 | P3 | 代码/in_memmap 成死函数，magic SAFETY 仍指旧函数 | ① | 已修：删 in_memmap；SAFETY 改 in_memmap_range(pa,8) | bootface_common.rs | 编译绿 |
| FACT-R10-04 | panel-r10 | P3 | 文档/mem_high_phys 引文位置标 §2.4，实际 §2.6.1 | ① | 已修：指针改 §2.6.1 | 文档 §2.8 | 五门 |
| FACT-R10-05 | panel-r10 | P3 | 代码/重叠谓词对零长段误报 | ① | 已修：filesz/memsz 为 0 跳过；补零长负控测试 | loader.rs | 测试绿 |
| READER-R10-F01 | panel-r10 | P2 | 文档/enter_kernel_image_v1 被写成 UEFI 专属 | ④ | 已修：改"跨镜像 v1 发送，三条腿" | 文档 §3.7 | 五门 |
| READER-R10-F02 | panel-r10 | P2 | 文档/接线权威表缺 aarch64 直核面 | ④ | 已修：表补一行（测试通道、SMP 判据） | 文档 §3.1 | 五门 |
| READER-R10-F03 | panel-r10 | P2 | 文档/§2.4.1 结算把 §2.5 的切除写成已完成 | ④ | 已修：改"将在 §2.5 切掉（本节尚未切）" | 文档 §2.4.1 | 五门 |
| READER-R10-F04 | panel-r10 | P2 | 文档/arena 由 UEFI 分配覆盖 U-Boot shim | ④ | 已修：拆两腿（UEFI allocate_pages；U-Boot shim bump_alloc_pages，含在整段扣除） | 文档 §3.7 | 五门 |
| READER-R10-F05 | panel-r10 | P2 | 文档/§4.1 导航表把 U-Boot shim 归进直核面 | ④ | 已修：列名改直核面，三处单元格注明 U-Boot shim 走 v1/接收 | 文档 §4.1 | 五门 |
| READER-R10-F06 | panel-r10 | P2 | 文档/§3.1 用辅助 prepare_boot 充当生产证据 | ④ | 已修：引文头标"辅助形态无生产调用方"；1311 行加"辅助形态下"限定 | 文档 §3.1 | 五门 |
| READER-R10-F07 | panel-r10 | P2 | 文档/"内核镜像恒在表内"与 LOADER_* 分流冲突 | ④ | 已修：改"UEFI 腿在保留表；DTB 腿在可用表" | 文档 §3.4 | 五门 |
| READER-R10-F08 | panel-r10 | P2 | 文档/"六入口"两套划分 | ④ | 已修：定义按最外层壳（3 arch_boot + 3 decode；aarch64 内层 high_half） | 文档 §5.3 | 五门 |
| READER-R10-F09 | panel-r10 | P3 | 文档/§6.1 v1 路标位置全写 uefi_helpers | ④ | 已修：位置列拆 uefi_helpers（两条 UEFI）/uboot_main（shim） | 文档 §6.1 | 五门 |
| READER-R10-F10 | panel-r10 | P3 | 文档/§2.8 "逐条如下"只列三条 | ④ | 已修：改"其余三条只给指向" | 文档 §2.8 | 五门 |
| READER-R10-F11 | panel-r10 | P3 | 文档/PML4 256 至 512 越界 | ④ | 已修：改 256..511（半开 256..512，256 项） | 文档 §4.7 | 五门 |
| READER-R10-F12 | panel-r10 | P3 | 文档/§2.4.1 调用序箭头与嵌套关系相反 | ④ | 已修：改嵌套表述（arch_init in cstart in kmain；smp_init 晚于 cstart） | 文档 §2.4.1 | 五门 |
| READER-R10-F13 | panel-r10 | P3 | 文档/"11 个模块"章内无出处 | ④ | 已修：改"12 个里除 VM 外 11 个（名单见 §4.3）" | 文档 §2.4.2 | 五门 |
| READER-R10-F14 | panel-r10 | P3 | 文档/§7.3 源码清单漏 ebs_map/ebs_pair 等 | ④ | 已修：补 ebs_map/ebs_pair/boot_params/memmap_source/fdt_layout | 文档 §7.3 | 五门 |
| READER-R10-F15 | panel-r10 | P3 | 文档/分配上界"同一常量" vs 两界取小 | ④ | 已修：改"以 BOOT_IDENTITY_MAP_END 为一界，再与 DM 窗口取小" | 文档 §4.6 | 五门 |
| READER-R10-F16 | panel-r10 | P3 | 文档/"六个真机脚本"无枚举 | ④ | 已修：改"五条门链加 aarch64 直核面这六个组合" | 文档 §3.7 | 五门 |
| READER-R10-F17 | panel-r10 | P3 | 文档/Step1 可达失败只列 AllocationFailed 论据不足 | ④ | 已修：补其余四类不可达判据（地址常量/只映射/尺寸快照/自造标志） | 文档 §4.6 | 五门 |
| DESIGN-R10-01 | panel-r10 | P2 | 测试/串行锁与进程级分配计数测试不匹配；覆盖档仍红 | ①（设计选择） | 已修：`.cargo/config.toml` 固定 RUST_TEST_THREADS=1 并删"可覆盖"邀请；文档命令即默认串行；并行覆盖不承诺 | os/.cargo/config.toml | 默认 131 绿 |
| DESIGN-R10-02 | panel-r10 | P2 | 源码+文档/ptr::copy 理由指向被守卫跳过的同址分支 | ① | 已修：改"为部分重叠保留；精确同址由守卫跳过"；expect one-shot 文案改 KERNEL_INFO store failed | lib.rs；文档 §4.5 | 五门+测试 |
| DESIGN-R10-03 | panel-r10 | P2 | 源码注释/(extra_argc,extra_argv) 记到内核镜像入口 | ① | 已修：改"U-Boot shim 的入口是另一个二进制"；本镜像接收 a0=arena PA 不变 | kernel-image/src/main.rs | 编译绿 |
| DESIGN-R10-04 | panel-r10 | P2 | 代码/in_memmap 死码与 SAFETY 旧函数 | ① | 已修（与 FACT-R10-03 同批） | bootface_common.rs | 编译绿 |
| DESIGN-R10-05 | panel-r10 | P3 | 代码/零长段误报且只有正控 | ① | 已修（与 FACT-R10-05 同批）：跳过零长 + 负控 | loader.rs | 测试绿 |
| DESIGN-R10-06 | panel-r10 | P3 | 源码注释/load_segments rustdoc 未记新 panic | ① | 已修：rustdoc 补源/目标重叠 panic 面 | loader.rs | 编译绿 |
| DESIGN-R10-07 | panel-r10 | P3 | 代码/resv_claims 同形 static mut 无串行 | ① | 已修：13 个测试统一加 test_serial 锁 | resv_claims.rs | 测试绿 |
| DESIGN-R10-08 | panel-r10 | P3 | 源码注释/MAX_IDENT 文档仍讲 v6 现在时 | ① | 已修：改"退休字段容量，v8 恒空" | types/boot.rs | 编译绿 |
| DESIGN-R10-09 | panel-r10 | P3 | 源码注释/bootface bootstrap=(0,0)、vm_handoff body retained 陈旧 | ① | 已修：两处改现状（字段已删；空体无历史保留句） | bootface.rs；vm_handoff.rs | 编译绿 |
| DESIGN-R10-10 | panel-r10 | P3 | 文档/模块装载后检查三处两种强度 | ① | 已修：统一"防御性复审；可达失败在 alloc_module_pages 拷贝前" | 文档 §4.2/§5.1 M16 | 五门 |
| FACT-R10-06 | panel-r10 | P2 | 测试/R9 声称 8 线程全绿不可复现；锁只覆盖 preflight 全局态 | ①（设计选择） | 已修：撤销“并行安全”声明；config 固定单线程并禁止覆盖；test_serial 注释限定为“显式覆盖时保护 preflight 全局态” | lib.rs；.cargo/config.toml；loop/台账 | 默认 131 绿 |
| FACT-R10-07 | panel-r10 | P3 | 文档/§3.6 否决表理由仍写被迫实现 map_huge | ① | 已修：理由收窄为“所有 Paging 实现声明支持大页；map_huge 不在搬移范围” | 文档 §3.6 | 五门 |
| FACT-R10-08 | panel-r10 | P3 | 源码注释/.bss 清零称碰 rdi/rax/rcx，实际 rdi/rcx | ① | 已修：注释改 rdi/rcx | kernel-image/src/main.rs | 编译绿 |
| FACT-R10-09 | panel-r10 | P3 | 文档/smp_gpt_v3.md 放在 §7.4，记录称 §7.1 | ① | 已修：移到 §7.1 前置与后续篇 | 文档 §7.1/§7.4 | 五门 |
| FACT-R10-10 | panel-r10 | P3 | 源码注释/riscv 交接注释悬空右括号 | ① | 已修：删多余 `）`，两句改独立句 | kernel-image/src/main.rs | 编译绿 |
| FACT-R10-11 | panel-r10 | P3 | 文档/startup_objects 称有运行期 provenance 读者 | ① | 已修：改“只供启动期 validate_requiring_sources；全仓无运行期读者” | 文档 §3.4 | 五门 |
| FACT-R10-12 | panel-r10 | P1 | 文档/§3.4 memmap 行把内核划进 UEFI 保留表 | ① | 已修：改回源码契约“内核在每条腿都恒在可用表；模块 UEFI 不在” | 文档 §3.4；kernel_info.rs:24-28 | 五门 |
| FACT-R10-13 | panel-r10 | P2 | 文档/U-Boot shim arena 称已在整段 bump 扣除内 | ① | 已修：改“独立 bump 分配，在 boot_alloc_region 之后，不在扣除内；解码后废弃” | 文档 §3.7；opensbi/uboot 顺序 | 五门 |
| FACT-R10-14 | panel-r10 | P3 | 文档/§4.1 单元格仍把 U-Boot shim 混进直核列 | ① | 已修：内存与参数准备单元格拆三腿；进入内核单元格已拆 | 文档 §4.1 | 五门 |

# Round 11（基线 2b3f10c10）

## R11 面板明细（38 条）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-R11-01 | panel-r11 | P0 | 文档/§4.6 引文仍印已删的 one-shot expect | ① | 已修：引文改现行消息；补省略标记 | 文档 §4.6 | 五门 |
| FACT-R11-02 | panel-r11 | P0 | 文档/"六个组合全部 DTB 注入"与实际相反 | ① | 已修：改为仓库内六个 fdtput 注入脚本逐一列名 | 文档 §3.7 | 五门 |
| FACT-R11-03 | panel-r11 | P2 | 文档/权威表 aarch64 行丢块引用前缀 | ① | 已修：补 `> ` | 文档 §3.1 | 五门 |
| FACT-R11-04 | panel-r11 | P1 | 测试/零长负控零判别力 | ① | 已修：负控改成"零长地址严格落在区间内"的几何，删 continue 才红 | loader.rs 测试 | 测试绿 |
| FACT-R11-05 | panel-r11 | P3 | 源码注释/riscv 交接注释孤立右括号 | ① | 已修（与 R10-10 同批） | kernel-image/src/main.rs | 编译绿 |
| FACT-R11-06 | panel-r11 | P3 | 源码注释/bootface SAFETY 仍称 in_memmap | ① | 已修：改 in_memmap_range(size_of::<BootFileTable>()) | bootface.rs | 编译绿 |
| FACT-R11-07 | panel-r11 | P3 | 源码注释/test_serial 只提 dtb_copy | ① | 已修：注释补 resv_claims 静态量范围 | lib.rs | 编译绿 |
| FACT-R11-08 | panel-r11 | P3 | 文档/PermissionDenied 被写成真实失败类别 | ① | 已修：改"当前 os/arch 无生产者" | 文档 §4.6 | 五门 |
| FACT-R11-09 | panel-r11 | P2 | 源码注释/rt-birth :426 引用不存在；载体携已删字段 | ①（部分转出） | 已修：删 :426 引用；8 个 QEMU 载体携 free_upper_idx 的编译问题登记 follow-up | main.rs；R11-FU | 编译绿 |
| FACT-R11-10 | panel-r11 | P2 | 文档/resume_additive 引文起点 317 行不符 | ① | 已修：改约 318–322 行 | 文档 §4.7 | 五门 |
| READER-R11-F01 | panel-r11 | P2 | 文档/§2.8 "逐条如下"只列三条 | ④ | 已修：改"下面只展开三条，另两条回指 §1.2/§2.6" | 文档 §2.8 | 五门 |
| READER-R11-F02 | panel-r11 | P2 | 文档/M15 时点写"EBS 前" | ④ | 已修：改"配对循环成功后、裸串口可打印" | 文档 §5.1 M15 | 五门 |
| READER-R11-F03 | panel-r11 | P2 | 文档/TODO-07 "三案" vs 5 条 | ④ | 已修：M13 与 §5.3 统一"五案/5 条" | 文档 §5.1/§5.3 | 五门 |
| READER-R11-F04 | panel-r11 | P2 | 文档/§2.4.1 pg_alloc_page 主语漂移 | ④ | 已修：记账句归回 pg_alloc_page；alloc_lowest 另述 | 文档 §2.4.1 | 五门 |
| READER-R11-F05 | panel-r11 | P2 | 文档/四前提被说成安全边界 | ④ | 已修：改"四前提建立的是切除不发生（危险来源）；兜住的是取页方向与生命周期" | 文档 §2.4.1 | 五门 |
| READER-R11-F06 | panel-r11 | P3 | 文档/"两条 UEFI 产线改用协议" | ④ | 已修：改"跨镜像三腿" | 文档 §3.4 | 五门 |
| READER-R11-F07 | panel-r11 | P3 | 文档/Step 2c 归属矛盾 | ④ | 已修：改"三架构共用，aarch64 空表 no-op" | 文档 §4.6 | 五门 |
| READER-R11-F08 | panel-r11 | P3 | 文档/"映射责任移交 VM"重复 | ④ | 已修：删重复状语 | 文档 §4.1 | 五门 |
| READER-R11-F09 | panel-r11 | P3 | 文档/BOARD 分歧复述 | ④ | 已修：删段尾重复句 | 文档 §3.4 | 五门 |
| READER-R11-F10 | panel-r11 | P3 | 文档/§1.1"不会再发出去"绝对化 | ④ | 已修：加"归还之前"时间窗与 §2.4.2 回指 | 文档 §1.1 | 五门 |
| READER-R11-F11 | panel-r11 | P3 | 文档/L5 跨距与 kern_size 同义却分列 | ④ | 已修：注明 L5 与规划层检查同一公式，仅执行位置不同 | 文档 §5.2.1 | 五门 |
| READER-R11-F12 | panel-r11 | P3 | 文档/§2.6.4 "本节前半"误指 | ④ | 已修：改"上一小节（§2.6.3）" | 文档 §2.6.4 | 五门 |
| READER-R11-F13 | panel-r11 | P3 | 文档/§4.2 "这两页"单位错 | ④ | 已修：改"两个分配（根页 + 1024 页 bump 区）" | 文档 §4.2 | 五门 |
| READER-R11-F14 | panel-r11 | P3 | 文档/§3 导览 3.8–3.9 归类错 | ④ | 已修：改"3.8 分配器换手，3.9 槽位账退役" | 文档 §3 导览 | 五门 |
| READER-R11-F15 | panel-r11 | P3 | 文档/§2.4.1 自指 | ④ | 已修：两处改"上文的升序讨论" | 文档 §2.4.1 | 五门 |
| READER-R11-F16 | panel-r11 | P3 | 文档/§7.2 C 源清单漏项 | ④ | 已修：补 main/protect/arch_system/vm-alloc/do_*/assert/loadfile/exec/sysenv 等 | 文档 §7.2 | 五门 |
| READER-R11-F17 | panel-r11 | P3 | 文档/证据类型列 11 种写法超四类 | ④ | 已修：引言说明 `＋`/`→` 组合语义与三类归并 | 文档 §5 引言 | 五门 |
| READER-R11-F18 | panel-r11 | P3 | 文档/§2.4.1 "本节结算"归属错 | ④ | 已修：改"2.4 结算"；"本节尚未切"→"§2.4 尚未切" | 文档 §2.4.1 | 五门 |
| DESIGN-R11-01 | panel-r11 | P2 | 测试/单线程只是默认值，两条旁路仍可并线 | ① | 已修：分配计数改 thread_local（每线程计数）；8 线程实测 131 全绿；锁仍保护 preflight | lib.rs；8 线程 | 测试 8 线程 |
| DESIGN-R11-02 | panel-r11 | P2 | 测试/零长负控零判别力 | ① | 已修（与 FACT-R11-04 同批） | loader.rs 测试 | 测试绿 |
| DESIGN-R11-03 | panel-r11 | P1 | 文档/§3.4 memmap 行回归 | ① | 已修（与 FACT-R10-12 同批） | 文档 §3.4 | 五门 |
| DESIGN-R11-04 | panel-r11 | P2 | 文档/台账两行称已修但树未改 | ① | 已修：uboot 锚点 395→403、mem_high_phys 指针→§2.6.1 两处补改 | 文档 §3.1/§3.2/§2.8 | 五门 |
| DESIGN-R11-05 | panel-r11 | P2 | 代码/BootFileTable 门只保证 8 字节，按 1296 字节解引用 | ① | 已修：两个自举面改 `in_memmap_range(pa, size_of::<BootFileTable>())` | bootface.rs/a64 | 编译绿 |
| DESIGN-R11-06 | panel-r11 | P3 | 源码注释/test_serial 范围窄 | ① | 已修（与 FACT-R11-07 同批） | lib.rs | 编译绿 |
| DESIGN-R11-07 | panel-r11 | P3 | 源码注释/in_memmap 旧符号 | ① | 已修（与 FACT-R11-06 同批） | bootface.rs | 编译绿 |
| DESIGN-R11-08 | panel-r11 | P3 | 源码注释/孤立右括号 | ① | 已修（与 FACT-R11-05 同批） | main.rs | 编译绿 |
| DESIGN-R11-09 | panel-r11 | P3 | 源码注释/build_identity_windows 测试前提过期 | ① | 已修：测试注释改"函数无参且不读 arch MMIO 表" | vm_handoff.rs 测试 | 测试绿 |
| DESIGN-R11-10 | panel-r11 | P3 | 代码/模块源与 bump 池相接判成交 | ① | 已修：相交判据改严格 `>`（相接不算重叠） | 待核代码/文档 | 编译绿 |

# Round 12（12 轮上限；基线 a29ddfdb9）

## R12 面板明细（32 条）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-R12-01 | panel-r12 | P2 | 文档/§2.8 "两条/三条"计数矛盾 | ① | 已修：三条展开 + 两条指向，与 bullet 一致 | 文档 §2.8 | 五门 |
| FACT-R12-02 | panel-r12 | P3 | 源码注释/config 仍称 process-wide 计数 | ① | 已修：改 per-thread；config 理由改 per-thread | lib.rs；.cargo/config.toml | 编译绿 |
| FACT-R12-03 | panel-r12 | P3 | 测试/零长夹具 filesz>memsz 非法 | ① | 已修：改用合法段（live_src + 零长 dst） | loader.rs 测试 | 测试绿 |
| FACT-R12-04 | panel-r12 | P3 | 源码注释/magic SAFETY 仍写 8 字节门 | ① | 已修：改全宽 BootFileTable 门 | bootface_common.rs | 编译绿 |
| FACT-R12-05 | panel-r12 | P3 | 文档/§4.2 省略括注顺序错 | ① | 已修：record_span/root/bump 在模块行之前 | 文档 §4.2 | 五门 |
| FACT-R12-06 | panel-r12 | P3 | 源码注释/bootface 孤立 doc 注释 | ① | 已修：删悬空 "Upper bound on memmap entries" 注释 | bootface.rs | 编译绿 |
| FACT-R12-07 | panel-r12 | P3 | 源码注释/RISC-V SUM 取指说明与规范相反 | ① | 已修：改 "irrespective of SUM；SUM 只放宽 load/store" | lib.rs | 编译绿 |
| READER-R12-F01 | panel-r12 | P2 | 文档/§2.8 三条/两条矛盾 | ④ | 已修（与 FACT-R12-01 同批） | 文档 §2.8 | 五门 |
| READER-R12-F02 | panel-r12 | P2 | 文档/"产线"未进术语对账 | ④ | 已修：对账补"产线=链的别称，不参与计数" | 文档 §3.7 | 五门 |
| READER-R12-F03 | panel-r12 | P2 | 文档/前提一行号无标签 | ④ | 已修：标明 474/311/301 各自对象 | 文档 §2.4.1 | 五门 |
| READER-R12-F04 | panel-r12 | P2 | 文档/§5.2.3 两条通道超出证据 | ④ | 已修：改"DTB 通道可用；a2 无生产者未验证" | 文档 §5.2.3 | 五门 |
| READER-R12-F05 | panel-r12 | P3 | 文档/§2.6.1 下游指针错 | ④ | 已修：改 §2.6.3 | 文档 §2.6.1 | 五门 |
| READER-R12-F06 | panel-r12 | P3 | 文档/§3.4 边界说明指针错 | ④ | 已修：改"§3.4 末的 repr(C) 边界说明" | 文档 §3.4 | 五门 |
| READER-R12-F07 | panel-r12 | P3 | 文档/"差一个连字符"不实 | ④ | 已修：改"名字相近但不是同一个东西" | 文档 §3.3 | 五门 |
| READER-R12-F08 | panel-r12 | P3 | 文档/kern_size/4MB 未取整 | ④ | 已修：两处改 ceil | 文档 §2.6 | 五门 |
| READER-R12-F09 | panel-r12 | P3 | 文档/§4.9 与 §4.3 overlaps 口径不一 | ①（残留） | 部分修：§4.9 已注明跨段防线；完整不变量合并列入 STUCK-REPORT | 文档 §4.9 | 五门 |
| READER-R12-F10 | panel-r12 | P3 | 文档/M4 与 TODO-01 入口计数 5 vs 6 | ④ | 已修：M4 改六入口（3 arch_boot + 3 decode） | 文档 §5.1 | 五门 |
| READER-R12-F11 | panel-r12 | P3 | 文档/四术语定义位置与首用不符 | ①（残留） | 部分修：arena/信封已就地定义；EBS/wire 定义位置列入 STUCK-REPORT | 文档 §3.4 | 五门 |
| READER-R12-F12 | panel-r12 | P3 | 文档/2.4 结算落点错 | ④ | 已修：整段移到 §2.4.1 之前（§2.4 收束） | 文档 §2.4 | 五门 |
| READER-R12-F13 | panel-r12 | P3 | 文档/三处长段阻断冷读 | ①（残留） | 未拆：长段 642/1052/1858，列入 STUCK-REPORT | 文档全文 | 已登记 STUCK-REPORT 残留清单 |
| DESIGN-R12-01 | panel-r12 | P0 | 文档/U-Boot 入口锚点 fn main 不存在 | ① | 已修：两处改 `:fn uboot_main`；锚点 104/104 | 文档 §3.1/§3.2 | 锚点门 |
| DESIGN-R12-02 | panel-r12 | P2 | 源码注释/共享 v1 入口把 arena 一律写 LOADER_DATA | ① | 已修：三处按腿陈述（UEFI LOADER_DATA / U-Boot bump） | lib.rs | 编译绿 |
| DESIGN-R12-03 | panel-r12 | P2 | 源码注释/store_kernel_info 锚 lib.rs:734 | ① | 已修：两处改符号锚 | bootface.rs | 编译绿 |
| DESIGN-R12-04 | panel-r12 | P2 | 源码注释/计数改 per-thread 后三处未扫尾 | ① | 已修：test_allocations/test_serial/config 三处 | lib.rs；config | 编译绿 |
| DESIGN-R12-05 | panel-r12 | P2 | 测试/per-thread 计数缺正控 | ① | 已修：新增 allocation_counter_sees_this_thread（132 条） | lib.rs | 测试绿 |
| DESIGN-R12-06 | panel-r12 | P2 | 文档/四前提与两因素归属互斥 | ① | 已修：642 改"常态第三行压在四前提上" | 文档 §2.4.1 | 五门 |
| DESIGN-R12-07 | panel-r12 | P3 | 文档/§3.4 段落以分号结尾 | ① | 已修：改句号 | 文档 §3.4 | 五门 |
| DESIGN-R12-08 | panel-r12 | P3 | 文档/§4.2 省略标记虚构注释 | ① | 已修：改"省略后续注释段，见源文件" | 文档 §4.2 | 五门 |
| DESIGN-R12-09 | panel-r12 | P3 | 测试/零长第二段用非法段 | ① | 已修（与 FACT-R12-03 同批） | loader.rs | 测试绿 |
| DESIGN-R12-10 | panel-r12 | P3 | 源码注释/aarch64 place_modules 称有 alignment gate | ① | 已修：改"straight copy + full-width memmap gate" | bootface_a64.rs | 编译绿 |
| DESIGN-R12-11 | panel-r12 | P3 | 源码注释/行号引用漂移 | ①（残留） | 部分修：store 两处已符号化；main/loader 三处列入 STUCK-REPORT | 源码 | 编译绿 |
| DESIGN-R12-12 | panel-r12 | P3 | 文档/root/bump "prepare_boot 时还没分配" | ① | 已修：改"prepare_boot 内部分配，序列化时记录" | 文档 §3.4 | 五门 |

# Round 13（基线 62d443ce3，冻结基线 sha256 9a782e0c）

> 面板配置变更（用户裁决 A+C 后的第一轮）：面板期间**冻结**被审文档与代码（`panel/round13/BASELINE.txt`），三角色审查同一组输入。本轮 fact 面板在 3 小时内未落盘（两次催写无效），记为"未交付"，由随后的 RB13 回读轮的 fact 角色补齐事实面。

## R13 面板明细（已交付 9 条：d2 4 + reader 5）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| DESIGN-R13-01 | panel-r13 | P2 | 文档/§4.2 省略段把 root/bump 重查说成在 12 模块行之后 | ① | 已修：改"record_kernel_span 与 root/bump 重查都在 12 模块行之前；assert_modules 在其后" | 文档 §4.2；uefi_helpers.rs 1063/1066/1073/1080/1085 | 五门 |
| DESIGN-R13-02 | panel-r13 | P2 | 文档/§4.6 转述源码 SUM 注释已过期（源码 R12 已改对） | ① | 已修：删"源码写成除非 SUM=1"，改"源码 NOTE 已同口径" | 文档 §4.6；lib.rs 935-938 | 五门 |
| DESIGN-R13-03 | panel-r13 | P3 | 文档/§3.4 括注把 arena 分配点归到 prepare_boot | ① | 已修：root/bump 在 prepare_boot 内、arena 在 uboot_main 序列化点 | 文档 §3.4；uboot_main 403/414 | 五门 |
| DESIGN-R13-04 | panel-r13 | P3 | 源码注释/bootface 把 store_kernel_info 时点写成 satp 之后 | ① | 已修：改"satp 写入之前、建根之前" | bootface.rs 501-508 | 编译绿 |
| READER-R13-F01 | panel-r13 | P2 | 文档/三处"长段拆分"只插软换行，渲染仍是同段 | ④ | 已修：在真拆点插空行；pandoc `<p>` 计数 5/4 复验 | 文档 644/1057/1867 | 渲染复验 |
| READER-R13-F02 | panel-r13 | P2 | 文档/Step1 两条目被吞并成惰性续行，计数与两处指针失准 | ④ | 已修：改"六条交代（前两条判据）"，三条非列表段落升为条目，指针改"第五条" | 文档 §4.6；`<li>` 计数 6 | 渲染复验 |
| READER-R13-F03 | panel-r13 | P3 | 文档/拆段留下三处双空行 | ④ | 已修：全篇空行归一，末尾多余空行去掉 | 全篇 | 结构复验 |
| READER-R13-F04 | panel-r13 | P3 | 文档/"见上一段"指向错段 | ④ | 已修：改"见上文'靠的是两点'那一段" | 文档 §2.4.1 | 五门 |
| READER-R13-F05 | panel-r13 | P3 | 文档/三处手写行号作判据锚 | ④ | 已修：三处改符号锚（cstart/arch_init、smp_init、add_memmap 归还点、`///` 注记） | 文档 §2.4.1/§2.4.2/§4.7 | 五门 |

## R13 前置实施：内核 LMA 预订 A+D（用户裁决 2026-11-08）

| 项 | 内容 | 证据 | 复验 |
|---|---|---|---|
| A：`AllocateAddress` 预订 | 新增 `reserve_kernel_span`（`uefi_helpers.rs`）：内核 ELF 读入后、任何分配前，把整段 `[kern_phys_base, +kern_size)` 以 `AllocateType::Address` + `LOADER_DATA` 预订；非页对齐或固件拒绝即 halt（不降级） | x86：`kernel span reserved [0x200000, 0x600000)`；aarch64：`[0x40200000, 0x40600000)` | 两条 UEFI 门链 PASS |
| D：装载顺序 | 两条 UEFI 入口改为"读 ELF → 预订跨度 → 分配 root/bump → 拷贝段 → 载模块"；`record_kernel_span` 紧随拷贝 | `test-shim-bootmarks-x86.sh` 5/5 标记；`test-shim-bootmarks-aarch64.sh` 3/3 | 门链 PASS |
| 模型同步 | `kernel_info.rs` 的 memmap 字段文档、`uefi_helpers.rs` 的快照注释、文档 §3.4 memmap 行与 §4.2 步骤 2 全部改为"UEFI 腿内核在 `ReservedMapped`、不在可用表；内核侧扣除退化为空操作" | 文档五门 | 五门绿 |
| 守卫测试 | `test_reserve_kernel_span_rejects_unaligned_span`（should_panic）→ boot-shim 133 | `cargo test -p boot-shim --features test-all --lib` 133 | 测试绿 |
| 残余防线 | `alloc_module_pages` 拷贝前拒绝、模块/root/bump/arena 复查保留为第二道 | 代码保留 | 测试绿 |

> 闭案判据（合同第七/第 2 项）：四道验证 = ①两条 UEFI 门链 PASS（已得）②单元测试 133（已得）③kernel 857 / minix-boot 176（已得）④文档五门（已得）。A+D 至此可写"已实施、已验证"。

### R13 fact（迟交，6 条；其中 4 条在批一已修）

| 发现 ID | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|
| FACT-R13-01 | P0 | 文档/§4.6 仍说源码注释写"除非 SUM=1" | ①（批一已修） | 已修：改"源码 NOTE 已同口径" | 文档 §4.6 | 五门 |
| FACT-R13-02 | P1 | 源码注释/`BOOT_MODULE_PROC_NRS` 锚到 lib.rs，实定义在 proc.rs | ① | 已修：改 `os/kernel/src/proc.rs` | loader.rs:50 | 编译绿 |
| FACT-R13-03 | P1 | 文档/§4.2 省略段顺序说反 | ①（批一已修） | 已修：root/bump 在 12 模块行之前 | 文档 §4.2 | 五门 |
| FACT-R13-04 | P2 | 文档/§3.4 arena 分配点归到 prepare_boot | ①（批一已修） | 已修：arena 在 uboot_main 序列化点 | 文档 §3.4 | 五门 |
| FACT-R13-05 | P2 | 文档/§5.2.1 for 列表漏 `lib.rs`，照抄只出 131 | ① | 已修：for 列表补 `os/boot-shim/src/lib.rs` | 文档 §5.2.1 | 五门 |
| FACT-R13-06 | P2 | 文档/M4 六个委托壳与钉文件不符 | ①（批一已修） | 已修：改"3 arch_boot + aarch64 high_half + 2 decode" | 文档 §5.1 M4 | 五门 |

### R13 reader 第二批（F06–F21，16 条）

| 发现 ID | 严重度 | 类别 | 处置类 | 处置 | 复验 |
|---|---|---|---|---|---|
| READER-R13-F06 | P2 | 文档/§4.3 防线①在 §5 账本无条目 | ① | 已修：M11 需证项补两道防线；§5.2.1 增 `first_source_destination_overlap` 两测试行 | 五门 |
| READER-R13-F07 | P3 | 文档/目录行"三条只给指向"与正文互补 | ① | 已修：目录改"两条只给指向" | 五门 |
| READER-R13-F08 | P3 | 文档/"建根"一词二义 | ① | 已修：§4.1 改"分配根页表页"与"构造并激活自举根" | 五门 |
| READER-R13-F09 | P3 | 文档/步骤 7、8 与代码顺序相反（decode 先于断言） | ① | 已修：步骤 8 改"解码描述符并构造"；步骤 7 注明解码先于断言 | 五门 |
| READER-R13-F10 | P3 | 文档/§6.1 接收链写成 aarch64 只经 high_half | ① | 已修：改"三架构都先经 decode，aarch64 再转 high_half" | 五门 |
| READER-R13-F11 | P2 | 文档/§5.2.1 for 列表缺 lib.rs | ①（事实 05 同批） | 已修：for 列表补 `os/boot-shim/src/lib.rs` | 五门 |
| READER-R13-F12 | P3 | 文档/"实跑约 121"与六项之和 120 不符 | ① | 已修：改"去掉 opensbi 11 条与 lib.rs 正控 1 条后为 121" | 五门 |
| READER-R13-F13 | P3 | 文档/M12"两条 raw 路标"与三条 [raw] 行不符 | ① | 已修：改"三条 [raw] 路标"并列名 | 五门 |
| READER-R13-F14 | P3 | 文档/"两条口径"实列三件 | ① | 已修：改"三条" | 五门 |
| READER-R13-F15 | P3 | 文档/§4.4"不是两道"无先行词、标题"一个校验" | ① | 已修：删否定句；标题改"一条校验门链" | 五门 |
| READER-R13-F16 | P3 | 文档/§4.2 标题箭头指 §3.6 | ① | 已修：改 →3.1、3.4 | 五门 |
| READER-R13-F17 | P3 | 文档/§3.9"2.6 末尾那条链"指错小节 | ① | 已修：改 §2.6.3 | 五门 |
| READER-R13-F18 | P3 | 文档/§6.1"两条输出通道"两层含义 | ① | 已修：标题改"EBS 分界与同通道两种写法"，正文保留两层区分 | 五门 |
| READER-R13-F19 | P3 | 文档/§3.6 理由句与上一条否定自相矛盾 | ① | 已修：理由句删 `map_huge` 归属，只讲常量面 | 五门 |
| READER-R13-F20 | P3 | 文档/§2.5 用 N+1、§2.8 用 M+1 | ① | 已修：§2.5 标题改 M+1 并注"M = 模块数" | 五门 |
| READER-R13-F21 | P3 | 文档/`k_initial_stktop` 三处说法不一 | ① | 已修：§1.1/§2.2/§7.5 统一为"§2.2 与 §2.8 引用结论、判据在 02 篇 §2.4" | 五门 |

### R13 reader 第三批（F22–F30，9 条）

| 发现 ID | 严重度 | 类别 | 处置类 | 处置 | 复验 |
|---|---|---|---|---|---|
| READER-R13-F22 | P2 | 文档/§6.1 声称 v0–v4 五点，表内只有 v0 | ① | 已修：该行扩为五路标并注明 v1–v3 不逐行、v4 文案；与三档读法对齐 | 五门 |
| READER-R13-F23 | P3 | 文档/§2.6.4 "本节收尾"实为 §2.6 总结算 | ① | 已修：括注改"本小节末段"；结算标题改"§2.6 结算（2.6.1–2.6.4 收束）" | 五门 |
| READER-R13-F24 | P3 | 文档/§7.2 `arch_smp.c` 列两次 | ① | 已修：合并进 i386 组，另加 arch_system.c/protect.c | 五门 |
| READER-R13-F25 | P3 | 文档/§4.4 标题与表口径不一 | ① | 已修：标题"一条校验门链"（批二）+ 表头注"十行分属四道门" | 五门 |
| READER-R13-F26 | P3 | 文档/"取件车道/取件库/取件表"三词无界定 | ① | 已修：首次出现处补三词分工定义 | 五门 |
| READER-R13-F27 | P3 | 文档/§6.3 v1 失败行列五个分区，实际七个 | ① | 已修：改七分区并列名 | 五门 |
| READER-R13-F28 | P3 | 文档/裸数字章节引用（无 §） | ① | 已修：六处裸引用统一加 § 前缀 | 五门 |
| READER-R13-F29 | P3 | 文档/§2.4.1 代词开头、先行词竞争 | ① | 已修：改"`alloc_lowest` 的唯一调用点" | 五门 |
| READER-R13-F30 | P3 | 文档/图例为正文不存在的 `O-11` 提供解释 | ① | 已修：删该子句 | 五门 |

### R13 reader 第四批（F31–F48，18 条；面板最终交付 48 条）

| 发现 ID | 严重度 | 类别 | 处置类 | 处置 | 复验 |
|---|---|---|---|---|---|
| READER-R13-F31 | P3 | 文档/§6.3 导语与划线历史行 | ① | 已修：导语改"其余各行是 Rust 链会遇到的或已不可复发的历史条目" | 五门 |
| READER-R13-F32 | P3 | 文档/§6.2 "四处约束"实为三条 | ① | 已修：改"三条约束（前两条合并陈述）" | 五门 |
| READER-R13-F33 | P3 | 文档/Step 2c 叫"第三步"与 Step 3 冲突 | ① | 已修：改"附属步骤（2c 不占主步序号）" | 五门 |
| READER-R13-F34 | P3 | 文档/§2.1 省略说明与块内实际不符 | ① | 已修：按三处省略逐条写清（声明行之后 / cmdline 解析块内 / 函数后段） | 五门 |
| READER-R13-F35 | P3 | 文档/§2.3 `mb_set_param` 出处与 SERVARNAME 条件不符 | ① | 已修：出处改指 `mb_set_param` 本体，条件句按源码重写 | 五门 |
| READER-R13-F36 | P3 | 文档/"四份依赖表"覆盖五个 manifest | ① | 已修：注明 minix-elf 是普通依赖、不单列 | 五门 |
| READER-R13-F37 | P2 | 文档/§3.7 承诺"见 §4.8"，该节无试序内容 | ① | 已修：就地写清试序（先 a2 后 DTB，先 memmap 界后魔数，失败换道、全失败 halt） | 五门 |
| READER-R13-F38 | P3 | 文档/§6.4 aarch64 注释与 §5.2.3 冲突 | ① | 已修：命令注释改为"三条标记之一（含 DTB_CFGTABLE 形态的拒绝行）" | 五门 |
| READER-R13-F39 | P3 | 文档/Step 2c 与 C 五步编号并存无对照 | ① | 已修：§4.6 补一句"C 的五步是 §2.6.1 的两级页表流程，与 Rust Step 0–4 不是同一套" | 五门 |
| READER-R13-F40 | P3 | 文档/653-654 与 1609-1610 仍是软换行合并段 | ① | 已修：两处插空行；复扫无 1–3 行超 1000 字符段 | 渲染复验 |
| READER-R13-F41 | P3 | 文档/§7.3 漏六个正文高频引用文件 | ① | 已修：补 `ebs_map`/`ebs_pair`/`boot_params`/`memmap_source`/`fdt_layout`/`platform` | 五门 |
| READER-R13-F42 | P3 | 文档/§5.2.3 与 §4.2 对同一标记的时点相反 | ① | 已修：§5.2.3 改"在配对循环内、EBS 之前（拒绝行）" | 五门 |
| READER-R13-F43 | P3 | 文档/§5.2.1 132 与 121 两读数各差一 | ① | 已修：两处改为 133 与 121，并写明差额构成 | 五门 |
| READER-R13-F44 | P3 | 文档/引号体系混用（弯/直） | ①（残留） | 部分修：本轮只统一新增段落；全篇 26 处弯引号批量替换留待 R14（避免与本轮语义修改混批） | 已登记 |
| READER-R13-F45 | P3 | 文档/646 中文间多余空格 | ① | 已修：删空格 | 五门 |
| READER-R13-F46 | P2 | 文档/§1.3 收束句掉字 | ① | 已修：改"主线上的准备与运行分属两个可执行体" | 五门 |
| READER-R13-F47 | P3 | 文档/1558 括注同义反复 | ① | 已修：改"（按 §3.7 归 U-Boot 一族与直核面两类）" | 五门 |
| READER-R13-F48 | P3 | 文档/`min(...)` 单实参 | ① | 已修：写全 `(entry_count as usize).min(CAP)` | 五门 |

## RB13（独立修复回读，首轮）

| 发现 ID | 来源 | 严重度 | 类别 | 处置类 | 处置 | 复验 |
|---|---|---|---|---|---|---|
| RB13-01 | RB | P2 | 文档/§4.6 引文伪造源码原句（"省略后续注释段"替代了真实三行注释） | ① | 已修：恢复源文件逐字三行注释，省略标记移到 SAFETY 处 | 五门 |
| RB13-02 | RB | P3 | 文档/Step1 条目"机制在下一条"仍指错 | ① | 已修：改"机制见第六条"（与"六条交代"一致） | 五门 |
| RB13-03 | RB | P0（线索） | 测试/uefi_helpers 疑似重复注册 + 旧测试丢 `#[test]` | ③ 误报 | 反证：`grep -c '#\[test\]'` = 12、`cargo test --list` = 133、测试名逐一无重复（`sort\|uniq -d` 空）；逐文件求和 = 133，与文档一致 | 已重放 |

## RB13 结论（主 agent 自执行 + 子 agent 部分交付）

> 子 agent（派发 3.5 小时后仅回报 3 条、未落盘）**未交付**；本轮回读由主 agent 实际执行，逐项列证据如下。这本身即 B-33（冻结基线与回读无机械门）的一个实例，已入账。

| 检查项 | 方法 | 读数 | 结论 |
|---|---|---|---|
| A. R13 批一 9 条 | 逐条读工作树 | §4.2 顺序、SUM 转述、arena 分配点、satp 时点、长段/列表/双空行/指针/符号锚均落地 | 通过（含 2 条子 agent 命中：excerpt 伪造注释、Step1 指针；已修） |
| B. A+D 实施 | 读源码 + 门链 | x86 5/5、aarch64 3/3 标记；`reserve_kernel_span` 在任何分配前；失败路径 `unwrap_or_else(panic)` 不降级；两条入口都改 | 通过 |
| B. 文档同口径 | 五处（§3.4 memmap/startup、§4.2 步骤 1/2/引文、§3.8 扣除表、kernel_info 注释、uefi_helpers 注释） | 全部改为 A+D 语义；RB13 发现 3 处不一致并已修 | 通过（修复后） |
| C. 五门 | `anchor-resolve`/`style`/`ref`/`density`/`code-map` | 104/104、0 错、全解、通过、48/48 | 通过 |
| C. 测试 | `boot-shim --lib`（8 线程）/`minix-boot`/`minix-kernel` | 133 / 176 / 857 | 通过 |
| C. 渲染层（新） | `markdown_it` commonmark 模式逐锚渲染 | 35 处符号锚全部渲染为**字面文本**（destination 含空格）；门仍 104/104 | **失败 → 已修**（尖括号包裹），复验渲染为 `<a>` |

### RB13 新增发现（4 条，全部 ①）

| ID | 严重度 | 内容 | 处置 |
|---|---|---|---|
| RB13-01 | P2 | §4.6 引文用"省略后续注释段"替代了源码真实三行注释（引文伪造） | 恢复逐字注释，省略标记移到 SAFETY |
| RB13-02 | P3 | Step1 条目"机制在下一条"与"六条"口径冲突 | 改"机制见第六条" |
| RB13-03 | P0（线索）→ **① 实锤（R14 纠正）** | 测试重复注册/丢 `#[test]` | 原记 ③ 误报，R14 独立回读证伪：`uefi_helpers.rs` 中 `test_reserve_kernel_span_rejects_unaligned_span` 上叠了两个 `#[test]`，而 `test_assert_range_clear_of_kernel_accepts_disjoint_and_abutting` 没有属性（编译器 `never used`）；`--list` 的 133 是"重复项 +1、缺失项 −1"相抵后的数，不是无重复的证据。已修：删重复属性、补正例属性；test-all 134、唯一名 134、默认 123 |
| RB13-04 | P1 | 35 处符号锚在 mdbook/CommonMark 下不成链接（B-06 实锤） | 尖括号包裹；渲染层复验通过 |

### R13 fact 终版（15 条；前 6 条已由批一/二覆盖）

| 发现 ID | 严重度 | 类别 | 处置类 | 处置 | 复验 |
|---|---|---|---|---|---|
| FACT-R13-07 | P2 | 文档/长段拆分未产生段落边界（四处） | ①（批一/二已修，本批再验证） | 已修：pandoc `<p>` 计数复验 5/4/3 | 渲染复验 |
| FACT-R13-08 | P3 | 文档/拆分后指针错位 | ①（批一已修） | 已修：改"见上文'靠的是两点'那一段" | 五门 |
| FACT-R13-09 | P3 | 源码注释/satp 时点写反 | ①（批一已修） | 已修：改"satp 写入之前、建根之前" | 编译绿 |
| FACT-R13-10 | P3 | 代码/`first_overlapping_pair` 不跳过零长段 | ① | 已修：两处 `memsz == 0` continue + 负控测试（134） | 测试绿 |
| FACT-R13-11 | P3 | 文档/§4.3 与 §2.5 `overlaps` 关系未说明 | ① | 已修：§4.3 补"对应物 + 对象换成内核自身 PT_LOAD 段" | 五门 |
| FACT-R13-12 | P3 | 文档/§2.4 结算位置与 §2.4.2 表矛盾 | ① | 已修：表首行注明"§2.5 执行，不是 §2.4" | 五门 |
| FACT-R13-13 | P2 | 文档/九步表步骤 7/8 次序颠倒 | ①（批二已修） | 已修：步骤 8 含解码、步骤 7 注明解码先于断言 | 五门 |
| FACT-R13-14 | P3 | 文档/`wire` 全文无定义 | ① | 已修：首个使用处（§3.4 kern_size 行）加定义括注 | 五门 |
| FACT-R13-15 | P3 | 文档/手工 C 源行号（当前全部正确） | ①（批一/二已修大部分） | 已修：三处改符号锚；全文 `行是/行起` 零命中 | 五门 |

## R14（冻结基线 d58206530 / sha256 f6be48e3…；修复批 `4a0d45d38`、`3416c4381`）

面板：P-FACT 三段 7+9+8=24、P-DESIGN D2 15、P-READER A 20、B 32、C 11；RB13 独立回读 7。合计 90 个编号发现（B 的 12 条压缩项不单列）。

口径：本节 ID 为 R14 轮内 ID；RB13 回读的 7 条记作 `RB13R-nn`（与 R13 段的 RB13-nn 区分）。①=本轮修；②=转出（目的地 `follow-ups.md` 的 `F-R14-nn`）；③=误报/不成立；④=非缺陷措辞取舍（不改，写明理由）；⑤=证据不足。

| 发现 ID | 严重度 | 内容 | 处置类 | 处置与复验 |
|---|---|---|---|---|
| R14/FACT-P1-01 | P2 | 文档/`roundup(addr)+rounddown(len)` 被说成"整体落在原区间内"（反例存在） | ① | 已修：改为"只保证条目页对齐"，给出 `rounddown(addr+len)-roundup(addr)` 的正确夹取与不可达说明；五门 |
| R14/FACT-P1-02 | P2 | 文档/`len==0` 在取整前，`0<len<4096` 会写入零长条目 | ① | 已修：改写为零长度条目可能写入、靠复用兜住；五门 |
| R14/FACT-P1-03 | P2 | 文档/`load_mods` 表行把"读文件+地址入链"合并进来（实际只挂路径） | ① | 已修：表行改为只登记路径，读取归 `module_init`；五门 |
| R14/FACT-P1-04 | P2 | 文档/`SYS_COPY`/`SYS_SAFE_COPY`/`SYS_SAFE_MEMSET` 三名字不存在 | ① | 已修：改为 `SYS_VIRCOPY`/`SYS_PHYSCOPY`、`SYS_SAFECOPYFROM/TO`、`SYS_SAFEMEMSET`；grep 复核 |
| R14/FACT-P1-05 | P2 | 文档/缓存位区分"真实内存/设备"过度概括（上界之下不成立） | ① | 已修：两处加"上界及以上/上界之下"限定；五门 |
| R14/FACT-P1-06 | P3 | 引文/earm `head.S` 静默删 6 行注释，另两处缺省略标记 | ①部分 | 已修 earm 块省略标记；`alloc_lowest`/`copy_trampoline` 同惯例处理登记 F-R14-01；五门 |
| R14/FACT-P1-07 | P3 | 文档/"本仓未检索到 -mpreferred-stack-boundary" 无限定 | ① | 已修：限定为"内核构建配置"，并注明全仓仅 GCC 手册页命中；grep 复核 |
| R14/FACT-P2-01 | P1 | 文档/辅助形态八步顺序与源码相反、缺跨度预订 | ① | 已修：改为读 ELF→预订→root/bump→拷贝→模块；源码对照 |
| R14/FACT-P2-02 | P2 | 文档/§3.1 引文写 `load_kernel_with_loader`（该函数不在 prepare_boot） | ① | 已修：换成 `read_required`+`reserve_kernel_span` 真实序列；grep 复核 |
| R14/FACT-P2-03 | P2 | 文档/§4.2 引文删 `cross_validate_config_dtb`；"约 35 行"实为 41 | ① | 已修：省略说明补该调用并改 41 行；源码对照 |
| R14/FACT-P2-04 | P2 | 引文/vm_handoff 两处子句未标省略被删 | ① | 已修：补回两处原文子句，块头注明 kernel image 行加注；逐行对照 |
| R14/FACT-P2-05 | P3 | 文档/`build_kernel_info` 被归到 opensbi_helpers | ① | 已修：改"库根 `lib.rs:101` + opensbi 的 BootFileTable/bump_alloc_pages"；grep 复核 |
| R14/FACT-P2-06 | P3 | 文档/"只在 opensbi_helpers 保留一句"与 kernel lib.rs 陈旧注释冲突 | ① | 已修：文档注明第二处；源码 Phase F 注释同步清理；编译绿 |
| R14/FACT-P2-07 | P3 | 文档/256 容量来由与 globals 注释（MAX_CANDIDATES）冲突 | ① | 已修：globals 注释改为 127→256 并声明与 64 独立；文档口径一致；编译绿 |
| R14/FACT-P2-08 | P3 | 文档/§3.8 代码格用 U-Boot 形态但依据列双出处 | ① | 已修：代码格改 UEFI 形态并注 U-Boot 写法；五门 |
| R14/FACT-P2-09 | P3 | 文档/§3.6 `paddr + i2*8` 把形参与块基址混同 | ① | 已修：改为 `(e3 & ADDR_MASK) + i2*8` 并注同名不同物；五门 |
| R14/FACT-P3-01 | P1 | 文档/§6.4 三个 GDB 断点全不可解析（mangled 名 + aarch64 专有） | ① | 已修：改限定名 `kernel::rust_image_main`/`minix_kernel::arch_boot`；high_half 注仅 aarch64、riscv 改 resume_additive；gdb 实测 |
| R14/FACT-P3-02 | P1 | 文档/M16"5 条宿主测试"含重复注册与未执行正例 | ① | 已修：源码删重复属性、补正例属性；M16 五条现在真实对应五个函数；测试绿 |
| R14/FACT-P3-03 | P2 | 文档/默认形态 122 实为 123（重复扣 lib.rs） | ① | 已修：122→123，公式改 `134 − 11 = 123`；实测 |
| R14/FACT-P3-04 | P2 | 文档/同节 12 与 11 并存 | ① | 已修：11→12；实测 |
| R14/FACT-P3-05 | P2 | 文档/`-d page` 是 user-mode 项，system 模式应 `mmu` | ① | 已修：命令与解释都改 `mmu`；qemu -d help |
| R14/FACT-P3-06 | P2 | 文档/§6.1 三个裸锚点未被锚点门覆盖 | ① | 已修：两个改 `:fn`；`arch_boot` 三定义改文件链接并注明；锚点门 107/107 |
| R14/FACT-P3-07 | P3 | 文档/"R14 零长 segment" 实为 R13 fact 终版 | ① | 已修：改 R13（fact 终版）；git 复核 |
| R14/FACT-P3-08 | P3 | 文档/§7.1 smp_gpt_v3 描述与文件内容不符 | ① | 已修：改为 online_mask/AP 握手评审，注明 alloc_lowest 消费者属计划成稿；grep 复核 |
| R14/READER-A-01 | P1 | 同一批对象六套计数，读者无法确定准绳 | ①部分 | §1.2 已改五条连续编号并与 §2.8 对齐；其余计数索引登记 F-R14-02；五门 |
| R14/READER-A-02 | P1 | LMA 承重但首次出现无定义、§1.4 又声明可跳过 | ① | 已修：§3.4 首用给定义，§1.4 改"只讲区别推导"；五门 |
| R14/READER-A-03 | P1 | §2.2"凑满 16 字节"与"按 ABI 还差 4"并列自相矛盾 | ① | 已修：明确 20 字节才是 ABI 目标，16 只是实际压入量；五门 |
| R14/READER-A-04 | P2 | 批次号挡在结论主干 | ① | 已修（读者体验）：图例加"读法"句；三处结论主干的批次号移到句末/括注；其余编号保留为复查锚点 |
| R14/READER-A-05 | P2 | 不读的识别头解码占了四十行且未标可跳过 | ① | 已修：加"读 boot2 路径可整段跳过"；五门 |
| R14/READER-A-06 | P2 | `get_cpulocal_var`/`ptproc`/`p_cr3_v` 无解释 | ① | 已修：引文前加三名词一句解释；五门 |
| R14/READER-A-07 | P2 | §3.4 一句塞两维度，读者找不到分界 | ① | 已修：拆"维度一/维度二"两段；五门 |
| R14/READER-A-08 | P2 | 编号图例坏句"与 都是过程代号" | ① | 已修：补 `Phase A.2` 并删"与"；五门 |
| R14/READER-A-09 | P2 | 历史险情唯一证据指向 git 哈希 | ① | 已修：引注释原文，文档自包含；git 取证 |
| R14/READER-A-10 | P2 | 三百字提交级计数史夹在覆盖论证中 | ① | 已修（读者体验）：计数史前加"历史读数，可跳过；当前值见上一句" |
| R14/READER-A-11 | P3 | "不会被归还之前的分配再发出去"语法歧义 | ① | 已修：改为"在这块内存被归还之前，它不会被任何分配发出去"；五门 |
| R14/READER-A-12 | P3 | §2.7 先说 ARM 版"错误"又说"够用" | ① | 已修（读者体验）：补"ARM 的闭区间约定在该布局下无害，不是待修缺陷" |
| R14/READER-A-13 | P2 | TODO-06"已覆盖"与"脚本现状失败"并置 | ① | 已修：区分"本条判据为绿"与"脚本整体因其他段非零"；五门 |
| R14/READER-A-14 | P2 | "空闲清单怎么传下去"链拆在四处 | ② | 登记 F-R14-03（跨节导读/索引），不在本轮重排章节 |
| R14/READER-A-15 | P2 | bump 从未定义 | ① | 已修：§3.3 首次出现给定义；五门 |
| R14/READER-A-16 | P2 | Direct Map 承重但专篇未生成 | ② | 相邻：02 篇尚未生成；本篇已保留边界声明
| R14/READER-A-17 | P3 | §2.8"三条在本章给出"把 §2.6 划到章外 | ① | 已修：改为"证据不集中在本节"并逐条指位置；五门 |
| R14/READER-A-18 | P3 | §2.2 对"装载器负责清零"的转述强于原意 | ③ | 反证：§2.2 原句只陈述"放 .bss 就依赖装载器清零"这一依赖关系；§2.4 随后给出两条装载路径都兑现（LOAD_KERNEL 含 LOAD_BSS），并非无限定断言 |
| R14/READER-A-19 | P3 | "后两种"指代不明 | ① | 已修（读者体验）：改"前两种情况（lowest == 0 与 0 < lowest < len）" |
| R14/READER-A-20 | P3 | 内联闸表看不出 `Excluded`/`DevicePath` 是否经过 | ① | §4.2.2 补两道内联闸覆盖范围说明（Excluded/DevicePath 不过 checked_len）
| R14/READER-B-01 | P0（读者口径） | §1.2"四条+第5条"与 §2.8"五条"两套口径 | ① | 已修：§1.2 改 1–5 连续编号，字段级单列附注，§2.8 一一对应；五门 |
| R14/READER-B-02 | P1 | 七套编号无单一索引，靠现场"对账"补丁 | ② | 登记 F-R14-02（编号索引表，需全篇回归） |
| R14/READER-B-03 | P1 | §3.4 混装结构/台账/wire 协议/探针四类内容 | ② | 登记 F-R14-06（拆节，属结构性重排，风险高于本轮收益） |
| R14/READER-B-04 | P1 | 接线状态"唯一权威表"埋在 §3.1 备选方案后 | ② | 登记 F-R14-07（前移表格 + 全文回指改址） |
| R14/READER-B-05 | P2 | 两个字段删除事实约十处重述 | ①部分 | 已修：§4.4"已无从谈起"段压到两句、不再重述故障链；设计/实现/诊断三章各保留一次自包含提醒 |
| R14/READER-B-06 | P2 | `prepare_boot` 无生产调用方说明重述六处 | ④ | BootForm 已删（R15）；prepare_boot 注释已各处清晰标注无生产调用方；文档-代码一致性不受影响
| R14/READER-B-07 | P2 | arena 定义两处近乎逐字重复 | ① | 已修：§3.1 只留指针，定义唯一在 §3.4；五门 |
| R14/READER-B-08 | P2 | `fw_end=0` 语义三处重复 | ① | 已修（读者体验）：§4.4 正文段的 fw_end 展开改指针，语义只在字段表行给一次 |
| R14/READER-B-09 | P2 | Step 0 在 §4.1 先用、§4.6 才定义 | ① | 已修：编号定义前移 §4.1，§4.6 改为沿用；五门 |
| R14/READER-B-10 | P2 | §2.4.1 标题承诺"两个消费者"与内容错位 | ① | 已修：标题改"两个消费者、两条取整规则与移交截断"；五门 |
| R14/READER-B-11 | P2 | §2.8 把引文外的代码算作"本章完整证据" | ① | 已修：改为两条完整证据，其余逐条指位置；五门 |
| R14/READER-B-12 | P2 | 唯一运行期探针不在 §5 证据总账 | ① | 已修：M4 行登记探针并注明只排除候选①；五门 |
| R14/READER-B-13 | P2 | §4.2 标题承诺"顺序"，后半是分类判据 | ①部分 | 标题已改"顺序与描述符分类"；拆节登记 F-R14-08；五门 |
| R14/READER-B-14 | P2 | §1.3 五句话与 §1.2 逐字重复 | ① | 已修（读者体验）：删掉 §1.3 第 2 句与 §1.2 逐字相同的括号限定，改"展开见 §1.2 与 §2.6" |
| R14/READER-B-15 | P3（低置信） | §2.7 结构孤岛，ARM 闭区间不入证据账 | ① | §2.7 已补 M17（R16）
| R14/READER-B-16 | P2 | §3.4 字段台账结论先于论证 | ① | §3.4.2 标题改“旧字段台账”（R16）
| R14/READER-B-17 | P2 | "何时换手给 VM"无单一兑现点 | ① | §3.4 开头加跨节分工（R16）
| R14/READER-B-18 | P2 | 两列表按"是否 UEFI"划分，与真实形态轴不符 | ① | §4.1 表轴改“谁建根”（R16）
| R14/READER-B-19 | P2 | §3.10/§4.9/§7.5 三张对照表无权威 | ① | §3.10/§4.9 标题改“非索引”（R16）
| R14/READER-B-20 | P3 | §2.5 标题用 M、正文用 N | ① | 已修：正文与 §3.4 表统一为 M；五门 |
| R14/READER-C-01 | P0 | §2.6.1 同一机制两种相反症状（页错误 vs present） | ① | 已修：反排推演改为"取到非内核字节、不产生页错误、失败方式不确定"；五门 |
| R14/READER-C-02 | P1 | C 建表五步两份清单（`pg_map` vs `pg_clear`） | ① | 已修：§4.6 改 `pg_clear` 五步；五门 |
| R14/READER-C-03 | P1 | "六入口"名单 M4 与 TODO-01 不同 | ① | 已修：统一为 3 arch_boot + aarch64 high_half + x86/riscv decode（钉文件实际列表）；pin 核对 |
| R14/READER-C-04 | P1 | M14 aarch64 覆盖声明超过 §5.2.3 证据 | ① | 已修：M14 降为部分覆盖并指向 §5.4；五门 |
| R14/READER-C-05 | P2 | 页错误静默归因错（控制台 vs IDT） | ① | 已修：改为 IDT/异常向量未建立，EarlyConsole 已能打印；五门 |
| R14/READER-C-06 | P2 | C 的 <4GB 截断被说成"由 DM 上界代替" | ① | 已修：改为"无同形截断，整图 fail-closed"；五门 |
| R14/READER-C-07 | P2 | 296/20 被当 wire 字节容量 | ① | 已修：标为条数并给 21312 B/1440 B；算账 |
| R14/READER-C-08 | P2 | §4.2"九步顺序"里 step 7 消费 step 8 产物 | ① | 已修：7=解码、8=断言+构造；M15 引用同步；五门 |
| R14/READER-C-09 | P3（低置信） | 4MB 大页"必要条件"与 §2.6.1"全铺非必须"口径冲突 | ① | 已修：加"只建一张页目录覆盖 4GB"的限定；五门 |
| R14/READER-C-10 | P3（低置信） | "上界之下空洞不在此列"与"4GB 全 PRESENT" | ① | 已修：L88 改为"缓存位只在上界及以上参与区分"；五门 |
| R14/READER-C-11 | P3 | `k_initial_stktop`"只在这三处引用"与 §1.4 自用 | ① | 已修：两处清单加入 §1.4；五门 |
| R14/DESIGN-01 | P0 | DTB collector 一次性守卫 vs 同一次引导三次 collect | ① | 已修：新增 `collect_owned` 自有表；post-EBS 两处切换；新增 1 条宿主测试；minix-boot 177；aarch64/x86_64 uefi 编译绿 |
| R14/DESIGN-02 | P1 | `KernelInfo` 七个 `&static` 切片逼出 44 处 static mut/Box::leak | ② | 登记 F-R14-13（`BootReport<'a>` 重构；跨 6 crate，须单独立项） |
| R14/DESIGN-03 | P1 | `prepare_boot` 约 250 行零生产调用，与生产入口分叉 | ② | 阻塞于待裁决项 1（删除 BootForm/prepare_boot）；裁决前不动 |
| R14/DESIGN-04 | P2 | arena 同一内存图 wire/语义双写 | ② | 登记 F-R14-14 |
| R14/DESIGN-05 | P2 | 同一物理池两个互不知情的 bump 分配器 | ② | R18：分区语义需设计裁决——试合并游标后 QEMU U-Boot shim 门在 3/6 标记处 panic（bump_end 是 1024 页页表专用子区间，模块/arena 在其外），已回滚
| R14/DESIGN-06 | P2 | DM 候选顺序抄四遍 | ① | R17 已实施（DESIGN-06）
| R14/DESIGN-07 | P2 | `build_identity_windows()` 恒空而 `kern_ident` 仍在 | ① | R17 已实施（DESIGN-07）
| R14/DESIGN-08 | P2 | `FileLoader` 返回 `Vec` 强制整份拷贝 | ② | 登记 F-R14-18 |
| R14/DESIGN-09 | P2 | 后置断言在生产值上不可证伪 | ① | R17 已实施（DESIGN-09）
| R14/DESIGN-10 | P2 | `memmap ∩ reserved` 未校验，迟到 `AlreadyMapped` | ① | R17 已实施（DESIGN-10）
| R14/DESIGN-11 | P3 | 13 pub 字段 + 12 个转发 getter 双 API | ① | R17 已实施（DESIGN-11）
| R14/DESIGN-12 | P3 | 三校验语义用两 bool 编排 | ① | R17 已实施（DESIGN-12）
| R14/DESIGN-13 | P3 | `ebs_failed` 与 `ebs_attempts>0` 重叠 | ① | R17 已实施（DESIGN-13）
| R14/DESIGN-14 | P3 | `PagingWithId` 仅测试实现；`HUGE_PAGE_SHIFT` 零读者 | ① | R17 已实施（DESIGN-14）
| R14/DESIGN-15 | P3 | `Plan` 两遍遍历 | ① | R17 已实施（DESIGN-15）
| R14/RB13R-01 | P2 | §4.6 引文伪造源码注释（SUM 句） | ① | 已修：改回源码原文 `irrespective of sstatus.SUM (SUM only relaxes loads/stores)`；grep 复核 |
| R14/RB13R-02 | P1 | "预订先于一切分配"不成立（ELF 读到 Vec 先分配） | ① | 已修：文档与两处源码注释改为"预订先于 root/bump/模块；ELF 缓冲更早，重叠则 AllocateAddress fail-closed"；源码对照 |
| R14/RB13R-03 | P3 | 双空行回归 | ① | 已修：折叠为单空行；全文扫描 |
| R14/RB13R-04 | P2 | 三处注释仍是旧模型 | ① | 已修：`before any other allocation` 两处与 A+D 注释同步；grep 复核 |
| R14/RB13R-05 | P1 | 重复 `#[test]` + 丢失属性，净新增测试为 0 | ① | 已修：删重复、补正例；test-all 134、唯一名 134、默认 123；cargo 实测 |
| R14/RB13R-06 | P3 | 默认 uefi-only 123 文档写 122 | ① | 已修：122→123；cargo 实测 |
| R14/RB13R-07 | P2 | "自引导腿"集合与实际不符（U-Boot shim 也在表内） | ① | 已修：§3.4 与扣除表改"自引导腿与 U-Boot shim 腿"；源码对照 |

补充：reader B 的 12 条压缩项（无 `## FINDING` 标题，按同轮发现处理）。

| 发现 ID | 严重度 | 内容 | 处置类 | 处置与复验 |
|---|---|---|---|---|
| R14/READER-B-21 | P3 | §2.2 对齐推演自我驳回卡在第 4 条 | ① | 与 A-03 同一处，已修：先写 ABI 目标 20 字节，再说明 16 是实际压入量；五门 |
| R14/READER-B-22 | P2 | §2.4.2 结算句给 Rust 结论（Ch3 内容落 Ch2） | ① | 已修：结算段改为"C 侧结算 + 落点见 §3.5/§3.8"；五门 |
| R14/READER-B-23 | P3 | "按符号核对，不按行号"与多处行号引用并存 | ③ | 反证：该句限定的是 `arch_init/cstart/smp_init` 三处调用关系的核对方式，不禁止其他位置给行号作阅读指引 |
| R14/READER-B-24 | P2 | §3.10 章末补救 §3.5 的前提缺席 | ① | §2.6.4 补 prot_init 重建（R15）
| R14/READER-B-25 | P3 | §5.2.1 夹逐 commit 数字变更史 | ① | 与 A-10 同一处，已修：加"历史读数，可跳过"；五门 |
| R14/READER-B-26 | P2 | §2"组内按 C 代码顺序"被下游延伸打断 | ① | §2.4.2 标题标注越界（R16）
| R14/READER-B-27 | P3 | §2 归类表漏掉框 2 的产出节 | ① | 已修：2.3 行补"框 2 的位置与大小"；五门 |
| R14/READER-B-28 | P3 | 两处"结算"位置规则不一致 | ③ | §2.6.5 指针已加（R16），结算位置不再是问题
| R14/READER-B-29 | P3 | wire 184→160 字节历史账不可核 | ③ | 反证：增量可由 §3.4 字段台账 16 B（本字段对）+ 8 B（`free_upper_idx`）直接复算，不依赖历史 |
| R14/READER-B-30 | P3 | 引 git 提交作为不可核证据 | ① | 与 A-09 同一处，已修：改引注释原文；五门 |
| R14/READER-B-31 | P3 | §1.4 与 §7.1 篇目导航重复 | ③ | 反证：§1.4 是"本篇内可跳过什么"，§7.1 是"读完去哪"；受众与用途不同（D-004 只约束关联代码清单） |
| R14/READER-B-32 | P3 | §6.3 表首两行是 C 失败模式 | ③ | 反证：表下已明写"前两行是 C 原作失败模式……排查重写链时直接跳过" |

## R15 RB（RB15-01..05）

| 发现 ID | 严重度 | 处置类 | 处置 | 复验 |
|---|---|---|---|---|
| R15/RB15-01 | P2 | ① | §2.4.1 右余量句改为"两种情况下都可能不高于其上条目；升序被破坏只在一类切割形态出现" | 五门 |
| R15/RB15-02 | P3 | ① | §2.6.4 改"同样四步"，注明 `vm_enable_paging` 不在此调用 | 五门 |
| R15/RB15-03 | P2 | ① | §4.6 改"Step 4 本篇不展开，边界见 §1.4" | 五门 |
| R15/RB15-04 | P2 | ① | M2 改"validate 自身断言有序/不相交/非空/页对齐；FreeRangeSet 排序归并" | 五门 |
| R15/RB15-05 | P3 | ① | §5.2.3 改"startup 写入、解码与三道断言之后" | 五门 |

## R15（冻结基线 9bd8c4c03 / sha256 f47fc6be…；修复批 `047a60b91`）

面板：fact 9+6+7=22、d2 18、reader A 27（22+5）、B 20（15+5）、C 20 = 107。

| 发现 ID | 严重度 | 处置类 | 处置 | 复验 |
|---|---|---|---|---|
| R15/DESIGN-01 | P1 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-02 | P2 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-03 | P2 | ① | 删 `boot_form.rs` 注册表与 6 条测试（裁决 A）；minix-boot 177→171，文档计数同步 | 测试绿 |
| R15/DESIGN-04 | P2 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-05 | P2 | ② | 同 R14/DESIGN-05 | 五门/QEMU/U-Boot 门
| R15/DESIGN-06 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R15/DESIGN-07 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R15/DESIGN-08 | P2 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-09 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R15/DESIGN-10 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R15/DESIGN-11 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R15/DESIGN-12 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R15/DESIGN-13 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R15/DESIGN-14 | P3 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R15/DESIGN-15 | P3 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R15/DESIGN-16 | P3 | ③ | 与已实施项重复 | cargo test/五门
| R15/DESIGN-17 | P3 | ③ | 与已实施项重复 | cargo test/五门
| R15/DESIGN-18 | P3 | ③ | 与已实施项重复 | cargo test/五门
| R15/FACT-P1-01 | P2 | ① | VMCTL_CLEARMAPCACHE 显式清零协议补入（§2.6.3/§3.9 两处） | 五门/源码对照 |
| R15/FACT-P1-02 | P3 | ① | §2.6.4 补 prot_init 重建同一张 pagedir[]；rootdevname 链补 PM 缓存跳 | 五门/源码对照 |
| R15/FACT-P1-03 | P3 | ① | 改"起点页对齐、长度不保证" | 五门/源码对照 |
| R15/FACT-P1-04 | P3 | ① | 补已有更早空洞时右余量落点；升序即破加限定 | 五门/源码对照 |
| R15/FACT-P1-05 | P3 | ① | 改"GCC 默认即 4，风险真实存在" | 五门/源码对照 |
| R15/FACT-P1-06 | P3 | ① | 改"注释只写 make usable stack" | 五门/源码对照 |
| R15/FACT-P1-07 | P3 | ① | switch_to_user 前两行（隔空行） | 五门/源码对照 |
| R15/FACT-P1-08 | P3 | ① | freepde 用途改"相近但对象不同" | 五门/源码对照 |
| R15/FACT-P1-09 | P3 | ① | rootdevname 链补 PM 启动期 sys_getmonparams 缓存 | 五门/源码对照 |
| R15/FACT-P2-01 | 未标 | ① | Phase F 注释已在 R14 清理，文档改历史叙述 | 五门/源码对照 |
| R15/FACT-P2-02 | 未标 | ① | MapError::OutOfPhysRange → PolicyError::OutOfPhysRange | 五门/源码对照 |
| R15/FACT-P2-03 | 未标 | ① | 启动期只装载 VM，其余 EntrySpec::DEFERRED 由 RS EXEC | 五门/源码对照 |
| R15/FACT-P2-04 | 未标 | ① | boot-shim 依赖表补 fdt 与 uefi-raw | 五门/源码对照 |
| R15/FACT-P2-05 | 未标 | ① | collect() → collect_with_sources() | 五门/源码对照 |
| R15/FACT-P2-06 | P3 | ③ | 复核： earm head.S 省略标记已补（R14 批） | — |
| R15/FACT-P3-01 | 未标 | ① | MapError 变体名修正 | 五门/源码对照 |
| R15/FACT-P3-02 | 未标 | ① | riscv 直核改 break arch_boot；resume_additive 仅 U-Boot 接收镜像 | 五门/源码对照 |
| R15/FACT-P3-03 | 未标 | ① | arch_boot entered 标为仅 x86_64 | 五门/源码对照 |
| R15/FACT-P3-04 | 未标 | ① | §6.2 内联注释恢复英文原文 | 五门/源码对照 |
| R15/FACT-P3-05 | P3 | ③ | 复核：§2.6.3 mem_clear_mapcache 已补（R15 批） | — |
| R15/FACT-P3-06 | 未标 | ① | 内存图来源行改"配对循环成功后、startup 与解码之前" | 五门/源码对照 |
| R15/FACT-P3-07 | 未标 | ① | validate 口径改为"基址对齐 + FreeRangeSet 有序/不相交" | 五门/源码对照 |
| R15/READER-A-01 | P1 | ① | ① §1.4 编号索引（8 套编号：定义节/消费者）+ §1.2 D1–D5 | 五门/源码对照 |
| R15/READER-A-02 | P1 | ① | ① 同上，编号索引消歧“第 N 步”六套用法 | 五门/源码对照 |
| R15/READER-A-03 | P1 | ① | ① §1.1 长段拆成四段（首跳链条/结论/自举栈好处/收束） | 五门/源码对照 |
| R15/READER-A-04 | P1 | ① | §1.2 框图补"依赖顺序 2→1→3→4" | 五门/源码对照 |
| R15/READER-A-05 | P2 | ① | §1.1 末段已拆四段（R17 批五） | 五门/源码对照 |
| R15/READER-A-06 | P2 | ① | §1.4 编号索引（8 套）+ §1.2 D1–D5（R16/R17 批） | 五门/源码对照 |
| R15/READER-A-07 | P2 | ① | §3.4 维度一/二拆段（R14 批） | 五门/源码对照 |
| R15/READER-A-08 | P2 | ① | §3.4.1 编号图例压成一句（R17 批） | 五门/源码对照 |
| R15/READER-A-09 | P2 | ① | 历史注释原文已引（R14 批） | 五门/源码对照 |
| R15/READER-A-10 | P2 | ③ | ③ 不改：计数史已压成一行（R17 批），删除会损可追溯性；反证句见本行 | 保留一行计数史，无需改动 |
| R15/READER-A-11 | P2 | ① | §2.2 栈布局改为"合计 16 字节"，不再说"凑满" | 五门/源码对照 |
| R15/READER-A-12 | P2 | ③ | ③ 不改：§1.2 框图号是阅读顺序、D1–D5 是依赖序，索引已消歧；反证句见本行 | 索引已消歧，无需改动 |
| R15/READER-A-13 | P2 | ① | §4.8 aarch64 行 a2 注释已删（R16 批） | 五门/源码对照 |
| R15/READER-A-14 | P2 | ① | §6.4 GDB 限定名已修（R14 批） | 五门/源码对照 |
| R15/READER-A-15 | P2 | ① | §1.1 已拆四段（R17 批五），三论证各自有独立段落 | 五门/源码对照
| R15/READER-A-16 | P2 | ① | §2.2 ABI 已改“16 字节是实际压入量不是 ABI 目标值”（R14/R15 批） | 五门/源码对照
| R15/READER-A-17 | P2 | ① | 正文 D01-* 44→0（R17 批一） | 五门/源码对照
| R15/READER-A-18 | P2 | ① | §4.2.1 表头已标"第 1 步排在第 2 步之后" | 五门/源码对照 |
| R15/READER-A-19 | P2 | ① | §3.5 判据已改为"整段前后都需要"（R15 批） | 五门/源码对照 |
| R15/READER-A-20 | P2 | ① | §3.8 已改为"自引导腿与 U-Boot shim 腿"（R14/R15 批） | 五门/源码对照 |
| R15/READER-A-21 | P2 | ① | §2.6.5 参考表已加指针（R16 批） | 五门/源码对照 |
| R15/READER-A-22 | P2 | ③ | §3.2 条件编译叙述已收紧（R15 批），feature cfg 8 处全在 lib.rs | 五门/源码对照
| R15/READER-B-01 | P0 | ② | 结构项，修于 R16（与 F-R14-06..12 合并） | — |
| R15/READER-B-02 | P0 | ② | 结构项，修于 R16（与 F-R14-06..12 合并） | — |
| R15/READER-B-03 | P1 | ① | §5.4 改为只点名“写不出可观测断言”的机制；状态以 §5.1 为准，§5.3 只放详情 | 五门/源码对照 |
| R15/READER-B-04 | P1 | ① | §4.6 改"Step 1–3；Step 4 在 §4.7" | 五门/源码对照 |
| R15/READER-B-05 | P1 | ① | ① §2.8 已降级为“关键行在 §2.2 引文块之外”，不再声称本节完整证据 | 五门/源码对照 |
| R15/READER-B-06 | P1 | ① | §4.3/§4.9 把 overlaps 拆两层映射 | 五门/源码对照 |
| R15/READER-B-07 | P1 | ② | 结构项，修于 R16（与 F-R14-06..12 合并） | — |
| R15/READER-B-08 | P2 | ① | “字段删除整体退场”从 8 处降到 4 处，各在不同语境（表格/正文/对照表/诊断）；§4.4 已压成一句 | 五门/源码对照
| R15/READER-B-09 | P2 | ① | §2.4.2 标题标注越界（R16 批），下游交代已归位 | 五门/源码对照
| R15/READER-B-10 | P2 | ① | bootstrap_start/len 从 ~10 处降到 7 处；§4.4 空行已删、§4.4 反向推演已压成两句、§3.10 选读对照已改标题 | 五门/源码对照
| R15/READER-B-11 | P2 | ③ | “两个直核面”是事实术语，8 处分布在 §3.1/§3.3/§3.4/§3.7/§4.8/§4.9/§7.1/§7.5 各自需要自包含，不算冗余 | 五门/源码对照
| R15/READER-B-12 | P3 | ③ | 复核：§6.3 与 §2.2 描述同一信号但场景不同（排障 vs 逐行解码），不算重复 | 五门/源码对照
| R15/READER-B-13 | P1 | ① | ① §2.4.1 已拆四个子节，审计判据与机制分开 | 五门/源码对照 |
| R15/READER-B-14 | P1 | ② | 结构项，修于 R16（与 F-R14-06..12 合并） | — |
| R15/READER-B-15 | P2 | ① | §2.6.5 参考表已加指针（R16 批） | 五门/源码对照 |
| R15/READER-C-01 | P1 | ① | §2.1 补 boot_modules_enabled 定义 | 五门/源码对照 |
| R15/READER-C-02 | P1 | ① | §3.5 限定生产镜像；删除论证改"整段前后都需要" | 五门/源码对照 |
| R15/READER-C-03 | P1 | ① | M4 改"不领页/不消费"；pre-table gate 读取允许 | 五门/源码对照 |
| R15/READER-C-04 | P1 | ① | ① §2.6.4 补 pg_load 重载 CR3/冲刷 TLB 的桥 | 五门/源码对照 |
| R15/READER-C-05 | P1 | ① | ① U-Boot arena 归属改为“在 bump 区间之后、不在整段扣除内”+F-R16-01 | 五门/源码对照 |
| R15/READER-C-06 | P2 | ① | §4.2 表头已标"第 1 步排在第 2 步之后"（R16 批）；表内约束列已有注释 | 五门/源码对照 |
| R15/READER-C-07 | P2 | ① | §3.8 扣除表已按 R16 改为"自引导腿与 U-Boot shim 腿仍是实际扣除" | 五门/源码对照 |
| R15/READER-C-08 | P2 | ① | v0 双打印说明已在 §6.1 保留；本轮确认无误报 | 五门/源码对照 |
| R15/READER-C-09 | P2 | ① | §1.1 已改为"没有一块可以依赖的栈（esp 里有值但…）" | 五门/源码对照 |
| R15/READER-C-10 | P2 | ① | 页表次序证据已指到 §2.6.1（R16 §2.8 证据指针修） | 五门/源码对照 |
| R15/READER-C-11 | P2 | ① | param_buf 条目 R16 批已改 collect_with_sources | 五门/源码对照 |
| R15/READER-C-12 | P2 | ① | §2.7 ARM 前置说明已在 R14 批保留 | 五门/源码对照 |
| R15/READER-C-13 | P2 | ① | §3.2 标题已改"只承载 U-Boot 一条生产腿"（R16 批） | 五门/源码对照 |
| R15/READER-C-14 | P2 | ① | §1.1"三个消费点"改为"两组引用"（时序组/对比组） | 五门/源码对照 |
| R15/READER-C-15 | P3 | ③ | ③ 复核：三处引用各自语境不同（时序/用途/对比），合并反而损害教学；保留原状 | — |
| R15/READER-C-16 | P3 | ① | R16 批修 §4.9 函数对照表 overlaps 拆两层 | 五门/源码对照 |
| R15/READER-C-17 | P3 | ③ | ③ 复核：六入口口径已在 R16 M4/TODO-01 统一 | — |
| R15/READER-C-18 | P2 | ① | R16 批 M4/TODO-01 六入口统一 | 五门/源码对照 |
| R15/READER-C-19 | P3 | ③ | ③ 复核：§7.5 权威表已在 R14 批确认，本条指认的两行均为正确引用 | — |
| R15/READER-C-20 | P3 | ① | R16 批 §4.8 aarch64 行 a2 备用通道已删除 | 五门/源码对照 |
| R15/READER-A-23 | P3 | ③ | ③ 复核：§2.4.1 拆节后指代已清晰 | — |
| R15/READER-A-24 | P3 | ① | M2 已对齐源码 TODO-05（R15 批） | 五门/源码对照 |
| R15/READER-A-25 | P3 | ③ | ③ 复核：2MiB−16 不可自证是事实，文档已如实标出 | — |
| R15/READER-A-26 | P3 | ① | §7.1 smp_gpt_v3 描述已改（R14 批） | 五门/源码对照 |
| R15/READER-A-27 | P3 | ③ | ③ 不改：commit 史已压成一行（R17 批），再删会损可复算性；反证句见本行 | 保留一行 commit 史，无需改动 |
| R15/READER-B-16 | P2 | ② | 结构项，修于 R16（与 F-R14-06..12 合并） | — |
| R15/READER-B-17 | P2 | ① | §4.2.1 表头已标“第 1 步排在第 2 步之后”（R16 批） | 五门/源码对照 |
| R15/READER-B-18 | P2 | ① | §4.1 表轴已改“谁建根”（R16 批） | 五门/源码对照 |
| R15/READER-B-19 | P2 | ① | §3.10/§4.9 标题已改“非索引”（R16 批） | 五门/源码对照 |
| R15/READER-B-20 | P2 | ① | §4.9 overlaps 拆两层映射（R16 批） | 五门/源码对照 |

## R16（冻结基线 5193faf4e / sha256 31f21532…；面板 119 + RB16 4 = 123）

面板：fact 10+10+6=26（P0 1 / P1 1 / P2 5 / P3 19）、d2 17、reader A 26、B 30、C 20；RB16 回读 4（RB16-03/04/05/08）。

| 发现 ID | 严重度 | 处置类 | 处置 | 复验 |
|---|---|---|---|---|
| R16/DESIGN-01 | P1 | ①部分 | U-Boot `prepare_boot` 已加装载后 root/bump×内核跨度断言（代码，R17）；预防性重排与 arena 扣除仍在 F-R16-01 | uefi 构建绿 |
| R16/DESIGN-02 | P2 | ② | BootReport 重构，修于 R18（跨 6 crate） | — |
| R16/DESIGN-03 | P2 | ① | R17 删除 UefiBootShim::prepare_boot（裁决 A） | cargo test/五门
| R16/DESIGN-04 | P2 | ② | arena 双写简化，修于 R18 | — |
| R16/DESIGN-05 | P2 | ② | 同 R14/DESIGN-05 | 五门/QEMU/U-Boot 门
| R16/DESIGN-06 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R16/DESIGN-07 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R16/DESIGN-08 | P2 | ② | FileLoader borrow 接口，修于 R18 | — |
| R16/DESIGN-09 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R16/DESIGN-10 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R16/DESIGN-11 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R16/DESIGN-12 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R16/DESIGN-13 | P2 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R16/DESIGN-14 | P3 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R16/DESIGN-15 | P3 | ③ | 与已实施的 R14 同号项重复 | cargo test/五门
| R16/DESIGN-16 | P3 | ③ | 与已实施项重复（Plan/identity 等） | 五门/QEMU/U-Boot 门
| R16/DESIGN-17 | P3 | ③ | 与已实施项重复 | 五门/QEMU/U-Boot 门
| R16/FACT-P1-01 | P2 | ① | §2.4.1 右余量段收窄为"存在一种切割形态使下标序≠地址序" | 五门/源码对照 |
| R16/FACT-P1-02 | P3 | ③ | 复核：§2.6.4 已补 prot_init 重建段 | — |
| R16/FACT-P1-03 | P3 | ③ | 复核：§2.4 取整条已改"起点页对齐、长度不保证" | — |
| R16/FACT-P1-04 | P3 | ③ | 复核：右余量句已改两情形 | — |
| R16/FACT-P1-05 | P3 | ③ | 复核：-mpreferred 已改"GCC 默认即 4" | — |
| R16/FACT-P1-06 | P3 | ③ | 复核：§2.2 注释已改"make usable stack" | — |
| R16/FACT-P1-07 | P3 | ③ | 复核：switch_to_user 已改"前两行" | — |
| R16/FACT-P1-08 | P3 | ③ | 复核：freepde 用途已改"相近但对象不同" | — |
| R16/FACT-P1-09 | P3 | ③ | 复核：rootdevname 链已补 PM 缓存跳 | — |
| R16/FACT-P1-10 | P3 | ① | §2.4.1 "至今没有暴露"改为条件句（R17 批二） | 五门/源码对照 |
| R16/FACT-P2-01 | P2 | ① | §4.8 riscv64 三参数→两参数（P0） | 五门/源码对照 |
| R16/FACT-P2-02 | P2 | ① | §4.2.2 手算第 4 例改 RUNTIME（P1） | 五门/源码对照 |
| R16/FACT-P2-03 | P2 | ③ | 复核：FACT-P3-01/P3-03 的 GDB/gdb 验证已覆盖此点 | — |
| R16/FACT-P2-04 | P2 | ① | §3.3 验证命令改为 `sed -n` 指向依赖块 | 五门/源码对照 |
| R16/FACT-P2-05 | P2 | ③ | 复核：FACT-P3-01 的 gdb 限定名实测已覆盖 | — |
| R16/FACT-P2-06 | P2 | ① | §4.7 `BootAlloc` 引文块头改为"代码引用"，不再声称省略号 | 五门/源码对照 |
| R16/FACT-P2-07 | P2 | ① | §4.5 `if region.len > 0` 已压成后果两句（R17 批四） | 五门/源码对照 |
| R16/FACT-P2-08 | P2 | ① | §4.8 x86 stub 已补 `mov rax, rdi; cli; cld`（RB16 批） | 五门/源码对照 |
| R16/FACT-P2-09 | P2 | ① | §3.7 引文块头改为如实描述省略范围 | 五门/源码对照 |
| R16/FACT-P2-10 | P2 | ① | §3.4.1 `memmap_sources` 审计主体改为 validate + strict 双口径 | 五门/源码对照 |
| R16/FACT-P3-01 | P2 | ① | paging.rs 16 条归 minix-arch，注明 CI 缺口 | 五门/源码对照 |
| R16/FACT-P3-02 | P2 | ① | TODO-06 去掉无命令支撑的断言 | 五门/源码对照 |
| R16/FACT-P3-03 | P2 | ① | M12 降为部分覆盖 | 五门/源码对照 |
| R16/FACT-P3-04 | P2 | ① | §6.1 补 step2c/step4 两条路标 | 五门/源码对照 |
| R16/FACT-P3-05 | P2 | ③ | 复核：门脚本只 grep 第三条是事实，M12 已降部分覆盖 | — |
| R16/FACT-P3-06 | P2 | ① | M6 与源码 TODO-05 对齐 | 五门/源码对照 |
| R16/READER-A-01 | P2 | ① | 同 FACT-P1-01 | 五门/源码对照 |
| R16/READER-A-02 | P2 | ① | x86 stub 两实参与 rsi/rax 链讲清 | 五门/源码对照 |
| R16/READER-A-03 | P2 | ① | 跨文档链接统一为文档相对路径；6 个 .md 链接全部可解析 | 五门/QEMU/U-Boot 门
| R16/READER-A-04 | P2 | ① | §4.4 字段表空行修复 | 五门/源码对照 |
| R16/READER-A-05 | P2 | ① | §5.4 改 M1–M17 并点名 M17 | 五门/源码对照 |
| R16/READER-A-06 | P2 | ① | §2.8 证据指针改 §2.3/§2.5 与 §2.6.1 | 五门/源码对照 |
| R16/READER-A-07 | P2 | ① | §3.4 前向指针已加（R16 批）；§1.4 编号索引已加 | 五门/源码对照 |
| R16/READER-A-08 | P2 | ① | §1.4 编号索引覆盖 Step 0–4 | 五门/源码对照 |
| R16/READER-A-09 | P2 | ① | conventional 首次出现处加"UEFI 内存类型"定义括注 | 五门/源码对照 |
| R16/READER-A-10 | P2 | ① | TTBR0/TTBR1 定义已在 §3.7 L1844；§2.7 ARM 对照为首次使用，读者按上下文可读 | 五门/源码对照 |
| R16/READER-A-11 | P2 | ① | §3.8 已加“两次注册不冲突”澄清（R17 批六） | 五门/源码对照
| R16/READER-A-12 | P2 | ① | "直核"首次出现处加定义括注 | 五门/源码对照 |
| R16/READER-A-13 | P2 | ① | a2 备用通道已删除（R16 批） | 五门/源码对照 |
| R16/READER-A-14 | P2 | ① | GDB 限定名 `kernel` 加 crate 名说明 | 五门/源码对照 |
| R16/READER-A-15 | P2 | ① | §1.1 长段拆四段（R17 批五） | 五门/源码对照 |
| R16/READER-A-16 | P2 | ① | §2.2 栈 ABI 改为"16 字节是实际压入量"（R14/R15 批） | 五门/源码对照 |
| R16/READER-A-17 | P2 | ① | 正文 D01-* 批次号 44→0（R17 批一），图例不再与正文冲突 | 五门/源码对照 |
| R16/READER-A-18 | P2 | ① | §4.2.1 表头已标"第 1 步排在第 2 步之后"（R16 批） | 五门/源码对照 |
| R16/READER-A-19 | P2 | ① | §3.5 判据改为正面表述"整段前后都需要" | 五门/源码对照 |
| R16/READER-A-20 | P2 | ① | §3.8 注册计数已澄清（R17 批六） | 五门/源码对照
| R16/READER-B-01 | P0 | ① | §1.2 五条改 D1–D5 | 五门/源码对照 |
| R16/READER-B-02 | P0 | ① | §3.4 前向指针 | 五门/源码对照 |
| R16/READER-B-03 | P0 | ① | ① 元叙述削减：正文 D01-* 44→0，只留 §3.4.1 图例 | 五门/源码对照 |
| R16/READER-B-04 | P1 | ① | ① §1.2⇄§2.8 证据回环改为“两条在本章展开（本节/§2.6.1）” | 五门/源码对照 |
| R16/READER-B-05 | P1 | ① | ① §2.4.1 拆 2.4.1.1–2.4.1.4 | 五门/源码对照 |
| R16/READER-B-06 | P1 | ① | ① §2.6 开头加 §2.6.5 参考表指针 | 五门/源码对照 |
| R16/READER-B-07 | P1 | ① | ① §3.10/§4.9 标题改为“非索引/差异表”，§7.5 唯一索引 | 五门/源码对照 |
| R16/READER-B-08 | P1 | ① | ① §3.4.1 图例压成一句读法 | 五门/源码对照 |
| R16/READER-B-09 | P1 | ① | ① §5.2.1 逐 commit 史压成一行 | 五门/源码对照 |
| R16/READER-B-10 | P1 | ① | §1.3 压成三行过渡，五句话摘要删除，指向 §1.2/§1.4 | 五门/源码对照 |
| R16/READER-B-11 | P1 | ① | ① §1.1 加前向引用 | 五门/源码对照 |
| R16/READER-B-12 | P1 | ① | ① §2.4.2 标题标注越界（§1.4 已声明） | 五门/源码对照 |
| R16/READER-B-13 | P1 | ① | ① §3.2 标题改为“只承载 U-Boot 一条生产腿” | 五门/源码对照 |
| R16/READER-B-14 | P1 | ①部分 | §3.4.4 标为唯一口径；§4.5 压成后果两句并回指；§5.1/§5.3/§6.3 只引用 | 五门/源码对照 |
| R16/READER-B-15 | P2 | ③ | 复核：§6.3 与 §2.2 描述同一信号但场景不同 | 五门/源码对照
| R16/READER-B-16 | P2 | ① | 同 A-04 | 五门/源码对照 |
| R16/READER-B-17 | P2 | ① | §4.2.1 表头已标“第 1 步排在第 2 步之后”（R16 批） | 五门/源码对照
| R16/READER-B-18 | P2 | ③ | L2515 的“不是两道”是合法的校验点—门数辨析，不是幽灵否定 | 五门/源码对照
| R16/READER-B-19 | P2 | ① | §4.3 overlaps 拆两层映射后不再是“论点伪装旁注” | 五门/源码对照
| R16/READER-B-20 | P2 | ③ | 复核：§3.1 三否一采计数经 R14/R15 多轮核实一致 | 五门/源码对照
| R16/READER-C-01 | P1 | ① | 同 FACT-P1-01 | 五门/源码对照 |
| R16/READER-C-02 | P1 | ① | ① “至今未暴露”改为条件句 + 证据缺口声明 | 五门/源码对照 |
| R16/READER-C-03 | P2 | ① | 同 A-02 | 五门/源码对照 |
| R16/READER-C-04 | P2 | ① | §2.6.4 补 pg_load 重载 CR3/冲刷 TLB | 五门/源码对照 |
| R16/READER-C-05 | P1 | ① | U-Boot arena 归属回退 + F-R16-01 登记 | 五门/源码对照 |
| R16/READER-C-06 | P2 | ① | 同 A-06 | 五门/源码对照 |
| R16/READER-C-07 | P2 | ① | §6.1 v0 双打印说明已保留，aarch64 路标说明已在 §5.2.3 分开 | 五门/源码对照
| R16/READER-C-08 | P2 | ① | §3.8 注册计数已澄清（R17 批六） | 五门/源码对照
| R16/READER-C-09 | P2 | ③ | 复核：§2.6.3 与 §4.2 pg_map USER 位差异已分别解释清楚 | 五门/源码对照
| R16/READER-C-10 | P2 | ① | 同 A-05 | 五门/源码对照 |
| R16/READER-C-11 | P2 | ① | §2.2 已补“GCC 默认即 4，风险真实存在”（R15 批） | 五门/源码对照
| R16/READER-C-12 | P2 | ① | aarch64 x0 已标“QEMU 观察到，非规范保证”（R17 批八） | 五门/源码对照
| R16/READER-C-13 | P2 | ① | §2.4.2 标题已标注越界（R16 批） | 五门/源码对照
| R16/READER-C-14 | P2 | ③ | 复核：`createpde` 范围检查足够（`free_pde_idx` 界内 + 槽号界内已是完整防护） | — |
| R16/READER-C-15 | P3 | ③ | 复核：`AlreadyMapped` 守卫推演已改写（R14 批），`paddr` 同名不同物已注 | — |
| R16/READER-C-16 | P3 | ③ | 复核：GRUB 路径 `.bss` 零初值有 §2.4 两条装载证据 | — |
| R16/READER-C-17 | P3 | ③ | 复核：§7.5 `overlaps` 索引覆盖了所有变体 | — |
| R16/READER-C-18 | P2 | ① | §5.4 已改 M1–M17 并点名 M17（R16 批） | 五门/源码对照
| R16/READER-C-19 | P3 | ③ | 复核：四道门/五个门名计数口径已统一 | — |
| R16/READER-C-20 | P3 | ③ | 复核：`user_end` 归类与台账一致 | — |
| R16/READER-A-21 | P3 | ③ | 复核：§5.2.1 计数史已压一行（R17 批） | — |
| R16/READER-A-22 | P3 | ③ | 复核：§3.2 条件编译叙述已收紧（R15 批） | — |
| R16/READER-A-23 | P3 | ③ | 复核：§2.4.1 拆节后指代清晰 | — |
| R16/READER-A-24 | P3 | ③ | 复核：M2 已对齐源码 TODO-05 | — |
| R16/READER-A-25 | P3 | ③ | 复核：2MiB−16 如实标出 | — |
| R16/READER-A-26 | P3 | ③ | 复核：§7.1 smp 描述已改 | — |
| R16/READER-B-21 | P3 | ③ | 复核：§2.4.1 拆节后 align 推演不再自我驳回 | — |
| R16/READER-B-22 | P3 | ③ | §2.4.1 拆 2.4.1.1–4 后取整推演不再中断主线 | 五门/源码对照
| R16/READER-B-23 | P3 | ③ | §1.2 D1–D5 消除编号冲突 | 五门/源码对照
| R16/READER-B-24 | P3 | ③ | §3.10 标题改“非索引”后读者预期已校准 | 五门/源码对照
| R16/READER-B-25 | P3 | ③ | §2.8 证据指针已改 §2.3/§2.5 与 §2.6.1 | 五门/源码对照
| R16/READER-B-26 | P3 | ③ | §2.4.1 拆节后标题承诺已与内容对齐 | 五门/源码对照
| R16/READER-B-27 | P3 | ③ | §3.4 拆 3.4.1–4 后读者不再误入 arena 细节 | 五门/源码对照
| R16/READER-B-28 | P3 | ③ | §1.4 索引已覆盖八套编号 | 五门/源码对照
| R16/READER-B-29 | P3 | ③ | §3.7 前向指针已加 | 五门/源码对照
| R16/READER-B-30 | P3 | ③ | §4.2.1 表头已标执行序 | 五门/源码对照
| R16/RB16-03 | P2 | ① | §2.4.1 右余量段仍不成立→改为存在性结论 | 五门 |
| R16/RB16-04 | P2 | ① | §4.8 “在 3.7 已给出”假指针 | 五门 |
| R16/RB16-05 | P2 | ① | §5.4 部分覆盖清单漏 M12/M14 | 五门 |
| R16/RB16-08 | P1 | ① | U-Boot arena 归属写错（在 bump 区间之后） | 五门/源码对照 |

## R18 实施批（承接 R14/R15/R16 的转出项）

> 这些行不是新一轮面板发现，而是已裁决转出项的实施落账；`处置类` 记 ① 表示"本轮修完"。来源列给原台账行，证据列给可重放命令或测试名。

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| R18/DESIGN-02 | R14/R15/R16 DESIGN-02 | P1 | 设计/假所有权 | ① | `BootStorage` + `BootReport<'a>`；UEFI 与 U-Boot 两腿都改走 storage，内核入口收报告；UBoot 的 arena 与 startup 记录也在 `prepare_boot` 内完成 | `cargo test -p minix-boot` 183；`cargo test -p minix-kernel --features mock` 868；QEMU U-Boot 6/6 | 五门/QEMU 三门 |
| R18/DESIGN-04 | R14/R15/R16 DESIGN-04 | P2 | 设计/双写 | ① | 删除 arena 语义分区；wire v9；arena 8 页；分类同时写 storage 与 wire | `arena_partition_arithmetic_matches_the_design`；`rejects_bad_header_fields`（v8/v10 拒绝） | 五门/QEMU 三门 |
| R18/DESIGN-05 | R14/R15/R16 DESIGN-05 | P2 | 设计/双分配器 | ① | 保留两实例；公共核心 `BumpRegion`；编译期分区断言 + `prepare_boot` 运行期断言 | `cargo test -p minix-boot`（`bump` 5 条）；QEMU U-Boot 6/6 | 五门/QEMU U-Boot |
| R18/DESIGN-08 | R14/R15/R16 DESIGN-08 | P2 | 设计/强制拷贝 | ① | `FileBytes<'a>` 显式 Borrowed/Owned；U-Boot 借出、UEFI 拥有 | `cargo test -p boot-shim --features test-all` 135；`test_uboot_file_loader_reads_bytes_from_table` | 五门/QEMU 三门 |
| R18/D-6 | F-R16-02 | P1 | 代码/兜底 | ① | 删除 memmap 兜底；未注册即 fail-closed；正负控各一条 | `missing_boot_allocator_registration_fails_closed` | 五门/源码对照 |
| R18/D-7 | F-R16-01（负控面） | P2 | 测试缺口 | ① | ELF 源缓冲与目标 LMA 重叠必须在首段写入前停机 | `source_destination_overlap_stops_before_any_segment_write` | 五门 |

## R17 面板轮（2026-11-09，用户指示持续循环；全角色）

> 面板原始输出：`tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round17/`。本轮按用户标准"P0–P3 全部修"处置；设计第二遍对第一遍的否决项记 ③（不改代码，理由在 D2 条目内）。

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01、CLAIM 01 | fact / refute | P0 | 事实/引文 | ① | §3.6 删除不存在的 `HUGE_PAGE_SHIFT`/`PTE_HUGE_IDENTIFIER_BIT`；理由与否决表改写为"标识位留在实现内" | grep 两个常量零命中；trait 实为 5 成员 | `grep -n "HUGE_PAGE_SHIFT\|PTE_HUGE_IDENTIFIER_BIT" 文档` 零命中 |
| FACT-02、CLAIM 02 | fact / refute | P0 | 事实/引文 | ① | §3.8 `BootAlloc::alloc` 换成 `BumpRegion` 实现；反事实的 `fetch_add` 改为 `alloc_pages(1)` | `boot_alloc.rs:118-144` | `rg fetch_add 文档` 零命中 |
| FACT-03 | fact | P0 | 事实/引文 | ① | §3.7 `enter_kernel_image_v1` 引文改 `report` 四标量调用 | `lib.rs` 签名与函数体 | 引文与源码对读 |
| FACT-04 | fact | P1 | 事实/合同 | ① | §4.4 改为"两直核腿共用 builder；三条 v1 腿共用 storage/report 形状" | `build_kernel_info` 调用点 grep | 文档与 §3.4 一致 |
| FACT-05、FACT-06、READER-A-07、READER-B-03、READER-C-03、CLAIM 03 | 多角色 | P0 | 测试计数 | ① | §5.2.1 逐文件表与求和统一为实测 22/2/21/10，补 bump/boot_storage；kernel_info 拆解改 9/3/4/4/1=21 | `cargo test -- --list` 与 `grep -c '#\[test\]'` | 表、求和式、总数三处一致 |
| FACT-07 | fact | P2 | 引文省略 | ① | BootPrepareResult 节选标省略；arena 注释补全；R18 订正块头为"略去 struct 文档注释与 trait 级文档"，`report` 字段中文旁注标为编者加注 | `boot_shim.rs` 对读 | 块头与实际省略一致 |
| FACT-08 | fact | P2 | 机制误述 | ① | §2.4.2 改为"切走区间、左右余量补回；整条目被覆盖才留零长" | `cut_memmap` 源码 | 与源码机制一致 |
| FACT-09 | fact | — | — | ③ | 行号 759/760 正确，省略 758 可接受，不构成缺陷 | 源码 grep | — |
| FACT-10 | fact | P3 | 省略行数 | ① | "约 41 行"改"约 35 行" | 源码段计数 | 文本检查 |
| READER-A-01、READER-B-01 | reader-a/b | P1 | 结构/编号 | ④ | 把 §3.4.5 整块移到 §3.4.4 之后，编号与阅读顺序一致 | 标题顺序 | `rg '^#### 3.4.[345]'` 递增 |
| READER-A-02 | reader-a | P0 | 结构/时序 | ④ | §4.2.1 表加"时间序"列；R18 补：首列改"动作 ID"，动作 2 拆成 1（读/预订）/3（拷贝/模块） | 代码块顺序与伪代码 | 动作 2 的模块加载不再被说成时间序 1 |
| READER-A-03、READER-B-05 | reader-a/b | P2 | 因果/悬空结论 | ④ | §2.2 明确 C 入口不满足 SysV 对齐、风险前提与 Rust 侧对齐/禁 SSE 的对照 | `main.rs` 入口 asm 与 `.cargo/config.toml` | 文档结论含裁决句 |
| READER-A-04、READER-B-04 | reader-a/b | P1 | 因果/编号 | ④ | §1.2 D1 改"第二步 `pg_identity`"并说明依赖是缓存属性输入；补"编号不是执行序，执行序 D2→D1→D3→D4" | §2.6.1 五步表 | 编号说明与 §2.8 一致 |
| READER-A-05 | reader-a | P2 | 前提缺失 | ④ | §2.6.1 U/S 句补 `CR4.SMEP = 0` 前提与 Rust 侧不依赖该前提 | `pg_utils.c` 只清 PGE/PSE | 文本含前提 |
| READER-A-06 | reader-a | P2 | 证据缺失 | ④ | §2.4.1.3 四前提补 `main.c:147/311`、`arch_system.c:246/277-278`、`arch_smp.c:76` 与 236 CPU 阈值 | 源码锚点 | 四前提各有锚点 |
| READER-A-08 | reader-a | P2 | 模型误导 | ④ | §3.1 标明 `exit_boot_services` 是测试辅助，生产走配对循环且不丢弃内存图 | `pair_and_classify` | 文本分工明确 |
| READER-A-09、READER-A-10 | reader-a | P2 | 术语迟到 | ④ | §3.2 首段定义"腿"；trait 代码块后定义 bump | 文档 | 首次使用前有定义 |
| READER-A-11 | reader-a | P3 | 计数负担 | ④ | §3.7 补"每套计数只在本用途内使用，跨节换算单位" | 文档 | 文本含换算说明 |
| READER-A-12 | reader-a | P2 | 自相矛盾 | ④ | §6.1 "两条通道"改"两个函数" | 文档 | 同节不再冲突 |
| READER-A-13 | reader-a | P2 | 计数错误 | ④ | "其余四类"改"其余五类（AlreadyMapped 见下句）" | `PageTableError` 6 变体 | 1+5=6 |
| READER-A-14 | reader-a | P2 | 符号错配 | ④ | §7.5 改"角色对应、非同一对象；数组顶是 `kernel_boot_stack_top`" | §4.4 字段表 | 两处一致 |
| READER-A-15 | reader-a | P3 | 术语未定义 | ④ | §1.4 首段定义"快照"与"恒等窗口" | 文档 | 首用前有定义 |
| READER-A-16 | reader-a | P2 | 指针错节 | ④ | 五处 `§3.1 现状表` 改 `§3.1.1 接线状态表` | 标题实际位置 | `rg '§3.1 的?现状'` 零命中 |
| READER-B-02 | reader-b | P2 | 跨节半账 | ④ | §3.5 给 16+8=24、184−24=160 完整式；§3.9 只回指 | 文档 | 两节不再互指 |
| READER-B-06、READER-C-12 | reader-b/c | P2 | 指针错节 | ④ | §1.4 时序组改指 §3.4.4 探针 | 节内容 | 指针可落 |
| READER-B-07 | reader-b | P2 | 句子被标题截断 | ④ | 引导句移入 §3.4.2 标题之后（R17 冻结面漏修，R18 补修） | `grep -n "被删的 21 个字段\|^#### 3.4.2"`：标题在前 | 冒号后直接接表 |
| READER-B-08 | reader-b | P2 | 分账矛盾 | ④ | §2.8 删除"两条/三条"二分，按证据小节逐条列 | 文档 | 五条均有落点（3 条带小标题，2 条在引导句内带过；R18 回读订正复验措辞） |
| READER-B-09 | reader-b | P2 | 重复 | ④ | §3.9 三前提改为指向 §2.6.3，不复述 | 文档 | 只留 64 位侧三点 |
| READER-B-10 | reader-b | P2 | 边界混淆 | ④ | §2.4.1 开篇说明 2.4.1.4 是清单生命周期的第三件事 | 文档 | 开篇与子节一致 |
| READER-B-11 | reader-b | P2 | 开篇与正文不符 | ④ | §2.6.2 开篇改"alloc_pagetable 管页表页；pg_map 调 2.4.1.1 的 pg_alloc_page" | 文档 | 开篇与正文一致 |
| READER-B-12 | reader-b | P2 | 五步对账缺格 | ④ | §4.6 补 `pg_clear` 并入 `new_from_page` 等五步落点 | 文档/源码 | 五步各有落点 |
| READER-B-13 | reader-b | P3 | 残句 | ④ | 删"不是'两道'" | 文档 | 全文无该残句 |
| READER-B-14 | reader-b | P2 | 标题以偏概全 | ④ | §2.6.3 改"下游之一：借槽窗口" | 标题 | 与 2.6.4"两个下游"一致 |
| READER-B-15 | reader-b | P2 | 未定义事件 | ④ | §3.4.1 图例定义"字段删除"；一处错搭配改"该次删除中" | 文档 | 首用有定义 |
| READER-B-16 | reader-b | P3 | 前置缺失 | ④ | §3.1 采纳行加"库/可执行体区分见 §3.3" | 文档 | 指针存在 |
| READER-B-17 | reader-b | P2 | 三表权威 | ④ | §3.10 加"冲突以 §7.5 为准，与 §4.9 同规则" | 文档 | 两处规则一致 |
| READER-B-18 | reader-b | P3 | 空节 | ④ | §1.3 补"先看符号再看注释"的读法 | 文档 | 节有实质内容 |
| READER-B-19 | reader-b | P3 | 计数承诺 | ④ | §6.2"三条"改"四条"；§1.4"两处"改"三处" | 文档 | 条目数一致 |
| READER-B-20 | reader-b | P3 | 回指无锚 | ④ | 2.4.1.1 加"下标序与地址序的关系"引导；两处回指改用该名 | 文档 | 回指可定位 |
| READER-C-01 | reader-c | P1 | 契约三方矛盾 | ④ | §4.3 UbootFileLoader 引文改 `FileBytes::Borrowed` 签名/返回 | `opensbi_helpers.rs` | 三处一致 |
| READER-C-02 | reader-c | P1 | 算术缺口 | ④ | arena 注释补"v8 因此用九页 36864 B" | 文档 | 30624+6144≤36864 |
| READER-C-04 | reader-c | P1 | 计数错误 | ④ | "三个地址"改"四个地址（root/bump_base/bump_end/arena_pa）" | 结构体字段 | 四字段 |
| READER-C-05 | reader-c | P1 | 同行矛盾 | ④ | M16 末句改"UEFI 之外三条腿无同形运行期拒绝，由编译期断言/装载前 gate 覆盖" | 源码 | 同行无矛盾 |
| READER-C-06 | reader-c | P1 | 完备口径 | ④ | M13 加"对可分配/保留两档；Excluded/DevicePath/Side 不进表" | §4.2.2 | 两处一致 |
| READER-C-07 | reader-c | P1 | 理由缺限定 | ④ | 扣除表模块行标注 UEFI 为 no-op、理由只适用于 blob 在 free list 的腿 | §3.4.1 | 两处一致 |
| READER-C-08 | reader-c | P1 | 索引错配 | ④ | §1.4 五步消费者改 §4.2.2 / §5.2.1（ebs_map 44 条） | 文档 | 指针可落 |
| READER-C-09 | reader-c | P1 | 层级措辞 | ④ | "最外层壳"改"各架构最靠近泛型体的委托壳"，并写共享入口另钉 | §4.8 调用链 | 与调用链一致 |
| READER-C-10 | reader-c | P2 | 全称缺项 | ④ | §3.5 "两类执行形态"改"当前两条生产形态；内联已退役" | 文档 | 全称有范围 |
| READER-C-11 | reader-c | P2 | 反事实落点 | ④ | §4.1 改"挪到 `enter_kernel_image_v1` 跳转后（接收侧 `arch_boot_resume_v1`）" | 入口分工 | 与 §3.7 一致 |
| READER-C-13 | reader-c | P2 | 条件缺失 | ④ | §2.3 改"arch 无读者、board 可被同名键遮蔽" | 文档后文 | 与后文一致 |
| READER-C-14 | reader-c | P3 | 索引错误 | ④ | 改"cmp_mod=0、内层 m=1 时 panic" | `overlaps` 代码 | 与循环一致 |
| READER-C-15 | reader-c | P2 | 容量口径 | ④ | §3.4.5 补 source 40/startup 20 槽及 UEFI source 写 wire 的说明 | `boot_storage.rs` | 五类+两类齐全 |
| CLAIM 04 | refute | — | 反证失败 | ③ | `alloc_lowest` 四前提实测成立，主张保留；量化阈值补入 READER-A-06 修复 | 源码 | 反证失败=主张成立 |
| CLAIM 05 | refute | P2 | 事实过度断言 | ① | `kern_stack_top` 行区分跨镜像清零与直核面未清零，安全性不依赖清零 | `main.rs` 三个 `_start` | 三腿口径分别写 |
| CLAIM 06 | refute | P2 | 可达停机点缺失 | ① | §2.4.1.4 与 §3.4 补 `VM_BOOT_HANDOFF_MAX_REGIONS=64` 与停机行为 | `vm_handoff.rs:466` | 两级上限写明 |
| DESIGN-D2-01、DESIGN-D2-02、DESIGN-D2-03 | design-d2 | — | 设计前提 | ③ | 第一遍的 40 槽可变账/AllocAuthority/页目录 typestate 在实现中没有需求方，拒绝采纳 | D2 条目证据 | 不改代码 |
| DESIGN-D2-04 | design-d2 | P2 | 并发 UB | ① | `pt_alloc::register/register_free` 用 `PT_CLAIMED` swap 抢写权后再写指针、最后发布 | `cargo test -p minix-arch` 287 通过 | 两个写者只一个赢 |
| DESIGN-D2-05 | design-d2 | P2 | 半提交状态 | ① | 新增 `commit_map_tables(len,len)` 一次提交；两处生产调用点改用；R18 补：旧 setter 收进 `#[cfg(test)]`（消死代码）、`commit_map_tables` 加单测、§4.2 引文与 §3.4.5 提交协议同步 | `boot-shim` 135 通过；`minix-boot` 186 通过；`cargo check -p minix-boot` 无 dead_code | 两表同轮；生产面只留 used |
| DESIGN-D2-06 | design-d2 | P2 | 构造中合法值 | ① | `build_kernel_info` 收 `memmap_sources`/`startup_objects`；两直核腿先算后传，删除事后赋值（文档同步在 R18 批次补） | riscv/aarch64 `cargo check` | 无事后补写 |
| DESIGN-D2-07 | design-d2 | P2 | 容量三处 | ① | `globals.rs` 加四条跨 crate 编译期等式；`FREE_SET_CAP` 引用 store 常量 | `minix-kernel` 860 通过 | 常量漂移编译失败 |
| DESIGN-D2-08 | design-d2 | P2 | 双枚举器 | ① | 抽 `for_each_live_bootstrap`，`classify` 与 `vm_boot_exclusions` 共用（文档引文在 R18 批次补） | `minix-kernel` 860 通过 | 成员单源 |
| DESIGN-D2-09 | design-d2 | P2 | 假 'static | ① | `OwnedDtb` 改 raw pointer + `bytes()`；`dtb_copy` 与 `dumpdtb` 两处 `Box::leak` 移除；`capture` 收 `&[u8]`（代码在 R18 批次提交，R17 冻结面未含） | `grep Box::leak` 两文件零命中 | 两处成立条件消失 |
| DESIGN-D2-10 | design-d2 | P3 | 死 API | ① | `push_module` 收进 `#[cfg(test)]`；`push_platform_source`/`push_startup` 有生产调用者故保留 | grep 调用点 | 生产面只留 used |
| DESIGN-D2-11 | design-d2 | P2 | 静默参数 | ① | U-Boot 腿打印 rejected/dropped；两直核腿补 rejected/dropped 十六进制读数 | 三处代码 | 三腿均出声 |
| DESIGN-D2-12 | design-d2 | P2 | 单实现 trait | ② | 文档已按事实修正；trait 去留 F-R17-01（截止 R19，待用户裁决） | `boot_shim.rs` | 文档与 impl 数一致；trait 本体保留 |
| DESIGN-D2-13 | design-d2 | P2 | 死代码/测试专用 | ② | `classified_descriptors` 已删；测试三件套 feature 门 F-R17-02（截止 R19） | grep 零调用 | 生产面已无零调用函数；三件套仍在该批 |
| DESIGN-D2-14 | design-d2 | — | 反向证据 | ③ | 运行期 enum 是跨边界状态的正确形态，不改 | D2 条目 | — |
| DESIGN-D2-15、DESIGN-D2-16、DESIGN-D2-19、DESIGN-D2-20 | design-d2 | — | D1 表述 | ③ | 第一遍方案的垫片/Paging/测试/恒等范围表述与实现相反；D2 结论保留实现 | D2 条目 | 不改代码 |
| DESIGN-D2-17 | design-d2 | — | 容量依据 | ① | `WIRE_MEMMAP_CAP` 注记 128 的推导与实测 12–13；保留 128 | `handoff_v1.rs` | 注释含依据 |
| DESIGN-D2-18 | design-d2 | P2 | 权威错配 | ② | 给 `KernelInfo` 加 authority 字段属契约变更（13→14），登记 F-R17-04 待裁决 | D2 条目 | 台账有去向 |

### R17 独立回读（RB17）与补修

`panel/round17-rb.md` 判 NOT PASS（P0=0、P1=0；原因是半修记整修、冻结面与台账不一致、引文未同步）。逐条处置：

| RB17 项 | 严重度 | 处置 | 证据 |
|---|---|---|---|
| H1 / B-07 引导句仍在标题之前 | P2 | ① R18 已把标题移到引导段之前（`#### 3.4.2` 先于"被删的 21 个字段…"） | `grep -n "被删的 21 个字段\|^#### 3.4.2"`：标题行号更小 |
| N1 §4.2 两次 setter 引文 | P2 | ① 改 `storage.commit_map_tables(memmap_len, reserved_len)`；§3.4.5 提交协议补一句 | 与 `uefi_helpers.rs:1104` 同形 |
| N2 §3.8 重复 `PT_REGISTERED.store` | P2 | ① 删重复行，保留 Release 注释后的一条 | 引文与 `pt_alloc.rs:100-116` 同序 |
| N3 "静态切片" | P2 | ① 改 `Option<Vec<u8>>`（自有缓冲） | 与 `load_dumpdtb_blob` 返回类型一致 |
| N4 缺 `.as_deref()` | P3 | ① 引文补 `.as_deref()` | 与 `uefi_helpers.rs:1112` 同形 |
| N5 `memmap_slots_mut` 文档指向死 setter | P3 | ① 改指 `commit_map_tables` | 文档与 `boot_storage.rs` 一致 |
| N6 setter 死代码 + `commit_map_tables` 零单测 | P2 | ① setter 收进 `#[cfg(test)]`；新增 `commit_map_tables_commits_both_prefixes_together`；`cargo check -p minix-boot` 无 dead_code | `minix-boot` 186 通过 |
| N7 / H2 / H3 / H4 / H5 冻结面不含 D2-09/05 降级/06·08 文档同步 | P2 | ② 冻结面与台账的差异如实记入本节；这些补修随 R18 提交，BASELINE 在 R18 重新冻结（R17 的 `fix_head` 不再声称包含它们） | `git show 7a36a9253` 与工作树对读 |
| FACT-07 `report` 字段中文旁注未标 | P3 | ① 块头补"`report` 字段后的中文旁注为编者加注" | 文本检查 |
| A-02 时间序列不单调 | P3 | ① 动作 2 的时间序改为"1（读/预订）/ 3（拷贝/模块）" | 表与伪代码一致 |
| B-08 复验语夸大 | P3 | ③ 台账复验语已订正为"3 条带小标题，2 条在引导句内带过" | 本节上表 |
| D2-08 `#[cfg(test)] classify_regions` 仍手写成员序列 | P3 | ② 测试专用副本不影响生产单源；转 R19 观察，不阻塞 | 生产两处均走 `for_each_live_bootstrap` |
| D2-04 无并发双写者测试 | P3 | ② 现有两条串行注册测试覆盖契约；并发压测成本高于收益，登记 R19 观察 | `pt_alloc_write_once` 2 通过 |

## R18 面板轮（2026-11-09，用户指示持续循环；全角色 + DESIGN 两遍）

> 面板原始输出：`tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round18/`（fact、reader-a/b/c、refute、design-d1、design-d2）。D1 是独立设计方案，不计文档缺陷；D2 对 D1 的否决按 ③ 记录，其中 D2-10 指出的接收侧空白按 ① 实施。数值口径见 `metrics.md`。

| 发现 ID | 来源 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | fact | P0 | 引文/签名 | ① | §4.4 `build_kernel_info` 引文补 `memmap_sources`/`startup_objects` 两参，函数体改为原样转发；删“builder 置空” | `os/boot-shim/src/lib.rs:101` 十参 | 引文与源码对读 |
| FACT-02 | fact | P0 | 引文/返回类型 | ① | §3.2 UEFI `FileLoader::read` 改 `Option<FileBytes<'a>>` 与 `.map(FileBytes::Owned)` | `uefi_helpers.rs:1144-1155` | 引文逐字 |
| FACT-03 | fact | P0 | 引文/并发机制 | ① | §3.8/§4.7 改 `PT_CLAIMED.swap(AcqRel)` 抢写权 → 写指针 → 发布；删“先读 PT_REGISTERED” | `pt_alloc.rs:100-116` | 引文与源码同序 |
| FACT-04 | fact | P1 | 引文归属 | ① | §3.8 扣除段改引 `for_each_live_bootstrap(...)` 与 `boot_bump` 分支 | `vm_handoff.rs:415-440` | 引文可命中 |
| FACT-05 | fact | P1 | 诊断路标 | ① | §6.1 两条路标前缀 `kernel-image:` → `kernel:`（保留另一条真为 `kernel-image:` 的） | `lib.rs:888/1130` | grep 命中 |
| FACT-06 | fact | P1 | 作用域 | ① | §2.2 SSE 句拆成内核目标默认 + shim 目标显式禁 SSE 两条 | `.cargo/config.toml:38-46`、`xtask/image.rs:53` | `rustc --print cfg` |
| FACT-07 | fact | P2 | 锚点 | ① | §2.4.1.3 `arch_system.c:246` 改 `:277-278` | 源码 grep | 锚点可解析 |
| FACT-08、FACT-09 | fact | P2 | 引文注释 | ③ | 两处块头已分别声明“中文注释为译述”“行内注释有一处截断”，非缺陷 | 引文头原文 | 文本检查 |
| FACT-10 | fact | P2 | 元注释 | ① | §3.2 块头改“略去 struct 文档注释与 trait 级文档；`report` 字段中文旁注为编者加注” | `boot_shim.rs:17-66` | 块头与实际一致 |
| FACT-11 | fact | P2 | 引文改写低报 | ① | §3.8 扣除表 `kern_size()` 改回字段名；块头补模块行限定说明 | `vm_handoff.rs:17-40` | 逐字对读 |
| FACT-12 | fact | P2 | 反事实前提 | ① | §2.4.1.4 改“归并后仍两两不相接且超过 64 条”；补相接区间会被合并 | `vm_handoff.rs:263-307/722-760` | 条件完整 |
| FACT-13 | fact | P2 | 编号归属 | ③ | §5.1 M2 已写“R16 DESIGN-10，R17 实施批落码”，与源码注释一致 | `kernel_info.rs` 注释 | 无冲突 |
| FACT-14 | fact | P2 | 常量归属 | ① | §4.1 已写“定义在 `lib.rs`，`main.rs` 传常量”，与 `BOOT_PT_BUMP_PAGES` 一致 | `lib.rs:250` | 归属正确 |
| FACT-15 | fact | P3 | 复数/单数 | ① | §3.1 注改“`build_memmap()` 返回的 conventional 列表” | `uefi_helpers.rs:486` | 与引文一致 |
| FACT-16 | fact | P3 | 行号 | ① | §2.6.3 引 `do_vmctl.c:164` | 源码 grep | 锚点正确 |
| FACT-17 | fact | P3 | 措辞边界 | ① | §3.4.1 改“启动期之后没有读者” | 文本 | 与 §4.4 门链一致 |
| FACT-18 | fact | P3 | 符号/字符串 | ① | §3.4.1 改“已没有符号定义（仅 `bootface_a64.rs` 一行注释提及该字符串）” | `grep FREE_MEMMAP os` | 阴性声明精确 |
| FACT-19 | fact | P3 | 未验证前提 | ③ | `CONFIG_MAX_CPUS ≤ 236` 已给公式；跳板体积未实测属文档已声明的边界，不改 | §2.4.1.3 | — |
| FACT-20 | fact | — | 已核实无问题 | ③ | `load_stack_start` 4096 对齐链已逐项核对通过，无需改 | §2.2 | — |
| FACT-21 | fact | P3 | 前提缺失 | ① | §3.4.1 补“前提是固件给该段置 `WRITE_BACK`；否则落 `Side`” | `ebs_map.rs` Step 5 | 与 §4.2.2 一致 |
| READER-A-01..20 | reader-a | P1/P2/P3 | 读者障碍 | ④ | §1.2 D1 拆句；编号→执行序小表；§1.4 补 arena/切根定义与四栈名表；§2.6.3 补 `pde = freepdes[free_pde_idx]`；§2.4.1.1 改三态表；§4.2.2 五例改编号列表并定义 `UNKNOWN_FOR_TESTING`；§4.2.1 首列改动作 ID；§2.7 贴 ARM `overlaps` 宏与循环；§2.2 降级 SSE 断言；§3.4.4 标“无归档日志”；§1.1 改双重否定；§1.1 改 Multiboot 分页表述；§3.1 加“不是同一 trait 两实现”注；§1.4 索引补六套计法；§2.4.1.3 补 236 公式；删批次号；§1.1 拆三句；§3.1 改“两个内存分配接口” | 各条锚点在 panel 文件内 | 六门 + 原句 grep |
| READER-B-01..14、READER-B-16..20 | reader-b | P2/P3 | 结构/权威 | ④ | §7.5 `overlaps` 改三处落点；§1.4 索引改六套；§2.8 树补 ELF/识别头分支与粗档分支；§3.4.3 改“接收侧构造、发送侧只构造 BootReport”；§3.4.2 指针改 §3.7；§3.4.4 删同节重复“预防性不变量”段；§2.4.2 改“本节两处 + 对照一处”；§2.4.1.2/.3 标题对齐；§2.4.1.1 删重复结论；§2.5/§2.6.1 的链接符号引用统一；§4.3 开篇改“没有编译期检查”；§1.4 rootdevname 边界放宽；卷首改“第 1 段/第 4 段”；§6.1 `build_memmap(s)` 口径；§3.8 定义 `VmBootAllocator`；§5.1 补 TODO-08/09；§3.3 标题限定 `minix-kernel`；§3.1.1 改“第一张表终态、第二张表否决记录”；§3.6 四条理由编号；§2.7 宏块 | 各条锚点在 panel 文件内 | 六门 + 原句 grep |
| READER-B-15 | reader-b | P3 | 单复数 | ③ | 源码同时存在 `build_memmaps`（诊断、双表）与 `build_memmap`（单表包装）；§6.1 已分别写明，非缺陷 | `uefi_helpers.rs:384/486` | 两名各有定义 |
| READER-C-01..18 | reader-c | P1/P2/P3 | 跨节因果 | ④ | 图注改“建表第二步”；arena v8 账改四项 36768 B/九页；§4.8 改“两种进入方式（第三种已删除）”；“唯一改硬件状态”改“唯一改 CR0/CR4 模式位”；§5.4 改“未登记 TODO”；M6 明说全量走查未登记；§3.5 改“所有生产链接布局 + VMA=LMA 测试内核”；§1.4 rootdevname 边界；§4.2.1 动作 2 时间序拆 1/3；§4.6 缓存范围改“上界所在 4MB 块及其以上”；§6.3 改“prepare_boot 于装载/建根之前（该路径无 EBS）”；§3.5 删除论证回指本节；§3.4.5 补 40→296 规则；§4.6 “没有访问者”降为显式前提；§3.4.1 改“启动期之后没有读者”；§3.4.2 补 `cttyline`/`cttybaud` 处置；`[ARCH]` 第三条改“C 写但不保证生效” | 各条锚点在 panel 文件内 | 六门 + 原句 grep |
| READER-C-10 | reader-c | P2 | 历史账不可闭合 | ③ | 该箭头串已在 R17 收口时删除；现只保留逐文件表 + 最终总数 + 实跑读数 | §5.2.1 | 无中间箭头 |
| REFUTE CLAIM 01、02 | refute | — | 反证失败 | ③ | 主张成立，保留 §4.6 Step 1 错误集与 additive 接收侧叙述 | panel 内证据 | — |
| REFUTE CLAIM 03 | refute | P1 | C 读者计数 | ① | §3.4.2 GET_KINFO 改“C 侧至少四个活读取点、VM 读 ≥11 字段” | `do_getinfo.c`/`vm` 源码 | 与 grep 一致 |
| REFUTE CLAIM 04 | refute | P1 | 重复键分支 | ① | §3.4.2 改“首条记录重复时 C 后写胜出；非首条时账上两条、读首条”，并标为与 C 相反的新分歧 | `pre_init.c` 源码推演 | 两分支分开写 |
| REFUTE CLAIM 05 | refute | P0 | 测试虚构 | ① | `ebs_boundary_audit.rs` 补 `allocate_pages` 注入负控与注释豁免自检，共 4 条；M12/TODO-04 口径改为“两枚注入负控 + 注释豁免自检” | `cargo test --test ebs_boundary_audit` 4 通过 | 负控可独立触发 |
| REFUTE CLAIM 06 | refute | P1 | 映射档 | ① | §4.6 改“三条生产链当前只能选 2 MiB 回档；1 GiB 是理论首选档，未跑（TODO-05）”；M5 同步 | `boot_policy.rs:90-133`、三份 ld | 与 TODO-05 一致 |
| DESIGN-D2-01、03、06、07、12 | design-d2 | — | D1 方案前提 | ③ | D1 的最小恒等窗、可变内存账、`Option` 空槽、摘要第 4 条与现仓裁决/C 行为冲突；D2 判拒，本仓保持既有设计 | D2 条目内 C/源码行号 | 不改 |
| DESIGN-D2-02、05、09、11 | design-d2 | — | D1 方案分歧 | ③ | 可变账 vs 不可变 memmap、`一律停机` vs EBS 重试、四态 typestate vs `KernelMapPolicy`、二选一框架缺直核形态——均按现仓裁决与文档事实拒绝 D1 方案 | D2 条目 | 不改 |
| DESIGN-D2-04、08、13 | design-d2 | — | D1 载荷/锚点 | ③ | D1 载荷缺 bump 三件套/多出无读者字段/锚点错位；现仓 v1 信封已含 bump 三件套、无多余偏移字段，文档锚点另行核对 | D2 条目与 `handoff_v1.rs` | 不改 |
| DESIGN-D2-10 | design-d2 | P2 | 接收侧空白 | ① | `KernelInfo::validate` 增加内核镜像×模块、模块×模块半开区间复核（相切放行），补两条负控；§5.1 M2 同步 | `kernel_info.rs` validate | `minix-boot` 186 通过；两条新测试 |

### R18 独立回读（RB18）与补修

`panel/round18-rb.md` 判 NOT PASS（P0=0、P1=1、P2=1、P3=3）。21 条 ① 中 20 条成立条件已消失、RB17 的 12 项补修全部成立；阻断项是本轮新增测试后 §5.2.1 逐文件转录漏改，以及 §3.4.2 新补方位词与源码相反。逐条：

| RB18 项 | 严重度 | 处置 | 证据 |
|---|---|---|---|
| R18-P1-01 §5.2.1 `boot_storage.rs` 仍写 4 | P1 | ① 改 5；块内求和恢复 186 | `grep -c '#\[test\]'` = 5；`cargo test -p minix-boot` 186 |
| R18-P2-01 “新值追加到缓冲开头” | P2 | ① 改“缓冲末尾”，与 `mb_set_param` 的双 NUL 写点及 §2.3 例子一致 | `pre_init.c:61-63` |
| R18-P3-01 §3.8 `// Release:` 注释在 `store` 之后 | P3 | ① 移回 `store` 之前，与 `pt_alloc.rs:111-113` 同序 | 引文逐字 |
| R18-P3-02 REFUTE-03 措辞 | P3 | ① 文档改“C 侧至少四个活读取点”，与台账一致 | §3.4.2 |
| R18-P3-03 M12 未写“两枚注入负控” | P3 | ① M12 补“两枚注入负控”，与 TODO-04/台账一致 | §5.1 M12 |

## R19 面板轮（2026-11-09，R18 收口后；全角色 + DESIGN 两遍）

> 面板输出：`tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round19/`。D1 的 5 条决策是方案，不计入发现；D2 的 12 条发现里 2 条落码、10 条按 D1 方案否决记 ③。总计 80 条：FACT 6（2 主条 + 4 次级）、READER-A 20、READER-B 20、READER-C 15、REFUTE 7、DESIGN-D2 12。

| 发现 ID | 来源 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | fact | P1 | 取槽算术 | ① | §2.6.3 正常路径 `p_cr3_v[961+k-1]` 改 `961+k`（`pg_mapkernel` 返回第一个空闲槽） | `pg_utils.c:186-202`、`memory.c:707-718` | 与 §1115 反事实分开 |
| FACT-02 | fact | P2 | 路标前缀 | ① | §6.1 第二条路标补 `kernel-image:` 前缀 | `lib.rs:396` | grep 命中 |
| FACT-03、FACT-04、FACT-05、FACT-06 | fact | P3 | 引文口径 | ① | §4.4 引文头声明路径限定符略去；§3.3 删“完整依赖表”并注 `bitflags` 从略；§4.2 引文头标两处未展开段；§2.6.4 行号改 360–363 | 源码对读 | 块头与源码一致 |
| READER-A-01..20 | reader-a | P1/P2/P3 | 读者障碍 | ④ | §2.6.3 槽号；§1.4 六套计数补“架构×链五行”；§2.8 明列三条论证 + 两条指向；§1.1 表限定 C/x86-32；§3.4.5 标题去批次号、图例补 R14–R18/A+D/裁决 A；§2.4.1.2 只留结论指针；§6.4 GDB 指 `os/kernel-image/Cargo.toml`；§7.5 `load_stack` 拆“无同名对应物”；wire 版本/别名；§7.4 补 `test-cmd-smoke.sh`；`module_init` 关系；§6.1 限定 UEFI；ARM 寄存器括注；§4.5 截断口径；§1.4 MapKey/快照序号；依赖 A/B 映射；§1.3 “五条依赖”；6b；三次注册拆解 | 各条锚点在 panel 内 | 六门 + 原句 grep |
| READER-B-01 | reader-b | P1 | 取槽算术 | ① | 同 FACT-01，§2.6.3 改 `961+k` | 文档内 §951-953/§1115 | 两处不再冲突 |
| READER-B-02 | reader-b | P2 | 依赖父子错挂 | ④ | §1.2 改“D5 是建表动作内部”，执行序写 `D2→D1→（D5）→D3→D4` | §2.8 同序 | 两节一致 |
| READER-B-03 | reader-b | P2 | 计数索引 | ④ | 同 A-02：补第六套与消费者 | §1.4/§3.7 | 六套可对 |
| READER-B-04 | reader-b | P2 | 腿数/构造入口 | ④ | §3.4.1 改“三条 v1 发送腿”与“两个构造入口” | §3.7/§4.4 | 与权威表一致 |
| READER-B-05 | reader-b | P2 | 分配器注册歧义 | ④ | §4.4 门链拆“boot bump 区间非 None 即停机”与“`pt_alloc` 由守卫注册” | `lib.rs` `boot_validate_and_prepare` | 与 §3.8/§4.1 一致 |
| READER-B-06 | reader-b | P2 | 方向词 | ④ | §3.5 “上文”改“下文” | 表在句后 | 方向正确 |
| READER-B-07 | reader-b | P2 | 输入/输出约束反了 | ④ | §5.1 M2 改 `add_memmap` 的完整取整/丢弃/截断行为 | §2.4 代码 | 与 §2.4 一致 |
| READER-B-08、READER-B-09 | reader-b | P3 | 死索引/缩写 | ④ | 图例删死项；补 A+D 与裁决 A 定义 | 全文 grep | 回查可落 |
| READER-B-10 | reader-b | P2 | 边界漏项 | ④ | §1.4 名单补 §5.2.3 `entering scheduler` | §5.2.3 | 边界自洽 |
| READER-B-11 | reader-b | P2 | 总说与分腿相反 | ④ | §3.4.1 改“不保证已扣除全部占用区” | 同段分腿细节 | 三处一致 |
| READER-B-12 | reader-b | P3 | TODO 一号两用 | ③ | M6 已改“TODO-05 只登记 1 GiB 首选档；全量走查未登记”，不再复用 | §5.1/§5.3 | 无复用 |
| READER-B-13 | reader-b | P3 | 落点串联误导 | ④ | §4.9 行加“`build_kernel_info` 仅直核” | §4.4 | 两组入口分清 |
| READER-B-14 | reader-b | P3 | 边界未覆盖 §3.4.2 | ④ | 同 B-10 的 §1.4 名单 | §3.4.2 | 已列 |
| READER-B-15、READER-B-16 | reader-b | P3 | 措辞/指针 | ④ | “否决记录”改“裁决记录（三否一采）”；“切换记录”改“切换依据…收口记录/实现现状” | §3.1/§3.1.1 | 指针可落 |
| READER-B-17 | reader-b | P3 | 三处维护容量 | ④ | §2.4.1.4 只留指针，删 128/64 数字 | §3.4.1/§4.5 | 单一权威 |
| READER-B-18 | reader-b | P3 | 状态口子 | ④ | §4.8 改“已实现（接线状态见 §3.1.1）” | §3.1.1 | 状态单源 |
| READER-B-19、READER-B-20 | reader-b | P3 | 承诺过强/消费者错 | ④ | §7.5 改“不保证穷举”；九步消费者改 §4.2.1 | 文档内 | 索引可复核 |
| READER-C-01..15 | reader-c | P1/P2/P3 | 跨节因果 | ④ | 槽号；§4.1 发送侧不构造 KernelInfo；M6/TODO-05；§2.4.1.3 缺陷分支；`arch=`→`board=`；六套计数；三条 v1 腿；§3.10 指针改 §2.4.2；§1.2 cut 动作指 §2.5；§3.3 grep→sed；两种“覆盖”分开；§1.4 GET_KINFO 指 §3.4.2；checked_shape 范围；§2.1 参数语义限定；门表十二行 | 各条锚点在 panel 内 | 六门 + 原句 grep |
| REFUTE CLAIM 01 | refute | P1 | GET_KINFO 字段依据 | ① | §3.4.2 改“五个有读者 + 两个 C 布局位（freepde_start 恒 0、vir_kern_start 零读者）” | 仓库 grep | `[ARCH]` 与 §3.9 一致 |
| REFUTE CLAIM 02 | refute | P1 | 参数分歧计数 | ① | §3.4.2 改五条（补容量丢弃与键大小写），删“截断”措辞；`boot_params` 测试补“后续记录也被拒”断言 | `boot_params.rs:276-288`、C 逐字转录程序 | 五条与测试同源 |
| REFUTE CLAIM 03、04、07 | refute | — | 反证失败 | ③ | 删除论证、1 GiB/2 MiB 档、覆盖账均成立，保留 | panel 内证据 | — |
| REFUTE CLAIM 05 | refute | P2 | 跨架构普遍化 | ① | §4.6 改“kernel_identity_mmio_regions 不是 Step 1 窗口清单；x86 分支为空” | `os/arch/src/lib.rs:255`、`os/kernel/src/lib.rs:914-928` | 与同节上文一致 |
| REFUTE CLAIM 06 | refute | P2 | 调用者枚举错 | ① | §4.7 改“跨镜像腿由 boot-shim；两个直核面在 bootface 初始化并注册” | `bootface.rs:272`、`bootface_a64.rs:256` | 五腿齐全 |
| DESIGN-D2-01..10 | design-d2 | — | D1 方案否决 | ③ | D1 把 C 形态当现仓/另立表/校验和/最小窗上界/类型状态跨产物等，按现仓裁决与文档事实拒绝；不新增代码 | D2 条目内源码行号 | 不改 |
| DESIGN-D2-11 | design-d2 | P1 | release 回绕 | ① | `KernelInfo::validate` 的 kernel/module 跨度改 `checked_add`，新增两条溢出负控 | `kernel_info.rs` 新增测试 | `minix-boot` 188 通过 |
| DESIGN-D2-12 | design-d2 | P2 | 门表未同步 | ① | §4.4 门链/表补接收侧跨度复核行，写明 `checked_add` 与信任边界 | §4.4 | 文档与代码一致 |

### R19 独立回读（RB19）与补修

`panel/round19-rb.md` 判 NOT PASS（13 条 ① 中 12 条成立，唯一阻断是 FACT-06 行号改错；另三条 P2/P3 备注）。逐条：

| RB19 项 | 严重度 | 处置 | 证据 |
|---|---|---|---|
| FACT-06 `protect.c` 区间 | P2 | ① 改 `360-363`（358 是注释收尾、359 空行；四步在 360–363） | `nl -ba protect.c` 逐行 |
| `boot_params.rs` 注释仍写 three | P3 | ① 改 five observable divergences | 源码注释 |
| `uboot_main.rs` 注释写 `KernelInfo` | P3 | ① 改 `BootReport`（该腿是 v1 发送腿） | 源码注释 |
| §4.9 “截断规则”未随五条同步 | P3 | ① 改“整条丢弃规则（五条可观察分歧见 §3.4.2）” | §4.9 |
| ledger READER-B-13 声称“§4.9 加仅直核”无文本 | P3 | ① 已补“（仅直核面）”，台账与文本一致 | §4.9 行 |
| ledger FACT-02 证据行号 | P3 | ③ 复核 `grep -rn "DM ok, jumping to kmain"` 只命中 `os/kernel/src/lib.rs:396`；`main.rs:387` 不成立，保留原证据 | 全仓 grep |
| `pe = pb + prev.len` 裸加 | P3 | ① 同改 `checked_add`，与 kernel span 一致 | `kernel_info.rs` |

## R20 面板轮（2026-11-09，R19 meta PASS 后；全角色，D1 轻量）

> 面板输出：`panel/round20/`（fact 3、reader-a 20、reader-b 20、reader-c 8、refute 6、design-d2 7；design-d1 为轻量方案，不计发现）。总计 64 条。

| 发现 ID | 来源 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-01 | fact | P2 | 关联 C 清单完备性 | ① | §7.2 加“只列主干文件；逐段小文件见正文脚注”，列出 14 组缺失路径类别 | `grep -o minix3/...` 集合差 | 清单口径自洽 |
| FACT-02、FACT-03 | fact | P3 | 引文注释口径 | ① | §3.8 块头标“SAFETY 注释有节略”；§3.2 标“一处英文注释译为中文” | 源码对读 | 块头与内容一致 |
| READER-A-01..20 | reader-a | P1/P2/P3 | 读者障碍 | ④ | §1.2 四坐标加读序说明；§1.1 补 VM/PM/RS/DTB/SBI/OVMF/AAVMF/MTRR/LSR 缩写；§1.4 索引加“成员见 §3.7”与五栈名（补 `load_stack_start`）；§2.4.1.1 表第三行改“通常不断开”；§4.1 列名改“非 UEFI 路径”；boot-shim 首用加定义；§3.4.1 memmap 长单元格未拆（③）；§2.3 补首条记录条件；§2.4.1.3 补“缺口大于还回量”；E820 页对齐改“通常”；§5.2.1 补 868 命令与实测；§4.2.1 说明动作 ID 非时间序；门表改“五个门标签 + 规划期一行”；§4.4 补“2MiB−16”来源；卷首改论证段定位；三基址标架构；17–40 标 1-based；§1.3 删重复执行序 | 各条锚点在 panel 内 | 六门 + 原句 grep |
| READER-B-01..20 | reader-b | P2/P3 | 结构/权威 | ④ | §3.10/§4 导航/§4.4 标题改“两个构造入口”；§4.3 补接收侧模块复核；§3.10 来源句改“来源记录进 KernelInfo/source 分区”；§3.1 U-Boot 寄存器改 `a0`；§2.4.1.3 取整口径拆开；§1.4 五栈名；§7.5 `k_initial_stktop` 指 §2.8；§4.2.1 动作 8 改“先构造报告再复核”；§3.4 通道补 U-Boot shim；§4.6 Step 1 函数归属改“riscv 经 identity_device_windows 使用”；§3.8 注册表补“shim 间接注册”；§4.3/§7.5 overlaps 三处口径；§5.4 改 M3/M6 子项；§1.4 门链成员指 §5.2.3；§4.9/§3.4 重复与反事实分离（③）；表行 2 改“行 1 子情形”；§3.4.3 `x0` 限定 aarch64 | 各条锚点在 panel 内 | 六门 + 原句 grep |
| READER-C-01..08 | reader-c | P1/P2 | 跨节因果 | ④ | §2.6.4 补“TLB 窗口 + 回填”区分；§2.7 改“入口/参数来源唯一按架构另写”；§3.4 拆“深拷要求一致 / v1 解码 vs 直核 builder”；§4.6 补 `identity_device_windows` 包装关系；§2.4.1.4 指针改 128/64 权威 + 128 落地区；§3.5 正面判据改“无可单独界定可归还段”；§3.7 补两个“5”成员不同；§3.4.2 内部键检查标大小写不敏感 | 各条锚点在 panel 内 | 六门 + 原句 grep |
| REFUTE CLAIM 01 | refute | P1 | 参数分歧计数 | ① | §3.4.2 改八条，补分隔符/非 ASCII/空键；`[ARCH]` 标签同步 | `boot_params.rs` 测试、C 复算 | 八条与源码/测试一致 |
| REFUTE CLAIM 02 | refute | P2 | 删除扫描模型漏支 | ① | §2.3 补“前缀匹配不要求在记录起点，后缀可致伪记录” | C 扫描模型 | 两分支都写明 |
| REFUTE CLAIM 03 | refute | P2 | 字段分类错 | ① | §3.4.2 `user_end` 移出“上浮为常量”，release/version 改“构建身份” | 字段表 | 分类自洽 |
| REFUTE CLAIM 04 | refute | P2 | 1 GiB 落选原因 | ① | §4.6 补 `kern_size % 1GiB == 0` 条件，说明生产跨度约 4 MiB | `boot_policy.rs` | 条件完整 |
| REFUTE CLAIM 05 | refute | P3 | 预算单腿普遍化 | ① | §4.1 补“两个直核面 `PT_BUMP_PAGES = 256`” | `bootface.rs:135`、`bootface_a64.rs:99` | 三腿分开 |
| REFUTE CLAIM 06 | refute | P3 | M6 证据说大 | ① | §5.1 M6 改“合成窗口/1 GiB 块配置下验证策略函数，不跑生产 Step 1” | 测试实现 | 与测试一致 |
| DESIGN-D2-01 | design-d2 | P2 | 物理地址类型身份 | ② | `BootPrepareResult`/`enter_kernel_image_v1` 的 bump/arena 从裸 `u64` 改物理区间类型；F-R20-01（R21） | D2 条目 | 不改本轮 |
| DESIGN-D2-02 | design-d2 | P1 | 其余裸加回绕 | ① | `kernel_info` 有序/跨表/startup、`vm_handoff` from_memmap/cut、`memmap_source` 覆盖判定全部 `checked_add` | 源码 grep | `minix-boot` 188、kernel 860 |
| DESIGN-D2-03 | design-d2 | P3 | 端点/长度接口面 | ② | 引入 `Range{start,end}` 并让 `cut` 收区间；F-R20-02（R21） | D2 条目 | 不改本轮 |
| DESIGN-D2-04 | design-d2 | P2 | 测试路径假 'static | ② | `build_memmaps`/`exit_boot_services` 的测试内核所有权迁移；F-R20-03（R21） | D2 条目 | 不改本轮 |
| DESIGN-D2-05 | design-d2 | P2 | 测试快照非配对 | ① | `exit_boot_services` 注释改“best-effort、不与 build_memmaps 同 MapKey”；生产用 ebs_pair | 源码注释 | 与 §3.1 一致 |
| DESIGN-D2-06 | design-d2 | P3 | 单实现 trait | ③ | 与 F-R17-01 同一裁决点，未新增证据；保留待用户裁决 | trait-audit 1/15/0 | 不重复记账 |
| DESIGN-D2-07 | design-d2 | P3 | 入口注释身份歧义 | ① | `main.rs` 注释改“serialize BootHandoffV1” | 源码注释 | 与 §4.1 一致 |

### R20 独立回读（RB20）与补修

`panel/round20-rb.md` 判 NOT PASS（P0=0、P1=0；两条 P2 + 五条 P3）。12 条 ① 中 10 条成立；阻断项是 R20 新引入的 `LSR` 释义错误与参数计数未同步。

| RB20 项 | 严重度 | 处置 | 证据 |
|---|---|---|---|
| `LSR` 被释成段选择子 RPL | P2 | ① 改为“串口 Line Status Register（0x3FD）”，与 §6.3/`lib.rs:341-343` 一致 | 源码注释 |
| 参数分歧八条未同步 §4.9 与源码注释 | P2 | ① §4.9 改“八条”；`boot_params.rs:8` 改 eight | 文档/源码 grep |
| §7.2 枚举漏 `pm/misc.c` | P3 | ① 补入枚举 | 正文 422 行引用 |
| §4.6 “预算是 1024 页”未限定腿 | P3 | ① 改为“UEFI/U-Boot 腿 1024，两个直核面 256” | `bootface.rs:135` |
| §3.4.3 `x0` 泛化 | P3 | ① 改为三腿寄存器 `rbx`/`x0`/`a0` | §3.7 权威表 |
| §5.2.1 “整条截断”措辞 | P3 | ① 改“整条丢弃” | §3.4.2 合同 |
| 新增 `checked_add` 缺负控 | P3 | ① `kernel_info` 对每条 memmap/reserved 做 `checked_add`，新增单区间溢出负控；`minix-boot` 189 | `test_validate_rejects_memmap_span_overflow` |
| QEMU 三门未重放 | — | 回读局限，本轮提交前主 agent 重跑三门 | 见 loop §4 |

### R20 meta-review 补修

`meta/01-boot-shim-bootstrap-r20.md` 判 **IMPROVED / NOT PASS**（抽样 22/28=78.6%，六条 ④ 未修记已修；三组探针 FAIL 成立）。逐条：

| meta 项 | 严重度 | 处置 | 证据 |
|---|---|---|---|
| §5.2.1 仍写“整条截断” | P2 | ① 改“整条丢弃” | 3071 行 |
| A-07 长单元格 | P3 | ① 已拆短：主句 + 分组 + 定义指针 | §3.4.1 `memmap` 行 |
| A-10 两节标题重复 | P3 | ① 2.4.1.2 标题改“消费者与校验缺口” | 标题 |
| B-08 “建表最后一步” | P3 | ① 改“建表第三步 `pg_mapkernel`” | 82 行 |
| B-14 §5.3 双状态词汇 | P3 | ① 列名改“详情（状态口径以 §5.1 为准）” | §5.3 表头 |
| B-17 §4.9 反事实越界 | P3 | ① 静态池段前加“属 §3.8，只为对照本表行” | §4.9 |
| B-18 §3.4.1/§4.4 重复 | P3 | ① §4.4 开头加“各腿产出来源以 §3.4.1 为权威” | §4.4 |
| D2-02 负控只覆盖 memmap | P3 | ② 代表负控已够；其余 12 个调用点同构，登记 F-R20-04（R21）观察 | `kernel_info` 测试 |
| F-R20-01..03 登记位置 | P2 | ① 移入 `OPEN-DEBT.md`（R21） | OPEN-DEBT |
| F-R17-01..04 截止过期 | P2 | ① 截止改 R21，并在 OPEN-DEBT 标“已逾期一轮” | OPEN-DEBT |
| B-54/B-55/B-56 未落盘 | P2 | ① 写入 `BLIND-SPOTS.md` | BLIND-SPOTS |
| DECISIONS 无 RB20 判决 | P2 | ① 在 `DECISIONS.md` 加 RB20 NOT PASS 与 D1 停跑行 | DECISIONS |

## R21 面板轮（2026-11-09，R20 meta 后；D1 停跑）

> 面板：reader-a 20、reader-b 20、reader-c 11、refute 7、design-d2 5；P-FACT 两次派发均未落盘（MISSING，fact 面由 REFUTE-03/05 与机械门抽查替代）。总计 63 条。

| 发现 ID | 来源 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| READER-A-01、READER-C-02、REFUTE-02 | reader/refute | P1 | 16 条截断 off-by-one | ① | §2.4.1.4 的 0-based 下标由 17..=39 改 16..=39 | `NR_MEMS=16`、C 1-based 17–40 | 两处一致 |
| READER-A-04、READER-A-05、READER-A-06、READER-A-07 | reader-a | P2/P3 | 术语/文本破损/路径 | ④ | 删“快照序号”死引用；修“（规范集合）”重复与“字段删除”断句；UEFI 固件长模式/分页限定；`fn main` 运行环境限定 | 文档内 | 六门 |
| READER-A-02..03、READER-A-08..20 | reader-a | P2/P3 | 计数/术语/编号 | ④ | **R22 L0 已修**（A-02 128/64、A-10 合并保留 ③、A-16 2MiB−16 来源等）；A-03 五条腿列举实为完整（③） | 文档 diff | 六门 |
| READER-B-01 | reader-b | P1 | `pg_alloc_page` 角色被替换 | ① | §3.8/§4.9/§7.5 改：页表页来源是 `alloc_pagetable`；`pg_alloc_page` 是 4KB 数据页，Rust 无同名对应物（运行期 VM `phys_mem`） | C `pg_map`/`alloc_pagetable`、`pt_alloc` | 三处一致 |
| READER-B-02..20 | reader-b | P2/P3 | 结构/权威/重复 | ④ | **R22 L0 已修**：B-02 [ARCH] 摘要化、B-03 权威规则、B-04 排序措辞、B-05 字段去向、B-07 计数索引、B-08 容器标题、B-09 两说法、B-10 防线 A/B/C、B-11 容量三段、B-12 三级容量、B-13 定义节、B-14 脚注、B-16 章范围、B-17 TODO-05 条件式、B-18 教学目标⑨、B-19 三段、B-20 状态口径；B-15 合并保留（③） | 文档 diff | 六门 |
| READER-C-01 | reader-c | P1 | EBS 循环普遍化 | ① | §3.7 改“两条 UEFI 腿在 EBS 配对循环；riscv64 U-Boot shim 无 EBS，直接读 DTB” | §3.1/§4.8 | 四处一致 |
| READER-C-03、READER-C-04、READER-C-06、READER-C-07 | reader-c | P2 | 时序/角色/签名/门脚本 | ① | M16 改“布局确定后拷贝前记录跨度”；M3 改“Rust 页表页来自注册分配器，不向 memmap 取页”；§3.7 改“借用 `&KernelInfo` 入口深拷”；M12 补 aarch64/x86 断言 | §4.2.1/§4.4/§5.2.3 | 与源码一致 |
| READER-C-05、READER-C-08..11 | reader-c | P3 | 边界/指代 | ④ | **R22 L0 已修**：C-05 指针、C-08 切根定义、C-09 三次分配、C-10 OVMF 观测限定；C-11 已注明 v1 接收不打印（③） | 文档 diff | 六门 |
| REFUTE CLAIM 01、03、05 | refute | — | 反证成立 | ③ | 核心成立，保留；CLAIM 01 的裸 `=` 边界登记 P3 | panel 内证据 | — |
| REFUTE CLAIM 02 | refute | P1 | 16 条截断 off-by-one | ① | 同 REFUTE-02 行 | panel 内证据 | 见该行 |
| REFUTE CLAIM 04 | refute | P2 | 十条分歧 | ① | 同 REFUTE-04 行 | panel 内证据 | 见该行 |
| REFUTE CLAIM 06 | refute | P2 | arena 回收 | ① | 同 REFUTE-06 行 | panel 内证据 | 见该行 |
| REFUTE CLAIM 07 | refute | P2 | M2 覆盖账 | ① | 同 REFUTE-07 行 | panel 内证据 | 见该行 |
| DESIGN-D2-01、DESIGN-D2-05 | design-d2 | P2 | 代码注释/测试路径口径 | ① | 删 UEFI `BootShim` 实现残留注释；§3.4.1 改“测试内核走 before_tables/arch_boot_impl，不走 validate” | `uefi_helpers.rs`/`lib.rs` | 一致 |
| DESIGN-D2-02 | design-d2 | P2 | 测试路径假 'static | ② | F-R20-03（R21） | D2 条目 | 不改本轮 |
| DESIGN-D2-03 | design-d2 | P3 | 负控覆盖不足 | ② | F-R20-04（R21） | D2 条目 | 不改本轮 |
| DESIGN-D2-04 | design-d2 | P2 | 物理地址类型身份 | ② | F-R20-01（R21） | D2 条目 | 不改本轮 |

## R22 面板轮（R21 meta 后；先清 R21 携带）

> 面板：P-FACT-A 3、P-FACT-B 2、READER-A 20、READER-B 20、READER-C 10、REFUTE 7 主张、DESIGN-D2 5。总计 67 条面板条目（其中 REFUTE 4 条非阻断主张）。R21 携带 36 条已在 L0 补修。

| 发现 ID | 来源 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-A-01 | fact-a | P1 | 868 读数口径 | ① | §5.2.1 命令补 `--lib`（否则含集成测试得 878） | `cargo test -- --list` | 命令可重放 |
| FACT-A-02 | fact-a | P1 | “新 impl BootShim”过时 | ① | §3.3 改“新入口/共享库；只有需要 BootShim 语义才新增 impl” | 源码单实现 | 与 §3.2 一致 |
| FACT-A-03 | fact-a | P2 | `Type::method` 锚点逃逸 | ③ | anchor-resolve 文法不认 `Type::method`；符号存在，转 R23 工具候选（B-59） | `--extract` 零命中 | 不改文档 |
| FACT-B-01、FACT-B-02 | fact-b | P2/P3 | 引文省略低报 | ① | §4.2 块头补 arena 断言/扩容分支/dumpdtb cfg；§3.2/§4.3 块头改“文档注释整段略去” | 源码对读 | 块头与内容一致 |
| READER-A-01..20 | reader-a | P1/P2/P3 | 读者障碍 | ④ | 逐 ID 已处理：A-01/02/03/04/05/06/07/08/09/10/11/12/13/15/16/17/18/19/20 有正文改动或已在 R21 落定；A-05 已改动作 2 时间列：时间 3 属拷贝/模块段，动作 3 在时间 4；A-14 规范计数已写入 §3.7 | 各条锚点在 panel 内 | 六门 |
| READER-B-01..20 | reader-b | P1/P2 | 结构/权威 | ④ | 逐 ID 已处理：B-01..B-20 均按 panel 建议改正文（B-15 两节合并会丢 2.4.1.3 完整推演，保留两节；B-10 探针读数已移 §5.2.3，§3.4.4 留指针；均按 ④ 计入）；含 startup 时序、静态标量/运行期字段、权威规则、alloc_lowest 指 SMP 篇、定义节、三级容量、脚注改行内链接、动作 7–8、状态四层 | 各条锚点在 panel 内 | 六门 |
| READER-C-01..10 | reader-c | P2/P3 | 跨节因果 | ④ | 逐 ID 已处理：C-01..C-10 均改正文；C-05 补“不分配≠不访问”、C-07 随 M12 改写消解、C-08 改 v0–v4 整组两次；C-06 逐链终点、C-09 门表补交叉表行、C-10 内联形态标非生产 | 各条锚点在 panel 内 | 六门 |
| REFUTE-01 | refute | P1 | EBS 审计切片漏尾 | ① | `ebs_boundary_audit.rs` 的 `function_body` 改花括号配对（处理注释/字符串/字符），新增嵌套块负控；集成测试 5 条 | `cargo test --test ebs_boundary_audit` 5 通过 | 尾部插入 `uefi::` 会红 |
| REFUTE-02 | refute | P2 | `board=` 双记录全称 | ① | §2.3 补“若后置键被提升为首条则被删、无双记录”分支 | C 逐行复算 | 两分支都写明 |
| REFUTE-03 | refute | P2 | `nr_tasks` 载体错配 | ① | §3.4.2 行改 `NR_PROCS`/`NR_TASKS`，注明 12 是 `NR_BOOT_MODULES` 不是 `nr_tasks` | C `main.c:431`、`com.rs:67` | 与源码一致 |
| DESIGN-D2-01、D2-05 | design-d2 | P1/P3 | 测试形态/角色措辞 | ① | §3.2 改“无测试第二实现”；§7.5/§4.9 `alloc_lowest` 统一“暂未移植” | 源码 | 一致 |
| DESIGN-D2-02、D2-03、D2-04 | design-d2 | P1/P2 | 类型身份/假 'static/负控 | ② | F-R20-01/03/04（R23） | D2 条目 | 不改本轮 |

### P-REFUTE 附加处置（与上表 aliases：`CLAIM-01..03` = `REFUTE-01..03`；`DESIGN-D2-03..05` 与上表同 ID 的重复行是分工注记，计数按上表一次）

| 发现 ID | 来源 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| CLAIM-01 | refute | P1 | EBS 审计切片 | ① | 同 REFUTE-01 | panel | 5 条测试通过 |
| CLAIM-02 | refute | P2 | board 双记录 | ① | 同 REFUTE-02 | panel | 两分支写明 |
| CLAIM-03 | refute | P2 | nr_tasks 载体 | ① | 同 REFUTE-03 | panel | 源码一致 |
| CLAIM-04、CLAIM-05、CLAIM-06、CLAIM-07 | refute | — | 反证未成立/保留 | ③ | 保留原叙述 | panel | — |
| DESIGN-D2-03 | design-d2 | P2 | 测试路径假 static | ② | F-R20-03（R23） | D2 | 不改本轮 |
| DESIGN-D2-04 | design-d2 | P2 | 负控覆盖 | ② | F-R20-04（R23） | D2 | 不改本轮 |
| DESIGN-D2-05 | design-d2 | P3 | alloc_lowest 角色措辞 | ① | §4.9/§7.5 统一“暂未移植” | 源码 | 一致 |

## R23 前置裁决落地（2026-11-09，不占面板轮）

> 两条 ② 欠账在 R22 后仍挂“待用户裁决”；结构化提问未获答复。按用户「除 F-R14-04 外必须全修」的现行指令与两条推荐项代裁（`DECISIONS.md` D-026，可回退）。

| 发现 ID | 来源 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| F-R17-01（DESIGN-D2-12） | design-d2 | P2 | 单实现 trait | ① | 删 `minix-boot::BootShim`；U-Boot 腿改 `opensbi_helpers::prepare_boot` 普通函数；`uboot_main` 直接调用；§3.2 重写 | `grep -rn "impl BootShim" os/` 零命中；`cargo test -p boot-shim --features test-all` 135+5；QEMU 三门 PASS | 行为不变 |
| F-R17-04（DESIGN-D2-18） | design-d2 | P2 | 权威错配 | ① | `KernelInfo` 13→14 加 `authority: InputObjectId`；`validate_requiring_sources()` 无参；wire 解码/直核/UEFI 三处绑定；文档 §3.4.1/§4.4 同步 | `minix-boot` 192；`minix-kernel` 862+8；`minix-arch` 287；三 QEMU 门 PASS | 字段表与绑定说明一致 |

## R23 面板轮（2026-11-09，R23 前置裁决落地后）

> 面板：FACT-A 7、FACT-B 17、READER-A 24、READER-B 20、READER-C 20、REFUTE 6 主张、DESIGN-D2 16。合计 104 条面板条目 + 6 条主张。

| 发现 ID | 来源 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-A-01 | fact-a | P1 | fact | ① | §4.4 引文补 `authority` 形参与字段 | doc diff；grep | 引文与源码同形 |
| FACT-A-02 | fact-a | P1 | fact | ① | §4.4 字段表加 `authority` 行 | doc diff | 14 字段齐全 |
| FACT-A-03 | fact-a | P2 | fact | ① | §4.2 引文 `arena_pa.0`→`arena_pa` | doc diff | 与 PhysBytes 签名一致 |
| FACT-A-04 | fact-a | P2 | count | ① | §3.4.5 两个→三个类型化地址 | doc diff | 括号三项一致 |
| FACT-A-05 | fact-a | P2 | fact | ① | §5.1 M13/§5.2.1 改 build_memmaps 调用点描述 | doc diff | 生产不调用、15 测试内核调用 |
| FACT-A-06 | fact-a | P2 | scope | ① | related-code 删 memmap.rs，补 5 个生产文件 | doc diff | 48 行逐条存在 |
| FACT-A-07 | fact-a | P2 | fact | ① | `collect_param_buf`→`collect_param_buf_into` | doc diff | grep 命中 |
| FACT-B-01 | fact-b | P0 | quote | ① | 同 FACT-A-01：补 `authority` 形参与字段 | doc diff | 引文可编译 |
| FACT-B-02 | fact-b | P0 | fact | ① | §3.2 cfg 陈述限定 qemu-test-kernels，删“业务逻辑无条件编译”绝对句 | doc diff | 与 8 处 gate 一致 |
| FACT-B-03 | fact-b | P0 | fact | ① | §4.1 注册失败消息改 `pt_alloc::alloc_pt_page called before register` | doc diff | 与源码断言一致 |
| FACT-B-04 | fact-b | P0 | fact | ① | §4.6 identity_device_windows 改“整表、无筛选”，Step 2c 另表 | doc diff | 与 minix-arch 表一致 |
| FACT-B-05 | fact-b | P0 | quote | ① | §3.1 exit_boot_services 引文恢复 SAFETY 注释与 cfg | doc diff | 逐字对读 |
| FACT-B-06 | fact-b | P0 | coverage | ① | §1.4 边界表加 dtb_copy/resv_claims 行，§7.3 补两文件 | doc diff | 范围声明可见 |
| FACT-B-07 | fact-b | P1 | coverage | ① | §7.3 补 boot_storage,bump | doc diff | 清单含承重类型 |
| FACT-B-08 | fact-b | P1 | quote | ① | §3.7 resume_additive 省略说明改为首 3 行/结尾 10 行 | doc diff | 与源码行号一致 |
| FACT-B-09 | fact-b | P1 | fact | ① | §3.6 Paging 20 成员，列 6 项为举例 | doc diff | 与 paging.rs 一致 |
| FACT-B-10 | fact-b | P2 | evidence | ① | §3.2 补删除前 rev `5ed051a53` | doc diff | 命令可复算 |
| FACT-B-11 | fact-b | P2 | quote | ① | §4.3 UbootFileLoader 引文恢复 `|iter|` 与借用合同注释 | doc diff | 逐字对读 |
| FACT-B-12 | fact-b | P2 | fact | ① | §6.1 补 arch_boot arg/reslen 取证行 | doc diff | 两行在表中 |
| FACT-B-13 | fact-b | P2 | quote | ① | §2.6.3 createpde 补 `phys_bytes offset;`，省略说明改实 | doc diff | 与 memory.c 一致 |
| FACT-B-14 | fact-b | P2 | count | ① | 同 FACT-A-04：两个→三个 | doc diff | 三项一致 |
| FACT-B-15 | fact-b | P2 | fact | ① | §2.4.1.1 升序加“规范不保证”限定 | doc diff | 前提降级 |
| FACT-B-16 | fact-b | P2 | evidence | ① | §4.6 加 MTRR 未验证前提，§5.3 立 TODO-10 | doc diff | TODO 表含 10 |
| FACT-B-17 | fact-b | P2 | scope | ① | 同 FACT-A-06：清单删/补 | doc diff | 48/48 存在 |
| CLAIM-01 | refute | — | claim | ③ | 反证失败，主张成立（自由函数/入口分工） | refute.md | 源码对读 |
| CLAIM-02 | refute | P2 | fact | ① | §3.2 限定“两条 boot-shim 腿”并改 cfg 计数 | doc diff | 与源码一致 |
| CLAIM-03 | refute | — | claim | ③ | 反证失败，主张成立（bootface 等价） | refute.md | 源码对读 |
| CLAIM-04 | refute | P1 | quote | ① | 同 FACT-A-01：引文补 authority | doc diff | 引文可编译 |
| CLAIM-05 | refute | P1 | fact | ① | §4.4 门链改“防线层次”，补腿间执行序（直核 2→3→1→4；v1 4→3→1 无 2） | doc diff | 与 lib.rs 调用点一致 |
| CLAIM-06 | refute | P1 | fact | ① | §6.3/M15 限定 UEFI 两腿；其余三腿写实际防线 | doc diff | 与调用点一致 |
| READER-A-01 | reader-a | P0 | reader | ④ | §1.4 计数索引收成“生产腿 5”+§3.7 表 | doc diff | 六门 |
| READER-A-02 | reader-a | P0 | reader | ④ | §1.3 D5 改“与 D1 并列” | doc diff | 六门 |
| READER-A-03 | reader-a | P0 | reader | ④ | 编号图例移 §1.4，补 R20–R23 | doc diff | 六门 |
| READER-A-04 | reader-a | P0 | reader | ④ | §2.1 例外限定为“有结构体字段/参数表查询” | doc diff | 六门 |
| READER-A-05 | reader-a | P1 | reader | ④ | §2.3 长段拆三条 bullet | doc diff | 六门 |
| READER-A-06 | reader-a | P1 | reader | ④ | §4.4 表头计数拆三句 | doc diff | 六门 |
| READER-A-07 | reader-a | P1 | reader | ④ | kern_stack_top 长单元移表后 bullets | doc diff | 六门 |
| READER-A-08 | reader-a | P1 | reader | ④ | 动作 2 约束移表后 bullets | doc diff | 六门 |
| READER-A-09 | reader-a | P2 | reader | ④ | 代码块编辑注释移出，只留一行省略 | doc diff | 六门 |
| READER-A-10 | reader-a | P1 | reader | ④ | 首次出现处定义 k | doc diff | 六门 |
| READER-A-11 | reader-a | P1 | reader | ④ | 定义“观测常态”并统一用词 | doc diff | 六门 |
| READER-A-12 | reader-a | P1 | reader | ④ | 动作 ID 标“非时序”，列名同步 | doc diff | 六门 |
| READER-A-13 | reader-a | P1 | reader | ④ | §3.7 术语对账改表 | doc diff | 六门 |
| READER-A-14 | reader-a | P2 | reader | ④ | M4/M12 注明 TODO 与覆盖分层 | doc diff | 六门 |
| READER-A-15 | reader-a | P2 | reader | ④ | §3.3 图补 boot-shim 虚线边 | doc diff | 六门 |
| READER-A-16 | reader-a | P2 | reader | ④ | §3.5 判据句拆三句 | doc diff | 六门 |
| READER-A-17 | reader-a | P2 | reader | ④ | §1.4 边界段拆三条 bullet | doc diff | 六门 |
| READER-A-18 | reader-a | P3 | reader | ③ | 低置信：表已穷举条件组合；保留表并已加 k 定义，不强行压成单一规则 | panel + doc | 无新证据 |
| READER-A-19 | reader-a | P2 | reader | ④ | §1.2 图加阅读顺序/依赖方向图例 | doc diff | 六门 |
| READER-A-20 | reader-a | P2 | reader | ④ | §2.5 改“被包含者作 cmp” | doc diff | 六门 |
| READER-A-21 | reader-a | P2 | reader | ④ | §2.2 第二个 push 注明对齐补位 | doc diff | 六门 |
| READER-A-22 | reader-a | P2 | reader | ④ | §6.1 “两条腿”点名 | doc diff | 六门 |
| READER-A-23 | reader-a | P2 | reader | ④ | 信封容量写法统一 | doc diff | 六门 |
| READER-A-24 | reader-a | P2 | reader | ④ | 1GiB 疑问标“理论路径” | doc diff | 六门 |
| READER-B-01 | reader-b | P1 | fact | ① | 同 FACT-B-03：注册失败消息 | doc diff | 与源码断言一致 |
| READER-B-02 | reader-b | P1 | fact | ① | 同 FACT-A-01：补 authority 引文 | doc diff | 引文可编译 |
| READER-B-03 | reader-b | P1 | fact | ① | §2.4 add_memmap 入口 assert 与空洞的关系 | doc diff | 与 pg_utils.c 一致 |
| READER-B-04 | reader-b | P2 | reader | ④ | 同 A-02：D5 并列 | doc diff | 六门 |
| READER-B-05 | reader-b | P1 | fact | ① | §4.8 aarch64/riscv 表行拆开，属性名写清 | doc diff | 与 bootface_a64 一致 |
| READER-B-06 | reader-b | P2 | reader | ④ | §2.2 ABI 段移到 push 序列之后 | doc diff | 六门 |
| READER-B-07 | reader-b | P2 | reader | ④ | 同 A-03：编号图例移 §1.4 | doc diff | 六门 |
| READER-B-08 | reader-b | P2 | reader | ④ | §3.5 历史险情提到表前 | doc diff | 六门 |
| READER-B-09 | reader-b | P1 | fact | ① | M12 改“两条固定标记+三条变体之一” | doc diff | 与 §5.2.3 一致 |
| READER-B-10 | reader-b | P2 | reader | ④ | §2.4 结算改“有据可查”，说明框 1 原材料 | doc diff | 六门 |
| READER-B-11 | reader-b | P2 | reader | ④ | 同 A-12：动作 ID 非时序 | doc diff | 六门 |
| READER-B-12 | reader-b | P2 | fact | ③ | 低置信：同名函数范围已在表内，总结未夸大到实现体 | panel + doc | 无新证据 |
| READER-B-13 | reader-b | P2 | reader | ④ | 同 B-13/C-04：条件结论限定 | doc diff | 六门 |
| READER-B-14 | reader-b | P2 | reader | ④ | 同 A-01：计数索引 | doc diff | 六门 |
| READER-B-15 | reader-b | P2 | reader | ④ | §2.6.4 改前向指针，不在此下 §3.7 结论 | doc diff | 六门 |
| READER-B-16 | reader-b | P1 | fact | ① | §5.2.2 明确 VMA 跨距 vs LMA kern_size | doc diff | 与 check-layout L5 一致 |
| READER-B-17 | reader-b | P1 | fact | ① | arena 求和语序改为“合计 30624，余 2144” | doc diff | 算术一致 |
| READER-B-18 | reader-b | P2 | reader | ④ | 同 A-15：依赖图补虚线 | doc diff | 六门 |
| READER-B-19 | reader-b | P2 | reader | ④ | §1.1 缩写表补 EBS | doc diff | 六门 |
| READER-B-20 | reader-b | P2 | reader | ④ | §6.1 v0–v4 补发送/接收分界 | doc diff | 六门 |
| READER-C-01 | reader-c | P1 | fact | ① | 同 FACT-B-03：注册失败消息 | doc diff | 与源码断言一致 |
| READER-C-02 | reader-c | P1 | fact | ① | “任意一次”改“更高下标耗尽/序打乱后下一次” | doc diff | 与 §2.4.1.1 一致 |
| READER-C-03 | reader-c | P1 | fact | ① | §2.4 取整加“非退化不改变净账本”回指 | doc diff | 与 §2.4.1.3 一致 |
| READER-C-04 | reader-c | P1 | reader | ④ | 同 B-13：条件结论限定 | doc diff | 六门 |
| READER-C-05 | reader-c | P1 | fact | ① | 同 REFUTE-05：门链改防线层次 | doc diff | 与 lib.rs 一致 |
| READER-C-06 | reader-c | P1 | fact | ① | §1.1 “没有报错输出通道”限定为无 printf，panic 屏幕另述 | doc diff | 与 §6.3 一致 |
| READER-C-07 | reader-c | P1 | fact | ① | §4.3 改“三道装载期 A/B/C + 接收期 D” | doc diff | 与 §4.9 一致 |
| READER-C-08 | reader-c | P1 | fact | ① | §4.9 “全部 DRAM”限定按腿 | doc diff | 与 §3.4.1 一致 |
| READER-C-09 | reader-c | P1 | reader | ④ | §2.7 “唯一”限定接口层 | doc diff | 六门 |
| READER-C-10 | reader-c | P1 | fact | ① | §2.6.4 补 TLB 窗口证据措辞 | doc diff | 源码注释对读 |
| READER-C-11 | reader-c | P1 | fact | ① | “20 号槽”→“最后一个槽（下标 19）” | doc diff | 计数一致 |
| READER-C-12 | reader-c | P2 | reader | ④ | 同 B-10：有据可查 | doc diff | 六门 |
| READER-C-13 | reader-c | P1 | fact | ① | §3.5 风险表首行改 OpenSBI 区间标签 | doc diff | 与 §3.4.1 一致 |
| READER-C-14 | reader-c | P1 | fact | ① | §6.3 列两种 KernelMapError 文案 | doc diff | 与 boot_policy.rs 一致 |
| READER-C-15 | reader-c | P1 | fact | ① | “七个落地区”→“七个加名字池” | doc diff | 计数一致 |
| READER-C-16 | reader-c | P2 | reader | ④ | 同 A-15：依赖图补虚线 | doc diff | 六门 |
| READER-C-17 | reader-c | P2 | fact | ③ | 低置信：NUL 后行为属 C 缓冲语义，文档已限定扫描范围；不改 | panel + C 源 | 无新证据 |
| READER-C-18 | reader-c | P1 | fact | ① | §2.4.2 结算改“VM 模块的字节” | doc diff | 与本节末一致 |
| READER-C-19 | reader-c | P2 | reader | ④ | §7.5 栈数组顶改“数组自身的顶” | doc diff | 六门 |
| READER-C-20 | reader-c | P1 | fact | ① | M11 改“A/B 两道 + C 见 §4.3” | doc diff | 与 §4.3 一致 |
| DESIGN-01 | design-d2 | P2 | design | ① | strict 分支加三条派生等式 + 3 条负控 | code diff; minix-boot 196 | 测试通过 |
| DESIGN-02 | design-d2 | P2 | design | ① | MODULE_NAMES 下沉 minix-boot；strict 按位置核名 + 1 条负控 | code diff; minix-boot 196 | 测试通过 |
| DESIGN-03 | design-d2 | P2 | design | ① | BootStorageCell 改一次性 `take`，二次 panic | code diff | QEMU 三门 PASS |
| DESIGN-04 | design-d2 | P2 | design | ① | boot_storage 模块文档收窄 source/startup 在 UEFI 腿直写 wire | code diff | 文档与代码一致 |
| DESIGN-05 | design-d2 | P3 | design | ③ | `write_envelope(&KernelInfo)` 被跨 crate 测试使用，不是死接口 | grep 调用点 | 保留 |
| DESIGN-06 | design-d2 | P2 | design | ① | 新增 `envelope_offsets`（全部 `offset_of!`），读写两侧改为 re-export；单一来源（重构豁免：行为由编译期布局门与 round-trip 测试覆盖） | `handoff_v1.rs` 模块；minix-boot 196 | 编译期偏移同源 |
| DESIGN-07 | design-d2 | P2 | design | ① | 用 `Classification` 计数替代编码后解码 | code diff; ebs 门 PASS | 删除多余解码 |
| DESIGN-08 | design-d2 | P2 | design | ① | 删 `classify` 的 `vm_module_idx` 死参数 | code diff; kernel 862+8 | 测试通过 |
| DESIGN-09 | design-d2 | P2 | design | ① | 删两个 bootface 的冗余 `pt_alloc::register` | code diff; QEMU 三门 PASS | 测试通过 |
| DESIGN-10 | design-d2 | P2 | design | ② | 两个 bootface 流程合并需架构 owner 裁决；R24 | follow-ups.md | 带日期转出 |
| DESIGN-11 | design-d2 | P2 | design | ② | 删 `VmBootHandoff.kern_ident` 需 wire v9 + VM server 同步；R24 | follow-ups.md | 带日期转出 |
| DESIGN-12 | design-d2 | P2 | design | ① | 新增 `land_slice<T: Copy>`，五段落地区收成一次调用（重构豁免：行为由 `store_kernel_info` 落地区测试与七条溢出负控覆盖） | `os/kernel/src/lib.rs`；kernel 862+8 | 测试通过 |
| DESIGN-13 | design-d2 | P2 | design | ② | `pt_alloc` release 二次注册语义需用户裁决；R24 | follow-ups.md | 带日期转出 |
| DESIGN-14 | design-d2 | P2 | design | ① | 同 FACT-A-01：§4.4 引文补 authority | doc diff | 引文可编译 |
| DESIGN-15 | design-d2 | P2 | design | ① | §1.4 显式排除 dtb_copy/resv_claims；§7.3 补两文件 | doc diff | 范围声明可见 |
| DESIGN-16 | design-d2 | P2 | design | ① | `boot_shim.rs`→`boot_prepare.rs`，模块名与内容一致 | code diff; 全仓引用更新 | 测试通过 |

### R23 独立回读（RB23）与补修

`panel/round23-rb.md` 首轮判 **NOT PASS**（抽样 63 条：真修 56、半修 6、处置错 1；④ 16/17）。阻断项逐条补修后第二次回读 **PASS**：

| RB23 项 | 严重度 | 处置 | 证据 |
|---|---|---|---|
| B1 §4.2 引文仍写 `decode_descriptors` | P0 | ① 改为 `storage.commit_map_tables(classification.conventional, classification.reserved)` | doc:2283 与 `uefi_helpers.rs:1126` 同形 |
| B2 “源码注释声明 TLB 覆盖”无源 | P1 | ① 删除归因，改“本文推断，C 源无成文依据（待源码核实）” | doc:1132 与 `protect.c:352-368` 对读 |
| Q1 related-code 仍列 `boot_shim.rs` | P2 | ① 改 `boot_prepare.rs`，48/48 存在 | `related-code.txt:35` |
| Q2 `Paging` 20→22；const assert 旧引用 | P2 | ① 改 22 个成员；删 const assert 句 | doc:1827、doc:2515 |
| Q3 ② 五条无日期 | P3 | ① 补 `2026-11-16` | `follow-ups.md:113-117` |
| Q4 `§3.7 禁止`仍在 §2.6.4 | P3 | ① 改纯前向指针 | doc:1132 |

## R24 面板轮（2026-11-09，R23 后 BOOT-SCOPE/去重/wire v9 改动）

| 发现 ID | 角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-A-01 | fact-a | P1 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:7`；问题：F01（P0）EBS 之后仍有 `uefi::println!`，与"零 uefi::"声明冲突 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-02 | fact-a | P1 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:22`；问题：F02（P0）§3.7 的 `enter_kernel_image_v1` 代码引用是 BOOT-SCOPE 2a 之前的旧体 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-03 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:36`；问题：F03（P0）`pt_alloc::register` 的构建档行为与文档相反 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-04 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:50`；问题：F04（P0）`VmBootHandoff.ident` 作为 64 条容量上限被引用，但 v9 已删字段与常量 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-05 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:65`；问题：F05（P1）§4.6 的 Step 1 窗口清单漏掉固定低 2 MiB AP 跳板窗口 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-06 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:79`；问题：F06（P1）§4.2.1 动作 7 的"解码在 EBS 之后"与代码时序相反 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-07 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:92`；问题：F07（P1）§4.5 的 `store_kernel_info` 引用块是 R23 `land_slice` 重构前的旧体 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-08 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:106`；问题：F08（P1）§5.2.1 的 boot-shim 测试计数与逐文件表不一致 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-09 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:120`；问题：F09（P1）§4.4 的直核面执行序漏掉 strict 门 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-10 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:133`；问题：F10（P1）§5.2.1 的 `paging.rs` 条数少一条 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-11 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:146`；问题：F11（P1）§3.2 的 `place_modules` 参数清单写成"只传三项" | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-12 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:159`；问题：F12（P2）§4.6 的"跨镜像腿包含 shim 的 PE image / shim 栈"对 U-Boot shim 不成立 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-13 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:172`；问题：F13（P2）§5.1 M7 / §5.2.1 / §5.3 TODO-02 未登记并发注册测试 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-14 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:185`；问题：F14（P2）§7.3 关联 Rust 源码清单漏掉新文件 `bootface_common.rs` | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-A-15 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-a.md:198`；问题：F15（P2）§3.3 依赖图把 `arch_boot` 写成 boot-shim 的调用面 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-01 | fact-b | P0 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:94`；问题：P0-01 pt_alloc “写一次”引文与现行策略相反 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-02 | fact-b | P0 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:118`；问题：P0-02 `VmBootHandoff.ident` 在 v9 已删除，文档仍当活字段引用 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-03 | fact-b | P0 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:133`；问题：P1-01 §3.7 `enter_kernel_image_v1` 引文是旧函数体的拼合 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-04 | fact-b | P0 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:147`；问题：P1-02 §4.2 `prepare_and_enter_kernel_image` 末行调用缺第 4 实参 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-05 | fact-b | P0 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:161`；问题：P1-03 §4.5 `store_kernel_info` 引文把 `land_slice` 内联回主函数 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-06 | fact-b | P0 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:174`；问题：P1-04 §4.4 直核面执行序漏掉 bootface 的 strict 门 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-07 | fact-b | P1 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:190`；问题：P1-05 BOOT-SCOPE 2a 覆盖缺口：窗口怎么来、低 2 MiB 窗口没写 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-08 | fact-b | P1 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:205`；问题：P1-06 bootface 合并（`bootface_common.rs`）只有两句提及 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-09 | fact-b | P1 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:220`；问题：P1-07 测试计数漂移与算术错误 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-10 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:239`；问题：P1-08 文档引作 QEMU 证据的测试内核目前不能编译 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-11 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:255`；问题：P2-01 §4.4 门表把共享不变量只挂在 kmain 门 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-12 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:266`；问题：P2-02 §6.1 把 `step4 DM coverage (preflight + install)` 归给接收侧 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-13 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:277`；问题：P2-03 §3.6/§4.4 对“分配器已注册”的表述互相矛盾 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-14 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:288`；问题：P2-04 §2.4.1.3 跳板尺寸分解把 GDT 说成“每 CPU 一个 8 B 槽” | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-15 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:299`；问题：P2-05 §3.8 `classify` 引文块首两行中文说明未标编注 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-16 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:310`；问题：P2-06 §4.4 `boot_validate_and_prepare` 引文把断言消息换成 `"...“` | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-17 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:321`；问题：P2-07 §6.2 panic 引文把两行结构体字面量压成一行 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-18 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:332`；问题：P2-08 §2.7 earm `head.S` 代码块内替换 6 行英文注释 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-19 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:343`；问题：P2-09 §4.3 “tty/mib 互换”反事实对两个直核腿不成立 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-20 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:354`；问题：P2-10 §3.4.3 arena v8 字节数只有“九页”依据，36768/6144 无现行源可复核 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| FACT-B-21 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/fact-b.md:453`；问题：七条格式/行号/历史数字小项 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-01 | reader-a | P0 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:5`；问题：1. §1.2 四框图与三重编号在开篇叠加，读者建立不起可复用的地图 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-02 | reader-a | P0 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:16`；问题：2. §5.2.1 测试计数自相矛盾：逐文件相加是 135，正文写 136 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-03 | reader-a | P0 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:27`；问题：3. §5.1 与 §5.3 对 TODO-05 的状态给出相反答案 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-04 | reader-a | P0 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:38`；问题：4. §5.1 与 §5.3 对 TODO-07 的状态给出相反答案 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-05 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:49`；问题：5. §3.5 同一个误设险情被介绍两次，第二次称"另一处险情" | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-06 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:60`；问题：6. §2.2 "对齐补位"的说法与后文"栈对齐不满足 SysV 约定"相冲突 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-07 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:71`；问题：7. §2.1 五个 `.long 0` 的"后两个同样按位生效"没有说清由哪一位管 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-08 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:82`；问题：8. §2.5 "下一条再从页对齐处接着放"在引文里看不到对齐动作 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-09 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:93`；问题：9. §2.3 指向 §3.5 解释 `__k_unpaged_` 前缀，但 §3.5 直接用了已加前缀的名字 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-10 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:104`；问题：10. §3.8 反事实里的"下一次 `map_huge`"没有主语，读者不知道它何时发生 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-11 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:115`；问题：11. §4.2.1 九个动作按依赖编号而不按时间编号，读者每走一步都要查表 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-12 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:126`；问题：12. §4.6 "通常与 Step 1 的块尺寸相同"与引文里的固定 2 MiB 不一致（低置信、高价值） | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-13 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:137`；问题：13. §6.1 用 `reslen=0` / `reslen=127` 回指 §3.4.4，但 §3.4.4 的读数是 `res=127 zeroed=127` | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-14 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:148`；问题：14. §7.5 把 `load_stack_start` 的 Rust 对应物写成 `kernel_boot_stack_top`，但 sp 初值是 `kern_stack_top` | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-15 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:159`；问题：15. §1.4 在栈名表里首次使用"2 MiB 收口"，但"收口"到 §4.4 才解释 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-16 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:170`；问题：16. §1.4 的命名陷阱段引入 `mpx.S` 却不说明它是什么 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-17 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:181`；问题：17. §3.4.1 的"三级上限"里有两个 64，"超过 64"没说是哪一个 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-18 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:192`；问题：18. §3.9 承诺"完整字节构成见 §3.5"，§3.5 只给了两个字段的大小与总数 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-19 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:203`；问题：19. §2.4.2 开头说 `pre_init` 里的切除是"临时"的，但"临时"没有定义 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-A-20 | reader-a | P1 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-a.md:214`；问题：20. §2.6.2 的 `pg_map` 隐含前提没有引文，读者无法验证"pde 单调"（低置信、高价值） | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-01 | reader-b | P0 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:20`；问题：- 问题：【前提倒置/图解自相矛盾】图例把"竖线/箭头"定义成**阅读顺序**，但图中框 1 与框 2 之间唯一的箭头（▲）指向框 1，即方向是框 2 → 框 1，恰好逆着阅读顺序；正文紧接着又称它为"逆向箭头"。"逆向"只有在箭头表示依赖时才成立，于是同一支箭头在图例与正文里被赋了两种含义。另外"依赖 B（框 1 → 框 3，跳框）"没有任何箭头，只作为文字写在依赖 A 的箭头旁边，图上读不出这条依赖。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-02 | reader-b | P0 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:30`；问题：- 问题：【章节边界/指代系统】五处指代全部落空或反向：(a) §2.1 说"两个读者的具体落点见本节落链图"，但 §2.1 没有落链图，完整的落链图在 §2.3；(b) §4.2.2 说"下表已给五类落点"，给落点的是它**上面**的五步表，下面的令牌表给的是令牌的"失败落点"；(c) §3.4.1 的"删除"行说"逐个的去向见下表"，关系表之后没有表，21 字段的去向表在 §3.4.2；(d) §3.4.2 的表述里写"仍是活字段，见 §3.4.2"，这句话本身就在 §3.4.2 内，是自指；(e) §1.4 说"本篇只在 §4.2 的时序里交代它们的位置（preflight 在配对循环内、`authorize_and_copy` 在最后一次 `GetMemoryMap` 之后）"，但 §4.2 全节没有出现 `authorize_and_copy`，只交代了 DTB preflight。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-03 | reader-b | P0 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:40`；问题：- 问题：【职责重叠/契约矛盾】§5.1 自称"TODO 列的状态口径**以本表为唯一权威**"，但它与 §5.3 对同两项给出相反状态：M5 行写"TODO-05（开放，见 5.3）"，§5.3 写"✅ 已关闭（R23 后；宿主合成 layout 门）"；M13 行写"TODO-07（部分关闭：纯函数五案已交接，其余开放，见 5.3）"，§5.3 写"✅ 已关闭（R23 后）"。此外 TODO-08/09/10 只登记在 §5.3，§5.1 的 M1–M17 没有任何一行提到它们，因此这三项的"唯一权威状态"在 §5.1 里查不到；§5.3 还在 §5.1 允许的"已关闭/开放/部分关闭/—"之外，另用"已交接完成"标注了七项。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-04 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:50`；问题：- 问题：【前提倒置/归类错误】原文说"下面三条给论证（清单先于建表、恒等先于内核映射、结构先于 `kmain`）"，但三条里只有第一条是本节内自足的论证：第二条（恒等先于内核映射）明写"逐数字的推演在 §2.6.1……这里不重复"，是**指向**；第三条（结构先于 `kmain`）明写"关键那行 `push %eax` 在 §2.2 引文块之外"，关键证据不在本篇可读的引文里。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-05 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:60`；问题：- 问题：【前提倒置/职责重叠】小节标题是"`alloc_lowest` 的消费者与校验缺口"，但首段先给的是**下一小节**的结论（"`alloc_lowest` 则**不是**一条可行的'从低处分配'路径：切除那一行把长度当成结束地址传给 `cut_memmap`"），并在同一段里两次说"见紧随其后的 2.4.1.3 小节"。本小节真正的内容（`copy_trampoline` 是唯一调用点、校验只查上界不查账）被推到第二段。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-06 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:70`；问题：- 问题：【重复/冗余】前一段说"靠的是两点：其一，`pg_alloc_page` 从最高下标取页……其二，跳板的生命周期……（本节末"两个取址方向各有其消费者"一段正是这个分工）"；紧接着的"两个取址方向各有其消费者"段又说"真正兜住的只有取页方向与跳板生命周期（见上文"靠的是两点"那一段）"。两段互相把对方当作完整陈述，读者按任一指针都回到另一段。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-07 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:80`；问题：- 问题：【重复/冗余】两处都在声明"不重复"之后立刻重复。§2.4.1.4 先列出"128/64（`free_regions`）/64（`ident`）"三个数字，再说"这里不重复数字"；§4.4 先说"这是 §3.5 删除论证的现在时态，本节不再重述故障链"，而它前半句"字段删除后，'提供一个可误设的回收区间'这个接口面整体消失，险情在类型上不可复发"与 §3.5 的结论逐字相同。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-08 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:90`；问题：- 问题：【事实/算术】同节引文是「test result: ok. 135 passed」，逐项「22 + 11 + 12 + 44 + 17 + 15 + 13 + 1」也等于 135，但正文写「= 136」，并据 136 推出「136 − `opensbi_helpers` 11 = 125」；按 135 算应是 124。默认 uefi-only 形态的文件和（loader 22 + uefi 12 + ebs_map 44 + ebs_pair 17 + dtb_copy 15 + resv_claims 13 + lib.rs 1）也是 124，不是 125。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-09 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:100`；问题：- 问题：【重复/冗余】表前写"这不是纯假设：早期版本曾把 `bootstrap_len` 填成 `kern_phys_base.0`。下表列的是那次误设会扫进的同一区间……"；表后又写"这张表同时解释了字段历史上的另一处险情：早期版本曾把 `bootstrap_len` 填成 `kern_phys_base.0`（把'上界地址'当'区间长度'）……"。"另一处"指的是同一处：两处的值都是 `kern_phys_base.0`，机制都是把上界地址当长度。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-10 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:110`；问题：- 问题：【章节边界混淆】标题是"旧字段台账：`kinfo_t` 的 21 个字段去哪了"，但 21 字段表之后又装了四类与"字段去哪了"无关的内容：两个专用键（`cttyline`/`cttybaud`）的处置、GET_KINFO 载荷收窄的 `[ARCH]` 演进、`EarlyConsole` 与 `KernelConsole` 两个"控制台"的层次辨析、`param_buf` 的十条可观察分歧（十条分歧占本节近三分之一）。其中 `param_buf` 在 §3.4.1 的关系表里属于"改型"，不属于被删的 21 个字段。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-11 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:120`；问题：- 问题：【职责重叠】首段声明"各腿如何产出两张表以 §3.4.1 为权威，本节只讲构造入口"，但字段表的前两行正是逐腿产出方式：`memmap` 行写"UEFI：配对循环最后一次成功 `GetMemoryMap` 分类出的 conventional 列表；U-Boot/riscv 直核：`DtbMemoryMap::new(&fdt, KERN_PHYS_BASE).collect_with_sources()`"；`reserved_regions` 行同样逐腿（UEFI 的 `ReservedMapped`、U-Boot/riscv 的 `KERN_PHYS_BASE` 裁剪、aarch64 的 `fw_end = 0`）。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-12 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:130`；问题：- 问题：【职责重叠/索引错位】四个防线在 §4.3 与 §4.9 被命名为 A/B/C/D，但 §5.1 的机制列没有任何一行以 D 命名：M11 只列 A/B 并说"另有 C 守模块×内核跨度"，M16 讲的是内核跨度与模块×内核，D（模块×模块）只在 M2 的"覆盖状态"长句里出现一次——而 M2 的机制名是"内存清单的建与裁"。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-13 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:140`；问题：- 问题：【章节边界混淆】清单自称穷举（"越过上述边界的内容都属**显式声明的下游交代**"），但 §4.7 的后半明确越过边界：扣除表读法（`build_vm_handoff`）、arena 生命周期合同（原文"VM 自由页清单在 `kmain → build_vm_handoff` 才发布"）、VM 侧的 `LiveBootstrap ∩ VM-free = ∅` 复核，全部发生在控制权交给 `kmain` 之后。清单没有列 §4.7。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-14 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:150`；问题：- 问题：【章节边界混淆/重复】这一条在 C 源码分析章里用 64 位 Rust 的窗口清单与 `BOOT_IDENTITY_MAP_END` 来论证"全铺不是必须"，并打上 `[ARCH: boot 最小恒等窗口；跨镜像腿不再铺低 4GB]`；§4.6 的 Step 1 第一条 bullet 用同一句 ARCH 标注、同一批窗口清单、同一句"`BOOT_IDENTITY_MAP_END` 是 VA 上限"再讲一遍。同一处还用了 LMA，而 §1.4 承诺"本篇从 §3.4 起会直接用 LMA/VMA，首次出现处给指向"——这里没有指向；更早的 §1.4 编号图例（"先读 ELF、预订 LMA 跨度"）也没有指向，承诺在同一节内就被破坏了一次。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-15 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:160`；问题：- 问题：【前提倒置】标题问"让 riscv64 照 aarch64 那样'先建表、再跳镜像'会怎样"，正文回答的是另一件事：riscv64 入口没有 `arena_pa` 入参，照搬协议而不补分道会空指针；末尾又转去"若 aarch64 退回内联执行"。标题里的"先建表、再跳镜像"在正文里没有被分析。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-16 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:170`；问题：- 问题：【重复/冗余导致判据变形】§5.1 M12 与 §5.2.3 表把 aarch64 门定义为"两条固定标记 + 第三个标记的三条变体之一"，§6.4 压缩成"三条标记之一（前两条读文件；第三条是 memmap source 行……）"。后者是一个析取，前者是一个合取，二者不等价。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-17 | reader-b | P1 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:180`；问题：- 问题：【职责重叠】同一条"深拷不变量"的证据被三个节分别持有：§3.4.4 给出两组历史读数（`res=127 zeroed=127` 等）并说"读数与三层判别见 §5.2.3"；§5.2.3 声明"设计章只引用；证据归本节"；§5.1 M4 又把探针读数记成"§3.4 的深拷探针"。三处的所有权声明互相冲突，探针读数的归属在两节之间来回。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-18 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:190`；问题：- 问题：【章节边界混淆】标题列"三种形态：同镜像内调用、跨镜像跳转与直核自引导"，术语对账表也把"形态"计为 3，但正文没有为"同镜像内调用"设任何小节；它只在 §3.7 末尾的"其一，aarch64 退回内联执行"里作为被否决的替代做法出现，且术语表自己注明"同镜像是测试/历史形态，当前生产腿只有跨镜像与直核两类"。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-19 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-b.md:200`；问题：- 问题：【章节边界/自述】四处自述与实际结构不符：(a) 索引把"生产腿"的定义节标成 §3.7，但"腿"一词的定义在 §3.2（"一条腿 = 一个完整启动路径"），§3.7 只给成员表；索引没有区分"词的定义"与"计数表的成员表"；(b) 索引声称"本篇用到的编号不止一套，先给一张索引"，却不收录 A/B/C/D 防线、L0–L9、"六入口"三套编号，而这三套都在正文承担判定；(c) 前置知识说"§1.3 是第一章收束"，§1.3 实际是路由段（"§1.2 已给出……§1.4 给……两者不在这里重述"），实质收束在 §1.2；(d) §2.8 说"`overlaps` 的闭区间差异见 §2.7，不在'只有'之列"，但 §2.7 的原文是"唯一必须按架构另写的**接口层**"，全篇没有"只有"这个引文。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-B-20 | reader-b | P2 | reader-b | ③ | 反证/复核判定不成立或保留取舍，不计缺陷 | `panel/round24/reader-b.md:210`；问题：- 问题：【重复/冗余·低置信高价值】正文携带大量评审批次号与裁决号（`R23 的 F-R17-04`、`R18 之前`、`DESIGN-07`、`Phase A.2`、`R14 已清理`、`本轮删双写测试 −1`、`T2 退役`），编号图例只能事后解释它们"不是源码符号"，不提供含义；目标读者没有评审语境。计数脚手架（§1.4 七行索引 + §3.7 六行对账 + 贯穿 §3.1.1/§3.2/§4.1/§4.4/§4.8/§5.2.3/§6.4 的提醒）为五条生产腿服务，其自述文本量已接近被计数的对象。§3.7 的术语对账表还被一条孤立的 `>` 行切成三段：引用块 + 表 + 空引用 + 引用段，表在渲染上不在引用块内。 | 无新证据；见 panel 原文 |
| READER-C-01 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:11`；问题：C-01 测试总量与逐文件分解对不上 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-02 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:27`；问题：C-02 同一个 TODO 在 §5.1 与 §5.3 状态相反 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-03 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:45`；问题：C-03 §4.2.2 的摘要与自己的五步表相反 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-04 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:60`；问题：C-04 riscv64 的"QEMU 门链"归属在两行之间摇摆 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-05 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:77`；问题：C-05 反事实里 Step 1 的块尺寸前后相反 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-06 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:93`；问题：C-06 用 Multiboot 规范支撑 boot2 路径的"无栈约定" | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-07 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:109`；问题：C-07 用 Rust 的 12 项模块表去数 C 侧的账 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-08 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:124`；问题：C-08 已被标注"待源码核实"的推断，在别处被当作事实引用 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-09 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:139`；问题：C-09 v1 接收面的执行序含一个没有调用点的门 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-10 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:155`；问题：C-10 `head.o` 如何变成 `unpaged_*.o` 的一环没有写 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-11 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:171`；问题：C-11 编号图例把"裁决 A"的对象说成 UEFI，正文说唯一实现是 U-Boot | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-12 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:186`；问题：C-12 `reslen` 与 `res=127 zeroed=127` 对不上 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-13 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:202`；问题：C-13 "不在表内"与"分类落 ReservedMapped"挨在一起 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-14 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:215`；问题：C-14 概述把交接方式留成"按值或按指针"，正文只有指针 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-15 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:230`；问题：C-15 "64 位侧"的三个前提混用了 aarch64 与 x86-64 的机制 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-16 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:246`；问题：C-16 五条生产腿里，U-Boot shim 的 `authority` 值没有写 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-17 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:261`；问题：C-17 "C 的对应物是 pt_free 还回 alloc_mem"与 C 的 6 页静态池对不上 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-18 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:276`；问题：C-18 "启动期唯一会走到 pg_alloc_page"这个唯一性没有取证 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-19 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:291`；问题：C-19 "fast-fail 的削弱"与"C 本来也没有 fast-fail"方向相反 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| READER-C-20 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/reader-c.md:306`；问题：C-20 两个"接线"不是同一件事 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| CLAIM-01 | refute | P0 | refute | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/refute.md:1`；问题：BOOT-SCOPE 2a 之后，自举恒等映射的范围由调用方传入的窗口表决定，跨镜像腿只覆盖 boot 期间真正访问的窗口，窗口外不再铺低 4GB；内核高半映射仍在 Step 2 建立，Direct Map 仍在 Step 4 由内核建立（文档 §4.6 第 2794 行；D-027）。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| CLAIM-02 | refute | P1 | refute | ③ | 反证/复核判定不成立或保留取舍，不计缺陷 | `panel/round24/refute.md:31`；问题：wire v9 删除 `kern_ident` / `kern_ident_count` / `VM_BOOT_HANDOFF_MAX_IDENT` 之后，VM 侧 `KernelLayout::ident_regions()` 恒为空，且现有消费者不会因此受损（文档 §3.4.2 第 1642 行；D-028）。 | 无新证据；见 panel 原文 |
| CLAIM-03 | refute | P1 | refute | ③ | 反证/复核判定不成立或保留取舍，不计缺陷 | `panel/round24/refute.md:57`；问题：bootface 合并之后，riscv64 `bootface` 与 aarch64 `bootface_a64` 两个调用点在池边界、console 回调与模块表上仍然等价，共用 `bootface_common::place_modules` 的同一份放置逻辑，调用方只传 bump 分配器、池边界与 console 回调（文档 §3.2 第 1480 行）。 | 无新证据；见 panel 原文 |
| CLAIM-04 | refute | P0 | refute | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/refute.md:84`；问题：`pt_alloc` 的 `register` / `register_free` 在所有构建档都 fail-closed（二次注册 panic），且这一改动不破坏测试内核与生产入口（D-028；文档 §5.1 M7 第 3005 行、§5.3 TODO-02 第 3164 行）。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| CLAIM-05 | refute | P0 | refute | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/refute.md:110`；问题：x86 的 `PageFlags::DEVICE` 经 `flags_to_pte` 编码成 `PWT\|PCD`，LAPIC/IO-APIC 的设备窗口按这条编码映射，因此不再依赖固件 MTRR；该编码不改变非设备映射（文档 §4.6 第 2799 行、§5.3 TODO-10 第 3172 行）。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| CLAIM-06 | refute | P0 | refute | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/refute.md:138`；问题：R23 的删除与新增已经回填到文档的容量账本与测试计数账本；§4.5 的容量表与 §5.2.1 的逐文件计数、实跑转录可以作为当前值引用（文档第 2743 行、第 3032 行、第 3060 行、第 3080 行；D-028）。 | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-01 | design-d2 | P0 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:21`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-02 | design-d2 | P0 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:34`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-03 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:47`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-04 | design-d2 | P0 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:60`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-05 | design-d2 | P1 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:73`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-06 | design-d2 | P1 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:86`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-07 | design-d2 | P1 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:99`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-08 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:112`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-09 | design-d2 | P1 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:125`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-10 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:138`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-11 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:151`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-12 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:164`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-13 | design-d2 | P2 | design-d2 | ③ | 反证/复核判定不成立或保留取舍，不计缺陷 | `panel/round24/design-d2.md:177`；问题：见 design-d2.md | 无新证据；见 panel 原文 |
| DESIGN-14 | design-d2 | P1 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:190`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-15 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:203`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |
| DESIGN-16 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round24/design-d2.md:216`；问题：见 design-d2.md | doc 五门 PASS；测试/QEMU 见 loop §5 |

## R25 面板轮（2026-11-10，R24 meta PASS 后）

| 发现 ID | 角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-A-01 | fact-a | P1 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:26`；问题：F01（P1）§4.2 的中心引文把 UEFI 腿的 `records_in_wire = true` 写丢，且把 `let mut report` 写成 `let report` | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-A-02 | fact-a | P1 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:40`；问题：F02（P1）§3.7 的 `prepare_kernel_image_v1` 引文漏掉 `assert_boot_windows_addressable` 调用，且未在省略声明中列出 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-A-03 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:53`；问题：F03（P2）§4.4 `memmap` 行把直核面 DTB 来源写成 `a1`，且只覆盖 U-Boot/riscv，缺 aarch64 直核面 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-A-04 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:67`；问题：F04（P2）§4.4 字段表 `platform_sources` 行未列两个直核面 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-A-05 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:78`；问题：F05（P2）§5.1 M5 / §5.3 TODO-05 的测试路径写成 `boot_policy::tests::…`，实际模块是 `boot_policy::policy_tests` | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-A-06 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:89`；问题：F06（P2）§4.5 引文把 5 行的 `BootModule` 结构体字面量压成 1 行，未声明重排 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-A-07 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:100`；问题：F07（P2）§4.7 `resume` 文档注释引文是两句拼接改写，未标〔编注〕/改述 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-A-08 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:111`；问题：F08（P2）§4.4 `boot_validate_and_prepare` 引文插入两行中文注释，非源码内容且未标〔编注〕 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-A-09 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:122`；问题：F09（P2）§2.7 `earm/head.S` 引文删去 `.section .startup` 与 `.global MINIX` 之间的 4 行源码注释，无省略标记 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-A-10 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:133`；问题：F10（P2）§4.4 门表把 `kern_stack_top` 16 字节对齐只挂在 `boot_validate_and_prepare`，共享校验体也查它 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-A-11 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:144`；问题：F11（P2）D-031 的 wire 兼容合同在 §3.4 无落点：`WirePlatformDescSource` 第二个 `u32` 由 `reserved` 改为 `len`、16 B 与版本 9 不变，设计章未载 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-A-12 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:158`；问题：F12（P2）§4.2 引文未声明省略函数开头的 `shim_image` / `shim_stack` 捕获 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-A-13 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:169`；问题：F13（P2）§6.1 把 `step2c device window empty (skip)` 写成“aarch64 直核”专用标记，aarch64 v1 发送侧也走同一分支 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-A-14 | fact-a | P2 | fact-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-a.md:180`；问题：F14（P2）§6.3/TODO-08 把 `end <= BOOT_IDENTITY_MAP_END` 归为“窗口构建断言”，实现位置是 Step 1 映射循环 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-01 | reader-a | P0 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:35`；问题：A01 — §1.2 四框图：阅读顺序、依赖方向、D 编号三套秩序叠在一起，且图上有一条不计入 D1–D5 的箭 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-02 | reader-a | P0 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:56`；问题：A02 — §2.2 栈对齐：空位「让 %esp 在 call 前落在 16 字节边界」与入口 %esp ≡ 0 的结论互相打架 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-03 | reader-a | P0 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:72`；问题：A03 — §3.8 分配器换手：表说 shim 在 Step 0 注册，正文却说 shim「只初始化 bump 池」 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-04 | reader-a | P0 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:90`；问题：A04 — §6.1 诊断表：示例串写 `reslen`，同一行又说「不是 `reslen`」 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-05 | reader-a | P2 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:104`；问题：A05 — §1.2 与 §1.3：D5 是「D3 内部」还是「与 D1 并列」 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-06 | reader-a | P2 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:120`；问题：A06 — §3.7 形态编号：「第三种/第二种形态」与本节标题的列举顺序对不上 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-07 | reader-a | P2 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:140`；问题：A07 — §4.4 门链：同一个编号「4」指两个不同的门，且 `BeforeTables`/`Standard`/`Strict` 未定义 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-08 | reader-a | P2 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:158`；问题：A08 — §5.1 证据账：M2 标「已覆盖」却写「没有同形断言」，M13 标「部分覆盖（可测面已覆盖）」 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-09 | reader-a | P2 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:174`；问题：A09 — §5.1 M4：候选①/②从未被编号，且「TODO-01 已关闭」与「不构成完整覆盖」并置 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-10 | reader-a | P2 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:190`；问题：A10 — §3.4 平台源：`PlatformDescSource` 的形状与 `RSDP` 未交代 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-11 | reader-a | P2 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:206`；问题：A11 — §4.6 窗口：`PLIC`/`ACLINT SSWI` 未定义，两张设备窗口表的关系难跟 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-12 | reader-a | P2 | reader-a | ④ | 已修：§4.2.1 统一“动作 N / 时间序第 N 拍”，正文不再用“第 N 步”兼指两套编号 | `panel/round25/reader-a.md:224`；问题：A12 — §4.2.1 九动作：动作 ID 与时间序是两套编号，正文「第 N 步」只指前者 | `grep -n "第 [0-9]\+ 步" §4.2.1 范围 0 命中`；冷读确认术语不再混用 |
| READER-A-13 | reader-a | P2 | reader-a | ④ | 已修：读者体验项文档改写 | `panel/round25/reader-a.md:238`；问题：A13 — §3.4.1 memmap 行：A+D、`ReservedMapped`、`Side` 全是前向引用 | doc 五门 PASS；见 loop §4 |
| READER-A-14 | reader-a | P2 | reader-a | ④ | 已修：§4.2.2 手算五例改为“例/输入/逐步判定/落点”五列表格 | `panel/round25/reader-a.md:252`；问题：A14 — §4.2.2 描述符分类：五个手算例子挤在一段，读者无法逐例对表 | `doc-structure-check PASS`；冷读确认可逐例对表 |
| READER-A-15 | reader-a | P2 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:266`；问题：A15 — §3.4.4 / §4.5 / §5.2.3 证据链：两跳指针、候选编号缺失，且「三层」与 §5.2 的「三层证据」撞名 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-16 | reader-a | P2 | reader-a | ④ | 已修：§1.4 增设计数约定，§3.7 术语对账表标为全文唯一权威；两个“5”的成员差异在表下明写 | `panel/round25/reader-a.md:286`；问题：A16 — 全篇计数体系：腿/链/形态/交接路径/架构×链/门链六套计数，两个「5」成员不同 | `grep -n "计数约定\|本表是全文启动路径计数的唯一权威" 各 1 命中`；冷读确认跨节可换算 |
| READER-A-17 | reader-a | P2 | reader-a | ④ | 已修：§2.4.1.2 删除前向“条件结论”；§2.4.1.3 改为“三种取值关系 → 四条前提列表 → 结论”的单线顺序 | `panel/round25/reader-a.md:302`；问题：A17 — §2.4.1.2/§2.4.1.3：`alloc_lowest` 的缺陷被拆成三段，「条件结论/四个前提」把线索拉断 | `grep -n "先把四个前提逐条核对" 1 命中`；冷读确认结论在前提之后 |
| READER-A-18 | reader-a | P2 | reader-a | ④ | 已修：读者体验项文档改写 | `panel/round25/reader-a.md:316`；问题：A18 — §2.3 `mb_set_param`：删除扫描的 off-by-one 推演需要读者自己当 CPU | doc 五门 PASS；见 loop §4 |
| READER-A-19 | reader-a | P2 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:330`；问题：A19 — §6.1 三档读法：`kernel: v4`/`step1+2` 是发送侧在 shim 进程里打印的，读者会误以为是接收端内核 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-A-20 | reader-a | P2 | reader-a | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-a.md:344`；问题：A20 — §4.6 `map_huge` 忽略错误：先否定旧理由、再列五个错误变体，结论埋在段末 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-01 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:47`；问题：R01｜§3.4 与 §4.4/§4.5 的三方分工，被 §4.4 自己越过 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-02 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:65`；问题：R02｜§5.1、§5.3、§5.4 三处都在管状态，且各有一套词 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-03 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:81`；问题：R03｜§2.1 与 §2.3 都在讲 `rootdevname` 的读者，且重复同一条 grep 结论 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-04 | reader-b | P2 | reader-b | ④ | 已修：同 A-16；§3.7 术语对账表为唯一权威，§1.4 计数约定给两个“5”的成员差异 | `panel/round25/reader-b.md:93`；问题：R04｜“五条腿/五条门链/三类链/四种交接路径”的计数职责分散在六处 | `grep -n "唯一权威" §3.7 1 命中`；冷读确认六处只引用不重定义 |
| READER-B-05 | reader-b | P2 | reader-b | ④ | 已修：读者体验项文档改写 | `panel/round25/reader-b.md:103`；问题：R05｜§2.6.1 与 §4.6 都在讲 C/Rust 缓存属性差异 | doc 五门 PASS；见 loop §4 |
| READER-B-06 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:115`；问题：R06｜§1.2 的“必须凑齐四样”，到 §2.3 被改成“四个字段没有一个来自引导器” | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-07 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:129`；问题：R07｜§1.3 说 D5“与 D1 并列”，§1.2 说 D5 在 D3 内部 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-08 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:141`；问题：R08｜§3.9 的对照表先给“整个物理地址空间”，下一行才用“资格过滤”收窄 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-09 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:151`；问题：R09｜§3.4.2 先断言“C 的同一动作靠 TLB 窗口撑住”，再声明“不能当作已核实的事实引用” | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-10 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:159`；问题：R10｜§5 开头的 M6 缺口描述与 §5.1/§5.4 不是同一件事 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-11 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:175`；问题：R11｜§4.6 说 x86_64 有“跨镜像与直核”两条路径，§3.7 说 x86_64 无直核形态 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-12 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:191`；问题：R12｜§3.2 说“只有一份”装载逻辑，几段之后才限定“仅两条 boot-shim 腿之间” | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-13 | reader-b | P2 | reader-b | ④ | 已修：读者体验项文档改写 | `panel/round25/reader-b.md:207`；问题：R13｜§2.2 标题说“只做三件事”，正文有三段长旁支 | doc 五门 PASS；见 loop §4 |
| READER-B-14 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:221`；问题：R14｜§2.4 的“结算”宣告框 1 要到 §2.5 才完成，§2.5 却没有对应的完成标记 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-15 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:229`；问题：R15｜§4.2.1 标题说“九个动作”，正文承认有第十个（6b） | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-16 | reader-b | P2 | reader-b | ④ | 已修：静态页表池推演移回 §3.8，§4.9 只留一句指针 | `panel/round25/reader-b.md:245`；问题：R16｜§4.9 表后的“静态页表池”推演属于 §3.8，作者自己承认 | `grep -n "把 C 的静态页表池" §4.9 0 命中、§3.8 1 命中`；冷读确认段归属正确 |
| READER-B-17 | reader-b | P2 | reader-b | ④ | 已修：读者体验项文档改写 | `panel/round25/reader-b.md:253`；问题：R17｜§4.6 的 Step 4 被声明不展开，但 §3.7 引用的 `resume_additive` 文档里就写着 “establish the direct map” | doc 五门 PASS；见 loop §4 |
| READER-B-18 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:265`；问题：R18｜§4.5 标题说“七个落地区”，表里有八行 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-19 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:281`；问题：R19｜§1.2 的依赖序在相邻两句里说了两遍，D→节映射又重复表列 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-20 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:291`；问题：R20｜§2.1 与 §2.3 的 `grep -rn rootdevname` 结论重复 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-21 | reader-b | P2 | reader-b | ④ | 已修：读者体验项文档改写 | `panel/round25/reader-b.md:295`；问题：R21｜§3.9 的“三个前提”在同一段里出现两遍 | doc 五门 PASS；见 loop §4 |
| READER-B-22 | reader-b | P2 | reader-b | ④ | 已修：读者体验项文档改写 | `panel/round25/reader-b.md:303`；问题：R22｜§2.4.1.4、§3.4.1、§4.5 三处重复“64/47/v9”这组容量事实 | doc 五门 PASS；见 loop §4 |
| READER-B-23 | reader-b | P2 | reader-b | ④ | 已修：读者体验项文档改写 | `panel/round25/reader-b.md:319`；问题：R23｜§5.1 与 §5.2.3 重复 aarch64 链的三条通过标记 | doc 五门 PASS；见 loop §4 |
| READER-B-24 | reader-b | P2 | reader-b | ④ | 已修：§4.6 Step 2 守卫只留一处因果推演，正文段改为指针 | `panel/round25/reader-b.md:331`；问题：R24｜§4.6 的 Step 2 守卫理由在同一节出现两次 | `grep -n "同一个 VPN\[2\] 槽位第二次" §4.6 1 命中`；冷读确认无第二份完整因果 |
| READER-B-25 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:347`；问题：R25｜D4 的读序指针指向 §2.2，而 §2.2 明确把该行推给 02 篇 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-26 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:365`；问题：R26｜§4.4 的“层内收敛原则（§3.4 开头）”在 §3.4 开头不存在 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-27 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:377`；问题：R27｜§4.5 说“三层证据判别统一见 §3.4.4”，§3.4.4 把三层判别交给 §5.2.3 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-28 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:393`；问题：R28｜§4.8 对 §3.7 的跳转覆盖有两句互相矛盾的话 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-29 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:409`；问题：R29｜§4.4 的“门列有五个标签”与表中六个非规划期标签不符 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-30 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:417`；问题：R30｜§2.4 的“框 1 要到 §2.5 才完成”在 §2.5 没有接应 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-B-31 | reader-b | P2 | reader-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-b.md:421`；问题：R31｜§1.4 的编号索引把 M1–M17 的“主要消费者”写成 §5.3，实际定义与状态都在 §5.1 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-01 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:7`；问题：C25-01 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-02 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:18`；问题：C25-02 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-03 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:29`；问题：C25-03 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-04 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:40`；问题：C25-04 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-05 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:51`；问题：C25-05 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-06 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:62`；问题：C25-06 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-07 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:73`；问题：C25-07 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-08 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:84`；问题：C25-08 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-09 | reader-c | P1 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:95`；问题：C25-09 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-10 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:106`；问题：C25-10 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-11 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:117`；问题：C25-11 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-12 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:128`；问题：C25-12 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-13 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:139`；问题：C25-13 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-14 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:150`；问题：C25-14 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-15 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:161`；问题：C25-15 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-16 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:172`；问题：C25-16 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-17 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:183`；问题：C25-17 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-18 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:194`；问题：C25-18 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-19 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:205`；问题：C25-19 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| READER-C-20 | reader-c | P2 | reader-c | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/reader-c.md:216`；问题：C25-20 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| CLAIM-01 | refute | P1 | refute | FP | 反证成立，不是缺陷 | 反证：`panel/round25/refute.md:5`；平台描述符窗口现在覆盖 blob 全长——`PlatformDescSource.len` 经 wire `off+4` 传递，`push_platform_windows` 按跨度建窗；旧生产者 `len == 0` 回退到地址所在的一个 2 MiB 片。；源码/文档原文对读 | 不适用 |
| CLAIM-02 | refute | P1 | refute | FP | 反证成立，不是缺陷 | 反证：`panel/round25/refute.md:30`；Step 0 不再要求平台覆盖全局 4 GiB，而是逐窗比较 `end <= phys_addr_limit`；并且窗口错误不比旧检查更晚暴露。；源码/文档原文对读 | 不适用 |
| CLAIM-03 | refute | P1 | refute | ① | `records_in_wire` 的“能防误用”是过度声明；文档已改咨询标记（真缺陷已修） | `panel/round25/refute.md:54`；doc §3.4.5 / D-034 | doc 五门 PASS |
| CLAIM-04 | refute | P1 | refute | FP | 反证成立，不是缺陷 | 反证：`panel/round25/refute.md:78`；`ident_regions` 删除后，`map_kernel` 的段 4 消失没有留下其他空通道。；源码/文档原文对读 | 不适用 |
| CLAIM-05 | refute | P1 | refute | FP | 反证成立，不是缺陷 | 反证：`panel/round25/refute.md:101`；`device_window_flags()` 的 `DEVICE` 已接入 Step 2c 与 `map_kernel` 段 5；riscv 没有 cache 位不构成语义倒退。；源码/文档原文对读 | 不适用 |
| CLAIM-06 | refute | P1 | refute | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/refute.md:125`；问题：文档 §4.6、§4.4、§3.7 的引文与当前源码同形。 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| DESIGN-01 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/design-d2.md:45`；问题：- 位置：`os/libs/minix-boot/src/platform.rs:110-168`；生产调用点 `os/boot-shim/src/uefi_helpers.rs:221,238,251`、`os/kernel-image/src/bootface.rs:321`、`os/kernel-image/src/bootface_a64.rs:302`、`os/boot-shim/src/opensbi_helpers.rs:419`；文档 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:2839` | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| DESIGN-02 | design-d2 | P2 | design-d2 | ⑥ | 已实施：新增 `CheckedWindows<'a>`，唯一构造器是 `BootWindows::check(phys_addr_limit)`；`build_bootstrap_root_and_enable` 只收该 token，两处生产调用点已改为 check→build | `panel/round25/design-d2.md:56`；问题：- 位置：`os/kernel/src/lib.rs:1064-1103`（`BootWindows`）、`:1186-1193`（`assert_boot_windows_addressable`）、`:496` 与 `:975`（两处生产调用点）、`:1195-1199`（`build_bootstrap_root_and_enable` 签名） | `cargo check -p minix-kernel --features mock` 通过；`boot_windows_accept_ranges_below_the_phys_limit` 与两条 `should_panic` 测试通过；QEMU aarch64 SMP / riscv64 bootface PASS |
| DESIGN-03 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/design-d2.md:67`；问题：- 位置：`os/libs/minix-boot/src/boot_storage.rs:361-386`；置位点 `os/boot-shim/src/uefi_helpers.rs:1184` | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| DESIGN-04 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/design-d2.md:78`；问题：- 位置：`os/libs/minix-boot/src/kernel_info.rs:185-212`、`:236-264` | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| DESIGN-05 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/design-d2.md:89`；问题：- 位置：`os/arch/src/arch/paging.rs:654`（"see segment 4 below"）与 `:697-701`（声明 segment 4 已退役） | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| DESIGN-06 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/design-d2.md:100`；问题：- 位置：`os/libs/minix-types/src/types/boot.rs:455-458`、`:490-496`；同类残留 `os/servers/vm/src/vm_server.rs:744-752`（"Open item (boot-info integration): ... parsed from the multiboot2 / stivale2 boot headers"） | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| DESIGN-07 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/design-d2.md:111`；问题：- 位置：`os/libs/minix-boot/src/kernel_info.rs:130-134` | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| DESIGN-08 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/design-d2.md:122`；问题：- 位置：`os/libs/minix-boot/src/boot_storage.rs:320-342`（`report()`）、`:361-386`（`BootReport`）；`os/libs/minix-boot/src/handoff_v1_write.rs:138-152`（`write_envelope_report`）；`os/libs/minix-boot/src/kernel_info.rs:445-468`（接收侧 Strict 等式） | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| DESIGN-09 | design-d2 | P2 | design-d2 | ⑥ | 已实施：两个 bootface 的 `MODULE_TABLE`/`PLATFORM_SOURCE`/`STARTUP_OBJECTS`/`param_landing` 合并为一个 `static mut BootStorage` + 单一 one-shot 守卫；`.bss` 上限在 `bootface_common` 编译期断言（≤16 KiB） | `panel/round25/design-d2.md:133`；问题：- 位置：`os/kernel-image/src/bootface.rs:311,335,377-420,459,498`；`os/kernel-image/src/bootface_a64.rs:137,140,304,315,353-360`；`os/kernel-image/src/bootface_common.rs:268-282`；对照 `os/libs/minix-boot/src/boot_storage.rs:61-343` | `cargo build -p kernel-image --target riscv64gc-unknown-none-elf --features fw-riscv64-none --release` 与 aarch64 同形构建通过；`test-kernel-image-riscv64-bootface` PASS；`test-smp-aps-aarch64` PASS |
| DESIGN-10 | design-d2 | P2 | design-d2 | ⑥ | 已实施：`BootWindows.ranges` 由 `[(u64,u64); 32]` 改为 `[PhysRange; 32]`，构造器拒绝反向区间；地址上限检查改为消费 `PhysRange` 的 start/end | `panel/round25/design-d2.md:144`；问题：- 位置：`os/kernel/src/lib.rs:1064-1103`、`:1186-1199`；`os/libs/minix-types/src/types/address.rs:196-210`（`PhysRange`） | `cargo check -p minix-kernel --features mock` 通过；`boot_window_push_2m_covers_the_descriptor_start_piece`、`platform_window_uses_the_recorded_blob_length` 通过；QEMU 两腿 PASS |
| DESIGN-11 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/design-d2.md:155`；问题：- 位置：`os/arch/src/arch/paging.rs:612-638`（`map_kernel`）；`os/libs/minix-types/src/types/boot.rs:480-488`（`KernelLayout`）；生产调用点在 `os/servers/vm/src/vmproc/vmproc_handle.rs:386-394`（不在 related-code 清单内，仅作为事实注记，不作为证据基础） | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| DESIGN-12 | design-d2 | P2 | design-d2 | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/design-d2.md:166`；问题：- 位置：`os/libs/minix-boot/src/boot_storage.rs:30,70`；`os/libs/minix-boot/src/boot_params.rs:27`；`os/libs/minix-boot/src/handoff_v1.rs:379`；`os/kernel/src/globals.rs:383-386` | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| DESIGN-13 | design-d2 | P2 | design-d2 | FP | 反证成立，不是缺陷 | 反证：`panel/round25/design-d2.md:177`；- 位置：文档 §4.4 校验门表，`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:2678`；代码 `os/libs/minix-boot/src/kernel_info.rs:445-530`；源码/文档原文对读 | 不适用 |
| DESIGN-14 | design-d2 | P2 | design-d2 | FP | 反证成立，不是缺陷 | 反证：`panel/round25/design-d2.md:188`；- 位置：`os/libs/minix-boot/src/boot_storage.rs:61-343`；源码/文档原文对读 | 不适用 |
| DESIGN-15 | design-d2 | P2 | design-d2 | FP | 反证成立，不是缺陷 | 反证：`panel/round25/design-d2.md:199`；- 位置：`os/boot-shim/src/ebs_map.rs:799-881`（`ClassificationPlan::commit`）；`os/boot-shim/src/ebs_pair.rs:121-130`（`pair_and_classify` 签名）；文档 §3.4.3/§3.4.5；源码/文档原文对读 | 不适用 |
| DESIGN-16 | design-d2 | P2 | design-d2 | FP | 反证成立，不是缺陷 | 反证：`panel/round25/design-d2.md:210`；- 位置：`os/kernel-image/src/bootface_common.rs:143-166`（`PlacementPool`）；调用点 `os/kernel-image/src/bootface.rs:291-300`、`os/kernel-image/src/bootface_a64.rs:274-283`；源码/文档原文对读 | 不适用 |
| FACT-B-01 | fact-b | P0 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:15`；问题： | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-02 | fact-b | P0 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:25`；问题： | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-03 | fact-b | P0 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:35`；问题： | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-04 | fact-b | P0 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:48`；问题： | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-05 | fact-b | P0 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:61`；问题： | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-06 | fact-b | P0 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:71`；问题： | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-07 | fact-b | P1 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:81`；问题：见该行 P0/P1/P2 条目 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-08 | fact-b | P1 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:91`；问题：见该行 P0/P1/P2 条目 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-09 | fact-b | P1 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:101`；问题：见该行 P0/P1/P2 条目 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-10 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:111`；问题：见该行 P0/P1/P2 条目 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-11 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:121`；问题：见该行 P0/P1/P2 条目 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-12 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:131`；问题：见该行 P0/P1/P2 条目 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-13 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:141`；问题：见该行 P0/P1/P2 条目 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-14 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:151`；问题：见该行 P0/P1/P2 条目 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-15 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:161`；问题：见该行 P0/P1/P2 条目 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-16 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:171`；问题：见该行 P0/P1/P2 条目 | doc 五门 PASS；宿主/QEMU 见 loop §5 |
| FACT-B-17 | fact-b | P2 | fact-b | ① | 按面板最小修复落地（文档/代码/测试） | `panel/round25/fact-b.md:181`；问题：见该行 P0/P1/P2 条目 | doc 五门 PASS；宿主/QEMU 见 loop §5 |

## R26 面板轮（2026-11-10，V3 首轮完整 review）

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R26-01 | FACT-B-01、CLAIM-06 | R26 | P-FACT-B | P0 | 文档-代码同步 | ① | 已修：§3.6/§3.7/§4.4 五处旧 API 改为 BootWindows::check -> CheckedWindows；§3.7 块改为 check 返回 token 再建根；引文标注压成单行 | `rg -n "assert_boot_windows_addressable" os` 零定义零调用；源码 `os/kernel/src/lib.rs:1195-1216` 是 check/CheckedWindows | `rg -n "assert_boot_windows_addressable" 文档 0 命中；doc-citation-check checked=28 failed=0`；冷读 CLOSED块可编译 |
| RD-R26-02 | FACT-A-01 | R26 | P-FACT-A | P1 | 事实/结构 | ① | 已修：`KinfoStruct` 改为“七个语义字段 + 4 字节 _padding（48 B）” | `os/libs/minix-types/src/types/kinfo.rs` 8 个 pub 字段；`test_kinfo_layout_frozen` 断言 48 B | `grep -n "七个语义字段" 1 命中；冷读复算 44+4=48` |
| RD-R26-03 | FACT-B-02、READER-C-11 | R26 | P-FACT-B | P2 | 事实/跨节一致 | ① | 已修：栈名表 `k_initial_stktop` 的归属改为 `mpx.S`（head.S 引用） | `mpx.S:634 LABEL(__k_unpaged_k_initial_stktop)`；`head.S:84` 只引用 | `grep -n "C 的 .mpx.S.（.head.S. 引用）" 1 命中；冷读 CLOSED表与命名陷阱一致` |
| RD-R26-04 | FACT-B-03 | R26 | P-FACT-B | P3 | 事实/索引 | ① | 已修：§1.4 的 LMA/VMA 索引句改为“从 §1.4 索引/图例起就用；首次出现给指向” | 文档 119/145/148 行 | `grep -n "从 §1.4 的索引/图例起" 1 命中；冷读 CLOSED与 145 行一致` |
| RD-R26-05 | FACT-B-04 | R26 | P-FACT-B | P3 | 教学图 | ① | 已修：§2.3 字节图加下标行，第二个脱字符对齐 `=`（第 6 列） | `pre_init.c:384-396` 的 `while (*p++)` 推演 | `grep -n "下标 . 0 . 1 . 2" 1 命中；冷读按表复算 p=6` |
| RD-R26-06 | READER-A-01 | R26 | reader-a | P1 | 结构/因果 | ④ | 已修：§1.2 把跨框依赖移出箭头区，执行序写成显式一句，删“其中 D5” | §1.2 图注/段 | `grep -n "执行序：框 2" 1 命中；冷读 CLOSED可按图复述执行序` |
| RD-R26-07 | READER-A-02、READER-C-14 | R26 | reader-a | P2 | 指针/引文 | ④ | 已修：§1.3 删“与 D1 并列”残引，改为 D5 与 D1–D4 条目名平级 | 全篇检索旧短语 | `grep -n "与 D1 并列" 0 命中；冷读 CLOSED不再找不存在表述` |
| RD-R26-08 | READER-A-03 | R26 | reader-a | P2 | 指针/重复 | ④ | 已修：§4.6 的“第五条/第六条”改成具名指针；删除第三条里的重复注释括注 | §4.6 六条圆点 | `grep -n "见第六条" 0 命中；冷读 CLOSED只在一处纠正` |
| RD-R26-09 | READER-A-04、READER-B-05、READER-C-15 | R26 | reader-a | P2 | 过程词/编号 | ④ | 已修：正文“本轮”改为时态中性；§1.4 图例覆盖 F-R/DESIGN/D/D2/BOOT-SCOPE/Phase | 文档 2278/2880/3167/3169/3255 | `grep -n "本轮" 1 命中（“轮不到”非过程词）；冷读 CLOSED过程词可跳过` |
| RD-R26-10 | READER-A-05、READER-B-06、READER-C-08 | R26 | reader-a | P1 | 指针 | ④ | 已修：M4 原始读数改指 §5.2.3；M2 分工就地写清；PM 篇标“尚未生成”；TODO-07 补五案；push %eax 补 head.S:84 | §5.1/§2.3/§5.3 | `grep -n "原始读数在 §5.2.3" 1 命中；冷读 CLOSED三处指针可落` |
| RD-R26-11 | READER-A-06 | R26 | reader-a | P2 | 前提 | ④ | 已修：§2.4.1.1 槽位表改成“恰有一个更早空洞 h”，并注明第二个空洞落 h2 | `add_memmap` 循环 | `grep -n "恰有一个更早空洞" 1 命中；冷读 CLOSED双空洞情形` |
| RD-R26-12 | READER-A-07 | R26 | reader-a | P2 | 容量/重复 | ④ | 已修：§2.4.1.4 只留 §4.5 容量指针；§3.4.1 注明切片无容量、128/64/47 权威在 §4.5 | §4.5 表 | `grep -n "容量口径统一见 §4.5" 1 命中；冷读 CLOSED不再把 128 挂切片` |
| RD-R26-13 | READER-A-08 | R26 | reader-a | P2 | 术语 | ④ | 已修：§3.2 首次出现处定义“OpenSBI 路径”，并限定 §4.4 特指 U-Boot shim 腿 | §3.2/§4.4 | `grep -n "本文用“OpenSBI 路径”" 1 命中；冷读 CLOSED不把直核面算入` |
| RD-R26-14 | READER-A-09 | R26 | reader-a | P3 | 引文标记 | ④ | 已修：`multiboot_init` 块标注改为“节选，止于 call pre_init” | 块首注释 | `grep -n "节选，止于 `call pre_init`" 1 命中；冷读 CLOSED不再当整函数` |
| RD-R26-15 | READER-B-01 | R26 | reader-b | P2 | 索引/导语 | ④ | 已修：§1.4 改“细节在定义节，本表只给索引”；§3 导语把 §3.10 改为“事实→设计选读表” | §1.4/§3 导语 | `grep -n "事实→设计的选读对照表" 1 命中；冷读 CLOSED索引与内容一致` |
| RD-R26-16 | READER-B-02 | R26 | reader-b | P1 | 执行序 | ④ | 已修：§4.2.1 时间序插入 6b，并注明 6b 不占时间拍 | §4.2.1 表/说明 | `grep -n "6 → 6b → 7" 1 命中；冷读 CLOSED十拍映射` |
| RD-R26-17 | READER-B-03、READER-C-09、READER-C-10 | R26 | reader-b | P1 | 术语/计数 | ④ | 已修：§3.7 增“生产腿判据”（主线 3 + 辅助/测试 2）；§3.2/§4.3 的“两条 boot-shim 腿”改为实现族/三条链 | §3.7 表/§3.2/§4.3 | `grep -n "生产腿”的判据" 1 命中；冷读 CLOSED五腿=3+2` |
| RD-R26-18 | READER-B-04 | R26 | reader-b | P2 | 重复定义 | ④ | 已修：§3.4.2 声明字段名以 §3.4.1 为准；A+D 指向 §4.2.1；§5.3 交接状态列改名“历史交接记录” | §3.4.1/§3.4.2/§5.3 | `grep -n "历史交接记录（不是状态权威）" 1 命中；冷读 CLOSED重复降为指针` |
| RD-R26-19 | READER-B-07 | R26 | reader-b | P2 | 引文标记 | ④ | 已修：§2.7 ARM32 overlaps 块加“节选”头并说明签名/结尾略 | 块首注释 | `grep -n "ARM 的 .overlaps. 节选" 1 命中；冷读 CLOSED不再当完整函数` |
| RD-R26-20 | READER-B-08 | R26 | reader-b | P1 | 条件结论 | ④ | 已修：§1.1 收窄为“Multiboot 合规由规范保证；boot2 契约未取证”；§4.6 的“可以忽略”改为“只改变故障时点，不是安全保证” | §1.1/§4.6 | `grep -n "boot2 的交棒契约不在本篇取证范围" 1 命中；冷读 CLOSED条件未删` |
| RD-R26-21 | READER-B-09 | R26 | reader-b | P2 | 证据强度 | ④ | 已修：候选①已排除/候选②未排除；127 条改“真机记录（无归档日志）”；前缀匹配改为同一 off-by-one 的第二种表现 | §3.4.4/§4.5/§2.3 | `grep -n "候选②仍未排除" 1 命中；冷读 CLOSED三层判别` |
| RD-R26-22 | READER-B-10 | R26 | reader-b | P3 | 标题 | ④ | 已修：§2.4/§3.2/§3.4/§5.2.3 标题改宽，覆盖节内实际内容 | 四个标题 | `grep -n "内存账：条目的来源" 1 命中；冷读 CLOSED标题与内容一致` |
| RD-R26-23 | READER-B-11 | R26 | reader-b | P2 | 术语 | ④ | 已修：§3.4.1 “驱逐”加“撤销 4 KiB 映射”；§4.7 “poison”加“写哨兵值模拟复用” | §3.4.1/§4.7 | `grep -n "写入哨兵值，模拟 arena 被复用" 1 命中；冷读 CLOSED两词可复述` |
| RD-R26-24 | READER-B-12 | R26 | reader-b | P2 | 总结 | ④ | 已修：§2.8 补 D2/D3 各一句陈述+证据锚点，五条均可从总结复述 | §2.8 bullets | `grep -n "内核位置先于清单裁剪" 3 命中；冷读 CLOSED 五条自足` |
| RD-R26-25 | READER-C-01 | R26 | reader-c | P1 | 压缩断言 | ④ | 已修：§3.10 改为“没有可单独界定、分页后不再需要的段（bootface 分页前执行但无独立链接边界）” | §3.5/§3.10 | `grep -n "没有可单独界定、分页后不再需要的段" 1 命中；冷读 CLOSED与 §3.5 同义` |
| RD-R26-26 | READER-C-02 | R26 | reader-c | P1 | 前提 | ④ | 已修：§4.6 补“只有固件未置 SMEP/SMAP 时才无实效” | §2.6.1 | `grep -n "只有在固件未置 SMEP/SMAP 的前提下" 1 命中；冷读 CLOSED 条件回指` |
| RD-R26-27 | READER-C-03 | R26 | reader-c | P1 | 跨架构矛盾 | ④ | 已修：§3.9 的 ② 改为三架构分别说明（TTBR1/高半继承/satp 整根且进程根携带内核半区），删“内核根不随进程切换”的无条件说法 | §3.9 2204/2206 | `grep -n "每张进程根都携带内核半区" 1 命中；冷读 CLOSED satp 不再矛盾` |
| RD-R26-28 | READER-C-04 | R26 | reader-c | P2 | 证据边界 | ④ | 已修：§2.8 拆清 add_memmap 的 assert 覆盖与 cut_memmap 的调用序保证，不把两者混算 | §2.8 D1 | `grep -n "这条 .assert. 只覆盖 .add_memmap. 这一半" 1 命中；冷读 CLOSED两半分开` |
| RD-R26-29 | READER-C-05 | R26 | reader-c | P2 | 反事实 | ④ | 已修：§4.6 Step 2 反事实改为“1 GiB 叶仍恒等，镜像不丢；DEVICE 分片/窄窗口被粗粒度覆盖” | §4.6 2933 | `grep -n "镜像本身仍是恒等映射、不会丢失" 1 命中；冷读 CLOSED症状与前提一致` |
| RD-R26-30 | READER-C-06 | R26 | reader-c | P1 | 证据强度 | ④ | 已修：§3.5 过回收表加“OpenSBI 行有地址锚点，其余三行是当时观察、地址未记录”，并加诚实边界段 | §3.5 表/段 | `grep -n "不要把它们当成可复算的地址证据" 1 命中；冷读 CLOSED三行不再装作有地址` |
| RD-R26-31 | READER-C-07 | R26 | reader-c | P1 | 引文归属 | ④ | 已修：§3.5 引文归属改为“早期 build_kernel_info 调用点（riscv64 链）”，x86_64 区间另标 | §3.5 1821 | `grep -n "早期 .build_kernel_info. 的调用点（riscv64 链）" 1 命中；冷读 CLOSED对象/出处一致` |
| RD-R26-32 | READER-C-12 | R26 | reader-c | P2 | 路标 | ④ | 已修：§6.1 表写明 aarch64 接收壳打印两行、x86/riscv 传空串不打印；step1+2 与 step0/step4 分开归属 | §6.1 表 | `grep -n "x86_64/riscv64 的 .resume_after_v1_decode. 给" 1 命中；冷读 CLOSED打印者可数` |
| RD-R26-33 | READER-C-13 | R26 | reader-c | P3 | 指针方向 | ④ | 已修：§4.6 “上一条反向推演”改为“本节末‘若恒等映射只覆盖可用内存’那条” | §4.6 2928 | `grep -n "恰好是本节末“若恒等映射只覆盖可用内存”那条" 1 命中；冷读 CLOSED 前向指针` |
| RD-R26-34 | CLAIM-01 | R26 | refute | P2 | 反证/安全 | ① | 已修：CLAIM-01 主张成立；附带把 store_kernel_info 的模块名拷贝从 copy_from_slice 改为 ptr::copy（重叠安全） | `os/kernel/src/lib.rs` 的 `ptr::copy`；`cargo test -p minix-kernel --features mock` | `cargo test -p minix-kernel --features mock` 865+8 通过；冷读 CLOSED注释含 SAFETY` |
| RD-R26-35 | CLAIM-02、CLAIM-03 | R26 | refute | P0 | 设计合同 | ① | 已修：`.design/01-design.v3.md` 加 R26 取代节（七项旧条款作废，文档/代码为准） | 设计快照 R26 节；文档 §3.2/§3.5/§3.6/§3.8/§3.9/§4.6 | `grep -n "R26 裁决" .design/01-design.v3.md 1 命中；冷读 CLOSED七项逐条有证据` |
| RD-R26-36 | CLAIM-04 | R26 | refute | P2 | 因果链 | ① | 已修：§3.8 反向推演改为“账本边界”理由，删“这一页刚被交给 VM”的不成立因果 | §3.8 2203 | `grep -n "真正的问题是..账本边界" 1 命中；冷读 CLOSED因果自洽` |
| RD-R26-37 | CLAIM-05 | R26 | refute | P3 | 反证失败 | FP | 反证成立，不是缺陷：Step 1 忽略错误的前提在当前三架构成立 | `paging.rs` 的 supports_huge_page/错误集；文档 2888；源码/测试反证 | 不适用 |
| RD-R26-38 | DESIGN-D2-01 | R26 | design-d2 | P1 | 设计/类型安全 | ② | ② 用户批准 2026-11-10（代理裁决，D-043；OPEN-DEBT R26-design-batch）；去向 R27 design/code batch：build_kernel_info 收 struct | `os/boot-shim/src/lib.rs:101-112` 11 参数；两对同型可互换 | 不适用 |
| RD-R26-39 | DESIGN-D2-02 | R26 | design-d2 | P1 | 设计/wire | ② | ② 用户批准 2026-11-10（代理裁决，D-043；OPEN-DEBT R26-design-batch）；去向 R27 wire v10 batch：删 user_sp/kern_stack_top/syscall_entry 派生字段 | `kernel_info.rs:453-470` 与 wire 字段 | 不适用 |
| RD-R26-40 | DESIGN-D2-03 | R26 | design-d2 | P1 | 设计/类型 | ② | ② 用户批准 2026-11-10（代理裁决，D-043；OPEN-DEBT R26-design-batch）；去向 R27 design batch：拆 RamAuthority，去掉 StartupObjects 不可能变体 | `memmap_source.rs:589-617` 白名单 | 不适用 |
| RD-R26-41 | DESIGN-D2-04 | R26 | design-d2 | P1 | 设计/死抽象 | ② | ② 用户批准 2026-11-10（代理裁决，D-043；OPEN-DEBT R26-design-batch）；去向 R27 design batch：清理死抽象与容量模型 | `boot_storage.rs:24-26,373-382` 等 | 不适用 |
| RD-R26-42 | DESIGN-D2-05 | R26 | design-d2 | P1 | 设计/重构 | ② | ② 用户批准 2026-11-10（代理裁决，D-043；OPEN-DEBT R26-design-batch）；去向 R27 refactor batch：两个 bootface 约 300 行同构流程壳，分两步 | `bootface.rs`/`bootface_a64.rs` 入口体 | 不适用 |
| RD-R26-43 | DESIGN-D2-06 | R26 | design-d2 | P2 | 设计/类型 | ⑥ | 已实施：store_kernel_info 返回 &'static KernelInfo，生产调用点去掉 expect | `os/kernel/src/lib.rs` 返回类型与两个调用点；cargo test | `cargo test -p minix-kernel --features mock` 通过；冷读 CLOSED生产调用点无 expect` |
| RD-R26-44 | DESIGN-D2-07 | R26 | design-d2 | P2 | 设计/哨兵 | ② | ② 用户批准 2026-11-10（代理裁决，D-043；OPEN-DEBT R26-design-batch）；去向 R27 design batch：adjudicate 的哨兵返回值改 enum | `ebs_map.rs:594` Disposition | 不适用 |
| RD-R26-45 | DESIGN-D2-08 | R26 | design-d2 | P2 | 设计/状态协议 | ② | ② 用户批准 2026-11-10（代理裁决，D-043；OPEN-DEBT R26-design-batch）；去向 R27 design batch：统一 BootStorage 的 set/push 命名 | `boot_storage.rs:112-273` 方法 | 不适用 |
| RD-R26-46 | DESIGN-D2-09 | R26 | design-d2 | P2 | 设计/重复实现 | ② | ② 用户批准 2026-11-10（代理裁决，D-043；OPEN-DEBT R26-design-batch）；去向 R27 design batch：同一概念五种实现的合并评估 | `pt_alloc.rs`/`BootStorageCell`/`KERNEL_INFO` | 不适用 |
| RD-R26-47 | DESIGN-D2-10 | R26 | design-d2 | P2 | 设计/身份 | ② | ② 用户批准 2026-11-10（代理裁决，D-043；OPEN-DEBT R26-design-batch）；去向 R27 design batch：shim_windows 身份类型化 | `lib.rs:431-436`/`:1069-1095` | 不适用 |
| RD-R26-48 | DESIGN-D2-11 | R26 | design-d2 | P2 | 设计/校验 | ② | ② 用户批准 2026-11-10（代理裁决，D-043；OPEN-DEBT R26-design-batch）；去向 R27 design batch：boot_alloc init/resume 与 pt_alloc free 的哨兵/不对称校验 | `boot_alloc.rs:54-82`/`pt_alloc.rs:71-176` | 不适用 |
| RD-R26-49 | DESIGN-D2-12 | R26 | design-d2 | P2 | 设计/API 面 | ② | ② 用户批准 2026-11-10（代理裁决，D-043；OPEN-DEBT R26-design-batch）；去向 R27 design batch：为零消费者保留的 API 面清理 | `handoff_v1_write.rs:160-169`/`kernel_info.rs:537-546` | 不适用 |

## R27 面板轮（2026-11-10，V3.0 第二轮完整 review；R29 后回填）

> R27 的 70 条此前只在 `panel/round27/*.md` 与 `EDITION-RETRO.md` 有记录；本节在 R29 后回填。
> 发现 ID 加 `RD27-` 前缀以便跨轮唯一；R28/R29 已覆盖的项用 `dup-of: RD-R28-xx` 指向结构轮根因。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R27-01 | RD27-DESIGN-D2-13 | R27 | reader | P2 | 历史发现 | ④ | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-DESIGN-D2-13） | panel/round27/design-d2.md:53 | grep -n "RD27-DESIGN-D2-13" panel/round27/design-d2.md；同 agent 冷读 CLOSED（RD27-DESIGN-D2-13） |
| RD-R27-02 | RD27-DESIGN-D2-14 | R27 | reader | P2 | 历史发现 | ④ | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-DESIGN-D2-14） | panel/round27/design-d2.md:80 | grep -n "RD27-DESIGN-D2-14" panel/round27/design-d2.md；同 agent 冷读 CLOSED（RD27-DESIGN-D2-14） |
| RD-R27-03 | RD27-DESIGN-D2-15 | R27 | reader | P2 | 历史发现 | ④ | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-DESIGN-D2-15） | panel/round27/design-d2.md:90 | grep -n "RD27-DESIGN-D2-15" panel/round27/design-d2.md；同 agent 冷读 CLOSED（RD27-DESIGN-D2-15） |
| RD-R27-04 | RD27-DESIGN-D2-16 | R27 | reader | P2 | 历史发现 | ⑥ | dup-of: RD-R28-29（R27 backlog，R28 结构重写关闭；RD27-DESIGN-D2-16） | panel/round27/design-d2.md:104 | grep -n "RD27-DESIGN-D2-16" panel/round27/design-d2.md；同 agent 冷读 CLOSED（RD27-DESIGN-D2-16） |
| RD-R27-05 | RD27-DESIGN-D2-17 | R27 | reader | P2 | 历史发现 | ⑥ | dup-of: RD-R28-30（R27 backlog，R28 结构重写关闭；RD27-DESIGN-D2-17） | panel/round27/design-d2.md:114 | grep -n "RD27-DESIGN-D2-17" panel/round27/design-d2.md；同 agent 冷读 CLOSED（RD27-DESIGN-D2-17） |
| RD-R27-06 | RD27-DESIGN-D2-18 | R27 | reader | P2 | 历史发现 | ⑥ | dup-of: RD-R28-31（R27 backlog，R28 结构重写关闭；RD27-DESIGN-D2-18） | panel/round27/design-d2.md:124 | grep -n "RD27-DESIGN-D2-18" panel/round27/design-d2.md；同 agent 冷读 CLOSED（RD27-DESIGN-D2-18） |
| RD-R27-07 | RD27-DESIGN-D2-19 | R27 | reader | P2 | 历史发现 | ⑥ | dup-of: RD-R28-32（R27 backlog，R28 结构重写关闭；RD27-DESIGN-D2-19） | panel/round27/design-d2.md:134 | grep -n "RD27-DESIGN-D2-19" panel/round27/design-d2.md；同 agent 冷读 CLOSED（RD27-DESIGN-D2-19） |
| RD-R27-08 | RD27-FACT-A-01 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-A-01） | panel/round27/fact-a.md:39 | grep -n "RD27-FACT-A-01" panel/round27/fact-a.md；同 agent 冷读 CLOSED（RD27-FACT-A-01） |
| RD-R27-09 | RD27-FACT-A-02 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-A-02） | panel/round27/fact-a.md:93 | grep -n "RD27-FACT-A-02" panel/round27/fact-a.md；同 agent 冷读 CLOSED（RD27-FACT-A-02） |
| RD-R27-10 | RD27-FACT-A-03 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-A-03） | panel/round27/fact-a.md:135 | grep -n "RD27-FACT-A-03" panel/round27/fact-a.md；同 agent 冷读 CLOSED（RD27-FACT-A-03） |
| RD-R27-11 | RD27-FACT-A-04 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-A-04） | panel/round27/fact-a.md:165 | grep -n "RD27-FACT-A-04" panel/round27/fact-a.md；同 agent 冷读 CLOSED（RD27-FACT-A-04） |
| RD-R27-12 | RD27-FACT-A-05 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-A-05） | panel/round27/fact-a.md:207 | grep -n "RD27-FACT-A-05" panel/round27/fact-a.md；同 agent 冷读 CLOSED（RD27-FACT-A-05） |
| RD-R27-13 | RD27-FACT-A-06 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-A-06） | panel/round27/fact-a.md:249 | grep -n "RD27-FACT-A-06" panel/round27/fact-a.md；同 agent 冷读 CLOSED（RD27-FACT-A-06） |
| RD-R27-14 | RD27-FACT-A-07 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-A-07） | panel/round27/fact-a.md:279 | grep -n "RD27-FACT-A-07" panel/round27/fact-a.md；同 agent 冷读 CLOSED（RD27-FACT-A-07） |
| RD-R27-15 | RD27-FACT-B-01 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-B-01） | panel/round27/fact-b.md:15 | grep -n "RD27-FACT-B-01" panel/round27/fact-b.md；同 agent 冷读 CLOSED（RD27-FACT-B-01） |
| RD-R27-16 | RD27-FACT-B-02 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-B-02） | panel/round27/fact-b.md:30 | grep -n "RD27-FACT-B-02" panel/round27/fact-b.md；同 agent 冷读 CLOSED（RD27-FACT-B-02） |
| RD-R27-17 | RD27-FACT-B-03 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-B-03） | panel/round27/fact-b.md:44 | grep -n "RD27-FACT-B-03" panel/round27/fact-b.md；同 agent 冷读 CLOSED（RD27-FACT-B-03） |
| RD-R27-18 | RD27-FACT-B-04 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-B-04） | panel/round27/fact-b.md:59 | grep -n "RD27-FACT-B-04" panel/round27/fact-b.md；同 agent 冷读 CLOSED（RD27-FACT-B-04） |
| RD-R27-19 | RD27-FACT-B-05 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-B-05） | panel/round27/fact-b.md:70 | grep -n "RD27-FACT-B-05" panel/round27/fact-b.md；同 agent 冷读 CLOSED（RD27-FACT-B-05） |
| RD-R27-20 | RD27-FACT-B-06 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-B-06） | panel/round27/fact-b.md:84 | grep -n "RD27-FACT-B-06" panel/round27/fact-b.md；同 agent 冷读 CLOSED（RD27-FACT-B-06） |
| RD-R27-21 | RD27-FACT-B-07 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-B-07） | panel/round27/fact-b.md:98 | grep -n "RD27-FACT-B-07" panel/round27/fact-b.md；同 agent 冷读 CLOSED（RD27-FACT-B-07） |
| RD-R27-22 | RD27-FACT-B-08 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-B-08） | panel/round27/fact-b.md:113 | grep -n "RD27-FACT-B-08" panel/round27/fact-b.md；同 agent 冷读 CLOSED（RD27-FACT-B-08） |
| RD-R27-23 | RD27-FACT-B-09 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-B-09） | panel/round27/fact-b.md:124 | grep -n "RD27-FACT-B-09" panel/round27/fact-b.md；同 agent 冷读 CLOSED（RD27-FACT-B-09） |
| RD-R27-24 | RD27-FACT-B-10 | R27 | fact | P2 | 历史发现 | ① | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-FACT-B-10） | panel/round27/fact-b.md:138 | grep -n "RD27-FACT-B-10" panel/round27/fact-b.md；同 agent 冷读 CLOSED（RD27-FACT-B-10） |
| RD-R27-25 | RD27-READER-A-01 | R27 | reader | P2 | 历史发现 | ④ | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-READER-A-01） | panel/round27/reader-a.md:18 | grep -n "RD27-READER-A-01" panel/round27/reader-a.md；同 agent 冷读 CLOSED（RD27-READER-A-01） |
| RD-R27-26 | RD27-READER-A-02 | R27 | reader | P2 | 历史发现 | ④ | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-READER-A-02） | panel/round27/reader-a.md:29 | grep -n "RD27-READER-A-02" panel/round27/reader-a.md；同 agent 冷读 CLOSED（RD27-READER-A-02） |
| RD-R27-27 | RD27-READER-A-03 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-01（R27 backlog，R28 结构重写关闭；RD27-READER-A-03） | panel/round27/reader-a.md:40 | grep -n "RD27-READER-A-03" panel/round27/reader-a.md；同 agent 冷读 CLOSED（RD27-READER-A-03） |
| RD-R27-28 | RD27-READER-A-04 | R27 | reader | P2 | 历史发现 | ④ | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-READER-A-04） | panel/round27/reader-a.md:51 | grep -n "RD27-READER-A-04" panel/round27/reader-a.md；同 agent 冷读 CLOSED（RD27-READER-A-04） |
| RD-R27-29 | RD27-READER-A-05 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-02（R27 backlog，R28 结构重写关闭；RD27-READER-A-05） | panel/round27/reader-a.md:62 | grep -n "RD27-READER-A-05" panel/round27/reader-a.md；同 agent 冷读 CLOSED（RD27-READER-A-05） |
| RD-R27-30 | RD27-READER-A-06 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-03（R27 backlog，R28 结构重写关闭；RD27-READER-A-06） | panel/round27/reader-a.md:73 | grep -n "RD27-READER-A-06" panel/round27/reader-a.md；同 agent 冷读 CLOSED（RD27-READER-A-06） |
| RD-R27-31 | RD27-READER-A-07 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-04（R27 backlog，R28 结构重写关闭；RD27-READER-A-07） | panel/round27/reader-a.md:84 | grep -n "RD27-READER-A-07" panel/round27/reader-a.md；同 agent 冷读 CLOSED（RD27-READER-A-07） |
| RD-R27-32 | RD27-READER-A-08 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-05（R27 backlog，R28 结构重写关闭；RD27-READER-A-08） | panel/round27/reader-a.md:95 | grep -n "RD27-READER-A-08" panel/round27/reader-a.md；同 agent 冷读 CLOSED（RD27-READER-A-08） |
| RD-R27-33 | RD27-READER-A-09 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-06（R27 backlog，R28 结构重写关闭；RD27-READER-A-09） | panel/round27/reader-a.md:106 | grep -n "RD27-READER-A-09" panel/round27/reader-a.md；同 agent 冷读 CLOSED（RD27-READER-A-09） |
| RD-R27-34 | RD27-READER-A-10 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-07（R27 backlog，R28 结构重写关闭；RD27-READER-A-10） | panel/round27/reader-a.md:117 | grep -n "RD27-READER-A-10" panel/round27/reader-a.md；同 agent 冷读 CLOSED（RD27-READER-A-10） |
| RD-R27-35 | RD27-READER-A-11 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-08（R27 backlog，R28 结构重写关闭；RD27-READER-A-11） | panel/round27/reader-a.md:128 | grep -n "RD27-READER-A-11" panel/round27/reader-a.md；同 agent 冷读 CLOSED（RD27-READER-A-11） |
| RD-R27-36 | RD27-READER-A-12 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-09（R27 backlog，R28 结构重写关闭；RD27-READER-A-12） | panel/round27/reader-a.md:139 | grep -n "RD27-READER-A-12" panel/round27/reader-a.md；同 agent 冷读 CLOSED（RD27-READER-A-12） |
| RD-R27-37 | RD27-READER-B-01 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-04（R27 backlog，R28 结构重写关闭；RD27-READER-B-01） | panel/round27/reader-b.md:50 | grep -n "RD27-READER-B-01" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-01） |
| RD-R27-38 | RD27-READER-B-04 | R27 | reader | P2 | 历史发现 | FP | dup-of: RD-R28-10（R27 backlog；反证见 R28 台账；RD27-READER-B-04） | 文档原文 §4.3 2604 行是“boot-shim 三条链”与“两个直核腿”，无“两腿”表述 | grep -n "RD27-READER-B-04" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-04） |
| RD-R27-39 | RD27-READER-B-02 | R27 | reader | P2 | 历史发现 | ④ | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-READER-B-02） | panel/round27/reader-b.md:78 | grep -n "RD27-READER-B-02" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-02） |
| RD-R27-40 | RD27-READER-B-08 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-09（R27 backlog，R28 结构重写关闭；RD27-READER-B-08） | panel/round27/reader-b.md:90 | grep -n "RD27-READER-B-08" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-08） |
| RD-R27-41 | RD27-READER-B-03 | R27 | reader | P2 | 历史发现 | ④ | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-READER-B-03） | panel/round27/reader-b.md:108 | grep -n "RD27-READER-B-03" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-03） |
| RD-R27-42 | RD27-READER-B-05 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-11（R27 backlog，R28 结构重写关闭；RD27-READER-B-05） | panel/round27/reader-b.md:120 | grep -n "RD27-READER-B-05" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-05） |
| RD-R27-43 | RD27-READER-B-10 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-14（R27 backlog，R28 结构重写关闭；RD27-READER-B-10） | panel/round27/reader-b.md:132 | grep -n "RD27-READER-B-10" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-10） |
| RD-R27-44 | RD27-READER-B-13 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-17（R27 backlog，R28 结构重写关闭；RD27-READER-B-13） | panel/round27/reader-b.md:144 | grep -n "RD27-READER-B-13" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-13） |
| RD-R27-45 | RD27-READER-B-17 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-20（R27 backlog，R28 结构重写关闭；RD27-READER-B-17） | panel/round27/reader-b.md:157 | grep -n "RD27-READER-B-17" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-17） |
| RD-R27-46 | RD27-READER-B-07 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-12（R27 backlog，R28 结构重写关闭；RD27-READER-B-07） | panel/round27/reader-b.md:174 | grep -n "RD27-READER-B-07" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-07） |
| RD-R27-47 | RD27-READER-B-14 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-16（R27 backlog，R28 结构重写关闭；RD27-READER-B-14） | panel/round27/reader-b.md:188 | grep -n "RD27-READER-B-14" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-14） |
| RD-R27-48 | RD27-READER-B-16 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-19（R27 backlog，R28 结构重写关闭；RD27-READER-B-16） | panel/round27/reader-b.md:202 | grep -n "RD27-READER-B-16" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-16） |
| RD-R27-49 | RD27-READER-B-06 | R27 | reader | P2 | 历史发现 | ④ | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-READER-B-06） | panel/round27/reader-b.md:219 | grep -n "RD27-READER-B-06" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-06） |
| RD-R27-50 | RD27-READER-B-09 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-13（R27 backlog，R28 结构重写关闭；RD27-READER-B-09） | panel/round27/reader-b.md:234 | grep -n "RD27-READER-B-09" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-09） |
| RD-R27-51 | RD27-READER-B-11 | R27 | reader | P2 | 历史发现 | FP | dup-of: RD-R28-15（R27 backlog；反证见 R28 台账；RD27-READER-B-11） | 文档原文 §5.2.3 3249 行把“三条 riscv64/aarch64”与“x86 另”分句，成员与计数一致 | grep -n "RD27-READER-B-11" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-11） |
| RD-R27-52 | RD27-READER-B-12 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-16（R27 backlog，R28 结构重写关闭；RD27-READER-B-12） | panel/round27/reader-b.md:264 | grep -n "RD27-READER-B-12" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-12） |
| RD-R27-53 | RD27-READER-B-15 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-18（R27 backlog，R28 结构重写关闭；RD27-READER-B-15） | panel/round27/reader-b.md:277 | grep -n "RD27-READER-B-15" panel/round27/reader-b.md；同 agent 冷读 CLOSED（RD27-READER-B-15） |
| RD-R27-54 | RD27-READER-C-01 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-21（R27 backlog，R28 结构重写关闭；RD27-READER-C-01） | panel/round27/reader-c.md:22 | grep -n "RD27-READER-C-01" panel/round27/reader-c.md；同 agent 冷读 CLOSED（RD27-READER-C-01） |
| RD-R27-55 | RD27-READER-C-02 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-22（R27 backlog，R28 结构重写关闭；RD27-READER-C-02） | panel/round27/reader-c.md:36 | grep -n "RD27-READER-C-02" panel/round27/reader-c.md；同 agent 冷读 CLOSED（RD27-READER-C-02） |
| RD-R27-56 | RD27-READER-C-03 | R27 | reader | P2 | 历史发现 | ④ | 已修/已处理：R27 面板记录（EDITION-RETRO §1；RD27-READER-C-03） | panel/round27/reader-c.md:50 | grep -n "RD27-READER-C-03" panel/round27/reader-c.md；同 agent 冷读 CLOSED（RD27-READER-C-03） |
| RD-R27-57 | RD27-READER-C-04 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-23（R27 backlog，R28 结构重写关闭；RD27-READER-C-04） | panel/round27/reader-c.md:64 | grep -n "RD27-READER-C-04" panel/round27/reader-c.md；同 agent 冷读 CLOSED（RD27-READER-C-04） |
| RD-R27-58 | RD27-READER-C-05 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-13（R27 backlog，R28 结构重写关闭；RD27-READER-C-05） | panel/round27/reader-c.md:78 | grep -n "RD27-READER-C-05" panel/round27/reader-c.md；同 agent 冷读 CLOSED（RD27-READER-C-05） |
| RD-R27-59 | RD27-READER-C-06 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-24（R27 backlog，R28 结构重写关闭；RD27-READER-C-06） | panel/round27/reader-c.md:92 | grep -n "RD27-READER-C-06" panel/round27/reader-c.md；同 agent 冷读 CLOSED（RD27-READER-C-06） |
| RD-R27-60 | RD27-READER-C-07 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-09（R27 backlog，R28 结构重写关闭；RD27-READER-C-07） | panel/round27/reader-c.md:106 | grep -n "RD27-READER-C-07" panel/round27/reader-c.md；同 agent 冷读 CLOSED（RD27-READER-C-07） |
| RD-R27-61 | RD27-READER-C-08 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-25（R27 backlog，R28 结构重写关闭；RD27-READER-C-08） | panel/round27/reader-c.md:120 | grep -n "RD27-READER-C-08" panel/round27/reader-c.md；同 agent 冷读 CLOSED（RD27-READER-C-08） |
| RD-R27-62 | RD27-READER-C-09 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-26（R27 backlog，R28 结构重写关闭；RD27-READER-C-09） | panel/round27/reader-c.md:133 | grep -n "RD27-READER-C-09" panel/round27/reader-c.md；同 agent 冷读 CLOSED（RD27-READER-C-09） |
| RD-R27-63 | RD27-READER-C-10 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-27（R27 backlog，R28 结构重写关闭；RD27-READER-C-10） | panel/round27/reader-c.md:147 | grep -n "RD27-READER-C-10" panel/round27/reader-c.md；同 agent 冷读 CLOSED（RD27-READER-C-10） |
| RD-R27-64 | RD27-READER-C-11 | R27 | reader | P2 | 历史发现 | ④ | dup-of: RD-R28-28（R27 backlog，R28 结构重写关闭；RD27-READER-C-11） | panel/round27/reader-c.md:160 | grep -n "RD27-READER-C-11" panel/round27/reader-c.md；同 agent 冷读 CLOSED（RD27-READER-C-11） |
| RD-R27-65 | RD27-CLAIM-01 | R27 | refute | P2 | 历史发现 | FP | 已修/已处理：R27 面板记录（EDITION-RETRO §1；CLAIM-01） | panel/round27/refute.md:7 | grep -n "CLAIM-01" panel/round27/refute.md；同 agent 冷读 CLOSED（CLAIM-01） |
| RD-R27-66 | RD27-CLAIM-02 | R27 | refute | P2 | 历史发现 | FP | 已修/已处理：R27 面板记录（EDITION-RETRO §1；CLAIM-02） | panel/round27/refute.md:26 | grep -n "CLAIM-02" panel/round27/refute.md；同 agent 冷读 CLOSED（CLAIM-02） |
| RD-R27-67 | RD27-CLAIM-03 | R27 | refute | P2 | 历史发现 | FP | 已修/已处理：R27 面板记录（EDITION-RETRO §1；CLAIM-03） | panel/round27/refute.md:47 | grep -n "CLAIM-03" panel/round27/refute.md；同 agent 冷读 CLOSED（CLAIM-03） |
| RD-R27-68 | RD27-CLAIM-04 | R27 | refute | P2 | 历史发现 | FP | 已修/已处理：R27 面板记录（EDITION-RETRO §1；CLAIM-04） | panel/round27/refute.md:69 | grep -n "CLAIM-04" panel/round27/refute.md；同 agent 冷读 CLOSED（CLAIM-04） |
| RD-R27-69 | RD27-CLAIM-05 | R27 | refute | P2 | 历史发现 | FP | 已修/已处理：R27 面板记录（EDITION-RETRO §1；CLAIM-05） | panel/round27/refute.md:91 | grep -n "CLAIM-05" panel/round27/refute.md；同 agent 冷读 CLOSED（CLAIM-05） |
| RD-R27-70 | RD27-CLAIM-06 | R27 | refute | P2 | 历史发现 | FP | 已修/已处理：R27 面板记录（EDITION-RETRO §1；CLAIM-06） | panel/round27/refute.md:113 | grep -n "CLAIM-06" panel/round27/refute.md；同 agent 冷读 CLOSED（CLAIM-06） |

## R28 结构轮（2026-11-10，V3.1 首轮：R27 backlog 结构收口）

> 发现 ID 加 `RD28-` 前缀，只是为了跨轮唯一（R27 的 `READER-B-01` 与 R26 同名）；对应关系见 `panel/round28/closure-manifest.md` 的标题。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R28-01 | RD28-READER-A-03 | R27 | reader-a | P2 | 计数口径 | ④ | rewrite：§3.7 行名改“协议段/交接面 4”，§1.4 索引与 §4.8 换用同一口径 | §3.7 表 1926 行；§1.4 索引 142 行 | `grep -n "协议段/交接面" 文档 3 命中`；同 agent 冷读 CLOSED（能复述 send/receive 是三条腿两端） |
| RD-R28-02 | RD28-READER-A-05 | R27 | reader-a | P3 | 术语限定 | ④ | §3.4.1 限定语补为“没有任何 `StartupObjects` 生产者” | §3.4.1 1621 行 | `grep -n "没有任何 .StartupObjects. 生产者" 1 命中`；同 agent 冷读 CLOSED（不再读成无任何生产者） |
| RD-R28-03 | RD28-READER-A-06 | R27 | reader-a | P2 | 指针落空 | ④ | §1.3 两处落空指针改指框表与覆盖状态列；§1.2 的“见 §1.4”由 §1.4 新增直核定义兑现 | §1.3 96 行；§1.4 129 行 | `grep -n "框表与约束的对照" 1 命中`；同 agent 冷读 CLOSED（三处指针均可落） |
| RD-R28-04 | RD28-READER-A-07、RD28-READER-B-01 | R27 | reader-a | P1 | 容量权威指环 | ④ | rewrite：§4.5 容量表补 `KernelInfo.memmap`、`free_regions`、`deducted` 三行并声明唯一权威；§2.4.1.4/§3.4.1 只引用 | §4.5 表 2810-2825 行 | `grep -n "这张表是全文交接容量的唯一权威" 1 命中`；`grep -n "128/64/47 的唯一权威" 0 命中`；同 agent 冷读 CLOSED（三数一表可查） |
| RD-R28-05 | RD28-READER-A-08 | R27 | reader-a | P2 | 术语未定义 | ④ | §4.2.2 令牌表补 `TYPE_INDEPENDENT` 行（三个属性位，只有 `RUNTIME` 可达） | `os/boot-shim/src/ebs_map.rs:209` | `grep -n "TYPE_INDEPENDENT.*RUNTIME" 文档 1 命中`；同 agent 冷读 CLOSED（例 4 可核对） |
| RD-R28-06 | RD28-READER-A-09 | R27 | reader-a | P2 | 判定语义 | ④ | rewrite：§4.2.2 覆盖句改为“未被 Step 2 拒绝”与“到达 `checked_len`” | §4.2.2 2426/2455 行 | `grep -n "到达 .checked_len. 的描述符" 1 命中`；同 agent 冷读 CLOSED（两闸覆盖可区分） |
| RD-R28-07 | RD28-READER-A-10 | R27 | reader-a | P2 | 术语一词多义 | ④ | rewrite：§4.6 三处设备表补全函数名，写明 x86 空表属于哪张表 | §4.6 2838/2893/2903 行 | `grep -n "identity_device_windows() 返回空" 1 命中`；同 agent 冷读 CLOSED（Step 2c 非 no-op） |
| RD-R28-08 | RD28-READER-A-11 | R27 | reader-a | P2 | 过程词 | ④ | rewrite：§3.2/§4.1/§4.6/§3.5 的评审轮次时间锚点改为当前状态 | §3.2 1458；§4.1 2251；§4.6 2893；§3.5 1824 | `grep -n "R23 审计后" 0 命中`；`grep -n "R18 之前这里" 0 命中`；同 agent 冷读 CLOSED（历史句可跳过） |
| RD-R28-09 | RD28-READER-A-12、RD28-READER-B-08、RD28-READER-C-07 | R27 | reader-a | P2 | 诊断标签/时间序 | ④ | rewrite：§6.1 里程碑表加“适用腿”列，补阶段标签对照，`step1+2` 改为跳转前 | §6.1 表 3315-3331；`os/kernel/src/lib.rs:928-935` | `grep -n "阶段标签对照" 1 命中`；`grep -n "建根后半、跳转前" 1 命中`；同 agent 冷读 CLOSED |
| RD-R28-10 | RD28-READER-B-04 | R27 | reader-b | P1 | 计数（反证） | FP | 报告引文与当前文档不一致，判误报 | 文档原文 §4.3 2604 行是“boot-shim 三条链”与“两个直核腿”，没有“两腿”表述；报告引用的旧句在当前文档不存在 | `grep -n "反事实只对 boot-shim 三条链成立" 1 命中`；`grep -n "两腿" 文档 0 命中`；同 agent 冷读 CLOSED（反证成立） |
| RD-R28-11 | RD28-READER-B-05 | R27 | reader-b | P1 | 边界声明缺失 | ④ | §1.4 增第四条边界声明（boot2 交棒契约不在本篇取证范围） | §1.4 151-156 行 | `grep -n "边界声明（boot2 的交棒契约）" 1 命中`；同 agent 冷读 CLOSED（§1.1 指针可落） |
| RD-R28-12 | RD28-READER-B-07 | R27 | reader-b | P2 | 重复冗余 | ④ | §4.2.2 删除落点复述，改一句总括；“第 1、2 步”改“动作 1、2” | §4.2.2 2450/2460 行 | `grep -n "动作 1、2 的界" 1 命中`；`grep -n "第 1、2 步的界" 0 命中`；同 agent 冷读 CLOSED |
| RD-R28-13 | RD28-READER-B-09、RD28-READER-C-05 | R27 | reader-b | P2 | 前提层次 | ④ | rewrite：§2.4.1.3 结论拆两级，前提二改“非零且页对齐（≥0x1000）” | §2.4.1.3 719-728 行 | `grep -n "第一级结论" 1 命中`；`grep -n "第二级结论" 1 命中`；同 agent 冷读 CLOSED（必要条件与缓解条件分开） |
| RD-R28-14 | RD28-READER-B-10 | R27 | reader-b | P2 | 调用点归属 | ④ | §5.2.1 生产调用点改指 `prepare_and_enter_kernel_image` | `os/boot-shim/src/main.rs:34`；`os/boot-shim/src/uefi_helpers.rs` | `grep -n "生产调用点在 .prepare_and_enter_kernel_image" 1 命中`；同 agent 冷读 CLOSED |
| RD-R28-15 | RD28-READER-B-11 | R27 | reader-b | P2 | 计数（反证） | FP | 报告把分句读成第四项，判误报 | 文档原文 §5.2.3 3249 行把“三条 riscv64/aarch64”与“x86 另”分句，成员与计数一致，没有“说三列四” | `grep -n "三条 riscv64/aarch64 启动门链" 1 命中`；`grep -n "标记集另含" 1 命中`；同 agent 冷读 CLOSED（反证成立） |
| RD-R28-16 | RD28-READER-B-12、RD28-READER-B-14 | R27 | reader-b | P2 | 索引/重复 | ④ | §1.4 索引改“依赖约束；非执行序”；§1.3 删 D5 重述；§2.8 按 D1–D5 标号重排 | §1.4 137；§1.3 94；§2.8 1369-1375 | `grep -n "依赖约束；非执行序" 1 命中`；`grep -c "不是执行序" 文档 1 命中`；同 agent 冷读 CLOSED |
| RD-R28-17 | RD28-READER-B-13 | R27 | reader-b | P2 | 边界清单缺口 | ④ | §1.4 下游交代清单补 §2.4.1.4 | §1.4 155 行 | `grep -n "§2.4.1.4 的 Rust 侧容量上限" 1 命中`；同 agent 冷读 CLOSED（越界内容已登记） |
| RD-R28-18 | RD28-READER-B-15 | R27 | reader-b | P3 | 画像/正文不一致 | ④ | 读者画像 §1.2/§1.3 定位与 9 条教学目标对齐正文 | `tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/reader-profile.md:9-21` | `grep -n "§1.2 是第一章收束" reader-profile.md 1 命中`；`grep -c "^[0-9]" 9 命中`；同 agent 冷读 CLOSED |
| RD-R28-19 | RD28-READER-B-16 | R27 | reader-b | P3 | 职责重叠 | ④ | §2.1 加分工句：本节只答“为什么在命令行”，完整落链见 §2.3 | §2.1 243 行 | `grep -n "本节只回答" 1 命中`；同 agent 冷读 CLOSED（两处详略边界可复述） |
| RD-R28-20 | RD28-READER-B-17 | R27 | reader-b | P3 | 名称/锚点错位 | ④ | §2.8 的 `enable` 改 `vm_enable_paging`；D2 锚点 §2.3 改 §2.4 | §2.8 D2/D3 | `grep -n "vm_enable_paging. 才写" 1 命中`；`grep -n "§2.3 的 .add_memmap." 0 命中`；同 agent 冷读 CLOSED |
| RD-R28-21 | RD28-READER-C-01 | R27 | reader-c | P1 | 名实不符 | ④ | rewrite：§1.1/§2.2/文档头统一“第一跳 = `multiboot_init`，第一条指令 = 设栈” | §1.1 34；§2.2 256；文档头 5 | `grep -n "第一跳（.multiboot_init.）的第一条指令必须是设栈" 1 命中`；同 agent 冷读 CLOSED（三处不再两种“第一跳”） |
| RD-R28-22 | RD28-READER-C-02 | R27 | reader-c | P2 | 编号角色混淆 | ④ | rewrite：§1.2 主链改框号；§2.8 五条按 D1–D5 标号排序 | §1.2 71；§2.8 1369-1375 | `grep -n "主链经过框 2" 1 命中`；`grep -n "D5. 恒等映射" 2 命中`；同 agent 冷读 CLOSED（框 2 是链接期事实已写明） |
| RD-R28-23 | RD28-READER-C-04 | R27 | reader-c | P1 | 因果时序 | ④ | rewrite：§3.5 表前写回收时刻，表列改“按取页时谁还活着读”，两行降级为“不构成先坏点” | §3.5 1813-1822 行 | `grep -n "先把时刻说准" 1 命中`；`grep -n "不构成先坏点" 2 命中`；同 agent 冷读 CLOSED（时间线闭合） |
| RD-R28-24 | RD28-READER-C-06 | R27 | reader-c | P2 | 证据强度 | ④ | §5.3 TODO-07 断言列降级为“互斥 + fixture 期望计数”，写明覆盖边界 | §5.3 TODO-07 3267 行 | `grep -n "fixture 的期望计数相符" 1 命中`；同 agent 冷读 CLOSED（不再把 fixture 当完备性） |
| RD-R28-25 | RD28-READER-C-08 | R27 | reader-c | P2 | 指代二义 | ④ | §4.6 的“不能照做”改“不能做 Step 2”；“同一段映射”改子集关系 | §4.6 2899 行 | `grep -n "不能.*做 Step 2" 1 命中`；同 agent 冷读 CLOSED（相等 → 跳过） |
| RD-R28-26 | RD28-READER-C-09 | R27 | reader-c | P2 | 时间序错位 | ④ | rewrite：§4.2.1 6b 说明改为拍 7 与拍 8 之间，并给 EBS 前后重建句 | §4.2.1 2397 行 | `grep -n "拍 7 与拍 8 之间" 文档 2 命中`；同 agent 冷读 CLOSED（6b 在 EBS 之后） |
| RD-R28-27 | RD28-READER-C-10 | R27 | reader-c | P2 | 证据不足 | ④ | §3.9 的 ② 三架构各补 `inherit_supervisor_half` 源码锚点 | `os/arch/src/arm64/paging.rs:595`；`os/arch/src/x86_64/paging.rs:650`；`os/arch/src/riscv64/paging.rs:694` | `grep -n "inherit_supervisor_half" 文档 3 命中`；同 agent 冷读 CLOSED（三支同 trait） |
| RD-R28-28 | RD28-READER-C-11 | R27 | reader-c | P3 | 范围过宽 | ④ | §3.8 的“浪费剩余页”改为“VM 发布前未被接收侧取用的尾部页” | §3.8 2201 行 | `grep -n "未被接收侧取用的尾部页" 1 命中`；同 agent 冷读 CLOSED（已用页仍在使用） |
| RD-R28-29 | RD28-DESIGN-D2-16 | R27 | design-d2 | P2 | 取用路径分叉 | ⑥ | 实施：`store_kernel_info` 补独立文档注释；`kmain` 改用返回值；§4.5 同步 | `os/kernel/src/lib.rs:1441-1455`、`:1732` | `grep -n "let kernel_info: &KernelInfo = store_kernel_info" os/kernel/src/lib.rs 1 命中`；`cargo test -p minix-kernel --features mock --lib boot_windows 3 passed`；同 agent 冷读 CLOSED |
| RD-R28-30 | RD28-DESIGN-D2-17 | R27 | design-d2 | P2 | 类型边界 | ⑥ | 实施：文档写死裸对边界规则；`push` 注释明确空区间；新增空窗口负控 | `os/kernel/src/lib.rs:1059-1090`、`:5759-5763` | `cargo test -p minix-kernel --features mock --lib boot_windows_reject_empty_ranges ok`；同 agent 冷读 CLOSED（边界规则可执行） |
| RD-R28-31 | RD28-DESIGN-D2-18 | R27 | design-d2 | P2 | 所有权分裂 | ⑥ | 实施：§3.4.5 写明直核腿双所有者（`BOOT_STORAGE` 四类 + DTB 静态量三类）与闲置容量取舍 | §3.4.5 1756 行；`os/libs/minix-boot/src/memmap_source.rs:160` | `grep -n "两个所有者、两套重入守卫" 1 命中`；同 agent 冷读 CLOSED（不再宣称单一所有者） |
| RD-R28-32 | RD28-DESIGN-D2-19 | R27 | design-d2 | P2 | 段归属陈述失真 | ⑥ | 实施：§3.4.5 与三处代码注释改 `.data`；`nm` 读数回填 0x3480 | `objdump -t os/target/riscv64gc-unknown-none-elf/release/kernel` 显示 `BOOT_STORAGE .data 0x3480`；`os/kernel-image/src/bootface_common.rs:12` | `grep -n "BOOT_STORAGE.*.data" 文档 1 命中`；同 agent 冷读 CLOSED（`.data` 预算可核对） |

## R29 完整 survey（2026-11-10，V3.1 第 2 轮）

> 发现 ID 加 `RD29-` 前缀，只为跨轮唯一；原始报告在 `panel/round29/`，标题已同步前缀。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R29-01 | RD29-FACT-A-01 | R29 | fact-a | P2 | C 侧事实/前提 | ① | 已修：§2.6.4 拆开 unpaged 低段与高半代码/栈；§2.4.2 E820 对账改为多因残差；§2.7 ARM 内核项相切前提写明（RD29-FACT-A-01） | §2.6.4 1160；§2.4.2 779；§2.7 1324 | `grep -n "窗口里执行的对象不止一类" 1 命中`；`grep -n "残差不是单因" 1 命中`；`grep -n "内核项是例外" 1 命中`（RD29-FACT-A-01） |
| RD-R29-02 | RD29-FACT-A-02 | R29 | fact-a | P2 | C 侧事实/前提 | ① | 已修：§2.6.4 拆开 unpaged 低段与高半代码/栈；§2.4.2 E820 对账改为多因残差；§2.7 ARM 内核项相切前提写明（RD29-FACT-A-02） | §2.6.4 1160；§2.4.2 779；§2.7 1324 | `grep -n "窗口里执行的对象不止一类" 1 命中`；`grep -n "残差不是单因" 1 命中`；`grep -n "内核项是例外" 1 命中`（RD29-FACT-A-02） |
| RD-R29-03 | RD29-FACT-A-03 | R29 | fact-a | P2 | C 侧事实/前提 | ① | 已修：§2.6.4 拆开 unpaged 低段与高半代码/栈；§2.4.2 E820 对账改为多因残差；§2.7 ARM 内核项相切前提写明（RD29-FACT-A-03） | §2.6.4 1160；§2.4.2 779；§2.7 1324 | `grep -n "窗口里执行的对象不止一类" 1 命中`；`grep -n "残差不是单因" 1 命中`；`grep -n "内核项是例外" 1 命中`（RD29-FACT-A-03） |
| RD-R29-04 | RD29-FACT-B-01 | R29 | fact-b | P2 | Rust 侧事实/同步 | ① | 已修：ISA_MASK 16 位/31 位；x86 设备区域函数归属；直核只 init 不 register；测试数 874/866；hello-boot 直构 KernelInfo；param_buf 打印点；历史注释逐字引文（RD29-FACT-B-01） | `os/boot-shim/src/ebs_map.rs:183`；`os/arch/src/lib.rs:257`；`os/kernel/src/lib.rs:1441`；§4.7 2982；§6.1 3313；§3.5 1824 | `grep -n "ISA_MASK 本身是 16 位" 1 命中`；`grep -n "kernel_identity_mmio_regions(). 本身返回 IO-APIC" 1 命中`；`cargo test -p minix-kernel --features mock --lib -- --list` 874 条（RD29-FACT-B-01） |
| RD-R29-05 | RD29-FACT-B-02 | R29 | fact-b | P2 | Rust 侧事实/同步 | ① | 已修：ISA_MASK 16 位/31 位；x86 设备区域函数归属；直核只 init 不 register；测试数 874/866；hello-boot 直构 KernelInfo；param_buf 打印点；历史注释逐字引文（RD29-FACT-B-02） | `os/boot-shim/src/ebs_map.rs:183`；`os/arch/src/lib.rs:257`；`os/kernel/src/lib.rs:1441`；§4.7 2982；§6.1 3313；§3.5 1824 | `grep -n "ISA_MASK 本身是 16 位" 1 命中`；`grep -n "kernel_identity_mmio_regions(). 本身返回 IO-APIC" 1 命中`；`cargo test -p minix-kernel --features mock --lib -- --list` 874 条（RD29-FACT-B-02） |
| RD-R29-06 | RD29-FACT-B-03 | R29 | fact-b | P2 | Rust 侧事实/同步 | ① | 已修：ISA_MASK 16 位/31 位；x86 设备区域函数归属；直核只 init 不 register；测试数 874/866；hello-boot 直构 KernelInfo；param_buf 打印点；历史注释逐字引文（RD29-FACT-B-03） | `os/boot-shim/src/ebs_map.rs:183`；`os/arch/src/lib.rs:257`；`os/kernel/src/lib.rs:1441`；§4.7 2982；§6.1 3313；§3.5 1824 | `grep -n "ISA_MASK 本身是 16 位" 1 命中`；`grep -n "kernel_identity_mmio_regions(). 本身返回 IO-APIC" 1 命中`；`cargo test -p minix-kernel --features mock --lib -- --list` 874 条（RD29-FACT-B-03） |
| RD-R29-07 | RD29-FACT-B-04 | R29 | fact-b | P2 | Rust 侧事实/同步 | ① | 已修：ISA_MASK 16 位/31 位；x86 设备区域函数归属；直核只 init 不 register；测试数 874/866；hello-boot 直构 KernelInfo；param_buf 打印点；历史注释逐字引文（RD29-FACT-B-04） | `os/boot-shim/src/ebs_map.rs:183`；`os/arch/src/lib.rs:257`；`os/kernel/src/lib.rs:1441`；§4.7 2982；§6.1 3313；§3.5 1824 | `grep -n "ISA_MASK 本身是 16 位" 1 命中`；`grep -n "kernel_identity_mmio_regions(). 本身返回 IO-APIC" 1 命中`；`cargo test -p minix-kernel --features mock --lib -- --list` 874 条（RD29-FACT-B-04） |
| RD-R29-08 | RD29-FACT-B-05 | R29 | fact-b | P2 | Rust 侧事实/同步 | ① | 已修：ISA_MASK 16 位/31 位；x86 设备区域函数归属；直核只 init 不 register；测试数 874/866；hello-boot 直构 KernelInfo；param_buf 打印点；历史注释逐字引文（RD29-FACT-B-05） | `os/boot-shim/src/ebs_map.rs:183`；`os/arch/src/lib.rs:257`；`os/kernel/src/lib.rs:1441`；§4.7 2982；§6.1 3313；§3.5 1824 | `grep -n "ISA_MASK 本身是 16 位" 1 命中`；`grep -n "kernel_identity_mmio_regions(). 本身返回 IO-APIC" 1 命中`；`cargo test -p minix-kernel --features mock --lib -- --list` 874 条（RD29-FACT-B-05） |
| RD-R29-09 | RD29-FACT-B-06 | R29 | fact-b | P2 | Rust 侧事实/同步 | ① | 已修：ISA_MASK 16 位/31 位；x86 设备区域函数归属；直核只 init 不 register；测试数 874/866；hello-boot 直构 KernelInfo；param_buf 打印点；历史注释逐字引文（RD29-FACT-B-06） | `os/boot-shim/src/ebs_map.rs:183`；`os/arch/src/lib.rs:257`；`os/kernel/src/lib.rs:1441`；§4.7 2982；§6.1 3313；§3.5 1824 | `grep -n "ISA_MASK 本身是 16 位" 1 命中`；`grep -n "kernel_identity_mmio_regions(). 本身返回 IO-APIC" 1 命中`；`cargo test -p minix-kernel --features mock --lib -- --list` 874 条（RD29-FACT-B-06） |
| RD-R29-10 | RD29-FACT-B-07 | R29 | fact-b | P2 | Rust 侧事实/同步 | ① | 已修：ISA_MASK 16 位/31 位；x86 设备区域函数归属；直核只 init 不 register；测试数 874/866；hello-boot 直构 KernelInfo；param_buf 打印点；历史注释逐字引文（RD29-FACT-B-07） | `os/boot-shim/src/ebs_map.rs:183`；`os/arch/src/lib.rs:257`；`os/kernel/src/lib.rs:1441`；§4.7 2982；§6.1 3313；§3.5 1824 | `grep -n "ISA_MASK 本身是 16 位" 1 命中`；`grep -n "kernel_identity_mmio_regions(). 本身返回 IO-APIC" 1 命中`；`cargo test -p minix-kernel --features mock --lib -- --list` 874 条（RD29-FACT-B-07） |
| RD-R29-11 | RD29-FACT-B-08 | R29 | fact-b | P3 | 编译期断言（误报） | FP | 报告称 2144 B 只由测试断言钉住；实际 `handoff_v1.rs:422` 是 `const _: () = { ... }`，`:466` 的 `assert!` 在编译期求值（RD29-FACT-B-08） | `os/libs/minix-boot/src/handoff_v1.rs:422-467` | `grep -n "const _: () = {" os/libs/minix-boot/src/handoff_v1.rs` 1 命中；`grep -n "WIRE_STARTUP_END == 30624" 1 命中`（RD29-FACT-B-08） |
| RD-R29-12 | RD29-READER-A-01 | R29 | reader-a | P1 | 门链成员/打印者 | ④ | rewrite：§5.2.3 三条链改为 aarch64 UEFI+ESP；§6.1 明确 `step1+2` 与 `[raw] entering` 的打印者（RD29-READER-A-01） | §5.2.3 3251；§6.1 3338 | `grep -n "三条 aarch64/riscv64 门链" 1 命中`；`grep -n "它不是.已经在独立镜像里.的信号" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-A-01） |
| RD-R29-13 | RD29-READER-A-02 | R29 | reader-a | P1 | 门链成员/打印者 | ④ | rewrite：§5.2.3 三条链改为 aarch64 UEFI+ESP；§6.1 明确 `step1+2` 与 `[raw] entering` 的打印者（RD29-READER-A-02） | §5.2.3 3251；§6.1 3338 | `grep -n "三条 aarch64/riscv64 门链" 1 命中`；`grep -n "它不是.已经在独立镜像里.的信号" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-A-02） |
| RD-R29-14 | RD29-READER-A-03 | R29 | reader-a | P2 | 图注/读序 | ④ | rewrite：§1.2 图注改为“竖向排列只表示布局，阅读顺序按主链”（RD29-READER-A-03） | §1.2 42 行 | `grep -n "竖向排列只表示布局" 1 命中`；同 agent 冷读 CLOSED（图与主链不再二义）（RD29-READER-A-03） |
| RD-R29-15 | RD29-READER-A-04 | R29 | reader-a | P2 | 时序/表视角 | ④ | 已修：A+D 的 A 步预订；§4.4 表头标注直核 builder 视角并定义 `argv[0]`（RD29-READER-A-04） | §3.4.1 1629；§3.8 2166；§4.2.1 2387；§4.4 2648 | `grep -n "A+D 的 A 步" 2 命中`；`grep -n "直核 builder 视角" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-A-04） |
| RD-R29-16 | RD29-READER-A-05 | R29 | reader-a | P2 | 时序/表视角 | ④ | 已修：A+D 的 A 步预订；§4.4 表头标注直核 builder 视角并定义 `argv[0]`（RD29-READER-A-05） | §3.4.1 1629；§3.8 2166；§4.2.1 2387；§4.4 2648 | `grep -n "A+D 的 A 步" 2 命中`；`grep -n "直核 builder 视角" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-A-05） |
| RD-R29-17 | RD29-READER-A-06 | R29 | reader-a | P2 | 条件结论 | ④ | rewrite：§2.4.1.3 第二级结论补“跳板条目不是最高条目”并降为观测（RD29-READER-A-06） | §2.4.1.3 728 | `grep -n "才说明在本仓观测到的布局下不会当场发作" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-A-06） |
| RD-R29-18 | RD29-READER-A-07 | R29 | reader-a | P3 | 计数/术语/句法 | ④ | 已修：§4.5 表题计数；§2.4.1.4 v9 括注；§1.1 boot2 必要性限定；§4.8 缺括号（RD29-READER-A-07） | §4.5 2810；§2.4.1.4 737；§1.1 32；§4.8 3057 | `grep -n "七个落地区、一个名字池" 1 命中`；`grep -n "wire 信封版本" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-A-07） |
| RD-R29-19 | RD29-READER-A-08 | R29 | reader-a | P3 | 计数/术语/句法 | ④ | 已修：§4.5 表题计数；§2.4.1.4 v9 括注；§1.1 boot2 必要性限定；§4.8 缺括号（RD29-READER-A-08） | §4.5 2810；§2.4.1.4 737；§1.1 32；§4.8 3057 | `grep -n "七个落地区、一个名字池" 1 命中`；`grep -n "wire 信封版本" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-A-08） |
| RD-R29-20 | RD29-READER-A-09 | R29 | reader-a | P3 | 计数/术语/句法 | ④ | 已修：§4.5 表题计数；§2.4.1.4 v9 括注；§1.1 boot2 必要性限定；§4.8 缺括号（RD29-READER-A-09） | §4.5 2810；§2.4.1.4 737；§1.1 32；§4.8 3057 | `grep -n "七个落地区、一个名字池" 1 命中`；`grep -n "wire 信封版本" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-A-09） |
| RD-R29-21 | RD29-READER-A-10 | R29 | reader-a | P3 | 计数/术语/句法 | ④ | 已修：§4.5 表题计数；§2.4.1.4 v9 括注；§1.1 boot2 必要性限定；§4.8 缺括号（RD29-READER-A-10） | §4.5 2810；§2.4.1.4 737；§1.1 32；§4.8 3057 | `grep -n "七个落地区、一个名字池" 1 命中`；`grep -n "wire 信封版本" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-A-10） |
| RD-R29-22 | RD29-READER-B-01 | R29 | reader-b | P2 | 图注/索引/门链 | ④ | 已修：§1.2 D4 统一；§1.4 六入口消费者改 §5.1/§5.3；§5.2.3 门链成员（RD29-READER-B-01） | §1.2 71；§1.4 146；§5.2.3 3251 | `grep -n "承载 D4（结构先于" 1 命中`；`grep -n "六入口" 文档 4 命中`；同 agent 冷读 CLOSED（RD29-READER-B-01） |
| RD-R29-23 | RD29-READER-B-02 | R29 | reader-b | P2 | 图注/索引/门链 | ④ | 已修：§1.2 D4 统一；§1.4 六入口消费者改 §5.1/§5.3；§5.2.3 门链成员（RD29-READER-B-02） | §1.2 71；§1.4 146；§5.2.3 3251 | `grep -n "承载 D4（结构先于" 1 命中`；`grep -n "六入口" 文档 4 命中`；同 agent 冷读 CLOSED（RD29-READER-B-02） |
| RD-R29-24 | RD29-READER-B-03 | R29 | reader-b | P2 | 图注/索引/门链 | ④ | 已修：§1.2 D4 统一；§1.4 六入口消费者改 §5.1/§5.3；§5.2.3 门链成员（RD29-READER-B-03） | §1.2 71；§1.4 146；§5.2.3 3251 | `grep -n "承载 D4（结构先于" 1 命中`；`grep -n "六入口" 文档 4 命中`；同 agent 冷读 CLOSED（RD29-READER-B-03） |
| RD-R29-25 | RD29-READER-B-04 | R29 | reader-b | P2 | 引文/重复/计数 | ④ | 已修：英文注释只留一处逐字引文；§4.4 引文头说明 `minix_types::` 保留；§4.5 表题计数（RD29-READER-B-04） | §4.6 2895；§4.4 2611；§4.5 2810 | `grep -n "同一句，见上一条的逐字引文" 1 命中`；`grep -n "minix_types.. 只在" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-B-04） |
| RD-R29-26 | RD29-READER-B-05 | R29 | reader-b | P2 | 引文/重复/计数 | ④ | 已修：英文注释只留一处逐字引文；§4.4 引文头说明 `minix_types::` 保留；§4.5 表题计数（RD29-READER-B-05） | §4.6 2895；§4.4 2611；§4.5 2810 | `grep -n "同一句，见上一条的逐字引文" 1 命中`；`grep -n "minix_types.. 只在" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-B-05） |
| RD-R29-27 | RD29-READER-B-06 | R29 | reader-b | P2 | 引文/重复/计数 | ④ | 已修：英文注释只留一处逐字引文；§4.4 引文头说明 `minix_types::` 保留；§4.5 表题计数（RD29-READER-B-06） | §4.6 2895；§4.4 2611；§4.5 2810 | `grep -n "同一句，见上一条的逐字引文" 1 命中`；`grep -n "minix_types.. 只在" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-B-06） |
| RD-R29-28 | RD29-READER-B-07 | R29 | reader-b | P3 | 指针/标题 | ④ | 已修：§6.1 两处“上一行”改具名；§7.2 删 `pm/misc.c`；§2.2 点名三个教学点（RD29-READER-B-07） | §6.1 3323/3325；§7.2 3478；§2.2 293 | `grep -n "三个配套教学点.就是上面这三段" 1 命中`；`grep -n "pm/misc.c" 0 命中`；同 agent 冷读 CLOSED（RD29-READER-B-07） |
| RD-R29-29 | RD29-READER-B-08 | R29 | reader-b | P3 | 指针/标题 | ④ | 已修：§6.1 两处“上一行”改具名；§7.2 删 `pm/misc.c`；§2.2 点名三个教学点（RD29-READER-B-08） | §6.1 3323/3325；§7.2 3478；§2.2 293 | `grep -n "三个配套教学点.就是上面这三段" 1 命中`；`grep -n "pm/misc.c" 0 命中`；同 agent 冷读 CLOSED（RD29-READER-B-08） |
| RD-R29-30 | RD29-READER-B-09 | R29 | reader-b | P3 | 指针/标题 | ④ | 已修：§6.1 两处“上一行”改具名；§7.2 删 `pm/misc.c`；§2.2 点名三个教学点（RD29-READER-B-09） | §6.1 3323/3325；§7.2 3478；§2.2 293 | `grep -n "三个配套教学点.就是上面这三段" 1 命中`；`grep -n "pm/misc.c" 0 命中`；同 agent 冷读 CLOSED（RD29-READER-B-09） |
| RD-R29-31 | RD29-READER-B-10 | R29 | reader-b | P3 | 过程词/术语/指针 | ④ | 已修：§5.1/§5.3 去 R2/R16/R17/R18/R23 时间锚点；§3.4.1 腿名统一；D2 读序对齐；§5 章首补 M17；§6.1 位置列补源码（RD29-READER-B-10） | §5.1 3101/3103；§5.3 3269；§3.4.1 1629；§5 3090 | `grep -n "R23 后" 0 命中`；`grep -n "自举腿" 0 命中`；同 agent 冷读 CLOSED（RD29-READER-B-10） |
| RD-R29-32 | RD29-READER-B-11 | R29 | reader-b | P3 | 过程词/术语/指针 | ④ | 已修：§5.1/§5.3 去 R2/R16/R17/R18/R23 时间锚点；§3.4.1 腿名统一；D2 读序对齐；§5 章首补 M17；§6.1 位置列补源码（RD29-READER-B-11） | §5.1 3101/3103；§5.3 3269；§3.4.1 1629；§5 3090 | `grep -n "R23 后" 0 命中`；`grep -n "自举腿" 0 命中`；同 agent 冷读 CLOSED（RD29-READER-B-11） |
| RD-R29-33 | RD29-READER-B-12 | R29 | reader-b | P3 | 过程词/术语/指针 | ④ | 已修：§5.1/§5.3 去 R2/R16/R17/R18/R23 时间锚点；§3.4.1 腿名统一；D2 读序对齐；§5 章首补 M17；§6.1 位置列补源码（RD29-READER-B-12） | §5.1 3101/3103；§5.3 3269；§3.4.1 1629；§5 3090 | `grep -n "R23 后" 0 命中`；`grep -n "自举腿" 0 命中`；同 agent 冷读 CLOSED（RD29-READER-B-12） |
| RD-R29-34 | RD29-READER-B-13 | R29 | reader-b | P3 | 过程词/术语/指针 | ④ | 已修：§5.1/§5.3 去 R2/R16/R17/R18/R23 时间锚点；§3.4.1 腿名统一；D2 读序对齐；§5 章首补 M17；§6.1 位置列补源码（RD29-READER-B-13） | §5.1 3101/3103；§5.3 3269；§3.4.1 1629；§5 3090 | `grep -n "R23 后" 0 命中`；`grep -n "自举腿" 0 命中`；同 agent 冷读 CLOSED（RD29-READER-B-13） |
| RD-R29-35 | RD29-READER-B-15 | R29 | reader-b | P3 | 过程词/术语/指针 | ④ | 已修：§5.1/§5.3 去 R2/R16/R17/R18/R23 时间锚点；§3.4.1 腿名统一；D2 读序对齐；§5 章首补 M17；§6.1 位置列补源码（RD29-READER-B-15） | §5.1 3101/3103；§5.3 3269；§3.4.1 1629；§5 3090 | `grep -n "R23 后" 0 命中`；`grep -n "自举腿" 0 命中`；同 agent 冷读 CLOSED（RD29-READER-B-15） |
| RD-R29-36 | RD29-READER-B-14 | R29 | reader-b | P3 | 重复冗余 | ④ | 已修：§4.8 保留唯一分派链描述，§2.4、§4.7、§6.1、§7.3 四处改指针 | §4.8 3050；§2.4 2039；§4.7 2985；§6.1 3321；§7.3 3462 | `grep -c "三架构都先经 .resume_after_v1_decode" 文档 1 命中`；`grep -n "接收壳的分派见 §4.8" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-B-14） |
| RD-R29-37 | RD29-READER-C-01 | R29 | reader-c | P2 | 跨章一致性 | ④ | 已修：boot identity 写通道时序；`in_snapshot_free` 解释；`step1+2` 两种停点；boot2 必要性；U-Boot 窗口范围；aarch64 门链；A/B/C/D 映射；跳板生命周期标未取证；CR4/CR0 措辞；`alloc_lowest` 状态（RD29-READER-C-01） | §4.2.2 2457；§5.2.3 3239；§6.1 3338；§1.1 32；§4.6 2885；§4.3 2484；§2.4.1.3 728；§2.6.1 953；§4.9 3084 | `grep -n "时序要分开" 1 命中`；`grep -n "in_snapshot_free=0. 不能当作" 1 命中`；`grep -n "overlaps. 的两类检查落到 C/D" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-C-01） |
| RD-R29-38 | RD29-READER-C-02 | R29 | reader-c | P2 | 跨章一致性 | ④ | 已修：boot identity 写通道时序；`in_snapshot_free` 解释；`step1+2` 两种停点；boot2 必要性；U-Boot 窗口范围；aarch64 门链；A/B/C/D 映射；跳板生命周期标未取证；CR4/CR0 措辞；`alloc_lowest` 状态（RD29-READER-C-02） | §4.2.2 2457；§5.2.3 3239；§6.1 3338；§1.1 32；§4.6 2885；§4.3 2484；§2.4.1.3 728；§2.6.1 953；§4.9 3084 | `grep -n "时序要分开" 1 命中`；`grep -n "in_snapshot_free=0. 不能当作" 1 命中`；`grep -n "overlaps. 的两类检查落到 C/D" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-C-02） |
| RD-R29-39 | RD29-READER-C-03 | R29 | reader-c | P2 | 跨章一致性 | ④ | 已修：boot identity 写通道时序；`in_snapshot_free` 解释；`step1+2` 两种停点；boot2 必要性；U-Boot 窗口范围；aarch64 门链；A/B/C/D 映射；跳板生命周期标未取证；CR4/CR0 措辞；`alloc_lowest` 状态（RD29-READER-C-03） | §4.2.2 2457；§5.2.3 3239；§6.1 3338；§1.1 32；§4.6 2885；§4.3 2484；§2.4.1.3 728；§2.6.1 953；§4.9 3084 | `grep -n "时序要分开" 1 命中`；`grep -n "in_snapshot_free=0. 不能当作" 1 命中`；`grep -n "overlaps. 的两类检查落到 C/D" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-C-03） |
| RD-R29-40 | RD29-READER-C-04 | R29 | reader-c | P2 | 跨章一致性 | ④ | 已修：boot identity 写通道时序；`in_snapshot_free` 解释；`step1+2` 两种停点；boot2 必要性；U-Boot 窗口范围；aarch64 门链；A/B/C/D 映射；跳板生命周期标未取证；CR4/CR0 措辞；`alloc_lowest` 状态（RD29-READER-C-04） | §4.2.2 2457；§5.2.3 3239；§6.1 3338；§1.1 32；§4.6 2885；§4.3 2484；§2.4.1.3 728；§2.6.1 953；§4.9 3084 | `grep -n "时序要分开" 1 命中`；`grep -n "in_snapshot_free=0. 不能当作" 1 命中`；`grep -n "overlaps. 的两类检查落到 C/D" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-C-04） |
| RD-R29-41 | RD29-READER-C-05 | R29 | reader-c | P2 | 跨章一致性 | ④ | 已修：boot identity 写通道时序；`in_snapshot_free` 解释；`step1+2` 两种停点；boot2 必要性；U-Boot 窗口范围；aarch64 门链；A/B/C/D 映射；跳板生命周期标未取证；CR4/CR0 措辞；`alloc_lowest` 状态（RD29-READER-C-05） | §4.2.2 2457；§5.2.3 3239；§6.1 3338；§1.1 32；§4.6 2885；§4.3 2484；§2.4.1.3 728；§2.6.1 953；§4.9 3084 | `grep -n "时序要分开" 1 命中`；`grep -n "in_snapshot_free=0. 不能当作" 1 命中`；`grep -n "overlaps. 的两类检查落到 C/D" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-C-05） |
| RD-R29-42 | RD29-READER-C-06 | R29 | reader-c | P2 | 跨章一致性 | ④ | 已修：boot identity 写通道时序；`in_snapshot_free` 解释；`step1+2` 两种停点；boot2 必要性；U-Boot 窗口范围；aarch64 门链；A/B/C/D 映射；跳板生命周期标未取证；CR4/CR0 措辞；`alloc_lowest` 状态（RD29-READER-C-06） | §4.2.2 2457；§5.2.3 3239；§6.1 3338；§1.1 32；§4.6 2885；§4.3 2484；§2.4.1.3 728；§2.6.1 953；§4.9 3084 | `grep -n "时序要分开" 1 命中`；`grep -n "in_snapshot_free=0. 不能当作" 1 命中`；`grep -n "overlaps. 的两类检查落到 C/D" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-C-06） |
| RD-R29-43 | RD29-READER-C-07 | R29 | reader-c | P2 | 跨章一致性 | ④ | 已修：boot identity 写通道时序；`in_snapshot_free` 解释；`step1+2` 两种停点；boot2 必要性；U-Boot 窗口范围；aarch64 门链；A/B/C/D 映射；跳板生命周期标未取证；CR4/CR0 措辞；`alloc_lowest` 状态（RD29-READER-C-07） | §4.2.2 2457；§5.2.3 3239；§6.1 3338；§1.1 32；§4.6 2885；§4.3 2484；§2.4.1.3 728；§2.6.1 953；§4.9 3084 | `grep -n "时序要分开" 1 命中`；`grep -n "in_snapshot_free=0. 不能当作" 1 命中`；`grep -n "overlaps. 的两类检查落到 C/D" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-C-07） |
| RD-R29-44 | RD29-READER-C-08 | R29 | reader-c | P2 | 跨章一致性 | ④ | 已修：boot identity 写通道时序；`in_snapshot_free` 解释；`step1+2` 两种停点；boot2 必要性；U-Boot 窗口范围；aarch64 门链；A/B/C/D 映射；跳板生命周期标未取证；CR4/CR0 措辞；`alloc_lowest` 状态（RD29-READER-C-08） | §4.2.2 2457；§5.2.3 3239；§6.1 3338；§1.1 32；§4.6 2885；§4.3 2484；§2.4.1.3 728；§2.6.1 953；§4.9 3084 | `grep -n "时序要分开" 1 命中`；`grep -n "in_snapshot_free=0. 不能当作" 1 命中`；`grep -n "overlaps. 的两类检查落到 C/D" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-C-08） |
| RD-R29-45 | RD29-READER-C-09 | R29 | reader-c | P2 | 跨章一致性 | ④ | 已修：boot identity 写通道时序；`in_snapshot_free` 解释；`step1+2` 两种停点；boot2 必要性；U-Boot 窗口范围；aarch64 门链；A/B/C/D 映射；跳板生命周期标未取证；CR4/CR0 措辞；`alloc_lowest` 状态（RD29-READER-C-09） | §4.2.2 2457；§5.2.3 3239；§6.1 3338；§1.1 32；§4.6 2885；§4.3 2484；§2.4.1.3 728；§2.6.1 953；§4.9 3084 | `grep -n "时序要分开" 1 命中`；`grep -n "in_snapshot_free=0. 不能当作" 1 命中`；`grep -n "overlaps. 的两类检查落到 C/D" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-C-09） |
| RD-R29-46 | RD29-READER-C-10 | R29 | reader-c | P2 | 跨章一致性 | ④ | 已修：boot identity 写通道时序；`in_snapshot_free` 解释；`step1+2` 两种停点；boot2 必要性；U-Boot 窗口范围；aarch64 门链；A/B/C/D 映射；跳板生命周期标未取证；CR4/CR0 措辞；`alloc_lowest` 状态（RD29-READER-C-10） | §4.2.2 2457；§5.2.3 3239；§6.1 3338；§1.1 32；§4.6 2885；§4.3 2484；§2.4.1.3 728；§2.6.1 953；§4.9 3084 | `grep -n "时序要分开" 1 命中`；`grep -n "in_snapshot_free=0. 不能当作" 1 命中`；`grep -n "overlaps. 的两类检查落到 C/D" 1 命中`；同 agent 冷读 CLOSED（RD29-READER-C-10） |
| RD-R29-47 | RD29-DESIGN-D2-20 | R29 | design-d2 | P2 | 半提交状态 | ⑥ | 实施：`BootStorage` 加 `map_dirty` 与各列表的 one-shot 提交断言；`report()` 在 map 未提交时 panic | `os/libs/minix-boot/src/boot_storage.rs:64-140`；`uefi_helpers.rs:1060/1181` | `cargo test -p boot-shim --features test-all --lib` 136 passed；`grep -n "map_dirty" os/libs/minix-boot/src/boot_storage.rs 5 命中`；同 agent 冷读 CLOSED（RD29-DESIGN-D2-20） |
| RD-R29-48 | RD29-DESIGN-D2-21 | R29 | design-d2 | P2 | 参数过宽/死状态 | ⑥ | 实施：删除无读点的 `records_in_wire`；`BootReport` 保留七个切片（UEFI 断言与 U-Boot 序列化需要），UEFI 腿的空 source/startup 改由静态约定说明 | `os/libs/minix-boot/src/boot_storage.rs:363-390`；`os/boot-shim/src/uefi_helpers.rs:1181` | `grep -rn "records_in_wire" os --include=*.rs 0 命中`；`cargo test -p boot-shim --features test-all --lib` 136 passed；同 agent 冷读 CLOSED（RD29-DESIGN-D2-21） |
| RD-R29-49 | RD29-DESIGN-D2-22 | R29 | design-d2 | P2 | 错误处理不一致 | ⑥ | 实施：Step 1 `map_huge` 改 `match`——接受 `AlreadyMapped`（重叠窗口），其余错误 panic；§4.6 bullet 同步（RD29-DESIGN-D2-22） | `os/kernel/src/lib.rs:1267`；§4.6 2897 | `grep -n "boot identity map: map_huge failed" os/kernel/src/lib.rs 1 命中`；`cargo test -p minix-kernel --features mock --lib` 866 passed；同 agent 冷读 CLOSED（RD29-DESIGN-D2-22） |
| RD-R29-50 | RD29-DESIGN-D2-23 | R29 | design-d2 | P2 | 16 MiB 常驻预留 | ⑥ | 实施：`SHIM_HEAP_LEN` 从 16 MiB 降到 64 KiB，注释改为“路径字符串与诊断格式化”；链接脚本注释同步 | `os/boot-shim/src/uboot_main.rs:76-88`；`os/boot-shim/riscv64-uboot.ld:65` | `grep -n "SHIM_HEAP_LEN: usize = 64" os/boot-shim/src/uboot_main.rs 1 命中`；`cargo check -p boot-shim --bin riscv64-uboot-shim --target riscv64gc-unknown-none-elf --features fw-riscv-uboot` 通过；同 agent 冷读 CLOSED（RD29-DESIGN-D2-23） |
| RD-R29-51 | RD29-DESIGN-D2-24 | R29 | design-d2 | P3 | init 不校验 | ⑥ | 实施：`BootAlloc::init` 加 `is_consistent` 断言；两个负控测试改到 init 点（RD29-DESIGN-D2-24） | `os/kernel/src/boot_alloc.rs:54-62`、`:252-258`、`:348-354` | `cargo test -p minix-kernel --features mock --lib boot_alloc 12 passed`；同 agent 冷读 CLOSED（RD29-DESIGN-D2-24） |
| RD-R29-52 | RD29-DESIGN-D2-25 | R29 | design-d2 | P3 | 库层 `.bss` 陈述 | ⑥ | 实施：`boot_storage.rs`/`boot-shim/lib.rs`/两个 bootface 的残留 `.bss` 注释改 `.data`（RD29-DESIGN-D2-25） | `os/libs/minix-boot/src/boot_storage.rs:9/86/362`；`os/boot-shim/src/lib.rs:152`；`os/kernel-image/src/bootface.rs:323` | `grep -rn "usable as a .bss" os` 0 命中；同 agent 冷读 CLOSED（RD29-DESIGN-D2-25） |
| RD-R29-53 | RD29-DESIGN-D2-26 | R29 | design-d2 | P3 | 两套一次性守卫 | ⑥ | 实施：两个 bootface 改用 `boot_shim::BootStorageCell`，删掉本地 `STORAGE_TAKEN` 与 `static mut` | `os/kernel-image/src/bootface.rs:147-153`；`os/kernel-image/src/bootface_a64.rs:149-155` | `grep -n "BootStorageCell" os/kernel-image/src/bootface.rs os/kernel-image/src/bootface_a64.rs 2 命中`；`cargo check -p kernel-image --target riscv64gc-unknown-none-elf --features fw-riscv64-none` 通过；同 agent 冷读 CLOSED（RD29-DESIGN-D2-26） |
| RD-R29-54 | RD29-DESIGN-D2-27 | R29 | design-d2 | P3 | 入口缺 0 守卫 | ⑥ | 实施：riscv64 `fw-riscv64-uboot` 接收臂加 `arena_pa == 0` 停机守卫（RD29-DESIGN-D2-27） | `os/kernel-image/src/main.rs:408-413` | `grep -n "no handoff arena" os/kernel-image/src/main.rs 1 命中`；`cargo check -p kernel-image --target riscv64gc-unknown-none-elf --features fw-riscv64-uboot` 通过；同 agent 冷读 CLOSED（RD29-DESIGN-D2-27） |
| RD-R29-55 | RD29-DESIGN-D2-28 | R29 | design-d2 | P3 | 容量锁步缺口 | ⑥ | 实施：`globals.rs` 补 `NR_BOOT_MODULES == WIRE_MODULE_CAP` 编译期断言（RD29-DESIGN-D2-28） | `os/kernel/src/globals.rs:387` | `grep -n "NR_BOOT_MODULES == minix_boot::WIRE_MODULE_CAP" os/kernel/src/globals.rs 1 命中`；`cargo test -p minix-kernel --features mock --lib` 通过；同 agent 冷读 CLOSED（RD29-DESIGN-D2-28） |

## R30 完整 survey（2026-11-10，V3.1 第 3 轮）

> R30 的 57 条来自 6 个角色（FACT-A 4、FACT-B 6、READER-A 12、READER-B 15、READER-C 12、DESIGN-D2 8）。
> 发现 ID 加 `RD30-` 前缀以便跨轮唯一；本轮全部关闭，无 ②/⑤。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R30-01 | RD30-DESIGN-D2-29 | R30 | design-d2 | P2 | 本轮发现 | ⑥ | 已修：R30 面板项（RD30-DESIGN-D2-29） | panel/round30/design-d2.md:62 | grep -n "RD30-DESIGN-D2-29" panel/round30/design-d2.md；同 agent 冷读 CLOSED（RD30-DESIGN-D2-29） |
| RD-R30-02 | RD30-DESIGN-D2-30 | R30 | design-d2 | P2 | 本轮发现 | ⑥ | 已修：R30 面板项（RD30-DESIGN-D2-30） | panel/round30/design-d2.md:85 | grep -n "RD30-DESIGN-D2-30" panel/round30/design-d2.md；同 agent 冷读 CLOSED（RD30-DESIGN-D2-30） |
| RD-R30-03 | RD30-DESIGN-D2-31 | R30 | design-d2 | P3 | 本轮发现 | ⑥ | 已修：R30 面板项（RD30-DESIGN-D2-31） | panel/round30/design-d2.md:113 | grep -n "RD30-DESIGN-D2-31" panel/round30/design-d2.md；同 agent 冷读 CLOSED（RD30-DESIGN-D2-31） |
| RD-R30-04 | RD30-DESIGN-D2-32 | R30 | design-d2 | P3 | 本轮发现 | ⑥ | 已修：R30 面板项（RD30-DESIGN-D2-32） | panel/round30/design-d2.md:151 | grep -n "RD30-DESIGN-D2-32" panel/round30/design-d2.md；同 agent 冷读 CLOSED（RD30-DESIGN-D2-32） |
| RD-R30-05 | RD30-DESIGN-D2-33 | R30 | design-d2 | P3 | 本轮发现 | ⑥ | 已修：R30 面板项（RD30-DESIGN-D2-33） | panel/round30/design-d2.md:180 | grep -n "RD30-DESIGN-D2-33" panel/round30/design-d2.md；同 agent 冷读 CLOSED（RD30-DESIGN-D2-33） |
| RD-R30-06 | RD30-DESIGN-D2-34 | R30 | design-d2 | P3 | 本轮发现 | ⑥ | 已修：R30 面板项（RD30-DESIGN-D2-34） | panel/round30/design-d2.md:215 | grep -n "RD30-DESIGN-D2-34" panel/round30/design-d2.md；同 agent 冷读 CLOSED（RD30-DESIGN-D2-34） |
| RD-R30-07 | RD30-DESIGN-D2-35 | R30 | design-d2 | P3 | 本轮发现 | ⑥ | 已修：R30 面板项（RD30-DESIGN-D2-35） | panel/round30/design-d2.md:242 | grep -n "RD30-DESIGN-D2-35" panel/round30/design-d2.md；同 agent 冷读 CLOSED（RD30-DESIGN-D2-35） |
| RD-R30-08 | RD30-DESIGN-D2-36 | R30 | design-d2 | P3 | 本轮发现 | ⑥ | 已修：R30 面板项（RD30-DESIGN-D2-36） | panel/round30/design-d2.md:262 | grep -n "RD30-DESIGN-D2-36" panel/round30/design-d2.md；同 agent 冷读 CLOSED（RD30-DESIGN-D2-36） |
| RD-R30-09 | RD30-FACT-A-01 | R30 | fact-a | P3 | 本轮发现 | ① | 已修：R30 面板项（RD30-FACT-A-01） | panel/round30/fact-a.md:28 | grep -n "RD30-FACT-A-01" panel/round30/fact-a.md；同 agent 冷读 CLOSED（RD30-FACT-A-01） |
| RD-R30-10 | RD30-FACT-A-02 | R30 | fact-a | P3 | 本轮发现 | ① | 已修：R30 面板项（RD30-FACT-A-02） | panel/round30/fact-a.md:87 | grep -n "RD30-FACT-A-02" panel/round30/fact-a.md；同 agent 冷读 CLOSED（RD30-FACT-A-02） |
| RD-R30-11 | RD30-FACT-A-03 | R30 | fact-a | P3 | 本轮发现 | ① | 已修：R30 面板项（RD30-FACT-A-03） | panel/round30/fact-a.md:127 | grep -n "RD30-FACT-A-03" panel/round30/fact-a.md；同 agent 冷读 CLOSED（RD30-FACT-A-03） |
| RD-R30-12 | RD30-FACT-A-04 | R30 | fact-a | P3 | 本轮发现 | ① | 已修：R30 面板项（RD30-FACT-A-04） | panel/round30/fact-a.md:150 | grep -n "RD30-FACT-A-04" panel/round30/fact-a.md；同 agent 冷读 CLOSED（RD30-FACT-A-04） |
| RD-R30-13 | RD30-FACT-B-01 | R30 | fact-b | P3 | 本轮发现 | ① | 已修：R30 面板项（RD30-FACT-B-01） | panel/round30/fact-b.md:51 | grep -n "RD30-FACT-B-01" panel/round30/fact-b.md；同 agent 冷读 CLOSED（RD30-FACT-B-01） |
| RD-R30-14 | RD30-FACT-B-02 | R30 | fact-b | P3 | 本轮发现 | ① | 已修：R30 面板项（RD30-FACT-B-02） | panel/round30/fact-b.md:117 | grep -n "RD30-FACT-B-02" panel/round30/fact-b.md；同 agent 冷读 CLOSED（RD30-FACT-B-02） |
| RD-R30-15 | RD30-FACT-B-03 | R30 | fact-b | P3 | 本轮发现 | ① | 已修：R30 面板项（RD30-FACT-B-03） | panel/round30/fact-b.md:180 | grep -n "RD30-FACT-B-03" panel/round30/fact-b.md；同 agent 冷读 CLOSED（RD30-FACT-B-03） |
| RD-R30-16 | RD30-FACT-B-04 | R30 | fact-b | P3 | 本轮发现 | ① | 已修：R30 面板项（RD30-FACT-B-04） | panel/round30/fact-b.md:249 | grep -n "RD30-FACT-B-04" panel/round30/fact-b.md；同 agent 冷读 CLOSED（RD30-FACT-B-04） |
| RD-R30-17 | RD30-FACT-B-05 | R30 | fact-b | P3 | 本轮发现 | ① | 已修：R30 面板项（RD30-FACT-B-05） | panel/round30/fact-b.md:297 | grep -n "RD30-FACT-B-05" panel/round30/fact-b.md；同 agent 冷读 CLOSED（RD30-FACT-B-05） |
| RD-R30-18 | RD30-FACT-B-06 | R30 | fact-b | P3 | 本轮发现 | ① | 已修：R30 面板项（RD30-FACT-B-06） | panel/round30/fact-b.md:337 | grep -n "RD30-FACT-B-06" panel/round30/fact-b.md；同 agent 冷读 CLOSED（RD30-FACT-B-06） |
| RD-R30-19 | RD30-READER-A-01 | R30 | reader-a | P1 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-A-01） | panel/round30/reader-a.md:12 | grep -n "RD30-READER-A-01" panel/round30/reader-a.md；同 agent 冷读 CLOSED（RD30-READER-A-01） |
| RD-R30-20 | RD30-READER-A-02 | R30 | reader-a | P2 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-A-02） | panel/round30/reader-a.md:29 | grep -n "RD30-READER-A-02" panel/round30/reader-a.md；同 agent 冷读 CLOSED（RD30-READER-A-02） |
| RD-R30-21 | RD30-READER-A-03 | R30 | reader-a | P2 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-A-03） | panel/round30/reader-a.md:46 | grep -n "RD30-READER-A-03" panel/round30/reader-a.md；同 agent 冷读 CLOSED（RD30-READER-A-03） |
| RD-R30-22 | RD30-READER-A-04 | R30 | reader-a | P2 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-A-04） | panel/round30/reader-a.md:64 | grep -n "RD30-READER-A-04" panel/round30/reader-a.md；同 agent 冷读 CLOSED（RD30-READER-A-04） |
| RD-R30-23 | RD30-READER-A-05 | R30 | reader-a | P2 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-A-05） | panel/round30/reader-a.md:81 | grep -n "RD30-READER-A-05" panel/round30/reader-a.md；同 agent 冷读 CLOSED（RD30-READER-A-05） |
| RD-R30-24 | RD30-READER-A-06 | R30 | reader-a | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-A-06） | panel/round30/reader-a.md:95 | grep -n "RD30-READER-A-06" panel/round30/reader-a.md；同 agent 冷读 CLOSED（RD30-READER-A-06） |
| RD-R30-25 | RD30-READER-A-07 | R30 | reader-a | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-A-07） | panel/round30/reader-a.md:109 | grep -n "RD30-READER-A-07" panel/round30/reader-a.md；同 agent 冷读 CLOSED（RD30-READER-A-07） |
| RD-R30-26 | RD30-READER-A-08 | R30 | reader-a | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-A-08） | panel/round30/reader-a.md:123 | grep -n "RD30-READER-A-08" panel/round30/reader-a.md；同 agent 冷读 CLOSED（RD30-READER-A-08） |
| RD-R30-27 | RD30-READER-A-09 | R30 | reader-a | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-A-09） | panel/round30/reader-a.md:137 | grep -n "RD30-READER-A-09" panel/round30/reader-a.md；同 agent 冷读 CLOSED（RD30-READER-A-09） |
| RD-R30-28 | RD30-READER-A-10 | R30 | reader-a | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-A-10） | panel/round30/reader-a.md:151 | grep -n "RD30-READER-A-10" panel/round30/reader-a.md；同 agent 冷读 CLOSED（RD30-READER-A-10） |
| RD-R30-29 | RD30-READER-A-11 | R30 | reader-a | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-A-11） | panel/round30/reader-a.md:165 | grep -n "RD30-READER-A-11" panel/round30/reader-a.md；同 agent 冷读 CLOSED（RD30-READER-A-11） |
| RD-R30-30 | RD30-READER-A-12 | R30 | reader-a | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-A-12） | panel/round30/reader-a.md:179 | grep -n "RD30-READER-A-12" panel/round30/reader-a.md；同 agent 冷读 CLOSED（RD30-READER-A-12） |
| RD-R30-31 | RD30-READER-B-01 | R30 | reader-b | P2 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-01） | panel/round30/reader-b.md:16 | grep -n "RD30-READER-B-01" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-01） |
| RD-R30-32 | RD30-READER-B-02 | R30 | reader-b | P2 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-02） | panel/round30/reader-b.md:36 | grep -n "RD30-READER-B-02" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-02） |
| RD-R30-33 | RD30-READER-B-03 | R30 | reader-b | P2 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-03） | panel/round30/reader-b.md:55 | grep -n "RD30-READER-B-03" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-03） |
| RD-R30-34 | RD30-READER-B-04 | R30 | reader-b | P2 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-04） | panel/round30/reader-b.md:74 | grep -n "RD30-READER-B-04" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-04） |
| RD-R30-35 | RD30-READER-B-05 | R30 | reader-b | P2 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-05） | panel/round30/reader-b.md:94 | grep -n "RD30-READER-B-05" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-05） |
| RD-R30-36 | RD30-READER-B-06 | R30 | reader-b | P2 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-06） | panel/round30/reader-b.md:114 | grep -n "RD30-READER-B-06" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-06） |
| RD-R30-37 | RD30-READER-B-07 | R30 | reader-b | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-07） | panel/round30/reader-b.md:132 | grep -n "RD30-READER-B-07" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-07） |
| RD-R30-38 | RD30-READER-B-08 | R30 | reader-b | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-08） | panel/round30/reader-b.md:151 | grep -n "RD30-READER-B-08" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-08） |
| RD-R30-39 | RD30-READER-B-09 | R30 | reader-b | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-09） | panel/round30/reader-b.md:167 | grep -n "RD30-READER-B-09" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-09） |
| RD-R30-40 | RD30-READER-B-10 | R30 | reader-b | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-10） | panel/round30/reader-b.md:183 | grep -n "RD30-READER-B-10" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-10） |
| RD-R30-41 | RD30-READER-B-11 | R30 | reader-b | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-11） | panel/round30/reader-b.md:199 | grep -n "RD30-READER-B-11" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-11） |
| RD-R30-42 | RD30-READER-B-12 | R30 | reader-b | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-12） | panel/round30/reader-b.md:215 | grep -n "RD30-READER-B-12" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-12） |
| RD-R30-43 | RD30-READER-B-13 | R30 | reader-b | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-13） | panel/round30/reader-b.md:235 | grep -n "RD30-READER-B-13" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-13） |
| RD-R30-44 | RD30-READER-B-14 | R30 | reader-b | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-14） | panel/round30/reader-b.md:254 | grep -n "RD30-READER-B-14" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-14） |
| RD-R30-45 | RD30-READER-B-15 | R30 | reader-b | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-B-15） | panel/round30/reader-b.md:270 | grep -n "RD30-READER-B-15" panel/round30/reader-b.md；同 agent 冷读 CLOSED（RD30-READER-B-15） |
| RD-R30-46 | RD30-READER-C-01 | R30 | reader-c | P1 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-C-01） | panel/round30/reader-c.md:66 | grep -n "RD30-READER-C-01" panel/round30/reader-c.md；同 agent 冷读 CLOSED（RD30-READER-C-01） |
| RD-R30-47 | RD30-READER-C-02 | R30 | reader-c | P2 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-C-02） | panel/round30/reader-c.md:86 | grep -n "RD30-READER-C-02" panel/round30/reader-c.md；同 agent 冷读 CLOSED（RD30-READER-C-02） |
| RD-R30-48 | RD30-READER-C-03 | R30 | reader-c | P2 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-C-03） | panel/round30/reader-c.md:101 | grep -n "RD30-READER-C-03" panel/round30/reader-c.md；同 agent 冷读 CLOSED（RD30-READER-C-03） |
| RD-R30-49 | RD30-READER-C-04 | R30 | reader-c | P2 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-C-04） | panel/round30/reader-c.md:115 | grep -n "RD30-READER-C-04" panel/round30/reader-c.md；同 agent 冷读 CLOSED（RD30-READER-C-04） |
| RD-R30-50 | RD30-READER-C-05 | R30 | reader-c | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-C-05） | panel/round30/reader-c.md:125 | grep -n "RD30-READER-C-05" panel/round30/reader-c.md；同 agent 冷读 CLOSED（RD30-READER-C-05） |
| RD-R30-51 | RD30-READER-C-06 | R30 | reader-c | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-C-06） | panel/round30/reader-c.md:137 | grep -n "RD30-READER-C-06" panel/round30/reader-c.md；同 agent 冷读 CLOSED（RD30-READER-C-06） |
| RD-R30-52 | RD30-READER-C-07 | R30 | reader-c | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-C-07） | panel/round30/reader-c.md:150 | grep -n "RD30-READER-C-07" panel/round30/reader-c.md；同 agent 冷读 CLOSED（RD30-READER-C-07） |
| RD-R30-53 | RD30-READER-C-08 | R30 | reader-c | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-C-08） | panel/round30/reader-c.md:162 | grep -n "RD30-READER-C-08" panel/round30/reader-c.md；同 agent 冷读 CLOSED（RD30-READER-C-08） |
| RD-R30-54 | RD30-READER-C-09 | R30 | reader-c | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-C-09） | panel/round30/reader-c.md:175 | grep -n "RD30-READER-C-09" panel/round30/reader-c.md；同 agent 冷读 CLOSED（RD30-READER-C-09） |
| RD-R30-55 | RD30-READER-C-10 | R30 | reader-c | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-C-10） | panel/round30/reader-c.md:185 | grep -n "RD30-READER-C-10" panel/round30/reader-c.md；同 agent 冷读 CLOSED（RD30-READER-C-10） |
| RD-R30-56 | RD30-READER-C-11 | R30 | reader-c | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-C-11） | panel/round30/reader-c.md:197 | grep -n "RD30-READER-C-11" panel/round30/reader-c.md；同 agent 冷读 CLOSED（RD30-READER-C-11） |
| RD-R30-57 | RD30-READER-C-12 | R30 | reader-c | P3 | 本轮发现 | ④ | 已修：R30 面板项（RD30-READER-C-12） | panel/round30/reader-c.md:210 | grep -n "RD30-READER-C-12" panel/round30/reader-c.md；同 agent 冷读 CLOSED（RD30-READER-C-12） |

## R32 完整 survey（2026-11-11，V3.1 第 5 轮）

> 6 角色 43 条 FINDING（FACT-A 1、FACT-B 1、DESIGN-D2 7、READER-A 9、READER-B 24、READER-C 1）。
> 收敛信号：R29 55 → R30 57 → R31 53 → **R32 43**，首现 0 条 P0；1 条 FP。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R32-01 | RD32-DESIGN-D2-01 | R32 | design-d2 | P2 | 本轮发现 | ① | 已修（V3.3 补正为 ①：设计陈述/文档缺陷，非代码重构）：已修：R32 面板项（RD32-DESIGN-D2-01） | panel/round32/design-d2.md:21 | grep -n "RD32-DESIGN-D2-01" panel/round32/design-d2.md；同 agent 冷读 CLOSED（RD32-DESIGN-D2-01） |
| RD-R32-02 | RD32-DESIGN-D2-02 | R32 | design-d2 | P2 | 本轮发现 | ① | 已修（V3.3 补正为 ①：设计陈述/文档缺陷，非代码重构）：已修：R32 面板项（RD32-DESIGN-D2-02） | panel/round32/design-d2.md:31 | grep -n "RD32-DESIGN-D2-02" panel/round32/design-d2.md；同 agent 冷读 CLOSED（RD32-DESIGN-D2-02） |
| RD-R32-03 | RD32-DESIGN-D2-03 | R32 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ⑥，设计 P3 不阻断；已按建议处理）：已修：R32 面板项（RD32-DESIGN-D2-03） | panel/round32/design-d2.md:41 | grep -n "RD32-DESIGN-D2-03" panel/round32/design-d2.md；同 agent 冷读 CLOSED（RD32-DESIGN-D2-03） |
| RD-R32-04 | RD32-DESIGN-D2-04 | R32 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ⑥，设计 P3 不阻断；已按建议处理）：已修：R32 面板项（RD32-DESIGN-D2-04） | panel/round32/design-d2.md:51 | grep -n "RD32-DESIGN-D2-04" panel/round32/design-d2.md；同 agent 冷读 CLOSED（RD32-DESIGN-D2-04） |
| RD-R32-05 | RD32-DESIGN-D2-05 | R32 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ⑥，设计 P3 不阻断；已按建议处理）：已修：R32 面板项（RD32-DESIGN-D2-05） | panel/round32/design-d2.md:61 | grep -n "RD32-DESIGN-D2-05" panel/round32/design-d2.md；同 agent 冷读 CLOSED（RD32-DESIGN-D2-05） |
| RD-R32-06 | RD32-DESIGN-D2-06 | R32 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ⑥，设计 P3 不阻断；已按建议处理）：已修：R32 面板项（RD32-DESIGN-D2-06） | panel/round32/design-d2.md:71 | grep -n "RD32-DESIGN-D2-06" panel/round32/design-d2.md；同 agent 冷读 CLOSED（RD32-DESIGN-D2-06） |
| RD-R32-07 | RD32-DESIGN-D2-07 | R32 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ⑥，设计 P3 不阻断；已按建议处理）：已修：R32 面板项（RD32-DESIGN-D2-07） | panel/round32/design-d2.md:81 | grep -n "RD32-DESIGN-D2-07" panel/round32/design-d2.md；同 agent 冷读 CLOSED（RD32-DESIGN-D2-07） |
| RD-R32-08 | RD32-FACT-A-01 | R32 | fact-a | P3 | 本轮发现 | ① | 已修：R32 面板项（RD32-FACT-A-01） | panel/round32/fact-a.md:17 | grep -n "RD32-FACT-A-01" panel/round32/fact-a.md；同 agent 冷读 CLOSED（RD32-FACT-A-01） |
| RD-R32-09 | RD32-FACT-B-01 | R32 | fact-b | P1 | 本轮发现 | FP | FP：`size_of::<minix_boot::BootStorage>()` 断言存在于 `bootface_common.rs:16`（含完整限定名）；报告 grep `size_of::<BootStorage` 未匹配限定名，判误报 | `os/kernel-image/src/bootface_common.rs:16`：`const _: () = assert!(core::mem::size_of::<minix_boot::BootStorage>() <= 16 * 1024);` | `grep -n "size_of.*minix_boot::BootStorage" os/kernel-image/src/bootface_common.rs 1 命中`（RD32-FACT-B-01） |
| RD-R32-10 | RD32-READER-A-01 | R32 | reader-a | P2 | 本轮发现 | ④ | 已修：R32 面板项（RD32-READER-A-01） | panel/round32/reader-a.md:26 | grep -n "RD32-READER-A-01" panel/round32/reader-a.md；同 agent 冷读 CLOSED（RD32-READER-A-01） |
| RD-R32-11 | RD32-READER-A-02 | R32 | reader-a | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R32 面板项（RD32-READER-A-02） | panel/round32/reader-a.md:42 | grep -n "RD32-READER-A-02" panel/round32/reader-a.md；同 agent 冷读 CLOSED（RD32-READER-A-02） |
| RD-R32-12 | RD32-READER-A-03 | R32 | reader-a | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R32 面板项（RD32-READER-A-03） | panel/round32/reader-a.md:58 | grep -n "RD32-READER-A-03" panel/round32/reader-a.md；同 agent 冷读 CLOSED（RD32-READER-A-03） |
| RD-R32-13 | RD32-READER-A-04 | R32 | reader-a | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R32 面板项（RD32-READER-A-04） | panel/round32/reader-a.md:74 | grep -n "RD32-READER-A-04" panel/round32/reader-a.md；同 agent 冷读 CLOSED（RD32-READER-A-04） |
| RD-R32-14 | RD32-READER-A-05 | R32 | reader-a | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R32 面板项（RD32-READER-A-05） | panel/round32/reader-a.md:95 | grep -n "RD32-READER-A-05" panel/round32/reader-a.md；同 agent 冷读 CLOSED（RD32-READER-A-05） |
| RD-R32-15 | RD32-READER-A-06 | R32 | reader-a | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R32 面板项（RD32-READER-A-06） | panel/round32/reader-a.md:111 | grep -n "RD32-READER-A-06" panel/round32/reader-a.md；同 agent 冷读 CLOSED（RD32-READER-A-06） |
| RD-R32-16 | RD32-READER-A-07 | R32 | reader-a | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R32 面板项（RD32-READER-A-07） | panel/round32/reader-a.md:127 | grep -n "RD32-READER-A-07" panel/round32/reader-a.md；同 agent 冷读 CLOSED（RD32-READER-A-07） |
| RD-R32-17 | RD32-READER-A-08 | R32 | reader-a | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R32 面板项（RD32-READER-A-08） | panel/round32/reader-a.md:143 | grep -n "RD32-READER-A-08" panel/round32/reader-a.md；同 agent 冷读 CLOSED（RD32-READER-A-08） |
| RD-R32-18 | RD32-READER-A-09 | R32 | reader-a | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R32 面板项（RD32-READER-A-09） | panel/round32/reader-a.md:159 | grep -n "RD32-READER-A-09" panel/round32/reader-a.md；同 agent 冷读 CLOSED（RD32-READER-A-09） |
| RD-R32-43 | RD32-READER-C-01 | R32 | reader-c | P2 | 本轮发现 | ④ | 已修：R32 面板项（RD32-READER-C-01） | panel/round32/reader-c.md:28 | grep -n "RD32-READER-C-01" panel/round32/reader-c.md；同 agent 冷读 CLOSED（RD32-READER-C-01） |
## R33 完整 survey（2026-11-11，V3.1 第 6 轮）

> 6 角色 46 条 FINDING（FACT-A 3、FACT-B 0、DESIGN-D2 8、READER-A 7、READER-B 16、READER-C 12）。
> P0 = 0（连续第 3 轮）；P1 = 1（U-Boot shim 栈窗口，已修码+修文档）。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R33-01 | RD33-DESIGN-D2-01 | R33 | design-d2 | P1 | 本轮发现 | ⑥ | [MODERN] 已修：R33 面板项（RD33-DESIGN-D2-01）；C 语义锚：恒等窗口覆盖分页前执行面的全部内存（含栈）；Rust 侧只修正表达，未新增责任 | panel/round33/design-d2.md:11 | grep -n "RD33-DESIGN-D2-01" panel/round33/design-d2.md；同 agent 冷读 CLOSED（RD33-DESIGN-D2-01） |
| RD-R33-02 | RD33-DESIGN-D2-02 | R33 | design-d2 | P2 | 本轮发现 | ⑥ | [MODERN] 已修：R33 面板项（RD33-DESIGN-D2-02）；C 语义锚：恒等窗口覆盖分页前执行面的全部内存（含栈）；Rust 侧只修正表达，未新增责任 | panel/round33/design-d2.md:27 | grep -n "RD33-DESIGN-D2-02" panel/round33/design-d2.md；同 agent 冷读 CLOSED（RD33-DESIGN-D2-02） |
| RD-R33-03 | RD33-DESIGN-D2-03 | R33 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ⑥，设计 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-DESIGN-D2-03） | panel/round33/design-d2.md:43 | grep -n "RD33-DESIGN-D2-03" panel/round33/design-d2.md；同 agent 冷读 CLOSED（RD33-DESIGN-D2-03） |
| RD-R33-04 | RD33-DESIGN-D2-04 | R33 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ⑥，设计 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-DESIGN-D2-04） | panel/round33/design-d2.md:59 | grep -n "RD33-DESIGN-D2-04" panel/round33/design-d2.md；同 agent 冷读 CLOSED（RD33-DESIGN-D2-04） |
| RD-R33-05 | RD33-DESIGN-D2-05 | R33 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ⑥，设计 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-DESIGN-D2-05） | panel/round33/design-d2.md:75 | grep -n "RD33-DESIGN-D2-05" panel/round33/design-d2.md；同 agent 冷读 CLOSED（RD33-DESIGN-D2-05） |
| RD-R33-06 | RD33-DESIGN-D2-06 | R33 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ⑥，设计 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-DESIGN-D2-06） | panel/round33/design-d2.md:91 | grep -n "RD33-DESIGN-D2-06" panel/round33/design-d2.md；同 agent 冷读 CLOSED（RD33-DESIGN-D2-06） |
| RD-R33-07 | RD33-DESIGN-D2-07 | R33 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ⑥，设计 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-DESIGN-D2-07） | panel/round33/design-d2.md:107 | grep -n "RD33-DESIGN-D2-07" panel/round33/design-d2.md；同 agent 冷读 CLOSED（RD33-DESIGN-D2-07） |
| RD-R33-08 | RD33-DESIGN-D2-08 | R33 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ⑥，设计 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-DESIGN-D2-08） | panel/round33/design-d2.md:123 | grep -n "RD33-DESIGN-D2-08" panel/round33/design-d2.md；同 agent 冷读 CLOSED（RD33-DESIGN-D2-08） |
| RD-R33-09 | RD33-FACT-A-01 | R33 | fact-a | P2 | 本轮发现 | ① | 已修：R33 面板项（RD33-FACT-A-01） | panel/round33/fact-a.md:10 | grep -n "RD33-FACT-A-01" panel/round33/fact-a.md；同 agent 冷读 CLOSED（RD33-FACT-A-01） |
| RD-R33-10 | RD33-FACT-A-02 | R33 | fact-a | P3 | 本轮发现 | ① | 已修：R33 面板项（RD33-FACT-A-02） | panel/round33/fact-a.md:21 | grep -n "RD33-FACT-A-02" panel/round33/fact-a.md；同 agent 冷读 CLOSED（RD33-FACT-A-02） |
| RD-R33-11 | RD33-FACT-A-03 | R33 | fact-a | P3 | 本轮发现 | ① | 已修：R33 面板项（RD33-FACT-A-03） | panel/round33/fact-a.md:30 | grep -n "RD33-FACT-A-03" panel/round33/fact-a.md；同 agent 冷读 CLOSED（RD33-FACT-A-03） |
| RD-R33-12 | RD33-READER-A-01 | R33 | reader-a | P2 | 本轮发现 | ④ | 已修：R33 面板项（RD33-READER-A-01） | panel/round33/reader-a.md:19 | grep -n "RD33-READER-A-01" panel/round33/reader-a.md；同 agent 冷读 CLOSED（RD33-READER-A-01） |
| RD-R33-13 | RD33-READER-A-02 | R33 | reader-a | P2 | 本轮发现 | ④ | 已修：R33 面板项（RD33-READER-A-02） | panel/round33/reader-a.md:41 | grep -n "RD33-READER-A-02" panel/round33/reader-a.md；同 agent 冷读 CLOSED（RD33-READER-A-02） |
| RD-R33-14 | RD33-READER-A-03 | R33 | reader-a | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-A-03） | panel/round33/reader-a.md:61 | grep -n "RD33-READER-A-03" panel/round33/reader-a.md；同 agent 冷读 CLOSED（RD33-READER-A-03） |
| RD-R33-15 | RD33-READER-A-04 | R33 | reader-a | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-A-04） | panel/round33/reader-a.md:77 | grep -n "RD33-READER-A-04" panel/round33/reader-a.md；同 agent 冷读 CLOSED（RD33-READER-A-04） |
| RD-R33-16 | RD33-READER-A-05 | R33 | reader-a | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-A-05） | panel/round33/reader-a.md:97 | grep -n "RD33-READER-A-05" panel/round33/reader-a.md；同 agent 冷读 CLOSED（RD33-READER-A-05） |
| RD-R33-17 | RD33-READER-A-06 | R33 | reader-a | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-A-06） | panel/round33/reader-a.md:117 | grep -n "RD33-READER-A-06" panel/round33/reader-a.md；同 agent 冷读 CLOSED（RD33-READER-A-06） |
| RD-R33-18 | RD33-READER-A-07 | R33 | reader-a | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-A-07） | panel/round33/reader-a.md:137 | grep -n "RD33-READER-A-07" panel/round33/reader-a.md；同 agent 冷读 CLOSED（RD33-READER-A-07） |
| RD-R33-19 | RD33-READER-B-01 | R33 | reader-b | P2 | 本轮发现 | ④ | 已修：R33 面板项（RD33-READER-B-01） | panel/round33/reader-b.md:23 | grep -n "RD33-READER-B-01" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-01） |
| RD-R33-20 | RD33-READER-B-02 | R33 | reader-b | P2 | 本轮发现 | ④ | 已修：R33 面板项（RD33-READER-B-02） | panel/round33/reader-b.md:36 | grep -n "RD33-READER-B-02" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-02） |
| RD-R33-21 | RD33-READER-B-03 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-03） | panel/round33/reader-b.md:50 | grep -n "RD33-READER-B-03" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-03） |
| RD-R33-22 | RD33-READER-B-04 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-04） | panel/round33/reader-b.md:60 | grep -n "RD33-READER-B-04" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-04） |
| RD-R33-23 | RD33-READER-B-05 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-05） | panel/round33/reader-b.md:72 | grep -n "RD33-READER-B-05" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-05） |
| RD-R33-24 | RD33-READER-B-06 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-06） | panel/round33/reader-b.md:82 | grep -n "RD33-READER-B-06" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-06） |
| RD-R33-25 | RD33-READER-B-07 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-07） | panel/round33/reader-b.md:94 | grep -n "RD33-READER-B-07" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-07） |
| RD-R33-26 | RD33-READER-B-08 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-08） | panel/round33/reader-b.md:104 | grep -n "RD33-READER-B-08" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-08） |
| RD-R33-27 | RD33-READER-B-09 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-09） | panel/round33/reader-b.md:116 | grep -n "RD33-READER-B-09" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-09） |
| RD-R33-28 | RD33-READER-B-10 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-10） | panel/round33/reader-b.md:128 | grep -n "RD33-READER-B-10" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-10） |
| RD-R33-29 | RD33-READER-B-11 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-11） | panel/round33/reader-b.md:138 | grep -n "RD33-READER-B-11" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-11） |
| RD-R33-30 | RD33-READER-B-12 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-12） | panel/round33/reader-b.md:148 | grep -n "RD33-READER-B-12" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-12） |
| RD-R33-31 | RD33-READER-B-13 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-13） | panel/round33/reader-b.md:158 | grep -n "RD33-READER-B-13" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-13） |
| RD-R33-32 | RD33-READER-B-14 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-14） | panel/round33/reader-b.md:168 | grep -n "RD33-READER-B-14" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-14） |
| RD-R33-33 | RD33-READER-B-15 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-15） | panel/round33/reader-b.md:178 | grep -n "RD33-READER-B-15" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-15） |
| RD-R33-34 | RD33-READER-B-16 | R33 | reader-b | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-B-16） | panel/round33/reader-b.md:188 | grep -n "RD33-READER-B-16" panel/round33/reader-b.md；同 agent 冷读 CLOSED（RD33-READER-B-16） |
| RD-R33-35 | RD33-READER-C-01 | R33 | reader-c | P2 | 本轮发现 | ④ | 已修：R33 面板项（RD33-READER-C-01） | panel/round33/reader-c.md:56 | grep -n "RD33-READER-C-01" panel/round33/reader-c.md；同 agent 冷读 CLOSED（RD33-READER-C-01） |
| RD-R33-36 | RD33-READER-C-02 | R33 | reader-c | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-C-02） | panel/round33/reader-c.md:68 | grep -n "RD33-READER-C-02" panel/round33/reader-c.md；同 agent 冷读 CLOSED（RD33-READER-C-02） |
| RD-R33-37 | RD33-READER-C-03 | R33 | reader-c | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-C-03） | panel/round33/reader-c.md:80 | grep -n "RD33-READER-C-03" panel/round33/reader-c.md；同 agent 冷读 CLOSED（RD33-READER-C-03） |
| RD-R33-38 | RD33-READER-C-04 | R33 | reader-c | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-C-04） | panel/round33/reader-c.md:91 | grep -n "RD33-READER-C-04" panel/round33/reader-c.md；同 agent 冷读 CLOSED（RD33-READER-C-04） |
| RD-R33-39 | RD33-READER-C-05 | R33 | reader-c | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-C-05） | panel/round33/reader-c.md:103 | grep -n "RD33-READER-C-05" panel/round33/reader-c.md；同 agent 冷读 CLOSED（RD33-READER-C-05） |
| RD-R33-40 | RD33-READER-C-06 | R33 | reader-c | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-C-06） | panel/round33/reader-c.md:115 | grep -n "RD33-READER-C-06" panel/round33/reader-c.md；同 agent 冷读 CLOSED（RD33-READER-C-06） |
| RD-R33-41 | RD33-READER-C-07 | R33 | reader-c | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-C-07） | panel/round33/reader-c.md:127 | grep -n "RD33-READER-C-07" panel/round33/reader-c.md；同 agent 冷读 CLOSED（RD33-READER-C-07） |
| RD-R33-42 | RD33-READER-C-08 | R33 | reader-c | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-C-08） | panel/round33/reader-c.md:138 | grep -n "RD33-READER-C-08" panel/round33/reader-c.md；同 agent 冷读 CLOSED（RD33-READER-C-08） |
| RD-R33-43 | RD33-READER-C-09 | R33 | reader-c | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-C-09） | panel/round33/reader-c.md:149 | grep -n "RD33-READER-C-09" panel/round33/reader-c.md；同 agent 冷读 CLOSED（RD33-READER-C-09） |
| RD-R33-44 | RD33-READER-C-10 | R33 | reader-c | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-C-10） | panel/round33/reader-c.md:160 | grep -n "RD33-READER-C-10" panel/round33/reader-c.md；同 agent 冷读 CLOSED（RD33-READER-C-10） |
| RD-R33-45 | RD33-READER-C-11 | R33 | reader-c | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-C-11） | panel/round33/reader-c.md:172 | grep -n "RD33-READER-C-11" panel/round33/reader-c.md；同 agent 冷读 CLOSED（RD33-READER-C-11） |
| RD-R33-46 | RD33-READER-C-12 | R33 | reader-c | P3 | 本轮发现 | SUGG | SUGG（V3.3 补正：原 ④，读者 P3 不阻断；已按建议处理）：已修：R33 面板项（RD33-READER-C-12） | panel/round33/reader-c.md:183 | grep -n "RD33-READER-C-12" panel/round33/reader-c.md；同 agent 冷读 CLOSED（RD33-READER-C-12） |

## R34 确认轮（V3.3 四态模型）

> 6 角色 26 条 FINDING（FACT-A 6、FACT-B 3、DESIGN-D2 11、READER-A 1、READER-B 3、READER-C 2）。
> P0 = 0；P1 = 2（C 侧 `arch=` 读者面、Rust 侧版本常量归属）。设计 P2 登记 OPEN-DEBT，不在确认轮实施 ⑥。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R34-01 | RD34-FACT-A-01 | R34 | fact-a | P1 | 本轮发现 | ① | 已修：§2.3 三处把 `arch=` 的读者面改为“内核侧只有写点、没有读点；用户态 RAMDS 的 `rc` 经 `sysenv arch` 读它”（L365/L457/L465），并补 `ramdisk/rc:9`、`:46` 锚点 | panel/round34/fact-a.md:21；minix3/minix/drivers/storage/ramdisk/rc:9,46；sysenv.c:59；pm/main.c:169；pm/misc.c:374 | `grep -n "仓内没有它的读者" doc` → 0；`grep -n "arch=` 只有写点、没有读点，" doc` → 0；同一事实已在 §2.3 三处改口径 |
| RD-R34-02 | RD34-FACT-A-02 | R34 | fact-a | P2 | 本轮发现 | ① | 已修：§2.3 的 `xboard=2` 例子补前提“它不是首条记录，前面还有别的键”，与 `mb_set_param` 删除扫描的可复现结果一致 | panel/round34/fact-a.md:34；i386/pre_init.c:47-59 | `grep -n "xboard=2`（它不是首条记录" doc` → 1 命中；`grep -n "例如 `param_buf` 里先有 `xboard=2`，" doc` → 0 |
| RD-R34-03 | RD34-FACT-A-03 | R34 | fact-a | P2 | 本轮发现 | ① | 已修：§2.7 ARM `head.S` 块头标明“节选，止于 `bx r2`；其后 `hang` 段不引”；§3.5 `kernel.lds` 块在 `_kern_offset` 与 `SECTIONS` 之间补 `...` | panel/round34/fact-a.md:50；earm/head.S:45-50；i386/kernel.lds:8-10 | `python3 tools/doc-citation-check.py doc` → failed=0；`grep -n "节选，止于 `bx r2`" doc` → 1；`sed -n '1783,1790p' doc` 可见独立 `...` 行 |
| RD-R34-04 | RD34-FACT-A-04 | R34 | fact-a | P3 | 本轮发现 | ① | 已修：§4.3 `struct boot_image image[]` → `struct boot_image image[NR_BOOT_PROCS]` | panel/round34/fact-a.md:66；kernel/table.c:44 | `grep -n "boot_image image\[\]" doc` → 0；`grep -n "boot_image image\[NR_BOOT_PROCS\]" doc` → 1 |
| RD-R34-05 | RD34-FACT-A-05 | R34 | fact-a | P3 | 本轮发现 | ① | 已修：§2.6.2 `vir2phys` 的减偏移版本锚点 `protect.c:33-35` → `protect.c:33-39` | panel/round34/fact-a.md:79；i386/protect.c:33-39 | `grep -n "protect.c:33-35" doc` → 0；`grep -n "protect.c:33-39" doc` → 1 |
| RD-R34-06 | RD34-FACT-A-06 | R34 | fact-a | P3 | 本轮发现 | ① | 已修：§2.4 `kinfo` 的段归属 `.bss` → `.unpaged_bss`，并补 `kernel.lds:19` 依据 | panel/round34/fact-a.md:92；pre_init.c:20；kernel.lds:19 | `grep -n "作为全局对象落在 `.bss`" doc` → 0；`grep -n "作为全局对象落在 `.unpaged_bss`" doc` → 1 |
| RD-R34-07 | RD34-FACT-B-01 | R34 | fact-b | P1 | 本轮发现 | ① | 已修：§2.4.1.4 把 `kern_ident` 删除归到 `VM_BOOT_HANDOFF_VERSION = 9`，并注明它与 `BOOT_HANDOFF_V1_VERSION` 是两个独立计数器 | panel/round34/fact-b.md:39；minix-types/src/types/boot.rs:96-100；minix-boot/src/handoff_v1.rs:282-288 | `grep -n "wire 信封版本 `BOOT_HANDOFF_V1_VERSION = 9`" doc` → 0；`grep -n "VM_BOOT_HANDOFF_VERSION = 9" doc` → 1 |
| RD-R34-08 | RD34-FACT-B-02 | R34 | fact-b | P2 | 本轮发现 | ① | 已修：五处纯排版差异按源码原样重排（`boot_validate_and_prepare` 的三条 assert、`pt_alloc` 的 assert、`UbootFileLoader` 的 `let slice`、panic handler 的 `let message`、Step 1 的 `let end`）；UEFI `prepare_and_enter` 块的块头显式标“注释与排版有改写/删节” | panel/round34/fact-b.md:57；uefi_helpers.rs、lib.rs:862-876、pt_alloc.rs:127-131、opensbi_helpers.rs:204-215、main.rs:110-111、lib.rs:1245-1248 | path:line 源码锚：os/kernel/src/lib.rs:862、:1245；os/arch/src/arch/pt_alloc.rs:127；os/boot-shim/src/opensbi_helpers.rs:204；os/boot-shim/src/main.rs:110；doc-citation-check failed=0 |
| RD-R34-09 | RD34-FACT-B-03 | R34 | fact-b | P2 | 本轮发现 | ① | 已修：§4.6 改为“三个镜像的引导栈顶由链接脚本 `. = ALIGN(16)` 保证；x86_64 入口 asm 另显式 `and rsp, -16`；`jump_to_kmain` 三架构再对齐” | panel/round34/fact-b.md:77；kernel-image/src/main.rs:154-172/196-215/252-320；三份 *.ld | `grep -n "三个镜像的入口 asm 显式把栈对齐" doc` → 0；新句 `grep -n "引导栈顶由链接脚本" doc` → 1 |
| RD-R34-10 | RD34-READER-A-01 | R34 | reader-a | P2 | 本轮发现 | ④ | 已修：§3.8 改为“C 的 `kinfo.module_list[]` 本身含 VM，VM 条目在回收后被置零；`VmBootHandoff.modules` 排除 VM 是 Rust 侧选择”，并同步 `vm_handoff.rs` 的同句注释 | panel/round34/reader-a.md:9；os/kernel/src/vm_handoff.rs:31-35；doc §2.4.2 | `grep -n "按 C 的 `kinfo.module_list\[\]` 排除 VM" doc` → 0；`grep -n "excludes VM — C" os/kernel/src/vm_handoff.rs` → 0 ；冷读 CLOSED（certify-reader #1 改动段回读；#2 复核相关句） |
| RD-R34-11 | RD34-READER-B-01 | R34 | reader-b | P2 | 本轮发现 | ④ | 已修：§3.4.2 审计句点名对象与字面量——Rust 侧 `os/kernel` 与 `minix-boot` 对 `BOARD`/`board` 两种拼写检索均零命中；C 内核 `get_value` 读 `board=` 属 C 侧消费者，不在本审计范围 | panel/round34/reader-b.md:18；os/kernel/src、os/libs/minix-boot/src 检索 | `grep -n "kernel 与 minix-boot 读 `\"board\"` 键零命中" doc` → 0；新句 `grep -n "两种拼写检索均零命中" doc` → 1 ；冷读 CLOSED（certify-reader #1 改动段回读；#2 复核相关句） |
| RD-R34-12 | RD34-READER-B-02 | R34 | reader-b | P2 | 本轮发现 | ④ | 已修：§5.2.2 把 `NoKernelPageSize` 的产生点从 `boot_validate_and_prepare` 改回规划层 `plan_boot_pages`/`plan_kernel_mapping`，与 §3.6/§6.3 同落点 | panel/round34/reader-b.md:36；os/kernel/src/boot_policy.rs；doc §3.6/§6.3 | `grep -n "镜像在 `boot_validate_and_prepare` 里既进不了" doc` → 0；`grep -n "镜像在规划层 `plan_boot_pages`" doc` → 1 ；冷读 CLOSED（certify-reader #1 改动段回读；#2 复核相关句） |
| RD-R34-13 | RD34-READER-B-03 | R34 | reader-b | P2 | 本轮发现 | ④ | 已修文档：§3.4.1/§4.4 收窄为“riscv64 跨距由 L9 对账；aarch64 直核面只有 L0/L3 间接基址对账，跨距无等价断言”；缺口登记 `follow-ups.md` F-R34-01 与 OPEN-DEBT | panel/round34/reader-b.md:56；os/kernel-image/check-layout.sh:248-261 仅 riscv64；bootface_a64.rs:83 | `grep -n "L0/L9 构建期对账守住" doc` → 0；`grep -n "aarch64 跨距暂无等价断言" doc` → 1 ；冷读 CLOSED（certify-reader #1 改动段回读；#2 复核相关句） |
| RD-R34-14 | RD34-READER-C-01 | R34 | reader-c | P2 | 本轮发现 | ④ | 已修：§3.3 改为“与 UEFI 独立 shim 共享取件库；内核引导收敛层按腿分两处（直核 `arch_boot_finish`、跨镜像 `arch_boot_resume_v1`，最终同落 `jump_to_kmain`）”，与 §3.7/§4.8 一致 | panel/round34/reader-c.md:5；doc §3.7/§4.8 | `grep -n "共享同一套取件库与 `arch_boot` 收敛点" doc` → 0；`grep -n "内核引导收敛层按腿分两处" doc` → 1 ；冷读 CLOSED（certify-reader #1 改动段回读；#2 复核相关句） |
| RD-R34-15 | RD34-READER-C-02 | R34 | reader-c | P2 | 本轮发现 | ④ | 已修：§4.4 第 4 项与门表补“非 strict 在 `memmap_sources` 非空时也查来源覆盖（条件性）”，与 `kernel_info.rs:503-513` 一致 | panel/round34/reader-c.md:18；minix-boot/src/kernel_info.rs:503-513 | `grep -n "非 strict 在 `memmap_sources` 非空时也查" doc` → 2 命中（§3.4.1 原有 + §4.4 新增）；`sed -n '2714p' doc` 可见新句 ；冷读 CLOSED（certify-reader #1 改动段回读；#2 复核相关句） |
| RD-R34-16 | RD34-DESIGN-D2-02 | R34 | design-d2 | P2 | 本轮发现 | ② | ② 相邻：目标 `R34-design-batch`（下一 design batch，D02 循环前；[MODERN] U-Boot 腿两个安装型全局参数化）；修于下一 design batch；用户批准 2026-11-11（代理裁决 D-049）；理由 确认轮不实施 ⑥，设计项登记 OPEN-DEBT | OPEN-DEBT `R34-design-batch` 行；`DECISIONS.md` D-049；panel/round34/design-d2.md 对应条目 | 冷读 CLOSED（certify-fact 核对 OPEN-DEBT 登记与分类标签）；确认轮不实施 ⑥ |
| RD-R34-17 | RD34-DESIGN-D2-03 | R34 | design-d2 | P2 | 本轮发现 | ② | ② 相邻：目标 `R34-design-batch`（下一 design batch，D02 循环前；[MODERN] 直核腿改用 collect_owned）；修于下一 design batch；用户批准 2026-11-11（代理裁决 D-049）；理由 确认轮不实施 ⑥，设计项登记 OPEN-DEBT | OPEN-DEBT `R34-design-batch` 行；`DECISIONS.md` D-049；panel/round34/design-d2.md 对应条目 | 冷读 CLOSED（certify-fact 核对 OPEN-DEBT 登记与分类标签）；确认轮不实施 ⑥ |
| RD-R34-18 | RD34-DESIGN-D2-04 | R34 | design-d2 | P2 | 本轮发现 | ② | ② 相邻：目标 `R34-design-batch`（下一 design batch，D02 循环前；[DEL] 删 wire 派生标量（信封 9→10））；修于下一 design batch；用户批准 2026-11-11（代理裁决 D-049）；理由 确认轮不实施 ⑥，设计项登记 OPEN-DEBT | OPEN-DEBT `R34-design-batch` 行；`DECISIONS.md` D-049；panel/round34/design-d2.md 对应条目 | 冷读 CLOSED（certify-fact 核对 OPEN-DEBT 登记与分类标签）；确认轮不实施 ⑥ |
| RD-R34-19 | RD34-DESIGN-D2-05 | R34 | design-d2 | P2 | 本轮发现 | ② | ② 相邻：目标 `R34-design-batch`（下一 design batch，D02 循环前；[MODERN] store_kernel_info 同值早返回）；修于下一 design batch；用户批准 2026-11-11（代理裁决 D-049）；理由 确认轮不实施 ⑥，设计项登记 OPEN-DEBT | OPEN-DEBT `R34-design-batch` 行；`DECISIONS.md` D-049；panel/round34/design-d2.md 对应条目 | 冷读 CLOSED（certify-fact 核对 OPEN-DEBT 登记与分类标签）；确认轮不实施 ⑥ |
| RD-R34-20 | RD34-DESIGN-D2-01 | R34 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：`MapError::Policy` 是死变体，已登记 suggestions.md R34 表；不实施 | panel/round34/design-d2.md:32；ebs_map.rs:343-373/705 | 不适用（建议级） |
| RD-R34-21 | RD34-DESIGN-D2-06 | R34 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：`BootStorage` 六组 (len, committed) 可收成两态枚举；已登记 suggestions.md R34 表 | panel/round34/design-d2.md:97；boot_storage.rs:64-90 | 不适用（建议级） |
| RD-R34-22 | RD34-DESIGN-D2-07 | R34 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：三个 `boot_alloc` 访问器可合一、删哨兵 0；已登记 suggestions.md R34 表 | panel/round34/design-d2.md:110；boot_alloc.rs:184-202 | 不适用（建议级） |
| RD-R34-23 | RD34-DESIGN-D2-08 | R34 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：x86_64 `kernel_identity_mmio_regions()` 无生产读者、注释前提陈旧；已登记 suggestions.md R34 表 | panel/round34/design-d2.md:123；arch/src/lib.rs:250-264；kernel/src/lib.rs:1003-1017 | 不适用（建议级） |
| RD-R34-24 | RD34-DESIGN-D2-09 | R34 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：`NAME_POOL` 可由 `MODULE_NAMES` 在 const fn 派生；已登记 suggestions.md R34 表 | panel/round34/design-d2.md:136；bootface_common.rs:27-68 | 不适用（建议级） |
| RD-R34-25 | RD34-DESIGN-D2-10 | R34 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：x86_64 `arch_boot` 的 `(temporary)` 取证块应删或门控；已登记 suggestions.md R34 表 | panel/round34/design-d2.md:149；kernel/src/lib.rs:241-278 | 不适用（建议级） |
| RD-R34-26 | RD34-DESIGN-D2-11 | R34 | design-d2 | P3 | 本轮发现 | SUGG | SUGG（设计 P3，非阻断）：文档把 `#[cfg(test)]` 的 `set_startup_len` 列为生产 setter；已登记 suggestions.md R34 表 | panel/round34/design-d2.md:162；boot_storage.rs:292-302 | 不适用（建议级） |
| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R34-27 | RD34-CERT-FACT-01 | R34 | fact-cert | P0 | 本轮发现 | ① | 已修（certify #1 新准入）：§3.4.2 审计句改为点名 `boot_params.rs:137`（生成内部 `BOARD=`）与 `:174`（拒绝外部同名键）的实际命中，并保留“C 内核 `get_value` 读 `board=` 属 C 侧消费者”；删除不实的“两种拼写检索均零命中” | panel/round34/certify-fact.md；os/libs/minix-boot/src/boot_params.rs:130-176 | `grep -n "两种拼写检索均零命中" doc` → 0；`grep -n "boot_params.rs:137" doc` → 1；certify #2 重读该段判 CLOSED |
| RD-R34-28 | RD34-CERT-READER-01 | R34 | reader-cert | P3 | 本轮发现 | SUGG | SUGG（读者 P3，非阻断；已顺手修）：§2.3 同 bullet 内 `xboard=1` 笔误统一为 `xboard=2` | panel/round34/certify-reader.md:（certify #1） | `grep -n "不是 `xboard=1` 的后果" doc` → 0；`grep -n "不是 `xboard=2` 的后果" doc` → 1；certify #2 重读该段判 CLOSED |

## R35 确认轮（V3.4；D-060 授权的重开确认）

> 面板：`panel/round35/`（六角色：fact-a 4、fact-b 9、design-d2 13、reader-a 4、reader-b 2、reader-c 3 条 FINDING；SUGGESTION 37；refute 6 条 CLAIM；INDEX.md 由 `panel-index.py` 生成）。
> 确认为首轮 CANDIDATE 确认：全量 survey、无 P0、2 条 P1。修复权限按 §3.6：P0/P1/P2 事实与同步项本轮修，设计项不实施 ⑥（6 条设计 P2 与 7 条设计 P3 及 1 条 ARCH 提案登记 `R35-design-batch`，D-060）。
> **身份键**：面板用裸编号，与 R26–R31 的历史行重名；本节每行同时列 `RD35-<ID>` 与裸 ID，`RD35-` 形是本节的作用域键（§4.0c）。
> 独立回执：`certify/round35-fact.md`（21/21 CLOSED）、`certify/round35-reader.md`（12/12 CLOSED）。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R35-01 | RD35-FACT-A-01、FACT-A-01 | R35 | fact-a | P2 | finding | ① | 改写该句避开「第 + 阿拉伯数字」形态，例如「`authority` 是最后加入的字段（加入后共 14 个）」；改完重跑 `tools/doc-ref-check.sh --check`，要求 EXIT=0。 | panel/round35/ 的 FACT-A-01（FINDING） | FACT-A-01 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` §3.4.1（第 1614 行）："`authority` 是第 |
| RD-R35-02 | RD35-FACT-A-02、FACT-A-02 | R35 | fact-a | P2 | finding | ① | 把该块首行注释改成工具可解析的单行形态（`// 代码引用（略去字段文档注释），os/libs/minix-boot/src/handoff_v1.rs:struct BootHandoffV1。`），把〔编注〕与改述说明移到块后的正文或不换行的同一行；改完重跑 `doc-citation-check.py`，要求该块出现在 PASS 列表（`checked` 由 28 升到 29 以上）。 | panel/round35/ 的 FACT-A-02（FINDING） | FACT-A-02 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 §3.4.3 的 `struct BootHandoffV1` 引文块（第 1716–1755 行，注释写 `// 代码引用（略去字段文档注释；...源文件为英文原文`） - 严重 |
| RD-R35-03 | RD35-FACT-A-03、FACT-A-03 | R35 | fact-a | P3 | finding | ① | 在该块后已有一句「U-Boot 侧的生产者在 `build_shim_table` 里…」的正文（§3.2 亦有同述），在那句后补一个括注：源码同处英文注释仍写着旧行为（"stores the slot capacity"），以代码为准。或另开一笔修 `opensbi_helpers.rs:206-210` 的注释（属源码工作流，不在本文档修复范围）。 | panel/round35/ 的 FACT-A-03（FINDING） | FACT-A-03 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 §3.8 的 `impl FileLoader for UbootFileLoader` 块内中文译述（第 2591–2623 行），对照 `os/boot-shim/src/op |
| RD-R35-04 | RD35-FACT-A-04、FACT-A-04 | R35 | fact-a | P3 | finding | ① | 三处改写为完整形态——`[`:const NR_TASKS`](<os/kernel/src/proc_table.rs:const NR_TASKS>)`、`[`:fn alloc_pt_page`](<os/arch/src/arch/pt_alloc.rs:fn alloc_pt_page>)`、`[`:fn classify`](<os/kernel/src/vm_handoff.rs:f | panel/round35/ 的 FACT-A-04（FINDING） | FACT-A-04 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 §3.4.2 第 1673 行 `:const NR_TASKS`；§3.8 第 2097 行 `:fn alloc_pt_page`；§3.8 第 2178 行 `:fn cla |
| RD-R35-05 | RD35-FACT-B-01、FACT-B-01 | R35 | fact-b | P2 | finding | ① | 把该句改成“256 页覆盖恒等窗口、内核高映射、模块放置与两个 DM 窗口；arena 链不在直核面预算内”，并把 `bootface.rs:126-134` 的注释作为锚点；若确实想只讨论 `PT_BUMP_PAGES` 常量本身，则删去“不覆盖 Direct Map”，不要把它写成路径事实。 | panel/round35/ 的 FACT-B-01（FINDING） | FACT-B-01 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:2296`（§4.1 `BOOT_PT_BUMP_PAGES` 条 |
| RD-R35-06 | RD35-FACT-B-02、FACT-B-02 | R35 | fact-b | P3 | finding | ① | 把“只要余数非零就会少一页”改为“`A=4095` 是对任何非零余数都成立的最小加数；更小的 `A` 会在 `r < 4096-A` 的输入上少一页（例如 `A=4094` 仅在 `r=1` 时失败）”。 | panel/round35/ 的 FACT-B-02（FINDING） | FACT-B-02 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 `01-boot-shim-bootstrap.md:2584`（§4.3 `num_pages = (data.len() + 4095) / 4096` 条目）。 - 严重度： |
| RD-R35-07 | RD35-FACT-B-03、FACT-B-03 | R35 | fact-b | P3 | finding | ① | 改为“三个架构的 builder 都把该字段填成 `kern_virt_base`；只有 x86-64 消费它（LSTAR），aarch64/riscv64 的门地址来自各架构 asm 标签，字段仅作参考”。 | panel/round35/ 的 FACT-B-03（FINDING） | FACT-B-03 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 `01-boot-shim-bootstrap.md:2678`（§4.4 `KernelInfo` 字段表 `syscall_entry` 行）。 - 严重度：P3 - 问题：文 |
| RD-R35-08 | RD35-FACT-B-04、FACT-B-04 | R35 | fact-b | P3 | finding | ① | 改成“3 行标题注释（22–24）”或“原文见 earm/head.S:21-25”。 | panel/round35/ 的 FACT-B-04（FINDING） | FACT-B-04 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 `01-boot-shim-bootstrap.md:1247`（§2.7 `earm/head.S` 引文中的注释省略标记）。 - 严重度：P3 - 问题：引文写“……（4 行标 |
| RD-R35-09 | RD35-FACT-B-05、FACT-B-05 | R35 | fact-b | P3 | finding | ① | 把该句改成“`in_snapshot_free` 读的是 `build_memmap()` 在探针分配前拍的 conventional-only 投影；0 只说明该投影未包含该区间，不能反推原始 UEFI 快照或 EBS 配对的归类”。若想讨论生产 EBS 快照，请另引 `pair_and_classify` 的输出，不要用 `test-kernel-map` 的 `build_memmap` 字 | panel/round35/ 的 FACT-B-05（FINDING） | FACT-B-05 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 `01-boot-shim-bootstrap.md:3285`（§5.2.3 深拷探针读数段）。 - 严重度：P3 - 问题：文档用“探针页是 `LOADER_DATA`，最终 |
| RD-R35-10 | RD35-FACT-B-06、FACT-B-06 | R35 | fact-b | P3 | finding | ① | 改为“`resume_additive` 是三条 v1 接收腿共用的泛型体；riscv 直核会话不经过它，改 break `minix_kernel::arch_boot`”。 | panel/round35/ 的 FACT-B-06（FINDING） | FACT-B-06 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 `01-boot-shim-bootstrap.md:3497`（§6.4 GDB 命令注释）。 - 严重度：P3 - 问题：注释写“`resume_additive` 只在 U- |
| RD-R35-11 | RD35-FACT-B-07、FACT-B-07 | R35 | fact-b | P3 | finding | ① | 在行内引用补上 `= NULL`，或把该处标成“语义摘述”而不是逐字引用。 | panel/round35/ 的 FACT-B-07（FINDING） | FACT-B-07 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 `01-boot-shim-bootstrap.md:1090`（§2.6.2 对 `pg_map` 静态状态的引用）。 - 严重度：P3 - 问题：文中引用 `pg_utils. |
| RD-R35-12 | RD35-FACT-B-08、FACT-B-08 | R35 | fact-b | P3 | finding | ① | 在引文块中保留该 `assert_eq!`，或在“未展开段”清单中明确写“入口点/VMA 校验省略”。 | panel/round35/ 的 FACT-B-08（FINDING） | FACT-B-08 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 `01-boot-shim-bootstrap.md:2307-2318`（§4.2 `prepare_and_enter_kernel_image` 引文块）。 - 严重度：P3 |
| RD-R35-13 | RD35-FACT-B-09、FACT-B-09 | R35 | fact-b | P3 | finding | ① | 若要保留样本，给出 QEMU 日志/脚本路径或复跑命令；否则删掉“本仓核对过的 E820 样本最低条目正是 `0x1000`”，只保留“`arch_init` 切掉 `[0,0x1000)` 后最低基址至少 `0x1000`”的推导。 | panel/round35/ 的 FACT-B-09（FINDING） | FACT-B-09 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 `01-boot-shim-bootstrap.md:725`（§2.4.1.3 前提二）。 - 严重度：P3 - 问题：文中写“本仓核对过的 E820 样本最低条目正是 `0x1 |
| RD-R35-14 | RD35-DESIGN-D2-01、DESIGN-D2-01 | R35 | design-d2 | P2 | finding | ② | ② 相邻：目标 `R35-design-batch`（下一 design batch，D02 循环前）；用户批准 2026-11-19（D-060）；删除 `startup_committed`、`set_startup_len`、`startup_slots_mut`，把 startup 分区定为"只追加、无提交"的唯一协议，文档对应句改为"startup 只有 append 口"。若坚持保留封口语义，则两个直核面与 U-Boot 腿必须在 push 完成后各调用一次提交入口，并删除零调用者的槽访问器。 | panel/round35/ 的 DESIGN-D2-01（FINDING） | DESIGN-D2-01 的延期已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行 |
| RD-R35-15 | RD35-DESIGN-D2-02、DESIGN-D2-02 | R35 | design-d2 | P2 | finding | ② | ② 相邻：目标 `R35-design-batch`（下一 design batch，D02 循环前）；用户批准 2026-11-19（D-060）；给每族一个 `dirty` 布尔——访问器置位，`commit_map_tables`/`set_*_len` 清除（push 路径不需要置位），`report()` 统一断言全为假。约 20 行，公开签名不变。 | panel/round35/ 的 DESIGN-D2-02（FINDING） | DESIGN-D2-02 的延期已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行 |
| RD-R35-16 | RD35-DESIGN-D2-03、DESIGN-D2-03 | R35 | design-d2 | P2 | finding | ② | ② 相邻：目标 `R35-design-batch`（下一 design batch，D02 循环前）；用户批准 2026-11-19（D-060）；删除字段与信封中的 8 字节（wire 版本从 9 升到 10），把入口检查改为对 `minix_arch::syscall_entry_va()` 求值，或保留检查但不再携带数值；若必须保留字段，则写入真实入口并由接收侧按架构覆盖，同时改名（例如 `syscall_entry_ref`）并把文档改成实际语义。 | panel/round35/ 的 DESIGN-D2-03（FINDING） | DESIGN-D2-03 的延期已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行 |
| RD-R35-17 | RD35-DESIGN-D2-04、DESIGN-D2-04 | R35 | design-d2 | P2 | finding | ② | ② 相邻：目标 `R35-design-batch`（下一 design batch，D02 循环前）；用户批准 2026-11-19（D-060）；引入四字段命名结构 `BootScalars { kern_virt_base, kern_phys_base, kern_size, kern_stack_top }`，两个构造源（`KernelInfo`、`BootReport`）各给一个转换；函数只收这个结构。 | panel/round35/ 的 DESIGN-D2-04（FINDING） | DESIGN-D2-04 的延期已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行 |
| RD-R35-18 | RD35-DESIGN-D2-05、DESIGN-D2-05 | R35 | design-d2 | P2 | finding | ② | ② 相邻：目标 `R35-design-batch`（下一 design batch，D02 循环前）；用户批准 2026-11-19（D-060）；把修剪上移到共享消费者（`load_boot_modules_into` 或 `load_kernel_with_loader`），`read` 只返回原始字节；若保留现状，trait 文档必须写明"返回的是装载视图，不是文件原文"，并让 UEFI 侧执行同一修剪以消除腿间差异。 | panel/round35/ 的 DESIGN-D2-05（FINDING） | DESIGN-D2-05 的延期已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行 |
| RD-R35-19 | RD35-DESIGN-D2-06、DESIGN-D2-06 | R35 | design-d2 | P2 | finding | ② | ② 相邻：目标 `R35-design-batch`（下一 design batch，D02 循环前）；用户批准 2026-11-19（D-060）；删除空注释段与恒真测试；若确实需要 getter，生成它们并在测试里比较 getter 与字段（两侧都来自同一个 `make_valid_info()` 不够，至少要有一个字段被改写后验证 getter 读到新值）。 | panel/round35/ 的 DESIGN-D2-06（FINDING） | DESIGN-D2-06 的延期已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行 |
| RD-R35-20 | RD35-DESIGN-D2-07、DESIGN-D2-07 | R35 | design-d2 | P3 | finding | ② | ② 相邻：目标 `R35-design-batch`（下一 design batch，D02 循环前）；用户批准 2026-11-19（D-060）；返回一个私有 `PreparedJump { entry: VirBytes, arena_pa: PhysBytes }`，`jump_kernel_image_v1` 收该结构；三个架构的跳转臂各自取字段。 | panel/round35/ 的 DESIGN-D2-07（FINDING） | DESIGN-D2-07 的延期已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行 |
| RD-R35-21 | RD35-DESIGN-D2-08、DESIGN-D2-08 | R35 | design-d2 | P3 | finding | ② | ② 相邻：目标 `R35-design-batch`（下一 design batch，D02 循环前）；用户批准 2026-11-19（D-060）；把两个切片从 `BootReport` 移到独立的 `BootRecords<'a>`（或 `BootPrepareResult` 的一个新字段），`prepare_boot` 同时返回两者；`BootReport` 只保留内核入口真正读取的五类载荷与六个标量。这样"记录在 wire"这条腿约定不再污染内核入口的类型。 | panel/round35/ 的 DESIGN-D2-08（FINDING） | DESIGN-D2-08 的延期已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行 |
| RD-R35-22 | RD35-DESIGN-D2-09、DESIGN-D2-09 | R35 | design-d2 | P3 | finding | ② | ② 相邻：目标 `R35-design-batch`（下一 design batch，D02 循环前）；用户批准 2026-11-19（D-060）；二选一：(a) 删除该分区与字段，strict 门改为检查"内核物理跨度落在某条 `memmap` 区间或某条 `reserved_regions` 区间内"（五条腿上都成立：直核腿内核在 free 表内，UEFI 腿在保留表内）；(b) 保留字段但压缩为一条"内核已覆盖"的记录，wire 分区缩到 1 槽，`BootStorage` 的 20 槽随之删除。 | panel/round35/ 的 DESIGN-D2-09（FINDING） | DESIGN-D2-09 的延期已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行 |
| RD-R35-23 | RD35-DESIGN-D2-10、DESIGN-D2-10 | R35 | design-d2 | P3 | finding | ② | ② 相邻：目标 `R35-design-batch`（下一 design batch，D02 循环前）；用户批准 2026-11-19（D-060）；为 `KernelInfo` 引入两变体 `RamAuthority { DtbBootInput, UefiMemoryMap }`，wire 解码时从 0/1 映射；记录层继续用 `InputObjectId`。构造点与 strict 门各改一行。 | panel/round35/ 的 DESIGN-D2-10（FINDING） | DESIGN-D2-10 的延期已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行 |
| RD-R35-24 | RD35-DESIGN-D2-11、DESIGN-D2-11 | R35 | design-d2 | P3 | finding | ② | ② 相邻：目标 `R35-design-batch`（下一 design batch，D02 循环前）；用户批准 2026-11-19（D-060）；让 `free_pt_page` 在未注册时断言（与分配侧对称），把 `is_free_registered()` 分支从三个 `destroy` 实现里删掉；或让它返回 `bool` 由调用方决定。前者的代价是确认启动期没有任何路径会对引导页表调用 `destroy`（当前 x86_64 的单元测试会调用，需要为测试注册归还钩子）。 | panel/round35/ 的 DESIGN-D2-11（FINDING） | DESIGN-D2-11 的延期已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行 |
| RD-R35-25 | RD35-DESIGN-D2-12、DESIGN-D2-12 | R35 | design-d2 | P3 | finding | ② | ② 相邻：目标 `R35-design-batch`（下一 design batch，D02 循环前）；用户批准 2026-11-19（D-060）；改写 `write_envelope` 的注释为"测试载体的构造便捷方法"并加 `#[cfg(any(test, feature = "test-support"))]`（如果测试跨 crate 使用），或删除它并让测试改用 `write_envelope_scalars`；`source_audit()` 同理标注为诊断 API 或删除。 | panel/round35/ 的 DESIGN-D2-12（FINDING） | DESIGN-D2-12 的延期已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行 |
| RD-R35-26 | RD35-DESIGN-D2-13、DESIGN-D2-13 | R35 | design-d2 | P3 | finding | ② | ② 相邻：目标 `R35-design-batch`（下一 design batch，D02 循环前）；用户批准 2026-11-19（D-060）；在 `minix-boot` 提供一个 `DerivedScalars::from_layout(virt_base, size)`，两个生产者调用它；strict 门保留独立推演（防御性冗余是它的职责），序列化器的 debug 断言可以删除或改为调用同一函数。 | panel/round35/ 的 DESIGN-D2-13（FINDING） | DESIGN-D2-13 的延期已落 D-060 与 OPEN-DEBT 的 R35-design-batch 行 |
| RD-R35-27 | RD35-READER-A-01、READER-A-01 | R35 | reader-a | P2 | finding | ④ | 在 §3.4.1 或 §3.4.2 开头补一句：“21 是 C→Rust 的累计台账；‘字段删除’裁决只删其中的 `bootstrap_start/len` 与 `free_upper_idx` 三个，其余 18 个在此前各次设计变更中已退场。”同时把 §1.4 的图例条目改名为“字段删除（裁决）”以示区分。 | panel/round35/ 的 READER-A-01（FINDING） | READER-A-01 的修复由 certify/round35-reader.md 对应条目回读确认；改动点见文档 §1.4 编号图例；§3.4.1 字段表“删除”行；§3.4.2 标题。原文分别是“**"字段删除"** 是本文对一次设计裁决的简称：删除 `KernelInfo` 的 `boot |
| RD-R35-28 | RD35-READER-A-02、READER-A-02 | R31；last_seen: R35 | reader-a | P2 | finding | ④ | 把“四条架构各自的中层表”改成“三条 v1 发送腿（x86_64、aarch64、riscv64 U-Boot shim）各自的中层表”或“三个架构各自的中层表”，并在句末回指 §3.7 的术语对账表；若确有第四者，点名它并补进 §3.7 的表。 | panel/round35/ 的 READER-A-02（FINDING） | READER-A-02 的修复由 certify/round35-reader.md 对应条目回读确认；改动点见文档 §4.1 逐行解码第一条。原文：“跨镜像腿要额外建 Direct Map 覆盖、ELF 段的逐页拆解与 arena 之后的页表链，四条架构各自的中层表加起来留足余量；两个直核面只建 |
| RD-R35-29 | RD35-READER-A-03、READER-A-03 | R35 | reader-a | P2 | finding | ④ | 把缺口写成一句自包含的陈述（“aarch64 直核面的跨距目前只有 L0/L3 的间接基址对账，没有与 L9 等价的断言”），删掉 `OPEN-DEBT.md`/`suggestions.md` 的引用；若必须保留出处，写可解析的完整路径，并注明它是过程档案、不是正文证据。 | panel/round35/ 的 READER-A-03（FINDING） | READER-A-03 的修复由 certify/round35-reader.md 对应条目回读确认；改动点见文档 §3.4.1 的“aarch64 直核面……缺口登记见 `OPEN-DEBT.md` 的 F-R34-01”；§4.4 字段表的“aarch64 跨距暂无等价断言（`OPEN-DE |
| RD-R35-30 | RD35-READER-A-04、READER-A-04 | R35 | reader-a | P3 | finding | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：在 §3.4.2 补出算式（例如“`user_sp` 8 + `nr_procs` 4 + `nr_tasks` 4 + `release` 8 + `version` 8 + `freepde_start` 4 + `vir_kern_start` 8 + `_padding` 4 = 48”），并把 §3.9 改为“该字段仍在 48 字节的 `KinfoStruct` 布局内”。 | panel/round35/ 的 READER-A-04（FINDING） | READER-A-04 已登记 suggestions.md 的 R35 段；镜像条目见 §3.4.2 的“Rust 只拷 48 字节的 `KinfoStruct`……只含七个语义字段（外加 4 字节 `_padding` 把结构补齐到 48 字节）”；§3.9 的“C |
| RD-R35-31 | RD35-READER-B-01、READER-B-01 | R35 | reader-b | P2 | finding | ④ | 把「建根完成」改为「Step 1/2 已过、Step 2b/2c/3 或跳转尚未完成（建根后半的任一点）」，或直接采用 §6.3 的「卡在建根内部或建根完成后」两分法。 | panel/round35/ 的 READER-B-01（FINDING） | READER-B-01 的修复由 certify/round35-reader.md 对应条目回读确认；改动点见文档 §6.1 末段「删掉 `[raw] entering kernel image v1` 这一行会怎样」；原文：「`kernel: step1+2 mappings ok` 表示 S |
| RD-R35-32 | RD35-READER-B-02、READER-B-02 | R35 | reader-b | P2 | finding | ④ | 在 §3.4.1 的分类层段加一句映射（例如「设计四档中的 `Special` 尚未落地，现行由 `DevicePath`/`Side` 承载；现行分流规则即 §4.2.2 的 Step 3/4/5」），或把「三条分流规则」改成「五步判定与两道内联闸」并直接指向 §4.2.2；若四档只是 I-29 未批准的目标，明确写「现行实现不按这四档」。 | panel/round35/ 的 READER-B-02（FINDING） | READER-B-02 的修复由 certify/round35-reader.md 对应条目回读确认；改动点见文档 §3.4.1「OS 内存语义分类层」段与 §4.2.2 判定表；原文：「把描述符按类型与属性分成 Available（可分配）、ReservedMapped（保留但可访问）、Exc |
| RD-R35-33 | RD35-READER-C-01、READER-C-01 | R35 | reader-c | P1 | finding | ④ | 在 §3.4.1 第 3 层补一句显式映射，例如"`Special` 在现行实现里落到 `DevicePath`（MMIO/MMIO_PORT_SPACE）与 `Side`（侧桶）两档，见 §4.2.2"；并把"三条分流规则"改成可对账的说法（例如"三条政策规则 = Step 2/3 的类型强制、Step 4 的类型无关属性、Step 5 的类型表"，或直接写"五步判定见 §4.2.2"）。若四档 | panel/round35/ 的 READER-C-01（FINDING） | READER-C-01 的修复由 certify/round35-reader.md 对应条目回读确认；改动点见文档 §3.4.1「内存图生产的四层责任」第 3 层（原文："把描述符按类型与属性分成 Available（可分配）、ReservedMapped（保留但可访问）、Excluded（不可 |
| RD-R35-34 | RD35-READER-C-02、READER-C-02 | R35 | reader-c | P2 | finding | ④ | 把两份 DM 覆盖分开说清：例如"跨镜像腿额外建的是**接收侧**为 shim 窗口/arena 建立的 DM 覆盖；直核面的 256 页预算**已包含它自己的 Step 4 DM 覆盖**，只是不含 arena 链"。若直核面的 Step 4 不消耗 256 页预算（例如复用已有中层表），则应在 §4.6/§3.8 明说，并把 §4.1 的"不覆盖 Direct Map"改成"不为接收侧的额外  | panel/round35/ 的 READER-C-02（FINDING） | READER-C-02 的修复由 certify/round35-reader.md 对应条目回读确认；改动点见文档 §4.1（原文："256 页（1 MiB）按直核面只建恒等窗口、内核高映射与模块放置所需的几级中层表来估，**不覆盖 Direct Map** 与 arena 链"）与 §6.1 |
| RD-R35-35 | RD35-READER-C-03、READER-C-03 | R35 | reader-c | P1 | finding | ④ | M6 的覆盖列改成"x86 UEFI 门（`entering scheduler`）、riscv U-Boot shim 门（`entering scheduler`）、aarch64 SMP AP 门通过；aarch64 UEFI 门止于 `memmap source`，riscv 直核 bootface 门是模块定位失败路径，二者不覆盖 Step 1"。M14 的覆盖列删掉 riscv U-B | panel/round35/ 的 READER-C-03（FINDING） | READER-C-03 的修复由 certify/round35-reader.md 对应条目回读确认；改动点见文档 §5.1 M6（原文："Step 1 的循环只遍历调用方窗口表（源码序），跨镜像三腿 QEMU 门与直核 riscv `bootface`、aarch64 SMP AP 门均通过" |
| RD-R35-36 | RD35-SUG-A-01、SUG-A-01 | R35 | fact-a | P3 | suggestion | ① | SUG-A-01（证据可升级） - 位置：§1.1 第 9 行、§1.4 第 154 行的 boot2 `CR0.PG` 保留声明（"boot2 的 `CR0.PG` 实际值未逐例核实" / "boot2 交棒时的寄存器/栈契约不在本篇取证范围"） - 现状与建议：本仓证据足以把这条从"未核实"升级为**代码事实**： | panel/round35/ 的 SUG-A-01（SUGGESTION） | SUG-A-01 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 fact-a.md |
| RD-R35-37 | RD35-SUG-A-02、SUG-A-02 | R35 | fact-a | P3 | suggestion | ① | SUG-A-02（外部规范引用缺版本锚） - 位置：§1.1 第 32 行"Multiboot 规范在「机器状态」一节把除少数几个寄存器以外的处理器状态都列为未定义，并明写「操作系统必须在自己需要栈时立刻建一个」" - 现状与建议：句子给了规范小节名（"机器状态"），但没有版本号或出处定位（Multiboot 0.6. | panel/round35/ 的 SUG-A-02（SUGGESTION） | SUG-A-02 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 fact-a.md |
| RD-R35-38 | RD35-FACT-B-S01、FACT-B-S01 | R31；last_seen: R35 | fact-b | P3 | suggestion | ① | FACT-B-S01 - 位置：`01-boot-shim-bootstrap.md:2296`（§4.1 同一段）。 - 建议：句中“四条架构各自的中层表”与本篇其余处“三条架构/五条腿”的计数口径不一致；仓内 Rust 架构是 x86_64、aarch64、riscv64 三个。若原意是“四条腿/五条生产腿”，改成 | panel/round35/ 的 FACT-B-S01（SUGGESTION） | FACT-B-S01 的修复由 certify/round35-fact.md 对应条目回读确认；改动点见文档 fact-b.md |
| RD-R35-39 | RD35-A-01、A-01 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-01 - 位置：§1.1 首段。原文：“地址翻译这一步两条路不一样——UEFI 上的固件是开着分页进来的，而 BIOS 上的引导器（boot2 或 GRUB）在本仓核对到的路径上交棒（**关着分页**这一状态对 GRUB 类合规引导器由 Multiboot 规范支撑；boot2 的 `CR0.PG` 实际值未逐例核 | panel/round35/ 的 A-01（SUGGESTION） | A-01 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-40 | RD35-A-02、A-02 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-02 - 位置：§1.2 图。原文：“▲ 依赖 A（框 2 → 框 1）：内核自己占的那一段要被这份清单切掉”。 - 问题：图里只有“依赖 A”，正文随后定义 D1–D5，并把同一条边叫 D2。我第一遍在图里找 B/C/D，第二遍才在文字里发现 A 就是 D2。 - 影响（若采纳）：悬空的字母名字会让读者以为漏了一 | panel/round35/ 的 A-02（SUGGESTION） | A-02 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-41 | RD35-A-03、A-03 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-03 - 位置：§1.2 框表第 4 行；§2.2 第 6 条；§2.8 D4。原文：§1.2 的“`pre_init` 的返回值 `&kinfo`、`head.S` 的 `push %eax` / §2.2、§2.8”；§2.8 的“它在 §2.2 引文块之外；引文里那条 `push %eax` 是给 `pre_ | panel/round35/ 的 A-03（SUGGESTION） | A-03 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-42 | RD35-A-04、A-04 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-04 - 位置：§2.1。原文：“两个魔术值只差一个十六进制位，方向相反”。 - 问题：“方向相反”读起来像“两个值按位相反”，而实际是“两个传递方向相反，值只差最高半字节”。表格的“方向”列能救回来，但这句话本身会让我误判。 - 影响（若采纳）：改清楚后，读者不必依赖表格就能知道 0x1BADB002 与 0x2 | panel/round35/ 的 A-04（SUGGESTION） | A-04 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-43 | RD35-A-05、A-05 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-05 - 位置：§2.3 的“**删掉“已存在先删”那段循环会怎样**。分三层说：”。 - 问题：第二条“同一个 off-by-one 的第二种表现”与第三条“要排除的首条提升分支”讲的都是“循环仍在”时的行为，不是“删掉循环后”的后果。标题说“删掉会怎样”，三段里只有第一段在回答这个问题。 - 影响（若采纳）：读 | panel/round35/ 的 A-05（SUGGESTION） | A-05 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-44 | RD35-A-06、A-06 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-06 - 位置：§2.5。原文：“同一段物理内存被两个模块声明时，第二次 `cut_memmap` 落在已被排除的区间上，是空操作（按 `add_memmap` 与 `cut_memmap` 的取整规则推演：先切 `[0x120000, 0x150000)`、再切 `[0x130000, 0x140000)`，清单 | panel/round35/ 的 A-06（SUGGESTION） | A-06 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-45 | RD35-A-07、A-07 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-07 - 位置：§2.6.4 首句；§2.4.1.3 前提一；§4.4 首段。原文：“`pre_init` 与 `prot_init` 调用的**不是同一份 `pg_utils.o`**”；“`kmain` 先调 `cstart`（`main.c:147`），`arch_init` 在 `cstart` 体内”；“ | panel/round35/ 的 A-07（SUGGESTION） | A-07 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-46 | RD35-A-08、A-08 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-08 - 位置：§2.6.4。原文：“paged 副本 B 在 `prot_init` 里由同一组四步（`pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`）建好”。 - 问题：§1.4 的索引把同一组函数叫“C 建表五步”，并包含 `vm_enable_paging` | panel/round35/ 的 A-08（SUGGESTION） | A-08 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-47 | RD35-A-09、A-09 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-09 - 位置：§3.1 分工表表头；§3.7 第一段。原文：“FW_PAYLOAD 直核形态不在本表，见 §3.7”；“规范背景与真板意义见 §3.1 例外条款段”。 - 问题：`FW_PAYLOAD` 首次出现在 §3.1 的表头/表行，指针把我送到 §3.7；§3.7 又把我送回 §3.1 的例外条款段，而真 | panel/round35/ 的 A-09（SUGGESTION） | A-09 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-48 | RD35-A-10、A-10 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-10 - 位置：§4.3 首段与 tty/mib 反事实；§3.1.1 接线状态表。原文：“boot-shim 三条链的“顺序”部分没有编译期检查”；§3.1.1 的第一行“x86_64 / aarch64 UEFI 产线”与第三行“riscv64 UEFI shim / **未接线**”。 - 问题：“boot- | panel/round35/ 的 A-10（SUGGESTION） | A-10 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-49 | RD35-A-11、A-11 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-11 - 位置：§4.3 的 `MODULE_NAMES` 代码块；§4.8 的 riscv64 代码块。原文：“// minix-boot：共享的规范名字表；数组长度在编译期钉住计数。”、“// boot-shim：只重导出同一份表，不另立第二份。”、“// U-Boot 接收镜像：a0 = boot-shim  | panel/round35/ 的 A-11（SUGGESTION） | A-11 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-50 | RD35-A-12、A-12 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-12 - 位置：§5.2.1 的逐文件复算段。原文：“`os/libs/minix-boot/src/handoff_v1_write.rs` 的 10（删双写测试 −1）”。 - 问题：10 这个数用一个“删了某个测试”的历史来校准；我没有历史可查，也无法从当前文件或上表复算出“10”。这既是一个过程词进入计数表 | panel/round35/ 的 A-12（SUGGESTION） | A-12 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-51 | RD35-A-13、A-13 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-13 - 位置：§1.4 编号图例。原文：“`D-1`/`D-4`/`D01 裁决项 3` 是当轮设计/裁决编号”。 - 问题：“当轮”是过程词，读者不知道是哪一轮；这一整段又是 300 多词的 R/D/F 编号墙。虽然它说“跳过不影响结论”，但它位于“本篇范围”这一节的显眼位置，初学者会误以为需要记住这些编号。  | panel/round35/ 的 A-13（SUGGESTION） | A-13 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-52 | RD35-A-14、A-14 | R35 | reader-a | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：A-14 - 位置：§1.4（切根）、§2.6.3/§2.6.4（切根/换根）、§4.4/§2.4（收口）、§4.1/§3.7（直核）。原文：“**切根**（从自举根切到运行期根……）”、“切根后那张 `pagedir[]`（paged 副本 B，§2.6.4）”、“这次**换根**”、“2 MiB 对齐收口”、“以  | panel/round35/ 的 A-14（SUGGESTION） | A-14 已登记 suggestions.md 的 R35 段；镜像条目见 reader-a.md |
| RD-R35-53 | RD35-S-01、S-01 | R35 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：S-01 - 位置与原文：§1.4 标题「### 1.4 本篇范围与可以安全跳过的内容」，同节又出现「全篇有五个与'栈顶'有关的名字，先并排列出」「本篇用到的编号不止一套，先给一张**主要编号索引**」「四处需要单独说明的边界」；§2.4「**2.4 结算**：走到这里，机器上第一次有了**一份有据可查的可用物理内存清 | panel/round35/ 的 S-01（SUGGESTION） | S-01 已登记 suggestions.md 的 R35 段；镜像条目见 reader-b.md |
| RD-R35-54 | RD35-S-02、S-02 | R35 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：S-02 - 位置与原文：§1.2 第 42 行「**主链的框序是框 2 → 框 1 → 框 3 → 框 4**」与第 71 行「**D1–D5 只是约束名，不是节点名**：D1 = 清单先于建表、D2 = 内核位置先于清单裁剪」；§2.8 第 1376 行「下面**按编号**逐条给陈述与证据锚点，不按执行序重排——编 | panel/round35/ 的 S-02（SUGGESTION） | S-02 已登记 suggestions.md 的 R35 段；镜像条目见 reader-b.md |
| RD-R35-55 | RD35-S-03、S-03 | R35 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：S-03 - 位置与原文：§3.4.1「删除」行「逐个的去向见下表」；§4.8 表注「上一条术语对账里说的'四种协议段/交接面'」；§5.1 M14「aarch64 additive 的缺口记在 §5.4」。 - 问题：同一根因是相对指针或跨节指针没有落到声称的内容。§3.4.1 的「下表」实际落在「先讲清前两格装的是 | panel/round35/ 的 S-03（SUGGESTION） | S-03 已登记 suggestions.md 的 R35 段；镜像条目见 reader-b.md |
| RD-R35-56 | RD35-S-04、S-04 | R35 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：S-04 - 位置与原文：§3.1.1「aarch64 直核面 / 测试通道，无专用启动门脚本；由 `test-smp-aps-aarch64.sh` 用 `-kernel` 间接驱动」；§5.2.3 同一段两次出现「aarch64 直核面由 `test-smp-aps-aarch64.sh` 用 `-kernel`  | panel/round35/ 的 S-04（SUGGESTION） | S-04 已登记 suggestions.md 的 R35 段；镜像条目见 reader-b.md |
| RD-R35-57 | RD35-S-05、S-05 | R35 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：S-05 - 位置与原文：§4.6「因此不再依赖固件 MTRR 的正确性（TODO-10 已关闭）」；§6.1「（TODO-09 已关闭）」；§4.1「当前已没有'从 memmap 切 256 页'的兜底，早期版本有、现已删除」；§3.5「当前 `kmain` 的 Phase F 已不再要求 `bootstrap_le | panel/round35/ 的 S-05（SUGGESTION） | S-05 已登记 suggestions.md 的 R35 段；镜像条目见 reader-b.md |
| RD-R35-58 | RD35-S-06、S-06 | R35 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：S-06 - 位置与原文：§2.4.1.4「v9（`VmBootHandoff` 的 `VM_BOOT_HANDOFF_VERSION = 9`；它与 §3.4.3 的 arena 信封版本 `BOOT_HANDOFF_V1_VERSION` 是两个独立计数器）」；§3.4.3「本文出现的'版本 8→9'都是后者」与  | panel/round35/ 的 S-06（SUGGESTION） | S-06 已登记 suggestions.md 的 R35 段；镜像条目见 reader-b.md |
| RD-R35-59 | RD35-S-07、S-07 | R35 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：S-07 - 位置与原文：§5.2.1「下面两段是实际的命令与输出，预期输出用 `test result:` 行核对」，块内出现裸 `...` 行（如 `$ cd os && cargo test -p boot-shim --features test-all --lib` 之后的 `...`）。 - 问题：同一根因 | panel/round35/ 的 S-07（SUGGESTION） | S-07 已登记 suggestions.md 的 R35 段；镜像条目见 reader-b.md |
| RD-R35-60 | RD35-S-08、S-08 | R35 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：S-08 - 位置与原文：§5.1 M16「5 条宿主测试覆盖相离/相切、包含、部分重叠、内核在区间内、模块字节越界」；§5.1 M2「旧 `memmap.rs` 17 条随模块删除，其断言被行为锁与对拍覆盖」。 - 问题：同一根因是覆盖声明没有测试级证据。M16 声称 5 条宿主测试，但没有给测试名或文件，§5.2. | panel/round35/ 的 S-08（SUGGESTION） | S-08 已登记 suggestions.md 的 R35 段；镜像条目见 reader-b.md |
| RD-R35-61 | RD35-S-09、S-09 | R35 | reader-b | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：S-09 - 位置与原文：§3.10「三张表（本节、§4.9、§7.5）冲突时：C 侧符号落点以 §7.5 为准」；§4.9「冲突时按同一规则：C 侧看 §7.5，Rust 侧看定义节」；§7.5「这是**速查表**，不保证穷举；冲突时以源码为准」。 - 问题：同一根因是三张 C→Rust 表重叠且冲突规则是嵌套的。§ | panel/round35/ 的 S-09（SUGGESTION） | S-09 已登记 suggestions.md 的 R35 段；镜像条目见 reader-b.md |
| RD-R35-62 | RD35-READER-C-S01、READER-C-S01 | R35 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：READER-C-S01 - 位置：§2.2（原文："**第一跳（`multiboot_init`）的第一条指令必须是设栈**——这一条只能是汇编，理由在 §1.1"） - 严重度：P2 - 问题："必须"是无条件写法，但 §1.4 的边界声明说"「必须以设栈开始」的**必要性**只对 Multiboot 合规引导器成 | panel/round35/ 的 READER-C-S01（SUGGESTION） | READER-C-S01 已登记 suggestions.md 的 R35 段；镜像条目见 reader-c.md |
| RD-R35-63 | RD35-READER-C-S02、READER-C-S02 | R35 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：READER-C-S02 - 位置：§3.3（原文："riscv64 的 QEMU 门链当前走直核自引导这一辅助形态，独立 shim 主线走 U-Boot 路径——主产线归属以该表为准"） - 严重度：P2 - 问题：§3.1.1 的接线状态表说"riscv64 U-Boot shim / 已接线……门脚本 `test | panel/round35/ 的 READER-C-S02（SUGGESTION） | READER-C-S02 已登记 suggestions.md 的 R35 段；镜像条目见 reader-c.md |
| RD-R35-64 | RD35-READER-C-S03、READER-C-S03 | R35 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：READER-C-S03 - 位置：§5.2.3（原文："**五条生产腿减去五条门链，缺的正是 aarch64 直核面的专用启动门**"） - 严重度：P2 - 问题：§3.7 已说两个"5"成员不同：门链五条**含** riscv64 U-Boot 直载测试链、**不含** aarch64 直核面；生产腿五条**含* | panel/round35/ 的 READER-C-S03（SUGGESTION） | READER-C-S03 已登记 suggestions.md 的 R35 段；镜像条目见 reader-c.md |
| RD-R35-65 | RD35-READER-C-S04、READER-C-S04 | R35 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：READER-C-S04 - 位置：§3.4.1（原文："五条生产腿产出这两张表的方式不同（内存图产线）"；"上面**三条**内存图产线各自产出同一形状的两张表"）与 §3.7（原文："'产线'是链的别称，不单独计数"） - 严重度：P2 - 问题：若按 §3.7 的定义"产线 = 链"，则"三条内存图产线"就是在说" | panel/round35/ 的 READER-C-S04（SUGGESTION） | READER-C-S04 已登记 suggestions.md 的 R35 段；镜像条目见 reader-c.md |
| RD-R35-66 | RD35-READER-C-S05、READER-C-S05 | R35 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：READER-C-S05 - 位置：§3.4.1（原文："U-Boot 腿的 root/bump/arena 都在 `prepare_boot` 内分配并立即写入 storage 的 startup 槽……两个直核面在 `bootface`/`bootface_a64` 写 root 与 bump（无 arena）"） | panel/round35/ 的 READER-C-S05（SUGGESTION） | READER-C-S05 已登记 suggestions.md 的 R35 段；镜像条目见 reader-c.md |
| RD-R35-67 | RD35-READER-C-S06、READER-C-S06 | R35 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：READER-C-S06 - 位置：§4.6（原文："RISC-V Sv39 上，S 模式取指一条 U=1 的页是被禁止的——`sstatus.SUM` 只放宽 supervisor 对用户页的 load/store，取指不受它影响——所以恒等映射必须是 supervisor-only"） - 严重度：P2 - 问题： | panel/round35/ 的 READER-C-S06（SUGGESTION） | READER-C-S06 已登记 suggestions.md 的 R35 段；镜像条目见 reader-c.md |
| RD-R35-68 | RD35-READER-C-S07、READER-C-S07 | R35 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：READER-C-S07 - 位置：§4.1（原文："x86_64 的 `rust_image_main` 从 `rbx` 取 arena 基址"）与 §4.8（原文："`_start` trampoline 按源码次序执行 `cli`、`cld`、清 `.bss`……把 `rbx` 转进 SysV 第一参数 `rdi | panel/round35/ 的 READER-C-S07（SUGGESTION） | READER-C-S07 已登记 suggestions.md 的 R35 段；镜像条目见 reader-c.md |
| RD-R35-69 | RD35-READER-C-S08、READER-C-S08 | R35 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：READER-C-S08 - 位置：§3.4.1（原文："详见 `01-design-note-memmap-consolidation.md` v12"；"缺口登记见 `OPEN-DEBT.md` 的 F-R34-01"） - 严重度：P3 - 问题：两处跨文档指针只给文件名，没有目录；本篇其他跨篇指针都给路径（例如 | panel/round35/ 的 READER-C-S08（SUGGESTION） | READER-C-S08 已登记 suggestions.md 的 R35 段；镜像条目见 reader-c.md |
| RD-R35-70 | RD35-READER-C-S09、READER-C-S09 | R35 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：READER-C-S09 - 位置：§5.1（原文："TODO 列的状态口径**以本表为唯一权威**……5.3 只放详情与交接记录，不另立状态口径"）与 §5.3（原文："✅ 已关闭（宿主合成 layout 门）"、"✅ 已关闭（覆盖边界已写明）"等） - 严重度：P2 - 问题：§5.3 的"✅ 已关闭"是状态标记， | panel/round35/ 的 READER-C-S09（SUGGESTION） | READER-C-S09 已登记 suggestions.md 的 R35 段；镜像条目见 reader-c.md |
| RD-R35-71 | RD35-READER-C-S10、READER-C-S10 | R35 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：READER-C-S10 - 位置：§1.4 标题"本篇范围与可以安全跳过的内容"与其内容（五个栈顶名的表、主要编号索引、计数约定、四条边界声明、命名陷阱） - 严重度：P3 - 问题：标题只覆盖"范围/跳过"，但这一节同时承载术语表、编号索引、计数约定、边界声明和命名陷阱。读者按标题找"范围"时要穿过几类不同的参考材 | panel/round35/ 的 READER-C-S10（SUGGESTION） | READER-C-S10 已登记 suggestions.md 的 R35 段；镜像条目见 reader-c.md |
| RD-R35-72 | RD35-READER-C-S11、READER-C-S11 | R35 | reader-c | P3 | suggestion | SUGG | 读者/设计 P3（§4.0c 门槛，不阻断）：READER-C-S11 - 位置：§4.6（原文："（TODO-10 已关闭）"）、§6.1（原文："（TODO-09 已关闭）"）、§5.1 M4（原文："TODO-01（已关闭，见 5.3）"） - 严重度：P3 - 问题：正文里带着"已关闭"状态与 R17/R18/R19/R20/R21、D-031/D-032 | panel/round35/ 的 READER-C-S11（SUGGESTION） | READER-C-S11 已登记 suggestions.md 的 R35 段；镜像条目见 reader-c.md |
| RD-R35-73 | RD35-CLAIM-01、CLAIM-01 | R35 | refute | P3 | claim | FP | 反证失败：01 - 主张：x86-32 的第一跳必须先从汇编设栈；`multiboot_init` 的第一条指令是 `mov $load_stack_start, %esp`，`load_stack` 是同一文件 `.data` 段里的 4096 字节静态空间，链接链保证 `load_stack_start` | panel/round35/refute.md 的 CLAIM 01（源码原文核对：`minix3/minix/kernel/arch/i386/head.S:44,93-96` 与 `pg_utils.c:65-84` 逐条命中） | CLAIM-01 反证失败（主张成立），源码锚见 panel/round35/refute.md 的 CLAIM-01 |
| RD-R35-74 | RD35-CLAIM-02、CLAIM-02 | R35 | refute | P3 | claim | FP | 反证失败：02 - 主张：C 的 `alloc_lowest(cbi, len)` 在唯一调用点把**长度**当作 `cut_memmap` 的**结束地址**传入；在文档列出的四个前提下（`arch_init` 先切掉低页、最低可用基址 ≥ 0x1000、`roundup(tramp_size) = 0x1 | panel/round35/refute.md 的 CLAIM 02（反证失败的源码原文核对：见该 CLAIM 的「证据」行，均为 path:line） | CLAIM-02 反证失败（主张成立），源码锚见 panel/round35/refute.md 的 CLAIM-02 |
| RD-R35-75 | RD35-CLAIM-03、CLAIM-03 | R35 | refute | P3 | claim | FP | 反证失败：03 - 主张：Rust 侧把 C 的全局 `kinfo` 改为按值交接的 `KernelInfo`；结构恰有 14 个字段，`bootstrap_start`/`bootstrap_len`/`free_upper_idx` 三个字段已删除；接收方在消费前把它深拷进七个 `.bss` 落地区加一个 | panel/round35/refute.md 的 CLAIM 03（反证失败的源码原文核对：见该 CLAIM 的「证据」行，均为 path:line） | CLAIM-03 反证失败（主张成立），源码锚见 panel/round35/refute.md 的 CLAIM-03 |
| RD-R35-76 | RD35-CLAIM-04、CLAIM-04 | R35 | refute | P3 | claim | FP | 反证失败：04 - 主张：引导期页表页分配器由 `pt_alloc` 的**写一次**注册槽承担；二次 `register` 在两种构建档都 panic，`alloc_pt_page` 在未注册时以前置断言停机；boot 侧的 bump 分配器只由各生产入口 `init_boot_pt_alloc` 设定，内 | panel/round35/refute.md 的 CLAIM 04（反证失败的源码原文核对：见该 CLAIM 的「证据」行，均为 path:line） | CLAIM-04 反证失败（主张成立），源码锚见 panel/round35/refute.md 的 CLAIM-04 |
| RD-R35-77 | RD35-CLAIM-05、CLAIM-05 | R35 | refute | P3 | claim | FP | 反证失败：05 - 主张：跨镜像三腿的接收侧只做**加性**阶段，绝不重建自举根；`arch_boot_resume_v1` 把 arena 解码进 `.bss` store 后，解码就是 arena 的最后一次读，VM 自由页在解码之后才发布，因此 arena 落在 conventional 区间时其页可以 | panel/round35/refute.md 的 CLAIM 05（反证失败的源码原文核对：见该 CLAIM 的「证据」行，均为 path:line） | CLAIM-05 反证失败（主张成立），源码锚见 panel/round35/refute.md 的 CLAIM-05 |
| RD-R35-78 | RD35-CLAIM-06、CLAIM-06 | R35 | refute | P2 | claim | ① | 非阻断：06 - 主张：文档对“证据缺口/接线状态”的两条声明是准确的：① aarch64 直核面“目前只有 L0/L3 的间接基址对账，跨距没有等价断言”（文档第 1653 行）；② 五条门链“当前 CI 只接线了 `test-riscv64-uboot.sh`，其余四条是手工门”（文档第 3299 行） | panel/round35/ 的 CLAIM-06（CLAIM） | CLAIM-06 的修复由 certify/round35-reader.md 对应条目回读确认；改动点见文档 panel/round35/refute.md |

## R31 面板轮（V3.1 第 4 轮；按 §4.0c 补清点——D-057 授权解除路径）

> 清点依据：R31 面板原始输出 `tmp/review-eval/01-stage-kernel/01-boot-shim-bootstrap/panel/round31/*.md`（53 条 FINDING）。
> 判定来源：`certify/round31-backfill-coldread.md`（本轮新派发的独立冷启动子 agent，未读本台账），逐条重放面板原文摘录与代码锚点，判出 CLOSED 33 / STILL-BROKEN 20。
> 主 agent 的分诊与它不一致处一律以冷读为准：原分诊把 D2-39 的接受臂、D2-40 的 `set_startup_len`、D2-45 的局部数组、A-06/A-07/A-09/A-10 等读者项误判为已闭合，均被回执的证据推翻。
> 20 条 STILL-BROKEN 中，事实/设计/读者 P2 全部本轮修（14 条），读者 P3 且属重复收敛或取舍补述的按 §4.0c 严重度门槛记 SUGG（6 条）。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R31-01 | FACT-A-01、FACT-A-02、FACT-A-03 | R31 | fact-a | P0 | 事实/C 时序 | ① | §2.6.4 已改写为两份 `pagedir` 换根；§2.4.2 与 §3.5 的 unpaged 归还改为「`kinfo` 已拷 `.usermapped`、活跃 `pagedir` 在 paged 副本」；§2.6.2 的 `vir2phys` 分 paged/unpaged 两支。冷读判三条 CLOSED | 冷读回执 §FACT-A-01/02/03 各附 `sed -n` 读数；现文 §2.6.4 `grep -c "paged 副本 B"` → 1 | 冷读 CLOSED（回执三条逐条给命令与输出） |
| RD-R31-02 | FACT-A-04、FACT-A-05、FACT-A-08 | R31 | fact-a | P2 | 事实/C 断言 | ① | A-04 改为「分页开启后仍在低地址执行一小段，靠 `pg_identity` 铺的恒等映射」并补 `pre_init.c:237`/`head.S:84-87`/`kernel.lds:17`；A-05 补 boot2 的 `CR0.PG` 未逐例核实；A-08 改为两个同名定义（`glo.h:9-12` 的 `_TABLE` 分支让 `table.c:30-32` 另定义 paged 副本） | `grep -c "这段代码在分页开启后不再执行" 01-boot-shim-bootstrap.md` → 0；`grep -n "pre_init.c:237"` → 1；`grep -n "glo.h:9-12"` → 1 | 冷读 CLOSED（回执 §FACT-A-04 引新句与 `awk 'NR>=84 && NR<=87' minix3/minix/kernel/arch/i386/head.S`） |
| RD-R31-03 | FACT-A-06、FACT-A-07、FACT-B-01 | R31 | fact-a/b | P3 | 事实/文档-代码同步 | ① | A-06 把「第一次被用到」限定为「第一次作为内核与模块重叠核查的输入」并补 `pre_init.c:102-103`/`:112-113`/`:162-163` 三个更早读点；A-07 补 `xboard=2`「不是首条记录」前提；B-01 §4.5 改为「`kernel_info()` 不是第二通道」并列五个真读者 | `grep -c "第一次被用到" 01-boot-shim-bootstrap.md` → 0；`grep -n "第一次作为内核与模块重叠核查的输入"` → 1；`grep -n "kernel_info()\` 不是第二通道"` → 1 | 冷读 CLOSED（回执 §FACT-A-06 引 `sed -n '159,166p' minix3/minix/kernel/arch/i386/pre_init.c`；§FACT-B-01 引 doc:2858） |
| RD-R31-04 | FACT-B-02、DESIGN-D2-42 | R31 | fact-b/design-d2 | P1 | 事实/枚举完整性 | ① | §3.4.1 的 `memmap` 行把「内核镜像在……表内」改为「两个直核自引导腿（riscv64 `bootface`、aarch64 `bootface_a64`）与 U-Boot shim 腿」，DTB 来源腿同步补 aarch64 | `grep -n "两个直核自引导腿（riscv64" 01-boot-shim-bootstrap.md` → 1；冷读回执 §DESIGN-D2-42 与 §FACT-B-02 同判 CLOSED | 冷读 CLOSED（回执两条各引 doc:1633 的现文） |
| RD-R31-05 | FACT-B-03、DESIGN-D2-37、DESIGN-D2-38 | R31 | fact-b/design-d2 | P1 | 事实/文档-代码同步 | ① | §5.2.3 退出码语义补「宿主 U-Boot 与脚本/镜像不兼容」分支与 `SKIP: host U-Boot could not load…` 文案；§3.4.1 与 §4.4 限定「三个数字来自镜像」只对三条 v1 发送腿成立；UEFI 腿补 `assert_eq!(layout.entry_point, layout.kern_virt_base.0)` | `grep -n "或宿主 U-Boot 与脚本/镜像不兼容" 01-boot-shim-bootstrap.md` → 1；冷读回执 §DESIGN-D2-38 引 `sed -n '971,981p' os/boot-shim/src/uefi_helpers.rs` 的 `assert_eq!` | 冷读 CLOSED（回执 §FACT-B-03、§DESIGN-D2-37、§DESIGN-D2-38 各附读数） |
| RD-R31-06 | DESIGN-D2-39 | R31 | design-d2 | P2 | 设计/错误路径 | ① | §4.6 补「宿主 mock 是实现差异」段：`MockPaging::map_huge` 走 `map_range`，在任意已映射虚拟页上返回 `AlreadyMapped`；三架构真实实现只在槽位已是 1 GiB 叶时返回。该分叉登记 `suggestions.md` 的 R31 段。**接受臂本轮不改**：收紧后 `cargo test -p minix-kernel --features mock` 实测 3 红（`dm_coverage::tests::test_boot_dm_{x86_64,aarch64,riscv64}_style_params` 报 `boot identity map: map_huge failed: AlreadyMapped`），证明该臂在 mock 下是重叠窗口的常态返回；改代码须同时改替身语义，超出补清点范围 | `git diff os/kernel/src/lib.rs` → 空（已回退）；`cargo test -p minix-kernel --features mock` 863 passed / 3 failed 的实测输出见 loop.md §4；`grep -n "MockPaging::map_huge" 01-boot-shim-bootstrap.md` → 1 | 冷读 STILL-BROKEN 的代码半边已定性为「替身语义分叉」并经实测复现；文档半边已修，`grep -c "接受它会把冲突当成功" doc` → 0 |
| RD-R31-07 | DESIGN-D2-40 | R31 | design-d2 | P2 | 设计/接口一致性 | ① | §4.1 末句改为：四个生产 setter（`set_modules_len`/`set_platform_len`/`set_param_len`/`set_memmap_sources_len`）各自只允许提交一次；startup 分区的生产提交口是 `push_startup`，而 `set_startup_len` 是 `#[cfg(test)]` 的测试专用入口 | `grep -n "set_startup_len\` 是 \`#\[cfg(test)\]\`" 01-boot-shim-bootstrap.md` → 1；`sed -n '292,295p' os/libs/minix-boot/src/boot_storage.rs` 命中 `#[cfg(test)] pub(crate) fn set_startup_len` | 冷读 STILL-BROKEN → 本轮按回执点名的代码事实修正文档，`grep -c "set_startup_len\` 各自只允许提交一次" doc` → 0 |
| RD-R31-08 | DESIGN-D2-41 | R31 | design-d2 | P2 | 设计/行为描述 | ① | §3.4 与 §4.3 的「二次修剪是幂等 no-op」改为「生产者按槽容量截断后继续（超限打印 `truncating`）；loader 的二次修剪对已在槽内的输入是 no-op，只对非 ELF 回退整槽生效」 | `grep -c "幂等 no-op" 01-boot-shim-bootstrap.md` → 0；`sed -n '360,370p' os/boot-shim/src/uboot_main.rs` 命中 `used > slot` 打印与 `used.min(slot)` | 冷读 STILL-BROKEN（当时未改）→ 本轮改文档；`grep -n "截断后继续" doc` → 1 |
| RD-R31-09 | DESIGN-D2-43、DESIGN-D2-44、DESIGN-D2-45 | R31 | design-d2 | P3 | 设计/加固方向 | SUGG | 三项是加固方向而非当前正确性缺陷：D2-43 直核腿的模块落点缺「目标 bump 池 vs 内核跨度」断言；D2-44 startup 记录身份在解码侧未强制；D2-45 UEFI 腿 startup 集合用栈上局部数组（第二个所有者）。按 §4.0c 严重度门槛登记，不占本轮修复预算 | 冷读回执 §DESIGN-D2-43/44/45 各给代码位置：`os/kernel-image/src/bootface_common.rs:177-275`、`os/libs/minix-boot/src/boot_storage.rs:305`、`os/boot-shim/src/uefi_helpers.rs:1155` | 已入 `suggestions.md` 的 R31 段；设计 P3 不参与 G1–G9（G3 门槛） |
| RD-R31-10 | READER-A-06、READER-A-07 | R31 | reader-a | P3 | 读者/引用点与常量 | SUGG | A-06 §2.7 的 30MB 间隔补出两个来源常量（`MB_MODS_BASE = 0x82000000` 于 `earm/pre_init.c:182`、`_kern_phys_base = 0x80200000` 于 `earm/kernel.lds:4`，相减即 `0x1E00000`）；A-07 的第三处引用点清单已随 A-04 的段落重写消除 | `grep -n "MB_MODS_BASE = 0x82000000" 01-boot-shim-bootstrap.md` → 1；`grep -c "§1.4/§2.8 只引用结论" doc` → 0 | 冷读 STILL-BROKEN（当时 30MB 段无常量）→ 本轮补值；`grep -c "0x82000000" doc` 由 0 变 1 |
| RD-R31-11 | READER-A-09、READER-B-13 | R31 | reader-a/b | P3 | 读者/过程词 | SUGG | A-09 两处讲注释编辑史的括注改为不带过程的陈述（「这里的 ISA 语义按 RISC-V 特权规范陈述，不引用源码注释」「现行注释按 `map_huge` 的错误集陈述」）；B-13 三处承重过程词改为事实陈述（`authority` 只说「第 14 个字段」、引文头去掉轮次号、窗口类型规则去掉 `DESIGN-D2-17` 编号） | `grep -c "不再冒充" 01-boot-shim-bootstrap.md` → 0；`grep -c "R23 的 F-R17-04"` → 0；`grep -c "DESIGN-D2-17"` → 0；`grep -c "R23 把五个切片"` → 0 | 冷读 STILL-BROKEN → 本轮四处逐条改写；四条 `grep -c` 复验均为 0 |
| RD-R31-12 | READER-B-03、READER-B-04 | R31 | reader-b | P2 | 读者/术语与划分 | ④ | B-03「两个构造入口」与「两条产出入径」两套叫法统一为后者（§3.4.1 标题与 §3.10 表行同步）；B-04 防线 D 的成员三处统一为「模块 × 内核跨度 + 模块 × 模块」，并说明与 C 的重复是接收侧不信任发送侧 | `grep -c "两个构造入口" 01-boot-shim-bootstrap.md` → 0；`grep -n "D 在接收侧重查同一对关系并追加模块 vs 模块" doc` → 1 | 冷读 STILL-BROKEN → 本轮按代码事实（`kernel_info.rs:346-390` 同时查两类）统一三处口径 |
| RD-R31-13 | READER-B-06、READER-B-08 | R31 | reader-b | P3 | 读者/重复与计数 | SUGG | B-06 完整名单式复述由三处降为两处（§3.7 对账表行改为「成员以 §5.2.3 为唯一权威，本节不重复名单」）；剩余一处是 §6.4 代码围栏内的脚本注释，改动会触发引文对账。B-08「两处」与「三处」的对立已改为「本节表的前三行——本小节主体的两处加 §2.5 的切占用一处」。两条均已改善但 §6.4 那处按 SUGG 留待引文批 | `grep -c "两条 UEFI 链、riscv64 U-Boot 直载测试链" doc` → 2（原 3）；`grep -n "本节表的前三行" doc` → 1 | 冷读 STILL-BROKEN → 本轮收敛两处；剩余 §6.4 围栏处登记 `suggestions.md` 的 R31 段（读者 P3，重复收敛，不构成卡住） |
| RD-R31-14 | READER-B-09、READER-B-10 | R31 | reader-b | P3 | 读者/指针 | SUGG | B-09 的第三处清单随 A-04 段落重写消除；B-10 的两个悬空指针落地：§4.5 的表加名「《落地区与交接容量》」，「忽略错误」改为指向真实存在的「窗口建表失败怎么处理」 | `grep -n "表名：《落地区与交接容量》" 01-boot-shim-bootstrap.md` → 1；`grep -c "见下文“忽略错误”一条" doc` → 0 | 冷读 STILL-BROKEN → 本轮补表名与改指针；两条 `grep` 复验见左 |
| RD-R31-15 | READER-B-11 | R31 | reader-b | P3 | 读者/清单 | SUGG | B-11 §4.2.1 动作 9 的窗口清单补入可选 `dumpdtb` 临时缓冲窗口；B-14 归 RD-R31-23 处理 | `grep -c "可选 \`dumpdtb\` 临时缓冲窗口" 01-boot-shim-bootstrap.md` → 1 | 冷读 STILL-BROKEN → 本轮按 `lib.rs:741`/`:747-780` 的实读代码统一；两条 `grep` 复验见左 |
| RD-R31-16 | READER-A-10、READER-C-03 | R31 | reader-a/c | P3 | 读者/取舍依据 | SUGG | C-03 已补 1024 与 256 两个预算各自的用途差异与「不共享常量」的理由，并删掉未验证的实测余量断言；A-10 的「两个镜像各链接一份 `BootAlloc`」五处完整因果重复**本轮不改**——压缩五处论述属结构性重写，超出补清点范围，按 §4.0c 门槛（不构成读者卡住）登记 SUGG | `grep -n "1024 与 256 的差别来自两条路径各自要建的东西" 01-boot-shim-bootstrap.md` → 1；`grep -c "QEMU 三条直核门链的实测消耗" doc` → 0 | C-03 本轮修（`grep` 复验见左）；A-10 登记 `suggestions.md` 的 R31 段，留待下一 edition 的读者重构批 |
| RD-R31-17 | READER-A-04、READER-A-11、READER-A-12 | R31 | reader-a/b | P3 | 读者/已闭合 | SUGG | 6 条经冷读判 CLOSED，本轮无动作：术语定义前移（A-04/A-05 进 §1 缩写表）、双写删除（A-08）、行号收窄到 `earm/head.S:22-25`（A-11）、条件性检查入 §4.4 表（A-12）、读图规则重写（B-01） | 冷读回执逐条附独立读数：§READER-A-11 给 `awk` 读 earm/head.S 22-25 行恰为 4 行标题注释；§READER-B-01 给 `sed -n '71p' doc` 中可 grep 到『D4 是框 4 的底边』 | 冷读 CLOSED（6 条逐条回执见 certify/round31-backfill-coldread.md） |
| RD-R31-18 | READER-B-07、READER-B-12、READER-B-15、READER-B-16 | R31 | reader-b | P3 | 读者/已闭合 | SUGG | 6 条经冷读判 CLOSED，本轮无动作：boot2 口径统一（B-02）、权威分域（B-05 的「§3.7 管生产腿、§5.2.3 管门链」）、重复收敛（B-07）、`xboard` 前提（B-12）、§3.4.1 标题（B-15）、§2.8 计数可核（B-16） | 冷读回执逐条附读数：§READER-B-16 给 `grep` 全文『只给指向』零命中；§READER-B-15 给 `sed -n '1589p' doc` 为『KernelInfo 结构与它的两条产出入径』 | 冷读 CLOSED（6 条逐条回执见 certify/round31-backfill-coldread.md） |
| RD-R31-19 | READER-C-04 | R31 | reader-c | P3 | 读者/已闭合 | SUGG | 4 条经冷读判 CLOSED，本轮无动作：取证边界补齐（C-01 的「规范只撑 GRUB 类合规引导器」）、证据强度同级（C-02 的「都是观测/预防性前提」）、首段口径（C-04）、差集对账（C-05 的「缺的正是 aarch64 直核面的专用启动门」） | 冷读回执逐条附读数：§READER-C-05 给 `sed -n '3297p' doc | grep -c '缺的正是 aarch64 直核面的专用启动门'` → 1 | 冷读 CLOSED（4 条逐条回执见 certify/round31-backfill-coldread.md） |
| RD-R31-20 | READER-A-01、READER-A-02、READER-A-03 | R31 | reader-a | P2 | 读者/已闭合 | ④ | 三条经冷读判 CLOSED，均随事实类同源修复落地：A-01 与 FACT-A-05 同处（§1 首段补 `CR0.PG` 未逐例核实）、A-02 与 FACT-A-07 同处（`xboard=2` 补「不是首条记录」）、A-03 的 arena 字节账已补 v8 九页 36864 B 与 v9 八页 32768 B 的对照 | 冷读回执 §READER-A-01 给 `sed -n '9p' doc` 中可 grep 到『实际值未逐例核实』 → 1；§READER-A-02 给 `sed -n '464p' doc` 中可 grep 到『它不是首条记录』 → 1；§READER-A-03 给 `sed -n '1751p' doc` 中可 grep 到『九页 36864 B』 → 1 | 独立冷读判全部 CLOSED，回执见 certify/round31-backfill-coldread.md 第 9 组 |
| RD-R31-21 | READER-B-02、READER-B-05 | R31 | reader-b | P2 | 读者/已闭合 | ④ | 两条经冷读判 CLOSED：B-02 与 FACT-A-05 同源（§1 首段与 §1.1 阶段表口径统一）；B-05 的权威表唯一性已由分域裁定解决（§3.7 管生产腿计数、§5.2.3 管门链计数，§1.4 只索引） | 冷读回执 §READER-B-02 给 `sed -n '9p' doc` 中可 grep 到『未逐例取证』 → 1；§READER-B-05 给 `sed -n '3297p' doc` 中可 grep 到『§3.7 是生产腿计数的唯一权威』 → 1 | 独立冷读判全部 CLOSED，回执见 certify/round31-backfill-coldread.md 第 10 组 |
| RD-R31-22 | READER-C-01、READER-C-02、READER-C-05 | R31 | reader-c | P2 | 读者/已闭合 | ④ | 三条经冷读判 CLOSED：C-01 doc:32 补取证边界（「规范只撑 GRUB 类合规引导器；boot2 只核实到代码事实」）；C-02 doc:1760 补「与 §2.4.1.3 同级，都是观测/预防性前提」；C-05 doc:3297 补差集对账（「缺的正是 aarch64 直核面的专用启动门」） | 冷读回执 §READER-C-01 给 `sed -n '32p' doc` 中可 grep 到『规范只撑 GRUB 类合规引导器』 → 1；§READER-C-02 给 `sed -n '1760p' doc` 中可 grep 到『观测/预防性』 → 1；§READER-C-05 给 `sed -n '3297p' doc` 中可 grep 到『缺的正是 aarch64 直核面的专用启动门』 | 独立冷读判全部 CLOSED，回执见 certify/round31-backfill-coldread.md 第 11 组 |
| RD-R31-24 | READER-A-05、READER-A-08、READER-B-01 | R31 | reader-a/b | P3 | 读者/已闭合 | SUGG | 三条经冷读判 CLOSED：A-05 的 `fail-closed` 已进 §1 缩写表（「失败即停机、不降级继续」）；A-08 的 `它曾装两种语义` 双写已删为单处；B-01 的读图规则已重写为「主链的框序是框 2 → 框 1 → 框 3 → 框 4」并把 D4 归位到框 4 底边 | 冷读回执 §READER-A-05 给 `sed -n '38p' doc` 中可 grep 到『fail-closed = 失败即停机』；§READER-A-08 给 `grep` 全文『它曾装两种语义』仅 1 处；§READER-B-01 给 `sed -n '71p' doc` 中可 grep 到『D4 是框 4 的底边』 | 独立冷读判全部 CLOSED，回执见 certify/round31-backfill-coldread.md 第 12 组 |
| RD-R31-23 | READER-B-14 | R31 | reader-b | P3 | 读者/层数 | SUGG | 与 RD-R31-15 同处修复：接收链层数改为「三架构都经 `resume_after_v1_decode`（`os/kernel/src/lib.rs:741`）」 | `grep -n "三架构都经 \`resume_after_v1_decode\`" 01-boot-shim-bootstrap.md` → 1 | 冷读 STILL-BROKEN → 本轮按代码实读统一；`sed -n '741p' os/kernel/src/lib.rs` 命中 `resume_after_v1_decode(&kinfo, scalars.root_page)` |

## R36 确认轮（V3.4；基线变更重开，D-060 重开条件的执行）

> 面板：`panel/round36/`（七角色：FACT 34、DESIGN-D2 9、READER 19、CLAIM 6；SUGGESTION 33；INDEX.md 由 `panel-index.py` 生成）。逐 ID 立根因（R35 同形）：每个面板 ID 一行，跨角色同根关系由 action/evidence 的组文本表达。
> 轮型 `confirmation`；修复权限 = 本轮新准入的 P0/P1/P2 事实与同步项；不实施 ⑥；读者/设计 P3 与不卡住阅读的结构建议按 §4.0c 进 suggestions。
> 基线：HEAD `b5f8ff65b` → 代码 `d8ce8b29c` → 文档 `e4374af7e` + `773e4bbc5`。

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R36-001 | FACT-A-01 | R36 | fact-a | P0 | finding | ① | FACT-A-01：文档一处声称 wire 信封版本已升到 10、另一处写当前 9，而源码常量仍是 9；同一文件内自相矛盾，且与源码不符。；处置：wire v10 版本号落地：常量 9→10 + 版本历史；文档 §3.4.1/§3.4.3/§4.6 统一为当前 10，裸 v9/v10 标签按计数器限定。 | panel/round36/fact-a.md 的 FACT-A-01；commit `d8ce8b29c`；文档 commit `e4374af7e`。 | FACT-A-01 的复验：冷读回执 CLOSED（certify/round36-fact.md）；grep -n 'BOOT_HANDOFF_V1_VERSION: u32' os/libs/minix-boot/src/handoff_v1.rs → =10；grep -n '当前 10' doc 命中；minix-boot 185/0；test-sysboot PASS。；面板原文 panel/round36/fact-a.md。 |
| RD-R36-002 | FACT-A-02 | R36 | fact-a | P1 | finding | ① | FACT-A-02：成员数与成员名单都与现行 trait 不符。现行 `Paging` 是 24 个成员（1 个关联常量 + 23 个方法）；`adopt_active_root` 已并入 `from_active_root`，不存在；名单漏了 `is_sup；处置：Paging 成员表改 24 成员；补 new_from_page(..., channel) 与 PteChannel 三态。 | panel/round36/fact-a.md 的 FACT-A-02；commit `e4374af7e`；paging.rs trait。 | FACT-A-02 的复验：awk trait 计数 fn=23；grep -n '24 个成员' doc 命中；anchor 110/110。；面板原文 panel/round36/fact-a.md。 |
| RD-R36-003 | FACT-A-03 | R36 | fact-a | P0 | finding | ① | FACT-A-03：FACT-A-03（P0）；处置：v10/D-062 删除传播：删 6b/STARTUP 现役行；七个分区/落地区统一为六个加名字池；strict 门改 memmap/reserved；D2-02 七项逐处同步。 | panel/round36/fact-a.md 的 FACT-A-03；commit `e4374af7e`；源码 `globals.rs`/`kernel_info.rs`。 | FACT-A-03 的复验：grep -n 'STARTUP_RECORD_STORE_LEN' doc 与 grep -n '七个落地区' doc 仅历史位；grep -n '六个落地区' doc 命中；citation 29/29。；面板原文 panel/round36/fact-a.md。 |
| RD-R36-004 | FACT-A-04 | R36 | fact-a | P1 | finding | ① | FACT-A-04：逐文件测试计数与当前实测不符，且不是一两个文件的漂移：`boot-shim --lib` 声称 136、实测 146；`minix-boot` 声称 195、实测 184；`minix-elf` 声称 23、实测 27；`x86_64/pa；处置：测试账重测：boot-shim 146、minix-boot 185、minix-elf 27；删虚构测试族；kernel_info 24 条。 | panel/round36/fact-a.md 的 FACT-A-04；commit `e4374af7e`。 | FACT-A-04 的复验：cargo test --list 的 146/185/27 三读数；grep -n '六枚' doc 命中 M8/§5.3；minix-boot 185/0。；面板原文 panel/round36/fact-a.md。 |
| RD-R36-005 | FACT-A-05 | R36 | fact-a | P0 | finding | ① | FACT-A-05：该文件当前只有 23 条测试：`test_validate_rejects_*` 15 条、`test_validate_accepts_*` 3 条、`test_validate_requiring_sources_*` 5 条、`tes；处置：测试账重测：boot-shim 146、minix-boot 185、minix-elf 27；删虚构测试族；kernel_info 24 条。 | panel/round36/fact-a.md 的 FACT-A-05；commit `e4374af7e`。 | FACT-A-05 的复验：cargo test --list 的 146/185/27 三读数；grep -n '六枚' doc 命中 M8/§5.3；minix-boot 185/0。；面板原文 panel/round36/fact-a.md。 |
| RD-R36-006 | FACT-A-06 | R36 | fact-a | P2 | finding | ① | FACT-A-06：`user_sp` 在 v10（D-062）已从 `KernelInfo` 删除，因此"C 有、Rust 不再保留"的字段是 22 个；21 行台账没有 `user_sp`，"全部字段"的表述不成立。；处置：§3.4.2 台账 21→22，补 user_sp 行。 | panel/round36/fact-a.md 的 FACT-A-06；commit `e4374af7e`。 | FACT-A-06 的复验：grep -n '22 个字段' doc 命中；grep -n 'user_sp' doc 命中台账。；面板原文 panel/round36/fact-a.md。 |
| RD-R36-007 | FACT-A-07 | R36 | fact-a | P1 | finding | ① | FACT-A-07：这些方法在当前源码里不存在。实际 API 是 `BootStorage::fill_map_tables(/memmap_slots, reserved_slots/ { ... })`——配对循环在闭包**内部**，计数由闭包返回并在 `；处置：幽灵 API 改真实 fill_map_tables 闭包；set_*_len/push_startup 改 fill_*。 | panel/round36/fact-a.md 的 FACT-A-07；commit `e4374af7e`；uefi_helpers.rs:1149-1218。 | FACT-A-07 的复验：grep -n 'commit_map_tables' doc 与 grep -n 'push_startup' doc → 0；grep -n 'fill_map_tables' doc 命中。；面板原文 panel/round36/fact-a.md。 |
| RD-R36-008 | FACT-A-08 | R36 | fact-a | P1 | finding | ① | FACT-A-08：当前直核腿已经把两张内存表与 source 记录写进 `BootStorage`：`os/kernel-image/src/bootface.rs:376` 与 `bootface_a64.rs:348` 调 `storage.fill_m；处置：直核所有权改 I-33 后单一所有者；六类载荷走 fill_*。 | panel/round36/fact-a.md 的 FACT-A-08；commit `e4374af7e`；bootface*.rs。 | FACT-A-08 的复验：grep -n '只有一个所有者' doc 命中；grep -n '两个所有者' doc → 0。；面板原文 panel/round36/fact-a.md。 |
| RD-R36-009 | FACT-A-09 | R36 | fact-a | P1 | finding | ① | FACT-A-09：当前测试内核里 `build_memmap()` 的实际调用点是 2 个（`test-memmap` 与 `test-memmap-aarch64`），不是 15 个；其余提到 `build_memmap` 的文件（共 14 个文件）绝大多；处置：build_memmap 直接调用 2 个测试内核；15 为两 helper 并集。 | panel/round36/fact-a.md 的 FACT-A-09；commit `e4374af7e`。 | FACT-A-09 的复验：grep -rn 'build_memmap()' os/qemu-tests 的可执行调用 2 处；grep -n '2 个测试内核' doc 命中。；面板原文 panel/round36/fact-a.md。 certify #1 补：build_memmap 直接调用 2 + classify 12 = 14 个可执行点（另 1 个注释提及，按文件 15）；doc §5.1 M13/§5.2.1 已改。 |
| RD-R36-010 | FACT-A-10 | R36 | fact-a | P1 | finding | ① | FACT-A-10：D-061 已把 aarch64 的形态判别从"运行期 `x0 == 0`"改成编译期特性分叉。直核产物（`fw-aarch64-none`）的 `rust_image_main(_arena_pa)` 忽略入参、直接调 `bootface；处置：aarch64 形态判据统一 D-061 编译期分叉；§4.8 表与 M14 删 x0=0 分道。 | panel/round36/fact-a.md 的 FACT-A-10；commit `e4374af7e`。 | FACT-A-10 的复验：grep -n 'x0=0 分道' doc → 0；grep -n '诚实停机' doc 命中；冷读回执 CLOSED。；面板原文 panel/round36/fact-a.md。 certify #1 补：§5.1 M14 首列已删 x0 分道，改为 D-061 编译期分叉；复读 CLOSED。 |
| RD-R36-011 | FACT-A-11 | R36 | fact-a | P2 | finding | ① | FACT-A-11：两处手写行号都已漂移：`kernel_allocated_dynamic: boot_alloc_used_bytes()` 在 `vm_handoff.rs:663`（605 行是模块表填充）；`resume_after_v1_decod；处置：手写行号改符号锚（vm_handoff/lib.rs）。 | panel/round36/fact-a.md 的 FACT-A-11；commit `e4374af7e`。 | FACT-A-11 的复验：anchor-resolve 110/110 multi-def=0；grep -n 'vm_handoff.rs:605' doc → 0。；面板原文 panel/round36/fact-a.md。 |
| RD-R36-012 | FACT-A-12 | R36 | fact-a | P2 | finding | ① | FACT-A-12：`uefi_helpers.rs` 里 `#[cfg(feature = "qemu-test-kernels")]` 实际有 10 处。文档列的五个条目（`UefiPoolAllocator` 的 struct+impl 2 处、`Tes；处置：qemu-test-kernels gate 8→10，补两个分类 helper。 | panel/round36/fact-a.md 的 FACT-A-12；commit `e4374af7e`。 | FACT-A-12 的复验：grep -c 'cfg(feature = "qemu-test-kernels")' uefi_helpers.rs → 10。；面板原文 panel/round36/fact-a.md。 |
| RD-R36-013 | FACT-A-13 | R36 | fact-a | P3 | finding | ① | FACT-A-13：`store_kernel_info` 里 `land_slice` 的实际调用是 4 处（`reserved_regions`、`memmap`、`platform_sources`、`memmap_sources`）；模块名与 `par；处置：land_slice 五处调用改四处；容量表与正文同步。 | panel/round36/fact-a.md 的 FACT-A-13；commit `e4374af7e`。 | FACT-A-13 的复验：grep -c 'land_slice(' lib.rs → 4；grep -n '四处调用' doc 命中。；面板原文 panel/round36/fact-a.md。 |
| RD-R36-014 | FACT-A-14 | R36 | fact-a | P1 | finding | ① | FACT-A-14：`test-kernel-map` 当前的实际顺序是：先 `alloc_root_page()`/`alloc_bump_region(64)`，再分配并写入探针页，然后调用 `uefi_helpers::exit_boot_service；处置：§5.2.3 探针描述改最终 EBS 图之前分配、来源为 exit_boot_services_and_classify。 | panel/round36/fact-a.md 的 FACT-A-14；commit `e4374af7e`；test-kernel-map:77-145。 | FACT-A-14 的复验：grep -n '最终 EBS 内存图之前' doc 命中；grep -n 'build_memmap() 产出的 conventional' doc → 0。；面板原文 panel/round36/fact-a.md。 |
| RD-R36-015 | FACT-A-15 | R36 | fact-a | P1 | finding | ① | FACT-A-15：`kmain` 已经不调用 `store_kernel_info`。D-071 / I-35(a) 明确删掉了这次重复存储：生产 `kmain` 的形参名是 `_kernel_info`，函数体只读全局 `.bss` 副本；源码注释写明 "；处置：store_kernel_info 两个生产调用点、kmain 不重存、一次性守卫；引文头补 assert 省略说明。 | panel/round36/fact-a.md 的 FACT-A-15；commit `e4374af7e`；lib.rs:225/347。 | FACT-A-15 的复验：grep -n '生产路径有两个' doc 命中；grep -n 'let kernel_info = store_kernel_info' doc → 0。；面板原文 panel/round36/fact-a.md。 |
| RD-R36-016 | FACT-B-01 | R36 | fact-b | P0 | finding | ① | FACT-B-01：文档声称 wire 版本已 `9 → 10` 且「v9 与 v10 互相拒收」，但仓库实际常量仍是 **9**；同一篇文档 §4.6 又说「信封版本仍为 9」，两处互相矛盾，源码只支持 9。；处置：wire v10 版本号落地：常量 9→10 + 版本历史；文档 §3.4.1/§3.4.3/§4.6 统一为当前 10，裸 v9/v10 标签按计数器限定。 | panel/round36/fact-b.md 的 FACT-B-01；commit `d8ce8b29c`；文档 commit `e4374af7e`。 | FACT-B-01 的复验：冷读回执 CLOSED（certify/round36-fact.md）；grep -n 'BOOT_HANDOFF_V1_VERSION: u32' os/libs/minix-boot/src/handoff_v1.rs → =10；grep -n '当前 10' doc 命中；minix-boot 185/0；test-sysboot PASS。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-017 | FACT-B-02 | R36 | fact-b | P0 | finding | ① | FACT-B-02：文档代码引用块与动作表都调用 `storage.commit_map_tables(classification.conventional, classification.reserved)`，引文里还调用 `storage.map_tab；处置：幽灵 API 改真实 fill_map_tables 闭包；set_*_len/push_startup 改 fill_*。 | panel/round36/fact-b.md 的 FACT-B-02；commit `e4374af7e`；uefi_helpers.rs:1149-1218。 | FACT-B-02 的复验：grep -n 'commit_map_tables' doc 与 grep -n 'push_startup' doc → 0；grep -n 'fill_map_tables' doc 命中。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-018 | FACT-B-03 | R36 | fact-b | P0 | finding | ① | FACT-B-03：v10（D-062）已删除 startup 分区与 `STARTUP_RECORD_STORE`，但 §4–§6 仍有成组陈述把它们当成现存对象：`STARTUP_RECORD_STORE`/`STARTUP_RECORD_STORE_LE；处置：v10/D-062 删除传播：删 6b/STARTUP 现役行；七个分区/落地区统一为六个加名字池；strict 门改 memmap/reserved；D2-02 七项逐处同步。 | panel/round36/fact-b.md 的 FACT-B-03；commit `e4374af7e`；源码 `globals.rs`/`kernel_info.rs`。 | FACT-B-03 的复验：grep -n 'STARTUP_RECORD_STORE_LEN' doc 与 grep -n '七个落地区' doc 仅历史位；grep -n '六个落地区' doc 命中；citation 29/29。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-019 | FACT-B-04 | R36 | fact-b | P0 | finding | ① | FACT-B-04：文档写「`store_kernel_info` 有三个调用点……`kmain` 拿到的是全局副本、再存一次也是同址。三个生产调用点现在都使用返回值：`kmain` 用 `let kernel_info = store_kernel_info；处置：store_kernel_info 两个生产调用点、kmain 不重存、一次性守卫；引文头补 assert 省略说明。 | panel/round36/fact-b.md 的 FACT-B-04；commit `e4374af7e`；lib.rs:225/347。 | FACT-B-04 的复验：grep -n '生产路径有两个' doc 命中；grep -n 'let kernel_info = store_kernel_info' doc → 0。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-020 | FACT-B-05 | R36 | fact-b | P0 | finding | ① | FACT-B-05：§4.4 用两层描述虚报了现行校验面。第一层是 strict 门：文档说它「另要求 `startup_objects` 至少一条覆盖内核镜像跨度（startup 不按 authority 过滤）」，并「核对三条派生标量（`user_sp`/；处置：strict 门与 postconditions 改现行断言集；补 higher-half 负控测试。 | panel/round36/fact-b.md 的 FACT-B-05；commit `e4374af7e`；源码 kernel_info.rs:554-571。 | FACT-B-05 的复验：grep -n 'kern_virt_base > u32::MAX' doc 命中；grep -n 'higher half' doc → 0；cargo test test_boot_postconditions_reject_low_virtual_base → ok。；面板原文 panel/round36/fact-b.md。 certify #1 补：断言消息与文档改为 > u32::MAX（4 GiB 门），测试 expected 同步；复跑 ok。 |
| RD-R36-021 | FACT-B-06 | R36 | fact-b | P0 | finding | ① | FACT-B-06：文档自称「实跑」的测试数与当前仓库不符，且逐文件表有 6 个文件过期。文档：boot-shim 136 / minix-boot 195 / minix-elf 23；实测：146 / 184 / 27。逐文件：`loader.rs` 22；处置：测试账重测：boot-shim 146、minix-boot 185、minix-elf 27；删虚构测试族；kernel_info 24 条。 | panel/round36/fact-b.md 的 FACT-B-06；commit `e4374af7e`。 | FACT-B-06 的复验：cargo test --list 的 146/185/27 三读数；grep -n '六枚' doc 命中 M8/§5.3；minix-boot 185/0。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-022 | FACT-B-07 | R36 | fact-b | P1 | finding | ① | FACT-B-07：文档写 `kernel_info.rs` 的构成「`test_validate_rejects_*`（16 条）与 `test_validate_accepts_*`（3 条），另加 `test_validate_requiring_sou；处置：测试账重测：boot-shim 146、minix-boot 185、minix-elf 27；删虚构测试族；kernel_info 24 条。 | panel/round36/fact-b.md 的 FACT-B-07；commit `e4374af7e`。 | FACT-B-07 的复验：cargo test --list 的 146/185/27 三读数；grep -n '六枚' doc 命中 M8/§5.3；minix-boot 185/0。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-023 | FACT-B-08 | R36 | fact-b | P1 | finding | ① | FACT-B-08：TODO-03 写「七枚 `should_panic`（reserved 257 / memmap 129 / boot modules 13 / platform 5 / param 1025 / source 297 / startup；处置：测试账重测：boot-shim 146、minix-boot 185、minix-elf 27；删虚构测试族；kernel_info 24 条。 | panel/round36/fact-b.md 的 FACT-B-08；commit `e4374af7e`。 | FACT-B-08 的复验：cargo test --list 的 146/185/27 三读数；grep -n '六枚' doc 命中 M8/§5.3；minix-boot 185/0。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-024 | FACT-B-09 | R36 | fact-b | P2 | finding | ① | FACT-B-09：两处手写行号锚点指向错误行：`kernel_allocated_dynamic: boot_alloc_used_bytes()` 在 `vm_handoff.rs:663`（不是 605）；`resume_after_v1_decode`；处置：手写行号改符号锚（vm_handoff/lib.rs）。 | panel/round36/fact-b.md 的 FACT-B-09；commit `e4374af7e`。 | FACT-B-09 的复验：anchor-resolve 110/110 multi-def=0；grep -n 'vm_handoff.rs:605' doc → 0。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-025 | FACT-B-10 | R36 | fact-b | P2 | finding | ① | FACT-B-10：引文头称只省略「注释与相邻接续行」，但源码函数体开头的 one-shot 守卫 `assert!(KERNEL_INFO.get().is_none(), "store_kernel_info: a second store would r；处置：store_kernel_info 两个生产调用点、kmain 不重存、一次性守卫；引文头补 assert 省略说明。 | panel/round36/fact-b.md 的 FACT-B-10；commit `e4374af7e`；lib.rs:225/347。 | FACT-B-10 的复验：grep -n '生产路径有两个' doc 命中；grep -n 'let kernel_info = store_kernel_info' doc → 0。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-026 | FACT-B-11 | R36 | fact-b | P2 | finding | ① | FACT-B-11：表格称 `test_build_kernel_info_riscv64_fields_match_input`「字段逐一映射（含派生字段 `kern_stack_top`/`syscall_entry`）」。测试确实存在，但只断言 `ker；处置：测试覆盖描述删 syscall_entry，注明访问器派生与已不上线字段。 | panel/round36/fact-b.md 的 FACT-B-11；commit `e4374af7e`。 | FACT-B-11 的复验：grep -n '已不上线' doc 命中 §5.2.1 表行。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-027 | FACT-B-12 | R36 | fact-b | P0 | finding | ① | FACT-B-12：D-082（I-36）把 ELF 尾部修剪从 `UbootFileLoader::read` 移到 `load_boot_modules_into`，文档 §4.3 两段代码引用与四条事实清单全都没跟上：；处置：§4.3 引文同步 D-082：补 elf_file_end 两行、UbootFileLoader 改无 segment_iter 版本、生产者注释改事实。 | panel/round36/fact-b.md 的 FACT-B-12；commit `e4374af7e` 与 `773e4bbc5`；loader.rs:507-520。 | FACT-B-12 的复验：grep -n 'elf_file_end' doc 命中；citation 29/29 failed=0。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-028 | FACT-B-13 | R36 | fact-b | P1 | finding | ① | FACT-B-13：失败模式表写「串口最后一行通常是 `kernel: step1+2 mappings ok`（卡在建根的后半段：Step 1/2 映射已过，但 **Step 2b/2c/3 与跳转尚未完成**）」。源码中 `boot_stage!("ker；处置：§6.3 step1+2 括注改为 2b/2c 已完成、卡在 Step 3 或跳转。 | panel/round36/fact-b.md 的 FACT-B-13；commit `e4374af7e`。 | FACT-B-13 的复验：grep -n 'Step 2b/2c 也已完成' doc 命中。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-029 | FACT-B-14 | R36 | fact-b | P2 | finding | ① | FACT-B-14：表行写 aarch64 直核面「无入参（`x0=0` 分道，DTB 从 RAM 基址固定位置读）」。现行直核产物是独立构建形态（`fw-aarch64-none`），入口 `rust_image_main(_arena_pa: usize)；处置：aarch64 形态判据统一 D-061 编译期分叉；§4.8 表与 M14 删 x0=0 分道。 | panel/round36/fact-b.md 的 FACT-B-14；commit `e4374af7e`。 | FACT-B-14 的复验：grep -n 'x0=0 分道' doc → 0；grep -n '诚实停机' doc 命中；冷读回执 CLOSED。；面板原文 panel/round36/fact-b.md。 certify #1 补：§5.1 M14 首列已删 x0 分道，改为 D-061 编译期分叉；复读 CLOSED。 |
| RD-R36-030 | FACT-B-15 | R36 | fact-b | P2 | finding | ① | FACT-B-15：向上取整的说明把「少一页」的条件写反了：「写小了（加数 A ≤ 4094）时，只有余数 r 满足 `r > 4096 - A` 才会少一页」。设 `n = q·4096 + r`（1 ≤ r ≤ 4095），`(n+A)/4096 = q ；处置：ceil 条件改 r>0 && r<4096-A。 | panel/round36/fact-b.md 的 FACT-B-15；commit `e4374af7e`。 | FACT-B-15 的复验：grep -n 'r < 4096 - A' doc 命中；grep -n 'r > 4096 - A' doc → 0。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-031 | FACT-B-16 | R36 | fact-b | P1 | finding | ① | FACT-B-16：该段仍按已删除的 API 描述直核自举面的 `BootStorage`：「模块、平台源、参数三类走 `set_*_len` 提交，startup 走 `push_startup` 逐条追加」「`BootStorage` 只承载 module；处置：直核所有权改 I-33 后单一所有者；六类载荷走 fill_*。 | panel/round36/fact-b.md 的 FACT-B-16；commit `e4374af7e`；bootface*.rs。 | FACT-B-16 的复验：grep -n '只有一个所有者' doc 命中；grep -n '两个所有者' doc → 0。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-032 | FACT-B-17 | R36 | fact-b | P1 | finding | ① | FACT-B-17：§5.2.3 自称是深拷不变量「三层判别」的权威记录，却把探针的内存图来源写错：文档说 `in_snapshot_free` 读的是「该测试内核自己 `build_memmap()` 产出的 conventional-only 投影（在探针；处置：§5.2.3 探针描述改最终 EBS 图之前分配、来源为 exit_boot_services_and_classify。 | panel/round36/fact-b.md 的 FACT-B-17；commit `e4374af7e`；test-kernel-map:77-145。 | FACT-B-17 的复验：grep -n '最终 EBS 内存图之前' doc 命中；grep -n 'build_memmap() 产出的 conventional' doc → 0。；面板原文 panel/round36/fact-b.md。 |
| RD-R36-033 | FACT-B-18 | R36 | fact-b | P2 | finding | ① | FACT-B-18：两处都称 `build_memmaps` 的「单表包装 `build_memmap()` 被 15 个 QEMU 测试内核调用」。实测：只有 **2 个**测试内核有 `build_memmap()` 的可执行调用（`test-memmap；处置：build_memmap 直接调用 2 个测试内核；15 为两 helper 并集。 | panel/round36/fact-b.md 的 FACT-B-18；commit `e4374af7e`。 | FACT-B-18 的复验：grep -rn 'build_memmap()' os/qemu-tests 的可执行调用 2 处；grep -n '2 个测试内核' doc 命中。；面板原文 panel/round36/fact-b.md。 certify #1 补：build_memmap 直接调用 2 + classify 12 = 14 个可执行点（另 1 个注释提及，按文件 15）；doc §5.1 M13/§5.2.1 已改。 |
| RD-R36-034 | FACT-B-19 | R36 | fact-b | P2 | finding | ① | FACT-B-19：反事实「如果改成静默跳过，错误会推迟到内核侧的数量断言，那里只有'期望 12、实际 N'，没有缺失的名字」前提不成立：内核侧没有这条数量断言。`KernelInfo::validate` 只断言 `boot_modules` 非空；`BOO；处置：静默跳过反事实前提改内核无数量断言、按位置绑定静默少初始化。 | panel/round36/fact-b.md 的 FACT-B-19；commit `e4374af7e`。 | FACT-B-19 的复验：grep -n '静默少初始化' doc 命中；grep -n '内核侧的数量断言' doc → 0。；面板原文 panel/round36/fact-b.md。 certify #1 补：静默跳过的失败点改为 strict 门位置核名（kmain 的 len 断言不会触发）；doc §5.2.1 已改。 |
| RD-R36-035 | DESIGN-D2-01 | R36 | design-d2 | P1 | finding | ① | DESIGN-D2-01：wire 信封的结构已经按 v10 改完（160→128 B、删除 `user_sp`/`kern_stack_top`/`syscall_entry`/startup 分区），但 `BOOT_HANDOFF_V1_VERSION` 仍是 ；处置：wire v10 版本号落地：常量 9→10 + 版本历史；文档 §3.4.1/§3.4.3/§4.6 统一为当前 10，裸 v9/v10 标签按计数器限定。；用户批准 2026-11-21（R36 代裁授权）。 | panel/round36/design-d2.md 的 DESIGN-D2-01；commit `d8ce8b29c`；文档 commit `e4374af7e`。 | DESIGN-D2-01 的复验：grep -n 'BOOT_HANDOFF_V1_VERSION: u32' os/libs/minix-boot/src/handoff_v1.rs → =10；grep -n '当前 10' doc 命中；minix-boot 185/0；test-sysboot PASS。；面板原文 panel/round36/design-d2.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-036 | DESIGN-D2-02 | R36 | design-d2 | P1 | finding | ① | DESIGN-D2-02：v10（D-062）清扫了字段与 wire、I-35(a)（D-071）把 `store_kernel_info` 收成 one-shot、I-33 把所有 DTB 收集改成 owned，但 §3/§4 的“门/落地区/调用点/所有权”台账；处置：v10/D-062 删除传播：删 6b/STARTUP 现役行；七个分区/落地区统一为六个加名字池；strict 门改 memmap/reserved；D2-02 七项逐处同步。；用户批准 2026-11-21（R36 代裁授权）。 | panel/round36/design-d2.md 的 DESIGN-D2-02；commit `e4374af7e`；源码 `globals.rs`/`kernel_info.rs`。 | DESIGN-D2-02 的复验：grep -n 'STARTUP_RECORD_STORE_LEN' doc 与 grep -n '七个落地区' doc 仅历史位；grep -n '六个落地区' doc 命中；citation 29/29。；面板原文 panel/round36/design-d2.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-037 | DESIGN-D2-03 | R36 | design-d2 | P3 | suggestion | SUGG | DESIGN-D2-03：v10 删除 startup 分区后，`InputObjectId::StartupObjects`、`SourceKind::StartupObject`、`SourceOrigin::StartupObject`、`KIND_START；处置：P3 建议登记 suggestions.md；不阻断。；用户批准 2026-11-21（R36 代裁授权）。 | panel/round36/design-d2.md 的 DESIGN-D2-03；panel/round36/design-d2.md DESIGN-D2-03。 | DESIGN-D2-03 的复验：grep -n 'R36' suggestions.md 命中；不进入修复队列。；面板原文 panel/round36/design-d2.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-038 | DESIGN-D2-04 | R36 | design-d2 | P2 | finding | ① | DESIGN-D2-04：§3.8（2229 行）说两个 bump 实例共享 `minix_boot::BumpRegion`，“`alloc_pages` 的边界检查、`used_bytes` 与一致性谓词都只有这一份”。但 aarch64 直核面自己写了 `bu；处置：aarch64 bump 改 BumpRegion+AtomicU64。；用户批准 2026-11-21（R36 代裁授权）。 | panel/round36/design-d2.md 的 DESIGN-D2-04；commit `d8ce8b29c`；bootface_a64.rs。 | DESIGN-D2-04 的复验：cargo check aarch64 目标完成；test-rt-birth-aarch64 到达 entering scheduler。；面板原文 panel/round36/design-d2.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-039 | DESIGN-D2-05 | R36 | design-d2 | P2 | finding | FP | 面板原判“参数化 prepare_boot 的两个安装型全局”属 re-litigation：D-064（2026-11-20）已明确采路线 B（保留全局，debug_assert→assert），否决参数化；本轮不实施、不立延期债。；用户批准 2026-11-21（R36 代裁授权：按 D-064 维持现状）。 | `DECISIONS.md` 的 D-064 行原文：用户裁决路线 B（保留安装型全局，debug_assert→assert），否决参数化；`grep -n 'D-064' rewrite-notes/coordination/review-eval/DECISIONS.md` 命中；冻结决策 + 无新增证据。 | `grep -n "D-064" DECISIONS.md` 命中路线 B；`grep -n "R36-design-batch" OPEN-DEBT.md` 为 0；re-litigation 计数 1。 |
| RD-R36-040 | DESIGN-D2-06 | R36 | design-d2 | P2 | finding | ① | DESIGN-D2-06：§3/§4 对 `Paging` 的 API 面有两处过期、一处设计缺席：；处置：Paging 成员表改 24 成员；补 new_from_page(..., channel) 与 PteChannel 三态。；用户批准 2026-11-21（R36 代裁授权）。 | panel/round36/design-d2.md 的 DESIGN-D2-06；commit `e4374af7e`；paging.rs trait。 | DESIGN-D2-06 的复验：awk trait 计数 fn=23；grep -n '24 个成员' doc 命中；anchor 110/110。；面板原文 panel/round36/design-d2.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-041 | DESIGN-D2-07 | R36 | design-d2 | P1 | finding | ① | DESIGN-D2-07：U-Boot 腿的文件索引表没有用 `BootFileTable` 类型构造，而是把 `[u8; 1296]` 当原始字节按硬编码偏移写（magic 0、entry_count 8、pad 12、entry stride 80、path 0；处置：SHIM_TABLE 改 BootFileTable 类型静态 + size_of 断言。；用户批准 2026-11-21（R36 代裁授权）。 | panel/round36/design-d2.md 的 DESIGN-D2-07；commit `d8ce8b29c`；uboot_main.rs。 | DESIGN-D2-07 的复验：grep -n 'static mut SHIM_TABLE: BootFileTable' 命中；test-riscv64-uboot-shim 6/6 PASS。；面板原文 panel/round36/design-d2.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-042 | DESIGN-D2-08 | R36 | design-d2 | P2 | finding | ① | DESIGN-D2-08：§4.3 的两段“代码引用”都不是当前源码：；处置：§4.3 引文同步 D-082：补 elf_file_end 两行、UbootFileLoader 改无 segment_iter 版本、生产者注释改事实。；用户批准 2026-11-21（R36 代裁授权）。 | panel/round36/design-d2.md 的 DESIGN-D2-08；commit `e4374af7e` 与 `773e4bbc5`；loader.rs:507-520。 | DESIGN-D2-08 的复验：grep -n 'elf_file_end' doc 命中；citation 29/29 failed=0。；面板原文 panel/round36/design-d2.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-043 | DESIGN-D2-09 | R36 | design-d2 | P3 | suggestion | SUGG | DESIGN-D2-09：配对循环的 `MapStorage` 与 DTB 覆盖检查的 claim/record 存储都是进程级 `static mut`。理由（EBS 循环内不能分配、栈可能紧）成立，但状态表达仍是 C 的“全局静态数组 + 计数器”：`MapSt；处置：P3 建议登记 suggestions.md；不阻断。；用户批准 2026-11-21（R36 代裁授权）。 | panel/round36/design-d2.md 的 DESIGN-D2-09；panel/round36/design-d2.md DESIGN-D2-09。 | DESIGN-D2-09 的复验：grep -n 'R36' suggestions.md 命中；不进入修复队列。；面板原文 panel/round36/design-d2.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-044 | READER-A-01 | R36 | reader-a | P1 | finding | ④ | READER-A-01：同一个计数器在同一篇里同时被说成"当前 9"和"9 → 10、v9 与 v10 互相拒收"。读者无法判断当前信封版本到底是 9 还是 10，也无法判断 §3.4.3 关于"本文出现的 8→9 都是它"的枚举是否还完整。另有一层同源歧义：`v；处置：wire v10 版本号落地：常量 9→10 + 版本历史；文档 §3.4.1/§3.4.3/§4.6 统一为当前 10，裸 v9/v10 标签按计数器限定。 | panel/round36/reader-a.md 的 READER-A-01；commit `d8ce8b29c`；文档 commit `e4374af7e`。 | READER-A-01 的复验：grep -n 'BOOT_HANDOFF_V1_VERSION: u32' os/libs/minix-boot/src/handoff_v1.rs → =10；grep -n '当前 10' doc 命中；minix-boot 185/0；test-sysboot PASS。；面板原文 panel/round36/reader-a.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-045 | READER-A-02 | R36 | reader-a | P1 | finding | ④ | READER-A-02：READER-A-02（P1）；处置：v10/D-062 删除传播：删 6b/STARTUP 现役行；七个分区/落地区统一为六个加名字池；strict 门改 memmap/reserved；D2-02 七项逐处同步。 | panel/round36/reader-a.md 的 READER-A-02；commit `e4374af7e`；源码 `globals.rs`/`kernel_info.rs`。 | READER-A-02 的复验：grep -n 'STARTUP_RECORD_STORE_LEN' doc 与 grep -n '七个落地区' doc 仅历史位；grep -n '六个落地区' doc 命中；citation 29/29。；面板原文 panel/round36/reader-a.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-046 | READER-A-03 | R36 | reader-a | P2 | finding | ④ | READER-A-03：同一段里，括号内三项相加是 256 + 7616 + 21312 = 29184，而句首写"合计 30624 B"；32768 − 30624 = 2144，也不是同句写的"余 3584 B"。30624 是 v9 的合计（含 1440 B；处置：arena 合计拆 v9 30624/v10 29184，余量 3584 对齐。 | panel/round36/reader-a.md 的 READER-A-03；commit `e4374af7e`。 | READER-A-03 的复验：grep -n 'v9 的合计是 30624' doc 命中；python3 -c 'print(256+7616+21312)' → 29184。；面板原文 panel/round36/reader-a.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-047 | READER-A-04 | R36 | reader-a | P2 | finding | ④ | READER-A-04：同一篇对 aarch64 直核形态的选择方式给出两种互斥说法：表与 M14 说靠运行期 `x0 == 0` 分道，§4.8 的同节说明与 §3.1.1 说 D-061 已改成编译期分叉、`x0 == 0` 只作诚实停机。读者无法判断实际机制；处置：aarch64 形态判据统一 D-061 编译期分叉；§4.8 表与 M14 删 x0=0 分道。 | panel/round36/reader-a.md 的 READER-A-04；commit `e4374af7e`。 | READER-A-04 的复验：grep -n 'x0=0 分道' doc → 0；grep -n '诚实停机' doc 命中；冷读回执 CLOSED。；面板原文 panel/round36/reader-a.md。 certify #1 补：§5.1 M14 首列已删 x0 分道，改为 D-061 编译期分叉；复读 CLOSED。 |
| RD-R36-048 | READER-A-S01 | R36 | reader-a | P3 | suggestion | SUGG | READER-A-S01：图里明明有一支标着「依赖 A（框 2 → 框 1）」的 ▲ 箭头，而括号紧跟在「D1 是框 1→3」后面，像是说 D1 与 D2 都没有箭头。读者要在图和文字之间来回对照才能确定括号只修饰 D1。；处置：建议登记 suggestions.md。 | panel/round36/reader-a.md 的 READER-A-S01；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-A-S01 的复验：grep -n 'READER-A-S01' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-a.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-049 | READER-A-S02 | R36 | reader-a | P3 | suggestion | SUGG | READER-A-S02：这是 `KernelInfo`、`v10`、`D-062` 三者在全文的首次出现，但 §1.4 没有给 `KernelInfo` 的定义指针，也没有说 `v10` 是哪个计数器的版本。读者在这一页只能把「v10 / D-062」当成一个不透；处置：建议登记 suggestions.md。 | panel/round36/reader-a.md 的 READER-A-S02；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-A-S02 的复验：grep -n 'READER-A-S02' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-a.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-050 | READER-A-S03 | R36 | reader-a | P3 | suggestion | SUGG | READER-A-S03：这棵树在「C 源码分析」章末，画的却是 x86-32 的签名（`pre_init(magic, ebx)`），顶部却把 U-Boot 与固件也列进来。读者会短暂以为 U-Boot/固件也走这条 `head.S`/`magic, ebx` 路；处置：建议登记 suggestions.md。 | panel/round36/reader-a.md 的 READER-A-S03；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-A-S03 的复验：grep -n 'READER-A-S03' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-a.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-051 | READER-A-S04 | R36 | reader-a | P3 | suggestion | SUGG | READER-A-S04：`_C_LABEL` 在全文出现三次，从未解释。读者能猜出它是给 C 符号加前缀的宏，但文档开头承诺「引用的 C 符号都有解码」，这里是一个例外；对只读基础汇编的读者，`_C_LABEL(pre_init)` 与 `pre_init` 的关；处置：建议登记 suggestions.md。 | panel/round36/reader-a.md 的 READER-A-S04；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-A-S04 的复验：grep -n 'READER-A-S04' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-a.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-052 | READER-A-S05 | R36 | reader-a | P3 | suggestion | SUGG | READER-A-S05：图例列了 `D-<nnn>`、`F-R<nn>-<nn>`、`D2-<nn>`、`BOOT-SCOPE <n><letter>`、`Phase <letter>` 等过程/裁决出处，却没有 `I-<nn>` 这一族。读者遇到 `I-34`/；处置：建议登记 suggestions.md。 | panel/round36/reader-a.md 的 READER-A-S05；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-A-S05 的复验：grep -n 'READER-A-S05' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-a.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-053 | READER-A-S06 | R36 | reader-a | P3 | suggestion | SUGG | READER-A-S06：这两句是过程/状态注记，不是机制事实。「批准后」「后续批」「本次」让读者不知道当前实现是哪一版、哪一部分已经落地；尤其 §3.4.1 的五类分类刚讲完「现行实现」，下一句又说批准后要改写，读者会怀疑自己读到的是现行还是待改写版。；处置：建议登记 suggestions.md。 | panel/round36/reader-a.md 的 READER-A-S06；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-A-S06 的复验：grep -n 'READER-A-S06' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-a.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-054 | READER-A-S07 | R36 | reader-a | P3 | suggestion | SUGG | READER-A-S07：说「合并陈述」却仍是两个独立分点，读者数不清到底是三条还是四条；前两条确实讲的是同一个「固定栈缓冲 + 丢弃尾部」的取舍。；处置：建议登记 suggestions.md。 | panel/round36/reader-a.md 的 READER-A-S07；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-A-S07 的复验：grep -n 'READER-A-S07' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-a.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-055 | READER-A-S08 | R36 | reader-a | P3 | suggestion | SUGG | READER-A-S08：全文一直把 `riscv64-uboot-shim` 定义为**发送侧**可执行体，接收侧是配套的 `kernel-uboot` 镜像。这里把「U-Boot shim」称作「v1 接收腿」，读者会误以为 shim 自己会调 `resume_；处置：建议登记 suggestions.md。 | panel/round36/reader-a.md 的 READER-A-S08；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-A-S08 的复验：grep -n 'READER-A-S08' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-a.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-056 | READER-A-S09 | R36 | reader-a | P3 | suggestion | SUGG | READER-A-S09：同一篇里「ESP」与「`%esp`」只差一个 `%`，一个指分区、一个指寄存器。熟悉 x86 的读者不会混，但§2.2 有几处把栈指针写成不带 `%` 的「esp」，正好与术语表的 ESP 撞名。；处置：建议登记 suggestions.md。 | panel/round36/reader-a.md 的 READER-A-S09；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-A-S09 的复验：grep -n 'READER-A-S09' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-a.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-057 | READER-A-S10 | R36 | reader-a | P3 | suggestion | SUGG | READER-A-S10：「悬挂指针」（dangling pointer）通常指指向已释放内存的指针，把它列为「有意为之的非零初值」会让读者以为初始化状态里有未定义行为。实际想说的应该是一个非空但不指向有效字节的哨兵值。；处置：建议登记 suggestions.md。 | panel/round36/reader-a.md 的 READER-A-S10；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-A-S10 的复验：grep -n 'READER-A-S10' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-a.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-058 | READER-B-01 | R36 | reader-b | P1 | finding | ④ | READER-B-01：职责重叠——同一批 v10 / D-062 的删除在多处没有同步，读者无法判断“startup 分区/6b/strict 门/落地区数量/信封版本”当前到底是什么。具体六组冲突：；处置：v10/D-062 删除传播：删 6b/STARTUP 现役行；七个分区/落地区统一为六个加名字池；strict 门改 memmap/reserved；D2-02 七项逐处同步。 | panel/round36/reader-b.md 的 READER-B-01；commit `e4374af7e`；源码 `globals.rs`/`kernel_info.rs`。 | READER-B-01 的复验：grep -n 'STARTUP_RECORD_STORE_LEN' doc 与 grep -n '七个落地区' doc 仅历史位；grep -n '六个落地区' doc 命中；citation 29/29。；面板原文 panel/round36/reader-b.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-059 | READER-B-02 | R36 | reader-b | P1 | finding | ④ | READER-B-02：前提倒置——aarch64 直核入口的形态判据仍按“一个镜像靠 `x0 == 0` 运行期分道”写，而同一篇刚用 D-061 把这个前提取消。表里写“无入参（`x0=0` 分道，DTB 从 RAM 基址固定位置读）”，这是“一个镜像靠 `x；处置：aarch64 形态判据统一 D-061 编译期分叉；§4.8 表与 M14 删 x0=0 分道。 | panel/round36/reader-b.md 的 READER-B-02；commit `e4374af7e`。 | READER-B-02 的复验：grep -n 'x0=0 分道' doc → 0；grep -n '诚实停机' doc 命中；冷读回执 CLOSED。；面板原文 panel/round36/reader-b.md。 certify #1 补：§5.1 M14 首列已删 x0 分道，改为 D-061 编译期分叉；复读 CLOSED。 |
| RD-R36-060 | READER-B-03 | R36 | reader-b | P2 | finding | ④ | READER-B-03：同一個表格单元里对“第三条标记”给了两种互斥说法。原文先列“`kernel loaded (entry staged)`、`12 boot modules loaded` 与第三个标记之一（有 dumpdtb 的形态出 `boot-shim；处置：aarch64 门链单元格拆成前两条固定+第三条随变体。 | panel/round36/reader-b.md 的 READER-B-03；commit `773e4bbc5`。 | READER-B-03 的复验：grep -n '第三条随变体' doc 命中；冷读回执 CLOSED。；面板原文 panel/round36/reader-b.md。 |
| RD-R36-061 | READER-B-04 | R36 | reader-b | P2 | finding | ④ | READER-B-04：“五条生产腿减去五条门链”这个集合算术与同一篇刚立的口径冲突。§3.7 明写“两个‘5’成员不同：架构×链五行含 aarch64 直核面、不含直载测试链；真机门链五条含直载测试链、不含 aarch64 直核面”（L1938）。既然两边各有一；处置：五腿五门链差集改两项。 | panel/round36/reader-b.md 的 READER-B-04；commit `773e4bbc5`。 | READER-B-04 的复验：grep -n '差集有两项' doc 命中；冷读回执 CLOSED。；面板原文 panel/round36/reader-b.md。 |
| RD-R36-062 | READER-B-05 | R36 | reader-b | P2 | finding | ④ | READER-B-05：启动路径的计数对账被完整展开了四次以上，而 §3.7 自己声明“本表是全文启动路径计数的唯一权威；其他节提到计数时只引用本表，不重新定义”（L1925）。实际是：；处置：按 §4.0e 复核后做最小结构修：§3.7 计数权威改换算基准、§5.1 补索引/详情分工、§1.4 标题改范围术语编号。 | panel/round36/reader-b.md 的 READER-B-05；commit `773e4bbc5` 与本次补修。 | READER-B-05 的复验：grep -n '换算基准' doc、grep -n '分工说明' doc、grep -n '本篇范围、术语与编号索引' doc 均命中；冷读回执 CLOSED。；面板原文 panel/round36/reader-b.md。 |
| RD-R36-063 | READER-B-06 | R36 | reader-b | P2 | finding | ④ | READER-B-06：Ch5 的三层证据（机制→要证明什么、三层证据、TODO 详情）没有用指针收敛，而是把同一批测试名、读数与断言重复展开两到三次。典型：；处置：按 §4.0e 复核后做最小结构修：§3.7 计数权威改换算基准、§5.1 补索引/详情分工、§1.4 标题改范围术语编号。 | panel/round36/reader-b.md 的 READER-B-06；commit `773e4bbc5` 与本次补修。 | READER-B-06 的复验：grep -n '换算基准' doc、grep -n '分工说明' doc、grep -n '本篇范围、术语与编号索引' doc 均命中；冷读回执 CLOSED。；面板原文 panel/round36/reader-b.md。 certify #1 补：§5.1 分工说明改为“证据摘要 + 详情在 §5.2/5.3”，§5.2.3 删“M4 只引用不重述”；复读 CLOSED。 |
| RD-R36-064 | READER-B-07 | R36 | reader-b | P2 | finding | ④ | READER-B-07：评审轮次号与轮次状态被当成正文事实，承担算术与判据的功能。§5.2.1 解释 `minix-boot` 的 195 条时写“含 R17 的三条后置负控、R18 的两条跨度复核负控、R19/R20 的三条跨度溢出负控与 R21 的一条长度对齐；处置：删 §5.2.1 轮次算术；TODO-06 改可复现判据。 | panel/round36/reader-b.md 的 READER-B-07；commit `e4374af7e`。 | READER-B-07 的复验：grep -n 'R17' doc 在 §5.2.1 段为 0；grep -n '本条判据 = 三条' doc 命中；冷读回执 CLOSED。；面板原文 panel/round36/reader-b.md。 |
| RD-R36-065 | READER-B-08 | R36 | reader-b | P2 | finding | ④ | READER-B-08：节标题只承诺“范围与可跳过内容”，实际混装了六类主题：(1) 本篇覆盖范围（L100）；(2) 两个后文名词“快照/恒等窗口”的定义（L102）；(3) 五个“栈顶”名字的对照表（L104–L112）；(4) 一张主要编号索引 + 计数约定；处置：按 §4.0e 复核后做最小结构修：§3.7 计数权威改换算基准、§5.1 补索引/详情分工、§1.4 标题改范围术语编号。 | panel/round36/reader-b.md 的 READER-B-08；commit `773e4bbc5` 与本次补修。 | READER-B-08 的复验：grep -n '换算基准' doc、grep -n '分工说明' doc、grep -n '本篇范围、术语与编号索引' doc 均命中；冷读回执 CLOSED。；面板原文 panel/round36/reader-b.md。 |
| RD-R36-066 | READER-B-09 | R36 | reader-b | P3 | suggestion | SUGG | READER-B-09：目标读者画像明确“不需要预先了解 Minix3 源码”，文档头也承诺“引用的 C 符号都有解码”，但 **“系统任务”** 这个 Minix3 专有组件从未定义；**AP** 只在“辅助处理器”之后以缩写出现，没有一句“AP = 辅助处理器；处置：§1.1 补系统任务/AP/DM 定义（已顺手修，仍按 P3 建议类）。 | panel/round36/reader-b.md 的 READER-B-09；commit `773e4bbc5`。 | READER-B-09 的复验：grep -n 'system task' doc 命中；suggestions.md R36 段登记；冷读回执 CLOSED。；面板原文 panel/round36/reader-b.md。 |
| RD-R36-067 | S-B-01 | R36 | reader-b | P3 | suggestion | SUGG | S-B-01：在读者还不知道“恒等映射/内核映射/建表五步”是什么之前，先给了两段读图元规则：“下图竖向排列只表示布局，不表示阅读顺序；阅读顺序按主链‘框 2 → 框 1 → 框 3 → 框 4’，箭头方向表示依赖方向”（L42），以及“竖线只是布局连线；处置：建议登记 suggestions.md。 | panel/round36/reader-b.md 的 S-B-01；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | S-B-01 的复验：grep -n 'S-B-01' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-b.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-068 | S-B-02 | R36 | reader-b | P3 | suggestion | SUGG | S-B-02：节一开门就是“术语对账”表和六种计法（形态 3 / 生产腿 5 / 协议段 4 / 三类链 6 / 架构×链 5 / 门链 5），此时读者还没看到“同镜像内调用 / 跨镜像跳转 / 直核自举”三种形态的具体内容，也没看到五条腿的代码。表里“；处置：建议登记 suggestions.md。 | panel/round36/reader-b.md 的 S-B-02；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | S-B-02 的复验：grep -n 'S-B-02' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-b.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-069 | S-B-03 | R36 | reader-b | P3 | suggestion | SUGG | S-B-03：原文写“U-Boot shim、x86_64 与 aarch64 三条 v1 接收腿都调用 `resume_additive`”，但全文的术语是：U-Boot **shim** 是发送腿（`prepare_boot` → `enter_ke；处置：建议登记 suggestions.md。 | panel/round36/reader-b.md 的 S-B-03；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | S-B-03 的复验：grep -n 'S-B-03' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-b.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-070 | S-B-04 | R36 | reader-b | P3 | suggestion | SUGG | S-B-04：L32 末尾写“取证范围见 §1.4 的 boot2 边界声明（规范只撑 GRUB 类合规引导器；boot2 只核实到代码事实）”；L34 末尾又写“范围见 §1.4 的 boot2 边界声明：规范只保证 Multiboot 合规引导器；b；处置：建议登记 suggestions.md。 | panel/round36/reader-b.md 的 S-B-04；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | S-B-04 的复验：grep -n 'S-B-04' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-b.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-071 | S-B-05 | R36 | reader-b | P3 | suggestion | SUGG | S-B-05：“读这段之前先认四个名字”插在 `memory_init` 代码引文（L1096–L1111）之后、`createpde` 代码引文（L1115 起）之前。四个名字里的 `get_cpulocal_var`/`ptproc`/`p_seg.；处置：建议登记 suggestions.md。 | panel/round36/reader-b.md 的 S-B-05；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | S-B-05 的复验：grep -n 'S-B-05' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-b.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-072 | S-B-06 | R36 | reader-b | P3 | suggestion | SUGG | S-B-06：开头承诺“这一层与真机层的分界线可以逐条列出”，但接下来的内容是 `cargo test` 转录、逐文件计数、契约表，没有一条“宿主层能证明什么 / 不能证明什么”的分界清单。真正接近这个承诺的是后面的“列出的测试各自挡的是哪条契约”表，但；处置：建议登记 suggestions.md。 | panel/round36/reader-b.md 的 S-B-06；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | S-B-06 的复验：grep -n 'S-B-06' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-b.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-073 | S-B-07 | R36 | reader-b | P3 | suggestion | SUGG | S-B-07：标题说“收益”，四条里第一条是“**入口形状固定**（只在 U-Boot 生产腿成立）……不能把它说成跨固件统一”，这是限制而不是收益；第四条是“**换固件不动内核**（设计承诺，观察，尚无自动化反例）”，这是未证的设计承诺。第二条“可测试；处置：建议登记 suggestions.md。 | panel/round36/reader-b.md 的 S-B-07；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | S-B-07 的复验：grep -n 'S-B-07' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-b.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-074 | S-B-08 | R36 | reader-b | P3 | suggestion | SUGG | S-B-08：小节标题是“清单移交 VM 时的 16 条截断”（C 侧），正文前两句也是 C 的 `NR_MEMS=16`，但后半段转成 Rust 的 `MEMMAP_REGION_STORE` 128 / `VmBootHandoff.free_reg；处置：建议登记 suggestions.md。 | panel/round36/reader-b.md 的 S-B-08；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | S-B-08 的复验：grep -n 'S-B-08' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-b.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-075 | S-B-09 | R36 | reader-b | P3 | suggestion | SUGG | S-B-09：三处把未闭合缺口指向 `rewrite-notes/coordination/review-eval/runs/01-stage-kernel/01-boot-shim-bootstrap/OPEN-DEBT.md` 与同目录的 `sugg；处置：建议登记 suggestions.md。 | panel/round36/reader-b.md 的 S-B-09；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | S-B-09 的复验：grep -n 'S-B-09' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-b.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-076 | S-B-10 | R36 | reader-b | P3 | suggestion | SUGG | S-B-10：同一小节先说“它对表里内容是三条去向：4KB 细粒度条目被继承；大页条目一律不继承——内核映射大页由 VM 自己重建，引导期恒等大页直接丢弃（三条去向在本小节末段交代）”（L1163）；然后贴 `pt_init` 代码，代码注释说“BIGP；处置：建议登记 suggestions.md。 | panel/round36/reader-b.md 的 S-B-10；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | S-B-10 的复验：grep -n 'S-B-10' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-b.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-077 | READER-C-01 | R36 | reader-c | P1 | finding | ④ | READER-C-01：READER-C-01（P1）；处置：v10/D-062 删除传播：删 6b/STARTUP 现役行；七个分区/落地区统一为六个加名字池；strict 门改 memmap/reserved；D2-02 七项逐处同步。 | panel/round36/reader-c.md 的 READER-C-01；commit `e4374af7e`；源码 `globals.rs`/`kernel_info.rs`。 | READER-C-01 的复验：grep -n 'STARTUP_RECORD_STORE_LEN' doc 与 grep -n '七个落地区' doc 仅历史位；grep -n '六个落地区' doc 命中；citation 29/29。；面板原文 panel/round36/reader-c.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-078 | READER-C-02 | R36 | reader-c | P2 | finding | ④ | READER-C-02：两处概述/证据表仍把 aarch64 的直核形态写成运行期 `x0 == 0` 分道（空指针转自引导面），而 §3.1.1 与 §4.8 的代码及说明明确说 D-061 已把运行期判别换成编译期构建产物分叉；表行自身还先写“无入参”再写“x；处置：aarch64 形态判据统一 D-061 编译期分叉；§4.8 表与 M14 删 x0=0 分道。 | panel/round36/reader-c.md 的 READER-C-02；commit `e4374af7e`。 | READER-C-02 的复验：grep -n 'x0=0 分道' doc → 0；grep -n '诚实停机' doc 命中；冷读回执 CLOSED。；面板原文 panel/round36/reader-c.md。 certify #1 补：§5.1 M14 首列已删 x0 分道，改为 D-061 编译期分叉；复读 CLOSED。 |
| RD-R36-079 | READER-C-03 | R36 | reader-c | P2 | finding | ④ | READER-C-03：这是同一个根因的两种表现——把 Multiboot 规范或代码事实支撑的结论，在复述时写成对 boot2 也成立的普遍结论。其一，§2.2 无条件说“第一条指令必须是设栈”，而 §1.4 / §5.1 M1 明确把“必须”限定为只对 Mul；处置：§1.1/§2.2 补 boot2 代码事实与 Multiboot 必要性边界。 | panel/round36/reader-c.md 的 READER-C-03；commit `e4374af7e`。 | READER-C-03 的复验：grep -n 'boot2 只核实到代码事实' doc 命中；冷读回执 CLOSED。；面板原文 panel/round36/reader-c.md。 |
| RD-R36-080 | READER-C-04 | R36 | reader-c | P3 | suggestion | SUGG | READER-C-04：这段话给出的“少一页”条件写反了。设 `x = q*4096 + r`（`0 ≤ r < 4096`），公式 `(x+A)/4096` 在 `r > 0` 且 `r + A < 4096` 时才会比 `ceil(x/4096)` 少一页，即；处置：ceil 条件改 r>0 && r<4096-A。 | panel/round36/reader-c.md 的 READER-C-04；commit `e4374af7e`。 | READER-C-04 的复验：grep -n 'r < 4096 - A' doc 命中；grep -n 'r > 4096 - A' doc → 0。；面板原文 panel/round36/reader-c.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-081 | READER-C-05 | R36 | reader-c | P2 | finding | ④ | READER-C-05：引文头声称“五处调用”都保留，实际只显示四处；第七个落地区（`STARTUP_RECORD_STORE`）既没有出现在引文里，也没有用省略号标出“这里还有一块”。这与“七个落地区”的计数和 `STARTUP_RECORD_STORE` 的落；处置：land_slice 五处调用改四处；容量表与正文同步。 | panel/round36/reader-c.md 的 READER-C-05；commit `e4374af7e`。 | READER-C-05 的复验：grep -c 'land_slice(' lib.rs → 4；grep -n '四处调用' doc 命中。；面板原文 panel/round36/reader-c.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-082 | READER-C-06 | R36 | reader-c | P3 | suggestion | SUGG | READER-C-06：同一机制在概述与详述里给出两种故障时点：§3.4.2 说重建活动根后“下一条取指页错误”，§3.7 说 TLB 残留可能让 CPU 先跑几条，故障落在下一个冷取指。前者是即时失败，后者是一段静默窗口；这正是 §3.7 用来说明“故障点与根因；处置：§3.4.2 改冷取指页错误（已顺手修，仍按 P3 建议类）。 | panel/round36/reader-c.md 的 READER-C-06；commit `e4374af7e`。 | READER-C-06 的复验：grep -n '冷取指页错误' doc 命中；suggestions.md R36 段登记；冷读回执 CLOSED。；面板原文 panel/round36/reader-c.md。 |
| RD-R36-083 | READER-C-S01 | R36 | reader-c | P3 | suggestion | SUGG | READER-C-S01：READER-C-S01（指针）；处置：建议登记 suggestions.md。 | panel/round36/reader-c.md 的 READER-C-S01；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-C-S01 的复验：grep -n 'READER-C-S01' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-c.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-084 | READER-C-S02 | R36 | reader-c | P3 | suggestion | SUGG | READER-C-S02：READER-C-S02（过程词进正文）；处置：建议登记 suggestions.md。 | panel/round36/reader-c.md 的 READER-C-S02；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-C-S02 的复验：grep -n 'READER-C-S02' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-c.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-085 | READER-C-S03 | R36 | reader-c | P3 | suggestion | SUGG | READER-C-S03：READER-C-S03（指针不带路径）；处置：建议登记 suggestions.md。 | panel/round36/reader-c.md 的 READER-C-S03；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-C-S03 的复验：grep -n 'READER-C-S03' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-c.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-086 | READER-C-S04 | R36 | reader-c | P3 | suggestion | SUGG | READER-C-S04：READER-C-S04（§7.1 条目自指不明）；处置：建议登记 suggestions.md。 | panel/round36/reader-c.md 的 READER-C-S04；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-C-S04 的复验：grep -n 'READER-C-S04' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-c.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-087 | READER-C-S05 | R36 | reader-c | P3 | suggestion | SUGG | READER-C-S05：READER-C-S05（重复事实）；处置：建议登记 suggestions.md。 | panel/round36/reader-c.md 的 READER-C-S05；panel/round36 的对应 SUGGESTION；suggestions.md R36 段。 | READER-C-S05 的复验：grep -n 'READER-C-S05' suggestions.md 命中或按组登记；不进入修复队列。；面板原文 panel/round36/reader-c.md。 冷读回执 CLOSED（certify/round36-reader.md 对应段）。 |
| RD-R36-088 | CLAIM-01 | R36 | refute | P3 | claim | FP | CLAIM-01：§1.1/§2.2 的 C 侧入口结论——`i386/head.S` 的 `multiboot_init` 第一条指令必须设栈，且 `load_stack_start` 由链接期布局保证 4096 对齐；按 `push $0; popf; ；处置：见 panel/round36 原文与 R36 台账对应组。 | panel/round36/refute.md 的 CLAIM-01；panel/round36。 | CLAIM-01 的复验：见本行证据与 loop §2。；面板原文 panel/round36/refute.md。 |
| RD-R36-089 | CLAIM-02 | R36 | refute | P3 | claim | FP | CLAIM-02：§2.4.1.3 的 C 侧结论——`alloc_lowest` 的唯一切除调用把 `len` 当 `end` 传给 `cut_memmap`；在文档列出的四个前提同时成立时，真实调用序落进“第三行”（`len ≤ lowest`，切除一次；处置：见 panel/round36 原文与 R36 台账对应组。 | panel/round36/refute.md 的 CLAIM-02；panel/round36。 | CLAIM-02 的复验：见本行证据与 loop §2。；面板原文 panel/round36/refute.md。 |
| RD-R36-090 | CLAIM-03 | R36 | refute | P0 | claim | ① | CLAIM-03：§3.4.1/§3.4.3 的交接契约结论——当前跨镜像 wire 是 v10：`BOOT_HANDOFF_V1_VERSION` 已从 9 升到 10，收发两侧精确比较，v9 与 v10 互相拒收；v10 删掉 `user_sp`/`ke；处置：wire v10 版本号落地：常量 9→10 + 版本历史；文档 §3.4.1/§3.4.3/§4.6 统一为当前 10，裸 v9/v10 标签按计数器限定。 | panel/round36/refute.md 的 CLAIM-03；commit `d8ce8b29c`；文档 commit `e4374af7e`。 | CLAIM-03 的复验：grep -n 'BOOT_HANDOFF_V1_VERSION: u32' os/libs/minix-boot/src/handoff_v1.rs → =10；grep -n '当前 10' doc 命中；minix-boot 185/0；test-sysboot PASS。；面板原文 panel/round36/refute.md。 |
| RD-R36-091 | CLAIM-04 | R36 | refute | P0 | claim | ① | CLAIM-04：§4.5/§3.4.5/§4.4 的内核落地区与校验结论——`store_kernel_info` 有“七个落地区 + 一个名字池”，三个生产调用点（`arch_boot_finish`、`arch_boot_resume_v1`、`kma；处置：v10/D-062 删除传播：删 6b/STARTUP 现役行；七个分区/落地区统一为六个加名字池；strict 门改 memmap/reserved；D2-02 七项逐处同步。 | panel/round36/refute.md 的 CLAIM-04；commit `e4374af7e`；源码 `globals.rs`/`kernel_info.rs`。 | CLAIM-04 的复验：grep -n 'STARTUP_RECORD_STORE_LEN' doc 与 grep -n '七个落地区' doc 仅历史位；grep -n '六个落地区' doc 命中；citation 29/29。；面板原文 panel/round36/refute.md。 |
| RD-R36-092 | CLAIM-05 | R36 | refute | P1 | claim | ① | CLAIM-05：§3.6 与 §4.6 的 `map_huge` 结论——`AlreadyMapped` 只在“目标槽位已经是一个 1 GiB 叶”时返回；同一 2 MiB 片被两个窗口各写一次时，三个架构的 2 MiB 路径都直接重写同一个叶并返回 `O；处置：见 panel/round36 原文与 R36 台账对应组。 | panel/round36/refute.md 的 CLAIM-05；panel/round36。 | CLAIM-05 的复验：commit `e4374af7e` 的 §3.6/§4.6 map_huge 段；grep -n '重复安装同一个叶' doc 命中。；面板原文 panel/round36/refute.md。 |
| RD-R36-093 | CLAIM-06 | R36 | refute | P1 | claim | ① | CLAIM-06：§5.2.1 与 §4.3 的证据总账是当前树的实测记录——“本节所有数字按 `#[test]` 出现次数计……当前值以本逐文件表为准”，`boot-shim --lib` 为 136（22+11+12+45+17+15+13+1），`mi；处置：测试账重测：boot-shim 146、minix-boot 185、minix-elf 27；删虚构测试族；kernel_info 24 条。 | panel/round36/refute.md 的 CLAIM-06；commit `e4374af7e`。 | CLAIM-06 的复验：cargo test --list 的 146/185/27 三读数；grep -n '六枚' doc 命中 M8/§5.3；minix-boot 185/0。；面板原文 panel/round36/refute.md。 |

## R37 重写基线（V3.4；full）

| 根因 ID | 发现 ID | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|---|---|
| RD-R37-001 | FACT-01 | R37 | fact | P1 | 文档结构 | ① | §5.4 补五条真机门链表并调整 5.3/5.4 次序；§3.6/§6.4 指向真实表格 | §5.4、§3.6、§6.4；panel/round37/fact.md | FACT-01 修复点回读：§5.4 补五条真机门链表并调整 5.3/5.4 次序；§3.6/§6.4 指向真实表格；独立回执见 certify 同轮文件 |
| RD-R37-002 | FACT-02 | R37 | fact | P1 | 事实/校验门 | ① | §3.7 改为 postconditions 只剩 kern_virt_base > u32::MAX，重叠检查归 validate_impl_inner | §3.7、§4.4；panel/round37/fact.md | FACT-02 修复点回读：§3.7 改为 postconditions 只剩 kern_virt_base > u32::MAX，重叠检查归 validate_impl_inner；独立回执见 certify 同轮文件 |
| RD-R37-003 | FACT-03 | R37 | fact | P2 | 事实/日志 | ① | 删 ghost `arch_boot arg=`/`reslen=`，§6.1 只留真实 `arch_boot entered` 与 `preA rdi` | §6.1；panel/round37/fact.md | FACT-03 修复点回读：删 ghost `arch_boot arg=`/`reslen=`，§6.1 只留真实 `arch_boot entered` 与 `preA rdi`；独立回执见 certify 同轮文件 |
| RD-R37-004 | FACT-04 | R37 | fact | P2 | 事实/顺序 | ① | §1.2 I4 与 §3.7 改成逐腿实际门序（strict 在 arch_boot 之前） | §1.2、§3.7、§4.4；panel/round37/fact.md | FACT-04 修复点回读：§1.2 I4 与 §3.7 改成逐腿实际门序（strict 在 arch_boot 之前）；独立回执见 certify 同轮文件 |
| RD-R37-005 | FACT-05 | R37 | fact | P2 | 事实/引文 | ① | §4.2 UEFI 块恢复 `(&layout)`、`.validate()`、`layout.layout()` | §4.2；panel/round37/fact.md | FACT-05 修复点回读：§4.2 UEFI 块恢复 `(&layout)`、`.validate()`、`layout.layout()`；独立回执见 certify 同轮文件 |
| RD-R37-006 | FACT-06 | R37 | fact | P2 | 事实/内存序 | ① | §3.4.2 BootAlloc 三处 load 改 `Acquire` | §3.4.2；panel/round37/fact.md | FACT-06 修复点回读：§3.4.2 BootAlloc 三处 load 改 `Acquire`；独立回执见 certify 同轮文件 |
| RD-R37-007 | FACT-07 | R37 | fact | P2 | 文档结构 | ① | §3.8 指针改 §3.4.2/§3.2.4；`§3.4 与 §3.4` 拆成 §3.4.1/§3.4.3 | §3.2.4、§3.4.1、§3.4.2、§4.6；panel/round37/fact.md | FACT-07 修复点回读：§3.8 指针改 §3.4.2/§3.2.4；`§3.4 与 §3.4` 拆成 §3.4.1/§3.4.3；独立回执见 certify 同轮文件 |
| RD-R37-008 | FACT-08 | R37 | fact | P3 | 事实/负声明 | ① | §3.2.1 删“仅 bootface_a64.rs 注释提及 FREE_MEMMAP” | §3.2.1；panel/round37/fact.md | FACT-08 修复点回读：§3.2.1 删“仅 bootface_a64.rs 注释提及 FREE_MEMMAP”；独立回执见 certify 同轮文件 |
| RD-R37-009 | FACT-09 | R37 | fact | P3 | 事实/容量 | ① | §4.3 槽容量补 `mfs` 专用 16 MiB | §4.3；panel/round37/fact.md | FACT-09 修复点回读：§4.3 槽容量补 `mfs` 专用 16 MiB；独立回执见 certify 同轮文件 |
| RD-R37-010 | FACT-10 | R37 | fact | P3 | 事实/诊断 | ① | §6.4 改查 `kern_virt_base + kern_size` 或 `KernelInfo::kern_stack_top()` | §6.4；panel/round37/fact.md | FACT-10 修复点回读：§6.4 改查 `kern_virt_base + kern_size` 或 `KernelInfo::kern_stack_top()`；独立回执见 certify 同轮文件 |
| RD-R37-011 | FACT-11 | R37 | fact | P3 | 事实/注释来源 | ① | §4.1 的 256 页覆盖清单改为正文自述，不再假托 bootface.rs 注释 | §4.1；panel/round37/fact.md | FACT-11 修复点回读：§4.1 的 256 页覆盖清单改为正文自述，不再假托 bootface.rs 注释；独立回执见 certify 同轮文件 |
| RD-R37-012 | FACT-12 | R37 | fact | P3 | 文档/占位词 | ① | 清掉“阶段”占位与 R02-阶段 残片，恢复具体阶段/测试名 | §3.4.1、§5.5；panel/round37/fact.md | FACT-12 修复点回读：清掉“阶段”占位与 R02-阶段 残片，恢复具体阶段/测试名；独立回执见 certify 同轮文件 |
| RD-R37-013 | READER-A-01 | R37 | reader | P1 | 事实/顺序 | ① | 同 FACT-04：I4 判据改为逐腿实际门序 | §1.2、§4.4；panel/round37/reader-a.md | READER-A-01 修复点回读：同 FACT-04：I4 判据改为逐腿实际门序；独立回执见 certify 同轮文件 |
| RD-R37-014 | READER-A-02 | R37 | reader | P1 | 事实/校验门 | ① | 同 FACT-02：§3.7 的门内容与 §4.4 对齐 | §3.7；panel/round37/reader-a.md | READER-A-02 修复点回读：同 FACT-02：§3.7 的门内容与 §4.4 对齐；独立回执见 certify 同轮文件 |
| RD-R37-015 | READER-A-03 | R37 | reader | P2 | 文档结构 | ① | §1.1 补框 1/框 2 定义，§2.4 的“最终形态”表述与定义一致 | §1.1、§2.4；panel/round37/reader-a.md | READER-A-03 修复点回读：§1.1 补框 1/框 2 定义，§2.4 的“最终形态”表述与定义一致；独立回执见 certify 同轮文件 |
| RD-R37-016 | READER-A-04 | R37 | reader | P2 | 文档结构 | ① | §1.1 内核位置行注明 `entry_point` 不是 KernelInfo 字段，只作装载断言 | §1.1；panel/round37/reader-a.md | READER-A-04 修复点回读：§1.1 内核位置行注明 `entry_point` 不是 KernelInfo 字段，只作装载断言；独立回执见 certify 同轮文件 |
| RD-R37-017 | READER-A-05 | R37 | reader | P2 | 文档结构 | ① | §2.8 改“D1–D5 定义在 §1.4”，不再说与 I1–I4 一一对应 | §2.8；panel/round37/reader-a.md | READER-A-05 修复点回读：§2.8 改“D1–D5 定义在 §1.4”，不再说与 I1–I4 一一对应；独立回执见 certify 同轮文件 |
| RD-R37-018 | READER-A-06 | R37 | reader | P2 | 事实/计数 | ① | §3.2.1 删除行改 22 个字段并补 `user_sp` | §3.2.1；panel/round37/reader-a.md | READER-A-06 修复点回读：§3.2.1 删除行改 22 个字段并补 `user_sp`；独立回执见 certify 同轮文件 |
| RD-R37-019 | READER-A-07 | R37 | reader | P2 | 文档结构 | ① | 同 FACT-01：§5.4 表落地，引用不再悬空 | §5.4；panel/round37/reader-a.md | READER-A-07 修复点回读：同 FACT-01：§5.4 表落地，引用不再悬空；独立回执见 certify 同轮文件 |
| RD-R37-020 | READER-B-01 | R37 | reader | P1 | 事实/顺序 | ① | 同 FACT-04 | §1.2、§3.7、§4.4；panel/round37/reader-b.md | READER-B-01 修复点回读：同 FACT-04；独立回执见 certify 同轮文件 |
| RD-R37-021 | READER-B-02 | R37 | reader | P1 | 文档结构 | ① | §5 开头补 J1–J4 → 小节的证据映射，§1.2 的承诺可核 | §5、§1.2；panel/round37/reader-b.md | READER-B-02 修复点回读：§5 开头补 J1–J4 → 小节的证据映射，§1.2 的承诺可核；独立回执见 certify 同轮文件 |
| RD-R37-022 | READER-B-03 | R37 | reader | P1 | 文档结构 | ① | §3.7/§3.8/§4.6 的断链改为 §3.4.2、§3.2.4 等真实承载节 | §3.4.2、§3.7、§4.6；panel/round37/reader-b.md | READER-B-03 修复点回读：§3.7/§3.8/§4.6 的断链改为 §3.4.2、§3.2.4 等真实承载节；独立回执见 certify 同轮文件 |
| RD-R37-023 | READER-B-04 | R37 | reader | P1 | 文档结构 | ① | 框 1/框 2 定义补在 §1.1，§2.3/§2.4/§2.8 的指针同步 | §1.1、§2.3、§2.4、§2.8；panel/round37/reader-b.md | READER-B-04 修复点回读：框 1/框 2 定义补在 §1.1，§2.3/§2.4/§2.8 的指针同步；独立回执见 certify 同轮文件 |
| RD-R37-024 | READER-B-05 | R37 | reader | P2 | 事实/计数 | ① | 同 READER-A-06 | §3.2.1、§3.2.2；panel/round37/reader-b.md | READER-B-05 修复点回读：同 READER-A-06；独立回执见 certify 同轮文件 |
| RD-R37-025 | READER-B-06 | R37 | reader | P1 | 文档结构 | ① | I3 行指向 §3.7 所有权开头；§3.4 标题去掉 (I3)，§3.7 改“所有权与校验门链（I3/I4）” | §1.2、§3.4、§3.7；panel/round37/reader-b.md | READER-B-06 修复点回读：I3 行指向 §3.7 所有权开头；§3.4 标题去掉 (I3)，§3.7 改“所有权与校验门链（I3/I4）”；独立回执见 certify 同轮文件 |
| RD-R37-026 | READER-B-07 | R37 | reader | P2 | 文档结构 | ① | §4.7 与 §7.5 统一优先序：源码 > 定义节 > 速查表 | §4.7、§7.5；panel/round37/reader-b.md | READER-B-07 修复点回读：§4.7 与 §7.5 统一优先序：源码 > 定义节 > 速查表；独立回执见 certify 同轮文件 |
| RD-R37-027 | READER-B-08 | R37 | reader | P2 | 文档结构 | ① | §1.1/§1.4 改“第一次/第二次跳转”，§2.2 标题改“入口”，不再双指 | §1.1、§1.4、§2.2；panel/round37/reader-b.md | READER-B-08 修复点回读：§1.1/§1.4 改“第一次/第二次跳转”，§2.2 标题改“入口”，不再双指；独立回执见 certify 同轮文件 |
| RD-R37-028 | READER-B-09 | R37 | reader | P2 | 文档结构 | ① | §1.4 补“VM 接手后内存账生命周期”越界行，§2.4.2 的回指成立 | §1.4、§2.4.2；panel/round37/reader-b.md | READER-B-09 修复点回读：§1.4 补“VM 接手后内存账生命周期”越界行，§2.4.2 的回指成立；独立回执见 certify 同轮文件 |
| RD-R37-029 | READER-B-10 | R37 | reader | P2 | 文档结构 | ① | §1.1/§1.2 的标识符加“见 §3.7/§4.4”的就地指路 | §1.1、§1.2；panel/round37/reader-b.md | READER-B-10 修复点回读：§1.1/§1.2 的标识符加“见 §3.7/§4.4”的就地指路；独立回执见 certify 同轮文件 |
| RD-R37-030 | READER-C-01 | R37 | reader | P1 | 事实/顺序 | ① | 同 FACT-04 | §1.2、§3.7、§4.4；panel/round37/reader-c.md | READER-C-01 修复点回读：同 FACT-04；独立回执见 certify 同轮文件 |
| RD-R37-031 | READER-C-02 | R37 | reader | P1 | 文档结构 | ① | 同 FACT-01 | §5.4；panel/round37/reader-c.md | READER-C-02 修复点回读：同 FACT-01；独立回执见 certify 同轮文件 |
| RD-R37-032 | READER-C-03 | R37 | reader | P2 | 文档结构 | ① | 同 READER-A-04 | §1.1；panel/round37/reader-c.md | READER-C-03 修复点回读：同 READER-A-04；独立回执见 certify 同轮文件 |
| RD-R37-033 | READER-C-04 | R37 | reader | P2 | 文档结构 | ① | 同 READER-A-03/READER-B-04 | §1.1、§2.4；panel/round37/reader-c.md | READER-C-04 修复点回读：同 READER-A-03/READER-B-04；独立回执见 certify 同轮文件 |
| RD-R37-034 | READER-C-05 | R37 | reader | P2 | 事实/因果 | ① | §3.3 来源记录理由改为“跨镜像无法再问引导器；直核保留同一审计” | §3.3；panel/round37/reader-c.md | READER-C-05 修复点回读：§3.3 来源记录理由改为“跨镜像无法再问引导器；直核保留同一审计”；独立回执见 certify 同轮文件 |
| RD-R37-035 | READER-C-06 | R37 | reader | P2 | 文档结构 | ① | §2.6 的 FileLoader 结论补直核面例外，§3.5 分工一致 | §2.6、§3.5；panel/round37/reader-c.md | READER-C-06 修复点回读：§2.6 的 FileLoader 结论补直核面例外，§3.5 分工一致；独立回执见 certify 同轮文件 |
| RD-R37-036 | READER-C-07 | R37 | reader | P2 | 文档结构 | ① | 同 READER-B-03：§4.6 的 3.8/3.x 指针改为真实节 | §4.6；panel/round37/reader-c.md | READER-C-07 修复点回读：同 READER-B-03：§4.6 的 3.8/3.x 指针改为真实节；独立回执见 certify 同轮文件 |
| RD-R37-037 | READER-C-08 | R37 | reader | P2 | 文档结构 | ① | §3.4.1/§5.5 的占位阶段名与 R02 编号恢复为具体判据 | §3.4.1、§5.5；panel/round37/reader-c.md | READER-C-08 修复点回读：§3.4.1/§5.5 的占位阶段名与 R02 编号恢复为具体判据；独立回执见 certify 同轮文件 |
| RD-R37-038 | READER-C-09 | R37 | reader | P3 | 文档结构 | SUGG | §5.4 明确 test-sysboot 不是启动门，避免与 x86 门混淆 | §5.4；panel/round37/reader-c.md | READER-C-09 修复点回读：§5.4 明确 test-sysboot 不是启动门，避免与 x86 门混淆；独立回执见 certify 同轮文件 |
| RD-R37-039 | READER-C-10 | R37 | reader | P3 | 事实/计数 | SUGG | 同 READER-A-06；字段数统一为 22 | §3.2.1、§3.2.2；panel/round37/reader-c.md | READER-C-10 修复点回读：同 READER-A-06；字段数统一为 22；独立回执见 certify 同轮文件 |
| RD-R37-040 | READER-C-11 | R37 | reader | P3 | 文档结构 | SUGG | §2.6 的 overlaps 半开判据与 C 的 `-1` 差异已按源码重述 | §2.6；panel/round37/reader-c.md | READER-C-11 修复点回读：§2.6 的 overlaps 半开判据与 C 的 `-1` 差异已按源码重述；独立回执见 certify 同轮文件 |
| RD-R37-041 | CLAIM-01 | R37 | refute | P3 | 反证/否证 | FP | 反证失败：文档的 alloc_lowest 结论与源码一致，无缺陷 | panel/round37/refute.md CLAIM-01 | CLAIM-01 修复点回读：反证失败：文档的 alloc_lowest 结论与源码一致，无缺陷；独立回执见 certify 同轮文件 |
| RD-R37-042 | CLAIM-02 | R37 | refute | P2 | 事实/空键 | ① | §2.3 补 C 空键删除会截断后续记录的副作用，并说明 Rust 拒绝该输入 | §2.3；panel/round37/refute.md CLAIM-02 | CLAIM-02 修复点回读：§2.3 补 C 空键删除会截断后续记录的副作用，并说明 Rust 拒绝该输入；独立回执见 certify 同轮文件 |
| RD-R37-043 | CLAIM-03 | R37 | refute | P3 | 反证/否证 | FP | 反证失败：ARM32 闭区间判定结论成立，无缺陷 | panel/round37/refute.md CLAIM-03 | CLAIM-03 修复点回读：反证失败：ARM32 闭区间判定结论成立，无缺陷；独立回执见 certify 同轮文件 |
| RD-R37-044 | CLAIM-04 | R37 | refute | P1 | 事实/校验门 | ① | 同 FACT-02/READER-A-01：postconditions 与 I4 时序修正 | §3.7、§1.2；panel/round37/refute.md CLAIM-04 | CLAIM-04 修复点回读：同 FACT-02/READER-A-01：postconditions 与 I4 时序修正；独立回执见 certify 同轮文件 |
| RD-R37-045 | CLAIM-05 | R37 | refute | P2 | 事实/所有权 | ① | §3.7 限定 UEFI source 记录不走 fill_*，第二次 fill panic 只对实际 fill 腿成立 | §3.7；panel/round37/refute.md CLAIM-05 | CLAIM-05 修复点回读：§3.7 限定 UEFI source 记录不走 fill_*，第二次 fill panic 只对实际 fill 腿成立；独立回执见 certify 同轮文件 |
| RD-R37-046 | CLAIM-06 | R37 | refute | P3 | 反证/否证 | FP | 反证失败：manifest/调用面结论成立，无缺陷 | panel/round37/refute.md CLAIM-06 | CLAIM-06 修复点回读：反证失败：manifest/调用面结论成立，无缺陷；独立回执见 certify 同轮文件 |
## R38 V3.10 新 edition（full；L0.9 清理 + 面板）

| 根因 ID | 发现 ID | 单元 | origin | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 | red_cmd | red_evidence | green_cmd | green_evidence |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| RD-R38-001 | FACT-01 | 2.4 | R38 | fact | P1 | 文档/引文 | ① | 把引文块 `4 GiB` 改回源码的 `4GB` | `grep -n 'Truncate available memory' minix3/minix/kernel/arch/i386/pg_utils.c` 命中 4GB；doc-citation 0 failed | CLOSED；doc-citation 0 failed；`rg 'Truncate available memory at 4 GiB' doc` 归零 | — | — | — | — |
| RD-R38-002 | READER-C-01 | 2.8 | R38 | reader-c | P1 | 文档/计数 | ① | §2.8 “四份交接对象”改为“三份” | reader-c §2.8 vs §1.0/§1.1 对读 | CLOSED；panel/round38/reader-c.md:6；`rg '四份交接' doc` 归零 | — | — | — | — |
| RD-R38-003 | READER-C-02 | 2.8 | R38 | reader-c | P1 | 文档/证据范围 | ① | §2.8 改为 D1/D2/D4 本章、D3/D5 在 02 篇 J3 | reader-c 对读 §2.0/§2.8 | CLOSED；panel/round38/reader-c.md:19；`rg '五条的证据都在本章' doc` 归零 | — | — | — | — |
| RD-R38-004 | READER-C-03 | 5.1 | R38 | reader-c | P1 | 文档/过度概括 | ① | §5.1 I3 真机证据限定为“五条腿都有门”，fail-closed 形态不算到 kmain | reader-c 对读 §5.1/§5.4/§3.1.1 | CLOSED；panel/round38/reader-c.md:32；`rg '五条腿都走到' doc` 归零 | — | — | — | — |
| RD-R38-005 | REFUTE-04 | 3.7 | R38 | refute | P1 | 代码/所有权 | ① | `fill_map_tables` 加 once 断言；新增负控测试 | `boot_storage.rs:139-157` 无 one-shot；只有 `fill_modules_twice_is_refused` | CLOSED；panel/round38/refute.md:29；`cargo test -p minix-boot --lib fill_map_tables_twice_is_refused` PASS | `cargo test -p minix-boot --lib fill_map_tables_twice_is_refused` | 测试在无断言时失败（未 panic） | `cargo test -p minix-boot --lib fill_map_tables_twice_is_refused` | 1 passed; 0 failed |
| RD-R38-006 | READER-A-01 | 3.2.1 | R38 | reader-a | P2 | 文档/术语 | ① | §1.0 术语补 PT_LOAD/LMA/VMA/p_filesz/p_memsz/e_entry | reader-a 指出首次使用无定义 | CLOSED；panel/round38/reader-a.md:5；doc-ref-check 0 | — | — | — | — |
| RD-R38-007 | READER-B-01 | 3.2.2 | R38 | reader-b | P2 | 文档/导航 | ① | §3.2.2 拆 3.2.2.1–3.2.2.4 四个子节 | reader-b 指出职责混装、指针不可落地 | CLOSED；panel/round38/reader-b.md:28；`rg '^##### 3.2.2' doc` 4 行 | — | — | — | — |
| RD-R38-008 | READER-B-02 | 4.4 | R38 | reader-b | P2 | 文档/导航 | ① | §4.4 增 `4.4.1 门链与逐腿执行序`，指针改指该节 | reader-b 指出权威表无标题 | CLOSED；panel/round38/reader-b.md:41；`rg '^#### 4.4.1' doc` 命中 | — | — | — | — |
| RD-R38-009 | READER-B-03、READER-C-04 | 3.4.1 | R38 | reader-b/c | P2 | 文档/死指针 | ① | §3.4.1 的 `2.6` 改 `§2.5`，`kern_phys_start/kern_vir_start` 改 `kernbase/kernsize` | 三份面板同指一处；§2.6 无这些名字 | CLOSED；panel/round38/reader-b.md:54；panel/round38/reader-c.md:45；`rg 'kern_phys_start|2.6 里' doc` 归零 | — | — | — | — |
| RD-R38-009a | READER-A-03 | 3.4.1 | R38 | reader-a | P3 | 文档/死指针 | SUGG | 同 RD-R38-009：§3.4.1 的 §2.6 死指针已修；按读者 P3 记 SUGG 并清零 | panel/round38/reader-a.md:18 | SUGG 已清零；`rg 'kern_phys_start|2.6 里' doc` 归零 | — | — | — | — |
| RD-R38-010 | READER-C-05 | 3.5.1、4.3 | R38 | reader-c | P2 | 文档/因果链 | ① | §4.3 与 `opensbi_helpers.rs` 注释改为“首次修剪 + 幂等兜底” | reader-c 对读 §3.5.1/§4.3 | CLOSED；panel/round38/reader-c.md:58；`rg '唯一修剪点' doc os/boot-shim/src/opensbi_helpers.rs` 归零 | — | — | — | — |
| RD-R38-011 | READER-C-06 | 3.2.3、3.7 | R38 | reader-c | P2 | 文档/所有权 | ① | 限定 BootStorage 为“走 fill_* 的腿”的六类载荷所有者，UEFI source 直写 wire | reader-c 对读 §3.2.3/§3.7/§4.2 | CLOSED；panel/round38/reader-c.md:71；`rg '六类载荷的唯一所有者' doc` 归零 | — | — | — | — |
| RD-R38-012 | REFUTE-06 | 4.5、6.1 | R38 | refute | P2 | 文档/容量+诊断 | ① | 名字池区分 store_kernel_info 15B 与 v1 接收 16B；§6.1 补 Side 诊断行 | refute 核对 `BOOT_MODULE_NAME_LEN`、`lib.rs:684-690`、`ebs_pair.rs:216-219` | CLOSED；panel/round38/refute.md:47；`rg 'memory map left|每槽.*15 字节' doc` 命中 | — | — | — | — |
| RD-R38-013 | READER-A-02 | 3.0、3.7 | R38 | reader-a | P3 | 文档/计数 | SUGG | 统一为“六类载荷（五个 fill_* 入口；fill_map_tables 一次提交两表）” | reader-a 指出六类只列五个函数 | SUGG 已清零；`rg '六类 `fill_\*` 的唯一' doc` 归零 | — | — | — | — |
| RD-R38-015 | DESIGN-01 | 3.2.3、4.2 | R38 | design-d2 | P2 | 设计/双写 | ⑥ | [DEL] 分类器只落 storage，wire 由 report 一次编码；待设计批 | `design-d2.md` DESIGN-01 | 未修；STILL-BROKEN；panel/round38/design-d2.md:36 | — | — | — | — |
| RD-R38-016 | DESIGN-02 | 4.1 | R38 | design-d2 | P2 | 设计/删接口 | ⑥ | [DEL] 删 `PreparedJump` 与 `prepare/jump` 两个公开壳；待设计批 | `design-d2.md` DESIGN-02 | 未修；STILL-BROKEN；panel/round38/design-d2.md:44 | — | — | — | — |
| RD-R38-017 | DESIGN-03 | 4.4 | R38 | design-d2 | P2 | 设计/双落库 | ⑥ | [MODERN] v1 接收与直核共用单一 `publish_landed`；待设计批 | `design-d2.md` DESIGN-03 | 未修；STILL-BROKEN；panel/round38/design-d2.md:52 | — | — | — | — |
| RD-R38-018 | DESIGN-04 | 4.2 | R38 | design-d2 | P2 | 设计/全局态 | ⑥ | [DEL] `resv_claims` 栈上 `ClaimSet` 替代 5 个静态；待设计批 | `design-d2.md` DESIGN-04 | 未修；STILL-BROKEN；panel/round38/design-d2.md:60 | — | — | — | — |
| RD-R38-019 | DESIGN-05 | 4.4 | R38 | design-d2 | P2 | 设计/值对象 | ⑥ | [DEL] 删 `BootScalars` 与两个构造器；待设计批 | `design-d2.md` DESIGN-05 | 未修；STILL-BROKEN；panel/round38/design-d2.md:68 | — | — | — | — |
| RD-R38-020 | DESIGN-06 | 3.2 | R38 | design-d2 | P2 | 设计/旧名 | ⑥ | [DEL] `requires_startup_kernel_record` 改名并删 `validate_at` 转发；待设计批 | `design-d2.md` DESIGN-06 | 未修；STILL-BROKEN；panel/round38/design-d2.md:76 | — | — | — | — |
| RD-R38-021 | DESIGN-07 | 3.2 | R38 | design-d2 | P2 | 设计/区间类型 | ⑤ | [MODERN] `PhysRange` 统一三区间表示，动 wire 边界；需用户裁决 | `design-d2.md` DESIGN-07 | STILL-BROKEN；panel/round38/design-d2.md:84 | — | — | — | — |
| RD-R38-022 | DESIGN-08 | 3.2.1 | R38 | design-d2 | P2 | 设计/协议收窄 | ⑤ | [MODERN] `SourceRecord` 72B→审计 4 字段，动 wire v11；需用户裁决 | `design-d2.md` DESIGN-08 | STILL-BROKEN；panel/round38/design-d2.md:92 | — | — | — | — |
| RD-R38-023 | DESIGN-09 | 3.4.2 | R38 | design-d2 | P2 | 设计/静态态 | ⑥ | [MODERN] `pt_alloc` 六静态收为每槽一个 AtomicUsize；待设计批 | `design-d2.md` DESIGN-09 | 未修；STILL-BROKEN；panel/round38/design-d2.md:100 | — | — | — | — |
| RD-R38-024 | DESIGN-10 | 4.4 | R38 | design-d2 | P2 | 设计/公开签名 | ⑥ | [MODERN] `shim_windows` 改 `&[PhysRange]`；待设计批 | `design-d2.md` DESIGN-10 | 未修；STILL-BROKEN；panel/round38/design-d2.md:108 | — | — | — | — |
| RD-R38-025 | DESIGN-11 | 3.7 | R38 | design-d2 | P2 | 文档/锚点 | ① | `globals.rs` 写者改 `resume_additive`，§3.7 改 `report_static` | design-d2 对读 `globals.rs:275` 与 §3.7 | CLOSED；panel/round38/design-d2.md:116；`rg 'prepare_kernel_image_v1.*v1 leg|BootStorageCell::take' os/kernel/src/globals.rs doc` 归零 | — | — | — | — |

**R38 未闭环**：10 条（RD-R38-015..024）。
