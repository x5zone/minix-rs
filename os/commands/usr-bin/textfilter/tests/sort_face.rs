//! End-to-end face for the `sort` binary: real process, real pipes and real files,
//! checked against `minix3/usr.bin/sort/sort.c` (its option loop, the operand
//! handling at `:266-290` and the output leg at `:320-379`).
//!
//! The output leg is the interesting part for a test: it does not just write a
//! file, it decides *how* — a fresh name is opened directly, an existing regular
//! file is replaced through a temporary whose name is never left behind, and an
//! unwritable one is refused before a single byte changes. All of that is
//! observable from outside the process, which is what these goldens assert.

mod face_support;

use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SORT: &str = env!("CARGO_BIN_EXE_sort");

fn run(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, String) {
    let mut child = Command::new(SORT)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the sort binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn written(tag: &str, bytes: &[u8]) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("sort-face-{}-{tag}", std::process::id()));
    fs::write(&path, bytes).expect("writing the scratch file");
    path
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

fn clean(path: &Path) {
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o644));
    let _ = fs::remove_file(path);
}

/// 本进程的**有效** uid（`/proc/self/status` 里 `Uid:` 那行的第二列）。
/// 拿不准就返 `None`，让调用方按普通用户那一支走。
///
/// 这一探针不是可有可无的：本仓的权威编译门跑在 docker `minix-ci:1.94` 里，而那个镜像
/// 默认以 root 执行（`docker run --rm minix-ci:1.94 id` → `uid=0(root)`）。root 无视
/// 权限位，所以下面那枚「只读目标」门在容器内与在宿主上看到的是**两种都合法**的行为。
fn effective_uid() -> Option<u32> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            return rest.split_whitespace().nth(1).and_then(|v| v.parse().ok());
        }
    }
    None
}

#[test]
fn without_the_flag_the_result_goes_to_standard_output() {
    let (out, status, err) = run(&[], b"c\na\nb\n");
    assert_eq!(out, b"a\nb\nc\n", "stderr was {err:?}");
    assert_eq!(status, 0);
    assert_eq!(err, "");
}

#[test]
fn a_name_that_does_not_exist_yet_is_written_directly() {
    // `lstat` fails, so C skips the temporary entirely and opens the name with "w"
    // (`sort.c:324-326`, `:352-355`).
    let target = written("fresh-target", b"placeholder");
    // Removing it is what puts the name in the state this branch cares about.
    fs::remove_file(&target).expect("the name must not exist");
    let input = written("fresh-input", b"c\na\nb\n");
    let (out, status, err) = run(&["-o", &text(&target), &text(&input)], b"");
    assert_eq!(out, Vec::<u8>::new(), "stdout must stay empty: {out:?}");
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(fs::read(&target).expect("the output exists"), b"a\nb\nc\n");
    clean(&target);
    clean(&input);
}

#[test]
fn an_existing_file_is_replaced_through_a_temporary_and_keeps_its_mode() {
    // The branch that needs the temporary: the old file's mode is copied onto it
    // before the names swap (`sort.c:366`), and the temporary is unlinked after the
    // link, so nothing is left beside the target (`:372-379`).
    let target = written("existing-target", b"stale line\n");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).expect("chmod");
    // The inode is what tells the two branches apart from outside: the temporary
    // leaves the name on a fresh inode, while opening the existing name with "w"
    // would reuse the old one after truncating it.
    let old_inode = fs::metadata(&target).expect("metadata").ino();
    let input = written("existing-input", b"b\na\n");
    let directory = target.parent().expect("a parent directory");
    let before: Vec<String> = fs::read_dir(directory)
        .expect("readable temp dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(&format!("sort-face-{}-", std::process::id())))
        .collect();
    let (out, status, err) = run(&["-o", &text(&target), &text(&input)], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(fs::read(&target).expect("the target"), b"a\nb\n");
    assert_eq!(
        fs::metadata(&target)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o7777,
        0o640,
        "the old file's permission bits must survive the replacement"
    );
    let written_meta = fs::metadata(&target).expect("metadata");
    assert_ne!(
        written_meta.ino(),
        old_inode,
        "an existing file must be replaced, not truncated"
    );
    let after: Vec<String> = fs::read_dir(directory)
        .expect("readable temp dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(&format!("sort-face-{}-", std::process::id())))
        .collect();
    let mut extra: Vec<&String> = after.iter().filter(|name| !before.contains(name)).collect();
    extra.sort();
    assert_eq!(
        extra,
        Vec::<&String>::new(),
        "a temporary name was left behind"
    );
    clean(&target);
    clean(&input);
}

#[test]
fn sorting_a_file_over_itself_works() {
    // Same name for input and output is the case the temporary exists for: the
    // operand is read first, and only then does the name move
    // (`sort.c:320-379` after the input loop at `:266-290`).
    let one = written("in-place", b"c\na\nb\n");
    let (out, status, err) = run(&["-o", &text(&one), &text(&one)], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(fs::read(&one).expect("still readable"), b"a\nb\nc\n");
    clean(&one);
}

#[test]
fn an_unwritable_target_is_refused_before_anything_changes() {
    // `access(outpath, W_OK)`（`sort.c:336-337`）那一支在 C 是 `err(2, "%s", outpath)`：
    // 程序名、路径、`strerror` 句子三件齐备（旧写法只印到路径为止），且退 2。
    // 旧内容必须还在。
    //
    // **这一判据只在非 root 成立**：带 `CAP_DAC_OVERRIDE` 的进程（root 默认有）走
    // `access(2)`/`open` 时不看权限位，只受只读挂载这类文件系统级限制约束。所以同一句
    // C 命令以 root 跑就会写出结果并退 0。权威容器门正是 root，故两支各自钉住自己的
    // 形——而不是把其中一支跳过。
    let target = written("read-only", b"keep me\n");
    fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).expect("chmod");
    let input = written("read-only-input", b"b\na\n");
    let (out, status, err) = run(&["-o", &text(&target), &text(&input)], b"");
    assert_eq!(out, Vec::<u8>::new());
    if effective_uid() == Some(0) {
        // root：`access` 放行 → 写成功，目标内容变成排好的结果。
        assert_eq!(status, 0, "root 下 access 放行，stderr 是 {err:?}");
        assert_eq!(err, "", "root 那一支不该发任何句子");
        assert_eq!(fs::read(&target).expect("written"), b"a\nb\n");
    } else {
        assert_eq!(status, 2, "stderr was {err:?}");
        assert_eq!(
            err,
            format!("sort: {}: Permission denied\n", text(&target)),
            "那句得把号对应的句子带上"
        );
        assert_eq!(fs::read(&target).expect("untouched"), b"keep me\n");
    }
    clean(&target);
    clean(&input);
}

#[test]
fn the_check_mode_never_opens_the_output() {
    // `-c` runs `order()` and exits before the output branch exists
    // (`sort.c:316-319`), so a disorder report must not create the named file and
    // must not overwrite an existing one either.
    let target = written("check-target", b"untouched\n");
    let input = written("check-input", b"b\na\n");
    let (out, status, err) = run(&["-c", "-o", &text(&target), &text(&input)], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1, "stderr was {err:?}");
    // 逐字而非 `contains`：这一枚之前只写 `contains("disorder")`，所以壳把那句自创成
    // `sort: disorder: ` 的时候它照样绿（批次二百九十二收严）。
    assert_eq!(
        err, "sort: found disorder: a\n",
        "那句得逐字对位 `msort.c:403`"
    );
    assert_eq!(fs::read(&target).expect("untouched"), b"untouched\n");
    clean(&target);
    clean(&input);
}

#[test]
fn operands_concatenate_and_a_missing_one_exits_two() {
    // Each operand is opened and read in argument order; standard input is then
    // ignored, because C only falls back to it when `optind == argc`
    // (`sort.c:306-312`). An unopenable operand is C's `access` failure at status 2
    // (`sort.c:284`).
    let one = written("operand-a", b"d\nb\n");
    let two = written("operand-b", b"c\n");
    let (out, status, err) = run(&[&text(&one), &text(&two)], b"ignored stdin\n");
    assert_eq!(out, b"b\nc\nd\n", "stderr was {err:?}");
    assert_eq!(status, 0);
    let missing = format!("sort-face-{}-missing", std::process::id());
    let (out, status, err) = run(&[&missing], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 2, "stderr was {err:?}");
    assert!(err.contains(&missing), "stderr was {err:?}");
    clean(&one);
    clean(&two);
}

#[test]
fn a_lone_dash_names_standard_input() {
    // C rewrites a `-` operand to `/dev/stdin` (`sort.c:270-281`); this shell reads
    // the already-open standard input, which is the same bytes.
    let (out, status, err) = run(&["-"], b"c\na\n");
    assert_eq!(out, b"a\nc\n", "stderr was {err:?}");
    assert_eq!(status, 0);
    // `-o` with no value at all is C's `?` and lands on the usage path, status 2.
    let (out, status, err) = run(&["-o"], b"a\n");
    assert_eq!(out, Vec::<u8>::new(), "stderr was {err:?}");
    assert_eq!(status, 2, "stderr was {err:?}");
    // 两行：libc 的那一句在前（`getopt.c:132-136`，字母不带引号），整个 usage 块在后。
    // 旧壳只印一句自创的 `sort: bad option`。
    assert_eq!(
        err,
        format!("sort: option requires an argument -- o\n{USAGE}")
    );
}

/// C `usage()` 的那一行（`sort.c:405-410`）：两条 `fprintf` 拼成**一行**，第二条以
/// 十三个空格开头（从 `sort.o` 的 `.rodata` 数出来的）。这里逐字钉住，间距也算。
const USAGE: &str = "usage: sort [-bcdfHilmnrSsu] [-k field1[,field2]] [-o output] [-R char] [-T dir]             [-t char] [file ...]\n";

#[test]
fn an_unknown_letter_prints_the_libc_line_then_the_usage() {
    let (out, status, err) = run(&["-Z"], b"a\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 2);
    assert_eq!(err, format!("sort: unknown option -- Z\n{USAGE}"));
}

#[test]
fn a_bad_key_gets_its_own_sentence_without_the_usage() {
    // `setfield`/`setcolumn` 那五支 `errx` 都**不接 usage**（`init.c:160-216`），与上面
    // 那两支正好相反；把退码与「有没有 usage 块」一起钉住才分得开。五句文本单源在
    // `sort.rs:KeySpecError::sentence`，而每一句用哪些形都是拿参照件
    //（`tmp/family-diag-ref/sort_ref`，由 `minix3/usr.bin/sort/` 真源链成）逐形量过的：
    // 旧写法把五句挤成一句 `illegal field descriptor`，所以四句都在拿别人的句子签字。
    for (spec, sentence) in [
        ("x", "field numbers must be positive"),
        ("0", "field numbers must be positive"),
        ("1,-2", "field numbers must be positive"),
        ("1.0", "illegal offset"),
        ("1,0.5", "cannot indent end of line"),
        ("1 2", "illegal field descriptor"),
        ("1n2", "illegal field descriptor"),
        ("2.5,1.2", "fields out of order"),
    ] {
        let (out, status, err) = run(&["-k", spec], b"a\n");
        assert_eq!(out, Vec::<u8>::new(), "键形 {spec:?} 不该打任何东西");
        assert_eq!(status, 2, "键形 {spec:?} 的 stderr 是 {err:?}");
        assert_eq!(
            err,
            format!("sort: {sentence}\n"),
            "键形 {spec:?} 该走那一句，而且不接 usage"
        );
    }
    // 反方向也要钉：C 接受的那些形不能报任何句子。`-k 2,1` 不报 `fields out of order`
    //（结束列没给 `.C` 时缩进是 0，`:214` 那个 `tcol.indent != 0` 不成立）；`-k 1,2.0`
    // 不报 `illegal offset`（减一只发生在开始列，`:166-167`）。旧台账把 `-k 2,1` 登记成
    // 「在 C 是 fields out of order」，本轮拿参照件推翻了那一句。
    for spec in ["2,1", "1,2.0", "1,2.", "1,0", "1b", "+1", "01"] {
        let (out, status, err) = run(&["-k", spec], b"a\n");
        assert_eq!(status, 0, "键形 {spec:?} 的 stderr 是 {err:?}");
        assert_eq!(err, "", "键形 {spec:?} 不该报句子");
        assert_eq!(out, b"a\n", "键形 {spec:?}");
    }
}

#[test]
fn the_length_flag_sorts_by_span_length() {
    // `-l` 排的是跳完前导空白后的「跨度字节数」（C `fields.c` 的 `length()`），不是内容。
    // 下面四行读数是拿参照件 `tmp/family-diag-ref/sort_ref` 在同一输入上量出来的：
    // 空行（0）、`b 2`（1）、`cc 3`（2）、`aaa 1`（3）。字典序与数值序都给不出这个序。
    let input = b"aaa 1\nb 2\ncc 3\n\n";
    let (out, status, err) = run(&["-l"], input);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"\nb 2\ncc 3\naaa 1\n");
    // 局部 `-k 1l` 与全局 `-l` 同序；`-l` 配 `-r` 就是它的倒序。
    assert_eq!(run(&["-k", "1l"], input).0, out);
    assert_eq!(run(&["-l", "-r"], input).0, b"aaa 1\ncc 3\nb 2\n\n");
    // `l` 与 `n` 同时给时按长度走（`enterfield` 先查 `L` 再查 `N`）：参照件上
    // `-k 1ln` 与 `-k 1nl` 的读数与 `-k 1l` 逐字节相同。
    assert_eq!(run(&["-k", "1ln"], input).0, out);
    assert_eq!(run(&["-k", "1nl"], input).0, out);
    // 补白不计入长度：`-t:` 下第一行「  aa:1」的字段一是 `  aa`，跳完两个空白是 2，
    // 所以它排在长度为 1 的 `b:2` 之后。
    let delimited = b"  aa:1\nb:2\nccc:3\n";
    let (out, status, err) = run(&["-t:", "-k", "1l"], delimited);
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out, b"b:2\n  aa:1\nccc:3\n",
        "参照件给的序就是 b:2、  aa:1、ccc:3"
    );
    // 反方向的一枚：不带 `-l` 时同一输入走字典序，带补白的 `  aa:1` 反而排在第一位。
    assert_eq!(run(&["-t:", "-k", "1"], delimited).0, delimited);
}

#[test]
fn numeric_sort_normalizes_signs_and_zeros_the_way_c_does() {
    // C 的 `number()` 把前导零吃掉、正号只跨过、小数尾零随 terminator 一起回退，
    // 所以这些形**同值**——同值就意味着并列键，而并列键保持输入序（批次二百九十的
    // 规则）。读数拿参照件 `sort_ref -n` 逐对量过（两向输入都原序输出）。
    // 旧写法只认负号且不归一小数尾零，`+7` 会被当成「一个数字也没转出来」而落到零，
    // 于是 `007`/`7`/`+7` 三行被拆成两个键——那时这里回退整行，看着像对的。
    for input in [
        b"007\n7\n".to_vec(),
        b"7\n007\n".to_vec(),
        b"+7\n7\n".to_vec(),
        b"1.5\n1.50\n".to_vec(),
        b"1.50\n1.5\n".to_vec(),
        b".5\n0.5\n".to_vec(),
        b"+0\n-0\n".to_vec(),
        b"12.0\n12\n".to_vec(),
        b"  12\n12\n".to_vec(),
    ] {
        let (out, status, err) = run(&["-n"], &input);
        assert_eq!(status, 0, "stderr 是 {err:?}");
        assert_eq!(out, input, "{input:?} 两行同值，必须保持输入序");
    }
    // 归一不能把量级弄丢：去零之后仍按位数与逐位比。
    let (out, _, err) = run(&["-n"], b"7\n6\n");
    assert_eq!(out, b"6\n7\n", "stderr 是 {err:?}");
    let (out, _, err) = run(&["-n"], b"0007\n0006\n");
    assert_eq!(
        out, b"0006\n0007\n",
        "前导零归一后仍要比出量级，stderr 是 {err:?}"
    );
    // C 不认指数：`1e3` 的键是 1（在 `e` 处停住），所以排在 `1000` 前。
    let (out, _, err) = run(&["-n"], b"1000\n1e3\n");
    assert_eq!(out, b"1e3\n1000\n", "参照件同读数，stderr 是 {err:?}");
    // 一个数字也没有的形按零算（C 的注释明写“\"0\", \"0.00\", \"000\" or \"fubar\" but sorts as 0”）。
    let (out, _, err) = run(&["-n"], b"abc\n0\n");
    assert_eq!(
        out, b"abc\n0\n",
        "两者都是零键，保持输入序，stderr 是 {err:?}"
    );
}

#[test]
fn equal_keys_keep_input_order_and_capital_s_switches_to_the_record() {
    // 并列键的规则（批次二百九十按参照件 `sort_ref` 逐形量过）。输入四行的键（第二列）
    // 全部相等，所以四种写法给的是四种读数，每一枚都只对得上自己那一支：
    let input = b"b 1\na 1\nc 1\nd 1\n";
    // 默认：C 不拿整条记录再排（`fields.c:168` 在 `posix_sort` 为零时把 `keylen` 收到
    // 只含键），所以并列键保持输入序——旧写法在这里回退到整行，给出的是 a b c d。
    assert_eq!(run(&["-k", "2,2"], input).0, input.to_vec());
    // `-S` 才是「按键排完再按整条记录排」（`sort.c:202-212`）。
    assert_eq!(run(&["-S", "-k", "2,2"], input).0, b"a 1\nb 1\nc 1\nd 1\n");
    // 全局 `-r` 连稳定序一起倒装（参照件 `-r -k 2,2` 给出 d c a b）。
    assert_eq!(run(&["-r", "-k", "2,2"], input).0, b"d 1\nc 1\na 1\nb 1\n");
    // `-u` 在选项环**之后**撤掉 `-S`（`:256-258`），所以两形都只留首行；旧写法因为总
    // 按整行拆平行，会把四行全留下——那是 `sort -u` 的反面。
    assert_eq!(run(&["-u", "-k", "2,2"], input).0, b"b 1\n");
    assert_eq!(run(&["-S", "-u", "-k", "2,2"], input).0, b"b 1\n");
    assert_eq!(run(&["-u", "-S", "-k", "2,2"], input).0, b"b 1\n");
    // `-c` 看的是同一把尺：默认四行不算失序（退 0、不打任何东西），开了 `-S` 之后
    // `a 1` 排在 `b 1` 之后就是失序（参照件 `errx(1, "found disorder: %s", …)`，
    // `msort.c:403`，退 1）。文案本轮（批次二百九十二）已按参照件逐字钉住，
    // 精确到「哪个程序名前缀 + 哪一个词」，见下面那枚独立门。
    let (out, status, err) = run(&["-c", "-k", "2,2"], input);
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(err, "");
    let (out, status, err) = run(&["-c", "-S", "-k", "2,2"], input);
    assert_eq!(out, Vec::<u8>::new(), "失序检查不该往标准输出打东西");
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, "sort: found disorder: a 1\n");
}

#[test]
fn the_two_check_reports_use_c_sentences_and_one_line_each() {
    // `-c` 那两句话逐字对位 C 的 `msort.c:order()`（参照件 `sort_ref` 同读数）：
    // 失序 `found disorder: <后一条>`（`:403`），`-u` 下键相等 `found non-uniqueness:
    // <后一条>`（`:405-407`）。两句都退 1、都不接 usage、都只打**一行**（C 在打印前把
    // 记录尾部的分隔符写成 `\0`，所以句尾那个换行是 `errx` 自己加的）。
    let (out, status, err) = run(&["-c"], b"b\na\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, "sort: found disorder: a\n");
    // 这两句都不是 usage 错：整个 stderr 就那一句，usage 块一个字也不许跟出来。
    assert_eq!(err.matches('\n').count(), 1, "stderr 是 {err:?}");
    let (out, status, err) = run(&["-c", "-u"], b"a\na\nb\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1, "stderr 是 {err:?}");
    assert_eq!(err, "sort: found non-uniqueness: a\n");
    // 先后：C 在同一趟比较里先看失序再看重复，所以 `b b a` 报的是先撞上的那对
    //（重复的 `b`），而不是后面那次失序——参照件同读数。
    let (_, status, err) = run(&["-c", "-u"], b"b\nb\na\n");
    assert_eq!(
        (status, err.as_str()),
        (1, "sort: found non-uniqueness: b\n")
    );
    // 反方向的一枚：没有 `-u` 时同样的输入不该冒出「non-uniqueness」那句话（退 0）。
    let (out, status, err) = run(&["-c"], b"a\na\nb\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(
        (status, err.as_str()),
        (0, ""),
        "相等键在无 -u 时既不失序也不报错"
    );
}

#[test]
fn a_second_delimiter_option_takes_the_message_plus_the_usage_block() {
    // `usage("multiple field delimiters")`（`sort.c:214-215`）：先 `%s: %s` 那一句，
    // 再接整个 usage 行。而多字节本身不是错（C 只取首字节）。
    let (out, status, err) = run(&["-t,", "-t;"], b"a\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 2);
    assert_eq!(err, format!("sort: multiple field delimiters\n{USAGE}"));
    let (out, status, err) = run(&["-t::", "-k", "1"], b"a:a\n");
    assert_eq!(out, b"a:a\n", "多字节分隔符取首字节，不该报错：{err:?}");
    assert_eq!(status, 0);
}

// —— 以下六枚钉 `-R`（记录分隔符）与它和 `-t` 的相互判据（批次三百零四）。金样逐形
// 从参照件 `tmp/family-diag-ref/sort_ref`（`minix3/usr.bin/sort/` 真源 + 垫片链成）
// 现测抄回：`-R ,` 把输入按逗号切成记录，最后一段保留它自己的换行；输出是
// 「每条记录 + 一个分隔符」，所以**以分隔符收尾**（`files.c:194`/`:215` 的 `putrec`）。

/// 记录切分与收尾形状：`b,a,d,c\n` 切成 `b`/`a`/`d`/`c\n`，排序后逐条补逗号。
#[test]
fn dash_r_splits_records_on_the_given_byte_and_ends_the_output_with_it() {
    let (out, status, err) = run(&["-R", ","], b"b,a,d,c\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"a,b,c\n,d,", "最后一条记录带着它内部的换行");
}

/// 尾随分隔符不留空记录（C 的 `getrec` 在 EOF 前读到零字节就返回 0）。
#[test]
fn a_trailing_record_delimiter_does_not_make_an_extra_record() {
    let (out, status, err) = run(&["-R", ","], b"b,a,d,");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"a,b,d,");
    let (out, status, _) = run(&["-R", ","], b"");
    assert_eq!(
        (out.as_slice(), status),
        (b"".as_ref(), 0),
        "空输入一条记录也没有"
    );
}

/// 单字节值是**字面字节**：`-R 0` 按字符 `0` 切，不是零字节；而 `\NNN` 形走八进制。
#[test]
fn dash_r_single_byte_is_literal_while_the_backslash_form_is_octal() {
    let (out, status, err) = run(&["-R", "0"], b"a0b0c\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"a0b0c\n0");
    let (out, status, err) = run(&["-R", "\\011"], b"b,a,d,c\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(
        out, b"b,a,d,c\n\t",
        "整串只有一个制表符分隔的记录，输出以制表收尾"
    );
}

/// `-u` 与 `-c` 走的都是同一批记录，所以去重也按切分后的记录算。
#[test]
fn dash_u_dedups_records_not_lines() {
    let (out, status, err) = run(&["-R", ",", "-u"], b"a,a,b\n");
    assert_eq!(status, 0, "stderr 是 {err:?}");
    assert_eq!(out, b"a,b\n,");
}

/// 两句 `errx`（`sort.c:217-218`、`:233-238`）：**只有这一行**、不接 usage、退 2。
/// `invalid` 那支带的是 C 当时手里的 `optarg`——反斜缀已被跳过。
#[test]
fn the_clash_and_invalid_legs_print_one_line_and_skip_the_usage_block() {
    let (out, status, err) = run(&["-R", ",", "-t", ","], b"a\n");
    assert_eq!(out.as_slice(), b"");
    assert_eq!(status, 2);
    assert_eq!(err, "sort: record/field delimiter clash\n");
    // 反向不查：只有 `-t` 那一支读掩码。参照件对这一形给 `a\n,b,`：换行在这一形里只是
    // 普通字节，所以它**留在记录内容里**，而输出以记录分隔符收尾。第一版把期望按「逗号
    // 分开两行」的直觉写成 `a,b,\n`，被本枚门与参照件一起挡下。
    let (out, status, err) = run(&["-t", ",", "-R", ","], b"b,a\n");
    assert_eq!(
        (out.as_slice(), status),
        (b"a\n,b,".as_ref(), 0),
        "stderr 是 {err:?}"
    );
    let (_, status, err) = run(&["-R", "300"], b"a\n");
    assert_eq!(status, 2);
    assert_eq!(err, "sort: invalid record delimiter 300\n");
    let (_, status, err) = run(&["-R", "\\q"], b"a\n");
    assert_eq!(status, 2);
    assert_eq!(err, "sort: invalid record delimiter q\n");
    let (_, status, err) = run(&["-t", "\n"], b"a\n");
    assert_eq!(status, 2, "默认记录分隔符就是换行");
    assert_eq!(err, "sort: record/field delimiter clash\n");
}

/// `multiple record delimiters` 走 `usage()`，所以是**两行**（一句带程名的话 +
/// 那个 usage 块）。参照件在宿主走到这一步会崩（它自己那份 `usage()` 依赖的垫片在
/// 宿主段错误，同 `join_ref` 那一坑），所以这一枚的形状按 C 源码文本钉：
/// `sort.c:399-412` 的第一条 `fprintf` 不带换行、第二条以十三个空格开头。
#[test]
fn a_second_stuck_dash_r_lands_on_the_usage_leg() {
    let (out, status, err) = run(&["-R", ",", "-R", "x"], b"a\n");
    assert_eq!(out.as_slice(), b"", "报文只走 stderr");
    assert_eq!(status, 2, "C 的 usage() 是 exit(2)");
    assert!(
        err.starts_with("sort: multiple record delimiters\nusage: sort "),
        "实际 stderr 是 {err:?}"
    );
    assert!(
        err.contains("[-R char]"),
        "usage 块里要带得上 `-R` 那一项：{err:?}"
    );
}
