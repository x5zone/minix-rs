# 01-stage 01 R1 缺陷台账（V3.12，草案）

- edition: V3.12
- 轮次: R1 full
- 归并口径：按根因合并六份面板报告；括号内为原始发现 ID。
- 计数草案：发现总数 48；唯一根因数 40；重复合并 8；P0 0 / P1 5 / P2 24 / P3 19。

| 发现 ID | 单元 | 处置类 | 处置 | 复验 | 根因 ID | origin |
|---|---|---|---|---|---|---|
| READER-B-01 | 2.4 | ① | §2.4.2.1 "右余量落点"格删"或数组末尾"，改为两种终局：m 之后第一个空槽；其后无空槽则 `add_memmap` 在 `panic("no available memmap slot")` 停机（与 §2.4 引文一致） | `rg -n '数组末尾' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n 'no available memmap slot' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥2：§2.4 引文 + §2.4.2.1 表） | R1-01 | R1 |
| READER-B-02 | 5.4, 6.4 | ① | §6.4 "关键行"注释删"两条链看 `### TEST_RESULT: PASS … ###`"的合并说法，改为逐脚本判据：仅 `test-riscv64-uboot.sh` 看 `### TEST_RESULT: PASS hello-boot-riscv64 ###`，另两条按 §5.4 对应行 | `rg -n '两条链看' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n 'test-riscv64-uboot.sh' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1，且同段逐脚本映射） | R1-02 | R1 |
| READER-C-01、READER-C-02 | 1.2, 3.7, 4.4, 5.1, 5.3 | ① | I4 改为"首次消费（建表/映射/分配）前的校验必须完成；`kmain` 入口的 `validate` 是消费后的复核门"；§5.1 I4 行与 §5.3 "校验先于消费"行的"不证明什么"补"不证明顺序；顺序由 §4.4.1 的调用序与代码走查保证"，或补一条断言"校验失败后未发生建表/分配"的顺序负控 | `rg -n '所有校验在消费' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n '不证明校验先于消费' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1） | R1-03 | R1 |
| READER-A-05、READER-B-12、DESIGN-01 | 3.2, 4.4 | ① | §3.2.2.2 改为"五个有 Rust userland 读者的语义字段"：`nr_procs`/`nr_tasks`/`release`/`version` 由 IS 读，`user_sp` 由 RS 的 exec 经 GET_KINFO（`KinfoStruct.user_sp`，offset 8）读（`os/servers/rs/src/trap_api.rs:223-230`、`:430`）；删"不是 `KinfoStruct.user_sp`"；保留同段 `[ARCH]` 的"五个 used + 两个占位"；补一句"`KernelInfo.user_sp` 与 `KinfoStruct.user_sp` 同名不同物" | `rg -n -e '四个有 Rust userland 读者' -e '不是 .KinfoStruct.user_sp' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n -e 'RS.*user_sp' -e 'user_sp.*GET_KINFO' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1） | R1-04 | R1 |
| READER-A-12 | 1.0, 1.1, 3.4 | ① | §1.0 术语补 `Direct Map`（一句：内核访问物理内存的一种映射方式，详见 §4.8）与 `v1 腿`（三条跨镜像路径）；§3.4.2 首次出现 `PML4` 处补"（四级页表顶层，§3.4.3 详述）" | `rg -n 'Direct Map' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（首个命中应在 §1.0 术语且带定义）；`rg -n 'PML4' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（§3.4.2 首次出现处带"四级页表顶层"括注） | R1-05 | R1 |
| READER-A-08 | 1.0, 3.0, 3.2 | ① | §1.0 或 §3.0 术语补"**v1**：交接协议的族名（第一代），与信封版本号 `BOOT_HANDOFF_V1_VERSION`（当前 10）不是同一个计数器；v9/v10 指后者"；§1.1 首次出现"三条 v1 腿"处加一次性回指 | `rg -n '协议族' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1，§1.0 或 §3.0）；`rg -n 'BOOT_HANDOFF_V1_VERSION' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（定义句同时说明族名与信封版本两层） | R1-06 | R1 |
| READER-A-13 | 1.4, 3.2, 3.4, 5.5, 6.3 | ① | REVIEW：`check-layout.sh` 的实际编号未核，修法以核实结果为准。§1.4 首次出现 `L0–L10` 处补同指："指 `os/kernel-image/check-layout.sh` 的检查项编号，清单见 02 篇 §X"；若 02 篇清单与脚本检查项不是同一套，则给其中一套换前缀；页表级 `L0`/`L2` 处加一句"此处 L 是页表级别，不是检查项编号" | `rg -n 'L0–L10' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（首次命中行含 `check-layout.sh` 同指）；`rg -n 'check-layout\.sh 的 L' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥4） | R1-07 | R1 |
| READER-C-03 | 1.3, 1.4, 3.6, 4.2, 4.7 | ① | §1.3/§1.4 接缝改写："boot-shim 侧的建自举根、开分页与绝对跳转属于 01 的发送侧；02 只展开内核侧（`arch_boot_resume_v1` 之后的加性阶段与 `kmain` 进入）的完整机制"；把"镜像半"限定为内核侧摆放 | `rg -n '把镜像半留给 02 篇' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n -e '发送侧的建根' -e '建根.*发送侧' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1） | R1-08 | R1 |
| READER-C-05 | 2.0, 6.3 | ① | §2.0 的"现场是 panic 刷屏并等按键"前加"默认（未设 `cttyline=`）"，并把通道分流回指 §6.3 的互斥通路（设 `cttyline=` 时串口同步输出后立即复位） | `rg -n '默认（未设 .cttyline=.）' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1，§2.0 同段）；`rg -n '现场是 panic 刷屏并等按键' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 1，同行含"默认"限定） | R1-09 | R1 |
| READER-A-01、READER-B-08、READER-C-10 | 2.2 | ① | §2.2 末句删"标题里的"，改为"本节的三处教学点就是上面这三段"；或把三项补进 §2.2 标题（"与三个配套教学点"） | `rg -n '标题里的“三个配套教学点”' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0；§3.0 的"本节标题里的 GRUB"是另一处合法用法，保留）；`rg -n '三个配套教学点' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 1） | R1-10 | R1 |
| READER-A-02、READER-C-11 | 2.2, 3.4 | ① | §2.2 删"§2.2 已引"（改为"§3.4.1 会再用到这条机制"）；§3.4.1 把"§2.2 的 `k_initial_stktop` 改名"改为实际讲改名机制处，或补一句"构建期前缀规则同样作用于 `k_initial_stktop`，其落点见 02 篇 §2.4" | `rg -n -e '§2\.2 已引' -e '§2\.2 的 .k_initial_stktop. 改名' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0） | R1-11 | R1 |
| READER-C-06 | 2.3 | ① | §2.3 第二条把"那是下一条『首条提升分支』的场景"改为"那正是下一条要排除的『首条提升分支』的反面" | `rg -n '那是下一条' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0） | R1-12 | R1 |
| READER-A-04 | 2.4 | ① | 第 610 行"**2.4 结算**"改为"**2.4.1 结算**"（它结算的是细表/粗档与 add/cut 段），或移到 §2.4.2.5 之后；同句"§2.4 尚未切"改为"到这里尚未切" | `rg -n '\*\*2\.4 结算\*\*' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n '2\.4 尚未切' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0） | R1-13 | R1 |
| READER-A-11 | 3.2, 4.2 | ① | REVIEW：`conventional ≡ EfiConventionalMemory` 与 `WRITE_BACK` 位名/语义由 reader 未核源码，修前需一条 E1。§4.2.2 令牌表补 `WRITE_BACK` 行（"UEFI 写回缓存属性位；不置位表示没有普通可缓存语义，不能当普通 RAM 发给 VM"）并给 `conventional` 一句释义（UEFI `EfiConventionalMemory`，固件声明可自由分配）；§3.2.1 的"定义见 §3.4/§4.2.2"改为实际定义处（`LOADER_DATA` 见 §3.1，`conventional` 见本处补句） | `rg -n '定义见 §3\.4/§4\.2\.2' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n 'EfiConventionalMemory' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1）；`rg -n 'WRITE_BACK' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（令牌表新增行，期望 ≥3） | R1-14 | R1 |
| READER-B-05 | 3.2 | ① | §3.2.2.3 拆标题：前半保留"两种控制台"；`reserved_regions`/`kern_stack_top`/`syscall_entry`/`platform_sources`/`param_buf` 四条另起 `##### 3.2.2.x 字段补充说明`（或标题改为覆盖两类内容） | `rg -n '两种控制台' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1）；`rg -n '字段补充说明' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1） | R1-15 | R1 |
| READER-A-10、READER-C-04 | 3.4, 4.0, 4.1 | ① | §4.0 失败对照把"`init_boot_pt_alloc` 的 `register`"改为"`boot_validate_and_prepare` 内的 `pt_alloc::register`"；同段明确 `init_boot_pt_alloc` 只设 bump 区间、不写注册槽 | `rg -n 'init_boot_pt_alloc.*的.*register' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（§4.0 行期望 0；§3.4.2 原有的"shim 侧不直接调用 `register`"保留）；`rg -n 'register.*boot_validate_and_prepare' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1） | R1-16 | R1 |
| READER-A-09 | 3.4, 4.0, 4.1, 4.6 | ① | 以 §4.1 末段（带"从 memmap 兜底会把活页再发一次"的理由）为唯一完整陈述；其余四处改为回指；删 §4.1 Step 0 括号内第二次"`boot_alloc_region()` 为 `None` 即停机" | `rg -n 'boot_alloc_region.*即停机' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（修复后 ≤3，且 §4.1 Step 0 括号内只出现一次）；`rg -n '见 §4\.1 末段' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥2 回指） | R1-17 | R1 |
| READER-C-07 | 4.1, 4.2 | ① | §4.2.1 时间拍表前加"动作 2 拆成 2a（读/预订）与 2b（拷贝/模块）两拍，10 拍对应 9 个动作"；动作 ID 的 `2a`/`2b` 改为不与 Step 2b/2c 冲突的写法（如 `2-读`/`2-拷贝`），或在"三套编号要分清"里点名该碰撞 | `rg -n '10 拍' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1）；`rg -n -e '动作 2b' -e '2-读' -e '2-拷贝' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1） | R1-18 | R1 |
| FACT-01 | 4.2 | ① | §4.2.2 把"经 `MapError::Policy`/`RejectedDescriptor` 上抛"改为"`PolicyError::OutOfPhysRange`，由 `classify` 包成 `MapError::RejectedDescriptor(DescriptorRejection { failed: PolicyError, .. })` 上抛"；删 `MapError::Policy` 提法 | `rg -n 'MapError::Policy' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n 'MapError::RejectedDescriptor' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1） | R1-19 | R1 |
| READER-B-06 | 5.1, 5.3 | ① | §5.3 改为 §5.1 的机制索引并明写"同一批证据、按机制重排"（深拷→I1/I3、来源→I2、校验→I4）；或把证据列换成 §5.1 没有的测试名/命令，并写清两节不是两份证据 | `rg -c '不证明任意畸形 wire' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（修复后 1，仅 §5.1）；`rg -n -e '同一批证据' -e '按机制重排' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1） | R1-20 | R1 |
| READER-C-08 | 3.6, 5.1, 5.5 | ① | §5.1 I3 行"五条腿都有门覆盖"改为"五条腿都有真机证据；其中四条有 §5.4 的启动门链，aarch64 直核由 `test-smp-aps-aarch64` 驱动到 SMP 交付"，与 §3.6/§5.5 共用口径 | `rg -n '五条腿都有门覆盖' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n 'test-smp-aps-aarch64' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥2，§5.1 与 §5.5 同口径） | R1-21 | R1 |
| READER-B-04 | 5.1, 5.5 | ① | §5.5 给 `A2` 一句定义（症状、哪条腿、登记处）并明确归类"已登记的功能遗留，不是测试缺口"；若本节只谈测试缺口，标题收窄为"测试缺口的交接" | `rg -n 'A2' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（每处有同指）；`rg -n -e 'A2.*功能遗留' -e 'A2.*非测试缺口' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1） | R1-22 | R1 |
| READER-B-07 | 3.2, 4.2, 4.5, 4.6 | ① | 以 §3.2.1 的 v10 blockquote 为唯一完整展开；§3.2.3 的 arena 注释只留布局与 `ARENA_CAP`；§4.2/§4.2.1 只保留解释 6b 空缺所需一句并回指；§4.5 的 `STARTUP_RECORD_STORE` 行改为一行删除记录并回指；§4.6 改为"arena 身份随 startup 分区退场（§3.2.1）" | `rg -c '20 × 72' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（修复后 ≤1，仅 §3.2.1）；`rg -n '见 §3\.2\.1' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥3 回指） | R1-23 | R1 |
| DESIGN-02 | 3.2, 3.7, 4.4 | ⑥ | BLOCKED（设计批级，§四.6；本轮不实施，等作者裁决）: 直核发送侧统一成借用视图，`'static` 只在接收侧 `.bss` 落下：删 `BootReport`/`BootStorageCell`/`report_static`/`build_kernel_info` 与 `bootface_common` 的 `STORAGE`（或按 DESIGN-02 "若我错了"只删伪 `'static` 桥）；入口签名二选一：(a) `KernelInfo<'a>` 或 (b) 收 `&BootReport<'_>`。按 `review-publication.md` §四.6 属设计批级（跨 crate 所有权/借用模型），**不在轮内实施**；见"未决"节 | 落地后：`rg -n -e 'report_static' -e 'BootStorageCell' -e 'build_kernel_info' os/boot-shim/src/lib.rs os/kernel-image/src/bootface_common.rs`（期望 0）；`cargo test -p minix-boot --lib`（期望 188/0）；`cargo test -p minix-kernel --features mock --lib`（基线 866 非 ignored 通过/0/8 ignored） | R1-24 | R1 |
| READER-A-03、READER-B-03 | 7.5 | ① | §7.5 速查表章节列改正：`pre_init` → §2.1–§2.3/§2.8/§3.6/§4.1–§4.2/§4.7；五个 `pg_*` → §2.2/§2.8/§3.4.3/§4.7；`mem_high_phys` → §2.4/§2.4.2/§3.2.2.1；无法确指的删该列，不保留错指针 | `rg -n 'pre_init.*§2\.6' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n 'pg_clear.*§2\.6' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n 'mem_high_phys.*§1\.2' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0） | R1-25 | R1 |
| READER-A-06 | 4.5 | ① | §4.5 "前三个按…列"改为"下面三个容量"（明说是挑出来的三个）；`BOOT_MODULE_STORE = 12` 补一句来源（等于 `MODULE_NAMES` 的长度，回指 §4.3）；"另外几个容量定案"按表序重排或明说挑选标准 | `rg -n '前三个按' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n 'BOOT_MODULE_STORE' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望含来源句） | R1-26 | R1 |
| READER-A-07 | 3.7 | ① | §3.7 开头"两条腿的 store 与 boot_validate 次序相反"改为"这两组腿（直核面与 v1 接收面）"或直接点名两个家族，避免与 §3.6 的"五条生产腿"计数冲突 | `rg -n '两条腿的 store' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n -e '两组腿' -e '直核面与 v1 接收面' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1） | R1-27 | R1 |
| READER-A-14 | 4.2, 6.1 | ① | 删 §4.2.2 末与 §6.1 表后的两句"没有单独的 `reserved-big` 行"；若保留该信息，先补一句它指"旧版诊断曾把 reserved 表按大小拆两行"，并给出旧版出处 | `rg -n 'reserved-big' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0，或首次命中带定义） | R1-28 | R1 |
| READER-C-09 | 4.4 | ① | §4.4 `build_kernel_info` 引文编注删"`minix_types::` 只在 `USER_STACK_TOP` 一处保留以标明常量归属"，或说明该处在被略去的文档注释/相邻代码里并给出位置 | `rg -n '只在 .USER_STACK_TOP. 一处保留' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0） | R1-29 | R1 |
| READER-C-12 | 3.2, 4.5 | ① | REVIEW：`vm_handoff::classify` 的计数口径（扣除前/后）未核，修前需一条 E1。§3.2.1 `memmap` 行"65–128 条能过落地区 128 槽、交接容量断言 fail-closed"改为"扣除后仍有 65–128 条自由区间时"；§4.5 `VmBootHandoff.free_regions` 行注明计的是 `classify` 扣除后的自由区间数 | `rg -n '65–128' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0，或同句含"扣除后"）；`rg -n 'free_regions.*扣除后' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1） | R1-30 | R1 |
| READER-B-09 | 4.2, 5.1 | ① | §4.2 引文省略说明的"§5.1 M16"改为实际定位：§5.1 I3 的宿主证据行或 §5.3 的深拷行；若 M16 是外部测试清单编号，补"（测试清单 M16，见 …）" | `rg -n 'M16' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0，或同行给出可解析定位） | R1-31 | R1 |
| READER-B-10 | 3.3, 3.7 | ① | REVIEW：`MRB` 的准确同指未核源码，修前需一条 E1。§3.3 首次出现 `MRB` 处补一句同指（Multiboot 记录；按源码核实全称），或换成读者可识别的路径/结构名；§3.7 的 `MRB` 回指该定义 | `rg -n 'MRB' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（首次命中带定义）；`rg -n -e 'MRB.*Multiboot' -e 'MRB.*记录' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1） | R1-32 | R1 |
| READER-B-11 | 4.4 | ① | §4.4 防线列表第 1 项补"`kern_stack_top` 16 字节对齐"，或把"不是完整清单"的警告从 item 2 提到列表开头、明说 item 1 也不完整 | `rg -n 'kern_stack_top.*16 字节对齐' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（修复后 ≥2：§4.4 列表与 §4.4.1 表） | R1-33 | R1 |
| READER-C-13 | 3.0, 3.7 | ① | 把"实际走 `fill_*` 的腿上的六类载荷"改成"U-Boot shim 腿上的六类载荷；UEFI 腿的 storage 只拥有五类，source 直写 wire"（§3.0 术语与 §3.7 两处同步） | `rg -n '六类载荷' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（修复后每处限定 U-Boot shim 腿，或写成六类/五类分述） | R1-34 | R1 |
| FACT-02 | 5.4 | ① | §5.4 "riscv64 直核 bootface"行的判据串改为 `table holds 0x` + 十六进制条目数，或直接引脚本第 124 行的完整串 `table holds 0x0000000000000000 entries`，与 §4.3 的打印分解一致 | `rg -n 'table holds 0x0 entries' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；`rg -n 'table holds 0x' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1，且与脚本串一致） | R1-35 | R1 |
| FACT-03 | 4.4 | ① | §4.4 说明句"签名是 `fn boot_validate_and_prepare<A: BootArch>(kern_virt_base: VirBytes, kern_phys_base: PhysBytes, kern_size: u64)`"末尾补 `-> BootPageSupport<A>`（与 `os/kernel/src/lib.rs:863-868` 一致） | `rg -n '签名是 .fn boot_validate_and_prepare' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（命中行含 `-> BootPageSupport<A>`） | R1-36 | R1 |
| DESIGN-03 | 3.3 | ① | 删无生产消费者的 `SourceClass`、`SourceKind::class()`、`SourceRecord::physical_range()`、`SourceRange::physical()` 及依赖它们的测试断言（改为直接断言 `record.range` 与 `record.address_domain`）；或反向把审计的 `is_reservation_record` 改成由 `class()` + kind 子集表达，二选一，不留两条分类规则 | `rg -n -e 'SourceClass' -e 'physical_range' os/libs/minix-boot/src/memmap_source.rs`（期望 0，若选删除）；`cargo test -p minix-boot --lib`（期望 188/0） | R1-37 | R1 |
| DESIGN-04 | 3.2 | ① | 进程内 `PlatformDescSource.len` 改为 `Option<NonZeroU32>`：`new()`/`with_optional_len(None)` → `None`，`with_len` 收 `NonZeroU32`；消费者把 `has_len()`/`len()` 换成一个 `match`；wire 编码 `None → 0`、`Some(n) → n.get()`，16 B 形状不变（加 `const` 断言钉住） | `rg -n 'Option<NonZeroU32>' os/libs/minix-boot/src/platform.rs`（期望 ≥1）；`rg -n 'has_len' os/libs/minix-boot/src/platform.rs os/kernel/src/lib.rs`（期望 0）；`cargo test -p minix-boot --lib`（期望 188/0） | R1-38 | R1 |
| DESIGN-05 | 4.6, 4.4 | ① | 三处源码注释改正：`arch_boot_resume_v1` 的 arena 合同删"startup-object record"子句（保留 `LastRead(arena) ≺ Publish(VMFreePages)`）；`kernel_info.rs` 的 `memmap` 注释删不存在的 `memmap()` 访问器指向，改为"消费者按腿自行扣除，见 `vm_handoff::classify` 与 `vm_boot_exclusions`"；`build_kernel_info` 注释改为"只服务两个直核面；v1 三条腿由接收侧解码"（若 DESIGN-02 落地则随函数删除） | `rg -n 'startup-object record' os/kernel/src/lib.rs`（期望 0）；`rg -n 'memmap\(\) accessor' os/libs/minix-boot/src/kernel_info.rs`（期望 0）；`rg -n 'Every boot leg calls this function' os/boot-shim/src/lib.rs`（期望 0） | R1-39 | R1 |
| DESIGN-06 | 3.2, 3.3, 4.1 | ① | 四处精度改正：(a) §3.2.3 的 `repr(C)` 清单补 `PlatformDescSource` 例外及理由（钉 16 B 句柄形状，字段顺序仍非跨镜像 ABI）；(b) §3.3 "C 的 memmap 只有 base/len"改为"基址、长度与一个固定为 AVAILABLE 的类型标签，没有来源记录"；(c) §4.1 与 §3.2.1 的 `PT_BUMP_PAGES` 位置改为 `bootface_common`；(d) §3.2.2.3 "每架构只实现一个 `write_byte`"改为"至少实现 `write_byte`，`init` 可选，x86-64 用它初始化 COM1" | (a) `rg -n 'PlatformDescSource' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（同段含 `repr(C)` 例外句）；(b) `rg -n 'mm_type' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 ≥1）；(c) `rg -n 'bootface 的 .PT_BUMP_PAGES.' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 0）；(d) `rg -n 'write_byte' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（上下文含"`init` 可选"） | R1-40 | R1 |

## P-REFUTE 附加

| claim ID | 结果 | 处置 |
|---|---|---|
| REFUTE-01（CLAIM 01） | 反证失败（主张成立）；附 P2 观察 | ① §4.2.1 表后 bullet 的"更早一笔 UEFI 池分配"补 aarch64 的 `load_dumpdtb_blob()` `Vec`（同为预订前分配、同一 fail-closed 后果）。复验：`rg -n 'dumpdtb' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 §4.2.1 bullet 内 ≥1） |
| REFUTE-02（CLAIM 02） | 反证失败（主张成立） | 无观察；不改文档。复验：N/A |
| REFUTE-03（CLAIM 03） | 反证失败（主张成立）；附 P2 观察 | ① §4.4.1 逐腿执行序补 `kmain` 在 `validate()` 前还调用一次 `check_boot_postconditions()`（幂等复核，注明"不是门缺失"）。复验：`rg -n 'check_boot_postconditions' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望 §4.4.1 链内含第二次调用） |
| REFUTE-04（CLAIM 04） | 反证失败（主张成立）；附 P2 措辞观察 | ① §3.7 的 "`'static` 只由 `report_static` 的 one-shot 静态存储支撑"改为指向真正的守卫 `BootStorageCell::take`（`report_static` 自身无 flag）；或在句内写明"one-shot 由 `take` 与 fill 完成后的 boot 契约保证"。复验：`rg -n 'one-shot' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（同行含 `take` 或"契约"） |
| REFUTE-05（CLAIM 05） | 反证失败（主张成立）；附 P2 观察 | ①（可选）§4.6 的 arena 可回收论证把"VM 自由页在 `build_vm_handoff` 才发布"改为更紧的截止线："跳转之后不再读 arena；最早的重发者是同一 `kmain` 里的 `VmBootAllocator`，同样晚于解码"。复验：`rg -n 'build_vm_handoff' rewrite-notes/01-stage-kernel/01-boot-shim-bootstrap.md`（期望同段含"跳转之后不再读"或等价表述） |
| REFUTE-06（CLAIM 06） | 反证失败（主张成立） | 无观察；不改文档。复验：N/A |

## 建议（不进入主表）

| 建议 ID | 来源 | 建议 | 建议处置（修 / FP 反证） |
|---|---|---|---|
| READER-A-S01 | READER-A | 拆开 §2.3/§3.2.1/§3.6 的嵌套括号长句，把括号内容改成编号 bullet | 修（清单外体验项，SUGG 计账） |
| READER-A-S02 | READER-A | 正文中的设计史（"起初/已不存在/收口记录/那次险情/已回滚"）收进统一的"设计变迁"附录 | 修（与 READER-B-S08 同题合并执行） |
| READER-A-S03 | READER-A | "框 1/框 2"补 C 侧出处，或删"旧称"改直称 | 修（先核 C 注释；无出处则删"旧称"） |
| READER-A-S04 | READER-A | §5.1 I1 的"`KernelInfo` 字段台账"换成真正锁字段的测试名，或移到"文档约束"并注明不是测试 | 修 |
| READER-A-S05 | READER-A | §4.5 开头的 `arch_boot` 改为"直核面 `arch_boot_finish` 或 v1 接收面 `resume_additive`" | 修 |
| READER-A-S06 | READER-A | 给权威表编号（§3.6 换算表、§4.2.1 时间拍表、§4.2.2 五步表、§4.5 容量表） | 修 |
| READER-B-S01 | READER-B | §4.4 引文块头部补 `-> BootPageSupport` 或块末注明返回表达式 | 修（并入 R1-36，同一处补返回类型） |
| READER-B-S02 | READER-B | §4.4 的"窗口类型边界规则"段移到编号列表之外，保持 1–4 连续 | 修 |
| READER-B-S03 | READER-B | §6.1 标题补"里程碑清单"，或把里程碑表拆成带独立标题的小节 | 修（与 READER-C-S02 部分同题） |
| READER-B-S04 | READER-B | §2.4.2.5 提升为 §2.4 的兄弟小节（如 2.4.3），或扩 2.4.2 标题 | 修（与 READER-B-S16/READER-C-S01 同题） |
| READER-B-S05 | READER-B | §1.1 只保留"接缝正式声明见 §1.4"，删除重复展开 | 修（与 R1-08 同段，同批处理） |
| READER-B-S06 | READER-B | `wire` 的三次完整定义保留一处，另两处改回指 | 修 |
| READER-B-S07 | READER-B | §4.2.2 令牌表前补 UEFI 类型名对照（`CONVENTIONAL`/`LOADER_*`/`BOOT_SERVICES_*`/`ACPI_RECLAIM`/`WRITE_BACK`） | 修（与 R1-14 同题合并） |
| READER-B-S08 | READER-B | 删"收口记录/曾有一次/已回滚/先前形态"等过程叙述，只留技术理由 | 修（与 READER-A-S02 合并） |
| READER-B-S09 | READER-B | §3.6 引用块内的表格行补 `>` 前缀，或整段改普通正文 | 修（格式） |
| READER-B-S10 | READER-B | §4.1 Step 0 括号内删第二次"`boot_alloc_region()` 为 `None` 即停机" | 修（并入 R1-17） |
| READER-B-S11 | READER-B | §4.5 表前引言补"外加一行 v10 删除记录（`STARTUP_RECORD_STORE`）" | 修（并入 R1-23） |
| READER-B-S12 | READER-B | §3.2 "§2.4.2 末"改为"§2.4.2.4" | 修 |
| READER-B-S13 | READER-B | §6.4 第二/三层命令标明"以 riscv64 为例"并给出换架构的替换点，或补 x86_64/aarch64 命令 | 修 |
| READER-B-S14 | READER-B | §3.2.1 `reserved_regions` 行的 `ident_regions`/`map_kernel` 删除史移到 §3.4.3 或脚注 | 修 |
| READER-B-S15 | READER-B | UEFI source 分区例外以 §3.2.3 为唯一展开，其余三处改回指 | 修（与 R1-34 同段，同批处理） |
| READER-B-S16 | READER-B | §2.4 的"追加与切除：两个操作"小节补编号（后续顺延） | 修（与 READER-C-S01 合并） |
| READER-B-S17 | READER-B | 删除三处连续 `---`（第 1019–1020、2585–2586、2911–2912 行） | 修（已核实三处均存在） |
| READER-C-S01 | READER-C | §2.4 无编号小节编为 2.4.2（后续顺延）或并入 2.4.2 标题 | 修（与 READER-B-S16 合并） |
| READER-C-S02 | READER-C | §3.4.3/§4.4.1/§6.1 三处标题改为覆盖实际内容 | 修（与 READER-B-S03 部分同题） |
| READER-C-S03 | READER-C | §7.5 "本篇章节"列改名"相关章节"，或把 02 篇引用挪到单独列 | 修（已核实该列混入"02 篇的 5.2 节"） |
| READER-C-S04 | READER-C | 删除无台账可查的 `D-062`/`D-067`，或首次出现处给一句同指并保留锚点 | 修 |
| READER-C-S05 | READER-C | §3.5.2 补"`minix-boot` 的 `BootReport`/`BootStorage` 需要 `KernelInfo` 的字段类型，故成环"这一前提 | 修（先核 `minix-boot` 是否引用 `KernelInfo`；不成立则 FP 反证） |
| READER-C-S06 | READER-C | §2.4.2.5 把 12/11 的"本仓 `MODULE_NAMES` 口径"限定语提前到主句前 | 修 |
| READER-C-S07 | READER-C | §3.2.2 的 22 字段退场台账括号改为"`KernelInfo` 不再保留"，并回指 §3.2.2.2 的 `KinfoStruct` 活字段 | 修 |
| READER-C-S08 | READER-C | 补 `MRB`、`satp`/`Sv39`、`L9`/`L10`、`VmBootHandoff`、`same pool` 的定义或回指 | 修（`MRB` 并入 R1-32；`L9`/`L10` 并入 R1-07；其余另修） |
| READER-C-S09 | READER-C | v10 删除 startup 分区保留 §3.2.1 一处完整账，其余四处改回指 | 修（与 R1-23 同题） |
| READER-C-S10 | READER-C | 删 §4.1 Step 0 与 §3.2.1 `memmap` 行的两处自我重复 | 修（§4.1 部分并入 R1-17；§3.2.1 另修） |
| READER-C-S11 | READER-C | §3.5.1 的"（§3.6）"改为"（§4.1–§4.2）" | 修 |
| READER-C-S12 | READER-C | §2.2 补"Multiboot 不保证进入内核时的栈；boot2 只取证到实际执行"的前提 | 修（先核 boot2 栈状态） |
| READER-C-S13 | READER-C | §2.3 补"末尾词无 `=` 时 `p++` 越过结尾 NUL"的边界说明，或声明该输入不在解析范围 | 修（先核 `mi_cmdline` 是否保证末尾词带 `=`；保证则 FP 反证） |

## 未决/需主 agent 裁决

- **R1-24（DESIGN-02）**：跨 crate 的发送侧所有权/借用模型变更（删 `BootReport`/`BootStorageCell`/`report_static`/`build_kernel_info`/`STORAGE`，改 `arch_boot` 入口签名），按 `review-publication.md` §四.6 属"设计批级改动"，不在轮内实施。需主 agent 裁决：(i) 中止本轮回 Manuscript 阶段落地并重设基线，或 (ii) 只实施"删伪 `'static` 桥"的半版（保留 `BootReport`/`KernelInfo` 两套字段）；(iii) 若走全量，入口签名选 (a) `KernelInfo<'a>` 还是 (b) 收 `&BootReport<'_>`。在裁决前该行不得记 `FP` 或 `②`，也不得宣告该项闭环。

## 归并风险

1. **R1-04 的合并口径最可能被复核挑战。** READER-A-05 与 READER-B-12 都把问题读成"计数口径不齐"，其建议修法（只承认四个读者 + `user_sp` 旁路）与 DESIGN-01 的源码事实（`KinfoStruct.user_sp` 有活的 RS exec 读者）相反。三者同句同根因，但修复必须采用 DESIGN-01 的事实口径；若沿用 A-05/B-12 的措辞，会把一条 P1 事实错误修回原样。主 agent 复核时请以 `os/servers/rs/src/trap_api.rs:223-230`、`:430` 为准。
2. **R1-03（READER-C-01 + READER-C-02）按"I4 口径与证据不一致"合并。** 若主 agent 认为不变量文本与证据矩阵是两个可独立关闭的根因，可拆成两行；拆后唯一根因数变 41、重复合并变 7。
3. **R1-25（READER-A-03 + READER-B-03）合并了 §7.5 的两种落空。** A-03 只覆盖 `pg_*` 与 `pre_init` 行；B-03 另指出 `mem_high_phys → §1.2` 无该符号。两处同级同表，合并安全，但 B-03 多覆盖一行。
4. **R1-05（A-12）与 R1-06（A-08）都命中读者清单第 1 条，但术语与修法不同**，保持两行。若主 agent 按"术语定义后置"统一并根，唯一根因数会下降；本草案不建议合并，因为一个补 `Direct Map`/`PML4`/`v1 腿`，一个补"协议族 vs 信封版本"。
5. **P3 准入风险（§4.0c 严重度门槛）。** READER/DESIGN 的 P3 默认应入 `suggestions.md`（以 SUGG 行计入 SUGG 键、阻断 CONVERGED），本草案为覆盖完整性把 19 条 P3 全放主表。最可能应转 SUGG 的：READER-A-06（"前三个"）、READER-A-07（"两条腿"）、READER-B-11（§4.4 列表不完整，§4.4 已有"不是完整清单"警告）、READER-C-09（编注）、FACT-03（引文块已隐含返回值）、DESIGN-06 b（"只有 base/len"窄读可判风格）、DESIGN-03/04（设计取舍类）。主 agent 若转 SUGG，需把这几个 ID 移出主表并调整计数（发现总数/根因数按台账口径另算）。
6. **可能应转 FP 的行。** FACT-03（作者可能刻意只写"签名"的入参形状，返回值由引文块隐含）、READER-B-11（item 1 不列对齐可能因"不是完整清单"已声明）、DESIGN-06 b（按"memmap 条目在本项目语义上只有 base/len（类型恒 AVAILABLE）"的窄读）。三行都不影响其余行；转 FP 时须按 §4.1 写源码/文档原文反证，不能只写"作者取舍"。
7. **证据不足、已标 REVIEW 的四行**：READER-A-11（`conventional`/`WRITE_BACK` 的 UEFI 语义）、READER-A-13（`check-layout.sh` 实际编号）、READER-C-12（`classify` 计数口径）、READER-B-10（`MRB` 准确同指）。修复前各需一条 E1 核实；核不实则按 `⑤` 处理，不得直接改文档。
8. **REFUTE 的 4 条 P2 观察不在 48 条内。** 按 brief 的计数规则，`CLAIM`/REFUTE 不计入发现总数；若主 agent 决定准入这 4 条（aarch64 `dumpdtb` Vec、`kmain` 第二次 `check_boot_postconditions`、`report_static` one-shot 措辞、arena 生命周期论据），发现总数变 52、根因数 +4，需另开行并给 origin。REFUTE-02/06 无观察，不需改文档。
9. **§7.5 属 EXEMPT 章。** `tools/review-units.py` 的 `EXEMPT` 匹配"参见"标题，章 7 无单元 ID；R1-25 的单元列写 `7.5（EXEMPT 章，无单元 ID）`，机械门若要求合法单元 ID，需主 agent 换成章 ID 或标 `n/a`。
10. **代码类行的批次与基线。** 面板 survey 与 certify 期间未写 `os/`；R1-24 按 `review-publication.md` §四.6 需轮外落地；R1-37/38/39/40 是局部代码改动，可按 L3/L3.5 批次实施，但每批后必须重设基线（提交号 + 文档 sha256 + 测试计数）并重跑受影响的机械门。DESIGN-05 第 3 处（`build_kernel_info` 注释）会随 R1-24 删除函数自然消失，批次顺序应先 R1-24 后 R1-39。
11. **无 P0；40 个根因恰好等于面板的 40 条上限，未超过**，因此不需要"未展开根因"清单；若复核后根因数超过 40，需按 `review-panel.md` 在末尾补未展开清单。
