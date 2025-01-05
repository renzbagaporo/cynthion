#![no_std]
#![allow(clippy::inline_always)]
#![allow(clippy::must_use_candidate)]

use imxrt_rt as _;

// modules
pub mod gpio;
pub mod serial;
pub mod timer;
#[cfg(feature = "usb")]
pub mod usb;

pub mod fcb;

// re-export dependencies
#[cfg(feature = "usb")]
pub use smolusb;

pub use embedded_hal as hal;
pub use embedded_hal_0 as hal_0;
pub use embedded_hal_nb as hal_nb;

pub use nb;
