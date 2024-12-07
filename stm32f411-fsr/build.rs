use std::{env, fs, io, path::PathBuf};

fn add_linker_script() -> io::Result<()> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Put the linker script somewhere the linker can find it
    fs::copy("memory.x", out_dir.join("memory.x"))?;
    println!("cargo:rustc-link-search={}", out_dir.display());
    println!("cargo:rerun-if-changed=link.x");

    Ok(())
}

fn main() {
    add_linker_script().unwrap();
}
