#![no_std]
#![no_main]

use panic_probe as _;
use usbd_human_interface_device::device::joystick::JoystickReport;

static mut EP_MEMORY: [u32; 1024] = [0; 1024];

/// Raw ADC values
#[derive(Default, Clone)]
pub struct AdcValues([u16; 4]);

fn get_report(vals: &AdcValues) -> JoystickReport {
    // Read out 8 buttons first
    let mut buttons = 0;

    const THRESH: u16 = 512;
    for (idx, v) in vals.0.iter().enumerate() {
        if *v >= THRESH {
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

    use crate::AdcValues;
    use dwt_systick_monotonic::DwtSystick;
    use rtt_target::{rprintln, rtt_init_print};
    use stm32f4xx_hal::{
        adc::{
            config::{AdcConfig, Dma, SampleTime, Scan, Sequence},
            Adc,
        },
        dma::{config::DmaConfig, PeripheralToMemory, Stream0, StreamsTuple, Transfer},
        otg_fs::{UsbBus, USB},
        pac::{self, ADC1, DMA2},
        prelude::*,
        timer::{CounterHz, Event, Timer},
    };
    use usb_device::{
        bus::UsbBusAllocator,
        device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid},
    };
    use usbd_human_interface_device::{device::joystick::Joystick, prelude::*};

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
    }

    #[local]
    struct Local {
        buffer: Option<&'static mut [u16; 4]>,
        usb_dev: UsbDevice<'static, UsbBus<USB>>,
        timer: CounterHz<pac::TIM2>,
        joy: UsbHidClass<'static, UsbBus<USB>, frunk::HList!(Joystick<'static, UsbBus<USB>>)>,
        dma_counter: usize,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        rtt_init_print!();
        rprintln!("[rusty_dancepad]");

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

        let gpioa = dp.GPIOA.split();
        let gpiob = dp.GPIOB.split();
        let v1 = gpioa.pa5.into_analog();
        let v2 = gpioa.pa6.into_analog();
        let v3 = gpioa.pa7.into_analog();
        let v4 = gpiob.pb0.into_analog();

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

        let dma = StreamsTuple::new(dp.DMA2);

        let config = DmaConfig::default()
            .transfer_complete_interrupt(true)
            .memory_increment(true)
            .double_buffer(false);

        let adc_config = AdcConfig::default()
            .dma(Dma::Continuous)
            .scan(Scan::Enabled);

        let mut adc = Adc::adc1(dp.ADC1, true, adc_config);
        adc.configure_channel(&v1, Sequence::One, SampleTime::Cycles_480);
        adc.configure_channel(&v2, Sequence::Two, SampleTime::Cycles_480);
        adc.configure_channel(&v3, Sequence::Three, SampleTime::Cycles_480);
        adc.configure_channel(&v4, Sequence::Four, SampleTime::Cycles_480);
        adc.enable_temperature_and_vref();

        // These buffers need to be 'static to use safely with the DMA - we can't allow
        // them to be dropped while the DMA is accessing them. The easiest way
        // to satisfy that is to make them static, and the safest way to do that is with
        // `cortex_m::singleton!`
        let first_buffer = cortex_m::singleton!(: [u16; 4] = [0; 4]).unwrap();
        let second_buffer = Some(cortex_m::singleton!(: [u16; 4] = [0; 4]).unwrap());
        // Give the first buffer to the DMA. The second buffer is held in an Option in
        // `local.buffer` until the transfer is complete
        let transfer = Transfer::init_peripheral_to_memory(dma.0, adc, first_buffer, None, config);

        adc_poll::spawn_after(1.millis()).ok();

        (
            Shared {
                transfer,
                adc_values: Default::default(),
            },
            Local {
                buffer: second_buffer,
                usb_dev,
                joy,
                timer,
                dma_counter: 0,
            },
            init::Monotonics(mono),
        )
    }

    #[task(shared = [transfer])]
    fn adc_poll(mut cx: adc_poll::Context) {
        cx.shared.transfer.lock(|transfer| {
            transfer.start(|adc| {
                adc.start_conversion();
            });
        });

        adc_poll::spawn_after(1.millis()).ok();
    }

    #[task(binds = DMA2_STREAM0, shared = [transfer, adc_values], local = [buffer, dma_counter])]
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
            *vals = AdcValues(*buffer);
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

    #[task(binds = TIM2, local = [timer, usb_dev, joy], shared = [adc_values])]
    fn usb_report(mut cx: usb_report::Context) {
        let timer = cx.local.timer;

        let values = cx.shared.adc_values.lock(|vals| vals.clone());
        // Poll every 1ms
        match cx
            .local
            .joy
            .device()
            .write_report(&crate::get_report(&values))
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
}
