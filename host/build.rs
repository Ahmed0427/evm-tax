//! Compiles both guest programs to RISC-V before the host is built.
fn main() {
    sp1_build::build_program("../guest-native");
    sp1_build::build_program("../guest-evm");
}
