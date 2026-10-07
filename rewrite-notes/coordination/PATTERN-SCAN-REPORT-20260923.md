# PATTERN-SCAN-REPORT-20260923 — 历史日志模式挖掘与回归检查（C-61）

> **任务**：用户指令「从 FIXLOG、WORKLOG 等历史 LOG 中扫描并发掘模式，扫描得到测试，避免以后再出现类似的问题」。
> 交付三件：本目录文档 + [`tools/pattern-gate.sh`](../../tools/pattern-gate.sh)（13 项机械检查）+
> [`tools/pattern-gate-baseline.txt`](../../tools/pattern-gate-baseline.txt)（存量豁免）。
> 判别证据：`tmp/evidence/20260923-c61-pattern-gate/`（selftest 18 例正反矩阵 + 真树变异负例 6 组 + 全量运行实录）。
> **更正纪律**：本档断言全部可用所引条目标题 grep 复位；引用 FIXLOG 用「线 + 条目标题」定位（裸行号会漂移，
> 即 review-patterns 所称「代码注释行号漂移」一类），引用代码用符号名。发现写错就地追加更正节，不改写原条目。

---

## §1 扫描语料与方法

**语料**（全部精读或定向提取）：

| 语料 | 形态 | 说明 |
|---|---|---|
| `.review/zcode/edge1/FIXLOG.md` | 2219 行 | 全文精读。含 NK2/NK8/NK1/NK4-A 迭代1-21、NK4-B M3/M4 全系列补记 |
| `.review/zcode/edge2/FIXLOG.md` | 301 行 | 全文精读。L17 三批 + NL/NK/C 系列执行记录 |
| `.review/zcode/edge3/FIXLOG.md`（现行本） | 616 行 | 全文精读。Fix #122-#148 |
| `.review/zcode/edge3/FIXLOG_archive.md` | 3659 行 | 定向精读：流程事故段（#3222 事故两起、#233 流程教训）+ P0 代表条目 |
| `.review/zcode/edge3/FIXLOG.raw-20260920.bak` | 68552 行 | 不通读；与 archive 做 sort -u 差异核查——仅 8 行重复拼接伪影，**无独有内容丢失** |
| `.review/zcode/edge4/FIXLOG.md` | 128 行 | 全文精读 |
| `rewrite-notes/coordination/NK4A-QWEN-WORKLOG.md` | 471 行 | 全文精读（Task A/B/C 六轮取证） |
| `rewrite-notes/coordination/NK4B-WORKLOG.md` | 2446 行 | P0 核账 + P1 节全文精读；M3/M4 系列经 edge1 FIXLOG 补记覆盖 |
| `NK4A-REVIEW-REPORT.md`、`NK4-REGRESSION-REVIEW-20260922{,-PART2}.md` | 定向提取 | 评审归纳节（危险面、可沉淀资产、OQ-1 建议） |
| `new_edge1-4.md`、`edge_todo.md` | 定向提取 | §1 协作规则 + §2 认领板 C-XX 登记行教训 + 账本行判例 |
| `NK4A-TODO.md` §5/§9、`NK4B-TODO.md` §8、`NK4B-OPENING-PROMPT.md` | 定向提取 | 铁律条款 |
| `AI-chats/daily.todo.md` | 定向提取 | 教训条目（含未提交修改） |
| `.review/zcode/{fork-syscall-rewrite,runtime}/` STATE/fix-status | 定向提取 | P0/P1 级教训条目 |
| `prompt/review-rules/review-patterns.md` | 索引级 | 85 既有模式对照（§3），避免重复立模式 |

**方法**：先全文精读成稿（工作笔记先行，防上下文压缩丢失），再对每个候选模式问三个问题——
「在几份独立日志里出现过（频次）？」「当时怎么发现/怎么修的（已有对策）？」「能不能用 grep/脚本机械拦住
（检测方式）？」。能机械拦的落成 `pattern-gate.sh` 的 P 检查项；不能的进 §5 流程纪律清单。

---

## §2 模式目录（六族 34 条）

编号供人引用；「→P#」表示已落成 pattern-gate 检查项，「→§5」表示归入流程纪律。

### A 族：账面 / 日志类

| # | 模式 | 代表事故（出处条目） | 根因 | 检测 |
|---|---|---|---|---|
| A1 | **日志全量重复追加** | edge3 FIXLOG 膨胀到 68,552 行（同内容 32 遍、95.3% 重复行）；对策=追加前 `grep -c "^# "` 必须为 1（new_edge4 §1 规则 5） | 会话收尾把整文件重当增量写 | →P1 |
| A2 | **FIXLOG 编号撞号** | #117×3、#118×2、#119×2、#101/#102/#103/#104/#105 各×3、#125 撞号（edge3 archive 索引 + Fix #126 撞号教训） | 多会话向共享 gitignored FIXLOG 各自追加；tracked 文档引用序号不可移植 | →P10（增量拦 todo/new_todo 裸引）+§5 |
| A3 | **账面失真/滞后** | edge_todo 52 条中 10 条滞后可闭（LEDGER-AUDIT 专项）；S30 F3c-2 ⏳ 主张已被 17c88be0c 全额落地；NS10 账面三件两件已闭；X-5 过期误报（「跨轮状态陈旧 CTOS」家族，review-patterns 在案） | 账本行与代码演进不同步；「已闭单勿领」（new_edge3 :45） | 半机械：领取前对码（§5）；tools/todo-staleness-check.sh 已有 |
| A4 | **文档/注释引用不存在的符号** | 设计文档 08-pm-srv-fork.md §2.4/§4.2 引用 `SRV_FORK_INHERIT_FLAGS`，全仓零命中；srv_fork.rs 注释同样虚构（edge4 批A 附带发现 / Fix #124①） | 凭设计意图写「代码事实」 | →P13 |
| A5 | **C 锚点漂移** | `kernel/memory.c`、`krandom.c`、`arch/i386/smp.c` 不存在；`protect.c` 三处偏 7-18 行；行号上的角色标签错（打印者标成调用者）；「全仓无一处」实扫两目录（NK4B M4.4 五轮评审 + 旁支修复） | 凭记忆写锚点；范围词大于命令根 | →P11（增量）；存量靠 review-line-check.sh |

### B 族：wire / 常量 / 协议类（重灾区，7 条）

| # | 模式 | 代表事故 | 根因 | 检测 |
|---|---|---|---|---|
| B1 | **哨兵/调用号字面量化石值** | `SELF=-2`（真值 31742，-2 恰为 SYSTEM 端点，9 消费点永不命中，真机 wire 层才暴露）；`ANY=-3`；`MIB_CALL_SYSCTL=0x600`（权威 0x1800）；枚举序数当消息号（`CdevRequest::Ioctl as i32`=4 ≠ 0x404） | 手抄字面量 + canned 测试无端点语义校验 → 恒绿 | →P3（权威派生断言 + 0x1800 pin）；枚举序数族由 P2 清单里的 NS7-A 测试钉 |
| B2 | **回复车道偏移错位** | lseek 回复被覆盖（Fix #70）、Read 回复布局（Fix #50）、回复状态偏移（Fix #87）、fchmod mode @8 vs C @4（C-45）、PM GetTimeOfDay nsec @16 vs @8（C-54）、VM_MMAP 回程 m1p1@16 vs @40（NS5-A） | 回复 overlay 的 C 结构权威没有双侧对拍 | →P2 清单（车道 pin 测试存在性）+§5（新 lane 必须双侧对偶测试） |
| B3 | **符号域 / 正负号折算** | ERESTART +200 vs 线上 -200（edge3 暂停交接登记）；MIB 应答「负 m_type 判失败」vs C 正 errno 协议（C-59②，ENOMEM/ENOSYS 被当成功）；`KcallResult::Ok(errno)` 正号上线、VM 网关 `< 0` 判错把失败读成成功（edge1 Fix #9 迭代3 F10，未修登记） | i386 负 errno 语义在 Rust `Result` 世界的手工折算点分散 | →§5（跨号域边界必须 pin 正负两相）；部分由 P2 清单覆盖 |
| B4 | **u32 车道截断** | VM_MMAP addr/len `as u32`（栈顶 0x7fff_ffff_f000 必中，T2 阻塞位）；MIB_SYSCTL 三地址 lane u32（guest 基址 5GiB 必截断）；LP64 前提勘误——`_ASSERT_MSG_SIZE` 钉 56 字节使 C 头只有 i386 形态（NS5-A/NS11-B） | C i386 头照搬 + 宿主 canned 不发真实地址 | →P2 清单（>4GiB 往返 pin） |
| B5 | **canned 双方各自为政恒绿** | MIB_SYSCTL 两侧 canned 各用各的常量恒绿（C-59①）；creat 臂从客户端恒零填充的 padding 区解内联路径=真 wire 缺陷被自造请求形掩盖（NL10） | 测试替身不校验语义 → 双方自洽、互相矛盾 | →§5（跨服务联调线束 E5 族的存在意义；修法见 C-59/C-45 判例） |
| B6 | **宿主不可达缺陷** | NS7-A 两处（消息号取枚举序数、`if status == 0` 恒假门）宿主发送恒败故测试测不出，真机 ioctl 首拍即暴露；GetTimeOfDay 臂时钟源无注入缝宿主单测不可达（C-54 登记） | 宿主传输半恒败路径下的代码不可达 | →P2 清单（wire 形状 pin）+§5（真机冒烟兜底） |
| B7 | **布局/对齐算术错** | ps_strings 紧凑 24B vs C LP64 自然对齐 32B（edge1 迭代7 F13）；`.sdata2` 误归 `.bss`（NOBITS 静默丢内容，M4.2 写作期纠正）；「恰 56」草稿算术未含对齐间隙（NS11-B，编译期 size 断言拦住） | 手算布局无编译期/测试期钉 | →§5（布局必须 repr(C)+size 断言+两侧 pin） |

### C 族：测试有效性类（6 条）

| # | 模式 | 代表事故 | 根因 | 检测 |
|---|---|---|---|---|
| C1 | **断言凭记忆虚构** | console 首次 open 断言期望 0，实返 CDEV_CTTY 0x40000000——「先跑测得 left=0x40000000 再改，不凭记忆」（edge2 F3） | 断言先写后跑 | →§5（断言先跑再写） |
| C2 | **空转测试 / 碰巧通过** | VM 握手失败臂测试漏注册 RS caller，失败分支从未到达恒绿（Fix #130「碰巧通过」）；creat 续接体「假覆盖」；对策=反证跑——把目标改错测试必须红（edge4 批B 判例） | 失败路径测试不证明路径可达 | →§5（失败臂测试必须反证有牙） |
| C3 | **测试自身缺陷** | dispatch_times 旧测试靠其它测试先初始化 CLOCK_STATE，过滤单跑必 panic（测试顺序耦合，Fix #5）；「存常量读回」同义反复断言（fix-status.md Fix #3）；GETCWD fake 四测首轮失败全是测试数据构造错 | 测试间隐式依赖 / 断言无判别力 | →§5（单跑过滤必须绿；判别断言） |
| C4 | **夹具漂移断链** | `NoopKernelGateway` 缺 `sys_diagctl_stacktrace` → minix-tests 全 crate E0046 且阻断 clippy 多 crate 会话、pm 警告面 15 条不可见（Fix #142）；批量补 trait 方法两版脚本都有 bug——**必须 `cargo test`（test-profile）不能只 `cargo build`**（Fix #129 教训） | trait 加方法未跟夹具；#[cfg(test)] 不参与普通 build | →P4（[[test]] 对账）+§5（test-profile 编译门） |
| C5 | **伪验证 / 非法组合** | `--features alloc-global,panic-handler,std` 直调 none 构建 144 错——验证必须走消费方真实 feature 面（Fix #147）；`cargo test --workspace --exclude minix-kernel` 组合下 feature 统一三错（NL4 预存登记） | feature 组合形态与生产消费面脱节 | →§5（guest 构建走真实消费面） |
| C6 | **防回归测试被静默删除** | 历史事故的防回归测试（本档 §4 P2 表 20 个）无删除守卫 | 删测试无门 | →P2 |

### D 族：流程 / 并发 / 工具类（6 条）

| # | 模式 | 代表事故 | 根因 | 检测 |
|---|---|---|---|---|
| D1 | **共享主树裸 commit / amend 卷走他人工作** | 三起：amend 吞对方文档提交+卷入对方暂存批（archive :3222 事故1）；裸 `git add`+commit 卷走 17 文件（同 :3222 事故2）；STTY-G `--amend` 把对方 1302 行 exec_worker 一起改写（edge2 FIXLOG STTY-G 事故）。恢复法：`commit-tree` 逐字节复原（哈希一致）/ update-ref 退回。对策三条已固化（--only 显式文件、共享分支禁 amend、update-ref 后核对 staged） | 共享 index 无隔离 | →§5（git 纪律；无 hook 基础设施，机械拦截成本高于纪律） |
| D2 | **reset/checkout 冲离在制** | NK5 checkout+reset 冲毁主树两组在制（C-35 机制化起源）；NS11-B clippy 对账 checkout 往返冲掉六文件、从会话记录重建（Fix #141 过程事故）；「基线对账禁 checkout 未提交树」 | 主树承载多线未提交在制 | →§5（claim.sh 机制 + 对账用 stash/已提交基线；规则 8 改完即 commit） |
| D3 | **共享 target 指纹互踩** | 树内 docker 构建踩共享 target，NS11-B 在制 u32→u64 编译错窜入他线（NL10）；E0425 假失败判别法=单包构建退 0 + 换 `CARGO_TARGET_DIR` 复跑 | 并发会话共享构建产物 | →§5（树内 target 判例） |
| D4 | **同文件并行编辑互相回滚** | 同响应内 SearchReplace 与 sed 并改一文件互相回滚（NL4）；批量字符串扫描脚本三处缺陷致 8 文件语法损坏，git checkout 回退重做+单管线+验证切片收敛（Fix #145 过程事故） | 工具并行改同一文件 | →§5（串行为准；批量变换后编译驱动验证） |
| D5 | **退出码被吞** | run_all.sh 三处构建循环 `|| echo "(build failed)"`——aarch64 构建断裂 17 错被改判 skip（M3.1 发现路径之一）；登记待 P6 清理 | 脚本容错写法掩盖失败 | →P7（基线冻结三处，新增即拦） |
| D6 | **CI 门用 check 不用 build / 判据看退出码不看工件** | OQ-1 变异实证：cargo check 不做 codegen，inline asm 寄存器/fixup 错误静默放过（check 0 错、build 报 invalid fixup）；required-features 不满足时 cargo 对 bin **静默跳过不报错**——判据必须看工件存在（M3.3 反向判别） | 把「命令没报错」当「验证通过」 | →P6（防删门+防 check 退化）+§5 |

### E 族：语义 / 根因类（内核，8 条）

| # | 模式 | 代表事故 | 根因 | 检测 |
|---|---|---|---|---|
| E1 | **失败收口缺失 → 静默死锁掩盖真 bug** | VM noaddr/AccessViolation/Err 出口不清 PAGEFAULT → RS 永停静默死锁；补 SIGSEGV 收口后暴露底层 null-deref（「这是暴露，不是回归」，NK4-A Task A）。同族：RTS_SET 出队半缺失（NK2 vmctl_clear）、阻塞后不出队 spin-pick 86,444 次（NK1 缺陷3）、唤醒入队半缺失（NK1 缺陷5）、重挂起 spin-pick 10.6 万次（迭代14）——**「置旗不出队/入队」同族病前后 5 例** | C 宏（RTS_SET/RTS_UNSET）的半边语义在 Rust 里没有单一对应物 | →P2 清单（noaddr 收口测试）+§5（状态迁移必须成对审查 enqueue/dequeue 半） |
| E2 | **上下文撕裂 / 寄存器多身份** | IRQ/tick 臂不存全量帧 → 陈旧 ctx 恢复（C mpx.S 每入口 SAVE_PROCESS_CTX 不变量，迭代20）；frame 与 ctx 分管不同寄存器=任何只更新其一的路径交付撕裂寄存器文件（迭代21 副产品）；RBX 三重身份（ps_strings 出生值/IPC 状态寄存器/callee-saved 活值，NK4-A Task C 全史） | C 的 p_reg 单一快照不变量在 Rust frame/ctx 分家后失守 | →P2 清单（IRQ 镜像测试）+§5（撕裂风险在 SMP 收口轮审视清单） |
| E3 | **移植语义面：i386 caller-saved → x86-64 callee-saved** | C 只有 i386（bx）/earm（r1）定义 IPC_STATUS_REG；x86_64 选 RBX（callee-saved）使「内核写状态寄存器」从无害变摧毁用户活值（NK4B P1 三案上交） | 寄存器模型差异未提升为显式架构裁决 | →§5（架构级裁决上交，不自行定案） |
| E4 | **限次探针上限被高频事件耗尽** | pf-save cap 8/48 两次全部消耗在启动前段同一 refault 事件、崩溃现场未捕获——「连续第二次犯同一错误」；对策=按目标进程过滤+按 (rip,rbx) 去重+上限按预期重复次数设（NK4-A 第五/六轮自认缺陷） | 上限按串口可读性而非事件频度设 | →§5（探针纪律，NK4A-TODO §5 已固化；P12 兜架构门半边） |
| E5 | **穷举不全仍称穷尽** | 第 6 轮布防 5 类写点实际只布 3 站点，「五类写点零命中」结论不足以覆盖写者全集（NK4-A 第六轮补充自曝）；「全仓三处提到 SUM」实扫两目录（M4.4 更正） | 布防/断言先于全枚举 | →§5（穷举主张先出全枚举清单再布防；范围词=命令根） |
| E6 | **无定性不修复** | 铁律 #10：同一问题 3 轮无根因实证 → BLOCKED，不做无定性修复（NK4-A Task C 六轮、NK4B M4.4）；对照：迭代20 的 IRQ 存帧修是「独立成立的 C 偏差」即使证伪于本例也保留 | 防止用改动制造进展感 | →§5（已有任务书铁律，此处收录） |
| E7 | **阻塞前提过期** | SIGACT 件登记的阻塞理由（「裸 sigreturn 桩依赖 S3 投递臂」）被三事推翻（「跨轮状态陈旧 CTOS」，review-patterns 在案）；M4.1 前置件清单探错包名、M4.3 当场更正 | 领取时不核实登记前提 | →§5（动手前核实前提） |
| E8 | **服务端能力在、客户端缺位 = 死能力** | MIB_SYSCTL 服务端全真、客户端缺位 → 服务端全是无人问津的死能力、init 三件 ENOSYS（NS11-A）；RS_INIT 六服务器无一应答（NS1） | 只交付单侧半 | →§5（能力完整性两端对账） |

### F 族：冒烟契约 / 对外产物 / 环境陷阱（6 条）

| # | 模式 | 代表事故 | 根因 | 检测 |
|---|---|---|---|---|
| F1 | **冒烟契约 marker 无 emit 点** | T1 契约串 'entering scheduler' 全仓无 emit 点，smoke stage-3 永远过不去（edge1 迭代7 追记，NK4-A C-3 排查发现） | 判据写进脚本但无人负责 emit | →P5 |
| F2 | **对外产物字节漂移** | startup.nsh 的 `cd EFI\BOOT` 被顺手改成带尾反斜杠（行为等价但越线：任务书要求一字不动）——「宿主测试没有任何一条断言过 startup.nsh 的字节，漂移无人守」（M3.3 补记三）；对策=钉**字节**而非「能不能启动」+ 判别变异 | 冻结基线无字节门 | →P2 清单（startup_nsh 字节门测试） |
| F3 | **CRLF 入库** | aarch64.ld CRLF 入库（链接器容忍→全绿肉眼看不出，M3.2 评审复核抓到）；**M4.2 新写的 riscv64.ld 同病没人再查**（本次 P8 首跑实测在案）+ 4 个 .rs 文件 | 工具生成/跨平台编辑无 eol 门 | →P8（基线 5 处，新增即拦） |
| F4 | **会话产物误入库** | f1041b3f1 落盘 543 文件 +35.7 万行（tmp/.zcode/.trae 等），Fix #9 修 2 分流 + .gitignore 加防复发规则 | 无 ignore 防线 | →P9 |
| F5 | **工具缺位伪装成被测物错误** | 宿主 nm 不认 aarch64 ELF → 三条布局断言集体假 FAIL，读者会误判「镜像布局错了」（M3.2 CodeReview P2#1）；对策=先探测工具、失败原因落在工具上 | 判据未区分「工具失败」与「对象失败」 | →§5（check-layout.sh 已修，收录为先例） |
| F6 | **环境陷阱**（各有判例，处置法见 §5）：docker 不带 `-u` 写脏 target root 属主（假失败，M4.2 注记）；裸机工件是 deps/ 硬链接不能用时间戳判断是否重产（M3.3 假证据自纠）；`cargo clean -p` 不删目标架构工件 → 「无工件」现场造假失败（同上）；pkill 与 qemu 同命令行自匹配（M4.3 + NK4A-TODO §5）；grep BRE `\+` 是量词（M4.4 方法伪影第5次）；`head -6` 截断证据（NK4B P0 T0.2）；`grep -c PASS` 被自己注释里的字串误命中（M4.3 教训连踩两次）；SUM 子串误命中 RESUME（M4.4） | 证据链工具的隐性语义 | →§5（证据命令锚定输出格式） |

---

## §3 与既有 85 模式（prompt/review-rules/review-patterns.md）的对照

本次**新发现**（85 模式未覆盖，未登记进规则集——按用户裁决本次仅目录化）：

| 新模式 | 最接近的既有模式 | 差异点 |
|---|---|---|
| B1/B5 canned 双方各自为政恒绿 | 「依赖全局状态」（40）| 40 讲 flaky，本模式讲 **假绿**：替身无语义校验使两侧缺陷互相不可见 |
| D5 退出码被吞 | 「外部调用返回值被无说明忽略」（32） | 32 讲代码级，本模式讲**脚本/CI 层**失败改判 skip |
| D6 check 不查 inline asm / 静默跳过当通过 | 「测试未覆盖设计决策」（12） | 本模式是**验证工具能力边界**（codegen 期校验、工件存在性判据） |
| F1 契约 marker 无 emit 点 | 「跨文档阶段状态表漂移」（58） | 本模式是**契约↔实现双向**漂移，58 是单向文档漂移 |
| A2 FIXLOG 序号撞号 | 无 | 并发共享 gitignored 账本特有 |
| C4 夹具断链阻断多 crate | 无 | trait 演进的夹具半 |
| E1 置旗不出队/入队半 | 「资源获取后无释放路径」（33） | 33 讲资源，本模式讲**状态机迁移的配对半** |
| E4 探针上限被高频事件耗尽 | 无 | 取证方法论 |
| E5 穷举不全仍称穷尽 | 「因果链编造」（48）邻接 | 48 是编造，本模式是**以抽样冒充全集** |
| F6 族证据命令伪影 | 「无锚点知识点断言」（83）邻接 | 83 讲断言无锚点，本模式是**锚点命令本身的语义陷阱**（BRE/子串/截断） |

已被既有模式覆盖、本次仅补实例的：「裸整数表达语义 / C 式空指针哨兵 / 裸 as 截断」（16/17/20→B4）、
「注释理由虚假或牵强」（34→A4 邻接）、「因果链编造」（48→假归因「nt-tests 并发合入」实例）、
「参考代码路径漂移」（65/66→A5）、「跨轮状态陈旧」（70→A3/E7）、「代码注释行号漂移」（77→A5）、
「C 源码 bug 未显式标注」（78→B3 邻接）、「target-specific cfg 泄漏」（82→P12 同域；P12 落在 CI/build 门
这一 82 未覆盖的 enforcement 半边）、「无锚点知识点断言」（83→A5）。

---

## §4 机械检查映射（tools/pattern-gate.sh）

| 检查 | 级别 | 对应模式 | 事故出处（条目定位） | 实现要点 |
|---|---|---|---|---|
| P1 | gate | A1 | new_edge4 §1 规则 5；FIXLOG.md 头部归档注记 | 每份 edge*/FIXLOG.md 标题行=1 + 最大连续重复行≤5；.review 缺席 SKIP |
| P2 | gate | C6 | 本表下方 20 测试清单 | `fn <name>` 全 os/ grep ≥1 |
| P3 | gate | B1 | edge1 Fix #5（NK8）；C-59（edge4 §2 :74） | SELF/MIB_CALL_SYSCTL 派生断言 + `= -2/-3` 字面量零命中 + 0x1800 pin 在位 |
| P4 | gate | C4 | edge3 Fix #142（夹具断链）；os/tests [[test]] 纪律 | .rs ↔ [[test]] 双向 comm |
| P5 | gate | F1 | edge1 Fix #9 迭代7 追记（T1 marker） | 配对表：marker 在引用方脚本 + emit 范围可 grep |
| P6 | gate | D6 | OQ-1（5ba2644b8 注释 + edge1 迭代21 补记） | job 存在 + aarch64/riscv64 build 腿存在 + `cargo check -p minix-kernel` 零命中 |
| P7 | gate+基线 | D5 | M3.1（edge1 NK4-B P3 M3.1 补记）；run_all.sh 三处 | `(cargo\|bash\|sh) ... \|\| echo` 全量对账基线；本脚本自身豁免 |
| P8 | gate+基线 | F3 | NK4B M3.2 评审复核（aarch64.ld）；riscv64.ld 为本次首跑新发现 | `git ls-files --eol` i/crlf × {ld,sh,rs} |
| P9 | gate | F4 | edge1 Fix #9 修 2（落盘分流） | .gitignore 三条规则在位（.wt/、/tmp/*、!/tmp/nk4a/） |
| P10 | report + diff 门 | A2 | edge3 archive 索引撞号注记；Fix #126 教训 | tracked todo/new_todo 的 `Fix #N` 引用：存量 489 处只报告，--diff 拦新增 |
| P11 | diff 门 | A5 | NK4B M4.4 五轮评审（七组 C 对位缺陷） | 增量行 `.c:NNN` token：basename 在 minix3/ 可解析 + 行号落窗（多变体任一覆盖即过；基线可豁免引例） |
| P12 | report+基线 | D6/E4 | 9e115387e / ab79b40ba（M3.1 补记「为什么没人发现」） | 共享路径 4 文件中 asm!/rdmsr 行向上 6 行无 target_arch → 基线对账 |
| P13 | diff 门 | A4 | edge4 批A 附带发现；Fix #124① | 增量行反引号全大写带下划线 token 在 os/+minix3/ 存在性；PATTERN-SCAN-REPORT 自身豁免 |

**P2 清单（20 个防回归测试 ↔ 事故）**：`test_sys_times_self_sentinel_authority_pin`（NK8 SELF 化石值）、
`test_grantee_gate_any_authority_value`（NS2 ANY 哨兵）、`irq_entry_mirrors_user_frame_but_skips_kernel_origin`
（NK4-A 迭代20 IRQ 存帧）、`test_pagefault_unknown_region_sigsegv_and_clears_park`（NK4-A Task A noaddr 收口）、
`test_vmctl_clear_page_fault_requeues_target`（NK2 RTS_UNSET 出队半）、`test_vm_mmap_stack_lanes_carry_above_4gib`
（NS5-A u32 截断）、`test_flags_for_priv_proc`（C-28 PRIV_PROC）、
`startup_nsh_bytes_are_the_frozen_template_per_loader_name`（M3.3 startup.nsh 字节门）、
`boot_module_order_matches_boot_shim_module_names`（NS8 装机三重锁）、
`test_step0_creates_rproctab_grant_over_wire_mirror`（NS2 rproctab 授权）、
`test_cdev_ioctl_continuation_decodes_payload`（NS7-A 恒假门）、`test_lookup_label_hosted_is_none`（NL4 driver-rt）、
`test_feed_keyboard_byte_uses_full_table`（NL4 死缝）、`test_sigsuspend_carries_mask_only`（SIGACT 删死参）、
`test_sigaction_carries_sigreturn_stub`（NS11 桩地址）、`test_rs_init_birth_answered_with_ok`（NS1 出生应答）、
`load_process_elf_two_images_same_va_no_collision`（NK1 per-process 根）、`trampoline_address_is_nonzero`（NL3）、
`test_dispatch_times_self_replacement`（NK8 判别补强）、`test_check_gic_madt_decision_table`（C-38 决策表）。

**用法与语义**：

```bash
bash tools/pattern-gate.sh                    # 全量（P7/P8/P12 基线对账；基线外 FAIL）
bash tools/pattern-gate.sh --diff [RANGE]     # 附加增量门（P10/P11/P13 只看新增行）
bash tools/pattern-gate.sh --update-baseline  # 冻结当前存量进 baseline（人工复核后）
bash tools/pattern-gate.sh --self-test        # 18 例正反判别矩阵（mktemp 夹具，跑完即删）
```

退出码 0/1/2 照 unsafe-audit 惯例。基线 key=`检查名|路径|cksum(行内容)`——对行号漂移免疫、对内容改动敏感
（改了被豁免的行 = 当新违规重报，符合「动它就要处理它」）。判别证据：
`tmp/evidence/20260923-c61-pattern-gate/{selftest,mutation-negative,full-run-worktree}.log`——
selftest 18/18 全绿；真树变异 6 组（P3/P4/P5/P6/P7 注入真实违规全部现形，P6 拆出「缺腿」「check 退化」双断言），
还原后复绿 PASS=9 FAIL=0。

**局限（如实登记）**：①P12 是 6 行窗启发式，抓「cfg 紧邻缺失」形态，cfg 在更外层块的情况看不见——结构性拦截
靠 P6 的 CI build 门（check 不查 asm，同理 grep 也查不出）；②P1 只在主树有意义（.review 不入 git），worktree/CI
下 SKIP；③P2 清单是点名式，新事故的防回归测试要人工追加进 `P2_TESTS`（追加动作本身写进 §5 纪律）；
④P4/P5/P6/P9 只查结构存在性，语义正确性仍靠既有 cargo/CI 门。

---

## §5 不可机械化的流程纪律（已有判例与出处，供 review 对照）

1. **共享主树 git 三条**（D1/D2）：commit 一律 `--only <显式文件>`；共享分支禁 `--amend`；
   对账基线用 stash 或已提交态，禁 checkout 未提交树；update-ref/FF 后先 `git status` 核 staged。
   出处：archive :3222 两起 + STTY-G 双事故 + C-35 + Fix #141 过程事故。
2. **断言先跑再写，不凭记忆**（C1）；失败臂测试必须反证有牙（C2，edge4 批B「反证跑」判例）；
   过滤单跑必须绿（C3）；批量改码后必须 test-profile 编译（C4，Fix #129）。
3. **穷举先于布防/断言**（E5）：「全仓无一处 X」必须把 grep 完整路径参数抄进证据行，范围词=命令根；
   计数型断言写明由哪条命令哪个输出段得来；锚点要么锁输出格式要么锁字串（M4.3/M4.4 连续踩坑沉淀）。
4. **状态机迁移成对审查**（E1）：每置旗/清旗问 enqueue/dequeue 配对半；每加 wire 车道问 client/server 双侧
   对偶测试（B2/B5）；跨号域边界（errno 正负）pin 两相（B3）。
5. **验证工具能力边界**（D6/C5）：check 不查 inline asm——跨架构门必须 build；required-features 静默跳过——
   判据看工件不看退出码；guest 构建走消费方真实 feature 面。
6. **探针纪律**（E4，NK4A-TODO §5 已固化）：限次+cap 按目标事件预期频度设、按目标进程过滤、去重、
   `#[cfg(not(feature="mock"))]` + 架构门、「task1-close 裁决删除」注记。
7. **能力完整性两端对账**（E8）+ **登记前提动手前核实**（E7）+ **3 轮无定性转 BLOCKED**（E6）。
8. **证据命令防伪影**（F6）：`grep -F` 优先于 BRE 转义；`head` 截断不得当证据；存在性用 `find` 不用内容
   grep；锚点自查表对自写注释复跑一遍。

## §6 后续可选（本次不做，用户已裁决）

- CI 接线（新增 workflow 跑本脚本纯文本检查）——用户裁决暂不接。
- 新模式登记进 `prompt/review-rules/review-patterns.md`（B5/D5/D6/F1 等 10 条新发现）——需按 prompt/README
  同步三端派生文件并跑 check-review-rules.sh，另开任务。
- run_all.sh 三处吞退出码（P7 基线）在 P6 清理后出基线转真拦。
- P8 基线里 riscv64.ld 与 4 个 .rs 的 CRLF 建议顺手转 LF 后出基线。

---

## §7 增量扫描（C-62；截断面 3643c9331→a1b8b1663，NK4-C 274 笔）

> 承接 §1-§6 的首轮（C-61，截断面 3643c9331）。本轮为**增量轮**：语料边界=git log 3643c9331..a1b8b1663
>（274 笔，主体 NK4-C）+ 下表新增/增长文件。方法同 §1：全文/定向精读 → 对照 §2 六族 34 模式判新旧 →
> 可机械化者落 gate。**结论先行：无全新失败家族，新增模式全部可归并进既有六族（多数是高频家族的第 3-5 次
> 独立复发——复发频次本身印证了目录的价值）；机械面新增 2 个检查（P14/P15）、P2 清单 +7、P12 启发式修正。**

### 7.1 增量语料

| 语料 | 形态 | 说明 |
|---|---|---|
| `NK4C-WORKLOG.md` | **新**，5237 行 | 顶部「当前状态」摘要链（§1.89-§1.119）+ S0-S3 精读；164 节正文按 grep 定点取用 |
| `NK4C-REVIEW-REPORT-20260927-R3.md` | **新**，196 行 | 全文精读。214 笔机械对账（213 过/1 卫生违规/2 正则误报澄清）+ 交接方自审 4 项实锤 |
| `NK4C-REVIEW-REPORT-20260923{,-R2}.md` | **新** | 前两轮评审（R2 覆盖至 e90efbcd8） |
| `NK4C-RESUME-PROMPT.md` | **新**，361 行 | 全文精读。§4.2 探针纪律 + §9 已知陷阱清单（10 条）——纪律固化文档本身 |
| `NK4C-OPENING-PROMPT.md` | **新**，366 行 | 铁律与停止条件（与 NK4A/B 同构，增量读） |
| `.review/zcode/edge1/FIXLOG.md` | 2219→2640 行 | 迭代22-25 补记（NK4-C 阶段 0-1.2：A 案 R10 + Task C 第 10-19 轮取证） |
| `01-stage-kernel/firmware-heap-corruption.md` | **新** | 固件堆腐化教学文档（跨页表 `&'static` 失效 → landing pad，fix27c 同病第 3 发的固化） |
| `NK4A-HANDOFF-STATUS.md` | **新**，435 行 | 移交状态（佐证用） |
| `edge_todo.md` / `new_edge4.md` | 增量 | C-6x 登记行更新 |

### 7.2 新增模式（全部归并进 §2 既有族；标注第 N 次独立复发）

**B 族（wire/常量/协议）——复发密集区：**

| 归并 | 新实例（NK4-C 条目） | 说明 |
|---|---|---|
| B1（第 3 发）| §1.105 `CdevRequest::Open as i32`＝索引 0 ≠ 消息号 0x400；同族 `Select as i32` | 与 NS7-A（Ioctl 0x404）、edge3 #97「驱动消息位次是猜的」同一模式三次独立发生——fieldless enum 判别值冒充线路类型号。**机械面**：P2 清单新增 `test_open_request_wire_type_is_cdev_open_not_index` |
| B2（回复载荷丢失）| §1.106 stat 回复腿 `Ok(()) => zero()` 纯状态回复、从不 copy_out 到用户缓冲 → `ls` 判目录失败；同族 StatVfs 同病登记 TODO | 「回复＝状态码」而忘了载荷半。配套发现死代码 `write_to` 88B 伪布局（全仓无调用者）。**机械面**：P2 + `test_stat_streams_struct_stat_through_copy_out` |
| B7（repr(C)/布局）| 1.12d `SLOT=80` 与同文件 size 断言 `16+size_of=96` 自相矛盾（越界 UB）+ 方向互换两臂与 doc 契约相反（R3 F-SELF-1）；§1.115 BootHandoff 嵌套类型 repr(Rust)→4 类型补 `#[repr(C)]` | R3 判词：**「const 断言必须作为常量来源而非事后证明」**——断言写对了但代码不消费它。**机械面**：offset_of!/size 断言守卫测试已是仓内范式（P2 既有清单覆盖形态） |
| B3（载荷语义）| §1.117 `while waitpid(-1).is_ok()` 误译 C `while waitpid(...)>0`——`Ok(0)`（有活子无退出）永真 → INIT 忙等 6 万次/40s | **Ok 类型 ≠ 业务成功：成功语义在载荷不在 Result 类型**。修复主动清扫同族 RS SIGCHLD 排空环（一次修复两处同族）。**机械面**：P2 + `test_{entity,runetcrc}_fork_failure_reap_loop_exits_on_zero` |

**E 族（内核语义/根因）：**

| 归并 | 新实例 | 说明 |
|---|---|---|
| E 族·新形态「容量 ≠ 拓扑」| §1.118 SCHED `loads:[CpuLoad; MAX_CPUS=32]` 初始化满容量，`pick` 迭代不受 `processors_count` 约束 → 4 核机选中幽灵核 4..31 → EINVAL → INIT fork EPERM 活锁 | C `pick_cpu`（schedule.c:67）只循环 `processors_count`；数组满容量初始化 + 满容量遍历破坏契约。修复带双向判别回归测试。**机械面**：P2 + `test_phantom_seats_beyond_topology` |
| E 族·新形态「容量 vs 记账双口径」| §1.111 `total_pages` 旧值=记账累加（对位 C mem_init），但位图按绝对页号寻址——aarch64 RAM 基址 0x40000000 页号自 262144 起 → 容量 2800 位图全面越界；修复后又暴露 vsi_total 记账口径混淆（抬高 1GiB）→ 拆双字段 | **位图容量（最高绝对页号）与可用页记账是两个量**，C 里本就分开。**机械面**：P2 + `test_boot_params_validate_total_pages_mismatch` |
| E1（置旗/入队配对半·持续复发）| F10d 家族三种变体（privctl 绕过 RTS 宏 → runnable=yes queued=no；`clear_ipc_refs` 裸 clear；`do_trace` 裸 set/clear）——RESUME-PROMPT §9.8 已固化为陷阱条款 | **机械面（本轮新增 P14）**：grep `p_rts_flags.(set|clear|insert|remove)` 非 proc.rs 定义域的调用点，基线对账（存量 219 处冻结，新增即抓人审）。启发式无法判「是否补了入队出队半」（语义判断），门的价值是强制改 RTS 语义者过审视线 |
| E2/E3（上下文/移植语义）| §1.112/113：aarch64 无 TSS.sp0 硬件重装，阻塞腿不弹帧 → SP_EL1 每轮空闲 receive 泄漏 288B 下行踩 .bss（x86 免疫的跨架构不对称）；§1.109 `msr SP_EL1` 在 SPSel=1 UNDEFINED；§1.115 `tlbi alle1is` 刷 stage-2 在 EL1-only 体系 UNDEFINED；§1.110 APTable 极性与 x86 USER 位相反（=1 是禁止）；§1.111 `write_pte_dm` 无条件发射 tlbi 而 x86 同名函数早有 KernelDm 门控（跨架构同形函数的隐式前提） | CodeReview Critical-1 抓住栈纪律；「把静默内存踩踏换成显式 panic」是本轮反复出现的正面范式（fail-fast 优先于静默） |
| E 族·新形态「additive resume」| §1.115 kernel-image 调 `arch_boot` 把**已装载进 TTBR 的 L0 清零**摧毁自身高半映射；正解=`arch_boot_resume_high_half` 纯 additive + 各镜像独立 .bss 副本 | **重初始化路径不得清正激活的状态；跨镜像交接数据必须落本镜像自己的存储** |
| E4（取证仪器）| §1.99→§1.100 committed 探针每秒 ~460 行刷屏**掩盖了真死锁**（探针 DoS = observer effect）；§1.118 aarch64 userland `console_write` 根本不落 serial（唯 diagctl 可见）→「先前所有靠 console 串推的退出形态皆不可信」；diagctl 在活锁洪流下非确定性丢弃（同码一次 14 行一次 0 行）→ 正解=IPC reply m_type 回传码分区 | 新形态：**取证通道本身要先验证**（E4 原形态是上限耗尽，本轮是通道可达性/可靠性）；「洪流下串口不可靠，用带内通道回传判定值」已固化为 NK4-C 方法论 |

**C 族（测试有效性）：**

| 归并 | 新实例 | 说明 |
|---|---|---|
| C2（修复侧新形态）| 1.11e：taskcall ELOCKED 重试**两版从未实际生效**——重试读 reply 语义（rv==208）而内核走 syscall 错误臂 | 「修复是否生效」要验证失败路径真的走到修复代码；与「碰巧通过的测试」同根：验证点与执行路径错位 |
| C2（架构级判词）| R3 F-SELF-2：drain 代停车在偏离 C 的架构内把补丁打对（真机验证通过），后被 B24 按 C proc.c:569-583 结构性取代 | **「验证通过 ≠ 架构正确」**——局部正确性无法救赎架构偏差；应更早对照 C 质疑架构本身 |
| C1（取证判别力）| R3 F-SELF-1①：探针链只验证了 range check 未验证数据内容（「探针验证了检查通过却没验证读到的字节」）——方向互换漏诊 | 探针断言的判别力要与缺陷形态匹配（守卫缺陷验守卫、载荷缺陷验载荷） |

**D/F 族（流程/环境）：**

| 归并 | 新实例 | 说明 |
|---|---|---|
| D1（git add -A）| d6f176451 `git add -A os/` 误提交 `os/.dockercargo/registry/` 158 文件 4.7MB vendored 依赖源码且 .gitignore 无条目（R3 唯一卫生违规，登记待清） | **机械面（本轮新增 P15）**：vendored 目录跟踪数=0 + gitignore 条目在位；本单已执行 R3 待办 1（git rm -r --cached + gitignore），门从出生即真拦 |
| F3（CRLF 第 4 发）| `os/libs/minix-boot/src/handoff.rs`（§1.115 新文件）CRLF 入库——本轮 P8 首跑即抓 | 与 aarch64.ld→riscv64.ld→4×.rs 同族持续蔓延；进基线（NK4-C 活跃文件不触碰），建议 NK4-C 收线时统一转 LF |
| D6/P12（架构门第 3 发）| §1.107 `boot_init_timer` 无条件调用 x86 专属 `pic_init` 等（aarch64 E0425）——同族第三次；本轮 P12 对 HEAD 实跑新抓 6 处形态命中，人工复核=2 处启发式误报（doc 注释里的 `naked_asm!`、cfg 门在 >6 行窗外的 GS 采样探针）+ 其余为 committed 取证基础设施 | **机械面（本轮修正 P12 启发式）**：排除纯注释行 + 窗口 6→12 行；修正后存量 3 处进基线（boot-shim asm、方案 A 跳板、cr2 探针）。结构性拦截仍靠 P6 的 CI build 门 |
| F6（证据命令伪影）| R3 P6：日志含 NUL 字节时 grep 计数不可靠（以 python 字节级统计为准）；R3 对账脚本正则误报 2 笔（未豁免 message 明写「含代码」的 commit） | F6 族持续积累：证据工具的隐性语义 |

**正面范式沉淀（§5 补充）**：金丝雀/毒化实验（第 10-16 轮：canary 注入判「保存即错 vs 保存后被改」、毒化扩散追踪级联损伤、PA 别名检测）——强判别力取证实验的设计模板；哨兵分区法（`si res=902+22=924` 用哨兵值+错误码分区钉死错误来源层）；误诊诚实翻案文化（§1.116 翻 §1.115、§1.118续 翻 §1.118、§1.119 翻静态刻画，R3 确认「未发现虚构取证」）；QEMU 上游源码实读钉死 GICR 行为（R3 评语 exemplary）。

### 7.3 机械面变化汇总

| 变化 | 内容 |
|---|---|
| P12 修正 | 启发式排除纯注释行 + cfg 门窗口 6→12 行（本轮 6 处 FAIL 人工复核 2 处误报驱动）；修正后实抓 3 处 committed 取证基础设施进基线 |
| P14 新增 | RTS 裸 set/clear 对账（F10d 家族三变体驱动）；存量 219 处冻结——启发式不判语义合法性，门价值=新增强制过审；selftest 正反例 + 真树变异验证 |
| P15 新增 | vendored 目录防线（d6f176451 事故驱动）；ls-files 半 + gitignore 半；本单执行 R3 待办 1 后从出生即绿 |
| P2_TESTS +7 | phantom_seats（§1.118）/ open_request_wire_type（§1.105）/ stat_streams（§1.106）/ do_fork_downgrades（B48）/ total_pages_mismatch（§1.111）/ reap_loop×2（§1.117）——全部 grep 实证在位后入列，27/27 |
| baseline 重生成 | 222 keys（P7×3 + P8×6 + P12×3 + P14×219 + ……）；P8 新增 handoff.rs |
| .dockercargo 修复 | gitignore 条目 + `git rm -r --cached`（158 文件解除跟踪，盘面保留，docker 构建不受影响）——执行 NK4C R3 报告 §7.3 待办 1 |

### 7.4 判别证据（tmp/evidence/20260927-c62-pattern-gate/）

selftest 24 例（+P14×2/P15×2）正反矩阵全绿；真树变异：P14 新增裸 set 即 FAIL、P15 gitignore 删条目即 FAIL、还原复绿；full 模式 PASS=11 FAIL=0 SKIP=1（worktree）/ 主树合并后终验。**过程事故如实记**：变异还原用 `git checkout -- .gitignore` 把自己未提交的 gitignore 条目一并冲掉（P15 假 FAIL 一次）——变异脚本的还原必须用「改前备份/改后还原」，对未提交在制禁 checkout（D2 模式第 N 次现世，本次受害者是检查者自己）。# PATTERN-SCAN-REPORT-20260923 — 历史日志模式挖掘与回归检查（C-61）

> **任务**：用户指令「从 FIXLOG、WORKLOG 等历史 LOG 中扫描并发掘模式，扫描得到测试，避免以后再出现类似的问题」。
> 交付三件：本目录文档 + [`tools/pattern-gate.sh`](../../tools/pattern-gate.sh)（13 项机械检查）+
> [`tools/pattern-gate-baseline.txt`](../../tools/pattern-gate-baseline.txt)（存量豁免）。
> 判别证据：`tmp/evidence/20260923-c61-pattern-gate/`（selftest 18 例正反矩阵 + 真树变异负例 6 组 + 全量运行实录）。
> **更正纪律**：本档断言全部可用所引条目标题 grep 复位；引用 FIXLOG 用「线 + 条目标题」定位（裸行号会漂移，
> 即 review-patterns 所称「代码注释行号漂移」一类），引用代码用符号名。发现写错就地追加更正节，不改写原条目。

---

## §1 扫描语料与方法

**语料**（全部精读或定向提取）：

| 语料 | 形态 | 说明 |
|---|---|---|
| `.review/zcode/edge1/FIXLOG.md` | 2219 行 | 全文精读。含 NK2/NK8/NK1/NK4-A 迭代1-21、NK4-B M3/M4 全系列补记 |
| `.review/zcode/edge2/FIXLOG.md` | 301 行 | 全文精读。L17 三批 + NL/NK/C 系列执行记录 |
| `.review/zcode/edge3/FIXLOG.md`（现行本） | 616 行 | 全文精读。Fix #122-#148 |
| `.review/zcode/edge3/FIXLOG_archive.md` | 3659 行 | 定向精读：流程事故段（#3222 事故两起、#233 流程教训）+ P0 代表条目 |
| `.review/zcode/edge3/FIXLOG.raw-20260920.bak` | 68552 行 | 不通读；与 archive 做 sort -u 差异核查——仅 8 行重复拼接伪影，**无独有内容丢失** |
| `.review/zcode/edge4/FIXLOG.md` | 128 行 | 全文精读 |
| `rewrite-notes/coordination/NK4A-QWEN-WORKLOG.md` | 471 行 | 全文精读（Task A/B/C 六轮取证） |
| `rewrite-notes/coordination/NK4B-WORKLOG.md` | 2446 行 | P0 核账 + P1 节全文精读；M3/M4 系列经 edge1 FIXLOG 补记覆盖 |
| `NK4A-REVIEW-REPORT.md`、`NK4-REGRESSION-REVIEW-20260922{,-PART2}.md` | 定向提取 | 评审归纳节（危险面、可沉淀资产、OQ-1 建议） |
| `new_edge1-4.md`、`edge_todo.md` | 定向提取 | §1 协作规则 + §2 认领板 C-XX 登记行教训 + 账本行判例 |
| `NK4A-TODO.md` §5/§9、`NK4B-TODO.md` §8、`NK4B-OPENING-PROMPT.md` | 定向提取 | 铁律条款 |
| `AI-chats/daily.todo.md` | 定向提取 | 教训条目（含未提交修改） |
| `.review/zcode/{fork-syscall-rewrite,runtime}/` STATE/fix-status | 定向提取 | P0/P1 级教训条目 |
| `prompt/review-rules/review-patterns.md` | 索引级 | 85 既有模式对照（§3），避免重复立模式 |

**方法**：先全文精读成稿（工作笔记先行，防上下文压缩丢失），再对每个候选模式问三个问题——
「在几份独立日志里出现过（频次）？」「当时怎么发现/怎么修的（已有对策）？」「能不能用 grep/脚本机械拦住
（检测方式）？」。能机械拦的落成 `pattern-gate.sh` 的 P 检查项；不能的进 §5 流程纪律清单。

---

## §2 模式目录（六族 34 条）

编号供人引用；「→P#」表示已落成 pattern-gate 检查项，「→§5」表示归入流程纪律。

### A 族：账面 / 日志类

| # | 模式 | 代表事故（出处条目） | 根因 | 检测 |
|---|---|---|---|---|
| A1 | **日志全量重复追加** | edge3 FIXLOG 膨胀到 68,552 行（同内容 32 遍、95.3% 重复行）；对策=追加前 `grep -c "^# "` 必须为 1（new_edge4 §1 规则 5） | 会话收尾把整文件重当增量写 | →P1 |
| A2 | **FIXLOG 编号撞号** | #117×3、#118×2、#119×2、#101/#102/#103/#104/#105 各×3、#125 撞号（edge3 archive 索引 + Fix #126 撞号教训） | 多会话向共享 gitignored FIXLOG 各自追加；tracked 文档引用序号不可移植 | →P10（增量拦 todo/new_todo 裸引）+§5 |
| A3 | **账面失真/滞后** | edge_todo 52 条中 10 条滞后可闭（LEDGER-AUDIT 专项）；S30 F3c-2 ⏳ 主张已被 17c88be0c 全额落地；NS10 账面三件两件已闭；X-5 过期误报（「跨轮状态陈旧 CTOS」家族，review-patterns 在案） | 账本行与代码演进不同步；「已闭单勿领」（new_edge3 :45） | 半机械：领取前对码（§5）；tools/todo-staleness-check.sh 已有 |
| A4 | **文档/注释引用不存在的符号** | 设计文档 08-pm-srv-fork.md §2.4/§4.2 引用 `SRV_FORK_INHERIT_FLAGS`，全仓零命中；srv_fork.rs 注释同样虚构（edge4 批A 附带发现 / Fix #124①） | 凭设计意图写「代码事实」 | →P13 |
| A5 | **C 锚点漂移** | `kernel/memory.c`、`krandom.c`、`arch/i386/smp.c` 不存在；`protect.c` 三处偏 7-18 行；行号上的角色标签错（打印者标成调用者）；「全仓无一处」实扫两目录（NK4B M4.4 五轮评审 + 旁支修复） | 凭记忆写锚点；范围词大于命令根 | →P11（增量）；存量靠 review-line-check.sh |

### B 族：wire / 常量 / 协议类（重灾区，7 条）

| # | 模式 | 代表事故 | 根因 | 检测 |
|---|---|---|---|---|
| B1 | **哨兵/调用号字面量化石值** | `SELF=-2`（真值 31742，-2 恰为 SYSTEM 端点，9 消费点永不命中，真机 wire 层才暴露）；`ANY=-3`；`MIB_CALL_SYSCTL=0x600`（权威 0x1800）；枚举序数当消息号（`CdevRequest::Ioctl as i32`=4 ≠ 0x404） | 手抄字面量 + canned 测试无端点语义校验 → 恒绿 | →P3（权威派生断言 + 0x1800 pin）；枚举序数族由 P2 清单里的 NS7-A 测试钉 |
| B2 | **回复车道偏移错位** | lseek 回复被覆盖（Fix #70）、Read 回复布局（Fix #50）、回复状态偏移（Fix #87）、fchmod mode @8 vs C @4（C-45）、PM GetTimeOfDay nsec @16 vs @8（C-54）、VM_MMAP 回程 m1p1@16 vs @40（NS5-A） | 回复 overlay 的 C 结构权威没有双侧对拍 | →P2 清单（车道 pin 测试存在性）+§5（新 lane 必须双侧对偶测试） |
| B3 | **符号域 / 正负号折算** | ERESTART +200 vs 线上 -200（edge3 暂停交接登记）；MIB 应答「负 m_type 判失败」vs C 正 errno 协议（C-59②，ENOMEM/ENOSYS 被当成功）；`KcallResult::Ok(errno)` 正号上线、VM 网关 `< 0` 判错把失败读成成功（edge1 Fix #9 迭代3 F10，未修登记） | i386 负 errno 语义在 Rust `Result` 世界的手工折算点分散 | →§5（跨号域边界必须 pin 正负两相）；部分由 P2 清单覆盖 |
| B4 | **u32 车道截断** | VM_MMAP addr/len `as u32`（栈顶 0x7fff_ffff_f000 必中，T2 阻塞位）；MIB_SYSCTL 三地址 lane u32（guest 基址 5GiB 必截断）；LP64 前提勘误——`_ASSERT_MSG_SIZE` 钉 56 字节使 C 头只有 i386 形态（NS5-A/NS11-B） | C i386 头照搬 + 宿主 canned 不发真实地址 | →P2 清单（>4GiB 往返 pin） |
| B5 | **canned 双方各自为政恒绿** | MIB_SYSCTL 两侧 canned 各用各的常量恒绿（C-59①）；creat 臂从客户端恒零填充的 padding 区解内联路径=真 wire 缺陷被自造请求形掩盖（NL10） | 测试替身不校验语义 → 双方自洽、互相矛盾 | →§5（跨服务联调线束 E5 族的存在意义；修法见 C-59/C-45 判例） |
| B6 | **宿主不可达缺陷** | NS7-A 两处（消息号取枚举序数、`if status == 0` 恒假门）宿主发送恒败故测试测不出，真机 ioctl 首拍即暴露；GetTimeOfDay 臂时钟源无注入缝宿主单测不可达（C-54 登记） | 宿主传输半恒败路径下的代码不可达 | →P2 清单（wire 形状 pin）+§5（真机冒烟兜底） |
| B7 | **布局/对齐算术错** | ps_strings 紧凑 24B vs C LP64 自然对齐 32B（edge1 迭代7 F13）；`.sdata2` 误归 `.bss`（NOBITS 静默丢内容，M4.2 写作期纠正）；「恰 56」草稿算术未含对齐间隙（NS11-B，编译期 size 断言拦住） | 手算布局无编译期/测试期钉 | →§5（布局必须 repr(C)+size 断言+两侧 pin） |

### C 族：测试有效性类（6 条）

| # | 模式 | 代表事故 | 根因 | 检测 |
|---|---|---|---|---|
| C1 | **断言凭记忆虚构** | console 首次 open 断言期望 0，实返 CDEV_CTTY 0x40000000——「先跑测得 left=0x40000000 再改，不凭记忆」（edge2 F3） | 断言先写后跑 | →§5（断言先跑再写） |
| C2 | **空转测试 / 碰巧通过** | VM 握手失败臂测试漏注册 RS caller，失败分支从未到达恒绿（Fix #130「碰巧通过」）；creat 续接体「假覆盖」；对策=反证跑——把目标改错测试必须红（edge4 批B 判例） | 失败路径测试不证明路径可达 | →§5（失败臂测试必须反证有牙） |
| C3 | **测试自身缺陷** | dispatch_times 旧测试靠其它测试先初始化 CLOCK_STATE，过滤单跑必 panic（测试顺序耦合，Fix #5）；「存常量读回」同义反复断言（fix-status.md Fix #3）；GETCWD fake 四测首轮失败全是测试数据构造错 | 测试间隐式依赖 / 断言无判别力 | →§5（单跑过滤必须绿；判别断言） |
| C4 | **夹具漂移断链** | `NoopKernelGateway` 缺 `sys_diagctl_stacktrace` → minix-tests 全 crate E0046 且阻断 clippy 多 crate 会话、pm 警告面 15 条不可见（Fix #142）；批量补 trait 方法两版脚本都有 bug——**必须 `cargo test`（test-profile）不能只 `cargo build`**（Fix #129 教训） | trait 加方法未跟夹具；#[cfg(test)] 不参与普通 build | →P4（[[test]] 对账）+§5（test-profile 编译门） |
| C5 | **伪验证 / 非法组合** | `--features alloc-global,panic-handler,std` 直调 none 构建 144 错——验证必须走消费方真实 feature 面（Fix #147）；`cargo test --workspace --exclude minix-kernel` 组合下 feature 统一三错（NL4 预存登记） | feature 组合形态与生产消费面脱节 | →§5（guest 构建走真实消费面） |
| C6 | **防回归测试被静默删除** | 历史事故的防回归测试（本档 §4 P2 表 20 个）无删除守卫 | 删测试无门 | →P2 |

### D 族：流程 / 并发 / 工具类（6 条）

| # | 模式 | 代表事故 | 根因 | 检测 |
|---|---|---|---|---|
| D1 | **共享主树裸 commit / amend 卷走他人工作** | 三起：amend 吞对方文档提交+卷入对方暂存批（archive :3222 事故1）；裸 `git add`+commit 卷走 17 文件（同 :3222 事故2）；STTY-G `--amend` 把对方 1302 行 exec_worker 一起改写（edge2 FIXLOG STTY-G 事故）。恢复法：`commit-tree` 逐字节复原（哈希一致）/ update-ref 退回。对策三条已固化（--only 显式文件、共享分支禁 amend、update-ref 后核对 staged） | 共享 index 无隔离 | →§5（git 纪律；无 hook 基础设施，机械拦截成本高于纪律） |
| D2 | **reset/checkout 冲离在制** | NK5 checkout+reset 冲毁主树两组在制（C-35 机制化起源）；NS11-B clippy 对账 checkout 往返冲掉六文件、从会话记录重建（Fix #141 过程事故）；「基线对账禁 checkout 未提交树」 | 主树承载多线未提交在制 | →§5（claim.sh 机制 + 对账用 stash/已提交基线；规则 8 改完即 commit） |
| D3 | **共享 target 指纹互踩** | 树内 docker 构建踩共享 target，NS11-B 在制 u32→u64 编译错窜入他线（NL10）；E0425 假失败判别法=单包构建退 0 + 换 `CARGO_TARGET_DIR` 复跑 | 并发会话共享构建产物 | →§5（树内 target 判例） |
| D4 | **同文件并行编辑互相回滚** | 同响应内 SearchReplace 与 sed 并改一文件互相回滚（NL4）；批量字符串扫描脚本三处缺陷致 8 文件语法损坏，git checkout 回退重做+单管线+验证切片收敛（Fix #145 过程事故） | 工具并行改同一文件 | →§5（串行为准；批量变换后编译驱动验证） |
| D5 | **退出码被吞** | run_all.sh 三处构建循环 `|| echo "(build failed)"`——aarch64 构建断裂 17 错被改判 skip（M3.1 发现路径之一）；登记待 P6 清理 | 脚本容错写法掩盖失败 | →P7（基线冻结三处，新增即拦） |
| D6 | **CI 门用 check 不用 build / 判据看退出码不看工件** | OQ-1 变异实证：cargo check 不做 codegen，inline asm 寄存器/fixup 错误静默放过（check 0 错、build 报 invalid fixup）；required-features 不满足时 cargo 对 bin **静默跳过不报错**——判据必须看工件存在（M3.3 反向判别） | 把「命令没报错」当「验证通过」 | →P6（防删门+防 check 退化）+§5 |

### E 族：语义 / 根因类（内核，8 条）

| # | 模式 | 代表事故 | 根因 | 检测 |
|---|---|---|---|---|
| E1 | **失败收口缺失 → 静默死锁掩盖真 bug** | VM noaddr/AccessViolation/Err 出口不清 PAGEFAULT → RS 永停静默死锁；补 SIGSEGV 收口后暴露底层 null-deref（「这是暴露，不是回归」，NK4-A Task A）。同族：RTS_SET 出队半缺失（NK2 vmctl_clear）、阻塞后不出队 spin-pick 86,444 次（NK1 缺陷3）、唤醒入队半缺失（NK1 缺陷5）、重挂起 spin-pick 10.6 万次（迭代14）——**「置旗不出队/入队」同族病前后 5 例** | C 宏（RTS_SET/RTS_UNSET）的半边语义在 Rust 里没有单一对应物 | →P2 清单（noaddr 收口测试）+§5（状态迁移必须成对审查 enqueue/dequeue 半） |
| E2 | **上下文撕裂 / 寄存器多身份** | IRQ/tick 臂不存全量帧 → 陈旧 ctx 恢复（C mpx.S 每入口 SAVE_PROCESS_CTX 不变量，迭代20）；frame 与 ctx 分管不同寄存器=任何只更新其一的路径交付撕裂寄存器文件（迭代21 副产品）；RBX 三重身份（ps_strings 出生值/IPC 状态寄存器/callee-saved 活值，NK4-A Task C 全史） | C 的 p_reg 单一快照不变量在 Rust frame/ctx 分家后失守 | →P2 清单（IRQ 镜像测试）+§5（撕裂风险在 SMP 收口轮审视清单） |
| E3 | **移植语义面：i386 caller-saved → x86-64 callee-saved** | C 只有 i386（bx）/earm（r1）定义 IPC_STATUS_REG；x86_64 选 RBX（callee-saved）使「内核写状态寄存器」从无害变摧毁用户活值（NK4B P1 三案上交） | 寄存器模型差异未提升为显式架构裁决 | →§5（架构级裁决上交，不自行定案） |
| E4 | **限次探针上限被高频事件耗尽** | pf-save cap 8/48 两次全部消耗在启动前段同一 refault 事件、崩溃现场未捕获——「连续第二次犯同一错误」；对策=按目标进程过滤+按 (rip,rbx) 去重+上限按预期重复次数设（NK4-A 第五/六轮自认缺陷） | 上限按串口可读性而非事件频度设 | →§5（探针纪律，NK4A-TODO §5 已固化；P12 兜架构门半边） |
| E5 | **穷举不全仍称穷尽** | 第 6 轮布防 5 类写点实际只布 3 站点，「五类写点零命中」结论不足以覆盖写者全集（NK4-A 第六轮补充自曝）；「全仓三处提到 SUM」实扫两目录（M4.4 更正） | 布防/断言先于全枚举 | →§5（穷举主张先出全枚举清单再布防；范围词=命令根） |
| E6 | **无定性不修复** | 铁律 #10：同一问题 3 轮无根因实证 → BLOCKED，不做无定性修复（NK4-A Task C 六轮、NK4B M4.4）；对照：迭代20 的 IRQ 存帧修是「独立成立的 C 偏差」即使证伪于本例也保留 | 防止用改动制造进展感 | →§5（已有任务书铁律，此处收录） |
| E7 | **阻塞前提过期** | SIGACT 件登记的阻塞理由（「裸 sigreturn 桩依赖 S3 投递臂」）被三事推翻（「跨轮状态陈旧 CTOS」，review-patterns 在案）；M4.1 前置件清单探错包名、M4.3 当场更正 | 领取时不核实登记前提 | →§5（动手前核实前提） |
| E8 | **服务端能力在、客户端缺位 = 死能力** | MIB_SYSCTL 服务端全真、客户端缺位 → 服务端全是无人问津的死能力、init 三件 ENOSYS（NS11-A）；RS_INIT 六服务器无一应答（NS1） | 只交付单侧半 | →§5（能力完整性两端对账） |

### F 族：冒烟契约 / 对外产物 / 环境陷阱（6 条）

| # | 模式 | 代表事故 | 根因 | 检测 |
|---|---|---|---|---|
| F1 | **冒烟契约 marker 无 emit 点** | T1 契约串 'entering scheduler' 全仓无 emit 点，smoke stage-3 永远过不去（edge1 迭代7 追记，NK4-A C-3 排查发现） | 判据写进脚本但无人负责 emit | →P5 |
| F2 | **对外产物字节漂移** | startup.nsh 的 `cd EFI\BOOT` 被顺手改成带尾反斜杠（行为等价但越线：任务书要求一字不动）——「宿主测试没有任何一条断言过 startup.nsh 的字节，漂移无人守」（M3.3 补记三）；对策=钉**字节**而非「能不能启动」+ 判别变异 | 冻结基线无字节门 | →P2 清单（startup_nsh 字节门测试） |
| F3 | **CRLF 入库** | aarch64.ld CRLF 入库（链接器容忍→全绿肉眼看不出，M3.2 评审复核抓到）；**M4.2 新写的 riscv64.ld 同病没人再查**（本次 P8 首跑实测在案）+ 4 个 .rs 文件 | 工具生成/跨平台编辑无 eol 门 | →P8（基线 5 处，新增即拦） |
| F4 | **会话产物误入库** | f1041b3f1 落盘 543 文件 +35.7 万行（tmp/.zcode/.trae 等），Fix #9 修 2 分流 + .gitignore 加防复发规则 | 无 ignore 防线 | →P9 |
| F5 | **工具缺位伪装成被测物错误** | 宿主 nm 不认 aarch64 ELF → 三条布局断言集体假 FAIL，读者会误判「镜像布局错了」（M3.2 CodeReview P2#1）；对策=先探测工具、失败原因落在工具上 | 判据未区分「工具失败」与「对象失败」 | →§5（check-layout.sh 已修，收录为先例） |
| F6 | **环境陷阱**（各有判例，处置法见 §5）：docker 不带 `-u` 写脏 target root 属主（假失败，M4.2 注记）；裸机工件是 deps/ 硬链接不能用时间戳判断是否重产（M3.3 假证据自纠）；`cargo clean -p` 不删目标架构工件 → 「无工件」现场造假失败（同上）；pkill 与 qemu 同命令行自匹配（M4.3 + NK4A-TODO §5）；grep BRE `\+` 是量词（M4.4 方法伪影第5次）；`head -6` 截断证据（NK4B P0 T0.2）；`grep -c PASS` 被自己注释里的字串误命中（M4.3 教训连踩两次）；SUM 子串误命中 RESUME（M4.4） | 证据链工具的隐性语义 | →§5（证据命令锚定输出格式） |

---

## §3 与既有 85 模式（prompt/review-rules/review-patterns.md）的对照

本次**新发现**（85 模式未覆盖，未登记进规则集——按用户裁决本次仅目录化）：

| 新模式 | 最接近的既有模式 | 差异点 |
|---|---|---|
| B1/B5 canned 双方各自为政恒绿 | 「依赖全局状态」（40）| 40 讲 flaky，本模式讲 **假绿**：替身无语义校验使两侧缺陷互相不可见 |
| D5 退出码被吞 | 「外部调用返回值被无说明忽略」（32） | 32 讲代码级，本模式讲**脚本/CI 层**失败改判 skip |
| D6 check 不查 inline asm / 静默跳过当通过 | 「测试未覆盖设计决策」（12） | 本模式是**验证工具能力边界**（codegen 期校验、工件存在性判据） |
| F1 契约 marker 无 emit 点 | 「跨文档阶段状态表漂移」（58） | 本模式是**契约↔实现双向**漂移，58 是单向文档漂移 |
| A2 FIXLOG 序号撞号 | 无 | 并发共享 gitignored 账本特有 |
| C4 夹具断链阻断多 crate | 无 | trait 演进的夹具半 |
| E1 置旗不出队/入队半 | 「资源获取后无释放路径」（33） | 33 讲资源，本模式讲**状态机迁移的配对半** |
| E4 探针上限被高频事件耗尽 | 无 | 取证方法论 |
| E5 穷举不全仍称穷尽 | 「因果链编造」（48）邻接 | 48 是编造，本模式是**以抽样冒充全集** |
| F6 族证据命令伪影 | 「无锚点知识点断言」（83）邻接 | 83 讲断言无锚点，本模式是**锚点命令本身的语义陷阱**（BRE/子串/截断） |

已被既有模式覆盖、本次仅补实例的：「裸整数表达语义 / C 式空指针哨兵 / 裸 as 截断」（16/17/20→B4）、
「注释理由虚假或牵强」（34→A4 邻接）、「因果链编造」（48→假归因「nt-tests 并发合入」实例）、
「参考代码路径漂移」（65/66→A5）、「跨轮状态陈旧」（70→A3/E7）、「代码注释行号漂移」（77→A5）、
「C 源码 bug 未显式标注」（78→B3 邻接）、「target-specific cfg 泄漏」（82→P12 同域；P12 落在 CI/build 门
这一 82 未覆盖的 enforcement 半边）、「无锚点知识点断言」（83→A5）。

---

## §4 机械检查映射（tools/pattern-gate.sh）

| 检查 | 级别 | 对应模式 | 事故出处（条目定位） | 实现要点 |
|---|---|---|---|---|
| P1 | gate | A1 | new_edge4 §1 规则 5；FIXLOG.md 头部归档注记 | 每份 edge*/FIXLOG.md 标题行=1 + 最大连续重复行≤5；.review 缺席 SKIP |
| P2 | gate | C6 | 本表下方 20 测试清单 | `fn <name>` 全 os/ grep ≥1 |
| P3 | gate | B1 | edge1 Fix #5（NK8）；C-59（edge4 §2 :74） | SELF/MIB_CALL_SYSCTL 派生断言 + `= -2/-3` 字面量零命中 + 0x1800 pin 在位 |
| P4 | gate | C4 | edge3 Fix #142（夹具断链）；os/tests [[test]] 纪律 | .rs ↔ [[test]] 双向 comm |
| P5 | gate | F1 | edge1 Fix #9 迭代7 追记（T1 marker） | 配对表：marker 在引用方脚本 + emit 范围可 grep |
| P6 | gate | D6 | OQ-1（5ba2644b8 注释 + edge1 迭代21 补记） | job 存在 + aarch64/riscv64 build 腿存在 + `cargo check -p minix-kernel` 零命中 |
| P7 | gate+基线 | D5 | M3.1（edge1 NK4-B P3 M3.1 补记）；run_all.sh 三处 | `(cargo\|bash\|sh) ... \|\| echo` 全量对账基线；本脚本自身豁免 |
| P8 | gate+基线 | F3 | NK4B M3.2 评审复核（aarch64.ld）；riscv64.ld 为本次首跑新发现 | `git ls-files --eol` i/crlf × {ld,sh,rs} |
| P9 | gate | F4 | edge1 Fix #9 修 2（落盘分流） | .gitignore 三条规则在位（.wt/、/tmp/*、!/tmp/nk4a/） |
| P10 | report + diff 门 | A2 | edge3 archive 索引撞号注记；Fix #126 教训 | tracked todo/new_todo 的 `Fix #N` 引用：存量 489 处只报告，--diff 拦新增 |
| P11 | diff 门 | A5 | NK4B M4.4 五轮评审（七组 C 对位缺陷） | 增量行 `.c:NNN` token：basename 在 minix3/ 可解析 + 行号落窗（多变体任一覆盖即过；基线可豁免引例） |
| P12 | report+基线 | D6/E4 | 9e115387e / ab79b40ba（M3.1 补记「为什么没人发现」） | 共享路径 4 文件中 asm!/rdmsr 行向上 6 行无 target_arch → 基线对账 |
| P13 | diff 门 | A4 | edge4 批A 附带发现；Fix #124① | 增量行反引号全大写带下划线 token 在 os/+minix3/ 存在性；PATTERN-SCAN-REPORT 自身豁免 |

**P2 清单（20 个防回归测试 ↔ 事故）**：`test_sys_times_self_sentinel_authority_pin`（NK8 SELF 化石值）、
`test_grantee_gate_any_authority_value`（NS2 ANY 哨兵）、`irq_entry_mirrors_user_frame_but_skips_kernel_origin`
（NK4-A 迭代20 IRQ 存帧）、`test_pagefault_unknown_region_sigsegv_and_clears_park`（NK4-A Task A noaddr 收口）、
`test_vmctl_clear_page_fault_requeues_target`（NK2 RTS_UNSET 出队半）、`test_vm_mmap_stack_lanes_carry_above_4gib`
（NS5-A u32 截断）、`test_flags_for_priv_proc`（C-28 PRIV_PROC）、
`startup_nsh_bytes_are_the_frozen_template_per_loader_name`（M3.3 startup.nsh 字节门）、
`boot_module_order_matches_boot_shim_module_names`（NS8 装机三重锁）、
`test_step0_creates_rproctab_grant_over_wire_mirror`（NS2 rproctab 授权）、
`test_cdev_ioctl_continuation_decodes_payload`（NS7-A 恒假门）、`test_lookup_label_hosted_is_none`（NL4 driver-rt）、
`test_feed_keyboard_byte_uses_full_table`（NL4 死缝）、`test_sigsuspend_carries_mask_only`（SIGACT 删死参）、
`test_sigaction_carries_sigreturn_stub`（NS11 桩地址）、`test_rs_init_birth_answered_with_ok`（NS1 出生应答）、
`load_process_elf_two_images_same_va_no_collision`（NK1 per-process 根）、`trampoline_address_is_nonzero`（NL3）、
`test_dispatch_times_self_replacement`（NK8 判别补强）、`test_check_gic_madt_decision_table`（C-38 决策表）。

**用法与语义**：

```bash
bash tools/pattern-gate.sh                    # 全量（P7/P8/P12 基线对账；基线外 FAIL）
bash tools/pattern-gate.sh --diff [RANGE]     # 附加增量门（P10/P11/P13 只看新增行）
bash tools/pattern-gate.sh --update-baseline  # 冻结当前存量进 baseline（人工复核后）
bash tools/pattern-gate.sh --self-test        # 18 例正反判别矩阵（mktemp 夹具，跑完即删）
```

退出码 0/1/2 照 unsafe-audit 惯例。基线 key=`检查名|路径|cksum(行内容)`——对行号漂移免疫、对内容改动敏感
（改了被豁免的行 = 当新违规重报，符合「动它就要处理它」）。判别证据：
`tmp/evidence/20260923-c61-pattern-gate/{selftest,mutation-negative,full-run-worktree}.log`——
selftest 18/18 全绿；真树变异 6 组（P3/P4/P5/P6/P7 注入真实违规全部现形，P6 拆出「缺腿」「check 退化」双断言），
还原后复绿 PASS=9 FAIL=0。

**局限（如实登记）**：①P12 是 6 行窗启发式，抓「cfg 紧邻缺失」形态，cfg 在更外层块的情况看不见——结构性拦截
靠 P6 的 CI build 门（check 不查 asm，同理 grep 也查不出）；②P1 只在主树有意义（.review 不入 git），worktree/CI
下 SKIP；③P2 清单是点名式，新事故的防回归测试要人工追加进 `P2_TESTS`（追加动作本身写进 §5 纪律）；
④P4/P5/P6/P9 只查结构存在性，语义正确性仍靠既有 cargo/CI 门。

---

## §5 不可机械化的流程纪律（已有判例与出处，供 review 对照）

1. **共享主树 git 三条**（D1/D2）：commit 一律 `--only <显式文件>`；共享分支禁 `--amend`；
   对账基线用 stash 或已提交态，禁 checkout 未提交树；update-ref/FF 后先 `git status` 核 staged。
   出处：archive :3222 两起 + STTY-G 双事故 + C-35 + Fix #141 过程事故。
2. **断言先跑再写，不凭记忆**（C1）；失败臂测试必须反证有牙（C2，edge4 批B「反证跑」判例）；
   过滤单跑必须绿（C3）；批量改码后必须 test-profile 编译（C4，Fix #129）。
3. **穷举先于布防/断言**（E5）：「全仓无一处 X」必须把 grep 完整路径参数抄进证据行，范围词=命令根；
   计数型断言写明由哪条命令哪个输出段得来；锚点要么锁输出格式要么锁字串（M4.3/M4.4 连续踩坑沉淀）。
4. **状态机迁移成对审查**（E1）：每置旗/清旗问 enqueue/dequeue 配对半；每加 wire 车道问 client/server 双侧
   对偶测试（B2/B5）；跨号域边界（errno 正负）pin 两相（B3）。
5. **验证工具能力边界**（D6/C5）：check 不查 inline asm——跨架构门必须 build；required-features 静默跳过——
   判据看工件不看退出码；guest 构建走消费方真实 feature 面。
6. **探针纪律**（E4，NK4A-TODO §5 已固化）：限次+cap 按目标事件预期频度设、按目标进程过滤、去重、
   `#[cfg(not(feature="mock"))]` + 架构门、「task1-close 裁决删除」注记。
7. **能力完整性两端对账**（E8）+ **登记前提动手前核实**（E7）+ **3 轮无定性转 BLOCKED**（E6）。
8. **证据命令防伪影**（F6）：`grep -F` 优先于 BRE 转义；`head` 截断不得当证据；存在性用 `find` 不用内容
   grep；锚点自查表对自写注释复跑一遍。

## §6 后续可选（本次不做，用户已裁决）

- CI 接线（新增 workflow 跑本脚本纯文本检查）——用户裁决暂不接。
- 新模式登记进 `prompt/review-rules/review-patterns.md`（B5/D5/D6/F1 等 10 条新发现）——需按 prompt/README
  同步三端派生文件并跑 check-review-rules.sh，另开任务。
- run_all.sh 三处吞退出码（P7 基线）在 P6 清理后出基线转真拦。
- P8 基线里 riscv64.ld 与 4 个 .rs 的 CRLF 建议顺手转 LF 后出基线。

---

## §7 增量扫描（C-62；截断面 3643c9331→a1b8b1663，NK4-C 274 笔）

> 承接 §1-§6 的首轮（C-61，截断面 3643c9331）。本轮为**增量轮**：语料边界=git log 3643c9331..a1b8b1663
>（274 笔，主体 NK4-C）+ 下表新增/增长文件。方法同 §1：全文/定向精读 → 对照 §2 六族 34 模式判新旧 →
> 可机械化者落 gate。**结论先行：无全新失败家族，新增模式全部可归并进既有六族（多数是高频家族的第 3-5 次
> 独立复发——复发频次本身印证了目录的价值）；机械面新增 2 个检查（P14/P15）、P2 清单 +7、P12 启发式修正。**

### 7.1 增量语料

| 语料 | 形态 | 说明 |
|---|---|---|
| `NK4C-WORKLOG.md` | **新**，5237 行 | 顶部「当前状态」摘要链（§1.89-§1.119）+ S0-S3 精读；164 节正文按 grep 定点取用 |
| `NK4C-REVIEW-REPORT-20260927-R3.md` | **新**，196 行 | 全文精读。214 笔机械对账（213 过/1 卫生违规/2 正则误报澄清）+ 交接方自审 4 项实锤 |
| `NK4C-REVIEW-REPORT-20260923{,-R2}.md` | **新** | 前两轮评审（R2 覆盖至 e90efbcd8） |
| `NK4C-RESUME-PROMPT.md` | **新**，361 行 | 全文精读。§4.2 探针纪律 + §9 已知陷阱清单（10 条）——纪律固化文档本身 |
| `NK4C-OPENING-PROMPT.md` | **新**，366 行 | 铁律与停止条件（与 NK4A/B 同构，增量读） |
| `.review/zcode/edge1/FIXLOG.md` | 2219→2640 行 | 迭代22-25 补记（NK4-C 阶段 0-1.2：A 案 R10 + Task C 第 10-19 轮取证） |
| `01-stage-kernel/firmware-heap-corruption.md` | **新** | 固件堆腐化教学文档（跨页表 `&'static` 失效 → landing pad，fix27c 同病第 3 发的固化） |
| `NK4A-HANDOFF-STATUS.md` | **新**，435 行 | 移交状态（佐证用） |
| `edge_todo.md` / `new_edge4.md` | 增量 | C-6x 登记行更新 |

### 7.2 新增模式（全部归并进 §2 既有族；标注第 N 次独立复发）

**B 族（wire/常量/协议）——复发密集区：**

| 归并 | 新实例（NK4-C 条目） | 说明 |
|---|---|---|
| B1（第 3 发）| §1.105 `CdevRequest::Open as i32`＝索引 0 ≠ 消息号 0x400；同族 `Select as i32` | 与 NS7-A（Ioctl 0x404）、edge3 #97「驱动消息位次是猜的」同一模式三次独立发生——fieldless enum 判别值冒充线路类型号。**机械面**：P2 清单新增 `test_open_request_wire_type_is_cdev_open_not_index` |
| B2（回复载荷丢失）| §1.106 stat 回复腿 `Ok(()) => zero()` 纯状态回复、从不 copy_out 到用户缓冲 → `ls` 判目录失败；同族 StatVfs 同病登记 TODO | 「回复＝状态码」而忘了载荷半。配套发现死代码 `write_to` 88B 伪布局（全仓无调用者）。**机械面**：P2 + `test_stat_streams_struct_stat_through_copy_out` |
| B7（repr(C)/布局）| 1.12d `SLOT=80` 与同文件 size 断言 `16+size_of=96` 自相矛盾（越界 UB）+ 方向互换两臂与 doc 契约相反（R3 F-SELF-1）；§1.115 BootHandoff 嵌套类型 repr(Rust)→4 类型补 `#[repr(C)]` | R3 判词：**「const 断言必须作为常量来源而非事后证明」**——断言写对了但代码不消费它。**机械面**：offset_of!/size 断言守卫测试已是仓内范式（P2 既有清单覆盖形态） |
| B3（载荷语义）| §1.117 `while waitpid(-1).is_ok()` 误译 C `while waitpid(...)>0`——`Ok(0)`（有活子无退出）永真 → INIT 忙等 6 万次/40s | **Ok 类型 ≠ 业务成功：成功语义在载荷不在 Result 类型**。修复主动清扫同族 RS SIGCHLD 排空环（一次修复两处同族）。**机械面**：P2 + `test_{entity,runetcrc}_fork_failure_reap_loop_exits_on_zero` |

**E 族（内核语义/根因）：**

| 归并 | 新实例 | 说明 |
|---|---|---|
| E 族·新形态「容量 ≠ 拓扑」| §1.118 SCHED `loads:[CpuLoad; MAX_CPUS=32]` 初始化满容量，`pick` 迭代不受 `processors_count` 约束 → 4 核机选中幽灵核 4..31 → EINVAL → INIT fork EPERM 活锁 | C `pick_cpu`（schedule.c:67）只循环 `processors_count`；数组满容量初始化 + 满容量遍历破坏契约。修复带双向判别回归测试。**机械面**：P2 + `test_phantom_seats_beyond_topology` |
| E 族·新形态「容量 vs 记账双口径」| §1.111 `total_pages` 旧值=记账累加（对位 C mem_init），但位图按绝对页号寻址——aarch64 RAM 基址 0x40000000 页号自 262144 起 → 容量 2800 位图全面越界；修复后又暴露 vsi_total 记账口径混淆（抬高 1GiB）→ 拆双字段 | **位图容量（最高绝对页号）与可用页记账是两个量**，C 里本就分开。**机械面**：P2 + `test_boot_params_validate_total_pages_mismatch` |
| E1（置旗/入队配对半·持续复发）| F10d 家族三种变体（privctl 绕过 RTS 宏 → runnable=yes queued=no；`clear_ipc_refs` 裸 clear；`do_trace` 裸 set/clear）——RESUME-PROMPT §9.8 已固化为陷阱条款 | **机械面（本轮新增 P14）**：grep `p_rts_flags.(set|clear|insert|remove)` 非 proc.rs 定义域的调用点，基线对账（存量 219 处冻结，新增即抓人审）。启发式无法判「是否补了入队出队半」（语义判断），门的价值是强制改 RTS 语义者过审视线 |
| E2/E3（上下文/移植语义）| §1.112/113：aarch64 无 TSS.sp0 硬件重装，阻塞腿不弹帧 → SP_EL1 每轮空闲 receive 泄漏 288B 下行踩 .bss（x86 免疫的跨架构不对称）；§1.109 `msr SP_EL1` 在 SPSel=1 UNDEFINED；§1.115 `tlbi alle1is` 刷 stage-2 在 EL1-only 体系 UNDEFINED；§1.110 APTable 极性与 x86 USER 位相反（=1 是禁止）；§1.111 `write_pte_dm` 无条件发射 tlbi 而 x86 同名函数早有 KernelDm 门控（跨架构同形函数的隐式前提） | CodeReview Critical-1 抓住栈纪律；「把静默内存踩踏换成显式 panic」是本轮反复出现的正面范式（fail-fast 优先于静默） |
| E 族·新形态「additive resume」| §1.115 kernel-image 调 `arch_boot` 把**已装载进 TTBR 的 L0 清零**摧毁自身高半映射；正解=`arch_boot_resume_high_half` 纯 additive + 各镜像独立 .bss 副本 | **重初始化路径不得清正激活的状态；跨镜像交接数据必须落本镜像自己的存储** |
| E4（取证仪器）| §1.99→§1.100 committed 探针每秒 ~460 行刷屏**掩盖了真死锁**（探针 DoS = observer effect）；§1.118 aarch64 userland `console_write` 根本不落 serial（唯 diagctl 可见）→「先前所有靠 console 串推的退出形态皆不可信」；diagctl 在活锁洪流下非确定性丢弃（同码一次 14 行一次 0 行）→ 正解=IPC reply m_type 回传码分区 | 新形态：**取证通道本身要先验证**（E4 原形态是上限耗尽，本轮是通道可达性/可靠性）；「洪流下串口不可靠，用带内通道回传判定值」已固化为 NK4-C 方法论 |

**C 族（测试有效性）：**

| 归并 | 新实例 | 说明 |
|---|---|---|
| C2（修复侧新形态）| 1.11e：taskcall ELOCKED 重试**两版从未实际生效**——重试读 reply 语义（rv==208）而内核走 syscall 错误臂 | 「修复是否生效」要验证失败路径真的走到修复代码；与「碰巧通过的测试」同根：验证点与执行路径错位 |
| C2（架构级判词）| R3 F-SELF-2：drain 代停车在偏离 C 的架构内把补丁打对（真机验证通过），后被 B24 按 C proc.c:569-583 结构性取代 | **「验证通过 ≠ 架构正确」**——局部正确性无法救赎架构偏差；应更早对照 C 质疑架构本身 |
| C1（取证判别力）| R3 F-SELF-1①：探针链只验证了 range check 未验证数据内容（「探针验证了检查通过却没验证读到的字节」）——方向互换漏诊 | 探针断言的判别力要与缺陷形态匹配（守卫缺陷验守卫、载荷缺陷验载荷） |

**D/F 族（流程/环境）：**

| 归并 | 新实例 | 说明 |
|---|---|---|
| D1（git add -A）| d6f176451 `git add -A os/` 误提交 `os/.dockercargo/registry/` 158 文件 4.7MB vendored 依赖源码且 .gitignore 无条目（R3 唯一卫生违规，登记待清） | **机械面（本轮新增 P15）**：vendored 目录跟踪数=0 + gitignore 条目在位；本单已执行 R3 待办 1（git rm -r --cached + gitignore），门从出生即真拦 |
| F3（CRLF 第 4 发）| `os/libs/minix-boot/src/handoff.rs`（§1.115 新文件）CRLF 入库——本轮 P8 首跑即抓 | 与 aarch64.ld→riscv64.ld→4×.rs 同族持续蔓延；进基线（NK4-C 活跃文件不触碰），建议 NK4-C 收线时统一转 LF |
| D6/P12（架构门第 3 发）| §1.107 `boot_init_timer` 无条件调用 x86 专属 `pic_init` 等（aarch64 E0425）——同族第三次；本轮 P12 对 HEAD 实跑新抓 6 处形态命中，人工复核=2 处启发式误报（doc 注释里的 `naked_asm!`、cfg 门在 >6 行窗外的 GS 采样探针）+ 其余为 committed 取证基础设施 | **机械面（本轮修正 P12 启发式）**：排除纯注释行 + 窗口 6→12 行；修正后存量 3 处进基线（boot-shim asm、方案 A 跳板、cr2 探针）。结构性拦截仍靠 P6 的 CI build 门 |
| F6（证据命令伪影）| R3 P6：日志含 NUL 字节时 grep 计数不可靠（以 python 字节级统计为准）；R3 对账脚本正则误报 2 笔（未豁免 message 明写「含代码」的 commit） | F6 族持续积累：证据工具的隐性语义 |

**正面范式沉淀（§5 补充）**：金丝雀/毒化实验（第 10-16 轮：canary 注入判「保存即错 vs 保存后被改」、毒化扩散追踪级联损伤、PA 别名检测）——强判别力取证实验的设计模板；哨兵分区法（`si res=902+22=924` 用哨兵值+错误码分区钉死错误来源层）；误诊诚实翻案文化（§1.116 翻 §1.115、§1.118续 翻 §1.118、§1.119 翻静态刻画，R3 确认「未发现虚构取证」）；QEMU 上游源码实读钉死 GICR 行为（R3 评语 exemplary）。

### 7.3 机械面变化汇总

| 变化 | 内容 |
|---|---|
| P12 修正 | 启发式排除纯注释行 + cfg 门窗口 6→12 行（本轮 6 处 FAIL 人工复核 2 处误报驱动）；修正后实抓 3 处 committed 取证基础设施进基线 |
| P14 新增 | RTS 裸 set/clear 对账（F10d 家族三变体驱动）；存量 219 处冻结——启发式不判语义合法性，门价值=新增强制过审；selftest 正反例 + 真树变异验证 |
| P15 新增 | vendored 目录防线（d6f176451 事故驱动）；ls-files 半 + gitignore 半；本单执行 R3 待办 1 后从出生即绿 |
| P2_TESTS +7 | phantom_seats（§1.118）/ open_request_wire_type（§1.105）/ stat_streams（§1.106）/ do_fork_downgrades（B48）/ total_pages_mismatch（§1.111）/ reap_loop×2（§1.117）——全部 grep 实证在位后入列，27/27 |
| baseline 重生成 | 222 keys（P7×3 + P8×6 + P12×3 + P14×219 + ……）；P8 新增 handoff.rs |
| .dockercargo 修复 | gitignore 条目 + `git rm -r --cached`（158 文件解除跟踪，盘面保留，docker 构建不受影响）——执行 NK4C R3 报告 §7.3 待办 1 |

### 7.4 判别证据（tmp/evidence/20260927-c62-pattern-gate/）

selftest 24 例（+P14×2/P15×2）正反矩阵全绿；真树变异：P14 新增裸 set 即 FAIL、P15 gitignore 删条目即 FAIL、还原复绿；full 模式 PASS=11 FAIL=0 SKIP=1（worktree）/ 主树合并后终验。**过程事故如实记**：变异还原用 `git checkout -- .gitignore` 把自己未提交的 gitignore 条目一并冲掉（P15 假 FAIL 一次）——变异脚本的还原必须用「改前备份/改后还原」，对未提交在制禁 checkout（D2 模式第 N 次现世，本次受害者是检查者自己）。

---

## §8 补扫轮（C-63；用户点名缺口的闭合）

> 用户核对语料清单后指出缺口并提醒 `.zcode` 下还有日志。本节记录补扫范围、新增模式与机械化判定。
> **结论先行：三路补扫（160+ 条提取）无全新失败家族；新增模式全部归并既有族，其中约 10 条为高价值
> 新形态；机械化面 P2_TESTS +1（本轮素材以语义级为主，强机械化候选已在前两轮穷尽）。**

### 8.1 补扫语料（闭合用户点名的缺口）

| 语料 | 规模 | 提取方式 | 产出 |
|---|---|---|---|
| `.review/{claude,codex,trae}` | **1689 个 md**（297/1247/145） | 三目录定向提取（教训密度文件优先、每目录 ≤15 文件） | 52 条 |
| `.zcode/plans/` | 35 个会话计划文件（304K） | 全量提取 | 60+ 条 |
| `NK4C-WORKLOG.md` 正文 164 节 | 5237 行（此前只读状态链） | 关键词定向深抽 | 53 条 |
| `NK4C-REVIEW-REPORT-20260923{,-R2}.md` | 105+91 行 | 补读全文 | 正面范式为主（S0-S3 教科书级证据链、自我证伪修正、独立实测复跑）+ kdst 门事故 |
| `NK4C-OPENING-PROMPT.md` / `NK4A-QWEN-OPENING-PROMPT.md` | 366/127 行 | 补读 | 与 NK4A/B 同构确认（铁律无新增） |
| `NK4A-HANDOFF-STATUS.md` | 435 行 | 抽读 | 佐证（无新形态） |
| `~/.zcode/cli` | **2.1G 宿主全局 CLI 数据** | **划界不扫** | 见下方理由 |

**~/.zcode/cli 划界理由**：它是宿主级 ZCode CLI 的跨项目会话数据（含全部项目的 transcripts、插件缓存），
不属于仓库语料——跨项目噪声远高于本项目信号，且含用户其他项目的隐私内容；`.zcode/plans/`（项目内、
已被 .gitignore 管理的会话计划产物）已全量扫。若将来要扫宿主会话记录，正确口径是按本项目会话 ID 过滤
后单独立项，不混入本目录。

### 8.2 新增模式（归并既有族；精选高价值 20 条，全量提取存于三路 agent 记录）

**B 族补强：**
- 「车道契约必须每层同步」：F10b kernel 侧 `reply_wire()` 修了，PM 侧 `Reply(e.to_errno())` 仍回正值
  → 父拿 Ok(正 errno) 当 child_pid（F10 全仓 ~37 站点穷举，ReplyIntent 拆臂登记）——B3 的架构级修法判例。
- 「信号位约定分裂」：producer `1u64 << sig` vs 消费点 12 处 `1<<(signo-1)`（04-pm plan 提取）——
  B3/B4 家族；SIGKMEM=71 超 64 位 SigSet 位宽同族（edge1 迭代8 已录）。
- 「类型推导步长不能抄成常量」：C 32 位布局凑出的槽距 80 被抄成常量，真实 `WireAsyncSlot`=96
  （§1.16）；且「修一个正确偏差可能揭出真根因」（旧栈单槽恰好躲过）。
- 「每个『C 同款常量』要查其成立条件」：LP64 下 phdr 界用 `SECTOR_SIZE` 是 32 位 phdr 时代假设
  （§1.65）；C 里被 `#if 0` 停用的检查才是设计本意。
- 「寄存器车道迁移必须穷举全部读写点」：RBX→R10 迁移后唯一漏改点 `sync_status_register_to_frame`
  仍写 RBX，旧守卫单测 RBX→RBX 是恒等空操作测不出（§1.40）——「守卫断言自身必须判别力验证」。

**E 族补强：**
- 「多唤醒互覆单槽」：`wake_target: Option` 被同一次 syscall 第二次 record 覆盖 → 被覆写者 rts 已清
  却永不入队（1.12a）——E 族新形态（事件记录槽容量）。
- 「异步发送槽不能是栈上局部变量」：内核不拷贝槽内容，延迟投递时读已弹栈=UB；对位 C static
  msgtable 用持久 `AsyncSendQueue`（1.13/1.14）。
- 「多页虚拟传输必须逐页 walk」：`cross_space_memset/copy` 对进程虚拟多页目标只解首帧线性写满
  count，DM 窗口线性映射使 present 守卫对越界写零拦截（§1.70）——单页/物理连续时潜伏不发作；
  B39 逐连续段 walk 重写收口。
- 「容量修复的布局副作用」：静态数组变大平移 .bss/栈基址，撞未 eager 物化初始栈缺口，纯常量修复
  变启动 panic（1.59/1.60）——「剃刀边缘」形态；死路结论写进常量文档块 + const assert 锁不变量。
- 「sendrec 原子性三连」：停车 getfrom 必钉目的地（ANY 让第三方消息冒充 reply）/ REPLY_PEND 必须
  send 前无条件置（只置 Blocked 腿 → Delivered/Blocked 随调度时序翻转 = layout 敏感 bug 机制根源）
  / 快慢路径钉同一语义（1.10z/1.119续-2）。
- 「对齐 C 行为优先于本地严格化」：shmdt 刷 atime 是 C shm.c:228-229 怪癖，注释钉死防「好心修错」；
  sweep 的 saturating_sub → wrapping_sub 对齐 C u8 回绕（13-stage-ipc plan 提取）；MAGIC 失配降级
  为「无 kerninfo 状态」继续运行（runtime STATE Fix #5 三态而非硬失败）。

**C 族补强：**
- 「测试失败先判期望再判实现」：三处测试失败全是期望算术错误、引擎全对；一次「失败」是真值判断
  错误（`(a*)*$` 本就匹配行尾空串）（codex 18-stage Rule Discovery）。
- 「孤儿 API 的测试 FAIL 是 API 自身缺陷」：map_lazy 写 PFN_NONE 槽但 get_slot 过滤 → unwrap
  panic，语义待定只能 backlog（codex vm STATE 的 map_lazy 孤儿 API 条目）——测试红不一定是被测物红。
- 「虚构测试固化反线格式」：`test_getpid_returns_reply_type` 断言 `reply_with_type(7)=Ok(7)`，与
  服务端真实线格式相反——host 全绿真机必挂（§1.18）；与同族兄弟客户端交叉核对是判别法。
- 「回归测必须查正是 bug 的漏网处」：1.99 回归测只断言 char-mode 没查设备号——正是 bug 漏网处；
  补 `assert_eq!(node.device, 0x400)`（改前必挂）才算锁死。
- 「空断言测试对分组写反照样过」→ 改显式 64 项 EXPECT 表让写反在 host 直接红（§1.116）——
  **P2_TESTS 本轮 +1（`classification_matches_the_expected_dfsc_table`）即此判例**。

**D 族补强：**
- 「修复动作制造漂移」：批量改行号前必须 grep 重放，否则「修复」即二次污染（claude STATE Round 30
  首次批量拦截：3 处初检误报全部被 grep 重放否决，避免了修复自身制造 3 处新漂移）——C 伪验证的
  镜像形态。
- 「mock 门形态不一致」：kdst 探针三调用点 `#[cfg(not(test))]` vs 定义 `not(feature="mock")` →
  宿主 mock 构建 E0425 必炸（R1 评审 + 200a016a8 修复）——C4 夹具家族新实例（cfg 门两套惯用法
  并存的仓内陷阱）。
- 「重写后强制回归轮」：rewrite 级修复自身引入 3 个新 P0（enum 值 15→19/8→13、签名 &Message→
  &mut Message），「一轮 CONVERGED」不成立（trae 25-misc）——rewrite 级判据。
- 「gate 只查存在性不查内容」：`.claude/.codex` 的 checks/code.md 被 review-core 旧副本顶替（16 个
  code checks 全缺失）而校验脚本只查存在性（trae review-rules-meta Fix #12，已补内容校验）——
  门禁自身的内容完整性校验判例。
- 「整包裸 cargo fmt 事故」第 2 发：基线严重未格式化 crate 一次重排 35 文件（1.119续）——F2 纪律
  的第 2 次独立违反，报告 §5.8 纪律引用双判例。

**F 族补强：**
- 「观测扰动翻转」：探针一装上当前 build 即把失败态翻成成功态（build-layout 敏感非确定性）——
  解法=零扰动取证（不加新探针，直接跑已内置探针的 committed 构建取早发签名）（1.119）。
- 「panic=0 可能是假阴性」：diagctl 静默丢弃 >16B 写，长 panic 消息根本不上串口（§1.19）——
  「无 panic」不能当无故障证据。
- 「跨轮对账必错」：用户 VA 跨轮稳定、物理地址每轮变——跨轮对账必须用同一轮日志（交接节）。
- 「release 无 debuginfo 下 Location::file() 返回空串」：诊断手段先在目标构建形态下验证可用性
  （1.10h）。
- 「签名一致性可能是增量构建陈旧 artifact 侥幸」：固化无 panic 基线必须干净 HEAD 全量重建
  （1.49）。
- 「签名解译先确认进制与字段宽」：OOM-RT 打印是十六进制，`big=20/20` 实为 32/32；打印宽度不足
  把 `0x400` 截成 `00`（1.59）。

### 8.3 机械化判定

- **P2_TESTS +1**：`classification_matches_the_expected_dfsc_table`（64 项 DFSC 期望表）——「空断言
  对分组写反照样过」的仓内判例，grep 实证在位。
- **无新 P 检查**：本轮 160+ 条素材中，结构可 grep 的形态（mock 门不一致、信号位偏移分裂、恒真
  断言）误报率均高于信息量——如 `not(test)` 与 `not(feature="mock")` 各有合法用途、`1<<(sig-1)`
  与 `1u64<<sig` 各自合法（SigSet vs 单信号）；强机械化候选（RTS 裸操作、vendored 目录、CRLF、
  退出码吞没）已在 C-61/C-62 落地。语义级新形态归 §5 流程纪律与上表，供 review 对照。
- gate 基线不变（P2 加名不改 P7/P8/P12/P14 对账面），无需 `--update-baseline`。

### 8.4 三路补扫的正面沉淀（§5 补充）

- 「三向核对」执行前判据（claude 三向核对候选规则）：处方被后续冻结决策取代→记账取代；规模超单轮→
  收敛子集+战役登记；锚点已消失→先修锚点。
- 「因果序纪律」：先审源（review-rules）再以源为基准审三派生，违反因果序结论无效（trae
  review-rules-meta STATE）。
- 「误报挂账为一等状态」：Open 误报与 P0/P1 并列追踪，不静默丢弃（trae STATE 头块）。
- 「避免并行线程写共享账本」：fresh 读尾部 + wc + 单次 append + 追加后复核（多路 plan 提取一致）。
- 「哨兵迭代方法论」：每代被前代阴性结果重新定向；「零输出」有两种解释（真没到 vs cap 饥饿），
  假阴性三次复发后固化「判别窗口需无条件打印 + 足够 cap + 按目标进程过滤」（S2/1.9b/1.9c）。
- 「日志形态学配对」：`pdmv-set` 紧接同址 `fx` 成对出现 → 区分正常完成与异常写，一次日志形态判据
  推翻整轮归因（S3）——R3 评语「教科书级证据链」的核心手法。

---

## §9 增量轮二（C-64；截断面 a1b8b1663→c9e6ada3d，NK4-C 续-60~77c）

> 承接 §7/§8。本轮语料结构的两大变化：**riscv-reviewlog.md 首次入扫**（C-61/62/63 语料清单均未含——
> 用户原话「REVIEWLOG」即此文件，语料缺口闭合）与**迁移交接件群 + R3 评审报告入扫**。
> 结论先行：无全新失败家族；新增形态集中在**取证方法学**（排除法双缺口/探针有效性三缺/阴性证据谱）、
> **吞错家族分类学**（riscv 侧线债⑫：D5/E1 之外的第三层）与**结构债分类学**（组合缝隙/第二事实源/
> 债随形态迁移）。机械化面：P2_TESTS +15（增量轮新增防回归测试全量入列）+ **P16 新检查**
> （trap 腿消息物化防线）。

### 9.1 增量语料（实测边界）

| 语料 | 规模 | 说明 |
|---|---|---|
| git 提交消息 a1b8b1663..c9e6ada3d | 117 笔 | NK4-C 续-60~77c 为主 + 迁移/交接 docs 提交 |
| NK4C-WORKLOG.md delta | +3225/-3 行 | §1.119续~§1.120续-59+（含续-60~77c 全部章节） |
| riscv-reviewlog.md | 1579 行（全文） | **首轮全扫**；riscv64 侧线评审日志（§D 结构债 D.0-D.14、债①-⑬、吞错家族、§E 预裁决章） |
| 迁移期新文档 11 件 | 约 200KB | NK4C R3 评审报告（C-63 只扫 R1/R2，R3 首扫）、R3-P1-AUDIT-TABLE、REVIEWER-SESSION-RECORD、MIGRATION、MISC-CONCEPTS-SESSION-HANDOFF、VEC-CAP 双档案（untracked）、EXEC-REBIND-LIVELOCK、trap-boundary-message-materialization、RESUME-PROMPT、GLM53-PROMPT |
| misc_concepts.md delta | +659 行 | 知识点增量（负结论/方法坑密度高） |
| 实测零增量 | — | FIXLOG 四份（末次改动早于 C-63 交付）、.zcode/plans（最新文件早于 C-63 交付）、.review STATE（停在九月中旬）——mtime 与内容双核实 |

**方法**：五路并行 agent 深读提取 146 条原始候选（WORKLOG delta 38 / reviewlog 35 / 评审文档 35 /
BUG 档案 25 / 提交消息独有 13）→ 逐字引文锚点抽验（8 组全通过，1 处行号偏 2 已实测校正）→ 对照
§2/§7/§8 目录判新。全量提取清单存于本轮 agent 记录与 evidence 目录。

### 9.2 新增模式（归并既有族；锚点为 WORKLOG 行号/文件+行号/提交号）

**E 族——取证方法学（本轮最重，全部出自 NK4-C 真机取证线）：**

1. **「排除法终点缺 Ground-Truth 核验」**（无定性不修 E6 的取证面镜像）：续-70 以「十二枚阴性排除一切备选
   ＝逻辑完备闭合」宣告「缺跨 CPU TLB shootdown」坐实即成修；续-72 查 C arch_do_vmctl.c 推翻——C 只做
   本地 invlpg/write_cr3、根本无跨 CPU shootdown 机制，实现它会偏离真值。判据：排除法收敛的「唯一剩余
   机制」必须先查 C 源确认该机制在真值中存在及形态；被排除的只是本 port 的机制清单，漏了「真值中不存在」一类。
2. **「探测域外候选不入枚举集」**：同案根因（halt 的 AP 持续收 LAPIC tick 重入内核）位于全部仪器化腿之外；
   候选枚举从未包含「未仪器化 CPU 的周期性活动」。全阴性轮收敛前必须先列探测域外清单：未仪器化 CPU、
   时钟/中断源、固件/DMA 活动。
3. **「探针比较域错配」**（续-61）：守卫 hits 表把传入 VA 窗口与表内 PA 记录直接相等比对——永不相等，
   零命中被当阴性证据（本轮诚实标注作废）。判据：探针比较两侧必须同域同单位。
4. **「探针活性自证缺失」**（探针上限耗尽 E4 第 3 次复发后升格为模板）：续-61 r1 在已持锁上下文重取自旋锁致
   零命中；续-16 镜像陈旧伪造零命中；续-24 恒假守卫使扫描腿整块未跑。判据：探针轮四查——runtime 他处确打
   过该标记、镜像构建晚于末次源改、nm 确认探针符号在二进制、入口哨兵先于判定逻辑。
5. **「仪器资源预算超安装上下文」**（续-63）：在 panic 嵌套 trap 栈做全量别名扫描，页表 walk 撑爆内核栈——
   「缺页 handler 内严禁页表 walk」第二次独立坐实；判词细化「非证伪，乃不可测」。
6. **「写守卫键单向性」**（续-65）：守卫键只监控 dst 在受保护对象的方向，污染实际走 src 在受保护对象的反向
   （VM text 被读出写进内核上下文）。判据：写守卫对每个受保护对象枚举 src/dst 双向设键。
7. **「判词强度与证据等级脱钩」机制级细化**（§8 判例续）：续-59 双阴性系单字段闸（只检 frame.rip），其阴性
   被升格为「排除寄存器上下文恢复整族」——实际 restore 有 frame 与 gp_regs 双输入源、污染可发生在探针点
   之后。判据：「排除一整族」必须附覆盖域枚举（时刻窗×输入源×地址空间），缺维降级为「该点该源阴性」。
8. **「A/B 对照基线未生效」**（续-77a 披露）：前轮 stash 对照因路径异常实际未回退，「非本改动引入」结论无据；
   改 checkout 基线 commit 重做。判据：对照镜像与实验镜像必须验证存在可观测差异（strings/标记 diff）。
9. **「巧合数据自洽叙事」**（misc 5.11）：长度寄存器恰等于目标指针 → 自洽推出「失控清零覆盖堆池」完整故事，
   一条更正即整体撤回。判据：假说登记强制证据等级（猜想/有佐证/已证伪）且叙事必须能被所引原始读数否证。
10. **「零读数过度推理」**（misc 2.7）：「故障时读数为零」被推成「曾被写后清除」，合法解释还有未写/观测不到。
    判据：零读数三解释枚举（未写/写后清/测不到）+ 回读型哨兵定夺。
11. **「判决工具污染」**（misc 2.8）：毒化读数越出第一窗口级联扩散，哨兵被当真实交付值污染用户态有实录。
    判据：毒化读数只采信第一窗口；判决后立即摘除（kill 开关）。

**E 族——内核语义：**

12. **「trap 边界编译器物化缺口」**（§1.120续-22）：用户态对消息头的最后一次标量 store 在 svc 指令前未落
    内存（alloca 优化物化缺口），内核 trap 腿读旧 m_type=0 → 10+ 轮误诊；修复=trap 腿
    commit_message_to_memory（整对象 read_volatile 强制物化，minix-sys）。**机械化：新检查 P16**。
13. **「stvec 必须恒描述即将运行的模式」**（续-75）：内核腿尾声不按 SPP 重选 stvec，S 态嵌套返回后下一 S 态
    中断把陷阱帧建到用户栈。
14. **「每 CPU 架构不变式依赖固件遗留态」**（misc 3.24）：BSP 由固件启用的特性位 AP 入口汇编不继承——同访问
    主核成功副核风暴；显式置位不依赖固件遗留。
15. **「锁原语语义矩阵缺态」**（misc 3.13）：中断入口 CAS 失败无法区分本 CPU 持锁（应继承）与他 CPU 持锁
    （须自旋）→ 双 CPU 撕裂，单核结构性测不出。
16. **「C 三态返回值二值化」**（misc 3.26）：waitpid 正/零/负三态译成「结果正常即继续」，零（有活子）落错侧
    → 收尸环无界忙等。与符号域 B3 同域但机制独立——谓词窗口缩窄。
17. **「位集换互斥枚举谓词缩窗」**（misc 3.27）：僵尸化整体替换「退出中」位 → 下游位测试谓词对正常退出进程
    返回假，父管理器恐慌。
18. **「校验次序后置」**（§1.120续-5）：零长检查排到页表解析之后 → 零字节请求带悬垂地址炸 FS 服务器；C 真值
    把 EDOM 检查放在一切地址工作之前（memory.c:608 为各调用腿共用 virtual_copy 的首检查）。校验顺序属
    可观测语义，重排即行为偏离。**机械化：P2_TESTS +3**（拷贝腿/dispatch 腿零长 + 校验次序测试）。

**B 族：**

19. **「无显式判别值枚举按整型发送得序数」**（misc 3.28；消息号=枚举序数 B1 第四发变体）：线路号=基址+序数；
    解码按「收到值减基址」查表落空 → 分派置空既不回复也不报错 → 发送方静默永久等待（失败收口缺失 E1 放大器）。
20. **「多架构车道迁移尾巴」**（回复车道偏移错位 B2/符号域 B3 复发，MIGRATION §4.B）：reply_wire x86/aarch64
    腿已迁、riscv 腿仍 reply_code；门控放宽先于后端就绪 → 错误号符号翻转上线。车道迁移按架构逐腿推进必有
    尾巴——对账表逐腿勾。

**D 族——流程：**

21. **「哨兵修复只修受害架构」**（reviewlog §A.7）：aarch64 已付学费的 sscratch 哨兵未回灌共享根因，riscv
    无条件 csrw 同型即炸。「已付学费当验收单」对账法：修受害侧时强制追问同型位点全集。
22. **「未核验报告经交接链固化升级」**（GLM53-PROMPT→活锁复盘证伪）：未复现的幽灵回归（单核四轮零复现）
    被写成「你要解决的核心问题」，接手者被指派解不存在的问题。判据：交接件的回归声称必须带复跑证据；
    未亲手复现只能标「待复核报告」。
23. **「隐式不变量成立域未声明」**（reviewlog §A.7）：sscratch 自愈依赖「每次陷阱原路返回」对称性，park 一
    落地即破——不变量的隐含适用域比实现面窄。
24. **「半修复解锁危险态」**（续-73）：三件套未齐时单补唤醒臂把「安全死锁态」翻转成腐蚀活跃态，判定净负回滚。
    判据：多件套修复整体评估，「净负即回滚」。
25. **「里程碑地形标签」**（复发 3 发：续-26/36/46）：marker 达成声明必须附启动器 argv 原文——非标准地形
    （手改单核）达成不等于标准启动器达成；子代理报告必须亲跑复现入判例。

**吞错家族（债⑫）专节——退出码被吞 D5/失败收口缺失 E1 之外的第三层：**

- **层次区分**：D5 在工具/进程边界（shell/自动化证据链吞 exit code）；E1 在系统行为层（失败路径缺收口致
  静默死锁）；债⑫在**消息层**——IPC reply/copy/grant 的真实错误码在服务器内部被湮灭，主要危害是可观测性
  （真实错误码不落任何诊断面→取证成本放大），次要危害才是活性（exit 丢父通知向 E1 漂移——吞错是 E1 的
  隐匿推手）。
- **四形态**：①签名级——fn copy_out(...) 返回 () 把 Result 抹成无事件（fs-rt transport，类型层就无法表达
  错误，结构上重于散点 let _ =）；②调用级 let _ =（31 站/13 文件三问分诊：乙诊断面下限 7、维持并注释 23、
  甲签名改造 1——「C 对位是否真检查」轴在案，一刀切清剿破坏 C 保真）；③字面兜底值——reply_to_guardian
  兜底 -1 线格式 C 忠实但真 errno 无处可去（EPERM 悬案放大器）；④正 errno 走成功车道（与符号域 B3 同族两面）。
- **判例测试**：test_sched_start_propagates_denied_reply_code（§1.119续-7；P2_TESTS 本轮收入）。

**riscv 侧线结构债分类学（reviewlog 独有，反哺收尾期）：**

26. **「完成件组合缝隙」**：全部完成件各自验收通过、风险全在组合缝（sscratch 组合缝/SUM 组合缝）；亚型
    「两笔正确改动合成一个洞」——VmDm 门控修掉无条件刷新，同时打破「每次 PTE 写自带刷新」的另一设计依据。
27. **「无操作兼容层静默化」**：riscv port_io 桩返 0/BadCall，TTY 协议流程全绿而字符永远出不了串口——
    协议绿不等于功能活；移植时对「每架构无操作」桩建清单逐个绑接线计划。
28. **「载体成立前提不等于生产前提」**：载体自带的 SUM 开窗被生产 restore_to_user 改写 stvec 顶掉；U-Boot
    载体入口平坦物理地址 vs 生产镜像高半 VA。载体与生产共走同一代码路径是验收项。
29. **「第二事实源」**：不链接不死亡的平行事实（旧三份 link.ld 文档锚点指着、注释版断言与实现两套）——
    删有阻力留必漂移；正解=宿主测试改读生产脚本，单源化由测试强制而非注释约定。
30. **「债随形态迁移」**：甲案落地把 boot-shim bump 池债原样迁入 kernel-image 侧 BUMP_PTR；扩容修复只是把
    确定性 panic 推迟——形态迁移/容量翻倍不等于债消除。
31. **「伪对位翻译」**：quiet_wait 阻塞 IPC 轮询冒充 C sigsuspend 睡眠等信号——「对位注释」在场而语义走样；
    对位要核语义非核存在。
32. **「承诺注释当实现」**：protection.rs「sstatus.SUM is set later」无任何置起点——承诺句式注释
    （later/will/TBD 非 TODO 标记）无追踪即假账面。
33. **「环境运气不变量」**：「新帧恰为全零」押注 boot 期无页缓存可回收，回收一上线即隐蔽数据污染——正解是
    显式请求标志而非押注分配器行为。
34. **「模拟器不建模故障类」**：dc cvau/ic iallu 缓存维护 QEMU 永不复现、实机必炸；16550 LSR 错读被 QEMU
    串口同步排空掩盖——禁止以 QEMU 绿当免疫证据，判别只能到「指令存在+调用点存在」级。
35. **「验证配方敏感性假断裂」**：裸 cargo check 默认特性引入 std 路径 372 错，与被测物无关（NK4B M4.4
    独立发现互证）——跨架构编译性检查必须固化生产配方。

**F 族：**

36. **「同工具异实现静默改判据」**（R3 方法注记+评审者会话记录双确认）：本机 grep 实为 ugrep，含 NUL 串口
    日志计数被吞且不报错——marker/panic 计数判据可静默失真。判据配方：判据命令统一 tr -d NUL 预处理
    （已固化进迁移交接件判据命令）。
37. **「对账脚本漏实现豁免子句」**（R3）：判据表明写「message 明写含代码则豁免」，脚本正则未实现 → 2 笔
    误报旗标。判据与实现逐条对表。
38. **「哈希长度不归一化」**（评审者会话记录）：rev-list 全哈希与 show 短哈希做集合比对 → 段归属失配。
39. **「多模式 grep 负结论出自截断输出」**（misc 5.19；reviewlog 债⑩翻案）：2960 行子串噪声 + head 截断 +
    截断处停止 → 「fence.i 零命中」假结论，外部复核翻案。负结论登记三元组：检索根+模式+全量计数。
40. **「量级异常盲区」**（EXEC-REBIND-LIVELOCK）：全环成功指标（exec 成功/ok=1/零 panic）下同址缺页重复
    74015 次——错误码/panic 类判据对此结构性失明。判据：冒烟加同址重复计数上限。
41. **「阴性证据失效谱」**（misc 2.9 合并四判例）：通道不在吐字/工具自测过但目标零命中/扫域够不到/截断埋真
    命中——「别把扫不到当不存在」四联判据。

**元教训（VEC-CAP 双档案对比——同 bug 双线取径）：**

42. **「归属未定时机制分歧是伪分歧」**：主线判「init 内某 Vec」、GLM 线判「归属未定」——共享库符号（两份
    ELF 都含）对账定不了 init/sh、exec 前后；先裁决崩溃归属（文本段 VA 区间对两份 ELF .text）再比机制。
43. **「交接件任务句的机制预设窄化搜索域」**：「找出哪个槽位落了栈指针」的预设把独立线钉在用户侧布局，而
    主候选根本不经过槽位。交接模板：机制预设显式标「待验前提」，独立线首动作检验预设本身。
44. **「证伪清单结论对论据错」**：E2BIG 早返论据不成立（checked_add 只拦回绕不设上界），真排除理由是
    try_reserve 优雅失败+服务端 ARG_MAX——错误论据会被后续轮当已证事实引用。证伪条目强制「结论级/论据级」
    分账。
45. **「正交失败模式共享症状域需入口分诊」**（MIGRATION §4.A）：aarch64 双失败模式（OOM-RT vs memreq 洪流）
    共享「boot 卡死」症状域，接手者先分诊再动手；已有专攻其一不修另一的分治先例。

### 9.3 机械化判定

| 变化 | 内容 |
|---|---|
| P2_TESTS +15 | 续-75 ×4（信号帧槽位映射/全量存帧持久/A1 状态车道/park 判别值）、§1.119续-3 notify REPLY_PEND 门、续-18 同根切换镜像同步、§1.119续-7 sched_start 拒绝码传播（吞错家族判例）、§1.120续-5 零长三腿+校验次序、续-51 boot DM 保留区三件、续-73 过渡守卫钳、续-76b PFEC 跨腿等值表——全部 grep 实证在位且出处提交钉准（43/43） |
| P16 新检查 | trap 腿消息物化防线（minix-sys/src/ipc.rs 内 commit_message_to_memory 定义+调用点在位；§1.120续-22 事故）。选型理由：该缺陷无宿主可测运行时行为（ABI/优化器层），符号存在性+调用点在位是唯一机械守卫；selftest 正反例 3 例（定义缺/调用缺/正例） |
| 无其他新 P 检查 | 本轮强候选（吞错签名级/同工具异实现/负结论三元组/量级异常计数）均为证据格式与判据配方类——误报率高于信息量（照 §8.3 判据），归 §5 纪律与判据模板；结构性拦截继续靠 CI build 门 P6 与 P12/P14 存量对账 |
| baseline 不变 | 增量区间漂移 triage 零新增存量：全量实跑 P7×3/P8×6/P12×3/P14 存量全部命中基线（NK4-C 续-60~77c 117 笔未产生基线外违规——裸 RTS、CRLF、退出码吞没纪律良好）；P10 增量新增 todo 裸引 Fix #N 仅 2 处（本报告 A2 目录行自身的事故举例，非新增待办）；代码文件（*.rs）新增行反引号全大写符号 20 个全仓存在性核查零虚构（文档行内的探针名引用如 NK63ENTER 属一次性取证伪影的文档化记载，不在代码面核查范围） |

### 9.4 复发确认（归并既有族，本轮再添实例）

- **置旗/入队配对半 E1 ×2**：续-72 idle 漏 stop_local_timer 半（移植 C proc.c:195 紧邻 if 块只抄半——
  本语料最大事故：十四轮 -smp2/-smp4 VM 用户栈腐蚀）+续-75 sscratch 重固定半。
- **穷举不全仍称穷尽 E5 ×4**：续-62 扫描窗手估偏窄制造假证伪/续-66 landmark 顺手集漏 proc-table 帧
  （翻案后以真实 landmark 重验）/续-69 换根不变量三腿缺 exec 腿/EXEC-REBIND 三腿穷举启发式沉淀。
- **探针/通道不可靠 E4 ×5**：越界读 Message 相邻字段（续-32，单条伪影推翻三轮定谳）/trap 入口 m_type=调用号
  双值车道（续-21）/探针 DoS 复发+per-nr 去重对策定型（续-77c）/gdb watchpoint 机制级不可行定性（续-69）/
  diagctl 16B 静默丢弃簇（R3）。
- **回复车道偏移错位 B2/符号域 B3**：riscv 腿 reply_wire 未迁+to_errno 返正值（吞错家族第 4 形态）。
- **空转/修复未生效 C2**：taskcall 重试两版从未生效（1.11e R3 重提+状态账误记生效——「空转归因」）。
- **账面失真/滞后 A3 ×2**：迁移交接件携带过期「未提交在途」叙述被 MIGRATION 专节破除+RESUME-PROMPT
  状态叙述过时误导接手者。
- **观测扰动翻转新亚型「探针存在即压制故障」**（续-42）：用户态探针构建下生产故障消失，此前「探针构建=
  生产复现」前提整体失效；零扰动取证定型。
- **panic 渲染器复合格式串截断**（续-77b）：A/B 两轮实锤，真机探针须纯整数列。
- **单侧半死能力 E8 ×2**：riscv park epilogue 无条件 sret（阻塞 IPC 结构性不可实现）/cpu_is_idle 只置不清。
- **移植语义面 E3 dormant 陷阱第二发**：riscv is_write_fault 位语义与其唯一消费者仅 x86 实例化，待接线时引爆。
- **夹具漂移 C4**：QEMU 回写 UEFI vars 文件——复跑前强制拷副本隔离。

### 9.5 正面沉淀（§5 补充）

- **反判矩阵**：判别性断言验收须含至少一格「喂错必红」（check-layout riscv64 行判例：喂 x86_64 工件=假
  PASS 面、paddr 整体偏移=当场红）。
- **反向验证**：新回归测试先在旧代码跑出红（ad9d37429 判例：两断言均失败后才算测试有判别力）。
- **真实 landmark 独立重验**：已关闭结论用完整 landmark 集重验后才真关闭（续-68 关闭跨池别名假设）。
- **净负即回滚**：多件套修复的 A/B 带「净负判据」（续-73 回滚 proc_table.rs 改动）。
- **零扰动取证定型**：零用户态探针纯净基线+复用 committed 探针的构建（续-42/6e78f233a 方法论突破）。
- **判词分级模板**：「非证伪乃不可测」（续-63）/「该点该源阴性」（降级表述）——防判词通胀。
- **交接件分节时效标注**：MIGRATION 对过期叙述逐条辟谣+标注「纪律与陷阱部分仍然有效」。
- **负结论登记三元组**：检索根+模式+全量计数（misc 5.19 纪律化）。

### 9.6 语料划界

- FIXLOG 四份/.zcode/plans/.review STATE 实测零增量（见 9.1）；~/.zcode/cli 维持 §8.1 划界不扫。
- tmp/nk4a 原始串口产物不扫：raw 证据非教训载体，其教训已被 WORKLOG 收编；将来要挖按会话 ID 过滤单独立项。
- VEC-CAP 双档案（untracked）本轮已扫；NK4-C 线 bug 闭环后决定入库或归档——两线交接件对其定性相反
  （MISC-HANDOFF 列为待扫源、MIGRATION 称可丢弃），本身即「交接件间状态不一致」实例（A3 家族）。

---

## §10 增量轮三（C-65；截断面 c9e6ada3d→484b5a12b，NK4-C 续-80~283）

> 承接 §9。本轮为四轮中规模最大：297 笔提交、NK4C-WORKLOG delta +3624 行、misc_concepts 两轮合并
> 增量 81→148 条（+67）、TRANSIENT-PTE 档案 795 行与结构债总册 1405 行两份长档首扫。
> 结论先行：无全新失败家族；新增形态高度聚集在**取证仪器治理**（β 竞态大案的十余种仪器失效形态）
> 与**判读算术化/账面治理**两簇；机械化面 P2_TESTS +21（brk/split 生产级修复族与 PL011/ATF/FPU
> 判别测试）+ **P12 启发式第三发修正**（包含函数名架构前缀=隐式门）。

### 10.1 增量语料（实测边界）

| 语料 | 规模 | 说明 |
|---|---|---|
| git 提交消息 c9e6ada3d..484b5a12b | 297 笔（实测 305 含 docs） | NK4-C 续-80~283 为主体 |
| NK4C-WORKLOG.md delta | +3624 行 | §续-77f~§续-283 |
| misc_concepts.md delta | +672 行 | **两轮合并**（增量三 +33、增量四 +34；81→148 条）——提交消息里的「114→148」仅指增量四 |
| NK4C-BUG-RISCV64-TRANSIENT-PTE.md | 795 行（首扫） | β 竞态结构化档案 |
| STRUCTURAL-DEBT-REGISTER-20260930.md | 1405 行（首扫，untracked） | 33 笔结构债总册 + §E 预裁决章 |
| NK4C-RETRO-AUDIT-20261001.md | 203 行（首扫，untracked） | 回溯审计 |
| R3.1 评审群 | 报告 185 + 对账表 151 | R31 报告首扫 |
| 接续 PROMPT 三份 + riscv 取证方法论 + glm 移交 | 121+145+185+164+82 | 首扫 |
| .zcode/plans | +2 文件 | 会话计划 |
| 实测零增量 | — | FIXLOG 四份（仍停 09-20~23）、.review 三工具 STATE、edge_todo；AI-chats daily.todo 尾部轻扫 |

**方法**：七路并行 agent 深读（WORKLOG 对半两路/misc_concepts/档案+方法论/债总册+回溯审计/评审群+PROMPT 群/提交消息去重路）提取 **236 条原始候选**；引文锚点抽验 10 组——7 组逐字吻合，3 组行号偏移（引文内容真实、行号未核对）已全部按 grep 实测重钉后才写入本节；对照 §2/§7/§8/§9 目录判新。

### 10.2 新增模式（归并既有族；按簇组织，锚点为实测行号/文件+行号）

**一、取证仪器治理（本轮最大簇，出自 β 竞态大案全史）：**

1. **「常量心算错造幻影链」**（续-142，WORKLOG 行 111）：崩溃寄存器里的 0x00fffffffffff000 被判
   「确定性寄存器破坏」，实为 query 正常算出的 PTE 掩码——期望值漏了 lui 的 64 位符号扩展，
   据此建起约 50 节的「GPR 恢复破坏」幻影链（续-139~190，行 82 一轮判死实验终局）。判据：
   反汇编常量语义求值禁 32 位心算；疑似破坏值先全树 grep 常量/掩码表再定性。
2. **「产物身份失真族」**（续-106 环境项，行 231/9440）：stash 回退源码再 build 会用无修复源码覆盖
   被测产物，SKIP_BUILD 复用即重现陈旧故障=假回归（险推翻已入库修复）；同族 cargo 增量不重编
   三方矛盾假信号。判据：SKIP_BUILD 前产物指纹双验（md5+反汇编特征）。
3. **「崩溃帧寄存器语义误判造错靶」**（行 11188 等）：a1 误当 self 指针、sp 误当 walk vaddr、
   同号异主张冠李戴——watchpoint/插件/扫描约十轮资源错投到不存在的物理靶。判据：设观察点前
   寄存器×反汇编逐位交叉验证（a1==stval 应然性检查）。
4. **「方法论死刑判在错前提上」**：「硬件 watchpoint 路线死路」的定谳出自错靶负结果+观察点语法
   被拒收未察觉（awatch 拒收→watchpoint 根本没设上→continue 直冲崩溃点）。判据：负结果定谳前
   武装确认行在场+靶变量受控。
5. **「共享错误字面量的多重负结果伪独立」**（TRANSIENT-PTE 行 474）：三套观察点脚本共用同一个
   少一位零的字面量（注释公式却是对的）——三重「独立」负结果实为同一前提三次复读，支撑了
   错误的「不可用」结论。判据：多条负结果引证前 grep 各装置是否共享同一常量。
6. **「跨构建证据拼合」**（TRANSIENT-PTE 行 320）：崩点指纹对与反汇编解读取自不同 build 被
   默认同源拼合，推出的收敛事后整体下调未证。判据：布局敏感证据强制携带 build/commit 标签。
7. **「检测器自检缺位」**：置位/查询次序倒置→全量误报（18,080 命中=全量 PT 分配数，纯噪声人工
   剔除）；环形窗容量被两进程交错缺页冲刷→同址 refault 漏判（定性随窗口容量翻转）。
8. **「事件观测仪对肇事访问本身失明」**：QEMU mem-cb 不为引发故障的那次访问触发——基于
   「时间线里没人写该槽」的推理对肇事者结构性失明（幸存者偏差）。
9. **「治疗性注入先估成本」**：门禁注入 -v 让 skip 掉的长跑用例真跑（2.2e12 次迭代/51 小时）——
   改判据前先读默认值并估算真跑成本。
10. **「修复 vs 扰动统计裁决公式化」**（续-283，行 12093）：探针构建崩率 2/11 vs 基线 7/10
    （二项 P<0.001）才定谳纯扰动——5 跑 1 崩时尚可辩；配套「扰动分形选择性」：崩形分布本身
    是扰动读数（β 消失而 BTreeMap panic 仍在=选择性压制最紧窗）。
11. **「单证据定性翻转链」**：(A) 全局定性被单轮单证据反复改写 8+ 次（覆盖洞→算术→帧双发→
    UAF→瞬态→错靶→双机制）——判据：定性变更门槛 ≥2 独立证据或统计显著性。
12. **「遗留探针滞留成主故障源」**：树上滞留的取证探针在 U 态发 SBI ecall 被内核当 IPC Send
    →SIGSEGV，引发 37 轮幻影追猎；排查任何新故障前先全量滚除/门控在树探针。

**二、账面与翻案传播：**

13. **「翻案传播三层脱钩」**（TRANSIENT-PTE 档案自审）：修正只写一次、不向既有副本传播——
    档案内叙述 vs 可执行命令块（同文档行 736/755：叙述已改路径、命令块仍带错路径，照抄即复发）、
    正文 vs 附录（未编译 vs 已实跑并存）、源档案 vs 沉淀层（方法论文档把已打问号的结论原样固化、
    无状态头无回链）三层全部中招。
14. **「交接置信度升格」**（glm 移交 行 80）：交接件把单证据链候选写成「唯一靶/定死」，下一会话
    按定死执行整套工单随即被推翻。判据：交接统一「当前最佳候选+置信级别」话术。
15. **「环境限制结论传抄」**：「本环境无法驱动 gdb、需真人真终端」实为 attach 时机错误+残留 qemu
    占单客户端口——受控复测一轮推翻，作为前提已在多轮交接传抄。判据：能力性阴性结论须附
    复测矩阵（attach 时机/端口清理）才可入死路清单。
16. **「候选地图免检传递」**：交接传递的不只是结论还有「下一步/候选地图」，其中含全树零命中的
    未验证断言——上一轮的行动指针必须与结论一样过真源审计。
17. **「滚净断言与 git 事实不符」**（RETRO 行 172-174）：message+WORKLOG 断言探针已滚 tracked 净，
    git show --stat 实为零代码改动、5 处探针存活 HEAD。判据：滚净断言附 --stat+blob grep 双证。
18. **「幽灵 open」**（R31 行 75/154）：缺口闭合事实写进 WORKLOG/message，登记台账自身不回写
    状态——台账恒说开放，误导下轮重复审计已闭合项。判据：销账同时落事实源与状态源两处。
19. **「撞号纪律文件自身撞号」**：misc_concepts §3.45 两条目同号（A2 家族在本文件的复发；
    建议该线下轮改号——非本线文件不代改）。

**三、门禁盲区（三路独立发现）：**

20. **「-p 包集口径盲区」**：权威 host 测试用显式 -p 子集排除 tests/ 成员，集成测试 crate 编译
    断裂（mock trait 未同步）对全部绿色门禁不可见数周（AF-5）。三路独立发现（B/C2/D）。
    判据：门禁须含一条 workspace --all-targets 编译兜底——**目前仓内尚未落地**（grep 零命中），
    本节登记为建议，不立检查（门出生即红违例）。
21. **「验证数字不绑包集」**：同一 commit 的 1400/0 与 1771/0 并非矛盾（4-crate 子集 vs 6-crate
    权威集）。判据：计数断言必须引用产生它的完整命令行；对账仅允许同口径数字互比。
22. **「死腿全绿」**：懒 FPU 门控位值写错（Initial 而非 Off）→轮转腿不可达死代码，host 单测+
    布局检查全绿结构性失明——「探针 0 命中」的语义是「腿未通」而非「无事发生」（运行期可达性
    只认真机命中计数>0）。
23. **「marker 回显假阳性」**（RETRO 行 105-106）：marker 字面量存在于 /etc/rc 脚本文本内，
    cat 回显形态可单独满足计数——计数须分形态（裸 stdout 行 vs 文件回显行）。
24. **「残渣扫描正则过时」**（R31 行 35）：探针残渣 0 命中的净判定基于固定字面量 nkNN/NK*-TEMP，
    不覆盖 sf-*/mrr 等后期命名族——模式集须从探针命名史动态生成。
25. **「兼容层宏降级成函数」**（续-277 CodeReview P1）：编译期宏被做成返回 0 的函数→测试循环
    零次执行仍全绿（空洞通过）。判据：断言循环计数>0。
26. **「pipefail×grep -q×大输出假失败三要件」**：命中即退令上游收 SIGPIPE→管道 141 被判失败
    （四发复发：真机门三轮假 FAIL+smoke）。修=直读文件（grep -qa）。全仓 40+ 处合法使用——
    误报率>信息量，归 §5 纪律不立检查。
27. **「打印宽度小于值域」**：2 位 hex 打 0x400 截成 00（归因带偏一轮）；复合编码字段按 u16
    打印丢高 16 位（三轮才破案）。判据：诊断格式宽度对最大值域做编译期断言。
28. **「台账邻位误归因」**：sysconf 占位使某用例从未真跑，「同 case 全过」系邻位 passed 误归因——
    台账「通过」须以本案结果行为准（无结果行=未跑非通过）。

**四、内核语义：**

29. **「恢复类 asm clobber callee-saved」**（续-129，行 175 区）：恢复函数 clobber 列含 callee-saved
    FP 寄存器→LLVM 在 epilogue 自动 reload 把刚恢复的用户车道覆盖回进入值——真机两轮稳定只是
    单 FP 场景的偶然保护。判据：恢复类 asm 禁 clobber callee-saved（「故意不实声明」须配捕获测试）。
30. **「策略寄存器治理三合一」**（续-129/131）：同一 CPACR 双写点互相覆盖+位值与语义名不符
    （名「脏」值实「净」）+注释位值漂移骗过静态审查。判据：策略寄存器单点所有+常量名→规范位值→
    消费谓词三表闭合（rg 交叉检索）。
31. **「live-CSR 写被 trap 出口整帧回装冲销」**：门控载体应为帧而非 CSR——活写 sstatus 的门控
    被 trap stub 出口回装冲销。
32. **「跳过优化判据读软件镜像」**：凡「缓存态 vs 硬件态」双记账的 skip 快路径，判据必须读硬件
    实时寄存器——镜像分叉后假阳性跳过跑错页表树（对位 C klib.S 读硬件判例）。
33. **「代写页表按变更类型刷缓存义务」**：同帧改权限/换物理帧/新建映射三路写全程零失效——
    换帧不失效旧译文产出「单字陈旧邻字正常」签名。
34. **「钳制掩护缺失臂」**（SD-24，行 996）：过渡守卫钳把 SMP 正确性域转成调度限制——三个缺失
    臂因此永不触发、永不见报，「removal conditions 才是实际交付物」。判据：钳制类守卫枚举其
    登记的缺失臂逐臂验证存在实现。
35. **「C 无权威处的自由选择无人标裁决」**（SD-14）：C 源对 x86_64 IPC 状态寄存器沉默，重写选
    RBX（callee-saved）成为最高价值未决 ABI 决定，每次编译器升级重掷骰子。判据：C 沉默处的
    自由选择强制标 pending-decision 入 OQ 队列。
36. **「枚举修复遗类开放」**（SD-16 R2）：conversion table 修复已知腿、类保持开放——失败发生在
    「未来位点」时必须选类型闭合（newtype）而非枚举表（calls.rs 臂原样复发）。判据：修复方案
    评审必问「失败发生在既有位点还是未来位点」。
37. **「回避约束传染」**（SD-26）：不可修债把「刻意不碰 PM」写进多个无关修复的隐含约束——
    回避成本放大一切诊断成本。判据：grep commit message/注释的 bypass/不碰表述回溯背后未修债。
38. **「已裁决未排程」**（SD-22/6）：决定记在散文里、无批次无触发器，永久悬置且阻塞下游验证。
    判据：grep 设计文档裁决表述↔批次计划对账（有裁决无批次即登记）。
39. **「跨持有者字段单侧半」**（SD-15b）：缺页地址三架构都写、仅 x86 消费（resumed context 的
    TLB 冲刷腿）。判据：字段级 writer 集×reader 集，差集非空即单侧半。
40. **「孤儿原语×承诺注释交集」**（SD-23）：完整已测的 eager save/restore 零生产调用者，两条
    承诺注释把缺失接线描述为已存在——两个已录形态的交集才是「穿过多轮 review」的原因。

**五、wire/测试：**

41. **「类型化 codec 存在处手搓 raw」**（RETRO 行 37-44）：类型化编解码已在而客户端手写 raw 偏移
    →同一 wire 契约的第二编码，漂移即崩（TTY 崩）。判据：codec 存在处禁手写 raw 布局。
42. **「零消费者 wire 包装潜伏双错」**（续-132）：客户端编码与服务器解码必须有一对同源 round-trip
    测试（同一 union 臂常量）；「脚本化测试验证了错误 wire」比没有测试更危险。
43. **「夹具形态合法性」**：夹具用生产不存在的形→防御性收紧把「夹具宽容」误读成「行为回归」；
    夹具期望隐式绑定 libc 实现（picolibc random 序列≠NetBSD→结构性永久失败）。
44. **「外来 libc 符号接管三定律」**：别名不满足裸名引用→链接器拽入库内同名成员→强符号双定义。

**六、流程/治理：**

45. **「误诊码滞留生产树」**：为证伪假设打的防御码在假设被证伪后滞留（无 cfg 门的硬编码地址窗、
    单遍减法隐式依赖）——假设证伪轮的代码改动默认随轮回退，保留须单独立项+评审。
46. **「既有载具 grep 先行」**（SD 总册 R2）：修复建议三轮改判皆因低估代码里已有的现成载具
    （PageSupplier/delivermsg seam/孤儿 FPU 原语）——建议落地前强制既有载具检索。
47. **「反幻觉核查」**（misc 5.21）：「缺陷已由外部修复，评审其提交后继续」类指令与证据不符
    （不存在对应提交）——固定证据清单可机械裁决。
48. **「正确但搭车的修复回滚后转登记为债」**（SD-21）：真机证伪回滚的修复携独立证据链转债条目，
    三个月后升格主犯时顺畅重落地——「正确」与「该不该落地」是两个判据。
49. **「commit message 是根因密度最高语料」**（SD 总册 §2 实测结论）：对历史日志挖掘的扫面
    优先级排序直接适用（本轮 route E 的产出密度证实）。

### 10.3 机械化判定

| 变化 | 内容 |
|---|---|
| P2_TESTS +21 | 续-279m brk 真源直译族 ×8+split 丢 memtype 生产级 P0 ×2+堆增长重叠拒绝（4994feddd）、续-132 PL011 MMIO 臂 ×4（2a6c55912）、续-277c 跨 libc stat 布局对账（85c12c6e2）、riscv 懒 FPU FS 门控（ef3c40c6b）、续-133 FPU 陷阱谓词钉 FS==Off（f89b7c731）、ATF 套件对账三件（51ac9e5c5/d33c24fd7）——全部 #[test] 属性+grep 在位+出处提交钉准（64/64） |
| P12 修正（第三发） | 包含函数名架构前缀（riscv64/aarch64/x86_64/x86）视为隐式架构门——本轮主树实跑抓到 trap_dispatch.rs:1980（riscv64_pagefault_body 的 csrr satp 探针，cfg 门在 44 行外）驱动；修正后该误报消除，一个旧基线键（trap_dispatch.rs:975 cr2 探针，在 x86_trap_dispatch_body 内）结构性豁免（语义核实正确），基线存量 3→2。selftest 新增正反例 1 组 |
| 无其他新 P 检查 | pipefail×grep -q 组合（40+ 处合法使用）、孤儿 API 零调用者（WIP 面大）、workspace 编译兜底（未落地，门出生即红）均按误报率>信息量判据归报告建议；结构性拦截继续靠 P6 与存量对账 |
| baseline 不变 | P7×3/P8×6/P14 存量全在基线；P12 修正后 3→2（结构性豁免非删除，基线键保留无害）；P10 增量 0 新增裸引；代码面 P13 符号 21 个零虚构 |

### 10.4 复发确认（归并既有族，本轮再添实例）

- **观测扰动翻转**：速度型插件使 race 消失（续-270）/探针构建改变崩率（续-283 统计定谳）——对策升格为二项检验公式。
- **跨轮对账必错**：旧 boot 的 0x9dc39000 对当轮 0x9dc38000 拼破案叙事（续-163/164 终审证伪）。
- **穷举不全 E5**：update_flags 两调用者只 instrument 一条（4 轮矛盾）；「addi 不可能自陷」枚举漏控制流脱轨（终局恰是脱轨）。
- **隐式不变量成立域未声明**：VmDm 跳 sfence 的「not-present→present 无需刷」前提被 CoW RO→RW 场景击穿（续-164）。
- **修复生效声明与代码未执行（空转归因）**：单轮零触发即宣告「家族全灭」，下轮复发（续-168/169）。
- **carrier/观测条件外推**：-S 时序扰动下的形态两次被立为定论（续-206/209 被正常 boot 推翻）。
- **承诺注释当实现**：三腿完成臂「NoReply 不在此腿出现」注释断言被新调用方击穿（panic）；「sstatus.SUM is set later」同族。
- **B7 布局算术**：跨 libc struct stat 尺寸/偏移差越界砸保存寄存器（全零跳转误导现场）。
- **panic 渲染截断/打印宽度**：2 位 hex 分母截成 00（歧义读数带偏归因）。
- **伪对位翻译/无操作兼容层**：槽位 hint 当 break 语义驱动两轮试修；libsemihost 假 sbrk 永不下陷。
- **半修复解锁危险态**：新 brk 腿未通先拆旧路径→libc 16B 步重试环比原崩更糟（静默挂）。
- **正交失败模式入口分诊**：β（真槽读）与 α（控制流脱轨）共享症状域，统一归因致错靶——沉淀出 a1==stval 分诊判别器。
- **捕获探针预算/通道**：first-N 门控+缓冲宽度+链式分事务成标准件；diagctl 风暴窗口非确定丢弃（A/B 双轮判别配方）。
- **交接数值转抄错误**：x19 被转抄成 sepc 指纹进入 banner+交接 prompt（下游按错误指纹对账）。
- **账面滞后**：reviewlog 状态标记未随闭合更新（幽灵 open，见 10.2-18）。

### 10.5 正面沉淀（§5 补充）

- **零改动诊断仪**（misc 2.19）：按包开调试断言=绕开「处理路径内禁搭仪器」禁区的现成仪器——违反点在真机精确行崩溃。
- **手段能力矩阵**（misc 2.20）：假设收敛期强制列七种仪器的能力边界（零扰动？/产出类型/当前状态），路线选择从矩阵推导。
- **崩溃地址算术判读四判据**（misc 2.22）：槽读形状/页内偏移自洽/差值等于容量/语义坐标逐层换算——零依赖符号与快照的判读前置门。
- **结论受控词表+降级台账**（misc 2.25）：未坐实与已排除混写会让接手者在废结论上盖楼；「再现重开」登记项。
- **判据谓词语义层级**（misc 2.23）：语义谓词 vs 字节模式决定证据强度差一个量级——归档谓词原文。
- **契约测试折叠**（misc 5.29/SD 总册 §6）：把仅真机可验的验收折进宿主测试的债永不再消耗真机轮次——15 笔折叠候选已在册；抗性边界=可达性/多核 TLB/微架构时序/固件通道四类真机保留域。
- **同型位点表+顺序对账表**（misc 5.23/5.31）：实施轮从探索降级为对账；同序阶段排除重排类修法。
- **预裁决三件套**（SD §E）：可观测触发条件+授权边界+终局归属；冲突走勘误不回改正文。
- **审计隔离架构**（RETRO）：detached worktree+git show 读 committed blob+真机结论串口重放（重放=解析既有证据，重跑=重建现场）。
- **锚点未命中≠造假**（RETRO 行 154）：换路径全树重搜后才可下造假结论（险误报 P0-fact 判例）。

### 10.6 语料划界

- FIXLOG 四份/.review STATE/edge_todo 实测零增量；~/.zcode/cli 维持划界不扫；tmp/nk4a raw 产物维持不扫。
- misc_concepts §3.45 撞号登记不代改（NK4-C 线文件且主树在制）；03-stage-rs 文档重排线产物（doc_rerank_qwen 等）为设计文档非教训载体，轻扫未立条。
- VEC-CAP 双档案维持 C-64 已扫结论；TRANSIENT-PTE/SD 总册/RETRO-AUDIT 本轮首扫完成（untracked 三件建议其线闭环后定入库归属）。

---

## §11 补扫+增量合一轮（C-66；增量截断面 484b5a12b→现行 HEAD + 历史缺口 8 处闭合）

> 承接 §9/§10 与上轮盘点：C-65 结束时确认增量流已追平、历史主干已扫，但「全部」口径下有 8 处缺口。
> 本轮定位=补扫闭合缺口+追平新增量。四路并行深读提取 **119 条候选**（增量 29/迁移线 20/.review
> 扩面 40/小件包 30）。**最重要的结构性发现：C-63 的最大单点遗漏是 edge1-kfix.md**——一个文件贡献
> 10+ 条平台前提类新形态（boot hart 任意性/SBI ret=0 假成功/PTE 保留编码软硬判据分裂/AP 位错位/
> WPRI 位写/固件停放区踩向量）。

### 11.1 增量语料（实测边界）

| 语料 | 规模 | 说明 |
|---|---|---|
| git 提交 484b5a12b..HEAD | 86 笔 | 续-284~341 + T13 地址常量清扫交付；**零新增 #[test]**（os/ 与 tools/ 实测均 0） |
| NK4C-WORKLOG delta | +732 行 | 续-284~341 |
| misc_concepts 增量五 | 148→160（+199 行） | |
| 新文档 | R32 评审报告+对账表、QEMU-ENVIRONMENTS（248）、ADDRESS-CONSTANT-AUDIT（226）+tools/address-constant-scan.py（79 新工具）、接续PROMPT、TRANSIENT-PTE delta +224 | 三件 untracked 档案（SD 总册/RETRO-AUDIT/TRANSIENT-PTE）已在窗口内转 tracked（上轮 G10 缺口闭合） |
| 历史缺口闭合 | new_laptop_migrate 8 件（42KB）首扫、.review 三工具扩面 38 文件（21 全读+17 抽读+4 密度抽查）、NK4B-WORKLOG 后半 1622 行、HANDOFF-NK4A×2、NK4-REGRESSION-REVIEW×2、NK4A/B-TODO 非定向部分、AI-chats daily.todo 中段、new_edge1/2/3 他线行 | |
| 实测零增量 | FIXLOG 四份（仍 09-20~23）、.review STATE | |

### 11.2 新增模式（归并既有族；按路组织，锚点经 grep 实测重钉）

**一、增量路（续-284~341 + T13）：**

1. **「硬件前置语义盲区」**（WORKLOG:12708，续-338）：「PTE 有效+CPU 仍 fault」这类架构上不可能的
   矛盾，第一步先算地址规范性（canonicality）——全套 in-guest 自检漏这一项，因为软件走表按位取
   索引天然不看高位；「不可能态」其实是检查域盲区。T13 判据表已机械化此形态。
2. **「归账正则色集」**（WORKLOG:12208，续-286）：「无结果行」≠「非 passed 结果行」——atf 合法终态
   `skipped:reason` 双漏计，2 例缺额升格成整案误判下一轮反转。判据：终态色集 {passed,failed,broken,
   skipped} 逐色喂样本验正则；配套 Rule Discovery「计数规则采纳时 grep 同文件全部计数点一次扫清」。
3. **「跨 run 生命周期伪差」**（WORKLOG:12524，续-330 撤回笔）：帧号/root PA 是 run 内局部标识，
   跨 run 对账产生伪「根错配」——两次「破案铁证」即撤。判据：动态分配值对账必须同 run 同构建。
4. **「限次兜底的静默残留」**（region_map.rs:102-104，续-334 成修）：视图摆动被重试×8 吞成自愈，
   根因修复后该兜底成 workaround 残留，叠加既有 pop_first 重建 3 次封顶——真损坏复发时第 4 次起
   失明。判据：兜底/重试计数点必须配封顶后告警路径；结案清场清单核对项。
5. **「门分母自派生」**（test-atf-riscv64.sh:90 续-340b）：EXPECTED 从被测产物链自身派生——装配残缺
   时期望同步缩水，门对残缺集判 PASS。判据：分母计数必须锚独立真源（manifest/常量）。
6. **「跨目标编译的架构前提断言」**（WORKLOG:12742，续-338b P0）：「按架构取值的常量」接入共享 crate
   后，既有 `<2^39` 断言在宿主编译下测的是另一架构的值——断言此前「一直通过」给人错误信心。
7. **「审计交付无回归网」**（AUDIT:218 + 实测 86 笔零 #[test]）：T13 清扫 13 项发现（高危 3）零钉扎——
   无编译期断言、无测试、无 CI 门，R1-R5 悬在建议层，高危 A3 是「一条 guest 调用即可触发」的活口。
   判据：审计交付门——每个高危发现必须绑定钉扎物或显式 DEFERRED 编号。
8. **「审计判定探针活性须带纪元参数」**（AUDIT:137 A7 失准）：以「当前合法地址域」判探针死活，
   未区分缺陷纪元——探针执勤期猎取的地址恰高于阈值（当时是活的），修复后才恒死；对探针作者的
   「x86 心智模型」归因失实。
9. **「实验工件入树的非文本盲区」**：os/target_smp 122 个 cargo 缓存 + os/tmp 23 个取证串口 +
   零引用 .bin 入树且 WORKLOG 零提及——文本残渣扫描（nkNN/NK*-TEMP 模式）对非文本工件形态
   整体逃逸；且与 tmp/nk4a/ 惯例分叉出 os/tmp/ 孤儿副本。**机械化：新检查 P17（report 式）**。
10. **「同语义多腿差分是免费对照组」**（WORKLOG:12691）：「boot 腿活、exec 腿死」的差分形状长期在场
    直到定案回溯才被识别为判据——健康腿即免费对照，应升格为排查首查项。
11. **「扫描工具自身死通道」**：address-constant-scan.py 的 DEC_RE 定义未使用（docstring 声称十进制
    通道、实现缺失）——工具对自己能力的 P13；十进制 ≥2^38 常量静默漏扫。归 NK4-C 线修（不代改）。
12. **「串口行内交织碎裂计数」**（WORKLOG:12673）：并发串口把探针行碎成「fillt oo」，整串匹配把在场
    证据计成零输出——判别探针换装后才补齐。判据：串口解析共用 helper（字节级+空白容错+碎片正则）。

**二、迁移线（new_laptop_migrate + AI-chats）：**

13. **「大 blob 入史阻断 push」**（GIT取证会话记录:22）：586MB 串口证据被 add -f 绕过 .gitignore 入史，
    一个 blob 扣押全部 2008 未推送提交；同文件揭示 ignore 防线的洞（`!/tmp/nk4a/` 白名单放行+
    .int/.img/.serial 零规则——新机一次普通 git add -A 就会扫入 693MB）。「防复发防线自身有洞」
    二阶形态：未爆地雷比已爆的更危险。
14. **「filter-branch 残留反噬」**（同文件:139）：历史重写后 refs/original/* 残留若一并 push 会把刚摘除
    的大 blob 原样带回——清理静默归零（实测当前 refs/original=0 干净）。
15. **「交接第一动作前提失效」**（同文件:12）：交接件写「迁移第一动作必须 git push」，push 可行性已被
    586MB blob 推翻且未同步——照章执行第一步即失败（前提过期 E7 在交接链上的第 N 发）。
16. **「记忆库路径哈希断链」**（同文件:163）：宿主级记忆库按项目绝对路径哈希索引，路径一变静默断链、
    无报错——迁移四表协议未覆盖的隐性断链。
17. **「架构契约未落规则」**（daily.todo:110，已落地判例）：「命令层只依赖 libc 层」契约曾只存在于口头，
    每轮会话自创违规路径——后由 tools/check-command-boundary.sh 落地；判例出处登记。
18. **「编号孤儿」**（daily.todo:228）：scan/评审临时编号写进正式文档与代码，中间产物删除后全部引用
    变孤儿——「过程痕迹不入正式文档」规则的事故原型（Hidden Folder Convention 的起源）。

**三、.review 扩面（edge1-kfix.md 平台前提族 + 账面族）：**

19. **「boot hart 不可假设」**（edge1-kfix.md:287）：OpenSBI/QEMU 每次复位任意选 boot hart（Domain0
    Boot HART 0/1 摇摆），载体硬编码 cpus[1] 使半数轮次自启——平台非确定性被误归因载体竞态。
20. **「平台 API 恒绿假成功」**（:260）：SBI send_ipi ret=0 表示接受投递但 MSIP 停在 M 级永不被取——
    成功返回码与投递生效完全脱钩，-d int 全量 trace 才定案。
21. **「软硬双判据分歧」**（:315）：L3 描述符 0b01 是硬件保留编码（MMU 直接 fault），自实现软件 walk
    却能找到条目——验证器与执行器判据分裂，「查得到、用不了」极具误导。
22. **「权限位错位+反函数同源互掩」**（:316）：AP[2:1] 常量错一位，正反函数同源错误互相掩盖，只有
    真机权限故障能暴露。
23. **「boot 期 cfg 门的测试静默缺席」**（NK4B-WORKLOG:838）：目标架构 cfg 门的 mod 内测试在宿主编译
    单元里不存在——「编译通过、测试为零」的假收敛。判据：新增测试必须伴随宿主测试计数上涨。
24. **「空上下文替身主动作恶」**（FIXES-LOG:707）：兼容层 helper 自建 fresh 空 PrivTable 恒 false——
    死腿不是绿而是生产路径全量误拒（SYS_PRIVCTL 全被错拒的 latent bug）。
25. **「同一配置两个方向失真」**（FIXES-LOG:441+STATE:341）：RUST_TEST_THREADS=1 既掩盖夹具竞争
    （假绿）又因从仓库根调用漏读 os/.cargo/config 而静默失效（flake 显形）。
26. **「幽灵完成清单」**（trae STATE:457）：完成清单带行数细节（229行/195行）但产物从未生成——
    账面比真实更具体，细节反而强化假象。
27. **「CONVERGED 语义漂移」**（WORKFLOW-META-REVIEW:176）：「我审完了一遍」≠「产物完整」，两种
    CONVERGED 不可区分——产物缺失在账面上不可见。
28. **「预生成=提前写答案」**（trae STATE:1252）：机械化补齐 40 个预生成快照被裁定为违背 review
    独立性整批回滚——形式合规≠实质合规的判例。
29. **「批量豁免未批先判」**（trae STATE:1715）：单次豁免被静默泛化（局部决策的全局化扩散），批量
    豁免登记「待用户确认」但 CONVERGED 判定先于批准落地。
30. **「u32 截断骗过下游校验」**（FIXES-LOG:418）：priority=256 截断为 0 通过 `0>15=false` 校验——
    用户态骗得最高优先级（提权）；「只加注释不改逻辑」的修复让已知漏洞多活一轮。
31. **「grep 命中≠语义替代」**（TODO-07-VERIFICATION:232）：回归验证只 grep 了 DirectMapArch 导入
    未验证函数实际行为——修 symptom≠修 cause，旧机制与新机制并存只有 Read 函数体可见。
32. **「零发现自检的激励畸变」**（RULE-SELF-REVIEW:222）：零发现触发重审的机制可被「留一些发现」
    策略性规避——度量规则的博弈面（规则设计者自己承认）。
33. **「文风门缺位期成本后置」**（codex sched STATE:97）：14 篇通过全部技术门后因文言文风被用户
    要求整体返工——缺位的门把成本推给最贵的裁决点。
34. **「per-doc 绿与 stage 真实态脱钩」**（claude STATE:370）：mib 22 篇逐文档 CONVERGED 而执行半
    整层缺席（main.rs 空转/walker 零调用方）——per-doc 判定聚合不成 stage 判定，跨两 stage 复现。

**四、小件包（NK4B 后半 + 账本他线行）：**

35. **「全称否定断言的范围词纪律」**（NK4B:2152，SUM/PAN 双发）：「全仓三处提到 SUM」实扫两目录——
    范围词必须与 grep 实参一字不差，范围词不能靠括号收窄。本弧七轮评审收敛的单根因「先写结论
    再补命令」的主规则化。
36. **「断言自坏」**（NK4B:2411）：防回归断言被解释该断言的更正说明自身（逐字引用旧措辞）打红——
    改写错说法时用同义转述而非原文引用。纯新失效形状，任何断言型工作流可复用。
37. **「巧合等值假 PASS」**（NK4B:1762）：判别断言喂被拒装工件仍 PASS——QEMU 拒装后回退的平台约定址
    恰等于 .ld 的 KERNEL_PHYS_BASE；「反判例」自身要防回退巧合。配套「负例先证可再生」（:909，
    cargo clean -p 不删跨 target 工件，反向判别整节作废）。
38. **「计数锚点自污染」**（NK4B:1786）：计数正则被同文件自己的头注释命中（`] PASS` 字样），连踩两次
    收敛到行首格式锁；「锚点要么锁格式要么锁字串，不能锁长得像格式的字串」。
39. **「观察手段缺陷险推翻已成立结论」**（NK4B:1871）：grep 模式漏 ` (lib)` 四字符产生假阴性复现，
    差点当场改口推翻可复跑锚点——「复现不出先查自己的观察手段再改结论」。
40. **「载体与生产的 stvec 争夺」**（NK4B:2168）：生产初始化覆盖载体自装的 trap handler，载体验证副本
    静默变死腿——fault 落在生产代码的真正机制，也解释「载体开了 SUM 为何还 fault」。
41. **「同型窗口异构」**（NK4B:2337）：riscv csrs sstatus 立即生效、aarch64 写 SPSR_EL1 要 eret 后才
    生效而拷贝发生在 eret 前——两架构「同型开窗」实际不同构，开窗时机错位。
42. **「未验证归因写进提交消息」**（NK4B:2399）：把测试数变化归因于「并发会话合入 nt-tests」——仓内
    根本没有 nt-tests，实为基线只计 lib 口径；「数字没记错，因果是我编的」。凡写「A 造成 B」必须附
    可复跑命令。
43. **「子代理旁支细节是反例线索」**（NK4B:2394）：子代理 PASSED 结论顺带提及的旁支常量恰是推翻
    编排者全称断言的反例——绿灯正文之外的边角料须回读。
44. **「账本状态行被合并回退」**（new_edge2:22）：并发提交把已销账 ✅ 行回退成 ☐，认领锁面失真直接
    导致误领一次——共享账本的合并语义风险（与 C-64 销账滞留同族）。
45. **「长会话自述文档系统性降级」**（HANDOFF-NK4A:103）：200k 上下文模型多轮迭代后对「当初为什么
    这么写」失忆混淆——自述文档降级为地图，文档-代码矛盾本身列为评审发现。

### 11.3 .review 扩面密度结论（回答「1667 个未读文件还要不要扫」）

- 六族高密度文件（STATE/fix-status/VERIFICATION/fix-log/test-audit/edge-kfix 命名族，约 **80-120 个**）
  值得后续轮次扫完——本轮 38 文件已产出 40 条，是四路中单文件密度最高的路（edge1-kfix.md 一个文件
  10+ 条）。
- scan/structure/SYMBOLS/VERIFY 中间产物（约 **300 个**）经抽查确认低叙事密度，**永久跳过**。
- codex 24 个 per-stage 扫描卷宗与 per-doc 快照为门禁证据存档，密度中低，按需抽查。
- 本轮扩面 38/1667（2.2%）+ C-63 的 22 文件——高密度族覆盖约 1/3；剩余按增量轮节奏消化即可，
  不值得单独一轮全量通读（边际密度已低于增量流）。

### 11.4 机械化判定

| 变化 | 内容 |
|---|---|
| P12 基线 +1/-1 | 新命中 lib.rs:3842（finish_and_restore 内 sfence.vma，riscv 专属指令；函数无架构前缀无 cfg 门——第四发误报形态「门在调用图可达性里」，x86 构建下函数不可达故宿主全绿）。人工复核后 `--update-baseline`：新键入库；一个被 C-65 架构前缀启发式结构性豁免的陈旧键（trap_dispatch.rs:975）自然消解；P8 一处 CRLF 已被 NK4-C 线转 LF（6→5）同步消解 |
| P17 新检查（report 式） | 构建缓存/取证产物跟踪防线：os/target_smp+os/tmp 的 git ls-files 计数报告（现 122+23=145），镜像 P10 形态不阻断——清理归 NK4-C 收线批次，清零后可转强制门。selftest 加 SKIP 用例 |
| P2_TESTS +0（如实记） | 本增量 86 笔**零新增 #[test]**（os/ 与 tools/ 实测 0）——T13 纯审计交付其 13 项发现零钉扎已立为形态 7；brk/split/PL011 等族的测试在 C-65 已全量入列 |
| 无其他新 P 检查 | 大 blob 体积扫描（全对象 git cat-file 较重，gate 运行时敏感）与 refs/original 残留（当前实测 0）归 §5 纪律与 git 取证线；DEC_RE 死正则归 NK4-C 线修（不代改他线工具） |

### 11.5 复发确认（要点）

产物身份失真五连（批次误标/mod bins/table.bin/旧 serial 假绿/SKIP mtime 链——建议机制级新鲜度断言收口）；常量心算三连（1<<38 位移错位形/&0x7fffffff 掩码假设/i32 符扩形状——「先查架构上限再心算生产语义」入四判据族）；归账正则色集复发（K20 计数口径 139→138 同次虚高+漏计）；跨 run 对账必错两发；观测扰动（CAP=8 被早期无关 fault 耗尽）；死腿全绿（休眠缺口成簇：profile 零调用方/dormant EOI）；单侧半（VMINHIBIT 远程臂漏 FLUSH_TLB）；wire 常量错位三例（VM_INFO 0/1/2 vs C 1/2/3 等——处方固化为协议常量数值守卫测试）；门禁不阻断（review-init --design-policy strict 只 print 不 exit 2）。

### 11.6 正面沉淀（§5 补充）

fix-guard grep 重放拦截批量误报（3+2 处，判据流程按设计生效的实证）；「树必须常绿」整批回退+统一参数次序使机械重构十轮不收敛变一次贯通（K20）；同 fault 闭合判据/翻转判据（None→Some）/全寄存器定谳法；门判据离线双形态回归（marker 回显形态预防性回归）；审计隔离架构的重放降级（RETRO）在本轮 R32 复用良好；「补一行打印先于一切修复」。

### 11.7 语料划界更新

- 8 处历史缺口：全部闭合或定界（§11.3 .review 扩面 38 文件+密度结论；new_laptop_migrate 8 件全读；
  NK4B 后半/HANDOFF×2/REGRESSION×2/TODO 正文/AI-chats 中段/new_edge1-3 他线行全读）。
- **历史欠账状态：主干与高密度族已尽。剩余=.review 六族高密度文件约 80-120 个（按增量轮节奏消化，
  不单独立轮）+ scan/structure 等中间产物约 300 个永久跳过。**
- ~/.zcode/cli 维持划界；tmp/nk4a raw 产物维持不扫；os/target_smp+os/tmp 的 145 个已跟踪工件建议
  NK4-C 收线时清理（P17 持续计数跟踪）。

---

## §12 增量轮四（C-67；截断面 5cf88a552→07ae5efed，NK4-C 续-342~384 三终收尾）

> 承接 §9-§11。本轮增量是 NK4-C 三终目标的收官段：53 笔提交内完成目标③ riscv 36/36 正式达成
> （续-384）与三终结案横幅。深读三路提取 **63 条候选**（增量 20/.review 高密度族 28/提交消息 15）。
> 两条结构性结论：①收官宣告存在四重折扣（宣告先于去插桩复验/判据未钉构建指纹/回归面含灭证间歇/
> 单样本叠加开放概率背景）；②**.review 欠账清零**——非中间产物全集实为 39 个（上轮 80-120 估计偏大），
> 本批 27 个全消化后基本无剩余。

### 12.1 增量语料（实测边界）

| 语料 | 规模 | 说明 |
|---|---|---|
| git 提交 5cf88a552..07ae5efed | 53 笔 | 续-342~384 + 收尾 + R33 + misc 增量六（160→169） |
| NK4C-WORKLOG delta | +711 行 | 收官段 |
| NK4C-BUG-RISCV64-MEMORY-CORRUPTION.md | 542 行（首扫） | 终局档案；**已提交版冻结早熟，+301/-60 终态更正悬置脏改动**（见 12.2-8） |
| R33 评审群 | 报告 188 + 对账表 84 | 对账表含 63 个裸 NEL 字节非合法 UTF-8（12.2-17） |
| 接续 PROMPT b/c | 107+143 | 含结案横幅 |
| .review 高密度族 | 27 个非中间产物文件全消化（18 全读+9 抽读） | 上轮 80-120 估计修正为 39；scan/structure 族 1523 个维持永久跳过 |
| 实测零增量 | FIXLOG 四份（仍 09-20~23）、.review STATE | |

### 12.2 新增模式（归并既有族；锚点 grep 实测重钉）

**一、收官宣告质量（四重折扣）：**

1. **「宣告先于去插桩复验」**（WORKLOG:13509）：36/36 判据读数采集于带全族诊断探针的内核，而本案
   自证「插桩打印量本身是独立变量」——「探针滚除后的门复跑」列为下一笔。收官里程碑的判据读数必须
   在去插桩态复验。
2. **「奠基证据未钉构建指纹」**（WORKLOG:13487-13509 全节 md5 计数=0）：收官 36/36 未钉 kernel/ELF
   任何构建指纹——R33 已点名过同缺口（H9），最重要的里程碑原样复发。
3. **「灭证间歇标签」**（WORKLOG:13505）：aarch64 回归门一次缺案，证据已被 cleanup 删除，缺案身份
   永久不可仲裁，仍贴「一次性间歇」标签并附未坐实猜测——门/工作流的清理步骤必须豁免异常轮产物。
4. **「概率框架吸附确定性子案」**（WORKLOG:13）：t_strerror 在每轮 riscv 全量门 4/4 用例全灭（三轮
   同脸）而同二进制 aarch64 全绿=确定性缺陷，却在「非确定污染」概率框架下滞留三轮。判据：同一失败
   脸谱跨 boot 计数 ≥3 ⇒ 确定性优先分诊（可机械化）。
5. **「评审移交项无销账面」**（R33:156 H1）：评审明确「交主线」的处置（rm --cached 147 工件+gitignore
   三条）在 53 笔中零执行、未进任何收尾清单——评审移交项逐条对账 git 状态。

**二、终局档案与账面：**

6. **「幽灵未完成清单」**（AUDIT:237 vs boot.rs:130）：审计文档修复追踪注记停在旧时点，后续轮已修项
   不回写——按权威审计文档核对得到与树相反的状态（上轮「幽灵完成清单」的反极性）。判据：审计发现项
   ↔代码锚点逐项脚本对账。
7. **「终局档案冻结早熟」**：已提交版冻在交接时点（「实验丁尚未测」与收官事实相反），三腿修复/定谳
   全在 WORKLOG 与未提交脏改动里，终态更正靠接续 PROMPT 横幅外挂——fresh clone 者拿到的是中段
   快照；且已定谳样本（VA=0=TLS 缺陷）仍以腐坏面孔留在档案判读面，除名只写在另一文件。
8. **「防御性丢弃臂无丢弃计数」**（WORKLOG:15）：为堵洪流安装的静默丢弃臂成为九环停摆链唯一零可见
   环节，三轮才定位——「丢弃臂打丢弃标记」纪律在案但无清点门保证执行。判据：全仓防御性丢弃臂清点
   丢弃计数/日志一行扫。
9. **「参考实现同手派生」**（WORKLOG:13004）：宿主对抗测试 5000 点模糊对账对 TLS 掩码误解零差异——
   参考实现复制了被测实现同一误解，差分测试鉴别力上限=参考来源独立性。判据：参考实现标注
   「规范转写/同手派生」来源级别。
10. **「签名参数化强制枚举」**（WORKLOG:15）：给 exit_proc 增加 sig_status 参数迫使触碰全部 6 调用点，
    翻出 step10 恒零硬编码——潜伏缺陷藏在从未被终态覆盖的分支上，签名变更是唯一强制枚举机制。

**三、绿证据失真（提交消息路核心簇）：**

11. **「测试不住 cfg(target_arch) 模块」**（e4c03cebb）：写在架构门控模块内的测试宿主 cargo test 根本
    不编译=从未存在的守卫（本批四次反复）。
12. **「手抄 wire 布局漂移」**（58b9c4195）：探针手抄 struct 少 m_source 使魔术串从未到达内核——
    证据生产链静默断裂产出假干净零读数。
13. **「解析器锚点命中文档注释」**（232ee7e9d）：形状测试锚点命中文档注释里的同名标号串，切片切错段，
    六条规则全绿而真腿从未进检查器——巧合式正确。
14. **「跨架构对照只核单侧」**（378a06d15）：「用户腿清白/两腿封死」只核了保存侧，缺陷在未核的恢复侧
    ——overclaim 下一笔即被打脸；跨架构对照必须盖入口保存与出口恢复两段。
15. **「探针封顶摧毁频次语义」**（0fde7999b）：INACT_N 封顶 4 次使「出现过一条」被读成「只发生一次」
    ——一过性与活锁式反复修法完全不同而不可判别；改 2 的幂节流打点恢复计数。
16. **「绿无可达性计数不定谳修复」**（fc8eea4e4）：全绿但无路径执行计数，「修好」宣告被自己的判据纪律
    拦住；同族：12 个不碰 errno 的用例全绿掩盖确定性 TLS 缺陷至今。
17. **「注释事实断言被当证据」**（295201a7a）：「riscv crt0 反汇无 TLS 步骤」注释断言为假且恰在自述
    触发条件上翻车；同族十六进制地址少一位——注释是无编译器检查的证据面。
18. **「驱动自杀误读」**（7c297a7ec）：门脚本 STALLED 收尾 kill 被读成客机自主关机——凡「谁让 QEMU
    退出」断言先看驱动脚本自己的 kill 路径（「脚本动作主体补进候选解释集」的收官实例）。
19. **「报错远离真因」**（295201a7a）：镜像配方 $ 收口符漏写使报错停在 EOF，与真因位置无关。

**四、.review 高密度族（账面/评审方法学）：**

20. **「跨模块模板复制残留」**（codex devman STATE:62）：Resume Point 预检命令参数是别的模块名——
    照跑会校验别的模块的覆盖率。判据：STATE 内命令参数与自身目录名比对。
21. **「门/根因结论同报告冲突」**（claude 08-system:23 vs 94）：交叉检查门判「全部正确」、Double-check
    根因判「3 处错误」并存无复核。
22. **「行号漂移源在代码注释」**（同文件:30）：doc 行号漂移的源头是代码注释写错被 doc 忠实转述——
    上游换成代码工件。
23. **「多 AI 共识当证据」**（trae TODO-VERIFICATION:10）：两个 AI 先后报同一假缺陷成「共识」被当证据
    写进 P0——共识不是事实，唯一挡下的是 grep 验证（引用文件不存在）。
24. **「伪零发现」**（claude 05-clock:88）：连续四轮零发现被当流程改善庆祝，实为整整一个维度从未被
    检查——低发现率可能只反映审计维度未展开。
25. **「子集声明使计数不可证伪」**（claude 03-kmain:48）：doc 测试清单系统性 undercount 最高 4.2 倍，
    「仅列子集」声明使任何偏差都不可证伪。判据：doc 测试数 vs cargo test 实际数比值扫描。
26. **「同名验证门证据形态漂移」**（trae clock-timer:48 vs 03-stage:66）：同名门一边做名对账一边退化
    为纯通过计数——门标签一致而校验内容衰减。
27. **「跨 crate 重复 const 双盲」**（claude 05-clock:90）：DEFAULT_HZ 跨 crate 重复定义，编译器与测试
    双盲——「编译绿」对语义唯一性无信号。
28. **「容差未显眼化」**（同文件:84）：存在 30% 测试计数容差但不在门清单显眼位置——doc 声称数可偏离
    实际三成仍 PASS。
29. **「同文件双账本」**（WORKFLOW-FIX-SUMMARY:14 vs 47）：两个「一、」章节两份不同口径漏洞表并存
    ——追加式写作未合并先前段落。
30. **「scope 声明虚设」**（codex fork-syscall STATE:5 vs 116）：头部范围 drivers、正文混装 net 批章节。
31. **「同 agent 独立验证制度化推迟」**（codex 01-stage:27 等 ≥8 处）：「独立验证全由同一 agent 完成」
    局限自知并标注，修复方式是登记为里程碑 backlog——缺口被推迟而非消除。
32. **「偏移符号诊断漂移成因」**（claude 06-proc-init:89）：负偏移=锚点上方代码增量、正偏移=doc 写于
    代码删除后——锚点漂移从「对不对」升级为「往哪边错、为什么」。
33. **「forward-reference 合规基线」**（claude 10-switch:38）：引用不存在文件但显式标注「待落地」判合规
    ——显性声明使死链合法化，幻影链判定的必要对照。
34. **「born ignored 测试」**（trae 03-stage:220）：新增 2 测试出生即 ignored（依赖未接线 logger）仍计入
    交付。判据：新增测试清单 × #[ignore] 比对。

### 12.3 机械化判定

| 变化 | 内容 |
|---|---|
| P2_TESTS +12 | T13 A1/A2 修复 ×3（7ced6473b/043c64d56：用户范围双族对照/MAP_FIXED 越界 fail-closed/hint 越界拒绝——「审计无回归网」形态的修复闭环样本）+ 续-383 甲案 ×1（6310c7237 DumpCore 信号号）+ riscv walker 判别族 ×8（3199537b0/fcb8c542e：非规范 VA/错位巨叶/L0 指针形/V=0+RWX/W~R 叶形/4K 正路/高 PPN 位/差分模糊）——全部 #[test]+出处钉准（76/76）；变异验证改名即 FAIL |
| P12 键格式修正 | 键=P12|路径|内容 cksum（去行号）——交接教训⑦「基线键对行号漂移免疫」的实现归位：C-66 收编 lib.rs:3842 后函数长 6 行移至 3848 即失配重报（本轮主树实跑 FAIL 驱动）；修正后活体判别=3848 与 3842 同内容键命中。基线精确三换三（同 cksum 去行号），P7/P8/P14 键格式不动 |
| P17 维持 REPORT | 145 工件仍被跟踪（R33 H1 处置零执行=12.2-5 形态），清理仍待 NK4-C |
| 无其他新 P 检查 | 「N≥3 同脸确定性分诊」「丢弃臂计数扫」等强候选属判读规则/全仓清点类，误报率考量归报告；审计↔代码锚点对账脚本已由 NK4-C 自有交付物示范 |

### 12.4 复发确认（要点）

奠基证据未钉构建指纹（H9 复发）；产物身份失真（诊断镜像须覆盖构建嵌入路径名）；跨 run 跨阶段 diff 全噪声（对照实验同 run 双采样——跨轮对账必错族）；死腿全绿（tp 依赖面未枚举=边界声明无清账项变体）；灭证间歇与跨轮对账必错同族；幽灵 open（ENOSYS open 项缺席收官清单）；门跑在并发探针工作树（C-66 教训复核后采纳判例）；评审自评降级无豁免记录；workspace 红基线化跨三工具独立出现（AF-5 族）；「实现本就正确」类误报与真修同栏（发现-修复统计污染）。

### 12.5 正面沉淀（§5 补充）

「同脸 N≥3 确定性分诊」；「先读码后上机+C 真源逐环对照+修前判据预写」（goal-③ 两根因的共同发现路径）；评审发现逐条四选一显性处置（采纳/不采+理由+替代去向/登记/挂账）；勘误就地保留取证历史（【就地勘误】块不改写原文）；代码与台账同笔入库；交接文档结案横幅（防新会话按过期指令行动）；「什么也没发生」的读数比错读数更值钱，只能靠「本该出现却没出现」发现。

### 12.6 语料划界更新

- **.review 欠账清零**：非中间产物全集 39 个（上轮 80-120 估计修正），本批后剩余仅 1 个低价值候选
  清单文件；1523 个 scan/structure/SYMBOLS/VERIFY 中间产物维持永久跳过。历史欠账至此**全部闭合或
  永久豁免**。
- MEMORY-CORRUPTION 档案的终态更正（+301/-60）与探针滚除在 NK4-C 收尾批次在途（挖掘时点 28 文件
  未提交在制，未触碰）；P17 的 145 工件清理同批。
- 后续增量触发条件照旧：NK4-C 收尾批次落定后（滚除+档案终态入库）值得一小轮对账扫描。

---

## §13 增量轮五（C-68；截断面 ad14ced56→76c669b35，NK4-C 续-385~427）

> 承接 §12。本轮增量是三终达成后的新战役段：三架构 SMP 对等启动（aarch64 半 SMP 落地/riscv SMP
> 系列）+ 信号/notify wire 工作（minix-sef 双形态派发）+ 探针滚除战役（净删 2900 行）+ 续-413
> 评审回执轮九件事。深读两路提取 **44 条候选**（增量 26/提交消息+工具审查 18）。

### 13.1 增量语料（实测边界）

| 语料 | 规模 | 说明 |
|---|---|---|
| git 提交 ad14ced56..76c669b35 | 61 笔 | 续-385~427 |
| NK4C-WORKLOG delta | +579 行 | |
| TODO-3ARCH-PARITY-20261006.md | 429 行（首扫） | 三架构对等台账；**入库即 CRLF**（847 行假 diff 事故在本轮内已自纠，见 13.2-16） |
| R34 评审群 | 报告 161 + 对账表 62 | |
| misc_concepts 增量七 | 169→179 | SMP 方法教训主体已被其收录（2.29/2.30/3.67-3.71/4.18），本轮不重复立条 |
| 收尾批次对账 | — | 145 工件**仍未清理**（移交项销账形态持续第三轮）；MEMORY-CORRUPTION/TRANSIENT-PTE 档案终态同步在途（f24204aed） |
| 共享工具触碰 | tools/unsafe-audit.sh +20 | 续-413⑥ 门假红修复——修法正确但残余缺陷经审查实锤（13.2-1） |

### 13.2 新增模式（归并既有族；锚点 grep 实测）

**一、门与工具（本轮最重簇——「增量门工具也在被测面内」）：**

1. **「增量门读数随运行时工作树漂移」**（WORKLOG:13919 + 本轮审查实证）：unsafe-audit.sh 的 --diff
   分支按命中行号**读工作树**补前 5 行 CTX——对历史 RANGE 跑时，若那些行后来被增删，辩护会被推出
   窗（假红）或读进新内容（假绿）；已实证同一历史 RANGE 当日 bare=0、次日 bare=1。头注自报的局限
   写的是「读到新内容」，实际翻车的是反方向。**根治建议（移交 NK4-C）：双参 RANGE 时 CTX 改读
   `git show <新侧>:<file>`；`[ -f "$f" ] || continue` 对已删/改名文件静默跳过 CTX 也应打标记。**
2. **「总量 pin 对成员级越界免疫」**（WORKLOG:13978）：MessageUnion=72/Message=80 的尺寸 pin 与违规值
   同源演化——成员越界推高总量、pin 同步改写后恒绿，总量断言对逐成员越界结构性免疫（唯一读者是
   C 侧逐成员 _ASSERT_MSG_SIZE）。判据：尺寸断言必须逐成员钉，不只总量。
3. **「挂名未注册测试」**（b2d2c68b4）：test_sched_message_layouts 缺 #[test] 从不运行（编译器 never
   used 告警偶然暴露）；同族残留 test_nreqs_and_is_fs_rq_gate（fs_driver.rs:837）**实测仍缺**——
   测试名在执行位缺，覆盖假象。判据：测试构建 deny unused 或 CI 对 cfg(test) 内未注册 fn 计数清零。
4. **「门修复无门」**（WORKLOG:13567）：修假绿门的笔自身只有 bash -n 语法检查、无触发验证——吞退出码
   的修复逻辑零测试，正确性悬置于下次真实失败。
5. **「功能删除后测试名保留旧行为承诺」**（WORKLOG:14074）：sigset_to_u64 删掉后测试名仍叫
   round_trips_low_half 而已不做往返——名字成为对读者的假断言，当场更名。

**二、绿证据与基线：**

6. **「能力宣称缺专属机制标记」**（38514a26a）：atfsmp1 36/36 全绿但零 SMP 标记——dumpdtb 无 -smp
   使 DTB 单核、smp_init 短路，36/36 实为单核路径。判据：每个能力宣称必须有专属观测标记进门判据
   （如 ap-arrived hart=N）。
7. **「长期红基线期望反真源」**（WORKLOG:13743）：两个长期既有失败被登记「基线既有」容忍多轮——
   一查之下是测试期望方向与真源相反（红本身就是证据）。
8. **「回退宣告不核终态」**（WORKLOG:13683）：「smp.rs 接线已回退」实际未回退成功（cwd 漂移致
   checkout 静默失败），407 门跑的是带接线内核、401 门判定来自无接线旧内核——错误基线延续两轮
   污染两轮门读数。判据：状态变更类操作宣告成功前核终态（命令返回不等于状态改变）。
9. **「文本形状审计的提取器对格式敏感」**（同脸 N≥5：95638ac80/b2303fb6c/2a251000a/c8d7ba87d）：
   bitflags 双空格对齐致子串失配/尾随空格/{mask} 被当格式化插值/审计窗口切到下一 handler 把别家
   语句扫进来——首版失败多数是提取器自身鲁棒性坑。判据：锚定正则+空白归一+金丝雀自测进提取器模板。
10. **「脚本批量删除啃进产品语义」**（6e2a34e76）：净删 2900 行的探针滚除，脚本三处吞掉产品语句
    （TOCTOU 拷贝段/send_blocking/孤儿 cfg）——探针与产品语句同块时脚本删除不可用，逐处手工+每步
    编译+宿主测试是下限。
11. **「移除类改动的波及面缺 cfg(test) 桩」**（29d43e72f）：删 trait 死契约面漏了软件模拟器 impl，
    cargo test -p minix-vm --lib 自那轮起编译失败——移除笔判据必须含 --workspace --tests 或
    「凡实现该 trait 的模拟实现」清单。
12. **「同坑多站不继承教训」**（WORKLOG:14085/14089）：impl FnMut vs dyn FnMut 的 E0277 四轮各撞
    一次——教训写在旧现场注释，每站新包装层不继承；教训载体错位于现场而非模式库。
13. **「台账预设修法未经验证入库」**（WORKLOG:13647）：TODO 甲案（摘除 default=mock）经现读判为
    不成立——会整体消灭宿主测试面；修法栏与现状栏同样会过期，预设修法须现场验证后才入台账。

**三、语义与协议：**

14. **「忠实片段组合死锁」**（WORKLOG:14010）：两个各自忠实 C 的局部语义（吞帧 continue+回调不可
    exit）在缺失第三原语（sef_cancel 逃生门）时组合成通电即死锁——逐段对齐不保证系统级安全。
15. **「假注册面陈述正当化缺口」**（WORKLOG:14058/14080）：「C 也没有 FS 驱动注册信号 handler」是
    可被 grep 证伪的假陈述，却给四个恒 false 空钩子盖上「本来就该如此」的牌子，且两度晋升
    （代码 doc→交接件顶块）。
16. **「协议臂单侧建模」**（WORKLOG:14093）：SEF_INIT_* 发送半已建模、接收半零消费——通电时语义
    静默反转（按指示崩溃被读成 fresh 正常启动）。
17. **「旁证当守卫」**（2ba6887ee，续-420 自我否证）：拿「内核恒写 NOTIFY_MESSAGE」旁证给
    notify_sigset 加防御断言，被自己合法夹具打回后两轮撤销——守卫前提必须与被保护事实同义
    （真前提在 status 字不在 m_type）。
18. **「CRLF 文件遭文本模式整读写刷行尾」**（98ec6f4a3）：入库即 CRLF 的台账被文本模式整读写刷成
    LF，真实 11 行改动放大成 847 行假 diff——已按二进制换回。判据：.gitattributes 标行尾或编辑后
    eol 对账进验证串。
19. **「教训登记与门固化之间的复发窗」**（38514a26a）：「拓扑真值=DTB、dump 须 -smp」教训先文字
    登记，缺强制点则原样复发一次后才固化成门注释——教训条目应带「下次由哪道门拦截」字段，无
    强制点不收账。
20. **「元产物不豁免」**（47270b3f7/e5c030d5d）：承载规则的交接件/WORKLOG 自身撞文风门 N≥3——
    元文档默认不过门的惯例是漏洞。
21. **「长寿命在制跨 N 笔未入库」**（88e98e49f）：改动压在工作树跨 22 笔提交，期间任何批量操作可毁
    可污——在制台账化+时效上限。
22. **「章首摘要不随正文同步」**（e71aab064）：正文改两层→三层而第 5 行章首摘要没跟——同文档前后
    矛盾；关键断言的文档内镜像位清单（章首摘要/顶块/台账行）。
23. **「蒸馏文档增量编辑结构损伤」**（misc_concepts:14）：增量重写吞掉条目标题/留游离重复裸标题，
    跨两轮未察觉——引用与检索失准。
24. **「消费方计数把注释当调用点」**（475450052）：6→14→13 三次改口，14 是把注释行当调用点——
    计数只认代码符号位并附枚举清单。
25. **「以未触碰推定替代实跑写进提交」**（3ca5fd8ca）：「rustfmt 零漂移」当时为假（漂移系前笔遗留）
    ——「我没改」推不出「它是干净的」，commit message 是不可变宣告面。

### 13.3 机械化判定

| 变化 | 内容 |
|---|---|
| P2_TESTS +11 | minix-sef 信号臂家族：run_once ×3+signal_term ×2（4bb3539d4）、effective_kernel_signal（78b277a0d）、get_work ×3（811cf7ff4）、init_proc_nr+kernel_signal 窗口边界 ×2（b2d2c68b4——「无消费方新面挂归属」判例的窗口测试）——全部 #[test]+出处钉准（87/87）；变异改名即 FAIL |
| 基线收编/消解 | P12 +boot-shim 新内容键（续-395 端口勘误 0x3f9→0x3fd 修 SD-5，同一条发射循环的合法内容变更，人工复核）；-lib.rs sfence 键（探针滚除战役已删该探针——正确的消解）；-P7×3（run_all.sh 吞退出码清理落地——P7 自出生以来的存量清零） |
| 无新 P 检查 | 「逐成员尺寸断言」「deny unused 测试」「CTX 读 git show」均属 NK4-C 线自己的修复面（红线已写在案/工具归其所有）；「能力标记进门判据」属门判据设计纪律归报告 |
| P17 维持 REPORT | 145 工件第三轮未清理 |

### 13.4 复发确认（要点）

「增量门假红要修门不是绕门」（教训升级：门的判定是运行时工作树的函数）；时态即断言；单类目对账写全量零新增；空转断言；配对参数分叉假绿（dumpdtb 缺 -smp 二犯后固化注释）；同 agent 独立评审降级（R34 J3）；扫描快照过期（trait 必需成员误判死代码）；文档测试计数三态失真（44/47、42/135）。

### 13.5 正面沉淀（§5 补充）

「拿不准不顺手删，登记待裁决」（flush_count 处置）；无消费方新面强制挂 C 侧读者+接入批次；勘误就地保留取证历史（本笔两度执行）；「教训条目带下次由哪道门拦截」字段提案；TRANSIENT-PTE 第 11 章三态结案对账+资产可用性未复证标注（终局档案的范本节）。

### 13.6 语料划界更新

- 收尾批次余量收窄至一项：145 工件清理（P17 持续计数）；档案终态已开始入库（f24204aed）。
- unsafe-audit.sh 残余缺陷（CTX 读工作树 vs RANGE 新侧）移交 NK4-C（13.2-1 含根治建议）；
  test_nreqs_and_is_fs_rq_gate 缺 #[test] 残留同批（13.2-3 实测仍在）。
- 后续触发条件照旧：SMP 战役推进 + 收尾批次最终落定。
