# RD06 round1 · 读者 C（因果一致性）

- 阅读材料：`rewrite-notes/01-stage-kernel/06-proc-init-boot-proc.md`（2976 行，全文冷读一遍后按主张回查）＋ `tmp/review-eval/01-stage-kernel/06-proc-init-boot-proc/reader-profile.md`。
- 方法：先抽出关键因果主张（四道抑制位的置位/解除责任、boot 循环的顺序依赖、"非 VM boot 模块由 VM 亲自装载"、三套编号映射、RTS 宏的跃迁语义、三层存储约束、各条 C→Rust 对位声明），再逐条追它在概述（§1）、C 分析（§2）、小结（§2.9/§3.14/§4.9）、设计（§3）、实现（§4）中的复述，按三查收口：自相矛盾、证据不足、单形态结论写成普遍结论。
- 行号（Lxxx）指本篇文档当前版本的行号；引文逐字摘自该文档。

## FINDING RD06-READER-C-01（P1）

**`rts_set` 的 `.set(flags)` 语义在文档内无法判定：核心对位声明"三段结构与 RTS_SET 相同"缺证据，自然读法与 §2.9 的终态表直接矛盾。**

- 主张一（§3.3，L1575）："三段结构与 `RTS_SET` 相同：读旧值、改位、按跃迁决定队列动作"；主张二（§3.9，L1990）："Rust 侧保持位的语义与值不变"。
- C 侧机制（§2.2.1，L541–548）：`RTS_SET` 是 `p_rts_flags |= (f)`——按位或，保留其余位。
- Rust 侧证据（§3.3，L1555–1571）：`ProcessTable::rts_set` 的改值一步是 `p.p_rts_flags.set(flags);`，参数是完整位集；`RtsFlags` 只给了 `pub struct RtsFlags(AtomicU32);`（L1545），`set` 方法体全文未引。按原子包装的自然读法，`set` 是整值覆盖而非按位或。
- 矛盾链：若 `.set` 为整值覆盖，则 §3.9 的置位序列——else 分支 L1998 `.set(RtsFlagsBits::NO_PRIV | RtsFlagsBits::NO_QUANTUM)`，随后双闸 L2007 `.set(RtsFlagsBits::VMINHIBIT | RtsFlagsBits::BOOTINHIBIT)`——会把十个非可调度模块刚置的 `NO_PRIV|NO_QUANTUM` 抹掉，与 §2.9 终态表（L1299："RTS_NO_PRIV | RTS_NO_QUANTUM 置位、双闸置位、RTS_PROC_STOP 置位"）及 §3.9 自己的"保持位的语义与值不变"冲突，M3（boot 后位集与 C 一致）随之失据；若 `.set` 内部实为按位或，对位成立，但文档没有任何可核对证据——且 §2.2.2（L608）刚证明 `rts_setflags` 才是"整值覆盖"的对位物，若 `rts_set` 也是覆盖，§2.2.1 建立的"RTS_SET 按位或 vs RTS_SETFLAGS 整值覆盖"二分在 Rust 侧坍塌。
- 影响：RTS 位语义的 C 对位是设计章（§3.3）、实现章（§3.9、§4.7）与测试章（M4）反复复述的核心主张，读者在文档内无法消解此歧义。
- 建议修复：补引 `RtsFlags::set` 的实现体，或在 §3.3 明写内层为按位或；随后让 §3.9 的置位序列与 §2.9 终态表逐位对账。

## FINDING RD06-READER-C-02（P2）

**"非 VM boot 模块由谁装载"在文档引用的源码注释里自相矛盾（RS vs VM），文档未做任何调和。**

- 正文主张（§2.4 L950、§2.7、§3.4、§4.7）：其余 boot 模块由 VM 装载。§2.7（L1189）引 ProcKind 注释"its image is loaded by **VM** at runtime …（…not by RS itself）"（L1611–1618），并声明"与 C 里实际装载它们的是 VM（§2.4 的 init_vm 遍历）一致"。
- 同一篇引出的另外三处注释说反了：lib.rs 映射注释（L1632）"other user    → ProcKind::UserService (RS will load ELF later)"；`EntrySpec` 结构注释（L1651）"user process not yet loaded by RS"；`EntrySpec::DEFERRED` 注释（L1668）"will be set by RS"。
- 三处与正文方向相反的引文被原样当作证据呈现，全文没有一处调和、标注或登记（对比 §3.13 对旧节号尚且登记了 SRC-10 待同步）。
- 影响："VM 亲自装载"是 §1.3、§2.4、§2.7、§3.4、§4.7 反复复述的一等因果主张；读者面前同时摆着两个相反载体，文档内无法判定 Rust 侧真实归属——若实现真由 RS 装载，§2.7 的"一致"结论本身就错；若是注释漂移，也应按本篇自己的纪律留痕。
- 建议修复：给三处引文加调和注（漂移则登记同步项；实现若真由 RS 装载则更正 §2.7/§3.4 的归属表述）。

## FINDING RD06-READER-C-03（P2）

**§2.9 小结表第三行"名字与模块地址已登记"与 §2.1.1/§2.6 矛盾（或未声明地偷换了"名字"的口径）。**

- C 侧规则：§2.1.1（L295）"其余 11 个用户模块在 boot 期没有名字——它们各自启动后经 exec 路径拿名字"；§2.6 动作 4（L1028）只对 `i < NR_TASKS` 写名字。
- §2.9 填充表（L1299）却写"其余用户模块（10 项）| 名字与模块地址已登记、…"。
- 同表第二行（L1298）刚以字段口径讨论过名字（"名字：VM 已写…RS 未写"），第三行的"名字……已登记"若解作"写入槽"则与前文直接冲突；若解作"登记在 image 清单"，则该清单条目从编译期就存在、与"模块地址由 boot 循环回填"这类 boot 期动作并列表述且不作说明，同一张表内口径悄然切换。
- 影响：小结是读者复核全章的锚点，这一行让"boot 期谁有名字"的因果链在小结处断掉。
- 建议修复：第三行改为"名字未写（exec 时才写），模块地址已登记"；若意图是清单口径则显式注明。

## FINDING RD06-READER-C-04（P2）

**§1.4.1"把模块序号直接当 proc 号用"反推与本节自己的映射表不符；VM panic 链条缺可达的因果步骤。**

- 反推（L205）："module_list[0]（ds）会被装进 proc 号 0 的槽，也就是 PM 的槽；DS 与 PM 的端点互换。"按本节自己的表（L183–199：ds 模块 0/proc 号 6，rs 模块 1/proc 号 2，pm 模块 2/proc 号 0，sched 3/4，vfs 4/1，memory 5/3，tty 6/5，mib 7/7），"模块序号当 proc 号"是一个七元轮换：DS 落进 PM 的槽（端点 0）、PM 落进 RS 的槽（端点 2）、RS 落进 VFS 的槽（端点 1）、……、tty 落进 DS 的槽（端点 6）。PM 拿到的是 RS 的端点而非 DS 的端点，"互换"按任何严格读法都不成立。
- panic 链条（L205）："VM 启动时靠线性扫描 kernel_boot_info.boot_procs[] 按 proc_nr 找自己的条目……错位后它找不到 DS 的 boot 条目，直接 panic("no init_proc")"。按文档自己给出的机制（§2.4 L925–947 的 init_vm 逐条遍历、init_proc 按 proc_nr 匹配内核槽），boot_procs 的 12 个条目无论如何都在，"找不到 DS 的 boot 条目"从哪一步推出、panic 的触发查找是哪一次，文档没有给出可复算的步骤。
- 复述扩散：§1.3（L149）以"（端点互换 + VM panic("no init_proc")）"回指同一条，把不准确表述带进概述。
- 建议修复：按映射表把轮换结果写准，并补出 panic 的实际触发条件。

## FINDING RD06-READER-C-05（P2）

**§2.4"把 image[] 里 DS 与 RS 两行调换"反推漏掉位置配对破坏，与本节自己的机制分析、§2.6 的同类反推互相矛盾。**

- 本节前文（L891）刚建立："第一条的消费点是 boot 循环的模块下标：`kinfo.module_list[i - NR_TASKS]` 这一句隐含了'第 i 个清单条目对应第 i − 5 个模块'"。
- 由此推，调换第 5、6 行后：第 5 条（rs，proc 号 2）配到 module_list[0]（ds 的二进制）、第 6 条（ds）配到 module_list[1]（rs 的二进制）——RS 的槽装载 DS 的 ELF，是二进制错装，不只是先后次序。
- 但反推（L952）只说"VM 的遍历先给 RS 建页表……受影响的是启动期'谁先具备运行条件'的时序——这类改动的可观察差别不在单条分支上，而在多个服务竞争同一批资源的先后"，对配对破坏只字未提。§2.6 对同族场景（L1104"顺序一乱，内核 task 会被当成用户模块去 kinfo.module_list 里取地址"）恰好给出了正确机制——同一文档内两处反推结论互相矛盾。
- 建议修复：按本节自己的配对机制重写该反推的后果（错装二进制为主，时序为次）。

## FINDING RD06-READER-C-06（P2）

**§3.13 的 fork RTS 修正清单漏 `NO_PRIV`，与 §2.2.2 的 C 侧行为（do_fork 置 NO_PRIV）对不上。**

- C 侧：§2.2.2 的 `RTS_NO_PRIV` 行（L586）置位者含"system/do_fork.c（子进程先无权限）"；`RTS_NO_QUANTUM` 行（L593）也列 do_fork。
- Rust 侧：§3.13（L2285）"p_rts_flags（复制后 NO_QUANTUM 置位、SIGNALED/SIG_PENDING/P_STOP 清掉）"——SIGNALED/SIG_PENDING/P_STOP 三项与 §2.2.2 各行的 do_fork 解除条目逐条咬合，唯独 NO_PRIV 缺席。
- 两种可能文档内无法排除：fork_from 的清单真漏 NO_PRIV（Rewrite 档位下未标注的语义偏差，fork 出的系统进程可能在无权限状态下可运行），或文档转述清单时漏了一项。§5.3 有 `test_fork_child_rts_corrections`，但文档未给出其断言内容，无法裁决。
- 建议修复：补 NO_PRIV 一项，或明写"Rust fork 路径不置 NO_PRIV"及其对位机制，并附测试断言要点。

## FINDING RD06-READER-C-07（P3）

**§2.0 第二层的承重前提"内核全生命周期不使用堆分配器"在文档内没有检索证据。**

- 主张（L253）："C 内核里没有 `malloc`/`kmalloc`/`slab` 之类的调用"。它是三层因果链（§2.0 第三层"只能落成编译期定容的静态数组"）、§2.0 末反推与 §3.8 零堆设计的公共前提。
- 文档给出的证据（`kernel_may_alloc` 开关、`pg_alloc_page` 的 assert）只证明"窗口机制存在"，不证明"全生命周期无分配器调用"；§6.2 的八条命令没有一条覆盖它。按本篇自己立的可复现口径，这是一个裸断言。
- 建议修复：§6.2 补一条 grep 读数（如 `grep -rn "malloc\|kmalloc\|slab" minix3/minix/kernel/`）。

## FINDING RD06-READER-C-08（P3）

**§3.11"两边都没有调度器读它"与同节两处引文"The scheduler must check allows(cpu)"相互拉扯，"不改变现行为"的结论前提不稳。**

- 引文（L2155–2157 与 L2176–2177 两处）："The scheduler must check `allows(cpu)` before enqueuing into a per-CPU ready queue."——命令式，读起来像现行约定。
- 正文（L2166）："这个差异不改变现行为，因为两边都没有调度器读它"；选型表（L2193）只说"C 的零值在当前树里没有读者"。
- 若 Rust 调度器真按注释检查 affinity，则"C 不读、Rust 读"本身就是未标注的行为差异，该结论的理由不成立；若不读，引文的时态夸大了现状。文档内无法判定。
- 建议修复：写明 Rust 调度器当前是否实际调用 `allows(cpu)`，并把引文时态改准。

## FINDING RD06-READER-C-09（P3）

**§4.7 记录了"用户模块轮写 module.name"的 Rust 行为，未按本篇自己的档位纪律标注它与 C 的差异。**

- C 侧规则（§2.1.1 L295、§2.6 动作 4）：boot 期只有内核 task 与 VM 写名字，其余 11 个模块 exec 时才写。
- §4.7 对照表第 4–5 行（L2678）："内核 task 轮写的是常量名；用户模块轮写 module.name"——Rust 在 boot 期给用户模块写了名字，是 §2.1.1 规则的直接反例。该行与 §3.14/§4.9 对照表均未给出差异标注或档位（p_name 只影响内核诊断输出，属轻微可观察差异，但按"三处一致标注"的纪律应留痕）。
- 建议修复：在该行补一句差异说明（[ARCH] 标注，或说明属诊断信息层面的无害偏差）。

## FINDING RD06-READER-C-10（P3）

**§5.1 的 `default = ["mock"]` 表述疑似过时（证据源在会话环境自带的项目指令，列为待核线索而非已验证断言）。**

- §5.1（L2787）："os/kernel/Cargo.toml 的 `default = ["mock"]` 就是宿主测试形态，生产形态走 `--no-default-features`"。
- 会话环境自带的项目指令（AGENTS.md，不在本读者允许阅读的材料内，故仅作线索）记载 SD-30 已清空三包默认形态、裸跑 `cargo test -p minix-kernel` 会被 `form_contract` 编译期绊线挡住、必须显式 `--features mock`。若如此，本句及派生的"生产形态走 --no-default-features"均已失准，§5.1 的证据叙事应改为"显式 `--features mock`"。
- 建议修复：交事实读者用 `grep -n "default" os/kernel/Cargo.toml` 复核并更正 §5.1。

## SUGGESTION RD06-RC-S1

§2.6 裸 `|=` 等价性论证（L1102）的括号枚举只有两类："要么刚被 RTS_SET 置了两位，要么是内核 task（RTS_SLOT_FREE 尚未清）"。双闸实际覆盖 RS 与十个模块：RS 既非前者也非后者，它不可运行的真正原因同样是 SLOT_FREE 未清。结论无误，情形清单应补第三类。

## SUGGESTION RD06-RC-S2

§2.4 的 NOTIFY 结论（L920）"表头注释声明的那条优先级语义，其载体是清单顺序本身，而清单顺序在代码里的消费者是 boot 循环（第 1、2 条）与 VM 的启动遍历"——这两类消费者消费顺序的目的分别是条目 1、2 与页表建立次序，都不是 NOTIFY 优先级，"其载体"一语与其证据脱节。另外本节已有全部数据（特权槽号 PM 5、VFS 6、RS 7、DS 11，投递取最低位），足以直接点明"按该机制 DS 不是第一个被投递的系统进程"，与表头注释"DS must be the first system process"正面冲突——这层冲突值得显式写出，而不是留给读者自己算。

## SUGGESTION RD06-RC-S3

Rust 侧每 CPU idle 槽的居所没有交代，§3.1 的"逐行对照"对 C 清表第三个循环不完整：C 明说"idle 结构**不在 `proc[]` 数组里**……也不需要 `p_nr`"（L1015），而 §3.1 引的 `ProcessTable::new` 片段带"对 idle 槽的三处赋值（端点、`RTS_PROC_STOP`、名字）与一次下标计算（`let idle_idx`）"（L1367），且明示 ProcessTable 只有 procs/sched/vm_request_queue 三个字段（L1351）。idle 槽若在 `procs[]` 内，其 p_nr 如何赋、与 C 的"无 p_nr"属性如何对应；若在 sched 内，应点明。§5.3 的 `test_process_table_idle`/`test_process_table_const_init_idle_name` 表明 idle 槽在表的可测范围内，更需要一句话讲清位置。

## SUGGESTION RD06-RC-S4

§3.9 开头（L1990）"四位分散在四处置位……boot 循环置两位、do_vmctl 解两位、do_privctl 解一位、bsp_finish_booting 解一位"与 §1.4/§2.6 对不上：boot 期四位全部由 boot 循环置（动作 8、11、12），"置两位"只数了双闸；运行期置位点（do_fork、do_vmctl 的 SET、do_runctl、smp）也被略去。建议按 §2.2.2 的口径改写。

## SUGGESTION RD06-RC-S5

§2.0 第三层（L255）"前三层合起来把选择空间压到一处：容量是编译期常量、窗口内没有分配器、表要用到关机"——第三个前提"表要用到关机"在前文没有作为任何一层出现（第二层讲分配器缺席，不是表的寿命）。枚举与层结构对不上；建议把"表要用到关机"升为显式前提，或改称"三个前提"。

## SUGGESTION RD06-RC-S6

§3.7 开头（L1873）把两件事压进一句："C 的负担是一句 if(...) panic(...)……全部失败原因合成一个 panic，而在此之前……一旦返回值被丢成零 PC 就再也看不出来"。C 的 VM 分支对失败是 panic，不存在"丢成零 PC"的路径；"丢成零 PC"是 Rust 旧实现的历史（§3.4/§3.7 反推所述）。建议拆开表述：C 的负担是"失败不分类"，Rust 旧形态的负担才是"静默零 PC"。

## SUGGESTION RD06-RC-S7

§5.2 与 §5.4 的台账没有完全咬合：M13（ImageInsideSegments 前提，R06-T04）在 §5.4 缺口表里没有对应行（FPU 行只覆盖 M12/R06-T03）；§5.4 各行也不携带 R06-Txx 编号，M3/M11（R06-T01）、M10（R06-T02）只能靠读者自行对应。建议缺口表补 T04 行并统一携带编号。

## SUGGESTION RD06-RC-S8

§3.4 对 `KERNEL_TASK`/`DEFERRED` 并存意义的论证（L1680"如果只有一个 NONE 常量，arch 层无法区分……第一次调度时该不该报错也就无从判断"）缺承载机制：§4.1（L2352）明说 arch 把三个 `Option` 全部 `unwrap_or(0)` 收成零，而 arch 的输入参数本就含 `ProcKind`（"永远没有入口 vs 还没装载"的区分已在角色参数里）。文档内没有展示任何读这个区分的"第一次调度报错"检查点。建议给出消费点（函数名），或把论证弱化为"意图留痕"。

---

- 本次扫描：10 条 FINDING，8 条 SUGGESTION。
