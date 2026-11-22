# P-REFUTE · round1 · D02 higher-half kernel

角色：独立反证者。只读仓库；未读作者结论、其他角色报告与历史 panel。
真值基线：`minix3/` C 源 > 设计契约 > Rust 实现 > 文档。
取证方式：直接读源码/脚本；命令均给出可复算形式。证据强度按 L1（命令输出）与 L2（手工 grep/逐行阅读）标注；凡依赖已构建工件或"本轮实跑"的断言，一律不给 CONFIRM。

---

## CLAIM 01

**主张**（§3.4）："`Paging` 本身已有 24 个成员（`const PAGE_SIZE` 与 23 个方法：`phys_addr_limit`、`validate_virtual_range`、`new`、`new_from_page`、`from_active_root`、`inherit_supervisor_half`、`enable`、`destroy`、`is_supervisor_va`、`is_inherited`、`ensure_writable_range`、`map`、`remap`、`unmap`、`update_flags`、`query`、`split_huge`、`grant_user_walk`、`root_paddr`、`flush_tlb`、`flush_tlb_addr`、`map_range`、`unmap_range`）。"

**反证构造**：逐条清点 `os/arch/src/arch/paging.rs` 的 `pub trait Paging` 块（第 212 行起），把 `const` 与 `fn` 分别计数，再与文档列出的 23 个名字做集合比对。

**取证**：`awk '/pub trait Paging/{f=1} f{print NR": "$0} f&&/^}/{exit}' os/arch/src/arch/paging.rs` 输出成员为：`const PAGE_SIZE`；`phys_addr_limit`、`validate_virtual_range`、`new`、`new_from_page`、`from_active_root`、`inherit_supervisor_half`、`enable`、`destroy`、`is_supervisor_va`、`is_inherited`、`ensure_writable_range`、`map`、`remap`、`unmap`、`update_flags`、`query`、`split_huge`、`grant_user_walk`、`root_paddr`、`flush_tlb`、`flush_tlb_addr`、`map_range`、`unmap_range`。方法计数 23，加 `const` 为 24；名字与文档枚举逐一相同，无缺项、无多项。

## VERDICT

CONFIRM（L1）。唯一保留：`split_huge`/`grant_user_walk`/`inherit_supervisor_half` 是带默认体的方法，但仍是 trait 成员，计入 23 不改变结论。

---

## CLAIM 02

**主张**（§3.2）："解析器放在 `os/libs/minix-elf`，是一个独立 crate，被五个 crate 依赖"，并列出 `os/arch`、`os/boot-shim`、`os/servers/vfs`、`os/servers/vm`、`os/servers/rs` 五处。

**反证构造**：全仓搜 `minix-elf =` 的 Cargo 依赖行，检查是否有文档未列出的第六个消费者，或列出的任一处并不存在。

**取证**：`grep -rl 'minix-elf =' os/*/Cargo.toml os/*/*/Cargo.toml` 输出恰为 `os/arch/Cargo.toml`、`os/boot-shim/Cargo.toml`、`os/servers/rs/Cargo.toml`、`os/servers/vfs/Cargo.toml`、`os/servers/vm/Cargo.toml`，计数 5。文档的五行复算命令与结果一致。

## VERDICT

CONFIRM（L1）。注意该 grep 只覆盖两层的 `Cargo.toml`；若未来新增三层深度的消费者，这条命令会漏计，但就当前树而言主张成立。

---

## CLAIM 03

**主张**（§3.2）："`ElfError` 共 12 个变体，按产生层分四组：文件头五道 → `TooShort`、`BadMagic`、`Not64Bit`、`NotLittleEndian`、`NotExecutable`；程序头表两道 → `PhdrOutOfBounds`、`PhdrEntryTooSmall`；迭代器段形两道 → `SegmentOutOfBounds`、`FileszExceedsMemsz`；loader 三道 → `NoLoadSegments`、`InvalidSegment`、`TargetBufferTooSmall`。"

**反证构造**：数 `enum ElfError` 的变体，并检查每个变体实际在哪个层返回，看分组是否与代码位置一致。

**取证**：`sed -n '/pub enum ElfError/,/^}/p' os/libs/minix-elf/src/lib.rs` 列出 12 个变体，名字与文档完全一致；`TargetBufferTooSmall` 是结构体变体，仍计一个。分组与返回点抽检：`TooShort`/`BadMagic`/`Not64Bit`/`NotLittleEndian`/`NotExecutable` 在 `parse_ehdr`；`PhdrOutOfBounds`/`PhdrEntryTooSmall` 在 `SegmentIter::new`/`parse_phdr`/`next`；`SegmentOutOfBounds`/`FileszExceedsMemsz` 在 `next`；`NoLoadSegments`/`InvalidSegment` 在 `compute_kernel_layout`/`validate`，`TargetBufferTooSmall` 在 `load_segments_into_buffer`。

## VERDICT

CONFIRM（L1）。12 与四组 5/2/2/3 均成立。

---

## CLAIM 04

**主张**（§2.2）："名单里显式列了 9 个对象 … 再从三个库的未分页清单里各取一批：`${MINLIB_OBJS_UNPAGED}` 3 个、`${MINC_OBJS_UNPAGED}` 13 个、`${SYS_OBJS_UNPAGED}` 2 个。合计 27 个对象被改名。"

**反证构造**：按文档自己的复算命令数三张清单；再检查 27 个名字里是否有重复（重复会让"27 个对象"变成重复计数），并确认显式名单确实是 9 个。

**取证**：
- `awk '/^MINC_OBJS_UNPAGED=/{f=1} f{print} f&&/udivdi3\.o/{exit}' minix3/minix/kernel/arch/i386/Makefile.inc | grep -oE '[A-Za-z0-9_]+\.o' | wc -l` → `13`。
- `grep -E '^MINLIB_OBJS_UNPAGED=' … | grep -oE '[A-Za-z0-9_]+\.o' | wc -l` → `3`（`_cpufeature.o _cpuid.o get_bp.o`）。
- 同法换 `SYS_OBJS_UNPAGED` → `2`（`assert.o stacktrace.o`）。
- `.for unpaged_obj in head.o pre_init.o direct_tty_utils.o pg_utils.o klib.o utility.o arch_reset.o io_inb.o io_outb.o …` 显式项恰 9 个。
- 去重核对：显式 9 项与三张清单无交集（`klib.o` 在显式项，不在 `MINLIB_OBJS_UNPAGED`），合计 27 个唯一对象名。

## VERDICT

CONFIRM（L1）。9+3+13+2=27 且无重复。

---

## CLAIM 05

**主张**（§2.5 与 §2.6）："i386 侧 `k_initial_stktop` 落在高一侧的通用 `.bss`"；"`earm/mpx.S` 的等价物在 `.data`（`k_initial_stack` 前后有 `.data`/`.bss`/`.data` 切换）"，故 ARM 侧 `k_initial_stack` 落在高半 `.data`；`mpx.o` 不在改名名单，`unpaged_*.o` 通配匹配不到它。

**反证构造**：若 `k_initial_stack` 落在 `.unpaged_bss`/`.unpaged_data`（低侧），文档结论反转。检查三件事：栈标签所在段；`kernel.lds` 的低侧收段通配是否可能收到 `mpx.o`；改名名单是否含 `mpx.o`。

**取证**：
- `grep -n 'k_initial_st\|^\.data\|^\.bss\|^\.text' minix3/minix/kernel/arch/i386/mpx.S`：`.bss`（631）→ `k_initial_stack:`（632）→ `.space K_STACK_SIZE`（633）→ `LABEL(__k_unpaged_k_initial_stktop)`（634）。标签在手写带前缀的宏里，前缀不是归属证据。
- `sed -n '265,272p' minix3/minix/kernel/arch/earm/mpx.S`：`.data`（265）、`.bss`（267）、`.data`（268）、`k_initial_stack:`（270），确认文档描述的三段切换。
- `minix3/minix/kernel/arch/i386/kernel.lds`：`.unpaged_text : { unpaged_*.o(.text) }`、`.unpaged_data … { unpaged_*.o(.data .rodata*) }`、`.unpaged_bss … { unpaged_*.o(.bss COMMON) }`，三者之后才 `. += _kern_offset`；通用 `.bss` 只收 `*(.bss* COMMON)`。`mpx.o` 文件名无 `unpaged_` 前缀，通配不可达。
- `Makefile.inc` 的 `.for unpaged_obj in …` 名单无 `mpx.o`；`L10` 脚本还对此做了机械断言（`grep -qE '(^|[[:space:]\\])mpx\.o'` 命中即 FAIL）。

## VERDICT

CONFIRM（L1 + L2）。两边栈定义所在段都按文档所述；i386 落高半 `.bss`、earm 落高半 `.data` 的推理链闭合。文档额外声明的"栈区不与其他内容重叠仍靠静态阅读"也如实标注为未自动断言。

---

## CLAIM 06

**主张**（§2.1、§6.2）："`kernel.lds` 五处 `AT()`（`.usermapped_glo`、`.usermapped`、`.text`、`.data`、`.bss`）"；"三份生产脚本 `grep -c ': AT('` 各 4；放宽成 `'AT('` 时 x86_64 仍 4、aarch64/riscv64 各 5（多出的一条来自头注释）"。

**反证构造**：数 C 脚本与三份 Rust 脚本的 `AT()` 出现位置，并区分段头表达式与注释文本。

**取证**：
- `grep -c 'AT(' minix3/minix/kernel/arch/i386/kernel.lds` → `5`；逐行看均为段头 `: AT(ADDR(sec) - _kern_offset)`，没有第 6 处。
- `grep -c ': AT(' os/kernel-image/x86_64.ld` → `4`；`aarch64.ld` → `4`；`riscv64.ld` → `4`。
- `grep -c 'AT(' os/kernel-image/x86_64.ld` → `4`；`aarch64.ld` → `5`；`riscv64.ld` → `5`。多出的一处确实是文件头注释里对 `AT()` 的说明（aarch64 在 13–14 行附近，riscv64 在 12 行）。

## VERDICT

CONFIRM（L1）。计数与口径说明都准确。

---

## CLAIM 07

**主张**（§3.5、§4.1）："三份脚本在收口之后各记一个 `__image_end`，并在 `SECTIONS` 之外用两条 `ASSERT` … 一条查 `__image_end` 减首段 VMA 不超过 `KERN_SIZE`（0x400000），另一条查该差值是 2MiB 的整数倍——后者正是'收口语句还在'的机械证据"；x86_64 减 `KERNEL_HIGH_BASE + KERNEL_PHYS_BASE`，aarch64/riscv64 减 `KERNEL_VIRT_BASE`。

**反证构造**：检查每份脚本的 `ASSERT` 条数、被减项与比较对象；若某份缺一条，则"删 ALIGN 会先在链接期失败"的因果链断开。

**取证**：
- `x86_64.ld:116-119`：`ASSERT(__image_end - (KERNEL_HIGH_BASE + KERNEL_PHYS_BASE) <= 0x400000, …)` 与 `ASSERT((__image_end - (KERNEL_HIGH_BASE + KERNEL_PHYS_BASE)) % 0x200000 == 0, …)`。
- `aarch64.ld:117-120`：`ASSERT(__image_end - KERNEL_VIRT_BASE <= 0x400000, …)` 与 `% 0x200000 == 0`。
- `riscv64.ld:125-128`：同形，减 `KERNEL_VIRT_BASE`。
- 三份都在 `.bss` 括号内有 `. = ALIGN(0x200000);`（x86_64:98、aarch64:98、riscv64:105）。

## VERDICT

CONFIRM（L1）。两条 ASSERT 都在 `SECTIONS` 之外，且减数与被减数按文档所述的架构差异书写。

---

## CLAIM 08

**主张**（§5.1）："`os/libs/minix-elf` 的 27 个测试（`grep -c '    fn test_' … 输出 27`）"；"`os/boot-shim` 的 `loader.rs` 部分有 32 个测试（`grep -c '#\[test\]' os/boot-shim/src/loader.rs` 输出 32；`cd os && cargo test -p boot-shim --lib` 在默认特性下共 136 个测试）"。

**反证构造**：三个计数分别复算；再检查"默认特性下 136"是否与特性门控一致（若某模块测试在默认特性下不编译，总数应低于全文件 `#[test]` 之和）。

**取证**：
- `grep -c '    fn test_' os/libs/minix-elf/src/lib.rs` → `27`。
- `grep -c '#\[test\]' os/boot-shim/src/loader.rs` → `32`。
- 全 crate `#[test]` 合计：`dtb_copy.rs 15 + ebs_map.rs 45 + ebs_pair.rs 17 + lib.rs 1 + loader.rs 32 + opensbi_helpers.rs 11 + resv_claims.rs 14 + uefi_helpers.rs 12 = 147`。
- `os/boot-shim/Cargo.toml`：`default = ["uefi"]`，`opensbi` 不在默认特性里；`opensbi_helpers.rs` 的 11 个测试因此在默认构建下不编译。`147 − 11 = 136`，与文档一致。

## VERDICT

CONFIRM（L1，含一次门控推演）。"136"未用 `cargo test` 实跑核对（只读约束下未构建），但 147−11 的算式与特性表闭合，没有第二种合理解释。

---

## CLAIM 09

**主张**（§4.3、§3.6）："同一片被两个窗口各写一次时，第二次安装看到 `e2` 已 present，三份实现都返回 `AlreadyMapped`（不比较物理地址与标志）"；"`AlreadyMapped` 的语义是'目标槽已有叶（任意尺寸）'，不是'重复映射'"，并称源码 Step 1 注释仍写旧行为、与实现不符。

**反证构造**：若任一架构的 `map_huge` 在"已有同尺寸叶"时返回 `Ok`，或先比较 paddr/flags 再决定，则文档的"三份都"与"不比较"同时破产；若源码注释其实已改成新行为，则"源码注释与实现不符"这句本身不成立。

**取证**（逐架构读 `fn map_huge`）：
- `os/arch/src/x86_64/paging.rs:1296-1328`：1 GiB 分支 `if e3 & PRESENT != 0 { return Err(AlreadyMapped) }`；2 MiB 分支先 `e3 present && e3 PS` → `AlreadyMapped`（"A 1 GiB leaf blocks the finer install."），再 `e2 & PRESENT != 0` → `AlreadyMapped`。全程未比较 `paddr` 或 flags。
- `os/arch/src/arm64/paging.rs:1219-1248`：同样的三段守卫，`e2` 已 present 即 `AlreadyMapped`。
- `os/arch/src/riscv64/paging.rs:1226-1257`：三处 `return Err(PageTableError::AlreadyMapped)`，结构与前两者同形。
- `os/kernel/src/lib.rs` 的 Step 1 调点：`Ok(()) | Err(PageTableError::AlreadyMapped) => {}`，把重复安装吸收为成功，与"重叠窗口是常态路径"一致。

## VERDICT

CONFIRM（x86_64 为 L2 逐行；arm64/riscv64 为 L2 grep 定位守卫行，未全文通读）。"不比较物理地址与标志"在三个实现里都成立。保留意见：文档关于"源码 Step 1 注释仍按旧行为写"的陈述本轮未逐字复核（不在 10–15 条主证范围内），不影响本条结论。

---

## CLAIM 10

**主张**（§5.3）："PC 不进入 x86_64/aarch64 的成败判定（riscv64 相反）"；"x86_64 与 aarch64 只打印（`PC 不在高半` 时输出 `INFO`，只在失败分支里出现）"；"riscv64 把 `pc_at_high` 并入 PASS 条件，PC 落在低半即 `FAIL: PC not at high address (riscv64 delta path broken)`"；`sp_aligned` 在 x86_64 检查 `sp & 0xF == 8`、在 aarch64/riscv64 检查 `sp & 0xF == 0`。

**反证构造**：若 `pc_ok` 在非 riscv64 上不是恒真，或 INFO 行出现在 PASS 分支，或 `sp_aligned` 的掩码写反，则文档对"测试判据"的描述失真。

**取证**：`sed -n '/fn kmain_verify/,/^}/p' os/kernel/src/lib.rs`：
- `let pc_at_high = pc >= kern_high;`
- `#[cfg(target_arch = "riscv64")] let pc_ok = pc_at_high;`
- `#[cfg(not(target_arch = "riscv64"))] let pc_ok = true;`
- PASS 条件为 `sp_ok && sp_aligned && fp_zero && pc_ok`。
- INFO 与 riscv64 的 FAIL 行都在 `else`（失败）分支内；riscv64 分支打印 `FAIL: PC not at high address (riscv64 delta path broken)`，非 riscv64 分支打印 `INFO: PC not at high address (expected in test kernel)`。
- `sp_aligned`：x86_64 为 `sp & 0xF == 8`，aarch64/riscv64 为 `sp & 0xF == 0`。

## VERDICT

CONFIRM（L1）。

---

## CLAIM 11

**主张**（§3.3、§3.7）："`boot_validate_and_prepare` 里有一条断言 `kern_stack_top must be 16-byte aligned`"；"`boot_validate_and_prepare` 只负责采集能力事实与注册分配器"，选粒度的是 `plan_kernel_mapping`。

**反证构造**：若该断言不在 `boot_validate_and_prepare` 内（例如在 `arch_boot` 或 `arch_boot_impl`），则 §3.3/§3.7 的函数归属错误；若 `boot_validate_and_prepare` 实际调用了粒度选择，则 §3.5 反复声明的分工（"选粒度在 boot_policy"）失真。

**取证**：
- `os/kernel/src/lib.rs:863` 起为 `fn boot_validate_and_prepare<A: BootArch>(`；断言在 907–911：`assert!(kern_stack_top.0.is_multiple_of(16), "arch_boot: kern_stack_top must be 16-byte aligned");`。函数归属成立。
- `plan_kernel_mapping` 在 `os/kernel/src/boot_policy.rs`，以 `preferred.supported() && layout_satisfies(...)` 的阶梯选粒度；`arch_boot` 把 `plan_boot_pages` 的错误 panic 成 `arch_boot: page-size policy: {e}`。

## VERDICT

CONFIRM（L1）。断言的 panic 文案带前缀 `arch_boot:`，文档引用的是子串 `kern_stack_top must be 16-byte aligned`，不构成不符。

---

## CLAIM 12

**主张**（§4.4 对齐算账）：x86_64 序列 `mov rsp` → `and rsp,-16` → `xor rbp` → `push 0` → `sub rsp,8` → `call` 之后，"进入 `kmain` 时 RSP = stack_top − 24，即 8 mod 16"；"只去掉 `sub rsp, 8` 也会坏"（入口变成 0 mod 16）；"只去掉 `push 0`"同样坏。

**反证构造**：按 16 字节模算逐情形复算；若某情形的入口值其实合法，则文档的"三种都会坏/两条会坏"分类错误。

**取证**（纯算术，stack_top ≡ 0 mod 16 记为 T）：
- 全序列：T → `and` T → `push` T−8 → `sub` T−16 → `call` T−24 ≡ 8 (mod 16)；System V 要求入口 `(RSP+8) % 16 == 0`，即 RSP ≡ 8，通过。
- 去掉 `sub`：T−8 → `call` T−16 ≡ 0，违反；`sp_aligned = sp & 0xF == 8` 会红。
- 去掉 `push`：T → `sub` T−8 → `call` T−16 ≡ 0，违反。
- 三条都去掉且 T 对齐：T → `call` T−8 ≡ 8，恰好合法——文档在情形①里也明说了这一点。
- 复核实现：`os/kernel/src/arch/x86_64/higher_half.rs` 的 asm 序列确为 `mov rsp` / `and rsp, -16` / `xor rbp, rbp` / `push 0` / `sub rsp, 8` / `call {kmain}` / `options(noreturn)`。

## VERDICT

CONFIRM（L1 算术 + L2 读 asm）。三分法与"恰好合法"的边界都算对。

---

## CLAIM 13

**主张**（§4.5）："x86_64 用一段 global-asm stub：它的两个 SysV 实参是 `(entry, arena_pa)`——`rdi` 收 entry、`rsi` 收 arena——stub 内 `mov rbx, rsi; mov rax, rdi; cli; cld; jmp rax`，所以接收侧看到的仍是 `rbx` = arena"。§4.5.1 另称 `_start` trampoline 按序执行 `cli`、`cld`、清 `.bss`（只碰 rdi/rcx）、设栈、把入口寄存器转进 `rdi`、再 `call rust_image_main`。

**反证构造**：在 boot-shim 侧找该 stub，若不存在或指令序列/寄存器绑定不同，则"跨镜像交接的寄存器通道"这一整段失真。注意文档把 stub 放在"发送侧"叙述里，若实际定义在内核 crate（被 shim 调用），位置叙述需核对但不改变语义。

**取证**：
- `grep -rn 'jmp rax\|mov rax' os/boot-shim/src/` 无命中；`grep -rn 'fn enter_kernel_image_v1' os/boot-shim/src/` 也无定义。
- 定义在 `os/kernel/src/lib.rs:538` 的 `pub fn enter_kernel_image_v1`，其 `jump_kernel_image_v1` 对 x86_64 调 `__minix_x86_enter_image(entry, arena_pa)`；`global_asm!` 块（同文件，`__minix_x86_enter_image:`）逐字为 `mov rbx, rsi` / `mov rax, rdi` / `cli` / `cld` / `jmp rax`；Rust 声明 `unsafe extern "sysv64" { fn __minix_x86_enter_image(entry: u64, arena_pa: u64) -> !; }`。
- `os/kernel-image/src/main.rs` 的 x86_64 `_start`：`cli`、`cld`、`lea rdi,[rip+__bss_start]`/`lea rcx,[rip+__bss_end]` 清 `.bss`、`lea rsp,[rip+kernel_boot_stack_top]`、`and rsp,-16`、`xor ebp,ebp`、`mov rdi,rbx`、`call {rust_image_main}`。

## VERDICT

CONFIRM（L1）。指令序列逐字对上。位置说明：stub 不在 boot-shim crate 而在 `os/kernel/src/lib.rs`（文档 §4.5 的"发送侧"是调用关系叙述，§7.3 也把 `os/kernel/src/lib.rs` 列入关联源），不构成事实错误；trampoline 序列比文档列举多出 `and rsp,-16` 与 `xor ebp,ebp` 两条，属省略而非错列。

---

## CLAIM 14

**主张**（§4.4 其二）："`enable()` 在写完 `ttbr0_el1`/`ttbr1_el1` 之后已经执行 `dsb sy; isb`，写完 `sctlr_el1`（置 M=1）之后又执行一次 `isb`；三条 aarch64 路径都经过 `enable()` … 删掉 `jump_to_kmain` 里的 `isb` 不会产生'按旧映射取指'的失败"。

**反证构造**：若 `enable()` 的屏障次序是"先写 SCTLR 再写 TTBR"或缺少 `isb`，则"防御性 isb"的定性错误；若存在不经过 `enable()` 的 aarch64 入口，则"三条路径都经过"不成立。

**取证**：
- `os/arch/src/arm64/paging.rs:721-736`：`msr ttbr0_el1`（721）→ `msr ttbr1_el1`（722）→ `dsb sy`（724）→ `isb`（725）→ `mrs sctlr_el1`（733）→ `msr sctlr_el1`（735）→ `isb`（736）。
- `isb` 在 `jump_to_kmain` 的 asm 里位于 `br` 之前，注释自述 `defensive barrier; enable() already synchronized TTBR/SCTLR`（`os/kernel/src/arch/aarch64/higher_half.rs`）。
- "三条路径都经过 `enable()`"只做了结构核对：`arch_boot_impl` 的 Step 3 调 `A::Paging::enable`，v1 接收走 shim 的 `build_bootstrap_root_and_enable`；未逐条枚举 aarch64 全部入口（该枚举属 01 篇）。

## VERDICT

CONFIRM（L2，限 `enable()` 内部次序与 `jump_to_kmain` 的 `isb` 位置）。"三条路径都经过 `enable()`"这一句本轮未独立证伪，标为未覆盖而非已确认。

---

## CLAIM 15

**主张**（§5.2 的 M12 行与 §6.2 注释）："`check-layout.sh` 三架构实跑（本轮复跑全 PASS）"；"当前工件的 LOAD 行数：三架构各 4 行 … 4 行依次是 `.text`（RX）、`.rodata`（R，含 `.lrodata` 与 `.eh_frame_hdr`）、`.data`（RW）、`.bss`（RW、NOBITS）"；"x86_64 的 `.lrodata.*` 已在 R4 收进 `.rodata`"。

**反证构造**：这两条都是对**已构建工件/已发生运行**的断言。只读且未构建的环境下，只能检查脚本与链接脚本是否支持该结论，不能检查工件本身。若 `os/target` 下没有对应 release 工件，则无法证实也无法证伪。

**取证**：
- 三份 `.ld` 的 `.rodata` 通配确实含 `.lrodata`（x86_64.ld:1193 附近的 `*(.rodata .rodata.* .lrodata .lrodata.*)`），`.bss` 含 `.lbss`，`.data` 含 `.ldata`——脚本层面与"4 段"相容。
- `/DISCARD/` 丢弃 `.eh_frame` 但不丢 `.eh_frame_hdr`（它是链接器合成节），与"随 `.rodata` 落段"相容。
- 未执行 `check-layout.sh`，也未找到可读的已构建工件来数 `readelf -lW | grep -c '^  LOAD'`；`grep -c` 的实际读数无从取得。

## VERDICT

UNCLEAR。脚本与链接脚本支持该结论，但"各 4 行"和"本轮复跑全 PASS"都是工件/运行态断言，只读会话无法复核。按 Gate 证据分级，这属于 L3 推断，不能计为 CONFIRM；需要一次带 toolchain 的 `bash os/kernel-image/check-layout.sh all` 与 `readelf -lW` 才能落定。

---

## 汇总

| 判定 | 条数 | 条目 |
|---|---|---|
| CONFIRM | 14 | 01–14 |
| REFUTE | 0 | — |
| UNCLEAR | 1 | 15 |

本轮未构造出可坐实的反例。最接近"位置性偏差"的是 CLAIM 13：文档把 x86_64 交接 stub 写在"发送侧"的叙述里，实际定义在 `os/kernel/src/lib.rs` 而非 `os/boot-shim`；指令序列与寄存器约定本身逐字正确，故记为 CONFIRM 附注而非 REFUTE。唯一无法在只读会话内闭合的是 CLAIM 15 的工件计数与"本轮实跑"声明，按规则记 UNCLEAR，不给模糊通过。
