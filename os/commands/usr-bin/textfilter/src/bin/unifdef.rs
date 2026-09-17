//! Minix-RS unifdef — selectively remove C preprocessor conditionals.
//!
//! Ground truth: `minix3/usr.bin/unifdef/unifdef.c`, usage
//! `unifdef [-cdeklst] [-o output] [-Dsym[=val]] [-Usym]
//! [-iDsym[=val]] [-iUsym] ... [file]`. The deciding half (the ten-
//! state #if machine and expression evaluator) is the library's
//! `unifdef` module; this program parses the command line, reads the
//! input (stdin or one file), and reports the exit status: 0 when the
//! input was already clean, 1 when lines were dropped, 2 on trouble.
//! `-d` (debugging) and `-e` (obfuscated-directive tolerance) are
//! accepted and ignored — the DODGY table columns they gate are a
//! registered corner. File access goes through `std::fs` (the file
//! open seam, edge E-CMDSYSFACE). The hosted-versus-target seams are
//! the echo template's (`os/commands/bin/fileops/src/bin/echo.rs`).

#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::unifdef::{add_symbol, Options, Unifdef, UnifdefError};
use minix_sys::read;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut options = Options::default();
    let mut ofilename: Option<String> = None;
    let mut operand: Option<String> = None;

    let mut i = 1;
    while i < argv.len() {
        let arg = argv[i].clone();
        if !arg.starts_with('-') || arg.len() == 1 {
            if operand.is_some() {
                support::warn(b"unifdef: can only do one file\n");
                support::terminate(2);
            }
            operand = Some(arg);
            i += 1;
            continue;
        }
        let chars: Vec<char> = arg.chars().skip(1).collect();
        let mut k = 0usize;
        while k < chars.len() {
            let c = chars[k];
            match c {
                'D' | 'U' | 'i' => {
                    // -Dsym[=val] / -Usym / -i[D|U]sym: the symbol is
                    // attached or the next argument.
                    let (ignore, kind) = if c == 'i' {
                        k += 1;
                        match chars.get(k) {
                            Some('D') => (true, true),
                            Some('U') => (true, false),
                            _ => {
                                support::warn(b"unifdef: -i must be followed by D or U\n");
                                usage();
                            }
                        }
                    } else {
                        (false, c == 'D')
                    };
                    k += 1;
                    let spec = if k < chars.len() {
                        Some(chars[k..].iter().collect::<String>())
                    } else {
                        i += 1;
                        argv.get(i).cloned()
                    };
                    match spec {
                        Some(s) => {
                            if let Err(msg) = add_symbol(&mut options, &s, kind, ignore) {
                                support::warn(format!("unifdef: {}\n", msg).as_bytes());
                                usage();
                            }
                        }
                        None => usage(),
                    }
                }
                'c' => options.complement = true,
                'd' => {} // debugging reports: no-op here
                'e' => {} // iocccok: DODGY columns not modeled
                'k' => options.killconsts = true,
                'l' => options.lnblank = true,
                's' => options.symlist = true,
                't' => options.text = true,
                'I' => {} // cpp compatibility: ignored
                'o' => {
                    k += 1;
                    if k < chars.len() {
                        ofilename = Some(chars[k..].iter().collect());
                    } else {
                        i += 1;
                        ofilename = argv.get(i).cloned();
                        if ofilename.is_none() {
                            usage();
                        }
                    }
                }
                _ => usage(),
            }
            k += 1;
        }
        i += 1;
    }

    if options.symbols.is_empty() && !options.symlist {
        support::warn(b"unifdef: must -D or -U at least one symbol\n");
        usage();
    }

    let input = match operand.as_deref() {
        None | Some("-") => {
            let mut v = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                match read(support::STDIN, &mut chunk) {
                    Ok(0) => break,
                    Ok(n) => v.extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                }
            }
            v
        }
        Some(path) => match std::fs::read(path) {
            Ok(v) => v,
            Err(_) => {
                support::warn(format!("unifdef: can't open {}\n", path).as_bytes());
                support::terminate(2);
            }
        },
    };
    let text = String::from_utf8_lossy(&input).into_owned();

    match Unifdef::new(options).run(&text) {
        Ok(outcome) => {
            match ofilename {
                Some(path) => {
                    if let Err(_) = std::fs::write(&path, &outcome.output) {
                        support::warn(format!("unifdef: can't open {}\n", path).as_bytes());
                        support::terminate(2);
                    }
                }
                None => support::emit(&outcome.output),
            }
            support::terminate(outcome.exitstat);
        }
        Err(err) => {
            support::warn(error_text(&err).as_bytes());
            support::terminate(2);
        }
    }
}

fn error_text(err: &UnifdefError) -> String {
    match err {
        UnifdefError::EofInComment => "unifdef: EOF in comment\n".to_string(),
        UnifdefError::InappropriateElif { line, if_line, depth } => {
            if *depth == 0 {
                format!("unifdef: {}: Inappropriate #elif\n", line)
            } else {
                format!("unifdef: {}: Inappropriate #elif (#if line {} depth {})\n", line, if_line, depth)
            }
        }
        UnifdefError::InappropriateElse { line } => {
            format!("unifdef: {}: Inappropriate #else\n", line)
        }
        UnifdefError::InappropriateEndif { line } => {
            format!("unifdef: {}: Inappropriate #endif\n", line)
        }
        UnifdefError::PrematureEof => "unifdef: Premature EOF\n".to_string(),
        UnifdefError::ObfuscatedControlLine { line } => {
            format!("unifdef: {}: Obfuscated preprocessor control line\n", line)
        }
        UnifdefError::TooDeep { line } => {
            format!("unifdef: {}: Too many levels of nesting\n", line)
        }
    }
}

fn usage() -> ! {
    support::warn(
        b"usage: unifdef [-cdeklst] [-o output] [-Dsym[=val]] [-Usym] [-iDsym[=val]] [-iUsym] ... [file]\n",
    );
    support::terminate(2);
}
