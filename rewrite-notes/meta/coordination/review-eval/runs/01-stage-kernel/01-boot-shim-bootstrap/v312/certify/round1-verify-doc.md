# 01-stage 01 R1 文档修复独立复验（V3.12，round1）

- 复验日期：2026-11-23
- 复验范围：`R1-01`–`R1-23`、`R1-25`–`R1-36`，共 35 个根因。
- **R1-24 不在本报告范围**：台账 `未决/需主 agent 裁决` 第 1 条已把它标为 `BLOCKED`（设计批级改动，`review-publication.md` §四.6），本轮不实施，按其台账要求既不得记 `FP`/`②`，也不得宣告闭环。本报告不对它下任何结论。
- 复验方式：逐行重跑台账 `复验` 列的 `rg`/`sed` 命令；对命令里的引号、反引号、正则转义造成的字面失配，用安全等价命令重跑并在表内注明；随后读对应文档小节并核源码/C 源，判语义而不是只判字符串消失。
- 未读取历史面板报告、其他轮次报告或 `panel/`、`unit-r1/` 目录内容。
- **复检更新（2026-11-23）**：两处修补后 R1-14 已 PASS，整体判定由 FAIL 改为 PASS；复检证据见 §5。§3 保留初判记录，§4 的弱证据与未处理建议未在复检中重审，仍然有效。

## 1. 基线

| 项 | 值 |
|---|---|
| HEAD | `7a356f781c8ad049c6d31a05a709bb699df66250` |
| 文档 | `/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` |
| 文档 sha256 | `b1a9bd6603c3f274a42e23ef06f38c0e767ce02c844768b78e7e755712a15623` |
| 行数 | `2979` |

## 2. 逐根复验

| 根因 ID | 状态 | 复验命令 | 实际输出摘要 | 语义复核 |
|---|---|---|---|---|
| R1-01 | PASS | `rg -n '数组末尾' <doc>`；`rg -n 'no available memmap slot' <doc>` | 前者 0；后者 2 处：L533（C 引文）、L671（§2.4.2.1 表） | §2.4.2.1 表已写两种终局：“m 之后第一个空槽；若其后没有空槽，`add_memmap` 在 `panic(...)` 处停机”。C 侧 `minix3/minix/kernel/arch/i386/pg_utils.c:95-123` 的 `add_memmap` 确实是找空槽、找不到才 panic。语义成立。 |
| R1-02 | PASS | `rg -n '两条链看' <doc>`；`rg -n 'test-riscv64-uboot.sh' <doc>` | 前者 0；后者 8 处（L1128/1646/2638/2645/2667/2866/2871/2954） | §6.4 L2871-2873 改为逐脚本判据：`test-riscv64-uboot.sh` 看 `### TEST_RESULT: PASS hello-boot-riscv64 ###`、`test-riscv64-uboot-shim.sh` 看六路标、`test-kernel-image-riscv64-bootface.sh` 看表址行与失败行序；与 §5.4 L2667-2669 三行一致。合并说法已消失。 |
| R1-03 | PASS | `rg -n '所有校验在消费' <doc>`；`rg -n '不证明校验先于消费' <doc>` | 前者 0；后者 1 处（L2641） | §1.2 I4 行改为“首次消费（建表/映射/分配）前的校验必须完成；`kmain` 入口的 `validate` 是消费后的复核门”；§5.3 L2655 同口径写“不证明顺序（顺序由 §4.4.1 的调用序与代码走查保证）”。与 §3.7/§4.4.1 的逐腿执行序不矛盾。 |
| R1-04 | PASS | `rg -n -e '四个有 Rust userland 读者' -e '不是 .KinfoStruct.user_sp' <doc>`；`rg -n -e 'RS.*user_sp' -e 'user_sp.*GET_KINFO' <doc>` | 前者 0；后者 2 处（L1250、L2310） | §3.2.2.2 已改为“五个有 Rust userland 读者的语义字段”：`nr_procs`/`nr_tasks`/`release`/`version` 由 IS 读，`user_sp` 由 RS exec 经 `GET_KINFO` 读 `KinfoStruct.user_sp`（offset 8）。源码核实：`os/servers/rs/src/trap_api.rs:223-230` 的 `fn user_sp` 与 `:430` 的 `let user_sp = self.user_sp()?;`；同段补了“`KernelInfo.user_sp` 与 `KinfoStruct.user_sp` 同名不同物”。DESIGN-01 的事实口径被采用。 |
| R1-05 | PASS | `rg -n 'Direct Map' <doc>`；`rg -n 'PML4' <doc>` | `Direct Map` 12 处；术语定义在 L51。`PML4` 首次在 L1402（§3.4.2），括注“x86-64 四级页表的顶层，§3.4.3 详述” | §1.0 已有 `Direct Map` 一句定义与 `v1 腿` 定义（L50-51）；§3.4.2 首次 `PML4` 处有同级括注。弱证据：`Direct Map` 的首个命中是 L4 的前置导语（在 §1.0 之前），不是“首个命中即 §1.0 术语”；定义本身在 §1.0 与 §4.8 L2571 都在。 |
| R1-06 | PASS | `rg -n '协议族' <doc>`；`rg -n 'BOOT_HANDOFF_V1_VERSION' <doc>` | 前者 0；后者 3 处（L49、L1185、L1282） | L49 改为“**v1**：交接协议的族名（第一代），与信封版本号 `BOOT_HANDOFF_V1_VERSION`（当前 10）不是同一个计数器；v9/v10 指后者”，语义正是要求的“族名 vs 信封版本两层”；L1282 再展开协议身份/信封版本两层。字面 `协议族` 未命中是因为作者用了“协议的族名”，不是漏修。弱证据：§1.1 首次出现“三条 v1 腿”（L60）处没有加台账建议的一次性回指，但 §1.0 定义紧在其前。 |
| R1-07 | PASS | `rg -n 'L0–L10' <doc>`；`rg -n 'check-layout\.sh 的 L' <doc>`（字面）；安全等价两条：`rg -n 'check-layout\.sh.*的 L' <doc>` 与 `rg -n 'check-layout\.sh.*的检查项' <doc>` | L110 一处；字面命令 0（反引号挡在 `.sh` 与“ 的”之间）；安全等价合计 4 处：L110（检查项编号）、L1213（L9）、L2675（L10）、L2849（L3/L4/L5） | L110 首次出现处已补同指“`L0–L10` 指 `os/kernel-image/check-layout.sh` 的检查项编号，权威清单只在 02”；02 篇 §5.2 确有 `L0–L10` 唯一权威清单（`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1870`）；脚本实际编号 L0–L10 与文档一致（`os/kernel-image/check-layout.sh:9-27,47-74`）。弱证据：台账要求的“页表级 `L0`/`L2` 处加一句‘此处 L 是页表级别，不是检查项编号’”没有落地（§3.4.3 L1518 的 `L0`/`L2` 仍无该括注；L1791 的 `L0` 在代码引文里）。 |
| R1-08 | PASS | `rg -n '把镜像半留给 02 篇' <doc>`；`rg -n -e '发送侧的建根' -e '建根.*发送侧' <doc>` | 前者 0；后者 3 处（L1925、L2779、L2845），但都不是目标句 | 目标句在 §1.3 L90：“Rust 把数据与**发送侧的建自举根、开分页、绝对跳转**留在 boot-shim/自引导面，把**内核侧的加性阶段**（`arch_boot_resume_v1` 之后）与 `kmain` 进入留给 02 篇。”语义即台账要求的接缝改写；§1.4 的范围表也把“`arch_boot_resume_v1` 之后的加性阶段与换栈进入”归 02。字面 `发送侧的建根` 未命中是因为写成“建自举根”。 |
| R1-09 | PASS | `rg -n '默认（未设 .cttyline=.）' <doc>`；`rg -n '现场是 panic 刷屏并等按键' <doc>` | 各 1 处，同一行 L150 | L150 同句已加“默认（未设 `cttyline=`）”，并写“设 `cttyline=` 时 panic 走串口同步输出后立即复位，不刷屏、不等按键（§6.3）”；§6.3 L2839 的失败模式行同口径。两处互斥通路一致。 |
| R1-10 | PASS | `rg -n '标题里的“三个配套教学点”' <doc>`；`rg -n '三个配套教学点' <doc>` | 两者均 0 | §2.2 末句 L281 已改为“本节的三处教学点就是上面这三段：……”。台账的第二个命令期望 `三个配套教学点` 为 1，与它自己给出的可选修法（删“标题里的”，改成“本节的三处教学点……”）冲突；作者选了该修法，语义成立。代理命令期望值过期，不是文档缺陷。 |
| R1-11 | PASS | `rg -n -e '§2\.2 已引' -e '§2\.2 的 .k_initial_stktop. 改名' <doc>` | 0 | §2.2 L261 已改为“§3.4.1 会再用到这条机制”；§3.4.1 L1374 补了“`k_initial_stktop` 的构建期前缀规则是同一机制的另一例，其落点见 02 篇 §2.4”，即台账给的第二条可选修法。 |
| R1-12 | PASS | `rg -n '那是下一条' <doc>` | 0 | §2.3 L451 已改为“那正是下一条要排除的‘首条提升分支’的反面，不是 `xboard=2` 的后果”，语义正是台账要求的改写。 |
| R1-13 | PASS | `rg -n '\*\*2\.4 结算\*\*' <doc>`；`rg -n '2\.4 尚未切' <doc>` | 两者均 0；实际为 L613 `**2.4.1 结算**` 与同句“到这里尚未切” | §2.4.1 结算的确实是细表/粗档与 add/cut 段（“一份有据可查的可用物理内存清单”），编号与内容对齐；同句已改“到这里尚未切”。 |
| R1-14 | **FAIL（初判）→ PASS（复检，见 §5）** | `rg -n '定义见 §3\.4/§4\.2\.2' <doc>`；`rg -n 'EfiConventionalMemory' <doc>`；`rg -n 'WRITE_BACK' <doc>` | 0；1 处（L2098）；7 处（L1193/2096/2098/2117/2118/2120/2851） | 令牌定义已补：L2098 写 `CONVENTIONAL` = UEFI `EfiConventionalMemory`（固件声明可自由分配的 RAM），`WRITE_BACK` = UEFI 属性位 `0x8`；§4.2.2 Step 5 与手算例一致。但初判时 §3.2.1 的 `memmap` 行 L1193 仍写“**LMA/VMA 与 `LOADER_DATA` 的定义见 §3.4/§2.4**”。实际 `LOADER_DATA` 的定义在 §3.1 L1110（“`LOADER_DATA` 是 UEFI 内存类型标签，意思是……”，带 blockquote 定义）；§2.4（L453-779）没有 `LOADER_DATA` 一词，§3.4（L1332-1536）只有 L1479/L1487 在代码引文里使用该名、没有定义。台账处置明确要求“`LOADER_DATA` 见 §3.1”。复检：L1193 已改为“`LOADER_DATA` 的定义见 §3.1”，R1-14 改判 PASS（见 §5.2）。 |
| R1-15 | PASS | `rg -n '两种控制台' <doc>`；`rg -n '字段补充说明' <doc>` | 前者 1 处（L1254）；后者 0 | 小节标题改为 `##### 3.2.2.3 两种控制台与四个字段的补充`，属台账允许的第二个修法（“或标题改为覆盖两类内容”）：前半“两种控制台”保留，后半覆盖 `reserved_regions`/`kern_stack_top`+`syscall_entry`/`platform_sources`/`param_buf` 的补充内容。弱证据：标题说“四个字段”，实际覆盖五个字段名（`kern_stack_top` 与 `syscall_entry` 合在一个 bullet），且没有另起 `字段补充说明` 小节；内容覆盖成立，计数措辞略松。 |
| R1-16 | PASS | `rg -n 'init_boot_pt_alloc.*的.*register' <doc>`；`rg -n 'register.*boot_validate_and_prepare' <doc>` | 前者 1 处（L1471，§3.4.2）；后者 3 处（L1468/1471/1877） | §4.0 失败对照 L1877 已改为“把 `boot_validate_and_prepare` 内的 `pt_alloc::register` 挪到进入内核之后……`init_boot_pt_alloc` 只初始化 bump 区间，不写注册槽”，把 `init_boot_pt_alloc` 与 `register` 的归属分开了；§3.4.2 L1471 的“shim 侧不直接调用 `register`”是台账明确保留的合法用法。首条命令的 L1471 命中是 `register` 被 `is_registered()` 误匹配，不是 §4.0 行。 |
| R1-17 | PASS | `rg -n 'boot_alloc_region.*即停机' <doc>`；`rg -n '见 §4\.1 末段' <doc>` | 前者 2 处（L1898、L2345），≤3，且 §4.1 Step 0 括号内只出现一次；后者仅 1 处（L1898） | §4.1 末段 L1928 是唯一带完整理由（“从 memmap 兜底会把活页再发一次”）的陈述；§4.1 Step 0 L1898 是回指；L2345 是代码引文里的短注释。弱证据：台账期望 `见 §4.1 末段` ≥2，实际只有 1 处；§4.0 L1877 与 §4.6 L2530/L2533 还有“直接停机/兜底已删”的短句，但没有回指。完整长陈述已收敛到一处，重复展开的根因基本消除，指针数量未达台账预期。 |
| R1-18 | PASS | `rg -n '10 拍' <doc>`；`rg -n -e '动作 2b' -e '2-读' -e '2-拷贝' <doc>` | 各 1 处（同一行 L2038） | L2038 已写“动作 2 拆成 2a（读/预订）与 2b（拷贝/模块）两拍，所以 10 拍对应 9 个动作；动作 2b 与 §4.1 的 Step 2b 是两套编号，不要混读。”作者选了台账的“点名碰撞”修法，未改 ID；时间拍表 10 行与说明一致。 |
| R1-19 | PASS | `rg -n 'MapError::Policy' <doc>`；`rg -n 'MapError::RejectedDescriptor' <doc>` | 前者 0；后者 1 处（L2127） | L2127 已写“`PolicyError::OutOfPhysRange`，由 `classify` 包成 `MapError::RejectedDescriptor` 上抛”，不再出现 `MapError::Policy`。源码核实：`os/boot-shim/src/ebs_map.rs:326` 有 `PolicyError::OutOfPhysRange`，`:674` 有 `MapError::RejectedDescriptor(DescriptorRejection { ... })`；`MapError::Policy` 全仓不存在。 |
| R1-20 | PASS | `rg -c '不证明任意畸形 wire' <doc>`；`rg -n -e '同一批证据' -e '按机制重排' <doc>` | 2（L2640、L2653）；4 处（L2649/2653/2654/2655） | §5.3 L2649 已明写“下表与 §5.1 使用同一批宿主/真机证据，只按机制重排；它不是第二份独立证据，读证据去 §5.1 的对应行”，三行证据列都指回 §5.1 的 I1/I3、I2、I4 行（同批证据）。弱证据：台账期望 `不证明任意畸形 wire` 修复后为 1（仅 §5.1），实际 2，§5.3 的索引行仍在“不证明什么”列重复了该短语；这不是第二份证据集，但字面期望未达。 |
| R1-21 | PASS | `rg -n '五条腿都有门覆盖' <doc>`；`rg -n 'test-smp-aps-aarch64' <doc>` | 前者 0；后者 8 处（L1131/1798/2640/2645/2671/2675/2862/2954） | §5.1 I3 行 L2640 已改为“五条腿都有真机证据；其中四条有 §5.4 的启动门链，aarch64 直核由 `test-smp-aps-aarch64` 驱动到 SMP 交付”；§5.5 L2675 同口径。§3.6/§5.5 的“五条门链不含 aarch64 直核”也一致。 |
| R1-22 | PASS | `rg -n 'A2' <doc>`；`rg -n -e 'A2.*功能遗留' -e 'A2.*非测试缺口' <doc>` | 2 处（L2645、L2675）；1 处（L2675） | §5.5 L2675 已给 A2 定义：“aarch64 直核在 SMP 交付之后进入用户链时的 Data Abort”，并明确“它是已登记的功能遗留，不是测试缺口，单列不计入本表”；L2645 的“已登记的 A2 遗留”与之同指。弱证据：台账要求的“登记处”没有显式给出（只说“已登记”，未指向 ledger/issue 位置）；§5.5 是“未闭环项交接”节，可视为登记面，但读者无法从这句直接查到登记记录。 |
| R1-23 | PASS | `rg -c '20 × 72' <doc>`；`rg -n '见 §3\.2\.1' <doc>` | 1（L1185，§3.2.1）；5 处（L2010/2034/2069/2071/2481；L2539 为“删除账见 §3.2.1”） | `20 × 72` 的完整账只在 §3.2.1 的 v10 blockquote L1185；§3.2.3 的 arena 注释只留布局与 `ARENA_CAP`（L1310-1314）；§4.2/§4.2.1 只留“EBS 之后没有 startup 写入段（删除账见 §3.2.1）”；§4.5 L2481 是 `STARTUP_RECORD_STORE` 一行删除记录；§4.6 L2539 是“startup 分区的删除账见 §3.2.1”。重复展开已收敛。 |
| R1-25 | PASS | `rg -n 'pre_init.*§2\.6' <doc>`；`rg -n 'pg_clear.*§2\.6' <doc>`；`rg -n 'mem_high_phys.*§1\.2' <doc>` | 三者均 0 | §7.5 L2966 `pre_init` 行的相关章节为 §2.1–§2.3 / §2.8 / §3.6 / §4.1–§4.2 / §4.7；L2971 `pg_clear` 等五行为 §2.2 / §2.8 / §3.4.3 / §4.7 / 02 篇 4.3；L2976 `mem_high_phys` 为 §2.4 / §2.4.2 / §3.2.2.1。三个错误指针都已改正。 |
| R1-26 | PASS | `rg -n '前三个按' <doc>`；`rg -n 'BOOT_MODULE_STORE' <doc>` | 前者 0；后者 2 处（L2414 代码、L2477 表行） | §4.5 L2491 已改为“容量分两组说：下面三个容量按‘有独立设计依据’列，后几个按‘直接对齐既有契约’列”，明说挑选标准；`BOOT_MODULE_STORE` 行 L2477 为 `minix_boot::NR_BOOT_MODULES` / 12 / panic，同段 L2491 末尾补“`BOOT_MODULE_STORE = 12` 的来源是 `MODULE_NAMES` 的长度（见 §4.3），它不是独立容量”；§4.3 L2162/L2170 确为 12 项。 |
| R1-27 | PASS | `rg -n '两条腿的 store' <doc>`；`rg -n -e '两组腿' -e '直核面与 v1 接收面' <doc>` | 前者 0；后者 1 处（L1836） | §3.7 首段 L1836 已改为“这两组腿（直核面与 v1 接收面）的 store 与 boot_validate 次序相反”，不再用“两条腿”与 §3.6 的“五条生产腿”计数冲突。 |
| R1-28 | PASS | `rg -n 'reserved-big' <doc>` | 0 | §4.2.2 末与 §6.1 表后的两句“没有单独的 `reserved-big` 行”都已删除；没有留下无定义指称。 |
| R1-29 | PASS | `rg -n '只在 .USER_STACK_TOP. 一处保留' <doc>` | 0 | §4.4 `build_kernel_info` 引文的编注已改为“略去函数文档注释与 `minix_boot::` 路径限定符；`minix_boot::RamAuthority` 保留以标出权威类型”，不再声称 `minix_types::` 只在 `USER_STACK_TOP` 一处保留。引文正文与该说明一致。 |
| R1-30 | PASS | `rg -n '65–128' <doc>`；`rg -n 'free_regions.*扣除后' <doc>` | 1 处（L1178，同句含“扣除后仍有 65–128 条自由区间”）；1 处（L2484） | §3.2.1 已把口径限定为“**扣除后**仍有 65–128 条自由区间时能过落地区的 128 槽，但会在交接容量断言处 fail-closed”；§4.5 `VmBootHandoff.free_regions` 行注明“计的是 `classify` 扣除后的自由区间数”。源码核实：`os/kernel/src/vm_handoff.rs:452-566` 的 `classify` 先 `free.cut(...)` 扣除 record，再按 DM 窗口裁剪，最后填 `free_regions`，确为扣除后计数。 |
| R1-31 | PASS | `rg -n 'M16' <doc>` | 0 | §4.2 引文的省略说明现在写“`assert_modules_clear_of_kernel` 略：位置在本行之后，说明见动作 2 与 §5.1 的 I3 宿主证据行”；§5.1 I3 行存在且含 `BootStorage` 容量/半提交/one-shot 测试。`M16` 不再出现，也不再有无处可查的测试清单编号。 |
| R1-32 | PASS | `rg -n 'MRB' <doc>`；`rg -n -e 'MRB.*Multiboot' -e 'MRB.*记录' <doc>` | 2 处（L1328、L1838）；2 处（L1328、L1838） | §3.3 L1328 首次出现处已补定义：“MRB（device-tree memory reservation block，设备树内存保留块）”；源码核实：`os/libs/minix-boot/src/fdt_layout.rs:17-20` 的 `mrb_start`/`mrb_end` 注释为 “memory reservation block”，与文档一致。§3.7 L1838 的第二次出现是“DTB `/reserved-memory`/MRB 记录”，有前文定义可回指；未逐字写“见 §3.3”，但同篇定义在前，可接受。 |
| R1-33 | PASS | `rg -n 'kern_stack_top.*16 字节对齐' <doc>` | 3 处（L2328、L2355、L2372） | §4.4 防线列表第 1 项 L2355 已补“`kern_stack_top` 16 字节对齐”；§4.4.1 表 L2372 也有同名行；L2328 的引文头注明该断言在引文中省略。≥2 的期望满足。 |
| R1-34 | PASS | `rg -n '六类载荷' <doc>` | 2 处（L1070、L1838） | 两处都已限定：“`BootStorage` 拥有 U-Boot shim 腿上的六类载荷；UEFI 腿的 storage 只拥有五类（五个 `fill_*` 入口；`fill_map_tables` 一次提交两表）；UEFI 腿的 source 分区由两个阶段直写 wire”。§3.0 术语与 §3.7 两处同步，符合台账要求。 |
| R1-35 | PASS | `rg -n 'table holds 0x0 entries' <doc>`；`rg -n 'table holds 0x' <doc>` | 前者 0；后者 1 处（L2669） | §5.4 L2669 已改为“表址行、`table holds 0x` + 十六进制条目数（脚本完整串为 `table holds 0x0000000000000000 entries`）”；脚本 `os/qemu-tests/test-kernel-image-riscv64-bootface.sh:124` 的 grep 串正是 `table holds 0x0000000000000000 entries`。文档的缩写与完整串并存，且与脚本一致。 |
| R1-36 | PASS | `rg -n '签名是 .fn boot_validate_and_prepare' <doc>`（字面正则失效：`.` 吃掉 `f`）；安全等价 `rg -n '签名是 .*boot_validate_and_prepare' <doc>` | 字面 0；安全等价 1 处（L2328） | L2328 的签名已补返回类型：“签名是 `fn boot_validate_and_prepare<A: BootArch>(kern_virt_base: VirBytes, kern_phys_base: PhysBytes, kern_size: u64) -> BootPageSupport<A>`”。源码核实：`os/kernel/src/lib.rs:861-865` 的定义为同名同参，返回 `BootPageSupport<A>`；台账写的 `:863-868` 在行号上有 ±2 偏移，但函数与返回类型一致。 |

## 3. 判定（初判，修复前）

**VERDICT: FAIL（初判；两处修补后改为 PASS，见 §5）**

失败根因：**R1-14**（已修复，见 §5）。

理由：R1-14 的处置不只要求“补 `WRITE_BACK`/`conventional` 定义”，还明确要求把 §3.2.1 的 `LOADER_DATA` 指针改到实际定义处（§3.1）。初判时 L1193 写的是“`LMA/VMA` 与 `LOADER_DATA` 的定义见 §3.4/§2.4”；§2.4 无 `LOADER_DATA`，§3.4 只在代码引文里使用该名，实际定义在 §3.1 L1110。读者按该指针找不到定义，属于未修完的语义缺陷，不是代理命令的期望值问题。

其余 34 个在范围内的根因判 PASS；其中若干为“弱证据 PASS”，见下节，不单独升级为 FAIL，因为其根因的语义修复已落地，只是台账期望值与文档措辞/指针数量不完全对齐。

## 4. 残余项

### 4.1 根因级弱证据（不影响 PASS 判定，但建议下一轮收紧）

- **R1-05**：`Direct Map` 的首个命中是 L4 的前置导语，位于 §1.0 之前；术语定义在 L51，符合“§1.0 有定义”，但不符合台账“首个命中即 §1.0 术语且带定义”的字面期望。
- **R1-06**：`协议族` 字面 0 命中；作者用“协议的族名”（L49），语义等价。§1.1 首次出现“三条 v1 腿”处没有按台账建议加一次性回指。
- **R1-07**：台账要求的“页表级 `L0`/`L2` 处加一句‘此处 L 是页表级别，不是检查项编号’”未落地（§3.4.3 L1518、代码引文 L1791）。`check-layout.sh 的 L` 字面命令因反引号返回 0，安全等价 4 处；L0–L10 同指本身已修好。
- **R1-15**：标题“四个字段的补充”实际覆盖五个字段名（`kern_stack_top` 与 `syscall_entry` 合在一个 bullet）；没有另起 `字段补充说明` 小节，走的是台账允许的“标题覆盖两类内容”分支。
- **R1-17**：`见 §4.1 末段` 只有 1 处（L1898），台账期望 ≥2；§4.0 L1877 与 §4.6 L2530/L2533 的短句没有回指。完整长陈述已唯一化，重复展开的根因基本消除。
- **R1-20**：`不证明任意畸形 wire` 计数为 2（§5.1 与 §5.3 索引行各一处），台账期望修复后 1（仅 §5.1）。§5.3 已明确“同一批证据、只按机制重排”，不是第二份证据集；索引行的“不证明什么”列重复了该短语，属冗余而非语义冲突。
- **R1-22**：§5.5 给了 A2 的症状、腿与“功能遗留、非测试缺口”的归类，但没有给出台账要求的“登记处”（“已登记”无指针）。

### 4.2 建议（`## 建议` 节）仍未处理项

以下按台账建议 ID 列，均已对照当前文档取证；未列出的建议（B-S01、B-S03、B-S11、B-S12、B-S16/C-S01、B-S17、C-S02、C-S03、C-S04、C-S05、C-S07、C-S09、C-S12 等）已落地或已并入对应根因。

- **A-S01**：§2.3/§3.2.1/§3.6 的嵌套括号长句仍在（如 §3.2.1 L1178 单行 800+ 字符），未改成编号 bullet。
- **A-S02 / B-S08**：正文里的设计史仍在：L1127、L1150（“收口记录”）、L1135（“先前形态”）、L1386/L1397（“曾有一次/那次险情”）、L1544（“起初……现在已不存在”）、L2537（“合并曾被试过……已回滚”）。
- **A-S03**：“框 1 / 框 2”仍在 §1.0 L53 标“C 侧旧称”，未补 C 侧出处。
- **A-S04**：§5.1 I1 行 L2638 仍把“`KernelInfo` 字段台账”列为宿主证据，未换成真正锁字段的测试名或移入“文档约束”。
- **A-S05**：§4.5 开头 L2387 仍是“`arch_boot` 的早期动作……”，未改为“直核面 `arch_boot_finish` 或 v1 接收面 `resume_additive`”。
- **A-S06**：权威表未统一编号；目前只有容量表 L2471 有《落地区与交接容量》表名，§3.6 换算表、§4.2.1 时间拍表、§4.2.2 五步表都没有编号。
- **B-S02**：“窗口类型边界规则”段仍在 §4.4 编号列表的第 1 项与第 2 项之间（L2357），列表 1–4 被打断。
- **B-S04 / READER-C-S01 的未合并部分**：§2.4.2.5 仍是 `##### 2.4.2.5` 的嵌套小节（L734），未提升为 §2.4 的兄弟小节，§2.4.2 标题也未扩到覆盖模块内存。
- **B-S05**：§1.1 L70 仍有完整的接缝段落，§1.4 又重复一套；未收成“正式声明见 §1.4”。
- **B-S06**：`wire` 有三处完整定义：L48（§1.0）、L1067（§3.0）、L1181（§3.2.1 行内括注）；未保留一处、其余回指。
- **B-S07**：§4.2.2 只补了 `CONVENTIONAL`/`WRITE_BACK` 两个类型名，未给出 `LOADER_*`/`BOOT_SERVICES_*`/`ACPI_RECLAIM` 的 UEFI 类型名对照。
- **B-S09**：§3.6 L1824-1831 的表格（表头与数据行）没有 `>` 前缀，前面的 blockquote 在表格处中断；未按建议补 `>` 或改普通正文。
- **B-S13**：§6.4 第二/三层仍是 riscv64 单架构 QEMU 命令（L2884-2891），未标“以 riscv64 为例”或给换架构替换点。
- **B-S14**：§3.2.1 `reserved_regions` 行 L1260 仍带 `ident_regions`/`map_kernel` 的删除史，未移到 §3.4.3 或脚注。
- **B-S15**：UEFI source 分区例外仍在 §3.0 L1070、§3.2.3 L1314、§3.7 L1838 三处展开，未以 §3.2.3 为唯一展开、其余回指。
- **C-S06**：§2.4.2.5 L770 的“本仓 `MODULE_NAMES` 口径”限定语仍在括号内、位于“12/11”主句之后，未提前到主句前。
- **C-S08 未合并部分**：`satp`/`Sv39` 仍在 §3.4.3 L1518 首次出现且无定义/回指（MRB 与 L9/L10 已由 R1-32/R1-07 处理）。
- **C-S10**：§3.2.1 `memmap` 行 L1193 仍有自我重复：“内核不保留可变 memmap 副本（早期引导段已移除，内核不再保留可变 memmap 副本）”；§4.1 Step 0 的重复也仍在（L1898）。
- **C-S11**：§3.5.1 L1546 仍写“UEFI 生产入口 `prepare_and_enter_kernel_image` 仍是独立入口（§3.6）”，未按建议改为“（§4.1–§4.2）”。
- **C-S13**：§2.3 命令行解析段 L351 仍未补“末尾词无 `=` 时 `p++` 越过结尾 NUL”的边界说明，也未声明该输入不在解析范围。

### 4.3 复验过程说明

- 台账的 `复验` 命令里有若干条字面期望与作者所选修法冲突或正则本身失效；这些行在上表“实际输出摘要”与“语义复核”里已逐条说明，并给出安全等价命令：
  - `rg -n 'check-layout\.sh 的 L'`：Markdown 反引号挡在 `.sh` 与“ 的”之间，字面 0；安全等价 `check-layout\.sh.*的 (L|检查项)` 得 4 处。
  - `rg -n '签名是 .fn boot_validate_and_prepare'`：`.` 匹配了 `fn` 的 `f`，正则无法命中“签名是 fn”；安全等价 `签名是 .*boot_validate_and_prepare` 得 L2328。
  - `rg -n '三个配套教学点'`：台账期望 1，但它自己给出的可选修法就是删除该短语；作者选了该修法，实际 0。
  - `rg -n '字段补充说明'`：台账允许“标题改为覆盖两类内容”，作者选了该分支，标题 L1254 已覆盖两类内容，实际 0。
  - `rg -n '不证明任意畸形 wire'`：台账期望 1，实际 2；§5.3 的索引行重复该短语，但已明确是同一批证据。
- 未修改文档、台账或任何源码；本报告是本次唯一写入的文件。

## 5. 复检（2026-11-23，两处修补后）

### 5.1 基线变化

| 项 | 初判 | 复检 |
|---|---|---|
| HEAD | `7a356f781c8ad049c6d31a05a709bb699df66250` | `9228fd5129015ac343f2b3b715c12a3550af424f` |
| 文档 sha256 | `b1a9bd6603c3f274a42e23ef06f38c0e767ce02c844768b78e7e755712a15623` | `c9f2952333486d20f5af48164421a07aae9a4b0809841d790918cd9e0657cb3c` |
| 行数 | `2979` | `2979` |

`git diff --stat -- rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md` 显示 `3 insertions(+), 3 deletions(-)`；逐行看 diff，改动只落在 §3.2.1 `memmap` 行（L1193）、§3.2.2.3 `platform_sources` bullet（L1262）、§3.2.3 arena 段（L1316）三处，没有其它文档改动。

### 5.2 R1-14 复检：PASS

L1193 现在写：“LMA/VMA 的定义见 §3.4/§2.4，`LOADER_DATA` 的定义见 §3.1，`conventional` 与 `WRITE_BACK` 的定义见 §4.2.2 的令牌表”。

- `rg -n '`LOADER_DATA` 的定义见 §3\.1' <doc>` → 1 处（L1193）。
- `rg -n '`LOADER_DATA` 的定义见 §3\.4' <doc>` → 0。
- §3.1 的 blockquote（L1110）仍是实际定义：“`LOADER_DATA` 是 UEFI 内存类型标签，意思是……”。
- `conventional`/`WRITE_BACK` 的 §4.2.2 指针仍在 L2098 的令牌定义处。

初判的失败点已修复，**R1-14 改判 PASS**。

Residual（非阻断，且不在 R1-14 台账处置的点名范围内）：同一句的第一小节仍写“LMA/VMA 的定义见 §3.4/§2.4”。§2.4（L453-779）没有 `LMA`/`VMA` 出现；§3.4 只在 §3.4.1 的 `AT()` 段用它解释“装载地址/执行地址”，并在 L1378/L1382 以 `VMA=LMA` 的形式使用，不构成缩写定义。显式定义在 §1.0 L54：“`p_paddr` 是装载地址（LMA），`p_vaddr` 是执行地址（VMA）”。这一小节建议另行清理（指向 §1.0 L54，或明确写“LMA/VMA 的机制见 §3.4.1”并删去 §2.4）；本次不据此重判 R1-14，因为台账 R1-14 的处置只点名 `LOADER_DATA` 与 `conventional`。

### 5.3 PlatformDescSource 段复检：一致性成立

文档侧：

- L1262 的 bullet：“每个 `PlatformDescSource` 是 16 B 句柄 `{ kind: PlatformDescKind, len: Option<NonZeroU32>, phys_addr: PhysBytes }`；`len = None` 表示生产者未记录 blob 长度……wire 侧仍写 `u32`：`None` 编码为 0，解码时 0 还原为 `None`。”
- L1316 的 arena 段：“平台源 **wire 记录** 是 16 B 的 `kind u32 | len u32 | addr u64`：`len` 复用旧 `reserved` 槽位，旧生产者写 0 表示未知、新读者回退单片。”

源码侧核对：

- `os/libs/minix-boot/src/platform.rs:122-131`：`#[repr(C)]` 的 `PlatformDescSource` 字段为 `kind`、`len: Option<core::num::NonZeroU32>`、`phys_addr`；`:133` 有 `size_of::<PlatformDescSource>() == 16` 的编译期断言。
- `with_len`（`:147-150`）收 `core::num::NonZeroU32`；`with_optional_len`（`:161-172`）把 `Some(0)` 与 `None` 都落到 `new()` 的 `len: None`。
- 写侧 `os/libs/minix-boot/src/handoff_v1_write.rs:244`：`put_u32(..., source.len().map_or(0, |n| n.get()))`，即 `None → 0`、`Some(n) → n.get()`。
- 读侧 `os/libs/minix-boot/src/handoff_v1_read.rs:311`：`PlatformDescSource::with_optional_len(kind, PhysBytes(addr), Some(len))`，wire 的 0 经 `with_optional_len` 还原为 `None`。
- wire 结构 `os/libs/minix-boot/src/handoff_v1.rs:64-68`：`WirePlatformDescSource { kind: u32, len: u32, addr: u64 }`；`:402` 有 16 B/8 B 对齐断言。

一致性结论：

- 文档把进程内表示（`Option<NonZeroU32>`）与 wire 表示（`u32`，0 为未知哨兵）分开陈述，且 arena 段明确写“平台源 **wire 记录**”，与源码的两种形状一致；L1282 的“`PlatformDescSource` 用 `#[repr(C)]` 钉 16 B 句柄形状”仍然成立（`Option<NonZeroU32>` 的 niche 让结构保持 16 B）。
- L2316 的“两个直核面：DTB（`with_optional_len(DTB, …)`，各一条）”与源码 API 一致。
- 全文没有残留 `has_len`/`len()` 的旧调用描述；`len = 0` 只出现在 L1316 的 wire 段，且被限定为“旧生产者写 0”，不再冒充进程内字段语义。

Note（源码注释，不是本文件缺陷）：`os/libs/minix-boot/src/platform.rs:110-114` 的 struct 级文档注释仍写“`0` means the producer did not record a length”，在 `len` 改为 `Option<NonZeroU32>` 后已过时；同一结构的字段注释（`:127-128`）写的是 `None`。这属于 R1-38 的代码注释同步残余，不影响本次文档复检结论。

### 5.4 判定更新

**VERDICT: PASS**

R1-14 改判 PASS；35 个在范围内的根因现在全部 PASS（34 个初判 PASS + R1-14 复检 PASS）。§4 的弱证据根因与未处理建议未在本次复检中重审，仍按原样保留，不构成本次判定反转的阻断项；§4.2 的未处理建议仍是下一轮清理对象。R1-24 仍为 `BLOCKED`，不在本报告范围内。
