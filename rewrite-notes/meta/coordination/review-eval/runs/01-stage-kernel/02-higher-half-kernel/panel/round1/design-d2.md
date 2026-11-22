# D02 设计差异审查（P-DESIGN D2）

> 角色：设计差异审查，只读。输入：`panel/round1/design-d1.md` 的 D1–D7、`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` §3.4/§3.5/§3.7/§3.8、三份 `os/kernel/src/arch/*/higher_half.rs`、`os/kernel-image/src/bootface_common.rs`、`os/boot-shim/src/uefi_helpers.rs`（仅跳转/建根段）、`AGENTS.md` Key Constraints。未读文档第 4 章以后、其他评审产物。

## DIFF-TABLE

| 轴 | 现有实现 | D1 设计 | 差异性质 |
|---|---|---|---|
| A 自举根 / 追加 DM | 根在三个边界上都是裸 `PhysBytes`：shim `alloc_root_page()` → `enter_kernel_image_v1(arena_pa, &report, root_page, &shim_windows)`（`os/boot-shim/src/uefi_helpers.rs:657,1279-1283`）；直核面 bump 池取一页 → `arch_boot(&kernel_info, root_page)`（`os/kernel-image/src/bootface_common.rs:449-463,561`）；"已激活"只由所在腿隐含（`os/kernel/src/lib.rs:299 fn resume_additive` 走 `from_active_root`，`os/kernel/src/lib.rs:214 fn arch_boot_finish` 走建根腿），无类型标记 | `BootstrapRoot<Unbuilt>` → `BootstrapRoot<Ready>` → `ActiveRoot`，`map` 只在 `Unbuilt` 上，`activate` 消费 `Ready`；根帧由 `BootstrapRoot` 从 `&'static mut` 切片独占，不走堆；通用层拥有 `identity → kernel → device → activate → enter` 次序，架构层只经 `BootMapping` 写表（`design-d1.md` §2.2/§2.4/§2.7 D4） | 载体级差异为主：外部行为等价（v1 腿仍"发送侧建根、接收侧只 additive"，与 §3.8 表格同构）。**但** D1 的 D7（恒等窗口收窄到引导足迹）与 D6（外部字节 `Result`）是行为可见项，不能算进 D4 的载体账 |
| B 页尺寸 1GiB/2MiB/4KiB | 文档侧决策：`HugePages: Paging` 扩展 trait 带 `HUGE_PAGE_SIZES`/`HUGE_PAGE_SIZE`/`FALLBACK_HUGE_PAGE_SIZE`/`supports_huge_page`/`map_huge`（`os/arch/src/arch/paging_ext.rs:trait HugePages`，文档 §3.4 摘录）；选页阶梯在 `os/kernel/src/boot_policy.rs:98 fn plan_kernel_mapping`（文档 §3.5）：首选 1GiB 且三量整除 → 回退 2MiB 且三量整除 → 否则 panic；4KiB 档被文档明确否决（§3.5 取舍表第二行"不收口，运行时用 4KiB 细粒度建表：否"） | `design-d1.md` 全文**没有** `HUGE_PAGE_SIZE`/`PageSize` 概念：粒度只作为 `ImageLayout` 的校验谓词（"跨距不满足粒度"是 `BootError` 变体，`design-d1.md` §2.5），块尺寸与 PS 位留在架构实现（§2.7、§5 的"4 MiB 大页与 PSE/PGE 舞蹈"一行） | 不是同一决策的两个方案：D1 概念更少但错误面不同（`Result` vs 启动期 panic），且 D1 不承担"首选/回退阶梯"这条策略。题面把 `HUGE_PAGE_SIZE` 记在 D1 名下与文本不符 |
| C 换栈 / 跳转契约 | `HigherHalf::jump_to_kmain(kinfo: &KernelInfo) -> !`（`os/kernel/src/boot/higher_half.rs:trait HigherHalf`）；入口地址以 `sym kmain` 在编译期焊进汇编（三份 `higher_half.rs` 的 `kmain = sym kmain`）；断言只覆盖对齐：x86_64 两条（`os/kernel/src/arch/x86_64/higher_half.rs:34-43`），aarch64 一条（`aarch64/higher_half.rs:33-37`），riscv64 一条加 `checked_sub` 高阶不变式（`riscv64/higher_half.rs:36-40,105-109`）；"只调用一次"仅在 Safety 文档段 | `BootEnter::enter(EnterTarget{entry, stack_top, arg}) -> !`，`entry` 是值而非符号；`BootCpu` 线性 token（`!Send`，建表与进入取 `&mut BootCpu`）使"引导期独占/调用一次"成为类型事实；debug 构建断言中断状态（x86 EFLAGS.IF / ARM DAIF / RISC-V sstatus.SIE）作为 J4 前提证据（`design-d1.md` §2.6、§2.7 D5） | 类型保证的**范围**差异：D1 把"不返回"（两方共有）扩展到"独占/一次"，且把 entry 值化以获得可 mock 性；两者对"换栈先于任何压栈"都只有汇编语句顺序保证，无类型、无断言 |

## FINDING

1. **[MODERN] 根的"建成 vs 已激活"在现有类型里不存在，是真实可消除的错位面。** 三条腿（UEFI shim、U-Boot shim 的 `enter_kernel_image_v1`、直核面 `arch_boot`）都把根当裸 `PhysBytes` 传递，"这条腿的根是否已激活、能否重建"只写在调用点注释与文档 §3.8 里；`resume_additive` 之所以正确，靠的是它选了 `from_active_root`（`os/kernel/src/lib.rs:299`、`1752`），而不是类型禁止它调用建根。D1 的 `BootstrapRoot<Ready> → ActiveRoot` 让"接收侧重 tb 根"不可写出，这是载体级改进、不改变外部行为，建议只吸收这一点；不要连带 D7（恒等窗口收窄）——那条改变页错误语义，属独立行为决策。

2. **[DEL] `HUGE_PAGE_SIZES` 的首档 1GiB 对当前三工件不可达，属可删除的声明盈余。** 文档 §3.5 第一行自承三个 `kern_phys_base`（`0x200000`/`0x40200000`/`0x80200000`）都非 1GiB 对齐，`kern_size` 又非 1GiB 整数倍，故首选档永远命中不了；同表第二行又把 4KiB 档明确否决。于是 `[GIB_1, MIB_2]` 的阶梯实际只剩一档，"首选/回退"结构是为不可达状态付的复杂度。另：`FALLBACK_HUGE_PAGE_SIZE ∈ HUGE_PAGE_SIZES` 在 trait 片段（`os/arch/src/arch/paging_ext.rs:trait HugePages`）里没有 const 断言，三个常量之间的成员关系只是文档约定——这不是非法状态（`plan_kernel_mapping` 的 `FallbackUnsupported` 兜住了），但没有编译期约束。若把阶梯挪进 D1，还必须把"粒度不满足"从 panic 改回 `Result`（D1 §2.5），否则 D1 的错误分层自相矛盾。

3. **[ARCH] 入口地址被 `sym kmain` 焊死，`HigherHalf` 不可宿主 mock；D1 的 `EnterTarget.entry` 值化解开这一层。** 三份实现都在汇编里引用 `kmain` 符号（`x86_64/higher_half.rs:71` 等），于是 trait 的"可测试性"只到签名层：无法在宿主断言"传给进入的入口/栈/参数"。D1 把三者装进 `EnterTarget` 并在 `MockEnter` 上断言，是抽象边界（符号 vs 值）的实质差异。注意 D1 的 token 只覆盖"调用一次"，不覆盖"栈顶在已映射窗口内"——后者现有与 D1 都只剩文档/注释（文档 §3.7 自承 x86_64 的"调用点 PC = 链接地址"无运行期检查），这一条至少应在宿主 mock 层补断言，或在 `EnterTarget` 构造处断言。

## SUBTRACTION-TABLE

| 可删项 | 现位置 | D1 替代物 | 理由 |
|---|---|---|---|
| `root_page: PhysBytes` 形参在三处签名中各写一遍 | `os/kernel/src/lib.rs:214`、`os/kernel-image/src/bootface_common.rs:561`、`os/boot-shim/src/uefi_helpers.rs:1279` | `ActiveRoot` 句柄 | 同一事实三处可各自错位，无类型阻止传错页 |
| 三份 `kinfo.kern_stack_top()` 派生 + 对齐断言 | `x86_64/higher_half.rs:29-43`、`aarch64:29-37`、`riscv64:32-40` | `EnterTarget` 在通用层算一次并断言 | 现已无"第二来源"，但三份复制意味着三处可漂移 |
| 1GiB 首选档及其选择分支 | `os/kernel/src/boot_policy.rs:98 fn plan_kernel_mapping` 与三份 `paging.rs` 常量 | 无（直接 2MiB 档） | 当前无工件可达（文档 §3.5 第 1 行），声明大于可达状态 |

## VERDICT

**PARTIAL / 不推翻现有外部行为。** A 项：D1 与现实现外部行为等价，差异集中在承载（根 typestate、根帧所有权、建根次序归属），其中只有 D7/D6 是行为可见项，须与 D4 分开记账。B 项：D1 根本没有页尺寸概念，不能当作 `HUGE_PAGE_SIZE` 扩展 trait 的对照方案；现有文档阶梯的问题是不可达的 1GiB 档与缺失的常量成员约束，不是非法状态。C 项：`-> !` 与三份对齐断言只保证"不返回 + 栈对齐"，"换栈成对、只调用一次、栈已映射"仍靠汇编顺序与文档；D1 的 `BootCpu` token 与 `EnterTarget` 值化是值得吸收的两点，但 token 不覆盖栈映射，需另补断言。建议：吸收根 typestate 与 EnterTarget/BootCpu；驳回"把 HUGE_PAGE_SIZE 阶梯并入 D1"；1GiB 档按删除候选处理。
