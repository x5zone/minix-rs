//! 端到端门的公共夹具：把输入喂给已启动的真子进程，并解释为什么写标准输入不能 panic。
//!
//! 每一条 `*_face.rs` 门都是「起真进程 → 写标准输入 → 收三条通道」这三步。第二步有个
//! 容易被忽略的前提：**命令可能根本就读标准输入**。C 的诊断支大多排在任何读之前
//! （`minix3/usr.bin/pr/pr.c` 的 `usage()`、`minix3/usr.bin/vis/vis.c` 的集外字母支、
//! `minix3/games/primes/primes.c` 的负号支都是这种形状），子进程一退出，管道的读端就关闭，
//! 父进程这时的 `write_all` 拿到 `BrokenPipe`。
//!
//! 这件事**发不发生取决于父子双方谁先被调度**：输入小到一个 64K 管道缓冲能装下时，
//! 父进程往往在子进程退出之前就写完了，于是同一条用例时红时绿。实测形态是
//! `pr_face` 单目标三连跑 1 红 2 绿、`vis_face` 在同容器串跑多门那一趟独红
//! （取证见 `rewrite-notes/18-stage-commands/todo.md` 批次二百九十九那条的登记③）。
//! 把它当 panic，就是把门自己的竞态当成了被测代码的缺陷。
//!
//! 因此这里只放过 `BrokenPipe`，其余写错当场断言失败——否则门会静默失去输入而假绿。

use std::io::ErrorKind;
use std::io::Write as _;
use std::process::Child;

/// 把 `input` 写进子进程的标准输入并关闭它（关闭是必须的：命令普遍读到文件尾才收工）。
///
/// 子进程在读之前就退出的那些用例里，这一写会拿到 `BrokenPipe`，那是被测形状的正常
/// 结果，不是门的失败；判据仍在 stdout、stderr 与退码三条通道上，它们由调用方的
/// `wait_with_output` 收集，与写入是否完整无关。
pub fn write_stdin(child: &mut Child, input: &[u8]) {
    let mut stdin = child.stdin.take().expect("stdin piped");
    if let Err(err) = stdin.write_all(input) {
        assert_eq!(
            err.kind(),
            ErrorKind::BrokenPipe,
            "写标准输入只该因为「子进程已退出」而失败，实际是 {err}"
        );
    }
}
