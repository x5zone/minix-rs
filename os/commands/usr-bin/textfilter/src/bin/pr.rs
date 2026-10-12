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
//! or each file's mtime; `pr.h:63`). The multi-column forms (`-column`, `-a`,
//! `-m`), double spacing (`-d`), page offsets (`-o`), the formfeed (`-F`), and
//! the starting page (`+first`) are later batches. An unopenable operand warns
//! and is skipped with a nonzero exit (`strerror` suffix not reproduced).


#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::pr::{PrOptions, paginate_files};
use minix_sys::read;

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let mut options = PrOptions::default();
    let mut title_override: Option<String> = None;
    let mut operands: Vec<String> = Vec::new();
    let mut index = 1;
    while index < argv.len() {
        let arg = argv[index].as_str();
        if arg == "-t" {
            options.no_header = true;
        } else if arg == "-h" {
            index += 1;
            if index >= argv.len() {
                usage();
            }
            title_override = Some(argv[index].clone());
        } else if arg == "-l" {
            index += 1;
            if index >= argv.len() {
                usage();
            }
            match argv[index].parse::<usize>() {
                // pr.c:1875-1878: the page must carry header and trailer.
                Ok(length) if length > 10 => options.page_len = length,
                _ => usage(),
            }
        } else if arg.starts_with('+') {
            // `+first_page` and multi-column forms are later batches.
            support::warn(b"pr: +page and -column forms are not modelled yet\n");
            support::terminate(2);
        } else if arg.len() > 1 && arg.starts_with('-') {
            // An option letter we do not model (`-d`, `-a`, `-m`, `-column`).
            support::warn(b"pr: option form not modelled yet\n");
            support::terminate(2);
        } else {
            // A file operand; `-` names standard input (pr.c:1338).
            operands.push(String::from(arg));
        }
        index += 1;
    }

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

    for page in paginate_files(&files, &options, title_override.as_deref()) {
        for line in page.lines() {
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

fn usage() -> ! {
    support::warn(b"usage: pr [-t] [-h header] [-l lines]\n");
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
