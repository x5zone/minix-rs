# 01-stage 01 R1 缺陷台账（V3.12，post-DESIGN-02）

- edition: V3.12
- 轮次: R1 full
- 基线: doc sha `996ff161…`（2,946 行），commit `cdd0d1c77`
- 归并口径：按根因合并面板 FINDING；P-REFUTE 的 CLAIM 单列 `## P-REFUTE 附加`。
- 面板 FINDING 36：P1 3 / P2 18 / P3 15；另有 REFUTE 2 个 P1 阻断与 4 个 P2 观察、certify 1 个 P0-code-bug（QEMU 载体 E0061）。

| 发现 ID | 单元 | 处置类 | 处置 | 复验 | 根因 ID | origin |
|---|---|---|---|---|---|---|
| READER-C-01、FACT-01、DESIGN-01 | 3.2, 4.0, 4.4 | ① | §4.0/§4.4 改为“接收侧统一经 land_and_store：直核面 store_boot_report，v1 面 store_kernel_info”；§4.4 的 source_records 行按 UEFI 直写 wire 改写；§3.2 改为“KernelInfo 去掉 authority 的九字段视图”；§3.7 的 fill_* 计数改为 4 次调用/5 类落点。 | rg -n '接收侧统一走 `store_boot_report`|五条腿的发送侧都填 `BootReport`' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 0）；rg -n '统一经 `land_and_store`|四个 `fill_\*` 调用' 同文件（期望 ≥1） | R1-01 | R1 |
| READER-C-02 | 3.6, 3.7, 4.4.1, 4.5, 4.8 | ① | §3.7 增加“解码落盘”与“store_kernel_info 包装调用”两个事件的定义；把“store 与 boot_validate 次序相反”限定为包装调用。 | rg -n '两个容易同名的事件|包装调用次序相反' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 ≥1） | R1-02 | R1 |
| DESIGN-02 | 4.2 | ① | §4.2 的 BootStorage 位置改为发送入口自己的栈帧（三处生产调用都是 `let mut storage`）。 | rg -n '住在发送入口自己的栈帧' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1）；rg -n '静态区（`.data`' 同文件（期望 0） | R1-03 | R1 |
| DESIGN-03 | 3.2, 4.4 | ① | 文档与代码注释中的 `KernelInfo<'static>` 改为 `KernelInfo`；首次出现处说明切片字段是 `&'static [T]`。 | rg -n "KernelInfo<'static>" os rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md --glob '*.rs' --glob '*.md'（期望 0） | R1-04 | R1 |
| DESIGN-04 | 4.4.1 | ① | 非 strict 的 `validate_view` 改收 `Option<RamAuthority>`；`BootReport::validate_before_tables()` 去参；`arch_boot_finish` 同步。 | rg -n 'validate_before_tables\(authority\)' os rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 0）；cargo test -p minix-boot --lib（188/0） | R1-05 | R1 |
| DESIGN-05 | 4.4.1 | ① | 删除零调用点的 `KernelInfo::validate_before_tables` 与 `BootReport::validate`；保留 `BootReport::validate_before_tables()` 与两个 strict 入口。 | rg -n 'fn validate_before_tables|fn validate\(' os/libs/minix-boot/src/kernel_info.rs（只保留 BootReport 的 pre-table 入口与 KernelInfo/BootReport 的 strict 入口） | R1-06 | R1 |
| DESIGN-06 | 4.5 | ① | 删除 `bootface_common.rs` 中无读者的 `param_ptr` 初始化与赋值。 | rg -n 'param_ptr' os/kernel-image/src/bootface_common.rs（期望 0） | R1-07 | R1 |
| DESIGN-07 | 4.4 | ① | 删除 uefi_helpers/opensbi_helpers 的 `test_kernel_info` 辅助函数，测试直接断言 `BootReport` 字段。 | rg -n 'test_kernel_info\(' os/boot-shim/src（期望 0，kerninfo.rs 的同名测试 helper 除外）；cargo test -p boot-shim --lib（136/0） | R1-08 | R1 |
| DESIGN-08 | 3.2.3 | FP | 保留 `PlatformDescSource` 的 `#[repr(C)]` 与 16 B 断言：它是文档化的句柄形状锁，当前无消费者可见差异；D2 自己给出“删除或保留都合理”，本轮按现状保留，不作为缺陷。 | rg -n 'repr\(C\)' os/libs/minix-boot/src/platform.rs（保留）；文档 §3.2.3 的“例外”句与现状一致 | R1-09 | R1 |
| DESIGN-09 | 3.2.2.2 | ① | §3.2.2.2 把 `GET_KMESSAGES`/`VmBootHandoff` 标为 Rust 新增通道，不再归到 C 侧。 | rg -n '新增的 `GET_KMESSAGES`/`VmBootHandoff`' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-10 | R1 |
| READER-B-01 | 3.2.2.2 | ① | §3.2.2.2 的“七个语义字段”改为“七个字段（五个有读者 + 两个 C 布局位）”。 | rg -n '只含七个字段（五个有读者' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-11 | R1 |
| READER-B-02、READER-A-05 | 1.2, 3.7, 4.4 | ① | §1.2 I3 行补 UEFI source 例外；§4.4 source_records 行改为 UEFI 直写 wire；§3.7 明确“走 fill_* 的腿”才适用第二次 fill panic。 | rg -n 'UEFI 腿五类归 `BootStorage`，source 分区直写 wire' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 ≥1） | R1-12 | R1 |
| READER-B-03 | 4.5 | ① | §4.5 把第二个“守卫”命名为 `land_slice` 的指针相等检查，区分 one-shot 守卫。 | rg -n '`land_slice` 的指针相等检查' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-13 | R1 |
| READER-B-04 | 2.3, 4.7 | ① | 删除“PM 篇/SMP 篇（尚未生成）”悬空指针，改为本篇可核对锚点。 | rg -n '尚未生成' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 0） | R1-14 | R1 |
| READER-A-01、READER-A-02 | 1.0, 1.1 | ① | §1.0 术语补 `memmap`/`reserved_regions`；§1.1 行内区分可分配/不可分配；小结与失败对照加“定义见本节术语”。 | rg -n '\*\*`memmap`\*\*：可分配候选区间表' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-15 | R1 |
| READER-A-03 | 3.0, 3.2, 3.6 | ① | `arena` 定义改为“三条跨镜像 v1 腿”；§3.2 的“三形态一致”改为“三种形态（含已退役同镜像测试形态）在深拷要求上一致”。 | rg -n '三形态 v1 交接' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 0） | R1-16 | R1 |
| READER-A-04 | 3.7, 4.6 | ① | §3.7 开头补“发布/消费”的局部定义，并说明 VM 自由页的发布是另一件事。 | rg -n '本节的“发布”指把落地后的 `KernelInfo` 写进' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-17 | R1 |
| READER-A-06 | 3.2.1, 3.3 | ① | §3.3 的 fail-closed 限定为生产腿 strict 门；补非 strict 只在 source_records 非空时复核。 | rg -n '在生产腿的 strict 门里' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-18 | R1 |
| READER-A-07 | 4.4, 4.4.1 | ① | §4.4 的 pre-table 括注按腿分开：直核面两道，v1 只有 postconditions。 | rg -n '直核面前置 pre-table 与 postconditions' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-19 | R1 |
| READER-A-08 | 3.4.2 | ① | §3.4.2 的注册计数改为三次 PT_ALLOC 注册 + 一次 VM PT_FREE 归还钩子注册。 | rg -n '三次 `PT_ALLOC` 注册与一次 VM `PT_FREE`' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-20 | R1 |
| READER-A-09 | 3.2.3 | ① | §3.2.3 的版本转移列表补 7→8，与前句口径一致。 | rg -n '本篇出现的"7→8"、"8→9"与"9→10"' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-21 | R1 |
| READER-A-10 | 4.5 | ① | §4.5 的“实现侧增量见本节末”改为指向 §4.7 的 cut_memmap 行。 | rg -n '实现侧增量见 §4.7 的 `cut_memmap` 行' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-22 | R1 |
| READER-A-11 | 4.7, 7.5 | ① | §4.7/§7.5 的 Rust 列把 `alloc_pagetable` 改为 `alloc_pt_page`。 | rg -n '引导期页表页见 `alloc_pagetable`' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 0） | R1-23 | R1 |
| READER-A-12 | 2.4.2.5 | ① | §2.4.2.5 的“见 §2.4.2 的还…行”改为 §2.4.2.5（三处）。 | rg -n '见 §2.4.2 的(”|“)?还' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 0） | R1-24 | R1 |
| READER-A-13 | 3.4 | ① | §3.4 标题扩为“bootstrap 区间、分配器换手与页目录槽位”。 | rg -n '### 3.4 bootstrap 区间、分配器换手与页目录槽位' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-25 | R1 |
| READER-A-14 | 3.2.2 | ① | §3.2.2 开头补四个读者任务的分组说明与可跳过边界。 | rg -n '本节按四个读者任务分组' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-26 | R1 |
| READER-A-15 | 2.4.2.3 | ① | §2.4.2.3 末段压缩为回指第二级结论与 2.4.2.1，不再重复四个限定。 | rg -n '取页方向与跳板生命周期为什么只算观测常态' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-27 | R1 |
| READER-A-16 | 2.1 | ① | §2.1 multiboot 行补 exec.c:736 的 `snprintf(cmdline, len, "%s %s", file, args)` 锚点。 | rg -n 'snprintf\(cmdline, len, "%s %s", file, args\)' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-28 | R1 |
| READER-C-03 | 1.3, 3.5.1 | ① | §1.3 的 FileLoader 句加限定：仅 UEFI 与 U-Boot 两族，两个直核面走 bootface_common。 | rg -n '仅 UEFI 与 U-Boot 两族' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-29 | R1 |
| READER-C-04 | 3.2 | ① | §3.2 的“解码之后再消费”改为“落盘之后再消费”，并区分 v1 解码落盘与直核借用落盘。 | rg -n '落盘之后再消费' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 ≥1） | R1-30 | R1 |
| READER-C-05 | 3.7 | ① | §3.7 的 UEFI fill 计数改为四个 fill_* 调用、五类落点；source 只在 U-Boot shim 腿使用。 | rg -n '四个 `fill_\*` 调用' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 ≥1） | R1-31 | R1 |
| READER-C-06 | 3.3 | ① | §3.3 的 kind 枚举后补 `kind = Memory` 指 `/memory` 这一类，UEFI conventional 描述符按同一 kind 归类。 | rg -n '审计里的 `kind = Memory` 指 `/memory`' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-32 | R1 |
| REFUTE-01 | 1.2 | ① | §1.2 的 I1 限定为“发送侧虚拟地址指针/进程局部句柄不可”，并补一句物理地址是值、可跨镜像。 | rg -n '物理地址是值，可以跨镜像' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 1） | R1-33 | R1 |
| REFUTE-04 | 4.4.1 | ① | §3.7/§4.4.1 的 kmain 链补第二次 `check_boot_postconditions`；§4.4.1 的 QEMU 空模块豁免从 kmain 行移回 validate_before_tables。 | rg -n '`kmain` 的 `validate` → `check_boot_postconditions` 复核' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 ≥1） | R1-34 | R1 |
| REFUTE-06 | 2.1, 2.6 | ① | §2.1/§2.6 的 HAS_MODS 绝对句改为“正常分配下”；补模块表分配失败时提前返回、停在 HAS_MODS 断言。 | rg -n '正常分配下反序时' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md（期望 ≥1） | R1-35 | R1 |
| CERTIFY-01 | 4.4 | ① | 修复 DESIGN-02(b) 引入的三个 QEMU 载体 E0061：test-higher-half、-aarch64、-riscv64 改为构造 `BootReport` 并调用三参数 `arch_boot`。 | cargo build -p test-higher-half --target x86_64-unknown-uefi --features fw-x86-uefi（exit 0）；cargo build -p test-higher-half-aarch64 --target aarch64-unknown-uefi --features fw-aarch64-uefi（exit 0）；cargo build -p test-higher-half-riscv64 --target riscv64gc-unknown-none-elf --features fw-riscv64-none（exit 0）；三支 QEMU 门 PASS（evidence/y01b-qemu-*.log） | R1-36 | DESIGN-02 verify |

## P-REFUTE 附加

| claim ID | 结果 | 处置 | 根因 |
|---|---|---|---|
| REFUTE-01（CLAIM 01） | 非阻断 P2：I1 句柄措辞与 `PlatformDescSource` 物理地址冲突 | ① 限定为发送侧虚拟地址/进程局部句柄；物理地址是值 | R1-33 |
| REFUTE-02（CLAIM 02） | 阻断 P1：接收侧统一 `store_boot_report` 不成立 | ① 两入口按腿分述，统一到 `land_and_store` | R1-01 |
| REFUTE-03（CLAIM 03） | 阻断 P1：`BootStorage` 在 `.data` 静态区不成立 | ① 改为发送入口栈帧；fill 计数改 4 次调用 | R1-03 |
| REFUTE-04（CLAIM 04） | 反证失败（门序成立）；附 P2：kmain 第二次 postconditions 与 QEMU 豁免挂错门 | ① 补链与改行 | R1-34 |
| REFUTE-05（CLAIM 05） | 反证失败（无 unpaged 段，字段删除成立） | FP；不改文档 | — |
| REFUTE-06（CLAIM 06） | 非阻断 P2：HAS_MODS 绝对句漏 `module_init` 分配失败分支 | ① 加“正常分配下”限定与失败分支 | R1-35 |

## 建议（不进入主表）

| 建议 ID | 来源 | 处置 |
|---|---|---|
| READER-A-S01 | READER-A | FP：单元格偏长但不构成理解阻断，保留表格形态 |
| READER-A-S02 | READER-A | 修：§2.4.2.3 末段已压缩为回指；第二级结论保留完整表述 |
| READER-A-S03 | READER-A | 修：§3.5.1 已补角色一/角色二 |
| READER-A-S04 | READER-A | FP：UEFI DTB source 记录的消费者需源码取证，本轮不臆造用途；已在 §3.7 保留“strict 按 authority 过滤”的事实 |
| READER-A-S05 | READER-A | 修：§2.2/§4.1 自指改为本节 |
| READER-B-S01 | READER-B | 部分修：§1.3 重复 bullet 改回指；其余重复处保留权威入口，未逐条重写 |
| READER-B-S02 | READER-B | 修：§1.0 术语补 memmap/reserved，并在小结/失败对照加指针 |
| READER-B-S03 | READER-B | 修：§1.0 补直核面/落地区术语 |
| READER-B-S04 | READER-B | 部分修：§3.4 标题与 §4 章首已改；§2.4.2.5/§4.3 的结构拆分留待 R2 |
| READER-B-S05 | READER-B | 修：§5.5 标题改为“测试缺口与关闭记录” |
| READER-B-S06 | READER-B | FP：结论前置是导读设计，四个前提紧随其后；未造成事实误读 |
| READER-B-S07 | READER-B | 修：§4.6 加“无归档日志”；§3.4.2 断言清单标“按源码核对” |
| READER-B-S08 | READER-B | 修：mmap_size 改为“当前遍历边界，切除不回缩” |
| READER-B-S09 | READER-B | 修：§4.5 表前补删除记录；§5.1 补 I3 三档；§3.2.2 补 22/16 计数区别 |
| READER-B-S10 | READER-B | 修：§2.2/§4.1 自指改为本节 |
| READER-B-S11 | READER-B | 修：删除“本节标题里的 GRUB” |
| READER-B-S12 | READER-B | 修：§2.4.1.1 标题层级、§3.2.2.4 缩进、§4.5 空行 |
| READER-B-S13 | READER-B | 部分修：BootShim 段改当前规则；历史事故/合并加“无归档日志”；其余历史叙事留待 R2 |
| READER-C-S01 | READER-C | FP：160/128 的 32 B 差额需要 v9 信封逐字段取证；本轮不臆造字段，留待历史取证 |
| READER-C-S02 | READER-C | 修：BootStorage 位置改为发送入口栈帧 |
| READER-C-S03 | READER-C | 修：EBS/DM 首次出现补全称；C=1 改为 CONFIG_MAX_CPUS |
| READER-C-S04 | READER-C | 修：§1.0 补直核面术语，统一直核腿/面 |
| READER-C-S05 | READER-C | 修：§3.2.1 自指改本节表；wire 标签补定义 |
| READER-C-S06 | READER-C | 修：历史事故/合并加“无归档日志” |
| READER-C-S07 | READER-C | 修：四前提改为“核对过的事实（调用序 + 地址/尺寸关系）” |
| P-FACT SUGGESTION | P-FACT | FP（文档侧）：15 个 C/asm/ld 引文块已人工逐行核对一致；工具多行标注识别缺口记 WF-09，留待工具修复 |
