#![feature(panic_update_hook)]

use std::io::{self, Read, Write};
use std::sync::{
    self, Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use std::{env, fs, process, thread};

use abi::{AdcValues, Codec, Command};
use env_logger::Env;
use log::{debug, info, warn};

type Response = abi::Response<4>;

const DEFAULT_COM_PATH: &str = "/tmp/ttyUSB0";

/// File path to serial terminal, e.g., "/dev/ttyUSB0". Can be specified using the `COM_PATH`
/// environment variable.
static COM_PATH: sync::LazyLock<String> = sync::LazyLock::new(|| {
    env::var("COM_PATH")
        .ok()
        .unwrap_or(DEFAULT_COM_PATH.to_string())
});

/// Spawns a task that listens on terminal for a character, then sets the returned boolean as false.
fn spawn_cli(running: Arc<AtomicBool>) {
    tokio::spawn(async move {
        while running.load(Ordering::Acquire) {
            // Pause to get one character
            let term = console::Term::stdout();
            let c = term.read_char().unwrap_or_else(|e| match e.kind() {
                io::ErrorKind::Interrupted => {
                    println!("read interrupted: another thread exited");
                    process::exit(0);
                }
                _ => panic!("{}", e.to_string()),
            });
            match c {
                'q' | _ => running.store(false, Ordering::Relaxed),
            }
        }
    });
}

/// Frees the serial port
fn free_port() {
    warn!("manually dropping virtual port at {}", *COM_PATH);
    if fs::exists(&*COM_PATH).unwrap() {
        fs::remove_file(&*COM_PATH).unwrap();
    }
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let running = Arc::new(AtomicBool::new(true));
    spawn_cli(Arc::clone(&running));
    info!("Opened port at {}", &*COM_PATH);
    let (mut serial, _pty) = vsp_router::create_virtual_serial_port(&*COM_PATH).unwrap();
    ctrlc::set_handler(move || {
        debug!("enter: ctrl + C handler");
        free_port();
    })
    .unwrap();
    std::panic::update_hook(move |prev, info| {
        debug!("enter: panic hook");
        free_port();
        prev(info);
    });

    let mut cmd_buf = [0u8; MAX_LEN];
    let buf = &mut [0u8; 1];
    let mut idx = 0;
    // Wait for command
    const MAX_LEN: usize = Response::MAX_SERIALIZED_LEN;

    while running.load(Ordering::Acquire) {
        // Read byte-by-byte until we receive a packet frame
        if let Ok(_n) = serial.read(buf) {
            debug!("Read: {buf:?}");

            let byte = buf[0];
            if byte != abi::corncobs::ZERO {
                cmd_buf[idx] = byte;
                idx += 1;
                continue;
            }
            // Frame byte -> construct message
            debug!("Received packet: {:?}", &cmd_buf[..idx]);

            let cmd = Command::deserialize_in_place(&mut cmd_buf)
                // Hard error on failing to deserialize a cmd
                .expect("unable to deserialize a known command");
            println!("Received command: {cmd:?}");

            match cmd {
                Command::GetValues => {
                    //let now = Instant::now().duration_since(UNIX_EPOCH);
                    let values = AdcValues([4, 3, 2, 1]);

                    let resp_buf = &mut [0u8; 255];
                    let packet = Response::Values(values).serialize(resp_buf).unwrap();
                    serial.write_all(packet).unwrap();
                }
                Command::GetThresh => {
                    todo!()
                }
            }

            idx = 0;
            cmd_buf = [0u8; MAX_LEN];
        }

        thread::sleep(Duration::from_millis(100));
    }
    info!("Exit");
}
