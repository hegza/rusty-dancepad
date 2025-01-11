#![no_std]
#![no_main]
#![allow(static_mut_refs)]
#![feature(stmt_expr_attributes)]
mod logging;
mod profile;
mod push_buffer;

// HACK: this has to be exactly the number of ADCs in use
const MAX_ADC_COUNT: usize = 8;
type AdcValues = abi::AdcValues<MAX_ADC_COUNT>;
use core::ptr;

use panic_probe as _;
use profile::{Profile, SensorConfig, TrigCond};
use stm32f4xx_hal::otg_fs::{UsbBus, USB};
use usb_device::{
    bus::UsbBusAllocator,
    device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid},
};
use usbd_human_interface_device::{
    device::joystick::{Joystick, JoystickReport},
    prelude::*,
};

const DEFAULT_PROFILE: Profile = Profile {
    pa0: None,
    // Up, left
    pa1: Some(SensorConfig {
        cond: TrigCond::Rel { idle: 0, thr: 0.25 },
        btn: 2,
    }),
    // Up, right
    pa2: Some(SensorConfig {
        cond: TrigCond::Rel { idle: 0, thr: 0.25 },
        btn: 2,
    }),
    // Right, left
    pa3: Some(SensorConfig {
        cond: TrigCond::Rel { idle: 0, thr: 0.25 },
        btn: 1,
    }),
    // Right, right
    pa4: Some(SensorConfig {
        cond: TrigCond::Rel { idle: 0, thr: 0.25 },
        btn: 1,
    }),
    // Down, left
    pa5: Some(SensorConfig {
        cond: TrigCond::Rel { idle: 0, thr: 0.25 },
        btn: 3,
    }),
    // Down, right
    pa6: Some(SensorConfig {
        cond: TrigCond::Rel { idle: 0, thr: 0.25 },
        btn: 3,
    }),
    // Left, left
    pa7: Some(SensorConfig {
        cond: TrigCond::Rel { idle: 0, thr: 0.25 },
        btn: 0,
    }),
    // Left, right
    pb0: Some(SensorConfig {
        cond: TrigCond::Rel { idle: 0, thr: 0.25 },
        btn: 0,
    }),
    pb1: None,
};

/// Scratchpad for USB
static mut EP_MEMORY: [u32; 1024] = [0; 1024];
static mut USB_BUS_ALLOCATOR: Option<UsbBusAllocator<UsbBus<USB>>> = None;

fn get_report(vals: &AdcValues, adc_map: &[(usize, u16)]) -> JoystickReport {
    // Joystick exposes 8 buttons, represented as the single bits in a u8
    let mut buttons = 0;

    // Fill out each button state based on whether the ADC value is above the threshold
    for (val, (btn_idx, thr)) in vals.iter().zip(adc_map.iter()) {
        if *val >= *thr {
            buttons |= 0b1 << btn_idx;
        }
    }

    // Always return center for the analog value
    let (x, y) = (0, 0);

    JoystickReport { buttons, x, y }
}

fn setup_usb_joystick(
    usb: USB,
) -> (
    UsbDevice<'static, UsbBus<USB>>,
    UsbHidClass<'static, UsbBus<USB>, frunk::HList!(Joystick<'static, UsbBus<USB>>)>,
) {
    let usb_bus = UsbBus::new(usb, unsafe { &mut *ptr::addr_of_mut!(crate::EP_MEMORY) });
    unsafe { USB_BUS_ALLOCATOR.replace(usb_bus) };

    let joy = UsbHidClassBuilder::new()
        .add_device(usbd_human_interface_device::device::joystick::JoystickConfig::default())
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
}

#[rtic::app(device = stm32f4xx_hal::pac, dispatchers = [EXTI0])]
mod app {
    use crate::DEFAULT_PROFILE;
    use crate::{setup_usb_joystick, AdcValues, MAX_ADC_COUNT};
    use dwt_systick_monotonic::DwtSystick;
    use heapless::Vec;
    use log::{info, trace};
    use rtt_target::{rprint, rprintln, rtt_init_print};
    use stm32f4xx_hal::otg_fs::{UsbBus, USB};
    use stm32f4xx_hal::{
        adc::{
            config::{AdcConfig, Dma, SampleTime, Scan},
            Adc,
        },
        dma::{config::DmaConfig, PeripheralToMemory, Stream0, StreamsTuple, Transfer},
        gpio::{self, Output, PushPull},
        pac::{self, ADC1, DMA2},
        prelude::*,
        timer::{CounterHz, Event, Timer},
    };
    use usb_device::device::UsbDevice;
    use usbd_human_interface_device::{device::joystick::Joystick, prelude::*};

    const MONO_HZ: u32 = 84_000_000;

    #[monotonic(binds = SysTick, default = true)]
    type MyMono = DwtSystick<MONO_HZ>;

    type DMATransfer = Transfer<
        Stream0<DMA2>,
        0,
        Adc<ADC1>,
        PeripheralToMemory,
        &'static mut [u16; MAX_ADC_COUNT],
    >;

    #[shared]
    struct Shared {
        transfer: DMATransfer,
        adc_values: AdcValues,
        adc_map: Vec<(usize, u16), MAX_ADC_COUNT>,
    }

    #[local]
    struct Local {
        buffer: Option<&'static mut [u16; MAX_ADC_COUNT]>,
        timer: CounterHz<pac::TIM2>,
        usb_dev: UsbDevice<'static, UsbBus<USB>>,
        joy: UsbHidClass<'static, UsbBus<USB>, frunk::HList!(Joystick<'static, UsbBus<USB>>)>,
        dma_counter: usize,
        /*
        cmd_buf: Option<PushBuffer<{ abi::Command::MAX_SERIALIZED_LEN }>>,
        serial_rx: serial::Rx<USART1>,
        serial_tx: serial::Tx<USART1>,
        */
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

        let gpiob = dp.GPIOB.split();

        /*
        let tx_pin = gpiob.pb6;
        let rx_pin = gpiob.pb7;
        let config = serial::Config::default().baudrate(115200.bps());

        let serial = Serial::new(dp.USART1, (tx_pin, rx_pin), config, &clocks).unwrap();
        let (serial_tx, mut serial_rx) = serial.split();

        serial_rx.listen();
        serial_rx.listen_idle();
        */

        let gpioa = dp.GPIOA.split();

        // USB
        let usb = USB::new(
            (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
            (gpioa.pa11, gpioa.pa12),
            &clocks,
        );
        let (usb_dev, joy) = setup_usb_joystick(usb);

        let adc_config = AdcConfig::default()
            .dma(Dma::Continuous)
            .scan(Scan::Enabled);
        let mut adc = Adc::adc1(dp.ADC1, true, adc_config);
        let mut adc_count = 0;

        let prof = DEFAULT_PROFILE;
        if prof.pa0.is_some() {
            adc.configure_channel(
                &gpioa.pa0.into_analog(),
                (adc_count as u8 + 1u8).into(),
                SampleTime::Cycles_480,
            );
            adc_count += 1;
        }
        if prof.pa1.is_some() {
            adc.configure_channel(
                &gpioa.pa1.into_analog(),
                (adc_count as u8 + 1u8).into(),
                SampleTime::Cycles_480,
            );
            adc_count += 1;
        }
        if prof.pa2.is_some() {
            adc.configure_channel(
                &gpioa.pa2.into_analog(),
                (adc_count as u8 + 1u8).into(),
                SampleTime::Cycles_480,
            );
            adc_count += 1;
        }
        if prof.pa3.is_some() {
            adc.configure_channel(
                &gpioa.pa3.into_analog(),
                (adc_count as u8 + 1u8).into(),
                SampleTime::Cycles_480,
            );
            adc_count += 1;
        }
        if prof.pa4.is_some() {
            adc.configure_channel(
                &gpioa.pa4.into_analog(),
                (adc_count as u8 + 1u8).into(),
                SampleTime::Cycles_480,
            );
            adc_count += 1;
        }
        if prof.pa5.is_some() {
            adc.configure_channel(
                &gpioa.pa5.into_analog(),
                (adc_count as u8 + 1u8).into(),
                SampleTime::Cycles_480,
            );
            adc_count += 1;
        }
        if prof.pa6.is_some() {
            adc.configure_channel(
                &gpioa.pa6.into_analog(),
                (adc_count as u8 + 1u8).into(),
                SampleTime::Cycles_480,
            );
            adc_count += 1;
        }
        if prof.pa7.is_some() {
            adc.configure_channel(
                &gpioa.pa7.into_analog(),
                (adc_count as u8 + 1u8).into(),
                SampleTime::Cycles_480,
            );
            adc_count += 1;
        }
        if prof.pb0.is_some() {
            adc.configure_channel(
                &gpiob.pb0.into_analog(),
                (adc_count as u8 + 1u8).into(),
                SampleTime::Cycles_480,
            );
            adc_count += 1;
        }
        if prof.pb1.is_some() {
            adc.configure_channel(
                &gpiob.pb1.into_analog(),
                (adc_count as u8 + 1u8).into(),
                SampleTime::Cycles_480,
            );
            #[allow(unused_assignments)]
            adc_count += 1;
        }

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
        let first_buffer =
            cortex_m::singleton!(: [u16; MAX_ADC_COUNT] = [0; MAX_ADC_COUNT]).unwrap();
        let second_buffer =
            Some(cortex_m::singleton!(: [u16; MAX_ADC_COUNT] = [0; MAX_ADC_COUNT]).unwrap());
        // Give the first buffer to the DMA. The second buffer is held in an Option in
        // `local.buffer` until the transfer is complete
        let transfer =
            Transfer::init_peripheral_to_memory(dma.0, adc, first_buffer, None, dma_config);

        adc_poll::spawn_after(1.millis()).ok();

        (
            Shared {
                transfer,
                adc_values: Default::default(),
                adc_map: prof.get_adc_map(),
            },
            Local {
                buffer: second_buffer,
                usb_dev,
                joy,
                timer,
                dma_counter: 0,
                /*
                serial_rx,
                serial_tx,
                cmd_buf: None,
                */
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
            for _ in 0..20_000_000 {
                unsafe { core::arch::asm!("nop") };
            }
            // Turn off LED
            led.set_low();
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
        let raw_volts = buffer.clone();

        // Now that we're finished with this buffer, put it back in `local.buffer` so
        // it's ready for the next transfer If we don't do this before the next
        // transfer, we'll get a panic
        *local.buffer = Some(buffer);

        // Print periodically
        *local.dma_counter = (*local.dma_counter + 1) % 500;
        if *local.dma_counter == 0 {
            for (idx, raw) in raw_volts.into_iter().enumerate() {
                let voltage = sample_to_millivolts(raw);
                rprint!("voltage {}: {:<4} ", idx, voltage,);
            }
            rprintln!();
        }
    }

    #[task(binds = TIM2, priority = 2, local = [timer, usb_dev, joy], shared = [adc_values, adc_map])]
    fn usb_report(mut cx: usb_report::Context) {
        let timer = cx.local.timer;

        let values = cx.shared.adc_values.lock(|vals| vals.clone());
        let adc_map = cx.shared.adc_map.lock(|vals| vals.clone());
        // Poll every 1ms
        match cx
            .local
            .joy
            .device()
            .write_report(&crate::get_report(&values, &adc_map))
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

    /*
    #[task(binds = USART1, priority = 1, shared = [adc_map, adc_values], local = [cmd_buf, serial_rx, serial_tx])]
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
    */
}
