//! Make Cargo rebuild the kernel image when the bare-metal link inputs change.
//!
//! Cargo does not track the linker script or custom target JSON as build inputs
//! by default, so editing them would otherwise silently reuse a stale binary.

fn main() {
    println!("cargo:rerun-if-changed=../../linker/x86_64.ld");
    println!("cargo:rerun-if-changed=../../targets/x86_64-unknown-zeroxos.json");
}
