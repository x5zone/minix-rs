//! Minix-RS tr — translate or delete bytes.
//!
//! Ground truth: `minix3/usr.bin/tr/tr.c` (NetBSD), `tr [-cs] string1
//! [string2]`. Modes carried by the library: translate (set1 maps onto
//! set2, the last set2 byte repeating), delete (`-d`, set1 removed), and
//! squeeze (`-s`, repeats in set2 — or set1 with `-d` — collapse to one).
//! `-c` complements set1 before the mode applies (the translate map then
//! sends every unlisted byte to the last set2 byte, matching the C).
//! An empty input is success; a bad set or a nonsense combination exits 1.
//!
//! The set grammar (ranges, repeats, `[:class:]`) is the library's
//! `parse_set`, which expands named classes inline; the complement is a
//! membership inversion over the expanded set.

#[path = "../bin_support.rs"]
mod support;

use minix_textfilter::tr::{delete, parse_set, squeeze, translate};

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let mut complement = false;
    let mut delete_flag = false;
    let mut squeeze_flag = false;
    let mut operands: Vec<&str> = Vec::new();
    for arg in &argv[1..] {
        if arg.starts_with('-') && arg.len() > 1 {
            for flag in arg[1..].bytes() {
                match flag {
                    b'c' => complement = true,
                    b'd' => delete_flag = true,
                    b's' => squeeze_flag = true,
                    _ => usage(),
                }
            }
            continue;
        }
        operands.push(arg);
    }
    match (delete_flag, squeeze_flag) {
        (true, true) => delete_squeeze(operands, complement),
        (true, false) => delete_only(operands, complement),
        // One set squeezes itself; two sets translate first (C: both
        // `tr -s a` and `tr -s a b` are valid).
        (false, true) if operands.len() == 1 => squeeze_only(operands, complement),
        (false, true) => translate_squeeze(operands, complement),
        (false, false) => translate_only(operands, complement),
    }
}

/// `tr -d set1` needs exactly one set.
fn delete_only(mut operands: Vec<&str>, complement: bool) {
    if operands.len() != 1 {
        usage();
    }
    let set = expand(operands.remove(0), complement);
    let input = support::read_stdin();
    let mut out = vec![0u8; input.len() + 1];
    match delete(&input, &set, &mut out) {
        Ok(len) => support::emit(&out[..len]),
        Err(_) => support::terminate(1),
    }
    support::terminate(0);
}

/// `tr -s set1`: collapse repeats of set1.
fn squeeze_only(mut operands: Vec<&str>, complement: bool) {
    let set = expand(operands.remove(0), complement);
    let input = support::read_stdin();
    let mut out = vec![0u8; input.len() + 1];
    match squeeze(&input, &set, &mut out) {
        Ok(len) => support::emit(&out[..len]),
        Err(_) => support::terminate(1),
    }
    support::terminate(0);
}

/// `tr [-c] set1 set2` with an optional `-s` on set2.
fn translate_squeeze(operands: Vec<&str>, complement: bool) {
    if operands.len() != 2 {
        usage();
    }
    let set1 = expand(operands[0], complement);
    let set2 = expand(operands[1], false);
    let input = support::read_stdin();
    let mut stage = vec![0u8; input.len() + 1];
    let len = match translate(&input, &set1, &set2, &mut stage) {
        Ok(len) => len,
        Err(_) => support::terminate(1),
    };
    let mut out = vec![0u8; len + 1];
    let len = match squeeze(&stage[..len], &set2, &mut out) {
        Ok(len) => len,
        Err(_) => support::terminate(1),
    };
    support::emit(&out[..len]);
    support::terminate(0);
}

/// `tr -c -s set1`: delete set1, then squeeze repeats of set1.
fn delete_squeeze(mut operands: Vec<&str>, complement: bool) {
    if operands.len() != 1 {
        usage();
    }
    let set = expand(operands.remove(0), complement);
    let input = support::read_stdin();
    let mut stage = vec![0u8; input.len() + 1];
    let len = match delete(&input, &set, &mut stage) {
        Ok(len) => len,
        Err(_) => support::terminate(1),
    };
    let mut out = vec![0u8; len + 1];
    let len = match squeeze(&stage[..len], &set, &mut out) {
        Ok(len) => len,
        Err(_) => support::terminate(1),
    };
    support::emit(&out[..len]);
    support::terminate(0);
}

/// `tr [-c] set1 set2` without `-s`.
fn translate_only(operands: Vec<&str>, complement: bool) {
    if operands.len() != 2 {
        usage();
    }
    let set1 = expand(operands[0], complement);
    let set2 = expand(operands[1], false);
    let input = support::read_stdin();
    let mut out = vec![0u8; input.len() + 1];
    match translate(&input, &set1, &set2, &mut out) {
        Ok(len) => support::emit(&out[..len]),
        Err(_) => support::terminate(1),
    }
    support::terminate(0);
}

/// Expands one set expression into its 256-byte universe; a complemented
/// set lists every byte the expression did not (ascending).
fn expand(text: &str, complement: bool) -> Vec<u8> {
    let mut expanded = [0u8; 256];
    let used = match parse_set(text, &mut expanded) {
        Ok((used, _, _)) => used,
        Err(_) => support::terminate(1),
    };
    if !complement {
        return expanded[..used].to_vec();
    }
    let mut member = [false; 256];
    for &byte in &expanded[..used] {
        member[byte as usize] = true;
    }
    (0..=255u8)
        .filter(|byte| !member[*byte as usize])
        .collect()
}

fn usage() -> ! {
    support::emit(b"usage: tr [-cs] string1 [string2]\n");
    support::terminate(1);
}
