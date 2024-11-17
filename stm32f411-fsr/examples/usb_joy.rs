#![no_main]
#![no_std]

// Print panic message to probe console
use panic_probe as _;

use usbd_human_interface_device::device::joystick::JoystickReport;

static mut EP_MEMORY: [u32; 1024] = [0; 1024];

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true)]
mod app {
    use core::ptr;

    use rtt_target::{rprintln, rtt_init_print};
    use stm32f4xx_hal::{
        gpio::{Output, PushPull, PC13},
        otg_fs::{UsbBus, USB},
        pac,
        prelude::*,
        timer::{CounterHz, Event, Timer},
    };
    use usb_device::{
        bus::UsbBusAllocator,
        device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid},
    };
    use usbd_human_interface_device::{device::joystick::Joystick, prelude::*};

    static mut USB_BUS_ALLOCATOR: Option<UsbBusAllocator<UsbBus<USB>>> = None;

    #[shared]
    struct Shared {}

    // Local resources go here
    #[local]
    struct Local {
        led: PC13<Output<PushPull>>,
        timer: CounterHz<pac::TIM2>,
        usb_dev: UsbDevice<'static, UsbBus<USB>>,
        joy: UsbHidClass<'static, UsbBus<USB>, frunk::HList!(Joystick<'static, UsbBus<USB>>)>,
        cycles: usize,
    }

    #[init]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        rtt_init_print!();
        rprintln!("hello world");

        // Get the dp peripherals
        let dp: pac::Peripherals = ctx.device;

        // Initialize the clocks
        let rcc = dp.RCC.constrain();
        let clocks = rcc.cfgr.sysclk(84.MHz()).freeze();

        // Initialize GPIOC (for the onboard LED on PC13)
        let gpioc = dp.GPIOC.split();
        let mut led = gpioc.pc13.into_push_pull_output();
        led.set_high();

        // Configure TIM2 as a periodic timer
        let mut timer = Timer::new(dp.TIM2, &clocks).counter_hz();
        timer.start(1_000.Hz()).unwrap();
        timer.listen(Event::Update);

        let gpioa = dp.GPIOA.split();

        // USB
        let usb = USB::new(
            (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
            (gpioa.pa11, gpioa.pa12),
            &clocks,
        );

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

        // TODO: this may help with some debug setups
        //usb_dev.force_reset();

        (
            Shared {},
            Local {
                usb_dev,
                led,
                timer,
                joy,
                cycles: 0,
            },
            init::Monotonics(),
        )
    }

    #[idle(local = [])]
    fn idle(_: idle::Context) -> ! {
        loop {}
    }

    #[task(binds = TIM2, local = [led, timer, usb_dev, cycles, joy])]
    fn blink(cx: blink::Context) {
        let led = cx.local.led;
        let timer = cx.local.timer;

        // Toggle the LED every 1000 cycles (~1 per sec)
        *cx.local.cycles += 1;
        if *cx.local.cycles % 1000 == 0 {
            led.toggle();
        }

        // Poll every 1ms
        match cx.local.joy.device().write_report(&crate::get_report()) {
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

fn get_report() -> JoystickReport {
    // Read out 8 buttons first
    let buttons = 0b0101_0101;

    // Always return center
    let (x, y) = (0, 0);

    JoystickReport { buttons, x, y }
}
