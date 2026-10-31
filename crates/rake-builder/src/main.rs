#[cfg(target_os = "macos")]
include!("macos.rs");

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The Rake builder is macOS-only. Use the rake CLI on Linux.");
    std::process::exit(1);
}
