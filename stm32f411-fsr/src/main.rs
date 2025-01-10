#![no_std]
#![no_main]
#![allow(static_mut_refs)]
mod logging;
mod push_buffer;

type AdcValues = abi::AdcValues<4>;
use panic_probe as _;
use usbd_human_interface_device::device::joystick::JoystickReport;

#[rustfmt::skip]
const DEFAULT_THRESH: [u16; 4] = [
    // Left
    2400,
    // Down
    450,
    // Right
    2200,
    // Up
    1000,
    // ???: above is not flashed yet
];

static mut EP_MEMORY: [u32; 1024] = [0; 1024];

fn get_report(vals: &AdcValues, thresh: &[u16; 4]) -> JoystickReport {
    // Read out 8 buttons first
    let mut buttons = 0;

    for (idx, v) in vals.iter().enumerate() {
        if *v >= thresh[idx] {
            buttons |= 0b1 << idx;
        }
    }

    // Always return center for the analog value
    let (x, y) = (0, 0);

    JoystickReport { buttons, x, y }
}

#[rtic::app(device = stm32f4xx_hal::pac, dispatchers = [EXTI0])]
mod app {
    use core::ptr;

    use crate::{push_buffer::PushBuffer, AdcValues, DEFAULT_THRESH};
    use abi::Codec;
    use dwt_systick_monotonic::DwtSystick;
    use log::{debug, info, trace, warn};
    use rtt_target::{rprintln, rtt_init_print};
    use stm32f4xx_hal::{
        adc::{
            config::{AdcConfig, Dma, SampleTime, Scan, Sequence},
            Adc,
        },
        dma::{config::DmaConfig, PeripheralToMemory, Stream0, StreamsTuple, Transfer},
        gpio::{self, Output, PushPull},
        otg_fs::{UsbBus, USB},
        pac::{self, ADC1, DMA2, TIM1, USART1},
        prelude::*,
        serial::{self, Serial},
        timer::{self, CounterHz, Event, Timer},
    };
    use usb_device::{
        bus::UsbBusAllocator,
        device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid},
    };
    use usbd_human_interface_device::{device::joystick::Joystick, prelude::*};
    use usbd_serial::embedded_io::Write;

    static mut USB_BUS_ALLOCATOR: Option<UsbBusAllocator<UsbBus<USB>>> = None;

    const MONO_HZ: u32 = 84_000_000;

    #[monotonic(binds = SysTick, default = true)]
    type MyMono = DwtSystick<MONO_HZ>;

    type DMATransfer =
        Transfer<Stream0<DMA2>, 0, Adc<ADC1>, PeripheralToMemory, &'static mut [u16; 4]>;

    #[shared]
    struct Shared {
        transfer: DMATransfer,
        adc_values: AdcValues,
        thresh: [u16; 4],
    }

    #[local]
    struct Local {
        buffer: Option<&'static mut [u16; 4]>,
        usb_dev: UsbDevice<'static, UsbBus<USB>>,
        timer: CounterHz<pac::TIM2>,
        joy: UsbHidClass<'static, UsbBus<USB>, frunk::HList!(Joystick<'static, UsbBus<USB>>)>,
        dma_counter: usize,
        cmd_buf: Option<PushBuffer<{ abi::Command::MAX_SERIALIZED_LEN }>>,
        serial_rx: serial::Rx<USART1>,
        serial_tx: serial::Tx<USART1>,
        led: gpio::PC13<Output<PushPull>>,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        rtt_init_print!();
        rprintln!("[rusty_dancepad]");

        crate::logging::init();
        info!("logger initialized at level {}", log::max_level());

        let dp: pac::Peripherals = cx.device;

        let rcc = dp.RCC.constrain();
        let clocks = rcc
            .cfgr
            .use_hse(25.MHz())
            .require_pll48clk()
            .sysclk(MONO_HZ.Hz())
            .hclk(MONO_HZ.Hz())
            .pclk1(42.MHz())
            .pclk2(84.MHz())
            .freeze();

        let mut dcb = cx.core.DCB;
        let dwt = cx.core.DWT;
        let systick = cx.core.SYST;

        let mono = DwtSystick::new(&mut dcb, dwt, systick, MONO_HZ);

        // Configure TIM2 as a periodic timer
        let mut timer = Timer::new(dp.TIM2, &clocks).counter_hz();
        timer.start(1_000.Hz()).unwrap();
        timer.listen(Event::Update);

        // Configure the LED pin as a push pull output and obtain handle
        // On the Blackpill STM32F411CEU6 there is an on-board LED connected to pin PC13
        // 1) Promote the GPIOC PAC struct
        let gpioc = dp.GPIOC.split();

        // 2) Configure PORTC OUTPUT Pins and Obtain Handle
        let led = gpioc.pc13.into_push_pull_output();

        let gpioa = dp.GPIOA.split();
        let gpiob = dp.GPIOB.split();
        let v1 = gpioa.pa5.into_analog();
        let v2 = gpioa.pa6.into_analog();
        let v3 = gpioa.pa7.into_analog();
        let v4 = gpiob.pb0.into_analog();

        let tx_pin = gpiob.pb6;
        let rx_pin = gpiob.pb7;
        let config = serial::Config::default().baudrate(115200.bps());

        let serial = Serial::new(dp.USART1, (tx_pin, rx_pin), config, &clocks).unwrap();
        let (serial_tx, mut serial_rx) = serial.split();

        serial_rx.listen();
        serial_rx.listen_idle();

        // USB
        let (usb_dev, joy) = {
            let usb = USB::new(
                (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
                (gpioa.pa11, gpioa.pa12),
                &clocks,
            );

            let usb_bus = UsbBus::new(usb, unsafe { &mut *ptr::addr_of_mut!(crate::EP_MEMORY) });
            unsafe { USB_BUS_ALLOCATOR.replace(usb_bus) };

            let joy = UsbHidClassBuilder::new()
                .add_device(
                    usbd_human_interface_device::device::joystick::JoystickConfig::default(),
                )
                .build(unsafe { USB_BUS_ALLOCATOR.as_ref().unwrap() });

            //https://pid.codes
            let usb_dev = UsbDeviceBuilder::new(
                unsafe { USB_BUS_ALLOCATOR.as_ref().unwrap() },
                UsbVidPid(0x1209, 0x0001),
            )
            .strings(&[StringDescriptors::default()
                .manufacturer("Hegza")
                .product("Rusty Joystick")
                .serial_number("TEST")])
            .unwrap()
            .build();

            (usb_dev, joy)
        };

        let adc_config = AdcConfig::default()
            .dma(Dma::Continuous)
            .scan(Scan::Enabled);

        let mut adc = Adc::adc1(dp.ADC1, true, adc_config);
        adc.configure_channel(&v1, Sequence::One, SampleTime::Cycles_480);
        adc.configure_channel(&v2, Sequence::Two, SampleTime::Cycles_480);
        adc.configure_channel(&v3, Sequence::Three, SampleTime::Cycles_480);
        adc.configure_channel(&v4, Sequence::Four, SampleTime::Cycles_480);
        adc.enable_temperature_and_vref();

        let dma = StreamsTuple::new(dp.DMA2);
        let dma_config = DmaConfig::default()
            .transfer_complete_interrupt(true)
            .memory_increment(true)
            .double_buffer(false);

        // These buffers need to be 'static to use safely with the DMA - we can't allow
        // them to be dropped while the DMA is accessing them. The easiest way
        // to satisfy that is to make them static, and the safest way to do that is with
        // `cortex_m::singleton!`
        let first_buffer = cortex_m::singleton!(: [u16; 4] = [0; 4]).unwrap();
        let second_buffer = Some(cortex_m::singleton!(: [u16; 4] = [0; 4]).unwrap());
        // Give the first buffer to the DMA. The second buffer is held in an Option in
        // `local.buffer` until the transfer is complete
        let transfer =
            Transfer::init_peripheral_to_memory(dma.0, adc, first_buffer, None, dma_config);

        adc_poll::spawn_after(1.millis()).ok();

        (
            Shared {
                transfer,
                adc_values: Default::default(),
                thresh: DEFAULT_THRESH,
            },
            Local {
                buffer: second_buffer,
                usb_dev,
                joy,
                timer,
                dma_counter: 0,
                serial_rx,
                serial_tx,
                cmd_buf: None,
                led,
            },
            init::Monotonics(mono),
        )
    }

    #[idle(local = [led], shared = [])]
    fn idle(ctx: idle::Context) -> ! {
        let led = ctx.local.led;
        let mut i = 0;
        loop {
            // Turn On LED
            led.set_high();
            // Delay
            for _ in 0..20_000_000 {
                unsafe { core::arch::asm!("nop") };
            }
            // Turn off LED
            led.set_low();
            // Obtain shared delay variable and delay
            for _ in 0..20_000_000 {
                unsafe { core::arch::asm!("nop") };
            }
            i = (i + 1) % 4;
            if i == 0 {
                trace!("alive");
            }
        }
    }

    #[task(shared = [transfer], priority = 4)]
    fn adc_poll(mut cx: adc_poll::Context) {
        cx.shared.transfer.lock(|transfer| {
            transfer.start(|adc| {
                adc.start_conversion();
            });
        });

        adc_poll::spawn_after(1.millis()).ok();
    }

    #[task(binds = DMA2_STREAM0, priority = 3, shared = [transfer, adc_values], local = [buffer, dma_counter])]
    fn dma(cx: dma::Context) {
        let dma::Context { mut shared, local } = cx;
        let (buffer, sample_to_millivolts) = shared.transfer.lock(|transfer| {
            // When the DMA completes it will return the buffer we gave it last time - we
            // now store that as `buffer` We still have our other buffer waiting
            // in `local.buffer`, so `take` that and give it to the `transfer`
            let (buffer, _) = transfer
                .next_transfer(local.buffer.take().unwrap())
                .unwrap();

            let sample_to_millivolts = transfer.peripheral().make_sample_to_millivolts();
            (buffer, sample_to_millivolts)
        });

        shared.adc_values.lock(|vals| {
            *vals = abi::AdcValues(*buffer);
        });

        // Pull the ADC data out of the buffer that the DMA transfer gave us
        let raw_volt1 = buffer[0];
        let raw_volt2 = buffer[1];
        let raw_volt3 = buffer[2];
        let raw_volt4 = buffer[3];

        // Now that we're finished with this buffer, put it back in `local.buffer` so
        // it's ready for the next transfer If we don't do this before the next
        // transfer, we'll get a panic
        *local.buffer = Some(buffer);

        // Print periodically
        *local.dma_counter = (*local.dma_counter + 1) % 500;
        if *local.dma_counter == 0 {
            let voltage1 = sample_to_millivolts(raw_volt1);
            let voltage2 = sample_to_millivolts(raw_volt2);
            let voltage3 = sample_to_millivolts(raw_volt3);
            let voltage4 = sample_to_millivolts(raw_volt4);

            rprintln!(
                "voltage 1: {:<4}, voltage 2: {:<4}, voltage 3: {:<4}, voltage 4: {:<4}",
                voltage1,
                voltage2,
                voltage3,
                voltage4
            );
        }
    }

    #[task(binds = TIM2, priority = 2, local = [timer, usb_dev, joy], shared = [adc_values, thresh])]
    fn usb_report(mut cx: usb_report::Context) {
        let timer = cx.local.timer;

        let values = cx.shared.adc_values.lock(|vals| vals.clone());
        let thresh = cx.shared.thresh.lock(|vals| vals.clone());
        // Poll every 1ms
        match cx
            .local
            .joy
            .device()
            .write_report(&crate::get_report(&values, &thresh))
        {
            Err(UsbHidError::WouldBlock) => {}
            Ok(_) => {}
            Err(e) => {
                core::panic!("Failed to write joystick report: {:?}", e)
            }
        }

        if cx.local.usb_dev.poll(&mut [cx.local.joy]) {}

        // Clear the timer interrupt flag
        timer.clear_all_flags();
    }

    #[task(binds = USART1, priority = 1, shared = [thresh, adc_values], local = [cmd_buf, serial_rx, serial_tx])]
    fn uart_rx(mut cx: uart_rx::Context) {
        let b = cx.local.serial_rx.read().unwrap();
        if b != abi::corncobs::ZERO {
            cx.local
                .cmd_buf
                .get_or_insert(PushBuffer::default())
                .push(b)
                // SAFETY: panics on buffer overflow
                .unwrap();
            return;
        }

        // Frame received -> act
        let (mut packet, _len) = cx
            .local
            .cmd_buf
            .take()
            // SAFETY: guaranteed to exist, inserted by push above if it didn't exist
            .unwrap()
            .finish();
        let cmd = abi::Command::deserialize_in_place(&mut packet).unwrap();

        let resp = match cmd {
            abi::Command::GetValues => {
                abi::Response::Values4(cx.shared.adc_values.lock(|vals| vals.clone()).0)
            }
            abi::Command::GetThresh => abi::Response::Values4(cx.shared.thresh.lock(|th| *th)),
            abi::Command::SetThresh4(nth) => {
                cx.shared.thresh.lock(|th| {
                    *th = nth.into();
                });
                abi::Response::Ok
            }
            abi::Command::Ping => abi::Response::Ok,
        };
        let mut resp_buf = [0u8; abi::Response::MAX_SERIALIZED_LEN];
        resp.serialize(&mut resp_buf)
            // SAFETY: serialization should never fail
            .unwrap();
        cx.local.serial_tx.write_all(&resp_buf).unwrap();
    }
}
