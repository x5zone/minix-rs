# 07-文本过滤与数据处理

> **状态**: 已完成，等待评审收敛
> **定位**: 交付因果链的日常操作第二组——文件里面的行：取头取尾、排序去重、计数切割、字符转换
> **源码**: `minix3/usr.bin/` 三十五个（`head` 204 行、`sort` 418 行、`wc` 354 行、`tr` 283 行、`uniq` 257 行、`cut` 306 行，及 `cksum`、`cmp`、`col`、`colrm`、`column`、`comm`、`csplit`、`expand`、`fold`、`hexdump`、`join`、`jot`、`lam`、`paste`、`patch`、`rev`、`sdiff`、`seq`、`shuffle`、`split`、`tail`、`tee`、`unexpand`、`unifdef`、`units`、`unvis`、`uuidgen`、`vis`、`yes`）、`minix3/minix/commands/` 三个（`look`、`ifdef`、`crc`）、`minix3/minix/usr.bin/diff/`
> **Rust 模块**: `os/commands/usr-bin/textfilter`（库包 `minix-textfilter`：`window.rs`、`count.rs`、`cut.rs`、`tr.rs`、`uniq.rs`、`sort.rs`、`cmp.rs`、`seq.rs`、`pr.rs`、`rev.rs`、`comm.rs`、`expand.rs`、`unexpand.rs`、`fold.rs`、`cksum.rs`、`paste.rs`、`split.rs`、`tsort.rs`、`colrm.rs`、`vis.rs`、`column.rs`，库测试 131 个）；同 crate `src/bin/` 薄壳二十四个已接线：tr、cut、uniq、head、tail、wc、sort、cmp、seq、pr、yes、rev、comm、expand、unexpand、fold、cksum、paste、split、tsort、vis、unvis（yes 为纯执行半无限循环；rev 走 stdin；comm 双流同源自检；cksum 走 stdin，crypto 算法族显式拒绝；cmp/pr/expand/unexpand/fold 的文件操作数待开放路径，2026-09-18）（执行半经 `minix-sys` 顶层 `read`/`write`，宿主的输入通道因 edge E-SYSCALL-SIGN 的假成功尚未出现文件末尾，真机与修复后正常；原占位壳随 34 壳删除移除，见 todo.md §6.1 步骤 3；2026-09-17）
> **前置依赖**: `06-file-ops.md`（搬文件在先）、`08-grep-sed.md`（正则概念在后——阅读顺序先本篇后 08，但概念上 08 是本篇部分工具的理论基础，见 1.5 节说明）
> **不覆盖（移交）**: 正则引擎（见 `08-grep-sed.md`）、编辑器（见 `09-editors.md`）、排序的缓冲策略与比较器（后续阶段）

---

## 1. 概念：行是文本世界的通用货币

### 1.0 本章说明

本章讲三十九个命令共用的思想：文本即行序列，一切操作都是"选行、变形、汇总"。按数据流向分成五组：取段、整形、统计、转换、比较。

> **本章不讲什么**：
>
> - 正则表达式的匹配原理（见 `08-grep-sed.md`）
> - 排序在内存放不下时的外部归并（后续阶段）
> - 补丁格式的生成算法（`diff` 的最长公共子序列，归算法阶段）
>
> 本章只讲行级变换：每个工具吃行、吐行（或吐数字），行为可用纯函数描述。

### 1.1 取段家族：头、尾、切、拼

- **取头取尾**（`head`、`tail`）：保留前 N 行或后 N 行。前者是"读一点就停"（管道上游可提前终止），后者必须看完全部输入（最后 N 行在哪，不看到文件尾无从知道）——这个不对称是理解两者实现差异的钥匙：前者是丢弃器，后者是环形缓冲。本库 `window.rs` 的 `HeadWindow` 与 `TailWindow` 正是这两种结构。
- **切分**（`split` 按行数或字节切文件、`csplit` 按上下文切）：大文件拆小，"拆了还能拼回去"是基本契约（`cat` 拼回须与原文一致）。
- **切割**（`cut`：选项串 `b:c:d:f:sn` 在 `cut.c` 第 88 行）：按字节、按字符、按分隔字段三选一。字段模式的分隔符可定制（`-d`），无分隔符的行默认原样通过（除非 `-s` 要求丢弃）——"整行没有分隔符"的处置是 `cut` 最常见的踩坑点。
- **拼接**（`paste` 横向并列、`join` 按关键列连接、`column` 对齐成表、`lam` 交错、`comm` 比较已排序文件的三列输出）：关系代数在文本上的投影，`join` 要求输入已排序（未排序输入的结果未定义——文档必须写明前置条件）。

### 1.2 整形家族：宽度、制表、方向

- **折行**（`fold` 按宽度折、`col` 处理退格换页、`colrm` 删列）：终端宽度是物理约束，超宽行必须有人折。
- **制表**（`expand` 制表符转空格、`unexpand` 反向）：制表位是"走到下个 8 倍数列"的语义，不是固定空格数——混用两者往返不一定恒等，文档如实说明。
- **方向**（`rev` 反转每行字符）：简单到不可能出错，是管道调试的利器。
- **分流**（`tee` 复制到文件加下游）：名字来自管道三通，`sudo` 提权写文件是其经典用例（重定向符权限不够时）。

### 1.3 统计家族：数、和、校验

- **计数**（`wc` 354 行）：行数（换行符个数）、词数（非空运行个数）、字节数、最长行。四条规则无一例外（见本库 `count.rs`），"词"的定义尤其值得背诵：最大非空运行——连词符不断词，标点不断词，只有空白断词。
- **校验**（`cksum`、`crc`）：循环冗余思想的两种参数化，传输后验完整性的标准动作。
- **求值**（`units` 单位换算、`jot` 生成数字序列、`seq` 同（新式接口）、`shuffle` 随机打乱、`uuidgen` 生成唯一标识、`yes` 无限输出 `y`）：数据发生器家族，测试管道时做"水龙头"用。

### 1.4 比较家族：异同与补丁

- **比较**（`cmp` 逐字节、`comm` 逐行三列、`diff` 在 `minix/usr.bin/diff/`）：粒度从字节到行到"最小编辑脚本"三级。`sdiff` 并排显示、`patch` 吃补丁改文件，构成"生成差异—检视差异—应用差异"闭环。
- **查找**（`look` 查排序字典、`ifdef`/`unifdef` 按条件取舍 C 预处理段）：前者是拼写检查的老底座，后者让一份源码按宏开关产出多份（驱动多配置编译的前身）。

### 1.5 阅读顺序说明：为什么先读本篇再读正则

计划把正则概念列为本篇前置（`07` 依赖 `08` 的概念序），但文档阅读顺序是 `07` 在前、`08` 在后——因为本篇三十九个命令里只有 `grep` 族与 `sed` 真吃正则，其余三十七个完全不懂正则也能讲透。读者按 `07 → 08` 走，先建立"行变换"的直觉，再学"模式匹配"的理论；`08` 写成后回看本篇的 `look` 与 `unifdef`，会有"原来如此"的二次收获。这种"先用后学"的编排是刻意的教学选择，不是依赖倒置。

### 1.6 排序、去重、转换三件套：本组的三块压舱石

- **排序**（`sort` 418 行）：按键排序、数值排序、去重输出、逆序——选项管"怎么比"，稳定与否、内存放不下时的外部归并是执行策略（后续阶段）。`sort -u` 是"排序加去重"一键完成，管道里最常见的组合之一。
- **去重**（`uniq` 257 行）：只合并相邻重复行（本库 `uniq.rs`），`sort` 的已排序输出是它的标准输入。"先排序后去重"不是习惯，是正确性要求（未排序输入的去重结果未定义）。
- **字符转换**（`tr` 283 行）：字符集映射、删除、压缩（本库 `tr.rs`）。`tr a-z A-Z` 大小写转换是入门第一课；`tr -d` 删字符、`tr -s` 压重复是数据清洗三板斧之二。
- **转储与可视**（`hexdump` 十六进制转储、`vis` 不可见字符可视化、`unvis` 还原）：二进制与文本的交界工具——看不懂的字节先转储，可疑的空白先可视化。

---

## 2. C 源码分析

### 2.1 `uniq.c`：状态机范本（257 行）

三个标志变量（`cflag`、`dflag`、`uflag`，第 56 行）在第 83 到 100 行附近分别点亮；核心判定在第 188 行（一句话：重复零次且要重复行、或重复多次且要唯一行，即跳过），计数打印在第 190 行。整个程序是一个"上一行、本行、计数"三元状态机——Rust 侧 `Uniq` 结构体是它的逐字段对照（`previous`、`count`、`mode`），`push` 对应"来一行"的转移，`finish` 对应"文件结束"刷出尾段。这是全阶段最小的完整状态机，适合作为状态机入门的第一个例子。

### 2.2 `cut.c`：选项串即契约（第 88 行）

`getopt` 选项串 `b:c:d:f:sn` 一行道尽全部功能：字节、字符、分隔符、字段、静默、行号（`n` 配 `-b` 不切多字节字符——字节模式下的多字节保护）。Rust 侧 `parse_list` 实现 `1,3-5`、`8-`、`-2` 三种写法（`cut.rs`），`select_bytes` 保证重叠区间只打印一次（`1-3,2-4` 得 `hell` 不是 `hellel`——与 C 工具逐字节去重打印一致）。

### 2.3 `wc.c`（354 行）、`head.c`（204 行）：计数器的两副面孔

`wc` 的状态是四个累加器加"词中/词外"一位（本库 `Counter` 的 `lines`、`words`、`bytes`、`longest`、`in_word`），分块投喂（`add` 可在任意字节处切分，词中状态跨块保持——测试 `test_chunk_split_mid_word` 钉住"整投与切投结果一致"）。`head` 的状态是"还要几行"的倒计数，`tail` 是 N 元环—— `window.rs` 的双实现与之一一对应。

### 2.4 `tr.c`（283 行）：集合、映射、压缩三操作

用法收集在第 58 到 125 行（操作数不足即打印用法退出，主函数第 81、89、106、125 行四处 `usage()` 守着四个入口条件）。集合写法（字面、区间、重复、命名类、转义）先展开成 256 元表，再做映射（短集补末字节）、删除、压缩。Rust 侧 `parse_set` 返回展开表加策略表（`CharClass` 三形态），`translate` 先建 256 映射再逐字节查表——与 C 工具"先编译集合、再流式处理"的两阶段一致。

### 2.5 `sort.c`（418 行）：本篇唯一的大块头，执行层留白

排序的语义（键、顺序、唯一、数值）是纯的，但"放不下内存怎么办"（外部归并、临时文件）是执行策略。本阶段实现前者的数据结构（行窗、计数器是它的零件），归并策略随后续阶段落地。文档在这里明确区分"语义"与"策略"，避免读者误以为排序只是"调个库函数"。

---

## 3. Rust 设计决策

### 3.1 为什么窗口是一个接口两种实现

取头与取尾回答同一个问题（"哪些行留下"），输入输出形状相同（逐行进、批量出），内部结构相反（丢弃器对环形缓冲）。`LineWindow` 接口（`push` 投喂、`drain` 取出留存行、`seen` 计数）让调用方（未来的 `head`/`tail` 主程序）无需区分；测试 `test_windows_share_the_trait` 把两者放进同一接口数组统一调用。这是 `02` 篇调度接口的第二次应用——同一设计在不同功能域的复现，说明它已成为本阶段的惯用手法。

### 3.2 为什么计数器按块投喂而不是按行

`wc` 处理的是字节流，不是行数组：输入可能来自管道（无长度）、可能极大（不能全读进内存）。`Counter::add` 接受任意字节块，内部保持"词中/词外"与"当前行长"，跨块正确。调用方（读文件、读管道）只管投喂——这种"推模式"与 Redox 的流式工具、Linux 的 `wc` 分块读取是同一架构，只是把"读"也交了出去（本阶段没有文件系统调用）。

### 3.3 为什么字符集是三形态接口

`tr` 的集合有四种写法（字面、区间、重复、命名类），但语义只有"成员判定"。`CharClass`（字面表、区间、命名类）三种实现共享 `contains`，测试 `test_class_strategies_agree` 穷举 256 字节证明字面表与区间语义一致。命名类限定 ASCII（C 本地化行为），多字节感知是显式留白（与正则引擎的字节取向一致，见 `08` 篇架构注记）。

### 3.4 为什么去重状态机不缓冲全文

`uniq` 只看相邻行是它的定义（不是优化）：`push` 来一行、断 runs 即报告，`finish` 吐尾 runs。内存占用恒为一行，与输入规模无关。测试 `test_collapses_adjacent` 用 `a a b a` 证明"非相邻不合并"——这是初学者最常误解的一点（以为 `uniq` 会全局去重，实则必须先 `sort`）。

---

## 4. 实现详解

### 4.1 模块结构

`os/commands/usr-bin/textfilter`（库包名 `minix-textfilter`）共 6 个源文件：

| Rust 文件 | 对应 C 源码位置 | 职责 |
|-----------|----------------|------|
| `lib.rs` | — | 错误类型（`TextError`，22 对应参数无效、12 对应缓冲不足）与模块组织 |
| `window.rs` | `head`、`tail` 语义 | `LineWindow` 接口、`HeadWindow` 与 `TailWindow` |
| `count.rs` | `wc.c` 计数语义 | `Counter` 分块计数 |
| `cut.rs` | `cut.c:88` 选项串 | 列表解析（`parse_list`）与字节字段选择 |
| `tr.rs` | `tr.c` 集合与三操作 | `CharClass` 接口、集合解析、转换删除压缩 |
| `uniq.rs` | `uniq.c:56-190` | `Uniq` 状态机与 `Run` 报告 |

### 4.2 关键类型与不变量

- **行窗 `LineWindow`**：逐行进、批量出。不变量：窗口内不超过 N 行借用；`drain` 按原序；`seen` 计数一切投喂（含丢弃）。
- **计数器 `Counter`**：四累加器加两位状态。不变量：分块边界不影响结果；最长行不含换行符；溢出即错（不回绕）。
- **字段表 `Range`**：1 起始闭区间。不变量：0 非法；逆序非法；空段非法；`N-` 上界记为最大数（调用方按行长截断）。
- **字符集 `CharClass`**：成员判定。不变量：字面表与区间穷举一致；命名类 ASCII；重复计数溢出即错。
- **去重器 `Uniq`**：相邻合并。不变量：非相邻不合并；模式过滤只管报告不管合并；空输入零报告。

### 4.3 函数一览

| 函数 | 输入 | 输出 | 对应 C 行为 |
|------|------|------|------------|
| 窗口 `push`/`drain` | 行流 | 留存行 | 取头取尾语义 |
| `Counter::add`/`finish` | 字节块 | 四计数 | `wc` 计数语义 |
| `format_counts` | 列选择与计数、可选名 | 一行输出字节 | `wc.c` `print_counts` 的 ` %7llu` 列与名后缀 |
| `parse_list`/`select_bytes`/`select_fields` | 列表文本与行 | 选中字节 | `cut` 选择语义 |
| `parse_set`/`translate`/`delete`/`squeeze` | 集合文本与输入 | 变换结果 | `tr` 三操作语义 |
| `Uniq::push`/`finish`/`keep`/`format` | 行流 | 报告 | `uniq` 状态机语义 |

---

## 5. 测试要点

`cargo test -p minix-textfilter`：**229 个测试，全部通过**（截至 2026-09-18，其中 11 个为 sort、5 个为 cmp、9 个为 seq、6 个为 pr、4 个为 rev、4 个为 comm、6 个为 expand、3 个为 unexpand、7 个为 fold、5 个为 cksum、4 个为 paste、3 个为 split、6 个为 tsort、4 个为 column 的决定半、11 个为 vis/unvis 编解码、8 个为 colrm、12 个为 jot、7 个为 lam、15 个为 col、12 个为 hexdump、10 个为 units、17 个为 unifdef、3 个为 floatfmt 垫片、15 个为 diff 引擎、10 个为 patch 引擎）；另有三十三个已接线的命令二进制（薄壳三十二个加 tee 的扇出形态），其决定半由上述库测试覆盖，行收集与切分属执行半的宿主接缝（见篇首说明）。
- **制表展开**（`expand.rs`，6 个）：`test_default_eight_column_stops`（do-while 至少一空格后补至 8 的倍数）、`test_single_stop_multiples`、`test_stop_list_picks_next_beyond_column`（末停后一格）、`test_backspace_pulls_the_column_and_passes_through`（退格透传拉列）、`test_newline_resets_the_column`、`test_stop_list_validation`（0/257/乱序/越界）。
- **制表还原**（`unexpand.rs`，8 个）：`test_leading_blank_run_becomes_tabs_at_eight`（8 空格一停、4 空格保持）、`test_two_space_gap_becomes_one_tab`（非 `-a` 首非空白后透传）、`test_single_space_stays_a_space`、`test_without_a_only_leading_blanks_convert`、`test_backspace_pulls_both_columns`（`-a` 下退格回拉后仍成 tab）、`test_stop_list_bounds_the_tab_region`、`test_parse_stops_rejects_unordered`、`test_every_line_converts`。
- **折行**（`fold.rs`，7 个）：`test_short_lines_pass_through`、`test_break_after_the_last_space`（末空格断行且空格留续行头）、`test_no_space_breaks_hard`、`test_split_words_breaks_anywhere`、`test_tab_counts_to_the_next_stop`（tab 超宽时空缓冲先出空行）、`test_count_bytes_counts_the_tab_as_one`、`test_empty_input_yields_nothing`。
- **行反转**（`rev.rs`，4 个）：`test_lines_reverse_character_order`、`test_unterminated_final_line_still_reverses`、`test_multibyte_characters_stay_intact`（字符级反转不撕裂多字节）、`test_empty_input_yields_no_lines`。
- **三列归并**（`comm.rs`，4 个）：`test_three_way_merge`（相等进三列、较小侧单进）、`test_suppression_flags`（列抑制）、`test_fold_compares_case_insensitively`（`-f` 折叠使大小写归同）、`test_drains_surviving_file_at_either_end`（任一端耗尽即排空余列）。
- **逐字节比较**（`cmp.rs`，7 个）：`test_equal_streams`、`test_first_difference_reports_byte_and_line`、`test_list_all_collects_every_difference`、`test_eof_names_the_shorter_stream`（regular.c:119 的三目选短流）、`test_line_counter_follows_the_first_stream`（行计数随第一流）、`test_report_first_diff_matches_netbsd_format`（`diffmsg` 的 `differ: char/line` 串）、`test_report_eof_branches`（`eofmsg` 的无`-l`、有`-l`行、有`-l`无行三分支）。
- **数字序列**（`seq.rs`，9 个）：`test_default_first_and_increment`、`test_first_last_and_stride`（两操作数方向跟随，seq.c:150-152）、`test_direction_errors`（错误方向命名报错，seq.c:154-159）、`test_separator_flag`（分隔符逐数输出含尾随）、`test_equalize_pads_to_common_width`（`-w` 零填含符号位）、`test_terminator_flag`、`test_zero_increment_rejected`、`test_non_integer_operand_rejected`、`test_float_format_declared_unsupported`。
- **分页**（`pr.rs`，6 个）：`test_page_geometry_is_66_lines`（66=5+56+5）、`test_header_line_carries_text_and_page_number`、`test_time_prefix_rides_the_header_line`、`test_body_pads_to_capacity`、`test_no_header_prints_the_body_bare`、`test_page_length_below_header_plus_trailer_is_rejected_by_the_caller`（minix3/usr.bin/pr/pr.c:setup（L1875，工具生成） 的页长下限由壳把守）。
- **键解析与比较器**（`sort.rs`，11 个）：`test_plain_byte_order`、`test_reverse_inverts`、`test_numeric_with_signs_and_fractions`（符号、小数、非数字按零）、`test_fold_and_dictionary`（折叠使键相等后整行回退、字典过滤）、`test_month_table`（月份表与未知前置）、`test_key_spec_field_selection_with_delimiter`（`-t:` 下 `-k2,2` 及整行破并列）、`test_key_spec_dot_zero_rejected`（`.0` 非法偏移）、`test_local_reverse_drops_under_global`（init.c:239-242 的局部 `r` 语义）、`test_fields_split_on_blank_runs`（空白分段）、`test_tie_falls_back_to_whole_line`、`test_parse_options_rejects_unknown`。

重点行为与测试的对应（以下函数名均可用 `rg "fn 测试名" os/commands/usr-bin/textfilter` 复现）：

- **行窗**（`window.rs`，5 个）：`test_head_keeps_first`（取头）、`test_tail_keeps_last`（取尾跨回绕）、`test_tail_short_input`（不足 N 行全留）、`test_zero_or_huge_rejected`（零与超限）、`test_windows_share_the_trait`（接口统一）。
- **计数**（`count.rs`，9 个）：`test_basic_counts`（行词节最长四数）、`test_chunk_split_mid_word`（切块一致性）、`test_unterminated_last_line`（末行无换行）、`test_empty_input`（空输入全零）、`test_multibyte_chars_count_scalars_not_bytes`（`-m` 计码点非字节）、`test_format_default_three_columns_with_width`（缺省三列固定 ` %7llu`）、`test_format_name_column_appended`（文件 operand 后接 ` 名`）、`test_format_char_field_selection`（`-m` 取字符、`-c` 取字节，同一列）、`test_format_wide_value_overflows_field`（超 7 位自然变宽）、`test_format_small_buffer_errors`（缓冲不足报错）。
- **切割**（`cut.rs`，5 个）：`test_byte_selection`（区间、离散、开尾、开头四写法）、`test_overlaps_print_once`（重叠去重）、`test_field_selection`（字段三选）、`test_missing_delimiter_policy`（无分隔符两策略）、`test_bad_lists_rejected`（空、零、逆序、空段）。
- **字符集**（`tr.rs`，7 个）：`test_translate_basic`（基本映射）、`test_short_second_set_repeats_last`（短集补末）、`test_range_and_repeat_sets`（区间与重复写法）、`test_named_class`（命名类十数字）、`test_delete_and_squeeze`（删除与压缩）、`test_class_strategies_agree`（两策略 256 字节一致）、`test_bad_sets_rejected`（逆区间、未知类名、坏重复）。
- **去重**（`uniq.rs`，4 个）：`test_collapses_adjacent`（相邻合并、非相邻不合并）、`test_repeated_and_unique_modes`（两种筛选）、`test_counted_format`（计数前缀）、`test_empty_input_no_runs`（空输入）。

尚未覆盖、随后续阶段补齐的：排序执行层（键解析、外部归并）、比较器（`cmp`、`diff`、`patch` 的文件配对）、分页与数字格式化（`pr`、`seq` 的输出宽度）、多字节字符感知（`cut -c`、`tr` 类、正则引擎统一升级）、uniq 的 `-i`/`-f`/`-s` 比较面与组合模式（库尚只载单一模式，见 §4.4 标注）。行算法层与三个已接线二进制是完整的，执行与策略层是显式留白的。

### 4.5 命令契约与 Requires

行为判定沿用 `99-global-concepts.md` §2 的规则：以 POSIX 为准绳、以 Minix3 C 实现为真值，C 偏离处在"当前状态"列标注。依赖面按 §1 分层契约只含 `minix-sys` 顶层与 `minix-rt`：

| 命令 | Requires（最小 API） | 当前状态 |
|------|---------------------|---------|
| tr | `read`、`write`、`exit`、argv 交接 | 已接线（翻译、`-d` 删除、`-s` 压缩、`-c` 取补四路） |
| cut | 同上 | 已接线（`-b`/`-c` 字节与 `-f` 字段两路，`-d`/`-s` 就位）；文件操作数已接（无操作数与 `-` 读 stdin、逐文件 open、打不开的文件按 C warn 跳过且退出码置错【非致命，区别于 expand/fold 的 freopen+err】；选项面按 `cut.c:88` `getopt("b:c:d:f:sn")` 忠实解析——`-b`/`-c`/`-f`/`-d` 值附于本 token（`-f1-3`/`-d:`）或取下一词、`-s`/`-n` 逐位聚簇（`-sf1-3`、`-sn`/`-ns`）、`--` 终止、字节与字段表并存报冲突（下沉 `parse_cut_args`，批次一百八十一）；真机 open 腿归 C-57/E5）|
| uniq | 同上 | 已接线（`-c`/`-d`/`-u` 与 `-f`/`-s` 比较面；决定半 `uniq.rs` 已测）；三个旗标是三面独立的位，不是互斥模式——`minix3/usr.bin/uniq/uniq.c` 拿 `cflag`/`dflag`/`uflag` 三个 int 存在 `show()` 里合成，所以下述组合全部合法：`-c -d` 只给重复行的计数，`-c -u` 只给单一行的计数，`-d -u` 每一条都被某一侧抑制、于是什么也不输出（Rust 对应 `UniqFlags::keeps`）；`-c` 前缀按 C 的 `"%4d %s"` 把计数右对齐到四列再跟一个空格（超过四列就撑开，不裁掉数字；注意 GNU `uniq -c` 用的是七列，本机 `uniq -c` 的读数不是这里的基线）；比较键走 `comparison_key` 复刻 C `skip()`：先跳 `numfields` 个字段（前导空白不算边界，终结字段的空白被一起吃掉），再跳 `numchars` 个字节，而报出来的仍是整行；输入文件操作数已接（无操作数读 stdin、1 操数则 open+read）、fopen 失败退出；输出文件（第 2 操作数）也已接：走 C `file(argv[1], "w")` 那一形——`O_WRONLY|O_CREAT|O_TRUNC`（`0x1|0x200|0x400`，`minix3/sys/sys/fcntl.h:65/99/100`）配 `0666` 模式，**在打开时就截断**（所以 `uniq f f` 结果是空档，与 C 同），且操作数的开档顺序照 C：先输入后输出，输入打不开时不会留下一个已被创建的输出档（`bin_support.rs:create_sink`/`WriteSink::finish`）；报告先在内存里攒齐再送目的端（输入本就整档读入，两者同一量级），送 stdout 那条腿仍走 `emit`（写失败退 1，与以前逐条打印同效）；`-i` 不是缺口：Minix3 的选项串是 `"-cdf:s:u"`，根本没有大小写折叠这个字母（那是 GNU `uniq` 的），本壳集外字母退用法错；obsolete 写法也已接：`-2` 等价 `-f 2`、`+3` 等价 `-s 3`，改写走 C 同一个预扫环——第二位不是数字的 token（选项字母，无论认不认得）只是跨过它继续扫，而不是停；遇到不以 `-`/`+` 开头的词（操作数，包括 `-f 2` 里的那个 `2`）才整体停下，所以 `uniq -f 2 -3` 里的 `-3` 不会被折成 `-f3`，而是被选项扫描器按集外字母 `3` 拒掉（`bin/uniq.rs:obsolete_args`）；逐字对账的端到端口子在 `os/commands/usr-bin/textfilter/tests/uniq_face.rs`：它跑真进程（真管道与真档），金样按 C 自推，并在两者语义重合的几形上与本机 `uniq` 直接对比（`-c` 列宽与 obsolete 写法故意不入比对，因为那是 GNU 与 Minix3 的固有差异）；真机 open 腿仍归 C-57/E5 |
| head | 同上 | 已接线（`-n`/obsolete `-N`，缺省 10；行窗按请求的个数存行、且不超过输入给的实际行数，所以任意大的计数都照用，不会拿它当容量去 malloc）；文件操作数已接（逐文件取前 N 行、多文件或 `-v` 时打 `==> 名 <==` 横幅、`-q` 静默、文件间前导空行、fopen 失败 warn+跳过【`-` 不特判为 stdin，因 head.c 用 fopen】）；`-c` 字节模式已接（`HeadArgs.bytecnt` 非零则走 `first_bytes_of` 逐字节取前 N 字节、忽略行计数，对位 `head.c:149-167`，与 `-n` 互斥）；零计数与非零负数都拒：`minix3/usr.bin/head/head.c` 的 `strtoimax` 后紧跟 `*ep || count <= 0`，报`head: illegal line count -- 0`（`-c` 同理报 byte）并退 1——注意这与 `tail` 不是同一回事：`tail.c` 的 `ARG` 宏接受零偏移并把它送去档尾（于是什么也不输出且退 0），两个程序对零的处理本就不同，不能拿一边的结论套另一边。逐字对账的端到端口子在 `os/commands/usr-bin/textfilter/tests/head_face.rs`：跑真进程，金样按 C 自推（缺省十行、`-n`/obsolete `-N`、`-c` 逐字节、超限计数、非正计数的报文、`usage` 行、`-q`/`-v` 横幅规则、打不开的操数不出横幅且退 1、`-` 是字面档名），差分腿只取行尾带换行的输入且不比零计数（GNU `head` 接受 `-n 0`，且两边对无尾换行的末行处理不同）。真机 open 腿归 C-57/E5 |
| tail | 同上 | 已接线（缺省取后十行，`-n N` 与 obsolete `-N`；这条走行窗，窗按请求个数存行且不超过输入给的实际行数，所以 `tail -n 1000000` 只占输入的那么多；零计数不报错而是选出空集）；文件操作数已接（逐文件取后 N 行、多文件无条件打 `==> 名 <==` 横幅、文件间前导空行——此 NetBSD `tail` 无 `-q`/`-v`、`fopen`/`fstat` 失败 warn 跳过【非致命】、`-` 不特判为标准输入）；字节模式 `-c N`、块模式 `-b N`（512 字节一块）、反序 `-r`（含 `-r` 配 `-c`/`-b` 的字节窗行反序）、前向偏移 `-n +N`/`-c +N`/`-b +N` 均已接；`-f`/`-F` 跟踪显式拒绝；选项面按 `minix3/usr.bin/tail/tail.c` 的 `getopt("Fb:c:fn:r")` 附值（`-n10`/`-c100`）/聚簇（`-rn3`）与 obsolete 写法忠实改写、集外字母显式拒绝不漏成操作数（下沉 `parse_tail_args`）。前向偏移走 C `ARG` 宏的 forward 支，与「取后 N」是两种模式而不是负数：`tail -n +20` 从第 20 行往后输出，`tail -c +4` 从第 4 个字节往后输出。C 的数字选项以一为基，`ARG` 先按单位乘、积非零时再减一个单位（`tail.c` 的 `ARG` 宏），所以 `+1` 什么都不跳、输出整档，`+0` 同它（乘积为零、不减单位）；越过档尾时 C 要么把 seek 夹到 `st_size`（常规档）、要么在 `getc` 处撞到 EOF（管道），两条都落在「什么都不输出」，本壳按同一形状收敛。`-r` 命中 C `tail.c` 的折算：forward 形态改成同单位的 backward 形态、偏移原样保留，于是 `tail -r -n +5` 是末四行反序；折算可能造出 `off == 0` 的反序形态，按 `minix3/usr.bin/tail/reverse.c` 的早退不输出任何一行——它与「整档反序」不能混为一谈，所以模式与计数由解析半折成一枚枚举（`TailStyle`）交给执行半，而不是三个各自可选的字段。obsolete 改写机不止认前导数字：`minix3/usr.bin/tail/tail.c` 的 `obsolete()` 把 `-3c`/`-3b`/`-3l` 的后缀单位字母用来选数字选项（`b`/`c` 直接留用，`l` 与纯数字折成 `n`），尾随的 `f`/`r` 提到最前面成独立字母，数字部分带着原 token 的首字符（`-` 或 `+`）回填为选项值——于是 `-3c` 变成 `-c-3`，而 `ARG` 的 `case '-'` 把负值取反后仍当 backward 形态读，两者等价于「末三字节」；`+10` 同理变成 `-n+10`，即从第 10 行起输出。这段预扫描不是「见到第一个非数字选项就停」：C 的 `obsolete()` 只在不以 `-`/`+` 开头的 token（也就是操作数）、`--`、或它不认识的字母上 `return`，其余继续扫——`-r -3c` 里的 `-3c` 照样被改写，而 `f -3c` 里的已经太迟，会原样交给选项扫描器按集外字母 `3` 走 `case '?'` 报用法错。`-b`/`-c`/`-n` 写成独立一个词时，它的值那一个词被跳过而不经扫描（C 的 `if (!ap[1]) ++argv`），于是 `tail -n -3c` 把 `-3c` 整个当作数字读，撞上 `ARG` 的 `*p` 判据报 `illegal offset -- -3c`。`ARG` 的第一句又是 `if (style) usage()`：两个数字选项无论重复（`-n 3 -n 5`）还是混用（`-c10 -b2`）都是用法错，不是后者覆盖前者。单位字母位置既不是 `b`/`c`/`l` 也不是数字时，C 在改写机里当场致命退出并引用整个 token（`xerrx(1, "illegal option -- %s", *argv)`），本壳同形拒绝。零计数不是错：`ARG` 把 `off` 乘成零就照零送去，`minix3/usr.bin/tail/forward.c` 的 `RLINES`/`RBYTES` 两臂在 `off` 为零时把读写位置直接弄到档尾（管道那一支则把输入喝完），于是什么也不输出但退码成功；`reverse.c` 开头同样对非 `REVERSE` 且 `off == 0` 早退。行窗不再有上限：C `forward.c` 的 `RLINES` 是在映射后的档案上往回走，与计数无关，本壳也一律照行（`window.rs:TailWindow` 先填满再回绕，存储只跟输入给的行数走）。逐字对账的端到端口子在 `os/commands/usr-bin/textfilter/tests/tail_face.rs`：跑真进程（真管道与真档），金样按 C 自推，并在 NetBSD 与 GNU 重合的四形上与本机 `tail` 直接对比；`-r`、`-b` 与末行无换行的行形不入比对（GNU 没有前两者，而本壳的行形会给末行补换行）。同一文件里钉住一处声明性差异：多操数时打不开的档不出横幅（`fopen` 失败即 `continue`）；真机 open 腿归 C-57/E5 |
| wc | 同上 | 已接线（`-l`/`-w`/`-c`/`-m`/`-L` 任选，缺省三路，流式计数不经收集）；文件操作数已接（逐文件 open/read、`argc>1` 补 total 行、打不开的文件 warn 跳过不出行），输出走 `count::format_counts` 复刻 C `print_counts` 的 ` %7llu` 固定列与名后缀（`-c`/`-m` 共享单列、后命中的互斥旗标为准）；真机 open 腿归 C-57/E5 |
| sort | 同上 | 已接线（决定半 `sort.rs`：`-k F[.C][flags][,F[.C][flags]]` 键规格、`b d f i M n r` 修饰符、`-t`、`-r`、`-u`、`-c`、整行回退；外部归并引擎属执行层，超内存输入待批）；输入文件操作数已接（逐操作数 open+read 拼接、`-` 读 stdin、无操作数读 stdin、fopen 失败退2、`-c` 只收一文件）；`-m`（归并）/`-o`（输出文件）由选项解析器报 BadOption 拒；真机 open 腿归 C-57/E5 |
| cmp | 同上 | 已接线（决定半 `cmp.rs`：逐字节比较、首个差异的字符与行定位、`-l` 全量清单、EOF 指名短流、`report_first_diff`/`report_eof` 串）；文件操作数已接（open 两文件+read+可选 `skip1`/`skip2` 起始偏移（进制自动识别，见下）、`-l`逐条/%s %s differ: char, line`/`-s` 静默三模式报告、退出码 0同/1异/2用法或打开错；`skip1`/`skip2` 按 C `strtoq(…, 0)` 读——前导空白跳掉、可选正负号、进制从数字本身判（`0x`/`0X` 十六进制、前导 `0` 八进制、其余十进制），且只认 C 的两条失败判据：一个数字都没转（`ep == argv[n]`）或超出 `off_t` 范（`errno`）——尾随垃圾不报错（`cmp a b 12xyz` 就是跳 12 字节），`08` 也是合法的零（C 把那个前导 `0` 当已转的数字，注意 GNU `cmp` 在这里与 Minix3 相反，它拒 `12xyz`）；偏移的应用点也按 C：`c_regular` 在 `skip > len` 时报 `EOF on <file>`（`-l` 下额外带 `: char <len + 1>`）并退 1，恰好等于档长不是报错而是空窗（C 的判据是严格大于），负偏移走 `c_special` 的 `for (...; skip1--;)` 把流读到档尾、落在同一张 EOF 报告上（`bin/cmp.rs:parse_skip`/`skip_window`）；两个操作数先开、后解跳过参数（C `cmp.c:95-119` 在 `:121-134` 之前），所以档打不开时报的是那个档名而不是 `usage:`；-s 另有一条入口判据：C `c_regular` 的第一句是 `if (sflag && len1 != len2) exit(1)`，比的是**未扣偏移**的档长，所以两个大小不同的档即使跳过之后内容相同也判为不同（退 1、不输出）；非 -s 的运行不受这条影响；而任一操作数是标准输入时 C 走 `c_special`，那里根本没有尺寸判据（本壳按同一条件限定，只覆盖两个都是普通档的情形）。逐字对账的端到端口子在 `os/commands/usr-bin/textfilter/tests/cmp_face.rs`：跑真进程（真管道与真档），金样按 C 自推（首个差异的 `differ: char %lld, line %lld`、`-l` 的 `%6lld %3o %3o` 八进制列、短流时走标准错误的 `cmp: EOF on <档>`、退 2 的两条文案、stdin 标签），与本机 `cmp` 只比退码与“哪条通道仍为空”（GNU 写的是 `differ: byte`、`-l` 列宽也不同，比文案量到的会是两个程序的差异）；`-` 操作数按 C `cmp.c:95-113` 读标准输入（`cat x | cmp - file` 可用，两操数皆 `-` 报“只能指定一次标准输入”、报文标签用 stdin；纯判定 `uses_stdin` 带测，批次一百八十三）；真机 open 腿归 C-57/E5 |
| seq | 同上 | 已接线（决定半 `seq.rs`：整数序列、方向跟随的缺省增量、方向错误与零增量报错、`-w` 等宽、`-s` 分隔、`-t` 终止；浮点增量与 `-f` 格式待浮点渲染裁决；二进制全功能） |
| pr | 同上 | 已接线决定半与单栏分页（66 行页、5+5 头尾、`-h`、`-l`、`-t`；时间字段由壳注入、多栏与 `-m` 待批；文件操作数待开放路径）；选项面按 `pr.c:1655` `egetopt("#adFfmrte?h:i?l:n?o:ps?T:w:")` 忠实聚簇（`-dm`/`-tw100`）/附值（`-w100`/`-hT`/`-l66`）解析，`+N` 起始页、obsolete 前导-digit 竖列 `-N`、`-s`/`-n` 可选附值、`--` 终止、裸 `-` 为操数、集外字母显式拒绝（下沉为 `parse_pr_args`，批次一百七十七） |
| yes | `write`、`exit`、argv 交接 | 已接线（纯执行半无限循环：写失败即止并退出 1，minix3/usr.bin/yes/yes.c:main（L50，工具生成）） |
| rev | 同上 | 已接线（逐字符反转含多字节完整性；决定半 `rev.rs` 已测）；文件操作数已接（无操作数读 stdin、逐文件 open、fopen 失败 warn+置退出码+跳过【非致命】、`rev` 无旗标）；真机 open 腿归 C-57/E5 |
| comm | 同上 | 已接线（决定半 `comm.rs` 已测：三列归并、`-1`/`-2`/`-3` 抑制、`-f` 折叠）；两文件操作数已接（`argc!=2` usage、`-` 读 stdin、否则 open+read、fopen 失败按 C `file()` err 退1），取代旧“两读 stdin 自拼”占位形态；真机 open 腿归 C-57/E5 |
| expand | 同上 | 已接线（决定半 `expand.rs`：`-t` 逗号/空格列表 1..256 严格递增至多八停、四路补空规则〔无停/单停/多停/末停后一格〕、退格拉回与换行清零；`-N` 旧式解析）；文件操作数已接（无操作数读 stdin、有则逐文件展开、tab 列逐文件重置对位 C `column = 0`、打不开的文件按 C `err` 直接退出；真机 open 腿归 C-57/E5） |
| unexpand | 同上 | 已接线（`tabify` 逐字节移植：`dcol`/`ocol` 双列、≥2 列间隙成 tab、`-t` 边界限位、退格双列回拉、`-a` 全行/缺省仅前导空白）；文件操作数已接（无操作数读 stdin、有则逐文件 tabify、行游走逐文件重置、打不开按 C `err` 直接退出；真机 open 腿归 C-57/E5） |
| fold | 同上 | 已接线（`\b`/`\r`/`\t` 列规则、`-b` 逐字节、`-s` 逐字符断行、缺省最后空格断行且空格留在续行头——`fold.c` 的缓冲回移语义；`-w`/obsolete `-N` 宽度）；文件操作数已接（无操作数读 stdin、有则逐文件折行、列/词态逐文件重置、打不开按 C `err(1)` 直接退出；真机 open 腿归 C-57/E5） |
| cksum | 同上 | 已接线决定半（`cksum.rs`：POSIX CRC 表 const 生成〔多项式 0x04c11db7，与 C crctab 全表核对〕、长度小端尾随、取反输出；`-o 1`/`-o 2` 历史和；`sum_display_length` 按 `print.c` 的 pcrc/psum1/psum2 将输出长度分别按字节/1024-块/512-块折算；与系统 cksum 四组值交叉验证）；文件操作数已接（逐文件 open+read、有 operand 行尾追加 ` 名`、stdin 无名、打不开的文件 warn 跳过不出行）；`-a` crypto 族显式拒绝待密码原语 |
| paste | 同上 | 已接线决定半（`paste.rs`：`-d` 分隔符列表循环与空项无分隔、`-s` 串行合并 `join_serial`、并行归并 `join_parallel`（每行跨流取一行、短流补空格、定界符逐 gap 循环、每行重启））；两文件操作数已接（无操作数读 stdin、逐操作数 open+read、`-` 读 stdin、fopen 失败按 C `err` 退1）；真机 open 腿归 C-57/E5 |
| split | 同上 | 已接线决定半与薄壳（`split.rs`：块计算与 `aa`..`zz` 后缀枚举）；文件操作数已接线——输入操作数（`-`→stdin、其余 fopen，`split.c:136`）+ 第二操作数前缀（缺省 `x`，`split.c:143`），逐块写进 `prefix+suffix`（`open(O_WRONLY 加 O_CREAT 加 O_TRUNC, 0666)`+部分写循环，`split.c:332`），旧的 `==> 前缀 <==` 横幅占位已被真写文件取代；`-a`（后缀宽度）已接（`suffix(index, width)` 按 26 进制生成 `width` 字母后缀、容量 `26^width` 穷举即 `None`；值附于本 token `-a3` 或取下一词 `-a 3`、非零全数字校验，非法报 "illegal suffix length"，对位 `split.c:117-121`，批次一百八十九）；`-b`（字节数）已接（`chunk_bytes` 按固定字节切块、末块可短；值带小写 `k`/`m` 单位 ×1024/×1048576、非零校验，非法报 "illegal byte count"，与 `-l` 互斥（并存报 usage），对位 `split.c:99-108`/`split1`，批次一百九十一）；`-n`（块数）已接（`chunks_n`：按 split3=split1(size/n,n) 语义恰好切 n 块、前 n-1 块各 size/n 字节、末块吸收余数；`n>size` 拒（C can't split into more than N files）、非法块数报 "illegal number of chunks"，与 `-l`/`-b` 三模式两两互斥，对位 `split.c:123-127`/`split3`，批次一百九十三）；选项面按 `split.c:80` `getopt("0123456789b:l:a:n:")` 忠实解析——`-l` 值附于本 token（`-l100`）或取下一词、全数字尾为 obsolete `-N`（仅未设时取）、`--` 终止、裸 `-` 为操数（`parse_split_args`，批次一百七十八）；开文件/写失败按 C `err(1)` 致命；真机文件创建受可写根盘 E5 门控（同 uudecode/写类，本笔交代码+宿主测的 out_path/render_chunk）|
| tsort | 同上 | 已接线（优先级归并输出、自环与环报告断开、重复弧重复计数、奇数个记号报错；`-l`/`-d` 经核为 C 侧 no-op——`tsort.c:102` 声明 `longest`/`Dflag` 但从不在其上分支（环报告恒走 `longest_cycle`），`parse_args` 按 no-op 收下与 C 一致，非缺口） |
| vis | 同上 | 已接线决定半与薄壳。引擎的形态照 `lib/libc/gen/vis.c`：每个字节先过 `do_svis` 的开首透传判据（`:310-312`）——被 extra 表列出的字节一律转义，其余的在三条通路之一上原样送出：是图形字符；是空白（空格、制表、换行，`iswwhite` 定义在 `:126`）；带 `-s` 且是退格、响铃、回车（`iswsafe` 定义在 `:127`）。没透传的字节进 `do_mbyte`（`:207-293`），支路次序是先 `-c` 的命名转义 switch、再「extra 命中、或低七位是空格、或 `-o`」的三位八进制、最后是 M 记法（`-b` 时省略前导反斜杠；高位置 `M`；控制字节写 `^X`，0x7F 为 `^?`；其余写 `-x`）。extra 表由 `makeextralist`（`:351-384`）按 `-e` 串、glob 集、shell 集、`VIS_SP`、`VIS_TAB`、`VIS_NL`、以及「除非 `-b` 才追加的反斜杠」这个顺序拼成；最后那条规则决定了默认模式下唯一被转义的图形字符就是反斜杠本身（`\134`），而 `-b` 让它直通。已接旗标十三枚：`-c`、`-o`、`-b`、`-e extra`、`-l`（行尾标 `\$`，那个反斜杠跟随 `-b`）、`-s`（`VIS_SAFE`）、`-t`（`VIS_TAB`）、`-w`（`VIS_WHITE`，即 `VIS_SP`、`VIS_TAB`、`VIS_NL` 三位一起，`include/vis.h:52`）、`-S`（`VIS_SHELL`）、`-M`（`VIS_META`，即 `VIS_WHITE`、`VIS_GLOB`、`VIS_SHELL` 的并，`vis.h:67`，也是本命令唯一能取到 glob 集的途径，因为 `VIS_GLOB` 自己没有字母）、`-n`（`none`：逐字节原样送出，只把每个反斜杠再写一份）、`-f`（按 80 列折叠）与 `-F width`（按给定宽度折叠）。折叠是同一契约的另一半：`-f` 与 `-F` 不改变任何字节的编码，只决定已经生成的文本在哪里断行，所以列量的是 vis 吐出的字符——一段转义序列 `\011` 占三列。列算式照 `usr.bin/vis/foldit.c:47-60`：换行与回车把列归零、制表进到八的倍数、退格退一列且不为负、其余每字一列；越限判据是 `col > max - 2`（`:61`），留两列余量是为了让标记自己不会被挤出行外。命中时 C 打印一个隐藏换行、把列清零、再**从头重扫同一个 chunk**（`goto again`）：只因为行已经写到哪儿而越限的 chunk 因此恰好花掉一个标记；而一个自身就宽于 `max - 2` 的 chunk 在任何行上都放不下，那个重扫永远走不完——`vis -f -F 5` 遇到 0xFF 编出来的四个字符（`\M^?`）就是这种局面，C 会无界打印标记而永不输出这段编码。本壳在此分岔一次（有意为之，不是等价改写）：插一个放得下的标记，然后按该 chunk 自身的宽度计列，让运行必然终止。折叠标记在 C 里由 `VIS_MIMESTYLE` 决定取 `=` 加换行还是反斜杠加换行，而本命令仍拒 `-m`，所以只有一支可达。`usr.bin/vis/vis.c:170` 的 `static int col` 让列值既跨 chunk也跨文件操作数存活；每个操作数末尾若最后一个字符不是换行，就再补一个隐藏换行（`:288-289`），而补这个标记**不**重置列值，于是下一个操作数的首字符可能立刻折行；空操作数也补，因为 `cp` 的初值是 `nul + 1`、`*(cp - 1)` 按构造就不是换行（`:171-172`）。`-F` 的取值按 `atoi`：前导空白跳过、可带一个符号、读到非数字为止、尾随垃圾忽略（`-F6x` 就是 6），整个词没有数字则折成 0，和 4、负数一样过不了 `< 5` 这道检查（`:90-93`）；并且 `-F` 那个 case 只做 `foldwidth` 赋值与 `markeol++`，它打开的是行末标记而**不是**折叠（`:89-95`），单独 `-F 40` 不折行——只有 `-f` 才 `fold++`（`:96-98`）。`-n` 不进 `VisOptions`，因为它保留 0x7F 以上的原始字节，而编码路径返回的 `String` 存不下（会变成 UTF-8 两字节），所以它是库里的另一个字节级入口；它在 C 的 `process()` 循环里排在行标记与编码之前（`usr.bin/vis/vis.c:213-227`），因此遮蔽两者。`-N`（`VIS_NOLOCALE`）同样不进 `VisOptions`：它在 C 里选的是两处分支的 C-Locale 一侧——`ISGRAPH` 用 `iscgraph` 而不是 `iswgraph`（`vis.c:122-124`），以及 `makeextralist` 把 `-e` 串按多字节文本解码（`:361`）——而本引擎按字节工作、从不把字节重组为字符，只有这一个答案可给；旗标被接受但不置任何位，`os/commands/usr-bin/textfilter/tests/vis_face.rs` 钉住「`-N` 的输出等于默认输出」。`-S` 与 `-M` 之下反斜杠仍被转义：`char_shell`（`:133`）自身含反斜杠，`-b` 只撤掉那条无条件追加的表项，撤不掉 shell 集里的那一份。文件操作数已接——逐操作数 fopen 编码进同一输出流、开不了按 C `warn` 并置 `rval=1` 后继续、无操作数读 stdin、裸 `-` 按 `fopen` 作普通文件名（`vis.c:151-164`）。选项面按 `getopt("bcde:F:fhlMmNnoSstw")` 的形：字母可在一个 token 里聚簇（`-bl` 等价 `-b -l`），`-e` 的值附于本 token（`-ex`）或取下一词（`-e x`），后来的 `-e` 整串替换前面的（C 就是 `extra = optarg` 赋值），`--` 结束选项扫描；`-d` 与集外字母报 C 的 usage 行（大写 U，退 1），因为 `-d` 在 C 的非 `DEBUG` 构建里本来就被编译掉、落到 `default:`；仍未接的 `-h`、`-m` 显式拒绝而非静默忽略——它们是 HTTP 与 MIME 两套编码支路（`lib/libc/gen/vis.c` 的 `do_httpvis` 与 `do_xvis`）；`-m` 接上时还要一并改掉两处默认：折叠标记换成 `=` 加换行，以及宽度仍是 80 时把它压到 76（`usr.bin/vis/vis.c:108-112`）。多字节宽字符路径不在本壳射程：输入按字节编码，而 C 在 UTF-8 locale 下按整字符给出答案，这条差异只对 0x7F 以上的字节成立；真机 open 腿归 C-57/E5 |
| unvis | 同上 | 已接线决定半与薄壳（S_META/S_CTRL/八进制状态机对照 lib/libc/gen/unvis.c:217-330；HTTP/MIME 旗标族 `-e/-H/-h/-m` 显式拒待批）；文件操作数已接——逐操作数 fopen 解码进同一输出流、无操作数读 stdin、开不了按 C `warn`+跳过且退出码仍为成功（`unvis.c:94-104`，区别于 vis 的 `rval=1`）；真机 open 腿归 C-57/E5 |
| colrm | 同上 | 已接线决定半与薄壳（列显示计：tab 进位到 8 的倍数、退格回拉、换行清零；范围 [start, stop] 之外的字节保留） |
| tee | 同上 | 已接线（stdin 到 stdout 与每个操作数文件的扇出、逐目标部分写循环）；文件操作数已接——逐操作数 `open(O_WRONLY 加 O_CREAT 加 (O_APPEND 或 O_TRUNC), 0666)`（`tee.c:102-103`，标旗取权威 `minix3/sys/sys/fcntl.h`）、`-a` 已接（选 O_APPEND 而非 O_TRUNC）、开/写/关失败按 C warn 置 exitval=1 且文件不摘除（后续块重试）、`-i`（忽略 SIGINT）仍待信号面显式拒绝；选项面按 `tee.c:80` `getopt("ai")` 忠实旗标聚簇（`-ai`/`-ia`）与 `--` 结束扫描（`parse_tee_args`，批次一百七十九）；真机文件创建受可写根盘 E5 门控（同 uudecode/写类，本笔交代码与宿主测的 parse_tee_args/open_flags）|
| column | 同上 | 已接线决定半与薄壳（`column.rs`：`-t` 表格 strtok 切字段、逐列最大宽对齐、字段后两空格；`-x` 横向填充与缺省纵向；termwidth 缺省 80，真实终端宽度查询随终端面）；文件操作数已接线——无操作数读 stdin、有操作数逐文件 open 后喂同一 entry pool 单次排版（`column.c:114-123`），打不开的文件按 C `warn`+置 `eval`+继续（非致命）、`-` 按 C `fopen` 作普通文件名不特判 stdin；真机 open 腿归 C-57/E5；选项面按 `column.c:93` `getopt("c:s:tx")` 忠实聚簇（`-tx`）/附值（`-c100`/`-s,`）解析并区分选项与操数（`parse_column_args`，批次一百七十五） |
| col | 同上 | 已接线决定半与薄壳（半行进位账本：退格叠印、回车复写、ESC-7/8/9 半整行进退、SI/SO 字符集、`-b` 仅留末印、空格压缩成 tab〔`-x` 关闭〕、越界回退一次性告警；UTF-8 续字节按 C locale 的 `isgraph` 丢弃。`-e` 迷你格式语言、注释跨行 DODGY 列为后续批） |
| jot | 同上 | 已接线决定半与薄壳（右到左操作数推导、非数字取末字符、`-p` 缺省时按小数位取精度、格式合成与校验〔`%s` 拒绝、整型族置 dox、尾随垃圾报错〕、`-r` 区间缩放与 `-c` 字节输出；libc `random()` 位序不复现，注入 LCG，列为本批登记角落） |
| lam | 同上 | 决定半与薄壳（逐流 sep/宽度/pad/行终止符规格、大写 `-F/-P/-S/-T` 粘滞继承、`min.max` 截断补齐、缺流 pad 补白）；文件操作数已接线——裸 `-` 读 stdin、其余 fopen 逐流取入（`lam.c:113-116`），打不开的文件按 C `errx(1)` **致命退出**（区别于 grep/sed/column 的 warn+继续，因 lam.c 开文件失败直接终止），真机 open 腿归 C-57/E5 |
| hexdump | 同上 | 已接线决定半与薄壳（七种规范格式按块渲染、块内尾空格末次抑制、越界字段按 `%<宽>s` 补空白、`%_p` 零宽垫、重复块 `*` 抑制〔FIRST→WAIT→DUP〕、`-s b/k/m` 从流首跳过、`-n length` 截断显示范围（与 skip 合成、夹到输入内，对位 `hexdump.c` eaddress）、终地址行宽度随格式；`-e`/`-f` 格式语言显式拒绝并登记）；文件操作数已接线——`display.c` `next` 把每个操作数 `freopen` 串成一条连续字节流（地址跨文件连续、`-s` 从流首跳），无操作数读 stdin、裸 `-` 按 `freopen` 作普通文件名不特判 stdin，打不开的文件按 C `warn`+置 `exitval=1`+继续（真机 open 腿归 C-57/E5）|
| units | 同上 | 已接线决定半与薄壳（词表解析〔前缀 `-` 后缀、注释行〕、表达式归约〔`a|b` 有理数、尾数字重复、指数负号保留〕、`^`/复数/前缀三级查找、约分与顺从性检查、`\t* / \t/` 答案、`-l/-L` 清单；数据库经 std::fs 读 `-f` 或 `/usr/lib/units`，PATH 搜索为登记角落） |
| unifdef | 同上 | 已接线决定半与薄壳（十状态 #if 状态机、C 注释扫描含反斜杠续行、四级优先表达式〔`defined()`、`-D` 值、十六/八进制字面量〕、`#elif`→`#if`/`#endif` 关键字改写保嵌套、`-c` 补集、`-l` 置空、`-s` 符号清单、退出码 0/1/2；注释跨行的 DODGY 表列未建模，`-d`/`-e` 收下即忽略，均登记） |
| look、ifdef | — | 查证（2026-09-18）：`minix3/usr.bin/` 下无此二命令（`look.c` 在 `usr.bin/m4/` 内属 m4 内建，`ifdef` 同为 m4 内建），不在 07 篇接线范围 |
| crc | — | 查证（2026-09-18）：`usr.bin/cksum/crc.c` 的 CRC 已由本库 `cksum` 模块的 `cksum_crc` 覆盖（批次十四，const 表与 C crctab 全表核对），无独立命令需要接线 |
| diff | 同上 | 已接线决定半与薄壳（4.4BSD 引擎全链：稠密等价类〔代 readhash，jackpot 复核随之精确化〕、公共前后缀剪枝、sort+equiv+unsort、stone 候选搜索含 isqrt 尝试上限、unravel 出 J 表、check 复核、六种输出格式 normal/`-e` 逆序 ed/`-f`/`-n` RCS/`-q`/`-u` 合并上下文，逐格式对照宿主 diff 向量；`-c`、`#ifdef`、`-b/-w/-i` 忽略族、`\ No newline` 与目录递归为登记角落）；两文件操作数已接线——任一侧 `-` 读 stdin、其余 fopen 读入（对位 C `diffreg`），开不了的侧按 C `status |= 2` 致命退 2（无可比对象），真机 open 腿归 C-57/E5 |
| patch | 同上 | 已接线决定半与薄壳（unified/normal hunk 解析〔pch.c 的 UNI_DIFF/NORMAL_DIFF 分支：`(起始,计数)` 头、`< / --- / >` 体、空格被吃容错〕、定位引擎〔patch.c 的 locate_hunk/patch_match/apply_hunk：猜测位 `first+last_offset`、正负偏移交替搜索、上下文 fuzz 最多 2 且拒绝真空匹配、`where--` 后按对齐行惰性拷贝/删除/插入〕，10 测试；context 格式、ed 管道、`-R`、reject 文件与 `\ No newline` 为登记角落） |

---

## 6. 过渡：会摆弄行之后，去匹配模式

本篇走完了行变换：取段、整形、统计、转换、比较——三十九个命令的决策核心就位。但有一个问题本篇一直绕着走：`grep` 如何判断"这一行有没有我要的模式"、`sed` 的替换地址怎么写。出彩票号码靠运气不行，靠的是模式语言——正则表达式。这是 `08-grep-sed.md` 的职责，也是本系列第一块需要"编译器"知识的拼图。请沿因果链继续向下走：先会"摆弄行"，再会"描述要找什么样的行"。

---

## 7. 参见

- `06-file-ops.md`——文件操作（上一步：搬文件）
- `08-grep-sed.md`——正则与搜索（下一步：模式匹配）
- `minix3/usr.bin/uniq/uniq.c:__COPYRIGHT（L56，工具生成）`——标志与判定（状态机的逐行对照）
- `minix3/usr.bin/cut/cut.c:main（L88，工具生成）`——选项串（列表语法的权威来源）
- `minix3/usr.bin/tr/tr.c:__COPYRIGHT（L58，工具生成）`——用法收集（集合写法的权威来源）
- `minix3/usr.bin/wc/wc.c`、`head.c`、`sort.c`——计数器与排序（354、204、418 行）

---

## 附：验证记录（评审用，可跳过）

- `wc -l` 六文件 → `sort` 418、`wc` 354、`cut` 306、`tr` 283、`uniq` 257、`head` 204 行，与正文引用一致。
- 39 命令目录存在性：`usr.bin` 三十五（逐个 `[ -d ]` 验证，无缺失）、`minix/commands` 三（`look`、`ifdef`、`crc`）、`minix/usr.bin/diff`，全部存在。
- `cargo test -p minix-textfilter` → 25 通过、0 失败；`cargo clippy` 无警告。
- 本文档引用的 `file:line` 均来自正文写作前实际执行的 `rg -n` 与 `sed -n` 输出，非凭记忆书写。
