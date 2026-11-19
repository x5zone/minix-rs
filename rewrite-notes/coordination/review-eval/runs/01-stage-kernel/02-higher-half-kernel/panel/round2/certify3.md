# D02 R2 最终认证（第 3 次、闭集）

- 文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 行数：1280
- sha256：`6c923216e9b853f61598e71095b692a28dd86358416e1745cf2cd9372b7488ea`（前缀 `6c923216`，与任务给定一致）
- 读取范围：§前置知识 L8、§2.2 L207 及其邻句（§2.2 L130–L208、§3.1 L330–L400、§4.4 L1000–L1078）。未读历史评审、其他面板、expected。除本文件外未改动仓库任何文件；objcopy 探针在临时目录完成并已删除。

## 结论摘要

三条核对项全部 CLOSED；四道门全部通过（exit 0）。新发现 1 项 P2（L207 的未定义符号清单不完整，可选修复，不阻断认证，不重开第 2 条）。无新 P0/P1。

## 逐条核对

### 第 1 条：§前置知识 L8 的歧义与五条腿（CLOSED）

原文（L8）：

> Rust 侧各启动腿与测试载体的起点按腿不同（测试载体不是生产路径）：v1 send 在交接前是“分页已开、shim 低恒等视图”；riscv64 自引导从“分页未开、物理视图”开始（Step 3 才开）；aarch64 direct-core 低地址进入、物理视图；riscv64 测试载体物理视图、PC 在高半 VMA；inline 测试载体低地址视图。

歧义消除：括号内“测试载体不是生产路径”是显式断言，不再需要读者从上下文推断。§3.1 L362 末尾同样写明测试载体“不是生产路径”，§4.4 L1005/L1014 把生产路径只列为 A（v1 send/receive）与 B（riscv64 自引导），L1020 另列“测试/直核路径（inline）”。三处口径一致，生产路径不含测试载体。

五条腿对照（L8 与 §3.1 L368–L373 表、§4.4 L1029–L1034 表）：

| # | L8 的腿与起点 | §3.1 表的进入/执行视图与 PC | §4.4 表的进入/执行视图与 PC |
|---|---|---|---|
| 1 | v1 send：交接前“分页已开、shim 低恒等视图” | 高半视图 / 高半 VMA | 高半视图 / 高半 VMA，不需要 `delta` |
| 2 | riscv64 自引导：“分页未开、物理视图”（Step 3 开分页） | 物理视图 / 高半 VMA（direct leg 加 `delta`） | 物理视图 / 高半 VMA（已加 `delta`） |
| 3 | aarch64 direct-core：低地址进入、物理视图 | 低地址进入、物理视图 / 高半 VMA（`ldr` 绝对值） | 同 |
| 4 | riscv64 测试载体：物理视图、PC 在高半 VMA | 物理视图 / 高半 VMA（direct leg 加 `delta`） | 与自引导合并一行：物理视图 / 高半 VMA（已加 `delta`） |
| 5 | inline 测试载体：低地址视图 | 低地址视图 / 低地址 | 低地址视图 / 低地址 |

逐腿一致。第 1 条的“交接前 shim 低恒等视图”与表里的“高半视图”不是矛盾：L8 用“在交接前”限定了 shim 一侧的阶段，表描述的是 v1 接收镜像跳转后的执行视图（§4.4 L1010–L1012 的 `_start` → `arch_boot_resume_v1` → `jump_to_kmain` 落在高半 VMA）。第 4 条在 §4.4 与第 2 条合并为一行，是两腿链接形态与视图相同而合并，不丢腿；§4.4 的代码块 L1022 仍单列“riscv64 测试载体 → 直接 `arch_boot`”。第 5 条的“inline 测试载体”与 §3.1 L372、L990 的“x86_64/aarch64 的内联测试载体”同指，L990 明确“riscv64 测试载体不是这种”，与第 4 条的分列自洽。

结论：CLOSED。

### 第 2 条：§2.2 L207 的 `__k_unpaged_*` 五函数（CLOSED）

原文（L207 首句）：

> 链接期会得到五个函数各自的 `__k_unpaged_*` 未定义符号（`__k_unpaged_pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging`；`pre_init.o` 一侧的引用已加前缀，普通 `pg_utils.o` 只有不带前缀的定义）。

核对结果：

1. 五个函数与顺序和 `minix3/minix/kernel/arch/i386/pre_init.c:230-234` 完全一致：

```c
pg_clear();
pg_identity(&kinfo);
kinfo.freepde_start = pg_mapkernel();
pg_load();
vm_enable_paging();
```

2. 五个函数在 i386 侧各自只有一处定义，且都在 `i386/pg_utils.c`（`cut_memmap` 之外的五函数）：`pg_identity` L162、`pg_mapkernel` L186、`vm_enable_paging` L204、`pg_load` L247、`pg_clear` L254。`vm_enable_paging` 已包含在清单里，不再停留在旧的 `__k_unpaged_pg_*` 范围。
3. 前缀机制与文字一致：`Makefile.inc:51-54` 的 `.for unpaged_obj in …` 列出 `pg_utils.o`，循环体 L64/L67 用 `objcopy --prefix-symbols=__k_unpaged_` 生成 `unpaged_pg_utils.o`；普通 `pg_utils.o` 仍由 `SRCS`（L76）编译，只有不带前缀的定义。
4. 清单采用“首项带全前缀、后续项为同前缀尾名”的斜杠省略写法。前导语“五个函数各自的 `__k_unpaged_*`”已把前缀规则说清，五个尾名逐项可还原为 `__k_unpaged_pg_identity`、`__k_unpaged_pg_mapkernel`、`__k_unpaged_pg_load`、`__k_unpaged_vm_enable_paging`，不构成歧义。
5. 宿主侧最小复现确认 `objcopy --prefix-symbols` 会同时改写未定义引用（临时对象：改写前 `U bar`，改写后 `U __k_unpaged_bar`），与 L154 的“定义与引用都加”一致。

结论：CLOSED。五个函数已正确、含 `vm_enable_paging`，与 `pre_init.c:230-234` 一致。

### 第 3 条：§2.2 L207 的 `pre_init.o` 半句主语（CLOSED）

原文（L207 第二句）：

> 若想用 `pre_init.o` 做同型替换也不可行——`pre_init.c` 不在任何 `SRCS` 里，去掉名单项会连唯一生成规则一起消失。

核对结果：

1. 主语与逻辑已闭合：“若想用 `pre_init.o` 做同型替换”给出动作主体与动作对象，破折号后给出不可行的原因；不再有悬空的“`pre_init.o` 一侧的引用已加前缀”式半句（该内容已在前一句的括号里以“`pre_init.o` 一侧的引用已加前缀”完整表述）。
2. `pre_init.c` 确实不在 `SRCS` 里：`minix3/minix/kernel/arch/i386/Makefile.inc:75-81` 的 `SRCS+=` 两行列出的是 `mpx.S arch_clock.c … pg_utils.c` 与 `do_iopenable.c … usermapped_glo_ipc.S`，没有 `pre_init.c`。
3. `pre_init.o` 在该目录只出现在 `.for unpaged_obj in …` 名单（L51-54）；去掉名单项，`unpaged_pre_init.o` 的唯一生成规则（L57-68，含 bitcode 分支）随之消失，且没有别的目标依赖 `pre_init.o`。全树检索印证：`grep -rn 'pre_init' minix3/minix/kernel/` 只命中 `head.S` 的调用、`main.c` 的注释、`Makefile.inc` 的注释与名单，没有第二处 `SRCS` 或生成规则。
4. 该句与前后句的因果衔接成立：上一句说明“去掉 `pg_utils.o` 得到的是链接期未定义符号，不是静默执行”；本句排除“换 `pre_init.o` 做同型替换”这条替代路径；下一句给出真正能构造“函数落高半、分页前执行”的错法（把调用方也移出名单）。

结论：CLOSED。

## Gate 证据

均在本仓库根执行，命令与输出如下（`exit` 为命令本身退出码，不取管道尾命令的码）：

| 门 | 命令 | 输出 | exit |
|---|---|---|---|
| doc-citation-check | `python3 -I tools/doc-citation-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | `blocks=32 checked=1 failed=0 skipped=31 warned=0` | 0 |
| anchor-resolve --check | `bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | `anchors=55 resolved=55 zero-def=0 multi-def=0` | 0 |
| doc-structure-check | `python3 -I tools/doc-structure-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | `✅ doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析` | 0 |
| doc-ref-check | `AWK=gawk tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | `✅ doc-ref-check: rewrite-notes/01-stage-kernel/02-higher-half-kernel.md 内部章节引用均可解析` | 0 |

补充：L207 的两个锚点均解析成功——`minix3/minix/kernel/arch/i386/pg_utils.c:pg_mapkernel` → 源文件 L186（`int pg_mapkernel(void)`）；`minix3/minix/kernel/arch/i386/pre_init.c:pre_init` → 源文件 L217（`kinfo_t *pre_init(u32_t magic, u32_t ebx)`）。citation-check 的 `checked=1` 是该工具只对可机械对账的代码块做逐行比对，其余 31 块按工具自身规则跳过，不是失败。

## FINDING RD2-CERT3-01（P2）

位置：§2.2 L207 首句“链接期会得到五个函数各自的 `__k_unpaged_*` 未定义符号”。

事实：去掉 `pg_utils.o` 后，链接期出现的未定义符号不止这五个。`pre_init.c` 的同一对象里还有 `get_parameters`，它调用 `add_memmap`（L182、L186、L187）与 `cut_memmap`（L211）；两者同样只在 `i386/pg_utils.c` 定义（`cut_memmap` L32、`add_memmap` L86，全树无第二处定义，也无宏改名）。`pre_init.o` 整体被 `objcopy --prefix-symbols` 改写，这两个引用也会变成 `__k_unpaged_add_memmap` 与 `__k_unpaged_cut_memmap`，因此实际是 7 个未定义符号。

取证命令与读数：

```text
$ grep -nE 'add_memmap|cut_memmap' minix3/minix/kernel/arch/i386/pre_init.c
182:			add_memmap(cbi, mmap->mm_base_addr, mmap->mm_length);
186:		add_memmap(cbi, 0, mbi->mi_mem_lower*1024);
187:		add_memmap(cbi, 0x100000, mbi->mi_mem_upper*1024);
211:		cut_memmap(cbi,

$ grep -rn 'void add_memmap\|void cut_memmap' minix3/minix/kernel/arch/i386/
minix3/minix/kernel/arch/i386/pg_utils.c:32:void cut_memmap(kinfo_t *cbi, phys_bytes start, phys_bytes end)
minix3/minix/kernel/arch/i386/pg_utils.c:86:void add_memmap(kinfo_t *cbi, u64_t addr, u64_t len)

$ grep -rn '#define[[:space:]]*\(add_memmap\|cut_memmap\)' minix3/
（无命中）
```

这条与文档自身的前提一致：L154 已写明 `objcopy --prefix-symbols` 把“定义与引用都加”前缀。对未定义引用的改写也已用最小对象复现：改写前 `U bar`，改写后 `U __k_unpaged_bar`。

影响：不改变该段的主结论——失败发生在链接期，不是“低侧副本静默执行”；也不会推翻第 2 条的 CLOSED 判定（`pre_init.c:230-234` 的五个函数本身已正确、完整）。影响限于“会怎样”推演的产物清单不完整：按文档复现实验的读者会看到 7 条未定义符号，而正文只列 5 条。

建议（二选一，均为 P2 可选修复）：
- 把“五个函数各自的”改为“至少这五个函数各自的”，并在括号补一句“同一对象的 `get_parameters` 还引用 `__k_unpaged_add_memmap`/`__k_unpaged_cut_memmap`，实际共 7 个”；
- 或直接把清单补成 7 个（按 `pre_init.c` 的调用顺序：`pg_clear`、`pg_identity`、`pg_mapkernel`、`pg_load`、`vm_enable_paging`，再补 `add_memmap`、`cut_memmap` 并注明后两者来自 `get_parameters`）。

级别理由：P2（完整性/表述清晰度）。清单是说明性枚举，句中没有“只有”限定，主结论不受影响；但作为可复现的链接结果描述，五个不是完整读数，故不升 P0/P1。

## 最终

三条核对项全部 CLOSED，四道门全部通过。无新 P0/P1；新 P2 一项（RD2-CERT3-01），可选修复，不阻断认证。

最终认证新准入 = 1（该计数已被下方“追加认证”一节覆盖）

## 追加认证：RD2-CERT3-01 修复后复验

- 重读范围：仅 §2.2 L207 该句（文档当前 sha256 前缀 `8b3680a1`，1280 行）。未重扫全文。
- 该句现为：“链接期会得到 `__k_unpaged_*` 未定义符号（至少 `__k_unpaged_pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`/`vm_enable_paging`，另有 `__k_unpaged_add_memmap`/`cut_memmap`；…）”。
- 与 `pre_init.c` 的调用逐项对账：`pg_clear` L230、`pg_identity` L231、`pg_mapkernel` L232、`pg_load` L233、`vm_enable_paging` L234、`add_memmap` L182/L186/L187、`cut_memmap` L211。这七个正是 `pre_init.o` 引用且 `pg_utils.c` 定义的完整符号集合，清单与调用一致；“至少”与“另有”的措辞准确；`cut_memmap` 的前缀省略由本句前导 `__k_unpaged_*` 与并列结构消歧。
- RD2-CERT3-01：CLOSED。该句其余部分（`pre_init.o` 同型替换的主语与因果）未变，原第 3 条维持 CLOSED。
- 新 P0/P1/P2：无。

最终认证新准入 = 0
