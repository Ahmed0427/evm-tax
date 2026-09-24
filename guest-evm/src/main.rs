#![no_main]
sp1_zkvm::entrypoint!(main);

pub fn main() {
    let n: u32 = sp1_zkvm::io::read();

    // read and ignore, so both guests consume identical input.
    let _use_u64: u32 = sp1_zkvm::io::read();

    let calldata = fib::U256::from(n).to_be_bytes::<32>();
    let out = fib::run(fib::FIB_BYTECODE, &calldata);

    // committing the result does two jobs: it lets the host check both arms
    // computed the same number, and it stops llvm deleting the loop as dead.
    sp1_zkvm::io::commit_slice(&out);
}
