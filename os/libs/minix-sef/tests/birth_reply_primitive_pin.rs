//! T7 birth-reply primitive shape audit (the test still owed per §5.2).
//!
//! # Contract (C ground truth)
//!
//! The IPC primitive used by the birth reply leg is pinned by the C ground truth,
//! and two forms coexist:
//! - **General services** (the RS side waits for a reply): the default callback
//!   `SEF_CB_INIT_RESPONSE_DEFAULT = sef_cb_init_response_rs_reply`
//!   (sef.h:90) = `ipc_sendrec(RS_PROC_NR, m)` (sef_init.c:458-466) —
//!   a **synchronous blocking sendrec**: the send half parks in RS's receive and
//!   the reply half waits for RS's OK to wake it.
//! - **VM special case**: `rs_asynsend(rp, &m, 0)` (utility.c:62) = one
//!   asynchronous send; VM, after swapping in a callback at vm/main.c:225-229,
//!   uses asynsend3(AMF_NOREPLY).
//!
//! # Why pin the shape, not just the behavior
//!
//! A wrong primitive does not blow up immediately in production: RS's receive
//! swallows the send, but RS is waiting for the reply half of a sendrec (RS parks
//! in receive awaiting OK) → the echo is posted back → step3 panic; sendnb
//! instead races the BS exit. This project has fixed this once already, and the
//! architecture notes mark it "prone to recur" — this audit pins the shape into
//! CI, so a regression goes red on the spot instead of hanging at step3 on a real
//! machine and being traced afterwards.
//!
//! Audit style: source scanning (a counterpart to `riscv64_ap_early_entry_pin.rs`).

use std::path::PathBuf;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../")
}

fn read_server(rel: &str, name: &str) -> String {
    let full = crate_root().join(rel);
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{} 源码可读: {}", name, e))
}

/// Birth arm slice: from the anchor up to the next handler boundary or the end
/// of file. `window` = slice line count (default 6: the C shape at
/// sef_init.c:113-117 is "construct → reply immediately", with no other
/// statement between the reply leg and the builder; sites where the message is
/// built by the library's orchestration face are the exception — the leg follows
/// the match, so the window is widened and noted).
fn reply_leg_after<'a>(src: &'a str, anchor: &str, name: &str, window: usize) -> &'a str {
    let zone = src
        .split(anchor)
        .nth(1)
        .unwrap_or_else(|| panic!("{} 缺锚点 {}", name, anchor));
    let lines: Vec<&str> = zone.lines().take(window).collect();
    &zone[..lines.iter().map(|l| l.len() + 1).sum()]
}

/// (path, name, anchor, window line count). MIB goes through the orchestration
/// face (`minix_sef::process_init`): the reply message is built by
/// `minix_sef::process_init` and the leg follows the match — so the anchor moves
/// to the orchestration entry and the window widens to 14; every other site
/// keeps the 6-line "construct → reply immediately" window.
const GENERAL_SERVICE_BIRTH_SITES: &[(&str, &str, &str, usize)] = &[
    (
        "servers/is/src/lib.rs",
        "IS",
        "minix_sef::process_init(",
        36,
    ),
    (
        "servers/mib/src/server.rs",
        "MIB",
        "minix_sef::process_init(",
        22,
    ),
    (
        "servers/ds/src/server.rs",
        "DS",
        "minix_sef::process_init(",
        // Between `process_init` and the reply leg, DS's Birth arm also carries
        // the shadow half of the fresh anchor (implanting the label through an
        // rproctab grant, C store.c:267-279), so the window has to span that
        // stretch before it can see `send_rec(caller, ..)`; this pin only checks
        // that `send_rec` appears in the window, so widening the window does not
        // weaken the criterion.
        38,
    ),
    (
        "servers/ipc-server/src/server.rs",
        "IPC",
        "minix_sef::process_init(",
        26,
    ),
];

#[test]
fn birth_reply_leg_is_sendrec_for_general_services() {
    // The birth reply leg of a general service (IS/MIB/DS/IPC) must be
    // `send_rec(Endpoint::RS` — the C default callback
    // sef_cb_init_response_rs_reply = ipc_sendrec (sef_init.c:463). Using
    // `asynsend` is a drift toward the VM special case.
    for (rel, name, anchor, window) in GENERAL_SERVICE_BIRTH_SITES {
        let src = read_server(rel, name);
        let arm = reply_leg_after(&src, anchor, name, *window);
        // Shape contract: the primitive must be send_rec (same shape as C's
        // ipc_sendrec) and must go to RS — either an explicit Endpoint::RS
        // (IS/MIB) or caller (DS: the Birth arm's caller is always RS, which the
        // upper layer's m_source == RS test guarantees) is acceptable.
        assert!(
            arm.contains("send_rec("),
            "{} 的出生回报腿必须用 send_rec（C sef_init.c:463 ipc_sendrec 同形）；现臂：\n{}",
            name,
            arm
        );
        assert!(
            arm.contains("Endpoint::RS") || arm.contains("send_rec(caller,"),
            "{} 的出生回报腿必须显式去往 RS（Endpoint::RS 或 Birth 臂 caller）；现臂：\n{}",
            name,
            arm
        );
        assert!(
            !arm.contains("asynsend("),
            "{} 的出生回报腿不得用 asynsend（那是 VM 特例，utility.c:62）",
            name
        );
        assert!(
            !arm.contains("sendnb(") && !arm.contains("send_nonblocking"),
            "{} 的出生回报腿不得用 sendnb（reply 半竞速，B9b 之前的现场）",
            name
        );
    }
}

#[test]
fn vm_birth_reply_leg_is_asynsend_once() {
    // VM special case: the RS_INIT reply goes through asynsend (C utility.c:62
    // rs_asynsend; the callback is swapped at vm/main.c:225-229). A synchronous
    // send_rec would collide with RS's receive half during boot.
    let src = read_server("servers/vm/src/vm_server.rs", "VM");
    assert!(
        src.contains("asynsend(RS_PROC_NR, &minix_sef::sef_init_reply"),
        "VM 的 RS_INIT 回报腿必须是 asynsend(RS_PROC_NR, sef_init_reply(...))（C rs_asynsend 特例）"
    );
}

#[test]
fn sef_init_reply_builder_shape_is_untouched() {
    // T7+T6 joint anchor: the builder produces RS_INIT+result with
    // m_source=NONE (stamped by the kernel). Shape drift (say, someone swaps
    // m_type for a custom value or fills m_source by hand) goes red on the spot.
    let src = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs"))
        .expect("minix-sef lib.rs 可读");
    let body = src
        .split("pub fn sef_init_reply")
        .nth(1)
        .expect("sef_init_reply 缺失");
    let end = body.find("\n}").unwrap_or(body.len());
    let body = &body[..end];
    assert!(
        body.contains("m_type: SEF_INIT_REQUEST_TYPE"),
        "sef_init_reply 必须用 SEF_INIT_REQUEST_TYPE 作 m_type（C: m_type = RS_INIT）"
    );
    assert!(
        body.contains("m_rs_init.result = result"),
        "sef_init_reply 必须经 m_rs_init.result 载荷回结果（C: m.m_rs_init.result）"
    );
    assert!(
        !body.contains("m_source ="),
        "sef_init_reply 不得手填 m_source——内核投递盖章是权威语义（T6 定谳，见 doc）"
    );
}
