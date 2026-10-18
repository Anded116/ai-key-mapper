//! Mapping engine: PlayStation controller state in, keyboard and mouse out.

mod buttons;
mod engine;
pub mod flick;
pub mod gyro;
pub mod keys;
pub mod output;
pub mod profile;

pub use engine::{Engine, Snapshot};
pub use keys::Output;
pub use profile::{Btn, Profile, ProfileError};
