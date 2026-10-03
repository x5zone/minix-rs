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
