fn main() {
    let out = std::env::var("OUT_DIR").unwrap();
    std::fs::write(format!("{out}/level.rs"), "pub const LEVEL: u32 = 7;\n").unwrap();
    println!("cargo::rerun-if-changed=build.rs");
}
