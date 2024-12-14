use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{self, Arc, RwLock};
use std::time::Duration;
use std::{env, process, thread};

use abi::{AdcValues, Codec, Command};
use log::trace;

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
fn start_cli() -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    {
        let running = Arc::clone(&running);
        tokio::spawn(async move {
            while running.load(Ordering::Acquire) {
                // Pause to get one character
                let term = console::Term::stdout();
                let c = term.read_char().unwrap_or_else(|e| match e.kind() {
                    io::ErrorKind::Interrupted => {
                        println!("read interrupt: another thread exited");
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
    running
}

pub fn create_port(running: Arc<AtomicBool>) -> Arc<RwLock<tokio_serial::SerialStream>> {
    let (stream, _pty) = vsp_router::create_virtual_serial_port(&*COM_PATH).unwrap();
    let stream = Arc::new(RwLock::new(stream));
    {
        let stream = Arc::clone(&stream);
        ctrlc::set_handler(move || {
            running.store(false, Ordering::Release);
            println!("received Ctrl+C, dropping virtual port at {}", *COM_PATH);

            drop(stream.try_write().unwrap());

            process::exit(1);
        })
        .expect("Error setting Ctrl-C handler");
    }
    stream
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let running = start_cli();

    let stream = create_port(Arc::clone(&running));

    let mut cmd_buf = [0u8; MAX_LEN];
    let buf = &mut [0u8; 1];
    let mut idx = 0;
    // Wait for command
    const MAX_LEN: usize = Response::MAX_SERIALIZED_LEN;
    while running.load(Ordering::Acquire) {
        // Read byte-by-byte until we receive a packet frame
        if let Ok(_n) = stream.try_write().unwrap().read(buf) {
            trace!("Read: {buf:?}");

            let byte = buf[0];
            if byte != abi::corncobs::ZERO {
                cmd_buf[idx] = byte;
                idx += 1;
                continue;
            }

            // Frame byte -> construct message
            trace!("Received packet: {:?}", &cmd_buf[..idx]);
            let cmd = Command::deserialize_in_place(&mut cmd_buf)
                // Hard error on failing to deserialize a cmd
                .expect("unable to deserialize a known command");
            trace!("Deserialized Response: {cmd:?}");
            println!("Response: {cmd:?}");

            match cmd {
                Command::GetValues => {
                    //let now = Instant::now().duration_since(UNIX_EPOCH);
                    let values = AdcValues([4, 3, 2, 1]);

                    let resp_buf = &mut [0u8; 255];
                    let packet = Response::Values(values).serialize(resp_buf).unwrap();
                    stream.try_write().unwrap().write_all(packet).unwrap();
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
}
