//! Minix-RS units — conversion program.
//!
//! Ground truth: `minix3/usr.bin/units/units.c`, usage
//! `units [-Llqv] [-f filename] [[count] from-unit to-unit]`. The
//! deciding half (units-file parsing, expression reduction,
//! conformability, answer rendering, the `-l`/`-L` listings) is the
//! library's `units` module. The units database is read through
//! `std::fs` (the file-open seam, edge E-CMDSYSFACE) from `-f` or the
//! default `/usr/lib/units` path; the PATH search of the C is a
//! registered corner. The interactive prompt face reads stdin when
//! no operands are given, prompts on stdout like the C's printf. The
//! hosted-versus-target seams are the echo template's
//! (`os/commands/bin/fileops/src/bin/echo.rs`).

#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::units::{
    add_unit, complete_reduce, list_units, read_units, render_answer, show_answer, Answer,
    UnitsTable, PRECISION, PRECISION_LIST_EXPAND,
};
use minix_sys::read;

const UNITSFILE: &str = "/usr/lib/units";

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut userfile: Option<String> = None;
    let mut list = false;
    let mut listexpand = false;
    let mut quiet = false;

    let mut i = 1;
    while i < argv.len() {
        let arg = argv[i].clone();
        if !arg.starts_with('-') || arg.len() == 1 {
            break;
        }
        for c in arg[1..].chars() {
            match c {
                'l' => list = true,
                'L' => {
                    list = true;
                    listexpand = true;
                }
                'q' => quiet = true,
                'f' => {
                    i += 1;
                    match argv.get(i) {
                        Some(v) => userfile = Some(v.clone()),
                        None => usage(),
                    }
                }
                'v' => {
                    support::emit(
                        b"\n  units version 1.0  Copyright (c) 1993 by Adrian Mariano\n                    This program may be freely distributed\n",
                    );
                    usage();
                }
                _ => usage(),
            }
        }
        i += 1;
    }

    let operands: Vec<String> = argv[i..].to_vec();
    if operands.len() > 3 || (list && !operands.is_empty()) {
        usage();
    }

    let text = load_units_file(userfile.as_deref());
    let table = read_units(&split_file_lines(&text));

    if list {
        let prec = if listexpand { PRECISION_LIST_EXPAND } else { PRECISION };
        let (text, errors) = list_units(&table, listexpand, prec);
        support::emit(text.as_bytes());
        support::terminate(if errors > 0 { 1 } else { 0 });
    }

    match operands.len() {
        2 | 3 => {
            // `units 3 meters feet`: the count joins the have side.
            let have_expr = if operands.len() == 3 {
                format!("{} {}", operands[0], operands[1])
            } else {
                operands[0].clone()
            };
            let want_expr = operands[operands.len() - 1].clone();
            match convert(&table, &have_expr, &want_expr) {
                Some(answer) => {
                    support::emit(render_answer(&answer, PRECISION).as_bytes());
                    support::terminate(0);
                }
                None => support::terminate(1),
            }
        }
        0 => {
            // Interactive face: "You have: "/"You want: " pairs.
            if !quiet {
                support::emit(
                    format!(
                        "{} units, {} prefixes\n\n",
                        table.units.len(),
                        table.prefixes.len()
                    )
                    .as_bytes(),
                );
            }
            loop {
                if !quiet {
                    support::emit(b"You have: ");
                }
                let have_line = match read_line() {
                    Some(l) => l,
                    None => {
                        if !quiet {
                            support::emit(b"\n");
                        }
                        support::terminate(0);
                    }
                };
                if !quiet {
                    support::emit(b"You want: ");
                }
                let want_line = match read_line() {
                    Some(l) => l,
                    None => {
                        if !quiet {
                            support::emit(b"\n");
                        }
                        support::terminate(0);
                    }
                };
                if let Some(answer) = convert(&table, &have_line, &want_line) {
                    support::emit(render_answer(&answer, PRECISION).as_bytes());
                }
            }
        }
        _ => usage(),
    }
}

fn convert(table: &UnitsTable, have_expr: &str, want_expr: &str) -> Option<Answer> {
    let mut have = add_unit(have_expr, false).ok()?;
    complete_reduce(table, &mut have).ok()?;
    let mut want = add_unit(want_expr, false).ok()?;
    complete_reduce(table, &mut want).ok()?;
    Some(show_answer(&have, &want))
}

/// Reads the units database: `-f` path or the default; a missing file
/// exits 1 like the C's `err(1, ...)`.
fn load_units_file(path: Option<&str>) -> Vec<u8> {
    match path.unwrap_or(UNITSFILE) {
        real => match std::fs::read(real) {
            Ok(v) => v,
            Err(_) => {
                support::warn(format!("units: can't open {}\n", real).as_bytes());
                support::terminate(1);
            }
        },
    }
}

fn split_file_lines(bytes: &[u8]) -> Vec<&str> {
    let text = std::str::from_utf8(bytes).unwrap_or("");
    let mut lines: Vec<&str> = Vec::new();
    for line in text.split_inclusive('\n') {
        lines.push(line.strip_suffix('\n').unwrap_or(line));
    }
    lines
}

fn read_line() -> Option<String> {
    let mut line = Vec::new();
    let mut chunk = [0u8; 1];
    loop {
        match read(support::STDIN, &mut chunk) {
            Ok(0) => {
                if line.is_empty() {
                    return None;
                }
                break;
            }
            Ok(_) => {
                if chunk[0] == b'\n' {
                    break;
                }
                line.push(chunk[0]);
            }
            Err(_) => return None,
        }
    }
    Some(String::from_utf8_lossy(&line).into_owned())
}

fn usage() -> ! {
    support::warn(b"\nunits [-Llqv] [-f filename] [[count] from-unit to-unit]\n");
    support::terminate(3);
}
