//! runs both arms in the same zkvm and prints the cycle counts side by side.

use fib::U256;
use sp1_sdk::blocking::{CpuProver, Prover, ProverClient};
use sp1_sdk::{include_elf, Elf, SP1Stdin};

const NATIVE_ELF: Elf = include_elf!("guest-native");
const EVM_ELF: Elf = include_elf!("guest-evm");

/// run one guest and report what it computed and how many cycles it took.
fn execute(prover: &CpuProver, elf: Elf, n: u32, use_u64: bool) -> (U256, u64) {
    let mut stdin = SP1Stdin::new();
    stdin.write(&n);
    stdin.write(&(use_u64 as u32));

    let (public_values, report) = prover.execute(elf, stdin).run().expect("guest panicked");

    let mut word = [0u8; 32];
    word.copy_from_slice(public_values.as_slice());
    (U256::from_be_bytes(word), report.total_instruction_count())
}

/// cycles for the loop only: the run with n, minus the run with n = 0 (setup).
fn loop_cycles(prover: &CpuProver, elf: Elf, n: u32, use_u64: bool) -> (U256, u64) {
    let (_, baseline) = execute(prover, elf.clone(), 0, use_u64);
    let (output, total) = execute(prover, elf, n, use_u64);
    (output, total - baseline)
}

fn main() {
    let prover = ProverClient::builder().cpu().build();

    // setup cost of each arm (n = 0); this is what loop_cycles subtracts.
    let (_, native_base) = execute(&prover, NATIVE_ELF, 0, false);
    let (_, evm_base) = execute(&prover, EVM_ELF, 0, false);

    println!("\nfib(n) cycles, loop only (setup removed)");
    println!("setup: native {native_base}, evm {evm_base}\n");
    println!("{:>6} {:>10} {:>10} {:>7}", "n", "native", "evm", "ratio");

    for n in [10u32, 100, 1000, 10000] {
        let (native_out, native) = loop_cycles(&prover, NATIVE_ELF, n, false);
        let (evm_out, evm) = loop_cycles(&prover, EVM_ELF, n, false);

        assert_eq!(native_out, evm_out, "arms disagree at n={n}");

        let ratio = evm as f64 / native as f64;
        println!("{n:>6} {native:>10} {evm:>10} {ratio:>6.1}x");
    }

    let n = 69;
    let (out64, w64) = loop_cycles(&prover, NATIVE_ELF, n, true);
    let (out256, w256) = loop_cycles(&prover, NATIVE_ELF, n, false);
    let (out_evm, evm) = loop_cycles(&prover, EVM_ELF, n, false);
    assert_eq!(out64, out256);
    assert_eq!(out64, out_evm);

    let width_cost = w256 as f64 / w64 as f64;
    let interp_cost = evm as f64 / w256 as f64;

    println!("\nat n={n}:");
    println!("  native u64:      {w64:>7} cycles");
    println!("  native u256:     {w256:>7} cycles  ({width_cost:.1}x slower)");
    println!("  EVM interpreter: {evm:>7} cycles  ({interp_cost:.1}x slower)\n");
}
