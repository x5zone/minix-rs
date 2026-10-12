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

use minix_sys::read;
use minix_textfilter::pr::{
    PrError, column_width, merge_body_rows, paginate, paginate_files, parse_pr_args, render_page,
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
        let input = read_stdin();
        files.push(("", split_lines(&input)));
    } else {
        for op in &operands {
            if op == "-" {
                let input = read_stdin();
                files.push(("", split_lines(&input)));
            } else {
                match support::read_file(op) {
                    Ok(bytes) => files.push((op.as_str(), split_lines(&bytes))),
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
            for line in render_page(&page, options.offset) {
                let mut out = line.as_bytes().to_vec();
                out.push(b'\n');
                support::emit(&out);
            }
        }
        support::terminate(if had_error { 1 } else { 0 });
    }

    for page in paginate_files(&files, &options, title_override.as_deref()) {
        for line in render_page(&page, options.offset) {
            let mut out = line.as_bytes().to_vec();
            out.push(b'\n');
            support::emit(&out);
        }
    }
    support::terminate(if had_error { 1 } else { 0 });
}

fn read_stdin() -> Vec<u8> {
    let mut input = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match read(support::STDIN, &mut chunk) {
            Ok(0) => break,
            Ok(count) => input.extend_from_slice(&chunk[..count]),
            Err(_) => break,
        }
    }
    input
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

/// Map a [`PrError`] from `parse_pr_args` to the exact C error path (message and
/// exit code) the old in-bin parser used, then terminate.
fn arg_error(e: PrError) -> ! {
    match e {
        PrError::Usage => usage(),
        PrError::ColumnTooSmall(n) => {
            support::warn(format!("pr: page width is too small for {n} columns\n").as_bytes());
            usage()
        }
        PrError::AcrossNeedsColumns => {
            support::emit(b"pr: -a flag requires multiple columns\n");
            usage()
        }
        PrError::Unmodelled => {
            support::warn(b"pr: option form not modelled yet\n");
            support::terminate(2);
        }
    }
}

fn usage() -> ! {
    support::warn(
        b"usage: pr [-t] [-h header] [-l lines] [-d] [+page] [-columns] [-a] [-m] [-w width] [-s[C]] [-o offset] [-n[C][width]]\n",
    );
    support::terminate(2);
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
