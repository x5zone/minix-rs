# P-DESIGN-D2 独立设计审查（round 31）

## 审查范围与依据

- 目标：`rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 第 3 章（Rust 设计决策）与第 4 章（实现详解）。
- 关联代码：`os/boot-shim/src/{loader,uefi_helpers,opensbi_helpers,uboot_main,lib,ebs_map,ebs_pair}.rs`、`os/libs/minix-boot/src/{boot_storage,kernel_info,handoff_v1,handoff_v1_read,handoff_v1_write,memmap_source,boot_prepare}.rs`、`os/kernel/src/{lib,boot_alloc,globals,vm_handoff,boot_policy}.rs`、`os/arch/src/arch/{paging,paging_ext,pt_alloc}.rs`、`os/arch/src/{x86_64,arm64,riscv64}/paging.rs`、`os/kernel-image/src/{main,bootface,bootface_a64,bootface_common}.rs`、四份 `Cargo.toml`、`os/kernel-image/check-layout.sh`。
- 只读审查；未采信作者解释、历史评审或其他角色输出。代码状态为当前工作树（含未提交修改）。
- 关注维度：设计不对称、非法状态、所有权/接口顺序、抽象边界、最小化机会、文档与代码设计陈述不符。

## 指定处置复核（D2-29..36 的四个主题）

| 主题 | 当前代码 | 与文档的关系 | 结论 |
|---|---|---|---|
| `entry_point` 交叉校验 | `loader.rs:392-401` 新增 `assert_eq!(layout.entry_point, layout.kern_virt_base.0)`，但只在 `load_kernel_with_loader_gated` 内；该函数唯一调用点是 U-Boot 的 `opensbi_helpers.rs:370`。UEFI 生产入口走 `compute_kernel_layout`（`uefi_helpers.rs:970`）+ `load_segments_into_phys_memory`（`:994`），不经过此断言。 | 文档第 3/4 章通篇没有提到 `e_entry` 或该交叉校验；§4.4 的字段表只讲 PT_LOAD 派生的三个数字。 | 处置不完整：断言只覆盖 U-Boot 一条腿，两条 UEFI 产线仍直接跳 `kern_virt_base`；文档也未记录。见 D2-38。 |
| `BootStorage` push 提交断言 | `boot_storage.rs` 新增 `map_dirty` 与五个 `*_committed` 标志；`push_platform_source`、`push_startup` 在提交后 append 会 panic；`set_*_len` 重复提交会 panic。但 `map_dirty` 只由 `memmap_slots_mut`/`reserved_slots_mut`/`map_tables_mut` 设置；`startup_slots_mut` 无任何调用点也不设脏标志；`set_startup_len` 是 `#[cfg(test)]`；platform 同时存在 push 与 set 两种非互斥提交路径。 | §3.4.5 第 1757 行说 “`map_tables_mut`/`*_slots_mut` 会置 `map_dirty`” 并把 `set_startup_len` 列为生产侧一次性提交。 | 处置部分落地，但覆盖范围小于文档声称。见 D2-40。 |
| 依赖图 | 四份 manifest 与 §3.3 的依赖表/图一致：boot-shim → minix-types/minix-boot/minix-arch/minix-kernel/minix-plat/minix-elf/fdt/可选 uefi+uefi-raw；kernel 无 boot-shim、无 uefi；minix-boot → minix-types+fdt；kernel-image 的 target 表在 riscv64/aarch64 各加 boot-shim(opensbi)+fdt。图中 `minix-boot → fdt` 与 `kernel-image → minix-types` 未画，但图下已声明“只画主边”。 | 一致。 | 通过，无 finding。 |
| startup 归属 | U-Boot 在 `prepare_boot` 内用 `storage.push_startup` 写齐五类并序列化 `report.startup_objects`；两个直核面同样用 `BootStorage`；UEFI 则在 EBS 之后用栈上局部 `[SourceRecord; 20]` 写 wire，`BootStorage.startup_objects` 与 `BootReport.startup_objects` 都为空。 | §3.4（第 1682 行）与 §3.4.5（第 1756/1758 行）已描述这一分裂，但没有说明为什么 UEFI 需要第二个所有者，也未给出生产者/解码侧的记录身份约束。 | 处置与代码一致，但留下两个设计问题：UEFI 的第二个所有者（D2-45）与 startup 记录身份未强制（D2-44）。 |

---

## FINDING DESIGN-D2-37

**标题**：§4.4 与 §3.4.1 把“布局数字来自 ELF”说成五条腿的统一事实，两个直核腿实际使用硬编码链接常量

**位置**：
- 文档 §3.4.1 “升为字段”的设计细节，第 1641 行：“这三个数字……现在的来源是 ELF 的 PT_LOAD 段……让 shim 自己解析镜像……唯一数据来源（镜像本身）的自然结果”。
- 文档 §4.4 字段表，第 2659 行：`kern_virt_base` / `kern_phys_base` / `kern_size` 的“谁填”写“装载函数”，“值从哪来”写“ELF 各 PT_LOAD 段的 `min(vaddr)` / `min(paddr)` / `max(paddr+memsz) - min(paddr)`”。
- 代码：`os/kernel-image/src/bootface.rs:105-121`（`KERN_PHYS_BASE`、`KERN_VIRT_BASE`、`KERN_SIZE` 三个字面量），`os/kernel-image/src/bootface_a64.rs:76-83`（同形三个字面量）；`build_kernel_info` 调用点在 `bootface.rs:427-439`、`bootface_a64.rs:400-410`。

**现象**：
只有两条 UEFI 链与 U-Boot shim 通过 `compute_kernel_layout` 解析 ELF 的 PT_LOAD 段得到这三个数字。两个直核腿不解析自己的 ELF，而是把三个链接期字面量直接传给 `build_kernel_info`。`check-layout.sh` 的 L0/L9/A1 只是在构建期把常量与镜像对账，运行期 `KernelInfo` 并不从镜像派生。因此“数字与镜像一致是唯一数据来源的自然结果”只在 3/5 条腿上成立；另外 2 条腿的信任来源是两份手工维护的常量加一个构建脚本。

**影响**：
1. 读者会以为五条腿都不存在“调用者传错布局数字”的接口面，从而低估直核腿的维护风险。
2. 源码注释本身记录了这类漂移的真实事故：`bootface.rs:113-121` 写明硬编码的 2 MiB 拷贝曾让 `IMAGE_HEAP` 增长把真实跨度推到 4 MiB，高半只映射了一半镜像、`kern_stack_top` 落在 `.bss` 中部，静默损坏。文档的“ELF 派生”叙述恰好掩盖了这个已知失败模式。
3. 这不是当前运行期缺陷（`check-layout.sh` L9 会在构建期拦住超跨度），但设计模型与代码不一致；若未来新增直核腿或复制 `KERN_*` 常量，L9 只校验被检查的那份产物，不校验所有消费者。

**最小改法**：
- 把 §4.4 该行拆成两行或在“谁填/值从哪来”里分腿写清：v1 三条腿 = 装载函数从 ELF PT_LOAD 派生；两个直核腿 = 链接期常量（`bootface*.rs`），由 `check-layout.sh` 的 L0/L9/A1 对账。
- 同步改写 §3.4.1 第 1641 行的“唯一数据来源”结论，限定到“需要 shim 解析镜像的腿”。
- 可选但更彻底：让链接脚本导出 `_kern_size` 一类符号（C 的做法），或由构建生成布局头文件，使直核腿也只有一个数据来源，而不是两份常量。

**严重度**：P1（设计陈述与代码不符，且掩盖了源码已记录的真实事故模式；当前由构建期脚本兜底，不是活运行期 bug）

**反证路径**：
1. 读 `bootface.rs:105-121`、`bootface_a64.rs:76-83`：三个常量是字面量，没有 ELF 解析。
2. `grep -rn "compute_kernel_layout" os/`：只有 `loader.rs` 定义、`uefi_helpers.rs:970` 与 `opensbi_helpers.rs:370` 附近使用；两个 `bootface` 都不调用。
3. 读 `bootface.rs:427-439`：`build_kernel_info(..., VirBytes(KERN_VIRT_BASE), PhysBytes(KERN_PHYS_BASE), KERN_SIZE, ...)`，即调用者常量。
4. `os/kernel-image/check-layout.sh` 的 L0/L9 是构建期断言，不是运行期派生。

---

## FINDING DESIGN-D2-38

**标题**：`entry_point` 交叉校验只加在 U-Boot 路径上；UEFI 两条产线仍把 `kern_virt_base` 当作跳转目标而不校验 `e_entry`

**位置**：
- 代码：`os/boot-shim/src/loader.rs:392-401`（断言）；`os/boot-shim/src/opensbi_helpers.rs:370`（唯一调用 `load_kernel_with_loader_gated` 的生产点）；`os/boot-shim/src/uefi_helpers.rs:969-996`（UEFI 读 ELF、`compute_kernel_layout`、`load_segments_into_phys_memory`）；`os/kernel/src/lib.rs:510`（`let entry = core::hint::black_box(report.kern_virt_base.0);`）。
- 文档：§4.4 第 2659 行（只讲三个 PT_LOAD 数字）；§4.3 第 2489 行起的防线清单未提 `e_entry`；全文无 `entry_point`/`e_entry`/`交叉校验` 字样。

**现象**：
本轮新增的断言注释说“loader 是最后见到 `e_entry` 的地方”，但只有 U-Boot 的 `load_kernel_with_loader_gated` 见到 `e_entry`。UEFI 生产入口直接调用 `compute_kernel_layout`（返回 `entry_point` 但随后丢弃）和 `load_segments_into_phys_memory`，最后跳到 `report.kern_virt_base`。如果链接脚本的 `ENTRY(_start)` 或 `.text.boot` 次序发生变化，而 `check-layout.sh` 没有覆盖某次打包（例如 ESP 上放了另一份 `kernel.elf`），UEFI 链会跳到首段 VMA 而不是 `_start`；`entry_point` 字段在 UEFI 路径上完全没有读者。

**影响**：
两条 UEFI 产线的跳转目标失去最后一道运行期校验。故障在开启分页之后以取指异常出现，与“链接布局漂移”之间隔着页表切换，归因困难；这正是该断言在 U-Boot 路径上要消除的失败模式。文档也没有告诉读者这条断言存在、覆盖哪条腿。

**最小改法**：
- 把断言下沉到 `compute_kernel_layout`（所有解析 ELF 的调用者自动获得；需把 `loader.rs` 测试夹具的 `e_entry` 从 `0xFFFFFFFF80001000` 改成与首段 VMA 一致，`loader.rs:585` 附近）。
- 或在 `uefi_helpers.rs:970` 的 `compute_kernel_layout` 之后补同一断言，作为最小改动。
- 在 §4.4 的 `kern_*` 行或 §4.3 防线清单加一句：跳转目标是首段 VMA，`e_entry` 必须与它相等；三条 ELF 解析腿都在装载前校验。

**严重度**：P2

**反证路径**：
1. `grep -rn "load_kernel_with_loader_gated" os/boot-shim/src`：只有 `loader.rs` 定义与 `opensbi_helpers.rs:370` 调用。
2. 读 `uefi_helpers.rs:969-996`：`compute_kernel_layout` 后直接 `load_segments_into_phys_memory`，无 `entry_point` 比较。
3. 读 `kernel/src/lib.rs:510`：entry 取 `report.kern_virt_base.0`。
4. `grep -n "entry_point" rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`：零命中。

---

## FINDING DESIGN-D2-39

**标题**：§4.6 对 `map_huge` 返回 `AlreadyMapped` 的语义说反了；Step 1 因此把一个真实的 1 GiB 叶冲突当成“重叠窗口的正常结果”接受

**位置**：
- 文档 §4.6，第 2905-2906 行：“`AlreadyMapped` 是重叠窗口的正常结果（同一个 2 MiB 叶被重写成相同标志）”“第二次 `map_huge` 可能返回 `AlreadyMapped`（同一个叶已经存在）；它表示‘这次写把同一叶重写成相同标志’”。
- 代码：`os/kernel/src/lib.rs:1260-1273`（新增注释“`AlreadyMapped` is the expected result for an overlapping window”与 `Ok(()) | Err(PageTableError::AlreadyMapped) => {}`）；`os/arch/src/x86_64/paging.rs:1212-1229`；`os/arch/src/arm64/paging.rs:1182-1200`；`os/arch/src/riscv64/paging.rs:1173-1194`。

**现象**：
三个架构的 `map_huge` 在 2 MiB 路径上都无条件写叶条目并返回 `Ok(())`；`AlreadyMapped` 只在目标槽位已经是一个 1 GiB 叶、且继续按 2 MiB 走会“降级”该叶时返回。也就是说：
- 同一 2 MiB 叶被两个窗口重写 → 返回 `Ok(())`，不是 `AlreadyMapped`；
- `AlreadyMapped` 表示“这里已经有一个 1 GiB 叶，再写 2 MiB 会破坏它”，是需要 fail-closed 的真实冲突。

文档与代码注释把两者说反了，并且 Step 1 的 `match` 把 `AlreadyMapped` 加入接受分支。当前 Step 1 只写 2 MiB 叶、从不创建 1 GiB 叶，所以该分支据信不可达；但“不可达”正是靠这段代码自己的前提维持的，一旦未来 Step 1/Step 2 在同一槽位引入 1 GiB 叶，或接收侧换入一张外来根，接受分支会让请求的 2 MiB 映射静默不安装（槽位仍是被 1 GiB 叶覆盖的旧物理基址/旧属性），而不是停机。

**影响**：
1. 文档给出的“重叠无害”论证建立在一个不存在的机制上；真正的原因是“同一槽位写入相同 2 MiB 叶返回 `Ok`，且 Step 1 从不产生 1 GiB 叶”。
2. 错误处理臂把唯一能表示“与 1 GiB 叶冲突”的错误码当成成功；如果前提被破坏，表现是 MMIO 属性或物理映射错误，而不是启动中止。
3. 该注释是本轮修改新引入的，属于新的文档-代码同步偏差。

**最小改法**：
- 代码：Step 1 只接受 `Ok(())`；`Err(PageTableError::AlreadyMapped)` 走 `panic!("boot identity map: 1 GiB leaf conflict ...")`（或实现显式 demotion，但那超出 Step 1 的职责）。
- 文案：把 §4.6 第 2905-2906 行改为“同一 2 MiB 叶的重复写入返回 `Ok` 并覆盖为相同标志；`AlreadyMapped` 只表示槽位已有 1 GiB 叶，Step 1 视为致命冲突”。同步修正 `kernel/src/lib.rs:1260-1265` 的注释。

**严重度**：P2（当前设计下该分支不可达，但错误语义与文档机制均错，且接受臂是未来 1 GiB 冲突的唯一防线）

**反证路径**：
1. 读 `x86_64/paging.rs:1212-1229`：2 MiB 路径先检查 `e3` 是否 PRESENT+PS（1 GiB 叶），是则返回 `AlreadyMapped`；否则无条件 `write_entry(pd, i2, ...)` 并 `Ok(())`。
2. 读 `arm64/paging.rs:1182-1200`：同样只在 `e1` 是 1 GiB block 时返回 `AlreadyMapped`；2 MiB 写入无条件。
3. 读 `riscv64/paging.rs:1173-1194`：只在 L2 项是 1 GiB leaf 时返回 `AlreadyMapped`；2 MiB 写入无条件。
4. 读 `git diff -- os/kernel/src/lib.rs` 中该 hunk 被删除的旧注释：原文明确写“`AlreadyMapped` needs a pre-existing 1 GiB leaf, which Step 1 never creates”。

---

## FINDING DESIGN-D2-40

**标题**：`BootStorage` 的“半提交状态”防护只覆盖两张内存表；文档却声称所有 `*_slots_mut` 都会置脏，并把仅测试可用的 `set_startup_len` 当成生产提交

**位置**：
- 代码：`os/libs/minix-boot/src/boot_storage.rs:143-176`（只有 `memmap_slots_mut`/`reserved_slots_mut`/`map_tables_mut` 设置 `map_dirty`）；`:194-207`（`module_slots_mut` 无脏标志）；`:224-251`（`platform_slots_mut` 无脏标志；`push_platform_source` 与 `set_platform_len` 两条提交路径）；`:272-313`（`memmap_source_slots_mut`/`startup_slots_mut` 无脏标志；`set_startup_len` 为 `#[cfg(test)] pub(crate)`，`startup_slots_mut` 无调用点）；`:356-365`（`report()` 只断言 `!map_dirty`）。
- 文档 §3.4.5，第 1757 行：“`map_tables_mut`/`*_slots_mut` 会置 `map_dirty`……`report()` 在 `map_dirty` 时直接 panic，所以‘槽已写、长度未提交’的视图不能流到消费者。`set_modules_len`/`set_platform_len`/`set_param_len`/`set_memmap_sources_len`/`set_startup_len` 各自只允许提交一次，第二次调用 panic。”

**现象**：
1. `*_slots_mut` 是通配写法，但实际只有 `memmap_slots_mut`、`reserved_slots_mut`（以及两者的组合 `map_tables_mut`）置 `map_dirty`。`module_slots_mut`、`platform_slots_mut`、`param_buf_mut`、`memmap_source_slots_mut`、`startup_slots_mut` 全部不置脏；`report()` 对它们没有任何断言。生产者把槽写好后忘记 `set_modules_len`/`set_platform_len`/`set_param_len`/`set_memmap_sources_len`，报告会静默看到 0 条，而不是 panic。
2. `set_startup_len` 只在 `#[cfg(test)]` 下存在，生产路径根本不能调用；U-Boot/直核走 `push_startup` 逐条追加，`startup_committed` 在生产构建里永远为 false，`push_startup` 的“提交后 append 会 panic”断言在生产构建里是死代码。
3. `startup_slots_mut` 是公开 API 且零调用点；它允许“写满 20 槽、不提交”，与 `push_startup` 形成第二条无守卫路径。
4. platform 表有两条互不排斥的提交路径：`push_platform_source` 追加后 `platform_committed` 仍为 false，随后调用 `set_platform_len` 不会触发“已提交”断言，可以直接改写计数（可缩短到 0），而文档的“每条表只允许提交一次”只覆盖 `set_platform_len` 的第二次调用。

**影响**：
“槽已写、长度未提交”这一非法状态在 7 张表中的 5 张上仍可表示，而且消费者的观察是“空列表”而非 fail-fast。对 modules/platform/param/source 而言，损失会在内核启动后或接收侧 strict 门才暴露；对 startup 而言，UEFI 的 local-array 路径与 U-Boot 的 push 路径都不经过 `set_startup_len`，文档描述的提交保护实际上不存在。

**最小改法**：
- 给每张表加独立的 `*_dirty` 标志，所有返回 `&mut [T]`/`&mut [u8; N]` 的访问器置脏，`set_*_len`/`push_*` 清除；`report()` 对任一脏标志 panic（或返回 `Result`）。
- 或者用 RAII 填充守卫（访问器返回 `FillGuard<'_, T>`，`commit(len)` 消费守卫），使“未提交的槽”不能直接流到 `report()`。
- 把 list 表的唯一提交路径定为一种：要么 push，要么 bulk+set；删除 `startup_slots_mut` 与 `set_startup_len`（无调用点），或让它们走同一套脏/提交协议。
- 文档改为只声明代码实际保证的范围。

**严重度**：P2

**反证路径**：
1. 读 `boot_storage.rs:143-176`：只有三个 map 访问器写 `self.map_dirty = true`。
2. 读 `boot_storage.rs:194-207`、`:224-251`、`:272-313`：其余访问器不写任何脏标志。
3. 读 `boot_storage.rs:356-365`：`report()` 只 `assert!(!self.map_dirty)`，不检查其他表。
4. `grep -rn "startup_slots_mut\\|set_startup_len" os/`：`startup_slots_mut` 只有定义，`set_startup_len` 只有定义（且 `#[cfg(test)]`）。

---

## FINDING DESIGN-D2-41

**标题**：U-Boot shim 的固定 staging 槽对超限输入选择“截断后继续”，且没有打包期尺寸断言；文档把二次修剪说成幂等 no-op

**位置**：
- 代码：`os/boot-shim/src/uboot_main.rs:266-320`（`staged_elf_used_len`；解析失败返回 `None`）；`:347-352`（kernel 槽 2 MiB，`used > slot` 时打印 “truncating” 后继续）；`:387-390`（module 槽 1 MiB，同样打印后继续）；`:185-187`（`MODULE_FILE_STAGE_STRIDE = 0x10_0000`）。
- 文档 §3.2，第 1515 行：“U-Boot 侧的生产者在 `build_shim_table` 里按最后一个 PT_LOAD 的文件尾修剪槽长度，loader 的二次修剪是幂等 no-op”；§4.3 的 `UbootFileLoader` 注释（文档第 2594-2604 行附近）重复“按同一规则再修剪一次（幂等 no-op，兼容非 ELF 回退整槽）”。

**现象**：
1. `staged_elf_used_len` 在 ELF 头/程序头表读不到、`off + filesz` 溢出或结构不合法时返回 `None`；调用方 `.unwrap_or(slot)` 把整槽当作文件长度，且这条 fallback 不打印任何提示。
2. 当 `used > slot` 时，代码打印 “kernel/module ELF exceeds the … staging slot — truncating” 后继续，把 `used.min(slot)` 写进表；随后 loader 按这个长度拷贝被截断的字节，启动继续。
3. 模块 staging 地址按 1 MiB 步长排列（`0x8700_0000`、`0x8710_0000`…）。如果某个模块文件超过 1 MiB，U-Boot 的 `fatload` 在 shim 运行前就已经写过了下一个模块的 staging 区；shim 的“截断”只是事后处理，无法恢复被覆盖的下一槽。
4. 没有任何打包期/构建期断言保证 kernel ≤ 2 MiB、每个模块 ≤ 1 MiB；`uboot_main.rs` 被本轮删除的旧注释记录过 kernel ELF 约 1.2 MiB、12 个模块合计约 2.5 MiB（`git diff -- os/boot-shim/src/uboot_main.rs` 可见），余量存在但不是契约。
5. “loader 的二次修剪是幂等 no-op”只在 producer 成功按 PT_LOAD 修剪时成立；在 `unwrap_or(slot)` 的整槽回退里，loader 的第二次修剪会实际缩短长度，不是 no-op。代码注释在同一句里既说“幂等 no-op”又说“兼容非 ELF 回退整槽”，自相矛盾。

**影响**：
这是 U-Boot 腿唯一的“输入超限”路径，却与整条启动路径的 fail-closed 纪律相反：缺件、ELF 解析失败、内核跨度重叠都 halt，唯独“文件大于 staging 槽”静默截断并继续。后果是启动进入一个被截断的 kernel/模块，失败点被推迟到 `kmain` 之后或某个服务进程内；若超限的是模块，可能先污染下一模块的 staging 区，症状与“某个服务不响应”之间隔着整个启动流程。当前尺寸余量使它不是活故障，但这是可预期的构建增长风险（同一文件在 `bootface.rs:113-121` 已记录过一次因尺寸增长导致的静默损坏）。

**最小改法**：
- `build_shim_table`：`used > slot` 与“ELF 解析失败”都改为 halt（打印槽名、路径、used/slot）；不要用 `unwrap_or(slot)` 把解析失败伪装成合法长度。
- 在 image 打包（xtask 或 U-Boot 脚本生成）处断言 kernel ELF ≤ 2 MiB、每个模块 ≤ 1 MiB（或直接由 stride 推导），失败即构建失败。
- 文档 §3.2/§4.3 写明 1 MiB/2 MiB 是硬上限，并修正“幂等 no-op”的表述：producer 成功修剪时是 no-op，整槽回退时不是。

**严重度**：P2

**反证路径**：
1. 读 `uboot_main.rs:362-366` 与 `:388-395`：`used > slot` 分支只打印，不 `halt()`；随后写 `used.min(slot)`。
2. 读 `uboot_main.rs:288-319`：任何头/程序头读取失败都返回 `None`，调用方 `.unwrap_or(slot)`。
3. 读 `uboot_main.rs:185-187`：模块 stride 1 MiB；没有与文件大小相关的断言。
4. `grep -rn "1 MB module slot\\|2 MB kernel slot\\|MODULE_FILE_STAGE_STRIDE" os/xtask os/qemu-tests`：没有尺寸校验（脚本只按固定地址 fatload）。

---

## FINDING DESIGN-D2-42

**标题**：§3.4.1 的 `memmap` 行漏掉 aarch64 直核腿；该腿的 kernel/bump 页同样在自由表内，与同节下面三条 bullet 自相矛盾

**位置**：
- 文档 §3.4.1，第 1630 行：`memmap` 行写“内核镜像在**riscv64 直核自引导腿**与 U-Boot shim 腿的表内”，同段又写“**DTB 来源腿**（riscv64 直核与 U-Boot shim）的 bump 页在表内”。
- 同节第 1636-1637 行的三条 bullet 写“两个直核面：同一份 DTB 解析实现；aarch64 传 `fw_end = 0`”。
- 代码：`os/kernel-image/src/bootface_a64.rs:203`（`DtbMemoryMap::new(&fdt, 0)`）、`:76-95`（`KERN_PHYS_BASE`、`KERN_SIZE`、`BUMP_BASE`、`BUMP_SIZE`）。

**现象**：
aarch64 直核腿也是 DTB 来源腿：它用 `DtbMemoryMap::new(&fdt, 0)` 收集 `/memory`，`fw_end = 0` 只关闭固件前缀裁剪；kernel 镜像 `[0x4020_0000, +0x40_0000)` 与模块 bump 池 `[0x4400_0000, 0x4600_0000)` 都没有从自由表切除。它们与 riscv64 直核、U-Boot shim 一样，会在 `vm_handoff::classify` 里由 kernel 行与模块行做**实际扣除**，不是 UEFI 的 `LOADER_DATA` 空操作。

第 1630 行的两个括号都只列了 riscv64 直核与 U-Boot，漏掉 aarch64；而第 1636-1637 行的 bullet 明确把 aarch64 算作两个直核面之一。同一节内部不一致。

**影响**：
文档用来解释“为什么 UEFI 的 kernel 扣除行退化为空操作、哪些腿是实际扣除”的模型，对 aarch64 给出了错误归类。当前 `classify` 无条件扣除 kernel 行，所以不会因为该文档错误产生内存错误；但这段正是读者用来验证“扣除表是否完整”的依据，后续若有人按文档给 aarch64 加“no-op”短路，就会把内核镜像页放进自由清单。

**最小改法**：
把第 1630 行的两处“riscv64 直核”改为“两个直核腿”（或显式列出 riscv64/aarch64），与第 1636-1637 行的三条 bullet、以及 §3.7 的五条生产腿计数对齐。

**严重度**：P2（文档事实/设计模型错误；当前代码未受影响，但该模型是扣除表正确性的依据）

**反证路径**：
1. 读 `bootface_a64.rs:195-210`：`fw_end = 0` 的注释明确说“nothing is trimmed here”。
2. 读 `bootface_a64.rs:76-95`：kernel/bump 常量均落在 DTB `/memory` 的 DRAM 范围内。
3. 读 `kernel/src/vm_handoff.rs:675-700` 的 `for_each_live_bootstrap`：kernel 行与模块行无条件加入扣除记录。
4. 对照 §3.4.1 第 1636-1637 行：同节已把 aarch64 算作直核面。

---

## FINDING DESIGN-D2-43

**标题**：两个直核腿的模块落点没有“目标 bump 池 vs 内核镜像跨度”的断言；aarch64 的池常量与 kernel 常量之间也没有编译期关系

**位置**：
- 代码：`os/kernel-image/src/bootface_common.rs:177-275`（`place_modules`：只检查源区间在 memmap 内、源不与 bump 池相交；`dest = bump_alloc_pages(pages)` 后直接拷贝，不检查 dest 与 kernel 的关系）；`os/kernel-image/src/bootface_a64.rs:76-95`（`KERN_PHYS_BASE=0x4020_0000`、`KERN_SIZE=0x40_0000`、`BUMP_BASE=0x4400_0000`、`BUMP_SIZE=0x0200_0000`，无 `const _: () = assert!(...)` 连接两者）。
- 文档 §3.2 第 1517 行（`place_modules` 的检查清单）；§4.3 第 2489 行（A/B/C 防线清单）。

**现象**：
UEFI 腿有 `alloc_module_pages` 的拷贝前拒绝 + `assert_modules_clear_of_kernel` 的装载后复查；U-Boot 腿有 `KERNEL_MAX_LMA_END <= MODULE_REGION_BASE` 的编译期断言与装载前 layout gate。两个直核腿共用的 `place_modules` 只检查：
- 条目数、basename、长度非零；
- 源区间整段落在 memmap 的单个区间；
- 源区间不与 bump 池相交；
然后从调用者的 bump 分配器取目标页并 `copy_nonoverlapping`。它不接收 kernel 跨度，因此**目标页是否落在内核镜像上**没有检查。riscv64 的池（`opensbi_helpers` 的 `MODULE_REGION_BASE`）间接被 `KERNEL_MAX_LMA_END <= MODULE_REGION_BASE` 覆盖；aarch64 的池是 `bootface_a64` 自己的 `BUMP_BASE/BUMP_SIZE`，与 `KERN_PHYS_BASE/KERN_SIZE` 之间没有任何断言。

**影响**：
未来调整 aarch64 的 kernel 物理基址、镜像跨度或 bump 池常量时，模块页可能落在内核镜像上，`place_modules` 的拷贝会直接覆盖正在执行的代码/数据，且没有任何断言先失败。当前常量距离足够远（kernel 结束 `0x4060_0000`，池起点 `0x4400_0000`），但这是“靠数值运气”而不是契约；该路径与 UEFI/U-Boot 的显式模块-vs-内核防线不对称。

**最小改法**：
- 给 `PlacementPool` 增加 `kernel_start`/`kernel_end`（或增加一个 `kernel: PhysRange` 字段），在 `dest` 分配后、拷贝前断言 `[dest, dest+pages*4096)` 与 kernel 跨度不相交；两个直核腿都把各自常量传进去。
- 同时在 `bootface_a64.rs` 加编译期断言：`KERN_PHYS_BASE + KERN_SIZE <= BUMP_BASE || BUMP_BASE + BUMP_SIZE <= KERN_PHYS_BASE`；`bootface.rs` 可依赖 `opensbi_helpers` 的现有断言，但显式写一条更清楚。

**严重度**：P3（当前常量安全，属于未来改动/新腿复用的防线缺口）

**反证路径**：
1. 读 `bootface_common.rs:177-275`：`place_modules` 的形参只有 `table/memmap/pool/module_table/bump_alloc_pages/say/say_hex`，没有 kernel 跨度；检查清单里没有 dest-vs-kernel。
2. 读 `bootface_a64.rs:76-95`：四个常量各自定义，文件内没有把它们联系起来的 `const _` 断言。
3. `grep -n "assert!" os/kernel-image/src/bootface_a64.rs`：只有 `num_pages > 0` 与模块路径长度断言，没有池/内核关系断言。

---

## FINDING DESIGN-D2-44

**标题**：startup 分区的记录身份在生产/解码两侧都没有强制；strict 门只检查“有一条记录盖住内核”，任意 `SourceRecord` 都能冒充 startup 对象

**位置**：
- 代码：`os/libs/minix-boot/src/boot_storage.rs:305-313`（`push_startup(record: SourceRecord)` 接受任意记录）；`os/libs/minix-boot/src/handoff_v1_read.rs:370-405`（`decode_startup_objects` 对每条 72 B 记录调用通用 `WireSourceRecord::decode`，不检查 `kind`/`origin`）；`os/libs/minix-boot/src/handoff_v1.rs:171-261`（`decode` 接受 `KIND_MEMORY`、`KIND_FIRMWARE_PREFIX` 等全部标签）；`os/libs/minix-boot/src/kernel_info.rs:520-531`（strict 门只要求 `startup_objects` 中任意一条覆盖 kernel 跨度）。
- 文档 §3.4 第 1682 行（`startup_objects` 是“规范集合”，五类记录）、§3.4.1 第 1622 行（strict 门审计）。

**现象**：
`startup_objects` 的“规范集合”是靠生产者的调用约定维持的，类型与校验都没有表达它：
- 生产者侧：`push_startup` 接收任意 `SourceRecord`，不要求 `SourceKind::StartupObject`/`SourceOrigin::StartupObject`/`input_object == StartupObjects`；`startup_slots_mut` 更是直接暴露原始槽。
- 接收侧：`decode_startup_objects` 与 `decode_sources` 共用通用解码器，接受 Memory/FirmwarePrefix/MrbEntry/Reserved*/UefiExcluded 等所有 kind，只要 72 B 结构可解码。
- strict 门：只做区间覆盖检查（`r.range.start <= kern_lo && kern_hi <= r.range.end_exclusive`），不检查记录种类。

因此一条 `kind=Memory`、`origin=MemoryNode`、`input_object=DtbBootInput` 的记录，只要区间盖住 kernel，就能让 `startup_objects` 的规范集合检查通过；“这个分区里装的是 startup 对象”并没有被验证。

**影响**：
1. 生产者的 bug（例如把 memmap 记录误写进 startup 槽）或损坏/伪造的 wire 可以让 strict 门的 provenance 审计报告“规范集合已满足”，而实际没有收到 startup 生产者写的五类记录。
2. 该字段的语义从“启动对象集合”退化为“任意覆盖内核的记录”，后续消费者无法安全依赖 `SourceKind`。

**最小改法**：
- 生产者：新增 `BootStorage::push_startup_range(start, end)`，内部构造 `startup_record(next_index, start, end)`；把 `push_startup(SourceRecord)` 改为私有/测试专用；删除或同样约束 `startup_slots_mut`。
- 接收侧：在 `decode_startup_objects` 内要求 `record.kind == SourceKind::StartupObject`、`record.origin` 匹配 `SourceOrigin::StartupObject(_)`、`record.input_object == InputObjectId::StartupObjects`，否则返回 `BadSourceRecord`。
- 文档在 §3.4 明确“记录身份由类型/解码门强制”，而不只是调用约定。

**严重度**：P3（无内存安全后果，但 provenance 审计的唯一 startup 检查可被非 startup 记录满足）

**反证路径**：
1. 读 `boot_storage.rs:305-313`：`push_startup` 只检查容量与 `startup_committed`，不检查 `record.kind/origin`。
2. 读 `handoff_v1_read.rs:370-405`：`decode_startup_objects` 只做 `element_count` + 逐条 `wire.decode()`，没有 kind 过滤。
3. 读 `handoff_v1.rs:177-205`：`decode` 对 8 种 kind 都返回 `Some`。
4. 读 `kernel_info.rs:520-531`：strict 门只调用 `startup_objects.iter().any(|r| range covers kernel)`。

---

## FINDING DESIGN-D2-45

**标题**：UEFI 腿的 startup 规范集合由栈上局部数组而不是 `BootStorage` 持有，形成第二个所有者与第四份手写构建逻辑

**位置**：
- 代码：`os/boot-shim/src/uefi_helpers.rs:1140-1175`（局部 `[SourceRecord; WIRE_STARTUP_CAP]`、局部 `push_startup` 闭包、`serializer.write_startup(&startup[..startup_n])`）；`os/boot-shim/src/uefi_helpers.rs:1060`（第一次 `storage.report(...)`，此时 startup 槽为空）；`os/boot-shim/src/opensbi_helpers.rs:432-444`（U-Boot 的 `push_startup_record`）；`os/kernel-image/src/bootface.rs:367-403`、`bootface_a64.rs:340-375`（两个直核面的手写五类）。
- 文档 §3.4 第 1682 行（“四个写点”）、§3.4.5 第 1756/1758 行（UEFI startup 槽留空、U-Boot 写 storage）。

**现象**：
同一份 canonical 集合（kernel、每个模块、root、bump、可选 arena）有四份手写实现：
1. UEFI：局部栈数组 + 局部索引 + 局部容量断言，直接写 wire；
2. U-Boot：`push_startup_record` 辅助函数写 storage，再从 `report.startup_objects` 序列化；
3. riscv64 `bootface`：手写循环写 storage；
4. aarch64 `bootface_a64`：同形但独立一份。

UEFI 的实现导致 `BootReport.startup_objects` 在该腿上永远为空，报告不是完整视图；同时局部数组复制了容量断言与索引逻辑。所有五类地址（kernel/modules/root/bump/arena）在 `uefi_helpers.rs:1060` 构造第一次报告之前就已经全部可用，写 wire 也不依赖 EBS，因此“必须放在 EBS 之后”不是技术约束，而是当前借用顺序的产物（第一次 `report` 借用了 storage，之后再想 `push_startup` 需要可变借用）。

**影响**：
1. 两类所有者、四份构建逻辑：新增一类启动对象或调整容量时，必须同时改四处；UEFI 的局部实现容易与另外三处漂移（例如零长度跳过规则：UEFI/`push_startup_record` 跳过 len==0，两个 bootface 直接 push 不跳过）。
2. UEFI 的 `report.startup_objects` 为空，任何希望在发送侧、跳转前断言 canonical 集合的代码在这条腿上自动失效；只能依赖接收侧 strict 门，故障点被推到跳转之后。
3. 文档必须为 UEFI 保留一段“storage 槽留空”的例外说明，增加理解成本。

**最小改法**：
- 在 `uefi_helpers.rs` 中把五类记录的 `push_startup` 移到第一次 `storage.report(...)` 之前（`arena_pa` 在 `:1042` 已分配，其余值更早），然后与 envelope/modules/platform/param 一起 `serializer.write_startup(report.startup_objects)`；删除局部数组与局部容量断言。`reset_map_partitions` 明确不清 startup 分区，提前写入不会破坏重试语义。
- 抽一个共享函数（例如 `minix_boot::push_startup_set(storage, kernel_span, modules, root, bump, arena: Option<PhysRange>)`），四个生产者都调用它；零长度规则集中在一处。
- 文档删除“UEFI startup 槽留空”的例外，改为“五条腿都通过 storage 的 startup 槽；wire 从同一报告编码”。

**严重度**：P3（设计不对称/最小化机会；当前行为正确，但有两个所有者与四份易漂移的实现）

**反证路径**：
1. 读 `uefi_helpers.rs:1042-1072`：arena 分配与第一次 report 在 `:1060`；kern/modules/root/bump 更早已可用。
2. 读 `uefi_helpers.rs:1140-1175`：startup 只在 report 之后用局部数组构建，未写 `storage.startup_objects`。
3. 读 `opensbi_helpers.rs:432-444`、`bootface.rs:367-403`、`bootface_a64.rs:340-375`：另外三份手写实现。
4. 读 `handoff_v1_write.rs:417-435`：`reset_map_partitions` 明确不清 startup 分区。

---

## SUGGESTION

### S-1：`BootStorageCell::take` 的 `'static` 所有权没有类型化，两个 bootface 用裸指针制造 `'static` 切片

- 位置：`os/boot-shim/src/lib.rs:143-172`；`os/kernel-image/src/bootface.rs:404-425`；`os/kernel-image/src/bootface_a64.rs:376-396`；文档 §3.4.5 第 1759 行。
- 现状：`take(&self) -> &mut BootStorage` 的生命周期绑定在 `&self` 上，但 cell 是 `static`，调用方（bootface）在写完所有槽后用 `let _ = storage;` 丢弃引用，再用保存的裸指针 `from_raw_parts` 制造多个 `&'static [T]`。当前顺序是安全的（之后无写入），但编译器无法阻止“保留 `&mut` 后再写”的别名，文档所说的 one-shot 只覆盖重入，不覆盖这类别名。
- 建议：增加 `pub fn take_static(&'static self) -> &'static mut BootStorage`（安全的前提是 cell 在 static 中且 one-shot 已由 `taken` 保证），再提供 `BootStorage::freeze(&'static self) -> FrozenBoot<'static>` 之类的安全冻结视图，返回 `'static` 的 modules/platform/param/startup 切片；两个 bootface 删除三处 `unsafe from_raw_parts`，所有权边界由类型表达。

### S-2：aarch64 直核/ UEFI 的分道依赖 `x0 == 0` 的非规范 QEMU 观察；riscv64 用编译期 feature 做同一件事

- 位置：`os/kernel-image/src/main.rs:365-390`（aarch64 `rust_image_main` 的 `arena_pa == 0` 分道）；`main.rs:3035-3055`（riscv64 用 `fw-riscv64-uboot` feature 分成两个入口）；文档 §4.8 第 3018-3031、3077 行。
- 现状：aarch64 用“运行时哨兵 0”区分直核与 v1 接收；文档自己承认“QEMU 观察到不设 x0 参数——非规范保证，仅本仓 QEMU 虚机实测；真板 `-kernel` 行为以板级文档为准”。如果某个未来 aarch64 引导路径把 x0 留成 0，镜像会静默走直核面；如果留成非零垃圾，则会走 v1 解码并以 magic 失败停机。
- 建议：与 riscv64 对齐，用编译期 feature/独立入口符号区分两种交付形态；或至少要求 v1 跳转在 x0 之外再带一个 magic/tag（例如信封首字的副本），使“0”不再是唯一分道依据。若维持现状，把该非规范假设写进 §3.1.1 的接线状态表而不只是 §4.8。

### S-3：直核腿仍有两套所有者（`BootStorage` + DTB 收集器静态量），约 9 KiB 容量闲置

- 位置：`os/kernel-image/src/bootface.rs:213`/`bootface_a64.rs:203`（`DtbMemoryMap` 的 `memmap/reserved/sources` 来自收集器静态量）；`os/libs/minix-boot/src/boot_storage.rs:64-90`（storage 仍保留 memmap/reserved/source 槽）；文档 §3.4.5 第 1759 行已把该取舍说成“目标只完成了一半”。
- 建议：若后续要收口“一份存储、一套守卫”，把 DTB 收集器的 memmap/reserved/sources 也写入 `BootStorage`，删除 `DTB_FREE`/`DTB_RESERVED`/`DTB_SOURCES` 与第二套一次性守卫；在此之前，文档保持现状但把它标注为已登记的设计债，不要在其他节把它说成“单一所有者”。

---

## 统计与最严重三条

- 发现数：9
- 按严重度：P1 = 1，P2 = 5，P3 = 3
- 附加建议：3 条（S-1..S-3）

**最严重三条**：

1. **D2-37（P1）**：§4.4/§3.4.1 把“布局数字由 ELF 派生、镜像本身是唯一数据来源”说成五条腿的统一事实；两个直核腿实际使用 `bootface*.rs` 里三份手工常量，只靠 `check-layout.sh` L0/L9 构建期对账。源码注释记录了这类常量漂移曾导致高半只映射半个镜像、`kern_stack_top` 落进 `.bss` 的静默损坏，文档的叙述正好掩盖了这一已知失败模式。
2. **D2-38（P2）**：本轮新增的 `entry_point` 交叉校验只存在于 `load_kernel_with_loader_gated`，唯一生产调用点是 U-Boot 腿；两条 UEFI 产线仍然直接跳 `kern_virt_base`，不校验 `e_entry`。链接布局漂移会在开启分页后表现为取指异常，而文档完全没有记录这条断言及其覆盖范围。
3. **D2-39（P2）**：§4.6 与 `kernel/src/lib.rs:1260-1265` 把 `AlreadyMapped` 说成“重叠 2 MiB 窗口的正常结果”；三个架构的 `map_huge` 实际只在槽位已有 1 GiB 叶时返回该错误，重叠 2 MiB 叶返回 `Ok(())` 并覆盖。Step 1 的接受臂因此会把真实的 1 GiB 冲突当成成功，且文档的“重叠无害”论证建立在错误机制上。
