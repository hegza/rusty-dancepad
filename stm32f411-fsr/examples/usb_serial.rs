#![no_main]
#![no_std]

// Print panic message to probe console
use panic_probe as _;

static mut EP_MEMORY: [u32; 1024] = [0; 1024];

#[rtic::app(device = stm32f4xx_hal::pac, peripherals = true)]
mod app {
    use core::ptr;

    use rtt_target::{rprintln, rtt_init_print};
    use stm32f4xx_hal::otg_fs::UsbBus;
    use stm32f4xx_hal::prelude::*;
    use stm32f4xx_hal::{
        gpio::{Output, PushPull, PC13},
        otg_fs::USB,
        pac,
        timer::{CounterHz, Event, Timer},
    };
    use usb_device::bus::UsbBusAllocator;
    use usb_device::device::{StringDescriptors, UsbDevice, UsbDeviceBuilder, UsbVidPid};
    use usbd_serial::SerialPort;

    static mut USB_BUS_ALLOCATOR: Option<UsbBusAllocator<UsbBus<USB>>> = None;

    #[shared]
    struct Shared {}

    // Local resources go here
    #[local]
    struct Local {
        led: PC13<Output<PushPull>>,
        timer: CounterHz<pac::TIM2>,
        serial: SerialPort<'static, UsbBus<USB>>,
        usb_dev: UsbDevice<'static, stm32f4xx_hal::otg_fs::UsbBus<USB>>,
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
        timer.start(1u32.Hz()).unwrap();
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

        let serial = usbd_serial::SerialPort::new(unsafe { USB_BUS_ALLOCATOR.as_ref().unwrap() });

        let usb_dev = UsbDeviceBuilder::new(
            unsafe { USB_BUS_ALLOCATOR.as_ref().unwrap() },
            UsbVidPid(0x16c0, 0x27dd),
        )
        .device_class(usbd_serial::USB_CLASS_CDC)
        .strings(&[StringDescriptors::default()
            .manufacturer("Fake Company")
            .product("Product")
            .serial_number("TEST")])
        .unwrap()
        .build();

        // TODO: helps with debug
        // force_reenumeration

        (
            Shared {},
            Local {
                usb_dev,
                serial,
                led,
                timer,
            },
            init::Monotonics(),
        )
    }

    // Optional idle, can be removed if not needed.
    #[idle(local = [usb_dev, serial])]
    fn idle(cx: idle::Context) -> ! {
        loop {
            if !cx.local.usb_dev.poll(&mut [cx.local.serial]) {
                continue;
            }

            let mut buf = [0u8; 64];

            match cx.local.serial.read(&mut buf) {
                Ok(count) if count > 0 => {
                    // Echo back in upper case
                    for c in buf[0..count].iter_mut() {
                        if 0x61 <= *c && *c <= 0x7a {
                            *c &= !0x20;
                        }
                    }

                    let mut write_offset = 0;
                    while write_offset < count {
                        match cx.local.serial.write(&buf[write_offset..count]) {
                            Ok(len) if len > 0 => {
                                write_offset += len;
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    #[task(binds = TIM2, local = [led, timer])]
    fn blink(ctx: blink::Context) {
        rprintln!("tick");
        let led = ctx.local.led;
        let timer = ctx.local.timer;

        // Toggle the LED
        led.toggle();

        // Clear the timer interrupt flag
        timer.clear_all_flags();
    }
}
