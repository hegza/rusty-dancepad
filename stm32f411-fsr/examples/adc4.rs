#![no_std]
#![no_main]

use panic_probe as _;

#[rtic::app(device = stm32f4xx_hal::pac, dispatchers = [EXTI0])]
mod app {
    use dwt_systick_monotonic::DwtSystick;

    use rtt_target::{rprintln, rtt_init_print};
    use stm32f4xx_hal::{
        adc::{
            config::{AdcConfig, Dma, SampleTime, Scan, Sequence},
            Adc,
        },
        dma::{config::DmaConfig, PeripheralToMemory, Stream0, StreamsTuple, Transfer},
        pac::{self, ADC1, DMA2},
        prelude::*,
    };

    const MONO_HZ: u32 = 84_000_000; // 8 MHz

    #[monotonic(binds = SysTick, default = true)]
    type MyMono = DwtSystick<MONO_HZ>;

    type DMATransfer =
        Transfer<Stream0<DMA2>, 0, Adc<ADC1>, PeripheralToMemory, &'static mut [u16; 4]>;

    #[shared]
    struct Shared {
        transfer: DMATransfer,
    }

    #[local]
    struct Local {
        buffer: Option<&'static mut [u16; 4]>,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        rtt_init_print!();
        rprintln!("[adc]");

        let device: pac::Peripherals = cx.device;

        let rcc = device.RCC.constrain();
        let _clocks = rcc
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

        let gpioa = device.GPIOA.split();
        let gpiob = device.GPIOB.split();
        let v1 = gpioa.pa5.into_analog();
        let v2 = gpioa.pa6.into_analog();
        let v3 = gpioa.pa7.into_analog();
        let v4 = gpiob.pb0.into_analog();

        let dma = StreamsTuple::new(device.DMA2);

        let config = DmaConfig::default()
            .transfer_complete_interrupt(true)
            .memory_increment(true)
            .double_buffer(false);

        let adc_config = AdcConfig::default()
            .dma(Dma::Continuous)
            .scan(Scan::Enabled);

        let mut adc = Adc::adc1(device.ADC1, true, adc_config);
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

        polling::spawn_after(1.secs()).ok();

        (
            Shared { transfer },
            Local {
                buffer: second_buffer,
            },
            init::Monotonics(mono),
        )
    }

    #[task(shared = [transfer])]
    fn polling(mut cx: polling::Context) {
        cx.shared.transfer.lock(|transfer| {
            transfer.start(|adc| {
                adc.start_conversion();
            });
        });

        polling::spawn_after(1.secs()).ok();
    }

    #[task(binds = DMA2_STREAM0, shared = [transfer], local = [buffer])]
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

        // Pull the ADC data out of the buffer that the DMA transfer gave us
        let raw_volt1 = buffer[0];
        let raw_volt2 = buffer[1];
        let raw_volt3 = buffer[2];
        let raw_volt4 = buffer[3];

        // Now that we're finished with this buffer, put it back in `local.buffer` so
        // it's ready for the next transfer If we don't do this before the next
        // transfer, we'll get a panic
        *local.buffer = Some(buffer);

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
