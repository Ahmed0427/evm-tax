#![no_main]
sp1_zkvm::entrypoint!(main);

pub fn main() {
    let n: u32 = sp1_zkvm::io::read();
    let use_u64: u32 = sp1_zkvm::io::read();

    let result = if use_u64 == 1 {
        fib::U256::from(fib::fib_native_u64(n))
    } else {
        fib::fib_native(n)
    };

    // committing the result does two jobs: it lets the host check both arms
    // computed the same number, and it stops llvm deleting the loop as dead.
    sp1_zkvm::io::commit_slice(&result.to_be_bytes::<32>());
}
