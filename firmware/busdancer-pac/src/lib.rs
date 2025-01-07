#![no_std]
pub mod clock {
    const SYSTEM_CLOCK_FREQUENCY: u32 = 500_000_000;

    #[must_use]
    pub const fn sysclk() -> u32 {
        SYSTEM_CLOCK_FREQUENCY
    }
}

use imxrt_ral::Instances;
pub use imxrt_ral::{NVIC_PRIO_BITS};
pub struct UART(pub imxrt_ral::lpuart::LPUART2);

impl UART {
    #[inline]
    pub const unsafe fn steal() -> Self {
        Self(imxrt_ral::lpuart::LPUART2::instance())
    }
}

impl core::fmt::Debug for UART {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        f.debug_struct("UART").finish()
    }
}
pub struct USB2(pub imxrt_ral::lpuart::LPUART1);

impl USB2 {
    #[inline]
    pub const unsafe fn steal() -> Self {
        Self(imxrt_ral::lpuart::LPUART1::instance())
    }
}

#[allow(non_camel_case_types)]
pub struct USB2_EP_CONTROL;

impl USB2_EP_CONTROL {
    #[inline]
    pub const unsafe fn steal() -> Self {
        Self
    }
}

#[allow(non_camel_case_types)]
pub struct USB2_EP_IN;

impl USB2_EP_IN {
    #[inline]
    pub const unsafe fn steal() -> Self {
        Self
    }
}

#[allow(non_camel_case_types)]
pub struct USB2_EP_OUT;

impl USB2_EP_OUT {
    #[inline]
    pub const unsafe fn steal() -> Self {
        Self
    }
}
pub struct Peripherals
{
    peripherals : imxrt_ral::Instances,
    pub USB2 : USB2,
    pub USB2_EP_IN : USB2_EP_IN,
    pub USB2_EP_CONTROL: USB2_EP_CONTROL,
    pub USB2_EP_OUT : USB2_EP_OUT,
}

impl Peripherals {
    #[inline]
    pub fn take() -> Option<Self> {
        Some(unsafe { Self {
            peripherals : Instances::instances(),
            USB2 : USB2::steal(),
            USB2_EP_IN : USB2_EP_IN::steal(),
            USB2_EP_OUT : USB2_EP_OUT::steal(),
            USB2_EP_CONTROL : USB2_EP_CONTROL::steal()
        }  })
    }
}


#[derive(Debug, Copy, Clone)]
#[repr(u16)]
pub enum Interrupt {
    USB2 = 0,
    USB2_EP_CONTROL = 1,
    USB2_EP_IN = 2,
    USB2_EP_OUT = 3,
}

pub mod csr;