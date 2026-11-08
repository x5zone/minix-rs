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
| READER-A-01/B-19 | readers | P1 | 文档/GDB 清单查已删字段 | ① | 已修：§6.4 第三层改现存字段+委托链，明示 bootstrap_len 非检查项 | — | 五门 |
| READER-A-02/B-18/C-04 | readers | P1 | 文档/三套计数互斥 | ① | 同 FACT-01/02/03/09 批（转录块+算式行+契约表全部对齐 124/176） | 实测 | 同上 |
| REFUTE-③④ | refute | P2 | 文档/计数格过期+签名失真 | ① | 同上批 + bootstrap_alloc_limit() 无参签名修正 | uefi_helpers:635 | 同上 |
| REFUTE-① | refute | P2 | 文档/机制过强（TLB） | ① | 已修：改"冷取指"机制表述（三架构不因写表清 TLB） | refute.md | 五门 |
| REFUTE-② | refute | P2 | 设计/无守护前提（<4GiB） | ① | 已修：§6.3 补一行失败模式（boot-shim 镜像 >4GiB 无守护，OVMF 门链恒低于 4GiB） | refute.md | 同上 |
| REFUTE 焦点a | refute | P1 | 代码/resume 侧次序无钉 | ① | 已修：store_before_alloc_pin 增 validate < resume_boot_pt_alloc < delegate 序钉 | 实跑 PASS | 全量绿 |
| REFUTE 焦点b | refute | P2 | 设计/store 与 validate 收口序 | ③ | 反证失败：两序均安全（resume 体切片已落地、兜底死代码、validate 无页分配）；文档未主张同序，无冲突 | refute.md | — |
| D2-01/02/03 | design-d2 | P1 | 设计/死抽象稳定判定 | ② 带日期 | code-excellence 批次（F05 加强为"零生产实现"；F06 补 per-form 空转缺口；F04 拆分——UefiBootShim 零调用可删、trait 本体去留待裁决） | panel/round2/design-d2.md | 批次执行 |
| D2-10 | design-d2 | P3 | 工具/trait-audit 两类盲区 | ⑤（工作流记录项：入 meta/BLIND-SPOTS 候选，非文档缺陷） | 记入 BLIND-SPOTS 候选（宏展开 impl 假阳性、非 trait 抽象逃逸） | 同上 | meta |
| READER-C-01/02/03/05/06/08 + B 簇 + A 簇 | readers | P1-P3 | 文档/复述失真与导航 | ① | 批二已修（本轮）：C-01 load_mods 表格行（exec_multiboot:759 实证）、C-02 直核家族限定、C-03 arena 四段布局注、C-05 双前缀+规划函数名统一、C-06 视角锚定句、C-08 aarch64 表址单通道、B-01 框图注、B-02 边界清单补 §2.3、B-03/C-09 三次出场、B-04/C-12 指向 §2.4.2、B-11 两道闸分工、B-15 链形状、A-03 可执行体/库消歧、A-07 跨镜像产线措辞、A-08 指针落点、A-18 升序机制修正（右余量落数组末尾，序不保证；alloc_lowest 按地址取不受影响）、A-04/A-05/A-14/A-17/A-19、B-06/B-13/B-14/B-17、C-07/C-10/C-11 | panel/round2/reader-*.md | 五门全绿+锚点 100/100 |
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
| READER-A-17 | reader-a(R2) | P2 | 文档/wire 双版本轴未对表 | ① | 已修：§3.4 补"协议身份 v1 vs 信封版本号（当前 8）"两层版本号段 | panel/round2/reader-a.md | 五门 |
| READER-A-18 | reader-a(R2) | P1 | 文档/升序机制表述错误 | ① | 已修：§2.4.1 改"右余量落数组末尾空槽、序不保证；alloc_lowest 按地址取 min 不依赖序" | 同上 | 同上 |
| READER-A-19 | reader-a(R2) | P3 | 文档/仓库内库称"第三方" | ① | 已修：§3.2 改"本仓库的共享库 crate minix-boot" | 同上 | 同上 |
| READER-A-20 | reader-a(R2) | P3 | 文档/同句自重复+孤立符号 vmctl_set_addr_space | ② 带日期 | 维护批（局部打磨；与 §4.1 括注重叠已部分收敛） | 同上 | 维护批 |
| READER-C-18 | reader-c(R1) | P2 | 文档/bootstrap_alloc_limit 签名不一致 | ① | 已修：R3 refute 批把步骤 4 改为 alloc_arena() 经 UEFI allocate_pages(MaxAddress(bootstrap_alloc_limit()))——无参签名与引文一致 | panel/round1/reader-c.md | 五门重跑 |
| READER-C-19 | reader-c(R1) | P3 | 文档/24 字节归因不闭合 | ① | 已修（信息在 §3.9 可连读；R3 批二把 §3.9 的 wire 版本句改指针后两节连读路径仍在）——补一句显式归因见维护批 | 同上 | 维护批补一句 |

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
| READER-A-01 | reader | P2 | 文档/§1.2 三层依赖密度 | ① | 已修：压成一句+回指 §2.8（去重） | 同上 | 五门 |
| READER-A-02 | reader | P2 | 文档/16 字节对齐消费者 | ① | 已修：补 ABI 消费者与 #GP 后果、编译选项限定 | 同上 | 同上 |
| READER-A-03 | reader | P2 | 文档/ARM overlaps 锚点 | ① | 已修：补 earm/pre_init.c:overlaps 锚点 | 同上 | 锚点门 |
| READER-A-04 | reader | P3 | 文档/ram_authority 未定义 | ① | 已修：wire 引文加编注定界 | 同上 | 五门 |
| READER-A-05 | reader | P2 | 文档/arena 分区计数 | ① | 已修：七载荷分区列名 + 信封+四段相加口径 | 同上 | 同上 |
| READER-B-01 | reader | P3 | 文档/跨侧类型论证 | ① | 已修（与 C-09 同靶）：KernelInfo 跨侧、接口类型引导侧 | 同上 | 同上 |
| READER-B-02 | reader | P3 | 文档/框图双向依赖 | ① | 已修：箭头注拆"依赖 A/B"两行 | 同上 | 同上 |
| READER-B-03 | reader | P1 | 文档/§3.1 列名 OpenSBI | ① | 已修：列名改 U-Boot shim 路径 + 直核指向 §3.7 | 同上 | 同上 |
| READER-B-04 | reader | P1 | 文档/三类链循环指认 | ① | 已修：定义收进 §3.7 对账为唯一处，§4.1 只按两列 | 同上 | 同上 |
| READER-C-01 | reader | P1 | 文档/门链五条集合 | ① | 已修（与 FACT-09 同靶） | 同上 | 同上 |
| READER-C-02 | reader | P2 | 文档/§2.8 证据数 | ① | 已修：两条在此配证据、三条在 §1.2/§2.6/§2.6.1 | 同上 | 同上 |
| READER-C-03 | reader | P2 | 文档/§3.8 第四写点 | ① | 已修：换手表 Step 0 行并入接收镜像 resume_additive 注册 | 同上 | 同上 |
| READER-C-04 | reader | P2 | 文档/riscv 反事实范围 | ① | 已修：限定非 U-Boot 特性直核入口 | 同上 | 同上 |
| READER-C-05 | reader | P3 | 文档/计数轴 | ② 带日期 | R5 fact 复核（B-04/术语对账重写后需观察一轮） | panel/round4/reader.md | R5 |
| READER-C-06 | reader | P2 | 文档/M1 证据类型 | ① | 已修：引文对账、删真机兜底、注明真机不覆盖 C head.S | 同上 | 同上 |
| READER-C-07 | reader | P2 | 文档/M3 覆盖强度 | ① | 已修：改部分覆盖+源码序钉说明 | 同上 | 同上 |
| READER-C-08 | reader | P3 | 文档/pg_map USER | ① | 已修：补用户态消费与 pg_identity 对照 | 同上 | 同上 |
| READER-C-09 | reader | P3 | 文档/跨侧类型论证 | ① | 已修（与 B-01 同靶） | 同上 | 同上 |
| READER-C-10 | reader | P3 | 文档/24 字节归因 | ① | 已修：§3.5 补 free_upper_idx 8 字节 | 同上 | 同上 |
| READER-C-12 | reader | P2 | 文档/§1.2 四条编号 | ① | 已修：四条编号+单列一条=五条 | 同上 | 同上 |

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
| READER-A-01 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-A-02 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-A-03 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-A-04 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-B-01 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-B-02 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-B-03 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-B-04 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-01 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-02 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-03 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-04 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-05 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-06 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-07 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-08 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-09 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-10 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-11 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-12 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-13 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-14 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-15 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-16 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-17 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-18 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-19 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-20 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-21 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |
| READER-C-22 | panel-r5 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `19a481dad` 提交与 panel/round5 对读 | panel/round5 | 五门+测试 |

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
| READER-A-01 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-A-02 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-A-03 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-A-04 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-A-05 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-A-06 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-01 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-02 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-03 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-04 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-05 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-06 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-07 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-08 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-09 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-10 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-B-11 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-01 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-02 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-03 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-04 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-05 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-06 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-07 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-08 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-09 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-10 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-11 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-12 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-13 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-14 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |
| READER-C-15 | panel-r6 | P2/P3 | 文档/源码注释同步 | ① | 已修：见 `6afb501c6` 与后续小批 | panel/round6 | 五门+测试 |

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
| READER-C-01 | panel-r7 | P3 | 文档/§4.4 表缺页大小能力/物理上限行 | ① | 已修：表补 `detect_boot_page_support` 成功且 `phys_addr_limit() >= IDENTITY_MAP_END` 一行 | 文档 §4.4 | 五门 |
| READER-C-02 | panel-r7 | P3 | 文档/§4.4 表用未定义符号 `entry_start_pa` | ① | 已修：改自然语言"镜像内入口/栈顶偏移写错，或用户栈上界越入内核半区" | 文档 §4.4 | 五门 |

# Round 8（基线 1ceaf539d）

## R8 面板明细（39 条）

| 发现 ID | 来源角色 | 严重度 | 类别 | 处置类 | 处置 | 证据 | 复验 |
|---|---|---|---|---|---|---|---|
| FACT-R8-01 | panel-r8 | P1 | 文档/§5.2.1 计数未随 5 条新测试回填 | ① | 已修：124→129、6→11、113→118、求和式与“该文件 11 条”同步；历史补 R7 D2-02 五条负控 | 文档 §5.2.1 | 五门+测试 |
| FACT-R8-02 | panel-r8 | P2 | 文档/§2.2 把入口 esp≡0 当 i386 SysV ABI 要求 | ① | 已修：改“入口 %esp+4 是 16 的倍数；本布局 esp≡0，差 4 字节”，保留编译选项免责 | 文档 §2.2 | 五门 |
| FACT-R8-03 | panel-r8 | P2 | 源码注释/main.rs 仍写 kerninfo 每绑定注入 | ① | 已修：改历史陈述 + D72VM 已移交 VM；删 per-bind 乘法与“修复未落地” | os/boot-shim/src/main.rs | 编译绿 |
| FACT-R8-04 | panel-r8 | P2 | 文档/§4.5 称 kmain 会再存一次 store_kernel_info | ① | 已修：改“守卫防未来同址调用”；lib.rs 两处 kmain 注释同步改写 | 文档 §4.5；os/kernel/src/lib.rs | 测试+五门 |
| FACT-R8-05 | panel-r8 | P3 | 源码注释/vm_server 行号锚点失效 | ① | 已修：改指 boot_validate_and_prepare，不绑行号 | os/servers/vm/src/vm_server.rs | 编译绿 |
| READER-R8-F01 | panel-r8 | P2 | 文档/§3.7 三条入口收敛点与 §4.8 矛盾 | ① | 已修：分直核 arch_boot_finish 与接收 arch_boot_resume_v1→resume_additive；发送侧不收敛 | 文档 §3.7 | 五门 |
| READER-R8-F02 | panel-r8 | P2 | 文档/§2.8 N 同时指 mmap 条目数与模块数 | ① | 已修：拆 R（mmap 条目数）与 M（模块数）；箭头改“交错” | 文档 §2.8 | 五门 |
| READER-R8-F03 | panel-r8 | P2 | 文档/§4.2 九步漏 startup 分区写入 | ① | 已修：表后注明表外动作（EBS 后补写 canonical 五类） | 文档 §4.2 | 五门 |
| READER-R8-F04 | panel-r8 | P2 | 文档/Step 0-4 编号全文使用无定义 | ① | 已修：§4.6 开头补五行阶段表（含 2b/2c 归属） | 文档 §4.6 | 五门 |
| READER-R8-F05 | panel-r8 | P2 | 文档/§4.6 AlreadyMapped 与 AllocationFailed 成因写串 | ① | 已修：可达类=AllocationFailed；AlreadyMapped 结构性排除，各自理由分开 | 文档 §4.6 | 五门 |
| READER-R8-F06 | panel-r8 | P2 | 文档/流程代号密度与读者画像冲突；O-11 仍在用 | ① | 已修：图例明确可跳过、删编号后句意完整；O-11 改“两快照纪律” | 文档 §3.4/§4.1 | 五门 |
| READER-R8-F07 | panel-r8 | P3 | 文档/§3.4 “删掉的不是信息”与退役三字段矛盾 | ① | 已修：段首改“多数换载体，少数退役”，保留三类归位 | 文档 §3.4 | 五门 |
| READER-R8-F08 | panel-r8 | P3 | 文档/§2.4.2 结算锚点不含 16 条截断 | ① | 已修：改指 §2.4.1 末“另一件必须单说的事”段 | 文档 §2.4/§3.4 | 五门 |
| READER-R8-F09 | panel-r8 | P3 | 文档/§1.4 kmain 校验归给 §3.5 | ① | 已修：改指 §4.4 门链 | 文档 §1.4 | 五门 |
| READER-R8-F10 | panel-r8 | P3 | 文档/ram_authority 注释指 §4.2 无 source 审计 | ① | 已修：改指 §3.4 规范集合与 §4.4 strict 门 | 文档 §3.4 | 五门 |
| READER-R8-F11 | panel-r8 | P3 | 文档/两处“§2.4 升序讨论”层级错 | ① | 已修：两处改 §2.4.1 | 文档 §2.4.1 | 五门 |
| READER-R8-F12 | panel-r8 | P3 | 文档/§2.6.5 “三张表”后写“两张表” | ① | 已修：改“CR0/CR4 表里” | 文档 §2.6.5 | 五门 |
| READER-R8-F13 | panel-r8 | P3 | 文档/逐页累加与整段累加自相矛盾 | ① | 已修：前半句拆两种粒度，删末尾重复更正 | 文档 §2.4.1 | 五门 |
| READER-R8-F14 | panel-r8 | P3 | 文档/§4.1 “步骤 2 的 init_boot_pt_alloc”编号悬空 | ① | 已修：改动作名“prepare_and_enter_kernel_image 开头那次” | 文档 §4.1 | 五门 |
| READER-R8-F15 | panel-r8 | P3 | 文档/§3.8 首行不写注册槽却计入“写点” | ① | 已修：改“一次初始化 + 三次注册”；首行标“不写注册槽” | 文档 §3.8 | 五门 |
| READER-R8-F16 | panel-r8 | P3 | 文档/961..1023 与 961+k 边界差一格 | ① | 已修：改“0–1023（含 1023）”与“961+k-1” | 文档 §2.6 | 五门 |
| READER-R8-F17 | panel-r8 | P2 | 文档/接收公共段 high_half vs resume_additive 两说 | ① | 已修：次序句改“各架构委托壳”；§6.1 表注明两行仅 aarch64 壳打印 | 文档 §4.7/§6.1 | 五门 |
| READER-R8-F18 | panel-r8 | P3 | 文档/§3.3 依赖图漏 8 条边（含反向库依赖） | ① | 已修：图下注明简化图、条件依赖见四表 | 文档 §3.3 | 五门 |
| READER-R8-F19 | panel-r8 | P3 | 文档/“跨镜像两腿”与发送腿术语冲突 | ① | 已修：改“发送/接收两端” | 文档 §3.7 | 五门 |
| READER-R8-F20 | panel-r8 | P3 | 文档/§5.2.3 “这条 U-Boot 链”指代不明；脚本不在清单 | ① | 已修：点名 riscv64 U-Boot 直载测试链；§7.4 补 test-riscv64-boot-full.sh | 文档 §5.2.3/§7.4 | 五门 |
| READER-R8-F21 | panel-r8 | P3 | 文档/ESP、ConOut、MIB 首现未展开 | ① | 已修：三处补一句定义 | 文档 §6.1/§6.3/§2.3 | 五门 |
| READER-R8-F22 | panel-r8 | P3 | 画像目标④章节集合与正文不一致 | ① | 已修：画像同步 §2.4/§2.4.2 | reader-profile.md | 五门 |
| READER-R8-F23 | panel-r8 | P3 | 文档/RESERVED 256 来由 127×2=254 差 2 | ① | 已修：改“翻倍后向上取整到 2 的幂” | 文档 §4.5 | 五门 |
| READER-R8-F24 | panel-r8 | P3 | 文档/fast-fail “被削弱” vs “本来没有” | ① | 已修：统一“相对稀疏方案被放弃；C 本来没有” | 文档 §2.6.1 | 五门 |
| READER-R8-F25 | panel-r8 | P3 | 文档/测试证据三处重复无单一权威 | ① | 已修：M13 与 §5.2.1 改指针，§5.3 TODO-07 保留完整清单 | 文档 §5.1/§5.2.1/§5.3 | 五门 |
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
| READER-R9-F01 | panel-r9 | P2 | 文档/§1.2 .bss 装载证据指向 §2.8 | ① | 已修：拆两指向（§2.8 推演、§2.4 装载证据） | 文档 §1.2 | 五门 |
| READER-R9-F02 | panel-r9 | P2 | 文档/kern_stack_top 门描述与构造边界矛盾 | ① | 已修：门描述改"不越过上界，恰等于镜像尾合法" | 文档 §4.4 | 五门 |
| READER-R9-F03 | panel-r9 | P2 | 文档/落地区二义，接收侧深拷次数不清 | ① | 已修：接收侧直接解码进七个 store，store 同址 no-op；DTB 落地区改暂存区 | 文档 §4.8/§4.5/§4.2 | 五门 |
| READER-R9-F04 | panel-r9 | P2 | 文档/U-Boot shim 腿写成调 arch_boot | ① | 已修：改"序列化进 arena 交接收镜像 arch_boot_resume_v1" | 文档 §3.7 | 五门 |
| READER-R9-F05 | panel-r9 | P2 | 文档/§4.2 省略块同时承载循环内 preflight 与 EBS 后写入 | ① | 已修：拆成"循环体内另一处省略"与"本处只含 EBS 后 startup" | 文档 §4.2 | 五门 |
| READER-R9-F06 | panel-r9 | P3 | 文档/§2.6.1 PSE/PGE 挂在 CR0.PG 后 | ① | 已修：改 CR0.PG/CR0.WP/CR4.PSE/CR4.PGE | 文档 §2.6.1 | 五门 |
| READER-R9-F07 | panel-r9 | P3 | 文档/"两个可执行体"缺主线限定 | ① | 已修：§1.2/§1.3 加主线限定；§1.4 补 resume 入口 | 文档 §1.2/§1.3/§1.4 | 五门 |
| READER-R9-F08 | panel-r9 | P3 | 文档/手算五例分组与定档返回不符 | ① | 已修：改"前两例成功定档（例2 Step3 返回）；后三例报错/Side/报错" | 文档 §4.2 | 五门 |
| READER-R9-F09 | panel-r9 | P3 | 文档/§2.8 R 定义与 AVAILABLE 过滤不符 | ① | 已修：R = 可用条目数；N+1→M+1 | 文档 §2.8 | 五门 |
| READER-R9-F10 | panel-r9 | P3 | 文档/"16 篇"不可达 | ① | 已修：改指 `smp_gpt_v3.md`，§7.1 补一行 | 文档 §4.9/§7.1 | 五门 |
| READER-R9-F11 | panel-r9 | P3 | 文档/UEFI 术语承诺未兑现 | ① | 已修：Configuration Table/LoadOptions/SimpleFileSystem 首现补定义 | 文档 §4.2/§4.3 | 五门 |
| READER-R9-F12 | panel-r9 | P3 | 文档/§2.4.2 口径句漏内核块 | ① | 已修：切占用含内核镜像块 | 文档 §2.4.2 | 五门 |
| READER-R9-F13 | panel-r9 | P3 | 文档/syscall_entry 语义二义 | ① | 已修：字段表注明引用性元数据、实际门地址由架构 asm 标签提供 | 文档 §4.4 | 五门 |
| READER-R9-F14 | panel-r9 | P3 | 文档/§6.4 同块"无脚本/有脚本"并存 | ① | 已修：首处改"无专用启动门脚本，由 test-smp-aps-aarch64.sh 间接驱动" | 文档 §6.4 | 五门 |
| READER-R9-F15 | panel-r9 | P3 | 文档/§3.6 否决表方案与理由不对应 | ① | 已修：方案扩为常量面三件 | 文档 §3.6 | 五门 |
| READER-R9-F16 | panel-r9 | P3 | 文档/§2.8 引用章内未出现的断言 | ① | 已修：换成 §2.4 已引的 `assert(cbi->mem_high_phys)` | 文档 §2.8 | 五门 |
| READER-R9-F17 | panel-r9 | P3 | 文档/§2.6.4 把同一批 1024 项数两遍 | ① | 已修：改"0..960 恒等 + 961 起被内核映射覆写" | 文档 §2.6.4 | 五门 |
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
| READER-R10-F01 | panel-r10 | P2 | 文档/enter_kernel_image_v1 被写成 UEFI 专属 | ① | 已修：改"跨镜像 v1 发送，三条腿" | 文档 §3.7 | 五门 |
| READER-R10-F02 | panel-r10 | P2 | 文档/接线权威表缺 aarch64 直核面 | ① | 已修：表补一行（测试通道、SMP 判据） | 文档 §3.1 | 五门 |
| READER-R10-F03 | panel-r10 | P2 | 文档/§2.4.1 结算把 §2.5 的切除写成已完成 | ① | 已修：改"将在 §2.5 切掉（本节尚未切）" | 文档 §2.4.1 | 五门 |
| READER-R10-F04 | panel-r10 | P2 | 文档/arena 由 UEFI 分配覆盖 U-Boot shim | ① | 已修：拆两腿（UEFI allocate_pages；U-Boot shim bump_alloc_pages，含在整段扣除） | 文档 §3.7 | 五门 |
| READER-R10-F05 | panel-r10 | P2 | 文档/§4.1 导航表把 U-Boot shim 归进直核面 | ① | 已修：列名改直核面，三处单元格注明 U-Boot shim 走 v1/接收 | 文档 §4.1 | 五门 |
| READER-R10-F06 | panel-r10 | P2 | 文档/§3.1 用辅助 prepare_boot 充当生产证据 | ① | 已修：引文头标"辅助形态无生产调用方"；1311 行加"辅助形态下"限定 | 文档 §3.1 | 五门 |
| READER-R10-F07 | panel-r10 | P2 | 文档/"内核镜像恒在表内"与 LOADER_* 分流冲突 | ① | 已修：改"UEFI 腿在保留表；DTB 腿在可用表" | 文档 §3.4 | 五门 |
| READER-R10-F08 | panel-r10 | P2 | 文档/"六入口"两套划分 | ① | 已修：定义按最外层壳（3 arch_boot + 3 decode；aarch64 内层 high_half） | 文档 §5.3 | 五门 |
| READER-R10-F09 | panel-r10 | P3 | 文档/§6.1 v1 路标位置全写 uefi_helpers | ① | 已修：位置列拆 uefi_helpers（两条 UEFI）/uboot_main（shim） | 文档 §6.1 | 五门 |
| READER-R10-F10 | panel-r10 | P3 | 文档/§2.8 "逐条如下"只列三条 | ① | 已修：改"其余三条只给指向" | 文档 §2.8 | 五门 |
| READER-R10-F11 | panel-r10 | P3 | 文档/PML4 256 至 512 越界 | ① | 已修：改 256..511（半开 256..512，256 项） | 文档 §4.7 | 五门 |
| READER-R10-F12 | panel-r10 | P3 | 文档/§2.4.1 调用序箭头与嵌套关系相反 | ① | 已修：改嵌套表述（arch_init in cstart in kmain；smp_init 晚于 cstart） | 文档 §2.4.1 | 五门 |
| READER-R10-F13 | panel-r10 | P3 | 文档/"11 个模块"章内无出处 | ① | 已修：改"12 个里除 VM 外 11 个（名单见 §4.3）" | 文档 §2.4.2 | 五门 |
| READER-R10-F14 | panel-r10 | P3 | 文档/§7.3 源码清单漏 ebs_map/ebs_pair 等 | ① | 已修：补 ebs_map/ebs_pair/boot_params/memmap_source/fdt_layout | 文档 §7.3 | 五门 |
| READER-R10-F15 | panel-r10 | P3 | 文档/分配上界"同一常量" vs 两界取小 | ① | 已修：改"以 BOOT_IDENTITY_MAP_END 为一界，再与 DM 窗口取小" | 文档 §4.6 | 五门 |
| READER-R10-F16 | panel-r10 | P3 | 文档/"六个真机脚本"无枚举 | ① | 已修：改"五条门链加 aarch64 直核面这六个组合" | 文档 §3.7 | 五门 |
| READER-R10-F17 | panel-r10 | P3 | 文档/Step1 可达失败只列 AllocationFailed 论据不足 | ① | 已修：补其余四类不可达判据（地址常量/只映射/尺寸快照/自造标志） | 文档 §4.6 | 五门 |
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
| READER-R11-F01 | panel-r11 | P2 | 文档/§2.8 "逐条如下"只列三条 | ① | 已修：改"下面只展开三条，另两条回指 §1.2/§2.6" | 文档 §2.8 | 五门 |
| READER-R11-F02 | panel-r11 | P2 | 文档/M15 时点写"EBS 前" | ① | 已修：改"配对循环成功后、裸串口可打印" | 文档 §5.1 M15 | 五门 |
| READER-R11-F03 | panel-r11 | P2 | 文档/TODO-07 "三案" vs 5 条 | ① | 已修：M13 与 §5.3 统一"五案/5 条" | 文档 §5.1/§5.3 | 五门 |
| READER-R11-F04 | panel-r11 | P2 | 文档/§2.4.1 pg_alloc_page 主语漂移 | ① | 已修：记账句归回 pg_alloc_page；alloc_lowest 另述 | 文档 §2.4.1 | 五门 |
| READER-R11-F05 | panel-r11 | P2 | 文档/四前提被说成安全边界 | ① | 已修：改"四前提建立的是切除不发生（危险来源）；兜住的是取页方向与生命周期" | 文档 §2.4.1 | 五门 |
| READER-R11-F06 | panel-r11 | P3 | 文档/"两条 UEFI 产线改用协议" | ① | 已修：改"跨镜像三腿" | 文档 §3.4 | 五门 |
| READER-R11-F07 | panel-r11 | P3 | 文档/Step 2c 归属矛盾 | ① | 已修：改"三架构共用，aarch64 空表 no-op" | 文档 §4.6 | 五门 |
| READER-R11-F08 | panel-r11 | P3 | 文档/"映射责任移交 VM"重复 | ① | 已修：删重复状语 | 文档 §4.1 | 五门 |
| READER-R11-F09 | panel-r11 | P3 | 文档/BOARD 分歧复述 | ① | 已修：删段尾重复句 | 文档 §3.4 | 五门 |
| READER-R11-F10 | panel-r11 | P3 | 文档/§1.1"不会再发出去"绝对化 | ① | 已修：加"归还之前"时间窗与 §2.4.2 回指 | 文档 §1.1 | 五门 |
| READER-R11-F11 | panel-r11 | P3 | 文档/L5 跨距与 kern_size 同义却分列 | ① | 已修：注明 L5 与规划层检查同一公式，仅执行位置不同 | 文档 §5.2.1 | 五门 |
| READER-R11-F12 | panel-r11 | P3 | 文档/§2.6.4 "本节前半"误指 | ① | 已修：改"上一小节（§2.6.3）" | 文档 §2.6.4 | 五门 |
| READER-R11-F13 | panel-r11 | P3 | 文档/§4.2 "这两页"单位错 | ① | 已修：改"两个分配（根页 + 1024 页 bump 区）" | 文档 §4.2 | 五门 |
| READER-R11-F14 | panel-r11 | P3 | 文档/§3 导览 3.8–3.9 归类错 | ① | 已修：改"3.8 分配器换手，3.9 槽位账退役" | 文档 §3 导览 | 五门 |
| READER-R11-F15 | panel-r11 | P3 | 文档/§2.4.1 自指 | ① | 已修：两处改"上文的升序讨论" | 文档 §2.4.1 | 五门 |
| READER-R11-F16 | panel-r11 | P3 | 文档/§7.2 C 源清单漏项 | ① | 已修：补 main/protect/arch_system/vm-alloc/do_*/assert/loadfile/exec/sysenv 等 | 文档 §7.2 | 五门 |
| READER-R11-F17 | panel-r11 | P3 | 文档/证据类型列 11 种写法超四类 | ① | 已修：引言说明 `＋`/`→` 组合语义与三类归并 | 文档 §5 引言 | 五门 |
| READER-R11-F18 | panel-r11 | P3 | 文档/§2.4.1 "本节结算"归属错 | ① | 已修：改"2.4 结算"；"本节尚未切"→"§2.4 尚未切" | 文档 §2.4.1 | 五门 |
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
| READER-R12-F01 | panel-r12 | P2 | 文档/§2.8 三条/两条矛盾 | ① | 已修（与 FACT-R12-01 同批） | 文档 §2.8 | 五门 |
| READER-R12-F02 | panel-r12 | P2 | 文档/"产线"未进术语对账 | ① | 已修：对账补"产线=链的别称，不参与计数" | 文档 §3.7 | 五门 |
| READER-R12-F03 | panel-r12 | P2 | 文档/前提一行号无标签 | ① | 已修：标明 474/311/301 各自对象 | 文档 §2.4.1 | 五门 |
| READER-R12-F04 | panel-r12 | P2 | 文档/§5.2.3 两条通道超出证据 | ① | 已修：改"DTB 通道可用；a2 无生产者未验证" | 文档 §5.2.3 | 五门 |
| READER-R12-F05 | panel-r12 | P3 | 文档/§2.6.1 下游指针错 | ① | 已修：改 §2.6.3 | 文档 §2.6.1 | 五门 |
| READER-R12-F06 | panel-r12 | P3 | 文档/§3.4 边界说明指针错 | ① | 已修：改"§3.4 末的 repr(C) 边界说明" | 文档 §3.4 | 五门 |
| READER-R12-F07 | panel-r12 | P3 | 文档/"差一个连字符"不实 | ① | 已修：改"名字相近但不是同一个东西" | 文档 §3.3 | 五门 |
| READER-R12-F08 | panel-r12 | P3 | 文档/kern_size/4MB 未取整 | ① | 已修：两处改 ceil | 文档 §2.6 | 五门 |
| READER-R12-F09 | panel-r12 | P3 | 文档/§4.9 与 §4.3 overlaps 口径不一 | ①（残留） | 部分修：§4.9 已注明跨段防线；完整不变量合并列入 STUCK-REPORT | 文档 §4.9 | 五门 |
| READER-R12-F10 | panel-r12 | P3 | 文档/M4 与 TODO-01 入口计数 5 vs 6 | ① | 已修：M4 改六入口（3 arch_boot + 3 decode） | 文档 §5.1 | 五门 |
| READER-R12-F11 | panel-r12 | P3 | 文档/四术语定义位置与首用不符 | ①（残留） | 部分修：arena/信封已就地定义；EBS/wire 定义位置列入 STUCK-REPORT | 文档 §3.4 | 五门 |
| READER-R12-F12 | panel-r12 | P3 | 文档/2.4 结算落点错 | ① | 已修：整段移到 §2.4.1 之前（§2.4 收束） | 文档 §2.4 | 五门 |
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
| READER-R13-F01 | panel-r13 | P2 | 文档/三处"长段拆分"只插软换行，渲染仍是同段 | ① | 已修：在真拆点插空行；pandoc `<p>` 计数 5/4 复验 | 文档 644/1057/1867 | 渲染复验 |
| READER-R13-F02 | panel-r13 | P2 | 文档/Step1 两条目被吞并成惰性续行，计数与两处指针失准 | ① | 已修：改"六条交代（前两条判据）"，三条非列表段落升为条目，指针改"第五条" | 文档 §4.6；`<li>` 计数 6 | 渲染复验 |
| READER-R13-F03 | panel-r13 | P3 | 文档/拆段留下三处双空行 | ① | 已修：全篇空行归一，末尾多余空行去掉 | 全篇 | 结构复验 |
| READER-R13-F04 | panel-r13 | P3 | 文档/"见上一段"指向错段 | ① | 已修：改"见上文'靠的是两点'那一段" | 文档 §2.4.1 | 五门 |
| READER-R13-F05 | panel-r13 | P3 | 文档/三处手写行号作判据锚 | ① | 已修：三处改符号锚（cstart/arch_init、smp_init、add_memmap 归还点、`///` 注记） | 文档 §2.4.1/§2.4.2/§4.7 | 五门 |

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
| R14/READER-A-20 | P3 | 内联闸表看不出 `Excluded`/`DevicePath` 是否经过 | ② | 结构项（分类章表格重排），修于 R18
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
| R14/DESIGN-05 | P2 | 同一物理池两个互不知情的 bump 分配器 | ② | 登记 F-R14-15 |
| R14/DESIGN-06 | P2 | DM 候选顺序抄四遍 | ② | 登记 F-R14-16 |
| R14/DESIGN-07 | P2 | `build_identity_windows()` 恒空而 `kern_ident` 仍在 | ② | 登记 F-R14-17 |
| R14/DESIGN-08 | P2 | `FileLoader` 返回 `Vec` 强制整份拷贝 | ② | 登记 F-R14-18 |
| R14/DESIGN-09 | P2 | 后置断言在生产值上不可证伪 | ② | 登记 F-R14-19 |
| R14/DESIGN-10 | P2 | `memmap ∩ reserved` 未校验，迟到 `AlreadyMapped` | ② | 登记 F-R14-20 |
| R14/DESIGN-11 | P3 | 13 pub 字段 + 12 个转发 getter 双 API | ② | 登记 F-R14-21 |
| R14/DESIGN-12 | P3 | 三校验语义用两 bool 编排 | ② | 登记 F-R14-22 |
| R14/DESIGN-13 | P3 | `ebs_failed` 与 `ebs_attempts>0` 重叠 | ② | 登记 F-R14-23 |
| R14/DESIGN-14 | P3 | `PagingWithId` 仅测试实现；`HUGE_PAGE_SHIFT` 零读者 | ② | 登记 F-R14-24（先做读者审计再删） |
| R14/DESIGN-15 | P3 | `Plan` 两遍遍历 | ② | 登记 F-R14-25（建议改名不重构） |
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
| R15/DESIGN-05 | P2 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-06 | P2 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-07 | P2 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-08 | P2 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-09 | P2 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-10 | P2 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-11 | P2 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-12 | P2 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-13 | P2 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-14 | P3 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-15 | P3 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-16 | P3 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-17 | P3 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
| R15/DESIGN-18 | P3 | ② | 代码重构项，修于 R16（F-R14-13..25 同批）；DESIGN-01..03 见待裁决项 1 | — |
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
| R15/READER-A-10 | P2 | ④ | ④ 不改：计数史已压成一行（R17 批），删除会损可追溯性 | — |
| R15/READER-A-11 | P2 | ① | §2.2 栈布局改为"合计 16 字节"，不再说"凑满" | 五门/源码对照 |
| R15/READER-A-12 | P2 | ④ | ④ 不改：§1.2 框图号是阅读顺序、D1–D5 是依赖序，索引已消歧 | — |
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
| R15/READER-A-27 | P3 | ④ | ④ 不改：commit 史已压成一行（R17 批），再删会损可复算性 | — |
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
| R16/DESIGN-03 | P2 | ② | prepare_boot 删除，阻塞于裁决项 1 | — |
| R16/DESIGN-04 | P2 | ② | arena 双写简化，修于 R18 | — |
| R16/DESIGN-05 | P2 | ② | 双 bump 分配器统一，修于 R18 | — |
| R16/DESIGN-06 | P2 | ② | DM 候选顺序提常量，修于 R18 | — |
| R16/DESIGN-07 | P2 | ② | identity_windows 恒空删除，修于 R18 | — |
| R16/DESIGN-08 | P2 | ② | FileLoader borrow 接口，修于 R18 | — |
| R16/DESIGN-09 | P2 | ② | 后置断言独立输入，修于 R18 | — |
| R16/DESIGN-10 | P2 | ① | 同上 | cargo test 全绿
| R16/DESIGN-11 | P2 | ② | pub 字段/getter 二选一，修于 R18 | — |
| R16/DESIGN-12 | P2 | ② | 三校验 enum 化，修于 R18 | — |
| R16/DESIGN-13 | P2 | ① | 同上 | cargo test 全绿
| R16/DESIGN-14 | P3 | ② | PagingWithId/HUGE_PAGE_SHIFT 审计后删，修于 R18 | — |
| R16/DESIGN-15 | P3 | ② | Plan 两遍遍历改名，修于 R18 | — |
| R16/DESIGN-16 | P3 | ② | 同上 R18 | — |
| R16/DESIGN-17 | P3 | ② | 同上 R18 | — |
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
| R16/READER-A-03 | P2 | ② | 跨文档链接基址（B-46/C-18 门），修于 R18 | 五门/源码对照
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
