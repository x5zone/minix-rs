# P-REFUTE · 01-boot-shim-bootstrap · round36

> 立场：只尝试推翻文档的核心主张，不补新问题。以下每条都给出可落到源码/文档原句的证据。
> 真值优先序按 `AGENTS.md`：Minix3 C 源码行为 > 设计契约 > Rust 实现 > 文档。本次以当前工作树（HEAD `e04e61cb5`）为准。

## CLAIM 01

- 主张：§1.1/§2.2 的 C 侧入口结论——`i386/head.S` 的 `multiboot_init` 第一条指令必须设栈，且 `load_stack_start` 由链接期布局保证 4096 对齐；按 `push $0; popf; push $0; push %ebx; push %eax; call` 的次序，`pre_init` 入口处 `%esp ≡ 0 (mod 16)`，不满足 i386 SysV 的入口 `%esp ≡ 12 (mod 16)`；这把 C 原作的入口栈对齐记成风险提示而非设计选择。
- 反证尝试：
  - 换引导路径：boot2 与 Multiboot 合规引导器都跳到同一个 `MINIX:` → `jmp multiboot_init`；`head.S` 没有第二个入口。boot2 不读 Multiboot 头只影响“必要性”论证，不改变“入口实际执行这条设栈指令”的代码事实（文档 §1.4 边界声明已自己限定）。
  - 换架构分支：ARM32 的 `earm/head.S` 用 `ldr sp, =load_stack_start` + `bl pre_init`，没有 x86 的 push/popf 序列，不构成本条反例；文档 §2.7 已单独处理。
  - 检查对齐链是否真的成立：`kernel.lds` 的 `.unpaged_data ALIGN(4096)` 与 `Makefile.inc` 的 `.for unpaged_obj in head.o pre_init.o …` 顺序；`head.o` 的 `.data` 只有 `load_stack: .space 4096`，`load_stack_start` 是段起点 + 4096。没有找到能让 `load_stack_start` 落在非 4096 边界的链接路径。
  - 检查失败路径：`pre_init` 第一条 `assert(magic == MULTIBOOT_INFO_MAGIC)` 在栈对齐问题之前；即使 SSE 访问发生，也只会表现为另一种停机，不会让“入口 esp 值”这个算术结论失效。
- 结果：反证失败（主张成立；准确说是“文档的算术与链接布局陈述成立，ABI 风险按文档所述保持为提示”）。
- 证据：
  - `minix3/minix/kernel/arch/i386/head.S:68-77`（`mov $load_stack_start, %esp`、`push $0`/`popf`、`push $0`、`push %ebx`、`push %eax`、`call pre_init`）与 `:93-96`（`.data` / `load_stack: .space 4096` / `load_stack_start:`）。
  - `minix3/minix/kernel/arch/i386/kernel.lds:17-18`（`.unpaged_data ALIGN(4096) : { unpaged_*.o(.data .rodata*) }`）。
  - `minix3/minix/kernel/arch/i386/Makefile.inc:51-70, 83`（`head.o` 是 `unpaged_obj` 列表首项，`OBJS.kernel += ${UNPAGED_OBJS}`）。
  - 文档原句：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:276-285`（第 1、4、5 条解码与“C 原作的入口栈对齐不满足 SysV 约定”结论）。
- 若我错了：若链接命令行上还有另一个 `unpaged_*.o` 的 `.data` 排在 `head.o` 之前，或 `head.o` 的 `.data` 里在 `load_stack` 之前还有别的输入节，`load_stack_start` 就不一定 4096 对齐，上面的 `%esp ≡ 0` 会变成别的模值；我检查的 Makefile 展开顺序与 `head.S` 的 `.data` 内容都没有出现这种情况。

## CLAIM 02

- 主张：§2.4.1.3 的 C 侧结论——`alloc_lowest` 的唯一切除调用把 `len` 当 `end` 传给 `cut_memmap`；在文档列出的四个前提同时成立时，真实调用序落进“第三行”（`len ≤ lowest`，切除一次都不发生），即“第三行常态”；这是需要四前提 + 取页方向 + 跳板生命周期才成立的观测结论，不是结构保证。
- 反证尝试：
  - 找第二个调用点：`rg "alloc_lowest" minix3/` 只有定义、原型和 `arch_smp.c:76` 一处调用；ARM 侧只有原型没有调用。
  - 检查调用时序：`kmain` → `cstart()`（`main.c:147`）→ `arch_init()`（`main.c:474`）末尾两条 `cut_memmap`（`arch_system.c:277-278`）；`smp_init()` 在 `main.c:311`，晚于 `cstart`。`alloc_lowest` 只从 `copy_trampoline`（`arch_smp.c:70-81`）到达。
  - 检查最低条目基址：`arch_init` 切 `[BIOS_MEM_BEGIN, BIOS_MEM_END]` = `[0,0x4FF]`，`cut_memmap` 终点向上取整到 `0x1000`；再切 `[0x90000,0xFFFFF]`。`add_memmap` 把每条记录起点向上取整，故最低可用基址至少 `0x1000`。
  - 检查 `len`：`tramp_size = &__trampoline_end - tramp_start`；`trampoline.S` 里代码 + 4+4+8+8 数据 + `GDT_SIZE*8` + `IDT_SIZE*8`，当前 `CONFIG_MAX_CPUS=1`、`GDT_SIZE=7`、`IDT_SIZE=256`，总量约 2.2 KB，`roundup(...,4096)=0x1000`；`0x1000 ≤ 0x1000`，仍在第三行。
  - 检查退化/边界：`lowest == len` 时 `cut_memmap(start=lowest,end=len)` 的 `substart >= subend`，同样不切；`lowest=0` 不可达（含 0 低段已被 `arch_init` 切掉）。
  - 换配置：若 `CONFIG_MAX_CPUS` 大到让 `GDT_SIZE*8` 把 `tramp_size` 推过 4096，`len=0x2000 > lowest=0x1000`，就会落进第二行；当前构建常量是 1，文档也声明了“改配置要重算”的口径。
- 结果：反证失败（主张成立，但限定在文档自设的“当前常量 + 四前提”口径内；这不是结构保证，文档已如实声明）。
- 证据：
  - `minix3/minix/kernel/arch/i386/pg_utils.c:65-84`（`alloc_lowest` 调 `cut_memmap(cbi, lowest, len)`）、`:564-595`（`cut_memmap(cbi, phys_bytes start, phys_bytes end)` 签名）。
  - `minix3/minix/kernel/arch/i386/arch_smp.c:70-81`（唯一调用点与 `assert(trampoline_base + tramp_size < (1 << 20))`）。
  - `minix3/minix/kernel/arch/i386/arch_system.c:277-278` 与 `minix3/minix/include/arch/i386/include/memory.h:29,33,118-119`（两条切除的常量）。
  - `minix3/minix/kernel/kernel.h:7-8,51`（`CONFIG_MAX_CPUS=1`）、`minix3/minix/kernel/arch/i386/include/archconst.h:11,19-21,31`（`IDT_SIZE=256`、`GDT_SIZE=TSS_INDEX(CONFIG_MAX_CPUS)=6+1`、`DESC_SIZE=8`）、`trampoline.S:54-58`。
  - 文档原句：`01-boot-shim-bootstrap.md:714-731`（三种取值关系、四前提与“第三行常态”定义）。
- 若我错了：若在 `init` 之外的启动路径里还有 `alloc_lowest` 调用（我没有找到）；或 `arch_init` 的两条切除没有先于 `smp_init`（当前 `main.c` 顺序成立）；或 `CONFIG_MAX_CPUS` 被构建系统改成远大于 238 的值（当前 `kernel.h` 默认 1），第三行结论就会改写。

## CLAIM 03

- 主张：§3.4.1/§3.4.3 的交接契约结论——当前跨镜像 wire 是 v10：`BOOT_HANDOFF_V1_VERSION` 已从 9 升到 10，收发两侧精确比较，v9 与 v10 互相拒收；v10 删掉 `user_sp`/`kern_stack_top`/`syscall_entry` 与 startup 分区，信封从 160 B 缩到 128 B，arena 仍是八页、已用 29184 B、余 3584 B。
- 反证尝试：
  - 直接读常量：`os/libs/minix-boot/src/handoff_v1.rs:288` 是 `pub const BOOT_HANDOFF_V1_VERSION: u32 = 9;`，不是 10。
  - 读结构体：`BootHandoffV1`（`:299-330`）确实已经没有 `user_sp`/`kern_stack_top`/`syscall_entry`/`startup_byte_len`；`size_of::<BootHandoffV1>() == 128` 的编译期断言在 `:435`。也就是说“v10 的布局”已在树上，但“v10 的版本号”没有。
  - 读接收侧：`handoff_v1_read.rs:107` 用 `!= BOOT_HANDOFF_V1_VERSION` 精确比较；由于双方都读到 9，版本门本身不会把旧 v9（160 B 信封）与当前 128 B 信封按版本区分——旧包会先在 `check_header` 的 `SIZE != size_of::<BootHandoffV1>()`（`:110-112`）处被拒，报的是 `BadEnvelopeSize`，不是 `UnsupportedVersion`。
  - 读文档自身：同一篇 §3.4.3 `:1713` 写“信封版本号（`BOOT_HANDOFF_V1_VERSION`，当前 9）”；§4.6 `:2892` 写“信封版本仍为 9”；而 §3.4.1 `:1625` 写“`BOOT_HANDOFF_V1_VERSION` 9 → 10”。三处互相冲突。
  - 查提交历史：`git show 2a345f566` 的提交信息声称“版本 9→10”，但该提交的 diff 没有改 `BOOT_HANDOFF_V1_VERSION` 行；当前树的 blame 仍把它记在 `10ecd8ef4`（v9 引入）名下。没有构建脚本或 cfg 在编译期重写这个常量。
- 结果：阻断（P0-fact）。文档的“当前版本号是 10 / v9 与 v10 按版本互拒”不成立；而且文档内部对“当前是 9 还是 10”自相矛盾。布局是 v10、版本标签是 v9，这本身是代码与文档需要对齐的契约缺口。
- 证据：
  - `os/libs/minix-boot/src/handoff_v1.rs:288,299-330,398-402,435,446-448`。
  - `os/libs/minix-boot/src/handoff_v1_read.rs:100-114`。
  - 文档原句：`01-boot-shim-bootstrap.md:1625`（9→10）、`:1713`（当前 9）、`:1744`（v10 删 startup）、`:2892`（信封版本仍为 9）。
  - `git show 2a345f566 -- os/libs/minix-boot/src/handoff_v1.rs`：删字段/删分区，但 `BOOT_HANDOFF_V1_VERSION` 行未变。
- 若我错了：若评审基准是提交信息里的“设计意图”而不是当前源码常量，则文档可被解释成“应然 v10”；但 `AGENTS.md` 的真值优先序把 Rust 实现排在文档之前，且本次任务明确按当前树核。若存在一个我未发现的构建期生成步骤把常量替换成 10（`grep -rn BOOT_HANDOFF_V1_VERSION os` 只看到普通 const 的读写），本条才可撤销。

## CLAIM 04

- 主张：§4.5/§3.4.5/§4.4 的内核落地区与校验结论——`store_kernel_info` 有“七个落地区 + 一个名字池”，三个生产调用点（`arch_boot_finish`、`arch_boot_resume_v1`、`kmain` 重存同址）；落地区表包含 `STARTUP_RECORD_STORE`（20 条 / wire 分区 1440 B）；`check_boot_postconditions` 检查 `syscall_entry` 在镜像跨度内、`kern_stack_top` 不越上界、用户栈不越入内核半区；接收侧解码“七个分区”。
- 反证尝试：
  - 数落地区：`os/kernel/src/globals.rs:299-419` 只有 `RESERVED_REGION_STORE`、`MEMMAP_REGION_STORE`、`BOOT_MODULE_STORE`/`BOOT_MODULE_NAME_STORE`、`PLATFORM_SOURCE_STORE`、`PARAM_BUF_STORE`、`SOURCE_RECORD_STORE` 六类；`grep -rn STARTUP_RECORD_STORE os/kernel/src os/libs/minix-boot/src` 零命中。
  - 数 `store_kernel_info` 调用点：`os/kernel/src/lib.rs:225`（`arch_boot_finish`）与 `:347`（`resume_additive`）两处；`kmain` 在 `:1820-1821` 直接读 `crate::kernel_info()` 全局槽，不再调用 store。`store_kernel_info` 自己在 `:1578-1583` 有一条 `KERNEL_INFO.is_none()` 断言（I-35(a)/D-071 一次性槽）：“a second store would rewrite a live &'static slot”；因此文档说的“kmain 再存一次同址跳过”在当前代码里会直接 panic，不是 no-op。
  - 数实际 land 的切片：`lib.rs:1601-1689` 依次 land `reserved_regions`、`memmap`、`boot_modules`（名字池）、`platform_sources`、`param_buf`、`memmap_sources`，共六类；`:1681-1689` 的注释明写“v10 / D-062 removed the startup-object class”。
  - 接收侧：`arch_boot_resume_v1`（`lib.rs:680-772`）解码 `memmap`、`reserved_regions`、`boot_modules`、`platform_sources`、`param_buf`、`sources` 六个载荷分区，没有 startup 分区；`handoff_v1.rs` 的 arena 注释（`:398-401`）与 `WIRE_SOURCES_END=29184` 也说明 startup 分区已退场。
  - postconditions：`os/libs/minix-boot/src/kernel_info.rs:554-573` 的 `check_boot_postconditions` 只剩一条 `kern_virt_base > u32::MAX` 的高半断言；注释明写 v10 删掉了 `user_sp`/`syscall_entry`/`kern_stack_top` 三条地址关系断言，`validate_requiring_sources` 也不再核对这三条派生标量（`:324-345` 的 strict 段改为模块名、source 覆盖与内核跨度覆盖）。文档 §4.4 的 gate 表 `:2742-2746` 与正文 `:2726` 仍写旧的三组检查；同表 `:2746` 还写“`startup_objects` 至少一条覆盖内核镜像跨度……核对三条派生标量”，而源码 `:324-345` 已改成“内核跨度落在 memmap 或 reserved_regions 的某一区间”，`startup_objects` 字段和三条派生标量都已不存在。
- 结果：阻断（P0-fact）。这不是“数字差一”，而是文档把 v10 已删除的 startup 落地区、一次性 store 语义和 postcondition 检查都写成了现役结构；`kmain` 重存的说法与一写断言直接冲突。
- 证据：
  - `os/kernel/src/lib.rs:225,347,1574-1583,1601-1689,1806-1821`。
  - `os/kernel/src/globals.rs:299-419`（无 STARTUP store）。
  - `os/libs/minix-boot/src/kernel_info.rs:324-345,554-573`。
  - 文档原句：`01-boot-shim-bootstrap.md:2837-2847`（七个落地区 + STARTUP 20 槽 + 1440 B）、`:2855`（三个调用点 + kmain 重存）、`:2726` 与 `:2742-2746`（postconditions 三组检查）、`:1759-1764`（BootStorage 的 startup 20 项）、`:1585/2059`（接收方解码七个分区）。
- 若我错了：若评审对象是 v10 提交之前的文档版本，则这些句子可能自洽；但当前文档同一批句子已经自称 v10/D-062，而当前树 `grep STARTUP_RECORD_STORE` 零命中。若 `kmain` 的 store 调用通过某个宏或别名间接发生（`grep -rn "store_kernel_info" os/kernel/src` 只有上述两处调用），或 `STARTUP_RECORD_STORE` 定义在我未搜到的 cfg 模块里，本条才可部分撤销。

## CLAIM 05

- 主张：§3.6 与 §4.6 的 `map_huge` 结论——`AlreadyMapped` 只在“目标槽位已经是一个 1 GiB 叶”时返回；同一 2 MiB 片被两个窗口各写一次时，三个架构的 2 MiB 路径都直接重写同一个叶并返回 `Ok(())`；因此 Step 1 里的 `AlreadyMapped` 接受臂是防御性例外、在根初始化路径下不可达。
- 反证尝试：
  - x86-64：`map_huge` 的 1 GiB 分支先拒绝任何已存在的 PDPT 条目（`paging.rs:1296-1299`）；随后对 2 MiB 路径有 `if e3 & PRESENT != 0 && e3 & PS != 0 { return Err(AlreadyMapped) }`（`:1309-1311`），再往下还有 `if e2 & PRESENT != 0 { return Err(AlreadyMapped) }`（`:1325-1327`）——即“该 2 MiB 叶已存在”本身就返回 `AlreadyMapped`，不是重写后 `Ok(())`。
  - aarch64：`map_huge` 在 1 GiB 分支拒绝已存在条目（`:1220-1222`），2 MiB 路径先拒 1 GiB block（`:1233-1236`），再在 `e2 & VALID != 0` 时返回 `AlreadyMapped`（`:1248-1250`）。
  - riscv64：1 GiB 分支拒绝已存在 `l2e`（`:1219-1222`），2 MiB 路径在 `l1e & V != 0` 时返回 `AlreadyMapped`（`:1249-1251`）。
  - 再看 Step 1 的调用点：`build_bootstrap_root_and_enable` 对每个 2 MiB piece 调 `map_huge`，用 `Ok(()) | Err(AlreadyMapped) => {}` 吸收（`os/kernel/src/lib.rs:1376-1393`）。也就是说“重叠窗口仍能继续”靠的是调用方接受 `AlreadyMapped`，不是“2 MiB 路径返回 Ok”。
  - 可达性：UEFI 腿的 `shim_stack_window()` 是“从当前 SP 向下 4 MiB、再按 2 MiB 对齐”的保守窗口（`uefi_helpers.rs:990-1004`），root/bump/arena 又是随后由固件在 `MaxAddress` 之下任意放置；两者落入同一个 2 MiB piece 时，第二次 `map_huge` 会返回 `AlreadyMapped`，接受臂并非只在“1 GiB 叶”场景才可能到达。
- 结果：阻断（P1）。文档把“重写并返回 Ok”写成了三个架构的共同语义，实际三个实现都在“2 MiB 叶已存在”时返回 `AlreadyMapped`；Step 1 的行为之所以仍正常，是因为调用方把 `AlreadyMapped` 当成功吸收，而不是因为该分支不可达。§3.6 `:1919` 的“只处理 1GB 大页”也同样过窄。
- 证据：
  - `os/arch/src/x86_64/paging.rs:1296-1327`。
  - `os/arch/src/arm64/paging.rs:1220-1250`。
  - `os/arch/src/riscv64/paging.rs:1219-1251`。
  - `os/kernel/src/lib.rs:1376-1393`（`Ok(()) | Err(PageTableError::AlreadyMapped) => {}`）。
  - `os/boot-shim/src/uefi_helpers.rs:990-1004`（保守栈窗口）。
  - 文档原句：`01-boot-shim-bootstrap.md:1919`（“只处理 1GB 大页”）、`:2927-2928`（“AlreadyMapped 只在槽位已是 1 GiB 叶”“三个架构的 2 MiB 路径都直接重写同一个叶并返回 Ok”）。
- 若我错了：若文档说的“2 MiB 路径”指 `MockPaging::map_range` 或其他测试替身，而不是三个生产 `HugePages::map_huge` 实现，那它需要改写主语；但原句明写“三个架构”。若当前 HEAD 的 `map_huge` 是评审后才改的实现（HEAD 提交 `e04e61cb5` 恰好是 map_huge 引文同步提交），文档至少应在 §4.6 同步这次改动；我按当前树判定。

## CLAIM 06

- 主张：§5.2.1 与 §4.3 的证据总账是当前树的实测记录——“本节所有数字按 `#[test]` 出现次数计……当前值以本逐文件表为准”，`boot-shim --lib` 为 136（22+11+12+45+17+15+13+1），`minix-boot` 为 195（kernel_info 31、handoff_v1_read 21、memmap_source 74 等），`store_kernel_info` 系列 8 条（1 行为 + 7 超容量），`paging.rs` 17 条；§4.3 的 `UbootFileLoader::read` 与 `load_boot_modules_into` 代码引用是现行源码。
- 反证尝试：
  - 按文档自述口径 `grep -c '#\[test\]'` 复算当前树：`loader.rs=31`（文档 22）、`resv_claims.rs=14`（13）、`kernel_info.rs=23`（31）、`memmap_source.rs=75`（74）、`handoff_v1_read.rs=17`（21）、`minix-elf/src/lib.rs=27`（23）、`dm_coverage.rs=11`（10）、`x86_64/paging.rs=20`（17）；`store_kernel_info` 测试当前是 6 枚超容量 + 1 枚落地区行为 = 7 条（文档 8 条，且 §5.3 TODO-03 列的 `startup 21` 负控在源码已不存在）。
  - 因此 `boot-shim` 的逐文件和至少是 `31+11+12+45+17+15+14+1 = 146`，不是 136；`minix-boot` 的逐文件和是 `23+2+10+17+5+5+29+2+8+8+75 = 184`，不是 195。`cargo test` 的实际过滤可能让“running N”与 `#[test]` 数不完全相等，但文档既然自述“按 `#[test]` 出现次数计”，这些逐文件值就已经对不上。
  - `UbootFileLoader::read`：文档 §4.3 `:2603-2627` 的引文含 `minix_elf::segment_iter(slice)` 二次修剪和“已在槽内的输入是 no-op”的说明；当前 `opensbi_helpers.rs:197-215` 的 `read` 直接 `Some(FileBytes::Borrowed(slice))`，注释明确写 D-082/I-36“`read` 只处理字节来源，不做任何 ELF 语义”，并称“表存的是槽容量”是已推翻的误判。引文与源码是两套实现。
  - `load_boot_modules_into`：文档 §4.3 `:2531-2581` 的引文在 `let data = data.as_slice();` 之后直接进 `num_pages`，没有当前源码 `loader.rs:507-520` 的 `let used = elf_file_end(data).unwrap_or(data.len()); let data = &data[..used];` 段；`BootModule.len` 现在记录的是 ELF 运行期文件长度（修剪后），不是文档 `:2588` 说的“文件字节数”。
- 结果：阻断（P1）。§5.2.1 自称“当前值以本逐文件表为准”，但这些数字在当前树已经整体漂移；§4.3 的两段“代码引用”中至少一段与现行实现直接冲突，另一段漏掉了决定 `BootModule.len` 语义的修剪步。按项目的引用纪律，这会破坏“文档-代码同步”门，而不只是计时误差。
- 证据：
  - 文档：`01-boot-shim-bootstrap.md:3182-3232`（命令、逐文件表与 136/195）、`:3234`（store 8 条、paging 17 条）、`:3249`（已删测试的历史说明）、`:2531-2588`（装载引文与“文件字节数”）、`:2593-2627`（UbootFileLoader 引文）。
  - 源码计数：`grep -c '#\[test\]'` 对 `os/boot-shim/src/{loader,resv_claims}.rs`、`os/libs/minix-boot/src/{kernel_info,memmap_source,handoff_v1_read}.rs`、`os/libs/minix-elf/src/lib.rs`、`os/kernel/src/{dm_coverage,lib}.rs`、`os/arch/src/x86_64/paging.rs`。
  - 源码实现：`os/boot-shim/src/opensbi_helpers.rs:197-215`、`os/boot-shim/src/loader.rs:507-520`、`os/kernel/src/lib.rs` 的 `test_store_kernel_info_*` 只有 7 个（1 + 6）。
- 若我错了：若评审基准是某个更早提交的测试快照（例如 v10 提交前），这些数字可能自洽；但文档同时自称 v10/D-062，且 §5.2.1 明写“当前值以本逐文件表为准”。若 `cargo test --features test-all --lib` 对 `loader.rs` 的 31 个 `#[test]` 有 9 个因 cfg 被过滤，逐文件表也应写出过滤条件；我检查的 `mod tests` 没有这类 cfg，且文档的计数定义没有排除它们。

## 完成声明

- CLAIM 条数：6（CLAIM 01–06）。
- 结果分布：反证失败 2（CLAIM 01、02）；阻断 4（CLAIM 03 P0-fact、04 P0-fact、05 P1、06 P1）；非阻断 0。
- 说明：CLAIM 01/02 的“反证失败”指文档主张在文档自设口径内成立；CLAIM 03–06 的反例均落在当前树源码位置或文档原句上。未修改任何源码、文档或轨迹文件，仅写入本报告。
