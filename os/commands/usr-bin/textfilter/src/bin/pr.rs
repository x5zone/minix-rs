//! Minix-RS pr — paginated output.
//!
//! Ground truth: `minix3/usr.bin/pr/pr.c` (NetBSD) with the constants of
//! `pr.h` (66-line pages, 5-line header and trailer). The deciding half
//! — page assembly, the header line, body padding, and per-file pagination —
//! is the library's `pr` module (`paginate_files`); this program parses the
//! flags, reads each file operand (a `-` names standard input, and no operand
//! means standard input alone titled `""`), and prints. Each file paginates
//! independently with its name as the header title, and `-h` overrides that
//! title for every file (`pr.c:224-233`, `nxtfile` `pr.c:1318-1376`). The time
//! field of the header line is not rendered here (the C uses the current date,
//! or each file's mtime; `pr.h:63`). Vertical multi-column output (`-N`, with
//! `-w` width and `-s` separator) is modelled via the library's `vertcol` face,
//! and the `-o` left margin via `render_page`; the single-column `-n` running
//! line number (C `addnum`) via the `number_width`/`number_char` options; the
//! across-fill `-a` (C `horzcol`) via the `across` option; the double-spacing
//! `-d` (C `otln`+`setup`) via `double_space` in the single-column path; the
//! multi-file merge `-m` (C `mulfile`, one column per operand) via
//! `merge_body_rows`; the formfeed (`-F`), `-d` combined with columns, and
//! multi-column `-n` are later batches. An
//! unopenable operand warns
//! and is skipped with a nonzero exit (`strerror` suffix not reproduced).

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::pr::{
    PrError, column_width, contract_tabs, expand_tabs, merge_body_rows, paginate, paginate_files,
    parse_pr_args, render_page,
};

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    // argv folding lives in the deciding half (`parse_pr_args`): BSD getopt with
    // clustered flag letters, glued-or-next values, and the post-scan page-width
    // /column/-a validations. The result is projected back onto the value types
    // the file/merge/paginate tail below already consumes.
    let parsed = match parse_pr_args(&args[1..]) {
        Ok(p) => p,
        Err(e) => arg_error(e),
    };
    let options = parsed.options;
    let merge = parsed.merge;
    let title_override: Option<String> = parsed.title_override.map(String::from);
    let operands: Vec<String> = parsed.operands.iter().map(|s| String::from(*s)).collect();

    // Read each operand into a logical file (name, lines). No operands means
    // standard input alone, titled `""` (pr.h:63 `FNAME`); an unopenable
    // operand warns and is skipped, turning the exit status nonzero.
    let mut files: Vec<(&str, Vec<String>)> = Vec::new();
    let mut had_error = false;
    if operands.is_empty() {
        let input = support::read_stdin();
        files.push(("", split_lines(&expand_input(&input, &options))));
    } else {
        for op in &operands {
            if op == "-" {
                let input = support::read_stdin();
                files.push(("", split_lines(&expand_input(&input, &options))));
            } else {
                match support::read_file(op) {
                    Ok(bytes) => {
                        let bytes = expand_input(&bytes, &options);
                        files.push((op.as_str(), split_lines(&bytes)));
                    }
                    Err(_) => {
                        support::warn(format!("pr: cannot open {op}\n").as_bytes());
                        had_error = true;
                    }
                }
            }
        }
    }

    // `-m`: merge the operands side by side, one column per file (`mulfile`).
    // With fewer than two operands C falls back to normal output. POSIX leaves
    // the merge page header unspecified (`pr.c:848-849`), so the title is the
    // first operand's name unless `-h` overrides it.
    if merge && files.len() >= 2 {
        let streams: Vec<Vec<String>> = files.iter().map(|(_, ls)| ls.clone()).collect();
        let n = streams.len();
        let (colwd, _) = match column_width(options.page_width, n) {
            Some(widths) => widths,
            None => {
                support::warn(format!("pr: page width is too small for {n} columns\n").as_bytes());
                usage();
            }
        };
        let merged = merge_body_rows(&streams, colwd, options.sep);
        let mut opts = options.clone();
        opts.columns = 1;
        opts.header = title_override
            .clone()
            .unwrap_or_else(|| String::from(files[0].0));
        for page in paginate(&merged, &opts) {
            emit_page(&page, &options);
        }
        support::terminate(if had_error { 1 } else { 0 });
    }

    for page in paginate_files(&files, &options, title_override.as_deref()) {
        emit_page(&page, &options);
    }
    support::terminate(if had_error { 1 } else { 0 });
}

/// The input leg of C's `-e` (`inln`, `pr.c:1032-1068`): tabs are expanded while the
/// line is still a byte stream, which is why the column arithmetic downstream sees the
/// expanded width. No `-e` and no implied multi-column mode means C's `ingap == 0`
/// and the bytes pass through untouched.
fn expand_input(data: &[u8], options: &minix_textfilter::pr::PrOptions) -> Vec<u8> {
    match options.expand {
        Some((character, gap)) => expand_tabs(data, character, gap),
        None => data.to_vec(),
    }
}

/// One page out, with C's output-leg asymmetry kept: `otln` contracts only the body
/// lines (`pr.c:1137-1240`), while `prhead` and `prtail` write the header block and
/// the trailer block with bare `printf`/`putchar` (`:1499-1513`, `:1524-1573`), so
/// blanks in those lines are never folded back into tabs.
fn emit_page(page: &minix_textfilter::pr::Page, options: &minix_textfilter::pr::PrOptions) {
    let header = page.header_lines.len();
    let body_end = header + page.body_lines.len();
    for (index, line) in render_page(page, options.offset).iter().enumerate() {
        let body_line = index >= header && index < body_end;
        let mut out = match (body_line, options.contract) {
            (true, Some((character, gap))) => contract_tabs(line.as_bytes(), character, gap),
            _ => line.as_bytes().to_vec(),
        };
        out.push(b'\n');
        support::emit(&out);
    }
    if page.form_feed {
        // `prtail` writes the form feed with a bare `putchar` and no newline
        // (`pr.c:1556-1557`), so the byte lands right after the page's last line.
        support::emit(b"\x0c");
    }
}

fn split_lines(input: &[u8]) -> Vec<String> {
    if input.is_empty() {
        return Vec::new();
    }
    let text = core::str::from_utf8(input).unwrap_or("");
    let mut pieces: Vec<String> = text.split('\n').map(String::from).collect();
    if input.ends_with(b"\n") {
        pieces.pop();
    }
    pieces
}

/// Map a [`PrError`] to the exact C diagnostic and exit status. Every one of these
/// is `setup` returning 1: the message goes out on its own (no usage line), and
/// `main` then exits 1 (`pr.c:142-160`, `1616-1627`).
fn arg_error(e: PrError) -> ! {
    match e {
        PrError::Usage => usage(),
        PrError::PlusPage => fail(b"pr: +page number must be 1 or more\n"),
        PrError::ColumnCount => fail(b"pr: -columns must be 1 or more\n"),
        PrError::NumberLines => fail(b"pr: Number of lines must be 1 or more\n"),
        PrError::OffsetValue => fail(b"pr: -o offset must be 1 or more\n"),
        // C's string really does end in a space before the newline (`pr.c:1800`).
        PrError::WidthValue => fail(b"pr: -w width must be 1 or more \n"),
        PrError::NumberWidth => fail(b"pr: -n width must be 1 or more\n"),
        PrError::InvalidValue(letter, value) => {
            support::warn(format!("pr: invalid value for -{letter} {value}\n").as_bytes());
            support::terminate(1);
        }
        PrError::AcrossNeedsColumns => fail(b"pr: -a flag requires multiple columns\n"),
        PrError::MergeAcross => fail(b"pr: -m cannot be used with -a\n"),
        PrError::ColumnTooSmall(n) => {
            support::warn(format!("pr: page width is too small for {n} columns\n").as_bytes());
            support::terminate(1);
        }
        PrError::Unmodelled => {
            support::warn(b"pr: option form not modelled yet\n");
            support::terminate(1);
        }
    }
}

fn fail(message: &[u8]) -> ! {
    support::warn(message);
    support::terminate(1);
}

/// C's `usage()` text verbatim, three lines whose second and third each start with
/// ten spaces (`pr.c:1618-1626`). It is written as one literal because a Rust line
/// continuation would eat exactly those leading spaces.
const USAGE: &[u8] = b"usage: pr [+page] [-col] [-adFfmprt] [-e[ch][gap]] [-h header]\n          [-i[ch][gap]] [-l line] [-n[ch][width]] [-o offset]\n          [-s[ch]] [-T timefmt] [-w width] [-] [file ...]\n";

/// C's `usage()` is three lines and does not itself exit; `main` then leaves with
/// status 1 because `setup` reported the failure (`pr.c:1616-1627`, `:158-160`).
fn usage() -> ! {
    support::warn(USAGE);
    support::terminate(1);
}

#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
