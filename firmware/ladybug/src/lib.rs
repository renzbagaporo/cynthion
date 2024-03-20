#![allow(dead_code, unused_imports, unused_mut, unused_variables)]
#![cfg_attr(feature = "nightly", feature(error_in_core))]
#![cfg_attr(feature = "nightly", feature(panic_info_message))]
#![cfg_attr(not(test), no_std)]

use core::cell::RefCell;
use core::marker::PhantomData;

// - public types -------------------------------------------------------------

#[derive(Clone, Copy)]
pub enum Channel {
    A,
    B,
}

pub trait LogicAnalyzer {
    fn high(&self, channel: Channel, bit_number: u8);
    fn low(&self, channel: Channel, bit_number: u8);
}

#[non_exhaustive]
pub struct Bit;

impl Bit {
    // - PMOD A --

    pub const A_GET_EVENTS: u8 = 0;
    pub const A_READ_CONTROL: u8 = 1;
    pub const A_READ_ENDPOINT: u8 = 2;
    pub const A_WRITE_ENDPOINT: u8 = 3;
    pub const A_PRIME_RECEIVE: u8 = 4;

    pub const A_PACKET_PUSH: u8 = 6;
    pub const A_PACKET_POP: u8 = 7;

    // - PMOD B --

    pub const B_EP_IS_0: u8 = 0;
    pub const B_EP_IS_1: u8 = 1;
    pub const B_IRQ_BUS_RESET: u8 = 2;
    pub const B_IRQ_EP_CONTROL: u8 = 3;
    pub const B_IRQ_EP_IN: u8 = 4;
    pub const B_IRQ_EP_OUT: u8 = 5;
}

// - public methods -----------------------------------------------------------

/// Sets the [`LogicAnalyzer`] used by ladybug.
pub fn set_analyzer(analyzer: &'static dyn LogicAnalyzer) {
    unsafe {
        LADYBUG = analyzer;
    }
}

/// Returns a reference to the logic analyzer.
#[must_use]
pub fn ladybug() -> &'static dyn LogicAnalyzer {
    unsafe { LADYBUG }
}

/// Issues a pulse on the given GPIO channel and bit number.
///
/// # Safety
///
/// This is not interrupt safe so you'll want to make sure you use
/// separate channels for tracing in your main program loop vs
/// interrupt handlers.
#[allow(clippy::inline_always)]
#[inline(always)]
pub fn trace<R>(channel: Channel, bit_number: u8, f: impl FnOnce() -> R) -> R {
    #[cfg(not(feature = "enable"))]
    {
        f()
    }
    #[cfg(feature = "enable")]
    {
        ladybug().high(channel, bit_number);
        let result = f();
        ladybug().low(channel, bit_number);
        result
    }
}

// - No-op LogicAnalyzer ------------------------------------------------------

struct LadybugDummy;
impl LogicAnalyzer for LadybugDummy {
    fn high(&self, channel: Channel, bit_number: u8) {}
    fn low(&self, channel: Channel, bit_number: u8) {}
}

// - global singleton ---------------------------------------------------------

static mut LADYBUG: &dyn LogicAnalyzer = &LadybugDummy;
