# D03 R1  certify 事实回读（03-kmain-cstart）

- 文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/03-kmain-cstart.md`
- 行数：1384
- sha256：`bf055daf0fb3d950cab51e81463651c5671c0c1df2d15aeea3023f3e1bf7e82f`（与任务给的 `bf055daf` 一致）
- 状态：工作区已修改（`git diff` 基准 `8ab89133e`，本文件即本轮改动）；27 行新增 / 25 行删除
- 只读范围：本轮 diff 的 23 个 hunk 及其上下邻句/邻行。没有读历史评审、其他面板、`expected/`；没有改动文档。
- 方法：逐条把文档断言对回 `minix3/` 的 C 源与 `os/` 的 Rust 源，命令与输出随条附出。

## 0. 五道文档门（L1 工具输出）

| 门 | 命令 | 输出（摘要） | 退出码 |
|---|---|---|---|
| doc-citation-check | `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/03-kmain-cstart.md` | `blocks=42 checked=0 failed=0 skipped=42 warned=0` | 0 |
| anchor-resolve | `bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md` | `anchors=75 resolved=75 zero-def=0 multi-def=0` | 0 |
| doc-structure-check | `python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/03-kmain-cstart.md` | `doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析`（工具原文以对勾符号开头） | 0 |
| doc-ref-check | `bash tools/doc-ref-check.sh rewrite-notes/01-stage-kernel/03-kmain-cstart.md` | `doc-ref-check: ... 内部章节引用均可解析`（同上，工具原文以对勾符号开头） | 0 |
| doc-code-map | `bash tools/doc-code-map.sh --check rewrite-notes/01-stage-kernel/03-kmain-cstart.md` | 21 个文件，存在性 21/21 通过 | 0 |

五门全过，但第一门的覆盖是零：`checked=0 / skipped=42`。原因是本文件的代码块标注统一写成 `/* 代码引用，… */`，而工具的正则只认行首 `//`（`tools/doc-citation-check.py` 的 `ANNOTATION_RE`）。同一工具在 `01-boot-shim-bootstrap.md` 上是 `checked=29`、在 `02-higher-half-kernel.md` 上是 `checked=1`，说明这不是工具坏了，是本文的标注风格让逐行对账整体空转。见 FINDING RD3-CERT-FACT-05。

## 1. 逐条核对

### 1.1 §2.1 两行 memcpy 的“暂存副本 → 最终全局”，以及删掉后的失败点 — REOPEN

**结论（搬运方向）：成立。** 三段证据都对得上：

- `minix3/minix/kernel/arch/i386/Makefile.inc:51-69` 的 `unpaged_obj` 名单含 `pre_init.o` 与 `pg_utils.o`，逐个经 `objcopy --prefix-symbols=__k_unpaged_` 改名；
- `minix3/minix/kernel/arch/i386/kernel.lds:19` 是 `.unpaged_bss ALIGN(4096) : { unpaged_*.o(.bss COMMON) }`，位置在 `kernel.lds:22` 的 `. += _kern_offset;` 之前，因此 VMA 等于物理地址；
- `minix3/minix/kernel/usermapped_data.c` 定义 `struct kinfo kinfo __section(".usermapped");` 与 `struct kmessages kmessages __section(".usermapped");`，`main.c` 经 `glo.h:22/24` 的普通 `extern` 解析到这两份；
- `minix3/minix/kernel/arch/i386/pre_init.c:20-21` 定义未分页侧的 `kinfo`/`kmessages`，`:237` 是 `return &kinfo;`，`head.S:86-87` 把 `%eax` 压栈后 `call kmain`。

所以 `kmain` 的 `local_cbi` 拿到的是 `&__k_unpaged_kinfo`（`pre_init` 侧的暂存副本），两行 `memcpy` 是低地址到高半的真实搬运，不是自拷贝。

**但同一句的末一个分句写反了**：文档第 141 行说“`kmain` 的 `local_cbi` 指向的也是这份最终全局”。按上面的调用链，`local_cbi` 指向的正是 `.unpaged_bss` 的暂存副本，不是 `.usermapped` 最终全局；写成“也是最终全局”会让本段自己的论点（不是自拷贝）当场自相矛盾。见 FINDING RD3-CERT-FACT-01。

**结论（删掉的后果）：方向成立，落点写错。** 删掉两行后 `.usermapped` 的 `kinfo` 保持全零，`kinfo.mbi.mi_mods_count` 为 0，`NR_BOOT_MODULES` 为非零，两者不等——这一层判断是对的。但文档第 179 行写的“先炸”位置是错的：

- `minix3/minix/kernel/main.c:160-162` 的实际内容是 `if(NR_BOOT_MODULES != kinfo.mbi.mi_mods_count) panic("expecting %d boot processes/modules, found %d", …)`，是 `panic` 不是 `assert`，标识符是 `mi_mods_count` 不是 `mods_with_kernel`；
- `mods_with_kernel` 在 `main.c` 里一次都没出现（`grep -rn mods_with_kernel minix3/minix/kernel/main.c` 无输出）；带 `> 0` 的那条断言在 `minix3/minix/servers/vm/main.c:452`（`assert(kernel_boot_info.mods_with_kernel > 0);`），且在用户态 VM 里，内核走不到；
- 真正的第一处失败在更早的位置：`kmain` 在 `main.c:147` 调 `cstart()`，`main.c:403` 起的 `cstart` 第一句就是 `main.c:411` 的 `prot_init()`；`prot_init` 调 `pg_identity(&kinfo)`（`arch/i386/protect.c:361`），而 `minix3/minix/kernel/arch/i386/pg_utils.c:170` 是 `assert(cbi->mem_high_phys);`。`kinfo` 全零时这条断言先炸；内核的 `assert` 是 `minix3/minix/lib/libsys/assert.c` 的 `__assert13`，最终 `panic("assert failed")`，`NDEBUG` 在整个 `minix3/minix/kernel` 与 `libsys` 里没有被定义。

也就是说：删掉两行确实会挂，但挂点是 `pg_utils.c:170` 的 `assert(cbi->mem_high_phys)`（经 `cstart` → `prot_init` → `pg_identity`），不是 `main.c:160-162` 的 boot module 计数 panic。见 FINDING RD3-CERT-FACT-02。

### 1.2 §2.1 `kernel_may_alloc` 的两份对象 — CLOSED

- `minix3/minix/kernel/glo.h:5-12`：只有看到 `_TABLE` 时才把 `EXTERN` 改成空串；`glo.h:76` 是 `EXTERN int kernel_may_alloc;`。`minix3/minix/kernel/table.c:27` 正是 `#define _TABLE`，于是普通侧的唯一零初始化定义落在 `table.c` 的 BSS。
- `minix3/minix/kernel/arch/i386/pre_init.c:33` 是 `int kernel_may_alloc = 1;`（earm 侧 `:38` 同形），该对象随 `pre_init.o` 改名进 `.unpaged_bss`。
- 普通副本的写入点只有两处：`main.c:142` 置 1、`main.c:105`（`bsp_finish_booting`）置 0。所以“进入 `kmain` 时是 0，`kmain` 这一行是置 1 的唯一动作”成立，是承重赋值。
- 读者是 `pg_utils.c:42/74/102/143/274` 的 `assert(kernel_may_alloc)`；未分页副本的这五条断言经改名解析到 `pre_init.c` 的初值 1，普通副本解析到 `table.c` 的 BSS，与文档的两侧分工一致。

文档里“`glo.h` 的 `EXTERN int` 在普通编译单元里展开成零初始化的 BSS 定义”这一句，字面上容易被读成“每个普通编译单元都生成存储”，准确说法是“只有 `table.c`（`_TABLE`）生成存储，其余编译单元展开为 `extern` 声明”。括注已经点名 `table.c`，语义不误导，按 CLOSED 处理，不另立条目。

### 1.3 §2.6 双副本 / 两张 pagedir / 切 CR3 / 两份 vir2phys / 删掉原地重填与 TLB 解释 — 正文 CLOSED，但整篇口径 REOPEN

已核对的事实链，全部成立：

- `minix3/minix/kernel/arch/i386/pg_utils.c:19` 是 `static u32_t pagedir[1024] __aligned(4096);`，`pg_utils.c` 同时出现在 `Makefile.inc` 的普通 `SRCS` 与 `unpaged_obj` 名单里，所以物理上确有两份 `pagedir`：一份在 `.unpaged_bss`（VMA=PA），一份在 `.bss`（高半，`kernel.lds:32`）。
- `pre_init` 的四个调用（`pre_init.c:230-233` 的 `pg_clear`/`pg_identity`/`pg_mapkernel`/`pg_load`）与 `prot_init` 的四个调用（`protect.c:360-363`）同名但绑定到不同对象：`pre_init.o`、`pg_utils.o` 都在改名名单里，`protect.o` 不在。因此 `prot_init` 清空/建表/装载作用在普通副本上，`pg_load` 用 `vir2phys(pagedir)` 算出这张新表的物理地址写 CR3（`pg_utils.c:247-251`）。
- 两份 `vir2phys` 的绑定也对：未分页侧是 `pre_init.c` 的恒等版，普通侧是 `protect.c:32-39` 的减偏移版；`pg_load` 的引用随对象走。
- 本轮删掉的“清零与重填之间不写 CR3、靠 TLB 旧条目继续取指”这条机制，本来就不成立（重建对象不是当前在用的表），删除是对的；`§3.5`（第 895 行）同步改成“不是在原表上重填”，也对。
- 反事实一段（第 669 行）的“普通副本的 `pagedir` 在 `.bss` 冷启动为零，`pg_load()` 切 CR3 后立即缺页/三重故障，不会照常跑”成立；前一支“删掉 `pg_identity` 后第一个物理地址访问缺页”也成立。
- 顺带核对了本轮改写的 PWT/PCD 细节：`pg_utils.c:173-183` 用 `I386_VM_ADDR_MASK_4MB`（`minix3/minix/include/arch/i386/include/vm.h:19` 的 `0xFFC00000`）做 `<=` 比较，未对齐时最后一块真实内存被一起标记、对齐时标记的是顶端之上第一块——新表述准确。

**REOPEN 的原因不在 §2.6/§3.5 本体，而在 §5.4 的 M9 行没有跟着改**：第 1275 行仍写“重建页表的**原地重填**（§2.6，C 侧）｜清零→重填窗口内取指可行（**TLB 旧条目**）”，与本轮刚确立的两副本结论直接冲突，也与它自己引用的 §2.6 冲突。见 FINDING RD3-CERT-FACT-03。

### 1.4 §3.5 Step 1 现行注释与 `BOOT_IDENTITY_MAP_END` 重导出 — CLOSED

- `os/arch/src/lib.rs:217` 确为 `pub const BOOT_IDENTITY_MAP_END: u64 = 0x1_0000_0000;`。
- `os/kernel/src/lib.rs:79` 是 `use minix_arch::{ BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END, … };`，`IDENTITY_MAP_END` 在 kernel crate 内处处可用（`dm_coverage.rs:424` 的 `use crate::IDENTITY_MAP_END;` 即此）。
- Step 1 的现行注释（`os/kernel/src/lib.rs:1222-1230`）原文是“C maps 4 GiB with `pg_identity` because the 32-bit kernel runs inside that window; the Rust cross-image leg only needs its shim image/stack, the kernel LMA span, the bootstrap pool …, the firmware tables …, and the device windows …”，与文档第 891 行的引述一致，“不再是 1024×4MB 全量映射”的判断成立。

术语上，“重导出”对应的是 `pub use`；此处是带改名的私有导入（同一文件的 `pub use minix_arch::TrapStyle as PublicTrapStyle;` 才是重导出）。事实结论不受影响，按 CLOSED 处理，术语见 FINDING RD3-CERT-FACT-06（P2）。

### 1.5 §4.1 / §4.7 的 `KERNEL_MAY_ALLOC`、§4.4 三个读数、§5 计数 — CLOSED

- `grep -rn "KERNEL_MAY_ALLOC" os/kernel/src os/arch/src` 只剩 `os/kernel/src/lib.rs:3701/3706/3916` 三处注释，`os/kernel/src/globals.rs` 里没有该静态量；`lib.rs:3916-3919` 的 Step 8 注释原文与文档第 965 行的引述一致（“该开关唯一的读者是已删除的 add_memmap 断言；账本换成 FreeRangeSet 后……不需要运行期布尔”）。`add_memmap` 在 `os/` 下已无任何调用点（只有 `lib.rs:1859`、`lib.rs:2827` 两处历史注释提到它），所以“`init_proc_and_boot` 与 Phase F 都没有 `add_memmap` 调用”成立。
- §4.4 的“三个读数”：`os/kernel/src/lib.rs:1699-1712` 的诊断块实际只打印 `rdi`、`kinfo=`（全局槽指针）、`rsp=` 三个值；注释里仍提到 `bootstrap_len`，但代码没有输出。从“四个”改成“三个”与实际一致。
- §5.1 计数：`for f in …; do grep -c '#\[test\]' …` 对 9 个文件的结果与文档第 1212-1220 行逐行相同，合计 16+13+11+2+1+3+2+1+7 = 56。全量口径 `grep -rh '#\[test\]' os/arch/src --include=*.rs | wc -l` = 374，`grep -rl` = 54 个含测试的文件；`os/arch/tests/*.rs` 为 21 个文件 / 95 条测试（`grep -rc` 求和 = 95）。三个数字都复现。

唯一瑕疵是 `src/*.rs` 这个写法本身：顶层 `os/arch/src/*.rs` 只有 3 个文件，374/54 只有在递归口径下才复现。见 FINDING RD3-CERT-FACT-08（P2）。

### 1.6 §2.4 IOPL、§3.7 门数、§1.2/§4.6 入口拆行 — 前两项中“门数”的归因 REOPEN，其余 CLOSED

- **IOPL 可授予**：`minix3/minix/kernel/arch/i386/protect.c:51` 是 `pp->p_reg.psw |= 0x3000;`（IOPL 两位，值 3），调用链为 `do_iopenable.c:28` 的 `enable_iop(proc_addr(proc_nr))`，用户侧入口 `minix3/minix/lib/libsys/arch/i386/sys_eniop.c`，实例调用者见 `minix3/minix/drivers/storage/memory/memory.c:370`。TSS 的 `iobase = sizeof(struct tss_s)`（`protect.c:170`）确实落在限长（`init_param_dataseg` 折出的 `sizeof - 1`，`protect.c:163`）之外，默认拒绝 CPL>IOPL 的 `in`/`out` 成立；加上 IOPL=3 豁免后“不是绝对全部被拒”也成立。
- **入口拆行**：`§1.2` 新行（第 46 行）与 `§4.6` 新行（第 1180 行）的四格逐一成立：C 是 `SYSENTER_*`/`STAR`+`EFER` 的 MSR 路径（`protect.c:185-212`），Rust x86-64 是 SYSCALL/SYSRET + `LSTAR`（`trap_entry.rs` 的 `configure_syscall`），aarch64 的 `svc` 走 `VBAR_EL1`，riscv64 的 `ecall` 走 `stvec`。
- **门数 39 − 16 + 64 + 3/5 = 90/92**：总数对，归因错。
  - 总数复算：`protect.c` 的 `gate_table_pic` 16 条 + `gate_table_exceptions` 23 条 = 39；`apic.c:764-833` 的 `gate_table_ioapic` 是 `LAPIC_VECTOR(0..63)`（`apic.c:96` 的 `LAPIC_VECTOR(irq) = IRQ0_VECTOR + irq`，`interrupt.h:17` 的 `IRQ0_VECTOR = 0x50`，即 0x50–0x8F 共 64 条）外加 0xFF spurious 与 0xFE error 两条；`apic.c:913` 另装 0xF0 定时器门；`apic.c:842-846` 的 `gate_table_smp` 在 `CONFIG_SMP` 下再装 0xF1/0xF2 两条。于是 APIC 非 SMP = 39−16+64+3 = 90，APIC+SMP = 92。数字与文档一致。
  - 但括注“3 或 5 取决于是否计入两条错误向量”不成立：0xFF/0xFE 已经在“再加 spurious/错误向量”与那个 `+3` 里，3 与 5 的差是 `CONFIG_SMP` 的两条 IPI 门（0xF1 调度、0xF2 halt）。见 FINDING RD3-CERT-FACT-04（P1）。

## FINDING RD3-CERT-FACT-01（P0）

- 位置：文档第 141 行（§2.1 第二个动作）。
- 原文：“`main.c` 里的 `kinfo`/`kmessages` 是 `usermapped_data.c` 定义的 `.usermapped` 最终全局，**`kmain` 的 `local_cbi` 指向的也是这份最终全局**。”
- 事实：`head.S:86-87` 把 `pre_init` 的返回值 `%eax` 直接压栈给 `kmain`，而 `pre_init.c:237` 返回的是 `&kinfo`；由于 `pre_init.o` 在改名名单里，这个 `kinfo` 是 `__k_unpaged_kinfo`，落在 `.unpaged_bss`。`local_cbi` 指向的是暂存副本，不是 `.usermapped` 最终全局。若真的指向最终全局，这两行就是自拷贝，本段“不是自拷贝”的论点不成立。
- 影响：直接推翻本段的核心论据，属于“描述与 C 事实不符”的 P0-fact。
- 建议修复：改为“`main.c` 里的 `kinfo`/`kmessages` 是 `usermapped_data.c` 定义的 `.usermapped` 最终全局；`kmain` 的 `local_cbi` 指向的是 `pre_init` 那份 `.unpaged_bss` 暂存副本（`pre_init` 返回 `&__k_unpaged_kinfo`，`head.S` 把该返回值压栈传入）”。
- 证据：`minix3/minix/kernel/arch/i386/head.S:77-88`、`arch/i386/pre_init.c:20-21/237`、`kernel/usermapped_data.c:7`、`arch/i386/Makefile.inc:51-69`、`arch/i386/kernel.lds:19-22`。

## FINDING RD3-CERT-FACT-02（P0）

- 位置：文档第 179 行（§2.1 反事实段）。
- 原文：“启动会在 boot module 计数检查处失败（`main.c:160-162` 的 `assert(m->mods_with_kernel > 0)` 一类断言先炸）”。
- 事实三层：
  1. `main.c:160-162` 是 `if(NR_BOOT_MODULES != kinfo.mbi.mi_mods_count) panic("expecting %d boot processes/modules, found %d", NR_BOOT_MODULES, kinfo.mbi.mi_mods_count);`，是 `panic` 不是 `assert`，比较对象是 `mi_mods_count` 不是 `mods_with_kernel`；`mods_with_kernel` 在 `main.c` 中零出现。
  2. 带 `> 0` 的断言在 `minix3/minix/servers/vm/main.c:452`（`assert(kernel_boot_info.mods_with_kernel > 0);`），属用户态 VM，内核 panic 之前走不到。
  3. 真正先炸的是 `minix3/minix/kernel/arch/i386/pg_utils.c:170` 的 `assert(cbi->mem_high_phys);`：`kmain` 在 `main.c:147` 调 `cstart()`，`cstart` 的第一句是 `prot_init()`，`prot_init` 调 `pg_identity(&kinfo)`，`kinfo` 全零时 `mem_high_phys` 为 0。中间的 `main.c:133`（`env_get`/`get_board_id_by_name`）读的是全零 `param_buf`，即使走到 `strncmp(NULL, …)` 也不会缺页——`pre_init` 建的恒等映射覆盖 0–4GB（`pg_utils.c` 的 `pg_identity` 把 1024 个 PDE 全填上）。内核 `assert` 实现在 `minix3/minix/lib/libsys/assert.c`，最终 `panic("assert failed")`；`NDEBUG` 未定义。
- 影响：删两行确实会挂的结论对，但“先炸”的位置与引用的代码都错；读者按锚点找不到被引用的断言。属于 C 代码引用错误（P0-fact）。
- 建议修复：改为“……启动会在 `cstart` → `prot_init` → `pg_identity` 的 `assert(cbi->mem_high_phys)`（`i386/pg_utils.c:170`）上先炸；即使越过这一步，`main.c:160-162` 的 `panic("expecting %d boot processes/modules, found %d", …)`（`NR_BOOT_MODULES` 对 `kinfo.mbi.mi_mods_count`）也会拦下。`servers/vm/main.c:452` 的 `assert(... mods_with_kernel > 0)` 属用户态 VM，走不到。”
- 证据：`minix3/minix/kernel/main.c:128-162/147/403-411`、`arch/i386/protect.c:321/360-363`、`arch/i386/pg_utils.c:162-181`、`minix3/minix/lib/libsys/assert.c:15-24`、`minix3/minix/servers/vm/main.c:452`、`minix3/minix/include/minix/param.h:14-24`。

## FINDING RD3-CERT-FACT-03（P0）

- 位置：文档第 1275 行（§5.4 反转核对表 M9 行）。
- 原文：“M9 ｜ 重建页表的**原地重填**（§2.6，C 侧）｜ 清零→重填窗口内取指可行（**TLB 旧条目**）；`pg_load` 后表内容齐全”。
- 事实：本轮 §2.6（第 615 行）与 §3.5（第 895 行）已经确立：`pre_init` 在未分页副本的 `pagedir` 上建第一张表，`prot_init` 在普通副本的 `.bss pagedir` 上建第二张表再切 CR3，“不是同一数组的原地重填”。既然被清零/重填的表不是当前在用的那张，就不存在“靠 TLB 旧条目在清零→重填窗口内取指”的机制。M9 与同篇新结论、与其自身引用的 §2.6 三方冲突。
- 影响：同一文档对同一机制给出互斥陈述；M9 还挂着 R03-T07 的登记，读者会以为有一个被验证过的“原地重填”行为。属于 P0-fact（虚构 C 侧机制）。
- 建议修复：把 M9 改为“重建页表的对象切换（§2.6，C 侧）｜ 清零/建表/装载作用在非在用的普通副本 `pagedir` 上；`pg_load` 切 CR3 后新表内容齐全”，删掉“原地重填”与“TLB 旧条目”；R03-T07 的“登记，无动作”可保留（它对应的“C 侧不在本仓测试面”仍然成立）。
- 证据：`rewrite-notes/01-stage-kernel/03-kmain-cstart.md:615/669/895/1275`、`minix3/minix/kernel/arch/i386/pg_utils.c:19/247-256`、`arch/i386/protect.c:360-363`、`arch/i386/kernel.lds:19/32`。

## FINDING RD3-CERT-FACT-04（P1）

- 位置：文档第 947 行（§3.7 门数对照）末句。
- 原文：“该对照只对 `prot_init`/`idt_init` 完成时成立；APIC/SMP 路径用 `gate_table_ioapic` 的 64 条门（0x50–0x8F）替换 16 条 PIC 门，再加 spurious/错误向量：39 − 16 + 64 + 3/5 = 90/92（**3 或 5 取决于是否计入两条错误向量**）”。
- 事实：90/92 的总数正确，但 3 与 5 的差不是“两条错误向量”。APIC 模式下的 +3 已经包含 0xFF spurious、0xFE error 与 0xF0 APIC 定时器；再计入 `CONFIG_SMP` 的两条 IPI 门（0xF1 `APIC_SMP_SCHED_PROC_VECTOR`、0xF2 `APIC_SMP_CPU_HALT_VECTOR`）才是 +5。按现有括注，读者会重复计入 0xFF/0xFE，算不出 90/92。
- 影响：公式结论对、归因错，属依据错误（P1）；若按“虚构事实”从严，可升级为 P0-fact。
- 建议修复：改为“……再加 spurious（0xFF）、error（0xFE）与 APIC 定时器（0xF0）三条：39 − 16 + 64 + 3 = 90；`CONFIG_SMP` 再装 0xF1/0xF2 两条 IPI 门，得 92”。
- 证据：`minix3/minix/kernel/arch/i386/protect.c:107-153`（16 + 23 条）、`arch/i386/apic.c:96/764-846/892/913`、`arch/i386/apic.h:90-94`、`minix3/minix/include/arch/i386/include/interrupt.h:17`。

## FINDING RD3-CERT-FACT-05（P2）

- 位置：全文 42 个 fenced code block 的引文标注。
- 事实：`python3 tools/doc-citation-check.py` 输出 `blocks=42 checked=0 failed=0 skipped=42 warned=0`。工具只识别行首 `//` 形式的标注（`ANNOTATION_RE = ^\s*//\s*(代码引用|节选|…)`），本文全部写成 `/* 代码引用，… */`，因此逐行子序列对账一条也没跑。对照：`01-boot-shim-bootstrap.md` 是 `checked=29`，`02-higher-half-kernel.md` 是 `checked=1`。
- 影响：这道门对本文当前是空转，等于没有 L1 证据。不是本轮引入，但本轮在改动段里新增/改写了引述，值得一并处理。
- 建议修复（二选一）：把代码块首行标注改成 `// …` 风格（与 01/02 篇一致）；或扩展 `tools/doc-citation-check.py` 的 `ANNOTATION_RE` 接受 `/* */`（需同时跑 `--self-test`）。
- 证据：上述命令输出；`tools/doc-citation-check.py:42-44`。

## FINDING RD3-CERT-FACT-06（P2）

- 位置：文档第 891 行。
- 原文：“`IDENTITY_MAP_END` 是 `BOOT_IDENTITY_MAP_END` 的**重导出**”。
- 事实：`os/kernel/src/lib.rs:79` 是 `use minix_arch::{ BOOT_IDENTITY_MAP_END as IDENTITY_MAP_END, … };`，私有导入加改名，不是 `pub use`；它对本 crate 的子模块可见（`dm_coverage.rs:424` 的 `use crate::IDENTITY_MAP_END;` 靠的是“子模块可见父模块私有项”），但对外不构成导出。同文件 `pub use minix_arch::TrapStyle as PublicTrapStyle;` 才是重导出。
- 影响：术语层面的小偏差，不影响“常量定义在 `os/arch/src/lib.rs:217`”这一结论。
- 建议修复：把“重导出”改为“导入并改名（`use … as`）”。
- 证据：`os/kernel/src/lib.rs:79/85`、`os/arch/src/lib.rs:217`、`os/kernel/src/dm_coverage.rs:424`。

## FINDING RD3-CERT-FACT-07（P2）

- 位置：文档第 144 行（代码块标注）与第 965 行（§4.1 首段）。
- 事实两处：
  1. 第 141 行已改口为“不是自拷贝”，但紧邻的代码块标注仍写“教学简化（只保留与**自拷贝论证**有关的四行…）”，读者会以为这一段在为自拷贝作证。
  2. 第 965 行写“随后**两步各有 C 侧对应**：`minix_platform::init_from_kinfo`（…）；C 的 `kernel_may_alloc` 闸门已随 D01-F08 淘汰（…）没有对应物”——同一句先说“各有对应”，再说其中一项“没有对应物”，自相抵消。
- 影响：本轮改写后残留的旧措辞，不涉及事实错误，但会把两处新结论读反。
- 建议修复：第 144 行改成“教学简化（保留与两份副本论证有关的四行…）”；第 965 行改成“随后两步：`minix_platform::init_from_kinfo` 对应 C 的平台描述符；另一处 `kernel_may_alloc` 闸门在 Rust 侧没有对应物（D01-F08）”。
- 证据：文档第 141/144/965 行本身；`os/kernel/src/lib.rs:3916-3919`。

## FINDING RD3-CERT-FACT-08（P2）

- 位置：文档第 1223 行。
- 原文：“arch crate 全量 `src/*.rs` 是 374 个测试/54 文件”。
- 事实：`ls os/arch/src/*.rs` 只有 3 个文件（`lib.rs` 等顶层文件），374 条测试与 54 个文件只有在递归口径下才复现：`grep -rh '#\[test\]' os/arch/src --include=*.rs | wc -l` = 374，`grep -rl '#\[test\]' os/arch/src --include=*.rs | wc -l` = 54（全部 `.rs` 文件是 87 个）。下一分句的 `os/arch/tests/*.rs` 是真平铺（21 个文件），两句并置会让人误以为 `src` 也是平铺。
- 影响：数字对、路径口径不可复现，属改善性（P2）。
- 建议修复：写成“arch crate 全量 `os/arch/src` 下的 `.rs`（递归）含 374 条 `#[test]`，分布在 54 个文件”。
- 证据：上述三条计数命令的输出。

## 未准入的观察（不计入新准入）

- `os/kernel/src/lib.rs:2827` 的注释仍写“the VM module's physical memory is reclaimed via `add_memmap` (undoes the `cut_memmap` …)”，与 D01-F08 之后“`add_memmap` 已无定义/无调用”冲突。这是 Rust 代码注释的陈旧，不是文档事实错误；本文件第 965 行的“没有 `add_memmap` 调用”判断成立。
- 本轮未跑 QEMU 真机链（任务只要求五道文档门），§5.2/§5.4 的“真机读数”类断言只做了源码侧核对，没有寄存器实测证据。

## 结论

| 核对项 | 结论 |
|---|---|
| 1. §2.1 两行 memcpy 方向 | CLOSED |
| 1. §2.1 删两行的失败点 | REOPEN（RD3-CERT-FACT-02） |
| 1. §2.1 `local_cbi` 归属 | REOPEN（RD3-CERT-FACT-01） |
| 2. §2.1 `kernel_may_alloc` 两份对象 | CLOSED |
| 3. §2.6 双副本/两张表/切 CR3/两份 `vir2phys` | CLOSED |
| 3. 删掉原地重填与 TLB 解释（全篇口径） | REOPEN（RD3-CERT-FACT-03，§5.4 M9 未同步） |
| 4. §3.5 Step 1 注释与常量来源 | CLOSED（术语见 RD3-CERT-FACT-06） |
| 5. §4.1/§4.7 `KERNEL_MAY_ALLOC` 已删 | CLOSED |
| 5. §4.4 三个读数 | CLOSED |
| 5. §5 计数 374/95-21 | CLOSED（口径见 RD3-CERT-FACT-08） |
| 6. §2.4 IOPL 可授予 | CLOSED |
| 6. §3.7 门数总数 | CLOSED（90/92 复算一致） |
| 6. §3.7 3/5 归因 | REOPEN（RD3-CERT-FACT-04） |
| 6. §1.2/§4.6 入口拆行 | CLOSED |

certify 新准入 = 8（P0 3：RD3-CERT-FACT-01/02/03；P1 1：RD3-CERT-FACT-04；P2 4：RD3-CERT-FACT-05/06/07/08）
