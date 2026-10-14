# 22-纯输入输出游戏

> **状态**: 已完成，等待评审收敛
> **定位**: 交付因果链的验收层第一组——只用标准输入输出说话的游戏，系统能跑、管道能通的证明
> **源码**: `minix3/games/factor/factor.c`（输出形状见头注释、因子打印函数见第 184 行附近、大数路径见第 110 行附近、用法 `factor [value ...]` 见第 268 行附近）、`minix3/games/primes/primes.c` 与 `pattern.c` 与 `pr_tbl.c` 与 `spsp.c`（区间素数表）、`minix3/games/caesar/caesar.c`（轮转表构造见第 82 行附近、溢出保护 `rot %= LETTERS` 见第 86 行附近、轮转解析见第 125 行附近）、`minix3/games/morse/morse.c`（码表见第 96 行附近、用法 `morse [-ds] [string ...]` 见第 140 行附近、解码匹配见第 219 行附近）、`minix3/games/pig/pig.c`（元音规则见第 103 行附近、用法 `pig` 见第 133 行附近）、`minix3/games/arithmetic/arithmetic.c`（缺省范围 10 见第 99 行附近、对错计数见第 100 行附近、范围选项见第 107 行附近）、`minix3/games/banner/banner.c`（整幅宽 132 见第 58 行附近、用法 `banner [-w width] [message]` 见第 1057 行附近、列缩放见第 1064 行到第 1066 行附近）、`minix3/games/number/number.c`（数字转英文单词）、`minix3/games/bcd/bcd.c`（大字显示）、`minix3/games/ppt/ppt.c`（纸带 punch 显示）
> **Rust 模块**: `os/commands/games/stdio-games`（库包 `minix-stdio-games`：`factor.rs`、`primes.rs`、`caesar.rs`、`morse.rs`、`words.rs`、`banner.rs`、`quiz.rs`、`bcd.rs`、`ppt.rs`，库测试 71 个；端到端门九扇 `tests/morse_face.rs` 十二枚、`tests/factor_face.rs` 十枚、`tests/primes_face.rs` 八枚、`tests/caesar_face.rs` 七枚、`tests/number_face.rs` 七枚、`tests/pig_face.rs` 六枚、`tests/ppt_face.rs` 六枚、`tests/bcd_face.rs` 六枚与 `tests/arithmetic_face.rs` 四枚）；同 crate `src/bin/` 薄壳九个已接线（bcd、ppt、factor、primes、caesar、morse、pig、number、arithmetic，各含 7 个行缓冲与错误面测试，连同上述库测试与端到端门共 200 个测试全过）；执行半经 `minix-sys` 顶层 `read`/`write`（宿主传输语义待 edge E-SYSCALL-SIGN）；`banner` 薄壳待真位图字体资产后接线（同名占位二进制壳已删除，见 todo.md §6.1 步骤 3）
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

数词把整个数**按三位一组**读出来，每读满一组就报一个尺度名（hundred、thousand、million 一路到 vigintillion，共二十二个，因此上限是六十五位），而每一组后面都跟一个属于自己的句点——所以 1234 是两行（`one thousand.` 与 `two hundred thirty-four.`），不是一行。小数部分先看第一个非零位，再用**位数**给它的量级命名（一位是 tenths、两位是 hundredths、更多由 `ten-`/`hundred-` 前缀加尺度名拼出），并在自己末尾再点一个句点。`-l` 把组末的句点换成空格、把整行的换行挪到最后，但不改量级名末尾那个句点。负号单独占一行。

### 1.4 大字与测验：横幅宽度与对错计数

横幅把消息打成大字，整幅光栅宽 132 列，`-w` 选项压窄，经典终端宽 80 列是常用值。压窄不是裁掉，而是按比例把输出列映射回光栅列，四舍五入，所以窄横幅的字是挤的不是缺的。宽度为零或超过整幅宽都拒绝。

算术测验把操作数限制在**零到范围（含端点）**，缺省十；答对加一，答错加一，百分比是答对除以总数。答错不是「重问一遍就算」：C 会把这道题的**右操作数**与「加乘时的左操作数 / 减除时的答案」各记一笔五个惩罚，此后的抽值在 `范围 + 活惩罚总数` 的跨度上取模，落在惩罚段里就命中链表——越靠后答错的数越先被命中，命中一次消耗一点，耗尽即从表上摘除。于是同一个难题会反复出现，直到答对为止，而提示语只出题时打一次。减法在零处截断（不够减算零），除法只出整除题（出题时保证整除），乘法用饱和乘防溢出。范围解析拒绝零与非数字。

---

## 2. C 源码分析

### 2.1 分解：输出形状与两条路

输出形状写在头注释里：数加冒号加因子，因子不递减。因子打印函数（第 184 行附近）从小因子找起，除尽就打印并继续除，遇到比表大的素数直接打印。大数走概率路径（第 110 行附近），本篇只到试除。用法行（第 268 行附近）说明无参数时从标准输入读数。

Rust 侧 `factor.rs` 的试除（小素数表起步、六步轮转续航、余数收尾）与小路逐项对应，行绘制（数加冒号加空格分隔）与输出形状逐项对应。取值那一条腿按 C 逐字坐实成 `verdict`：前导空白属于数（`isblank` 扫描 `:163`，`strtoul` 也跳同一集合）、符号可选（`:119-128`），数字之后只能直接遇到行末或换行，否则就是那句 `<值>: illegal numeric format.`（`:177`）。三句拒绝各归不同的来路且不可互换：`pr_fact` 的防火墙管 0 与 1（`:218-219`）；C 的一句话滑差让**以 `-` 开头的操作数**也答这一句（`:173-174`）；只有标准输入那条腿在空白扫描之后另有 `negative numbers aren't permitted.`（`:166-168`）；纯空白行则是静默跳过（`:164-165`）。选项面同样照 C：空 optstring（`:147`）使任何 `-x` 落到 `case '?'` 而报 usage，而 C 的 `usage()` 是以状态 **0** 退出的（`:266-270`）；`--` 是结束标记而非操作数，单独的 `-` 是操作数。唯一保留的差异是溢出：C 走 `err(1, "%s", str)` 带 `strerror` 后缀（`:123-124`），本树没有 strerror 表，于是退码同为 1、句子按非法格式给，这条差异在库注释与本文状态列都写明。

### 2.2 素数：区间生成

素数工具在上下界之间生成，倒置区间输出空。模式文件与素数表文件是加速手段，核心语义是区间里每个素数恰出现一次。Rust 侧 `primes.rs` 的逐候选试除与核心语义逐项对应，上界一百万是工作集上限，超了报超范围。

### 2.3 轮转：建表与解析

轮转表构造（`init_rottbl`，`caesar.c:81-97`）先把整张表填成恒等（`:88-89`）——这正是「非字母原样通过」的实现方式，也把 0x80 以上的字节留在恒等段——再只覆盖大小写两段字母，并且先 `rot %= LETTERS` 再填表（`:86`，注释写明是为了防整数溢出）。轮转解析（`get_rotation`，`:120-137`）用 `strtol`，因此前导空白与正负号都能解析出来，随后才用「空串或尾随垃圾」判非法、用 `<0` 与 `> INT_MAX` 判越界，三句都走 `err` 退出。无参数时走的是另一条腿 `guess_and_rotate`（`:141-187`）：先把公布的比例表整个做一次 `log` 变形（`:150-151`），再把输入的前 2048 个字节按字节值计数（`inbuf[2048]`、`obs[NCHARS]`），对 26 个候选逐一算点积——观测向量取某个字母两种大小写之和，权重表按 `(i + try) % 26` **反向**错位，所以「被右移 5 格的文本」答的是 21；比较用严格大于且第 0 个候选无条件占位（`:176-180`），平手时保留较早的那个。选出后先把这 2048 字节按新表打出，剩下的继续用同一张表流过（`:183-186`），因此窗口只决定投票、不决定翻译。执行侧同一条腿的三种退出也照 C：多参数时 usage 行落 stderr 退 1，坏值走 `err` 的句子 （`caesar: Bad rotation value ` 加原值再加一个单引号）——句尾的 strerror 文本（`Invalid argument` 与 `Numerical result out of range`）本树无表，只到引号里的原值为止，两种来路在决定半里仍然分开保留。Rust 侧 `caesar.rs` 的单字节轮转与建表语义逐项对应，行轮转要求等长（调用方保证缓冲），长度不对报参数无效。

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

数词的分组读法在 `number.c` 里分四层：`convert`（`:125-192`）扫描并拆出整数与小数，`unit`（`:194-227`）负责三位一组与尺度名，`number`（`:229-265`）读一组内的一到三位，`pfract`（`:267-283`）按**小数位数**量级命名。四条最容易被抹平的事实：组末句点使一个数出多行；小数先走到第一个非零位再决定说什么，但量级名用总位数；扫描器对空白的态度是**逐个吃掉行首的、拒绝行内的**；而那句拒绝报文 `illegal number: %s`（`:156`）打的是 C 手里剩下的整串——扫描只在遇到点时写一个终止符（`:149`），别处一个字节也不动，所以 `1.2 3` 只剩 `1`，而 `1e5` 和 `12-34` 照原样全打；在 `fgets` 那条路上连行尾换行也还在串里，于是 `12a` 的报文后面多一个空行。入口也没有选项魔法：`getopt(argc, argv, "l")`，其余字母报 usage 退 1，无操作数时按 `char line[256]` 的 `fgets` 读（**可用 255**），拿不到换行就是致命的 `line too long.`，第二件起前头打 `...\n`。三句拒绝都走 `errx`，程名由 `getprogname()` 来（`lib/libc/gen/verrx.c:59`），而 usage 行（`:288`）把 `number` 写死在字面量里——同一个命令的两种做法。

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
| `bcd.rs` | `bcd.c` `:87-120`（孔位表）、`:153-220`（`printcard`） | 打孔表、单卡渲染 |
| `ppt.rs` | `ppt.c` `:52`（EDGE）、`:98-170`（读带循环与 `getppt`）、`:130-145`（`putppt`） | 纸带行渲染、读带还原 |
| `bin_support.rs` | — | 各二进制壳共用：行缓冲（C `fgets` 形状）、十进制格式化、C 的 `err(3)` 那一行的渲染（`err_line` 转发 `minix_types::err_line`，形状见 `minix3/lib/libc/gen/verrc.c`；权威随句子表一起住在 `minix-types`）、宿主/真机接缝（`args`/`terminate`/`emit`/`warn`/`read_stdin`/`stdin_chunk` 各自双生；`stdin_chunk` 的失败面带着当场的 `errno`，好让读出错与读到尽头分得开） |
| `factor.rs` | `factor.c` `:119-128`、`:147-179`、`:196-252` | 取值判定、试除、行绘制 |
| `primes.rs` | `primes.c` 区间语义；界限解析对位 `strtoumax` 的三种失手（`_strtol.h:69` 的 `EINVAL`、`:157`/`:172` 的 `ERANGE`、`primes.c:140` 的 `*p != '\0'`） | `parse_bound`（返回 `u64` 与 `BoundError` 三形，不管上限）、`over_cap`（C 在 `main` 里的那支上限检查）、区间生成 |
| `caesar.rs` | `caesar.c` `:81-97`（建表）、`:120-137`（解析）、`:141-187`（频率破解） | 轮转解析、单字节轮转、破解用的计数与点积 |
| `morse.rs` | `morse.c` 三张码表（`:53-111`）、`morse()`/`show()`（`:231-261`）、`decode()` 与 `-d` 循环（`:146-228`） | 逐字节查表、两种渲染形态、整串解码、解码字符流（`decode_stream`） |
| `words.rs` | `pig.c` `:94-130`（`pigout`）、`number.c` `:112-298`（`convert`/`unit`/`number`/`pfract`） | 猪拉丁语、数词分组读法与量级命名 |
| `banner.rs` | `banner.c` 第 58 行与第 1057 行与第 1064 行附近 | 宽度解析、列缩放、字形源接口与行绘制 |
| `quiz.rs` | `arithmetic.c` `:95-102`（键集与范围）、`:191-266`（出题与答错的记账）、`:283-364`（罚则链表） | 范围解析、答案计算、计分、罚则链表与抽值 |

### 4.2 关键类型与不变量

- **小素数表 `SMALL_PRIMES`**：百以内二十五素数。不变量：升序；试除起点；表外由轮转接手。
- **摩斯码表 `MorseTable`**：编码查询加解码查询。不变量：静态表三十六条（字母二十六加数字十个）；空表一切查无；大小写不分。
- **字形源 `GlyphSource`**：五行字块查询。不变量：行号零到四；块渲染可打印字符恒有字形；空源一切查无。
- **猪拉丁语**：三规则分支。不变量：`way` 判据不含 `y` 而搬运停止判据含 `y`（首位置也测）；`qu` 整体搬运；大小写只动两个位置（原词首字母转小写、新首字母按原大写转回）；全大写词后缀大写。
- **测验计分 `Score`**：对错两数。不变量：饱和加；总数为零时百分比为零；减法截零；除法除零得零。
- **横幅宽度**：一到整幅宽。不变量：零与超限报超范围；非数字报参数无效；列缩放端点恒等。
- **打孔卡 `bcd.rs`**：孔位表逐字节十二位。不变量：表与 `bcd.c` 的 256 项逐值相同（脚本对过，零差异）；渲染恒为**十五行**一框（顶栏、文本行、十二打孔行、底栏）；文本先去掉第一个换行、截到四十八列、再就地大写，之后文本行与打孔行都用这份大写串查表——表的大小写两半确实不同（`'A'`=`0x900`、`'a'`=`0x880`），但下半永远查不到；无孔字符（可打印集里只有空格，控制字符也算）在文本行留空白；行填充按 `"   123456789"`，即前三行空白、第 3 到 11 行是 `1` 到 `9`。
- **纸带 `ppt.rs`**：十一列线形（八数据位夹进纸孔）。不变量：编码位序自高到低、进纸孔在位 3 与位 2 之间（`.` 先于位 2 的字符合成）；读带以进纸孔为锚、非空格即孔。越界列在本树按空白处理，而 C 是在 `char buf[132]` 里读 `p[-1]`..`p[-5]`——取到的是上一次 `fgets` 留在栈上的残值，进纸孔落在前五行时甚至读到数组之外（`ppt.c:153-167`）。那是未定义行为而非规则，不复现，只写明。

打孔与读带共用一张列映射表，它把「十一列线形」这句话变成可核对的常量（`os/commands/games/stdio-games/src/ppt.rs`）：

```rust
/// Column order of the eight data bits around the feed hole: `bit_at[k]`
/// is the byte bit carried by the tape column `dot + offset[k]`.
const BIT_AT_OFFSET: [(isize, u8); 8] = [
    (-5, 0x80), (-4, 0x40), (-3, 0x20), (-2, 0x10),
    (-1, 0x08), (1, 0x04), (2, 0x02), (3, 0x01),
];
```

它的角色是读带侧的坐标系：以进纸孔所在列 `dot` 为原点，八列各绑一个位。绑法必须与写入侧逐列对齐——`putppt` 让位序自高到低走，并在 `i == 2` 之前先打一个 `.`（`ppt.c:134-141`），于是点左边第五列正是 bit 7（0x80），点右边第一列是 bit 2（0x04），点两侧都不跨过去。写成「偏移 + 位」的常量数组而不是循环里的一步算式，是因为算式（`dot - 5 + k` 之类）只要少一个括号就把点也算成一列、整张带子的字节平移一位，而数组错了一项会立刻被往返测抓出。反过来推：若把 `.` 改成写在 bit 2 之后，偏移表要整体右移一列，读带侧不改就每个字节都错一位——这条不对称正是「进纸孔在位 3 与位 2 之间」要写成不变量的理由。

### 4.3 函数一览

| 函数 | 输入 | 输出 | 对应 C 行为 |
|------|------|------|------------|
| `verdict` | 词、是否来自标准输入 | 判定（值／跳过／三句拒绝） | 取值与诊断语义 |
| `trial_divide` | 数值、缓冲 | 因子数 | 试除语义 |
| `render_factors` | 数值、因子、缓冲 | 行长度 | 行形状语义 |
| `sieve` | 下界、上界、缓冲 | 素数个数 | 区间生成语义 |
| `parse_rotation` | 词 | 轮转量或两种失败 | `get_rotation` 语义（`caesar.c:120-137`） |
| `observe` / `best_rotation` / `guess_rotation` | 字节 / 计数表 / 字节 | 计数 / 胜出轮转量 / 轮转量 | 破解语义（`caesar.c:141-187`） |
| `code_for_byte` | 字节 | 码串（空白得空码、表外得无） | 编码查表语义 |
| `show_line` | 码串、形态、缓冲 | 行长度 | 两种渲染形态语义 |
| `decode_code` | 码串 | 字节（结束记号无输出） | 整串解码语义 |
| `decode_stream` | 字节流、写出回调 | 无 | 解码字符流语义 |
| `pig_word` | 词、缓冲 | 词长 | 猪拉丁语义 |
| `convert` | 一行字节、是否 `-l`、缓冲 | 词长或三种拒绝 | 数词整条腿语义（含 `unit`/`number`/`pfract` 的私有实现） |
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
| bcd | `read`、`write`、`exit`、argv 交接 | 已接线并逐枚对 C（`src/bin/bcd.rs`）：每操作数一张卡、无操作数时按 `fgets` 读入 `char cardline[80]` —— 即可用长度是 **79**，超长的物理行被切成 79 与其余两段各成一卡（本轮修正：此前按 80 切，第二卡少一个字符）；无任何选项解析，`-x` 也当文本渲染；端到端门 `tests/bcd_face.rs` 六枚 |
| ppt | 同上 | 已接线并逐枚对 C（`src/bin/ppt.rs`）：`EDGE` 首尾框带、每字节一行打孔、操作数**之间**补一个空白行而末尾不补、标准输入逐字节无行分组；`-d` 拒绝操作数、按 `fgets` 逐行读带，同步之前的无孔行当前导跳过，同步之后遇到无孔行就补上欠着的一个换行并立即结束（末字节本身就是换行时不欠）；读**出错**与读到尽头分两条路：前者按 C 走 `err(1, "fgets")`（`ppt.c:109-111` 的 `!feof(stdin)`）退 1，后者才补那个换行；`getopt("dh")` 逐字母聚簇，`-h` 与集外字母同路报 usage 退 1，`--` 被消费、裸 `-` 是操作数；读失败那句 `err(1, "fgets")` 因本树无 strerror 表而按普通结束处理（已登记） |
| factor | 同上 | 已接线（`src/bin/factor.rs`）：操作数与标准输入两形都经 `support::stdin_chunk` 走真标准输入，标准输入读**出错**走 C 的 `err(1, "stdin")`（`factor.c:157-161`：与读到尽头分开，后者安静退 0），`pr_fact` 写下 `value:` 后逐个追加「空格 + 因子」、行尾一个换行的形状端到端可判；取值判定 `verdict` 与三句 `errx` 报文按 C 落地（前导空白、符号、行末校验、0/1 与负数分句、`-x` 报 usage 且退 0）；溢出那条腿单独成句：`strtoul` 置 `ERANGE` 时 C 走 `err(1, "%s", str)`（`factor.c:123-126`），答 `factor: <原串>: Result too large or too small`（尾句取自本树的 `sys_errlist`），且它先于「行末只许换行」的检验（`:127`），所以带尾巴的超大数答这句而不提格式 |
| primes | 同上 | 已接线；界限封顶 1,000,000（试除引擎定位），C 的近 2^32 流式筛待后续批。诊断面由批次二百五十二逐枚接上（此前**一支都没有**，坏参数一律静默退 1）：`err(1, "%s", argv[i])` 两形（转不出数字 → `primes: <串>: Invalid argument`；值越范围 → 缀 `ERANGE` 的句子，`primes.c:138`/`:145`/`:160`）、`errx(1, "%s: illegal numeric format.")`（转出数字有尾巴，`errno` 未置，句末带句点，`:140`/`:147`）、`stop value too large (>N).`（`:149`，本树填自己的上限）、`start value must be less than stop value.`（`:171-172`）、`negative numbers aren't permitted.`（`:133`/`:155`，只在操数上触发——选项扫描在前，故 `primes -5` 落 usage 而不是这一支）。`usage()` 是硬写字面量（`:339-343`），本命令不跟 `argv[0]` 走，与 `rm`/`cp`/`mv` 相反。仍待批：`-d`/`-h` 两面输出（`:107-112` 与 `:255-256`/`:327-328`，现按拒收报出）、零操数从标准输入读数（`read_num_buf`，`:176-205`，含它自己的四支）、`strtoumax` base 0 的前缀与空白形 |
| caesar | 同上 | 已接线两条路：固定轮转（`caesar N`，非字母走恒等段、轮转量先对 26 取余）与**无参频率破解**（`observe`/`best_rotation`/`guess_rotation` 复刻 C 的 `log` 变形表、2048 字节窗口投票、严格大于与第 0 候选占位的平手规则）；端到端门 `tests/caesar_face.rs` 五枚。入口面也已按 C：`get_rotation` 的接受规则（吃前导空白与 `+`、上界 `INT_MAX`、负号算越界不算畸形）、坏值句子落 stderr、多参数时 usage 落 stderr 退 1。坏值那句现在带上了 C 的 strerror 句尾——句子取自本树的 `sys_errlist`（见 `minix-types` 的 `Errno::message`），因此 `EINVAL` 缀 `Invalid argument`、`ERANGE` 缀 `Result too large or too small`（Minix3 的措辞，不是 glibc 的）。流失败三面由批次二百五十三接上，且**分属两族**：写失手是 C 的 `putchar` 返 `EOF` 那一支（`caesar.c:103-107`、`:116-120`）——`err(EXIT_FAILURE, "<stdout>")` 立刻缀号退出（宿主拿 `/dev/full` 可复现：`caesar: <stdout>: No space left on device`）；而结尾两处看的是 stdio 的错标志，标志不带号，所以是 `errx`（`<stdin>` 在 `:205-208`）；`fflush` 后那支 `<stdout>`（`:211-214`）在本树无对位——本壳逐块直写、失败当场报，不等收尾冲刷（声明，不假造） |
| morse | 同上 | 已接线编码面（默认逐符号 `dit`/`daw`、`-s` 短形、每字节一行、操作数收尾行与整轮一次 `SK`、表外字节不出行、`-d`/`-s` 聚簇与 `--`/裸 `-` 的 getopt 形、用法行落 stderr 退 1）与 `-d` 的解码两形：操作数整串比对不拆空格（`:146-150`），标准输入按 C 的字符流（冲刷型分隔符不打印、十个符号算胡码并吞掉其后首字节、整轮收尾一个换行、查不到答 `x`、结束记号静默） |
| pig | 同上 | 已接线并逐枚对 C：词是连续字母串（数字标点冲刷并透出）、悬挂词在 EOF 不打印、第 1025 个字母触发 `ate too much!`、操作数被忽略而 `-x` 报 usage 退 1；`pig_word` 复刻 `pigout()` 的两套元音集合、`qu` 整体搬运与「大小写跟着字母走」；端到端门 `tests/pig_face.rs` 六枚 |
| number | 同上 | 已接线并逐枚对 C：三位一组带尺度名（hundred 到 vigintillion，六十五位封顶）、**每组自带句点**所以一个数可出多行、小数按位数命名量级、`-l` 换句点为空格、负号独立成行；入口按 `getopt("l")`，无操作数时 `fgets` 可用长度 255、无换行即 `line too long.`、第二件起前头 `...\n`；三句 `errx` 全落 stderr 且**程名取 `getprogname()`**（批次二百五十四；`lib/libc/gen/verrx.c:59`），其中 `illegal number: ` 后面是 C 手里剩下的整串——只有一个点会被写成终止符，无点就连行尾换行一起打（故 stdin 那条路上报文后面多一个空行），`1e5` 答 `1e5` 而不是 `1`；usage 行相反，C 把 `number` 写死在字面量里（`number.c:288`），改名也不跟着走。端到端门 `tests/number_face.rs` 七枚 |
| arithmetic | 同上 + 时钟读（播种） | 已接线并逐枚对 C：出题按 `problem()` 的形状抽值（减除从答案反推、除法造整除并埋余数、范围**含端点**）、答错走 `penalise` 链表记账（右操作数一律记，另按加乘记左操作数、减除记答案，各五个惩罚），抽值在 `范围 + 活惩罚` 跨度上取模并逐个消耗，同一题问到答对为止、提示语只打出题时一次；结束输入只打换行并退 0（分数只在每二十题与中断时打印）。诊断面由批次二百五十一按 C 接齐：三支选项错是 `errx(1, …)`（`:126`、`:132`、`:362`）**写标准错误**（本壳此前把这三行 `emit` 到了标准输出），`usage()` 是 `fprintf(stderr, "Usage: %s …", getprogname())`（`:378-385`，大写 `U`、名字嵌在串里；本壳此前是小写且把名字硬写）；C 的格式串自己以 `arithmetic: ` 开头，配上 `errx` 的程名前缀就成了 `arithmetic: arithmetic: invalid range.`——重复的名字是 C 的原文，照抄并登记为上游缺陷，不静默改好。EOF 处 C 在 `:148-150` 直接 `exit(0)` 而不调 `showstats`，所以「结束输入不打分数」是本壳与 C 一致，不是缺口。仍待批：`Total time N seconds; X.X seconds per problem` 的用时统计与 SIGINT 打印 |
| banner | 同上 + 真位图字体资产 | 决定半已测，薄壳待字体后接线 |

---

## 5. 测试要点

`cargo test -p minix-stdio-games`：**20 个测试目标、200 个测试，全部通过**——库测 71 个（九个决定半模块），九个二进制壳各带 7 个共享面测试（行缓冲、十进制格式化与 C 的 `err(3)` 句渲染），外加九扇端到端门 （`tests/morse_face.rs` 12、`tests/factor_face.rs` 10、`tests/primes_face.rs` 8、`tests/caesar_face.rs` 7、`tests/number_face.rs` 7、`tests/pig_face.rs` 6、`tests/ppt_face.rs` 6、`tests/bcd_face.rs` 6、`tests/arithmetic_face.rs` 4）。

重点行为与测试的对应（以下函数名均可用 `rg "fn 测试名" os/commands/games/stdio-games` 复现）：

- **分解**（`factor.rs`，6 个）：`verdicts_accept_signs_and_blanks_the_way_strtoul_does`（符号、前导空白与行末判定）、`verdicts_split_the_three_sentences_by_path_and_magnitude`（三句拒绝按来路分）、`test_small_factoring`（小数、素数、一）、`test_large_prime_beyond_table`（表外大素数与倍数）、`test_line_shape`（行形状）、`test_small_buffer_rejected`（缓冲过小）。
- **素数**（`primes.rs`，4 个）：`test_first_primes`（前二十内八素数）、`test_inverted_interval_empty`（倒置区间空）、`test_single_values`（单点素数与合数与二）、`test_bounds_checked`（界限与缓冲）。
- **素数界限的三条腿（进程级面）**（端到端门 `tests/primes_face.rs`，8 枚）：`a_bounded_run_prints_primes_and_stays_silent_on_stderr`（区间两端都算，stderr 一声不出）、`digits_with_trailing_junk_is_the_errx_line` 与 `a_word_that_cannot_convert_carries_the_number_sentence`（同一句坏参数的两种失手在 C 里分属两族：前者短语无句子、句末带句点；后者缀 `EINVAL` 的句子）、`an_inverted_interval_is_refused_by_phrase`、`a_stop_beyond_the_cap_names_the_cap`、`an_operand_starting_with_a_hyphen_is_the_negative_phrase`（`primes 5 -3` 走负号支，`primes -5` 走 usage）、`unknown_letters_and_extra_operands_answer_the_hardcoded_usage`（含未实现旗标的拒收支）、`the_usage_line_ignores_argv_zero_while_the_error_lines_do_not`（副本改名：报文行跟着改、usage 不改）。
- **轮转**（`caesar.rs`，9 个）：`test_rot_thirteen_round_trip`（十三轮转往返）、`test_zero_and_full_turns`（零与整圈）、`test_non_letters_pass_through`（非字母直通）、`rotations_parse_the_way_strtol_does_and_then_reject_by_range`（前导空白、符号、`INT_MAX` 与两种失败）、`each_failure_carries_the_errno_c_sets_and_minix3_words_it`（两种失败各带自己那个 errno，并缀 Minix3 表里的句子）、`the_weighted_table_is_the_log_form_of_the_published_frequencies`（权重表 = 比例表取对数再平移，且峰在 `e`、谷在 `j`）、`the_log_step_changes_the_answer_on_a_thin_alphabet`（同一组观测下对数表与原始比例表给出不同答案）、`the_cracker_finds_the_shift_that_was_applied`（一段英文在 1/5/13/19/25 格位移后都能还原）、`a_single_letter_observation_picks_the_shift_that_puts_it_on_the_peak`（单字母观测把该字母搬到权重峰值那一格）。
- **轮转端到端**（`tests/caesar_face.rs`，7 个）：`a_rotation_shifts_letters_alone_and_folds_at_the_alphabet`（含 0x80/0xFF 恒等段）、`the_argument_free_mode_guesses_the_rotation_and_prints_the_plaintext`（无参模式逐格还原）、`only_the_first_two_thousand_forty_eight_bytes_vote_and_all_of_them_rotate`（窗口外不投票、但仍被翻译）、`the_rotation_argument_is_parsed_like_strtol_and_rejected_in_two_orders`（接受规则与两种失败各成一句报文）、`more_than_one_argument_answers_the_usage_line_on_stderr`（usage 落 stderr 退 1）、`a_failed_write_reports_the_stdout_leg_with_the_number`（`/dev/full` 当标准输出：缀号的 `err` 那一族）、`a_failed_read_reports_the_stdin_leg_without_a_number`（目录当标准输入：不缀号的 `errx` 那一族）。
- **摩斯**（`morse.rs`，12 个）：`the_default_form_spells_each_symbol_and_reads_any_non_dot_as_a_daw`（默认形态与非点即划）、`the_short_form_prints_the_run_after_one_space_and_even_a_blank_run_keeps_it`（短形与空码那一个空格）、`whitespace_is_the_empty_code_and_vertical_tab_counts_as_whitespace`（分隔符即空码，含 0x0B）、`punctuation_comes_from_the_other_table_and_unlisted_bytes_print_nothing`（十二项标点与表外字节不出行）、`whole_string_lookups_answer_letters_digits_punctuation_noise_and_the_silent_mark`（整串比对与静默的结束记号）、`one_separator_between_groups_prints_nothing_and_a_trailing_group_is_dropped`（冲刷型分隔符与末尾悬挂码组）、`ten_symbols_are_gibberish_and_the_swallow_eats_the_separator_behind_them`（十个符号算胡码并吞掉其后首字节）、`bytes_that_are_neither_symbols_nor_separators_are_ignored_while_nothing_is_pending`（既非符号也非分隔符且无悬挂时静默）、`test_case_insensitive`（大小写不分）、`test_digits_present`（数字）、`unlisted_bytes_stay_silent_on_the_encode_side`（编码侧表外字节静默、解码侧答 `x`）、`a_short_buffer_is_refused_rather_than_truncated`（缓冲不足即拒绝）。另有端到端门 `tests/morse_face.rs` 十二枚金样（真进程、真管道，逐枚按 `morse.c` 的 `show()` 与操作数循环手推）。
- **文字**（`words.rs`，11 个）：`test_vowel_start_gains_way`（元音开头加 `way`）、`test_consonant_cluster_moves`（辅音串搬运，含 `yellow` 不搬运）、`a_leading_y_never_moves_because_the_stop_set_contains_it`（首字母 `y` 一步不搬）、`the_qu_unit_moves_together`（`qu` 整体搬运，含 `QQUAKES` 的第二个 `q`）、`case_travels_with_the_letters_rather_than_being_normalized`（`Pig`/`String`/`sTring`/`STRING` 四例）、`test_capitalization_kept`（`Apple`/`APPLE`）、`each_scale_group_ends_with_a_period_of_its_own`（每组自带句点、两位十位带连字符）、`the_fraction_is_named_by_how_many_digits_it_has`（量级按小数位数命名）、`linear_mode_trades_the_periods_for_spaces`（`-l` 的换行位置）、`refused_numbers_name_the_text_c_would_still_print`（三种拒绝与 65 位上限）、`test_bad_words_rejected`。
- **数词端到端**（`tests/number_face.rs`，7 个）：`every_scale_group_ends_with_a_period_of_its_own`（每组自带句点）、`the_fraction_is_named_by_how_many_digits_it_has`（量级按位数命名）、`an_operand_without_a_newline_is_a_line_too_long`（`fgets` 的 255 与 `...` 分隔）、`the_refusals_print_exactly_what_c_would_still_hold`（`illegal number:` 打到串尾，只有已被替换的点才截短它）、`a_refusal_on_the_read_path_carries_the_lines_own_newline`（stdin 那条路的报文带行尾换行、后面多一个空行）、`the_three_refusals_follow_argv_zero_while_the_usage_does_not`（副本改名：三句拒绝跟着走、usage 不跟）、`the_option_face_has_one_letter_and_no_getopt_magic`（`-l` 之外皆 usage）。逐字节金样另拿宿主编译的 `number.c` 参照进程对过 26 例（见 `../todo.md` 批次二百五十四）。
- **猪拉丁语流**（端到端门 `tests/pig_face.rs`，6 个）：`a_word_is_a_run_of_letters_and_every_other_byte_passes_through`（字母成词、其余透出）、`a_pending_word_at_end_of_input_is_never_printed`（EOF 悬挂词丢弃）、`the_two_vowel_sets_decide_which_words_move_at_all`、`case_travels_with_the_letters_rather_than_being_normalized`（同名的进程级对照）、`one_thousand_twenty_four_letters_fit_and_the_next_one_is_fatal`（1024/1025 边界）、`operands_are_counted_and_ignored_while_a_bad_option_answers_usage`。
- **横幅**（`banner.rs`，5 个）：`test_widths_parse`、`test_scrunch_endpoints`（端点恒等）、`test_block_renders_framed`（框形）、`test_empty_source_misses`、`test_bad_row_rejected`。
- **测验**（`quiz.rs`，10 个）：`test_ranges_parse`、`test_answers_computed`（四则与截零）、`test_grading_counts`、`penalties_charge_five_and_pay_off_in_insertion_order`（头插顺序与逐个消耗）、`an_entry_dies_when_its_penalty_is_paid_out`（五次之后条目消失）、`a_roll_past_the_end_of_the_list_is_the_inconsistency_c_reports`（越界即 C 的 bug 句）、`a_full_list_drops_the_new_penalty_as_c_drops_a_failed_malloc`（满表丢惩罚）、`the_draw_spans_the_range_plus_the_live_penalties`（取模跨度含活惩罚）、`a_wrong_answer_charges_the_right_operand_and_the_side_that_was_solved_for`（加乘记左操作数、减除记答案）、`test_empty_score_zero`。
- **打孔卡**（`bcd.rs`，8 个）：`test_frame_geometry`（十五行框与列数）、`test_hole_marks_against_the_table`（孔行与表一致）、`test_lowercase_becomes_uppercase`（渲染前大写）、`test_truncation_at_48_columns`、`test_trailing_newline_is_dropped`、`test_newline_inside_text_terminates_the_line`、`test_oversized_output_reports_out_of_range`、`the_shipped_table_keeps_q_and_r_apart_and_blanks_have_no_holes`（表把 Q 与 R 分开、空白类字符无孔）。
- **打孔卡端到端**（`tests/bcd_face.rs`，6 个）：`a_card_is_fifteen_lines_framed_by_the_two_rules`（十五行与两条边框）、`the_text_row_prints_only_characters_that_carry_a_pattern`（无孔字符留空白、制表符本身绝不出现在输出）、`the_twelve_rows_read_the_pattern_from_the_top_bit_down`（`A`=`0x900` 恰好第 0 与第 3 行打孔）、`the_text_is_upper_cased_and_cut_at_forty_eight_columns`、`a_line_longer_than_seventy_nine_characters_becomes_two_cards`、`there_is_no_option_parsing_so_a_leading_dash_is_text`。
- **纸带**（`ppt.rs`，6 个）：`test_punch_line_shape`、`test_punch_all_zero_and_all_one`、`test_punch_reports_small_output`、`test_decode_reverses_punch`（编码解码往返）、`test_decode_line_without_feed_hole_is_none`（失步）、`test_decode_treats_anything_but_blank_as_hole`。
- **纸带端到端**（`tests/ppt_face.rs`，6 个）：`the_encoder_frames_the_tape_and_punches_one_line_per_byte`（`EDGE` 框带与逐字节一行）、`a_punched_blank_separates_operands_and_none_follows_the_last`（操作数之间补空白、末尾不补）、`the_decoder_reads_back_what_the_encoder_punched_and_stops_at_lost_sync`（往返、失步即止、换行不欠）、`decode_refuses_operands_and_h_is_not_help`（`-d` 带操作数与 `-h`/`-x` 都报 usage 退 1）、`a_failed_read_leaves_with_c_fgets_sentence_and_exits_one`（读出错走 `ppt: fgets: Is a directory` 退 1；读到尽头才是补换行后安静退 0）、`double_dash_ends_the_options_and_a_lone_dash_is_an_operand`（`--` 与裸 `-` 的归属）。
- **因数分解**（端到端门 `tests/factor_face.rs`，10 个）：`operands_print_one_line_each_in_the_c_shape`（操作数逐值一行与素数自身）、`standard_input_supplies_one_value_per_line_and_matches_the_operand_run`（标准输入逐行取值、与操作数形同结果）、`an_unterminated_last_line_is_still_a_value`（末行无换行仍是值）、`a_value_below_two_stops_the_run_with_c_sentence_on_stderr`（≤ 1 停止整轮并报 C 句）、`leading_blanks_belong_to_the_number_and_a_blank_only_line_is_silent`（缩进属于数、纯空白行静默）、`the_two_minus_sentences_depend_on_which_loop_saw_the_value`（两句负数报按来路分）、`trailing_junk_is_reported_as_an_illegal_numeric_format`（拖尾垃圾报非法格式并点名）、`a_failed_read_leaves_with_c_stdin_sentence_and_exits_one`（读**出错**与读到尽头分两条路：目录描述符读出 `EISDIR`，得 `factor: stdin: Is a directory` 并退 1，空管道仍安静退 0）、`an_option_like_argument_answers_the_usage_line_and_leaves_with_zero`（`-x` 报 usage 且退 0）。
- **算术测验的报文通道**（端到端门 `tests/arithmetic_face.rs`，4 枚）：`an_invalid_range_is_an_errx_line_on_standard_error` 与 `an_unknown_key_is_an_errx_line_on_standard_error`（逐字含 C 那个重复前缀，且断言标准输出一个字节也不写）、`the_usage_line_is_capital_with_the_name_from_argv_zero`（大写 `Usage:` + 程名；多余操数也走这一支）、`the_quiz_face_writes_nothing_on_standard_error`（答题与提示语一行也不写标准错误）。分数行为何不在本门射程：C 在 `:148-150` 遇 EOF 直接 `exit(0)`，`showstats` 只在每二十题与 SIGINT 时跑。
- **行缓冲与错误面**（`bin_support.rs`，随九个壳各 7 个）：`test_lines_include_newline_and_split`、`test_final_fragment_without_newline_is_a_line`、`test_oversized_input_returns_full_buffer_lines`（满缓冲即行）、`test_empty_input_yields_no_lines`、`test_reader_failure_ends_stream_with_error`、`test_err_line_writes_c_shape_with_the_tables_sentence`（C 的 `err(3)` 那一行：程序名、标签、冒号空格、本表给的句子；表外的号走 `Unknown error: %u` 且按 `unsigned` 读数）。

尚未覆盖、随后续阶段补齐的：大数概率分解（试除下游）、arithmetic 的用时统计与 SIGINT 分数打印（需要秒级时钟与信号面，`%.1f` 的舍入还欠一个决定）、banner 真位图字体与薄壳接线、primes 近 2^32 流式筛（现封顶 1,000,000，见 §4.4 标注）。十个玩法决定半与九个二进制壳是完整的，其余是显式留白的。

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
