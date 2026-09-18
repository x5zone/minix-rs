//! Socket-driver client vocabulary: the single home for the sdev wire
//! numbers, packing helpers, error mapping, and socket event objects.
//!
//! C correspondence: `libsockdriver` — the socket-driver client library
//! parallel to `libbdev` (which this workspace mirrors as
//! `minix-bdev`). Before this crate existed the same vocabulary lived in
//! two unrelated places: the netdriver library carried the request
//! numbers and event objects, and the VFS socket layer carried a full
//! independent copy of the dialogue policy. This crate is the one
//! definition each of them now consumes (edge E-SDEVOWN).
//!
//! What belongs here is the part every consumer shares: message numbers,
//! operation and reply classification, flag packing, the error-to-errno
//! map, reply routing, and the event/hash vocabulary. What stays with a
//! consumer is whatever reaches into its own tables — the VFS dialogue
//! (`suspend_aux`, revival groups, driver-death stop plans) reads the
//! VFS fproc and smap state, so it remains there over this vocabulary.

pub mod sdev;
pub mod sockevent;
