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
    /// prefixes. Failure is a startup failure — a driver nobody can find
    /// must not serve.
    fn publish_label(&mut self, label: &str) -> Result<(), i32>;

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
}
