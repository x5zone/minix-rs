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
use minix_text_games::acronym::matching_expansions;

const DEFAULT_DB: &str = "/usr/share/misc/acronyms";

fn run() -> ! {
    let argv: Vec<String> = support::args();
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let mut db_files: Vec<&str> = Vec::new();
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
                        // 脚本 `:21` 是 `acronyms="$OPTARG $acronyms"`——每一枚 `-f` 都
                        // **插到列表最前面**，所以 `-f w1 -f w2` 的搜索次序是 w2 先。
                        // 现量两档各有 `LOL` 时输出 `LOL: second file` 换行 `first file`。
                        db_files.insert(0, path);
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

    // 脚本 `:42-56` 造那个档列表：`-f` 给过就用 `-f` 的（`ACRONYMDB` 此时**整支不看**，
    // 现量 `ACRONYMDB=<有该词的库> sh … -f /etc/hostname lol` → 查无）；否则 `ACRONYMDB`
    // 非空时把它的整串当列表（`acronyms="$ACRONYMDB"`，随后 `for f in $acronyms` 按
    // **空白断词**，现量 `ACRONYMDB="w1  w2 "` 这种连排与尾部空格都能命中 w2 里的条目）；
    // 两者都空才去对 `/usr/share/misc/acronyms*` 做通配展开——那一支要目录枚举，本 crate
    // 现在拿不到（`dents` 住在 `minix-fileops`），所以退回单档缺省名，这一形已登记。
    let env_list = if db_files.is_empty() {
        env_value("ACRONYMDB").unwrap_or_default()
    } else {
        String::new()
    };
    let mut files: Vec<&str> = db_files;
    if files.is_empty() {
        files.extend(env_list.split_ascii_whitespace());
    }
    if files.is_empty() {
        files.push(DEFAULT_DB);
    }

    // 列表里每一档都要**先**过一遍：脚本 `:64-69` 是一条独立的前置圈，逐档
    // `[ ! -f $f ]`，第一档开不了就打 `cannot open acronym database file \`<档>'` 并
    // **当场退 1**——后面还有好档也不查、也不印任何命中行（现量
    // `-f <含 LOL 的档> -f <不存在> lol` 只出那一句、退 1；反序同）。
    // 前置圈过了之后才逐档读进同一个行池（`:76` 的 `fgrep -h $acronyms` 是把所有档
    // 当一个流搜），顺序就是列表顺序。
    // 开档读档走 `support::slurp_operand` 的双生面（真机腿 `minix_sys::{open, read, close}`、
    // 宿主腿 `std::fs`）。改前这里直接写死 `minix_sys::open`，**宿主上任何真实存在的库档都报
    // `cannot open` 退 1**——与批次三百六十一给 `fortune` 治的是同一枚缺陷。
    let mut lines: Vec<&str> = Vec::new();
    let mut pool: Vec<String> = Vec::new();
    for path in &files {
        let image = match support::slurp_operand(path) {
            Ok(image) => image,
            Err(support::OperandFailure::Open(_)) => {
                // 脚本原句 `wtf: cannot open acronym database file \`<路径>'`（`:67`，实测同）。
                support::warn(
                    format!("wtf: cannot open acronym database file `{path}'\n").as_bytes(),
                );
                support::terminate(1);
            }
        };
        pool.push(String::from_utf8_lossy(&image).into_owned());
    }
    for text in &pool {
        lines.extend(text.lines());
    }

    let mut status = 0;
    for term in &terms {
        // 脚本 `:75` 先把查询词转大写（`tr '[a-z]' '[A-Z]'`）再拿它输出，实测
        // `ACRONYMDB=<库> sh … nasa` → `NASA: National Aeronautics and Space Administration`。
        // 改前本壳原样输出查询词，等于把「打印哪个形态」这件真值漏掉了。
        let shown = term.to_ascii_uppercase();
        let expansions = matching_expansions(&lines, term);
        if !expansions.is_empty() {
            // 一个词命中**多行**时（同档重复词条，或两档各有该词），`:77` 的 `sed` 把所有
            // 匹配行都留下、`:80` 的 `echo "$target: $ans"` 只在**第一行**前面加 `TARGET: `，
            // 后面的行裸着出。现量：同档两行 `LOL` → `LOL: first match` 换行 `second match`。
            let joined = expansions.join("\n");
            support::emit(format!("{shown}: {joined}\n").as_bytes());
            continue;
        }
        {
            // 脚本原句走的是**操作数原样**而不是大写化的 target：`:105` 写的是
            // `echo "$PROGNAME: I don't know what \`$i' means!"`（`$i` 是移位后的
            // 实参，`$target` 只在命中那支用）。实测 `… zzz` → ``I don't know what
            // `zzz' means!``，本壳一度印成 `ZZZ` 已经改回。句子本身走 stderr、
            // 末了 `rv=1`（`:106-107`）。
            support::warn(format!("wtf: I don't know what `{term}' means!\n").as_bytes());
            status = 1;
        }
    }
    support::terminate(status)
}

/// 环境里 `NAME` 的值：按 C 的 `getenv` 取**第一条**带 `NAME=` 前缀的串并去掉前缀。
/// 与 `os/commands/bin/fileops/src/bin/du.rs:env_value` 同法——两处的宿主/真机双生面
/// 各自住在自己的 `bin_support`，所以这份扫描也跟着各留一份（提到共件要跨 crate，
/// 那属 `os/libs/` 的归属决定，不在本轮射程）。
fn env_value(name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    support::envs().iter().find_map(|entry| {
        let text = core::str::from_utf8(entry).ok()?;
        text.strip_prefix(prefix.as_str()).map(String::from)
    })
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
