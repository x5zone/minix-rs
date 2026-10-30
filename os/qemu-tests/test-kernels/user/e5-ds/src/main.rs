//! e5-ds — the real-machine half of the E5(f) end-to-end integration
//! carrier: a walk-through of the DS data service transport chain.
//!
//! This is not just another unit test: it runs in user mode (CPL3) after a
//! real machine boot and sends real requests to the DS server (endpoint 6)
//! through minix-sys's `DsClient`. One round trip must stand up four legs at
//! once — `SYS_SETGRANT` (registers the grant table in this process's
//! privilege structure, the SYSCALL leg), `_taskcall` on `int 0x21` vector
//! 33 (the IPC leg), the DS server's `run()` send/receive loop, and the
//! server-side `sys_safecopyfrom`/`sys_safecopyto` (cross-space copies). The
//! checks are recorded segment by segment: whichever leg breaks, the serial
//! output stops at that leg's marker rather than at a bare "no error at the
//! end".
//!
//! **Caller identity decides which legs are available** (ground truth:
//! `minix3/minix/servers/ds/store.c`). DS's send/receive surface is tiered
//! by whether the caller has a label: `ds_getprocname()` (store.c:110-126)
//! recognizes only DS itself (hard-coded "ds") and the TYPE_LABEL entries in
//! the ds_store. This carrier is an ordinary user process born from init→sh
//! and RS does not know it, so it has no label, and therefore:
//!
//! - `do_publish` source gate → EPERM (store.c:297-300)
//! - `do_delete` source gate → EPERM (store.c:588-591)
//! - `do_subscribe` source gate → ESRCH (store.c:466-469, "subscribers need a
//!   name")
//! - `do_check` unknown source → ESRCH (store.c:544-550)
//! - `do_retrieve` has **no source gate** (store.c:380-398 reads the key name
//!   and then looks it up directly) → a normal round trip; the key-name ferry
//!   goes through `get_key_name`→`sys_safecopyfrom` (store.c:160-172) and the
//!   value write-back through `sys_safecopyto`: both copy legs are exercised
//!   on real hardware.
//!
//! These rejections are **correct behavior** and are asserted one by one.
//! C's `dstest.c` can publish/delete/subscribe because the test suite brings
//! dstest up as an RS service per `minix3/minix/tests/ds/system.conf` (RS
//! publishes a label for it). This tree does not yet have that dynamic
//! service surface, so the executable half of the publish family is recorded
//! as "gated rather than broken"; its final real-machine state is tracked
//! under E5(f).
//!
//! The label surface has its own explicit readout `E5f-LABELS n/12`: in C,
//! DS cold-boot initialization pulls RS's whole rprocpub table through
//! `sef_cb_init_fresh` and implants labels one by one with `map_service`
//! (store.c:251-282); in this tree that half's decode is still pending
//! (DS's `main.rs` states it starts from the reset state), so today's
//! expected readout is 0. The readout and the criterion are written
//! separately; once the implant lands the number should flip to 12, and the
//! gate's second criterion tightens accordingly.

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

// The freestanding side keeps a link reference to minix-rt (the same note as
// the DS server's main): the bin code itself does not go through any symbol
// in rt, so an unreferenced rlib is dropped entirely and the trio
// `_start`/`#[global_allocator]`/`#[panic_handler]` disappears with it (this
// carrier uses `Vec`, and cannot link without an allocator). The command side
// gets the same effect from the `minix_rt::crt0::args` reference in
// `bin_support.rs`; this file does not read argv, hence the explicit
// `extern crate`.
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_sys::ds::DsClient;
use minix_sys::grant::GrantTable;
use minix_sys::ipc::DirectTrapTransport;
use minix_sys::syscall::DirectKernelCallTransport;
use minix_sys::Fd;
use minix_types::{DS_MAX_KEYLEN, DsFlags, Endpoint, EPERM, ESRCH};

/// Standard output, POSIX `STDOUT_FILENO` (the same VFS leg as /bin/echo).
const STDOUT: Fd = 1;

/// Common prefix of the per-segment markers; the gate script counts "how far
/// we got" by it.
const TAG: &str = "e5-ds: ";

/// The DS labels of the boot services and their agreed slots. The name
/// authority is RS's boot service table
/// (`os/servers/rs/src/table.rs:132-149`, mirroring C
/// `minix3/minix/servers/rs/table.c:15-30`'s `boot_image_priv_table`) —
/// note it does not fully match the on-disk installed file names (the MFS
/// row is called `fs_imgrd` on the label surface, same as C table.c:27).
/// Once label implantation lands, each of these resolves back to itself.
const SERVICES: [(&str, Endpoint); 12] = [
    ("ds", Endpoint::DS),
    ("rs", Endpoint::RS),
    ("pm", Endpoint::PM),
    ("sched", Endpoint::SCHED),
    ("vfs", Endpoint::VFS),
    ("memory", Endpoint::MEM),
    ("tty", Endpoint::TTY),
    ("mib", Endpoint::MIB),
    ("vm", Endpoint::VM),
    ("pfs", Endpoint::PFS),
    ("fs_imgrd", Endpoint::MFS),
    ("init", Endpoint::INIT),
];

// ── Serial output helpers (no stdio; this file carries its own decimal formatting) ──

fn emit(bytes: &[u8]) {
    // A failed console write has no recoverable way out: a lost marker makes
    // the gate FAIL — do not swallow it silently.
    let _ = minix_sys::write(STDOUT, bytes);
}

fn say(s: &str) {
    emit(s.as_bytes());
}

/// Decimal output (negatives keep their sign; the magnitude is taken as u64,
/// avoiding the i64::MIN absolute-value overflow).
fn say_num(v: i64) {
    let mut buf = [0u8; 24];
    say(dec(v, &mut buf));
}
/// Small decimal buffer: write the number into a fixed stack buffer and
/// return the visible substring (zero heap allocation).
fn dec<'a>(v: i64, buf: &'a mut [u8; 24]) -> &'a str {
    let mut i = buf.len();
    let negative = v < 0;
    let mut n = v.unsigned_abs();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if negative {
        i -= 1;
        buf[i] = b'-';
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("?")
}

/// Segment success marker: `e5-ds: <step> ok`.
fn step_ok(name: &str) {
    say(TAG);
    say(name);
    say(" ok\n");
}

/// Fail and terminate: print the mismatch description and the value; the last
/// line is the FAIL marker for the gate script. C's dstest aborts directly
/// with `assert()` (dstest.c:26 etc.); this carrier replaces the abort with a
/// readable termination — there is no core dump to inspect on real hardware.
fn fail(what: &str, value: i64) -> ! {
    say(TAG);
    say("FAIL ");
    say(what);
    say(" = ");
    say_num(value);
    say("\n");
    say("### TEST_RESULT: FAIL e5-ds ###\n");
    minix_sys::exit(1)
}

/// Mismatch report: print both the expected and the actual code (the
/// rejection code is itself the criterion; do not conflate them). `got_ok`
/// writes "successful reply" separately from "errno 0" — they have the same
/// value on the wire, and without the split one cannot tell whether DS
/// judged success or merely lost the sign of the error code.
fn mismatch_ok(what: &str, want: i32) {
    say(TAG);
    say("FAIL ");
    say(what);
    say(" want ");
    say_num(want as i64);
    say(" got OK\n");
}

fn mismatch(what: &str, want: i32, got: i32) {
    say(TAG);
    say("FAIL ");
    say(what);
    say(" want ");
    say_num(want as i64);
    say(" got ");
    say_num(got as i64);
    say("\n");
}

/// One request must return a specific rejection code: only an actual code
/// equal to the expected one counts as "chain works, semantics rejected".
/// `Ok(true)` lets the caller accumulate the failure count so several
/// mismatches in one segment are visible at once.
fn expect_errno(what: &str, want: i32, r: Result<(), i32>) -> bool {
    match r {
        Err(e) if e == want => true,
        Err(e) => {
            mismatch(what, want, e);
            false
        }
        Ok(()) => {
            mismatch_ok(what, want);
            false
        }
    }
}

// ── Transport-chain criteria: the correct landing points for a label-less caller on DS ──

/// Six real task calls. What is asserted is C's rejection codes — the code
/// itself proves the request reached DS, the key-name ferry read our granted
/// memory, the decision layer ran, and the reply came back.
fn leg_chain(ds: &mut DsClient<DirectTrapTransport, DirectKernelCallTransport>,
             grants: &mut GrantTable) {
    // Emit an "about to make the first taskcall" marker first: if the serial
    // output stops here, the request went out but DS is not serving (blocked
    // in sendrec), which is distinguishable from "chain works but semantics
    // rejected".
    say(TAG);
    say("step chain begin (first taskcall to DS endpoint ");
    say_num(Endpoint::DS.get() as i64);
    say(")\n");

    let mut bad = 0usize;

    // 1) retrieve has no source gate (store.c:380-398): an absent key → ESRCH.
    //    This covers get_key_name's sys_safecopyfrom (store.c:160-172).
    let r = ds.retrieve_u32(grants, "e5.absent.key").map(|_| ());
    if !expect_errno("retrieve absent u32", ESRCH, r) {
        bad += 1;
    }

    // 2) retrieve_str: the same key ferry, plus a WRITE value grant
    //    registration (the grant lifecycle has the same shape for
    //    publish/retrieve; no copy is triggered when the key does not exist).
    let mut buf = [0u8; 32];
    let r = ds.retrieve_str(grants, "e5.absent.key", &mut buf).map(|_| ());
    if !expect_errno("retrieve absent str", ESRCH, r) {
        bad += 1;
    }

    // 3) retrieve_mem: the TYPE_MEM tier, same ESRCH.
    let r = ds.retrieve_mem(grants, "e5.absent.key", &mut buf).map(|_| ());
    if !expect_errno("retrieve absent mem", ESRCH, r) {
        bad += 1;
    }

    // 4) publish source gate (store.c:297-300) → EPERM.
    let r = ds.publish_u32(grants, "e5.test_u32", 1234, DsFlags::empty());
    if !expect_errno("publish u32 nameless", EPERM, r) {
        bad += 1;
    }

    // 5) delete source gate (store.c:588-591) → EPERM.
    let r = ds.delete(grants, "e5.test_u32", DsFlags::TYPE_U32);
    if !expect_errno("delete nameless", EPERM, r) {
        bad += 1;
    }

    // 6) subscribe source gate (store.c:466-469) → ESRCH (not EPERM; C's choice).
    let r = ds.subscribe(grants, "e5.test_u32", 0);
    if !expect_errno("subscribe nameless", ESRCH, r) {
        bad += 1;
    }

    // 7) check unknown source (store.c:544-550) → ESRCH.
    let mut key = [0u8; DS_MAX_KEYLEN];
    match ds.check(grants, &mut key) {
        Err(ESRCH) => {}
        Err(e) => {
            mismatch("check nameless", ESRCH, e);
            bad += 1;
        }
        Ok(None) => {
            mismatch_ok("check nameless", ESRCH);
            bad += 1;
        }
        Ok(Some(_)) => {
            say(TAG);
            say("FAIL check nameless want ESRCH got event\n");
            bad += 1;
        }
    }

    if bad != 0 {
        fail("chain leg mismatches", bad as i64);
    }
    step_ok("chain");
}

// ── Label-surface readout: how many of RS→DS's 12 service labels are implanted ──

/// Resolve each service name's label and print the count as `E5f-LABELS n/12`.
///
/// For each resolved label (the expectation once implantation lands is all
/// 12) two more things are done: reconcile the endpoint slot against
/// `MODULE_NAMES`, and a reverse `retrieve_label_name` round trip — the
/// latter is the `sys_safecopyto` leg writing a value into our granted
/// memory, so both directions are real. Unresolved names only record a
/// readout today and do not fail, because the failure point is the implant
/// half, not the transport chain.
fn leg_labels(ds: &mut DsClient<DirectTrapTransport, DirectKernelCallTransport>,
              grants: &mut GrantTable) {
    let mut resolved = 0usize;
    for (name, expected) in SERVICES.iter() {
        match ds.retrieve_label_endpt(grants, name) {
            Ok((ep, _flags)) => {
                resolved += 1;
                say(TAG);
                say("label ");
                say(name);
                say(" = ");
                say_num(ep.get() as i64);
                say("\n");
                if (ep.get() & 0xFF) != expected.get() & 0xFF {
                    fail("label slot mismatch for", expected.get() as i64);
                }
                let mut back = [0u8; DS_MAX_KEYLEN];
                match ds.retrieve_label_name(grants, ep, &mut back) {
                    Ok(len) => {
                        // C ds.c:155's discipline: the last byte is pinned to NUL and the name body precedes it.
                        let body = &back[..len];
                        let cut = body.iter().position(|&b| b == 0).unwrap_or(body.len());
                        if &body[..cut] != name.as_bytes() {
                            fail("reverse name mismatch for", expected.get() as i64);
                        }
                    }
                    Err(e) => fail("retrieve_label_name", e as i64),
                }
            }
            Err(e) => {
                say(TAG);
                say("label ");
                say(name);
                say(" unresolved errno ");
                say_num(e as i64);
                say("\n");
            }
        }
    }
    say(TAG);
    say("E5f-LABELS ");
    say_num(resolved as i64);
    say("/12\n");
}

// ── Notification family: the carrier's boundary (no empty fake self-checks) ──
//
// E5(f)'s notify loop criterion (2) (C `subs.c:20-45`: subscribe → DS
// `ipc_notify`s the subscriber on update → `ds_check` retrieves the key name)
// needs a **subscriber with a label**: the subscription table is keyed by
// owner name (store.c:466-469; no name means ESRCH), and labels can only be
// published by RS (the rprocpub implant at store.c:251-282 plus dynamic
// publication at manager.c:787-862). This carrier cannot reach that leg, so
// it does not fabricate a "notification received" readout here.

/// The body: the single execution sequence shared by the two main shapes.
fn run() -> ! {
    let mut ds = DsClient::new(
        DirectTrapTransport,
        DirectKernelCallTransport,
        Endpoint::DS,
    );
    let mut grants = GrantTable::new();

    say(TAG);
    say("begin (E5(f) DS transport-chain carrier)\n");

    leg_chain(&mut ds, &mut grants);
    leg_labels(&mut ds, &mut grants);

    say("### TEST_RESULT: PASS e5-ds ###\n");
    minix_sys::exit(0)
}

/// Installed-image entry: minix-rt's crt0 spawn chain resolves `main` by
/// name (consumer contract: name + Rust ABI + `-> i32`, all three required).
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

/// Host-form entry: the std build goes through rustc's startup glue, so
/// `main` must return `Termination` (the `i32` type does not implement it;
/// rustc 1.94 reports E0277).
#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
