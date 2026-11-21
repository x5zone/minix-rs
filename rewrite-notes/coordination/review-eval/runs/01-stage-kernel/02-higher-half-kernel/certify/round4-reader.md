# D02 R4 第三次独立 certify 回读（读者面）

- 角色：冷启动读者面 certify，未参与面板、修复与前两轮。
- 目标文档：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/02-higher-half-kernel.md`。
- 最终 HEAD：`2ca9a9cd7c0b3cea80da601e498ccf8087a478fc`；该文件在 `git status --porcelain` 下无改动，回读对象是 HEAD 内容。
- 输入：`/tmp/r4-cert-reader.md` 的 12 个读者面修改点；未读 panel/round4、`minix3/`、`os/` 或历史评审。
- 方法：逐条按修改点描述回读原文并做文档内一致性核对；READER-C-05 另在 `/tmp` 临时目录做一次最小 GNU ld 行为复现（临时产物已删除，仓库无写入）。

## 逐条回读

READER-A-01：CLOSED — “迭代器在 `next()` 里做类型过滤与范围检查：只有非 `PT_LOAD` 类型过滤走 `continue`（正常过滤），其余表项范围与段形失败都以 `Some(Err(..))` 浮出（I-48/D-084）。”（§3.2）§4.2 的代码注释同口径：“I-48 / D-084: a malformed program header is an `Err` item, not a silent skip”。原先“表项解析错误走 `continue`、段数据越界走 `continue`”的两条互斥结局已收口；`SegmentIter::new` 与 `parse_phdr` 的未检查加法被单独标为 D2-01 已知残留，不再与迭代器契约混写。

READER-A-02：CLOSED — “`delta = kern_virt_base − kern_phys_base`，例如加载基址 `0x80200000` 时 `nm` 给出 `0x80200160`，而 `$rip` 是 `0xffffffc000000160`”（§6.2）。复算：`0xffffffc000000000 − 0x80200000 + 0x80200160 = 0xffffffc000000160`，与 §4.1 的 riscv64 基址 `0xFFFFFFC000000000`、§6.2 的 riscv64 L4 平移量 `0xffffffbf7fe00000` 一致。

READER-B-01：CLOSED — “riscv64 测试载体虽然也直接调 `arch_boot`，但它链接在物理地址上、`direct leg` 加 `delta`，跳转完成时 PC 落在高半 VMA（下表第 3 行）。”（§3.1）登记表数据第 3 行确为“载体单地址脚本 | riscv64 测试载体（直接 `arch_boot`） | 物理视图 | 高半 VMA（direct leg 加 delta）”；旧“第 5 行”指向的低地址测试载体行不再被误引。

READER-B-02：CLOSED — “Rust 侧把 C 内联在引导器里的这段解析搬到独立 boot-shim：boot-shim 与内核镜像分属两个可执行体，装载发生在 shim 内、由 shim 自己按 `p_paddr` 摆放”（§3.2）；同段收口句“与 C 的差别不在‘有没有外部装载者’，而在装载者是本仓代码、与内核共用同一份解析器”明确了外部装载者存在。原先“Rust 侧不存在……由另一个程序按 `p_paddr` 摆放镜像的角色”与“装载发生在 shim 内”并置的前提倒置已消失。

READER-B-03：CLOSED — 与 READER-A-01 同源：§3.2 的“只有非 `PT_LOAD` 类型过滤走 `continue`……其余表项范围与段形失败都以 `Some(Err(..))` 浮出”与 §4.2 的“畸形程序头由迭代器以 `Err` 项浮出”同口径；§3.2 代码节选中 `Err(e) => return Some(Err(e))` 取代了旧的 `Err(_) => continue`。

READER-B-04：CLOSED — “脚本不变量进断言脚本，栈顶值进派生访问器，跳转前提进 `unsafe` 契约”（§4.5）；同节表格行写作“`KernelInfo::kern_stack_top()`（`kern_virt_base + kern_size` 的派生访问器）”，与 §3.3 的“v10（D-062）之前这里曾是一个独立字段 `kern_stack_top`……字段与三个派生标量一起删除，公式只留下访问器这一份”一致，未再把栈顶说成结构体字段。

READER-C-01：CLOSED — §3.2 的代码节选已改为“`let phdr = match parse_phdr(self.image, offset, self.ehdr.e_phentsize) { Ok(p) => p, Err(e) => return Some(Err(e)), }`”，签名仍是 `Option<Self::Item>`；正文同句说明“只有非 `PT_LOAD` 类型过滤走 `continue`（正常过滤），其余表项范围与段形失败都以 `Some(Err(..))` 浮出”（§3.2）。§4.2 的调用方 `let seg = seg_result?;` 与注释“malformed program header is an `Err` item, not a silent skip”与之一致。

READER-C-02：CLOSED — 与 READER-B-01 同源；“（下表第 3 行）”（§3.1）与表内第 3 行 riscv64 测试载体的“高半 VMA（direct leg 加 delta）”逐项对应，旧“第 5 行”无残留。

READER-C-03：CLOSED — 与 READER-B-04 同源；§4.5 的共同点句已由“栈顶值进结构体字段”改为“栈顶值进派生访问器”，表格行也以 `KernelInfo::kern_stack_top()` 访问器为落点，不再与 §3.3 的“字段已删除、公式只留在访问器”冲突。

READER-C-04：CLOSED — 与 READER-A-02 同源；“例如加载基址 `0x80200000` 时 `nm` 给出 `0x80200160`，而 `$rip` 是 `0xffffffc000000160`”（§6.2）与 delta 算式一致，旧的错误读数 `0xffffffc000200160` 在全文已无命中。

READER-C-05：CLOSED — “旧工件的 `readelf -lW` 显示该孤儿段的 VMA=`0xffff800000290000`、LMA=`0x290000`，平移量与其它段一致，所以 L4 仍通过——GNU ld 对无 `AT()` 的孤儿段按相邻段继承 LMA 平移，不套用 §2.1 的‘无 `AT()` 即 VMA=LMA’默认规则；该段又落在 2 MiB 收口之前，所以 L5 也通过。”（§4.1）任务要求的三件都在：旧工件 VMA/LMA 两个读数、L4 仍通过的理由、与 §2.1“不给 `AT()` 的输出段，它的装载地址与虚拟地址相同”（§2.1）的例外关系。独立复核：在 `/tmp` 用 GNU ld 2.42 做最小链接，脚本只给 `.text`/`.rodata`/`.data` 写 `AT()`，未匹配的 `.lrodata.foo` 孤儿节得到 VMA=`0xffff800000200014`、LMA=`0x200014`，确实继承相邻 `.rodata` 的平移量 `0xffff800000000000`，而不是退化成 VMA=LMA。

READER-C-06：CLOSED — §3.4 已把坏情形写成三种：“真正会坏的有三种情形：①三者都去掉时，一个非 16 对齐的 `stack_top` 会一路传到 `kmain`；②只去掉 `sub rsp, 8` 也会坏……③只去掉 `push 0` 时终止槽缺失，且 `call` 前 RSP 由 `and` 后的 16 边界直接减 8（`call` 压返回地址），入口 RSP 变成 0 mod 16，同样违反 System V 的 `(RSP+8) % 16 == 0`，`sp_aligned` 会在真机上当场红。”（§3.4）§4.3 的括号句“去掉它改变的是入口栈的对齐，`sp_aligned` 断言会当场失败”（§4.3）与第三种情形同结论；`push 0` 仍保留“终止槽、不在帧指针链上”的描述，但已不再能被读成可删的遗留物。

## 汇总

12 条全部 CLOSED，0 条 STILL-BROKEN。重点残项 READER-C-05 已补上旧工件的 VMA/LMA 读数，并明确写出“GNU ld 对无 `AT()` 的孤儿段继承相邻段 LMA 平移，不套用 §2.1 默认规则”，与文档内 §2.1、§6.2 的 L4/L5 口径一致；独立最小链接实验也只支持这一方向，没有发现反证。

## 局限

1. `/tmp/r4-cert-reader.md` 中 READER-C-05 的“期望复核点”仍是模板串（`grep -n '栈顶值进派生访问器'`，处置句与 B-04/C-03 相同），与该条目的实际描述不符。本次按条目描述回读 §4.1 的孤儿段句，而不是以该 grep 判 CLOSED。
2. 未读 panel/round4、`minix3/`、`os/` 与历史评审；文档引用的旧工件事实（`readelf` 读数、986 个节、`PT_LOAD` 从 5 个回到 4 个）只做了文档内一致性与算术复核，没有对旧工件重新取证。
3. GNU ld 行为复现只覆盖“孤儿节继承相邻段 LMA 平移”这一点（宿主 binutils 2.42，合成的三输出段脚本，孤儿节与 `.rodata` 合并在同一 `PT_LOAD`），没有复现旧工件的“独立 `PT_LOAD`”布局；C-05 的通过依据是“句子已交代关系且技术方向可复现”，不是对历史工件的完整重放。
4. 本次是单 agent 冷读回读，未做跨工具复核；READER-A-01、READER-B-03、READER-C-01 涉及的 D2-01（`SegmentIter::new` 与 `parse_phdr` 的未检查加法，debug 下 panic）仍是文档明确标注的代码债，不是本轮读者面冲突的残留。
5. 未运行文档链接的宿主测试或真机链路；本轮范围是读者面文字回读。

- 结论：PASS（12/12 CLOSED，0 STILL-BROKEN）。

- 遗留 P0: 0 / P1: 0
