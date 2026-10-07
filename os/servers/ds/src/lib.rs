#![cfg_attr(not(test), no_std)]

//! Minix-RS data store (DS): the registry others read to survive.
//!
//! The user-space side of Minix3's publish/subscribe registry: publishers
//! store state here, RS restarts the crashed from what survives. This crate
//! opens with the startup skeleton documented in
//! `rewrite-notes/07-stage-ds/01-ds-init-main.md`:
//!
//! - [`sef`] — the two init names plus the transfer hook (who boots how).
//! - [`dispatch`] — the seven letters, the two refusals, the reply rule.
//!
//! The data model lives beside it, documented in
//! `rewrite-notes/07-stage-ds/03-ds-data-structures.md`:
//!
//! - [`store`] — entries and the entry table (what survives, in what shape).
//! - [`subscription`] — subscribers and the told-map (who is told what).
//!
//! Slot motion lives beside it, documented in
//! `rewrite-notes/07-stage-ds/04-ds-slot-management.md`:
//!
//! - [`slots`] — taking, releasing, and finding seats (motion without shape).
//!
//! Identity and judgement live beside them, documented in
//! `rewrite-notes/07-stage-ds/05-ds-identity-auth.md`:
//!
//! - [`identity`] — endpoint↔name translation (who is who, lent not copied).
//! - [`auth`] — the permission verdict (gates as selective doors).
//!
//! Boot shadowing lives beside it, documented in
//! `rewrite-notes/07-stage-ds/06-ds-boot-mapping.md`:
//!
//! - [`boot`] — the fresh anchor: reset, shadow, batch (motion without transport).
//!
//! Handler verdicts live beside it, documented in
//! `rewrite-notes/07-stage-ds/07-ds-publish.md`:
//!
//! - [`publish`] — the publish verdict: six refusals, two ways to land
//!   (decision without commit; heap/transport/notify stand beyond).
//!
//! Read-back verdicts live beside them, documented in
//! `rewrite-notes/07-stage-ds/08-ds-retrieve.md`
//! and `…/09-ds-delete.md`:
//!
//! - [`retrieve`] — the retrieve verdict: bounds, lookup, gate, length.
//! - [`delete`] — the delete verdict plus the table edit, including the
//!   label cascade (heap buffers handed back, A-3).
//!
//! Subscription life lives beside them, documented in
//! `rewrite-notes/07-stage-ds/10-ds-subscribe-check.md`:
//!
//! - [`subscribe`] — the subscribe verdict and seat write, matched by the
//!   anchored ERE engine ([ARCH A-2] — C's `regcomp("^…$")` + `regexec`).
//! - [`pattern`] — the match engine: anchored POSIX ERE, full match,
//!   no allocation.
//! - [`notify`] — the sweep: whom a change wakes, and whom it skips.
//! - [`check`] — the check verdict: oldest pending update first.
//!
//! Image lending and the caller side live beside them, documented in
//! `rewrite-notes/07-stage-ds/11-ds-getsysinfo.md`
//! and `…/12-ds-client-library.md`:
//!
//! - [`getsysinfo`] — the image verdict: which query, how many bytes.
//! - [`client`] — grant sizing, NUL discipline, flag assembly.
//!
//! The loop and its seams live beside them, documented in
//! `rewrite-notes/07-stage-ds/01-ds-init-main.md`:
//!
//! - [`heap`] — the STR/MEM buffer pool (A-3: fixed slots, ENOMEM at
//!   exhaustion, descriptors handed back on delete).
//! - [`server`] — the assembly: receive → triage → seven arms → reply,
//!   over the [`server::DsIpc`] and [`server::DsKernel`] seams.

pub mod auth;
pub mod boot;
pub mod check;
pub mod client;
pub mod delete;
pub mod dispatch;
pub mod getsysinfo;
pub mod heap;
pub mod identity;
pub mod notify;
pub mod pattern;
pub mod publish;
pub mod retrieve;
pub mod sef;
pub mod server;
pub mod slots;
pub mod store;
pub mod subscribe;
pub mod subscription;
