//! VirtIO Drivers
//!
//! ors implements VirtIO Legacy Driver:
//! https://docs.oasis-open.org/virtio/virtio/v1.1/virtio-v1.1.pdf

pub mod block;
mod configuration;
mod queue;

pub use configuration::Configuration;
pub use queue::{Buffer, VirtQueue};
