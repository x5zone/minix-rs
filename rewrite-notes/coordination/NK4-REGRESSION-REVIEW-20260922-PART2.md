# NK4 回归评审报告 PART2——前棒时代 + 迭代1-10 时代回填审（2026-09-22）

> 接续 `NK4-REGRESSION-REVIEW-20260922.md`（PART1，审 `6a7d513d0^..a476b86f9`）。
> 本篇回填两段未审历史：
> ①**前棒时代**（用户引用的 8 commit：8df3cfe33/5e2566777/b66c8b0e5/
>   7875926cd/cea27290a/32c3c5c54/f5a6bd436/940ad8363，2026-09-21 上午）；
> ②**迭代1-10 时代**（cd22e2e68..6a7d513d0^，18 commit：方案C 执行期
>   9 笔 fix/feat + 6 笔探针 + 分流/载体门/docs）。
> **排除**（已审）：busy-billing 11 commit（`NK4A-REVIEW-REPORT.md` §3.3
> 逐一定性）；`6a7d513d0^..a476b86f9`（PART1）。
> HEAD 仍为 3b54a36e5（与 PART1 快照一致，qwen 未新增 commit）——无增量项。

---

## 一、前棒时代（8 commit，全部首审）

| commit | 定性 | 复核要点 |
|--------|------|----------|
| 5e2566777 shim 阶段打印 | 可信 | 首亮可观测性地基；ConOut 落串口已实证 |
| 8df3cfe33 D-64② 快照顺序 | **优秀** | diff 本身极小（一行移位+注释），但根因链完整：UEFI 分配改标 LOADER_DATA 的语义 → 先拍快照则守卫误炸+活页表页进 conventional（A2）；注释与 07-paging_init_design §6.0-A2 交叉引用在位 |
| b66c8b0e5 EBS 后路标 | 可信 | |
| 7875926cd 裸写 COM1 | 可信 | uefi::println! EBS 后死锁的绕行正确；raw_serial 进 lib 半（uefi_helpers 属 lib）的归属判断正确；非 x86 空实现（后续 M3.3 由 qwen 补成 PL011 实腿） |
| cea27290a arch_boot 路标 | 可信 | minix_plat 门面用法同 debug.rs 既有惯例 |
| 32c3c5c54 panic 诊断注册前移 | **优秀** | 「三 arch 同改」核实：注册点前移到 arch_boot 入口使依赖（EarlyConsole/SMP_STATE 回退/stacktrace）在入口即可用，kmain 侧幂等——本弧线 aarch64 内核后来能打出 panic 诊断（M3.4 platform panic 可见）正是此修的红利 |
| f5a6bd436 kmain A-D 路标 | 可信 | boot_stage 宏提升模块级——本次复核确认宏体仅 mock 门+CurrentEarlyConsole 门面，**三架构通用**（qwen M3.4 实测声明成立） |
| 940ad8363 ld 跨距 2MiB 收口 | **优秀** | .lbss/.ldata 孤儿节收编 + ALIGN(0x200000) + 「改 ld 必 touch 重链」教训沉淀；NK4B M3.2 的 aarch64.ld 直接复用该契约形状 |

**前棒总评**：8 笔全部可信，其中 3 笔优秀。这一棒的核心贡献是
「首亮三断」（快照顺序/镜像跨距/无声 panic）的根因链与可观测性地基，
后续所有会话（busy-billing/方案C/qwen）都站在它铺的路标体系上。
无发现。

## 二、迭代1-10 时代（18 commit）

### 2.1 fix/feat 9 笔（新鲜眼复核）

| commit | 定性 | 复核要点 |
|--------|------|----------|
| 9f16c6470 载体架构门（F1） | 可信 | 与 ab79b40ba 同类问题的第一次修复；worktree 基线对照方法论扎实 |
| 33841ca07 落盘分流（F4） | 可信 | 384/158 分流账目清晰；**OQ 仍未决**：study-notes+archive_bak 收纳待用户（登记在案） |
| 9764d4c7e F0 GS.BASE | **优秀** | 新鲜眼复核通过：删选择子恢复的依据（MSR-owned + load_with_tss 先例）站得住；三点采样探针把"从未编程 vs 中途被清"判别开；x86 专属文件（trap_return.rs）无需跨架构门 |
| 91961877b do_exec 非致命 | 可信 | C do_exec.c:37-42 对位（名字拷贝失败非致命） |
| e4eefb386 填充链五闭合 | **优秀** | 六缺陷一 commit（当时已自评"应拆"，FIXLOG 如实）——单项质量高、commit 粒度纪律违例已登记 |
| 8a15c37a6 段基页对齐 | 可信 | gp-byte 直读探针闭环（vaddr 0x224d81 内容 = 错位证据） |
| 8c7c53ae5 停车臂+E1 核心 | 可信 | VmSuspendContext.saved_m_user 引入；递归 scheduler_loop 反模式当时已识别（迭代6 修正） |
| cb4e895a9 + 50ee0e470 + 43474883c | 可信 | T1 契约标记（smoke 契约与代码漂移的修复）；ps_strings LP64 对位；迭代8 三半 |
| ca8e6ffb3 ps_strings 布局 | 可信 | exec.h:111-116 + minix-rt repr(C) 双侧对账（前窗已验，本次抽认） |

### 2.2 探针 6 笔 + docs 2 笔

可信：全部限次+mock 门（迭代10 期已逐步形成纪律）；docs 两笔（评审报告
落盘、账本同步）与代码对应关系核实。

## 三、跨 commit 线程闭合检查（本次回填的主要价值）

### 线程 a：公共路径 x86 取证打断跨架构编译（本弧线发生两次的事故类）

- **事故一**：9e115387e 四处无门 x86 asm → F1 → 9f16c6470 修；
- **事故二**：迭代11-20 期间新增探针再度失守 → ab79b40ba（qwen）再修
  17 错（含自己探针 3 条）；
- **当前闭合性（本次 grep + 实证双验）**：HEAD 上公共路径的 x86 asm 全部
  位于 `#[cfg(target_arch = "x86_64")]` 块内（抽查 lib.rs:550 preA 探针、
  trap_dispatch.rs:386 PF 探针——后者所在 x86_trap_dispatch_body 整体
  x86 门）；**实证**：M3.2 aarch64 内核镜像产出、M3.3 boot-shim aarch64
  装机、M3.4 aarch64 内核点电（platform panic 可见）——aarch64-uefi 面
  在 HEAD 编译且运行到 kmain。
- **结构性建议（OQ-1，上交用户）**：该事故类已发生两次，靠人工纪律收口
  脆弱。建议 CI（.github/workflows/qemu-tests.yml 或独立 check job）加
  `cargo check --target aarch64-unknown-uefi -p minix-kernel`（及
  riscv64）常驻门，使该类回归在 CI 层结构性不可能。载体验证命令已在
  NK4A FIXLOG 与 ab79b40ba 验证段现成。

### 线程 b：boot_stage 路标体系三架构通用性

宏体仅 `#[cfg(not(feature = "mock"))]` + minix_plat::CurrentEarlyConsole
门面（arch 无关）——qwen M3.4「boot_stage 体系实测为三架构通用」的
声明在源码层成立。**闭合**。

### 线程 c：memmap 快照 → 身份窗口 → PMM 一致性

8df3cfe33（快照后拍）→ busy-billing 快照三移（最终 pre-EBS 快照+双守卫，
NK4A-REVIEW 已审）→ 当前 assert_bootstrap_outside_memmap 仍守 root+bump
两笔（uefi_helpers.rs:201-202）——**F8（守卫 belt-and-braces 扩展：
kernel 段+模块段两笔断言）仍开放未做**，与前轮登记一致，维持待办。

### 线程 d：RBX 三重身份全历史连贯性

前棒 940ad8363（rbx=ps_strings 出生值）→ busy-billing/KERNINFO 次级
返回通道（rbx=页地址）→ NK4-A 迭代12（RECEIVE 序言清零，rbx=干净 IPC
状态）→ 迭代20（IRQ 全量存帧保活值）→ qwen Task C 取证（三重身份
静态穷举）——五段叙事在源码层互洽，最终态：wrapper 自存自恢复用户
rbx；内核侧 rbx 三身份各用途的转换点均有明确 owner。**闭合**（该线程
是 qwen Task C 取证笔记的杰出贡献，建议作为测试学习素材保留）。

## 四、发现清单（本篇增量）

- **OQ-1**（上交用户）：CI 加 aarch64/riscv64 常驻 check 门（结构性消灭
  「公共路径 x86 取证打断跨架构编译」事故类；已发生两次，人工纪律两次
  失守）。建议归属 qemu-tests.yml 或独立 workflow，验证命令现成。
- **OQ-2**（沿用登记，仍未决）：study-notes + archive_bak 是否正式收纳
  （33841ca07 分流时上交，待用户）。
- **F8**（沿用登记）：assert_bootstrap_outside_memmap 只护 root+bump，
  kernel 段/模块段两笔 belt-and-braces 断言未做。
- 无新 P0/P1/P2 正确性问题。

## 五、方法与局限

- 与 PART1 相同：只读提交态静态审（git show），零构建/零 QEMU/零工作树
  写入（并发安全边界见 PART1 §一）；
- 前棒与迭代1-10 的「真机验证」陈述引自 FIXLOG 各条当时实录（c 系列/
  smoke 序列），本次未复跑；
- PART1（6a7d513d0^..a476b86f9）与 busy-billing 段不重复审，跨线程
  分析按需引用其结论。
