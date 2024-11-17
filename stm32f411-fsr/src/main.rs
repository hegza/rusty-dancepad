#![deny(unsafe_code)]
#![no_main]
#![no_std]

// Print panic message to probe console
use panic_probe as _;

use stm32f4xx_hal::gpio::{Input, PA0, PA1, PA2, PA3};

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true)]
mod app {
    use stm32f4xx_hal::prelude::*;

    #[shared]
    struct Shared {}

    // Local resources go here
    #[local]
    struct Local {}

    #[init]
    fn init(mut ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        (
            Shared {
               // Initialization of shared resources go here
            },
            Local {
                // Initialization of local resources go here
            },
            init::Monotonics(),
        )
    }

    // Optional idle, can be removed if not needed.
    #[idle]
    fn idle(_: idle::Context) -> ! {
        loop {
            continue;
        }
    }
}

struct AdcPins {
    adc0: PA0<Input>,
    adc1: PA1<Input>,
    adc2: PA2<Input>,
    adc3: PA3<Input>,
}

fn get_report(/*pins: AdcPins*/) -> JoystickReport {
    // Read out 8 buttons first
    let mut buttons = 0;

    buttons = 0b0101_0101;

    // Always return center
    let (x, y) = (0, 0);

    JoystickReport { buttons, x, y }
}
