//! VirtIO Legacy Driver

pub mod block;
mod configuration;
mod queue;

pub use configuration::Configuration;
pub use queue::{Buffer, VirtQueue};
