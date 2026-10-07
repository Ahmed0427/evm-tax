//! Links contract.o, the contract compiled by `solang compile --target riscv`.
use std::path::Path;

fn main() {
    let object = Path::new(env!("CARGO_MANIFEST_DIR")).join("contract.o");
    assert!(object.exists(), "copy a Solang object file to {}", object.display());
    println!("cargo:rustc-link-arg={}", object.display());
    println!("cargo:rerun-if-changed={}", object.display());
}
