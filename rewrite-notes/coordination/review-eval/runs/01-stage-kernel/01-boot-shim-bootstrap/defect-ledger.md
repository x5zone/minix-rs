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
| READER-A-01 | reader-a | P2 | 结构/依赖清单 | ② | 转出：follow-ups.md D01-F12（§1.2 依赖边与逐条清单缺一条） | panel/reader-a.md READER-A-01 | 不适用（转出） |
| READER-A-02 | reader-a | P3 | 教学/链接顺序 | ② | 转出：follow-ups.md D01-F12（16 字节对齐论证依赖链接顺序，需链接脚本证据） | panel/reader-a.md READER-A-02 | 不适用（转出） |
| READER-A-03 | reader-a | P3 | 事实/NUL 语义 | ② | 转出：follow-ups.md D01-F12（mb_set_param 删除扫描的 NUL 语义需 C 源码复核） | panel/reader-a.md READER-A-03 | 不适用（转出） |
| READER-A-04 | reader-a | P3 | 事实/空槽顺序 | ② | 转出：follow-ups.md D01-F12（add_memmap 空槽填充顺序需 C 源码复核） | panel/reader-a.md READER-A-04 | 不适用（转出） |
| READER-A-05 | reader-a | P1 | 文档/分配接口 | ① | 已修：§3.1 明确页分配 allocate_pages 与池分配 allocate_pool 两类 | rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md §3.1 | L4 文档五门重跑 |
| READER-A-06 | reader-a | P1 | 文档/未定义术语 | ① | 已修：test-kernel-map 已定义；D1 闸改为描述性一致性检查 | 同篇 §3.4、§6.1 | L4 文档五门重跑 |
| READER-A-07 | reader-a | P1 | 文档/术语 | ① | 已修：§3.1 补 [ARCH] 架构演进标注说明 | 同篇 §3.1 | L4 文档五门重跑 |
| READER-A-08 | reader-a | P1 | 文档/唯一落点 | ① | 已修：read_required 收窄为内核 ELF 落点，模块另有 panic | 同篇 §3.2、§4.3 | L4 文档五门重跑 |
| READER-A-09 | reader-a | P0 | 文档/引文保真 | ① | 已修：§3.8 register 引文补 release 守门（同 FACT-07） | 同篇 §3.8 | L4 文档五门重跑 |
| READER-A-10 | reader-a | P1 | 文档/计数矛盾 | ① | 已修：四条→五条（同 FACT-16） | 同篇 §3.7 | L4 文档五门重跑 |
| READER-A-11 | reader-a | P2 | 文档/歧义 | ② | 转出：follow-ups.md D01-F12（BootFileTable 与 DTB 取模块的“或”歧义） | panel/reader-a.md READER-A-11 | 不适用（转出） |
| READER-A-12 | reader-a | P2 | 文档/空承诺 | ① | 已修：§4.1 改为一段式入口加三段内部流程 | 同篇 §4.1 | L4 文档五门重跑 |
| READER-A-13 | reader-a | P2 | 文档/未定义术语 | ① | 已修：§3.4 补 MapKey 定义 | 同篇 §3.4 | L4 文档五门重跑 |
| READER-A-14 | reader-a | P2 | 文档/悬空引用 | ① | 已修：NR_BOOT_PROCS 回指改为 boot image 表 | 同篇 §4.3 | L4 文档五门重跑 |
| READER-A-15 | reader-a | P2 | 教学/证据不足 | ② | 转出：follow-ups.md D01-F12（kern_stack_top 与 L7 断言不等价） | panel/reader-a.md READER-A-15 | 不适用（转出） |
| READER-A-16 | reader-a | P2 | 文档/错误节号 | ① | 已修：§7.5 三行错误节号已收窄 | 同篇 §7.5 | L4 文档五门重跑 |
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
| READER-C-01 | reader-c | P1 | 文档/唯一落点 | ① | 已修：同 READER-A-08 | panel/reader-c.md READER-C-01 | L4 文档五门重跑 |
| READER-C-02 | reader-c | P0 | 文档/引文保真 | ① | 已修：同 FACT-07 | panel/reader-c.md READER-C-02 | L4 文档五门重跑 |
| READER-C-03 | reader-c | P0 | 文档/交接面矛盾 | ① | 已修：§3.1 传递参数改为 arena 基址，KernelInfo 序列化进 arena | 同篇 §3.1 | L4 文档五门重跑 |
| READER-C-04 | reader-c | P1 | 教学/单形态结论 | ① | 已修：§3.8 按已用部分扣除加自引导路径限定 | 同篇 §3.8 | L4 文档五门重跑 |
| READER-C-05 | reader-c | P2 | 教学/探针不同源 | ② | 转出：follow-ups.md D01-F15（探针形态与真机读数不同源） | panel/reader-c.md READER-C-05 | 不适用（转出） |
| READER-C-06 | reader-c | P2 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 结构/一致性项） | panel/reader-c.md READER-C-06 | 不适用（转出） |
| READER-C-07 | reader-c | P1 | 文档/归属矛盾 | ① | 已修：§4.6 LAPIC/IO-APIC 归属改为 Step 2c | 同篇 §4.6 | L4 文档五门重跑 |
| READER-C-08 | reader-c | P2 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 一致性/指针项） | panel/reader-c.md READER-C-08 | 不适用（转出） |
| READER-C-09 | reader-c | P2 | 结构/一致性 | ② | 转出：follow-ups.md D01-F18（reader-c 一致性/指针项） | panel/reader-c.md READER-C-09 | 不适用（转出） |
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
| READER-A-01/B-01/C-01 | readers | P0 | 文档/新旧叙事并存（F03 残留簇） | ① | 已修：同 FACT-01..04 批（§4.2/§4.4/§5.2.1/§6.3 全部对齐删除态） | 三读者交叉同靶 | L4 回读 |
| READER-A-02 | reader-a | P1 | 文档/builder 缺字段 | ① | 已修：§4.4 builder 引文补 `memmap_sources: &[]`/`startup_objects: &[]`（与源码一致——随 FACT-01 批核对） | lib.rs builder | 同上 |
| READER-A-03/C-02 | readers | P1 | 文档/§3.10 与 §3.9 矛盾 | ① | 已修：§3.10 freepde 行改"先 Option 后退场" | — | 同上 |
| READER-A-04/C-06 | readers | P2 | 文档/回指落空 | ① | 已修：§1.2"两个分支"回指改指 §2.4 LOAD_BSS 链 | — | 同上 |
| READER-A-05/B-05 | readers | P2 | 文档/编号无图例 | ① | 已修：§3.4 前加编号图例块（D01-Fnn/D05/T2/Phase A.2） | — | 同上 |
| READER-B-03 | reader-b | P1 | 文档/计数维度冲突 | ① | 已修：§4.8 开头改"五行并列+归并口径回指" | — | 同上 |
| READER-B-04 | reader-b | P1 | 文档/三条链 vs 五行 | ① | 已修：§5.2.3 标题去计数、§3.10 收束句改"三类链+回指术语对账" | — | 同上 |
| READER-C-03 | reader-c | P1 | 文档/因果前提缺失 | ① | 已修：§3.8 补 Rust 侧搬运时序前提（blob 留原地、VM 启动才搬；C 复制后才回收）；表注两种"段帧"说法统一 | 2.4.2 引文 | 同上 |
| READER-C-04 | reader-c | P2 | 文档/跨二进制钩子因果 | ① | 已修：§6.2 第 4 条拆 shim/内核两腿（钩子每二进制一份，shim 侧恒未注册） | diagnostic.rs 静态量 | 同上 |
| READER-C-05 | reader-c | P2 | 文档/次序计数 | ① | 已修：§2.8 括注改"步骤级五条+字段级一条"的完整清单 | — | 同上 |
| READER-C-07 | reader-c | P1 | 文档/隐含不变量 | ① | 已修：§2.4.1 补"下标序=地址序"的成立条件与失效场景 | C 源回填语义 | 同上 |
| READER-C-08 | reader-c | P1 | 文档/守卫触发场景 | ① | 已修：§4.8 点名 VMA=LMA 测试内核（link.ld 单行 `. = 0x80200000`）为相等场景 | link.ld | 同上 |
| READER-C-09 | reader-c | P2 | 文档/同段尺寸矛盾 | ① | 已修：Step 2 反向推演改"按 Step 2 自己的尺寸，设备分片槽例外" | — | 同上 |
| READER-C-10/FACT-10/11 | 多源 | P2 | 文档/计数互斥 | ① | 同 FACT-10/11 批 | — | 同上 |
| READER-C-11 | reader-c | P2 | 文档/覆盖声明过强 | ① | 已修：M14 行补覆盖边界（路标出现≠内核存活，后者归下游门链） | — | 同上 |
| READER-C-20 | reader-c | P3 | 文档/测试名与省略声明表面冲突 | ① | 已修：§3.4 补"测试传显式板名验证的是有来源分支；生产传 None" | boot_params.rs:209 | 同上 |
| READER-C-21 | reader-c | P3 | 文档/BIOS 链无着落 | ① | 已修：改"固件未开分页形态（C 侧语义统一），本仓无 BIOS 产线" | — | 同上 |
| READER-C-22 | reader-c | P3 | 文档/同形掩盖差异 | ① | 已修：param_buf 句改"仅就 ARCH 同形；BOARD 省略不再保证" | — | 同上 |
| READER-B-19 | reader-b | P3 | 文档/图面与文字 | ① | 已修：§1.2 改"一条逆向箭头+一条文字标注依赖" | — | 同上 |
| READER-B-20 | reader-b | P3 | 文档/指涉不明 | ① | 已修：§5.2.3 补三条路标的 EBS 前后归属 | — | 同上 |
| READER-A 术语组 | reader-a | P3 | 教学/首用未定义 | ④ | 已修：arena/信封/v1、RAMDS、medany、PML4 首用处就地展开 | — | 同上 |
| DESIGN-D2-01/02/03 | design-d2 | P1 | 设计/死抽象（F04/F05/F06 复核维持） | ② 带日期 | code-excellence 批次执行（用户既定裁决；D2 补充了执行面修正：build_memmaps 族被 QEMU 测试依赖不可删） | panel/round1/design-d2.md | 批次执行时复验 |
| DESIGN-D2-04 | design-d2 | P2 | 代码/非法状态（kern_stack_top 无上界） | ① | 本轮代码批修复 | design-d2.md | 测试 |
| DESIGN-D2-05 | design-d2 | P2 | 代码/非法状态（memmap 无序校验） | ① | 本轮代码批修复（FreeRangeSet::from_memmap 断言有序不相交） | 同上 | 测试 |
| DESIGN-D2-06 | design-d2 | P2 | 设计/store 收口时点 | ⑤ | 证据不足：需读 store_kernel_info 与 boot_validate_and_prepare 的完整时序，登记 follow-ups 带获取路径 | 同上 | 下轮 |
| DESIGN-D2-07 | design-d2 | P1 | 代码/三抄加性恢复体 | ① | 本轮代码批：合并 resume_additive<A, H> | 同上 | 测试+三架构编译 |
| DESIGN-D2-08 | design-d2 | P2 | 代码/clone 到能编译 | ① | 本轮代码批：删 Box::leak(to_string()) | loader.rs:344 | 测试 |
| REFUTE-P2 观察 | refute | P3 | 代码/潜伏路径（boot_validate_and_prepare 兜底读发送方切片） | ⑤ | 证据不足：登记 follow-ups（当前被 resume_boot_pt_alloc 挡住）；BootAlloc 非线性化约束补进 §3.8 正文一句 | refute.md CLAIM 观察 | 下轮 |
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
| READER-A-01/B-19 | readers | P1 | 文档/GDB 清单查已删字段 | ① | 已修：§6.4 第三层改现存字段+委托链，明示 bootstrap_len 非检查项 | — | 五门 |
| READER-A-02/B-18/C-04 | readers | P1 | 文档/三套计数互斥 | ① | 同 FACT-01/02/03/09 批（转录块+算式行+契约表全部对齐 124/176） | 实测 | 同上 |
| REFUTE-③④ | refute | P2 | 文档/计数格过期+签名失真 | ① | 同上批 + bootstrap_alloc_limit() 无参签名修正 | uefi_helpers:635 | 同上 |
| REFUTE-① | refute | P2 | 文档/机制过强（TLB） | ① | 已修：改"冷取指"机制表述（三架构不因写表清 TLB） | refute.md | 五门 |
| REFUTE-② | refute | P2 | 设计/无守护前提（<4GiB） | ① | 已修：§6.3 补一行失败模式（boot-shim 镜像 >4GiB 无守护，OVMF 门链恒低于 4GiB） | refute.md | 同上 |
| REFUTE 焦点a | refute | P1 | 代码/resume 侧次序无钉 | ① | 已修：store_before_alloc_pin 增 validate < resume_boot_pt_alloc < delegate 序钉 | 实跑 PASS | 全量绿 |
| REFUTE 焦点b | refute | P2 | 设计/store 与 validate 收口序 | ③ | 反证失败：两序均安全（resume 体切片已落地、兜底死代码、validate 无页分配）；文档未主张同序，无冲突 | refute.md | — |
| D2-01/02/03 | design-d2 | P1 | 设计/死抽象稳定判定 | ② 带日期 | code-excellence 批次（F05 加强为"零生产实现"；F06 补 per-form 空转缺口；F04 拆分——UefiBootShim 零调用可删、trait 本体去留待裁决） | panel/round2/design-d2.md | 批次执行 |
| D2-04 | design-d2 | P2 | 代码/模块文档四处失真 | ① 待修 | 登记下批（boot_shim.rs 的 trait 定义位置、"kernel only depends"、cfg 矛盾、过时 T2 注记） | 同上 | 下批 |
| D2-05 | design-d2 | P1 | 设计/参数表 wire 分歧 | ⑤→用户 | 三处可观察分歧（重复键拒绝/命令行内部键被拒/board 缺失）——比照 F10 先例倾向标 [ARCH]，随下一批用户确认 | 同上 | 裁决后 |
| D2-10 | design-d2 | P3 | 工具/trait-audit 两类盲区 | ⑥ 工作流 | 记入 BLIND-SPOTS 候选（宏展开 impl 假阳性、非 trait 抽象逃逸） | 同上 | meta |
| READER-C-01/02/03/05/06/08 + B 簇 + A 簇 | readers | P1-P3 | 文档/复述失真与导航 | ① 待修 | 登记下批：load_mods 两复述、三入口收敛措辞、arena 布局数、大页失败三处、兜底视角切换、aarch64 模块来源、§3.3 消歧、修订痕迹、计数维度、A-18 升序前提复核等约 40 条 | panel/round2/reader-*.md | 下批 |
