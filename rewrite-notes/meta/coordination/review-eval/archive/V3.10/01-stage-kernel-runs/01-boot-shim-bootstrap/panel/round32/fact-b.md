# FACT-B 审查报告 — 01-boot-shim-bootstrap.md（R32）

- 审查角色：P-FACT-B（只读，事实核对 B）
- 日期：2026-11-11
- 基线：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`，3565 行，
  sha256 = `7fd75e78c9e1b43588c59fb193ad3845742bc37fc991e0d6b308e2dc09065329`（与 R32 基线前缀一致，未漂移）
- 范围：第 3、4 章全部，以及 §5–§6 中引用 Rust 代码 / 测试 / 日志的断言（C 源引用与纯 C 侧叙述按分工归 FACT-A，本报告不重复）
- 方法：逐节提取「当前代码状态」类断言（结构体字段、常量值、函数签名、cfg 计数、测试名与计数、
  串口路标字符串、脚本判据、诊断消息），逐条对照 `os/` 源码。历史读数（真机记录、已删除注释、
  已回滚方案）标注为不可复跑项，只核对文档自身声称的「现行不存在/已删除」是否属实。

---

## FINDING RD32-FACT-B-01

**位置**：§3.4.5「直核自举面同构（DESIGN-09）」bullet，段归属句（文档 1770 行附近）：

> **段归属与所有权边界要说准**：`BootStorage::new()` 不是全零初值（平台源的 `kind = DTB = 1`、
> 模块名的非空悬挂指针都是非零值），所以这个静态落在 **`.data`**（可装载数据），不是零填充的
> `.bss`；**容量断言 `size_of::<BootStorage>() <= 16 KiB` 约束的是这块 `.data` 的大小，不是 `.bss`**。

**源码证据**：
- 全仓（`os/`，排除 target）检索 `size_of::<BootStorage` → **0 命中**。
- `os/libs/minix-boot/src/boot_storage.rs`：结构体与 `const fn new()`（40–113 行）周围无任何
  `assert!`/`const _: ()` 涉及 `BootStorage` 的总大小；`16 KiB`/`16384`/`16 * 1024` 在该文件
  0 命中。全文件仅有的尺寸相关断言是 `const _: () = assert!(PARAM_BUF_CAP == WIRE_PARAM_CAP)`（39 行）。
- 与本句相邻的 `.data` 结论本身有源码支撑：`os/libs/minix-boot/src/boot_storage.rs:96–98`
  `BootStorage::new` 的文档注释写明 "because `new()` contains non-zero defaults, such a static
  lands in `.data`, not `.bss`"（`ZERO_SOURCE = PlatformDescSource::new(DTB, …)`，`DTB = 1`
  见 `os/libs/minix-boot/src/platform.rs:97`；`ZERO_MODULE.name = ""` 的悬挂指针按 Rust
  `NonNull` 语义非零）。
- 仓库内现存唯一的 `size_of::<…>` 引导面用法是 `size_of::<BootFileTable>()`
  （`os/kernel-image/src/bootface.rs:237`、`bootface_a64.rs:226`），类型不同。

**为什么错**：该句以「容量断言 X 存在」的口吻引用一个编译期不变量，并把它作为 `.data`/`.bss`
归属论证的一部分。这个断言在当前树里不存在——既不在 `minix-boot`，也不在 boot-shim /
kernel-image 侧。`.data` 归属的结论不受影响（源码注释直接支撑），但被引用的约束是虚构的：
读者按 §5 的口径复算（L1 工具证据）会得到零命中，且「这块 `.data` 被约束在 16 KiB 内」的
安全含义（镜像装载面预算）实际没有对应闸门。

**严重度**：P1（引用不存在的断言＝事实性缺陷；因相邻结论本身成立、不改变任何机制性行为结论，
不升 P0。若按本仓 P0-fact 的严格口径，也可主张 P0——由主审裁决）。

**反证路径**：
1. `grep -rn "size_of::<BootStorage" os/`（排除 target）→ 若命中任何 `assert!(… <= 16 * 1024 …)`
   即本条被推翻；
2. `grep -rn "16 KiB\|16384\|16 \* 1024" os/libs/minix-boot/src/boot_storage.rs` → 0 命中；
3. 若维护者能指出该断言所在的历史 commit 或另一 crate（例如某设计快照把它登记为 TODO 而未落地），
   则改为「文档把未落地的规划写成了现行事实」，降级为 P2 并在 §5.3 登记需求。

---

## SUGGESTION RD32-FACT-B-01（P3）

**位置**：§5.2.2 断言清单 L7 行（文档 3260 行附近）：

> | L7 | 引导栈符号存在、跨度 64KiB、落在末段内（**不检查自顶向下方向**）——这是宿主侧的弱检查；… |

**源码证据**：
- `os/kernel-image/check-layout.sh:24`（脚本头部注释）：「L7 引导栈符号存在、**自顶向下 64 KiB**、
  整体落在末段」；
- `os/kernel-image/check-layout.sh:226–239`：实际判定是
  `span_want=$((st - sb))` 且要求 `span_want == 0x10000` 且 `sb >= last_lo && st <= last_hi`
  ——`st - sb == 64 KiB` 为真就要求 `kernel_boot_stack_top` 的地址高于
  `kernel_boot_stack_bottom`，即**符号间的高低方向已被该算术约束**（若两符号颠倒，span 为负，
  断言失败）。

**为什么不妥**：「不检查自顶向下方向」与脚本自身的头部注释（「自顶向下 64 KiB」）直接冲突；
按字面读，方向是查了的（top 必须在 bottom 之上 64 KiB）。该行后半句「sp 初值与数组顶的真实
关系……由 ld 脚本的排布保证」想说的弱检查（宿主脚本无法验证运行期 sp 是否取数组顶）是成立的，
但括注措辞会让读者以为符号方向也未校验。

**建议**：改为「不检查运行期 sp 取值/栈生长方向；符号间 top>bottom 与 64 KiB 跨度已查」，
或删去括注、把弱检查的准确边界写进后半句。

**反证路径**：`sed -n '226,239p' os/kernel-image/check-layout.sh` ——若实现改为不约束符号
方向（例如取绝对值），本条降为无效。

---

## SUGGESTION FACT-B-02（P3）

**位置**：§3.2 bootface_common 段（文档 1519 行附近）：

> `PlacementPool` 存在的理由不是打包参数：三个相邻的裸 `u64`（名字池地址、bump 起点、bump 终点）
> 互换后仍能编译并产生"看似合理"的放置，**结构体的命名字段把这类调用点错误挡在编译期**。

**源码证据**：
- `os/kernel-image/src/bootface_common.rs:160–164`：`PlacementPool { names, bump_start, bump_end }`，
  三个字段全部承载 `u64` 值；
- 两个调用点 `os/kernel-image/src/bootface.rs:299–303`（`names: names_pool, bump_start:
  bump_pool_base(), bump_end: bump_end_bound()`）与 `bootface_a64.rs:275–278`
  （`bump_start: BUMP_BASE, bump_end: BUMP_BASE + BUMP_SIZE`）。

**为什么不妥**：命名字段挡住的是「位置实参错位」（不再有函数形参顺序可错）与漏写，但对
**同类型值级互换**没有编译期防护——`bump_start: bump_end_bound(), bump_end: bump_pool_base()`
这样的值级写反照样编译通过并"看似合理"。三个成员同为 `u64`，命名字段不构成类型级屏障；
真正成立的说法是"调用点必须逐字段具名，错位从编译期错误降为可审查的具名差异"。

**建议**：把「挡在编译期」弱化为「调用点逐字段具名、错误可被 review 直接看见」；若要真正的
编译期防护，可把 bump 起止改成 `PhysRange`（`PhysRange::new` 拒绝反向区间，已存在该类型），
届时 `bump_end < bump_start` 才真的编译不过/构造即 panic。

**反证路径**：在任一调用点把 `bump_start`/`bump_end` 的实参互换后 `cargo build`——能通过编译
即证实本条（仅 riscv64/aarch64 target 可编；宿主可对 `bootface_common` 做等价小实验）。

---

## 复核通过项（抽样记录，非穷举——全部为 L1/L2 级证据）

以下断言逐条核对通过（文件:行为本次实查锚点；同一批断言只列首查锚点）：

**§3 章**
- §3.1 `exit_boot_services` 引文逐字（`uefi_helpers.rs:1229–1240`，cfg `qemu-test-kernels`）；
  「测试内核路径专用、best-effort、不与 MapKey 配对」的注释口径一致。
- §3.2 `BootPrepareResult` 四字段（`boot_prepare.rs:26–34`）；`prepare_boot(&mut BootStorage,
  bump_pages) -> BootPrepareResult<'a>`（`opensbi_helpers.rs:244–247`）；`uboot_main` 直接调用
  （`uboot_main.rs:444`）。
- §3.2 `FileLoader`/`read_required` 引文逐字（`loader.rs:31–43`）；`UefiFileLoader::read`
  引文（`uefi_helpers.rs:1247–1254`）；`entry_path_eq`/`UbootFileLoader::read` 引文
  （`opensbi_helpers.rs:175–181、198–221`）；`valid_entries` 夹取
  （`opensbi_helpers.rs:164–167`）；`build_shim_table` 在 `.bss` + "does not fit the
  BootFileEntry field" 断言（`uboot_main.rs:328、350、376`）。
- §3.2 bootface_common 五步检查、`NAME_SLOT=16`、`NAME_POOL` 12×16、编译期逐字节对账、
  riscv64 池 `0x8200_0000..0x8400_0000`（`opensbi_helpers.rs:64、69`）、aarch64
  `BUMP_BASE=0x4400_0000`+`BUMP_SIZE=0x0200_0000`（`bootface_a64.rs:72、92、95`）。
- §3.2 cfg 计数：`lib.rs` 恰 7 处 `#[cfg(feature…)]`（6 uefi + 1 opensbi，`lib.rs:221–249`）；
  `uefi_helpers.rs` 恰 8 处 `qemu-test-kernels` gate（157–1229）；`impl BootShim` 当前树 0 命中。
- §3.3 四份 manifest 依赖表逐项核对（boot-shim / kernel / minix-boot / kernel-image 及两个
  target 块）；`boot-shim → minix-arch` 调用面仅 `boot_dm_admissible_end`；
  `boot-shim → minix-kernel` 仅 `init_boot_pt_alloc` + `enter_kernel_image_v1`（grep 实测）。
- §3.4.1 `KernelInfo` 14 字段逐字段（`kernel_info.rs`）；128/64/47/40/20/296/1024/16 各容量
  （`globals.rs:295–400`、`minix-types/src/types/boot.rs:107、122`、`handoff_v1.rs:366–418`、
  `memmap_source.rs:46–51`→DTB_SOURCE_CAP=40）。
- §3.4.1 authority 绑定：wire 只收 0/1（`handoff_v1_read.rs:162–166`）；两直核面传
  `DtbBootInput`（`bootface_a64.rs:409`、`bootface.rs` 同型）、U-Boot shim 在 `Serializer::new`
  声明（`uboot_main.rs:459`）、UEFI 传 `UefiMemoryMap`（`uefi_helpers.rs:1060`）；
  `KernelInfo.authority = StartupObjects` 无生产者（仅记录身份字段使用，`memmap_source.rs:773`）。
- §3.4.2 `KinfoStruct` 48 B + 7 字段 + `_padding`（`kinfo.rs:18–54`）；kmess 10000 字节环
  （`kmess.rs:34`）；`FREE_MEMMAP` 仅 `bootface_a64.rs:199` 一行注释；`"board"` 键读者 0 命中；
  `normalize_param_buf` ARCH 编译目标生成 / BOARD 有源才生成 / 内部键拒绝
  （`boot_params.rs:40–137`）及 4 个具名测试全存在。
- §3.4.3 `BootHandoffV1` 22 字段引文逐字；version=9；ENVELOPE_CAP=256；WIRE_END=7872；
  source 至 29184（296×72）；startup 至 30624（20×72）；`assert_eq!(ARENA_CAP -
  WIRE_STARTUP_END, 2144)`（`handoff_v1.rs:288–495`）；v9 注释仍写 "6144 B" 而常量已删
  （`handoff_v1.rs:282–285`）——与文档自指一致。
- §3.4.5 UEFI 腿 storage source/startup 槽留空（`uefi_helpers.rs` 无
  `set_memmap_sources_len`/`set_startup_len` 调用，source 直写 wire 于 `ebs_pair.rs:194、237–243`，
  startup 直写于 `uefi_helpers.rs:1147–1181`）；`map_dirty`/`commit_map_tables`/`report()` panic
  （`boot_storage.rs:84–189、356–365`）；五个 `set_*_len` 一次提交断言
  （`boot_storage.rs:200–306`）；`BootStorageCell::take` one-shot（`boot-shim/src/lib.rs:164–172`）；
  闲置容量 ≈ 9 KiB（128×16+256×16+40×72=9024 B，算术成立）。
- §3.6 `HugePages` trait 引文逐字（`paging_ext.rs`）；x86_64 三常量与 `map_huge` 守卫
  （`x86_64/paging.rs:1154–1156` + PDPT_SHIFT/PS/AlreadyMapped 段）；`Paging` 恰 22 成员
  （1 const + 21 方法，`arch/src/arch/paging.rs:186–559` 实数）；`plan_kernel_mapping` 尺寸阶梯
  引文逐字（`boot_policy.rs:98–138`）；`BootWindows::check`/`CheckedWindows` token 流向。
- §3.7 `arch_boot_finish` 引文逐字（`kernel/src/lib.rs:214–228`）；x86_64 `arch_boot` 仅诊断 +
  `arch_boot_finish`（`lib.rs:237–291`）；`enter_kernel_image_v1`/`prepare_kernel_image_v1`/
  `jump_kernel_image_v1` 签名与 aarch64 asm `in("x0")`（`lib.rs:427–582`）；riscv64 `fence.i; jr`
  与 `in("a0")`；x86 global-asm stub `mov rbx,rsi; mov rax,rdi; cli; cld; jmp rax`
  （`lib.rs:595–606`）；`resume_additive` 文档注释引文（`lib.rs:307` 起）；
  `new_from_page` 清零 512（`x86_64/paging.rs:612–623`）；六个入口委托壳实数
  （3×`arch_boot` + `arch_boot_resume_high_half` + x86/riscv `resume_after_v1_decode`，
  `lib.rs:379、747–789`）。
- §3.8 `pt_alloc.rs` `register`/`alloc_pt_page` 引文逐字（`PT_CLAIMED`/`PT_REGISTERED`/
  AcqRel/Release/Acquire 语义一致，`pt_alloc.rs:62–115`）；`BootAlloc::alloc` 引文逐字
  （`boot_alloc.rs`）；VM 侧 `vm_pt_alloc`/`vm_pt_free` 注册 + `is_registered()` 守卫
  （`vm_server.rs:382–391`）；`for_each_live_bootstrap` 调用段与扣除表头注释
  （`vm_handoff.rs:425–435、675`；`push` 整段扣除注释逐字）。
- §3.9 三架构 `inherit_supervisor_half` 锚点（`arm64/paging.rs:595`、`x86_64/paging.rs:650`
  ——PML4 256..512、`riscv64/paging.rs:694`——Sv39 L2 256..512）；`getinfo_kinfo` 固定填 0
  （`misc.rs:721–727`）；`KinfoStruct.vir_kern_start` 无仓内读者（wire 填充除外）。

**§4 章**
- §4.1 `main.rs` 入口引文（`boot-shim/src/main.rs:34–45`）；`BOOT_PT_BUMP_PAGES=1024`
  （`boot-shim/src/lib.rs:258`）；两直核面 `PT_BUMP_PAGES=256`（`bootface_a64.rs:100`、
  `bootface.rs` 同型）；`vmctl_set_addr_space` 不再注入 kerninfo 页
  （`syscall.rs:3153`、3153+39 注释）。
- §4.2 `prepare_and_enter_kernel_image` 顺序引文与源码逐段对齐（`uefi_helpers.rs:953–1202`，
  省略段声明与实况一致：root/bump 断言在 storage take 之前、模块复查在 12-modules 行之后、
  arena 断言在 arena 分配之后）；九动作时序表与源码顺序一致。
- §4.2.1 动作 4 `alloc_arena` 经 `MaxAddress(bootstrap_alloc_limit())`+`LOADER_DATA`
  （`uefi_helpers.rs:645–654`）；`bootstrap_alloc_limit`/`alloc_root_page` 引文逐字
  （640 行段）；6b startup 补写段（1147–1181，canonical 五类、容量前置断言）。
- §4.2.2 `adjudicate` 五步 + 两道内联闸逐行（`ebs_map.rs:551–585、594–661`）；
  `LAST_KNOWN=UNACCEPTED(15)`、类型 0–15、`KNOWN` 16 常量覆盖 31 个位位（手工复算：bits 0–4、
  12–19、62、63、ISA_MASK bits 44–59 = 5+8+1+1+16=31）；`AMBIGUOUS`/`PROFILE_ONLY`/
  `TYPE_INDEPENDENT` 位集逐项；`checked_shape`/`checked_len` 失败集逐项；五例手算全部复现；
  `MaxAddress` 语义段与 `alloc_root_page` 一致。
- §4.3 路径常量、`MODULE_NAMES` 12 项及顺序（`kernel_info.rs:685–687`）；
  `load_boot_modules_into` 引文逐字（`loader.rs`）；`num_pages` ceil 公式、页尾清零、
  `len` 记文件字节数、缺件 panic 消息逐字；`UbootFileLoader` 二次修剪幂等（`opensbi_helpers.rs:
  206–219`）；`load_kernel_with_loader_gated` 的 e_entry 断言（UEFI 侧在
  `prepare_and_enter_kernel_image` 内另有一处 `assert_eq!(entry_point, kern_virt_base.0)`，
  两处计法成立）。
- §4.4 `build_kernel_info` 引文逐字（`boot-shim/src/lib.rs`）；`kern_stack_top`=
  `kern_virt_base+kern_size`、`syscall_entry`=`kern_virt_base`；`boot_validate_and_prepare`
  断言链逐条（`kernel/src/lib.rs:839–914`）；`ValidationLevel` 三档
  （`kernel_info.rs:192–200`）；直核面 strict 门先于 `arch_boot`
  （`bootface.rs:440`、`bootface_a64.rs:411`）；接收面
  validate_requiring_sources → postconditions → resume_boot_pt_alloc 次序
  （`lib.rs:729–741`）；`USER_STACK_TOP` 单一权威（`minix-types/src/types/boot.rs:189`）。
- §4.5 `land_slice`/`store_kernel_info` 引文逐字（`kernel/src/lib.rs`）；七落地区 + 名字池
  容量表逐行（256/128/12/4/1024/296/20/16，及 `VM_BOOT_HANDOFF_MAX_REGIONS=64`、
  `MAX_DEDUCTED=47`）；两处交接容量 fail-closed 断言实锚
  （`vm_handoff.rs:752`、`vm_handoff.rs:205–210`）；名字池唯一允许截断、15 字节 + UTF-8 前缀
  回退；三个生产调用点 + 返回值消费（`lib.rs:225、352、1738`）；`kernel_info()` 读者
  （GET_KINFO/GET_IMAGE=misc.rs、phys_ram_view=syscall.rs:2593–2598、
  harden_kernel_text=lib.rs:1605、kmain=lib.rs:1688）。
- §4.6 Step 1/2 引文与源码逐行（`kernel/src/lib.rs:1020` `DEV_PIECE=1<<21` 为文件级常量，
  与引文头声明一致）；窗口断言 `end <= IDENTITY_MAP_END`（`lib.rs:1250`，该名即
  `BOOT_IDENTITY_MAP_END` 的重导出别名，`lib.rs:79`）；`enable()` 四件事引文逐字
  （`x86_64/paging.rs`，含 NXE→CR3→CR0.PG|WP→PGE 顺序与 UEFI 已开分页的注记）；
  `identity_device_windows` 三架构落点（aarch64=平台 MMIO 基窗、riscv64=PLIC+UART 整表、
  x86=空）与 `kernel_identity_mmio_regions` x86 两固定页（`arch/src/lib.rs:258–322`）
  及「无生产读者」（唯一引用在 `vm_handoff.rs:1187` 的测试内）；`device_window_regions`
  x86 两窗 bootstrap:true、aarch64 空、riscv64 三窗含 ACLINT SSWI bootstrap:false
  （`arch/src/arch/device_window.rs:141–215`）；x86 `flags_to_pte` DEVICE→PWT|PCD、
  `pte_to_flags` 只能还原 WRITE_THROUGH|NO_CACHE（`x86_64/paging.rs:141–162`）；
  `MMIO_VA_BASE`/`use_high_mmio_window`（Step 2b）；AP_STARTUP_PA=0x5000
  （`ap_early_entry.rs:53`）；`x86_64-unknown-none`/`.cargo/config.toml`
  `-C target-feature=-sse,-sse2,-mmx`（`os/.cargo/config.toml:45`）。
- §4.7 `init`/`resume` 引文逐字（`boot_alloc.rs:54–90`）；U-Boot 池顺序与三道运行期断言
  （根页=池首页、PT 子区间紧随、不越池尾，`opensbi_helpers.rs:349–364`）；编译期四组断言
  （4KiB 对齐、`KERNEL_MAX_LMA_END<=MODULE_REGION_BASE`、根+PT+ARENA_CAP≤池、
  `BUMP_END<=boot_dm_admissible_end`，`opensbi_helpers.rs:91–95、575–606`）；
  `init_boot_pt_alloc` U-Boot 腿写 `result.bump.start().0/.end().0`（`uboot_main.rs:450`）；
  两 bump 实例共享 `BumpRegion` 纯值核心。
- §4.8 `rust_image_main` 三臂引文逐字（`kernel-image/src/main.rs:341–357、377–390、
  398–401、407–419`，含 x86 `arena_pa==0` 诚实失败、aarch64 `==0` 分道、riscv64 两特性臂）；
  x86 `_start` trampoline（cli/cld/清 .bss 只碰 rdi/rcx/rbx 不动/rsp 对 16/`mov rdi,rbx`/
  `call rust_image_main`，`main.rs:137–171`）；`riscv64.ld` 三常量
  （`KERNEL_VIRT_BASE=0xFFFFFFC000000000`、`KERNEL_PHYS_BASE=0x80200000`、`ENTRY(_start)`，
  `riscv64.ld:39–42`）；x86_64.ld `KERNEL_PHYS_BASE=0x200000`（27 行，§3.5 引用一致）；
  表址通道 `opensbi,boot-file-table` 仅 DTB `/chosen` 一条；六个脚本用 fdtput 注入
  （脚本名逐一在 `os/qemu-tests/` 存在）。
- §4.9 `install_at`（`ap_early_entry.rs:244`）与「地址由调用者给」注释；`map_kernel` 两参
  （`vmproc_handle.rs:386` `map_kernel(&mut pt, &layout)`）。
- §4.2.2/§4.3/§6.3 诊断消息逐字：`conventional memmap [a,b) overlaps bootstrap allocation`
  （`uefi_helpers.rs:573`）；`boot-shim: memmaps conv=… reserved=… unmapped=… (empty map)`
  （`uefi_helpers.rs:424、507`）；`kernel-image: handoff v1 envelope invalid`
  （`kernel/src/lib.rs:652`）；`bootstrap_len must be 0` 全仓 0 命中（§6.3 划线行属实）；
  两条 page-size policy 前缀（`lib.rs:459、947`）；`enter_kernel_image_v1: bump region not
  registered`（`lib.rs` 内 expect）。

**§5 章**
- 测试计数逐文件复算 21 个文件全部一致（22/11/12/45/17/15/13/6/1 与
  32/8/8/74/2/10/21/29/2/5/5 与 23；boot-shim 136、minix-boot 196、minix-elf 23 的分组和
  与文档算式一致）；`grep -c '#\[test\]'` 与文档口径（含 `#[should_panic]`）对齐。
- 具名测试全部实锚：`test_load_boot_modules_*` 5 组、`test_bss_region_is_zeroed_after_load`、
  `test_compute_kernel_layout_from_mock_elf`、opensbi 4 条、`test_build_kernel_info_fields`、
  `kernel_info.rs` 16+3+8+4+1=32 拆分、`test_store_kernel_info_*` 8 条（1+7 负控，溢出阈值
  257/129/13/5/1025/297/21 与各容量 +1 一致）、`pt_alloc_write_once.rs` 2 +
  `pt_alloc_concurrent.rs` 1、`boot_alloc.rs` 10、`vm_handoff.rs` 21（含 free_set_* 11 条
  行为锁/对拍）、`dm_coverage.rs` 10、`boot_policy::policy_tests::
  picks_the_preferred_size_when_supported_and_aligned`、`missing_boot_allocator_registration_
  fails_closed`、`store_before_alloc_pin.rs`、`test_device_flags_set_pwt_and_pcd`、
  `decoded_map_survives_arena_poison_and_reissues_the_arena_frames`、
  `boot_identity_windows_are_2mib_and_mark_device_pieces` 等 6 条窗口测试。
- `check-layout.sh` L0–L8 清单与脚本头注释逐条对应；L9 仅 riscv64 分支、对账对象
  `bootface.rs KERN_SIZE`（脚本 248–262）；退出码 0/1/2（脚本 37–39）。
- 深拷探针：`test-kernel-map` 内 N-5 probe 实锚（`qemu-tests/test-kernels/kernel/bootstrap/
  test-kernel-map/src/main.rs:9–14、35–41、90–96`：快照后 `MaxAddress(4GiB-4KiB)`+LOADER_DATA、
  三时点校验和、print-only 不改 PASS 判据——与 §5.2.3 的三层判别叙述一致）。
- 五条门链脚本全部实锚且判据标记一致：`test-shim-bootmarks-x86.sh`（5 标记）、
  `test-shim-bootmarks-aarch64.sh`（2 固定 + MARK3 变体 + DTB preflight 拒绝行 +
  panicked 提前收尾）、`test-riscv64-uboot.sh`（TEST_RESULT PASS；SKIP 原因 + U-Boot 版本、
  尾部 20 行仅在非 SKIP 路径）、`test-riscv64-uboot-shim.sh`（6/6 标记，205–230）、
  `test-kernel-image-riscv64-bootface.sh`（`table holds 0x0000000000000000 entries` + 行序 +
  `module placement failed`，108–145）。
- CI 接线：`.github/workflows/qemu-tests.yml:87` 仅 `test-riscv64-uboot.sh`；
  `cmd-smoke.yml` matrix 为 test-cmd-smoke 三架构（非五门链成员）——与「其余四条是手工门」
  表述一致。
- TODO-01～10 详情行与源码锚点一致（`lock_boot_globals`、`arch_boot_test`、D-029 冻结决策
  的说明位置、TODO-08 的窗口断言、TODO-10 的 `test_device_flags_set_pwt_and_pcd`）。
- C 侧被 Rust 节引用的两个常量：`MAXMEMMAP=40`（`minix3/minix/include/minix/param.h:13`）、
  `NR_MEMS=16`（`minix3/minix/include/minix/const.h:98`）→ `MIN(40,16)=16` 与 §3.4.1 的
  「C 只读前 16 条」口径一致。

**§6 章**
- §6.1 里程碑表逐行实锚（`uefi_helpers.rs:954、1029、1037、1142、1196–1202`；
  `uboot_main.rs:492`；`opensbi_helpers.rs:341`；`lib.rs:257/267/283`（x86_64 测试入口）、
  `839–914`（v0–v4）、`935/977/983/1375/1398`（step0/2c/step4/step1+2）、`391–392`
  （aarch64 additive 两行））；x86_64 带 `, x86_64`、aarch64 无后缀、riscv64 带 `, riscv64`
  的三变体（`kernel-image/src/main.rs:354/387/3063`）；
  `  param_buf bytes=` 两空格前缀（`bootface.rs:335`、`bootface_a64.rs:309`）。
- §6.1 `raw_serial` x86_64 引文逐字（`boot-shim/src/lib.rs:272–282`）；raw_serial/line 差异
  （x86_64 line 等腾空+补 `\r`，aarch64/riscv64 共用带节流发射器，`lib.rs:284–304、348–361`）。
- §6.2 panic handler 引文逐字（`boot-shim/src/main.rs`，StackWriter 256 B、
  run_panic_diagnostic_hook 兜底行）；钩子随 minix-types 每二进制一份
  （`minix-types/src/types/diagnostic.rs:26–50`）；`register_panic_diagnostic`
  （`kernel/src/lib.rs:3403`）。
- §6.4 脚本清单与「关键行缺失时」三脚本行为（aarch64 打印出现/未出现、U-Boot SKIP 版本、
  bootface 尾 20 行）；GDB 断点名与「`kernel` 是 kernel-image bin 名」
  （`kernel-image/Cargo.toml` `[[bin]] name = "kernel"`）。

**不可复跑项核对**（文档自述为历史/观察，仅核对「现行不存在」声明）：
- `res=127 zeroed=127`、15 条 conventional 零化两组真机读数：文档自标无归档日志，不核。
- `the previous PhysBytes(0)+len=kern_phys_base.0…` 旧注释：文档自述「字段删除时随参数删除」，
  现行树 grep 不到该串（属实）；替代回归说明 "structurally impossible" 在
  `opensbi_helpers.rs:875` 实锚。
- 「合并 bump 游标曾回滚（3/6 路标 panic）」：历史事件，现行 `BootAlloc::resume` 的
  `is_consistent` 守卫与「两个实例不共享游标」现状相符。
- `5ed051a53` 上 `impl BootShim` 单实现：历史 commit 论据，当前树 0 命中属实。

---

## 结论

- 当前文档在本次范围内共 **1 条 FINDING（P1×1）**、**2 条 SUGGESTION（P3×2）**。
- 全部引用型断言（代码引文、常量值、字段清单、cfg/测试计数、脚本判据、诊断字符串、
  容量与断言锚点）经逐条对照源码，除上述三条外均与 `os/` 现状一致；未发现引用位置漂移
  或「引文与源码不符」级别的 P0 缺陷。
- 唯一 P1（RD32-FACT-B-01）是引用了不存在的 `size_of::<BootStorage>() <= 16 KiB` 容量断言；
  相邻的 `.data`/`.bss` 结论本身有源码支撑，修复方式为删除该括注或补上真实断言并登记 TODO。
