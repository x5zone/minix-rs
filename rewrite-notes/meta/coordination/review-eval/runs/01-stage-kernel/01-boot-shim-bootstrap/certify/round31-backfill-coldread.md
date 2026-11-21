# R31 补清点独立冷读回执

- 结论：PASS
- 核验条数：53
- CLOSED：33；STILL-BROKEN：20
- 时间：2026-11-19T13:04:15+08:00

## 核验基线

- 文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`
  sha256 `5009e56814b1eab0e5f35c9a5a5b33cbcab9bc4e8b62d680f9d1a93782afe907`，3580 行
  （R31 面板基线为 3554 行 / `1601d667…`；当前已推进到 R34 修复轮之后）
- 条数构成：FACT-A 8（01–08，其 09–11 在面板中记为 SUGGESTION、不计入）＋ FACT-B 3 ＋ DESIGN-D2 9（37–45）＋ READER-A 12 ＋ READER-B 16 ＋ READER-C 5 ＝ 53。
- 判定口径：只判「该发现在**当前**文档/代码上是否仍成立」。原句被改写、代码被改、或该问题已被后续轮次修掉 = `CLOSED`；原缺陷仍在（含只修了并列子项中的一半）= `STILL-BROKEN`。
- 取证方式：全部只读，命令一律以 `cd /home/xzhao/github/minix-rs` 为前置；下文 `$D` 指 `rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`。

---

## FACT-A-01

- 判定：CLOSED
- 证据：`grep -c '不是“清活动根再回填”' $D` → 1（doc:1159）。§2.6.4 第 1 段已改写为「`pre_init` 与 `prot_init` 调用的**不是同一份 `pg_utils.o`**……`static u32_t pagedir[1024]` 因而有**两个独立副本**：unpaged 副本 A……paged 副本 B……最后 `pg_load` 把 `CR3` 切到 B」，并明写「正是这次**换根**，不是“清活动根再回填”」；`grep -c '时序脆弱' $D` → 0。原「C 确实在分页已开时清活动根」与「时序脆弱」均已不在。代码侧一致：`minix3/minix/kernel/arch/i386/pg_utils.c:19` 有 `static u32_t pagedir[1024]  __aligned(4096);`，`protect.c:360-363` 四步在 paged 副本上跑、`pg_load` 收尾。

## FACT-A-02

- 判定：CLOSED
- 证据：`grep -c '启动后仍有读者' $D` → 0；`grep -c '归还的安全性依赖' $D` → 0。doc:750「还引导段」行现写「C 的 `unpaged` 段只在启动期需要：`kinfo` 在 `kmain` 开头已拷贝到 `.usermapped` 副本，活跃的 `pagedir[]` 与静态页表池属于高半 paged `pg_utils.o` 副本；与模块无关」。§3.5 的同说法（原 doc:1806）现为 doc:1818，同口径。C 侧佐证：`sed -n '126,131p' minix3/minix/kernel/main.c` 命中 `memcpy(&kinfo, local_cbi, sizeof(kinfo));`。

## FACT-A-03

- 判定：CLOSED
- 证据：`sed -n '1072p' $D | grep -c '减 `_kern_offset`'` → 1。doc:1072 现写「这里要分清两个副本：`alloc_pagetable` 的活路径是 paged `pg_utils.o`……它的 `vir2phys` 解析到 `protect.c:33-39` 的减 `_kern_offset` 版本；`pre_init.c` 里 `return (phys_bytes) addr;` 的恒等版只属于 unpaged 副本，服务不了这条活路径」。`grep -c '引导期恒等映射成立' $D` → 0。C 侧：`grep -n 'vir2phys' minix3/minix/kernel/arch/i386/protect.c` → `33`、`36`（减 `_kern_vir_base - _kern_phys_base`）。

## FACT-A-04

- 判定：CLOSED
- 证据：`grep -c '这段代码在分页开启后不再执行' $D` → 0。doc:363 现写「分页开启之后这段代码**仍在低地址上执行一小段**，靠的正是 `pg_identity` 铺下的恒等映射：`pre_init.c:237` 的 `return &kinfo` 之后……控制权回到 `head.S:84-87`」，`grep -c '已在低地址上执行一小段' $D` → 1。原断言已反转，并补上原发现点名的 `head.S:84-87` 证据。

## FACT-A-05

- 判定：CLOSED
- 证据：`sed -n '9p' $D | grep -c '实际值未逐例核实'` → 1。doc:9 现写「BIOS 上的引导器（boot2 或 GRUB）在本仓核对到的路径上交棒（**关着分页**这一状态对 GRUB 类合规引导器由 Multiboot 规范支撑；boot2 的 `CR0.PG` 实际值未逐例核实）」。原「boot2 或 GRUB 关着分页」的普遍结论已收窄为分腿口径，与 doc:27 阶段表「boot2 实际 `CR0.PG` 状态未证明」一致。

## FACT-A-06

- 判定：CLOSED
- 证据：`sed -n '852p' $D | grep -c '第一次作为内核与模块重叠核查的输入'` → 1。doc:852 现写「这两个符号在本函数里各有更早的读点：`_kern_phys_base` 与 `_kern_size` 在 `pre_init.c:102-103` 的 `kernbase`/`kernsize` 初始化处已取址……`_kern_size` 另在 `pre_init.c:162-163` 用来算 `kinfo.kernel_allocated_bytes`……所以这里的“第一次”指的是**第一次作为内核与模块重叠核查的输入**」。C 侧：`sed -n '158,168p' minix3/minix/kernel/arch/i386/pre_init.c` 命中 `kinfo.kernel_allocated_bytes = (phys_bytes) &_kern_size;`。

## FACT-A-07

- 判定：CLOSED
- 证据：`sed -n '464p' $D | grep -c '它不是首条记录，前面还有别的键'` → 1。doc:464 现写「例如 `param_buf` 里先有 `xboard=2`（它不是首条记录，前面还有别的键），内核再写 `board=` 时……」。原发现要求的「非首条」前提已显式写入，并补「这一步**不会**改写 `machine.board_id`」。

## FACT-A-08

- 判定：CLOSED
- 证据：`sed -n '550p' $D | grep -c '两个同名定义'` → 1。doc:550 现写「`kernel_may_alloc` 是全局开关，但它有**两个同名定义**：`glo.h:76` 写的是 `EXTERN int kernel_may_alloc;`，而 `glo.h:9-12` 在 `_TABLE` 定义存在时把 `EXTERN` 改定义为空——`table.c:30-32` 正是先 `#define _TABLE` 再包含 `kernel/kernel.h`……`pre_init.c:33` 的 `int kernel_may_alloc = 1;` 是 unpaged 副本」。C 侧三点全中：`sed -n '28,32p' minix3/minix/kernel/table.c` → `#define _TABLE`；`sed -n '74,78p' minix3/minix/kernel/glo.h` → `EXTERN int kernel_may_alloc;`；`sed -n '31,35p' minix3/minix/kernel/arch/i386/pre_init.c` → `int kernel_may_alloc = 1;`。

---

## FACT-B-01

- 判定：CLOSED
- 证据：`sed -n '2859p' $D | grep -c '不是第二通道'` → 1。doc:2859 现写「`kernel_info()` 不是第二通道：`GET_KINFO`/`GET_IMAGE`、`phys_ram_view`、`harden_kernel_text` 与 `kmain` 都还通过它读取同一 `.bss` 槽」。代码侧：`grep -rn 'kernel_info()' os/kernel/src/misc.rs os/kernel/src/syscall.rs os/kernel/src/lib.rs` 命中 misc.rs:721、misc.rs:1355、misc.rs:1387、syscall.rs:2598、lib.rs:1605、lib.rs:1688，与文档列出的读者集合一致。原「只留作诊断/测试的第二通道」已删。

## FACT-B-02

- 判定：CLOSED
- 证据：`sed -n '1633p' $D | grep -c '两个直核自引导腿'` → 1。doc:1633 `memmap` 行现写「内核镜像在**两个直核自引导腿（riscv64 `bootface`、aarch64 `bootface_a64`）**与 U-Boot shim 腿的表内」，同段模块句写「**DTB 来源腿**（两个直核面与 U-Boot shim）的 bump 页在表内」。代码侧：`os/kernel-image/src/bootface_a64.rs:203` 与 `:76-95` 的常量仍在，`bootface_a64` 构造前不切除这些区间，与文档新口径一致。

## FACT-B-03

- 判定：CLOSED
- 证据：`sed -n '3299p' $D | grep -c '宿主 U-Boot 与脚本/镜像不兼容'` → 1；`sed -n '3476p' $D | grep -c '不再回打尾部 20 行'` → 1。doc:3299 现写「2 = SKIP（前置工具缺失，或宿主 U-Boot 与脚本/镜像不兼容；`test-riscv64-uboot.sh` 在装载故障时打印 `SKIP: host U-Boot could not load…` 与版本）」；§6.4 也改为「U-Boot 脚本在宿主兼容缺口时打印 SKIP 原因与 U-Boot 版本（不再回打尾部 20 行）」。脚本事实未变且已被吸收：`grep -n 'HOST_GAP\|exit 2\|could not load' os/qemu-tests/test-riscv64-uboot.sh` → 28/146/149/157/159/160/162。

---

## DESIGN-D2-37

- 判定：CLOSED
- 证据：`sed -n '1653p' $D | grep -c '两个直核面是例外'` → 1。doc:1653 现写「**两个直核面是例外**：`bootface`/`bootface_a64` 用自己文件里的 `KERN_VIRT_BASE`/`KERN_PHYS_BASE`/`KERN_SIZE` 常量，不从 ELF 头解析……因此“三个数字来自镜像”只对三条 v1 发送腿成立，直核面是“常量 + 部分构建期对账”的另一条来源」，并登记 aarch64 跨距缺口（`OPEN-DEBT.md` 的 F-R34-01）。doc:2675 字段表同行也补「直核面由 `KERN_*` 常量提供」。原「唯一数据来源的自然结果」的无条件结论已限定。

## DESIGN-D2-38

- 判定：CLOSED
- 证据：UEFI 腿已补同一断言：`sed -n '976,979p' os/boot-shim/src/uefi_helpers.rs` → `assert_eq!(layout.entry_point, layout.kern_virt_base.0, "boot-shim: ELF entry does not equal the first PT_LOAD VMA");`。原报告称该断言只在 `load_kernel_with_loader_gated`（U-Boot 唯一调用点），UEFI 直接跳 `kern_virt_base` 不校验 `e_entry`——此分工已不成立。文档也已记录覆盖范围：`sed -n '2675p' $D | grep -c 'e_entry == min(vaddr)'` → 1（原文「全文无 `entry_point`/`e_entry` 字样」已不成立）。

## DESIGN-D2-39

- 判定：STILL-BROKEN
- 证据：文案半边已修，代码半边未动。已修：`sed -n '2932p' $D | grep -c '都直接重写同一个叶并返回'` → 1，doc:2932 现写「**重叠窗口的真实语义**：同一 2 MiB 片被两个窗口各写一次时，三个架构的 2 MiB 路径都直接重写同一个叶并返回 `Ok(())`；`AlreadyMapped` 只在槽位已是 1 GiB 叶时出现」，原「`AlreadyMapped` 是重叠窗口的正常结果」已删；代码注释同步（`sed -n '1236,1240p' os/kernel/src/lib.rs` →「`map_huge` returns `AlreadyMapped` only when a 1 GiB leaf already occupies the slot, which cannot happen here because Step 1 never emits 1 GiB leaves.」）。未修：原 finding「现象」第 3 条点名的接受臂仍在原位——`sed -n '1270,1276p' os/kernel/src/lib.rs` → `match paging.map_huge(...) { Ok(()) | Err(PageTableError::AlreadyMapped) => {} ... }`。该 finding 的「最小改法」第一项要求「Step 1 只接受 `Ok(())`；`Err(...AlreadyMapped)` 走 panic」，未实施；文档虽已把该臂登记为「防御性写法……若真带，接受它会把冲突当成功，届时要收紧」（doc:2931），那是承认风险，不是消除风险。按 D2 组统一口径（最小改法的代码项是否完成），判 STILL-BROKEN。

## DESIGN-D2-40

- 判定：STILL-BROKEN
- 证据：故障面已收窄一半，另一半原样留存。已修：doc:1769 现写「**半提交状态由类型/断言挡住（仅限内存表）**：`map_tables_mut`/`memmap_slots_mut`/`reserved_slots_mut` 会置 `map_dirty`……其余五族（modules/platform/param/sources/startup）……没有 `map_dirty` 级的 report 拦截」，原「`*_slots_mut` 会置 `map_dirty`」的通配已精确化（代码侧 `grep -n 'map_dirty' os/libs/minix-boot/src/boot_storage.rs` → `144`/`154`/`174` 三处置真，一致）。未修：doc:1769 末句仍写「`set_modules_len`/`set_platform_len`/`set_param_len`/`set_memmap_sources_len`/`set_startup_len` 各自只允许提交一次，第二次调用 panic」，把 `set_startup_len` 与四个生产 setter 并列——而源码 `sed -n '292,295p' os/libs/minix-boot/src/boot_storage.rs` 是 `#[cfg(test)] pub(crate) fn set_startup_len`，生产路径根本不可调用（生产提交口是 `push_startup`，`:305`）。这正是 D2-40 标题的后半句（「把仅测试可用的 `set_startup_len` 当成生产提交」），代码未改、文档未标。判 STILL-BROKEN。

## DESIGN-D2-41

- 判定：STILL-BROKEN
- 证据：截断后继续的代码原样留存：`sed -n '362,366p' os/boot-shim/src/uboot_main.rs` → `let used = staged_elf_used_len(addr, slot).unwrap_or(slot); if used > slot { raw_serial_line("boot-shim: kernel ELF exceeds the 2 MiB staging slot — truncating"); }`，随后 `used.min(slot)` 写入；模块侧 `:389-395` 同形。`grep -rn 'MODULE_FILE_STAGE_STRIDE' os/qemu-tests os/xtask` → 零命中，打包期尺寸断言仍不存在。文档侧只把「幂等 no-op」改准（doc:1518 现写「超过槽容量时打印 `truncating` 后按槽容量写入长度，即**截断后继续**，不是停机」），但原 finding 的主项「超限选择截断后继续 + 无打包期断言 + `unwrap_or(slot)` 把解析失败伪装成整槽」三点均未改。

## DESIGN-D2-42

- 判定：CLOSED
- 证据：与 FACT-B-02 同根因、同一处修复。`sed -n '1633p' $D | grep -c '两个直核自引导腿'` → 1；两处括号均改为「两个直核自引导腿（riscv64 `bootface`、aarch64 `bootface_a64`）」/「两个直核面与 U-Boot shim」，与 §3.4.1 下三条 bullet 的「两个直核面」口径一致。`grep -c 'riscv64 直核自引导腿' $D` → 0。

## DESIGN-D2-43

- 判定：STILL-BROKEN
- 证据：`grep -n 'const _' os/kernel-image/src/bootface_a64.rs` → 零命中；该文件内断言只有 `:116`（`num_pages > 0`）与 `:335`（`!sources.is_empty()`），没有把 `KERN_PHYS_BASE + KERN_SIZE` 与 `BUMP_BASE` 联系起来的编译期断言（`sed -n '76,100p' os/kernel-image/src/bootface_a64.rs` 四个常量各自独立）。`sed -n '177,186p' os/kernel-image/src/bootface_common.rs` 的 `place_modules` 形参仍为 `table/memmap/pool/module_table/bump_alloc_pages/say/say_hex`；`PlacementPool`（`:160-172`）只有 `names`/`bump_start`/`bump_end`，无 kernel 跨度。原 finding 的两个主张（dest-vs-kernel 无检查、aarch64 常量间无编译期关系）均未被处理。

## DESIGN-D2-44

- 判定：STILL-BROKEN
- 证据：`sed -n '303,312p' os/libs/minix-boot/src/boot_storage.rs` 的 `push_startup(&mut self, record: SourceRecord)` 仍只检查容量与 `startup_committed`，无 `kind`/`origin`/`input_object` 校验。`sed -n '370,405p' os/libs/minix-boot/src/handoff_v1_read.rs` 的 `decode_startup_objects` 仍逐条 `wire.decode().ok_or(HandoffV1ReadError::BadSourceRecord)`，无 kind 过滤（该范围内唯一 `kind` 命中是字段赋值 `kind: arena[off + 64]`）。strict 门仍只做区间覆盖：`sed -n '520,531p' os/libs/minix-boot/src/kernel_info.rs` → `self.startup_objects.iter().any(|r| r.range.start <= kern_lo && u128::from(kern_hi) <= r.range.end_exclusive)`。文档 doc:1694 仍写「规范集合」而未声明身份由解码门强制。

## DESIGN-D2-45

- 判定：STILL-BROKEN
- 证据：`sed -n '1155,1179p' os/boot-shim/src/uefi_helpers.rs`：局部 `[minix_boot::startup_record(0, 0, 0); minix_boot::WIRE_STARTUP_CAP]`、局部 `push_startup` 闭包、`write_startup(&startup[..startup_n])` 原样保留；`grep -n '\.report(' os/boot-shim/src/uefi_helpers.rs` → `1064`、`1185` 两处，startup 仍写在第二次 report 之后的局部数组里；`sed -n '1333p' os/boot-shim/src/uefi_helpers.rs` → `&[], // startup_objects`，该腿 `BootReport.startup_objects` 仍为空。四份手写实现未收敛。文档 doc:1694「四个写点」与 doc:1769「UEFI 腿 `startup_objects` 切片也为空」仍在描述这一分裂，未收口。

---

## READER-A-01

- 判定：CLOSED
- 证据：与 FACT-A-05 同根因、同一处修复。`sed -n '9p' $D | grep -c '实际值未逐例核实'` → 1；doc:9 与 doc:27 不再互相矛盾。

## READER-A-02

- 判定：CLOSED
- 证据：与 FACT-A-07 同根因、同一处修复。`sed -n '464p' $D | grep -c '它不是首条记录，前面还有别的键'` → 1。

## READER-A-03

- 判定：CLOSED
- 证据：`sed -n '1751p' $D | grep -c '九页 36864 B'` → 1。doc:1751 现写「v8 的 arena 是九页 36864 B（30624 + 6144 = 36768，向上取整到 9 页）；v9 删掉 6144 B 后回到八页 32768 B，已用 30624 B」。原「加法闭合不了、读者无法把减 6144 B 与八页对齐」的缺口已由显式 v8 页数补上。

## READER-A-04

- 判定：CLOSED
- 证据：术语已前移集中定义。`sed -n '38p' $D` 一行含「**fail-closed** = 失败即停机、不降级继续；**conventional** = UEFI 内存类型中可作普通可用内存的那一类；**DEV_PIECE** = 2 MiB 分片尺寸；**poison** = 写哨兵值模拟内存被复用；**PML4** = x86-64 四级页表的顶层；**ESP** = EFI 系统分区」，`grep -c 'fail-closed\*\* = 失败即停机' $D` → 1。`wire` 在该行之前零使用：`awk 'NR<1575 && /wire/' $D` → 零命中；其定义并入 doc:1623 的括注（「wire = 跨镜像交接协议里按固定字节宽度写进 arena 的那份表示」）。原报告点名的六个词中五个进 doc:38 缩写表，`wire` 的首次正文使用落在其定义同节（§3.4）内。

## READER-A-05

- 判定：CLOSED
- 证据：同 READER-A-04。doc:38 已给「**fail-closed** = 失败即停机、不降级继续」，`sed -n '38p' $D | grep -c 'fail-closed\*\* = 失败即停机'` → 1。原「全文没有一句定义」已不成立。

## READER-A-06

- 判定：STILL-BROKEN
- 证据：doc:1330 仍写「内核与 0 号模块的 30MB 间隔是“当前镜像远小于 30MB”的构建事实……若内核恰好长到 `0x1E00000` 字节，内核项与 0 号模块会相切并被误报」，而决定该间隔的常量仍未给出：`grep -c '0x82000000' $D` → 0（全文皆无）；`grep -c '0x80200000' $D` → 2，两处分别在 doc:1830（§3.5 的 riscv64 例子）与 doc:3101（§4.8），都不在 §2.7 该段。C 源里三常量确实存在（`minix3/minix/kernel/arch/earm/pre_init.c:182` → `#define MB_MODS_BASE  0x82000000`；`minix3/minix/kernel/arch/earm/kernel.lds:4` → `_kern_phys_base = 0x80200000;`），读者原则上可复算 `0x1E00000`，但文档未给锚点，原「无法复算」仍成立。

## READER-A-07

- 判定：STILL-BROKEN
- 证据：三处引用点清单已收敛到两套、未统一。`grep -n '只引用结论\|引用它的结论\|引用那个结论' $D` → doc:153、doc:286 两处，措辞已统一为同一句「本篇只在 §2.2、§2.6.4、§2.8、§7.5 与……」，`sed -n '286p' $D | grep -c '§2.6.4'` → 1（原报告指 §2.2 漏 §2.6.4，已补）。但第三处 doc:363 仍写「判据在 `02-higher-half-kernel.md` §2.4；§1.4/§2.8 只引用结论」，`sed -n '363p' $D | grep -c '§1.4/§2.8 只引用结论'` → 1——仍只列 §1.4/§2.8，漏 §2.2/§2.6.4/§7.5，与另两处的五项清单不一致。原 finding 的三份互斥清单已降为「五项 vs 两项」的不一致，缺陷仍在。

## READER-A-08

- 判定：CLOSED
- 证据：`grep -c '它曾装两种语义' $D` → 1（doc:1693）。双写已删，只剩一处「它曾装两种语义：其一（x86 内联窗口）……其二（UEFI 链通用）……」。

## READER-A-09

- 判定：STILL-BROKEN
- 证据：两处编辑史括注原样留存。doc:2925「（源码 Step 1 的旧 NOTE 曾在 BOOT-SCOPE 2a 重写中删除；这里的 ISA 语义按 RISC-V 特权规范陈述，不再冒充现行源码注释。）」——`grep -c '不再冒充' $D` → 1；doc:2927「现注释已改写为 `map_huge` 错误集口径，见下文“忽略错误”一条」——`grep -c '忽略错误' $D` → 1（唯一命中即此句本身）。原 finding 的两句括注均未删改或移出结论句。

## READER-A-10

- 判定：STILL-BROKEN
- 证据：同一事实仍带完整因果重复五处：`grep -n '两个镜像各链接一份\|另一份副本\|不共享游标\|合并方案已回滚' $D` → 5 处命中，落点 doc:2050（§3.7）、doc:2300（§4.1）、doc:2410（§4.2.1 动作 1）、doc:3023（§4.7）、doc:3029（§4.7）。原 finding 要求「保留 §4.7 的完整论述，§3.7/§4.1/§4.2.1 压缩成一句带指针」，当前 doc:2050「两个镜像各链接一份 `BootAlloc`，接收方若从区间基址重新发页，会把 boot-shim 已经用作页表页的页再发一次」与 doc:2300「接收镜像自己链接的那份 `BootAlloc` 副本」仍是独立完整论述。重复计数未降。

## READER-A-11

- 判定：CLOSED
- 证据：doc:1247 现写「`/* ……（4 行标题注释省略，原文见 earm/head.S:22-25）…… */`」，行号已收窄到 22-25。源侧核对：`awk 'NR>=22 && NR<=25' minix3/minix/kernel/arch/earm/head.S` → 恰 4 行（三行 `/*===*/` 框 + 一行 MINIX 标题），与「4 行」自洽。`grep -c '20-27' $D` → 0。

## READER-A-12

- 判定：CLOSED
- 证据：doc:2743 `KernelInfo::validate`（`kmain` 入口）行现写「复核 `kern_size`/`memmap`/`param_buf` 非地址不变量；`boot_modules` 非空……；`memmap_sources` 非空时另查来源覆盖（条件性，见 §3.4.1）」，`sed -n '2743p' $D | grep -c '来源覆盖'` → 1。原「§4.4 门表只把它记在 `validate_requiring_sources` 名下」已不成立。代码侧佐证该条件性检查确实在非 strict 路径：`sed -n '500,510p' os/libs/minix-boot/src/kernel_info.rs` → `} else if !self.memmap_sources.is_empty() { match crate::memmap_source::audit_source_coverage(...)`。

---

## READER-B-01

- 判定：CLOSED
- 证据：doc:71 读图规则已重写：「**主链的框序是框 2 → 框 1 → 框 3 → 框 4**：D2 是框 2→1，D1 是框 1→3（图上没有箭头，靠文字给出），D3 与 D5 发生在框 3 内部，D4 是框 4 的底边（框 4 → `kmain`）。框 3 → 框 4 的竖箭头只承载字段级回填 `freepde_start`（回填不计入 D1–D5），不是 D4。」——`sed -n '71p' $D | grep -c 'D4 是框 4 的底边'` → 1，`grep -c '框 3 → 框 4 的竖箭头只承载字段级回填'` → 1。原三点中前两点（D4 挂错边并自相矛盾、D3 在链上无处安放）已修：D4 归位到框 4 底边，D3/D5 明确为「框 3 内部」。第三点（开篇与读图规则的重复）属 P3 冗余残留，不构成原 P2 判据。

## READER-B-02

- 判定：CLOSED
- 证据：与 FACT-A-05/READER-A-01 同根因、同一处修复。`sed -n '9p' $D | grep -c '未逐例取证'` → 1。原 finding 的三点中，第 1 点（同一状态在两处被赋不同置信度，本条的 P2 依据）与第 3 点（doc:2968 的第三处表述）均已消：doc:2968 现写「本仓生产链不存在那条 BIOS 链，此处指 C 侧语义的统一」。第 2 点（引号内「分页未开（x86-32）」非阶段表逐字子串）仍在（`grep -c '分页未开（x86-32）' $D` → 1，阶段表原串含分号），属引文格式级残留，不改变本条判定。

## READER-B-03

- 判定：STILL-BROKEN
- 证据：术语仍两套、宾语仍不统一。`grep -c '两个构造入口' $D` → 2（doc:1589 §3.4.1 标题、doc:2251 §3.10 表行），而 doc:1642 与 doc:2631 已改称「两条产出入径」；doc:2251 仍写「3.4 两张表（可用 / 保留）+ 两个构造入口」。doc:2629 标题仍以「`KernelInfo` 的构造：两条产出入径与一条校验门链」开头，而 doc:2631 正文自承「三条 v1 腿……发送侧不构造 `KernelInfo`」，即标题的宾语与正文自述仍互相拆台。§4.4 正文对 v1 接收侧解码构造仍只有一句（`awk 'NR>=2629 && NR<=2750' $D | grep -c '接收侧'` → 4，其中仅 doc:2653 附近一句提「v1 接收侧由 wire 解码后填同一结构」）。原 finding 的三点均仍有对应。

## READER-B-04

- 判定：STILL-BROKEN
- 证据：防线 D 的成员仍两说。doc:144（§1.4 编号索引）写「C 模块×内核、D 接收侧复核（模块×模块 + 内核×模块；内核×模块与 C 故意重复）」——`sed -n '144p' $D | grep -c '故意重复'` → 1；doc:2505（§4.3 首次展开）仍写「C 守模块 vs 内核跨度……D 在接收侧守模块 vs 模块（`KernelInfo::validate`，§4.4）」；doc:3571（§7.5 速查）仍写「D `KernelInfo::validate`（模块 × 模块，接收侧复核）」。代码侧 `KernelInfo::validate` 同时查模块×内核与模块×模块（`sed -n '346,390p' os/libs/minix-boot/src/kernel_info.rs` 的「Kernel image vs module spans, and module-vs-module spans」段），即 §1.4 口径正确、§4.3/§7.5 口径漏项——原 finding 的「成员范围两说」仍成立，且 §1.4 自承「内核×模块与 C 故意重复」说明四道防线不是互斥划分这一点也仍在。

## READER-B-05

- 判定：CLOSED
- 证据：doc:3297 现明确分域裁定：「**§3.7 是生产腿计数的唯一权威，本节是门链计数的唯一权威**，两者不互相改写」，`sed -n '3297p' $D | grep -c '§3.7 是生产腿计数的唯一权威，本节是门链计数的唯一权威'` → 1；doc:133 的计数约定也写「说“五条门链”时指 §5.2.3 的真机门链」。原 finding 的核心（三处对「谁裁定五条门链」给出三个答案、违反权威表唯一）已由分域裁定解决：§3.7 管生产腿、§5.2.3 管门链、§1.4 只做索引并指向 §3.7。

## READER-B-06

- 判定：STILL-BROKEN
- 证据：完整名单式复述仍在三处：`grep -c '两条 UEFI 链、riscv64 U-Boot 直载测试链' $D` → 3（doc:1944 §3.7 对账表行、doc:3297 §5.2.3、doc:3463 §6.4 第一层注释块）。doc:133 的第四条已改为指向式（只说「五条门链指 §5.2.3 的真机门链」，不再列全名单），但原 finding 要求「后两处应只给指向，现在每次都是完整名单」——doc:1944 与 doc:3463 仍是完整枚举，`grep -c '只给指向' $D` → 0。缺陷形态未变（复述次数 4→3）。

## READER-B-07

- 判定：CLOSED
- 证据：「图上没有箭头，靠文字给出」现只出现一次：`grep -c '图上没有箭头，靠文字给出' $D` → 1（doc:71）；doc:73 的「跨框依赖（图上画不下）：框 1 → 框 3（跳框）——建表的第二步（`pg_identity`）要读清单算出的内存上界」只补机制理由，不再重复「没有箭头」这一事实。「不计入 D1–D5」两处分工不同、不再是近同措辞重复（`grep -c '回填不计入 D1–D5'` → 1，`grep -c '不计入上面五条次序约束'` → 1，一处是箭头说明、一处是附注标题）。原 finding 的主项（doc:71 与 doc:73 用近同措辞说两遍同一事实）已消。

## READER-B-08

- 判定：STILL-BROKEN
- 证据：doc:744 仍写「接下来两处“改这份账”的动作发生在 `pre_init()` 返回之后」（`sed -n '744p' $D | grep -o '接下来两处[^—]*'` → 命中），doc:786 仍写「**2.4.2 结算**（C 侧）：这三处下游动作在 C 里……」（`sed -n '786p' $D | grep -c '这三处下游动作'` → 1）。「两处」与「三处」的对立原样留存；doc:744 同段的括注「下表把这两处连同 `pre_init` 内的“切占用”一处并列（第一行属 §2.5，只作对照，不是本节主体）」只解释了表格构成，未消结算句把三处统称「下游动作」的冲突。

## READER-B-09

- 判定：STILL-BROKEN
- 证据：与 READER-A-07 同一处缺陷。doc:153 与 doc:286 已统一为同一份五项清单（`sed -n '286p' $D | grep -c '§2.6.4'` → 1，原「§2.2 漏 §2.6.4」已补），但 doc:363 的第三处仍只列 §1.4/§2.8：`sed -n '363p' $D | grep -c '§1.4/§2.8 只引用结论'` → 1。三份封闭声明降为「五项清单 ×2 vs 两项清单 ×1」的不一致，`k_initial_stktop` 的引用点集合在文档内仍无单一答案。原 finding 的判据（同一事实的权威归属不唯一）仍成立。

## READER-B-10

- 判定：STILL-BROKEN
- 证据：两个按引号标签指路的指针仍落空。(1)「落地区与交接容量」：`grep -c '落地区与交接容量' $D` → 2，两处都是**引用句**（doc:740、doc:1620），§4.5 表自身的引言在 doc:2841 为「七个落地区、一个名字池、一行 `KernelInfo.memmap` 切片视图与两个 `VmBootHandoff` 交接字段的容量与溢出行为」，仍无该表名。(2)「忽略错误」：`grep -c '忽略错误' $D` → 1，唯一命中即 doc:2927 的引用句本身；§4.6 的条目名仍是「Step 1 的 `map_huge(...)` 用 `match` 区分两类结果」（doc:2931）。原 finding 的两支均未改。

## READER-B-11

- 判定：STILL-BROKEN
- 证据：`sed -n '2418p' $D | grep -c 'dumpdtb'` → 0。doc:2418（§4.2.1 动作 9 约束）仍写「窗口表此时已包含 shim image/stack（UEFI）或 `[SHIM_LINK_BASE, UBOOT_SHIM_STACK_TOP)`（U-Boot），以及内核 LMA、root/bump/arena、模块、平台源、设备窗口；x86 另含低 2 MiB AP 跳板窗口」，不含可选 `dumpdtb` 窗口；而 doc:2920（§4.6 第一层）写「可选的 `dumpdtb` 临时缓冲、内核 LMA、root/bump/arena、12 个模块、固件描述符表与设备窗口」（`sed -n '2920p' $D | grep -o '可选.*设备窗口'` → 命中）。清单不一致原样保留。

## READER-B-12

- 判定：CLOSED
- 证据：与 FACT-A-07/READER-A-02 同根因、同一处修复。`sed -n '464p' $D | grep -c '它不是首条记录，前面还有别的键'` → 1，最小复现前提已补齐。

## READER-B-13

- 判定：STILL-BROKEN
- 证据：承重过程词仍在。`grep -n 'R23 的 F-R17-04' $D` → doc:1614（「`authority` 由 R23 的 F-R17-04 加入，总数从 13 变成 14」）；`grep -n 'R23 把五个切片' $D` → doc:2756（代码引文头）；`grep -n 'R17 的三条后置负控' $D` → doc:3226；`grep -n 'F-R23-05' $D` → doc:2114 与 doc:3033；`grep -n 'DESIGN-D2-17' $D` → doc:2724；原 §6.4 的「§5.3 TODO-08 的关闭记录」现位于 doc:3450。原 finding 点名的六处全部仍在，且仍是结论句的依据而非可跳过索引。

## READER-B-14

- 判定：STILL-BROKEN
- 证据：两处层数描述仍互不相容。doc:3025：「跨镜像路径上必须先 `resume_boot_pt_alloc`、再进各架构委托壳（aarch64 经 `arch_boot_resume_high_half`，x86_64/riscv64 **直接进** `resume_additive`）」——`sed -n '3025p' $D | grep -o 'x86_64/riscv64 直接进 .resume_additive.'` → 命中。doc:3094：「跨镜像三腿收敛在 `arch_boot_resume_v1`——三架构都先经 `resume_after_v1_decode`」——`sed -n '3094p' $D | grep -o '三架构都先经 .resume_after_v1_decode.'` → 命中。代码站在 doc:3094 一侧：`grep -n 'resume_after_v1_decode' os/kernel/src/lib.rs` → `741`（调用点）、`747`/`755`/`774`（三架构各一版定义），x86_64 与 riscv64 的 `resume_after_v1_decode` 体内再转 `resume_additive`。故 doc:3025 的「直接进 `resume_additive`」仍错，原 finding 的层数冲突原样保留。

## READER-B-15

- 判定：CLOSED
- 证据：§3.4.1 标题已改为「`KernelInfo` 结构与它的两个构造入口」（doc:1589），不再是原「交接的两个维度与 `KernelInfo` 结构」；`grep -c '两个维度' $D` → 1，唯一命中在 doc:1579 的 §3.4 正文（「分两个维度说」），标题不再承诺本节正文不含的内容。原 finding 的核心（标题承诺「两个维度」、正文只有结构体与字段表）已消。

## READER-B-16

- 判定：CLOSED
- 证据：`grep -c '只给指向' $D` → 0。doc:174 的 §2.8 行现写「本章收成一张调用树与五条次序约束（D1–D5）（逐条给陈述与证据锚点，机制推演按条目回指本书小节）」，原「其中两条只给指向」这一不可复核的计数已删除并替换为可核对的描述。

---

## READER-C-01

- 判定：CLOSED
- 证据：doc:32 已补取证范围边界：「取证范围见 §1.4 的 boot2 边界声明（规范只撑 GRUB 类合规引导器；boot2 只核实到代码事实）」，`sed -n '32p' $D | grep -c '规范只撑 GRUB 类合规引导器；boot2 只核实到代码事实'` → 1。doc:154 边界声明也改为「这条边界不改变“入口执行设栈指令”的代码事实；“必须以设栈开始”的**必要性**只对 Multiboot 合规引导器成立，对 boot2 只取证到代码事实」。原 finding 指出的「必要性未取证却下结论」的缺环已补。

## READER-C-02

- 判定：CLOSED
- 证据：doc:1760 末句现写「这条不变量与 §2.4.1.3 的“跳板生命周期”同级：都是**观测/预防性**前提，不是“某条机制已由实验证实”的结论；把它写成已证设计会高估证据强度」，`sed -n '1760p' $D | grep -c '都是\*\*观测/预防性\*\*前提'` → 1。原 finding 要求的「显式指出它与 §2.4.1.3 处同一证据强度等级」已逐字落地。

## READER-C-03

- 判定：STILL-BROKEN
- 证据：doc:2296 仍只陈述两套预算而不给取舍因果：「……`boot_shim::BOOT_PT_BUMP_PAGES` 是**页表页预算**（常量值 1024，定义在 `lib.rs`；1024 页 = 4MiB；两个直核面另用 `bootface` 的 `PT_BUMP_PAGES = 256`）」，`sed -n '2296p' $D | grep -c '只覆盖\|为什么\|因为'` → 0。代码侧两个常量确为独立定义、无关联：`grep -n 'PT_BUMP_PAGES' os/kernel-image/src/*.rs` → `bootface.rs:134`、`bootface_a64.rs:100` 各 `= 256`。原 finding 的「读者拿 256 页去推直核面够不够，得不到可检验结论」仍成立。

## READER-C-04

- 判定：CLOSED
- 证据：与 FACT-A-05/READER-A-01/READER-B-02 同根因、同一处修复。`sed -n '9p' $D | grep -c 'boot2 的 .CR0.PG. 实际值未逐例核实'` → 1，首段已采用阶段表口径；原 finding 建议的「本仓路径的 C 侧按未开分页假定并防御性清 `PG`；boot2 实际状态未证明」已以等价形式实现。

## READER-C-05

- 判定：CLOSED
- 证据：doc:3297 已显式写出原 finding 要求的差集对账：「**五条生产腿减去五条门链，缺的正是 aarch64 直核面的专用启动门**；它只有 SMP 门间接覆盖，这是本篇证据总账里的已知缺口（§5.1 M14）」，`sed -n '3297p' $D | grep -c '缺的正是 aarch64 直核面的专用启动门'` → 1。

---

## 汇总

| 组 | 条数 | CLOSED | STILL-BROKEN |
|---|---|---|---|
| FACT-A（01–08） | 8 | 8 | 0 |
| FACT-B（01–03） | 3 | 3 | 0 |
| DESIGN-D2（37–45） | 9 | 3（37/38/42） | 6（39/40/41/43/44/45） |
| READER-A（01–12） | 12 | 8（01–05/08/11/12） | 4（06/07/09/10） |
| READER-B（01–16） | 16 | 7（01/02/05/07/12/15/16） | 9（03/04/06/08/09/10/11/13/14） |
| READER-C（01–05） | 5 | 4（01/02/04/05） | 1（03） |
| 合计 | 53 | 33 | 20 |

### 判定口径中需要说明的一处尺度

DESIGN-D2 组的判定按「原 finding 的最小改法里，代码项是否完成」统一尺度：

- D2-37（无代码项，纯文档收窄）→ 文档已收窄并登记缺口，CLOSED。
- D2-38（最小改法要求 UEFI 侧补同一断言）→ `uefi_helpers.rs:976-979` 已补，CLOSED。
- D2-42（纯文档枚举补全）→ 已补，CLOSED。
- D2-39（最小改法第一项要求接受臂改 panic）→ 文案已修、接受臂未改，STILL-BROKEN。
- D2-40/41/43/44/45 → 代码项未完成，STILL-BROKEN。

若改用「文档描述是否已如实反映现状」这一更窄的尺度，D2-39/41/45 会翻成 CLOSED，总数变为 36/17。本回执采用前一口径（更严），并在每条证据里写明未完成的是哪一半。

### 未闭合条目的共同形态

- 设计侧六条（D2-39/40/41/43/44/45）都是**代码未动或只修一半**：Step 1 的 `AlreadyMapped` 接受臂、`set_startup_len` 的测试可见性未在文档标出、U-Boot 装载槽的截断继续、aarch64 池常量的编译期断言缺位、startup 记录身份未强制、UEFI 的 startup 局部数组。其中 D2-41 与 D2-45 的文档描述已如实交代现状（「截断后继续」「四个写点」），属「文档如实记录了未收口的设计」；D2-40 则是文档仍把测试专用 setter 与生产 setter 并列，属遗留口径错误。
- 读者侧十四条（RA-06/07/09/10、RB-03/04/06/08/09/10/11/13/14、RC-03）均为**文档结构/表述层**，与代码正确性无关：缺常量锚点、引用清单未统一、编辑史括注未清、重复论述、术语两义、防线成员两说、名单式复述、两处/三处口径、标签指针落空、窗口清单漏项、承重过程词、层数描述冲突、两套预算无因果。

结论：冷读验证通过，53 条判定已逐条落证。
