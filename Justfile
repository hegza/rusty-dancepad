check *args:
    cargo check -p rusty-dancepad {{args}}

build *args:
    cargo build -p rusty-dancepad {{args}}

run *args:
    cd stm32f411-fsr && cargo embed -p rusty-dancepad {{args}}

emulate *args:
    cargo run -p emulator {{args}}

cli *args:
    COM_PATH=/tmp/ttyUSB0 cargo run -p dancepad-cli {{args}}

