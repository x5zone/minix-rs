# 22-纯输入输出游戏

> **状态**: 已完成，等待评审收敛
> **定位**: 交付因果链的验收层第一组——只用标准输入输出说话的游戏，系统能跑、管道能通的证明
> **源码**: `minix3/games/factor/factor.c`（输出形状见头注释、因子打印函数见第 184 行附近、大数路径见第 110 行附近、用法 `factor [value ...]` 见第 268 行附近）、`minix3/games/primes/primes.c` 与 `pattern.c` 与 `pr_tbl.c` 与 `spsp.c`（区间素数表）、`minix3/games/caesar/caesar.c`（轮转表构造见第 82 行附近、溢出保护 `rot %= LETTERS` 见第 86 行附近、轮转解析见第 125 行附近）、`minix3/games/morse/morse.c`（码表见第 96 行附近、用法 `morse [-ds] [string ...]` 见第 140 行附近、解码匹配见第 219 行附近）、`minix3/games/pig/pig.c`（元音规则见第 103 行附近、用法 `pig` 见第 133 行附近）、`minix3/games/arithmetic/arithmetic.c`（缺省范围 10 见第 99 行附近、对错计数见第 100 行附近、范围选项见第 107 行附近）、`minix3/games/banner/banner.c`（整幅宽 132 见第 58 行附近、用法 `banner [-w width] [message]` 见第 1057 行附近、列缩放见第 1064 行到第 1066 行附近）、`minix3/games/number/number.c`（数字转英文单词）、`minix3/games/bcd/bcd.c`（大字显示）、`minix3/games/ppt/ppt.c`（纸带 punch 显示）
> **Rust 模块**: `os/commands/games/stdio-games`（库包 `minix-stdio-games`：`factor.rs`、`primes.rs`、`caesar.rs`、`morse.rs`、`words.rs`、`banner.rs`、`quiz.rs`、`bcd.rs`、`ppt.rs`，库测试 57 个；端到端门三扇 `tests/morse_face.rs` 十二枚、`tests/factor_face.rs` 八枚与 `tests/pig_face.rs` 六枚）；同 crate `src/bin/` 薄壳九个已接线（bcd、ppt、factor、primes、caesar、morse、pig、number、arithmetic，各含 6 个行缓冲测试，连同上述库测试与端到端门共 137 个测试全过）；执行半经 `minix-sys` 顶层 `read`/`write`（宿主传输语义待 edge E-SYSCALL-SIGN）；`banner` 薄壳待真位图字体资产后接线（同名占位二进制壳已删除，见 todo.md §6.1 步骤 3）
> **前置依赖**: `06-file-ops.md`（标准输入输出面先行）
> **不覆盖（移交）**: 终端控制（见第 23 篇）、键盘读取与逐行显示执行（各游戏二进制包）、大数概率分解（后续补齐）

---

## 1. 概念：只用说话证明系统活着

### 1.0 本章说明

想象系统刚装好，你想确认它真的活着。最轻的确认是不碰屏幕控制，只说话：给程序一行字，它回一行字。十个纯输入输出游戏就是这种确认：分解质因数、列素数、转摩斯电码、玩猪拉丁语、做算术测验、打大字横幅、数数转单词。它们只依赖三件事：启动、标准输入输出、退出，所以最早可验收。

> **本章不讲什么**：
>
> - 屏幕定位与键盘逐键读取（见第 23 篇）
> - 大数的概率分解（本篇只到试除，后续补齐）
> - 纸带与编码显示的字体位图（执行层随字体数据落地）
>
> 本章只讲玩法语言：输入长什么样，输出怎么排，规则怎么判。

### 1.1 数论两件：分解与列举

分解游戏读入整数，输出该数加冒号加从小到大排好的因子，重复因子重复出现。素数游戏读入上下界，输出区间里所有素数。两者的数学核心都是试除：从小因子开始，能除就除，除尽再看剩下的。分解多一个收尾：如果除到平方根还没除尽，剩下的就是素数。素数多一个起点：如果下界倒置，直接输出空。

为什么试除够用？因为游戏面对的是教学数字，不是密码学大数。试除的代码最短、最容易验证，配一张百以内小素数表起步，再用六步轮转跳过偶数和三的倍数，十万以内的数瞬间出结果。大数走概率路径，那是另一个故事，本篇诚实留白。

### 1.2 密码与电码：轮转与点划

轮转密码把每个字母向后推固定格，大写在 大写里转，小写在小写里转，别的字符原样通过。推 13 格最有名：推两次回到原文，加解密同一个操作。轮转量先对 26 取余再转，超大输入不会溢出。解析轮转量时拒绝负号与非数字，溢出报超范围而不是回绕。

摩斯电码把字母翻译成点划组合，编码查表，解码整串匹配。字母大小写不分，数字直接映射，词间空白变成斜杠。未知字符报查无而不是跳过，因为静默跳过会让收报人对丢字一无所知。解码只认点和划，别的符号直接报参数无效。

### 1.3 文字游戏：猪拉丁语与数词

猪拉丁语的规则分三种：元音开头直接加后缀，辅音串开头把串搬到尾再加后缀，没有元音可停的串就整个搬完。这里藏着两套不同的字母集合——决定「直接加 `way`」的那一套**不含 `y`**，决定搬运在哪里停下的那一套**含 `y` 且从首字母就检查**，于是 `yellow` 根本不搬运（`yelloway`），而 `yes` 也只加后缀（`yesay`）。另一对容易被抹平的是大小写：被搬走的串里只有原词的**第一个**字母会被转小写，新的首字母若本来大写才被转成大写，其余字母原样走——`Pig` 得 `Igpay`，混写的 `sTring` 得 `ingsTray`——搬到头后的那串里，原本大写的字母照样大写。全大写词整套后缀也用大写（`WAY`/`AY`）。这些细节不是装饰，是几十年玩家约定的玩法，改一条就不是猪拉丁语了。

数字转单词把阿拉伯数字读成英文：十九以内直查，二十以上十位加个位，百位加 hundred，千位加 thousand，零单独读。范围收到九十九万九千九百九十九，够覆盖日常报数，超了报超范围。

### 1.4 大字与测验：横幅宽度与对错计数

横幅把消息打成大字，整幅光栅宽 132 列，`-w` 选项压窄，经典终端宽 80 列是常用值。压窄不是裁掉，而是按比例把输出列映射回光栅列，四舍五入，所以窄横幅的字是挤的不是缺的。宽度为零或超过整幅宽都拒绝。

算术测验把操作数限制在零到范围里，缺省十。答对加一，答错加一，百分比是答对除以总数。减法在零处截断（不够减算零），除法只出整除题（出题时保证整除），乘法用饱和乘防溢出。范围解析拒绝零与非数字。

---

## 2. C 源码分析

### 2.1 分解：输出形状与两条路

输出形状写在头注释里：数加冒号加因子，因子不递减。因子打印函数（第 184 行附近）从小因子找起，除尽就打印并继续除，遇到比表大的素数直接打印。大数走概率路径（第 110 行附近），本篇只到试除。用法行（第 268 行附近）说明无参数时从标准输入读数。

Rust 侧 `factor.rs` 的试除（小素数表起步、六步轮转续航、余数收尾）与小路逐项对应，行绘制（数加冒号加空格分隔）与输出形状逐项对应。取值那一条腿按 C 逐字坐实成 `verdict`：前导空白属于数（`isblank` 扫描 `:163`，`strtoul` 也跳同一集合）、符号可选（`:119-128`），数字之后只能直接遇到行末或换行，否则就是那句 `<值>: illegal numeric format.`（`:177`）。三句拒绝各归不同的来路且不可互换：`pr_fact` 的防火墙管 0 与 1（`:218-219`）；C 的一句话滑差让**以 `-` 开头的操作数**也答这一句（`:173-174`）；只有标准输入那条腿在空白扫描之后另有 `negative numbers aren't permitted.`（`:166-168`）；纯空白行则是静默跳过（`:164-165`）。选项面同样照 C：空 optstring（`:147`）使任何 `-x` 落到 `case '?'` 而报 usage，而 C 的 `usage()` 是以状态 **0** 退出的（`:266-270`）；`--` 是结束标记而非操作数，单独的 `-` 是操作数。唯一保留的差异是溢出：C 走 `err(1, "%s", str)` 带 `strerror` 后缀（`:123-124`），本树没有 strerror 表，于是退码同为 1、句子按非法格式给，这条差异在库注释与本文状态列都写明。

### 2.2 素数：区间生成

素数工具在上下界之间生成，倒置区间输出空。模式文件与素数表文件是加速手段，核心语义是区间里每个素数恰出现一次。Rust 侧 `primes.rs` 的逐候选试除与核心语义逐项对应，上界一百万是工作集上限，超了报超范围。

### 2.3 轮转：建表与解析

轮转表构造（第 82 行附近）先取余防溢出（第 86 行附近），大小写各转各的，非字母直通。轮转解析（第 125 行附近）做溢出检查。Rust 侧 `caesar.rs` 的单字节轮转与建表语义逐项对应，行轮转要求等长（调用方保证缓冲），长度不对报参数无效。

### 2.4 摩斯：三张码表、每字节一行与整串解码

码表其实是三张：`digit` 十项（`morse.c:53-64`）、`alph` 二十六项（`:65-92`）、`other` 十二项再加一个 `'\0'` 哨兵（`:97-111`）。解码也是整串比对（`:201-228`），部分匹配不算：`.-` 是 A、`.-.` 是 R、`.-..` 是 L，`...-.-`（结束记号）是**唯一不输出任何字节**的输入（`:225-226`），其余查不到一律答 `x`（`:227`）——那是 C 的「线路噪声」答案，不是错误。标准输入那条腿把三个反直觉形状叠在一起（`:151-185`）：**撞上悬挂码组的分隔符只负责冲刷、自己不打印**，空格属于「连着两个都扑空」的那一个（`:175-182`），所以 `...  ---` 一个空格都不打而 `...   ---` 打一个；攒够十个符号算胡码，打一个 `x`、清零，再把同一串剩下的符号**连同紧跟其后的那个字节一起吞掉**（`:160-169`），被吞的字节再也充当不了分隔符；收尾的 `putchar('\n')` 在循环之外（`:185`），于是空输入也有一行，而末尾没等到分隔符的码组直接丢掉。操作数走另一条腿：整串比对、不拆空格（`:146-150`），所以 `morse -d "... ---"` 得 `x` 而不是 `SO`。用法行（`:140`）的两个字母管的是两件互不相干的事：`-d` 选方向，`-s` 只换**渲染形态**——`show()`（`:253-261`）不带 `-s` 时把每个符号拼成 ` dit` 或 ` daw`，带 `-s` 时打一个空格再加整串符号。它既不是码表打印也不管容错：`decode()` 从不读这个旗标，所以 `-d` 之下 `-s` 一个字节都不改。编码面的形状是**每个输入字节一行**：空白字节不被跳过，而是渲染成空码（`:239-240`）；每个操作数末尾再补一行 `show("")`（`:191`）；整轮结束后统一打一次 `show("...-.-")`（`:195`，即 SK 记号）——收尾行属于整轮而不属于任何操作数。表里没有的字节连一行都不产生：C 的 `else` 支扫完 `other[]` 落空就直接离开（`:241-250`），既不报错也不打空行。两处不对称值得专门钉住：C 的判据是 `*s == '.'`，**凡不是点的都算划**；C 的 `isspace` 含垂直制表 0x0B，而 `u8::is_ascii_whitespace` 不含——照抄后者会吞掉一个分隔行。Rust 侧 `morse.rs` 把三张表与两种形态做成纯查表与纯渲染函数（`code_for_byte` 与 `show_line`），执行半 `bin/morse.rs` 负责逐字节调用、操作数收尾与整轮收尾的次序。

两条腿共用同一个输出件，`show()` 值得整段看（`morse.c:253-261`）：

```c
void
show(const char *s)
{
        if (sflag)
                printf(" %s", s);
        else for (; *s; ++s)
                printf(" %s", *s == '.' ? "dit" : "daw");
        printf("\n");
}
```

它的角色是「一行」的定义者：编码腿每查到一个码就调它一次，收尾的 `SK` 也调它一次（`:195`）。三段各有各的条件。`sflag` 那一支把整串一次打出，前面只有一个空格，因此空码打出的是一行「一个空格」。`else` 那一支逐符号打词，判据写作 `*s == '.'`——**凡不是点的都算划**；码表里只有点与划，所以这条不对称在正常输入下观察不到，除非码串里混进第三种字符，本树就用一个混入 `x` 的码串把这条钉成断言（见 §5 的摩斯清单）。最后那句 `printf("\n")` 落在两支之外，于是**一次调用恰好一行**；也正因为它在两支之外，`show("")` 是一行空白而不是零行——这就是分隔符自己占一行的全部来由。

反过来推：把那句换行挪进 `else` 支，`-s` 形态的输出会全部挤在同一行里；把判据改成 `*s == '-'`，混入的第三种字符就从「划」翻成「点」。两种改法都改变可观察输出，所以按原样保留，而不是顺手把它合理化。


### 2.5 猪拉丁语与算术：元音规则与范围计数

元音规则（`pig.c:103` 与 `:114`）是**两套集合**：`way` 判据看 `aeiouAEIOU`，搬运停止判据看 `aeiouyAEIOUY` 且从 `start == 0` 就测，故首字母 `y` 让循环一步都不走；搬运时 `qu` 算一个整体（`:117-119`），所以 `quay` 是 `ayquay`。大小写是「跟着字母走」而不是归一化：`if (!allupper) buf[0] = tolower(buf[0])`（`:111-112`）与 `if (firstupper) buf[start] = toupper(buf[start])`（`:124-125`）各只碰一个位置。用法行（`:131-135`）是光秃的 `pig`：optstring 为空，`-x` 报 usage 并退 1，而**操作数被数出来却从不读**——`main` 之后只消费标准输入（`:76`）。流本身（`:62-92`）还有三条：一个词是**连续的 `isalpha` 字节**，数字与标点既冲刷词又原样透出；末行没有分隔符时那个悬挂词**永不打印**（循环退出后直接 `exit(0)`）；字母连到第 1025 个才触发 `errx(1, "ate too much!")`（`:78-79`，检查发生在存入之前）。算术的缺省范围（第 99 行附近）与对错计数（第 100 行附近）是计分核心，范围选项（第 107 行附近）收操作数。Rust 侧 `words.rs` 的三规则分支与元音规则逐项对应，`quiz.rs` 的范围解析与计分与算术语义逐项对应。

### 2.6 横幅：整幅宽与列缩放

整幅宽 132（第 58 行附近）是光栅上限，宽度选项超限拒绝（第 1052 行附近），用法行（第 1057 行附近）收宽度与消息。列缩放公式（第 1064 行到第 1066 行附近）把输出列按比例映射回光栅列并四舍五入。Rust 侧 `banner.rs` 的宽度解析、上界检查、缩放公式与这三处逐项对应，字形源以接口隔离（真位图随执行层，块渲染做可测替身）。

---

## 3. Rust 设计决策

### 3.1 为什么试除先用小素数表再用轮转，而不是从头轮转

从二开始一个一个试要除很多合数，百以内小素数表把最小的二十五个素数一次除尽，剩下的数已经小了很多，轮转再接手。表的代价是二十五个常数，收益是每次分解省掉大量试除。这是空间换时间的最小实例：表再大收益递减（大素数命中的概率低），表太小輪转起步慢，百以内是甜点。

### 3.2 为什么摩斯码表与空表做成接口的两实现，而不是一个表加开关

空表不是关掉的表，是没有数据的表：查什么都 miss。测试想验证缺数据时的行为，用空表直接得到查无，不用构造坏数据。接口让两种行为并列：静态表答得出，空表答不出，调用方（编码函数）只认接口不认具体表。如果做成开关，测试要翻开关，开关状态是全局的，并行测试互相干扰。类型隔离比状态开关干净。

### 3.3 为什么大小写跟着字母走，而不是按词归一化

统一转小写最容易实现，但它会改写玩家看到的东西：`Apple` 若整词小写就成 `appleway`，名字的大小写丢了。C 的做法只动两个位置——搬走的串里**只有原词第一个字母**转小写（`:111-112`），新的首字母若原词以大写开头才转回大写（`:124-125`），其余字母（包括串里的 `T`）保持原样。于是 `Pig` 得 `Igpay`、`String` 得 `Ingstray`、混写的 `sTring` 得 `ingsTray`。为什么不索性更聪明一点，比如「串里的大写也保留原位」或「整句首字母大写」？因为搬运本身改变了字母次序，任何按位置的规则都会把大小写和字母拆开——`STRING` 的后缀要不要大写，只能由「整词每个字母都大写」这条判据（`:98-100`）决定，而不是由某个位置上的字母决定。反过来推：若把 `:124-125` 那次转大写也省掉，`Pig` 会打成 `igpay`，玩家认不出这是自己那个专有名词；若把 `:111-112` 的小写也省掉，`Pig` 又会打成 `IgPpay` 那样把大写留在串中。两条规则各只动一个位置，正是为了让「字母带着自己的大小写移动」这句话成立。

### 3.4 为什么横幅字形用接口隔离，而不是直接内嵌位图

真位图字体体积大（几十个字符乘多行），内嵌进判断层会让库臃肿，而且字体是数据不是逻辑，数据的归宿是资源文件。接口隔离后，判断层只管宽度、缩放、行绘制，字形从接口来：块渲染做测试替身（行为明确不同，测试一眼可辨），真字体随执行层落地。测试替身和真实现行为不同是故意的：测试替身只证明排版逻辑对，不冒充字体对。

### 3.5 与同类系统的对照：游戏只做玩法，输入输出留给系统

本篇延续全阶段的分层：纯层把玩法算明白（分解、查表、轮转、计分），读键盘、写屏幕、分行这些事归各游戏的二进制包。同类现代系统的文字游戏也是这个分法，只是界面库不同，本篇的纯判断层不依赖任何一种界面实现。最早可验收的承诺不变：这七个模块零系统调用，裸机测试就能跑。

---

## 4. 实现详解

### 4.1 模块结构

`os/commands/games/stdio-games`（库包名 `minix-stdio-games`）共 11 个源文件（`bin_support.rs` 经 `#[path]` 只进入各二进制壳，不进入库）：

| Rust 文件 | 对应 C 源码位置 | 职责 |
|-----------|----------------|------|
| `lib.rs` | — | 错误类型（`GameError`，22 对应参数无效、2 对应查无、34 对应超范围）与模块组织 |
| `bcd.rs` | `bcd.c` 第 87 到 120 行（孔位表）、第 151 到 220 行（卡片绘制） | 打孔表、单卡渲染 |
| `ppt.rs` | `ppt.c` 第 52 行（EDGE）、第 129 到 175 行（打孔与读带） | 纸带行渲染、读带还原 |
| `bin_support.rs` | — | 各二进制壳共用：行缓冲（C `fgets` 形状）、十进制格式化、宿主/真机接缝（`args`/`terminate`/`emit`/`warn`/`read_stdin`/`stdin_chunk` 各自双生） |
| `factor.rs` | `factor.c` `:119-128`、`:147-179`、`:196-252` | 取值判定、试除、行绘制 |
| `primes.rs` | `primes.c` 区间语义 | 界限解析、区间生成 |
| `caesar.rs` | `caesar.c` 第 82 行与第 86 行与第 125 行附近 | 轮转解析、单字节轮转、行轮转 |
| `morse.rs` | `morse.c` 三张码表（`:53-111`）、`morse()`/`show()`（`:231-261`）、`decode()` 与 `-d` 循环（`:146-228`） | 逐字节查表、两种渲染形态、整串解码、解码字符流（`decode_stream`） |
| `words.rs` | `pig.c` 第 103 行、`number.c` | 猪拉丁语、数字转单词 |
| `banner.rs` | `banner.c` 第 58 行与第 1057 行与第 1064 行附近 | 宽度解析、列缩放、字形源接口与行绘制 |
| `quiz.rs` | `arithmetic.c` 第 99 行与第 100 行附近 | 范围解析、答案计算、计分 |

### 4.2 关键类型与不变量

- **小素数表 `SMALL_PRIMES`**：百以内二十五素数。不变量：升序；试除起点；表外由轮转接手。
- **摩斯码表 `MorseTable`**：编码查询加解码查询。不变量：静态表三十六条（字母二十六加数字十个）；空表一切查无；大小写不分。
- **字形源 `GlyphSource`**：五行字块查询。不变量：行号零到四；块渲染可打印字符恒有字形；空源一切查无。
- **猪拉丁语**：三规则分支。不变量：`way` 判据不含 `y` 而搬运停止判据含 `y`（首位置也测）；`qu` 整体搬运；大小写只动两个位置（原词首字母转小写、新首字母按原大写转回）；全大写词后缀大写。
- **测验计分 `Score`**：对错两数。不变量：饱和加；总数为零时百分比为零；减法截零；除法除零得零。
- **横幅宽度**：一到整幅宽。不变量：零与超限报超范围；非数字报参数无效；列缩放端点恒等。
- **打孔卡 `bcd.rs`**：孔位表逐字节十二位。不变量：表按 `bcd.c` 逐值转写；渲染恒为十四行框；文本先截断四十八列再大写。
- **纸带 `ppt.rs`**：十一列线形（八数据位夹进纸孔）。不变量：编码位序自高到低、进纸孔在位 3 与位 2 之间；读带以进纸孔为锚、非空格即孔、越界列视为空白。

### 4.3 函数一览

| 函数 | 输入 | 输出 | 对应 C 行为 |
|------|------|------|------------|
| `verdict` | 词、是否来自标准输入 | 判定（值／跳过／三句拒绝） | 取值与诊断语义 |
| `trial_divide` | 数值、缓冲 | 因子数 | 试除语义 |
| `render_factors` | 数值、因子、缓冲 | 行长度 | 行形状语义 |
| `sieve` | 下界、上界、缓冲 | 素数个数 | 区间生成语义 |
| `parse_rotation` | 词 | 轮转量 | 轮转解析语义 |
| `rotate_line` | 输入、轮转量、缓冲 | 无 | 轮转语义 |
| `code_for_byte` | 字节 | 码串（空白得空码、表外得无） | 编码查表语义 |
| `show_line` | 码串、形态、缓冲 | 行长度 | 两种渲染形态语义 |
| `decode_code` | 码串 | 字节（结束记号无输出） | 整串解码语义 |
| `decode_stream` | 字节流、写出回调 | 无 | 解码字符流语义 |
| `pig_word` | 词、缓冲 | 词长 | 猪拉丁语义 |
| `number_words` | 数值、缓冲 | 词长 | 数词语义 |
| `render_banner_row` | 字形源、文本、行号、缓冲 | 行长度 | 横幅行语义 |
| `grade` | 计分、题目、答案 | 对错 | 计分语义 |
| `hole_mask` | 字节 | 十二位孔码 | `holes[256]` 查表语义 |
| `render_card` | 行、缓冲 | 长度 | `printcard` 语义 |
| `punch_byte` | 字节、缓冲 | 行长度 | `putppt` 语义 |
| `decode_line` | 带行 | 字节或失步 | `getppt` 语义 |

### 4.4 命令契约与 Requires

行为判定沿用 `99-global-concepts.md` §2 的规则：以 POSIX 为准绳、以 Minix3 C 实现为真值，C 实现偏离处在本表"当前状态"列显式标注（如 primes 的界限封顶、arithmetic 的播种来源）。依赖面按 §1 分层契约只含 `minix-sys` 顶层与 `minix-rt`：

| 命令 | Requires（最小 API） | 当前状态 |
|------|---------------------|---------|
| bcd | `read`、`write`、`exit`、argv 交接 | 已接线（`src/bin/bcd.rs`） |
| ppt | 同上 | 已接线（`src/bin/ppt.rs`，编码与 `-d` 解码两路） |
| factor | 同上 | 已接线（`src/bin/factor.rs`）：操作数与标准输入两形都经 `support::stdin_chunk` 走真标准输入，`pr_fact` 写下 `value:` 后逐个追加「空格 + 因子」、行尾一个换行的形状端到端可判；取值判定 `verdict` 与三句 `errx` 报文按 C 落地（前导空白、符号、行末校验、0/1 与负数分句、`-x` 报 usage 且退 0）；仅溢出那句因本树无 `strerror` 表而按非法格式句给出（退码同 C，已声明） |
| primes | 同上 | 已接线；界限封顶 1,000,000（试除引擎定位），C 的近 2^32 流式筛待后续批 |
| caesar | 同上 | 已接线固定轮转路；无参频率破解（`guess_and_rotate`）待决定半 |
| morse | 同上 | 已接线编码面（默认逐符号 `dit`/`daw`、`-s` 短形、每字节一行、操作数收尾行与整轮一次 `SK`、表外字节不出行、`-d`/`-s` 聚簇与 `--`/裸 `-` 的 getopt 形、用法行落 stderr 退 1）与 `-d` 的解码两形：操作数整串比对不拆空格（`:146-150`），标准输入按 C 的字符流（冲刷型分隔符不打印、十个符号算胡码并吞掉其后首字节、整轮收尾一个换行、查不到答 `x`、结束记号静默） |
| pig | 同上 | 已接线并逐枚对 C：词是连续字母串（数字标点冲刷并透出）、悬挂词在 EOF 不打印、第 1025 个字母触发 `ate too much!`、操作数被忽略而 `-x` 报 usage 退 1；`pig_word` 复刻 `pigout()` 的两套元音集合、`qu` 整体搬运与「大小写跟着字母走」；端到端门 `tests/pig_face.rs` 六枚 |
| number | 同上 | 已接线 |
| arithmetic | 同上 + 时钟读（播种） | 已接线；`penalise` 罚则提示与用时统计待批 |
| banner | 同上 + 真位图字体资产 | 决定半已测，薄壳待字体后接线 |

---

## 5. 测试要点

`cargo test -p minix-stdio-games`：**102 个测试，全部通过**（库 48 加九个二进制壳的行缓冲与格式化各 6 个；截至 2026-09-17）。

重点行为与测试的对应（以下函数名均可用 `rg "fn 测试名" os/commands/games/stdio-games` 复现）：

- **分解**（`factor.rs`，6 个）：`verdicts_accept_signs_and_blanks_the_way_strtoul_does`（符号、前导空白与行末判定）、`verdicts_split_the_three_sentences_by_path_and_magnitude`（三句拒绝按来路分）、`test_small_factoring`（小数、素数、一）、`test_large_prime_beyond_table`（表外大素数与倍数）、`test_line_shape`（行形状）、`test_small_buffer_rejected`（缓冲过小）。
- **素数**（`primes.rs`，4 个）：`test_first_primes`（前二十内八素数）、`test_inverted_interval_empty`（倒置区间空）、`test_single_values`（单点素数与合数与二）、`test_bounds_checked`（界限与缓冲）。
- **轮转**（`caesar.rs`，5 个）：`test_rot_thirteen_round_trip`（十三轮转往返）、`test_zero_and_full_turns`（零与整圈）、`test_non_letters_pass_through`（非字母直通）、`test_rotations_parse`（解析与溢出）、`test_length_mismatch_rejected`（不等长拒绝）。
- **摩斯**（`morse.rs`，12 个）：`the_default_form_spells_each_symbol_and_reads_any_non_dot_as_a_daw`（默认形态与非点即划）、`the_short_form_prints_the_run_after_one_space_and_even_a_blank_run_keeps_it`（短形与空码那一个空格）、`whitespace_is_the_empty_code_and_vertical_tab_counts_as_whitespace`（分隔符即空码，含 0x0B）、`punctuation_comes_from_the_other_table_and_unlisted_bytes_print_nothing`（十二项标点与表外字节不出行）、`whole_string_lookups_answer_letters_digits_punctuation_noise_and_the_silent_mark`（整串比对与静默的结束记号）、`one_separator_between_groups_prints_nothing_and_a_trailing_group_is_dropped`（冲刷型分隔符与末尾悬挂码组）、`ten_symbols_are_gibberish_and_the_swallow_eats_the_separator_behind_them`（十个符号算胡码并吞掉其后首字节）、`bytes_that_are_neither_symbols_nor_separators_are_ignored_while_nothing_is_pending`（既非符号也非分隔符且无悬挂时静默）、`test_case_insensitive`（大小写不分）、`test_digits_present`（数字）、`unlisted_bytes_stay_silent_on_the_encode_side`（编码侧表外字节静默、解码侧答 `x`）、`a_short_buffer_is_refused_rather_than_truncated`（缓冲不足即拒绝）。另有端到端门 `tests/morse_face.rs` 十二枚金样（真进程、真管道，逐枚按 `morse.c` 的 `show()` 与操作数循环手推）。
- **文字**（`words.rs`，8 个）：`test_vowel_start_gains_way`（元音开头加 `way`）、`test_consonant_cluster_moves`（辅音串搬运，含 `yellow` 不搬运）、`a_leading_y_never_moves_because_the_stop_set_contains_it`（首字母 `y` 一步不搬）、`the_qu_unit_moves_together`（`qu` 整体搬运，含 `QQUAKES` 的第二个 `q`）、`case_travels_with_the_letters_rather_than_being_normalized`（`Pig`/`String`/`sTring`/`STRING` 四例）、`test_capitalization_kept`（`Apple`/`APPLE`）、`test_number_words`（零到千）、`test_bad_words_rejected`。
- **猪拉丁语流**（端到端门 `tests/pig_face.rs`，6 个）：`a_word_is_a_run_of_letters_and_every_other_byte_passes_through`（字母成词、其余透出）、`a_pending_word_at_end_of_input_is_never_printed`（EOF 悬挂词丢弃）、`the_two_vowel_sets_decide_which_words_move_at_all`、`case_travels_with_the_letters_rather_than_being_normalized`（同名的进程级对照）、`one_thousand_twenty_four_letters_fit_and_the_next_one_is_fatal`（1024/1025 边界）、`operands_are_counted_and_ignored_while_a_bad_option_answers_usage`。
- **横幅**（`banner.rs`，5 个）：`test_widths_parse`、`test_scrunch_endpoints`（端点恒等）、`test_block_renders_framed`（框形）、`test_empty_source_misses`、`test_bad_row_rejected`。
- **测验**（`quiz.rs`，4 个）：`test_ranges_parse`、`test_answers_computed`（四则与截零）、`test_grading_counts`、`test_empty_score_zero`。
- **打孔卡**（`bcd.rs`，7 个）：`test_frame_geometry`（十四行框与列数）、`test_hole_marks_against_the_table`（孔行与表一致）、`test_lowercase_becomes_uppercase`（渲染前大写）、`test_truncation_at_48_columns`、`test_trailing_newline_is_dropped`、`test_newline_inside_text_terminates_the_line`、`test_oversized_output_reports_out_of_range`。
- **纸带**（`ppt.rs`，6 个）：`test_punch_line_shape`、`test_punch_all_zero_and_all_one`、`test_punch_reports_small_output`、`test_decode_reverses_punch`（编码解码往返）、`test_decode_line_without_feed_hole_is_none`（失步）、`test_decode_treats_anything_but_blank_as_hole`。
- **因数分解**（端到端门 `tests/factor_face.rs`，8 个）：`operands_print_one_line_each_in_the_c_shape`（操作数逐值一行与素数自身）、`standard_input_supplies_one_value_per_line_and_matches_the_operand_run`（标准输入逐行取值、与操作数形同结果）、`an_unterminated_last_line_is_still_a_value`（末行无换行仍是值）、`a_value_below_two_stops_the_run_with_c_sentence_on_stderr`（≤ 1 停止整轮并报 C 句）、`leading_blanks_belong_to_the_number_and_a_blank_only_line_is_silent`（缩进属于数、纯空白行静默）、`the_two_minus_sentences_depend_on_which_loop_saw_the_value`（两句负数报按来路分）、`trailing_junk_is_reported_as_an_illegal_numeric_format`（拖尾垃圾报非法格式并点名）、`an_option_like_argument_answers_the_usage_line_and_leaves_with_zero`（`-x` 报 usage 且退 0）。
- **行缓冲**（`bin_support.rs`，随九个壳各 6 个）：`test_lines_include_newline_and_split`、`test_final_fragment_without_newline_is_a_line`、`test_oversized_input_returns_full_buffer_lines`（满缓冲即行）、`test_empty_input_yields_no_lines`、`test_reader_failure_ends_stream_with_error`、`test_utoa_shapes`。

尚未覆盖、随后续阶段补齐的：大数概率分解（试除下游）、factor 溢出那句 `err` 的 `strerror` 后缀（现按非法格式句给出，退码同 C）、caesar 无参频率破解（`guess_and_rotate`）、arithmetic 罚则提示（`penalise`）与用时统计、banner 真位图字体与薄壳接线、primes 近 2^32 流式筛（现封顶 1,000,000，见 §4.4 标注）。十个玩法决定半与九个二进制壳是完整的，其余是显式留白的。

---

## 6. 过渡：说得清之后，去动屏幕

本篇走完了说话：分解、列举、轮转、电码、文字、大字、测验。系统已经能陪人玩文字游戏。

但屏幕还静着：虫子没爬，雨没下，方块没掉，贪吃蛇没吃，地牢没探。这是 `23-terminal-games.md` 的职责（终端控制游戏）。请沿因果链继续向下走：先会说话，再会动屏幕。

---

## 7. 参见

- `06-file-ops.md`——标准输入输出（本篇的前置）
- `23-terminal-games.md`——终端游戏（下一步：动屏幕）
- `minix3/games/factor/factor.c:main（L184，工具生成）`——试除与用法（`factor.rs` 的形状来源）
- `minix3/games/caesar/caesar.c:init_rottbl`——建表与解析（`caesar.rs` 的形状来源）
- `minix3/games/morse/morse.c:__COPYRIGHT（L96，工具生成）`——码表与匹配（`morse.rs` 的形状来源）
- `minix3/games/pig/pig.c:pigout（L103，工具生成）`——元音规则（`words.rs` 的形状来源）

---

## 附：验证记录（评审用，可跳过）

- `rg -n "pr_fact|pollard_rho|usage: factor" factor.c` → 第 184、110、268 行命中。
- `rg -n "init_rottbl|rot %=|get_rotation" caesar.c` → 第 82、86、125 行命中。
- `rg -n "morse\[|usage: morse|strcmp\(other" morse.c` → 第 96、140、219 行命中。
- `rg -n "as a vowel|usage: pig" pig.c` → 第 103、133 行命中。
- `rg -n "rangemax|nright|Specify a range" arithmetic.c` → 第 99、100、107 行命中。
- `rg -n "DWIDTH|usage: banner" banner.c` → 第 58、1057 行命中。
- `cargo test -p minix-stdio-games` → 35 通过、0 失败；`cargo clippy` 无警告。
- 本文档引用的 `file:line` 均来自正文写作前实际执行的 `rg -n` 输出，非凭记忆书写。
