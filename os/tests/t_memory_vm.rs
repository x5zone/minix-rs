//! Rust-leg translation of the memory-domain tests from minix3/minix/tests
//! (test6/44/64/75).
//!
//! # Translation mapping
//!
//! The VM server crate itself is not linkable from outside (all modules are
//! `pub(crate)`), so this file pins the **byte contract of the user-space
//! half**: which fields the C libc (`libsys`/`libc/sys`) brk/mmap/munmap/
//! vm_fork wrappers put on the wire and how failures come back — using
//! `CannedTransport` to script replies and really running on the host. The VM
//! server's consuming half is covered by unit tests inside the `minix-vm` crate
//! (19 mmap tests, 8 brk tests, 10 fork tests); reconciling the two sides is
//! the complete chain.
//!
//! | C test | Semantic home |
//! |---|---|
//! | test6(brk/sbrk) | [`brk_short_circuits_and_adopts_approved_boundary`] |
//! | test44(mmap) | [`mmap_carries_seven_lanes_and_third_party_flag`] |
//! | test44(munmap) | [`munmap_carries_addr_and_len`] |
//! | test64(mmap across fork, kernel half) | [`fork_address_space_carries_endpoint_and_slot`] |
//! | test75(getrusage)/test85(block device EOF) | getrusage belongs to the PM time face (not translated; on the books); block device EOF belongs to a later `t_fs_special` item |
//!
//! # C↔Rust semantic gap previously pinned (now fixed)
//!
//! `minix_sys::vm::fork_address_space_via` once used raw offsets (payload
//! written at 0/4, read at 0), while this tree's `include/minix/ipc.h`
//! `mess_1` first member is `uint64_t m1ull1`, and `vm_fork.c:21`'s
//! `VMF_CHILD_ENDPOINT` (= m1i3, `com.h:635`) does not sit at offset 0. It now
//! uses the named `mess_1` lanes (VMF_ENDPOINT/VMF_SLOTNO/VMF_CHILD_ENDPOINT),
//! consistent with the VM side's named `VmForkIn`/`VmForkOut` decode.
//!
//! Delivery gate = compilation; all tests are `#[ignore]`, and each test's
//! attribute states the precondition for enabling it.

use minix_sys::ipc::CannedTransport;
use minix_sys::vm::{
    MAP_FLAG_THIRD_PARTY, MapRequest, break_via, fork_address_space_via, mmap_via, munmap_via,
    vm_endpoint,
};
use minix_types::{ENOMEM, MessMmap, Message, OK, VirBytes};

const CALLER: minix_types::Endpoint = minix_types::Endpoint(0x102);

fn ok_reply() -> Message {
    Message {
        m_type: OK,
        ..Message::default()
    }
}

// ---------------------------------------------------------------------------
// test6 — brk: shortcut, adoption and rejection
// ---------------------------------------------------------------------------

/// C semantics (`libc/sys/brk.c:22-34`): when the request equals the cached
/// boundary, succeed directly without sending to the server; otherwise put it
/// on the wire for a decision and adopt the new boundary only on success; on
/// failure the old boundary is untouched.
#[test]
#[ignore = "点亮前提:VM brk 消费半随载体点亮后复核"]
fn brk_short_circuits_and_adopts_approved_boundary() {
    let cached = VirBytes(0x1000_0000);

    // Shortcut: request == cache → no round trip at all.
    let transport = CannedTransport::new();
    let got = break_via(&transport, cached, cached).expect("捷径直接成功");
    assert_eq!(got, cached);
    assert!(transport.sent.borrow().is_empty(), "捷径不上 wire");

    // Server approves: the first 8 bytes of the outbound payload are the
    // requested boundary, and on success the request value comes back.
    let requested = VirBytes(0x1001_0000);
    let mut transport = CannedTransport::new();
    transport.reply_sendrec(Ok(ok_reply()));
    let got = break_via(&transport, cached, requested).expect("批准后采纳");
    assert_eq!(got, requested, "返回值 = 请求边界(C brk.c:34)");
    let sent = transport.sent.borrow();
    assert_eq!(sent.len(), 1);
    let (dest, msg) = &sent[0];
    assert_eq!(*dest, vm_endpoint(), "brk 发往 VM");
    let wire = unsafe { &msg.m_u.raw[..8] };
    assert_eq!(wire, &requested.0.to_ne_bytes(), "载荷首 8 字节 = 请求边界");

    // Server rejects: `Err` carries the errno and the caller must not adopt.
    let mut transport = CannedTransport::new();
    transport.reply_sendrec(Ok(Message {
        m_type: -ENOMEM,
        ..Message::default()
    }));
    let err = break_via(&transport, cached, requested).expect_err("拒绝必须传播");
    assert_eq!(err.to_i32(), ENOMEM, "ENOMEM 原样回传");
}

// ---------------------------------------------------------------------------
// test44 — mmap: seven field lanes and the third-party flag
// ---------------------------------------------------------------------------

/// C semantics (`libc/sys/mmap.c:21-47`'s `minix_mmap_for`): seven mapping
/// fields enter the `mess_mmap` lanes; when the beneficiary is not the caller,
/// MAP_THIRDPARTY is added automatically; the reply reads the chosen address
/// from the retaddr lane.
#[test]
#[ignore = "点亮前提:VM mmap 消费半随载体点亮后复核"]
fn mmap_carries_seven_lanes_and_third_party_flag() {
    // Self mapping: no third-party flag.
    let mut transport = CannedTransport::new();
    let mut reply = ok_reply();
    // The reply's active lane is retaddr (C mmap.c:46); writing a union field
    // has no read, so no unsafe is needed.
    reply.m_u.m_mmap.retaddr = 0x5000_0000;
    transport.reply_sendrec(Ok(reply));

    let request = MapRequest {
        beneficiary: CALLER,
        address: VirBytes(0),
        length: VirBytes(0x10_0000),
        protection: 0x3, // PROT_READ|PROT_WRITE
        flags: 0x1000,   // MAP_ANON (minix-side flag)
        file: -1,
        offset: 0,
    };
    let chosen = mmap_via(&transport, CALLER, request).expect("自映射成功");
    assert_eq!(chosen, VirBytes(0x5000_0000), "地址从 retaddr 车道读回");

    let sent = transport.sent.borrow();
    let (_, msg) = &sent[0];
    // SAFETY(test): m_mmap is VM_MMAP's documented payload arm.
    let m: &MessMmap = unsafe { &msg.m_u.m_mmap };
    assert_eq!(m.addr, 0, "提示地址车道");
    assert_eq!(m.len, 0x10_0000, "长度车道");
    assert_eq!(m.prot, 0x3, "保护位车道");
    assert_eq!(m.fd, -1, "匿名映射 fd = -1");
    assert_eq!(m.forwhom, CALLER.0, "受益者 = 调用者");
    assert_eq!(m.flags as u32 & MAP_FLAG_THIRD_PARTY, 0, "自映射无第三方旗");

    // Mapping for someone else: MAP_THIRDPARTY is added automatically
    // (C mmap.c:36-38).
    let mut transport = CannedTransport::new();
    transport.reply_sendrec(Ok(ok_reply()));
    let request = MapRequest {
        beneficiary: minix_types::Endpoint(0x201),
        ..request
    };
    let _ = mmap_via(&transport, CALLER, request);
    let sent = transport.sent.borrow();
    let (_, msg) = &sent[0];
    let m: &MessMmap = unsafe { &msg.m_u.m_mmap };
    assert_eq!(m.forwhom, 0x201, "受益者车道 = 他人");
    assert_eq!(
        m.flags as u32 & MAP_FLAG_THIRD_PARTY,
        MAP_FLAG_THIRD_PARTY,
        "他人受益自动加第三方旗(C mmap.c:36-38)"
    );
}

// ---------------------------------------------------------------------------
// test44 — munmap: address and length lanes
// ---------------------------------------------------------------------------

/// C semantics (`libc/sys/mmap.c:76-85`): munmap puts the address and length on
/// the addr/len lanes of the same overlay, and the reply only reports
/// success/failure.
#[test]
#[ignore = "点亮前提:VM munmap 消费半随载体点亮后复核"]
fn munmap_carries_addr_and_len() {
    let mut transport = CannedTransport::new();
    transport.reply_sendrec(Ok(ok_reply()));

    munmap_via(&transport, VirBytes(0x5000_0000), VirBytes(0x10_0000)).expect("munmap 成功");

    let sent = transport.sent.borrow();
    let (_, msg) = &sent[0];
    let m: &MessMmap = unsafe { &msg.m_u.m_mmap };
    assert_eq!(m.addr, 0x5000_0000, "地址车道");
    assert_eq!(m.len, 0x10_0000, "长度车道");
}

// ---------------------------------------------------------------------------
// test64 — vm_fork: endpoint and slot lanes
// ---------------------------------------------------------------------------

/// C semantics (`libsys/vm_fork.c:10-24`; named `mess_1` lanes): the endpoint
/// and slot go on `VMF_ENDPOINT`/`VMF_SLOTNO` (m1i1/m1i2, `com.h:633-634`); the
/// reply's child endpoint is read from `VMF_CHILD_ENDPOINT` = m1i3 (`com.h:635`,
/// read at `vm_fork.c:21`); a negative reply code turns into the errno.
#[test]
fn fork_address_space_carries_endpoint_and_slot() {
    let parent = minix_types::Endpoint(0x102);
    let mut transport = CannedTransport::new();
    let mut reply = ok_reply();
    reply.m_u.m_m1.m1i3 = 0x103;
    transport.reply_sendrec(Ok(reply));

    let child = fork_address_space_via(&transport, parent, 7).expect("vm_fork 成功");
    assert_eq!(
        child,
        minix_types::Endpoint(0x103),
        "子端点从 VMF_CHILD_ENDPOINT(m1i3)读回"
    );

    let sent = transport.sent.borrow();
    let (dest, msg) = &sent[0];
    assert_eq!(*dest, vm_endpoint());
    // SAFETY(test): m1i1/m1i2 are the request's active lanes (vm_fork.c:16-17).
    let m1 = unsafe { &msg.m_u.m_m1 };
    assert_eq!(m1.m1i1, 0x102, "父端点走 VMF_ENDPOINT 车道");
    assert_eq!(m1.m1i2, 7, "槽位走 VMF_SLOTNO 车道");
}
