//! End-to-end face for the `wtf` binary, checked against
//! `minix3/games/wtf/wtf`（注意真源是 POSIX shell script，不是 C：`file` 的读数与
//! 由此带来的判据差别见 `rewrite-notes/18-stage-commands/todo.md` 批次三百七十六）。
//!
//! 这一套门是补出来的：`wtf` 此前**没有任何端到端门**，所以两处让它对真库完全失效的
//! 缺陷一直没人发现——① 开档腿写死 `minix_sys::open`，宿主上真实存在的档也报
//! `cannot open`；② 记录分隔按冒号断词，而真库 `minix3/share/misc/acronyms` 的 557 行里
//! 只有 1 行含冒号（`$NetBSD:` 标识行），条目是 `10Q\tthank you` 这种制表符形。
//! 库内那枚直接对真库读数的门（`acronym.rs` 的 `the_shipped_acronym_database_is_readable_and_resolves`）
//! 与下面第一枚进程级门各钉一层。
//!
//! 写腿那一格**不在这里钉**：`wtf` 自己没有写失手的处理逻辑，报文与退码都由解释它的
//! shell 给（宿主实测 dash 是「一句 `echo: I/O error` 加退 0」，见批次三百七十六），
//! 那不是本壳能齐平的接口，账挂在 `TEXTGAMES-EMIT-ERRLEG` 的结论里。

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn wtf_bin() -> &'static str {
    env!("CARGO_BIN_EXE_wtf")
}

fn fixture(tag: &str, bytes: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("minix-wtf-face-{tag}.db"));
    std::fs::write(&path, bytes).expect("write wtf fixture");
    path
}

fn run(args: &[&str]) -> (String, i32, String) {
    let out = Command::new(wtf_bin())
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn wtf")
        .wait_with_output()
        .expect("wait wtf");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        out.status.code().expect("exit code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// 命中的一形：库能开、条目能解、打出「词: 释义」、退 0、stderr 空。
/// 这一枚同时是缺陷 ① 与 ② 的守卫——任一处退化都会让它变红。
#[test]
fn a_real_database_resolves_a_term_and_exits_zero() {
    let db = fixture(
        "hit",
        b"NASA\tNational Aeronautics and Space Administration\n",
    );
    let (out, code, err) = run(&["-f", &db.to_string_lossy(), "nasa"]);
    assert_eq!(
        (out.as_str(), code, err.as_str()),
        (
            "NASA: National Aeronautics and Space Administration\n",
            0,
            ""
        ),
        "脚本 `:75` 先把查询词转大写再输出，打印形态跟着大写化的 target 走"
    );
    let _ = std::fs::remove_file(&db);
}

/// 大小写不敏感是本壳的既有口径（脚本侧是把查询词转大写后再比对，效果在这条形相同）。
#[test]
fn the_lookup_ignores_case() {
    let db = fixture("case", b"Y2K\tYear 2000 problem\n");
    for (term, want) in [("y2k", "Y2K"), ("Y2K", "Y2K")] {
        let (out, code, _) = run(&["-f", &db.to_string_lossy(), term]);
        assert_eq!(
            (out.as_str(), code),
            (
                format!("{}: Year 2000 problem\n", term.to_ascii_uppercase()).as_str(),
                0
            ),
            "词条 {want}"
        );
    }
    let _ = std::fs::remove_file(&db);
}

/// 开不了的库：本壳句形 `wtf: <路径>: cannot open` 退 1。
/// （脚本那侧是 `wtf: cannot open acronym database file \`<路径>'`，两者尚未齐平，
/// 那笔账在 `WTF-SCRIPT-PARITY`，本枚门钉的是「报哪条路径、退码几」这一层。）
#[test]
fn a_missing_database_names_the_path_and_exits_one() {
    let db = std::env::temp_dir().join("minix-wtf-face-definitely-not-here.db");
    let _ = std::fs::remove_file(&db);
    let (out, code, err) = run(&["-f", &db.to_string_lossy(), "nasa"]);
    assert_eq!(out, "", "失败时不该往标准输出写东西");
    assert_eq!(
        err,
        format!(
            "wtf: cannot open acronym database file `{}'\n",
            db.display()
        ),
        "脚本 `:67` 的原句：反引号与单引号包住档名"
    );
    assert_eq!(code, 1);
}

/// 查无此词：退 1 并把句子走 stderr（脚本同形：句子走 `1>&2` 且 `rv=1`）。
#[test]
fn an_unknown_term_exits_one_with_the_sentence_on_stderr() {
    let db = fixture(
        "miss",
        b"NASA\tNational Aeronautics and Space Administration\n",
    );
    let (out, code, err) = run(&["-f", &db.to_string_lossy(), "zzznotaword"]);
    assert_eq!(out, "");
    assert_eq!(
        err, "wtf: I don't know what `zzznotaword' means!\n",
        "脚本 `:105` 印操作数原样（大写化只用在命中那支），走 stderr 且 rv=1"
    );
    assert_eq!(code, 1);
    let _ = std::fs::remove_file(&db);
}

/// 没有操作数：用法句走**标准输出**并退 1（脚本 `usage()` 是 `echo` 到 stdout）。
#[test]
fn no_operand_prints_usage_and_exits_one() {
    let (out, code, err) = run(&["-f", "/dev/null"]);
    // 脚本的 usage() 是 echo 到**标准输出**再退 1（`:11-14`，实测 stderr 空）。
    assert_eq!(
        out, "usage: wtf [-o] [-f dbfile] [is] term ...\n",
        "usage 走 stdout"
    );
    assert_eq!(err, "", "stderr 该是空的");
    assert_eq!(code, 1, "usage 那支退 1");
}

/// 脚本对 `is` 只做一件事（`minix3/games/wtf/wtf:32-34`）：`if [ "$1" = "is" ]; then
/// shift; fi`——移走**开头**那一枚，一次，比的是字面量。之后的每个 token 都是查询词，
/// 包括后面再出现的 `is`。四形的期望全部现量自
/// `sh minix3/games/wtf/wtf -f minix3/share/misc/acronyms <形>`（读数存
/// `tmp/wtf-393/script-readings.log`）：
/// `is lol` → 一行命中、退 0；`lol is` → 一行命中加一句 `is` 查无、退 1；
/// `is is lol` → 只移走第一枚，第二枚照查（一句查无加一行命中）；
/// `-- is lol` → 选项环之后的那一枚 `is` 同样被移走。
/// 旧写法在查询循环里跳过**任意位置**的 `is`，所以后三形都少一行报文且退码也差。
#[test]
fn only_the_leading_is_word_is_dropped() {
    let db = fixture("isword", b"LOL\tlaughing out loud\n");
    let db = db.to_string_lossy().into_owned();

    let (out, code, err) = run(&["-f", &db, "is", "lol"]);
    assert_eq!(out, "LOL: laughing out loud\n");
    assert_eq!(err, "");
    assert_eq!(code, 0, "开头那枚 is 被移走，剩下的照查");

    let (out, code, err) = run(&["-f", &db, "lol", "is"]);
    assert_eq!(
        out, "LOL: laughing out loud\n",
        "尾部的 is 不影响前面命中那行"
    );
    assert_eq!(err, "wtf: I don't know what `is' means!\n");
    assert_eq!(code, 1, "尾部 is 是个查询词，查无就把退码置 1");

    let (out, code, err) = run(&["-f", &db, "is", "is", "lol"]);
    assert_eq!(out, "LOL: laughing out loud\n");
    assert_eq!(
        err, "wtf: I don't know what `is' means!\n",
        "只移走开头那一枚"
    );
    assert_eq!(code, 1);

    let (out, code, err) = run(&["-f", &db, "--", "is", "lol"]);
    assert_eq!(out, "LOL: laughing out loud\n");
    assert_eq!(err, "");
    assert_eq!(code, 0, "`--` 之后的 is 仍是开头那个操作数");
}

/// 移走那一枚 `is` 比的是**字面量**（`[ "$1" = "is" ]`），不是大小写不敏感的比较：
/// `IS` 是个普通查询词。参照件现量 `IS lol` → 一句 ``I don't know what `IS' means!``
/// 加一行 `LOL: laughing out loud`，退 1。旧写法的库面判据 `is_skipped_word` 是
/// 大小写不敏感的，于是这一形少一行报文、退码也差。
#[test]
fn the_dropped_is_word_is_matched_case_sensitively() {
    let db = fixture("iscase", b"LOL\tlaughing out loud\n");
    let (out, code, err) = run(&["-f", &db.to_string_lossy(), "IS", "lol"]);
    assert_eq!(out, "LOL: laughing out loud\n");
    assert_eq!(err, "wtf: I don't know what `IS' means!\n");
    assert_eq!(code, 1);
    let _ = std::fs::remove_file(&db);
}

/// 单给一枚 `is`：移走之后没有操作数，走 `usage()`（脚本 `:36-38` 的 `[ -z "$1" ]`）。
/// 参照件现量 usage 走 **stdout**、退 1、stderr 空。
#[test]
fn a_lone_is_word_leaves_no_operand_and_prints_usage() {
    let db = fixture("islone", b"LOL\tlaughing out loud\n");
    let (out, code, err) = run(&["-f", &db.to_string_lossy(), "is"]);
    assert_eq!(out, "usage: wtf [-o] [-f dbfile] [is] term ...\n");
    assert_eq!(err, "");
    assert_eq!(code, 1);
    let _ = std::fs::remove_file(&db);
}
