# riscv-reviewlog.md — riscv64 静态扫描线程

## 0. 本线程的定位、判定语义与纪律

本文件由「riscv64 静态扫描线程」产出。主线程（NK4-C，见同目录 `NK4C-WORKLOG.md`）正在逐架构打通
「冷启动到 rc marker 到 18-stage 命令面到 minix3 tests 上机」这条完整链路，x86_64 已达成
（§1.105 单核 marker 首次出现、§1.106 echo/ls/cat 三命令真机跑通），aarch64 进行中（§1.119）。
本线程不跑真机、不改任何代码，只做一件事：把 x86_64 与 aarch64 两轮已经付出学费的每一处，
静态对照 riscv64 的当前代码提前核一遍，让主线程将来的 riscv64 轮次从「探索」降级为「对账」。

三条判定语义，贯穿全文：

1. **「已就绪」**：静态证据表明该阶段在 riscv64 上无需新代码即可工作，但按任务书铁律，它只是
   「可以开真机轮」的必要条件而非充分条件——真机判据永远归主线程。
2. **「有缺口」**：riscv64 上缺少一段已知需要的代码，且缺口的位置与形状已经用文件加行号钉死。
   所有缺口一律记录在案交主线程排期，本线程不修。
3. **「需真机才能判」**：静态分析无法替代运行时证据的项，单列进 §A.13 真机待验表。

真值来源：riscv64 现状断言全部来自实际读到的文件与行号（第 1 轮 2026-09-27 上午、
第 2 轮同日下午，均为当日 HEAD）；工作日志叙述与当前代码冲突时以代码为准并标注「翻案」。
唯一写入的文件就是本文件。

输入面（第 2 轮补全）：第 1 轮精读 `NK4C-WORKLOG.md`（§1.78–§1.119 全量）与
`tmp/nk4a/fixlog_m32_p2.md`、`tmp/nk4a/fixlog_m32_round2.md`（NK4-B P3 M3.2 对
`os/kernel-image/check-layout.sh` 的评审账，「预期表缺第三架构即硬失败」「riscv64 首段
虚拟地址惯例只有 plus_phys|virt 两种合法值」两笔加固构成本文 A.0/B-D1 的证据）；
任务书提到的 `tmp/nk4c-fixlog.md` 不存在（`ls tmp/*.md` 只有三个 invariant 讨论稿）。
第 2 轮补扫同目录其余 17 份日志：`NK4B-WORKLOG.md`（2446 行精读——riscv64.ld、
check-layout.sh、OpenSBI 装载链、M4.x 里程碑的产出线程，riscv 命中 164 处）、
`NK4-REGRESSION-REVIEW-20260922.md` 与 `-PART2.md`、
`NK4C-REVIEW-REPORT-20260927-R3.md`、`NK4C-REVIEW-REPORT-20260923.md` 与 `-R2.md`、
`NK4C-OPENING-PROMPT.md` / `NK4C-RESUME-PROMPT.md`（含主线程 riscv 既定路线，
见 §B D1/D7）、`NK4B-TODO.md`、`NK4B-OPENING-PROMPT.md`、
`PATTERN-SCAN-REPORT-20260923.md`、`NK4A-REVIEW-REPORT.md`、`README.md`、
NK4A 群四份（riscv 零命中，仅存档参照）。`.zcode` 侧：
`~/.zcode/cli/log/zcode-2026-09-22..27.jsonl` 六份逐日日志 grep riscv64 全部零命中
（会话过程日志，不含未入库的方向信息）；`~/.zcode/cli/rollout/` 是本线程及子代理的
模型输入输出流，无独立证据价值。

---

## 进度快照（跨轮状态，供续轮接手）

| 轮次 | 日期 | 覆盖面 | 新增证据锚点 | 遗留 |
|---|---|---|---|---|
| 第 1 轮 | 2026-09-27 | 真值输入全量（`NK4C-WORKLOG.md` §1.78–§1.119 + 两份 fixlog）＋ os/arch riscv64 全部 17 文件 ＋ os/plat riscv 腿 ＋ 内核 arch_boot/trap_dispatch/with_protection ＋ minix-sys ipc.rs 门控 ＋ 装机链（riscv64.ld、boot-shim、xtask、minix-boot）＋ VM 帧池/TTY 串口/init 用户态抽查 ＋ 生产配方 cargo check | 见 §A/§B/§C 全部锚点；编译性证据见 §A.0 | §A 各「有缺口」项待主线程排期；D1–D6 待用户拍板；真机待验表未启动 |
| 第 2 轮 | 2026-09-27 | 补扫 17 份 N\*/\*RE\* 日志（NK4B-WORKLOG 精读 2446 行、回归评审×2、R3 评审报告、NK4C 开局/续跑 prompt、NK4B-TODO、PATTERN-SCAN、NK4A 评审报告、20260923 两份评审）＋ .zcode 六日日志排查（riscv 零命中）＋ 4 项现场复核 | sstatus.SUM 缺口（C25）、reply_wire 未迁移（C24）、M4.3 OpenSBI 腿已实证、M4.4 三件缺件＋.bss 清零、a1 状态车道选型免疫（C26）、riscv64.ld CRLF 基线（C32）、NK4C prompt 阶段 3 既定裁决（甲案＋SUM 丙案） | D7 新增；C 表 24–31 行；A.1/A.3/A.6 修订；A.0 补 CI 门建议 |
| 第 3 轮 | 2026-09-27 | WORKLOG 结构债深度分析（§D 全新章）：8 笔债的现状锚点核验（reservedqueue/quiet_wait/发射端口/ev_new 缺席/link.ld 引用面/handoff 契约等 10 项 grep 实测）＋ 五参照系对照 ＋ 每债 ≥3 方案 ＋ 修复批次矩阵 | D.1–D.9；`impl MemType for AnonymousMemory` 无 eager 物化复核（memtype.rs:253 起）；boot-shim lib.rs:136 错端口 0x3f9 在 HEAD 复核 | D 节各「裁决归属」待用户/主线程认领；批次建议入 §D.9 |
| 第 4 轮 | 2026-09-27 | 外部复核抽验通过后补账 4 笔遗漏债：债⑨ PAF_CLEAR（管线三层建成、漏斗一接头未插——三后端 CLEAR 分支全在、`to_alloc_flags` 仍 `allow(dead_code)`、`alloc_pfn_reclaiming` 无 flags 形参）、债⑩ 缓存维护（全仓 fence.i/cvau/iallu 零命中【第 5 轮已修正：fence.i 实有 2 行命中，见 D.11】，riscv 面首次入账）、债⑪ 代刷腿翻案（vmctl FlushTlb/InvlPg 三架构已在＝FIX-24，V13-P2-1「零调用者是有意设计」的依据在 VmDm 通道不成立）、债⑫ 吞错家族目录（~30 站点/12 文件＋旗舰两点锚点修正） | D.10–D.13＋D.0/D.9 扩表；fs-rt transport.rs:272/250/262、pm ipc/vfs.rs:270、syscall.rs:2555-2592/2767-2774、arch_do_vmctl.c:51、cow_exec_pf.rs:281-297、vir_region.rs:59-71、三分配器 CLEAR 行 | 债⑫ 续轮逐点展开（30 站点三问表）；移交项待主线程改口（见下待办区） |
| 第 5 轮 | 2026-09-27 | 外部复核驱动修正债⑩：riscv 散点 fence.i 翻案（higher_half.rs:54 活指令＋:38 注释，boot 腿 hart0 一次性）→ D.11 现状段重写、方案表对位列更新、推荐收紧为甲；主动扫描 WORKLOG/FIXLOG 补漏：债⑬ 新立（bump 池无回收＋kerninfo 责任移交，「结构债三件套」第二件）、B33a 入债⑫家族（正 errno 成功车道）、债⑥ 方案乙补三件套锚点 | 修正版分模式 grep（fence.i=2、cvau/iallu=0）＋第 4 轮原命令噪声定量（2960 行）；kerninfo.rs:24-31；exec.rs:53-62；x86_64/aarch64 higher_half.rs 无散点核查 | D.11 误判教训入 misc_concepts 候选（移交项）；os/kernel/src/arch/ 三架构文件除 higher_half 外仍欠逐行对账（附录欠账区） |
| 第 6 轮 | 2026-09-27 | 编辑补账三笔（D.14 移至 D.13 后、跨债总结句改十三笔并移全章文末、快照第 4 轮行加注不改写）＋债⑫ 大件交付：逐点处置表 31 站/13 文件（乙 7＋维持 23＋甲 1；B33a 调用侧复核＝calls.rs:633 不取负现症实锤；同线过滤口径修正——跨行拆分调用漏计 devman 3 站）；**收敛声明：§D 结构债扫描线按停止规则收敛** | D.13 逐点处置表两批；pm calls.rs:631-634；devman 跨行补计；移交项三笔列全 | §D 线收敛；后续价值在批次实施即时修正；§A 范围挂账项不动 |

**移交项（本线程无权限做，交主线程一次性收口；第 6 轮列全为三笔）**：

1. **WORKLOG §1.115 债② 口径**：原文「riscv64 同构接入先行＝让 BootHandoff 获得第二
   消费者」与本文件 §D.2 触发条件 3 的发现冲突——prompt 已裁的 riscv 甲案是
   kernel-image 自当引导体、**不经 BootHandoff 跨镜像交接**，甲案落地不构成第二消费
   者。改述建议：「riscv 甲案落地可验证『kernel-image 自引导』这一第三形态，
   BootHandoff 的第二消费者仍等 x86 迁移或其它消费方」，并加反向引用
   `notes/rewrite/fork-syscall-rewrite/riscv-reviewlog.md §D.2`。
2. **NK4C-WORKLOG §1.111 的「未接线已知缺口」措辞**：第 4 轮债⑪（本文件 §D.12）
   翻案钉死——vmctl FlushTlb/InvlPg 内核腿三架构已实现（FIX-24，
   `syscall.rs:2555-2592`），VM 侧零调用者是有意设计（V13-P2-1），真实缺口是
   「V13-P2-1 的刷新依据在 VmDm 通道不成立」。主线程下次触碰 §1.111 相关文本时按
   §D.12 措辞修正，反向引用 `riscv-reviewlog.md §D.12`。
3. **misc_concepts.md 教训条目**（第 5 轮登记，本线程无权写该文件）：「多模式 grep
   的负结论必须按模式拆分计数，且禁止出自被 `head` 截断的输出」——第 4 轮债⑩
   「全仓 fence.i 零命中」误判的完整因果（2960 行子串噪声＋截断＋未验证即下负结论），
   详见本文件 §D.11 自审段。

**移交项二（第 5 轮，misc_concepts 候选，本线程无权写该文件）**：「多模式 grep 的
负结论必须按模式拆分计数，且禁止出自被 `head` 截断的输出」——第 4 轮债⑩「全仓
fence.i 零命中」误判的完整因果（未锚定 `dc `/`ic ` 模式命中 2960 行子串噪声、
`head -8` 截断、截断处未验证即下负结论、复核时才由外部发现反例）。候选归属文件：
`notes/rewrite/fork-syscall-rewrite/misc_concepts.md`。

下一轮建议入口：按 §A.7（park 与栈模型，最高优先缺口）与 §A.5（帧池页表通道 sfence）准备
修复排期清单；B 表待用户裁决后把选定方案展开成实施配方。第 2 轮修正：D1/D7 的裁决项
发现主线程已有既定路线（NK4C-OPENING-PROMPT 阶段 3：甲案＋SUM 丙案＋sscratch 交换腿），
续轮以该路线为基线对账而非重新裁决。

---

# A. 按 boot 阶段排的 riscv64 接入验收清单

阶段骨架照抄 aarch64 走过的顺序。每阶段三栏：aarch64 对应轮次与解法锚点；riscv64 当前
代码的证据；判定。文件路径以仓库根为基，行号为 2026-09-27 读到的当前值。

## A.0 可编译性

**aarch64 对应轮**：§1.107——x86 专属的 `pic_init` 与两处遗留探针未做架构门控，导致
aarch64 目标报 17 处编译错误；解法＝删除探针＋把 `minix_plat::pic_init()` 门控为
`#[cfg(target_arch = "x86_64")]`。

**riscv64 现状（本轮实测）**：

- 工具链就绪：`rustup target list --installed` 含 `riscv64gc-unknown-none-elf`。
- 用生产配方编译通过。生产配方指：架构 crate 关闭默认特性、按架构开特性（`os/xtask/src/image.rs:18-26`
  的模块注释说明 boot-shim 与各镜像一律以 `--no-default-features --features fw-<架构>-none`
  形态构建，规避「显式混形态调用」陷阱）。本轮实测三条命令全部 Finished：
  1. `cargo check -p minix-arch --no-default-features --features riscv64 --target riscv64gc-unknown-none-elf`
  2. `cargo check -p minix-plat --no-default-features --target riscv64gc-unknown-none-elf`
  3. `cargo check -p kernel-image --features fw-riscv64-none --target riscv64gc-unknown-none-elf`
     （第三条连带 minix-kernel、minix-sys、minix-boot、minix-arch 依赖链一起通过，仅警告。）
- **配方敏感性警告（给主线程）**：用裸 `cargo check -p minix-arch --target riscv64gc-unknown-none-elf`
  （默认特性 = `runtime-window`）会得到 372 个错误的「假断裂」——根因是该形态引入需要 std
  的代码路径（`arch/src/arch/dm_coverage.rs:228` 的 `use std::collections::BTreeMap`），
  与目标无 std 的连锁反应。这与 aarch64 §1.107 的「门控缺失」不同类：aarch64 是缺一个
  `#[cfg]`，riscv64 是验收命令必须用生产配方。将来任何 riscv 编译性检查（包括 CI）都应
  固化 `--no-default-features` 配方。
- 静态布局断言已就位：`os/kernel-image/check-layout.sh:70` 有 riscv64 行（特性
  `fw-riscv64-none`、链接脚本 `riscv64.ld`、两个基址的期望值、首段虚拟地址惯例 `virt`），
  且行 109 注释自证「riscv64 已在 P4 M4.2 落地，实测两值」。
- **372 错误假断裂有独立旁证**：`NK4B-WORKLOG.md` M4.4「事实二」用同一条裸命令得到同一批
  372 条错误，并当场登记「任何人对 minix-kernel 做真机构建试探必须带 --no-default-features；
  不带得到的 372 条错是方法伪影，不是缺陷」——与本线程第 1 轮的独立发现互为印证。
- **结构性防线建议（登记交主线程）**：`NK4-REGRESSION-REVIEW-20260922-PART2.md` 的
  OQ-1 与 `PATTERN-SCAN-REPORT-20260923.md` 的 P6 门都提出同一件事：给
  aarch64/riscv64 目标加常驻 CI check 门（建议 qemu-tests.yml 或独立 workflow），
  把「公共路径的 x86 取证代码打断跨架构编译」这一事故类（已发生两次：9e115387e、
  §1.107）结构性消灭。本线程补一条：CI 门必须用生产配方（--no-default-features），
  否则会把 §A.0 的方法伪影当成真回归。另外 `NK4C-OPENING-PROMPT.md:89` 记载
  `minix-ci:1.94-arch` 镜像已含 aarch64 与 riscv64gc 两个 target（本机 docker 无该
  镜像，按内存铁律回退宿主时须自行确认 target 在位）。

**判定：已就绪（编译级）**。真机轮开工前唯一注意点＝固化生产配方。

## A.1 装机与取镜像面

**aarch64 对应轮**：§1.115 方案 A——boot-shim 跳板把控制权绝对跳进独立链接的高半
`kernel.elf`，跨镜像用 `BootHandoff` 载荷交接（`os/libs/minix-boot/src/handoff.rs`）；
装机面＝UEFI 盘形（FAT 分区里的 `.efi`）。

**riscv64 现状**：

- 链接脚本契约位已产出：`os/kernel-image/riscv64.ld`（101 行）——内核虚拟基址
  `0xFFFFFFC000000000`（Sv39 规范高半起点，注释第 21-29 行同时警示两个非法值
  `0xFFFFC00000000000` 与 `0xFFFFFC0000000000` 的重蹈坑）、物理基址 `0x80200000`
  （QEMU virt 从 `0x80000000` 起加 2MiB 对齐）、2MiB 跨距收口、64KiB 引导栈、riscv 专属
  拆分节（`.ldata/.lbss/.sdata/.sbss/.srodata/.sdata2`）全部归位。脚本头部注释明言：
  入口态由 OpenSBI（RISC-V 监督模式固件）以监督模式跳入，寄存器 `a0`＝hart 号、`a1`＝
  设备树物理指针，但把 `a1` 递进 KernelInfo 属尚未接线的交接协议。
- boot-shim 库侧完整、生产入口缺失：`os/boot-shim/src/opensbi_helpers.rs:193-267` 的
  `OpenSbiBootShim::prepare_boot` 已实现全部步骤（U-Boot 预载文件表 `BootFileTable`、
  硬编码 QEMU virt 内存图、根页与页表页池分配、内核 ELF 与 boot 模块装载、设备树→
  平台源）；`os/boot-shim/src/loader.rs:89-90` 有 `UbootFileLoader`。但 boot-shim 这个
  crate 唯一的二进制目标是 UEFI 入口（`os/boot-shim/Cargo.toml` 只有一个 `[[bin]]`，
  `src/main.rs:20` 用 `uefi::prelude`）；`opensbi_helpers.rs:270-274` 注释明言期望某个
  入口跳板在调 `prepare_boot` 前调用 `install_boot_file_table(a0)`，而全仓 grep 无此调用者
  （仅 qemu-tests 载体引用库）。qemu-tests 载体（`hello-boot-riscv64/src/main.rs` 头部
  「Why not use OpenSbiBootShim::prepare_boot」注释）是绕开 `prepare_boot` 手工构造
  `BootPrepareResult` 的，不构成生产入口。
- U-Boot 真链载体已验证过最小内核：`os/qemu-tests/test-riscv64-uboot.sh`（T-10）——
  U-Boot 扫 virtio-blk FAT 盘、`fatload` 装载内核 ELF、`bootelf` 跳入口，hello-boot 载体
  达到 PASS。注意它走的是「U-Boot 直接 bootelf 跳内核 ELF」形态，不经过 boot-shim；
  该载体能过是因为 hello-boot 的入口是平坦物理地址，而生产镜像的入口是高半虚拟地址
  （见下方 U-Boot 陷阱条）。
- **生产镜像的 OpenSBI 装载腿已实证（NK4B M4.3，第 2 轮补入）**：
  `os/qemu-tests/test-kernel-image-riscv64.sh` 用 QEMU `-bios default`（OpenSBI v1.3）
  直载 M4.2 的生产镜像：固件输出 `Domain0 Next Address = 0x80200000`（恰为
  `riscv64.ld` 的 `KERNEL_PHYS_BASE`，即固件按镜像自身最低装载物理址装载）、
  `Next Mode = S-mode`、`Next Arg1 = 0x8fe00000`（DTB 物理址，镜像尚未取用），镜像
  横幅在第 57 行出现；六轮真机 EXIT=0 且串口 md5 逐字相同，另有三格反判矩阵
  （喂 x86_64 工件＝假 PASS 面、paddr 整体 +2MiB＝A1 当场 FAIL，证明断言有判别力）。
  高半入口地址能在分页关闭时被执行的原因＝riscv64 默认 medany 代码模型下 `la` 被松弛为
  PC 相对寻址（`auipc+addi`），按实际执行 PC 解析——这同时是真机证据，支持 §A.2
  「高半进入语义」的乐观侧。
- **M4.4 三件缺件（NK4B M4.4 勘察钉到字段级，第 2 轮补入）**：生产镜像 `arch_boot`
  契约（`kernel/src/lib.rs:243` riscv64 臂）要被调到，缺三件事：
  1. **memmap 来源**：`os/kernel-image/src/main.rs` 的 riscv64 入口序列全文不读 `a1`
     （本轮 grep `a1|dtb|fdt|device` 零命中复核 NK4B 结论仍成立）；DTB 已由固件递到
     `0x8fe00000`，且 `test-rt-birth-riscv64/src/main.rs:426、493-501` 有逐行可抄的
     a1 递取＋DTB 解析＋`parse_by_kind` 真机先例；
  2. **12 个 boot_modules 来源**：`os/libs/minix-boot/src/kernel_info.rs:203-207` 的
     `validate()` 要求非空（`NR_BOOT_MODULES = 12`，`:333`）——装机面（xtask riscv 腿）
     必须先给出模块装载通道；
  3. **handoff 执行者裁决**：NK4B 列甲（kernel-image 自当引导体）/乙（新增 riscv
     boot-shim 等价体）/丙（本弧不点电）三案；**主线程既定路线已裁甲案**
     （`NK4C-OPENING-PROMPT.md` 阶段 3.1：「kernel-image riscv64 接 a1 DTB → 解
     memmap + 模块装载源 + .bss 清零 → 调 arch_boot」），续轮以该裁决为基线。
- **入口 `.bss` 清零缺失（NK4B M4.4 事实五，甲案共同前置）**：`kernel-image/src/main.rs`
  的三架构入口序列都只有立栈/清链寄存器/`call`，无 `.bss` 清零循环；而镜像的
  `IMAGE_ALLOCATOR`（带 `cursor: AtomicUsize`，`main.rs:197,232`）住在 `.bss`
  （`readelf` 实证 NOBITS），非零 cursor 让首次分配返回垃圾址。仓内唯一真机跑到
  用户态的载体 `test-rt-birth-riscv64` 入口 asm 带清零并注释自证（K10 round-1 finding：
  OpenSBI 交接不清内核镜像 BSS，垃圾读出假就绪）。
- **U-Boot 腿的入口陷阱（NK4B M4.3 评审登记，三方向未裁）**：`bootelf` 按 ELF 的
  `e_entry` 跳转，生产镜像 `e_entry` 是高半 `0xFFFFFFC000000000`，`satp=0` 下直跳即陷。
  三方向：(a) `.ld` 加平坦入口蹦床、(b) U-Boot 腿只装载不跳转、(c) 三架构统一走
  OpenSBI/boot-shim 一条装载链。宿主前置件缺口同轮更正：缺 `mkimage`（u-boot-tools）
  与 U-Boot blob 两项（无 dtc——M4.1 曾探错）；CI 镜像连 qemu 都没有，加包需重建
  `minix-ci`。
- xtask 双 honest bail（诚实退出而非假装成功）：装机面 `os/xtask/src/image.rs:195-203`
  对 riscv64 直接报「暂不可装配：启动路径不是 UEFI 盘形」并有回归测试（image.rs:710-714）；
  QEMU 调用面 `os/xtask/src/qemu.rs:50-53` 同样 bail（qemu.rs:39、63 均无 riscv 参数分支）。
  连带后果：riscv 目前没有任何 `-smp` 配置位——aarch64 的教训（§1.118：xtask 硬编码
  `-smp 4` 次级核未就绪引发调度扇出失败）在 riscv 尚无承载体，这是一个还没被踩过但要
  在写 qemu.rs riscv 分支时一次定对的决策点。

**判定：有缺口（形状已钉）**：
1. OpenSBI 腿装载生产镜像已实证（NK4B M4.3）；缺的是「镜像到达入口之后」的三件
   （memmap 来源、12 模块来源、甲案实施＋`.bss` 清零前置）——判据属 NK4C prompt
   阶段 3.1/3.2 与 NK4B-TODO M4.4。
2. xtask 装机/调用腿仍 honest bail（qemu.rs:39/63 无 riscv 分支、无 `-smp` 配置位）；
   扩面时利用 M3.3 决策四已铺好的表驱动先例（`Arch::uefi_slots()`，riscv64 行为 None，
   NK4B M3.3 决策四原话「以后 riscv64 要接装机面是往表里加一行，不是再改一次控制流」），
   并连回归测试一起改。
3. boot-shim 侧 `OpenSbiBootShim::prepare_boot` 库完整但生产入口二进制缺位——在已裁
   甲案（kernel-image 自当引导体）下，这份库的角色降为参照/备选，不必强行组装；
   `NK4B-WORKLOG` M4.4 评审亦将其登记为乙案形状。

## A.2 页表与高半进入

**aarch64 对应轮**：§1.110（`split_huge`/`grant_user_walk` 两个 trait 默认返回
NotSupported，boot 期 ELF 驱逐腿撞墙 → MappingFailed）；§1.114（内核跑在可切换的低半
身份映射上，首次切根自杀）；§1.115（方案 A 落地，独立 ELF 高半启动 + additive 续跑，
并踩「不得清零正激活的根」坑）。

**riscv64 现状**：

- Sv39 三级页表实现完整：`os/arch/src/riscv64/paging.rs:64-66`（级移位 30/21/12）、
  `:522-544` `enable()`（OpenSBI 不开 MMU，由内核自己写 `satp` 并 `sfence.vma`，注释
  524-535 行解释了与 x86/aarch64「固件已开分页」的差别）。
- **每进程根继承高半已实现（x86 `inherit_supervisor_half` 模式的 riscv 版）**：
  `paging.rs:486-505`，把 L2（根级）第 256..512 项原样拷贝，注释说明用户映射（低半）
  不会触碰高半所以共享下级表页是安全的。这是 §1.114 根因的天然免疫：riscv 只有一张
  `satp` 根表，高半随每进程根继承，切根后内核高半仍然可达。
- 高半跳转：内核 riscv64 内联 boot 路径在 `os/kernel/src/lib.rs:370-388`
  （`store_kernel_info` → `arch_boot_impl::<Riscv64Paging>` → `Riscv64HigherHalf::jump_to_kmain`）。
  `os/kernel/src/arch/riscv64/higher_half.rs:28` 定义跳转。`la` 在 riscv 上被链接器松弛为
  PC 相对寻址（`os/kernel-image/src/main.rs:83` 注释），这与 aarch64 §1.114 根因（PC 相对
  寻址把「高半跳转」留在低半）是同一家族的敏感点——riscv 内联形态的高半跳转语义必须在
  真机实测（见 §A.13）。
- **boot ELF 驱逐腿缺口（§1.110 同型，中招）**：共享装载体 `os/arch/src/arch/boot.rs:504-517`
  对「用户段虚拟地址落在仅监督可访问的身份大叶上」执行 `split_huge` → `grant_user_walk` →
  `unmap`。trait 默认实现返回 `NotSupported`（`os/arch/src/arch/paging.rs:357-358`、
  `:378-379`，文档明写 arm64/riscv64 需镜像 x86-64）；x86_64 有实现（`os/arch/src/x86_64/paging.rs:823/930`）、
  arm64 在 §1.110 补齐；`os/arch/src/riscv64/paging.rs` 全文（996 行，本轮通读）无这两个方法。
  riscv64 boot 身份映射用 1GiB 大叶（`paging.rs:725-726` `HUGE_PAGE_SIZE = 1<<30`），而 VM
  ELF 装载在低地址段，必然命中身份大叶覆盖 → `MappingFailed`。
- 链接脚本与直接映射窗口自洽：`os/arch/src/arch/direct_map.rs:147-153` riscv64 窗口常量
  （内核直接映射基址 `0xFFFF_FFC0_4000_0000`，规范形顶级项 257），注释 137-139 行记录了
  旧基址 `0xFFFF_FC00_0000_0000` 非规范、与镜像撞顶级项的修正史。

**判定：有缺口**（`split_huge`/`grant_user_walk` 未实现——但只影响 VM ELF 装载腿，boot
主干不撞）；高半进入语义需真机验（§A.13）。

## A.3 控制台早活

**aarch64 对应轮**：§1.114 的 MMIO 高半窗口修复（控制台跨切根存活）；§1.117 取证轮的
方法论教训（用户态 `console_write` 不落串口、diagctl 打印在洪流下不可靠）。

**riscv64 现状**：

- 内核侧早控制台存在，但实现是 **SBI 固件代管**而非 16550 直访（第 2 轮翻案第 1 轮的
  转述）：`os/plat/src/riscv64/early_console.rs:9-33`——每个字节经
  `sbi_ecall(SBI_CONSOLE_PUTCHAR=1, 0, byte)` 打出，OpenSBI 拥有串口并代管流控，因此
  没有 PL011 那类「不等发送寄存器空就写、长串丢字」问题（NK4B M3.3 对 PL011 补过
  有界等待节流，`1a2a8eb61`），也不需要在内核里配 16550 寄存器。**文档漂移**：
  `os/kernel-image/src/main.rs:52-57` 注释称 riscv64 早期控制台是「QEMU virt 的 16550
  UART（os/plat/src/riscv64/early_console.rs）」，与实现（SBI ecall）不符——登记为
  C 表文档漂移族一行（交主线程，本线程不修）。SBI 代管的边界：固件活着才可用；将来
  内核若要脱离固件直管 16550（地址 `0x10000000`），才涉及 MMIO 初始化与流控问题。
- 关机/退出通道存在：`os/plat/src/lib.rs:143-157`（sifive_test FINISHER，QEMU 退出）。
- **用户态控制台链断裂（给 rc marker 的 stdout 通路）**：TTY 驱动是用户态进程，串口访问
  走 `sys_inb/sys_outb` 内核调用（`os/drivers/tty/tty/src/serial.rs:25、72-76`，x86 的
  COM1 端口 0x3F8 形态）；而 riscv64 没有 I/O 端口指令这一概念，`os/plat/src/port_io.rs:20-21`
  注释明言「aarch64、riscv64 上这些方法是无操作返回 0，`SYS_DEVIO` 在分派层返回 BadCall」。
  也就是说 riscv 上 TTY 驱动能跑完协议流程，但每一条硬件访问都拿回 BadCall，字符永远出不了
  串口。这与 x86 上 §1.97 发现的「console 字符设备链未接线」不同层：协议层接线（§1.105）
  是架构中立的、已修；断的是硬件访问通道。riscv 的 16550 是内存映射寄存器（0x10000000），
  需要一条新的驱动侧硬件通道（见 §B D5）。

**判定：内核诊断通道已就绪；用户态串口通道有缺口（D5 决策项）**。

## A.4 platform 发现

**aarch64 对应轮**：§1.107（platform 源全灭即 panic）；§1.108（补 MADT GICR type-14 子表
解析——教训：不是放宽安全检查，是解析器少实现一个标准子表）。

**riscv64 现状**：

- 平台源在 boot-shim 侧组装：`os/boot-shim/src/opensbi_helpers.rs:222-232`——设备树指针
  非空则构造 `[DTB 源]`，否则空表（内核回退 QemuVirt 描述符）。
- 设备树解析有 riscv 分支：`os/libs/minix-platform/src/device_tree.rs:46-89`（riscv64 下
  解析出 `PlicDesc`/`ClintDesc`/`Riscv64ConsoleDesc`）；描述符族定义在
  `os/libs/minix-platform/src/arch/riscv64.rs`（头部注释声明分层：minix-boot 只见 trait）。
- 分派入口 `os/libs/minix-platform/src/kind.rs:49` `parse_by_kind`。
- 时钟与中断的实形态：时钟走 SBI TIME 扩展 ecall（`os/arch/src/riscv64/clock.rs:37-56`，
  注释带实测教训：CLINT 的 mtimecmp 在 OpenSBI 的 PMP 保护下从监督模式直访会吃访问例外，
  实锤 scause=5）；定时器门是 `sie.STIE`（`os/arch/src/riscv64/timer_irq_gate.rs:23-46`）；
  中断控制器是 PLIC 描述符。`os/arch/src/riscv64/arch_init.rs:38-46` 记录了另一条实测
  教训：监督模式写 PMP 寄存器直接非法指令（实锤 stval=0x3b051073），固件 owns PMP。

**判定：静态已就绪；设备树实解析与 PLIC/CLINT 实中断需真机验（§A.13）**。aarch64 的
教训（解析器少实现标准子表）在 riscv 的对应物是：设备树里 CPU 节点、中断扩展、内存节点
的每一类都必须有解析分支——本轮未逐节点审计 device_tree.rs 的覆盖度，登记为续轮项。

## A.5 VM 加载与帧池

**aarch64 对应轮**：§1.111 三连——`total_pages` 位图容量语义（位图按绝对页号寻址，容量必须
是地址空间最高页号而非记账累加）；EL0 发射特权 `tlbi`（用户态 VM 经 VmDm 通道调 `map` 时
`write_pte_dm` 无条件刷 TLB → 特权指令例外）；记账口径拆分（位图容量与 `vsi_total` 是两个量）。

**riscv64 现状**：

- `total_pages` 容量语义修复在 HEAD 且架构中立：`os/servers/vm/src/boot.rs:175-190`
  （validate 断言 total_pages == 最高空闲区页号），`:315` 按 free_regions 构造。riscv 的
  RAM 基址 `0x80000000` 比费尔米硬编码（见下）决定了起始页号 524288，恰好是这套「绝对页号」
  语义要处理的形态——修复已免疫。
- **帧池页表写通道的特权指令缺口（§1.111 同型，中招）**：`os/arch/src/riscv64/paging.rs:241-249`
  的 `write_pte_dm` 在每次写页表项后**无条件**执行 `sfence.vma`（监督特权指令），不区分
  访问通道。对比：aarch64 的修复是把刷新块门控在 `channel == PteChannel::KernelDm`
  （§1.111，对齐 x86 `invlpg` 门控先例 `os/arch/src/x86_64/paging.rs:188`）；riscv 的
  `PteChannel` 枚举（`paging.rs:197-203`，区分内核直接映射窗口与 VM 窗口）已经存在，
  `channel_to_ptr`（206-222 行）也在用它选窗口，唯独刷新指令没随通道门控。VM 服务器是
  用户态进程，经 VmDm 通道调 `map`/`remap`/`unmap`/`update_flags` 时这行 `sfence.vma`
  就是用户态非法指令例外——aarch64 §1.111 的第二个根因在 riscv 上原样存在。
- eager 物化收紧（§1.101）架构中立：`os/servers/vm/src/vm_server.rs:822`（file_pages 按文件
  尺寸算）、`:873`（逐页分配上界收紧）。
- VM 栈 runway（§1.95）架构中立：`os/arch/src/arch/boot.rs:405`（`VM_STACK_SIZE = 256KiB`，
  在共享的 `load_elf_into` 内，riscv 同享）。
- 内存图常量口径：`os/boot-shim/src/opensbi_helpers.rs:61-64`——`DRAM_BASE = 0x8000_0000`、
  `DEFAULT_RAM_SIZE = 0x800_0000`（128 MiB，与 QEMU virt 默认内存一致）；`:397-403`
  `build_memmap` 返回单区硬编码。主线程 x86/aarch64 真机习惯用 `-m 512M`：riscv 若沿用
  512M 启动，超出的内存会被这份硬编码内存图静默忽略（帧池只见到 128MiB）；见 §B D6。

**判定：有缺口**（`write_pte_dm` 的通道门控——单点修复，形状与 aarch64 §1.111 完全同构）；
帧池容量语义已免疫；内存图口径是 D6 决策项。

## A.6 原始 IPC 桥

**aarch64 对应轮**：§1.112——用户态六原语门控放宽到 `any(x86_64, aarch64)` + 内核
`aarch64_ipc_dispatch_body`（x86 桥的 EL1 镜像）。CodeReview W3 明确把 riscv64 **有意排除**，
理由：内核 `riscv64_user_body` 的原始 IPC 分支还答 `-ENOSYS`，先放宽用户门控只会把线上
错误号符号弄反（-38 对 +5）。

**riscv64 现状**：

- 用户态门控维持有意排除：`os/libs/minix-sys/src/ipc.rs:549-551`（send）至 `:652-654`
  （senda）六处 `#[cfg(all(any(target_arch = "x86_64", target_arch = "aarch64"), kernel_trap))]`；
  `:545-547` 注释原文点名 riscv64 被排除的原因链。riscv 上六原语落 `Err(EIO)` 短路。
- 陷入的用户半已就绪：`os/libs/minix-sys/src/arch_trap.rs:161-185` riscv64 `ecall` 体
  （含 §:145-148 契约注释：`ecall` 不自动步进 sepc，内核必须步过 4 字节，否则 sret 重捕
  同一条指令死循环）。
- 内核侧：`os/kernel/src/trap_dispatch.rs:1784-1804` `riscv64_user_body`——`ecall`（scause 8）
  已通：`:1790` 步进 sepc、`:1791` 读 a7（x17）分派、`:1792-1793` KERNEL_CALL 消息腿走
  `riscv64_kernel_call_leg`（`:1861-1891`，回复码写 a0）；其余原始 IPC 调用号在 `:1798-1799`
  答 `-ENOSYS`（常量 `ENOSYS_CODE = 38`，`:1718-1719`）。
- 文件头注释（`:1701-1706`）自证这是「registered gap」，与 aarch64 §1.112 前夜同形。
- **KERNEL_CALL 腿缺口一：负 errno 线上编码未迁移（第 2 轮新发现，NK4C prompt 的
  P1-arch/C2 项至今未完成）**。x86 腿已迁 `reply_wire()`（`trap_dispatch.rs:1618-1621`，
  注释自证「NK4-C F10b（P0-wire）：SYSCALL 腿线上取负由 reply_wire() 统一处理」）、
  aarch64 IPC 桥腿也已用 `reply_wire()`（`:2400`）；而 `riscv64_kernel_call_leg` 在
  `:1887` 仍用 `result.reply_code()`——`reply_wire()` 定义在
  `os/kernel/src/syscall.rs:244`（Ok→取负、Data→原样）。后果：riscv 上内核调用的
  负 errno 会以正数形态回给用户，`perform_syscall` 的 `m_type<0` 判错分支永不触发
  （`NK4C-REVIEW-REPORT-20260923.md` 把这条登记为「阶段 2/3 开工前必须修」的
  P1-arch，aarch64 半已在 §1.112 前后补掉，riscv 半遗留）。
- **KERNEL_CALL 腿缺口二：消息拷贝会撞 `sstatus.SUM`（第 2 轮新发现，NK4B M4.4
  载体侧补齐节的实锤）**。`riscv64_kernel_call_leg` 把用户消息指针交给
  `crate::ipc::KernelUserCopy`（`trap_dispatch.rs:1876-1883`），最终落到
  `os/kernel/src/ipc.rs:452`（`copy_msg_from_user`）/`:473`（`copy_msg_to_user`）——
  全内核仅这两处对用户虚拟地址直接 `read_volatile`/`write_volatile`（NK4B 逐行取证）。
  RISC-V 规则：监督态访问用户页（PTE U=1）必须 `sstatus.SUM=1`；生产代码无人置起——
  `INIT_USER_SSTATUS = 0x20` 只有 SPIE（`os/arch/src/riscv64/boot.rs:24`）、返回用户腿只动
  SPP/SPIE（`trap_return.rs:81-84`）、陷阱双腿存什么还什么（`trap_stub.rs:193/199-200/278/289-290`），
  `protection.rs:36` 还留着「sstatus.SUM is set later」的未兑现承诺。NK4B 的载体实测：
  `test-rt-birth-riscv64` 补上时钟初始化后前进一格，首个用户 ecall 的消息拷贝即
  `scause=0xd`（S 态载入页故障）死在 `copy_msg_from_user`；同载体自带的 SUM 开窗
  （`:248/:331/:336/:340`）因 `restore_to_user` 改写 `stvec` 而被生产腿顶掉，救不了
  生产拷贝路径。NK4B 列甲（粗粒度常开）/乙（拷贝窗口开关）/丙（VA→PA 走 DM 窗口，
  仓内 21 处既有先例）三案并推荐丙，**主线程 prompt 阶段 2.2/3.2 已裁丙案**；
  丙同时消灭 aarch64 PAN 与 x86 未来 SMAP 的同型问题。本项是三架构共享代码
  （`ipc.rs` 两个函数），属「公共腿」改动，排期归主线程。

**判定：有缺口（§B D4）**——缺内核 `riscv64_ipc_dispatch_body`（镜像 aarch64 版）＋
用户门控回开＋配套 `save_frame_to_context`/状态车道写回（aarch64 版在
`os/arch/src/arm64/trap_stub.rs`，riscv 侧 trap_stub 已有帧布局与分发注册，缺的是这两个
辅助函数与 park 决策 ABI，见 A.7）。

## A.7 阻塞 receive 与 park 模型（最高优先缺口集群）

**aarch64 对应轮**：§1.112 CodeReview Critical-1（阻塞腿从不再弹 288B 帧 → 栈棘轮踩 .bss）
与 §1.113 全轮——park 决策 ABI（`DispatchFn` 返回 `u64`：0=正常 eret，1=弹帧后进调度器）、
EL1h 单栈棘轮修复（每-CPU 栈基写入 `TPIDR_EL1`，返回用户态前重固定 sp）、throwaway 守卫
（`init_protection` 里 `with_protection` 会用 `init(0, VirBytes::new(0))` 造临时实例去够
`load()`，aarch64 曾因此把真实栈基覆盖成 0，修法＝arch 的 `init` 内加
`if kernel_stack_top.0 != 0` 哨兵）。

**riscv64 现状——三处同型位点，两处中招一处待接**：

1. **throwaway 覆盖 sscratch（§1.113 throwaway 陷阱的 riscv 版，中招，boot 必踩）**。
   证据链：
   - `os/kernel/src/lib.rs:1448-1451`：非 x86 的 `with_protection` 每次调用都现造临时实例
     `f(&CurrentProtection::init(0, minix_types::VirBytes::new(0)))`；
   - `os/kernel/src/lib.rs:1293` 真实初始化 `CurrentProtection::init(0, kernel_info.kern_stack_top)`
     先行、`:1304` `with_protection(|prot| prot.load())` 随后——即临时实例的 `init(0,0)`
     在真实 init 之后运行；
   - `os/arch/src/riscv64/protection.rs:110-120`：riscv 的 `init` **无条件**
     `csrw sscratch, kernel_stack_top`，没有 aarch64 §1.113 加的 `kernel_stack_top.0 != 0`
     哨兵。
   - 后果：`init_protection` 返回后 sscratch＝0。sscratch 在 riscv 上是用户态陷阱入口的
     栈交换寄存器（`os/arch/src/riscv64/trap_stub.rs:242` `csrrw sp, sscratch, sp`），
     值为 0 时第一次用户态陷阱会把 sp 换成 0、内核帧写到物理地址 0 附近——boot 死在
     第一个用户进程的第一条陷阱。全内核没有任何其它 sscratch 补写点（`set_kernel_stack`
     的调用者在 `os/kernel/src/` 下 grep 为零，唯一命中是 `os/kernel/src/globals.rs:174`
     的注释）。
2. **park 决策 ABI 未落（待接）**。aarch64 的分发 thunk 签名是 `-> u64`（PARK_NONE /
   PARK_RESCHEDULE，§1.113）；riscv 的分发类型仍是 `unsafe extern "C" fn(&mut Riscv64TrapFrame)`
   无返回值（`os/arch/src/lib.rs:442-445`），`riscv64_user_body`/`riscv64_kernel_body`
   均无 park 分支。阻塞 receive 在 riscv 上目前根本走不到（A.6 的 -ENOSYS 挡在前面），
   但 park 是 rc marker 的硬前置——INIT 等子进程、PM/VFS 事件循环全是阻塞 receive。
3. **switch-after-pop 后的栈与 sscratch 重固定缺失（aarch64 棘轮案的 riscv 版，待接）**。
   `os/arch/src/riscv64/trap_return.rs:55-134` 的 `restore_to_user` 只写 stvec、sepc、
   sstatus 并恢复通用寄存器，**完全不写 sscratch、也不重固定内核 sp**。aarch64 的对应
   修复是 eret 前 `mrs TPIDR_EL1; mov sp`（§1.113）。riscv 的模型里 sscratch 靠用户陷阱腿
   的两次 `csrrw` 自愈（`trap_stub.rs:242` 进入时 sp↔sscratch 交换、`:329` 返回时换回），
   但 park 路径恰恰**不经**返回腿的换回指令：分发体返回 PARK_RESCHEDULE 后弹帧进调度器，
   此时 sscratch 里停着的是上一个用户的 sp；调度器再 pick 到任何进程走 `restore_to_user`
   进用户态，下一次 U 态陷阱就会把内核帧建在 sscratch 里的陈旧用户栈上。也就是说：
   riscv 现在的「不变量由进出腿对称交换维持」只在「每次陷阱都原路返回」时成立，park 一落地
   即破。

**判定：有缺口（三件套：哨兵守卫、park ABI、restore 侧重固定）**。哨兵守卫是单点小修
（照抄 aarch64 §1.113 的 `!= 0` 哨兵），park ABI 是 D3 决策项的实施面。

## A.8 缺页路由（#PF→VM）

**aarch64 对应轮**：§1.116——aarch64 原本除 SVC 外全部当致命异常打死，补了取指/数据
中止分类、错误码合成（写位取自 ESR ISS[6] WnR 而非 DFSC 低位）、ForwardToVm 共享尾、
内核调 VmSuspend 臂；CodeReview BLOCKER-2 证明写位取错会让 COW 写车道被毁成无限重缺页。

**riscv64 现状**：

- 用户侧缺页（scause 12 取指缺页 / 13 载入缺页 / 15 存储缺页）在
  `os/kernel/src/trap_dispatch.rs:1803` 一律进 `riscv64_diag_panic`——`riscv64_user_body`
  只处理 scause 8（ecall）。文件头注释 `:1707-1710` 自证「faults: panic ... 是
  registered gap」。
- **写/读判别是 §1.116 BLOCKER-2 的同型疑点（条件不足，接线时必查）**：
  `os/arch/src/riscv64/exception.rs:67-73` 的 `is_write_fault` 用 `errcode & 1` 判写，
  注释自己承认「读写在 RISC-V 上是实现定义的、保守取低位」；但 RISC-V 的正确判别是
  scause 值本身（15＝存储缺页、13＝载入缺页、12＝取指缺页）。如果内核侧把 scause 塞进
  errcode（很可能，因为 riscv 陷阱帧没有独立错误码），则 13（读）与 15（写）的低位同为 1，
  **读缺页会被判成写**——按 §1.116 的教训，这会把 COW 共享页在首次读时就拷私有帧、反向
  场景把写误判成读走只读重映活锁。缺页路由腿落地时这里必须按 scause 重写，不能沿用。
- VmSuspend 臂：`riscv64_kernel_call_leg` 对无回复码结果直接 panic（`:1887-1890`），
  没有 aarch64 §1.116 第 4 项补的 VmSuspend 臂（x86 对位在 SYSCALL 腿）。

**判定：有缺口**（整条路由腿未落 + 写位判别方案须在落地时一并定对）。

## A.9 设备链接线（char open / CDEV 族）

**aarch64 对应轮**：§1.104→§1.105（CDEV_OPEN 线路类型号写成枚举索引 0、TTY 永不回复）。

**riscv64 现状**：协议修复架构中立且已在 HEAD——`os/servers/vfs/src/cdev.rs:170`
`m_type: CdevRequest::Open.message_type()`（附注释钉死「判别值是索引非线路号」）、
`:499-513` 回归测试钉住线路号与解码往返；全仓 grep `CdevRequest::<变体> as i32` 仅剩
`os/servers/vfs/src/main_loop.rs:3284` 一条注释（历史教训引用，非代码）。mfs 设备号
（§1.99）同样架构中立：`os/fs/mfs/src/server.rs:660` `child.zones[0]`（对位 C
`path.c:37` `fn_dev = i_zone[0]`）。

**判定：协议层已免疫；硬件访问通道有缺口（同 §A.3，D5）**。

## A.10 exec / newimage

**aarch64 对应轮**：§1.106（stat 回复从不 copy_out、magic grant 窗口 88/144 对 152 结构体
硬失败）、§1.89（fork 缺父侧 COW 写保护）。

**riscv64 现状**：三笔修复全部架构中立、已在 HEAD：

- grant 窗口统一 152：`os/servers/vfs/src/exec_worker.rs:82-85`（`STAT_BUF_SIZE =
  USER_STAT_SIZE`）、`os/servers/vfs/src/main_loop.rs:6873`、`os/servers/vfs/src/syscalls.rs:1174`；
  内核侧越界即整块拒绝的语义（工作日志 §1.106 记录 `os/kernel/src/grant.rs:452`）对本清单
  的意义是「窗口必须 ≥ 落字节量」，该约束由上面三处常量统一保证。
- 父侧 COW 写保护：`os/servers/vm/src/fork.rs` + `vmproc_handle.rs::protect_cow_pages`
  （§1.89 落地）不依赖任何架构特性；riscv 侧的前提是 `Paging::update_flags`（riscv 已实现，
  `os/arch/src/riscv64/paging.rs:664-680`）与 TLB 失效（`flush_tlb_addr`，:717-721）工作正常。
- 内核 `dispatch_exec` 只装上下文不建页表（工作日志 §1.91 读码链），地址空间安装走
  `vmctl_set_addr_space`——架构中立。

**判定：静态已就绪；COW 真回路（读触发拷贝、父写触发降级）需真机验（§A.13）**。

## A.11 runcom / rc

**aarch64 对应轮**：§1.117（reap 环 `.is_ok()` 误译 C 的 `waitpid(...) > 0`，`Ok(0)` 永真
→ 6 万次洪流）；§1.118 修复（SCHED `pick` 扫描界未钉 `processors_count`、选中幽灵 CPU）；
§1.118续/§1.119（fork EPERM 收敛到 switch-after-pop 家族的回复投递腿，build 布局敏感，未修完）。

**riscv64 现状**：

- reap 修复在 HEAD：`os/commands/sbin/init/src/runcom.rs:167`、
  `os/commands/sbin/init/src/single_user.rs:119`（`while matches!(host.waitpid(-1, WNOHANG),
  Ok((pid, _)) if pid > 0)`）、`os/servers/rs/src/trap_api.rs:471`。架构中立，riscv 免疫。
- 幽灵 CPU 修复在 HEAD：`os/servers/sched/src/cpu.rs:95` `loads.iter().enumerate().take(ncpus)`
  （扫描界钉在真实拓扑核数，对位 C `schedule.c:67`），架构中立。
- §1.118续/§1.119 的未决项（aarch64 上 PM 读回 SCHED 回复被间歇曲解成错误、对构建布局
  敏感）对 riscv 的投影：这套症状的嫌疑集中在 park-后-回复投递的寄存器写回（aarch64 是
  X0/X1 车道），riscv 的对应车道是 a0/a1——riscv 的回复写回点目前在 `riscv64_kernel_call_leg:1887`
  （写 `frame.gpr[10]`）与将来的 IPC 桥。**前置布防**：riscv 的 IPC 桥（A.6）与 park（A.7）
  落地时，直接按 aarch64 §1.113 CodeReview 的检查清单走一遍（BLOCKER-1 的硬编码偏移、
  状态车道寄存器、throwaway、恢复腿屏蔽），把 aarch64 已经交过的学费当验收单用。

**判定：静态免疫已核；可达性依赖 A.6/A.7 落地质量**。

## A.12 marker（rc 脚本 + echo/ls/cat）

**aarch64 对应轮**：§1.105（marker 首现）、§1.106（echo/ls/cat 全跑通）。

**riscv64 现状**：rc 脚本与命令播种（`os/xtask/src/image.rs` 的 `generate_etc_proto`）
属于装机面——riscv 装机腿整体缺位（A.1），所以本阶段对 riscv 的全部前置就是 A.0–A.11。

**判定：依赖全部前序阶段；无独立 riscv 缺口**。

## A.13 真机待验表（静态判不了、交给主线程择轮做）

| # | 待验项 | 为什么静态判不了 | 相关静态锚点 |
|---|---|---|---|
| 1 | 设备树实解析（CPU/中断/内存/串口节点 → PlicDesc/ClintDesc） | 解析代码存在但节点覆盖度未逐类审计，DTB 内容只有真机有 | opensbi_helpers.rs:222；device_tree.rs:46-89 |
| 2 | riscv 内联 boot 的高半跳转语义（`la` PC 相对 + 高半映射后 jump） | 与 aarch64 §1.114 同族敏感点，取决于链接/重定位实际结果 | kernel/src/lib.rs:370-388；kernel-image/src/main.rs:83 |
| 3 | sscratch 哨兵修复后的首个用户态陷阱 | 修复前的行为（帧写物理地址 0 附近）在模拟器上可能有隐蔽表现 | protection.rs:110-120；trap_stub.rs:242 |
| 4 | PLIC 外部中断与 CLINT/SSIP 的实投递（IPI 腿是注册缺口，单核 marker 不需要） | 中断路由只有真机/模拟器能证 | trap_dispatch.rs:1754-1756（SSI/SEI 注册缺口自证）；smp.rs:67-128 |
| 5 | COW 真回路（父子读写触发次数与帧数） | 页表 API 静态在、行为要跑 | fork.rs/vmproc_handle.rs；paging.rs:664-680 |
| 6 | 16550 早期控制台在内核态直访的输出质量（洪流下的丢弃率——§1.119 教训的 riscv 前置实测） | 丢弃率是运行时性质 | plat riscv64 early_console；clock.rs 的 PMP 实测教训预示 MMIO 面需逐点验 |
| 7 | DM 两个窗口在真机的实映射与 admissible 断言 | 编译期断言（opensbi_helpers.rs:369-375）只证常量，不证运行时窗口成立 | paging.rs:780-788（Riscv64DmCoverage）；direct_map.rs:147-153 |
| 8 | QEMU `-smp` 取值对 riscv 的正确配置（建议首轮单核，对齐 x86 §1.105 口径） | xtask riscv 分支尚不存在，配置时一次定对可避开 §1.118 教训 | qemu.rs:39/63（riscv 无分支） |
| 9 | riscv64 出生链载体的当前真实状态与 SUM 修复后的前进深度 | 载体已补时钟初始化（NK4B commit `630ea3398`）并前进一格，死在首个用户 ecall 的消息拷贝（scause 0xd，`copy_msg_from_user`）——SUM 丙案落地前任何更深推进都到不了 | NK4B-WORKLOG M4.4 载体补齐节（serial_clk1/clk2） |
| 10 | OpenSBI 递入的 `a1`（DTB @ 0x8fe00000，固件串口实证）在甲案接线后的实解析 | 生产入口零读取（本轮复核），载体先例（test-rt-birth-riscv64:426,493-501）只证同形可行 | NK4B M4.3 固件输出记录；kernel-image main.rs grep 零命中 |
| 11 | **exec 首进程取指（债⑩ 的 riscv 判别场景，第 5 轮补）**：散点 fence.i（higher_half.rs:54）只保 boot 转换腿，`load_elf_into` 写出的用户 text 帧在实机上是否需要 exec 腿的 fence.i 才能正确取指——这正是 marker 路径会先撞的场景 | QEMU 无缓存模型永不复现；实机判据；散点存在恰好把「boot 腿要 fence.i」钉成仓内事实，exec 腿同机理 | higher_half.rs:38-39/54；arch/boot.rs:565 |

---

# B. 设计决策点对比表（D1–D6 ＋ 第 2 轮新增 D7）

每个决策点给出不少于三个候选方案，每方案写：机制一句话、对位哪个已有案、代价与风险、
与「公共腿不动、架构层进 trait」铁律的相容性。表末给推荐序。**架构裁决归用户拍板，
本表只供决策**——这是任务书铁律，也是本项目「架构裁决级决策停下问用户」的既定纪律
（工作日志 §1.93 先例）。

## D1 装机与交棒形态

**候选 1：U-Boot fatload + BootFileTable + boot-shim 跳板（完整装机形）**
机制：U-Boot 把内核 ELF 与全部 boot 模块用 `fatload` 预载进 RAM，把文件表物理地址放在 `a0`，
boot-shim 入口读表、用 `UbootFileLoader` 组装 KernelInfo 与 boot 模块清单，再进内核。
对位：`OpenSbiBootShim::prepare_boot`（opensbi_helpers.rs:193-267）就是为它写的；Linux 的
U-Boot 启动面（bootm/bootefi）与本项目 UEFI 盘形的语义对应物；Minix3 无 RISC-V 移植、无
对应物。代价/风险：缺入口二进制（A.1 缺口 1）；U-Boot 脚本链（boot.scr/mkimage）是把
QEMU 载体扩成生产链的额外工程面；BootFileTable 的 magic/版本自校验沿用 §1.115 CONSIDER-4
的教训。铁律相容：好——全部改动在 boot-shim（装机层），公共腿不动。

**候选 2：OpenSBI 直接作为上一级，内核 ELF 作下一级（fw_jump/fw_dynamic 形）**
机制：OpenSBI（M 态固件）直接跳内核 ELF 入口（S 态），`a1` 递设备树；Linux RV 的标准
启动面（Image 头 + 设备树约定）；本仓已有 `fw-riscv64-none` 特性词与
`test-riscv64-uboot.sh` 实证的 bootelf 形。代价/风险：boot 模块（12 台服务的 imgrd）没有
自然的装载通道——要么内嵌内核镜像、要么 `-initrd` 语义自造，与 UEFI 盘形语义渐行渐远；
KernelInfo 的 boot_modules 载荷来源需新设计。铁律相容：好。

**候选 3：纯 `-kernel` 直载最小化验证（QEMU `-bios opensbi -kernel kernel.elf`，无 U-Boot）**
机制：载体测试现形——qemu-tests 全族已这么跑（hello-boot 头注释「loaded via -kernel」），
把生产内核按同形态载入，先打通 A.2–A.11 的全部内核侧行为再回头补装机。
对位：载体矩阵现状；aarch64 侧当年也是先有内联形态再有方案 A（§1.114→§1.115 的顺序）。
代价/风险：模块来源同样悬空（比候选 2 更裸）；marker 之前若无模块装载，得用最小 init 镜像
或裁剪 boot 清单；真实装机面继续欠账。铁律相容：最好（零新装机代码）。

**推荐序：3 → 2 → 1**（先用候选 3 把内核侧行为全部跑实、把 marker 打出来；模块装载语义
拍板后再决定 1 或 2 作为终态装机形）。标注：此序只反映「最快暴露内核侧未知数」，装机
终态选 1 还是 2 是用户裁决。**第 2 轮补充：主线程既定路线已裁甲案**——
`NK4C-OPENING-PROMPT.md` 阶段 3.1 明文「kernel-image riscv64 接 a1 DTB → 解 memmap +
模块装载源 + .bss 清零 → 调 arch_boot（过 validate 真门槛）」，即「kernel-image 自当
引导体」的形态（接近候选 2/3 的混合、不需要 boot-shim 组装）。本表保留作对照与代价
分析用，续轮以甲案为基线对账。

## D2 高半模型（单 satp、无 TTBR1 的世界）

**候选 1：每进程根继承内核高半（x86 `inherit_supervisor_half` 模式）**
机制：每个进程根建好后，把根级页表的内核半（Sv39 下即第 256..512 顶级项）原样拷贝，
内核高半在所有地址空间常驻。
对位：riscv **已经实现**——`os/arch/src/riscv64/paging.rs:486-505`；x86 同名方法先例。
代价/风险：每根 256 项拷贝的一次性成本；用户虚拟地址必须保持在 2^38 以下（Sv39 低半），
用户堆栈布局与 x86/aarch64 的 0x00007fff… 形态不同，任何「高地址用户栈」假设要审计
（riscv 的用户栈在低半高地址区，`arch/boot.rs:597-606` 的 stack_high 计算）。
铁律相容：已在 trait 里，零新增。

**候选 2：独立 ELF 跨镜像 handoff（aarch64 方案 A 模式，BootHandoff）**
机制：boot-shim 只建表+开分页，把 KernelInfo 序列化进物理 blob，绝对跳进独立链接的高半
`kernel.elf` 的 `_start`，后者 additive 续跑。
对位：§1.115 全轮；`os/libs/minix-boot/src/handoff.rs` 契约已是 `#[repr(C)]` 架构中立；
riscv64.ld 的独立内核镜像契约位已产出。代价/风险：**riscv 收益远小于 aarch64**——aarch64
需要它是因为 TTBR0/TTBR1 双根且内核从未进 TTBR1；riscv 单根 + 高半继承（候选 1）已经
天然免疫切根，handoff 只解决「boot 期高半进入」一件事。另外 §1.115 债① 已登记双 boot
形态并存的维护面代价 ×2；§1.115 增量 2 的「不得清零正激活的根」坑（`new_from_page`
会清零根页，riscv 的 `paging.rs:455-459` 同样在 `new_from_page` 里 `write_bytes(ptr,0,512)`）
在 riscv handoff 化时会原样复现。

**候选 3：维持现 HEAD 形态（内联 arch_boot + `jump_to_kmain` 高半跳转）**
机制：boot-shim 内联内核代码，`arch_boot_impl` 建身份+高半映射后 `jump_to_kmain`。
对位：x86 现行形态（riscv 与 x86 同在 `main.rs:82` 注释的「keep the inlined arch_boot
path」组）。代价/风险：`la` 的 PC 相对寻址语义使高半跳转的落点依赖链接与重定位实际结果
（A.13 待验项 2）；若真机实测跳转落点正确，此形态零改动即工作。

**符号扩展位与规范形审计（本决策点的公共底座）**：Sv39 的「高半」＝第 38 位为 1 且第 63..39
位随符号扩展，规范高半起点 `0xFFFFFFC000000000`（direct_map.rs:133-134、riscv64.ld 注释
21-29 行带两个非法值警示）。全仓 x86 的 BIT(63)/TTBR1 假设、aarch64 的 TTBR1 半假设，在
riscv 对应「顶级项 256..512」这一份常量族；本轮实测的三处基址（direct_map.rs:149、
riscv64.ld、paging.rs:492）自洽，且 direct_map.rs:137-139 记录了非规范旧值的修正史——
「规范形审计」在常量层已过，续轮只需对照 `inherit_supervisor_half` 的位区间。

**推荐序：3（现形态，若 A.13 待验项 2 实测通过）→ 1（继承已实现，切根免疫随候选 3 自动
成立）→ 2（仅当 boot 期高半进入实测失败才升级）**。标注：裁决归用户。

## D3 陷阱栈与 park 模型（单 sp 上下文的 switch-after-pop）

**候选 1：sscratch 交换 + park 决策 ABI（aarch64 §1.113 案的 riscv 镜像）**
机制：分发 thunk 签名改 `-> u64`（PARK_NONE/PARK_RESCHEDULE），用户腿尾声在 `bl` 后按
返回值决定「弹 34 槽帧后 sret」还是「弹帧后跳 resched 入口」；同时在 `restore_to_user`
里**重固定 sscratch 与内核 sp 到每 CPU 栈基**（riscv 的每 CPU 栈基寄存器候选：sscratch
本身在返回用户态后由返回腿的 `csrrw` 恢复持有内核栈顶——但 park 路径不经返回腿，所以
restore 侧必须显式写；寄存器选型见下方坑位说明）。
对位：aarch64 §1.113 全套（park ABI + TPIDR 棘轮 + daifset 屏蔽）；x86 的 `reenter_scheduler`
同形。代价/风险：asm 腿改动（34 槽帧布局已冻结并测试钉住，`trap_stub.rs:366-374`）；每 CPU
栈基需要一个可靠来源——aarch64 用 TPIDR_EL1，riscv 可用 sscratch 在 S 态的语义
（restore 前它是「上一个用户的 sp」，不可用）或引入每 CPU 变量（`current_cpu_id()` 的
既有通道）；中断屏蔽对应物＝`sret` 前关 `sstatus.SIE`（恢复由 sret 从 SPIE 还原）。
铁律相容：arch 层改动，公共腿不动。

**候选 2：x86 TSS.sp0 案（硬件重装栈）**
机制：特权级切换时硬件自动从 TSS 装内核栈。对位：x86 唯一。**对 riscv 不适用**——RISC-V
没有特权切换自动换栈的硬件机制，sscratch 软件交换就是本架构的规范做法（protection.rs
模块文档 41-51 行的对比表已论证）。列出仅为对照完整性。

**候选 3：先 mock（阻塞 IPC 继续显式 panic，park 延后）**
机制：IPC 桥接通（D4）但阻塞腿照 aarch64 §1.112 Critical-1 的临时形态返回可见 panic。
对位：aarch64 §1.112 的过渡态。代价/风险：rc marker 不可达（INIT/PM/VFS 事件循环全是
阻塞 receive），只能推进到「内核接收引擎活、阻塞即停」的 aarch64 §1.112 终态。
铁律相容：零改动（现状即是）。

**无论选哪个，A.7 第 1 项（throwaway 哨兵）都必须先做**——它是 boot 必踩点，且与 park
选型无关。

**推荐序：先哨兵 → 候选 1**（候选 3 只作为 IPC 桥与 park 之间的临时排期缓冲）。标注：
裁决归用户。

## D4 原始 IPC 桥接通路径

**候选 1：镜像 aarch64 案（新写 `riscv64_ipc_dispatch_body`）**
机制：照 §1.112 的 `aarch64_ipc_dispatch_body` 逐段镜像（bkl 锁继承 → 存帧 → FullContext →
`IpcCall::from_raw` 预解码 → bodyless 集不拷消息 → 内核用户拷贝 → `dispatch_ipc_entry` →
回复码写 a0、状态车道写 a1），配套 riscv 版 `save_frame_to_context`/
`sync_status_register_to_frame`（帧槽位映射：a0=gpr[10]、a1=gpr[11]，`Riscv64TrapFrame`
槽位即寄存器号，比 aarch64 的偏移表简单），用户态门控回开为
`any(x86_64, aarch64, riscv64)`。
对位：aarch64 §1.112 全轮，CodeReview 四条结论（尤其 W3 的「门控范围＝已支持架构」纪律）
直接可用。代价/风险：中等；ecall 的 sepc 步进契约（arch_trap.rs:145-148）已在内核腿
处理过（trap_dispatch.rs:1790），镜像时保持。铁律相容：arch 层 + kernel trap_dispatch
的 riscv cfg 块，公共 decoder（`IpcCall::from_raw`、`dispatch_ipc_entry`）不动。

**候选 2：复用公共 decoder、只补架构壳（把 dispatch body 提升为架构中立 helper）**
机制：像 §1.116 那样把可架构中立的纯 helper 抽到 cfg(any(arch, test)) 层，三架构共用。
对位：§1.116 的 `classify_abort_disposition` 先例。代价/风险：重构 aarch64 刚落地的公共
腿，与主线程「公共腿每轮在动」的节奏冲突（本线程任务书铁律 1 的冲突代价已被多工作树
纪律反复确认）；建议作为第二阶段而非接通路径。铁律相容：方向正确但时机需主线程裁。

**候选 3：先 mock 后真机（维持 -ENOSYS）**
机制：现状。对位：aarch64 §1.112 之前的状态。代价/风险：VM 服务器主循环 `receive` 拿到
EIO×64 后主动 panic（§1.111 末观察到的形态）——riscv 连 VM 主循环都进不了实质阶段。
铁律相容：无改动。

**推荐序：候选 1**（并明确 riscv 落地时把 aarch64 §1.113 的 CodeReview 清单当验收单：
偏移硬编码、状态车道、throwaway、恢复屏蔽四项逐一过）。标注：裁决归用户。

## D5 证据通道（§1.119 教训的前置布防）

**候选 1：16550 早期控制台 + 内核诊断调用（diagctl 直写）+ IPC 回复码回传分区**
机制：内核诊断沿用 plat 的 riscv 早控制台；「哪条腿/哪个值」的关键取证一律编码进 IPC
回复的 m_type 数值回传（§1.118续 方法论：串口打印在洪流下非确定性丢弃，回复码通道每轮
稳定），并在服务器侧用 `sys_diagctl_write` 作辅助。
对位：aarch64 §1.118续 确立的可靠通道纪律。代价/风险：零新代码（方法纪律）。
铁律相容：无关代码。

**候选 2：用户态 TTY 的 MMIO 硬件通道（A.3/A.9 缺口的修复设计）**
机制：三选一——(a) 内核把 `SYS_DEVIO` 在 riscv 上实现为受权限检查的 MMIO 代写
（按 C 的 devio 语义、把端口号换 MMIO 地址表）；(b) TTY 驱动启动时经 VM 映射 16550 的
MMIO 窗口到自身地址空间直访（riscv 用户态可访 MMIO 需 PTE 无 U 位限制的绕行或 PMP/委托
设计，标准做法是内核代写）；(c) 内核直接把早期控制台作为/dev/console 的后备
（简化但偏离 C 的 TTY 服务语义）。
对位：C 在 x86 用 IOPL 让 TTY 直访端口；riscv 无 IOPL 概念（riscv64/boot.rs:95-97 注释
自证），必须新设计——**这是 riscv 版 §1.97 那条「console 链未接线」教训的正面形态**。
代价/风险：(a) 最贴 C 语义但要在内核分派层做地址白名单；(c) 最快但语义欠账。
铁律相容：plat/arch 层新增通道 + 内核分派分支，公共协议不动。

**候选 3：内核侧进程生死记账（非串口通道）**
机制：内核对 child exec/exit 的关键事件做计数或环形记账（§1.119 建议的形态），取证时
经 diagctl 或回复码批量读出。
对位：§1.119「非串口可靠记账」建议。代价/风险：内核记账面新增（探针纪律敏感——committed
码不留探针是既定纪律，记账须做成正式诊断设施而非 TEMP 探针）。
铁律相容：需要作为正式诊断设施立项，不是顺手加打印。

**推荐序：候选 1 立即生效（纪律）→ 候选 2 在 A.3 排期时设计拍板 → 候选 3 视取证需要
立项**。标注：裁决归用户。

## D6 帧池与位图容量口径（riscv RAM 基址代入）

背景：§1.111 的容量语义修复已在 HEAD（A.5），位图按绝对页号寻址且容量断言为最高空闲区
页号。riscv 特有的问题是**内存图本身从哪来**：boot-shim 的 `build_memmap` 硬编码
QEMU virt 单区 128 MiB（opensbi_helpers.rs:61-64、397-403）。

**候选 1：解析设备树的 memory 节点作为真值源**
机制：DTB 里 `/memory@80000000` 的 reg 属性就是真实内存，OpenSBI 透传的 DTB 与 QEMU
`-m` 一致；`device_tree.rs` 已有解析框架，加一类节点解析。
对位：aarch64 的 ACPI MADT/DTB 双源纪律；候选与 `platform_sources` 组装点同侧。
代价/风险：设备树解析边界（节点缺失/多个 memory 区）要定规则；与候选 3 的 QEMU 默认值
一致性在测试里可断言。铁律相容：boot-shim 层。

**候选 2：抬常量对齐主线程习惯（-m 512M）**
机制：`DEFAULT_RAM_SIZE` 改 512 MiB，qemu.rs 的 riscv 分支补 `-m 512M`。
对位：x86/aarch64 真机现行口径。代价/风险：常量与 QEMU 实际 `-m` 脱节时（换机器/换参数）
又回到「内存图说谎」——这正是 §1.101 教训「侥幸内存映射掩盖真问题」的温床；帧池位图
按 512M 容量建、实际 QEMU 给 128M 时位图尾部页号越界会被 boot.rs:188 的断言拦下（好），
但反向（QEMU 给大、常量小）静默浪费（坏）。

**候选 3：维持 128 MiB（QEMU virt 默认）并约束调用习惯**
机制：常量不动，主线程 riscv 轮一律 `-m 128M`。对位：QEMU virt riscv 默认。代价/风险：
12 台服务 + 帧池 + 页表页池的预算在 128MiB 里是否够，只有真机能证；§1.101 的教训是
「内存余量临界时启动表现为第 8 台崩溃」这类形态，排查成本高。
铁律相容：三者均不动公共腿。

**推荐序：候选 1（真值源，一次性消除说谎面）→ 候选 3（保底，无代码改动）→ 候选 2
（仅在候选 1 的解析边界拖慢排期时作短期桥）**。标注：裁决归用户。

## D7 内核访问用户内存的通道（sstatus.SUM / aarch64 PAN / x86 SMAP 同族，第 2 轮新增）

背景：NK4B M4.4 载体侧补齐节实锤——生产内核只有 `ipc.rs:452/:473` 两处直接以内核地址
访问用户 VA（消息拷贝），riscv 上因 `sstatus.SUM=0` 必然页故障；NK4B 列三案并推荐丙，
主线程 prompt 阶段 2.2/3.2 已裁丙案。此处记录为独立决策点，供续轮对照实施面。

**候选 1（NK4B 案甲，粗粒度常开）**：`INIT_USER_SSTATUS` 加 SUM 位、`restore_to_user`
应用到 sstatus。机制一句话：当前进程是用户进程时内核全程可读用户页。对位：x86 未启
SMAP 的现状。代价/风险：粒度最粗，放弃硬件护栏；与「先软走页表再访问」的既有纪律相悖。

**候选 2（NK4B 案乙，拷贝窗口开关）**：给 `UserCopy` 加 arch 门控的开/关 guard，拷贝窗口
内 `csrs sstatus, SUM`。对位：Linux `enable_user_access()`；仓内载体
`test-rt-birth-riscv64/src/main.rs:248,331,336,340` 已有同位窗口模式可提升。代价/风险：
新增一层 kernel↔arch 抽象并为三架构各写一份；载体副本需剔除防双份。

**候选 3（NK4B 案丙，VA→PA 走 DM 窗口，已裁）**：`copy_msg_from_user`/`copy_msg_to_user`
改走 `resolve_physical` → `kernel_phys_to_virt` → 拷贝（`kernel/src/vm.rs:388-404` 标准三步，
仓内 `cross_space::` 21 处实际调用全走此路）；需处理 64 字节消息跨页分段
（`vm.rs:249` `lookup_range_in_table` 现成）与 DM 覆盖校验（`vm.rs:335` 现成）。
对位：本仓跨空间拷贝的既有纪律。代价/风险：只改两个函数（含分段循环）；用户页必须被
DM 窗口覆盖（boot DM 覆盖在 riscv 已有真机通过证据：`serial_clk1.log` 的
`step4 DM coverage ok`）。

**推荐与裁决状态**：NK4B 推荐丙、主线程 prompt 阶段 2.2/3.2 已裁丙案（riscv64 与
aarch64 共用同一次改动）；本表无异议，仅补一条实施提醒——丙案落地后，A.7 的 park
腿与 D4 的 IPC 桥都自动受益（它们同走 `KernelUserCopy`）。

---

# C. 坑对账表（x86/aarch64 已付学费 → riscv64 同型核查）

三列映射：踩坑轮次＋根因一句话 → riscv64 代码里同型位点实证（本轮读到的文件:行）→ 结论。
「已免疫」＝架构中立修复已在 HEAD，riscv 自动继承；「中招」＝riscv 上存在同型缺陷或
同型缺口（含有意登记的 registered gap）；「条件不足」＝缺陷形态依赖尚未落地的代码，
接线时必须按教训做。

| # | 学费（轮次 + 根因） | riscv64 同型位点实证 | 结论 |
|---|---|---|---|
| 1 | §1.113 BLOCKER-1：OS 层硬编码寄存器偏移 80 写 IPC 返回码，aarch64 落到 X7 | `grep write_user_register(.*, 80,` 在 `os/kernel/src/` 零命中；riscv 的 trait 实现 `set_ipc_return_reg`/`ipc_return_reg` 已存在（`os/arch/src/riscv64/boot.rs:159-168`，写 `ctx.a0`） | 已免疫 |
| 2 | §1.107：架构门缺失（x86 专属 `pic_init` 未门控）致跨架构 17 编译错 | plat 侧 riscv 分支齐全：`os/plat/src/lib.rs:86-87`（TIMER_IRQ）、`:119-120`（中断控制器）、`:129-130`（早控制台）；生产配方本轮编译通过（§A.0） | 已免疫（注意配方敏感性，§A.0） |
| 3 | §1.112：六原语门控范围＝已支持架构；riscv 有意排除 | `os/libs/minix-sys/src/ipc.rs:549-654` 六处 `any(x86_64, aarch64)`；内核侧 `-ENOSYS`：`os/kernel/src/trap_dispatch.rs:1798-1800` | 中招（有意 registered gap，解法＝D4 候选 1） |
| 4 | §1.111：EL0 通道发射特权 `tlbi` → 用户态同步例外 | `os/arch/src/riscv64/paging.rs:248` `write_pte_dm` 无条件 `sfence.vma`（监督特权指令），`PteChannel` 已存在但刷新不随通道门控；x86 先例门控在 `x86_64/paging.rs:188` | 中招（VM 用户态调 map 即非法指令；单点修复，形状同 §1.111；**代刷腿缺口与修复方案见 §D.12——内核 vmctl 腿已在，缺的是门控与 VmDm 通道的语义闭合**） |
| 5 | §1.110：`split_huge`/`grant_user_walk` trait 默认 NotSupported，boot ELF 驱逐腿 MappingFailed | `os/arch/src/arch/paging.rs:357-379`（默认）；`os/arch/src/arch/boot.rs:504-517`（驱逐腿必经）；`os/arch/src/riscv64/paging.rs` 无实现；riscv 身份映射用 1GiB 大叶（paging.rs:725-726） | 中招（VM ELF 装载腿必撞；boot 主干不撞） |
| 6 | §1.116：缺页不路由 VM、当致命异常打死 | `os/kernel/src/trap_dispatch.rs:1803` 用户侧非 ecall 一律 diag_panic；`:1707-1710` 注释自证 registered gap | 中招（有意缺口） |
| 7 | §1.116 BLOCKER-2：WnR 写位取自错误字段 → COW 写车道被毁、无限重缺页 | `os/arch/src/riscv64/exception.rs:67-73` `is_write_fault` 用 `errcode & 1` 且注释自认「实现定义」；RISC-V 正确判别＝scause 12/13/15 | 条件不足（缺页路由落地时必须按 scause 重写判别，见 A.8） |
| 8 | §1.113：throwaway 实例把真实栈基覆盖成 0 → 首个用户陷阱静默 data abort | `os/kernel/src/lib.rs:1448-1451` throwaway `init(0, 0)` 对 riscv 一样触发；`os/arch/src/riscv64/protection.rs:110-120` `init` 无 `!= 0` 哨兵、无条件 `csrw sscratch`；`set_kernel_stack` 全内核零调用者 | 中招（boot 必踩、最高优先；修复＝照抄 aarch64 §1.113 哨兵） |
| 9 | §1.113 SF4：per-CPU 寄存器所有权冲突（TPIDR_EL1 被两主争用） | `os/arch/src/riscv64/smp.rs:224-247` `current_cpu()` 把 sscratch 当 hart 号读；而 protection.rs:110-142 与 trap_stub.rs:242/329 把 sscratch 当内核栈顶交换寄存器 | 中招（一个 CSR 两个所有者；接线 SMP 或修 current_cpu 前必须裁决所有权） |
| 10 | §1.117：`.is_ok()` 误译 C `waitpid(...) > 0`，`Ok(0)` 永真洪流 | 修复在 HEAD 且架构中立：`runcom.rs:167`、`single_user.rs:119`、`trap_api.rs:471` 均 `pid > 0` | 已免疫 |
| 11 | §1.118 修复：SCHED pick 扫描界未钉真实核数、选中幽灵 CPU 致 EINVAL | `os/servers/sched/src/cpu.rs:95` `take(ncpus)`（扫描界夹在真实拓扑内） | 已免疫 |
| 12 | §1.105：CDEV_OPEN 线路类型号写成枚举索引 0、TTY 永不回复 | `os/servers/vfs/src/cdev.rs:170` 用 `message_type()`；回归钉 `:499-513`；全仓 `as i32` 残留仅注释（main_loop.rs:3284） | 已免疫 |
| 13 | §1.99：mfs lookup 设备号硬编码 0、console open 查表键错 | `os/fs/mfs/src/server.rs:660` `child.zones[0]`（对位 C `path.c:37`） | 已免疫 |
| 14 | §1.106：stat 回复从不 copy_out；magic grant 窗口小于结构体即整块 EPERM 硬失败 | 三处窗口统一 `USER_STAT_SIZE`（=152）：`exec_worker.rs:82-85`、`main_loop.rs:6873`、`syscalls.rs:1174`（grant 边界检查本体在 `os/kernel/src/grant.rs`，本轮未逐行复核，登记续轮顺验） | 已免疫（窗口常量层） |
| 15 | §1.79/1.80：exec ps_str 走 i32 截断 + 符号扩展 → 子进程悬空指针 | `grep "ps_str: i32\|newps_str: i32" os/ --include=*.rs` 零命中 | 已免疫 |
| 16 | §1.101：eager 物化按 memsz 逐页取帧 ×12 台撑爆帧池 | `os/servers/vm/src/vm_server.rs:822/873` `file_pages` 收紧在 HEAD | 已免疫 |
| 17 | §1.111：total_pages 位图容量语义（绝对页号寻址） | `os/servers/vm/src/boot.rs:175-190`（validate ＝最高空闲区页号）、`:315`（构造） | 已免疫（riscv RAM 基址 0x80000000 正是该语义的目标形态） |
| 18 | §1.119：串口证据在洪流下非确定性丢弃，不可作为「走了哪条腿」判据 | riscv 尚无洪流先例，但用户态串口通道本身断（A.3）＋早控制台是唯一输出，§1.118续 方法论（回复码回传分区）应作为 riscv 取证的启动纪律 | 布防项（D5） |
| 19 | §1.95：VM boot 栈 runway 不足 → 抬常量即崩 | `os/arch/src/arch/boot.rs:405` `VM_STACK_SIZE = 256KiB` 在共享装载体内，riscv 同享 | 已免疫 |
| 20 | §1.89：fork 缺父侧 COW 写保护 → 子读父写竞态腐蚀 | `os/servers/vm/src/fork.rs`/`vmproc_handle.rs::protect_cow_pages` 架构中立；riscv 侧 API 前提（`update_flags` paging.rs:664-680、`flush_tlb_addr` :717-721）已实现 | 已免疫（真机验前提见 §A.13 项 5） |
| 21 | §1.96：DeliverMsg suspend 的 target 传成发送方端点 → VM 映错地址空间 | `os/kernel/src/proc_table.rs:1311-1320` 修复在 HEAD（target = receiver 自身端点，附 C 对位注释），架构中立 | 已免疫 |
| 22 | §1.115 CONSIDER-5：数据→代码切换缺缓存维护（DC CVAU/IC IALLU） | riscv 对应物＝`fence.i`（指令流同步）；当前 riscv 装机腿未落，装机轮设计时须对照登记 | 条件不足（M4.3 设计清单项） |
| 23 | §1.113 SF2/§1.116：恢复序言的中断屏蔽与陷阱分类常量命名 | riscv park 未落地；落地时的对应物＝`sret` 前 `sstatus.SIE` 屏蔽（恢复由 SPIE 语义自动完成），分类常量族按 scause 建（12/13/15 与 8 分开） | 条件不足（D3/D4 实施清单项） |
| 24 | NK4C F10b（P1-arch）：x86 腿迁 `reply_wire()` 后，aarch64/riscv SYSCALL 腿未迁，负 errno 线上编码不一致 | `os/kernel/src/trap_dispatch.rs:1887` riscv 腿仍 `reply_code()`；x86 `:1621` 已 `reply_wire()`（F10b 注释在案）、aarch64 桥腿 `:2400` 已迁；`reply_wire` 定义 `syscall.rs:244` | 中招（NK4C prompt C2 项至今未完成，单点迁移＋判别测试） |
| 25 | NK4B M4.4：内核读用户页需 `sstatus.SUM`，生产侧无人置起——riscv 首个内核调用消息拷贝即 S 态页故障 | `os/kernel/src/ipc.rs:452/:473` 两处直访用户 VA；`INIT_USER_SSTATUS=0x20`（riscv64/boot.rs:24）、`trap_return.rs:81-84`、`protection.rs:36` 未兑现承诺；载体实测 scause 0xd 死于 `copy_msg_from_user` | 中招（三案已裁丙：VA→PA 走 DM 窗口，见 §B D7） |
| 26 | NK4B P1（x86_64）：IPC 状态寄存器选了 callee-saved 的 RBX，内核窗口外写毁用户活值，三修法上交裁决 | riscv 的状态车道选 a1（调用者保存的参数寄存器）：wrapper `inlateout("a1") a2 => status`（`minix-sys/src/arch_trap.rs:164-172`）＋ `or_ipc_status_reg` 写 `gp_regs[GP_A1]`（riscv64/boot.rs:146-157）——用户代码本就不指望 a1 跨陷阱存活 | 已免疫（选型层；a1/a0 双车道分工在 trap_stub 帧里天然成立） |
| 27 | R3 F-SELF-1②：1.12d WIP 硬编码 SENDA 槽步长 80，与同文件 size 断言 96 自相矛盾（错位读＋越界），是 B7 启动死锁真根因 | 修复由续跑方 B7 落地：步长改 `size_of::<WireAsyncSlot>()` 派生（架构中立，R3 报告 §二 F-SELF-1 在案） | 已免疫 |
| 28 | NK4B M4.4 事实五：kernel-image 三架构入口不清 `.bss`，而 `IMAGE_ALLOCATOR`（含分配游标）住在 `.bss`——OpenSBI 交接不清 BSS，垃圾读出假就绪 | `os/kernel-image/src/main.rs` riscv 入口序列无清零循环（本轮复核 grep 零命中）；载体先例 `test-rt-birth-riscv64` 入口带清零（K10 round-1 finding 注释自证） | 中招（甲案共同前置，NK4B 已登记待修＋判别性断言缺失未解） |
| 29 | NK4B M4.3 评审登记：U-Boot `bootelf` 按 `e_entry` 跳转，生产镜像入口是高半 VA，`satp=0` 下直跳即陷 | 生产镜像 `e_entry = 0xFFFFFFC000000000`（`riscv64.ld` + M4.2 布局断言）；hello-boot 载体能过只因平坦入口 | 条件不足（U-Boot 腿接通时按三方向裁决；主线程甲案路线下此腿降级为对照格） |
| 30 | NK4B M4.4 旁支：C 引用漂移族（六组登记不修）——含 riscv64/smp.rs 5 处引用不存在的 `minix3/minix/kernel/arch/i386/smp.c`（真身在 `arch_smp.c:357`） | `os/arch/src/riscv64/smp.rs:25/34/36/61/182` 本轮 grep 复核 5 处命中 | 已登记（文档级；逐条改需独立任务，NK4B 建议先符号化锚点再议 anchor-resolve 扫 .rs） |
| 31 | 本线程第 2 轮新发现（文档漂移族）：`kernel-image/src/main.rs:52-57` 注释称 riscv 早期控制台是 16550 MMIO，实现实为 SBI console_putchar ecall | `os/plat/src/riscv64/early_console.rs:9-33` 逐字节读毕——`sbi_ecall(SBI_CONSOLE_PUTCHAR=1, 0, byte)`，全文无 16550/MMIO | 已登记（文档级，一行注释对齐；顺带修正本文件 §A.3 第 1 轮的同一转述） |
| 32 | PATTERN-SCAN P8（CRLF 入库族）：aarch64.ld CRLF 第 1 发、riscv64.ld 第 2 发（P8 首跑实测在案）、handoff.rs 第 4 发 | riscv64.ld 在 P8 基线内（`git ls-files --eol` i/crlf）；链接器容忍所以构建全绿，肉眼看不出 | 已登记（基线拦新增；建议 NK4-C 收线时统一转 LF） |

## C 表的汇总结论（第 2 轮更新）

1. **架构中立修复的继承率是满分**：C 表 32 行里 13 行「已免疫」，全部来自 x86/aarch64
   轮次落地的架构中立修复（协议、调度、VM 语义、C 翻译语义、senda 步长派生），riscv
   不需要做任何事。这印证了「公共腿不动、arch 层进 trait」铁律的复利。
2. **riscv 专属的中招集中在四簇**：(a) sscratch 生命周期簇（C 表 8、9 行——boot 必踩 +
   SMP 前置裁决）；(b) VM 用户态页表通道簇（4、5 行——sfence 门控 + split_huge，都是
   aarch64 已修形状的直译）；(c) registered gap 簇（3、6 行——IPC 桥 + 缺页路由）；(d)
   **内核调用腿簇（第 2 轮新增：24、25、28 行——reply_wire 未迁、SUM 缺口、.bss 清零，
   三者都在「甲案接通生产入口」的必经路径上，NK4C prompt 阶段 3 的实施顺序恰好
   按 3.1 甲案→3.2 SUM 丙案→3.3 U-mode 腿排列，与本表簇序一致）**。
3. **没有发现任何「riscv 已实现但语义错了」的中招**——静态层面 riscv 专属代码
   （paging/trap/protection）的已完成部分质量与其它两架构一致；风险全部在「未完成的
   部分」与「完成部分之间的组合缝隙」（sscratch 组合缝与 SUM 组合缝是两处会死机的）。
   第 2 轮新增的一类是**文档级漂移**（30、31、32 行——C 引用路径、控制台实现注释、
   CRLF），行为无损但污染对账，全数登记交主线程。

---

# D. WORKLOG 结构债深度分析（第 3 轮新增）

## D.0 范围、方法与债务总表

本章把两条工作日志里**显式挂「结构债」名目**的条目（NK4C-WORKLOG §1.115 遗留小节的
债①债②）与 NK4B/NK4C 各轮**登记待裁、且性质是架构级而非单行修复**的条目（平台描述符
来源通道、链接脚本双份、早控制台发射面、VM 自举内存预映、init 收尾腿忙等、探针治理）
合起来做一次静态深分析。方法论按 code-excellence 约定：对每笔债先钉死 HEAD 现状锚点，
然后问「如果今天重写会怎么设计」，给出不少于三个候选方案（机制、对位、代价、与
「公共腿不动、架构层进 trait」铁律的相容性），并对照五个参照系——minix3 C 源
（ground truth）、Linux、Redox、Rust 社区惯例、操作系统理论——最后给 riscv64 投影与
推荐序。**架构级裁决归用户**；凡触及对外契约的方案按仓规标 `[ARCH: ...]`（三处一致：
文档、设计、代码），本章只做裁决前分析、不改任何代码。

| 债 | 名目 | 来源登记处 | 裁决状态 | riscv64 相关性 |
|---|---|---|---|---|
| 债① | 三种 boot 进入形态并存、维护面 ×2 | NK4C §1.115 债① | 未裁 | 高（甲案落地将产生第四变体） |
| 债② | x86_64 是否迁移方案 A | NK4C §1.115 债②（OQ） | 挂 OQ（R3 认同不预防性重构） | 中（触发条件与 riscv 联动） |
| 债③ | 平台描述符与内存图的固件来源通道 | NK4B M3.4（D1>B>A）＋ M4.4 事实一 | aarch64 待 D1/B，riscv 已裁甲案 | 高（A.1/A.4 的上游） |
| 债④ | 链接脚本双份常量表达（旧三份 link.ld 无构建引用） | NK4B M4.2 决策一（甲/乙/丙） | 未裁 | 中（riscv64.ld 在内） |
| 债⑤ | 早期控制台三套发射实现（x86 错端口＋无上限轮询） | NK4B M3.3 评审登记＋第 1 步划界 | 未裁（留给 x86 修复批次） | 低（SBI 后端合规） |
| 债⑥ | VM 自举内存预映缺件（C reservedqueue／MAP_PREALLOC） | NK4C §1.93–§1.95（r7b 只落 stopgap） | 修复方向已裁方案 A 精神、未实施 | 低（stopgap 共享，DM 覆盖真机已过） |
| 债⑦ | init 收尾腿忙等（quiet_wait 应为停车） | NK4C §1.117 修复轮 CodeReview #2 | 登记待办 | 低（marker 路径不经 death 腿） |
| 债⑧ | committed 探针与诊断设施治理（160 处/29 文件） | R3 §五.1＋各轮 task1-close 登记 | 既定 task1-close 大裁决 | 中（取证通道质量＝D5 的载体） |
| 债⑨ | PAF_CLEAR 缺页即清腿（管线建成、漏斗一个接头未插） | NK4C §1.101 CodeReview MUST-1(a) | 登记后续加固 | 中（riscv boot 同依赖） |
| 债⑩ | 数据→指令切换的缓存维护（fence.i / dc cvau / x86 免疫） | NK4C §1.115 CONSIDER-5（仅 aarch64 面） | aarch64 已登记、riscv/x86 面未入账 | **高（riscv 面是本轮新发现）** |
| 债⑪ | DM「存在→改」的代刷腿（腿在、VM 不用、论断在 VmDm 不成立） | NK4C §1.111（原文「未接线」——第 4 轮翻案） | 方案三档待裁 | **高（与 C 表第 4 行同一改动面）** |
| 债⑫ | 静默吞错家族（`let _ =` 包 IPC/copy/回复 ＋ 字面 −1 兜底 ＋ 正 errno 成功车道） | NK4C §1.106＋§1.118续/§1.119＋§1.57（B33a，第 5 轮补） | **逐点处置表已出（第 6 轮，D.13）**：31 站＝乙 7＋维持 23＋甲 1 | 中（取证可信度根因） |
| 债⑬ | boot bump 池无回收路径＋kerninfo 映射责任移交 VM 未落地（「结构债三件套」第二件） | NK4C §1.55/§1.56 三件套＋kerninfo.rs:24-31 既定设计（第 5 轮主动扫描补入） | 设计已在（移交触发点写明），实施未排 | 中（甲案下债随形态迁移而非消失） |

## D.1 债① 三种 boot 进入形态并存（[ARCH: boot-form-unification]）

### 现状静态刻画

同一份 `minix_kernel` crate 今天有三种进入形态，全部在 HEAD 可验：

- **形态一（x86_64 内联）**：内核以 rlib 链进 boot-shim PE，进程内直调
  `arch_boot`/`kmain`——`os/boot-shim/src/main.rs:85-86`
  `#[cfg(not(target_arch = "aarch64"))] minix_kernel::arch_boot(...)`（riscv64 同落此臂，
  尽管该 bin 的 UEFI 形态对 riscv 根本不产出，属「写着但不构成 riscv 生产路径」）。
- **形态二（aarch64 跨镜像）**：boot-shim 只建表+开分页，写 `BootHandoff` 载荷
  （`#[repr(C)]`＋单页尺寸编译期断言，`os/libs/minix-boot/src/handoff.rs:48/:74`），
  绝对跳进独立链接的高半 `kernel.elf`；`os/kernel/src/lib.rs:258-259`
  `#[cfg(all(not(feature="mock"), target_arch="aarch64"))] pub fn bootstrap_to_kernel_image`
  与 `lib.rs:328-329` `arch_boot_resume_high_half` 成对出现——两镜像各自持有 `.bss`
  副本，靠 `BootAlloc::resume`/`boot_alloc_next` 接续 bump 游标。
- **形态三（riscv64 内联＋镜像契约位）**：`main.rs:82` 注释「x86_64 / riscv64 keep the
  inlined arch_boot path」，但 riscv 的 UEFI bin 不存在；生产镜像只产出布局契约位
  （M4.2），甲案落地后 riscv 将是 **kernel-image 自当引导体**的第四变体（不经 boot-shim）。

隐性分叉的机制（不是风格问题）：两种形态的内核代码各自实例化全局态。§1.115 增量 2 的
真机踩坑是判例——kernel-image 侧若调共享的 `build_bootstrap_root_and_enable`，会对**已
激活**的根页执行 `new_from_page` 的清零（riscv 同款在 `os/arch/src/riscv64/paging.rs:455-459`
`write_bytes(ptr, 0, 512)`），当场摧毁自身高半映射；正解是 additive 续跑专用入口
（`arch_boot_resume_high_half`，内含 `set_current_root_phys` 不补则 panic 的第二坑）。
这类坑在每个共享 boot helper 的签名上都是隐形的——它在「两个 `.bss` 世界」的假设差异里。

### 如果今天重写

唯一形态：固件适配层（每架构一个 boot-shim/引导体，只负责到「分页已开＋信息递齐」）→
唯一内核 ELF 契约（`BootHandoff`）→ additive 续跑。每个 boot 特性在真机矩阵里占一格，
不是两格。

### 五参照系对照

- **C ground truth**：minix3 单形态——boot monitor 装载唯一内核镜像，内核自带未分页
  启动段（`kinfo.bootstrap_start/bootstrap_len`，C `pre_init.c:114-116`），装完后回收。
  形态二象性是移植过程的产物，不是 C 的语义要求。
- **Linux**：唯一产物链。x86_64 的 `head_64.S` 在内核**内部**建初始页表、跳
  `__START_KERNEL_map` 高半（早期引导期低半身份映射→高半的切换是内核自己的启动代码，
  不是外部交接协议）；arm64/riscv64 用 Image 头＋自重定位。引导器只递
  `boot_params`/DTB——「形态」从不分叉。
- **Redox**：最接近统一形态的现役系统——redox-bootloader（Rust，BIOS 与 UEFI 统一）
  装载唯一 kernel ELF、建页表（含内核高半）、递内存信息；内核无第二形态。
- **Rust 社区**：bootloader crate（统一 BIOS/UEFI 的内核加载器）、Hermit 的 loader
  分工同形；契约类型用 `#[repr(C)]`＋编译期断言（本仓 handoff.rs 已合规）；cfg 卫生
  的惯例是「同一 lib、每架构一个 bin、差异压进 bin」而不是「差异渗进 lib 的全局态」。
- **OS 理论**：引导分级（固件→二级引导→内核入口）的交接协议应单向收敛——二级引导
  负责到「分页开启＋信息规范」，内核入口契约最小化（两个寄存器＋一页载荷）。形态唯一性
  的收益是可测试性：双形态意味着每个 boot 回归都要双份真机签名（NK4C 债① 原文
  「维护面 ×2」）。

### 方案对比

| 方案 | 机制 | 对位 | 代价/风险 | 铁律相容 |
|---|---|---|---|---|
| 甲：统一到独立 ELF＋BootHandoff（三架构全迁） | x86 放弃内联，boot-shim 变纯建表器 | Redox；§1.115 aarch64 已走通的全套零件（handoff.rs、resume 入口、boot_alloc resume） | x86 生产链刚翻绿（R3 独立复证 marker×2），迁移重付真机验证；`[ARCH: boot-form-unification]` 三处一致 | 好（公共腿不动，boot-shim 归装机层） |
| 乙：保持多形态，把分叉显式化 | 共享 boot helper 全部提为 crate 公共 API＋两形态各跑宿主等价测试；内联形态编译期断言不触碰 handoff 面 | 本仓 M3.3「行为等价不是不改的理由」教训的推广——把形态等价变成契约测试 | 改动小，但双份真机签名维护面照旧；断言只能守住已知的 `.bss` 坑类 | 最好 |
| 丙：统一到内联 | aarch64 回退 | — | **否决**：TTBR0/TTBR1 双根下内联形态已被 §1.114 真机证死，回退是技术倒退 | — |
| 丁：riscv 落甲案、x86/aarch64 维持，形态数记入账本 | riscv kernel-image 自当引导体（第四变体）；xtask/check-layout 表驱动钉住；等价性断言照乙案加 | NK4C prompt 阶段 3.1 已裁的甲案；债② OQ 继续挂 | 形态数暂时 +1；换来 riscv 不被 boot-shim 组装工作阻塞 | 好 |

### riscv64 投影与推荐

riscv 甲案落地时把「共享 boot helper 是否有第二个调用形态」作为验收检查项（乙案的
等价性断言顺手落地）；x86 迁移与否完全由债② 的 OQ 触发条件决定（见 D.2）。推荐序：
**丁（短期，已裁路线）→ 乙（随丁落地）→ 甲（仅当债② 触发）**。裁决归属：债② OQ
（用户）；丁不需要新裁决。

## D.2 债② x86_64 迁移方案 A（挂 OQ）

### 现状与免疫情机理

x86_64 今天免疫切根，靠三件事叠加：CR3 单根、`inherit_supervisor_half` 拷贝
PML4[256..512]（`os/arch/src/x86_64/paging.rs:546`）、global 页强制继承——进程根永远
带着内核高半。aarch64 之所以必须方案 A，是 TTBR0/TTBR1 双根＋内联形态从未进 TTBR1 的
硬件现实（§1.114 钉死）；riscv 单 `satp`＋高半继承（`paging.rs:486-505`）天然免疫，
**与 x86 同属「不需要方案 A」组**。NK4C 债② 与 R3 评审一致口径：不预防性重构。

### 触发条件清单（把 OQ 变成可观测的判据）

1. 内联形态再出 boot 期难以定位的缺陷（特别是 `.bss`/重定位类——形态一的 ADRP/PC 相对
   语义与形态二不同的老坑家族）。
2. 出现需要独立内核 ELF 的消费方：启动度量、reboot 复用启动段、双内核 A/B 装载。
3. `BootHandoff` 契约需要第三个真实消费者验证架构中立性——**注意：riscv 甲案不是
   BootHandoff 消费者**（kernel-image 自当引导体、不跨镜像交接），所以甲案落地不满足
   这条；NK4C 债② 原文的「riscv64 同构接入先行」与 prompt 裁决的甲案形状有这个偏差，
   本条把偏差点明。
4. x86_64 装机面需要换固件形态（脱离 UEFI 盘形）。

### 方案与推荐

三案：维持 OQ（推荐，条件触发再议）／联动债①甲案全迁／半迁移（出 standalone 产物但不
切默认——半成品形态，不建议：正好制造第四、第五变体）。对照：Linux x86_64 高半由内核
自建无外部契约（形态内聚）；Redox 全架构统一（形态唯一）；理论判据＝形态数与回归矩阵
的乘积。推荐：**维持 OQ**，触发条件入账本，R3 同口径。

## D.3 债③ 平台描述符与内存图的固件来源通道

### 现状静态刻画

「release 生产链从哪拿平台信息（中断控制器/时钟/控制台/拓扑/内存图）」这一条边界，
三架构三个答案：

- x86/aarch64（UEFI）：配置表 ACPI/DTB——AAVMF 不插 DTB GUID、GICR 字段恒 0（M3.4
  实测矩阵把 ACPI 侧判死），故 aarch64 判决为 D1（dumpdtb 出的 blob 当 ESP 文件喂给
  现成 file loader）> B（显式描述符通道）> A（dev 回退仅点电）。
- riscv（OpenSBI）：固件把 DTB 物理址递在 `a1`（M4.3 固件串口实证 `0x8fe00000`），但
  生产入口零读取（本轮复核 `kernel-image/src/main.rs` grep 零命中）；boot-shim 的
  OpenSBI 库 `build_memmap` 则硬编码 QEMU virt 单区 128 MiB
  （`os/boot-shim/src/opensbi_helpers.rs:61-64/:397-403`）。
- 通道本身已经存在且架构中立：`KernelInfo.platform_sources`＋`parse_by_kind`
  （`os/libs/minix-platform/src/kind.rs:49`）——缺的从来不是抽象，是每架构「把固件
  事实递进来」的最后一根线。

### 如果今天重写与对照

- C ground truth：boot monitor 铺好 `kinfo`（`pre_init.c`），内核不挑固件。
- Linux：内核**自己**解析三源（E820/boot_params、DTB、ACPI），固件只递指针——
  「解析在内核、递送在固件」的分工；本仓的 `parse_by_kind` 形状与此同构。
- Redox：bootloader 统一产出规范化内存信息，内核只认一种。
- OS 理论：引导信息契约的单调收敛（firmware facts → normalized descriptor）；
  内存图是安全面（帧池位图容量、§1.111 教训），来源说谎＝启动期隐性 OOM（§1.101）。

### 方案

| 方案 | 机制 | 对位 | 代价/风险 | 相容 |
|---|---|---|---|---|
| 甲：每架构固件源归一（现状方向） | riscv 入口读 `a1`→DTB 源→memmap 一并解析（载体先例 test-rt-birth-riscv64:426,493-501 逐行可抄）；aarch64 走 D1；x86 维持配置表 | Linux 的「解析在内核」＋仓内既有 `PlatformDescSource` | 每架构各接各的线；memmap 来源与 DTB 解析耦合 | 好 |
| 乙：B 案显式描述符通道 | 装机面写规范化描述符文件/字段，release 直接认 | Redox 的 bootloader 统一信息 | 动 KernelInfo 契约位（`[ARCH]`）；riscv 无 UEFI 盘概念，依赖 U-Boot fatload 装机形 | 中（契约变更需三处一致） |
| 丙：硬编码＋dev 回退（现状 boot-shim 形状） | QEMU virt 常量＋release 拒 | 无（说谎面） | §1.101「侥幸内存映射掩盖真问题」的温床；仅可作点电 | 差（只配 dev） |

### riscv64 投影与推荐

riscv 走甲案（prompt 已裁），落地时把 `DEFAULT_RAM_SIZE` 硬编码一并换成 DTB memory
节点真值源（对齐 §B D6 推荐）；aarch64 的 D1 是主线程既定批次。推荐序：**甲（riscv，
已裁）→ D1（aarch64，已裁）→ 乙（仅当跨架构描述符需求出现）**。

## D.4 债④ 链接脚本双份常量表达

### 现状

`os/kernel/src/arch/{x86_64,aarch64,riscv64}/link.ld` 三份**无任何构建引用**（本轮
grep：仅 `os/arch/src/arch/direct_map.rs:72/:102/:134` 的文档注释与内核宿主测试的常量
断言引用它们）；生产镜像用 `os/kernel-image/*.ld`。旧三份缺 `AT()` 段分离、缺
`KEEP`、缺引导栈预留（NK4B M4.2 决策一逐条实测）——既不是死代码（文档锚点指着它）
也不是活代码（不链接），是「第二份事实」。

### 方案

| 方案 | 机制 | 对位 | 代价 | 相容 |
|---|---|---|---|---|
| 甲：删旧三份＋文档改指 `kernel-image/*.ld` | 单一事实源 | Linux（每架构单份 vmlinux.lds）、Redox、C minix3（Makefile 单份） | 改三处文档锚点＋宿主测试注释 | 好 |
| 乙：合并吸收 | 旧脚本反向吸收新契约 | — | 风险最大（旧脚本语义与 boot-shim 契约本不兼容） | 差 |
| 丙：头部注记「仅文档参照」 | 双份保留＋明示 | — | 最省但留坑（NK4B 自评） | 好 |
| 丁（本线程新增）：宿主测试改读生产脚本 | `test_linker_script_*_constraints` 把读的对象从旧脚本换成 `kernel-image/*.ld`——把 check-layout 的 L0「脚本↔预期表对账」思想拉进 Rust 宿主测试，旧三份随之自然失去事实源地位 | 本仓 M3.2 L0 断言先例 | 小（测试改读取路径）；单源化由测试强制而非注释约定 | 最好 |

对照 Rust 社区惯例：常量单源＋构建期断言（`const _: () = assert!` 本仓已用于基址
互检，direct_map.rs:155-160）正是丁案的精神。推荐：**丁（或甲）**，乙否决。裁决归属：
低风险批次，可交主线程顺手处理。

## D.5 债⑤ 早期控制台三套发射实现

### 现状

- boot-shim x86 发射器：`os/boot-shim/src/lib.rs:136` 轮询读端口 `0x3f9`——16550 的
  LSR 在 `0x3fd`，`0x3f9` 是 IER，其 bit5 恒 0 → 每字节空转满上限 10 万次才放行，
  节流形同虚设（M3.3 评审登记，非本批引入；QEMU 串口同步排空所以生产链仍绿）。
- plat x86_64 早控制台：`os/plat/src/x86_64/early_console.rs:64` **无上限**
  `while (inb(COM1_BASE + 5) & 0x20) == 0 {}`——真机串口异常时死循环（M3.3 第 1 步
  划界明确不修、留给 x86 批次）。
- PL011：已修为「共享纯函数 `tx_wait_then_send`＋有界等待」（`1a2a8eb61`），判断力
  抽到宿主可测层——本债的正面先例。
- riscv64：SBI console_putchar ecall（`plat/src/riscv64/early_console.rs:9-33`），
  固件代管流控，本债无关；唯 `kernel-image/src/main.rs:52-57` 注释与实现不符（C31）。

### 如果今天重写与对照

一个 `EarlyConsole` trait（已存在）＋每架构后端＋**发射契约收敛在共享层**（有界等待、
上限、CRLF 归一），boot-shim 不再自带端口汇编。对照：Linux 的 early_printk 每架构各自
实现但共享 putchar 协议与 `console_init` 分层；Redox 早控制台单一驱动接口；C minix3
`printf→putk→rs232` 单链。Rust 社区：`embedded-hal` 式「契约在 trait、实现在后端、
时序约束写成可测纯函数」——`tx_wait_then_send` 已是这个形状。

### 方案

| 方案 | 机制 | 代价/风险 | 相容 |
|---|---|---|---|
| 甲：boot-shim 统一走 minix-plat 后端 | x86 后端在 plat 补 `0x3fd`＋有界；boot-shim `raw_serial` 变薄壳（aarch64 先例 `5ac5625f1` 的推广） | 改 x86 发射面需真机双跑批次；契约测试随 `tx_wait_then_send` 模式推广 | 好 |
| 乙：仅修两处（原划界计划） | `0x3f9→0x3fd`＋无上限改有界，不动结构 | 最小；三套实现并存的维护面保留 | 最好 |
| 丙：冻结＋契约测试 | 发射时序抽宿主可测纯函数＋字节冻结测试（M3.3 startup.nsh 先例） | 不修只锁；错端口照样错 | 好 |

推荐：**乙先修（与 x86 修复批同车）→ 甲作为 S 类结构单元另立**。riscv 投影：零改动，
仅注释对齐一行。

## D.6 债⑥ VM 自举内存预映缺件（C reservedqueue ／ MAP_PREALLOC）

### 现状静态刻画

C 的完整形态有两层：`minix3/minix/servers/vm/alloc.c:57` `MAXRESERVEDPAGES=300` 的
reservedqueue，`:64` `int mappedin` 字段＋`:127` `rq->mappedin = mapped`——VM 启动前
把自身工作内存整片**预映**，因为用户态缺页服务者无法服务自身缺页（self-paging）；以及
libexec/region 的 `MAP_PREALLOC` 当场取帧（`region.c:492-499`）。Rust 侧两层都缺：
`os/servers/vm/src/mmap.rs:118-119` 对 PREALLOC 只记 `VrFlags::PREALLOC_MAP` 位；
`impl MemType for AnonymousMemory`（`os/servers/vm/src/memtype.rs:253` 起）只有
name/writable/ev_unreference/缺页处理（demand-fill），无 eager 物化。§1.92–§1.95 落的
是两层 stopgap：`VM_STACK_SIZE=256KiB` runway（`os/arch/src/arch/boot.rs:405`，共享
装载体、riscv 同享）＋`MAX_BIG_BLOCKS` 绑 `GLOBAL_POOL_PAGES`（§1.95）。VM 自身页表页
与元数据的运行期增长走 `heap_arena`→`vm_self_mappages`（活的、非本债范围）。

### 为什么是结构债

stopgap 的边界是启发式（runway 尺寸、布局敏感）——§1.92 的实验矩阵证明 64/128/1024
常量在「剃刀边缘布局」上翻转崩溃形态。C 的 reservedqueue 是原则解：预映语义不依赖
布局运气。OS 理论上这是 self-paging deadlock 的标准三解之一（预留池／内核代服务／
预映）——本仓实际是混合体：内核 bootstrap root＋`establish_boot_dm` 承担了部分
「内核代服务」，reservedqueue 的「预留池」腿缺位。

### 方案

| 方案 | 机制 | 对位 | 代价/风险 | 相容 |
|---|---|---|---|---|
| 甲：补 eager 物化 | `AnonymousMemory` 对 PREALLOC 区在 mmap 时逐页 `alloc_pfn+map_page`（C `region.c` 对位） | C MAP_PREALLOC | 帧预算上升（§1.59 家族）；需判别性回归测 | 好 |
| 乙：VM-backed 增长堆 | 堆供给走 VM 服务（`allocator`→`VM_BRK` 客户端腿，注意 alloc 再入）——NK4C-WORKLOG「结构债三件套」之一（:3082） | C `brk` 保真（`brk.c _syscall(VM_PROC_NR, VM_BRK)`） | 更大重构 | 中 |
| 丙：诚实关闭 stopgap | runway 语义写入设计文档为「已评估的临时形态」＋布局敏感警示 | — | 无代码；风险是后人误当终态 | 好 |
| 丁：内核代服务扩展 | 把 `establish_boot_dm` 的 bootstrap 语义延伸到 VM 运行期 | 本仓已有半形 | 模糊内核/VM 职责边界 | 差 |

对照：Linux 无此问题形态（内核自管 memblock reserve、无用户态缺页服务者）；Redox
的内存服务（memory:）同样是用户态进程，其自举由内核初始映射承担——与本仓
bootstrap root 同形。推荐：**丙立即（文档）＋甲列为 S 类单元（带判别测试才动）**。
riscv 投影：stopgap 共享已覆盖，DM 覆盖真机通过（NK4B serial_clk1），不阻塞 riscv。

## D.7 债⑦ init 收尾腿忙等（quiet_wait）

### 现状

`os/commands/sbin/init/src/driver.rs:383-388`：

```rust
fn quiet_wait(host: &mut dyn InitHost) -> ! {
    loop {
        let _ = host.waitpid(-1, 0);
    }
}
```

注释自称对位 C `init.c:850-856` 的 `sigfillset + for(;;) sigsuspend`——但 C 的形态是
**睡眠等信号**（无子进程事件时零开销驻留），Rust 版每轮一次阻塞 `waitpid` 往返
（阻塞在 PM）＝事件驱动的 IPC 轮询；信号投递腿不活时即纯忙转。§1.117 修复轮的
CodeReview #2 已把它登记为待办结构债（「正解是 park 非退避，不草率改时序」）。

### 方案

| 方案 | 机制 | 对位 | 代价 | 相容 |
|---|---|---|---|---|
| 甲：对位 C 停车 | 阻塞 receive 等 SIGCHLD/sigsuspend 语义——依赖信号投递腿（aarch64 §1.116 BLOCKER-1 已证明 FullContext 存帧缺失会让信号永投不出） | C `sigsuspend` | 需 park 腿＋信号腿先行 | 好 |
| 乙：WNOHANG＋定时退避 | 非阻塞轮询＋睡眠 | — | 不贴 C、仍忙转（只是降频） | 差 |
| 丙：保持＋文档标注 | death 腿非 marker 关键路径，先记账 | — | 无 | 好 |

对照：Linux 用户态 init 死亡态直接 panic 内核（无此腿）；OS 理论：收尾路径同样不该
空转（耗电/抢占噪声）。推荐：**丙立即＋甲随 park 落地批次**。riscv 投影：同文件架构
中立，marker 路径不经 death 腿——非阻塞项。

## D.8 债⑧ committed 探针与诊断设施治理

### 现状

160 处引用/29 文件（R3 §五.1）；其中 committed 取证基础设施（schedctl/en/rcvi 等）是
有意保留的诊断面，两支登记死探针（`vmpt2bf`/`sas-send`）等 task1-close 大裁决；
§1.100 已示范过一次 635 行的清理批次。§1.119 的教训给这条债加了权重：串口证据在洪流
下不可靠——**探针体系的可信度直接决定取证轮的成本**（D5 证据通道的载体就是它）。

### 方案

| 方案 | 机制 | 对位 | 代价 | 相容 |
|---|---|---|---|---|
| 甲：task1-close 大裁决（既定） | 逐支判定保留/删除 | 本仓既定节奏 | 一次性大评审 | 好 |
| 乙：分层诊断制度 | 三层契约——永久 boot 路标（`boot_stage!`）/限次取证探针（cap＋门＋回滚纪律）/committed 诊断设施（正式 API 化）——配清单生成脚本进 pattern-gate 家族 | Linux `pr_debug`/dyndbg 的编译期分层；Rust `log`/`tracing` 门控 | 制度成本；存量要先归类 | 好 |
| 丙：全清 | 删除一切非路标探针 | — | 丢掉活面包屑（§1.100 后 r10 面包屑论） | 差 |

推荐：**乙的分层契约为甲的裁决提供准绳**（先定「什么可以 committed」，再逐支判）。
riscv 投影：D5（非串口可靠记账）落地时，第 2 层的纪律直接复用。

## D.9 汇总：修复批次建议与裁决归属

| 批次 | 内容 | 前置 | 裁决 |
|---|---|---|---|
| 立即（文档级，可随任意批次搭车） | D.4 丁/甲（链接脚本单源）、D.5 注释对齐（C31）、D.6 丙（stopgap 语义文档化）、D.7 丙（quiet_wait 记账）、kernel-image 模块头交付边界更新（R3 7.3 项 3） | 无 | 主线程自主 |
| 主线程自主批（错误路径纪律） | **债⑫ 丙（吞错家族清点器进 pattern-gate 基线）＋ 乙（旗舰两点：fs-rt transport.rs:272、pm ipc/vfs.rs:270 诊断下限）**；续轮把 ~30 站点逐点过三问出处置表 | 无 | 主线程自主；甲（签名改造）列 S 单元 |
| aarch64 收口批（已登记缺口的收尾） | §1.115 CONSIDER-5 的 `dc cvau/ic iallu`（＝债⑩ 甲的三架构契约在 aarch64 的落地，顺带把 riscv fence.i 纳入同一 trait） | aarch64 真机批次 | 已登记，实施面待裁 |
| riscv 甲案落地批（NK4C prompt 阶段 3.1） | D.3 甲（a1→DTB→memmap 真值源，含 D6 常量换真值）＋ D.1 乙案等价性断言＋ D.8 乙案第 2 层纪律复用＋ **债⑨ 甲（漏斗接 CLEAR，判别测试随批）＋ 债⑩ 甲（`sync_icache` 契约、riscv 面＝exec 腿 fence.i；第 5 轮散点先例强化推荐）＋ 债⑪ 甲（write_pte_dm 门控对齐，与 C 表第 4 行同一改动面）** | 甲案开工 | 已裁（prompt 阶段 3）；债⑨⑩⑪ 的推荐项与甲案批同车 |
| park/信号批 | D.7 甲（quiet_wait 停车化） | park ABI＋信号投递腿 | 架构中立，随批次 |
| x86 修复批（既定划界） | D.5 乙（0x3f9→0x3fd＋无上限改有界）＋ D.5 甲作为后续 S 单元 | x86 批次开工 | 主线程自主 |
| VM handoff 收口批 | **债⑬ 甲（kerninfo 映射责任移交 VM，kerninfo.rs:24-31 既定设计补实施）**；债⑥ 甲（eager 物化）如裁决随批 | VM 接管用户 PTE 工作时（既定触发点） | 设计已在，排期归主线程 |
| OQ 悬置 | D.1 甲／D.2（x86 形态统一）——按触发条件清单再议 | 触发条件出现 | 用户 |

## D.10 债⑨ PAF_CLEAR 缺页即清腿（管线建成、漏斗一个接头未插）

### 现状静态刻画（第 4 轮实测）

- **C 对位**：`minix3/minix/servers/vm/vm.h:22` `#define PAF_CLEAR 0x01 /* Clear
  physical memory. */`——分配请求显式要求清零，allocator 消费。
- **Rust 侧的意外发现：管线三层建了两层半**。
  1. 请求位：`os/servers/vm/src/phys_mem/types.rs:87` `const CLEAR = 0x01`（注释自证
     对位 vm.h PAF_*）；
  2. 消费端：**三个分配器后端全部实现**——`bitmap_alloc.rs:374`、`buddy_alloc.rs:488`、
     `segment_tree_alloc.rs:290` 各有 `if flags.contains(PageAllocFlags::CLEAR)` 的
     零填分支；
  3. 翻译函数：`os/servers/vm/src/region/vir_region.rs:59-71` `to_alloc_flags`——
     `#[allow(dead_code)]`＋注释自证「V10-P2-1: no caller yet」，且映射逻辑已含
     `if !self.contains(Self::UNINITIALIZED) { af |= PageAllocFlags::CLEAR; }`（对位
     C `VR_UNINITIALIZED` 语义：未初始化区才豁免清零）。
  4. **唯一的断点**：缺页漏斗 `os/servers/vm/src/cow_exec_pf.rs:281-297` `alloc_and_map`
     调 `alloc_pfn_reclaiming`（`os/servers/vm/src/alloc_page.rs:236`，签名**不带
     flags**）——demand-fault 取帧从不请求 CLEAR，后续 `region.map_page` 也不清。
- **登记处**：`os/servers/vm/src/vm_server.rs:821` 注释（§1.101 修复时的 MUST-1(a)
  后续项）：「页对齐整页尾的零填依赖 ANON demand-fault 给零页——boot 期无页缓存可
  回收、alloc_pfn_reclaiming 退化为全新帧，故行为不变；把 demand-fault 显式接
  PAF_CLEAR（对位 C VR_UNINITIALIZED）列为后续加固项」。
- **WORKLOG 叙述 vs HEAD**：一致（转换函数至今死代码），无翻案；但 WORKLOG 的表述
  漏了更重要的一半——**死代码不是缺口本体**，缺口是「后端已就绪、漏斗不传参」：
  债的全部内容只是漏斗一个接头的插拔，外加「哪些区该豁免」的语义判断（翻译函数已写好）。

### 为什么是结构债

`alloc_pfn_reclaiming` 的「reclaiming」意味着帧回收路径存在或将来存在；「新帧恰为
全零」今天是运气（boot 期无页缓存可回收），页缓存/回收一上线就变成隐蔽数据污染
（.bss/堆尾读到上一任脏字节——§1.86 家族的另一种复现通道）。把「零初始化」押在
分配器行为不变量上而不是显式请求上，是 Rust 社区所称「以隐式不变量替代显式契约」
的典型形态。

### 方案

| 方案 | 机制 | 对位 | 代价/风险 | 相容 |
|---|---|---|---|---|
| 甲：漏斗插接头（C 忠实版） | `alloc_and_map` 按 region 类型构造 flags（非 UNINITIALIZED → CLEAR），`alloc_pfn_reclaiming` 加 flags 形参透传，`to_alloc_flags` 转正（去 `allow(dead_code)`） | C `PAF_CLEAR`＋`VR_UNINITIALIZED` 全语义；Linux 的 `__GFP_ZERO`（分配请求显式带零填标志——同形状） | 每次 demand-fault 多一次 4KiB memset（C 同付）；boot 期 12 服务的 .bss/堆尾全走此路，真机启动成本上升 | 好 |
| 乙：只清缺页腿、boot eager 腿不动 | 甲的子集：仅 `alloc_and_map` 传 CLEAR，`vm_server.rs` eager 腿维持自带 write_bytes 清头尾 | 现状最小增量 | 同甲的性能代价；语义同 C | 好 |
| 丙：删除管线、文档化不变量 | 删 `to_alloc_flags`＋`CLEAR` 位（含三后端分支），「新帧为零」写成 allocator 契约＋debug 构建毒化校验 | Rust 社区「unsafe 前置条件文档化＋debug_assert」风 | 帧回收/页缓存引入时静默腐烂（§1.101 原始担忧原样回归） | 中 |
| 丁：debug 毒化折中 | 丙的文档化＋debug 构建对「请求了 CLEAR 的调用点」补 memset、release 走零帧假设 | — | 两套行为分歧（debug 抓不到 release 假设破裂） | 差 |

对照：Linux 的零填从来是**分配请求标志**（`__GFP_ZERO`），不是分配器不变量——甲的
形状正是它；Redox 的物理分配器同理带 zero 请求。OS 理论：未初始化内存的「zeroed
by default（安全）」vs「uninitialized（性能）」之争，C 用 VR_UNINITIALIZED 显式豁免
恰是教科书形态。

### riscv64 投影与推荐

架构中立；riscv 的 boot 路径同吃这份运气（vm_server.rs:815 注释对三架构同样成立——
甲案批的 12 服务 .bss 尾在 riscv 上一样靠零帧）。推荐：**甲（或乙，同一修的语义
两档）**，判别性测试现成可造（非零毒化假帧＋UNINITIALIZED 区不清）；丙只配文档。
裁决归属：主线程自主批（带判别测试），建议搭 riscv 甲案批的车（本条在 riscv 上
是「exec 链路正确性」的前置，不是 marker 直接路径）。

## D.11 债⑩ 数据→指令切换的缓存维护（三架构三形态；riscv 用 fence.i）

### 现状静态刻画（第 4 轮实测；第 5 轮外部复核驱动修正 riscv 列）

- **登记处**：NK4C §1.115 CodeReview CONSIDER-5——kernel.elf 段体被 boot-shim 当数据
  memcpy 后首次当指令取，ARMv8 D7.5.9 要求 `dc cvau`＋`ic iallu`＋`isb`；QEMU 功能
  模型永不复现、实机必炸；登记为真机 bring-up 已知缺口。
- **HEAD grep（第 5 轮修正版，分模式计数）**：`cvau`/`iallu` 零命中——aarch64 面未
  落地，这半断言维持；`fence.i` **恰有 2 行命中、全在 riscv64**：
  `os/kernel/src/arch/riscv64/higher_half.rs:54` 的 `jump_to_kmain` 内联 asm 里有
  **一条活的 `fence.i`**，`:38-39` 注释写明用途（「synchronize I-cache with writes
  performed during paging setup」）。第 4 轮「riscv 面从未入账」的断言**错误，就此
  翻案**：riscv 已有一个散点实现，但它只覆盖 boot 转换腿（hart0 一次性，jump_to_kmain
  的 SAFETY 注释自证「called exactly once from the boot CPU」）。
- **散点不覆盖的面（修正后的缺口真身）**：
  1. `load_elf_into` 写出的用户 text 帧（`arch/boot.rs:565`）被 exec 进程取指——
     **每 hart、每次 exec** 都发生，散点的 boot 期一次性 fence.i 保护不到；
  2. 次核 bring-up（SMP）后的任何跨 hart 取指同步（RISC-V fence.i 只保当前 hart）；
  3. aarch64 的 `dc cvau`＋`ic iallu`＋`isb`（CONSIDER-5 正身，仍缺席）；
  4. x86_64：免疫（自一致缓存），无需任何指令——同位文件
     `os/kernel/src/arch/x86_64/higher_half.rs` 本轮查无缓存相关散点，正常。
- **同位文件核查（第 5 轮）**：x86_64 与 aarch64 的 `higher_half.rs` 均无缓存维护
  指令或相关注释——aarch64 的散点缺席与其 CONSIDER-5 登记一致（无注释-实现不符）。
- **写入点（谁是「数据」）**：`os/arch/src/arch/boot.rs:565`
  `copy_nonoverlapping`（`load_elf_into` 段体拷贝——写出的正是用户进程将来取指的
  text 帧）、`:584/:621` `write_bytes`（.bss 清尾）；aarch64 形态二另有 boot-shim 的
  `load_segments_into_phys_memory`（kernel.elf 段体→物理内存，随后跳入取指）。

### 三架构机理与 riscv 投影（本债重点）

| 面 | x86_64 | aarch64 | riscv64 |
|---|---|---|---|
| 固件→内核镜像 | 形态一 PE 由 UEFI LoadImage 装载，固件负责；形态无「外部 memcpy 后自跳」 | 形态二 boot-shim memcpy 段体→自跳＝**CONSIDER-5 正身** | 甲案 OpenSBI/U-Boot 装载＝固件责任（QEMU ROM 拷贝无缓存模型；实机上 U-Boot bootelf 是否 fence.i 待真机判，仓内无代码可写） |
| 内核/VM 拷贝 ELF text→帧→用户取指 | 免疫（x86 缓存自一致，store 对取指可见由硬件保证） | 需显式 `dc cvau`＋`ic iallu`＋`isb`（未落地） | **需 `fence.i`（每 hart、每次 exec；散点只保 boot 腿）**——RISC-V 特权规范：store 对取指的可见性唯一同步原语是 fence.i，且只保当前 hart；SMP 下远程同步需 SBI `remote_fence_i`（Linux 的 `flush_icache_range`＋SBI 遥程栅栏正是此形） |
| 仓内已有实现 | 无（无需） | 无（已登记） | **散点一处**：jump_to_kmain 的 fence.i（boot 转换腿、hart0 一次性——higher_half.rs:54） |

### 归属判断（债① 之下还是独立）

**独立成条（债⑩）**，理由：家族成员横跨三架构、两层加载面（固件→内核镜像与
内核→用户 text），「形态二的固有尾巴」（aarch64 面 3）只是三者之一；债① 获得
一个交叉引用即可。x86 形态一免疫的机理也只有在独立条目里才说得清。

### 方案（第 5 轮：对位列按散点先例更新）

| 方案 | 机制 | 对位 | 代价/风险 | 相容 |
|---|---|---|---|---|
| 甲：per-arch 缓存维护契约 | trait 增 `sync_icache(range)`（名可议）：aarch64＝dc cvau 逐行+ic iallu+isb；riscv＝`fence.i`（SMP 后补 SBI remote fence）；x86＝no-op；`load_elf_into` 尾部与 boot-shim 拷贝后调用 | Linux `flush_icache_range`/`arch_sync` 家族；**仓内先例＝higher_half.rs:54 的散点 fence.i（第 5 轮修正：先例已在，缺的是把它从 boot 一次性散点契约化为按范围、按 hart 的接口）**；仓内 `TlbArch` 同型 trait 先例 | `[ARCH]` 契约（三处一致）；QEMU 永不红，判别只能到「指令存在＋调用点存在」级 | 好（正是「arch 层进 trait」铁律的标准应用） |
| 乙：riscv 最小先行 | 仅 riscv 批次：exec 建映像腿（load_elf_into 尾）一条 `fence.i` | 散点同款指令、新调用点 | aarch64 面继续挂账；第二处散点（契约化债务 +1） | 中 |
| 丙：维持登记＋QEMU-only 边界声明 | CONSIDER-5 现状推广到 riscv | — | 实机 bring-up 时必返工 | 好 |

**第 5 轮修正后的推荐判断**：散点先例的存在**强化甲案**——推荐序从第 4 轮的「甲，或
乙最小步」收紧为**甲**：散点已证明 fence.i 在 riscv 的 boot 路径是必要原语，乙是在
第二个调用点复制同一裸指令（散点债务+1），而甲只是把既有事实（boot 腿需要它）提升为
契约并补齐 exec 腿的第二个消费者；乙的「快」收益在甲的 asm 改动量面前不显著。丙仍
只配文档。裁决归属：主线程（`[ARCH]` 契约需三处一致流程）。

**锚点漂移标注（第 5 轮自审）**：第 4 轮「全仓 grep fence.i 零命中」的误判成因
**不是目录盲区**——原命令的扫描范围本就覆盖 `os/kernel/src/arch/`；真实成因是
模式缺陷叠加截断未验证：未锚定的 `dc `/`ic ` 两案在 `os/` 全域命中 **2960 行**
（大头是驱动文档里的 "traffic" 子串），输出被 `head -8` 在第 8 行截断，fence.i 的
2 行真命中被埋在截断线之后，而我在截断处停止并写下了「零命中」。修正版按模式拆分
计数（fence.i=2、cvau/iallu=0）后真相立现。教训（「多模式 grep 必须按模式拆分计数，
负结论禁止出自截断输出」）登记为 misc_concepts 候选（本线程无权写该文件，见进度
快照移交项）。

## D.12 债⑪ DM「存在→改」的代刷腿——翻案：腿在、VM 不用、设计论断在 VmDm 通道不成立

### 现状静态刻画（第 4 轮实测；含对 §1.111 叙述的翻案）

- **内核代刷腿已实现且三架构齐**：`os/kernel/src/syscall.rs:2555-2575`（FIX-24
  Phase 5；`VmCtlParam::GetPdbr`/`FlushTlb`/`InvlPg`；注释明说三架构覆盖——x86 CR3/
  INVLPG、aarch64 TLBI、**riscv SFENCE.VMA**）；`:2582-2592` `FlushTlb` 臂调
  `CurrentTlbArch::flush_all()`。C 对位：`minix3/minix/kernel/arch/i386/arch_do_vmctl.c:51`
  `case VMCTL_FLUSHTLB`。
- **配套机制半也在**：`syscall.rs:2767-2774` `mark_flush_tlb`（对位 C
  `do_vmctl.c:133-135` 的 `MF_FLUSH_TLB`，恢复前消费——E-VMTLB 机制半）。
- **VM 侧调用者＝零，且是有意设计**：`syscall.rs:2564-2575` V13-P2-1 注释原文
  「the VM server has ZERO callers of FlushTlb/InvlPg — that is by design, not a
  missing wire」，依据＝C 的 VM 在 4 个站点自刷（pagetable.c:119/255/319/430）是因为
  它把进程内存 alias 进自己地址空间，而本仓 Direct Map 让 PTE 页翻译恒定、
  `write_pte_dm` 对每次 PTE 写绑定刷新。本轮 grep VM crate 确认零调用者（仅
  `pagetable/sim.rs:137/141` 测试桩）。
- **三架构 `write_pte_dm` 门控现状（同一件事的三种形状）**：aarch64 门控 KernelDm
  （`os/arch/src/arm64/paging.rs:246-249` `if channel == PteChannel::KernelDm`）；
  x86 同门控（`os/arch/src/x86_64/paging.rs:64-71/:169`，保守 `invlpg 0`）；
  **riscv 无门控**（`os/arch/src/riscv64/paging.rs:248` 无条件 `sfence.vma`——C 表
  第 4 行）。
- **翻案**：§1.111 把「unmap/remap/update_flags 需内核代刷」记作「与 x86 同为
  **未接线**已知缺口」——不准确。腿在（FIX-24 早于该轮）；准确的表述是：**腿在、
  设计上不用，而「不用」的依据（每次 PTE 写自带刷新）恰好被 §1.111 自己的修复在
  VmDm 通道打破了**——VmDm 通道门控掉刷新后，「存在→改」类操作（unmap/remap/
  update_flags 经 VM 用户态执行）之后**没有任何东西刷新目标进程的陈旧翻译**。

### 机理与窗口分析

V13-P2-1 的论断「Direct Map keeps translations constant」只覆盖 PTE **页本身**的
翻译，不覆盖被修改**目标页**在目标进程 TLB 里的旧翻译。单核下窗口多半自然闭合：
VM 改完 PTE 唤醒目标进程必经 `switch_address_space`→`set_active_root` 全量刷
（riscv：`os/arch/src/riscv64/tlb.rs:86-90` csrw satp＋`sfence.vma zero, zero`）。
但窗口在以下条件下重开：SMP 多核 TLB、惰性切根（lazy TLB，若将来实现）、以及
「改完不唤醒目标而目标随后自己跑」的任何路径。riscv 的特殊性是双面的：现在的
无门控形状在用户态执行 `sfence.vma`＝非法指令**硬失败**（比 aarch64/x86 的静默
陈旧更容易发现，但让 VM 的 map 腿在 riscv 上根本跑不通——C 表第 4 行）。

### 方案

| 方案 | 机制 | 对位 | 代价/风险 | 相容 |
|---|---|---|---|---|
| 甲：维持 V13-P2-1 现状＋riscv 门控对齐 | riscv `write_pte_dm` 补 `if channel == KernelDm`（对齐 x86/aarch64，即 C 表第 4 行的修复形状）；把「SMP/惰性切根重开条件」写进 V13-P2-1 注释与账本 | 本仓 V13-P2-1 设计论断的完整化 | 单核论证依赖「唤醒必切根」这一调度事实——注释里必须写明依据；SMP 时重议 | 好 |
| 乙：VmDm「存在→改」后 VM 显式发 vmctl InvlPg/FlushTlb | VM 在 unmap/remap/update_flags 的 VmDm 路径尾部发轻量 vmctl（腿已在，VM 侧加调用） | C VM 自刷 4 站点（pagetable.c） | 回到 C 的 self-flush 模型；每次 PTE 修改一次额外 IPC 往返 | 好 |
| 丙：复用 E-VMTLB 打标半 | VM 改 PTE 后对目标进程 `mark_flush_tlb`（机制半已在），恢复前消费 | C `MF_FLUSH_TLB`（do_vmctl.c:133-135） | 需新增「VM 代目标打标」的 vmctl 语义（现 mark 只在内核路径内用） | 中 |

对照：Linux 内核自管页表＋本地刷新＋SMP IPI（无用户态页表服务者，问题形态不同）；
C minix3 是 VM 自刷＋MF_FLUSH_TLB 双轨。推荐：**甲（riscv 批次内与 C 表第 4 行同一
改动面顺手落）**，乙/丙留 SMP 里程碑再议（彼时有真机判据与多核 TLB 现实）。裁决
归属：主线程自主批（甲是单点门控＋注释）。

## D.13 债⑫ 静默吞错家族（错误路径纪律；逐点处置表已出——第 6 轮）

### 现状静态刻画（第 4 轮实测）

**旗舰 1（接口级结构性吞错）**：`os/fs/fs-rt/src/transport.rs:269-273`
`fn copy_out(&mut self, offset: usize, bytes: &[u8])`——**签名返回 `()`**，内部
`let _ = self.ipc.copy_to(self.peer, self.grant, offset as u64, bytes)`（`:272`）：
grant 越界的内核硬失败 EPERM（`os/kernel/src/grant.rs`，§1.106 实证「越界非截断而是
整块拒绝」）在 FS 侧被接口签名抹成无事件。同文件同族：`:249-250`（绝对符号链接改写
字节经 grant 回写，`let _ =`）、`:261-262`（`reply` 的 `send`，`let _ =`）。真机
实证在案：`os/servers/vfs/src/exec_worker.rs:93-97` 注释引真机 bn34p——「exec 首块
读 `hdr[0..8]=00 00 00 00`，`req_read` 却回 Ok」。**锚点漂移标注**：该注释与
NK4C §1.106 原文写的位置是 `fs-rt/src/transport.rs:272`，HEAD 真身在
`os/fs/fs-rt/src/transport.rs`（crate 布局迁移后注释未跟），行号 272 恰好仍对。

**旗舰 2（字面 −1 兜底吞真码）**：`os/servers/pm/src/ipc/vfs.rs:270`
`svc.reply_to_guardian(slot, new_parent, -1)`——线格式对位 C `main.c:381-384`（C 也
回 −1，**不是 wire 错误**），但 `sched_start_user` 的真实失败码不落任何诊断面；
§1.118续/§1.119 的 EPERM 悬案正是被它放大绕了两轮（工作日志原文：「查
ipc/vfs.rs:256 字面 −1 吞真码是否掩盖了 IPC 层曲解码」）。

**家族清单（本轮 grep，案值排序）**：`let _ = ` 包 IPC/copy/reply 的站点全仓约
30 处/12 文件——前列：`os/servers/pm/src/exit.rs`（4）、`os/servers/ds/src/server.rs`（4）、
`os/servers/devman/src/ipc/minix.rs`（4）、`os/servers/vfs/src/main_loop.rs`（3）、
`os/servers/rs/src/live_update.rs`（3）、`os/servers/input/src/serve.rs`（3）、
`os/servers/rs/src/recovery.rs`（2）、`os/servers/ipc-server/src/boundary.rs`（2）、
`os/servers/vfs/src/exec_worker.rs`（1）、`os/servers/sched/src/server.rs`（1）。
排序维度：①吞的是硬失败还是尽力而为（grant EPERM 类最重）；②是否在 boot/marker
路径上；③C 对位是否真检查（C 的 safecopy 返回值多数被查）。

**家族第三员（第 5 轮主动扫描补入）：正 errno 走成功车道**——NK4C §1.57 登记
follow-up 未修的 B33a：`os/servers/pm/src/exec.rs:53-62` `ExecError::to_errno` 至今
返回**正值** errno（EPERM/ESRCH/14/EINVAL），经 INIT `exec_via` 的「成功车道」被吞成
EIO，真实错误码被掩盖。它与债⑫旗舰两点同根（错误路径纪律）、又与 C24（riscv 腿
`reply_code()` 未迁 `reply_wire()` 的负 errno 线上编码）是同一族的两个面：一个在
服务器内部分类、一个在线上编码。第 5 轮未逐点复核调用侧是否归一化（列入债⑫续轮
三问表）。

### 如果今天重写与对照

错误路径纪律的仓内应有形态：「回复线的兜底值可以 C 忠实地给（−1），但真 errno 必须
落在诊断面；接口签名不得以 `()` 把 `Result` 抹成无事件」。对照：Linux 内核错误路径
纪律（错误必须传播到调用链尽头或 `WARN_ON`，禁止静默收窄）；Rust 社区：`let _ =`
吃掉 must-use 值是 clippy `let_underscore_must_use` 点名的反模式，例外必须注释论证；
OS 理论：fail-stop 优于 fail-silent——静默错误把调试成本转嫁给下一轮取证（§1.119
两轮悬案即成本实证）。

### 方案（目录级，续轮展开）

| 方案 | 机制 | 代价 | 相容 |
|---|---|---|---|
| 甲：签名改造 | `FsTransport::copy_out` 等 `()` → `Result`，涟漪全部 FS 驱动实现 | 大（trait 契约变更） | 好 |
| 乙：诊断面下限 | 保持签名，吞错处统一过 diagctl/log（endpoint＋grant＋offset＋errno），旗舰两点先行 | 小 | 好 |
| 丙：家族清点器进基线 | grep 模式（`let _ =` ×copy/send/reply ＋ 字面 −1 回复腿）进 pattern-gate 家族，存量入基线、新增即拦 | 最小 | 好 |

推荐：**丙立即 → 乙（旗舰两点）→ 甲列 S 单元**。裁决归属：丙/乙主线程自主批；
甲 `[ARCH]` 级需走三处一致。

### 逐点处置表（第 6 轮交付；三问＝①吞的是硬失败还是尽力而为／②是否在 boot/marker 路径／③C 对位是否真检查；处置档＝乙诊断面｜甲签名改造｜维持并注释）

**计数口径修正（第 6 轮）**：第 4 轮的「~30 站点」是同线过滤口径（`let _ = ` 与调用
号同在一行）；跨行拆分调用（`let _ = self.kernel` 换行接 `.send(...)`）被漏计——仅
devman 一文件即补出 3 站。本表按第 6 轮逐点采集实录，**代码站 30＋B33a 1＝31 站/
13 文件**（另有 exec_worker.rs:95 一处为文档注释非代码站；fs-rt 三处旗舰已在 D.13
上文现状段，不重复入表；C 对位列只断言本轮实测，未核处如实标注）。

**优先批（boot/marker 相关 crate）**：

| 站点 | 吞的调用 | ①硬失败? | ②marker 路径? | ③C 对位 | 处置 |
|---|---|---|---|---|---|
| `pm/ipc/calls.rs:633`（B33a） | `Err(e) => ReplyIntent::Reply(e.to_errno())`——`exec.rs:53-62` 返回**正值**，走成功车道 | 硬失败被改写成伪成功 | 是（exec 链） | C `do_exec` 返回负 errno 直回 | **乙＋一行修**：`Reply(-e.to_errno())`（与 C24 同族线上编码，建议随 riscv 甲案批） |
| `pm/exit.rs:772` | `send(parent_ep, ...)` 子进程退出通知 | 尽力而为，但丢事件＝父永不收尸 | 是（INIT waitpid 依赖） | C `exit.c` 父通知腿 [未逐行核] | 乙 |
| `pm/exit.rs:791` | `send(parent_ep, &reply_msg)` | 尽力而为 | 是 | 同上 [未核] | 乙 |
| `pm/exit.rs:825` | `send(tracer_ep, &reply_msg)` | 尽力而为 | 否（tracer 腿） | C ptrace 腿 [未核] | 维持并注释 |
| `pm/exit.rs:882` | `let _ = reply_to_new_parent;`（变量绑定，测试件） | 非生产路径 | 否 | — | 维持（测试件） |
| `vfs/main_loop.rs:1219` | `handle_fs_reply(msg, codec)` 返回值 | 硬失败可吞（FS 回复处理错误） | 是（VFS 主循环＝open/stat 腿） | C vfs 主循环 [未核] | **乙**（marker 路径优先） |
| `vfs/main_loop.rs:4745` | `sendrec(p.fs_e, putnode)` | 尽力而为（释放通知） | 否（unmount 面） | C putnode [未核] | 维持并注释 |
| `vfs/main_loop.rs:7594` | `sendnb(target, &reply)` | 尽力而为 | 视 target 而定 | [未核] | 乙 |
| `vfs/main_loop.rs:7604` | `sendrec(RS, &mut reply)` | 尽力而为 | 否（RS 侧查询） | [未核] | 维持并注释 |
| `sched/server.rs:334` | `sendnb(sender, &reply)` | 尽力而为，但丢回复＝taskcall 侧超时 | 是（fork inherit 腿，aarch64 §1.118 的通道） | C sched 非阻塞回复 [未核] | 乙 |
| `devman/ipc/minix.rs:294` | `sendrec(RS, &mut reply)`（出生应答） | 尽力而为（注释已自陈「吞掉这条」） | boot 出生链 | 无 C 直接对位（devman 属 Rust 侧新增面，登记存疑） | 维持并注释（注释已在） |
| `devman/ipc/minix.rs:379-381` | `kernel.send(source, &reply_msg(status, ...))` | 尽力而为 | 否 | 同上 | 维持并注释 |
| `devman/ipc/minix.rs:409` | `send(current_source, &reply_msg(EFAULT, ...))` | 尽力而为 | 否 | 同上 | 维持并注释 |
| `devman/ipc/minix.rs:425` | `send(current_source, &m)` | 尽力而为 | 否 | 同上 | 维持并注释 |
| `devman/ipc/minix.rs:429` | `send(dest, msg)` | 尽力而为 | 否 | 同上 | 维持并注释 |
| `ipc-server/boundary.rs:341` | `sendnb(endpoint, &msg)` | 尽力而为 | 否 | 无 C 直接对位（ipc-server 属 Rust 侧新增面） | 维持并注释 |
| `ipc-server/boundary.rs:371` | `sendnb(to, msg)` | 尽力而为 | 否 | 同上 | 维持并注释 |
| `vfs/exec_worker.rs:95` | （文档注释，非代码站） | — | — | — | 维持（注释本身是真机实证记录） |

**第二批（非 marker 路径）**：

| 站点 | 吞的调用 | ① | ② | ③ | 处置 |
|---|---|---|---|---|---|
| `rs/lib.rs:335` | `kernel.reply(who_e, result, msg)` | 尽力而为，但 RS 是出生/信号枢纽 | boot 出生链间接相关 | C rs（manager.c）回复腿 [未核] | 乙 |
| `rs/lib.rs:465` | `kernel.notify(ep)` | 尽力而为 | 否 | [未核] | 维持并注释 |
| `rs/recovery.rs:605` | `kernel.reply(endpoint, 0, default)`（LATEREPLY 清账） | 尽力而为 | 否 | C rs recovery [未核] | 维持并注释 |
| `rs/recovery.rs:1019` | `kernel.reply(caller, r, default)` | 尽力而为 | 否 | 同上 | 维持并注释 |
| `rs/shell_update.rs:192` | `kernel.reply(m.m_source, 0, m)` | 尽力而为 | 否 | [未核] | 维持并注释 |
| `rs/live_update.rs:1863` | `kernel.reply(surviving_ep, result, ...)` | 尽力而为 | 否（live-update 面） | C rs live update [未核] | 维持并注释 |
| `rs/live_update.rs:1881` | `kernel.reply(ep, EDEADEPT, ...)` | 尽力而为 | 否 | 同上 | 维持并注释 |
| `rs/live_update.rs:2130` | `kernel.reply(last_slot_ep, result, ...)` | 尽力而为 | 否 | 同上 | 维持并注释 |
| `input/serve.rs:302` | `send(Endpoint::TTY, &mut m)` | 尽力而为 | 否（input→TTY） | 无 C 直接对位 [未核] | 维持并注释 |
| `input/serve.rs:323` | `sendnb(*caller, &mut m)` | 尽力而为 | 否 | 同上 | 维持并注释 |
| `input/serve.rs:328` | `notify(*target)` | 尽力而为 | 否 | 同上 | 维持并注释 |
| `ds/server.rs:190` | `send_rec(caller, &mut reply)` | 尽力而为 | 否（DS 查询腿） | C ds 目录在（`minix3/minix/servers/ds/`），逐行未核 | 维持并注释 |
| `ds/server.rs:207` | `send(caller, &message)` | 尽力而为 | 否 | 同上 | 维持并注释 |
| `ds/server.rs:515` | `notify(ep)` | 尽力而为 | 否 | 同上 | 维持并注释 |
| `ds/server.rs:575` | `notify(source_ep)` | 尽力而为 | 否 | 同上 | 维持并注释 |

**汇总**：31 站中判「乙（诊断面下限）」＝7 站（pm/exit.rs:772/:791、pm calls.rs:633
的 B33a 一行修、vfs/main_loop.rs:1219/:7594、sched/server.rs:334、rs/lib.rs:335）；
判「维持并注释」＝23 站（含 1 测试件、1 文档注释）；判「甲（签名改造）」仍只
fs-rt 旗舰一处（D.13 现状段）。分布与三问自洽：吞错面绝大多数是「尽力而为通知/
回复」的 C 惯例形态，真正要修的是三个硬失败/关键路径点（B33a calls.rs:633、
vfs:1219、fs-rt:272）加四个「丢了会卡 marker 通道」的通知点。

## D.14 债⑬ boot bump 池无回收路径＋kerninfo 映射责任移交未落地（「结构债三件套」第二件，第 5 轮主动扫描补入）

### 现状静态刻画（第 5 轮实测）

- **登记处**：NK4C-WORKLOG「结构债三件套」（:3082/:3115/:3137 反复重申）——
  ① VM-backed heap supplier（＝债⑥ 方案乙，C `brk` 保真）；② **boot bump 可回收、
  或 kerninfo 映射责任移交 VM**（本债）；③ `MAX_BIG_BLOCKS` 同族（已于 §1.95 闭环）。
- **bump 无 free 路径**：boot-shim 的页表页池是纯 bump——`prepare_boot(1024)`
  （`os/boot-shim/src/main.rs:61`），`:53-60` 注释自证三件事：「the pool has no free
  path — the real machine died `AllocationFailed` after ~40 binds」（B26 fix-B 的
  kerninfo 注入链每次 bind 烧 1-3 页）、「this pool only postpones the deterministic
  panic」、「`vm_handoff` deducts the WHOLE region from the VM free list whether
  used or not … this costs every boot 4 MiB」。`os/kernel/src/boot_alloc.rs` 头部
  自证「Simple bump allocator」——全文件无 free API。
- **责任移交的设计已在、实施未排**：`os/kernel/src/kerninfo.rs:24-31` 注释明文：
  「C's VM maps the section into every process and tells the kernel where; this
  kernel bootstrap has no VM yet … When VM takes over [the mapping responsibility]
  moves to VM together with the rest of the user PTE work」——引导期由内核代管是
  声明过的 bootstrap 阶段妥协，移交触发点（VM 接管）已写死在设计里。
- **容量扰动解在先**：§1.55 的 B30 修复把池从 128 页扩到 1024 页——确定性 panic 从
  「~40 次 bind」推迟到更远，代价是每 boot 固定 4 MiB 从 VM 帧池永久扣除。

### 为什么是结构债

「无 free 的 bump＋整段扣除」把两次成本焊死在每一次启动上（4 MiB 帧池损失＋bind 次
数上限），且 kexec/reboot/镜像重载类功能一出现即撞天花板。C 对位（kerninfo.rs 注释
原文）里这个职责本就归 VM；Linux 的同形问题是标准的「引导内存移交」——memblock 把
init 页表/引导分配器内存归还伙伴系统（`free_init_pages` 族）。OS 理论：bootstrap
allocator 可以无 free，但必须有显式的移交（handover）时刻——本仓有设计、缺实施。

### 方案（概要级，与债⑥ 联动）

| 方案 | 机制 | 对位 | 代价/风险 | 相容 |
|---|---|---|---|---|
| 甲：kerninfo 映射责任移交 VM | 按 kerninfo.rs:24-31 既定设计，VM 接管用户 PTE 工作时一并接管 kerninfo 页映射，内核 bootstrap 的 bind 注入链消失 | C（VM 管映射）＋仓内既定设计 | VM 侧需补 publish/映射腿；`[ARCH]` 面（kerninfo 契约） | 好 |
| 乙：boot 页表页可回收 | DM 建立完成后把 bump 池剩余页归还 VM 帧池 | Linux memblock 移交 | 需追踪「哪些 bump 页仍被引用」（kerninfo blob、handoff 页） | 中 |
| 丙：容量再扰动＋账本 | 现状延续，池尺寸按 bind 上限重估 | §1.55 形状 | 成本焊死、天花板照旧 | 差（只配短期） |

### riscv64 投影与推荐

甲案（kernel-image 自当引导体）下债**随形态迁移而非消失**：boot-shim 的池换成
kernel-image/OpenSBI 库里的 `BUMP_PTR`（`opensbi_helpers.rs:364-365`，同款无 free、
同款无回收），债的本体不变。推荐：**甲**（既定设计补实施），随 VM handoff 收口批
排期；乙作为甲实施前的过渡可选。裁决归属：设计已在（kerninfo.rs），实施排期归
主线程。

### 跨债的一句话总结（第 6 轮随 D.14 移位一并移至全章文末，改十三笔）

**这十三笔债共享同一个病根——「引导与诊断的契约面在移植过程中长出了多份平行实现，
而把平行实现压回单一契约的机制（表驱动、契约测试、分层制度）在仓内已有成功先例
（xtask `uefi_slots`、check-layout L0、`tx_wait_then_send`）」**。因此每个方案的
实现成本都不高，真正稀缺的是裁决节奏与真机验证批次——这正是把它们记成结构债而非
随手修掉的原因。

**收敛声明（第 6 轮，按停止规则）**：第 5 轮新增发现 1 笔（债⑬，无 P1 级误判）、
第 6 轮处置表交付后同轮新 P0/P1＝0（唯一翻案性发现 fence.i 散点系第 5 轮已结案），
连续两轮新 P1 ≤ 1 条件满足，**§D 结构债扫描线就此收敛**——后续价值在批次实施时的
即时修正（各债「裁决归属」已定），不再安排独立扫描轮。附录「已知未覆盖」中属 §A
范围的条目（device_tree 节点审计、SMP 生效性、18-stage 命令对账、fpu/signal 全量）
按原样挂账，不属本线收敛范畴。

# E. 用户预裁决记录（2026-09-27 追加，不经扫描轮）

> 本章是用户对 §D 挂起裁决项的**条件式预裁**，由旁支会话经用户口头授权代录
> （授权语境：本文件第 6 轮收敛后，用户决定不再开扫描轮、直接改档）。目的＝把
> 「要不要开始想」这类低信息量打断从主线程的执行路径上摘掉，把用户留在真正的
> 终局裁决点上。三条纪律：其一，**预裁不是终裁**——凡依赖尚未产生的真机证据的
> 裁决一律不写死（对照 B-D1 条的显式不预裁）；其二，本章与 §D 各债的「裁决归属」
> 段冲突时**以本章为准**，冲突处由后续维护者在本章条目下补勘误注，不回改 §D 正文；
> 其三，本章条目的修订与撤销须用户本人同意，扫描线与主线程均无权单方面改本章。

## E.1 债②（x86_64 是否迁移方案 A，D.2/OQ）——半预裁

触发条件（同时满足）：aarch64 方案 A 在真机上稳定运行不少于 10 个验证轮次，且
x86_64 当时没有在途的 boot 类调试战役（避免 §1.119续-6 那类布局敏感扰动与形态迁移
叠加）。触发后**授权主线程直接立项迁移评估**（多方案对比文档，规格同 D.2），不再
询问「要不要开始想」；评估产出的终局裁决（迁移或挂起）仍报用户。

## E.2 债①（三种 boot 进入形态并存，D.1）——顺序预裁

裁定三点：riscv 甲案收口之前**不启动**三形态统一（公共腿流沙期不碰引导形态）；
收口之后以 §A.13 真机待验表的数据为前提**单独立项**，不在其它修复批次中搭车；
终态选型（统一到哪一种形态）属终局裁决，届时按立项产出报用户，本章不预裁。

## E.3 B-D1（riscv 装机与交棒形态的终态：候选 1 对 候选 2）——显式不预裁

本条是本章唯一的「不写」决定，且刻意为之：候选 1 与候选 2 的取舍依赖 riscv 真机
bring-up 的实测结果（OpenSBI/U-Boot 行为面，仓内静态分析无法替代，任务书铁律 6），
现在写死就是在制造空壳断言——misc_concepts 已登记的「裁决前置的文档拖着执行不落
最后留下空壳」正是本条要避开的形态。判据落位：§A.13 待验表跑完、D.1 立项产出时，
与 E.2 的终局裁决合并为一次咨询。

## 附：扫描方法与边界声明

- 读取面（第 1 轮）：`NK4C-WORKLOG.md` §1.78–§1.119 全量；`tmp/nk4a/fixlog_m32_p2.md`、
  `tmp/nk4a/fixlog_m32_round2.md`；`os/arch/src/riscv64/` 全部 17 文件通读（paging、
  trap_stub、trap_return、protection、trap_entry、exception、boot、smp、clock、tlb、
  signal 头部、fpu 未逐行——fpu 为惰性 FPU 设计，不在 boot 路径）；kernel 侧定点读
  （lib.rs arch_boot/with_protection/init_protection、trap_dispatch.rs riscv 段全文）；
  minix-sys ipc.rs 六原语段全文；boot-shim main/loader/opensbi_helpers 关键段全文；
  kernel-image riscv64.ld 全文与 main.rs 头部；xtask image/qemu 的 riscv 段；vm boot.rs
  与 vm_server.rs 的 total_pages/file_pages 段；drivers/tty serial.rs 通道段；init 用户态
  三处修复行。
- 读取面（第 2 轮）：`NK4B-WORKLOG.md` 2446 行精读（P0/P1/P3 M3.1–M3.4/P4 M4.1–M4.4
  含评审闭环全节）；`NK4-REGRESSION-REVIEW-20260922.md` + `-PART2.md`、
  `NK4C-REVIEW-REPORT-20260927-R3.md` 三份评审全文；`NK4C-REVIEW-REPORT-20260923.md`
  + `-R2.md`、`NK4A-REVIEW-REPORT.md`、`PATTERN-SCAN-REPORT-20260923.md`、
  `NK4B-TODO.md`、`NK4B-OPENING-PROMPT.md`、`NK4C-OPENING-PROMPT.md`、
  `NK4C-RESUME-PROMPT.md` 的 riscv 命中段；`.zcode` 六日日志 grep 零命中记录；
  四项现场复核（arch_trap.rs riscv wrapper、plat riscv64 early_console 全文、
  kernel-image main.rs 的 a1/dtb grep、riscv64/smp.rs 的 C 引用 grep）。
- 读取面（第 4 轮）：债⑨——`os/servers/vm/src/phys_mem/types.rs:87`、三分配器
  CLEAR 分支（bitmap_alloc.rs:374/buddy_alloc.rs:488/segment_tree_alloc.rs:290）、
  `region/vir_region.rs:59-71`、`cow_exec_pf.rs:281-297`、`alloc_page.rs:236`、
  `vm_server.rs:810-830`、C `vm.h:22-27`；债⑩——全仓 fence.i/cvau/iallu 零命中
  grep、`arch/boot.rs:540-625` 拷贝/清零现场；债⑪——`kernel/src/syscall.rs`
  2550-2600 与 2760-2790 全段、C `arch_do_vmctl.c:51`、arm64/x86_64 write_pte_dm
  门控行、VM crate 调用者 grep；债⑫——`os/fs/fs-rt/src/transport.rs:240-274` 全段、
  `pm/src/ipc/vfs.rs:248-275`、全仓 `let _ =`×copy/send/reply 计数（12 文件）、
  `exec_worker.rs:93-97` 旧锚点对照。
- 读取面（第 5 轮）：`os/kernel/src/arch/riscv64/higher_half.rs` 全文（fence.i 散点
  翻案）＋同位两文件 `os/kernel/src/arch/{x86_64,aarch64}/higher_half.rs` 的缓存指令/
  注释 grep（均无散点、无注释-实现不符）；修正版分模式计数 grep（fence.i=2、
  cvau/iallu=0）与第 4 轮原命令的输出流量复盘（2960 行噪声）；债⑬ 三处锚点
  （`kerninfo.rs:24-31`、`boot_alloc.rs` 头部、boot-shim main.rs:53-61 复读）；
  WORKLOG/FIXLOG 主动扫描（.review/zcode 四册 FIXLOG 的结构债标记词、DEFERRED/OQ
  家族、NK4C-WORKLOG「债」字标记全量、「结构债三件套」三处原文）。
- **第 5 轮扫描欠账登记（读取面修正）**：`os/kernel/src/arch/`（三架构各自的
  higher_half/trap/link.rs 等约 9 个文件）从未逐行对账——第 5 轮只点了 higher_half
  三个；其余文件（x86_64/ap_early、arm64/{ap_early_entry,smp} 等）继续登记为续轮
  读取面。第 4 轮债⑩ 漏检的根因不是目录盲区（原命令范围本含该目录），而是
  **未锚定模式（`dc `/`ic `）产生 2960 行子串噪声＋`head -8` 截断＋截断处未按模式
  分计数验证**——负结论出自截断输出是方法违规，教训已入 D.11 自审与移交项。
- 运行面：仅宿主只读命令。编译性证据＝三条 `cargo check`（§A.0），产物全部落在
  gitignore 的 `target/`，未触碰任何 tracked 文件；未跑 QEMU、未动测试基线、未 commit。
  第 2 轮会话期间观测到工作树出现主线程对 `os/kernel/src/ipc.rs` 的 §1.119 REPLY_PEND
  在制改动（非本线程产物），原样保留未动，仅此登记。
- 已知未覆盖（登记续轮）：`os/libs/minix-platform/src/device_tree.rs` 的节点覆盖度
  逐类审计（A.4）；`os/kernel/src/smp.rs` 的 pick/唤醒腿对 riscv 的生效性细节（单核
  marker 不依赖 SMP，§1.118 修复已架构中立覆盖调度面）；`os/commands/` 其余 18-stage
  命令的 C 翻译语义全面对账（§1.117 类错误在 init 侧已修的是 reap 环一处，其它命令
  未逐个过）；fpu.rs 与 signal.rs 的完整对账；NK4A-QWEN-WORKLOG 与 NK4A-HANDOFF-STATUS
  全文（riscv 零命中，未逐行）。
- 本线程任何「已就绪」判定都不替代主线程的真机判据（任务书铁律 6）；§A.13 真机待验表
  是两边的交接面。
