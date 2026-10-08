//! e5-ds — E5(f) 端到端联调载体的真机半：DS 数据服务传输链走查。
//!
//! 这不是又一个单元测试：它跑在真机启动后的用户态（CPL3），经 minix-sys
//! 的 `DsClient` 向 DS 服务器（端点 6）发真请求。一次往返要同时立起四条
//! 腿——`SYS_SETGRANT`（把授权表登记进本进程的特权结构，SYSCALL 腿）、
//! `int 0x21` 向量 33 的 `_taskcall`（IPC 腿）、DS 服务器 `run()` 的收发
//! 环、服务器侧 `sys_safecopyfrom`/`sys_safecopyto`（跨空间拷贝）。判据
//! 逐段落盘：哪一条腿断了，串口就停在哪一条的标记上，而不是"最后没报错"。
//!
//! **调用者身份决定可走的腿**（真源 `minix3/minix/servers/ds/store.c`）。
//! DS 的收发面按"调用者有没有 label"分档：`ds_getprocname()`（store.c:110-126）
//! 只认 DS 自己（硬编码 "ds"）与 ds_store 里的 TYPE_LABEL 条目。本载体是
//! init→sh 生下的普通用户进程，RS 不认识它，因此它没有 label，于是：
//!
//! - `do_publish` 源闸 → EPERM（store.c:297-300）
//! - `do_delete` 源闸 → EPERM（store.c:588-591）
//! - `do_subscribe` 源闸 → ESRCH（store.c:466-469，"订阅者需要名字"）
//! - `do_check` 未知源 → ESRCH（store.c:544-550）
//! - `do_retrieve` **不设源闸**（store.c:380-398 读键名后直接查表）→ 正常
//!   往返；键名 ferry 走 `get_key_name`→`sys_safecopyfrom`（store.c:160-172），
//!   值回写走 `sys_safecopyto`：两条拷贝腿都被真机走到。
//!
//! 这些拒绝码是**正确行为**，逐条断言。C 的 `dstest.c` 之所以能发布/删除/
//! 订阅，是因为测试套按 `minix3/minix/tests/ds/system.conf` 把 dstest 作为
//! RS 服务拉起（RS 给它发布 label）。本树还没有那条动态服务面，所以发布族
//! 的可执行半记为「闸住而非断链」，其真机终态回写台账 E-DSWIRE/E5(f)。
//!
//! label 面另出一个显式读数 `E5f-LABELS n/12`：C 在 DS 冷启动时经
//! `sef_cb_init_fresh` 拉 RS 的 rprocpub 整表并逐条 `map_service` 植入
//! label（store.c:251-282）；本树那半解码挂在 E-RSWIRE（DS `main.rs` 自陈
//! 从 reset 态起步），今天的期望读数是 0。读数与判据分开写，植入落地后该
//! 数应翻成 12，门的第二判据即按它收紧。

#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

extern crate alloc;

// freestanding 侧持有 minix-rt 的链接引用（DS 服务器 main 的同款注记）：
// bin 代码本身不经 rt 的任何符号，未引用的 rlib 会被整体丢弃，
// `_start`/`#[global_allocator]`/`#[panic_handler]` 三件随之消失（本载体用
// `Vec`，没分配器就链不动）。命令侧靠 `bin_support.rs` 的 `minix_rt::crt0::args`
// 引用拉到同一效果，本文件不读 argv，故显式 `extern crate`。
#[cfg(all(not(test), target_os = "none"))]
extern crate minix_rt;

use minix_sys::ds::DsClient;
use minix_sys::grant::GrantTable;
use minix_sys::ipc::DirectTrapTransport;
use minix_sys::syscall::DirectKernelCallTransport;
use minix_sys::Fd;
use minix_types::{DS_MAX_KEYLEN, DsFlags, Endpoint, EPERM, ESRCH};

/// Standard output, POSIX `STDOUT_FILENO`（与 /bin/echo 同一条 VFS 腿）。
const STDOUT: Fd = 1;

/// 逐段标记的公共前缀，门脚本按它数「走到了哪一步」。
const TAG: &str = "e5-ds: ";

/// 引导服务的 DS label 与其约定槽位。名字真源是 RS 的启动服务表
/// （`os/servers/rs/src/table.rs:132-149`，对 C
/// `minix3/minix/servers/rs/table.c:15-30` 的 `boot_image_priv_table`）——
/// 注意它与盘上装机文件名不完全相同（MFS 那一行在 label 面上叫
/// `fs_imgrd`，C table.c:27 同名）。label 植入落地后每一个都能解析回自己。
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

// ── 串口输出小工具（无 stdio，本文件自带十进制格式化）──

fn emit(bytes: &[u8]) {
    // 写控制台失败没有可恢复的出路：标记丢了门自然 FAIL——不静默吞掉。
    let _ = minix_sys::write(STDOUT, bytes);
}

fn say(s: &str) {
    emit(s.as_bytes());
}

/// 十进制输出（负数带符号；幅值按 u64 取，避开 i64::MIN 的绝对值溢出）。
fn say_num(v: i64) {
    let mut buf = [0u8; 24];
    say(dec(v, &mut buf));
}
/// 十进制小缓冲：把数写进定长栈缓冲，返回其中的可见子串（零堆分配）。
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

/// 段成功标记：`e5-ds: <step> ok`。
fn step_ok(name: &str) {
    say(TAG);
    say(name);
    say(" ok\n");
}

/// 失败即终止：打印失配说明与读数，末行是给门脚本的 FAIL 标记。
/// C 的 dstest 用 `assert()` 直接 abort（dstest.c:26 等），本载体把 abort
/// 换成可读终止——真机上没有 core dump 可回看。
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

/// 失配报告：期望码与实得码都打出来（拒绝码本身就是判据，混不得）。
/// `got_ok` 把「成功回复」与「errno 0」分开写——两者在线序上同值，不分开
/// 就看不出 DS 到底是判了成功还是把错误码丢了符号。
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

/// 一条请求应回某个确定的拒绝码：实得码等于期望才算「链通、语义拒绝」。
/// `Ok(true)` 交给调用方累计失败数，让一段里的多处失配一次看全。
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

// ── 传输链判据段：无 label 调用者在 DS 上的正确落点 ──

/// 六次真任务调用。断言的是 C 的拒绝码——拒绝码本身就证明请求进了 DS、
/// 键名 ferry 读到了我们的授权内存、判定层跑了、应答回来了。
fn leg_chain(ds: &mut DsClient<DirectTrapTransport, DirectKernelCallTransport>,
             grants: &mut GrantTable) {
    // 先落一条「即将做第一次 taskcall」的标记：若串口停在这里，说明请求
    // 发出去了而 DS 没在服务（阻塞在 sendrec），与「链通但语义拒绝」可分。
    say(TAG);
    say("step chain begin (first taskcall to DS endpoint ");
    say_num(Endpoint::DS.get() as i64);
    say(")\n");

    let mut bad = 0usize;

    // 1) retrieve 不设源闸（store.c:380-398）：不存在的键 → ESRCH。
    //    这条覆盖 get_key_name 的 sys_safecopyfrom（store.c:160-172）。
    let r = ds.retrieve_u32(grants, "e5.absent.key").map(|_| ());
    if !expect_errno("retrieve absent u32", ESRCH, r) {
        bad += 1;
    }

    // 2) retrieve_str：同样的键 ferry，另带一条 WRITE 值 grant 登记
    //    （grant 生命周期 publish/retrieve 同形，键不存在时不触发拷贝）。
    let mut buf = [0u8; 32];
    let r = ds.retrieve_str(grants, "e5.absent.key", &mut buf).map(|_| ());
    if !expect_errno("retrieve absent str", ESRCH, r) {
        bad += 1;
    }

    // 3) retrieve_mem：TYPE_MEM 分档，同 ESRCH。
    let r = ds.retrieve_mem(grants, "e5.absent.key", &mut buf).map(|_| ());
    if !expect_errno("retrieve absent mem", ESRCH, r) {
        bad += 1;
    }

    // 4) publish 源闸（store.c:297-300）→ EPERM。
    let r = ds.publish_u32(grants, "e5.test_u32", 1234, DsFlags::empty());
    if !expect_errno("publish u32 nameless", EPERM, r) {
        bad += 1;
    }

    // 5) delete 源闸（store.c:588-591）→ EPERM。
    let r = ds.delete(grants, "e5.test_u32", DsFlags::TYPE_U32);
    if !expect_errno("delete nameless", EPERM, r) {
        bad += 1;
    }

    // 6) subscribe 源闸（store.c:466-469）→ ESRCH（不是 EPERM，C 的选型）。
    let r = ds.subscribe(grants, "e5.test_u32", 0);
    if !expect_errno("subscribe nameless", ESRCH, r) {
        bad += 1;
    }

    // 7) check 未知源（store.c:544-550）→ ESRCH。
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

// ── label 面读数：RS→DS 的 12 条服务 label 植入了几条 ──

/// 逐服务名解析 label 并把计数打成 `E5f-LABELS n/12`。
///
/// 解析到的（label 植入落地后的期望是 12 条全中）再做两件事：端点槽位与
/// `MODULE_NAMES` 对账、反向 `retrieve_label_name` 往返——后者是
/// `sys_safecopyto` 往我们的授权内存写值的腿，正反双向都真。解析不到的
/// 今天只记读数不判失败，因为失败点在植入半（E-RSWIRE），不在传输链。
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
                        // C ds.c:155 的纪律：末位钉 NUL，名字本体在 NUL 前。
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

// ── 通知族：本载体的边界（不做无内容的假自检）──
//
// E5(f) 的 notify 回环判据(2)（C `subs.c:20-45`：subscribe → DS 更新时
// `ipc_notify` 订阅者 → `ds_check` 取回键名）需要一个**有 label 的订阅者**：
// 订阅表按 owner 名字记账（store.c:466-469 无名即 ESRCH），而 label 只能由
// RS 发布（store.c:251-282 的 rprocpub 植入 + manager.c:787-862 的动态发布）。
// 本载体够不到那条腿，因此不在这里伪造"收到通知"的读数。

/// 目标体：两段 main 形态共用的唯一执行序列。
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

/// 装机形态入口：minix-rt 的 crt0 出生链按名解析 `main`
/// （consumer contract：名字 + Rust ABI + `-> i32` 三要素）。
#[cfg(all(not(test), target_os = "none"))]
#[unsafe(no_mangle)]
extern "Rust" fn main() -> i32 {
    run()
}

/// 宿主形态入口：std 构建走 rustc 的启动胶，`main` 必须返回
/// `Termination`（`i32` 不自带该 impl，rustc 1.94 的 E0277）。
#[cfg(any(test, not(target_os = "none")))]
fn main() {
    run()
}
