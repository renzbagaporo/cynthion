use core::marker::PhantomData;

use log::{error, info, trace, warn};

use crate::device::Descriptors;
use crate::event::UsbEvent;
use crate::setup::{Direction, Feature, Recipient, Request, RequestType, SetupPacket};
use crate::traits::UsbDriver;

// - State --------------------------------------------------------------------

/// Represents the current state of the Control interface.
#[derive(Clone, Copy, Debug)]
pub enum State {
    Idle,
    Send,
    WaitForZlp,
    SetAddress(u8),
    ReceiveHostData(SetupPacket),
    FinishHostData(SetupPacket),
    Complete,
    Stall,
}

// - Control ------------------------------------------------------------------

/// Implements a USB Control endpoint.
pub struct Control<'a, D, const RX_BUFFER_SIZE: usize> {
    endpoint_number: u8,
    descriptors: Descriptors<'a>,

    next: State,
    configuration: Option<u8>,
    feature_remote_wakeup: bool,

    rx_buffer: [u8; RX_BUFFER_SIZE],
    rx_buffer_position: usize,

    _marker: PhantomData<&'a D>,
}

impl<'a, D, const RX_BUFFER_SIZE: usize> Control<'a, D, RX_BUFFER_SIZE>
where
    D: UsbDriver,
{
    /// Returns the last received control data from the host.
    #[must_use]
    pub fn data(&'a self) -> &'a [u8] {
        &self.rx_buffer[..self.rx_buffer_position]
    }

    fn write_zlp(&self, usb: &D) {
        usb.write(self.endpoint_number, [].into_iter());
    }

    fn read_zlp(&self, usb: &D) -> bool {
        usb.read(self.endpoint_number, &mut [0; crate::EP_MAX_PACKET_SIZE]) == 0
    }
}

impl<'a, D, const RX_BUFFER_SIZE: usize> Control<'a, D, RX_BUFFER_SIZE>
where
    D: UsbDriver,
{
    #[must_use]
    pub fn new(endpoint_number: u8, descriptors: Descriptors<'a>) -> Self {
        Self {
            endpoint_number,
            descriptors: descriptors.set_total_lengths(), // TODO figure out a better solution
            next: State::Idle,
            configuration: None,
            feature_remote_wakeup: false,
            rx_buffer: [0; RX_BUFFER_SIZE],
            rx_buffer_position: 0,
            _marker: PhantomData,
        }
    }

    /// Dispatches an interrupt event generated by the USB peripheral
    /// for handling by the [`Control`] interface.
    ///
    /// Returns the last [`SetupPacket`] received if it could not be
    /// handled by the [`Control`] interface.  (e.g. if it was a
    /// [`RequestType::Class`] or [`RequestType::Vendor`] request)
    #[allow(clippy::too_many_lines)] // sometimes you can't have too much of a good thing!
    pub fn dispatch_event(&mut self, usb: &D, event: UsbEvent) -> Option<SetupPacket> {
        // The Control interface state machine operates on the latest
        // receive event and the current state of the interface.
        match (event, &self.next.clone()) {
            (UsbEvent::BusReset, _state) => {
                // reset
                self.next = State::Idle;
                // self.bus_reset(); - irq handler is doing the reset for us
            }

            (
                UsbEvent::ReceiveSetupPacket(endpoint_number, setup_packet),
                State::Idle | State::Stall,
            ) if endpoint_number == self.endpoint_number => {
                if matches!(self.next, State::Stall) {
                    // clear stall
                    warn!("TODO clearing stall");
                    self.next = State::Idle;
                }

                match (
                    setup_packet.direction(),
                    setup_packet.request_type(),
                    setup_packet.request(),
                ) {
                    // - standard requests
                    (Direction::DeviceToHost, RequestType::Standard, Request::GetDescriptor) => {
                        self.next = State::Send;
                        return self
                            .descriptors
                            .write(usb, self.endpoint_number, setup_packet);
                    }
                    (Direction::HostToDevice, RequestType::Standard, Request::SetAddress) => {
                        let address: u8 = (setup_packet.value & 0x7f) as u8;
                        self.next = State::SetAddress(address);
                        self.write_zlp(usb);
                    }
                    (Direction::HostToDevice, RequestType::Standard, Request::SetConfiguration) => {
                        let configuration: u8 = (setup_packet.value & 0xff) as u8;
                        // check whether this is a valid configuration
                        // TODO support multiple configurations
                        if configuration > 1 {
                            warn!("Control stall - unknown configuration {}", configuration);
                            self.configuration = None;
                            self.next = State::Stall;
                            usb.stall_endpoint_out(self.endpoint_number);
                            return None;
                        }
                        self.configuration = Some(configuration);
                        self.next = State::Complete;
                        self.write_zlp(usb);
                    }
                    (Direction::DeviceToHost, RequestType::Standard, Request::GetConfiguration) => {
                        self.next = State::Send;
                        if let Some(configuration) = self.configuration {
                            usb.write(self.endpoint_number, [configuration].into_iter());
                        } else {
                            usb.write(self.endpoint_number, [0].into_iter());
                        }
                    }
                    (Direction::DeviceToHost, RequestType::Standard, Request::GetStatus) => {
                        let status: u16 = 0b01; // bit 1:remote-wakeup bit 0:self-powered
                        let status = status | u16::from(self.feature_remote_wakeup) << 1;
                        self.next = State::Send;
                        usb.write(0, status.to_le_bytes().into_iter());
                    }
                    (direction, RequestType::Standard, Request::ClearFeature) => {
                        info!("  TODO Request::ClearFeature {:?}", direction);
                        let recipient = setup_packet.recipient();
                        let feature = Feature::from(setup_packet.value);
                        match (&recipient, &feature) {
                            (Recipient::Endpoint, Feature::EndpointHalt) => {
                                let endpoint_address = (setup_packet.index & 0xff) as u8;
                                usb.clear_feature_endpoint_halt(endpoint_address);
                                self.next = State::Complete;
                                self.write_zlp(usb);
                            }
                            (Recipient::Device, Feature::DeviceRemoteWakeup) => {
                                self.feature_remote_wakeup = false;
                                self.next = State::Complete;
                                self.write_zlp(usb);
                            }
                            _ => {
                                warn!(
                                    "SETUP stall: unhandled clear feature {:?}, {:?}",
                                    recipient, feature
                                );
                                self.next = State::Stall;
                                usb.stall_endpoint_in(self.endpoint_number);
                            }
                        }
                    }
                    (direction, RequestType::Standard, Request::SetFeature) => {
                        info!("  TODO Request::SetFeature {:?}", direction);
                        let recipient = setup_packet.recipient();
                        let feature = Feature::from(setup_packet.value);
                        self.next = State::Complete;
                        match (&recipient, &feature) {
                            (Recipient::Device, Feature::DeviceRemoteWakeup) => {
                                self.feature_remote_wakeup = true;
                                self.write_zlp(usb);
                            }
                            _ => {
                                warn!(
                                    "SETUP stall: unhandled set feature {:?}, {:?}",
                                    recipient, feature
                                );
                                usb.stall_endpoint_in(self.endpoint_number);
                                self.next = State::Stall;
                            }
                        }
                    }

                    // - unsupported requests with host data we need to read
                    (Direction::HostToDevice, _, _) if setup_packet.length > 0 => {
                        self.rx_buffer_position = 0;
                        self.next = State::ReceiveHostData(setup_packet);
                        usb.ep_out_prime_receive(self.endpoint_number); // prime to receive data from host
                    }

                    // - unsupported requests
                    (direction, request_type, request) => {
                        trace!(
                            "Unhandled request direction:{:?} request_type:{:?} request:{:?}",
                            direction, request_type, request
                        );
                        self.next = State::Idle;
                        return Some(setup_packet);
                    }
                }
            }

            // - handle states ------------------------------------------------
            (UsbEvent::SendComplete(endpoint_number), State::Send)
                if endpoint_number == self.endpoint_number =>
            {
                self.next = State::WaitForZlp;
                // prime to receive zlp from host
                usb.ep_out_prime_receive(self.endpoint_number);
            }

            (UsbEvent::ReceivePacket(endpoint_number), State::WaitForZlp)
                if endpoint_number == self.endpoint_number =>
            {
                if !self.read_zlp(usb) {
                    warn!(
                        "Control {:?} {:?} expected a ZLP but received data instead.",
                        event, self.next
                    );
                }
                self.next = State::Idle;
            }

            (UsbEvent::SendComplete(endpoint_number), &State::SetAddress(address))
                if endpoint_number == self.endpoint_number =>
            {
                self.next = State::Idle;
                usb.set_address(address); // set address
            }

            (UsbEvent::SendComplete(endpoint_number), State::Complete)
                if endpoint_number == self.endpoint_number =>
            {
                self.next = State::Idle;
            }

            (UsbEvent::ReceivePacket(endpoint_number), &State::ReceiveHostData(setup_packet))
                if endpoint_number == self.endpoint_number =>
            {
                let mut packet_buffer: [u8; crate::EP_MAX_PACKET_SIZE] =
                    [0; crate::EP_MAX_PACKET_SIZE];
                let bytes_read = usb.read(self.endpoint_number, &mut packet_buffer);

                // handle early abort
                if bytes_read == 0 {
                    warn!("Control receive early abort");
                    // we're done
                    self.next = State::FinishHostData(setup_packet);
                    self.write_zlp(usb);
                    return None;
                }

                // handle buffer overflow
                if self.rx_buffer_position + bytes_read > RX_BUFFER_SIZE {
                    error!("Control receive buffer overflow, truncating.");
                    // keep reading until the host has no more data to send
                    self.next = State::ReceiveHostData(setup_packet);
                    usb.ep_out_prime_receive(self.endpoint_number);
                    return None;
                }

                // append packet to rx_buffer
                let offset = self.rx_buffer_position;
                self.rx_buffer[offset..offset + bytes_read]
                    .copy_from_slice(&packet_buffer[..bytes_read]);
                self.rx_buffer_position += bytes_read;

                // are we done yet?
                if self.rx_buffer_position >= usize::from(setup_packet.length) {
                    // we're done
                    self.next = State::FinishHostData(setup_packet);
                    self.write_zlp(usb);
                } else {
                    // get ready to receive more data
                    self.next = State::ReceiveHostData(setup_packet);
                    // prime to receive next block of data from host
                    usb.ep_out_prime_receive(self.endpoint_number);
                }
            }

            (UsbEvent::SendComplete(endpoint_number), &State::FinishHostData(setup_packet))
                if endpoint_number == self.endpoint_number =>
            {
                // we've sent our zlp and now we are done
                self.next = State::Idle;

                // check for length mismatch
                if self.rx_buffer_position != usize::from(setup_packet.length) {
                    warn!(
                        "Control expected {} bytes of data from the host, but received {} bytes.",
                        setup_packet.length, self.rx_buffer_position,
                    );
                }

                return Some(setup_packet);
            }

            // we'll get these if someone is writing directly to usb1 outside control
            (UsbEvent::ReceivePacket(endpoint_number), State::Idle)
                if endpoint_number == self.endpoint_number =>
            {
                if !self.read_zlp(usb) {
                    warn!("Control expected a ZLP but received data instead.");
                }
            }
            (UsbEvent::SendComplete(endpoint_number), State::Idle)
                if endpoint_number == self.endpoint_number =>
            {
                // do nothing
            }

            (event, state) => {
                self.next = State::Idle;
                error!(
                    "Control state error. Received event '{:?}' while in state '{:?}'.",
                    event, state
                );
            }
        }

        None
    }
}
