//! A tiny EVM interpreter, plus one program written in EVM bytecode.
//!
//! This is the "middleman" in the experiment. The zkVM does not understand EVM
//! bytecode, so to run an EVM program inside a zkVM you must first compile an
//! *interpreter* to RISC-V and then have that interpreter walk the bytecode.
//! Every EVM opcode therefore costs many RISC-V instructions: fetch the byte,
//! match on it, pop 256-bit words off a stack, push the result back.
//!
//! The interpreter supports exactly the 10 opcodes the fib program uses. It has
//! no gas metering, no storage, no calls, no exception handling. That makes it
//! *cheaper* than a real EVM (revm), so any overhead it shows is a lower bound
//! on what a real EVM-in-zkVM costs.

pub use ruint::aliases::U256;

const ADD: u8 = 0x01;
const EQ: u8 = 0x14;
const CALLDATALOAD: u8 = 0x35;
const MLOAD: u8 = 0x51;
const MSTORE: u8 = 0x52;
const JUMP: u8 = 0x56;
const JUMPI: u8 = 0x57;
const JUMPDEST: u8 = 0x5b;
const PUSH1: u8 = 0x60;
const RETURN: u8 = 0xf3;

struct Evm<'a> {
    code: &'a [u8],
    calldata: &'a [u8],
    stack: Vec<U256>,
    memory: Vec<u8>,
    pc: usize,
}

impl<'a> Evm<'a> {
    fn pop(&mut self) -> U256 {
        self.stack.pop().expect("stack underflow")
    }

    fn grow(&mut self, offset: usize) {
        let needed = offset + 32;
        if self.memory.len() < needed {
            self.memory.resize(needed, 0);
        }
    }

    fn mload(&mut self, offset: usize) -> U256 {
        self.grow(offset);
        let mut word = [0u8; 32];
        word.copy_from_slice(&self.memory[offset..offset + 32]);
        U256::from_be_bytes(word)
    }

    fn mstore(&mut self, offset: usize, value: U256) {
        self.grow(offset);
        self.memory[offset..offset + 32].copy_from_slice(&value.to_be_bytes::<32>());
    }
}

/// Execute `code` with `calldata` and return whatever RETURN hands back.
/// This loop *is* the interpretation overhead the proposal wants to delete.
pub fn run(code: &[u8], calldata: &[u8]) -> Vec<u8> {
    let mut evm = Evm {
        code,
        calldata,
        stack: Vec::with_capacity(16),
        memory: Vec::new(),
        pc: 0,
    };

    loop {
        let op = evm.code[evm.pc];
        evm.pc += 1;

        match op {
            PUSH1 => {
                let byte = evm.code[evm.pc];
                evm.pc += 1;
                evm.stack.push(U256::from(byte));
            }

            ADD => {
                let (a, b) = (evm.pop(), evm.pop());
                evm.stack.push(a.wrapping_add(b));
            }

            EQ => {
                let (a, b) = (evm.pop(), evm.pop());
                evm.stack.push(U256::from(u8::from(a == b)));
            }

            CALLDATALOAD => {
                let offset: usize = evm.pop().to();
                let mut word = [0u8; 32];
                for i in 0..32 {
                    if let Some(b) = evm.calldata.get(offset + i) {
                        word[i] = *b;
                    }
                }
                evm.stack.push(U256::from_be_bytes(word));
            }

            MLOAD => {
                let offset: usize = evm.pop().to();
                let value = evm.mload(offset);
                evm.stack.push(value);
            }

            MSTORE => {
                let offset: usize = evm.pop().to();
                let value = evm.pop();
                evm.mstore(offset, value);
            }

            JUMP => {
                let dest: usize = evm.pop().to();
                debug_assert_eq!(evm.code[dest], JUMPDEST);
                evm.pc = dest;
            }

            JUMPI => {
                let dest: usize = evm.pop().to();
                let cond = evm.pop();
                if cond != U256::ZERO {
                    debug_assert_eq!(evm.code[dest], JUMPDEST);
                    evm.pc = dest;
                }
            }

            RETURN => {
                let offset: usize = evm.pop().to();
                let len: usize = evm.pop().to();
                evm.grow(offset);
                return evm.memory[offset..offset + len].to_vec();
            }

            JUMPDEST => {}

            other => panic!("unsupported opcode 0x{other:02x} at PC {}", evm.pc - 1),
        }
    }
}

// --------------------------------------------
// The program, in EVM bytecode.
//
//   n = calldata[0..32]
//   a, b = 0, 1
//   repeat n times: a, b = b, a + b
//   return a
//
// Memory layout (one 32-byte word each):
//   0x00 = i   0x20 = a   0x40 = b   0x60 = n
// --------------------------------------------

const LOOP_TOP: u8 = 0x15; // 21
const LOOP_END: u8 = 0x3c; // 60

#[rustfmt::skip]
pub const FIB_BYTECODE: &[u8] = &[
    //  0  n = calldataload(0);  mem[0x60] = n
    PUSH1, 0x00, CALLDATALOAD, PUSH1, 0x60, MSTORE,
    //  6  i = 0
    PUSH1, 0x00, PUSH1, 0x00, MSTORE,
    // 11  a = 0
    PUSH1, 0x00, PUSH1, 0x20, MSTORE,
    // 16  b = 1
    PUSH1, 0x01, PUSH1, 0x40, MSTORE,

    // 21  loop top
    JUMPDEST,
    // 22  if i == n goto end
    PUSH1, 0x60, MLOAD, PUSH1, 0x00, MLOAD, EQ, PUSH1, LOOP_END, JUMPI,
    // 32  next = a + b         
    PUSH1, 0x20, MLOAD, PUSH1, 0x40, MLOAD, ADD,
    // 39  a = b
    PUSH1, 0x40, MLOAD, PUSH1, 0x20, MSTORE,
    // 45  b = next
    PUSH1, 0x40, MSTORE,
    // 48  i = i + 1
    PUSH1, 0x00, MLOAD, PUSH1, 0x01, ADD, PUSH1, 0x00, MSTORE,
    // 57  goto loop top
    PUSH1, LOOP_TOP, JUMP,

    // 60  end: return a
    JUMPDEST,
    PUSH1, 0x20, MLOAD, PUSH1, 0x00, MSTORE,
    PUSH1, 0x20, PUSH1, 0x00, RETURN,
];

/// Run `FIB_BYTECODE` for a given n and decode the 32-byte result.
pub fn fib_evm(n: u32) -> U256 {
    let calldata = U256::from(n).to_be_bytes::<32>();
    let out = run(FIB_BYTECODE, &calldata);
    let mut word = [0u8; 32];
    word.copy_from_slice(&out);
    U256::from_be_bytes(word)
}

/// The same function written directly in Rust. This is the "native" arm.
/// Identical semantics: 256-bit words, wrapping at 2^256, same number of
/// additions. The only difference is how it reaches the zkVM.
pub fn fib_native(n: u32) -> U256 {
    let (mut a, mut b) = (U256::ZERO, U256::from(1u8));
    for _ in 0..n {
        let next = a.wrapping_add(b);
        a = b;
        b = next;
    }
    a
}

/// Same again at 64 bits, for the data-width comparison.
/// Only agrees with the 256-bit versions while fib(n) < 2^64, i.e. n <= 93.
pub fn fib_native_u64(n: u32) -> u64 {
    let (mut a, mut b) = (0u64, 1u64);
    for _ in 0..n {
        let next = a.wrapping_add(b);
        a = b;
        b = next;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jump_targets() {
        assert_eq!(FIB_BYTECODE[LOOP_TOP as usize], JUMPDEST);
        assert_eq!(FIB_BYTECODE[LOOP_END as usize], JUMPDEST);
    }

    #[test]
    fn test_agreement() {
        for n in [0u32, 1, 2, 10, 90, 100, 1000] {
            assert_eq!(fib_evm(n), fib_native(n), "mismatch at n={n}");
        }
        assert_eq!(fib_native(10), U256::from(55u8));
        assert_eq!(fib_native(90), U256::from(2880067194370816120u64));
    }

    #[test]
    fn test_width_agreement() {
        for n in [0u32, 1, 10, 90, 93] {
            assert_eq!(U256::from(fib_native_u64(n)), fib_native(n), "n={n}");
        }
        assert_ne!(U256::from(fib_native_u64(200)), fib_native(200));
    }
}
