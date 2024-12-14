mod serial;

use env_logger::Env;

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let mut port = serial::open().unwrap();

    let response = serial::exchange(&abi::Command::GetValues, &mut port).unwrap();
}
