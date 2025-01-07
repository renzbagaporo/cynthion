//! smolusb hal implementation for luna eptri peripherals

/// Re-export smolusb error type
pub use smolusb::error::ErrorKind as Error;

/*use smolusb::device::Speed;
use smolusb::setup::Direction;
use smolusb::traits::{
    ReadControl, ReadEndpoint, UnsafeUsbDriverOperations, UsbDriver, UsbDriverOperations,
    WriteEndpoint,
};

use crate::pac;
use pac::interrupt::Interrupt;*/

/// Default timeout for USB operations
pub const DEFAULT_TIMEOUT: usize = 1_000_000;

/// Macro to generate smolusb hal wrappers for `pac::USBx` peripherals
///
/// For example:
///
///     impl_usb! {
///         Usb0: USB0, USB0_EP_CONTROL, USB0_EP_IN, USB0_EP_OUT,
///         Usb1: USB1, USB1_EP_CONTROL, USB1_EP_IN, USB1_EP_OUT,
///     }
///
#[macro_export]
macro_rules! impl_usb {
    ($(
        $USBX:ident: $IDX:ident, $USBX_CONTROLLER:ty, $USBX_EP_CONTROL:ty, $USBX_EP_IN:ty, $USBX_EP_OUT:ty,
    )+) => {
        $(
            pub struct $USBX {
                pub controller: $USBX_CONTROLLER,
                pub ep_control: $USBX_EP_CONTROL,
                pub ep_in: $USBX_EP_IN,
                pub ep_out: $USBX_EP_OUT,
                pub device_speed: Speed,
            }

            impl $USBX {
                /// Create a new `Usb` instance.
                pub fn new(
                    controller: $USBX_CONTROLLER,
                    ep_control: $USBX_EP_CONTROL,
                    ep_in: $USBX_EP_IN,
                    ep_out: $USBX_EP_OUT,
                ) -> Self {
                    Self {
                        controller,
                        ep_control,
                        ep_in,
                        ep_out,
                        device_speed: Speed::Unknown,
                    }
                }

                /// Release all peripherals and consume self.
                pub fn free(
                    self,
                ) -> (
                    $USBX_CONTROLLER,
                    $USBX_EP_CONTROL,
                    $USBX_EP_IN,
                    $USBX_EP_OUT,
                ) {
                    (self.controller, self.ep_control, self.ep_in, self.ep_out)
                }

                /// Obtain a static [`Usb0`] instance for use in e.g. interrupt handlers
                ///
                /// # Safety
                ///
                /// 'Tis thine responsibility, that which thou doth summon.
                #[inline(always)]
                pub unsafe fn summon() -> Self {
                    Self {
                        controller: <$USBX_CONTROLLER>::steal(),
                        ep_control: <$USBX_EP_CONTROL>::steal(),
                        ep_in: <$USBX_EP_IN>::steal(),
                        ep_out: <$USBX_EP_OUT>::steal(),
                        device_speed: Speed::Unknown,
                    }
                }
            }

            impl $USBX {
                /// Enable all device interrupt events.
                pub fn enable_events(&self) {
                }

                /// Disable all device interrupt events.
                pub fn disable_events(&self) {
                }

                /// Returns the address of the control endpoint.
                #[must_use]
                pub fn ep_control_address(&self) -> u8 {
                    0
                }
            }

            // - trait: UsbDriverOperations -----------------------------------

            impl UsbDriverOperations for $USBX {
                /// Connect the device.
                fn connect(&mut self, device_speed: Speed) {
                }

                /// Disconnect the device.
                fn disconnect(&mut self) {
                }

                /// Perform a bus reset of the device.
                fn bus_reset(&self) {
                }

                /// Acknowledge the status stage of an incoming control request.
                fn ack(&self, endpoint_number: u8, direction: Direction) {
                }

                /// Set the device address.
                fn set_address(&self, address: u8) {
                }

                /// Stall the given IN endpoint number.
                fn stall_endpoint_in(&self, endpoint_number: u8) {
                }

                /// Stall the given OUT endpoint number.
                fn stall_endpoint_out(&self, endpoint_number: u8) {
                }

                /// Clear the PID toggle bit for the given endpoint address.
                ///
                /// TODO this works most of the time, but not always ...
                ///
                /// Also see: <https://github.com/greatscottgadgets/luna/issues/166>
                fn clear_feature_endpoint_halt(&self, endpoint_number: u8, direction: Direction) {
                }
            }

            // - trait: UnsafeUsbDriverOperations -----------------------------

            #[allow(non_snake_case)]
            mod $IDX {
                use lunasoc_hal::smolusb::EP_MAX_ENDPOINTS;

                #[cfg(target_has_atomic)]
                #[allow(clippy::declare_interior_mutable_const)]
                const ATOMIC_FALSE: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

                #[cfg(not(target_has_atomic))]
                pub static mut TX_ACK_ACTIVE: [bool; EP_MAX_ENDPOINTS] = [false; EP_MAX_ENDPOINTS];
                #[cfg(target_has_atomic)]
                pub static TX_ACK_ACTIVE: [core::sync::atomic::AtomicBool; EP_MAX_ENDPOINTS] =
                    [ATOMIC_FALSE; EP_MAX_ENDPOINTS];

            }

            impl UnsafeUsbDriverOperations for $USBX {
                #[inline(always)]
                unsafe fn set_tx_ack_active(&self, endpoint_number: u8) {
                }
                #[inline(always)]
                unsafe fn clear_tx_ack_active(&self, endpoint_number: u8) {
                }
                #[inline(always)]
                unsafe fn is_tx_ack_active(&self, endpoint_number: u8) -> bool {
                    false
                }
            }

            // - trait: Read/Write traits -------------------------------------

            impl ReadControl for $USBX {
                /// Read a setup packet from the control endpoint.
                fn read_control(&self, buffer: &mut [u8]) -> usize {
                    0
                }
            }

            impl ReadEndpoint for $USBX {
                /// Prepare OUT endpoint to receive a single packet.
                #[inline(always)]
                fn ep_out_prime_receive(&self, endpoint_number: u8) {
                }

                #[inline(always)]
                fn read(&self, endpoint_number: u8, buffer: &mut [u8]) -> usize {
                    0
                }
            }

            impl WriteEndpoint for $USBX {
                fn write<'a, I>(&self, endpoint_number: u8, iter: I) -> usize
                where
                    I: Iterator<Item = u8>
                {
                    0
                }

                fn write_requested<'a, I>(&self, endpoint_number: u8, requested_length: usize, iter: I) -> usize
                where
                    I: Iterator<Item = u8>
                {
                    0
                }

                fn write_with_packet_size<'a, I>(&self, endpoint_number: u8, requested_length: Option<usize>, iter: I, packet_size: usize) -> usize
                where
                    I: Iterator<Item = u8>
                {
                    0
                }

            }

            // mark implementation as complete
            impl UsbDriver for $USBX {}
        )+
    }
}
