//! Minix-RS kill — the doing half over `minix_proctools::signal`.
//!
//! Ground truth: `minix3/bin/kill/kill.c` — `-l` lists the names (line
//! 105), a glued `-name` goes through `signame_to_signum` (line 119),
//! `-s name` through the same (line 127), the default signal is SIGTERM
//! (line 83), and failures flip the exit status while the loop carries
//! on. Numbers go straight through when they land in 1..=32 (`NSIG`).

use minix_proctools::signal::{signame_to_signum, signum_to_signame, SIGNALS};
use minix_sys::kill;

/// Terminates the process with an exit status.
fn terminate(code: i32) -> ! {
    std::process::exit(code)
}

fn usage() -> ! {
    eprintln!("usage: kill [-s signame | -signum | -signame] {{pid | -pgid}} ...");
    eprintln!("       kill -l [exit_status]");
    terminate(1)
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();

    // `kill -l`: the name list, eight per line (`kill.c:100-112`).
    if args.len() == 2 && args[1] == "-l" {
        for (index, (_, name)) in SIGNALS.iter().enumerate() {
            print!("{name} ");
            if index % 8 == 7 {
                println!();
            }
        }
        println!();
        terminate(0);
    }
    // `kill -l status`: the name of `status & 0177` when it is known
    // (`kill.c:114-117` region).
    if args.len() == 3 && args[1] == "-l" {
        let status: i32 = match args[2].parse() {
            Ok(v) => v,
            Err(_) => usage(),
        };
        match signum_to_signame((status % 128) as u8) {
            Some(name) => println!("{name}"),
            None => usage(),
        }
        terminate(0);
    }

    let mut numsig: u8 = 15; // SIGTERM (`kill.c:83`)
    let mut at = 1;
    if let Some(spec) = args.get(1).filter(|_| args.len() >= 2) {
        if let Some(name) = spec.strip_prefix("-s") {
            // `-sname` glued or `-s name` split (`kill.c:123-131`).
            let text = if name.is_empty() {
                at += 1;
                args.get(2).copied().unwrap_or_else(|| usage())
            } else {
                name
            };
            numsig = parse_signal(text);
            at += 2;
        } else if spec.len() > 1 && spec.starts_with('-') {
            // `-name` or `-number` (`kill.c:114-122`).
            let text = &spec[1..];
            numsig = match text.parse::<u8>() {
                Ok(v) if (1..=32).contains(&v) => v,
                _ => parse_signal(text),
            };
            at += 2;
        }
    }

    let mut status = 0;
    for pid_text in &args[at..] {
        let pid: i32 = match pid_text.parse() {
            Ok(v) => v,
            Err(_) => {
                eprintln!("kill: {pid_text}: illegal pid");
                status = 1;
                continue;
            }
        };
        if pid == 0 {
            eprintln!("kill: {pid}: illegal pid");
            status = 1;
            continue;
        }
        // Negative pids are process groups; the kernel call takes them
        // verbatim (`kill.c:141-150`).
        if kill(pid, numsig as i32).is_err() {
            eprintln!("kill: {pid}: {numsig}: no such process");
            status = 1;
        }
    }
    terminate(status)
}

fn parse_signal(text: &str) -> u8 {
    match signame_to_signum(text) {
        Ok(v) => v,
        Err(_) => usage(),
    }
}
