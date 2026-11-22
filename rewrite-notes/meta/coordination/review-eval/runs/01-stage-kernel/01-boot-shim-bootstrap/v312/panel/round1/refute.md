DOC 01-boot-shim-bootstrap.md

# D01 反证报告（v312-r1）

对象：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（2983 行）。
方法：只从文档主张出发生成反例，反例必须落到源码位置、C 真值或文档原句；反证失败也照实写。
本轮只读关联代码清单与 C 源码；未读作者结论、历史评审或其他面板角色输出。

## CLAIM 01

- 主张：A+D 动作序成立——读完内核 ELF 后先用 `AllocateAddress` 把 `[kern_phys_base, +kern_size)` 整段预订成 `LOADER_DATA`，再分配 root/bump，再拷贝段、载模块、分配 arena；因此"预订之后"固件发出的任何页都不可能落在内核镜像跨度内；两条 UEFI 腿的内核因此不在自由表（`memmap`）内，而在 `reserved_regions`（`ReservedMapped`），或在该腿 fail-closed 的前提下落 `Side`；预订被固件拒绝即 halt，不降级（§3.2、§4.2.1 动作 1/2、§4.2.1 表后 bullet）。

- 反证尝试：
  1. 找"预订之后仍可能返回跨度内页"的分配点。`alloc_root_page`/`alloc_bump_region`/`alloc_arena` 全部走 `AllocateType::MaxAddress(bootstrap_alloc_limit())`（`os/boot-shim/src/uefi_helpers.rs:657-683`、`:700-709`）；模块页走 `alloc_module_pages`，它在拷贝前显式检查 `[base,end)` 与记录的 `KERNEL_SPAN_LO/HI` 不相交，命中即 panic（`uefi_helpers.rs:1372-1397`）。找不到绕过口。
  2. 找"预订被拒绝但继续走"的降级路径。`reserve_kernel_span` 对 `allocate_pages` 失败直接 panic，消息写明 "halting (the span would stay allocatable otherwise)"（`uefi_helpers.rs:78-87`）；对齐不满足也先 assert（`:72-76`）。无反例。
  3. 找"内核仍在自由表里"的腿分支。`adjudicate` 的 Step 5 只有在属性含 `WRITE_BACK` 时才把 `LOADER_DATA` 归 `ReservedMapped`，否则落 `Side`（`os/boot-shim/src/ebs_map.rs:634-644`）；落 `Side` 时严格门的内核跨度覆盖检查失败并 panic（`os/libs/minix-boot/src/kernel_info.rs:504-518`），即文档 §6.3 "固件不设 `WRITE_BACK`" 一行描述的行为。找不到"既不在两表、又继续启动"的路径。
  4. 找"预订之前"的分配是否遗漏：`read_required(KERNEL_PATH)` 的 `Vec` 与 aarch64 的 `load_dumpdtb_blob()` 都在 `reserve_kernel_span` 之前（`uefi_helpers.rs:1028-1032` vs `:1057`）。若落在跨度内，`AllocateAddress` 拒绝 → 上面第 2 条的 halt，不会静默重叠；文档已把 ELF `Vec` 写成例外（`:1048-1056`），但只点了 ELF 一笔。

- 结果：反证失败（主张成立）。附 P2 观察一条（不推翻主张）。

- 证据：
  - 顺序：`os/boot-shim/src/uefi_helpers.rs:1027-1069`（read → compute/validate → `reserve_kernel_span` → `record_kernel_span` → `alloc_root_page`/`alloc_bump_region` → `load_segments_into_phys_memory`）。
  - 预订语义：`uefi_helpers.rs:52-92`（`AllocateType::Address(base)`、`LOADER_DATA`、失败 halt）。
  - 事后复查：`uefi_helpers.rs:1073-1099`（root/bump/modules 与内核跨度不复用）、`:1117-1126`（arena 与内核跨度不复用）。
  - 分类落点：`os/boot-shim/src/ebs_map.rs:587-645`（Step 5 的 `WRITE_BACK`/类型表）；严格门内核跨度覆盖：`os/libs/minix-boot/src/kernel_info.rs:504-518`。
  - 交叉验证：D-062 的前提行（`DECISIONS.md:138`）以同样的 `reserve_kernel_span` 机制论证五条腿内核跨度都落在两张表内。

- 若我错了：若固件在 `AllocateAddress` 成功后仍把该区间发给 `allocate_pages`（即不遵守自己的分配账），或存在我在 48 个文件里没找到的第三条 UEFI 分配路径在 `reserve_kernel_span` 之前分配并写入内核跨度，则主张 1 不成立。P2 观察：§4.2.1 表后 bullet 的"更早一笔 UEFI 池分配"只列 ELF `Vec`，漏了 aarch64 的 `dumpdtb` `Vec`（同属预订前分配、同一 fail-closed 后果）；不影响"预订之后"这一主张的成立。

## CLAIM 02

- 主张：三条 v1 腿跨镜像只传值——wire 只写固定宽度标量与字节分区，`(ptr, len)` 不上线；接收侧把六个分区解码进自己的 `.bss` 落地区并只用这份副本；信封版本号收发两侧精确比较（v9 与 v10 互相拒收）；越界/不能整除/容量不足都在构造引用前失败（§1.2 I1、§3.2、§3.2.3、§4.1、§4.6）。

- 反证尝试：
  1. 在 `BootHandoffV1` 与六个 wire 记录里找指针字段或宿主宽度类型。`BootHandoffV1` 的标量全是 `u32/u64`（`os/libs/minix-boot/src/handoff_v1.rs:82-100` 是 `WireSourceRecord`；`WireBootModule`/`WireMemoryRegion`/`WirePlatformDescSource` 也都是定点宽度）。`kern_size` 用 `u64` 而不是 `usize`（`kernel_info.rs` 注释所述与 `handoff_v1.rs:278` 版本号一致）。没有可跨镜像的指针。
  2. 找"接收侧保留 arena 指针"的路径。`arch_boot_resume_v1` 的解码目标全是 `.bss` store（`os/kernel/src/lib.rs:670-733`），构造 `KernelInfo` 时六个切片全部 `from_raw_parts(*_store.as_ptr(), len)`（`:735-748`），`arena_pa` 不入结构；随后 `resume_additive` 的 `store_kernel_info` 因源/目标同址走 no-op（`:1562-1678` 的 `land_slice` 指针相等守卫）。
  3. 找版本宽松比较。`check_header` 用 `!=` 精确比较 `BOOT_HANDOFF_V1_VERSION`（`os/libs/minix-boot/src/handoff_v1_read.rs:107-109`），并校验 `size`/`arena_len`（`:110-115`）；`BOOT_HANDOFF_V1_VERSION = 10`（`handoff_v1.rs:278`）。v9 的 arena 布局会先被版本号拒掉，不存在"按旧布局乐观读"。
  4. 找分区越界/不整除的静默截断。`element_count` 检查 `byte_len % element_size == 0` 且 `count <= cap`（`handoff_v1_read.rs:197-207`），失败返回错误、由 `arch_boot_resume_v1` 的 `unwrap_or_else(panic!)` 终止。
  5. 找"接收侧消费发送侧池页"的残留：`store_kernel_info` 的注释与实现都把六个切片重指向 `.bss`（`os/kernel/src/lib.rs:1573-1676`）。

- 结果：反证失败（主张成立）。

- 证据：
  - 信封与容量：`os/libs/minix-boot/src/handoff_v1.rs:278`（版本 10）、`:356-392`（各分区容量与 `ARENA_CAP`）、`:435-437`、`:460-464`（`WIRE_SOURCES_END == 29184`、余量 3584 的编译期/测试断言）。
  - 头部校验：`handoff_v1_read.rs:100-117`。
  - 解码落点：`os/kernel/src/lib.rs:665-753`（六分区 → store → `KernelInfo`）。
  - 测试锚点：`vm_handoff.rs:1080-1090`（`decoded_map_survives_arena_poison_and_reissues_the_arena_frames`，把 arena 整块 poison 后仍能按解码值分类）。

- 若我错了：若某个 wire 记录里存在未在 docs/source 里声明的指针宽度字段（例如 `usize` 或函数指针），或某条 v1 腿的接收侧在解码后仍解引用 aria 内地址（而不是 store），则主张 2 不成立。当前源码里两者都找不到；唯一的"指针"是物理地址数值（模块 `start`、平台源 `phys_addr`），它们是值不是可达引用。

## CLAIM 03

- 主张：门链按腿成立，且"校验先于消费"（I4）在两条腿形状上是相反次序：直核面 `validate_requiring_sources`（strict，bootface 侧）→ `validate_before_tables` → `check_boot_postconditions` → `store_kernel_info` → `boot_validate_and_prepare` → `kmain::validate`；v1 接收面 `validate_requiring_sources` → `check_boot_postconditions` → `resume_boot_pt_alloc` → `boot_validate_and_prepare`（`resume_additive` 内）→ `store_kernel_info` → `kmain::validate`，且**不经过** `validate_before_tables`（§3.7、§4.4、§4.4.1）。

- 反证尝试：
  1. 找直核面次序相反或漏门的路径：`bootface_common::run_self_boot` 在调 `arch_boot` 前调用 `kernel_info.validate_requiring_sources()`（`os/kernel-image/src/bootface_common.rs:536-561`），`arch_boot_finish` 依次 `validate_before_tables` → `check_boot_postconditions` → `store_kernel_info` → `arch_boot_impl`（`os/kernel/src/lib.rs:213-233`），`arch_boot_impl` 开头调 `boot_validate_and_prepare`（`:954-966`）。没有反序。
  2. 找 v1 接收面偷偷走 pre-table 的路径：`validate_before_tables` 全仓只有一个调用点（`lib.rs:223`，`arch_boot_finish`）；`arch_boot_resume_v1` 与 `resume_additive` 体内都没有它（`:665-753`、`:299-349`）。主张可核。
  3. 找"接收侧先 store 后 validate"的反例：`resume_additive` 第 307 行先 `boot_validate_and_prepare`，第 324 行才 `store_kernel_info`。与文档"两条腿的 store 与 boot_validate 次序相反"一致。
  4. 找"strict 门缺省导致接收侧没查 pre-table 不变量"的漏洞：`validate_impl_inner` 对 `BeforeTables`/`Standard`/`Strict` 都查 `kern_size>0`、`memmap` 非空、`param_buf<=1024`（`kernel_info.rs:250-270`、`:420-450`），`Strict` 另加模块名按位核对与来源覆盖（`:450-495`）。因此"v1 接收面不经 pre-table"不是覆盖缺口。

- 结果：反证失败（主张成立）。附 P2 观察一条。

- 证据：
  - 直核链：`os/kernel-image/src/bootface_common.rs:557-561`；`os/kernel/src/lib.rs:213-233`、`:954-966`。
  - v1 链：`os/kernel/src/lib.rs:665-753`（`validate_requiring_sources` → `check_boot_postconditions` → `resume_boot_pt_alloc` → `resume_after_v1_decode`）、`:299-349`（`boot_validate_and_prepare` → `store_kernel_info`）。
  - 门内容：`os/libs/minix-boot/src/kernel_info.rs:220-247`（`validate_before_tables`/strict 入口）、`:250-495`（共享校验体）、`:553-568`（后置条件只剩 4 GiB 下限）。
  - `kmain` 复核：`os/kernel/src/lib.rs:1807-1847`（`validate()` 与再一次 `check_boot_postconditions()`）。

- 若我错了：若某条生产腿（不含 QEMU 测试内核）在消费前跳过了 strict 或 postconditions，或 v1 接收面实际调用了 `validate_before_tables`，则主张 3 不成立。P2 观察：§4.4.1 的逐腿链以"→ `kmain` 的 `validate`"收尾，但 `kmain` 在 `validate()` 之前还再调一次 `check_boot_postconditions()`（`lib.rs:1846`）；这是幂等的同门复核，门表只列一次不算错，但"逐腿执行序"作为清单并不完整。另：文档把 strict 的调用者写成"两个 bootface 在调 `arch_boot` 之前显式调用"，实际唯一调用点是两者共用的 `bootface_common::run_self_boot`；效果相同，措辞略松。

## CLAIM 04

- 主张：所有权与半提交面成立——`BootStorage` 是发送侧唯一所有者；每个载荷族只在 `fill_*` 闭包返回时提交（`fill_map_tables` 一次提交两表），"槽位已写、计数未提交"不可表示；第二次 `fill_*` panic；落地区容量溢出 panic，唯一允许截断的是模块名池；第二次 `store_kernel_info` 也 panic（§1.2 I3、§3.7、§4.5）。

- 反证尝试：
  1. 找可半提交的路径。五个 `fill_*` 都是 `assert!(!*_committed)` → 调闭包 → 校验计数 ≤ 容量 → 写 `*_len` → 置 committed（`os/libs/minix-boot/src/boot_storage.rs:141-223`）；闭包自己失败会 panic，不存在"计数已提交而表未写"的窗口。UEFI 腿的 source 分区不经 `fill_source_records`（分类器直写 wire + `push_dtb_sources` 再 `finish_sources`，`os/boot-shim/src/ebs_pair.rs:171-203`、`:243-250`），文档已明确把这族排除在"第二次 fill panic"之外。
  2. 找第二次 `fill_*` 不 panic 的构建档。assert 无 `cfg` 条件，测试 `fill_modules_twice_is_refused`/`fill_map_tables_twice_is_refused` 锁住；`BootStorageCell::take` 的二次调用同样 panic（`os/boot-shim/src/lib.rs:155-167`）。
  3. 找落地区的静默截断。`land_slice` 与两处内联容量检查全用 `assert!`（`os/kernel/src/lib.rs:1536-1551`（`land_slice`）、`:1604-1607`（`boot_modules`）、`:1650-1653`（`param_buf`））；模块名池用 `min(BOOT_MODULE_NAME_LEN - 1)`（`lib.rs:1611`），v1 接收路径按 16 字节原样落（`lib.rs:695-710`）——与文档"唯一允许截断"的口径一致，且两条路径截断点不同也写清了。
  4. 找第二次 `store_kernel_info` 的路径。`lib.rs:1567-1570` 的 one-shot 断言无 `cfg`；`os/qemu-tests/test-kernels/.../{test-rt-birth,test-rt-birth-riscv64,test-sysboot}/src/main.rs` 都是"test 自己 store 一次 + `arch_boot_impl`（不 store）"，没有二连 store。
  5. 找 `report_static` 自身的 one-shot 守卫（文档原句："`'static` 只由 `report_static` 的 one-shot 静态存储支撑"）。实际该函数没有 flag（`os/boot-shim/src/lib.rs:177-187`），one-shot 由 `BootStorageCell::take` 与"fill 完成后再取报告"的 boot 契约保证。这句话若读成"函数自守卫"则偏强；读成"底层静态存储只被一次 boot 填充"则成立。

- 结果：反证失败（主张成立）。附 P2 措辞观察一条。

- 证据：
  - 五族提交：`os/libs/minix-boot/src/boot_storage.rs:141-223`；`report()` 不再需要 dirty-flag 的理由在其注释（`:256-277`）。
  - 六个落地区与名字池：`os/kernel/src/globals.rs:297-417`；`store_kernel_info` 落地区实现 `os/kernel/src/lib.rs:1562-1678`。
  - 容量唯一权威表与溢出行为：文档 §4.5 表 vs `globals.rs` 常量（256/128/12/4/1024/`WIRE_SOURCES_CAP=296`），逐项一致。
  - 测试：`os/libs/minix-boot/src/boot_storage.rs:396-435`。

- 若我错了：若某个生产腿能在同一载荷族上第二次进入 `fill_*` 而不触发 assert（例如通过 `UnsafeCell` 绕过 `&mut`），或某条路径的落地区超容量被静默丢弃而不是 panic，则主张 4 不成立。P2：`report_static` 的 one-shot 是契约而非代码守卫，文档用"one-shot 静态存储"描述时最好指向 `BootStorageCell::take` 这个真正的守卫，否则读者会去找一个不存在的断言。

## CLAIM 05

- 主张：换手的三条事实成立——跨镜像接收侧用 `resume(base, next, end)` 从 `bump_next` 续用（三个值来自 wire，先校验 `base < end && base <= next <= end`），不会把 shim 已用作页表页的页重发；VM 交接按 `boot_alloc_region()` **整段**扣除 bump 区（不是按已用部分），UEFI 腿该行因 `LOADER_DATA` 退化为空操作；arena 不永久扣除，因为解码是 arena 的最后一次读、重发晚于解码（§3.4.2、§4.6）。

- 反证尝试：
  1. 找 resume 不校验或校验更弱的路径。`BootAlloc::resume` 与 `region()` 都断言 `BumpRegion::is_consistent`（`os/kernel/src/boot_alloc.rs:78-86`、`:94-106`），`is_consistent` 的定义正是 `start < end && start <= next <= end`（`os/libs/minix-boot/src/bump.rs:75-83`）；wire 侧 `decode_envelope` 还先做一遍同样的三元组检查（`os/libs/minix-boot/src/handoff_v1_read.rs:169-174`）。无双保险缺失。
  2. 找"按已用部分扣除"的反例。`for_each_live_bootstrap` 对 `boot_bump` 一律 `f(base, end - base)`（`os/kernel/src/vm_handoff.rs:756-758`），`classify` 传入 `boot_alloc_region()`（`:492-500`）；`DeductionCollector::push` 对每条做页对齐上取整（`:259-274`）。与文档"扣的是整段"一致。
  3. 找 arena 被永久扣除或被解码后读取的路径。扣除集合成员只有 kernel/root/bump/VM 启动器已用后缀/模块 blob（`vm_handoff.rs:741-767`），不含 arena；poison 测试把 arena 整块写成 `0xA5` 后仍按解码值分类并断言 arena 页被重新发出（`:1080-1090`）。`patch_runtime_fields_in` 只写四个 `u64`（`os/libs/minix-boot/src/handoff_v1_write.rs:86-95`），不是读。
  4. 找"最早可能重发 arena 的点早于解码"的路径。最早的解引用点 `arch_boot_resume_v1` 的 `decode_*` 在 `kmain` 之前；`vm_boot_exclusions`/`VmBootRegion::select_multi`/`VmBootAllocator` 在 `kmain` 的 `init_proc_and_boot`（`os/kernel/src/lib.rs:2933-2948`、`:2992-3007`），晚于解码。找不到早于解码的重发者。
  5. 找 VM 侧是否真的复核 `free ∩ deducted = ∅`。`os/servers/vm/src/boot.rs:433-480` 的 `reconcile` 逐条断言，文档引用的就是这个检查。

- 结果：反证失败（主张成立）。附 P2 观察一条。

- 证据：
  - 续用与校验：`os/kernel/src/boot_alloc.rs:54-106`；`os/libs/minix-boot/src/bump.rs:81-105`；`os/libs/minix-boot/src/handoff_v1_read.rs:153-175`。
  - 整段扣除：`os/kernel/src/vm_handoff.rs:487-513`、`:741-767`、`:259-274`；容量断言 `:234-238`、`os/libs/minix-types/src/types/boot.rs:69`、`:84`。
  - arena 合同：`os/kernel/src/lib.rs:640-656`（合同文档注释）、`:665-753`（解码）；`os/kernel/src/vm_handoff.rs:1080-1090`（poison 测试）。
  - VM 消费侧：`os/servers/vm/src/boot.rs:332-341`、`:433-480`。

- 若我错了：若存在一条腿把 bump 区按"已用部分"传给 VM，或某处在解码之前（例如 `boot_validate_and_prepare`/`establish_boot_dm` 期间）把 arena 页当空闲页发出，则主张 5 不成立。P2 观察：文档把生命周期边界写成"VM 自由页清单在 `kmain → build_vm_handoff` 才发布"，这不是最早的重发点——`vm_boot_exclusions`（`lib.rs:2717-2749`）不排除 arena，`VmBootAllocator` 在 `init_proc_and_boot` 里就可能拿到 arena 页，早于 `build_vm_handoff`。结论（arena 可回收）仍成立，因为该分配器同样晚于解码；但"publish 之前无重发"这句作理由偏窄，真正的截止线是"跳转之后不再读 arena"（文档的可执行规则本身写的是 "after the jump"，所以规则没错，只是论证中的时序说法不紧）。

## CLAIM 06

- 主张：C 侧 `alloc_lowest` 把长度传给了 `cut_memmap` 的结束地址参数，是一次真实的参数语义错位；在当前调用序与四个前提下"切除一次都不发生"（第三行常态），跳板页仍挂在空闲账上；这是核对过的调用序 + 观测布局，不是结构保证（§2.4.2.2、§2.4.2.3）。

- 反证尝试：
  1. 核对签名与实参。`cut_memmap(kinfo_t *cbi, phys_bytes start, phys_bytes end)`（`minix3/minix/kernel/arch/i386/pg_utils.c:32`），而 `alloc_lowest` 里是 `cut_memmap(cbi, lowest, len);`（同文件 `:80`）。主张成立，排除"其实第三个参数是长度"的误读（`add_memmap` 的第三个参数才是长度，`:86`）。
  2. 复核四个前提。① 嵌套：`kmain`（`main.c:115`）先 `cstart`（`:147`），`cstart` 里 `arch_init()`（`:474`），`arch_init` 末尾两条 `cut_memmap`（`arch_system.c:246`、`:277-278`）；`smp_init` 在 `kmain` 更后（`main.c:311`）。② 最低基址 ≥ 0x1000：两条切除取整后为 `[0, 0x1000)` 与 `[0x90000, 0x100000)`（`memory.h:118-119`、`:28-33`），`add_memmap` 的 `roundup` 保证剩余条目基址页对齐（`pg_utils.c:99`）。③ `roundup(tramp_size)` ≤ 最低基址：`tramp_size = &__trampoline_end - tramp_start`（`arch_smp.c:75`），跳板段含 `GDT_SIZE*DESC_SIZE`（`GDT_SIZE = TSS_INDEX(CONFIG_MAX_CPUS) = 6 + CONFIG_MAX_CPUS`，`archconst.h:19-21`；`CONFIG_MAX_CPUS=1`，`kernel.h:8`）与 `IDT_SIZE*DESC_SIZE`（`trampoline.S:47-57`），合计约 2212 B < 4096，`roundup` 为 0x1000。④ 唯一调用点：`alloc_lowest` 在 `minix3/minix/kernel/arch/i386/` 内只有定义、原型与 `copy_trampoline` 一处调用。四前提都成立。
  3. 找"第三行常态"变成第一/第二行的布局。第一行 `lowest == 0` 被前提 ② 排除；第二行 `0 < lowest < len` 需要 `lowest < 0x1000` 或 `roundup(tramp_size) > 0x1000` 且 `lowest < len`——前者被 `arch_init` 切除排除，后者需要 `CONFIG_MAX_CPUS` 增大约两个数量级（GDT 每 CPU +8 B，要到 4096 B 总预算耗尽需 ~237 个 CPU），而 `kernel.h` 硬编码为 1。本仓布局无第二行。
  4. 找"切除会发生"的路径。`cut_memmap(lowest, len)` 的退化情形是 `start == end`（0x1000 == 0x1000）且页对齐——文档 §2.4.2.3 明确：退化且页对齐时"取不取整都什么都没切"。因此"第三行常态"在这份调用序下确实意味着跳板页仍在账上。
  5. 检查文档是否把"常态"写成"结构保证"。§2.4.2.3 的原句是"四前提是核对过的调用序，不是结构保证…没有运行历史证据能证明它从未暴露；这不是它写对了"，且给出任何一条前提变化就重推的要求。没有夸大。

- 结果：反证失败（主张成立）。

- 证据：
  - 参数错位与三行分叉：`minix3/minix/kernel/arch/i386/pg_utils.c:65-83`；`cut_memmap` 取整与夹取 `:32-63`；`add_memmap` 的 `roundup/rounddown` 与 `len==0` `:86-121`（`:98-100`）。
  - 跳板大小：`minix3/minix/kernel/arch/i386/arch_smp.c:68-81`；`minix3/minix/kernel/arch/i386/trampoline.S:47-58`；`archconst.h:19-21`；`kernel.h:8`。
  - 两刀位置：`minix3/minix/kernel/arch/i386/arch_system.c:277-278`；`memory.h:28-33`、`:118-119`。
  - 调用嵌套：`minix3/minix/kernel/main.c:115`、`:147`、`:311`、`:474`。
  - 下游症状推演（跳板页被发出去）：`arch_smp.c:75-81` 的返回值只受 `< 1<<20` 断言约束，不检查"已从账上切除"——文档 §2.4.2.2 的判断正确。

- 若我错了：若 `arch_init` 的两刀之一在某些构建里被条件编译裁掉（我查到的源码是无条件调用），或 `CONFIG_MAX_CPUS` 被改成 ~237 以上使 `roundup(tramp_size)` 升到 0x2000 且最低条目基址恰为 0x1000，则第三行常态会被第二行替代；文档已把这两种变化列入"重推"条件，因此不是当前版本的错。

---

## 反证汇总

| 主张 | 结果 | 级别 |
|---|---|---|
| 01 A+D 预订与内核跨度隔离 | 反证失败（成立） | —（P2 例外清单漏 aarch64 dumpdtb Vec） |
| 02 wire 按值、接收侧深拷、版本精确比对 | 反证失败（成立） | — |
| 03 门链次序与"校验先于消费" | 反证失败（成立） | —（P2 逐腿链未列 kmain 第二次 postconditions） |
| 04 单所有者 / one-shot / 容量 panic | 反证失败（成立） | —（P2 report_static 的 one-shot 措辞） |
| 05 bump 续用、整段扣除、arena 生命周期 | 反证失败（成立） | —（P2 生命周期论据以 publish 为界，偏窄） |
| 06 C `alloc_lowest` 参数错位与第三行常态 | 反证失败（成立） | — |

本轮没有找到能推翻核心主张的阻断项（P0/P1）。三类 P2 都是"论证的边界/措辞"问题，不改变文档的事实结论：
1. §4.2.1 的"更早一笔池分配"只列 ELF `Vec`，漏了 aarch64 `load_dumpdtb_blob` 的 `Vec`（同为预订前分配，同一 fail-closed 后果）。
2. §4.4.1 的逐腿执行序以 "→ kmain 的 validate" 收尾，未列 `kmain` 在 `validate()` 前还会再跑一次 `check_boot_postconditions()`（幂等，不是门缺失）。
3. §3.7 的 "`'static` 只由 `report_static` 的 one-shot 静态存储支撑" 把 one-shot 的守卫位置说得比代码更强；真正的守卫是 `BootStorageCell::take`。
4. §4.6 用 "VM 自由页在 `build_vm_handoff` 才发布" 论证 arena 可回收；最早的重发点其实是同一 `kmain` 里的 `VmBootAllocator`（`vm_boot_exclusions` 不排除 arena），只是它同样晚于解码。规则本身（"跳转后不得再读 arena"）写对了，论证的时序说法可以更紧。
