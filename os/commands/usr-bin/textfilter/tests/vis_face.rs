//! End-to-end face for the `vis` binary: real process, real pipes and real
//! files, checked against `minix3/usr.bin/vis/vis.c` and the vis(3) engine in
//! `minix3/lib/libc/gen/vis.c`.
//!
//! The differential leg used to be missing (a Linux host ships no `vis`, and the
//! byte-exact expectations below were what the C source says); a later differential
//! leg built one by linking Minix's own `lib/libc/gen/vis.c` (the vis(3) engine) plus
//! Minix's getopt into a host reference — `tmp/vis-472/vis_ref`, invoked through a
//! same-named symlink, `LC_ALL=C` pinned on both sides. The expectations below
//! have all been re-measured against it; output is still compared as bytes
//! because `-n` copies the input through, including bytes above 0x7F, which no
//! `String` could carry unchanged.

mod face_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const VIS: &str = env!("CARGO_BIN_EXE_vis");

fn run(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, String) {
    let mut child = Command::new(VIS)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the vis binary should be runnable");
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
    path.push(format!("vis-face-{}-{tag}", std::process::id()));
    fs::write(&path, bytes).expect("writing the scratch file");
    path
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

/// `a`, a backslash, `b`, 0x01 (control), 0xFF (high bit), newline.
fn sample() -> Vec<u8> {
    vec![b'a', b'\\', b'b', 0x01, 0xFF, b'\n']
}

#[test]
fn default_encodes_only_the_non_graphic_bytes() {
    // `do_svis` passes graphic bytes and space/tab/newline through; 0x01 becomes
    // `\^A` and 0xFF becomes `\M^?` — `do_mbyte` emits the `M` and then the caret
    // form for the stripped 0x7F, with no `-` in between
    // (lib/libc/gen/vis.c:271-287). The backslash is graphic but sits on the extra
    // list `makeextralist` builds for every mode except `-b` (`vis.c:380`), so it
    // is the one graphic byte this output line encodes: `\134`.
    let (out, status, err) = run(&[], &sample());
    assert_eq!(out, b"a\\134b\\^A\\M^?\n".to_vec(), "stderr was {err:?}");
    assert_eq!(status, 0);
    assert_eq!(err, "");
}

#[test]
fn no_slash_drops_the_escape_prefix() {
    // `-b` is C's `VIS_NOSLASH` (vis.c:81), not `-n`.
    let (out, status, err) = run(&["-b"], &sample());
    assert_eq!(out, b"a\\b^AM^?\n".to_vec(), "stderr was {err:?}");
    assert_eq!(status, 0);
}

#[test]
fn mark_eol_ends_each_line_with_a_backslash_dollar() {
    // `vis.c:220-227`'s `else if (markeol && c == '\n')` arm writes `\`, `$`,
    // newline — and the marker's own backslash disappears under `-b`, which also
    // lets the input backslash through (`a\b` instead of `a\134b`).
    let (out, status, err) = run(&["-l"], &sample());
    assert_eq!(out, b"a\\134b\\^A\\M^?\\$\n".to_vec(), "stderr was {err:?}");
    assert_eq!(status, 0);
    let (out, _, _) = run(&["-b", "-l"], &sample());
    assert_eq!(out, b"a\\b^AM^?$\n".to_vec());
}

#[test]
fn none_copies_the_stream_and_doubles_backslashes() {
    // `vis.c`'s `if (none)` arm: 0xFF must come back as the byte 0xFF, not as
    // two UTF-8 bytes, and this is the letter that is *not* the no-slash flag.
    let (out, status, err) = run(&["-n"], &sample());
    assert_eq!(out, vec![b'a', b'\\', b'\\', b'b', 0x01, 0xFF, b'\n']);
    assert_eq!(status, 0, "stderr was {err:?}");
    // `-n` is tested before the marker in C's loop, so it wins.
    let (out, _, _) = run(&["-n", "-l"], &sample());
    assert_eq!(out, vec![b'a', b'\\', b'\\', b'b', 0x01, 0xFF, b'\n']);
}

#[test]
fn octal_and_cstyle_still_reach_the_same_stream() {
    // `-o` renders every encoded byte as three octal digits, the backslash
    // included because it is on the extra list.
    let (out, _, _) = run(&["-o"], &sample());
    assert_eq!(out, b"a\\134b\\001\\377\n".to_vec());
    // `-c` cannot reach the tab or the newline at all: `do_svis` passes
    // whitespace through before the flag-specific branch runs, so the only
    // escape here is 0x01, and that one has no named form (`\a` is 0x07), so
    // it renders in the caret form.
    let (out, _, _) = run(&["-c"], b"a\t\n\x01");
    assert_eq!(out, b"a\t\n\\^A".to_vec());
}

#[test]
fn operands_are_encoded_into_one_stream_and_a_bad_one_only_warns() {
    let one = written("op-a", b"a\n");
    let two = written("op-b", b"\x01");
    let missing = format!("vis-face-{}-missing", std::process::id());
    let (out, status, err) = run(&[&text(&one), &missing, &text(&two)], b"");
    assert_eq!(out, b"a\n\\^A".to_vec());
    assert_eq!(status, 1, "stderr was {err:?}");
    // C's form is the three-part `warn("%s", *argv)` (`vis.c:157`): `vis: name: strerror`,
    // and after printing it **carries on with the next operand** while pinning the
    // exit status to 1 (`:158`). This shell used to print only two parts.
    assert_eq!(
        err,
        format!("vis: {missing}: No such file or directory\n"),
        "开不了的操数是三段形，含 strerror 那一段"
    );

    // A bare `-` is a literal file name for `vis` (its operands go through
    // `fopen`), not a stdin alias.
    let (out, status, err) = run(&["-"], b"a\n");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1);
    assert_eq!(
        err, "vis: -: No such file or directory\n",
        "裸 `-` 也走 fopen，报文同是三段形"
    );

    for path in [&one, &two] {
        let _ = fs::remove_file(path);
    }
}

/// The same command seam as `run`, but stderr is handed back **byte for byte**:
/// after `from_utf8_lossy`, a bare `c3` is the replacement character U+FFFD and
/// `c3 83` is `Ã`; the two mistakes differ as strings, and neither is as telling
/// as comparing bytes directly.
fn run_streams(args: &[&str], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
    let mut child = Command::new(VIS)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the vis binary should be runnable");
    face_support::write_stdin(&mut child, input);
    let output = child.wait_with_output().expect("the run should finish");
    (
        output.stdout,
        output.status.code().unwrap_or(-1),
        output.stderr,
    )
}

/// The letter slot of the off-letter line is **one raw byte** (`-\u{e9}` hands back
/// `c3`, not `c3 a9` and not `c3 83`). The mechanism is in
/// `minix3/lib/libc/stdlib/getopt.c:79` (`optopt = *place++` takes one byte at a
/// time) and `:103` (`%c` writes it out as is). The differential leg — Minix's own
/// `lib/libc/gen/vis.c` plus Minix's getopt linked into a host reference,
/// `tmp/vis-472/vis_ref`, started through a same-named symlink with `LC_ALL=C`
/// pinned on both sides — re-measured every expectation below, and its eight
/// negative controls are recorded in `tmp/vis-472/measure.log` as the basis.
///
/// `c` is a valid letter in C's option string and takes no value, so a cluster
/// keeps consuming and reaches the first byte of `é`; `-Zé` stops at `Z` because
/// `vis.c:135-140`'s `case '?'`/`default` prints usage and `return 1`s, so the
/// second letter never gets a turn.
#[test]
fn the_libc_line_of_a_multibyte_flag_is_the_first_byte_of_the_flag() {
    const USAGE_LINE: &[u8] =
        b"Usage: vis [-bcfhlMmNnoSstw] [-e extra] [-F foldwidth] [file ...]\n";
    let want: Vec<u8> = [b"vis: unknown option -- ".as_slice(), b"\xc3\n", USAGE_LINE].concat();
    for form in ["-\u{e9}", "-c\u{e9}"] {
        let (out, status, err) = run_streams(&[form], b"a\n");
        assert_eq!(out, Vec::new(), "{form} 的报文只走标准错误");
        assert_eq!(status, 1, "{form} 的 stderr 是 {err:?}");
        assert_eq!(err, want, "{form} 的字母该是一个裸字节 c3");
    }

    let (_out, status, err) = run_streams(&["-Z\u{e9}"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"vis: unknown option -- Z\n".as_slice(), USAGE_LINE,].concat(),
        "环停在第一个坏字母上，`é` 根本不该被读到"
    );
}

/// These byte-handed-back legs are the family's first **verdict leg**: `-e`'s value
/// goes into the extra list as a whole string, and the extra list decides which
/// bytes take the octal branch — every step matches C: `extra = optarg`
/// (`vis.c:87`); `makeextralist` (`lib/libc/gen/vis.c:351-384`) puts the `-e`
/// string on the list **byte by byte** as `(wchar_t)(u_char)src[i]` when
/// `VIS_NOLOCALE` is set or multibyte decoding fails; `warn("%s", *argv)`
/// (`:157`) prints argv's original word; and the letter slot of the libc line is
/// one byte from `getopt.c:79`. Where folding turns one `ff` into three bytes
/// `ef bf bd`, the encoding answer changes with it (before folding `ff` is off the
/// list and takes the M- notation; after folding the list becomes `ef`/`bf`/`bd`
/// and `ff` is still off it). That makes this a member of the join/pr/sort family
/// rather than of the split/tail/unexpand group, where folding leaves the verdict
/// alone. The reference reading is `tmp/vis-472/measure.log` (26 forms, identical
/// byte for byte on both sides).
#[test]
fn the_extra_list_the_letter_and_the_operand_name_carry_the_argv_bytes() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    const USAGE_LINE: &[u8] =
        b"Usage: vis [-bcfhlMmNnoSstw] [-e extra] [-F foldwidth] [file ...]\n";

    fn run_raw(args: &[&[u8]], input: &[u8]) -> (Vec<u8>, i32, Vec<u8>) {
        let mut child = Command::new(VIS)
            .args(args.iter().map(|word| OsStr::from_bytes(word)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the vis binary should be runnable");
        face_support::write_stdin(&mut child, input);
        let output = child.wait_with_output().expect("the run should finish");
        (
            output.stdout,
            output.status.code().unwrap_or(-1),
            output.stderr,
        )
    }

    let source: &[u8] = b"x\xffy\n";
    let source_file = written("hi-ff", source);
    let operand = text(&source_file);
    let operand_bytes: &[u8] = operand.as_bytes();
    // `-e <ff>`: ff joins the list, so an ff in the input switches from M- notation
    // to three-digit octal. The detached and attached value forms give the same answer.
    for form in [
        vec![b"-e".to_vec(), b"\xff".to_vec()],
        vec![b"-e\xff".to_vec()],
    ] {
        let args: Vec<&[u8]> = form
            .iter()
            .map(Vec::as_slice)
            .chain([operand_bytes])
            .collect();
        let (out, status, err) = run_raw(&args, source);
        assert_eq!(err, Vec::<u8>::new(), "{form:?} 的报文只走标准输出");
        assert_eq!(status, 0, "{form:?} stderr 是 {err:?}");
        assert_eq!(
            out,
            b"x\\377y\n".to_vec(),
            "{form:?}：ff 在名单上，走八进制"
        );
    }
    // The other way round: with an ASCII byte on the list, ff falls back to M-
    // notation and y takes octal — the list is a byte-wise set, not a "escape a
    // bit more" switch.
    let (out, status, _err) = run_raw(&[b"-e", b"y", operand_bytes], source);
    assert_eq!(status, 0);
    assert_eq!(
        out,
        b"x\\M^?\\171\n".to_vec(),
        "y 上榜而 ff 不在，编码跟着名单走"
    );
    // The letter slot: a single raw byte is reported as itself, and the folded
    // three-byte `ef bf bd` no longer appears.
    let (_out, status, err) = run_raw(&[b"-\xff"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"vis: unknown option -- ".as_slice(), b"\xff\n", USAGE_LINE].concat()
    );
    // The operand name: the three-part `warn("%s", *argv)` form, with the file-name
    // slot as the raw byte and the strerror segment unchanged.
    let (_out, status, err) = run_raw(&[b"nosuch\xff.txt"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"vis: nosuch\xff.txt: No such file or directory\n".to_vec()
    );
    // Folding `-F`'s value cannot change the verdict: `atoi` accepts only ASCII
    // digits, and folded or not the value is not a number and converts to 0, so
    // both fail the `< 5` test — and that sentence carries no `%s`, so the value
    // never reaches the message.
    let (out, status, err) = run_raw(&[b"-F", b"\xff"], b"a\n");
    assert_eq!(status, 1);
    assert_eq!(err, b"vis: can't fold lines to less than 5 cols\n".to_vec());
    assert_eq!(out, Vec::<u8>::new());
    let _ = fs::remove_file(&source_file);
}

#[test]
fn letters_outside_the_c_optstring_are_refused_with_usage() {
    // Every letter of C's `"bcde:F:fhlMmNnoSstw"` is now modelled, so the only
    // refusals left are the letters C itself sends to `default:`. `-d` belongs in
    // that group: it is C's debug counter, compiled out of this build, so the
    // answer is the usage line rather than a “not modelled” note about our shell.
    // An off-letter produces **two lines**: libc's `getopt`
    // (`minix3/lib/libc/stdlib/getopt.c:101-104`; the option string does not start
    // with `:`, so `opterr` is in effect) prints `vis: unknown option -- <letter>`
    // first, and `case '?'`/`default` then prints the usage line (`vis.c:135-140`).
    // The old assertion only checked `contains("Usage: vis [")`, so a missing first
    // line went unnoticed; it is now pinned by exact equality.
    const USAGE_LINE: &str = "Usage: vis [-bcfhlMmNnoSstw] [-e extra] [-F foldwidth] [file ...]\n";
    for (flag, letter) in [("-z", 'z'), ("-1", '1')] {
        let (out, status, err) = run(&[flag], b"a\n");
        assert_eq!(out, Vec::<u8>::new(), "flag {flag} printed {out:?}");
        assert_eq!(status, 1, "flag {flag} stderr was {err:?}");
        assert_eq!(
            err,
            format!("vis: unknown option -- {letter}\n{USAGE_LINE}"),
            "flag {flag} 的两行形与 C 不符"
        );
    }
    // `-d` is a letter that takes **no value**: `vis.c:73`'s string splits into
    // `b c d e: F: f h l M m N n o S s t w`, with the colons on `e` and `F`, so `d`
    // with nothing after it is still a complete letter. The reference measured four
    // forms (`tmp/vis-472/measure.log`: `-d`, `-d x`, `-cd`, `-dZ`) and **all of
    // them print the usage line alone** — getopt accepts the letter, so libc does
    // not report a missing value, and `case 'd'` is compiled out of a non-`DEBUG`
    // build and falls to `default:`. This shell used to treat `d` as a valued
    // letter and printed libc's extra line when `-d` came last.
    for form in [vec!["-d"], vec!["-d", "x"], vec!["-cd"], vec!["-dZ"]] {
        let (out, status, err) = run(&form, b"a\n");
        assert_eq!(out, Vec::<u8>::new(), "{form:?} printed {out:?}");
        assert_eq!(status, 1, "{form:?} stderr was {err:?}");
        assert_eq!(err, USAGE_LINE, "{form:?} 该只有 usage 一行");
    }
    // And the one combination C refuses by name: `-h` with `-m`. The check sits
    // after the whole option loop and before any operand is opened
    // (`usr.bin/vis/vis.c:142-144`), so it outranks an unreadable file.
    for flags in [
        vec!["-h", "-m"],
        vec!["-hm"],
        vec!["-m", "-h", "missing-operand"],
    ] {
        let (out, status, err) = run(&flags, b"a\n");
        assert_eq!(out, Vec::<u8>::new(), "{flags:?} printed {out:?}");
        assert_eq!(status, 1, "{flags:?} stderr was {err:?}");
        assert!(
            err.contains("Can't specify -m and -h at the same time"),
            "{flags:?} stderr was {err:?}"
        );
        assert!(
            !err.contains("missing-operand"),
            "{flags:?} should not have reached the operands: {err:?}"
        );
    }
}

/// Every line that names the program follows C's `getprogname()`, not a hardcoded
/// `vis`: the usage line (`vis.c:136-139` spells `"Usage: %s …"`), the `-F`
/// refusal (`errx`, `:91`) and the `-m`/`-h` refusal (`errx`, `:144`). The
/// registered `VIS-USAGE-PROGNAME-HARDCODED` named only the usage line; the two
/// `errx` lines were measured to have the same root after a `vv` symlink turned
/// all four reference lines into `vv`, and the reference `tmp/vis-472/vis_ref`
/// reads `vv: unknown option -- Z` / `Usage: vv […]` / `vv: can't fold lines to
/// less than 5 cols` / `vv: Can't specify -m and -h at the same time`. The gate
/// sets `argv[0]` directly (`CommandExt::arg0`), so no symlink is needed on the
/// Rust side.
#[test]
fn every_program_name_slot_follows_argv0() {
    use std::os::unix::process::CommandExt;

    fn run_as(name: &str, args: &[&str]) -> (Vec<u8>, i32, Vec<u8>) {
        let child = Command::new(VIS)
            .arg0(name)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the vis binary should be runnable");
        let output = child.wait_with_output().expect("the run should finish");
        (
            output.stdout,
            output.status.code().unwrap_or(-1),
            output.stderr,
        )
    }

    const USAGE_VV: &[u8] = b"Usage: vv [-bcfhlMmNnoSstw] [-e extra] [-F foldwidth] [file ...]\n";
    // The libc line and the usage line share one origin: both take `argv[0]`.
    let (out, status, err) = run_as("vv", &["-Z"]);
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1);
    assert_eq!(
        err,
        [b"vv: unknown option -- Z\n".as_slice(), USAGE_VV].concat(),
        "-Z 的两行都必须跟着 argv[0]"
    );
    // `-d` reaches the usage line alone (`case 'd'` is compiled out).
    let (out, status, err) = run_as("vv", &["-d"]);
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1);
    assert_eq!(err, USAGE_VV, "-d 只有 usage 一行，名字也是 vv");
    // The two `errx` refusals prefix `getprogname()`.
    let (out, status, err) = run_as("vv", &["-F", "4"]);
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"vv: can't fold lines to less than 5 cols\n".to_vec(),
        "-F 的 errx 前缀也必须跟着 argv[0]"
    );
    let (out, status, err) = run_as("vv", &["-m", "-h"]);
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1);
    assert_eq!(
        err,
        b"vv: Can't specify -m and -h at the same time\n".to_vec(),
        "-m/-h 的 errx 前缀也必须跟着 argv[0]"
    );
    // Under its own name the answer is unchanged: the other face tests that
    // expect `vis:` keep their own contract, and the name really is argv[0].
    let (_, status, err) = run_as("vis", &["-F", "4"]);
    assert_eq!(status, 1);
    assert_eq!(err, b"vis: can't fold lines to less than 5 cols\n".to_vec());
}

#[test]
fn letters_cluster_and_values_attach_or_follow() {
    // `getopt("bcde:F:fhlMmNnoSstw")` clusters flags, and `e` takes the rest of its
    // token or the following word.
    assert_eq!(
        run(&["-bl"], b"a\n").0,
        run(&["-b", "-l"], b"a\n").0,
        "-bl must equal -b -l"
    );
    assert_eq!(run(&["-e", "a"], b"a").0, b"\\141".to_vec());
    assert_eq!(run(&["-ea"], b"a").0, b"\\141".to_vec());
    // A later `-e` replaces the list, because C assigns `extra = optarg`.
    assert_eq!(run(&["-e", "ab", "-e", "b"], b"ab").0, b"a\\142".to_vec());
    // `--` ends option scanning, so what follows is an operand.
    let (out, status, err) = run(&["--", "-c"], b"");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1, "stderr was {err:?}");
    assert!(err.contains("vis: -c"), "stderr was {err:?}");
}

#[test]
fn extra_bytes_protect_graphic_characters_and_meta_space_renders_octal() {
    // A byte named by `-e` skips `do_svis`' pass-through, so a printable letter
    // becomes three octal digits; under `-c` a graphic non-octal letter that is
    // not in the skip list gets the short `\c` form instead, and a skip-list or
    // octal-digit letter falls back to octal because `\n`-style escapes there
    // would be ambiguous.
    assert_eq!(run(&["-e", "x"], b"x").0, b"\\170".to_vec());
    assert_eq!(run(&["-c", "-e", "x"], b"x").0, b"\\x".to_vec());
    assert_eq!(run(&["-c", "-e", "n"], b"n").0, b"\\156".to_vec());
    assert_eq!(run(&["-c", "-e", "3"], b"3").0, b"\\063".to_vec());
    // 0xA0 is the meta-space: `(c & 0177) == ' '` sends it to the octal branch, so
    // it never becomes a meta sequence.
    assert_eq!(run(&[], &[0xA0]).0, b"\\240".to_vec());
}

#[test]
fn safe_admits_the_three_named_controls_and_beats_cstyle() {
    // `-s` widens only the pass-through test (`vis.c:312`), so backspace, bell and
    // carriage return come back as themselves while every other control still
    // escapes. The pass-through runs before the `-c` switch, so `-c -s` must not
    // turn them into `\b`, `\a`, `\r`.
    let (out, status, err) = run(&["-s"], b"\x08\x07\r\x01");
    assert_eq!(
        out,
        vec![0x08u8, 0x07, 0x0D, b'\\', b'^', b'A'],
        "stderr was {err:?}"
    );
    assert_eq!(status, 0);
    assert_eq!(
        run(&["-c", "-s"], b"\x08\x07\r").0,
        vec![0x08u8, 0x07, 0x0D]
    );
    // Without `-s` the same three bytes are controls.
    assert_eq!(run(&[], b"\x08\x07\r").0, b"\\^H\\^G\\^M".to_vec());
}

#[test]
fn whitespace_and_magic_letters_change_only_the_extra_list() {
    // `-t` sets `VIS_TAB` alone, `-w` the three whitespace bits, `-S` the shell
    // magic set and `-M` the whole `VIS_META` union (`include/vis.h:49-67`). An
    // extra-list member escapes, and where `-c` has a named form for it that form
    // wins because the switch precedes the octal branch in `do_mbyte`.
    assert_eq!(run(&["-t"], b"a\tb c\n").0, b"a\\011b c\n".to_vec());
    assert_eq!(run(&["-c", "-t"], b"a\tb").0, b"a\\tb".to_vec());
    assert_eq!(run(&["-w"], b" \t\nx").0, b"\\040\\011\\012x".to_vec());
    // `$` is shell magic, `*` is glob magic: `-S` names the first and leaves the
    // second alone, `-M` names both plus the whitespace.
    assert_eq!(run(&["-S"], b"$*").0, b"\\044*".to_vec());
    assert_eq!(run(&["-M"], b"$* ").0, b"\\044\\052\\040".to_vec());
    // A shell byte and the default backslash coexist on one list; membership, not
    // duplication, is what `do_svis` asks (`vis.c:310`). `char_shell` itself
    // contains a backslash (`vis.c:133`), so `-S` encodes one whether or not `-b`
    // has cleared the default entry.
    assert_eq!(run(&["-S"], b"a\\$").0, b"a\\134\\044".to_vec());
    assert_eq!(run(&["-S", "-b"], b"a\\$").0, b"a\\134\\044".to_vec());
}

#[test]
fn nolocale_letter_is_accepted_and_changes_nothing() {
    // `-N` (`VIS_NOLOCALE`) picks the C-locale twin of `ISGRAPH` and of the `-e`
    // string decoding (`lib/libc/gen/vis.c:122-124,361`). This engine reads
    // bytes and never reassembles characters, so the answer is the same either
    // way; the flag must still be accepted, not refused.
    let (out, status, err) = run(&["-N"], &sample());
    assert_eq!(status, 0, "stderr was {err:?}");
    assert_eq!(out, run(&[], &sample()).0, "-N must equal the default");
    // Clusters and combinations keep working: `-Nc` is `-c`. And `-b` removes only
    // the unconditional backslash entry (`vis.c:380`) — `char_shell` names the same
    // byte (`vis.c:133`), so `-S` and `-M` put it back on the list and a backslash
    // still encodes.
    assert_eq!(run(&["-Nc"], b"a\r").0, run(&["-c"], b"a\r").0);
    assert_eq!(run(&["-b"], b"\\").0, b"\\".to_vec());
    assert_eq!(run(&["-M", "-b"], b"$\\").0, b"\\044\\134".to_vec());
}

/// `minix3/usr.bin/vis/vis.c:58` starts `foldwidth` at 80 and `-f` leaves it there,
/// so `-f` alone means 78 printable columns before the hidden newline (`vis.c`
/// breaks at `col > max - 2`, `usr.bin/vis/foldit.c:61`).
fn eighty() -> Vec<u8> {
    vec![b'x'; 80]
}

#[test]
fn fold_writes_a_hidden_newline_and_closes_a_partial_line() {
    // Eighty columns of `x` fold after 78, and the two leftover columns are a
    // partial line, which `usr.bin/vis/vis.c:288-289` terminates with one more marker.
    let mut expected = vec![b'x'; 78];
    expected.extend_from_slice(b"\\\nxx\\\n");
    assert_eq!(run(&["-f"], &eighty()).0, expected);
    // An empty input still gets the closing marker: `cp` starts at `nul + 1`, so
    // `*(cp - 1)` is not a newline (`usr.bin/vis/vis.c:171-172`, and the comment there says so).
    assert_eq!(run(&["-f"], b"").0, b"\\\n".to_vec());
    // A run that ends exactly on a newline needs no closing marker.
    assert_eq!(run(&["-f"], b"ab\n").0, b"ab\n".to_vec());
}

#[test]
fn fold_flag_alone_marks_lines_but_does_not_fold() {
    // `-F width` sets the width and `markeol++` but not `fold` (`usr.bin/vis/vis.c:89-95`),
    // while `-f` sets `fold` and leaves the width alone (`usr.bin/vis/vis.c:96-98`). So `-F 6` alone
    // marks lines and never folds, and `-f -F 6` folds at 4 columns.
    assert_eq!(run(&["-F", "6"], b"abcdefgh").0, b"abcdefgh".to_vec());
    assert_eq!(run(&["-l"], b"abcdefgh").0, b"abcdefgh".to_vec());
    assert_eq!(
        run(&["-f", "-F", "6"], b"abcdefgh").0,
        b"abcd\\\nefgh\\\n".to_vec()
    );
    // Attached value and clustered letters are the same call: `-F6` ≡ `-F 6`.
    assert_eq!(run(&["-fF6"], b"abcdefgh").0, b"abcd\\\nefgh\\\n".to_vec());
    // `-F` marks the end of line, so a newline in the input gains its `\$`; the
    // three characters of that marker are three columns, which is why a `\n` right
    // after a full line folds first.
    assert_eq!(run(&["-f", "-F", "6"], b"ab\n").0, b"ab\\$\n".to_vec());
    assert_eq!(
        run(&["-f", "-F", "6"], b"abcd\n").0,
        b"abcd\\\n\\$\n".to_vec()
    );
}

#[test]
fn fold_width_is_read_the_way_atoi_reads_it() {
    // C checks `atoi(optarg) < 5` (`usr.bin/vis/vis.c:90-93`), and `atoi` converts a word with
    // no digits to zero, ignores trailing garbage and takes a sign — so `4`, `abc`
    // and `-3` all fail the check while `6x` passes it as 6.
    let refused: Vec<Vec<&str>> = vec![
        vec!["-F", "4"],
        vec!["-F4"],
        vec!["-Fabc"],
        vec!["-F-3"],
        vec!["-F0"],
    ];
    for flag in refused {
        let (out, status, err) = run(&flag, b"abcdefgh");
        assert_eq!(out, Vec::<u8>::new(), "{flag:?} printed {out:?}");
        assert_eq!(status, 1, "{flag:?} stderr was {err:?}");
        assert!(
            err.contains("can't fold lines to less than 5 cols"),
            "{flag:?} stderr was {err:?}"
        );
    }
    // Trailing garbage is `atoi`'s business, not getopt's: `-F6x` is width 6.
    assert_eq!(
        run(&["-f", "-F6x"], b"abcdefgh").0,
        b"abcd\\\nefgh\\\n".to_vec()
    );
    // A `-F` with nothing after it is the missing-argument case of getopt.
    let (out, status, err) = run(&["-F"], b"a");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1, "stderr was {err:?}");
    assert_eq!(
        err,
        "vis: option requires an argument -- F\nUsage: vis [-bcfhlMmNnoSstw] [-e extra] [-F foldwidth] [file ...]\n",
        "缺值那一支是两行（libc 首行 + usage）；旧断言只看 `contains`，少了首行也测不出"
    );
    // A missing value for `-e` is the other member of the same family; only the
    // letter changes.
    let (out, status, err) = run(&["-e"], b"a");
    assert_eq!(out, Vec::<u8>::new());
    assert_eq!(status, 1, "stderr was {err:?}");
    assert!(
        err.starts_with("vis: option requires an argument -- e\nUsage: vis ["),
        "stderr was {err:?}"
    );
}

#[test]
fn fold_counts_columns_of_the_encoded_text_and_carries_the_column_across_files() {
    // `usr.bin/vis/foldit.c:47-60` walks the text it was handed, so a tab is the next multiple
    // of eight columns, an escape sequence is however many characters it spells,
    // and `usr.bin/vis/vis.c:170`'s `static int col` survives from one operand to the next.
    // A tab at column 8 overflows the four-column limit of `-F 10`, and the closing
    // marker does not reset the column, so the second file opens on a fold.
    assert_eq!(
        run(&["-f", "-F", "10"], b"\t\t").0,
        b"\t\\\n\t\\\n".to_vec()
    );
    let one = written("fold-a", &[b'x'; 78]);
    let two = written("fold-b", b"ab");
    let mut expected = vec![b'x'; 78];
    expected.extend_from_slice(b"\\\n\\\nab\\\n");
    assert_eq!(
        run(&["-f", &one.to_string_lossy(), &two.to_string_lossy()], b"").0,
        expected
    );

    // A control byte encodes to three characters and therefore costs three columns:
    // at the four-column limit of `-F 6`, `a`, `\^A`, `b` fills a line exactly and
    // the `b` that would be the fifth column opens the next one.
    assert_eq!(
        run(&["-f", "-F", "6"], b"a\x01b").0,
        b"a\\^A\\\nb\\\n".to_vec()
    );
    for path in [&one, &two] {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn fold_applies_to_the_none_path_too() {
    // In `process()` the `none` branch only decides what goes into `buff`; the
    // folding test that follows it runs whatever the chunk turned out to be
    // (`usr.bin/vis/vis.c:213-228` then `:268-278`), so `-n` folds and `-F`'s line marker is
    // still shadowed by `-n`.
    assert_eq!(
        run(&["-n", "-f", "-F", "6"], b"abcdefgh").0,
        b"abcd\\\nefgh\\\n".to_vec()
    );
    // `-n` doubles a backslash, and both bytes count as columns.
    assert_eq!(
        run(&["-n", "-f", "-F", "6"], b"abcd\\e").0,
        b"abcd\\\n\\\\e\\\n".to_vec()
    );
}

#[test]
fn http_and_mime_letters_switch_to_their_own_escapes_and_hex_case() {
    // The two alternate encodings disagree about the case of their hex digits
    // exactly as the C's two tables do (`xtoa` lowercase after `%`, `XTOA` uppercase
    // after `=`), and both hand a byte they allow back to the standard engine, so
    // `-h` with `-e` still protects a letter the same way it does in default mode.
    assert_eq!(run(&["-h"], b"a b").0, b"a%20b".to_vec());
    assert_eq!(run(&["-h"], &[0xFF]).0, b"%ff".to_vec());
    assert_eq!(run(&["-m"], &[0xFF]).0, b"=FF".to_vec());
    assert_eq!(run(&["-h", "-e", "a"], b"ab").0, b"\\141b".to_vec());
    // A newline is structure to MIME and data to HTTP.
    assert_eq!(run(&["-m"], b"a\nb").0, b"a\nb".to_vec());
    assert_eq!(run(&["-h"], b"a\nb").0, b"a%0ab".to_vec());
    // A space escapes in MIME only when a line ends after it, which is the
    // `iswspace(c) && (nextc == '\r' || nextc == '\n')` clause of `do_mvis`.
    assert_eq!(run(&["-m"], b"a b\n").0, b"a b\n".to_vec());
    assert_eq!(run(&["-m"], b"a \nb").0, b"a=20\nb".to_vec());
    // `=` is quoted-printable's own introducer, and the C catches it with the range
    // test `c > 60 && c < 62`.
    assert_eq!(run(&["-m"], b"a=b").0, b"a=3Db".to_vec());
    assert_eq!(run(&["-m", "-b"], b"a\\").0, b"a=5C".to_vec());
}

#[test]
fn mime_moves_the_default_fold_width_and_writes_its_own_marker() {
    // `-m` pulls the fold width from the 80 default down to 76
    // (`usr.bin/vis/vis.c:110-111`) and changes the hidden newline from a backslash
    // to an `=` (`usr.bin/vis/foldit.c:63`, `usr.bin/vis/vis.c:289`), so `-f` breaks
    // at 74 columns for MIME where it breaks at 78 without the flag.
    let hundred = vec![b'x'; 100];
    let mut mime_expected = vec![b'x'; 74];
    mime_expected.extend_from_slice(b"=\n");
    mime_expected.extend(vec![b'x'; 26]);
    mime_expected.extend_from_slice(b"=\n");
    assert_eq!(run(&["-m", "-f"], &hundred).0, mime_expected);
    let mut plain_expected = vec![b'x'; 78];
    plain_expected.extend_from_slice(b"\\\n");
    plain_expected.extend(vec![b'x'; 22]);
    plain_expected.extend_from_slice(b"\\\n");
    assert_eq!(run(&["-f"], &hundred).0, plain_expected);
    // An explicit `-F` wins over the 76 adjustment in either order, because C only
    // touches a width that is still 80.
    let mut width40 = vec![b'x'; 38];
    width40.extend_from_slice(b"=\n");
    width40.extend(vec![b'x'; 38]);
    width40.extend_from_slice(b"=\n");
    width40.extend(vec![b'x'; 24]);
    width40.extend_from_slice(b"=\n");
    assert_eq!(run(&["-m", "-f", "-F", "40"], &hundred).0, width40);
    assert_eq!(run(&["-F", "40", "-m", "-f"], &hundred).0, width40);
}
