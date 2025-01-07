#![no_std]
#![no_main]

use heapless::mpmc::MpMcQueue as Queue;
use log::{debug, error, info, trace, warn};

use crate::hal::smolusb;
use smolusb::control::Control;
use smolusb::descriptor::StringDescriptor;
use smolusb::device::{Descriptors, Speed};
use smolusb::setup::{Direction, Recipient, RequestType, SetupPacket};
use smolusb::traits::{ReadEndpoint, UsbDriverOperations, WriteEndpoint};

use libgreat::gcp::{GreatDispatch, GreatResponse, LIBGREAT_MAX_COMMAND_SIZE};
use libgreat::{GreatError, GreatResult};

use moondancer::event::InterruptEvent;
use moondancer::usb::vendor::{VendorRequest, VendorValue};
use moondancer::{hal, pac, util};

#[cfg(feature = "cynthion_hw")]
use pac::csr::interrupt;

// - configuration ------------------------------------------------------------

const DEVICE_SPEED: Speed = Speed::High;

// - MachineExternal interrupt handler ----------------------------------------

static EVENT_QUEUE: Queue<InterruptEvent, 64> = Queue::new();

#[inline(always)]
fn dispatch_event(event: InterruptEvent) {
    match EVENT_QUEUE.enqueue(event) {
        Ok(()) => (),
        Err(_) => {
            error!("MachineExternal - event queue overflow");
            while let Some(interrupt_event) = EVENT_QUEUE.dequeue() {
                error!("{:?}", interrupt_event);
            }
            loop {
                unsafe {
                    riscv::asm::nop();
                }
            }
        }
    }
}

#[allow(non_snake_case)]
#[no_mangle]
extern "C" fn MachineExternal() {
    #[cfg(feature = "cynthion_hw")]
    let event = moondancer::util::get_usb_interrupt_event();
    #[cfg(feature = "cynthion_hw")]
    dispatch_event(event);
}

// - main entry point ---------------------------------------------------------

#[cfg(feature = "vexriscv")]
#[riscv_rt::pre_init]
unsafe fn pre_main() {
    pac::cpu::vexriscv::flush_icache();
    #[cfg(feature = "vexriscv_dcache")]
    pac::cpu::vexriscv::flush_dcache();
}

use imxrt_ral as ral;

const LED_OFFSET: u32 = 11;
const LED: u32 = 1 << LED_OFFSET;

/// Microseconds, given the clock selection and configuration
/// for the timer.
const PIT_PERIOD_US: u32 = 500_000;

#[riscv_rt::entry]
fn main() -> ! {
    let iomuxc = unsafe { ral::iomuxc::IOMUXC::instance() };
    // Set the GPIO pad to a GPIO function (ALT 5)
    ral::write_reg!(ral::iomuxc, iomuxc, SW_MUX_CTL_PAD_GPIO_11, 5);
    // Increase drive strength, but leave other fields at their current value...
    ral::modify_reg!(
        ral::iomuxc,
        iomuxc,
        SW_PAD_CTL_PAD_GPIO_11,
        DSE: DSE_7_R0_7
    );

    let gpio2 = unsafe { ral::gpio::GPIO1::instance() };
    // Set GPIO2[3] to an output
    ral::modify_reg!(ral::gpio, gpio2, GDIR, |gdir| gdir | LED);

    let ccm = unsafe { ral::ccm::CCM::instance() };
    // Disable the PIT clock gate while we change the clock...
    ral::modify_reg!(ral::ccm, ccm, CCGR1, CG6: 0b00);
    // Set the periodic clock divider, selection.
    // 24MHz crystal oscillator, divided by 24 == 1MHz PIT clock
    ral::modify_reg!(
        ral::ccm,
        ccm,
        CSCMR1,
        PERCLK_PODF: DIVIDE_24,
        PERCLK_CLK_SEL: PERCLK_CLK_SEL_1 // Oscillator clock
    );
    // Re-enable PIT clock
    ral::modify_reg!(ral::ccm, ccm, CCGR1, CG6: 0b11);

    let pit = unsafe { ral::pit::PIT::instance() };
    // Disable the PIT, just in case it was used by the boot ROM
    ral::write_reg!(ral::pit, pit, MCR, MDIS: MDIS_1);
    // Reset channel 0 control; we'll use channel 0 for our timer
    ral::write_reg!(ral::pit::timer, &pit.TIMER[0], TCTRL, 0);
    // Set the counter value
    ral::write_reg!(ral::pit::timer, &pit.TIMER[0], LDVAL, PIT_PERIOD_US);
    // Enable the PIT timer
    ral::modify_reg!(ral::pit, pit, MCR, MDIS: MDIS_0);

    // initialize firmware
    let mut firmware = Firmware::new(pac::Peripherals::take().unwrap());
    match firmware.initialize() {
        Ok(()) => (),
        Err(e) => {
            panic!("Firmware panicked during initialization: {}", e)
        }
    }

    let mut on = false;
    loop {
        on = !on;
        if on {
            ral::write_reg!(ral::gpio, gpio2, DR_SET, LED);
        } else {
            ral::write_reg!(ral::gpio, gpio2, DR_CLEAR, LED);
        }

        // Start counting!
        ral::write_reg!(ral::pit::timer, &pit.TIMER[0], TCTRL, TEN: 1);
        // Are we done?
        while ral::read_reg!(ral::pit::timer, &pit.TIMER[0], TFLG, TIF == 0) {}
        // We're done; clear the flag
        ral::write_reg!(ral::pit::timer, &pit.TIMER[0], TFLG, TIF: 1);
        // Turn off the timer
        ral::write_reg!(ral::pit::timer, &pit.TIMER[0], TCTRL, TEN: 0);
    }

    // enter main loop
    let e = firmware.main_loop();

    // panic!
    panic!("Firmware exited unexpectedly in main loop: {:?}", e)
}

// - Firmware -----------------------------------------------------------------

struct Firmware<'a> {
    // peripherals
    #[cfg(feature = "cynthion_hw")]
    leds: pac::LEDS,

    usb2: hal::Usb2,

    // usb2 control endpoint
    usb2_control: Control<'a, hal::Usb2, LIBGREAT_MAX_COMMAND_SIZE>,

    // state
    libgreat_response: Option<GreatResponse>,
    libgreat_response_last_error: Option<GreatError>,

    // classes
    core: libgreat::gcp::class_core::Core,
    moondancer: moondancer::gcp::moondancer::Moondancer,

    pub _marker: core::marker::PhantomData<&'a ()>,
}

// - lifecycle ----------------------------------------------------------------

impl<'a> Firmware<'a> {
    fn new(peripherals: pac::Peripherals) -> Self {
        // initialize libgreat class registry
        static CLASSES: [libgreat::gcp::Class; 4] = [
            libgreat::gcp::class_core::CLASS,
            moondancer::gcp::firmware::CLASS,
            moondancer::gcp::selftest::CLASS,
            moondancer::gcp::moondancer::CLASS,
        ];
        let classes = libgreat::gcp::Classes(&CLASSES);

        // enable ApolloAdvertiser to disconnect the Cynthion USB2 control port from Apollo
        #[cfg(feature = "cynthion_hw")]
        let advertiser = peripherals.ADVERTISER;
        #[cfg(feature = "cynthion_hw")]
        advertiser.enable().write(|w| w.enable().bit(true));

        // get Cynthion hardware revision information from the SoC
        let board_major = 0;
        let board_minor = 0;

        #[cfg(feature = "cynthion_hw")]
        let info = &peripherals.INFO;
        #[cfg(feature = "cynthion_hw")]
        let board_major = info.version_major().read().bits() as u8;
        #[cfg(feature = "cynthion_hw")]
        let board_minor = info.version_minor().read().bits() as u8;

        // initialize logging
        moondancer::log::set_port(moondancer::log::Port::Both);
        moondancer::log::init();
        info!(
            "{} {} v{}",
            env!("CARGO_PKG_AUTHORS"),
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
        );
        info!("Logging initialized");

        // initialize ladybug
        #[cfg(feature = "cynthion_hw")]
        moondancer::debug::init(peripherals.GPIOA, peripherals.GPIOB);
        

        // get Cynthion SPI Flash uuid from the SoC
        let uuid = [0, 0, 0, 0, 0, 0, 0, 0];
        #[cfg(feature = "cynthion_hw")]
        let uuid = util::read_flash_uuid(&peripherals.SPI0).unwrap_or([0_u8; 8]);

        let uuid = util::format_flash_uuid(uuid);

        // build string descriptor table
        //
        // FIXME crimes should not have to be committed in order to provide smolusb with a dynamic string table!
        #[allow(clippy::items_after_statements)]
        let string_descriptors = {
            static mut UUID: heapless::String<16> = heapless::String::new();
            static mut ISERIALNUMBER: StringDescriptor = StringDescriptor::new("0000000000000000");
            unsafe {
                UUID = uuid.clone();
                ISERIALNUMBER = StringDescriptor::new(UUID.as_str());
            }
            static mut STRING_DESCRIPTORS: [&StringDescriptor; 9] = [
                &moondancer::usb::STRING_DESCRIPTOR_1,
                &moondancer::usb::STRING_DESCRIPTOR_2,
                unsafe { &*core::ptr::addr_of!(ISERIALNUMBER) },
                &moondancer::usb::STRING_DESCRIPTOR_4,
                &moondancer::usb::STRING_DESCRIPTOR_5,
                &moondancer::usb::STRING_DESCRIPTOR_6,
                &moondancer::usb::STRING_DESCRIPTOR_7,
                &moondancer::usb::STRING_DESCRIPTOR_8,
                &moondancer::usb::STRING_DESCRIPTOR_9,
            ];
            unsafe { &*core::ptr::addr_of!(STRING_DESCRIPTORS) }
        };

        // usb2: control (host on r0.4)
        let usb2 = hal::Usb2::new(
            peripherals.USB2,
            peripherals.USB2_EP_CONTROL,
            peripherals.USB2_EP_IN,
            peripherals.USB2_EP_OUT,
        );

        // usb0: target
        #[cfg(feature = "cynthion_hw")]
        let usb0 = hal::Usb0::new(
            peripherals.USB0,
            peripherals.USB0_EP_CONTROL,
            peripherals.USB0_EP_IN,
            peripherals.USB0_EP_OUT,
        );

        // format bcdDevice
        let bcd_device: u16 = u16::from_be_bytes([board_major, board_minor]);

        let usb2_control = Control::<_, LIBGREAT_MAX_COMMAND_SIZE>::new(
            0,
            Descriptors {
                // required
                device_speed: DEVICE_SPEED,
                device_descriptor: smolusb::descriptor::DeviceDescriptor {
                    bcdDevice: bcd_device,
                    ..moondancer::usb::DEVICE_DESCRIPTOR
                },
                configuration_descriptor: moondancer::usb::CONFIGURATION_DESCRIPTOR_0,
                string_descriptor_zero: moondancer::usb::STRING_DESCRIPTOR_0,
                string_descriptors,
                // optional
                device_qualifier_descriptor: Some(moondancer::usb::DEVICE_QUALIFIER_DESCRIPTOR),
                other_speed_configuration_descriptor: Some(
                    moondancer::usb::OTHER_SPEED_CONFIGURATION_DESCRIPTOR_0,
                ),
                microsoft10: Some(smolusb::descriptor::microsoft10::Descriptors {
                    string_descriptor: moondancer::usb::STRING_DESCRIPTOR_0XEE,
                    compat_id_feature_descriptor:
                        moondancer::usb::MS_OS_10_COMPATIBLE_ID_FEATURE_DESCRIPTOR,
                    extended_properties_feature_descriptor:
                        moondancer::usb::MS_OS_10_EXTENDED_PROPERTIES_FEATURE_DESCRIPTOR,
                }),
            },
        );

        // initialize libgreat classes
        let core = libgreat::gcp::class_core::Core::new(classes, moondancer::BOARD_INFORMATION);

        let moondancer = moondancer::gcp::moondancer::Moondancer::new(
            #[cfg(feature = "cynthion_hw")]
            usb0
        );

        Self {
            #[cfg(feature = "cynthion_hw")]
            leds: peripherals.LEDS,
            usb2,
            usb2_control,
            libgreat_response: None,
            libgreat_response_last_error: None,
            core,
            moondancer,
            _marker: core::marker::PhantomData,
        }
    }

    fn initialize(&mut self) -> GreatResult<()> {
        // leds: starting up
        #[cfg(feature = "cynthion_hw")]
        self.leds
            .output()
            .write(|w| unsafe { w.output().bits(1 << 2) });

        // connect usb2
        self.usb2.connect(DEVICE_SPEED);
        info!("Connected usb2 device");

        // enable interrupts
        unsafe {
            // set mstatus register: interrupt enable
            riscv::interrupt::enable();

            // set mie register: machine external interrupts enable
            #[cfg(feature = "cynthion_hw")]
            riscv::register::mie::set_mext();

            // write csr: enable usb2 interrupts

            #[cfg(feature = "cynthion_hw")]
            interrupt::enable(pac::Interrupt::USB2);
            #[cfg(feature = "cynthion_hw")]
            interrupt::enable(pac::Interrupt::USB2_EP_CONTROL);
            #[cfg(feature = "cynthion_hw")]
            interrupt::enable(pac::Interrupt::USB2_EP_IN);
            #[cfg(feature = "cynthion_hw")]
            interrupt::enable(pac::Interrupt::USB2_EP_OUT);

            // enable usb2 interrupt events
            self.usb2.enable_events();
        }

        Ok(())
    }
}

// - main loop ----------------------------------------------------------------

impl<'a> Firmware<'a> {
    #[inline(always)]
    fn main_loop(&'a mut self) -> GreatResult<()> {
        let mut max_queue_length: usize = 0;
        let mut queue_length: usize = 0;
        let mut counter: usize = 1;

        info!("Peripherals initialized, entering main loop");

        loop {
            // leds: main loop is responsive, interrupts are firing
            #[cfg(feature = "cynthion_hw")]
            self.leds
                .output()
                .write(|w| unsafe { w.output().bits((counter % 0xff) as u8) });

            if queue_length > max_queue_length {
                max_queue_length = queue_length;
                debug!("max_queue_length: {}", max_queue_length);
            }
            queue_length = 0;

            while let Some(interrupt_event) = EVENT_QUEUE.dequeue() {
                use moondancer::{
                    event::InterruptEvent::*,
                    UsbInterface::{Control, Target},
                };
                use smolusb::event::UsbEvent::*;

                counter += 1;
                queue_length += 1;

                // leds: event loop is active
                #[cfg(feature = "cynthion_hw")]
                self.leds
                    .output()
                    .write(|w| unsafe { w.output().bits(1 << 0) });

                match interrupt_event {
                    // - misc event handlers --
                    ErrorMessage(message) => {
                        error!("MachineExternal Error: {}", message);
                    }

                    // - usb2 Control event handlers --

                    // Usb2 received a control event
                    Usb(
                        Control,
                        event @ (BusReset
                        | ReceiveControl(0)
                        | ReceiveSetupPacket(0, _)
                        | ReceivePacket(0)
                        | SendComplete(0)),
                    ) => {
                        trace!("Usb(Control, {:?})", event);
                        if let Some(setup_packet) =
                            self.usb2_control.dispatch_event(&self.usb2, event)
                        {
                            // vendor requests are not handled by control
                            self.handle_vendor_request(setup_packet)?;
                        }
                    }

                    // - usb0 Target event handlers --

                    // enqueue moondancer events
                    Usb(Target, usb_event) => self.moondancer.dispatch_event(usb_event),

                    // Unhandled event
                    _ => {
                        error!("Unhandled event: {:?}", interrupt_event);
                    }
                }
            }
        }
    }
}

// - usb2 control handler -----------------------------------------------------

impl<'a> Firmware<'a> {
    /// Handle GCP vendor requests
    fn handle_vendor_request(&mut self, setup_packet: SetupPacket) -> GreatResult<()> {
        let direction = setup_packet.direction();
        let request_type = setup_packet.request_type();
        let recipient = setup_packet.recipient();
        let vendor_request = VendorRequest::from(setup_packet.request);
        let vendor_value = VendorValue::from(setup_packet.value);

        log::debug!(
            "handle_vendor_request: {:?} {:?} {:?} {:?} {:?}",
            request_type,
            recipient,
            direction,
            vendor_request,
            vendor_value
        );

        match (&request_type, &recipient, &vendor_request) {
            // handle apollo stub interface requests
            (RequestType::Vendor, Recipient::Interface, VendorRequest::ApolloClaimInterface) => {
                // send zlp
                #[cfg(feature = "cynthion_hw")]
                self.usb2.write(0, [].into_iter());

                // allow apollo to claim Cynthion's control port
                info!("Releasing Cynthion USB Control Port and activating Apollo");
                #[cfg(feature = "cynthion_hw")]
                let advertiser = unsafe { pac::ADVERTISER::steal() };
                #[cfg(feature = "cynthion_hw")]
                advertiser.enable().write(|w| w.enable().bit(false));
            }

            // handle moondancer control requests
            (RequestType::Vendor, _, VendorRequest::UsbCommandRequest) => {
                match (&vendor_value, &direction) {
                    // host is starting a new command sequence
                    #[cfg(feature = "cynthion_hw")]
                    (VendorValue::Execute, Direction::HostToDevice) => {
                        trace!("  GOT COMMAND data:{:?}", self.usb2_control.data());
                        self.dispatch_libgreat_request()?;
                    }

                    // host is ready to receive a response
                    (VendorValue::Execute, Direction::DeviceToHost) => {
                        trace!("  GOT RESPONSE REQUEST");
                        self.dispatch_libgreat_response(setup_packet)?;
                    }

                    // host would like to abort the current command sequence
                    (VendorValue::Cancel, Direction::DeviceToHost) => {
                        debug!("  GOT ABORT");
                        self.dispatch_libgreat_abort(setup_packet)?;
                    }

                    _ => {
                        error!(
                            "handle_vendor_request stall: unknown vendor request and/or value direction{:?} vendor_request{:?} vendor_value:{:?}",
                            direction, vendor_request, vendor_value
                        );
                        #[cfg(feature = "cynthion_hw")]
                        match direction {
                            Direction::HostToDevice => self.usb2.stall_endpoint_out(0),
                            Direction::DeviceToHost => self.usb2.stall_endpoint_in(0),
                        }
                    }
                }
            }
            (RequestType::Vendor, _, VendorRequest::Unknown(vendor_request)) => {
                error!(
                    "handle_vendor_request Unknown vendor request '{}'",
                    vendor_request
                );
                #[cfg(feature = "cynthion_hw")]
                match direction {
                    Direction::HostToDevice => self.usb2.stall_endpoint_out(0),
                    Direction::DeviceToHost => self.usb2.stall_endpoint_in(0),
                }
            }
            (RequestType::Vendor, _, _vendor_request) => {
                // TODO this is from one of the legacy boards which we
                // need to support to get `greatfet info` to finish
                // enumerating through the supported devices.
                //
                // see: host/greatfet/boards/legacy.py

                // The greatfet board scan code expects the IN endpoint
                // to be stalled if this is not a legacy device.
                #[cfg(feature = "cynthion_hw")]
                self.usb2.stall_endpoint_in(0);

                warn!("handle_vendor_request Legacy libgreat vendor request");
            }
            _ => {
                error!(
                    "handle_vendor_request Unknown vendor request: '{:?}'",
                    setup_packet
                );
                #[cfg(feature = "cynthion_hw")]
                match direction {
                    Direction::HostToDevice => self.usb2.stall_endpoint_out(0),
                    Direction::DeviceToHost => self.usb2.stall_endpoint_in(0),
                }
            }
        }

        Ok(())
    }
}

// - libgreat command dispatch ------------------------------------------------

impl<'a> Firmware<'a> {
    fn dispatch_libgreat_request(&mut self) -> GreatResult<()> {
        
        // let command_buffer:[u8] = [0, 1, 2, 3];
        // #[cfg(feature = "cynthion_hw")]
        // let command_buffer = self.usb2_control.data();

        // // parse command
        // let (class_id, verb_number, arguments) = match libgreat::gcp::Command::parse(command_buffer)
        // {
        //     Some(command) => (command.class_id(), command.verb_number(), command.arguments),
        //     None => {
        //         error!("dispatch_libgreat_request failed to parse libgreat command");
        //         return Ok(());
        //     }
        // };

        // // dispatch command
        // let response_buffer: [u8; LIBGREAT_MAX_COMMAND_SIZE] = [0; LIBGREAT_MAX_COMMAND_SIZE];
        // let response = match class_id {
        //     // class: core
        //     libgreat::gcp::ClassId::core => {
        //         self.core.dispatch(verb_number, arguments, response_buffer)
        //     }
        //     // class: firmware
        //     libgreat::gcp::ClassId::firmware => {
        //         moondancer::gcp::firmware::dispatch(verb_number, arguments, response_buffer)
        //     }
        //     // class: selftest
        //     libgreat::gcp::ClassId::selftest => {
        //         moondancer::gcp::selftest::dispatch(verb_number, arguments, response_buffer)
        //     }
        //     // class: moondancer
        //     libgreat::gcp::ClassId::moondancer => {
        //         self.moondancer
        //             .dispatch(verb_number, arguments, response_buffer)
        //     }
        //     // class: unsupported
        //     _ => {
        //         error!(
        //             "dispatch_libgreat_request error: Class id '{:?}' not found",
        //             class_id
        //         );
        //         Err(GreatError::InvalidArgument)
        //     }
        // };

        // // queue response
        // match response {
        //     Ok(response) => {
        //         self.libgreat_response = Some(response);
        //         self.libgreat_response_last_error = None;
        //     }
        //     Err(e) => {
        //         error!(
        //             "dispatch_libgreat_request error: failed to dispatch command {:?} 0x{:X} {}",
        //             class_id, verb_number, e
        //         );

        //         self.libgreat_response = None;
        //         self.libgreat_response_last_error = Some(e);

        //         // stall endpoint to trigger dispatch_libgreat_abort from control host
        //         #[cfg(feature = "cynthion_hw")]
        //         self.usb2.stall_endpoint_in(0);
        //     }
        // }

        Ok(())
    }

    fn dispatch_libgreat_response(&mut self, setup_packet: SetupPacket) -> GreatResult<()> {
        let requested_length = setup_packet.length as usize;

        // do we have a response ready?
        if let Some(response) = &mut self.libgreat_response {
            // prime to receive host zlp
            #[cfg(feature = "cynthion_hw")]
            self.usb2.ep_out_prime_receive(0);

            // send response
            #[cfg(feature = "cynthion_hw")]
            self.usb2.write_requested(0, requested_length, response);

            // clear any queued responses
            self.libgreat_response = None;

        } else if let Some(error) = self.libgreat_response_last_error {
            warn!("dispatch_libgreat_response error result: {:?}", error);

        } else {
            #[cfg(feature = "cynthion_hw")]
            self.usb2.stall_endpoint_in(0);
            error!("dispatch_libgreat_response stall: libgreat response requested but no response or error queued");
        }

        Ok(())
    }

    fn dispatch_libgreat_abort(&mut self, setup_packet: SetupPacket) -> GreatResult<()> {
        let requested_length = setup_packet.length as usize;

        // prime to receive host zlp
        #[cfg(feature = "cynthion_hw")]
        self.usb2.ep_out_prime_receive(0);

        // send error response
        if let Some(error) = self.libgreat_response_last_error {
            #[cfg(feature = "cynthion_hw")]
            self.usb2.write_requested(0, requested_length, (error as u32).to_le_bytes().into_iter());
            warn!("dispatch_libgreat_abort: {:?}", error);
        } else {
            #[cfg(feature = "cynthion_hw")]
            self.usb2.write_requested(0, requested_length, (GreatError::StateNotRecoverable as u32).to_le_bytes().into_iter());
            warn!("dispatch_libgreat_abort: libgreat abort requested but no error queued");
        }

        // clear any queued responses
        self.libgreat_response = None;
        self.libgreat_response_last_error = None;

        Ok(())
    }
}
