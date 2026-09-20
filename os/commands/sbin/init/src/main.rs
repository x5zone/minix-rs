//! Init: the first user process (PID 1, boot chain terminus).
//!
//! Entry flow (`minix3/sbin/init/init.c:229-367`): identity check, new
//! session, device probe, flag parsing, signal registration, stdio
//! cleanup, securelevel probe, then `transition()` (see 02).

// P1-2 ③（决策记录：09-stage-init/todo.md）：真机用户镜像的目标是
// `x86_64-unknown-none`（rt-birth 先例；workspace .cargo/config 为该
// target 配 static 重定位），二进制在那一侧是 freestanding PID 1 ——
// 入口走 minix-rt 的 `_start` 出生链（os/libs/minix-rt/src/crt0.rs:334，
// RBX 载 ps_strings 描述符），panic handler 由本 crate 自带（本文件底部）。
// 门控用 `target_os = "none"` 而不是 `not(test)`：宿主（linux-gnu/musl）
// 的开发构建必须保持 std 形态才可链接——rustc 对 hosted target 一律传
// Scrt1.o（其 `_start` 需要 `main` + `__libc_start_main`），no_std + no_main
// 在宿主链接必然断；freestanding 形态只在真 target 上成立。`cfg(test)`
// 下与宿主构建同为 std：测试二进制链接真 std，其 panic_impl/入口与
// minix-rt 的 no_std 专属件互斥（E0152 死结，workspace 声明 +
// dev-dependency 双声明共同解开）。
#![cfg_attr(all(not(test), target_os = "none"), no_std, no_main)]

// The containers live in `alloc` (not the std prelude) so the module
// tree is `no_std`-movable: `alloc::sync::Arc` and `alloc::collections`
// resolve identically in the std test build and in a freestanding build.
// `#[macro_use]` carries the `vec!`/`format!` macros crate-wide; the
// container TYPES (`Vec`/`String`/...) are imported per module from
// `alloc`, mirroring how `alloc::sync::Arc` is already imported.
#[macro_use]
extern crate alloc;

mod clean_ttys;
mod contracts;
mod driver;
mod entry;
mod host;
mod log;
mod multi_user;
mod password;
mod runcom;
mod session;
mod session_db;
mod signal_state;
mod shutdown;
mod single_user;
mod state_machine;
mod sysctl;
mod ttys;
mod utmp;
mod wait;

use alloc::{string::String, vec::Vec};
use alloc::string::ToString;
use driver::{DriverState, TTYS_PATH};
use entry::{InitialState, decide_entry, parse_boot_args};
use host::{InitHost, MinixSysHost};
use state_machine::StateKind;
use alloc::sync::Arc;

/// PID 1 的入口：签名与返回值形态对齐 minix-rt 出生链的消费契约
/// （crt0.rs:293 `unsafe extern "Rust" { fn main() -> i32; }`，rt-birth
/// 先例 main.rs:117）。freestanding 构建里 `run_transition` 发散
/// （`-> !`），永不返回；宿主 std 构建里它是普通入口（i32 实现
/// Termination，链接器需要它喂 Scrt1.o 的 `main`）。
fn main() {
    minix_rt::init();
    let mut host = MinixSysHost;

    // C step 1: identity gate (init.c:242-249). getuid has no client
    // wrapper yet (E-INITSYS ②) — say so once and continue; pid 1 is
    // still verified whenever getpid answers.
    match host.getuid() {
        Ok(0) => {}
        Ok(uid) => log::emergency(&mut host, &format!("init must run as root (uid {uid})")),
        Err(e) => log::warning(&mut host, &format!("identity check unavailable: {e}")),
    }
    match host.getpid() {
        Ok(1) | Err(_) => {}
        Ok(pid) => {
            log::emergency(&mut host, &format!("init already running (pid {pid})"));
            host.exit_process(1);
        }
    }

    // C step 2: session leadership (init.c:255).
    if let Err(e) = host.setsid() {
        log::warning(&mut host, &format!("setsid unavailable: {e}"));
    }

    // C step 3: device probe (init.c:269-270). ENOSYS counts as "no
    // console" and forces single-user, the same fallback as C's
    // missing console.
    let console_ok = entry::ensure_console(&mut host, single_user::CONSOLE_PATH);

    // C step 4: flag parsing (init.c:287-303). Args come from the
    // birth descriptor via minix-rt — no std env anywhere on the boot
    // path (P1-2).
    let argv: Vec<String> = (0..minix_rt::crt0::argv_count())
        .filter_map(|i| minix_rt::crt0::argv_bytes(i))
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .collect();
    let (boot_args, boot_warnings) = parse_boot_args(&argv);
    for warning in &boot_warnings {
        log::warning(&mut host, warning);
    }
    let decision = decide_entry(&boot_args, console_ok);

    // C step 5: signal registration (init.c:310-334). ENOSYS until
    // E-INITSYS ① — the machine runs with default dispositions until
    // then, which the boot log says out loud.
    if let Err(e) = host.register_handlers(&host::default_signal_spec()) {
        log::warning(&mut host, &format!("signal registration unavailable: {e}"));
    }

    // C step 6: close fds 0/1/2 (init.c:339-341).
    if let Err(e) = host.close_std_fds() {
        log::warning(&mut host, &format!("stdio cleanup unavailable: {e}"));
    }

    // C step 7: securelevel probe (init.c:353) — the level seeds the
    // single-user password gate (doc 12). C reads -1 when the kernel
    // has no level support.
    let from_securitylevel = host.securitylevel().ok().flatten().unwrap_or(-1);

    // C step 8: transition() — never returns. The password verifier is
    // filled by the password module (ARCH A-12); until then the gate
    // stays closed off, matching an empty `pw_passwd`.
    let signals = Arc::new(signal_state::SignalState::default());
    let mut state = DriverState {
        boot_args,
        mode: decision.runcom_mode,
        console_ok,
        console_secure: false,
        from_securitylevel,
        sessions: Vec::new(),
        db: crate::session_db::SessionMapDb::default(),
        signals: signals.clone(),
        sessions_seen: false,
        did_multiuser_chroot: false,
        rootdir: "/".to_string(),
        // C step 7b: read root's hash for the single-user gate
        // (getpwnam, init.c:733) — an unreadable passwd means no gate,
        // exactly the C shape when getpwnam fails.
        root_verify: host
            .read_file("/etc/passwd")
            .ok()
            .and_then(|body| password::root_password_hash(&body))
            .and_then(|hash| password::build_verifier(&hash)),
    };

    let initial = match decision.initial {
        InitialState::Runcom => StateKind::Runcom,
        InitialState::SingleUser => StateKind::SingleUser,
    };
    let _ = TTYS_PATH; // read per read_ttys step through the seam
    driver::run_transition(&mut host, &mut state, initial);
}

/// PID 1 的 panic handler（P1-2 ③ 决策记录的落地形态；门控与 crate 的
/// freestanding 形态一致，见文件顶部）。
///
/// 裁决：init 采用 minix-rt 的统一渲染——`minix_rt::diag::
/// format_panic_report`（栈上缓冲、无分配器、无系统调用，minix-rt
/// lib.rs:287 同款）——加上 init 特有的收尾：PID 1 不允许自旋挂死
/// 全机（minix-rt 自带 handler 的兜底是 spin，lib.rs:305-307，对
/// PID 1 等于把整台机器挂死），所以渲染完成后经 SYS_DIAGCTL 把报告
/// 送到内核诊断台（C 的 panic 即死语义，minix3/minix/lib/libsys/
/// panic.c:51-58 emit→exit 阶梯），然后非零 `exit`——内核与 C 世界
/// 一样看得见"init 死了"。
///
/// minix-rt 自带的 `#[panic_handler]` 在 `panic-handler` feature 后面
/// （minix-rt lib.rs:279），本 crate 不启用该 feature——图内恰好一个
/// handler，永不 E0152。宿主/std 构建下本函数整体编译出局（std 的
/// panic_impl 接管）。
#[cfg(all(not(test), target_os = "none"))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    let mut buffer = [0u8; 128];
    let length = minix_rt::diag::format_panic_report(
        info.location().map(|location| (location.file(), location.line())),
        info.message(),
        &mut buffer,
    );
    // 渲染失败不可能从这里 panic（format_panic_report 对超出缓冲的
    // 字节静默截断）；非 UTF-8 消息换成诚实占位。诊断台发送失败本身
    // 不再重试——panic 路径上的内核调用失败没有更有意义的去向，exit
    // 的可见结果（init 退出）不变。
    let message = core::str::from_utf8(&buffer[..length])
        .unwrap_or("init panicked (non-utf8 message)");
    let _ = minix_sys::syscall::sys_diagctl_write(
        &minix_sys::syscall::DirectKernelCallTransport,
        message,
    );
    minix_sys::exit(1)
}
