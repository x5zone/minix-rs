# 反证报告：01 · Boot-shim 引导准备与控制权交接

反证对象：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
取证根：`/home/xzhao/github/minix-rs`；下文 `path:line` 均相对于该根。

## CLAIM 01

- 主张：x86-32 的第一跳必须先从汇编设栈；`multiboot_init` 的第一条指令是 `mov $load_stack_start, %esp`，`load_stack` 是同一文件 `.data` 段里的 4096 字节静态空间，链接链保证 `load_stack_start` 按 4096 对齐；因此 `pre_init` 入口 `%esp` 落在 16 字节边界上，不满足 i386 SysV ABI 的 `%esp ≡ 12 (mod 16)`（文档 §1.1/§2.2，原文见文档第 32–36、256–286 行）。

- 反证尝试：
  1. 找“入口不是 `MINIX` / 第一条不是设栈”的相反执行路径。`minix3/minix/kernel/arch/i386/kernel.lds:2` 的 `ENTRY(__k_unpaged_MINIX)` 与 `head.S:42-45` 的 `MINIX: jmp multiboot_init` 相连，`multiboot_init` 从 `head.S:44` 的 `mov $load_stack_start, %esp` 开始；`multiboot_init` 内没有先于它的指令，也没有另一条入口臂。
  2. 找“`load_stack_start` 不是段起点 + 4096”的链接布局。`minix3/minix/kernel/arch/i386/Makefile.inc:51` 的 unpaged 对象列表以 `head.o` 为首，`:83` 的 `OBJS.kernel += ${UNPAGED_OBJS}` 保持该顺序；`kernel.lds:17-18` 的 `.unpaged_data` 从 `ALIGN(4096)` 开始按命令行顺序收 `unpaged_*.o(.data .rodata*)`；`head.S:93-96` 的 `.data` 以 `load_stack: .space 4096` 开头，`load_stack_start` 正好在 +4096。
  3. 重算入栈序列，寻找“实际是 `≡12`”的可能。`load_stack_start` 记作 top：`push $0; popf` 净零；`push $0` 得 top−4；`push %ebx` 得 top−8；`push %eax` 得 top−12；`call` 压返回地址后入口 esp = top−16。top 是 4096 的倍数，所以入口 `%esp ≡ 0 (mod 16)`，不是 12。
  4. 找 ARM32 或 boot2 的反例。ARM32 的 `earm/head.S` 是同一模式（`ldr sp, =load_stack_start`），不构成相反路径；boot2 的交棒寄存器/栈契约文档已在 §1.4 明确划出取证边界，没有把“Multiboot 规范必要性”推广到 boot2。

- 结果：反证失败（主张成立；文档同时把 boot2 的必要性限定为代码事实，这一限定与源码一致）。

- 证据：
  - `minix3/minix/kernel/arch/i386/head.S:42-53`（`MINIX` 跳到 `multiboot_init`；`mov $load_stack_start, %esp`；三组 push；`call pre_init`）。
  - `minix3/minix/kernel/arch/i386/head.S:93-96`（`.data` / `load_stack: .space 4096` / `load_stack_start:`）。
  - `minix3/minix/kernel/arch/i386/Makefile.inc:51,83`（`head.o` 在 unpaged 列表首位；列表按原序进入 `OBJS.kernel`）。
  - `minix3/minix/kernel/arch/i386/kernel.lds:2,17-18`（入口符号；`.unpaged_data ALIGN(4096)`）。
  - `minix3/minix/kernel/arch/i386/pre_init.c:225-237`（`pre_init` 的入口与返回 `&kinfo`）。

- 若我错了：只要源码在 `multiboot_init` 之前插入一条改 `%esp` 的指令，或链接脚本不再把 `head.o` 的 `.data` 放在 `.unpaged_data` 的首个 `ALIGN(4096)` 起点，或 `load_stack_start` 不再是该数组的高端符号，这条主张就会被推翻。当前三点均未出现。

## CLAIM 02

- 主张：C 的 `alloc_lowest(cbi, len)` 在唯一调用点把**长度**当作 `cut_memmap` 的**结束地址**传入；在文档列出的四个前提下（`arch_init` 先切掉低页、最低可用基址 ≥ 0x1000、`roundup(tramp_size) = 0x1000`、`alloc_lowest` 无第二调用点），真实调用落在“`len ≤ lowest`”一行，切除是空操作；因此跳板内存仍留在空闲清单上，这是潜在缺陷而不是当前可复现故障（文档 §2.4.1.2/§2.4.1.3，原文见文档第 710–735 行）。

- 反证尝试：
  1. 找第二个调用点。`grep -rn "alloc_lowest" minix3/` 只命中 `minix3/minix/kernel/arch/i386/pg_utils.c:65`（定义）、`.../include/arch_proto.h:204`（原型）、`.../arch_smp.c:76`（调用）；`minix3/minix/kernel/arch/earm/include/arch_proto.h:37` 只有原型，没有 ARM 定义或调用。第二条调用点不存在。
  2. 找“`lowest < len`”的调用序。`minix3/minix/kernel/arch/i386/arch_system.c:277-278` 在 `arch_init` 里先 `cut_memmap(&kinfo, 0x0, 0x4FF)` 与 `cut_memmap(&kinfo, 0x90000, 0xFFFFF)`；`cut_memmap` 把起点向下、终点向上取整（`pg_utils.c:30-43`），第一条实际切掉 `[0, 0x1000)`；`add_memmap` 又把每条记录起点向上对齐到页（`pg_utils.c:99`），所以此后最低可用基址不可能低于 0x1000。
  3. 找“`tramp_size` 超过 0x1000”的当前配置。`trampoline.S:55-63` 的布局是 4+4+8+8 字节指针/描述符槽 + `GDT_SIZE*8` + `IDT_SIZE*8`；`archconst.h:11,19-21,31` 给出 `IDT_SIZE=256`、`GDT_SIZE=6+CONFIG_MAX_CPUS`、`DESC_SIZE=8`；`kernel.h:7-8` 的默认 `CONFIG_MAX_CPUS=1`，所以 GDT 是 7×8=56 字节，IDT 是 2048 字节，加上 16 位代码与 24 字节指针槽约 2.2 KiB，`roundup` 后 4096；`len = 0x1000`。
  4. 找“空操作不成立”的细节。`alloc_lowest` 传入的 `lowest` 至少 0x1000，`len` 为 0x1000；`cut_memmap(cbi, 0x1000, 0x1000)` 在 `pg_utils.c:30-43` 的取整后仍是 `[0x1000, 0x1000)`，循环里 `substart >= subend` 直接 `continue`（`pg_utils.c:54-55`），一次都不切。文档的“第三行常态”成立。

- 结果：反证失败（主张成立；文档把结论标成“四前提下”的条件结论，而不是结构保证，这一点也与源码相符）。

- 证据：
  - `minix3/minix/kernel/arch/i386/pg_utils.c:65-84`（`alloc_lowest` 的唯一调用 `cut_memmap(cbi, lowest, len)`）。
  - `minix3/minix/kernel/arch/i386/pg_utils.c:30-63`（`cut_memmap` 的起止语义与空区间 `continue`）。
  - `minix3/minix/kernel/arch/i386/arch_smp.c:68-76`（`tramp_size` 与唯一调用点）。
  - `minix3/minix/kernel/arch/i386/arch_system.c:277-278`（`arch_init` 的两条低段切除）。
  - `minix3/minix/kernel/arch/i386/trampoline.S:55-63`、`.../include/archconst.h:11,19-21,31`、`minix3/minix/kernel/kernel.h:7-8`（表尺寸与默认 CPU 数）。
  - 复算命令：`grep -rn "alloc_lowest" minix3/`（仅上述四个命中）。

- 若我错了：只要实际构建把 `CONFIG_MAX_CPUS` 调到足以让 `GDT_SIZE*8 + 2048` 超过 4096，`len` 就会变成 0x2000 或更大，调用落入第二行（`0 < lowest < len`，部分切除），文档的“第三行”结论就不再成立；或者若 `arch_init` 的两条 `cut_memmap` 被移到 `smp_init` 之后、或最低 E820 可用区落在 0x1000 以下，前提二也会失效。文档已明确写出“任何一条前提变化就要重推”，所以这不是文档错，而是它自带的失效条件。
  - 附带观察（非阻断，P2）：文档 §3.5 说“`add_memmap` 对零长请求静默吸收”，这句话不是无条件的。`pg_utils.c:97` 的 `assert(cbi->mmap_size < MAXMEMMAP)` 在 `pg_utils.c:98` 的 `if(len == 0) return;` **之前**；若 `mmap_size` 已达 40，零长请求会在容量断言处 panic，而不是静默返回。当前启动序里 `mmap_size` 远小于 40，所以 CLAIM 02 的第三行结论不受影响，但绝对措辞应收窄为“容量未满时零长请求静默吸收”。

## CLAIM 03

- 主张：Rust 侧把 C 的全局 `kinfo` 改为按值交接的 `KernelInfo`；结构恰有 14 个字段，`bootstrap_start`/`bootstrap_len`/`free_upper_idx` 三个字段已删除；接收方在消费前把它深拷进七个 `.bss` 落地区加一个名字池，容量分别为 256/128/12/4/1024/296/20，溢出即 panic；wire 版本为 9，信封 160 字节（文档 §3.4/§3.5/§3.9/§4.5，原文见文档第 1575–1634、2751–2863 行）。

- 反证尝试：
  1. 数 `KernelInfo` 字段，找第 15 个或“已删字段仍在”。`os/libs/minix-boot/src/kernel_info.rs:22-184` 的字段为 `memmap`、`reserved_regions`、`memmap_sources`、`startup_objects`、`kern_virt_base`、`kern_phys_base`、`kern_size`、`user_sp`、`kern_stack_top`、`syscall_entry`、`boot_modules`、`platform_sources`、`param_buf`、`authority`，共 14 个；`grep -rn "bootstrap_start\|bootstrap_len\|free_upper_idx" os/kernel/src os/libs/minix-boot/src os/kernel-image/src` 只命中 `handoff_v1.rs` 的版本注释、`misc.rs` 的 `freepde_start` 注释和 `lib.rs` 的取证注释，没有字段定义。
  2. 找“漏掉一个落地区”。`os/kernel/src/lib.rs:1453-1563` 的 `store_kernel_info` 依次处理 `reserved_regions`（1471）、`memmap`（1483）、`boot_modules`（1484–1528，手工循环）、`platform_sources`（1530）、`param_buf`（1531–1542，先 `fill(0)` 再复制）、`memmap_sources`（1549）、`startup_objects`（1550–1552），随后把七个切片的指针改指到 store（1553–1562）；名字池由 `BOOT_MODULE_NAME_STORE` 单独承担。没有第八个需要深拷的切片。
  3. 对容量常量。`os/kernel/src/globals.rs:295,325,341,364,378,398,400` 给出 256、128、16（名字槽，复制时截到 15）、4、1024、`WIRE_SOURCES_CAP`、`WIRE_STARTUP_CAP`；`os/libs/minix-boot/src/handoff_v1.rs:375-411` 给出 128/256/4/1024/296/20；`handoff_v1.rs:436` 断言 `size_of::<BootHandoffV1>() == 160`，`:288` 是版本 9。三项数字与文档表一致。
  4. 找“溢出不 panic”的分支。`land_slice` 在 `lib.rs:1428-1431` 用 `assert!` 拒绝超容量；`boot_modules` 在 `:1487-1490`、`param_buf` 在 `:1533-1536` 各自有独立断言；名字池是唯一允许截断的（`:1494` 取 `min(BOOT_MODULE_NAME_LEN - 1)`，`:1514-1516` 用 `from_utf8` 的合法前缀）。没有静默截断其他落地区的路径。

- 结果：反证失败（主张成立；字段数、落地区数、容量和 wire 尺寸均与源码一致）。

- 证据：
  - `os/libs/minix-boot/src/kernel_info.rs:22-184`（14 个字段；无 `bootstrap_*`/`free_upper_idx`）。
  - `os/kernel/src/lib.rs:1427-1563`（`land_slice` 与 `store_kernel_info` 的七个落地区）。
  - `os/kernel/src/globals.rs:295,325,341,364,378,398,400`（落地区容量）。
  - `os/libs/minix-boot/src/handoff_v1.rs:288,375-411,436`（版本 9、wire 容量、160 字节信封）。
  - `os/kernel/src/lib.rs:5836,5878,5928-6017`（测试调用点，排除在生产三调用点之外）。

- 若我错了：只要 `KernelInfo` 在生产构建里多出第 15 个切片字段、`store_kernel_info` 漏拷其中任何一个、或某个落地区的超容量行为从 `panic` 变成截断，这条主张就会被推翻。当前源码没有这些路径。

## CLAIM 04

- 主张：引导期页表页分配器由 `pt_alloc` 的**写一次**注册槽承担；二次 `register` 在两种构建档都 panic，`alloc_pt_page` 在未注册时以前置断言停机；boot 侧的 bump 分配器只由各生产入口 `init_boot_pt_alloc` 设定，内核侧原来“从 memmap 切兜底区间”的路径已删除（文档 §3.8/§4.1/§4.7，原文见文档第 2091–2220、2296–2300 行）。

- 反证尝试：
  1. 找“二次注册被允许”的分支。`os/arch/src/arch/pt_alloc.rs:100-101` 用 `PT_CLAIMED.swap(true, AcqRel)` 后 `assert!(!claimed, ...)`；这是普通 `assert!`，不是 `debug_assert!`，release 档同样生效。没有重置 `PT_CLAIMED` 的代码，也没有“后注册者覆盖前者”的分支。
  2. 找“未注册也能分配”的路径。`alloc_pt_page` 在 `pt_alloc.rs:129-132` 先 `assert!(is_registered(), ...)`；`is_registered` 只是 `PT_REGISTERED.load(Relaxed)`。没有返回默认分配器的 fallback。
  3. 找“从 memmap 切兜底区间”的执行路径。`grep -rn "init_boot_pt_alloc\|boot_alloc::init\|boot_alloc_region" os/kernel/src os/boot-shim/src os/kernel-image/src` 只显示四个生产初始化点：`uefi_helpers.rs:995`、`uboot_main.rs:451`、`bootface.rs:280`、`bootface_a64.rs:257`；`os/kernel/src/lib.rs:884-897` 的注释与 `require_boot_region` 明确说明兜底已删除，缺失即 fail-closed。
  4. 找“VM 与内核共用同一份注册槽导致二次注册”的架构分支。VM 在 `os/servers/vm/src/vm_server.rs:382-391` 用自己的 `is_registered()`/`is_free_registered()` 守卫注册；它是独立用户态地址空间里的另一份静态量，不与内核或 boot-shim 的副本共享。三个二进制（boot-shim、内核镜像、VM）各自注册一次，符合文档的“每个二进制各有一份 `PT_REGISTERED`”。

- 结果：反证失败（主张成立；没有相反执行路径。`pt_alloc_write_once.rs` 与 `pt_alloc_concurrent.rs` 的测试也覆盖了两种档与并发双写）。

- 证据：
  - `os/arch/src/arch/pt_alloc.rs:91-140`（写一次注册、reader 发布、`alloc_pt_page` 前置断言）。
  - `os/kernel/src/boot_alloc.rs:176-177,184-186,210-212`（只有 `init/resume/region`，没有 memmap 兜底）。
  - `os/kernel/src/lib.rs:884-897`（`require_boot_region`；注释明说兜底已删除）。
  - `os/servers/vm/src/vm_server.rs:382-391`（VM 侧独立注册）。
  - `os/arch/tests/pt_alloc_write_once.rs`、`os/arch/tests/pt_alloc_concurrent.rs`（两种构建档的二次注册与并发双写）。
  - 附带观察（非阻断，P2）：`os/kernel/src/vm_handoff.rs:228-230` 仍写着 “The no-shim fallback path registers its bump region *from* the memmap”，与 `lib.rs:887-891` 的“已删除”注释和实际调用图相反。这是一条陈旧源码注释，不是可执行路径；它不推翻本主张，但会误导读者，属于源码侧文档债。

- 若我错了：只要存在一个 `cfg` 分支在 `PT_REGISTERED` 为假时返回某个默认页表页（例如从 memmap 直接切一段），或某个生产入口直接写 `PT_ALLOC` 而不经 `register` 的 `PT_CLAIMED` 占位，这条主张就会被推翻。当前 grep 不到这样的分支。

## CLAIM 05

- 主张：跨镜像三腿的接收侧只做**加性**阶段，绝不重建自举根；`arch_boot_resume_v1` 把 arena 解码进 `.bss` store 后，解码就是 arena 的最后一次读，VM 自由页在解码之后才发布，因此 arena 落在 conventional 区间时其页可以重新发给 VM（文档 §3.7/§4.7/§4.8，原文见文档第 1931–1979、3035–3096 行）。

- 反证尝试：
  1. 找“接收侧重建根”的调用。`os/kernel/src/lib.rs:326-375` 的 `resume_additive` 只调用 `boot_validate_and_prepare`、`plan_dm_policy`、`set_current_root_phys`、`store_kernel_info`、`register_panic_diagnostic`、`establish_boot_dm`、`jump_to_kmain`；`build_bootstrap_root_and_enable` 只在发送侧 `prepare_kernel_image_v1`（`lib.rs:490` 附近）出现，不在接收链上。`lib.rs:304-318` 的文档注释明确写出重建会清零活动根并导致下一次取指故障。
  2. 找“解码之后还读 arena”的路径。`lib.rs:645-741` 的 `arch_boot_resume_v1` 用 `decode_envelope`、`decode_memmap`、`decode_reserved_regions`、`decode_modules`、`decode_platform_sources`、`decode_param_buf`、`decode_sources`、`decode_startup_objects` 把七类载荷复制进各自 store，然后构造 `KernelInfo` 的切片指向这些 store；`arena` 变量在函数结束后不再被引用。平台源记录里是 DTB/RSDP 的物理地址，不是 arena 内指针；模块名被复制进名字池。没有“解码后再从 arena 读”的路径。
  3. 找“arena 页仍被永久扣除”的相反分类。`os/kernel/src/vm_handoff.rs:18-34` 的扣除表只列内核镜像、VM 根页、bump 整段、bootstrap 已用后缀、全部模块 blob；没有 arena 行。`vm_handoff.rs:1026-1027` 的宿主测试 `decoded_map_survives_arena_poison_and_reissues_the_arena_frames` 正是“解码后整块 poison，arena 页按生命周期重新发给 VM”的断言。
  4. 找“U-Boot shim 的 arena 落在 bump 整段扣除内”的分配序。`os/boot-shim/src/opensbi_helpers.rs:347-365` 先分配 root 页与页表子区间，`:421-424` 在模块之后分配 arena；`bump` 返回的只是页表子区间 `[bump_base, bump_end)`，arena 在其后，因此不属于整段扣除。`startup_objects` 仍记录 arena 的身份（`opensbi_helpers.rs:426-436`），与“记录它曾经是什么，不是它还活着”一致。

- 结果：反证失败（主张成立；接收链上没有重建根，arena 也没有被保留为活引用）。

- 证据：
  - `os/kernel/src/lib.rs:304-375`（`resume_additive` 的加性序列与 `build_bootstrap_root_and_enable` 的禁止说明）。
  - `os/kernel/src/lib.rs:645-741`（`arch_boot_resume_v1` 的完整解码与 `resume_after_v1_decode` 分派）。
  - `os/kernel/src/vm_handoff.rs:18-34,1026-1027`（扣除表不含 arena；poison 测试）。
  - `os/boot-shim/src/opensbi_helpers.rs:347-365,421-436`（U-Boot shim 的 root/bump/模块/arena 分配序与 startup 记录）。

- 若我错了：只要接收侧某个架构委托壳再次调用 `build_bootstrap_root_and_enable`/`Paging::new_from_page`，或某个消费者在 `arch_boot_resume_v1` 返回后仍解引用 arena 指针，或 `classify` 把 arena 加进扣除记录（从而不再重发），这条主张就会被推翻。当前源码没有这三条路径。

## CLAIM 06

- 主张：文档对“证据缺口/接线状态”的两条声明是准确的：① aarch64 直核面“目前只有 L0/L3 的间接基址对账，跨距没有等价断言”（文档第 1653 行）；② 五条门链“当前 CI 只接线了 `test-riscv64-uboot.sh`，其余四条是手工门”（文档第 3299 行）。

- 反证尝试：
  1. 对①：`os/kernel-image/aarch64.ld:116-117` 有一条链接期 `ASSERT(__bss_end - KERNEL_VIRT_BASE <= 0x400000, ...)`；`os/kernel-image/riscv64.ld:124-125` 同样有一条。这不是文档所说的“等价断言”（它只保证镜像不**超过** `KERN_SIZE`，不保证等于），但它确实在链接期守住了“镜像比常量更长”这个危险方向；`aarch64.ld:110-117` 的注释还明确说“链接期直接失败，不留运行时隐患”。因此“aarch64 只有 L0/L3 的间接基址对账”是不完整的：还有一条链接期上界断言，且它正好覆盖了会导致高半少映射半个镜像的那一侧。文档的“没有等价断言”在严格意义上仍成立（缺的是 L9 那种等值对账），但对风险证据的描述低估了现有保护。判 P2，不判 P0/P1：没有代码缺陷，也没有被误报的安全缺口的反向证据。
  2. 对②：`.github/workflows/cmd-smoke.yml:53-57,79-87` 在 CI 里运行 `test-cmd-smoke.sh`（三条矩阵腿之一），而 `os/qemu-tests/test-cmd-smoke.sh:137-153` 的 Stage 3b 逐条断言了 x86 shim 的三条交接路标：`boot-shim: kernel loaded`、`boot-shim: 12 boot modules loaded`、`boot-shim: [raw] entering kernel image v1`。`.github/workflows/qemu-tests.yml:86-87` 确实只单独接线了 `test-riscv64-uboot.sh`；`os/qemu-tests/run_all.sh` 也不包含五条 bootmark 脚本。因此“五条**专用脚本**里只有 riscv64 U-Boot 直载测试链进 CI”成立，但“CI 只接线了一条启动链”在读者层面不成立：x86 UEFI 链的三条 shim 路标已经由 cmd-smoke 作业在 CI 常态守护，只是不覆盖接收端与 `entering scheduler` 两条。判 P2，属覆盖状态的表述精度问题。
  3. 对①的进一步核对：`os/kernel-image/check-layout.sh:248-260` 的 L9 只在 `arch = riscv64` 时执行，确实没有 aarch64 等值对账；`os/kernel-image/aarch64.ld` 的 ASSERT 是上界而不是等值，所以文档的核心结论（aarch64 缺 L9 等值对账）没有被推翻。

- 结果：非阻断（两处证据/覆盖声明的精度问题，均判 P2；文档的核心设计结论不受影响）。

- 证据：
  - `os/kernel-image/aarch64.ld:110-117`（链接期 `ASSERT(__bss_end - KERNEL_VIRT_BASE <= 0x400000, ...)`）。
  - `os/kernel-image/riscv64.ld:118-125`（同一上界断言；与 `check-layout.sh:248-260` 的 L9 等值对账并存）。
  - `os/kernel-image/check-layout.sh:248-260`（L9 仅 riscv64）。
  - `.github/workflows/cmd-smoke.yml:53-57,79-87`（CI 矩阵含 `test-cmd-smoke.sh`）。
  - `os/qemu-tests/test-cmd-smoke.sh:137-153`（Stage 3b 的三条 x86 shim 路标）。
  - `.github/workflows/qemu-tests.yml:86-87`、`os/qemu-tests/run_all.sh`（五条专用 bootmark 脚本不在 CI；`run_all.sh` 中 grep 不到 `shim-bootmarks|uboot-shim|bootface`）。

- 若我错了：若 `aarch64.ld` 的 `ASSERT` 在构建时被跳过（例如 xtask 不链接该脚本）、或 `test-cmd-smoke.sh` 只在本地矩阵而不在 CI 的 `cmd-smoke` 作业里运行、或 Stage 3b 的三条标记不是 x86 UEFI 的 shim→内核交接，那么这两条反证都不成立，文档原声明应恢复为准确。当前三条前提都由源码与 workflow 直接支撑。

另附两条 P2 观察，不单独计 CLAIM：① `add_memmap` 的零长“静默吸收”以容量未满为前提（`pg_utils.c:97-98`）；② `vm_handoff.rs:228-230` 的“no-shim fallback”是陈旧源码注释，实际兜底已删除（`lib.rs:884-897`）。

- 本次反证：6 条 CLAIM（阻断 0 / 非阻断 1 / 反证失败 5）。
