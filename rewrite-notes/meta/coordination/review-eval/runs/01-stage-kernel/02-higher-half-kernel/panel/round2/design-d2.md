# D02 R2 设计差异审查（P-DESIGN D2）

- 审查对象：`rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 的 §3、§4。
- 只读清单：上列文档 §3/§4；`os/kernel/src/arch/{x86_64,aarch64,riscv64}/higher_half.rs`；`os/kernel-image/src/bootface_common.rs`；`os/kernel/src/boot_policy.rs`；`os/arch/src/arch/paging_ext.rs`；`AGENTS.md` 的 Key Constraints。为核对文档自述的链接期与常量事实，只追加了只读 grep：三份 `os/kernel-image/*.ld`、三份架构 `paging.rs` 的大页常量、`os/arch/src/arch/paging.rs` 的 trait 成员、`os/qemu-tests` 的 `link.ld` 清单。
- 未读：作者解释、历史 panel、`panel/round1/`。R1 修复的复核依据是 `git show 1a4359c09 -- rewrite-notes/01-stage-kernel/02-higher-half-kernel.md` 的文档差异本身，不是任何说明文字。
- 未覆盖边界：`os/boot-shim/src/loader.rs`、`os/libs/minix-elf/src/lib.rs`、`os/kernel/src/lib.rs` 不在本轮只读清单内，凡依赖这三处才能判定的断言（例如 12 个错误变体的返回点计数、`boot_validate_and_prepare` 的断言原文）本轮不作结论。

## 一、R1 文档修复的复核（只评是否引入设计事实错误）

R1 在 §3/§4 范围内改了四处，逐一对照源码与工件：

1. §3.1 把「第三种形态在载体侧」改为「第二种形态的实例在载体侧」，并补「第三种形态（没有链接脚本、沿用工具链默认布局）用于其余测试载体」。复算：`find os/qemu-tests/test-kernels -name link.ld` 只列出 11 个 riscv64 载体脚本与 3 个用户态脚本，没有任何 x86_64/aarch64 内核载体脚本。修改后与工件一致，未引入错误。
2. §3.1 登记表末行标签由「载体单地址（工具链默认布局）」改为「载体工具链默认布局」。与上一处的三分法对齐，正确。
3. §3.2 把 `InvalidSegment` 的「返回点是三个」改为「不止三个」，并点名 `segment_spans` 与缓冲区装载路径另有检查。方向正确（把穷举降级为不穷举），但具体计数本轮无法复核，见「未覆盖边界」。
4. §4.0 小结把「自举根在活动根上建立并开启分页」改为「自举根由建根者建立并开启分页（v1 是 boot-shim，直核是内核镜像）；接收侧只在活动根上追加 Direct Map」。与 §3.6、§3.8、§4.5 的 v1/直核分工一致，正确。

结论：R1 的四处 §3/§4 修改未引入新的设计事实错误。

## 二、FINDING

### D2-R2-01 [MODERN] §3.4 的「`map_huge` 只执行已经验证过的写入」在自举恒等窗口这条路上不成立（P1）

锚点：文档 §3.4 末句「能力事实由 `BootPageSupport` 快照收集，选页由纯规划函数完成，`map_huge` 只执行已经验证过的写入。」

事实：能力快照与阶梯只覆盖内核镜像映射。`os/kernel/src/boot_policy.rs:117-128` 的阶梯是 `plan_kernel_mapping` 对 `KernelLayout` 的三量整除判断；文档 §4.3 自己摘出的 Step 1 循环（`os/kernel/src/lib.rs` 的 `build_bootstrap_root_and_enable`）直接写 `PageSize::MIB_2`，`DEV_PIECE = 1 << 21` 是文件级常量，既不查 `BootPageSupport`，也不查 `HugePages::supports_huge_page`。`map_huge` 实现侧同样不复核能力：文档 §3.4 摘的 x86-64 1 GiB 分支只查 `PRESENT`，没有 CPUID 门（`os/arch/src/x86_64/paging.rs` 的 `map_huge`）。

后果：§3.4 把「验证」写成整条建表路径的属性，而实际上能力门只在内核镜像那一段；恒等窗口的 2 MiB 分片是未经能力快照的无条件写入。当前三架构的 4 KiB 粒度翻译模式都支持 2 MiB 块，所以现在不会出错；但这条语句会让读者以为 Step 1 也受策略层保护，一旦引入其它粒度（例如将来 16 KiB/64 KiB 页粒度），改动者会在错误的位置找验证点。修法二选一：把该句限定为「内核镜像映射的 `map_huge` 只执行已经验证过的写入」，或在 Step 1 的 2 MiB 常量旁补一条编译期/启动期能力断言，并在 §3.6 的 Step 表里写明这条分片的验证来源。

### D2-R2-02 [MODERN] §3.7 的前置条件②与同节表格自相矛盾，且直核腿传入的不是高地址引用（P1）

锚点：文档 §3.7 正文「方法上方的 Safety 段列了三条前置条件：①…②`kinfo` 在高地址有效可访问」；同节表格第 2 行写的是「调用者保证传入地址可读；生产 `kmain` 第一条语句即遮蔽为 `.bss` 副本，形参只做交接、不解引用」。

事实：三份实现的 SAFETY 注释都逐字写「`kinfo` is valid and accessible at a high address」（`os/kernel/src/arch/x86_64/higher_half.rs:44-48`、`aarch64/higher_half.rs:38-42`、`riscv64/higher_half.rs:41-45`）。而直核腿的调用链是 `bootface_common::run_self_boot` 在栈上构造 `KernelInfo`，随后 `minix_kernel::arch_boot(&kernel_info, root_page)`（`os/kernel-image/src/bootface_common.rs:557-561`）；此时镜像仍在物理视图、分页尚未开启，这个引用是低地址/物理视图的地址，不是「高地址」。同节表格第 2 行已经把这层放宽成「可读」，正文却仍按高地址陈述。

后果：unsafe 契约的前置条件比两条生产路径实际提供的更强，读契约的人会推出「直核腿不可能合法调用」这一错误结论；这也是 §4.5「跳转时消费的是副本地址」与「形参此后无读者」两句读起来互相打架的根源。修法：把前置条件②改成「`kinfo` 在跳转这一刻的活动视图里可读；生产 `kmain` 只比较指针、不解引用，消费的是 `.bss` 副本」，并同步三份实现的 SAFETY 注释；若确实要求高地址，则必须在调用点先落副本再传副本地址，并把这个动作写进 §4.5 的路径图。

### D2-R2-03 [MODERN] §4.4 把两条对齐断言「省略」给 §3.7，但 §3.7 从未记录这两条断言（P2）

锚点：文档 §4.4 代码引用的说明「两条 16 字节对齐断言省略，论证见 §3.7」；§4.5 状态表「生产路径的静态断言见 §3.7」。

事实：三份实现各有 `assert_eq!(stack_top.0 & 0xF, 0, ...)`（x86_64 第 34-38 行、aarch64 第 33-37 行、riscv64 第 36-40 行），x86_64 另有 `assert_eq!(stack_top.0.wrapping_sub(24) & 0xF, 8, ...)`（第 39-43 行），注释写明这是 R02-T04 的生产路径 ABI 检查。§3.7 的相关文字（「栈顶的 16 字节对齐不再是调用者要传对的值：`boot_validate_and_prepare` 的断言覆盖现算的 `kern_stack_top`」）只把保证归给 `boot_validate_and_prepare`，`## 3.7` 的前置条件表也没有对齐这一行。§5.4 的 M13 反而准确记录了这两条断言。

后果：§4.4 与 §4.5 的交叉引用指向一个不存在对应内容的段落，设计章留下的空白只能由测试章补。修法：在 §3.7 增加一行「栈顶 16 字节对齐：`boot_validate_and_prepare` 的生产断言 + 三份实现的本地断言（x86_64 另查 `(top − 24) ≡ 8 (mod 16)`）」，说明两处断言的职责差异（前者查来源，后者查本地 asm 的入口 ABI），§4.4 的「省略」才有落点。

### D2-R2-04 [MODERN] `HUGE_PAGE_SIZE` 的契约注释只写 Direct Map，而 §3.4/§3.5 把它当成内核映射阶梯（P2）

锚点：文档 §3.4 逐字引用的 trait 片段，`/// Preferred huge page size for Direct Map.` 后接 `const HUGE_PAGE_SIZE: PageSize;`；§3.5 又把同一常量作为内核镜像映射的选择阶梯（「首选支持且三量整除 → 回退…」）。

事实：`os/arch/src/arch/paging_ext.rs:79-80` 的注释确实只提 Direct Map；真正的内核镜像消费者在 `os/kernel/src/boot_policy.rs:117-123`（`plan_kernel_mapping` 用 `support.preferred()`），Direct Map 消费者是另一处的 `plan_dm_policy`。同一个常量服务两个消费者，注释只点名一个。

后果：读 §3.4 会以为这个常量属于 Direct Map 层，读 §3.5 又看到它决定内核镜像粒度，两节无法拼成一份契约；排查「为什么内核镜像映射选了 2 MiB」的人会先去看 Direct Map 策略。修法：在 §3.4 的引用旁加一句「该常量的注释仍写作 Direct Map，实际消费者有两处：内核镜像映射与 Direct Map；两者的阶梯语义相同、布局输入不同」，或在源码里把注释改成中性表述后由文档照引。

### D2-R2-05 [DEL] 三份实现重复断言 `boot_validate_and_prepare` 已保证的对齐，设计章未裁决权威（P2）

锚点：§3.3「`lib.rs` 的 `fn boot_validate_and_prepare` 里有一条断言 `kern_stack_top must be 16-byte aligned`」；三份 `higher_half.rs` 的同一条断言。

事实：若 §3.3 的陈述成立，`kern_stack_top` 在到达 `jump_to_kmain` 之前已经过一次 16 字节对齐断言，三份实现再各断言一次「来源值对齐」是同一不变量的第二次强制。设计章没有说明这是有意保留的生产路径防线（注释自称 R02-T04 生产路径 ABI 检查），还是对上游断言的不信任。

后果：两处断言谁权威没有答案。上游断言被削弱时，本地断言会掩盖回归（值仍然对齐，但保证点已经不在设计声称的位置）；本地断言被删时，也没有测试会红。修法见 SUBTRACTION-TABLE 第 1 行。

## 三、ARCH-PROPOSAL

### D2-R2-A1 [ARCH] 把「已选定的页尺寸」做成类型，而不是文档里的口头承诺

现状：`HugePages` 暴露 `HUGE_PAGE_SIZES`/`supports_huge_page`/`map_huge(size)` 三件互相独立的入口；`map_huge` 接一个裸 `PageSize`，能力校验与写入之间没有类型上的联系。这正让 D2-R2-01 能成立：Step 1 直接传字面量 2 MiB，编译器不会问「这个尺寸是谁验证的」。

提议：由策略层产出一个不可伪造的「已选尺寸」令牌（例如 `KernelMapPolicy::page_size` 已经是这个角色，只是 `map_huge` 不收它），把写入入口改成 `map_huge_chosen(vaddr, paddr, chosen: SelectedSize<A>, flags)` 或让 `map_huge` 在 debug 断言 `Self::supports_huge_page(size)`；Step 1 的 2 MiB 分片改为从一个显式的 `BOOT_IDENTITY_PIECE`（附 `supports_huge_page` 的编译期或启动期断言）取得。这样「`map_huge` 只执行已经验证过的写入」从注释变成类型/断言，D2-R2-01 的修法二也就有了统一落点。

代价与边界：`HugePages` 是 arch trait，按 AGENTS.md 的抽象语义原则，令牌类型应放在 OS 语义层（`boot_policy`/`paging_ext` 的规划侧），不要把 CPUID 或粒度位带进 trait 签名；`SelectedSize<A>` 只是把已有的 `KernelMapPolicy<A>` 关联类型化，不新增硬件概念。

## 四、SUBTRACTION-TABLE

| 候选 | 证据 | 处置 | 风险 |
|---|---|---|---|
| 三份 `jump_to_kmain` 里的 `stack_top & 0xF == 0` 断言（x86_64 另有 `(top − 24) & 0xF == 8`） | `x86_64/higher_half.rs:34-43`、`aarch64/higher_half.rs:33-37`、`riscv64/higher_half.rs:36-40`；文档 §3.3 称 `boot_validate_and_prepare` 已断言同一不变量 | 二选一：删本地三条来源断言、保留 x86_64 的 psABI 算式断言，并在 §3.7 写明唯一保证点；或保留四条、删掉 §3.7「boot_validate 覆盖即可」的措辞，把本地断言升格为契约 | 若未来新增一条接收路径绕过 `boot_validate_and_prepare`，删掉本地断言会让越界栈顶直达 `kmain`；保留则需要在测试里锚定两条断言的职责 |
| §3.4 的「`Paging` 本身已有 24 个成员」并逐个点名 23 个方法 | 文档 §3.4 该段；`os/arch/src/arch/paging.rs` 的 trait 成员清单 | 删掉成员枚举，只留「职责面并不窄」的结论；计数与清单改由 trait 定义本身承担 | 读者少一份现成索引；但枚举本来就会随 trait 演进漂移，且本轮无法用一条 grep 稳定复算（成员声明跨多行） |
| §3.4 的「标识位不进入 trait」段里对「三个架构都要提供、其中两个恒为 0」的反事实铺陈 | 文档 §3.4 第（2）条；三份 `paging.rs` 均无该常量 | 保留结论（标识位留在实现内），压掉反事实假设的展开 | 教学性略降，但结论与理由（架构差异不进共有接口）仍完整 |

## 五、VERDICT

`NOT PASS`（设计轴）。R1 在 §3/§4 的四处修改未引入新的设计事实错误，链接脚本形态三分法与载体清单、`KERN_SIZE = 0x40_0000`、三架构 `HUGE_PAGE_SIZE = 1 GiB`/回退 2 MiB、`.bss` 末尾 2 MiB 收口与两条 `ASSERT`、64 KiB 引导栈符号在 `__bss_end` 前相邻赋值，这些本轮复算全部与源码/工件一致。仍存在 1 条 P1 与 4 条 P2：

- P1：D2-R2-01（§3.4 的「已验证写入」覆盖不到自举恒等窗口）；D2-R2-02（§3.7 前置条件②与同节表格、直核腿调用点三者不一致）。
- P2：D2-R2-03（§4.4/§4.5 的交叉引用无落点）；D2-R2-04（`HUGE_PAGE_SIZE` 注释与实际消费者不符）；D2-R2-05（对齐断言的双重强制未裁决）。

修完 D2-R2-01、D2-R2-02 的速度线是各改一段文字或加一条断言；D2-R2-03 需要补一行表格；D2-R2-04、D2-R2-05 可与它们同批。ARCH-PROPOSAL 不阻断本轮，只作为下一版契约的候选。

## 六、复算命令与输出（本轮实际执行）

以下为实际执行的命令与摘录输出（长输出按行号摘录，不改写内容）：

```
$ find os/qemu-tests/test-kernels -name "link.ld" | sort
  …/hello-boot-riscv64/link.ld, test-higher-half-riscv64/link.ld,
  test-kernel-map-riscv64/link.ld, test-memmap-riscv64/link.ld,
  test-paging-enable-riscv64/link.ld, test-protection-riscv64/link.ld,
  test-rt-birth-riscv64/link.ld, test-shutdown-riscv64/link.ld,
  test-smp-ipi-riscv64/link.ld, test-smp-topo-riscv64/link.ld,
  test-timer-irq-riscv64/link.ld, user/rt-birth/link.ld,
  user/sysboot-rx/link.ld, user/sysboot-tx/link.ld
  （无 x86_64/aarch64 内核载体脚本）

$ grep -n "ASSERT\|__image_end\|ALIGN(0x200000)\|KERNEL_VIRT_BASE\s*=\|KERNEL_HIGH_BASE\s*=\|KERNEL_PHYS_BASE\s*=" os/kernel-image/x86_64.ld os/kernel-image/aarch64.ld os/kernel-image/riscv64.ld
  x86_64.ld:26,27,98,99,116,118；aarch64.ld:38,39,98,99,117,119；
  riscv64.ld:41,42,105,106,125,127
  （三份都是「收口 ALIGN → __image_end → 上界 ASSERT + 2MiB 整除 ASSERT」）

$ grep -n "HUGE_PAGE_SIZE\|FALLBACK_HUGE\|HUGE_PAGE_SIZES\|fn supports_huge_page" <各架构 paging.rs>
  x86_64/paging.rs:1236-1238、riscv64/paging.rs:1182-1184、arm64/paging.rs:1166-1168
  三处均为 GIB_1 / MIB_2

$ grep -n "kernel_boot_stack_bottom\|kernel_boot_stack_top\|0x10000\|ALIGN(16)" os/kernel-image/*.ld
  三份均为 ALIGN(16) → bottom → . += 0x10000 → top，且 top 与 __bss_end 相邻赋值

$ grep -n "KERN_SIZE\|KERN_VIRT_BASE\|KERN_PHYS_BASE\|0x40_0000" os/kernel-image/src/bootface_common.rs os/kernel-image/src/bootface.rs os/kernel-image/src/bootface_a64.rs os/kernel-image/*.ld
  bootface.rs:100,103,113（0x8020_0000 / 0xFFFF_FFC0_0000_0000 / 0x40_0000）；
  bootface_a64.rs:73,76,80（0x4020_0000 / 0xFFFF_8000_0000_0000 / 0x40_0000）

$ grep -c "AT(" os/kernel-image/x86_64.ld os/kernel-image/aarch64.ld os/kernel-image/riscv64.ld
  4 / 5 / 5

$ grep -rl 'minix-elf =' os/*/Cargo.toml os/*/*/Cargo.toml
  os/arch/Cargo.toml、os/boot-shim/Cargo.toml、os/servers/rs/Cargo.toml、
  os/servers/vfs/Cargo.toml、os/servers/vm/Cargo.toml（五行）
```

对齐断言、`boot_policy.rs` 的阶梯位置、`paging_ext.rs` 的 `HUGE_PAGE_SIZE` 注释与 `BootArch::type Paging: HugePages` 绑定，来自对清单内文件的整文件阅读（`higher_half.rs` 三份、`boot_policy.rs`、`paging_ext.rs`），不是 grep 摘录。
