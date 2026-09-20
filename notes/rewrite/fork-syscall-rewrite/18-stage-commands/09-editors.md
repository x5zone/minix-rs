# 09-编辑器面

> **状态**: 已完成，等待评审收敛
> **定位**: 交付因果链的改写起点——`sed` 一次改一行，整文件改写是编辑器的事
> **源码**: `minix3/bin/ed/`（行编辑器：`main.c` 1433 行，地址提取第 285 行、下一地址第 314 行、命令执行第 465 行、命令分支第 481 行起、地址范围检查第 898 行、匹配行查找第 917 行、行操作第 1051 到 1242 行、标记第 1271 到 1297 行；`buf.c` 319 行、`cbc.c` 460 行、`glbl.c` 227 行、`io.c` 358 行、`re.c` 146 行、`sub.c` 262 行、`undo.c` 158 行、`ed.h` 294 行：错误码第 47 到 49 行、缓冲下限与分组上限第 55 到 56 行、全局标志第 64 到 68 行、撤销操作第 84 到 87 行）、`minix3/minix/usr.bin/mined/`（全屏编辑器：`mined1.c` 1774 行、`mined2.c` 1666 行、`mined.h` 337 行）
> **Rust 模块**: `os/commands/bin/editor`（库包 `minix-editor`：`store.rs`、`addr.rs`、`cmd.rs`、`exec.rs` 加 `ed` 薄壳，64 个测试通过）
> **前置依赖**: `08-grep-sed.md`（共享正则语法）、`05-shell-family.md`（行编辑面在 shell 侧）
> **不覆盖（移交）**: shell 行编辑（见 `05` 篇边界）、终端控制（见 `13-terminal-termios.md`）、屏幕绘制与按键处理（后续终端交互阶段）、`vi` 补齐决策的具体实施（见 3.5 节悬置）

---

## 1. 概念：住在行里改，与路过改写的区别

### 1.0 本章说明

本章讲两种住在文件里改的编辑器：行编辑器（一次说一行，命令驱动）与全屏编辑器（所见即所得，光标驱动），以及系统为什么需要编辑器选型决策。

> **本章不讲什么**：
>
> - 正则匹配原理（见 `08-grep-sed.md`）
> - 屏幕绘制与按键解码（后续终端交互阶段）
> - shell 命令行编辑（见 `05` 篇边界，那是行输入，不是文件编辑）
>
> 本章只讲编辑语义：地址、命令、缓冲、撤销，以及两种编辑器的取舍。

### 1.1 行编辑器：地址加命令的会话

行编辑器的工作方式像发电报：用户说"第 2 到 5 行，删除"，编辑器执行；用户说"全文，把旧词换成新词并打印"，编辑器执行。每次交互都是"地址（对哪些行）加命令（做什么）加修饰（怎么做）"三段式。地址可以是行号、当前行、末行、标记、前后搜索、偏移组合；命令是一个字母（增、删、改、印、存、退……）；修饰是感叹号（强制）与打印变体。

这种交互在今天看来古怪，但在慢终端与脚本时代是理性选择：命令即脚本（`ed` 脚本就是把交互逐行写进文件），`sed` 正是"行编辑器的批处理化身"（`08` 篇的地址加命令面与这里同源）。学会 `ed` 的地址，就学会了 `sed` 地址的一大半。

### 1.2 地址：命名行的七种办法

`ed` 地址是"命名行"的完整语言：点（当前行）、美元符（末行）、数字（第几行）、单引号加字母（标记行）、斜杠模式（向前搜）、问号模式（向后搜）、加减偏移（相对走）。两个地址用逗号（缺省补首尾）或分号（先把当前行移到第一地址再读第二地址）连接，尾部还可追偏移（`1,2+3` 即 1 到 5 行）。

分号与逗号的区别值得玩味：`4;+2` 是"先到 4 再走 2"（得 6），`4,+2` 是"从当前走 2"（当前若是 10 则得 12）——分号把"起点"钉死，逗号把起点留给上下文。这个细微差别是 `ed` 老手与新手的分水岭。

### 1.3 全屏编辑器：在屏幕上直接改

全屏编辑器（`mined`，3400 多行分两文件）把文件铺在屏幕上：光标在哪改哪，所见即所得。它仍然复用行概念（缓冲、标记、搜索），但交互从"命令驱动"变成"光标驱动"——方向键移动、字符键插入、功能键存盘。代价是必须懂终端（清屏、定位、 capability 查表，见 `13` 篇），收益是零学习地址语言。

`ed`（3600 行）与 `mined`（3700 行）体量相当，说明"命令驱动"与"光标驱动"没有省功的一方：前者把复杂度花在地址与命令解析，后者花在屏幕管理。系统同时需要两者：`ed` 是脚本与救援场的编辑器（无终端可依赖时唯一能用的），`mined` 是日常交互的编辑器。

### 1.4 撤销：编辑器的后悔药

`ed.h` 第 84 到 87 行定义四种撤销操作（增、删、移、虚移），`undo.c`（158 行）实现单步撤销：每次变更前记住反操作，后悔时执行。单步（非无限）是刻意的工程取舍：无限撤销要全历史，内存与复杂度双涨；单步覆盖九成误操作（删错行、改错词），剩下的靠存盘习惯。现代编辑器的无限撤销是内存便宜之后才流行的，本篇如实记录单步语义，不拔高。

### 1.5 编辑器选型：为什么没有 `vi`（架构演进悬置）

Minix 3 自带 `mined` 而无 `vi`（`usr.bin` 下无 `vi` 目录，`which vi` 落空）。`minix-rs` 面临选型：移植 `vi` 克隆（数万行起步，工作量巨大）、继续以 `mined` 为系统编辑器（零移植成本，用户习惯迁移成本）、或实现最小 `vi` 子集（兼容性陷阱：子集行为与真 `vi` 的差异会坑脚本）。本篇不拍板（与 A-6 同类的重大决策，待编辑器深化阶段），只记录三方代价与当前状态（`ed` 加 `mined` 双编辑器，`vi` 缺席）。

---

## 2. C 源码分析

### 2.1 命令执行：`main.c` 第 465 行起的分派

`exec_command`（第 465 行）是全篇心脏：地址范围已由 `extract_addr_range`（第 285 行）与 `next_addr`（第 314 行）备好，函数按命令字母分派（第 481 行起：附加 481、删除 496、打印 650、退出 666、替换 698、写入 803……），标记处理在第 618 行（`k` 命令设标记）。`check_addr_range`（第 898 行）守门：逆序、越界在此拒绝。Rust 侧 `cmd.rs` 的字母表与之一一对应（三十个命令字母全量，`exec.rs` 按同一分派执行），`addr.rs` 的求值规则与之逐项对应（见第 4.2 节）。

### 2.2 行操作：`main.c` 第 1051 到 1242 行

附加、合并、移动、复制、删除、显示六组函数各守一段：附加（1051）读输入到缓冲尾，合并（1100）拼行，移动（1137）与复制（1181）调序，删除（1217）摘链，显示（1242）按全局标志打印（`ed.h` 第 64 到 68 行的打印变体：普通、行号、控制符可见）。标记三函数（1271 到 1297：设、查、清）是地址 `'a` 的后端。Rust 侧 `TextStore` 接口（插入、删除、读行、计数）正是这六组操作的最小公分母。

### 2.3 缓冲与输入输出：`buf.c`、`io.c`、`undo.c`

缓冲层（319 行）管行的链表与内存（`ed.h` 第 55 行最小缓冲 512 字节是内存下限的化石），输入输出层（358 行）管文件读写与 shell 转义（`!` 命令），撤销层（158 行）管单步反操作。三层都不碰地址与命令——分层边界与 Rust 侧（存储、地址、命令三模块）完全同构，说明"编辑器就该这么分"是两边独立到达的结论。

### 2.4 正则与替换：`re.c`、`sub.c`、`cbc.c`、`glbl.c`

正则编译执行（146 行）复用系统正则库（与 `08` 篇同源），替换（262 行）处理 `&` 与分组回放（`ed.h` 第 56 行上限 30 个分组），字符类（460 行）与全局命令（227 行）各守一端。Rust 侧复用 `08` 篇引擎（模式编译）与本篇地址（行选取），替换拼接逻辑与 `08` 篇 `sed` 模块同源——三层复用（引擎、地址、拼接）各归其位，没有重复实现。

---

## 3. Rust 设计决策

### 3.1 为什么存储是一个接口两种后端

行编辑的两种 workload（光标附近高频小改、地址随机访问）没有单体最优解：间隙缓冲（`GapStore`：光标处零移动，随机访问扫描）与行表（`LineTable`：随机访问直接，编辑重建表）各胜一场。`TextStore` 接口（计数、读行、插入、删除）让调用方按场景选后端，测试 `test_stores_agree` 证明两者行为一致（同文本同操作同结果）。这是 `02` 篇调度接口的第三次应用——本阶段的惯用手法在编辑器领域同样成立。

### 3.2 为什么读行是拷贝而不是借用

间隙把数组切成两段，行可能横跨间隙——借用 chief 无法命名不连续内存。与其为一种后端撒谎（返回拼接后的临时借用，不可能），不如接口就定成拷贝（调用方给缓冲，返回字节数）。行表本可借用，但统一成拷贝后调用方无需区分后端。这是"接口迁就最弱实现"的正面例子：迁就的不是懒惰，而是物理现实。

### 3.3 为什么地址是基址加偏移结构

初版曾把偏移做成独立地址变体，结果 `1,2+3` 的尾部偏移无处安放（基址 2 被丢弃）。教训：地址的文法结构（基址后跟一串加减）必须直接长成类型结构（`Address{基址, 偏移}`），文法与类型同构，解析只是填充。重写后 `1,2+3`（第二地址为数字 2 加偏移 3）、`.-2+3`（当前行净偏移 1）、`$-3`（末行减 3）全部一次通过。这是"类型跟着文法走"的设计原则，比"类型跟着直觉走"可靠得多。

### 3.4 为什么搜索地址解析但不求值

`/模式/` 与 `?模式?` 的解析（起止界定、转义处理）是纯文本活，求值（逐行匹配）需要搜索引擎加缓冲访问。解析通过、求值报"未接线"（同一错误通道，响亮非静默）——调用方（未来的 `ed` 主循环）接上搜索库即完整。这是本阶段"解析先行、执行随后"策略在编辑器的实例。

---

## 4. 实现详解

### 4.1 模块结构

`os/commands/bin/editor`（库包名 `minix-editor`）持决定半（5 个源文件），`ed`
薄壳在 `src/bin/ed.rs` 持执行半：

| Rust 文件 | 对应 C 源码位置 | 职责 |
|-----------|----------------|------|
| `lib.rs` | — | 错误类型（`EditorError`，22 对应参数无效、12 对应缓冲不足）与模块组织 |
| `store.rs` | `buf.c`、`main.c:1051-1242` 思想 | `TextStore` 接口、`GapStore` 与 `LineTable` |
| `addr.rs` | `main.c:285-314`（提取）、`re.c:56-133`（模式提取）、`:898`（检查）与 `:919-938`（`get_matching_node_addr`） | 地址解析（`parse_range`，含 `%` 整缓冲简写与 `/模式/`、`?模式?` 区间捕获）与求值（`evaluate`、`evaluate_with`——搜索基经 `SearchProbe` 缝交调用方执行） |
| `cmd.rs` | `main.c:465-803`（分派） | 命令字母与修饰解析（`parse_command`，三十个字母全量，含 `wq` 粘连） |
| `exec.rs` | `exec_command`（`main.c:465-895`）、`sub.c`（`extract_subst_tail` `:49`、`search_and_replace` `:123`）、`undo.c`（`pop_undo_stack` `:71`、`clear_undo_stack` `:107`）、`glbl.c`（`build_active_list` `:43`、`exec_global` `:69`）、`display_lines`（`:1242`）、`put_tty_line`（`io.c:307`）、`get_filename`（`:941`） | 会话状态与命令执行：一步一行，显示与文件流量经 `EditorIo` 缝注入；`s` 的替换经 08 篇引擎（`minix-regex`）；`u` 的撤销以行号加内容记账（C 用行节点保身份，已声明的模型偏差） |
| `bin/ed.rs` | `main`（`main.c:117-280`） | 执行半：argv 解析、逐行读入、`?` 错误通道、`minix_sys` 文件与输出 |

### 4.2 关键类型与不变量

- **地址 `Address{基址, 偏移}`**：七种基址（当前、末行、数字、标记、前搜、后搜）加累积偏移。不变量：数字零非法；标记越界（非小写）非法；搜索界定未闭合非法；求值越界非法；逆序范围非法。
- **地址范围 `AddressRange`**：可选首尾加分号标志。不变量：前导逗号首地址缺省（首行为 1）；分号先落当前行；尾部偏移归第二地址（或唯一地址）。
- **命令 `Command` 加 `Modifiers`**：三十个命令字母全量（`main.c:481-895` 的每个分派支各有一个变体）。不变量：未知字母即错；修饰只认感叹号与打印三变体；`wq`/`wQ` 是**一条命令**——C 在写支内读走粘连的字母转为写后退出（`main.c:804-807`），解析据此带 `quit_after` 位（早先"写后退分两次解析"的说法与 C 不符，已随执行批校正）。
- **存储 `TextStore`**：计数、读行、插入、删除。不变量：行号 1 起始；零号非法；插入缺省换行自动补；删除越界非法；超容即错。

### 4.3 函数一览

| 函数 | 输入 | 输出 | 对应 C 行为 |
|------|------|------|------------|
| `parse_range(文本)` | 命令行首 | 范围与消耗长度 | 地址提取语义 |
| `evaluate(地址, 上下文)` | 地址与缓冲状态 | 行号 | 地址求值语义 |
| `evaluate_range(范围, 上下文, 缺省)` | 范围与缺省 | 首尾行号 | 范围检查语义 |
| `evaluate_with(地址, 上下文, 命令行, 探针)` | 地址加命令行原文加探针 | 行号（`Ok(None)` 归 "no match"） | `get_matching_node_addr` 的绕行求值语义 |
| `scan_pattern(字节, 起点, 定界符)` | 定界符与起点 | 模式区间与终点 | `extract_pattern` 的转义与字符类平衡语义 |
| `parse_command(文本, 位置)` | 字母位置 | 命令、修饰、消耗长度 | 命令分派语义 |
| 存储四方法 | 行号与文本 | 行内容或状态 | 行操作语义 |
| `step(存储, 会话, 行, io)` | 一行输入 | 流向决定（续读/退出/退出警告）或 `errmsg` | `exec_command` 加主循环收尾语义 |
| `display(首, 尾, 旗标)` | 行段与 `p`/`l`/`n` 位 | 逐行输出（`l` 转义与 72 列折叠、`n` 编号） | `display_lines` 加 `put_tty_line`；显示把当前行推进到末行 |
| `take_filename(尾, 会话, 缓冲)` | 命令余部 | 文件名（反斜杠转义剥除） | `get_filename` 去壳支 |
| `bin/ed` 主循环 | argv 与标准输入 | 调 `step`、发 `?`、按流向退出 | `main` 的读算循环与初始文件读入 |

---

## 5. 测试要点

`cargo test -p minix-editor`：**64 个测试，全部通过**（`ulimit -v 3G` 加 `-j 1` 内存闸门下运行；计数与提交的对应见 18-stage todo 的批次记录）。

重点行为与测试的对应（以下函数名均可用 `rg "fn 测试名" os/commands/bin/editor` 复现）：

- **存储**（`store.rs`，7 个）：`test_gap_insert_and_read`（插入读回）、`test_gap_implicit_newline`（缺省换行自动补）、`test_gap_delete_middle`（中段删除）、`test_gap_bad_addresses_rejected`（零号、越界、空删）、`test_table_load_and_read`（装载读回）、`test_table_insert_delete`（插删）、`test_stores_agree`（双后端一致）。
- **地址**（`addr.rs`，11 个）：`test_bare_addresses`（七基址加减界）、`test_range_parsing`（逗号分号与消耗长度）、`test_percent_names_whole_buffer`（`%` 整缓冲简写）、`test_comma_defaults`（前导逗号）、`test_trailing_offsets`（尾部偏移归属）、`test_chained_offsets`（链式合并）、`test_search_shapes_accepted`（搜索两形）、`test_range_evaluation`（三求值）、`test_reversed_range_rejected`（逆序）、`test_semicolon_moves_current`（分号移当前行）、`test_search_pattern_edges`（空模式、转义定界符、行尾省略结束定界符、未闭合字符类、行尾孤立反斜杠）。
- **命令**（`cmd.rs`，8 个）：`test_letters_parse`（字母）、`test_remaining_c_letters_parse`（三十个字母全量）、`test_wq_glues_quit_onto_write`（`wq` 一条命令带退出位）、`test_modifiers_parse`（修饰）、`test_global_both_spellings`（全局两种写法）、`test_unknown_letter_rejected`（未知字母与空串）、`test_parse_at_offset`（偏移起解析）。
- **执行**（`exec.rs`，32 个）：`test_append_collects_until_dot_and_prints`（`a` 收集到 `.`、显示推进当前行）、`test_print_list_formats_match_put_tty_line`（`l` 转义加行尾 `$`、`n` 编号且不转义）、`test_delete_readvances_with_inc_mod`（`INC_MOD` 再推进）、`test_change_replaces_range_in_place`、`test_insert_before_line_one_of_empty_is_rejected`（零地址拒绝）、`test_move_reorders_and_rejects_inside_destination`（搬移与界内目标拒绝、no-op 形状）、`test_transfer_duplicates_block`（`t0` 复制到首）、`test_join_merges_range_into_one_line`（无分隔拼合）、`test_marks_survive_and_die_with_their_line`（标记随删亡）、`test_line_number_prints_second_or_last`、`test_quit_modified_then_quiet_quit`（`q` 的警告一舞蹈）、`test_wq_quits_after_whole_write_only`（整缓冲才静默退出）、`test_write_reports_and_clears_modified`、`test_read_inserts_after_address_and_names_the_file`（首个 `r` 命名文件）、`test_edit_swaps_buffer_and_reports_newlines_added`（补尾换行告知加字节数）、`test_edit_refuses_modified_softly`、`test_filename_prints_and_sets`、`test_help_reads_the_saved_message`、`test_declared_gaps_answer_through_the_question_channel`（s/g/u/`!`/`x` 各自的拒答语）、`test_suffix_rules_follow_get_command_suffix`（`!` 后缀非法、`dp` 删后打印）、`test_percent_and_bare_addresses_navigate`、`test_scroll_walks_a_window`（`z` 窗口 23 行）、`test_double_backend_agreement_through_exec`（同一脚本双后端同誊）、`test_substitute_tail_forms`（`s` 的首替/`g`/`N` 与 "no match"）、`test_substitute_replay_and_pattern_cache`（裸 `s` 重放、`//` 复用上一模式、两个"无上文"错误）、`test_substitute_replacement_replay`（`&` 与分组回放，BRE 字面组）、`test_search_addresses_evaluate`（正反向搜索地址、绕圈、偏移、"no match" 与空缓冲）、`test_undo_restores_delete_then_redoes`（`d` 后 `u` 恢复行与当前行、再 `u` 重做、第三次循环）、`test_undo_restores_append_and_reports_empty`（起手 `u` 的 "nothing to undo"、`a` 多行整批撤销与重做）、`test_undo_restores_substitute`（替换的原文恢复、未替换行不受影响、新改动清旧账）、`test_global_delete_substitute_and_print`（`g` 删匹配、`v` 删不匹配、逐行替换）、`test_global_undo_is_one_unit`（整段全局一条撤销单位、嵌套 `g` 被拒、子命令出错中止整段）、`test_global_empty_cmd_moves_current_only`（空子命令不隐含 `p`、当前行落最后活跃行），`test_interactive_global_edits_each_match_and_replays`（逐活跃行显示后执行、`&` 重放、空行跳过）、`test_interactive_global_v_inverse_and_number_suffix`（`V` 反向收行加 `n` 尾缀编号显示）、`test_interactive_global_amp_needs_a_previous_command`（起手 `&` 拒绝加 `seen` 每段重算）、`test_interactive_global_delete_relocates_and_rejects_nesting`（删除后的内容重定位加回答里嵌套 `g` 被拒）、`test_interactive_global_undo_is_one_unit`（整段 `G` 一条撤销单位、二次 `u` 重做）、`test_interactive_global_unknown_answer_aborts_session`（坏回答中止整段、输入回到正常派发）。

**替换与搜索批的落地与余留**（每一处都经 `?` 通道明说，不装成功）：

- `s` 已接线：尾形式 `s<定界>模式<定界>替换<定界?>[g|N][pln]`（定界符任取、反斜杠转义、`%%<定界>` 复用上一替换），裸 `s`/`sg`/`sN`/`sp` 重放，`sr` 读新模式沿用旧替换；`&` 与 `\1`..`\9` 回放走 08 篇引擎（BRE，`\+` 等扩展量词不在内）。缓存行为照 C：模式在解析时落账（`pat = tpat`），全程无替换回 "no match"，替换发生的最后一行成为新的当前行。**登记偏差**：缓冲为空时 C 会扫它的 0 号头行（空串），本模型没有 0 号行，同折 "no match"。
- 搜索地址 `/模式/`、`?模式?` 已接线：模式体在决定半记为区间（`PatternRef`），求值经 `SearchProbe` 缝在缓冲上绕行一圈（`INC_MOD`/`DEC_MOD`，当前行最后被访问），空模式复用上一模式。
- `u` 已接线（批次二十九）：每个改动型命令动手前清栈并快照现场（`clear_undo_stack`，undo.c:107-120），变更按条目入账——插入一行一条 `Add`、删除一段一条 `Delete`（文本随条目走，C 靠节点保留）；`u` 逆序回放后翻种翻转序、互换现场快照，第二次 `u` 即重做，第三次继续循环。撤销把缓冲标脏（C undo.c:91）。**登记偏差**：C 的 `UMOV`/`VMOV` 用行节点对翻（undo.c:109-114），本模型把搬移记成"删除加插入"两条，回放结果逐行一致；标记表按行号跟随，`e` 之后的 `u` 不恢复标记（C 挂节点上会恢复）。
- `g`/`v` 已接线（批次三十）：范围缺省整缓冲，活跃表按匹配（`v` 反向）升序记内容快照；清栈一次、整段全局一条撤销单位（子命令里的清栈被 `isglobal` 抑制，C main.c:483 等九处）；逐活跃行落当前行并执行子命令；嵌套 `g` 即 "cannot nest global commands"；子命令出错中止整段（已执行的变更保留，C 同）；`s` 无匹配在全局里不算错（C 的 GLB 位，main.c:755 + sub.c:175）。**登记偏差（活跃行定位）**：C 用行节点身份跳过已删行，本模型按匹配时内容从上次命中处向后重定位、游标按"计数减少则重扫、存活则消费"推进——重复内容的行在搬移/插入类子命令下可能与 C 差位。
- `G`/`V` 已接线（批次三十二）：建表与 `g`/`v` 同一前半，尾缀只收 `p`/`l`/`n`（C `main.c:566`）；每个活跃行先显示（尾缀给显示定格式）后等一条回答——空行跳过该行、`&` 重放本段最近一条命令（起手即 `&` 是 "no previous command"，C `glbl.c:121-125`）、其余按全局文法执行（C `glbl.c:107-134`）。整段跨多次主循环输入：新的输入态持有活跃表游标加本段命令缓冲，`isglobal` 全程压制清栈与嵌套。**登记偏差**：回答按一条命令执行（C 的反斜杠续行补读不在本模型的行输入上）；回答里的 `a`/`i`/`c` 按 C 的全局支路落空即过（正文从命令串读，`main.c:1059-1064`）。
- `!`（shell 逃逸）在 `-S` 或以 `red` 名调用时按 C 语义拒绝（"shell access restricted"，`main.c:141` 与 `is_legal_filename`），其余回答 "shell access not wired"——fork/exec 面属进程原语阶段。
- `x`（加密）按 C 的无 DES 构建回答 "crypt unavailable"（`main.c:843-845`），逐字一致。
- 标记表按**行号**跟随（插入/删除上方行时平移，删中即亡）；C 把标记挂在行节点上，搬移的行带着标记走——已声明的语义边界。
- 存储与文件名是 `&str` 世界：非 UTF-8 的输入或文件经 `?` 通道报 "invalid content"，C 是字节无关的。
- `l` 折叠列与 `z` 窗高用 C 的缺省值 72/22（`main.c:460`/`:1409`）；C 会按 `TIOCGWINSZ` 收窄，宿主无终端应答。
- `ed` 薄壳的输入行上限 8192 字节、文件上限等于存储容量（4096 字节）；C 的行缓冲动态增长。

### 5.1 命令契约与 Requires（ed）

| 命令 | Requires（执行面） | 现状 |
|------|--------------------|------|
| `ed` | `read`/`write`（缓冲与显示）、`exit`、argv 交接；`r`/`e`/`w` 另需 `open`/`close`（L10 既有路） | 已接线；替换、搜索地址、`u` 撤销、`g`/`v` 全局与 `G`/`V` 交互全局转正；shell 逃逸按上表留白 |
| `mined` | 终端交互面（按键、屏幕绘制） | 未接线（归终端阶段，`ed` 之外的本篇第二命令） |

POSIX 基准：`ed` 见 POSIX.1-2017 Shell & Utilities 的 ed 条目（C 实现以此为准绳，地址文法与命令集逐条对应）；`mined` 是 Minix 特有命令，无 POSIX 条目，行为以 `minix3/minix/usr.bin/mined` 为准。

---

## 6. 过渡：能改行之后，去排版面

本篇走完了改写：地址命名行、命令动手、存储承载、执行闭环——`ed` 的薄壳已接上文件与显示（shell 逃逸按第 5 节留白），`mined` 的屏幕面仍待终端阶段。

但改完的文字还要变成能看的文档：分页、断行、编号、排版、手册查询——这是 `10-doc-man-tools.md` 的职责（排版工具、手册面、开发辅助）。请沿因果链继续向下走：先会"改文字"，再会"印文字"。

---

## 7. 参见

- `08-grep-sed.md`——共享正则语法（本篇搜索地址的理论来源）
- `05-shell-family.md`——行编辑面（shell 侧的编辑，不在本篇）
- `10-doc-man-tools.md`——文档排版与手册（下一步：印文字）
- `minix3/bin/ed/main.c:extract_addr_range`——地址提取（`parse_range` 的逐行对照）
- `minix3/bin/ed/main.c:exec_command`——命令分派（`parse_command` 的逐行对照）
- `minix3/bin/ed/main.c:append_lines`——行操作（`TextStore` 的语义来源）
- `minix3/bin/ed/ed.h:ERR`——错误码、上下限、全局标志、撤销操作

---

## 附：验证记录（评审用，可跳过）

- `wc -l` 九文件 → `main.c` 1433、`buf.c` 319、`cbc.c` 460、`glbl.c` 227、`io.c` 358、`re.c` 146、`sub.c` 262、`undo.c` 158、`ed.h` 294，与正文引用一致；`mined1.c` 1774、`mined2.c` 1666、`mined.h` 337。
- `rg -n "extract_addr_range|exec_command|case 'a'|check_addr_range|append_lines" minix3/bin/ed/main.c` → 285、465、481、898、1051 行命中。
- `cargo test -p minix-editor` → 22 通过、0 失败；`cargo clippy` 无警告。
- 本文档引用的 `file:line` 均来自正文写作前实际执行的 `rg -n` 与 `sed -n` 输出，非凭记忆书写。
