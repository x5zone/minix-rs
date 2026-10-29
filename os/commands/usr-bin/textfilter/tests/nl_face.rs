//! End-to-end face for the `nl` binary: real process, real pipes, checked against
//! `minix3/usr.bin/nl/nl.c` (the option loop `:149-233`, the operand guard
//! `:239-249`, `parse_numbering` `:340-373` and `usage()` `:400-405`) plus the
//! libc `getopt` it drives (`minix3/lib/libc/stdlib/getopt.c:101-104`, `:132-136`).
//!
//! 两条出口形状要分清，这是本枚门的全部要点：**getopt 自己**报的错（集外字母、带值选项
//! 缺值）是「libc 那一行 + 命令的 usage 两行」；**命令自己**的 `errx` 只有一行且不接
//! usage。旧写法把两者都压成一行自创话，也没有 getopt 的逐字母扫描（`-ba`、`-pb t` 都会
//! 掉进集外字母那支）。

mod face_support;

use face_support::write_stdin;
use std::process::{Command, Stdio};

const NL: &str = env!("CARGO_BIN_EXE_nl");

/// C `usage()` 的两行原文：第一行末尾是 `\n\t`，第二行以制表符缩进。
const USAGE: &str = concat!(
    "Usage: nl [-p] [-b type] [-d delim] [-f type] [-h type] [-i incr] [-l num]\n",
    "\t[-n format] [-s sep] [-v startnum] [-w width] [file]\n"
);

fn run(args: &[&str], input: &[u8]) -> (String, i32, String) {
    let mut child = Command::new(NL)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the nl binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// 与 `run` 同一条命令缝，但标准错误**逐字节**交回：`from_utf8_lossy` 把一个裸 `c3`
/// 渲染成三字节 U+FFFD，文本形分不清「一个字节」与「两个字节」这两种错法。
fn run_bytes(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(NL)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the nl binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 集外字母那一行的字母位是**一个裸字节**。期望逐形取自本轮新配的参照件
/// `tmp/nl-417/nl_ref`（`minix3/usr.bin/nl/nl.c` + Minix 的 `getopt.c` + 垫片，经同名软链
/// `nl` 起 ⇒ 程序名段与本壳一致；本族此前没有 Minix 血脉的 nl 参照件，批次三百零六那批
/// 读数是直接回 C 源推的）：`-é` 与 `-pé` 都是 `nl: unknown option -- ` + 单个 `c3` +
/// 换行再跟那**两行** usage、退 1；`-Zé` 停在 `Z`；反面的 `-dé` 里 `é` 是**值**（选项串
/// `"pb:d:f:h:i:l:n:s:v:w:"` 里只有 `p` 不带值），当正则用，退 0 且一句不报。
/// 这一支的环整个住在薄壳里（`src/bin/nl.rs:136` 的 `bytes[pos] as char`），没有可单测的
/// 库函数 ⇒ 取法半与渲染半都由这一枚面门承载（与批次四百零八的 `uniq` 同形，那轮的
/// 结论是「环不在库里就不硬造库门」）。
#[test]
fn the_libc_line_of_a_multibyte_flag_is_one_byte_for_nl() {
    let want: Vec<u8> = [
        b"nl: unknown option -- ".as_slice(),
        b"\xc3\n",
        USAGE.as_bytes(),
    ]
    .concat();
    for form in ["-\u{e9}", "-p\u{e9}"] {
        let (out, status, err) = run_bytes(&[form], b"a\n");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_bytes(&["-Z\u{e9}"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"nl: unknown option -- Z\n".as_slice(), USAGE.as_bytes(),].concat(),
        "环停在第一个坏字母上，第二个字节根本不该被读到"
    );

    let (out, status, err) = run_bytes(&["-d\u{e9}"], b"a\n");
    assert_eq!(status, 0, "带值字母把两字节整个当值收走，退码是成功的");
    assert_eq!(err, Vec::new(), "那一形不报案：{err:?}");
    assert_eq!(
        out,
        b"     1\ta\n".to_vec(),
        "分隔符正则用不上就不改行（参照件同形）"
    );
}

/// 集外字母：libc 先印 `nl: unknown option -- <字母>`（`getopt.c:101-104`），随后
/// `case '?': default: usage()`（`nl.c:230-233`）——三行，退 1。
#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], b"a\n");
    assert_eq!(out, "", "报文只走 stderr");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, format!("nl: unknown option -- Z\n{USAGE}"));
}

/// 带值选项缺值：libc 的另一句（`getopt.c:132-136`），同样接 usage。旧写法把这一支当成
/// 「值非法」，用的是被拒值自己的句子，等于把两支护成了同一支。
#[test]
fn a_value_taking_letter_without_its_value_prints_the_libc_line() {
    for (letter, args) in [('b', &["-b"][..]), ('w', &["-w"][..]), ('n', &["-n"][..])] {
        let (out, status, err) = run(args, b"a\n");
        assert_eq!(out, "");
        assert_eq!(status, 1, "{args:?} 的 stderr 是 {err:?}");
        assert_eq!(
            err,
            format!("nl: option requires an argument -- {letter}\n{USAGE}"),
            "{args:?}"
        );
    }
}

/// `parse_numbering` 的句子带**段名**（`numbering_properties[]` 的 name 依次是
/// footer / body / header，`nl.c:73-75`），且把整个被拒的值打出来（`:366-369`）。
/// 只有一行，不接 usage。
#[test]
fn numbering_type_errors_name_their_own_section() {
    for (flag, name) in [('b', "body"), ('f', "footer"), ('h', "header")] {
        let (out, status, err) = run(&[&format!("-{flag}"), "q"], b"a\n");
        assert_eq!(out, "");
        assert_eq!(status, 1);
        assert_eq!(
            err,
            format!("nl: illegal {name} line numbering type -- q\n"),
            "-{flag} 的句子该带自己的段名"
        );
    }
}

/// getopt 的逐字母扫描：值可以附在本 token（`-ba`），字母可以聚簇（`-pb t`）。
/// 旧写法按整 token 匹配，这两形都会掉进集外字母那支。
#[test]
fn inline_and_clustered_letters_parse_like_getopt() {
    // 本枚门只管诊断面：编号输出的形状由 `src/nl.rs` 的库测负责，这里不拿二进制输出当金样。
    let (out, status, err) = run(&["-ba", "-w", "4"], b"x\n");
    assert_eq!((status, err.as_str()), (0, ""), "附值形该被接受");
    assert!(!out.is_empty(), "被接受就该有编号输出");
    let (out, status, err) = run(&["-pb", "t"], b"x\n");
    assert_eq!((status, err.as_str()), (0, ""), "聚簇形该被接受");
    assert!(!out.is_empty(), "被接受就该有编号输出");
}

/// `-w` 的三支各不同句：转不出数字是 `invalid width value -- <值>`，转出来但 ≤ 0 是
/// `width argument must be > 0 -- <值>`（`nl.c:220-228`，值也被打出来）。
#[test]
fn width_rejects_unparseable_and_nonpositive_with_different_sentences() {
    let (_, status, err) = run(&["-w", "x"], b"a\n");
    assert_eq!(
        (status, err.as_str()),
        (1, "nl: invalid width value -- x\n")
    );
    let (_, status, err) = run(&["-w", "0"], b"a\n");
    assert_eq!(
        (status, err.as_str()),
        (1, "nl: width argument must be > 0 -- 0\n")
    );
}

/// `-d` 收下前两个字符，第三个才拒（`nl.c:157-169`）；空值不动默认（两个
/// `if (optarg[n] != '\0')` 都不成立），所以 `-d ''` 是合法无操作。
#[test]
fn delim_takes_two_characters_and_refuses_a_third() {
    let (_, status, err) = run(&["-d", "abc"], b"a\n");
    assert_eq!(
        (status, err.as_str()),
        (1, "nl: invalid delim argument -- abc\n")
    );
    let (out, status, err) = run(&["-d", ""], b"x\n");
    assert_eq!((status, err.as_str()), (0, ""), "空值该是无操作");
    assert!(!out.is_empty(), "被接受就该有编号输出");
}

/// 其余数值支各报自己的句子，且都把被拒的值原样打出去（`nl.c:176-228`）。
#[test]
fn the_number_arguments_quote_the_rejected_value() {
    for (args, sentence) in [
        (vec!["-i", "2x"], "invalid incr argument -- 2x"),
        (vec!["-l", "2x"], "invalid num argument -- 2x"),
        (vec!["-v", "1x"], "invalid startnum value -- 1x"),
        (vec!["-n", "xy"], "illegal format -- xy"),
    ] {
        let (_, status, err) = run(&args, b"a\n");
        assert_eq!(status, 1, "{args:?} 的 stderr 是 {err:?}");
        assert_eq!(err, format!("nl: {sentence}\n"), "{args:?}");
    }
}

/// C 这里**没有**非零检查（只查 `*ep`），所以 `-i 0` 合法、退 0；旧写法自创过一句
/// 「increment must be non-zero」，那是把不存在的守卫当真。
#[test]
fn a_zero_increment_is_accepted() {
    let (_, status, err) = run(&["-i", "0"], b"a\n");
    assert_eq!((status, err.as_str()), (0, ""), "stderr 是 {err:?}");
}

/// 两个以上操作数是 `usage()` 那一支（`nl.c:246-248`），**不带** libc 那一行——它与
/// 集外字母的区别正是这枚门要钉的：同样是 usage，行数不同。
#[test]
fn more_than_one_operand_is_plain_usage_without_the_libc_line() {
    let (_, status, err) = run(&["f1", "f2"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(err, USAGE);
}

/// 操作数腿：读得动的档名，其内容要进标准输出并被编号。这一枚门是批次四百一十八补的——
/// 此前与操作数有关的两枚只钉「开不了报 `Cannot open …`」与「多于一个操作数是 usage」，
/// **没有任何一枚断言可读档名的编号输出**，于是 `src/bin/nl.rs` 里 `read_file` 成功那一侧
/// 的字节被丢弃这种缺陷可以静默通过（`nl file` 什么都不输出还退 0）。金样按参照件
/// `tmp/nl-417/nl_ref` 的读法：两行各得 `     1\t`/`     2\t` 前缀，退 0 且不报案。
/// 标准输入这里给空，输出仍有内容才算「内容确实来自档名」。
#[test]
fn a_readable_operand_is_numbered_into_stdout() {
    let path = format!("/tmp/nl-face-{}-operand.txt", std::process::id());
    std::fs::write(&path, b"alpha\nbeta\n").expect("the operand file should be writable");
    let (out, status, err) = run(&[path.as_str()], b"");
    let _ = std::fs::remove_file(&path);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(err, "", "正常编号不该报案");
    assert_eq!(
        out, "     1\talpha\n     2\tbeta\n",
        "档名内容必须进输出：读不到内容就是操作数腿被丢弃"
    );
}

/// 末行不带换行的档名：参照件 `tmp/nl-417/nl_ref` 打的是 `     1\tx`（**不补换行**），
/// 本壳在这一形上与参照件不同（`NL-FINAL-LINE-ADDS-NEWLINE` 在册待修，同 `head`/`tail`
/// 那一族），所以这枚门只钉「内容进了输出」这一件本笔修的事，末行的换行差另由登记跟踪。
#[test]
fn an_operand_without_a_trailing_newline_still_reaches_the_output() {
    let path = format!("/tmp/nl-face-{}-noeol.txt", std::process::id());
    std::fs::write(&path, b"x").expect("the operand file should be writable");
    let (out, status, err) = run(&[path.as_str()], b"");
    let _ = std::fs::remove_file(&path);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert!(
        out.starts_with("     1\tx"),
        "编号内容要来自档名，实际 {out:?}"
    );
}

/// 读不到的操作数走 `err(3)`：程序名、**带反引号的操作数**、`strerror` 的句子三件齐备
/// （`nl.c:244-246` 的 `err(EXIT_FAILURE, "Cannot open `%s'", argv[0])`），退 1。
#[test]
fn an_unreadable_operand_names_the_operand_and_the_number_sentence() {
    let (out, status, err) = run(&["/tmp/nl-face-definitely-absent"], b"a\n");
    assert_eq!(out, "");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert!(
        err.starts_with("nl: Cannot open `/tmp/nl-face-definitely-absent': "),
        "该是 err(3) 的三段形，实际 {err:?}"
    );
    assert!(
        !err.contains("cannot open input"),
        "自创短语顶替了 strerror 句子：{err:?}"
    );
}

/// 编号腿不给无尾换行的末行补换行。C 用 `fgets` 连着串尾换行一起读、再用
/// `printf("%s%s", sep, buffer)` 原样打出（`minix3/usr.bin/nl/nl.c:288`、`:340`），
/// 参照件 `tmp/nl-417/nl_ref` 现量 `     1\talpha\n     2\tbeta\n     3\tgamma`
/// （末尾无换行）。登记项 `NL-FINAL-LINE-ADDS-NEWLINE` 由此闭合；本支上一枚门刻意只断
/// `starts_with`（当时补换行是分叉），现在按逐字节断言钉住。
#[test]
fn an_unterminated_final_line_is_numbered_without_a_newline() {
    let (out, code, err) = run(&[], b"alpha\nbeta\ngamma");
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        ("     1\talpha\n     2\tbeta\n     3\tgamma", 0, "")
    );
}

/// 字节形 argv 的命令缝：`&str` 递不出裸 `ff`（`OsStr` 从 `str` 过来时那串已是合法 UTF-8）。
/// 两路输出都逐字节交回——`-d` 与 `-s` 的值直接决定「哪一行算分页符」与「编号与正文之间
/// 打什么」，文本形分不清一枚字节与三枚折叠字节。
fn run_argv(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    use std::os::unix::ffi::OsStrExt as _;
    let mut child = Command::new(NL)
        .args(args.iter().map(|word| std::ffi::OsStr::from_bytes(word)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the nl binary should be runnable");
    write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// 参数缝在 `nl` 上同时改到**判据、匹配与输出**三处，形状与 `join` 的 `-t` 同族而更密：
/// ①`nl.c:158-161` 的 `-d` 取 `optarg[0]`、`optarg[1]` 两个**字节**，第三个字节（`:163`）才落
///   `invalid delim argument -- %s`（`:165`）。文本缝之下那两枚裸字节各折成三枚
///   （`ef bf bd` ×2 ⇒ 值长六），从接受翻成拒绝——负控 L1（把壳侧 argv 退回
///   `args()` 的 lossy 取法）当场让本枚红，读数在 `tmp/nl-465/negctl-L1.log`。
/// ②匹配那一步也吃字节：输入 `\xff\xa1` 一行配 `-d<ff><a1>` 时整行是分隔符 ⇒ 切段、
///   不编号，随后那 `q` 行交回的是六枚空格加制表符（两侧逐字节同）。
/// ③`-s` 的值由 `nl.c:340` 的 `printf("%s%s", sep, buffer)` 原样打在编号与正文之间，
///   **它整个替换掉缺省的那个制表符**，所以 `1` 后面直接是那一枚 `ff`。
/// ④控制形 `-s<é>`（`c3 a9`，合法 UTF-8 的两枚字节）两侧都照原样打两枚——缝改成字节既不
///   放宽判据，也不把合法序列拆成单字节。
#[test]
fn a_raw_byte_delimiter_matches_the_line_and_a_raw_byte_separator_is_printed() {
    // ①②两枚裸字节作分隔符：接受，并且真的拿去匹配那一行。
    let (out, status, err) = run_argv(&[b"-d\xff\xa1"], b"\xff\xa1\nq\n");
    assert_eq!(
        status, 0,
        "两枚裸字节是合法的分隔符，不该被拒：stderr 是 {err:?}"
    );
    assert_eq!(
        out, b"      \tq\n",
        "分隔符行切段后不编号：六枚空格 + 制表符 + 正文"
    );
    // ③`-s` 的值进输出：编号之后直接是那一枚 `ff`，没有别的字符。
    let (out, status, err) = run_argv(&[b"-s\xff"], b"a\nb\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out,
        [
            b"     1".as_slice(),
            &[0xff],
            b"a\n     2".as_slice(),
            &[0xff],
            b"b\n".as_slice(),
        ]
        .concat(),
        "分隔串必须是 argv 原字节，不是折叠出的三枚"
    );
    // ④控制形：合法的两枚字节照原样出去。
    let (out, status, err) = run_argv(&[b"-s\xc3\xa9"], b"a\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out,
        [b"     1".as_slice(), &[0xc3, 0xa9], b"a\n".as_slice()].concat()
    );
    // ⑤单枚裸字节：`optarg[1]` 是 `\0` ⇒ 只换 `delim[0]`，第二字符留默认 `:`（源码判据，与
    //   库内 `test_delim_one_char_option` 同一族）。参照件在这一形报 `invalid delim argument`
    //   ——那是 `optarg[2]` 的越界读产物而不是判据本身（在册 `NL-DELIM-LENGTH-RULE` 待裁决），
    //   所以本段钉的是本壳按源码得到的形状，不与参照件对齐。
    let (out, status, err) = run_argv(&[b"-d\xff"], b"\xff:\nq\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"      \tq\n", "分隔符是 `ff` 加默认的 `:`");
}

/// 七句把值拼进报文的腿，值位全是 argv 原字节：`invalid delim argument -- %s`
/// （`nl.c:165`）、`invalid incr argument -- %s`（`:182`）、`invalid num argument -- %s`
/// （`:191`）、`illegal format -- %s`（`:203`）、`invalid startnum value -- %s`（`:214`）、
/// `invalid width value -- %s`（`:223`）、`illegal <段名> line numbering type -- %s`（`:393`）。
/// 七句都是 `errx`：**只有一行、不接 usage、退 1**，所以行数与值位在同一枚门里各钉一次。
/// 字母位那一枚由跨壳表 `no_command_aborts_on_a_nonutf8_option_letter` 承载，本枚不重复。
/// 注意非法类型那三支的首字节判据：C 的 `parse_numbering` 只 `switch (argstr[0])`
/// （`nl.c:362`），所以 `-b 'a<ff>'` 是**接受**的（本枚取的是首字节就非法的那一形）。
#[test]
fn the_seven_value_echoes_carry_the_argv_bytes() {
    let cases: [(&[&[u8]], &[u8]); 7] = [
        (
            &[b"-dabc\xff", b"2"],
            b"nl: invalid delim argument -- abc\xff\n",
        ),
        (
            &[b"-i", b"5\xff", b"2"],
            b"nl: invalid incr argument -- 5\xff\n",
        ),
        (
            &[b"-l", b"5\xff", b"2"],
            b"nl: invalid num argument -- 5\xff\n",
        ),
        (&[b"-n", b"l\xff", b"2"], b"nl: illegal format -- l\xff\n"),
        (
            &[b"-v", b"5\xff", b"2"],
            b"nl: invalid startnum value -- 5\xff\n",
        ),
        (
            &[b"-w", b"5\xff", b"2"],
            b"nl: invalid width value -- 5\xff\n",
        ),
        (
            &[b"-b", b"\xff", b"2"],
            b"nl: illegal body line numbering type -- \xff\n",
        ),
    ];
    for (argv, want) in cases {
        let (out, status, err) = run_argv(argv, b"a\n");
        assert_eq!(out, Vec::new(), "{argv:?} 的报文只走标准错误");
        assert_eq!(status, 1, "{argv:?} 的 stderr 是 {err:?}");
        assert_eq!(
            err, want,
            "{argv:?} 该是那一行且值位是原字节（少了 usage 那一行）"
        );
    }
    // 同一句在 `-f`/`-h` 上换的只是段名（`numbering_properties[]` 的 name 字段）。
    for (letter, name) in [("-f", "footer"), ("-h", "header")] {
        let (_out, _status, err) = run_argv(&[letter.as_bytes(), &[0xff][..], b"2"], b"a\n");
        assert_eq!(
            err,
            [
                format!("nl: illegal {name} line numbering type -- ").as_bytes(),
                &[0xff][..],
                b"\n".as_slice(),
            ]
            .concat(),
            "`{letter}` 那一句带的是自己的段名"
        );
    }
    // 首字节合法而尾巴带裸字节的那一形是**接受**的（C 只看首字符）——反面形钉在这里，
    // 免得把「非法类型」那句当成对所有坏尾巴的反应。
    // 这里不带操作数：`nl` 的唯一操作数是**档名**（不是 reps，与 `jot` 那类位置参数不同），
    // 递一个 `2` 进去会变成「开不了的档名」那句而先退掉。
    let (out, status, err) = run_argv(&[b"-ba\xff"], b"a\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"     1\ta\n");
}
