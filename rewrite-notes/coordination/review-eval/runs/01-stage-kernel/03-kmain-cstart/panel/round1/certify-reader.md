# D03 R1 certify 读者回读者

## 0. 材料、读取范围与取证

- 正文：`/home/xzhao/github/minix-rs/rewrite-notes/01-stage-kernel/03-kmain-cstart.md`，1384 行。
- sha256：`bf055daf0fb3d950cab51e81463651c5671c0c1df2d15aeea3023f3e1bf7e82f`，与任务给的 `bf055daf` 前缀一致。
- 读者画像：`/home/xzhao/github/minix-rs/tmp/review-eval/01-stage-kernel/03-kmain-cstart/reader-profile.md`。
- 只读范围：§1.2（38–57）、§2.1（96–180）、§2.4 的 IOPL 行（456）、§2.6（594–670）、§3.2（778–806）、§3.3（807–843）、§3.7（928–952）、§4.6（1135–1182）、§4.7（1183–1203）、§5.3（1251–1262，仅用于核对 §5.4 的缺口归属）、§5.4（1263–1281），以及 `git diff` 的改动段。未读源码、其他文档、其他面板、`expected/`。
- 验证方式：同 agent 只读复核，下面每条都重放了定位命令；算术为手工复算并写出中间值。

```text
$ sha256sum rewrite-notes/01-stage-kernel/03-kmain-cstart.md
bf055daf0fb3d950cab51e81463651c5671c0c1df2d15aeea3023f3e1bf7e82f  rewrite-notes/01-stage-kernel/03-kmain-cstart.md

$ git diff --stat -- rewrite-notes/01-stage-kernel/03-kmain-cstart.md
 1 file changed, 27 insertions(+), 25 deletions(-)

$ grep -n 'local_cbi\|暂存副本\|最终全局' rewrite-notes/01-stage-kernel/03-kmain-cstart.md
141:第二个动作的两行 `memcpy` 不是自拷贝：……`kmain` 的 `local_cbi` 指向的也是这份最终全局。……
179:**把第二步的两行 `memcpy` 删掉会怎样**：最终 `.usermapped` 全局保持全零，……

$ grep -n '原地重填\|同一数组\|TLB' rewrite-notes/01-stage-kernel/03-kmain-cstart.md
615:……`pg_load()` 最后把这张新表的物理地址写进 CR3，从旧表切到新表——不是同一数组的原地重填。
1275:| M9 | 重建页表的原地重填（§2.6，C 侧） | 清零→重填窗口内取指可行（TLB 旧条目）；……

$ grep -n '三问\|三份答案\|①和③\|③ 还带' rewrite-notes/01-stage-kernel/03-kmain-cstart.md
40:三份答案落在三张表上，每张表的装填方式由它回答的问题决定：
51:三张表的一个差别是：②是逐条目不同的……①和③各只有一套（DPL 存在少数几个描述符里、`sp0` 每 CPU 一个字段）。
53:③ 还带一个“内核态里又出事用哪根栈”的子问题……
1174:把三架构的 Rust 落点按 §1.2 的三问收一张表：
1176:| 三问 | x86-64 | aarch64 | riscv64 |
```

## 1. 逐段裁决

| 段落 | 裁决 | 依据 |
|---|---|---|
| §2.1 两行 `memcpy`（141，邻 144/179） | **REOPEN** | 141 的 `local_cbi` 残句与同段“不是自拷贝”的论点、179 的删除反事实都冲突；144 的“自拷贝论证”仍是旧措辞。见 RD3-CERT-READER-01。 |
| §2.1 `kernel_may_alloc`（153–175） | **CLOSED** | 把普通 BSS 副本与未分页副本分开：普通副本进 `kmain` 为 0、`kmain` 置 1；未分页副本在 `pre_init` 已为 1；`alloc_lowest` 的 `assert` 是读者；grep 证据与结论一致。措辞可再精确一处（SUGGESTION 1）。 |
| §2.6 `pg_utils` 双副本（615、669） | **CLOSED** | 615 说明 `unpaged_pg_utils.o` 与普通 `pg_utils.o` 各有一份 `pagedir`，`prot_init` 的清空/建表/装载作用于普通副本，`pg_load` 切 CR3；669 的反事实与两副本模型一致。 |
| §2.6 两份 `vir2phys`（644–662） | **CLOSED** | 按对象副本与符号所在段解释两份实现：未分页副本绑定 `pre_init.c` 的恒等版，普通副本绑定 `protect.c` 的减偏移版；与 615 的双副本模型一致。 |
| §3.2 职责句（780） | **CLOSED** | 把“TSS + 段描述符 + 内核栈”与系统调用 MSR 归 §2.4、把“IDT + 门”归 §2.5，归属正确；798 把 syscall MSR 落到 `TrapEntryArch::configure_syscall`。建议补一句消歧（SUGGESTION 2）。 |
| §3.7 门数算式（947） | **CLOSED** | 18+4+16+3=41；16+23=39；39−1+3=41；39−16+64+3=90、+5=92。向量 9/15 的口径与 §2.5 的 39 一致。 |
| §5.4 T03/T06/T07 就地定义（1270–1278） | **REOPEN** | T03（1270/1271）与 T06（1274）的就地描述可读；T07 所在的 M9（1275）仍写“原地重填 / TLB 旧条目”，与 R1 后的 §2.6（615）和 §3.5（893）冲突。见 RD3-CERT-READER-02。 |
| §1.2/§4.6 入口表拆行（42–47、1176–1181） | **REOPEN** | 拆行本身正确（syscall 不再混在异常/中断行）；但 §1.2 的“三份答案 / ①和③ / ③ 还带”（40、51、53、57）与 §4.6 的“三问”表头（1174、1176）仍是三行口径，四行表下编号错位。见 RD3-CERT-READER-03。 |
| §2.4 IOPL（456） | **CLOSED** | “`iobase` 高于限长→无 I/O 位图”的推导正确；“默认拒绝 CPL > IOPL，但 `enable_iop`/`SYS_IOPENABLE` 可授予 IOPL=3（`psw |= 0x3000`）”修正了“全部被拒”的绝对化表述。 |
| §3.3 x86 限定（809）/ §4.7 non-x86 无状态（1197） | **CLOSED** | 809 明确“只适用 x86-64；aarch64/riscv64 的保护状态是寄存器，不进入全局”；1197 把 `prot_init_done` 分成 x86-64 全局 `Option` 与 non-x86 无状态 accessor；两者一致，没有残留“non-x86 也进全局”的表述。 |

## FINDING RD3-CERT-READER-01（P1）

位置：§2.1，`03-kmain-cstart.md:141`；邻句 144、179。

原文（141）：

> `main.c` 里的 `kinfo`/`kmessages` 是 `usermapped_data.c` 定义的 `.usermapped` 最终全局，`kmain` 的 `local_cbi` 指向的也是这份最终全局。

问题：同段第一句的论点是“两行 `memcpy` 不是自拷贝”。如果 `local_cbi` 指向的就是目的对象 `.usermapped` 全局，那么 `memcpy(&kinfo, local_cbi, sizeof(kinfo))` 的源与目的是同一个对象，正好回到自拷贝；这与 179 的反事实（删掉两行会丢失 `module_list` 与 `kmessages`）也冲突。按本段自己的证据（`pre_init.o` 经 `objcopy --prefix-symbols=__k_unpaged_` 改名、`kinfo`/`kmessages` 落在 `.unpaged_bss`、`return &kinfo` 经 `head.S` 传给 `kmain`），`local_cbi` 指向的是 `pre_init` 的暂存副本，不是最终全局。

这是 R1 修复从旧“自拷贝”论证留下的未翻转残句：`git diff` 里旧文写的是“`local_cbi == &kinfo`，第一行是一次全局自拷贝”，新文把结论翻成“不是自拷贝”，但这一小句仍保留了旧的等同关系。

影响：读者无法从这一段确定源与目的到底是不是同一对象；“不是自拷贝”的论证被自己的证据句推翻。

修法：把“`kmain` 的 `local_cbi` 指向的也是这份最终全局”改为“`kmain` 的 `local_cbi` 指向的是 `pre_init` 的暂存副本”（或改为“`kmain` 的 `&kinfo`（目的）才是这份最终全局”）。同时把 144 行代码块注释里的“自拷贝论证”改为“拷贝关系论证”或“非自拷贝论证”——该注释仍是旧论点的措辞。

## FINDING RD3-CERT-READER-02（P1）

位置：§5.4 M9 行（`03-kmain-cstart.md:1275`），与 §2.6（615）、§3.5（893）对读。

原文（1275）：

> | M9 | 重建页表的原地重填（§2.6，C 侧） | 清零→重填窗口内取指可行（TLB 旧条目）；`pg_load` 后表内容齐全 | ……

问题：R1 修复后的 §2.6 明确写“不是同一数组的原地重填”：`pg_utils.c` 有两份对象，`prot_init` 在普通副本的 `.bss pagedir` 上清空/建表，`pg_load()` 才把 CR3 从旧表切到新表；§3.5 也写“不是在原表上重填”。在新的两副本模型下，清零/重填发生在非活动副本上，活动表在此期间不受影响，“TLB 旧条目”不再是这段代码成立的机制。M9 的机制名与“需要证明什么”都停留在旧模型上。

影响：T07 的“就地定义”建立在一个已被 §2.6 否定的机制上；读者按 M9 去核 §2.6 会得到相反的故事。

修法：把 M9 的机制名与断言改成两副本模型，例如“重建页表（两份 `pagedir`，`pg_load` 切 CR3）”，需要证明“清零/重填只作用于非活动副本；`pg_load` 后新表内容齐全（恒等窗口 + 内核映射）”。如果要保留 TLB 讨论，应说明它属于 `pre_init` 第一张表的建立，而不是 `prot_init` 的重建。

## FINDING RD3-CERT-READER-03（P2）

位置：§1.2（`03-kmain-cstart.md:40–57`）与 §4.6（1174–1181）。

问题：R1 把“异常/中断/syscall 入口”拆成“异常/中断入口”与“系统调用入口”两行后，表格从三行变四行，但周围的“三问”口径没有同步：

- §1.2 第 40 行“三份答案落在三张表上”：系统调用入口这一行不由三张表中的任何一张回答（x86 是 MSR，aarch64/riscv64 是向量表的复用）。
- §1.2 第 51 行“①和③各只有一套（DPL……、`sp0` 每 CPU 一个字段）”：按四行表，③ 是系统调用行，但括注解释的是 `sp0`（第 4 行）。
- §1.2 第 53 行“③ 还带一个‘内核态里又出事用哪根栈’的子问题”：③ 同样是系统调用行，子问题却属于栈（第 4 行）。
- §1.2 第 57 行与 §1.4 第 90 行仍说“三张表与它们回答的三个问题 / 三问”。
- §4.6 第 1174 行“按 §1.2 的三问收一张表”与表头“三问”（1176）下面也是四行。

影响：读者画像的教学目标 1 是“三张表各回答什么问题”；表格四行与“三问”的标题/编号不一致，读者会问“系统调用入口算不算三问之一”。

修法（二选一）：(a) 保留“三问”只指三张表，把系统调用行标成“额外一问（不由三张表回答）”，并修正 51/53 行的 ③；(b) 把“三问”改成“四问”，重排 ①②③④，并同步 §4.6 的表头与 1174 行。无论哪种，51/53 行的 ③ 都必须与表格行号一致。

## SUGGESTION

1. §2.1 第 153 行“`glo.h` 的 `EXTERN int` 在普通编译单元里展开成零初始化的 BSS 定义（`table.c` 一侧的普通副本）”——措辞可以更精确：`EXTERN` 在多数编译单元里展开为声明，只有 `table.c`（定义 `EXTERN` 为空）那一份才展开为零初始化定义。建议改为“`EXTERN` 在 `table.c` 里展开成零初始化的 BSS 定义；其余编译单元只看到声明”。这是措辞偏好，不影响本段结论。

2. §3.2 第 780 行“Rust 把这两个职责拆成两个 trait”——建议补半句：“系统调用 MSR 虽然写在 `tss_init`（§2.4）里，但它属 trap 入口职责，归 `TrapEntryArch`。”否则读者会按“两个职责 = 两个 trait”一一对应，和下方映射表第 798 行（syscall MSR → `TrapEntryArch::configure_syscall`）对不上。这是消歧，不是改事实。

3. §3.7 第 947 行的 90/92 公式只给了“3/5”的总数，没有像 41 那样逐项列出。如果希望读者复算 APIC/SMP 路径，建议把三个或五个向量列出来；基础的 41 与 39 两条算式已经逐项可复算，此条只是完整度偏好。

## 结论

- §2.1 `memcpy` 段：**REOPEN**（RD3-CERT-READER-01）。
- §2.1 `kernel_may_alloc` 段：**CLOSED**。
- §2.6 `pg_utils` 双副本与 `vir2phys`：**CLOSED**。
- §3.2 职责句：**CLOSED**（附 SUGGESTION 2）。
- §3.7 门数算式：**CLOSED**。
- §5.4：T03/T06 **CLOSED**，T07/M9 **REOPEN**（RD3-CERT-READER-02）。
- §1.2/§4.6 入口表拆行：**REOPEN**（RD3-CERT-READER-03）。
- §2.4 IOPL：**CLOSED**。
- §3.3 x86 限定与 §4.7 non-x86 无状态：**CLOSED**。

`certify 新准入 = 3`（P1×2 + P2×1；SUGGESTION 不计入）
