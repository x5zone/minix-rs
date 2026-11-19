# D02 R2 独立设计审查（P-DESIGN-D2）— `02-higher-half-kernel.md` §3–§4

基线核验：`wc -l` = 1280，`sha256sum` 前缀 `0888ffb0bbb27b4f`，与任务基线一致。
读取范围：目标文档 §3（326–649）与 §4（651–1093）；`tools/doc-code-map.sh <doc> --check` 的 28 个文件（存在性 28/28）；`trait-audit.md`；`AGENTS.md` Key Constraints；`round1-decisions-snippet.md`。
未读：作者解释、本 run 顶部 V1 段、R1 轨迹、其他面板输出、`expected/`、`check-layout.sh`、`os/.cargo/config.toml`、各 `Cargo.toml`、`minix3/` C 源。凡结论依赖这些允许集外材料，均标“未复核”，不以推断充证据。
唯一执行的动作：在 `/tmp/d2probe` 建一次性探针 crate 复现 ELF 解析 panic（release 与 debug 各一次），未改仓库任何文件。

## DIFF-TABLE

### 七维差异表

| # | 维度 | 文档陈述（§） | 代码事实 | 判定 |
|---|---|---|---|---|
| 1 | 链接布局与地址不变量 | §3.1：三形态（生产 `AT()`／`os/kernel/src/arch/*/link.ld` 无读者／载体单地址）；“谁读 `p_paddr` 谁才需要 LMA”；两种 `AT` 写法同一不变量 | 生产三脚本 `: AT(` 各 4 条；arch 三脚本各 0 条（`grep -c ': AT('` 复算）。`build.rs:19-27` 按 target 三选一；全仓无 `src/arch/.*link.ld` 的链接引用（只有注释与 lockstep 测试）。`KERN_VIRT_BASE`/`KERN_PHYS_BASE` 常量与生产一致 | 一致；第三形态是死脚本，存在部分漂移（F5） |
| 2 | ELF 解析与错误模型 | §3.2：九变体；字段/偏移表；迭代器“任一步失败都 `continue`”；字段去向表“列全” | 九变体、偏移、`from_le_bytes` 全部对上；`compute_kernel_layout` 的四个 `InvalidSegment` 点与测试名对上。防御性范围检查有两处未检查加法；字段去向表只覆盖两个内核侧消费者 | 有差异（F1、F3）；`entry_point` 行有偏差（F2） |
| 3 | 栈与高半跳转契约 | §3.3：`kern_stack_top = kern_virt_base + kern_size`；64 KiB `kernel_boot_stack_*`；§3.4：trait + Safety 四条 + `-> !`；§4.3：六条 x86 汇编与对齐算术 | `build_kernel_info`（`boot-shim/src/lib.rs:125`）与 `BootStorage::report`（`minix-boot/src/boot_storage.rs:378`）两处同式；三份生产 `.ld` 都有 64 KiB 栈与 2 MiB 收口；trait 签名与 3 份实现一致；跳转调用点只有 `kernel/src/lib.rs:232`、`:375` | 一致；`stack_top` 形参冗余（已登记 D2-04）；x86 终止槽是 C 形状残留（F6） |
| 4 | 映射粒度与收口 | §3.5：1 GiB 首选→2 MiB 回退→错误；`NoKernelPageSize` 消息；2 MiB `ALIGN` 同时决定三量；自引导 `KERN_SIZE=0x40_0000` | `boot_policy.rs:98-140` 阶梯逐行对上；`lib.rs:947` panic 文案对上；三架构 `HUGE_PAGE_SIZE=GIB_1`、`FALLBACK=MIB_2`；`bootface.rs:121` 与 `bootface_a64.rs:83` 均为 `0x40_0000`；生产 `.ld` 收口语句在 `.bss` 内 | 一致；L4/L5/L9 的宿主输出不在允许集，未复核 |
| 5 | 启动路径与状态机 | §3.1：v1 send/receive 与 direct 面分工；§4.4：四条路径、`arch_boot_impl` Step 1–4、状态表 | `uefi_helpers.rs:953/1216`、`uboot_main.rs:445/503`、`bootface.rs:427-442`、`lib.rs:214-400/645-830/925-980` 与文本路径一致；Step 1/2/2b/2c/3/4 注释在 `lib.rs:1221/1280/1308/1354` | 一致；§4.4 节选遗漏 strict gate（F4）；状态表证据列指向测试专用函数（F9） |
| 6 | 所有权、生命周期与 `.bss` 落盘 | §3.3：栈顶按值交接；§4.4：两条生产路径跳转前都把 `KernelInfo` 落 `.bss`，arena 解码是最后一次读 | `store_kernel_info`（`lib.rs:1453-1565`）复制 7 类 slice 到 `.bss`；`arch_boot_finish` 与 `resume_additive` 都在跳转前调用它；`arch_boot_resume_v1` 的七个 decode 全部落 store；`validate_requiring_sources` 在 v1 与两个自引导面执行 | 一致（F4 是节选呈现问题，不是实现问题） |
| 7 | 证据与可测试性 | §3.2 测试名；§4.1 L1/L3b/L6/L8；§4.4 临时打印；状态表引 `kmain_verify` | loader/minix-elf 测试名逐一存在；`HIGHER_HALF_CALLED` 只写不读；`kmain_verify` 是 `#[cfg(feature="qemu_test")]`；临时打印在 `lib.rs:252-257/1689-1710` | 部分；F7、F9；`check-layout.sh` 的 L 门未复核 |

### 抽查证据清单（只列会影响判定的条目）

| 文档行 | 陈述 | 代码/命令证据 | 结论 |
|---|---|---|---|
| 356-357 | arch `link.ld` 无生产链接读者，只有常量锁步测试 | `grep -rn 'src/arch/.*link.ld' --include='*.rs' --include='*.toml' --include='*.sh' --include='Makefile' os/` 只命中注释与 `link_ld_lockstep.rs`；`link_ld_lockstep.rs:49-72` 只比每架构两个基址常量（3 架构） | 一致；锁定面窄（F5） |
| 394 | `entry_point` 由装载方“决定从哪里进入镜像” | `loader.rs:394-400` 注释明说“jump target is `kern_virt_base`, not `entry_point`”，随后 assert 两者相等；`lib.rs:510` `entry = report.kern_virt_base.0` | 偏差（F2） |
| 419-440 | 结构体字段与来源 | `minix-elf/src/lib.rs:88-153` 字段逐个对上 | 一致；注释被改写（F8） |
| 444-452 | `parse_ehdr`/`parse_phdr` 偏移表 | `lib.rs:185-296` 偏移 16/18/24/32/54/56 与 0/4/8/16/24/32/40/48 全对上 | 一致 |
| 454/496 | 迭代器防御性解析，任一步失败 `continue` | `lib.rs:333-337` 与 `:250-254` 用未检查 `+`；探针 release 在 `:254` panic、debug 在 `:333` panic | 偏差（F1） |
| 500-511 | 字段去向表“列全”，随后说“四种去向” | 表只有“内核镜像”与 `load_elf_into` 两列；`vm_server.rs:998-1008` 读 `seg.flags`（PF_W/PF_X），`trap_api.rs:443-469` 读 `offset/filesz/vaddr/entry` | 偏差（F3） |
| 533-545 | 64 KiB 引导栈 + 收口填充 + 栈顶公式 | 三份生产 `.ld` 都有 `0x10000` 与 `kernel_boot_stack_*`；`compute_kernel_layout` 用 `paddr+memsz` 极值；`boot_validate_and_prepare` 有 16 字节对齐断言（`lib.rs:878-881`） | 一致 |
| 562-572 | trait Safety 四条；`stack_top` 冗余 | `boot/higher_half.rs:40-48` 四条；两个调用点都传 `kinfo.kern_stack_top` | 一致；冗余已登记 D2-04 |
| 589 | x86 入口 RSP = stack_top − 24，`sp & 0xF == 8` | `x86_64/higher_half.rs:44-55`；`kmain_verify` 的 `sp_aligned`（`lib.rs:2145-2155`） | 算术一致 |
| 618-621 | 1 GiB 首选、2 MiB 回退、错误分档 | `arch/paging_ext.rs:80-83`；`arch/{x86_64,arm64,riscv64}/paging.rs` 的 `HUGE_PAGE_SIZE`/`FALLBACK` | 一致 |
| 623-637 | `.bss` 末尾 2 MiB 收口决定三量 | 三份生产 `.ld` 的 `ALIGN(0x200000)` 都在 `.bss` 花括号内；`riscv64.ld:124`/`aarch64.ld:116` 有 `ASSERT`，x86 无 | 一致 |
| 655-670 | `build.rs` 三选一；宿主目标 `None` | `kernel-image/build.rs:19-27` 逐行对上 | 一致 |
| 696-697 | `_start` 在 `.text.boot`，锚点静态 + KEEP | `main.rs:112-114/136-172`；三份 `.ld` 的 `KEEP` | 一致 |
| 726-729 | `.data`/`.bss` 通配与四个符号 | 三份 `.ld` 对上；727 写“三个符号”，729 写“这四个符号” | 文档内部计数 slip（见下） |
| 743 | 三份脚本五处差异 | 逐项实读对上（`.riscv.attributes`、`OUTPUT_ARCH`、`ASSERT`、`.ap_early_entry/.ltext`、基址） | 一致 |
| 761-789 | `parse_ehdr` 五道校验次序 | `lib.rs:185-209` 逐字对上 | 一致 |
| 797-834 | `compute_kernel_layout` 函数体 | `loader.rs:175-213` 逐行对上（含 `found_load`、`checked_add/sub`） | 一致 |
| 843-868 | `load_segments_into_phys_memory` 主干与两处重叠检测 | `loader.rs:286-320`、`:321-372`；panic 与 `InvalidSegment` 的不对称对上 | 一致 |
| 878-899 | x86 asm 六条与 C 四行对照 | `x86_64/higher_half.rs:44-55` 逐条对上 | 一致 |
| 904-927 | aarch64 asm / `isb` 防御性 | `aarch64/higher_half.rs:40-55` 逐条对上 | 一致 |
| 932-975 | riscv delta、`auipc/bltu`、注释读数复算 | `riscv64/higher_half.rs:92-121`；`0xFFFFFFC00000477A + 0xFFFFFFBF7FE00000 = 0xFFFFFF7F7FE0477A` 复算成立 | 一致（文档正确指出来源注释多一个 `f`） |
| 1005-1023 | 四条路径的装载/建根分工 | `main.rs` 四个 `rust_image_main` 分支；`lib.rs` 的 `arch_boot_finish`/`resume_additive`；`bootface*` 调用 | 一致 |
| 1064 | 两条生产路径跳转前都落 `.bss` | `lib.rs:220-232`（direct）、`:365-375`（v1） | 一致 |
| 1070-1076 | 状态表五项 | 分页/保护/中断顺序与代码对上；栈行证据指向 `kmain_verify`（qemu_test-only） | 部分（F9） |

### 文档内部的小 slip（不单独立 FINDING）

| 位置 | 现象 | 处置 |
|---|---|---|
| §4.1 行 727 vs 729 | 先说“三个符号”，随后列四个名字并写“上面这四个符号” | 编辑改为“两对（四个）符号”或“四个符号” |
| §3.3 行 519 | “`uefi_helpers.rs` 与 `opensbi_helpers.rs` 只 `use crate::build_kernel_info`”在非测试代码中不成立；这两条生产腿走 `BootStorage::report()`，import 只出现在两文件的 `#[cfg(test)]` 模块 | 措辞改为“两文件的测试模块 import；生产 v1 腿走 report() 的同式” |
| §4.4 行 1011 | 路径 A 写“落 `.bss`、记录活动根”，代码顺序是 `set_current_root_phys` → `store_kernel_info` → `establish_boot_dm` | 顺序不承载语义，可不改；若改则按代码顺序 |

### 冻结决策落地核对（round1-decisions-snippet）

| 决策 | 核对结果 | 证据 |
|---|---|---|
| D-032 `BootWindows::push` 反序/零长 fail-fast | 成立 | `lib.rs:1085-1095` `assert!(start < end)`；空区间 sentinel 不暴露 |
| D-033 物理上限逐窗推导 | 成立 | `lib.rs:1199-1210` 对每个 `PhysRange` 断言 `end <= phys_addr_limit` |
| D-040 `CheckedWindows` token 保留、检查与建根唯一调用序 | 成立 | `lib.rs:1194-1210` 私有 tuple struct；`build_bootstrap_root_and_enable` 只接受该类型（`:1214-1218`） |
| D-042 `BootWindows.ranges` 用 `PhysRange` | 成立 | `lib.rs:1100` `[PhysRange; MAX_BOOT_WINDOWS]` |
| D-045 节选只许省略不许改写 | **需裁决**：§3.2 两处明写“注释有删改”并确实改写注释；§4.2 的“注释有删改”实为删除 | F8 |
| D-001/D-024/D-046/D-047 | 遵守：未把四章骨架当优化对象；未以 ARCH 提案作硬阻断；未实施最小化 | 本报告结构 |

### 翻译残留六类

| # | 类别 | 实例与证据 | 判定 |
|---|---|---|---|
| 1 | C 链接符号/常量残留 | 三份 arch `link.ld` 的 `PROVIDE(k_initial_stktop = .)`（16 KiB 栈）与 `kern_size` 符号；生产侧改用 `kernel_boot_stack_*` + `KERN_SIZE`/ELF 实测。`k_initial_stktop` 无 Rust 读者 | 死符号（F5） |
| 2 | C 机制直译 | x86 trampoline 的 `push 0`（C `Terminate stack`）+ `sub rsp, 8`；文档自述它“不在帧指针链上” | 直译残留（F6） |
| 3 | C 双段式布局 | `objcopy --prefix-symbols` + `unpaged_*.o` 已淘汰，Rust 侧无 unpaged 段（§4.5 与代码一致） | 无残留（通过） |
| 4 | C 数据/口径残留 | `KernelInfo` 的 `#[repr(C)]` 与 C 行号注释是刻意锚点，不是残留；`Elf64Phdr.p_align`/`LoadSegment.align` 无读者（文档自述） | 可接受；`align` 为可选清理项 |
| 5 | 平行实现残留 | `build_kernel_info` 与 `BootStorage::report` 各写一遍 `kern_stack_top = kern_virt_base + kern_size`；`load_segments_into_phys_memory` 与 `_into_buffer` 是双实现（文档已承认 D02-F04） | 公式重复（ARCH 提案覆盖）；孪生已登记 |
| 6 | 死载体/死测试残留 | 三份 arch `link.ld` + 只锁两个常量的 `link_ld_lockstep.rs`；`MockHigherHalf` 的 `HIGHER_HALF_CALLED` 只写不读；x86 两块 `temporary` 取证打印（已登记 D2-13） | F5、F7、D2-13 |

### 死抽象 / 死代码

| 对象 | 事实 | 判定 |
|---|---|---|
| `trait HigherHalf` | trait-audit：4 impl、31 引用、MULTI-IMPL；生产 3 份 + 测试 1 份；两个调用点 | 不是死抽象；但 `stack_top` 形参冗余（D2-04） |
| `MockHigherHalf` / `HIGHER_HALF_CALLED` | 静态只被 `store(true)`，全仓无读者；测试只做 `accept_higher_half::<MockHigherHalf>()` 编译期约束，永不调用 `jump_to_kmain` | 死测试代码（F7） |
| 三份 `os/kernel/src/arch/*/link.ld` | 无生产链接读者；带 16 KiB `k_initial_stktop` 与不含收口的 `kern_size`，与生产栈/跨距设计不一致；lockstep 只锁两个基址 | 死载体 + 部分漂移（F5） |
| `LoadSegment.align` / `Elf64Phdr.p_align` | 无读者（文档自述；28 文件内 grep 无 `seg.align`） | 可选删除 `LoadSegment.align`；结构体字段保留可辩护 |
| `KernelLoadResult.entry_point` | 唯一用途是 `assert_eq!(entry_point, kern_virt_base)`；跳转目标是 `kern_virt_base` | 近死字段：保留为交叉校验，但文档语义要改（F2） |
| `KernelInfo.syscall_entry` | 生产无功能读者；唯一真实读者 `test-protection-riscv64`；strict 档断言等于 `kern_virt_base`；v2 计划删槽（D2-03） | 已登记的参考槽，不是新问题 |
| x86 `push 0` 终止槽 | 内核自身 unwinder 从 FP 链起点走，`push 0` 不在链上；`xor rbp` 已使寄存器为 0 | 冗余指令（F6） |

### 非法状态

| # | 非法状态 | 是否可表达 | 现状与文档 | 判定 |
|---|---|---|---|---|
| I1 | `jump_to_kmain` 的 `stack_top` ≠ `kinfo.kern_stack_top` | 可表达 | 两个生产调用点都传字段；类型不拦；文档自述 D2-04 | 已承认；ARCH 提案可消 |
| I2 | 第二次调用 `jump_to_kmain` | 可表达 | Safety 第四条靠调用者；`-> !` 只拦返回 | 已承认，无类型解 |
| I3 | 畸形 ELF 的 `e_phoff` 绕过表范围检查 | 可表达 | `new()`/`parse_phdr` 未检查加法；探针 panic | 未登记（F1） |
| I4 | 直接调用 `load_segments_into_phys_memory` 且 `filesz > memsz` 或地址溢出 | 可表达 | 函数自身不校验；文档已承认 D02-F05，生产由 `compute_kernel_layout` 先跑 | 已承认 |
| I5 | 多段 ELF 的 `vaddr-paddr` 平移不一致 | 可表达 | `compute_kernel_layout` 独立取 `min(vaddr)`/`min(paddr)`；只靠宿主 L4 检查，装载器本身不 fail-closed | 设计缺口（建议，非 FINDING） |
| I6 | 迭代器跳过坏段导致布局偏小 | 可表达 | 文档已承认 D2-05 | 已承认 |
| I7 | 源/目标重叠走 panic 而非错误 | 可表达 | 文档已承认 D2-06 | 已承认 |

### 最小化机会（本轮不实施，⑥）

| # | 机会 | 收益 | 裁决 |
|---|---|---|---|
| M1 | x86 `push 0; sub rsp, 8` → `sub rsp, 16` | 少一条指令、少一次栈写；入口 RSP 仍 `stack_top-24` | 需裁决（Rewrite 是否要求保留 C 终止槽）：F6 |
| M2 | `HigherHalf::jump_to_kmain(kinfo, stack_top)` → `jump_to_kmain(kinfo)` | 消掉 I1 非法状态与两处重复取值 | 接受（D2-04 延期；ARCH 提案） |
| M3 | 删除三份 arch `link.ld` + `link_ld_lockstep.rs`，或把 lockstep 扩到全部布局量 | 去掉无读者载体与“假同步”测试 | 需裁决（教学载体 vs 死代码）：F5 |
| M4 | 删除 `LoadSegment.align`（保留 `Elf64Phdr.p_align` 供结构完整性） | 少一个无读者字段 | 接受为 P3 建议，非本轮 |
| M5 | 用单一 `BootLayout` 构造器替代 `build_kernel_info` / `BootStorage::report` 两处公式 | 公式与派生字段单点；strict 等值断言退化为构造不变式 | ARCH 提案 |
| M6 | 删除 `MockHigherHalf` 的 `HIGHER_HALF_CALLED` 静态（保留 impl 做编译期约束） | 去掉只写不读的原子与死分支 | 接受（F7） |
| M7 | 删除 x86 两块 `temporary` 取证打印或加 feature 门 | 生产串口噪声归零 | 已登记 D2-13，接受延期 |
| M8 | 装载器增加平移一致性检查（所有段 `vaddr-paddr` 同一常量） | 把 I5 从宿主 L4 提升为装载期 fail-closed | 接受为 D2-05 邻项，非本轮 |

## FINDING RD2-DESIGN-D2-01（P2）[MODERN]

- location：目标文档 §3.2 行 454 与行 496；`os/libs/minix-elf/src/lib.rs:333-337`（`SegmentIter::new` 的表范围算式）、`:250-254`（`parse_phdr` 的越界算式与切片索引）、`:355-372`（`next` 的 checked 计算）。
- severity：P2（设计陈述不符 + 失败语义缺口；P0-code-bug 候选）。
- class：防御性解析的陈述与实现不符；畸形输入把“跳过/返回 Err”变成 panic。
- problem：文档说迭代器“做防御性表项计算、防御性解析、类型过滤与范围检查；任一步失败都 `continue`”，并把“`checked_mul`/`checked_add` 溢出、表项越界或太小时跳过”列为第一类语义。实际 `SegmentIter::new` 用 `ehdr.e_phoff as usize + (e_phnum as usize) * (e_phentsize as usize)`（未检查加法），`parse_phdr` 用 `offset + (entsize as usize) > image.len()`（同样未检查）。构造 `e_phoff = u64::MAX - 32`、`e_phnum = 1`、`e_phentsize = 56`、`image.len() = 64`：release 下 `new()` 的加法回绕到 23 ≤ 64 而通过；`next()` 的 `checked_add(e_phoff, 0)` 返回 `Some(huge)`；`parse_phdr` 的 `offset + 56` 回绕到 23 而通过；随后 `&image[offset..]` 在 `lib.rs:254` panic。debug 下 `new()` 的加法在 `lib.rs:333` 直接 panic。两种构建都以 panic 结束，既不是 `Err(PhdrOutOfBounds)`，也不是“跳过该段”。
- impact：损坏或被篡改的 `kernel.elf` 会让 boot-shim 在解析阶段停机（panic），而不是走可分类的错误路径；§3.2 的“防御性/任一步失败都 continue”与 D2-05 的“引导路径应 fail-closed”在这一点上都不成立（panic 不可分类，错误现场丢失）。影响面是启动期 DoS，不是内存破坏。
- evidence：在 `/tmp/d2probe`（独立目录、未改仓库）用 path 依赖 `minix-elf` 复现。release：`panicked at os/libs/minix-elf/src/lib.rs:254: range start index 18446744073709551583 out of range for slice of length 64`；debug：`panicked at os/libs/minix-elf/src/lib.rs:333: attempt to add with overflow`。两次都由 `catch_unwind` 捕获为 `Err`，证明 panic 逃出了 `segment_iter`。
- minimal fix：`SegmentIter::new` 里先 `let table_len = (e_phnum as usize).checked_mul(e_phentsize as usize).ok_or(ElfError::PhdrOutOfBounds)?;`，再 `let phdr_end = (e_phoff as usize).checked_add(table_len).ok_or(ElfError::PhdrOutOfBounds)?;`；`parse_phdr` 把 `offset + entsize > image.len()` 改成 `let Some(end) = offset.checked_add(entsize as usize) else { return Err(ElfError::PhdrOutOfBounds); }; if end > image.len() { return Err(ElfError::PhdrOutOfBounds); }`。两处改完，构造输入走 `Err`，文档的“跳过”语义只需保留在 `next()` 的类型/范围过滤上。
- confidence：高（两种构建均实测复现，代码路径可逐步复算）。
- source check：代码实读 + 运行时探针；文档行 454/496 实读。未依赖允许集外材料。
- if wrong：若项目认定“畸形 ELF 启动期 panic 也是可接受的 fail-stop”，则本条的严重度降为 P3，但 §3.2 的“任一步失败都 continue / 不是报错”仍必须改写，不能保留为设计陈述。
- 裁决：接受（P2）。本轮只读不实施；建议并入 D2-05 的 fail-closed 整改一起做。

## FINDING RD2-DESIGN-D2-02（P3）[MODERN]

- location：目标文档 §3.1 行 394 的映射表行 `ENTRY(_start)` → `entry_point` → “装载方决定从哪里进入镜像”。
- severity：P3（SUGGESTION，默认不阻断）。
- class：设计陈述与实现不符（字段语义归属）。
- problem：`KernelLoadResult.entry_point` 在装载路径中不是“决定进入地址”的量。`loader.rs:394-400` 的注释写明“The jump target is `kern_virt_base` (the lowest PT_LOAD VMA), not `entry_point`”，随后用 `assert_eq!(layout.entry_point, layout.kern_virt_base.0)` 做交叉校验；`prepare_kernel_image_v1` 把 `entry = report.kern_virt_base.0`（`kernel/src/lib.rs:510`）交给 `jump_kernel_image_v1`（`:530`）；`uefi_helpers.rs:977-981` 重复同一断言。启动链（boot-shim / kernel / kernel-image）没有把 `entry_point` 当作跳转目标的读者（`grep -rn entry_point` 只命中 loader/uefi 的定义、断言与测试）。
- impact：读者会以为 `entry_point` 与 `kern_virt_base` 是两个可独立变化的量；实际不变量是“两者必须相等”，跳转只认 `kern_virt_base`。若有人为“修正入口”只改 `e_entry`，会在装载期断言失败，而不是按新入口跳转。
- evidence：`os/boot-shim/src/loader.rs:394-400`；`os/boot-shim/src/uefi_helpers.rs:977-981`；`os/kernel/src/lib.rs:510`、`:530`。
- minimal fix：把该行改成“装载方交叉校验 `entry_point == kern_virt_base`；跳转目标是 `kern_virt_base`；`entry_point` 只记录 ELF 头声明的第一条指令地址，无独立选择权”。
- confidence：高（代码与注释直接反驳原表述）。
- source check：代码实读 + 映射目录内 grep；未依赖允许集外材料。
- if wrong：若允许集外还有一条腿把 `entry_point` 当跳转目标，那一腿可另论；本仓 v1 生产路径仍是 `kern_virt_base`。
- 裁决：接受（P3 SUGGESTION，文档修正）。

## FINDING RD2-DESIGN-D2-03（P3）[MODERN]

- location：目标文档 §3.2 行 500-511（`LoadSegment` 字段去向表与“四种去向”句）。
- severity：P3（SUGGESTION）。
- class：文档覆盖度不足（自称“列全”但只覆盖两个消费者）。
- problem：行 500 说“这张表把每个字段的去向列全”，表却只有两列：装载内核镜像、`load_elf_into`。同节前文列了四个调用点；代码里 `vm_server` 的 bootstrap 装载读 `seg.flags` 决定 `VrFlags::WRITABLE/EXECUTABLE`（`vm_server.rs:998-1008`），`trap_api.rs` 的 exec 路径读 `seg.offset/filesz/vaddr` 与 `entry_point`（`:443-469`），vfs 的 `exec_worker` 用自己的 `ElfHead` 读 `p_type/p_memsz` 等（`:219-242`、`:1024`）。行 511 的“同一份解析结果、四种去向”与两列表格不对应。
- impact：读者无法从表里看到 `flags`、`vaddr`、`memsz` 在用户态装载器里的去向；修改 `LoadSegment` 字段时容易漏掉 `vm_server`/`trap_api` 两个消费者。文档与代码同步面不完整。
- evidence：`os/servers/vm/src/vm_server.rs:890-892`（`entry_point` + `segment_iter`）、`:998-1008`（`seg.flags & PF_W/PF_X`）；`os/servers/rs/src/trap_api.rs:443-469`（`seg.offset/filesz/vaddr` + `entry_point`）；`os/servers/vfs/src/exec_worker.rs:219-242`、`:1024`（自有 `ElfHead`，只用 `minix_elf::ET_EXEC/PT_LOAD` 常量）。
- minimal fix：二选一——(a) 表格加“rs exec”“VM bootstrap”两列，至少补 `flags`/`vaddr`/`offset+filesz`/`memsz` 的去向；(b) 把句子限定为“内核侧两个装载场景”，并加一句列出用户态两个消费者各自的字段面。行 511 的“四种去向”改为与表格一致的说法。
- confidence：高（消费者代码实读）。
- source check：代码实读 + grep；未依赖允许集外材料。
- if wrong：若“四种去向”指的是四个字段而非四个调用点，句子本身仍然歧义，最小修正不变。
- 裁决：接受（P3 SUGGESTION）。

## FINDING RD2-DESIGN-D2-04（P3）[MODERN]

- location：目标文档 §4.4 行 1040-1058（`fn bootface` 节选及其省略号说明，尤其是行 1041 的“省略号处为 DTB 解析、模块表、模块装载”）。
- severity：P3（SUGGESTION）。
- class：节选完整性 / 文档-代码同步（遗漏承载语义的调用）。
- problem：节选保留 `build_kernel_info(...)` 与 `minix_kernel::arch_boot(&kernel_info, root_page)`，中间用 `...` 连接；行 1041 把省略内容限定为“DTB 解析、模块表、模块装载”。实际的 `bootface.rs` 在这两者之间还有 `kernel_info.validate_requiring_sources();`（`:440`），`bootface_a64.rs` 同形（`:411`）。这是自引导面的 strict gate：它检查来源记录分区非空、模块名按位置匹配、`kern_stack_top == kern_virt_base + kern_size`、`syscall_entry == kern_virt_base` 等。节选与说明都没有提到它。
- impact：读者会得到“build → say → arch_boot”的路径印象，看不到自引导面在进入内核前已经跑过严格校验；日后改这条路径时容易把 gate 当冗余删掉。v1 接收侧在 `arch_boot_resume_v1` 里也调用同一个 strict gate（`kernel/src/lib.rs:738`），文档同样没有在 §4.4 点出这一对称点。
- evidence：`os/kernel-image/src/bootface.rs:427-442`；`os/kernel-image/src/bootface_a64.rs:398-413`；`os/libs/minix-boot/src/kernel_info.rs:264-266`（`validate_requiring_sources` 的语义）。
- minimal fix：把 `kernel_info.validate_requiring_sources();` 一行加进节选（最省事），或把省略号说明改为“省略号处为 DTB 解析、模块表、模块装载与 strict gate”，并在正文补一句“两个自引导面在 `arch_boot` 前都跑 `validate_requiring_sources`，与 v1 接收侧对称”。
- confidence：高（代码与节选逐行比对）。
- source check：代码实读；未依赖允许集外材料。
- if wrong：若作者把“KernelInfo 组装”理解为包含校验，行 1041 的省略清单仍然不完整，最小修正（补一行或补一句）不变。
- 裁决：接受（P3 SUGGESTION，文档修正）。

## FINDING RD2-DESIGN-D2-05（P3）[DEL]

- location：目标文档 §3.1 行 356-357、行 373（登记表最后一行）、§3.3 行 533-545；文件 `os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld`、`os/kernel/tests/link_ld_lockstep.rs`。
- severity：P3（SUGGESTION；死载体/最小化）。
- class：死载体 + 部分布局漂移 + “假同步”测试。
- problem：三份 arch `link.ld` 没有生产链接读者（文档自述，grep 复算只命中注释与 lockstep 测试）。它们仍声明 `KERN_VIRT_BASE`/`KERN_PHYS_BASE`（被测试锁定）、`kern_size = . - KERN_VIRT_BASE`（在 `.bss` 之后、16 KiB 栈之前，不含 2 MiB 收口）以及 `k_initial_stktop`（`. += 0x4000`，16 KiB）。生产脚本的对应物是 `.bss` 内 64 KiB `kernel_boot_stack_bottom/top`、`. = ALIGN(0x200000)`、以及由 ELF 实测得到的 `kern_size`/`kern_stack_top`。`link_ld_lockstep.rs:49-72` 只比每架构两个基址常量，不比 `kern_size`、栈大小、收口或段形状；因此“记录的是预期布局”只在两个基址上成立，其余是 C 时代的旧布局。
- impact：三份死脚本构成第二套、局部与生产矛盾的布局描述；lockstep 测试给出“已同步”的错觉。`k_initial_stktop` 正是 §3.3 明确说被 `KernelInfo.kern_stack_top` 取代的 C 符号，却仍以 16 KiB 形态留在仓里。维护者改布局时可能先改这里（无效），或据此推断栈只有 16 KiB。
- evidence：`os/kernel/src/arch/x86_64/link.ld:43-53`、`aarch64/link.ld:37-45`、`riscv64/link.ld:47-55`；生产 `os/kernel-image/x86_64.ld:86-98`、`aarch64.ld:85-98`、`riscv64.ld:93-105`；`os/kernel/tests/link_ld_lockstep.rs:43-73`；文档 356-357。
- minimal fix：二选一并写进文档——(a) 删除三份脚本与 `link_ld_lockstep.rs`，§3.1 的“第三种形态”改成“历史脚本（已删除）”或直接删掉该行；(b) 保留为教学载体，但把 lockstep 扩到全部布局量（`kern_size`、`k_initial_stktop`、栈大小、2 MiB 收口），并把 16 KiB 栈改成与生产同形的 64 KiB 注释性预留，同时在文件头写明“not linked; production source is `os/kernel-image/*.ld`”。
- confidence：高（文件内容与测试范围实读）。
- source check：代码实读 + `grep -c ': AT('` 与全仓引用复算；未依赖允许集外材料。
- if wrong：若项目明确把这些脚本定位为“历史快照，不承诺与生产同步”，则应去掉 lockstep 测试的“防漂移”暗示，并加历史标注；本条的删除建议可降为 P3 可选。
- 裁决：需裁决（保留为教学载体 vs 删除；无论哪种，先消除“只锁两个常量就叫锁步”的假同步）。

## FINDING RD2-DESIGN-D2-06（P3）[DEL]

- location：目标文档 §4.3 行 902（`push 0` 终止槽“不在帧指针链上”）与 §3.4 行 589（对齐算术）；代码 `os/kernel/src/arch/x86_64/higher_half.rs:44-55`。
- severity：P3（SUGGESTION；C 机制直译残留 / 最小化）。
- class：翻译残留（C `Terminate stack` 槽）+ 多余指令。
- problem：x86 trampoline 的顺序是 `mov rsp, stack_top; and rsp,-16; xor rbp,rbp; push 0; sub rsp,8; call kmain`。文档说 `push 0` 是 C 的 `Terminate stack` 槽，但“不在帧指针链上”；`sub rsp,8` 是 psABI 垫片。两者合起来只是在调用前保留 16 字节并对齐；零字位于返回地址之上，内核自身的 unwinder 从 `fp` 寄存器链走，链终止于 `fp == 0` 或 `return_addr == 0`（`stacktrace.rs:87-120`），不会读 `[stack_top-8]`。aarch64/riscv 都没有这个槽。因此 `push 0; sub rsp,8` 与单条 `sub rsp,16` 的入口状态等价（入口 RSP 仍是 `stack_top-24`，仍是 8 mod 16）。
- impact：一条多余指令 + 一次无读者的栈写；§3.4 的对齐算术与 §4.3 的“六条对四行”都要为这个 C 形状多解释一段。功能上无影响。
- evidence：`os/kernel/src/arch/x86_64/higher_half.rs:44-55`；`os/arch/src/arch/stacktrace.rs:87-120`（从 fp 走，链终止条件不读终止槽）；文档 589/902。
- minimal fix：把 `push 0` 与 `sub rsp, 8` 换成 `sub rsp, 16`，同步改 §3.4 的算式（`sub rsp,16` 后 RSP 为 `stack_top-16`，`call` 压栈后入口 `stack_top-24`）与 §4.3 的指令表。保留 `xor rbp`。
- confidence：高（内核自身 unwinder 不读该槽）；对“外部调试器是否依赖该哨兵”只有中等置信。
- source check：代码实读；未执行真机/调试器实验（允许集外）。
- if wrong：若调试器、取证脚本或未来 unwinder 依赖 `[stack_top-8] == 0` 作为哨兵，或 Rewrite 口径要求保留 C 初始栈形状，则保留并关闭本条（把 §4.3 的“不在帧指针链上”改成“保留给外部观察者”）。
- 裁决：需裁决（Rewrite 是否把初始栈终止槽算作外部行为）。

## FINDING RD2-DESIGN-D2-07（P3）[DEL]

- location：`os/kernel/src/lib.rs:6069-6100`（`MockHigherHalf` 与 `HIGHER_HALF_CALLED`）；`os/kernel/src/boot/higher_half.rs:21-30`（trait 文档的 “Testability” 理由）。
- severity：P3（SUGGESTION；死测试代码）。
- class：死抽象支撑 / 只写不读的测试状态。
- problem：`MockHigherHalf::jump_to_kmain` 里 `HIGHER_HALF_CALLED.store(true, SeqCst)`，全仓没有读这个静态的地方；唯一的测试 `test_higher_half_trait_is_implementable` 只调用 `accept_higher_half::<MockHigherHalf>()`，这是编译期 bound 检查，`jump_to_kmain` 的函数体从不执行（它进入无限循环）。trait 文档的 “Testability” 说 “Mock implementations can verify the signature and the call contract; the `-> !` return means sequencing itself is not observable at runtime”——后半句正好否定了前半句：`-> !` 方法无法返回，调用序列无法在测试里观察。
- impact：测试代码里有一个只写不读的原子和一个不会执行的分支；trait 选择理由中的 “Testability” 一条在代码里没有对应证据。功能无影响，但它是“死抽象”审查的直接样本。
- evidence：`lib.rs:6069-6100`；`grep -rn HIGHER_HALF_CALLED os/kernel` 只有定义与一次 store，无 load；`higher_half.rs:28-30`。
- minimal fix：删除 `HIGHER_HALF_CALLED` 与那次 store，保留 `loop {}` 的 mock 和编译期 bound 测试；把 trait 文档的 Testability 改为 “编译期可验证实现存在；运行期调用序列不可观测（`-> !`）”。或者把 mock 换成不写入任何全局的 `loop {}`。
- confidence：高（grep 直接证明无读者）。
- source check：代码实读 + grep；未依赖允许集外材料。
- if wrong：若未来测试计划从另一线程观察该静态，现在应加注释说明用途；当前是无读者的死状态。
- 裁决：接受（P3，清理）。

## FINDING RD2-DESIGN-D2-08（P1 条件）[MODERN]

- location：目标文档 §3.2 行 419（结构体节选头：“注释有删改——源文件用 `///` 文档注释，此处压缩为行内注释”）与行 457（迭代器节选头：“注释有删改”）；源 `os/libs/minix-elf/src/lib.rs:89-100`、`:369-372`；冻结决策 D-045（`round1-decisions-snippet.md` 行 9：“节选引文只许省略不许改写”）。
- severity：P1（条件性；仅当 D-045 的“节选引文”覆盖注释时成立）。
- class：冻结决策符合性 / 引文改写。
- problem：两处节选不是只省略，而是改写了注释。例：源 `/// e_type — object file type (must be ET_EXEC)` 变成 `// 必须是 ET_EXEC(2)`；源 `/// e_phentsize — program header table entry size` 变成 `// 每个 Phdr 的大小`；源 `// parse_phdr already validated bounds in new(), but be defensive` 变成 `// 防御性解析：表项越界/太小时跳过`。文档自己用“注释有删改”披露了这一点。按 D-045 的字面（只许省略不许改写），这是改写；若 D-045 的“引文”只指代码行、注释视为编辑性说明，则不构成违规，但当前措辞（“注释有删改”）会被读成“引文已改写”。
- impact：若违规成立，它是冻结决策层面的流程问题，不是 P3 设计建议；同时节选失去逐行对源的能力（review 的锚点纪律要求引文可回查）。若不违规，也应在节选头明确“代码逐字、注释为转述”，避免读者把转述当原文。
- evidence：文档 419/457；源 `lib.rs:89-100`、`:369-372`；D-045 行 9。
- minimal fix：两种合规路径——(a) 注释恢复逐字（可以整段省略，不许改写），代码行保持逐字；(b) 节选头改成“代码逐字引用；注释为转述，非引文”，并由冻结账行裁决 D-045 不覆盖注释。
- confidence：改写事实高置信；D-045 是否覆盖注释中等置信（决策只有一行，未定义“引文”边界）。
- source check：文档与源逐行对比；未运行 certify/`--stale-lines`（工具不在允许集，且本条是文本比对）。
- if wrong：若 D-045 明确排除注释，本条关闭为“无违规”，保留披露措辞即可。
- 裁决：需裁决（把 D-045 的“引文”边界写进决策账行；在裁决前不要以“已符合 D-045”认证）。

## FINDING RD2-DESIGN-D2-09（P3）[MODERN]

- location：目标文档 §4.4 行 1073（状态表“栈指针”行的“依据”列：`fn kmain_verify` 的三条断言）。
- severity：P3（SUGGESTION）。
- class：证据引用不精确（用测试专用函数为生产状态背书）。
- problem：`kmain_verify` 只在 `#[cfg(feature = "qemu_test")]` 下编译（`kernel/src/lib.rs:2043-2110`）；生产 `kmain` 是另一个 `#[cfg(all(not(feature="mock"), not(feature="qemu_test")))]` 的函数（`:1673`），不会执行 `kmain_verify` 的三条断言（`sp_ok`、`sp_aligned`、`fp_zero`）。状态表描述的是“跳转完成时系统处在什么状态”（生产路径也在内），却把依据指到一个只存在于 qemu_test 载体的验证函数。
- impact：读者可能以为生产 `kmain` 在入口检查 SP/FP；实际检查只在测试载体执行。状态值本身仍由三份 asm 保证（`mov rsp`/`and`/`xor rbp` 等），所以这不是状态错误，而是证据指针错误。
- evidence：`lib.rs:2043-2110`（`kmain_verify` 的 cfg 与三条断言）、`:1673`（生产 `kmain` 的 cfg）；文档 1073。
- minimal fix：把该格改成“由 §4.3 的三份 asm 保证；`kmain_verify` 的三条断言只在 `qemu_test` 载体执行（展开见 §5.2）”。一句话即可。
- confidence：高（cfg 与函数边界实读）。
- source check：代码实读；未依赖允许集外材料。
- if wrong：若后文（§5.2 或测试要点）已经明确限定 `kmain_verify` 只覆盖 qemu_test 载体，则本条降为“交叉引用精确化”，可不改。
- 裁决：接受（P3，措辞精确化）。

## ARCH-PROPOSAL

### A1：把启动布局收成一个值对象，并把跳转改成 typestate 契约（与 HANDOFF v2 对齐）

**问题（不是 FINDING，是架构级机会）**：同一组布局事实目前有五个承载面——生产 `.ld` 常量、`KernelLoadResult`、`KernelInfo` 的派生标量、`BootStorage::report` 的派生标量、`bootface*.rs` 的常量副本。派生公式 `kern_stack_top = kern_virt_base + kern_size` 在 `boot_shim::build_kernel_info`（`boot-shim/src/lib.rs:125`）与 `minix_boot::BootStorage::report`（`minix-boot/src/boot_storage.rs:378`）各写一遍；接收侧再用 `validate_requiring_sources` 的等值断言把两份公式拉回一致（`minix-boot/src/kernel_info.rs:458-470`）。`HigherHalf::jump_to_kmain(kinfo, stack_top)` 又允许把 `kinfo.kern_stack_top` 再传一遍，形成 D2-04 的非法状态面。三份 arch `link.ld` 是这套重复的第六个影子（F5）。

**提案**：
1. 在 `minix-boot` 定义唯一的 `BootLayout` 值对象：`{ virt_base, phys_base, size, stack_top, entry }`，构造器只有两个——`from_elf_segments`（v1：检查 `filesz <= memsz`、地址算术、所有段 `vaddr - paddr` 同一常量）与 `from_constants`（direct：常量 + 编译期断言）。`stack_top`/`entry` 在构造时派生并检查，不再由调用方各写一遍。
2. `KernelInfo`/`BootReport` 内嵌或派生自 `BootLayout`；`build_kernel_info` 与 `BootStorage::report` 变成同一个构造器的两个薄适配层（或只留一个），重复公式消失。
3. `HigherHalf::jump_to_kmain(&KernelInfo) -> !`（去掉 `stack_top` 形参）；新增 `ValidatedForJump` token，只由 `validate + establish_boot_dm` 的成功路径产出，`jump_to_kmain` 只接受该 token。这样 D2-04 的 I1、D2-05 的“跳过坏段”与“先校验后建根再跳”的调用序都变成类型可表达的约束，而不是文档纪律。
4. `entry_point` 从“入口候选”降为构造期交叉校验（等于 `kern_virt_base`），与 F2 的措辞修正一致；`syscall_entry` 参考槽按 D2-03 在 v2 删除。
5. 三份 arch `link.ld` 与 `link_ld_lockstep.rs` 随 F5 的裁决删除或改为真正的全量锁步。

**边界与代价**：这会改线协议（v1→v2）与 `KernelInfo` 的构造面，属于 Architectural Evolution；按 AGENTS.md 必须在文档、设计、代码三处标注 `[ARCH: boot-layout-v2]`，并按 D-047 取得判决账行后才可实施。它不是本轮阻断项；R2 只记录方向。若只想要最小版本，可先做 3（去掉 `stack_top` 形参）+ 2（合并公式），不动线协议；这时它是 Refactor，不是 ARCH。

**不提案的部分**：不改 `KernelInfo` 的 `#[repr(C)]`（它不是 ABI，保留可调试性）；不改 `HigherHalf` 的 `-> !`；不改 direct/v1 两条建根路径的分工（§4.4 已论证）。

## VERDICT

**设计轴判定：PASS-WITH-FINDINGS；不按 CONVERGED 收口。**

理由与计数：
- 在允许集内逐条核对 §3/§4 的可执行陈述，核心设计（三形态链接布局、`KernelInfo` 按值交接、`HigherHalf` 三实现、页尺寸阶梯、v1/direct 分工、落 `.bss` 生命周期）与代码一致；冻结决策 D-032/033/040/042 在代码中成立。
- 1 条 P2（RD2-DESIGN-D2-01：ELF 表范围检查的未检查加法导致畸形输入 panic，直接反驳 §3.2 的“防御性/任一步失败都 continue”）。
- 7 条 P3：F2（`entry_point` 语义）、F3（字段表覆盖度）、F4（bootface 节选漏 strict gate）、F5（三份死 link 脚本 + 假同步）、F6（x86 `push 0` 残留）、F7（MockHigherHalf 死状态）、F9（`kmain_verify` 证据限定）。
- 1 条条件性 P1 流程项：F8（D-045 的“节选引文”是否覆盖注释）。在判决账行给出边界前，不应以“D-045 已满足”认证。
- 未发现新的 P0-design 级矛盾；D2-04/D2-05/D2-06/D02-F04/D02-F05/D2-13 仍是已登记缺口，不重复立项。

**对 R2 确认轮的建议**：
1. F1 要么本轮修（两处 `checked_add`），要么明确并入 D2-05 的 fail-closed 整改，并把 §3.2 的“任一步失败都 continue”改成与代码一致的分层描述。
2. F8 先裁决 D-045 的“引文”边界；裁决前 F8 保持 NEEDS-ADJUDICATION。
3. F5/F6 是“保留 C 形状 vs Rust 最小化”的口径选择，建议在冻结账里各给一行，避免下一轮反复。
4. 第六项（最小化）本轮按纪律未实施：M1–M8 只记录，ARCH-PROPOSAL 不作为硬阻断。

**验证方式与局限**：本报告是同 agent 只读核对——代码实读、文档实读、文档自带的复算命令（`grep -c ': AT('`、`grep -rl 'minix-elf ='`、全仓 `src/arch/.*link.ld` 检索）加一次 `/tmp/d2probe` 探针。另有少数同仓源只做定点 grep 或片段读取（`boot_storage.rs`、`paging_ext.rs`、`arch/*/paging.rs`、`qemu-tests` 载体的 `main.rs`、`dm_coverage.rs`），用于核对常量与调用点；它们不在 28 文件清单内，未作为设计结论的唯一依据，也未读任何禁止项。没有跨工具复核；`check-layout.sh` 的 L0–L9、QEMU 装载行为、`os/.cargo/config.toml`、各 `Cargo.toml`、`minix3/` C 源不在允许集内，相关陈述标“未复核”，不能把本报告的“一致”当作对 L 门的背书。若 F1 的探针被判为超出设计轴范围，可降为 P3，但 §3.2 的措辞仍需修。

