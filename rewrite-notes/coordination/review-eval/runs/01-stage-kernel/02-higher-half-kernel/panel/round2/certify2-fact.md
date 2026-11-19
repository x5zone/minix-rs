# D02 R2 最终 certify 事实回读（第 2 次确认）

- 文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`
- 规模：1280 行；sha256 前缀 `06e4987f`（全文 `06e4987f53a3d51f1e92bb2bff6af247cc30eeffbf3077307bfb2698999b1bfe`）
- 范围：只读指定改动句与邻句；未重扫全文，未读历史评审、其他面板与 expected。
- 结论：6/6 项主述 CLOSED；新增 2 条 P2（可选修复，均不推翻主述）。

## 逐条回读

### 1. §前置知识 按腿起点（含 riscv64 测试载体）与“各腿起点不同” — CLOSED

逐腿核对（文档 L8）：

- v1 send“分页已开、shim 低恒等视图”：`os/boot-shim/src/uefi_helpers.rs:915-916` 写明 shim 在 high-half jump 之前一直执行在最小恒等映射里；`os/boot-shim/src/uboot_main.rs:36-40` 写明 `bootelf` 以无 MMU 进入，bootstrap root 启用后 shim 仍走 root 的 identity window，直到 `enter_kernel_image_v1` 跳到 receiver 的 high-half `_start`；`os/kernel/src/lib.rs:547-548` 的 `jump_kernel_image_v1` 注释写明“paging is enabled and `entry` maps the executable kernel-image `_start`”。三处与“交接前”的限定一致。
- riscv64 自引导“分页未开、物理视图，Step 3 才开”：`os/kernel-image/src/bootface.rs:44-52` 写明 satp=0 的物理视图；`os/kernel/src/lib.rs:969-972` 写明 Steps 1-3 先建 bootstrap root 再 `enable`，Step 4 才是 DM。
- aarch64 direct-core“低地址进入、物理视图”：`os/kernel-image/src/bootface_a64.rs:47` 写明“Under the MMU-off physical view”；同文件 `:296-297` 写明该面“runs before paging is enabled”；`os/kernel-image/aarch64.ld` 的 `KERNEL_PHYS_BASE = 0x40200000` 是固件实际进入的物理低地址，VMA 是 `0xFFFF800000000000`。
- riscv64 测试载体“物理视图、PC 在高半 VMA”：`test-higher-half-riscv64/src/main.rs:111-123` 合成 `KernelInfo`（`kern_virt_base = 0xFFFF_FFC0_0000_0000`，`kern_phys_base = 0x8000_0000`，`kern_size = 0x400_000`）；`os/kernel/src/arch/riscv64/higher_half.rs` 的 `auipc/bltu` 分支在 PC 低于 `kern_virt_base` 时加 `delta = kern_virt_base - kern_phys_base`，跳转目标是 `kmain` 的高半 VMA；文档 §5.2 也记录了同一读数（`0xffffffc0…`，修正前 `0x8020…`）。
- inline 测试载体“低地址视图”：`test-higher-half/src/main.rs` 与 `test-higher-half-aarch64/src/main.rs` 都直接 inline 调用 `minix_kernel::arch_boot`，没有生产镜像的 `AT()` 两套地址；文档 §3.1 的载体行同义。
- C 侧 i386“分页未开”：`minix3/minix/kernel/arch/i386/pre_init.c:230-234` 依次调用 `pg_clear/pg_identity/pg_mapkernel/pg_load/vm_enable_paging`；`head.S:77` 在 `pre_init` 返回前没有开分页。

“各腿起点不同”成立：上列六处视图互不相同（高半恒等、物理未开、低地址物理、物理视图高进、低地址视图、C 侧未开分页）。主述事实正确。

### 2. §2.1 L126 `.bss` 的 NOBITS 尾段与清零起点 — CLOSED

- `minix3/minix/kernel/arch/i386/kernel.lds:33-38`：`.data ALIGN(4096) : AT(ADDR(.data) - _kern_offset)` 之后是 `. = ALIGN(4096); .bss ALIGN(4096) : AT(ADDR(.bss) - _kern_offset)`；两者同为 RW、相邻（仅有页对齐空隙），GNU ld 会把它们并入同一个 RW `PT_LOAD`，`.bss` 是该段的 NOBITS 尾段。
- 用同形脚本在宿主上复算（`ld -m elf_i386`，GNU ld 2.42）：产物只有两个 `LOAD`，第二个 `RW` 段 `FileSiz=0xc`、`MemSiz=0x1040`，Section to Segment mapping 为 `.data .bss`。装载方按 `[paddr + p_filesz, paddr + p_memsz)` 清零，起点正是 `.data` 文件字节之后；只有 `.bss` 被单独拆成一段时，清零才会从该段自己的 `p_paddr`（反事实里是 `0x00400000`）起。文档的条件句与 ELF 语义一致。
- 反事实“五处 `AT()` 全写成 `AT(_kern_phys_base)`”：同一脚本改成常量 `AT` 后，GNU ld 2.42 实测报 `section .data LMA [00400000,0040000b] overlaps section .text LMA [00400000,00400001]` 并以 exit 1 拒绝产出，与“现行 GNU ld 对 LMA 重叠报错并拒绝产出 ELF”一致。

### 3. §2.2 L207 `pg_utils.o` 未定义符号说明与括注 — CLOSED（另有 P2，见 FINDING-01）

- `minix3/minix/kernel/arch/i386/Makefile.inc:51` 的 `.for unpaged_obj in ...` 名单包含 `pg_utils.o`；同文件 `SRCS+=` 一行（`:75` 起）包含 `pg_utils.c`，但不包含 `pre_init.c`。
- `pre_init.c:230-234` 调用 `pg_clear/pg_identity/pg_mapkernel/pg_load/vm_enable_paging` 五个函数；五个定义都在 `pg_utils.c`（`:162`、`:186`、`:204`、`:247`、`:254`）。
- `objcopy --prefix-symbols=__k_unpaged_` 对调用方实测：未定义符号为 `__k_unpaged_pg_clear`、`__k_unpaged_pg_identity`、`__k_unpaged_pg_load`、`__k_unpaged_pg_mapkernel`、`__k_unpaged_vm_enable_paging`；未加前缀的 `pg_utils.o` 只有不带前缀的五个定义。主述“pre_init.o 一侧的引用已加前缀，普通 pg_utils.o 只有不带前缀的定义”正确；括注左右配对，无悬空括注。
- 该段“pre_init.c 不在任何 SRCS 里，去掉名单项会连唯一生成规则一起消失”成立：`pre_init.c` 只经 `.for` 循环生成 `unpaged_pre_init.o`，不在任何 `SRCS` 清单里。

### 4. §2.3 L267 “上一处是停在 hang 停机，这一处坏在数据访问” — CLOSED

- `minix3/minix/kernel/arch/i386/head.S:87-91`：`call _C_LABEL(kmain)` 之后是 `/* not reached */ hang: jmp hang`。只切栈不跳时（`call` 被移除或落到下一行），PC 停在 `hang` 的兜底循环，症状是停机而不是页错误。
- `head.S:69` 初始栈是 `mov $load_stack_start, %esp`；`:94-96` 的 `load_stack: .space 4096; load_stack_start:` 表明栈区在 `head.S` 自己的 `.data` 段、低地址一侧。只跳不切时，`call` 先把返回地址压进这个低地址栈，`kmain` 的序言与后续 `push/pop` 也继续使用它；恒等映射移除后第一次栈访问落在无映射页，`CR2` 是低地址栈页地址。文档把“上一处”归为停机、“这一处”归为数据访问，与 `head.S` 事实一致。

### 5. §2.4/§2.5 的 i386/ARM 限定 — CLOSED

- i386 结论“高一侧”：`i386/mpx.S:628-632` 是 `.data`、`.bss`、`k_initial_stack:`，栈定义在 `.bss`；`i386/Makefile.inc:51` 的改名名单没有 `mpx.o`，`SRCS+=` 里 `mpx.S` 只进普通源码清单；`i386/kernel.lds:18-38` 的 `.unpaged_bss` 只收 `unpaged_*.o(.bss COMMON)`，通用 `.bss` 在 `. += _kern_offset` 之后。三条证据推出 `k_initial_stktop` 落在 `0xF04xxxxx` 一侧，成立。
- ARM 限定“`.data`、证据待补”：`earm/mpx.S:265-272` 为 `.data`（魔术字）→ `.bss` → `.data`（`k_initial_stack`），栈等价物确在 `.data`；`earm/kernel.lds:33-37` 的通用 `.data` 在 `. += _kern_offset` 之后。文档只说段归属、把改名名单与收段证据列为“待补”，没有把 i386 的结论外推到 ARM，属于正确的欠声明。
- §2.5 链接布局表：带 `AT()` 的普通段“符号值高一侧、装载地址低一侧，差是 `_kern_offset`”对 i386/ARM 的 `kernel.lds` 都成立；`.unpaged_*` 三段的符号值与装载地址都在低侧（`i386/kernel.lds:15-24` 与 `earm/kernel.lds:15-24` 一致）。
- “两个动作指向同一侧”只对 i386 断言：`head.S:69` 的 `mov $k_initial_stktop, %esp` 携带高半符号值，`head.S:87` 的 `call kmain` 是同一高半 `.text` 内的相对位移；ARM 侧只写到“按同一判据、`.data` 收段证据待补”，没有过度断言。与 `earm/head.S:42` 的 `ldr sp, =k_initial_stktop`、`:45-46` 的 `ldr r2, =_C_LABEL(kmain); bx r2` 对照，限定的方向正确。

### 6. §3.2 迭代器 panic 语义、§3.3 `syscall_entry` 读者范围、四处“注释有改写” — CLOSED

- 迭代器 panic 语义：`os/libs/minix-elf/src/lib.rs:324-333` 的 `SegmentIter::new` 用未检查的 `e_phoff as usize + (e_phnum as usize) * (e_phentsize as usize)`；`:246-255` 的 `parse_phdr` 用未检查的 `offset + (entsize as usize)`。`next()` 里的 `checked_mul/checked_add` 是防御性 `continue`，但表范围加法一旦溢出（debug 直接 panic；release 回绕后落到 `&image[offset..]` 的切片越界）仍是 panic 而不是 `Err`。文档把两类路径分开表述，正确。
- `syscall_entry` 读者范围（文档 L531，实际位于 §3.3）：生产路径 `os/kernel/src/lib.rs:2276` 用 `trap.configure_syscall(syscall_entry_va())`，不消费 `KernelInfo.syscall_entry`；验证/序列化三处读取分别见 `os/libs/minix-boot/src/kernel_info.rs:459-460`（`validate_requiring_sources` 断言等于 `kern_virt_base`）、`:582-589`（`check_boot_postconditions` 断言落在镜像内）、`os/libs/minix-boot/src/handoff_v1_write.rs:143`（`debug_assert_eq!`）。全仓检索 `configure_syscall` 后，唯一以 `info.syscall_entry` 传入的是 `os/qemu-tests/test-kernels/kernel/bootstrap/test-protection-riscv64/src/main.rs:148`；其余调用点传 `minix_arch::syscall_entry_va()`。
- 四处“注释有改写”：`grep -c "注释有改写"` = 4，`grep -c "注释有删改"` = 0；位置为 L419、L457、L796、L1124。逐处对照源文件：L419 的 `Elf64Ehdr/Elf64Phdr/LoadSegment` 源用 `///` 文档注释、节选改为行内注释；L457 的 `next()` 源注释“already validated bounds in new(), but be defensive”被改写为中文防御性注释；L796 的 `compute_kernel_layout` 源注释被压缩/省略；L1124 的 `kmain_verify` 源对齐注释被压缩成两行 `->` 表述。四处标注与节选现状一致。

## FINDING RD2-CERT2-FACT-01（P2）

**位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:207`。

**事实**：该句说“链接期会得到 `__k_unpaged_pg_*` 未定义符号”。但 `pre_init.c:230-234` 调用的五个函数中有 `vm_enable_paging`，它在 `pg_utils.c:204` 定义；`objcopy --prefix-symbols=__k_unpaged_` 后，调用方的未定义符号是四个 `__k_unpaged_pg_*` 加 `__k_unpaged_vm_enable_paging`（宿主复算：`nm -u` 输出五行，其中 `U __k_unpaged_vm_enable_paging` 不匹配 `__k_unpaged_pg_*` 这个 glob）。同一段（L207）后文自己也把 `vm_enable_paging` 列为同一个对象里的五个函数之一，前后不一致。

**影响**：主述（前缀不匹配导致链接期未定义符号）仍然成立；但 glob 让读者以为只有 `pg_*` 一族受影响，按该模式检索链接日志会漏掉一个符号。

**建议**：把 `__k_unpaged_pg_*` 改为 `__k_unpaged_*`（并说明是 `pg_utils.o` 的五个外部定义），或写成“`__k_unpaged_pg_*` 与 `__k_unpaged_vm_enable_paging`”。属可选修复。

## FINDING RD2-CERT2-FACT-02（P2）

**位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:8` 与 `:362`。

**事实**：L8 的引导语是“Rust 生产路径的起点按腿不同”，随后用分号并列了五条腿，其中最后两条是“riscv64 测试载体”和“inline 测试载体”；L362 明确写“测试载体走 inline `arch_boot`，是低地址执行视图，不是生产路径”。同篇对“生产路径”与“测试载体”的划分在 §3.1、§4.4、§5.4 反复出现（如 L1064“两条生产路径”、L1163“两条生产路径”），L8 的引导语把测试载体并入“Rust 生产路径”，与这些表述不一致。

**影响**：五条腿各自的视图与 PC 事实都正确（见第 1 条）；问题只在引导语的术语范围，不影响技术结论。

**建议**：把“Rust 生产路径的起点”改为“Rust 侧各腿的起点（生产路径与测试载体）”，或把测试载体两句从该引导语的并列范围中拆出。属可选修复。

## 门检查

| 门 | 命令 | 结果 |
|---|---|---|
| doc-citation-check | `python3 tools/doc-citation-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | exit 0；`blocks=32 checked=1 failed=0 skipped=31 warned=0` |
| anchor-resolve --check | `bash tools/anchor-resolve.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | exit 0；`anchors=55 resolved=55 zero-def=0 multi-def=0` |
| doc-structure-check | `python3 tools/doc-structure-check.py rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | exit 0；`[PASS] doc-structure-check: 1 个文件表结构与内部 .md 链接均可解析` |
| doc-ref-check | `bash tools/doc-ref-check.sh --check rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` | exit 0；`[PASS] doc-ref-check: ... 内部章节引用均可解析` |

说明：`doc-citation-check` 的 `checked=1 skipped=31` 是该工具的既有筛选行为——只有显式“代码引用”块进入机械对账，其余“节选”块按设计跳过；本文件的唯一进入块（597-610）PASS，未出现 failed/warned。

最终认证新准入 = 2
