use abi::{Codec, Command};
use log::{debug, info, trace};
use serial2::SerialPort;
use std::{env, io, sync, time::Duration};

type Response = abi::Response<4>;

/// File path to serial terminal, e.g., "/dev/ttyUSB0". Can be specified using the `COM_PATH`
/// environment variable.
static COM_PATH: sync::LazyLock<String> = sync::LazyLock::new(|| {
    env::var("COM_PATH").ok().unwrap_or_else(|| {
        let default_path = if cfg!(target_os = "linux") {
            Some("/dev/ttyUSB0")
        }
        // On Windows, use something like "COM1". For COM ports above COM9, you need to use
        // the win32 device namespace, for example "\\.\COM10" (or "\\\\.\\COM10" with
        // string escaping). For more details, see:
        // https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file?redirectedfrom=MSDN#win32-device-namespaces
        else if cfg!(target_os = "windows") {
            Some("COM3")
        }
        // I have no idea what they use on Mac or any other platform
        else {
            None
        };
        if let Some(p) = default_path {
            info!("COM_PATH not provided via env, using platform default: {p}");
            p.to_string()
        } else {
            panic!("Please specify COM_PATH via env")
        }
    })
});

// One second timeout for getting a response from the device
const DEFAULT_TIMEOUT: Duration = Duration::from_millis(1000);

/// Opens a serial port
pub fn open() -> io::Result<SerialPort> {
    let mut port = SerialPort::open(&*COM_PATH, 115200).unwrap();

    // Needed for windows, but should not hurt on Linux
    // TODO: re-enable for actual hardware (feature flag / runtime config for emulator)
    /*port.set_dtr(true).unwrap();
    port.set_rts(true).unwrap();*/
    port.set_write_timeout(DEFAULT_TIMEOUT).unwrap();
    port.set_read_timeout(DEFAULT_TIMEOUT).unwrap();

    Ok(port)
}

fn main() {
    let mut port = open().unwrap();

    let response = exchange(&abi::Command::GetValues, &mut port).unwrap();
    println!("Received: {response:?}");
}

/// Send a command over serial and wait for response from the device. Blocks until response is
/// received or timeout.
///
/// # Arguments
///
/// * `cmd` - command to send to device
/// * `port` - serial port with a connected device (ESP32-C3 serial server)
/// * `timeout` - optional timeout, default if `None`
pub fn exchange(cmd: &Command, port: &mut SerialPort) -> Result<Response, ResponseError> {
    debug!("sending: {:?}", cmd);
    send(cmd, port);
    wait_for_response(port)
}

/// Send a command over serial
pub fn send(cmd: &Command, port: &mut SerialPort) {
    trace!("Serializing Command {cmd:?}");
    // Construct the command packet
    let mut cmd_buf = [0u8; Command::MAX_SERIALIZED_LEN];
    let cmd_packet = cmd
        .serialize(&mut cmd_buf)
        // Hard error on failing to serialize a command on the host
        .expect("Command ABI should not have changed");
    trace!("Serialized packet: {cmd_packet:?}");

    // Send the packet over serial
    port.write(cmd_packet)
        // Hard error on failing to write over serial
        .unwrap();
}

#[derive(Debug)]
pub enum ResponseError {
    Timeout,
}

/// Wait for a [abi::Response] from the device. Blocks until response is received or timeout.
pub fn wait_for_response(port: &mut SerialPort) -> Result<Response, ResponseError> {
    let mut resp_buf = [0u8; Response::MAX_SERIALIZED_LEN];

    // Read byte-by-byte until we receive a packet frame
    for idx in 0..Response::MAX_SERIALIZED_LEN {
        let byte = &mut resp_buf[idx..idx + 1];
        port.read_exact(byte).map_err(|e| match e.kind() {
            io::ErrorKind::TimedOut => ResponseError::Timeout,
            // Hard error on any other type of error
            _ => panic!("failed to read from serial: {e}"),
        })?;
        if byte[0] == abi::corncobs::ZERO {
            break;
        }
    }

    trace!("Response buffer: {resp_buf:?}");
    let response = Response::deserialize_in_place(&mut resp_buf)
        // Hard error on failing to deserialize a response
        .expect("Response ABI should not have changed");
    trace!("Deserialized Response: {response:?}");
    Ok(response)
}
