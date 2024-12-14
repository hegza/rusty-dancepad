use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use abi::{AdcValues, Codec, Command};
use log::trace;

type Response = abi::Response<4>;

/// Spawns a task that listens on terminal for a character, then sets the returned boolean as false.
fn start_cli() -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    {
        let running = Arc::clone(&running);
        tokio::spawn(async move {
            // Pause to get one character
            let term = console::Term::stdout();
            let c = term.read_char().unwrap();
            match c {
                'q' | _ => running.store(false, Ordering::Relaxed),
            }
        });
    }
    running
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let (mut stream, _pty) =
        vsp_router::create_virtual_serial_port(option_env!("COM_PATH").unwrap_or("/tmp/ttyUSB0"))
            .unwrap();

    let running = start_cli();

    let mut cmd_buf = [0u8; MAX_LEN];
    let buf = &mut [0u8; 1];
    let mut idx = 0;
    // Wait for command
    const MAX_LEN: usize = Response::MAX_SERIALIZED_LEN;
    while running.load(Ordering::Relaxed) {
        // Read byte-by-byte until we receive a packet frame
        if let Ok(_n) = stream.read(buf) {
            println!("read: {buf:?}");

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
                    stream.write(packet).unwrap();
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
