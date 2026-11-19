# P-FACT 独立事实审查：02-higher-half-kernel.md

审查对象：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`（1280 行）。
机械门读数沿用实测值（anchor 55/55；citation blocks 32 / checked 25 / failed 0 / skipped 4 / warned 3；structure/ref/style/density 通过；doc-code-map 28/28）。本次另做源码抽查，未发现“锚点解析不到但事实重要”的断言。

---

## FINDING RD3-FACT-01（P2）

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1072`（§4.4 状态表“程序计数器”行）
- **严重度**：P2
- **问题**：该单元格写“aarch64 direct-core：低地址进入（`ldr` 取高半绝对值跳）”。表头是“跳转完成时的值”，而 §3.1 表（同篇 `:371`）、§4.4 表（同篇 `:1033`）与实现（`os/kernel/src/arch/aarch64/higher_half.rs:48-49`：`ldr x1, ={kmain}` / `br x1`）都表明跳转完成时 PC 是 `kmain` 的链接期高半 VMA，不是低地址。“低地址进入”是 bootface 的入口态，不是 `jump_to_kmain` 完成态。
- **影响**：按状态表读会把 aarch64 direct-core 的 PC 判据理解反；同一篇的 §3.1、§4.3、§4.4 其余三处与该单元格自相矛盾。
- **证据或推理**：E1。`higher_half.rs:48-49` 取链接期绝对值后 `br`；§3.1 `:371` 同行“跳转完成时 PC = 高半 VMA（`ldr` 取绝对值）”；§4.4 `:1033` 同表“高半 VMA（`ldr` 取绝对值）”。
- **最小修复**：改为“`kmain` 的高半 VMA（`ldr` 取绝对值跳；低地址只是进入 bootface 的入口态）”。
- **置信度**：高
- **是否需源码核实**：是（已核，`os/kernel/src/arch/aarch64/higher_half.rs:48-49`）
- **若我错了**：若“低地址进入”指 `_start` 入口 PC 而非 `jump_to_kmain` 完成后的 PC，则表头“跳转完成时的值”和同表其他行都需改写；否则本行应改。

---

## FINDING RD3-FACT-02（P2）

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1228-1229`、`:1239`（§6.2 第五组预期输出）
- **严重度**：P2
- **问题**：文档说 `$rip` 应与 `nm <工件> | grep kmain` 的符号值一致，并把 riscv64 测试载体列为“高半 VMA”。但该载体链接在 `0x80200000`（`os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-riscv64/link.ld:6`），`nm` 的 `kmain` 是 `0x80200160`；该载体把 `kern_virt_base = 0xFFFF_FFC0_0000_0000`、`kern_phys_base = DRAM_BASE = 0x8000_0000`（`os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-riscv64/src/main.rs:118,128`），direct leg 加 `delta = 0xFFFFFFBF80000000` 后 PC = `0xFFFFFFC000200160`。二者相差 `delta`，不相等。
- **影响**：按该命令在 riscv64 测试载体上核对会得到“符号值 ≠ PC”，误判为交接失败；诊断步骤在该架构上不可用。
- **证据或推理**：E1。`nm os/target/riscv64gc-unknown-none-elf/release/test-higher-half-riscv64 | grep kmain` 输出 `0000000080200160 T _ZN12minix_kernel5kmain...`；`readelf -h` 入口 `0x80200000`；`src/main.rs:118,128`；`os/kernel/src/arch/riscv64/higher_half.rs` 的 `add t1, t1, t2`；复算：`0xFFFFFFBF80000000 + 0x80200160 = 0xFFFFFFC000200160`。
- **最小修复**：对该载体改为“`$rip` = `nm` 的 `kmain` + (`kern_virt_base` − `kern_phys_base`)”；或把 nm 等式限定为“生产镜像与 x86_64/aarch64 测试载体”，riscv64 测试载体单列差值算式。
- **置信度**：高
- **是否需源码核实**：是（已核）
- **若我错了**：若 `nm` 的“工件”指生产 `kernel.elf` 而非被调试的 riscv64 测试载体，等式可成立；但文档明确把“riscv64 测试载体”写进同一句，工件只能是该载体。

---

## FINDING RD3-FACT-03（P2）

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:755`（§4.1 基址反事实）
- **严重度**：P2
- **问题**：文档断言“引导期的恒等映射已经在那里放了 1GiB 叶子”，并据此说 `map_huge` 会以 `AlreadyMapped` 拦住把基址写成 `0xFFFFFC0000000000` 的错误。实际 `build_bootstrap_root_and_enable` 的 Step 1 恒等映射固定用 2MiB 叶子（`os/kernel/src/lib.rs:1268-1271`，`PageSize::MIB_2`），代码注释明确“Step 1's fresh root never creates one [1 GiB leaf]”（`os/kernel/src/lib.rs:1262-1264`）。把基址改成 `0xFFFFFC...` 后，内核策略选中的仍是 2MiB（`kern_phys_base = 0x80200000` 不是 1GiB 对齐）；riscv64 的 `map_huge` 2MiB 路径在已有 L1 表项上直接写叶子，不检查已有叶子（`os/arch/src/riscv64/paging.rs:1184-1193`），所以会静默覆盖恒等映射，而不是返回 `AlreadyMapped`。
- **影响**：文档把一条并不存在的保护当成结论。读者会以为这个基址错误能在建表阶段被拦下；实际是静默破坏，且被覆盖的可能正是当前执行页。“覆盖范围从 1GiB 缩到 2MiB”也不符合当前 Step 1 的 2MiB 布局。
- **证据或推理**：E1。`os/kernel/src/lib.rs:1262-1264`、`:1268-1271`；`os/arch/src/riscv64/paging.rs:1184-1193`；`os/kernel/src/boot_policy.rs:120-132` 的 2MiB 选择条件。
- **最小修复**：改写该反事实：Step 1 是 2MiB 叶子；2MiB `map_huge` 会直接覆盖 L1 叶子且不报 `AlreadyMapped`，结果是静默破坏恒等映射。删掉“1GiB→2MiB”的收缩描述与“`AlreadyMapped` 提前报出”的结论；若要保留 1GiB 假设，必须明确标成假设而非当前实现。
- **置信度**：高
- **是否需源码核实**：是（已核）
- **若我错了**：若存在另一条启动路径（不是 `build_bootstrap_root_and_enable`）先放 1GiB 恒等叶子，请给出该路径的 path:line；当前仓内未找到。

---

## FINDING RD3-FACT-04（P3）

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:612`（§3.5）
- **严重度**：P3
- **问题**：文档把 panic 消息写成 `arch_boot: page-size policy: no kernel page size satisfies the layout`。实际 `arch_boot_impl` 调用 `plan_boot_pages`，其错误是 `BootPagePolicyError::Kernel(KernelMapError::NoKernelPageSize)`；Display 为 `kernel mapping policy: no kernel page size satisfies the layout`，完整 panic 是 `arch_boot: page-size policy: kernel mapping policy: no kernel page size satisfies the layout`（`os/kernel/src/lib.rs:946-947`；`os/kernel/src/boot_policy.rs:189-192`、`:51`）。另外 v1 路径的同一错误前缀是 `enter_kernel_image_v1: page-size policy: …`（`os/kernel/src/lib.rs:458-459`），不是 `arch_boot:`。
- **影响**：按整串 grep 会漏；读者也无法区分 direct 与 v1 两条腿的同名错误。§6.1 只引叶子消息 `no kernel page size satisfies the layout`，那一条是准确的。
- **证据或推理**：E1。上述 path:line。
- **最小修复**：在 §3.5 引用完整 direct 路径字符串（含 `kernel mapping policy:`），或只引叶子消息并说明 v1 前缀不同。
- **置信度**：高
- **是否需源码核实**：是（已核）
- **若我错了**：若 `BootPagePolicyError` 的 Display 在某个版本中直接转发 `KernelMapError` 而不加前缀，则原串成立；当前源码不是。

---

## FINDING RD3-FACT-05（P3）

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:562`（§3.4）
- **严重度**：P3
- **问题**：正文说“（表末一行是返回类型，与四条并列列出）”，但紧随其后的表（同篇 `:564-570`）最后一行是“只调用一次”；返回类型“不返回”在倒数第二行（`:569`）。表也不是“四条 + 返回类型”的 5 行顺序，而是三条前置 + 返回类型 + 第四条前置。
- **影响**：读者按“表末一行”找返回类型会找错；表结构与正文描述不一致。
- **证据或推理**：E1（文档内部，`:564-570`）。
- **最小修复**：改为“表中一行是返回类型，与四条前置条件并列”，或把“不返回”行移到最后。
- **置信度**：高
- **是否需源码核实**：否（文档内部）
- **若我错了**：若“表末一行”指某个我不认识的版式约定，请指出；按当前 Markdown 表格它指“只调用一次”行。

---

## FINDING RD3-FACT-06（P3）

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:994`（§4.3）
- **严重度**：P3
- **问题**：文档写“源码注释把这条记录为‘内核从未迁到它的高半 VMA’”。在相关源码中找不到这句（或等价英文）注释。`os/kernel/src/arch/riscv64/higher_half.rs:41-52` 的实际注释说的是 direct leg “still executes below `kern_virt_base` (identity window)”，补 `delta` 后 “landing PC at kmain's true high-half VMA”，没有“从未迁移”的表述。全仓 `grep -rn '从未迁到\|从未迁移到\|never migrated'` 只命中本文件 `:994`。
- **影响**：带引号的“源码注释”无法在源码中复核；读者会以为这是原话。
- **证据或推理**：E1。`higher_half.rs:41-52`；全仓 grep 结果只有本文件。
- **最小修复**：改成实际注释的转述（“注释说 direct leg 仍执行在 `kern_virt_base` 之下，需补 `delta` 才落到高半 VMA”），或把引号去掉标成“大意”。
- **置信度**：高
- **是否需源码核实**：是（已核）
- **若我错了**：若该句在未纳入本次检索的文件（如某次提交说明）里，那也不是“源码注释”；请给出 path:line。

---

## FINDING RD3-FACT-07（P3）

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1230`、`:1239`（§6.2 第五组）
- **严重度**：P3
- **问题**：预期输出写“`$rsp` 落在 `kern_virt_base` 之上（`0xffff8000…` 一段）”。这是 x86_64/aarch64 的前缀；riscv64 生产镜像的 `kern_virt_base` 是 `0xFFFFFFC000000000`（`os/kernel-image/riscv64.ld:39`），riscv64 测试载体的 `kern_stack_top` 是 `0xFFFFFFC004000000`（`os/qemu-tests/test-kernels/kernel/bootstrap/test-higher-half-riscv64/src/main.rs:132`）。riscv64 上不会出现 `0xffff8000…` 的 SP。
- **影响**：按该示例在 riscv64 上查 SP 会看错地址段，误判“切栈未生效”。
- **证据或推理**：E1。上述 path:line；`kern_virt_base` 的定义与 verify 断言。
- **最小修复**：改为“按架构：x86_64 为 `0xffff8000…` 之上；aarch64 为 `0xffff8000…` 之上；riscv64 为 `0xffffffc0…` 之上”。
- **置信度**：高
- **是否需源码核实**：是（已核）
- **若我错了**：若第五组被明确限定为 x86_64 专用，请在该组标题/命令注明；当前它写的是“确认跳转后 PC 与 SP 各落在哪一侧”，覆盖三架构。

---

## FINDING RD3-FACT-08（P3）

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:519`（§3.3）
- **严重度**：P3
- **问题**：文档写“`uefi_helpers.rs` 与 `opensbi_helpers.rs` 的测试模块只 `use crate::build_kernel_info`”。字面上不成立：两个测试模块还 `use super::*`、`use minix_boot::…`、`use minix_types::…` 等（`os/boot-shim/src/uefi_helpers.rs:1301-1306`；`os/boot-shim/src/opensbi_helpers.rs:669-673`）。文档想说明的“只有一份 builder 实现、两个测试模块都引用它”是对的。
- **影响**：把“只”读成“除它之外没有别的导入”会得到错误事实；这是一句可被 grep 直接反证的表述。
- **证据或推理**：E1。上述 path:line。
- **最小修复**：改为“两个测试模块都 `use crate::build_kernel_info`（此外还 `use super::*` 等），共享同一份 builder 实现”。
- **置信度**：高
- **是否需源码核实**：是（已核）
- **若我错了**：若“只”修饰的是“从 crate 根导入的只有 build_kernel_info”（`use super::*` 不算 crate 根导入），请改写成这个更窄的限定；当前字面含义更宽。

---

## SUGGESTION RD3-SUG-01

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:627-632`（§3.5 引用的 `riscv64.ld` 注释）
- **问题**：引用的链接脚本注释说“boot_validate_and_prepare 按 huge-page 阶梯选映射粒度”，但实际选粒度的是 `plan_kernel_mapping`（`os/kernel/src/boot_policy.rs:98-133`）；`boot_validate_and_prepare` 只采集能力事实并注册分配器（`os/kernel/src/lib.rs:839-910`）。文档正文（`:595`）说对了，但引文块把源码里的这句旧注释原样传播。
- **建议**：在引文块后加一句“脚本注释这句与当前实现不符：选粒度在 `plan_kernel_mapping`，`boot_validate_and_prepare` 只采集能力事实”，或只引用该注释的后半段。
- **置信度**：高
- **是否需源码核实**：是（已核）

## SUGGESTION RD3-SUG-02

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1226-1227`、`:1239`（§6.2 第五组 GDB 命令）
- **问题**：命令用 `$rip`/`$rsp`（x86_64 寄存器名），但该组声称覆盖“跳转后 PC 与 SP 各落在哪一侧”，对 aarch64/riscv64 应分别是 `$pc`/`$sp`。
- **建议**：注明“以下寄存器名为 x86_64；aarch64/riscv64 用 `$pc`/`$sp`”，或按架构给两条命令。
- **置信度**：高
- **是否需源码核实**：否（GDB 常识）

## SUGGESTION RD3-SUG-03

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:356`（§3.1）
- **问题**：复算声明“`grep -rn 'src/arch/.*link.ld' os/qemu-tests/*.sh os/xtask/src/*.rs` 无命中；把检索面扩到全仓的 `*.rs`/`*.toml`/`*.sh`/Makefile 后仍无命中——只有注释引用脚本里的常量”。第一部分（qemu/xtask）确实无命中；扩到全仓后实际有 5 行命中，且都在注释里：`os/kernel/src/lib.rs:6220`、`os/kernel/tests/link_ld_lockstep.rs:3`、`os/arch/src/arch/direct_map.rs:244,296,350`。文档括号里“只有注释引用”是对的，但“仍无命中”与命令输出矛盾。
- **建议**：改成“无构建输入命中；全仓扩展检索只有 5 处注释引用（列出 path:line）”，或给命令加 `--exclude`/过滤注释后再声称无命中。
- **置信度**：高
- **是否需源码核实**：是（已核，命令输出见上）

## SUGGESTION RD3-SUG-04

- **位置**：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md:1217`（§6.2 第四组）
- **问题**：文档说 `qemu_int.log` 的 `check_exception` 行“含出错类型与故障地址”。QEMU `-d int` 的 `check_exception old: … new 0xe` 行只给向量（出错类型）；故障地址在紧随其后的异常明细行（`v=… e=… CR2=…` 或 `stval`/`pc` 字段）。
- **建议**：改成“`check_exception` 行给向量；故障地址在随后一行（x86 看 `CR2`，arm/riscv 看相应字段）”，或把“含故障地址”移到下一行。
- **置信度**：中（QEMU 版本可能影响格式）
- **是否需源码核实**：是（建议在目标 QEMU 版本上核一次实测日志）

---

- 本次扫描：8 条 FINDING，4 条 SUGGESTION。
