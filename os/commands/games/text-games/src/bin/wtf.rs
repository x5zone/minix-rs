//! Minix-RS wtf — the doing half over `minix_text_games::acronym`.
//!
//! Ground truth: `minix3/games/wtf/wtf`（`-f` 指库、`-o` 只认全字、
//! 开头那一枚 `is` 被移走而后面出现的 `is` 照查；库行是 `term: expansion`）。决定半
//! （行解析、大小写不敏感首中查询）在库内；本壳拼参数、读库、
//! 逐词打印 `term: expansion`，任一词查无即退出 1。
//!
//! 库默认 `/usr/share/misc/acronyms`（NetBSD wtf 的缺省路径）。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[path = "../bin_support.rs"]
mod support;
use minix_text_games::acronym::lookup_acronym;

const DEFAULT_DB: &str = "/usr/share/misc/acronyms";

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let mut db_path = DEFAULT_DB;
    let mut at = 1;
    // 真源 `minix3/games/wtf/wtf:16-27` 走 `getopts f:o`，认不出来的旗标一律 `usage`
    // 退 1（实测 `sh minix3/games/wtf/wtf -Z nasa` → 先一句 shell 自己的
    // `Illegal option -Z`、再程序自己的 usage、退码 1）。改前本壳把 `-Z` 当查询词咽了下去。
    // 那句 `Illegal option …` 是 shell 印的，不是 wtf 印的——与批次三百七十六对 `wtf`
    // 写腿的结论同一条：判据不归这个程序，所以本壳只复用它自己那句 usage。
    //
    // 停环的位置由 `getopts` 定：第一个非选项 token 一到就停，**不重排**，剩下的整段
    // 经脚本 `:30` 的 `shift "$(expr "$OPTIND" - 1)"` 交回位置参数。
    while at < args.len() {
        match args[at] {
            "-f" => {
                at += 1;
                match args.get(at).copied() {
                    Some(path) => {
                        db_path = path;
                        at += 1;
                    }
                    // 实测 `sh … -f` → shell 一句 `No arg for -f option` + 程序的 usage
                    // 退 1；本壳自创的 `wtf: -f needs a database path` 两句都不是，改掉。
                    None => usage(),
                }
            }
            "-o" => {
                // 只认全字的旗标：决定半的查询本就是全字匹配，语义已含。
                // 真正挑 `*-o` 那批库档要的是通配展开，那是 A9c 的第②条。
                at += 1;
            }
            "--" => {
                at += 1;
                break;
            }
            leading if leading.starts_with('-') && leading.len() > 1 => usage(),
            _ => break,
        }
    }
    // 脚本对 `is` 只做了这一件事（`:32-34`）：`if [ "$1" = "is" ]; then shift; fi`——
    // 移走**开头**那一枚，一次，而且比的是字面量（`IS` 不算，它是个查询词）。
    // 之后每个 token 都是查询词，**包括后面再出现的 `is`**。三形现量
    // （`sh minix3/games/wtf/wtf -f minix3/share/misc/acronyms …`）：
    // `y2k is` → `y2k` 与 `is` 各一句「I don't know what …」；
    // `is is y2k` → 先 `is` 后 `y2k`（只移走开头那一枚）；
    // `IS lol` → `IS` 查无一句，随后仍印 `LOL: laughing out loud`，退 1。
    // 旧写法在查询循环里跳过任意位置的 `is`，还把它做成大小写不敏感的
    // `is_skipped_word`，于是上面三形全与参照件分叉（且退码也差）。
    let mut terms: Vec<&str> = args[at.min(args.len())..].to_vec();
    if terms.first().copied() == Some("is") {
        terms.remove(0);
    }
    if terms.is_empty() {
        usage();
    }

    // 开档读档走 `support::slurp_operand` 的双生面（真机腿 `minix_sys::{open, read, close}`、
    // 宿主腿 `std::fs`）。改前这里直接写死 `minix_sys::open`，**宿主上任何真实存在的库档都报
    // `cannot open` 退 1**——与批次三百六十一给 `fortune` 治的是同一枚缺陷。
    let image = match support::slurp_operand(db_path) {
        Ok(image) => image,
        Err(support::OperandFailure::Open(_)) => {
            // 脚本原句 `wtf: cannot open acronym database file \`<路径>'`（`:67`，实测同）。
            support::warn(
                format!("wtf: cannot open acronym database file `{db_path}'\n").as_bytes(),
            );
            support::terminate(1);
        }
    };

    let text = String::from_utf8_lossy(&image).into_owned();
    let lines: Vec<&str> = text.lines().collect();

    let mut status = 0;
    for term in &terms {
        // 脚本 `:75` 先把查询词转大写（`tr '[a-z]' '[A-Z]'`）再拿它输出，实测
        // `ACRONYMDB=<库> sh … nasa` → `NASA: National Aeronautics and Space Administration`。
        // 改前本壳原样输出查询词，等于把「打印哪个形态」这件真值漏掉了。
        let shown = term.to_ascii_uppercase();
        match lookup_acronym(&lines, term) {
            Ok(expansion) => support::emit(format!("{shown}: {expansion}\n").as_bytes()),
            Err(_) => {
                // 脚本原句走的是**操作数原样**而不是大写化的 target：`:105` 写的是
                // `echo "$PROGNAME: I don't know what \`$i' means!"`（`$i` 是移位后的
                // 实参，`$target` 只在命中那支用）。实测 `… zzz` → ``I don't know what
                // `zzz' means!``，本壳一度印成 `ZZZ` 已经改回。句子本身走 stderr、
                // 末了 `rv=1`（`:106-107`）。
                support::warn(format!("wtf: I don't know what `{term}' means!\n").as_bytes());
                status = 1;
            }
        }
    }
    support::terminate(status)
}

/// 脚本的 `usage()`（`minix3/games/wtf/wtf:11-14`）是 `echo` 到**标准输出**再退 1，
/// 实测分路读数：stdout 收到 `usage: wtf [-o] [-f dbfile] [is] term ...`、stderr 空。
/// 改前本壳把它发到 stderr，且文本里没有 `[-o]`。
fn usage() -> ! {
    support::emit(b"usage: wtf [-o] [-f dbfile] [is] term ...\n");
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
