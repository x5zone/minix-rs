//! minix3/minix/tests Rust 腿翻译 —— 内存域(test6/44/64/75)。
//!
//! # 翻译映射
//!
//! VM 服务器本体 crate 外不可链接(模块全 `pub(crate)`),本文件钉的是
//! **用户态半的字节契约**:C libc(`libsys`/`libc/sys`)的 brk/mmap/munmap/
//! vm_fork 包装把哪些字段放上 wire、失败如何回传——用 `CannedTransport`
//! 脚本应答并在宿主真跑。VM 服务器消费半由 `minix-vm` crate 内单测覆盖
//! (mmap 19 测、brk 8 测、fork 10 测),两侧对账即完整链路。
//!
//! | C 测试 | 语义归宿 |
//! |---|---|
//! | test6(brk/sbrk) | [`brk_short_circuits_and_adopts_approved_boundary`] |
//! | test44(mmap) | [`mmap_carries_seven_lanes_and_third_party_flag`] |
//! | test44(munmap) | [`munmap_carries_addr_and_len`] |
//! | test64(mmap 跨 fork 的内核半) | [`fork_address_space_carries_endpoint_and_slot`] |
//! | test75(getrusage)/test85(块设备 EOF) | getrusage 归 PM time 面(未翻译,挂账);块设备 EOF 归 `t_fs_special` 后续条目 |
//!
//! # 曾钉住的 C↔Rust 语义缺口(已修)
//!
//! `minix_sys::vm::fork_address_space_via` 曾用裸偏移(载荷 0/4 写、0 读),
//! 而本树 `include/minix/ipc.h` 的 `mess_1` 首成员是 `uint64_t m1ull1`,
//! `vm_fork.c:21` 的 `VMF_CHILD_ENDPOINT`(= m1i3,`com.h:635`)不在偏移 0。
//! 现已改为命名 `mess_1` 车道(VMF_ENDPOINT/VMF_SLOTNO/VMF_CHILD_ENDPOINT),
//! 与 VM 侧 `VmForkIn`/`VmForkOut` 的命名解码一致。
//!
//! 交付门 = 编译;全部测试 `#[ignore]`,点亮前提见各测试属性。

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
// test6 —— brk:捷径、采纳与拒绝
// ---------------------------------------------------------------------------

/// C 语义(`libc/sys/brk.c:22-34`):请求等于缓存边界时不发服务器直接成功;
/// 否则上 wire 请裁决,成功才采纳新边界;失败原边界不动。
#[test]
#[ignore = "点亮前提:VM brk 消费半随载体点亮后复核"]
fn brk_short_circuits_and_adopts_approved_boundary() {
    let cached = VirBytes(0x1000_0000);

    // 捷径:请求 == 缓存 → 不产生任何 round trip。
    let transport = CannedTransport::new();
    let got = break_via(&transport, cached, cached).expect("捷径直接成功");
    assert_eq!(got, cached);
    assert!(transport.sent.borrow().is_empty(), "捷径不上 wire");

    // 服务器批准:出站载荷首 8 字节 = 请求边界,成功后返回请求值。
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

    // 服务器拒绝:Err 携带 errno,调用方不得采纳。
    let mut transport = CannedTransport::new();
    transport.reply_sendrec(Ok(Message {
        m_type: -ENOMEM,
        ..Message::default()
    }));
    let err = break_via(&transport, cached, requested).expect_err("拒绝必须传播");
    assert_eq!(err.to_i32(), ENOMEM, "ENOMEM 原样回传");
}

// ---------------------------------------------------------------------------
// test44 —— mmap:七条字段车道与第三方旗
// ---------------------------------------------------------------------------

/// C 语义(`libc/sys/mmap.c:21-47` 的 `minix_mmap_for`):七个映射字段入
/// `mess_mmap` 车道;受益者非调用者时自动加 MAP_THIRDPARTY;应答从 retaddr
/// 车道读回选定地址。
#[test]
#[ignore = "点亮前提:VM mmap 消费半随载体点亮后复核"]
fn mmap_carries_seven_lanes_and_third_party_flag() {
    // 自映射:不带第三方旗。
    let mut transport = CannedTransport::new();
    let mut reply = ok_reply();
    // 应答的活跃车道是 retaddr(C mmap.c:46);union 字段赋值无读,
    // 不需要 unsafe。
    reply.m_u.m_mmap.retaddr = 0x5000_0000;
    transport.reply_sendrec(Ok(reply));

    let request = MapRequest {
        beneficiary: CALLER,
        address: VirBytes(0),
        length: VirBytes(0x10_0000),
        protection: 0x3, // PROT_READ|PROT_WRITE
        flags: 0x1000,   // MAP_ANON(minix 侧旗)
        file: -1,
        offset: 0,
    };
    let chosen = mmap_via(&transport, CALLER, request).expect("自映射成功");
    assert_eq!(chosen, VirBytes(0x5000_0000), "地址从 retaddr 车道读回");

    let sent = transport.sent.borrow();
    let (_, msg) = &sent[0];
    // SAFETY(test): m_mmap 是 VM_MMAP 的文档化载荷臂。
    let m: &MessMmap = unsafe { &msg.m_u.m_mmap };
    assert_eq!(m.addr, 0, "提示地址车道");
    assert_eq!(m.len, 0x10_0000, "长度车道");
    assert_eq!(m.prot, 0x3, "保护位车道");
    assert_eq!(m.fd, -1, "匿名映射 fd = -1");
    assert_eq!(m.forwhom, CALLER.0, "受益者 = 调用者");
    assert_eq!(m.flags as u32 & MAP_FLAG_THIRD_PARTY, 0, "自映射无第三方旗");

    // 为他人映射:自动加 MAP_THIRDPARTY(C mmap.c:36-38)。
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
// test44 —— munmap:地址与长度车道
// ---------------------------------------------------------------------------

/// C 语义(`libc/sys/mmap.c:76-85`):munmap 把地址与长度放上同一 overlay
/// 的 addr/len 车道,应答只看成败。
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
// test64 —— vm_fork:端点与槽位车道
// ---------------------------------------------------------------------------

/// C 语义(`libsys/vm_fork.c:10-24`;`mess_1` 命名车道):端点与槽位走
/// `VMF_ENDPOINT`/`VMF_SLOTNO`(m1i1/m1i2,`com.h:633-634`);应答的子端点
/// 在 `VMF_CHILD_ENDPOINT` = m1i3(`com.h:635`,`vm_fork.c:21` 读取);
/// 负回码转 errno。
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
    // SAFETY(test): m1i1/m1i2 是请求的活跃车道(vm_fork.c:16-17)。
    let m1 = unsafe { &msg.m_u.m_m1 };
    assert_eq!(m1.m1i1, 0x102, "父端点走 VMF_ENDPOINT 车道");
    assert_eq!(m1.m1i2, 7, "槽位走 VMF_SLOTNO 车道");
}
