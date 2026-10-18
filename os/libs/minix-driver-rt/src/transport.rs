//! Transport verbs a driver needs from its environment.
//!
//! C correspondence: the primitives every `*_driver_task` loop calls —
//! `driver_receive` (`driver.h`), `send`, `asynsend3`, `sys_safecopyfrom`
//! and `sys_safecopyto` inside the transfer hooks, and the data-store
//! publication behind `chardriver_announce` (`chardriver.c:99`).
//!
//! C calls these kernel primitives directly, which is why the C drivers
//! cannot be tested off-target. The trait keeps the policy (what to do
//! with a message) behind the seam and the mechanism (which kernel call)
//! in the implementation.

use minix_types::{Endpoint, Message};

/// Message transport and grant access for one driver process.
pub trait DriverTransport {
    /// Blocking receive from any source (`driver_receive(ANY, &m)`).
    ///
    /// SEF switch point: a production implementation destined for real
    /// boots swaps this one method to `minix_sef::sef_receive_status`
    /// (RS ping absorption) when lifecycle handling lands; the loop in
    /// [`crate::runtime`] is unchanged.
    fn receive(&mut self, msg: &mut Message) -> Result<(), i32>;

    /// Blocking send (`send`).
    fn send(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32>;

    /// Send-and-receive (`ipc_sendrec`). NK4-C B9b: the RS birth-report
    /// leg uses this so the driver parks in receive(RS) for RS's catch
    /// reply (C `sef_cb_init_response_rs_reply`, sef_init.c:458-466).
    fn send_rec(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32>;

    /// Non-blocking send without a reply (`asynsend3(AMF_NOREPLY)`):
    /// character-driver replies ride this (`chardriver_reply_task`).
    fn asynsend(&mut self, dst: Endpoint, msg: &mut Message) -> Result<(), i32>;

    /// Copy bytes out of a caller's grant into `buf`
    /// (`sys_safecopyfrom`): read requests pull data this way.
    fn copy_from_grant(
        &mut self,
        granter: Endpoint,
        grant: i32,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<(), i32>;

    /// Copy bytes from `buf` into a caller's grant
    /// (`sys_safecopyto`): write requests and read replies push data
    /// this way.
    fn copy_to_grant(
        &mut self,
        granter: Endpoint,
        grant: i32,
        offset: u64,
        buf: &[u8],
    ) -> Result<(), i32>;

    /// Publish the driver's label through the data store (announce).
    ///
    /// C: `chardriver_announce` publishes `drv.chr.<name>`
    /// (`chardriver.c:99`); the block and net families publish their own
    /// prefixes. 动词是 `ds_publish_u32(key, DS_DRIVER_UP, DSF_OVERWRITE)`
    /// （chardriver.c:119）——本方名说的是键名的角色（告知“这个 label 活了”），
    /// 不是 DS 条目类型：TYPE_LABEL 只有 RS 能发（store.c:302-304）。
    /// Failure is a startup failure — a driver nobody can find
    /// must not serve.
    fn publish_label(&mut self, label: &str) -> Result<(), i32>;

    /// Copy bytes into a caller's address space (the classic
    /// `sys_datacopy` path; the readclock family's plain requests carry a
    /// buffer address instead of a grant).
    ///
    /// Additive with an honest default: a transport that only speaks
    /// grants reports `ENOSYS` rather than pretending the copy happened.
    fn copy_to_addr(&mut self, dest: Endpoint, addr: u64, buf: &[u8]) -> Result<(), i32> {
        let _ = (dest, addr, buf);
        Err(-minix_types::ENOSYS)
    }

    /// Copy bytes out of a caller's address space (`sys_datacopy`).
    fn copy_from_addr(&mut self, src: Endpoint, addr: u64, buf: &mut [u8]) -> Result<(), i32> {
        let _ = (src, addr, buf);
        Err(-minix_types::ENOSYS)
    }

    /// Resolve a published label back to its endpoint through the data
    /// store (a label lookup): an input driver finds its server this way
    /// before trusting a configuration (`do_conf` looks up `"input"` and
    /// compares, `inputdriver.c:82-111`).
    ///
    /// Additive with an honest default: a transport that cannot reach the
    /// data store reports `None`, which the caller treats as a failed
    /// lookup (C ignores the message when the lookup fails). A production
    /// implementation backs this with the data store's endpoint retrieve.
    fn lookup_label(&mut self, label: &str) -> Option<Endpoint> {
        let _ = label;
        None
    }

    /// Allocate a direct grant over this process's own memory
    /// (`cpf_grant_direct`): the i2c exec request hands the bus driver a
    /// grant on the request block.
    ///
    /// Additive with an honest default: a transport without a grant table
    /// reports `ENOSYS` rather than fabricating a grant id.
    fn grant_direct(
        &mut self,
        dest: Endpoint,
        address: u64,
        len: usize,
        write: bool,
    ) -> Result<i32, i32> {
        let _ = (dest, address, len, write);
        Err(-minix_types::ENOSYS)
    }

    /// Release a grant (`cpf_revoke`). The default refuses like
    /// [`DriverTransport::grant_direct`].
    fn grant_revoke(&mut self, grant: i32) -> Result<(), i32> {
        let _ = grant;
        Err(-minix_types::ENOSYS)
    }

    /// Wait for a hardware conversion (`micro_delay`), in microseconds.
    ///
    /// Additive with an honest default: a transport without the kernel
    /// delay call reports `ENOSYS`, so a sensor read fails instead of
    /// sampling a conversion that has not finished.
    fn delay_micros(&mut self, micros: u32) -> Result<(), i32> {
        let _ = micros;
        Err(-minix_types::ENOSYS)
    }

    /// Wall-clock seconds (`time(NULL)`; C `gettimeofday` through PM).
    ///
    /// Used by drivers whose sampling rate is throttled by the clock (the
    /// SHT21 cache samples at most once per second). Additive with an
    /// honest default: a transport without a clock reports `ENOSYS`, and
    /// the caller must not fabricate a timestamp.
    fn now_seconds(&mut self) -> Result<i64, i32> {
        Err(-minix_types::ENOSYS)
    }

    /// The label name an endpoint registered with the data store
    /// (`ds_retrieve_label_name`): the i2c bus driver keys a device
    /// reservation by the owner's label so a restarted owner can reclaim
    /// the same device.
    ///
    /// Additive with an honest default: a transport that cannot reach the
    /// data store reports `None`, which the caller treats as a failed
    /// lookup (C's `do_reserve` fails the request).
    fn label_name(&mut self, endpoint: Endpoint) -> Option<alloc::string::String> {
        let _ = endpoint;
        None
    }

    /// Send a signal to a process (`kill(pid, sig)`; C's PM round-trip):
    /// the TPS65217 power button tells init to shut down.
    ///
    /// Additive with an honest default: a transport without the signal
    /// path reports `ENOSYS` rather than pretending the signal landed.
    fn signal(&mut self, pid: i32, signal: i32) -> Result<(), i32> {
        let _ = (pid, signal);
        Err(-minix_types::ENOSYS)
    }
}
