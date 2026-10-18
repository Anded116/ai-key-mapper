//! Journaled cloak for HidHide 1.5.x (no session blacklist): every change we make is written
//! to a journal before it reaches the driver, and restore takes back only those entries.

pub mod journal;
pub mod ops;
pub mod pads;
pub mod procs;

pub type Result<T, E = Box<dyn std::error::Error + Send + Sync>> = std::result::Result<T, E>;
