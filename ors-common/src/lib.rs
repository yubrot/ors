#![feature(maybe_uninit_uninit_array)] // for non_contiguous
#![feature(maybe_uninit_array_assume_init)] // for non_contiguous
#![no_std]

#[cfg(test)]
extern crate alloc;

pub mod frame_buffer;
pub mod memory_map;
pub mod non_contiguous;
