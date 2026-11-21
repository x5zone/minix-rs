# D03 R1 独立事实审查（P-FACT，C/Rust 合并）

## 基线核对

- 目标文件：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
- 行数：1382（`wc -l`）
- sha256：`469727d74e1ec63c90720ffc81a0fb4804a50b108b22566791ff2b19bd8e11a2`，前缀 `469727d74e1ec63c` 与任务基线一致。
- 我未读取历史评审、本 run 顶部 V1 段、其他面板输出与 expected/。

## 门读书记录（本轮实际执行）

| 检查 | 命令 | 读数 | 退出码 |
|---|---|---|---|
| doc-citation-check | `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/03-kmain-cstart.md` | `blocks=42 checked=0 failed=0 skipped=42 warned=0` | 0 |
| anchor-resolve --check | `bash tools/anchor-resolve.sh --check <doc>` | `anchors=80 resolved=78 zero-def=2 multi-def=0` | 0 |
| doc-structure-check | `python3 tools/doc-structure-check.py <doc>` | `1 个文件表结构与内部 .md 链接均可解析` | 0 |
| doc-ref-check | `bash tools/doc-ref-check.sh --check <doc>` | `内部章节引用均可解析` | 0 |
| doc-code-map --check | `bash tools/doc-code-map.sh <doc> --check` | 21 个文件，存在性 21/21 通过 | 0 |
| doc-style-lint | `bash tools/doc-style-lint.sh <doc>` | `error级=0` | 0 |

两条 ZERO-DEF（anchor-resolve 原始输出）：

```
rewrite-notes/01-stage-kernel/03-kmain-cstart.md:964: os/kernel/src/lib.rs:fn kernel_may_alloc → ZERO-DEF（符号不存在/已改名）→ 按 P0-fact 处理
rewrite-notes/01-stage-kernel/03-kmain-cstart.md:1187: os/kernel/src/globals.rs:static KERNEL_MAY_ALLOC → ZERO-DEF（符号不存在/已改名）→ 按 P0-fact 处理
```

doc-citation-check 的 `checked=0 / skipped=42` 原因是该工具只解析以 `//` 开头的引文标注（`ANNOTATION_RE = ^\s*//\s*(代码引用|节选|...)`），而本文所有代码块用 `/* ... */`，因此 42 个块全部被跳过、没有任何逐行对账。见 SUGGESTION-1。

## Doc Code Sync 七项读数

| 项 | 读数 |
|---|---|
| Sync-1 `static mut` | `rg -n "static mut" <doc>` 零命中（exit 1） |
| Sync-2 路径与仓库 | 见 doc-code-map：21/21 存在；C 侧 7.2 清单 13 个路径逐一 `[ -f ]` 全部存在；Rust 侧 7.3 清单全部存在 |
| Sync-3 虚构常量 | 抽查的 C 宏（`IDT_SIZE`/`GDT_SIZE`/`TSS_INDEX`/`X86_STACK_TOP_RESERVED`/`ARM_STACK_TOP_RESERVED`/`TSS_TYPE`/`IRQ0_VECTOR`/`IRQ8_VECTOR` 等）均在 `minix3/` 有定义 |
| Sync-4 文档内 TODO | `rg "^\s*>\s*\*\*TODO" <doc>` 零命中（exit 1）；§5.3 的 R03-T01..T07 在 `rewrite-notes/coordination/DOC-ITER-LEDGER.md` 有登记 |
| Sync-5 符号锚点 | 80 锚点、78 resolved、2 ZERO-DEF（见 FINDING RD3-FACT-03） |
| Sync-6 workspace 前缀 | 无裸 `kernel/src/` 路径（grep exit 0 但无输出行） |
| Sync-7 测试数量 | 9 个文件的逐文件计数与合计 56 全对；crate 全量数字已漂移（见 FINDING RD3-FACT-06） |

---

## FINDING RD3-FACT-01（P0）
- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/03-kmain-cstart.md:140-148`（另 `:178` 的反事实与 `:136` 的步骤描述同根）
- 严重度：P0（P0-fact）
- 问题：文档断言 `kmain` 开头两行 `memcpy` 的“源与目的是同一个对象”，因此第一行是“全局自拷贝”、第二行 `kinfo.kmess` 就是最终 `kmessages` 的地址，并据此在 :178 断言“删掉这两行 C 侧切不出后果”。这个论证对当前 C 树不成立：`pre_init.c` 里的 `kinfo`/`kmessages` 是**未分页（unpaged）暂存副本**，与 `main.c`/`kmain` 使用的最终全局不是同一对象。
- 影响：§2.1 的核心论证与其反事实前提都错了。删掉这两行不是 no-op：最终 `kinfo`/`kmessages`（`.usermapped`）会保持全零，`pre_init` 收集的 multiboot 信息、模块表、`kmess` 缓冲区全部丢失；按源码里紧随其后的检查，`kmain` 会在 `NR_BOOT_MODULES != kinfo.mbi.mi_mods_count`（`minix3/minix/kernel/main.c:160-162`）处 panic。文档用这个错误前提去对照 Rust 侧 kmain 的故障（:178），会让读者以为 C 侧本来就可以不拷贝。
- 证据或推理（E1/E2/E3）：
  - E1：`minix3/minix/kernel/usermapped_data.c:7,9` 定义最终全局：`struct kinfo kinfo __section(".usermapped");`、`struct kmessages kmessages __section(".usermapped");`（该文件在 `SRCS` 里，`minix3/minix/kernel/Makefile:15`）。
  - E1：`minix3/minix/kernel/arch/i386/pre_init.c:20-21` 另定义 `kinfo_t kinfo;` / `struct kmessages kmessages;`；`pre_init.o` 在未分页改名名单里（`minix3/minix/kernel/arch/i386/Makefile.inc:51`），构建用 `objcopy --prefix-symbols=__k_unpaged_`（同文件 :64/:67）。实测该选项同时改定义与引用：`objcopy --prefix-symbols=__k_unpaged_ t1.o` 后 `nm` 同时出现 `D __k_unpaged_g` 与 `U __k_unpaged_h`。
  - E1：`minix3/minix/kernel/arch/i386/kernel.lds:19` 把 `unpaged_*.o` 的 `.bss` 收进 `.unpaged_bss`（VMA=物理地址），而 `.usermapped` 在 `_kern_offset` 之后（:22-27）；两组地址不同。
  - E1：`minix3/minix/kernel/main.c:128-129` 的 `memcpy(&kinfo, local_cbi, ...)` 中 `kinfo` 解析到 `usermapped_data.c` 的明文符号（`main.c` 未被改名，`pre_init.o` 的副本已被改名），`local_cbi` 是 `pre_init` 返回的 `&__k_unpaged_kinfo`。
  - E2：若两行是自拷贝，C 代码没有理由在 `kmain` 里再写一遍；而 `usermapped_data.c` 的存在、`objcopy` 改名与 `.unpaged_bss` 分节正是为了避免同名符号相撞并给预重定位阶段提供暂存区。
- 最小修复：改写 §2.1 第二段：说明 `pre_init` 使用 `.unpaged_bss` 里的暂存 `kinfo`/`kmessages`，`kmain` 的两行是“暂存副本 → 最终 `.usermapped` 全局”的搬运；删除“自拷贝”措辞与 :178 的 C 侧反事实，或把反事实改成“删掉后最终全局为零、`module_list`/`kmess` 丢失，启动在 boot module 计数处失败”。
- 置信度：高
- 是否需源码核实：是，已核（`usermapped_data.c`、`pre_init.c`、`Makefile.inc` 改名名单、`kernel.lds`、`main.c`）
- 若我错了：若链接期存在把明文 `kinfo` 解析到 `__k_unpaged_kinfo` 的别名，则自拷贝成立；我检索了 `minix3/minix/kernel/` 与 `kernel.lds`，未发现 `PROVIDE(kinfo...)`、`#define kinfo` 或其它别名；`proto.h:50` 只有 `kmain` 的别名。若存在某个未纳入 `minix3/` 的构建补丁完成该别名，则本条撤回。
- 对应判据：review-doc-checklist §2.2（C 代码引用验证，P0）、§2.0.3（解释因果链验证，P0）；AGENTS.md“真值优先序：Minix3 C 源码行为 > 文档”

## FINDING RD3-FACT-02（P0）
- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/03-kmain-cstart.md:152-161`
- 严重度：P0（P0-fact）
- 问题：文档断言 `kernel_may_alloc` 的“两处定义（带初值的那个副本）在 i386 与 earm 各自的 `pre_init.c` 里”，并据此说“进入 `kmain` 时它的值已经是 1；`kmain` 这一行是把同一句话再说一遍”。对 `main.c` 实际读写的那个符号不成立：普通（非未分页）副本由 `table.c` 通过 `EXTERN` 展开定义，BSS 初值为 0；`pre_init.c:33` 的 `=1` 副本在未分页改名后是另一个符号，只被未分页的 `pg_utils.o` 使用。
- 影响：把一个**承重赋值**说成冗余。`kmain` 的 `kernel_may_alloc = 1`（`minix3/minix/kernel/main.c:142`）是把普通副本从 0 置 1 的唯一动作；`cstart` 之后 `arch_boot_proc` 走 `libexec_pg_alloc → pg_map(PG_ALLOCATEME,...) → pg_alloc_page` 的 `assert(kernel_may_alloc)`（普通 `pg_utils.o`，`minix3/minix/kernel/arch/i386/pg_utils.c:143` 等）依赖它。读者若据此认为该行可删，会直接在 Phase C 触发断言。
- 证据或推理（E1/E2/E3）：
  - E1：`minix3/minix/kernel/glo.h:9-12` 在 `_TABLE` 下把 `EXTERN` 定义为空；`minix3/minix/kernel/table.c:27` 定义 `_TABLE` 并 `#include "kernel/kernel.h"`（:42 再包含 `glo.h`），因此 `glo.h:76` 的 `EXTERN int kernel_may_alloc;` 在 `table.o` 里生成一个零初始化的明文定义。
  - E1：`pre_init.o` 在未分页改名名单（`arch/i386/Makefile.inc:51`），`pre_init.c:33` 的 `int kernel_may_alloc = 1;` 成为 `__k_unpaged_kernel_may_alloc`；未分页 `pg_utils.o` 的引用同被改名，故它看到的是 1。普通 `pg_utils.o`（`SRCS` 里同样编译）的引用保持明文，绑定 `table.o` 的 0 初值副本。
  - E1：文档自己的命令 `grep -rn "int kernel_may_alloc" minix3/minix/kernel/` 只能看到字面定义，看不到 `table.c` 经宏生成的定义，所以该读数不能证明“只有两处定义”。
  - E2：`main.c` 的 `kmain` 被 `proto.h:50` 的 `#define kmain __k_unpaged_kmain` 改名（`main.c` 未定义 `UNPAGED`），说明该工程的“未分页”是符号级命名空间而非整文件同名共享；`kernel_may_alloc` 遵循同一机制。
- 最小修复：改写第四步段落：区分“未分页暂存副本（初值 1，供 `pre_init` 阶段使用）”与“普通 `table.c` 零初始化副本（`kmain` 置 1、`bsp_finish_booting` 置 0）”；删除“把同一句话再说一遍”的结论。
- 置信度：高
- 若我错了：若 `table.c` 的宏展开因 `glo.h` include guard 或 `_TABLE` 时序未生成 `kernel_may_alloc` 定义，则明文符号只能来自别处；我按 `glo.h:9-12` 与 `table.c:27`/`kernel.h:42` 的包含链判断该定义存在。若构建产物 `nm table.o` 显示无该符号，则本条撤回。
- 对应判据：review-doc-checklist §2.2（C 代码引用验证，P0）、§2.0.3（因果链，P0）

## FINDING RD3-FACT-03（P0）
- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/03-kmain-cstart.md:964`、`:1187`
- 严重度：P0（P0-fact；Sync-5 规定符号 0 定义 = P0-fact）
- 问题：文档两处把已删除的 Rust 符号当成现行实现：:964 断言 Phase A 有 `KERNEL_MAY_ALLOC.store(true, Ordering::Release)`，并给 `fn kernel_may_alloc` 挂锚点、称其“当前没有生产调用者”；:1187 把 `static KERNEL_MAY_ALLOC` 锚到 `os/kernel/src/globals.rs` 并称“保留（改型为原子量）”。当前源码里这两个符号都不存在，`kmain` 也没有这次 store。
- 影响：§4.1（Phase A 的落地）与 §4.7（C↔Rust 对照表）各有一行现行业务事实是假的；anchor 检查已按 P0-fact 打标。更麻烦的是它与同段 :964 自己的“`add_memmap` 的 Rust 对应物已不在 `os/kernel/src`”相矛盾：读者无法判断闸门到底还在不在。
- 证据或推理（E1/E2/E3）：
  - E1：`bash tools/anchor-resolve.sh --check <doc>` 输出两条 `ZERO-DEF`（:964 `fn kernel_may_alloc`、:1187 `static KERNEL_MAY_ALLOC`），汇总 `anchors=80 resolved=78 zero-def=2`。
  - E1：`grep -rn "KERNEL_MAY_ALLOC" os --include=*.rs --exclude-dir=target` 零命中；`grep -rn "fn kernel_may_alloc" os --include=*.rs --exclude-dir=target` 零命中。全树唯一命中在构建产物 `os/target/fmtchk/kernel_src_lib.rs:1850`（`pub fn kernel_may_alloc()`），不是 `src` 源码。
  - E1：文档自己给的复现命令 `grep -rn 'kernel_may_alloc()' os --include=*.rs` 实际会命中 `os/target/fmtchk/kernel_src_lib.rs:1850` 一行，不是“零命中”；若加 `--exclude-dir=target` 才是零命中。
  - E1：`git show c03a26875 --stat` 的提交说明：“删除 KERNEL_MAY_ALLOC 全局布尔（唯一读者是被删的 add_memmap 断言）；kernel_may_alloc() 公开函数随之删除”；diff 里对应 `-KERNEL_MAY_ALLOC.store(true, Ordering::Release)`、`-pub fn kernel_may_alloc()`。
  - E1：当前 `os/kernel/src/lib.rs` 的 `kmain`（:1673 起）在 Phase A 只做 `validate`/`check_boot_postconditions`/console/`store_kernel_info`/`init_from_kinfo`，没有 `KERNEL_MAY_ALLOC` 或等价 store。
- 最小修复：删除 :964 的 `KERNEL_MAY_ALLOC.store(...)` 与 getter 句，改为“Phase A 不再有分配闸门（D01-F08 删除了唯一读者）；Phase A.2 故意为空”；:1187 行改为“已删除（D01-F08）；VM 启动前的写权限由调用图/FreeRangeSet 保证”，并去掉锚点。若设计上要恢复闸门，则应先恢复代码再保留文档（不是本轮文档修复的选择）。
- 置信度：高
- 是否需源码核实：是，已核（anchor 工具、grep、git 提交）
- 若我错了：若 `KERNEL_MAY_ALLOC` 定义在 `os/` 之外的 crate 或以宏生成而我的 grep 漏掉，则本条撤回；但 anchor 工具与全树 grep 都零命中，且提交记录明确写“删除”。
- 对应判据：review-doc-checklist Sync-5（0 定义 = P0-fact）、§2.4（文档与代码一致性，P1，此处升为 P0）、AGENTS.md “没有附命令与输出的 ✅ 一律作废”

## FINDING RD3-FACT-04（P1）
- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/03-kmain-cstart.md:890`
- 严重度：P1（文档与代码漂移；引文已不存在）
- 问题：文档称 `build_bootstrap_root_and_enable` 的 Step 1 注释写着 `Maps 1024 × 4MB = 4GB (C) or 4 × 1GB (x86-64)…`。当前源码里没有这句话；该注释已在 2026-11-09 的 BOOT-SCOPE 2a 提交中被替换，Step 1 现在明确只铺“最小 boot 恒等窗口”，不再铺低 4GB。
- 影响：§3.5 用这条引文支撑“恒等窗口仍由 01/02 篇承担”的对照；读者按引文去源码核会找不到，并且会误以为当前实现仍是 1024×4MB 全量恒等映射——正好与现行设计相反（现行是 minimal windows，越界空洞会变成缺页）。
- 证据或推理（E1/E2/E3）：
  - E1：当前 `os/kernel/src/lib.rs:1212` 起的 `build_bootstrap_root_and_enable`，Step 1 注释原文为 `C maps 4 GiB with pg_identity because the 32-bit kernel runs inside that window; the Rust cross-image leg only needs its shim image/stack, ... the minimal windows turn those into page faults.`；全文件 grep `Maps 1024`/`4 × 1GB`/`1024 × 4MB` 零命中。
  - E1：`git show 3016acbfc:os/kernel/src/lib.rs | sed -n '930,932p'` 显示该引文在文档最后一次编辑时确实存在（`:932 // Maps 1024 × 4MB = 4GB (C) or 4 × 1GB (x86-64) or equivalent on other archs.`）；`git show 4f2e9e786` 显示该行被删除（`-    // Maps 1024 × 4MB = 4GB ...`）。
- 最小修复：把引文替换为现行 Step 1 注释的原文（或改述为“现行 Step 1 只铺最小恒等窗口；C 的 `pg_identity` 对应关系写在注释里，但已不是 1024×4MB 全量映射”），并保留“常量定义在 `os/arch/src/lib.rs`”的事实（`BOOT_IDENTITY_MAP_END`，`os/arch/src/lib.rs:217`）。
- 置信度：高
- 是否需源码核实：是，已核（当前源码 + git 3016acbfc/4f2e9e786）
- 若我错了：若仓库另有未提交/未跟踪的 lib.rs 变体包含该注释，则本条撤回；当前工作树 `git status` 未显示 `os/kernel/src/lib.rs` 被修改。
- 对应判据：review-doc-checklist §2.2（代码引用验证）、§2.4（文档与代码一致性，P1）、Sync-2

## FINDING RD3-FACT-05（P2）
- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/03-kmain-cstart.md:968`
- 严重度：P2
- 问题：文档说“诊断块留下的四个读数（`rdi`、全局槽指针、`rsp`、`bootstrap_len`）就是为此加的”。当前 `kmain` 的诊断块只打印三个读数（`rdi`、`kinfo` 槽指针、`rsp`），`bootstrap_len` 只出现在两处注释里，没有任何读取或打印。
- 影响：§4.1 用这四个读数作为“参数引用被 spill/覆写”这一故障记录的取证；读者按文档去串口日志找 `bootstrap_len` 会找不到，也会高估现有诊断的覆盖。
- 证据或推理（E1/E2/E3）：
  - E1：`os/kernel/src/lib.rs:1707-1713` 的打印序列只有 `preA rdi=...`、` kinfo=...`、` rsp=...`；grep `bootstrap_len` 在 `lib.rs` 只有 :1680（注释叙述）与 :1691（注释“bootstrap_len read through the slot”），没有表达式读取。
  - E2：源码注释（:1689-1691）声称“read ... and the bootstrap_len read through the slot”，与同一函数体的实际打印不一致；文档照抄了注释口径。
- 最小修复：把“四个读数”改为“三个读数（rdi、全局槽指针、rsp）”；若确需 `bootstrap_len` 证据，则在诊断块补一行 `write_hex(kernel_info.bootstrap_len)` 并同步文档。
- 置信度：高
- 是否需源码核实：是，已核（lib.rs 诊断块与 `bootstrap_len` 全部引用）
- 若我错了：若 `bootstrap_len` 由宏 `boot_stage!` 或其它日志路径输出而我的 grep 漏掉，则本条撤回；我按 `grep -n "bootstrap_len" os/kernel/src/lib.rs` 只看到两处注释。
- 对应判据：review-doc-checklist §2.4（文档与代码一致性）、§2.2（引用验证）

## FINDING RD3-FACT-06（P2）
- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/03-kmain-cstart.md:1221`
- 严重度：P2（精确计数漂移；不影响结论，但读者复算结果不同）
- 问题：文档称“arch crate 全量 `src/*.rs` 是 375 个测试/54 文件，`os/arch/tests/*.rs` 另有 94 个/20 文件”。当前实际是 374 个测试/54 个含测试文件，以及 `os/arch/tests` 95 个测试/21 个文件。逐文件计数与“9 文件合计 56”仍然全对。
- 影响：§5.1 的上下文数字会被复算的读者判为错；`375`/`94` 的差值恰好来自文档最后一次编辑后的两删一增与新测试文件，属于持续漂移点。
- 证据或推理（E1/E2/E3）：
  - E1：`find os/arch/src -name '*.rs' -print0 | xargs -0 cat | grep -c '#\[test\]'` → 374；`grep -rl '#\[test\]' os/arch/src --include=*.rs | wc -l` → 54。
  - E1：`find os/arch/tests -name '*.rs' -print0 | xargs -0 cat | grep -c '#\[test\]'` → 95；`find os/arch/tests -name '*.rs' | wc -l` → 21。
  - E1：在文档最后一次编辑的 `3016acbfc` 上复算，恰为 375/54 与 94/20（`git ls-tree` + `git show` 逐文件 grep）；`git diff 3016acbfc HEAD -- os/arch/src` 显示 2 个 `#[test]` 被删、1 个新增；`git diff --stat` 显示新增 `os/arch/tests/pt_alloc_concurrent.rs`（1 条测试）。
  - E1：文档 §6.2 自己给的 9 文件命令输出 `16/13/11/2/1/3/2/1/7` 与当前源码完全一致（我按原命令重跑，逐行一致）。
- 最小修复：把 `375` 改 `374`、`94 个/20 文件` 改 `95 个/21 文件`；或在数字后标注“截至某提交”。逐文件表与 56 合计不动。
- 置信度：高
- 是否需源码核实：是，已核（当前工作树 + `3016acbfc` 快照）
- 若我错了：若统计口径把 `#[cfg(test)]` 模块数或其它属性也算作测试，数字会不同；但文档自己给的口径是 `grep -c '#\[test\]'`（:1319），按该口径当前值是 374/95。
- 对应判据：review-doc-checklist Sync-7（测试数量声称偏差）、§2.4

## FINDING RD3-FACT-07（P3）
- 位置：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/03-kmain-cstart.md:631`
- 严重度：P3（边界条件下的行为描述不精确）
- 问题：文档写“比较用的是 4MB 掩码，所以最后一个仍含真实内存的 4MB 块也会被一起标记”。该句只在 `mem_high_phys` 不是 4MB 对齐时成立。若 `mem_high_phys` 恰好落在 4MB 边界（常见：内存大小是 4MB 的整数倍），被标记的是从 `mem_high_phys` 开始的第一个**已经完全在内存之上**的块；最后一块真实内存反而不被标记。
- 影响：读者可能把“最后一个真实内存块被标记为 PWT|PCD（禁缓存）”当成无条件事实，从而在解释性能或 MMIO 访问时推错。源码注释的原意是“不对应物理内存的区间按非缓存映射”，文档的附加推论超出了它。
- 证据或推理（E1/E2/E3）：
  - E1：`minix3/minix/kernel/arch/i386/pg_utils.c` 的 `pg_identity` 条件为 `if((cbi->mem_high_phys & I386_VM_ADDR_MASK_4MB) <= (phys & I386_VM_ADDR_MASK_4MB)) flags |= PWT|PCD;`，`I386_VM_ADDR_MASK_4MB = 0xFFC00000`（`minix3/minix/include/arch/i386/include/vm.h:19`）。
  - E2：取 `mem_high_phys = 0x0800_0000`（128 MiB，对齐）：`0x0800_0000 & 0xFFC0_0000 = 0x0800_0000`；`i >= 32` 的目录项被标记（`i=32` 对应 `0x0800_0000`，已完全在内存之上），`i=31`（`0x07C0_0000..0x07FF_FFFF`，最后一块真实内存）不满足 `0x0800_0000 <= 0x07C0_0000`，不被标记。
  - E2：取 `mem_high_phys = 0x0810_0000`（非对齐）：`& mask = 0x0800_0000`，`i=32` 被标记，且该块确实还含真实内存（直到 `0x0810_0000`）——此时文档的说法成立。
- 最小修复：改为“`mem_high_phys` 未 4MB 对齐时，它所在的那最后一块真实内存也会被一起标记；对齐时被标记的是内存顶端之上的第一块”。或直接删掉“所以最后一个……”这半句，保留“从 `mem_high_phys` 所在的 4MB 对齐块起”。
- 置信度：高
- 是否需源码核实：是，已核（`pg_identity` 条件与掩码常量）
- 若我错了：若 `mem_high_phys` 在运行期总是非 4MB 对齐（例如 multiboot 报告的最高可用区间末尾总带偏移），则该句在本项目里恒真；但源码没有这种保证，`mem_high_phys` 只是 `max(addr+len)`。
- 对应判据：review-doc-checklist §2.1（概念准确性）、§2.4

## FINDING RD3-FACT-08（P0）
- 位置：`:661`（`vir2phys`/`pg_utils` 段）与 `:614`、`:668`（“同一个静态页目录数组”与“原地重填”机制/反事实）、`:892`（§3.5 复用同一机制）
- 严重度：P0（P0-fact）
- 问题：文档把 `pg_utils.c` 当成只编译一份的对象：说 `pg_utils.o` 在改名名单里、它的 `vir2phys` 调用解析到 `pre_init.c` 的恒等版、`pagedir` 落在 `.unpaged_bss`；并据此在 §2.6 把 `prot_init` 的页表重建讲成“清空同一个静态数组、清零与重填之间不写 CR3、取指靠 TLB 旧条目”的**原地重填**，还断言删掉重建只留 `pg_load` 时“冷启动可能照常跑”。实际上 `pg_utils.c` 编译两份：`unpaged_pg_utils.o`（改名，`.unpaged_bss` 里的 `pagedir`，`pre_init` 使用）与普通 `pg_utils.o`（明文，`.bss` 里的另一个 `pagedir`，`prot_init` 使用）。`prot_init` 清空/重填的不是当前 CR3 指向的那张表；它在另一个数组里建第二张表，最后由 `pg_load` 切换 CR3。
- 影响：§2.6 的机制叙述与两个反事实都建立在“同一数组原地重填”上，对当前 C 树不成立；§3.5 拿这个机制去对照 Rust 的论证也随之失准。尤其是“删掉清空与重建、只留 `pg_load`，冷启动可能照常跑”：普通 `pagedir` 冷启动是 BSS 全零，`pg_load` 会把 CR3 指到这张零表，下一条高半取指即缺页/三重故障，不是“看不出差别”。
- 证据或推理（E1/E2/E3）：
  - E1：`minix3/minix/kernel/arch/i386/Makefile.inc:75-76` 的 `SRCS` 含 `pg_utils.c`，普通 `pg_utils.o` 由 `SRCS` 生成的 `OBJS` 带入链接（`minix3/share/mk/bsd.prog.mk:425-426` 把 `SRCS` 赋给 `SRCS.${PROG}`，:523 再生成 `OBJS.${PROG}`）；同文件 :51 的未分页名单又生成 `unpaged_pg_utils.o`（:64/:67 `objcopy --prefix-symbols=`，:83 追加进 `OBJS.kernel`）。两者同时链接。
  - E1：`minix3/minix/kernel/arch/i386/kernel.lds:17-19` 只把 `unpaged_*.o` 的 `.text/.data/.bss` 收进 `.unpaged_*`；普通 `pg_utils.o` 的 `pagedir`（`pg_utils.c:19` 的 `static u32_t pagedir[1024]`）在 `.bss`（高半 VMA）。两份 `pagedir` 是不同地址的两个静态数组。
  - E1：`pre_init.c:230-233` 的 `pg_clear/pg_identity/pg_mapkernel/pg_load` 在 `pre_init.o` 改名后绑定 `unpaged_pg_utils.o`（`.unpaged_bss` 那份 `pagedir`，经 `unpaged` 恒等 `vir2phys`）；`protect.c:360-363` 的同一组调用在普通 `protect.o` 里保持明文，绑定普通 `pg_utils.o`（`.bss` 那份 `pagedir`）与 `protect.c` 的减偏移 `vir2phys`。`pre_init` 的 `pg_load` 写入的是未分页 `pagedir` 的物理地址；`prot_init` 的 `pg_load` 写入的是另一个数组的物理地址。
  - E2：“清零与重填之间没有写 CR3”字面为真，但安全性不来自 TLB 旧条目，而来自当前 CR3 根本不指向被清空的数组；`prot_init` 是“建第二张表 → 末尾切换 CR3”，不是原地重填。
- 最小修复：改写 §2.6 机制段：明确 `pg_utils.c` 有两份对象、两份 `pagedir`；`pre_init` 在未分页副本上建第一张表并装载，`prot_init` 在普通副本上建第二张表并在末尾切换 CR3；把“原地重填/同一个静态数组/取指靠 TLB 旧条目”改为“不同数组 + 末步切换”；并相应改写 §3.5 的 C 侧前提。`:661` 的 `vir2phys` 段也应点明“只有 unpaged 副本绑定 `pre_init.c` 的恒等版，普通副本绑定 `protect.c` 的减偏移版”。
- 置信度：高（`Makefile` 包含链、链接脚本、两份源码相互印证；未实际构建）
- 是否需源码核实：是，已核 `Makefile.inc`/`bsd.prog.mk`/`kernel.lds`/`pg_utils.c`/`pre_init.c`/`protect.c`；构建产物 `nm` 未核
- 若我错了：若普通 `pg_utils.o` 实际未进入链接（例如 `OBJS.kernel` 被整体覆盖），则 `protect.o` 的明文 `pg_load`/`pg_clear` 会无定义、内核无法链接，且文档的“`pagedir` 在 `.unpaged_bss`”就会成立。若能提供内核链接命令显示只有 `unpaged_pg_utils.o`，则本条撤回。
- 对应判据：review-doc-checklist §2.2（C 代码引用验证，P0）、§2.0.3（解释因果链验证，P0）、反事实前提核对

## SUGGESTION

### SUGGESTION-1（门禁空转）
- 位置：全文 42 个代码块（典型如 :99-100、:142-148、:154-159、:164-172、:184-259、:282-330 的块首标注）
- 问题：`doc-citation-check` 对本文的输出是 `checked=0 skipped=42`，因为它只认 `^\s*//\s*(代码引用|节选|...)` 形式的标注，而本文所有块用 `/* 代码引用，... */`。这意味着“引文逐行对账”这道机械门对本文完全空转；本轮我用手工+源码对读补上了，但下一轮若没人手工核，回归不会被工具拦住。
- 最小修复：把块首标注改成 `// 代码引用，...`（C 块也合法），或扩展工具同时接受 `/* ... */`。二者取其一即可让 `checked` 从 0 变成实际块数。
- 置信度：高（已读工具 `ANNOTATION_RE` 与实测输出）
- 对应判据：AGENTS.md “Gate 证据强度分级”与 Doc Code Sync 的引文门；本项是流程/工具建议，不计入 P0/P1。

### SUGGESTION-2（命令读数措辞）
- 位置：`:923`
- 问题：文档称 `rg -n "fn mask_all" os/plat/src` “命中三架构实现”。实际输出 5 处：trait 声明（`os/plat/src/interrupt.rs:167`）、mock（`os/plat/src/mock.rs:42`）、三架构实现（`x86_64/interrupt.rs:487`、`arm64/interrupt.rs:319`、`riscv64/interrupt.rs:167`）。三架构实现确实存在，但“命中三”与读者复跑看到的 5 行不符。
- 最小修复：改为“命中三架构实现各一处（另有 trait 声明与 mock 实现）”。
- 置信度：高
- 对应判据：Gate 证据可复现性

### SUGGESTION-3（引文口径）
- 位置：`:802`
- 问题：“`TrapEntryArch` 的注释是同一句话的另一半”不准确：`ProtectionArch` 的文档注释里已是完整句（`init() + load() must complete before TrapEntryArch::init()...`），`TrapEntryArch` 的文档注释是同一要求的**另一份表述**，且只写 `load()`（`os/arch/src/arch/trap_entry.rs:107-112`），并非同一句的后半截。
- 最小修复：改为“`TrapEntryArch` 的注释以 `load()` 口径重述了同一要求”。
- 置信度：高
- 对应判据：引文逐字/省略纪律

### SUGGESTION-4（读数指向）
- 位置：`:923`
- 问题：“`rg -n "intr.init\\(\\)" os/kernel/src/lib.rs` 命中 `init_clock_and_interrupts` 的调用点”——该正则命中的是函数体内部的 `intr.init();`（`os/kernel/src/lib.rs:2498`），不是 `init_clock_and_interrupts` 的调用点（调用点在 `kmain` 里）。
- 最小修复：改为“命中 `init_clock_and_interrupts` 内部的 `intr.init()` 行”。
- 置信度：高
- 对应判据：Gate 证据可复现性

---

## 本轮事实审查小结

- 基线：1382 行 / sha256 `469727d74e1ec63c…`，与任务给定一致。
- 六项工具全部 exit 0；但 `doc-citation-check` 因标注格式对本文空转（checked=0），`anchor-resolve` 抓到 2 条 ZERO-DEF。
- 共 8 条 FINDING：P0×4（FACT-01 C 侧 kinfo/kmessages 自拷贝论证、FACT-02 C 侧 kernel_may_alloc 冗余论证、FACT-03 Rust 侧已删符号仍当现行、FACT-08 pg_utils 双份编译导致 §2.6“原地重填”机制与反事实错误）、P1×1（FACT-04 Step 1 引文已不存在）、P2×2（FACT-05 四个诊断读数实为三个、FACT-06 crate 测试总数漂移）、P3×1（FACT-07 4MB 掩码边界句）。
- 已逐条附最小修复与“若我错了”的撤稿条件；FACT-01/02/03/04 的源码证据均为 E1（命令输出或源码原文），FACT-07/08 含 E2 推导。
- 未读取历史评审、V1 段、其他面板输出与 expected/；本报告的判断独立于这些材料。

## 附：已核对且一致的关键事实（无 finding，供交叉复核）

- C 代码块逐字抽查（含制表符级比对）：`main.c` 的 `kmain` 头部与 `cstart` 整函数、`protect.c` 的 `prot_init`/`tss_init`/`prot_load_selectors`/`idt_init`/`idt_copy_vectors`/`int_gate`、`pg_utils.c` 的 `alloc_lowest` 节选/`pg_clear`/`pg_identity`/`pg_load`/`pg_mapkernel`、`pre_init.c` 的恒等 `vir2phys`/`return &kinfo`/`cbi->kmess = &kmess`、`protect.c` 的减偏移 `vir2phys`、earm 的 `prot_init`/`tss_init`、`arch_finish_switch_to_user` 节选、`mpx.S` 的 `ipc_entry_syscall_percpu` 宏与 AP 入口、`klib.S` 的 `ia32_msr_write` 参数去向，均与 `minix3/` 原文一致（注意：逐字一致不等于解释正确，§2.1 与 §2.6 的语义错误见 FACT-01/02/08）。Rust 注释引文的失效项是 FINDING RD3-FACT-04。
- 命令读数复算一致：`grep -c '#if' i386/protect.c` → 0；`gate_table_pic` 的 `hwint` → 16；`gate_table_exceptions` 的 `_VECTOR` → 23、`USER_PRIVILEGE` → 6；`grep -rn "int kernel_may_alloc"` → 三行且行号 76/38/33 全对；`printf '%d\n' $((6+1))` → 7；`grep -n "const IDT_ENTRIES"` → 114:256；`STUB_VECTORS` 计数 → 41；9 文件 `#[test]` 计数 → 16/13/11/2/1/3/2/1/7（合计 56）。
- C 常量/结构：`IDT_SIZE=256`、`DESC_SIZE=8`、`TSS_INDEX_FIRST=6`、`GDT_SIZE=TSS_INDEX(CONFIG_MAX_CPUS)`、`CONFIG_MAX_CPUS=1`、`SEG_SELECTOR(i)=i*8`、`LDT_INDEX=5`、`X86_STACK_TOP_RESERVED=2*sizeof(reg_t)`、`ARM_STACK_TOP_RESERVED=2*sizeof(reg_t)`、`TSS_TYPE=AVL_286_TSS|DESC_386_BIT`、`SEGMENT=0x10`（S 位第 4 位）、`DPL=0x60`/`DPL_SHIFT=5`、`IRQ0_VECTOR=0x50`/`IRQ8_VECTOR=0x70`；`struct tss_s` 27 字段；`struct gate_table_s` 三字段无 IST；`struct gatedesc_s` 五字段。均与文档一致。
- Rust 常量/签名：`GDT_USER_ANCHOR_INDEX=3`、`GDT_USER_DS_INDEX=4`、`GDT_USER_CS_INDEX=5`、`GDT_TSS_FIRST_INDEX=6`、`GDT_SLOTS_PER_TSS=2`、`MAX_CPUS=8`、`GDT_ENTRIES=22`、`TSS64_SIZE=104`、`X86_64_STACK_TOP_RESERVED=16`、访问字节 `0x9A/0x92/0xFA/0xF2`、`STAR=(KERN_CS<<32)|(ANCHOR<<48)`、`MSR_SFMASK=0xC0000084`、`SFMASK_CLEAR_IF=0x200`、`EFER_SCE=1`、`IST_STACK_SIZE=0x2000`；`GsArea` 三字段及 `gs:0x0/0x8/0x10` 偏移；`ProtectionArch` 的 `init/set_kernel_stack/refresh_descriptors/program_entry_base/load/init_ap` 与 `TrapEntryArch` 的 `init/configure_syscall/load/load_ap/set_handler` 均存在。文档表述与源码一致。
- 测试名核对：`tss64_size_is_104_bytes`、`tss64_offsets_correct`、`gdt_descriptors_have_correct_dpl`、`gdt_descriptors_are_flat_mode`、`tss_descriptor_is_64bit`、`tss_descriptor_high_half_carries_base_above_4gib`、`init_creates_tss_descriptor_in_gdt`、`set_kernel_stack_updates_sp0`、`init_sets_tss_sp0_below_reserved_area`、`test_stub_coverage_matches_idt_gates`、`test_errcode_classification_matches_sdm`、`privilege_level_roundtrip`、`protection_has_cpu_count`、`aarch64_trap_entry_arch_impl_satisfies_trait_bound`、`riscv64_trap_entry_arch_impl_satisfies_trait_bound` 全部在 `os/arch/src/` 中存在且归属文件与文档一致。
- 阴性声明核对：`grep -c '#if' i386/protect.c = 0`、`add_memmap`/`FREE_MEMMAP` 在 `os/kernel/src` 无实现、`protect.o` 不在改名名单、`ls minix3/minix/kernel/arch` 只有 i386/earm、`tss_init` 五个调用点全部丢弃返回值、`load()` 只有 `lidt` 不检查门、x86_64 真机载体不调 `install_trap_stubs`、aarch64 载体不调 `init_protection`/`CurrentProtection`/`CurrentTrapEntry`、SP_EL1 回环无写——均成立。唯一不成立的阴性声明是 :964 的 “`kernel_may_alloc()` grep 零命中”（见 FACT-03）。
- 真机载体：三份 `test-protection*/src/main.rs` 的寄存器判据、`### TEST_RESULT: PASS test-protection[-aarch64|-riscv64] ###` 字符串、`run_all.sh` 的三架构清单、`run_qemu.sh` 的 `### TEST_RESULT: PASS` 前缀匹配均与文档一致；aarch64 载体确实不调 `init_protection`/`CurrentProtection`/`CurrentTrapEntry`，SP_EL1 只读回环中代码只有一条 `mrs x0, SP_EL1`，无 `msr SP_EL1` 写。
