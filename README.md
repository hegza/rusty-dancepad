# Rusty Dancepad

RTIC-based USB HID Joystick to read force-sensitive resistors for dance gaming purposes.

## Requirements

- A Rust toolchain: [rustup](https://rustup.rs/)
- Compiler backend for the target platform
  - `rustup target add thumbv7em-none-eabihf`
- [probe-rs](https://probe.rs/)
  - `cargo install probe-rs --features cli`

## Build

```sh
cargo build --release
```

## Flash & run/debug

You can flash the firmware using one of these tools:

- `cargo flash --release` — just flash
- `cargo run --release` — flash and run using `probe-rs run` runner or `probe-run` runner
  (deprecated) which you can set in `.cargo/config.toml`
- `cargo embed --release` — multifunctional tool for flash and debug

You also can debug your firmware on device from VS Code with
[probe-rs](https://probe.rs/docs/tools/vscode/) extension or with `probe-rs gdb` command. For this,
you will need the SVD specification for your chip. You can load patched SVD files
[here](https://stm32-rs.github.io/stm32-rs/).

## Share USB device from Windows

Open an elevated PowerShell

```ps
usbipd --help
usbipd list
usbipd bind --busid=<BUSID>
```

Attach

```ps
usbipd attach --wsl --busid=<BUSID>
```
