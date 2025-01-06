#![no_std]

pub mod clock {
    const SYSTEM_CLOCK_FREQUENCY: u32 = 500_000_000;

    #[must_use]
    pub const fn sysclk() -> u32 {
        SYSTEM_CLOCK_FREQUENCY
    }
}

pub use imxrt_ral::{interrupt, Interrupt, NVIC_PRIO_BITS};

pub struct Peripherals(pub imxrt_ral::Instances);

impl Peripherals {
    #[inline]
    pub fn take() -> Option<Self> {
        Some(unsafe { Self(imxrt_ral::Instances::instances()) })
    }
}