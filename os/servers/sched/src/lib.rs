#![cfg_attr(not(test), no_std)]

//! Minix-RS scheduler (SCHED): policy upstairs, mechanism downstairs.
//!
//! The user-space side of Minix3's two-layer scheduling model: the kernel
//! preempts and accounts, SCHED decides who runs next. This crate opens
//! with the startup skeleton documented in
//! `notes/rewrite/fork-syscall-rewrite/06-stage-sched/01-sched-init-main.md`:
//!
//! - [`sef`] — the two init names, the machine news, the fresh-boot token.
//! - [`dispatch`] — the five letters, the notify door, the reply rule.
//! - [`schedproc`] — the scheduling record (seven fields ride, one does not).
//! - [`table`] — the slot doors (occupied or vacant).
//! - [`valid`] — the sender names (who may knock).
//! - [`priority`] — the model behind the numbers (how high, how long).
//! - [`scheduling`] — the service arms (takeover first, 06).
//! - [`kernel_api`] — the kernel boundary (what goes down, 09).
//! - [`cpu`] — the choice behind placement (who goes where, 10).
//! - [`balancer`] — the wait behind rebalance (how long, one step up, 11).
//! - [`client`] — the client's mirror (which road, what rides, whose name, 13).

pub mod balancer;
pub mod client;
pub mod cpu;
pub mod dispatch;
pub mod kernel_api;
pub mod priority;
pub mod scheduling;
pub mod schedproc;
pub mod sef;
pub mod table;
pub mod valid;

pub use sef::*;
