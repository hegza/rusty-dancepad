#![deny(unsafe_code)]
#![no_main]
#![no_std]

// Print panic message to probe console
use panic_probe as _;

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true)]
mod app {
    use stm32f4xx_hal::{
        gpio::{Output, PushPull, PC13},
        pac,
        prelude::*,
        timer::{Event, Timer},
    };

    #[shared]
    struct Shared {}

    // Local resources go here
    #[local]
    struct Local {
        led: PC13<Output<PushPull>>,
        timer: Timer<pac::TIM2>,
    }

    #[init]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        // Get the device peripherals
        let device: pac::Peripherals = ctx.device;

        // Initialize the clocks
        let rcc = device.RCC.constrain();
        let clocks = rcc.cfgr.sysclk(84.MHz()).freeze();

        // Initialize GPIOA (for the onboard LED on PA5)
        let gpioc = device.GPIOC.split();
        let led = gpioc.pc13.into_push_pull_output();

        // Configure TIM2 as a periodic timer
        let mut timer = Timer::new(device.TIM2, &clocks);
        timer.listen(Event::Update);

        (Shared {}, Local { led, timer }, init::Monotonics())
    }

    // Optional idle, can be removed if not needed.
    #[idle]
    fn idle(_: idle::Context) -> ! {
        loop {
            continue;
        }
    }

    #[task(binds = TIM2, local = [led, timer])]
    fn blink(ctx: blink::Context) {
        let led = ctx.local.led;
        let timer = ctx.local.timer;

        // Toggle the LED
        led.toggle();

        // Clear the timer interrupt flag
        timer.clear_all_flags();
    }
}
