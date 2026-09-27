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
| 4 | §1.111：EL0 通道发射特权 `tlbi` → 用户态同步例外 | `os/arch/src/riscv64/paging.rs:248` `write_pte_dm` 无条件 `sfence.vma`（监督特权指令），`PteChannel` 已存在但刷新不随通道门控；x86 先例门控在 `x86_64/paging.rs:188` | 中招（VM 用户态调 map 即非法指令；单点修复，形状同 §1.111） |
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
