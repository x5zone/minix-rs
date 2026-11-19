# P-FACT-B 审查报告 — 01-boot-shim-bootstrap.md（R33）

- **角色**：P-FACT-B（只读事实核查；对照 `os/` Rust 源码与测试）
- **目标文档**：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
- **基线核验**：sha256 = `14a27c145d070c480600d72d7276bef5f3d030e5248c4b758196651dca3a1d37`，与 R33 基线前缀 `14a27c145d070c48` 一致；行数 3565，一致。
- **范围**：第 3 章（§3.1–§3.10，行 1391–2256）、第 4 章（§4.1–§4.9，行 2257–3118）、§5（行 3119–3312）、§6（行 3313–3496）中**引用 Rust 代码/测试/日志的断言**。C 侧断言（minix3/）不在本次射程。
- **证据强度**：L1（工具输出）用于四组 cargo test 主读数与逐文件 `#[test]` 计数；其余为 L2（grep/sed 手工核对），全部附命令与文件:行号。
- **结论**：**发现 0 条**（P0=0，P1=0，P2=0，P3=0）。本轮对约 150 条离散断言逐条核对，未发现文档与源码不符之处。另有 2 条非缺陷类 SUGGESTION。

---


（无。本轮未发现任何文档真实缺陷。）

---

## SUGGESTION S1：§3.4.5 的 16 KiB 容量断言可补文件锚点

- **位置**：§3.4.5 第 4 条 bullet（"直核自举面同构（DESIGN-09）"段），原文："容量断言 `size_of::<BootStorage>() <= 16 KiB` 约束的是这块 `.data` 的大小，不是 `.bss`。"
- **源码证据**：`os/kernel-image/src/bootface_common.rs:16` — `const _: () = assert!(core::mem::size_of::<minix_boot::BootStorage>() <= 16 * 1024);`
- **说明**：断言真实存在、数值正确（16 KiB = 16 × 1024），但它的落点在 `kernel-image` 的 `bootface_common.rs`（直核面），而 `BootStorage` 定义在 `os/libs/minix-boot/src/boot_storage.rs`——在 minix-boot 包里 grep 不到这条断言。文档按上下文（该 bullet 讲的就是两个 bootface）语义上不算错，但未给锚点，读者在定义侧找断言会落空。
- **建议**：补一句"（断言在 `os/kernel-image/src/bootface_common.rs:16`）"或给出复算命令。
- **严重度**：不构成缺陷（SUGGESTION）。
- **反证路径**：`grep -rn "size_of::<BootStorage" os/ --include="*.rs"` 仅命中 bootface_common.rs 一处。

## SUGGESTION S2：§4.2 的 "AllocateAddress" 可补 Rust 侧拼写

- **位置**：§4.2.1 动作 2 bullet（"读 ELF → `reserve_kernel_span` 用 `AllocateAddress` 把 … 预订为 `LOADER_DATA`"）与 §4.2 引文的旁注 `// AllocateAddress([kern_phys_base, +kern_size), LOADER_DATA)`。
- **源码证据**：`os/boot-shim/src/uefi_helpers.rs:79` — `AllocateType::Address(base)`；源码注释层也混用两词（`uefi_helpers.rs:57` 用 `AllocateType::Address`，`uefi_helpers.rs:984`、`1424` 与 `opensbi_helpers.rs:388` 用 "AllocateAddress"）。
- **说明**：文档用的是 UEFI 规范的分配类型名（EfiAllocateAddress），与源码注释的用词一致，语义无误；但 uefi-rs 0.33 的枚举变体拼写是 `AllocateType::Address`，按 "AllocateAddress" grep Rust 代码会落空。
- **建议**：首次出现处补一次 Rust 侧拼写（如"用 `AllocateAddress`（Rust 侧 `AllocateType::Address`）"）。
- **严重度**：不构成缺陷（SUGGESTION）。
- **反证路径**：`grep -rn "AllocateAddress" os/ --include="*.rs"` 仅命中注释；`grep -n "AllocateType::Address" os/boot-shim/src/uefi_helpers.rs` 命中实现。

---

## 核对账目（按节汇总；全部为"核对一致"）

### 第 3 章

| 断言 | 源码证据 | 结果 |
|---|---|---|
| §3.1 `exit_boot_services` 为 `qemu-test-kernels` 门控、返回 map 可丢弃、best-effort 非 MapKey 配对 | `os/boot-shim/src/uefi_helpers.rs:1229-1241`（引文逐字一致） | 一致 |
| §3.1.1 riscv64 UEFI 目标缺失；五条腿接线状态 | `boot-shim/Cargo.toml`（uefi 依赖与 target 无关）、各腿代码 | 一致 |
| §3.2 `BootShim` trait 已删除 | `grep -rn "impl BootShim\|trait BootShim" os/` 零命中 | 一致 |
| §3.2 `prepare_boot(&mut BootStorage, bump_pages) -> BootPrepareResult<'a>` 签名；`uboot_main` 直接调用 | `opensbi_helpers.rs:244-247`、`uboot_main.rs:411,444` | 一致 |
| §3.2 `BootPrepareResult` 四字段（report/root_page/bump/arena_pa） | `minix-boot/src/boot_prepare.rs`（逐字一致） | 一致 |
| §3.2 FileLoader/read_required 默认 panic 文案 | `boot-shim/src/loader.rs:146-161`（逐字一致） | 一致 |
| §3.2 UefiFileLoader read：SimpleFileSystem + 斜杠替换 + Owned | `uefi_helpers.rs:1247-1259` | 一致 |
| §3.2 UbootFileLoader read：Borrowed + 按 PT_LOAD 尾修剪幂等 | `opensbi_helpers.rs:198-221` | 一致 |
| §3.2 bootface_common：NAME_SLOT=16、NAME_POOL 12 槽、五道放置检查、编译期逐字节对账、PlacementPool 命名字段 | `kernel-image/src/bootface_common.rs:16-65,177-289` | 一致 |
| §3.2 riscv64 池 `0x8200_0000..0x8400_0000` | `opensbi_helpers.rs:69`（MODULE_REGION_BASE=0x8000_0000+0x0200_0000） | 一致 |
| §3.2 lib.rs 7 处固件 feature 属性（6 uefi + 1 opensbi） | `boot-shim/src/lib.rs:221-249`（第 8 个 grep 命中是文档注释） | 一致 |
| §3.2 uefi_helpers 8 处 qemu-test-kernels 门，覆盖名单 | `uefi_helpers.rs:157,160,394,406,409,418,523,1229` | 一致 |
| §3.2 MockLoader 存在且 `impl FileLoader` | `loader.rs:496,509` | 一致 |
| §3.2 main 调 `prepare_and_enter_kernel_image(boot_shim::BOOT_PT_BUMP_PAGES)`；bin required-features | `main.rs:75`、`Cargo.toml:64` | 一致 |
| §3.3 四份依赖表（boot-shim/kernel/minix-boot/kernel-image + 两张 target 表） | 四个 Cargo.toml 逐条核对 | 一致 |
| §3.3 boot-shim→minix-arch 仅 `boot_dm_admissible_end`；→minix-kernel 仅 `enter_kernel_image_v1`+`init_boot_pt_alloc` | `grep minix_arch::/minix_kernel:: boot-shim/src` | 一致 |
| §3.4.1 KernelInfo 14 字段 | `kernel_info.rs`（14 个 pub 字段） | 一致 |
| §3.4.1 落地区 128 槽 / VM 交接 64 / deducted 47 | `globals.rs:325`、`minix-types/src/types/boot.rs:107,122` | 一致 |
| §3.4.1 非空 `memmap_sources` 的 lenient 覆盖审计；strict 另查 startup 盖内核、派生标量、MODULE_NAMES 位置核名 | `kernel_info.rs:445-545`（else-if 分支、`requires_startup_kernel_record`、三 `assert_eq!`、按位置核名） | 一致 |
| §3.4.1 authority 绑定（wire 解码读 `ram_authority`；UEFI 腿 `UefiMemoryMap`；U-Boot shim 发送侧 `DtbBootInput`；`validate_requiring_sources()` 无参） | `handoff_v1.rs:299-335`、`uefi_helpers.rs`（Serializer::new(…, UefiMemoryMap)）、`kernel_info.rs:264-266` | 一致 |
| §3.4.2 GET_KINFO 载荷：KinfoStruct 48 B、七语义字段、`freepde_start` 恒填 0、release/version 来自 minix-types | `minix-types/src/types/kinfo.rs:25-44,54`、`kernel/src/misc.rs:711-737` | 一致 |
| §3.4.2 "kernel 与 minix-boot 读 board 键零命中" | `grep -rn '"board"' os/kernel/src os/libs/minix-boot/src` 零命中 | 一致 |
| §3.4.3 BootHandoffV1 逐字段；ARENA_CAP=8×4096；30624 由断言钉住、余量 2144；ENVELOPE_CAP=256；296=256+40；72 B/条 | `handoff_v1.rs:288-418,461-495` | 一致 |
| §3.4.3 v9 删除 6 KiB 语义工作分区、信封 160 B | 结构体算术 160 B ✓；`handoff_v1.rs` 无工作分区常量 | 一致 |
| §3.4.5 BootStorage 非全零 → `.data`；source 槽 40（DTB_SOURCE_CAP=8+8+16+8）、startup 20；UEFI storage source/startup 槽空 | `boot_storage.rs:54-121`、`memmap_source.rs:236`、`uefi_helpers.rs`（分类器直写 wire） | 一致 |
| §3.4.5 半提交防线的两档：内存表 `map_dirty`/`commit_map_tables`/report() panic；其余五族 one-shot set_*_len | `boot_storage.rs`（逐条方法核对） | 一致 |
| §3.4.5 BootStorageCell::take one-shot panic | `boot-shim/src/lib.rs:143-172` | 一致 |
| §3.4.5 FileBytes Borrowed/Owned 三条合同；`into_owned` | `loader.rs`、`opensbi_helpers.rs:216-219` | 一致 |
| §3.5 wire 7→8 释放 16+8=24 B、envelope 184→160；x86_64 误设区间 `[0, 0x200000)`；opensbi_helpers 测试模块保留"结构上不可能"回归说明；`bootstrap_len must be 0` 现行 grep 不到 | 结构体算术 ✓、`x86_64.ld:27`、`opensbi_helpers.rs:874-876`、全仓 grep 零命中 | 一致 |
| §3.6 HugePages trait 形状；x86 常量 `[GIB_1, MIB_2]`；supports_huge_page 查 CPUID.80000001H:EDX.GBPAGES；map_huge PDPT_SHIFT 分支与 AlreadyMapped 守卫 | `arch/src/arch/paging_ext.rs`、`arch/src/x86_64/paging.rs`（引文逐字一致） | 一致 |
| §3.6 Paging 22 成员（1 常量 + 21 方法，含 enable/destroy/flush_tlb/flush_tlb_addr） | `arch/src/arch/paging.rs:186-456` 逐个计数 | 一致 |
| §3.6 plan_kernel_mapping 尺寸阶梯四分支（含 FallbackUnsupported 优先于 NoKernelPageSize） | `kernel/src/boot_policy.rs`（引文逐字一致） | 一致 |
| §3.7 arch_boot_finish 引文（validate_before_tables → check_boot_postconditions → store → register_panic_diagnostic → arch_boot_impl → jump） | `kernel/src/lib.rs:218-239`（逐字一致） | 一致 |
| §3.7 enter/prepare_kernel_image_v1、jump_kernel_image_v1（aarch64 `br {entry}` + `in("x0") arena_pa`） | `kernel/src/lib.rs`（引文逐字一致） | 一致 |
| §3.7 resume_additive 文档注释（TTBR1 免疫 + 加性序列枚举） | `kernel/src/lib.rs`（引文一致；"结尾 10 行"省略声明与实际相符） | 一致 |
| §3.7 六脚本 fdtput 注入 `opensbi,boot-file-table` | `grep fdtput os/qemu-tests/*.sh` 恰为文档列出的 6 个 | 一致 |
| §3.8 pt_alloc 引文（PT_CLAIMED/PT_REGISTERED/release-acquire 配对、双档 fail-closed） | `arch/src/arch/pt_alloc.rs`（逐字一致） | 一致 |
| §3.8 BootAlloc::alloc 引文（BumpRegion::resumed + 512×u64 清零 + not(test) 门） | `kernel/src/boot_alloc.rs`（逐字一致） | 一致 |
| §3.8 三次注册账（init 不注册；boot_validate_and_prepare 守卫注册；VM `is_registered`/`register_free`） | `uefi_helpers.rs:995`、`kernel/src/lib.rs`、`vm_server.rs:382-391` | 一致 |
| §3.8 U-Boot 腿 `result.bump.start()/end()` 传 `init_boot_pt_alloc` | `uboot_main.rs:450` | 一致 |
| §3.8 vm_handoff 扣除表头 + `for_each_live_bootstrap` 调用段 | `vm_handoff.rs:22-30,428-435`（引文一致） | 一致 |
| §3.8 minix-kernel 自身不再调 init（kernel 内仅 dm_coverage 测试调用） | `grep init_boot_pt_alloc os/`（4 生产点全在 shim/kernel-image） | 一致 |
| §3.9 三架构 `inherit_supervisor_half` 锚点；报告位固定 0 | 三 paging.rs 行号 ✓、`misc.rs:721-723` | 一致 |
| §3.10 对照表各行与 §3.5/§3.6/§3.7/§3.8/§3.9 的落点 | 前述各节核对结果 | 一致 |

### 第 4 章

| 断言 | 源码证据 | 结果 |
|---|---|---|
| §4.1 main 引文（1024 页注释骨架）；BOOT_PT_BUMP_PAGES=1024；bootface 两面 PT_BUMP_PAGES=256 | `main.rs:59-75`、`boot-shim/src/lib.rs:258`、`bootface.rs:134`、`bootface_a64.rs:100` | 一致 |
| §4.1 Step 0–4 编号与兜底删除 | `kernel/src/lib.rs`（require_boot_region 停机；无 memmap 兜底） | 一致 |
| §4.2 prepare_and_enter_kernel_image 引文顺序（A+D、storage.take、模块、平台、参数、arena、Serializer、配对循环、raw_serial、commit、三道断言、enter_kernel_image_v1） | `uefi_helpers.rs`（函数体逐段核对，含 DTB preflight 位于配对循环内、startup 补写段约 38 行） | 一致 |
| §4.2.1 动作 4 `alloc_arena` 经 `MaxAddress(bootstrap_alloc_limit())` LOADER_DATA 八页 | `uefi_helpers.rs:645-653,592-596` | 一致 |
| §4.2.1 动作 9 窗口表构成；x86 低 2 MiB AP 跳板窗口；U-Boot shim 窗口 `[SHIM_LINK_BASE, __bss_end)` | `kernel/src/lib.rs`（windows.push(0, DEV_PIECE) x86-only）、`uboot_main.rs:64,71,497` | 一致 |
| §4.2.2 五步判定 + 两道内联闸（含 `Excluded`/`DevicePath` 不经过 `checked_len`） | `ebs_map.rs:594-659`（逐行对应） | 一致 |
| §4.2.2 令牌表：LAST_KNOWN=UNACCEPTED(15)、AMBIGUOUS 五位、PROFILE_ONLY 三位、TYPE_INDEPENDENT=RUNTIME\|NON_VOLATILE\|SPECIAL_PURPOSE、checked_shape/checked_len、pa_limit 为页表地址宽度 | `ebs_map.rs:149-232`、`uefi_helpers.rs:539-549`（aarch64 1<<48、x86 1<<52） | 一致 |
| §4.2.2 `bootstrap_alloc_limit = boot_dm_admissible_end() - 1` 引文 | `uefi_helpers.rs:592-596`（逐字一致） | 一致 |
| §4.3 三条路径常量 + MODULE_NAMES 12 项顺序 + boot-shim 仅重导出 | `loader.rs:31,39,42,71`、`minix-boot/src/kernel_info.rs:685-688` | 一致 |
| §4.3 load_boot_modules_into 引文（ceil 公式、页尾清零、len=文件字节数、缺件 panic 文案） | `loader.rs`（逐字一致） | 一致 |
| §4.3 entry_path_eq + BOOT_FILE_PATH_MAX/表容量夹取 + 构建器 NUL 契约 | `opensbi_helpers.rs:112-181`、`uboot_main.rs`（构建期 assert `does not fit…` 类消息） | 一致 |
| §4.3 ds 位置 0 进程号 6 | `kernel/src/proc.rs:111-124`（ProcNr(6)） | 一致 |
| §4.4 build_kernel_info 引文与字段表（user_sp=USER_STACK_TOP、kern_stack_top=基址+size、syscall_entry=基址） | `boot-shim/src/lib.rs`（逐字一致，限定符剥离声明相符） | 一致 |
| §4.4 装载器断言 `e_entry == min(vaddr)` 两处（UEFI 入口一处 + loader.rs 一处） | `uefi_helpers.rs`（assert_eq entry/kern_virt_base）、`loader.rs:399` | 一致 |
| §4.4 boot_validate_and_prepare 校验段引文 | `kernel/src/lib.rs:856-899`（逐字一致） | 一致 |
| §4.4 四级防线与十三行门表（validate_before_tables 不查 boot_modules 非空等） | `kernel_info.rs:242-263`、`kernel/src/lib.rs` | 一致 |
| §4.4 腿间执行序（直核 strict→pre-table→post→boot_validate→kmain；v1 接收 strict→post→boot_validate，不经过 pre-table；postconditions 调用点在 validate_requiring_sources 之后、resume_boot_pt_alloc 之前） | `kernel-image/src/bootface*.rs`、`kernel/src/lib.rs:645-741` | 一致 |
| §4.5 land_slice/store_kernel_info 引文（含 param.fill(0)、名字池截断 15 字节 + from_utf8 valid_up_to） | `kernel/src/lib.rs`（逐字一致） | 一致 |
| §4.5 容量表 11 行（256/128/12/4/1024/296/20/16/切片/64/47） | `globals.rs:295-435`、`handoff_v1.rs:386,408`、`minix-types/src/types/boot.rs:107,122` | 一致 |
| §4.5 三个调用点都取返回值 | `kernel/src/lib.rs:225,352,1738` | 一致 |
| §4.5 测试内核 write-once 静态表 + qemu-test-kernels 门 | `uefi_helpers.rs:390-416`（TestMapTables） | 一致 |
| §4.6 Step 1/2 循环引文（DEV_PIECE=1<<21、identity_piece_is_device、AlreadyMapped 防御臂、Step 2 守卫） | `kernel/src/lib.rs:1212-1290`（引文一致；文档略去的 `assert!(end <= IDENTITY_MAP_END)` 行与其 TODO-08 描述一致，IDENTITY_MAP_END 是 minix-arch BOOT_IDENTITY_MAP_END 的再导出别名） | 一致 |
| §4.6 设备窗口：aarch64=MMIO 基窗、riscv64=kernel_identity_mmio_regions 全表（PLIC+UART 两项）、x86 空；x86 kernel_identity_mmio_regions 返回 IO-APIC/LAPIC 且无生产读者 | `kernel/src/lib.rs:1003-1017`、`arch/src/lib.rs:255-322`、`vm_handoff.rs:1187`（测试读者） | 一致 |
| §4.6 Step 2b MMIO_VA_BASE + use_high_mmio_window；Step 2c x86/riscv 非空、aarch64 空；ACLINT SSWI bootstrap=false | `kernel/src/lib.rs:1328-1344,1420`、`arch/src/arch/device_window.rs:141-238` | 一致 |
| §4.6 X86_64Paging::enable 引文（EFER.NXE → CR3 → CR0.PG\|WP → CR4.PGE） | `arch/src/x86_64/paging.rs:686-715`（逐字一致） | 一致 |
| §4.6 入口栈对齐与 SSE；`jump_to_kmain` `sub rsp, 8` | `kernel/src/arch/x86_64/higher_half.rs:43-53` | 一致 |
| §4.7 BootAlloc init/resume 引文（is_consistent 断言文案） | `kernel/src/boot_alloc.rs`（逐字一致） | 一致 |
| §4.7 U-Boot 池发放顺序（根页→1024 页子区间→模块→arena）与运行期断言（root=池首页、子区间紧随） | `opensbi_helpers.rs:346-376`、arena 在模块后 `bump_alloc_pages(ARENA_CAP/4096)` | 一致 |
| §4.7 KERNEL_MAX_LMA_END = KERN_PHYS_BASE + 0x40_0000 + 编译期 `<= MODULE_REGION_BASE` | `opensbi_helpers.rs:91-95,379-385` | 一致 |
| §4.7 arena 生命周期合同 + poison 测试名 | `vm_handoff.rs`（`decoded_map_survives_arena_poison_and_reissues_the_arena_frames` 存在） | 一致 |
| §4.8 rust_image_main 三臂引文（aarch64 x0 分道、riscv64 非 U-Boot (hart,dtb)/U-Boot arena 两形态、x86 无直核面） | `kernel-image/src/main.rs:341-416`（逐字一致） | 一致 |
| §4.8 x86 stub `mov rbx, rsi; mov rax, rdi; cli; cld; jmp rax`；`_start` 次序（cli/cld/清 .bss 只碰 rdi/rcx/设栈/mov rdi,rbx/call） | `kernel/src/lib.rs:596-608`、`kernel-image/src/main.rs:136-170` | 一致 |
| §4.8 riscv64.ld 三值（KERNEL_VIRT_BASE=0xFFFFFFC000000000、KERNEL_PHYS_BASE=0x80200000、ENTRY(_start)）；VMA=LMA 测试内核（`link.ld` `. = 0x80200000` 类） | `os/kernel-image/riscv64.ld:39-42`、qemu test link 脚本 | 一致 |
| §4.8 riscv64 jump_to_kmain 的 auipc 执行位置探针（直核加 delta、接收侧不加） | `kernel/src/arch/riscv64/higher_half.rs:30-100` | 一致 |
| §4.8 aarch64 直核 DTB_PA=DRAM_BASE=0x4000_0000 | `bootface_a64.rs:72,87` | 一致 |
| §4.9 表各行 Rust 落点（install_at 存在、FreeRangeSet::cut 被 classify 调用、alloc_pt_page、X86_64Paging::map 等） | `ap_early_entry.rs:244`、`vm_handoff.rs:323,442`、`pt_alloc.rs`、x86 paging.rs | 一致 |

### 第 5 章

| 断言 | 证据 | 结果 |
|---|---|---|
| `cargo test -p boot-shim --features test-all --lib` = 136 passed | 本轮实跑：`running 136 tests / ok. 136 passed` | 一致 |
| 默认 uefi-only 形态 125（136 − opensbi 11） | 本轮实跑：`running 125 tests / ok. 125 passed` | 一致 |
| `cargo test -p minix-boot -p minix-elf` = 196 与 23 | 本轮实跑：196 passed、23 passed | 一致 |
| `cargo test -p minix-kernel --features mock --lib` = 874（866+8 ignored） | 本轮实跑：`running 874 tests / 866 passed; 8 ignored` | 一致 |
| 逐文件 `#[test]` 计数 21 个文件全部 22/11/12/45/17/15/13/6/1/32/8/8/74/2/10/21/29/2/5/5/23 | 本轮复算（同命令） | 全部一致 |
| kernel 侧：boot_alloc 10、vm_handoff 21、dm_coverage 10、x86 paging.rs 17（属 minix-arch）、pt_alloc.rs 为零、pt_alloc_write_once 2、pt_alloc_concurrent 1 | grep 复算 | 一致 |
| store_kernel_info 系列 8 条（1 行为 + 7 超容量负控，fixtures=LEN+1） | `kernel/src/lib.rs:5791-6011` | 一致 |
| FreeRangeSet 行为锁 11 条 | `vm_handoff.rs:1198` 标记后 11 个 #[test] | 一致 |
| 命名测试 22 个全部存在（M3/M6/M4/M13/M14/M16/TODO-05/07、poison、validate 家族 16+3+8+4+1=32） | grep 逐个定位 | 一致 |
| check-layout.sh L0–L8 头注 + L9 riscv64-only；L7 为弱检查（64 KiB 跨度 + 末段内，sp 初值关系不查） | `os/kernel-image/check-layout.sh:10-25,226-260` | 一致 |
| L5 检查 VMA 跨距（§5.2.2 去掉 L5 推演中的口径区分） | `check-layout.sh:205-210`（末段 VMA+memsz − 首段 VMA） | 一致 |
| `build_memmap()` 单表包装被 15 个测试内核调用；生产不调用 | `grep -rln build_memmap os/qemu-tests/test-kernels` = 15 | 一致 |
| 深拷探针读数 `pa=0x5e9b000`、`pre=post_ebs=post_enable=0x4a00_0000_0001_ff00`、`verdict=replayed`、`in_snapshot_free` 口径警示；探针 `MaxAddress(4 GiB − 4 KiB)` LOADER_DATA | `tmp/c2b-final-runall.log:28899`（归档日志逐字命中）、`test-kernel-map/src/main.rs:90-103` | 一致 |
| 真机门链五条脚本、判定标记（x86 5 条、U-Boot shim 6 条、aarch64 两条固定+第三变体、bootface 故意失败路径 `table holds 0x…0000 entries`+`module placement failed` 行序、hello-boot PASS） | 五个脚本逐个核对标记串与判据 | 一致 |
| 退出码 0/1/2 语义；U-Boot SKIP 打印原因与版本、不回打尾部 20 行 | 各脚本 `skip()/fail()` 与日志分支 | 一致 |
| CI 只接线 `test-riscv64-uboot.sh` | `.github/workflows/qemu-tests.yml:87` 唯一命中 | 一致 |
| aarch64 脚本 mdir 前置检查（kernel.elf + 12 模块名） | `test-shim-bootmarks-aarch64.sh:85-94` | 一致 |
| TODO-01~10 状态与关闭记录（D-029、BOOT-SCOPE 2a、宿主合成 layout、EBS 审计三负控） | `lib.rs`（D-029 相关）、`check-layout.sh`、`ebs_boundary_audit.rs:175-221`（三枚注入负控+注释豁免自检） | 一致 |

### 第 6 章

| 断言 | 源码证据 | 结果 |
|---|---|---|
| raw_serial x86_64 引文（`out dx, al` / 0x3f8、nomem/nostack、不等不补） | `boot-shim/src/lib.rs:272-282`（逐字一致） | 一致 |
| raw_serial_line：等腾空（LSR bit5 有界轮询）+ `\r` 补全；aarch64 两函数共用带节流发射器（源码注释明写） | `boot-shim/src/lib.rs:348-362,367+`、`284-295`（aarch64 注释逐字对应） | 一致 |
| 里程碑表全部条目（含 `kernel: v0`–`v4`、`step0 validate ok` 仅直核、`step1+2 mappings ok` 跳转前、`step2c device window empty (skip)` 仅 aarch64、`arch_boot arg/reslen/entered` 仅 x86_64、aarch64 additive 两行仅 aarch64 接收腿、x86/riscv 接收传空串） | `kernel/src/lib.rs:257,267,283,856-907,928-983,1375,1398`、`kernel-image/src/main.rs`、`kernel/src/lib.rs:386-392,755-783` | 一致 |
| `boot-shim: memmaps conv=… (empty map)` 无 reserved-big 行 | `uefi_helpers.rs:424` | 一致 |
| panic handler 引文（StackWriter 256 B、截断好于静默、raw_serial_line、diagnostic hook 兜底、spin_loop 驻留） | `boot-shim/src/main.rs:87-125`（逐字一致） | 一致 |
| 钩子静态量随 minix-types 每二进制一份；shim 侧恒未注册；直核/接收两侧注册点 | `minix-types/src/types/diagnostic.rs:26-50`、`kernel/src/lib.rs`（register_panic_diagnostic 两处调用点） | 一致 |
| v1 失败消息族 `kernel-image: handoff v1 … invalid:`（信封+七分区） | `kernel/src/lib.rs:647-725`（八条 panic 文案逐个对应） | 一致 |
| §6.3 表（模块缺失/ELF 读不到文案、bump 耗尽、`enter_kernel_image_v1: bump region not registered`、`bootstrap_len` 划线条目 grep 不到、4 GiB 之上 fail-closed、`conventional memmap [a,b) overlaps bootstrap allocation [c,d)`、大页阶梯两族消息前缀与两变体文案） | `loader.rs:437-443`、`uefi_helpers.rs:573`、`opensbi_helpers.rs`、`kernel/src/lib.rs`（NoKernelPageSize/FallbackUnsupported panic 文案） | 一致 |
| §6.4 命令层（五脚本名、GDB 断点名 `kernel::rust_image_main`/`minix_kernel::arch_boot*`、bin crate 名 `kernel`） | `os/qemu-tests/` 脚本清单、`kernel-image/Cargo.toml:2,65` | 一致 |
| §6.1 `raw_serial`/`raw_serial_line` 为同一通道两种写法（x86_64 例外说明） | `boot-shim/src/lib.rs:272-362` | 一致 |

---

## 方法说明

1. **基线**：先核 sha256 与行数，确认所审文本即 R33 基线。
2. **代码引文**：对文档中每段 `// 代码引用` 逐字比对源文件（含省略号声明是否与实际省略相符、编注是否标注为编注）。
3. **计数断言**：全部用文档自带的复算命令重放（`grep -c '#\[test\]'`、feature 属性计数、字段计数），四组 cargo test 主读数按文档命令实跑复核。
4. **结构断言**：依赖表、常量、容量、wire 布局、寄存器约定逐项与 manifest/源码对照。
5. **测试/脚本断言**：测试名逐个定位；门链脚本判定标记与退出码逐条读脚本。
6. **日志断言**：探针读数与 `tmp/c2b-final-runall.log` 的归档行比对（文档自称"无归档日志"的历史真机读数不作核查对象，文档已自带可靠性降级声明）。

## 结论

第 3、4、5、6 章射程内的 Rust 侧断言（引文、常量、计数、测试名、脚本行为、日志读数）与 `os/` 当前源码**全部一致**，本轮零缺陷。两处 SUGGESTION（S1 补 16 KiB 断言的文件锚点、S2 补 `AllocateType::Address` 的 Rust 拼写）均为可读性改进，不构成事实错误。
