#![no_std]
#![allow(clippy::inline_always)]
#![allow(clippy::must_use_candidate)]

// modules
#[cfg(feature = "usb")]
pub mod usb;

// re-export dependencies
#[cfg(feature = "usb")]
pub use smolusb;

pub use nb;
