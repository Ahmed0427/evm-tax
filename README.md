# Does removing the EVM interpreter actually make proving cheaper?

Vitalik's [April 2025 proposal](https://ethereum-magicians.org/t/long-term-l1-execution-layer-proposal-replace-the-evm-with-risc-v/23617)
argues that ZK provers already work by compiling an EVM implementation down to
RISC-V, so letting contract developers write RISC-V directly removes a whole
interpretation layer. He suggests gains "over 100x in limited cases".

This repo measures that one claim, as simply as possible.

## The setup

One function — fibonacci — expressed two ways, run in the same zkVM (SP1) on
the same machine:

| arm | what the zkVM actually runs |
|-----|-----------------------------|
| **native** | fib compiled from Rust straight to RISC-V |
| **EVM** | an EVM interpreter compiled to RISC-V, walking fib as EVM bytecode |

Both arms compute fib(n) as 256-bit arithmetic wrapping at 2^256, so they
produce an identical number. The host checks that before reporting anything.
Whatever cycle difference remains is the cost of the interpreter.

## The metric

**Cycles** — RISC-V instructions retired. That is what a prover is paid to
prove, it is exactly reproducible on any machine, and it is the unit the
proposal's own figures use. You do not need to generate a proof to measure it;
the zkVM's emulator counts instructions directly, which is why this runs in
seconds instead of hours.

## Running it

```bash
curl -L https://sp1up.succinct.xyz | bash 
sp1up # one-time

./run
```

## The result on this machine (SP1 v6.8.0)

```
       n    native RISC-V     EVM bytecode     ratio
-----------------------------------------------------
      10              251            24160     96.3x
     100             2411           241600    100.2x
    1000            24011          2416000    100.6x
   10000           240011         24160000    100.7x
```

**~100x**, and flat across three orders of magnitude of n — which is the sign
that the measurement is clean. A ratio that drifted with n would mean fixed
setup cost was leaking into the numbers.

Per fibonacci iteration that is 24 native cycles against 2,416 EVM ones. The
loop body is 27 EVM opcodes, so the interpreter spends roughly **90 RISC-V
cycles per EVM opcode**: fetch the byte, dispatch on it, pop 256-bit words off
a `Vec`, do the 32-byte memory copies the EVM's memory model requires, push the
result back.

But the headline number splits in two:

```
  native, 64-bit words        452 cycles
  native, 256-bit words      2171 cycles  (4.8x of 64-bit)
  EVM bytecode             217440 cycles  (100.2x of 256-bit native)
```

Of the total ~480x gap between "EVM bytecode" and "Rust using the word size it
actually needs", **4.8x is the cost of 256-bit words** and ~100x is the cost of
interpretation. Only the second is what the proposal deletes; the first is a
separate argument about the EVM's word size, which a RISC-V L1 would also win
but for a different reason.

## The four files that matter

```
fib/src/lib.rs         the EVM interpreter (~120 lines) + fib in EVM bytecode
                       + fib in plain Rust. All three in one file so you can
                       see they compute the same thing.
guest-native/src/main.rs   arm A: 10 lines. Call the Rust fib.
guest-evm/src/main.rs      arm B: 10 lines. Call the interpreter.
host/src/main.rs           run both, check outputs match, print cycles.
```

That is the whole experiment. Everything else is Cargo manifests.

### How the EVM bytecode works

`FIB_BYTECODE` in `fib/src/lib.rs` is a handwritten EVM program:

```
n = calldata[0..32]
mem[0x00] = i = 0,  mem[0x20] = a = 0,  mem[0x40] = b = 1
loop: if i == n goto end
      a, b = b, a + b
      i = i + 1
      goto loop
end:  return a
```

Jump targets are byte offsets, written as constants and checked by a unit test
(`test_jump_targets`) rather than trusted. A wrong offset would silently
shorten the loop and hand the EVM arm a fake win; `test_agreement` catches that
by comparing against the Rust version for several values of n.

## What it reports

Two numbers, not one.

**The ratio per n.** Reported at n = 10, 100, 1000, 10000 rather than as a
single headline figure. If the ratio drifts a lot with n, fixed setup cost is
leaking into the measurement and the number is not trustworthy.

Fixed cost is removed by running each arm at n = 0 and subtracting: at n = 0 the
loop body never executes, so that run *is* the zkVM boot, input reading, commit
and interpreter prologue. The host prints both baselines so you can check them.

**Where the gap comes from.** The native arm's advantage is really two separate
effects, and lumping them together overstates the case:

1. *It does not need 256-bit words.* The EVM has no choice; every stack slot is
   256 bits. Native Rust can use a u64 where a u64 is enough.
2. *It does not need an interpreter.* No opcode fetch, no dispatch, no stack.

Only (2) is what the proposal deletes. The host runs fib at n=90 — small enough
that the 64-bit and 256-bit results are identical — three ways, so the two
effects are separated instead of conflated.

## Honest limits of this measurement

- **The interpreter is a toy.** 10 opcodes, no gas metering, no storage, no
  call frames, no exception handling. A real EVM (revm) does considerably more
  work per opcode, so the overhead measured here is a **lower bound**. Swapping
  in revm is the obvious next step and would only move the number up.
- **One workload.** Fibonacci is the proposal's own flagship example, and it is
  also close to the best case for the native arm: pure small-integer compute.
  Workloads that are actually 256-bit-native (modular exponentiation, elliptic
  curve arithmetic) should narrow or reverse the gap, and workloads dominated
  by hashing would sit behind precompiles in both worlds — which is
  [levs57's objection](https://ethereum-magicians.org/t/long-term-l1-execution-layer-proposal-replace-the-evm-with-risc-v/23617)
  in the forum thread. Do not read one number here as a verdict on the proposal.
- **Cycles, not proving time.** They track each other closely in SP1, but they
  are not the same thing, and precompiles break the relationship deliberately.
- **SP1 v6 is a 64-bit RISC-V zkVM** (`riscv64im-succinct-zkvm-elf`). On a
  32-bit zkVM the native 256-bit arithmetic would cost more and the 4.8x
  width figure above would grow.
- **No storage, no state.** Real contract execution spends a lot of its budget
  in Merkle proofs and state access, which are identical in both worlds and so
  dilute any interpreter saving.
