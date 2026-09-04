//! SCHED service arms: takeover, stop, quantum-exhausted, nice.
//!
//! One arm per handler (06~08): each owns its verdicts, the caller owns
//! the table, the take-over call, and the fan-out. The arms never touch
//! each other — 06 admits births, 07 releases slots, 08 demotes and
//! regrades.

pub mod nice;
pub mod noquantum;
pub mod start;
pub mod stop;
