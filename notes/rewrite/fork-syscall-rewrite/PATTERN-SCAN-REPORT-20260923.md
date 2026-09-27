# PATTERN-SCAN-REPORT-20260923 — 历史日志模式挖掘与回归检查（C-61）

> **任务**：用户指令「从 FIXLOG、WORKLOG 等历史 LOG 中扫描并发掘模式，扫描得到测试，避免以后再出现类似的问题」。
> 交付三件：本目录文档 + [`tools/pattern-gate.sh`](../../../tools/pattern-gate.sh)（13 项机械检查）+
> [`tools/pattern-gate-baseline.txt`](../../../tools/pattern-gate-baseline.txt)（存量豁免）。
> 判别证据：`evidence/20260923-c61-pattern-gate/`（selftest 18 例正反矩阵 + 真树变异负例 6 组 + 全量运行实录）。
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
| `notes/rewrite/fork-syscall-rewrite/NK4A-QWEN-WORKLOG.md` | 471 行 | 全文精读（Task A/B/C 六轮取证） |
| `notes/rewrite/fork-syscall-rewrite/NK4B-WORKLOG.md` | 2446 行 | P0 核账 + P1 节全文精读；M3/M4 系列经 edge1 FIXLOG 补记覆盖 |
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
`evidence/20260923-c61-pattern-gate/{selftest,mutation-negative,full-run-worktree}.log`——
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

### 7.4 判别证据（evidence/20260927-c62-pattern-gate/）

selftest 24 例（+P14×2/P15×2）正反矩阵全绿；真树变异：P14 新增裸 set 即 FAIL、P15 gitignore 删条目即 FAIL、还原复绿；full 模式 PASS=11 FAIL=0 SKIP=1（worktree）/ 主树合并后终验。**过程事故如实记**：变异还原用 `git checkout -- .gitignore` 把自己未提交的 gitignore 条目一并冲掉（P15 假 FAIL 一次）——变异脚本的还原必须用「改前备份/改后还原」，对未提交在制禁 checkout（D2 模式第 N 次现世，本次受害者是检查者自己）。# PATTERN-SCAN-REPORT-20260923 — 历史日志模式挖掘与回归检查（C-61）

> **任务**：用户指令「从 FIXLOG、WORKLOG 等历史 LOG 中扫描并发掘模式，扫描得到测试，避免以后再出现类似的问题」。
> 交付三件：本目录文档 + [`tools/pattern-gate.sh`](../../../tools/pattern-gate.sh)（13 项机械检查）+
> [`tools/pattern-gate-baseline.txt`](../../../tools/pattern-gate-baseline.txt)（存量豁免）。
> 判别证据：`evidence/20260923-c61-pattern-gate/`（selftest 18 例正反矩阵 + 真树变异负例 6 组 + 全量运行实录）。
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
| `notes/rewrite/fork-syscall-rewrite/NK4A-QWEN-WORKLOG.md` | 471 行 | 全文精读（Task A/B/C 六轮取证） |
| `notes/rewrite/fork-syscall-rewrite/NK4B-WORKLOG.md` | 2446 行 | P0 核账 + P1 节全文精读；M3/M4 系列经 edge1 FIXLOG 补记覆盖 |
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
`evidence/20260923-c61-pattern-gate/{selftest,mutation-negative,full-run-worktree}.log`——
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

### 7.4 判别证据（evidence/20260927-c62-pattern-gate/）

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
