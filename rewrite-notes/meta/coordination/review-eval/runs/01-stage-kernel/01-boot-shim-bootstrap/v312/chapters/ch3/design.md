DOC: /home/xzhao/github/minix-rs/tmp/review-loop/01-stage/chapters/01-boot-shim-bootstrap/ch3-final-46136cfb.md
CHAPTER: 3. Rust 设计决策
VERDICT: FAIL

校验：`sha256sum` = `46136cfb5cd29b7a1aa1417fc20ba982d2c4a584f291d62b12271a9a72dbc28d`，与要求一致；切片 589 行。
合同：`/home/xzhao/github/minix-rs/rewrite-notes/meta/design/01-stage-kernel/01-design.v4.md`；outline：`/home/xzhao/github/minix-rs/rewrite-notes/meta/design/01-stage-kernel/01-outline.v7.md`。
Ch1/Ch2 接口输入：`.../v312/summaries/ch1-summary.md`、`.../v312/summaries/ch2-summary.md`。

## 结论

主干一致：设计合同 P-1..P-8、I1–I4 在 Ch3 都有落点；`KernelInfo` 十字段与 wire v10（128 B 信封、六分区、`ARENA_CAP=32768`、余量 3584 B）、两张表与 11 字段 `SourceRecord`、bootstrap 区间删除、`PT_ALLOC` 初始化/注册换手、`FileLoader`/`prepare_boot`、五条生产腿与两族相反的校验次序，均与代码对得上。

判 FAIL 的原因是可定位的局部缺陷，不是章节结构问题：

- 两处把代码路径的承担者或上线内容说错：§3.6 线 511（U-Boot shim 把“表”序列化进 arena）、§3.7 线 552（`authorize_and_copy` 被写成调用 `resv_claims::capture`/`push_dtb_sources`）。
- 一处漏写设计合同已列的 `entry_point`：§3.2 的十字段台账与 wire 表都没有交代它为什么不在交接面上。
- 两处引用了切片中不存在的代码摘录：§3.4.1 线 287–298 的“逐条解码”没有可解码的链接脚本；§3.6 线 503–505 的“引文”没有引文。
- 两处非阻断修正：线 112 的 Ch1 术语锚点错误；线 418 的“五个裸 u64”与代码注释的三参旧签名说法不一致。

## 1. Ch3 的每条设计断言是否与设计合同/代码一致

### 1.1 `KernelInfo` wire（PASS）

- 切片 87–104、121、209–259。合同 P-1（按值交接、不做跨镜像 ABI）。
- 十字段与代码一致：`os/libs/minix-boot/src/kernel_info.rs:23-170` 的 `memmap`、`reserved_regions`、`source_records`、`kern_virt_base`、`kern_phys_base`、`kern_size`、`boot_modules`、`platform_sources`、`param_buf`、`authority`；无 `#[repr(C)]` 的说明也与文件头注释一致。
- wire v10 与数值全对：`BOOT_HANDOFF_V1_VERSION=10`（`os/libs/minix-boot/src/handoff_v1.rs:278`）、`ENVELOPE_CAP=256`（348）、`WIRE_END=7872`（378）、`WIRE_SOURCES_END=29184`（384）、`ARENA_CAP=32768`（392）；分区偏移/容量与切片 246–253 的表逐项一致。
- v10 删除 `startup_objects`/`user_sp`/`kern_stack_top`/`syscall_entry`、信封 160→128、startup 分区 1440 B 退场、余量 2144→3584 B：与 `handoff_v1.rs:278-306` 的版本说明、`ARENA_CAP` 注释及全仓 `StartupObjects`/`startup_record` 零命中一致。
- `RamAuthority`/`InputObjectId` 只有 DTB/UEFI 两值、wire 标签 2/6/5 为保留位：与 `os/libs/minix-boot/src/memmap_source.rs:555-600`、`handoff_v1.rs:100-125` 一致。
- 唯一缺口是 `entry_point` 未交代，见 §2 的 F3。

### 1.2 两张表与 `SourceRecord`（PASS）

- 切片 108–121、271–277。合同 P-2/P-3/P-4。
- `memmap` 为“可分配候选、未预扣”、`reserved_regions` 为“保留但可经 Direct Map 访问”，与 `kernel_info.rs` 两个字段的文档及 `validate_view` 的跨表不相交检查一致。
- `SourceRecord` 确为 11 字段：`os/libs/minix-boot/src/memmap_source.rs:492-517` 的 `id`、`kind`、`origin`、`range`、`input_object`、`local_source`、`snapshot_id`、`attribute`、`virtual_start`、`address_domain`、`raw_type`，与切片 275 的四个分组逐项对应。
- 覆盖审计的两个入口与 strict 分支：`audit_source_coverage`/`audit_source_coverage_for`（`memmap_source.rs:317-336`）；`KernelInfo::validate_requiring_sources` 在 strict 分支调用带 authority 过滤的后者，再查模块名与内核跨度（`kernel_info.rs:297-299、508-590`），与切片 277 一致。
- 切片 277 对 C `memmap` 只有基址/长度/固定 `AVAILABLE` 标签的判断，与 C `multiboot_memory_map_t` 和 `pg_utils.c:109` 的写法一致。

### 1.3 bootstrap 区间、分配器换手、页目录槽位（PASS）

- 切片 283–376。合同 P-6；outline 第 3 项把页目录槽位明确分给 Ch3。
- C 侧锚点全部对得上：`bootstrap_start/len` 来自 `_kern_unpaged_start/end`（`minix3/minix/kernel/arch/i386/pre_init.c:114-116`），归还发生在 `system_init()` 之后、`bsp_finish_booting()` 之前（`minix3/minix/kernel/main.c:295-324`）；`kernel.lds:15-34` 的 `__k_unpaged__kern_size`/`_kern_size` 与 `AT()` 平移、`objcopy --prefix-symbols=__k_unpaged_`（`arch/i386/Makefile.inc:64-67`）均与切片 294–297 一致。
- Rust 侧无该字段、`PT_ALLOC` 换手表与代码一致：`init_boot_pt_alloc`（`os/kernel/src/boot_alloc.rs:183`）、`BootAlloc::alloc` 的取页后清零（138–160，含切片引用的 `write_bytes(pa as *mut u64, 0, 512)`）、`boot_validate_and_prepare` 的 `is_registered()` 守卫（`os/kernel/src/lib.rs:933-934`）、VM 的 `vm_pt_alloc`/`vm_pt_free` 与 `register`/`register_free` 守卫（`os/servers/vm/src/vm_server.rs:379-388`）、`register` 的写一次语义（`os/arch/src/arch/pt_alloc.rs:55-72`）。
- `VmBootAllocator` 的“从区间末端向下”与 `BootAlloc` 相反：`os/kernel/src/vm_handoff.rs:465-480`；整段扣除的“consumed pages and remainder are both kernel-reserved”也在同文件 23 行附近。
- §3.4.3 谈 `inherit_supervisor_half`/PML4 槽位，停在这套机制为何退役的设计取舍，没有写建表步骤；按 outline 属 Ch3，且与合同“01 不写页表建立细节”不冲突。

### 1.4 取件抽象与 crate 方向（PASS；一处非阻断措辞问题）

- 切片 381–464。合同 P-7/P-8。
- `prepare_boot<'a>(&'a mut BootStorage, bump_pages) -> BootPrepareResult<'a>` 与 `BootPrepareResult` 四字段：`os/boot-shim/src/opensbi_helpers.rs:241`、`os/libs/minix-boot/src/boot_prepare.rs:26`。
- `FileLoader` 的两个实现、`read_required` 的带路径 panic、`elf_file_end` 的二次修剪、`MockLoader`、`build_shim_table` 的 2 MiB/1 MiB/16 MiB 槽与 `truncating` 后继续：`os/boot-shim/src/loader.rs:146-160、307、625`，`os/boot-shim/src/uboot_main.rs:373-420`，`os/boot-shim/src/uefi_helpers.rs:1350-1360`。
- `bootface_common` 的 `NAME_SLOT=16`、12 槽名字池、`place_modules` 的位置/长度/区间/池/内核跨度检查、`PlacementPool` 五字段、两侧 bump 区间 `0x8200_0000..0x8400_0000` 与 `0x4400_0000..0x4600_0000`：`os/kernel-image/src/bootface_common.rs:29-81、176-310、341`，`os/kernel-image/src/bootface_a64.rs:69-121`，`os/boot-shim/src/opensbi_helpers.rs:70-71`。
- 四份依赖表与 `kernel-image` 的 target 依赖：四份 `Cargo.toml` 与切片 447–450 一致；`os/kernel/Cargo.toml` 确无 `boot-shim`/`uefi`，`os/kernel-image/Cargo.toml` 只在 riscv64/aarch64 target 下加 `boot-shim`+`fdt`。
- 非阻断：切片 418 说“五个相邻的裸 `u64`”，而 `bootface_common.rs:170-173` 的注释写“used to be three adjacent bare `u64` parameters”。当前 `PlacementPool` 确实是五个同型字段；若句子在讲旧签名，数字与代码注释冲突。建议改成“`PlacementPool` 的五个同型字段互换后仍能编译”，或与代码注释对齐。

### 1.5 五条生产腿、所有权与门链（FAIL）

- 五条腿的成员、计数换算、主线/辅助定位：切片 465–547 与 §3.1.1 表一致；门脚本 `os/qemu-tests/test-riscv64-uboot-shim.sh` 的 6/6 路标、`run_all.sh:300-304` 的注册、`test-kernel-image-riscv64-bootface.sh` 只验证表址/模块定位失败、`test-smp-aps-aarch64.sh` 用 `fw-aarch64-none` + `-kernel`、六个 `fdtput` 脚本，均与切片 56–59、507 一致。
- 门链次序：切片 550–558 与代码一致——直核面 `os/kernel-image/src/bootface_common.rs:554-556` 先 `validate_requiring_sources(DTB)`，`os/kernel/src/lib.rs:215-240` 再 `validate_before_tables`/`check_boot_postconditions`/可信链断言/`store_boot_report`/`arch_boot_impl`；v1 接收面 `os/kernel/src/lib.rs:673-760` 先 `validate_requiring_sources`、`check_boot_postconditions`、`resume_boot_pt_alloc`，`resume_additive` 内 `boot_validate_and_prepare`，之后 `store_kernel_info`；`kmain` 在 `os/kernel/src/lib.rs:1859-1861` 复核 strict 与 postcondition；`check_boot_postconditions` 只剩 `kern_virt_base > u32::MAX`（`os/libs/minix-boot/src/kernel_info.rs:619-635`）。
- FAIL-1（线 511）：切片说 U-Boot shim“把表与模块序列化进 arena”。代码里 `BootFileTable` 是 shim 在 `.bss` 自建（`os/boot-shim/src/uboot_main.rs:373`），经 `UbootFileLoader` 用来取件；arena 只承载 `boot_modules` 分区里的 `BootModule` 描述符（name/start/len）与 memmap/reserved/platform/param/sources，没有 `BootFileTable` 分区（`os/libs/minix-boot/src/handoff_v1.rs:347-392`）；模块字节留在 bump 物理页，不进 wire。最小修复：改为“它用自建表取件，把模块描述表（`BootModule`：name/start/len）与 memmap 等载荷序列化进 arena；`BootFileTable` 与模块字节留在物理内存”。
- FAIL-2（线 552）：切片说“`authorize_and_copy` 随后用 `resv_claims::capture` 生成 DTB `/reserved-memory`/MRB 记录并经 `push_dtb_sources` 追加、二次 `finish_sources()`”。代码里 `dtb_copy::authorize_and_copy(descs)` 只做授权与拷贝并返回 `OwnedDtb`（`os/boot-shim/src/dtb_copy.rs:176`）；`resv_claims::capture`、`push_dtb_sources`、二次 `finish_sources()` 都在 EBS 配对循环里，由 `ebs_pair` 在授权返回后调用（`os/boot-shim/src/ebs_pair.rs:192-201、243-250`）。最小修复：把主语改为“配对循环在 `authorize_and_copy` 返回 DTB 副本后调用 `resv_claims::capture`，再经 `push_dtb_sources` 追加并二次 `finish_sources()`”。
- 其余所有权断言成立：UEFI 腿 storage 走四个 `fill_*`、五类载荷、source 只写 wire；U-Boot shim 与两个直核腿走 `fill_source_records`；`fill_*` 闭包返回时提交计数、半提交不可表示。锚点：`os/libs/minix-boot/src/boot_storage.rs:86-229`、`os/boot-shim/src/opensbi_helpers.rs:317-330`、`os/kernel-image/src/bootface_common.rs:355、541`。

## 2. 设计合同已裁决但 Ch3 漏写/写反的决策；Ch3 自造且合同没有的决策

### 2.1 F3（FAIL）：`entry_point` 漏写

- 合同 `01-design.v4.md:11` 明确把 `entry_point` 列入“内核位置记录”：`kern_virt_base`/`kern_phys_base`/`kern_size`/`entry_point`。
- Ch3 的十字段台账（切片 87–98）与 wire 信封/分区表（219–253）都没有 `entry_point`，全章也没有一句说明它为什么不在交接面上。切片 132 只说“三个布局数字由 shim 从 ELF 算出”，把合同里的四项缩成三项，未交代第四项。
- 代码事实：`compute_kernel_layout` 从 ELF 头取 `e_entry` 并放进 `KernelLoadResult.entry_point`（`os/boot-shim/src/loader.rs:251、293`）；UEFI 生产入口断言 `layout.entry_point == layout.kern_virt_base.0`（`os/boot-shim/src/uefi_helpers.rs:1043-1046`），跳转目标也用发送侧的布局值。因此它是发送侧可导出、接收侧可由 `kern_virt_base` 重建的值，不单独上线是合理的——但合同列了它，Ch3 必须收口。
- 最小修复：在 §3.2.1“升为字段”行之后（或 §3.2.3 wire 表前）加一句：“`entry_point` 不进 `KernelInfo`/wire：boot-shim 从 ELF `e_entry` 取它并断言等于 `kern_virt_base`，跳转只用发送侧布局值；接收侧从 `kern_virt_base` 进入，不需要单独字段。” 这样合同四项都有去向。

### 2.2 P-1..P-8 的覆盖（PASS）

| 合同决策 | Ch3 落点 | 结论 |
|---|---|---|
| P-1 `KernelInfo` 按值、不做跨镜像 ABI | §3.2、§3.2.3 | PASS |
| P-2 `RamAuthority` 区分输入对象与 RAM 权威 | §3.2.1 线 102–106 | PASS |
| P-3 两张内存表分离 | §3.3 线 273 | PASS |
| P-4 `SourceRecord` 带 provenance | §3.3 线 275–277 | PASS |
| P-5 `BootStorage` 单所有者 + `fill_*` | §3.7 线 552–558 | PASS（UEFI source 只写 wire 的例外已在同段写明，不构成第二所有者） |
| P-6 分配器换手用 init/resume | §3.4.2、§3.6 线 499 | PASS |
| P-7 直核面共享 `run_self_boot` | §3.5.1 线 410、§3.6 线 460 | PASS |
| P-8 五条腿按发送语义收敛 | §3.6 线 483、533–547 | PASS |

### 2.3 合同 I4 判据的箭头序（合同侧待修，不是 Ch3 写反）

- 合同 `01-design.v4.md:22` 写 `validate_before_tables → store_kernel_info → validate_requiring_sources`。
- 代码里这只对直核面近似成立：v1 接收面是 `validate_requiring_sources → check_boot_postconditions → resume_boot_pt_alloc → boot_validate_and_prepare → store_kernel_info`（`os/kernel/src/lib.rs:673-760`），直核面是发送侧 `validate_requiring_sources` → `validate_before_tables` → … → `store_boot_report` → `boot_validate_and_prepare` → `kmain` 的 strict `validate`（`os/kernel-image/src/bootface_common.rs:554-556`、`os/kernel/src/lib.rs:215-240、1859-1861`）。
- Ch3 §3.7 线 550–556 明确写出“两组腿的包装调用次序相反”，与代码一致。结论：Ch3 没有写反；合同 I4 的箭头序需在下一版按腿细化（只读复核，不改合同）。

### 2.4 Ch3 自造的决策（未发现无依据的发明）

- §3.1.1 的两个 aarch64 产物（`fw-aarch64-none` / `fw-aarch64-uefi-recv`）：`os/kernel-image/Cargo.toml:28、36`、`os/xtask/src/image.rs:86、107-108、1303-1304`、`os/qemu-tests/test-smp-aps-aarch64.sh:75-83` 都有实现锚点。
- wire v10 的删除项、`a2` 通道退役、GET_KINFO 收窄、`BootReport` 九字段视图、`ReservedMapped` 五类落点：分别有 `handoff_v1.rs:278-306`、`bootface.rs:126-143`、`os/libs/minix-types/src/types/kinfo.rs:15`、`boot_storage.rs:292-314`、`ebs_map.rs:600-655` 支撑。合同没有逐条列出这些细节，但它们是代码已实现的裁决，不是凭空新增。

## 3. Ch3 是否越界写逐行代码走读、重讲 Ch1/Ch2

### 3.1 F4（FAIL）：§3.4.1 的“逐条解码”没有可解码的摘录

- 切片 287 说“这段区域的性质写在链接脚本里：”，289 只有标准注记“代码路径与关键行见第四章的对应小节；本节只给设计契约与取舍。”，291 直接“逐条解码：”，随后 293–298 逐条解释“前三个赋值”“`. = _kern_phys_base;`”“`. += _kern_offset;`”“`AT()`”等，但切片里没有任何链接脚本行可读。
- 这既是一段没有对象的逐行走读，也让读者无法核对“前三个赋值”指什么。Ch2 §2.5.1 已把段划分/边界符号的细节明确交给 §3.4（全文章节 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:361`），所以这里不是重讲 Ch2，而是 Ch3 自己该给的落点没有给全。
- 最小修复（二选一）：其一，插入 8–12 行链接脚本短摘录（属允许的短摘录）；其二，更符合本章“只给设计契约与取舍”的写法——删掉“逐条解码”框架，把 293–298 压缩成设计结论（unpaged 段执行地址等于物理地址、只服务分页前、Rust 侧没有可单独归还的内核段），行级细节指向第四章/02 篇。

### 3.2 F5（FAIL）：§3.6 的“引文”没有引文

- 切片 501 的 bullet 以冒号结尾，503 只有标准注记，505 却写“引文里的 `establish the direct map` 就是 02 篇的 4.3 节 的 Step 4”“引文省略了该参数”。切片中没有这段引文。
- 锚点：该句来自 `os/kernel/src/lib.rs:295` 的 `arch_boot_resume_v1` 文档注释（“land the KernelInfo into this image's `.bss`, establish the direct map, then enter `kmain`”）。`new_from_page(root_page, channel)` 的真实签名在 `os/arch/src/arch/paging.rs:270`。
- 最小修复：删掉“引文”指代，直接写“additive resume 的 Step 4 就是 02 篇 §4.3 的 `establish the direct map`；`new_from_page(root_page, channel: PteChannel)` 的真实签名是……”，或补一段允许的短摘录。

### 3.3 其余边界（PASS）

- 没有逐行实现走读：§3.2.1 只给字段/协议表，§3.4.2 只给换手表，§3.5.1 只给签名与检查项；短代码块只有 `BootPrepareResult`，属允许的类型/契约摘录。
- 没有重讲 Ch1：Ch3 只引用三类数据、I1–I4 与接缝，不重复 Ch1 的概念推导。
- 没有重讲 Ch2：§3.4.1 是 Ch2 明确移交的段划分落点；§3.2.2.4 的十条是“Rust 与 C 的可观察分歧”，不是 Ch2 源码分析的复述。
- §3.4.3 的页表槽位内容停在“为什么 C 的借槽机制在 64 位退役”，没有写建表/映射步骤；按 outline 属 Ch3 的设计取舍。

## 4. 与 Ch1/Ch2 摘要的接口是否一致

### 4.1 三类数据与 I1–I4（PASS）

- Ch1 摘要的三类数据（可用内存图、内核镜像位置、交接结构）在 Ch3 均有对应：§3.2/§3.3 两张表与 `KernelInfo`/wire，§3.2.1 的 `kern_virt_base`/`kern_phys_base`/`kern_size`，§3.7 的所有权与门链。
- I1–I4 的章节标题与判据都在：I1 §3.2、I2 §3.3、I3/I4 §3.7；I2 的 `audit_source_coverage`、I3 的 `BootStorage`/`fill_*`、I4 的逐腿校验次序都能落到代码。
- 唯一接口缺口是 Ch1 摘要的三类数据只写“内核落在哪里、有多大”，没有 `entry_point`；设计合同 v4 明确列了它。F3 的修复句同时对齐两边。

### 4.2 Ch2 的 C 行为外部结果（PASS）

- 模块数 12 的硬约束：切片 161、416 与 `NR_BOOT_MODULES`/`NAME_POOL` 一致；`place_modules` 的“条目数 ≥ 12 断言、有效条目不足返回 None”与 `bootface_common.rs:212-226` 一致。
- 内核与模块占用区必须从可用清单切除、相切不算重叠、页对齐/夹取：Ch3 的 `classify`/扣除表/`validate_view` 没有反着写；`validate_view` 的 `w_end <= w[1].base` 允许相切（`kernel_info.rs` 的区间检查段）。
- D1–D5 的次序没有被 Ch3 改写；Ch3 只引用 §2.4.2/§2.5 的锚点，不另立次序。
- ARM32 对照：切片 13、45 引用 §2.7 的“四步/仅第 1 步随引导器变”，该说法在 Ch2 全文 1011–1022 行有依据。
- 校验次序的“两族相反”与 Ch2 的 C 行为不冲突：C 没有 Rust 的两族接收面，Ch3 是在 Rust 设计层面新增的约束。

### 4.3 非阻断修正：线 112 的 Ch1 术语锚点

- 切片 112 写“LMA/VMA 的定义见 §1.0 术语与 §3.4”。Ch1 的术语表在 §1.6 本章回顾（全文 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md:129` 的 `#### 术语`，位于 §1.6；Ch1 摘要也把 §1.0 标为“问题入口”）。最小修复：`§1.0 术语` → `§1.6 术语`。

## 5. 证据口径

PASS：本复核只给结论级证据，按切片行号 + 合同/代码锚点定位；没有要求或重做逐行代码走读。所有 FAIL 都给出了可局部执行的修复，不需要整章重写。

## 最小修复清单

1. 切片 511（§3.6）：把“它把表与模块序列化进 arena”改为“它用自建 `BootFileTable` 取件，把模块描述表（`BootModule`：name/start/len）与 memmap 等载荷序列化进 arena；`BootFileTable` 与模块字节留在物理内存”。
2. 切片 552（§3.7）：把“`authorize_and_copy` 随后用 `resv_claims::capture`……”改为“配对循环在 `authorize_and_copy` 返回 DTB 副本后调用 `resv_claims::capture`，再经 `push_dtb_sources` 追加并二次 `finish_sources()`”。
3. §3.2.1（切片 96 后）或 §3.2.3（切片 215 前）：补一句 `entry_point` 的去向——发送侧从 ELF `e_entry` 取、断言等于 `kern_virt_base`，接收侧由 `kern_virt_base` 进入，不单独进 `KernelInfo`/wire。
4. 切片 287–298（§3.4.1）：删除“逐条解码”框架并压缩成设计结论，或补 8–12 行链接脚本短摘录；推荐前者，保持本章“只给设计契约与取舍”。
5. 切片 503–505（§3.6）：删掉“引文”指代，直接陈述 additive resume 的 Step 4 与 `new_from_page(root_page, channel)` 的真实签名；或补一段允许的短摘录。
6. 切片 112（§3.2.1）：`§1.0 术语` → `§1.6 术语`。
7. 切片 418（§3.5.1，非阻断）：把“五个相邻的裸 `u64`”改为“`PlacementPool` 的五个同型字段”，或与 `bootface_common.rs:170-173` 的“three adjacent bare u64”旧签名说法对齐。

## 非阻断观察

- §3.2.1 线 93 的 16→128/64 容量变更（C 的 16 条静默截断不再继承）是外部可观察行为变化，但该段没有按三档术语标 `[ARCH]`；若项目把它判为 Architectural Evolution，应补标并说明这是哪一档。
- 合同 I4 判据的箭头序按腿细化，见 §2.3；合同 P-5 的“BootStorage 单所有者”与 UEFI source 只写 wire 的例外已在 Ch3 §3.7 写明，不构成矛盾，但合同下一版可把该例外写成显式条款。
- 切片 85 的“现役字段速查在 §3.2.1”是 §3.2.1 内部自指，属编辑瑕疵；不影响设计一致性。
